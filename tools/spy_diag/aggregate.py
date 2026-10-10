"""Pooled panel numbers and the result chart from analysis.json.

Usage: python aggregate.py ANALYSIS.json SUMMARY.json OUTDIR
Writes OUTDIR/aggregate.json and OUTDIR/spy-diagnosis.png.
"""
import json
import sys
from collections import Counter
from pathlib import Path

MAX_DEPTH = 32


def main():
    a = json.load(open(sys.argv[1]))
    summary = json.load(open(sys.argv[2]))
    out = Path(sys.argv[3])
    agg = Counter()
    rates = Counter()
    for rid, r in a.items():
        s = r["selection"]
        agg["completed_suffixes"] += s["suffix_complete_natural"]
        agg["completed_suffix_wins"] += s["suffix_complete_wins"]
        for step, src in s["choice_source_in_complete"].items():
            for k, v in src.items():
                agg[f"complete_{step}_{k}"] += v
        for k, v in s["choice_rates_by_source"].items():
            rates[k + "|offered"] += v["offered"]
            rates[k + "|chosen"] += v["chosen"]
        agg["tree_nodes"] += r["tree"]["nodes"]
        agg["mill_selection_nodes"] += r["tree"]["forced_complete_selection_nodes"]
        for w in r["worlds"]:
            agg["worlds"] += 1
            agg["eval_J"] += w["j"]
            agg["focal_nonforced"] += w["focal_nonforced"]
            agg["attempted_lookups"] += w["attempted_lookups"]
            agg["matched"] += w["matched"]
            self_resolved = w["steps"]["self_target"]["status"] == "resolved"
            agg["self_resolved_worlds"] += self_resolved
            run = w["descending_run"]
            if self_resolved and run["length"] >= 10 and w["first_miss_depth"] == run["start_depth"]:
                agg["self_resolved_first_miss_at_mill_start"] += 1
            for o in w["opportunities"]:
                key = f"{o['kind']}|{o['src']}|{'chosen' if o['chosen'] else 'not'}"
                agg["opp|" + key] += 1
                if "at_node" in o:
                    agg[f"verdict|{o['kind']}|{o['at_node']['verdict']}|{'chosen' if o['chosen'] else 'not'}"] += 1
    agg = dict(sorted(agg.items()))
    agg["choice_rates_pooled"] = {k.rsplit("|", 1)[0]: [rates[k.rsplit("|", 1)[0] + "|chosen"], v]
                                  for k, v in rates.items() if k.endswith("|offered")}
    (out / "aggregate.json").write_text(json.dumps(agg, indent=1))

    import matplotlib
    matplotlib.use("Agg")
    import matplotlib.pyplot as plt
    rows = summary
    labels = [x["role"].replace("_", " ") for x in rows]
    fig, ax = plt.subplots(1, 2, figsize=(13, 5.2), gridspec_kw={"width_ratios": [1.35, 1]})
    y = list(range(len(rows)))[::-1]
    for yi, x in zip(y, rows):
        td = x["tree_node_depth"]["max"]
        gd = x["giant_offer_depth"]["median"] if x["giant_offer_depth"] else None
        miss = x["eval_first_miss_depth_median"]
        ax[0].barh(yi, td, color="#4C78A8", height=0.55)
        ax[0].plot([miss], [yi], marker="|", color="#E45756", markersize=16, mew=3)
        if gd is not None:
            ax[0].plot([gd], [yi], marker="o", color="#F58518", markersize=8)
    ax[0].axvline(MAX_DEPTH, color="#888", ls="--", lw=1)
    ax[0].text(MAX_DEPTH + 1, -0.6, "depth limit 32", color="#666", fontsize=9)
    ax[0].set_yticks(y)
    ax[0].set_yticklabels(labels, fontsize=9)
    ax[0].set_xlabel("focal decisions from the root")
    ax[0].set_title("Where the tree ends vs where Dread Return's target is chosen", fontsize=10.5)
    ax[0].plot([], [], color="#4C78A8", lw=8, label="deepest tree node")
    ax[0].plot([], [], marker="|", color="#E45756", ls="", markersize=12, mew=3, label="execution leaves the tree (median)")
    ax[0].plot([], [], marker="o", color="#F58518", ls="", label="DR -> Giant offered (selection median)")
    ax[0].legend(fontsize=8, loc="lower right")
    ax[0].set_xlim(0, 170)
    cats = ["tree", "expand", "tail_after_expand"]
    names = {"tree": "existing node", "expand": "new node", "tail_after_expand": "plain tail"}
    pooled = agg["choice_rates_pooled"]
    bottom = [0, 0]
    colors = {"tree": "#4C78A8", "expand": "#72B7B2", "tail_after_expand": "#F58518"}
    steps = [("self_target", "Spy targets itself"), ("dr_giant", "DR targets Giant")]
    for c in cats:
        vals = [agg.get(f"complete_{s}_{c}", 0) for s, _ in steps]
        ax[1].bar([n for _, n in steps], vals, bottom=bottom, color=colors[c], label=names[c], width=0.55)
        bottom = [b + v for b, v in zip(bottom, vals)]
    for i, (s, _) in enumerate(steps):
        ch, off = pooled.get(f"{s}|tail_after_expand", [0, 0])
        ax[1].text(i, bottom[i] + 6, f"plain policy picks it\n{ch:,} of {off:,} offers", ha="center", fontsize=8.5)
    ax[1].set_ylim(0, max(bottom) * 1.25)
    ax[1].set_title(f"Who made the choice in the {agg['completed_suffixes']} completed lines (selection)", fontsize=10.5)
    ax[1].legend(fontsize=8, loc="upper left")
    fig.tight_layout()
    fig.savefig(out / "spy-diagnosis.png", dpi=130)
    print(json.dumps(agg, indent=1))


if __name__ == "__main__":
    main()
