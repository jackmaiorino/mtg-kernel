"""Held-out evaluation statistics for the Foundations Limited campaign (issue #657).

Implements Section 4 of `docs/design/limited_campaign_v1.md` over the game records that
`kernel_limited_match_v1 play` writes (`games.jsonl` plus `games.jsonl.summary.json`): the pair
means, the paired bootstrap, the exact sign and Wilcoxon tests on discordant pairs, the Wilson
intervals, the stall sensitivity column, the subgroup audit, Gates E0 to E3 and the Holm adjustment
when two checkpoints are evaluated.

The candidate checkpoint is policy A of every run (`-a learner-checkpoint:<path>`) and the fixed
opponent is policy B. Each run is given with a second run of the same command at another worker
count (the replay), which Gate E0 compares on pairs 0..4.

    python limited_campaign_eval_v1.py evaluate \\
        --run u_final initial-weights/step-0 step0/games.jsonl step0-replay/games.jsonl \\
        --run u_final uniform-legal/v1 ... --out eval/evaluation.json

`evaluation.json` is canonical: sorted keys, no wall-time or worker-count fields and no paths of the
games files, so the same records give the same bytes whatever their order, host timing or worker count.
Timing, worker counts and the games paths go to the sidecar `evaluation.timing.json`. Checkpoints are
identified by the path in their policy spec (`learner-checkpoint:<path>`), which does reach the canonical
file through the specs and the rows, and by the SHA-256 of the checkpoint file, which the driver writes to the
summary as `a_checkpoint_sha256` (and `b_checkpoint_sha256` for a learner opponent) and which reaches the
canonical file through the summary. Every learner seat must have a valid digest; a run and its replay must
record the same digests and learner contract (else Gate E0 fails); a candidate's runs must record one digest
and all runs one learner contract, and the step-0 opponent one digest across candidates (else the records are
refused). The exit code is 0 when the
records were evaluated (whatever the gates say) and 2 when they were refused.
"""
from __future__ import annotations

import argparse
from array import array
from dataclasses import dataclass
from decimal import ROUND_CEILING, ROUND_FLOOR, Context, Decimal, localcontext
from fractions import Fraction
from functools import lru_cache
import hashlib
import json
import math
import os
from pathlib import Path
import sys
import time
from typing import Any

SCHEMA = "limited-campaign-evaluation/v1"
TIMING_SCHEMA = "limited-campaign-evaluation-timing/v1"
MATCH_SCHEMA = "mtg-kernel-limited-match/v1"
SESSION = "FullPoolV1"
MAX_TURNS = 100
MAX_DECISIONS = 4_000

# Section 4.3: 10,000 resamples seeded with SplitMix64(657_999).
BOOTSTRAP_SEED = 657_999
BOOTSTRAP_REPLICATES = 10_000
MASK64 = (1 << 64) - 1
GOLDEN_GAMMA = 0x9E37_79B9_7F4A_7C15

# Section 4.1: one candidate is tested at alpha 0.05, two with Holm at alpha 0.025 for the better one.
ALPHA = Fraction(1, 20)
HOLM_ALPHA = Fraction(1, 40)
CANDIDATES = ("u_final", "u_best_dev")

# Section 4.2: opponent ID -> (role, pairs K, seed, policy B spec rule).
FAIR_SEARCH_IDS = ("pimc:sims=16:depth=24:closedlist-full-pool-heuristic-v1",
                   "pimc:sims=8:depth=24:closedlist-full-pool-heuristic-v1")
OPPONENTS = {
    "initial-weights/step-0": ("step_0", 200, 657_101),
    "uniform-legal/v1": ("uniform_legal", 200, 657_102),
    "sampled-public-heuristic/v1": ("heuristic", 200, 657_103),
    FAIR_SEARCH_IDS[0]: ("fair_search", 100, 657_104),
    FAIR_SEARCH_IDS[1]: ("fair_search", 100, 657_104),
}
ROLES = ("step_0", "uniform_legal", "heuristic", "fair_search")

# Section 4.3 gates: each test is (role, threshold on the lower bound, strict).
GATE_TESTS = {
    "E1": (("step_0", Fraction(52, 100)), ("uniform_legal", Fraction(60, 100))),
    "E2": (("heuristic", Fraction(50, 100)),),
    "E3": (("fair_search", Fraction(50, 100)),),
}
STALL_RATE_MAX = Fraction(2, 100)
REPLAY_PAIRS = range(0, 4)

# Section 1.2 and 4.3: the eval split and its audit subgroups.
SAMPLE_SHA256 = "c7e9c271e15150c245aeabf5f2d10d73fbc41aa1586645101c64198d5562d23b"
REGISTRY_SHA256 = {
    "data/cards_v1.json": "ef738001c373076039f211582f5fd07b11903788e70641ec7e23c84ba4f69b14",
    "data/limited/fdn_v1/cards_v1.json": "91dcd869bb339450d0adc3e8cfa1a8320f560e53c337f21fb263a7a059839e13",
}
# Section 4.2: the heuristic and the search opponent sample worlds from the 40-deck train corpus.
BELIEF_ROLES = ("heuristic", "fair_search")
BELIEF_DECKS = 40
# Summary fields that identify the build and the decks; a run and its replay, and all runs of one
# evaluation, must agree on each (the last two only where the driver writes them).
BUILD_KEYS = ("engine", "card_db_hash", "session", "full_validation", "playable_decks_sha256")
DECK_CONTENT_KEYS = ("game_deck_contents_sha256", "belief_deck_contents_sha256")
# What the driver writes when a seat is a learner checkpoint: the SHA-256 of each learner's checkpoint
# file and the learner policy contract. A run and its replay must agree on all three.
LEARNER_KEYS = ("a_checkpoint_sha256", "b_checkpoint_sha256", "learner_policy_contract")
LEARNER_SPEC_PREFIX = "learner-checkpoint:"
EVAL_DECKS = 40
EVAL_FULL_ONLY = 19
OVER_40_DECKS = ("FDN_top_03718_UR", "FDN_top_21511_WUR", "FDN_top_24789_BGpW")
DRAKE_DECKS = ("FDN_top_03718_UR", "FDN_top_13096_WU")

RESULTS = ("A", "B", "draw", "stall", "error")
OUTCOME = {"A": "win", "B": "loss", "draw": "draw", "stall": "stall", "error": "error"}
# Candidate half-points per game: the primary column scores a stall as a draw, the sensitivity
# column as a loss; an engine error scores as a loss in both (it also fails Gate E0).
HALF_POINTS = {"primary": {"A": 2, "draw": 1, "stall": 1, "B": 0, "error": 0},
               "stall_sensitivity": {"A": 2, "draw": 1, "stall": 0, "B": 0, "error": 0}}
ROW_KEYS = ("decks", "side", "pols", "starting", "winner", "turns", "decisions", "rollout_errors")
SUMMARY_CANONICAL_KEYS = frozenset({
    "schema", "engine", "card_db_hash", "a", "b", "split", "seed", "first_pair", "pairs", "legs",
    "playable_decks", "playable_decks_sha256", "belief_overlap", "belief_decks", "games", "a_wins",
    "b_wins", "draws", "stalls", "errors", "full_validation", "session", "world_contract",
    "game_deck_contents_sha256", "belief_deck_contents_sha256", "max_turns", "max_decisions",
    "a_checkpoint_sha256",
    "b_checkpoint_sha256",
    "learner_policy_contract"})
SUMMARY_TIMING_KEYS = frozenset({"wall_s", "games_per_hour", "workers"})
SUMMARY_REQUIRED = ("schema", "engine", "card_db_hash", "a", "b", "split", "seed", "first_pair", "pairs", "legs",
                    "playable_decks", "playable_decks_sha256", "belief_overlap", "belief_decks", "games",
                    "a_wins", "b_wins", "draws", "stalls", "errors", "full_validation", "session", "workers")


class EvaluationInputError(ValueError):
    """The records are malformed, incomplete or not the design's evaluation."""


# ---------------------------------------------------------------------------------------------
# Generator

class SplitMix64:
    """Mirror of `mtg_kernel::state::SplitMix64`: add the golden gamma, then mix."""

    def __init__(self, seed: int) -> None:
        if type(seed) is not int or not 0 <= seed <= MASK64:
            raise ValueError("seed must be an unsigned 64-bit integer")
        self.state = seed

    def next_u64(self) -> int:
        self.state = (self.state + GOLDEN_GAMMA) & MASK64
        z = self.state
        z = ((z ^ (z >> 30)) * 0xBF58_476D_1CE4_E5B9) & MASK64
        z = ((z ^ (z >> 27)) * 0x94D0_49BB_1331_11EB) & MASK64
        return z ^ (z >> 31)


def unbiased_index(size: int, rng: SplitMix64) -> int:
    """Uniform index in [0, size): reject draws in the incomplete top bucket, then take the remainder."""
    limit = ((1 << 64) // size) * size
    while True:
        value = rng.next_u64()
        if value < limit:
            return value % size


@lru_cache(maxsize=4)
def bootstrap_indices(pairs: int, replicates: int, seed: int) -> array:
    """Resample indices, replicate-major: replicate r draws `pairs` indices from one stream."""
    rng = SplitMix64(seed)
    return array("I", (unbiased_index(pairs, rng) for _ in range(pairs * replicates)))


# ---------------------------------------------------------------------------------------------
# Deterministic normal distribution (Decimal arithmetic only, no platform libm)

_PI = Decimal("3.14159265358979323846264338327950288419716939937510582097494459230781640628620899863")


def _context() -> Context:
    return Context(prec=60)


def normal_cdf(x: Decimal) -> Decimal:
    """Phi(x) = 1/2 + phi(x) * sum x^(2n+1) / (2n+1)!!, clamped beyond |x| = 12."""
    with localcontext(_context()):
        x = Decimal(x)
        if x > 12:
            return Decimal(1)
        if x < -12:
            return Decimal(0)
        square = x * x
        term, total, n = x, x, 0
        tolerance = Decimal(10) ** -70
        while abs(term) > tolerance:
            term = term * square / (2 * n + 3)
            total += term
            n += 1
        density = (-square / 2).exp() / (2 * _PI).sqrt()
        return Decimal("0.5") + density * total


@lru_cache(maxsize=None)
def normal_ppf(p: Fraction) -> Decimal:
    """Inverse of `normal_cdf` by bisection on [-12, 12]; p must lie strictly inside (0, 1)."""
    if not 0 < p < 1:
        raise ValueError("p must lie strictly between 0 and 1")
    with localcontext(_context()):
        target = Decimal(p.numerator) / Decimal(p.denominator)
        low, high = Decimal(-12), Decimal(12)
        for _ in range(110):
            middle = (low + high) / 2
            if normal_cdf(middle) < target:
                low = middle
            else:
                high = middle
        return +((low + high) / 2)


# ---------------------------------------------------------------------------------------------
# Statistics over pair half-point totals (each pair total is in 0..4; y_p = total / 4)

def _fraction_text(value: Fraction) -> str:
    return f"{value.numerator}/{value.denominator}"


def _decimal_text(value: Decimal) -> str:
    with localcontext(_context()):
        return format(value.quantize(Decimal(10) ** -30), "f")


def pair_sd(totals: list[int]) -> float | None:
    """Empirical SD of the pair means y_p with the K - 1 denominator."""
    k = len(totals)
    if k < 2:
        return None
    mean = Fraction(sum(totals), 4 * k)
    variance = sum((Fraction(t, 4) - mean) ** 2 for t in totals) / (k - 1)
    with localcontext(_context()):
        return float((Decimal(variance.numerator) / Decimal(variance.denominator)).sqrt())


def _two_sided_symmetric(weights: dict[int, int], observed: int, center2: int) -> Fraction:
    """P(|T - E| >= |t - E|) for a distribution symmetric about E; values are doubled (2T, 2E)."""
    distance = abs(2 * observed - center2)
    total = sum(weights.values())
    hit = sum(w for t, w in weights.items() if abs(2 * t - center2) >= distance)
    return Fraction(hit, total)


def sign_test(totals: list[int]) -> dict:
    """Exact two-sided sign test on discordant pairs (y_p > 0.5 against y_p < 0.5)."""
    wins = sum(t > 2 for t in totals)
    losses = sum(t < 2 for t in totals)
    n = wins + losses
    weights = {k: math.comb(n, k) for k in range(n + 1)}
    p = _two_sided_symmetric(weights, wins, n)
    return {"better_pairs": wins, "worse_pairs": losses, "tied_pairs": len(totals) - n,
            "p_value": float(p), "p_value_exact": _fraction_text(p)}


def wilcoxon_test(totals: list[int]) -> dict:
    """Exact two-sided Wilcoxon signed-rank test on discordant pairs, midranks for tied |y_p - 0.5|.

    The null distribution is the exact one conditional on the tie pattern: every discordant pair's
    sign is a fair coin. Ranks are doubled so every sum stays an integer.
    """
    differences = [t - 2 for t in totals if t != 2]
    n = len(differences)
    magnitudes = sorted({abs(d) for d in differences})
    groups = []
    below = 0
    for magnitude in magnitudes:
        size = sum(abs(d) == magnitude for d in differences)
        doubled_rank = 2 * below + size + 1
        positives = sum(d == magnitude for d in differences)
        groups.append((doubled_rank, size, positives))
        below += size
    weights = {0: 1}
    for doubled_rank, size, _ in groups:
        step: dict[int, int] = {}
        for value, weight in weights.items():
            for k in range(size + 1):
                key = value + doubled_rank * k
                step[key] = step.get(key, 0) + weight * math.comb(size, k)
        weights = step
    observed2 = sum(rank * positives for rank, _, positives in groups)
    center2 = sum(rank * size for rank, size, _ in groups)  # 2 * E[2T+] = sum of doubled ranks
    # Work in doubled statistic units: value = 2 T+, center of 2 T+ is center2 / 2.
    p = _two_sided_symmetric(weights, observed2, center2)
    return {"discordant_pairs": n, "w_plus": observed2 / 2, "w_plus_expected": center2 / 4,
            "p_value": float(p), "p_value_exact": _fraction_text(p)}


def wilson_interval(successes: int, trials: int) -> dict | None:
    """95% Wilson score interval in Decimal arithmetic (z = 1.959963984540054)."""
    if trials == 0:
        return None
    with localcontext(_context()):
        s, n = Decimal(successes), Decimal(trials)
        z = Decimal("1.959963984540054")
        z2 = z * z
        p = s / n
        denominator = 1 + z2 / n
        center = (p + z2 / (2 * n)) / denominator
        margin = z * (p * (1 - p) / n + z2 / (4 * n * n)).sqrt() / denominator
        lower, upper = max(Decimal(0), center - margin), min(Decimal(1), center + margin)
    return {"successes": successes, "trials": trials, "estimate": successes / trials,
            "lower": float(lower), "upper": float(upper)}


def elo_difference(totals: list[int]) -> float | None:
    """400 log10(ybar / (1 - ybar)); undefined at a score of 0 or 1."""
    s, whole = sum(totals), 4 * len(totals)
    if s in (0, whole):
        return None
    with localcontext(_context()):
        return float(400 * (Decimal(s) / Decimal(whole - s)).log10())


def percentile_indices(replicates: int, alpha: Fraction) -> tuple[int, int]:
    """Order-statistic indices: floor(alpha/2 (R-1)) and ceil((1 - alpha/2) (R-1))."""
    low = alpha / 2 * (replicates - 1)
    high = (1 - alpha / 2) * (replicates - 1)
    return math.floor(low), math.ceil(high)


def bootstrap(totals: list[int], alphas: tuple[Fraction, ...], replicates: int = BOOTSTRAP_REPLICATES,
              seed: int = BOOTSTRAP_SEED) -> dict:
    """Paired percentile and BCa bootstrap over the pair totals, one interval per alpha.

    The gate bound is the lower of the two lower bounds (the BCa one when it is defined).
    """
    k = len(totals)
    indices = bootstrap_indices(k, replicates, seed)
    sums = [sum(totals[i] for i in indices[r * k:(r + 1) * k]) for r in range(replicates)]
    digest = hashlib.sha256((",".join(map(str, sums)) + "\n").encode("ascii")).hexdigest()
    ordered = sorted(sums)
    observed = sum(totals)
    below = sum(s < observed for s in sums)
    centered = [k * t - observed for t in totals]
    second = sum(d * d for d in centered)
    third = sum(d * d * d for d in centered)
    bca_reason = None
    if second == 0:
        bca_reason = "all pair totals are equal"
    elif below in (0, replicates):
        bca_reason = "no replicate lies below the observed mean" if below == 0 else "every replicate lies below"
    with localcontext(_context()):
        acceleration = Decimal(third) / (6 * Decimal(second) * Decimal(second).sqrt()) if second else Decimal(0)
        z0 = normal_ppf(Fraction(below, replicates)) if bca_reason is None else None
    out = {"replicates": replicates, "seed": seed, "replicate_sums_sha256": digest,
           "observed_sum": observed, "replicates_below_observed": below,
           "bca_acceleration": _decimal_text(acceleration), "bca_z0": None if z0 is None else _decimal_text(z0),
           "intervals": {}}
    whole = 4 * k
    for alpha in alphas:
        low_index, high_index = percentile_indices(replicates, alpha)
        interval: dict[str, Any] = {
            "alpha": _fraction_text(alpha),
            "percentile": {"lower_index": low_index, "upper_index": high_index,
                           "lower_sum": ordered[low_index], "upper_sum": ordered[high_index],
                           "lower": ordered[low_index] / whole, "upper": ordered[high_index] / whole}}
        bca: dict[str, Any] = {"defined": False, "reason": bca_reason}
        if bca_reason is None:
            with localcontext(_context()):
                adjusted = []
                for tail in (alpha / 2, 1 - alpha / 2):
                    w = z0 + normal_ppf(tail)
                    denominator = 1 - acceleration * w
                    if denominator <= 0:
                        adjusted = None
                        break
                    adjusted.append(normal_cdf(z0 + w / denominator))
                if adjusted is None:
                    bca["reason"] = "acceleration leaves the adjustment undefined"
                else:
                    span = Decimal(replicates - 1)
                    lo = int((adjusted[0] * span).to_integral_value(rounding=ROUND_FLOOR))
                    hi = int((adjusted[1] * span).to_integral_value(rounding=ROUND_CEILING))
                    lo, hi = min(max(lo, 0), replicates - 1), min(max(hi, 0), replicates - 1)
                    bca = {"defined": True, "reason": None, "lower_alpha": _decimal_text(adjusted[0]),
                           "upper_alpha": _decimal_text(adjusted[1]), "lower_index": lo, "upper_index": hi,
                           "lower_sum": ordered[lo], "upper_sum": ordered[hi],
                           "lower": ordered[lo] / whole, "upper": ordered[hi] / whole}
        interval["bca"] = bca
        gate_sum = min(ordered[low_index], bca["lower_sum"]) if bca["defined"] else ordered[low_index]
        interval["gate_lower_sum"] = gate_sum
        interval["gate_lower"] = gate_sum / whole
        out["intervals"][_fraction_text(alpha)] = interval
    return out


def column_statistics(totals: list[int], replicates: int, seed: int) -> dict:
    """Section 4.3 item 1 for one scoring column."""
    k = len(totals)
    return {"pairs": k, "half_points": sum(totals), "mean": sum(totals) / (4 * k), "pair_sd": pair_sd(totals),
            "pair_means": {str(v / 4): sum(t == v for t in totals) for v in range(5)},
            "bootstrap": bootstrap(totals, (ALPHA, HOLM_ALPHA), replicates, seed),
            "sign_test": sign_test(totals), "wilcoxon": wilcoxon_test(totals),
            "elo_difference": elo_difference(totals)}


# ---------------------------------------------------------------------------------------------
# Inventory (Section 1.2) and its subgroups

def load_inventory(path: Path | None) -> dict:
    """The deck inventory from `limited_training_inventory_v1`, or from a saved copy of its output."""
    if path is not None:
        return json.loads(path.read_text(encoding="utf-8"))
    sys.path.insert(0, str(Path(__file__).resolve().parent))
    import limited_training_inventory_v1
    return limited_training_inventory_v1.inventory()


def eval_subgroups(inventory: dict) -> dict:
    """Check the eval split against the design's pins and return its per-deck classes."""
    if inventory.get("schema") != "limited-training-deck-inventory/v1":
        raise EvaluationInputError("inventory schema is not limited-training-deck-inventory/v1")
    if inventory.get("sample_sha256_in_pool_order") != SAMPLE_SHA256:
        raise EvaluationInputError("inventory sample_sha256_in_pool_order differs from the design's pin")
    if inventory.get("registry_sha256") != REGISTRY_SHA256:
        raise EvaluationInputError("inventory card-file SHA-256 values differ from the design's pins")
    decks = [d for d in inventory["decks"] if d["split"] == "eval"]
    # The driver deals pair p from the playable eval decks in pool order and records the SHA-256 of
    # their names, each followed by a newline; the inventory lists the decks in pool order.
    pool_order = hashlib.sha256("".join(f"{d['deck']}\n" for d in decks).encode("utf-8")).hexdigest()
    full = sorted(d["deck"] for d in decks if d["declared_full_only"])
    over_40 = sorted(d["deck"] for d in decks if d["cards"] > 40)
    drake = sorted(d["deck"] for d in decks if d["contains_drake_hatcher"])
    if len(decks) != EVAL_DECKS or len(full) != EVAL_FULL_ONLY:
        raise EvaluationInputError(f"eval split has {len(decks)} decks, {len(full)} Full-only; design says 40, 19")
    if tuple(over_40) != OVER_40_DECKS or tuple(drake) != DRAKE_DECKS:
        raise EvaluationInputError("eval over-40 or Drake Hatcher decks differ from the design's lists")
    return {"decks": sorted(d["deck"] for d in decks), "full_only": full, "over_40": over_40, "drake": drake,
            "playable_decks_sha256": pool_order}


# ---------------------------------------------------------------------------------------------
# Records

def read_jsonl(path: Path) -> list[dict]:
    rows = []
    for number, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        if not line.strip():
            continue
        try:
            row = json.loads(line)
        except json.JSONDecodeError as error:
            raise EvaluationInputError(f"{path.name} line {number}: not JSON ({error.msg})") from None
        if not isinstance(row, dict):
            raise EvaluationInputError(f"{path.name} line {number}: not a JSON object")
        rows.append(row)
    return rows


def read_summary(games_path: Path) -> dict:
    path = Path(f"{games_path}.summary.json")
    if not path.is_file():
        raise EvaluationInputError(f"missing summary {path.name} beside {games_path.name}")
    summary = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(summary, dict):
        raise EvaluationInputError(f"{path.name} is not a JSON object")
    return summary


def canonical_row(row: dict) -> str:
    """A game record without its wall-time field, keys sorted."""
    return json.dumps({k: v for k, v in row.items() if k != "ms"}, sort_keys=True, separators=(",", ":"))


def _is_int(value: Any) -> bool:
    return type(value) is int


def check_specs(summary: dict, label: str, spec_b_ok) -> None:
    """Policy A is the candidate checkpoint and policy B the named opponent (Section 4.2)."""
    if not str(summary.get("a")).startswith("learner-checkpoint:"):
        raise EvaluationInputError(f"{label}: policy A {summary.get('a')!r} is not a learner-checkpoint spec")
    if not spec_b_ok(str(summary.get("b"))):
        raise EvaluationInputError(f"{label}: policy B {summary.get('b')!r} is not this opponent's spec")


def _is_sha256(value: Any) -> bool:
    return isinstance(value, str) and len(value) == 64 and all(c in "0123456789abcdef" for c in value)


def check_learner_keys(summary: dict, label: str) -> None:
    """A learner seat's checkpoint digest is present and valid exactly when that policy is a learner, and the
    learner policy contract is present exactly when a seat is one."""
    learner = False
    for side in ("a", "b"):
        key = f"{side}_checkpoint_sha256"
        if str(summary.get(side)).startswith(LEARNER_SPEC_PREFIX):
            learner = True
            if not _is_sha256(summary.get(key)):
                raise EvaluationInputError(f"{label}: summary {key} is missing or not a SHA-256 digest")
        elif key in summary:
            raise EvaluationInputError(f"{label}: summary {key} for a policy that is not a learner checkpoint")
    contract = summary.get("learner_policy_contract")
    if learner != (contract is not None) or (learner and not (isinstance(contract, str) and contract)):
        raise EvaluationInputError(f"{label}: summary learner_policy_contract does not match its learner seats")


def check_summary(summary: dict, label: str, *, seed: int, pairs: int, spec_b_ok, games: list[dict],
                  role: str) -> None:
    missing = [k for k in SUMMARY_REQUIRED if k not in summary]
    if missing:
        raise EvaluationInputError(f"{label}: summary lacks {missing}")
    unknown = sorted(set(summary) - SUMMARY_CANONICAL_KEYS - SUMMARY_TIMING_KEYS)
    if unknown:
        raise EvaluationInputError(f"{label}: summary has unknown keys {unknown}")
    expected = {"schema": MATCH_SCHEMA, "split": "eval", "seed": seed, "first_pair": 0, "pairs": pairs, "legs": 2,
                "playable_decks": EVAL_DECKS, "session": SESSION, "games": 2 * pairs}
    for key, value in expected.items():
        if summary[key] != value or type(summary[key]) is not type(value):
            raise EvaluationInputError(f"{label}: summary {key} is {summary[key]!r}, expected {value!r}")
    for key, value in (("max_turns", MAX_TURNS), ("max_decisions", MAX_DECISIONS)):
        if key in summary and summary[key] != value:
            raise EvaluationInputError(f"{label}: summary {key} is {summary[key]!r}, expected {value}")
    check_specs(summary, label, spec_b_ok)
    check_learner_keys(summary, label)
    if not _is_int(summary["workers"]) or summary["workers"] < 1:
        raise EvaluationInputError(f"{label}: summary workers is not a positive count")
    if not _is_int(summary["belief_overlap"]) or summary["belief_overlap"] < 0:
        raise EvaluationInputError(f"{label}: summary belief_overlap is not a count")
    if role in BELIEF_ROLES and (summary["belief_decks"] != BELIEF_DECKS or not _is_int(summary["belief_decks"])):
        raise EvaluationInputError(
            f"{label}: belief_decks is {summary['belief_decks']!r}, the design's corpus has {BELIEF_DECKS} decks")
    tallies = {"a_wins": "A", "b_wins": "B", "draws": "draw", "stalls": "stall", "errors": "error"}
    for key, result in tallies.items():
        count = sum(g["result"] == result for g in games)
        if summary[key] != count:
            raise EvaluationInputError(f"{label}: summary {key} = {summary[key]!r} but the rows hold {count}")


def check_rows(rows: list[dict], label: str, *, seed: int, pairs: int, spec_a: str, spec_b: str,
               eval_decks: set[str]) -> list[dict]:
    """Validate one run's rows and return them ordered by (pair, leg)."""
    by_key: dict[tuple[int, int], dict] = {}
    for row in rows:
        for key in ("game", "pair", "leg", "seed", "winner"):
            if not _is_int(row.get(key)):
                raise EvaluationInputError(f"{label}: a row's {key} is missing or not an integer")
        pair, leg, result = row["pair"], row["leg"], row.get("result")
        where = f"{label} pair {pair} leg {leg}"
        if result not in RESULTS:
            raise EvaluationInputError(f"{where}: unknown result {result!r}")
        if not 0 <= pair < pairs or leg not in (0, 1):
            raise EvaluationInputError(f"{where}: outside pairs 0..{pairs} or legs 0..2")
        if row["game"] != 2 * pair + leg or row["seed"] != seed:
            raise EvaluationInputError(f"{where}: game index or seed does not match the run")
        if (pair, leg) in by_key:
            raise EvaluationInputError(f"{where}: duplicate record")
        if result != "error" or "decks" in row:
            for key in ROW_KEYS:
                if key not in row:
                    raise EvaluationInputError(f"{where}: missing {key}")
            side = ["A", "B"] if leg == 0 else ["B", "A"]
            decks = row["decks"]
            if row["side"] != side:
                raise EvaluationInputError(f"{where}: side {row['side']!r}, expected {side}")
            if row["pols"] != [spec_a if s == "A" else spec_b for s in side]:
                raise EvaluationInputError(f"{where}: pols do not match the summary's policies")
            if (not isinstance(decks, list) or len(decks) != 2 or decks[0] == decks[1]
                    or not all(isinstance(d, str) and d in eval_decks for d in decks)):
                raise EvaluationInputError(f"{where}: decks {decks!r} are not two distinct eval decks")
            if row["starting"] not in (0, 1) or type(row["starting"]) is not int:
                raise EvaluationInputError(f"{where}: starting is not 0 or 1")
            for key in ("turns", "decisions", "rollout_errors"):
                if not _is_int(row[key]) or row[key] < 0:
                    raise EvaluationInputError(f"{where}: {key} is not a count")
            winner = side.index(result) if result in ("A", "B") else -1
            if row["winner"] != winner:
                raise EvaluationInputError(f"{where}: winner {row['winner']} disagrees with result {result}")
        elif "rollout_errors" in row and (not _is_int(row["rollout_errors"]) or row["rollout_errors"] < 0):
            raise EvaluationInputError(f"{where}: rollout_errors is not a count")
        by_key[pair, leg] = row
    missing = [(p, l) for p in range(pairs) for l in (0, 1) if (p, l) not in by_key]
    if missing:
        raise EvaluationInputError(f"{label}: {len(missing)} games missing, first {missing[0]}")
    for pair in range(pairs):
        first, second = by_key[pair, 0], by_key[pair, 1]
        if "decks" in first and "decks" in second:
            for key in ("decks", "starting", "env_seed"):
                if first.get(key) != second.get(key):
                    raise EvaluationInputError(f"{label} pair {pair}: legs differ in {key}")
    return [by_key[p, l] for p in range(pairs) for l in (0, 1)]


# ---------------------------------------------------------------------------------------------
# One run

@dataclass
class RunInput:
    candidate: str
    opponent: str
    rows: list[dict]
    summary: dict
    replay_rows: list[dict]
    replay_summary: dict


def _spec_rule(opponent: str):
    if opponent == "initial-weights/step-0":
        return lambda spec: spec.startswith("learner-checkpoint:") and spec.endswith("step_0000.json")
    if opponent == "uniform-legal/v1":
        return lambda spec: spec == "random"
    return lambda spec: spec == opponent


def _tally(games: list[dict]) -> dict:
    counts = {"games": len(games), "win": 0, "draw": 0, "loss": 0, "stall": 0, "error": 0}
    for game in games:
        counts[OUTCOME[game["result"]]] += 1
    return counts


def _pair_group(pairs: list[list[dict]]) -> dict:
    games = [g for pair in pairs for g in pair]
    totals = [sum(HALF_POINTS["primary"][g["result"]] for g in pair) for pair in pairs]
    group = _tally(games)
    group.update({"pairs": len(pairs), "half_points": sum(totals),
                  "mean": sum(totals) / (4 * len(pairs)) if pairs else None})
    return group


def subgroup_audit(games: list[dict], classes: dict) -> dict:
    """Section 4.3 item 3: W/D/L/stall by the candidate's deck class, by pair stratum, and for the
    pairs that hold an over-40 deck or a Drake Hatcher deck. A game that errored before its deal
    has no decks and is counted under `games_without_deal` only."""
    full = set(classes["full_only"])
    dealt = [g for g in games if "decks" in g]
    per_leg = {"full_only": [], "contains_partial": []}
    for game in dealt:
        deck = game["decks"][game["side"].index("A")]
        per_leg["full_only" if deck in full else "contains_partial"].append(game)
    pairs: dict[int, list[dict]] = {}
    for game in dealt:
        pairs.setdefault(game["pair"], []).append(game)
    complete = [legs for _, legs in sorted(pairs.items()) if len(legs) == 2]
    strata = {"full_vs_full": [], "mixed": [], "partial_vs_partial": []}
    for legs in complete:
        fulls = sum(d in full for d in legs[0]["decks"])
        strata[("partial_vs_partial", "mixed", "full_vs_full")[fulls]].append(legs)

    def involving(names: list[str]) -> dict:
        group = _pair_group([legs for legs in complete if set(legs[0]["decks"]) & set(names)])
        group["by_deck"] = {name: _pair_group([legs for legs in complete if name in legs[0]["decks"]])
                            for name in names}
        return group

    return {"games_without_deal": len(games) - len(dealt),
            "per_leg_by_candidate_deck": {k: _tally(v) for k, v in per_leg.items()},
            "per_pair_stratum": {k: _pair_group(v) for k, v in strata.items()},
            "pairs_with_over_40_deck": involving(classes["over_40"]),
            "pairs_with_drake_hatcher_deck": involving(classes["drake"])}


def analyze_run(run: RunInput, classes: dict, replicates: int, seed: int) -> dict:
    role, pairs, run_seed = OPPONENTS[run.opponent]
    label = f"{run.candidate} vs {run.opponent}"
    rule = _spec_rule(run.opponent)
    eval_decks = set(classes["decks"])
    check_specs(run.summary, label, rule)
    spec_a, spec_b = str(run.summary.get("a")), str(run.summary.get("b"))
    games = check_rows(run.rows, label, seed=run_seed, pairs=pairs, spec_a=spec_a, spec_b=spec_b,
                       eval_decks=eval_decks)
    check_summary(run.summary, label, seed=run_seed, pairs=pairs, spec_b_ok=rule, games=games, role=role)

    # Replay: the same command at another worker count, at least pairs 0..4.
    replay_label = f"{label} (replay)"
    replay_pairs = run.replay_summary.get("pairs")
    if not _is_int(replay_pairs) or replay_pairs < len(REPLAY_PAIRS) or replay_pairs > pairs:
        raise EvaluationInputError(f"{replay_label}: summary pairs must cover pairs 0..{len(REPLAY_PAIRS)}")
    replay_games = check_rows(run.replay_rows, replay_label, seed=run_seed, pairs=replay_pairs,
                              spec_a=spec_a, spec_b=spec_b, eval_decks=eval_decks)
    check_summary(run.replay_summary, replay_label, seed=run_seed, pairs=replay_pairs, spec_b_ok=rule,
                  games=replay_games, role=role)
    if (run.replay_summary["a"], run.replay_summary["b"]) != (spec_a, spec_b):
        raise EvaluationInputError(f"{replay_label}: policies differ from the main run")
    main_lines = [canonical_row(g) for g in games if g["pair"] in REPLAY_PAIRS]
    replay_lines = [canonical_row(g) for g in replay_games if g["pair"] in REPLAY_PAIRS]
    main_digest = hashlib.sha256("\n".join(main_lines).encode("utf-8")).hexdigest()
    replay_digest = hashlib.sha256("\n".join(replay_lines).encode("utf-8")).hexdigest()
    workers = (run.summary["workers"], run.replay_summary["workers"])
    replay = {"pairs": [REPLAY_PAIRS.start, REPLAY_PAIRS.stop], "games_canonical_sha256": main_digest,
              "replay_games_canonical_sha256": replay_digest, "identical": main_lines == replay_lines,
              "worker_counts_differ": workers[0] != workers[1]}

    counts = _tally(games)
    rollout_errors = sum(g.get("rollout_errors", 0) for g in games)
    stall_rate = Fraction(counts["stall"], len(games))
    integrity = {"errors": counts["error"], "rollout_errors": rollout_errors,
                 "belief_overlap": run.summary["belief_overlap"], "stalls": counts["stall"],
                 "stall_rate": float(stall_rate), "stall_rate_exact": _fraction_text(stall_rate),
                 "limits_recorded": "max_turns" in run.summary and "max_decisions" in run.summary,
                 "playable_decks_in_design_order": run.summary["playable_decks_sha256"]
                 == classes["playable_decks_sha256"],
                 "replay_build_differs": [k for k in BUILD_KEYS + DECK_CONTENT_KEYS
                                          if run.summary.get(k) != run.replay_summary.get(k)],
                 # The replay plays the same checkpoint files under the same learner contract.
                 "replay_learner_differs": [k for k in LEARNER_KEYS
                                            if run.summary.get(k) != run.replay_summary.get(k)],
                 "replay": replay}
    failures = []
    if integrity["replay_learner_differs"]:
        failures.append("replay_learner_differs")
    if counts["error"]:
        failures.append("errors")
    if rollout_errors:
        failures.append("rollout_errors")
    if run.summary["belief_overlap"]:
        failures.append("belief_overlap")
    if stall_rate > STALL_RATE_MAX:
        failures.append("stall_rate")
    if not replay["identical"]:
        failures.append("replay_not_identical")
    if not replay["worker_counts_differ"]:
        failures.append("replay_same_worker_count")
    if not integrity["limits_recorded"]:
        failures.append("limits_not_recorded")
    if not integrity["playable_decks_in_design_order"]:
        failures.append("playable_decks_not_in_design_order")
    if integrity["replay_build_differs"]:
        failures.append("replay_build_differs")
    integrity["failures"] = failures
    integrity["pass"] = not failures

    columns = {}
    for name, points in HALF_POINTS.items():
        totals = [points[games[2 * p]["result"]] + points[games[2 * p + 1]["result"]] for p in range(pairs)]
        columns[name] = column_statistics(totals, replicates, seed)
    decisive = counts["win"] + counts["loss"]
    summary_canonical = {k: v for k, v in run.summary.items() if k in SUMMARY_CANONICAL_KEYS}
    return {"opponent": run.opponent, "role": role, "pairs": pairs, "seed": run_seed,
            "candidate_spec": spec_a, "opponent_spec": spec_b, "summary": summary_canonical,
            "games_canonical_sha256": hashlib.sha256(
                "\n".join(canonical_row(g) for g in games).encode("utf-8")).hexdigest(),
            "counts": counts, "integrity": integrity, "primary": columns["primary"],
            "stall_sensitivity": columns["stall_sensitivity"],
            "wilson_descriptive": {"wins_over_decisive_games": wilson_interval(counts["win"], decisive),
                                   "wins_over_all_games": wilson_interval(counts["win"], len(games))},
            "subgroups": subgroup_audit(games, classes)}


# ---------------------------------------------------------------------------------------------
# Gates

def lower_bound_tests(runs: dict[str, dict[str, dict]], column: str) -> dict[str, dict[str, dict]]:
    """Each candidate's lower-bound tests, with the Holm adjustment when two candidates are evaluated.

    With two candidates, for each opponent the candidate with the higher mean (the better of the two)
    is tested at alpha 0.025 (97.5% interval); the other is tested at alpha 0.05 only if the better
    one passed (Holm step-down), else it fails. Equal means leave no order: both are tested at 0.025.
    """
    candidates = sorted(runs)
    tests: dict[str, dict[str, dict]] = {c: {} for c in candidates}
    thresholds = {role: threshold for gate in GATE_TESTS.values() for role, threshold in gate}
    for role, threshold in thresholds.items():
        present = [c for c in candidates if role in runs[c]]
        stats = {c: runs[c][role][column] for c in present}

        def passes(candidate: str, alpha: Fraction) -> bool:
            interval = stats[candidate]["bootstrap"]["intervals"][_fraction_text(alpha)]
            return Fraction(interval["gate_lower_sum"], 4 * stats[candidate]["pairs"]) > threshold

        def record(candidate: str, alpha: Fraction, passed: bool, rule: str) -> None:
            interval = stats[candidate]["bootstrap"]["intervals"][_fraction_text(alpha)]
            tests[candidate][role] = {"threshold": float(threshold), "alpha": _fraction_text(alpha),
                                      "lower_bound": interval["gate_lower"], "pass": passed, "rule": rule}

        if len(candidates) == 1:
            for c in present:
                record(c, ALPHA, passes(c, ALPHA), "single candidate, 95% interval")
            continue
        if len(present) < 2:
            for c in present:
                record(c, HOLM_ALPHA, passes(c, HOLM_ALPHA), "other candidate's run absent, 97.5% interval")
            continue
        means = {c: Fraction(stats[c]["half_points"], 4 * stats[c]["pairs"]) for c in present}
        first, second = sorted(present, key=lambda c: means[c], reverse=True)
        if means[first] == means[second]:
            for c in present:
                record(c, HOLM_ALPHA, passes(c, HOLM_ALPHA), "equal means, both at 97.5%")
            continue
        better_passed = passes(first, HOLM_ALPHA)
        record(first, HOLM_ALPHA, better_passed, "Holm: better of two, 97.5% interval")
        record(second, ALPHA, better_passed and passes(second, ALPHA),
               "Holm: second of two, 95% interval only after the better one passed")
    return tests


def gate_verdicts(tests: dict[str, dict], e0_pass: bool) -> dict:
    gates: dict[str, dict] = {}
    previous_pass, previous_name = e0_pass, "E0"
    for name, required in GATE_TESTS.items():
        missing = [role for role, _ in required if role not in tests]
        failed = [role for role, _ in required if role in tests and not tests[role]["pass"]]
        reasons = ([f"{previous_name} did not pass"] if not previous_pass else [])
        reasons += [f"no run against {role}" for role in missing] + [f"{role} lower bound" for role in failed]
        passed = not reasons
        gates[name] = {"pass": passed, "reasons": reasons}
        previous_pass, previous_name = passed, name
    return gates


def evaluate(inputs: list[RunInput], inventory: dict, replicates: int = BOOTSTRAP_REPLICATES,
             seed: int = BOOTSTRAP_SEED) -> dict:
    classes = eval_subgroups(inventory)
    seen: dict[tuple[str, str], str] = {}
    for run in inputs:
        if run.candidate not in CANDIDATES:
            raise EvaluationInputError(f"unknown candidate {run.candidate!r}; expected one of {CANDIDATES}")
        if run.opponent not in OPPONENTS:
            raise EvaluationInputError(f"unknown opponent {run.opponent!r}")
        key = (run.candidate, OPPONENTS[run.opponent][0])
        if key in seen:
            raise EvaluationInputError(f"two runs for {run.candidate} against {key[1]}")
        seen[key] = run.opponent
    for role in ROLES:
        ids = {opponent for (_, r), opponent in seen.items() if r == role}
        if len(ids) > 1:
            raise EvaluationInputError(f"candidates were evaluated against different {role} opponents {sorted(ids)}")
    if not inputs or "u_final" not in {r.candidate for r in inputs}:
        raise EvaluationInputError("no run of u_final: u_best_dev is evaluated only beside u_final")

    runs: dict[str, dict[str, dict]] = {}
    for run in sorted(inputs, key=lambda r: (r.candidate, OPPONENTS[r.opponent][0])):
        result = analyze_run(run, classes, replicates, seed)
        runs.setdefault(run.candidate, {})[result["role"]] = result
    specs = {c: {r["candidate_spec"] for r in roles.values()} for c, roles in runs.items()}
    for candidate, values in specs.items():
        if len(values) != 1:
            raise EvaluationInputError(f"{candidate}: runs name different candidate checkpoints {sorted(values)}")
    if len(runs) == 2 and specs["u_final"] == specs["u_best_dev"]:
        raise EvaluationInputError("u_final and u_best_dev name the same checkpoint")
    # The same path may hold another file between runs: the recorded file digests must agree as well.
    digests = {c: {r["summary"].get("a_checkpoint_sha256") for r in roles.values()} for c, roles in runs.items()}
    for candidate, values in digests.items():
        if len(values) != 1:
            raise EvaluationInputError(f"{candidate}: runs record different candidate checkpoint digests")
    if len(runs) == 2 and digests["u_final"] == digests["u_best_dev"]:
        raise EvaluationInputError("u_final and u_best_dev record the same checkpoint digest")
    step_zero = {roles["step_0"]["summary"].get("b_checkpoint_sha256") for roles in runs.values()
                 if "step_0" in roles}
    if len(step_zero) > 1:
        raise EvaluationInputError("candidates record different initial-weights/step-0 checkpoint digests")
    contracts = {r["summary"].get("learner_policy_contract") for roles in runs.values() for r in roles.values()}
    if len(contracts) > 1:
        raise EvaluationInputError("runs record different learner policy contracts")

    expected = [(c, role) for c in sorted(runs) for role in ROLES]
    absent = [f"{c} vs {role}" for c, role in expected if role not in runs[c]]
    failed = [f"{c} vs {role}: {', '.join(runs[c][role]['integrity']['failures'])}"
              for c, role in expected if role in runs[c] and not runs[c][role]["integrity"]["pass"]]
    all_runs = [runs[c][role] for c, role in expected if role in runs[c]]
    build_mismatches = []
    for key in BUILD_KEYS + DECK_CONTENT_KEYS:
        values = {json.dumps(r["summary"].get(key), sort_keys=True) for r in all_runs
                  if key in BUILD_KEYS or r["summary"].get(key) is not None}
        if len(values) > 1:
            build_mismatches.append(key)
    e0 = {"pass": not absent and not failed and not build_mismatches, "absent_runs": absent,
          "failed_runs": failed, "build_mismatches_across_runs": build_mismatches}

    candidates: dict[str, dict] = {}
    gates_by_column = {}
    for column in HALF_POINTS:
        tests = lower_bound_tests(runs, column)
        gates_by_column[column] = {c: (tests[c], gate_verdicts(tests[c], e0["pass"])) for c in runs}
    for candidate in sorted(runs):
        tests, gates = gates_by_column["primary"][candidate]
        sens_tests, sens_gates = gates_by_column["stall_sensitivity"][candidate]
        highest = ([name for name in GATE_TESTS if gates[name]["pass"]] or ["none"])[-1]
        entry = {"checkpoint_spec": next(iter(specs[candidate])), "runs": runs[candidate],
                 "tests": tests, "gates": gates, "highest_gate_passed": highest,
                 "stall_sensitivity": {"tests": sens_tests, "gates": sens_gates},
                 # Section 4.3 and the lead's ruling: the sensitivity column is not a gate, but claim text
                 # must state it when that column fails; these are the gates where the two columns disagree.
                 "gates_differing_under_stall_sensitivity": [
                     name for name in GATE_TESTS if gates[name]["pass"] != sens_gates[name]["pass"]]}
        fair = runs[candidate].get("fair_search")
        if gates["E1"]["pass"] and gates["E2"]["pass"] and not gates["E3"]["pass"] and fair is not None:
            entry["fair_search_report"] = {"mean": fair["primary"]["mean"],
                                           "elo_difference": fair["primary"]["elo_difference"],
                                           "note": "E1 and E2 pass, E3 does not: no search superiority claim"}
        candidates[candidate] = entry
    return {"schema": SCHEMA, "design": "docs/design/limited_campaign_v1.md section 4",
            "bootstrap": {"generator": "splitmix64 (mtg_kernel::state::SplitMix64)", "seed": seed,
                          "replicates": replicates,
                          "index_rule": "reject draws at or above floor(2^64 / K) * K, then draw mod K",
                          "percentile_rule": "order statistics floor(alpha/2 (R-1)) and ceil((1-alpha/2)(R-1))",
                          "gate_bound": "the lower of the percentile and BCa lower bounds",
                          "stream_rule": "every run and each scoring column draws its replicate-major indices from "
                                         "the same generator stream, restarted at the seed, so no draw depends on "
                                         "argument or file order; runs with the same pair count draw the same "
                                         "indices"},
            "inventory": {"sample_sha256_in_pool_order": inventory["sample_sha256_in_pool_order"],
                          "registry_sha256": inventory.get("registry_sha256"),
                          "eval_full_only": classes["full_only"], "eval_over_40": classes["over_40"],
                          "eval_drake_hatcher": classes["drake"]},
            "multiplicity": ({"candidates_evaluated": 1, "alpha": _fraction_text(ALPHA)} if len(runs) == 1 else
                             {"candidates_evaluated": 2, "better_of_two_alpha": _fraction_text(HOLM_ALPHA),
                              "second_of_two_alpha": _fraction_text(ALPHA),
                              "rule": "Holm step-down per opponent test: the higher held-out mean is tested "
                                      "first; the other only after it passed"}),
            "gate_e0": e0, "candidates": candidates}


def canonical_bytes(evaluation: dict) -> bytes:
    return (json.dumps(evaluation, sort_keys=True, indent=2, ensure_ascii=True, allow_nan=False) + "\n").encode()


def timing_sidecar(inputs: list[RunInput], paths: list[tuple[Path, Path]], seconds: float) -> dict:
    runs = []
    for run, (games, replay) in zip(inputs, paths):
        runs.append({"candidate": run.candidate, "opponent": run.opponent,
                     "games_path": str(games), "replay_path": str(replay),
                     "main": {k: run.summary.get(k) for k in sorted(SUMMARY_TIMING_KEYS)}
                     | {"game_ms_total": sum(r.get("ms", 0) for r in run.rows)},
                     "replay": {k: run.replay_summary.get(k) for k in sorted(SUMMARY_TIMING_KEYS)}
                     | {"game_ms_total": sum(r.get("ms", 0) for r in run.replay_rows)}})
    return {"schema": TIMING_SCHEMA, "evaluation_seconds": seconds, "runs": runs}


def _write(path: Path, data: bytes) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    staging = path.with_name(path.name + ".staging")
    staging.write_bytes(data)
    os.replace(staging, path)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = parser.add_subparsers(dest="command", required=True)
    run = sub.add_parser("evaluate")
    run.add_argument("--run", nargs=4, action="append", required=True,
                     metavar=("CANDIDATE", "OPPONENT", "GAMES_JSONL", "REPLAY_GAMES_JSONL"))
    run.add_argument("--out", type=Path, required=True, help="the canonical evaluation.json")
    run.add_argument("--timing-out", type=Path, help="default: evaluation.timing.json beside --out")
    run.add_argument("--inventory", type=Path, help="saved inventory JSON (default: computed from the repo)")
    args = parser.parse_args(argv)
    started = time.monotonic()
    try:
        inputs, paths = [], []
        for candidate, opponent, games, replay in args.run:
            games, replay = Path(games), Path(replay)
            inputs.append(RunInput(candidate, opponent, read_jsonl(games), read_summary(games),
                                   read_jsonl(replay), read_summary(replay)))
            paths.append((games, replay))
        evaluation = evaluate(inputs, load_inventory(args.inventory))
    except (EvaluationInputError, OSError, json.JSONDecodeError) as error:
        print(f"refused: {error}", file=sys.stderr)
        return 2
    timing_out = args.timing_out or args.out.with_name(args.out.stem + ".timing.json")
    _write(args.out, canonical_bytes(evaluation))
    sidecar = timing_sidecar(inputs, paths, time.monotonic() - started)
    _write(timing_out, (json.dumps(sidecar, sort_keys=True, indent=2) + "\n").encode())
    verdicts = {c: {"E0": evaluation["gate_e0"]["pass"]} | {g: v["pass"] for g, v in e["gates"].items()}
                for c, e in evaluation["candidates"].items()}
    print(json.dumps({"evaluation": str(args.out), "timing": str(timing_out), "gates": verdicts}, sort_keys=True))
    return 0


if __name__ == "__main__":
    sys.exit(main())
