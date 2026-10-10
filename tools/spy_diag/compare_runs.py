"""Before/after table for two analyze.py outputs on the same panel.

Usage: python compare_runs.py BEFORE/analysis.json AFTER/analysis.json BEFORE_ROWS_DIR AFTER_ROWS_DIR OUT.json
Rows dirs hold the s4a_diag_root rows (for CPU, simulations and J).
"""
import glob
import json
import sys
from collections import Counter


def rows(d):
    out = {}
    for f in glob.glob(f"{d}/*.jsonl"):
        for line in open(f, encoding="utf-8"):
            r = json.loads(line)
            if r.get("kind") == "s4a_diag_root" and r.get("diag", {}).get("trace"):
                out[r["root_id"]] = r
    return out


def tree_giant(nodes_lab):
    return nodes_lab


def side(a, r):
    s = a["selection"]
    ws = a["worlds"]
    e = r["arms"]["E"]
    dr_src = Counter()
    for w in ws:
        dr_src.update(w["steps"]["dr_giant"]["offer_sources"])
    self_status = Counter(w["steps"]["self_target"]["status"] for w in ws)
    dr_status = Counter(w["steps"]["dr_giant"]["status"] for w in ws)
    return {
        "simulations_natural": s["natural"],
        "selection_completions": s["suffix_complete_natural"],
        "completion_wins": s["suffix_complete_wins"],
        "giant_choice_source_in_completions": s["choice_source_in_complete"]["dr_giant"],
        "self_choice_source_in_completions": s["choice_source_in_complete"]["self_target"],
        "tree_nodes": a["nodes"],
        "tree_depth_max": (s["tree_node_depth"] or {}).get("max"),
        "giant_offer_depth_median": (s["giant_offer_depth"] or {}).get("median"),
        "eval_completed": sum(w["suffix_stage"] == 3 for w in ws),
        "eval_J": e["summary"]["j"],
        "eval_wins": e["summary"]["w"],
        "eval_dr_offer_sources": dict(dr_src),
        "eval_self_target": dict(self_status),
        "eval_dr_giant": dict(dr_status),
        "eval_first_miss_depth_median": sorted(w["first_miss_depth"] or 0 for w in ws)[len(ws) // 2],
        "eval_matched_decisions": sum(w["matched"] for w in ws),
        "eval_focal_nonforced": sum(w["focal_nonforced"] for w in ws),
        "selection_transitions": e["selection"]["transitions"],
        "eval_transitions": e["summary"]["eval_transitions"],
        "root_wall_s": round(r["timing"]["root_wall"], 1),
    }


def main():
    before, after = json.load(open(sys.argv[1])), json.load(open(sys.argv[2]))
    rb, ra = rows(sys.argv[3]), rows(sys.argv[4])
    out = []
    for rid in sorted(after, key=lambda k: after[k]["role"]):
        out.append({"root_id": rid, "role": after[rid]["role"],
                    "before": side(before[rid], rb[rid]), "after": side(after[rid], ra[rid])})
    json.dump(out, open(sys.argv[5], "w"), indent=1)
    for x in out:
        b, a = x["before"], x["after"]
        print(f"{x['role']:20} comp {b['selection_completions']:4}->{a['selection_completions']:5} "
              f"giant-src {json.dumps(a['giant_choice_source_in_completions'])} "
              f"evalcomp {b['eval_completed']}->{a['eval_completed']} J {b['eval_J']}->{a['eval_J']} "
              f"W {b['eval_wins']}->{a['eval_wins']} sims {b['simulations_natural']}->{a['simulations_natural']} "
              f"depth {b['tree_depth_max']}->{a['tree_depth_max']} DRdep {b['giant_offer_depth_median']}->{a['giant_offer_depth_median']}")
        print(f"{'':20} self {json.dumps(a['eval_self_target'])} dr {json.dumps(a['eval_dr_giant'])} "
              f"dr-offer-src {json.dumps(a['eval_dr_offer_sources'])}")


if __name__ == "__main__":
    main()
