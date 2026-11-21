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
import shlex
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


# Earlier push runs a report examines for the last monitored commit before calling the range incomplete.
MAX_HISTORY_PAGES = 5


def last_monitored_commit(get: Get, repo: str, branch: str, run_id) -> tuple[str | None, bool]:
    """(head of the newest push run on the branch created before this run that ran its checks, found): (None,
    True) when the branch has no such run, (None, False) when the examined history ran out first."""
    this = get(f"/repos/{repo}/actions/runs/{run_id}")
    for page in range(1, MAX_HISTORY_PAGES + 1):
        # Run ids increase with creation, so an earlier run created in the same second still counts.
        runs = get(f"/repos/{repo}/actions/workflows/ci.yml/runs?event=push&branch={branch}&status=completed"
                   f"&created=%3C%3D{this['created_at']}&per_page=50&page={page}")["workflow_runs"]
        for run in runs:
            if run["id"] < int(run_id) and run["head_sha"] != this["head_sha"] and ran(get, repo, run):
                return run["head_sha"], True
        if len(runs) < 50:
            return None, True
    return None, False


# The most first-parent commits a report walks back; a longer range is reported as incomplete, never silently.
MAX_COVERED = 100


def covered_merges(get: Get, repo: str, branch: str, run_id, head: str) -> tuple[list[dict], bool]:
    """First-parent commits after the last monitored commit up to head, oldest first ({sha, pull, author}), and
    whether the walk reached that commit (always true when there is no earlier monitored run)."""
    base, found = last_monitored_commit(get, repo, branch, run_id)
    shas, sha, complete = [], head, found
    if base:
        while sha != base:
            if len(shas) == MAX_COVERED:
                complete = False
                break
            shas.append(sha)
            parents = get(f"/repos/{repo}/commits/{sha}")["parents"]
            if not parents:
                complete = False      # reached a root without meeting the base (history rewritten)
                break
            sha = parents[0]["sha"]
    else:
        shas = [head]
    return [merged_pull_request(get, repo, branch, sha) for sha in reversed(shas)], complete


def merged_pull_request(get: Get, repo: str, branch: str, sha: str) -> dict:
    """{sha, pull, author}: the pull request merged into branch whose merge commit is sha (pull None if there is
    no such pull request or more than one)."""
    pulls, page = [], 1
    while True:
        rows = get(f"/repos/{repo}/commits/{sha}/pulls?per_page=100&page={page}") or []
        pulls += rows
        if len(rows) < 100:
            break
        page += 1
    matches = [p for p in pulls if p.get("merged_at") and (p.get("base") or {}).get("ref") == branch
               and p.get("merge_commit_sha") == sha]
    pull = matches[0] if len(matches) == 1 else None
    return {"sha": sha, "pull": pull["number"] if pull else None,
            "author": (pull.get("user") or {}).get("login") if pull else None}


def is_monitor(get: Get, repo: str, run: dict) -> bool:
    """Whether a completed run is a red monitor: its post-merge-report job executed (the workflow starts it only
    for a push run, or a full dispatch, on main or a release line with a failed or cancelled job)."""
    jobs = get(f"/repos/{repo}/actions/runs/{run['id']}/jobs?per_page=100")["jobs"]
    return any(job["name"] == "Report a red post-merge run" and job.get("started_at")
               and job.get("conclusion") in ("success", "failure", "cancelled", "timed_out") for job in jobs)


# Failed-job name prefixes and the dispatch scope that reruns them alone; any other job reruns with a full dispatch.
JOB_SCOPES = [("Trainer library tests", "trainer"), ("Python tools unit tests", "tools"), ("Python tests", "python"),
              ("Build shared Limited tests", "rules-limited"), ("Rust tests (ubuntu-24.04, limited", "rules-limited")]


def rerun_scope(failed: list[str]) -> str:
    """The one dispatch scope that reruns every failed job, or "" for a full dispatch."""
    found = {next((scope for prefix, scope in JOB_SCOPES if name.startswith(prefix)), "") for name in failed}
    return found.pop() if len(found) == 1 else ""


def bisect_commands(merges: list[dict], scope: str) -> list[str]:
    """Per covered commit: push a branch at it and dispatch the rerun there."""
    flag = f" -f scope={scope}" if scope else ""
    return [f"git push origin {m['sha']}:refs/heads/bisect-{m['sha'][:9]} && "
            f"gh workflow run ci.yml --ref bisect-{m['sha'][:9]}{flag}" for m in merges]


def strip_mentions(text: str) -> str:
    return text.replace("@", "@​")


def label(merge: dict) -> str:
    return f"`{merge['sha'][:9]}`" + (f" (#{merge['pull']})" if merge["pull"] else " (no pull request)")


def compose(run_url: str, ref: str, event: str, failed: list[str], merges: list[dict],
            complete: bool = True) -> tuple[str, str]:
    kind = "push run" if event == "push" else "full dispatch run"
    oldest = merges[0]
    responder = (f"the author of #{oldest['pull']}" if oldest["pull"] else f"the author of `{oldest['sha'][:9]}`")
    if not complete:
        responder = "the author of the oldest merge in the range (beyond the listed commits)"
    scope = rerun_scope(failed)
    rerun = f"a `{scope}` dispatch" if scope else "a full dispatch"
    body = "\n".join([
        f"Post-merge CI {kind} on `{ref}` failed: {run_url}", "",
        (f"It covers {len(merges)} merge{'s' if len(merges) != 1 else ''}, oldest first:" if complete else
         f"It covers more than {len(merges)} merges; the newest {len(merges)}, oldest first (the range continues "
         "before the first one listed):"),
        *[f"- {label(m)}" for m in merges], "", "Failed jobs:", *[f"- {name}" for name in failed], "",
        f"First responder: {responder} (the oldest covered merge). They bisect by rerunning the failed jobs "
        f"({rerun}) on each covered commit, then fix forward or revert the same day (merge rule of 2026-11-21) "
        "and say which in the handoff log:", "", "```", *bisect_commands(merges, scope), "```"])
    line = (f"[CI] post-merge {kind} red on {ref} covering " + ("" if complete else "(incomplete, newest only) ")
            + ", ".join(label(m).replace("`", "") for m in merges)
            + f": {', '.join(failed)}; first responder {responder}, who bisects with {rerun.replace('`', '')} on "
            f"each covered commit; {run_url}")
    return strip_mentions(body), strip_mentions(line)


def append_command(line: str) -> str:
    """The exact command, run from a collab checkout, that appends this run's line to the handoff log."""
    return f"printf '%s\\n' {shlex.quote(line)} >> {HANDOFF_LOG}"


def report(get: Get, repo: str, run: dict) -> tuple[str, str, list[dict]]:
    """Comment body, handoff line and covered merges of a red run (run: id, head_sha, head_branch, event, url)."""
    failed = failed_jobs(get, repo, run["id"]) or ["(no failed job identified; see the run)"]
    merges, complete = covered_merges(get, repo, run["head_branch"], run["id"], run["head_sha"])
    body, line = compose(run["html_url"], run["head_branch"], run["event"], failed, merges, complete)
    return body, line, merges


def main() -> int:
    repo, run_id = os.environ["GITHUB_REPOSITORY"], os.environ["GITHUB_RUN_ID"]
    get = lambda path: request("GET", path)
    run = get(f"/repos/{repo}/actions/runs/{run_id}")
    run.update(head_sha=os.environ.get("GITHUB_SHA", run["head_sha"]),
               head_branch=os.environ.get("GITHUB_REF_NAME", run["head_branch"]),
               event=os.environ.get("GITHUB_EVENT_NAME", run["event"]))
    body, line, merges = report(get, repo, run)
    command = append_command(line)
    # The handoff line and its command come first, so a failed comment never hides them.
    print(body)
    print("Handoff log line: " + line)
    print("To add it to the handoff log, run from a collab checkout: " + command, flush=True)
    summary = os.environ.get("GITHUB_STEP_SUMMARY")
    if summary:
        with open(summary, "a", encoding="utf-8") as out:
            out.write(body + "\n\nHandoff log line: `" + line + "`\n\nTo add it to the handoff log, run from a "
                      "collab checkout: `" + command + "`\n")
    failures = []
    for number in sorted({m["pull"] for m in merges if m["pull"]}):
        try:
            request("POST", f"/repos/{repo}/issues/{number}/comments", {"body": body})
        except Exception as error:  # noqa: BLE001 - report every comment failure, then fail the job
            failures.append(f"#{number}: {error}")
    for failure in failures:
        print("Could not comment on " + failure, file=sys.stderr)
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
