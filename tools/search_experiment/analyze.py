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
import math

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


def analyze(paths, panel, seeds, budgets, worlds=128, repetitions=20000, bootstrap_seed=2026101301,
            completions=(), cpu_tolerance=.05):
    expected = {(r["id"], r["position"], seed, budget, arm)
                for r in panel for seed in seeds for budget in budgets for arm in "ABC"}
    require(repetitions > 0 and 0 <= cpu_tolerance <= .05, "invalid bootstrap count or CPU comparability envelope")
    indexed = {}
    costs = {}
    for path in completions:
        batch = json.loads(Path(path).read_text())
        for result in batch["results"]:
            if result and result.get("output_sha256"):
                costs[result["output_sha256"]] = result
    technical = []
    for path in paths:
        actual_cost = costs.get(digest(path))
        for line in Path(path).read_text(encoding="utf-8").splitlines():
            if not line.strip():
                continue
            row = json.loads(line)
            require(row.get("kind") == "search_allocation_experiment", "unexpected native result kind")
            key = (row["id"], row["position"], row["seed"], row.get("anchor", row["budget"]), row["arm"])
            require(key in expected, f"result outside frozen panel: {key}")
            if key in indexed:
                # Eval-only receipts may re-publish a selection; exact evaluation
                # identity is required before deduplicating.
                require(indexed[key]["primary_sha256"] == row["primary_sha256"]
                        and indexed[key]["evaluation_sha256"] == row["evaluation_sha256"],
                        f"conflicting duplicate result: {key}")
                continue
            indexed[key] = row
            issues = []
            if row.get("safety_cap_reached") is not False:
                issues.append("safety-cap status missing or bound")
            if row.get("technical_incomplete") is not False or row.get("selection", {}).get("faults"):
                issues.append("technical search failure or status missing")
            if not actual_cost or not math.isfinite(actual_cost["cpu_seconds"]) or actual_cost["cpu_seconds"] <= 0:
                issues.append("final full process CPU missing")
            else:
                row["actual_full_cpu_seconds"] = actual_cost["cpu_seconds"]
                row["actual_wall_seconds"] = actual_cost["wall_seconds"]
                for field in ("pure_evaluation_cpu_seconds", "execution_prep_cpu_seconds"):
                    if not isinstance(row.get(field), (float, int)) or not math.isfinite(row[field]) or row[field] < 0:
                        issues.append(f"missing/invalid cost: {field}")
                if not issues:
                    used = actual_cost["cpu_seconds"] - row["pure_evaluation_cpu_seconds"]
                    row["actual_search_execution_cpu_seconds"] = used
                    target = row.get("cpu_budget_seconds")
                    if not isinstance(target, (float, int)) or not math.isfinite(target) or target <= 0:
                        issues.append("equal CPU target missing")
                    elif abs(used - target) > cpu_tolerance * target:
                        issues.append("actual search/execution CPU outside declared envelope")
            if issues:
                technical.append({"key": key, "issues": issues})
    missing = sorted(expected - indexed.keys())
    report = {"schema": "search-sharing-allocation-analysis/v1", "complete_panel": not missing,
              "missing": missing, "root_count": len(panel), "expected_rows": len(expected),
              "observed_rows": len(indexed), "evaluation_worlds": worlds,
              "technical_failures": technical, "cpu_envelope_relative": cpu_tolerance,
              "costs": [{"key": key, "full_process_cpu_seconds": row.get("actual_full_cpu_seconds"),
                         "search_execution_cpu_seconds": row.get("actual_search_execution_cpu_seconds"),
                         "wall_seconds": row.get("actual_wall_seconds"), "target": row.get("cpu_budget_seconds"),
                         "selection_transitions": row.get("selection", {}).get("transitions")}
                        for key, row in indexed.items()],
              "bootstrap": {"unit": "root", "replicates": repetitions, "seed": bootstrap_seed,
                            "interval_percent": 97.5, "primary_anchor": 64000,
                            "nominal_family_coverage_percent": 95,
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
                    cpu_a, cpu_b = a.get("actual_search_execution_cpu_seconds"), b.get("actual_search_execution_cpu_seconds")
                    target = a.get("cpu_budget_seconds")
                    comparable = (cpu_a is not None and cpu_b is not None and isinstance(target, (float, int))
                                  and target > 0 and target == b.get("cpu_budget_seconds")
                                  and abs(cpu_a - cpu_b) <= cpu_tolerance * target)
                    row["cpu_pair_comparable"] = comparable
                    pairs.append({"root": root["id"], "position": root["position"], "seed": seed, "budget": budget, **row})
                    if budget == 64000:
                        unresolved += row["unresolved_pairs"]
                        values.append(row["difference_per_scheduled_world"])
            if len(values) == len(seeds):
                root_values[root["id"]] = statistics.mean(values)
        primary_technical = [issue for issue in technical if issue["key"][3] == 64000 and issue["key"][4] in ("A", arm)]
        primary_pairs = [pair for pair in pairs if pair["budget"] == 64000]
        ready = (not missing and unresolved == 0 and not primary_technical
                 and all(pair["cpu_pair_comparable"] for pair in primary_pairs)
                 and len(root_values) == len(panel))
        contrast = {"paired_comparisons": len(pairs), "unresolved_pairs": unresolved, "pairs": pairs,
                    "root_means": root_values, "quality_resolved": ready, "advances_quality": False}
        if len(root_values) == len(panel):
            values = list(root_values.values())
            contrast["mean_difference"] = statistics.mean(values)
            rng = random.Random(bootstrap_seed)
            draws = [statistics.mean(rng.choices(values, k=len(values))) for _ in range(repetitions)]
            interval = [quantile(draws, .0125), quantile(draws, .9875)]
            contrast["bonferroni_97_5_percent_interval"] = interval
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
    parser.add_argument("--completion", action="append", required=True, help="ordered runtime completion records with final OS CPU")
    parser.add_argument("--cpu-tolerance", type=float, default=.05)
    args = parser.parse_args()
    result = analyze(args.input, json.loads(Path(args.panel).read_text()), args.seeds.split(","),
                     [int(n) for n in args.budgets.split(",")], completions=args.completion, cpu_tolerance=args.cpu_tolerance)
    publish(args.out, result)
    print(json.dumps({"verdict": result["verdict"], "complete_panel": result["complete_panel"],
                      "advances": {k: v["advances_quality"] for k, v in result["contrasts"].items()}}))
