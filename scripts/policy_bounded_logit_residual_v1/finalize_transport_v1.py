#!/usr/bin/env python3
"""Seal measured Rust parity into a bounded-logit successor package."""

from __future__ import annotations

import argparse
import json
from pathlib import Path
import shutil
import sys

import package_v1 as package


pipeline = package.pipeline
RESULT_SCHEMA = "mtg-kernel-structured-policy-bounded-logit-residual-transport/v1"


def _fail(message: str) -> None:
    raise ValueError(message)


def finalize(args: argparse.Namespace) -> dict:
    if args.output.exists():
        _fail("bounded-logit transport output already exists")
    if (
        not 0.0 <= args.maximum_absolute_logit_error <= pipeline.TRANSPORT_LIMIT
        or not args.parent_value_bit_exact
    ):
        _fail("measured bounded-logit transport does not pass")
    candidate_path = args.root / pipeline.CANDIDATE_FILENAME
    report_path = args.root / "report.json"
    weights_path = args.root / "weights.f32le"
    candidate = json.loads(candidate_path.read_text(encoding="utf-8"))
    report = json.loads(report_path.read_text(encoding="utf-8"))
    if (
        candidate.get("schema") != package.CANDIDATE_SCHEMA
        or report.get("schema") != package.REPORT_SCHEMA
        or candidate.get("report", {}).get("sha256") != pipeline._sha256(report_path)
        or candidate.get("weights", {}).get("sha256") != pipeline._sha256(weights_path)
    ):
        _fail("bounded-logit package binding mismatch")
    candidate_backup = args.root.parent / f"{args.root.name}.pretransport.candidate.json"
    report_backup = args.root.parent / f"{args.root.name}.pretransport.report.json"
    if candidate_backup.exists() or report_backup.exists():
        _fail("bounded-logit transport backups already exist")
    shutil.copyfile(candidate_path, candidate_backup)
    shutil.copyfile(report_path, report_backup)
    report["transport"] = {
        "maximum_absolute_logit_error": args.maximum_absolute_logit_error,
        "parent_value_bit_exact": True,
    }
    report_path.write_bytes(pipeline.history_publish._json_bytes(report))
    candidate["report"]["sha256"] = pipeline._sha256(report_path)
    candidate_path.write_bytes(pipeline.history_publish._json_bytes(candidate))
    result = {
        "schema": RESULT_SCHEMA,
        "decision": "PASS",
        "candidate_root": str(args.root),
        "maximum_absolute_logit_error": args.maximum_absolute_logit_error,
        "parent_value_bit_exact": True,
        "candidate_json_sha256": pipeline._sha256(candidate_path),
        "report_sha256": pipeline._sha256(report_path),
        "weights_sha256": pipeline._sha256(weights_path),
        "composite_model_parameter_sha256": candidate[
            "composite_model_parameter_sha256"
        ],
        "backups": {
            "candidate": str(candidate_backup),
            "report": str(report_backup),
        },
    }
    pipeline._write_new_json(args.output, result)
    return result


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, required=True)
    parser.add_argument("--maximum-absolute-logit-error", type=float, required=True)
    parser.add_argument("--parent-value-bit-exact", action="store_true")
    parser.add_argument("--output", type=Path, required=True)
    print(json.dumps(finalize(parser.parse_args()), sort_keys=True, allow_nan=False))
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, RuntimeError, ValueError) as error:
        print(f"error: {error}", file=sys.stderr)
        raise SystemExit(2)
