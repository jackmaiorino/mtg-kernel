"""List red post-merge CI runs since a time, one handoff log line each, for the sessions' watch loops.

usage: python tools/ci/red_push_runs.py --since 2026-11-21T07:00:00Z [--repo mtg-kernel/mtg-kernel]
       [--branches main,release/mull-c5719e1cf] [--append HANDOFF_LOG]
Uses the gh CLI (already authenticated where the watch loops run). A run counts when it is a push or dispatch
run on one of the branches (a name ending in / is a prefix) that did not succeed and whose post-merge-report job
ran (the workflow's own monitor predicate: a push run or a full dispatch with a failed or cancelled job). Each line is
the post-merge report's line: every merge the run covers, oldest first, with its pull request, and the first
responder when known. Recent red attempts without a confirmed reporter are printed separately. With --append,
only confirmed monitor lines are appended to the handoff log. Exits nonzero if GitHub cannot be queried.
"""
from __future__ import annotations

import argparse
import json
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import post_merge_report  # noqa: E402


def gh_get(path: str):
    out = subprocess.run(["gh", "api", path.lstrip("/")], capture_output=True, text=True, encoding="utf-8",
                         check=True).stdout
    return json.loads(out or "null")


# GitHub limits queries with workflow filters to this many results.
RESULT_CAP = 1000
MONITOR_EVENTS = ("push", "workflow_dispatch")
RED_CONCLUSIONS = ("failure", "cancelled", "timed_out")


def timestamp(value: str) -> datetime:
    try:
        parsed = datetime.fromisoformat(value.replace("Z", "+00:00"))
    except ValueError as error:
        raise argparse.ArgumentTypeError(f"invalid ISO 8601 timestamp: {value}") from error
    return parsed if parsed.tzinfo else parsed.replace(tzinfo=timezone.utc)


def listed_runs(repo: str) -> list[dict]:
    """List retained monitor events without a creation-time cutoff."""
    base = f"/repos/{repo}/actions/workflows/ci.yml/runs?per_page=100"
    found = {}
    for event in MONITOR_EVENTS:
        query, page = f"{base}&event={event}", 1
        while True:
            result = gh_get(f"{query}&page={page}")
            if query != base and result["total_count"] >= RESULT_CAP:
                query, page = base, 1
                found.clear()
                continue
            runs = result["workflow_runs"]
            found.update({run["id"]: run for run in runs})
            if len(runs) < 100:
                break
            page += 1
        if query == base:
            break
    return sorted(found.values(), key=lambda row: (row["created_at"], row["id"]), reverse=True)


def runs_since(repo: str, since: datetime, branches: list[str]):
    """Yield recent failed attempts, including those superseded by an already visible rerun."""
    for run in listed_runs(repo):
        branch = run.get("head_branch") or ""
        if (run["event"] not in MONITOR_EVENTS
                or not any(branch == name or (name.endswith("/") and branch.startswith(name)) for name in branches)
                or run["path"].split("@", 1)[0] != ".github/workflows/ci.yml"
                or timestamp(run["updated_at"]) < since):
            continue
        for number in range(run["run_attempt"], 0, -1):
            attempt = post_merge_report.exact_attempt(gh_get, repo, run, number)
            if timestamp(attempt["updated_at"]) < since:
                break
            if attempt["status"] == "completed" and attempt["conclusion"] in RED_CONCLUSIONS:
                yield attempt


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
    parser.add_argument("--since", required=True, type=timestamp)
    parser.add_argument("--repo", default="mtg-kernel/mtg-kernel")
    parser.add_argument("--branches", default="main,release/")
    parser.add_argument("--append", help="handoff log file to append new lines to")
    args = parser.parse_args(argv)
    branches = [b.strip() for b in args.branches.split(",") if b.strip()]
    lines, exit_code = [], 0
    for attempt in runs_since(args.repo, args.since, branches):
        try:
            jobs = list(post_merge_report.run_jobs(gh_get, args.repo, attempt))
            if post_merge_report.is_monitor(jobs):
                line = post_merge_report.report(gh_get, args.repo, attempt, jobs=jobs)[1]
                print(line)
                lines.append(line)
                continue
        except subprocess.CalledProcessError as error:
            print(f"Could not verify report for {attempt['id']}: {(error.stderr or '').strip()}", file=sys.stderr)
            exit_code = 1
        print(f"[CI] {attempt['event']} {attempt['conclusion']} on {attempt['head_branch']} at "
              f"{attempt['head_sha'][:9]} (post-merge report unconfirmed); {post_merge_report.attempt_url(attempt)}")
    if args.append:
        append_new(args.append, lines)
    return exit_code


if __name__ == "__main__":
    sys.exit(main())
