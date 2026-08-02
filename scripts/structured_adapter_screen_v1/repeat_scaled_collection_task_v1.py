#!/usr/bin/env python3
"""Repeat the first successful scaled collection task and compare exact bytes."""

from __future__ import annotations

import argparse
import json
import sys
from argparse import Namespace
from pathlib import Path
from typing import Any

sys.path.insert(0, str(Path(__file__).resolve().parent))
import collect_scaled_structured_corpus_v1 as collector  # noqa: E402


SCHEMA = "mtg-kernel-scaled-structured-corpus-repeat/v1"


def _fail(message: str) -> None:
    raise ValueError(message)


def repeat(evidence_root: Path, task_first_pair: int | None) -> dict[str, Any]:
    report_path = evidence_root / "repeat" / "repeat-report.json"
    if report_path.exists():
        _fail("refusing to overwrite an existing repeat report")
    state_path = evidence_root / "collection-state.json"
    state = json.loads(state_path.read_text(encoding="utf-8"))
    if state.get("schema") != collector.SCHEMA:
        _fail("collection state schema mismatch")
    successful = sorted(
        state.get("successful", []),
        key=lambda result: (result["task"]["first_pair"], result["task"]["pair_count"]),
    )
    if not successful:
        _fail("collection has no successful task to repeat")
    if task_first_pair is None:
        original = successful[0]
    else:
        matches = [
            result
            for result in successful
            if result["task"]["first_pair"] == task_first_pair
        ]
        if len(matches) != 1:
            _fail(f"expected one successful task at pair {task_first_pair}")
        original = matches[0]
    original_task = collector._task_from_dict(original["task"])
    task = collector.Task(
        original_task.first_pair,
        original_task.pair_count,
        "repeat",
        original_task.replaces_pair,
    )
    config = state.get("config", {})
    args = Namespace(
        evidence_root=evidence_root / "repeat",
        mage_repo=Path(config["mage_repo"]),
        scorer_exe=Path(config["scorer_exe"]),
        outcome_root=Path(config["outcome_root"]),
        source_database=Path(config["source_database"]),
        maven=Path(config["maven"]),
    )
    if (
        not args.mage_repo.is_dir()
        or not args.scorer_exe.is_file()
        or not args.outcome_root.is_dir()
        or not args.source_database.is_file()
        or not args.maven.is_file()
        or collector._sha256(args.scorer_exe) != collector.SCORER_SHA256
        or collector._sha256(args.source_database) != collector.CARD_DB_SHA256
    ):
        _fail("repeat input validation failed")
    database = collector._worker_database(
        args.evidence_root, 0, args.source_database
    )
    rerun = collector._run_task(task, 0, args, database)
    if rerun.get("status") != "success":
        _fail(f"repeat task failed: {rerun}")
    comparisons = {
        kind: {
            "original_path": original[kind]["path"],
            "original_sha256": original[kind]["sha256"],
            "repeat_path": rerun[kind]["path"],
            "repeat_sha256": rerun[kind]["sha256"],
            "byte_identical": original[kind]["sha256"] == rerun[kind]["sha256"],
        }
        for kind in ("teacher", "outcome")
    }
    result = {
        "schema": SCHEMA,
        "task": collector._task_dict(original_task),
        "comparisons": comparisons,
        "repeat_log": {
            "path": rerun["log"],
            "sha256": rerun["log_sha256"],
        },
        "elapsed_seconds": rerun["elapsed_seconds"],
        "pass": all(value["byte_identical"] for value in comparisons.values()),
    }
    collector._atomic_json(report_path, result)
    return result


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--evidence-root", type=Path, required=True)
    parser.add_argument("--task-first-pair", type=int)
    args = parser.parse_args()
    result = repeat(args.evidence_root, args.task_first_pair)
    print(json.dumps(result, sort_keys=True, allow_nan=False))
    return 0 if result["pass"] else 2


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (KeyError, OSError, RuntimeError, ValueError) as error:
        print(f"error: {error}", file=sys.stderr)
        raise SystemExit(2)
