"""Choose which Linux groups of this CI run go to the Mac runners, within their free slots.

The Mac runners run in a Linux arm64 VM on the Mac Studio. The detect-changes job runs this once per run,
before any group job exists; the Linux rows of rust-test-groups, rust-limited-groups and python-tests
route by its output. A self-hosted job cannot move once queued and GitHub keeps a queued job for a day,
so the choice fails closed: any error, a slow lookup, a Mac that looks off or busy, or no free slot routes
nothing, and every group stays on GitHub's hosted runners in this same run (no rerun).

The job token cannot list self-hosted runners (that needs admin rights), so liveness comes from the Mac
heartbeat workflow (.github/workflows/mac-heartbeat.yml), a no-op job on the Mac runners every five
minutes. The Mac counts as available only when its last heartbeat succeeded within MAX_HEARTBEAT_AGE and
no heartbeat has waited for a Mac runner longer than MAX_HEARTBEAT_WAIT (a Mac that is off, asleep or busy
with other jobs leaves heartbeats queued).

Environment:
  MAC_GROUPS   JSON list of group names in order of preference (vars.MTG_CI_MAC_GROUPS). Unset or empty
               routes nothing without any API call.
  MAC_SLOTS    Most jobs the Mac runners may hold at once, queued or running (vars.MTG_CI_MAC_SLOTS, the
               workflow passes 3 when it is unset).
  MAC_ELIGIBLE JSON list of every group with the same results on the Mac runners (the workflow's list).
  SCOPE, CHECKS, PYTHON_RELEVANT, RUST_RELEVANT  detect-changes outputs. Linux rows exist only for
               the full and pr sets. CHECKS narrows the available job families after scope selection.
  GH_TOKEN, GITHUB_REPOSITORY, GITHUB_RUN_ID, optional GITHUB_API_URL, GITHUB_OUTPUT, GITHUB_STEP_SUMMARY.

Writes `groups=<JSON list>` to GITHUB_OUTPUT. Exit 0 always.

Free slots = MAC_SLOTS minus unfinished jobs of other runs that a Mac runner could take (their runs-on
labels are all Mac runner labels). Best effort: runs starting together can both take the last free slot,
and this run's own later jobs are not seen by runs that start meanwhile; such a job waits for the next
Mac runner to finish (bounded by Mac throughput, not stuck).
"""
from __future__ import annotations

import datetime
import json
import os
import sys

from self_hosted_budget import Budget, _get, active_jobs, parse_slots

# Every label a Mac runner carries (GitHub adds self-hosted, Linux and ARM64; the rest are custom).
MAC_RUNNER_LABELS = frozenset({"self-hosted", "linux", "arm64", "fleet-mac-linux", "mac-studio-vm"})
HEARTBEAT_WORKFLOW = "mac-heartbeat.yml"
MAX_HEARTBEAT_AGE = 15 * 60
MAX_HEARTBEAT_WAIT = 4 * 60
LINUX_SCOPES = ("full", "pr")
FULL_RUST_GROUPS = frozenset({"eternal-integration", "workspace-targets", "native-store", "standard",
                              "limited-store"})
NOT_STARTED = ("requested", "pending", "waiting", "queued")


def parse_list(text: str | None) -> list[str]:
    """A JSON list of strings; anything else (a bare string included) is an error."""
    value = json.loads(text) if text else []
    if not isinstance(value, list) or not all(isinstance(item, str) for item in value):
        raise ValueError("expected a JSON list of strings")
    return value


def seconds(stamp: str) -> float:
    return datetime.datetime.fromisoformat(stamp.replace("Z", "+00:00")).timestamp()


def mac_available(runs: list[dict], now: float) -> str | None:
    """None when the heartbeat runs (newest first) show a live, unblocked Mac; otherwise the reason."""
    succeeded = [run for run in runs if run.get("status") == "completed" and run.get("conclusion") == "success"]
    if not succeeded:
        return "no successful heartbeat"
    age = now - max(seconds(run["updated_at"]) for run in succeeded)
    if age > MAX_HEARTBEAT_AGE:
        return f"last heartbeat {int(age)} s ago"
    waits = [now - seconds(run["created_at"]) for run in runs if run.get("status") in NOT_STARTED]
    if waits and max(waits) > MAX_HEARTBEAT_WAIT:
        return f"a heartbeat has waited {int(max(waits))} s for a Mac runner"
    return None


def held_by_mac(jobs: list[dict], skip_run: int | None = None) -> int:
    """Unfinished jobs of other runs that a Mac runner could be holding or will take."""
    count = 0
    for job in jobs:
        if job.get("status") == "completed" or (skip_run is not None and job.get("run_id") == skip_run):
            continue
        labels = {label.lower() for label in job.get("labels") or []}
        if labels and labels <= MAC_RUNNER_LABELS:
            count += 1
    return count


def wanted(groups: list[str], eligible: list[str], scope: str, python: bool, rust: bool,
           checks: str = "all") -> list[str]:
    """The named groups this run actually has as Linux rows, in the variable's order, without repeats."""
    if scope not in LINUX_SCOPES:
        return []
    python = python and checks in {"all", "python"}
    rust = rust and checks in {"all", "rules-limited"}
    allowed = set(eligible)
    if checks == "rules-limited":
        allowed -= FULL_RUST_GROUPS
    chosen = []
    for group in groups:
        if group in allowed and group not in chosen and (python if group.startswith("python-") else rust):
            chosen.append(group)
    return chosen


def choose(now: float) -> tuple[list[str], str]:
    groups = parse_list(os.environ.get("MAC_GROUPS"))
    if not groups:
        return [], "MTG_CI_MAC_GROUPS is empty"
    candidates = wanted(groups, parse_list(os.environ.get("MAC_ELIGIBLE")), os.environ.get("SCOPE", ""),
                        os.environ.get("PYTHON_RELEVANT") == "true", os.environ.get("RUST_RELEVANT") == "true",
                        checks=os.environ.get("CHECKS", "all"))
    if not candidates:
        return [], "no named group runs on Linux in this run"
    slots = parse_slots(os.environ.get("MAC_SLOTS"))
    if not slots:
        return [], "no Mac slots"
    repo, budget = os.environ["GITHUB_REPOSITORY"], Budget()
    runs = _get(f"/repos/{repo}/actions/workflows/{HEARTBEAT_WORKFLOW}/runs?per_page=30", budget)["workflow_runs"]
    reason = mac_available(runs, now)
    if reason:
        return [], f"Mac unavailable: {reason}"
    run_id = int(os.environ.get("GITHUB_RUN_ID") or 0) or None
    held = held_by_mac(active_jobs(repo, budget), skip_run=run_id)
    chosen = candidates[:max(0, slots - held)]
    return chosen, f"{held} job(s) of other runs queued or running there, {slots} slots, candidates {json.dumps(candidates)}"


def main() -> int:
    try:
        chosen, detail = choose(datetime.datetime.now(datetime.timezone.utc).timestamp())
    except Exception as error:  # noqa: BLE001 - any doubt keeps every group on hosted runners
        chosen, detail = [], f"check failed: {type(error).__name__}: {error}"
    line = f"Mac groups: {json.dumps(chosen)} ({detail})"
    print(line)
    for name, text in (("GITHUB_OUTPUT", f"groups={json.dumps(chosen)}\n"), ("GITHUB_STEP_SUMMARY", line + "\n")):
        path = os.environ.get(name)
        if path:
            with open(path, "a", encoding="utf-8") as out:
                out.write(text)
    return 0


if __name__ == "__main__":
    sys.exit(main())
