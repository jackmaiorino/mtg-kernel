#!/usr/bin/env python3
"""Seal measured native transport into a staged structured successor package."""

from __future__ import annotations

import argparse
import json
import os
import shutil
import tempfile
from pathlib import Path
from typing import Any

import fit_complete_history_live_candidate_v1 as history_publish
import fit_policy_live_candidate as live
import fit_policy_only_structured_successor_v1 as successor


RESULT_SCHEMA = "mtg-kernel-structured-policy-successor-transport-result/v1"
MAXIMUM_ERROR = 3.0e-5


def _fail(message: str) -> None:
    raise ValueError(message)


def _load_json(path: Path) -> dict[str, Any]:
    value = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(value, dict):
        _fail(f"{path} is not a JSON object")
    return value


def _atomic_json(path: Path, value: Any) -> None:
    payload = history_publish._json_bytes(value)
    with tempfile.NamedTemporaryFile(
        mode="wb", dir=path.parent, prefix=path.name + ".", suffix=".tmp", delete=False
    ) as handle:
        temporary = Path(handle.name)
        handle.write(payload)
        handle.flush()
        os.fsync(handle.fileno())
    os.replace(temporary, path)


def finalize(args: argparse.Namespace) -> dict[str, Any]:
    if args.output.exists():
        _fail(f"refusing to overwrite {args.output}")
    if not args.maximum_absolute_logit_error >= 0.0:
        _fail("maximum absolute logit error must be finite and nonnegative")
    if args.maximum_absolute_logit_error > MAXIMUM_ERROR:
        _fail("maximum absolute logit error exceeds 3e-5")
    if not args.parent_value_bit_exact:
        _fail("parent value was not bit exact")
    candidate_path = args.root / successor.CANDIDATE_FILENAME
    report_path = args.root / "report.json"
    weights_path = args.root / "weights.f32le"
    candidate = _load_json(candidate_path)
    report = _load_json(report_path)
    if candidate.get("schema") != successor.SCHEMA:
        _fail("candidate schema mismatch")
    if report.get("schema") != successor.REPORT_SCHEMA:
        _fail("report schema mismatch")
    if candidate.get("report", {}).get("sha256") != live._sha256(report_path):
        _fail("candidate does not bind the current report")
    if candidate.get("weights", {}).get("sha256") != live._sha256(weights_path):
        _fail("candidate does not bind the current weights")

    before = {
        "candidate_json_sha256": live._sha256(candidate_path),
        "report_sha256": live._sha256(report_path),
    }
    candidate_backup = Path(str(args.root) + ".pretransport.candidate.json")
    report_backup = Path(str(args.root) + ".pretransport.report.json")
    if candidate_backup.exists() or report_backup.exists():
        _fail("pretransport backup already exists")
    shutil.copyfile(candidate_path, candidate_backup)
    shutil.copyfile(report_path, report_backup)

    report["transport"] = {
        "maximum_absolute_logit_error": args.maximum_absolute_logit_error,
        "parent_value_bit_exact": True,
    }
    _atomic_json(report_path, report)
    candidate["report"]["sha256"] = live._sha256(report_path)
    _atomic_json(candidate_path, candidate)
    after = {
        "candidate_json_sha256": live._sha256(candidate_path),
        "report_sha256": live._sha256(report_path),
        "weights_sha256": live._sha256(weights_path),
        "composite_model_parameter_sha256": candidate[
            "composite_model_parameter_sha256"
        ],
    }
    result = {
        "schema": RESULT_SCHEMA,
        "decision": "PASS",
        "candidate_root": str(args.root),
        "maximum_absolute_logit_error": args.maximum_absolute_logit_error,
        "parent_value_bit_exact": True,
        "before": before,
        "after": after,
        "backups": {
            "candidate": str(candidate_backup),
            "candidate_sha256": live._sha256(candidate_backup),
            "report": str(report_backup),
            "report_sha256": live._sha256(report_backup),
        },
    }
    _atomic_json(args.output, result)
    return result


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, required=True)
    parser.add_argument("--maximum-absolute-logit-error", type=float, required=True)
    parser.add_argument("--parent-value-bit-exact", action="store_true")
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    print(json.dumps(finalize(args), sort_keys=True, allow_nan=False))
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, RuntimeError, ValueError) as error:
        print(f"error: {error}", file=__import__("sys").stderr)
        raise SystemExit(2)
