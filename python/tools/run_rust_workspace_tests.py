"""Run the workspace suite and its snapshot timing gate without contention."""

from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import re
import subprocess
import sys

from run_rust_integration_tests import (MAX_COMMAND, SUITES, command_length, listed_tests, run_recorded,
                                        run_suite, suite_modules)
import rust_workspace_lib_plan as lib_plan


TIMING_TEST = "snapshot::tests::snapshot_clone_cost_is_bounded"
# The targets part names its benchmark targets one by one. `--benches` selects
# every target with `bench = true`, and the library has that by default, so
# with it the targets part compiled and ran the library's unit tests a second
# time (2026-11-16: all 3,841 test names of the lib part ran again there).
PARTS = {"lib": ["--lib"], "targets": ["--bins", "--examples"]}
NAMED_KINDS = {"test": "--test", "bench": "--bench"}
# From this script to its own Cargo runner process: the part of the library's
# tests to run ([index, count]), a file for the names of the binary's tests
# (and nothing run), and a file for the names of the tests a part executed.
SHARD_ENV = "MTG_CI_WORKSPACE_LIB_SHARD"
LIST_ENV = "MTG_CI_WORKSPACE_LIB_LIST"
EXECUTED_ENV = "MTG_CI_WORKSPACE_LIB_EXECUTED"
# libtest's last line: "test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 4227 filtered out; ...".
SUMMARY = re.compile(r"^test result: (ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored; (\d+) measured; "
                     r"(\d+) filtered out")


def default_named_targets() -> list[str]:
    """`--test NAME` or `--bench NAME` for every integration test and benchmark --all-targets would build."""
    return named_targets(json.loads(subprocess.run(
        ["cargo", "metadata", "--no-deps", "--locked", "--format-version", "1"],
        check=True, capture_output=True, text=True,
    ).stdout))


def named_targets(metadata: dict) -> list[str]:
    """The integration test and benchmark targets of `cargo metadata` output, as Cargo selection flags.

    Naming a target whose required features are off is an error, while
    --all-targets skips it, so keep only targets the default features enable.
    """
    selected = []
    for package in metadata["packages"]:
        features = package["features"]
        enabled, pending = set(), ["default"] if "default" in features else []
        while pending:
            name = pending.pop()
            if name in features and name not in enabled:
                enabled.add(name)
                pending.extend(features[name])
        for target in package["targets"]:
            flag = NAMED_KINDS.get(target["kind"][0]) if len(target["kind"]) == 1 else None
            if flag and set(target.get("required-features", [])) <= enabled:
                selected += [flag, target["name"]]
    return selected


def timing_options() -> dict:
    # Keep the short wall-clock measurement ahead of background Windows
    # services. Correctness tests retain normal priority.
    return {"creationflags": subprocess.HIGH_PRIORITY_CLASS} if sys.platform == "win32" else {}


def timing_command(executable: str) -> list[str]:
    return [executable, TIMING_TEST, "--exact", "--include-ignored", "--test-threads=1", "--nocapture"]


def run_library(executable: str) -> int:
    """The whole library in one process, then the timing gate alone (an unsplit run)."""
    result = subprocess.run([executable, "--skip", TIMING_TEST], check=False)
    if result.returncode:
        return result.returncode
    print(f"Running isolated timing gate: {TIMING_TEST}", flush=True)
    return subprocess.run(timing_command(executable), check=False, **timing_options()).returncode


def run_timing_gate(executable: str, listed: int) -> tuple[int, bool]:
    """Run the timing gate alone; (exit code, whether its output shows that test and only it ran).

    With one test thread and --nocapture, libtest writes "test <name> ... " when the test starts and the
    outcome after the test's own output, so the result is not one line. The evidence is the start line
    and libtest's summary: one passed, none failed or ignored, every other listed test filtered out.
    """
    print(f"Running isolated timing gate: {TIMING_TEST}", flush=True)
    lines = []
    sys.stdout.flush()
    with subprocess.Popen(timing_command(executable), stdout=subprocess.PIPE, **timing_options()) as process:
        for raw in process.stdout:
            sys.stdout.buffer.write(raw)
            sys.stdout.buffer.flush()
            lines.append(raw.decode("utf-8", "replace").rstrip("\r\n"))
    starts = [line for line in lines if line.startswith("test ") and " ... " in line]
    summaries = [match.groups() for match in map(SUMMARY.match, lines) if match]
    ran = (len(starts) == 1 and starts[0].startswith(f"test {TIMING_TEST} ... ")
           and summaries[-1:] == [("ok", "1", "0", "0", "0", str(listed - 1))])
    return process.returncode, ran


def list_library(executable: str, path: str) -> int:
    """The build job of the parts: write the names of the binary's tests and run none of them."""
    tests = listed_tests(executable)
    if not tests:
        raise ValueError(f"The library test binary lists no tests: {executable}")
    listing = {"attempt": int(os.environ.get("GITHUB_RUN_ATTEMPT") or 0), "tests": sorted(tests)}
    Path(path).write_text(json.dumps(listing, indent=1) + "\n", encoding="utf-8")
    print(f"Workspace library test list: {len(tests)} tests written, none run", flush=True)
    return 0


def library_plan(executable: str, count: int) -> tuple[list[str], list[list[str]], list[list[str]]]:
    """(the binary's tests, filters of each part, tests each part selects), with the whole plan checked.

    Every part computes the same plan from the binary's own list and the timing table, then asks the binary
    to list what each part's command selects and fails unless every test but the timing gate is in exactly
    one part, as the plan's model of libtest says.
    """
    listed = listed_tests(executable)
    if TIMING_TEST not in listed:
        raise ValueError(f"The timing gate {TIMING_TEST} is not a test of the binary")
    # --skip is a substring filter too: a test whose name contains the gate's would run in no part.
    hidden = sorted(test for test in listed if TIMING_TEST in test and test != TIMING_TEST)
    if hidden:
        raise ValueError(f"Tests the timing gate's --skip would hide: {hidden}")
    tests = [test for test in listed if test != TIMING_TEST]
    weight = lib_plan.load_weights()
    plan, walls = lib_plan.plan_parts(tests, count, weight)
    selections = []
    for part, filters in enumerate(plan):
        if not filters or any(item.startswith("-") for item in filters):
            raise ValueError(f"Part {part} of the plan has no filters or an option among them")
        selection = ["--skip", TIMING_TEST, *filters]
        if command_length([executable, "--list", "--format=terse", *selection]) > MAX_COMMAND:
            raise ValueError(f"The filters of part {part} are too long for one command: use more parts")
        selected = listed_tests(executable, *selection)
        if sorted(selected) != sorted(lib_plan.selected_by(filters, tests)):
            raise ValueError(f"The binary selects other tests for part {part} than the plan's model of libtest")
        selections.append(selected)
        print(f"Workspace library part {part}/{count}: {len(selected)} tests, {len(filters)} filters, "
              f"{walls[part] / 60:.1f} simulated minutes: " + " ".join(filters), flush=True)
    lib_plan.check_partition(selections, tests)
    print(f"Workspace library plan: {len(tests)} tests in {count} parts, each in exactly one part as the binary "
          f"lists them, and the timing gate in part 0", flush=True)
    return listed, plan, selections


def run_library_part(executable: str, index: int, count: int, executed_path: str) -> int:
    listed, plan, selections = library_plan(executable, count)
    selected = selections[index]
    command = [executable, "--skip", TIMING_TEST, *plan[index]]
    print(f"Running workspace library part {index}/{count}: {len(selected)} tests", flush=True)
    returncode, reported = run_recorded(command)
    # A passing process must have reported each of its tests (more than once
    # is a replayed line) and no test of another part.
    missing = sorted(set(selected) - set(reported))
    unexpected = sorted(set(reported) - set(selected))
    if not returncode and (missing or unexpected):
        print(f"Tests reported by part {index} differ from its selection: missing {missing[:20]} "
              f"({len(missing)}), unexpected {unexpected[:20]} ({len(unexpected)})", flush=True)
        returncode = 1
    executed = set(reported) & set(selected)
    failed = int(bool(returncode))
    if index == 0:
        # The timing gate runs once per system, in part 0 after its tests and
        # alone, as it ran after the whole library before the split.
        if failed:
            print(f"Timing gate not run: part {index} failed", flush=True)
        else:
            gate, ran = run_timing_gate(executable, len(listed))
            if ran:
                executed.add(TIMING_TEST)
            elif not gate:
                print(f"The timing gate's output does not show {TIMING_TEST} alone passing", flush=True)
            failed += int(bool(gate) or not ran)
    record = {"shard": [index, count], "attempt": int(os.environ.get("GITHUB_RUN_ATTEMPT") or 0),
              "listed": sorted(listed), "executed": sorted(executed), "failed": failed}
    Path(executed_path).write_text(json.dumps(record, indent=1) + "\n", encoding="utf-8")
    print(f"Workspace library part {index}/{count}: executed {len(executed)} of {len(listed)} tests, "
          f"{failed} failed process(es)", flush=True)
    return int(bool(failed))


def run_tests(executable: str, arguments: list[str]) -> int:
    """The Cargo runner: one test executable of the workspace suite."""
    if Path(executable).name.startswith("mtg_kernel-"):
        if arguments:
            raise ValueError("Workspace library runner expects no test arguments")
        if os.environ.get(LIST_ENV):
            return list_library(executable, os.environ[LIST_ENV])
        shard = json.loads(os.environ.get(SHARD_ENV, "null"))
        if shard is not None:
            return run_library_part(executable, shard[0], shard[1], os.environ[EXECUTED_ENV])
        return run_library(executable)
    if any(Path(executable).name.startswith(suite + "-") for suite in SUITES):
        if arguments:
            raise ValueError("Grouped workspace runner expects no test arguments")
        return run_suite(executable, allow_empty=True)
    return subprocess.run([executable, *arguments], check=False).returncode


def main() -> int:
    if len(sys.argv) > 1 and sys.argv[1] == "--run":
        executable, *arguments = sys.argv[2:]
        return run_tests(executable, arguments)

    parser = argparse.ArgumentParser(description=__doc__)
    # CI may run the library tests and every other target as parallel jobs.
    # The parts test disjoint targets and together equal --all-targets.
    parser.add_argument("--part", choices=PARTS)
    parser.add_argument("--no-fail-fast", action="store_true")
    # The library part only: build once and list, or run one part of its tests.
    parser.add_argument("--no-run", action="store_true", help="Build the library tests and run nothing")
    parser.add_argument("--list-only", type=Path, metavar="FILE",
                        help="Build the library tests, write their names to FILE and run none")
    parser.add_argument("--shard-index", type=int)
    parser.add_argument("--shard-count", type=int)
    parser.add_argument("--executed-list", type=Path, metavar="FILE",
                        help="With a shard: write the names of the tests it executed to FILE")
    args = parser.parse_args()
    sharded = args.shard_index is not None or args.shard_count is not None
    if (args.no_run or args.list_only or sharded or args.executed_list) and args.part != "lib":
        parser.error("--no-run, --list-only and the shard options need --part lib")
    if sum((args.no_run, args.list_only is not None, sharded)) > 1:
        parser.error("Choose one of --no-run, --list-only and a shard")
    if sharded and (args.shard_index is None or args.shard_count is None or args.executed_list is None
                    or not 0 <= args.shard_index < args.shard_count):
        parser.error("A shard needs 0 <= --shard-index < --shard-count and --executed-list")
    if args.executed_list and not sharded:
        parser.error("--executed-list needs a shard")
    targets = ["--all-targets"]
    if args.part:
        targets = PARTS[args.part]
        if args.part == "targets":
            targets = [*targets, *default_named_targets()]
    for suite in SUITES:
        suite_modules(Path(__file__).resolve().parents[2], suite)
    environment = os.environ.copy()
    for name in (SHARD_ENV, LIST_ENV, EXECUTED_ENV):
        environment.pop(name, None)
    # The runner process starts in the package directory, so files are named
    # by their full paths.
    if args.list_only:
        environment[LIST_ENV] = str(args.list_only.resolve())
    if sharded:
        environment[SHARD_ENV] = json.dumps([args.shard_index, args.shard_count])
        environment[EXECUTED_ENV] = str(args.executed_list.resolve())
    runner = [sys.executable, str(Path(__file__).resolve()), "--run"]
    configuration = "target.'cfg(all())'.runner=" + json.dumps(runner)
    # The build job and the parts run this same Cargo command, so a part
    # reuses the build job's output through Cargo's own freshness checks.
    return subprocess.run([
        "cargo", "test", "--release", "--locked", "--config", configuration,
        "--workspace", *targets, *(["--no-fail-fast"] if args.no_fail_fast else []),
        *(["--no-run"] if args.no_run else []),
    ], env=environment, check=False).returncode


if __name__ == "__main__":
    raise SystemExit(main())
