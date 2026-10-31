"""Rerun Windows CI on hosted runners when a self-hosted runner could not admit a job.

A job that a self-hosted runner accepted cannot move to another runner. Its job-started hook
(collab tools/ci-runners/ci_job_slots.py) waits for host_slots CPU admission at most
MTG_CI_ADMISSION_TIMEOUT seconds; on timeout it fails the job before any step, printing
ADMISSION_MARKER. This script runs in a hosted job after the self-hosted-capable jobs. It finds the
failed self-hosted jobs of this run whose log carries the marker and, if there are any, dispatches
this workflow on the same branch with `windows_runner` set to GitHub's hosted Windows image, so
those groups run again there. Real test failures (no marker) are never rerun. A dispatched run
never dispatches again (the workflow skips this job when `windows_runner` is set).

Environment: GH_TOKEN (actions: write), GITHUB_REPOSITORY, GITHUB_RUN_ID, FALLBACK_REF (the branch),
optional GITHUB_STEP_SUMMARY and GITHUB_API_URL. Exit 0 unless the GitHub API fails.
"""
from __future__ import annotations

import json
import os
import sys
import urllib.request

ADMISSION_MARKER = "MTG_CI_ADMISSION_TIMEOUT"
HOSTED_WINDOWS = '"windows-2025"'
WORKFLOW = "ci.yml"


def admission_timeouts(jobs: list[dict], logs) -> list[dict]:
    """The failed self-hosted jobs whose log shows the admission timeout marker. `logs(job_id)` returns the
    job's log text (or None when it cannot be read: such a job is not rerun, since its cause is unknown)."""
    out = []
    for job in jobs:
        if job.get("conclusion") != "failure" or "self-hosted" not in (job.get("labels") or []):
            continue
        text = logs(job["id"])
        if text is not None and ADMISSION_MARKER in text:
            out.append(job)
    return out


def _request(method: str, path: str, body: dict | None = None) -> bytes:
    api = os.environ.get("GITHUB_API_URL", "https://api.github.com")
    data = None if body is None else json.dumps(body).encode("utf-8")
    req = urllib.request.Request(api + path, data=data, method=method, headers={
        "Authorization": "Bearer " + os.environ["GH_TOKEN"], "Accept": "application/vnd.github+json",
        "X-GitHub-Api-Version": "2022-11-28"})
    with urllib.request.urlopen(req, timeout=60) as response:
        return response.read()


def run_jobs(repo: str, run_id: str) -> list[dict]:
    jobs, page = [], 1
    while True:
        rows = json.loads(_request("GET", f"/repos/{repo}/actions/runs/{run_id}/jobs?per_page=100&page={page}"))["jobs"]
        jobs += rows
        if len(rows) < 100:
            return jobs
        page += 1


def job_log(repo: str, job_id: int) -> str | None:
    try:
        return _request("GET", f"/repos/{repo}/actions/jobs/{job_id}/logs").decode("utf-8", "replace")
    except OSError:
        return None


def main() -> int:
    repo, run_id, ref = os.environ["GITHUB_REPOSITORY"], os.environ["GITHUB_RUN_ID"], os.environ["FALLBACK_REF"]
    timed_out = admission_timeouts(run_jobs(repo, run_id), lambda job_id: job_log(repo, job_id))
    lines = []
    if timed_out:
        _request("POST", f"/repos/{repo}/actions/workflows/{WORKFLOW}/dispatches",
                 {"ref": ref, "inputs": {"windows_runner": HOSTED_WINDOWS}})
        lines.append(f"Self-hosted CPU admission timed out for {len(timed_out)} job(s); dispatched {WORKFLOW} on "
                     f"`{ref}` with windows_runner={HOSTED_WINDOWS}. Read that run's checks for these groups:")
        lines += [f"- {job['name']} ({job.get('runner_name')})" for job in timed_out]
    else:
        lines.append("No self-hosted job timed out waiting for CPU admission; nothing to rerun.")
    print("\n".join(lines))
    summary = os.environ.get("GITHUB_STEP_SUMMARY")
    if summary:
        with open(summary, "a", encoding="utf-8") as out:
            out.write("\n".join(lines) + "\n")
    return 0


if __name__ == "__main__":
    sys.exit(main())
