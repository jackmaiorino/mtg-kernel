"""Branching attribution and B1/D8 comparison (collab LANES/search-budget-plan-20261011).

CLI:
  attribution --rows B1ROWS... --out ATTRIBUTION.json   Stage A (selection diagnostics only)
  compare     --b1 B1ROWS... --d8 D8ROWS... --out RESULT.json

Panel roots are the 36 IDs of cases.txt; the two named roots are reported apart.
Labels (erratum, collab LANES/search-identity-audit-20261011): L counts physical groups;
"share"/"first_substep" are the first-recorded-choice metrics (not necessarily substep 0);
"selection_seconds" is elapsed wall time, not CPU.
Per root, evaluation metrics average over its 16 worlds; lineages average roots.
Missing roots, failed searches, rejected samples and invalid (nonnatural, truncated,
faulted) evaluations make the comparison incomplete; they are never losses.
"""
from __future__ import annotations

import argparse
import json
import statistics as st
from pathlib import Path

LINEAGES = ("r1", "r2")


def rows(paths):
    out = {}
    for p in paths:
        for line in Path(p).read_text(encoding="utf-8").splitlines():
            if line.strip():
                r = json.loads(line)
                if r.get("kind") == "error":
                    raise SystemExit(f"error row: {r}")
                if r.get("kind") == "branch_root":
                    if r["root_id"] in out:
                        raise SystemExit(f"duplicate root {r['root_id']}")
                    out[r["root_id"]] = r
    return out


def named(root_id):
    return "@" in root_id


def tree_summary(r):
    t, s = r["tree"], r["simulations"]
    deep = [d for d in t["by_physical_depth"] if d["physical_depth"] <= 6]
    return {"nodes": t["nodes"], "max_physical_depth": t["max_physical_depth"],
            "max_topology_depth": t["max_topology_depth"],
            "links": t["links"], "unexpanded_backed_edges": t["unexpanded_backed_edges"],
            "groups_with_several_children": t["action_groups"]["with_several_children"],
            "observation_split": t["action_groups"]["observation_split"],
            "menu_only_split": t["action_groups"]["menu_only_split"],
            "sibling_tags": t["action_groups"]["sibling_difference_tags"],
            "repeated_views": t["repeated_views"],
            "depth_profile": [{k: d[k] for k in ("physical_depth", "nodes", "visits", "mean_legal", "mean_tried",
                                                 "mean_child_bearing", "mean_effective_actions", "mean_top_share",
                                                 "nodes_with_eligible")} for d in deep],
            "simulations": s}


def attribution(b1: dict) -> dict:
    panel = {k: v for k, v in b1.items() if not named(k)}
    out = {"roots": len(panel), "named": sorted(k for k in b1 if named(k)),
           "parity": {k: v["b1_parity"] for k, v in b1.items()},
           "failed_searches": [k for k, v in b1.items() if v["selection_failed"]],
           "lineages": {}, "per_root": {k: tree_summary(v) for k, v in sorted(b1.items())}}
    for lin in LINEAGES:
        rs = [v for v in panel.values() if v["lineage"] == lin]
        if not rs:
            continue
        def m(f):
            vals = [f(r) for r in rs]
            return {"mean": st.mean(vals), "median": st.median(vals), "min": min(vals), "max": max(vals)}
        out["lineages"][lin] = {
            "roots": len(rs),
            "nodes": m(lambda r: r["tree"]["nodes"]),
            "max_physical_depth": m(lambda r: r["tree"]["max_physical_depth"]),
            "extra_action_groups": m(lambda r: r["tree"]["links"]["extra_action_groups"]),
            "extra_observations": m(lambda r: r["tree"]["links"]["extra_observations_within_groups"]),
            "identity_holds_all": all(r["tree"]["links"]["identity_holds"] for r in rs),
            "share_groups_observation_split": m(lambda r: r["tree"]["action_groups"]["observation_split"]
                                                / max(1, r["tree"]["links"]["G_action_groups_with_children"])),
            "views_reached_by_several_histories": m(lambda r: r["tree"]["repeated_views"]["views_reached_by_several_histories"]),
            "unexpanded_backed_edges": m(lambda r: r["tree"]["unexpanded_backed_edges"]["edges"]),
            "depth2_effective_actions": m(lambda r: next((d["mean_effective_actions"] for d in r["tree"]["by_physical_depth"]
                                                         if d["physical_depth"] == 2), 0.0)),
            "depth3_nodes_with_eligible_share": m(lambda r: next((d["nodes_with_eligible"] / max(1, d["nodes"])
                                                               for d in r["tree"]["by_physical_depth"]
                                                               if d["physical_depth"] == 3), 0.0)),
            "transitions_after_first_expansion_share": m(
                lambda r: r["simulations"]["natural"]["mean_transitions_after_first_expansion"]
                / max(1.0, r["simulations"]["natural"]["mean_transitions"])),
            "first_expansion_mean_depth": m(lambda r: sum(int(k) * v for k, v in r["simulations"]["natural"]
                                                          ["first_expansion_depth_histogram"].items())
                                            / max(1, r["simulations"]["natural"]["count"])),
            "sibling_tags": {tag: sum(r["tree"]["action_groups"]["sibling_difference_tags"].get(tag, 0) for r in rs)
                             for tag in ("own_private", "opponent_known", "own_public", "opponent_public",
                                         "public_other", "menu", "residual")},
            "decks": sorted({r["own"] for r in rs})}
    return out


def root_eval(r):
    ws = r["evaluation"]
    valid = [w for w in ws if "rejected" not in w and w.get("valid")]
    if r["selection_failed"] or len(valid) != 16:
        return None
    m = lambda k: st.mean(w[k] for w in valid)  # noqa: E731
    # Row schema v2 renamed the v1 fields: "first_substep_*" counted a group's first
    # recorded nonforced choice, which need not be substep zero.
    v2 = r.get("schema") == "branch-root-v2"
    share = "first_recorded_choice_share" if v2 else "first_substep_share"
    first = "first_recorded_choice_controlled" if v2 else "first_substep_controlled"
    desc = "descendant_first_recorded_controlled" if v2 else "descendant_controlled"
    return {"L": m("L"), "L_values": [w["L"] for w in valid], "share": m(share),
            "first_substep": m(first), "descendants": m(desc),
            "after_fallback": m("support_after_first_fallback"), "wins": sum(w["win"] for w in valid),
            "first_unsupported": [w["first_unsupported_group"] for w in valid],
            "first_miss": [w["first_miss_group"] for w in valid],
            "selection_seconds": r["timing"]["selection_wall_seconds" if v2 else "selection"], "states": [w["world_state_hash"] for w in valid],
            "outcomes": [w["win"] for w in valid]}


def compare(b1: dict, d8: dict) -> dict:
    out = {"incomplete": [], "lineages": {}, "named": {}, "per_root": {}}
    for k in sorted(set(b1) | set(d8)):
        a, b = b1.get(k), d8.get(k)
        ea, eb = (root_eval(a) if a else None), (root_eval(b) if b else None)
        if ea is None or eb is None:
            out["incomplete"].append(k)
            continue
        if ea["states"] != eb["states"]:
            out["incomplete"].append(f"{k}: evaluation worlds differ")
            continue
        disc = {"d8_only_win": sum(1 for x, y in zip(ea["outcomes"], eb["outcomes"]) if y and not x),
                "b1_only_win": sum(1 for x, y in zip(ea["outcomes"], eb["outcomes"]) if x and not y)}
        rec = {"B1": {x: ea[x] for x in ("L", "share", "first_substep", "descendants", "after_fallback", "wins",
                                          "selection_seconds")},
               "D8": {x: eb[x] for x in ("L", "share", "first_substep", "descendants", "after_fallback", "wins",
                                          "selection_seconds")},
               "discordance": disc, "own": a["own"], "lineage": a["lineage"],
               "staged_revisits": b["simulations"]["natural"]["revisits_to_tail_nodes"],
               "retained_tail_nodes": b["simulations"]["natural"]["retained_tail_nodes"],
               "max_depth": {"B1": a["tree"]["max_physical_depth"], "D8": b["tree"]["max_physical_depth"]},
               "nodes": {"B1": a["tree"]["nodes"], "D8": b["tree"]["nodes"]},
               "completed": {"B1": a["simulations"]["natural"]["count"], "D8": b["simulations"]["natural"]["count"]}}
        (out["named"] if named(k) else out["per_root"])[k] = rec
    for lin in LINEAGES:
        rs = [v for v in out["per_root"].values() if v["lineage"] == lin]
        if not rs:
            continue
        agg = {arm: {x: st.mean(r[arm][x] for r in rs) for x in ("L", "share", "first_substep", "descendants",
                                                                  "after_fallback", "selection_seconds")}
               for arm in ("B1", "D8")}
        wins = {arm: sum(r[arm]["wins"] for r in rs) for arm in ("B1", "D8")}
        complete = len(rs) == 18
        screen = (complete and agg["D8"]["L"] > agg["B1"]["L"] and agg["D8"]["share"] > agg["B1"]["share"]
                  and wins["D8"] >= wins["B1"])
        out["lineages"][lin] = {"roots": len(rs), "complete": complete, "means": agg, "wins": wins,
                                "worlds": 16 * len(rs),
                                "L_per_selection_second": {arm: agg[arm]["L"] / agg[arm]["selection_seconds"]
                                                           for arm in ("B1", "D8")},
                                "screen": screen}
    out["complete"] = not out["incomplete"] and all(v["complete"] for v in out["lineages"].values()) \
        and len(out["lineages"]) == 2
    out["disposition"] = ("incomplete" if not out["complete"] else
                          "promising mechanism screen" if all(v["screen"] for v in out["lineages"].values())
                          else "screen not met")
    return out


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    sub = ap.add_subparsers(dest="op", required=True)
    a = sub.add_parser("attribution")
    a.add_argument("--rows", nargs="+", required=True)
    a.add_argument("--out", type=Path, required=True)
    c = sub.add_parser("compare")
    c.add_argument("--b1", nargs="+", required=True)
    c.add_argument("--d8", nargs="+", required=True)
    c.add_argument("--out", type=Path, required=True)
    args = ap.parse_args(argv)
    if args.op == "attribution":
        result = attribution(rows(args.rows))
    else:
        result = compare(rows(args.b1), rows(args.d8))
    args.out.write_text(json.dumps(result, indent=1) + "\n")
    print(json.dumps({k: v for k, v in result.items() if k not in ("per_root", "named")}, indent=1)[:3000])
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
