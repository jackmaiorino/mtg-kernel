"""Refresh rust_case_seconds.json from the hosted Linux shards of CI runs.

Usage: python python/tools/update_rust_case_seconds.py RUN_ID [RUN_ID ...]
       python python/tools/update_rust_case_seconds.py --logs LOG [LOG ...]

The first form reads each run's isolated-case records through the GitHub CLI.
The second reads job logs saved from the Actions page (plain text, or gzipped
with a .gz name), for a machine without the GitHub CLI; give it the Linux
Limited shards' logs. Cases seen in the given runs or logs replace their old
times; other recorded cases are kept.
"""

from __future__ import annotations

import gzip
import json
from pathlib import Path
import re
import statistics
import subprocess
import sys
from typing import Iterable

from run_rust_lib_filters import CASE_SECONDS_PATH

FINISHED = re.compile(r'\{"event": "rust_test_case_finished".*\}')


def gh(*arguments: str) -> str:
    return subprocess.run(["gh", *arguments], check=True, capture_output=True,
                          text=True, encoding="utf-8", errors="replace").stdout


def run_logs(runs: list[str]) -> Iterable[str]:
    for run in runs:
        jobs = json.loads(gh("run", "view", run, "--json", "jobs"))["jobs"]
        for job in jobs:
            if job["name"].startswith("Rust tests (ubuntu-24.04, limited"):
                yield gh("api", f"repos/{{owner}}/{{repo}}/actions/jobs/{job['databaseId']}/logs")


def file_logs(paths: list[str]) -> Iterable[str]:
    for path in map(Path, paths):
        opener = gzip.open if path.suffix == ".gz" else open
        with opener(path, "rt", encoding="utf-8", errors="replace") as handle:
            yield handle.read()


def case_times(logs: Iterable[str]) -> dict[str, list[float]]:
    """Seconds of every passed isolated case, by name, from the logs' finished records."""
    seen: dict[str, list[float]] = {}
    for log in logs:
        for match in FINISHED.finditer(log):
            row = json.loads(match.group(0))
            if row.get("status") == "passed":
                seen.setdefault(row["case"], []).append(row["elapsed_seconds"])
    return seen


def update_table(seen: dict[str, list[float]], path: Path | None = None) -> dict[str, int]:
    path = path or CASE_SECONDS_PATH
    recorded = json.loads(path.read_text()) if path.is_file() else {}
    recorded.update({case: max(1, round(statistics.mean(times))) for case, times in seen.items()})
    path.write_text(json.dumps(dict(sorted(recorded.items())), indent=1) + "\n", newline="\n")
    return recorded


def main() -> int:
    arguments = sys.argv[1:]
    if not arguments or arguments == ["--logs"]:
        raise SystemExit(__doc__)
    logs = file_logs(arguments[1:]) if arguments[0] == "--logs" else run_logs(arguments)
    seen = case_times(logs)
    if not seen:
        raise SystemExit("No passed isolated-case records found")
    recorded = update_table(seen)
    print(f"Updated {len(seen)} cases; table has {len(recorded)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
