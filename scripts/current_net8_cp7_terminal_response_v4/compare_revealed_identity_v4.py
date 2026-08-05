#!/usr/bin/env python3
"""Compare a batched V4 throughput screen to the revealed one-pair corpus."""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import re
import sys
from typing import Any

from outcome_v2 import canonical_json_bytes, exclusive_write, iter_jsonl, sha256, validate_outcome_shard


REPORT_SCHEMA = "mtg-kernel-current-net8-cp7-terminal-response-v4-revealed-identity/v1"
BASELINE_NAME = re.compile(r"gae8-pair-([0-9]{4,})\.outcome\.jsonl\Z")
CANDIDATE_NAME = re.compile(r"gae8-p([0-9]{6,})-n([0-9]{3})\.outcome\.jsonl\Z")


def fail(message: str) -> None:
    raise ValueError(message)


def _discover_baseline(root: Path, first_pair: int, pair_count: int) -> list[tuple[Path, int, int]]:
    discovered: dict[int, Path] = {}
    for path in root.glob("gae8-pair-*.outcome.jsonl"):
        match = BASELINE_NAME.fullmatch(path.name)
        if match is None:
            fail(f"unexpected revealed baseline shard name: {path.name}")
        pair = int(match.group(1))
        if first_pair <= pair < first_pair + pair_count:
            if pair in discovered:
                fail(f"duplicate revealed baseline pair {pair}")
            discovered[pair] = path.resolve()
    if set(discovered) != set(range(first_pair, first_pair + pair_count)):
        fail("revealed baseline pair inventory is incomplete")
    return [(discovered[pair], pair, 1) for pair in sorted(discovered)]


def _discover_candidate(root: Path, first_pair: int, pair_count: int) -> list[tuple[Path, int, int]]:
    shards: list[tuple[Path, int, int]] = []
    for path in root.glob("gae8-p*.outcome.jsonl"):
        match = CANDIDATE_NAME.fullmatch(path.name)
        if match is None:
            fail(f"unexpected throughput-screen shard name: {path.name}")
        shards.append((path.resolve(), int(match.group(1)), int(match.group(2))))
    shards.sort(key=lambda row: row[1])
    expected = first_pair
    for _, shard_first, shard_count in shards:
        if shard_first != expected:
            fail("throughput-screen shard inventory is missing, overlapping, or reordered")
        expected += shard_count
    if expected != first_pair + pair_count:
        fail("throughput-screen pair inventory differs from the requested panel")
    return shards


def _normalized_pair_rows(
    shards: list[tuple[Path, int, int]],
    *,
    base_seed: int,
) -> tuple[dict[int, list[dict[str, Any]]], list[dict[str, Any]]]:
    pairs: dict[int, list[dict[str, Any]]] = {}
    reports = []
    for path, first_pair, pair_count in shards:
        report = validate_outcome_shard(
            path,
            base_seed=base_seed,
            first_pair=first_pair,
            pair_count=pair_count,
        )
        reports.append({key: value for key, value in report.items() if key != "header"})
        rows = iter_jsonl(path)
        next(rows)
        for source in rows:
            row = dict(source)
            pair = row["pair_index"]
            row.pop("record_ordinal")
            if row["record_type"] == "decision":
                row.pop("outcome_decision_ordinal")
            elif row["record_type"] == "terminal":
                row.pop("first_outcome_decision_ordinal")
            else:
                fail(f"unknown row type while normalizing {path}")
            pairs.setdefault(pair, []).append(row)
        if sha256(path) != report["sha256"]:
            fail(f"outcome shard changed during identity comparison: {path}")
    return pairs, reports


def _pair_digest(rows: list[dict[str, Any]]) -> str:
    digest = hashlib.sha256()
    for row in rows:
        digest.update(canonical_json_bytes(row))
    return digest.hexdigest()


def compare(
    *,
    baseline_root: Path,
    candidate_root: Path,
    report_path: Path,
    base_seed: int,
    first_pair: int,
    pair_count: int,
) -> dict[str, Any]:
    if (
        not 0 <= base_seed <= 0x7FFF_FFFF_FFFF_FFFF
        or first_pair < 0
        or not 1 <= pair_count <= 4096
    ):
        fail("invalid revealed identity panel")
    report_path = report_path.resolve()
    if report_path.exists():
        fail(f"identity report already exists: {report_path}")
    baseline_shards = _discover_baseline(baseline_root.resolve(), first_pair, pair_count)
    candidate_shards = _discover_candidate(candidate_root.resolve(), first_pair, pair_count)
    baseline_pairs, baseline_reports = _normalized_pair_rows(baseline_shards, base_seed=base_seed)
    candidate_pairs, candidate_reports = _normalized_pair_rows(candidate_shards, base_seed=base_seed)
    expected_pairs = set(range(first_pair, first_pair + pair_count))
    if set(baseline_pairs) != expected_pairs or set(candidate_pairs) != expected_pairs:
        fail("normalized pair coverage mismatch")
    pair_evidence = []
    for pair in sorted(expected_pairs):
        baseline_rows = baseline_pairs[pair]
        candidate_rows = candidate_pairs[pair]
        baseline_digest = _pair_digest(baseline_rows)
        candidate_digest = _pair_digest(candidate_rows)
        if baseline_digest != candidate_digest or baseline_rows != candidate_rows:
            fail(f"revealed output identity mismatch at pair {pair}")
        pair_evidence.append(
            {
                "pair_index": pair,
                "normalized_row_count": len(baseline_rows),
                "normalized_sha256": baseline_digest,
            }
        )
    report = {
        "schema": REPORT_SCHEMA,
        "status": "pass",
        "tool": {
            "path": str(Path(__file__).resolve()),
            "sha256": sha256(Path(__file__).resolve()),
        },
        "comparison": "all outcome-v2 fields except shard-local ordinal offsets",
        "base_seed": base_seed,
        "first_pair": first_pair,
        "pair_count": pair_count,
        "episode_count": pair_count * 2,
        "baseline_root": str(baseline_root.resolve()),
        "candidate_root": str(candidate_root.resolve()),
        "collection_reports": {
            "baseline": (
                None
                if not (baseline_root.resolve().parent / "report.json").is_file()
                else {
                    "path": str(baseline_root.resolve().parent / "report.json"),
                    "sha256": sha256(baseline_root.resolve().parent / "report.json"),
                }
            ),
            "candidate": (
                None
                if not (candidate_root.resolve().parent / "report.json").is_file()
                else {
                    "path": str(candidate_root.resolve().parent / "report.json"),
                    "sha256": sha256(candidate_root.resolve().parent / "report.json"),
                }
            ),
        },
        "baseline_shards": baseline_reports,
        "candidate_shards": candidate_reports,
        "pairs": pair_evidence,
        "excluded_ordinal_fields": [
            "record_ordinal",
            "outcome_decision_ordinal",
            "first_outcome_decision_ordinal",
        ],
        "nonclaims": [
            "revealed identity is a harness check, not playing-strength evidence",
        ],
    }
    exclusive_write(report_path, canonical_json_bytes(report, indent=2))
    return report


def _parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--baseline-root", type=Path, required=True)
    parser.add_argument("--candidate-root", type=Path, required=True)
    parser.add_argument("--report", type=Path, required=True)
    parser.add_argument("--base-seed", type=int, default=1_820_001)
    parser.add_argument("--first-pair", type=int, default=0)
    parser.add_argument("--pair-count", type=int, default=32)
    return parser


def main(argv: list[str] | None = None) -> int:
    args = _parser().parse_args(argv)
    report = compare(
        baseline_root=args.baseline_root,
        candidate_root=args.candidate_root,
        report_path=args.report,
        base_seed=args.base_seed,
        first_pair=args.first_pair,
        pair_count=args.pair_count,
    )
    print(json.dumps(report, sort_keys=True, allow_nan=False))
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, ValueError) as error:
        print(f"compare_revealed_identity_v4: ERROR: {error}", file=sys.stderr)
        raise SystemExit(2)
