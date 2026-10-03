"""Descriptive policy disagreement on identical saved inputs, not a strength plot."""
import argparse
from pathlib import Path
import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt
import numpy as np
from public_evaluation_dispatch_v1 import read, write, pin


def main(root):
    analysis = read(root/"analysis.json")
    assert analysis["complete"] and not analysis["strength_claim"]
    summaries = analysis["summaries"]
    keys = [k for k in ["all", "prevention_active/False", "prevention_active/True"] if k in summaries]
    titles = {"all":"All sampled choices", "prevention_active/False":"No active prevention", "prevention_active/True":"Active prevention"}
    labels = {"t1-control":"C1", "t1-structured":"T1", "t2-control":"C2", "t2-structured":"T2"}
    pair_labels = [labels[p["from_model"]]+" / "+labels[p["to_model"]] for p in summaries["all"]["pairs"]]
    fig, axes = plt.subplots(1, 2, figsize=(12.5, 5.5))
    colors = ["#546E7A", "#2878B5", "#DB6A34"]
    x = np.arange(6)
    width = .24
    for j,k in enumerate(keys):
        s = summaries[k]
        offset=(j-(len(keys)-1)/2)*width
        label=f"{titles[k]} (n={s['rows']:,})"
        axes[0].bar(x+offset, [100*p["top_action_disagreement"] for p in s["pairs"]], width, label=label, color=colors[j])
        axes[1].bar(x+offset, [100*p["mean_tv"] for p in s["pairs"]], width, color=colors[j])
    for ax in axes:
        ax.set_xticks(x, pair_labels)
        ax.spines[["top", "right"]].set_visible(False)
        ax.grid(axis="y", alpha=.2)
        ax.set_axisbelow(True)
    axes[0].set_ylabel("Top-action disagreement (%)")
    axes[1].set_ylabel("Mean total variation × 100")
    axes[0].set_title("Which action ranks first?")
    axes[1].set_title("How much probability mass moves?")
    fig.suptitle("Four fixed checkpoints on identical saved decisions", fontsize=16, y=.98)
    handles, legend_labels = axes[0].get_legend_handles_labels()
    fig.legend(handles, legend_labels, loc="lower center", bbox_to_anchor=(.5,.095), ncol=3, frameon=False, fontsize=9)
    fig.text(.5,.025,"C = control; T = prevention treatment; 1/2 = training run. 240 late-training archives, ≤64 choices each.\nDescriptive row-weighted diagnostics. Unequal exposure groups; no strength or causal claim.",ha="center",fontsize=9,color="#444444")
    fig.tight_layout(rect=(0,.19,1,.94))
    for suffix in ["png", "svg"]: fig.savefig(root/f"policy-drift-summary.{suffix}", dpi=160)
    write(root/"figure-artifacts.json", dict(analysis=pin(root/"analysis.json"), script=pin(__file__),
        figures=[pin(root/f"policy-drift-summary.{s}") for s in ["png", "svg"]]))


if __name__ == "__main__":
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root",type=Path,required=True)
    main(parser.parse_args().root)
