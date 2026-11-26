"""The scope table, the workflow and docs/CI-SCOPES.md agree; changes map to the scopes the merge rule sets."""
import os
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
        for job in ("rust-test-groups", "rust-tests", "eternal-integration-build", "rust-eternal-integration-parts",
                    "eternal-integration-inventory", "workspace-lib-build", "rust-workspace-lib-parts",
                    "workspace-lib-inventory"):
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
                stem = prefix.split("::")[0]
                self.assertTrue(any(n == stem or n.startswith(stem) for n in names), prefix)

    def test_the_tools_commands_are_the_python_tests_job_commands(self):
        text = WORKFLOW.read_text(encoding="utf-8")
        block = text[text.index("\n  python-tests:\n"):text.index("\n  trainer-tests:\n")]
        joined = re.sub(r"\\\n\s+", "", block)
        for command in ci_scope.TOOLS_COMMANDS:
            with self.subTest(command=command):
                self.assertIn(command.replace("cd python/tools/tests && ", "").split(" && ")[-1], joined)

    def test_every_tools_dir_has_a_command_in_tools_commands(self):
        for name in ci_scope.TOOLS_DIRS:
            with self.subTest(tool=name):
                self.assertTrue(
                    any(re.search(rf"-s tools/{re.escape(name)}(?:/|\s)", command) for command in ci_scope.TOOLS_COMMANDS),
                    f"tools/{name} is in TOOLS_DIRS but missing from TOOLS_COMMANDS",
                )

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
        self.assertEqual(self.scope("tools/cube_v1/deal.py"), "tools")
        self.assertEqual(self.scope("tools/reach/plan.py"), "python")          # its tests are in python/tests
        self.assertEqual(self.scope("python/tests/test_x.py", "python/tools/foo.py"), "python")
        self.assertEqual(self.scope("mtg-kernel/src/native_expanded_training_run_v1.rs",
                                    "mtg-kernel/src/bin/native_expanded_training_run_v1.rs"), "trainer")
        self.assertEqual(self.scope("mtg-kernel/src/bin/kernel_limited_match_v1.rs"), "rules-limited")
        self.assertEqual(self.scope("python/tools/check_limited_common_client_v1.py"), "python")
        self.assertEqual(self.scope(".github/workflows/viewer.yml"), "workflow")
        self.assertEqual(self.scope("docs/a.md", "README.md"), "docs")

    def test_the_files_that_choose_rust_tests_run_the_whole_set_and_the_rust_jobs(self):
        (line,) = [line for line in WORKFLOW.read_text(encoding="utf-8").splitlines()
                   if "python/tools/run_rust_integration_tests.py|" in line]
        relevant = line.strip().rstrip(")").split("|")
        for name in ("run_rust_workspace_tests.py", "rust_workspace_lib_plan.py", "rust_workspace_lib_seconds.json",
                     "run_rust_integration_tests.py", "check_rust_integration_parts.py"):
            with self.subTest(file=name):
                self.assertEqual(ci_scope.scope_of(f"python/tools/{name}"), "pr")
                self.assertIn(f"python/tools/{name}", relevant)

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
            text = path.read_text(encoding="utf-8", errors="replace")
            if rel == "lib.rs":
                # The crate root declares every module; only its declarations are exempt.
                text = re.sub(r"^\s*(pub(\([a-z]+\))? )?mod [a-z_0-9]+;\s*$", "", text, flags=re.MULTILINE)
            for root, item in re.findall(r"(" + "|".join(roots) + r")(?:::([a-z_0-9]+))?", text):
                refs.add((root, item))
        for root, item in refs:
            # An item of the module root (not a submodule file) is the root file itself.
            sub = src / root / item
            if item and not (sub.with_suffix(".rs").exists() or sub.is_dir()):
                item = ""
            target = f"mtg-kernel/src/{root}/{item}.rs" if item else f"mtg-kernel/src/{root}.rs"
            with self.subTest(reference=f"{root}::{item}"):
                self.assertNotEqual(ci_scope.scope_of(target), "trainer", target)

    def test_a_limited_binary_with_its_own_tests_runs_the_whole_set(self):
        bins = ROOT / "mtg-kernel" / "src" / "bin"
        for path in sorted(bins.glob("kernel_limited_*.rs")):
            rel = path.relative_to(ROOT).as_posix()
            has_tests = "#[test]" in path.read_text(encoding="utf-8", errors="replace")
            with self.subTest(binary=rel):
                self.assertEqual(ci_scope.scope_of(rel), "pr" if has_tests else "rules-limited")

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


class ShardTests(unittest.TestCase):
    def test_every_trainer_filter_lands_in_exactly_one_shard(self):
        flat = [f for shard in ci_scope.TRAINER_SHARDS for f in shard]
        self.assertEqual(sorted(flat), sorted(ci_scope.TRAINER_FILTERS))
        self.assertEqual(len(flat), len(set(flat)))
        self.assertTrue(all(ci_scope.TRAINER_SHARDS))

    def test_the_matrix_has_one_job_per_shard(self):
        text = WORKFLOW.read_text(encoding="utf-8")
        job = text[text.index("\n  trainer-tests:\n"):]
        job = job[:job.index("\n    steps:\n")]
        self.assertIn(f"shard: {list(range(len(ci_scope.TRAINER_SHARDS)))}", job)
        self.assertEqual(len(ci_scope.TRAINER_SHARD_FEATURES), len(ci_scope.TRAINER_SHARDS))
        self.assertIn("check_filters_select.py", WORKFLOW.read_text(encoding="utf-8"))

    def test_cuda_only_modules_run_in_the_cuda_shard(self):
        # Modules lib.rs compiles only with the CUDA features must sit in a shard built with them.
        lib = (ROOT / "mtg-kernel" / "src" / "lib.rs").read_text(encoding="utf-8")
        for shard, features in zip(ci_scope.TRAINER_SHARDS, ci_scope.TRAINER_SHARD_FEATURES):
            for f in shard:
                module = f.split("::")[0]
                gate = re.search(r'#\[cfg\(feature = "([a-z0-9-]+)"\)\]\s*(?:pub )?mod ' + re.escape(module) + r';', lib)
                if gate:
                    with self.subTest(filter=f):
                        self.assertIn(gate.group(1), features.split(","))
        self.assertIn("trainer-filters --shard ${{ matrix.shard }}", WORKFLOW.read_text(encoding="utf-8"))


class ReferenceGuardTests(unittest.TestCase):
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
        self.assertIn("run: python tools/ci/ci_scope.py check-references", WORKFLOW.read_text(encoding="utf-8"))


class MonitorTests(unittest.TestCase):
    def test_the_monitor_runs_on_a_failed_or_timed_out_dependency(self):
        condition = workflow()["jobs"]["post-merge-report"]["if"]
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

    def test_later_jobs_page_is_judged_at_the_first_attempt_cutoff(self):
        import post_merge_report as report
        cutoff = "2026-11-21T12:30:00Z"
        path = "/repos/o/r/actions/runs/8/jobs?per_page=100&filter=all&page="
        for started, expected in [("2026-11-21T11:00:00Z", True), (cutoff, False)]:
            pages = {
                path + "1": {"jobs": [{"conclusion": "cancelled"}] * 100},
                path + "2": {"jobs": [{"conclusion": "success", "started_at": started}]},
            }
            with self.subTest(started=started):
                self.assertEqual(report.ran(pages.__getitem__, "o/r", {"id": 8}, cutoff), expected)

    def test_an_earlier_run_being_rerun_still_bounds_the_range(self):
        # Push run 8 (c2) ran before the red run; it is being rerun now (in progress, a second attempt queued).
        get = self.fake_api(["c0", "c1", "c2", "c3"],
                            [(8, "c2", "rerun-running", "2026-11-21T11:30:00Z"),
                             (6, "c1", "success", "2026-11-21T11:00:00Z")],
                            {"c2": (836, "a"), "c3": (837, "b")})
        body, line, merges = self.run_report(get)
        self.assertEqual([(m["sha"], m["pull"]) for m in merges], [("c3", 837)])
        self.assertIn("first responder the author of #837", line)

    def test_no_earlier_monitored_run_is_an_unbounded_range(self):
        get = self.fake_api(["c1", "c2", "c3"], [], {"c3": (837, "x")})
        body, line, merges = self.run_report(get)
        self.assertEqual([m["sha"] for m in merges], ["c3"])
        self.assertIn("(incomplete, newest only)", line)

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
