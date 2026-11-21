"""The scope table, the workflow and docs/CI-SCOPES.md agree; changes map to the scopes the merge rule sets."""
import re
import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT / "tools" / "ci"))
import ci_scope  # noqa: E402

WORKFLOW = ROOT / ".github" / "workflows" / "ci.yml"


def workflow():
    """{"jobs": {job id: {"if": its job-level condition}}} without a YAML parser (none is in the test env)."""
    text = WORKFLOW.read_text(encoding="utf-8")
    parts = re.split(r"^  ([a-z][a-z-]*):\n", text[text.index("\njobs:\n"):], flags=re.MULTILINE)
    jobs = {}
    for name, body in zip(parts[1::2], parts[2::2]):
        match = re.search(r"^    if: (.*)$", body, re.MULTILINE)
        jobs[name] = {"if": match.group(1) if match else ""}
    return {"jobs": jobs}


class TableTests(unittest.TestCase):
    def test_the_docs_table_is_generated_from_the_scope_table(self):
        self.assertEqual((ROOT / "docs" / "CI-SCOPES.md").read_text(encoding="utf-8"), ci_scope.markdown())

    def test_every_job_a_scope_names_exists_and_runs_only_for_its_scopes(self):
        jobs = workflow()["jobs"]
        for name, spec in ci_scope.SCOPES.items():
            for job in spec["jobs"]:
                if name == "pr":
                    continue
                with self.subTest(scope=name, job=job):
                    self.assertIn(job, jobs)
                    condition = str(jobs[job].get("if", ""))
                    self.assertIn("needs.detect-changes.outputs.checks", condition)
                    self.assertIn(name, condition)

    def test_the_whole_set_jobs_run_for_all_and_never_for_an_unrelated_scope(self):
        jobs = workflow()["jobs"]
        for job in ("rust-test-groups", "rust-tests"):
            self.assertIn("needs.detect-changes.outputs.checks == 'all'", jobs[job]["if"])
        for job in ("python-env-build", "python-tests"):
            self.assertIn("""contains(fromJSON('["all", "python"]'), needs.detect-changes.outputs.checks)""", jobs[job]["if"])
        for job in ("limited-build", "rust-limited-groups"):
            self.assertIn("""contains(fromJSON('["all", "rules-limited"]'), needs.detect-changes.outputs.checks)""", jobs[job]["if"])

    def test_the_trainer_filters_name_existing_modules(self):
        src = ROOT / "mtg-kernel" / "src"
        names = {p.stem for p in src.glob("*.rs")} | {p.name for p in src.iterdir() if p.is_dir()}
        for prefix in ci_scope.TRAINER_FILTERS:
            with self.subTest(filter=prefix):
                stem = prefix.rstrip(":")
                self.assertTrue(any(n == stem or n.startswith(stem) for n in names), prefix)

    def test_the_tools_commands_are_the_python_tests_job_commands(self):
        text = WORKFLOW.read_text(encoding="utf-8")
        block = text[text.index("\n  python-tests:\n"):text.index("\n  trainer-tests:\n")]
        joined = re.sub(r"\\\n\s+", "", block)
        for command in ci_scope.TOOLS_COMMANDS:
            with self.subTest(command=command):
                self.assertIn(command.replace("cd python/tools/tests && ", "").split(" && ")[-1], joined)

    def test_the_trainer_record_names_every_bit_identity_row_and_the_merge_condition(self):
        record = ci_scope.trainer_record()
        for row in ci_scope.BIT_IDENTITY_ROWS:
            self.assertIn(row, record)
        self.assertIn("znver5", record)
        self.assertIn("not mergeable", record)


class ScopeTests(unittest.TestCase):
    def scope(self, *paths):
        return ci_scope.scope_of_files(list(paths))

    def test_single_scope_changes(self):
        self.assertEqual(self.scope("tools/w5/chains.py", "tools/w5/tests/test_w5_chains.py", "docs/W5.md"), "w5-tools")
        for read_by_python_tests in ("tools/spy_reset/panel.py", "tools/spy_reset/pilot.py",
                                     "python/tools/nine_deck_public_panel_v1.py", "tools/ex1/target_ref.py"):
            self.assertEqual(self.scope(read_by_python_tests), "python")
        self.assertEqual(self.scope("tools/stage4a/report.py"), "tools")
        self.assertEqual(self.scope("tools/reach/plan.py"), "python")          # its tests are in python/tests
        self.assertEqual(self.scope("python/tests/test_x.py", "python/tools/foo.py"), "python")
        self.assertEqual(self.scope("mtg-kernel/src/native_expanded_training_run_v1.rs",
                                    "mtg-kernel/src/bin/native_expanded_training_run_v1.rs"), "trainer")
        self.assertEqual(self.scope("mtg-kernel/src/bin/kernel_limited_match_v1.rs"), "rules-limited")
        self.assertEqual(self.scope("python/tools/check_limited_common_client_v1.py"), "python")
        self.assertEqual(self.scope(".github/workflows/viewer.yml"), "workflow")
        self.assertEqual(self.scope("docs/a.md", "README.md"), "docs")

    def test_shared_files_make_a_trainer_change_mixed(self):
        for shared in ("mtg-kernel/src/sideboard_play_policy_v1.rs", "mtg-kernel/src/rl_session.rs",
                       "mtg-kernel/src/rl_session/pauper_london_v1/action_windows_tests.rs",
                       "mtg-kernel/src/native_policy_value_net_v1.rs", "mtg-kernel/src/native_flat_tensorizer_v2.rs",
                       "mtg-kernel/src/fast_sampler.rs", "mtg-kernel/src/unclamped_softmax_sampler_v1.rs",
                       "mtg-kernel/src/expanded_deck_training_v1/public_features/opponent_kind/tests.rs",
                       "mtg-kernel/src/priority_action_windows_v1.rs", "mtg-kernel/src/london_mulligan_v1.rs",
                       "mtg-kernel/src/expanded_deck_training_v1.rs",
                       "mtg-kernel/src/native_policy_train_step_v1/weighted_v3.rs",
                       "mtg-kernel/src/expanded_deck_training_v1/checkpoint_container_v1/update_tests.rs",
                       "mtg-kernel/src/expanded_deck_training_v1/stack_features.rs",
                       "mtg-kernel/src/expanded_deck_training_v1/s_learner_v1.rs",
                       "mtg-kernel/src/expanded_deck_training_v1/weights_snapshot_v1.rs"):
            with self.subTest(file=shared):
                self.assertEqual(ci_scope.scope_of(shared), "shared")
                self.assertEqual(self.scope("mtg-kernel/src/native_expanded_training_run_v1.rs", shared), "pr")

    def test_no_trainer_scoped_module_is_read_from_outside_the_trainer(self):
        # A file maps to trainer only while code outside the trainer modules cannot reach it.
        src = ROOT / "mtg-kernel" / "src"
        roots = tuple(ci_scope.TRAINER_ROOTS)
        refs = set()
        for path in src.rglob("*.rs"):
            rel = path.relative_to(src).as_posix()
            if rel.split("/")[0].removesuffix(".rs") in roots or rel in ("bin/native_expanded_training_run_v1.rs",):
                continue
            for root, item in re.findall(r"(" + "|".join(roots) + r")(?:::([a-z_0-9]+))?", path.read_text(encoding="utf-8", errors="replace")):
                refs.add((root, item))
        for root, item in refs:
            if root == "native_expanded_training_run_v1":
                continue   # only lib.rs declares it
            # An item of the module root (not a submodule file) is the root file itself.
            sub = src / root / item
            if item and not (sub.with_suffix(".rs").exists() or sub.is_dir()):
                item = ""
            target = f"mtg-kernel/src/{root}/{item}.rs" if item else f"mtg-kernel/src/{root}.rs"
            with self.subTest(reference=f"{root}::{item}"):
                self.assertNotEqual(ci_scope.scope_of(target), "trainer", target)

    def test_engine_card_and_unmapped_files_never_skip_the_rules_groups(self):
        for path in ("mtg-kernel/src/engine.rs", "mtg-kernel/src/eternal_cards_v1.rs", "mtg-kernel/src/card_def.rs",
                     "data/eternal_v1/cards.json", "mtg-kernel/src/lib.rs", "Cargo.lock", "mtg-kernel/build.rs",
                     "python/mtg_kernel_rl/features.py", ".github/workflows/ci.yml", "tools/ci/hosted_fallback.py",
                     "python/tools/run_rust_lib_filters.py", "data/notes.md"):
            with self.subTest(file=path):
                self.assertEqual(self.scope(path), "pr")
        self.assertEqual(self.scope("mtg-kernel/src/expanded_deck_training_v1.rs", "mtg-kernel/src/engine.rs"), "pr")
        self.assertEqual(self.scope("tools/stage4a/a.py", "python/tests/test_a.py"), "pr")

    def test_choose(self):
        trainer = ["mtg-kernel/src/native_expanded_training_run_v1.rs"]
        self.assertEqual(ci_scope.choose("pull_request", "pr", "", [], trainer), "trainer")
        self.assertEqual(ci_scope.choose("pull_request", "pr", "", ["ci:full"], trainer), "all")
        # Labels never narrow: an engine change labelled ci:docs or ci:trainer still runs everything.
        self.assertEqual(ci_scope.choose("pull_request", "pr", "", ["ci:docs"], ["mtg-kernel/src/engine.rs"]), "all")
        self.assertEqual(ci_scope.choose("pull_request", "pr", "", ["ci:trainer"], ["mtg-kernel/src/engine.rs"]), "all")
        self.assertEqual(ci_scope.choose("pull_request", "draft", "", [], trainer), "all")
        self.assertEqual(ci_scope.choose("pull_request", "pr", "", [], ["mtg-kernel/src/engine.rs"]), "all")
        self.assertEqual(ci_scope.choose("push", "full", "", [], trainer), "all")
        self.assertEqual(ci_scope.choose("workflow_dispatch", "full", "trainer", [], []), "trainer")
        self.assertEqual(ci_scope.choose("workflow_dispatch", "full", "", [], []), "all")
        self.assertEqual(ci_scope.choose("workflow_dispatch", "full", "nonsense", [], []), "all")


class MonitorTests(unittest.TestCase):
    def test_the_comment_names_the_merge_commit_and_pull_request_and_mentions_no_one(self):
        import post_merge_report as report
        body, line = report.compose("o/r", "https://x/run/1", "abcdef1234567", "main", "push",
                                    ["Rust tests (ubuntu-24.04, limited-model)", "job by @someone"], {"number": 42})
        for text in (body, line):
            self.assertIn("abcdef123", text)
            self.assertIn("#42", text)
            self.assertNotIn("@s", text)
        self.assertIn("full dispatch run", report.compose("o/r", "u", "abc", "main", "workflow_dispatch", ["a"], None)[0])

    def test_red_runs_are_push_or_dispatch_failures_on_the_watched_branches(self):
        import red_push_runs as red
        runs = [{"conclusion": "failure", "event": "push", "head_branch": "main"},
                {"conclusion": "failure", "event": "workflow_dispatch", "head_branch": "release/mull-c5719e1cf"},
                {"conclusion": "failure", "event": "pull_request", "head_branch": "main"},
                {"conclusion": "success", "event": "push", "head_branch": "main"},
                {"conclusion": "failure", "event": "push", "head_branch": "feature"}]
        self.assertEqual(len(red.red_runs(runs, ["main", "release/"])), 2)


if __name__ == "__main__":
    unittest.main()
