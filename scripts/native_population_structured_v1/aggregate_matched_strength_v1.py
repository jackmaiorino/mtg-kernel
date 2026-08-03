#!/usr/bin/env python3
"""Compare two native Pool3 outcome streams on an exact matched seed panel."""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
from typing import Any


SCHEMA = "mtg-kernel-native-population-matched-strength/v1"
OUTCOME_CONTRACT = "mtg-kernel-native-population-outcome-jsonl/v1"


def _fail(message: str) -> None:
    raise ValueError(message)


def _sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def _load_terminals(
    path: Path, expected_base_seed: int, expected_pairs: int
) -> tuple[dict[str, Any], dict[tuple[int, int, str], dict[str, Any]]]:
    header: dict[str, Any] | None = None
    terminals: dict[tuple[int, int, str], dict[str, Any]] = {}
    with path.open("r", encoding="utf-8") as handle:
        for line_number, line in enumerate(handle, 1):
            row = json.loads(line)
            record_type = row.get("record_type")
            if line_number == 1:
                if (
                    record_type != "header"
                    or row.get("export_contract") != OUTCOME_CONTRACT
                    or row.get("selection_source") != "candidate_checkpoint_policy"
                ):
                    _fail(f"{path} has an invalid header")
                header = row
                continue
            if record_type != "terminal":
                continue
            if row.get("base_seed_u64_hex") != f"{expected_base_seed:016x}":
                _fail(f"{path} contains an unexpected base seed")
            terminal = row.get("terminal")
            if (
                not isinstance(terminal, dict)
                or terminal.get("terminal_classification") != "natural"
                or row.get("candidate_terminal_reward") not in (-1, 0, 1)
            ):
                _fail(f"{path} contains a non-natural or invalid terminal")
            pair_index = int(row["pair_index"])
            episode_id = int(row["episode_id"])
            seat = str(row["candidate_seat"])
            key = (pair_index, episode_id, seat)
            if (
                pair_index < 0
                or pair_index >= expected_pairs
                or seat not in ("p0", "p1")
                or key in terminals
            ):
                _fail(f"{path} contains an invalid or duplicate terminal key")
            terminals[key] = row
    if header is None or len(terminals) != expected_pairs * 2:
        _fail(f"{path} does not contain the exact expected terminal panel")
    for pair_index in range(expected_pairs):
        seats = {
            seat
            for observed_pair, _, seat in terminals
            if observed_pair == pair_index
        }
        if seats != {"p0", "p1"}:
            _fail(f"{path} pair {pair_index} is not seat swapped")
    return header, terminals


def aggregate(args: argparse.Namespace) -> dict[str, Any]:
    candidate_header, candidate = _load_terminals(
        args.candidate_outcome, args.base_seed, args.pairs
    )
    baseline_header, baseline = _load_terminals(
        args.baseline_outcome, args.base_seed, args.pairs
    )
    if set(candidate) != set(baseline):
        _fail("candidate and baseline terminal keys differ")

    gains = losses = ties = 0
    wins = {"candidate": 0, "baseline": 0}
    seat_wins = {
        "p0": {"candidate": 0, "baseline": 0},
        "p1": {"candidate": 0, "baseline": 0},
    }
    for key in sorted(candidate):
        candidate_row = candidate[key]
        baseline_row = baseline[key]
        for field in (
            "pair_environment_seed_u64_hex",
            "episode_id",
            "candidate_seat",
        ):
            if candidate_row.get(field) != baseline_row.get(field):
                _fail(f"matched terminal field differs at {key}: {field}")
        candidate_reward = int(candidate_row["candidate_terminal_reward"])
        baseline_reward = int(baseline_row["candidate_terminal_reward"])
        seat = key[2]
        wins["candidate"] += int(candidate_reward > 0)
        wins["baseline"] += int(baseline_reward > 0)
        seat_wins[seat]["candidate"] += int(candidate_reward > 0)
        seat_wins[seat]["baseline"] += int(baseline_reward > 0)
        gains += int(candidate_reward > baseline_reward)
        losses += int(candidate_reward < baseline_reward)
        ties += int(candidate_reward == baseline_reward)

    seat_deltas = {
        seat: values["candidate"] - values["baseline"]
        for seat, values in seat_wins.items()
    }
    gates = {
        "paired_gain_margin": gains >= losses + args.required_gain_margin,
        "p0_win_delta_floor": seat_deltas["p0"] >= args.seat_win_delta_floor,
        "p1_win_delta_floor": seat_deltas["p1"] >= args.seat_win_delta_floor,
    }
    result = {
        "schema": SCHEMA,
        "base_seed": args.base_seed,
        "pairs": args.pairs,
        "games": args.pairs * 2,
        "candidate": {
            "outcome_jsonl": str(args.candidate_outcome),
            "sha256": _sha256(args.candidate_outcome),
            "checkpoint": candidate_header["checkpoint"],
            "wins": wins["candidate"],
        },
        "baseline": {
            "outcome_jsonl": str(args.baseline_outcome),
            "sha256": _sha256(args.baseline_outcome),
            "checkpoint": baseline_header["checkpoint"],
            "wins": wins["baseline"],
        },
        "paired": {"gains": gains, "losses": losses, "ties": ties},
        "wins_by_candidate_seat": seat_wins,
        "candidate_minus_baseline_wins_by_seat": seat_deltas,
        "gate_config": {
            "required_gain_margin": args.required_gain_margin,
            "seat_win_delta_floor": args.seat_win_delta_floor,
        },
        "gates": gates,
        "pass": all(gates.values()),
        "non_claims": [
            "native Rally Pool3 development result only",
            "no XMage, CP7, human, cross-deck, promotion, or pro-level claim",
        ],
    }
    if args.output.exists():
        _fail(f"refusing to overwrite {args.output}")
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(
        json.dumps(result, indent=2, sort_keys=True, allow_nan=False) + "\n",
        encoding="utf-8",
        newline="\n",
    )
    return result


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--candidate-outcome", type=Path, required=True)
    parser.add_argument("--baseline-outcome", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--base-seed", type=int, required=True)
    parser.add_argument("--pairs", type=int, required=True)
    parser.add_argument("--required-gain-margin", type=int, default=20)
    parser.add_argument("--seat-win-delta-floor", type=int, default=-4)
    args = parser.parse_args()
    if (
        args.base_seed < 0
        or args.pairs < 1
        or args.required_gain_margin < 1
        or args.seat_win_delta_floor > 0
    ):
        parser.error("invalid matched-strength configuration")
    print(json.dumps(aggregate(args), sort_keys=True, allow_nan=False))
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (KeyError, OSError, TypeError, ValueError, json.JSONDecodeError) as error:
        print(f"error: {error}", file=__import__("sys").stderr)
        raise SystemExit(2)
