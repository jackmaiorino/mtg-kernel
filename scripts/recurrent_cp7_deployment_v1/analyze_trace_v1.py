#!/usr/bin/env python3
"""Compare recurrent-candidate and parent terminal-screen trajectories."""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import re
import sys
from typing import Any


SCHEMA = "mtg-kernel-recurrent-cp7-trace-analysis/v1"
REPORT_SCHEMA = "mtg-kernel-recurrent-cp7-terminal-screen/v1.report"
LEG_PREFIX = "XMAGE_RALLY_ANCHOR_LEG PASS "
FIELD_RE = re.compile(r"(?P<key>[^ =]+)=(?P<value>[^ ]+)")
IGNORED_TRAJECTORY_FIELDS = frozenset({"elapsed_ms", "winner"})


def _sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def _write_new(path: Path, value: Any) -> None:
    if path.exists():
        raise RuntimeError(f"refusing to overwrite {path}")
    path.write_text(
        json.dumps(value, indent=2, sort_keys=True, allow_nan=False) + "\n",
        encoding="utf-8",
        newline="\n",
    )


def _leg_rows(path: Path) -> dict[int, dict[str, str]]:
    rows: dict[int, dict[str, str]] = {}
    for line in path.read_text(encoding="utf-8").splitlines():
        if not line.startswith(LEG_PREFIX):
            continue
        fields = {
            match.group("key"): match.group("value")
            for match in FIELD_RE.finditer(line[len(LEG_PREFIX) :])
        }
        episode = int(fields["episode"])
        if episode in rows:
            raise RuntimeError(f"duplicate episode {episode} in {path}")
        rows[episode] = fields
    if len(rows) != 2:
        raise RuntimeError(f"expected two leg rows in {path}, found {len(rows)}")
    return rows


def run(args: argparse.Namespace) -> int:
    report_path = args.evidence_root / "report.json"
    report = json.loads(report_path.read_text(encoding="utf-8"))
    if report.get("schema") != REPORT_SCHEMA:
        raise RuntimeError("terminal-screen report schema mismatch")
    matched = report.get("matched_pairs", [])
    if len(matched) != len(report.get("accepted_pairs", [])):
        raise RuntimeError("matched-pair and accepted-pair counts differ")

    games: list[dict[str, Any]] = []
    changed_pairs: set[int] = set()
    for pair in matched:
        pair_index = int(pair["pair_index"])
        candidate_path = args.evidence_root / "tasks" / f"search-pair-{pair_index:04d}.log"
        parent_path = args.evidence_root / "tasks" / f"parent-pair-{pair_index:04d}.log"
        if _sha256(candidate_path) != pair["candidate_log_sha256"]:
            raise RuntimeError(f"candidate log hash mismatch for pair {pair_index}")
        if _sha256(parent_path) != pair["parent_log_sha256"]:
            raise RuntimeError(f"parent log hash mismatch for pair {pair_index}")
        candidate_rows = _leg_rows(candidate_path)
        parent_rows = _leg_rows(parent_path)
        if candidate_rows.keys() != parent_rows.keys():
            raise RuntimeError(f"episode mismatch for pair {pair_index}")
        for episode in sorted(candidate_rows):
            candidate = candidate_rows[episode]
            parent = parent_rows[episode]
            for fixed in ("episode", "candidate", "environment_seed"):
                if candidate.get(fixed) != parent.get(fixed):
                    raise RuntimeError(
                        f"matched field {fixed} differs for pair {pair_index}, episode {episode}"
                    )
            compared_fields = sorted(
                (candidate.keys() | parent.keys()) - IGNORED_TRAJECTORY_FIELDS
            )
            changed_fields = [
                field
                for field in compared_fields
                if candidate.get(field) != parent.get(field)
            ]
            trajectory_changed = bool(changed_fields)
            outcome_changed = candidate.get("winner") != parent.get("winner")
            if trajectory_changed:
                changed_pairs.add(pair_index)
            games.append(
                {
                    "pair_index": pair_index,
                    "episode": episode,
                    "candidate_seat": candidate["candidate"],
                    "candidate_winner": candidate["winner"],
                    "parent_winner": parent["winner"],
                    "outcome_changed": outcome_changed,
                    "trajectory_changed": trajectory_changed,
                    "changed_fields": changed_fields,
                }
            )

    changed_games = [game for game in games if game["trajectory_changed"]]
    unchanged_games = [game for game in games if not game["trajectory_changed"]]
    outcome_flips = [game for game in games if game["outcome_changed"]]
    by_seat = {
        seat: sum(
            game["trajectory_changed"] for game in games if game["candidate_seat"] == seat
        )
        for seat in ("p0", "p1")
    }
    analysis = {
        "schema": SCHEMA,
        "terminal_report_sha256": _sha256(report_path),
        "pair_count": len(matched),
        "game_count": len(games),
        "trajectory_changed_games": len(changed_games),
        "trajectory_unchanged_games": len(unchanged_games),
        "pairs_with_trajectory_change": len(changed_pairs),
        "trajectory_changed_games_by_candidate_seat": by_seat,
        "terminal_outcome_flips": len(outcome_flips),
        "ignored_trajectory_fields": sorted(IGNORED_TRAJECTORY_FIELDS),
        "games": games,
        "non_claims": [
            "aggregate trace differences do not identify whether a changed action was better",
            "terminal win or loss remains the only playing-strength outcome",
        ],
    }
    _write_new(args.output, analysis)
    print(json.dumps(analysis, sort_keys=True))
    return 0


def self_test() -> int:
    fields = {
        match.group("key"): match.group("value")
        for match in FIELD_RE.finditer("episode=7 candidate=p1 winner=p0 turns=12")
    }
    if fields != {
        "episode": "7",
        "candidate": "p1",
        "winner": "p0",
        "turns": "12",
    }:
        raise RuntimeError("field parser self-test failed")
    print("analyze_recurrent_cp7_trace_v1: SELF-TEST PASS")
    return 0


def arguments() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--evidence-root", type=Path)
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    if args.self_test:
        return args
    if args.evidence_root is None:
        parser.error("--evidence-root is required")
    if args.output is None:
        args.output = args.evidence_root / "trace_analysis.json"
    return args


if __name__ == "__main__":
    try:
        parsed = arguments()
        sys.exit(self_test() if parsed.self_test else run(parsed))
    except Exception as error:
        print(f"analyze_recurrent_cp7_trace_v1: ERROR: {error}", file=sys.stderr)
        sys.exit(1)
