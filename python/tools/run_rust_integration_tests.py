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


def module_seconds(suite: str, path: Path = MODULE_SECONDS_PATH) -> tuple[dict[str, float], float]:
    table = json.loads(path.read_text(encoding="utf-8")) if path.is_file() else {}
    recorded = table.get(suite, {})
    # A module without a recorded time gets the mean, so a new module is
    # never assumed to be cheap.
    return recorded, (sum(recorded.values()) / len(recorded) if recorded else 1.0)


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


def run_suite(executable: str, *, allow_empty: bool = False, selection: dict | None = None) -> int:
    tests = listed_tests(executable)
    if not tests and not allow_empty:
        raise ValueError(f"Consolidated suite has no tests: {executable}")
    if any("::" not in test for test in tests):
        raise ValueError("Consolidated tests must belong to original source modules")
    modules = sorted({test.split("::", 1)[0] for test in tests})
    compiled = modules
    if selection is not None:
        # A compiled module outside the planned inventory would belong to no
        # shard; fail rather than skip it.
        outside = sorted(set(modules) - set(selection["inventory"]))
        if outside:
            raise ValueError(f"Compiled integration modules in no shard: {outside}")
        selected = set(selection["selected"])
        modules = [module for module in modules if module in selected]
        for module in sorted(selected - set(compiled)):
            print(f"Integration module without compiled tests: {module}", flush=True)
    failures = 0
    for module in modules:
        # Each original file gets a fresh process, with its existing internal
        # libtest concurrency and ignored-test behavior. The trailing ::
        # prevents similarly named modules from overlapping.
        selected_tests = [test for test in tests if test.startswith(module + "::")]
        print(f"Running integration module: {module} ({len(selected_tests)} listed tests)", flush=True)
        result = subprocess.run([executable, module + "::"], check=False)
        failures += bool(result.returncode)
    if selection is None:
        print(f"Integration inventory: {len(tests)} tests, {len(modules)} modules, {failures} failed modules",
              flush=True)
    else:
        shard, count = selection["shard"]
        chosen = set(modules)
        run = sum(test.split("::", 1)[0] in chosen for test in tests)
        print(f"Integration inventory: shard {shard}/{count} ran {run} of {len(tests)} tests in "
              f"{len(modules)} of {len(compiled)} modules, {failures} failed modules", flush=True)
    return int(bool(failures))


def main() -> int:
    if sys.argv[1:2] == ["--run"]:
        if len(sys.argv) != 3:
            raise ValueError("Integration runner expects exactly one executable")
        return run_suite(sys.argv[2], selection=json.loads(os.environ.get(SHARD_ENV, "null")))
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("suite", choices=SUITES)
    parser.add_argument("--features", required=True)
    parser.add_argument("--shard-index", type=int,
                        help="Run only this shard of the suite's modules (with --shard-count)")
    parser.add_argument("--shard-count", type=int)
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[2]
    modules = suite_modules(root, args.suite)
    environment = os.environ.copy()
    environment.pop(SHARD_ENV, None)
    if args.shard_index is not None or args.shard_count is not None:
        if (args.shard_index is None or args.shard_count is None
                or not 0 <= args.shard_index < args.shard_count):
            parser.error("Sharding requires 0 <= --shard-index < --shard-count")
        recorded, default = module_seconds(args.suite)
        plan = shard_plan(modules, args.shard_count, recorded, default)
        report_plan(args.suite, plan, args.shard_index, recorded, default)
        environment[SHARD_ENV] = json.dumps({"inventory": modules, "selected": plan[args.shard_index],
                                             "shard": [args.shard_index, args.shard_count]})
    runner = [sys.executable, str(Path(__file__).resolve()), "--run"]
    configuration = "target.'cfg(all())'.runner=" + json.dumps(runner)
    return subprocess.run([
        "cargo", "test", "--release", "--locked", "--config", configuration,
        "-p", "mtg-kernel", "--features", args.features, "--test", args.suite,
    ], env=environment, check=False).returncode


if __name__ == "__main__":
    raise SystemExit(main())
