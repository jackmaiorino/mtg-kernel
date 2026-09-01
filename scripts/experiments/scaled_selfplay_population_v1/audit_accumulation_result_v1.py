#!/usr/bin/env python3
"""Narrow audit helpers for a completed ACCUMULATION result.

This tool does not make or change the frozen ACCUMULATION decision. It supports
two validation tasks required by the current project laws:

* rerun one revealed evaluator seed twice and require byte-identical outcomes;
* compute a labeled, post-hoc paired-cluster bootstrap from retained raw data.
"""

from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
import os
import subprocess
import sys
import time
from collections import Counter
from pathlib import Path
from typing import Any

import numpy as np


TEST_NAME = (
    "native_science_loop_v1::windows_science_loop_tests::"
    "ladder_head_to_head_eval_v1"
)
H2H_ENVIRONMENT = (
    "H2H_CANDIDATE_STORE_ROOT",
    "H2H_CANDIDATE_GEN",
    "H2H_CANDIDATE_USE_STORE_RUN",
    "H2H_CANDIDATE_BASE_SEED",
    "H2H_CANDIDATE_POOL_JSON",
    "H2H_UPDATES",
    "H2H_INIT_STORE",
    "H2H_INIT_GEN",
    "H2H_OPPONENT_STORE_ROOT",
    "H2H_OPPONENT_GEN",
    "H2H_PAIRS",
    "H2H_EVAL_SEED",
    "H2H_ENVIRONMENT_RANDOMIZATION_V2",
    "H2H_OUTCOME_JSON",
    "WIDE",
)


def require(condition: bool, message: str) -> None:
    if not condition:
        raise ValueError(message)


def no_duplicate_object(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
    value: dict[str, Any] = {}
    for key, item in pairs:
        require(key not in value, f"duplicate JSON key: {key}")
        value[key] = item
    return value


def load_json(path: Path) -> dict[str, Any]:
    with path.open("r", encoding="utf-8-sig") as stream:
        value = json.load(stream, object_pairs_hook=no_duplicate_object)
    require(type(value) is dict, f"JSON root must be an object: {path}")
    return value


def canonical_bytes(value: Any) -> bytes:
    return (
        json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(",", ":"))
        + "\n"
    ).encode("utf-8")


def write_new_json(path: Path, value: Any) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("xb") as stream:
        stream.write(canonical_bytes(value))


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def file_record(path: Path) -> dict[str, Any]:
    resolved = path.resolve()
    return {
        "path": str(resolved),
        "bytes": resolved.stat().st_size,
        "sha256": sha256_file(resolved),
    }


def validate_identity(
    observed: dict[str, Any], expected: dict[str, Any], label: str
) -> None:
    checks = {
        "run_sha256": "run_sha256",
        "generation": "generation",
        "checkpoint_manifest_sha256": "checkpoint_sha256",
        "checkpoint_payload_sha256": "state_sha256",
        "model_parameter_sha256": "model_parameter_sha256",
    }
    for observed_key, expected_key in checks.items():
        require(
            observed.get(observed_key) == expected.get(expected_key),
            f"{label} {observed_key} mismatch",
        )


def validate_outcome(
    path: Path,
    candidate: dict[str, Any],
    opponent: dict[str, Any],
    pair_count: int,
    evaluation_seed: int,
) -> None:
    outcome = load_json(path)
    require(
        outcome.get("schema") == "mtg-kernel-head-to-head-terminal-stream/v1",
        "outcome schema mismatch",
    )
    require(outcome.get("evaluation_base_seed") == evaluation_seed, "seed mismatch")
    require(outcome.get("pair_count") == pair_count, "pair count mismatch")
    require(outcome.get("episode_count") == pair_count * 2, "episode count mismatch")
    runtime = outcome.get("runtime", {})
    require(runtime.get("all_natural") is True, "outcome contains a non-natural game")
    require(
        runtime.get("environment_randomization_v2") is True,
        "environment randomization v2 was not active",
    )
    validate_identity(outcome.get("candidate", {}), candidate, "candidate")
    validate_identity(outcome.get("opponent", {}), opponent, "opponent")
    episodes = outcome.get("episodes")
    require(type(episodes) is list and len(episodes) == pair_count * 2, "bad episodes")
    for pair_index in range(pair_count):
        pair = episodes[pair_index * 2 : pair_index * 2 + 2]
        require(
            [row.get("learner_seat") for row in pair] == ["P0", "P1"],
            f"seat swap mismatch at pair {pair_index}",
        )
        require(
            len({row.get("environment_seed") for row in pair}) == 1,
            f"CRN seed mismatch at pair {pair_index}",
        )
        require(
            all(row.get("terminal_order_rank") in (-1, 0, 1) for row in pair),
            f"nonterminal rank at pair {pair_index}",
        )


def run_repeat(
    executable: Path,
    repo_root: Path,
    root: Path,
    label: str,
    candidate: dict[str, Any],
    opponent: dict[str, Any],
    pair_count: int,
    evaluation_seed: int,
) -> dict[str, Any]:
    run_root = root / label
    run_root.mkdir()
    outcome_path = run_root / "outcome.json"
    stdout_path = run_root / "stdout.log"
    stderr_path = run_root / "stderr.log"
    environment = os.environ.copy()
    for name in H2H_ENVIRONMENT:
        environment.pop(name, None)
    environment.update(
        {
            "H2H_CANDIDATE_STORE_ROOT": str(Path(candidate["store_root"]).resolve()),
            "H2H_CANDIDATE_GEN": str(candidate["generation"]),
            "H2H_CANDIDATE_USE_STORE_RUN": "1",
            "H2H_OPPONENT_STORE_ROOT": str(Path(opponent["store_root"]).resolve()),
            "H2H_OPPONENT_GEN": str(opponent["generation"]),
            "H2H_PAIRS": str(pair_count),
            "H2H_EVAL_SEED": str(evaluation_seed),
            "H2H_ENVIRONMENT_RANDOMIZATION_V2": "1",
            "H2H_OUTCOME_JSON": str(outcome_path.resolve()),
        }
    )
    command = [
        str(executable.resolve()),
        TEST_NAME,
        "--ignored",
        "--exact",
        "--nocapture",
        "--test-threads=1",
    ]
    started = time.perf_counter()
    with stdout_path.open("xb") as stdout, stderr_path.open("xb") as stderr:
        completed = subprocess.run(
            command,
            cwd=repo_root,
            env=environment,
            stdout=stdout,
            stderr=stderr,
            check=False,
        )
    wall_seconds = time.perf_counter() - started
    require(completed.returncode == 0, f"{label} exited {completed.returncode}")
    require(outcome_path.is_file(), f"{label} did not publish outcome.json")
    validate_outcome(outcome_path, candidate, opponent, pair_count, evaluation_seed)
    return {
        "label": label,
        "exit_code": completed.returncode,
        "wall_seconds": wall_seconds,
        "stdout": file_record(stdout_path),
        "stderr": file_record(stderr_path),
        "outcome": file_record(outcome_path),
    }


def determinism(args: argparse.Namespace) -> None:
    leg_spec_path = args.leg_spec.resolve()
    gate_plan_path = args.source_gate_plan.resolve()
    leg_spec = load_json(leg_spec_path)
    gate_plan = load_json(gate_plan_path)
    candidate = leg_spec["candidate"]
    opponent = leg_spec["fixed_opponent"]
    executable = Path(leg_spec["executable"]["path"]).resolve()
    require(executable.is_file(), "pinned evaluator executable is missing")
    require(
        sha256_file(executable) == leg_spec["executable"]["sha256"],
        "pinned evaluator executable hash mismatch",
    )
    require(
        gate_plan.get("executable", {}).get("sha256") == leg_spec["executable"]["sha256"],
        "source gate plan binds a different executable",
    )
    root = args.output_root.resolve()
    root.mkdir(parents=True, exist_ok=False)
    plan = {
        "schema": "accumulation-result-determinism-preflight-plan/v1",
        "disposition": "revealed-preflight-only; never part of model selection",
        "repo_root": str(args.repo_root.resolve()),
        "leg_spec": file_record(leg_spec_path),
        "source_gate_plan": file_record(gate_plan_path),
        "executable": {**file_record(executable), "source_commit": leg_spec["executable"]["source_commit"]},
        "toolchain": gate_plan["toolchain"],
        "candidate": candidate,
        "opponent": opponent,
        "pair_count": args.pair_count,
        "evaluation_seed": args.evaluation_seed,
        "repeat_count": 2,
        "test_name": TEST_NAME,
    }
    plan_path = root / "plan.json"
    write_new_json(plan_path, plan)
    repeats = [
        run_repeat(
            executable,
            args.repo_root.resolve(),
            root,
            label,
            candidate,
            opponent,
            args.pair_count,
            args.evaluation_seed,
        )
        for label in ("repeat-a", "repeat-b")
    ]
    bit_identical = repeats[0]["outcome"]["sha256"] == repeats[1]["outcome"]["sha256"]
    manifest = {
        "schema": "accumulation-result-determinism-preflight/v1",
        "passed": bit_identical,
        "disposition": "PASS" if bit_identical else "FAIL",
        "plan": file_record(plan_path),
        "repeats": repeats,
        "output_outcomes_bit_identical": bit_identical,
        "terminal_reward_only": True,
        "not_promotion_authority": True,
    }
    manifest_path = root / "manifest.json"
    write_new_json(manifest_path, manifest)
    print(manifest_path)
    require(bit_identical, "same-seed evaluator outcomes were not bit-identical")


def load_module(path: Path) -> Any:
    name = "accumulation_v1_analysis_for_result_audit"
    spec = importlib.util.spec_from_file_location(name, path)
    require(spec is not None and spec.loader is not None, "cannot load analyzer module")
    module = importlib.util.module_from_spec(spec)
    sys.modules[name] = module
    spec.loader.exec_module(module)
    return module


def interval(values: np.ndarray) -> dict[str, float]:
    low, high = np.quantile(values, [0.025, 0.975], method="linear")
    return {"lower": float(low), "upper": float(high)}


def bootstrap(args: argparse.Namespace) -> None:
    analyzer_path = args.analyzer.resolve()
    run_root = args.run_root.resolve()
    leg_spec_path = args.leg_spec.resolve()
    retained_path = args.retained_analysis.resolve()
    retained = load_json(retained_path)
    analyzer = load_module(analyzer_path)
    (
        _leg_spec,
        _reference,
        scores,
        leg_scores,
        _identifiers,
        _raw_authorities,
        _authorities,
    ) = analyzer.reconstruct(run_root, leg_spec_path, args.mode)
    decision_n = int(retained["decision_N"])
    require(0 < decision_n <= len(scores), "retained decision N is outside reconstructed data")
    decision_legs = leg_scores[:decision_n]
    observed_overall = float(sum((p0 + p1) / 2 for p0, p1 in decision_legs) / decision_n)
    require(
        abs(observed_overall - float(retained["delta_hat"])) <= 1e-15,
        "reconstructed observed delta differs from retained analysis",
    )
    joint = Counter((int(p0), int(p1)) for p0, p1 in decision_legs)
    categories = sorted(joint)
    category_counts = np.array([joint[item] for item in categories], dtype=np.int64)
    probabilities = category_counts.astype(np.float64) / decision_n
    rng = np.random.default_rng(args.bootstrap_seed)
    draws = rng.multinomial(decision_n, probabilities, size=args.replicates)
    p0_values = np.array([item[0] for item in categories], dtype=np.float64)
    p1_values = np.array([item[1] for item in categories], dtype=np.float64)
    overall_values = (p0_values + p1_values) / 2.0
    boot_overall = draws @ overall_values / decision_n
    boot_p0 = draws @ p0_values / decision_n
    boot_p1 = draws @ p1_values / decision_n
    observed_p0 = float(sum(p0 for p0, _p1 in decision_legs) / decision_n)
    observed_p1 = float(sum(p1 for _p0, p1 in decision_legs) / decision_n)
    result = {
        "schema": "accumulation-result-paired-bootstrap-audit/v1",
        "disposition": "descriptive-post-hoc-only; original frozen gate unchanged",
        "analyzer": file_record(analyzer_path),
        "leg_spec": file_record(leg_spec_path),
        "retained_analysis": file_record(retained_path),
        "run_root": str(run_root),
        "mode": args.mode,
        "decision": retained["decision"],
        "decision_N": decision_n,
        "bootstrap": {
            "unit": "matched CRN seat-swapped pair cluster",
            "method": "multinomial resampling of observed joint P0/P1 score categories",
            "replicates": args.replicates,
            "seed": args.bootstrap_seed,
            "quantile_method": "linear percentile 2.5/97.5",
            "joint_category_counts": [
                {"p0": p0, "p1": p1, "count": joint[(p0, p1)]}
                for p0, p1 in categories
            ],
        },
        "overall": {
            "observed_delta": observed_overall,
            "ci95": interval(boot_overall),
            "bootstrap_fraction_gt_zero": float(np.mean(boot_overall > 0.0)),
        },
        "P0": {
            "observed_delta": observed_p0,
            "ci95": interval(boot_p0),
            "bootstrap_fraction_gt_zero": float(np.mean(boot_p0 > 0.0)),
        },
        "P1": {
            "observed_delta": observed_p1,
            "ci95": interval(boot_p1),
            "bootstrap_fraction_gt_zero": float(np.mean(boot_p1 > 0.0)),
        },
        "both_observed_seat_deltas_nonnegative": observed_p0 >= 0.0 and observed_p1 >= 0.0,
        "terminal_reward_only": True,
        "not_promotion_authority": True,
    }
    write_new_json(args.output.resolve(), result)
    print(args.output.resolve())


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    subparsers = parser.add_subparsers(dest="command", required=True)

    determinism_parser = subparsers.add_parser("determinism")
    determinism_parser.add_argument("--repo-root", required=True, type=Path)
    determinism_parser.add_argument("--leg-spec", required=True, type=Path)
    determinism_parser.add_argument("--source-gate-plan", required=True, type=Path)
    determinism_parser.add_argument("--output-root", required=True, type=Path)
    determinism_parser.add_argument("--evaluation-seed", required=True, type=int)
    determinism_parser.add_argument("--pair-count", type=int, default=1)

    bootstrap_parser = subparsers.add_parser("bootstrap")
    bootstrap_parser.add_argument("--analyzer", required=True, type=Path)
    bootstrap_parser.add_argument("--run-root", required=True, type=Path)
    bootstrap_parser.add_argument("--leg-spec", required=True, type=Path)
    bootstrap_parser.add_argument("--retained-analysis", required=True, type=Path)
    bootstrap_parser.add_argument(
        "--mode", choices=("initial", "confirmation"), required=True
    )
    bootstrap_parser.add_argument("--bootstrap-seed", required=True, type=int)
    bootstrap_parser.add_argument("--replicates", type=int, default=200_000)
    bootstrap_parser.add_argument("--output", required=True, type=Path)

    args = parser.parse_args()
    if args.command == "determinism":
        require(args.pair_count > 0, "pair count must be positive")
        determinism(args)
    else:
        require(args.replicates > 0, "replicate count must be positive")
        bootstrap(args)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
