#!/usr/bin/env python3
"""Combine disjoint full-panel bounded-logit grid shards."""

from __future__ import annotations

import argparse
import json
from pathlib import Path
from typing import Any

import screen_v1 as screen


def _fail(message: str) -> None:
    raise ValueError(message)


def combine(paths: list[Path]) -> dict[str, Any]:
    reports = [json.loads(path.read_text(encoding="utf-8")) for path in paths]
    if not reports:
        _fail("at least one shard is required")
    first = reports[0]
    identity_fields = (
        "schema",
        "method",
        "source",
        "initializer_identity",
        "rejected_state_sha256",
        "clip_grid",
        "maximum_initializer_alignment_error",
    )
    for report in reports:
        if report.get("mechanism_gate", {}).get("decision") != "SHARD":
            _fail("input is not a screen shard")
        for field in identity_fields:
            if report.get(field) != first.get(field):
                _fail(f"shard identity mismatch: {field}")
    rows = [row for report in reports for row in report["rows"]]
    if sorted(row["clip"] for row in rows) != list(screen.CLIP_GRID):
        _fail("shards do not cover the exact fixed grid")
    rows.sort(key=lambda row: row["clip"])
    safe = [row for row in rows if row["safety_gate"]["pass"]]
    selected = max(safe, key=lambda row: row["clip"]) if safe else None
    checks = {
        "safe_grid_point_exists": selected is not None,
        "selected_mean_tv_at_least_four_times_trust_projection": selected is not None
        and selected["metrics"]["overall"]["mean_total_variation"]
        >= screen.MEAN_TV_MIN,
        "selected_top_action_change_rate_at_least_four_times_trust_projection": selected is not None
        and selected["metrics"]["overall"]["top_action_change_rate"]
        >= screen.TOP_CHANGE_MIN,
    }
    return {
        "schema": first["schema"],
        "method": first["method"],
        "source": first["source"],
        "initializer_identity": first["initializer_identity"],
        "rejected_state_sha256": first["rejected_state_sha256"],
        "clip_grid": first["clip_grid"],
        "maximum_initializer_alignment_error": first[
            "maximum_initializer_alignment_error"
        ],
        "rows": rows,
        "selected_clip": None if selected is None else selected["clip"],
        "mechanism_gate": {
            "decision": "PASS" if all(checks.values()) else "REJECT",
            "checks": checks,
        },
        "shards": [
            {
                "path": str(path),
                "evaluated_clips": report["evaluated_clips"],
                "runtime_seconds": report["runtime_seconds"],
            }
            for path, report in zip(paths, reports, strict=True)
        ],
        "topology_runtime_seconds": max(
            float(report["runtime_seconds"]) for report in reports
        ),
        "summed_worker_runtime_seconds": sum(
            float(report["runtime_seconds"]) for report in reports
        ),
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--shard", type=Path, action="append", required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if args.output.exists():
        _fail(f"output already exists: {args.output}")
    result = combine(args.shard)
    args.output.write_text(
        json.dumps(result, indent=2, sort_keys=True, allow_nan=False) + "\n",
        encoding="utf-8",
        newline="\n",
    )
    print(json.dumps(result, sort_keys=True, allow_nan=False))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
