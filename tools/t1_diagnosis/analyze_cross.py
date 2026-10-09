#!/usr/bin/env python3
"""Summarize `regret_census_v1 mode=cross` rows (stage 3).

Cells: root action (the model's sampled root action `t1`, or the search
alternative `alt`: condition B's choice at the top budget, or the next best
by mean when that equals the model's, `alt_forced`) x follow-up (`t1` =
plain, `improved:turn`, and the long arm `improved:turn+G` under
HORIZON_LONG, `improved:game` by default). Cells are looked up by name in
each row. The outcome per root and cell is the share of paired evaluation
playouts that ended in a natural focal win.

Rules (design v3, "Stage 3: the crossing"):
- every interval is a percentile bootstrap clustered by corpus game;
- primary (frozen): V(model, long) - V(model, turn), with the stage-2
  dispositions at delta = 5pp: helps = plain 95% lower bound above 0;
  evidence against = 95% upper bound below delta and stage-3 adequacy met;
  inconclusive otherwise;
- stage-3 adequacy, each criterion reported: (a) the long arm's cap-hit
  rate is at most 25% (over the model's-action long cell, the primary's
  arm); (b) the long arm searches at least one decision beyond the root's
  turn at 75% or more of roots, per root (model, long) searched decisions >
  (model, turn) searched decisions (the arms nest, so a larger count means a
  decision beyond the turn was searched); (c) the turn continuation check
  (model, turn) - (model, plain) is at least 0 as a point estimate;
- the reading table: four contrasts with Holm across them (step-down; the
  contrast ranked i by bootstrap p-value uses the 1 - 0.05/(4 - i) interval,
  "helps" = Holm-adjusted lower bound above 0): isolated correction
  (alt, plain) - (model, plain) on unforced roots; within-turn gain
  (model, turn) - (model, plain); beyond-turn primary; alternative with
  follow-up (alt, turn) - (alt, plain). Rows are checked top to bottom and
  the first match wins; each contrast's disposition is reported (helps by
  the Holm bound, evidence against by the plain upper bound and adequacy).

The long arm's cap-hit rate of every cell is reported (a capped arm is
"long", not "game", when its cap binds in more than a quarter of playouts).

Every logged deviation (a focal decision where improved play chose another
action than the model's sample) is classified by keywords into mana
allocation, creature preservation, attacks, Gate targets or colours,
prevention, card selection or other. Each deviation inherits the paired
playout result: success when the improved playout scored above the
plain-follow-up playout from the same root action and determinization,
failure when below, neutral otherwise. That inherited label is not a
counterfactual. When rows carry deviation counterfactuals (DEVIATION_EVAL), a
class's cost is the mean difference, chosen minus sampled, over its
counterfactual-evaluated deviations (natural-win share primary, mean score
alongside); a positive cost means the model's sampled action did worse than
the improved choice, and a harmful deviation (the improved choice won fewer
natural wins) counts as a search failure. Through the root's turn the turn
and long arms of one root action have the same state and decision seeds, so
an in-turn deviation is logged (and counterfactually evaluated) in both; the
costs and transitions count each (root, root action, playout, ordinal) once.

`--expect MANIFEST.json` rejects rows whose recorded `config` differs from
the manifest, as in analyze_search.py.

Usage: analyze_cross.py CROSS.jsonl [MORE.jsonl ...] [--boot 2000] [--seed 7] [--examples 3]
       [--expect MANIFEST.json] [--json OUT]
"""

import argparse
import json
import os
import random
import sys
from collections import Counter, defaultdict

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from analyze_search import (  # noqa: E402
    DELTA,
    HELPS,
    ClusterBoot,
    check_manifest,
    disposition,
    drop_no_evaluation,
    holm_bounds,
)

T1 = "t1"
TURN = "improved:turn"
GAME = "improved:game"
CAP_MAX = 0.25
BEYOND_SHARE = 0.75
CLASSES = [
    "prevention",
    "gate_targets_colours",
    "mana_allocation",
    "attacks",
    "creature_preservation",
    "card_selection",
    "other",
]
READING = [
    ("isolated_correction", "one isolated correction carries the benefit"),
    ("within_turn", "local sequencing and timing lead"),
    ("beyond_turn", "repeated mistakes or longer dependencies matter; this alone does not show a memory deficit"),
    ("alt_turn", "the initial correction helps only with better follow-up"),
    (None, "no stage-3 explanation is supported; each contrast's disposition is reported"),
]


def mean(xs):
    xs = list(xs)
    return sum(xs) / len(xs) if xs else float("nan")


def win(scores):
    return mean(1.0 if s == 1.0 else 0.0 for s in scores)


def fmt(x):
    return "   n/a" if x != x else f"{100 * x:6.1f}"


def classify(dev):
    """Keyword classifier over both semantics strings of a deviation.

    Order: prevention, mana allocation (any mana ability, including a Gate's),
    Gate targets or colours (a Gate's non-mana ability, target or colour
    choice), attacks, creature preservation, card selection, other."""
    text = f"{dev.get('sampled_sem') or ''} {dev.get('chosen_sem') or ''}"
    parsed = []
    for s in (dev.get("sampled_sem"), dev.get("chosen_sem")):
        try:
            parsed.append(json.loads(s))
        except (TypeError, ValueError):
            parsed.append({})
    kinds = {p.get("action_kind", "") for p in parsed if isinstance(p, dict)}
    sources = {str(p.get("source", "")) for p in parsed if isinstance(p, dict)}
    if "Prismatic Strands" in text or "prevent" in text.lower():
        return "prevention"
    if "activate_mana_ability" in kinds or "mana_choice" in text:
        return "mana_allocation"
    if any("Gate" in s for s in sources) or "choose_effect_color" in kinds:
        return "gate_targets_colours"
    if kinds & {"choose_attacker_inclusion", "declare_attackers"}:
        return "attacks"
    if kinds & {"choose_blocker_inclusion", "declare_blockers_for_attacker", "choose_cost_target"} or "sacrific" in text.lower():
        return "creature_preservation"
    if kinds & {"cast_spell", "play_land", "discard", "choose_target", "choose_effect_target",
                "choose_london_bottom", "activate_ability", "pass"}:
        return "card_selection"
    return "other"


def long_arm(cells):
    """The long arm's follow-up label: the model row's follow-up that is
    neither plain nor turn."""
    follows = [c.split("|", 1)[1] for c in cells if c.startswith("t1|")]
    longs = [f for f in follows if f not in (T1, TURN)]
    return longs[0] if longs else GAME


def jobs_by_cell(row):
    """{cell name: evaluation job} for one row."""
    return dict(zip(row["cells"], row["eval"]["jobs"], strict=True))


def cell_wins(row):
    return {c: win(j["scores"]) for c, j in jobs_by_cell(row).items()}


def cap_rates(rows, cells):
    """{cell: cap hits, playouts, rate} when the rows record cap hits."""
    if not any("cap_hits" in j for r in rows for j in r["eval"]["jobs"]):
        return {}
    out = {}
    for c in cells:
        jobs = [jobs_by_cell(r).get(c) for r in rows]
        hits = sum(j.get("cap_hits", 0) for j in jobs if j)
        n = sum(len(j["scores"]) for j in jobs if j)
        out[c] = {"cap_hits": hits, "playouts": n, "rate": hits / n if n else float("nan")}
    return out


def counterfactual_costs(rows):
    """Per-class cost of deviations from their fresh counterfactuals.

    The turn and long arms of one root action nest: through the root's turn
    they reach the same states with the same decision seeds, so an in-turn
    deviation (and its counterfactual) appears in both cells. Each (root,
    root action, playout, ordinal) is counted once, for costs and
    transitions alike. `harmful`/`helpful` are decided on natural wins."""
    classes = {cls: {"evaluated": 0, "helpful": 0, "harmful": 0, "neutral": 0, "win": [], "score": []}
               for cls in CLASSES}
    logged = evaluated = transitions = duplicates = 0
    seen = set()
    for idx, r in enumerate(rows):
        rid = r.get("root_id", r.get("root_index", idx))
        for c, job in jobs_by_cell(r).items():
            init, follow = c.split("|", 1)
            if follow == T1:
                continue
            for dev in job["deviations"]:
                logged += 1
                cf = dev.get("counterfactual")
                if not cf:
                    continue
                key = (rid, init, dev.get("playout"), dev.get("ordinal", ("step", dev.get("step"))))
                if key in seen:
                    duplicates += 1
                    continue
                seen.add(key)
                transitions += cf.get("transitions", 0)
                n = cf.get("playouts", 0)
                if not n:
                    continue
                evaluated += 1
                e = classes[classify(dev)]
                e["evaluated"] += 1
                dw = cf["chosen_natural_wins"] - cf["sampled_natural_wins"]
                e["win"].append(dw / n)
                e["score"].append(cf["chosen_mean"] - cf["sampled_mean"])
                if dw < 0:
                    e["harmful"] += 1
                elif dw > 0:
                    e["helpful"] += 1
                else:
                    e["neutral"] += 1
    for e in classes.values():
        e["cost_win"] = mean(e.pop("win"))
        e["cost_score"] = mean(e.pop("score"))
    return {"logged": logged, "evaluated": evaluated, "transitions": transitions,
            "duplicates_skipped": duplicates, "classes": classes}


def reading_contrasts(rows, per, long_follow):
    """Per-root values of the four reading-table contrasts (None where a
    root does not enter: forced roots for the isolated correction)."""
    long_t1, long_alt = f"t1|{long_follow}", f"alt|{long_follow}"

    def d(x, y, keep=lambda r: True):
        return [v[x] - v[y] if keep(r) and x in v and y in v else None for r, v in zip(rows, per, strict=True)]

    return {
        "isolated_correction": d(f"alt|{T1}", f"t1|{T1}", lambda r: not r.get("alt_forced", False)),
        "within_turn": d(f"t1|{TURN}", f"t1|{T1}"),
        "beyond_turn": d(long_t1, f"t1|{TURN}"),
        "alt_turn": d(f"alt|{TURN}", f"alt|{T1}"),
    }, {
        "isolated_correction_all_roots": d(f"alt|{T1}", f"t1|{T1}"),
        "long_vs_turn_alt": d(long_alt, f"alt|{TURN}"),
    }


def stage3_adequacy(rows, boot, values, long_follow):
    """Stage-3 adequacy, each criterion reported individually."""
    long_t1 = f"t1|{long_follow}"
    hits = n = 0
    beyond = []
    for r in rows:
        jobs = jobs_by_cell(r)
        lj, tj = jobs.get(long_t1), jobs.get(f"t1|{TURN}")
        if lj is not None:
            hits += lj.get("cap_hits", 0)
            n += len(lj["scores"])
        beyond.append(lj is not None and tj is not None
                      and lj.get("searched_decisions", 0) > tj.get("searched_decisions", 0))
    rate = hits / n if n else float("nan")
    share = mean(1.0 if b else 0.0 for b in beyond)
    cont = boot.point(values["within_turn"])
    out = {
        "a_cap_hit_rate": {"passed": n > 0 and rate <= CAP_MAX, "rate": rate, "cap_hits": hits, "playouts": n,
                           "threshold": CAP_MAX, "cell": long_t1},
        "b_beyond_turn": {"passed": bool(rows) and share >= BEYOND_SHARE, "share": share, "roots": sum(beyond),
                          "of": len(rows), "threshold": BEYOND_SHARE},
        "c_turn_continuation": {"passed": cont >= 0, "diff": cont},
    }
    out["met"] = all(c["passed"] for c in out.values())
    return out


def read_table(helps):
    """The predeclared reading: first matching row, top to bottom."""
    iso, within, beyond, alt_turn = (helps[k] for k in ("isolated_correction", "within_turn", "beyond_turn",
                                                         "alt_turn"))
    matches = [iso and not beyond, within and not beyond, beyond, alt_turn and not iso, True]
    i = matches.index(True)
    return READING[i][0], READING[i][1]


def analyze(rows, n_boot, seed, delta=DELTA):
    """The stage-3 statistics of `rows`: cells, the reading contrasts with
    Holm, the primary's disposition, adequacy and the reading."""
    longs = {long_arm(r["cells"]) for r in rows}
    if len(longs) > 1:
        raise SystemExit(f"rows mix long arms {sorted(longs)}; pass --expect to keep one configuration")
    long_follow = longs.pop()
    per = [cell_wins(r) for r in rows]
    boot = ClusterBoot(rows, n_boot, random.Random(seed))
    cells = []
    for r in rows:
        cells += [c for c in r["cells"] if c not in cells]
    out = {"roots": len(rows), "games": len({r.get("game") for r in rows}), "long_follow": long_follow,
           "cells": {c: boot.point([v.get(c) for v in per]) for c in cells}}
    values, extra = reading_contrasts(rows, per, long_follow)
    adequacy = stage3_adequacy(rows, boot, values, long_follow)
    out["adequacy"] = adequacy
    hb = holm_bounds(boot, values)
    contrasts = {}
    for k, d in values.items():
        lo, hi = boot.ci(d)
        e = {"diff": boot.point(d), "lo": lo, "hi": hi, "n": sum(x is not None for x in d)}
        e.update(hb[k])
        e["disposition"] = disposition(hb[k]["helps"], hi, adequacy["met"], delta)
        contrasts[k] = e
    p = contrasts["beyond_turn"]
    out["primary"] = {"contrast": f"V(model, {long_follow}) - V(model, {TURN})", "diff": p["diff"], "lo": p["lo"],
                      "hi": p["hi"], "n": p["n"], "delta": delta,
                      "disposition": disposition(p["lo"] > 0, p["hi"], adequacy["met"], delta)}
    out["reading_contrasts"] = contrasts
    out["descriptive"] = {}
    for k, d in extra.items():
        lo, hi = boot.ci(d)
        out["descriptive"][k] = {"diff": boot.point(d), "lo": lo, "hi": hi, "n": sum(x is not None for x in d)}
    row, text = read_table({k: contrasts[k]["helps"] for k in values})
    out["reading"] = {"row": row, "text": text}
    return out, per, cells


def print_analysis(res):
    print(f"\nprimary {res['primary']['contrast']}: {fmt(res['primary']['diff'])}  "
          f"[{fmt(res['primary']['lo'])}, {fmt(res['primary']['hi'])}]  n {res['primary']['n']}  "
          f"-> {res['primary']['disposition']} (delta {100 * res['primary']['delta']:.0f}pp, plain 95% bounds)")
    a = res["adequacy"]
    print(f"stage-3 adequacy: {'met' if a['met'] else 'NOT met'}")
    for k, c in a.items():
        if k == "met":
            continue
        fields = ", ".join(f"{f} {round(v, 4) if isinstance(v, float) else v}" for f, v in c.items() if f != "passed")
        print(f"  {k:<20} {'pass' if c['passed'] else 'FAIL'}  ({fields})")
    print("reading contrasts (pp; 95% bootstrap clustered by corpus game; Holm step-down across the four):")
    for k, e in res["reading_contrasts"].items():
        print(f"  {k:<20} {fmt(e['diff'])}  [{fmt(e['lo'])}, {fmt(e['hi'])}]  Holm [{fmt(e['lo_holm'])}, "
              f"{fmt(e['hi_holm'])}]  n {e['n']}  {e['disposition']}")
    print("  (isolated_correction is on unforced roots; descriptive below)")
    for k, e in res["descriptive"].items():
        print(f"  {k:<30} {fmt(e['diff'])}  [{fmt(e['lo'])}, {fmt(e['hi'])}]  n {e['n']}")
    print(f"reading (first matching row): {res['reading']['text']}")
    if res["reading"]["row"] is None:
        print("  " + ", ".join(f"{k}: {e['disposition']}" for k, e in res["reading_contrasts"].items()))
    if res["primary"]["disposition"] == HELPS and not res["reading_contrasts"]["beyond_turn"]["helps"]:
        print("  note: the primary helps on its plain bound but not on the Holm bound the reading table uses")


def load(paths):
    rows, errors, seen = [], [], set()
    for path in paths:
        with open(path) as f:
            for line in f:
                r = json.loads(line)
                if r.get("kind") == "cross" and r.get("root_id") not in seen:
                    seen.add(r.get("root_id"))
                    rows.append(r)
                elif r.get("kind") == "error":
                    errors.append(r)
    rows.sort(key=lambda r: (r["turn"], str(r.get("root_id"))))
    return rows, errors


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("cross", nargs="+", help="one or more cross output files (rows deduplicated by root_id)")
    ap.add_argument("--boot", type=int, default=2000)
    ap.add_argument("--seed", type=int, default=7)
    ap.add_argument("--examples", type=int, default=3)
    ap.add_argument("--expect", help="manifest whose config every row must match")
    ap.add_argument("--json")
    a = ap.parse_args()
    rows, errors = load(a.cross)
    print(f"rows {len(rows)}, error rows {len(errors)}")
    for e in errors:
        print(f"  error root {e.get('game')}: {str(e.get('error'))[:200]}")
    out = {"errors": len(errors)}
    if a.expect:
        with open(a.expect) as f:
            manifest = json.load(f)
        rows, rejected = check_manifest(rows, manifest)
        out["rejected"] = len(rejected)
        print(f"manifest {a.expect}: {len(rows)} rows match, {len(rejected)} rejected")
        for r, reasons in rejected[:10]:
            print(f"  rejected {r.get('root_id', r.get('root_index'))}: {'; '.join(reasons)[:300]}")
    else:
        configs = {json.dumps(r.get("config"), sort_keys=True) for r in rows}
        if len(configs) > 1:
            print(f"WARNING: rows carry {len(configs)} distinct configs; pass --expect to keep one")
    rows = drop_no_evaluation(rows, out)
    if not rows:
        return
    res, per, cells = analyze(rows, a.boot, a.seed)
    out.update(res)
    print(f"\n{res['roots']} roots from {res['games']} corpus games; long arm {res['long_follow']}")
    print(f"\n{'root':<12}{'tag':<22}{'alt forced':>11}" + "".join(f"{c:>22}" for c in cells))
    for r, v in zip(rows, per, strict=True):
        print(f"{str(r.get('root_id', r['root_index'])):<12}{str(r.get('mechanism_tag', '')):<22}"
              f"{str(r['alt_forced']):>11}" + "".join(f"{fmt(v.get(c, float('nan'))):>22}" for c in cells))
    print("mean win%   " + " " * 33 + "".join(f"{fmt(res['cells'][c]):>22}" for c in cells))
    print_analysis(res)

    caps = cap_rates(rows, cells)
    if caps:
        out["cap_hit"] = caps
        print("\ncap hit rate per cell (a focal multi-action decision after the capped window closed):")
        for c in cells:
            e = caps[c]
            print(f"  {c:<28} {e['cap_hits']:>5} of {e['playouts']:>5}  {fmt(e['rate'])}%")

    # Deviations, labelled by the paired playout result.
    counts = defaultdict(Counter)
    examples = defaultdict(list)
    per_cell = Counter()
    for r in rows:
        jobs = jobs_by_cell(r)
        for c, job in jobs.items():
            init, follow = c.split("|", 1)
            if follow == T1 or f"{init}|{T1}" not in jobs:
                continue
            base = jobs[f"{init}|{T1}"]["scores"]
            mine = job["scores"]
            for dev in job["deviations"]:
                e = dev["playout"]
                delta = mine[e] - base[e] if e < len(mine) and e < len(base) else 0.0
                label = "success" if delta > 0 else "failure" if delta < 0 else "neutral"
                cls = classify(dev)
                counts[cls][label] += 1
                counts[cls]["total"] += 1
                if dev.get("all_equal"):
                    counts[cls]["tie"] += 1
                per_cell[c] += 1
                if len(examples[cls]) < a.examples and not dev.get("all_equal"):
                    examples[cls].append({"root": r.get("root_id"), "cell": c, "turn": dev["turn"],
                                          "phase": dev["phase"], "own_turn": dev.get("own_turn"),
                                          "t1": dev.get("sampled_sem"), "improved": dev.get("chosen_sem"),
                                          "means": dev.get("means"), "result": label})
    print("\ndeviations per cell: " + ", ".join(f"{c}: {n}" for c, n in sorted(per_cell.items())))
    print("(tie = every candidate had the same inner mean, so the higher-probability rule, not the")
    print(" search, made the choice differ from T1's sample; only outputs made before the improved")
    print(" continuation kept T1's sampled action on ties can contain these)")
    print(f"\n{'class':<24}{'total':>7}{'success':>9}{'failure':>9}{'neutral':>9}{'tie':>7}")
    for cls in CLASSES:
        c = counts[cls]
        print(f"{cls:<24}{c['total']:>7}{c['success']:>9}{c['failure']:>9}{c['neutral']:>9}{c['tie']:>7}")
    for cls in CLASSES:
        for ex in examples[cls]:
            print(f"  [{cls}] {ex['root']} {ex['cell']} t{ex['turn']} {ex['phase']} own={ex['own_turn']} "
                  f"{ex['result']} means={[round(m, 2) for m in ex['means'] or []]}\n"
                  f"      T1:       {str(ex['t1'])[:200]}\n      improved: {str(ex['improved'])[:200]}")
    cf = counterfactual_costs(rows)
    if cf["evaluated"]:
        out["counterfactual"] = cf
        print(f"\ndeviation counterfactuals: {cf['evaluated']} distinct evaluated deviations "
              f"({cf['duplicates_skipped']} nested-arm duplicates skipped) of {cf['logged']} logged "
              f"({cf['transitions']} transitions, counted apart from the playouts)")
        print("cost = mean(chosen - sampled) over evaluated deviations; harmful = improved choice won fewer "
              "natural wins (a search failure)")
        print(f"{'class':<24}{'evaluated':>10}{'cost win pp':>12}{'cost score':>11}{'helpful':>9}{'harmful':>9}{'neutral':>9}")
        for cls in CLASSES:
            e = cf["classes"][cls]
            print(f"{cls:<24}{e['evaluated']:>10}{fmt(e['cost_win']):>12}{fmt(e['cost_score']):>11}"
                  f"{e['helpful']:>9}{e['harmful']:>9}{e['neutral']:>9}")
    out["deviations"] = {cls: dict(counts[cls]) for cls in CLASSES}
    out["deviations_per_cell"] = dict(per_cell)
    out["examples"] = {cls: examples[cls] for cls in CLASSES}

    print("\ncost per root:")
    for r in rows:
        jobs = jobs_by_cell(r)
        parts = [f"{c}={mean(j['transitions']):.0f}tr/{mean(j['wall']):.1f}s" for c, j in jobs.items()]
        print(f"  {r.get('root_id')}: sel {r['selection_wall']:.1f}s, eval {r['eval']['actual_wall']:.1f}s, "
              f"failures {r['eval']['failed_playouts']}, inner failures "
              f"{sum(j['inner_failures'] for j in jobs.values())}; " + ", ".join(parts))
    if a.json:
        with open(a.json, "w") as f:
            json.dump(out, f, indent=1)


if __name__ == "__main__":
    main()
