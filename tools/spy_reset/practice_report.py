"""Frozen analysis of the practice track (discovery-efficiency-v1, RUNNER.md sections 3-4).

  exposure  --root ROOT --out OUT.json
      Training exposure funnel and gradient inclusion from each block's exposure.jsonl
      and updates.json under ROOT/train/<arm>/b<k>/ (blocks 0-7 main, 8-9 ordinary-only).
  panel     --index INDEX.json --out OUT.json [--no-bootstrap]
      INDEX: {"panels": [{"checkpoint": ID, "opponent": "T1"|"H", "rows": PATH, "labels": PATH}]}
      with IDs r1-parent, r2-parent and <lineage>-<O|U|M>-<main|final>.

Panel outputs, separately by lineage and reference: deck scores (win 1, draw 0.5, loss 0),
the equal-weight nine-deck score, Spy J (fresh resolved sequence then natural win), C
(sequence regardless of win), self-target, Dread Return and Spy score over the fixed 72
Spy cases; contrasts M-U (final: primary; main: acquisition), M-O, U-O and endpoint-parent;
then equal-weight aggregates over lineages and references. 10,000 paired cluster-bootstrap
draws (seat-pair blocks resampled within ordered matchups, jointly across references and
checkpoints, seed 2026101201). Missing technical cases are unknown with best/worst bounds.
Per non-Spy deck retention: endpoint-parent and endpoint-O with intervals; lower bound below
-0.02 is an unresolved risk, upper bound below -0.02 is material harm. Then the fixed
descriptive dispositions.
"""
from __future__ import annotations

import argparse
import json
import random
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import practice  # noqa: E402

DECKS, SPY, REPEATS = practice.DECKS, practice.SPY, practice.EVAL_REPEATS
LINEAGES, ARMS, REFS = ("r1", "r2"), practice.ARMS, ("T1", "H")
ENDS = ("main", "final")
BOOT_SEED, DRAWS, RETENTION = 2026101201, 10_000, 0.02
EVENTS = ("cast_offered", "cast_chosen", "self_offered", "self_chosen", "dr_giant_offered", "dr_giant_chosen",
          "spy_resolved", "self_target_resolved", "dr_giant_resolved")


# ---- exposure (RUNNER section 3) ----

def exposure(root: Path) -> dict:
    out = {}
    for lineage in LINEAGES:
        for arm in ARMS:
            name = f"{lineage}-{arm}"
            agg = {ph: {"episodes": 0, "reset": 0, "spy_reset": 0, "spy_ordinary": 0, "unique_sources": set(),
                        "unique_cuts": set(), "repeated_cuts": 0, "inherited_stage": {}, "fresh": {e: 0 for e in EVENTS},
                        "fresh_full_sequence": 0, "remaining_sequence_completed": 0, "inherited_complete": 0,
                        "spy_reset_wdl": [0, 0, 0], "spy_ordinary_fresh_full_sequence": 0,
                        "returns": 0, "suffix_rows": 0, "learner_rows": 0, "value_abs_error": 0.0,
                        "by_deck_reset": {}, "update_mismatches": 0, "updates": 0,
                        "prefix_rows_in_episodes": 0}
                   for ph in ("main", "wash")}
            for k in range(10):
                block = root / "train" / name / f"b{k}"
                ph = "main" if k < 8 else "wash"
                a = agg[ph]
                rows = [json.loads(l) for l in (block / "exposure.jsonl").read_text().splitlines() if l.strip()]
                per_iteration: dict = {}
                for r in rows:
                    if r["kind"] != "train_exposure":
                        raise ValueError(f"{block}: {r}")
                    it = Path(r["file"]).parts[-4]
                    pi = per_iteration.setdefault(it, [0, 0])
                    pi[0] += r["learner_rows"]
                    pi[1] += r["learner_physical_decisions"]
                    a["episodes"] += 1
                    a["returns"] += r["reward"]
                    a["suffix_rows"] += r["suffix_rows"]
                    a["learner_rows"] += r["learner_rows"]
                    if r["first_learner_value"] is not None:
                        a["value_abs_error"] += abs(r["reward"] - max(-1.0, min(1.0, r["first_learner_value"])))
                    spy = r["learner_deck"].startswith("Spy/")
                    if r["reset"]:
                        a["reset"] += 1
                        if r["first_suffix_step"] != r["reset"]["cut_step"]:
                            a["prefix_rows_in_episodes"] += 1
                        src = r["reset"]["guide"].rsplit("/c", 1)[0]
                        cut = r["reset"]["guide"]
                        a["repeated_cuts"] += cut in a["unique_cuts"]
                        a["unique_cuts"].add(cut)
                        a["unique_sources"].add(src)
                        deck = r["learner_deck"].split("/")[0]
                        a["by_deck_reset"][deck] = a["by_deck_reset"].get(deck, 0) + 1
                        if spy:
                            a["spy_reset"] += 1
                            st = str(r["inherited"]["stage"])
                            a["inherited_stage"][st] = a["inherited_stage"].get(st, 0) + 1
                            for e in EVENTS:
                                a["fresh"][e] += bool(r["fresh"][e])
                            a["fresh_full_sequence"] += r["fresh_full_sequence"]
                            a["remaining_sequence_completed"] += r["remaining_sequence_completed"]
                            a["inherited_complete"] += r["inherited_complete"]
                            a["spy_reset_wdl"][{1: 0, 0: 1, -1: 2}[r["reward"]]] += 1
                    elif spy:
                        a["spy_ordinary"] += 1
                        a["spy_ordinary_fresh_full_sequence"] += r["fresh_full_sequence"]
                for u in json.loads((block / "updates.json").read_text()):
                    a["updates"] += 1
                    got = per_iteration.get(u["iteration"], [None, None])
                    if got != [u["policy_substeps"], u["physical_decisions"]]:
                        a["update_mismatches"] += 1
            for ph, a in agg.items():
                for key in ("unique_sources", "unique_cuts"):
                    a[key] = len(a[key])
                n = max(a["episodes"], 1)
                a["mean_return"] = a.pop("returns") / n
                a["mean_suffix_rows"] = a["suffix_rows"] / n
                a["mean_value_abs_error"] = a.pop("value_abs_error") / n
            out[name] = agg
    return out


# ---- panel (RUNNER section 4) ----

def load(index: dict) -> dict:
    data = {}
    expected = {(o, t, k, s) for o in range(DECKS) for t in range(DECKS) for k in range(REPEATS) for s in (0, 1)}
    seeds = {(c["own"], c["other"], c["repeat"]): c["seed"] for c in practice.eval_cases()}
    for p in index["panels"]:
        key = (p["checkpoint"], p["opponent"])
        if key in data:
            raise ValueError(f"duplicate panel {key}")
        name = f"{key[0]}-vs-{key[1]}"
        cases = {}
        for line in Path(p["rows"]).read_text().splitlines():
            if line.strip():
                r = json.loads(line)
                ck = (r["own"], r["other"], r["repeat"], r["seat"])
                if ck not in expected or ck in cases or r["panel"] != name or r["seed"] != seeds[ck[:3]]:
                    raise ValueError(f"{p['rows']}: unknown, duplicate or foreign case {ck}")
                cases[ck] = r
        if cases.keys() != expected:
            raise ValueError(f"{p['rows']}: case roster differs")
        labels = {}
        for line in Path(p["labels"]).read_text().splitlines():
            if line.strip():
                r = json.loads(line)
                labels[r["id"]] = r
        for (o, t, k, s), case in cases.items():
            lab = labels.get(f"{name}/o{o}-t{t}-k{k}-s{s}")
            if o == SPY and case["complete"]:
                if lab is None or lab.get("kind") != "label_replay":
                    raise ValueError(f"{p['labels']}: missing label o{o}-t{t}-k{k}-s{s}")
                if case["score"] != (0.5 if lab["winner"] is None else float(lab["winner"] == s)):
                    raise ValueError(f"{p['labels']}: label winner differs from the row")
            case["labels"] = lab
        data[key] = cases
    return data


def block_stats(cases: dict, bound: str) -> dict:
    """(own, other, repeat) -> [score sum, scored cases, J, C, self, DR] over both seats."""
    out = {}
    for (o, t, k, s), c in cases.items():
        b = out.setdefault((o, t, k), [0.0, 0, 0, 0, 0, 0])
        if c["complete"]:
            v = c["score"]
        elif bound == "observed":
            continue
        else:
            v = 0.0 if bound == "worst" else 1.0
        b[0] += v
        b[1] += 1
        if o == SPY and c["complete"]:
            lab = c["labels"] or {}
            labels = lab.get("labels", {})
            b[2] += bool(lab.get("combo_won"))
            b[3] += bool(lab.get("combo_resolved"))
            b[4] += bool(labels.get("self_chosen"))
            b[5] += bool(labels.get("dr_giant_chosen"))
    return out


def metrics(stats: dict, blocks: dict) -> dict:
    """blocks: (own, other) -> repeat indices (a bootstrap resample). Spy rates use the
    fixed unconditional denominator (all scheduled Spy cases in the resample)."""
    deck, spy = [], [0, 0, 0, 0]
    for o in range(DECKS):
        tot, n = 0.0, 0
        for t in range(DECKS):
            for k in blocks[(o, t)]:
                b = stats[(o, t, k)]
                tot += b[0]
                n += b[1]
                if o == SPY:
                    for i in range(4):
                        spy[i] += b[2 + i]
        deck.append(tot / n if n else float("nan"))
    n_spy = DECKS * REPEATS * 2
    return {"deck": deck, "nine": sum(deck) / DECKS, "spy_score": deck[SPY], "J": spy[0] / n_spy,
            "C": spy[1] / n_spy, "self_chosen": spy[2] / n_spy, "dr_giant_chosen": spy[3] / n_spy}


def contrasts(stats: dict, blocks: dict, stats_y: dict | None = None) -> dict:
    """First-minus-second contrasts. With ``stats_y`` the second term uses those statistics
    (missing-data bounds: lower = worst first minus best second, upper = the reverse)."""
    cache = {key: metrics(st, blocks) for key, st in stats.items()}
    cache_y = cache if stats_y is None else {key: metrics(st, blocks) for key, st in stats_y.items()}
    out = {}
    for lineage in LINEAGES:
        for ref in REFS:
            def get(ck):
                return cache.get((ck, ref))

            def get_y(ck):
                return cache_y.get((ck, ref))
            for end in ENDS:
                pairs = [("M-U", "M", "U"), ("M-O", "M", "O"), ("U-O", "U", "O")]
                for name, a, b in pairs:
                    x, y = get(f"{lineage}-{a}-{end}"), get_y(f"{lineage}-{b}-{end}")
                    if x and y:
                        out[f"{lineage}/{ref}/{end}/{name}"] = {k: x[k] - y[k] for k in
                                                                ("nine", "spy_score", "J", "C", "self_chosen", "dr_giant_chosen")}
                        out[f"{lineage}/{ref}/{end}/{name}"]["deck"] = [p - q for p, q in zip(x["deck"], y["deck"])]
                for arm in ARMS:
                    x, y = get(f"{lineage}-{arm}-{end}"), get_y(f"{lineage}-parent")
                    if x and y:
                        out[f"{lineage}/{ref}/{end}/{arm}-parent"] = {"nine": x["nine"] - y["nine"],
                                                                      "deck": [p - q for p, q in zip(x["deck"], y["deck"])]}
    for end in ENDS:
        for name in ("M-U", "M-O", "U-O"):
            parts = [out.get(f"{l}/{r}/{end}/{name}") for l in LINEAGES for r in REFS]
            if all(parts):
                out[f"aggregate/{end}/{name}"] = {k: sum(p[k] for p in parts) / len(parts)
                                                  for k in ("nine", "spy_score", "J", "C")}
    return out, cache


def bootstrap(stats: dict) -> dict:
    rng = random.Random(BOOT_SEED)
    draws: dict = {}
    for _ in range(DRAWS):
        blocks = {(o, t): [rng.randrange(REPEATS) for _ in range(REPEATS)] for o in range(DECKS) for t in range(DECKS)}
        c, _ = contrasts(stats, blocks)
        for key, v in c.items():
            for k, x in v.items():
                if k == "deck":
                    for d, y in enumerate(x):
                        draws.setdefault((key, f"deck{d}"), []).append(y)
                else:
                    draws.setdefault((key, k), []).append(x)
    out = {}
    for (key, k), xs in draws.items():
        xs.sort()
        out.setdefault(key, {})[k] = [xs[int(0.025 * len(xs))], xs[int(0.975 * len(xs)) - 1]]
    return out


def retention(point: dict, ci: dict) -> dict:
    out = {}
    for key, v in point.items():
        if key.startswith("aggregate") or not key.endswith(("-parent", "-O")) or "deck" not in v:
            continue
        if key.endswith("M-U"):
            continue
        for d in range(DECKS):
            if d == SPY:
                continue
            lo, hi = ci.get(key, {}).get(f"deck{d}", [None, None])
            out[f"{key}/deck{d}"] = {"diff": v["deck"][d], "ci95": [lo, hi],
                                     "unresolved_risk": lo is not None and lo < -RETENTION,
                                     "material_harm": hi is not None and hi < -RETENTION}
    return out


FEW_FRESH_SELF_TARGET = 10  # fresh self-target choices across U and M main phases, both lineages


def disposition(point: dict, ci: dict, ret: dict, exposure_summary: dict | None, lower: dict | None,
                panels: dict) -> list[str]:
    """The fixed descriptive dispositions (RUNNER section 4). ``lower`` holds the opposite-arm
    missing-data lower bounds (None when no case is missing)."""
    notes = []
    fresh_full = None
    if exposure_summary is not None:
        main = [exposure_summary[f"{l}-{a}"]["main"] for l in LINEAGES for a in ("U", "M")]
        fresh_self = sum(m["fresh"]["self_chosen"] for m in main)
        fresh_full = sum(m["fresh_full_sequence"] + m["remaining_sequence_completed"] for m in main)
        if fresh_self < FEW_FRESH_SELF_TARGET or fresh_full == 0:
            notes.append(f"Few/no fresh opportunities or successes ({fresh_self} fresh self-target choices, "
                         f"{fresh_full} fresh completions in U/M practice): exposure or exploration remains "
                         "unresolved; reset learning is not disproven.")
    final_j = sum(panels[(f"{l}-{a}-final", r)]["J"] for l in LINEAGES for a in ("U", "M") for r in REFS
                  if (f"{l}-{a}-final", r) in panels)
    if fresh_full and final_j == 0:
        notes.append("Fresh practice completed the sequence but ordinary-start U/M endpoints never did (J = 0): "
                     "local rehearsal without demonstrated transfer.")
    agg = point.get("aggregate/final/M-U")
    agg_ci = ci.get("aggregate/final/M-U", {}).get("nine")
    parts = [point.get(f"{l}/{r}/final/M-U", {}).get("nine") for l in LINEAGES for r in REFS]
    bound_ok = lower is None or (lower.get("aggregate/final/M-U", {}).get("nine", -1) > 0)
    judged = [v for k, v in ret.items() if "/final/M-parent/" in k or "/final/M-O/" in k]
    unresolved = any(v["unresolved_risk"] for v in judged)
    harm = any(v["material_harm"] for v in judged)
    if agg and agg_ci and agg_ci[0] > 0 and all(p is not None and p > 0 for p in parts) and bound_ok:
        if unresolved or harm:
            notes.append("M improves over U after ordinary-only training with compatible directions and a positive "
                         "aggregate interval, with unresolved retention for the final M endpoint: a priority "
                         "benefit, not an overall improvement endorsement.")
        else:
            notes.append("M improves over U after ordinary-only training with compatible directions and a positive "
                         "aggregate interval: evidence for this priority rule on this bank, not confirmation.")
    else:
        notes.append("M-U uncertain or negative: no demonstrated priority advantage at this dose.")
    main_ci = ci.get("aggregate/main/M-U", {}).get("nine")
    if main_ci and main_ci[0] > 0 and not (agg_ci and agg_ci[0] > 0):
        notes.append("Main endpoint improves but the ordinary-only endpoint does not keep it: persistence problem.")
    if harm:
        notes.append("Material harm (upper bound below -0.02) in at least one non-Spy deck for the final M endpoint.")
    return notes


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    sub = ap.add_subparsers(dest="op", required=True)
    e = sub.add_parser("exposure")
    e.add_argument("--root", type=Path, required=True)
    e.add_argument("--out", type=Path, required=True)
    p = sub.add_parser("panel")
    p.add_argument("--index", type=Path, required=True)
    p.add_argument("--exposure", type=Path)
    p.add_argument("--out", type=Path, required=True)
    p.add_argument("--no-bootstrap", action="store_true")
    a = ap.parse_args(argv)
    if a.op == "exposure":
        res = exposure(a.root)
        a.out.write_text(json.dumps(res, indent=1) + "\n")
        print(json.dumps({k: {ph: {x: v[ph][x] for x in ("episodes", "spy_reset", "fresh_full_sequence",
                                                           "update_mismatches", "prefix_rows_in_episodes")}
                              for ph in v} for k, v in res.items()}))
        return 0
    data = load(json.loads(a.index.read_text()))
    full = {(o, t): list(range(REPEATS)) for o in range(DECKS) for t in range(DECKS)}
    stats = {key: block_stats(cases, "observed") for key, cases in data.items()}
    point, panels = contrasts(stats, full)
    missing = sum(not c["complete"] for cases in data.values() for c in cases.values())
    bounds = {}
    if missing:
        worst = {key: block_stats(cases, "worst") for key, cases in data.items()}
        best = {key: block_stats(cases, "best") for key, cases in data.items()}
        bounds["lower"], _ = contrasts(worst, full, best)
        bounds["upper"], _ = contrasts(best, full, worst)
    ci = {} if a.no_bootstrap else bootstrap(stats)
    ret = retention(point, ci)
    exp = json.loads(a.exposure.read_text()) if a.exposure else None
    result = {"panels": {f"{k[0]}/{k[1]}": v for k, v in panels.items()}, "contrasts": point, "ci95": ci,
              "missing_cases": missing, "bounds": bounds, "retention": ret,
              "disposition": disposition(point, ci, ret, exp, bounds.get("lower"), panels)}
    a.out.write_text(json.dumps(result, indent=1) + "\n")
    print(json.dumps({"primary": {k: v for k, v in point.items() if "final/M-U" in k},
                      "disposition": result["disposition"]}))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
