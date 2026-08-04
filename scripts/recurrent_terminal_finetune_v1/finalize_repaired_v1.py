#!/usr/bin/env python3
"""Finalize the repaired recurrent on-policy corpus and cache."""

from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import sys
import time
from typing import Any

import torch

import collect_onpolicy_v1 as corpus
import repair_failed_shard_v1 as repair


SCHEMA = "mtg-kernel-recurrent-terminal-onpolicy-repaired/v1"
REPAIRS = ((0, 58, 514), (128, 143, 515))


def _original_tasks(root: Path) -> list[dict[str, Any]]:
    results = []
    for first_pair, pair_count in corpus._task_ranges():
        if first_pair in {row[0] for row in REPAIRS}:
            continue
        stem = f"p{first_pair:06d}-n{pair_count:03d}"
        teacher = root / "tasks" / f"{stem}.teacher.jsonl"
        outcome = root / "tasks" / f"{stem}.outcome.jsonl"
        log = root / "tasks" / f"{stem}.log"
        final_lines = [
            line
            for line in log.read_text(encoding="utf-8").splitlines()
            if line.startswith("XMAGE_RALLY_ANCHOR_SPIKE PASS ")
        ]
        if len(final_lines) != 1:
            corpus._fail(f"original task {stem} lacks one final pass line")
        results.append(
            {
                "source": "original-parallel-shard",
                "first_pair": first_pair,
                "pair_count": pair_count,
                "teacher_path": str(teacher),
                "outcome_path": str(outcome),
                "log_path": str(log),
                "log_sha256": corpus._sha256(log),
            }
        )
    return results


def run(args: argparse.Namespace) -> int:
    report_path = args.original_root / "report.json"
    if args.cache.exists() or report_path.exists():
        corpus._fail("cache and repaired report must both be new")
    tasks = _original_tasks(args.original_root)
    repair_sources = []
    for repair_root, (first_pair, excluded_pair, replacement_pair) in zip(
        (args.repair_root_00, args.repair_root_128), REPAIRS
    ):
        repair_path = repair_root / "report.json"
        repair_report = json.loads(repair_path.read_text(encoding="utf-8"))
        failures = repair_report.get("failures", [])
        if (
            repair_report.get("schema") != repair.SCHEMA
            or repair_report.get("status") != "complete"
            or repair_report.get("source_failed_range")
            != {"first_pair": first_pair, "pair_count": 64}
            or len(failures) != 1
            or failures[0].get("pair_index") != excluded_pair
            or failures[0].get("replacement_pair_index") != replacement_pair
            or repair_report.get("fold_counts")
            != {str(index): 16 for index in range(4)}
        ):
            corpus._fail("repair report identity or exclusion mismatch")
        repair_sources.append(
            {"path": str(repair_path), "sha256": corpus._sha256(repair_path)}
        )
        for task in repair_report["tasks"]:
            tasks.append({"source": "one-pair-repair", **task})
    tasks.sort(key=lambda value: int(value["first_pair"]))

    started = time.perf_counter()
    policy: list[dict[str, Any]] = []
    value: list[dict[str, Any]] = []
    complete_join = {
        "episode_count": 0,
        "pair_count": 0,
        "policy_step_count": 0,
        "physical_decision_count": 0,
        "selected_action_kind_counts": {},
        "selected_semantics_public": True,
        "terminal_replays_exact": True,
        "complete_policy_steps": True,
        "complete_physical_decisions": True,
    }
    for task in tasks:
        validated, task_policy, task_value = corpus._validate_task(
            Path(task["teacher_path"]),
            Path(task["outcome_path"]),
            int(task["first_pair"]),
            int(task["pair_count"]),
        )
        if (
            "teacher_sha256" in task
            and validated["teacher_sha256"] != task["teacher_sha256"]
        ) or (
            "outcome_sha256" in task
            and validated["outcome_sha256"] != task["outcome_sha256"]
        ):
            corpus._fail("task export changed after sealing")
        task.update(validated)
        policy.extend(task_policy)
        value.extend(task_value)
        corpus._merge_join(complete_join, validated["join"])

    excluded_pairs = {row[1] for row in REPAIRS}
    replacement_pairs = {row[2] for row in REPAIRS}
    expected_pairs = (set(range(corpus.PAIR_COUNT)) - excluded_pairs) | replacement_pairs
    observed_pairs = {int(row["pair_index"]) for row in value}
    fold_counts = {
        str(residue): sum(pair % 4 == residue for pair in observed_pairs)
        for residue in range(4)
    }
    if (
        observed_pairs != expected_pairs
        or int(complete_join["pair_count"]) != corpus.PAIR_COUNT
        or int(complete_join["episode_count"]) != corpus.PAIR_COUNT * 2
        or set(fold_counts.values()) != {128}
    ):
        corpus._fail("repaired aggregate coverage or fold balance mismatch")

    episode_rewards: dict[tuple[int, int, int], float] = {}
    for row in value:
        key = (
            int(row["pair_index"]),
            int(row["episode"]),
            int(row["candidate_seat"]),
        )
        reward = float(row["terminal_reward"])
        if key in episode_rewards and episode_rewards[key] != reward:
            corpus._fail("episode terminal reward mismatch")
        episode_rewards[key] = reward
    if len(episode_rewards) != corpus.PAIR_COUNT * 2:
        corpus._fail("episode reward coverage mismatch")
    outcome_counts = {
        "wins": sum(reward > 0 for reward in episode_rewards.values()),
        "draws": sum(reward == 0 for reward in episode_rewards.values()),
        "losses": sum(reward < 0 for reward in episode_rewards.values()),
    }

    card_max = max(int(row["object_card_ids"].max().item()) for row in policy + value)
    group_max = max(int(row["object_groups"].max().item()) for row in policy + value)
    payload = {
        "version": corpus.structured.SCRIPT_VERSION,
        "schema": corpus.CACHE_SCHEMA,
        "policy": policy,
        "value": value,
        "card_max": card_max,
        "group_max": group_max,
        "complete_history_join": complete_join,
        "source": {
            "schema": SCHEMA,
            "base_seed": corpus.BASE_SEED,
            "selected_pair_indices": sorted(expected_pairs),
            "excluded_pair_indices": sorted(excluded_pairs),
            "replacement_pair_indices": sorted(replacement_pairs),
            "tasks": tasks,
        },
    }
    temporary = args.cache.with_name(args.cache.name + ".tmp")
    if temporary.exists():
        corpus._fail(f"temporary cache already exists: {temporary}")
    torch.save(payload, temporary)
    os.replace(temporary, args.cache)
    report = {
        "schema": SCHEMA,
        "status": "complete",
        "base_seed": corpus.BASE_SEED,
        "pairs": corpus.PAIR_COUNT,
        "games": corpus.PAIR_COUNT * 2,
        "selected_pair_indices": sorted(expected_pairs),
        "excluded_pair_indices": sorted(excluded_pairs),
        "replacement_pair_indices": sorted(replacement_pairs),
        "fold_counts": fold_counts,
        "outcomes": outcome_counts,
        "complete_history_join": complete_join,
        "cache": str(args.cache),
        "cache_sha256": corpus._sha256(args.cache),
        "cache_bytes": args.cache.stat().st_size,
        "repair_reports": repair_sources,
        "finalize_seconds": time.perf_counter() - started,
        "tasks": tasks,
        "non_claims": [
            "corpus outcomes are training data and not strength evidence",
            "pairs 58 and 143 were excluded for mapper failures before outcome adjudication",
            "natural terminal win or loss is the only reward",
        ],
    }
    corpus._atomic_json(report_path, report)
    print(json.dumps(report, sort_keys=True, allow_nan=False))
    return 0


def arguments() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--original-root",
        type=Path,
        default=Path(r"D:\mtg-kernel-recurrent-terminal-onpolicy-v1"),
    )
    parser.add_argument(
        "--repair-root-00",
        type=Path,
        default=Path(r"D:\mtg-kernel-recurrent-terminal-onpolicy-v1-repair-00"),
    )
    parser.add_argument(
        "--repair-root-128",
        type=Path,
        default=Path(r"D:\mtg-kernel-recurrent-terminal-onpolicy-v1-repair"),
    )
    parser.add_argument(
        "--cache",
        type=Path,
        default=Path(r"D:\mtg-kernel-recurrent-terminal-onpolicy-v1\cache.pt"),
    )
    return parser.parse_args()


if __name__ == "__main__":
    try:
        raise SystemExit(run(arguments()))
    except (OSError, RuntimeError, ValueError) as error:
        print(f"finalize_recurrent_terminal_repair_v1: ERROR: {error}", file=sys.stderr)
        raise SystemExit(2)
