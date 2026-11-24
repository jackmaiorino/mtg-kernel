"""The GitHub REST lookups of ci.yml's "Choose the group set" step (a dispatch without inputs).

open-pr-head: "<number> <head sha> <base sha>" of the first open, ready pull request for this
repository's branch GITHUB_REF_NAME, or an empty line (for runners without the GitHub CLI). ready-pr selects
the same tuple from JSON on stdin.
pr-run SHA NUMBER: whole-set if this repository's ci.yml run for event pull_request at head commit SHA, for pull
request NUMBER, qualifies; else partial. The complement is taken only beside a pull request run that is still going
or has succeeded, with no job of its latest attempt failed, cancelled or timed out. In full, from that run and its
latest-attempt jobs:
  0. the run is still going, or finished with success; every job is queued or running (while the run is going) or
     finished with success or skipped; any other conclusion, or a status this code does not know, anywhere in the
     run disqualifies it (any failed job can be a prerequisite of jobs that then never ran);
  1. exactly one `CI plan: rust=.. python=.. scope=.. checks=..` job (the ci-plan job, named from detect-changes'
     outputs), in the exact format, completed with success;
  2. the plan says rust=true, scope=pr and checks=all;
  3. every Linux row of the rust-test-groups matrix (each group except native-store-1, which the pull request set
     excludes on Linux) is present and not skipped;
  4. the plan says python=false and no Python build row ran, or it says python=true and both Python build rows
     (`Build Python test binaries (ubuntu-24.04)` and `(windows-2025)`) are present and not skipped.
Anything else, and any error, is partial: the full set. Established directly: the plan, the Linux Rust rows, the
Python build rows and no failed job so far. What else the pull request set runs (lint, the Limited and Eternal
families, the Python shards) follows from the workflow's own conditions on those same outputs; a failed job is the
one way a scheduled family goes missing, and rule 0 excludes it. In use: a default dispatch at a head whose pull
request run already has a failed or cancelled job runs the full matrix; the complement is for a run that is passing
so far or has passed. The decision reads that run's actual jobs, never a file list (GitHub's three-dot list can
differ from the synthetic merge's diff the run classified).
Consequences: a pull request that touches only Rust sources keeps the complement with no Python anywhere before
the merge, as before (its paths are not Python-relevant; the push to main runs everything). On this change alone,
a pull request labelled ci:full whose paths are Rust-only has the plan rust=true python=false scope=pr checks=all,
so a default dispatch at its head takes the Windows complement, where the file-list rule gave it a full dispatch
(its file list mapped to a narrower scope). This is narrower than before only until the viewer-scope change
(pull request 955) merges, because that change makes ci:full set both relevance flags: the plan then reads
python=true, the run schedules Python, and the complement is taken only with the Python build rows present, which
is the whole set. A run of a ci.yml without the plan job gives partial until the pull request is updated.
Left open. A job that fails after the dispatch has decided can leave its dependents unscheduled in the pull request
run; rerun the dispatch, which will then run the full set.

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
import time
import urllib.parse
import urllib.request

import self_hosted_budget

WORKFLOW_PATH = ".github/workflows/ci.yml"
# The rust-test-groups matrix groups (tools/ci/tests/test_pr_lookup.py keeps this equal to ci.yml and disjoint from
# the other jobs named `Rust tests (<os>, <group>)`: the Limited groups and the Eternal integration parts, which
# can run without the pull request set).
RUST_TEST_GROUPS = ("workspace-lib", "workspace-targets", "standard", "eternal", "cuda", "native-store",
                    "native-store-1", "trace", "saved-input", "limited-store")
# The Linux rows of that matrix in the pull request set (its exclude expression drops native-store-1 on Linux; the
# test pins it).
LINUX_RUST_GROUPS = tuple(group for group in RUST_TEST_GROUPS if group != "native-store-1")
# The python-env-build matrix rows in the pull request and full sets (a test keeps them equal to ci.yml). The build,
# not the Python test shards: the shards expand only after the build finishes, and a dispatch right after a push
# must still see the evidence.
PYTHON_BUILD_JOBS = ("Build Python test binaries (ubuntu-24.04)", "Build Python test binaries (windows-2025)")
# The ci-plan job's name (tools/ci/tests/test_pr_lookup.py pins it to ci.yml).
PLAN = re.compile(r"CI plan: rust=(true|false) python=(true|false) scope=([a-z0-9-]+) checks=([a-z0-9-]+)")


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


# Conclusions of a job that leave the run green so far, and statuses of a job still on its way.
GREEN = ("success", "skipped")
ACTIVE = ("queued", "in_progress", "waiting", "pending", "requested")


def green_so_far(jobs: list, run: dict) -> bool:
    """Rule 0: the run is still going or finished with success, and no job of its latest attempt has failed, been
    cancelled, timed out or ended any other way than success or skipped (nor has a status this code does not know;
    a job still on its way is fine while the run is going)."""
    run_finished = run.get("status") == "completed"
    if run_finished and run.get("conclusion") != "success":
        return False
    if not run_finished and run.get("status") not in ACTIVE:
        return False
    for job in jobs:
        if job["status"] == "completed":
            if job["conclusion"] not in GREEN:
                return False
        elif run_finished or job["status"] not in ACTIVE:
            return False
    return True


def scheduled_whole_set(jobs: list, run: dict) -> bool:
    """Rules 0 to 4 of pr-run (see the module docstring) for one run's latest-attempt jobs. Every job record is
    validated before anything is decided; a malformed record anywhere is an error."""
    run_id = run.get("id")
    for job in jobs:
        if (not isinstance(job, dict) or job.get("run_id") != run_id or not isinstance(job.get("name"), str)
                or not isinstance(job.get("status"), str) or not (job.get("conclusion") is None
                                                                  or isinstance(job.get("conclusion"), str))):
            raise ValueError("expected job records of the chosen run")
    if not green_so_far(jobs, run):
        return False
    plans = [job for job in jobs if job["name"].startswith("CI plan")]
    if len(plans) != 1 or plans[0]["status"] != "completed" or plans[0]["conclusion"] != "success":
        return False
    plan = PLAN.fullmatch(plans[0]["name"])
    if not plan:
        return False
    rust, python, scope, checks = plan.groups()
    if (rust, scope, checks) != ("true", "pr", "all"):
        return False
    present = {job["name"] for job in jobs if job["conclusion"] != "skipped"}
    if not {f"Rust tests (ubuntu-24.04, {group})" for group in LINUX_RUST_GROUPS} <= present:
        return False
    if python == "false":
        # The plan skipped Python by relevance; a Python build row that ran anyway disagrees with it.
        return not present & set(PYTHON_BUILD_JOBS)
    return set(PYTHON_BUILD_JOBS) <= present


def newest_pull_request_runs(runs: list, repository: str, head_sha: str, number: int) -> list:
    """The ci.yml runs of event pull_request at head_sha in this repository (a fork's run at the same commit has
    another head repository) whose pull_requests list contains `number` (a run listed for another pull request
    sharing the head is not this one's; a run whose list holds both numbers is that run for both; an empty list
    does not match), newest by creation time; all runs sharing the newest time are returned, and a caller takes
    whole-set only if every one of them shows it."""
    chosen = []
    for run in runs:
        if not isinstance(run, dict):
            raise ValueError("expected run records")
        pulls = run.get("pull_requests")
        if not isinstance(pulls, list):
            raise ValueError("expected a pull_requests list")
        if (run.get("event") == "pull_request" and run.get("head_sha") == head_sha and run.get("path") == WORKFLOW_PATH
                and (run.get("repository") or {}).get("full_name") == repository
                and (run.get("head_repository") or {}).get("full_name") == repository
                and any(isinstance(pull, dict) and pull.get("number") == number for pull in pulls)):
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
    """Every row of a listing. A page that is not a listing, a total_count that changes between pages, an id seen
    twice (a row inserted between page reads shifts the pages) or rows that do not add up to total_count is an
    error, never a shorter list."""
    rows, page, total, ids = [], 1, None, set()
    while True:
        body = api(f"{path}&per_page=100&page={page}", budget)
        if not isinstance(body, dict) or not isinstance(body.get(key), list) or type(body.get("total_count")) is not int:
            raise ValueError(f"expected a {key} listing")
        if total is not None and body["total_count"] != total:
            raise ValueError(f"{key} listing changed between pages")
        total = body["total_count"]
        for row in body[key]:
            if not isinstance(row, dict) or type(row.get("id")) is not int or row["id"] in ids:
                raise ValueError(f"{key} listing has a missing or repeated id")
            ids.add(row["id"])
        rows += body[key]
        if len(body[key]) < 100:
            break
        page += 1
    if len(rows) != total:
        raise ValueError(f"{key} listing incomplete: {len(rows)} of {total}")
    return rows


def pr_run(repository: str, head_sha: str, number: int, budget=None) -> str:
    if not re.fullmatch(r"[0-9a-f]{40}", head_sha):
        raise ValueError("expected a full commit SHA")
    if type(number) is not int or number < 1:
        raise ValueError("expected a pull request number")
    budget = budget or self_hosted_budget.Budget(seconds=60.0, requests=20)
    runs = paginate(f"/repos/{repository}/actions/workflows/ci.yml/runs?event=pull_request&head_sha={head_sha}",
                    "workflow_runs", budget)
    newest = newest_pull_request_runs(runs, repository, head_sha, number)
    shown = [scheduled_whole_set(paginate(f"/repos/{repository}/actions/runs/{run['id']}/jobs?filter=latest", "jobs",
                                          budget), run) for run in newest]
    # The deadline once more after the last response: a late answer is no answer.
    if time.monotonic() > budget.deadline:
        raise TimeoutError("pr-run lookup exceeded its time budget")
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
    if argv[:1] == ["pr-run"] and len(argv) == 3 and argv[2].isdigit():
        # Printed only after every lookup and check passed; any exception exits non-zero with nothing printed.
        print(pr_run(repository, argv[1], int(argv[2])))
        return 0
    print("usage: pr_lookup.py open-pr-head | ready-pr | pr-run <head sha> <pull request number>", file=sys.stderr)
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
