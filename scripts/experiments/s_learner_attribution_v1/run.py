"""S learner attribution receipt (issue #598, first milestone).

Runs the public synthetic `s_transformer_v1` learner diagnostic and writes one
small manifest with every raw sample. It is a bounded engineering diagnostic,
not training qualification:

- `abba`: balanced A-B-B-A rounds of `gae-bench` per worker count, one
  process per attempt, with the child's wall time, CPU time and peak RSS, and
  the full Adam state digests of every attempt. Optional CPU affinity.
- `parity`: `parity-dump` under both binaries in fresh directories; the
  exported `files_sha256` must match.
- `profile`: `gae-bench` under a binary built with `s-learner-profile-v1`,
  whose per-repetition attribution `summarize` condenses. Its digests must
  match the default binaries'.
- `summarize`: a Markdown table of a manifest (timings and attribution).

Standard library only. Example (binaries built beforehand):

    python scripts/experiments/s_learner_attribution_v1/run.py run \\
        --baseline base/s_transformer_v1 --candidate cand/s_transformer_v1 \\
        --profile prof/s_transformer_v1 --out receipts/t4 --workers 1,2 \\
        --seed 7 --groups 32 --reps 3 --rounds 2 --cpus 0,1
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import statistics
import subprocess
import sys
import tempfile
import time


def sha256_file(path):
    digest = hashlib.sha256()
    with open(path, "rb") as stream:
        for chunk in iter(lambda: stream.read(1 << 20), b""):
            digest.update(chunk)
    return digest.hexdigest()


def binary_identity(path):
    path = Path(path).resolve()
    return {"path": str(path), "sha256": sha256_file(path), "bytes": path.stat().st_size}


def attempt(binary, arguments, cpus):
    """One measured child: the receipt plus wall, CPU and peak RSS (KiB).

    Output goes to unnamed files, not pipes, so a large profile receipt
    cannot block the child while this process waits for its usage.
    """
    command = [str(binary), *arguments]
    if cpus:
        command = ["taskset", "-c", cpus, *command]
    with tempfile.TemporaryFile() as stdout, tempfile.TemporaryFile() as stderr:
        started = time.monotonic()
        process = subprocess.Popen(command, stdout=stdout, stderr=stderr)
        _, status, usage = os.wait4(process.pid, 0)
        wall = time.monotonic() - started
        process.returncode = os.waitstatus_to_exitcode(status)
        stdout.seek(0)
        stderr.seek(0)
        output, diagnostics = stdout.read(), stderr.read()
    if process.returncode != 0:
        tail = diagnostics.decode(errors="replace")[-2000:]
        raise RuntimeError(f"{command} exited {process.returncode}: {tail}")
    return {
        "command": command,
        "wall_seconds": wall,
        "user_seconds": usage.ru_utime,
        "system_seconds": usage.ru_stime,
        "max_rss_kib": usage.ru_maxrss,
        "receipt": json.loads(output),
    }


def gae_arguments(args, workers):
    return [
        "gae-bench",
        f"seed={args.seed}",
        f"groups={args.groups}",
        f"reps={args.reps}",
        f"workers={workers}",
    ]


def digests(receipt):
    return receipt["before_state_sha256"], receipt["after_state_sha256"]


def warm_seconds(receipt):
    """Repetitions after the first, which grows the fresh workspaces."""
    return receipt["seconds"][1:] or receipt["seconds"]


def describe(values):
    if not values:
        return None
    return {
        "n": len(values),
        "mean": statistics.fmean(values),
        "median": statistics.median(values),
        "min": min(values),
        "max": max(values),
        "stdev": statistics.stdev(values) if len(values) > 1 else 0.0,
    }


def abba(args, out):
    arms = {"A": args.baseline, "B": args.candidate}
    results = {}
    for workers in args.workers:
        attempts = []
        for round_index in range(args.rounds):
            for position, arm in enumerate("ABBA"):
                record = attempt(arms[arm], gae_arguments(args, workers), args.cpus)
                record.update({"arm": arm, "round": round_index, "position": position})
                attempts.append(record)
                print(
                    f"workers={workers} round={round_index} {arm}: "
                    f"{record['receipt']['seconds']}",
                    file=sys.stderr,
                    flush=True,
                )
        seen = {digests(a["receipt"]) for a in attempts}
        if len(seen) != 1:
            raise RuntimeError(f"workers {workers}: full-state digests differ: {seen}")
        summary = {}
        for arm in "AB":
            mine = [a for a in attempts if a["arm"] == arm]
            per_rep = [s for a in mine for s in a["receipt"]["seconds"]]
            # Warm repetitions only (each attempt's first repetition grows its
            # fresh workspaces and is kept separately).
            warm = [s for a in mine for s in warm_seconds(a["receipt"])]
            first = [a["receipt"]["seconds"][0] for a in mine]
            summary[arm] = {
                "rep_seconds": describe(per_rep),
                "warm_rep_seconds": describe(warm),
                "first_rep_seconds": describe(first),
                "process_wall_seconds": describe([a["wall_seconds"] for a in mine]),
                "process_cpu_seconds": describe(
                    [a["user_seconds"] + a["system_seconds"] for a in mine]
                ),
                "max_rss_kib": describe([a["max_rss_kib"] for a in mine]),
            }
        rounds = []
        for round_index in range(args.rounds):
            mine = [a for a in attempts if a["round"] == round_index]
            arm_mean = lambda arm: statistics.fmean(
                s for a in mine if a["arm"] == arm for s in warm_seconds(a["receipt"])
            )
            rounds.append(arm_mean("B") / arm_mean("A"))
        a_mean = summary["A"]["warm_rep_seconds"] or summary["A"]["rep_seconds"]
        b_mean = summary["B"]["warm_rep_seconds"] or summary["B"]["rep_seconds"]
        ratio = b_mean["mean"] / a_mean["mean"]
        spread = max(
            (a_mean["max"] - a_mean["min"]) / a_mean["mean"],
            (b_mean["max"] - b_mean["min"]) / b_mean["mean"],
        )
        results[str(workers)] = {
            "workers": workers,
            "state_digests": list(next(iter(seen))),
            "attempts": attempts,
            "summary": summary,
            "candidate_to_baseline_warm_mean_ratio": ratio,
            "paired_round_ratios": rounds,
            "max_relative_spread_within_arm": spread,
            "verdict": "inconclusive (difference within observed spread)"
            if abs(ratio - 1.0) <= spread
            else ("candidate faster" if ratio < 1.0 else "candidate slower"),
        }
    return results


def parity(args, out):
    rows = {}
    for arm, binary in (("A", args.baseline), ("B", args.candidate)):
        directory = out / f"parity-{arm}"
        if directory.exists():
            raise RuntimeError(f"{directory} exists; parity-dump needs a fresh directory")
        record = attempt(
            binary,
            [
                "parity-dump",
                f"out={directory}",
                f"seed={args.seed}",
                f"batches={args.parity_batches}",
                f"groups={args.parity_groups}",
            ],
            args.cpus,
        )
        rows[arm] = record["receipt"]
    identical = rows["A"]["files_sha256"] == rows["B"]["files_sha256"]
    if not identical:
        raise RuntimeError("parity-dump files_sha256 differ between baseline and candidate")
    return {"files_sha256_identical": identical, "receipts": rows}


def aggregate(values):
    """One report-shaped value from several reports of the same shape.

    Equal leaves (every count the profile derives from the workload) are kept
    exactly; differing numbers (times, and allocator counts on threads that
    race for the job queue) become their mean.
    """
    first = values[0]
    if isinstance(first, dict):
        if any(set(v) != set(first) for v in values):
            raise RuntimeError("profile reports differ in shape")
        return {key: aggregate([v[key] for v in values]) for key in first}
    if isinstance(first, list):
        if any(len(v) != len(first) for v in values):
            raise RuntimeError("profile reports differ in shape")
        return [aggregate([v[i] for v in values]) for i in range(len(first))]
    if all(v == first for v in values):
        return first
    if isinstance(first, bool) or not all(isinstance(v, (int, float)) for v in values):
        raise RuntimeError(f"profile reports differ: {values}")
    return statistics.fmean(values)


def profile(args, out, reference):
    """Profile-build attempts. Raw receipts go to `out/raw` (not part of the
    manifest); the manifest keeps the aggregate of the warm and of the first
    repetitions, plus every warm repetition's stage times."""
    raw = out / "raw"
    raw.mkdir()
    rows = {}
    for workers in args.workers:
        attempts = []
        warm = []
        first = []
        for index in range(args.profile_runs):
            record = attempt(args.profile, gae_arguments(args, workers), args.cpus)
            receipt = record["receipt"]
            if reference and digests(receipt) != tuple(reference[str(workers)]["state_digests"]):
                raise RuntimeError(f"profile build digests differ at workers {workers}")
            path = raw / f"profile-workers{workers}-{index}.json"
            path.write_text(json.dumps(receipt, indent=1) + "\n", encoding="utf-8")
            reports = receipt["profile"]
            first.append(reports[0])
            warm.extend(reports[1:] or reports)
            attempts.append(
                {
                    "command": record["command"],
                    "wall_seconds": record["wall_seconds"],
                    "user_seconds": record["user_seconds"],
                    "system_seconds": record["system_seconds"],
                    "max_rss_kib": record["max_rss_kib"],
                    "seconds": receipt["seconds"],
                    "before_state_sha256": receipt["before_state_sha256"],
                    "after_state_sha256": receipt["after_state_sha256"],
                    "raw_receipt_sha256": sha256_file(path),
                }
            )
        rows[str(workers)] = {
            "attempts": attempts,
            "warm_reports": len(warm),
            "warm_stage_ns": {
                name: [r["stages"][name]["ns"] for r in warm] for name in warm[0]["stages"]
            },
            "warm": aggregate(warm),
            "first": aggregate(first),
        }
    return rows


def host():
    info = {"platform": platform.platform(), "python": platform.python_version()}
    try:
        cpuinfo = Path("/proc/cpuinfo").read_text()
        info["cpu_model"] = next(
            line.split(":", 1)[1].strip() for line in cpuinfo.splitlines() if line.startswith("model name")
        )
        info["logical_cpus"] = os.cpu_count()
        info["mem_total_kib"] = int(
            next(line.split()[1] for line in Path("/proc/meminfo").read_text().splitlines() if line.startswith("MemTotal"))
        )
    except (OSError, StopIteration):
        pass
    return info


def command_run(args):
    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=False)
    manifest = {
        "schema": "s-learner-attribution-receipt-v1",
        "scope": "synthetic learner diagnostic (gae-bench); not end-to-end or durable-update evidence",
        "issue": "jackmaiorino/mtg-kernel#598",
        "date_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "host": host(),
        "cpus": args.cpus,
        "build": json.loads(args.build) if args.build else None,
        "workload": {
            "seed": args.seed,
            "groups": args.groups,
            "reps": args.reps,
            "workers": args.workers,
            "rounds": args.rounds,
        },
        "binaries": {
            "baseline": {"commit": args.baseline_commit, **binary_identity(args.baseline)},
            "candidate": {"commit": args.candidate_commit, **binary_identity(args.candidate)},
        },
    }
    if args.profile:
        manifest["binaries"]["profile"] = {"commit": args.candidate_commit, **binary_identity(args.profile)}
    if not args.skip_parity:
        manifest["parity"] = parity(args, out)
    manifest["abba"] = abba(args, out)
    if args.profile:
        manifest["profile"] = profile(args, out, manifest["abba"])
    path = out / "manifest.json"
    with path.open("x", encoding="utf-8") as stream:
        json.dump(manifest, stream, indent=1, allow_nan=False)
        stream.write("\n")
    print(path)
    print(summarize(manifest))


def ms(ns):
    return ns / 1e6


def summarize(manifest):
    lines = []
    workload = manifest["workload"]
    lines.append(
        f"Workload: gae-bench seed={workload['seed']} groups={workload['groups']} "
        f"reps={workload['reps']}, {workload['rounds']} A-B-B-A rounds, cpus={manifest.get('cpus')}"
    )
    if "parity" in manifest:
        lines.append(f"parity-dump files_sha256 identical: {manifest['parity']['files_sha256_identical']}")
    lines.append("")
    lines.append("| workers | arm | warm rep mean s | min | max | first rep mean s | process CPU s | peak RSS MiB |")
    lines.append("|---:|---|---:|---:|---:|---:|---:|---:|")
    for key, row in manifest["abba"].items():
        for arm in "AB":
            s = row["summary"][arm]
            warm = s["warm_rep_seconds"] or s["rep_seconds"]
            lines.append(
                f"| {key} | {arm} | {warm['mean']:.4f} | {warm['min']:.4f} | {warm['max']:.4f} | "
                f"{s['first_rep_seconds']['mean']:.4f} | {s['process_cpu_seconds']['mean']:.3f} | "
                f"{s['max_rss_kib']['mean'] / 1024:.1f} |"
            )
    lines.append("")
    for key, row in manifest["abba"].items():
        lines.append(
            f"workers={key}: B/A warm mean {row['candidate_to_baseline_warm_mean_ratio']:.4f}, "
            f"paired rounds {[round(r, 4) for r in row['paired_round_ratios']]}, "
            f"max within-arm spread {row['max_relative_spread_within_arm']:.3f}: {row['verdict']}"
        )
    if "profile" in manifest:
        lines.append("")
        lines.extend(profile_summary(manifest["profile"]))
    return "\n".join(lines)


def mean_of(reports, getter):
    return statistics.fmean(getter(r) for r in reports)


def profile_summary(profile_rows):
    lines = []
    for key, row in profile_rows.items():
        # The aggregate of the warm repetitions (each attempt's first grows
        # fresh workspaces; its aggregate is reported separately below).
        reports = [row["warm"]]
        firsts = [row["first"]]
        substeps = reports[0]["substeps"]
        stage = lambda name, field="ns": mean_of(reports, lambda r: r["stages"][name][field])
        lines.append(f"### Profile, workers={key} (mean of {row['warm_reports']} warm repetitions, {substeps} substeps)")
        lines.append("")
        lines.append("| stage | spans | mean ms | ms/substep | allocs | alloc KiB |")
        lines.append("|---|---:|---:|---:|---:|---:|")
        for name in reports[0]["stages"]:
            if stage(name, "spans") == 0:
                continue
            lines.append(
                f"| {name} | {stage(name, 'spans'):.0f} | {ms(stage(name)):.3f} | "
                f"{ms(stage(name)) / substeps:.4f} | {stage(name, 'allocs'):.0f} | "
                f"{stage(name, 'alloc_bytes') / 1024:.1f} |"
            )
        first_allocs = statistics.fmean(f["stages"]["gradient"]["allocs"] for f in firsts)
        first_bytes = statistics.fmean(f["stages"]["gradient"]["alloc_bytes"] for f in firsts)
        lines.append("")
        lines.append(
            f"First repetition (fresh workspaces): gradient allocs {first_allocs:.0f}, "
            f"{first_bytes / 1024:.1f} KiB."
        )
        lines.append("")
        lines.append("| layer | " + " | ".join(["ffn", "ln2", "proj", "attention", "qkv", "ln1"]) + " |")
        lines.append("|---|" + "---:|" * 6)
        for index, layer in enumerate(reports[0]["backward_sections"]):
            cells = []
            for section in ["ffn", "ln2", "proj", "attention", "qkv", "ln1"]:
                value = mean_of(reports, lambda r: r["backward_sections"][index][section]["ns"])
                cells.append(f"{ms(value):.3f}")
            label = f"{layer['layer']}{' (final)' if layer['final_block'] else ''}"
            lines.append(f"| {label} | " + " | ".join(cells) + " |")
        macs = reports[0]["macs"]
        total = macs["total"]
        dead = macs["final_block_object_query_rows_total"]
        all_macs = total["dx"] + total["dw"] + total["attention"]
        dead_macs = dead["dx"] + dead["dw"] + dead["attention"]
        lines.append("")
        lines.append(
            f"Backward MACs per update: dx {total['dx']:,}, dw {total['dw']:,}, attention "
            f"{total['attention']:,}; final-block object query rows {dead_macs:,} "
            f"({100 * dead_macs / all_macs:.1f}% of {all_macs:,})."
        )
        terms = reports[0]["terms"]
        if terms["linear_terms"]:
            copied = (
                terms["linear_input_bytes"]
                + terms["linear_output_gradient_bytes"]
                + terms["layer_norm_gain_bytes"]
                + terms["layer_norm_bias_bytes"]
                + terms["addition_bytes"]
            )
            lines.append(
                f"Recorded per update: {copied / 2**20:.2f} MiB of term values "
                f"(Linear inputs {terms['linear_input_bytes'] / 2**20:.2f}, Linear output gradients "
                f"{terms['linear_output_gradient_bytes'] / 2**20:.2f}, LayerNorm gains "
                f"{terms['layer_norm_gain_bytes'] / 2**20:.2f}, LayerNorm biases "
                f"{terms['layer_norm_bias_bytes'] / 2**20:.2f}, additions "
                f"{terms['addition_bytes'] / 2**20:.2f}); {terms['linear_terms']} Linear terms, "
                f"{terms['layer_norm_rows']} LayerNorm rows, {terms['additions']} additions; "
                f"peak block {terms['peak_block_live_bytes'] / 2**20:.2f} MiB live, "
                f"{terms['peak_block_reserved_bytes'] / 2**20:.2f} MiB reserved."
            )
            for kind in ["record", "replay"]:
                drain = reports[0]["drains"][kind]
                busy = [mean_of(reports, lambda r, i=i: r["drains"][kind]["busy_ns_by_worker"][i])
                        for i in range(len(drain["busy_ns_by_worker"]))]
                wall = mean_of(reports, lambda r: r["drains"][kind]["wall_ns"])
                max_busy = mean_of(reports, lambda r: r["drains"][kind]["sum_of_max_worker_busy_ns"])
                lines.append(
                    f"{kind} drains: {drain['drains']} drains, {drain['helper_spawns']} helper spawns, "
                    f"{drain['joins']} joins, {drain['spawn_failures']} spawn failures; wall "
                    f"{ms(wall):.3f} ms, busy by worker {[round(ms(b), 3) for b in busy]} ms, sum of "
                    f"per-drain max busy {ms(max_busy):.3f} ms, spawn "
                    f"{ms(mean_of(reports, lambda r: r['drains'][kind]['spawn_ns'])):.3f} ms, join wait "
                    f"{ms(mean_of(reports, lambda r: r['drains'][kind]['join_wait_ns'])):.3f} ms."
                )
            shards = reports[0]["shards"]
            total_params = sum(s["params"] for s in shards)
            total_shard_macs = sum(s["macs"] for s in shards)
            total_ns = sum(mean_of(reports, lambda r, i=i: r["shards"][i]["ns"]) for i in range(len(shards)))
            lines.append("")
            lines.append("| shard | params % | MACs % | time % | visits by worker |")
            lines.append("|---|---:|---:|---:|---|")
            order = sorted(range(len(shards)), key=lambda i: -shards[i]["params"])
            for i in order:
                s = shards[i]
                ns = mean_of(reports, lambda r, i=i: r["shards"][i]["ns"])
                lines.append(
                    f"| {s['tensor']}[{s['start_row']}..+{s['rows']}] | {100 * s['params'] / total_params:.1f} | "
                    f"{100 * s['macs'] / max(total_shard_macs, 1):.1f} | {100 * ns / max(total_ns, 1):.1f} | "
                    f"{s['visits_by_worker']} |"
                )
        heap = reports[0]["heap"]
        lines.append("")
        lines.append(
            f"Heap (process, allocator counted {reports[0]['allocator_counted']}): peak "
            f"{heap['peak_live_bytes'] / 2**20:.1f} MiB, start {heap['start_live_bytes'] / 2**20:.1f} MiB."
        )
        lines.append("")
    return lines


def command_summarize(args):
    manifest = json.loads(Path(args.manifest).read_text())
    print(summarize(manifest))


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = parser.add_subparsers(dest="command", required=True)
    run = sub.add_parser("run")
    run.add_argument("--baseline", required=True)
    run.add_argument("--candidate", required=True)
    run.add_argument("--profile")
    run.add_argument("--baseline-commit", required=True)
    run.add_argument("--candidate-commit", required=True)
    run.add_argument("--build", help="JSON object describing the shared build settings")
    run.add_argument("--out", required=True)
    run.add_argument("--workers", type=lambda s: [int(v) for v in s.split(",")], default=[1, 2])
    run.add_argument("--seed", type=int, default=7)
    run.add_argument("--groups", type=int, default=32)
    run.add_argument("--reps", type=int, default=3)
    run.add_argument("--rounds", type=int, default=2)
    run.add_argument("--profile-runs", type=int, default=2)
    run.add_argument("--parity-batches", type=int, default=1)
    run.add_argument("--parity-groups", type=int, default=4)
    run.add_argument("--skip-parity", action="store_true")
    run.add_argument("--cpus", help="taskset CPU list, e.g. 0,1")
    run.set_defaults(func=command_run)
    summary = sub.add_parser("summarize")
    summary.add_argument("manifest")
    summary.set_defaults(func=command_summarize)
    args = parser.parse_args()
    args.func(args)


if __name__ == "__main__":
    main()
