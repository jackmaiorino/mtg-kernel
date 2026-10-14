"""Frozen analysis of the Spy archive distillation screen's raw-policy panels (DESIGN.md sections 6-7).

Input: an index {"panels": [{"checkpoint": ID, "opponent": "T1"|"H", "checkpoint_sha256": SHA,
"rows": PATH, "labels": PATH}]} with IDs {lineage}-{parent|O|D1U|D2U|D1L|D2L} and the archive
reset pilot's descriptive endpoints {lineage}-P (its plain-guide reset arm) and {lineage}-S.
Rows are nine_deck_public_panel_v1 rows (game one score per case); labels are
``regret_census_v1 mode=s4a-label-replay`` rows for the Spy-own cases. Both are checked as
tools/spy_reset/analyze.py checks them: panel id, checkpoint hash, case seed, the exact case
roster, label identity, and label consistency with the score. ``--fit`` takes fit.py's
FIT.json, for D1's held-out learner-offer probability.

Metrics per panel, on the 0-to-1 scale (win 1, draw 0.5, loss 0):
  J           the Spy-to-Giant sequence resolved, then a win, per scheduled Spy game;
  spy_score   Spy's score;
  eight_deck  the equal-weight mean of the eight non-Spy deck scores;
and, descriptively, combo_resolved, nine_deck, seven_deck, cawgates, and the self-target
and Dread-Return-to-Giant choice rates per game that offered them.

Contrasts per lineage: D1U-parent, D1L-parent, D2U-O and D2L-O (the screen), D2U-D1U and
D2L-D1L (the direct comparison between the two checkpoints), and, descriptively, D2U and D2L
against P and S and O against the parent (the drift ordinary training produces from the
parent), per opponent and pooled (the mean of the two opponents' values on the same draw). Cluster bootstrap: the seat pairs of a repeat are resampled
together within each ordered deck pair, one draw shared by every opponent, checkpoint and
lineage, 10,000 draws, seed 2026101320; bounds are the 2.5% and 97.5% quantiles. Missing
cases (technical terminations) take their bound's value within each draw: least favorable
to D (D's missing cases 0, the control's 1) and most favorable (the reverse).

Screening rule and readings: DESIGN.md section 7 as amended before execution (Jack,
2026-10-13), evaluated under each bound. A positive J result is behavioral transfer,
reported as established at D1, at D2 or at both; a behavioral loss or acquisition between
D1 and D2 is claimed only from their direct contrast in both lineages, and is reported with
the direct combo_resolved and Spy-score intervals. Win-rate effects (Spy, eight-deck and
nine-deck scores) are reported separately from J, per lineage and across lineages, since
more winning Spy sequences can mean that existing wins took a different route. A positive
reading must hold under the bound least favorable to D, harm is declared only under the
bound most favorable to D, and when the two bounds give different readings the reading is
"inconclusive: missing cases decide".

CLI:
  analyze  --index INDEX.json [--fit FIT.json] --out RESULT.json
  validate --index PILOT_INDEX.json --result PILOT_RESULT.json
``validate`` recomputes the archive reset pilot's per-opponent O-minus-parent intervals and
points (evidence/RESULT.json) with its seed 2026101017 through this analyzer's code, and
exits nonzero unless every one is equal.
"""
from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from distill_common import LINEAGES, pilot, read_json  # noqa: E402

DECKS, REPEATS = pilot.DECKS, pilot.PANEL_REPEATS
SPY, CAW = pilot.SPY, pilot.CAWGATES
EIGHT = [d for d in range(DECKS) if d != SPY]
SEVEN = [d for d in range(DECKS) if d not in (SPY, CAW)]
OPPONENTS = ("T1", "H")
ROLES = ("parent", "O", "D1U", "D2U", "D1L", "D2L", "P", "S")
BOOT_SEED, VALIDATION_SEED, DRAWS = 2026101320, 2026101017, 10_000
RETENTION, OFFER_FLOOR = 0.02, 0.05
SCREEN = (("D1U-parent", "D1U", "parent"), ("D1L-parent", "D1L", "parent"),
          ("D2U-O", "D2U", "O"), ("D2L-O", "D2L", "O"))
DIRECT = (("D2U-D1U", "D2U", "D1U"), ("D2L-D1L", "D2L", "D1L"))
DESCRIPTIVE = (("D2U-P", "D2U", "P"), ("D2U-S", "D2U", "S"), ("D2L-P", "D2L", "P"), ("D2L-S", "D2L", "S"),
               ("O-parent", "O", "parent"))
# Shown with each direct J reading: whether the line resolved at all, and the
# Spy score, since J also needs a win.
DIRECT_CONTEXT = ("J", "combo_resolved", "spy_score")
METRICS = ("J", "spy_score", "eight_deck")
EXTRA_METRICS = ("combo_resolved", "nine_deck", "seven_deck", "cawgates")
SPY_METRICS = ("J", "combo_resolved")
# Playing strength, reported apart from J (amendment, DESIGN.md section 7).
WIN_RATE_METRICS = ("spy_score", "eight_deck", "nine_deck")
# Bound -> (fill for D's missing cases, fill for the control's).
BOUNDS = {"worst": (0.0, 1.0), "best": (1.0, 0.0)}
# This analyzer's names for the archive reset pilot's RESULT.json metrics.
PILOT_METRICS = {"J": "combo_won", "combo_resolved": "combo_resolved", "spy_score": "spy_score",
                 "seven_deck": "seven_deck", "cawgates": "cawgates", "nine_deck": "nine_deck"}
INCONCLUSIVE = "inconclusive: missing cases decide"


def checkpoint_ids() -> set:
    return {f"{lineage}-{role}" for lineage in LINEAGES for role in ROLES}


def load(index: dict, allowed: set | None = None) -> dict:
    """data[(checkpoint, opponent)][(own, other, repeat, seat)] = case record."""
    allowed = checkpoint_ids() if allowed is None else allowed
    data = {}
    expected_cases = {(o, t, k, s) for o in range(DECKS) for t in range(DECKS)
                      for k in range(REPEATS) for s in (0, 1)}
    for panel in index["panels"]:
        key = (panel["checkpoint"], panel["opponent"])
        if key in data or key[1] not in OPPONENTS or key[0] not in allowed:
            raise ValueError(f"duplicate or unknown panel {key}")
        expected_panel = f"{key[0]}-vs-{key[1]}"
        expected_hash = panel.get("checkpoint_sha256")
        if (not isinstance(expected_hash, str) or len(expected_hash) != 64
                or any(c not in "0123456789abcdef" for c in expected_hash)):
            raise ValueError(f"{expected_panel}: expected checkpoint_sha256 is required")
        cases = {}
        for line in Path(panel["rows"]).read_text(encoding="utf-8").splitlines():
            if not line.strip():
                continue
            r = json.loads(line)
            case_key = (r["own"], r["other"], r["repeat"], r["seat"])
            if (any(type(v) is not int for v in case_key) or case_key not in expected_cases
                    or case_key in cases):
                raise ValueError(f"{panel['rows']}: duplicate or unknown case {case_key}")
            if (r.get("panel") != expected_panel or r.get("checkpoint_sha256") != expected_hash
                    or r.get("seed") != pilot.panel_seed(*case_key[:3])):
                raise ValueError(f"{panel['rows']}: panel, checkpoint or seed differs")
            if (type(r["complete"]) is not bool
                    or (r["complete"] and (type(r["score"]) not in (int, float) or r["score"] not in (0, 0.5, 1)))
                    or (not r["complete"] and r["score"] is not None)):
                raise ValueError(f"{panel['rows']}: invalid completion or score")
            cases[case_key] = r
        if cases.keys() != expected_cases:
            raise ValueError(f"{panel['rows']}: exact case roster differs")
        expected_labels = {f"{expected_panel}/o{o}-t{t}-k{k}-s{s}" for (o, t, k, s) in expected_cases if o == SPY}
        labels = {}
        for line in Path(panel["labels"]).read_text(encoding="utf-8").splitlines():
            if not line.strip():
                continue
            r = json.loads(line)
            if r["id"] not in expected_labels or r["id"] in labels:
                raise ValueError(f"{panel['labels']}: duplicate or foreign label {r['id']}")
            labels[r["id"]] = r
        for (o, t, k, s), case in cases.items():
            if o != SPY:
                continue
            lab = labels.get(f"{expected_panel}/o{o}-t{t}-k{k}-s{s}")
            if case["complete"] and (lab is None or lab["kind"] != "label_replay"):
                raise ValueError(f"{panel['labels']}: missing label for a complete Spy case o{o}-t{t}-k{k}-s{s}")
            if lab is not None and lab["kind"] == "label_replay":
                winner = lab["winner"]
                if (type(lab["focal"]) is not int or lab["focal"] != s
                        or (winner is not None and (type(winner) is not int or winner not in (0, 1)))
                        or type(lab["win"]) is not bool or lab["win"] != (winner == s)
                        or type(lab["combo_resolved"]) is not bool or type(lab["combo_won"]) is not bool
                        or lab["combo_won"] != (lab["combo_resolved"] and lab["win"])
                        or (case["complete"] and case["score"] != (0.5 if winner is None else float(winner == s)))):
                    raise ValueError(f"{panel['labels']}: focal, winner or score differs")
            case["labels"] = lab
        data[key] = cases
    return data


def case_value(case: dict, metric: str, fill):
    """A case's value for ``metric`` ("score" or a Spy metric), or ``fill`` when it is missing."""
    if not case["complete"]:
        return fill
    if metric == "score":
        return case["score"]
    return float(case["labels"]["combo_won" if metric == "J" else "combo_resolved"])


def mean(values):
    values = [v for v in values if v is not None]
    return sum(values) / len(values) if values else None


def summary(cases: dict, fill=None) -> dict:
    """Point values; ``fill`` None reads completed cases only."""
    decks = [mean([case_value(cases[(d, t, k, s)], "score", fill)
                   for t in range(DECKS) for k in range(REPEATS) for s in (0, 1)]) for d in range(DECKS)]
    out = {"deck_scores": decks, "nine_deck": mean(decks), "eight_deck": mean([decks[d] for d in EIGHT]),
           "seven_deck": mean([decks[d] for d in SEVEN]), "cawgates": decks[CAW], "spy_score": decks[SPY]}
    for metric in SPY_METRICS:
        out[metric] = mean([case_value(cases[(SPY, t, k, s)], metric, fill)
                            for t in range(DECKS) for k in range(REPEATS) for s in (0, 1)])
    return out


def missing(cases: dict) -> dict:
    gone = sorted(key for key, c in cases.items() if not c["complete"])
    return {"count": len(gone), "denominator": len(cases), "cases": [list(k) for k in gone]}


def secondary(cases: dict) -> dict:
    """Per-game line choices in completed Spy games, and the J games' clusters."""
    labels = [(t, k, c["labels"]) for (o, t, k, _), c in cases.items() if o == SPY and c["complete"]]
    out = {"spy_games": len(labels)}
    for name, offered, chosen in (("self_target", "self_offered", "self_chosen"),
                                  ("dr_giant", "dr_giant_offered", "dr_giant_chosen")):
        offer = [lab for _, _, lab in labels if lab["labels"][offered]]
        picks = sum(1 for lab in offer if lab["labels"][chosen])
        out[name] = {"offered_games": len(offer), "chosen_games": picks,
                     "chosen_per_offered_game": picks / len(offer) if offer else None}
    j = [(t, k) for t, k, lab in labels if lab["combo_won"]]
    out["J_games"] = len(j)
    out["J_clusters"] = len(set(j))
    return out


def arrays(cases: dict, fill: float):
    """score[o, t, k, s] and Spy metrics[m][t, k, s], missing cases at ``fill``."""
    import numpy as np
    score = np.full((DECKS, DECKS, REPEATS, 2), np.nan)
    spy = {m: np.full((DECKS, REPEATS, 2), np.nan) for m in SPY_METRICS}
    for (o, t, k, s), c in cases.items():
        score[o, t, k, s] = case_value(c, "score", fill)
        if o == SPY:
            for m in spy:
                spy[m][t, k, s] = case_value(c, m, fill)
    return score, spy


def draw_stats(score, spy, score_idx, spy_idx) -> dict:
    """One draw's panel values: tools/spy_reset/analyze.py's expressions,
    plus the eight-deck mean."""
    import numpy as np
    res = np.take_along_axis(score, score_idx, axis=2)
    decks = np.nanmean(res.reshape(DECKS, -1), axis=1)
    out = {"nine_deck": decks.mean(), "eight_deck": decks[EIGHT].mean(), "seven_deck": decks[SEVEN].mean(),
           "cawgates": decks[CAW], "spy_score": decks[SPY]}
    for m, v in spy.items():
        out[m] = np.nanmean(np.take_along_axis(v, spy_idx, axis=1))
    return out


def plan(data: dict, contrasts) -> list:
    """(lineage, name, D, control) for every contrast whose four panels exist."""
    return [(lineage, name, d, c) for lineage in LINEAGES for name, d, c in contrasts
            if all((f"{lineage}-{x}", opp) in data for x in (d, c) for opp in OPPONENTS)]


def points(data: dict, todo: list, metrics) -> dict:
    out, cache = {}, {}

    def s(ck, opp, fill):
        if (ck, opp, fill) not in cache:
            cache[(ck, opp, fill)] = summary(data[(ck, opp)], fill)
        return cache[(ck, opp, fill)]
    for lineage, name, d, c in todo:
        for bound, (fd, fc) in BOUNDS.items():
            sd = {opp: s(f"{lineage}-{d}", opp, fd) for opp in OPPONENTS}
            sc = {opp: s(f"{lineage}-{c}", opp, fc) for opp in OPPONENTS}
            for metric in metrics:
                for opp in OPPONENTS:
                    out[(lineage, name, metric, opp, bound)] = sd[opp][metric] - sc[opp][metric]
                out[(lineage, name, metric, "pooled", bound)] = (
                    (sd["T1"][metric] + sd["H"][metric]) / 2 - (sc["T1"][metric] + sc["H"][metric]) / 2)
    return out


def bootstrap(data: dict, todo: list, metrics, seed: int = BOOT_SEED) -> dict:
    """Cluster bootstrap intervals for every (lineage, contrast, metric,
    opponent or pooled, bound): one ``rng.integers`` draw of repeat indices
    per ordered deck pair, shared by every panel."""
    import numpy as np
    draws = DRAWS
    rng = np.random.default_rng(seed)
    filled = {}
    for key, cases in data.items():
        zero = arrays(cases, 0.0)
        filled[key] = {0.0: zero, 1.0: arrays(cases, 1.0) if missing(cases)["count"] else zero}
    keys = [(lineage, name, metric, opp, bound) for lineage, name, _, _ in todo for bound in BOUNDS
            for metric in metrics for opp in (*OPPONENTS, "pooled")]
    values = {key: np.empty(draws) for key in keys}
    for i in range(draws):
        idx = rng.integers(0, REPEATS, size=(DECKS, DECKS, REPEATS))
        score_idx = idx[:, :, :, None].repeat(2, axis=3)
        spy_idx = idx[SPY][:, :, None].repeat(2, axis=2)
        cache = {}

        def stats(ck, opp, fill):
            arrs = filled[(ck, opp)][fill]
            if id(arrs) not in cache:
                cache[id(arrs)] = draw_stats(*arrs, score_idx, spy_idx)
            return cache[id(arrs)]
        for lineage, name, d, c in todo:
            for bound, (fd, fc) in BOUNDS.items():
                sd = {opp: stats(f"{lineage}-{d}", opp, fd) for opp in OPPONENTS}
                sc = {opp: stats(f"{lineage}-{c}", opp, fc) for opp in OPPONENTS}
                for metric in metrics:
                    for opp in OPPONENTS:
                        values[(lineage, name, metric, opp, bound)][i] = sd[opp][metric] - sc[opp][metric]
                    values[(lineage, name, metric, "pooled", bound)][i] = (
                        (sd["T1"][metric] + sd["H"][metric]) / 2 - (sc["T1"][metric] + sc["H"][metric]) / 2)
    out = {}
    for key, v in values.items():
        v = np.sort(v)
        out[key] = [float(v[int(0.025 * draws)]), float(v[int(0.975 * draws) - 1])]
    return out


def guards_hold(ci, lineage: str, name: str, bound: str) -> bool:
    """Eight-deck lower bound above -0.02, and no detected Spy-score harm."""
    return (ci[(lineage, name, "eight_deck", "pooled", bound)][0] > -RETENTION
            and not ci[(lineage, name, "spy_score", "pooled", bound)][1] < 0)


def guard_reading(ci, lineage: str, name: str, bound: str) -> str:
    eight = ci[(lineage, name, "eight_deck", "pooled", bound)]
    spy = ci[(lineage, name, "spy_score", "pooled", bound)]
    if spy[1] < 0 or eight[1] < 0:
        return "interference (detected harm)"
    if eight[0] <= -RETENTION:
        return "retention not shown (inconclusive)"
    return "guards hold"


def j_above(ci, lineage: str, name: str, bound: str) -> bool:
    return ci[(lineage, name, "J", "pooled", bound)][0] > 0


def line_reading(ci, arm: str, bound: str, offer_probability) -> str:
    """Section 7's panel line readings on J alone (first matching row). A
    result established at one checkpoint only says nothing about loss or
    acquisition between them; that needs the direct contrast."""
    d1, d2 = f"D1{arm}-parent", f"D2{arm}-O"
    at_d1 = [j_above(ci, lin, d1, bound) for lin in LINEAGES]
    at_d2 = [j_above(ci, lin, d2, bound) for lin in LINEAGES]
    if all(at_d1) and all(at_d2):
        return "behavioral transfer established at D1 and D2"
    if all(at_d1):
        return "behavioral transfer established only at D1"
    if all(at_d2):
        return "behavioral transfer established only at D2"
    if any(at_d1) or any(at_d2):
        return "lineage-dependent; the screen cannot read positive"
    if offer_probability is None:
        return "no behavioral transfer established; held-out offer probability not given"
    if offer_probability < OFFER_FLOOR:
        return "no behavioral transfer established; D1's held-out offer probability is below 0.05"
    return "no behavioral transfer established; D1's held-out offer probability is 0.05 or more"


def direct_reading(ci, arm: str, bound: str) -> str:
    """A behavioral loss or acquisition between D1 and D2, claimed only from
    D2's J against D1 itself, in both lineages."""
    j = [ci[(lin, f"D2{arm}-D1{arm}", "J", "pooled", bound)] for lin in LINEAGES]
    if all(high < 0 for _, high in j):
        return "behavioral loss between D1 and D2"
    if all(low > 0 for low, _ in j):
        return "behavioral acquisition between D1 and D2"
    if any(high < 0 or low > 0 for low, high in j):
        return "lineage-dependent change between D1 and D2; no loss or acquisition claimed"
    return "no change between D1 and D2 established"


def effect_reading(interval) -> str:
    low, high = interval
    if low > 0:
        return "higher established"
    if high < 0:
        return "lower established"
    return "no change established"


def across(per: dict) -> str:
    """One bound's reading across lineages: an effect holds only when every
    lineage reads it, as for J."""
    if len(per) < len(LINEAGES):
        return "descriptive only: one lineage"
    values = set(per.values())
    return values.pop() if len(values) == 1 else "lineage-dependent"


def win_rates(ci, names) -> dict:
    """Playing-strength effects, apart from J: the pooled Spy, eight-deck and
    nine-deck score contrasts of every endpoint, per lineage and across
    lineages, each read under both bounds."""
    out = {}
    for name in names:
        for metric in WIN_RATE_METRICS:
            lineages = [lin for lin in LINEAGES if (lin, name, metric, "pooled", "worst") in ci]
            if not lineages:
                continue
            by_bound = {b: {lin: effect_reading(ci[(lin, name, metric, "pooled", b)]) for lin in lineages}
                        for b in BOUNDS}
            out[f"{name}/{metric}"] = {
                "reading": agree({b: across(per) for b, per in by_bound.items()}),
                "per_lineage": {lin: agree({b: by_bound[b][lin] for b in BOUNDS}) for lin in lineages}}
    return out


def offer_probability(fit: dict | None, arm: str):
    """D1's held-out offer probability for rows 5-6: pooled across lineages
    when fit.py pooled it, otherwise the larger lineage value (row 5 needs
    every lineage below the floor)."""
    if fit is None:
        return None
    entry = fit["learner_offers"][arm]
    if entry["pooled"] is not None:
        return entry["pooled"]["mean_deployed_self_probability"]
    values = [v["mean_deployed_self_probability"] for v in entry["per_lineage"].values()]
    return max(values) if len(values) == len(LINEAGES) and None not in values else None


def agree(by_bound: dict):
    values = list(by_bound.values())
    return values[0] if all(v == values[0] for v in values) else INCONCLUSIVE


def screen(ci: dict, fit: dict | None) -> dict:
    out = {}
    for arm in ("U", "L"):
        d1, d2 = f"D1{arm}-parent", f"D2{arm}-O"
        lineages = [lin for lin in LINEAGES if all((lin, n, "J", "pooled", "worst") in ci for n in (d1, d2))]
        entry = {"lineages": lineages, "guards": {}}
        for lin in lineages:
            for name in (d1, d2):
                entry["guards"][f"{lin}/{name}"] = agree({b: guard_reading(ci, lin, name, b) for b in BOUNDS})
        if len(lineages) < len(LINEAGES):
            entry["reading"] = ("descriptive only: one lineage passed the fit check" if lineages
                                else "not run: the arm passed the fit check in neither lineage")
            out[arm] = entry
            continue
        for kind, name in (("screen_at_D1", d1), ("screen_at_D2", d2)):
            holds = {b: all(j_above(ci, lin, name, b) and guards_hold(ci, lin, name, b) for lin in LINEAGES)
                     for b in BOUNDS}
            entry[kind] = (True if holds["worst"] else INCONCLUSIVE if holds["best"] else False)
        p = offer_probability(fit, arm)
        entry["offer_probability"] = p
        entry["reading"] = agree({b: line_reading(ci, arm, b, p) for b in BOUNDS})
        entry["direct"] = agree({b: direct_reading(ci, arm, b) for b in BOUNDS})
        entry["direct_intervals"] = {
            lin: {metric: {b: ci[(lin, f"D2{arm}-D1{arm}", metric, "pooled", b)] for b in BOUNDS}
                  for metric in DIRECT_CONTEXT} for lin in LINEAGES}
        out[arm] = entry
    return out


def analyze(index: dict, fit: dict | None) -> dict:
    data = load(index)
    result = {"schema": "spy-archive-distillation-v1/analysis", "seed": BOOT_SEED, "draws": DRAWS,
              "panels": {}, "missing": {}, "secondary": {}}
    for (ck, opp), cases in sorted(data.items()):
        result["panels"][f"{ck}/{opp}"] = summary(cases)
        result["missing"][f"{ck}/{opp}"] = missing(cases)
        result["secondary"][f"{ck}/{opp}"] = secondary(cases)
    todo = plan(data, SCREEN + DIRECT + DESCRIPTIVE)
    metrics = METRICS + EXTRA_METRICS
    point, ci = points(data, todo, metrics), bootstrap(data, todo, metrics)
    result["contrasts"] = {"/".join(k): {"point": point[k], "ci95": ci[k]} for k in point}
    result["screen"] = screen(ci, fit)
    result["win_rates"] = win_rates(ci, [name for name, _, _ in SCREEN + DIRECT])
    return result


def validate(index: dict, pilot_result: dict) -> list:
    """Mismatches between this analyzer's O-minus-parent values (per
    opponent, old seed) and the archive reset pilot's RESULT.json."""
    needed = {f"{lin}-{role}" for lin in LINEAGES for role in ("parent", "O")}
    data = load({"panels": [p for p in index["panels"] if p["checkpoint"] in needed]}, needed)
    todo = plan(data, (("O-parent", "O", "parent"),))
    if len(todo) != len(LINEAGES):
        return ["the index lacks a lineage's parent or O panels"]
    if any(missing(cases)["count"] for cases in data.values()):
        return ["validation panels have missing cases"]
    metrics = tuple(PILOT_METRICS)
    point, ci = points(data, todo, metrics), bootstrap(data, todo, metrics, VALIDATION_SEED)
    bad = []
    for lineage in LINEAGES:
        for opp in OPPONENTS:
            for metric, old in PILOT_METRICS.items():
                key = (lineage, "O-parent", metric, opp, "worst")
                recorded = pilot_result["contrasts"][f"{lineage}/{opp}/O-parent/{old}"]
                if recorded["ci95"] != ci[key] or recorded["point"] != point[key]:
                    bad.append({"contrast": f"{lineage}/{opp}/O-parent/{old}", "recorded": recorded,
                                "recomputed": {"point": point[key], "ci95": ci[key]}})
    return bad


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    sub = ap.add_subparsers(dest="op", required=True)
    a = sub.add_parser("analyze")
    a.add_argument("--index", type=Path, required=True)
    a.add_argument("--fit", type=Path)
    a.add_argument("--out", type=Path, required=True)
    v = sub.add_parser("validate")
    v.add_argument("--index", type=Path, required=True)
    v.add_argument("--result", type=Path, required=True)
    args = ap.parse_args(argv)
    if args.op == "validate":
        bad = validate(read_json(args.index), read_json(args.result))
        print(json.dumps({"equal": not bad, "mismatches": bad}))
        return 1 if bad else 0
    result = analyze(read_json(args.index), read_json(args.fit) if args.fit else None)
    args.out.write_text(json.dumps(result, indent=1) + "\n", encoding="utf-8")
    print(json.dumps({"screen": result["screen"], "win_rates": result["win_rates"]}))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
