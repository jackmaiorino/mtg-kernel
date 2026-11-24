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


def dispatch(repo, pages, *, after=READY, api_status=0, no_gh=False):
    lookup = repo / "pr_lookup_fixture.py"
    lookup.write_text(textwrap.dedent('''
        import json
        import os
        import sys
        from pathlib import Path

        sys.path.insert(0, os.environ["CI_TEST_TOOLS"])
        import pr_lookup

        def get(path):
            marker = Path("inventory-read")
            if "/pulls?state=open&head=" in path:
                key = "CI_TEST_PR_AFTER" if marker.exists() else "CI_TEST_PR_BEFORE"
                return json.loads(os.environ[key])
            assert path == "repos/fixture/repo/pulls/841/files?per_page=100&page=1", path
            marker.touch()
            if os.environ["CI_TEST_API_STATUS"] != "0":
                raise OSError("fixture API failure")
            [page] = json.loads(os.environ["CI_TEST_PAGES"])
            return page

        pr_lookup.get = get
        raise SystemExit(pr_lookup.main(sys.argv[1:]))
        '''), encoding="utf-8")
    script = workflow_step("Choose the group set").replace(
        "python3 tools/ci/ci_scope.py", '"$CI_TEST_PYTHON" "$CI_TEST_TOOLS/ci_scope.py"'
    ).replace("python3 tools/ci/pr_lookup.py", '"$CI_TEST_PYTHON" -B "$CI_TEST_LOOKUP"')
    script = '''
command() {
  if [ "$1" = -v ] && [ "$2" = gh ] && [ "$CI_TEST_NO_GH" = true ]; then return 1; fi
  builtin command "$@"
}
gh() {
  [ "$CI_TEST_NO_GH" = false ] || return 127
  if [ "$2" = --paginate ]; then
    [ "$3" = --slurp ] && [ "$4" = repos/fixture/repo/pulls/841/files ] || return 1
    touch inventory-read
    printf '%s' "$CI_TEST_PAGES"
    return "$CI_TEST_API_STATUS"
  fi
  if [ -f inventory-read ]; then
    printf '%s' "$CI_TEST_PR_AFTER"
  else
    printf '%s' "$CI_TEST_PR_BEFORE"
  fi
}
''' + script
    output = repo / "scope"
    output.unlink(missing_ok=True)
    (repo / "inventory-read").unlink(missing_ok=True)
    env = dict(os.environ, GITHUB_OUTPUT=output.as_posix(), GITHUB_STEP_SUMMARY=(repo / "summary").as_posix(),
               EVENT="workflow_dispatch", WINDOWS_RUNNER="", FULL_MATRIX="false", FALLBACK_SCOPE="", INPUT_SCOPE="",
               GITHUB_REPOSITORY="fixture/repo", GITHUB_REPOSITORY_OWNER="fixture", GITHUB_REF_NAME="test",
               GITHUB_SHA=HEAD, CI_TEST_PYTHON=Path(sys.executable).as_posix(), CI_TEST_TOOLS=CI_TOOLS.as_posix(),
               CI_TEST_LOOKUP=lookup.as_posix(), CI_TEST_PR_BEFORE=json.dumps([READY]), CI_TEST_PR_AFTER=json.dumps([after]),
               CI_TEST_PAGES=pages, CI_TEST_API_STATUS=str(api_status), CI_TEST_NO_GH=str(no_gh).lower())
    subprocess.run([BASH, "-eo", "pipefail", "-c", script], cwd=repo, env=env, check=True, capture_output=True)
    return output.read_text(encoding="utf-8").strip(), (repo / "inventory-read").is_file()


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
            # The website runs nothing of ci.yml alone or with docs; with other files they decide.
            ("viewer/public/café.html", "README.md", "false", "false", "viewer"),
            ("viewer/src/main.tsx", None, "false", "false", "viewer"),
            ("viewer/index.html", "mtg-kernel/src/engine.rs", "false", "true", "all"),
            ("viewer/index.html", "python/tools/café.py", "true", "false", "all"),
            ("viewer_tools/café.py", None, "false", "true", "all"),
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

    def test_manual_dispatch_uses_the_same_paths_as_pull_request_checks(self):
        rename = {"status": "renamed", "filename": "docs/moved.md", "previous_filename": "tools/stage4a/café.py"}
        cases = [
            ([[{"status": "added", "filename": "tools/stage4a/café\nstep.py"}]], "tools", "full"),
            ([[rename]], "tools", "full"),
            ([[rename], [{"status": "modified", "filename": "python/tests/test café.py"}]], "all", "windows-complement"),
            ([[{"status": "added", "filename": " tools/stage4a/café.py"}]], "all", "windows-complement"),
            ([[{"status": "added", "filename": "README.md "}]], "all", "windows-complement"),
        ]
        with tempfile.TemporaryDirectory() as scratch:
            repo = Path(scratch)
            for pages, expected_checks, expected_scope in cases:
                with self.subTest(pages=pages):
                    self.assertEqual(dispatch(repo, json.dumps(pages)), (f"scope={expected_scope}", True))
                    paths = [f[key] for page in pages for f in page for key in ("filename", "previous_filename") if key in f]
                    checks = subprocess.run([sys.executable, CI_TOOLS / "ci_scope.py", "choose"], cwd=repo, check=True,
                                            capture_output=True, env=dict(os.environ, EVENT="pull_request", SCOPE="pr",
                                                                         LABELS="[]", GITHUB_OUTPUT=""),
                                            input=("\0".join(paths) + "\0").encode("utf-8"))
                    self.assertTrue(checks.stdout.startswith(f"checks: {expected_checks}".encode("utf-8")))

            for payload in ("{", '[[{"status":"renamed","filename":"docs/moved.md"}]]',
                            '[[{"status":"added","filename":"Cargo.lock"}],null]'):
                with self.subTest(malformed=payload):
                    self.assertEqual(dispatch(repo, payload), ("scope=full", True))
            self.assertEqual(dispatch(repo, '[[{"status":"added","filename":"Cargo.lock"}]]', api_status=1),
                             ("scope=full", True))

    def test_manual_dispatch_rechecks_ready_identity_after_files_with_both_backends(self):
        full_scope_files = '[[{"status":"modified","filename":"Cargo.lock"}]]'
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
                        self.assertEqual(dispatch(Path(scratch), full_scope_files, after=after, no_gh=no_gh),
                                         (f"scope={expected}", True))

    def test_manual_dispatch_with_no_gh_keeps_full_scope_on_inventory_errors(self):
        with tempfile.TemporaryDirectory() as scratch:
            repo = Path(scratch)
            self.assertEqual(dispatch(repo, '[[{"status":"modified","filename":"Cargo.lock"}]]', api_status=1, no_gh=True),
                             ("scope=full", True))
            for status in ("", "unexpected", None):
                with self.subTest(status=status):
                    pages = json.dumps([[{"status": status, "filename": "Cargo.lock"}]])
                    self.assertEqual(dispatch(repo, pages, no_gh=True), ("scope=full", True))

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
                if name == "detect-changes":
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
