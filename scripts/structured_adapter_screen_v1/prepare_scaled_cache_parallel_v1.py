#!/usr/bin/env python3
"""Prepare the scaled structured cache across eight independent shard parsers."""

from __future__ import annotations

import argparse
import concurrent.futures
import gc
import hashlib
import json
import os
import sys
from pathlib import Path
from typing import Any

import torch

sys.path.insert(0, str(Path(__file__).resolve().parent))
import run_screen as screen  # noqa: E402


SCHEMA = "mtg-kernel-scaled-structured-cache-parallel/v1"
WORKERS = 8


def _fail(message: str) -> None:
    raise ValueError(message)


def _sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def _task_key(result: dict[str, Any]) -> tuple[int, int]:
    task = result["task"]
    return int(task["first_pair"]), int(task["pair_count"])


def _parse_shard(job: dict[str, Any]) -> dict[str, Any]:
    torch.set_num_threads(1)
    cache = Path(job["cache"])
    result = screen.prepare_cache(
        Path(job["teacher_path"]),
        Path(job["outcome_path"]),
        cache,
        job["teacher_sha256"],
        job["outcome_sha256"],
        True,
    )
    gc.collect()
    return {
        "task": job["task"],
        "teacher_sha256": job["teacher_sha256"],
        "outcome_sha256": job["outcome_sha256"],
        "cache": str(cache),
        "cache_sha256": _sha256(cache),
        **result,
    }


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


def prepare(
    collection_state_path: Path,
    combine_report_path: Path,
    cache: Path,
    output: Path,
) -> dict[str, Any]:
    if cache.exists() or output.exists():
        _fail("refusing to overwrite scaled cache outputs")
    state = json.loads(collection_state_path.read_text(encoding="utf-8"))
    combine = json.loads(combine_report_path.read_text(encoding="utf-8"))
    if (
        state.get("phase") != "complete"
        or state.get("pass") is not True
        or state.get("completed_pair_count") != 2_048
        or combine.get("schema") != "mtg-kernel-scaled-structured-corpus/v1"
        or combine.get("pass") is not True
        or combine.get("pair_count") != 2_048
    ):
        _fail("scaled collection or combine report is not passing")
    tasks = sorted(state["successful"], key=_task_key)
    shard_root = cache.parent / "shard-caches"
    if shard_root.exists():
        _fail("refusing to reuse an existing shard-cache directory")
    shard_root.mkdir(parents=True)
    jobs: list[dict[str, Any]] = []
    for result in tasks:
        first, count = _task_key(result)
        jobs.append(
            {
                "task": result["task"],
                "teacher_path": result["teacher"]["path"],
                "teacher_sha256": result["teacher"]["sha256"],
                "outcome_path": result["outcome"]["path"],
                "outcome_sha256": result["outcome"]["sha256"],
                "cache": str(shard_root / f"p{first:06d}-n{count:03d}.pt"),
            }
        )
    shard_reports: list[dict[str, Any]] = []
    with concurrent.futures.ProcessPoolExecutor(max_workers=WORKERS) as executor:
        futures = [executor.submit(_parse_shard, job) for job in jobs]
        for future in concurrent.futures.as_completed(futures):
            shard_reports.append(future.result())
    shard_reports.sort(key=lambda result: _task_key(result))

    policy: list[dict[str, Any]] = []
    value: list[dict[str, Any]] = []
    card_max = 0
    group_max = 0
    complete_join: dict[str, Any] = {
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
    for report in shard_reports:
        shard_path = Path(report["cache"])
        if _sha256(shard_path) != report["cache_sha256"]:
            _fail(f"shard cache hash mismatch: {shard_path}")
        payload = torch.load(shard_path, map_location="cpu", weights_only=False)
        if (
            payload.get("version") != screen.SCRIPT_VERSION
            or payload.get("source", {}).get("teacher_sha256")
            != report["teacher_sha256"]
            or payload.get("source", {}).get("outcome_sha256")
            != report["outcome_sha256"]
            or not payload.get("complete_history_join")
        ):
            _fail(f"invalid shard cache payload: {shard_path}")
        policy.extend(payload["policy"])
        value.extend(payload["value"])
        card_max = max(card_max, int(payload["card_max"]))
        group_max = max(group_max, int(payload["group_max"]))
        _merge_join(complete_join, payload["complete_history_join"])
        del payload
        gc.collect()
    policy_pairs = {int(example["pair_index"]) for example in policy}
    value_pairs = {int(example["pair_index"]) for example in value}
    expected_pairs = {
        pair
        for result in tasks
        for pair in range(_task_key(result)[0], sum(_task_key(result)))
    }
    if (
        policy_pairs != expected_pairs
        or value_pairs != expected_pairs
        or len(policy) != combine["teacher"]["decision_count"]
        or len(value) != combine["outcome"]["decision_count"]
        or complete_join["episode_count"] != 4_096
        or complete_join["pair_count"] != 2_048
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
        _fail("merged shard cache coverage mismatch")
    complete_join["selected_action_kind_counts"] = dict(
        sorted(complete_join["selected_action_kind_counts"].items())
    )
    payload = {
        "version": screen.SCRIPT_VERSION,
        "policy": policy,
        "value": value,
        "card_max": card_max,
        "group_max": group_max,
        "complete_history_join": complete_join,
        "source": {
            "teacher": combine["teacher"]["path"],
            "outcome": combine["outcome"]["path"],
            "teacher_sha256": combine["teacher"]["sha256"],
            "outcome_sha256": combine["outcome"]["sha256"],
        },
    }
    cache.parent.mkdir(parents=True, exist_ok=True)
    temporary = cache.with_suffix(cache.suffix + ".tmp")
    torch.save(payload, temporary)
    os.replace(temporary, cache)
    result = {
        "schema": SCHEMA,
        "collection_state": {
            "path": str(collection_state_path),
            "sha256": _sha256(collection_state_path),
        },
        "combine_report": {
            "path": str(combine_report_path),
            "sha256": _sha256(combine_report_path),
        },
        "cache": str(cache),
        "cache_sha256": _sha256(cache),
        "policy_examples": len(policy),
        "value_examples": len(value),
        "card_max": card_max,
        "group_max": group_max,
        "complete_history_join": complete_join,
        "workers": WORKERS,
        "shards": shard_reports,
        "pass": True,
    }
    output.write_text(
        json.dumps(result, sort_keys=True, indent=2, allow_nan=False) + "\n",
        encoding="utf-8",
        newline="\n",
    )
    return result


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--collection-state", type=Path, required=True)
    parser.add_argument("--combine-report", type=Path, required=True)
    parser.add_argument("--cache", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    result = prepare(
        args.collection_state, args.combine_report, args.cache, args.output
    )
    print(json.dumps(result, sort_keys=True, allow_nan=False))
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, RuntimeError, ValueError) as error:
        print(f"error: {error}", file=sys.stderr)
        raise SystemExit(2)
