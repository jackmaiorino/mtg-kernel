"""List red post-merge CI runs since a time, one handoff log line each, for the sessions' watch loops.

usage: python tools/ci/red_push_runs.py --since 2026-11-21T07:00:00Z [--repo jackmaiorino/mtg-kernel]
       [--branches main,release/mull-c5719e1cf] [--append HANDOFF_LOG]
Uses the gh CLI (already authenticated where the watch loops run). A run counts when it is a push or dispatch
run on one of the branches (a name ending in / is a prefix) that completed with conclusion failure. Each line is
the post-merge report's line: every merge the run covers, oldest first, with its pull request, and the first
responder. With --append, also appends each line the file does not already contain (the handoff log). Prints
nothing when there is none; exit 0 unless gh fails.
"""
from __future__ import annotations

import argparse
import json
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import post_merge_report  # noqa: E402


def gh_get(path: str):
    out = subprocess.run(["gh", "api", path.lstrip("/")], capture_output=True, text=True, encoding="utf-8",
                         check=True).stdout
    return json.loads(out or "null")


def runs_since(repo: str, since: str) -> list[dict]:
    out = subprocess.run(["gh", "api", "--paginate",
                          f"repos/{repo}/actions/workflows/ci.yml/runs?status=completed&created=>={since}&per_page=100",
                          "--jq", ".workflow_runs[] | tojson"], capture_output=True, text=True, encoding="utf-8",
                         check=True).stdout
    return [json.loads(line) for line in out.splitlines() if line.strip()]


def red_runs(runs: list[dict], branches: list[str]) -> list[dict]:
    return [r for r in runs if r.get("conclusion") == "failure" and r.get("event") in ("push", "workflow_dispatch")
            and (r.get("head_branch") in branches or any(b.endswith("/") and (r.get("head_branch") or "").startswith(b)
                                                         for b in branches))
            and r.get("path", ".github/workflows/ci.yml").endswith("ci.yml")]


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
    args = parser.parse_args(argv)
    branches = [b.strip() for b in args.branches.split(",") if b.strip()]
    lines = [post_merge_report.report(gh_get, args.repo, run)[1]
             for run in red_runs(runs_since(args.repo, args.since), branches)]
    for text in lines:
        print(text)
    if args.append:
        append_new(args.append, lines)
    return 0


if __name__ == "__main__":
    sys.exit(main())
