"""List red post-merge CI runs since a time, one handoff log line each, for the sessions' watch loops.

usage: python tools/ci/red_push_runs.py --since 2026-11-21T07:00:00Z [--repo jackmaiorino/mtg-kernel]
       [--branches main,release/mull-c5719e1cf]
Uses the gh CLI (already authenticated where the watch loops run). A run counts when it is a push or dispatch run on one of the branches (a
name ending in / is a prefix) that completed with conclusion failure. Prints nothing
when there is none; exit 0 unless gh fails.
"""
from __future__ import annotations

import argparse
import json
import subprocess
import sys


def gh(path: str) -> list[dict]:
    out = subprocess.run(["gh", "api", "--paginate", path, "--jq", ".workflow_runs[]"], capture_output=True,
                         text=True, encoding="utf-8", check=True).stdout
    return [json.loads(line) for line in out.splitlines() if line.strip()]


def red_runs(runs: list[dict], branches: list[str]) -> list[dict]:
    return [r for r in runs if r.get("conclusion") == "failure" and r.get("event") in ("push", "workflow_dispatch")
            and (r.get("head_branch") in branches or any(b.endswith("/") and (r.get("head_branch") or "").startswith(b)
                                                         for b in branches))
            and r.get("path", ".github/workflows/ci.yml").endswith("ci.yml")]


def line(run: dict) -> str:
    return (f"[CI] post-merge {run['event']} run red on {run['head_branch']} at {run['head_sha'][:9]} "
            f"({run['updated_at']}): {run['html_url']}")


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--since", required=True)
    parser.add_argument("--repo", default="jackmaiorino/mtg-kernel")
    parser.add_argument("--branches", default="main,release/")
    args = parser.parse_args(argv)
    branches = [b.strip() for b in args.branches.split(",") if b.strip()]
    runs = gh(f"repos/{args.repo}/actions/workflows/ci.yml/runs?status=completed&created=>={args.since}&per_page=100")
    for run in red_runs(runs, branches):
        print(line(run))
    return 0


if __name__ == "__main__":
    sys.exit(main())
