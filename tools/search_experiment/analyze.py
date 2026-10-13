"""Paired fresh-action win counts and nine-root clustered bootstrap readout.

No adaptive search terminal labels enter quality. Unknown/rejected evaluations
remain explicit and prevent an advancement verdict. The two primary contrasts
use the 64k CPU anchor only, average seeds within roots, and weight roots equally.
"""
import argparse
import json
from pathlib import Path
import random
import statistics

from launch import digest, publish, require


def quantile(values, q):
    values = sorted(values)
    index = (len(values) - 1) * q
    lo = int(index)
    hi = min(lo + 1, len(values) - 1)
    return values[lo] + (index - lo) * (values[hi] - values[lo])


def paired(a, b, worlds):
    ea, eb = a["evaluation"], b["evaluation"]
    require(ea.get("continuation") == eb.get("continuation") == "frozen-search-tree",
            "primary quality requires each arm's frozen searched continuation")
    require(ea.get("scheduled") == eb.get("scheduled") == worlds, "evaluation count differs")
    require(ea.get("seed_namespace") == eb.get("seed_namespace"), "fresh evaluation namespaces differ")
    sa, sb = ea["samples"], eb["samples"]
    require(len(sa) == len(sb) == worlds, "incomplete evaluation sample coverage")
    difference = 0
    unresolved = 0
    discordant = 0
    for left, right in zip(sa, sb):
        require((left["index"], left["world_seed"]) == (right["index"], right["world_seed"]),
                "evaluation worlds are not paired")
        valid = ("outcome" in left and "outcome" in right
                 and not left["outcome"].get("unknown", True)
                 and not right["outcome"].get("unknown", True))
        if not valid:
            unresolved += 1
            continue
        delta = int(right["outcome"]["w"]) - int(left["outcome"]["w"])
        difference += delta
        discordant += abs(delta)
    return {"paired_win_count_difference": difference, "worlds": worlds,
            "unresolved_pairs": unresolved, "discordant_pairs": discordant,
            "difference_per_scheduled_world": difference / worlds,
            "unresolved_bounds": [(difference - unresolved) / worlds, (difference + unresolved) / worlds]}


def analyze(paths, panel, seeds, budgets, worlds=128, repetitions=20000, bootstrap_seed=2026101301):
    expected = {(r["id"], r["position"], seed, budget, arm)
                for r in panel for seed in seeds for budget in budgets for arm in "ABC"}
    indexed = {}
    for path in paths:
        for line in Path(path).read_text(encoding="utf-8").splitlines():
            if not line.strip():
                continue
            row = json.loads(line)
            require(row.get("kind") == "search_allocation_experiment", "unexpected native result kind")
            key = (row["id"], row["position"], row["seed"], row["budget"], row["arm"])
            require(key in expected, f"result outside frozen panel: {key}")
            if key in indexed:
                # Eval-only receipts may re-publish a selection; exact evaluation
                # identity is required before deduplicating.
                require(indexed[key]["primary_sha256"] == row["primary_sha256"]
                        and indexed[key]["evaluation_sha256"] == row["evaluation_sha256"],
                        f"conflicting duplicate result: {key}")
                continue
            indexed[key] = row
    missing = sorted(expected - indexed.keys())
    report = {"schema": "search-sharing-allocation-analysis/v1", "complete_panel": not missing,
              "missing": missing, "root_count": len(panel), "expected_rows": len(expected),
              "observed_rows": len(indexed), "evaluation_worlds": worlds,
              "bootstrap": {"unit": "root", "replicates": repetitions, "seed": bootstrap_seed,
                            "interval_percent": 97.5, "primary_anchor": 64000,
                            "primary_contrasts": ["B-A", "C-A"], "secondary_anchor": 16000},
              "sources": [{"path": str(path), "sha256": digest(path)} for path in paths],
              "contrasts": {}, "support_failures": []}
    for key, row in indexed.items():
        if not row["teacher"].get("supported") or not row["evaluation"].get("supported"):
            report["support_failures"].append(key)
    for arm in "BC":
        pairs = []
        root_values = {}
        unresolved = 0
        for root in panel:
            values = []
            for seed in seeds:
                for budget in budgets:
                    prefix = (root["id"], root["position"], seed, budget)
                    if prefix + ("A",) not in indexed or prefix + (arm,) not in indexed:
                        continue
                    a, b = indexed[prefix + ("A",)], indexed[prefix + (arm,)]
                    for field in ("root_obs_sha256", "root_menu_sha256", "bank_binding_sha256", "key_mode", "interchange"):
                        require(a[field] == b[field], f"cross-arm root/policy binding differs: {field}")
                    row = paired(a, b, worlds)
                    pairs.append({"root": root["id"], "position": root["position"], "seed": seed, "budget": budget, **row})
                    if budget == 64000:
                        unresolved += row["unresolved_pairs"]
                        values.append(row["difference_per_scheduled_world"])
            if len(values) == len(seeds):
                root_values[root["id"]] = statistics.mean(values)
        ready = (not missing and unresolved == 0
                 and len(root_values) == len(panel))
        contrast = {"paired_comparisons": len(pairs), "unresolved_pairs": unresolved, "pairs": pairs,
                    "root_means": root_values, "quality_resolved": ready, "advances_quality": False}
        if len(root_values) == len(panel):
            values = list(root_values.values())
            contrast["mean_difference"] = statistics.mean(values)
            rng = random.Random(bootstrap_seed)
            draws = [statistics.mean(rng.choices(values, k=len(values))) for _ in range(repetitions)]
            interval = [quantile(draws, .0125), quantile(draws, .9875)]
            contrast["simultaneous_97_5_percent_interval"] = interval
            contrast["advances_quality"] = ready and interval[0] > 0
        report["contrasts"][f"{arm}-A"] = contrast
    report["verdict"] = "incomplete" if missing else ("inconclusive" if not all(c["quality_resolved"] for c in report["contrasts"].values())
                                                       else "resolved-development-panel")
    report["non_claim"] = "Development root panel only; no ordinary-start strength or learned-policy improvement claim."
    return report


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", action="append", required=True)
    parser.add_argument("--panel", required=True)
    parser.add_argument("--seeds", required=True, help="comma-separated exact PROFILE_SEED strings")
    parser.add_argument("--budgets", default="16000,64000")
    parser.add_argument("--out", required=True)
    args = parser.parse_args()
    result = analyze(args.input, json.loads(Path(args.panel).read_text()), args.seeds.split(","),
                     [int(n) for n in args.budgets.split(",")])
    publish(args.out, result)
    print(json.dumps({"verdict": result["verdict"], "complete_panel": result["complete_panel"],
                      "advances": {k: v["advances_quality"] for k, v in result["contrasts"].items()}}))
