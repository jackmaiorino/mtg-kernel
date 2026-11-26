"""The dispatch step's lookups: the ready pull request, and whether that pull request's own run says and shows that
it scheduled everything the Windows complement leaves to it (pr-run), decided from the run's actual jobs."""
import contextlib
import io
import json
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
NUMBER = 841

# Real job lists (names, status, conclusion; latest attempt). No real run has a plan job yet: the plan row is
# added by hand where a case needs it.
# Run 45502283040 (a ready Rust-only pull request, 2026-11-24, in progress; Python skipped by relevance).
RUST_ONLY_RUN_JOBS = [
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
# Run 45506288001 (pull request 955, in progress; Rust and Python scheduled).
PYTHON_RUN_JOBS = [
    ('Python tools unit tests (ubuntu-24.04)', 'completed', 'skipped'),
    ('Trainer library tests (ubuntu-24.04, shard ${{ matrix.shard }})', 'completed', 'skipped'),
    ('Detect changed paths', 'completed', 'success'),
    ('Build Python test binaries (ubuntu-24.04)', 'in_progress', None),
    ('Build Python test binaries (windows-2025)', 'in_progress', None),
    ('Build shared Eternal integration tests (ubuntu-24.04)', 'in_progress', None),
    ('Build shared Limited tests (ubuntu-24.04, binaries)', 'in_progress', None),
    ('Build shared Limited tests (ubuntu-24.04, default-list)', 'in_progress', None),
    ('Build shared Limited tests (ubuntu-24.04, tests)', 'in_progress', None),
    ('Rust formatting and lint', 'in_progress', None),
    ('Rust tests (ubuntu-24.04, cuda)', 'in_progress', None),
    ('Rust tests (ubuntu-24.04, eternal)', 'in_progress', None),
    ('Rust tests (ubuntu-24.04, limited-store)', 'in_progress', None),
    ('Rust tests (ubuntu-24.04, native-store)', 'in_progress', None),
    ('Rust tests (ubuntu-24.04, saved-input)', 'in_progress', None),
    ('Rust tests (ubuntu-24.04, standard)', 'in_progress', None),
    ('Rust tests (ubuntu-24.04, trace)', 'in_progress', None),
    ('Rust tests (ubuntu-24.04, workspace-lib)', 'in_progress', None),
    ('Rust tests (ubuntu-24.04, workspace-targets)', 'in_progress', None),
    ('Rust tests (windows-2025, native-store)', 'in_progress', None),
    ('Rust tests (windows-2025, native-store-1)', 'in_progress', None),
    ('Rust tests (windows-2025, workspace-lib)', 'in_progress', None),
    ('Rust tests (windows-2025, workspace-targets)', 'in_progress', None),
]
# Run 45504569745 (pull request 958's first commit, cancelled: expanded cancelled Linux Rust rows beside unexpanded cancelled dependent matrices).
CANCELLED_RUN_JOBS = [
    ('Build Python test binaries (ubuntu-24.04)', 'completed', 'cancelled'),
    ('Build Python test binaries (windows-2025)', 'completed', 'cancelled'),
    ('Build shared Eternal integration tests (ubuntu-24.04)', 'completed', 'cancelled'),
    ('Build shared Limited tests (ubuntu-24.04, binaries)', 'completed', 'cancelled'),
    ('Build shared Limited tests (ubuntu-24.04, default-list)', 'completed', 'cancelled'),
    ('Build shared Limited tests (ubuntu-24.04, tests)', 'completed', 'cancelled'),
    ('Eternal integration inventory (${{ matrix.os }})', 'completed', 'cancelled'),
    ('Hosted fallback for self-hosted admission timeouts', 'completed', 'cancelled'),
    ('Python tests (${{ matrix.os }}, shard ${{ matrix.shard }})', 'completed', 'cancelled'),
    ('Report a red post-merge run', 'completed', 'cancelled'),
    ('Rust formatting and lint', 'completed', 'cancelled'),
    ('Rust tests (${{ matrix.os }}, ${{ matrix.group }})', 'completed', 'cancelled'),
    ('Rust tests (${{ matrix.os }}, ${{ matrix.group }})', 'completed', 'cancelled'),
    ('Rust tests (ubuntu-24.04, cuda)', 'completed', 'cancelled'),
    ('Rust tests (ubuntu-24.04, eternal)', 'completed', 'cancelled'),
    ('Rust tests (ubuntu-24.04, limited-store)', 'completed', 'cancelled'),
    ('Rust tests (ubuntu-24.04, native-store)', 'completed', 'cancelled'),
    ('Rust tests (ubuntu-24.04, saved-input)', 'completed', 'cancelled'),
    ('Rust tests (ubuntu-24.04, standard)', 'completed', 'cancelled'),
    ('Rust tests (ubuntu-24.04, trace)', 'completed', 'cancelled'),
    ('Rust tests (ubuntu-24.04, workspace-lib)', 'completed', 'cancelled'),
    ('Rust tests (ubuntu-24.04, workspace-targets)', 'completed', 'cancelled'),
    ('Rust tests (windows-2025)', 'completed', 'cancelled'),
    ('Rust tests (windows-2025, native-store)', 'completed', 'cancelled'),
    ('Rust tests (windows-2025, native-store-1)', 'completed', 'cancelled'),
    ('Rust tests (windows-2025, workspace-lib)', 'completed', 'cancelled'),
    ('Rust tests (windows-2025, workspace-targets)', 'completed', 'cancelled'),
    ('Rust tests (ubuntu-24.04)', 'completed', 'failure'),
    ('Python tools unit tests (ubuntu-24.04)', 'completed', 'skipped'),
    ('Trainer library tests (ubuntu-24.04, shard ${{ matrix.shard }})', 'completed', 'skipped'),
    ('Detect changed paths', 'completed', 'success'),
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
PENDING_RUN_JOBS = []   # run 45500437409, waiting in its concurrency group: no jobs at all


def plan(rust="true", python="false", scope="pr", checks="all", status="completed", conclusion="success"):
    return (f"CI plan: rust={rust} python={python} scope={scope} checks={checks}", status, conclusion)


RUST_ONLY = RUST_ONLY_RUN_JOBS + [plan()]
WITH_PYTHON = PYTHON_RUN_JOBS + [plan(python="true")]
# The same run finished: every job succeeded or was skipped.
DONE = [(n, "completed", "success") if s != "completed" else (n, s, c) for n, s, c in WITH_PYTHON]


def pull(number, sha, repo=REPOSITORY, draft=False, ref=REF):
    return {"number": number, "state": "open", "draft": draft, "base": {"sha": BASE},
            "head": {"sha": sha, "ref": ref, "repo": {"full_name": repo} if repo else None}}


def jobs(rows, run_id=7):
    return [{"id": 1000 * run_id + index, "name": name, "status": status, "conclusion": conclusion, "run_id": run_id}
            for index, (name, status, conclusion) in enumerate(rows)]


def run(run_id, created="2026-11-24T10:00:00Z", event="pull_request", sha=HEAD, head_repo=REPOSITORY,
        path=".github/workflows/ci.yml", status="in_progress", conclusion=None, numbers=(NUMBER,), repo=REPOSITORY):
    return {"id": run_id, "event": event, "head_sha": sha, "path": path, "created_at": created, "status": status,
            "conclusion": conclusion, "repository": {"full_name": repo}, "head_repository": {"full_name": head_repo},
            "pull_requests": [{"number": number} for number in numbers]}


def replace(rows, name, status, conclusion):
    return [(n, status, conclusion) if n == name else (n, s, c) for n, s, c in rows]


class FakeApi:
    """Answers the two listings with pages of 100; hooks change a page to test pagination and errors."""

    def __init__(self, runs, jobs_by_run, fail=None, total_offset=0, malformed=None, page_hook=None):
        self.runs, self.jobs, self.fail, self.offset, self.malformed, self.hook, self.paths = \
            runs, jobs_by_run, fail, total_offset, malformed, page_hook, []

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
        if self.hook:
            body = self.hook(key, page, body)
        if self.malformed and self.malformed in path:
            body = {"message": "unexpected"}
        return body


def decide(runs, jobs_by_run, budget=None, number=NUMBER, **kwargs):
    fake = FakeApi(runs, jobs_by_run, **kwargs)
    with mock.patch.object(pr_lookup, "api", fake):
        return pr_lookup.pr_run(REPOSITORY, HEAD, number, budget), fake.paths


def whole(rows, the_run=None):
    return pr_lookup.scheduled_whole_set(jobs(rows), the_run or run(7))


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


class PlanTests(unittest.TestCase):
    """Rules 1 and 2: exactly one plan job, in the exact format, concluded success, saying rust, pr and all."""

    def test_a_rust_only_run_with_its_plan_shows_the_whole_set(self):
        self.assertTrue(whole(RUST_ONLY))
        self.assertTrue(whole(WITH_PYTHON))

    def test_a_missing_duplicated_or_unfinished_plan_is_not_evidence(self):
        cases = {"missing": RUST_ONLY_RUN_JOBS, "duplicated": RUST_ONLY + [plan()],
                 "duplicated, other values": RUST_ONLY + [plan(python="true")]}
        for status, conclusion in (("completed", "skipped"), ("completed", "cancelled"), ("completed", "failure"),
                                   ("queued", None), ("in_progress", None)):
            cases[f"{status}/{conclusion}"] = RUST_ONLY_RUN_JOBS + [plan(status=status, conclusion=conclusion)]
        for name, rows in cases.items():
            with self.subTest(case=name):
                self.assertFalse(whole(rows))

    def test_a_malformed_plan_name_is_not_evidence(self):
        for name in ("CI plan: python=false rust=true scope=pr checks=all",
                     "CI plan: rust=true python=false scope=pr checks=all extra=1",
                     "CI plan: rust=True python=false scope=pr checks=all",
                     "CI plan: rust=yes python=false scope=pr checks=all",
                     "CI plan: rust=true python=false scope=PR checks=all",
                     "CI plan: rust=true python=false scope=pr checks=all ",
                     "CI plan: rust=true python=false scope=pr_x checks=all",
                     "CI plan: rust=${{ needs.detect-changes.outputs.rust_relevant }} python=${{ "
                     "needs.detect-changes.outputs.python_relevant }} scope=${{ needs.detect-changes.outputs.scope }} "
                     "checks=${{ needs.detect-changes.outputs.checks }}"):
            with self.subTest(name=name):
                self.assertFalse(whole(RUST_ONLY_RUN_JOBS + [(name, "completed", "success")]))

    def test_the_plan_must_say_rust_pr_and_all(self):
        for kwargs in ({"rust": "false"}, {"scope": "draft"}, {"scope": "windows-complement"}, {"scope": "full"},
                       {"checks": "trainer"}, {"checks": "rules-limited"}, {"checks": "docs"}):
            with self.subTest(plan=kwargs):
                self.assertFalse(whole(RUST_ONLY_RUN_JOBS + [plan(**kwargs)]))

    def test_the_plan_job_name_in_ci_yml_is_the_format_the_parser_reads(self):
        text = WORKFLOW.read_text(encoding="utf-8")
        block = text[text.index("\n  ci-plan:\n"):]
        block = block[:block.index("\n\n  ")]
        (name,) = re.findall(r'^    name: "(.+)"$', block, re.MULTILINE)
        outputs = {"rust_relevant": "true", "python_relevant": "false", "scope": "pr", "checks": "all"}
        expanded = re.sub(r"\$\{\{ needs\.detect-changes\.outputs\.([a-z_]+) \}\}", lambda m: outputs[m.group(1)], name)
        self.assertEqual(expanded, "CI plan: rust=true python=false scope=pr checks=all")
        self.assertTrue(pr_lookup.PLAN.fullmatch(expanded))
        self.assertTrue(whole(RUST_ONLY_RUN_JOBS + [(expanded, "completed", "success")]))
        # The outputs it reads exist, it needs only detect-changes, and nothing needs it.
        detect = text[text.index("\n  detect-changes:\n"):text.index("\n  ci-plan:\n")]
        for output in outputs:
            self.assertIn(f"\n      {output}: ", detect)
        self.assertIn("    needs: detect-changes\n    runs-on: ubuntu-24.04\n", block)
        self.assertNotRegex(block, r"(?m)^    if:")
        self.assertEqual(re.findall(r"(?m)^    needs: .*ci-plan.*$", text), [])


class EvidenceTests(unittest.TestCase):
    """Rules 3 and 4, and the health of the run and of every evidence job."""

    def test_rows_finished_or_on_their_way_show_the_whole_set(self):
        for status, conclusion in (("queued", None), ("in_progress", None), ("waiting", None),
                                   ("completed", "success")):
            with self.subTest(status=status, conclusion=conclusion):
                rows = replace(RUST_ONLY, "Rust tests (ubuntu-24.04, workspace-targets)", status, conclusion)
                self.assertTrue(whole(rows))
        self.assertTrue(whole(DONE, run(7, status="completed", conclusion="success")))
        # A failed row is no longer evidence: the run is not green so far.
        self.assertFalse(whole(replace(RUST_ONLY, "Rust tests (ubuntu-24.04, workspace-targets)", "completed", "failure")))

    def test_a_cancelled_or_unstarted_run_or_evidence_job_is_not_evidence(self):
        for name in ("Rust tests (ubuntu-24.04, workspace-targets)", "Build Python test binaries (windows-2025)"):
            for status, conclusion in (("completed", "cancelled"), ("completed", "timed_out"),
                                       ("completed", "startup_failure"), ("completed", "neutral")):
                with self.subTest(job=name, conclusion=conclusion):
                    self.assertFalse(whole(replace(WITH_PYTHON, name, status, conclusion)))
        for status, conclusion in (("completed", "cancelled"), ("completed", "timed_out"),
                                   ("completed", "startup_failure"), ("weird", None)):
            with self.subTest(run=(status, conclusion)):
                self.assertFalse(whole(WITH_PYTHON, run(7, status=status, conclusion=conclusion)))
        # A finished run with a row still queued: that row will never run.
        self.assertFalse(whole(RUST_ONLY, run(7, status="completed", conclusion="success")))
        # Run 45504569745: cancelled Linux Rust rows beside unexpanded cancelled dependent matrices.
        self.assertFalse(whole(CANCELLED_RUN_JOBS + [plan(python="true")], run(7, status="completed",
                                                                                 conclusion="cancelled")))
        self.assertFalse(whole(CANCELLED_RUN_JOBS + [plan(python="true")]))

    def test_python_rows_follow_the_plan(self):
        ubuntu, windows = pr_lookup.PYTHON_BUILD_JOBS
        # python=true needs both build rows, healthy.
        for name, rows in {"missing": [r for r in WITH_PYTHON if "Python test binaries" not in r[0]],
                           "skipped": replace(replace(WITH_PYTHON, ubuntu, "completed", "skipped"), windows,
                                              "completed", "skipped"),
                           "cancelled": replace(WITH_PYTHON, windows, "completed", "cancelled"),
                           "one system only": [r for r in WITH_PYTHON if r[0] != windows],
                           "unexpanded only": RUST_ONLY_RUN_JOBS + [plan(python="true")]}.items():
            with self.subTest(python="true", case=name):
                self.assertFalse(whole(rows))
        # python=false: the build rows were skipped (the Rust-only fixture); rows that ran disagree with the plan.
        self.assertTrue(whole(RUST_ONLY))
        for status, conclusion in (("completed", "cancelled"), ("in_progress", None)):
            with self.subTest(python="false", row=(status, conclusion)):
                self.assertFalse(whole(RUST_ONLY + [(ubuntu, status, conclusion)]))

    def test_without_linux_rust_group_rows_nothing_is_shown(self):
        cases = {
            "draft run (lint only)": DRAFT_RUN_JOBS + [plan(scope="draft")],
            "pending run, no jobs": PENDING_RUN_JOBS + [plan()],
            "only skipped rows": [("Rust tests (ubuntu-24.04, workspace-targets)", "completed", "skipped"), plan()],
            "unexpanded names": [("Rust tests (${{ matrix.os }}, ${{ matrix.group }})", "queued", None), plan()],
            "windows rows only": [("Rust tests (windows-2025, workspace-targets)", "in_progress", None), plan()],
            "gate alone": [("Rust tests (ubuntu-24.04)", "completed", "success"), plan()],
            "limited group rows": [("Rust tests (ubuntu-24.04, limited-model)", "queued", None), plan()],
            "eternal integration parts": [("Rust tests (ubuntu-24.04, eternal-integration-1)", "queued", None), plan()],
            "lookalike group": [("Rust tests (ubuntu-24.04, workspace-targets-x)", "queued", None), plan()],
            "workspace library build and parts": [
                ("Build shared workspace library tests (ubuntu-24.04)", "completed", "success"),
                ("Rust tests (ubuntu-24.04, workspace-lib)", "queued", None),
                ("Rust tests (ubuntu-24.04, workspace-lib-1)", "queued", None),
                ("Rust tests (ubuntu-24.04, workspace-lib-2)", "queued", None),
                ("Rust tests (ubuntu-24.04, workspace-lib-3)", "queued", None), plan()],
        }
        for name, rows in cases.items():
            with self.subTest(case=name):
                self.assertFalse(whole(rows))

    def test_each_guard_decides_alone(self):
        # Fixtures that qualify in every respect but one, so only that guard decides.
        finished = run(7, status="completed", conclusion="success")
        self.assertTrue(whole(DONE, finished))
        # (a) A completed run that did not succeed, although every job listed succeeded or was skipped.
        self.assertFalse(whole(DONE, run(7, status="completed", conclusion="failure")))
        # (b) The plan's status must be completed (a queued plan row that already carries a success conclusion).
        rows = [(n, "queued", c) if n.startswith("CI plan") else (n, s, c) for n, s, c in RUST_ONLY]
        self.assertFalse(whole(rows))
        # (c) A job with a status this code does not know, in an otherwise qualifying run.
        self.assertFalse(whole(RUST_ONLY + [("Some new job", "mystery", None)]))
        self.assertTrue(whole(RUST_ONLY + [("Some new job", "queued", None)]))

    def test_any_failed_job_disqualifies_the_run(self):
        # A failed prerequisite leaves its dependents unscheduled: GitHub skips the Python shards when one Python
        # build fails, the Limited groups when the Limited build fails, the Eternal parts and inventory when the
        # Eternal build fails. So does any other failed, cancelled or timed-out job.
        ubuntu, windows = pr_lookup.PYTHON_BUILD_JOBS
        failed_python = replace(DONE, windows, "completed", "failure")
        failed_python = replace(failed_python, "Python tests (${{ matrix.os }}, shard ${{ matrix.shard }})",
                                "completed", "skipped")
        cases = {
            "failed Python build": failed_python,
            "failed Limited build": replace(DONE, "Build shared Limited tests (ubuntu-24.04, tests)", "completed",
                                            "failure"),
            "failed Eternal build": replace(DONE, "Build shared Eternal integration tests (ubuntu-24.04)", "completed",
                                            "failure"),
            "failed leaf test": DONE + [("Python tests (ubuntu-24.04, shard 0)", "completed", "failure")],
            "failed unrelated family": replace(DONE, "Rust formatting and lint", "completed", "failure"),
            "cancelled job": replace(DONE, "Rust formatting and lint", "completed", "cancelled"),
            "timed out job": replace(DONE, "Rust formatting and lint", "completed", "timed_out"),
            "action required": replace(DONE, "Rust formatting and lint", "completed", "action_required"),
            "neutral": replace(DONE, "Rust formatting and lint", "completed", "neutral"),
            "startup failure": replace(DONE, "Rust formatting and lint", "completed", "startup_failure"),
        }
        for name, rows in cases.items():
            for the_run in (run(7), run(7, status="completed", conclusion="success")):
                with self.subTest(case=name, run=the_run["status"]):
                    self.assertFalse(whole(rows, the_run))
        self.assertIn(("Rust formatting and lint", "completed", "success"), DONE)

    def test_a_red_conclusion_disqualifies_whatever_the_status_says(self):
        # A job reported as queued or running that already carries a red conclusion: contradictory, and red.
        ubuntu, windows = pr_lookup.PYTHON_BUILD_JOBS
        for status in ("in_progress", "queued", "waiting"):
            for conclusion in ("failure", "cancelled", "timed_out", "neutral", "action_required", "stale",
                               "startup_failure"):
                with self.subTest(status=status, conclusion=conclusion):
                    self.assertFalse(whole(replace(DONE, windows, status, conclusion), run(7)))
                    self.assertFalse(whole(replace(DONE, "Rust formatting and lint", status, conclusion), run(7)))
        # The same rows without a conclusion are jobs on their way, which is fine while the run is going.
        for status in ("in_progress", "queued", "waiting"):
            with self.subTest(status=status, conclusion=None):
                self.assertTrue(whole(replace(DONE, windows, status, None), run(7)))

    def test_every_linux_rust_group_row_must_be_present(self):
        for group in pr_lookup.LINUX_RUST_GROUPS:
            with self.subTest(missing=group):
                rows = [r for r in RUST_ONLY if r[0] != f"Rust tests (ubuntu-24.04, {group})"]
                self.assertFalse(whole(rows))
                self.assertFalse(whole(replace(RUST_ONLY, f"Rust tests (ubuntu-24.04, {group})", "completed",
                                               "skipped")))

    def test_a_rerun_of_the_plan_job_alone(self):
        # Rerunning one job: the latest attempt lists that job's new run and every other job's last run. While the
        # plan is queued again there is no plan; once it succeeds the run qualifies again.
        rerun = [(n, "queued", None) if n.startswith("CI plan") else (n, s, c) for n, s, c in DONE]
        self.assertFalse(whole(rerun, run(7, status="queued")))
        self.assertTrue(whole(DONE, run(7, status="completed", conclusion="success")))

    def test_every_job_record_is_validated_first(self):
        for bad in ({"id": 1, "name": None, "status": "queued", "conclusion": None, "run_id": 7},
                    {"id": 1, "name": "x", "status": None, "conclusion": None, "run_id": 7},
                    {"id": 1, "name": "x", "status": "queued", "conclusion": 3, "run_id": 7},
                    {"id": 1, "name": "x", "status": "queued", "conclusion": None, "run_id": 8}, "not a record"):
            with self.subTest(bad=bad), self.assertRaises(ValueError):
                # The malformed record comes after rows that already show everything.
                pr_lookup.scheduled_whole_set(jobs(RUST_ONLY) + [bad], run(7))

    def test_the_groups_and_python_rows_are_ci_yml_s(self):
        text = WORKFLOW.read_text(encoding="utf-8")

        def block(name):
            body = text[text.index(f"\n  {name}:\n"):]
            return body[:body.index("\n    steps:\n")]

        def groups(name):
            matrix = block(name).split("        group:\n", 1)[1]
            return re.findall(r"^          - ([a-z0-9-]+)$", matrix.split("\n        exclude:", 1)[0], re.MULTILINE)
        self.assertEqual(tuple(groups("rust-test-groups")), pr_lookup.RUST_TEST_GROUPS)
        # The pull request set's exclude list drops exactly native-store-1 on Linux.
        exclude = block("rust-test-groups").split("        exclude: ", 1)[1].split("\n", 1)[0]
        pr_branch = exclude.split("needs.detect-changes.outputs.scope == 'windows-pr') && '", 1)[1].split("'", 1)[0]
        linux_excluded = [row["group"] for row in json.loads(pr_branch) if row["os"] == "ubuntu-24.04"]
        self.assertEqual(linux_excluded, ["native-store-1"])
        self.assertEqual(pr_lookup.LINUX_RUST_GROUPS,
                         tuple(g for g in pr_lookup.RUST_TEST_GROUPS if g not in linux_excluded))
        self.assertIn("    name: Rust tests (${{ matrix.os }}, ${{ matrix.group }})\n", block("rust-test-groups"))
        self.assertIn('"ubuntu-24.04"', block("rust-test-groups").split("        os: ", 1)[1].split("\n", 1)[0])
        for other in ("rust-limited-groups", "rust-eternal-integration-parts", "rust-workspace-lib-parts"):
            with self.subTest(job=other):
                self.assertTrue(groups(other))
                self.assertFalse(set(groups(other)) & set(pr_lookup.RUST_TEST_GROUPS))
        self.assertIn("    name: Rust tests (${{ matrix.os }})\n", block("rust-tests"))
        python = block("python-env-build")
        self.assertIn("    name: Build Python test binaries (${{ matrix.os }})\n", python)
        self.assertIn("""os: ${{ fromJSON(contains(fromJSON('["full", "pr"]'), needs.detect-changes.outputs.scope) """
                      """&& '["ubuntu-24.04", "windows-2025"]' || '["windows-2025"]') }}""", python)
        self.assertEqual(pr_lookup.PYTHON_BUILD_JOBS, tuple(f"Build Python test binaries ({os_})"
                                                            for os_ in ("ubuntu-24.04", "windows-2025")))


class RunSelectionTests(unittest.TestCase):
    """Which run is the pull request's, pagination and errors."""

    def test_the_newest_pull_request_run_at_the_commit_decides(self):
        ready, draft = jobs(RUST_ONLY, 1), jobs(DRAFT_RUN_JOBS + [plan(scope="draft")], 2)
        self.assertEqual(decide([run(1, "2026-11-24T10:00:00Z"), run(2, "2026-11-24T09:00:00Z")],
                                {1: ready, 2: draft})[0], "whole-set")
        self.assertEqual(decide([run(1, "2026-11-24T09:00:00Z"), run(2, "2026-11-24T10:00:00Z")],
                                {1: ready, 2: draft})[0], "partial")
        # Two newest runs that disagree: not shown.
        self.assertEqual(decide([run(1), run(2)], {1: ready, 2: draft})[0], "partial")

    def test_runs_of_other_events_repositories_forks_workflows_commits_or_pull_requests_never_count(self):
        ready = {9: jobs(RUST_ONLY, 9)}
        for name, other in (("push", run(9, event="push")), ("dispatch", run(9, event="workflow_dispatch")),
                            ("fork", run(9, head_repo="someone/fork")), ("other repository", run(9, repo="else/where")),
                            ("other workflow", run(9, path=".github/workflows/viewer.yml")),
                            ("other commit", run(9, sha="b" * 40)), ("other pull request", run(9, numbers=(842,))),
                            ("no pull request", run(9, numbers=()))):
            with self.subTest(case=name):
                self.assertEqual(decide([other], ready)[0], "partial")
                # And a newer one never hides the pull request run.
                newer = dict(other, created_at="2026-11-24T11:00:00Z")
                self.assertEqual(decide([newer, run(1)], {**ready, 1: jobs(RUST_ONLY, 1)})[0], "whole-set")
        self.assertEqual(decide([], {})[0], "partial")
        # Two open pull requests sharing a head: each gets only its own run's evidence.
        self.assertEqual(decide([run(1, numbers=(841, 842))], {1: jobs(RUST_ONLY, 1)}, number=842)[0], "whole-set")

    def test_the_lookup_matches_by_commit_event_and_workflow_and_reads_the_latest_attempt(self):
        result, paths = decide([run(1)], {1: jobs(RUST_ONLY, 1)})
        self.assertEqual(result, "whole-set")
        self.assertEqual(paths, [f"/repos/{REPOSITORY}/actions/workflows/ci.yml/runs?event=pull_request&head_sha={HEAD}"
                                 "&per_page=100&page=1",
                                 f"/repos/{REPOSITORY}/actions/runs/1/jobs?filter=latest&per_page=100&page=1"])
        self.assertNotIn("branch", " ".join(paths))

    def test_a_rerun_of_failed_jobs_is_judged_on_the_latest_attempt(self):
        # filter=latest after a rerun of failed jobs: the rerun group rows are on their way, the rest kept from
        # attempt 1 (finished). The API returns only those rows; attempt 1's failed rows are not listed.
        latest = replace(RUST_ONLY, "Rust tests (ubuntu-24.04, workspace-targets)", "queued", None)
        latest = [(n, "completed", "success") if s == "in_progress" else (n, s, c) for n, s, c in latest]
        self.assertEqual(decide([run(1, status="queued")], {1: jobs(latest, 1)})[0], "whole-set")
        self.assertEqual(decide([run(1, status="completed", conclusion="failure")], {1: jobs(latest, 1)})[0], "partial")

    def test_pages_are_followed_and_must_be_consistent(self):
        many = [run(100 + i, "2026-11-24T08:00:00Z", event="push") for i in range(100)]
        self.assertEqual(decide(many + [run(1)], {1: jobs(RUST_ONLY, 1)})[0], "whole-set")
        with self.assertRaises(ValueError):
            decide([run(1)], {1: jobs(RUST_ONLY, 1)}, total_offset=1)
        # A run inserted between page reads shifts page 2: one run is listed twice, the newest is missing, and the
        # count still matches.
        def shifted(key, page, body):
            if key == "workflow_runs" and page == 2:
                return dict(body, workflow_runs=[many[-1]])
            return body
        with self.assertRaises(ValueError):
            decide(many + [run(1)], {1: jobs(RUST_ONLY, 1)}, page_hook=shifted)
        # A total_count that changes between pages (the last page's count matches the rows read).
        def recount(key, page, body):
            return dict(body, total_count=body["total_count"] + (key == "workflow_runs" and page == 1))
        with self.assertRaises(ValueError):
            decide(many + [run(1)], {1: jobs(RUST_ONLY, 1)}, page_hook=recount)
        # A listing capped at 1,000 of 1,001 records (GitHub returns at most 1,000 results): an error.
        capped = [run(2000 + i, "2026-11-24T08:00:00Z", event="push") for i in range(1000)] + [run(1)]
        def cap(key, page, body):
            return dict(body, workflow_runs=[]) if key == "workflow_runs" and page > 10 else body
        with self.assertRaises(ValueError):
            decide(capped, {1: jobs(RUST_ONLY, 1)}, page_hook=cap, budget=self_hosted_budget.Budget(requests=40))
        # A job page that fails, and a job listing that runs past one page.
        long_jobs = jobs(RUST_ONLY + [(f"Extra job {i}", "completed", "success") for i in range(100)], 1)
        self.assertEqual(decide([run(1)], {1: long_jobs})[0], "whole-set")
        def broken_jobs(key, page, body):
            return {"message": "unexpected"} if key == "jobs" and page == 2 else body
        with self.assertRaises(ValueError):
            decide([run(1)], {1: long_jobs}, page_hook=broken_jobs)

    def test_errors_and_late_answers_never_give_whole_set(self):
        good = ([run(1)], {1: jobs(RUST_ONLY, 1)})
        for kwargs, error in (({"fail": "/jobs"}, OSError), ({"fail": "/runs?"}, OSError),
                              ({"malformed": "/jobs"}, ValueError), ({"malformed": "/runs?"}, ValueError)):
            with self.subTest(kwargs=kwargs), self.assertRaises(error):
                decide(*good, **kwargs)
        with self.assertRaises(TimeoutError):
            decide(*good, budget=self_hosted_budget.Budget(requests=1))
        # The deadline passes during the last response: no verdict.
        budget = self_hosted_budget.Budget(seconds=60.0, requests=20)
        def late(key, page, body):
            if key == "jobs":
                budget.deadline -= 3600
            return body
        with self.assertRaises(TimeoutError):
            decide(*good, budget=budget, page_hook=late)
        for sha, number in (("main", NUMBER), (HEAD, 0), (HEAD, True)):
            with self.subTest(sha=sha, number=number), self.assertRaises(ValueError):
                pr_lookup.pr_run(REPOSITORY, sha, number)
        # The CLI prints nothing when the lookup fails, and the verdict only at the end.
        out = io.StringIO()
        with mock.patch.dict(os.environ, {"GITHUB_REPOSITORY": REPOSITORY}), \
                mock.patch.object(pr_lookup, "api", FakeApi(*good, fail="/jobs")), contextlib.redirect_stdout(out), \
                self.assertRaises(OSError):
            pr_lookup.main(["pr-run", HEAD, str(NUMBER)])
        self.assertEqual(out.getvalue(), "")
        with mock.patch.dict(os.environ, {"GITHUB_REPOSITORY": REPOSITORY}), \
                mock.patch.object(pr_lookup, "api", FakeApi(*good)), contextlib.redirect_stdout(out):
            self.assertEqual(pr_lookup.main(["pr-run", HEAD, str(NUMBER)]), 0)
            self.assertEqual(pr_lookup.main(["pr-run", HEAD]), 2)
        self.assertEqual(out.getvalue().splitlines(), ["whole-set"])


class TokenTests(unittest.TestCase):
    """The lookup uses the job token and the job's existing read permissions; nothing new is granted."""

    def test_the_detect_job_permissions_are_unchanged_and_already_read_runs_and_jobs(self):
        text = WORKFLOW.read_text(encoding="utf-8")
        job = text[text.index("\n  detect-changes:\n"):text.index("\n  ci-plan:\n")]
        self.assertIn("    permissions:\n      actions: read\n      contents: read\n      pull-requests: read\n"
                      "    outputs:\n", job)
        step = job[job.index("      - name: Choose the group set"):job.index("      - name: Classify changed paths")]
        self.assertIn("GH_TOKEN: ${{ github.token }}", step)
        self.assertIn('pr_run=$(python3 tools/ci/pr_lookup.py pr-run "$GITHUB_SHA" "$pr_number") || pr_run=', step)
        self.assertIn('if [ "$pr_run" = whole-set ] && rechecked=$(open_pr)', step)
        source = (WORKFLOW.parents[2] / "tools/ci/self_hosted_budget.py").read_text(encoding="utf-8")
        self.assertIn("/actions/runs/{run_id}/jobs?filter=latest", source)
        self.assertIn("run: python3 tools/ci/self_hosted_budget.py", job)
        plan_job = text[text.index("\n  ci-plan:\n"):text.index("\n  rust-quality:\n")]
        self.assertIn("    permissions: {}\n", plan_job)


if __name__ == "__main__":
    unittest.main()
