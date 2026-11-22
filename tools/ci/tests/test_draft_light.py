"""A draft pull request runs only the light set; marking it ready runs the full pull request set."""
import os
import re
import subprocess
import sys
import tempfile
import textwrap
import unittest
from pathlib import Path

WORKFLOW = Path(__file__).resolve().parents[3] / ".github" / "workflows" / "ci.yml"


def jobs():
    text = WORKFLOW.read_text(encoding="utf-8")
    parts = re.split(r"^  ([a-z][a-z-]*):\n", text[text.index("\njobs:\n"):], flags=re.MULTILINE)
    return dict(zip(parts[1::2], parts[2::2]))


def condition(body):
    match = re.search(r"^    if: (.*)$", body, re.MULTILINE)
    return match.group(1) if match else ""


class DraftLightTests(unittest.TestCase):
    def test_changed_path_gates_preserve_git_paths(self):
        detect = jobs()["detect-changes"]

        def script(step):
            block = detect.split(f"      - name: {step}\n", 1)[1]
            run = re.search(r"^        run: \|\n((?:          .*\n|\n)+)", block, re.MULTILINE)
            return textwrap.dedent(run[1])

        classify = script("Classify changed paths").replace("${{ github.event_name }}", "pull_request")
        checks = script("Choose the checks").replace(
            "python3 tools/ci/ci_scope.py", '"$CI_TEST_PYTHON" "$CI_TEST_SCOPE"'
        )
        bash = "C:/Program Files/Git/bin/bash.exe" if sys.platform == "win32" else "bash"
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
                    subprocess.run([bash, "-eo", "pipefail", "-c", rendered], cwd=repo, env=env, check=True,
                                   capture_output=True)

                self.assertEqual(output.read_text(encoding="utf-8").splitlines(),
                                 [f"python_relevant={python}", f"rust_relevant={rust}", f"checks={scope}"])

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
        self.assertIn("select(.head.repo.full_name == env.GITHUB_REPOSITORY and (.draft | not))", body)


if __name__ == "__main__":
    unittest.main()
