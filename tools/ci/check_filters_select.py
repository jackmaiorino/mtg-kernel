"""Fail if a library test filter selects no compiled test, so a filter can never claim coverage of an absent module.

usage: python tools/ci/check_filters_select.py --features FEATURES FILTER...
Builds (or reuses) the mtg-kernel release library test binary with the features and counts, per filter, the
compiled test names containing it (libtest's substring match).
"""
from __future__ import annotations

import argparse
import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--features", required=True)
    parser.add_argument("filters", nargs="+")
    args = parser.parse_args()
    command = ["cargo", "test", "--release", "--locked", "-p", "mtg-kernel", "--lib", "--no-run",
               "--message-format", "json-render-diagnostics"] + (["--features", args.features] if args.features else [])
    out = subprocess.run(command, cwd=ROOT, stdout=subprocess.PIPE, text=True, check=True).stdout
    executables = [r["executable"] for r in map(json.loads, out.splitlines())
                   if r.get("reason") == "compiler-artifact" and r.get("profile", {}).get("test")
                   and set(r.get("target", {}).get("kind", [])) & {"lib", "rlib"} and r.get("executable")]
    if len(executables) != 1:
        raise SystemExit(f"expected one mtg-kernel library test executable, found {executables}")
    listing = subprocess.run([executables[0], "--list", "--format", "terse"], cwd=ROOT, stdout=subprocess.PIPE,
                             text=True, check=True).stdout
    names = [line.removesuffix(": test") for line in listing.splitlines() if line.endswith(": test")]
    empty = [f for f in args.filters if not any(f in name for name in names)]
    for f in args.filters:
        print(f"{f}: {sum(f in name for name in names)} tests")
    if empty:
        print("filters that select no compiled test (features '" + args.features + "'): " + ", ".join(empty))
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
