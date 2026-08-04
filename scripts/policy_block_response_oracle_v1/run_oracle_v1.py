#!/usr/bin/env python3
"""Run a direct-terminal CEM oracle over terminal-PPO policy blocks."""

from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
import math
from pathlib import Path
import shutil
import struct
import sys
import time
from typing import Any

import torch


SCRIPT_DIR = Path(__file__).resolve().parent
BASE_DIR = SCRIPT_DIR.parent / "policy_space_response_oracle_v1"
BASE_MODULE_NAME = "policy_space_response_oracle_v1_base"
BASE_SPEC = importlib.util.spec_from_file_location(
    BASE_MODULE_NAME, BASE_DIR / "run_oracle_v1.py"
)
if BASE_SPEC is None or BASE_SPEC.loader is None:
    raise RuntimeError("unable to load policy-space response-oracle base module")
base = importlib.util.module_from_spec(BASE_SPEC)
sys.modules[BASE_MODULE_NAME] = base
BASE_SPEC.loader.exec_module(base)


SEARCH_SCHEMA = "mtg-kernel-structured-policy-block-response-oracle-search/v1"
FIT_SCHEMA = "mtg-kernel-structured-policy-block-response-oracle-selected-fit/v1"
MODEL_STATE_SCHEMA = FIT_SCHEMA + ".model-state"
CANDIDATE_SCHEMA = "mtg-kernel-structured-policy-successor-candidate/v6"
REPORT_SCHEMA = "mtg-kernel-structured-policy-block-response-oracle-report/v1"
PARITY_SCHEMA = "mtg-kernel-structured-policy-block-response-oracle-parity-fixture/v1"
ARCHITECTURE = (
    "complete-public-history-structured-policy-block-response-oracle-"
    "frozen-parent-value/v1"
)
COMPOSITE_DOMAIN = (
    b"mtg-kernel-structured-policy-block-response-oracle-composite-model/v1"
)
ORACLE_SEED = 20_260_808
BLOCKS = (
    ("state", ("state.",)),
    ("history", ("history.", "history_mix.")),
    ("objects", ("object.", "card.", "group.")),
    ("relations", ("edge.", "group_mix.")),
    ("action", ("action.",)),
    ("references", ("ref.",)),
    ("query", ("query.",)),
    ("combine_input", ("combine.0.",)),
    ("combine_output", ("combine.2.",)),
    ("policy_head", ("policy_head.",)),
)
BLOCK_COUNT = len(BLOCKS)
COEFFICIENT_MAX = 0.125
COEFFICIENT_L1_BUDGET = 0.625
UNIFORM_SCALE = 1.0 / 16.0
POPULATION = 20
ELITES = 5
GENERATIONS = 6
DEVELOPMENT_PAIRS = 256
DEVELOPMENT_BASE_SEED = 1_800_001
DEVELOPMENT_SEED_STRIDE = 10_000
SELECTOR_PAIRS = 256
SELECTOR_SEEDS = (1_860_001, 1_870_001)
FRESH_BASE_SEED = 1_880_001
FRESH_PAIRS = 1_024
INITIAL_SIGMA = 0.03
MIN_SIGMA = 0.01
MAX_SIGMA = 0.05
SCORER = base.SCORER
SCORER_SHA256 = (
    "78917bdf01b07bb90ea97fe740e278fb26377a12f531dff8fae22fc27bb75b26"
)
TRAINED_STATE = Path(
    r"D:\mtg-kernel-policy-only-structured-terminal-rung-v1\formal\candidate.state.pt"
)
TRAINED_STATE_SHA256 = (
    "4d1e9853d3472eb8817c10051c5ff779258bc1fc26130e956492ad598c877fe9"
)
SOURCE_FIT_REPORT_SHA256 = (
    "355c1b179ccd5de5d16f0aeb39dc101ae97a876208a2315358f98b06dcc30a81"
)


def _fail(message: str) -> None:
    raise ValueError(message)


def _f32(value: float) -> float:
    return struct.unpack("<f", struct.pack("<f", value))[0]


def _f32_bits(value: float) -> str:
    return f"{struct.unpack('<I', struct.pack('<f', value))[0]:08x}"


def _l2(values: list[float]) -> float:
    return sum(float(value) ** 2 for value in values)


def _l1(values: list[float]) -> float:
    return sum(abs(float(value)) for value in values)


def _block_index(parameter_name: str) -> int:
    matches = [
        index
        for index, (_, prefixes) in enumerate(BLOCKS)
        if any(parameter_name.startswith(prefix) for prefix in prefixes)
    ]
    if len(matches) != 1:
        _fail(f"policy parameter does not map to exactly one block: {parameter_name}")
    return matches[0]


def _project_coefficients(values: list[float]) -> list[float]:
    if len(values) != BLOCK_COUNT or any(not math.isfinite(value) for value in values):
        _fail("invalid policy-block coefficient vector")
    projected = [max(0.0, min(COEFFICIENT_MAX, float(value))) for value in values]
    total = sum(projected)
    if total > COEFFICIENT_L1_BUDGET:
        scale = COEFFICIENT_L1_BUDGET / total
        projected = [value * scale for value in projected]
    return [_f32(value) for value in projected]


def _base_states() -> tuple[dict[str, Any], dict[str, Any], dict[str, Any]]:
    initial, identity = base._base_state()
    if base.pipeline._sha256(TRAINED_STATE) != TRAINED_STATE_SHA256:
        _fail("policy-block displacement source state mismatch")
    trained = torch.load(TRAINED_STATE, map_location="cpu", weights_only=False)
    if trained.get("schema") != base.pipeline.MODEL_STATE_SCHEMA:
        _fail("policy-block displacement source schema mismatch")
    before = initial["model_state_dict"]
    after = trained["model_state_dict"]
    if before.keys() != after.keys():
        _fail("policy-block state key mismatch")
    for name in before:
        if before[name].shape != after[name].shape or before[name].dtype != after[name].dtype:
            _fail(f"policy-block state tensor mismatch: {name}")
        if name.startswith("value_head."):
            if not torch.equal(before[name], after[name]):
                _fail("terminal PPO displacement changed the frozen value head")
        else:
            _block_index(name)
    return initial, trained, identity


def _model_with_coefficients(
    initial: dict[str, Any],
    trained: dict[str, Any],
    coefficients: list[float],
) -> Any:
    coefficients = _project_coefficients(coefficients)
    if _l1(coefficients) > COEFFICIENT_L1_BUDGET + 1.0e-6:
        _fail("policy-block coefficient budget exceeded")
    state = {}
    for name, before in initial["model_state_dict"].items():
        after = trained["model_state_dict"][name]
        if name.startswith("value_head."):
            state[name] = before.clone()
        else:
            scale = coefficients[_block_index(name)]
            state[name] = before + (after - before) * scale
    model = base.pipeline.distill._model()
    model.load_state_dict(state, strict=True)
    return model


def _package(
    root: Path,
    initial: dict[str, Any],
    trained: dict[str, Any],
    identity: dict[str, Any],
    coefficients: list[float],
    source_commit: str,
    phase: str,
    generation: int,
    candidate_index: int,
    movement: dict[str, Any] | None = None,
) -> dict[str, Any]:
    candidate_path = root / base.pipeline.CANDIDATE_FILENAME
    report_path = root / "report.json"
    weights_path = root / "weights.f32le"
    if root.exists():
        if not all(path.is_file() for path in (candidate_path, report_path, weights_path)):
            _fail(f"incomplete existing policy-block package: {root}")
        candidate = json.loads(candidate_path.read_text(encoding="utf-8"))
        report = json.loads(report_path.read_text(encoding="utf-8"))
        if candidate.get("schema") != CANDIDATE_SCHEMA or report.get("schema") != REPORT_SCHEMA:
            _fail(f"existing policy-block package schema mismatch: {root}")
        return {
            "root": str(root),
            "candidate_json_sha256": base.pipeline._sha256(candidate_path),
            "report_sha256": base.pipeline._sha256(report_path),
            "weights_sha256": base.pipeline._sha256(weights_path),
            "composite_model_parameter_sha256": candidate[
                "composite_model_parameter_sha256"
            ],
        }
    exact = _project_coefficients(coefficients)
    base._package(
        root,
        initial,
        identity,
        [0.0] * base.PARAMETER_COUNT,
        source_commit,
        phase,
        generation,
        candidate_index,
        movement,
    )
    model = _model_with_coefficients(initial, trained, exact)
    payload, _ = base.pipeline.initializer._encoded_weights(model)
    weights_path.write_bytes(payload)
    weights_sha256 = base.pipeline._sha256(weights_path)
    composite_sha256 = hashlib.sha256(
        COMPOSITE_DOMAIN
        + bytes.fromhex(base.pipeline.live.PARENT_MODEL_PARAMETER_SHA256)
        + payload
    ).hexdigest()
    development_only = phase != "selected"
    report = {
        "schema": REPORT_SCHEMA,
        "initializer": {
            "candidate_json_sha256": identity["candidate_json_sha256"],
            "report_sha256": identity["report_sha256"],
            "weights_sha256": identity["weights_sha256"],
            "composite_model_parameter_sha256": identity[
                "composite_model_parameter_sha256"
            ],
            "model_state_sha256": base.INITIALIZER_STATE_SHA256,
        },
        "source": {
            "source_commit": source_commit,
            "oracle_seed": ORACLE_SEED,
            "phase": phase,
            "generation": generation,
            "candidate_index": candidate_index,
            "rejected_fit_report_sha256": SOURCE_FIT_REPORT_SHA256,
            "rejected_model_state_sha256": TRAINED_STATE_SHA256,
        },
        "config": {
            "architecture": ARCHITECTURE,
            "value_model": base.pipeline.VALUE_MODEL,
            "optimizer": "terminal-outcome-projected-antithetic-cem/v1",
            "adapter": "semantic-terminal-ppo-policy-block-displacement/v1",
            "block_count": BLOCK_COUNT,
            "coefficient_maximum": COEFFICIENT_MAX,
            "coefficient_l1_budget": COEFFICIENT_L1_BUDGET,
            "development_only": development_only,
        },
        "adapter": {
            "block_names": [name for name, _ in BLOCKS],
            "coefficients_f32_bits": [_f32_bits(value) for value in exact],
            "coefficient_l1": _l1(exact),
            "coefficient_l2_squared": _l2(exact),
        },
        "movement": movement,
        "transport": {
            "maximum_absolute_logit_error": 1.0,
            "parent_value_bit_exact": False,
        },
        "weights_sha256": weights_sha256,
        "composite_model_parameter_sha256": composite_sha256,
    }
    report_path.write_bytes(base.pipeline.history_publish._json_bytes(report))
    candidate = json.loads(candidate_path.read_text(encoding="utf-8"))
    candidate["schema"] = CANDIDATE_SCHEMA
    candidate["architecture"]["identity"] = ARCHITECTURE
    candidate["weights"]["sha256"] = weights_sha256
    candidate["weights"]["byte_count"] = len(payload)
    candidate["report"]["sha256"] = base.pipeline._sha256(report_path)
    candidate["composite_model_parameter_sha256"] = composite_sha256
    candidate_path.write_bytes(base.pipeline.history_publish._json_bytes(candidate))
    return {
        "root": str(root),
        "candidate_json_sha256": base.pipeline._sha256(candidate_path),
        "report_sha256": base.pipeline._sha256(report_path),
        "weights_sha256": weights_sha256,
        "composite_model_parameter_sha256": composite_sha256,
    }


def _check_scorer() -> None:
    if base.pipeline._sha256(SCORER) != SCORER_SHA256:
        _fail("policy-block scorer SHA-256 mismatch")


def preflight(args: argparse.Namespace) -> dict[str, Any]:
    if args.output.exists():
        _fail("policy-block preflight output exists")
    _check_scorer()
    initial, trained, identity = _base_states()
    zero = [0.0] * BLOCK_COUNT
    uniform = [UNIFORM_SCALE] * BLOCK_COUNT
    concentrated = [COEFFICIENT_MAX if index % 2 == 0 else 0.0 for index in range(BLOCK_COUNT)]
    coefficients = [zero, zero, uniform, concentrated]
    zero_root = args.evidence_root / "packages" / "candidate-00"
    packages = []
    jobs = []
    for index, values in enumerate(coefficients):
        label = f"candidate-{index:02d}"
        root = zero_root if index == 1 else args.evidence_root / "packages" / label
        packages.append(
            _package(root, initial, trained, identity, values, args.source_commit, "development", 0, index)
        )
        jobs.append((label, root, args.base_seed, args.pairs))
    evaluations, wall = base._evaluate_many(args.evidence_root / "evaluations", jobs)
    repeat = evaluations[0]["outcome_sha256"] == evaluations[1]["outcome_sha256"]
    active = any(
        evaluation["trajectory_sha256"] != evaluations[0]["trajectory_sha256"]
        for evaluation in evaluations[2:]
    )
    result = {
        "schema": SEARCH_SCHEMA + ".preflight",
        "decision": "PASS" if repeat and active else "REJECT",
        "base_seed": args.base_seed,
        "pairs_per_candidate": args.pairs,
        "candidate_count": len(jobs),
        "games": len(jobs) * args.pairs * 2,
        "wall_seconds": wall,
        "aggregate_games_per_second": len(jobs) * args.pairs * 2 / wall,
        "zero_repeat_bit_identical": repeat,
        "fixed_probe_changes_trajectories": active,
        "packages": packages,
        "evaluations": evaluations,
    }
    base._write_new_json(args.output, result)
    return result


def search(args: argparse.Namespace) -> dict[str, Any]:
    if args.output.exists():
        _fail("policy-block search output exists")
    _check_scorer()
    initial, trained, identity = _base_states()
    rng = base.SplitMix64(ORACLE_SEED)
    zero = [0.0] * BLOCK_COUNT
    uniform = [_f32(UNIFORM_SCALE)] * BLOCK_COUNT
    mean = list(uniform)
    sigma = INITIAL_SIGMA
    generations = []
    generation_means = []
    started = time.perf_counter()
    for generation in range(GENERATIONS):
        candidates = [zero, uniform]
        for _ in range((POPULATION - 2) // 2):
            direction = [rng.normal_approx() for _ in range(BLOCK_COUNT)]
            candidates.append(
                _project_coefficients(
                    [center + sigma * value for center, value in zip(mean, direction)]
                )
            )
            candidates.append(
                _project_coefficients(
                    [center - sigma * value for center, value in zip(mean, direction)]
                )
            )
        generation_seed = DEVELOPMENT_BASE_SEED + generation * DEVELOPMENT_SEED_STRIDE
        jobs = []
        package_reports = []
        for index, values in enumerate(candidates):
            label = f"generation-{generation:02d}-candidate-{index:02d}"
            root = args.evidence_root / "packages" / label
            package_reports.append(
                _package(root, initial, trained, identity, values, args.source_commit, "development", generation, index)
            )
            jobs.append((label, root, generation_seed, DEVELOPMENT_PAIRS))
        evaluations, wall = base._evaluate_many(args.evidence_root / "evaluations", jobs)
        order = sorted(
            range(POPULATION),
            key=lambda index: (-evaluations[index]["fitness"], _l2(candidates[index]), index),
        )
        elites = order[:ELITES]
        rank_weights = list(range(ELITES, 0, -1))
        denominator = sum(rank_weights)
        new_mean = _project_coefficients(
            [
                sum(
                    weight * candidates[index][parameter]
                    for weight, index in zip(rank_weights, elites)
                )
                / denominator
                for parameter in range(BLOCK_COUNT)
            ]
        )
        variance = sum(
            weight
            * sum(
                (candidates[index][parameter] - new_mean[parameter]) ** 2
                for parameter in range(BLOCK_COUNT)
            )
            / BLOCK_COUNT
            for weight, index in zip(rank_weights, elites)
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
                    "coefficients": candidates[index],
                    "coefficient_l1": _l1(candidates[index]),
                    "coefficient_l2_squared": _l2(candidates[index]),
                    "package": package_reports[index],
                    "evaluation": evaluations[index],
                }
                for index in range(POPULATION)
            ],
        }
        generation_path = args.evidence_root / f"generation-{generation:02d}.json"
        base._write_new_json(generation_path, generation_report)
        generations.append(
            {
                "path": str(generation_path),
                "sha256": base.pipeline._sha256(generation_path),
                "best_fitness": evaluations[order[0]]["fitness"],
                "wall_seconds": wall,
            }
        )
        generation_means.append(new_mean)
        mean = new_mean
        sigma = new_sigma
        print(
            f"BLOCK_ORACLE generation={generation + 1}/{GENERATIONS} "
            f"best_fitness={evaluations[order[0]]['fitness']} sigma={sigma:.6f}",
            flush=True,
        )
    selector_coefficients = [zero, uniform, *generation_means]
    selector_jobs = []
    selector_packages = []
    for index, values in enumerate(selector_coefficients):
        root = args.evidence_root / "packages" / f"selector-{index:02d}"
        selector_packages.append(
            _package(root, initial, trained, identity, values, args.source_commit, "selector", index, index)
        )
        for panel, seed in enumerate(SELECTOR_SEEDS):
            selector_jobs.append((f"selector-{index:02d}-panel-{panel}", root, seed, SELECTOR_PAIRS))
    selector_evaluations, selector_wall = base._evaluate_many(
        args.evidence_root / "selector-evaluations", selector_jobs
    )
    selector_reports = []
    for index, values in enumerate(selector_coefficients):
        panels = selector_evaluations[index * 2 : index * 2 + 2]
        selector_reports.append(
            {
                "index": index,
                "coefficients": values,
                "coefficient_l1": _l1(values),
                "coefficient_l2_squared": _l2(values),
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
            -item["coefficient_l2_squared"],
            -item["index"],
        ),
    )
    result = {
        "schema": SEARCH_SCHEMA,
        "decision": "SELECTED",
        "source_commit": args.source_commit,
        "config": {
            "oracle_seed": ORACLE_SEED,
            "blocks": [name for name, _ in BLOCKS],
            "coefficient_maximum": COEFFICIENT_MAX,
            "coefficient_l1_budget": COEFFICIENT_L1_BUDGET,
            "population": POPULATION,
            "elites": ELITES,
            "generations": GENERATIONS,
            "development_pairs_per_candidate": DEVELOPMENT_PAIRS,
            "selector_pairs_per_panel": SELECTOR_PAIRS,
            "selector_seeds": list(SELECTOR_SEEDS),
            "fitness": "two-times-total-terminal-reward-plus-worse-seat-terminal-reward/v1",
        },
        "generations": generations,
        "selector_wall_seconds": selector_wall,
        "selector_reports": selector_reports,
        "selected_index": selected["index"],
        "selected_coefficients": selected["coefficients"],
        "selected_worst_fitness": selected["worst_fitness"],
        "selected_summed_fitness": selected["summed_fitness"],
        "runtime_seconds": time.perf_counter() - started,
    }
    base._write_new_json(args.output, result)
    return result


def finalize_selected(args: argparse.Namespace) -> dict[str, Any]:
    for path in (args.output, args.output_root, args.output_state, args.output_parity):
        if path.exists():
            _fail(f"selected policy-block output exists: {path}")
    if base.pipeline._sha256(base.SOURCE_CACHE) != base.SOURCE_CACHE_SHA256:
        _fail("policy-block movement cache mismatch")
    search_report = json.loads(args.search_report.read_text(encoding="utf-8"))
    if search_report.get("schema") != SEARCH_SCHEMA or search_report.get("decision") != "SELECTED":
        _fail("policy-block search result mismatch")
    coefficients = _project_coefficients(search_report["selected_coefficients"])
    initial, trained, identity = _base_states()
    started = time.perf_counter()
    decisions, source, timings = base.pipeline._load_decisions(base.SOURCE_CACHE, None)
    model = _model_with_coefficients(initial, trained, coefficients)
    alignment = base.pipeline._alignment(model, decisions)
    if not alignment["pass"] and all(value == 0.0 for value in coefficients):
        _fail("zero selected policy-block response does not align with initializer")
    moved = time.perf_counter()
    movement = base.pipeline._movement(model, decisions)
    gate = base.pipeline._fit_gate(movement)
    measured = time.perf_counter()
    result = {
        "schema": FIT_SCHEMA,
        "decision": gate["decision"],
        "source": source,
        "search_report": str(args.search_report),
        "search_report_sha256": base.pipeline._sha256(args.search_report),
        "selected_coefficients": coefficients,
        "movement": movement,
        "fit_gate": gate,
        "phase_runtime_seconds": {
            **timings,
            "load_model_seconds": moved - (started + sum(timings.values())),
            "movement_seconds": measured - moved,
        },
        "runtime_seconds": measured - started,
    }
    base.pipeline.screen._atomic_torch_save(
        {
            "schema": MODEL_STATE_SCHEMA,
            "source": source,
            "selected_coefficients": coefficients,
            "movement": movement,
            "fit_gate": gate,
            "model_state_dict": model.state_dict(),
        },
        args.output_state,
    )
    parity = base.pipeline.initializer._parity_fixture(model, decisions)
    parity["schema"] = PARITY_SCHEMA
    base._write_new_json(args.output_parity, parity)
    result["model_state"] = {
        "path": str(args.output_state),
        "sha256": base.pipeline._sha256(args.output_state),
    }
    result["parity_fixture"] = {
        "path": str(args.output_parity),
        "sha256": base.pipeline._sha256(args.output_parity),
    }
    if gate["decision"] == "PASS":
        result["publication"] = _package(
            args.output_root,
            initial,
            trained,
            identity,
            coefficients,
            args.source_commit,
            "selected",
            int(search_report["selected_index"]),
            int(search_report["selected_index"]),
            movement,
        )
    else:
        result["publication"] = "WITHHELD"
    base._write_new_json(args.output, result)
    return result


def seal_transport(args: argparse.Namespace) -> dict[str, Any]:
    if args.output.exists():
        _fail("policy-block transport result exists")
    if (
        not 0.0 <= args.maximum_absolute_logit_error <= base.pipeline.TRANSPORT_LIMIT
        or not args.parent_value_bit_exact
    ):
        _fail("policy-block native transport does not pass")
    candidate_path = args.root / base.pipeline.CANDIDATE_FILENAME
    report_path = args.root / "report.json"
    candidate = json.loads(candidate_path.read_text(encoding="utf-8"))
    report = json.loads(report_path.read_text(encoding="utf-8"))
    if (
        candidate.get("schema") != CANDIDATE_SCHEMA
        or report.get("schema") != REPORT_SCHEMA
        or report.get("config", {}).get("development_only") is not False
        or report.get("movement") is None
        or candidate.get("report", {}).get("sha256") != base.pipeline._sha256(report_path)
    ):
        _fail("selected policy-block package binding mismatch")
    candidate_backup = args.root.parent / f"{args.root.name}.pretransport.candidate.json"
    report_backup = args.root.parent / f"{args.root.name}.pretransport.report.json"
    if candidate_backup.exists() or report_backup.exists():
        _fail("policy-block transport backups exist")
    shutil.copyfile(candidate_path, candidate_backup)
    shutil.copyfile(report_path, report_backup)
    report["transport"] = {
        "maximum_absolute_logit_error": args.maximum_absolute_logit_error,
        "parent_value_bit_exact": True,
    }
    report_path.write_bytes(base.pipeline.history_publish._json_bytes(report))
    candidate["report"]["sha256"] = base.pipeline._sha256(report_path)
    candidate_path.write_bytes(base.pipeline.history_publish._json_bytes(candidate))
    result = {
        "schema": SEARCH_SCHEMA + ".transport",
        "decision": "PASS",
        "candidate_json_sha256": base.pipeline._sha256(candidate_path),
        "report_sha256": base.pipeline._sha256(report_path),
        "weights_sha256": base.pipeline._sha256(args.root / "weights.f32le"),
        "maximum_absolute_logit_error": args.maximum_absolute_logit_error,
        "parent_value_bit_exact": True,
    }
    base._write_new_json(args.output, result)
    return result


def _parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    subparsers = parser.add_subparsers(dest="command", required=True)
    pre = subparsers.add_parser("preflight")
    pre.add_argument("--evidence-root", type=Path, required=True)
    pre.add_argument("--output", type=Path, required=True)
    pre.add_argument("--source-commit", required=True)
    pre.add_argument("--base-seed", type=int, default=1_790_001)
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
