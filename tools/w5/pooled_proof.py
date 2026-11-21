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

`compare` passes when every iteration's after-state is equal. With `--same-root` (the operator
states that both runs used the same root) every trajectory hash must be equal too; otherwise they
are reported only. Whether the roots match is never inferred from the receipts: a retried attempt
changes recorded paths without changing the root. Exit 0 on pass, 1 on a difference, 2 on bad input.
"""
from __future__ import annotations

import json
import sys
from pathlib import Path


def iterations(root: Path) -> dict:
    """Iteration index -> receipts of its completed attempt (the one with update.json)."""
    out = {}
    base = Path(root) / "iterations"
    if not base.is_dir():
        raise ValueError(f"{root}: no iterations directory")
    for iteration in sorted(p for p in base.iterdir() if p.is_dir()):
        attempts = sorted(iteration.glob("attempt-*/update/update.json"))
        if not attempts:
            continue
        update = json.loads(attempts[-1].read_text())
        collection_path = attempts[-1].parent.parent / "collect" / "collection.json"
        collection = json.loads(collection_path.read_text()) if collection_path.exists() else {}
        out[iteration.name] = {
            "after_state_sha256": update["after_state_sha256"],
            "checkpoint_path": update.get("checkpoint", {}).get("path"),
            "trajectories": [t["sha256"] for t in update.get("trajectories", [])],
            "collection_backend": collection.get("collection_backend"),
        }
    return out


def compare(reference: Path, pooled: Path, same_root: bool = False) -> dict:
    ref, pool = iterations(reference), iterations(pooled)
    rows, failures = [], 0
    for name in sorted(set(ref) | set(pool)):
        a, b = ref.get(name), pool.get(name)
        if a is None or b is None:
            rows.append({"iteration": name, "missing_in": "reference" if a is None else "pooled"})
            failures += 1
            continue
        states = a["after_state_sha256"] == b["after_state_sha256"]
        trajectories = a["trajectories"] == b["trajectories"]
        ok = states and (trajectories or not same_root)
        failures += not ok
        rows.append({"iteration": name, "after_state_equal": states, "trajectories_equal": trajectories,
                     "pooled_backend": b["collection_backend"], "pass": ok})
    return {"schema": "mtg-kernel-pooled-proof/v1", "same_root": same_root, "iterations": len(rows),
            "passed": len(rows) - failures, "pass": failures == 0 and bool(rows), "rows": rows}


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
