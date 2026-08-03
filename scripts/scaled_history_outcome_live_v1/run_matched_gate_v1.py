#!/usr/bin/env python3
"""Run the fresh matched gate for the scaled history outcome candidate."""

from __future__ import annotations

import argparse
import concurrent.futures
import json
import os
from pathlib import Path
import sys
import time
from typing import Any


sys.path.insert(
    0, str(Path(__file__).resolve().parents[1] / "history_value_search_live_v2")
)
import run_matched_gate_v2 as base  # noqa: E402


SCHEMA = "mtg-kernel-scaled-history-outcome-live-matched-gate/v1"


def _candidate_identity(root: Path) -> dict[str, str]:
    candidate_path = root / "structured_history_candidate.json"
    candidate = json.loads(candidate_path.read_text(encoding="utf-8"))
    if candidate.get("schema") != "mtg-kernel-structured-history-policy-value-residual-candidate/v1":
        raise RuntimeError("candidate schema mismatch")
    report = candidate.get("report", {})
    weights = candidate.get("weights", {})
    parent = candidate.get("parent", {})
    identity = {
        "adam_step": str(parent.get("adam_step")),
        "manifest": base._sha256(candidate_path),
        "payload": str(weights.get("sha256")),
        "train_state": str(report.get("sha256")),
        "model": str(candidate.get("composite_model_parameter_sha256")),
    }
    if any(len(value) != 64 for key, value in identity.items() if key != "adam_step"):
        raise RuntimeError("candidate identity is incomplete")
    return identity


def _adjudicate(
    args: argparse.Namespace,
    accepted: list[int],
    task_results: dict[tuple[int, str], dict[str, Any]],
    excluded: list[int],
    surplus: list[int],
) -> dict[str, Any]:
    candidate_wins = 0
    parent_wins = 0
    gains = 0
    losses = 0
    ties = 0
    seat_net = {"p0": 0, "p1": 0}
    matched_pairs = []
    for pair_index in accepted:
        candidate_task = task_results[(pair_index, "search")]
        parent_task = task_results[(pair_index, "parent")]
        candidate = base._read_pair(Path(candidate_task["log"]))
        parent = base._read_pair(Path(parent_task["log"]))
        for field in (
            "base_seed",
            "episodes",
            "pair_index",
            "environment_seed",
            "candidate_seats",
        ):
            if candidate.get(field) != parent.get(field):
                raise RuntimeError(f"pair {pair_index} mismatched field {field}")
        seats = candidate["candidate_seats"].split(",")
        candidate_winners = candidate["winners"].split(",")
        parent_winners = parent["winners"].split(",")
        if seats != ["p0", "p1"] or len(candidate_winners) != 2 or len(parent_winners) != 2:
            raise RuntimeError(f"pair {pair_index} has invalid seat or winner fields")
        for seat, candidate_winner, parent_winner in zip(
            seats, candidate_winners, parent_winners
        ):
            candidate_win = candidate_winner == seat
            parent_win = parent_winner == seat
            candidate_wins += int(candidate_win)
            parent_wins += int(parent_win)
            seat_net[seat] += int(candidate_win) - int(parent_win)
            if candidate_win and not parent_win:
                gains += 1
            elif parent_win and not candidate_win:
                losses += 1
            else:
                ties += 1
        matched_pairs.append(
            {
                "pair_index": pair_index,
                "environment_seed": candidate["environment_seed"],
                "candidate_log_sha256": candidate_task["log_sha256"],
                "parent_log_sha256": parent_task["log_sha256"],
            }
        )
    gates = {
        "paired_gain": gains >= losses + 3,
        "p0_net": seat_net["p0"] >= -2,
        "p1_net": seat_net["p1"] >= -2,
        "all_pairs_matched": len(matched_pairs) == args.target_pairs,
    }
    return {
        "schema": SCHEMA + ".report",
        "base_seed": args.base_seed,
        "accepted_pairs": accepted,
        "excluded_pairs": excluded,
        "surplus_unadjudicated_pairs": surplus,
        "matched_pairs": matched_pairs,
        "games": len(accepted) * 2,
        "candidate_wins": candidate_wins,
        "parent_wins": parent_wins,
        "gains": gains,
        "losses": losses,
        "ties": ties,
        "seat_net": seat_net,
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
    (args.evidence_root / "tasks").mkdir()
    for path in (
        args.maven,
        args.mage_repo,
        args.search_scorer,
        args.parent_scorer,
        args.search_root,
        args.parent_root,
        args.source_database,
    ):
        if not path.exists():
            raise RuntimeError(f"required path does not exist: {path}")
    if base._sha256(args.source_database) != base.CARD_DB_SHA256:
        raise RuntimeError("source card database SHA-256 mismatch")
    base.SEARCH_IDENTITY = _candidate_identity(args.search_root)

    accepted: list[int] = []
    excluded: list[int] = []
    surplus: list[int] = []
    task_results: dict[tuple[int, str], dict[str, Any]] = {}
    started = time.time()
    for batch_start in range(0, args.max_pairs, args.batch_pairs):
        pair_indices = list(
            range(batch_start, min(batch_start + args.batch_pairs, args.max_pairs))
        )
        tasks = [(arm, pair) for pair in pair_indices for arm in ("search", "parent")]
        with concurrent.futures.ThreadPoolExecutor(max_workers=len(tasks)) as executor:
            futures = {
                executor.submit(base._run_task, args, worker, arm, pair): (pair, arm)
                for worker, (arm, pair) in enumerate(tasks)
            }
            for future in concurrent.futures.as_completed(futures):
                result = future.result()
                task_results[(result["pair_index"], result["arm"])] = result
        successful = []
        for pair_index in pair_indices:
            both = all(
                task_results[(pair_index, arm)]["status"] == "success"
                for arm in ("search", "parent")
            )
            (successful if both else excluded).append(pair_index)
        remaining = args.target_pairs - len(accepted)
        accepted.extend(successful[:remaining])
        surplus.extend(successful[remaining:])
        state = {
            "schema": SCHEMA + ".state",
            "base_seed": args.base_seed,
            "target_pairs": args.target_pairs,
            "accepted_pairs": accepted,
            "excluded_pairs": excluded,
            "surplus_unadjudicated_pairs": surplus,
            "tasks": [task_results[key] for key in sorted(task_results)],
            "outcomes_parsed": False,
            "elapsed_seconds": time.time() - started,
        }
        base._atomic_json(args.evidence_root / "state.json", state)
        if len(accepted) >= args.target_pairs:
            break
    if len(accepted) != args.target_pairs:
        raise RuntimeError(
            f"only {len(accepted)} mutually successful pairs within {args.max_pairs}"
        )
    report = _adjudicate(args, accepted, task_results, excluded, surplus)
    report["elapsed_seconds"] = time.time() - started
    base._atomic_json(args.evidence_root / "report.json", report)
    state["outcomes_parsed"] = True
    state["report_sha256"] = base._sha256(args.evidence_root / "report.json")
    base._atomic_json(args.evidence_root / "state.json", state)
    print(json.dumps(report, sort_keys=True))
    return 0


def self_test() -> int:
    fields = base._fields(
        "XMAGE_RALLY_ANCHOR_PAIR PASS base_seed=7 episodes=2,3 "
        "pair_index=1 environment_seed=abc candidate_seats=p0,p1 winners=p1,p0"
    )
    if fields["pair_index"] != "1" or fields["winners"] != "p1,p0":
        raise RuntimeError("pair parser self-test failed")
    print("run_matched_gate_v1: SELF-TEST PASS")
    return 0


def arguments() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--evidence-root", type=Path)
    parser.add_argument("--base-seed", type=int, default=1_260_001)
    parser.add_argument("--target-pairs", type=int, default=16)
    parser.add_argument("--max-pairs", type=int, default=64)
    parser.add_argument("--batch-pairs", type=int, default=4)
    parser.add_argument(
        "--mage-repo",
        type=Path,
        default=Path(r"C:\Users\Jack\IdeaProjects\mage-kernel-anchor-spike-v1"),
    )
    parser.add_argument(
        "--search-scorer",
        type=Path,
        default=Path(r"C:\Users\Jack\IdeaProjects\mtg-kernel-structured-successor-screen-v1-codex\target\release\checkpoint_shadow_stdio_v1.exe"),
    )
    parser.add_argument(
        "--parent-scorer",
        type=Path,
        default=Path(r"C:\Users\Jack\IdeaProjects\mtg-kernel-structured-successor-screen-v1-codex\target\release\checkpoint_shadow_stdio_v1.exe"),
    )
    parser.add_argument("--search-root", type=Path)
    parser.add_argument("--parent-root", type=Path)
    parser.add_argument(
        "--source-database",
        type=Path,
        default=Path(r"C:\Users\Jack\IdeaProjects\mage-kernel-anchor-spike-v1\db\cards.h2.mv.db"),
    )
    parser.add_argument(
        "--maven",
        type=Path,
        default=Path(r"C:\Program Files\apache-maven-3.9.8\bin\mvn.cmd"),
    )
    args = parser.parse_args()
    if args.self_test:
        return args
    if args.evidence_root is None or args.search_root is None or args.parent_root is None:
        parser.error("--evidence-root, --search-root, and --parent-root are required")
    if args.base_seed != 1_260_001:
        parser.error("formal base seed is fixed to 1260001")
    if args.target_pairs != 16 or args.max_pairs != 64 or args.batch_pairs != 4:
        parser.error("formal topology is fixed to 16/64/4")
    return args


if __name__ == "__main__":
    try:
        parsed = arguments()
        sys.exit(self_test() if parsed.self_test else run(parsed))
    except Exception as error:
        print(f"run_matched_gate_v1: ERROR: {error}", file=sys.stderr)
        sys.exit(1)
