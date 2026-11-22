"""The release line's scope table, workflow and docs/CI-SCOPES.md agree; changes map as the lead ruled."""
import base64
import os
import re
import subprocess
import sys
import tempfile
import textwrap
import unittest
from pathlib import Path
from unittest.mock import patch

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

    def test_changed_path_gates_preserve_git_paths(self):
        workflow = WORKFLOW.read_text(encoding="utf-8")

        def script(step):
            block = workflow.split(f"      - name: {step}\n", 1)[1]
            run = re.search(r"^        run: \|\n((?:          .*\n|\n)+)", block, re.MULTILINE)
            return textwrap.dedent(run[1])

        classify = script("Classify changed paths").replace("${{ github.event_name }}", "pull_request")
        checks = script("Choose the checks").replace(
            "python3 tools/ci/ci_scope.py", '"$CI_TEST_PYTHON" "$CI_TEST_SCOPE"'
        )
        # Replace the two API responses. Git and both file consumers still run normally.
        checks = 'gh() { if [ "$2" = --paginate ]; then echo 0; else echo 1; fi; }\n' + checks
        bash = "C:/Program Files/Git/bin/bash.exe" if sys.platform == "win32" else "bash"
        trainer = "mtg-kernel/src/native_policy_train_step_v1/café.rs"
        cases = [
            ("tools/café.py", "docs/moved.md", "true", "false", "all"),
            ("docs/before.md", "tools/file with spaces.py", "true", "false", "all"),
            ("python/tests/café.py", None, "true", "false", "all"),
            ("docs/café.md", "docs/updated.md", "false", "false", "docs"),
            ("docs/before.md", trainer, "false", "true", "release-trainer"),
            (trainer, "docs/moved.md", "false", "true", "release-trainer"),
            ("tools/café.py", trainer, "true", "true", "all"),
        ]
        if sys.platform != "win32":
            cases.append(('tools/"quoted"\nname.py', None, "true", "false", "all"))

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
                           DRAFT="false", LABELS="[]", HEAD_REF="fixture", CI_TEST_PYTHON=Path(sys.executable).as_posix(),
                           CI_TEST_SCOPE=(ROOT / "tools/ci/ci_scope.py").as_posix())
                for rendered in (classify, checks):
                    subprocess.run([bash, "-eo", "pipefail", "-c", rendered], cwd=repo, env=env, check=True,
                                   capture_output=True)

                self.assertEqual(output.read_text(encoding="utf-8").splitlines(),
                                 [f"python_relevant={python}", f"rust_relevant={rust}", f"checks={scope}"])

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

    def test_feature_rows_include_every_full_set_feature_filter_for_the_qualifying_files(self):
        import rust_test_cfg
        rows = [rust_test_cfg.closure(f) for os_name, f in ci_scope.TRAINER_ROWS if os_name == "linux" and f]
        steps = rust_test_cfg.full_set_feature_steps()
        self.assertGreater(len(steps), 10)
        for features, filters in steps:
            hits = [n for n in rust_test_cfg.predict(features) if any(f in n for f in filters)]
            if not hits:
                continue
            with self.subTest(features=features):
                (row,) = [f for os_name, f in ci_scope.TRAINER_ROWS
                          if os_name == "linux" and f and rust_test_cfg.closure(f) == rust_test_cfg.closure(features)]
                self.assertTrue(set(hits) <= set(rust_test_cfg.row_tests(row)), features)
        self.assertTrue(rows)

    def test_local_commands_carry_the_lint_job_commands(self):
        text = WORKFLOW.read_text(encoding="utf-8")
        job = text[text.index("  rust-quality:\n"):text.index("  rust-test-groups:\n")]
        for command in ci_scope.QUALITY_COMMANDS:
            self.assertIn("run: " + command, job)

    def test_every_module_the_train_step_and_training_math_reference_is_filtered_or_excluded(self):
        import tempfile
        self.assertEqual(ci_scope.training_reference_gaps(), ([], []))
        # A new reference from the train step to an unexamined module fails the guard the trainer scope runs.
        with tempfile.TemporaryDirectory() as tmp:
            for name in ci_scope.TRAINING_REFERENCE_ROOTS:
                src = ROOT / name
                for file in ([src] if src.is_file() else sorted(src.rglob("*.rs"))):
                    out = Path(tmp) / file.relative_to(ROOT)
                    out.parent.mkdir(parents=True, exist_ok=True)
                    out.write_text(file.read_text(encoding="utf-8", errors="replace"), encoding="utf-8")
            math = Path(tmp) / "mtg-kernel/src/training_math_v1.rs"
            original = math.read_text(encoding="utf-8")
            for added, expected in [("use crate::unexamined_backend_v9::X;", ["unexamined_backend_v9"]),
                                    ("use crate::{fast_sampler, unexamined_v8::{A, B}};", ["unexamined_v8"]),
                                    ("use super::unexamined_v7::X;", ["unexamined_v7"]),
                                    ("mod t { use super::super::unexamined_v6::X; use super::local::Y; }",
                                     ["unexamined_v6"]),
                                    ("use crate::training_math_v1_extra::X;", ["training_math_v1_extra"]),
                                    ("// use crate::only_in_a_comment::X;", [])]:
                with self.subTest(reference=added):
                    math.write_text(original + "\n" + added + "\n", encoding="utf-8")
                    self.assertEqual(ci_scope.training_reference_gaps(tmp)[0], expected)
        self.assertIn('"tools/ci/ci_scope.py", "check-references"',
                      (ROOT / "tools/ci/run_trainer_row.py").read_text(encoding="utf-8"))

    def test_a_sync_pull_request_runs_the_full_set(self):
        trainer = ["mtg-kernel/src/expanded_deck_training_v1.rs"]
        self.assertEqual(ci_scope.choose("pull_request", "pr", "", [], trainer, 0, "main"), "all")
        self.assertEqual(ci_scope.choose("pull_request", "pr", "", [], trainer, 0, "release/mull-x"), "all")
        self.assertEqual(ci_scope.choose("pull_request", "pr", "", [], trainer, 0, "claude/feature"), "release-trainer")
        self.assertIn("HEAD_REF: ${{ github.head_ref }}", WORKFLOW.read_text(encoding="utf-8"))

    def test_the_record_names_the_mandatory_rows(self):
        record = ci_scope.trainer_record()
        for row in ci_scope.BIT_IDENTITY_ROWS:
            self.assertIn(row, record)
        self.assertIn("field absent", record)
        self.assertIn("not mergeable", record)


class CfgPredictorTests(unittest.TestCase):
    def test_cuda_row_runs_the_public_collector_refusal(self):
        import rust_test_cfg
        name = ("expanded_deck_training_v1::tests::priority_pass_offset_tests::"
                "public_collector_refuses_priority_pass_offset")
        features = "experimental-burn-net8-packed-cuda-v1,gpu-exact-v1"
        self.assertIn(name, rust_test_cfg.row_tests(features))
        self.assertNotIn(name, rust_test_cfg.predict(""))
        self.assertNotIn(name, rust_test_cfg.predict("", "windows"))

    def test_unsupported_host_is_rejected_before_building(self):
        import run_trainer_row
        for system, machine in (("linux", "aarch64"), ("win32", "ARM64"), ("darwin", "x86_64")):
            with (self.subTest(system=system, machine=machine),
                  patch.object(sys, "argv", ["run_trainer_row.py", "--features", ""]),
                  patch.object(sys, "platform", system),
                  patch.object(run_trainer_row.platform, "machine", return_value=machine),
                  patch.object(run_trainer_row, "build") as build,
                  patch("builtins.print") as diagnostic):
                self.assertEqual(run_trainer_row.main(), 2)
                build.assert_not_called()
                self.assertIn(f"unsupported host {system}/{machine}", diagnostic.call_args.args[0])

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

    def test_an_attributed_include_gates_every_included_test(self):
        files = {
            "m.rs": 'mod t {\n    #[cfg(feature = "a")]\n    include!(r#"extra.rs"#);\n    include!("plain\\x2ers");\n}\n',
            "m/../extra.rs": "#[test]\nfn one() {}\n#[test]\nfn two() {}\n",
            "plain.rs": "#[test]\nfn three() {}\n",
        }
        self.assertEqual(self.predicted(files, ""), ["m::t::three"])
        self.assertEqual(self.predicted(files, "a"), ["m::t::one", "m::t::three", "m::t::two"])

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
    def test_monitors_on_the_line_coalesce_as_on_main(self):
        text = WORKFLOW.read_text(encoding="utf-8")
        group = [l for l in text.splitlines() if l.startswith("  group: ")][0]
        # Only scoped or fallback dispatches get a run-specific group; pushes share one per line.
        self.assertNotIn("github.event_name == 'push'", group)
        self.assertEqual(group.count("run_id"), 1)
        self.assertIn("github.event_name == 'workflow_dispatch' && (inputs.scope != '' || inputs.windows_runner != '')",
                      group)
        self.assertIn("cancel-in-progress: ${{ github.event_name == 'pull_request' }}", text)

    def test_the_monitor_runs_on_a_failed_or_timed_out_dependency(self):
        condition = jobs()["post-merge-report"]
        self.assertIn("contains(needs.*.result, 'failure') || contains(needs.*.result, 'cancelled')", condition)

    @staticmethod
    def fake_api(chain, runs, pulls, run_id=9):
        """chain: first-parent shas oldest to newest (each a merge with a side parent); runs: earlier push runs
        newest first as (id, head, conclusion, created); pulls: sha -> (number, login)."""
        commits = []
        for i, sha in enumerate(chain):
            parents = [{"sha": chain[i - 1] if i else "root"}, {"sha": "side-" + sha}]
            commits.append({"sha": sha, "parents": parents})
        head = chain[-1]

        def get(path):
            if path == f"/repos/o/r/actions/runs/{run_id}":
                return {"id": run_id, "head_sha": head, "created_at": "2026-11-21T12:00:00Z",
                        "run_started_at": "2026-11-21T12:30:00Z", "run_attempt": 1}
            if path.startswith("/repos/o/r/actions/workflows/ci.yml/runs") and "status=completed" in path:
                return {"workflow_runs": [{"id": i, "head_sha": h, "conclusion": c, "created_at": t}
                                          for i, h, c, t in runs if c != "rerun-running"]}
            if path.startswith("/repos/o/r/actions/workflows/ci.yml/runs"):
                return {"workflow_runs": [{"id": i, "head_sha": h, "conclusion": c, "created_at": t}
                                          for i, h, c, t in runs]}
            if "/jobs" in path:
                rid = int(path.split("/runs/")[1].split("/")[0])
                ran = {i: c for i, h, c, t in runs}.get(rid) == "ran-cancelled"
                rerun = {i: c for i, h, c, t in runs}.get(rid) == "rerun-later"
                if {i: c for i, h, c, t in runs}.get(rid) == "rerun-running":
                    return {"jobs": [{"name": "Rust tests (ubuntu-24.04, limited)", "started_at": "2026-11-21T11:05:00Z",
                                      "conclusion": "success"},
                                     {"name": "Rust tests (ubuntu-24.04, limited)", "started_at": "2026-11-21T13:00:00Z",
                                      "conclusion": None}]}
                return {"jobs": [{"name": "Rust tests (ubuntu-24.04, limited)", "started_at":
                                  "2026-11-21T13:00:00Z" if rerun else "2026-11-21T11:05:00Z", "conclusion":
                                  "failure" if rid == run_id else ("success" if ran or rerun or
                                                                   {i: c for i, h, c, t in runs}.get(rid) == "success"
                                                                   else None)}]}
            if "/pulls?" in path:
                sha = path.split("/commits/")[1].split("/")[0]
                number, login = pulls[sha]
                return [{"number": number, "merged_at": "x", "user": {"login": login}, "merge_commit_sha": sha,
                         "base": {"ref": "main"}},
                        {"number": 999, "merged_at": "x", "user": {"login": "z"}, "merge_commit_sha": "other",
                         "base": {"ref": "main"}}]
            if path.startswith("/repos/o/r/commits/"):
                sha = path.rsplit("/", 1)[1]
                return {"parents": next(c["parents"] for c in commits if c["sha"] == sha)}
            raise AssertionError(path)
        return get

    def run_report(self, get):
        import post_merge_report as report
        return report.report(get, "o/r", {"id": 9, "head_sha": "c3", "head_branch": "main", "event": "push",
                                          "html_url": "https://x/runs/9"})

    def test_a_run_covering_one_merge(self):
        get = self.fake_api(["c1", "c2", "c3"], [(8, "c2", "success", "2026-11-21T11:00:00Z")],
                            {"c3": (837, "someone")})
        body, line, merges = self.run_report(get)
        self.assertEqual([(m["sha"], m["pull"]) for m in merges], [("c3", 837)])
        self.assertIn("It covers 1 merge, oldest first:", body)
        self.assertIn("covering c3 (#837)", line)
        self.assertIn("first responder the author of #837", line)
        self.assertIn("They bisect by rerunning the failed jobs", body)
        self.assertNotIn("someone", body + line)

    def test_a_run_covering_several_merges_skips_a_replaced_run(self):
        # Run 7 (head c2) was replaced while waiting and never ran; run 6 (head c1, cancelled at a shard's time
        # limit) did run, so c1 is the last monitored commit and the run covers c2 and c3.
        get = self.fake_api(["c0", "c1", "c2", "c3"],
                            [(7, "c2", "cancelled", "2026-11-21T11:30:00Z"),
                             (6, "c1", "ran-cancelled", "2026-11-21T11:00:00Z")],
                            {"c2": (836, "a@b"), "c3": (837, "c")})
        get_runs = get

        def get(path):
            out = get_runs(path)
            if "workflows/ci.yml/runs" in path:
                for run in out["workflow_runs"]:
                    run["conclusion"] = "cancelled" if run["conclusion"] == "ran-cancelled" else run["conclusion"]
            return out
        body, line, merges = self.run_report(get)
        self.assertEqual([(m["sha"], m["pull"]) for m in merges], [("c2", 836), ("c3", 837)])
        self.assertIn("It covers 2 merges, oldest first:\n- `c2` (#836)\n- `c3` (#837)", body)
        self.assertIn("covering c2 (#836), c3 (#837)", line)
        self.assertIn("First responder: the author of #836", body)

    def test_a_range_longer_than_the_walk_is_reported_as_incomplete(self):
        import post_merge_report as report
        get = self.fake_api(["c0", "c1", "c2", "c3"], [(8, "c0", "success", "2026-11-21T11:00:00Z")],
                            {"c2": (836, "a"), "c3": (837, "b")})
        saved, report.MAX_COVERED = report.MAX_COVERED, 2
        try:
            body, line, merges = self.run_report(get)
        finally:
            report.MAX_COVERED = saved
        self.assertEqual([m["sha"] for m in merges], ["c2", "c3"])
        self.assertIn("It covers more than 2 merges", body)
        self.assertIn("(incomplete, newest only)", line)
        self.assertNotIn("the author of #836", line)

    def test_the_collector_uses_the_monitor_predicate(self):
        import red_push_runs as red
        runs = [{"id": 1, "conclusion": "cancelled", "event": "push", "head_branch": "main"},
                {"id": 2, "conclusion": "failure", "event": "workflow_dispatch", "head_branch": "main"},
                {"id": 3, "conclusion": "failure", "event": "pull_request", "head_branch": "main"},
                {"id": 4, "conclusion": "success", "event": "push", "head_branch": "main"},
                {"id": 5, "conclusion": "failure", "event": "push", "head_branch": "feature"},
                {"id": 6, "conclusion": "cancelled", "event": "push", "head_branch": "release/x"}]
        report_ran = {1: "success", 2: "skipped", 6: None}   # 2: a scoped dispatch; 6: replaced before running

        def get(path):
            rid = int(path.split("/runs/")[1].split("/")[0])
            conclusion = report_ran.get(rid)
            return {"jobs": [{"name": "Report a red post-merge run", "conclusion": conclusion,
                              "started_at": None if conclusion == "skipped" else "t"}] if conclusion else []}
        self.assertEqual([r["id"] for r in red.red_runs(runs, ["main", "release/"], get, "o/r")], [1])

    def test_exhausted_history_is_incomplete_not_a_range_of_one(self):
        import post_merge_report as report
        get = self.fake_api(["c1", "c2", "c3"], [], {"c3": (837, "x")})

        def get_full_pages(path):
            if "workflows/ci.yml/runs" in path:
                return {"workflow_runs": [{"id": 100 + i, "head_sha": "c3", "conclusion": "cancelled",
                                           "created_at": "2026-11-21T11:00:00Z"} for i in range(50)]}
            return get(path)
        body, line, merges = self.run_report(get_full_pages)
        self.assertIn("(incomplete, newest only)", line)
        self.assertNotIn("the author of #837", line)

    def test_a_cancelled_report_job_still_marks_a_monitor(self):
        import post_merge_report as report
        for conclusion, expected in (("cancelled", True), ("timed_out", True), ("skipped", False)):
            jobs = [{"name": "Report a red post-merge run", "conclusion": conclusion,
                     "started_at": None if conclusion == "skipped" else "t"}]
            self.assertEqual(report.is_monitor(lambda path: {"jobs": jobs}, "o/r", {"id": 1}), expected)

    def test_an_earlier_run_in_the_same_second_is_the_last_monitored(self):
        get = self.fake_api(["c1", "c2", "c3"], [(8, "c2", "success", "2026-11-21T12:00:00Z")],
                            {"c3": (837, "x")})
        self.assertEqual([m["sha"] for m in self.run_report(get)[2]], ["c3"])

    def test_a_failed_comment_never_hides_the_handoff_line(self):
        import io
        import contextlib
        import post_merge_report as report
        get = self.fake_api(["c1", "c2", "c3"], [(8, "c2", "success", "2026-11-21T11:00:00Z")], {"c3": (837, "x")})
        run = {"id": 9, "head_sha": "c3", "head_branch": "main", "event": "push", "html_url": "u",
               "created_at": "2026-11-21T12:00:00Z", "run_started_at": "2026-11-21T12:30:00Z", "run_attempt": 1}

        def request(method, path, body=None):
            if method == "POST":
                raise OSError("403")
            return run if path.endswith("/runs/9") else get(path)
        saved = report.request
        report.request = request
        env = {"GITHUB_REPOSITORY": "o/r", "GITHUB_RUN_ID": "9", "GITHUB_SHA": "c3", "GITHUB_REF_NAME": "main",
               "GITHUB_EVENT_NAME": "push"}
        old_env = {k: os.environ.get(k) for k in env}
        os.environ.update(env)
        out, err = io.StringIO(), io.StringIO()
        try:
            with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
                code = report.main()
        finally:
            report.request = saved
            for k, v in old_env.items():
                os.environ.pop(k, None) if v is None else os.environ.__setitem__(k, v)
        self.assertEqual(code, 1)
        self.assertIn("Handoff log line: [CI] post-merge push run red on main covering c3 (#837)", out.getvalue())
        self.assertIn(">> HANDOFFS/20261114/README.md", out.getvalue())
        self.assertIn("Could not comment on #837", err.getvalue())

    def test_scoped_and_fallback_dispatches_never_join_the_monitor_group(self):
        text = WORKFLOW.read_text(encoding="utf-8")
        group = [l for l in text.splitlines() if l.startswith("  group: ")][0]
        self.assertIn("github.event_name == 'workflow_dispatch' && (inputs.scope != '' || inputs.windows_runner != '')"
                      " && format('-dispatch-{0}', github.run_id)", group)

    def test_only_replaced_earlier_runs_make_the_range_incomplete(self):
        get = self.fake_api(["c1", "c2", "c3"], [(8, "c2", "cancelled", "2026-11-21T11:30:00Z"),
                                                 (7, "c1", "cancelled", "2026-11-21T11:00:00Z")], {"c3": (837, "x")})
        body, line, merges = self.run_report(get)
        self.assertIn("(incomplete, newest only)", line)
        self.assertNotIn("first responder the author of #837", line)

    def test_a_later_rerun_of_an_earlier_push_never_changes_the_report(self):
        # Push run 8 (c2) was replaced while waiting; it was rerun successfully after the red run started.
        get = self.fake_api(["c0", "c1", "c2", "c3"],
                            [(8, "c2", "rerun-later", "2026-11-21T11:30:00Z"),
                             (6, "c1", "success", "2026-11-21T11:00:00Z")],
                            {"c2": (836, "a"), "c3": (837, "b")})
        body, line, merges = self.run_report(get)
        self.assertEqual([(m["sha"], m["pull"]) for m in merges], [("c2", 836), ("c3", 837)])
        self.assertIn("first responder the author of #836", line)

    def test_an_earlier_run_being_rerun_still_bounds_the_range(self):
        # Push run 8 (c2) ran before the red run; it is being rerun now (in progress, a second attempt queued).
        get = self.fake_api(["c0", "c1", "c2", "c3"],
                            [(8, "c2", "rerun-running", "2026-11-21T11:30:00Z"),
                             (6, "c1", "success", "2026-11-21T11:00:00Z")],
                            {"c2": (836, "a"), "c3": (837, "b")})
        body, line, merges = self.run_report(get)
        self.assertEqual([(m["sha"], m["pull"]) for m in merges], [("c3", 837)])
        self.assertIn("first responder the author of #837", line)

    def test_later_jobs_page_is_judged_at_the_first_attempt_cutoff(self):
        import post_merge_report as report
        cutoff = "2026-11-21T12:30:00Z"
        path = "/repos/o/r/actions/runs/8/jobs?per_page=100&filter=all&page="
        for started, expected in (("2026-11-21T11:00:00Z", True), (cutoff, False)):
            pages = {
                path + "1": {"jobs": [{"conclusion": "cancelled"}] * 100},
                path + "2": {"jobs": [{"conclusion": "success", "started_at": started}]},
            }
            with self.subTest(started=started):
                self.assertEqual(report.ran(pages.__getitem__, "o/r", {"id": 8}, cutoff), expected)

    def test_no_earlier_monitored_run_is_an_unbounded_range(self):
        get = self.fake_api(["c1", "c2", "c3"], [], {"c3": (837, "x")})
        body, line, merges = self.run_report(get)
        self.assertEqual([m["sha"] for m in merges], ["c3"])
        self.assertIn("(incomplete, newest only)", line)

    def test_first_release_monitor_covers_replaced_pushes_from_rollout(self):
        import post_merge_report as report
        base, early, a, b, c = (digit * 40 for digit in "12345")
        branch = "release/demo"
        ref = f"refs/heads/{branch}"
        chain = [base, early, a, b, c]
        pushes = [(8, b, "cancelled", "2026-11-21T11:30:00Z"),
                  (7, a, "cancelled", "2026-11-21T11:00:00Z")]
        get_run = self.fake_api(chain, pushes, {sha: (200 + i, "owner") for i, sha in enumerate(chain)})
        enabled = WORKFLOW.read_text(encoding="utf-8")
        disabled = enabled.replace("      - 'release/**'\n", "", 1)
        push_b = {"type": "PushEvent", "created_at": "2026-11-21T11:30:00Z",
                  "payload": {"ref": ref, "head": b, "before": a}}
        push_a = {"type": "PushEvent", "created_at": "2026-11-21T11:00:00Z",
                  "payload": {"ref": ref, "head": a, "before": base}}
        events = [push_b, push_a]
        current = {"ref": ref, "after": c, "before": b}

        def get(path):
            if path.startswith("/repos/o/r/events?"):
                return events
            if "/contents/" in path:
                sha = path.split("?ref=")[1]
                source = disabled if sha == base else enabled
                return {"encoding": "base64", "content": base64.b64encode(source.encode()).decode()}
            result = get_run(path)
            if "/pulls?" in path:
                for pull in result:
                    pull["base"]["ref"] = branch
            return result

        run = {"id": 9, "head_sha": c, "head_branch": branch, "event": "push", "html_url": "u"}
        body, _, merges = report.report(get, "o/r", run, current)
        self.assertEqual([(m["sha"], m["pull"]) for m in merges], [(early, 201), (a, 202), (b, 203), (c, 204)])
        self.assertIn("First responder: the author of #201", body)

        # A later push to the same revision cannot supply the missing historical boundary.
        events[1] = {**push_a, "created_at": "2026-11-21T12:01:00Z"}
        body, line, merges = report.report(get, "o/r", run, current)
        self.assertEqual([m["sha"] for m in merges], [c])
        self.assertIn("(incomplete, newest only)", line)
        self.assertNotIn("First responder: the author of #204", body)

    def test_the_comment_mentions_no_one(self):
        import post_merge_report as report
        body, line = report.compose("u", "main", "workflow_dispatch", ["job by @someone"],
                                    [{"sha": "abcdef1234", "pull": 7, "author": "x"}])
        self.assertNotIn("@s", body + line)
        self.assertIn("full dispatch run", body)

    def test_the_collector_splits_listings_above_the_result_cap(self):
        import red_push_runs as red
        from datetime import datetime, timezone
        calls = []

        def listing(repo, start, end):
            calls.append((start, end))
            hours = (end - start).total_seconds() / 3600
            return (1500 if hours > 24 else 10), [{"id": len(calls), "event": "push",
                                                     "updated_at": "2026-11-21T12:00:00Z"}]
        saved = red.listing
        red.listing = listing
        try:
            runs = red.runs_in("o/r", datetime(2026, 11, 1, tzinfo=timezone.utc), datetime(2026, 11, 3, tzinfo=timezone.utc))
        finally:
            red.listing = saved
        self.assertGreater(len(calls), 1)
        self.assertEqual(len(runs), len(calls) - sum(1 for s_, e_ in calls if (e_ - s_).total_seconds() > 86400))

    def test_straddling_release_failure_survives_scan_cutoff(self):
        import red_push_runs as red
        from datetime import datetime, timezone
        cutoff = "2026-11-21T12:00:00Z"
        branch = "release/demo"
        created = "2026-11-21T10:00:00Z"
        run = {"id": 1, "event": "push", "head_branch": branch, "created_at": created,
               "updated_at": "2026-11-21T12:01:00Z", "conclusion": "failure"}
        earlier = {**run, "id": 2, "updated_at": "2026-11-21T11:59:59Z"}
        with patch.object(red, "listing", return_value=(2, [run, earlier])) as listing:
            runs = red.runs_since("o/r", cutoff, lookback_days=1)
        self.assertLess(listing.call_args.args[1], datetime.fromisoformat(created.replace("Z", "+00:00")))
        self.assertEqual(listing.call_args.args[1].tzinfo, timezone.utc)
        job = {"name": "Report a red post-merge run", "conclusion": "success", "started_at": cutoff}
        self.assertEqual(red.red_runs(runs, ["release/"], lambda path: {"jobs": [job]}, "o/r"), [run])

    def test_the_append_command_and_append_are_exact(self):
        import post_merge_report as report
        import red_push_runs as red
        import shlex
        import subprocess
        import tempfile
        line = "[CI] post-merge push run red on main covering c3 (#837): it's \"x\"; $HOME `y`"
        command = report.append_command(line)
        self.assertTrue(command.endswith(">> HANDOFFS/20261114/README.md"))
        with tempfile.TemporaryDirectory() as tmp:
            (Path(tmp) / "HANDOFFS" / "20261114").mkdir(parents=True)
            bash = "C:/Program Files/Git/bin/bash.exe" if sys.platform == "win32" else "bash"
            subprocess.run([bash, "-c", command], cwd=tmp, check=True)
            self.assertEqual((Path(tmp) / "HANDOFFS/20261114/README.md").read_text(encoding="utf-8"), line + "\n")
            log = Path(tmp) / "README.md"
            log.write_text("old\n", encoding="utf-8")
            self.assertEqual(red.append_new(str(log), ["a", "old"]), ["a"])
            self.assertEqual(red.append_new(str(log), ["a"]), [])
            self.assertEqual(log.read_text(encoding="utf-8"), "old\na\n")

    def test_bisect_commands_use_the_scope_that_reruns_the_failed_jobs(self):
        import post_merge_report as report
        merges = [{"sha": "c2" * 5, "pull": 836, "author": "a"}, {"sha": "c3" * 5, "pull": 837, "author": "b"}]
        body, line = report.compose("u", "main", "push", ["Trainer library tests (ubuntu-24.04, shard 1)"], merges)
        scope = report.JOB_SCOPES[0][1]
        self.assertIn(f"git push origin {'c2' * 5}:refs/heads/bisect-c2c2c2c2c && gh workflow run ci.yml "
                      f"--ref bisect-c2c2c2c2c -f scope={scope}", body)
        self.assertIn(f"who bisects with a {scope} dispatch on each covered commit", line)
        body, line = report.compose("u", "main", "push", ["Rust tests (windows-2025, workspace-lib)"], merges)
        self.assertNotIn("-f scope", body)
        self.assertIn("who bisects with a full dispatch", line)



if __name__ == "__main__":
    unittest.main()
