#!/usr/bin/env python3
"""Repair one failed corpus shard with outcome-blind same-fold replacements."""

from __future__ import annotations

import argparse
import concurrent.futures
import json
from pathlib import Path
import sys
from typing import Any

import collect_onpolicy_v1 as corpus


SCHEMA = "mtg-kernel-recurrent-terminal-onpolicy-repair/v1"
FIRST_PAIR = 128
PAIR_COUNT = 64


def _next_replacement(residue: int, used: set[int]) -> int:
    candidate = 512 + ((residue - 512) % 4)
    while candidate in used:
        candidate += 4
    return candidate


def run(args: argparse.Namespace) -> int:
    if args.evidence_root.exists():
        corpus._fail(f"evidence root already exists: {args.evidence_root}")
    for path in (
        args.mage_repo,
        args.scorer_exe,
        args.candidate_root,
        args.source_database,
        args.maven,
        args.python,
    ):
        if not path.exists():
            corpus._fail(f"required path does not exist: {path}")
    if corpus._sha256(args.source_database) != corpus.SOURCE_DATABASE_SHA256:
        corpus._fail("source card database SHA-256 mismatch")
    args.evidence_root.mkdir(parents=True)
    (args.evidence_root / "tasks").mkdir()
    databases = [
        corpus._worker_database(args, worker) for worker in range(corpus.WORKERS)
    ]

    pending = list(range(args.first_pair, args.first_pair + PAIR_COUNT))
    used = set(pending)
    successes: list[dict[str, Any]] = []
    failures: list[dict[str, Any]] = []
    while pending:
        batch = pending[: corpus.WORKERS]
        pending = pending[corpus.WORKERS :]
        with concurrent.futures.ThreadPoolExecutor(max_workers=len(batch)) as executor:
            futures = {
                executor.submit(
                    corpus._run_task,
                    args,
                    worker,
                    pair,
                    1,
                    databases[worker],
                ): pair
                for worker, pair in enumerate(batch)
            }
            for future in concurrent.futures.as_completed(futures):
                pair = futures[future]
                try:
                    result = future.result()
                    successes.append(result)
                    print(
                        json.dumps(
                            {
                                "status": "success",
                                "pair_index": pair,
                                "successful_pairs": len(successes),
                            },
                            sort_keys=True,
                        ),
                        flush=True,
                    )
                except Exception as error:
                    replacement = _next_replacement(pair % 4, used)
                    used.add(replacement)
                    pending.append(replacement)
                    failures.append(
                        {
                            "pair_index": pair,
                            "replacement_pair_index": replacement,
                            "error": str(error),
                        }
                    )
                    print(
                        json.dumps(
                            {
                                "status": "excluded",
                                "pair_index": pair,
                                "replacement_pair_index": replacement,
                            },
                            sort_keys=True,
                        ),
                        flush=True,
                    )
    successes.sort(key=lambda value: int(value["first_pair"]))
    successful_pairs = [int(value["first_pair"]) for value in successes]
    fold_counts = {
        str(residue): sum(pair % 4 == residue for pair in successful_pairs)
        for residue in range(4)
    }
    if len(successes) != PAIR_COUNT or set(fold_counts.values()) != {16}:
        corpus._fail("repair did not preserve 64 pairs and balanced fold counts")
    report = {
        "schema": SCHEMA,
        "status": "complete",
        "source_failed_range": {"first_pair": args.first_pair, "pair_count": PAIR_COUNT},
        "successful_pairs": successful_pairs,
        "fold_counts": fold_counts,
        "failures": failures,
        "tasks": successes,
        "inputs": {
            "scorer_sha256": corpus._sha256(args.scorer_exe),
            "source_database_sha256": corpus.SOURCE_DATABASE_SHA256,
            "candidate_identity": corpus.CANDIDATE_IDENTITY,
        },
        "non_claims": [
            "exclusions are mapper failures and were made without terminal adjudication",
            "natural terminal win or loss is the only reward",
        ],
    }
    corpus._atomic_json(args.evidence_root / "report.json", report)
    print(json.dumps(report, sort_keys=True, allow_nan=False))
    return 0


def arguments() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--evidence-root", type=Path, required=True)
    parser.add_argument("--first-pair", type=int, default=FIRST_PAIR)
    parser.add_argument(
        "--mage-repo",
        type=Path,
        default=Path(r"C:\Users\Jack\IdeaProjects\mage-kernel-anchor-spike-v1"),
    )
    parser.add_argument(
        "--scorer-exe",
        type=Path,
        default=Path(
            r"C:\Users\Jack\IdeaProjects\mtg-kernel-structured-successor-screen-v1-codex\target\release\checkpoint_shadow_recurrent_cp7_stdio_v1.exe"
        ),
    )
    parser.add_argument(
        "--candidate-root",
        type=Path,
        default=Path(r"D:\mtg-kernel-recurrent-cp7-deployment-v1-preflight-02"),
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
        default=Path(r"C:\Users\Jack\IdeaProjects\mage\.mtgrl_venv\Scripts\python.exe"),
    )
    parser.add_argument("--task-timeout-seconds", type=int, default=7_200)
    args = parser.parse_args()
    if args.first_pair < 0 or args.first_pair % PAIR_COUNT:
        parser.error("--first-pair must be a nonnegative 64-pair shard boundary")
    return args


if __name__ == "__main__":
    try:
        raise SystemExit(run(arguments()))
    except (OSError, RuntimeError, ValueError) as error:
        print(f"repair_recurrent_terminal_shard_v1: ERROR: {error}", file=sys.stderr)
        raise SystemExit(2)
