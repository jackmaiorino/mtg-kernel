#!/usr/bin/env python3
"""Merge the fixed GAE8 XMage outcome shards into one strict training corpus."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
from typing import Any


EXPECTED_PAIR_COUNT = 64
EXPECTED_AUTHORITY = "current-net8-gae8-v1"
EXPECTED_EXPORT_CONTRACT = "mtg-kernel-xmage-cp7-outcome-jsonl/v2"


def fail(message: str) -> None:
    raise SystemExit(message)


def load_rows(path: Path) -> list[dict[str, Any]]:
    payload = path.read_bytes()
    if not payload.endswith(b"\n") or b"\r" in payload:
        fail(f"{path}: rows must use LF and end with LF")
    rows: list[dict[str, Any]] = []
    for line_number, line in enumerate(payload.splitlines(), start=1):
        if not line:
            fail(f"{path}:{line_number}: empty row")
        try:
            row = json.loads(line)
        except json.JSONDecodeError as error:
            fail(f"{path}:{line_number}: invalid JSON: {error}")
        if not isinstance(row, dict):
            fail(f"{path}:{line_number}: row is not an object")
        rows.append(row)
    return rows


def canonical_row(row: dict[str, Any]) -> bytes:
    return json.dumps(
        row,
        ensure_ascii=False,
        allow_nan=False,
        separators=(",", ":"),
    ).encode("utf-8") + b"\n"


def exclusive_write(path: Path, payload: bytes) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    descriptor = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o644)
    try:
        with os.fdopen(descriptor, "wb") as output:
            output.write(payload)
            output.flush()
            os.fsync(output.fileno())
    except BaseException:
        try:
            path.unlink()
        except FileNotFoundError:
            pass
        raise


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--input-root", type=Path, required=True)
    parser.add_argument("--output-jsonl", type=Path, required=True)
    parser.add_argument("--report", type=Path, required=True)
    args = parser.parse_args()

    if args.output_jsonl.exists() or args.report.exists():
        fail("output corpus and report must both be absent")

    paths = sorted(args.input_root.glob("gae8-pair-*.outcome.jsonl"))
    expected_names = [f"gae8-pair-{index:04d}.outcome.jsonl" for index in range(EXPECTED_PAIR_COUNT)]
    if [path.name for path in paths] != expected_names:
        fail("input root does not contain the exact 64-pair GAE8 outcome inventory")

    output_rows: list[dict[str, Any]] = []
    expected_header: dict[str, Any] | None = None
    input_sha256: dict[str, str] = {}
    next_record_ordinal = 1
    next_decision_ordinal = 0
    decision_rows = 0
    terminal_rows = 0
    reward_counts = {-1: 0, 0: 0, 1: 0}

    for pair_index, path in enumerate(paths):
        payload = path.read_bytes()
        input_sha256[path.name] = hashlib.sha256(payload).hexdigest()
        rows = load_rows(path)
        if not rows or rows[0].get("record_type") != "header":
            fail(f"{path}: missing first-row header")
        header = rows[0]
        if header.get("record_ordinal") != 0:
            fail(f"{path}: header ordinal is not zero")
        if header.get("export_contract") != EXPECTED_EXPORT_CONTRACT:
            fail(f"{path}: export contract mismatch")
        checkpoint = header.get("checkpoint")
        if not isinstance(checkpoint, dict) or checkpoint.get("authority_kind") != EXPECTED_AUTHORITY:
            fail(f"{path}: checkpoint authority mismatch")
        if expected_header is None:
            expected_header = header
            output_rows.append(dict(header))
        elif header != expected_header:
            fail(f"{path}: header differs from the first shard")

        active_episode: int | None = None
        active_first_decision: int | None = None
        active_decision_count = 0
        seen_terminals = 0
        for row in rows[1:]:
            record_type = row.get("record_type")
            if row.get("pair_index") != pair_index:
                fail(f"{path}: pair index mismatch")
            episode_id = row.get("episode_id")
            if not isinstance(episode_id, int):
                fail(f"{path}: episode id is not an integer")
            if record_type == "decision":
                if active_episode is None:
                    active_episode = episode_id
                    active_first_decision = next_decision_ordinal
                    active_decision_count = 0
                elif active_episode != episode_id:
                    fail(f"{path}: episode decisions are interleaved")
                row = dict(row)
                row["record_ordinal"] = next_record_ordinal
                row["outcome_decision_ordinal"] = next_decision_ordinal
                next_decision_ordinal += 1
                active_decision_count += 1
                decision_rows += 1
            elif record_type == "terminal":
                if active_episode is None:
                    if row.get("outcome_decision_count") != 0:
                        fail(f"{path}: terminal claims decisions without an active episode")
                    active_episode = episode_id
                    active_first_decision = None
                if active_episode != episode_id:
                    fail(f"{path}: terminal episode mismatch")
                row = dict(row)
                row["record_ordinal"] = next_record_ordinal
                row["first_outcome_decision_ordinal"] = active_first_decision
                row["outcome_decision_count"] = active_decision_count
                reward = row.get("candidate_terminal_reward")
                if reward not in reward_counts:
                    fail(f"{path}: invalid terminal reward")
                reward_counts[reward] += 1
                terminal_rows += 1
                seen_terminals += 1
                active_episode = None
                active_first_decision = None
                active_decision_count = 0
            else:
                fail(f"{path}: unexpected record type {record_type!r}")
            output_rows.append(row)
            next_record_ordinal += 1
        if active_episode is not None or seen_terminals != 2:
            fail(f"{path}: shard does not contain two closed episodes")

    if expected_header is None or terminal_rows != EXPECTED_PAIR_COUNT * 2:
        fail("merged corpus is incomplete")
    corpus_payload = b"".join(canonical_row(row) for row in output_rows)
    corpus_sha256 = hashlib.sha256(corpus_payload).hexdigest()
    exclusive_write(args.output_jsonl, corpus_payload)

    report = {
        "schema": "mtg-kernel-current-net8-cp7-terminal-response-corpus/v1",
        "authority_kind": EXPECTED_AUTHORITY,
        "export_contract": EXPECTED_EXPORT_CONTRACT,
        "pair_indices": list(range(EXPECTED_PAIR_COUNT)),
        "pair_count": EXPECTED_PAIR_COUNT,
        "episode_count": terminal_rows,
        "decision_row_count": decision_rows,
        "record_count_including_header": len(output_rows),
        "terminal_return_counts_loss_draw_win": [
            reward_counts[-1],
            reward_counts[0],
            reward_counts[1],
        ],
        "output": {
            "path": str(args.output_jsonl.resolve()),
            "byte_count": len(corpus_payload),
            "sha256": corpus_sha256,
        },
        "inputs": input_sha256,
    }
    report_payload = (json.dumps(report, indent=2, sort_keys=True) + "\n").encode("utf-8")
    exclusive_write(args.report, report_payload)
    print(json.dumps(report, separators=(",", ":")))


if __name__ == "__main__":
    main()
