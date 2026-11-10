"""SD1 analyzer (collab LANES/search-diagnostic-20261108/DESIGN.md v4 sections 3.2 to 3.4, 4 and 5; E6).

Measurement only. Reads `sd1-record/v1` records (tools/sd1/capture.py convert) of one position panel and applies the
design's fixed statistics, discovery calls, readout rows and stage triggers; `confirm` applies the fresh-panel Holm
confirmation to locked claims. Terminal win/loss of b512 SELF continuations only; never a rating or CP7.

Scoring estimand (section 3.2): every reference rollout scores 1 natural win, 1/2 natural draw or nonnatural ending,
0 natural loss. Each per-state statistic is linear in reference cells (Q_a(j), Q_b(j), V_p,a, V_p,b) once its
selection inputs (search targets and picks, q[Q_h] in the oracle, H*'s argmax) are fixed at the estimand. Its
worst cases score each nonnatural ending of a cell 0 or 1 by the sign of the cell's coefficient: gain-minimizing
(positive calls) scores 0 where the coefficient is positive, 1 where negative; gain-maximizing (exclusion calls) the
reverse. A contrast is its own linear form, so its worst case uses the combined coefficients.

Statistics per state (section 3.3), each the mean over the arm's search seeds:
- G^tgt_X = sum_{j in S} (q_X(j) - p(j)) Qbar(j), Qbar = (Q_a + Q_b)/2, q_X = ex1-target-v1 on the arm's root counts
  (value-leaf arms: Q-hat = value units / (n 2^20)); q = p on failure or no target (gain 0).
- G^pick_X = Qbar(frozen_choice) - Vbar_p; frozen_choice is recomputed from the recorded counts and seeded edge order
  (greatest wins/n, value-leaf: units/(n 2^20), among n >= 8, ties to the seeded order) and must equal the engine's.
  No supported edge: the plain fallback plays p's draw, whose value is V_p in expectation, so the gain is 0.
- G^pickv2_X: the same with pick-v2 (secondary).
- H* = 1/2 [Q_b(argmax Q_a) - V_p,b] + 1/2 [Q_a(argmax Q_b) - V_p,a] (argmax over covered actions, ties to the lower
  index).
- G^oracle = 1/2 sum_{j in S} (q[Q_a](j) - p(j)) Q_b(j) + (a, b exchanged), S the covered set, q[Q_h] the unchanged
  ex1-target-v1 applied to half h's counts (nonnatural endings as draws, the estimand).
Discovery (section 3.4): percentile bootstrap over states, 10,000 replicates, SplitMix64 seeded by
SHA256("sd1/boot")[:8] (little-endian) with Spellbench's unbiased index; one index draw per replicate shared by every
statistic and scoring. Index convention: the two-sided (1 - a) lower bound is sorted[floor(a/2 (R - 1))], the
one-sided (1 - a) upper bound sorted[R - 1 - floor(a (R - 1))]. Positive: two-sided 95% lower bound > 0 and point
>= delta_P under the estimand and under gain-minimizing scoring. Excluded: one-sided 95% upper bound < delta_P under
the estimand and under gain-maximizing scoring. Otherwise unresolved. delta_P = 0.008.

  sd1_analyze.py discover --records FILE --panel PANEL.json [--games G1.json G2.json] [--out RESULT.json]
  sd1_analyze.py confirm  --lock LOCK.json [--records FILE --panel PANEL.json] [--games ...] [--out RESULT.json]

Worst cases of H* and G^oracle account for selection: H*'s bounds are exact over every scoring of the nonnatural
endings (argmax and evaluation together); G^oracle's are a valid box bound (exact when no ending is nonnatural).
Games (section 4): S1's unit (opposing deck, seed family) within focal-deck strata, both seats and every arm
together, equal focal-deck weights, SHA256("sd1/games/boot")[:8], the same calls at delta_G = 0.05.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import math
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent / "ex1"))
sys.path.insert(0, str(HERE.parent / "w5"))
import target_ref  # noqa: E402
from joint_read import SplitMix64, unbiased_index  # noqa: E402

RESULT_SCHEMA = "sd1-result/v1"
RECORD_SCHEMA = "sd1-record/v1"
DELTA_P = 0.008
DELTA_G = 0.05
REPLICATES = 10_000
ALPHA = 0.05
BOOT_TAG = "sd1/boot"
GAMES_BOOT_TAG = "sd1/games/boot"
SUPPORT_MIN = 8
UNIT = 1 << 20
PRIMARY = ("D64", "P64", "T64", "D256")
CONTRASTS = (("T64", "P64"), ("P64", "D64"), ("D256", "D64"))
QUANTITIES = ("tgt", "pick")
SCORINGS = ("estimand", "gmin", "gmax")


class AnalyzeError(Exception):
    pass


def require(ok, message):
    if not ok:
        raise AnalyzeError(message)


def tag_seed(tag: str) -> int:
    return int.from_bytes(hashlib.sha256(tag.encode("utf-8")).digest()[:8], "little")


# ---------------------------------------------------------------- linear forms over reference cells

class Form:
    """const + sum_cell coef[cell] * Q[cell]; cells are ("q", half, action) or ("v", half, None)."""

    __slots__ = ("coef",)

    def __init__(self, coef=None):
        self.coef = dict(coef or {})

    def add(self, cell, c):
        if c != 0.0:
            self.coef[cell] = self.coef.get(cell, 0.0) + c
        return self

    def plus(self, other, scale=1.0):
        out = Form(self.coef)
        for cell, c in other.coef.items():
            out.add(cell, scale * c)
        return out

    def scaled(self, s):
        return Form({k: s * v for k, v in self.coef.items()})

    def value(self, cells, scoring):
        """cells[cell] = (wins, draws, nonnatural, n)."""
        total = 0.0
        for cell, c in self.coef.items():
            w, d, nn, n = cells[cell]
            if scoring == "estimand":
                s = 0.5
            elif scoring == "gmin":
                s = 0.0 if c > 0 else 1.0
            elif scoring == "gmax":
                s = 1.0 if c > 0 else 0.0
            else:
                raise AnalyzeError(f"unknown scoring {scoring}")
            total += c * (w + 0.5 * d + s * nn) / n
        return total


# ---------------------------------------------------------------- per-state reading

def policy(record):
    p, _, _ = target_ref.softmax([target_ref.f32(b) for b in record["policy"]["logit_bits"]])
    return p


def reference_cells(record):
    refs = record["references"]
    n = refs["half_rollouts"]
    require(n > 0, f"episode {record['episode']}: no references")
    actions = refs["actions"]
    cells = {}
    for half in ("a", "b"):
        q = refs["q"][half]
        require(len(q["wins"]) == len(q["draws"]) == len(q["nonnatural"]) == len(actions),
                f"episode {record['episode']}: reference widths differ")
        for i, j in enumerate(actions):
            w, d, nn = q["wins"][i], q["draws"][i], q["nonnatural"][i]
            require(min(w, d, nn) >= 0 and w + d + nn <= n, f"episode {record['episode']}: reference counts")
            cells[("q", half, j)] = (w, d, nn, n)
        v = refs["vp"][half]
        require(v["wins"] + v["draws"] + v["nonnatural"] <= n and sum(v["drawn"]) == n,
                f"episode {record['episode']}: V_p counts")
        cells[("v", half, None)] = (v["wins"], v["draws"], v["nonnatural"], n)
    return actions, cells


def estimand_q(cells, half, j):
    w, d, nn, n = cells[("q", half, j)]
    return (w + 0.5 * (d + nn)) / n


def edge_value(arm, a):
    if arm.get("value_units") is not None:
        return arm["value_units"][a] / (arm["n"][a] * UNIT)
    return arm["wins"][a] / arm["n"][a]


def frozen_choice(arm):
    """The engine's frozen execution rule on the recorded root: greatest edge value among n >= 8, ties to the
    seeded edge order (`perm_live`)."""
    best = None
    for a in arm["perm_live"]:
        if arm["n"][a] < SUPPORT_MIN:
            continue
        m = edge_value(arm, a)
        if best is None or m > best[1]:
            best = (a, m)
    return None if best is None else best[0]


def pick_v2(arm):
    n = arm["n"]
    best_n = max(n)
    if arm.get("value_units") is not None:
        score = lambda i: arm["value_units"][i]  # noqa: E731
    else:
        score = lambda i: 2 * arm["wins"][i] + arm["draws"][i]  # noqa: E731
    best = None
    for i in range(len(n)):
        if n[i] == best_n and (best is None or score(i) > score(best)):
            best = i
    return best


def arm_target(arm, bits):
    if arm["status"] != "ok":
        return None
    if arm.get("value_units") is not None:
        sys.path.insert(0, str(HERE.parent / "w5"))
        import value_leaf_2a  # noqa: E402
        t = value_leaf_2a.vl_target(bits, arm["n"], arm["value_units"])
    else:
        t = target_ref.target(bits, arm["n"], arm["wins"], arm["draws"])
    return None if isinstance(t, tuple) else t


def pick_form(a):
    return Form().add(("q", "a", a), 0.5).add(("q", "b", a), 0.5).add(("v", "a", None), -0.5).add(
        ("v", "b", None), -0.5)


def state_forms(record):
    """Per arm label: Forms for tgt, pick, pickv2 (seed means) plus support telemetry; and the budget-free H* and
    G^oracle Forms."""
    require(record.get("schema") == RECORD_SCHEMA and record.get("reached") is True, "not a reached SD1 record")
    bits = record["policy"]["logit_bits"]
    p = policy(record)
    width = len(p)
    require(width == record["width"], f"episode {record['episode']}: width")
    actions, cells = reference_cells(record)
    covered = set(actions)
    by_arm = {}
    for arm in record["arms"]:
        by_arm.setdefault(arm["arm"], []).append(arm)
    forms = {}
    tele = {}
    for label, rows in by_arm.items():
        k = len(rows)
        tgt, pick, pickv2 = Form(), Form(), Form()
        t_rows = []
        for arm in rows:
            row = {"status": arm["status"], "target": False, "support": 0, "m_s": 0.0, "frozen": None,
                   "pick_v2": None, "nonnatural": arm["search"].get("nonnatural", 0),
                   "completed": arm["search"].get("completed", 0), "attempted": arm["search"].get("attempted", 0),
                   "cpu_s": arm.get("cpu_s", 0.0)}
            fb = arm.get("fallback")
            if arm["status"] != "ok" or arm.get("frozen_choice") is None:
                # The executor's plain fallback plays p's draw; the capture records it with its own seed.
                require(isinstance(fb, int) and fb in covered,
                        f"episode {record['episode']} {label}: plain fallback draw missing or uncovered")
            if arm["status"] != "ok":
                pick = pick.plus(pick_form(fb), 1.0 / k)
                pickv2 = pickv2.plus(pick_form(fb), 1.0 / k)
            else:
                require(len(arm["n"]) == width, f"episode {record['episode']} {label}: root width")
                support = [a for a in range(width) if arm["n"][a] >= SUPPORT_MIN]
                row["support"] = len(support)
                row["m_s"] = sum(p[a] for a in support)
                t = arm_target(arm, bits)
                if t is not None:
                    row["target"] = True
                    for j in range(width):
                        if t["q"][j] != p[j]:
                            require(j in covered, f"episode {record['episode']} {label}: q moves uncovered {j}")
                            c = (t["q"][j] - p[j]) / k
                            tgt.add(("q", "a", j), 0.5 * c).add(("q", "b", j), 0.5 * c)
                f = frozen_choice(arm)
                require(f == arm.get("frozen_choice"), f"episode {record['episode']} {label}: frozen_choice "
                        f"recomputed {f} differs from the engine's {arm.get('frozen_choice')}")
                row["frozen"] = f
                played = f if f is not None else fb
                require(played in covered, f"episode {record['episode']} {label}: frozen pick uncovered")
                pick = pick.plus(pick_form(played), 1.0 / k)
                v2 = pick_v2(arm)
                require(v2 == arm.get("pick_v2"), f"episode {record['episode']} {label}: pick-v2 differs")
                row["pick_v2"] = v2
                require(v2 in covered, f"episode {record['episode']} {label}: pick-v2 uncovered")
                pickv2 = pickv2.plus(pick_form(v2), 1.0 / k)
            t_rows.append(row)
        forms[label] = {"tgt": tgt, "pick": pick, "pickv2": pickv2}
        tele[label] = t_rows
    # H*: argmax over covered actions on one half, evaluated on the other.
    hstar = Form()
    for sel, ev in (("a", "b"), ("b", "a")):
        best = max(actions, key=lambda j: (estimand_q(cells, sel, j), -j))
        hstar.add(("q", ev, best), 0.5).add(("v", ev, None), -0.5)
    # G^oracle: the unchanged operator on one half's counts, evaluated on the other.
    oracle = Form()
    for sel, ev in (("a", "b"), ("b", "a")):
        n = [0] * width
        w = [0] * width
        d = [0] * width
        for j in actions:
            cw, cd, cnn, cn = cells[("q", sel, j)]
            n[j], w[j], d[j] = cn, cw, cd + cnn
        t = target_ref.target(bits, n, w, d)
        if not isinstance(t, tuple):
            for j in actions:
                oracle.add(("q", ev, j), 0.5 * (t["q"][j] - p[j]))
    bounds = {"hstar": hstar_bounds(cells, actions), "oracle": oracle_bounds(bits, p, cells, actions)}
    return {"cells": cells, "arms": forms, "telemetry": tele, "hstar": hstar, "oracle": oracle, "bounds": bounds,
            "covered": len(actions), "full_coverage": record["references"]["full_coverage"]}


# ---------------------------------------------------------------- selection-sensitive worst cases (H*, oracle)

def cell_range(cells, half, j):
    """A Q cell's attainable values as integer score numerators over 2n: 2w + d + 2k for k = 0..nn."""
    w, d, nn, n = cells[("q", half, j)]
    return 2 * w + d, 2 * w + d + 2 * nn, 2 * n


def beats(v, j, others):
    """v at action j is the argmax (ties to the lower index) against others: [(action, value)]."""
    return all(v > u or (v == u and j < k) for k, u in others)


def argmax_extreme(cells, half, j1, j2, actions, want):
    """Extreme ('min' or 'max') of Q_half(j2) over the nonnatural scorings in which j1 is the argmax of Q_half;
    None when no scoring makes j1 the argmax. Every other action sits at its lowest value, which only helps j1."""
    rng = {j: cell_range(cells, half, j) for j in actions}
    den = rng[j1][2]
    if j1 == j2:
        others = [(k, rng[k][0]) for k in actions if k != j1]
        feasible = [v for v in range(rng[j1][0], rng[j1][1] + 1, 2) if beats(v, j1, others)]
        if not feasible:
            return None
        return (min(feasible) if want == "min" else max(feasible)) / den
    top = rng[j1][1]
    others = [(k, rng[k][0]) for k in actions if k not in (j1, j2)]
    if not beats(top, j1, others):
        return None
    grid = [v for v in range(rng[j2][0], rng[j2][1] + 1, 2) if beats(top, j1, [(j2, v)])]
    if not grid:
        return None
    return (min(grid) if want == "min" else max(grid)) / den


def v_extreme(cells, half, want):
    w, d, nn, n = cells[("v", half, None)]
    return (w + 0.5 * d + (nn if want == "max" else 0)) / n


def hstar_bounds(cells, actions):
    """Exact gain-minimizing and gain-maximizing H* over every scoring of the nonnatural endings, selections
    included: j1 = argmax Q_a is evaluated on Q_b and j2 = argmax Q_b on Q_a; the halves' scorings are independent,
    so each (j1, j2) pair is optimized half by half."""
    out = {}
    for direction, want, vwant in (("gmin", "min", "max"), ("gmax", "max", "min")):
        best = None
        for j1 in actions:
            for j2 in actions:
                qa = argmax_extreme(cells, "a", j1, j2, actions, want)
                qb = argmax_extreme(cells, "b", j2, j1, actions, want)
                if qa is None or qb is None:
                    continue
                v = 0.5 * (qb - v_extreme(cells, "b", vwant)) + 0.5 * (qa - v_extreme(cells, "a", vwant))
                if best is None or (v < best if want == "min" else v > best):
                    best = v
        out[direction] = best
    return out


def oracle_bounds(bits, p, cells, actions):
    """A valid (conservative) box bound of G^oracle over every scoring of the nonnatural endings: each q[Q_h](j) is
    bounded over the box of Q_h (own value low and the others high for its minimum, the reverse for its maximum),
    each evaluation cell over its range, and each product term takes its extreme corner. Exact when no nonnatural
    ending occurred."""
    width = len(p)

    def q_at(half, j, low_self):
        n, w, d = [0] * width, [0] * width, [0] * width
        for k in actions:
            cw, cd, cnn, cn = cells[("q", half, k)]
            high = (k != j) if low_self else (k == j)
            n[k], w[k], d[k] = cn, cw + (cnn if high else 0), cd
        t = target_ref.target(bits, n, w, d)
        return p[j] if isinstance(t, tuple) else t["q"][j]

    lo = hi = 0.0
    for sel, ev in (("a", "b"), ("b", "a")):
        for j in actions:
            dq = (q_at(sel, j, True) - p[j], q_at(sel, j, False) - p[j])
            a, b, den = cell_range(cells, ev, j)
            corners = [x * y for x in dq for y in (a / den, b / den)]
            lo += 0.5 * min(corners)
            hi += 0.5 * max(corners)
    return {"gmin": lo, "gmax": hi}


def statistic_forms(state):
    """Every named statistic of one state (absolute, contrasts, operator contrasts, budget-free)."""
    out = {}
    arms = state["arms"]
    for label, f in arms.items():
        out[f"{label}.tgt"] = f["tgt"]
        out[f"{label}.pick"] = f["pick"]
        out[f"{label}.pickv2"] = f["pickv2"]
        out[f"{label}.pick-tgt"] = f["pick"].plus(f["tgt"], -1.0)
    for x, y in CONTRASTS:
        if x in arms and y in arms:
            for qn in QUANTITIES:
                out[f"{x}-{y}.{qn}"] = arms[x][qn].plus(arms[y][qn], -1.0)
    if "VLS-h0" in arms and "VL8-h0" in arms:
        for qn in QUANTITIES:
            vls = arms["VLS-h0"][qn].plus(arms["VLS-t0"][qn]).scaled(0.5)
            vl8 = arms["VL8-h0"][qn].plus(arms["VL8-t0"][qn]).scaled(0.5)
            out[f"VLS.{qn}"] = vls
            out[f"VLS-VL8.{qn}"] = vls.plus(vl8, -1.0)
    for probe in ("TH64", "TL64"):
        if probe in arms and "P64" in arms:
            for qn in QUANTITIES:
                out[f"{probe}-P64.{qn}"] = arms[probe][qn].plus(arms["P64"][qn], -1.0)
    out["hstar"] = state["hstar"]
    out["oracle"] = state["oracle"]
    return out


# ---------------------------------------------------------------- bootstrap and calls

def lower_index(alpha, reps):
    """Two-sided (1 - alpha) lower bound index."""
    return math.floor(alpha / 2 * (reps - 1))


def upper_index(alpha, reps):
    """One-sided (1 - alpha) upper bound index."""
    return reps - 1 - math.floor(alpha * (reps - 1))


def bootstrap(values, seed, replicates):
    """values[name][scoring] = per-state list. One index draw per replicate, shared by every name and scoring;
    returns sorted replicate means per name and scoring."""
    names = list(values)
    n = len(values[names[0]][SCORINGS[0]])
    rng = SplitMix64(seed)
    reps = {k: {s: [] for s in values[k]} for k in names}
    for _ in range(replicates):
        idx = [unbiased_index(n, rng.next_u64) for _ in range(n)]
        for k in names:
            for s, xs in values[k].items():
                total = 0.0
                for i in idx:
                    total += xs[i]
                reps[k][s].append(total / n)
    return {k: {s: sorted(v) for s, v in d.items()} for k, d in reps.items()}


def p_positive(sorted_reps, point, delta):
    """Smallest alpha at which the two-sided lower bound is > 0 and the point is >= delta (1 when the point
    fails): the lower bound sorted[floor(alpha/2 (R-1))] is > 0 iff that index is >= the count of replicates <= 0."""
    if point < delta:
        return 1.0
    reps = len(sorted_reps)
    c = sum(1 for x in sorted_reps if x <= 0.0)
    return min(1.0, 2.0 * c / (reps - 1))


def p_excluded(sorted_reps, delta):
    """Smallest alpha at which the one-sided upper bound sorted[R-1-floor(alpha (R-1))] is < delta."""
    reps = len(sorted_reps)
    e = sum(1 for x in sorted_reps if x >= delta)
    return min(1.0, e / (reps - 1))


def call(entry, delta, alpha=ALPHA):
    """Discovery call of one statistic from its points and sorted replicates per scoring."""
    pts, reps = entry["point"], entry["reps"]
    pos = all(p_positive(reps[s], pts[s], delta) <= alpha for s in ("estimand", "gmin"))
    exc = all(p_excluded(reps[s], delta) <= alpha for s in ("estimand", "gmax"))
    return "positive" if pos else ("excluded" if exc else "unresolved")


def entry(point, reps, delta):
    """One statistic's reported bounds, p-values and discovery call from its points and sorted replicates per
    scoring (all three scorings: call-bearing; the estimand alone: descriptive)."""
    count = len(next(iter(reps.values())))
    lo, hi = lower_index(ALPHA, count), upper_index(ALPHA, count)
    out = {"point": point, "delta": delta,
           "lower95_two_sided": {s: reps[s][lo] for s in reps},
           "upper95_one_sided": {s: reps[s][hi] for s in reps}}
    if set(reps) == set(SCORINGS):
        out["p_positive"] = {s: p_positive(reps[s], point[s], delta) for s in ("estimand", "gmin")}
        out["p_excluded"] = {s: p_excluded(reps[s], delta) for s in ("estimand", "gmax")}
        out["call"] = call({"point": point, "reps": reps}, delta)
    else:
        out["call"] = "descriptive"
    return out


def summarize(values, seed, replicates, delta):
    reps = bootstrap(values, seed, replicates)
    return {k: entry({s: sum(xs) / len(xs) for s, xs in d.items()}, reps[k], delta) for k, d in values.items()}


# ---------------------------------------------------------------- readout rows (section 5) and triggers (section 8)

def readout(stats):
    def c(name):
        return stats[name]["call"] if name in stats else "not run"

    def pos(name):
        return c(name) == "positive"

    def exc(name):
        return c(name) == "excluded"

    rows = {}
    rows["Headroom"] = {"holds": pos("hstar"), "components": {"hstar positive": c("hstar")}}
    align = {qn: pos(f"T64.{qn}") and pos(f"T64-P64.{qn}") for qn in QUANTITIES}
    part_p = any(align.values())
    rows["Alignment"] = {"holds_part_p": part_p, "by_quantity": align,
                         "locked_quantity": ("pick" if align["pick"] else ("tgt" if align["tgt"] else None))}
    if "G1:T-E2-plain" in stats:
        games_ok = pos("G1:T-E2-plain") and pos("G1:T-E2-D-E2")
        rows["Alignment"].update(holds=part_p and games_ok, games={"T vs plain": c("G1:T-E2-plain"),
                                                                    "T - D": c("G1:T-E2-D-E2")})
    else:
        rows["Alignment"]["games"] = "G1 not run: the row is evaluated on its Part P conditions only"
    cons = {qn: pos(f"P64.{qn}") and pos(f"P64-D64.{qn}") for qn in QUANTITIES}
    rows["Consistency"] = {"holds": any(cons.values()), "by_quantity": cons}
    budget = {qn: pos(f"D256.{qn}") and pos(f"D256-D64.{qn}") for qn in QUANTITIES}
    rows["Budget"] = {"holds": any(budget.values()), "by_quantity": budget}
    if "VLS.pick" in stats:
        value = {qn: pos(f"VLS.{qn}") and pos(f"VLS-VL8.{qn}") for qn in QUANTITIES}
        rows["Value"] = {"holds": any(value.values()), "by_quantity": value}
    else:
        rows["Value"] = {"holds": False, "unresolved": "VLS not run"}
    branch_a = [x for x in PRIMARY if pos(f"{x}.pick-tgt") and pos(f"{x}.pick")]
    branch_b = exc("oracle")
    op = pos("hstar") and (bool(branch_a) or branch_b)
    arm_x = max(branch_a, key=lambda x: stats[f"{x}.pick-tgt"]["point"]["estimand"]) if branch_a else None
    rows["Operator"] = {"holds": op, "branch_a_arms": branch_a, "branch_b_oracle_excluded": branch_b,
                        "locked_branch": ("a" if branch_a else ("b" if branch_b else None)) if op else None,
                        "locked_arm": arm_x if op and branch_a else None}
    if "G2:TRUEOPP-plain" in stats:
        rows["Opponent"] = {"holds": pos("G2:TRUEOPP-plain") and pos("G2:TRUEOPP-SELF"),
                            "components": {"TRUEOPP vs plain": c("G2:TRUEOPP-plain"),
                                           "TRUEOPP - SELF": c("G2:TRUEOPP-SELF")}}
    else:
        rows["Opponent"] = {"holds": False, "unresolved": "G2 not run"}
    shape = pos("hstar") and all(exc(f"{x}.{qn}") for x in PRIMARY for qn in QUANTITIES)
    rows["Search shape"] = {"holds": shape}
    rows["Unresolved"] = {"holds": not any(r.get("holds") or r.get("holds_part_p") for r in rows.values())}
    triggers = {
        "2a D1M (Budget row holds)": rows["Budget"]["holds"],
        "2a T256 and 2c TH64/TL64 (Alignment row's Part P conditions hold)": rows["Alignment"]["holds_part_p"],
        "3a G1 (D, P or T at 64k or 256k has a positive G^pick)":
            any(pos(f"{x}.pick") for x in ("D64", "P64", "T64", "D256")),
        "3a P-E2 in G1 (P64 G^pick positive)": pos("P64.pick"),
        "3b G2 (some search arm positive vs plain in G1)":
            any(pos(f"G1:{x}-plain") for x in ("D-E2", "T-E2", "P-E2")),
    }
    return rows, triggers


# ---------------------------------------------------------------- confirmation (Holm over locked rows)

def holm(rows, stats):
    """rows: [{"row": name, "components": [{"stat": name, "kind": "positive"|"excluded"}]}]; stats: every component's
    entry (positions at delta_P, games at delta_G). Each row's p-value is the largest component p-value
    (intersection-union, each component under its own worst case); Holm step-down at familywise ALPHA over the m
    rows, positions and games in one family."""
    scored = []
    for r in rows:
        ps = []
        for comp in r["components"]:
            require(comp["stat"] in stats, f"locked component {comp['stat']} was not measured")
            s = stats[comp["stat"]]
            if comp["kind"] == "positive":
                ps.append(max(s["p_positive"].values()))
            elif comp["kind"] == "excluded":
                ps.append(max(s["p_excluded"].values()))
            else:
                raise AnalyzeError(f"unknown component kind {comp['kind']}")
        scored.append({"row": r["row"], "components": r["components"], "p": max(ps), "component_p": ps})
    m = len(scored)
    order = sorted(range(m), key=lambda i: (scored[i]["p"], scored[i]["row"]))
    open_ = True
    for rank, i in enumerate(order):
        level = ALPHA / (m - rank)
        scored[i]["holm_level"] = level
        scored[i]["confirmed"] = open_ and scored[i]["p"] <= level
        if not scored[i]["confirmed"]:
            open_ = False
    return scored


# ---------------------------------------------------------------- games (section 4): S1's resampling unit

def game_score(case, sign, scoring):
    """A game's score under the estimand (nonnatural 1/2) or a worst case for a term of the given sign."""
    if not case.get("nonnatural"):
        return case["score"]
    if scoring == "estimand":
        return 0.5
    low = (scoring == "gmin") == (sign > 0)
    return 0.0 if low else 1.0


def game_bootstrap(cases, contrasts, seed, replicates, scoring="estimand"):
    """cases: [{"focal_deck", "opposing_deck", "seed_family", "seat", "arm", "score", "nonnatural"}]. S1's unit is
    (opposing deck, seed family) within a focal deck, carrying both seats and every arm; units are resampled within
    focal-deck strata (as many draws as the stratum has units) and focal decks weigh equally. contrasts: [(x, y)]
    (y None: x alone). Returns per contrast the point and the sorted replicates, and the units per focal deck."""
    strata = {}
    for c in cases:
        unit = (c["opposing_deck"], c["seed_family"])
        strata.setdefault(c["focal_deck"], {}).setdefault(unit, {}).setdefault(c["arm"], []).append(c)
    decks = sorted(strata)
    units = {f: sorted(strata[f]) for f in decks}

    def unit_value(f, u, x, y):
        cell = strata[f][u]
        vx = sum(game_score(c, 1, scoring) for c in cell[x]) / len(cell[x])
        if y is None:
            return vx
        return vx - sum(game_score(c, -1, scoring) for c in cell[y]) / len(cell[y])

    table = {(x, y): {f: [unit_value(f, u, x, y) for u in units[f]] for f in decks} for x, y in contrasts}

    def estimate(idx_by_deck, key):
        return sum(sum(table[key][f][i] for i in idx_by_deck[f]) / len(idx_by_deck[f]) for f in decks) / len(decks)

    full = {f: list(range(len(units[f]))) for f in decks}
    points = {key: estimate(full, key) for key in table}
    rng = SplitMix64(seed)
    reps = {key: [] for key in table}
    for _ in range(replicates):
        idx = {f: [unbiased_index(len(units[f]), rng.next_u64) for _ in units[f]] for f in decks}
        for key in table:
            reps[key].append(estimate(idx, key))
    return {key: {"point": points[key], "reps": sorted(reps[key])} for key in table}, units


def game_calls(cases, contrasts, seed=None, replicates=REPLICATES, delta=DELTA_G):
    """Discovery calls on games (section 4): the section 3.4 rules at delta_G, with the estimand and its worst
    cases; one index stream per scoring from the same seed, so every scoring sees the same resampled units."""
    seed = tag_seed(GAMES_BOOT_TAG) if seed is None else seed
    by_scoring = {sc: game_bootstrap(cases, contrasts, seed, replicates, sc)[0] for sc in SCORINGS}
    out = {}
    for key in by_scoring["estimand"]:
        entry = {"point": {sc: by_scoring[sc][key]["point"] for sc in SCORINGS},
                 "reps": {sc: by_scoring[sc][key]["reps"] for sc in SCORINGS}}
        out[key] = {"point": entry["point"], "call": call(entry, delta)}
    return out


GAME_CONTRASTS = {"G1": (("D-E2", "plain"), ("T-E2", "plain"), ("P-E2", "plain"), ("T-E2", "D-E2")),
                  "G2": (("SELF", "plain"), ("TRUEOPP", "plain"), ("TRUEOPP", "SELF"))}


def game_stats(blocks, seed=None, replicates=REPLICATES):
    """blocks: {"G1": cases, "G2": cases}. Per block, every declared contrast whose arms ran, named
    "<block>:<x>-<y>", with the estimand and both worst cases over the same resampled units, at delta_G."""
    seed = tag_seed(GAMES_BOOT_TAG) if seed is None else seed
    out = {}
    for block, cases in blocks.items():
        arms = {c["arm"] for c in cases}
        contrasts = [(x, y) for x, y in GAME_CONTRASTS[block] if x in arms and y in arms]
        if not contrasts:
            continue
        by_scoring = {sc: game_bootstrap(cases, contrasts, seed, replicates, sc)[0] for sc in SCORINGS}
        for key in contrasts:
            point = {sc: by_scoring[sc][key]["point"] for sc in SCORINGS}
            reps = {sc: by_scoring[sc][key]["reps"] for sc in SCORINGS}
            out[f"{block}:{key[0]}-{key[1]}"] = entry(point, reps, DELTA_G)
    return out


def load_games(paths):
    blocks = {}
    for path in paths or []:
        doc = json.loads(Path(path).read_text(encoding="utf-8"))
        require(doc.get("schema") == "sd1-game-cases/v1" and doc.get("block") in GAME_CONTRASTS,
                f"{path}: not an SD1 game-cases file")
        require(doc["block"] not in blocks, f"{doc['block']} given twice")
        blocks[doc["block"]] = doc["cases"]
    return blocks


# ---------------------------------------------------------------- driver

def load_records(path, panel_doc):
    recs = [json.loads(line) for line in Path(path).read_text(encoding="utf-8").splitlines() if line.strip()]
    recs.sort(key=lambda r: r["episode"])
    ids = [[r["episode"], r["game"], r["step"], r["policy"]["logit_bits"]] for r in recs]
    require(ids == panel_doc["states"], "records differ from the panel (episodes, games, steps or policies)")
    body = json.dumps(panel_doc["states"], separators=(",", ":"))
    require(hashlib.sha256(body.encode()).hexdigest() == panel_doc["panel_sha256"], "panel hash mismatch")
    return recs


def call_bearing(name):
    """Statistics a section 3.4 claim reads (every scoring); the rest are descriptive (estimand only)."""
    base, _, quantity = name.partition(".")
    if name in ("hstar", "oracle"):
        return True
    if base in PRIMARY and quantity in ("tgt", "pick", "pick-tgt"):
        return True
    if base in {f"{x}-{y}" for x, y in CONTRASTS} and quantity in QUANTITIES:
        return True
    return base in ("VLS", "VLS-VL8", "TH64-P64", "TL64-P64") or (
        base in ("D1M", "T256") and quantity in QUANTITIES)


def per_state_values(states, names=None):
    """values[name][scoring] = per-state list; every state must carry every name. Call-bearing statistics get all
    three scorings, descriptive ones the estimand only."""
    forms = [statistic_forms(s) for s in states]
    names = names or sorted(set.intersection(*[set(f) for f in forms]))
    values = {}
    for name in names:
        scorings = SCORINGS if call_bearing(name) else ("estimand",)
        values[name] = {}
        for sc in scorings:
            if name in ("hstar", "oracle") and sc != "estimand":
                values[name][sc] = [s["bounds"][name][sc] for s in states]
            else:
                values[name][sc] = [f[name].value(s["cells"], sc) for f, s in zip(forms, states)]
    return values


def telemetry(states):
    out = {}
    labels = sorted({k for s in states for k in s["telemetry"]})
    for label in labels:
        rows = [r for s in states for r in s["telemetry"].get(label, [])]
        ok = [r for r in rows if r["status"] == "ok"]
        out[label] = {"searches": len(rows), "failed": len(rows) - len(ok),
                      "targets": sum(r["target"] for r in rows),
                      "mean_supported_actions": sum(r["support"] for r in rows) / len(rows),
                      "mean_supported_prior_mass": sum(r["m_s"] for r in rows) / len(rows),
                      "frozen_pick_share": sum(r["frozen"] is not None for r in rows) / len(rows),
                      "search_nonnatural": sum(r["nonnatural"] for r in rows),
                      "search_completed": sum(r["completed"] for r in rows),
                      "search_attempted": sum(r["attempted"] for r in rows),
                      "mean_cpu_s": sum(r["cpu_s"] for r in rows) / len(rows)}
    nn = sum(c[2] for s in states for c in s["cells"].values())
    total = sum(c[3] for s in states for c in s["cells"].values())
    out["references"] = {"mean_covered_actions": sum(s["covered"] for s in states) / len(states),
                         "full_coverage_share": sum(bool(s["full_coverage"]) for s in states) / len(states),
                         "nonnatural": nn, "rollouts": total}
    return out


def discover(records, panel_doc, replicates=REPLICATES, games=None):
    """Discovery on one panel. Statistics on every state use the full panel; arms measured only on the declared
    subset (D1M, T256) are analyzed on the subset with the same bootstrap stream. Game blocks join when given."""
    states = [state_forms(r) for r in records]
    values = per_state_values(states)
    stats = summarize(values, tag_seed(BOOT_TAG), replicates, DELTA_P)
    subset = panel_doc.get("subset")
    descriptive = {}
    if subset:
        sub_forms = [statistic_forms(s) for s in states[:subset]]
        extra = sorted(set.intersection(*[set(f) for f in sub_forms]) - set(values))
        if extra:
            sub_stats = summarize(per_state_values(states[:subset], extra), tag_seed(BOOT_TAG), replicates, DELTA_P)
            for k, v in sub_stats.items():
                v["panel"] = f"declared subset (first {subset} states)"
                stats[k] = v
        sub = per_state_values(states[:subset], sorted(values))
        descriptive["subset_points"] = {k: sum(v["estimand"]) / subset for k, v in sub.items()}
    if games:
        stats.update(game_stats(games, replicates=replicates))
    rows, triggers = readout(stats)
    return {"schema": RESULT_SCHEMA, "stage": "discovery (exploratory)", "panel_sha256": panel_doc["panel_sha256"],
            "states": len(states), "delta_p": DELTA_P, "replicates": replicates, "boot_tag": BOOT_TAG,
            "boot_seed": tag_seed(BOOT_TAG), "statistics": stats, "rows": rows, "triggers_as_evaluated": triggers,
            "telemetry": telemetry(states), "descriptive": descriptive,
            "law": "terminal win/loss of b512 SELF continuations only; no rating, panel score or CP7 read"}


def confirm(records, panel_doc, lock, replicates=REPLICATES, games=None):
    """Fresh-panel and fresh-case confirmation of the locked rows (section 3.4): position components on the
    confirmation panel at delta_P, game components on the fresh cases at delta_G, one Holm family."""
    names = sorted({c["stat"] for r in lock["rows"] for c in r["components"]})
    position = [n for n in names if ":" not in n]
    stats = {}
    if position:
        require(records is not None, "position components need the confirmation panel's records")
        states = [state_forms(r) for r in records]
        stats.update(summarize(per_state_values(states, position),
                               tag_seed(lock.get("boot_tag", "sd1/confirm/boot")), replicates, DELTA_P))
    if len(position) < len(names):
        require(bool(games), "game components need the fresh confirmation cases")
        stats.update(game_stats(games, tag_seed(lock.get("games_boot_tag", GAMES_BOOT_TAG)), replicates))
    scored = holm(lock["rows"], stats)
    return {"schema": RESULT_SCHEMA, "stage": "confirmation",
            "panel_sha256": panel_doc["panel_sha256"] if panel_doc else None,
            "rows": scored, "statistics": {k: stats[k] for k in names if k in stats}}


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = ap.add_subparsers(dest="cmd", required=True)
    for name in ("discover", "confirm"):
        p = sub.add_parser(name)
        p.add_argument("--records", required=name == "discover")
        p.add_argument("--panel", required=name == "discover")
        p.add_argument("--games", nargs="*", help="sd1-game-cases/v1 files (tools sd1_games.py collect)")
        p.add_argument("--out")
        if name == "confirm":
            p.add_argument("--lock", required=True)
    a = ap.parse_args(argv)
    try:
        panel_doc = json.loads(Path(a.panel).read_text(encoding="utf-8")) if a.panel else None
        records = load_records(a.records, panel_doc) if a.records else None
        games = load_games(a.games)
        if a.cmd == "discover":
            result = discover(records, panel_doc, games=games)
        else:
            result = confirm(records, panel_doc, json.loads(Path(a.lock).read_text(encoding="utf-8")), games=games)
    except AnalyzeError as e:
        print(f"error: {e}", file=sys.stderr)
        return 1
    text = json.dumps(result, indent=1, sort_keys=True) + "\n"
    digest = hashlib.sha256(text.encode()).hexdigest()
    if a.out:
        Path(a.out).write_text(text, encoding="utf-8", newline="\n")
    print(json.dumps({"result_sha256": digest, "rows": {k: v.get("holds", v.get("holds_part_p"))
                                                         for k, v in result.get("rows", {}).items()}
                      if a.cmd == "discover" else [r["confirmed"] for r in result["rows"]]}))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
