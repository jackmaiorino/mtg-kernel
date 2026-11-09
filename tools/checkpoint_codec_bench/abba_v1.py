"""A-B-B-A runner and comparison for the checkpoint codec benchmark (mtg-kernel#596).

`run` measures one retained fixture with a baseline and a candidate build of
`examples/checkpoint_codec_bench_v1.rs`, alternating baseline, candidate,
candidate, baseline for each round, and keeps every attempt: its command,
exit status, wall time, output and receipt. `compare` reads receipts back.

The comparison refuses receipts that did not measure identical work: the same
fixture manifest, the same measurement settings, every in-run check passed,
the same published bytes for every (state, cell) and the same real-update
after-state and checkpoint content. Then, for every stage and metric, each
run contributes its own median; a role's value is the median of its runs'
medians. The candidate is reported lower or higher only when every candidate
run lies beyond every baseline run (the ranges do not overlap) and that
separation would happen by chance, if the role made no difference, with
probability at most alpha (default 0.01): with n baseline and m candidate
runs that probability is 2 n! m! / (n + m)!, so one round (2 + 2 runs, 1/3)
can never decide and three rounds (6 + 6, about 0.002) are the minimum at
the default. Anything else is inconclusive. A same-binary (A/A) run measures
the run-to-run variation and how many rows a null comparison calls.

Standard library only, Python >= 3.9 (macOS's own python3 is 3.9);
`docs/checkpoint_codec_bench_v1.md` has the commands.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import math
import os
import platform
import shutil
import statistics
import subprocess
import sys
import time
from pathlib import Path

RECEIPT_SCHEMA = "mtg-kernel-checkpoint-codec-bench/v1"
RUN_SCHEMA = "mtg-kernel-checkpoint-codec-bench-abba/v1"
COMPARISON_SCHEMA = "mtg-kernel-checkpoint-codec-bench-comparison/v1"
ROLES = ("baseline", "candidate")
METRICS = ("wall_ns", "cpu_ns", "allocated_bytes", "allocations", "peak_heap_bytes",
           "retained_heap_bytes")
CHECK_IDENTITIES = ("state_sha256", "after_state_sha256", "v1_json_sha256", "v1_sha256", "v2_sha256")
ALPHA = 0.01


def require(condition, message):
    if not condition:
        raise ValueError(message)


def sha256_file(path):
    digest = hashlib.sha256()
    with open(path, "rb") as stream:
        for block in iter(lambda: stream.read(1 << 20), b""):
            digest.update(block)
    return digest.hexdigest()


def pin(path):
    path = Path(path).resolve()
    require(path.is_file(), f"not a file: {path}")
    return {"path": str(path), "sha256": sha256_file(path), "bytes": path.stat().st_size}


def abba_order(rounds):
    require(isinstance(rounds, int) and rounds >= 1, "rounds must be a positive integer")
    return [role for _ in range(rounds) for role in ("baseline", "candidate", "candidate", "baseline")]


def command_output(command):
    """A tool's version output, or None when it is not installed."""
    if shutil.which(command[0]) is None:
        return None
    try:
        completed = subprocess.run(command, capture_output=True, text=True, timeout=60)
    except (OSError, subprocess.SubprocessError) as error:
        return f"unavailable: {error}"
    return (completed.stdout + completed.stderr).strip()


def toolchain():
    return {
        "rustc": command_output(["rustc", "-vV"]),
        "cargo": command_output(["cargo", "-V"]),
        "cc": command_output(["cc", "--version"]),
        # Apple's linker has no --version.
        "ld": command_output(["ld", "-v"] if sys.platform == "darwin" else ["ld", "--version"]),
    }


def machine():
    record = {"platform": platform.platform(), "machine": platform.machine(),
              "processor": platform.processor(), "logical_cpus": os.cpu_count(),
              "python": sys.version}
    for name, key in (("/proc/cpuinfo", "model name"), ("/proc/meminfo", "MemTotal")):
        try:
            with open(name, encoding="utf-8") as stream:
                line = next((line for line in stream if line.startswith(key)), None)
        except OSError:
            line = None
        if line is not None:
            record[key.replace(" ", "_").lower()] = line.split(":", 1)[1].strip()
    if sys.platform == "darwin":
        record["sysctl"] = {name: command_output(["sysctl", "-n", name])
                            for name in ("machdep.cpu.brand_string", "hw.memsize", "hw.perflevel0.physicalcpu",
                                         "hw.perflevel1.physicalcpu")}
    return record


def write_json(path, value):
    data = json.dumps(value, indent=2, sort_keys=True) + "\n"
    temporary = Path(str(path) + ".tmp")
    temporary.write_text(data, encoding="utf-8")
    os.replace(temporary, path)


def load_receipt(path):
    with open(path, encoding="utf-8") as stream:
        receipt = json.load(stream)
    require(receipt.get("schema") == RECEIPT_SCHEMA, f"{path}: not a {RECEIPT_SCHEMA} receipt")
    return receipt


def identity_of(receipt):
    """What must be equal for two receipts to have measured identical work."""
    config = dict(receipt["config"])
    config.pop("public_pair", None)  # a path; its files are pinned by the pair check
    return {"fixture_manifest_sha256": receipt["fixture"]["manifest_sha256"], "config": config}


def _spread(values):
    middle = statistics.median(values)
    return None if middle == 0 else (max(values) - min(values)) / middle


def chance_separation(baseline_runs, candidate_runs):
    """The probability that `baseline_runs` and `candidate_runs` distinct
    values separate completely, either way round, when every ordering is
    equally likely (the role makes no difference): 2 n! m! / (n + m)!."""
    n, m = baseline_runs, candidate_runs
    require(n >= 1 and m >= 1, "each role needs at least one run")
    return min(1.0, 2 * math.factorial(n) * math.factorial(m) / math.factorial(n + m))


def rounds_needed(alpha):
    """The fewest A-B-B-A rounds whose 2r + 2r runs can reach `alpha`."""
    rounds = 1
    while chance_separation(2 * rounds, 2 * rounds) > alpha:
        rounds += 1
    return rounds


def verdict(baseline, candidate, alpha=ALPHA):
    if chance_separation(len(baseline), len(candidate)) > alpha:
        return "inconclusive"
    if max(candidate) < min(baseline):
        return "candidate_lower"
    if min(candidate) > max(baseline):
        return "candidate_higher"
    return "inconclusive"


def compare_metric(baseline, candidate, alpha=ALPHA):
    base, cand = statistics.median(baseline), statistics.median(candidate)
    return {"baseline_runs": baseline, "candidate_runs": candidate,
            "baseline_median": base, "candidate_median": cand,
            "ratio": None if base == 0 else cand / base,
            "baseline_spread": _spread(baseline), "candidate_spread": _spread(candidate),
            "chance_separation": chance_separation(len(baseline), len(candidate)),
            "verdict": verdict(baseline, candidate, alpha)}


def compare(entries, alpha=ALPHA):
    """Compares `entries`, a list of (role, receipt, label) in run order."""
    require(entries, "no receipts to compare")
    require(0 < alpha < 1, "alpha must lie between 0 and 1")
    roles = {role for role, _, _ in entries}
    require(roles <= set(ROLES), f"unknown roles {sorted(roles - set(ROLES))}")
    require(roles == set(ROLES), "the comparison needs baseline and candidate receipts")
    first_identity = identity_of(entries[0][1])
    published = None
    check_identities = {}
    series = {}
    process = {role: {"peak_rss_bytes": [], "section_seconds": {}} for role in ROLES}
    for role, receipt, label in entries:
        require(identity_of(receipt) == first_identity,
                f"{label}: measured another fixture or other settings than {entries[0][2]}")
        failed = [check for check in receipt["checks"] if check.get("passed") is not True]
        require(not failed, f"{label}: in-run checks failed: {failed}")
        these = {(item["state"], item["cell"]): (item["sha256"], item["bytes"])
                 for item in receipt["published"]}
        if published is None:
            published = these
        require(these == published, f"{label}: published bytes differ from {entries[0][2]}")
        for check in receipt["checks"]:
            for field in CHECK_IDENTITIES:
                if field in check:
                    key = (check.get("section", ""), check["state"], field)
                    held = check_identities.setdefault(key, check[field])
                    require(held == check[field], f"{label}: {' '.join(key)} differs")
        for item in receipt["series"]:
            key = (item["section"], item["state"], item["cell"], item["stage"])
            for metric in METRICS:
                summary = item["summary"].get(metric)
                if summary is not None:
                    series.setdefault(key, {}).setdefault(metric, {"baseline": [], "candidate": []})[
                        role].append(summary["median"])
        rss = receipt.get("process_peak_rss_bytes")
        if rss is not None:
            process[role]["peak_rss_bytes"].append(rss)
        for section, seconds in receipt.get("section_seconds", {}).items():
            process[role]["section_seconds"].setdefault(section, []).append(seconds)
    rows = []
    for key, metrics in sorted(series.items()):
        compared = {metric: compare_metric(runs["baseline"], runs["candidate"], alpha)
                    for metric, runs in metrics.items() if runs["baseline"] and runs["candidate"]}
        rows.append({"section": key[0], "state": key[1], "cell": key[2], "stage": key[3],
                     "metrics": compared})
    whole = {}
    if process["baseline"]["peak_rss_bytes"] and process["candidate"]["peak_rss_bytes"]:
        whole["process_peak_rss_bytes"] = compare_metric(process["baseline"]["peak_rss_bytes"],
                                                         process["candidate"]["peak_rss_bytes"], alpha)
    for section in sorted(set(process["baseline"]["section_seconds"])
                          & set(process["candidate"]["section_seconds"])):
        whole[f"section_seconds.{section}"] = compare_metric(
            process["baseline"]["section_seconds"][section],
            process["candidate"]["section_seconds"][section], alpha)
    runs_per_role = {role: sum(1 for r, _, _ in entries if r == role) for role in ROLES}
    chance = chance_separation(runs_per_role["baseline"], runs_per_role["candidate"])
    return {
        "schema": COMPARISON_SCHEMA,
        "rule": "per run, the stage's median; per role, the median of its runs; the candidate is lower "
                "or higher only when no candidate run overlaps any baseline run and such a separation "
                "has chance probability 2 n! m! / (n + m)! <= alpha, else inconclusive",
        "alpha": alpha,
        "chance_separation": chance,
        "decidable": chance <= alpha,
        "rounds_needed": rounds_needed(alpha),
        "claim": "codec, publication and readback stage costs and real-update costs on one fixture; "
                 "not an updates-per-hour, full-node or fleet claim",
        "runs": [{"role": role, "label": label} for role, _, label in entries],
        "runs_per_role": runs_per_role,
        "identity": first_identity,
        "published": [{"state": state, "cell": cell, "sha256": sha256, "bytes": length}
                      for (state, cell), (sha256, length) in sorted(published.items())],
        "check_identities": [{"section": section, "state": state, "field": field, "value": value}
                             for (section, state, field), value in sorted(check_identities.items())],
        "rows": rows,
        "process": whole,
    }


def format_table(comparison, metric="wall_ns"):
    """The stage rows for `metric`, then the whole-process rows (peak RSS in
    MiB, each section's seconds), with the run counts and the chance rule."""
    scale = {"wall_ns": 1e6, "cpu_ns": 1e6}.get(metric, 1.0)
    runs = comparison["runs_per_role"]
    chance, alpha = comparison["chance_separation"], comparison["alpha"]
    counts = f"{runs['baseline']} baseline and {runs['candidate']} candidate runs"
    if comparison["decidable"]:
        header = f"{counts}; a complete separation has chance probability {chance:.4f} <= alpha {alpha}"
    else:
        header = (f"{counts}: a complete separation has chance probability {chance:.4f} > alpha {alpha}, "
                  f"so every row is inconclusive; deciding needs --rounds {comparison['rounds_needed']} "
                  f"or more")
    lines = [header,
             f"{'section':<12} {'state':<11} {'cell':<9} {'stage':<24} {'base':>11} {'cand':>11} "
             f"{'ratio':>7} {'b-spr':>6} {'c-spr':>6}  verdict  ({metric}"
             f"{', ms' if scale == 1e6 else ''})"]
    results = [((row["section"], row["state"], row["cell"], row["stage"]), row["metrics"][metric], scale)
               for row in comparison["rows"] if metric in row["metrics"]]
    for name, result in comparison["process"].items():
        unit = 1 << 20 if name == "process_peak_rss_bytes" else 1.0
        label = "peak_rss_mib" if name == "process_peak_rss_bytes" else name.split(".", 1)[1] + "_s"
        results.append((("process", "-", "-", label), result, unit))

    def pct(value):
        return "-" if value is None else f"{100 * value:.1f}%"

    for (section, state, cell, stage), result, unit in results:
        ratio = "-" if result["ratio"] is None else f"{result['ratio']:.3f}"
        lines.append(f"{section:<12} {state:<11} {cell:<9} {stage:<24} "
                     f"{result['baseline_median'] / unit:>11.3f} {result['candidate_median'] / unit:>11.3f} "
                     f"{ratio:>7} {pct(result['baseline_spread']):>6} {pct(result['candidate_spread']):>6}  "
                     f"{result['verdict']}")
    if comparison["decidable"]:
        called = sum(1 for _, result, _ in results if result["verdict"] != "inconclusive")
        lines.append(f"{called} of {len(results)} rows called; with no real difference about "
                     f"{len(results) * chance:.2f} would be called by chance")
    return "\n".join(lines)


def run_abba(commands, binaries, fixture, out_dir, rounds, measure_args=(), attempt_timeout=None,
             keep_going=False, alpha=ALPHA):
    """Runs the A-B-B-A schedule; `commands` maps each role to the argv
    prefix of its benchmark binary. Returns (manifest, comparison or None)."""
    out_dir = Path(out_dir).resolve()
    fixture = Path(fixture).resolve()
    out_dir.mkdir(parents=True, exist_ok=False)
    manifest = {"schema": RUN_SCHEMA, "status": "running", "rounds": rounds,
                "order": abba_order(rounds), "binaries": binaries,
                "null_comparison": binaries["baseline"]["sha256"] == binaries["candidate"]["sha256"],
                "fixture": {"directory": str(fixture), "manifest": pin(fixture / "fixture.json")},
                "measure_args": list(measure_args), "toolchain": toolchain(), "machine": machine(),
                "started_unix": time.time(), "attempts": []}
    write_json(out_dir / "run.json", manifest)
    entries = []
    for index, role in enumerate(manifest["order"]):
        receipt = out_dir / f"receipt-{index:02d}-{role}.json"
        argv = [*commands[role], "measure", "--fixture", str(fixture),
                "--work", str(out_dir / f"work-{index:02d}"), "--out", str(receipt),
                "--label", f"{role} attempt {index}", *measure_args]
        attempt = {"index": index, "role": role, "argv": argv, "started_unix": time.time()}
        started = time.monotonic()
        with open(out_dir / f"attempt-{index:02d}-{role}.stdout", "wb") as stdout, \
                open(out_dir / f"attempt-{index:02d}-{role}.stderr", "wb") as stderr:
            try:
                completed = subprocess.run(argv, stdout=stdout, stderr=stderr, timeout=attempt_timeout)
                attempt["exit_code"] = completed.returncode
            except subprocess.TimeoutExpired:
                attempt["exit_code"] = None
                attempt["timed_out"] = True
        attempt["wall_seconds"] = time.monotonic() - started
        ok = attempt["exit_code"] == 0 and receipt.is_file()
        if ok:
            attempt["receipt"] = pin(receipt)
            entries.append((role, load_receipt(receipt), f"attempt {index} ({role})"))
        manifest["attempts"].append(attempt)
        write_json(out_dir / "run.json", manifest)
        if not ok and not keep_going:
            break
    for role, binary in binaries.items():
        require(sha256_file(binary["path"]) == binary["sha256"], f"the {role} binary changed during the run")
    complete = len(entries) == len(manifest["order"])
    comparison = None
    if complete:
        try:
            comparison = compare(entries, alpha)
            manifest["status"] = "compared"
        except ValueError as error:
            manifest["status"] = "invalid"
            manifest["error"] = str(error)
    else:
        manifest["status"] = "incomplete"
    manifest["finished_unix"] = time.time()
    if comparison is not None:
        write_json(out_dir / "comparison.json", comparison)
        manifest["comparison"] = pin(out_dir / "comparison.json")
    write_json(out_dir / "run.json", manifest)
    return manifest, comparison


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    commands = parser.add_subparsers(dest="command", required=True)
    run = commands.add_parser("run", help="measure baseline and candidate A-B-B-A")
    run.add_argument("--baseline-bin", required=True, type=Path)
    run.add_argument("--candidate-bin", required=True, type=Path)
    run.add_argument("--fixture", required=True, type=Path)
    run.add_argument("--out-dir", required=True, type=Path)
    run.add_argument("--rounds", type=int, default=1)
    run.add_argument("--attempt-timeout", type=float, default=None, help="seconds per attempt")
    run.add_argument("--keep-going", action="store_true", help="continue after a failed attempt")
    run.add_argument("--alpha", type=float, default=ALPHA,
                     help="largest chance probability of a separation that is still called")
    run.add_argument("measure_args", nargs=argparse.REMAINDER,
                     help="after --: more `measure` options for every attempt (identical for both roles)")
    again = commands.add_parser("compare", help="compare the receipts of a finished run directory")
    again.add_argument("--run-dir", required=True, type=Path)
    again.add_argument("--metric", default="wall_ns", choices=METRICS)
    again.add_argument("--alpha", type=float, default=ALPHA,
                       help="largest chance probability of a separation that is still called")
    args = parser.parse_args(argv)
    if args.command == "run":
        extra = args.measure_args[1:] if args.measure_args[:1] == ["--"] else args.measure_args
        binaries = {"baseline": pin(args.baseline_bin), "candidate": pin(args.candidate_bin)}
        manifest, comparison = run_abba(
            {role: [binaries[role]["path"]] for role in ROLES}, binaries, args.fixture, args.out_dir,
            args.rounds, extra, args.attempt_timeout, args.keep_going, args.alpha)
    else:
        manifest = json.loads((args.run_dir / "run.json").read_text(encoding="utf-8"))
        entries = [(attempt["role"], load_receipt(attempt["receipt"]["path"]),
                    f"attempt {attempt['index']} ({attempt['role']})")
                   for attempt in manifest["attempts"] if "receipt" in attempt]
        for attempt in manifest["attempts"]:
            if "receipt" in attempt:
                require(sha256_file(attempt["receipt"]["path"]) == attempt["receipt"]["sha256"],
                        f"receipt of attempt {attempt['index']} changed")
        comparison = compare(entries, args.alpha)
    if comparison is not None:
        print(format_table(comparison, getattr(args, "metric", "wall_ns")))
    print(f"status: {manifest['status']}" + (f" ({manifest['error']})" if "error" in manifest else ""))
    return 0 if manifest["status"] in ("compared",) or args.command == "compare" else 1


if __name__ == "__main__":
    sys.exit(main())
