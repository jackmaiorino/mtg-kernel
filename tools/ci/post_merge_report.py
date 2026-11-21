"""Comment a red post-merge CI run's failed jobs on the pull request its merge commit belongs to.

Runs in the workflow's post-merge-report job (a push run on main or a release line, or a full dispatch there,
with a failed job). Finds the pull request with GET /repos/{repo}/commits/{sha}/pulls, comments the failed
job names, the run link and the merge commit, without mentioning anyone, and prints one handoff log line
(the log lives in another repository; the sessions' watch loops copy the line from tools/ci/red_push_runs.py).

Environment: GH_TOKEN, GITHUB_REPOSITORY, GITHUB_RUN_ID, GITHUB_SHA, GITHUB_REF_NAME, GITHUB_EVENT_NAME,
optional GITHUB_SERVER_URL, GITHUB_API_URL, GITHUB_STEP_SUMMARY. Exit 0 unless the GitHub API fails.
"""
from __future__ import annotations

import json
import os
import sys
import urllib.request

# The aggregate checks only repeat the groups' results.
AGGREGATES = ("Rust tests (ubuntu-24.04)", "Rust tests (windows-2025)", "Report a red post-merge run",
              "Hosted fallback for self-hosted admission timeouts")


def request(method: str, path: str, body: dict | None = None):
    api = os.environ.get("GITHUB_API_URL", "https://api.github.com")
    data = None if body is None else json.dumps(body).encode("utf-8")
    req = urllib.request.Request(api + path, data=data, method=method, headers={
        "Accept": "application/vnd.github+json", "X-GitHub-Api-Version": "2022-11-28",
        "Authorization": "Bearer " + os.environ["GH_TOKEN"]})
    with urllib.request.urlopen(req, timeout=60) as response:
        return json.loads(response.read() or b"null")


def failed_jobs(repo: str, run_id: str) -> list[str]:
    names, page = [], 1
    while True:
        rows = request("GET", f"/repos/{repo}/actions/runs/{run_id}/jobs?per_page=100&page={page}")["jobs"]
        names += [job["name"] for job in rows if job.get("conclusion") == "failure" and job["name"] not in AGGREGATES]
        if len(rows) < 100:
            return sorted(set(names))
        page += 1


def merged_pull_request(repo: str, sha: str) -> dict | None:
    pulls = request("GET", f"/repos/{repo}/commits/{sha}/pulls") or []
    merged = [p for p in pulls if p.get("merged_at")]
    return (merged or pulls or [None])[0]


def strip_mentions(text: str) -> str:
    return text.replace("@", "@​")


def compose(repo, run_url, sha, ref, event, failed, pull) -> tuple[str, str]:
    where = f"`{ref}` at merge commit `{sha[:9]}`" + (f" (from #{pull['number']})" if pull else "")
    kind = "push run" if event == "push" else "full dispatch run"
    body = "\n".join([f"Post-merge CI {kind} on {where} failed: {run_url}", "",
                      *[f"- {name}" for name in failed], "",
                      "Fix forward or revert the same day (merge rule of 2026-11-21) and say which in the handoff log."])
    line = (f"[CI] post-merge {kind} red on {ref} at {sha[:9]}"
            + (f" (#{pull['number']})" if pull else "") + f": {', '.join(failed)}; {run_url}")
    return strip_mentions(body), strip_mentions(line)


def main() -> int:
    repo, run_id, sha = os.environ["GITHUB_REPOSITORY"], os.environ["GITHUB_RUN_ID"], os.environ["GITHUB_SHA"]
    ref, event = os.environ.get("GITHUB_REF_NAME", ""), os.environ.get("GITHUB_EVENT_NAME", "push")
    run_url = f"{os.environ.get('GITHUB_SERVER_URL', 'https://github.com')}/{repo}/actions/runs/{run_id}"
    failed = failed_jobs(repo, run_id)
    pull = merged_pull_request(repo, sha)
    body, line = compose(repo, run_url, sha, ref, event, failed, pull)
    if pull and failed:
        request("POST", f"/repos/{repo}/issues/{pull['number']}/comments", {"body": body})
    print(body)
    print("Handoff log line: " + line)
    summary = os.environ.get("GITHUB_STEP_SUMMARY")
    if summary:
        with open(summary, "a", encoding="utf-8") as out:
            out.write(body + "\n\nHandoff log line: `" + line + "`\n")
    return 0


if __name__ == "__main__":
    sys.exit(main())
