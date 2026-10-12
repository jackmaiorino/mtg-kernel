"""Held-out comparison of the search scheduler arms (collab
LANES/discovery-efficiency-plan-20261012, RUNNER.md section 6). Frozen before outcomes.

CLI: allocator_report.py --schedule DIR --rows DIR [DIR ...] --weights ENDPOINT.json
                         [--ledger FILE ...] [--feature-rows DIR ...] --out RESULT.json

Rows are ``s4a-allocator`` outputs named ``h-<lineage>-<index>.jsonl``, where index is the
line of ``heldout-<lineage>.jsonl``. Game score: win 1, draw 0.5, loss 0. A missing,
failed or non-natural game is unknown, never a loss: every mean is reported with its
worst (unknown = 0) and best (unknown = 1) bounds, and contrasts with their opposite-arm
bounds.

Primary: learned minus random equal-weight nine-deck score (mean of the nine own-deck
means) per lineage, and their equal-weight aggregate; point estimates only (no nine-deck
interval: eight strata hold one seat-pair unit). Current is the secondary comparator.
Spy is reported separately, with a 10,000-draw paired bootstrap over its nine seat-pair
units per lineage (conditional on this panel). Costs, scheduler diagnostics and, on the
logged feature rows, endpoint-versus-q total variation and expected requested allocation.
"""
from __future__ import annotations

import argparse
import importlib.util
import json
import random
from collections import Counter, defaultdict
from pathlib import Path

_spec = importlib.util.spec_from_file_location("allocator", Path(__file__).with_name("allocator.py"))
alloc = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(alloc)

SCORE = {"win": 1.0, "draw": 0.5, "loss": 0.0}
BOOTSTRAP_DRAWS = 10_000


def status(row):
    if row is None:
        return "unstarted"
    if row.get("kind") != "allocator_game":
        return "failed"
    g = row["game"]
    if not g.get("valid"):
        return "failed_search" if "failed search" in str(g.get("invalid_reason")) else "non_natural"
    return "natural"


def score(row):
    return SCORE[row["game"]["end"]] if status(row) == "natural" else None


def mean(xs):
    return sum(xs) / len(xs) if xs else None


def bounded(values):
    """Mean with unknowns at 0 and at 1."""
    lo = [0.0 if v is None else v for v in values]
    hi = [1.0 if v is None else v for v in values]
    return {"point": mean([v for v in values if v is not None]), "worst": mean(lo), "best": mean(hi),
            "n": len(values), "unknown": sum(v is None for v in values)}


def macro(per_deck, key):
    vals = [per_deck[d][key] for d in range(9)]
    return None if any(v is None for v in vals) else sum(vals) / 9


def arm_summary(games):
    """games: {own: [score or None]} for one arm and lineage."""
    per_deck = {d: bounded(games.get(d, [])) for d in range(9)}
    return {"decks": per_deck, "nine_deck": {k: macro(per_deck, k) for k in ("point", "worst", "best")}}


def contrast(a, b):
    """a - b for nine-deck summaries, with the bounds that favor each side."""
    pa, pb = a["nine_deck"], b["nine_deck"]
    sub = lambda x, y: None if x is None or y is None else x - y  # noqa: E731
    return {"point": sub(pa["point"], pb["point"]), "lower": sub(pa["worst"], pb["best"]),
            "upper": sub(pa["best"], pb["worst"])}


def spy_bootstrap(units, seed):
    """Paired seat-pair bootstrap of learned-random (and learned-current, random-current)
    Spy score over complete units; returns percentile 95% intervals."""
    rng = random.Random(seed)
    out = {}
    for a, b in (("learned", "random"), ("learned", "current"), ("random", "current")):
        diffs = [mean(u[a]) - mean(u[b]) for u in units if None not in u[a] + u[b]]
        if not diffs:
            out[f"{a}-{b}"] = None
            continue
        draws = sorted(mean([diffs[rng.randrange(len(diffs))] for _ in diffs]) for _ in range(BOOTSTRAP_DRAWS))
        out[f"{a}-{b}"] = {"point": mean(diffs), "lo95": draws[int(0.025 * BOOTSTRAP_DRAWS)],
                           "hi95": draws[int(0.975 * BOOTSTRAP_DRAWS) - 1], "complete_units": len(diffs),
                           "units": len(units)}
    return out


def game_cost(row):
    g = row["game"]
    dec = g.get("decisions", [])
    searches = g.get("searches", [])
    choosers = Counter(d["chooser"] for d in dec)
    sched = [d["scheduler"] for d in dec if d.get("scheduler")]
    exhausted = next((i for i, d in enumerate(dec) if d.get("remaining_after") == 0), None)
    return {"requested": sum(s["budget"] for s in searches), "spent": g.get("transitions_used", 0),
            "remaining": row["cap"] - g.get("transitions_used", 0), "searches": len(searches),
            "natural_simulations": sum(s.get("completed_natural", 0) for s in searches),
            "search_reasons": Counter(s["reason"] for s in searches),
            "scheduler_decisions": len(sched), "skips": sum(1 for s in sched if s["amount"] == 0),
            "chosen_amounts": Counter(str(s["amount"]) for s in sched),
            "decisions": len(dec), "supported": choosers["new_root"] + choosers["reused"],
            "plain": choosers["plain"], "exhausted": int(exhausted is not None),
            "first_exhausted_decision": exhausted,
            "real_transitions": g.get("real_transitions"), "wall": row.get("wall"),
            "controller_seconds": (row.get("scheduler") or {}).get("controller_seconds", 0.0)}


def add(acc, c):
    for k, v in c.items():
        if k == "first_exhausted_decision":
            continue
        if isinstance(v, Counter):
            acc.setdefault(k, Counter()).update(v)
        elif isinstance(v, (int, float)) and not isinstance(v, bool):
            acc[k] = acc.get(k, 0) + v
    acc["games"] = acc.get("games", 0) + 1


def coalesced(p, remaining):
    return [sum(p[a] for a in members) for _, members in alloc.choices(remaining)]


def feature_diagnostics(rows, w):
    """Endpoint pi and behavior mu versus q on logged lawful feature rows."""
    tv_pi, tv_mu, req_pi, req_mu, req_q, n = 0.0, 0.0, 0.0, 0.0, 0.0, 0
    for row in rows:
        for d in row["game"].get("decisions", []):
            s = d.get("scheduler")
            if not s:
                continue
            amounts = [c[0] for c in alloc.choices(s["remaining"])]
            pi = alloc.pi_of(w, s["x"])
            pc, mc, qc = (coalesced(p, s["remaining"]) for p in (pi, alloc.mixture(pi), alloc.Q))
            tv_pi += 0.5 * sum(abs(x - y) for x, y in zip(pc, qc))
            tv_mu += 0.5 * sum(abs(x - y) for x, y in zip(mc, qc))
            req_pi += sum(a * p for a, p in zip(amounts, pc))
            req_mu += sum(a * p for a, p in zip(amounts, mc))
            req_q += sum(a * p for a, p in zip(amounts, qc))
            n += 1
    if not n:
        return {"rows": 0}
    return {"rows": n, "mean_tv_pi_q": tv_pi / n, "mean_tv_mu_q": tv_mu / n,
            "mean_expected_request_pi": req_pi / n, "mean_expected_request_mu": req_mu / n,
            "mean_expected_request_q": req_q / n}


def load_dirs(dirs, prefix):
    out = {}
    for d in dirs:
        for p in sorted(Path(d).glob(f"{prefix}*.jsonl")):
            if ".failed" in p.name:
                continue
            row = alloc.read_row(p)
            if row is not None:
                out[p.stem] = row
    return out


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--schedule", type=Path, required=True)
    ap.add_argument("--rows", nargs="+", required=True)
    ap.add_argument("--weights", type=Path, required=True)
    ap.add_argument("--ledger", nargs="*", default=[])
    ap.add_argument("--feature-rows", nargs="*", default=[], help="training row dirs (t*.jsonl)")
    ap.add_argument("--out", type=Path, required=True)
    a = ap.parse_args(argv)
    w = alloc.read_weights(a.weights.read_bytes())["weights"]
    rows = load_dirs(a.rows, "h-")
    result = {"schema": "discovery-efficiency-v1/allocator-result", "weights_sha256": alloc.sha(a.weights.read_bytes()),
              "statuses": Counter(), "lineages": {}, "aggregate": {}, "costs": {}, "feature_diagnostics": {}}
    summaries = {}
    costs = defaultdict(dict)
    for li, lin in enumerate(alloc.LINEAGES):
        cases = [json.loads(l) for l in (a.schedule / f"heldout-{lin}.jsonl").read_text().splitlines() if l.strip()]
        games = defaultdict(lambda: defaultdict(list))
        units = defaultdict(lambda: {arm: [] for arm in alloc.ARMS})
        unit_counts, per_game = [], {}
        for i, c in enumerate(cases):
            row = rows.get(f"h-{lin}-{i}")
            if row is not None and (row.get("id") not in (None, c["id"]) or row.get("arm", c["arm"]) != c["arm"]):
                raise ValueError(f"h-{lin}-{i}: row {row.get('id')} is not case {c['id']}")
            st = status(row)
            result["statuses"][f"{c['arm']}:{st}"] += 1
            sc = score(row)
            own, other = c["case"]["own"], c["case"]["other"]
            games[c["arm"]][own].append(sc)
            units[(own, other)][c["arm"]].append(sc)
            if st == "natural":
                gc = game_cost(row)
                add(costs[lin].setdefault(c["arm"], {}), gc)
                per_game[c["id"]] = gc
        for (own, other), u in sorted(units.items()):
            unit_counts.append({"own": own, "other": other,
                                **{arm: dict(Counter("unknown" if v is None else {1.0: "W", 0.5: "D", 0.0: "L"}[v]
                                                     for v in u[arm])) for arm in alloc.ARMS}})
        summ = {arm: arm_summary(games[arm]) for arm in alloc.ARMS}
        summaries[lin] = summ
        spy_units = [u for (own, _), u in sorted(units.items()) if own == alloc.SPY]
        result["lineages"][lin] = {
            "arms": summ,
            "learned_minus_random": contrast(summ["learned"], summ["random"]),
            "learned_minus_current": contrast(summ["learned"], summ["current"]),
            "random_minus_current": contrast(summ["random"], summ["current"]),
            "spy": {arm: summ[arm]["decks"][alloc.SPY] for arm in alloc.ARMS},
            "spy_bootstrap": spy_bootstrap(spy_units, alloc.seed(alloc.NS, "analysis", "spy-bootstrap", lin)),
            "units": unit_counts, "games": per_game}
    for name in ("learned_minus_random", "learned_minus_current", "random_minus_current"):
        vals = [result["lineages"][lin][name] for lin in alloc.LINEAGES]
        result["aggregate"][name] = {k: (None if any(v[k] is None for v in vals) else mean([v[k] for v in vals]))
                                     for k in ("point", "lower", "upper")}
    result["aggregate"]["spy"] = {arm: mean([summaries[lin][arm]["decks"][alloc.SPY]["point"] or 0.0
                                             for lin in alloc.LINEAGES]) for arm in alloc.ARMS}
    for per_arm in costs.values():
        for acc in per_arm.values():
            acc["supported_fraction"] = acc.get("supported", 0) / max(1, acc.get("decisions", 0))
            acc["per_game"] = {k: acc[k] / acc["games"] for k in ("requested", "spent", "searches",
                                                                    "scheduler_decisions", "skips", "wall")
                               if k in acc}
    result["costs"] = costs
    ledger = []
    for p in a.ledger:
        ledger += [json.loads(l) for l in Path(p).read_text().splitlines() if l.strip()]
    cpu = defaultdict(float)
    for r in ledger:
        if r.get("kind", "process") == "process":
            cpu[r.get("phase", "?")] += r.get("cpu_seconds", 0.0)
    result["cpu_seconds_by_phase"] = cpu
    heldout_sched = [r for r in rows.values() if r.get("arm") in ("random", "learned")]
    result["feature_diagnostics"]["heldout"] = feature_diagnostics(heldout_sched, w)
    if a.feature_rows:
        result["feature_diagnostics"]["training"] = feature_diagnostics(load_dirs(a.feature_rows, "t").values(), w)
    a.out.write_text(json.dumps(result, indent=1, default=lambda o: dict(o)) + "\n")
    for lin in alloc.LINEAGES:
        print(lin, "learned-random", result["lineages"][lin]["learned_minus_random"])
    print("aggregate", result["aggregate"]["learned_minus_random"], "statuses", dict(result["statuses"]))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
