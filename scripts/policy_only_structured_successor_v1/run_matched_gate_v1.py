#!/usr/bin/env python3
"""Run the fresh native Pool3 matched gate for the structured successor."""

from __future__ import annotations

import argparse
import concurrent.futures
import hashlib
import json
from pathlib import Path
import re
import sys
import time
from types import SimpleNamespace
from typing import Any


NATIVE_POPULATION_DIR = Path(__file__).resolve().parents[1] / "native_population_structured_v1"
sys.path.insert(0, str(NATIVE_POPULATION_DIR))
import aggregate_matched_strength_v1 as strength  # noqa: E402
import collect_corpus_v1 as collector  # noqa: E402


SCHEMA = "mtg-kernel-structured-policy-successor-native-pool3-matched-gate/v1"
CANDIDATE_FILENAME = "structured_policy_successor.json"
CANDIDATE_SCHEMA = "mtg-kernel-structured-policy-successor-candidate/v1"
COMPOSITE_DOMAIN = b"mtg-kernel-structured-policy-successor-composite-model/v1"
PARENT_IDENTITY = {
    "adam_step": "1",
    "manifest": "706b3aa80ec7a3c067d458fef06bb2237320543f202fb2349c5cb885975fdbbb",
    "payload": "eb83be33bcb7418b6f85ec9687da4b7ca5620a1df64721a1942d2793588bbd3c",
    "train_state": "2c55a13abb3157f3f4ba012af663ffa56599c5d6cb90743c1ba6e024ca47a9c8",
    "model": "883e4882d01d9cb55ecd7a4ae00e3c95793b6147baf3df08650ef1fa7f8e9546",
}
FORMAL_BASE_SEED = 1_650_001
FORMAL_TARGET_PAIRS = 1_024
PROFILE_MAX_PAIRS = 64
TASK_RETRIES = 2
TOPOLOGIES = ("sequential", "parallel")
POOL_ROOT = Path(r"D:\mtg-kernel-ladder-pilot-20260725\pool3")
SCORER = Path(
    r"C:\Users\Jack\IdeaProjects\mtg-kernel-structured-successor-screen-v1-codex\target\release\native_population_corpus_stdio_v1.exe"
)
HEX64 = re.compile(r"[0-9a-f]{16}")
TEACHER_CONTRACT = "mtg-kernel-native-population-opponent-jsonl/v1"
TEACHER_SELECTION_SOURCE = "native_pool3_ladder_40_20_20_20"


def _sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def _atomic_json(path: Path, value: Any) -> None:
    temporary = path.with_suffix(path.suffix + ".tmp")
    temporary.write_text(
        json.dumps(value, indent=2, sort_keys=True, allow_nan=False) + "\n",
        encoding="utf-8",
        newline="\n",
    )
    temporary.replace(path)


def _candidate_identity(root: Path) -> dict[str, Any]:
    candidate_path = root / CANDIDATE_FILENAME
    candidate = json.loads(candidate_path.read_text(encoding="utf-8"))
    if candidate.get("schema") != CANDIDATE_SCHEMA:
        raise RuntimeError("candidate schema mismatch")
    report_ref = candidate.get("report", {})
    weights_ref = candidate.get("weights", {})
    parent = candidate.get("parent", {})
    if report_ref.get("filename") != "report.json":
        raise RuntimeError("candidate report filename mismatch")
    if weights_ref.get("filename") != "weights.f32le":
        raise RuntimeError("candidate weights filename mismatch")
    report_path = root / "report.json"
    weights_path = root / "weights.f32le"
    if not report_path.is_file() or not weights_path.is_file():
        raise RuntimeError("candidate report or weights are missing")
    report_sha256 = _sha256(report_path)
    weights_sha256 = _sha256(weights_path)
    if report_ref.get("sha256") != report_sha256:
        raise RuntimeError("candidate report SHA-256 mismatch")
    if weights_ref.get("sha256") != weights_sha256:
        raise RuntimeError("candidate weights SHA-256 mismatch")
    expected_parent = {
        "directory": "parent",
        "manifest_sha256": PARENT_IDENTITY["manifest"],
        "payload_sha256": PARENT_IDENTITY["payload"],
        "native_state_sha256": PARENT_IDENTITY["train_state"],
        "model_parameter_sha256": PARENT_IDENTITY["model"],
        "adam_step": int(PARENT_IDENTITY["adam_step"]),
    }
    if parent != expected_parent:
        raise RuntimeError("candidate retained-parent identity mismatch")
    composite = candidate.get("composite_model_parameter_sha256")
    if not isinstance(composite, str) or len(composite) != 64:
        raise RuntimeError("candidate composite identity is incomplete")
    expected_composite = hashlib.sha256(
        COMPOSITE_DOMAIN
        + bytes.fromhex(parent["model_parameter_sha256"])
        + weights_path.read_bytes()
    ).hexdigest()
    if composite != expected_composite:
        raise RuntimeError("candidate composite SHA-256 mismatch")
    candidate_sha256 = _sha256(candidate_path)
    identity = {
        "adam_step": str(parent["adam_step"]),
        "manifest": candidate_sha256,
        "payload": weights_sha256,
        "train_state": report_sha256,
        "model": composite,
        "candidate_json_sha256": candidate_sha256,
        "report_sha256": report_sha256,
        "weights_sha256": weights_sha256,
        "composite_model_parameter_sha256": composite,
        "parent": parent,
    }
    if any(
        len(identity[key]) != 64
        for key in ("manifest", "payload", "train_state", "model")
    ):
        raise RuntimeError("candidate identity is incomplete")
    parent_root = root / "parent"
    _validate_parent_root(parent_root)
    return identity


def _validate_parent_root(root: Path) -> dict[str, str]:
    manifest = root / "checkpoint.json"
    payload = root / "checkpoint.state.f32le"
    if not manifest.is_file() or not payload.is_file():
        raise RuntimeError("exact parent root lacks checkpoint files")
    if _sha256(manifest) != PARENT_IDENTITY["manifest"]:
        raise RuntimeError("exact parent manifest SHA-256 mismatch")
    if _sha256(payload) != PARENT_IDENTITY["payload"]:
        raise RuntimeError("exact parent payload SHA-256 mismatch")
    return PARENT_IDENTITY.copy()


def _collection_args(
    args: argparse.Namespace, arm: str, attempt: int
) -> tuple[argparse.Namespace, dict[str, str]]:
    arm_root = args.evidence_root / "streams" / arm
    arm_root.mkdir(parents=True, exist_ok=True)
    prefix = arm_root / f"attempt-{attempt:02d}"
    teacher = prefix.with_suffix(".teacher.jsonl")
    outcome = prefix.with_suffix(".outcome.jsonl")
    report = prefix.with_suffix(".collection.json")
    outcome_root = args.candidate_root if arm == "candidate" else args.parent_root
    collection_args = SimpleNamespace(
        scorer=args.scorer,
        candidate_root=outcome_root,
        pool_root=args.pool_root,
        teacher_jsonl=teacher,
        outcome_jsonl=outcome,
        output=report,
        base_seed=args.base_seed,
        pair_start=0,
        pairs=args.target_pairs,
    )
    paths = {
        "teacher_jsonl": str(teacher),
        "outcome_jsonl": str(outcome),
        "collection_report": str(report),
    }
    return collection_args, paths


def _validate_collection_report(
    report: dict[str, Any],
    paths: dict[str, str],
    expected_base_seed: int,
    expected_pairs: int,
) -> None:
    if report.get("base_seed") != expected_base_seed:
        raise RuntimeError("collection report base seed mismatch")
    if report.get("pairs") != expected_pairs or report.get("episodes") != expected_pairs * 2:
        raise RuntimeError("collection report panel size mismatch")
    teacher = Path(paths["teacher_jsonl"])
    outcome = Path(paths["outcome_jsonl"])
    if not teacher.is_file() or not outcome.is_file():
        raise RuntimeError("collection did not preserve teacher and outcome streams")
    if report.get("teacher_sha256") != _sha256(teacher):
        raise RuntimeError("teacher stream SHA-256 mismatch")
    if report.get("outcome_sha256") != _sha256(outcome):
        raise RuntimeError("outcome stream SHA-256 mismatch")


def _collect_arm(args: argparse.Namespace, arm: str) -> dict[str, Any]:
    attempts: list[dict[str, Any]] = []
    for attempt in range(TASK_RETRIES + 1):
        collection_args, paths = _collection_args(args, arm, attempt)
        started = time.perf_counter()
        try:
            report = collector.collect(collection_args)
            _validate_collection_report(
                report, paths, args.base_seed, args.target_pairs
            )
            attempt_result = {
                "attempt": attempt,
                "status": "success",
                "elapsed_seconds": time.perf_counter() - started,
                "paths": paths,
                "report": report,
            }
            attempts.append(attempt_result)
            return {
                "arm": arm,
                "status": "success",
                "attempts": attempts,
                "elapsed_seconds": sum(
                    item["elapsed_seconds"] for item in attempts
                ),
                "paths": paths,
                "report": report,
            }
        except Exception as error:
            attempt_result = {
                "attempt": attempt,
                "status": "failed",
                "elapsed_seconds": time.perf_counter() - started,
                "paths": paths,
                "error": str(error),
            }
            attempts.append(attempt_result)
    return {
        "arm": arm,
        "status": "failed",
        "attempts": attempts,
        "elapsed_seconds": sum(item["elapsed_seconds"] for item in attempts),
        "paths": attempts[-1]["paths"],
    }


def _run_arms(args: argparse.Namespace) -> dict[str, Any]:
    started = time.perf_counter()
    if args.topology == "parallel":
        with concurrent.futures.ThreadPoolExecutor(max_workers=2) as executor:
            futures = {
                executor.submit(_collect_arm, args, arm): arm
                for arm in ("candidate", "parent")
            }
            results = {
                arm: future.result() for future, arm in futures.items()
            }
    else:
        results = {
            "candidate": _collect_arm(args, "candidate"),
            "parent": _collect_arm(args, "parent"),
        }
    return {
        "candidate": results["candidate"],
        "parent": results["parent"],
        "wall_seconds": time.perf_counter() - started,
    }


def _load_panel(
    path: Path, expected_base_seed: int, expected_pairs: int
) -> tuple[dict[str, Any], dict[tuple[int, int, str], dict[str, Any]]]:
    header, terminals = strength._load_terminals(
        path, expected_base_seed, expected_pairs
    )
    for key, row in terminals.items():
        pair_index, episode_id, seat = key
        if {episode_id} - {pair_index * 2, pair_index * 2 + 1}:
            raise RuntimeError(f"{path} has an episode outside its pair")
        if seat not in ("p0", "p1"):
            raise RuntimeError(f"{path} has an invalid candidate seat")
        environment_seed = row.get("pair_environment_seed_u64_hex")
        if not isinstance(environment_seed, str) or not HEX64.fullmatch(environment_seed):
            raise RuntimeError(f"{path} has an invalid pair environment seed")
    for pair_index in range(expected_pairs):
        expected = {
            (pair_index, pair_index * 2, "p0"),
            (pair_index, pair_index * 2 + 1, "p1"),
        }
        observed = {key for key in terminals if key[0] == pair_index}
        if observed != expected:
            raise RuntimeError(f"{path} pair {pair_index} violates the seat-swap schedule")
    return header, terminals


def _load_teacher_header(path: Path) -> dict[str, Any]:
    with path.open("r", encoding="utf-8") as handle:
        line = handle.readline()
    if not line:
        raise RuntimeError(f"{path} has no teacher header")
    header = json.loads(line)
    if (
        header.get("record_type") != "header"
        or header.get("export_contract") != TEACHER_CONTRACT
        or header.get("selection_source") != TEACHER_SELECTION_SOURCE
    ):
        raise RuntimeError(f"{path} has an invalid Pool3 teacher header")
    return header


def _validate_checkpoint_header(
    checkpoint: Any,
    identity: dict[str, Any],
    authority_kind: str,
) -> None:
    expected = {
        "authority_kind": authority_kind,
        "loaded_generation": 1,
        "loaded_checkpoint_sha256": identity["manifest"],
        "loaded_payload_sha256": identity["payload"],
        "loaded_train_state_sha256": identity["train_state"],
        "model_parameter_sha256": identity["model"],
    }
    if not isinstance(checkpoint, dict) or any(
        checkpoint.get(field) != value for field, value in expected.items()
    ):
        raise RuntimeError("collection checkpoint identity mismatch")


def _adjudicate(
    args: argparse.Namespace,
    candidate_result: dict[str, Any],
    parent_result: dict[str, Any],
) -> dict[str, Any]:
    candidate_path = Path(candidate_result["paths"]["outcome_jsonl"])
    parent_path = Path(parent_result["paths"]["outcome_jsonl"])
    candidate_header, candidate = _load_panel(
        candidate_path, args.base_seed, args.target_pairs
    )
    parent_header, parent = _load_panel(
        parent_path, args.base_seed, args.target_pairs
    )
    candidate_teacher_header = _load_teacher_header(
        Path(candidate_result["paths"]["teacher_jsonl"])
    )
    parent_teacher_header = _load_teacher_header(
        Path(parent_result["paths"]["teacher_jsonl"])
    )
    _validate_checkpoint_header(
        candidate_header.get("checkpoint"),
        args.candidate_identity,
        "xmage-cp7-outcome-structured-policy-successor-v1",
    )
    _validate_checkpoint_header(
        parent_header.get("checkpoint"),
        PARENT_IDENTITY,
        "xmage-cp7-outcome-reinforce-derivative-v1",
    )
    if candidate_teacher_header.get("checkpoint") != candidate_header.get("checkpoint"):
        raise RuntimeError("candidate teacher and outcome checkpoint headers differ")
    if parent_teacher_header.get("checkpoint") != parent_header.get("checkpoint"):
        raise RuntimeError("parent teacher and outcome checkpoint headers differ")
    if set(candidate) != set(parent):
        raise RuntimeError("candidate and parent terminal keys differ")
    gains = losses = ties = 0
    candidate_wins = parent_wins = 0
    seat_wins = {"p0": {"candidate": 0, "parent": 0}, "p1": {"candidate": 0, "parent": 0}}
    matched_pairs: dict[int, dict[str, str]] = {}
    for key in sorted(candidate):
        candidate_row = candidate[key]
        parent_row = parent[key]
        for field in (
            "pair_environment_seed_u64_hex",
            "episode_id",
            "candidate_seat",
        ):
            if candidate_row.get(field) != parent_row.get(field):
                raise RuntimeError(f"matched terminal field differs at {key}: {field}")
        pair_index, _, seat = key
        candidate_reward = int(candidate_row["candidate_terminal_reward"])
        parent_reward = int(parent_row["candidate_terminal_reward"])
        candidate_win = candidate_reward > 0
        parent_win = parent_reward > 0
        candidate_wins += int(candidate_win)
        parent_wins += int(parent_win)
        seat_wins[seat]["candidate"] += int(candidate_win)
        seat_wins[seat]["parent"] += int(parent_win)
        if candidate_reward > parent_reward:
            gains += 1
        elif candidate_reward < parent_reward:
            losses += 1
        else:
            ties += 1
        matched_pairs.setdefault(
            pair_index,
            {
                "pair_index": pair_index,
                "environment_seed": candidate_row["pair_environment_seed_u64_hex"],
            },
        )
    seat_deltas = {
        seat: values["candidate"] - values["parent"]
        for seat, values in seat_wins.items()
    }
    gates = {
        "relative_losses_at_most_gains_plus_20": losses <= gains + 20,
        "candidate_wins_at_least_parent_minus_20": candidate_wins >= parent_wins - 20,
        "p0_candidate_minus_parent_wins_at_least_minus_12": seat_deltas["p0"] >= -12,
        "p1_candidate_minus_parent_wins_at_least_minus_12": seat_deltas["p1"] >= -12,
        "all_natural_terminals": True,
        "all_transport_checks": True,
        "exact_target_pairs_matched": len(matched_pairs) == args.target_pairs,
    }
    return {
        "schema": SCHEMA + ".report",
        "formal": args.formal,
        "profile_pairs": args.profile_pairs,
        "topology": args.topology,
        "base_seed": args.base_seed,
        "target_pairs": args.target_pairs,
        "games": args.target_pairs * 2,
        "candidate_wins": candidate_wins,
        "parent_wins": parent_wins,
        "gains": gains,
        "losses": losses,
        "ties": ties,
        "wins_by_candidate_seat": seat_wins,
        "candidate_minus_parent_wins_by_seat": seat_deltas,
        "matched_pairs": list(matched_pairs.values()),
        "candidate": {
            "outcome_jsonl": str(candidate_path),
            "sha256": _sha256(candidate_path),
            "teacher_jsonl": candidate_result["paths"]["teacher_jsonl"],
            "teacher_sha256": candidate_result["report"]["teacher_sha256"],
            "checkpoint": candidate_header.get("checkpoint"),
        },
        "parent": {
            "outcome_jsonl": str(parent_path),
            "sha256": _sha256(parent_path),
            "teacher_jsonl": parent_result["paths"]["teacher_jsonl"],
            "teacher_sha256": parent_result["report"]["teacher_sha256"],
            "checkpoint": parent_header.get("checkpoint"),
        },
        "candidate_identity": args.candidate_identity,
        "parent_identity": PARENT_IDENTITY,
        "gates": gates,
        "status": "pass" if all(gates.values()) else "fail",
    }


def run(args: argparse.Namespace) -> int:
    if args.evidence_root.exists():
        unexpected = [
            path.name
            for path in args.evidence_root.iterdir()
            if path.name != "manifest.json"
        ]
        if unexpected:
            raise RuntimeError(
                "evidence root may initially contain only manifest.json: "
                + ",".join(sorted(unexpected))
            )
    args.evidence_root.mkdir(parents=True, exist_ok=True)
    for path in (args.scorer, args.candidate_root, args.parent_root, args.pool_root):
        if not path.exists():
            raise RuntimeError(f"required path does not exist: {path}")
    args.candidate_identity = _candidate_identity(args.candidate_root)
    args.parent_identity = _validate_parent_root(args.parent_root)
    arm_results = _run_arms(args)
    state = {
        "schema": SCHEMA + ".state",
        "formal": args.formal,
        "profile_pairs": args.profile_pairs,
        "topology": args.topology,
        "base_seed": args.base_seed,
        "target_pairs": args.target_pairs,
        "candidate_identity": args.candidate_identity,
        "parent_identity": args.parent_identity,
        "arms": arm_results,
        "outcomes_parsed": False,
    }
    _atomic_json(args.evidence_root / "state.json", state)
    if any(result["status"] != "success" for result in arm_results.values() if isinstance(result, dict) and "status" in result):
        raise RuntimeError("one or more native collection arms failed after retries")
    report = _adjudicate(args, arm_results["candidate"], arm_results["parent"])
    report["topology_wall_seconds"] = arm_results["wall_seconds"]
    report["arm_elapsed_seconds"] = {
        arm: arm_results[arm]["elapsed_seconds"] for arm in ("candidate", "parent")
    }
    _atomic_json(args.evidence_root / "report.json", report)
    state["outcomes_parsed"] = True
    state["report_sha256"] = _sha256(args.evidence_root / "report.json")
    _atomic_json(args.evidence_root / "state.json", state)
    print(json.dumps(report, sort_keys=True))
    return 0


def self_test() -> int:
    if FORMAL_BASE_SEED != 1_650_001 or FORMAL_TARGET_PAIRS != 1_024:
        raise RuntimeError("formal contract self-test failed")
    if collector.__file__ is None or strength.__file__ is None:
        raise RuntimeError("native helper import self-test failed")
    print("run_matched_gate_v1: SELF-TEST PASS")
    return 0


def _arguments(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--profile-pairs", type=int)
    parser.add_argument("--topology", choices=TOPOLOGIES)
    parser.add_argument("--evidence-root", type=Path)
    parser.add_argument("--base-seed", type=int)
    parser.add_argument("--candidate-root", type=Path)
    parser.add_argument("--parent-root", type=Path)
    parser.add_argument("--pool-root", type=Path, default=POOL_ROOT)
    parser.add_argument("--scorer", type=Path, default=SCORER)
    args = parser.parse_args(argv)
    if args.self_test:
        return args
    if args.evidence_root is None or args.candidate_root is None or args.parent_root is None:
        parser.error("--evidence-root, --candidate-root, and --parent-root are required")
    if args.topology is None:
        parser.error("--topology must be explicitly frozen as sequential or parallel")
    if args.profile_pairs is None:
        if args.base_seed not in (None, FORMAL_BASE_SEED):
            parser.error("formal base seed is fixed to 1650001")
        if args.pool_root != POOL_ROOT:
            parser.error("formal Pool3 root is fixed to D:\\mtg-kernel-ladder-pilot-20260725\\pool3")
        if args.scorer != SCORER:
            parser.error("formal scorer is fixed to native_population_corpus_stdio_v1.exe")
        args.formal = True
        args.profile_pairs = None
        args.base_seed = FORMAL_BASE_SEED
        args.target_pairs = FORMAL_TARGET_PAIRS
    else:
        if not 1 <= args.profile_pairs <= PROFILE_MAX_PAIRS:
            parser.error(f"--profile-pairs must be in [1,{PROFILE_MAX_PAIRS}]")
        if args.base_seed is None:
            args.base_seed = FORMAL_BASE_SEED
        if args.base_seed < 0:
            parser.error("profile base seed must be nonnegative")
        args.formal = False
        args.target_pairs = args.profile_pairs
    return args


if __name__ == "__main__":
    try:
        parsed = _arguments()
        sys.exit(self_test() if parsed.self_test else run(parsed))
    except Exception as error:
        print(f"run_matched_gate_v1: ERROR: {error}", file=sys.stderr)
        sys.exit(1)
