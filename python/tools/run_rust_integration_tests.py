"""Run consolidated integration suites with the original per-file processes."""

from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import re
import subprocess
import sys


SUITES = {"fdn_integration": "fdn", "standard_integration": "standard",
          "pauper_integration": "pauper", "eternal_integration": "eternal"}
# Recorded seconds per module of a suite on a hosted Linux runner, used only
# to balance shards: a module's time runs from its "Running integration
# module" line to the next one in the job log.
MODULE_SECONDS_PATH = Path(__file__).with_name("rust_integration_module_seconds.json")
# The shard's module inventory and selection, from this script to its own
# Cargo runner process.
SHARD_ENV = "MTG_CI_RUST_INTEGRATION_SHARD"
# A file for the names of the binary's tests (and nothing run), and a file for
# the names of the tests a run executed, also for the runner process.
LIST_ENV = "MTG_CI_RUST_INTEGRATION_LIST"
EXECUTED_ENV = "MTG_CI_RUST_INTEGRATION_EXECUTED"
# A line libtest prints when a test ends: "test <name> ... ok", also with
# "- should panic" after the name, FAILED, or ignored (with or without a reason).
TEST_LINE = re.compile(r"^test (\S+)(?: - should panic)? \.\.\. (ok|FAILED|ignored)\b")
# Longest command line for one test process. Windows refuses more than 32,767
# characters; a unit that needs more must be split in the plan.
MAX_COMMAND = 30000


def suite_modules(root: Path, suite: str) -> list[str]:
    directory = root / "mtg-kernel" / "tests"
    source = (directory / f"{suite}.rs").read_text()
    declared = re.findall(r"^mod (\w+);$", source, re.MULTILINE)
    expected = sorted(path.stem for path in (directory / SUITES[suite]).glob("*.rs"))
    ungrouped = [path.name for path in directory.glob(SUITES[suite] + "_*.rs")
                 if path.stem != suite]
    if ungrouped:
        raise ValueError(f"{suite}: ungrouped integration sources: {ungrouped}")
    if not expected or sorted(declared) != expected:
        raise ValueError(f"{suite}: module declarations do not match source inventory")
    return sorted(declared)


def module_seconds(suite: str, inventory: list[str],
                   path: Path = MODULE_SECONDS_PATH) -> tuple[dict[str, float], float]:
    table = json.loads(path.read_text(encoding="utf-8")) if path.is_file() else {}
    recorded = table.get(suite, {})
    # A module without a recorded time gets the mean of the recorded modules
    # still in the inventory, so a new module is never assumed to be cheap
    # and a removed module's time does not count.
    current = set(inventory)
    present = [seconds for module, seconds in recorded.items() if module in current]
    return recorded, (sum(present) / len(present) if present else 1.0)


def isolated_tests(suite: str, path: Path = MODULE_SECONDS_PATH) -> dict[str, list[str]]:
    """{module: tests that run as units of their own}, from the table's "isolated" entry.

    One test can take most of its module's time (on 2026-11-24 one of the 19
    tests of eternal_bo3_v1 ran 16 of the module's 22 minutes). Naming it here
    lets the plan place it apart from the rest of its module.
    """
    table = json.loads(path.read_text(encoding="utf-8")) if path.is_file() else {}
    isolated = table.get("isolated", {}).get(suite, {})
    for module, names in isolated.items():
        if not names or len(set(names)) != len(names) or any("::" in name or not name for name in names):
            raise ValueError(f"{suite}: isolated tests of {module} must be distinct names without the module")
    return {module: list(names) for module, names in isolated.items()}


def plan_units(modules: list[str], isolated: dict[str, list[str]]) -> list[str]:
    """The units a plan places: each module, and each isolated test of a module in the inventory.

    The unit `module` stands for every test of the module that is not a unit
    of its own, so a test is in exactly one unit whatever the binary holds: an
    isolated name the binary lacks is an empty unit, and a test added to the
    module later belongs to the module's unit.
    """
    units = list(modules)
    for module in modules:
        units += [f"{module}::{name}" for name in isolated.get(module, [])]
    return units


def unit_tests(unit: str, units: list[str], tests: list[str]) -> list[str]:
    """The binary's tests that belong to one unit of the plan."""
    if "::" in unit:
        return [test for test in tests if test == unit]
    apart = {other for other in units if other.startswith(unit + "::")}
    return [test for test in tests if test.startswith(unit + "::") and test not in apart]


def check_assignment(plan: list[list[str]], tests: list[str]) -> None:
    """Fail unless every test of the binary is in exactly one shard of the plan."""
    units = [unit for shard in plan for unit in shard]
    if len(set(units)) != len(units):
        raise ValueError("A plan unit is in more than one shard")
    counts = dict.fromkeys(tests, 0)
    if len(counts) != len(tests):
        raise ValueError("The binary lists a test name twice")
    for unit in units:
        for test in unit_tests(unit, units, tests):
            counts[test] += 1
    wrong = sorted(test for test, count in counts.items() if count != 1)
    if wrong:
        raise ValueError(f"Tests not in exactly one shard: {wrong}")


def shard_plan(modules: list[str], count: int, recorded: dict[str, float], default: float) -> list[list[str]]:
    """Longest-first assignment of whole modules to `count` shards.

    Modules go heaviest first (ties by name) to the shard with the least
    recorded load (ties to the lowest index). Every shard job computes the
    same plan from the same inventory and table, and the plan is checked to
    hold every module exactly once. A module's shard depends only on the
    modules placed before it, so adding a module never moves one recorded as
    heavier than the default weight.
    """
    if count < 1:
        raise ValueError("Shard count must be at least 1")
    if len(set(modules)) != len(modules):
        raise ValueError("Duplicate integration modules")
    loads = [0.0] * count
    members: list[list[str]] = [[] for _ in range(count)]
    for module in sorted(modules, key=lambda name: (-recorded.get(name, default), name)):
        target = min(range(count), key=lambda shard: (loads[shard], shard))
        loads[target] += recorded.get(module, default)
        members[target].append(module)
    plan = [sorted(shard) for shard in members]
    placed = [module for shard in plan for module in shard]
    if sorted(placed) != sorted(modules):
        raise ValueError("Shard plan does not hold every module exactly once")
    if any(not shard for shard in plan):
        raise ValueError(f"Shard plan has an empty shard: {len(modules)} modules in {count} shards")
    return plan


def report_plan(suite: str, plan: list[list[str]], index: int, recorded: dict[str, float],
                default: float) -> None:
    count = len(plan)
    for shard, members in enumerate(plan):
        minutes = sum(recorded.get(module, default) for module in members) / 60
        unrecorded = [module for module in members if module not in recorded]
        marker = " (this job)" if shard == index else ""
        print(f"Integration shard {shard}/{count}{marker}: {len(members)} modules, "
              f"{minutes:.1f} recorded minutes, {len(unrecorded)} at the default weight: "
              + " ".join(members), flush=True)
    stale = sorted(set(recorded) - {module for members in plan for module in members})
    if stale:
        print(f"Recorded modules no longer in {suite}, ignored: {' '.join(stale)}", flush=True)
    print(f"Integration shard plan: {sum(map(len, plan))} modules of {suite} in {count} shards, "
          f"each in exactly one shard", flush=True)


def listed_tests(executable: str) -> list[str]:
    result = subprocess.run(
        [executable, "--list", "--format=terse"], check=True, capture_output=True, text=True,
    )
    return [line.removesuffix(": test") for line in result.stdout.splitlines()
            if line.endswith(": test")]


def run_recorded(command: list[str]) -> tuple[int, list[str]]:
    """Run one test process, pass its output through, and name the tests it reported.

    Only standard output is read, where libtest prints one line as each test
    ends; standard error still goes straight to the job log. Every line of
    that shape is returned, in order. That is evidence that a test ran, not
    what decides which tests run: a test, or a child process that shares its
    standard output, can print such a line too, and libtest replays what a
    passing test printed under --show-output. The caller therefore selects
    tests by exact name and only requires a line for each of them.
    """
    reported = []
    sys.stdout.flush()
    with subprocess.Popen(command, stdout=subprocess.PIPE) as process:
        for raw in process.stdout:
            sys.stdout.buffer.write(raw)
            sys.stdout.buffer.flush()
            match = TEST_LINE.match(raw.decode("utf-8", "replace"))
            if match:
                reported.append(match.group(1))
    return process.returncode, reported


def run_suite(executable: str, *, allow_empty: bool = False, selection: dict | None = None,
              list_path: str | None = None, executed_path: str | None = None) -> int:
    tests = listed_tests(executable)
    if not tests and not allow_empty:
        raise ValueError(f"Consolidated suite has no tests: {executable}")
    if any("::" not in test for test in tests):
        raise ValueError("Consolidated tests must belong to original source modules")
    if list_path:
        # The build job of a suite run in parts: name the binary's tests and
        # run none of them.
        listing = {"attempt": int(os.environ.get("GITHUB_RUN_ATTEMPT") or 0), "tests": sorted(tests)}
        Path(list_path).write_text(json.dumps(listing, indent=1) + "\n", encoding="utf-8")
        print(f"Integration test list: {len(tests)} tests written, none run", flush=True)
        return 0
    modules = sorted({test.split("::", 1)[0] for test in tests})
    compiled = modules
    # A unit is a module or, in a plan with isolated tests, one test.
    units, every_unit = modules, modules
    if selection is not None:
        # A compiled module outside the planned inventory would belong to no
        # shard; fail rather than skip it.
        outside = sorted(set(modules) - set(selection["inventory"]))
        if outside:
            raise ValueError(f"Compiled integration modules in no shard: {outside}")
        every_unit = list(selection["selected"])
        if "plan" in selection:
            # Every part checks the whole plan against the binary it runs.
            check_assignment(selection["plan"], tests)
            if selection["plan"][selection["shard"][0]] != selection["selected"]:
                raise ValueError("The selected units are not this shard of the plan")
            every_unit = [unit for shard in selection["plan"] for unit in shard]
        selected = set(selection["selected"])
        units = sorted(selected)
        for module in sorted(selected - set(compiled)):
            if "::" not in module:
                print(f"Integration module without compiled tests: {module}", flush=True)
    failures = 0
    expected: list[str] = []
    executed: set[str] = set()
    for unit in units:
        module = unit.split("::", 1)[0]
        selected_tests = unit_tests(unit, every_unit, tests)
        if not selected_tests:
            if "::" in unit:
                print(f"Isolated integration test not in the binary: {unit}", flush=True)
            continue
        expected += selected_tests
        # Each original file gets a fresh process, with its existing internal
        # libtest concurrency and ignored-test behavior. An unsharded run
        # selects a module by its prefix, as it always has; the trailing ::
        # keeps `alpha` from selecting `alphabet`. libtest's filter is a
        # substring match, though, so `alpha::` also selects
        # `beta_alpha::one`: with the two modules in different parts that
        # test would run in both. A run in parts therefore names every test
        # of a unit exactly, and a process can execute nothing else.
        print(f"Running integration module: {unit} ({len(selected_tests)} listed tests)", flush=True)
        if selection is None:
            command = [executable, module + "::"]
        else:
            command = [executable, "--exact", *selected_tests]
            if sum(map(len, command)) + len(command) > MAX_COMMAND:
                raise ValueError(f"The tests of {unit} are too long for one command: isolate some in the plan")
        # Keep the replay hashes from passing Eternal games and matches in CI.
        if module in {"eternal_catalog_v1", "eternal_bo3_v1"}:
            command.append("--show-output")
        if executed_path:
            returncode, reported = run_recorded(command)
            # A passing process must have reported each of its tests (more
            # than once is a replayed line) and no test of another unit.
            missing = sorted(set(selected_tests) - set(reported))
            unexpected = sorted(set(reported) - set(selected_tests))
            if not returncode and (missing or unexpected):
                print(f"Integration tests reported by {unit} differ from its selection: "
                      f"missing {missing}, unexpected {unexpected}", flush=True)
                returncode = 1
            executed |= set(reported) & set(selected_tests)
        else:
            returncode = subprocess.run(command, check=False).returncode
        failures += bool(returncode)
    if executed_path:
        record = {"shard": selection["shard"] if selection is not None else None,
                  "attempt": int(os.environ.get("GITHUB_RUN_ATTEMPT") or 0),
                  "listed": sorted(tests), "executed": sorted(executed), "failed": failures}
        Path(executed_path).write_text(json.dumps(record, indent=1) + "\n", encoding="utf-8")
    if selection is None:
        print(f"Integration inventory: {len(tests)} tests, {len(modules)} modules, {failures} failed modules",
              flush=True)
    else:
        shard, count = selection["shard"]
        print(f"Integration inventory: shard {shard}/{count} ran {len(expected)} of {len(tests)} tests in "
              f"{len(units)} units of {len(compiled)} modules, {failures} failed modules", flush=True)
    return int(bool(failures))


def main() -> int:
    if sys.argv[1:2] == ["--run"]:
        if len(sys.argv) != 3:
            raise ValueError("Integration runner expects exactly one executable")
        return run_suite(sys.argv[2], selection=json.loads(os.environ.get(SHARD_ENV, "null")),
                         list_path=os.environ.get(LIST_ENV), executed_path=os.environ.get(EXECUTED_ENV))
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("suite", choices=SUITES)
    parser.add_argument("--features", required=True)
    parser.add_argument("--shard-index", type=int,
                        help="Run only this shard of the suite's modules (with --shard-count)")
    parser.add_argument("--shard-count", type=int)
    parser.add_argument("--no-run", action="store_true", help="Build the suite and run nothing")
    parser.add_argument("--list-only", type=Path, metavar="FILE",
                        help="Build the suite, write the names of its tests to FILE and run none")
    parser.add_argument("--executed-list", type=Path, metavar="FILE",
                        help="Write the names of the tests this run executed to FILE")
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[2]
    modules = suite_modules(root, args.suite)
    environment = os.environ.copy()
    for name in (SHARD_ENV, LIST_ENV, EXECUTED_ENV):
        environment.pop(name, None)
    if args.shard_index is not None or args.shard_count is not None:
        if (args.shard_index is None or args.shard_count is None
                or not 0 <= args.shard_index < args.shard_count):
            parser.error("Sharding requires 0 <= --shard-index < --shard-count")
        units = plan_units(modules, isolated_tests(args.suite))
        recorded, default = module_seconds(args.suite, units)
        plan = shard_plan(units, args.shard_count, recorded, default)
        report_plan(args.suite, plan, args.shard_index, recorded, default)
        environment[SHARD_ENV] = json.dumps({"inventory": modules, "plan": plan,
                                             "selected": plan[args.shard_index],
                                             "shard": [args.shard_index, args.shard_count]})
    # The runner process starts in the package directory, so the files are
    # named by their full paths.
    if args.list_only:
        environment[LIST_ENV] = str(args.list_only.resolve())
    if args.executed_list:
        environment[EXECUTED_ENV] = str(args.executed_list.resolve())
    runner = [sys.executable, str(Path(__file__).resolve()), "--run"]
    configuration = "target.'cfg(all())'.runner=" + json.dumps(runner)
    command = ["cargo", "test", "--release", "--locked", "--config", configuration,
               "-p", "mtg-kernel", "--features", args.features, "--test", args.suite]
    if args.no_run:
        command.append("--no-run")
    return subprocess.run(command, env=environment, check=False).returncode


if __name__ == "__main__":
    raise SystemExit(main())
