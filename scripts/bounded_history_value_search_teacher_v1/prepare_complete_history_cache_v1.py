#!/usr/bin/env python3
"""Prepare the exact structured complete-history cache for search-teacher labels."""

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


SCHEMA = "mtg-kernel-depth8-bounded-value-search-teacher-complete-history-cache/v1"
CORPUS_SCHEMA = "mtg-kernel-depth8-bounded-value-search-teacher-corpus/v1"
BASE_SEED = 1_400_001
PAIR_START = 0
PAIR_COUNT = 256
SHADOW_NONINTERFERENCE = "byte_identical_export_streams"

PATH_HASH_FIELDS = (
    ("reference_log_path", "reference_log_sha256"),
    ("reference_teacher_path", "reference_teacher_sha256"),
    ("reference_outcome_path", "reference_outcome_sha256"),
    ("log_path", "log_sha256"),
    ("teacher_path", "teacher_sha256"),
    ("outcome_path", "outcome_sha256"),
)


def _fail(message: str) -> None:
    raise RuntimeError(message)


def _sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def _is_sha256(value: Any) -> bool:
    if not isinstance(value, str) or len(value) != 64:
        return False
    try:
        int(value, 16)
    except ValueError:
        return False
    return True


def _write_fresh_json(path: Path, value: Any) -> None:
    if path.exists():
        _fail(f"refusing to overwrite {path}")
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_name(path.name + ".tmp")
    if temporary.exists():
        _fail(f"temporary report path already exists: {temporary}")
    try:
        with temporary.open("x", encoding="utf-8", newline="\n") as handle:
            json.dump(value, handle, indent=2, sort_keys=True, allow_nan=False)
            handle.write("\n")
        os.replace(temporary, path)
    finally:
        if temporary.exists():
            temporary.unlink()


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
        target[key] = target[key] and source.get(key) is True


def _load_report(path: Path) -> tuple[dict[str, Any], str]:
    report_sha256 = _sha256(path)
    try:
        report = json.loads(path.read_text(encoding="utf-8"))
    except json.JSONDecodeError as error:
        _fail(f"collector report is not valid JSON: {error}")
    if not isinstance(report, dict):
        _fail("collector report must be an object")
    if (
        report.get("schema") != CORPUS_SCHEMA
        or report.get("status") != "complete"
        or report.get("base_seed") != BASE_SEED
        or report.get("pair_start") != PAIR_START
        or report.get("pairs") != PAIR_COUNT
    ):
        _fail("collector report is not the exact 256-pair search-teacher panel")
    quality_gate = report.get("target_quality_gate")
    by_seat = report.get("by_candidate_seat")
    if (
        not isinstance(quality_gate, dict)
        or quality_gate.get("minimum_equal_episode_mass_override_rate_per_seat") != 0.05
        or quality_gate.get("pass") is not True
        or report.get("advance_to_training") is not True
        or not isinstance(by_seat, dict)
        or any(
            not isinstance(by_seat.get(seat), dict)
            or by_seat[seat].get("equal_episode_mass_override_rate", -1.0) < 0.05
            for seat in ("p0", "p1")
        )
    ):
        _fail("collector target-quality gate is not a valid pass")
    tasks = report.get("tasks")
    if not isinstance(tasks, list) or not tasks:
        _fail("collector report has no tasks")
    return report, report_sha256


def _validate_task_files(task: dict[str, Any]) -> dict[str, Path]:
    if task.get("shadow_noninterference") != SHADOW_NONINTERFERENCE:
        _fail("task shadow_noninterference binding is not byte_identical_export_streams")
    paths: dict[str, Path] = {}
    observed_hashes: dict[str, str] = {}
    for path_key, hash_key in PATH_HASH_FIELDS:
        raw_path = task.get(path_key)
        expected_hash = task.get(hash_key)
        if not isinstance(raw_path, str) or not raw_path:
            _fail(f"task is missing {path_key}")
        if not _is_sha256(expected_hash):
            _fail(f"task has invalid {hash_key}")
        path = Path(raw_path)
        if not path.is_file():
            _fail(f"task path does not identify a file: {path}")
        observed_hash = _sha256(path)
        if observed_hash != expected_hash:
            _fail(f"task hash mismatch for {path_key}: {path}")
        paths[path_key] = path
        observed_hashes[path_key] = observed_hash

    if (
        observed_hashes["reference_teacher_path"]
        != observed_hashes["teacher_path"]
        or observed_hashes["reference_outcome_path"]
        != observed_hashes["outcome_path"]
    ):
        _fail("shadow teacher/outcome exports are not byte-identical to references")
    return paths


def _task_pair_range(task: dict[str, Any]) -> tuple[int, int, set[int]]:
    first_pair = task.get("first_pair")
    pair_count = task.get("pair_count")
    if type(first_pair) is not int or type(pair_count) is not int:
        _fail("task pair range is not integer-valued")
    if pair_count < 1 or first_pair < PAIR_START:
        _fail("task pair range is invalid")
    stop = first_pair + pair_count
    if stop > PAIR_START + PAIR_COUNT:
        _fail("task pair range exceeds the exact collector panel")
    return first_pair, pair_count, set(range(first_pair, stop))


def _example_pairs(rows: list[dict[str, Any]], lane: str) -> set[int]:
    pairs: set[int] = set()
    for row in rows:
        try:
            pair = int(row["pair_index"])
        except (KeyError, TypeError, ValueError) as error:
            _fail(f"{lane} example has no integer pair_index: {error}")
        pairs.add(pair)
    return pairs


def _initial_join() -> dict[str, Any]:
    return {
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


def prepare(args: argparse.Namespace) -> dict[str, Any]:
    if args.cache.exists() or args.report.exists():
        _fail("cache or report output already exists")
    cache_temporary = args.cache.with_name(args.cache.name + ".tmp")
    report_temporary = args.report.with_name(args.report.name + ".tmp")
    if cache_temporary.exists() or report_temporary.exists():
        _fail("temporary cache or report output already exists")

    corpus, corpus_sha256 = _load_report(args.collector_report)
    expected_pairs = set(range(PAIR_START, PAIR_START + PAIR_COUNT))
    observed_pairs: set[int] = set()
    policy: list[dict[str, Any]] = []
    value: list[dict[str, Any]] = []
    task_sources: list[dict[str, Any]] = []
    complete_join = _initial_join()
    started = time.perf_counter()

    try:
        raw_tasks = corpus["tasks"]
        if any(not isinstance(task, dict) for task in raw_tasks):
            _fail("collector task is not an object")
        tasks = sorted(raw_tasks, key=lambda task: int(task.get("first_pair", -1)))
        for task in tasks:
            first_pair, pair_count, task_pairs = _task_pair_range(task)
            if observed_pairs.intersection(task_pairs):
                _fail("collector task pair ranges overlap")
            observed_pairs.update(task_pairs)
            paths = _validate_task_files(task)

            task_policy, teacher_terminals = structured._load_teacher(paths["teacher_path"])
            task_value, outcome_terminals = structured._load_outcome(paths["outcome_path"])
            if not isinstance(task_policy, list) or not isinstance(task_value, list):
                _fail("structured loaders did not return policy/value lists")
            if _example_pairs(task_policy, "policy") != task_pairs:
                _fail(f"policy lane does not cover task pair range {first_pair}:{pair_count}")
            if _example_pairs(task_value, "value") != task_pairs:
                _fail(f"value lane does not cover task pair range {first_pair}:{pair_count}")
            join = structured._validate_complete_history_join(
                task_policy,
                task_value,
                teacher_terminals,
                outcome_terminals,
            )
            if not isinstance(join, dict):
                _fail("complete-history join did not return metadata")
            _merge_join(complete_join, join)
            policy.extend(task_policy)
            value.extend(task_value)
            task_sources.append(
                {
                    "first_pair": first_pair,
                    "pair_count": pair_count,
                    "shadow_noninterference": task["shadow_noninterference"],
                    **{
                        path_key: str(paths[path_key])
                        for path_key, _ in PATH_HASH_FIELDS
                    },
                    **{
                        hash_key: task[hash_key]
                        for _, hash_key in PATH_HASH_FIELDS
                    },
                    "join": join,
                }
            )

        if observed_pairs != expected_pairs:
            _fail("collector tasks do not cover the exact 256-pair panel")
        if _example_pairs(policy, "aggregate policy") != expected_pairs:
            _fail("aggregate policy lane does not cover the exact 256-pair panel")
        if _example_pairs(value, "aggregate value") != expected_pairs:
            _fail("aggregate value lane does not cover the exact 256-pair panel")
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
            _fail("aggregate complete-history join is incomplete")

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
                "collector_report": str(args.collector_report),
                "collector_report_sha256": corpus_sha256,
                "base_seed": BASE_SEED,
                "pair_start": PAIR_START,
                "pairs": PAIR_COUNT,
                "tasks": task_sources,
            },
        }
        args.cache.parent.mkdir(parents=True, exist_ok=True)
        torch.save(payload, cache_temporary)
        cache_sha256 = _sha256(cache_temporary)
        result = {
            "schema": SCHEMA,
            "status": "complete",
            "cache": str(args.cache),
            "cache_sha256": cache_sha256,
            "cache_bytes": cache_temporary.stat().st_size,
            "collector_report": str(args.collector_report),
            "collector_report_sha256": corpus_sha256,
            "base_seed": BASE_SEED,
            "pair_start": PAIR_START,
            "pairs": PAIR_COUNT,
            "policy_examples": len(policy),
            "value_examples": len(value),
            "card_max": card_max,
            "group_max": group_max,
            "complete_history_join": complete_join,
            "tasks": task_sources,
            "elapsed_seconds": time.perf_counter() - started,
        }
        args.report.parent.mkdir(parents=True, exist_ok=True)
        with report_temporary.open("x", encoding="utf-8", newline="\n") as handle:
            json.dump(result, handle, indent=2, sort_keys=True, allow_nan=False)
            handle.write("\n")
        os.replace(cache_temporary, args.cache)
        os.replace(report_temporary, args.report)
        return result
    finally:
        if cache_temporary.exists():
            cache_temporary.unlink()
        if report_temporary.exists():
            report_temporary.unlink()


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--collector-report", type=Path, required=True)
    parser.add_argument("--cache", type=Path, required=True)
    parser.add_argument("--report", type=Path, required=True)
    args = parser.parse_args(argv)
    args.collector_report = args.collector_report.resolve(strict=True)
    args.cache = args.cache.resolve()
    args.report = args.report.resolve()
    result = prepare(args)
    print(json.dumps(result, sort_keys=True, allow_nan=False))
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, RuntimeError, ValueError, KeyError) as error:
        print(f"error: {error}", file=sys.stderr)
        raise SystemExit(2)
