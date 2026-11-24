"""The scope table, the workflow and docs/CI-SCOPES.md agree; changes map to the scopes the merge rule sets."""
import fnmatch
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
                    "eternal-integration-inventory"):
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


VIEWER_WORKFLOW = ROOT / ".github" / "workflows" / "viewer.yml"
# Website files (pull request 951 changed README.md and the first five).
WEBSITE = ["viewer/index.html", "viewer/public/blog/2026-11-20-still-climbing/index.html", "viewer/public/blog/feed.xml",
           "viewer/public/blog/index.html", "viewer/src/main.tsx", "viewer/package-lock.json", "viewer/demo-release.json",
           "viewer/tests/browser/replay.spec.ts", "viewer/vite.config.ts"]
# Paths that start with the letters "viewer" (or name it) but are not under viewer/.
LOOKALIKES = ["viewer", "viewer_tools/x.py", "viewerx/index.html", "mtg-kernel/src/viewer.rs", "mtg-kernel/viewer/x.rs",
              "mtg-kernel/src/bin/viewer_capture_v1.rs", "mtg-kernel/src/viewer_capture_v1.rs",
              "python/tools/viewer_export_v1.py", "python/tests/test_viewer_uct_v1.py", "tools/viewer/x.py",
              "docs/design/viewer_spy_trace_v1.md", ".github/workflows/viewer.yml"]
# Non-website changes the composition tests pair with the website files.
OTHERS = [["README.md"], ["docs/a.md", "docs/notes.txt"], ["mtg-kernel/src/engine.rs"], ["Cargo.lock"],
          ["mtg-kernel/src/native_expanded_training_run_v1.rs"], ["mtg-kernel/src/bin/kernel_limited_match_v1.rs"],
          ["python/tools/foo.py"], ["python/tests/test_x.py", "README.md"], ["tools/stage4a/report.py"],
          ["tools/w5/chains.py"], ["tools/ci/hosted_fallback.py"], ["python/tools/foo.py", "tools/stage4a/a.py"],
          [".github/workflows/ci.yml"], [".github/workflows/viewer.yml"], ["data/eternal_v1/cards.json"],
          ["viewer_tools/x.py"], ["mtg-kernel/src/viewer.rs"]]
# Same-change baselines (finding 1 of the second review first).
SAME_CHANGE = [WEBSITE + ["tools/ci/hosted_fallback.py"], ["tools/ci/hosted_fallback.py"], WEBSITE,
               WEBSITE + ["README.md"], WEBSITE + ["mtg-kernel/src/engine.rs"], WEBSITE + ["python/tools/foo.py"],
               WEBSITE + [".github/workflows/ci.yml"], WEBSITE + [".github/workflows/s-transformer-v1.yml"],
               WEBSITE + ["data/eternal_v1/cards.json"]] + [WEBSITE + other for other in OTHERS] + OTHERS
RUST_SET = {"rust-quality", "rust-test-groups", "limited-build", "rust-limited-groups", "rust-tests",
            "eternal-integration-build", "rust-eternal-integration-parts", "eternal-integration-inventory"}
PYTHON_SET = {"python-env-build", "python-tests"}
FULL_PR = {(job, "pr") for job in RUST_SET | PYTHON_SET}
# Files outside viewer/ that name a path under it today (a few only as message text). A new reader of a website
# file must be added here, and then its files must leave the viewer scope.
NAMES_VIEWER_PATHS = {".github/workflows/viewer.yml", ".github/workflows/ci.yml", "tools/ci/ci_scope.py",
                      "tools/ci/tests/test_ci_scope.py", "tools/ci/tests/test_draft_light.py",
                      "python/tools/limited_call_session_v8.py", "python/tools/limited_end_turn_session_v9.py",
                      "python/tools/limited_genesis_wave_session_v10.py"}


def relevant(path):
    """(python_relevant, rust_relevant) of one unlabelled pull request path (ci_scope.relevance, which the classify
    step and the dispatch step's pr-run both call)."""
    return ci_scope.relevance([path])


# What part of the matrix a job covers in each group set: the pull request set runs Linux and the pull request's
# Windows groups, the complement only the Windows groups that set leaves out, a draft only Linux lint.
PARTS = {"full": ("linux", "windows-pr", "windows-rest"), "pr": ("linux", "windows-pr"),
         "windows-complement": ("windows-rest",), "draft": ("linux",)}


def coverage(scheduled):
    return {(job, part) for job, group_set in scheduled for part in PARTS[group_set]}


class Classifier:
    """The classifier under test (version "new"); at faf2b093b ("faf": the same scope table and relevance, but a
    default dispatch complemented every pull request whose scope was pr); or origin/main's before the viewer scope
    ("main": no viewer rule, docs the only neutral scope, no viewer/* relevance rule, ci:full leaving the relevance
    flags to the paths, and the faf dispatch rule)."""

    def __init__(self, version="new"):
        self.version = version

    def _main_tables(self):
        saved = ci_scope.PATH_RULES, ci_scope.NEUTRAL, ci_scope.RUST_RELEVANT_RULES
        ci_scope.PATH_RULES = [rule for rule in ci_scope.PATH_RULES if rule[1] != "viewer"]
        ci_scope.NEUTRAL = {"docs"}
        ci_scope.RUST_RELEVANT_RULES = [(tuple(p for p in patterns if p != "viewer/*"), value)
                                        for patterns, value in ci_scope.RUST_RELEVANT_RULES]
        return saved

    def _call(self, function, *args):
        if self.version != "main":
            return function(*args)
        saved = self._main_tables()
        try:
            return function(*args)
        finally:
            ci_scope.PATH_RULES, ci_scope.NEUTRAL, ci_scope.RUST_RELEVANT_RULES = saved

    def scope_of_files(self, paths):
        return self._call(ci_scope.scope_of_files, list(paths))

    def flags(self, paths, label):
        """detect-changes' python_relevant and rust_relevant for a pull request."""
        return self._call(ci_scope.relevance, list(paths), label and self.version != "main")

    def dispatch_group_set(self, pr_paths):
        """The group set of a dispatch without inputs at the head of a ready same-repository pull request with these
        files ("Choose the group set"; test_draft_light runs the real step against this)."""
        if self.version == "new":
            return "windows-complement" if ci_scope.pr_run_is_whole_set(list(pr_paths)) else "full"
        return "windows-complement" if self.scope_of_files(pr_paths) == "pr" else "full"

    def checks(self, event, group_set, input_scope, labels, paths):
        return self._call(ci_scope.choose, event, group_set, input_scope, labels, list(paths))

    def scheduled(self, event, paths=(), *, draft=False, label=False, input_scope="", pr_paths=None):
        """{(ci.yml job, group set)} whose job condition holds. event: pull_request (ready, draft, labelled),
        workflow_dispatch (with a scope input, or without inputs at the head of a ready pull request with pr_paths,
        or None for no such pull request) or push (main and tags). detect-changes and the jobs that react to other
        jobs' failures are left out."""
        sys.path.insert(0, str(Path(__file__).resolve().parent))
        from test_runner_routing import evaluate, truthy
        if event == "pull_request":
            group_set = "draft" if draft and not label else "pr"
            python, rust = self.flags(paths, label)
            checks = self.checks(event, group_set, "", ["ci:full"] if label else [], paths)
        elif event == "workflow_dispatch":
            group_set = "full" if input_scope or pr_paths is None else self.dispatch_group_set(pr_paths)
            python = rust = True
            checks = self.checks(event, group_set, input_scope, [], [])
        else:
            group_set, python, rust, checks = "full", True, True, self.checks(event, "full", "", [], [])
        context = {"needs.detect-changes.outputs.checks": checks, "needs.detect-changes.outputs.scope": group_set,
                   "needs.detect-changes.outputs.python_relevant": str(python).lower(),
                   "needs.detect-changes.outputs.rust_relevant": str(rust).lower()}
        ran = set()
        for name, spec in workflow()["jobs"].items():
            if name in ("detect-changes", "hosted-fallback", "post-merge-report"):
                continue
            condition = spec["if"].removeprefix("always() && ")
            # Only detect-changes outputs decide these jobs; anything else would make this model wrong.
            contexts = re.findall(r"\b(?:needs|github|vars|inputs|matrix|steps|env)\.[\w.\-*]+", condition)
            assert condition and all(c.startswith("needs.detect-changes.outputs.") for c in contexts), (name, condition)
            if truthy(evaluate(condition, **context)):
                ran.add((name, group_set))
        return ran

    def composition(self, paths, label=False):
        """What a ready pull request's run and a default dispatch at its head schedule together."""
        return self.scheduled("pull_request", paths, label=label) | self.scheduled("workflow_dispatch", pr_paths=paths)


def without_website(change):
    """The change without its website files; a website-only change is compared with a docs-only one (an empty file
    list fails closed and runs everything)."""
    return [path for path in change if not path.startswith("viewer/")] or ["README.md"]


def classify_step():
    text = WORKFLOW.read_text(encoding="utf-8")
    return text.split("      - name: Classify changed paths\n", 1)[1].split("\n      - name: ", 1)[0]


def jobs_that_run(paths, **kwargs):
    return {job for job, _ in Classifier().scheduled("pull_request", paths, **kwargs)}


def viewer_workflow_trigger(event):
    text = VIEWER_WORKFLOW.read_text(encoding="utf-8")
    triggers = text[text.index("\non:\n"):text.index("\npermissions:")]
    return re.search(rf"^  {event}:\n((?:    .*\n)+)", triggers, re.MULTILINE)[1]


def viewer_workflow_paths(event):
    """The `paths` filter of one trigger of viewer.yml, as regular expressions (GitHub globs: `**` crosses `/`)."""
    block = viewer_workflow_trigger(event)
    globs = [g.strip().strip("'\"") for g in re.search(r"^    paths: \[(.*)\]$", block, re.MULTILINE)[1].split(",")]
    return [re.compile("".join(".*" if part == "**" else "[^/]*" if part == "*" else re.escape(part)
                               for part in re.split(r"(\*\*|\*)", glob))) for glob in globs]


class ViewerScopeTests(unittest.TestCase):
    def test_website_changes_classify_as_viewer_and_schedule_no_rust_or_python_job(self):
        self.assertEqual(ci_scope.SCOPES["viewer"]["jobs"], [])
        for path in WEBSITE:
            with self.subTest(path=path):
                self.assertEqual(ci_scope.scope_of(path), "viewer")
                self.assertEqual(relevant(path), (False, False))
                self.assertEqual(jobs_that_run([path]), set())
        for paths in (WEBSITE, WEBSITE + ["README.md"], WEBSITE + ["docs/a.md", "viewer/README.md"]):
            with self.subTest(paths=paths):
                self.assertEqual(ci_scope.scope_of_files(paths), "viewer")
                self.assertEqual(ci_scope.choose("pull_request", "pr", "", [], paths), "viewer")
                self.assertEqual(jobs_that_run(paths), set())
                self.assertEqual(jobs_that_run(paths, draft=True), set())
        self.assertEqual(ci_scope.scope_of_files(["README.md"]), "docs")
        # Pull request 951 exactly.
        self.assertEqual(jobs_that_run(["README.md"] + WEBSITE[:5]), set())
        # The model is not vacuous: an engine file runs the Rust set, a Python tool the Python set.
        self.assertEqual(jobs_that_run(["mtg-kernel/src/engine.rs"]), RUST_SET)
        self.assertEqual(jobs_that_run(["python/tools/foo.py"]), PYTHON_SET)

    def test_website_plus_other_files_classifies_exactly_as_the_other_files(self):
        new = Classifier()
        for other in OTHERS:
            mixed = WEBSITE + other
            with self.subTest(other=other):
                alone = ci_scope.scope_of_files(other)
                self.assertEqual(ci_scope.scope_of_files(mixed), "viewer" if alone == "docs" else alone)
                for kwargs in ({}, {"draft": True}, {"label": True}):
                    self.assertEqual(new.scheduled("pull_request", mixed, **kwargs),
                                     new.scheduled("pull_request", other, **kwargs), kwargs)
                self.assertEqual(new.dispatch_group_set(mixed), new.dispatch_group_set(other))

    def test_a_pull_request_run_and_a_default_dispatch_together_cover_what_main_scheduled(self):
        # For every change, website files included: the pull request run plus a dispatch without inputs at its head
        # cover never less of the matrix than origin/main's classifier did for the SAME change, and schedule the
        # same with or without the website files.
        new, main = Classifier(), Classifier("main")
        for change in SAME_CHANGE:
            for label in (False, True):
                with self.subTest(change=change, label=label):
                    self.assertLessEqual(coverage(main.composition(change, label)),
                                         coverage(new.composition(change, label)))
                    others = without_website(change)
                    self.assertEqual(new.composition(change, label), new.composition(others, label))
        # The pull request run alone covers less than main's only where main's run grew because of the website
        # files (website only, or with docs, Python, tools, ci-tools or workflow files: they made it Rust-relevant
        # or mixed). That drop is the purpose of the scope: viewer.yml tests the website, and the rest is covered
        # only by a dispatch, which at such a head is a full one (website only included: no Rust job runs unless
        # someone dispatches).
        lost_somewhere = []
        for change in SAME_CHANGE:
            others = without_website(change)
            with self.subTest(change=change):
                now = coverage(new.scheduled("pull_request", change))
                self.assertLessEqual(coverage(main.scheduled("pull_request", others)), now)
                if coverage(main.scheduled("pull_request", change)) - now:
                    lost_somewhere.append(change)
                    self.assertEqual(new.dispatch_group_set(change), "full")
        self.assertIn(WEBSITE, lost_somewhere)
        self.assertIn(WEBSITE + ["tools/ci/hosted_fallback.py"], lost_somewhere)
        # Reviewer's cases: website plus a Python tool, and website plus a ci tool (scope pr, not Rust-relevant).
        for change in (WEBSITE + ["python/tools/foo.py"], WEBSITE + ["tools/ci/hosted_fallback.py"],
                       ["tools/ci/hosted_fallback.py"]):
            with self.subTest(change=change):
                self.assertEqual(new.dispatch_group_set(change), "full")
                self.assertEqual(main.dispatch_group_set(change), "windows-complement")
        # The snapshot of main is not the classifier under test: there the website ran the Rust set.
        self.assertEqual({job for job, _ in main.scheduled("pull_request", WEBSITE)}, RUST_SET)

    def test_a_dispatch_complements_only_a_pull_request_run_that_scheduled_the_whole_set(self):
        new = Classifier()
        for change in SAME_CHANGE:
            with self.subTest(change=change):
                if new.dispatch_group_set(change) == "windows-complement":
                    self.assertLessEqual({(job, "pr") for job in RUST_SET}, new.scheduled("pull_request", change))
                else:
                    self.assertFalse(ci_scope.pr_run_is_whole_set(change))
        self.assertEqual(new.dispatch_group_set(["mtg-kernel/src/engine.rs"]), "windows-complement")
        self.assertEqual(new.dispatch_group_set(["mtg-kernel/src/native_expanded_training_run_v1.rs"]), "full")

    def test_every_other_event_schedules_what_it_did_at_faf2b093b(self):
        # Only a default dispatch at a ready pull request's head changed since faf2b093b, and only for changes
        # whose pull request run scheduled no Rust job.
        new, faf = Classifier(), Classifier("faf")
        for change in SAME_CHANGE:
            with self.subTest(change=change):
                for kwargs in ({}, {"draft": True}, {"label": True}):
                    self.assertEqual(new.scheduled("pull_request", change, **kwargs),
                                     faf.scheduled("pull_request", change, **kwargs))
                for scope in ("", "trainer", "python"):
                    self.assertEqual(new.scheduled("workflow_dispatch", input_scope=scope),
                                     faf.scheduled("workflow_dispatch", input_scope=scope))
                if new.dispatch_group_set(change) != faf.dispatch_group_set(change):
                    self.assertEqual(new.flags(change, False)[1], False)
                    self.assertEqual(faf.scope_of_files(change), "pr")
        self.assertEqual(new.scheduled("push"), faf.scheduled("push"))

    def test_ci_full_runs_every_rust_and_python_job_whatever_the_paths(self):
        new, main = Classifier(), Classifier("main")
        for paths in (WEBSITE, WEBSITE + ["README.md"], ["README.md"], ["python/tools/foo.py"], ["mtg-kernel/src/engine.rs"]):
            with self.subTest(paths=paths):
                self.assertEqual(new.scheduled("pull_request", paths, label=True), FULL_PR)
                self.assertEqual(new.scheduled("pull_request", paths, draft=True, label=True), FULL_PR)
        self.assertEqual(new.scheduled("pull_request", WEBSITE), set())
        self.assertEqual(new.scheduled("pull_request", WEBSITE, draft=True), set())
        self.assertEqual(new.scheduled("pull_request", ["mtg-kernel/src/engine.rs"], draft=True), {("rust-quality", "draft")})
        # On origin/main the label left the flags to the paths: a labelled docs-only change ran nothing.
        self.assertEqual(main.scheduled("pull_request", ["README.md"], label=True), set())
        self.assertIn("FULL_LABEL: ${{ contains(github.event.pull_request.labels.*.name, 'ci:full') }}",
                      classify_step())

    def test_other_events_are_unchanged_by_the_scope(self):
        new = Classifier()
        full = {(job, "full") for job in RUST_SET | PYTHON_SET}
        self.assertEqual(new.scheduled("push"), full)
        self.assertEqual(new.scheduled("workflow_dispatch"), full)
        self.assertEqual(new.scheduled("workflow_dispatch", pr_paths=WEBSITE), full)
        self.assertEqual(new.scheduled("workflow_dispatch", input_scope="viewer"), full)   # not a dispatch scope
        self.assertEqual(new.scheduled("workflow_dispatch", input_scope="python"),
                         {("python-env-build", "full"), ("python-tests", "full")})
        self.assertEqual({job for job, _ in new.scheduled("workflow_dispatch", pr_paths=["mtg-kernel/src/engine.rs"])},
                         RUST_SET - {"rust-quality"})

    def test_paths_that_only_start_with_viewer_are_not_the_website(self):
        for path in LOOKALIKES:
            with self.subTest(path=path):
                self.assertNotEqual(ci_scope.scope_of(path), "viewer")
                self.assertNotEqual(ci_scope.choose("pull_request", "pr", "", [], [path]), "viewer")
        for path in ("viewer", "viewer_tools/x.py", "viewerx/index.html", "mtg-kernel/src/viewer.rs",
                     "mtg-kernel/src/bin/viewer_capture_v1.rs"):
            with self.subTest(path=path):
                self.assertEqual(relevant(path)[1], True)
                self.assertEqual(jobs_that_run([path]), RUST_SET)
        self.assertEqual(ci_scope.scope_of("python/tools/viewer_export_v1.py"), "python")
        self.assertEqual(ci_scope.scope_of("python/tests/test_viewer_uct_v1.py"), "python")

    def test_relevance_has_one_copy_and_agrees_with_the_scope_table(self):
        # The workflow keeps no copy of the relevance patterns: both steps call ci_scope.py.
        step = classify_step()
        self.assertNotIn("case ", step)
        self.assertIn('python3 tools/ci/ci_scope.py relevance < "$RUNNER_TEMP/ci-changed-paths"', step)
        self.assertNotIn("rust_relevant=true ;;", WORKFLOW.read_text(encoding="utf-8"))
        self.assertIn('pr_files "$pr_number" | python3 tools/ci/ci_scope.py pr-run', WORKFLOW.read_text(encoding="utf-8"))
        # A path no rule names is Rust-relevant.
        self.assertEqual(relevant("Cargo.lock"), (False, True))
        self.assertEqual(relevant("some/new/file"), (False, True))
        samples = WEBSITE + LOOKALIKES + [path for other in OTHERS for path in other] + [
            "viewer/README.md", "python/README.md", "tools/reach/plan.py", "python/tools/run_rust_lib_filters.py",
            "python/mtg_kernel_rl/features.py", "mtg-kernel/src/rl_session.rs", "mtg-kernel/build.rs", "pyproject.toml",
            ".github/workflows/w5-tools.yml", ".github/actions/x/action.yml", "tools/ci/ci_scope.py"]
        for path in samples:
            python, rust = relevant(path)
            scope = ci_scope.scope_of(path)
            with self.subTest(path=path, scope=scope, python=python, rust=rust):
                if scope == "viewer":
                    self.assertEqual((python, rust), (False, False))
                if not python and not rust:
                    # Nothing can run for it, so the table must name a neutral scope (one that never decides).
                    self.assertIn(scope, ci_scope.NEUTRAL)
                if scope in ("python", "tools"):
                    self.assertTrue(python)
                if scope in ("trainer", "shared", "rules-limited", None):
                    self.assertTrue(rust)

    def test_the_viewer_workflow_runs_on_every_website_path(self):
        (patterns,) = [patterns for patterns, scope in ci_scope.PATH_RULES if scope == "viewer"]
        # A deep path under each pattern of the scope, plus the samples.
        paths = WEBSITE + ["viewer/README.md"] + [p.replace("*", "a/b/c.html") for p in patterns]
        for event in ("pull_request", "push"):
            filters = viewer_workflow_paths(event)
            for path in paths:
                with self.subTest(event=event, path=path):
                    self.assertIn(ci_scope.scope_of(path), ("viewer", "docs"))
                    self.assertTrue(any(f.fullmatch(path) for f in filters), path)
            # The conversion is not vacuous.
            self.assertFalse(any(f.fullmatch("viewer_tools/x.py") for f in filters))
        self.assertIn("  push:\n    branches: [main]\n", VIEWER_WORKFLOW.read_text(encoding="utf-8"))

    def test_the_viewer_workflow_verifies_every_pull_request_it_triggers_on(self):
        # Only a paths filter: no branch filter, paths-ignore or activity types narrow the pull request trigger.
        keys = re.findall(r"^    ([a-z-]+):", viewer_workflow_trigger("pull_request"), re.MULTILINE)
        self.assertEqual(keys, ["paths"])
        text = VIEWER_WORKFLOW.read_text(encoding="utf-8")
        jobs = re.split(r"^  ([a-z][a-z-]*):\n", text[text.index("\njobs:\n"):], flags=re.MULTILINE)
        verify = dict(zip(jobs[1::2], jobs[2::2]))["verify"]
        self.assertNotRegex(verify, r"(?m)^    if:")
        self.assertEqual(re.findall(r"(?m)^        if: (.*)$", verify), ["always()"])

    def test_only_pinned_files_name_a_path_under_viewer(self):
        import subprocess
        listing = subprocess.run(["git", "ls-files", "-z"], cwd=ROOT, check=True, capture_output=True).stdout
        reference = re.compile(r"""(?:^|[\s"'`(=\[,:])viewer/|\.\./viewer\b|/\s*["']viewer["']""", re.MULTILINE)
        found = set()
        for rel in listing.decode("utf-8", errors="surrogateescape").split("\0"):
            if not rel or rel.startswith("viewer/"):
                continue
            if not (rel.startswith(".github/") or rel.endswith((".rs", ".py", ".toml", ".sh", ".ps1", ".yml", ".yaml"))):
                continue
            path = ROOT / rel
            if path.is_file() and reference.search(path.read_text(encoding="utf-8", errors="replace")):
                found.add(rel)
        self.assertEqual(found, NAMES_VIEWER_PATHS)


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


SCOPE_SCRIPT = ROOT / "tools" / "ci" / "ci_scope.py"


def run_scope(*args, stdin=b"", env=None, prelude=""):
    """Run ci_scope.py's CLI in a fresh interpreter; prelude (Python) runs after the import, to break it."""
    import subprocess
    code = (f"import sys; sys.path.insert(0, {str(SCOPE_SCRIPT.parent)!r}); import ci_scope\n{prelude}\n"
            f"sys.exit(ci_scope.main(['ci_scope.py', *{list(args)!r}]))")
    environment = {k: v for k, v in os.environ.items() if k not in ("GITHUB_OUTPUT", "FULL_LABEL")}
    environment.update(env or {})
    return subprocess.run([sys.executable, "-c", code], input=stdin, env=environment, capture_output=True)


class RelevanceCliTests(unittest.TestCase):
    """Condition 1: both outputs or neither, as the last act. Condition 2: any error exits non-zero."""

    def relevance(self, stdin, prelude="", label=None, output=True):
        import tempfile
        with tempfile.TemporaryDirectory() as tmp:
            out = Path(tmp) / "output"
            out.write_text("before=1\n", encoding="utf-8")
            env = {"GITHUB_OUTPUT": str(out)} if output else {}
            if label:
                env["FULL_LABEL"] = label
            result = run_scope("relevance", stdin=stdin, env=env, prelude=prelude)
            return result, out.read_text(encoding="utf-8")

    def test_relevance_writes_both_outputs_in_one_write_as_its_last_act(self):
        import io
        import unittest.mock
        writes = []

        class Recorder(io.StringIO):
            def write(self, text):
                writes.append(text)
                return len(text)

        stdin = unittest.mock.Mock()
        stdin.buffer = io.BytesIO(b"mtg-kernel/src/engine.rs\0python/tools/foo.py\0")
        with unittest.mock.patch.dict(os.environ, {"GITHUB_OUTPUT": "out", "FULL_LABEL": "false"}), \
                unittest.mock.patch.object(sys, "stdin", stdin), \
                unittest.mock.patch("builtins.open", lambda *a, **k: Recorder()), \
                unittest.mock.patch("sys.stdout", io.StringIO()):
            self.assertEqual(ci_scope.main(["ci_scope.py", "relevance"]), 0)
        self.assertEqual(writes, ["python_relevant=true\nrust_relevant=true\n"])

    def test_relevance_crashing_midway_exits_non_zero_and_writes_nothing(self):
        # Breaks the Rust rules: the crash comes after python relevance of the first path is computed.
        result, written = self.relevance(b"python/tools/foo.py\0", prelude="ci_scope.RUST_RELEVANT_RULES = None")
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(written, "before=1\n")
        result, written = self.relevance(b"a\0", prelude="def boom(*a): raise RuntimeError('x')\nci_scope.relevance = boom")
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(written, "before=1\n")

    def test_relevance_without_github_output_exits_non_zero(self):
        result, written = self.relevance(b"mtg-kernel/src/engine.rs\0", output=False)
        self.assertEqual(result.returncode, 2)
        self.assertIn(b"GITHUB_OUTPUT is not set", result.stderr)

    def test_an_empty_or_unreadable_path_list_fails_closed(self):
        for stdin in (b"", b"\0", b"\0\0"):
            with self.subTest(stdin=stdin):
                result, written = self.relevance(stdin)
                self.assertEqual(result.returncode, 0)
                self.assertEqual(written, "before=1\npython_relevant=true\nrust_relevant=true\n")
                self.assertIn(b"fail closed", result.stdout)
        self.assertEqual(Classifier().scheduled("pull_request", []), FULL_PR)
        self.assertEqual(ci_scope.choose("pull_request", "pr", "", [], []), "all")

    def test_odd_path_bytes_classify_like_the_shell_case(self):
        # Not UTF-8, or a newline inside a path: shell case `*` matches any byte, newlines included.
        for stdin, expected in ((b"mtg-kernel/\xff.rs\0", ("false", "true")), (b"viewer/\xff.html\0", ("false", "false")),
                                (b"docs/a\nb.rs\0", ("false", "false")), (b"a.md\nmtg-kernel/x.rs\0", ("false", "true")),
                                (b"tools/\xfe\ny.py\0", ("true", "false"))):
            with self.subTest(stdin=stdin):
                result, written = self.relevance(stdin)
                self.assertEqual(result.returncode, 0)
                python, rust = expected
                self.assertEqual(written, f"before=1\npython_relevant={python}\nrust_relevant={rust}\n")

    def test_bad_arguments_exit_non_zero(self):
        for args in (["relevance", "extra"], ["pr-run", "extra"], ["file-scope", "extra"], ["no-such-command"], []):
            with self.subTest(args=args):
                result = run_scope(*args, env={"GITHUB_OUTPUT": os.devnull})
                self.assertEqual(result.returncode, 2)
                self.assertNotIn(b"whole-set", result.stdout)

    def test_pr_run_prints_whole_set_only_when_every_check_passed(self):
        engine = b'[[{"status": "modified", "filename": "mtg-kernel/src/engine.rs"}]]'
        self.assertEqual(run_scope("pr-run", stdin=engine).stdout, b"whole-set\n")
        for stdin, prelude in ((b"[[]]", ""), (b"", ""), (b"{", ""), (b"[[{}]]", ""), (b"\xff", ""),
                               (engine, "def boom(*a): raise RuntimeError('x')\nci_scope.relevance = boom"),
                               (engine, "ci_scope.RUST_RELEVANT_RULES = None")):
            with self.subTest(stdin=stdin, prelude=prelude):
                result = run_scope("pr-run", stdin=stdin, prelude=prelude)
                self.assertNotIn(b"whole-set", result.stdout)
                if stdin != b"[[]]":
                    self.assertNotEqual(result.returncode, 0)
        self.assertEqual(run_scope("pr-run", stdin=b"[[]]").stdout, b"partial\n")


class NonPullRequestEventTests(unittest.TestCase):
    """Condition 3: pushes, tags and dispatches set both outputs themselves, before the classifier can run."""

    def test_the_non_pull_request_branch_writes_both_outputs_and_exits_before_the_classifier(self):
        step = classify_step()
        branch = step[step.index('if [ "${{ github.event_name }}" != "pull_request" ]; then'):]
        branch = branch[:branch.index("\n          fi\n")]
        self.assertEqual(re.findall(r"^            (.*)$", branch, re.MULTILINE),
                         ['echo "python_relevant=true" >> "$GITHUB_OUTPUT"',
                          'echo "rust_relevant=true" >> "$GITHUB_OUTPUT"', "exit 0"])
        self.assertLess(step.index(branch), step.index("ci_scope.py"))

    def test_the_real_step_never_calls_the_classifier_for_other_events(self):
        import subprocess
        import tempfile
        sys.path.insert(0, str(Path(__file__).resolve().parent))
        from test_draft_light import BASH, workflow_step
        for event in ("push", "workflow_dispatch", "schedule"):
            with self.subTest(event=event), tempfile.TemporaryDirectory() as tmp:
                out = Path(tmp) / "output"
                script = workflow_step("Classify changed paths").replace("${{ github.event_name }}", event).replace(
                    "python3 tools/ci/ci_scope.py", "exit 97; :")
                result = subprocess.run([BASH, "-eo", "pipefail", "-c", script], cwd=tmp, capture_output=True,
                                        env=dict(os.environ, GITHUB_OUTPUT=Path(out).as_posix(),
                                                 RUNNER_TEMP=Path(tmp).as_posix(), FULL_LABEL="false"))
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(out.read_text(encoding="utf-8"), "python_relevant=true\nrust_relevant=true\n")
        # The replacement is not vacuous: a pull request reaches it.
        with tempfile.TemporaryDirectory() as tmp:
            script = workflow_step("Classify changed paths").replace("${{ github.event_name }}", "pull_request").replace(
                "python3 tools/ci/ci_scope.py", "exit 97; :").replace("git diff", ": git diff")
            result = subprocess.run([BASH, "-eo", "pipefail", "-c", script], cwd=tmp, capture_output=True,
                                    env=dict(os.environ, GITHUB_OUTPUT=(Path(tmp) / "o").as_posix(),
                                             RUNNER_TEMP=Path(tmp).as_posix()))
            self.assertEqual(result.returncode, 97)


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
