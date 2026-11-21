"""A draft pull request runs only the light set; marking it ready runs the full pull request set."""
import re
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
                      "&& '-draft' || '' }}\n", text)
        self.assertIn("  cancel-in-progress: ${{ github.event_name == 'pull_request' }}\n", text)

    def test_a_draft_run_ends_the_full_runs_of_older_commits(self):
        body = jobs()["detect-changes"]
        step = body[body.index("      - name: Cancel this pull request's full runs at older commits"):]
        step = step[:step.index("\n\n")]
        self.assertIn("if: steps.scope.outputs.scope == 'draft' && github.event.pull_request.head.repo.full_name == "
                      "github.repository", step)
        self.assertIn('.head_sha != \\"$HEAD_SHA\\"', step)
        self.assertIn(".id != $GITHUB_RUN_ID", step)
        self.assertIn("continue-on-error: true", step)
        self.assertIn("      actions: write\n", body)

    def test_a_dispatch_runs_only_the_complement_of_a_ready_pull_request(self):
        body = jobs()["detect-changes"]
        self.assertIn("select(.head.repo.full_name == env.GITHUB_REPOSITORY and (.draft | not))", body)


if __name__ == "__main__":
    unittest.main()
