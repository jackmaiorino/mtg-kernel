"""S learner attribution receipt (issue #598, first milestone).

Runs the public synthetic `s_transformer_v1` learner diagnostic and writes one
small manifest with every raw timing sample. It is a bounded engineering
diagnostic, not training qualification:

- `parity`: `parity-dump` under both binaries in fresh directories; the
  exported `files_sha256` must match.
- `abba`: balanced A-B-B-A rounds of `gae-bench` per worker count, one
  process per attempt, with the child's wall time, CPU time, peak RSS and page
  faults. Every attempt's receipt, including both full train-state digests,
  must match apart from its times and the requested worker count.
- `profile`: `gae-bench` under a binary built with `s-learner-profile-v1`.
  Its digests must match the default binaries'. Raw receipts go to
  `<out>/raw`; the manifest keeps their aggregate and every repetition's stage
  times.
- `summarize`: Markdown tables of a manifest (timings and attribution).

Standard library only; Linux, native Windows or macOS. `--cpus` takes a
taskset-style list (`8-11`, `0,2`): Linux pins each child with `taskset`,
Windows pins this process and its children inherit it; macOS has no affinity. Example (binaries built beforehand; add
`.exe` on Windows):

    python scripts/experiments/s_learner_attribution_v1/run.py run \\
        --baseline base/s_transformer_v1 --candidate cand/s_transformer_v1 \\
        --profile prof/s_transformer_v1 --baseline-commit <sha> \\
        --candidate-commit <sha> --out receipts/t4 --workers 1,2 --seed 7 \\
        --groups 32 --reps 3 --rounds 4 --profile-runs 4 --cpus 2,3
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
    return {"path": str(path), "sha256": sha256_file(path), "bytes": path.stat().st_size}


def cpu_list(cpus):
    """Logical CPU numbers from a taskset-style list such as `8-11` or `0,2`."""
    numbers = set()
    for part in cpus.split(","):
        first, _, last = part.partition("-")
        numbers.update(range(int(first), int(last or first) + 1))
    return sorted(numbers)


def pin_windows(cpus):
    """Pins this process, and so every child it starts, to `cpus`."""
    import ctypes

    kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
    kernel32.GetCurrentProcess.restype = ctypes.c_void_p
    kernel32.SetProcessAffinityMask.argtypes = [ctypes.c_void_p, ctypes.c_size_t]
    mask = sum(1 << cpu for cpu in cpu_list(cpus))
    if not kernel32.SetProcessAffinityMask(kernel32.GetCurrentProcess(), mask):
        raise ctypes.WinError(ctypes.get_last_error())


def windows_usage(process):
    """CPU times, peak working set and page faults of an exited child."""
    import ctypes
    from ctypes import wintypes

    class Counters(ctypes.Structure):
        _fields_ = [
            ("cb", wintypes.DWORD),
            ("PageFaultCount", wintypes.DWORD),
            ("PeakWorkingSetSize", ctypes.c_size_t),
            ("WorkingSetSize", ctypes.c_size_t),
            ("QuotaPeakPagedPoolUsage", ctypes.c_size_t),
            ("QuotaPagedPoolUsage", ctypes.c_size_t),
            ("QuotaPeakNonPagedPoolUsage", ctypes.c_size_t),
            ("QuotaNonPagedPoolUsage", ctypes.c_size_t),
            ("PagefileUsage", ctypes.c_size_t),
            ("PeakPagefileUsage", ctypes.c_size_t),
        ]

    kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
    handle = ctypes.c_void_p(int(process._handle))
    times = [wintypes.FILETIME() for _ in range(4)]
    kernel32.GetProcessTimes.argtypes = [ctypes.c_void_p] + [ctypes.POINTER(wintypes.FILETIME)] * 4
    if not kernel32.GetProcessTimes(handle, *(ctypes.byref(t) for t in times)):
        raise ctypes.WinError(ctypes.get_last_error())
    counters = Counters()
    counters.cb = ctypes.sizeof(counters)
    kernel32.K32GetProcessMemoryInfo.argtypes = [ctypes.c_void_p, ctypes.POINTER(Counters), wintypes.DWORD]
    if not kernel32.K32GetProcessMemoryInfo(handle, ctypes.byref(counters), counters.cb):
        raise ctypes.WinError(ctypes.get_last_error())
    seconds = lambda t: ((t.dwHighDateTime << 32) | t.dwLowDateTime) / 1e7
    return {
        "user_seconds": seconds(times[3]),
        "system_seconds": seconds(times[2]),
        "max_rss_kib": counters.PeakWorkingSetSize // 1024,
        # Windows counts soft and hard faults together.
        "page_faults": counters.PageFaultCount,
        "involuntary_switches": None,
    }


def attempt(binary, arguments, cpus):
    """One measured child: the receipt plus wall, CPU and peak RSS (KiB).

    Output goes to unnamed files, not pipes, so a large profile receipt
    cannot block the child while this process waits for its usage. Linux
    pins each child with `taskset`; Windows pins this process once (see
    `main`) and children inherit it.
    """
    command = [str(binary), *arguments]
    if cpus and os.name != "nt":
        command = ["taskset", "-c", cpus, *command]
    with tempfile.TemporaryFile() as stdout, tempfile.TemporaryFile() as stderr:
        started = time.monotonic()
        process = subprocess.Popen(command, stdout=stdout, stderr=stderr)
        if os.name == "nt":
            process.wait()
            wall = time.monotonic() - started
            usage = windows_usage(process)
        else:
            _, status, rusage = os.wait4(process.pid, 0)
            wall = time.monotonic() - started
            process.returncode = os.waitstatus_to_exitcode(status)
            usage = {
                "user_seconds": rusage.ru_utime,
                "system_seconds": rusage.ru_stime,
                # Linux reports KiB, macOS bytes.
                "max_rss_kib": rusage.ru_maxrss // (1024 if sys.platform == "darwin" else 1),
                "page_faults": rusage.ru_minflt + rusage.ru_majflt,
                "involuntary_switches": rusage.ru_nivcsw,
            }
        stdout.seek(0)
        stderr.seek(0)
        output, diagnostics = stdout.read(), stderr.read()
    if process.returncode != 0:
        tail = diagnostics.decode(errors="replace")[-2000:]
        raise RuntimeError(f"{command} exited {process.returncode}: {tail}")
    return {"command": command, "wall_seconds": wall, **usage, "receipt": json.loads(output)}


def gae_arguments(args, workers):
    return [
        "gae-bench",
        f"seed={args.seed}",
        f"groups={args.groups}",
        f"reps={args.reps}",
        f"workers={workers}",
    ]


def learner_identity(receipt, args, workers, *, profiled):
    actual = (receipt["seed"], receipt["groups"], receipt["workers"], len(receipt["seconds"]))
    expected = (args.seed, args.groups, workers, args.reps)
    if actual != expected:
        raise RuntimeError(f"gae-bench fixture differs: {actual} vs {expected}")
    if ("profile" in receipt) != profiled or (profiled and len(receipt["profile"]) != args.reps):
        raise RuntimeError(f"gae-bench profile shape differs at workers {workers}")
    return {key: value for key, value in receipt.items() if key not in ("seconds", "profile")}


def warm_seconds(seconds):
    """Repetitions after a process's first. Every repetition starts from a
    clone of the warmed state with fresh workspaces; the first also pays the
    process's allocator, page and cache warm-up."""
    return seconds[1:] or seconds


def describe(values):
    values = [v for v in values if v is not None]
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
    worker_reference = None
    for workers in args.workers:
        attempts = []
        commands = {}
        common = None
        for round_index in range(args.rounds):
            for position, arm in enumerate("ABBA"):
                record = attempt(arms[arm], gae_arguments(args, workers), args.cpus)
                receipt = record.pop("receipt")
                commands[arm] = record.pop("command")
                # Everything but the times must be identical in every attempt,
                # including both full train-state digests.
                rest = learner_identity(receipt, args, workers, profiled=False)
                if common is None:
                    comparable = {key: value for key, value in rest.items() if key != "workers"}
                    if worker_reference is None:
                        worker_reference = comparable
                    elif comparable != worker_reference:
                        raise RuntimeError(f"workers {workers}: fixture or full train-state digest differs across worker counts")
                    common = rest
                elif rest != common:
                    raise RuntimeError(f"workers {workers}: receipts differ: {rest} vs {common}")
                record.update(
                    {"arm": arm, "round": round_index, "position": position, "seconds": receipt["seconds"]}
                )
                attempts.append(record)
                print(
                    f"workers={workers} round={round_index} {arm}: {receipt['seconds']}",
                    file=sys.stderr,
                    flush=True,
                )
        summary = {}
        for arm in "AB":
            mine = [a for a in attempts if a["arm"] == arm]
            per_rep = [s for a in mine for s in a["seconds"]]
            # Warm-process repetitions only (each attempt's first repetition
            # pays process warm-up and is kept separately).
            warm = [s for a in mine for s in warm_seconds(a["seconds"])]
            first = [a["seconds"][0] for a in mine]
            summary[arm] = {
                "rep_seconds": describe(per_rep),
                "warm_rep_seconds": describe(warm),
                "first_rep_seconds": describe(first),
                "process_wall_seconds": describe([a["wall_seconds"] for a in mine]),
                "process_cpu_seconds": describe(
                    [a["user_seconds"] + a["system_seconds"] for a in mine]
                ),
                "max_rss_kib": describe([a["max_rss_kib"] for a in mine]),
                "page_faults": describe([a["page_faults"] for a in mine]),
                "involuntary_switches": describe([a["involuntary_switches"] for a in mine]),
            }
        rounds = []
        for round_index in range(args.rounds):
            mine = [a for a in attempts if a["round"] == round_index]
            arm_mean = lambda arm: statistics.fmean(
                s for a in mine if a["arm"] == arm for s in warm_seconds(a["seconds"])
            )
            rounds.append(arm_mean("B") / arm_mean("A"))
        a_warm = summary["A"]["warm_rep_seconds"]
        b_warm = summary["B"]["warm_rep_seconds"]
        ratio = b_warm["mean"] / a_warm["mean"]
        spread = max(
            (a_warm["max"] - a_warm["min"]) / a_warm["mean"],
            (b_warm["max"] - b_warm["min"]) / b_warm["mean"],
        )
        results[str(workers)] = {
            "workers": workers,
            "commands": commands,
            "receipt_common": common,
            "attempts": attempts,
            "summary": summary,
            "candidate_to_baseline_warm_mean_ratio": ratio,
            "candidate_to_baseline_warm_median_ratio": b_warm["median"] / a_warm["median"],
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
    exactly; differing numbers (times, and counts that depend on which worker
    takes which job) become their mean. Per-worker count lists end at the
    last worker that did something, so shorter ones are padded with zeros.
    """
    first = values[0]
    if isinstance(first, dict):
        if any(set(v) != set(first) for v in values):
            raise RuntimeError("profile reports differ in shape")
        return {key: aggregate([v[key] for v in values]) for key in first}
    if isinstance(first, list):
        width = max(len(v) for v in values)
        if any(len(v) != width for v in values):
            numeric = all(isinstance(x, (int, float)) and not isinstance(x, bool) for v in values for x in v)
            if not numeric:
                raise RuntimeError("profile reports differ in shape")
            values = [v + [0] * (width - len(v)) for v in values]
        return [aggregate([v[i] for v in values]) for i in range(width)]
    if all(v == first for v in values):
        return first
    if isinstance(first, bool) or not all(isinstance(v, (int, float)) for v in values):
        raise RuntimeError(f"profile reports differ: {values}")
    return statistics.fmean(values)


def profile(args, out, reference):
    """Profile-build attempts. Raw receipts go to `out/raw` (not part of the
    manifest); the manifest keeps the aggregate of the warm-process and of
    the first repetitions, plus every warm-process repetition's stage
    times."""
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
            identity = learner_identity(receipt, args, workers, profiled=True)
            if identity != reference[str(workers)]["receipt_common"]:
                raise RuntimeError(f"profile build fixture or full train-state digest differs at workers {workers}")
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
                    "page_faults": record["page_faults"],
                    "involuntary_switches": record["involuntary_switches"],
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
            # First repetitions differ in time and allocations only.
            "first": {key: aggregate([r[key] for r in first]) for key in ("stages", "heap")},
        }
    return rows


def host():
    info = {
        "platform": platform.platform(),
        "python": platform.python_version(),
        "cpu_model": platform.processor() or None,
        "logical_cpus": os.cpu_count(),
    }
    try:
        cpuinfo = Path("/proc/cpuinfo").read_text()
        info["cpu_model"] = next(
            line.split(":", 1)[1].strip() for line in cpuinfo.splitlines() if line.startswith("model name")
        )
        info["mem_total_kib"] = int(
            next(line.split()[1] for line in Path("/proc/meminfo").read_text().splitlines() if line.startswith("MemTotal"))
        )
    except (OSError, StopIteration):
        pass
    return info


def command_run(args):
    args.baseline = Path(args.baseline).resolve(strict=True)
    args.candidate = Path(args.candidate).resolve(strict=True)
    if args.profile:
        args.profile = Path(args.profile).resolve(strict=True)
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
    lines.append(
        "| workers | arm | warm rep mean s | median | min | max | first rep mean s | process CPU s | peak RSS MiB | page faults |"
    )
    lines.append("|---:|---|---:|---:|---:|---:|---:|---:|---:|---:|")
    for key, row in manifest["abba"].items():
        for arm in "AB":
            s = row["summary"][arm]
            warm = s["warm_rep_seconds"]
            lines.append(
                f"| {key} | {arm} | {warm['mean']:.4f} | {warm['median']:.4f} | {warm['min']:.4f} | {warm['max']:.4f} | "
                f"{s['first_rep_seconds']['mean']:.4f} | {s['process_cpu_seconds']['mean']:.3f} | "
                f"{s['max_rss_kib']['mean'] / 1024:.1f} | {s['page_faults']['mean']:.0f} |"
            )
    lines.append("")
    for key, row in manifest["abba"].items():
        lines.append(
            f"workers={key}: B/A warm mean {row['candidate_to_baseline_warm_mean_ratio']:.4f}, "
            f"median {row['candidate_to_baseline_warm_median_ratio']:.4f}, "
            f"paired rounds {[round(r, 4) for r in row['paired_round_ratios']]}, "
            f"max within-arm spread {row['max_relative_spread_within_arm']:.3f}: {row['verdict']}"
        )
    if "profile" in manifest:
        lines.append("")
        lines.extend(profile_summary(manifest["profile"]))
    return "\n".join(lines)


def gemm_macs(macs):
    return macs["dx"] + macs["dw"] + macs["attention"]


def backward_macs_summary(macs, substeps):
    """The backward's GEMM multiply-adds. `total` counts the work that ran.
    Reports before the compact final block (no `final_block_removed_total`)
    ran every final-block object query row."""
    total = macs["total"]
    ran = gemm_macs(total)
    unused = gemm_macs(macs["final_block_object_query_rows_total"])
    removed_total = macs.get("final_block_removed_total")
    removed = gemm_macs(removed_total) if removed_total else 0
    full = ran + removed
    line = (
        f"Backward GEMM MACs per update: dx {total['dx']:,}, dw {total['dw']:,}, attention "
        f"{total['attention']:,} ({ran:,} run); final-block object query rows {unused:,} "
        f"({100 * unused / full:.1f}% of the full backward's {full:,})"
    )
    if removed_total is None:
        return line + "."
    live = macs["final_block_live_substeps"]
    return line + f", {removed:,} of them skipped ({live} of {substeps} substeps compact)."


def mean_of(reports, getter):
    return statistics.fmean(getter(r) for r in reports)


def profile_summary(profile_rows):
    lines = []
    for key, row in profile_rows.items():
        # The aggregate of the warm-process repetitions (each attempt's first
        # pays process warm-up; its aggregate is reported separately below).
        reports = [row["warm"]]
        firsts = [row["first"]]
        substeps = reports[0]["substeps"]
        stage = lambda name, field="ns": mean_of(reports, lambda r: r["stages"][name][field])
        lines.append(f"### Profile, workers={key} (mean of {row['warm_reports']} warm-process repetitions, {substeps} substeps)")
        lines.append("")
        lines.append("Gradient allocation columns count only the caller thread.")
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
            f"First repetition of each process: caller-thread gradient allocs {first_allocs:.0f}, "
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
        lines.append("")
        lines.append(backward_macs_summary(macs, substeps))
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
                f"peak active block {terms['peak_block_live_bytes'] / 2**20:.2f} MiB live, "
                f"{terms['peak_block_reserved_bytes'] / 2**20:.2f} MiB reserved."
            )
            if "peak_retained_term_capacity_bytes" in terms:
                lines.append(
                    f"Peak retained term capacity across all slots: "
                    f"{terms['peak_retained_term_capacity_bytes'] / 2**20:.2f} MiB."
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
    if getattr(args, "cpus", None) and sys.platform == "darwin":
        parser.error("macOS has no CPU affinity; run without --cpus")
    if getattr(args, "cpus", None) and os.name == "nt":
        pin_windows(args.cpus)
    args.func(args)


if __name__ == "__main__":
    main()
