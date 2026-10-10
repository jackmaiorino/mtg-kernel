"""Analyze Spy execution diagnosis traces (RUNNER.md section 5).

Usage: python analyze.py --panel PANEL.json --out DIR TRACE.jsonl [...]
Reads only passive trace sidecars written by `s4a-diag` with S4A_TRACE.
Writes DIR/analysis.json (per root, per world, aggregates by panel stratum).
Labels are descriptive only.

Action bits: 1 cast Spy, 2 Spy targets focal (self-target), 4 Dread Return
targets Lotleth Giant, 8 Spy targets another, 16 DR targets something else.
Event bits: 1 Spy resolved, 2 self-target resolved, 4 DR->Giant resolved,
8 DR->Giant on the stack. Depth = focal non-forced physical decisions from
the root (the tree's depth unit; MAX_DEPTH 32).
"""
import argparse
import json
from collections import Counter, defaultdict
from pathlib import Path
from panel_coverage import expected_roots, require_coverage

CAST, SELF, GIANT = 1, 2, 4
MIN = 8
STEPS = ((CAST, "cast", 1), (SELF, "self_target", 2), (GIANT, "dr_giant", 4))


def load(path):
    meta, sims, nodes, worlds = None, [], {}, []
    with open(path, encoding="utf-8") as f:
        for line in f:
            r = json.loads(line)
            k = r["r"]
            if k == "meta":
                meta = r
            elif k == "sel":
                sims.append(r)
            elif k == "node":
                nodes[r["id"]] = r
            elif k == "eval":
                worlds.append(r)
    return meta, sims, nodes, worlds


def ordered_stage(evs, cast):
    """Suffix stage reached, as labels.rs advances it (one step per transition)."""
    stage = 0 if cast else 1
    for _, _, b in evs:
        if stage == 0 and b & 1:
            stage = 1
        elif stage == 1 and b & 2:
            stage = 2
        elif stage == 2 and b & 4:
            stage = 3
    return stage


def edge_returns(sims):
    """Ordered natural terminal returns per (node id, edge index)."""
    hist = defaultdict(list)
    for s in sims:
        if s.get("end") not in ("win", "loss"):
            continue
        w = 1 if s["end"] == "win" else 0
        for p in s["path"]:
            hist[(p[0], p[1])].append(w)
    return hist


def history_summary(h):
    n = len(h)
    out = {"n": n, "wins": sum(h), "mean": round(sum(h) / n, 4) if n else None}
    half = n // 2
    if half >= MIN:
        out["early_mean"] = round(sum(h[:half]) / half, 4)
        out["late_mean"] = round(sum(h[half:]) / (n - half), 4)
    else:
        out["halves"] = "fewer than 8 returns per half"
    if n:
        marks, cum = [], 0
        points = {max(1, round(n * q / 10)) for q in range(1, 11)}
        for i, w in enumerate(h, 1):
            cum += w
            if i in points:
                marks.append([i, round(cum / i, 4)])
        out["cumulative"] = marks
    return out


def compat_verdict(node, bit, fc, hist, nid):
    """Why a compatible edge was or was not the frozen choice at a matched node."""
    rank = {e: i for i, e in enumerate(node["perm"])}
    rows = [{"edge": e, "lab": node["lab"][e], "n": n, "wins": w, "mean": round(w / n, 4) if n else None,
             "qualified": n >= MIN, "tie_rank": rank[e]} for e, (n, w) in enumerate(zip(node["n"], node["w"]))]
    comp = [r for r in rows if r["lab"] & bit]
    out = {"node_edges": len(rows), "qualified_edges": sum(r["qualified"] for r in rows)}
    if not comp:
        out["verdict"] = "no compatible edge at node"
        return out
    for r in comp:
        r["returns"] = history_summary(hist.get((nid, r["edge"]), []))
    out["compatible"] = comp
    fcr = next((r for r in rows if r["edge"] == fc), None)
    if fcr is not None:
        out["frozen_choice"] = dict(fcr, returns=history_summary(hist.get((nid, fc), [])))
    if any(r["edge"] == fc for r in comp):
        out["verdict"] = "compatible edge is the frozen choice"
    elif not any(r["qualified"] for r in comp):
        out["verdict"] = ("compatible edge lacks support, no qualified edge at node" if fcr is None
                          else "compatible edge lacks support, another edge qualified")
    else:
        # Match Node::frozen_choice's unrounded ratio. Rounded display values
        # cannot distinguish a genuine mean loss from a seeded tie.
        best = max((r for r in comp if r["qualified"]), key=lambda r: r["wins"] / r["n"])
        out["verdict"] = ("qualified compatible edge loses on mean" if best["wins"] / best["n"] < fcr["wins"] / fcr["n"]
                          else "qualified compatible edge ties, loses seeded order")
    return out


def eval_world(w, nodes, hist, cast):
    decs, evs = w["dec"], w["ev"]
    stage = ordered_stage(evs, cast)
    srcs = Counter(d["src"] for d in decs)
    first_miss = next((d for d in decs if d["src"] == "first_miss_plain"), None)
    out = {"world": w["world"], "end": w["out"]["end"], "j": w["out"]["j"], "suffix_stage": stage,
           "focal_nonforced": len(decs), "forced": w["forced"], "opponent": w["opp"], "sources": dict(srcs),
           "attempted_lookups": sum(1 for d in decs if d["src"] in
                                    ("tree_edge", "matched_no_qualified_plain", "first_miss_plain")),
           "matched": sum(1 for d in decs if d["src"] in ("tree_edge", "matched_no_qualified_plain")),
           "first_miss_depth": first_miss["dep"] if first_miss else None,
           "deepest_match": max((d["dep"] for d in decs if d["node"] is not None), default=0),
           "steps": {}, "opportunities": [], "descending_run": descending_run(decs)}
    st = next((t for t, _, b in evs if b & 2), None)
    giant = next((d for d in decs if d["off"] & GIANT), None)
    out["focal_decisions_self_resolution_to_giant_offer"] = (
        sum(1 for d in decs if st is not None and st <= d["t"] < giant["t"]) if giant and st is not None else None)
    for bit, name, need in STEPS:
        offers = [d for d in decs if d["off"] & bit]
        chosen = [d for d in offers if d["ch"] & bit]
        resolved = any(b & {1: 1, 2: 2, 4: 4}[bit] for _, _, b in evs)
        if not offers:
            status = "never offered"
        elif not chosen:
            status = "declined"
        elif not resolved:
            status = "chosen, not resolved"
        else:
            status = "resolved"
        out["steps"][name] = {"status": status, "offers": len(offers), "chosen": len(chosen),
                              "first_offer_depth": offers[0]["dep"] if offers else None,
                              "offer_sources": dict(Counter(d["src"] for d in offers)),
                              "chosen_sources": dict(Counter(d["src"] for d in chosen))}
    for d in decs:
        for bit, name, _ in STEPS[1:]:
            if not d["off"] & bit:
                continue
            o = {"kind": name, "t": d["t"], "pd": d["pd"], "ss": d["ss"], "dep": d["dep"], "src": d["src"],
                 "chosen": bool(d["ch"] & bit), "chosen_bits": d["ch"], "node": d["node"], "edge": d["edge"]}
            if isinstance(d["node"], int):
                o["at_node"] = compat_verdict(nodes[d["node"]], bit, d.get("fc"), hist, d["node"])
            else:
                o["stats"] = None
                o["stats_reason"] = {"after_miss_plain": "matching ended at an earlier miss",
                                     "first_miss_plain": "this history is not in the tree (first miss)",
                                     "depth_limit_plain": "beyond the depth limit"}.get(d["src"], d["src"])
            out["opportunities"].append(o)
    return out


def selection_summary(sims, nodes, cast):
    ends = Counter(s.get("end") for s in sims)
    natural = [s for s in sims if s.get("end") in ("win", "loss")]
    complete = [s for s in natural if ordered_stage(s.get("ev", []), cast) == 3]
    oc = Counter()
    giant_depths, self_depths, tail_depths = Counter(), Counter(), Counter()
    for s in natural:
        tail_depths[(s.get("tail") or {}).get("dep")] += 1
        for d in s.get("dec", []):
            for bit, name, _ in STEPS:
                if d["off"] & bit:
                    oc[f"{name}|offered|{d['src']}"] += 1
                    if d["ch"] & bit:
                        oc[f"{name}|chosen|{d['src']}"] += 1
            if d["off"] & GIANT:
                giant_depths[d["dep"]] += 1
            if d["off"] & SELF:
                self_depths[d["dep"]] += 1
    init = {name: Counter() for _, name, _ in STEPS}
    for s in complete:
        for d in s.get("dec", []):
            for bit, name, _ in STEPS:
                if d["ch"] & bit:
                    init[name][d["src"]] += 1
    rates = {}
    for _, name, _ in STEPS:
        for src in ("tree", "expand", "tail_after_expand", "tail_depth"):
            off = oc.get(f"{name}|offered|{src}", 0)
            if off:
                rates[f"{name}|{src}"] = {"offered": off, "chosen": oc.get(f"{name}|chosen|{src}", 0),
                                          "rate": round(oc.get(f"{name}|chosen|{src}", 0) / off, 4)}

    def q(c):
        xs = sorted(k for k, v in c.items() for _ in range(v) if k is not None)
        return {"n": len(xs), "min": xs[0], "median": xs[len(xs) // 2], "max": xs[-1]} if xs else None

    return {"simulations": dict(ends), "natural": len(natural), "suffix_complete_natural": len(complete),
            "suffix_complete_wins": sum(1 for s in complete if s["end"] == "win"),
            "choice_source_in_complete": {k: dict(v) for k, v in init.items()},
            "choice_rates_by_source": rates,
            "self_offer_depth": q(self_depths), "giant_offer_depth": q(giant_depths),
            "tail_start_depth": q(tail_depths),
            "tree_node_depth": q(Counter(n["dep"] for n in nodes.values()))}


def selection_kind(node):
    """A forced-complete selection node: every edge selects one more object
    for the same effect and the effect must select them all (min = max)."""
    kinds = set()
    for e in node["edges"]:
        try:
            x = json.loads(e)
        except ValueError:
            return None
        if x.get("action_kind") != "choose_effect_target":
            return None
        kinds.add((x["source"]["card_db_id"], x.get("min_targets"), x.get("max_targets")))
    if len(kinds) == 1:
        src, lo, hi = kinds.pop()
        if lo == hi:
            return src
    return None


def tree_composition(nodes):
    sel = Counter()
    for n in nodes.values():
        k = selection_kind(n)
        if k is not None:
            sel[k] += 1
    return {"nodes": len(nodes), "forced_complete_selection_nodes": sum(sel.values()),
            "by_source_card": dict(sel)}


def descending_run(decs):
    """Longest run of consecutive focal decisions whose menu shrinks by one
    each time (the shape of a forced-complete selection)."""
    best, cur, start = (0, None), 1, 0
    for i in range(1, len(decs)):
        if decs[i]["k"] == decs[i - 1]["k"] - 1 and decs[i]["k"] >= 2:
            cur += 1
        else:
            cur, start = 1, i
        if cur > best[0]:
            best = (cur, decs[start]["dep"])
    return {"length": best[0], "start_depth": best[1]}


def root_edges(nodes, hist):
    root = next((n for n in nodes.values() if n["parent"] is None), None)
    if root is None:
        return None
    rows = []
    for e, (n, w) in enumerate(zip(root["n"], root["w"])):
        if n:
            rows.append({"edge": e, "lab": root["lab"][e], "returns": history_summary(hist.get((root["id"], e), []))})
    return rows


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--panel", required=True)
    ap.add_argument("--out", required=True)
    ap.add_argument("--root-id", action="append", help="explicit qualification subset; default requires the full panel")
    ap.add_argument("traces", nargs="+")
    a = ap.parse_args()
    panel = json.loads(Path(a.panel).read_text())
    expected = expected_roots(panel, a.root_id)
    roles = {r["root_id"]: r["role"] for r in panel["roots"]}
    result = {}
    for t in a.traces:
        meta, sims, nodes, worlds = load(t)
        hist = edge_returns(sims)
        rid, cast = meta["root_id"], meta["cast_root"]
        if rid not in expected or rid in result:
            raise ValueError('duplicate or out-of-scope trace root: ' + rid)
        result[rid] = {"role": roles.get(rid), "stratum": meta["stratum"], "opp_model": meta["opp_model"],
                       "nodes": len(nodes), "tree": tree_composition(nodes),
                       "selection": selection_summary(sims, nodes, cast),
                       "root_edges": root_edges(nodes, hist),
                       "worlds": [eval_world(w, nodes, hist, cast) for w in worlds]}
    require_coverage(list(result), expected)
    Path(a.out).mkdir(parents=True, exist_ok=True)
    (Path(a.out) / "analysis.json").write_text(json.dumps(result, indent=1))
    print(json.dumps({rid: {"role": r["role"], "complete": r["selection"]["suffix_complete_natural"],
                            "J": sum(w["j"] for w in r["worlds"])} for rid, r in result.items()}))


if __name__ == "__main__":
    main()
