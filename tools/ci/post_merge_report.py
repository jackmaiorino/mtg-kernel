"""Report a red post-merge CI run on every pull request it covers, and print its handoff log line.

Runs in the workflow's post-merge-report job (a push run on main or a release line, or a full dispatch there,
with a failed or cancelled job). Pushes on a line share one concurrency group, so a run can cover several
merges: those after the last monitored commit, which is the head of the newest earlier push run on the branch
that actually ran (a run replaced while waiting never ran). The covered merges are the first-parent commits from
there to the run's head, oldest first, each with its pull request. The comment and the handoff log line name
them all and the first responder: the author of the oldest covered merge, who bisects by rerunning the failed
job scoped on each covered commit. No one is mentioned.

Environment: GH_TOKEN, GITHUB_REPOSITORY, GITHUB_RUN_ID, GITHUB_SHA, GITHUB_REF_NAME, GITHUB_EVENT_NAME,
optional GITHUB_SERVER_URL, GITHUB_API_URL, GITHUB_STEP_SUMMARY. Exit 0 unless the GitHub API fails.
"""
from __future__ import annotations

import json
import os
import sys
import urllib.request
from typing import Callable

# The aggregate checks only repeat the groups' results.
AGGREGATES = ("Rust tests (ubuntu-24.04)", "Rust tests (windows-2025)", "Report a red post-merge run",
              "Hosted fallback for self-hosted admission timeouts")
HANDOFF_LOG = "HANDOFFS/20261114/README.md"
Get = Callable[[str], object]


def request(method: str, path: str, body: dict | None = None):
    api = os.environ.get("GITHUB_API_URL", "https://api.github.com")
    data = None if body is None else json.dumps(body).encode("utf-8")
    req = urllib.request.Request(api + path, data=data, method=method, headers={
        "Accept": "application/vnd.github+json", "X-GitHub-Api-Version": "2022-11-28",
        "Authorization": "Bearer " + os.environ["GH_TOKEN"]})
    with urllib.request.urlopen(req, timeout=60) as response:
        return json.loads(response.read() or b"null")


def failed_jobs(get: Get, repo: str, run_id) -> list[str]:
    names, page = [], 1
    while True:
        rows = get(f"/repos/{repo}/actions/runs/{run_id}/jobs?per_page=100&page={page}")["jobs"]
        # A shard stopped at its job timeout concludes cancelled (or timed_out).
        names += [job["name"] for job in rows if job.get("conclusion") in ("failure", "cancelled", "timed_out")
                  and job["name"] not in AGGREGATES]
        if len(rows) < 100:
            return sorted(set(names))
        page += 1


def ran(get: Get, repo: str, run: dict) -> bool:
    """Whether a completed run actually ran its checks (a run replaced while waiting started no job)."""
    if run.get("conclusion") in ("success", "failure"):
        return True
    jobs = get(f"/repos/{repo}/actions/runs/{run['id']}/jobs?per_page=100")["jobs"]
    return any(job.get("conclusion") in ("success", "failure", "timed_out") for job in jobs)


def last_monitored_commit(get: Get, repo: str, branch: str, run_id) -> str | None:
    """Head of the newest push run on the branch, created before this run, that ran its checks."""
    this = get(f"/repos/{repo}/actions/runs/{run_id}")
    page = 1
    while page <= 5:
        runs = get(f"/repos/{repo}/actions/workflows/ci.yml/runs?event=push&branch={branch}&status=completed"
                   f"&per_page=50&page={page}")["workflow_runs"]
        for run in runs:
            if run["id"] != int(run_id) and run["created_at"] < this["created_at"] \
                    and run["head_sha"] != this["head_sha"] and ran(get, repo, run):
                return run["head_sha"]
        if len(runs) < 50:
            return None
        page += 1
    return None


def covered_merges(get: Get, repo: str, branch: str, run_id, head: str) -> list[dict]:
    """First-parent commits after the last monitored commit up to head, oldest first: {sha, pull, author}."""
    base = last_monitored_commit(get, repo, branch, run_id)
    shas = [head]
    if base:
        compare = get(f"/repos/{repo}/compare/{base}...{head}?per_page=250")
        parents = {c["sha"]: [p["sha"] for p in c["parents"]] for c in compare["commits"]}
        shas, sha = [], head
        while sha in parents and sha != base:
            shas.append(sha)
            sha = parents[sha][0]
        if not shas or (sha != base and len(compare["commits"]) >= 250):
            shas = shas or [head]      # beyond the comparison limit: report what is listed
    out = []
    for sha in reversed(shas):
        pulls = get(f"/repos/{repo}/commits/{sha}/pulls") or []
        merged = [p for p in pulls if p.get("merged_at")] or pulls
        pull = merged[0] if merged else None
        out.append({"sha": sha, "pull": pull["number"] if pull else None,
                    "author": (pull.get("user") or {}).get("login") if pull else None})
    return out


def strip_mentions(text: str) -> str:
    return text.replace("@", "@​")


def label(merge: dict) -> str:
    return f"`{merge['sha'][:9]}`" + (f" (#{merge['pull']})" if merge["pull"] else " (no pull request)")


def compose(run_url: str, ref: str, event: str, failed: list[str], merges: list[dict]) -> tuple[str, str]:
    kind = "push run" if event == "push" else "full dispatch run"
    oldest = merges[0]
    responder = (f"the author of #{oldest['pull']}" if oldest["pull"] else f"the author of `{oldest['sha'][:9]}`")
    body = "\n".join([
        f"Post-merge CI {kind} on `{ref}` failed: {run_url}", "",
        f"It covers {len(merges)} merge{'s' if len(merges) != 1 else ''}, oldest first:",
        *[f"- {label(m)}" for m in merges], "", "Failed jobs:", *[f"- {name}" for name in failed], "",
        f"First responder: {responder} (the oldest covered merge), who bisects by rerunning the failed job scoped "
        "on each covered commit (push a branch at the commit, then `gh workflow run ci.yml --ref BRANCH -f "
        "scope=SCOPE`), then fixes forward or reverts the same day (merge rule of 2026-11-21) and says which in "
        "the handoff log."])
    line = (f"[CI] post-merge {kind} red on {ref} covering " + ", ".join(label(m).replace("`", "") for m in merges)
            + f": {', '.join(failed)}; first responder {responder}; {run_url}")
    return strip_mentions(body), strip_mentions(line)


def append_command(created_at: str, ref: str) -> str:
    """The command a session runs, from a collab checkout, to put this red run in the handoff log."""
    return (f"python <mtg-kernel checkout>/tools/ci/red_push_runs.py --since {created_at or '<run start, UTC>'} "
            f"--branches {ref} --append {HANDOFF_LOG}")


def report(get: Get, repo: str, run: dict) -> tuple[str, str, list[dict]]:
    """Comment body, handoff line and covered merges of a red run (run: id, head_sha, head_branch, event, url)."""
    failed = failed_jobs(get, repo, run["id"]) or ["(no failed job identified; see the run)"]
    merges = covered_merges(get, repo, run["head_branch"], run["id"], run["head_sha"])
    body, line = compose(run["html_url"], run["head_branch"], run["event"], failed, merges)
    return body, line, merges


def main() -> int:
    repo, run_id = os.environ["GITHUB_REPOSITORY"], os.environ["GITHUB_RUN_ID"]
    get = lambda path: request("GET", path)
    run = get(f"/repos/{repo}/actions/runs/{run_id}")
    run.update(head_sha=os.environ.get("GITHUB_SHA", run["head_sha"]),
               head_branch=os.environ.get("GITHUB_REF_NAME", run["head_branch"]),
               event=os.environ.get("GITHUB_EVENT_NAME", run["event"]))
    body, line, merges = report(get, repo, run)
    for number in sorted({m["pull"] for m in merges if m["pull"]}):
        request("POST", f"/repos/{repo}/issues/{number}/comments", {"body": body})
    command = append_command(run.get("created_at", ""), run["head_branch"])
    print(body)
    print("Handoff log line: " + line)
    print("To add it to the handoff log, run from a collab checkout: " + command)
    summary = os.environ.get("GITHUB_STEP_SUMMARY")
    if summary:
        with open(summary, "a", encoding="utf-8") as out:
            out.write(body + "\n\nHandoff log line: `" + line + "`\n\nTo add it to the handoff log, run from a "
                      "collab checkout: `" + command + "`\n")
    return 0


if __name__ == "__main__":
    sys.exit(main())
