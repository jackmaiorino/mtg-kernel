#!/usr/bin/env python3
"""Run the fixed two-arm offline search-teacher learner.

The default mode validates inputs and writes a training plan.  ``--execute``
is an explicit opt-in for the fixed GPU run.  There is deliberately no arm,
epoch, checkpoint, or hyperparameter selection in this driver.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import platform
import random
import struct
import subprocess
import sys
import time
from typing import Any, Iterable

os.environ.setdefault("CUBLAS_WORKSPACE_CONFIG", ":4096:8")

import numpy as np
import torch


SCRIPT_DIR = Path(__file__).resolve().parent
REPO = SCRIPT_DIR.parent.parent
RECURRENT_DIR = SCRIPT_DIR.parent / "recurrent_cp7_dagger_v1"
STRUCTURED_DIR = SCRIPT_DIR.parent / "structured_adapter_screen_v1"
sys.path.insert(0, str(RECURRENT_DIR))
sys.path.append(str(STRUCTURED_DIR))

import run_screen_v1 as recurrent  # noqa: E402
import run_sparse_correction_v1 as sparse  # noqa: E402
import run_structured_outcome_policy_v1 as outcome  # noqa: E402
import run_structured_successor_distillation_v1 as distill  # noqa: E402


SCHEMA = "mtg-kernel-bounded-history-value-search-fixed-arms/v1"
COLLECTOR_SCHEMA = "mtg-kernel-depth8-bounded-value-search-teacher-corpus/v1"
LABEL_SCHEMA = "mtg-kernel-depth8-bounded-value-search-teacher/v1"
DIAGNOSTIC_PREFIX = "NATIVE_DEPTH8_BOUNDED_TEACHER_JSON "
PUBLIC_HISTORY_COMMITMENT = (
    "mtg-kernel-checkpoint-shadow-public-history-framed-sha256/v1"
)
CACHE_VERSION = "structured_adapter_screen_v1"
PAIR_COUNT = 256
HISTORY_LENGTH = 16
CARD_VOCAB = 136
HISTORY_FEATURE_DIM = 237
DIM = 128
EPOCHS = 8
BATCH_SIZE = 256
BETA = 6.0
LOG_RATIO_BUDGET = 0.49
LEARNING_RATE = 3.0e-4
WEIGHT_DECAY = 1.0e-4
GRADIENT_CAP = 5.0
SEED = 20_260_811
GPU_ORDINAL = 1
RELATIVE_NLL_MIN = 0.05
TOP1_DELTA_MIN = 0.03
MEAN_TV_MAX = 0.03
P90_TV_MAX = 0.10
OVERRIDE_RATE_MIN = 0.05

FROZEN_SETTINGS: dict[str, Any] = {
    "architecture": "width-128-two-layer-recurrent-structured-residual/v1",
    "width": DIM,
    "recurrent_layers": 2,
    "beta": BETA,
    "epochs": EPOCHS,
    "batch_size": BATCH_SIZE,
    "log_ratio_budget": LOG_RATIO_BUDGET,
    "optimizer": "AdamW",
    "learning_rate": LEARNING_RATE,
    "weight_decay": WEIGHT_DECAY,
    "gradient_norm_cap": GRADIENT_CAP,
    "seed": SEED,
    "split": "pair_index_mod4: train={1,2}, selection={3}, heldout={0}",
    "terminal_reward": "terminal win/loss/draw only; search labels are policy targets",
    "checkpoint_policy": "final_epoch_8_only",
    "arm_selection": "none",
    "selection_relative_search_target_nll_min": RELATIVE_NLL_MIN,
    "selection_search_target_top1_delta_min": TOP1_DELTA_MIN,
    "mean_tv_max": MEAN_TV_MAX,
    "p90_tv_max": P90_TV_MAX,
    "override_rate_min_per_seat": OVERRIDE_RATE_MIN,
}


def _fail(message: str) -> None:
    raise ValueError(message)


def _sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def _framed_update(digest: Any, label: str, payload: bytes) -> None:
    encoded = label.encode("utf-8")
    digest.update(len(encoded).to_bytes(4, "big"))
    digest.update(encoded)
    digest.update(len(payload).to_bytes(8, "big"))
    digest.update(payload)


def _public_history_sha256(row: dict[str, Any]) -> str:
    history = row.get("history_features")
    if not isinstance(history, torch.Tensor) or history.ndim != 2:
        _fail("cache row history_features is not a rank-2 tensor")
    if history.shape[0] > HISTORY_LENGTH or history.shape[1] != HISTORY_FEATURE_DIM:
        _fail("cache row history shape is outside the fixed contract")
    values = (
        history.detach()
        .cpu()
        .to(torch.float32)
        .contiguous()
        .numpy()
        .astype("<f4", copy=False)
        .tobytes()
    )
    digest = hashlib.sha256()
    _framed_update(digest, "schema", PUBLIC_HISTORY_COMMITMENT.encode("utf-8"))
    _framed_update(
        digest,
        "history_entry_count",
        struct.pack("<q", int(history.shape[0])),
    )
    _framed_update(digest, "history_features", values)
    return digest.hexdigest()


def _json_bytes(value: Any) -> bytes:
    return (json.dumps(value, indent=2, sort_keys=True, allow_nan=False) + "\n").encode(
        "utf-8"
    )


def _atomic_bytes(path: Path, payload: bytes) -> None:
    if path.exists():
        _fail(f"refusing to overwrite existing output: {path}")
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_name(path.name + ".tmp")
    if temporary.exists():
        _fail(f"temporary output already exists: {temporary}")
    temporary.write_bytes(payload)
    os.replace(temporary, path)


def _atomic_json(path: Path, value: Any) -> None:
    _atomic_bytes(path, _json_bytes(value))


def _git_head() -> str:
    completed = subprocess.run(
        ["git", "rev-parse", "HEAD"],
        cwd=REPO,
        check=True,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
    )
    return completed.stdout.strip()


def _candidate_seat(value: Any) -> int:
    if isinstance(value, str):
        lowered = value.lower()
        if lowered in {"p0", "seat0", "0"}:
            return 0
        if lowered in {"p1", "seat1", "1"}:
            return 1
    if isinstance(value, int) and not isinstance(value, bool) and value in (0, 1):
        return value
    _fail("candidate_seat must be P0/P1 or integer 0/1")


def _f32_from_hex(value: Any) -> float:
    if (
        not isinstance(value, str)
        or len(value) != 8
        or any(character not in "0123456789abcdef" for character in value)
    ):
        _fail("search label f32 bit string is invalid")
    return struct.unpack("<f", int(value, 16).to_bytes(4, "little"))[0]


def _f32_bits_hex(value: float) -> str:
    return f"{struct.unpack('<I', struct.pack('<f', value))[0]:08x}"


def _label_key(row: dict[str, Any]) -> tuple[int, str, int, int, int, int]:
    episode_id = row.get("episode_id")
    if not isinstance(episode_id, int) or isinstance(episode_id, bool) or episode_id < 0:
        _fail("search label episode_id is invalid")
    seat = _candidate_seat(row.get("candidate_seat"))
    if episode_id % 2 != seat:
        _fail("search label episode_id and candidate_seat disagree")
    pair_index = row.get("pair_index")
    if (
        not isinstance(pair_index, int)
        or isinstance(pair_index, bool)
        or pair_index < 0
        or episode_id != pair_index * 2 + seat
    ):
        _fail("search label pair_index, episode_id, and candidate_seat disagree")
    step = row.get("step")
    physical = row.get("physical_decision_id")
    if not isinstance(step, int) or isinstance(step, bool) or step < 0:
        _fail("search label step is invalid")
    if not isinstance(physical, int) or isinstance(physical, bool) or physical < 0:
        _fail("search label physical_decision_id is invalid")
    return (pair_index, str(episode_id), seat, step, physical, 0)


def _validate_label_row(row: dict[str, Any], expected_pairs: int = PAIR_COUNT) -> None:
    if row.get("schema") != LABEL_SCHEMA:
        _fail("search label schema mismatch")
    key = _label_key(row)
    if not 0 <= key[0] < expected_pairs:
        _fail("search label pair is outside the declared panel")
    legal_count = row.get("legal_action_count")
    if not isinstance(legal_count, int) or isinstance(legal_count, bool) or not 2 <= legal_count <= 8:
        _fail("search label legal_action_count must be in [2, 8]")
    for field in ("continuation_steps", "information_set_samples"):
        if row.get(field) != (8 if field == "continuation_steps" else 4):
            _fail(f"search label {field} does not match the fixed search")
    hashes = row.get("information_set_sample_hashes_u64_hex")
    if not isinstance(hashes, list) or len(hashes) != 4 or len(set(hashes)) != 4:
        _fail("search label must contain four distinct information-set hashes")
    for field in ("fallback_selected_index", "best_search_index", "teacher_selected_index"):
        value = row.get(field)
        if not isinstance(value, int) or isinstance(value, bool) or not 0 <= value < legal_count:
            _fail(f"search label {field} is outside the legal action range")
    differs = row.get("teacher_differs_from_fallback")
    if type(differs) is not bool or differs != (
        row["teacher_selected_index"] != row["fallback_selected_index"]
    ):
        _fail("search label teacher_differs_from_fallback is inconsistent")
    values = row.get("action_values_f32_bits_hex")
    if not isinstance(values, list) or len(values) != legal_count:
        _fail("search label action_values width does not match legal actions")
    decoded = [_f32_from_hex(value) for value in values]
    if not all(math.isfinite(value) for value in decoded):
        _fail("search label action value is non-finite")
    fallback = int(row["fallback_selected_index"])
    expected_best = fallback
    for index, value in enumerate(decoded):
        if value > decoded[expected_best]:
            expected_best = index
    margin = decoded[expected_best] - decoded[fallback]
    expected_teacher = (
        expected_best
        if expected_best != fallback and margin >= 0.25
        else fallback
    )
    if (
        row["best_search_index"] != expected_best
        or row.get("search_margin_f32_bits_hex") != _f32_bits_hex(margin)
        or row["teacher_selected_index"] != expected_teacher
    ):
        _fail("search label numerical target rule is inconsistent")
    if row.get("branch_count") != legal_count * 4:
        _fail("search label branch_count is inconsistent")
    natural = row.get("natural_terminal_branch_count")
    bootstrap = row.get("critic_bootstrap_branch_count")
    if not isinstance(natural, int) or not isinstance(bootstrap, int) or natural < 0 or bootstrap < 0:
        _fail("search label branch counts are invalid")
    if natural + bootstrap != row["branch_count"]:
        _fail("search label branch counts do not close")
    model_input = row.get("model_input_sha256")
    if (
        not isinstance(model_input, str)
        or len(model_input) != 64
        or any(character not in "0123456789abcdef" for character in model_input)
    ):
        _fail("search label model_input_sha256 is invalid")
    public_history = row.get("public_history_sha256")
    if (
        not isinstance(public_history, str)
        or len(public_history) != 64
        or any(character not in "0123456789abcdef" for character in public_history)
    ):
        _fail("search label public_history_sha256 is invalid")
    semantics = row.get("action_semantics")
    if not isinstance(semantics, list) or len(semantics) != legal_count:
        _fail("search label action_semantics width does not match legal actions")


def _load_jsonl(path: Path) -> list[dict[str, Any]]:
    rows: list[dict[str, Any]] = []
    with path.open("r", encoding="utf-8") as handle:
        for line_number, line in enumerate(handle, 1):
            if not line.strip():
                continue
            try:
                value = json.loads(line)
            except json.JSONDecodeError as error:
                _fail(f"{path}:{line_number}: invalid JSON: {error}")
            if not isinstance(value, dict):
                _fail(f"{path}:{line_number}: search label is not an object")
            rows.append(value)
    return rows


def _report_label_path(report_path: Path, report: dict[str, Any]) -> tuple[Path, str]:
    if report.get("schema") != SCHEMA + ".labels-report" or report.get("status") != "complete":
        _fail("search-label report is not a complete fixed-arm label report")
    if report.get("pair_count") != PAIR_COUNT:
        _fail("search-label report pair_count is not 256")
    if report.get("target_quality_gate", {}).get("pass") is not True:
        _fail("search-label report did not pass the pre-fit target-quality gate")
    cache_hash = report.get("history_cache_sha256")
    if not isinstance(cache_hash, str) or len(cache_hash) != 64:
        _fail("search-label report must pin history_cache_sha256")
    path_value = report.get("search_label_jsonl")
    hash_value = report.get("search_label_sha256")
    if not isinstance(path_value, str) or not isinstance(hash_value, str):
        _fail("search-label report must contain search_label_jsonl and search_label_sha256")
    path = Path(path_value)
    if not path.is_absolute():
        path = report_path.parent / path
    return path.resolve(strict=True), hash_value


def _load_collector_labels(
    report_path: Path, report: dict[str, Any]
) -> tuple[dict[tuple[Any, ...], dict[str, Any]], dict[str, Any]]:
    if (
        report.get("schema") != COLLECTOR_SCHEMA
        or report.get("status") != "complete"
        or report.get("base_seed") != 1_400_001
        or report.get("pair_start") != 0
        or report.get("pairs") != PAIR_COUNT
    ):
        _fail("search-teacher corpus report is not the qualified 256-pair panel")
    quality = report.get("target_quality_gate")
    seats = report.get("by_candidate_seat")
    if (
        not isinstance(quality, dict)
        or quality.get("minimum_equal_episode_mass_override_rate_per_seat")
        != OVERRIDE_RATE_MIN
        or not isinstance(seats, dict)
        or any(not isinstance(seats.get(seat), dict) for seat in ("p0", "p1"))
    ):
        _fail("search-teacher pre-fit target-quality gate is malformed")
    observed_pairs: set[int] = set()
    labels: dict[tuple[Any, ...], dict[str, Any]] = {}
    task_metadata: list[dict[str, Any]] = []
    for task in report.get("tasks", []):
        first_pair = task.get("first_pair")
        pair_count = task.get("pair_count")
        if not isinstance(first_pair, int) or not isinstance(pair_count, int) or pair_count < 1:
            _fail("search-teacher task range is invalid")
        task_pairs = set(range(first_pair, first_pair + pair_count))
        if observed_pairs & task_pairs:
            _fail("search-teacher task ranges overlap")
        observed_pairs |= task_pairs
        log_value = task.get("log_path")
        expected_log_hash = task.get("log_sha256")
        if not isinstance(log_value, str) or not isinstance(expected_log_hash, str):
            _fail("search-teacher task must pin log_path and log_sha256")
        log_path = Path(log_value)
        if not log_path.is_absolute():
            log_path = report_path.parent / log_path
        log_path = log_path.resolve(strict=True)
        if _sha256(log_path) != expected_log_hash:
            _fail(f"search-teacher task log hash mismatch: {log_path}")
        task_labels: list[dict[str, Any]] = []
        for line_number, line in enumerate(log_path.read_text(encoding="utf-8").splitlines(), 1):
            marker = line.find(DIAGNOSTIC_PREFIX)
            if marker < 0:
                continue
            try:
                row = json.loads(line[marker + len(DIAGNOSTIC_PREFIX) :].strip())
            except json.JSONDecodeError as error:
                _fail(f"{log_path}:{line_number}: invalid search diagnostic: {error}")
            if not isinstance(row, dict):
                _fail(f"{log_path}:{line_number}: search diagnostic is not an object")
            _validate_label_row(row)
            key = _label_key(row)
            if key[0] not in task_pairs:
                _fail(f"{log_path}:{line_number}: diagnostic escaped task pair range")
            if key in labels:
                _fail(f"duplicate search label key {key}")
            labels[key] = row
            task_labels.append(row)
        if task.get("diagnostics") != len(task_labels):
            _fail(f"search-teacher task diagnostic count mismatch: {log_path}")
        task_metadata.append(
            {
                "first_pair": first_pair,
                "pair_count": pair_count,
                "log_path": str(log_path),
                "log_sha256": expected_log_hash,
                "diagnostics": len(task_labels),
            }
        )
    if observed_pairs != set(range(PAIR_COUNT)):
        _fail("search-teacher task ranges do not cover exactly pairs 0..255")
    if report.get("search_diagnostics") != len(labels) or not labels:
        _fail("search-teacher corpus diagnostic count mismatch")
    recomputed_quality = _target_quality_from_labels(labels.values())
    for seat in ("p0", "p1"):
        observed = seats[seat]
        expected = recomputed_quality["by_candidate_seat"][seat]
        for field in ("labels", "overrides"):
            if observed.get(field) != expected[field]:
                _fail(f"search-teacher reported {seat} {field} does not match labels")
        for field in (
            "label_episode_mass",
            "override_episode_mass",
            "equal_episode_mass_override_rate",
        ):
            value = observed.get(field)
            if not isinstance(value, (int, float)) or not math.isclose(
                float(value), float(expected[field]), rel_tol=0.0, abs_tol=1.0e-15
            ):
                _fail(f"search-teacher reported {seat} {field} does not match labels")
    expected_gate = recomputed_quality["target_quality_gate"]
    if (
        quality.get("p0") is not expected_gate["p0"]
        or quality.get("p1") is not expected_gate["p1"]
        or quality.get("pass") is not expected_gate["pass"]
        or report.get("advance_to_training") is not expected_gate["pass"]
    ):
        _fail("search-teacher reported target-quality verdict does not match labels")
    if not expected_gate["pass"]:
        _fail("search-teacher pre-fit target-quality gate did not pass")
    return labels, {
        "report": str(report_path),
        "report_sha256": _sha256(report_path),
        "label_source": "collector_task_logs",
        "task_count": len(task_metadata),
        "tasks": task_metadata,
        "label_count": len(labels),
        "target_quality_recomputed": recomputed_quality,
    }


def _target_quality_from_labels(labels: Iterable[dict[str, Any]]) -> dict[str, Any]:
    episodes: dict[tuple[str, int], list[int]] = {}
    by_seat = {
        "p0": {"labels": 0, "overrides": 0},
        "p1": {"labels": 0, "overrides": 0},
    }
    for row in labels:
        seat = f"p{_candidate_seat(row.get('candidate_seat'))}"
        episode_id = int(row["episode_id"])
        differs = row.get("teacher_differs_from_fallback") is True
        by_seat[seat]["labels"] += 1
        by_seat[seat]["overrides"] += int(differs)
        counts = episodes.setdefault((seat, episode_id), [0, 0])
        counts[0] += 1
        counts[1] += int(differs)
    for seat, values in by_seat.items():
        seat_episodes = [
            counts
            for (episode_seat, _), counts in episodes.items()
            if episode_seat == seat
        ]
        values["label_episode_mass"] = float(len(seat_episodes))
        values["override_episode_mass"] = sum(
            overrides / labels for labels, overrides in seat_episodes
        )
        values["equal_episode_mass_override_rate"] = (
            values["override_episode_mass"] / values["label_episode_mass"]
            if values["label_episode_mass"]
            else 0.0
        )
    gate = {
        "minimum_equal_episode_mass_override_rate_per_seat": OVERRIDE_RATE_MIN,
        "p0": by_seat["p0"]["equal_episode_mass_override_rate"] >= OVERRIDE_RATE_MIN,
        "p1": by_seat["p1"]["equal_episode_mass_override_rate"] >= OVERRIDE_RATE_MIN,
    }
    gate["pass"] = gate["p0"] and gate["p1"]
    return {"by_candidate_seat": by_seat, "target_quality_gate": gate}


def _load_search_labels(report_path: Path, cache_sha256: str) -> tuple[dict[tuple[Any, ...], dict[str, Any]], dict[str, Any]]:
    report = json.loads(report_path.read_text(encoding="utf-8"))
    if report.get("schema") == COLLECTOR_SCHEMA:
        labels, metadata = _load_collector_labels(report_path, report)
        metadata["history_cache_sha256"] = cache_sha256
        metadata["history_cache_binding"] = "cache-hash-recorded-by-driver; collector-report-does-not-pin-cache"
        return labels, metadata
    label_path, expected_hash = _report_label_path(report_path, report)
    if report["history_cache_sha256"] != cache_sha256:
        _fail("search-label report is for a different history cache")
    observed_hash = _sha256(label_path)
    if observed_hash != expected_hash:
        _fail("search-label JSONL hash mismatch")
    labels: dict[tuple[Any, ...], dict[str, Any]] = {}
    for row in _load_jsonl(label_path):
        _validate_label_row(row)
        key = _label_key(row)
        if key in labels:
            _fail(f"duplicate search label key {key}")
        labels[key] = row
    if not labels:
        _fail("search-label JSONL is empty")
    if report.get("label_count") != len(labels):
        _fail("search-label report label_count mismatch")
    recomputed_quality = _target_quality_from_labels(labels.values())
    if not recomputed_quality["target_quality_gate"]["pass"]:
        _fail("search-label pre-fit target-quality gate does not pass when recomputed")
    return labels, {
        "report": str(report_path),
        "report_sha256": _sha256(report_path),
        "search_label_jsonl": str(label_path),
        "search_label_sha256": observed_hash,
        "label_count": len(labels),
        "history_cache_sha256": cache_sha256,
        "target_quality_recomputed": recomputed_quality,
    }


def _cache_key(row: dict[str, Any]) -> tuple[int, str, int, int, int, int]:
    return (
        int(row["pair_index"]),
        str(row["episode"]),
        int(row["candidate_seat"]),
        int(row["step"]),
        int(row["physical_group"]),
        int(row["substep_index"]),
    )


def _load_cache(cache_path: Path) -> tuple[list[dict[str, Any]], str, dict[str, Any]]:
    cache_sha256 = _sha256(cache_path)
    cache = torch.load(cache_path, map_location="cpu", weights_only=False)
    if not isinstance(cache, dict) or cache.get("version") != CACHE_VERSION:
        _fail("history cache version is not the validated structured cache")
    if not cache.get("complete_history_join"):
        _fail("history cache is not marked complete_history_join")
    policy = cache.get("policy")
    value = cache.get("value")
    if not isinstance(policy, list) or not isinstance(value, list) or not policy or not value:
        _fail("history cache must contain policy and value lanes")
    pairs = set(range(PAIR_COUNT))
    if {int(row["pair_index"]) for row in policy} != pairs or {int(row["pair_index"]) for row in value} != pairs:
        _fail("history cache does not contain exactly the fixed 256-pair panel")
    source = cache.get("source")
    if (
        not isinstance(source, dict)
        or source.get("base_seed") != 1_400_001
        or source.get("pair_start") != 0
        or source.get("pairs") != PAIR_COUNT
        or not isinstance(source.get("collector_report_sha256"), str)
        or len(source["collector_report_sha256"]) != 64
    ):
        _fail("history cache source does not bind the exact collector panel")
    distill.screen._attach_complete_action_history(policy, value, HISTORY_LENGTH, CARD_VOCAB)
    return value, cache_sha256, {
        "cache": str(cache_path),
        "cache_sha256": cache_sha256,
        "policy_examples": len(policy),
        "value_examples": len(value),
        "complete_history_join": cache["complete_history_join"],
        "collector_report_sha256": source["collector_report_sha256"],
    }


def _join_rows(
    cache_rows: list[dict[str, Any]], labels: dict[tuple[Any, ...], dict[str, Any]]
) -> list[dict[str, Any]]:
    by_key: dict[tuple[Any, ...], dict[str, Any]] = {}
    for row in cache_rows:
        key = _cache_key(row)
        if key in by_key:
            _fail(f"duplicate cache row key {key}")
        by_key[key] = row
    joined: list[dict[str, Any]] = []
    for key, label in labels.items():
        cache_row = by_key.get(key)
        if cache_row is None:
            _fail(f"search label has no cache row {key}")
        if int(cache_row["selected_index"]) != int(label["fallback_selected_index"]):
            _fail(f"fallback action does not join cache selected action {key}")
        if int(cache_row["old_logits"].numel()) != int(label["legal_action_count"]):
            _fail(f"legal action width does not join cache row {key}")
        if _public_history_sha256(cache_row) != label["public_history_sha256"]:
            _fail(f"public-history hash does not join cache row {key}")
        if int(cache_row["substep_count"]) != 1 or int(cache_row["substep_index"]) != 0:
            _fail(f"fixed-arm search label is not a one-substep surface row {key}")
        row = dict(cache_row)
        row["search_teacher_selected_index"] = int(label["teacher_selected_index"])
        row["fallback_selected_index"] = int(label["fallback_selected_index"])
        joined.append(row)
    if len(joined) != len(labels) or len({_cache_key(row) for row in joined}) != len(labels):
        _fail("search-label/cache join is not one-to-one")
    return sorted(joined, key=_cache_key)


def _decisions(rows: list[dict[str, Any]], target_field: str) -> list[Any]:
    prepared = []
    for source in rows:
        row = dict(source)
        row["teacher_selected_index"] = int(row[target_field])
        prepared.append(row)
    decisions = outcome._physical_decisions(prepared)
    if any(len(decision.rows) != 1 for decision in decisions):
        _fail("fixed-arm search corpus contains multi-substep decisions")
    return decisions


def _decision_identity(decision: Any) -> tuple[Any, ...]:
    return (
        tuple(decision.key),
        tuple(decision.episode_key),
        int(decision.pair_index),
        int(decision.candidate_seat),
        float(decision.episode_weight),
        tuple(_cache_key(row) for row in decision.rows),
        tuple(float(row["terminal_reward"]) for row in decision.rows),
    )


def _require_matched_arms(fallback: list[Any], search: list[Any], lane: str) -> str:
    fallback_identity = [_decision_identity(decision) for decision in fallback]
    search_identity = [_decision_identity(decision) for decision in search]
    if fallback_identity != search_identity:
        _fail(f"fixed arms do not share the exact ordered examples in {lane}")
    payload = json.dumps(
        fallback_identity,
        sort_keys=True,
        separators=(",", ":"),
        allow_nan=False,
    ).encode("utf-8")
    return hashlib.sha256(payload).hexdigest()


def _split(decisions: Iterable[Any]) -> tuple[list[Any], list[Any], list[Any]]:
    return recurrent._split(decisions)


def _new_model(device: torch.device) -> Any:
    random.seed(SEED)
    torch.manual_seed(SEED)
    torch.cuda.manual_seed_all(SEED)
    model = recurrent.RecurrentStructuredActorCritic(DIM).to(device)
    torch.nn.init.zeros_(model.policy_head.weight)
    torch.nn.init.zeros_(model.policy_head.bias)
    for parameter in model.value_head.parameters():
        parameter.requires_grad_(False)
    return model


def _synchronize(device: torch.device) -> None:
    if device.type == "cuda":
        torch.cuda.synchronize(device)


def _fit_fixed_arm(train: list[Any], device: torch.device) -> tuple[Any, dict[str, Any]]:
    model = _new_model(device)
    parameters = [parameter for parameter in model.parameters() if parameter.requires_grad]
    optimizer = torch.optim.AdamW(parameters, lr=LEARNING_RATE, weight_decay=WEIGHT_DECAY)
    rng = random.Random(SEED)
    history: list[dict[str, Any]] = []
    for epoch in range(1, EPOCHS + 1):
        model.train()
        started = time.perf_counter()
        loss_sum = 0.0
        steps = 0
        gradient_max = 0.0
        for batch in sparse._batches(train, BATCH_SIZE, rng):
            loss, _ = _dense_loss(model, batch, device)
            if not torch.isfinite(loss):
                _fail("fixed-arm loss is non-finite")
            optimizer.zero_grad(set_to_none=True)
            loss.backward()
            gradient = torch.nn.utils.clip_grad_norm_(parameters, GRADIENT_CAP)
            if not torch.isfinite(gradient):
                _fail("fixed-arm gradient is non-finite")
            optimizer.step()
            loss_sum += float(loss.detach())
            gradient_max = max(gradient_max, float(gradient))
            steps += 1
        _synchronize(device)
        history.append(
            {
                "epoch": epoch,
                "mean_loss": loss_sum / max(steps, 1),
                "optimizer_steps": steps,
                "maximum_preclip_gradient_norm": gradient_max,
                "seconds": time.perf_counter() - started,
            }
        )
    if len(history) != EPOCHS:
        _fail("fixed-arm epoch count changed")
    if any(not torch.isfinite(parameter).all() for parameter in model.parameters()):
        _fail("fixed-arm model contains non-finite parameters")
    return model, {
        "selected_epoch": None,
        "final_epoch": EPOCHS,
        "training_history": history,
        "maximum_preclip_gradient_norm": max(
            row["maximum_preclip_gradient_norm"] for row in history
        ),
        "model_state_sha256": recurrent._state_sha256(model),
    }


def _dense_loss(model: Any, decisions: list[Any], device: torch.device) -> tuple[Any, dict[str, float]]:
    rows = [decision.rows[0] for decision in decisions]
    packed = recurrent.pack_rows(rows, device)
    candidate_logits, _ = recurrent._candidate_logits(model, packed, LOG_RATIO_BUDGET)
    teacher = torch.tensor(
        [int(row["teacher_selected_index"]) for row in rows], dtype=torch.long, device=device
    )
    weights = torch.tensor(
        [float(decision.episode_weight) for decision in decisions], dtype=torch.float32, device=device
    )
    parent_logp = torch.log_softmax(packed.parent_logits, dim=1)
    candidate_logp = torch.log_softmax(candidate_logits, dim=1)
    parent_probability = torch.softmax(packed.parent_logits, dim=1)
    nll = -candidate_logp.gather(1, teacher.unsqueeze(1)).squeeze(1)
    mass = weights.sum().clamp_min(1.0e-12)
    cross_entropy = (nll * weights).sum() / mass
    parent_kl = (parent_probability * (parent_logp - candidate_logp)).sum(dim=1)
    preservation = (parent_kl * weights).sum() / mass
    total = cross_entropy + BETA * preservation
    return total, {"cross_entropy": float(cross_entropy.detach()), "parent_kl": float(preservation.detach())}


def _finite_tree(value: Any) -> bool:
    if isinstance(value, dict):
        return all(_finite_tree(child) for child in value.values())
    if isinstance(value, (list, tuple)):
        return all(_finite_tree(child) for child in value)
    if isinstance(value, (float, int)) and not isinstance(value, bool):
        return math.isfinite(float(value))
    return True


def _movement_checks(metrics: dict[str, Any], prefix: str) -> dict[str, bool]:
    rows = [metrics["overall"], *metrics["by_candidate_seat"].values()]
    return {
        f"{prefix}_finite": _finite_tree(metrics),
        f"{prefix}_mean_tv_within_envelope": all(
            row["mean_total_variation"] <= MEAN_TV_MAX for row in rows
        ),
        f"{prefix}_p90_tv_within_envelope": all(
            row["p90_total_variation"] <= P90_TV_MAX for row in rows
        ),
        f"{prefix}_hard_log_ratio_envelope": max(
            row["maximum_absolute_log_ratio"] for row in rows
        )
        <= LOG_RATIO_BUDGET,
    }


def _comparison_gate(
    search_metrics: dict[str, Any],
    fallback_metrics: dict[str, Any],
    search_training: dict[str, Any],
    fallback_training: dict[str, Any],
) -> dict[str, Any]:
    search_overall = search_metrics["overall"]
    fallback_overall = fallback_metrics["overall"]
    search_nll = search_overall["candidate_selected_index_nll"]
    fallback_nll = fallback_overall["candidate_selected_index_nll"]
    relative_nll = (fallback_nll - search_nll) / max(fallback_nll, 1.0e-12)
    top1_delta = (
        search_overall["candidate_top1"] - fallback_overall["candidate_top1"]
    )
    checks = {
        "search_target_nll_improves_at_least_5_percent": relative_nll
        >= RELATIVE_NLL_MIN,
        "search_target_top1_improves_at_least_3pp": top1_delta >= TOP1_DELTA_MIN,
        "search_target_nll_nonregressing_at_both_seats": all(
            search_metrics["by_candidate_seat"][str(seat)][
                "candidate_selected_index_nll"
            ]
            <= fallback_metrics["by_candidate_seat"][str(seat)][
                "candidate_selected_index_nll"
            ]
            for seat in (0, 1)
        ),
        "search_maximum_preclip_gradient_within_cap": search_training[
            "maximum_preclip_gradient_norm"
        ]
        <= GRADIENT_CAP,
        "fallback_maximum_preclip_gradient_within_cap": fallback_training[
            "maximum_preclip_gradient_norm"
        ]
        <= GRADIENT_CAP,
        **_movement_checks(search_metrics, "search"),
        **_movement_checks(fallback_metrics, "fallback"),
    }
    return {
        "search_target_relative_nll_improvement": relative_nll,
        "search_target_top1_delta": top1_delta,
        "checks": checks,
        "pass": all(checks.values()),
    }


def _refit_stability_gate(
    search_metrics: dict[str, Any],
    fallback_metrics: dict[str, Any],
    search_training: dict[str, Any],
    fallback_training: dict[str, Any],
) -> dict[str, Any]:
    checks = {
        "search_maximum_preclip_gradient_within_cap": search_training[
            "maximum_preclip_gradient_norm"
        ]
        <= GRADIENT_CAP,
        "fallback_maximum_preclip_gradient_within_cap": fallback_training[
            "maximum_preclip_gradient_norm"
        ]
        <= GRADIENT_CAP,
        **_movement_checks(search_metrics, "search"),
        **_movement_checks(fallback_metrics, "fallback"),
    }
    return {"checks": checks, "pass": all(checks.values())}


def _configure(device_ordinal: int) -> torch.device:
    if device_ordinal != GPU_ORDINAL:
        _fail("fixed formal runs require physical GPU ordinal 1")
    if not torch.cuda.is_available() or torch.cuda.device_count() <= GPU_ORDINAL:
        _fail("fixed formal run requires available CUDA GPU ordinal 1")
    random.seed(SEED)
    np.random.seed(SEED)
    torch.manual_seed(SEED)
    torch.cuda.manual_seed_all(SEED)
    torch.use_deterministic_algorithms(True)
    torch.backends.cudnn.benchmark = False
    torch.backends.cuda.matmul.allow_tf32 = False
    torch.backends.cudnn.allow_tf32 = False
    torch.set_num_threads(8)
    device = torch.device(f"cuda:{GPU_ORDINAL}")
    torch.cuda.set_device(device)
    return device


def _toolchain(device: torch.device | None) -> dict[str, Any]:
    result: dict[str, Any] = {"python": platform.python_version(), "torch": torch.__version__}
    if device is None:
        result["gpu_ordinal"] = None
        return result
    result.update(
        {
            "cuda": torch.version.cuda,
            "cudnn": torch.backends.cudnn.version(),
            "gpu_ordinal": device.index,
            "gpu_name": torch.cuda.get_device_name(device),
        }
    )
    return result


def _model_payload(arm: str, model: Any, metadata: dict[str, Any]) -> dict[str, Any]:
    state = {name: tensor.detach().cpu() for name, tensor in model.state_dict().items()}
    return {
        "schema": SCHEMA + ".model",
        "arm": arm,
        "settings": FROZEN_SETTINGS,
        "training": metadata,
        "model_state_dict": state,
        "model_state_sha256": recurrent._state_sha256(model),
    }


def _save_model(path: Path, payload: dict[str, Any]) -> str:
    if path.exists():
        _fail(f"refusing to overwrite existing model: {path}")
    temporary = path.with_name(path.name + ".tmp")
    if temporary.exists():
        _fail(f"temporary model output already exists: {temporary}")
    torch.save(payload, temporary)
    os.replace(temporary, path)
    return _sha256(path)


def _load_inputs(cache_path: Path, report_path: Path) -> tuple[list[dict[str, Any]], dict[str, Any]]:
    cache_rows, cache_sha256, cache_metadata = _load_cache(cache_path)
    labels, label_metadata = _load_search_labels(report_path, cache_sha256)
    if (
        label_metadata.get("label_source") == "collector_task_logs"
        and cache_metadata["collector_report_sha256"]
        != label_metadata["report_sha256"]
    ):
        _fail("history cache and search labels bind different collector reports")
    joined = _join_rows(cache_rows, labels)
    if not joined:
        _fail("search-label/cache join produced no examples")
    source = {**cache_metadata, **label_metadata, "joined_examples": len(joined)}
    return joined, source


def run(args: argparse.Namespace) -> dict[str, Any]:
    cache_path = args.history_cache.resolve(strict=True)
    report_path = args.search_label_report.resolve(strict=True)
    if args.output_dir.exists() and any(args.output_dir.iterdir()):
        _fail("output directory must be absent or empty")
    joined, source = _load_inputs(cache_path, report_path)
    command = [str(Path(sys.executable).resolve()), str(Path(__file__).resolve()), *sys.argv[1:]]
    result: dict[str, Any] = {
        "schema": SCHEMA,
        "status": "validated_plan",
        "settings": FROZEN_SETTINGS,
        "source": source,
        "split": {},
        "arms": {},
        "command": command,
        "git_commit": _git_head(),
        "toolchain": _toolchain(None),
        "non_claims": [
            "offline label fit is not playing strength",
            "terminal win/loss/draw remains the sole promotion measure",
            "search labels are supervised policy targets, not rewards",
            "no checkpoint or hyperparameter was selected",
        ],
    }
    fallback = _decisions(joined, "fallback_selected_index")
    search = _decisions(joined, "search_teacher_selected_index")
    all_examples_sha256 = _require_matched_arms(fallback, search, "all examples")
    fallback_split = _split(fallback)
    search_split = _split(search)
    split_identity = {
        name: _require_matched_arms(fallback_lane, search_lane, name)
        for name, fallback_lane, search_lane in zip(
            ("train", "selection", "heldout"), fallback_split, search_split
        )
    }
    result["split"] = {
        "train_decisions": len(fallback_split[0]),
        "selection_decisions": len(fallback_split[1]),
        "heldout_decisions": len(fallback_split[2]),
        "mod4": {"train": [1, 2], "selection": [3], "heldout": [0]},
        "all_examples_sha256": all_examples_sha256,
        "lane_example_sha256": split_identity,
    }
    result["arms"] = {
        "search-target": {"target": "search_teacher_selected_index", "decisions": len(search)},
        "fallback-control": {"target": "fallback_selected_index", "decisions": len(fallback)},
    }
    if not args.execute:
        _atomic_json(args.output_dir / "report.json", result)
        return result
    device = _configure(args.device)
    result["toolchain"] = _toolchain(device)
    started = time.perf_counter()
    search_train, search_selection, search_heldout = search_split
    fallback_train, _, _ = fallback_split
    search_model, search_training = _fit_fixed_arm(search_train, device)
    fallback_model, fallback_training = _fit_fixed_arm(fallback_train, device)
    search_selection_metrics = recurrent._evaluate(
        search_model,
        search_selection,
        BATCH_SIZE,
        device,
        LOG_RATIO_BUDGET,
    )
    fallback_selection_metrics = recurrent._evaluate(
        fallback_model,
        search_selection,
        BATCH_SIZE,
        device,
        LOG_RATIO_BUDGET,
    )
    selection_gate = _comparison_gate(
        search_selection_metrics,
        fallback_selection_metrics,
        search_training,
        fallback_training,
    )
    result["development"] = {
        "search-target": {
            "training": search_training,
            "selection_on_search_targets": search_selection_metrics,
        },
        "fallback-control": {
            "training": fallback_training,
            "selection_on_search_targets": fallback_selection_metrics,
        },
        "selection_gate": selection_gate,
    }
    if not selection_gate["pass"]:
        result["status"] = "stop_selection_residue_3"
        result["advance_to_terminal_selector"] = False
        result["elapsed_seconds"] = time.perf_counter() - started
        _atomic_json(args.output_dir / "report.json", result)
        return result

    search_heldout_metrics = recurrent._evaluate(
        search_model,
        search_heldout,
        BATCH_SIZE,
        device,
        LOG_RATIO_BUDGET,
    )
    fallback_heldout_metrics = recurrent._evaluate(
        fallback_model,
        search_heldout,
        BATCH_SIZE,
        device,
        LOG_RATIO_BUDGET,
    )
    heldout_gate = _comparison_gate(
        search_heldout_metrics,
        fallback_heldout_metrics,
        search_training,
        fallback_training,
    )
    result["development"]["search-target"][
        "heldout_on_search_targets"
    ] = search_heldout_metrics
    result["development"]["fallback-control"][
        "heldout_on_search_targets"
    ] = fallback_heldout_metrics
    result["development"]["heldout_gate"] = heldout_gate
    if not heldout_gate["pass"]:
        result["status"] = "stop_heldout_residue_0"
        result["advance_to_terminal_selector"] = False
        result["elapsed_seconds"] = time.perf_counter() - started
        _atomic_json(args.output_dir / "report.json", result)
        return result

    del search_model, fallback_model
    torch.cuda.empty_cache()
    search_refit, search_refit_training = _fit_fixed_arm(search, device)
    fallback_refit, fallback_refit_training = _fit_fixed_arm(fallback, device)
    search_refit_metrics = recurrent._evaluate(
        search_refit,
        search,
        BATCH_SIZE,
        device,
        LOG_RATIO_BUDGET,
    )
    fallback_refit_metrics = recurrent._evaluate(
        fallback_refit,
        search,
        BATCH_SIZE,
        device,
        LOG_RATIO_BUDGET,
    )
    refit_gate = _refit_stability_gate(
        search_refit_metrics,
        fallback_refit_metrics,
        search_refit_training,
        fallback_refit_training,
    )
    result["refit"] = {
        "search-target": {
            "training": search_refit_training,
            "all_pairs_on_search_targets": search_refit_metrics,
        },
        "fallback-control": {
            "training": fallback_refit_training,
            "all_pairs_on_search_targets": fallback_refit_metrics,
        },
        "stability_gate": refit_gate,
    }
    if not refit_gate["pass"]:
        result["status"] = "stop_full_refit_stability"
        result["advance_to_terminal_selector"] = False
        result["elapsed_seconds"] = time.perf_counter() - started
        _atomic_json(args.output_dir / "report.json", result)
        return result

    outputs = {
        "search-target": _save_model(
            args.output_dir / "search-target.model.pt",
            _model_payload(
                "search-target",
                search_refit,
                {
                    **search_refit_training,
                    "source": source,
                    "development_selection_gate": selection_gate,
                    "development_heldout_gate": heldout_gate,
                    "refit_stability_gate": refit_gate,
                    "all_pairs_on_search_targets": search_refit_metrics,
                },
            ),
        ),
        "fallback-control": _save_model(
            args.output_dir / "fallback-control.model.pt",
            _model_payload(
                "fallback-control",
                fallback_refit,
                {
                    **fallback_refit_training,
                    "source": source,
                    "development_selection_gate": selection_gate,
                    "development_heldout_gate": heldout_gate,
                    "refit_stability_gate": refit_gate,
                    "all_pairs_on_search_targets": fallback_refit_metrics,
                },
            ),
        ),
    }
    result["status"] = "complete"
    result["advance_to_terminal_selector"] = True
    result["outputs"] = outputs
    result["elapsed_seconds"] = time.perf_counter() - started
    _atomic_json(args.output_dir / "report.json", result)
    return result


def arguments(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--history-cache", type=Path, required=True)
    parser.add_argument("--search-label-report", type=Path, required=True)
    parser.add_argument("--output-dir", type=Path, required=True)
    parser.add_argument("--device", type=int, default=GPU_ORDINAL)
    parser.add_argument("--execute", action="store_true", help="explicitly run both fixed arms on GPU 1")
    return parser.parse_args(argv)


def main(argv: list[str] | None = None) -> int:
    try:
        result = run(arguments(argv))
    except (OSError, RuntimeError, ValueError, KeyError, TypeError) as error:
        print(f"error: {error}", file=sys.stderr)
        return 2
    print(json.dumps(result, sort_keys=True, allow_nan=False))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
