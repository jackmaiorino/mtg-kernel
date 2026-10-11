"""Compare compiled integration-test inventories against a pre-grouping checkout."""

from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import subprocess

from run_rust_integration_tests import SUITES, listed_tests, suite_modules


def inventory(root: Path, targets: list[str], features: str, target_dir: Path) -> set[tuple[str, str]]:
    command = ["cargo", "test", "--locked", "--no-run", "--message-format=json",
               "-p", "mtg-kernel", "--features", features]
    for target in targets:
        command += ["--test", target]
    environment = os.environ.copy()
    environment["CARGO_TARGET_DIR"] = str(target_dir)
    result = subprocess.run(command, cwd=root, env=environment, check=False, capture_output=True, text=True)
    if result.returncode:
        print(result.stderr, flush=True)
        result.check_returncode()
    found: dict[str, str] = {}
    for line in result.stdout.splitlines():
        message = json.loads(line)
        if message.get("reason") == "compiler-artifact" and message.get("executable"):
            target = message["target"]["name"]
            if target in targets and "test" in message["target"]["kind"]:
                found[target] = message["executable"]
    if set(found) != set(targets):
        raise ValueError(f"Missing compiled targets: {set(targets) - set(found)}")
    entries = set()
    for target, executable in found.items():
        for test in listed_tests(executable):
            if target in SUITES:
                module, separator, name = test.partition("::")
                if not separator:
                    raise ValueError(f"Test outside an original module: {test}")
                entries.add((module, name))
            else:
                entries.add((target, test))
    return entries


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--baseline", type=Path, required=True)
    parser.add_argument("--target-root", type=Path, required=True)
    parser.add_argument("suite", choices=SUITES)
    parser.add_argument("--features", required=True)
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[2]
    modules = suite_modules(root, args.suite)
    before = inventory(args.baseline.resolve(), modules, args.features,
                       args.target_root.resolve() / "baseline")
    after = inventory(root, [args.suite], args.features,
                      args.target_root.resolve() / "grouped")
    if before != after or not before:
        raise ValueError(f"Inventory mismatch: missing={sorted(before - after)}, added={sorted(after - before)}")
    print(f"Compiled inventory parity: {len(before)} tests in {len(modules)} original files", flush=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
