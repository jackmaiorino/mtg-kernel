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

    def test_the_job_rows_are_the_table(self):
        text = WORKFLOW.read_text(encoding="utf-8")
        job = text[text.index("  trainer-tests:\n"):]
        job = job[:job.index("    runs-on:")]
        (line,) = [l for l in job.splitlines() if l.strip().startswith("features: [")]
        linux = re.findall(r"'([^']*)'", line)
        windows = re.findall(r"- os: windows-2025\n\s+features: '([^']*)'", job)
        self.assertIn("os: [ubuntu-24.04]", job)
        self.assertEqual([("linux", f) for f in linux] + [("windows", f) for f in windows], ci_scope.TRAINER_ROWS)
        self.assertEqual(ci_scope.scope_of("docs/reports/x/pipeline.py"), None)
        self.assertEqual(ci_scope.scope_of_files(["docs/reports/x/pipeline.py", "mtg-kernel/src/rl_session.rs"]), "pr")

    def test_every_selected_test_runs_in_some_row_or_is_windows_only_with_features(self):
        import rust_test_cfg
        names = [name for name, _, _ in rust_test_cfg.all_tests() if rust_test_cfg.selected(name)]
        self.assertGreater(len(names), 1000)
        for name, file, windows_only in rust_test_cfg.uncovered():
            with self.subTest(test=name, file=file):
                self.assertTrue(windows_only, "no release-trainer row compiles this test")
        self.assertEqual(rust_test_cfg.check_coverage(), 0)
        # The Standard row must not rerun default tests whose fixtures Standard rejects.
        standard = rust_test_cfg.feature_only("standard-magezero-fixtures")
        self.assertIn("native_training_store_run_v2::tests::standard_fixture_refuses_unadmitted_training_profile",
                      standard)
        self.assertNotIn("native_training_store_run_v2::tests::valid_fixture_roundtrips_and_exposes_authority",
                         standard)

    def test_the_record_names_the_mandatory_rows(self):
        record = ci_scope.trainer_record()
        for row in ci_scope.BIT_IDENTITY_ROWS:
            self.assertIn(row, record)
        self.assertIn("field absent", record)
        self.assertIn("not mergeable", record)


class CfgPredictorTests(unittest.TestCase):
    def predicted(self, files, features, target="linux"):
        import tempfile
        import rust_test_cfg
        with tempfile.TemporaryDirectory() as tmp:
            for rel, text in files.items():
                path = Path(tmp) / rel
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text(text, encoding="utf-8")
            tests = []
            rust_test_cfg.scan_file("m", Path(tmp) / "m.rs", [], tests)
            enabled = frozenset(f for f in features.split(",") if f)
            return sorted(name for name, cfgs, _ in tests
                          if all(rust_test_cfg.evaluate(c, enabled, target) for c in cfgs))

    def test_conjunctions_negations_and_enclosing_modules(self):
        files = {
            "m.rs": """
                #[cfg(feature = "a")]
                mod gated;
                #[cfg(test)]
                mod tests {
                    const BRACES: &str = r#"{ } }"#;
                    #[test]
                    fn plain() { let c = '}'; }
                    #[test]
                    #[cfg(all(feature = "a", feature = "b"))]
                    fn both() {}
                    #[test]
                    #[cfg(all(feature = "a", not(feature = "b")))]
                    fn a_without_b() {}
                    #[cfg(windows)]
                    mod win {
                        #[test]
                        fn only_windows() {}
                    }
                    /* } */
                    #[test]
                    fn after() {}
                }
            """,
            "m/gated.rs": '#[test]\nfn inside() {}\n#[path = "other.rs"]\nmod moved;\n'
                          'mod spliced {\n    include!("spliced_tests.rs");\n}\n',
            "m/spliced_tests.rs": "#[test]\nfn from_include() {}\n",
            "m/other.rs": "#[test]\nfn moved_test() {}\n",
        }
        self.assertEqual(self.predicted(files, ""), ["m::tests::after", "m::tests::plain"])
        self.assertEqual(self.predicted(files, "a"), ["m::gated::inside", "m::gated::moved::moved_test",
                                                       "m::gated::spliced::from_include", "m::tests::a_without_b", "m::tests::after", "m::tests::plain"])
        self.assertEqual(self.predicted(files, "a,b"), ["m::gated::inside", "m::gated::moved::moved_test",
                                                         "m::gated::spliced::from_include", "m::tests::after", "m::tests::both", "m::tests::plain"])
        self.assertIn("m::tests::win::only_windows", self.predicted(files, "", "windows"))

    def test_an_exemption_needs_a_predicate_no_linux_build_satisfies(self):
        import rust_test_cfg
        tree = lambda text: rust_test_cfg.parse_cfg(rust_test_cfg.tokens(text))[0]
        both = [tree('all(feature = "limited-fdn-fixtures", feature = "native-training-store-v2-production")')]
        self.assertTrue(rust_test_cfg.satisfiable_on(both, "linux"))
        windows = [tree('all(windows, feature = "limited-fdn-fixtures")')]
        self.assertFalse(rust_test_cfg.satisfiable_on(windows, "linux"))
        self.assertTrue(rust_test_cfg.satisfiable_on(windows, "windows"))
        implied = [tree('all(feature = "limited-fdn-fixtures", not(feature = "standard-magezero-fixtures"))')]
        self.assertTrue(rust_test_cfg.satisfiable_on(implied, "linux"))


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
    def test_the_monitor_runs_on_a_failed_or_timed_out_dependency(self):
        condition = jobs()["post-merge-report"]
        self.assertIn("contains(needs.*.result, 'failure') || contains(needs.*.result, 'cancelled')", condition)

    def test_the_comment_mentions_no_one(self):
        import post_merge_report as report
        body, line = report.compose("o/r", "u", "abcdef1234", "release/mull-c5719e1cf", "push", ["x by @y"], {"number": 7})
        self.assertNotIn("@y", body + line)
        self.assertIn("#7", line)


if __name__ == "__main__":
    unittest.main()
