#!/usr/bin/env python3
"""Opt-in local benchmark of the native training run (#595, under #594's contract).

`generate` writes the source-generated net-8 fixture: the ignored lib test
`native_expanded_training_run_v1::fixture_bench_v1::write_native_fixture_v1`
builds synthetic valid FlatV4 weights, the checked-in Affinity/Terror schedule,
a fresh case and (after an in-process warm-up) a warm case that resumes the
retained warmed checkpoint.

`run` times two prebuilt `native_expanded_training_run_v1` binaries (a baseline
and a candidate) on those cases in balanced A-B-B-A blocks. Every sample is a
new process. A case always writes its run to the same fixed path, which the
driver moves out after each sample, so the raw files of any two samples
compare byte for byte; every difference is either accounted for (a timing
field, a build-provenance field across builds, or the pin of a file that
differs only that way) or reported as a mismatch.

Standard library only; Linux or macOS (os.wait4). See README.md for the commands.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import platform
import re
import shutil
import signal
import statistics
import subprocess
import sys
import threading
import time
from datetime import datetime, timezone
from pathlib import Path

FIXTURE_SCHEMA = "mtg-kernel-native-fixture/v1"
BENCH_SCHEMA = "mtg-kernel-native-fixture-bench/v1"
SAMPLE_SCHEMA = "mtg-kernel-native-fixture-sample/v1"
COMPARISON_SCHEMA = "mtg-kernel-native-fixture-comparison/v1"
GENERATE_SCHEMA = "mtg-kernel-native-fixture-generate/v1"
RUN_SCHEMA = "mtg-kernel-native-expanded-training-run/v1"
DIAGNOSTICS_ENV = "MTG_KERNEL_CACHE_DIAGNOSTICS_V1"
DIAGNOSTICS_SCHEMA = "mtg-kernel-cache-diagnostics/v1"
GENERATOR_TEST = "native_expanded_training_run_v1::fixture_bench_v1::write_native_fixture_v1"
ABBA = ("A", "B", "B", "A")

# Every difference between two samples' run trees must fall in one of these
# classes; anything else is a mismatch. Timing: a numeric leaf whose key ends
# in one of these suffixes (wall-clock receipts such as update.json's
# `checkpoint_io_seconds`; they never enter state or hashes). Provenance: the
# build identity in run.json, only between samples of different builds. Pin:
# the sha256 of a run file that itself differs only in accounted fields.
TIMING_SUFFIXES = ("_seconds", "_nanos", "_ns")
PROVENANCE_KEYS = frozenset({"git_commit", "tracked_tree_sha256", "implementation_sha256"})
JSON_DIFF_CAP = 64 << 20
COLLECTED_DIFFERENCES = 10_000
LISTED_DIFFERENCES = 20

# Per-iteration timings, by source. stdout: the run's own per-iteration line
# (`scheduler_timing`); the receipts: collect/collection.json, update/update.json.
SCHEDULER_FIELDS = (
    "iteration_wall_seconds",
    "collection_execution_seconds",
    "update_execution_seconds",
    "collection_validation_seconds",
    "update_validation_seconds",
)
COLLECTION_FIELDS = ("collection_elapsed_seconds", "collection_initialization_seconds")
UPDATE_FIELDS = (
    "input_read_seconds",
    "behavior_replay_seconds",
    "learner_update_seconds",
    "checkpoint_io_seconds",
    "update_elapsed_seconds",
)
PREPARATION_FIELDS = (
    "preparation_elapsed_seconds",
    "opponent_loading_seconds",
    "parallel_replay_seconds",
)
# Compared metrics: name -> True when lower is better.
PROCESS_METRICS = {
    "wall_seconds": True,
    "user_seconds": True,
    "sys_seconds": True,
    "max_rss_mib": True,
    "updates_per_hour": False,
    "outside_iterations_seconds": True,
}


class BenchError(Exception):
    pass


def utc_now() -> str:
    return datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%S.%fZ")


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with open(path, "rb") as handle:
        for block in iter(lambda: handle.read(1 << 20), b""):
            digest.update(block)
    return digest.hexdigest()


def read_json(path: Path):
    with open(path, "rb") as handle:
        return json.load(handle)


def write_json(path: Path, value) -> None:
    staged = path.with_name(f".{path.name}.stage")
    with open(staged, "w", encoding="utf-8") as handle:
        json.dump(value, handle, indent=2, sort_keys=True)
        handle.write("\n")
        handle.flush()
        os.fsync(handle.fileno())
    os.replace(staged, path)


def append_jsonl(path: Path, value) -> None:
    with open(path, "a", encoding="utf-8") as handle:
        handle.write(json.dumps(value, sort_keys=True) + "\n")
        handle.flush()
        os.fsync(handle.fileno())


def require_absent_or_empty(path: Path, what: str) -> None:
    if not path.is_absolute():
        raise BenchError(f"{what} must be an absolute path: {path}")
    if path.exists() and (not path.is_dir() or any(path.iterdir())):
        raise BenchError(f"{what} must be absent or an empty directory: {path}")


def command_output(command: list[str]) -> str | None:
    try:
        done = subprocess.run(command, capture_output=True, text=True, timeout=30, check=False)
    except (OSError, subprocess.SubprocessError):
        return None
    return (done.stdout or done.stderr).strip() or None


def max_rss_mib(ru_maxrss: int, system: str = sys.platform) -> float:
    """wait4's peak RSS in MiB: Linux reports KiB, macOS bytes."""
    return ru_maxrss / (1 << 20) if system == "darwin" else ru_maxrss / 1024


def host_record() -> dict:
    cpu = None
    try:
        for line in Path("/proc/cpuinfo").read_text().splitlines():
            if line.startswith("model name"):
                cpu = line.split(":", 1)[1].strip()
                break
    except OSError:
        cpu = command_output(["sysctl", "-n", "machdep.cpu.brand_string"])
    memory_kib = None
    try:
        for line in Path("/proc/meminfo").read_text().splitlines():
            if line.startswith("MemTotal:"):
                memory_kib = int(line.split()[1])
    except (OSError, ValueError):
        try:
            memory_kib = int(command_output(["sysctl", "-n", "hw.memsize"]) or "") // 1024
        except ValueError:
            pass
    first_line = lambda text: text.splitlines()[0] if text else None  # noqa: E731
    return {
        "hostname": platform.node(),
        "platform": platform.platform(),
        "kernel": platform.release(),
        "machine": platform.machine(),
        "cpu_model": cpu,
        "logical_cpus": os.cpu_count(),
        "affinity_cpus": len(os.sched_getaffinity(0)) if hasattr(os, "sched_getaffinity") else None,
        "memory_total_mib": round(memory_kib / 1024, 1) if memory_kib else None,
        "python": platform.python_version(),
        # The host's toolchain, which is not necessarily the one that built the arms.
        "rustc_on_host": command_output(["rustc", "-vV"]),
        "cc_on_host": first_line(command_output(["cc", "--version"])),
        "ld_on_host": first_line(command_output(["ld", "--version"])),
    }


# ---------------------------------------------------------------- generate


def generate(args) -> int:
    root = Path(args.root)
    require_absent_or_empty(root, "--root")
    env = dict(os.environ)
    settings = {
        "MTG_NATIVE_FIXTURE_ROOT": str(root),
        "MTG_NATIVE_FIXTURE_SEED": str(args.seed),
        "MTG_NATIVE_FIXTURE_UPDATES": str(args.updates),
        "MTG_NATIVE_FIXTURE_GAMES": str(args.games),
        "MTG_NATIVE_FIXTURE_WARMUP_UPDATES": str(
            args.updates if args.warmup_updates is None else args.warmup_updates
        ),
        "MTG_NATIVE_FIXTURE_COLLECTION_WORKERS": str(args.collection_workers),
        "MTG_NATIVE_FIXTURE_PREPARATION_WORKERS": str(args.preparation_workers),
    }
    env.update(settings)
    selector = ["--ignored", "--exact", GENERATOR_TEST, "--nocapture", "--test-threads", "1"]
    if args.test_binary:
        test_binary = Path(args.test_binary).resolve()
        command = [str(test_binary), *selector]
        # cargo runs a crate's tests from the package root.
        cwd = Path(args.repo) / "mtg-kernel"
        if not cwd.is_dir():
            cwd = None
    else:
        test_binary = None
        command = ["cargo", "test", "--release", "--locked", "-p", "mtg-kernel", "--lib",
                   GENERATOR_TEST, "--", *selector]
        cwd = args.repo
    started = time.monotonic()
    done = subprocess.run(command, cwd=cwd, env=env, capture_output=True, text=True, check=False)
    wall = time.monotonic() - started
    if done.returncode != 0:
        sys.stderr.write(done.stdout[-4000:] + done.stderr[-4000:])
        raise BenchError(f"fixture generator exited {done.returncode}")
    fixture_path = root / "fixture.json"
    if not fixture_path.is_file():
        raise BenchError(f"fixture generator wrote no {fixture_path}")
    fixture = read_json(fixture_path)
    validate_fixture(fixture, root.resolve())
    printed = [line for line in done.stdout.splitlines() if line.startswith("NATIVE_FIXTURE_V1 ")]
    if printed and json.loads(printed[-1].split(" ", 1)[1]) != fixture:
        raise BenchError("the generator's printed manifest differs from fixture.json")
    log_directory = root / "generate"
    log_directory.mkdir()
    (log_directory / "stdout.txt").write_text(done.stdout)
    (log_directory / "stderr.txt").write_text(done.stderr)
    record = {
        "schema": GENERATE_SCHEMA,
        "created_utc": utc_now(),
        "command": command,
        "settings": settings,
        "test_binary": None if test_binary is None else {
            "path": str(test_binary), "sha256": sha256_file(test_binary)},
        "generator_wall_seconds": round(wall, 3),
        "fixture_sha256": sha256_file(fixture_path),
        "note": "generation time includes the in-process warm-up; it is not a benchmark sample",
    }
    write_json(log_directory / "generate.json", record)
    print(json.dumps({"fixture": str(fixture_path), "fixture_sha256": record["fixture_sha256"],
                      "cases": [case["name"] for case in fixture["cases"]],
                      "generator_wall_seconds": record["generator_wall_seconds"]}, indent=2))
    return 0


def validate_fixture(fixture: dict, root: Path) -> None:
    if fixture.get("schema") != FIXTURE_SCHEMA:
        raise BenchError(f"not a {FIXTURE_SCHEMA} manifest")
    if Path(fixture["root"]) != root:
        raise BenchError(f"fixture root {fixture['root']} is not {root}")
    for case in fixture["cases"]:
        config = Path(case["config"]["path"])
        if sha256_file(config) != case["config"]["sha256"]:
            raise BenchError(f"case {case['name']}: {config} changed since generation")
        expected = root / "work" / case["name"] / "run"
        if Path(case["output_directory"]) != expected:
            raise BenchError(f"case {case['name']} does not write to {expected}")


# ---------------------------------------------------------------- processes


def run_process(command: list[str], env: dict, stdout_path: Path, stderr_path: Path,
                timeout: float | None) -> dict:
    """Runs one process to exit; its wall time, CPU and peak RSS (wait4)."""
    with open(stdout_path, "wb") as out, open(stderr_path, "wb") as err:
        started_utc = utc_now()
        started = time.monotonic()
        process = subprocess.Popen(command, stdout=out, stderr=err, env=env,
                                   start_new_session=True)
        timed_out = threading.Event()

        def kill() -> None:
            timed_out.set()
            try:
                os.killpg(process.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass

        timer = threading.Timer(timeout, kill) if timeout else None
        if timer:
            timer.start()
        try:
            _, status, usage = os.wait4(process.pid, 0)
        finally:
            if timer:
                timer.cancel()
        wall = time.monotonic() - started
        process.returncode = os.waitstatus_to_exitcode(status)
    return {
        "command": command,
        "started_utc": started_utc,
        "exit_code": process.returncode,
        "timed_out": timed_out.is_set(),
        "wall_seconds": wall,
        "user_seconds": usage.ru_utime,
        "sys_seconds": usage.ru_stime,
        "max_rss_mib": max_rss_mib(usage.ru_maxrss),
        "major_page_faults": usage.ru_majflt,
        "voluntary_context_switches": usage.ru_nvcsw,
        "involuntary_context_switches": usage.ru_nivcsw,
    }


def parse_stdout(path: Path) -> dict:
    iterations, result, other = [], None, 0
    for line in path.read_text(errors="replace").splitlines():
        try:
            value = json.loads(line)
        except json.JSONDecodeError:
            other += 1
            continue
        if isinstance(value, dict) and "completed_iteration" in value:
            iterations.append(value)
        elif isinstance(value, dict) and value.get("schema") == RUN_SCHEMA:
            result = value
        else:
            other += 1
    return {"iterations": iterations, "result": result, "other_lines": other}


def read_diagnostics(path: Path) -> dict | None:
    if not path.exists():
        return None
    lines = [json.loads(line) for line in path.read_text().splitlines() if line.strip()]
    if not lines:
        return None
    for line in lines:
        if line.get("schema") != DIAGNOSTICS_SCHEMA:
            raise BenchError(f"{path}: unexpected diagnostics schema")
    last = dict(lines[-1])
    for key in ("schema", "completed_iteration", "newly_completed", "process_id"):
        last.pop(key)
    return {"lines": len(lines), "process_ids": sorted({line["process_id"] for line in lines}),
            "completed_iterations": [line["completed_iteration"] for line in lines],
            "final": last}


# ---------------------------------------------------------------- run trees


def tree_files(root: Path) -> dict[str, Path]:
    files = {}
    if not root.exists():
        return files
    for directory, subdirectories, names in os.walk(root):
        subdirectories.sort()
        for name in sorted(names):
            path = Path(directory) / name
            files[path.relative_to(root).as_posix()] = path
    return files


def map_pin(path, prefix: str) -> str | None:
    """A pin's absolute path inside the fixed run directory, as a tree path."""
    if isinstance(path, str) and path.startswith(prefix + "/"):
        return path[len(prefix) + 1:]
    return None


def mapped_json(tree: Path, prefix: str, pin) -> dict | None:
    relative = map_pin(pin.get("path") if isinstance(pin, dict) else None, prefix)
    if relative is None or not (tree / relative).is_file():
        return None
    return read_json(tree / relative)


def boundaries(tree: Path, prefix: str) -> list[dict]:
    """The published state after every completed iteration."""
    found = []
    iterations = tree / "iterations"
    if not iterations.is_dir():
        return found
    for directory in sorted(iterations.iterdir()):
        complete = directory / "complete.json"
        if not complete.is_file():
            continue
        receipt = read_json(complete)
        identity = receipt.get("output_identity") or {}
        update = map_pin(receipt.get("update", {}).get("path"), prefix)
        checkpoint = None
        if update is not None:
            path = (tree / update).parent / "checkpoint.json"
            checkpoint = sha256_file(path) if path.is_file() else None
        found.append({"iteration": receipt.get("iteration"),
                      "adam_step": identity.get("adam_step"),
                      "state_sha256": identity.get("state_sha256"),
                      "output_identity_sha256": hashlib.sha256(
                          json.dumps(identity, sort_keys=True).encode()).hexdigest(),
                      "checkpoint_sha256": checkpoint})
    return found


def iteration_receipts(tree: Path, prefix: str) -> list[dict]:
    """Phase timings recorded in each completed iteration's receipts."""
    rows = []
    iterations = tree / "iterations"
    if not iterations.is_dir():
        return rows
    for directory in sorted(iterations.iterdir()):
        complete = directory / "complete.json"
        if not complete.is_file():
            continue
        receipt = read_json(complete)
        row = {"iteration": receipt.get("iteration")}
        collection = mapped_json(tree, prefix, receipt.get("collection")) or {}
        update = mapped_json(tree, prefix, receipt.get("update")) or {}
        for field in COLLECTION_FIELDS:
            row[field] = collection.get(field)
        for field in UPDATE_FIELDS:
            row[field] = update.get(field)
        preparation = update.get("update_preparation") or {}
        for field in PREPARATION_FIELDS:
            row[f"update_preparation.{field}"] = preparation.get(field)
        rows.append(row)
    return rows


def storage(tree: Path) -> dict:
    files = tree_files(tree)
    by_kind: dict[str, int] = {}
    total = 0
    for relative, path in files.items():
        size = path.stat().st_size
        total += size
        # One entry per file name pattern: episode-0001.json is "episode-#.json".
        kind = re.sub(r"[0-9]+", "#", Path(relative).name)
        by_kind[kind] = by_kind.get(kind, 0) + size
    return {"files": len(files), "bytes": total, "bytes_by_name": dict(sorted(by_kind.items()))}


def json_differences(a, b, path, parents, out) -> None:
    if len(out) >= COLLECTED_DIFFERENCES:
        return
    if isinstance(a, dict) and isinstance(b, dict):
        for key in sorted(set(a) | set(b)):
            if key not in a or key not in b:
                out.append((path + (key,), "structure", parents))
            else:
                json_differences(a[key], b[key], path + (key,), (a, b), out)
    elif isinstance(a, list) and isinstance(b, list):
        if len(a) != len(b):
            out.append((path, "structure", parents))
        else:
            for index, (left, right) in enumerate(zip(a, b)):
                json_differences(left, right, path + (index,), (a, b), out)
    elif type(a) is not type(b) or a != b:
        out.append((path, "value", parents))


def is_number(value) -> bool:
    return isinstance(value, (int, float)) and not isinstance(value, bool)


class TreeComparison:
    """Compares a sample's run tree with the reference sample's, file by file."""

    def __init__(self, reference: Path, sample: Path, prefix: str, across_builds: bool):
        self.reference, self.sample = reference, sample
        self.prefix, self.across_builds = prefix, across_builds
        self.reference_files = tree_files(reference)
        self.sample_files = tree_files(sample)
        self.results: dict[str, dict] = {}
        self.visiting: set[str] = set()

    def file(self, relative: str) -> dict:
        if relative in self.results:
            return self.results[relative]
        if relative in self.visiting:
            return {"status": "mismatch", "differences": ["pin cycle"]}
        self.visiting.add(relative)
        try:
            result = self._file(relative)
        finally:
            self.visiting.discard(relative)
        self.results[relative] = result
        return result

    def _file(self, relative: str) -> dict:
        left, right = self.reference_files.get(relative), self.sample_files.get(relative)
        if left is None or right is None:
            return {"status": "mismatch", "differences": ["file present on one side only"]}
        if left.stat().st_size == right.stat().st_size and sha256_file(left) == sha256_file(right):
            return {"status": "identical"}
        if max(left.stat().st_size, right.stat().st_size) > JSON_DIFF_CAP:
            return {"status": "mismatch", "differences": ["large file differs"]}
        try:
            a, b = read_json(left), read_json(right)
        except (json.JSONDecodeError, UnicodeDecodeError):
            return {"status": "mismatch", "differences": ["non-JSON file differs"]}
        found: list = []
        json_differences(a, b, (), (None, None), found)
        classes: dict[str, int] = {}
        unaccounted = []
        for path, kind, parents in found:
            label = self.classify(path, kind, parents, a, b)
            classes[label] = classes.get(label, 0) + 1
            if label == "unaccounted":
                unaccounted.append("/".join(map(str, path)) + f" ({kind})")
        if unaccounted:
            return {"status": "mismatch", "classes": classes,
                    "differences": unaccounted[:LISTED_DIFFERENCES],
                    "unaccounted": len(unaccounted),
                    "truncated": len(found) >= COLLECTED_DIFFERENCES}
        return {"status": "accounted", "classes": classes}

    def classify(self, path, kind, parents, a, b) -> str:
        if kind != "value" or not path or not isinstance(path[-1], str):
            return "unaccounted"
        key = path[-1]
        left, right = value_at(a, path), value_at(b, path)
        if key.endswith(TIMING_SUFFIXES) and is_number(left) and is_number(right):
            return "timing"
        if self.across_builds and key in PROVENANCE_KEYS:
            return "provenance"
        parent_a, parent_b = parents
        if (key == "sha256" and isinstance(parent_a, dict) and isinstance(parent_b, dict)
                and parent_a.get("path") == parent_b.get("path")):
            target = map_pin(parent_a.get("path"), self.prefix)
            if target is not None and self.file(target)["status"] == "accounted":
                return "pin"
        return "unaccounted"

    def summary(self) -> dict:
        names = sorted(set(self.reference_files) | set(self.sample_files))
        missing = [n for n in names if n not in self.sample_files]
        extra = [n for n in names if n not in self.reference_files]
        identical, accounted, mismatched = 0, [], []
        for name in names:
            if name in missing or name in extra:
                continue
            result = self.file(name)
            if result["status"] == "identical":
                identical += 1
            elif result["status"] == "accounted":
                accounted.append({"file": name, "classes": result["classes"]})
            else:
                mismatched.append({"file": name, **{k: v for k, v in result.items() if k != "status"}})
        reference_states = boundaries(self.reference, self.prefix)
        sample_states = boundaries(self.sample, self.prefix)
        if missing or extra or mismatched or reference_states != sample_states:
            status = "mismatch"
        elif accounted:
            status = "equal_up_to_accounted_fields"
        else:
            status = "identical"
        return {"status": status, "files": len(names), "identical_files": identical,
                "accounted_files": accounted, "mismatched_files": mismatched,
                "missing_files": missing, "extra_files": extra,
                "boundaries_equal": reference_states == sample_states,
                "boundaries": len(sample_states), "across_builds": self.across_builds}


def value_at(document, path):
    for step in path:
        document = document[step]
    return document


# ---------------------------------------------------------------- statistics


def describe(values: list[float]) -> dict:
    if not values:
        return {"n": 0}
    median = statistics.median(values)
    return {"n": len(values), "median": median, "mean": statistics.fmean(values),
            "min": min(values), "max": max(values),
            "stdev": statistics.stdev(values) if len(values) > 1 else None,
            "spread_fraction": (max(values) - min(values)) / median if median else None,
            "samples": values}


def verdict(baseline: list[float], candidate: list[float], blocks: list[tuple],
            lower_is_better: bool) -> dict:
    """The comparison rule (#594 item 3): fewer than two complete A-B-B-A
    blocks, an effect no larger than the observed run-to-run spread (the
    larger arm's max minus min over its median), or blocks that disagree in
    direction is inconclusive; otherwise the candidate/baseline median ratio
    is an improvement or a regression.
    """
    if len(baseline) < 2 or len(candidate) < 2:
        return {"verdict": "inconclusive", "reason": "fewer than two samples per arm"}
    base, cand = statistics.median(baseline), statistics.median(candidate)
    if base <= 0:
        return {"verdict": "inconclusive", "reason": "the baseline median is not positive"}
    ratio = cand / base
    spreads = [describe(values)["spread_fraction"] for values in (baseline, candidate)]
    spread = max(s for s in spreads if s is not None)
    block_ratios = [sum(b) / sum(a) for a, b in blocks if sum(a) > 0]
    result = {"ratio_candidate_to_baseline": ratio, "effect_fraction": ratio - 1,
              "max_spread_fraction": spread, "block_ratios": block_ratios}
    if len(block_ratios) < 2:
        result.update(verdict="inconclusive", reason="fewer than two complete A-B-B-A blocks")
    elif abs(ratio - 1) <= spread:
        result.update(verdict="inconclusive", reason="the median effect is within the observed spread")
    elif block_ratios and not (all(r < 1 for r in block_ratios) or all(r > 1 for r in block_ratios)):
        result.update(verdict="inconclusive", reason="A-B-B-A blocks disagree in direction")
    else:
        improved = ratio < 1 if lower_is_better else ratio > 1
        result.update(verdict="improvement" if improved else "regression",
                      reason="the median effect exceeds the observed spread in every block")
    return result


def sample_metrics(sample: dict) -> dict:
    """The compared numbers of one completed sample."""
    metrics = {name: sample["process"][name] for name in
               ("wall_seconds", "user_seconds", "sys_seconds", "max_rss_mib")}
    completed = sample["newly_completed_iterations"]
    wall = sample["process"]["wall_seconds"]
    metrics["updates_per_hour"] = completed * 3600 / wall if wall > 0 else None
    for field in SCHEDULER_FIELDS:
        values = [line.get("scheduler_timing", {}).get(field) for line in sample["iterations_stdout"]]
        if values and all(is_number(value) for value in values):
            metrics[f"scheduler.{field}"] = sum(values)
    if "scheduler.iteration_wall_seconds" in metrics:
        metrics["outside_iterations_seconds"] = wall - metrics["scheduler.iteration_wall_seconds"]
    for field in COLLECTION_FIELDS + UPDATE_FIELDS + tuple(
            f"update_preparation.{f}" for f in PREPARATION_FIELDS):
        values = [row[field] for row in sample["iteration_receipts"] if is_number(row.get(field))]
        if values:
            metrics[f"receipts.{field}"] = sum(values)
    return metrics


def compare_case(case: str, samples: list[dict], arms: dict) -> dict:
    timed = [s for s in samples if s["kind"] == "timed"]
    complete = [s for s in timed if s["ok"]]
    by_arm = {letter: [s for s in complete if s["arm_letter"] == letter] for letter in "AB"}
    metrics = {}
    names = sorted(set().union(*(set(s["metrics"]) for s in complete))) if complete else []
    for name in names:
        lower = PROCESS_METRICS.get(name, True)
        values = {letter: [s["metrics"][name] for s in by_arm[letter]
                           if is_number(s["metrics"].get(name))] for letter in "AB"}
        blocks = []
        for block in sorted({s["block"] for s in complete}):
            members = [s for s in complete if s["block"] == block and is_number(s["metrics"].get(name))]
            a = [s["metrics"][name] for s in members if s["arm_letter"] == "A"]
            b = [s["metrics"][name] for s in members if s["arm_letter"] == "B"]
            if len(a) == 2 and len(b) == 2:
                blocks.append((a, b))
        metrics[name] = {"lower_is_better": lower, "baseline": describe(values["A"]),
                         "candidate": describe(values["B"]),
                         **verdict(values["A"], values["B"], blocks, lower)}
    parity = [{"ordinal": s["ordinal"], "kind": s["kind"], "arm": s["arm"],
               "status": s["parity"]["status"] if s.get("parity") else "reference"}
              for s in samples if s["ok"]]
    counters = {}
    for letter in "AB":
        finals = [s["diagnostics"]["final"] for s in by_arm[letter] if s.get("diagnostics")]
        counters[arms[letter]["name"]] = {
            "available": bool(finals),
            "equal_across_samples": all(f == finals[0] for f in finals) if finals else None,
            "final": finals[0] if finals else None}
    cold = [s for s in samples if s["kind"] == "cold_resume"]
    failures = [{"ordinal": s["ordinal"], "arm": s["arm"], "kind": s["kind"], "error": s.get("error")}
                for s in samples if not s["ok"]]
    parity_ok = all(p["status"] in ("reference", "identical", "equal_up_to_accounted_fields")
                    for p in parity)
    provenance = {arms[l]["name"]: sorted({json.dumps(s.get("run_provenance"), sort_keys=True)
                                           for s in by_arm[l]}) for l in "AB"}
    return {"timed_samples": {arms[l]["name"]: len(by_arm[l]) for l in "AB"},
            "run_provenance": {name: [json.loads(v) for v in values]
                               for name, values in provenance.items()},
            "failures": failures, "parity_ok": parity_ok and not failures, "parity": parity,
            "metrics": metrics, "counters": counters,
            "cold_resume": [{"arm": s["arm"], "ok": s["ok"],
                             "first_process_completed": s.get("first_process_completed"),
                             "status": s["parity"]["status"] if s.get("parity") else None}
                            for s in cold],
            "storage": {arms[l]["name"]: describe([s["storage"]["bytes"] for s in by_arm[l]])
                        for l in "AB"}}


# ---------------------------------------------------------------- run


def plan_samples(cases: list[str], blocks: int, warmup_runs: int, cold_resume: bool) -> list[dict]:
    plan = []
    for case in cases:
        for repeat in range(warmup_runs):
            for letter in "AB":
                plan.append({"case": case, "kind": "warmup", "arm_letter": letter,
                             "block": None, "position": f"warmup{repeat + 1}"})
        for block in range(blocks):
            seen: dict[str, int] = {}
            for letter in ABBA:
                seen[letter] = seen.get(letter, 0) + 1
                plan.append({"case": case, "kind": "timed", "arm_letter": letter,
                             "block": block, "position": f"{letter}{seen[letter]}"})
        if cold_resume:
            for letter in "AB":
                plan.append({"case": case, "kind": "cold_resume", "arm_letter": letter,
                             "block": None, "position": "cold"})
    for ordinal, entry in enumerate(plan):
        entry["ordinal"] = ordinal
    return plan


def parse_assignments(values: list[str], what: str) -> dict:
    parsed = {}
    for value in values or []:
        key, sep, rest = value.partition("=")
        if not sep or not key:
            raise BenchError(f"{what} takes KEY=VALUE, not {value!r}")
        parsed[key] = rest
    return parsed


def parse_arm(value: str, letter: str) -> dict:
    name, sep, binary = value.partition("=")
    if not sep or not name or not binary:
        raise BenchError(f"an arm is NAME=BINARY, not {value!r}")
    path = Path(binary).resolve()
    if not path.is_file() or not os.access(path, os.X_OK):
        raise BenchError(f"arm {name}: {path} is not an executable file")
    return {"letter": letter, "name": name, "binary": str(path), "sha256": sha256_file(path),
            "bytes": path.stat().st_size, "env": {}}


def run(args) -> int:
    fixture_root = Path(args.fixture).resolve()
    fixture = read_json(fixture_root / "fixture.json")
    validate_fixture(fixture, fixture_root)
    out = Path(args.out)
    require_absent_or_empty(out, "--out")
    arms = {"A": parse_arm(args.baseline, "A"), "B": parse_arm(args.candidate, "B")}
    if arms["A"]["name"] == arms["B"]["name"]:
        raise BenchError("the baseline and candidate need different names")
    shared_env = parse_assignments(args.env, "--env")
    for value in args.arm_env or []:
        name, sep, assignment = value.partition(":")
        matches = [arm for arm in arms.values() if arm["name"] == name]
        if not sep or not matches:
            raise BenchError(f"--arm-env takes ARM:KEY=VALUE with a known arm, not {value!r}")
        matches[0]["env"].update(parse_assignments([assignment], "--arm-env"))
    for env in [shared_env, *(arm["env"] for arm in arms.values())]:
        if DIAGNOSTICS_ENV in env:
            raise BenchError(f"the driver sets {DIAGNOSTICS_ENV} for every sample")
    known = {case["name"]: case for case in fixture["cases"]}
    cases = args.cases.split(",") if args.cases else list(known)
    for case in cases:
        if case not in known:
            raise BenchError(f"unknown case {case}; the fixture has {sorted(known)}")
    plan = plan_samples(cases, args.blocks, args.warmup_runs, args.cold_resume)
    if args.plan_only:
        print(json.dumps(plan, indent=2))
        return 0
    for case in cases:
        if Path(known[case]["output_directory"]).exists():
            raise BenchError(f"{known[case]['output_directory']} exists: a previous driver was "
                             "interrupted; move it away before running")
    out.mkdir(parents=True, exist_ok=True)
    manifest = {
        "schema": BENCH_SCHEMA,
        "status": "running",
        "created_utc": utc_now(),
        "label": args.label,
        "build_profile": args.build_profile,
        "driver": {"path": str(Path(__file__).resolve()), "sha256": sha256_file(Path(__file__)),
                   "argv": sys.argv[1:]},
        "fixture": {"root": str(fixture_root), "sha256": sha256_file(fixture_root / "fixture.json"),
                    "generator": fixture.get("generator"), "options": fixture.get("options"),
                    "workload": fixture.get("workload"),
                    "cases": {name: {"config": known[name]["config"],
                                     "initial_source": known[name].get("initial_source"),
                                     "first_iteration": known[name].get("first_iteration"),
                                     "iterations": known[name].get("iterations")}
                              for name in cases}},
        "arms": {"baseline": arms["A"], "candidate": arms["B"]},
        "env": shared_env,
        "host": host_record(),
        "policy": {
            "order": "per case: warm-up runs (A then B, excluded from comparison), then "
                     f"{args.blocks} A-B-B-A block(s), then optional cold-resume checks",
            "process": "every sample is a new process of the arm's binary",
            "page_cache": "not dropped between samples",
            "work_path": "each case writes to its fixed output directory; the driver moves "
                         "the run under runs/ after each sample",
            "reference": "each case's first completed sample; every later sample's run tree "
                         "is compared with it",
            "max_wall_seconds": args.max_wall_seconds,
            "keep_runs": args.keep_runs,
        },
        "plan": plan,
    }
    write_json(out / "bench.json", manifest)
    samples_path = out / "samples.jsonl"
    references: dict[str, dict] = {}
    samples: list[dict] = []
    for entry in plan:
        arm = arms[entry["arm_letter"]]
        case = known[entry["case"]]
        sample = run_sample(entry, arm, case, out, shared_env, args.max_wall_seconds)
        reference = references.get(entry["case"])
        if sample["ok"] and reference is None:
            references[entry["case"]] = sample
        elif sample["ok"]:
            comparison = TreeComparison(out / reference["run_directory"], out / sample["run_directory"],
                                        case["output_directory"],
                                        across_builds=reference["arm_letter"] != entry["arm_letter"])
            sample["parity"] = comparison.summary()
            sample["parity"]["reference_ordinal"] = reference["ordinal"]
        if (args.keep_runs == "reference" and sample["ok"]
                and references.get(entry["case"]) is not sample):
            shutil.rmtree(out / sample["run_directory"], ignore_errors=True)
            sample["run_pruned"] = True
        append_jsonl(samples_path, sample)
        samples.append(sample)
        status = sample["parity"]["status"] if sample.get("parity") else (
            "reference" if sample["ok"] else f"FAILED: {sample.get('error')}")
        print(f"[{entry['ordinal'] + 1}/{len(plan)}] {entry['case']} {entry['kind']} "
              f"{entry['position']} {arm['name']}: {sample['process']['wall_seconds']:.2f} s, "
              f"{sample['process']['max_rss_mib']:.0f} MiB, {status}", flush=True)
    changed = [arm["name"] for arm in arms.values() if sha256_file(Path(arm["binary"])) != arm["sha256"]]
    comparison = {
        "schema": COMPARISON_SCHEMA,
        "created_utc": utc_now(),
        "label": args.label,
        "baseline": arms["A"]["name"],
        "candidate": arms["B"]["name"],
        "rule": verdict.__doc__.strip(),
        "binaries_unchanged": not changed,
        "cases": {case: compare_case(case, [s for s in samples if s["case"] == case], arms)
                  for case in cases},
    }
    comparison["ok"] = not changed and all(c["parity_ok"] for c in comparison["cases"].values())
    write_json(out / "comparison.json", comparison)
    manifest.update(status="complete", completed_utc=utc_now(),
                    outputs={"samples.jsonl": sha256_file(samples_path),
                             "comparison.json": sha256_file(out / "comparison.json")})
    write_json(out / "bench.json", manifest)
    print_summary(comparison)
    return 0 if comparison["ok"] else 1


def run_sample(entry: dict, arm: dict, case: dict, out: Path, shared_env: dict,
               max_wall_seconds: float | None) -> dict:
    name = f"{entry['ordinal']:03d}-{entry['kind']}-{entry['position']}-{arm['name']}"
    directory = out / "runs" / entry["case"] / name
    directory.mkdir(parents=True)
    work = Path(case["output_directory"])
    config = case["config"]["path"]
    env = dict(os.environ)
    env.update(shared_env)
    env.update(arm["env"])
    env[DIAGNOSTICS_ENV] = str(directory / "diagnostics.jsonl")
    sample = {"schema": SAMPLE_SCHEMA, **entry, "arm": arm["name"], "binary_sha256": arm["sha256"],
              "directory": str(directory.relative_to(out)),
              "run_directory": str((directory / "run").relative_to(out))}
    processes = []
    try:
        if entry["kind"] == "cold_resume":
            if case.get("iterations", 0) < 2:
                raise BenchError("a cold resume needs a case with at least two iterations")
            first = run_process([arm["binary"], config, "--max-new-iterations", "1"], env,
                                directory / "stdout-first.jsonl", directory / "stderr-first.txt",
                                max_wall_seconds)
            processes.append(first)
            first_out = parse_stdout(directory / "stdout-first.jsonl")
            sample["first_process_completed"] = (first_out["result"] or {}).get("completed_iterations")
            if first["exit_code"] != 0 or sample["first_process_completed"] != 1:
                raise BenchError("the interrupted first process did not stop after one iteration")
        processes.append(run_process([arm["binary"], config], env, directory / "stdout.jsonl",
                                     directory / "stderr.txt", max_wall_seconds))
    except BenchError as error:
        sample["error"] = str(error)
    finally:
        if work.exists():
            shutil.move(str(work), str(directory / "run"))
    final = processes[-1] if processes else None
    sample["processes"] = processes
    sample["process"] = final or {"wall_seconds": 0.0, "max_rss_mib": 0.0}
    if final is not None and "error" not in sample:
        if final["timed_out"]:
            sample["error"] = f"exceeded {max_wall_seconds} s"
        elif final["exit_code"] != 0:
            sample["error"] = f"exit code {final['exit_code']}"
    stdout = parse_stdout(directory / "stdout.jsonl") if (directory / "stdout.jsonl").exists() else {
        "iterations": [], "result": None, "other_lines": 0}
    result = stdout["result"] or {}
    tree = directory / "run"
    prefix = str(work)
    sample["iterations_stdout"] = stdout["iterations"]
    sample["stdout_other_lines"] = stdout["other_lines"]
    sample["newly_completed_iterations"] = len(stdout["iterations"])
    sample["completed_iterations"] = result.get("completed_iterations")
    sample["complete"] = result.get("complete")
    sample["boundaries"] = boundaries(tree, prefix)
    sample["durable_completed_iterations"] = len(sample["boundaries"])
    sample["iteration_receipts"] = iteration_receipts(tree, prefix)
    sample["diagnostics"] = read_diagnostics(directory / "diagnostics.jsonl")
    sample["storage"] = storage(tree)
    if (tree / "run.json").is_file():
        recorded = read_json(tree / "run.json")
        sample["run_provenance"] = {key: recorded.get(key) for key in sorted(PROVENANCE_KEYS)}
    if "error" not in sample and result.get("complete") is not True:
        sample["error"] = "the run did not report completion"
    sample["ok"] = "error" not in sample
    sample["metrics"] = sample_metrics(sample) if sample["ok"] else {}
    return sample


def print_summary(comparison: dict) -> None:
    print(f"\n{comparison['baseline']} (baseline) vs {comparison['candidate']} (candidate); "
          f"ok={comparison['ok']}")
    shown = ("wall_seconds", "user_seconds", "max_rss_mib", "updates_per_hour",
             "scheduler.collection_execution_seconds", "scheduler.update_execution_seconds",
             "scheduler.update_validation_seconds", "receipts.checkpoint_io_seconds")
    for case, result in comparison["cases"].items():
        print(f"\ncase {case}: parity_ok={result['parity_ok']} timed={result['timed_samples']}")
        for name in shown:
            metric = result["metrics"].get(name)
            if not metric:
                continue
            base, cand = metric["baseline"].get("median"), metric["candidate"].get("median")
            ratio = metric.get("ratio_candidate_to_baseline")
            spread = metric.get("max_spread_fraction")
            print(f"  {name:42s} {base!s:>10.10} {cand!s:>10.10} "
                  f"ratio={'-' if ratio is None else f'{ratio:.3f}'} "
                  f"spread={'-' if spread is None else f'{spread:.3f}'} {metric['verdict']}")


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    commands = parser.add_subparsers(dest="command", required=True)
    gen = commands.add_parser("generate", help="write the fixture")
    gen.add_argument("--root", required=True, help="absolute; absent or empty")
    gen.add_argument("--seed", type=int, default=2_026_091_601)
    gen.add_argument("--updates", type=int, default=2, help="iterations per case")
    gen.add_argument("--games", type=int, default=2, help="episodes per iteration")
    gen.add_argument("--warmup-updates", type=int, default=None,
                     help="iterations run in process for the warm case (default --updates; 0: no warm case)")
    gen.add_argument("--collection-workers", type=int, default=1)
    gen.add_argument("--preparation-workers", type=int, default=1)
    gen.add_argument("--test-binary", help="a built mtg-kernel lib test executable (default: cargo test)")
    gen.add_argument("--repo", default=str(Path(__file__).resolve().parents[2]),
                     help="repository root for the cargo route")
    bench = commands.add_parser("run", help="time two builds on the fixture's cases")
    bench.add_argument("--fixture", required=True, help="the generated fixture root")
    bench.add_argument("--out", required=True, help="absolute; absent or empty")
    bench.add_argument("--baseline", required=True, help="NAME=BINARY (arm A)")
    bench.add_argument("--candidate", required=True, help="NAME=BINARY (arm B)")
    bench.add_argument("--cases", help="comma-separated (default: every case)")
    bench.add_argument("--blocks", type=int, default=1, help="A-B-B-A blocks per case")
    bench.add_argument("--warmup-runs", type=int, default=1, help="untimed runs per arm per case")
    bench.add_argument("--cold-resume", action="store_true",
                       help="per arm and case: stop after one iteration, finish in a new process, compare")
    bench.add_argument("--env", action="append", help="KEY=VALUE for every sample")
    bench.add_argument("--arm-env", action="append", help="ARM:KEY=VALUE for one arm's samples")
    bench.add_argument("--max-wall-seconds", type=float, default=None, help="per process")
    bench.add_argument("--keep-runs", choices=("all", "reference"), default="all",
                       help="'reference' deletes every other run tree once compared")
    bench.add_argument("--label", default="", help="e.g. 'local smoke, shared vCPU, noisy'")
    bench.add_argument("--build-profile", default="unspecified",
                       help="how both arms were built (they must match)")
    bench.add_argument("--plan-only", action="store_true", help="print the sample order and exit")
    args = parser.parse_args(argv)
    if args.command == "run" and (args.blocks < 1 or args.warmup_runs < 0):
        parser.error("--blocks must be positive and --warmup-runs nonnegative")
    try:
        return generate(args) if args.command == "generate" else run(args)
    except BenchError as error:
        print(f"bench: {error}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    sys.exit(main())
