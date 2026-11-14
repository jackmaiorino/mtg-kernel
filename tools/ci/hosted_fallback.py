"""Rerun Windows CI on hosted runners when a self-hosted runner could not admit a job.

A job that a self-hosted runner accepted cannot move to another runner. Its job-started hook
(collab tools/ci-runners/ci_job_slots.py) waits for host_slots CPU admission at most
MTG_CI_ADMISSION_TIMEOUT seconds; on timeout it fails the job before any step, printing
ADMISSION_MARKER. This script runs in a hosted job after the self-hosted-capable jobs. It finds the
failed self-hosted jobs of this run whose log carries the marker and, if there are any, dispatches
this workflow on the same branch with `windows_runner` set to GitHub's hosted Windows image and
`fallback_scope` set to the Windows part of this run's group set (env SCOPE), so only Windows groups
run again there, never the Linux jobs this run already covered. Real test failures (no marker) are never rerun. A dispatched run
never dispatches again (the workflow skips this job when `windows_runner` is set).

Environment: GH_TOKEN (actions: write), GITHUB_REPOSITORY, GITHUB_RUN_ID, FALLBACK_REF (the branch),
optional GITHUB_STEP_SUMMARY and GITHUB_API_URL. Exit 0 unless the GitHub API fails.
"""
from __future__ import annotations

import json
import os
import sys
import time
import urllib.error
import urllib.request

ADMISSION_MARKER = "MTG_CI_ADMISSION_TIMEOUT"
HOSTED_WINDOWS = '"windows-2025"'
WORKFLOW = "ci.yml"
RETRYABLE_HTTP_STATUS = frozenset({404, 408, 429, 500, 502, 503, 504})
# This run's group set -> the Windows-only set a hosted rerun covers. Unknown sets rerun every
# Windows group.
FALLBACK_SCOPES = {"pr": "windows-pr", "windows-complement": "windows-complement", "full": "windows-full"}


def fallback_scope(scope: str | None) -> str:
    return FALLBACK_SCOPES.get(scope or "", "windows-full")


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


def _build_request(method: str, path: str, body: dict | None = None) -> urllib.request.Request:
    api = os.environ.get("GITHUB_API_URL", "https://api.github.com")
    data = None if body is None else json.dumps(body).encode("utf-8")
    req = urllib.request.Request(api + path, data=data, method=method, headers={
        "Accept": "application/vnd.github+json", "X-GitHub-Api-Version": "2022-11-28"})
    # The job log endpoint redirects to signed blob storage, which refuses a request that also
    # carries the token (401). urllib drops unredirected headers when it follows a redirect.
    req.add_unredirected_header("Authorization", "Bearer " + os.environ["GH_TOKEN"])
    return req


def _request(method: str, path: str, body: dict | None = None) -> bytes:
    with urllib.request.urlopen(_build_request(method, path, body), timeout=60) as response:
        return response.read()


def run_jobs(repo: str, run_id: str) -> list[dict]:
    jobs, page = [], 1
    while True:
        rows = json.loads(_request("GET", f"/repos/{repo}/actions/runs/{run_id}/jobs?per_page=100&page={page}"))["jobs"]
        jobs += rows
        if len(rows) < 100:
            return jobs
        page += 1


def job_log(repo: str, job_id: int, *, attempts: int = 4, delay: float = 2.0, sleep=time.sleep) -> str | None:
    """Fetch a completed job's log text, retrying briefly when GitHub has not yet finalized the log archive."""
    last_reason = None
    for attempt in range(attempts):
        try:
            text = _request("GET", f"/repos/{repo}/actions/jobs/{job_id}/logs").decode("utf-8", "replace")
            if text:
                return text
            last_reason = "empty log archive"
        except urllib.error.HTTPError as exc:
            try:
                exc.close()
            except OSError:
                pass
            last_reason = f"HTTP {exc.code}"
            if exc.code not in RETRYABLE_HTTP_STATUS:
                break
        except OSError as exc:
            last_reason = type(exc).__name__
        if attempt + 1 >= attempts:
            break
        sleep(delay * (2 ** attempt))
    print(f"hosted_fallback: could not read log for failed self-hosted job {job_id} ({last_reason})", file=sys.stderr)
    return None


def main() -> int:
    repo, run_id, ref = os.environ["GITHUB_REPOSITORY"], os.environ["GITHUB_RUN_ID"], os.environ["FALLBACK_REF"]
    timed_out = admission_timeouts(run_jobs(repo, run_id), lambda job_id: job_log(repo, job_id))
    lines = []
    if timed_out:
        scope = fallback_scope(os.environ.get("SCOPE"))
        _request("POST", f"/repos/{repo}/actions/workflows/{WORKFLOW}/dispatches",
                 {"ref": ref, "inputs": {"windows_runner": HOSTED_WINDOWS, "fallback_scope": scope}})
        lines.append(f"Self-hosted CPU admission timed out for {len(timed_out)} job(s); dispatched {WORKFLOW} on "
                     f"`{ref}` with windows_runner={HOSTED_WINDOWS} and fallback_scope={scope}. "
                     "Read that run's checks for these groups:")
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
