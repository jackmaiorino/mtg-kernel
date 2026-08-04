#!/usr/bin/env python3
"""Run a fresh matched terminal screen for the recurrent CP7 deployment."""

from __future__ import annotations

import argparse
import concurrent.futures
import json
import os
from pathlib import Path
import sys
import time
from typing import Any


SCRIPT_DIR = Path(__file__).resolve().parent
sys.path.insert(0, str(SCRIPT_DIR.parent / "scaled_history_outcome_live_v1"))
import run_matched_gate_v1 as scaled  # noqa: E402


base = scaled.base
SCHEMA = "mtg-kernel-recurrent-cp7-terminal-screen/v1"
PACKAGE_SCHEMA = "mtg-kernel-recurrent-cp7-deployment/v1"
PARENT_SCHEMA = "mtg-kernel-structured-policy-successor-candidate/v1"
SOURCE_DATABASE_SHA256 = "1defa6420bcf02b0f79c3313e964efce3b401838231e7ffe86c7c7ee6724e0b1"


def _package_identity(root: Path) -> dict[str, str]:
    manifest_path = root / "recurrent_cp7_deployment.json"
    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    if manifest.get("schema") != PACKAGE_SCHEMA:
        raise RuntimeError("recurrent package schema mismatch")
    identity = {
        "authority_kind": str(manifest["identity"]["authority_kind"]),
        "adam_step": str(manifest["parent"]["adam_step"]),
        "manifest": base._sha256(manifest_path),
        "payload": str(manifest["files"]["model"]["sha256"]),
        "train_state": str(manifest["model_state_sha256"]),
        "model": str(manifest["identity"]["model_parameter_sha256"]),
    }
    if any(
        len(value) != 64
        for key, value in identity.items()
        if key not in ("authority_kind", "adam_step")
    ):
        raise RuntimeError("recurrent package identity is incomplete")
    return identity


def _parent_identity(root: Path) -> dict[str, str]:
    candidate_path = root / "structured_policy_successor.json"
    candidate = json.loads(candidate_path.read_text(encoding="utf-8"))
    if candidate.get("schema") != PARENT_SCHEMA:
        raise RuntimeError("parent candidate schema mismatch")
    identity = {
        "authority_kind": "xmage-cp7-outcome-structured-policy-successor-v1",
        "adam_step": str(candidate["parent"]["adam_step"]),
        "manifest": base._sha256(candidate_path),
        "payload": str(candidate["weights"]["sha256"]),
        "train_state": str(candidate["report"]["sha256"]),
        "model": str(candidate["composite_model_parameter_sha256"]),
    }
    if any(
        len(value) != 64
        for key, value in identity.items()
        if key not in ("authority_kind", "adam_step")
    ):
        raise RuntimeError("parent identity is incomplete")
    return identity


def run(args: argparse.Namespace) -> int:
    if args.evidence_root.exists():
        raise RuntimeError(f"evidence root already exists: {args.evidence_root}")
    for path in (
        args.maven,
        args.mage_repo,
        args.search_scorer,
        args.parent_scorer,
        args.search_root,
        args.parent_root,
        args.source_database,
        args.python,
    ):
        if not path.exists():
            raise RuntimeError(f"required path does not exist: {path}")
    if base._sha256(args.source_database) != SOURCE_DATABASE_SHA256:
        raise RuntimeError("source card database SHA-256 mismatch")
    base.CARD_DB_SHA256 = SOURCE_DATABASE_SHA256
    base.SEARCH_IDENTITY = _package_identity(args.search_root)
    base.PARENT_IDENTITY = _parent_identity(args.parent_root)
    scaled.SCHEMA = SCHEMA
    os.environ["MTG_KERNEL_RECURRENT_CP7_PYTHON"] = str(args.python)

    args.evidence_root.mkdir(parents=True)
    (args.evidence_root / "tasks").mkdir()
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
    report = scaled._adjudicate(args, accepted, task_results, excluded, surplus)
    report["elapsed_seconds"] = time.time() - started
    report["candidate_identity"] = base.SEARCH_IDENTITY
    report["parent_identity"] = base.PARENT_IDENTITY
    report["source_database_sha256"] = SOURCE_DATABASE_SHA256
    report["non_claims"] = [
        "this rapid screen is not a pro-level claim",
        "terminal win or loss is the only promotion measure",
    ]
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
    print("run_recurrent_cp7_terminal_gate_v1: SELF-TEST PASS")
    return 0


def arguments() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--evidence-root", type=Path)
    parser.add_argument("--base-seed", type=int, default=2_020_001)
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
        default=Path(
            r"C:\Users\Jack\IdeaProjects\mtg-kernel-structured-successor-screen-v1-codex\target\release\checkpoint_shadow_recurrent_cp7_stdio_v1.exe"
        ),
    )
    parser.add_argument(
        "--parent-scorer",
        type=Path,
        default=Path(
            r"C:\Users\Jack\IdeaProjects\mtg-kernel-structured-successor-screen-v1-codex\target\release\checkpoint_shadow_stdio_v1.exe"
        ),
    )
    parser.add_argument(
        "--search-root",
        type=Path,
        default=Path(r"D:\mtg-kernel-recurrent-cp7-deployment-v1-preflight-02"),
    )
    parser.add_argument(
        "--parent-root",
        type=Path,
        default=Path(r"D:\mtg-kernel-policy-only-structured-successor-v1\candidate"),
    )
    parser.add_argument(
        "--source-database",
        type=Path,
        default=Path(
            r"C:\Users\Jack\IdeaProjects\mage-kernel-anchor-spike-v1\db\cards.h2.mv.db"
        ),
    )
    parser.add_argument(
        "--maven",
        type=Path,
        default=Path(r"C:\Program Files\apache-maven-3.9.8\bin\mvn.cmd"),
    )
    parser.add_argument(
        "--python",
        type=Path,
        default=Path(
            r"C:\Users\Jack\IdeaProjects\mage\.mtgrl_venv\Scripts\python.exe"
        ),
    )
    args = parser.parse_args()
    if args.self_test:
        return args
    if args.evidence_root is None:
        parser.error("--evidence-root is required")
    if args.target_pairs < 1 or args.max_pairs < args.target_pairs:
        parser.error("pair counts are invalid")
    if args.batch_pairs < 1 or args.batch_pairs > 8:
        parser.error("--batch-pairs must be in [1, 8]")
    args.selector = "policy"
    args.cp7_opponent_root = None
    return args


if __name__ == "__main__":
    try:
        parsed = arguments()
        sys.exit(self_test() if parsed.self_test else run(parsed))
    except Exception as error:
        print(f"run_recurrent_cp7_terminal_gate_v1: ERROR: {error}", file=sys.stderr)
        sys.exit(1)
