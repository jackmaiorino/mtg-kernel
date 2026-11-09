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
    require(isinstance(receipt, dict) and receipt.get("schema") == RECEIPT_SCHEMA,
            f"{path}: not a {RECEIPT_SCHEMA} receipt")
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


def unique_rows(rows, fields, label, kind):
    indexed = {}
    for row in rows:
        key = tuple(row[field] for field in fields)
        require(key not in indexed, f"{label}: duplicate {kind} row {key}")
        indexed[key] = row
    return indexed


def receipt_series(receipt, label):
    rows = unique_rows(receipt["series"], ("section", "state", "cell", "stage"), label, "series")
    signatures = {}
    for key, row in rows.items():
        summary = row["summary"]
        count = summary["n"]
        repetition_key = "update_reps" if key[0] == "updates" else "reps"
        require(type(count) is int and count > 0 and count == receipt["config"][repetition_key]
                and len(row["samples"]) == count,
                f"{label}: {key} lacks its complete raw sample count")
        metrics = {metric for metric in METRICS if metric in summary}
        require("wall_ns" in metrics, f"{label}: {key} lacks wall time")
        for metric in metrics:
            median = summary[metric]["median"]
            require(type(median) in (int, float) and math.isfinite(median),
                    f"{label}: {key} {metric} median is not finite")
        signatures[key] = (count, metrics)
    return rows, signatures


def compare(entries, alpha=ALPHA):
    """Compares `entries`, a list of (role, receipt, label) in run order."""
    require(entries, "no receipts to compare")
    require(0 < alpha < 1, "alpha must lie between 0 and 1")
    roles = {role for role, _, _ in entries}
    require(roles <= set(ROLES), f"unknown roles {sorted(roles - set(ROLES))}")
    require(roles == set(ROLES), "the comparison needs baseline and candidate receipts")
    require(len(entries) % 4 == 0 and [role for role, _, _ in entries] == abba_order(len(entries) // 4),
            "the comparison needs complete A-B-B-A rounds in planned order")
    first_identity = identity_of(entries[0][1])
    published = None
    first_checks = None
    first_signatures = None
    first_process_fields = None
    check_identities = {}
    series = {}
    process = {role: {"peak_rss_bytes": [], "section_seconds": {}} for role in ROLES}
    for role, receipt, label in entries:
        require(identity_of(receipt) == first_identity,
                f"{label}: measured another fixture or other settings than {entries[0][2]}")
        failed = [check for check in receipt["checks"] if check.get("passed") is not True]
        require(not failed, f"{label}: in-run checks failed: {failed}")
        sections = set(receipt["config"]["sections"])
        checks = unique_rows(receipt["checks"], ("section", "state"), label, "check")
        these = unique_rows(receipt["published"], ("state", "cell"), label, "published")
        these = {key: (item["sha256"], item["bytes"]) for key, item in these.items()}
        rows, signatures = receipt_series(receipt, label)
        section_seconds = receipt["section_seconds"]
        rss = receipt.get("process_peak_rss_bytes")
        process_fields = (rss is not None, set(section_seconds))
        require(sections and sections == {key[0] for key in checks} == {key[0] for key in rows}
                == set(section_seconds), f"{label}: requested sections lack checks or measurements")
        require(these or sections == {"public-pair"}, f"{label}: missing published identities")
        if published is None:
            published, first_checks, first_signatures, first_process_fields = (
                these, checks, signatures, process_fields)
        require(these == published, f"{label}: published bytes differ from {entries[0][2]}")
        require(checks.keys() == first_checks.keys(), f"{label}: check keys differ")
        require(signatures == first_signatures, f"{label}: series keys or metric coverage differ")
        require(process_fields == first_process_fields, f"{label}: process metric coverage differs")
        for (section, state), check in checks.items():
            original = first_checks[(section, state)]
            require(check.keys() == original.keys(), f"{label}: {section} {state} check fields differ")
            for field in CHECK_IDENTITIES:
                if field in check:
                    key = (section, state, field)
                    require(original[field] == check[field], f"{label}: {' '.join(key)} differs")
                    check_identities[key] = check[field]
            require(check == original, f"{label}: {section} {state} check differs")
        for key, item in rows.items():
            for metric in signatures[key][1]:
                series.setdefault(key, {}).setdefault(metric, {"baseline": [], "candidate": []})[
                    role].append(item["summary"][metric]["median"])
        if rss is not None:
            require(type(rss) in (int, float) and math.isfinite(rss), f"{label}: invalid peak RSS")
            process[role]["peak_rss_bytes"].append(rss)
        for section, seconds in section_seconds.items():
            require(type(seconds) in (int, float) and math.isfinite(seconds),
                    f"{label}: invalid {section} elapsed time")
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


def require_build_declarations(binaries):
    declared = [binaries[role].get("declared_build") for role in ROLES]
    for role, build in zip(ROLES, declared):
        require(isinstance(build, dict) and all(
            isinstance(build.get(field), str) and build[field].strip()
            for field in ("source", "toolchain", "profile")),
            f"{role}: missing declared source, toolchain or build profile")
    require(all(declared[0][field] == declared[1][field] for field in ("toolchain", "profile")),
            "baseline and candidate declare different build toolchains or profiles")


def checked_receipt(receipt, manifest, role):
    expected = manifest["fixture"]["manifest"]["sha256"]
    require(receipt["fixture"]["manifest_sha256"] == expected,
            "child receipt used a different fixture manifest than the run pin")
    build = receipt.get("build", {})
    require(isinstance(build, dict), "child receipt build identity is not an object")
    recorded_source = build.get("git_head")
    declared = manifest["binaries"][role]["declared_build"]["source"]
    require(recorded_source is None or recorded_source == declared,
            f"{role}: receipt build source differs from its declaration")
    return receipt


def attempt_arguments(run_dir, manifest, index, role):
    return ["measure", "--fixture", manifest["fixture"]["directory"],
            "--work", str(run_dir / f"work-{index:02d}"),
            "--out", str(run_dir / f"receipt-{index:02d}-{role}.json"),
            "--label", f"{role} attempt {index}", *manifest["measure_args"]]


def read_completed_run(run_dir, manifest):
    require(isinstance(manifest, dict) and manifest.get("schema") == RUN_SCHEMA,
            "not an A-B-B-A run manifest")
    require(manifest.get("status") == "compared", "the run is incomplete or invalid")
    require_build_declarations(manifest["binaries"])
    order = abba_order(manifest["rounds"])
    attempts = manifest["attempts"]
    require(manifest["order"] == order and len(attempts) == len(order),
            "attempts do not match the complete planned A-B-B-A schedule")
    entries = []
    for index, (role, attempt) in enumerate(zip(order, attempts)):
        require(attempt["index"] == index and attempt["role"] == role
                and attempt.get("exit_code") == 0 and not attempt.get("error")
                and not attempt.get("timed_out"),
                f"attempt {index}: wrong role or failed attempt")
        tail = attempt_arguments(run_dir, manifest, index, role)
        argv = attempt["argv"]
        prefix = argv[:-len(tail)]
        require(prefix and argv[-len(tail):] == tail
                and Path(prefix[-1]).resolve() == Path(manifest["binaries"][role]["path"]),
                f"attempt {index}: command differs from its planned binary or inputs")
        path = run_dir / f"receipt-{index:02d}-{role}.json"
        pinned = attempt["receipt"]
        require(Path(pinned["path"]).resolve() == path
                and sha256_file(path) == pinned["sha256"],
                f"receipt of attempt {index} changed or belongs to another attempt")
        receipt = checked_receipt(load_receipt(path), manifest, role)
        entries.append((role, receipt, f"attempt {index} ({role})"))
    return entries


def run_abba(commands, binaries, fixture, out_dir, rounds, measure_args=(), attempt_timeout=None,
             keep_going=False, alpha=ALPHA):
    """Runs the A-B-B-A schedule; `commands` maps each role to the argv
    prefix of its benchmark binary. Returns (manifest, comparison or None)."""
    require_build_declarations(binaries)
    require(0 < alpha < 1, "alpha must lie between 0 and 1")
    require(attempt_timeout is None or (math.isfinite(attempt_timeout) and attempt_timeout > 0),
            "attempt timeout must be finite and positive")
    order = abba_order(rounds)
    for role in ROLES:
        require(commands[role] and Path(commands[role][-1]).resolve() == Path(binaries[role]["path"]),
                f"{role} command does not name its pinned executable")
    out_dir = Path(out_dir).resolve()
    fixture = Path(fixture).resolve()
    fixture_path = fixture / "fixture.json"
    fixture_pin = pin(fixture_path)
    out_dir.mkdir(parents=True, exist_ok=False)
    manifest = {"schema": RUN_SCHEMA, "status": "running", "rounds": rounds,
                "order": order, "binaries": binaries,
                "null_comparison": binaries["baseline"]["sha256"] == binaries["candidate"]["sha256"],
                "fixture": {"directory": str(fixture), "manifest": fixture_pin},
                "measure_args": list(measure_args), "toolchain": toolchain(), "machine": machine(),
                "started_unix": time.time(), "attempts": []}
    write_json(out_dir / "run.json", manifest)
    entries = []
    for index, role in enumerate(manifest["order"]):
        receipt = out_dir / f"receipt-{index:02d}-{role}.json"
        argv = [*commands[role], *attempt_arguments(out_dir, manifest, index, role)]
        attempt = {"index": index, "role": role, "argv": argv, "started_unix": time.time()}
        started = time.monotonic()
        with open(out_dir / f"attempt-{index:02d}-{role}.stdout", "wb") as stdout, \
                open(out_dir / f"attempt-{index:02d}-{role}.stderr", "wb") as stderr:
            try:
                require(sha256_file(binaries[role]["path"]) == binaries[role]["sha256"],
                        f"the {role} binary changed before attempt {index}")
                require(sha256_file(fixture_path) == fixture_pin["sha256"],
                        f"the fixture manifest changed before attempt {index}")
                completed = subprocess.run(argv, stdout=stdout, stderr=stderr, timeout=attempt_timeout)
                attempt["exit_code"] = completed.returncode
            except subprocess.TimeoutExpired:
                attempt["exit_code"] = None
                attempt["timed_out"] = True
            except (OSError, ValueError) as error:
                attempt["exit_code"] = None
                attempt["error"] = str(error)
        attempt["wall_seconds"] = time.monotonic() - started
        ok = False
        try:
            if receipt.is_file():
                attempt["receipt"] = pin(receipt)
            if attempt["exit_code"] == 0:
                require("receipt" in attempt, "child exited successfully without a receipt")
                value = checked_receipt(load_receipt(receipt), manifest, role)
                entries.append((role, value, f"attempt {index} ({role})"))
                ok = True
        except (OSError, ValueError, KeyError, TypeError) as error:
            attempt["error"] = str(error)
        manifest["attempts"].append(attempt)
        write_json(out_dir / "run.json", manifest)
        if not ok and not keep_going:
            break
    comparison = None
    try:
        for role, binary in binaries.items():
            require(sha256_file(binary["path"]) == binary["sha256"], f"the {role} binary changed during the run")
        require(sha256_file(fixture_path) == fixture_pin["sha256"], "the fixture manifest changed during the run")
        if len(entries) == len(order):
            comparison = compare(entries, alpha)
            manifest["status"] = "compared"
        else:
            manifest["status"] = "incomplete"
    except (OSError, ValueError, KeyError, TypeError) as error:
        manifest["status"] = "invalid"
        manifest["error"] = str(error)
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
    for role in ROLES:
        run.add_argument(f"--{role}-build", required=True, type=json.loads,
                         help='JSON declaration with "source", "toolchain" and "profile"')
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
        binaries = {role: {**pin(getattr(args, f"{role}_bin")),
                           "declared_build": getattr(args, f"{role}_build")} for role in ROLES}
        manifest, comparison = run_abba(
            {role: [binaries[role]["path"]] for role in ROLES}, binaries, args.fixture, args.out_dir,
            args.rounds, extra, args.attempt_timeout, args.keep_going, args.alpha)
    else:
        run_dir = args.run_dir.resolve()
        try:
            manifest = json.loads((run_dir / "run.json").read_text(encoding="utf-8"))
            comparison = compare(read_completed_run(run_dir, manifest), args.alpha)
        except (OSError, ValueError, KeyError, TypeError) as error:
            print(f"status: invalid ({error})", file=sys.stderr)
            return 1
    if comparison is not None:
        print(format_table(comparison, getattr(args, "metric", "wall_ns")))
    print(f"status: {manifest['status']}" + (f" ({manifest['error']})" if "error" in manifest else ""))
    return 0 if manifest["status"] == "compared" else 1


if __name__ == "__main__":
    sys.exit(main())
