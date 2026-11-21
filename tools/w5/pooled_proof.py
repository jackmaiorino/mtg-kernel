#!/usr/bin/env python3
"""Pooled-lineage proof: compares a pooled run with a single-process run of the same recipe.

A checkpoint file records its run's trajectory paths, and a trajectory records the hash of the
checkpoint file it was played from. Two runs therefore publish equal trajectory bytes only when
they ran under the same run root path. The after-states (model state hashes) never depend on paths.

Procedure (same root, one host):
    native_expanded_training_run_v1 CONFIG.json                  # reference, output ROOT
    mv ROOT ROOT.reference
    native_expanded_training_run_v1 CONFIG.json --collect-external DISPATCHER   # pooled, output ROOT
    pooled_proof.py compare --same-root ROOT.reference ROOT
or run the two at the same ROOT on two hosts and compare copies of their receipts.

`compare` checks the ordered completion receipts against each `run.json` plan. The episode
schedule and starting and ending states must match, and each pooled iteration must use the
pooled backend. With `--same-root` (the operator states that both runs used the same root), the
resolved episode digest and ordered trajectory hashes must also match. Trajectory pins are
compared from the receipts, so their payload files need not be present.
Exit 0 on pass, 1 on a difference, 2 on bad input.
"""
from __future__ import annotations

import hashlib
import json
import re
import sys
from pathlib import Path

POOLED_BACKEND = "pooled-collection-shards-v1"
LOCAL_BACKENDS = (None, "native-cpu-sequential", "native-cpu-parallel-episodes-v1")
RUN_SCHEMA = "mtg-kernel-native-expanded-training-run/v1"

# The native trainer formats its attempt number with a minimum width of six.
ATTEMPT = r"attempt-(?:[0-9]{6}|[1-9][0-9]{6,})"


def completion(root: Path) -> dict:
    """Read the native trainer's ordered pins of published iteration receipts."""
    path = Path(root) / "completion.json"
    if not path.is_file():
        raise ValueError(f"{root}: no completion.json (the run did not finish)")
    document = json.loads(path.read_text())
    if document.get("schema") != RUN_SCHEMA or not isinstance(document.get("iterations"), list):
        raise ValueError(f"{path}: invalid native run completion")
    return document


def pinned_json(iteration: Path, pin: dict, relative_pattern: str) -> dict:
    """Relocate a receipt only within its expected iteration and native output layout."""
    _, marker, relative = pin["path"].replace("\\", "/").rpartition(f"/iterations/{iteration.name}/")
    if not marker or not re.fullmatch(relative_pattern, relative):
        raise ValueError(f"{pin['path']}: unexpected receipt path for iteration {iteration.name}")
    path = iteration / relative
    data = path.read_bytes()
    if hashlib.sha256(data).hexdigest() != pin["sha256"]:
        raise ValueError(f"{path}: differs from the iteration receipt's pin")
    return json.loads(data)


def is_sha256(value) -> bool:
    return isinstance(value, str) and len(value) == 64 and all(c in "0123456789abcdef" for c in value)


def iterations(root: Path, complete_pins: list, schedule: list) -> dict:
    """Iteration index -> the receipts its `complete.json` pins. The collection and the update
    can sit in different attempts (a resumed iteration reuses a published collection), so
    both are followed through the completion receipt, never guessed from the attempt layout."""
    out = {}
    base = Path(root) / "iterations"
    if not base.is_dir():
        raise ValueError(f"{root}: no iterations directory")
    for index, pin in enumerate(complete_pins):
        iteration = base / f"{index:06d}"
        receipt = pinned_json(iteration, pin, r"complete\.json")
        if (receipt.get("schema") != "mtg-kernel-native-expanded-iteration/v1"
                or type(receipt.get("iteration")) is not int or receipt["iteration"] != index):
            raise ValueError(f"{iteration}: completion receipt names another iteration")
        update = pinned_json(iteration, receipt["update"], rf"{ATTEMPT}/update/update\.json")
        collection = pinned_json(iteration, receipt["collection"], rf"{ATTEMPT}/collect/collection\.json")
        after = update["after_state_sha256"]
        before = update["before_state_sha256"]
        previous = out.get(f"{index - 1:06d}")
        checkpoint = update["checkpoint"]["sha256"]
        resolved = receipt["episodes_sha256"]
        if (update["complete"] is not True or collection["complete"] is not True
                or not all(map(is_sha256, (after, before, checkpoint, resolved)))
                or receipt["output_identity"]["state_sha256"] != after
                or receipt["output_identity"]["checkpoint_sha256"] != checkpoint
                or collection["behavior_state_sha256"] != before
                or (previous is not None and previous["after_state_sha256"] != before)):
            raise ValueError(f"{iteration}: collection or update differs from the committed states")
        if index >= len(schedule):
            raise ValueError(f"{iteration}: iteration exceeds the recorded plan")
        episode_schedule = schedule[index]["episodes"]
        pins = update["trajectories"]
        if (not pins or pins != collection["trajectories"]
                or len(pins) != len(episode_schedule)
                or any(not is_sha256(pin["sha256"])
                       or Path(pin["path"].replace("\\", "/")).name != f"episode-{slot:04d}.json"
                       for slot, pin in enumerate(pins))):
            raise ValueError(f"{iteration}: trajectory pins differ from the planned collection")
        out[iteration.name] = {
            "output_identity": receipt["output_identity"],
            "before_state_sha256": before,
            "after_state_sha256": after,
            "episode_schedule": episode_schedule,
            "resolved_episodes_sha256": resolved,
            "trajectories": [pin["sha256"] for pin in pins],
            "collection_backend": collection.get("collection_backend"),
        }
    return out


def run(root: Path) -> tuple[dict, int, bool]:
    done = completion(root)
    plan = json.loads((root / "run.json").read_bytes())
    if plan.get("schema") != RUN_SCHEMA or not isinstance(plan["config"].get("iterations"), list):
        raise ValueError(f"{root}: invalid native iteration plan")
    schedule = plan["config"]["iterations"]
    rows = iterations(root, done["iterations"], schedule)
    count = len(schedule)
    complete = (count > 0 and done.get("complete") is True
                and type(done.get("planned_iterations")) is int and type(done.get("completed_iterations")) is int
                and done["planned_iterations"] == done["completed_iterations"] == len(rows) == count
                and done["actual_identity"] == rows[f"{count - 1:06d}"]["output_identity"])
    return rows, count, complete


def compare(reference: Path, pooled: Path, same_root: bool = False) -> dict:
    ref, planned, ref_complete = run(reference)
    pool, pool_planned, pool_complete = run(pooled)
    rows = []
    runs_ok = ref_complete and pool_complete and planned == pool_planned
    for name in sorted(set(ref) | set(pool)):
        a, b = ref.get(name), pool.get(name)
        if a is None or b is None:
            rows.append({"iteration": name, "missing_in": "reference" if a is None else "pooled"})
            continue
        states = a["after_state_sha256"] == b["after_state_sha256"]
        trajectories = a["trajectories"] == b["trajectories"]
        same_start = a["before_state_sha256"] == b["before_state_sha256"]
        same_schedule = a["episode_schedule"] == b["episode_schedule"]
        same_resolved = a["resolved_episodes_sha256"] == b["resolved_episodes_sha256"]
        ok = (states and same_start and same_schedule and a["collection_backend"] in LOCAL_BACKENDS
              and b["collection_backend"] == POOLED_BACKEND
              and (not same_root or (trajectories and same_resolved)))
        row = {"iteration": name, "after_state_equal": states, "trajectories_equal": trajectories,
               "reference_backend": a["collection_backend"], "pooled_backend": b["collection_backend"],
               "pass": ok}
        if not ok:
            row.update(before_state_equal=same_start, episode_schedule_equal=same_schedule,
                       resolved_episodes_equal=same_resolved)
        rows.append(row)
    passed = sum(row.get("pass", False) for row in rows)
    return {"schema": "mtg-kernel-pooled-proof/v1", "same_root": same_root, "runs_complete": runs_ok,
            "planned_iterations": planned, "iterations": len(rows),
            "passed": passed, "pass": runs_ok and passed == len(rows), "rows": rows}


def main(argv: list[str]) -> int:
    args = argv[2:]
    same_root = "--same-root" in args
    args = [a for a in args if a != "--same-root"]
    if len(argv) < 2 or argv[1] != "compare" or len(args) != 2:
        print("usage: pooled_proof.py compare [--same-root] REFERENCE_ROOT POOLED_ROOT", file=sys.stderr)
        return 2
    try:
        report = compare(Path(args[0]), Path(args[1]), same_root)
    except (OSError, ValueError, KeyError) as error:
        print(f"pooled proof: {error}", file=sys.stderr)
        return 2
    print(json.dumps(report, indent=1))
    return 0 if report["pass"] else 1


if __name__ == "__main__":
    sys.exit(main(sys.argv))
