"""Saved-record repeat-support diagnosis (collab LANES/spy-history-support-plan-20261010, PLAN.md).

Usage: python support_chains.py TRACE_DIR OUT.json
Reads boundary-profile traces only (no engine run). For each preselected
case it binds the node by key, reconstructs the node's arrival chronology
from ordered selection records, walks its ancestor chain with per-edge
traffic, picks up to two same-root successful comparison nodes (most
winning Giant backups, then most visits, then lowest id), finds the
deepest common ancestor and how the paths split there, and lists the
evaluated prefix per world. Spy labels only pick diagnostic witnesses.
"""
import json
import sys
from collections import Counter, defaultdict
from pathlib import Path

GIANT, SELF = 4, 2
MIN = 8
CASES = [
    ("r1-cast-A48-g1939-s174", 8308, "matched target, insufficient arrivals"),
    ("r1-cast-A48-g508-s169", 443, "matched target, untried sweep blocks repeats"),
    ("r1-target-T1-g1777-s117", 4809, "matched target, insufficient arrivals"),
    ("r1-cast-T1-g4558-s69", 70, "successful contrast"),
    ("r2-cast-A48-g4837-s111", 308, "successful contrast"),
    ("r1-target-A48-g1480-s108", None, "history-miss contrast"),
]


def load(path):
    nodes, sims, worlds = {}, [], []
    with open(path, encoding="utf-8") as f:
        for line in f:
            r = json.loads(line)
            if r["r"] == "node":
                nodes[r["id"]] = r
            elif r["r"] == "sel":
                sims.append(r)
            elif r["r"] == "eval":
                worlds.append(r)
    return nodes, sims, worlds


def chain(nodes, nid):
    out = []
    while nid is not None:
        n = nodes[nid]
        out.append(nid)
        nid = n["parent"][0] if n["parent"] else None
    return out[::-1]


def edge_row(n, e):
    rank = {x: i for i, x in enumerate(n["perm"])}
    return {"edge": e, "lab": n["lab"][e], "n": n["n"][e], "wins": n["w"][e],
            "mean": round(n["w"][e] / n["n"][e], 4) if n["n"][e] else None,
            "qualified": n["n"][e] >= MIN, "tie_rank": rank[e],
            "semantic": n["edges"][e][:160]}


def ancestors(nodes, nid):
    rows = []
    ch = chain(nodes, nid)
    for a, b in zip(ch, ch[1:]):
        na, e = nodes[a], nodes[b]["parent"][1]
        best = max(range(len(na["n"])), key=lambda x: (na["n"][x], -x))
        qual = [x for x in range(len(na["n"])) if na["n"][x] >= MIN]
        fc = max(qual, key=lambda x: (na["w"][x] / na["n"][x], -na["perm"].index(x))) if qual else None
        rows.append({"node": a, "key": na["key"][:16], "dep": na["dep"], "visits": na["visits"], "width": len(na["n"]),
                     "toward_target": edge_row(na, e), "share_of_visits": round(na["n"][e] / max(na["visits"], 1), 4),
                     "most_visited_edge": edge_row(na, best), "frozen_choice": edge_row(na, fc) if fc is not None else None})
    return rows


def chronology(nodes, sims, nid):
    n = nodes[nid]
    counts = [0] * len(n["n"])
    events = []
    first_giant_win = None
    for s in sims:
        if s.get("end") not in ("win", "loss"):
            continue
        for p in s.get("path", []):
            if p[0] == nid:
                e = p[1]
                untried = counts[e] == 0
                counts[e] += 1
                win = s["end"] == "win"
                events.append({"sim": s["i"], "edge": e, "lab": n["lab"][e], "untried": untried,
                               "expanded_here": s.get("new") == nid, "win": win})
                if first_giant_win is None and win and n["lab"][e] & GIANT:
                    first_giant_win = s["i"]
    after = [x for x in events if first_giant_win is not None and x["sim"] > first_giant_win]
    reach8 = None
    seen = Counter()
    for x in events:
        seen[x["edge"]] += 1
        if reach8 is None and n["lab"][x["edge"]] & GIANT and seen[x["edge"]] >= MIN:
            reach8 = x["sim"]
    reconciled = counts == n["n"]
    return {"node": nid, "key": n["key"][:16], "visits": n["visits"], "width": len(n["n"]),
            "edges_tried": sum(c > 0 for c in counts), "reconciled_with_saved_counts": reconciled,
            "arrivals": len(events), "first_giant_win_sim": first_giant_win,
            "arrivals_after_first_giant_win": len(after),
            "repeat_selections_after": sum(not x["untried"] for x in after),
            "untried_selections_after": sum(x["untried"] for x in after),
            "sim_of_eighth_giant_backup": reach8,
            "total_simulations": sum(1 for s in sims if s.get("end") in ("win", "loss")),
            "events": events[:60]}


def success_nodes(nodes, exclude):
    cands = []
    for n in nodes.values():
        if n["id"] == exclude:
            continue
        g = [e for e, b in enumerate(n["lab"]) if b & GIANT and n["w"][e] > 0]
        if g:
            cands.append((-max(n["w"][e] for e in g), -n["visits"], n["id"]))
    return [c[2] for c in sorted(cands)[:2]]


def divergence(nodes, a, b):
    ca, cb = chain(nodes, a), chain(nodes, b)
    k = 0
    while k < min(len(ca), len(cb)) and ca[k] == cb[k]:
        k += 1
    if k == 0:
        return {"common": None}
    common = ca[k - 1]
    ea = nodes[ca[k]]["parent"][1] if k < len(ca) else None
    eb = nodes[cb[k]]["parent"][1] if k < len(cb) else None
    kind = ("same parent action, different following observation" if ea == eb and ea is not None
            else "different parent action")
    return {"common": common, "common_dep": nodes[common]["dep"], "kind": kind,
            "failed_edge": edge_row(nodes[common], ea) if ea is not None else None,
            "success_edge": edge_row(nodes[common], eb) if eb is not None else None,
            "failed_child_visits": nodes[ca[k]]["visits"] if k < len(ca) else None,
            "success_child_visits": nodes[cb[k]]["visits"] if k < len(cb) else None}


def eval_prefix(worlds, nid=None):
    out = []
    for w in worlds:
        rows, reached = [], False
        for d in w["dec"]:
            if d["node"] is None and d["src"] != "first_miss_plain":
                continue
            rows.append([d["dep"], d["node"], d["edge"], d["src"], d["off"], d["ch"]])
            if d["node"] == nid or d["src"] == "first_miss_plain":
                reached = d["node"] == nid
                break
        out.append({"world": w["world"], "reached_target_node": reached, "prefix": rows})
    return out


def reconcile_all(nodes, sims):
    """Per-edge natural returns rebuilt from ordered simulation paths equal
    the saved n and w at every node, and visits equal the sum of n."""
    n = defaultdict(lambda: Counter())
    w = defaultdict(lambda: Counter())
    for s in sims:
        if s.get("end") not in ("win", "loss"):
            continue
        for p in s.get("path", []):
            n[p[0]][p[1]] += 1
            w[p[0]][p[1]] += s["end"] == "win"
    for nid, node in nodes.items():
        if [n[nid][e] for e in range(len(node["n"]))] != node["n"]:
            return False
        if [w[nid][e] for e in range(len(node["w"]))] != node["w"] or sum(node["n"]) != node["visits"]:
            return False
    return True


def main():
    tdir, outp = Path(sys.argv[1]), sys.argv[2]
    result = {}
    for rid, nid, role in CASES:
        nodes, sims, worlds = load(tdir / f"{rid}.trace.jsonl")
        r = {"role": role, "all_nodes_reconciled": reconcile_all(nodes, sims)}
        if nid is None:
            r["eval_prefix"] = eval_prefix(worlds)
            miss = Counter()
            for w in worlds:
                for d in w["dec"]:
                    if d["src"] == "first_miss_plain":
                        miss[(d["dep"], d.get("miss_key", "")[:16])] += 1
            r["first_misses"] = [[k[0], k[1], v] for k, v in miss.most_common()]
        else:
            r["node_key"] = nodes[nid]["key"]
            r["ancestors"] = ancestors(nodes, nid)
            r["chronology"] = chronology(nodes, sims, nid)
            r["comparisons"] = []
            for sid in success_nodes(nodes, nid):
                r["comparisons"].append({"node": sid, "key": nodes[sid]["key"][:16], "visits": nodes[sid]["visits"],
                                         "giant": [edge_row(nodes[sid], e) for e, b in enumerate(nodes[sid]["lab"]) if b & GIANT],
                                         "divergence": divergence(nodes, nid, sid)})
            r["eval_prefix"] = eval_prefix(worlds, nid)
        result[rid] = r
    json.dump(result, open(outp, "w"), indent=1)
    for rid, r in result.items():
        print("==", rid, r["role"])
        if "chronology" in r:
            c = r["chronology"]
            print("  node", c["node"], "visits", c["visits"], "width", c["width"], "tried", c["edges_tried"],
                  "reconciled", c["reconciled_with_saved_counts"], "first Giant win sim", c["first_giant_win_sim"],
                  "arrivals after", c["arrivals_after_first_giant_win"], "repeat/untried after",
                  c["repeat_selections_after"], c["untried_selections_after"], "8th Giant backup", c["sim_of_eighth_giant_backup"],
                  "of", c["total_simulations"], "sims")
            for a in r["ancestors"]:
                t = a["toward_target"]
                print(f"    dep {a['dep']:2} node {a['node']:6} visits {a['visits']:6} w{a['width']:3} toward n {t['n']:6} "
                      f"mean {t['mean']} share {a['share_of_visits']} | top edge n {a['most_visited_edge']['n']} "
                      f"mean {a['most_visited_edge']['mean']} lab {a['most_visited_edge']['lab']}")
            for c2 in r["comparisons"]:
                d = c2["divergence"]
                print("   success node", c2["node"], "visits", c2["visits"], "giant",
                      [(g["n"], g["wins"]) for g in c2["giant"]], "| split at node", d.get("common"), "dep",
                      d.get("common_dep"), d.get("kind"), "failed-side n", (d.get("failed_edge") or {}).get("n"),
                      "success-side n", (d.get("success_edge") or {}).get("n"))
            print("  eval worlds reaching node", sum(w["reached_target_node"] for w in r["eval_prefix"]), "of", len(r["eval_prefix"]))
        else:
            print("  first misses (dep, key, worlds):", r["first_misses"][:6])


if __name__ == "__main__":
    main()
