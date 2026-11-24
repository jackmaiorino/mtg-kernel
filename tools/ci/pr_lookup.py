"""The GitHub REST lookups of ci.yml's "Choose the group set" step (a dispatch without inputs).

open-pr-head: "<number> <head sha> <base sha>" of the first open, ready pull request for this
repository's branch GITHUB_REF_NAME, or an empty line (for runners without the GitHub CLI). ready-pr selects
the same tuple from JSON on stdin.
pr-run SHA: whole-set if this repository's ci.yml run for event pull_request at head commit SHA scheduled the Linux
Rust groups and is completing them (expanded `Rust tests (ubuntu-24.04, <group>)` jobs of rust-test-groups, each
finished with success or failure, or queued or running in a run that is still going), else partial. A cancelled,
timed-out or otherwise unfinished run or group job is not evidence: nothing would complete Linux Rust then.
Only then may the dispatch run just the Windows groups that run leaves out. The decision reads that run's actual
jobs, never a file list: a file list (GitHub's three-dot list) can differ from what the run classified (the
synthetic merge's diff).

Reads GH_TOKEN (the job token: this job has actions: read, contents: read, pull-requests: read, and its
self_hosted_budget.py and mac_route.py steps read the same runs and jobs endpoints), GITHUB_API_URL,
GITHUB_REPOSITORY, GITHUB_REPOSITORY_OWNER and GITHUB_REF_NAME. Standard library only. Any failure (an API error,
malformed JSON, a pagination miss, the time or request budget) exits non-zero without printing whole-set, and the
step then keeps the full group set.
"""
from __future__ import annotations

import json
import os
import re
import sys
import urllib.parse
import urllib.request

import self_hosted_budget

WORKFLOW_PATH = ".github/workflows/ci.yml"
# The rust-test-groups matrix groups (tools/ci/tests/test_pr_lookup.py keeps this equal to ci.yml and disjoint from
# the other jobs named `Rust tests (<os>, <group>)`: the Limited groups and the Eternal integration parts, which
# can run without the pull request set).
RUST_TEST_GROUPS = ("workspace-lib", "workspace-targets", "standard", "eternal", "cuda", "native-store",
                    "native-store-1", "trace", "saved-input", "limited-store")


def get(path: str):
    request = urllib.request.Request(f"{os.environ.get('GITHUB_API_URL', 'https://api.github.com')}/{path}")
    request.add_header("Accept", "application/vnd.github+json")
    request.add_header("Authorization", f"Bearer {os.environ['GH_TOKEN']}")
    with urllib.request.urlopen(request, timeout=30) as response:
        return json.load(response)


def open_pr_head(pulls: list, repository: str, ref: str) -> str:
    for pull in pulls:
        head = pull["head"]
        if (pull["state"] != "open" or pull["draft"] is not False or head["ref"] != ref
                or (head["repo"] or {}).get("full_name") != repository):
            continue
        number, head_sha, base_sha = pull["number"], head["sha"], pull["base"]["sha"]
        if (type(number) is not int or number < 1
                or not all(isinstance(sha, str) and re.fullmatch(r"[0-9a-f]{40}", sha)
                           for sha in (head_sha, base_sha))):
            raise ValueError("Invalid ready pull request number or SHA")
        return f"{number} {head_sha} {base_sha}"
    return ""


# A group job or a run that ran to an end of its own, and one that is still on its way to one.
FINISHED = ("success", "failure")
ACTIVE = ("queued", "in_progress", "waiting", "pending", "requested")


def scheduled_linux_rust(jobs: list, run: dict) -> bool:
    """Whether one run's latest-attempt jobs show the Linux rows of rust-test-groups scheduled and being completed.

    There must be at least one expanded row that was not skipped, and every such row must have finished with
    success or failure, or be queued or running while the run itself is still going. A run that ended cancelled,
    timed out or failed to start, or any such row (a newer push, a manual cancel, a runner that never took the
    job), shows nothing: the dispatch would take the Windows complement and no run would complete Linux Rust.
    Whether the tests passed is judged on that run, not here. Rows exist once detect-changes has finished; before
    that, or for a draft (lint only) or a narrowed scope, there are none. The gate `Rust tests (ubuntu-24.04)`
    alone does not count: the group rows are the direct evidence."""
    run_id = run.get("id")
    run_finished = run.get("status") == "completed"
    if run_finished and run.get("conclusion") not in FINISHED:
        return False
    if not run_finished and run.get("status") not in ACTIVE:
        return False
    names = {f"Rust tests (ubuntu-24.04, {group})" for group in RUST_TEST_GROUPS}
    rows = []
    for job in jobs:
        if not isinstance(job, dict) or job.get("run_id") != run_id:
            raise ValueError("expected job records of the chosen run")
        if job.get("name") in names and job.get("conclusion") != "skipped":
            rows.append(job)
    if not rows:
        return False
    for job in rows:
        if job.get("status") == "completed":
            if job.get("conclusion") not in FINISHED:
                return False
        elif run_finished or job.get("status") not in ACTIVE:
            return False
    return True


def newest_pull_request_runs(runs: list, repository: str, head_sha: str) -> list:
    """The ci.yml runs of event pull_request at head_sha in this repository (a fork's run at the same commit has
    another head repository), newest first by creation time; all runs sharing the newest time are returned, and a
    caller takes whole-set only if every one of them shows it."""
    chosen = []
    for run in runs:
        if not isinstance(run, dict):
            raise ValueError("expected run records")
        if (run.get("event") == "pull_request" and run.get("head_sha") == head_sha and run.get("path") == WORKFLOW_PATH
                and (run.get("repository") or {}).get("full_name") == repository
                and (run.get("head_repository") or {}).get("full_name") == repository):
            if not isinstance(run.get("id"), int) or not isinstance(run.get("created_at"), str):
                raise ValueError("expected a run id and creation time")
            chosen.append(run)
    if not chosen:
        return []
    newest = max(run["created_at"] for run in chosen)   # ISO 8601 UTC: text order is time order
    return [run for run in chosen if run["created_at"] == newest]


def api(path: str, budget):
    return self_hosted_budget._get(path, budget)


def paginate(path: str, key: str, budget) -> list:
    """Every row of a listing; a page that is not a listing, or rows that do not add up to total_count, is an
    error (never a shorter list)."""
    rows, page = [], 1
    while True:
        body = api(f"{path}&per_page=100&page={page}", budget)
        if not isinstance(body, dict) or not isinstance(body.get(key), list) or type(body.get("total_count")) is not int:
            raise ValueError(f"expected a {key} listing")
        rows += body[key]
        if len(body[key]) < 100:
            break
        page += 1
    if len(rows) != body["total_count"]:
        raise ValueError(f"{key} listing incomplete: {len(rows)} of {body['total_count']}")
    return rows


def pr_run(repository: str, head_sha: str, budget=None) -> str:
    if not re.fullmatch(r"[0-9a-f]{40}", head_sha):
        raise ValueError("expected a full commit SHA")
    budget = budget or self_hosted_budget.Budget(seconds=60.0, requests=20)
    runs = paginate(f"/repos/{repository}/actions/workflows/ci.yml/runs?event=pull_request&head_sha={head_sha}",
                    "workflow_runs", budget)
    newest = newest_pull_request_runs(runs, repository, head_sha)
    shown = [scheduled_linux_rust(paginate(f"/repos/{repository}/actions/runs/{run['id']}/jobs?filter=latest", "jobs",
                                           budget), run) for run in newest]
    return "whole-set" if shown and all(shown) else "partial"


def main(argv: list) -> int:
    repository = os.environ["GITHUB_REPOSITORY"]
    if argv in (["open-pr-head"], ["ready-pr"]):
        ref = os.environ["GITHUB_REF_NAME"]
        if argv[0] == "open-pr-head":
            head = urllib.parse.quote(f"{os.environ['GITHUB_REPOSITORY_OWNER']}:{ref}", safe=":/")
            pulls = get(f"repos/{repository}/pulls?state=open&head={head}")
        else:
            pulls = json.load(sys.stdin)
        print(open_pr_head(pulls, repository, ref))
        return 0
    if argv[:1] == ["pr-run"] and len(argv) == 2:
        # Printed only after every lookup and check passed; any exception exits non-zero with nothing printed.
        print(pr_run(repository, argv[1]))
        return 0
    print("usage: pr_lookup.py open-pr-head | ready-pr | pr-run <head sha>", file=sys.stderr)
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
