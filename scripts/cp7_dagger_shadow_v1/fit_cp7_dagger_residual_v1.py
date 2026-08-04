#!/usr/bin/env python3
"""Fit and package a bounded structured residual from candidate-state CP7 labels."""

from __future__ import annotations

import argparse
import hashlib
import json
import math
from pathlib import Path
import random
import shutil
import subprocess
import sys
import time
from typing import Any, Iterable

import torch


SCRIPT_DIR = Path(__file__).resolve().parent
STRUCTURED_DIR = SCRIPT_DIR.parent / "structured_adapter_screen_v1"
sys.path.insert(0, str(STRUCTURED_DIR))

import fit_policy_only_structured_successor_v1 as publish  # noqa: E402
import run_screen as structured  # noqa: E402
import run_structured_outcome_policy_v1 as outcome  # noqa: E402
import run_structured_successor_distillation_v1 as distill  # noqa: E402


FIT_SCHEMA = "mtg-kernel-cp7-candidate-state-dagger-fit/v1"
REPORT_SCHEMA = "mtg-kernel-structured-policy-cp7-dagger-residual-report/v1"
CANDIDATE_SCHEMA = "mtg-kernel-structured-policy-successor-candidate/v8"
PARITY_SCHEMA = "mtg-kernel-structured-policy-cp7-dagger-residual-parity-fixture/v1"
ARCHITECTURE = (
    "complete-public-history-structured-policy-cp7-dagger-residual-"
    "frozen-parent-value/v1"
)
OBJECTIVE = "candidate-state-cp7-selected-index-cross-entropy/v1"
PROJECTION_METHOD = "initializer-weighted-centered-logit-residual-clamp/v1"
COMPOSITE_DOMAIN = b"mtg-kernel-structured-policy-cp7-dagger-residual-composite-model/v1"
HISTORY_CACHE_SHA256 = "98babc28617a57d3053bf178ba1d1084f943339f69d75b918402f2e4dd10d1df"
INITIALIZER_STATE_SHA256 = "ff2abf50e8760780a9331e53aa7323cb96e3c64edb6e7d89062dbe38bf6a5cc0"
INITIALIZER_CANDIDATE_SHA256 = "204beb91c1a4b039e0c497f2b420e823b5cc9e2ceb8560f897d0b6251e916b72"
INITIALIZER_REPORT_SHA256 = "7d854edb46119a611d4283e6cf4630d0207ceb24c12b4089a7d27a43c97fe0b3"
INITIALIZER_WEIGHTS_SHA256 = "ca3c45cd69d8d60f1f921bc78c27b098064ef6b16fe7566b84e5045681781b28"
INITIALIZER_COMPOSITE_SHA256 = "47b10c1114efc01f9445c71c0c8c4d8cd4a4b89a2154ac68275f3b0c6ebb9ce3"
BASE_SEED = 1_400_001
PAIR_COUNT = 256
SEED = 20_260_808
EPOCHS = 5
BATCH_SIZE = 64
LR = 3.0e-4
WEIGHT_DECAY = 1.0e-4
GRAD_CAP = 5.0
CLIP_GRID = (0.03, 0.04, 0.05, 0.06, 0.08, 0.10, 0.12, 0.16, 0.20, 0.24, 0.28, 0.32, 0.40)
MEAN_TV_MAX = 0.030
P90_TV_MAX = 0.100
JOINT_LOG_RATIO_MAX = 0.50
HELDOUT_RELATIVE_NLL_IMPROVEMENT_MIN = 0.05
HELDOUT_TOP1_DELTA_MIN = 0.03


def _fail(message: str) -> None:
    raise RuntimeError(message)


def _sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def _json_bytes(value: Any) -> bytes:
    return (
        json.dumps(value, indent=2, sort_keys=True, allow_nan=False) + "\n"
    ).encode("utf-8")


def _write_new(path: Path, value: Any) -> None:
    if path.exists():
        _fail(f"refusing to overwrite {path}")
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(_json_bytes(value))


def _git_head() -> str:
    completed = subprocess.run(
        ["git", "rev-parse", "HEAD"],
        cwd=SCRIPT_DIR.parent.parent,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
        check=True,
    )
    return completed.stdout.strip()


def _label_key(row: dict[str, Any]) -> tuple[int, str, int, int, int, int]:
    seat = row.get("candidate_seat")
    if isinstance(seat, str) and seat in ("p0", "p1"):
        seat = int(seat[1])
    return (
        int(row["pair_index"]),
        str(row["episode_id"]),
        int(seat),
        int(row["step"]),
        int(row["physical_decision_id"]),
        int(row["substep_index"]),
    )


def _example_key(row: dict[str, Any]) -> tuple[int, str, int, int, int, int]:
    return (
        int(row["pair_index"]),
        str(row["episode"]),
        int(row["candidate_seat"]),
        int(row["step"]),
        int(row["physical_group"]),
        int(row["substep_index"]),
    )


def _load_labels(corpus_report_path: Path) -> tuple[dict[tuple[int, str, int, int, int, int], dict[str, Any]], dict[str, Any]]:
    report_sha256 = _sha256(corpus_report_path)
    report = json.loads(corpus_report_path.read_text(encoding="utf-8"))
    if (
        report.get("schema") != "mtg-kernel-cp7-candidate-shadow-corpus/v1"
        or report.get("status") != "complete"
        or report.get("base_seed") != BASE_SEED
        or report.get("pair_start") != 0
        or report.get("pairs") != PAIR_COUNT
        or float(report.get("usable_fraction", 0.0)) < 0.95
    ):
        _fail("candidate-state label corpus is not qualified")
    labels: dict[tuple[int, str, int, int, int, int], dict[str, Any]] = {}
    teacher_hashes: list[str] = []
    for task in report.get("tasks", []):
        path = Path(task["teacher_path"])
        expected_sha256 = task["teacher_sha256"]
        if _sha256(path) != expected_sha256:
            _fail(f"teacher task hash mismatch: {path}")
        teacher_hashes.append(expected_sha256)
        with path.open("r", encoding="utf-8") as handle:
            for line in handle:
                row = json.loads(line)
                if row.get("record_type") != "decision":
                    continue
                teacher_index = row.get("teacher_selected_index")
                legal_count = row.get("legal_action_count")
                usable = (
                    row.get("teacher_status")
                    in {"source_id", "text", "text_disambig", "plan_pass", "step_pass"}
                    and isinstance(teacher_index, int)
                    and isinstance(legal_count, int)
                    and 0 <= teacher_index < legal_count
                )
                if not usable:
                    continue
                key = _label_key(row)
                if key in labels:
                    _fail(f"duplicate candidate-state label {key}")
                labels[key] = row
    if len(labels) != int(report["usable_labels"]):
        _fail("candidate-state label count mismatch")
    return labels, {
        "corpus_report": str(corpus_report_path),
        "corpus_report_sha256": report_sha256,
        "teacher_task_sha256s": teacher_hashes,
        "label_count": len(labels),
        "candidate_teacher_disagreements": report["candidate_teacher_disagreements"],
        "disagreement_fraction": report["disagreement_fraction"],
    }


def _load_decisions(
    history_cache_path: Path,
    labels: dict[tuple[int, str, int, int, int, int], dict[str, Any]],
) -> tuple[list[Any], dict[str, Any], dict[str, float]]:
    started = time.perf_counter()
    cache_sha256 = _sha256(history_cache_path)
    if cache_sha256 != HISTORY_CACHE_SHA256:
        _fail("complete-history cache SHA-256 mismatch")
    cache = torch.load(history_cache_path, map_location="cpu", weights_only=False)
    loaded = time.perf_counter()
    if cache.get("version") != structured.SCRIPT_VERSION or not cache.get("complete_history_join"):
        _fail("history cache is not the validated complete public history corpus")
    selected_pairs = set(range(PAIR_COUNT))
    policy = [row for row in cache["policy"] if int(row["pair_index"]) in selected_pairs]
    value = [row for row in cache["value"] if int(row["pair_index"]) in selected_pairs]
    if (
        {int(row["pair_index"]) for row in policy} != selected_pairs
        or {int(row["pair_index"]) for row in value} != selected_pairs
    ):
        _fail("history cache lacks the exact 256-pair label panel")
    structured._attach_complete_action_history(
        policy, value, distill.HISTORY_LENGTH, distill.CARD_VOCAB
    )
    history_ready = time.perf_counter()
    joined: list[dict[str, Any]] = []
    observed: set[tuple[int, str, int, int, int, int]] = set()
    for row in value:
        key = _example_key(row)
        label = labels.get(key)
        if label is None:
            continue
        if (
            row["decision_kind"] != "surface"
            or int(row["substep_count"]) != 1
            or int(row["substep_index"]) != 0
            or int(row["selected_index"]) != int(label["candidate_selected_index"])
            or int(row["old_logits"].numel()) != int(label["legal_action_count"])
        ):
            _fail(f"history-cache label join mismatch {key}")
        row["teacher_selected_index"] = int(label["teacher_selected_index"])
        joined.append(row)
        observed.add(key)
    if observed != set(labels):
        missing = len(set(labels) - observed)
        _fail(f"history cache did not join {missing} candidate-state labels")
    decisions = outcome._physical_decisions(joined)
    if any(len(decision.rows) != 1 for decision in decisions):
        _fail("priority-only DAgger decision unexpectedly has multiple substeps")
    grouped = time.perf_counter()
    return decisions, {
        "history_cache": str(history_cache_path),
        "history_cache_sha256": cache_sha256,
        "pair_count": PAIR_COUNT,
        "episode_count": PAIR_COUNT * 2,
        "physical_decision_count": len(decisions),
        "label_count": len(joined),
        "history_sources": "candidate_and_cp7_public_actions",
        "label_scope": "candidate_priority_surface_only/v1",
    }, {
        "hash_and_load_seconds": loaded - started,
        "attach_history_seconds": history_ready - loaded,
        "join_and_group_seconds": grouped - history_ready,
    }


def _load_initializer(state_path: Path, root: Path) -> tuple[Any, dict[str, Any]]:
    candidate_path = root / publish.CANDIDATE_FILENAME
    report_path = root / "report.json"
    weights_path = root / "weights.f32le"
    observed = {
        "candidate_json_sha256": _sha256(candidate_path),
        "report_sha256": _sha256(report_path),
        "weights_sha256": _sha256(weights_path),
        "model_state_sha256": _sha256(state_path),
    }
    expected = {
        "candidate_json_sha256": INITIALIZER_CANDIDATE_SHA256,
        "report_sha256": INITIALIZER_REPORT_SHA256,
        "weights_sha256": INITIALIZER_WEIGHTS_SHA256,
        "model_state_sha256": INITIALIZER_STATE_SHA256,
    }
    if observed != expected:
        _fail("structured initializer identity mismatch")
    payload = torch.load(state_path, map_location="cpu", weights_only=False)
    state = payload.get("model_state_dict")
    if not isinstance(state, dict):
        _fail("initializer state lacks model_state_dict")
    model = distill._model()
    model.load_state_dict(state, strict=True)
    observed["composite_model_parameter_sha256"] = INITIALIZER_COMPOSITE_SHA256
    return model, observed


def _policy_parameters(model: Any) -> list[torch.nn.Parameter]:
    parameters = []
    for name, parameter in model.named_parameters():
        if name.startswith("value_head."):
            parameter.requires_grad_(False)
        else:
            parameter.requires_grad_(True)
            parameters.append(parameter)
    return parameters


def _fit(model: Any, decisions: list[Any], fit_seed: int) -> list[dict[str, Any]]:
    weights = distill._episode_weights(decisions)
    parameters = _policy_parameters(model)
    optimizer = torch.optim.AdamW(parameters, lr=LR, weight_decay=WEIGHT_DECAY)
    rng = random.Random(fit_seed)
    history = []
    for epoch in range(EPOCHS):
        order = list(range(len(decisions)))
        rng.shuffle(order)
        weighted_loss = 0.0
        weighted_mass = 0.0
        gradient_norm_max = 0.0
        steps = 0
        model.train()
        for start in range(0, len(order), BATCH_SIZE):
            batch = [decisions[index] for index in order[start : start + BATCH_SIZE]]
            losses = []
            masses = []
            for decision in batch:
                _, row_mass = weights[decision.key]
                row = decision.rows[0]
                logits, _ = model._one(row)
                losses.append(
                    -torch.log_softmax(logits, dim=0)[int(row["teacher_selected_index"])]
                )
                masses.append(row_mass)
            mass_tensor = torch.tensor(masses, dtype=torch.float32)
            loss = (torch.stack(losses) * mass_tensor).sum() / mass_tensor.sum()
            optimizer.zero_grad(set_to_none=True)
            loss.backward()
            gradient_norm = torch.nn.utils.clip_grad_norm_(parameters, GRAD_CAP)
            if not torch.isfinite(gradient_norm):
                _fail("non-finite DAgger gradient")
            optimizer.step()
            mass = float(mass_tensor.sum())
            weighted_loss += float(loss.detach()) * mass
            weighted_mass += mass
            gradient_norm_max = max(gradient_norm_max, float(gradient_norm))
            steps += 1
        history.append(
            {
                "epoch": epoch + 1,
                "selected_index_cross_entropy": weighted_loss / weighted_mass,
                "policy_mass": weighted_mass,
                "maximum_preclip_gradient_norm": gradient_norm_max,
                "optimizer_steps": steps,
            }
        )
    return history


def _bounded_logits(initial: torch.Tensor, trained: torch.Tensor, clip: float) -> torch.Tensor:
    initial_f64 = initial.double()
    delta = trained.double() - initial_f64
    centered = delta - (torch.softmax(initial_f64, dim=0) * delta).sum()
    return initial_f64 + centered.clamp(min=-clip, max=clip)


def _empty_metric() -> dict[str, Any]:
    return {
        "initializer_nll": 0.0,
        "candidate_nll": 0.0,
        "initializer_top1": 0.0,
        "candidate_top1": 0.0,
        "tv": 0.0,
        "top_same": 0.0,
        "mass": 0.0,
        "tv_samples": [],
        "max_joint": 0.0,
        "rows": 0,
        "decisions": 0,
    }


def _finish_metric(raw: dict[str, Any]) -> dict[str, Any]:
    mass = max(float(raw["mass"]), 1.0e-12)
    initializer_nll = float(raw["initializer_nll"]) / mass
    candidate_nll = float(raw["candidate_nll"]) / mass
    top_same = float(raw["top_same"]) / mass
    return {
        "initializer_selected_index_nll": initializer_nll,
        "candidate_selected_index_nll": candidate_nll,
        "relative_nll_improvement": (initializer_nll - candidate_nll)
        / max(initializer_nll, 1.0e-12),
        "initializer_top1": float(raw["initializer_top1"]) / mass,
        "candidate_top1": float(raw["candidate_top1"]) / mass,
        "top1_delta": (float(raw["candidate_top1"]) - float(raw["initializer_top1"]))
        / mass,
        "mean_total_variation": float(raw["tv"]) / mass,
        "p90_total_variation": distill._weighted_quantile(raw["tv_samples"], 0.90),
        "top_action_agreement": top_same,
        "top_action_change_rate": 1.0 - top_same,
        "maximum_absolute_joint_log_ratio": float(raw["max_joint"]),
        "policy_mass": float(raw["mass"]),
        "policy_rows": int(raw["rows"]),
        "physical_decisions": int(raw["decisions"]),
    }


def _metrics(
    initializer: Any,
    trained: Any,
    decisions: list[Any],
    clip: float,
) -> dict[str, Any]:
    weights = distill._episode_weights(decisions)
    raw = {"overall": _empty_metric(), "seats": {0: _empty_metric(), 1: _empty_metric()}}
    initializer.eval()
    trained.eval()
    with torch.no_grad():
        for decision in decisions:
            _, row_mass = weights[decision.key]
            old_joint = 0.0
            new_joint = 0.0
            for row in decision.rows:
                initial_logits, _ = initializer._one(row)
                trained_logits, _ = trained._one(row)
                candidate_logits = _bounded_logits(initial_logits, trained_logits, clip)
                label = int(row["teacher_selected_index"])
                old_probability = torch.softmax(row["old_logits"].double(), dim=0)
                new_probability = torch.softmax(candidate_logits, dim=0)
                tv = float(0.5 * (old_probability - new_probability).abs().sum())
                selected = int(row["selected_index"])
                old_joint += float(torch.log_softmax(row["old_logits"].double(), dim=0)[selected])
                new_joint += float(torch.log_softmax(candidate_logits, dim=0)[selected])
                for target in (raw["overall"], raw["seats"][decision.candidate_seat]):
                    target["initializer_nll"] += float(-torch.log_softmax(initial_logits.double(), dim=0)[label]) * row_mass
                    target["candidate_nll"] += float(-torch.log_softmax(candidate_logits, dim=0)[label]) * row_mass
                    target["initializer_top1"] += float(int(initial_logits.argmax()) == label) * row_mass
                    target["candidate_top1"] += float(int(candidate_logits.argmax()) == label) * row_mass
                    target["tv"] += tv * row_mass
                    target["top_same"] += float(int(row["old_logits"].argmax()) == int(candidate_logits.argmax())) * row_mass
                    target["mass"] += row_mass
                    target["tv_samples"].append((tv, row_mass))
                    target["rows"] += 1
            joint_delta = abs(new_joint - old_joint)
            for target in (raw["overall"], raw["seats"][decision.candidate_seat]):
                target["max_joint"] = max(target["max_joint"], joint_delta)
                target["decisions"] += 1
    return {
        "overall": _finish_metric(raw["overall"]),
        "by_candidate_seat": {
            str(seat): _finish_metric(raw["seats"][seat]) for seat in (0, 1)
        },
    }


def _movement_safe(metrics: dict[str, Any]) -> bool:
    rows = [metrics["overall"], metrics["by_candidate_seat"]["0"], metrics["by_candidate_seat"]["1"]]
    return (
        all(row["mean_total_variation"] <= MEAN_TV_MAX for row in rows)
        and all(row["p90_total_variation"] <= P90_TV_MAX for row in rows)
        and metrics["overall"]["maximum_absolute_joint_log_ratio"] <= JOINT_LOG_RATIO_MAX
    )


def _screen(
    initializer: Any,
    trained: Any,
    selection: list[Any],
    heldout: list[Any],
) -> dict[str, Any]:
    rows = []
    for clip in CLIP_GRID:
        metrics = _metrics(initializer, trained, selection, clip)
        rows.append({"clip": clip, "metrics": metrics, "movement_safe": _movement_safe(metrics)})
    safe = [row for row in rows if row["movement_safe"]]
    selected = min(
        safe,
        key=lambda row: (row["metrics"]["overall"]["candidate_selected_index_nll"], row["clip"]),
        default=None,
    )
    if selected is None:
        return {"rows": rows, "selected_clip": None, "heldout": None, "gate": {"pass": False, "checks": {"safe_clip_exists": False}}}
    heldout_metrics = _metrics(initializer, trained, heldout, float(selected["clip"]))
    checks = {
        "safe_clip_exists": True,
        "heldout_movement_safe": _movement_safe(heldout_metrics),
        "heldout_relative_nll_improvement_at_least_0p05": heldout_metrics["overall"]["relative_nll_improvement"] >= HELDOUT_RELATIVE_NLL_IMPROVEMENT_MIN,
        "heldout_top1_delta_at_least_0p03": heldout_metrics["overall"]["top1_delta"] >= HELDOUT_TOP1_DELTA_MIN,
        "heldout_seat_0_nll_nonregression": heldout_metrics["by_candidate_seat"]["0"]["relative_nll_improvement"] >= 0.0,
        "heldout_seat_1_nll_nonregression": heldout_metrics["by_candidate_seat"]["1"]["relative_nll_improvement"] >= 0.0,
    }
    return {
        "rows": rows,
        "selected_clip": selected["clip"],
        "selection_metrics": selected["metrics"],
        "heldout": heldout_metrics,
        "gate": {"pass": all(checks.values()), "checks": checks},
    }


def _fixture_row(example: dict[str, Any]) -> dict[str, Any]:
    tensor = example["tensor"]
    acting_player = int(example["acting_player"])
    history = [
        entry["action_explicit_features"]
        + [float(int(entry["acting_player"]) == acting_player), float(int(entry["acting_player"]) != acting_player)]
        + entry["public_card_histogram"]
        for entry in example["history"]
    ]
    return {
        "state": torch.tensor(tensor["state"], dtype=torch.float32),
        "object_features": torch.tensor(tensor["object_features"], dtype=torch.float32),
        "object_card_ids": torch.tensor(tensor["object_card_ids"], dtype=torch.int64),
        "object_groups": torch.tensor(tensor["object_groups"], dtype=torch.int64),
        "edge_features": torch.tensor(tensor["edge_features"], dtype=torch.float32).reshape(-1, structured.EDGE_DIM),
        "edge_src": torch.tensor(tensor["edge_source_indices"], dtype=torch.int64),
        "edge_tgt": torch.tensor(tensor["edge_target_indices"], dtype=torch.int64),
        "action_features": torch.tensor(tensor["action_features"], dtype=torch.float32),
        "action_ref_features": torch.tensor(tensor["action_ref_features"], dtype=torch.float32).reshape(-1, structured.REF_DIM),
        "ref_card_ids": torch.tensor(tensor["action_ref_card_ids"], dtype=torch.int64),
        "ref_action_indices": torch.tensor(tensor["action_ref_action_indices"], dtype=torch.int64),
        "ref_node_indices": torch.tensor(tensor["action_ref_node_indices"], dtype=torch.int64),
        "candidate_seat": int(example["candidate_seat"]),
        "history_features": torch.tensor(history, dtype=torch.float32).reshape(-1, distill.HISTORY_FEATURE_DIM),
    }


def _parity(initializer: Any, trained: Any, clip: float, source_path: Path) -> dict[str, Any]:
    source = json.loads(source_path.read_text(encoding="utf-8"))
    examples = []
    initializer.eval()
    trained.eval()
    with torch.no_grad():
        for source_example in source.get("examples", []):
            row = _fixture_row(source_example)
            initial_logits, _ = initializer._one(row)
            trained_logits, _ = trained._one(row)
            example = dict(source_example)
            example["expected_structured_logits"] = _bounded_logits(initial_logits, trained_logits, clip).float().tolist()
            example["expected_value_residual_f32_bits"] = "00000000"
            examples.append(example)
    if len(examples) != 10:
        _fail("source parity fixture must contain ten examples")
    return {
        "schema": PARITY_SCHEMA,
        "output_semantics": "bounded-structured-logits-and-exact-parent-value/v1",
        "examples": examples,
    }


def _package(
    args: argparse.Namespace,
    initializer: Any,
    trained: Any,
    initializer_identity: dict[str, Any],
    source: dict[str, Any],
    label_source: dict[str, Any],
    screen: dict[str, Any],
    full_history: list[dict[str, Any]],
    full_metrics: dict[str, Any],
) -> dict[str, Any]:
    candidate_root = args.experiment_root / "candidate"
    parity_path = args.experiment_root / "candidate.parity.json"
    if candidate_root.exists() or parity_path.exists():
        _fail("candidate package output already exists")
    initial_payload, initial_parameters = publish._encoded_weights(initializer)
    trained_payload, trained_parameters = publish._encoded_weights(trained)
    parameters = []
    offset = 0
    for prefix, bindings in (("initializer", initial_parameters), ("trained", trained_parameters)):
        for binding in bindings:
            item = dict(binding)
            item["name"] = f"{prefix}.{binding['name']}"
            item["offset_f32"] = offset
            offset += int(item["count_f32"])
            parameters.append(item)
    payload = initial_payload + trained_payload
    if offset * 4 != len(payload):
        _fail("DAgger parameter layout mismatch")
    candidate_root.mkdir(parents=True)
    parent_output = candidate_root / "parent"
    parent_output.mkdir()
    parent_files = (
        ("checkpoint.json", publish.live.PARENT_MANIFEST_SHA256),
        ("checkpoint.state.f32le", publish.live.PARENT_PAYLOAD_SHA256),
    )
    for filename, expected_sha256 in parent_files:
        source_path = args.parent_outcome_root / filename
        if not source_path.is_file() or _sha256(source_path) != expected_sha256:
            _fail(f"retained parent hash mismatch: {source_path}")
        shutil.copyfile(source_path, parent_output / filename)
    weights_path = candidate_root / "weights.f32le"
    weights_path.write_bytes(payload)
    weights_sha256 = _sha256(weights_path)
    composite_sha256 = hashlib.sha256(
        COMPOSITE_DOMAIN
        + bytes.fromhex(publish.live.PARENT_MODEL_PARAMETER_SHA256)
        + payload
    ).hexdigest()
    report = {
        "schema": REPORT_SCHEMA,
        "initializer": initializer_identity,
        "source": {
            **source,
            **label_source,
            "base_seed": BASE_SEED,
            "source_commit": args.source_commit,
        },
        "split": {
            "train": "pair_index_mod_4_in_1_2",
            "selection": "pair_index_mod_4_eq_3",
            "heldout": "pair_index_mod_4_eq_0",
        },
        "config": {
            "architecture": ARCHITECTURE,
            "value_model": publish.VALUE_MODEL,
            "objective": OBJECTIVE,
            "projection_method": PROJECTION_METHOD,
            "logit_residual_clip": screen["selected_clip"],
            "seed": SEED,
            "epochs": EPOCHS,
            "batch_size_physical_decisions": BATCH_SIZE,
            "learning_rate": LR,
            "weight_decay": WEIGHT_DECAY,
            "gradient_norm_cap": GRAD_CAP,
            "history_length": distill.HISTORY_LENGTH,
            "history_feature_dim": distill.HISTORY_FEATURE_DIM,
        },
        "heldout_gate": screen["gate"],
        "heldout_metrics": screen["heldout"],
        "full_fit_training_history": full_history,
        "movement": full_metrics,
        "transport": {
            "maximum_absolute_logit_error": 1.0,
            "parent_value_bit_exact": False,
        },
        "weights_sha256": weights_sha256,
        "composite_model_parameter_sha256": composite_sha256,
    }
    report_path = candidate_root / "report.json"
    report_path.write_bytes(_json_bytes(report))
    candidate = {
        "schema": CANDIDATE_SCHEMA,
        "publication_encoding": "json-pretty-sorted-utf8-trailing-lf/v1",
        "parent": {
            "directory": "parent",
            "manifest_sha256": publish.live.PARENT_MANIFEST_SHA256,
            "payload_sha256": publish.live.PARENT_PAYLOAD_SHA256,
            "native_state_sha256": publish.live.PARENT_NATIVE_STATE_SHA256,
            "model_parameter_sha256": publish.live.PARENT_MODEL_PARAMETER_SHA256,
            "adam_step": publish.live.PARENT_ADAM_STEP,
        },
        "architecture": {
            "identity": ARCHITECTURE,
            "state_dim": structured.STATE_DIM,
            "object_dim": structured.OBJECT_DIM,
            "edge_dim": structured.EDGE_DIM,
            "action_dim": structured.ACTION_DIM,
            "ref_dim": structured.REF_DIM,
            "hidden_dim": distill.DIM,
            "card_vocab": distill.CARD_VOCAB,
            "card_embedding_dim": max(8, distill.DIM // 2),
            "group_vocab": distill.GROUP_VOCAB,
            "group_embedding_dim": max(8, distill.DIM // 3),
            "history_length": distill.HISTORY_LENGTH,
            "history_feature_dim": distill.HISTORY_FEATURE_DIM,
            "history_role_dim": 2,
            "value_model": publish.VALUE_MODEL,
        },
        "weights": {
            "filename": weights_path.name,
            "encoding": "ordered-row-major-finite-f32-little-endian/v1",
            "sha256": weights_sha256,
            "byte_count": len(payload),
            "parameter_count": offset,
            "parameters": parameters,
        },
        "report": {"filename": report_path.name, "sha256": _sha256(report_path)},
        "composite_model_parameter_sha256": composite_sha256,
    }
    candidate_path = candidate_root / publish.CANDIDATE_FILENAME
    candidate_path.write_bytes(_json_bytes(candidate))
    _write_new(parity_path, _parity(initializer, trained, float(screen["selected_clip"]), args.source_parity))
    return {
        "candidate_root": str(candidate_root),
        "candidate_json_sha256": _sha256(candidate_path),
        "report_sha256": _sha256(report_path),
        "weights_sha256": weights_sha256,
        "composite_model_parameter_sha256": composite_sha256,
        "parity_fixture": str(parity_path),
        "parity_fixture_sha256": _sha256(parity_path),
    }


def fit(args: argparse.Namespace) -> dict[str, Any]:
    if args.experiment_root.exists():
        _fail(f"experiment root already exists: {args.experiment_root}")
    args.experiment_root.mkdir(parents=True)
    started = time.perf_counter()
    structured._configure(SEED, args.threads)
    labels, label_source = _load_labels(args.corpus_report)
    decisions, source, timings = _load_decisions(args.history_cache, labels)
    loaded = time.perf_counter()
    train = [decision for decision in decisions if decision.pair_index % 4 in (1, 2)]
    selection = [decision for decision in decisions if decision.pair_index % 4 == 3]
    heldout = [decision for decision in decisions if decision.pair_index % 4 == 0]
    if not train or not selection or not heldout:
        _fail("DAgger split is empty")
    initializer, initializer_identity = _load_initializer(args.initializer_state, args.initializer_root)
    screen_model, _ = _load_initializer(args.initializer_state, args.initializer_root)
    screen_history = _fit(screen_model, train, SEED)
    screen_fit_done = time.perf_counter()
    screen = _screen(initializer, screen_model, selection, heldout)
    screened = time.perf_counter()
    full_model = None
    full_history = None
    full_metrics = None
    package = None
    if screen["gate"]["pass"]:
        full_model, _ = _load_initializer(args.initializer_state, args.initializer_root)
        full_history = _fit(full_model, decisions, SEED + 1)
        full_metrics = _metrics(initializer, full_model, decisions, float(screen["selected_clip"]))
        if not _movement_safe(full_metrics):
            screen["gate"]["checks"]["full_fit_movement_safe"] = False
            screen["gate"]["pass"] = False
        else:
            screen["gate"]["checks"]["full_fit_movement_safe"] = True
            package = _package(
                args,
                initializer,
                full_model,
                initializer_identity,
                source,
                label_source,
                screen,
                full_history,
                full_metrics,
            )
    finished = time.perf_counter()
    result = {
        "schema": FIT_SCHEMA,
        "decision": "PASS" if package is not None else "REJECT",
        "source": {**source, **label_source},
        "split": {
            "train_pairs": 128,
            "selection_pairs": 64,
            "heldout_pairs": 64,
            "train_decisions": len(train),
            "selection_decisions": len(selection),
            "heldout_decisions": len(heldout),
        },
        "config": {
            "objective": OBJECTIVE,
            "seed": SEED,
            "epochs": EPOCHS,
            "batch_size": BATCH_SIZE,
            "learning_rate": LR,
            "weight_decay": WEIGHT_DECAY,
            "gradient_norm_cap": GRAD_CAP,
            "threads": args.threads,
            "clip_grid": list(CLIP_GRID),
        },
        "screen_training_history": screen_history,
        "screen": screen,
        "full_fit_training_history": full_history,
        "full_fit_metrics": full_metrics,
        "package": package,
        "timings": {
            **timings,
            "load_total_seconds": loaded - started,
            "screen_fit_seconds": screen_fit_done - loaded,
            "screen_metrics_seconds": screened - screen_fit_done,
            "full_fit_package_seconds": finished - screened,
            "total_seconds": finished - started,
        },
        "non_claims": [
            "CP7 labels are supervision and not reward",
            "imitation diagnostics are not playing strength",
            "no promotion or pro-level claim without the fresh terminal gate",
        ],
    }
    _write_new(args.experiment_root / "fit.json", result)
    return result


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--corpus-report", type=Path, required=True)
    parser.add_argument("--history-cache", type=Path, required=True)
    parser.add_argument("--initializer-state", type=Path, required=True)
    parser.add_argument("--initializer-root", type=Path, required=True)
    parser.add_argument("--parent-outcome-root", type=Path, required=True)
    parser.add_argument("--source-parity", type=Path, required=True)
    parser.add_argument("--experiment-root", type=Path, required=True)
    parser.add_argument("--source-commit", required=True)
    parser.add_argument("--threads", type=int, default=6)
    args = parser.parse_args()
    for name in (
        "corpus_report",
        "history_cache",
        "initializer_state",
        "initializer_root",
        "parent_outcome_root",
        "source_parity",
    ):
        setattr(args, name, getattr(args, name).resolve(strict=True))
    args.experiment_root = args.experiment_root.resolve()
    if (
        not 1 <= args.threads <= 24
        or len(args.source_commit) != 40
        or any(character not in "0123456789abcdef" for character in args.source_commit)
        or args.source_commit != _git_head()
    ):
        _fail("invalid DAgger fit arguments")
    result = fit(args)
    print(json.dumps(result, sort_keys=True, allow_nan=False))
    return 0 if result["decision"] == "PASS" else 2


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, RuntimeError, ValueError) as error:
        print(f"error: {error}", file=sys.stderr)
        raise SystemExit(2)
