"""Run one release-trainer row: build the mtg-kernel library tests with a feature set, check the compiled
inventory against tools/ci/rust_test_cfg.py's prediction, and run that row's tests.

The default row runs every library test the trainer filters select (python/tools/run_rust_lib_filters.py, with
its per-case limits). Any other row runs exactly the selected tests its build compiles and the default build does
not, so tests that only hold without the feature (for example fixtures that a catalog feature rejects) never run
under it. A difference between the compiled inventory and the prediction fails the row before any test runs.

Usage: run_trainer_row.py --features FEATURES   ("" for default features; the target is this host's OS)
"""
from __future__ import annotations

import argparse
import json
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


def compiled(executable: str) -> set[str]:
    listing = subprocess.run([executable, "--list", "--format", "terse"], cwd=ROOT, stdout=subprocess.PIPE,
                             text=True, check=True).stdout
    names = {line.removesuffix(": test") for line in listing.splitlines() if line.endswith(": test")}
    return {name for name in names if rust_test_cfg.selected(name)}


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--features", required=True)
    features = parser.parse_args().features
    target = "windows" if sys.platform == "win32" else "linux"
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
    names = rust_test_cfg.feature_only(features, target)
    print(json.dumps({"feature_only": names, "count": len(names)}), flush=True)
    if not names:
        return 0
    return subprocess.run([executable, "--exact", *names], cwd=ROOT, check=False).returncode


if __name__ == "__main__":
    sys.exit(main())
