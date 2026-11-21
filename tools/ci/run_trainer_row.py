"""Run one release-trainer row: build the mtg-kernel library tests with a feature set, check the compiled
inventory against tools/ci/rust_test_cfg.py's prediction, and run that row's tests.

The default row runs every library test the trainer filters select (python/tools/run_rust_lib_filters.py, with
its per-case limits). Any other row runs exactly the selected tests its build compiles and the default build does
not, plus the selected tests the full pull request set's own commands with the same feature set run (so the row is
never weaker than the full set); tests that only hold without the feature never run under it; each of those runs as its own process with a time limit. A difference between the compiled inventory
and the prediction fails the row before any test runs. The Linux default row first fails if a selected test that
some Linux build could compile is compiled by no row (rust_test_cfg.py check-coverage).

Usage: run_trainer_row.py --features FEATURES   ("" for default features; the target is this host's OS)
"""
from __future__ import annotations

import argparse
import importlib.util
import json
import platform
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(Path(__file__).resolve().parent))
import ci_scope  # noqa: E402
import rust_test_cfg  # noqa: E402


def build(features: str) -> str:
    command = ["cargo", "test", "--release", "--locked", "-p", "mtg-kernel", "--lib", "--no-run",
               "--message-format", "json-render-diagnostics"]
    if features:
        command += ["--features", features]
    print("+ " + " ".join(command), flush=True)
    result = subprocess.run(command, cwd=ROOT, stdout=subprocess.PIPE, text=True, check=True)
    executables = [record["executable"] for record in map(json.loads, result.stdout.splitlines())
                   if record.get("reason") == "compiler-artifact" and record.get("profile", {}).get("test")
                   and set(record.get("target", {}).get("kind", [])) & {"lib", "rlib"} and record.get("executable")]
    if len(executables) != 1:
        raise SystemExit(f"expected one mtg-kernel library test executable, found {executables}")
    return executables[0]


def compiled(executable: str, ignored: bool = False) -> set[str]:
    """Selected names in the compiled inventory (with ignored=True, only the #[ignore] ones)."""
    listing = subprocess.run([executable, "--list", *(["--ignored"] if ignored else []), "--format", "terse"],
                             cwd=ROOT, stdout=subprocess.PIPE, text=True, check=True).stdout
    names = {line.removesuffix(": test") for line in listing.splitlines() if line.endswith(": test")}
    return {name for name in names if rust_test_cfg.selected(name)}


# Seconds a feature row's case may run before it is stopped and reported as timed out (the filter runner's
# default limit).
CASE_TIMEOUT_SECONDS = 1800


def filter_runner():
    spec = importlib.util.spec_from_file_location("run_rust_lib_filters", ROOT / "python/tools/run_rust_lib_filters.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--features", required=True)
    features = parser.parse_args().features
    target = "windows" if sys.platform == "win32" else "linux"
    if platform.machine().lower() not in ("x86_64", "amd64") or not sys.platform.startswith(("win32", "linux")):
        print(f"unsupported host {sys.platform}/{platform.machine()}: the predictor models x86-64 Linux and Windows")
        return 2
    if not features and target == "linux" and rust_test_cfg.check_coverage():
        return 1
    executable = build(features)
    actual, predicted = compiled(executable), set(rust_test_cfg.predict(features, target))
    if actual != predicted:
        print(json.dumps({"compiled_not_predicted": sorted(actual - predicted),
                          "predicted_not_compiled": sorted(predicted - actual)}, indent=1))
        print("The compiled inventory differs from tools/ci/rust_test_cfg.py's prediction; fix the predictor.")
        return 1
    print(f"inventory matches the prediction: {len(actual)} selected tests ({target}, features '{features}')",
          flush=True)
    if not features:
        command = [sys.executable, "python/tools/run_rust_lib_filters.py", "--features", "", *ci_scope.TRAINER_FILTERS]
        return subprocess.run(command, cwd=ROOT, check=False).returncode
    added = rust_test_cfg.row_tests(features, target)
    # #[ignore] cases keep their explicit-invocation rule: listed as ignored, never run or reported as passed.
    ignored = sorted(compiled(executable, ignored=True) & set(added))
    runnable = [name for name in added if name not in ignored]
    print(json.dumps({"row_tests": added, "count": len(added), "ignored": ignored,
                      "runnable_count": len(runnable)}), flush=True)
    if not runnable:
        return 0
    # One process per case, a few at a time, each stopped and reported at the limit (no long command lines).
    return filter_runner().run_isolated_cases(executable, runnable, CASE_TIMEOUT_SECONDS, ignored)


if __name__ == "__main__":
    sys.exit(main())
