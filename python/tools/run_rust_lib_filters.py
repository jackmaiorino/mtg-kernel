"""Build one release library and run its existing test filters through Cargo."""

from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import subprocess
import sys


FILTERS_ENV = "MTG_CI_RUST_LIB_FILTERS"
FILTER_TIMEOUT_ENV = "MTG_CI_RUST_LIB_FILTER_TIMEOUT_SECONDS"


def valid_filters(value: object) -> list[str]:
    if not isinstance(value, list) or not value or any(
        not isinstance(item, str) or not item or item.startswith("-") for item in value
    ):
        raise ValueError("Expected one or more nonempty test filters")
    return value


def main() -> int:
    if len(sys.argv) > 1 and sys.argv[1] == "--run":
        if len(sys.argv) != 3:
            raise ValueError("Cargo runner expects exactly one library test executable")
        filters = valid_filters(json.loads(os.environ[FILTERS_ENV]))
        filter_timeout = int(os.environ.get(FILTER_TIMEOUT_ENV, "1800"))
        if filter_timeout <= 0:
            raise ValueError("Library filter timeout must be positive")
        failed_filters = []
        first_failure = 0
        for test_filter in filters:
            print(f"Running compiled library filter: {test_filter}", flush=True)
            try:
                result = subprocess.run([sys.argv[2], test_filter], check=False, timeout=filter_timeout)
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
    parser.add_argument("--features", required=True)
    parser.add_argument("--filter-timeout-seconds", type=int, default=1800)
    parser.add_argument("filters", nargs="+")
    args = parser.parse_args()
    filters = valid_filters(args.filters)
    if args.filter_timeout_seconds <= 0:
        parser.error("--filter-timeout-seconds must be positive")
    environment = os.environ.copy()
    environment[FILTERS_ENV] = json.dumps(filters)
    environment[FILTER_TIMEOUT_ENV] = str(args.filter_timeout_seconds)
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
