"""Analyze Spy execution diagnosis traces (RUNNER.md section 5).

Usage: python analyze.py --panel PANEL.json --out DIR TRACE.jsonl [...]
Reads only passive trace sidecars written by `s4a-diag` with S4A_TRACE.
Writes DIR/analysis.json (per root, per world, aggregates) and prints a
short text summary. Labels are descriptive only.

Action bits: 1 cast Spy, 2 Spy targets focal (self-target), 4 Dread Return
targets Lotleth Giant, 8 Spy targets another, 16 DR targets something else.
Event bits: 1 Spy resolved, 2 self-target resolved, 4 DR->Giant resolved,
8 DR->Giant on the stack.
"""
import argparse
import json
from collections import Counter, defaultdict
from pathlib import Path

SELF, GIANT = 2, 4
MIN = 8


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


def edge_returns(sims):
    """Ordered natural terminal returns per (node id, edge index)."""
    hist = defaultdict(list)
    for s in sims:
        if s.get("end") not in ("win", "loss"):
            continue
        w = 1 if s["end"] == "win" else 0
        for p in s["path"]:
            hist[(p[0], p[1])].append((s["i"], w))
    return hist


def history_summary(h):
    n = len(h)
    wins = sum(w for _, w in h)
    out = {"n": n, "wins": wins, "mean": wins / n if n else None}
    half = n // 2
    if half >= MIN:
        a, b = h[:half], h[half:]
        out["early_mean"] = sum(w for _, w in a) / len(a)
        out["late_mean"] = sum(w for _, w in b) / len(b)
        out["halves_n"] = [len(a), len(b)]
    else:
        out["halves"] = "fewer than 8 returns per half"
    # Cumulative mean at each tenth of the history.
    if n:
        cum, marks = 0, []
        for i, (_, w) in enumerate(h, 1):
            cum += w
            if i in {max(1, round(n * q / 10)) for q in range(1, 11)}:
                marks.append([i, round(cum / i, 4)])
        out["cumulative"] = marks
    return out


def node_edge_table(node, hist, frozen_choice, chosen_edge):
    """Every edge at a matched node: counts, mean, qualification, seeded rank."""
    rank = {e: i for i, e in enumerate(node["perm"])}
    rows = []
    for e, (n, w) in enumerate(zip(node["n"], node["w"])):
        rows.append({"edge": e, "lab": node["lab"][e], "n": n, "wins": w,
                     "mean": (w / n) if n else None, "qualified": n >= MIN, "tie_rank": rank[e],
                     "frozen_choice": e == frozen_choice, "chosen": e == chosen_edge})
    return rows


def compat_verdict(rows, bit, frozen_choice):
    """Why a compatible edge (label bit) was or was not the frozen choice."""
    comp = [r for r in rows if r["lab"] & bit]
    if not comp:
        return {"compatible": 0, "verdict": "no compatible edge at node"}
    fc = next((r for r in rows if r["edge"] == frozen_choice), None)
    out = {"compatible": len(comp), "edges": [{k: r[k] for k in ("edge", "n", "wins", "mean", "qualified",
                                                                   "tie_rank")} for r in comp]}
    if any(r["edge"] == frozen_choice for r in comp):
        out["verdict"] = "compatible edge is the frozen choice"
    elif not any(r["qualified"] for r in comp):
        out["verdict"] = "compatible edges lack support (<8 backups)"
    elif fc is None:
        out["verdict"] = "no qualified edge at node"
    else:
        best = max((r for r in comp if r["qualified"]), key=lambda r: r["mean"])
        if best["mean"] < fc["mean"]:
            out["verdict"] = "qualified compatible edge loses on mean"
        else:
            out["verdict"] = "qualified compatible edge ties on mean, loses seeded order"
        out["frozen_choice_edge"] = {k: fc[k] for k in ("edge", "lab", "n", "wins", "mean", "tie_rank")}
    return out


def eval_world(w, nodes, hist):
    decs = w["dec"]
    ev = w["ev"]
    resolved = Counter()
    for t, actor, b in ev:
        for bit, name in ((1, "spy"), (2, "self"), (4, "giant")):
            if b & bit:
                resolved[name] += 1
    srcs = Counter(d["src"] for d in decs)
    out = {"world": w["world"], "end": w["out"]["end"], "j": w["out"]["j"], "focal_nonforced": len(decs),
           "forced": w["forced"], "opponent": w["opp"], "sources": dict(srcs),
           "attempted_lookups": sum(1 for d in decs if d["src"] in
                                    ("tree_edge", "matched_no_qualified_plain", "first_miss_plain")),
           "first_miss_t": next((d["t"] for d in decs if d["src"] == "first_miss_plain"), None),
           "deepest_match": max((d["dep"] for d in decs if d["node"] is not None), default=0),
           "resolved": dict(resolved), "opportunities": []}
    for d in decs:
        for bit, name in ((SELF, "self_target"), (GIANT, "dr_giant")):
            if not d["off"] & bit:
                continue
            opp = {"kind": name, "t": d["t"], "pd": d["pd"], "ss": d["ss"], "dep": d["dep"], "src": d["src"],
                   "chosen": bool(d["ch"] & bit), "chosen_bits": d["ch"], "node": d["node"], "edge": d["edge"]}
            if isinstance(d["node"], int):
                node = nodes[d["node"]]
                rows = node_edge_table(node, hist, d.get("fc"), d["edge"])
                opp["node_edges"] = len(rows)
                opp["compat"] = compat_verdict(rows, bit, d.get("fc"))
                for e in opp["compat"].get("edges", []):
                    e["returns"] = history_summary(hist.get((d["node"], e["edge"]), []))
                fc = d.get("fc")
                if fc is not None and not (node["lab"][fc] & bit):
                    opp["frozen_choice_returns"] = history_summary(hist.get((d["node"], fc), []))
            else:
                opp["stats"] = None
                opp["stats_reason"] = {"after_miss_plain": "matching ended at an earlier miss",
                                       "first_miss_plain": "history not in tree (first miss here)",
                                       "depth_limit_plain": "beyond depth limit"}.get(d["src"], d["src"])
            out["opportunities"].append(opp)
    return out


def selection_summary(sims, meta):
    """Discovered suffixes in selection: where their initiating choices came from."""
    cast = meta["cast_root"]
    ends = Counter(s.get("end") for s in sims)
    complete, src_self, src_giant = 0, Counter(), Counter()
    offered = Counter()
    for s in sims:
        # The resolved suffix in order (labels.rs): Spy (cast roots only),
        # then the self-target, then DR -> Giant.
        stage = 0 if cast else 1
        for _, _, b in s.get("ev", []):
            if stage == 0 and b & 1:
                stage = 1
            elif stage == 1 and b & 2:
                stage = 2
            elif stage == 2 and b & 4:
                stage = 3
        for d in s.get("dec", []):
            for bit, name in ((SELF, "self"), (GIANT, "giant")):
                if d["off"] & bit:
                    offered[f"{name}_offered_{d['src']}"] += 1
                    if d["ch"] & bit:
                        offered[f"{name}_chosen_{d['src']}"] += 1
        if s.get("end") in ("win", "loss") and stage == 3:
            complete += 1
            for d in s.get("dec", []):
                if d["ch"] & SELF:
                    src_self[d["src"]] += 1
                if d["ch"] & GIANT:
                    src_giant[d["src"]] += 1
    tails = Counter((s.get("tail") or {}).get("why", "none") for s in sims if s.get("end") in ("win", "loss"))
    depths = Counter(len(s.get("path", [])) for s in sims if s.get("end") in ("win", "loss"))
    return {"simulations": dict(ends), "suffix_resolved_natural": complete,
            "self_target_choice_source_in_resolved": dict(src_self),
            "dr_giant_choice_source_in_resolved": dict(src_giant),
            "offered_chosen_by_source": dict(offered), "tail_start": dict(tails),
            "path_length": dict(sorted(depths.items()))}


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--panel", required=True)
    ap.add_argument("--out", required=True)
    ap.add_argument("traces", nargs="+")
    a = ap.parse_args()
    roles = {r["root_id"]: r["role"] for r in json.loads(Path(a.panel).read_text())["roots"]}
    result = {}
    for t in a.traces:
        meta, sims, nodes, worlds = load(t)
        hist = edge_returns(sims)
        rid = meta["root_id"]
        result[rid] = {"role": roles.get(rid), "stratum": meta["stratum"], "opp_model": meta["opp_model"],
                       "nodes": len(nodes), "selection": selection_summary(sims, meta),
                       "worlds": [eval_world(w, nodes, hist) for w in worlds]}
    Path(a.out).mkdir(parents=True, exist_ok=True)
    (Path(a.out) / "analysis.json").write_text(json.dumps(result, indent=1))
    for rid, r in result.items():
        v = Counter()
        for w in r["worlds"]:
            for o in w["opportunities"]:
                key = f"{o['kind']}:{'chosen' if o['chosen'] else 'declined'}:{o['src']}"
                if "compat" in o:
                    key += f":{o['compat']['verdict']}"
                v[key] += 1
        print(rid, r["role"], json.dumps(r["selection"]["suffix_resolved_natural"]), dict(v))


if __name__ == "__main__":
    main()
