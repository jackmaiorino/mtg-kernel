"""A draft pull request runs only the light set; marking it ready runs the full pull request set."""
import json
import os
import re
import subprocess
import sys
import tempfile
import textwrap
import unittest
from pathlib import Path

WORKFLOW = Path(__file__).resolve().parents[3] / ".github" / "workflows" / "ci.yml"
BASH = "C:/Program Files/Git/bin/bash.exe" if sys.platform == "win32" else "bash"
CI_TOOLS = WORKFLOW.parents[2] / "tools/ci"
HEAD = "a" * 40
READY = {
    "state": "open", "draft": False, "number": 841, "base": {"sha": "b" * 40},
    "head": {"sha": HEAD, "ref": "test", "repo": {"full_name": "fixture/repo"}},
}


def jobs():
    text = WORKFLOW.read_text(encoding="utf-8")
    parts = re.split(r"^  ([a-z][a-z-]*):\n", text[text.index("\njobs:\n"):], flags=re.MULTILINE)
    return dict(zip(parts[1::2], parts[2::2]))


def condition(body):
    match = re.search(r"^    if: (.*)$", body, re.MULTILINE)
    return match.group(1) if match else ""


def workflow_step(step):
    block = jobs()["detect-changes"].split(f"      - name: {step}\n", 1)[1]
    run = re.search(r"^        run: \|\n((?:          .*\n|\n)+)", block, re.MULTILINE)
    return textwrap.dedent(run[1])


# Job lists of the pull request run at the dispatched commit (see test_pr_lookup.py for real ones).
LINUX_GROUPS = ("workspace-lib", "workspace-targets", "standard", "eternal", "cuda", "native-store", "trace",
                "saved-input", "limited-store")
WHOLE_SET_JOBS = [{"name": "Detect changed paths", "status": "completed", "conclusion": "success"},
                  *({"name": f"Rust tests (ubuntu-24.04, {group})", "status": "in_progress", "conclusion": None}
                    for group in LINUX_GROUPS),
                  {"name": "Rust tests (windows-2025, workspace-lib)", "status": "queued", "conclusion": None},
                  {"name": "Build Python test binaries (${{ matrix.os }})", "status": "completed", "conclusion": "skipped"},
                  {"name": "CI plan: rust=true python=false scope=pr checks=all", "status": "completed",
                   "conclusion": "success"}]
# The same run if its plan had said Python was relevant: the Python build rows are missing, so no complement.
PYTHON_MISSING_JOBS = WHOLE_SET_JOBS[:-1] + [{"name": "CI plan: rust=true python=true scope=pr checks=all",
                                              "status": "completed", "conclusion": "success"}]
DRAFT_JOBS = [{"name": "Detect changed paths", "status": "completed", "conclusion": "success"},
              {"name": "Rust formatting and lint", "status": "completed", "conclusion": "success"},
              {"name": "Rust tests (${{ matrix.os }}, ${{ matrix.group }})", "status": "completed", "conclusion": "skipped"},
              {"name": "CI plan: rust=false python=false scope=draft checks=all", "status": "completed",
               "conclusion": "success"}]


def pr_runs(event="pull_request", head=HEAD, repo="fixture/repo", number=841):
    return [{"id": 51, "event": event, "head_sha": head, "path": ".github/workflows/ci.yml",
             "created_at": "2026-11-24T10:00:00Z", "status": "in_progress", "conclusion": None,
             "repository": {"full_name": "fixture/repo"},
             "head_repository": {"full_name": repo}, "pull_requests": [{"number": number}]}]


def dispatch(repo, runs, jobs, *, after=READY, api_status=0, no_gh=False, malformed=False):
    """Run the real "Choose the group set" step for a dispatch without inputs at HEAD, with the ready pull request
    lookups (gh or pr_lookup.py) and the Actions listings (pr_lookup.py pr-run) answered from fixtures. Returns the
    scope output and whether the job list was read."""
    lookup = repo / "pr_lookup_fixture.py"
    lookup.write_text(textwrap.dedent('''
        import json
        import os
        import sys
        from pathlib import Path

        sys.path.insert(0, os.environ["CI_TEST_TOOLS"])
        import pr_lookup

        def get(path):
            assert "/pulls?state=open&head=" in path, path
            key = "CI_TEST_PR_AFTER" if Path("evidence-read").exists() else "CI_TEST_PR_BEFORE"
            return json.loads(os.environ[key])

        def api(path, budget):
            budget.spend()
            if os.environ["CI_TEST_API_STATUS"] != "0":
                raise OSError("fixture API failure")
            if "/actions/workflows/ci.yml/runs?event=pull_request&head_sha=" in path:
                rows, key = json.loads(os.environ["CI_TEST_RUNS"]), "workflow_runs"
            else:
                assert "/actions/runs/51/jobs?filter=latest&" in path, path
                Path("evidence-read").touch()
                rows = [dict(job, run_id=51, id=5100 + index)
                        for index, job in enumerate(json.loads(os.environ["CI_TEST_JOBS"]))]
                key = "jobs"
            if os.environ["CI_TEST_MALFORMED"] == "true":
                return {key: rows}
            return {key: rows, "total_count": len(rows)}

        pr_lookup.get = get
        pr_lookup.api = api
        raise SystemExit(pr_lookup.main(sys.argv[1:]))
        '''), encoding="utf-8")
    script = workflow_step("Choose the group set").replace(
        "python3 tools/ci/pr_lookup.py", '"$CI_TEST_PYTHON" -B "$CI_TEST_LOOKUP"')
    script = '''
command() {
  if [ "$1" = -v ] && [ "$2" = gh ] && [ "$CI_TEST_NO_GH" = true ]; then return 1; fi
  builtin command "$@"
}
gh() {
  [ "$CI_TEST_NO_GH" = false ] || return 127
  if [ -f evidence-read ]; then
    printf '%s' "$CI_TEST_PR_AFTER"
  else
    printf '%s' "$CI_TEST_PR_BEFORE"
  fi
}
''' + script
    output = repo / "scope"
    output.unlink(missing_ok=True)
    (repo / "evidence-read").unlink(missing_ok=True)
    env = dict(os.environ, GITHUB_OUTPUT=output.as_posix(), GITHUB_STEP_SUMMARY=(repo / "summary").as_posix(),
               EVENT="workflow_dispatch", WINDOWS_RUNNER="", FULL_MATRIX="false", FALLBACK_SCOPE="", INPUT_SCOPE="",
               GITHUB_REPOSITORY="fixture/repo", GITHUB_REPOSITORY_OWNER="fixture", GITHUB_REF_NAME="test",
               GITHUB_SHA=HEAD, GH_TOKEN="unused", CI_TEST_PYTHON=Path(sys.executable).as_posix(),
               CI_TEST_TOOLS=CI_TOOLS.as_posix(), CI_TEST_LOOKUP=lookup.as_posix(),
               CI_TEST_PR_BEFORE=json.dumps([READY]), CI_TEST_PR_AFTER=json.dumps([after]),
               CI_TEST_RUNS=json.dumps(runs), CI_TEST_JOBS=json.dumps(jobs), CI_TEST_API_STATUS=str(api_status),
               CI_TEST_MALFORMED=str(malformed).lower(), CI_TEST_NO_GH=str(no_gh).lower())
    subprocess.run([BASH, "-eo", "pipefail", "-c", script], cwd=repo, env=env, check=True, capture_output=True)
    return output.read_text(encoding="utf-8").strip(), (repo / "evidence-read").is_file()


class DraftLightTests(unittest.TestCase):
    def test_changed_path_gates_preserve_git_paths(self):
        classify = workflow_step("Classify changed paths").replace("${{ github.event_name }}", "pull_request")
        checks = workflow_step("Choose the checks").replace(
            "python3 tools/ci/ci_scope.py", '"$CI_TEST_PYTHON" "$CI_TEST_SCOPE"'
        )
        cases = [
            ("tools/café.py", "docs/moved.txt", "true", "false", "python"),
            ("docs/before.txt", "tools/file with spaces.py", "true", "false", "python"),
            ("python/tests/café.py", None, "true", "false", "python"),
            ("docs/café.md", "docs/updated.md", "false", "false", "docs"),
            ("docs/before.txt", "mtg-kernel/src/bin/kernel_limited_café.rs", "false", "true", "rules-limited"),
            ("tools/café.py", "mtg-kernel/src/bin/kernel_limited_café.rs", "true", "true", "all"),
        ]
        if sys.platform != "win32":
            cases.append(('tools/"quoted"\nname.py', None, "true", "false", "python"))

        for before, after, python, rust, scope in cases:
            with self.subTest(before=before, after=after), tempfile.TemporaryDirectory() as scratch:
                repo = Path(scratch)
                env = os.environ.copy()
                env.pop("GIT_CONFIG_COUNT", None)

                def git(*args):
                    git_options = ["-c", "user.name=CI fixture", "-c", "user.email=fixture@example.invalid",
                                   "-c", "commit.gpgsign=false", "-c", f"core.hooksPath={repo / 'no-hooks'}"]
                    subprocess.run(["git", *git_options, *args], cwd=repo, env=env, check=True)

                git("init", "--quiet")
                git("config", "core.quotePath", "true")
                git("config", "diff.renames", "true")
                (repo / before).parent.mkdir(parents=True, exist_ok=True)
                (repo / before).write_text("example\n", encoding="utf-8")
                git("add", "--", before)
                git("commit", "--quiet", "-m", "Add example")
                if after is None:
                    git("rm", "--quiet", "--", before)
                else:
                    (repo / after).parent.mkdir(parents=True, exist_ok=True)
                    git("mv", "--", before, after)
                git("commit", "--quiet", "-m", "Change example")

                output = repo / "outputs"
                env.update(RUNNER_TEMP=repo.as_posix(), GITHUB_OUTPUT=output.as_posix(),
                           GITHUB_STEP_SUMMARY=(repo / "summary").as_posix(), INPUT_SCOPE="", EVENT="pull_request",
                           SCOPE="pr", LABELS="[]", CI_TEST_PYTHON=Path(sys.executable).as_posix(),
                           CI_TEST_SCOPE=(WORKFLOW.parents[2] / "tools/ci/ci_scope.py").as_posix())
                for rendered in (classify, checks):
                    subprocess.run([BASH, "-eo", "pipefail", "-c", rendered], cwd=repo, env=env, check=True,
                                   capture_output=True)

                self.assertEqual(output.read_text(encoding="utf-8").splitlines(),
                                 [f"python_relevant={python}", f"rust_relevant={rust}", f"checks={scope}"])

    def test_a_dispatch_complements_only_what_the_pull_request_run_scheduled(self):
        # Point 3 with the real step: the complement needs the pull request run's own Linux Rust group jobs.
        with tempfile.TemporaryDirectory() as scratch:
            repo = Path(scratch)
            for no_gh in (False, True):
                with self.subTest(no_gh=no_gh):
                    self.assertEqual(dispatch(repo, pr_runs(), WHOLE_SET_JOBS, no_gh=no_gh), ("scope=windows-complement", True))
                    self.assertEqual(dispatch(repo, pr_runs(), DRAFT_JOBS, no_gh=no_gh), ("scope=full", True))
                    self.assertEqual(dispatch(repo, pr_runs(), PYTHON_MISSING_JOBS, no_gh=no_gh), ("scope=full", True))
                    self.assertEqual(dispatch(repo, pr_runs(number=842), WHOLE_SET_JOBS, no_gh=no_gh),
                                     ("scope=full", False))
                    self.assertEqual(dispatch(repo, pr_runs(), [], no_gh=no_gh), ("scope=full", True))
                    for runs in ([], pr_runs(event="push"), pr_runs(repo="someone/fork"), pr_runs(head="b" * 40)):
                        self.assertEqual(dispatch(repo, runs, WHOLE_SET_JOBS, no_gh=no_gh), ("scope=full", False))
                    self.assertEqual(dispatch(repo, pr_runs(), WHOLE_SET_JOBS, api_status=1, no_gh=no_gh),
                                     ("scope=full", False))
                    self.assertEqual(dispatch(repo, pr_runs(), WHOLE_SET_JOBS, malformed=True, no_gh=no_gh),
                                     ("scope=full", False))

    def test_manual_dispatch_rechecks_ready_identity_after_the_evidence_with_both_backends(self):
        cases = {
            "same tuple": (READY, "windows-complement"),
            "different PR": ({**READY, "number": 842}, "full"),
            "moved head": ({**READY, "head": {**READY["head"], "sha": "c" * 40}}, "full"),
            "moved base": ({**READY, "base": {"sha": "c" * 40}}, "full"),
            "became draft": ({**READY, "draft": True}, "full"),
            "closed": ({**READY, "state": "closed"}, "full"),
        }
        with tempfile.TemporaryDirectory() as scratch:
            for no_gh in (False, True):
                for case, (after, expected) in cases.items():
                    with self.subTest(case=case, no_gh=no_gh):
                        self.assertEqual(dispatch(Path(scratch), pr_runs(), WHOLE_SET_JOBS, after=after, no_gh=no_gh),
                                         (f"scope={expected}", True))

    def test_marking_a_draft_ready_starts_a_run(self):
        text = WORKFLOW.read_text(encoding="utf-8")
        self.assertIn("    types: [opened, synchronize, reopened, ready_for_review]\n", text)

    def test_the_draft_scope_needs_a_draft_without_the_full_label(self):
        body = jobs()["detect-changes"]
        self.assertIn("DRAFT: ${{ github.event.pull_request.draft }}", body)
        self.assertIn("FULL_LABEL: ${{ contains(github.event.pull_request.labels.*.name, 'ci:full') }}", body)
        self.assertIn('if [ "$EVENT" = pull_request ] && [ "$DRAFT" = true ] && [ "$FULL_LABEL" != true ]; then\n'
                      "            # Draft", body)
        self.assertIn("            scope=draft\n", body)

    def test_only_the_lint_job_runs_for_a_draft(self):
        for name, body in jobs().items():
            cond = condition(body)
            with self.subTest(job=name):
                if name in ("detect-changes", "ci-plan"):
                    # ci-plan runs in every run (a draft's too): one short hosted job that only names the plan.
                    self.assertEqual(cond, "")
                elif name == "rust-quality":
                    self.assertIn("""contains(fromJSON('["full", "pr", "draft"]'), needs.detect-changes.outputs.scope)""", cond)
                elif name == "hosted-fallback":
                    self.assertIn("needs.detect-changes.result == 'success' && "
                                  "needs.detect-changes.outputs.scope != 'draft'", cond)
                else:
                    self.assertIn("needs.detect-changes.outputs.scope != 'draft'", cond)

    def test_a_draft_run_has_its_own_concurrency_group(self):
        # Same predicate as the draft scope: a light run never cancels or replaces a full run.
        text = WORKFLOW.read_text(encoding="utf-8")
        self.assertIn("  group: ci-${{ github.workflow }}-${{ github.ref }}${{ github.event_name == 'pull_request' && "
                      "github.event.pull_request.draft && !contains(github.event.pull_request.labels.*.name, 'ci:full') "
                      "&& '-draft' || '' }}", text)
        self.assertIn("  cancel-in-progress: ${{ github.event_name == 'pull_request' }}\n", text)

    def test_a_dispatch_runs_only_the_complement_of_a_ready_pull_request(self):
        body = jobs()["detect-changes"]
        self.assertIn("python3 tools/ci/pr_lookup.py ready-pr", body)


if __name__ == "__main__":
    unittest.main()
