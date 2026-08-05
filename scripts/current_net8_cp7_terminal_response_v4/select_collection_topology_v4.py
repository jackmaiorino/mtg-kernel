#!/usr/bin/env python3
"""Select the single V4 collection topology after the bounded screen."""

from __future__ import annotations

import argparse
import json
from pathlib import Path
import sys

from collect_corpus_v4 import (
    _build_topology_selection_report,
    _topology_attempt_record,
    _validate_expected_topology_paths,
    _validate_screen_and_identity_evidence,
)
from outcome_v2 import canonical_json_bytes, exclusive_write


def select(
    *,
    attempt_01_screen: Path,
    attempt_01_identity: Path,
    attempt_02_screen: Path,
    attempt_02_identity: Path,
    output_path: Path,
) -> dict:
    output_path = output_path.resolve()
    if output_path.exists():
        raise RuntimeError(f"topology selection report already exists: {output_path}")
    attempts = []
    for attempt_id, screen_path, identity_path in (
        ("attempt-01", attempt_01_screen, attempt_01_identity),
        ("attempt-02", attempt_02_screen, attempt_02_identity),
    ):
        _validate_expected_topology_paths(attempt_id, screen_path, identity_path)
        evidence = _validate_screen_and_identity_evidence(screen_path, identity_path)
        attempts.append(_topology_attempt_record(attempt_id, evidence))
    report = _build_topology_selection_report(attempts)
    exclusive_write(output_path, canonical_json_bytes(report, indent=2))
    return report


def _parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--attempt-01-screen", type=Path, required=True)
    parser.add_argument("--attempt-01-identity", type=Path, required=True)
    parser.add_argument("--attempt-02-screen", type=Path, required=True)
    parser.add_argument("--attempt-02-identity", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    return parser


def main(argv: list[str] | None = None) -> int:
    args = _parser().parse_args(argv)
    report = select(
        attempt_01_screen=args.attempt_01_screen,
        attempt_01_identity=args.attempt_01_identity,
        attempt_02_screen=args.attempt_02_screen,
        attempt_02_identity=args.attempt_02_identity,
        output_path=args.output,
    )
    print(json.dumps(report, sort_keys=True, allow_nan=False))
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, RuntimeError, ValueError) as error:
        print(f"select_collection_topology_v4: ERROR: {error}", file=sys.stderr)
        raise SystemExit(2)
