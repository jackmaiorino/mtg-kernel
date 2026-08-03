#!/usr/bin/env python3
"""Run a direct-terminal CEM response oracle on the structured policy head."""

from __future__ import annotations

import argparse
import concurrent.futures
import hashlib
import json
import math
from pathlib import Path
import shutil
import struct
import sys
import time
from types import SimpleNamespace
from typing import Any

import torch


SCRIPT_DIR = Path(__file__).resolve().parent
SCRIPTS_DIR = SCRIPT_DIR.parent
TERMINAL_DIR = SCRIPTS_DIR / "policy_only_structured_terminal_rung_v1"
STRUCTURED_DIR = SCRIPTS_DIR / "structured_adapter_screen_v1"
POPULATION_DIR = SCRIPTS_DIR / "native_population_structured_v1"
QUALIFICATION_DIR = SCRIPTS_DIR / "policy_only_structured_successor_v1"
for directory in (TERMINAL_DIR, STRUCTURED_DIR, POPULATION_DIR, QUALIFICATION_DIR):
    sys.path.insert(0, str(directory))

import collect_corpus_v1 as collector  # noqa: E402
import run_matched_gate_v1 as qualification  # noqa: E402
import run_pipeline_v1 as pipeline  # noqa: E402


SEARCH_SCHEMA = "mtg-kernel-structured-policy-space-response-oracle-search/v1"
FIT_SCHEMA = "mtg-kernel-structured-policy-space-response-oracle-selected-fit/v1"
MODEL_STATE_SCHEMA = FIT_SCHEMA + ".model-state"
CANDIDATE_SCHEMA = "mtg-kernel-structured-policy-successor-candidate/v5"
REPORT_SCHEMA = "mtg-kernel-structured-policy-space-response-oracle-report/v1"
PARITY_SCHEMA = "mtg-kernel-structured-policy-space-response-oracle-parity-fixture/v1"
ARCHITECTURE = (
    "complete-public-history-structured-policy-space-response-oracle-"
    "frozen-parent-value/v1"
)
COMPOSITE_DOMAIN = (
    b"mtg-kernel-structured-policy-space-response-oracle-composite-model/v1"
)
ORACLE_SEED = 20_260_807
PARAMETER_COUNT = 48
MAX_ABS_DELTA = 0.05
POPULATION = 20
ELITES = 5
GENERATIONS = 6
DEVELOPMENT_PAIRS = 64
DEVELOPMENT_BASE_SEED = 1_710_001
DEVELOPMENT_SEED_STRIDE = 10_000
SELECTOR_PAIRS = 128
SELECTOR_SEEDS = (1_770_001, 1_780_001)
FRESH_BASE_SEED = 1_790_001
FRESH_PAIRS = 1_024
INITIAL_SIGMA = 0.01
MIN_SIGMA = 0.003
MAX_SIGMA = 0.02
MAX_WORKERS = 20
SCORER = qualification.SCORER
SCORER_SHA256 = (
    "3c1ea778f793fba867e78632d505fa9bd9197585cd42c046d6bc0451b7b18a5e"
)
POOL_ROOT = qualification.POOL_ROOT
INITIALIZER_ROOT = Path(r"D:\mtg-kernel-policy-only-structured-successor-v1\candidate")
INITIALIZER_STATE = Path(r"D:\mtg-kernel-policy-only-structured-successor-v1\candidate.state.pt")
INITIALIZER_STATE_SHA256 = pipeline.INITIALIZER_STATE_SHA256
PARENT_OUTCOME_ROOT = Path(
    r"D:\mtg-kernel-scaled-history-outcome-policy-v1\live-candidate\parent"
)
SOURCE_CACHE = Path(
    r"D:\mtg-kernel-policy-only-structured-terminal-rung-v1\formal\cache.pt"
)
SOURCE_CACHE_SHA256 = (
    "454e4ce1b8f7413839a36c8e2731fc0cb65581ce13e593634bffa70013a6f16d"
)


def _fail(message: str) -> None:
    raise ValueError(message)


def _write_new_json(path: Path, value: Any) -> None:
    pipeline._write_new_json(path, value)


def _f32(value: float) -> float:
    return struct.unpack("<f", struct.pack("<f", value))[0]


def _f32_bits(value: float) -> str:
    return f"{struct.unpack('<I', struct.pack('<f', value))[0]:08x}"


def _l2(delta: list[float]) -> float:
    return sum(float(value) ** 2 for value in delta)


class SplitMix64:
    def __init__(self, state: int):
        self.state = state & ((1 << 64) - 1)

    def next_u64(self) -> int:
        self.state = (self.state + 0x9E3779B97F4A7C15) & ((1 << 64) - 1)
        value = self.state
        value = ((value ^ (value >> 30)) * 0xBF58476D1CE4E5B9) & ((1 << 64) - 1)
        value = ((value ^ (value >> 27)) * 0x94D049BB133111EB) & ((1 << 64) - 1)
        return value ^ (value >> 31)

    def normal_approx(self) -> float:
        return sum(self.next_u64() / float(1 << 64) for _ in range(12)) - 6.0


def _base_state() -> tuple[dict[str, Any], dict[str, Any]]:
    if pipeline._sha256(INITIALIZER_STATE) != INITIALIZER_STATE_SHA256:
        _fail("response-oracle initializer state mismatch")
    payload = torch.load(INITIALIZER_STATE, map_location="cpu", weights_only=False)
    if payload.get("schema") != pipeline.initializer.MODEL_STATE_SCHEMA:
        _fail("response-oracle initializer state schema mismatch")
    identity = qualification._candidate_identity(INITIALIZER_ROOT)
    return payload, identity


def _model_with_delta(state_payload: dict[str, Any], delta: list[float]) -> Any:
    if len(delta) != PARAMETER_COUNT or any(
        not math.isfinite(value) or abs(value) > MAX_ABS_DELTA + 1.0e-9
        for value in delta
    ):
        _fail("response-oracle adapter is outside its fixed bounds")
    model = pipeline.distill._model()
    model.load_state_dict(state_payload["model_state_dict"], strict=True)
    frozen_before = {
        name: tensor.detach().cpu().contiguous().numpy().tobytes()
        for name, tensor in model.state_dict().items()
        if name != "policy_head.weight"
    }
    exact = torch.tensor([_f32(value) for value in delta], dtype=torch.float32)
    with torch.no_grad():
        model.policy_head.weight.add_(exact.reshape(1, -1))
    frozen_after = {
        name: tensor.detach().cpu().contiguous().numpy().tobytes()
        for name, tensor in model.state_dict().items()
        if name != "policy_head.weight"
    }
    if frozen_after != frozen_before:
        _fail("response-oracle package changed a frozen tensor")
    return model


def _package(
    root: Path,
    state_payload: dict[str, Any],
    identity: dict[str, Any],
    delta: list[float],
    source_commit: str,
    phase: str,
    generation: int,
    candidate_index: int,
    movement: dict[str, Any] | None = None,
) -> dict[str, Any]:
    if root.exists():
        candidate_path = root / pipeline.CANDIDATE_FILENAME
        report_path = root / "report.json"
        weights_path = root / "weights.f32le"
        if not all(path.is_file() for path in (candidate_path, report_path, weights_path)):
            _fail(f"incomplete existing response-oracle package: {root}")
        return {
            "root": str(root),
            "candidate_json_sha256": pipeline._sha256(candidate_path),
            "report_sha256": pipeline._sha256(report_path),
            "weights_sha256": pipeline._sha256(weights_path),
        }
    development_only = phase != "selected"
    model = _model_with_delta(state_payload, delta)
    payload, parameters = pipeline.initializer._encoded_weights(model)
    root.mkdir(parents=True)
    parent_output = root / "parent"
    parent_output.mkdir()
    parent_manifest = PARENT_OUTCOME_ROOT / "checkpoint.json"
    parent_payload = PARENT_OUTCOME_ROOT / "checkpoint.state.f32le"
    if (
        pipeline._sha256(parent_manifest) != pipeline.live.PARENT_MANIFEST_SHA256
        or pipeline._sha256(parent_payload) != pipeline.live.PARENT_PAYLOAD_SHA256
    ):
        _fail("response-oracle retained parent mismatch")
    shutil.copyfile(parent_manifest, parent_output / parent_manifest.name)
    shutil.copyfile(parent_payload, parent_output / parent_payload.name)
    weights_path = root / "weights.f32le"
    weights_path.write_bytes(payload)
    weights_sha256 = pipeline._sha256(weights_path)
    composite_sha256 = hashlib.sha256(
        COMPOSITE_DOMAIN
        + bytes.fromhex(pipeline.live.PARENT_MODEL_PARAMETER_SHA256)
        + payload
    ).hexdigest()
    exact_delta = [_f32(value) for value in delta]
    report = {
        "schema": REPORT_SCHEMA,
        "initializer": {
            "candidate_json_sha256": identity["candidate_json_sha256"],
            "report_sha256": identity["report_sha256"],
            "weights_sha256": identity["weights_sha256"],
            "composite_model_parameter_sha256": identity[
                "composite_model_parameter_sha256"
            ],
            "model_state_sha256": INITIALIZER_STATE_SHA256,
        },
        "source": {
            "source_commit": source_commit,
            "oracle_seed": ORACLE_SEED,
            "phase": phase,
            "generation": generation,
            "candidate_index": candidate_index,
        },
        "config": {
            "architecture": ARCHITECTURE,
            "value_model": pipeline.VALUE_MODEL,
            "optimizer": "terminal-outcome-antithetic-cem/v1",
            "adapter_parameter": "policy_head.weight",
            "adapter_parameter_count": PARAMETER_COUNT,
            "adapter_maximum_absolute_delta": MAX_ABS_DELTA,
            "development_only": development_only,
        },
        "adapter": {
            "delta_f32_bits": [_f32_bits(value) for value in exact_delta],
            "l2_squared": _l2(exact_delta),
        },
        "movement": movement,
        "transport": {
            "maximum_absolute_logit_error": 1.0,
            "parent_value_bit_exact": False,
        },
        "weights_sha256": weights_sha256,
        "composite_model_parameter_sha256": composite_sha256,
    }
    report_path = root / "report.json"
    report_path.write_bytes(pipeline.history_publish._json_bytes(report))
    report_sha256 = pipeline._sha256(report_path)
    candidate = {
        "schema": CANDIDATE_SCHEMA,
        "publication_encoding": "json-pretty-sorted-utf8-trailing-lf/v1",
        "parent": {
            "directory": "parent",
            "manifest_sha256": pipeline.live.PARENT_MANIFEST_SHA256,
            "payload_sha256": pipeline.live.PARENT_PAYLOAD_SHA256,
            "native_state_sha256": pipeline.live.PARENT_NATIVE_STATE_SHA256,
            "model_parameter_sha256": pipeline.live.PARENT_MODEL_PARAMETER_SHA256,
            "adam_step": pipeline.live.PARENT_ADAM_STEP,
        },
        "architecture": {
            "identity": ARCHITECTURE,
            "state_dim": pipeline.screen.STATE_DIM,
            "object_dim": pipeline.screen.OBJECT_DIM,
            "edge_dim": pipeline.screen.EDGE_DIM,
            "action_dim": pipeline.screen.ACTION_DIM,
            "ref_dim": pipeline.screen.REF_DIM,
            "hidden_dim": pipeline.distill.DIM,
            "card_vocab": pipeline.distill.CARD_VOCAB,
            "card_embedding_dim": max(8, pipeline.distill.DIM // 2),
            "group_vocab": pipeline.distill.GROUP_VOCAB,
            "group_embedding_dim": max(8, pipeline.distill.DIM // 3),
            "history_length": pipeline.distill.HISTORY_LENGTH,
            "history_feature_dim": pipeline.distill.HISTORY_FEATURE_DIM,
            "history_role_dim": 2,
            "value_model": pipeline.VALUE_MODEL,
        },
        "weights": {
            "filename": weights_path.name,
            "encoding": "ordered-row-major-finite-f32-little-endian/v1",
            "sha256": weights_sha256,
            "byte_count": len(payload),
            "parameter_count": pipeline.history_publish.EXPECTED_PARAMETER_COUNT,
            "parameters": parameters,
        },
        "report": {"filename": report_path.name, "sha256": report_sha256},
        "composite_model_parameter_sha256": composite_sha256,
    }
    candidate_path = root / pipeline.CANDIDATE_FILENAME
    candidate_path.write_bytes(pipeline.history_publish._json_bytes(candidate))
    return {
        "root": str(root),
        "candidate_json_sha256": pipeline._sha256(candidate_path),
        "report_sha256": report_sha256,
        "weights_sha256": weights_sha256,
        "composite_model_parameter_sha256": composite_sha256,
    }


def _panel_metrics(outcome_path: Path, base_seed: int, pairs: int) -> dict[str, Any]:
    _, panel = qualification._load_panel(outcome_path, base_seed, pairs)
    seat_reward = {"p0": 0, "p1": 0}
    wins = losses = draws = 0
    for (_, _, seat), row in panel.items():
        reward = int(row["candidate_terminal_reward"])
        seat_reward[seat] += reward
        wins += int(reward > 0)
        losses += int(reward < 0)
        draws += int(reward == 0)
    total_reward = seat_reward["p0"] + seat_reward["p1"]
    return {
        "wins": wins,
        "losses": losses,
        "draws": draws,
        "terminal_reward_sum": total_reward,
        "terminal_reward_sum_by_candidate_seat": seat_reward,
        "fitness": 2 * total_reward + min(seat_reward.values()),
    }


def _evaluate_one(
    evidence_root: Path,
    label: str,
    candidate_root: Path,
    base_seed: int,
    pairs: int,
) -> dict[str, Any]:
    root = evidence_root / label
    root.mkdir(parents=True, exist_ok=True)
    for attempt in range(100):
        prefix = root / f"attempt-{attempt:02d}"
        report_path = prefix.with_suffix(".collection.json")
        teacher_path = prefix.with_suffix(".teacher.jsonl")
        outcome_path = prefix.with_suffix(".outcome.jsonl")
        if report_path.is_file():
            report = json.loads(report_path.read_text(encoding="utf-8"))
            if teacher_path.is_file() and outcome_path.is_file():
                break
            _fail(f"completed oracle attempt is incomplete: {prefix}")
        if any(path.exists() for path in (teacher_path, outcome_path)):
            continue
        args = SimpleNamespace(
            scorer=SCORER,
            candidate_root=candidate_root,
            pool_root=POOL_ROOT,
            teacher_jsonl=teacher_path,
            outcome_jsonl=outcome_path,
            output=report_path,
            base_seed=base_seed,
            pair_start=0,
            pairs=pairs,
        )
        report = collector.collect(args)
        break
    else:
        _fail(f"no available oracle attempt for {label}")
    metrics = _panel_metrics(outcome_path, base_seed, pairs)
    return {
        "label": label,
        "candidate_root": str(candidate_root),
        "base_seed": base_seed,
        "pairs": pairs,
        "games": pairs * 2,
        "elapsed_seconds": report["elapsed_seconds"],
        "startup_seconds": report["startup_seconds"],
        "outcome_jsonl": str(outcome_path),
        "outcome_sha256": pipeline._sha256(outcome_path),
        "teacher_jsonl": str(teacher_path),
        "teacher_sha256": pipeline._sha256(teacher_path),
        "collection_report": str(report_path),
        "collection_report_sha256": pipeline._sha256(report_path),
        **metrics,
    }


def _evaluate_many(
    evidence_root: Path,
    jobs: list[tuple[str, Path, int, int]],
) -> tuple[list[dict[str, Any]], float]:
    started = time.perf_counter()
    results: dict[str, dict[str, Any]] = {}
    with concurrent.futures.ThreadPoolExecutor(
        max_workers=min(MAX_WORKERS, len(jobs))
    ) as executor:
        futures = {
            executor.submit(_evaluate_one, evidence_root, *job): job[0]
            for job in jobs
        }
        for future in concurrent.futures.as_completed(futures):
            result = future.result()
            results[result["label"]] = result
    return [results[job[0]] for job in jobs], time.perf_counter() - started


def preflight(args: argparse.Namespace) -> dict[str, Any]:
    if args.output.exists():
        _fail("response-oracle preflight output already exists")
    if pipeline._sha256(SCORER) != SCORER_SHA256:
        _fail("response-oracle scorer SHA-256 mismatch")
    state_payload, identity = _base_state()
    zero = [0.0] * PARAMETER_COUNT
    probe = [_f32(MAX_ABS_DELTA if index % 2 == 0 else -MAX_ABS_DELTA) for index in range(PARAMETER_COUNT)]
    deltas = [zero, zero, probe, [-value for value in probe]]
    jobs = []
    packages = []
    for index, delta in enumerate(deltas):
        label = f"candidate-{index:02d}"
        root = args.evidence_root / "packages" / label
        packages.append(
            _package(root, state_payload, identity, delta, args.source_commit, "development", 0, index)
        )
        jobs.append((label, root, args.base_seed, args.pairs))
    evaluations, wall = _evaluate_many(args.evidence_root / "evaluations", jobs)
    zero_repeat = evaluations[0]["outcome_sha256"] == evaluations[1]["outcome_sha256"]
    probe_active = any(
        evaluation["outcome_sha256"] != evaluations[0]["outcome_sha256"]
        for evaluation in evaluations[2:]
    )
    result = {
        "schema": SEARCH_SCHEMA + ".preflight",
        "decision": "PASS" if zero_repeat and probe_active else "REJECT",
        "base_seed": args.base_seed,
        "pairs_per_candidate": args.pairs,
        "candidate_count": len(jobs),
        "games": len(jobs) * args.pairs * 2,
        "wall_seconds": wall,
        "aggregate_games_per_second": len(jobs) * args.pairs * 2 / wall,
        "projected_formal_search_wall_seconds": wall * GENERATIONS,
        "zero_repeat_bit_identical": zero_repeat,
        "fixed_probe_changes_outcomes": probe_active,
        "packages": packages,
        "evaluations": evaluations,
    }
    _write_new_json(args.output, result)
    return result


def _clamp_delta(values: list[float]) -> list[float]:
    return [_f32(max(-MAX_ABS_DELTA, min(MAX_ABS_DELTA, value))) for value in values]


def search(args: argparse.Namespace) -> dict[str, Any]:
    if args.output.exists():
        _fail("response-oracle search output already exists")
    if pipeline._sha256(SCORER) != SCORER_SHA256:
        _fail("response-oracle scorer SHA-256 mismatch")
    state_payload, identity = _base_state()
    rng = SplitMix64(ORACLE_SEED)
    mean = [0.0] * PARAMETER_COUNT
    sigma = INITIAL_SIGMA
    generations = []
    generation_means = []
    started = time.perf_counter()
    for generation in range(GENERATIONS):
        candidates = []
        for _ in range(POPULATION // 2):
            direction = [rng.normal_approx() for _ in range(PARAMETER_COUNT)]
            candidates.append(
                _clamp_delta([center + sigma * value for center, value in zip(mean, direction)])
            )
            candidates.append(
                _clamp_delta([center - sigma * value for center, value in zip(mean, direction)])
            )
        generation_seed = DEVELOPMENT_BASE_SEED + generation * DEVELOPMENT_SEED_STRIDE
        jobs = []
        package_reports = []
        for index, delta in enumerate(candidates):
            label = f"generation-{generation:02d}-candidate-{index:02d}"
            root = args.evidence_root / "packages" / label
            package_reports.append(
                _package(root, state_payload, identity, delta, args.source_commit, "development", generation, index)
            )
            jobs.append((label, root, generation_seed, DEVELOPMENT_PAIRS))
        evaluations, wall = _evaluate_many(args.evidence_root / "evaluations", jobs)
        order = sorted(
            range(POPULATION),
            key=lambda index: (-evaluations[index]["fitness"], _l2(candidates[index]), index),
        )
        elites = order[:ELITES]
        rank_weights = list(range(ELITES, 0, -1))
        denominator = sum(rank_weights)
        new_mean = [
            _f32(
                sum(
                    rank_weight * candidates[index][parameter]
                    for rank_weight, index in zip(rank_weights, elites)
                )
                / denominator
            )
            for parameter in range(PARAMETER_COUNT)
        ]
        variance = sum(
            rank_weight
            * sum(
                (candidates[index][parameter] - new_mean[parameter]) ** 2
                for parameter in range(PARAMETER_COUNT)
            )
            / PARAMETER_COUNT
            for rank_weight, index in zip(rank_weights, elites)
        ) / denominator
        new_sigma = max(MIN_SIGMA, min(MAX_SIGMA, math.sqrt(max(variance, 0.0))))
        generation_report = {
            "generation": generation,
            "base_seed": generation_seed,
            "sigma_before": sigma,
            "sigma_after": new_sigma,
            "wall_seconds": wall,
            "elite_indices": elites,
            "mean": new_mean,
            "candidates": [
                {
                    "index": index,
                    "delta": candidates[index],
                    "l2_squared": _l2(candidates[index]),
                    "package": package_reports[index],
                    "evaluation": evaluations[index],
                }
                for index in range(POPULATION)
            ],
        }
        generation_path = args.evidence_root / f"generation-{generation:02d}.json"
        _write_new_json(generation_path, generation_report)
        generations.append(
            {
                "path": str(generation_path),
                "sha256": pipeline._sha256(generation_path),
                "best_fitness": evaluations[order[0]]["fitness"],
                "wall_seconds": wall,
            }
        )
        generation_means.append(new_mean)
        mean = new_mean
        sigma = new_sigma
        print(
            f"ORACLE generation={generation + 1}/{GENERATIONS} "
            f"best_fitness={evaluations[order[0]]['fitness']} sigma={sigma:.6f}",
            flush=True,
        )
    selector_deltas = [[0.0] * PARAMETER_COUNT, *generation_means]
    selector_jobs = []
    selector_packages = []
    for index, delta in enumerate(selector_deltas):
        root = args.evidence_root / "packages" / f"selector-{index:02d}"
        selector_packages.append(
            _package(root, state_payload, identity, delta, args.source_commit, "selector", index, index)
        )
        for panel, seed in enumerate(SELECTOR_SEEDS):
            selector_jobs.append((f"selector-{index:02d}-panel-{panel}", root, seed, SELECTOR_PAIRS))
    selector_evaluations, selector_wall = _evaluate_many(
        args.evidence_root / "selector-evaluations", selector_jobs
    )
    selector_reports = []
    for index, delta in enumerate(selector_deltas):
        panels = selector_evaluations[index * len(SELECTOR_SEEDS) : (index + 1) * len(SELECTOR_SEEDS)]
        selector_reports.append(
            {
                "index": index,
                "delta": delta,
                "l2_squared": _l2(delta),
                "package": selector_packages[index],
                "panels": panels,
                "worst_fitness": min(panel["fitness"] for panel in panels),
                "summed_fitness": sum(panel["fitness"] for panel in panels),
            }
        )
    selected = max(
        selector_reports,
        key=lambda item: (
            item["worst_fitness"],
            item["summed_fitness"],
            -item["l2_squared"],
            -item["index"],
        ),
    )
    result = {
        "schema": SEARCH_SCHEMA,
        "decision": "SELECTED",
        "source_commit": args.source_commit,
        "config": {
            "oracle_seed": ORACLE_SEED,
            "parameter_count": PARAMETER_COUNT,
            "maximum_absolute_delta": MAX_ABS_DELTA,
            "population": POPULATION,
            "elites": ELITES,
            "generations": GENERATIONS,
            "development_pairs_per_candidate": DEVELOPMENT_PAIRS,
            "selector_pairs_per_panel": SELECTOR_PAIRS,
            "selector_seeds": list(SELECTOR_SEEDS),
            "initial_sigma": INITIAL_SIGMA,
            "minimum_sigma": MIN_SIGMA,
            "maximum_sigma": MAX_SIGMA,
            "fitness": "two-times-total-terminal-reward-plus-worse-seat-terminal-reward/v1",
        },
        "generations": generations,
        "selector_wall_seconds": selector_wall,
        "selector_reports": selector_reports,
        "selected_index": selected["index"],
        "selected_delta": selected["delta"],
        "selected_worst_fitness": selected["worst_fitness"],
        "selected_summed_fitness": selected["summed_fitness"],
        "runtime_seconds": time.perf_counter() - started,
    }
    _write_new_json(args.output, result)
    return result


def finalize_selected(args: argparse.Namespace) -> dict[str, Any]:
    for path in (args.output, args.output_root, args.output_state, args.output_parity):
        if path.exists():
            _fail(f"selected response-oracle output exists: {path}")
    if pipeline._sha256(SOURCE_CACHE) != SOURCE_CACHE_SHA256:
        _fail("response-oracle movement cache mismatch")
    search_report = json.loads(args.search_report.read_text(encoding="utf-8"))
    if search_report.get("schema") != SEARCH_SCHEMA or search_report.get("decision") != "SELECTED":
        _fail("response-oracle search result mismatch")
    delta = [_f32(value) for value in search_report["selected_delta"]]
    state_payload, identity = _base_state()
    started = time.perf_counter()
    decisions, source, timings = pipeline._load_decisions(SOURCE_CACHE, None)
    model = _model_with_delta(state_payload, delta)
    alignment = pipeline._alignment(model, decisions)
    if not alignment["pass"] and all(value == 0.0 for value in delta):
        _fail("zero selected response does not align with initializer")
    moved = time.perf_counter()
    movement = pipeline._movement(model, decisions)
    gate = pipeline._fit_gate(movement)
    measured = time.perf_counter()
    result = {
        "schema": FIT_SCHEMA,
        "decision": gate["decision"],
        "source": source,
        "search_report": str(args.search_report),
        "search_report_sha256": pipeline._sha256(args.search_report),
        "selected_delta": delta,
        "movement": movement,
        "fit_gate": gate,
        "phase_runtime_seconds": {
            **timings,
            "load_model_seconds": moved - (started + sum(timings.values())),
            "movement_seconds": measured - moved,
        },
        "runtime_seconds": measured - started,
    }
    pipeline.screen._atomic_torch_save(
        {
            "schema": MODEL_STATE_SCHEMA,
            "source": source,
            "selected_delta": delta,
            "movement": movement,
            "fit_gate": gate,
            "model_state_dict": model.state_dict(),
        },
        args.output_state,
    )
    parity = pipeline.initializer._parity_fixture(model, decisions)
    parity["schema"] = PARITY_SCHEMA
    _write_new_json(args.output_parity, parity)
    result["model_state"] = {
        "path": str(args.output_state),
        "sha256": pipeline._sha256(args.output_state),
    }
    result["parity_fixture"] = {
        "path": str(args.output_parity),
        "sha256": pipeline._sha256(args.output_parity),
    }
    if gate["decision"] == "PASS":
        result["publication"] = _package(
            args.output_root,
            state_payload,
            identity,
            delta,
            args.source_commit,
            "selected",
            int(search_report["selected_index"]),
            int(search_report["selected_index"]),
            movement,
        )
    else:
        result["publication"] = "WITHHELD"
    _write_new_json(args.output, result)
    return result


def seal_transport(args: argparse.Namespace) -> dict[str, Any]:
    if args.output.exists():
        _fail("response-oracle transport result exists")
    if (
        not 0.0 <= args.maximum_absolute_logit_error <= pipeline.TRANSPORT_LIMIT
        or not args.parent_value_bit_exact
    ):
        _fail("response-oracle native transport does not pass")
    candidate_path = args.root / pipeline.CANDIDATE_FILENAME
    report_path = args.root / "report.json"
    candidate = json.loads(candidate_path.read_text(encoding="utf-8"))
    report = json.loads(report_path.read_text(encoding="utf-8"))
    if (
        candidate.get("schema") != CANDIDATE_SCHEMA
        or report.get("schema") != REPORT_SCHEMA
        or report.get("config", {}).get("development_only") is not False
        or report.get("movement") is None
        or candidate.get("report", {}).get("sha256") != pipeline._sha256(report_path)
    ):
        _fail("selected response-oracle package binding mismatch")
    candidate_backup = args.root.parent / f"{args.root.name}.pretransport.candidate.json"
    report_backup = args.root.parent / f"{args.root.name}.pretransport.report.json"
    if candidate_backup.exists() or report_backup.exists():
        _fail("response-oracle transport backups exist")
    shutil.copyfile(candidate_path, candidate_backup)
    shutil.copyfile(report_path, report_backup)
    report["transport"] = {
        "maximum_absolute_logit_error": args.maximum_absolute_logit_error,
        "parent_value_bit_exact": True,
    }
    report_path.write_bytes(pipeline.history_publish._json_bytes(report))
    candidate["report"]["sha256"] = pipeline._sha256(report_path)
    candidate_path.write_bytes(pipeline.history_publish._json_bytes(candidate))
    result = {
        "schema": SEARCH_SCHEMA + ".transport",
        "decision": "PASS",
        "candidate_json_sha256": pipeline._sha256(candidate_path),
        "report_sha256": pipeline._sha256(report_path),
        "weights_sha256": pipeline._sha256(args.root / "weights.f32le"),
        "maximum_absolute_logit_error": args.maximum_absolute_logit_error,
        "parent_value_bit_exact": True,
    }
    _write_new_json(args.output, result)
    return result


def _parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    subparsers = parser.add_subparsers(dest="command", required=True)
    pre = subparsers.add_parser("preflight")
    pre.add_argument("--evidence-root", type=Path, required=True)
    pre.add_argument("--output", type=Path, required=True)
    pre.add_argument("--source-commit", required=True)
    pre.add_argument("--base-seed", type=int, default=1_700_001)
    pre.add_argument("--pairs", type=int, default=16)
    formal = subparsers.add_parser("search")
    formal.add_argument("--evidence-root", type=Path, required=True)
    formal.add_argument("--output", type=Path, required=True)
    formal.add_argument("--source-commit", required=True)
    selected = subparsers.add_parser("finalize-selected")
    selected.add_argument("--search-report", type=Path, required=True)
    selected.add_argument("--source-commit", required=True)
    selected.add_argument("--output-root", type=Path, required=True)
    selected.add_argument("--output-state", type=Path, required=True)
    selected.add_argument("--output-parity", type=Path, required=True)
    selected.add_argument("--output", type=Path, required=True)
    seal = subparsers.add_parser("seal-transport")
    seal.add_argument("--root", type=Path, required=True)
    seal.add_argument("--maximum-absolute-logit-error", type=float, required=True)
    seal.add_argument("--parent-value-bit-exact", action="store_true")
    seal.add_argument("--output", type=Path, required=True)
    return parser


def main() -> int:
    args = _parser().parse_args()
    if args.command == "preflight":
        result = preflight(args)
    elif args.command == "search":
        result = search(args)
    elif args.command == "finalize-selected":
        result = finalize_selected(args)
    else:
        result = seal_transport(args)
    print(json.dumps(result, sort_keys=True, allow_nan=False))
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, RuntimeError, ValueError) as error:
        print(f"error: {error}", file=sys.stderr)
        raise SystemExit(2)
