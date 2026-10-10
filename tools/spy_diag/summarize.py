"""Compact per-root tables from analysis.json (analyze.py).

Usage: python summarize.py ANALYSIS.json OUT.json
"""
import json
import sys
from collections import Counter

ORDER = ["r1_target_loss_T1", "r1_target_loss_A48", "r1_dr_loss_T1", "r1_dr_loss_A48", "r2_positive_T1",
         "r2_positive_A48", "r2_zero_T1", "r2_zero_A48", "r1_J_control_T1", "r1_J_control_A48"]


def med(xs):
    xs = sorted(x for x in xs if x is not None)
    return xs[len(xs) // 2] if xs else None


def main():
    a = json.load(open(sys.argv[1]))
    rows = []
    for rid, r in sorted(a.items(), key=lambda kv: ORDER.index(kv[1]["role"])):
        s = r["selection"]
        ws = r["worlds"]
        step = {}
        for name in ("cast", "self_target", "dr_giant"):
            st = Counter(w["steps"][name]["status"] for w in ws)
            src_off = Counter()
            src_ch = Counter()
            for w in ws:
                src_off.update(w["steps"][name]["offer_sources"])
                src_ch.update(w["steps"][name]["chosen_sources"])
            step[name] = {"status": dict(st), "offer_sources": dict(src_off), "chosen_sources": dict(src_ch),
                          "first_offer_depth_median": med(w["steps"][name]["first_offer_depth"] for w in ws)}
        verdicts = Counter()
        for w in ws:
            for o in w["opportunities"]:
                verdicts[f"{o['kind']}: {o['at_node']['verdict'] if 'at_node' in o else o['stats_reason']}"
                         f" ({'chosen' if o['chosen'] else 'not chosen'})"] += 1
        rate = s["choice_rates_by_source"]
        rows.append({
            "root_id": rid, "role": r["role"], "stratum": r["stratum"], "opp": r["opp_model"],
            "selection_suffix_completions": s["suffix_complete_natural"],
            "selection_completion_wins": s["suffix_complete_wins"],
            "selection_natural_sims": s["natural"],
            "completions_choice_source": s["choice_source_in_complete"],
            "tail_dr_giant_rate": rate.get("dr_giant|tail_after_expand"),
            "tree_self_target_rate": rate.get("self_target|tree"),
            "tail_self_target_rate": rate.get("self_target|tail_after_expand"),
            "giant_offer_depth": s["giant_offer_depth"], "self_offer_depth": s["self_offer_depth"],
            "tree_node_depth": s["tree_node_depth"], "tail_start_depth": s["tail_start_depth"],
            "tree_nodes": r["tree"]["nodes"], "mill_selection_nodes": r["tree"]["forced_complete_selection_nodes"],
            "eval_J": sum(w["j"] for w in ws), "eval_wins": sum(w["end"] == "win" for w in ws),
            "eval_first_miss_depth_median": med(w["first_miss_depth"] for w in ws),
            "eval_deepest_match_median": med(w["deepest_match"] for w in ws),
            "eval_focal_nonforced": sum(w["focal_nonforced"] for w in ws),
            "eval_attempted_lookups": sum(w["attempted_lookups"] for w in ws),
            "eval_matched": sum(w["matched"] for w in ws),
            "eval_sources": dict(sum((Counter(w["sources"]) for w in ws), Counter())),
            "eval_steps": step, "eval_opportunity_verdicts": dict(verdicts),
            "eval_long_selection_runs": sum(w["descending_run"]["length"] >= 10 for w in ws),
        })
    json.dump(rows, open(sys.argv[2], "w"), indent=1)
    for x in rows:
        print(f"{x['role']:20} sel {x['selection_suffix_completions']:4} J {x['eval_J']} "
              f"src {json.dumps(x['completions_choice_source'])} "
              f"tailDR {x['tail_dr_giant_rate']} giantdep {x['giant_offer_depth'] and x['giant_offer_depth']['median']} "
              f"treedep {x['tree_node_depth']['max']} mill {x['mill_selection_nodes']}/{x['tree_nodes']} "
              f"miss {x['eval_first_miss_depth_median']} match {x['eval_deepest_match_median']}")
        print(f"{'':20} self {json.dumps(x['eval_steps']['self_target'])}")
        print(f"{'':20} dr   {json.dumps(x['eval_steps']['dr_giant'])}")
        print(f"{'':20} verdicts {json.dumps(x['eval_opportunity_verdicts'])}")


if __name__ == "__main__":
    main()
