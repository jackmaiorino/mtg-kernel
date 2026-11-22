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

import base64
import json
import os
import re
import shlex
import sys
import urllib.request
from typing import Callable

# The aggregate checks only repeat the groups' results.
REPORT_JOB = "Report a red post-merge run"
AGGREGATES = ("Rust tests (ubuntu-24.04)", "Rust tests (windows-2025)", REPORT_JOB,
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


def run_jobs(get: Get, repo: str, run: dict):
    """Read every page for the observed attempt, even if another rerun has started."""
    path = f"/repos/{repo}/actions/runs/{run['id']}/attempts/{run['run_attempt']}/jobs"
    page = 1
    while True:
        rows = get(f"{path}?per_page=100&page={page}")["jobs"]
        yield from rows
        if len(rows) < 100:
            return
        page += 1


def failed_jobs(jobs) -> list[str]:
    # A shard stopped at its job timeout concludes cancelled (or timed_out).
    return sorted({job["name"] for job in jobs if job.get("conclusion") in ("failure", "cancelled", "timed_out")
                   and job["name"] not in AGGREGATES})


def exact_attempt(get: Get, repo: str, run: dict, number: int) -> dict:
    if run["run_attempt"] == number:
        return run
    attempt = get(f"/repos/{repo}/actions/runs/{run['id']}/attempts/{number}")
    if (attempt["id"], attempt["run_attempt"], attempt["head_sha"]) != (run["id"], number, run["head_sha"]):
        raise ValueError("workflow attempt identity changed")
    # The branch history is anchored at the original push, even when a
    # particular attempt was created later.
    return {**run, **attempt, "created_at": run["created_at"]}


def ran(get: Get, repo: str, run: dict, before: str = "") -> bool:
    """Whether a run actually ran its checks (a run replaced while waiting started no job), judged only by jobs
    of any attempt that started before `before` (an ISO time), so a later rerun never changes an old report."""
    page = 1
    while True:
        jobs = get(f"/repos/{repo}/actions/runs/{run['id']}/jobs?per_page=100&filter=all&page={page}")["jobs"]
        if any(job.get("conclusion") in ("success", "failure", "timed_out")
               and (not before or (job.get("started_at") or "9") < before) for job in jobs):
            return True
        if len(jobs) < 100:
            return False
        page += 1


def first_attempt_start(get: Get, repo: str, run: dict) -> str:
    """When the run's first attempt started (its own reruns do not move it)."""
    if (run.get("run_attempt") or 1) == 1 and run.get("run_started_at"):
        return run["run_started_at"]
    first = get(f"/repos/{repo}/actions/runs/{run['id']}/attempts/1")
    return first.get("run_started_at") or first.get("created_at") or run.get("created_at", "")


# Earlier push runs a report examines for the last monitored commit before calling the range incomplete.
MAX_HISTORY_PAGES = 5


def last_monitored_commit(get: Get, repo: str, branch: str, this: dict) -> tuple[str | None, bool]:
    """Return the latest checked predecessor and whether the monitored history search was exhausted."""
    before = first_attempt_start(get, repo, this)
    for page in range(1, MAX_HISTORY_PAGES + 1):
        # Run ids increase with creation, so an earlier run created in the same second still counts.
        # No status filter: an earlier run being rerun is still listed, and ran() judges it only by its jobs that
        # started before this run's first attempt.
        runs = get(f"/repos/{repo}/actions/workflows/ci.yml/runs?event=push&branch={branch}"
                   f"&created=%3C%3D{this['created_at']}&per_page=50&page={page}")["workflow_runs"]
        earlier = [run for run in runs if run["id"] < int(this["id"]) and run["head_sha"] != this["head_sha"]]
        for run in earlier:
            if ran(get, repo, run, before):
                return run["head_sha"], True
        if len(runs) < 50:
            return None, True
    return None, False


def release_push_enabled(get: Get, repo: str, sha: str) -> bool | None:
    """Recognize the release workflow's push filters before and after monitor activation."""
    source = get(f"/repos/{repo}/contents/.github/workflows/ci.yml?ref={sha}")
    if source.get("encoding") != "base64":
        return None
    workflow = base64.b64decode(source["content"]).decode("utf-8")
    layout = "\n".join(line.rstrip() for line in workflow.splitlines()
                       if line.strip() and not line.lstrip().startswith("#")) + "\n"
    events = re.search(r"""(?m)^(?:on|'on'|"on"):\n((?: +.*\n)*)""", layout)
    push = re.search(r"(?m)^  push:\n((?: {4,}.*\n)*)", events[1]) if events else None
    if not push:
        return None
    filters = re.fullmatch(r"    branches:\n      - main\n(      - 'release/\*\*'\n)?    tags:\n      - '\*'\n", push[1])
    return bool(filters[1]) if filters else None


def first_release_push_boundary(get: Get, repo: str, branch: str, run: dict, head: str,
                                current_event: dict | None) -> str | None:
    """Follow contiguous same-ref push events to a revision that had no release monitor."""
    if not branch.startswith("release/") or release_push_enabled(get, repo, head) is not True:
        return None
    before_by_head: dict[str, set[str]] = {}
    ref = f"refs/heads/{branch}"

    def add(payload: dict):
        after, before = payload.get("head") or payload.get("after"), payload.get("before")
        if (payload.get("ref") == ref and after and before
                and re.fullmatch(r"[0-9a-f]{40}", after) and re.fullmatch(r"[0-9a-f]{40}", before)
                and before != "0" * 40 and before != after):
            before_by_head.setdefault(after, set()).add(before)

    if current_event:
        add(current_event)
    for page in range(1, 4):
        events = get(f"/repos/{repo}/events?per_page=100&page={page}")
        for event in events:
            if event["type"] == "PushEvent" and event["created_at"] <= run["created_at"]:
                add(event["payload"])
        if len(events) < 100:
            break
    seen = set()
    while head not in seen and len(before_by_head.get(head, ())) == 1:
        seen.add(head)
        before, = before_by_head[head]
        enabled = release_push_enabled(get, repo, before)
        if enabled is False:
            return before
        if enabled is None:
            return None
        head = before
    return None


# The most first-parent commits a report walks back; a longer range is reported as incomplete, never silently.
MAX_COVERED = 100


def covered_merges(get: Get, repo: str, run: dict,
                   current_event: dict | None = None) -> tuple[list[dict], bool]:
    """First-parent commits up to head, oldest first, and whether the walk reached a verified boundary."""
    branch, head = run["head_branch"], run["head_sha"]
    base, history_examined = last_monitored_commit(get, repo, branch, run)
    if not base and history_examined:
        base = first_release_push_boundary(get, repo, branch, run, head, current_event)
    shas, sha, complete = [], head, base is not None
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


def is_monitor(jobs) -> bool:
    """Whether a completed run is a red monitor: its post-merge-report job executed (the workflow starts it only
    for a push run, or a full dispatch, on main or a release line with a failed or cancelled job)."""
    return any(job["name"] == REPORT_JOB and job.get("started_at")
               and job.get("conclusion") in ("success", "failure", "cancelled", "timed_out") for job in jobs)


# Failed-job name prefixes and the dispatch scope that reruns them alone; any other job reruns with a full dispatch.
JOB_SCOPES = [("Trainer library tests", "release-trainer")]


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


def attempt_url(run: dict) -> str:
    return f"{run['html_url']}/attempts/{run['run_attempt']}"


def compose(run_url: str, ref: str, event: str, failed: list[str], merges: list[dict],
            complete: bool = True) -> tuple[str, str]:
    kind = "push run" if event == "push" else "full dispatch run"
    scope = rerun_scope(failed)
    rerun = f"a `{scope}` dispatch" if scope else "a full dispatch"
    if complete:
        oldest = merges[0]
        responder = f"the author of #{oldest['pull']}" if oldest["pull"] else f"the author of `{oldest['sha'][:9]}`"
        coverage = f"It covers {len(merges)} merge{'s' if len(merges) != 1 else ''}, oldest first:"
        action = (f"First responder: {responder} (the oldest covered merge). They bisect by rerunning the failed jobs "
                  f"({rerun}) on each covered commit, then fix forward or revert the same day (merge rule of "
                  "2026-11-21) and say which in the handoff log:")
        handoff = f"first responder {responder}, who bisects with {rerun} on each covered commit"
    else:
        coverage = "The coverage boundary could not be verified. Listed commits, oldest first:"
        action = (f"First responder: undetermined. To start bisecting, rerun the failed jobs "
                  f"({rerun}) on each listed commit:")
        handoff = f"first responder undetermined; bisect with {rerun} on each listed commit"
    body = "\n".join([
        f"Post-merge CI {kind} on `{ref}` failed: {run_url}", "",
        coverage,
        *[f"- {label(m)}" for m in merges], "", "Failed jobs:", *[f"- {name}" for name in failed], "",
        action, "", "```", *bisect_commands(merges, scope), "```"])
    line = (f"[CI] post-merge {kind} red on {ref} covering " + ("" if complete else "(incomplete) ")
            + ", ".join(label(m).replace("`", "") for m in merges)
            + f": {', '.join(failed)}; {handoff.replace('`', '')}; {run_url}")
    return strip_mentions(body), strip_mentions(line)


def append_command(line: str) -> str:
    """The exact command, run from a collab checkout, that appends this run's line to the handoff log."""
    return f"printf '%s\\n' {shlex.quote(line)} >> {HANDOFF_LOG}"


def report(get: Get, repo: str, run: dict, current_event: dict | None = None, *, jobs=None
           ) -> tuple[str, str, list[dict]]:
    """Comment body, handoff line and covered merges of a red run attempt."""
    failed = failed_jobs(run_jobs(get, repo, run) if jobs is None else jobs)
    merges, complete = covered_merges(get, repo, run, current_event)
    body, line = compose(attempt_url(run), run["head_branch"], run["event"],
                         failed or ["(no failed job identified; see the run)"], merges, complete)
    return body, line, merges


def main() -> int:
    repo, run_id = os.environ["GITHUB_REPOSITORY"], os.environ["GITHUB_RUN_ID"]
    get = lambda path: request("GET", path)
    run = exact_attempt(get, repo, get(f"/repos/{repo}/actions/runs/{run_id}"), int(os.environ["GITHUB_RUN_ATTEMPT"]))
    run.update(head_sha=os.environ.get("GITHUB_SHA", run["head_sha"]),
               head_branch=os.environ.get("GITHUB_REF_NAME", run["head_branch"]),
               event=os.environ.get("GITHUB_EVENT_NAME", run["event"]))
    current_event = None
    if run["event"] == "push" and (event_path := os.environ.get("GITHUB_EVENT_PATH")):
        with open(event_path, encoding="utf-8") as stream:
            current_event = json.load(stream)
    body, line, merges = report(get, repo, run, current_event)
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
