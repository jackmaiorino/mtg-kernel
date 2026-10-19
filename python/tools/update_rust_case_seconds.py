"""Refresh rust_case_seconds.json from the hosted Linux shards of CI runs.

Usage: python python/tools/update_rust_case_seconds.py RUN_ID [RUN_ID ...]

Reads each run's isolated-case records through the GitHub CLI. Cases seen in
the given runs replace their old times; other recorded cases are kept.
"""

from __future__ import annotations

import json
from pathlib import Path
import re
import statistics
import subprocess
import sys

from run_rust_lib_filters import CASE_SECONDS_PATH

FINISHED = re.compile(r'\{"event": "rust_test_case_finished".*\}')


def gh(*arguments: str) -> str:
    return subprocess.run(["gh", *arguments], check=True, capture_output=True,
                          text=True, encoding="utf-8", errors="replace").stdout


def main() -> int:
    runs = sys.argv[1:]
    if not runs:
        raise SystemExit(__doc__)
    seen: dict[str, list[float]] = {}
    for run in runs:
        jobs = json.loads(gh("run", "view", run, "--json", "jobs"))["jobs"]
        for job in jobs:
            if not job["name"].startswith("Rust tests (ubuntu-24.04, limited"):
                continue
            log = gh("api", f"repos/{{owner}}/{{repo}}/actions/jobs/{job['databaseId']}/logs")
            for match in FINISHED.finditer(log):
                row = json.loads(match.group(0))
                if row.get("status") == "passed":
                    seen.setdefault(row["case"], []).append(row["elapsed_seconds"])
    recorded = json.loads(CASE_SECONDS_PATH.read_text()) if CASE_SECONDS_PATH.is_file() else {}
    recorded.update({case: max(1, round(statistics.mean(times))) for case, times in seen.items()})
    CASE_SECONDS_PATH.write_text(json.dumps(dict(sorted(recorded.items())), indent=1) + "\n",
                                 newline="\n")
    print(f"Updated {len(seen)} cases; table has {len(recorded)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
