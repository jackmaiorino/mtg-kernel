#!/usr/bin/env python3
"""Validate and combine complete outcome-v2 shards without losing provenance."""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
from typing import Any, TextIO


CONTRACT = "mtg-kernel-xmage-cp7-outcome-jsonl/v2"
PARENT_MANIFEST_SHA256 = "706b3aa80ec7a3c067d458fef06bb2237320543f202fb2349c5cb885975fdbbb"
PARENT_PAYLOAD_SHA256 = "eb83be33bcb7418b6f85ec9687da4b7ca5620a1df64721a1942d2793588bbd3c"
PARENT_TRAIN_STATE_SHA256 = "2c55a13abb3157f3f4ba012af663ffa56599c5d6cb90743c1ba6e024ca47a9c8"
PARENT_MODEL_PARAMETER_SHA256 = "883e4882d01d9cb55ecd7a4ae00e3c95793b6147baf3df08650ef1fa7f8e9546"


def _fail(message: str) -> None:
    raise ValueError(message)


def _sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def _write_row(handle: TextIO, row: dict[str, Any]) -> None:
    handle.write(json.dumps(row, sort_keys=True, separators=(",", ":"), allow_nan=False))
    handle.write("\n")


def _load_rows(path: Path) -> list[dict[str, Any]]:
    rows: list[dict[str, Any]] = []
    with path.open("r", encoding="utf-8") as handle:
        for line_number, line in enumerate(handle, 1):
            if not line.strip():
                continue
            try:
                row = json.loads(line)
            except json.JSONDecodeError as error:
                _fail(f"{path}:{line_number}: invalid JSON: {error}")
            if not isinstance(row, dict):
                _fail(f"{path}:{line_number}: row is not an object")
            rows.append(row)
    if not rows:
        _fail(f"{path}: empty shard")
    return rows


def _checkpoint_identity(header: dict[str, Any]) -> tuple[Any, ...]:
    checkpoint = header.get("checkpoint")
    if not isinstance(checkpoint, dict):
        _fail("header checkpoint is missing")
    return (
        checkpoint.get("loaded_checkpoint_sha256"),
        checkpoint.get("loaded_payload_sha256"),
        checkpoint.get("loaded_train_state_sha256"),
        checkpoint.get("model_parameter_sha256"),
    )


def _validate_header(header: dict[str, Any]) -> None:
    if (
        header.get("record_type") != "header"
        or header.get("record_ordinal") != 0
        or header.get("schema_version") != 2
        or header.get("export_contract") != CONTRACT
    ):
        _fail("shard header is not exact outcome-v2")
    if _checkpoint_identity(header) != (
        PARENT_MANIFEST_SHA256,
        PARENT_PAYLOAD_SHA256,
        PARENT_TRAIN_STATE_SHA256,
        PARENT_MODEL_PARAMETER_SHA256,
    ):
        _fail("shard header does not bind the retained parent")


def _terminal_is_natural(row: dict[str, Any]) -> bool:
    terminal = row.get("terminal")
    return bool(
        isinstance(terminal, dict)
        and terminal.get("terminal_classification") == "natural"
        and terminal.get("terminal_code") == "natural_game_over"
        and terminal.get("terminal_reason") == "game_over"
        and terminal.get("terminal_outcome") in ("p0_win", "p1_win", "draw")
    )


def combine(
    inputs: list[Path], output: Path, first_pair: int, expected_pairs: int
) -> dict[str, Any]:
    if len(inputs) < 1 or first_pair < 0 or expected_pairs < 1:
        _fail("at least one input and one expected pair are required")
    if output.exists():
        _fail(f"refusing to overwrite {output}")
    output.parent.mkdir(parents=True, exist_ok=True)

    loaded: list[tuple[Path, list[dict[str, Any]]]] = []
    canonical_header: dict[str, Any] | None = None
    source_reports: list[dict[str, Any]] = []
    all_terminals: set[tuple[int, int, str]] = set()
    all_decisions: set[tuple[int, int, str, int]] = set()

    for path in inputs:
        rows = _load_rows(path)
        header = rows[0]
        _validate_header(header)
        if any(row.get("record_type") == "header" for row in rows[1:]):
            _fail(f"{path}: duplicate header")
        if canonical_header is None:
            canonical_header = header
        elif header != canonical_header:
            _fail(f"{path}: header differs from the first shard")

        local_decision_ordinal = 0
        terminals: set[tuple[int, int, str]] = set()
        decisions: set[tuple[int, int, str, int]] = set()
        for local_ordinal, row in enumerate(rows[1:], 1):
            if row.get("record_ordinal") != local_ordinal:
                _fail(f"{path}: non-contiguous record_ordinal at {local_ordinal}")
            record_type = row.get("record_type")
            pair = row.get("pair_index")
            episode = row.get("episode_id")
            seat = row.get("candidate_seat")
            if not isinstance(pair, int) or not isinstance(episode, int) or seat not in ("p0", "p1"):
                _fail(f"{path}: invalid episode identity at record {local_ordinal}")
            if record_type == "decision":
                if row.get("outcome_decision_ordinal") != local_decision_ordinal:
                    _fail(f"{path}: non-contiguous outcome_decision_ordinal")
                decisions.add((pair, episode, seat, local_decision_ordinal))
                local_decision_ordinal += 1
            elif record_type == "terminal":
                if not _terminal_is_natural(row):
                    _fail(f"{path}: non-natural terminal for episode {episode}")
                key = (pair, episode, seat)
                if key in terminals:
                    _fail(f"{path}: duplicate terminal {key}")
                terminals.add(key)
            else:
                _fail(f"{path}: unknown record_type {record_type!r}")
        if len(terminals) < 2 or len(terminals) % 2:
            _fail(f"{path}: terminal count is not complete pairs: {len(terminals)}")
        if all_terminals.intersection(terminals):
            _fail(f"{path}: terminal overlap with another shard")
        if all_decisions.intersection(decisions):
            _fail(f"{path}: decision overlap with another shard")
        all_terminals.update(terminals)
        all_decisions.update(decisions)
        loaded.append((path, rows))
        source_reports.append(
            {
                "path": str(path),
                "sha256": _sha256(path),
                "record_count": len(rows),
                "decision_count": local_decision_ordinal,
                "terminal_count": len(terminals),
                "first_pair": min(key[0] for key in terminals),
                "last_pair": max(key[0] for key in terminals),
            }
        )

    expected_terminals = {
        (pair, pair * 2 + seat, f"p{seat}")
        for pair in range(first_pair, first_pair + expected_pairs)
        for seat in (0, 1)
    }
    if all_terminals != expected_terminals:
        missing = sorted(expected_terminals - all_terminals)[:8]
        extra = sorted(all_terminals - expected_terminals)[:8]
        _fail(f"terminal coverage mismatch missing={missing} extra={extra}")

    assert canonical_header is not None
    global_record_ordinal = 0
    global_decision_ordinal = 0
    with output.open("x", encoding="utf-8", newline="\n") as handle:
        merged_header = dict(canonical_header)
        merged_header["record_ordinal"] = global_record_ordinal
        _write_row(handle, merged_header)
        global_record_ordinal += 1
        for _, rows in sorted(
            loaded,
            key=lambda item: min(
                row["pair_index"] for row in item[1] if row.get("record_type") == "terminal"
            ),
        ):
            for original in rows[1:]:
                row = dict(original)
                row["record_ordinal"] = global_record_ordinal
                if row.get("record_type") == "decision":
                    row["outcome_decision_ordinal"] = global_decision_ordinal
                    global_decision_ordinal += 1
                _write_row(handle, row)
                global_record_ordinal += 1

    return {
        "schema": "mtg-kernel-outcome-shard-combine-report/v1",
        "output": str(output),
        "output_sha256": _sha256(output),
        "record_count": global_record_ordinal,
        "decision_count": global_decision_ordinal,
        "terminal_count": len(all_terminals),
        "first_pair": first_pair,
        "pair_count": expected_pairs,
        "sources": source_reports,
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", action="append", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--first-pair", type=int, default=0)
    parser.add_argument("--expected-pairs", type=int, default=256)
    parser.add_argument("--report", type=Path)
    args = parser.parse_args()
    result = combine(args.input, args.output, args.first_pair, args.expected_pairs)
    payload = json.dumps(result, sort_keys=True, indent=2, allow_nan=False) + "\n"
    if args.report:
        if args.report.exists():
            _fail(f"refusing to overwrite {args.report}")
        args.report.parent.mkdir(parents=True, exist_ok=True)
        args.report.write_text(payload, encoding="utf-8", newline="\n")
    print(json.dumps(result, sort_keys=True, allow_nan=False))
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, ValueError) as error:
        print(f"error: {error}", file=__import__("sys").stderr)
        raise SystemExit(2)
