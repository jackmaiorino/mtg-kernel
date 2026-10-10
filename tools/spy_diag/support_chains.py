"""Saved-record repeat-support diagnosis (collab LANES/spy-history-support-plan-20261010, PLAN.md).

Usage: python support_chains.py TRACE_DIR ROWS_DIR OUT.json
Reads boundary-profile traces only (no engine run). For each preselected
case it binds the node by key, reconstructs the node's arrival chronology
from ordered selection records, walks its ancestor chain with per-edge
traffic, picks up to two same-root successful comparison nodes (first most
winning Giant backups, then most visits among remaining successful nodes), finds the
deepest common ancestor and how the paths split there, and lists the
evaluated prefix per world. Spy labels only pick diagnostic witnesses.

The fixed bindings below were recovered from the already selected archived
node IDs, not from a new case selection. They match the frozen boundary
result rows in LANES/spy-resolution-boundary-20261010/evidence. ROWS_DIR
must contain those retained s4a_diag_root rows (*.jsonl). Historical runtime,
changed bytes/keys/parents, duplicate records and inconsistent chronology
are refused before creating the output. Existing outputs are preserved.
Evidence source: mtg-kernel-collab commit
070f0f5ffac0d3e75b28e5cdfae7a9747840e285, evidence/rows/b-*.jsonl.
"""
import hashlib
import json
import sys
from collections import Counter
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
PINS = {
    "r1-cast-A48-g1939-s174": ("7b7ef9ce509e0354900e1cdf484e17ffac29d3aef50a88d70c100307280bec6d", "6744432dffb49c0874365174edaa482a120162933ef42c4b51b2d33623c0d597", [5256, 1]),
    "r1-cast-A48-g508-s169": ("b8f9376ffe10fb909ec48b91e3f35474a7d789069c39e021411ca8f93d09fc6d", "d83c1f22b09c395c244b5bb57d04c0f06f05c2a06eac5b0e450a88b455c0fbca", [75, 2]),
    "r1-target-T1-g1777-s117": ("2dfd9678e910f8e9db3ec430d0f64ac349021ef9cd166edfff3651d9ac911b0f", "0f3cfae22bb15739378334cd391ceb4b4e2ec259dad3c1e43c771b7b38b91f0a", [902, 2]),
    "r1-cast-T1-g4558-s69": ("9260415b9a3bdddf1c5a930c023eb7c5529cd9d70bc8a1fae8a5616a7353e3bb", "92e795524c0ade5410d5a14c0f6f8e9779f3c94cc94acfa0916716f992da32a9", [26, 1]),
    "r2-cast-A48-g4837-s111": ("50b8aefaef03db38e22e66243d0954c3273eb18984dec1b38070383ad8c0b506", "0b207e30b00ff2d9d3dacbc06de1dc933a24f5fa449559c49680579f2ecee1b6", [116, 0]),
    "r1-target-A48-g1480-s108": ("323b9cc935737e3e3de02f4e9f5a04a651a4b0626922249329aab39e2cf48fd8", None, None),
}


def load_rows(folder):
    rows = {}
    for path in sorted(Path(folder).glob("*.jsonl")):
        with path.open(encoding="utf-8") as stream:
            for line in stream:
                row = json.loads(line)
                if row.get("kind") != "s4a_diag_root":
                    continue
                rid = row["root_id"]
                if rid in rows:
                    raise ValueError(f"duplicate diagnostic root {rid}")
                rows[rid] = row
    return rows


def load(path, rid=None, pin=None, row=None, nid=None):
    nodes, sims, worlds = {}, [], []
    selection_ids, world_ids = set(), set()
    data = Path(path).read_bytes()
    if pin is not None:
        digest = hashlib.sha256(data).hexdigest()
        if digest != pin[0]:
            raise ValueError(f"trace hash differs from fixed case binding for {rid}")
        if (row is None or row.get("root_id") != rid
                or row.get("runtime_rules") != "resolution-boundary-v1"
                or row.get("diag", {}).get("trace", {}).get("sha256") != digest):
            raise ValueError(f"missing or incompatible boundary result row for {rid}")
    meta = None
    for line in data.decode("utf-8").splitlines():
        r = json.loads(line)
        if r["r"] == "meta":
            if meta is not None:
                raise ValueError(f"duplicate trace metadata in {path}")
            meta = r
        elif r["r"] == "node":
            if r["id"] in nodes:
                raise ValueError(f"duplicate node {r['id']} in {path}")
            nodes[r["id"]] = r
        elif r["r"] == "sel":
            if r["i"] in selection_ids or (sims and r["i"] <= sims[-1]["i"]):
                raise ValueError(f"duplicate or unordered selection in {path}")
            selection_ids.add(r["i"])
            sims.append(r)
        elif r["r"] == "eval":
            if r["world"] in world_ids:
                raise ValueError(f"duplicate evaluation world in {path}")
            world_ids.add(r["world"])
            worlds.append(r)
    if pin is not None:
        if meta is None or meta.get("schema") != "s4a-diag-trace/v1" or meta.get("root_id") != rid:
            raise ValueError(f"trace metadata differs for {rid}")
        if nid is not None:
            node = nodes.get(nid)
            if node is None or node.get("key") != pin[1] or node.get("parent") != pin[2]:
                raise ValueError(f"selected node key or parent differs for {rid}")
    return nodes, sims, worlds


def chain(nodes, nid):
    out = []
    seen = set()
    while nid is not None:
        if nid in seen:
            raise ValueError("cyclic node ancestry")
        seen.add(nid)
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
    wins = [0] * len(n["n"])
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
                wins[e] += int(win)
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
    reconciled = counts == n["n"] and wins == n["w"] and sum(counts) == n["visits"]
    if not reconciled:
        raise ValueError(f"selection chronology disagrees with saved counts for node {nid}")
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
    if not cands:
        return []
    first = min(cands, key=lambda c: (c[0], c[2]))
    remaining = [c for c in cands if c[2] != first[2]]
    second = min(remaining, key=lambda c: (c[1], c[2])) if remaining else None
    return [first[2]] + ([second[2]] if second is not None else [])


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
            if (nid is not None and d["node"] == nid) or d["src"] == "first_miss_plain":
                reached = nid is not None and d["node"] == nid
                break
        out.append({"world": w["world"], "reached_target_node": reached, "prefix": rows})
    return out


def build_result(tdir, rows, cases=CASES, pins=PINS):
    result = {}
    for rid, nid, role in cases:
        nodes, sims, worlds = load(Path(tdir) / f"{rid}.trace.jsonl", rid, pins[rid], rows.get(rid), nid)
        if not reconcile_all(nodes, sims):
            raise ValueError(f"saved all-node counts disagree for {rid}")
        r = {"role": role, "trace_sha256": pins[rid][0], "runtime_rules": "resolution-boundary-v1",
             "all_nodes_reconciled": True}
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
    return result


def reconcile_all(nodes, sims):
    """Rebuild natural n/w/visits at every node, including unselected nodes."""
    counts = {nid: [0] * len(node["n"]) for nid, node in nodes.items()}
    wins = {nid: [0] * len(node["w"]) for nid, node in nodes.items()}
    for sim in sims:
        if sim.get("end") not in ("win", "loss"):
            continue
        for entry in sim.get("path", []):
            if not isinstance(entry, (list, tuple)) or len(entry) < 2:
                return False
            nid, edge = entry[:2]
            if type(nid) is not int or type(edge) is not int:
                return False
            if nid not in nodes or not 0 <= edge < len(counts[nid]) or edge >= len(wins[nid]):
                return False
            counts[nid][edge] += 1
            wins[nid][edge] += int(sim["end"] == "win")
    return all(counts[nid] == node["n"] and wins[nid] == node["w"]
               and sum(counts[nid]) == node["visits"] for nid, node in nodes.items())


def write_result(tdir, rows_dir, outp, cases=CASES, pins=PINS):
    result = build_result(tdir, load_rows(rows_dir), cases, pins)
    with open(outp, "x", encoding="utf-8") as stream:
        json.dump(result, stream, indent=1)
        stream.write("\n")
    return result


def main():
    if len(sys.argv) != 4:
        raise SystemExit("usage: support_chains.py TRACE_DIR ROWS_DIR OUT.json")
    result = write_result(*sys.argv[1:])
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
