#!/usr/bin/env python3
"""Markdown tables from a line (b) engineering receipt (line_b_receipts_v1.py).

Usage: line_b_receipts_summary_v1.py RECEIPT.json
Engineering values only; no outcome claim.
"""
import json
import statistics
import sys
from pathlib import Path


def fmt(value, digits=4):
    if value is None:
        return "n/a"
    if isinstance(value, bool):
        return "yes" if value else "NO"
    if isinstance(value, float):
        return f"{value:.{digits}g}"
    return str(value)


def table(header, rows):
    lines = ["| " + " | ".join(header) + " |", "|" + "---|" * len(header)]
    lines += ["| " + " | ".join(fmt(cell) for cell in row) + " |" for row in rows]
    return "\n".join(lines)


def quantiles(values):
    values = sorted(v for v in values if v is not None)
    if not values:
        return (None, None, None)
    return (values[0], statistics.median(values), values[-1])


def acceptance(record):
    rows = []
    for label, update in record["updates"].items():
        teacher = update["teacher"] or {}
        telemetry = teacher.get("telemetry") or {}
        envelope = teacher.get("cuda_envelope") or {}
        gauge = update["scorer_bias_gauge"]
        rescore = [
            root.get("signed_difference")
            for root in telemetry.get("roots", [])
            if root.get("signed_difference") is not None
        ]
        rows.append(
            [
                label,
                update["after_state_sha256"][:12],
                update["adam_step"],
                gauge["raw_gradient_residual"],
                gauge["derived_absolute_bound"],
                envelope.get("max_abs_log_probability_discrepancy"),
                telemetry.get("head_l2_ratio"),
                statistics.mean(rescore) if rescore else None,
                update["seconds"],
            ]
        )
    out = [
        "Updates (engineering values):",
        table(
            [
                "update",
                "after state",
                "Adam step",
                "gauge residual",
                "gauge bound",
                "CUDA envelope max",
                "head L2 ratio",
                "mean rescore change",
                "seconds",
            ],
            rows,
        ),
    ]
    out.append("\nChecks:")
    checks = record.get("checks", {})
    flat = []
    for key, value in checks.items():
        if isinstance(value, dict):
            flat += [(f"{key}: {k}", v) for k, v in value.items()]
        else:
            flat.append((key, value))
    out.append(table(["check", "holds"], flat))
    return "\n".join(out)


def chain(record):
    rows = []
    for key, steps in record["chains"].items():
        rows.append(
            [
                key,
                len(steps),
                steps[-1]["after_state_sha256"][:12] if steps else None,
                sum(s["seconds"] for s in steps),
                sum(s["collect_seconds"] for s in steps),
            ]
        )
    checks = []
    for key, value in record.get("checks", {}).items():
        if key == "replays":
            for arm, replay in value.items():
                checks.append((f"{arm}: {replay['compared']}, identical at every update", replay["identical"]))
                checks.append((f"{arm}: update 0 trajectory files byte-identical", replay["update_0_trajectory_files_equal"]))
        else:
            checks.append((key, value))
    return "\n".join(
        [
            table(["chain", "updates", "final state", "update seconds", "collect seconds"], rows),
            "",
            table(["check", "holds"], checks),
        ]
    )


def throughput(record):
    runs = sorted(record["runs"].values(), key=lambda r: r["workers"])
    base = runs[0]["teacher_seconds"] if runs else None
    rows = [
        [
            r["workers"],
            r["roots"],
            r["rollouts"],
            r["teacher_seconds"],
            r["rollouts_per_second"],
            base / r["teacher_seconds"] if base and r["teacher_seconds"] else None,
            r["physical_decisions_mean"],
            r["physical_decisions_max"],
            r["census"]["censored_rollouts"] / r["census"]["rollouts"] if r["census"]["rollouts"] else None,
            r.get("host_cpu_percent_before"),
        ]
        for r in runs
    ]
    placement = record.get("placement", {})
    best = max((r["rollouts_per_second"] or 0.0 for r in runs), default=0.0)
    return "\n".join(
        [
            f"Placement: priority {placement.get('priority', 'n/a')}, affinity {placement.get('affinity', 'n/a')}.",
            f"Best teach-step rate {fmt(best)} rollouts per second against the cap's 1.48 completed"
            " rollouts per reserved host-second (256,000 rollouts in 48 host-hours); the teach"
            " step alone, before collection and update time.",
            "",
            table(
                [
                    "workers",
                    "roots",
                    "rollouts",
                    "teach step seconds",
                    "rollouts per second",
                    "speedup vs first row",
                    "decisions per rollout (mean)",
                    "decisions per rollout (max)",
                    "censored rollout fraction",
                    "host CPU percent before the run",
                ],
                rows,
            ),
            "",
            table(["check", "holds"], list(record.get("checks", {}).items())),
        ]
    )


def diagnostics(record):
    by_list = {}
    ratios = []
    for update in record["updates"]:
        ratios.append(update.get("head_l2_ratio"))
        for root in update["roots"]:
            by_list.setdefault(root["list"], []).append(root)
    rows = []
    all_roots = []
    for name, roots in sorted(by_list.items()):
        all_roots += roots
        rows.append(summary_row(name, roots))
    rows.append(summary_row("all", all_roots))
    low, mid, high = quantiles(ratios)
    return "\n".join(
        [
            table(
                [
                    "learner list",
                    "roots",
                    "complete",
                    "censored",
                    "rollouts",
                    "censored rollouts",
                    "student p_max (mean)",
                    "student entropy (mean, nats)",
                    "target p_max (mean)",
                    "target entropy (mean, nats)",
                    "complete roots with tied mean returns (target equals the student)",
                    "mean return spread (mean over complete roots)",
                    "decisions per rollout (median)",
                ],
                rows,
            ),
            "",
            f"Head L2 ratio (teacher over ordinary, per update): min {fmt(low)}, median {fmt(mid)}, max {fmt(high)} over {len(ratios)} updates.",
        ]
    )


def summary_row(name, roots):
    complete = [r for r in roots if r["status"] == "complete"]
    outcomes = [kind for r in roots for kind in r["rollout_outcomes"]]
    decisions = [d for r in roots for d in r["rollout_decisions"]]

    def mean(key, subset):
        values = [r[key] for r in subset if r[key] is not None]
        return statistics.mean(values) if values else None

    return [
        name,
        len(roots),
        len(complete),
        len(roots) - len(complete),
        len(outcomes),
        sum(1 for kind in outcomes if kind == "censored"),
        mean("student_p_max", roots),
        mean("student_entropy", roots),
        mean("target_p_max", complete),
        mean("target_entropy", complete),
        sum(1 for r in complete if r.get("mean_return_spread") == 0.0),
        mean("mean_return_spread", complete) if all("mean_return_spread" in r for r in complete) else None,
        statistics.median(decisions) if decisions else None,
    ]


def main():
    record = json.loads(Path(sys.argv[1]).read_text())
    schema = record["schema"]
    kind = schema.split("line-b-engineering-")[1].split("-receipt")[0]
    print(f"# {record['name']} ({schema})\n")
    print({"acceptance": acceptance, "chain": chain, "throughput": throughput, "diagnostics": diagnostics}[kind](record))


if __name__ == "__main__":
    main()
