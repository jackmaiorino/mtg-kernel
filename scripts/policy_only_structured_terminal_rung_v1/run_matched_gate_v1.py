#!/usr/bin/env python3
"""Run the fresh Pool3 strength gate for the terminal-only structured rung."""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import sys
from typing import Any


SCRIPT_DIR = Path(__file__).resolve().parent
QUALIFICATION_DIR = SCRIPT_DIR.parent / "policy_only_structured_successor_v1"
sys.path.insert(0, str(SCRIPT_DIR))
sys.path.insert(0, str(QUALIFICATION_DIR))

import run_matched_gate_v1 as qualification  # noqa: E402
import fit_head_only_v1 as head_only  # noqa: E402
import project_trust_region_v1 as projection  # noqa: E402
import run_pipeline_v1 as pipeline  # noqa: E402


SCHEMA = "mtg-kernel-structured-policy-terminal-rung-pool3-gate/v1"
FORMAL_BASE_SEED = 1_670_001
FORMAL_PAIRS = 1_024
PROFILE_MAX_PAIRS = 64
TOPOLOGIES = ("sequential", "parallel")
FORMAL_SCORER_SHA256 = (
    "3c1ea778f793fba867e78632d505fa9bd9197585cd42c046d6bc0451b7b18a5e"
)


def _sha256(path: Path) -> str:
    return qualification._sha256(path)


def _candidate_identity(root: Path) -> dict[str, Any]:
    candidate_path = root / pipeline.CANDIDATE_FILENAME
    report_path = root / "report.json"
    weights_path = root / "weights.f32le"
    candidate = json.loads(candidate_path.read_text(encoding="utf-8"))
    report = json.loads(report_path.read_text(encoding="utf-8"))
    contracts = {
        pipeline.CANDIDATE_SCHEMA: (
            pipeline.REPORT_SCHEMA,
            pipeline.COMPOSITE_DOMAIN,
            "xmage-cp7-outcome-structured-policy-successor-v2",
        ),
        projection.CANDIDATE_SCHEMA: (
            projection.REPORT_SCHEMA,
            projection.COMPOSITE_DOMAIN,
            "xmage-cp7-outcome-structured-policy-successor-v3",
        ),
        head_only.CANDIDATE_SCHEMA: (
            head_only.REPORT_SCHEMA,
            head_only.COMPOSITE_DOMAIN,
            "xmage-cp7-outcome-structured-policy-successor-v4",
        ),
        "mtg-kernel-structured-policy-successor-candidate/v5": (
            "mtg-kernel-structured-policy-space-response-oracle-report/v1",
            b"mtg-kernel-structured-policy-space-response-oracle-composite-model/v1",
            "xmage-cp7-outcome-structured-policy-successor-v5",
        ),
    }
    contract = contracts.get(candidate.get("schema"))
    if contract is None:
        raise RuntimeError("terminal-rung candidate schema mismatch")
    report_schema, composite_domain, authority_kind = contract
    if report.get("schema") != report_schema:
        raise RuntimeError("terminal-rung report schema mismatch")
    if candidate.get("report") != {
        "filename": "report.json",
        "sha256": _sha256(report_path),
    }:
        raise RuntimeError("terminal-rung report binding mismatch")
    weights = candidate.get("weights", {})
    if (
        weights.get("filename") != "weights.f32le"
        or weights.get("sha256") != _sha256(weights_path)
    ):
        raise RuntimeError("terminal-rung weights binding mismatch")
    parent = candidate.get("parent", {})
    expected_parent = {
        "directory": "parent",
        "manifest_sha256": qualification.PARENT_IDENTITY["manifest"],
        "payload_sha256": qualification.PARENT_IDENTITY["payload"],
        "native_state_sha256": qualification.PARENT_IDENTITY["train_state"],
        "model_parameter_sha256": qualification.PARENT_IDENTITY["model"],
        "adam_step": 1,
    }
    if parent != expected_parent:
        raise RuntimeError("terminal-rung retained-parent binding mismatch")
    expected_composite = hashlib.sha256(
        composite_domain
        + bytes.fromhex(parent["model_parameter_sha256"])
        + weights_path.read_bytes()
    ).hexdigest()
    if candidate.get("composite_model_parameter_sha256") != expected_composite:
        raise RuntimeError("terminal-rung composite identity mismatch")
    transport = report.get("transport", {})
    if (
        transport.get("parent_value_bit_exact") is not True
        or not isinstance(transport.get("maximum_absolute_logit_error"), (int, float))
        or not 0.0
        <= float(transport["maximum_absolute_logit_error"])
        <= pipeline.TRANSPORT_LIMIT
    ):
        raise RuntimeError("terminal-rung transport is not qualified")
    if candidate.get("schema", "").endswith("/v5") and (
        report.get("config", {}).get("development_only") is not False
        or report.get("movement") is None
        or report.get("source", {}).get("phase") != "selected"
    ):
        raise RuntimeError("response-oracle candidate is not selected and qualified")
    candidate_sha256 = _sha256(candidate_path)
    report_sha256 = _sha256(report_path)
    weights_sha256 = _sha256(weights_path)
    qualification._validate_parent_root(root / "parent")
    return {
        "adam_step": "1",
        "manifest": candidate_sha256,
        "payload": weights_sha256,
        "train_state": report_sha256,
        "model": expected_composite,
        "candidate_json_sha256": candidate_sha256,
        "report_sha256": report_sha256,
        "weights_sha256": weights_sha256,
        "composite_model_parameter_sha256": expected_composite,
        "parent": parent,
        "authority_kind": authority_kind,
    }


def _adjudicate(
    args: argparse.Namespace,
    candidate_result: dict[str, Any],
    baseline_result: dict[str, Any],
) -> dict[str, Any]:
    candidate_path = Path(candidate_result["paths"]["outcome_jsonl"])
    baseline_path = Path(baseline_result["paths"]["outcome_jsonl"])
    candidate_header, candidate = qualification._load_panel(
        candidate_path, args.base_seed, args.target_pairs
    )
    baseline_header, baseline = qualification._load_panel(
        baseline_path, args.base_seed, args.target_pairs
    )
    candidate_teacher = qualification._load_teacher_header(
        Path(candidate_result["paths"]["teacher_jsonl"])
    )
    baseline_teacher = qualification._load_teacher_header(
        Path(baseline_result["paths"]["teacher_jsonl"])
    )
    qualification._validate_checkpoint_header(
        candidate_header.get("checkpoint"),
        args.candidate_identity,
        args.candidate_identity["authority_kind"],
    )
    qualification._validate_checkpoint_header(
        baseline_header.get("checkpoint"),
        args.baseline_identity,
        "xmage-cp7-outcome-structured-policy-successor-v1",
    )
    if candidate_teacher.get("checkpoint") != candidate_header.get("checkpoint"):
        raise RuntimeError("candidate teacher and outcome checkpoint headers differ")
    if baseline_teacher.get("checkpoint") != baseline_header.get("checkpoint"):
        raise RuntimeError("baseline teacher and outcome checkpoint headers differ")
    if set(candidate) != set(baseline):
        raise RuntimeError("candidate and baseline terminal keys differ")

    gains = losses = ties = 0
    candidate_wins = baseline_wins = 0
    seat_wins = {
        "p0": {"candidate": 0, "baseline": 0},
        "p1": {"candidate": 0, "baseline": 0},
    }
    matched_pairs: dict[int, dict[str, Any]] = {}
    for key in sorted(candidate):
        candidate_row = candidate[key]
        baseline_row = baseline[key]
        for field in (
            "pair_environment_seed_u64_hex",
            "episode_id",
            "candidate_seat",
        ):
            if candidate_row.get(field) != baseline_row.get(field):
                raise RuntimeError(f"matched terminal field differs at {key}: {field}")
        pair_index, _, seat = key
        candidate_reward = int(candidate_row["candidate_terminal_reward"])
        baseline_reward = int(baseline_row["candidate_terminal_reward"])
        candidate_win = candidate_reward > 0
        baseline_win = baseline_reward > 0
        candidate_wins += int(candidate_win)
        baseline_wins += int(baseline_win)
        seat_wins[seat]["candidate"] += int(candidate_win)
        seat_wins[seat]["baseline"] += int(baseline_win)
        if candidate_reward > baseline_reward:
            gains += 1
        elif candidate_reward < baseline_reward:
            losses += 1
        else:
            ties += 1
        matched_pairs.setdefault(
            pair_index,
            {
                "pair_index": pair_index,
                "environment_seed": candidate_row[
                    "pair_environment_seed_u64_hex"
                ],
            },
        )
    seat_deltas = {
        seat: values["candidate"] - values["baseline"]
        for seat, values in seat_wins.items()
    }
    gates = {
        "gains_at_least_losses_plus_20": gains >= losses + 20,
        "candidate_wins_at_least_baseline_plus_20": (
            candidate_wins >= baseline_wins + 20
        ),
        "p0_candidate_minus_baseline_wins_at_least_minus_4": seat_deltas["p0"] >= -4,
        "p1_candidate_minus_baseline_wins_at_least_minus_4": seat_deltas["p1"] >= -4,
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
        "baseline_wins": baseline_wins,
        "gains": gains,
        "losses": losses,
        "ties": ties,
        "wins_by_candidate_seat": seat_wins,
        "candidate_minus_baseline_wins_by_seat": seat_deltas,
        "matched_pairs": list(matched_pairs.values()),
        "candidate": {
            "outcome_jsonl": str(candidate_path),
            "sha256": _sha256(candidate_path),
            "teacher_jsonl": candidate_result["paths"]["teacher_jsonl"],
            "teacher_sha256": candidate_result["report"]["teacher_sha256"],
            "checkpoint": candidate_header.get("checkpoint"),
        },
        "baseline": {
            "outcome_jsonl": str(baseline_path),
            "sha256": _sha256(baseline_path),
            "teacher_jsonl": baseline_result["paths"]["teacher_jsonl"],
            "teacher_sha256": baseline_result["report"]["teacher_sha256"],
            "checkpoint": baseline_header.get("checkpoint"),
        },
        "candidate_identity": args.candidate_identity,
        "baseline_identity": args.baseline_identity,
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
            raise RuntimeError("evidence root may initially contain only manifest.json")
    args.evidence_root.mkdir(parents=True, exist_ok=True)
    for path in (
        args.scorer,
        args.candidate_root,
        args.baseline_root,
        args.pool_root,
    ):
        if not path.exists():
            raise RuntimeError(f"required path does not exist: {path}")
    if _sha256(args.pool_root / "pool.json") != qualification.POOL_CONTRACT_SHA256:
        raise RuntimeError("Pool3 contract SHA-256 mismatch")
    if args.formal and _sha256(args.scorer) != FORMAL_SCORER_SHA256:
        raise RuntimeError("formal native scorer SHA-256 mismatch")
    args.candidate_identity = _candidate_identity(args.candidate_root)
    if args.formal:
        expected_seed = (
            1_790_001
            if args.candidate_identity["authority_kind"].endswith("-v5")
            else (
                1_680_001
                if args.candidate_identity["authority_kind"].endswith("-v4")
                else FORMAL_BASE_SEED
            )
        )
        if args.base_seed not in (None, expected_seed):
            raise RuntimeError("formal base seed does not match candidate contract")
        args.base_seed = expected_seed
    args.baseline_identity = qualification._candidate_identity(args.baseline_root)
    args.parent_root = args.baseline_root
    arm_results = qualification._run_arms(args)
    state = {
        "schema": SCHEMA + ".state",
        "formal": args.formal,
        "profile_pairs": args.profile_pairs,
        "topology": args.topology,
        "base_seed": args.base_seed,
        "target_pairs": args.target_pairs,
        "candidate_identity": args.candidate_identity,
        "baseline_identity": args.baseline_identity,
        "arms": arm_results,
        "outcomes_parsed": False,
    }
    qualification._atomic_json(args.evidence_root / "state.json", state)
    if any(
        result["status"] != "success"
        for result in arm_results.values()
        if isinstance(result, dict) and "status" in result
    ):
        raise RuntimeError("one or more native collection arms failed after retries")
    report = _adjudicate(args, arm_results["candidate"], arm_results["parent"])
    report["topology_wall_seconds"] = arm_results["wall_seconds"]
    report["arm_elapsed_seconds"] = {
        "candidate": arm_results["candidate"]["elapsed_seconds"],
        "baseline": arm_results["parent"]["elapsed_seconds"],
    }
    qualification._atomic_json(args.evidence_root / "report.json", report)
    state["outcomes_parsed"] = True
    state["report_sha256"] = _sha256(args.evidence_root / "report.json")
    qualification._atomic_json(args.evidence_root / "state.json", state)
    print(json.dumps(report, sort_keys=True, allow_nan=False))
    return 0


def _arguments() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--profile-pairs", type=int)
    parser.add_argument("--topology", choices=TOPOLOGIES, required=True)
    parser.add_argument("--evidence-root", type=Path, required=True)
    parser.add_argument("--base-seed", type=int)
    parser.add_argument("--candidate-root", type=Path, required=True)
    parser.add_argument("--baseline-root", type=Path, required=True)
    parser.add_argument("--pool-root", type=Path, default=qualification.POOL_ROOT)
    parser.add_argument("--scorer", type=Path, default=qualification.SCORER)
    args = parser.parse_args()
    if args.profile_pairs is None:
        if args.pool_root != qualification.POOL_ROOT:
            parser.error("formal Pool3 root is fixed")
        if args.scorer != qualification.SCORER:
            parser.error("formal scorer path is fixed")
        args.formal = True
        args.target_pairs = FORMAL_PAIRS
    else:
        if not 1 <= args.profile_pairs <= PROFILE_MAX_PAIRS:
            parser.error("profile pairs must be between 1 and 64")
        if args.base_seed is None or args.base_seed < 0:
            parser.error("profile requires a nonnegative base seed")
        args.formal = False
        args.target_pairs = args.profile_pairs
    return args


if __name__ == "__main__":
    try:
        raise SystemExit(run(_arguments()))
    except Exception as error:
        print(f"run_matched_gate_v1: ERROR: {error}", file=sys.stderr)
        raise SystemExit(1)
