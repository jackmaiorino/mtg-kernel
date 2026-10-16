"""Build one release library and run its existing test filters through Cargo."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys


FILTERS_ENV = "MTG_CI_RUST_LIB_FILTERS"
FILTER_TIMEOUT_ENV = "MTG_CI_RUST_LIB_FILTER_TIMEOUT_SECONDS"
SHARD_ENV = "MTG_CI_RUST_LIB_SHARD"
NOCAPTURE_ENV = "MTG_CI_RUST_LIB_NOCAPTURE"
SKIP_IGNORED_ENV = "MTG_CI_RUST_LIB_SKIP_IGNORED"


def valid_filters(value: object) -> list[str]:
    if not isinstance(value, list) or not value or any(
        not isinstance(item, str) or not item or item.startswith("-") for item in value
    ):
        raise ValueError("Expected one or more nonempty test filters")
    return value


def compiled_names(executable: str, test_filter: str, timeout: int, *, ignored: bool) -> list[str]:
    # Pinned libtest's pretty listing includes a final count; terse does not.
    command = [executable, test_filter, "--list", "--format", "pretty", "--color", "never"]
    if ignored:
        command.append("--ignored")
    result = subprocess.run(command, check=False, capture_output=True, text=True, timeout=timeout)
    if result.returncode:
        print(result.stdout, end="", flush=True)
        print(result.stderr, end="", file=sys.stderr, flush=True)
        result.check_returncode()
    names = []
    summary_count = None
    for line in result.stdout.splitlines():
        if not line.strip():
            continue
        if summary_count is not None:
            raise ValueError("Compiled test inventory has records after its summary")
        summary = re.fullmatch(r"(\d+) tests?, (\d+) benchmarks?", line)
        if summary:
            summary_count = int(summary[1])
            if int(summary[2]) != 0:
                raise ValueError("Sharded library inventory contains benchmarks")
            continue
        if not line.endswith(": test"):
            raise ValueError(f"Unexpected compiled test inventory line: {line!r}")
        name = line.removesuffix(": test")
        if not name or test_filter not in name or name in names:
            raise ValueError(f"Invalid or duplicate compiled test name: {name!r}")
        names.append(name)
    if summary_count is None or summary_count != len(names):
        raise ValueError("Compiled test inventory count is missing or inconsistent")
    return sorted(names)


def main() -> int:
    if len(sys.argv) > 1 and sys.argv[1] == "--run":
        if len(sys.argv) != 3:
            raise ValueError("Cargo runner expects exactly one library test executable")
        filters = valid_filters(json.loads(os.environ[FILTERS_ENV]))
        filter_timeout = int(os.environ.get(FILTER_TIMEOUT_ENV, "1800"))
        if filter_timeout <= 0:
            raise ValueError("Library filter timeout must be positive")
        nocapture = os.environ.get(NOCAPTURE_ENV, "0") == "1"
        skip_ignored = os.environ.get(SKIP_IGNORED_ENV, "0") == "1"
        shard = json.loads(os.environ.get(SHARD_ENV, "null"))
        commands = [(test_filter, [sys.argv[2], test_filter]) for test_filter in filters]
        if shard is not None:
            if (
                not isinstance(shard, list) or len(shard) != 2
                or any(type(value) is not int for value in shard)
                or not 0 <= shard[0] < shard[1] or len(filters) != 1
            ):
                raise ValueError("Expected one filter and a valid shard index/count")
            names = compiled_names(sys.argv[2], filters[0], filter_timeout, ignored=False)
            ignored = compiled_names(sys.argv[2], filters[0], filter_timeout, ignored=True)
            if not names or not set(ignored) <= set(names) or (ignored and not skip_ignored):
                raise ValueError("Sharded inventory is empty or contains unexpected ignored tests")
            # Preserve existing #[ignore] behavior only when explicitly requested.
            # Log every skipped compiled name; never enable its unregistered fixture.
            runnable = [name for name in names if name not in ignored]
            selected = runnable[shard[0]::shard[1]]
            if not selected:
                raise ValueError("Compiled test shard is empty")
            encoded = json.dumps(names, separators=(",", ":")).encode("utf-8")
            print(json.dumps({
                "filter": filters[0], "inventory": names, "count": len(names),
                "inventory_sha256": hashlib.sha256(encoded).hexdigest(),
                "ignored": ignored, "runnable_count": len(runnable),
                "shard_index": shard[0], "shard_count": shard[1], "selected": selected,
            }), flush=True)
            commands = [(f"{filters[0]} shard {shard[0]}/{shard[1]}",
                         [sys.argv[2], "--exact", *selected])]
        failed_filters = []
        first_failure = 0
        for test_filter, command in commands:
            if nocapture:
                command.append("--nocapture")
            print(f"Running compiled library filter: {test_filter}", flush=True)
            try:
                result = subprocess.run(command, check=False, timeout=filter_timeout)
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
    parser.add_argument("--shard-index", type=int)
    parser.add_argument("--shard-count", type=int)
    parser.add_argument("--nocapture", action="store_true")
    parser.add_argument("--skip-ignored", action="store_true")
    parser.add_argument("filters", nargs="+")
    args = parser.parse_args()
    filters = valid_filters(args.filters)
    if args.filter_timeout_seconds <= 0:
        parser.error("--filter-timeout-seconds must be positive")
    shard = None
    if args.shard_index is not None or args.shard_count is not None:
        if (args.shard_index is None or args.shard_count is None
                or not 0 <= args.shard_index < args.shard_count or len(filters) != 1):
            parser.error("Sharding requires one filter and 0 <= --shard-index < --shard-count")
        shard = [args.shard_index, args.shard_count]
    if args.skip_ignored and shard is None:
        parser.error("--skip-ignored requires sharding")
    environment = os.environ.copy()
    environment[FILTERS_ENV] = json.dumps(filters)
    environment[FILTER_TIMEOUT_ENV] = str(args.filter_timeout_seconds)
    environment[SHARD_ENV] = json.dumps(shard)
    environment[NOCAPTURE_ENV] = "1" if args.nocapture else "0"
    environment[SKIP_IGNORED_ENV] = "1" if args.skip_ignored else "0"
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
