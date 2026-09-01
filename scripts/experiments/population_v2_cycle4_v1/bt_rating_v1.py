"""Bradley-Terry rating fit over pairwise panel outcomes (cycle-4, derived metric).

Pre-registered as a NON-GATING derived metric (cycle-4 pre-registration
`OX_CYCLE4_PREREG_SKETCH_V2.md`; adopted after Jack's league-rating request):
each refresh's 28-matchup payoff panel, plus any cross-panel games among
identity-pinned models, feeds one Bradley-Terry fit anchored to a fixed
reference identity so ratings are comparable across refreshes and cycles.
Draws count as half a win to each side (the standard Davidson-free reduction);
terminal W/D/L are the only inputs.

Input schema (one JSON document):
    {
      "schema": "mtg-kernel-bt-rating-input/v1",
      "reference_id": "<model id whose rating is fixed at 0.0>",
      "pairs": [
        {"a_id": str, "b_id": str, "a_wins": int, "b_wins": int, "draws": int},
        ...
      ]
    }
Model ids are opaque strings; the caller pins them to checkpoint identities.

Output: ratings in natural-log units (positive = stronger than reference),
plus expected-score matrix entries for audit. The fit is the standard MM
(minorization-maximization) iteration for Bradley-Terry, deterministic, with
a fail-closed convergence check.
"""

from __future__ import annotations

import json
import math
import sys
from dataclasses import dataclass

SCHEMA_V1 = "mtg-kernel-bt-rating-input/v1"
# Convergence is judged in LOG space (max |log(updated) - log(old)|), so
# near-separated panels with a finite but extreme MLE converge instead of
# chasing an absolute-strength epsilon they can never satisfy (review
# finding). The iteration cap is generous because each MM sweep over an
# 8-model, 28-pair panel is microseconds.
MAX_ITERATIONS = 200_000
LOG_CONVERGENCE_EPSILON = 1e-10


class BtRatingError(ValueError):
    pass


@dataclass(frozen=True)
class PairRecord:
    a_id: str
    b_id: str
    a_score: float  # wins + draws/2
    b_score: float


def _load_pairs(document: dict) -> tuple[str, list[PairRecord]]:
    if document.get("schema") != SCHEMA_V1:
        raise BtRatingError("invalid schema")
    reference_id = document.get("reference_id")
    raw_pairs = document.get("pairs")
    if not isinstance(reference_id, str) or not reference_id:
        raise BtRatingError("missing reference_id")
    if not isinstance(raw_pairs, list) or not raw_pairs:
        raise BtRatingError("missing pairs")
    pairs: list[PairRecord] = []
    for entry in raw_pairs:
        keys = set(entry)
        if keys != {"a_id", "b_id", "a_wins", "b_wins", "draws"}:
            raise BtRatingError(f"invalid pair keys: {sorted(keys)}")
        a_id, b_id = entry["a_id"], entry["b_id"]
        if not isinstance(a_id, str) or not isinstance(b_id, str) or a_id == b_id:
            raise BtRatingError("invalid pair ids")
        counts = [entry["a_wins"], entry["b_wins"], entry["draws"]]
        if any(not isinstance(count, int) or count < 0 for count in counts):
            raise BtRatingError("invalid pair counts")
        if sum(counts) == 0:
            raise BtRatingError(f"empty pair {a_id} vs {b_id}")
        pairs.append(
            PairRecord(
                a_id=a_id,
                b_id=b_id,
                a_score=entry["a_wins"] + entry["draws"] / 2.0,
                b_score=entry["b_wins"] + entry["draws"] / 2.0,
            )
        )
    return reference_id, pairs


def _connected(ids: list[str], pairs: list[PairRecord]) -> bool:
    adjacency: dict[str, set[str]] = {model_id: set() for model_id in ids}
    for pair in pairs:
        adjacency[pair.a_id].add(pair.b_id)
        adjacency[pair.b_id].add(pair.a_id)
    seen = {ids[0]}
    frontier = [ids[0]]
    while frontier:
        for neighbor in adjacency[frontier.pop()]:
            if neighbor not in seen:
                seen.add(neighbor)
                frontier.append(neighbor)
    return len(seen) == len(ids)


def fit_bt_ratings(document: dict) -> dict:
    """Fits Bradley-Terry strengths by MM iteration; returns the result doc.

    Fails closed on: schema violations, a reference id absent from the pairs,
    a disconnected comparison graph, any model with zero total score or zero
    total counter-score (its rating would diverge to +/- infinity), or
    non-convergence within MAX_ITERATIONS.
    """
    reference_id, pairs = _load_pairs(document)
    ids = sorted({pair.a_id for pair in pairs} | {pair.b_id for pair in pairs})
    if reference_id not in ids:
        raise BtRatingError("reference_id has no games")
    if not _connected(ids, pairs):
        raise BtRatingError("comparison graph is disconnected")
    score: dict[str, float] = {model_id: 0.0 for model_id in ids}
    counter: dict[str, float] = {model_id: 0.0 for model_id in ids}
    for pair in pairs:
        score[pair.a_id] += pair.a_score
        counter[pair.a_id] += pair.b_score
        score[pair.b_id] += pair.b_score
        counter[pair.b_id] += pair.a_score
    for model_id in ids:
        if score[model_id] == 0.0 or counter[model_id] == 0.0:
            raise BtRatingError(f"degenerate record for {model_id}")

    strengths = {model_id: 1.0 for model_id in ids}
    iterations = 0
    for iteration in range(1, MAX_ITERATIONS + 1):
        iterations = iteration
        updated: dict[str, float] = {}
        for model_id in ids:
            denominator = 0.0
            for pair in pairs:
                if model_id == pair.a_id:
                    other = pair.b_id
                elif model_id == pair.b_id:
                    other = pair.a_id
                else:
                    continue
                games = pair.a_score + pair.b_score
                denominator += games / (strengths[model_id] + strengths[other])
            updated[model_id] = score[model_id] / denominator
        normalizer = updated[reference_id]
        updated = {model_id: value / normalizer for model_id, value in updated.items()}
        delta = max(
            abs(math.log(updated[model_id]) - math.log(strengths[model_id]))
            for model_id in ids
        )
        strengths = updated
        if delta < LOG_CONVERGENCE_EPSILON:
            break
    else:
        raise BtRatingError("MM iteration did not converge")

    ratings = {
        model_id: math.log(strengths[model_id]) for model_id in ids
    }
    # Structured pair entries rather than delimiter-joined keys: model ids
    # are opaque strings, so any joined encoding risks collisions (review
    # finding).
    expected = [
        {
            "a_id": pair.a_id,
            "b_id": pair.b_id,
            "expected_a_score": strengths[pair.a_id]
            / (strengths[pair.a_id] + strengths[pair.b_id]),
        }
        for pair in sorted(pairs, key=lambda entry: (entry.a_id, entry.b_id))
    ]
    return {
        "schema": "mtg-kernel-bt-rating-result/v1",
        "reference_id": reference_id,
        "iterations": iterations,
        "ratings_log_units": {key: ratings[key] for key in sorted(ratings)},
        "expected_scores": expected,
        "non_claims": [
            "derived non-gating metric; not a promotion or transfer result",
            "ratings are relative to the declared reference identity only",
            "terminal W/D/L are the only inputs; draws count half",
        ],
    }


def main() -> int:
    if len(sys.argv) != 3:
        print(
            "usage: bt_rating_v1.py <input.json> <output.json>",
            file=sys.stderr,
        )
        return 2
    with open(sys.argv[1], encoding="utf-8") as handle:
        document = json.load(handle)
    result = fit_bt_ratings(document)
    with open(sys.argv[2], "w", encoding="utf-8", newline="\n") as handle:
        json.dump(result, handle, indent=1, sort_keys=True)
        handle.write("\n")
    print(
        f"bt_rating_v1: {len(result['ratings_log_units'])} models, "
        f"{result['iterations']} iterations"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
