#!/usr/bin/env python3
"""Merge chunked V4 outcome-v2 shards into one canonical GAE8 corpus."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import sys
from typing import Any

from outcome_v2 import (
    EXPECTED_CHECKPOINT,
    EXPECTED_HEADER,
    OUTCOME_CONTRACT,
    canonical_json_bytes,
    exclusive_write,
    iter_jsonl,
    sha256,
    validate_outcome_shard,
    write_json_row,
)


REPORT_SCHEMA = "mtg-kernel-current-net8-cp7-terminal-response-v4-corpus/v1"
SHARD_NAME = re.compile(r"gae8-p([0-9]{6,})-n([0-9]{3})\.outcome\.jsonl\Z")


def fail(message: str) -> None:
    raise ValueError(message)


def _discover(input_root: Path) -> list[tuple[Path, int, int]]:
    if not input_root.is_dir():
        fail(f"input root is not a directory: {input_root}")
    paths = sorted(input_root.glob("*.outcome.jsonl"))
    if not paths:
        fail(f"input root contains no outcome shards: {input_root}")
    shards: list[tuple[Path, int, int]] = []
    for path in paths:
        match = SHARD_NAME.fullmatch(path.name)
        if match is None:
            fail(f"unexpected outcome shard name: {path.name}")
        shards.append((path.resolve(), int(match.group(1)), int(match.group(2))))
    shards.sort(key=lambda item: (item[1], item[2], item[0].name))
    return shards


def _validate_inventory(
    shards: list[tuple[Path, int, int]],
    *,
    base_seed: int,
    first_pair: int,
    expected_pairs: int,
) -> list[dict[str, Any]]:
    expected_next = first_pair
    reports: list[dict[str, Any]] = []
    for path, shard_first, shard_pairs in shards:
        if shard_first != expected_next:
            fail(
                "outcome shard range is missing, overlapping, or reordered at "
                f"pair {expected_next}: {path.name}"
            )
        report = validate_outcome_shard(
            path,
            base_seed=base_seed,
            first_pair=shard_first,
            pair_count=shard_pairs,
        )
        if report["first_pair"] != shard_first or report["pair_count"] != shard_pairs:
            fail(f"shard filename range differs from content: {path.name}")
        reports.append(report)
        expected_next += shard_pairs
    if expected_next != first_pair + expected_pairs:
        fail(
            f"exact pair count mismatch: covered {expected_next - first_pair}, "
            f"expected {expected_pairs}"
        )
    return reports


def _write_merged(
    shards: list[tuple[Path, int, int]],
    reports: list[dict[str, Any]],
    output: Path,
) -> tuple[int, int]:
    output.parent.mkdir(parents=True, exist_ok=True)
    descriptor = os.open(output, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o644)
    global_record_ordinal = 0
    decision_offset = 0
    try:
        with os.fdopen(descriptor, "w", encoding="utf-8", newline="\n") as handle:
            header = dict(EXPECTED_HEADER)
            header["record_ordinal"] = global_record_ordinal
            write_json_row(handle, header)
            global_record_ordinal += 1
            for (path, _, _), report in zip(shards, reports, strict=True):
                rows = iter_jsonl(path)
                shard_header = next(rows)
                if shard_header != EXPECTED_HEADER:
                    fail(f"input changed after validation: {path}")
                for original in rows:
                    row = dict(original)
                    row["record_ordinal"] = global_record_ordinal
                    if row.get("record_type") == "decision":
                        row["outcome_decision_ordinal"] = (
                            decision_offset + original["outcome_decision_ordinal"]
                        )
                    elif row.get("record_type") == "terminal":
                        first = original.get("first_outcome_decision_ordinal")
                        row["first_outcome_decision_ordinal"] = (
                            None if first is None else decision_offset + first
                        )
                    else:
                        fail(f"input changed to unknown record type: {path}")
                    write_json_row(handle, row)
                    global_record_ordinal += 1
                if sha256(path) != report["sha256"]:
                    fail(f"input changed while merging: {path}")
                decision_offset += report["decision_count"]
            handle.flush()
            os.fsync(handle.fileno())
    except BaseException:
        try:
            output.unlink()
        except FileNotFoundError:
            pass
        raise
    return global_record_ordinal, decision_offset


def merge(
    *,
    input_root: Path,
    output_jsonl: Path,
    report_path: Path,
    base_seed: int,
    first_pair: int,
    expected_pairs: int,
) -> dict[str, Any]:
    if (
        not 0 <= base_seed <= 0x7FFF_FFFF_FFFF_FFFF
        or first_pair < 0
        or not 1 <= expected_pairs <= 4096
    ):
        fail("invalid expected seed or pair range")
    output_jsonl = output_jsonl.resolve()
    report_path = report_path.resolve()
    if output_jsonl == report_path:
        fail("corpus and report paths must differ")
    if output_jsonl.exists() or report_path.exists():
        fail("corpus and report outputs must both be absent")
    shards = _discover(input_root.resolve())
    input_paths = {path for path, _, _ in shards}
    if output_jsonl in input_paths or report_path in input_paths:
        fail("outputs may not replace input shards")
    reports = _validate_inventory(
        shards,
        base_seed=base_seed,
        first_pair=first_pair,
        expected_pairs=expected_pairs,
    )
    record_count, decision_count = _write_merged(
        shards, reports, output_jsonl
    )
    merged_validation = validate_outcome_shard(
        output_jsonl,
        base_seed=base_seed,
        first_pair=first_pair,
        pair_count=expected_pairs,
    )
    if (
        merged_validation["record_count"] != record_count
        or merged_validation["decision_count"] != decision_count
        or merged_validation["episode_count"] != expected_pairs * 2
    ):
        fail("merged corpus validation counts differ from writer counts")
    source_reports = [
        {
            key: value
            for key, value in report.items()
            if key != "header"
        }
        for report in reports
    ]
    report: dict[str, Any] = {
        "schema": REPORT_SCHEMA,
        "status": "complete",
        "authority_kind": EXPECTED_CHECKPOINT["authority_kind"],
        "export_contract": OUTCOME_CONTRACT,
        "checkpoint_identity": EXPECTED_CHECKPOINT,
        "base_seed": base_seed,
        "first_pair": first_pair,
        "pair_count": expected_pairs,
        "episode_count": expected_pairs * 2,
        "decision_count": decision_count,
        "record_count_including_header": record_count,
        "output": {
            "path": str(output_jsonl),
            "sha256": sha256(output_jsonl),
            "byte_count": output_jsonl.stat().st_size,
        },
        "inputs": source_reports,
        "input_sha256_by_filename": {
            Path(source["path"]).name: source["sha256"] for source in source_reports
        },
        "ordinal_rewrite": {
            "record_ordinal": "contiguous_global_zero_based",
            "outcome_decision_ordinal": "contiguous_global_zero_based",
            "first_outcome_decision_ordinal": "shifted_to_global_or_null",
        },
        "analysis_policy": {
            "outcome_based_early_analysis": False,
            "terminal_values_used_only_for_contract_validation": True,
        },
    }
    exclusive_write(report_path, canonical_json_bytes(report, indent=2))
    return report


def _self_test() -> int:
    match = SHARD_NAME.fullmatch("gae8-p000032-n032.outcome.jsonl")
    if match is None or match.groups() != ("000032", "032"):
        fail("shard filename self-test failed")
    if EXPECTED_HEADER["checkpoint"] != EXPECTED_CHECKPOINT:
        fail("header checkpoint self-test failed")
    print("merge_corpus_v4: SELF-TEST PASS")
    return 0


def _parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--input-root", type=Path)
    parser.add_argument("--output-jsonl", type=Path)
    parser.add_argument("--report", type=Path)
    parser.add_argument("--base-seed", type=int, default=1_930_001)
    parser.add_argument("--first-pair", type=int, default=0)
    parser.add_argument("--expected-pairs", type=int, default=256)
    return parser


def main(argv: list[str] | None = None) -> int:
    args = _parser().parse_args(argv)
    if args.self_test:
        return _self_test()
    if args.input_root is None or args.output_jsonl is None or args.report is None:
        fail("input root, output corpus, and report paths are required")
    report = merge(
        input_root=args.input_root,
        output_jsonl=args.output_jsonl,
        report_path=args.report,
        base_seed=args.base_seed,
        first_pair=args.first_pair,
        expected_pairs=args.expected_pairs,
    )
    print(json.dumps(report, sort_keys=True, allow_nan=False))
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, ValueError) as error:
        print(f"merge_corpus_v4: ERROR: {error}", file=sys.stderr)
        raise SystemExit(2)
