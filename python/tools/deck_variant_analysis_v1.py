"""Analysis for brewing prototype B0 (v1), collab LANES/brewing-b0-20261020/DESIGN.md (v6) section 5.

Rows come from deck_variant_panel_v1 (one per case and deck). A case is
(pilot, stage, opponent, block, seat, starting player); deck 0 of a stage is the
incumbent. Paired deltas are candidate win minus incumbent win on the same case.

- Common mask: a case with a scientific failure for any deck of a pilot is masked
  for every deck of that pilot; joint-pilot statistics use the union across pilots.
- Field delta: the 1/9-weighted mean over opponents of the per-opponent mean delta.
- Bootstrap: cases resampled with replacement inside opponent x seat x starting
  player strata, jointly across pilots when they share a case list; percentile
  bounds at index floor(B * alpha) as in W5's paired_bootstrap_ci_with_alpha_v1.
- n4 rule: padded discordance over every scheduled case (masked counted
  discordant) plus Hoeffding padding at level 0.05/cells, max over cells,
  clamped to [0.25, 1]; n4 = 29,070 x v rounded up to a multiple of 36.
"""
from __future__ import annotations

import argparse
import itertools
import json
import math
from pathlib import Path

import numpy as np

OPPONENTS = 9
N4_COEFFICIENT = 29070
BLOCK = 36


def load_rows(paths) -> list[dict]:
    rows = []
    for path in paths:
        rows.extend(json.loads(line) for line in Path(path).read_text(encoding="utf-8").splitlines() if line)
    return rows


class Stage:
    """Win matrix per pilot: cases x decks with NaN where a case is masked."""

    def __init__(self, rows: list[dict], decks: list[int] | None = None):
        self.decks = sorted({row["deck"] for row in rows}) if decks is None else list(decks)
        self.pilots = sorted({row["pilot"] for row in rows})
        by_pilot = {pilot: {} for pilot in self.pilots}
        for row in rows:
            if row["deck"] not in self.decks:
                continue
            key = (row.get("stage", ""), row["opponent"], row["block"], row["seat"], row["starting_player"])
            by_pilot[row["pilot"]].setdefault(key, {})[row["deck"]] = row["win"] if row["complete"] else None
        self.keys = {}
        self.wins = {}
        self.scheduled = {}
        for pilot, cases in by_pilot.items():
            keys = sorted(cases)
            matrix = np.full((len(keys), len(self.decks)), np.nan)
            for i, key in enumerate(keys):
                row = cases[key]
                if all(row.get(deck) is not None for deck in self.decks):
                    matrix[i] = [row[deck] for deck in self.decks]
            self.keys[pilot], self.wins[pilot] = keys, matrix
            self.scheduled[pilot] = len(keys)

    def column(self, deck: int) -> int:
        return self.decks.index(deck)

    def masked_share(self, pilot: str) -> float:
        return float(np.isnan(self.wins[pilot][:, 0]).mean())

    def masked_by_opponent(self, pilot: str) -> dict[str, float]:
        masked, opponents = np.isnan(self.wins[pilot][:, 0]), self.opponents(pilot)
        return {str(o): float(masked[opponents == o].mean()) for o in range(OPPONENTS) if (opponents == o).any()}

    def validity(self) -> dict:
        """Section 5 check 3: invalid above 0.5% masked per pilot or 2% in any opponent stratum."""
        shares = {p: self.masked_share(p) for p in self.pilots}
        by_opponent = {p: self.masked_by_opponent(p) for p in self.pilots}
        valid = all(v <= 0.005 for v in shares.values()) and all(
            v <= 0.02 for rows in by_opponent.values() for v in rows.values())
        return {"valid": valid, "masked_share": shares, "masked_by_opponent": by_opponent}

    def deltas(self, pilot: str, deck: int, incumbent: int, worst_case: bool = False) -> np.ndarray:
        wins = self.wins[pilot]
        delta = wins[:, self.column(deck)] - wins[:, self.column(incumbent)]
        if worst_case:
            delta = np.where(np.isnan(delta), -1.0, delta)
        return delta

    def opponents(self, pilot: str) -> np.ndarray:
        return np.array([key[1] for key in self.keys[pilot]])

    def strata(self, pilot: str) -> np.ndarray:
        return np.array([key[1] * 4 + key[3] * 2 + key[4] for key in self.keys[pilot]])


def field_mean(delta: np.ndarray, opponents: np.ndarray) -> float:
    means = []
    for opponent in range(OPPONENTS):
        values = delta[(opponents == opponent) & ~np.isnan(delta)]
        if len(values):
            means.append(values.mean())
    return float(np.mean(means)) if means else float("nan")


def bootstrap_field(series: list[np.ndarray], opponents: np.ndarray, strata: np.ndarray, resamples: int,
                    seed: int, batch: int = 500) -> np.ndarray:
    """Resampled field deltas, shape (resamples, len(series)); every series shares one case index
    draw per resample (joint resampling)."""
    rng = np.random.Generator(np.random.PCG64(seed))
    stratum_ids = np.unique(strata)  # ascending: opponent * 4 + seat * 2 + starting player
    groups = [np.flatnonzero(strata == s) for s in stratum_ids]
    # Columns of one draw are concatenated in stratum order, so each opponent is one contiguous slice.
    bounds, position = {}, 0
    for s, g in zip(stratum_ids, groups):
        o = int(opponents[g[0]])
        lo, hi = bounds.get(o, (position, position))
        bounds[o] = (lo, position + len(g))
        position += len(g)
    out = np.empty((resamples, len(series)))
    stack = np.vstack(series)  # (series, cases)
    present = ~np.isnan(stack)
    values = np.where(present, stack, 0.0)
    for start in range(0, resamples, batch):
        b = min(batch, resamples - start)
        picks = np.concatenate([g[rng.integers(0, len(g), size=(b, len(g)))] for g in groups], axis=1)
        means = np.full((b, len(series), OPPONENTS), np.nan)
        for j in range(len(series)):
            v, p = values[j][picks], present[j][picks]
            for o, (lo, hi) in bounds.items():
                counts = p[:, lo:hi].sum(axis=1)
                with np.errstate(invalid="ignore", divide="ignore"):
                    means[:, j, o] = np.where(counts > 0, v[:, lo:hi].sum(axis=1) / counts, np.nan)
        out[start:start + b] = np.nanmean(means, axis=2)
    return out


def lower_bound(samples: np.ndarray, alpha: float) -> float:
    ordered = np.sort(samples)
    return float(ordered[min(int(math.floor(len(ordered) * alpha)), len(ordered) - 1)])


def upper_bound(samples: np.ndarray, alpha: float) -> float:
    ordered = np.sort(samples)
    return float(ordered[min(int(math.floor(len(ordered) * (1 - alpha))), len(ordered) - 1)])


def rank(estimates: dict[int, float], order: list[int]) -> list[int]:
    """Descending estimate; ties go to the lower global index."""
    return sorted(order, key=lambda deck: (-estimates[deck], deck))


def screen_estimates(stage: Stage, incumbent: int) -> dict[int, float]:
    """Pooled over pilots: each pilot's cases contribute to the per-opponent means."""
    out = {}
    for deck in stage.decks:
        if deck == incumbent:
            continue
        deltas = np.concatenate([stage.deltas(p, deck, incumbent) for p in stage.pilots])
        opponents = np.concatenate([stage.opponents(p) for p in stage.pilots])
        out[deck] = field_mean(deltas, opponents)
    return out


def hoeffding_v(stage: Stage, decks: list[int], incumbent: int, pilots: list[str], scheduled: int) -> dict:
    cells = [(deck, pilot) for deck in decks for pilot in pilots]
    padding = math.sqrt(math.log(len(cells) / 0.05) / (2 * scheduled))
    bounds = {}
    for deck, pilot in cells:
        wins = stage.wins[pilot]
        delta = wins[:, stage.column(deck)] - wins[:, stage.column(incumbent)]
        discordant = int(np.sum(np.isnan(delta) | (delta != 0))) + (scheduled - len(delta))
        bounds[f"{deck}/{pilot}"] = min(1.0, discordant / scheduled + padding)
    v = min(1.0, max(0.25, max(bounds.values())))
    n4 = int(math.ceil(N4_COEFFICIENT * v / BLOCK) * BLOCK)
    return {"cells": len(cells), "padding": padding, "bounds": bounds, "v": v, "planned_n4": n4}


def affordable_n4(cap_cpu_seconds: float, spent: float, reserve: float, cpu_per_case: float) -> int:
    return int(math.floor(max(0.0, cap_cpu_seconds - spent - reserve) / cpu_per_case / BLOCK) * BLOCK)


def kendall_tau_b(x: list[float], y: list[float]) -> float:
    concordant = discordant = ties_x = ties_y = 0
    for i, j in itertools.combinations(range(len(x)), 2):
        dx, dy = np.sign(x[i] - x[j]), np.sign(y[i] - y[j])
        if dx == 0 and dy == 0:
            continue
        if dx == 0:
            ties_x += 1
        elif dy == 0:
            ties_y += 1
        elif dx == dy:
            concordant += 1
        else:
            discordant += 1
    denominator = math.sqrt((concordant + discordant + ties_x) * (concordant + discordant + ties_y))
    return (concordant - discordant) / denominator if denominator else float("nan")


def joint_samples(stage: Stage, decks: list[int], incumbent: int, pilots: list[str], resamples: int, seed: int,
                  worst_case: bool = False) -> tuple[dict, dict]:
    """Point estimates and joint bootstrap samples for (deck, pilot) on the common case list."""
    reference = pilots[0]
    for pilot in pilots[1:]:
        assert stage.keys[pilot] == stage.keys[reference], "joint statistics need one shared case list"
    union = np.zeros(len(stage.keys[reference]), dtype=bool)
    for pilot in pilots:
        union |= np.isnan(stage.wins[pilot][:, 0])
    series, labels = [], []
    for deck in decks:
        for pilot in pilots:
            delta = stage.deltas(pilot, deck, incumbent, worst_case=worst_case)
            if not worst_case:
                delta = np.where(union, np.nan, delta)
            series.append(delta)
            labels.append((deck, pilot))
    opponents, strata = stage.opponents(reference), stage.strata(reference)
    points = {label: field_mean(s, opponents) for label, s in zip(labels, series)}
    samples = bootstrap_field(series, opponents, strata, resamples, seed)
    return points, {label: samples[:, i] for i, label in enumerate(labels)}


def m4_labels(stage: Stage, finalists: list[int], burnv2: int | None, incumbent: int, resamples: int,
              seed: int, principal=("g115", "a48"), third="H") -> dict:
    pilots = list(principal) + [third]
    decks = finalists + ([burnv2] if burnv2 is not None else [])
    points, samples = joint_samples(stage, decks, incumbent, pilots, resamples, seed)
    worst_points, worst = joint_samples(stage, decks, incumbent, pilots, resamples, seed, worst_case=True)
    out = {}
    for deck in decks:
        alpha = 0.05 / 3 if deck in finalists else 0.05
        per = {}
        for pilot in pilots:
            s = samples[(deck, pilot)]
            per[pilot] = {"estimate": points[(deck, pilot)], "lower_one_sided": lower_bound(s, alpha),
                          "ci95": [lower_bound(s, 0.025), upper_bound(s, 0.025)],
                          "worst_case_lower": lower_bound(worst[(deck, pilot)], alpha)}
            per[pilot]["confirmed"] = per[pilot]["lower_one_sided"] > 0 and per[pilot]["worst_case_lower"] > 0
            per[pilot]["harm"] = per[pilot]["ci95"][1] < 0
        confirmed = [p for p in principal if per[p]["confirmed"]]
        if len(confirmed) == 2:
            label = "principal-pilot gain with H harm" if per[third]["harm"] else "gain under both principal pilots"
            material = all(per[p]["estimate"] >= 0.02 for p in principal)
        elif len(confirmed) == 1:
            other = [p for p in principal if p not in confirmed][0]
            difference = samples[(deck, confirmed[0])] - samples[(deck, other)]
            specific = lower_bound(difference, alpha) > 0
            label = f"one-pilot gain ({confirmed[0]}): " + ("pilot-specific" if specific else "unresolved")
            material = None
        else:
            label, material = "no demonstrated gain", None
        out[str(deck)] = {"family_alpha": alpha, "pilots": per, "label": label, "material": material,
                          "harm_flags": [p for p in pilots if per[p]["harm"]]}
    return out


def rank_association(stage: Stage, decks: list[int], incumbent: int, pilots: list[str], resamples: int,
                     seed: int) -> dict:
    points, samples = joint_samples(stage, decks, incumbent, pilots, resamples, seed)
    out = {}
    for a, b in itertools.combinations(pilots, 2):
        xa, xb = [points[(d, a)] for d in decks], [points[(d, b)] for d in decks]
        taus = [kendall_tau_b([samples[(d, a)][r] for d in decks], [samples[(d, b)][r] for d in decks])
                for r in range(min(resamples, 2000))]
        ra, rb = rank(dict(zip(decks, xa)), decks), rank(dict(zip(decks, xb)), decks)
        out[f"{a}~{b}"] = {"tau_b": kendall_tau_b(xa, xb), "ci95": [lower_bound(np.array(taus), 0.025),
                                                                    upper_bound(np.array(taus), 0.025)],
                           "top3_overlap": len(set(ra[:3]) & set(rb[:3])), "top_agrees": ra[0] == rb[0],
                           "ranks": {str(d): [ra.index(d) + 1, rb.index(d) + 1] for d in decks}}
    return out


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("op", choices=("screen", "m3", "m4"))
    parser.add_argument("--rows", nargs="+", type=Path, required=True)
    parser.add_argument("--incumbent", type=int, default=0)
    parser.add_argument("--decks", default="")
    parser.add_argument("--finalists", default="")
    parser.add_argument("--cells", default="")
    parser.add_argument("--burnv2", type=int)
    parser.add_argument("--resamples", type=int, default=10000)
    parser.add_argument("--seed", type=int, required=True)
    parser.add_argument("--scheduled", type=int)
    parser.add_argument("--out", type=Path, required=True)
    a = parser.parse_args(argv)
    rows = load_rows(a.rows)
    stage = Stage(rows)
    if a.op == "screen":
        estimates = screen_estimates(stage, a.incumbent)
        result = {"estimates": {str(k): v for k, v in estimates.items()},
                  "ranking": rank(estimates, sorted(estimates)),
                  "validity": stage.validity()}
    elif a.op == "m3":
        decks = [int(x) for x in a.decks.split(",") if x]
        result = {"rank_association": rank_association(stage, decks, a.incumbent, stage.pilots, a.resamples, a.seed),
                  "validity": stage.validity()}
        points, _ = joint_samples(stage, decks, a.incumbent, stage.pilots, 10, a.seed)
        result["estimates"] = {f"{d}/{p}": v for (d, p), v in points.items()}
        cells = [int(x) for x in a.cells.split(",") if x]
        result["n4_rule"] = hoeffding_v(stage, cells, a.incumbent, stage.pilots, a.scheduled)
    else:
        finalists = [int(x) for x in a.finalists.split(",") if x]
        result = {"labels": m4_labels(stage, finalists, a.burnv2, a.incumbent, a.resamples, a.seed),
                  "validity": stage.validity()}
    with a.out.open("x", encoding="utf-8", newline="\n") as stream:
        stream.write(json.dumps(result, indent=1, sort_keys=True) + "\n")
    print(json.dumps({"op": a.op, "out": str(a.out)}))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
