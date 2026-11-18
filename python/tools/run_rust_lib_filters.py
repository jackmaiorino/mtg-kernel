"""Build one release library and run its existing test filters through Cargo."""

from __future__ import annotations

import argparse
from concurrent.futures import ThreadPoolExecutor, as_completed
import hashlib
import json
import os
from pathlib import Path
import re
import shlex
import signal
import subprocess
import sys
import tempfile
import threading
import time


FILTERS_ENV = "MTG_CI_RUST_LIB_FILTERS"
FILTER_TIMEOUT_ENV = "MTG_CI_RUST_LIB_FILTER_TIMEOUT_SECONDS"
SHARD_ENV = "MTG_CI_RUST_LIB_SHARD"
NOCAPTURE_ENV = "MTG_CI_RUST_LIB_NOCAPTURE"
SKIP_IGNORED_ENV = "MTG_CI_RUST_LIB_SKIP_IGNORED"
ISOLATED_CASES_ENV = "MTG_CI_RUST_LIB_ISOLATED_CASES"
LIMITED_ONLY_ENV = "MTG_CI_RUST_LIB_LIMITED_ONLY"
LIMITED_FEATURE = "limited-fdn-fixtures"
# Limited-only cases that fail and are tracked in the named issue; skipped by
# name until fixed.
KNOWN_FAILURES_PATH = Path(__file__).with_name("rust_limited_only_known_failures.json")
# Recorded seconds per isolated case on a hosted Linux runner, used only to
# balance shards; refresh with update_rust_case_seconds.py.
CASE_SECONDS_PATH = Path(__file__).with_name("rust_case_seconds.json")


def case_seconds() -> tuple[dict[str, float], float]:
    recorded = json.loads(CASE_SECONDS_PATH.read_text()) if CASE_SECONDS_PATH.is_file() else {}
    # Names without a recorded time get the mean, so a new test is never
    # assumed to be cheap.
    return recorded, (sum(recorded.values()) / len(recorded) if recorded else 1.0)


def longest_first(names: list[str]) -> list[str]:
    recorded, default = case_seconds()
    return sorted(names, key=lambda name: (-recorded.get(name, default), name))


def balanced_shard(names: list[str], index: int, count: int) -> list[str]:
    """Longest-first assignment by recorded case seconds.

    Every shard computes the same complete, disjoint partition from the same
    inventory and table.
    """
    recorded, default = case_seconds()
    loads = [0.0] * count
    members: list[list[str]] = [[] for _ in range(count)]
    for name in longest_first(names):
        target = min(range(count), key=lambda shard: (loads[shard], shard))
        loads[target] += recorded.get(name, default)
        members[target].append(name)
    return sorted(members[index])


def valid_filters(value: object) -> list[str]:
    if not isinstance(value, list) or not value or any(
        not isinstance(item, str) or not item or item.startswith("-") for item in value
    ):
        raise ValueError("Expected one or more nonempty test filters")
    return value


def workflow_limited_filters(path: Path) -> list[str]:
    """Library filters that the workflow's Limited steps already select.

    Reads every run_rust_lib_filters.py call and plain `cargo test --lib`
    call whose features include the Limited fixtures. Libtest matches these
    filters as substrings, as here.
    """
    filters = []
    for line in path.read_text(encoding="utf-8").splitlines():
        if LIMITED_FEATURE not in line:
            continue
        if "run_rust_lib_filters.py" in line:
            words = shlex.split(line.split("run_rust_lib_filters.py", 1)[1])
            valued = {"--features", "--shard-index", "--shard-count", "--filter-timeout-seconds"}
        elif "cargo test" in line and "--lib" in line.split():
            words = shlex.split(line.split("--lib", 1)[1].split(" -- ", 1)[0])
            valued = {"--features", "--test", "--bin", "-p", "--package"}
        else:
            continue
        if "--limited-only-workflow" in words:
            continue  # the Limited-only call itself selects no filter
        index = 0
        while index < len(words):
            word = words[index]
            if word == "--exact":
                raise ValueError(f"Unsupported option in a Limited filter call: {line.strip()}")
            if word in valued:
                index += 2
                continue
            if not word.startswith("-"):
                filters.append(word)
            index += 1
    if not filters:
        raise ValueError(f"No Limited library filters found in {path}")
    return sorted(set(filters))


def compiled_names(executable: str, test_filter: str, timeout: int, *, ignored: bool) -> list[str]:
    # Pinned libtest's pretty listing includes a final count; terse does not.
    # An empty filter lists the whole library.
    command = [executable, *([test_filter] if test_filter else []), "--list", "--format", "pretty",
               "--color", "never"]
    if ignored:
        command.append("--ignored")
    result = subprocess.run(command, check=False, capture_output=True, text=True, timeout=timeout)
    if result.returncode:
        print(result.stdout, end="", flush=True)
        print(result.stderr, end="", file=sys.stderr, flush=True)
        result.check_returncode()
    return listed_names(result.stdout, test_filter)


def default_library_names() -> list[str]:
    """Every library case of the default-feature build the workspace suite runs."""
    result = subprocess.run(
        ["cargo", "test", "--release", "--locked", "-p", "mtg-kernel", "--lib", "--",
         "--list", "--format", "pretty", "--color", "never"],
        check=False, stdout=subprocess.PIPE, text=True,
    )
    if result.returncode:
        print(result.stdout, end="", flush=True)
        result.check_returncode()
    return listed_names(result.stdout, "")


DEFAULT_LIST_SCHEMA = "mtg-ci-default-library-cases/v2"
# Accept a list without checking where it came from. Only for tests run where the checkout cannot name its own
# revision or compiler; CI never sets it.
UNVERIFIED_LIST_ENV = "MTG_CI_DEFAULT_LIST_UNVERIFIED_FOR_TEST"
REPO_ROOT = Path(__file__).resolve().parents[2]


def list_provenance() -> dict:
    """What decides the default-feature library's case list: the source revision, the feature set and the compiler.

    The revision is GITHUB_SHA when set (the commit every job of a CI run checks out), else git's HEAD. The
    compiler is the pinned toolchain's rustc, run in the repository so rust-toolchain.toml selects it. A value
    that cannot be found is None.
    """
    commit = os.environ.get("GITHUB_SHA") or None
    if commit is None:
        try:
            result = subprocess.run(["git", "rev-parse", "HEAD"], cwd=REPO_ROOT, capture_output=True, text=True,
                                    check=False)
            commit = (result.stdout.strip() or None) if result.returncode == 0 else None
        except OSError:
            commit = None
    host = release = None
    try:
        result = subprocess.run(["rustc", "-vV"], cwd=REPO_ROOT, capture_output=True, text=True, check=False)
        if result.returncode == 0:
            fields = dict(line.split(": ", 1) for line in result.stdout.splitlines() if ": " in line)
            host, release = fields.get("host"), fields.get("release")
    except OSError:
        pass
    return {"commit": commit, "features": "default", "rustc_host": host, "rustc_release": release}


def write_default_library_list(path: Path) -> list[str]:
    """Build the default-feature library test binary once and record its case names for the Limited-only shards,
    with the revision and build they belong to."""
    names = default_library_names()
    record = {"schema": DEFAULT_LIST_SCHEMA, "provenance": list_provenance(), "count": len(names), "names": names}
    path.write_text(json.dumps(record) + "\n", encoding="utf-8")
    print(f"Default library cases: {len(names)} written to {path} for {record['provenance']}", flush=True)
    return names


def read_default_library_list(path: Path) -> list[str]:
    """The names of a list written at this revision and build.

    A name in the list is subtracted from the Limited inventory, so a list from another revision could remove a
    case that has since become Limited-only, and that case would run nowhere. Such a list is refused, and so is
    any list when this checkout cannot name its own revision and compiler (unless UNVERIFIED_LIST_ENV is 1).
    """
    record = json.loads(path.read_text(encoding="utf-8"))
    names = record.get("names") if isinstance(record, dict) else None
    if (not isinstance(names, list) or not names or record.get("schema") != DEFAULT_LIST_SCHEMA
            or record.get("count") != len(names)
            or any(not isinstance(name, str) or not name for name in names)
            or names != sorted(set(names))):
        raise ValueError(f"Not a default library case list: {path}")
    own = list_provenance()
    if any(value is None for value in own.values()):
        if os.environ.get(UNVERIFIED_LIST_ENV) != "1":
            raise ValueError(f"This checkout cannot name its revision and compiler ({own}), so it cannot check "
                             f"that {path} belongs to it")
    elif record.get("provenance") != own:
        raise ValueError(f"Default library case list {path} comes from another revision or build: "
                         f"{record.get('provenance')}, this job is {own}")
    print(f"Default library cases: {len(names)} read from {path} for {own}", flush=True)
    return names


def listed_names(listing: str, test_filter: str) -> list[str]:
    names = []
    summary_count = None
    for line in listing.splitlines():
        if not line.strip():
            continue
        if summary_count is not None:
            raise ValueError("Compiled test inventory has records after its summary")
        summary = re.fullmatch(r"(\d+) tests?, (\d+) benchmarks?", line)
        if summary:
            summary_count = int(summary[1])
            if int(summary[2]) != 0:
                raise ValueError("Sharded library inventory contains benchmarks")
            continue
        if not line.endswith(": test"):
            raise ValueError(f"Unexpected compiled test inventory line: {line!r}")
        name = line.removesuffix(": test")
        if not name or test_filter not in name or name in names:
            raise ValueError(f"Invalid or duplicate compiled test name: {name!r}")
        names.append(name)
    if summary_count is None or summary_count != len(names):
        raise ValueError("Compiled test inventory count is missing or inconsistent")
    return sorted(names)


def case_resources() -> tuple[int, dict]:
    telemetry = {"cpu_count": os.cpu_count(), "process_cpu_count": None, "affinity": None, "cgroup": {}}
    counts = [4]
    if telemetry["cpu_count"]:
        counts.append(telemetry["cpu_count"])
    if hasattr(os, "process_cpu_count"):
        telemetry["process_cpu_count"] = os.process_cpu_count()
        if telemetry["process_cpu_count"]:
            counts.append(telemetry["process_cpu_count"])
    if hasattr(os, "sched_getaffinity"):
        try:
            telemetry["affinity"] = sorted(os.sched_getaffinity(0))
            if telemetry["affinity"]:
                counts.append(len(telemetry["affinity"]))
        except OSError as error:
            telemetry["affinity_error"] = str(error)
    # These are observations of the standard cgroup paths, not a placement claim.
    for name in ("cpu.max", "cpu/cpu.cfs_quota_us", "cpu/cpu.cfs_period_us"):
        path = Path("/sys/fs/cgroup") / name
        try:
            telemetry["cgroup"][name] = path.read_text().strip()
        except OSError:
            pass
    quota = telemetry["cgroup"].get("cpu.max", "").split()
    if len(quota) == 2 and quota[0].isdigit() and quota[1].isdigit() and int(quota[1]) > 0:
        counts.append(max(1, (int(quota[0]) + int(quota[1]) - 1) // int(quota[1])))
    elif (telemetry["cgroup"].get("cpu/cpu.cfs_quota_us", "").isdigit()
          and telemetry["cgroup"].get("cpu/cpu.cfs_period_us", "").isdigit()):
        quota_us = int(telemetry["cgroup"]["cpu/cpu.cfs_quota_us"])
        period_us = int(telemetry["cgroup"]["cpu/cpu.cfs_period_us"])
        if quota_us > 0 and period_us > 0:
            counts.append(max(1, (quota_us + period_us - 1) // period_us))
    return min(counts), telemetry


def run_isolated_cases(executable: str, selected: list[str], timeout: int, ignored: list[str]) -> int:
    workers, telemetry = case_resources()
    print(json.dumps({"isolated_case_workers": workers, "case_timeout_seconds": timeout,
                      "resources": telemetry}), flush=True)
    active: dict[str, subprocess.Popen] = {}
    lock, output_lock, stopping = threading.Lock(), threading.Lock(), threading.Event()

    def emit(event: str, **fields) -> None:
        with output_lock:
            sys.stdout.write(json.dumps({"event": event, **fields}) + "\n")
            sys.stdout.flush()

    def stop_child(child: subprocess.Popen) -> None:
        # Only Popen objects created below are registered. Never kill by name,
        # traverse another process tree, or signal a foreign process/group.
        if child.poll() is None:
            child.kill()
        child.wait(timeout=10)

    def run_case(name: str) -> dict:
        started = time.monotonic()
        row = {"case": name, "status": "not_started", "returncode": None}
        child = None
        try:
            with lock:
                if stopping.is_set():
                    return row
                command = [executable, "--exact", name, "--nocapture", "--test-threads", "1"]
                # Inherit stdout/stderr so a panic is visible immediately. The
                # surrounding named records attribute start, finish and timeout.
                child = subprocess.Popen(command)
                active[name] = child
            started = time.monotonic()
            emit("rust_test_case_started", case=name, pid=child.pid, timeout_seconds=timeout)
            deadline = started + timeout
            while True:
                if stopping.is_set() or time.monotonic() >= deadline:
                    row["status"] = "interrupted" if stopping.is_set() else "timeout"
                    emit("rust_test_case_" + row["status"], case=name, pid=child.pid,
                         elapsed_seconds=time.monotonic() - started)
                    stop_child(child)
                    row["returncode"] = 130 if stopping.is_set() else 124
                    break
                try:
                    row["returncode"] = child.wait(timeout=min(1, max(0.001, deadline - time.monotonic())))
                    row["status"] = "passed" if row["returncode"] == 0 else "failed"
                    break
                except subprocess.TimeoutExpired:
                    pass
        except Exception as error:
            row.update(status="execution_error", returncode=1, error=str(error))
            if child is not None:
                try:
                    stop_child(child)
                except Exception as cleanup_error:
                    row["cleanup_error"] = str(cleanup_error)
                    # An unreaped child must not allow replacements beyond the
                    # cap. Ordinary failures/timeouts still attempt every case.
                    stopping.set()
        finally:
            with lock:
                if child is None or child.poll() is not None:
                    active.pop(name, None)
            row["elapsed_seconds"] = time.monotonic() - started
            if child is not None:
                row["child_returncode"] = child.returncode
            emit("rust_test_case_finished", **row)
        return row

    def terminate(signum, _frame):
        raise SystemExit(128 + signum)

    previous = signal.signal(signal.SIGTERM, terminate)
    pool = ThreadPoolExecutor(max_workers=workers)
    futures = []
    results = {}
    try:
        # Start the longest recorded cases first so they overlap the short ones.
        futures = [pool.submit(run_case, name) for name in longest_first(selected)]
        for future in as_completed(futures):
            row = future.result()
            results[row["case"]] = row
    finally:
        stopping.set()
        for future in futures:
            future.cancel()
        with lock:
            children = list(active.values())
        for child in children:
            try:
                stop_child(child)
            except Exception as error:
                emit("rust_test_child_cleanup_failed", pid=child.pid, error=str(error))
        pool.shutdown(wait=True, cancel_futures=True)
        signal.signal(signal.SIGTERM, previous)
    ordered = [results[name] for name in selected]
    emit("rust_isolated_case_summary", results=ordered, compiled_ignored_names=ignored,
         ignored_executed=0, selected_count=len(selected))
    return next((row["returncode"] or 1 for row in ordered if row["status"] != "passed"), 0)


def main() -> int:
    if len(sys.argv) > 1 and sys.argv[1] == "--run":
        if len(sys.argv) != 3:
            raise ValueError("Cargo runner expects exactly one library test executable")
        limited_only = os.environ.get(LIMITED_ONLY_ENV, "")
        filters = [] if limited_only else valid_filters(json.loads(os.environ[FILTERS_ENV]))
        filter_timeout = int(os.environ.get(FILTER_TIMEOUT_ENV, "1800"))
        if filter_timeout <= 0:
            raise ValueError("Library filter timeout must be positive")
        nocapture = os.environ.get(NOCAPTURE_ENV, "0") == "1"
        skip_ignored = os.environ.get(SKIP_IGNORED_ENV, "0") == "1"
        isolated_cases = os.environ.get(ISOLATED_CASES_ENV, "0") == "1"
        shard = json.loads(os.environ.get(SHARD_ENV, "null"))
        if isolated_cases and shard is None:
            raise ValueError("Isolated cases require compiled-name sharding")
        if limited_only and not isolated_cases:
            raise ValueError("Limited-only cases require isolated cases")
        commands = [(test_filter, [sys.argv[2], test_filter]) for test_filter in filters]
        if shard is not None:
            if (
                not isinstance(shard, list) or len(shard) != 2
                or any(type(value) is not int for value in shard)
                or not 0 <= shard[0] < shard[1]
            ):
                raise ValueError("Expected a valid shard index/count")
            # Several filters share one balanced partition of their union.
            # Limited-only cases start from the whole library.
            listed = filters or [""]
            names = sorted({name for test_filter in listed for name in compiled_names(
                sys.argv[2], test_filter, filter_timeout, ignored=False)})
            ignored = sorted({name for test_filter in listed for name in compiled_names(
                sys.argv[2], test_filter, filter_timeout, ignored=True)})
            if limited_only:
                # Cases only the Limited build compiles: the workspace suite
                # never runs them, so run each one no Limited step selects.
                selection = json.loads(Path(limited_only).read_text())
                default, covered = set(selection["default"]), selection["covered"]
                names = [name for name in names
                         if name not in default and not any(f in name for f in covered)]
                known = json.loads(KNOWN_FAILURES_PATH.read_text())
                stale = sorted(set(known["cases"]) - set(names))
                if stale:
                    raise ValueError(f"Known failures are no longer Limited-only cases: {stale}")
                print(json.dumps({"skipped_known_failures": known["cases"], "issue": known["issue"]}),
                      flush=True)
                names = [name for name in names if name not in known["cases"]]
                ignored = [name for name in ignored if name in names]
                filters = [f"Limited-only cases outside {len(covered)} workflow filters"]
            if not names or not set(ignored) <= set(names) or (ignored and not skip_ignored):
                raise ValueError("Sharded inventory is empty or contains unexpected ignored tests")
            # Preserve existing #[ignore] behavior only when explicitly requested.
            # Log every skipped compiled name; never enable its unregistered fixture.
            runnable = [name for name in names if name not in ignored]
            selected = balanced_shard(runnable, shard[0], shard[1])
            if not selected:
                raise ValueError("Compiled test shard is empty")
            encoded = json.dumps(names, separators=(",", ":")).encode("utf-8")
            print(json.dumps({
                "filter": " ".join(filters), "inventory": names, "count": len(names),
                "inventory_sha256": hashlib.sha256(encoded).hexdigest(),
                "ignored": ignored, "runnable_count": len(runnable),
                "shard_index": shard[0], "shard_count": shard[1], "selected": selected,
            }), flush=True)
            if isolated_cases:
                return run_isolated_cases(sys.argv[2], selected, filter_timeout, ignored)
            commands = [(f"{' '.join(filters)} shard {shard[0]}/{shard[1]}",
                         [sys.argv[2], "--exact", *selected])]
        failed_filters = []
        first_failure = 0
        for test_filter, command in commands:
            if nocapture:
                command.append("--nocapture")
            print(f"Running compiled library filter: {test_filter}", flush=True)
            try:
                result = subprocess.run(command, check=False, timeout=filter_timeout)
                returncode = result.returncode
            except subprocess.TimeoutExpired:
                print(f"Compiled library filter timed out after {filter_timeout}s: {test_filter}", flush=True)
                returncode = 124
            if returncode:
                failed_filters.append(test_filter)
                if not first_failure:
                    first_failure = returncode
        if failed_filters:
            print("Failed compiled library filters: " + json.dumps(failed_filters), flush=True)
        return first_failure

    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--features")
    parser.add_argument("--filter-timeout-seconds", type=int, help="Seconds per filter or case (default 1800)")
    parser.add_argument("--shard-index", type=int)
    parser.add_argument("--shard-count", type=int)
    parser.add_argument("--nocapture", action="store_true")
    parser.add_argument("--skip-ignored", action="store_true")
    parser.add_argument("--isolated-cases", action="store_true",
                        help="Run each compiled shard name with its own timeout and at most four children")
    parser.add_argument("--limited-only-workflow", type=Path,
                        help="Run every case only the Limited build compiles that no Limited "
                             "filter in this workflow selects")
    parser.add_argument("--default-library-list", type=Path,
                        help="With --limited-only-workflow: the default-feature library case names written by "
                             "--write-default-library-list, instead of building that library here")
    parser.add_argument("--write-default-library-list", type=Path,
                        help="Only build the default-feature library test binary and write its case names here")
    parser.add_argument("filters", nargs="*")
    args = parser.parse_args()
    if args.write_default_library_list is not None:
        if (args.features is not None or args.filters or args.limited_only_workflow is not None
                or args.default_library_list is not None or args.shard_index is not None
                or args.shard_count is not None or args.filter_timeout_seconds is not None
                or args.nocapture or args.skip_ignored or args.isolated_cases):
            parser.error("--write-default-library-list takes no other option")
        write_default_library_list(args.write_default_library_list)
        return 0
    if args.features is None:
        parser.error("--features is required")
    if args.default_library_list is not None and args.limited_only_workflow is None:
        parser.error("--default-library-list requires --limited-only-workflow")
    if args.limited_only_workflow is not None:
        if args.filters:
            parser.error("--limited-only-workflow takes no filters")
        if not args.isolated_cases:
            parser.error("--limited-only-workflow requires --isolated-cases")
        covered = workflow_limited_filters(args.limited_only_workflow)
        default = (read_default_library_list(args.default_library_list) if args.default_library_list is not None
                   else default_library_names())
        selection = Path(tempfile.mkdtemp()) / "limited_only_selection.json"
        selection.write_text(json.dumps({"default": default, "covered": covered}))
        filters = []
    else:
        filters = valid_filters(args.filters)
    if args.filter_timeout_seconds is None:
        args.filter_timeout_seconds = 1800
    if args.filter_timeout_seconds <= 0:
        parser.error("--filter-timeout-seconds must be positive")
    shard = None
    if args.shard_index is not None or args.shard_count is not None:
        if (args.shard_index is None or args.shard_count is None
                or not 0 <= args.shard_index < args.shard_count):
            parser.error("Sharding requires 0 <= --shard-index < --shard-count")
        shard = [args.shard_index, args.shard_count]
    if args.skip_ignored and shard is None:
        parser.error("--skip-ignored requires sharding")
    if args.isolated_cases and shard is None:
        parser.error("--isolated-cases requires sharding")
    environment = os.environ.copy()
    environment[FILTERS_ENV] = json.dumps(filters)
    environment[FILTER_TIMEOUT_ENV] = str(args.filter_timeout_seconds)
    environment[SHARD_ENV] = json.dumps(shard)
    environment[NOCAPTURE_ENV] = "1" if args.nocapture else "0"
    environment[SKIP_IGNORED_ENV] = "1" if args.skip_ignored else "0"
    environment[ISOLATED_CASES_ENV] = "1" if args.isolated_cases else "0"
    environment[LIMITED_ONLY_ENV] = str(selection) if args.limited_only_workflow else ""
    runner = [sys.executable, str(Path(__file__).resolve()), "--run"]
    configuration = "target.'cfg(all())'.runner=" + json.dumps(runner)
    command = [
        "cargo", "test", "--release", "--locked", "--config", configuration,
        "-p", "mtg-kernel", "--features", args.features, "--lib",
    ]
    # Cargo supplies its normal package cwd and runtime environment to the
    # runner. Each filter still gets a fresh test process and its exit code.
    return subprocess.run(command, env=environment, check=False).returncode


if __name__ == "__main__":
    raise SystemExit(main())
