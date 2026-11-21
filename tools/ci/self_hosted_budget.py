"""Choose which pull request Windows groups go to the self-hosted pool, within its free slots.

A self-hosted runner takes one job at a time and a job cannot move once queued for it, so routing more
groups than the pool can start leaves them waiting in GitHub's queue with no fallback. The detect-changes
job runs this once per pull request run, before any Windows group job exists, and the Windows groups route
by its output.

Environment:
  PR_GROUPS  JSON list of groups to route, in order of preference (vars.MTG_CI_PR_WINDOWS_GROUPS).
  PR_RUNNER  JSON runs-on labels of the pool (vars.MTG_CI_PR_WINDOWS_RUNNER), a list or one string.
  PR_SLOTS   Most jobs the pool may hold at once, queued or running (vars.MTG_CI_PR_WINDOWS_SLOTS).
             Unset or empty: every named group routes, as before this budget existed.
  PR_ELIGIBLE  JSON list of the groups this run has on Windows; other named groups are skipped before
             the free slots are handed out (optional: unset, every named group is eligible).
  GH_TOKEN, GITHUB_REPOSITORY, GITHUB_RUN_ID, optional GITHUB_API_URL, GITHUB_OUTPUT, GITHUB_STEP_SUMMARY.

Writes `groups=<JSON list>` to GITHUB_OUTPUT. Any error, more than MAX_REQUESTS API calls or more than
DEADLINE_SECONDS while counting the pool's jobs routes no group (every group stays on hosted runners).
Exit 0 always.

The budget is best effort, not a reservation: runs that start within the same few seconds can each take
the same free slot; the later job then waits in GitHub's queue until a pool runner finishes a job (a
self-hosted queue has no timeout or fallback). A rejected group falls through to MTG_CI_WINDOWS_RUNNER,
which is meant to stay hosted while this budget is in use.
"""
from __future__ import annotations

import json
import os
import sys
import time
import urllib.request

ACTIVE_RUN_STATUSES = ("in_progress", "queued", "waiting", "pending", "requested")
DEADLINE_SECONDS = 60.0
MAX_REQUESTS = 60


class Budget:
    """Bounds the API work so the detect-changes job always finishes well inside its timeout."""

    def __init__(self, seconds: float = DEADLINE_SECONDS, requests: int = MAX_REQUESTS):
        self.deadline, self.requests = time.monotonic() + seconds, requests

    def spend(self) -> float:
        left = self.deadline - time.monotonic()
        self.requests -= 1
        if left <= 0 or self.requests < 0:
            raise TimeoutError("budget lookup exceeded its time or request limit")
        return min(15.0, left)


def parse_list(text: str | None) -> list[str]:
    value = json.loads(text) if text else []
    if isinstance(value, str):
        value = [value]
    if not isinstance(value, list) or not all(isinstance(item, str) for item in value):
        raise ValueError("expected a JSON list of strings")
    return value


def parse_slots(text: str | None) -> int | None:
    if text is None or not text.strip():
        return None
    slots = int(text)
    if slots < 0:
        raise ValueError("slots must not be negative")
    return slots


def held_by_pool(jobs: list[dict], labels: list[str], skip_run: int | None = None) -> int:
    """Jobs not yet completed that a pool runner could be holding or will take, counted generously (a
    wrong count only sends a group to hosted runners): every job requesting all the pool labels, and every
    self-hosted job that shares a custom pool label (any label but "self-hosted"), such as a broader
    ["self-hosted", "Windows", "mtg-ci-windows"] request that a pool runner can also serve. The runner list
    itself needs admin rights the job token lacks. Case-insensitive."""
    want = {label.lower() for label in labels}
    custom = want - {"self-hosted"}
    count = 0
    for job in jobs:
        if job.get("status") == "completed" or (skip_run is not None and job.get("run_id") == skip_run):
            continue
        have = {label.lower() for label in job.get("labels") or []}
        if want and (want <= have or ("self-hosted" in have and have & custom)):
            count += 1
    return count


def choose(groups: list[str], slots: int | None, held: int) -> list[str]:
    if slots is None:
        return list(groups)
    return list(groups[:max(0, slots - held)])


def _get(path: str, budget: Budget) -> dict:
    api = os.environ.get("GITHUB_API_URL", "https://api.github.com")
    request = urllib.request.Request(api + path, headers={
        "Accept": "application/vnd.github+json", "X-GitHub-Api-Version": "2022-11-28",
        "Authorization": "Bearer " + os.environ["GH_TOKEN"]})
    with urllib.request.urlopen(request, timeout=budget.spend()) as response:
        return json.loads(response.read())


def active_jobs(repo: str, budget: Budget | None = None) -> list[dict]:
    """Every job of every active run in the repository (any workflow can target the pool)."""
    budget = budget or Budget()
    run_ids = set()
    for status in ACTIVE_RUN_STATUSES:
        page = 1
        while True:
            rows = _get(f"/repos/{repo}/actions/runs?status={status}&per_page=100&page={page}", budget)["workflow_runs"]
            run_ids.update(row["id"] for row in rows)
            if len(rows) < 100:
                break
            page += 1
    jobs = []
    for run_id in sorted(run_ids):
        page = 1
        while True:
            rows = _get(f"/repos/{repo}/actions/runs/{run_id}/jobs?filter=latest&per_page=100&page={page}",
                        budget)["jobs"]
            jobs += rows
            if len(rows) < 100:
                break
            page += 1
    return jobs


def main() -> int:
    lines = []
    try:
        groups = parse_list(os.environ.get("PR_GROUPS"))
        labels = parse_list(os.environ.get("PR_RUNNER"))
        slots = parse_slots(os.environ.get("PR_SLOTS"))
        eligible = os.environ.get("PR_ELIGIBLE")
        if eligible:
            allowed = set(parse_list(eligible))
            groups = [group for group in groups if group in allowed]
        if slots is None or not groups:
            chosen = choose(groups, slots, 0)
            lines.append(f"Self-hosted pull request groups: {json.dumps(chosen)} (no slot budget set)")
        else:
            run_id = int(os.environ.get("GITHUB_RUN_ID") or 0) or None
            held = held_by_pool(active_jobs(os.environ["GITHUB_REPOSITORY"]), labels, skip_run=run_id)
            chosen = choose(groups, slots, held)
            lines.append(f"Self-hosted pull request groups: {json.dumps(chosen)} ({held} job(s) already queued or "
                         f"running on {json.dumps(labels)}, budget {slots})")
    except Exception as error:  # noqa: BLE001 - any doubt keeps every group on hosted runners
        chosen = []
        lines.append(f"Self-hosted pull request groups: [] (budget check failed: {type(error).__name__}: {error})")
    print("\n".join(lines))
    for name, text in (("GITHUB_OUTPUT", f"groups={json.dumps(chosen)}\n"),
                       ("GITHUB_STEP_SUMMARY", "\n".join(lines) + "\n")):
        path = os.environ.get(name)
        if path:
            with open(path, "a", encoding="utf-8") as out:
                out.write(text)
    return 0


if __name__ == "__main__":
    sys.exit(main())
