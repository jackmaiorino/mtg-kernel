"""W5 primary read (collab LANES/lead/W5-DESIGN.md v1.3 section 7.2; E13): the joint-refit bootstrap reader.

A bounded extension of W2's precision-gate reader (collab LANES/w2-ladder-20261031/tools/contrasts.py, branch
claude/w2-ladder-20261031 at 944cb06c): the same snapshot reading, seat-swapped pair units, anchored Bradley-Terry
refit and virtual draws, with section 7.2's estimand computed inside every joint refit.

- **Estimand:** the mean over chain pairs i of Elo(arm A chain i) minus Elo(arm B chain i), on the rated snapshot
  that holds every entrant of the read.
- **Interval:** 2,000 replicates; each resamples every matchup's seat-swapped pairs (rung against rung included),
  refits all entrants and rungs jointly and evaluates the estimand in that refit, so shared-rung covariance is
  kept. The interval is the 1.25% and 98.75% quantiles: sorted indices (n - 1) // 80 and (79 (n - 1) + 79) // 80
  of the n successful replicates (Spellbench's 95% convention at 1/80). Like Spellbench, the read fails closed when
  more than half of the replicate fits fail.
- **Seed:** the first 8 bytes (little-endian, as EX1 and W5 placement read them) of SHA256("w5/read/<P1|P2>") seed Spellbench's SplitMix64 stream, drawn
  in matchup order then pair slot with Spellbench's unbiased index.
- **Labels** (in order, the first that holds): A stronger (lower bound > 0 and at least k pair differences
  positive), B stronger (upper bound < 0 and at least k negative), no worthwhile difference (interval inside
  (-50, +50)), not resolved. k is 5 of 6, 3 of 4, 3 of 3. Whether the lower bound clears +50 is reported beside.
- **Secondaries:** a two-sided 97.5% t-interval over the chain-pair point differences; the sensitivity read without
  under-budget pairs (same replicates); per appended rung, a refit and bootstrap without that rung's matchups.

The fit is a verbatim port of spellbench.arena.ratings._fit_score_pairs (Spellbench beb4a08b), so the reader runs
without Spellbench for the statistics; reading a run or snapshot needs Spellbench on PYTHONPATH. Every read checks
that the point fit reproduces the published leaderboard ratings.

  PYTHONPATH=<spellbench>/python python joint_read.py --directory SNAPSHOT --spec READ.json
       --precision-gate CONTRASTS.json [--out OUT.json] [--verify-published]

An official read refuses a snapshot without a passing W2 precision-gate result for that directory, or an entrant
that is not rated there. The replicate count is fixed at 2,000 (tests inject smaller counts through `read`).

READ.json (`w5-read-spec/v1`): {"read": "P1", "arm_a": "X8", "arm_b": "C",
  "pairs": [{"pair": 1, "a": "<entrant>", "b": "<entrant>", "under_budget": false}, ...],
  "appended_rungs": ["<rung>", ...]}
"""
from __future__ import annotations

import argparse
import hashlib
import json
import math
import sys
from dataclasses import dataclass
from pathlib import Path

ELO = 400 / math.log(10)
REPLICATES = 2000
WORTHWHILE = 50.0
K_OF = {6: 5, 4: 3, 3: 3}
VIRTUAL_DRAWS = 1  # spellbench.arena.leaderboard.VIRTUAL_DRAWS_PER_MATCHUP
SPEC_SCHEMA = "w5-read-spec/v1"
RESULT_SCHEMA = "w5-read-result/v1"

# ---------------------------------------------------------------- Spellbench's fit and stream (ported verbatim)

MAX_ITERATIONS = 200_000
LOG_CONVERGENCE_EPSILON = 1e-10
_MASK64 = 0xFFFF_FFFF_FFFF_FFFF
_UINT64_CARDINALITY = 1 << 64
_GOLDEN_RATIO_64 = 0x9E37_79B9_7F4A_7C15


class FitError(ValueError):
    pass


class ReadError(Exception):
    pass


@dataclass(frozen=True)
class ScorePair:
    a_id: str
    b_id: str
    a_score: float
    b_score: float


class SplitMix64:
    def __init__(self, seed: int) -> None:
        if not 0 <= seed <= _MASK64:
            raise ValueError("seed must be a uint64")
        self._state = seed

    def next_u64(self) -> int:
        self._state = (self._state + _GOLDEN_RATIO_64) & _MASK64
        value = self._state
        value = ((value ^ (value >> 30)) * 0xBF58_476D_1CE4_E5B9) & _MASK64
        value = ((value ^ (value >> 27)) * 0x94D0_49BB_1331_11EB) & _MASK64
        return (value ^ (value >> 31)) & _MASK64


def unbiased_index(population_size: int, next_u64) -> int:
    limit = (_UINT64_CARDINALITY // population_size) * population_size
    while True:
        value = next_u64()
        if value < limit:
            return value % population_size


def _connected(ids, pairs) -> bool:
    adjacency = {model_id: set() for model_id in ids}
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


def fit_score_pairs(pairs, reference_id: str) -> dict[str, float]:
    """Anchored Bradley-Terry by MM iteration; log-unit ratings by id (the anchor is 0)."""
    if not pairs:
        raise FitError("missing pairs")
    ids = sorted({pair.a_id for pair in pairs} | {pair.b_id for pair in pairs})
    if reference_id not in ids:
        raise FitError("reference_id has no games")
    if not _connected(ids, pairs):
        raise FitError("comparison graph is disconnected")
    score = {model_id: 0.0 for model_id in ids}
    counter = {model_id: 0.0 for model_id in ids}
    for pair in pairs:
        score[pair.a_id] += pair.a_score
        counter[pair.a_id] += pair.b_score
        score[pair.b_id] += pair.b_score
        counter[pair.b_id] += pair.a_score
    for model_id in ids:
        if score[model_id] == 0.0 or counter[model_id] == 0.0:
            raise FitError(f"degenerate record for {model_id}")
    strengths = {model_id: 1.0 for model_id in ids}
    for _ in range(1, MAX_ITERATIONS + 1):
        updated = {}
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
        delta = max(abs(math.log(updated[model_id]) - math.log(strengths[model_id])) for model_id in ids)
        strengths = updated
        if delta < LOG_CONVERGENCE_EPSILON:
            break
    else:
        raise FitError("MM iteration did not converge")
    return {model_id: math.log(strengths[model_id]) for model_id in ids}


# ---------------------------------------------------------------- statistics

def read_seed(read: str) -> int:
    return int.from_bytes(hashlib.sha256(f"w5/read/{read}".encode("utf-8")).digest()[:8], "little")


def quantile_indices(n: int) -> tuple[int, int]:
    return (n - 1) // 80, (79 * (n - 1) + 79) // 80


@dataclass(frozen=True)
class Matchup:
    """One rated matchup: ids sorted, each complete pair's half-points (0 to 4) for a_id."""
    a_id: str
    b_id: str
    values: tuple[int, ...]


def point_fit(matchups, anchor: str) -> dict[str, float]:
    pairs = [ScorePair(m.a_id, m.b_id, float(sum(m.values)) + VIRTUAL_DRAWS,
                       4.0 * len(m.values) - sum(m.values) + VIRTUAL_DRAWS) for m in matchups if m.values]
    return fit_score_pairs(pairs, anchor)


def bootstrap(matchups, anchor: str, seed: int, replicates: int, estimands: dict) -> dict:
    """Joint resample-and-refit; every estimand (a function of one replicate's ratings) is evaluated per refit."""
    rng = SplitMix64(seed)
    samples = {name: [] for name in estimands}
    failed = 0
    for _ in range(replicates):
        pairs = []
        for m in matchups:
            n = len(m.values)
            a_score = 0.0
            for _ in range(n):
                a_score += m.values[unbiased_index(n, rng.next_u64)]
            pairs.append(ScorePair(m.a_id, m.b_id, a_score + VIRTUAL_DRAWS, 4.0 * n - a_score + VIRTUAL_DRAWS))
        try:
            fit = fit_score_pairs(pairs, anchor)
        except FitError:
            failed += 1
            continue
        for name, fn in estimands.items():
            samples[name].append(fn(fit))
    if failed * 2 > replicates:
        raise ReadError(f"bootstrap degenerate: {failed}/{replicates} replicate fits failed")
    return {"samples": samples, "failed": failed}


def interval(values) -> tuple[float, float]:
    ordered = sorted(values)
    lo, hi = quantile_indices(len(ordered))
    return ordered[lo], ordered[hi]


def label(lo: float, hi: float, diffs, arm_a: str, arm_b: str) -> str:
    k = K_OF.get(len(diffs))
    if k is None:
        raise ReadError(f"no pair-consistency count for {len(diffs)} pairs")
    if lo > 0 and sum(d > 0 for d in diffs) >= k:
        return f"{arm_a} stronger"
    if hi < 0 and sum(d < 0 for d in diffs) >= k:
        return f"{arm_b} stronger"
    if -WORTHWHILE < lo and hi < WORTHWHILE:
        return "no worthwhile difference"
    return "not resolved"


def _betacf(a: float, b: float, x: float) -> float:
    tiny, qab, qap, qam = 1e-300, a + b, a + 1.0, a - 1.0
    c, d = 1.0, 1.0 - qab * x / qap
    d = 1.0 / (d if abs(d) > tiny else tiny)
    h = d
    for m in range(1, 300):
        m2 = 2 * m
        aa = m * (b - m) * x / ((qam + m2) * (a + m2))
        d = 1.0 + aa * d
        d = 1.0 / (d if abs(d) > tiny else tiny)
        c = 1.0 + aa / c if abs(1.0 + aa / c) > tiny else tiny
        h *= d * c
        aa = -(a + m) * (qab + m) * x / ((a + m2) * (qap + m2))
        d = 1.0 + aa * d
        d = 1.0 / (d if abs(d) > tiny else tiny)
        c = 1.0 + aa / c if abs(1.0 + aa / c) > tiny else tiny
        delta = d * c
        h *= delta
        if abs(delta - 1.0) < 1e-15:
            break
    return h


def t_cdf(t: float, df: int) -> float:
    x = df / (df + t * t)
    a, b = df / 2.0, 0.5
    front = math.exp(math.lgamma(a + b) - math.lgamma(a) - math.lgamma(b) + a * math.log(x) + b * math.log1p(-x))
    ibeta = front * _betacf(a, b, x) / a if x < (a + 1) / (a + b + 2) else 1.0 - front * _betacf(b, a, 1 - x) / b
    return 1.0 - 0.5 * ibeta if t >= 0 else 0.5 * ibeta


def t_quantile(p: float, df: int) -> float:
    lo, hi = 0.0, 1000.0
    for _ in range(200):
        mid = (lo + hi) / 2
        if t_cdf(mid, df) < p:
            lo = mid
        else:
            hi = mid
    return (lo + hi) / 2


def t_interval(diffs) -> dict | None:
    n = len(diffs)
    if n < 2:
        return None
    mean = sum(diffs) / n
    sd = math.sqrt(sum((d - mean) ** 2 for d in diffs) / (n - 1))
    q = t_quantile(1 - 0.025 / 2, n - 1)
    return {"mean": mean, "sd": sd, "df": n - 1, "t": q, "ci975": [mean - q * sd / math.sqrt(n),
                                                                     mean + q * sd / math.sqrt(n)]}


def check_spec(spec: dict) -> None:
    if spec.get("schema", SPEC_SCHEMA) != SPEC_SCHEMA:
        raise ReadError("wrong read spec schema")
    if spec["read"] not in ("P1", "P2"):
        raise ReadError("read must be P1 or P2")
    numbers = [p["pair"] for p in spec["pairs"]]
    if len(set(numbers)) != len(numbers):
        raise ReadError("duplicate chain pair")
    entrants = [p[s] for p in spec["pairs"] for s in ("a", "b")]
    if len(set(entrants)) != len(entrants):
        raise ReadError("an entrant appears twice in the read")
    if set(spec.get("appended_rungs", [])) & set(entrants):
        raise ReadError("an appended rung is also an entrant of the read")


def read(matchups, anchor: str, ids: dict[str, str], spec: dict, replicates: int = REPLICATES) -> dict:
    """The whole read on one rated snapshot's matchups. ids maps entrant names to bot ids."""
    check_spec(spec)
    for p in spec["pairs"]:
        for side in ("a", "b"):
            if p[side] not in ids:
                raise ReadError(f"entrant {p[side]} is not on the snapshot")
    seed = read_seed(spec["read"])
    point = point_fit(matchups, anchor)
    pairs = spec["pairs"]
    kept = [p for p in pairs if not p.get("under_budget")]

    def estimand(subset):
        return lambda fit: sum(fit[ids[p["a"]]] - fit[ids[p["b"]]] for p in subset) / len(subset) * ELO

    estimands = {"primary": estimand(pairs)}
    if len(kept) not in (0, len(pairs)):
        estimands["without_under_budget"] = estimand(kept)
    boot = bootstrap(matchups, anchor, seed, replicates, estimands)
    diffs = [(point[ids[p["a"]]] - point[ids[p["b"]]]) * ELO for p in pairs]
    lo, hi = interval(boot["samples"]["primary"])
    result = {
        "schema": RESULT_SCHEMA, "read": spec["read"], "arm_a": spec["arm_a"], "arm_b": spec["arm_b"],
        "seed": seed, "replicates": replicates, "failed_replicates": boot["failed"],
        "successful_replicates": replicates - boot["failed"], "quantile_indices": quantile_indices(
            replicates - boot["failed"]),
        "estimate": sum(diffs) / len(diffs), "interval_975": [lo, hi],
        "pairs": [{"pair": p["pair"], "a": p["a"], "b": p["b"], "under_budget": bool(p.get("under_budget")),
                   "difference": d} for p, d in zip(pairs, diffs)],
        "positive_pairs": sum(d > 0 for d in diffs), "negative_pairs": sum(d < 0 for d in diffs),
        "label": label(lo, hi, diffs, spec["arm_a"], spec["arm_b"]),
        "lower_bound_clears_worthwhile": lo > WORTHWHILE,
        "secondary_t_interval": t_interval(diffs),
    }
    if "without_under_budget" in boot["samples"]:
        klo, khi = interval(boot["samples"]["without_under_budget"])
        kdiffs = [d for p, d in zip(pairs, diffs) if not p.get("under_budget")]
        result["sensitivity_without_under_budget"] = {
            "pairs": len(kept), "estimate": sum(kdiffs) / len(kdiffs), "interval_975": [klo, khi],
            "label": label(klo, khi, kdiffs, spec["arm_a"], spec["arm_b"]) if len(kdiffs) in K_OF else None}
    per_rung = []
    for rung in spec.get("appended_rungs", []):
        if rung not in ids:
            raise ReadError(f"appended rung {rung} is not on the snapshot")
        if ids[rung] == anchor:
            raise ReadError("the anchor cannot be refit away")
        rid = ids[rung]
        sub = [m for m in matchups if rid not in (m.a_id, m.b_id)]
        sub_point = point_fit(sub, anchor)
        sub_boot = bootstrap(sub, anchor, seed, replicates, {"primary": estimand(pairs)})
        slo, shi = interval(sub_boot["samples"]["primary"])
        sdiffs = [(sub_point[ids[p["a"]]] - sub_point[ids[p["b"]]]) * ELO for p in pairs]
        per_rung.append({"without": rung, "estimate": sum(sdiffs) / len(sdiffs), "interval_975": [slo, shi],
                         "failed_replicates": sub_boot["failed"]})
    result["per_rung_sensitivity"] = per_rung
    return result


# ---------------------------------------------------------------- reading a Spellbench run or snapshot

def load_directory(directory: Path, verify_published: bool = False) -> tuple[list[Matchup], str, dict, dict]:
    """Matchups, anchor id, name-to-id map and a provenance record; needs Spellbench on PYTHONPATH."""
    from dataclasses import replace

    from spellbench.arena import leaderboard, snapshot, store
    from spellbench.arena.config import TournamentConfig
    from spellbench.arena.ledger import parse_ledger

    manifest = store.read_json(directory / store.MANIFEST_NAME)
    if manifest.get("schema") != snapshot.SCHEMA:
        rows = parse_ledger(store.read_jsonl(directory / store.LEDGER_NAME))
    else:  # W2's ledger_rows: the snapshot's selected blocks, renumbered as snapshot.materialize does
        chosen = {}
        for receipt in manifest["sources"]:
            source = snapshot.read_source(snapshot.source_directory(directory, receipt["run"]))
            if source.manifest_sha256 != receipt["manifest_sha256"]:
                raise ReadError(f"source {receipt['run']} manifest digest changed")
            for pair in receipt["matchups"]:
                chosen[tuple(pair)] = source
        rows, next_pair = [], 0
        for pair in sorted(chosen):
            block = chosen[pair].blocks()[pair]
            local = {index: next_pair + i for i, index in enumerate(sorted({row.pair_index for row in block}))}
            next_pair += len(local)
            rows.extend(replace(row, pair_index=local[row.pair_index]) for row in block)
    config = TournamentConfig.from_json(store.read_json(directory / store.CONFIG_NAME))
    board = store.read_json(directory / store.LEADERBOARD_JSON_NAME)
    ids = {row["name"]: row["bot_id"] for row in board["rows"]}
    games: dict = {}
    for row in rows:  # W2's pair_totals
        if row.classification not in ("natural", "forfeit"):
            continue
        names = {seat.seat: seat.name for seat in row.seats}
        a, b = sorted(names.values())
        if a == b:
            continue
        half = 1 if row.winner is None else (2 if names[row.winner] == a else 0)
        games.setdefault((a, b), {}).setdefault(row.pair_index, {})[row.pair_slot] = half
    matchups = []
    for (a, b), by_pair in games.items():
        pairs = [(s[0], s[1]) for _, s in sorted(by_pair.items()) if set(s) == {0, 1}]
        if not pairs:
            continue
        a_id, b_id = sorted((ids[a], ids[b]))
        values = tuple(g0 + g1 if a_id == ids[a] else 4 - (g0 + g1) for g0, g1 in pairs)
        matchups.append(Matchup(a_id, b_id, values))
    matchups.sort(key=lambda m: (m.a_id, m.b_id))  # the leaderboard's matchup order
    anchor = ids[config.rating_anchor]
    point = point_fit(matchups, anchor)
    for row in board["rows"]:
        if row["rated"] and int(round((point[row["bot_id"]] * ELO + 1000) * 1000)) != row["elo_milli"]:
            raise ReadError(f"point fit does not reproduce {row['name']}'s published rating")
    rated_rows = {row["name"]: bool(row["rated"]) for row in board["rows"]}
    provenance = {"directory": str(directory), "rated": rated_rows, "manifest_sha256": hashlib.sha256(
        (directory / store.MANIFEST_NAME).read_bytes()).hexdigest(), "anchor": config.rating_anchor,
        "matchups": len(matchups), "pairs": sum(len(m.values) for m in matchups),
        "point_fit_reproduces_leaderboard": True}
    if verify_published:
        seed = leaderboard.rating_bootstrap_seed(config.stats_seed)
        name_of = {v: k for k, v in ids.items()}
        rated = [r for r in board["rows"] if r["rated"] and r["bot_id"] != anchor]
        boot = bootstrap(matchups, anchor, seed, config.bootstrap_replicates,
                         {r["bot_id"]: (lambda bid: lambda fit: fit[bid])(r["bot_id"]) for r in rated})
        ok = len(boot["samples"][rated[0]["bot_id"]]) if rated else 0
        lo_i, hi_i = (ok - 1) // 40, (39 * (ok - 1) + 39) // 40
        for r in rated:
            ordered = sorted(boot["samples"][r["bot_id"]])
            lo, hi = ordered[lo_i] * ELO + 1000, ordered[hi_i] * ELO + 1000
            if abs(lo - r["ci95_elo_milli"][0] / 1000) > 0.01 or abs(hi - r["ci95_elo_milli"][1] / 1000) > 0.01:
                raise ReadError(f"{name_of[r['bot_id']]} published interval does not reproduce")
        provenance["published_intervals_reproduced"] = len(rated)
    return matchups, anchor, ids, provenance


def admit_snapshot(directory: Path, gate_path: Path, spec: dict, provenance: dict) -> dict:
    """An official read needs a snapshot that passed W2's precision gate, with every entrant of the read rated."""
    gate = json.loads(Path(gate_path).read_text(encoding="utf-8"))
    if Path(gate["source"]).resolve() != Path(directory).resolve():
        raise ReadError("the precision-gate result is for another directory")
    if gate.get("gate") not in ("pass", "pass-8", "pass-12"):
        raise ReadError(f"the snapshot did not pass W2's precision gate ({gate.get('gate')})")
    for p in spec["pairs"]:
        for side in ("a", "b"):
            if not provenance["rated"].get(p[side]):
                raise ReadError(f"entrant {p[side]} is not rated on the snapshot")
    return {"path": str(gate_path), "sha256": hashlib.sha256(Path(gate_path).read_bytes()).hexdigest(),
            "gate": gate["gate"], "widest_adjacent_half_width": gate.get("widest_adjacent_half_width")}


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--directory", type=Path, required=True)
    parser.add_argument("--spec", type=Path, required=True)
    parser.add_argument("--out", type=Path, default=None)
    parser.add_argument("--verify-published", action="store_true")
    parser.add_argument("--precision-gate", type=Path, required=True,
                        help="W2 contrasts.py's JSON result for this snapshot (gate pass, pass-8 or pass-12)")
    args = parser.parse_args(argv)
    try:
        spec = json.loads(args.spec.read_text(encoding="utf-8"))
        matchups, anchor, ids, provenance = load_directory(args.directory, args.verify_published)
        provenance["precision_gate"] = admit_snapshot(args.directory, args.precision_gate, spec, provenance)
        result = read(matchups, anchor, ids, spec)
    except (ReadError, FitError) as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 1
    result["provenance"] = provenance
    result["spec_sha256"] = hashlib.sha256(args.spec.read_bytes()).hexdigest()
    text = json.dumps(result, indent=2) + "\n"
    if args.out:
        args.out.write_text(text, encoding="utf-8", newline="\n")
    print(text, end="")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
