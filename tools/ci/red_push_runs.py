"""List red post-merge CI runs since a time, one handoff log line each, for the sessions' watch loops.

usage: python tools/ci/red_push_runs.py --since 2026-11-21T07:00:00Z (runs that finished since then) [--repo jackmaiorino/mtg-kernel]
       [--branches main,release/mull-c5719e1cf] [--append HANDOFF_LOG]
Uses the gh CLI (already authenticated where the watch loops run). A run counts when it is a push or dispatch
run on one of the branches (a name ending in / is a prefix) that did not succeed and whose post-merge-report job
ran (the workflow's own monitor predicate: a push run or a full dispatch with a failed or cancelled job). Each line is
the post-merge report's line: every merge the run covers, oldest first, with its pull request, and the first
responder. With --append, also appends each line the file does not already contain (the handoff log). Prints
nothing when there is none; exit 0 unless gh fails.
"""
from __future__ import annotations

import argparse
import json
import subprocess
import sys
from datetime import datetime, timedelta
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import post_merge_report  # noqa: E402


def gh_get(path: str):
    out = subprocess.run(["gh", "api", path.lstrip("/")], capture_output=True, text=True, encoding="utf-8",
                         check=True).stdout
    return json.loads(out or "null")


def runs_since(repo: str, since: str, lookback_days: int) -> list[dict]:
    """Completed runs that finished at or after since. GitHub lists runs by creation only, so runs created up to
    lookback_days before since are examined (reruns of older runs are outside that window; the default covers
    any rerun GitHub still allows, 30 days)."""
    start = datetime.fromisoformat(since.replace("Z", "+00:00"))
    created = (start - timedelta(days=lookback_days)).strftime("%Y-%m-%dT%H:%M:%SZ")
    out = subprocess.run(["gh", "api", "--paginate",
                          f"repos/{repo}/actions/workflows/ci.yml/runs?status=completed&created=%3E%3D{created}"
                          "&per_page=100", "--jq", ".workflow_runs[] | select(.event == \"push\" or .event == "
                          "\"workflow_dispatch\") | tojson"], capture_output=True, text=True, encoding="utf-8",
                         check=True).stdout
    runs = [json.loads(line) for line in out.splitlines() if line.strip()]
    return [r for r in runs if datetime.fromisoformat(r["updated_at"].replace("Z", "+00:00")) >= start]


def red_runs(runs: list[dict], branches: list[str], get=None, repo: str = "") -> list[dict]:
    """Completed push or dispatch runs on the branches that did not succeed and whose post-merge-report job
    executed, the same predicate as the workflow's monitor (with get=None only the cheap filters apply)."""
    candidates = [r for r in runs if r.get("conclusion") in ("failure", "cancelled", "timed_out")
                  and r.get("event") in ("push", "workflow_dispatch")
                  and (r.get("head_branch") in branches
                       or any(b.endswith("/") and (r.get("head_branch") or "").startswith(b) for b in branches))
                  and r.get("path", ".github/workflows/ci.yml").endswith("ci.yml")]
    return [r for r in candidates if get is None or post_merge_report.is_monitor(get, repo, r)]


def append_new(path: str, lines: list[str]) -> list[str]:
    """Append the lines the file does not already contain; return them."""
    try:
        with open(path, encoding="utf-8") as existing:
            present = set(existing.read().splitlines())
    except FileNotFoundError:
        present = set()
    new = [text for text in lines if text not in present]
    if new:
        with open(path, "a", encoding="utf-8", newline="\n") as out:
            out.write("".join(text + "\n" for text in new))
    return new


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--since", required=True)
    parser.add_argument("--repo", default="jackmaiorino/mtg-kernel")
    parser.add_argument("--branches", default="main,release/")
    parser.add_argument("--append", help="handoff log file to append new lines to")
    parser.add_argument("--lookback-days", type=int, default=30,
                        help="examine runs created this many days before --since (default 30, GitHub's rerun limit)")
    args = parser.parse_args(argv)
    branches = [b.strip() for b in args.branches.split(",") if b.strip()]
    lines = [post_merge_report.report(gh_get, args.repo, run)[1]
             for run in red_runs(runs_since(args.repo, args.since, args.lookback_days), branches, gh_get, args.repo)]
    for text in lines:
        print(text)
    if args.append:
        append_new(args.append, lines)
    return 0


if __name__ == "__main__":
    sys.exit(main())
