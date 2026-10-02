"""Plot the completed retrospective crossover without pooling panels."""
import argparse
from pathlib import Path
from public_evaluation_dispatch_v1 import read, write, pin


def run(root):
    result = read(root / "analysis.json")
    if not result["complete"] or result["counts"]["new_matches"] != 3488:
        raise ValueError("complete diagnostic required")
    import matplotlib
    matplotlib.use("Agg")
    import matplotlib.pyplot as plt
    figure, axes = plt.subplots(1, 2, figsize=(11, 4.5), constrained_layout=True)
    for axis, cohort in zip(axes, ["focal", "canonical"]):
        cells = result["cohorts"][cohort]["cells"]
        for train, color, offset in [(1, "#237a95", -.12), (2, "#a95024", .12)]:
            values = []
            for panel in [1, 2]:
                item = cells[f"train{train}-panel{panel}"]["bo3"]
                value = item["difference"]*100
                low, high = [v*100 for v in item["paired_95_interval"]]
                values.append(value)
                axis.errorbar(panel+offset, value, yerr=[[value-low], [high-value]], fmt="o",
                              color=color, capsize=5)
                axis.annotate(f"{value:+.2f}", (panel+offset, value), xytext=(6, 0),
                              textcoords="offset points", fontsize=9)
            axis.plot([1+offset, 2+offset], values, color=color, alpha=.6,
                      label=f"Training pair {train}")
        axis.axhline(0, color="#888888", linewidth=1)
        axis.set(xticks=[1, 2], xticklabels=["Discovery seeds", "Replication seeds"],
                 xlim=(.55, 2.55), title=cohort.capitalize()+" BO3",
                 ylabel="Treatment minus matched control (percentage points)")
        axis.grid(axis="y", alpha=.2)
        axis.legend(loc="best")
    figure.suptitle("Same trained endpoints across both panels: paired 95% intervals")
    figure.savefig(root / "crossover-summary.png", dpi=180)
    figure.savefig(root / "crossover-summary.svg")
    plt.close(figure)
    full = read(root / "full-panel/result.json")
    qualification = read(root / "qualification.json")
    costs = {k: full[k] for k in ["staging_seconds", "execution_seconds", "recovery_seconds"]}
    write(root / "summary-artifacts.json", dict(analysis=pin(root / "analysis.json"),
        plot=pin(root / "crossover-summary.png"), svg=pin(root / "crossover-summary.svg"),
        actual_dispatch_seconds=sum(costs.values()), stages=costs,
        projected_seconds=qualification["selected"]["projected_seconds"],
        allocation=qualification["selected"]["allocation"], paid_compute=False))
    print(dict(actual_seconds=sum(costs.values()), selected=qualification["selected"]["id"]))


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, required=True)
    run(parser.parse_args().root)
