#!/usr/bin/env python3
"""Build an exact complete-history tensor cache from synchronized DAgger exports."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import sys
import time
from typing import Any

import torch


SCRIPT_DIR = Path(__file__).resolve().parent
STRUCTURED_DIR = SCRIPT_DIR.parent / "structured_adapter_screen_v1"
sys.path.insert(0, str(STRUCTURED_DIR))

import run_screen as structured  # noqa: E402


SCHEMA = "mtg-kernel-cp7-candidate-complete-history-cache/v1"
CORPUS_SCHEMA = "mtg-kernel-cp7-candidate-shadow-corpus/v1"
BASE_SEED = 1_400_001
PAIR_COUNT = 256


def _fail(message: str) -> None:
    raise RuntimeError(message)


def _sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def _write_new_json(path: Path, value: Any) -> None:
    if path.exists():
        _fail(f"refusing to overwrite {path}")
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(
        json.dumps(value, indent=2, sort_keys=True, allow_nan=False) + "\n",
        encoding="utf-8",
        newline="\n",
    )


def _merge_join(target: dict[str, Any], source: dict[str, Any]) -> None:
    for key in ("episode_count", "pair_count", "policy_step_count", "physical_decision_count"):
        target[key] += int(source[key])
    for kind, count in source["selected_action_kind_counts"].items():
        target["selected_action_kind_counts"][kind] = (
            target["selected_action_kind_counts"].get(kind, 0) + int(count)
        )
    for key in (
        "selected_semantics_public",
        "terminal_replays_exact",
        "complete_policy_steps",
        "complete_physical_decisions",
    ):
        target[key] = bool(target[key] and source[key])


def prepare(args: argparse.Namespace) -> dict[str, Any]:
    if args.cache.exists() or args.report.exists():
        _fail("cache or report output already exists")
    corpus_sha256 = _sha256(args.corpus_report)
    corpus = json.loads(args.corpus_report.read_text(encoding="utf-8"))
    if (
        corpus.get("schema") != CORPUS_SCHEMA
        or corpus.get("status") != "complete"
        or corpus.get("base_seed") != BASE_SEED
        or corpus.get("pair_start") != 0
        or corpus.get("pairs") != PAIR_COUNT
        or corpus.get("inputs", {}).get("opponent_teacher_export") is not True
        or float(corpus.get("usable_fraction", 0.0)) < 0.95
    ):
        _fail("synchronized candidate-state corpus is not qualified")
    started = time.perf_counter()
    policy: list[dict[str, Any]] = []
    value: list[dict[str, Any]] = []
    task_sources = []
    observed_pairs: set[int] = set()
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
    for task in corpus.get("tasks", []):
        first_pair = int(task["first_pair"])
        pair_count = int(task["pair_count"])
        task_pairs = set(range(first_pair, first_pair + pair_count))
        if observed_pairs.intersection(task_pairs):
            _fail("complete-history task pair ranges overlap")
        observed_pairs.update(task_pairs)
        teacher_path = Path(task["opponent_teacher_path"])
        outcome_path = Path(task["outcome_path"])
        if (
            _sha256(teacher_path) != task["opponent_teacher_sha256"]
            or _sha256(outcome_path) != task["outcome_sha256"]
        ):
            _fail("complete-history task hash mismatch")
        task_policy, teacher_terminals = structured._load_teacher(teacher_path)
        task_value, outcome_terminals = structured._load_outcome(outcome_path)
        if (
            {int(row["pair_index"]) for row in task_policy} != task_pairs
            or {int(row["pair_index"]) for row in task_value} != task_pairs
        ):
            _fail("complete-history task contains the wrong pair range")
        join = structured._validate_complete_history_join(
            task_policy,
            task_value,
            teacher_terminals,
            outcome_terminals,
        )
        _merge_join(complete_join, join)
        policy.extend(task_policy)
        value.extend(task_value)
        task_sources.append(
            {
                "first_pair": first_pair,
                "pair_count": pair_count,
                "teacher_path": str(teacher_path),
                "teacher_sha256": task["opponent_teacher_sha256"],
                "outcome_path": str(outcome_path),
                "outcome_sha256": task["outcome_sha256"],
                "join": join,
            }
        )
    if observed_pairs != set(range(PAIR_COUNT)):
        _fail("complete-history cache lacks the exact 256-pair panel")
    if (
        complete_join["pair_count"] != PAIR_COUNT
        or complete_join["episode_count"] != PAIR_COUNT * 2
        or not all(
            complete_join[key]
            for key in (
                "selected_semantics_public",
                "terminal_replays_exact",
                "complete_policy_steps",
                "complete_physical_decisions",
            )
        )
    ):
        _fail("complete-history aggregate join is incomplete")
    card_max = max(int(row["object_card_ids"].max().item()) for row in policy + value)
    group_max = max(int(row["object_groups"].max().item()) for row in policy + value)
    payload = {
        "version": structured.SCRIPT_VERSION,
        "policy": policy,
        "value": value,
        "card_max": card_max,
        "group_max": group_max,
        "complete_history_join": complete_join,
        "source": {
            "corpus_report": str(args.corpus_report),
            "corpus_report_sha256": corpus_sha256,
            "tasks": task_sources,
        },
    }
    args.cache.parent.mkdir(parents=True, exist_ok=True)
    temporary = args.cache.with_name(args.cache.name + ".tmp")
    if temporary.exists():
        _fail(f"temporary cache path already exists: {temporary}")
    torch.save(payload, temporary)
    os.replace(temporary, args.cache)
    result = {
        "schema": SCHEMA,
        "status": "complete",
        "cache": str(args.cache),
        "cache_sha256": _sha256(args.cache),
        "cache_bytes": args.cache.stat().st_size,
        "corpus_report": str(args.corpus_report),
        "corpus_report_sha256": corpus_sha256,
        "policy_examples": len(policy),
        "value_examples": len(value),
        "card_max": card_max,
        "group_max": group_max,
        "complete_history_join": complete_join,
        "tasks": task_sources,
        "elapsed_seconds": time.perf_counter() - started,
    }
    _write_new_json(args.report, result)
    return result


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--corpus-report", type=Path, required=True)
    parser.add_argument("--cache", type=Path, required=True)
    parser.add_argument("--report", type=Path, required=True)
    args = parser.parse_args()
    args.corpus_report = args.corpus_report.resolve(strict=True)
    args.cache = args.cache.resolve()
    args.report = args.report.resolve()
    result = prepare(args)
    print(json.dumps(result, sort_keys=True, allow_nan=False))
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, RuntimeError, ValueError) as error:
        print(f"error: {error}", file=sys.stderr)
        raise SystemExit(2)
