"""The release line's scope table, workflow and docs/CI-SCOPES.md agree; changes map as the lead ruled."""
import re
import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT / "tools" / "ci"))
import ci_scope  # noqa: E402

WORKFLOW = ROOT / ".github" / "workflows" / "ci.yml"


def jobs():
    text = WORKFLOW.read_text(encoding="utf-8")
    parts = re.split(r"^  ([a-z][a-z-]*):\n", text[text.index("\njobs:\n"):], flags=re.MULTILINE)
    out = {}
    for name, body in zip(parts[1::2], parts[2::2]):
        match = re.search(r"^    if: (.*)$", body, re.MULTILINE)
        out[name] = match.group(1) if match else ""
    return out


class TableTests(unittest.TestCase):
    def test_the_docs_table_is_generated_from_the_scope_table(self):
        self.assertEqual((ROOT / "docs" / "CI-SCOPES.md").read_text(encoding="utf-8"), ci_scope.markdown())

    def test_the_jobs_run_for_the_right_checks(self):
        conditions = jobs()
        self.assertIn("""contains(fromJSON('["all", "draft", "release-trainer"]'), needs.detect-changes.outputs.checks)""",
                      conditions["rust-quality"])
        for job in ("rust-test-groups", "limited-build", "rust-limited-groups", "rust-tests", "python-tests"):
            with self.subTest(job=job):
                self.assertIn("needs.detect-changes.outputs.checks == 'all'", conditions[job])
        self.assertEqual(conditions["trainer-tests"], "needs.detect-changes.outputs.checks == 'release-trainer'")

    def test_every_merge_into_a_release_line_runs_the_full_matrix(self):
        text = WORKFLOW.read_text(encoding="utf-8")
        self.assertIn("      - 'release/**'\n", text)
        self.assertIn("types: [opened, synchronize, reopened, ready_for_review]", text)

    def test_the_filters_cover_every_qualifying_shared_module_in_full(self):
        for module in ("native_policy_value_net_v1::", "native_flat_tensorizer_v", "fast_sampler::",
                       "unclamped_softmax_sampler_v1::", "native_opponent_sampler_v1::", "sideboard_play_policy_v1::",
                       "rl_session::", "expanded_deck_training_v1::"):
            self.assertIn(module, ci_scope.TRAINER_FILTERS)
        src = ROOT / "mtg-kernel" / "src"
        names = {p.stem for p in src.glob("*.rs")} | {p.name for p in src.iterdir() if p.is_dir()}
        for prefix in ci_scope.TRAINER_FILTERS:
            with self.subTest(filter=prefix):
                stem = prefix.rstrip(":")
                self.assertTrue(any(n == stem or n.startswith(stem) for n in names), prefix)

    def test_the_feature_matrix_is_the_table(self):
        text = WORKFLOW.read_text(encoding="utf-8")
        (line,) = [l for l in text.splitlines() if l.strip().startswith("features: [")]
        self.assertEqual(re.findall(r"'([^']*)'", line), ci_scope.FEATURE_SETS)
        self.assertEqual(ci_scope.scope_of("docs/reports/x/pipeline.py"), None)
        self.assertEqual(ci_scope.scope_of_files(["docs/reports/x/pipeline.py", "mtg-kernel/src/rl_session.rs"]), "pr")

    def test_the_record_names_the_mandatory_rows(self):
        record = ci_scope.trainer_record()
        for row in ci_scope.BIT_IDENTITY_ROWS:
            self.assertIn(row, record)
        self.assertIn("field absent", record)
        self.assertIn("not mergeable", record)


class ScopeTests(unittest.TestCase):
    def test_trainer_and_qualifying_shared_files(self):
        files = ["mtg-kernel/src/expanded_deck_training_v1.rs",
                 "mtg-kernel/src/expanded_deck_training_v1/public_features/replay_audit/tests.rs",
                 "mtg-kernel/src/sideboard_play_policy_v1.rs", "mtg-kernel/src/rl_session.rs",
                 "mtg-kernel/src/native_policy_value_net_v1.rs", "mtg-kernel/src/native_policy_train_step_v1/weighted_v3.rs",
                 "docs/NOTE.md"]
        self.assertEqual(ci_scope.scope_of_files(files), "release-trainer")

    def test_engine_card_data_and_other_files_run_the_whole_set(self):
        for other in ("mtg-kernel/src/engine.rs", "mtg-kernel/src/eternal_cards_v1.rs", "data/x.json", "Cargo.lock",
                      "mtg-kernel/src/lib.rs", "mtg-kernel/src/phase1_registry_transfer_v1/cli.rs",
                      ".github/workflows/ci.yml", "python/tools/run_rust_lib_filters.py", "tools/w5/chains.py"):
            with self.subTest(file=other):
                self.assertEqual(ci_scope.scope_of_files(["mtg-kernel/src/rl_session.rs", other]), "pr")

    def test_choose(self):
        trainer = ["mtg-kernel/src/expanded_deck_training_v1.rs"]
        self.assertEqual(ci_scope.choose("pull_request", "pr", "", [], trainer), "release-trainer")
        self.assertEqual(ci_scope.choose("pull_request", "pr", "", [], trainer, merge_commits=1), "all")
        self.assertEqual(ci_scope.choose("pull_request", "pr", "", ["ci:full"], trainer), "all")
        self.assertEqual(ci_scope.choose("pull_request", "pr", "", ["ci:release-trainer"], ["mtg-kernel/src/engine.rs"]), "all")
        self.assertEqual(ci_scope.choose("push", "full", "", [], trainer), "all")
        self.assertEqual(ci_scope.choose("workflow_dispatch", "full", "release-trainer", [], []), "release-trainer")
        self.assertEqual(ci_scope.choose("workflow_dispatch", "full", "", [], []), "all")


class MonitorTests(unittest.TestCase):
    def test_the_comment_mentions_no_one(self):
        import post_merge_report as report
        body, line = report.compose("o/r", "u", "abcdef1234", "release/mull-c5719e1cf", "push", ["x by @y"], {"number": 7})
        self.assertNotIn("@y", body + line)
        self.assertIn("#7", line)


if __name__ == "__main__":
    unittest.main()
