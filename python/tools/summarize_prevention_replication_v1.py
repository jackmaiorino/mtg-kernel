"""Descriptive replication report after the unchanged complete-panel analysis."""
import argparse
from collections import defaultdict
from pathlib import Path

from public_evaluation_dispatch_v1 import read, write, pin, checked


def run(pilot, evaluation, discovery):
    result = read(pilot / "analysis.json")
    earlier = read(discovery / "analysis.json")
    full = read(evaluation / "full-panel/result.json")
    choice = read(evaluation / "compute-choice-extended.json")
    reference = read(checked(choice["candidates"][0]["report"]))
    if not result["complete"] or result["matches"] != 2616 or len(full["jobs"]) != 90:
        raise ValueError("complete panel required")
    if result["evaluation"] != pin(evaluation / "evaluation-manifest.json"):
        raise ValueError("analysis belongs to a different panel")
    if len(reference["fingerprints"]) != 72:
        raise ValueError("qualification coverage differs")
    for name, digest in reference["fingerprints"].items():
        if full["fingerprints"].get(name) != digest:
            raise ValueError(f"qualification/full output differs: {name}")
    groups = {}
    fields = ["matches", "wins", "draws", "score", "game_one_score"]
    gates = "published-44ae71e1e126b63d"
    for label, columns in [("matchups", ["cohort", "own", "opponent", "arm"]),
                           ("seats", ["cohort", "seat", "starts", "arm"]),
                           ("focal_roles", ["role", "arm"])]:
        totals = defaultdict(lambda: dict.fromkeys(fields, 0))
        for original in result["breakdown"]:
            row = dict(original)
            if label == "focal_roles":
                if row["cohort"] != "focal":
                    continue
                row["role"] = ("Gates mirror" if row["own"] == row["opponent"] == gates
                    else "Playing Gates" if row["own"] == gates else "Facing Gates")
            key = tuple(row[c] for c in columns)
            for field in fields:
                totals[key][field] += row[field]
        groups[label] = [dict(zip(columns, key), **value) for key, value in sorted(totals.items())]
    costs = {k: full[k] for k in ["staging_seconds", "execution_seconds", "recovery_seconds"]}
    write(pilot / "secondary-audit.json", dict(prefix_full_exact_match_comparisons=72,
        natural_games=result["natural_games"], full_dispatch=costs,
        actual_dispatch_seconds=sum(costs.values()), **groups,
        analysis=pin(pilot / "analysis.json"), discovery=pin(discovery / "analysis.json"),
        non_claim="Descriptive subgroup summaries only. No changed gates, pooling, promotion or human-strength claim."))

    import matplotlib
    matplotlib.use("Agg")
    import matplotlib.pyplot as plt
    figure, axes = plt.subplots(1, 3, figsize=(12, 3.8), constrained_layout=True)
    for axis, cohort, metric, title in zip(axes,
            ["focal", "canonical", "canonical"],
            ["score", "score", "game_one"],
            ["Focal BO3", "Canonical BO3", "Canonical game one"]):
        for y, (name, data, color) in enumerate([
                ("Discovery", earlier, "#607d8b"), ("Replication", result, "#1769aa")]):
            comparison = data["cohorts"][cohort]["comparisons"]["structured-minus-control"]
            value = comparison["score_difference" if metric == "score" else "game_one_difference"] * 100
            interval = comparison["paired_95_interval" if metric == "score" else "game_one_paired_95_interval"]
            low, high = [v * 100 for v in interval]
            axis.errorbar(value, y, xerr=[[value-low], [high-value]], fmt="o", capsize=5, color=color)
            axis.annotate(f"{value:+.2f} [{low:+.2f}, {high:+.2f}]", (value, y),
                          xytext=(0, 12), textcoords="offset points", ha="center", fontsize=9)
        axis.axvline(0, color="#999999", linewidth=1)
        axis.set(yticks=[0, 1], yticklabels=["Discovery", "Replication"], ylim=(-.5, 1.6),
                 title=title, xlabel="Treatment minus control (percentage points)")
        axis.margins(x=.3)
        axis.grid(axis="x", alpha=.2)
    figure.suptitle("Independent prevention replication: separate paired 95% intervals")
    figure.savefig(pilot / "replication-summary.png", dpi=180)
    figure.savefig(pilot / "replication-summary.svg")
    plt.close(figure)
    print(dict(prefix_exact=72, costs=costs, focal_roles=groups["focal_roles"]))


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    for name in ["pilot", "evaluation", "discovery"]:
        parser.add_argument("--" + name, type=Path, required=True)
    args = parser.parse_args()
    run(args.pilot, args.evaluation, args.discovery)
