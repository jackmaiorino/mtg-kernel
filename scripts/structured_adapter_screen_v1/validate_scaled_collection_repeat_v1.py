#!/usr/bin/env python3
"""Validate repeat identity modulo interchangeable duplicate card instances."""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
from typing import Any


SCHEMA = "mtg-kernel-scaled-structured-corpus-repeat-equivalence/v1"
RAW_SCHEMA = "mtg-kernel-scaled-structured-corpus-repeat/v1"
ALLOWED_TERMINAL_DIFFERENCES = {
    "core_environment_hash_u64_hex",
    "diagnostic_state_hash_u64_hex",
}
ALLOWED_TEACHER_DECISION_DIFFERENCES = {
    "action_semantics",
    "selected_index",
    "selected_semantic",
}


def _fail(message: str) -> None:
    raise ValueError(message)


def _sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def _without_arena_ids(value: Any) -> Any:
    if isinstance(value, dict):
        return {
            key: _without_arena_ids(child)
            for key, child in value.items()
            if key != "arena_id"
        }
    if isinstance(value, list):
        return [_without_arena_ids(child) for child in value]
    return value


def _different_keys(left: dict[str, Any], right: dict[str, Any]) -> set[str]:
    return {key for key in set(left) | set(right) if left.get(key) != right.get(key)}


def _terminal_without_hashes(value: dict[str, Any]) -> dict[str, Any]:
    return {
        key: child
        for key, child in value.items()
        if key not in ALLOWED_TERMINAL_DIFFERENCES
    }


def _validate_pair_identity(
    left: dict[str, Any], right: dict[str, Any], path: Path, line_number: int
) -> int:
    fields = ("record_type", "pair_index", "episode_id", "candidate_seat")
    if any(left.get(field) != right.get(field) for field in fields):
        _fail(f"{path}:{line_number}: repeat row identity changed")
    pair = left.get("pair_index")
    if not isinstance(pair, int):
        _fail(f"{path}:{line_number}: repeat row lacks pair index")
    return pair


def _compare_export(kind: str, original: Path, repeated: Path) -> dict[str, Any]:
    if not original.is_file() or not repeated.is_file():
        _fail(f"{kind} repeat source is missing")
    exact_pairs: dict[int, bool] = {}
    terminals: set[tuple[int, int, str]] = set()
    difference_rows: list[dict[str, Any]] = []
    pair_bytes_original: dict[int, hashlib._Hash] = {}  # type: ignore[attr-defined]
    pair_bytes_repeated: dict[int, hashlib._Hash] = {}  # type: ignore[attr-defined]
    header_original = b""
    header_repeated = b""
    with original.open("rb") as left_handle, repeated.open("rb") as right_handle:
        for line_number, (left_line, right_line) in enumerate(
            zip(left_handle, right_handle, strict=True), 1
        ):
            left = json.loads(left_line)
            right = json.loads(right_line)
            if line_number == 1:
                if left != right or left.get("record_type") != "header":
                    _fail(f"{kind} repeat header changed")
                header_original = left_line
                header_repeated = right_line
                continue
            pair = _validate_pair_identity(left, right, original, line_number)
            exact_pairs.setdefault(pair, True)
            pair_bytes_original.setdefault(pair, hashlib.sha256()).update(left_line)
            pair_bytes_repeated.setdefault(pair, hashlib.sha256()).update(right_line)
            if left_line == right_line:
                if left.get("record_type") == "terminal":
                    terminals.add((pair, left["episode_id"], left["candidate_seat"]))
                continue
            exact_pairs[pair] = False
            different = _different_keys(left, right)
            record_type = left.get("record_type")
            if record_type == "terminal":
                if not different.issubset(ALLOWED_TERMINAL_DIFFERENCES):
                    _fail(f"{kind} terminal changed beyond diagnostic hashes")
                if _terminal_without_hashes(left) != _terminal_without_hashes(right):
                    _fail(f"{kind} terminal outcome or decision counts changed")
                terminals.add((pair, left["episode_id"], left["candidate_seat"]))
            elif kind == "teacher" and record_type == "decision":
                if not different.issubset(ALLOWED_TEACHER_DECISION_DIFFERENCES):
                    _fail(
                        f"teacher model input or parent output changed at line {line_number}: "
                        f"{sorted(different)}"
                    )
                if _without_arena_ids(left.get("selected_semantic")) != _without_arena_ids(
                    right.get("selected_semantic")
                ):
                    _fail("teacher selected actions differ beyond duplicate arena identity")
                if _without_arena_ids(left.get("action_semantics")) != _without_arena_ids(
                    right.get("action_semantics")
                ):
                    _fail("teacher action menus differ beyond duplicate arena identity")
            else:
                _fail(f"{kind} nonterminal scientific row changed")
            difference_rows.append(
                {
                    "line": line_number,
                    "pair_index": pair,
                    "episode_id": left["episode_id"],
                    "record_type": record_type,
                    "different_fields": sorted(different),
                }
            )
    pair_digests: dict[str, dict[str, str]] = {}
    for pair in sorted(pair_bytes_original):
        left_digest = hashlib.sha256(header_original)
        left_digest.update(pair_bytes_original[pair].digest())
        right_digest = hashlib.sha256(header_repeated)
        right_digest.update(pair_bytes_repeated[pair].digest())
        pair_digests[str(pair)] = {
            "original": left_digest.hexdigest(),
            "repeat": right_digest.hexdigest(),
        }
    if len(terminals) != 2 * len(exact_pairs):
        _fail(f"{kind} repeat lacks complete seat-swapped terminals")
    return {
        "original_path": str(original),
        "original_sha256": _sha256(original),
        "repeat_path": str(repeated),
        "repeat_sha256": _sha256(repeated),
        "pair_count": len(exact_pairs),
        "exact_pairs": sorted(pair for pair, exact in exact_pairs.items() if exact),
        "different_pairs": sorted(pair for pair, exact in exact_pairs.items() if not exact),
        "difference_rows": difference_rows,
        "pair_digests": pair_digests,
    }


def validate(raw_report_path: Path, output: Path) -> dict[str, Any]:
    if output.exists():
        _fail(f"refusing to overwrite repeat report: {output}")
    raw = json.loads(raw_report_path.read_text(encoding="utf-8"))
    if raw.get("schema") != RAW_SCHEMA:
        _fail("raw repeat report schema mismatch")
    reports = {
        kind: _compare_export(
            kind,
            Path(raw["comparisons"][kind]["original_path"]),
            Path(raw["comparisons"][kind]["repeat_path"]),
        )
        for kind in ("teacher", "outcome")
    }
    exact_pairs = sorted(
        set(reports["teacher"]["exact_pairs"]).intersection(
            reports["outcome"]["exact_pairs"]
        )
    )
    for kind in ("teacher", "outcome"):
        source = raw["comparisons"][kind]
        report = reports[kind]
        if (
            source["original_sha256"] != report["original_sha256"]
            or source["repeat_sha256"] != report["repeat_sha256"]
        ):
            _fail(f"{kind} raw report hashes do not match files")
    result = {
        "schema": SCHEMA,
        "raw_repeat_report": {
            "path": str(raw_report_path),
            "sha256": _sha256(raw_report_path),
        },
        "task": raw["task"],
        "teacher": reports["teacher"],
        "outcome": reports["outcome"],
        "exact_pair_count": len(exact_pairs),
        "exact_pairs": exact_pairs,
        "model_inputs_and_parent_outputs_exact": True,
        "terminal_outcomes_and_counts_exact": True,
        "remaining_differences": "interchangeable_duplicate_card_arena_ids_only",
        "pass": len(exact_pairs) >= 1,
    }
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(
        json.dumps(result, sort_keys=True, indent=2, allow_nan=False) + "\n",
        encoding="utf-8",
        newline="\n",
    )
    return result


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--raw-repeat-report", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    result = validate(args.raw_repeat_report, args.output)
    print(json.dumps(result, sort_keys=True, allow_nan=False))
    return 0 if result["pass"] else 2


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, ValueError) as error:
        print(f"error: {error}", file=__import__("sys").stderr)
        raise SystemExit(2)
