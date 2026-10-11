"""Run consolidated integration suites with the original per-file processes."""

from __future__ import annotations

import argparse
import json
from pathlib import Path
import re
import subprocess
import sys


SUITES = {"fdn_integration": "fdn", "standard_integration": "standard"}
# Main retains these future-card fixtures but does not execute them in the
# admitted FDN CI group: Skyship Buccaneer/Goblin Boarders are unregistered.
# Keep their standalone target and existing cfg/coverage unchanged.
STANDALONE = {"fdn_raid_entries_v1"}


def suite_modules(root: Path, suite: str) -> list[str]:
    directory = root / "mtg-kernel" / "tests"
    source = (directory / f"{suite}.rs").read_text()
    declared = re.findall(r"^mod (\w+);$", source, re.MULTILINE)
    expected = sorted(path.stem for path in (directory / SUITES[suite]).glob("*.rs"))
    ungrouped = [path.name for path in directory.glob(SUITES[suite] + "_*.rs")
                 if path.stem != suite and path.stem not in STANDALONE]
    if ungrouped:
        raise ValueError(f"{suite}: ungrouped integration sources: {ungrouped}")
    if not expected or sorted(declared) != expected:
        raise ValueError(f"{suite}: module declarations do not match source inventory")
    return sorted(declared)


def listed_tests(executable: str) -> list[str]:
    result = subprocess.run(
        [executable, "--list", "--format=terse"], check=True, capture_output=True, text=True,
    )
    return [line.removesuffix(": test") for line in result.stdout.splitlines()
            if line.endswith(": test")]


def run_suite(executable: str, *, allow_empty: bool = False) -> int:
    tests = listed_tests(executable)
    if not tests and not allow_empty:
        raise ValueError(f"Consolidated suite has no tests: {executable}")
    if any("::" not in test for test in tests):
        raise ValueError("Consolidated tests must belong to original source modules")
    modules = sorted({test.split("::", 1)[0] for test in tests})
    failures = 0
    for module in modules:
        # Each original file gets a fresh process, with its existing internal
        # libtest concurrency and ignored-test behavior. The trailing ::
        # prevents similarly named modules from overlapping.
        selected = [test for test in tests if test.startswith(module + "::")]
        print(f"Running integration module: {module} ({len(selected)} listed tests)", flush=True)
        result = subprocess.run([executable, module + "::"], check=False)
        failures += bool(result.returncode)
    print(f"Integration inventory: {len(tests)} tests, {len(modules)} modules, {failures} failed modules",
          flush=True)
    return int(bool(failures))


def main() -> int:
    if sys.argv[1:2] == ["--run"]:
        if len(sys.argv) != 3:
            raise ValueError("Integration runner expects exactly one executable")
        return run_suite(sys.argv[2])
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("suite", choices=SUITES)
    parser.add_argument("--features", required=True)
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[2]
    suite_modules(root, args.suite)
    runner = [sys.executable, str(Path(__file__).resolve()), "--run"]
    configuration = "target.'cfg(all())'.runner=" + json.dumps(runner)
    return subprocess.run([
        "cargo", "test", "--release", "--locked", "--config", configuration,
        "-p", "mtg-kernel", "--features", args.features, "--test", args.suite,
    ], check=False).returncode


if __name__ == "__main__":
    raise SystemExit(main())
