"""Run the workspace suite and its snapshot timing gate without contention."""

from __future__ import annotations

import json
from pathlib import Path
import subprocess
import sys

from run_rust_integration_tests import SUITES, run_suite, suite_modules


TIMING_TEST = "snapshot::tests::snapshot_clone_cost_is_bounded"
# The targets part names its benchmark targets one by one. `--benches` selects
# every target with `bench = true`, and the library has that by default, so
# with it the targets part compiled and ran the library's unit tests a second
# time (2026-11-16: all 3,841 test names of the lib part ran again there).
PARTS = {"lib": ["--lib"], "targets": ["--bins", "--examples"]}
NAMED_KINDS = {"test": "--test", "bench": "--bench"}


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


def main() -> int:
    if len(sys.argv) > 1 and sys.argv[1] == "--run":
        executable, *arguments = sys.argv[2:]
        if Path(executable).name.startswith("mtg_kernel-"):
            if arguments:
                raise ValueError("Workspace library runner expects no test arguments")
            result = subprocess.run([executable, "--skip", TIMING_TEST], check=False)
            if result.returncode:
                return result.returncode
            print(f"Running isolated timing gate: {TIMING_TEST}", flush=True)
            # Keep the short wall-clock measurement ahead of background
            # Windows services. Correctness tests retain normal priority.
            timing_options = ({"creationflags": subprocess.HIGH_PRIORITY_CLASS}
                              if sys.platform == "win32" else {})
            return subprocess.run([
                executable, TIMING_TEST, "--exact", "--include-ignored", "--test-threads=1", "--nocapture",
            ], check=False, **timing_options).returncode
        if any(Path(executable).name.startswith(suite + "-") for suite in SUITES):
            if arguments:
                raise ValueError("Grouped workspace runner expects no test arguments")
            return run_suite(executable, allow_empty=True)
        return subprocess.run([executable, *arguments], check=False).returncode

    arguments = sys.argv[1:]
    # CI may run the library tests and every other target as two parallel
    # jobs. The parts test disjoint targets and together equal --all-targets.
    targets = ["--all-targets"]
    if arguments[:1] == ["--part"] and len(arguments) >= 2:
        targets = PARTS.get(arguments[1], [])
        if arguments[1] == "targets":
            targets = [*targets, *default_named_targets()]
        arguments = arguments[2:]
    if not targets or arguments not in ([], ["--no-fail-fast"]):
        raise ValueError("Workspace suite runner accepts [--part lib|targets] [--no-fail-fast]")
    for suite in SUITES:
        suite_modules(Path(__file__).resolve().parents[2], suite)
    runner = [sys.executable, str(Path(__file__).resolve()), "--run"]
    configuration = "target.'cfg(all())'.runner=" + json.dumps(runner)
    return subprocess.run([
        "cargo", "test", "--release", "--locked", "--config", configuration,
        "--workspace", *targets, *arguments,
    ], check=False).returncode


if __name__ == "__main__":
    raise SystemExit(main())
