"""The dispatch step's lookups: the ready pull request, and whether that pull request's own run scheduled the Linux
Rust groups (pr-run), decided from the run's actual jobs."""
import contextlib
import io
import os
import re
import sys
import unittest
from pathlib import Path
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import pr_lookup  # noqa: E402
import self_hosted_budget  # noqa: E402

WORKFLOW = Path(__file__).resolve().parents[3] / ".github" / "workflows" / "ci.yml"
REPOSITORY = "owner/project"
REF = "topic/x"
BASE = "0" * 40
HEAD = "a" * 40

# Run 45502283040 (a ready pull request, 2026-11-24): names, status, conclusion, latest attempt.
READY_RUN_JOBS = [
    ('Build Python test binaries (${{ matrix.os }})', 'completed', 'skipped'),
    ('Python tests (${{ matrix.os }}, shard ${{ matrix.shard }})', 'completed', 'skipped'),
    ('Python tools unit tests (ubuntu-24.04)', 'completed', 'skipped'),
    ('Trainer library tests (ubuntu-24.04, shard ${{ matrix.shard }})', 'completed', 'skipped'),
    ('Detect changed paths', 'completed', 'success'),
    ('Build shared Eternal integration tests (ubuntu-24.04)', 'in_progress', None),
    ('Build shared Limited tests (ubuntu-24.04, binaries)', 'in_progress', None),
    ('Build shared Limited tests (ubuntu-24.04, default-list)', 'in_progress', None),
    ('Build shared Limited tests (ubuntu-24.04, tests)', 'in_progress', None),
    ('Rust formatting and lint', 'in_progress', None),
    ('Rust tests (ubuntu-24.04, cuda)', 'in_progress', None),
    ('Rust tests (ubuntu-24.04, native-store)', 'in_progress', None),
    ('Rust tests (ubuntu-24.04, standard)', 'in_progress', None),
    ('Rust tests (ubuntu-24.04, workspace-targets)', 'in_progress', None),
    ('Rust tests (windows-2025, native-store)', 'in_progress', None),
    ('Rust tests (windows-2025, native-store-1)', 'in_progress', None),
    ('Rust tests (windows-2025, workspace-lib)', 'in_progress', None),
    ('Rust tests (windows-2025, workspace-targets)', 'in_progress', None),
    ('Rust tests (ubuntu-24.04, eternal)', 'queued', None),
    ('Rust tests (ubuntu-24.04, limited-store)', 'queued', None),
    ('Rust tests (ubuntu-24.04, saved-input)', 'queued', None),
    ('Rust tests (ubuntu-24.04, trace)', 'queued', None),
    ('Rust tests (ubuntu-24.04, workspace-lib)', 'queued', None),
]
# Run 45499933838 (a draft pull request run: lint only).
DRAFT_RUN_JOBS = [
    ('Rust formatting and lint', 'completed', 'failure'),
    ('Build Python test binaries (${{ matrix.os }})', 'completed', 'skipped'),
    ('Build shared Limited tests (${{ matrix.os }}, ${{ matrix.part }})', 'completed', 'skipped'),
    ('Hosted fallback for self-hosted admission timeouts', 'completed', 'skipped'),
    ('Python tests (${{ matrix.os }}, shard ${{ matrix.shard }})', 'completed', 'skipped'),
    ('Python tools unit tests (ubuntu-24.04)', 'completed', 'skipped'),
    ('Report a red post-merge run', 'completed', 'skipped'),
    ('Rust tests (${{ matrix.os }})', 'completed', 'skipped'),
    ('Rust tests (${{ matrix.os }}, ${{ matrix.group }})', 'completed', 'skipped'),
    ('Rust tests (${{ matrix.os }}, ${{ matrix.group }})', 'completed', 'skipped'),
    ('Trainer library tests (ubuntu-24.04, shard ${{ matrix.shard }})', 'completed', 'skipped'),
    ('Detect changed paths', 'completed', 'success'),
]
# A run still waiting in its concurrency group (run 45500437409, pending): no jobs at all.
PENDING_RUN_JOBS = []


def pull(number, sha, repo=REPOSITORY, draft=False, ref=REF):
    return {"number": number, "state": "open", "draft": draft, "base": {"sha": BASE},
            "head": {"sha": sha, "ref": ref, "repo": {"full_name": repo} if repo else None}}


def jobs(rows, run_id=7):
    return [{"name": name, "status": status, "conclusion": conclusion, "run_id": run_id}
            for name, status, conclusion in rows]


def run(run_id, created="2026-11-24T10:00:00Z", event="pull_request", sha=HEAD, head_repo=REPOSITORY,
        path=".github/workflows/ci.yml", status="in_progress", conclusion=None):
    return {"id": run_id, "event": event, "head_sha": sha, "path": path, "created_at": created,
            "status": status, "conclusion": conclusion,
            "repository": {"full_name": REPOSITORY}, "head_repository": {"full_name": head_repo}}


class FakeApi:
    """Answers the two listings; runs: run records; jobs: {run id: job records}; page sizes of 100."""

    def __init__(self, runs, jobs_by_run, fail=None, total_offset=0, malformed=None):
        self.runs, self.jobs, self.fail, self.offset, self.malformed, self.paths = \
            runs, jobs_by_run, fail, total_offset, malformed, []

    def __call__(self, path, budget):
        budget.spend()
        self.paths.append(path)
        if self.fail and self.fail in path:
            raise OSError("fixture API failure")
        page = int(re.search(r"[?&]page=(\d+)", path).group(1))
        if "/actions/workflows/ci.yml/runs?" in path:
            key, rows = "workflow_runs", self.runs
        else:
            key, rows = "jobs", self.jobs[int(re.search(r"/runs/(\d+)/jobs", path).group(1))]
        body = {key: rows[(page - 1) * 100:page * 100], "total_count": len(rows) + self.offset}
        if self.malformed and self.malformed in path:
            body = {"message": "unexpected"}
        return body


def decide(runs, jobs_by_run, budget=None, **kwargs):
    fake = FakeApi(runs, jobs_by_run, **kwargs)
    with mock.patch.object(pr_lookup, "api", fake):
        return pr_lookup.pr_run(REPOSITORY, HEAD, budget), fake.paths


class PrLookupTests(unittest.TestCase):
    def test_open_pr_head_picks_the_first_ready_same_repository_pull_request(self):
        pulls = [pull(1, "a" * 40, repo="someone/fork"), pull(2, "b" * 40, draft=True), pull(3, "c" * 40, repo=None),
                 pull(4, "d" * 40, ref="different"), pull(5, "e" * 40), pull(6, "f" * 40)]
        self.assertEqual(pr_lookup.open_pr_head(pulls, REPOSITORY, REF), f"5 {'e' * 40} {BASE}")
        self.assertEqual(pr_lookup.open_pr_head(pulls[:4], REPOSITORY, REF), "")
        self.assertEqual(pr_lookup.open_pr_head([], REPOSITORY, REF), "")

    def test_open_pr_head_rejects_invalid_ready_identity(self):
        for record in (pull(True, "a" * 40), pull(0, "a" * 40), pull(1, "bad"),
                       {**pull(1, "a" * 40), "base": {"sha": "g" * 40}}):
            with self.subTest(record=record), self.assertRaises(ValueError):
                pr_lookup.open_pr_head([record], REPOSITORY, REF)

    def test_main_queries_the_ready_pull_request(self):
        calls = []

        def get(path):
            calls.append(path)
            return [pull(7, "f" * 40)]
        out = io.StringIO()
        environment = {"GITHUB_REPOSITORY": REPOSITORY, "GITHUB_REPOSITORY_OWNER": "owner", "GITHUB_REF_NAME": REF,
                       "GH_TOKEN": "unused"}
        with mock.patch.dict(os.environ, environment), mock.patch.object(pr_lookup, "get", get), \
                contextlib.redirect_stdout(out):
            self.assertEqual(pr_lookup.main(["open-pr-head"]), 0)
        self.assertEqual((out.getvalue(), calls), (f"7 {'f' * 40} {BASE}\n",
                                                   ["repos/owner/project/pulls?state=open&head=owner:topic/x"]))


class EvidenceTests(unittest.TestCase):
    """Point 1: the decision over literal job lists."""

    def test_linux_rust_group_rows_finished_or_on_their_way_show_the_whole_set(self):
        self.assertTrue(pr_lookup.scheduled_linux_rust(jobs(READY_RUN_JOBS), run(7)))
        for status, conclusion in (("queued", None), ("in_progress", None), ("completed", "success"),
                                   ("completed", "failure"), ("waiting", None)):
            with self.subTest(status=status, conclusion=conclusion):
                rows = [("Detect changed paths", "completed", "success"),
                        ("Rust tests (ubuntu-24.04, workspace-lib)", status, conclusion)]
                self.assertTrue(pr_lookup.scheduled_linux_rust(jobs(rows), run(7)))
        # A run that ended by itself, green or red, with every group row finished.
        done = [("Rust tests (ubuntu-24.04, workspace-lib)", "completed", "success"),
                ("Rust tests (ubuntu-24.04, standard)", "completed", "failure")]
        for conclusion in ("success", "failure"):
            with self.subTest(run=conclusion):
                self.assertTrue(pr_lookup.scheduled_linux_rust(jobs(done), run(7, status="completed",
                                                                               conclusion=conclusion)))

    def test_a_cancelled_or_unstarted_group_job_or_run_is_not_evidence(self):
        good = ("Rust tests (ubuntu-24.04, workspace-lib)", "completed", "success")
        # A group job that was cancelled, timed out, never started or ended in any other way, alone or
        # beside good rows: nothing completes Linux Rust, so the dispatch must run the full set.
        for conclusion in ("cancelled", "timed_out", "startup_failure", "neutral", "action_required", "stale", None):
            for rows in ([("Rust tests (ubuntu-24.04, standard)", "completed", conclusion)],
                         [good, ("Rust tests (ubuntu-24.04, standard)", "completed", conclusion)]):
                with self.subTest(job=conclusion, rows=len(rows)):
                    self.assertFalse(pr_lookup.scheduled_linux_rust(jobs(rows), run(7)))
        # A run that was cancelled (a newer push, a manual cancel), timed out or failed to start, whatever
        # its rows say; and a run in a state this code does not know.
        for status, conclusion in (("completed", "cancelled"), ("completed", "timed_out"),
                                   ("completed", "startup_failure"), ("completed", "skipped"),
                                   ("completed", "action_required"), ("completed", None), ("unknown", None),
                                   (None, None)):
            with self.subTest(run=(status, conclusion)):
                for rows in ([good], [("Rust tests (ubuntu-24.04, standard)", "queued", None)], READY_RUN_JOBS):
                    self.assertFalse(pr_lookup.scheduled_linux_rust(jobs(rows), run(7, status=status,
                                                                                    conclusion=conclusion)))
        # A row still queued or running in a run that has already finished cannot be on its way anywhere.
        for status in ("queued", "in_progress", "waiting"):
            with self.subTest(job_status=status):
                rows = [good, ("Rust tests (ubuntu-24.04, standard)", status, None)]
                self.assertFalse(pr_lookup.scheduled_linux_rust(jobs(rows), run(7, status="completed",
                                                                                conclusion="success")))
        # A job in a state this code does not know.
        self.assertFalse(pr_lookup.scheduled_linux_rust(
            jobs([("Rust tests (ubuntu-24.04, standard)", "unknown", None)]), run(7)))
        # Other jobs of the run may be cancelled or fail: only the Linux Rust group rows are the evidence.
        self.assertTrue(pr_lookup.scheduled_linux_rust(
            jobs([good, ("Rust tests (windows-2025, workspace-lib)", "completed", "cancelled"),
                  ("Python tests (ubuntu-24.04, shard 0)", "completed", "timed_out")]), run(7)))

    def test_a_cancelled_pull_request_run_gives_the_full_set_end_to_end(self):
        cancelled = [("Rust tests (ubuntu-24.04, workspace-lib)", "completed", "cancelled"),
                     ("Rust tests (ubuntu-24.04, standard)", "completed", "cancelled")]
        self.assertEqual(decide([run(1, status="completed", conclusion="cancelled")], {1: jobs(cancelled, 1)})[0],
                         "partial")
        self.assertEqual(decide([run(1, status="completed", conclusion="cancelled")],
                                {1: jobs(READY_RUN_JOBS, 1)})[0], "partial")
        # Superseded by a newer push to the same head is not possible (a new push is a new head); a rerun
        # after a cancel is the same run, latest attempt, and counts again once its rows are on their way.
        self.assertEqual(decide([run(1)], {1: jobs(READY_RUN_JOBS, 1)})[0], "whole-set")

    def test_anything_else_is_not_shown(self):
        cases = {
            "draft run (lint only)": DRAFT_RUN_JOBS,
            "pending run, no jobs": PENDING_RUN_JOBS,
            "only skipped rows": [("Rust tests (ubuntu-24.04, workspace-lib)", "completed", "skipped")],
            "unexpanded names": [("Rust tests (${{ matrix.os }}, ${{ matrix.group }})", "queued", None)],
            "windows rows only": [("Rust tests (windows-2025, workspace-lib)", "in_progress", None),
                                  ("Rust tests (windows-2025, native-store)", "queued", None)],
            "gate alone": [("Rust tests (ubuntu-24.04)", "completed", "success")],
            "limited group rows (rules-limited scope)": [("Rust tests (ubuntu-24.04, limited-model)", "queued", None),
                                                         ("Rust formatting and lint", "in_progress", None)],
            "eternal integration parts": [("Rust tests (ubuntu-24.04, eternal-integration-1)", "queued", None)],
            "detect still running": [("Detect changed paths", "in_progress", None)],
            "lookalike group": [("Rust tests (ubuntu-24.04, workspace-lib-x)", "queued", None)],
        }
        for name, rows in cases.items():
            with self.subTest(case=name):
                self.assertFalse(pr_lookup.scheduled_linux_rust(jobs(rows), run(7)))
        # Jobs of another run in the list are an error, never evidence.
        with self.assertRaises(ValueError):
            pr_lookup.scheduled_linux_rust(jobs(READY_RUN_JOBS, run_id=8), run(7))

    def test_the_groups_are_the_rust_test_groups_matrix_and_no_other_job_shares_them(self):
        text = WORKFLOW.read_text(encoding="utf-8")

        def block(name):
            body = text[text.index(f"\n  {name}:\n"):]
            return body[:body.index("\n    steps:\n")]

        def groups(name):
            matrix = block(name).split("        group:\n", 1)[1]
            return re.findall(r"^          - ([a-z0-9-]+)$", matrix.split("\n        exclude:", 1)[0], re.MULTILINE)
        self.assertEqual(tuple(groups("rust-test-groups")), pr_lookup.RUST_TEST_GROUPS)
        self.assertIn("    name: Rust tests (${{ matrix.os }}, ${{ matrix.group }})\n", block("rust-test-groups"))
        self.assertIn('"ubuntu-24.04"', block("rust-test-groups").split("        os: ", 1)[1].split("\n", 1)[0])
        for other in ("rust-limited-groups", "rust-eternal-integration-parts"):
            with self.subTest(job=other):
                self.assertTrue(groups(other))
                self.assertFalse(set(groups(other)) & set(pr_lookup.RUST_TEST_GROUPS))
        self.assertIn("    name: Rust tests (${{ matrix.os }})\n", block("rust-tests"))


class RunSelectionTests(unittest.TestCase):
    """Points 2 and 3: which run is the pull request's, and errors."""

    def test_the_newest_pull_request_run_at_the_commit_decides(self):
        ready, draft = jobs(READY_RUN_JOBS, 1), jobs(DRAFT_RUN_JOBS, 2)
        self.assertEqual(decide([run(1, "2026-11-24T10:00:00Z"), run(2, "2026-11-24T09:00:00Z")],
                                {1: ready, 2: draft})[0], "whole-set")
        self.assertEqual(decide([run(1, "2026-11-24T09:00:00Z"), run(2, "2026-11-24T10:00:00Z")],
                                {1: ready, 2: jobs(DRAFT_RUN_JOBS, 2)})[0], "partial")
        # Two newest runs that disagree: not shown.
        self.assertEqual(decide([run(1, "2026-11-24T10:00:00Z"), run(2, "2026-11-24T10:00:00Z")],
                                {1: ready, 2: draft})[0], "partial")

    def test_runs_of_other_events_forks_workflows_or_commits_never_count(self):
        ready = {9: jobs(READY_RUN_JOBS, 9)}
        for name, other in (("push", run(9, "2026-11-24T11:00:00Z", event="push")),
                            ("dispatch", run(9, "2026-11-24T11:00:00Z", event="workflow_dispatch")),
                            ("fork", run(9, "2026-11-24T11:00:00Z", head_repo="someone/fork")),
                            ("other workflow", run(9, "2026-11-24T11:00:00Z", path=".github/workflows/viewer.yml")),
                            ("other commit", run(9, "2026-11-24T11:00:00Z", sha="b" * 40))):
            with self.subTest(case=name):
                self.assertEqual(decide([other], ready)[0], "partial")
                # And a newer one never hides the pull request run.
                self.assertEqual(decide([other, run(1, "2026-11-24T10:00:00Z")],
                                        {**ready, 1: jobs(READY_RUN_JOBS, 1)})[0], "whole-set")
        self.assertEqual(decide([], {})[0], "partial")

    def test_the_lookup_matches_by_commit_event_and_workflow_and_reads_the_latest_attempt(self):
        result, paths = decide([run(1, "2026-11-24T10:00:00Z")], {1: jobs(READY_RUN_JOBS, 1)})
        self.assertEqual(result, "whole-set")
        self.assertEqual(paths, [f"/repos/{REPOSITORY}/actions/workflows/ci.yml/runs?event=pull_request&head_sha={HEAD}"
                                 "&per_page=100&page=1",
                                 f"/repos/{REPOSITORY}/actions/runs/1/jobs?filter=latest&per_page=100&page=1"])
        self.assertNotIn("branch", " ".join(paths))

    def test_pages_are_followed_and_must_add_up(self):
        many = [run(100 + i, "2026-11-24T08:00:00Z", event="push") for i in range(100)]
        self.assertEqual(decide(many + [run(1, "2026-11-24T10:00:00Z")], {1: jobs(READY_RUN_JOBS, 1)})[0], "whole-set")
        with self.assertRaises(ValueError):
            decide([run(1, "2026-11-24T10:00:00Z")], {1: jobs(READY_RUN_JOBS, 1)}, total_offset=1)

    def test_errors_never_give_whole_set(self):
        good = ([run(1, "2026-11-24T10:00:00Z")], {1: jobs(READY_RUN_JOBS, 1)})
        for kwargs, error in (({"fail": "/jobs"}, OSError), ({"fail": "/runs?"}, OSError),
                              ({"malformed": "/jobs"}, ValueError), ({"malformed": "/runs?"}, ValueError)):
            with self.subTest(kwargs=kwargs), self.assertRaises(error):
                decide(*good, **kwargs)
        with self.assertRaises(TimeoutError):
            decide(*good, budget=self_hosted_budget.Budget(requests=1))
        with self.assertRaises(ValueError):
            pr_lookup.pr_run(REPOSITORY, "main")
        # The CLI prints nothing when the lookup fails, and the verdict only at the end.
        out = io.StringIO()
        with mock.patch.dict(os.environ, {"GITHUB_REPOSITORY": REPOSITORY}), \
                mock.patch.object(pr_lookup, "api", FakeApi(*good, fail="/jobs")), contextlib.redirect_stdout(out), \
                self.assertRaises(OSError):
            pr_lookup.main(["pr-run", HEAD])
        self.assertEqual(out.getvalue(), "")
        with mock.patch.dict(os.environ, {"GITHUB_REPOSITORY": REPOSITORY}), \
                mock.patch.object(pr_lookup, "api", FakeApi(*good)), contextlib.redirect_stdout(out):
            self.assertEqual(pr_lookup.main(["pr-run", HEAD]), 0)
        self.assertEqual(out.getvalue().splitlines(), ["whole-set"])


class TokenTests(unittest.TestCase):
    """The lookup uses the job token and the job's existing read permissions; nothing new is granted."""

    def test_the_detect_job_permissions_are_unchanged_and_already_read_runs_and_jobs(self):
        text = WORKFLOW.read_text(encoding="utf-8")
        job = text[text.index("\n  detect-changes:\n"):text.index("\n  rust-quality:\n")]
        self.assertIn("    permissions:\n      actions: read\n      contents: read\n      pull-requests: read\n"
                      "    outputs:\n", job)
        self.assertEqual(text.count("    permissions:\n      actions: read\n      contents: read\n      pull-requests: read\n"), 1)
        step = job[job.index("      - name: Choose the group set"):job.index("      - name: Classify changed paths")]
        self.assertIn("GH_TOKEN: ${{ github.token }}", step)
        self.assertIn('pr_run=$(python3 tools/ci/pr_lookup.py pr-run "$GITHUB_SHA") || pr_run=', step)
        self.assertIn('if [ "$pr_run" = whole-set ] && rechecked=$(open_pr)', step)
        source = (WORKFLOW.parents[2] / "tools/ci/self_hosted_budget.py").read_text(encoding="utf-8")
        self.assertIn("/actions/runs/{run_id}/jobs?filter=latest", source)
        self.assertIn("run: python3 tools/ci/self_hosted_budget.py", job)
        # The same request helper (job token, read-only GET).
        self.assertIs(pr_lookup.self_hosted_budget._get, self_hosted_budget._get)


if __name__ == "__main__":
    unittest.main()
