"""Summarise the deck-signal experiment (cube_deck_signal_v1 JSONL; design note section 10).

Reads one part's game lines (in index order, from index 0) and computes the note's readings,
under three treatments of halted games (any game that did not end `natural_game_over`,
including a process death):

* ``draw``: a halted game counts 0.5 (the primary analysis);
* part A, ``c_loss`` and ``c_win``: a halted game counts as a loss, then as a win, for C;
* part B, ``culprit_loss`` and ``culprit_win``: both decks are coherent, so a halted game is
  charged to the deck that holds the halt's source card (the line's ``last_action_source``,
  else its ``recent_action_source``; decks that meet share no nonbasic card) and counts as a
  loss, then as a win, for that deck. A halt with no source, a source in both decks (a basic
  land) or in neither, or a process death, counts 0.5 in every treatment; the counts of
  charged and uncharged halts are reported.

Part A. Each pair's score is C's mean result over its two games (win 1, draw 0.5, loss 0).
Reported: the mean pair score and its 95% paired bootstrap interval (10,000 resamples of
the pairs; draw ``t`` of resample ``b`` is pair ``seed(["bootstrap", "deck-signal-A", b, t])
mod n``, with ``seed`` as in ``deal.py``; the interval is the replicate means at sorted
positions ``(B - 1) // 40`` and ``(39 (B - 1) + 39) // 40``, as ``evaluation_stats.py``
takes them). The reading, fixed by the note: lower bound above 0.55 is ``positive`` (under
uniform play a coherent deck beats a pile), upper bound below 0.55 is ``negative`` (at this
size it does not), anything else ``inconclusive``. A reading stands only if all three
treatments give it; otherwise the overall reading is ``inconclusive``. If more than 5% of
the games halt, the overall reading is ``technical_failure``.

Part B. The additive model of the note's section 6: the anchor deck's result in a game is
``0.5 + d_anchor - d_other`` plus noise. Strengths ``d`` are fitted by least squares over
every game with ``sum(d) = 0``; ``L`` is the game-weighted Laplacian of the matchups, the
residual variance is ``RSS / (games - (decks - 1))``, and deck ``i``'s noise variance is
``v_i = sigma_e^2 * pinv(L)[i][i]``. Reported: the raw standard deviation of the fitted
strengths, ``sqrt(sum((d_i - mean)^2) / (n - 1))``, and the standard deviation of deck
strength with that noise removed, ``sqrt(max(0, (sum((d_i - mean)^2) - sum(v_i)) / (n - 1)))``
(the note's quantity sigma_d), each with a 95% bootstrap interval over decks: 10,000
resamples, draw ``t`` of resample ``b`` is deck ``seed(["bootstrap", "deck-signal-B", b, t])
mod n``, the statistic recomputed on the resampled ``(d_i, v_i)`` with the fit held at the
full data. No pass or fail; the halt rate is reported.

Usage:
  deck_signal_summary.py --games GAMES.jsonl [--record RUN.json] --out SUMMARY.json
"""

from __future__ import annotations

import argparse
import collections
import hashlib
import json
import math
from pathlib import Path
import sys
from typing import Any, Iterable, Sequence

sys.path.insert(0, str(Path(__file__).resolve().parent))

import b_census_summary as bcs  # noqa: E402
from deal import MASK  # noqa: E402

GAME_LINE_SCHEMA = "cube_deck_signal_game/v1"
RESAMPLES = 10_000
THRESHOLD = 0.55
HALT_FAILURE_SHARE = 0.05
TREATMENTS = (("draw", 0.5), ("c_loss", 0.0), ("c_win", 1.0))
B_TREATMENTS = (("draw", 0.5), ("culprit_loss", 0.0), ("culprit_win", 1.0))
EXPECTED_GAMES = {"A": 800, "B": 1024}
COUNTS = ("driver_decisions", "physical_decisions", "policy_steps")


# ---------------------------------------------------------------------------
# Bootstrap
# ---------------------------------------------------------------------------


def resample_seed(namespace: str, b: int, t: int) -> int:
    """``deal.seed(["bootstrap", namespace, b, t])``, written out for speed.

    For a namespace of printable ASCII without quote or backslash and non-negative integers,
    this is the text ``json.dumps(..., separators=(",", ":"))`` writes."""
    text = f'["bootstrap","{namespace}",{b},{t}]'
    return int.from_bytes(hashlib.sha256(text.encode()).digest()[:8], "little") & MASK


def _check_namespace(namespace: str) -> None:
    if not all(" " <= c <= "~" and c not in '"\\' for c in namespace):
        raise ValueError(f"namespace {namespace!r} needs plain printable ASCII")


def interval_positions(resamples: int) -> tuple[int, int]:
    """Sorted positions of the 95% percentile interval (as evaluation_stats.py)."""
    return (resamples - 1) // 40, (39 * (resamples - 1) + 39) // 40


def bootstrap(
    namespace: str,
    n: int,
    statistics: dict[str, Any],
    resamples: int = RESAMPLES,
) -> dict[str, tuple[float, float]]:
    """For every named statistic (a function of a list of resampled indices), its 95%
    percentile interval over ``resamples`` resamples of ``n`` units. All statistics share
    the same resampled indices."""
    _check_namespace(namespace)
    if n <= 0:
        raise ValueError("nothing to resample")
    replicates: dict[str, list[float]] = {name: [] for name in statistics}
    for b in range(resamples):
        indices = [resample_seed(namespace, b, t) % n for t in range(n)]
        for name, statistic in statistics.items():
            replicates[name].append(statistic(indices))
    low, high = interval_positions(resamples)
    out = {}
    for name, values in replicates.items():
        values.sort()
        out[name] = (values[low], values[high])
    return out


# ---------------------------------------------------------------------------
# Lines
# ---------------------------------------------------------------------------


def read_lines(path: Path) -> list[dict[str, Any]]:
    rows = []
    with open(path, encoding="utf-8") as handle:
        for number, text in enumerate(handle, 1):
            if not text.strip():
                continue
            row = json.loads(text)
            if row.get("schema") != GAME_LINE_SCHEMA:
                raise ValueError(f"{path}:{number}: schema {row.get('schema')!r} is not {GAME_LINE_SCHEMA}")
            rows.append(row)
    return rows


def check_rows(rows: Sequence[dict[str, Any]]) -> str:
    """The part of the rows; refuses mixed parts and indices that are not 0, 1, 2, ..."""
    if not rows:
        raise ValueError("no game lines")
    parts = {row["part"] for row in rows}
    if len(parts) != 1 or not parts <= {"A", "B"}:
        raise ValueError(f"lines of parts {sorted(parts)}; one part, A or B")
    for expected, row in enumerate(rows):
        if row["index"] != expected:
            raise ValueError(f"line {expected} has index {row['index']}, expected {expected}")
    return parts.pop()


def halted(row: dict[str, Any]) -> bool:
    return not row["completed"]


def seat_value(row: dict[str, Any], seat: int, halt_value: float) -> float:
    """The result of ``seat`` in the game: its score when completed, else ``halt_value``."""
    if halted(row):
        return halt_value
    return float(row["score"][seat])


def common(rows: Sequence[dict[str, Any]]) -> dict[str, Any]:
    """Halt counts, stop kinds (recomputed from the raw end) and decision counts."""
    stop_kinds = collections.Counter()
    mismatches = []
    for row in rows:
        kind = bcs.stop_kind(row["end"])
        if kind != row.get("stop_kind"):
            mismatches.append({"index": row["index"], "line": row.get("stop_kind"), "recomputed": kind})
        if halted(row):
            stop_kinds[kind] += 1
    halts = sum(1 for row in rows if halted(row))
    counts = {}
    for key in COUNTS:
        values = [row[key] for row in rows if row.get(key) is not None]
        counts[key] = bcs.distribution(values)
        counts[key]["mean"] = sum(values) / len(values) if values else None
    return {
        "games": len(rows),
        "completed": len(rows) - halts,
        "halted": halts,
        "halt_share": halts / len(rows),
        "halt_share_above_5pct": halts / len(rows) > HALT_FAILURE_SHARE,
        "halted_by_stop_kind": dict(sorted(stop_kinds.items(), key=lambda item: (-item[1], item[0]))),
        "stop_kind_mismatches": mismatches,
        "decision_counts": counts,
    }


# ---------------------------------------------------------------------------
# Part A
# ---------------------------------------------------------------------------


def part_a_pairs(rows: Sequence[dict[str, Any]]) -> list[tuple[dict[str, Any], dict[str, Any]]]:
    """The games of each pair, in pair order; refuses a pair that is not one C-first game and
    one P-first game of the same two decks."""
    if len(rows) % 2:
        raise ValueError("part A needs two games per pair")
    pairs = []
    for at in range(0, len(rows), 2):
        first, second = rows[at], rows[at + 1]
        pair = at // 2
        if (first["pair"], first["pair_game"], second["pair"], second["pair_game"]) != (pair, 0, pair, 1):
            raise ValueError(f"pair {pair}: games are not (pair_game 0, pair_game 1)")
        if first["kinds"] != ["C", "P"] or second["kinds"] != ["P", "C"]:
            raise ValueError(f"pair {pair}: seats are not C, P then P, C")
        if first["decks"] != second["decks"][::-1]:
            raise ValueError(f"pair {pair}: the two games do not swap the same decks")
        pairs.append((first, second))
    return pairs


def c_value(row: dict[str, Any], halt_value: float) -> float:
    return seat_value(row, row["kinds"].index("C"), halt_value)


def reading_of(lower: float, upper: float) -> str:
    if lower > THRESHOLD:
        return "positive"
    if upper < THRESHOLD:
        return "negative"
    return "inconclusive"


def summarise_a(rows: Sequence[dict[str, Any]], resamples: int = RESAMPLES) -> dict[str, Any]:
    pairs = part_a_pairs(rows)
    n = len(pairs)
    scores = {
        name: [(c_value(first, value) + c_value(second, value)) / 2 for first, second in pairs]
        for name, value in TREATMENTS
    }
    intervals = bootstrap(
        "deck-signal-A",
        n,
        {name: (lambda idx, s=values: sum(s[i] for i in idx) / n) for name, values in scores.items()},
        resamples,
    )
    treatments = {}
    for name, _ in TREATMENTS:
        lower, upper = intervals[name]
        treatments[name] = {
            "mean_pair_score": sum(scores[name]) / n,
            "lower": lower,
            "upper": upper,
            "reading": reading_of(lower, upper),
        }
    out = common(rows)
    completed = [row for row in rows if not halted(row)]
    out["c_results_completed"] = {
        "wins": sum(1 for row in completed if c_value(row, 0.0) == 1.0),
        "draws": sum(1 for row in completed if c_value(row, 0.0) == 0.5),
        "losses": sum(1 for row in completed if c_value(row, 0.0) == 0.0),
    }
    out["seat0_wins_completed"] = sum(1 for row in completed if row["winner_player"] == 0)
    readings = {treatments[name]["reading"] for name, _ in TREATMENTS}
    if out["halt_share_above_5pct"]:
        overall = "technical_failure"
    elif len(readings) == 1:
        overall = readings.pop()
    else:
        overall = "inconclusive"
    out.update(
        {
            "part": "A",
            "pairs": n,
            "resamples": resamples,
            "interval_positions": interval_positions(resamples),
            "threshold": THRESHOLD,
            "treatments": treatments,
            "reading": overall,
        }
    )
    return out


# ---------------------------------------------------------------------------
# Part B
# ---------------------------------------------------------------------------


def invert(matrix: list[list[float]]) -> list[list[float]]:
    """Gauss-Jordan inverse with partial pivoting."""
    size = len(matrix)
    work = [list(row) + [1.0 if i == j else 0.0 for j in range(size)] for i, row in enumerate(matrix)]
    for col in range(size):
        pivot = max(range(col, size), key=lambda r: abs(work[r][col]))
        if abs(work[pivot][col]) < 1e-12:
            raise ValueError("singular matrix: the matchup graph is not connected")
        work[col], work[pivot] = work[pivot], work[col]
        scale = work[col][col]
        work[col] = [value / scale for value in work[col]]
        for row in range(size):
            if row != col and work[row][col] != 0.0:
                factor = work[row][col]
                work[row] = [a - factor * b for a, b in zip(work[row], work[col])]
    return [row[size:] for row in work]


def fit_additive(games: Sequence[tuple[int, int, float]], decks: int) -> dict[str, Any]:
    """Least-squares strengths of ``y = 0.5 + d[a] - d[o]`` over ``(a, o, y)`` games, with
    ``sum(d) = 0``; the residual variance and each deck's noise variance."""
    laplacian = [[0.0] * decks for _ in range(decks)]
    rhs = [0.0] * decks
    for a, o, y in games:
        laplacian[a][a] += 1.0
        laplacian[o][o] += 1.0
        laplacian[a][o] -= 1.0
        laplacian[o][a] -= 1.0
        rhs[a] += y - 0.5
        rhs[o] -= y - 0.5
    shifted = [[laplacian[i][j] + 1.0 / decks for j in range(decks)] for i in range(decks)]
    inverse = invert(shifted)
    strengths = [sum(inverse[i][j] * rhs[j] for j in range(decks)) for i in range(decks)]
    pinv_diagonal = [inverse[i][i] - 1.0 / decks for i in range(decks)]
    rss = sum((y - 0.5 - strengths[a] + strengths[o]) ** 2 for a, o, y in games)
    dof = len(games) - (decks - 1)
    if dof <= 0:
        raise ValueError("no residual degrees of freedom")
    residual_variance = rss / dof
    return {
        "strengths": strengths,
        "noise_variances": [residual_variance * value for value in pinv_diagonal],
        "residual_variance": residual_variance,
        "residual_dof": dof,
    }


def spread(values: Sequence[float], noise: Sequence[float]) -> tuple[float, float]:
    """(raw SD, noise-corrected SD) of a sample of strengths and their noise variances."""
    n = len(values)
    mean = sum(values) / n
    squares = sum((value - mean) ** 2 for value in values)
    raw = math.sqrt(squares / (n - 1))
    corrected = math.sqrt(max(0.0, (squares - sum(noise)) / (n - 1)))
    return raw, corrected


def culprit_seat(row: dict[str, Any]) -> int | None:
    """The seat whose deck alone holds the halted game's source card, if any."""
    for key in ("last_action_source", "recent_action_source"):
        source = row.get(key)
        if source is None:
            continue
        holders = [seat for seat in (0, 1) if source in row["decks"][seat]]
        return holders[0] if len(holders) == 1 else None
    return None


def anchor_value(row: dict[str, Any], charged_value: float) -> float:
    """The anchor deck's result: its score when completed; for a halted game charged to a
    deck, ``charged_value`` for that deck (so ``1 - charged_value`` for the other); else 0.5."""
    seat = row["seat"]
    if not halted(row):
        return float(row["score"][seat])
    culprit = culprit_seat(row)
    if culprit is None:
        return 0.5
    return charged_value if culprit == seat else 1.0 - charged_value


def summarise_b(rows: Sequence[dict[str, Any]], resamples: int = RESAMPLES) -> dict[str, Any]:
    decks = sorted({deck for row in rows for deck in row["matchup_decks"]})
    if decks != list(range(len(decks))) or len(decks) < 3:
        raise ValueError(f"ring decks {decks} are not 0..n-1 with n >= 3")
    n = len(decks)
    for row in rows:
        anchor, other = row["matchup_decks"]
        seat = row["seat"]
        expected = [anchor, other] if seat == 0 else [other, anchor]
        if row["seat_decks"] != expected or row["kinds"] != ["C", "C"]:
            raise ValueError(f"line {row['index']}: seat decks or kinds do not match its seat")
    treatments = {}
    for name, value in B_TREATMENTS:
        games = [(row["matchup_decks"][0], row["matchup_decks"][1], anchor_value(row, value)) for row in rows]
        fit = fit_additive(games, n)
        strengths, noise = fit["strengths"], fit["noise_variances"]
        raw, corrected = spread(strengths, noise)
        intervals = bootstrap(
            "deck-signal-B",
            n,
            {
                "raw": lambda idx: spread([strengths[i] for i in idx], [noise[i] for i in idx])[0],
                "sigma_d": lambda idx: spread([strengths[i] for i in idx], [noise[i] for i in idx])[1],
            },
            resamples,
        )
        deck_games = collections.Counter()
        deck_points = collections.Counter()
        for a, o, y in games:
            deck_games[a] += 1
            deck_games[o] += 1
            deck_points[a] += y
            deck_points[o] += 1.0 - y
        treatments[name] = {
            "sigma_d": corrected,
            "sigma_d_lower": intervals["sigma_d"][0],
            "sigma_d_upper": intervals["sigma_d"][1],
            "raw_sd": raw,
            "raw_sd_lower": intervals["raw"][0],
            "raw_sd_upper": intervals["raw"][1],
            "residual_sd": math.sqrt(fit["residual_variance"]),
            "residual_dof": fit["residual_dof"],
            "mean_noise_sd": math.sqrt(sum(noise) / n),
            "strengths": strengths,
            "deck_mean_scores": [deck_points[d] / deck_games[d] for d in range(n)],
        }
    out = common(rows)
    charged = sum(1 for row in rows if halted(row) and culprit_seat(row) is not None)
    out["halts_charged_to_a_deck"] = charged
    out["halts_uncharged"] = out["halted"] - charged
    out.update(
        {
            "part": "B",
            "decks": n,
            "matchups": len({row["matchup"] for row in rows}),
            "resamples": resamples,
            "interval_positions": interval_positions(resamples),
            "treatments": treatments,
        }
    )
    return out


# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------


def summarise(rows: Sequence[dict[str, Any]], resamples: int = RESAMPLES) -> dict[str, Any]:
    part = check_rows(rows)
    out = summarise_a(rows, resamples) if part == "A" else summarise_b(rows, resamples)
    out["expected_games"] = EXPECTED_GAMES[part]
    out["complete"] = len(rows) == EXPECTED_GAMES[part]
    if part == "A" and not out["complete"]:
        # The note's reading is for the declared 400 pairs only.
        out["reading_if_complete"] = out["reading"]
        out["reading"] = "incomplete"
    out["label"] = "shakedown under the uniform-legal pilot"
    return out


def main(argv: Iterable[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--games", type=Path, required=True)
    parser.add_argument("--record", type=Path)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args(list(argv) if argv is not None else None)
    rows = read_lines(args.games)
    summary = summarise(rows)
    summary["games_file_sha256"] = hashlib.sha256(args.games.read_bytes()).hexdigest()
    if args.record is not None:
        record = json.loads(args.record.read_text(encoding="utf-8"))
        if record.get("part") != summary["part"]:
            raise SystemExit(f"run record is part {record.get('part')!r}, lines are part {summary['part']!r}")
        summary["run_record"] = {
            key: record.get(key)
            for key in ("binary", "card_db_hash", "fence_sha256", "caps", "seed_namespace", "seeds")
        }
    if args.out.exists():
        raise SystemExit(f"{args.out} exists; the summary is written fresh")
    args.out.write_text(json.dumps(summary, indent=1, sort_keys=True) + "\n", encoding="utf-8")
    brief = {"part": summary["part"], "games": summary["games"], "halt_share": summary["halt_share"]}
    if summary["part"] == "A":
        brief["reading"] = summary["reading"]
        brief["treatments"] = {k: [v["mean_pair_score"], v["lower"], v["upper"]] for k, v in summary["treatments"].items()}
    else:
        brief["sigma_d"] = {
            k: [v["sigma_d"], v["sigma_d_lower"], v["sigma_d_upper"]] for k, v in summary["treatments"].items()
        }
    print(json.dumps(brief))
    return 0


if __name__ == "__main__":
    sys.exit(main())
