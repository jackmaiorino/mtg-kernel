"""The workflow's runs-on expressions send only trusted events to self-hosted runners.

The expressions are evaluated here with a small evaluator for the part of GitHub's expression language
they use (&&, ||, ==, !=, parentheses, string literals, context lookups, fromJSON, contains, startsWith, endsWith
and format).
"""
import json
import re
import unittest
from pathlib import Path

WORKFLOW = Path(__file__).resolve().parents[3] / ".github" / "workflows" / "ci.yml"
TOKEN = re.compile(r"\s*(&&|\|\||==|!=|[(),]|'(?:[^']|'')*'|[A-Za-z_][\w.\-]*)")
REPOSITORY = "owner/project"
WINDOWS_POOL, PR_POOL, LINUX_POOL = ["self-hosted", "windows-pool"], ["self-hosted", "pr-pool"], ["self-hosted", "linux-pool"]


def truthy(value):
    return value not in ("", None, False, 0)


def equal(left, right):
    if isinstance(left, str) and isinstance(right, str):
        return left.lower() == right.lower()
    return left == right


class Evaluator:
    def __init__(self, text, context):
        self.tokens, self.context, self.at = TOKEN.findall(text), context, 0
        if "".join(self.tokens) != re.sub(r"\s+(?=(?:[^']*'[^']*')*[^']*$)", "", text):
            raise ValueError("unsupported expression syntax")

    def take(self, expected=None):
        token = self.tokens[self.at]
        if expected is not None and token != expected:
            raise ValueError(f"expected {expected}, found {token}")
        self.at += 1
        return token

    def peek(self):
        return self.tokens[self.at] if self.at < len(self.tokens) else None

    # Each parse method returns a thunk, so && and || short-circuit as GitHub's do: the right side of
    # `a && fromJSON(b)` is never evaluated while a is falsy.
    def either(self):
        value = self.both()
        while self.peek() == "||":
            self.take()
            value = (lambda left, right: lambda: left() if truthy(left()) else right())(value, self.both())
        return value

    def both(self):
        value = self.comparison()
        while self.peek() == "&&":
            self.take()
            value = (lambda left, right: lambda: right() if truthy(left()) else left())(value, self.comparison())
        return value

    def comparison(self):
        value = self.operand()
        while self.peek() in ("==", "!="):
            negate = self.take() == "!="
            value = (lambda left, right, negate: lambda: equal(left(), right()) != negate)(value, self.operand(), negate)
        return value

    def operand(self):
        token = self.take()
        if token == "(":
            value = self.either()
            self.take(")")
            return value
        if token.startswith("'"):
            return lambda: token[1:-1].replace("''", "'")
        if self.peek() == "(":
            self.take("(")
            thunks = [self.either()]
            while self.peek() == ",":
                self.take()
                thunks.append(self.either())
            self.take(")")
            if token not in ("fromJSON", "format", "contains", "startsWith", "endsWith"):
                raise ValueError(f"unsupported function {token}")
            return lambda: call(token, [thunk() for thunk in thunks])
        return lambda: self.context.get(token, "")


def call(name, arguments):
    if name == "fromJSON":
        return json.loads(arguments[0])
    if name == "format":
        return re.sub(r"\{(\d+)\}", lambda match: str(arguments[1 + int(match.group(1))]), arguments[0])
    if isinstance(arguments[0], str):
        # GitHub compares strings case-insensitively; a missing value is ''.
        search, item = arguments[0].lower(), str(arguments[1]).lower()
        return {"contains": item in search, "startsWith": search.startswith(item), "endsWith": search.endswith(item)}[name]
    if name == "contains":
        return arguments[1] in arguments[0]
    raise ValueError(f"{name} of a non-string")


def evaluate(expression, **context):
    evaluator = Evaluator(expression, context)
    value = evaluator.either()
    if evaluator.peek() is not None:
        raise ValueError("trailing tokens")
    return value()


def runs_on_expressions():
    """{job name: runs-on expression} for every job whose runner is chosen by an expression."""
    text = WORKFLOW.read_text(encoding="utf-8")
    jobs = re.split(r"^  ([a-z][a-z-]*):\n", text[text.index("\njobs:\n"):], flags=re.MULTILINE)
    found = {}
    for name, body in zip(jobs[1::2], jobs[2::2]):
        match = re.search(r"^    runs-on: \$\{\{ (.*) \}\}$", body, re.MULTILINE)
        if match:
            found[name] = match.group(1)
    return found


def event(name="pull_request", head=REPOSITORY, **more):
    context = {"github.event_name": name, "github.repository": REPOSITORY}
    if name == "pull_request":
        context["github.event.pull_request.head.repo.full_name"] = head
    return {**context, **more}


EVERY_POOL = {"vars.MTG_CI_WINDOWS_RUNNER": json.dumps(WINDOWS_POOL), "vars.MTG_CI_PR_WINDOWS_RUNNER": json.dumps(PR_POOL),
              "vars.MTG_CI_PR_WINDOWS_GROUPS": '["workspace-targets", "eternal"]',
              "vars.MTG_CI_LINUX_RUNNER": json.dumps(LINUX_POOL), "vars.MTG_CI_LINUX_GROUPS": '["eternal", "workspace-targets"]'}


# Jobs on MTG_CI_DETECT_RUNNER: light, generic jobs whose runner does not depend on the matrix.
LIGHT_JOBS = ("detect-changes", "rust-tests")
DETECT_POOL = ["self-hosted", "fleet-mac-linux"]
OWNER = "maintainer"
LIGHT = {"vars.MTG_CI_DETECT_RUNNER": json.dumps(DETECT_POOL), "vars.MTG_CI_SELF_HOSTED_ACTORS": json.dumps([OWNER, "helper"])}


def by(actor, triggering=None, **context):
    return {"github.actor": actor, "github.triggering_actor": triggering or actor, **context}


class EvaluatorTests(unittest.TestCase):
    def test_operators_follow_github_semantics(self):
        self.assertEqual(evaluate("a == 'x' && fromJSON(b) || c", a="X", b='["p", "q"]', c="hosted"), ["p", "q"])
        self.assertEqual(evaluate("a == 'x' && fromJSON(b) || c", a="y", b='"p"', c="hosted"), "hosted")
        self.assertEqual(evaluate("missing || 'it''s'"), "it's")
        self.assertTrue(evaluate("contains(fromJSON(list || '[]'), item) && (a != 'b')", list='["k"]', item="k", a="c"))
        self.assertFalse(evaluate("contains(fromJSON(list || '[]'), item)", item="k"))
        with self.assertRaises(ValueError):
            evaluate("a ? b : c")

    def test_and_or_short_circuit_and_string_functions(self):
        self.assertEqual(evaluate("a && fromJSON(b) || 'hosted'", b="not json"), "hosted")
        with self.assertRaises(ValueError):
            evaluate("a && fromJSON(b) || 'hosted'", a="x", b="not json")
        self.assertTrue(evaluate("startsWith(a, '[') && endsWith(a, ']') && contains(a, format('\"{0}\"', b))",
                                 a='["Who"]', b="who"))
        self.assertFalse(evaluate("startsWith(a, '[')"))


class RoutingTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.expressions = runs_on_expressions()
        cls.groups = cls.expressions["rust-test-groups"]

    def route(self, system, group, **context):
        return evaluate(self.groups, **{"matrix.os": system, "matrix.group": group, **context})

    def test_every_expression_that_reads_a_setting_is_found(self):
        self.assertEqual(sorted(self.expressions), ["detect-changes", "limited-build", "python-env-build", "python-tests",
                                                    "rust-limited-groups", "rust-test-groups", "rust-tests"])

    def test_a_fork_pull_request_always_runs_on_hosted_runners(self):
        fork = event(head="someone/fork", **EVERY_POOL)
        for name, expression in self.expressions.items():
            if name in LIGHT_JOBS:
                continue
            for system in ("ubuntu-24.04", "windows-2025"):
                for group in ("eternal", "workspace-targets", "standard"):
                    with self.subTest(job=name, system=system, group=group):
                        self.assertEqual(evaluate(expression, **{"matrix.os": system, "matrix.group": group, **fork}),
                                         system)

    def test_named_linux_groups_go_to_the_linux_runner_for_trusted_events(self):
        for trusted in (event(), event("push"), event("workflow_dispatch")):
            with self.subTest(event=trusted["github.event_name"]):
                self.assertEqual(self.route("ubuntu-24.04", "eternal", **trusted, **EVERY_POOL), LINUX_POOL)
                self.assertEqual(self.route("ubuntu-24.04", "standard", **trusted, **EVERY_POOL), "ubuntu-24.04")

    def test_linux_stays_hosted_unless_both_settings_are_present(self):
        cases = [{}, {"vars.MTG_CI_LINUX_GROUPS": '["eternal"]'}, {"vars.MTG_CI_LINUX_RUNNER": json.dumps(LINUX_POOL)},
                 {"vars.MTG_CI_LINUX_GROUPS": "[]", "vars.MTG_CI_LINUX_RUNNER": json.dumps(LINUX_POOL)},
                 {"vars.MTG_CI_WINDOWS_RUNNER": json.dumps(WINDOWS_POOL), "vars.MTG_CI_PR_WINDOWS_RUNNER": json.dumps(PR_POOL),
                  "vars.MTG_CI_PR_WINDOWS_GROUPS": '["eternal"]', "inputs.windows_runner": json.dumps(WINDOWS_POOL)}]
        for settings in cases:
            with self.subTest(settings=settings):
                self.assertEqual(self.route("ubuntu-24.04", "eternal", **event(), **settings), "ubuntu-24.04")

    def test_linux_settings_do_not_move_windows_groups(self):
        linux_only = {key: value for key, value in EVERY_POOL.items() if "LINUX" in key}
        self.assertEqual(self.route("windows-2025", "eternal", **event(), **linux_only), "windows-2025")
        self.assertEqual(self.route("windows-2025", "eternal", **event("push"), **linux_only), "windows-2025")

    def test_the_values_set_today_route_what_they_routed_before(self):
        # Since 2026-11-18 08:45 ET: the whole Eternal integration suite (one job again) to the Linux pool,
        # workspace-lib of a pull request to the pull request Windows pool.
        today = {"vars.MTG_CI_LINUX_GROUPS": '["eternal-integration"]', "vars.MTG_CI_LINUX_RUNNER": json.dumps(LINUX_POOL),
                 "vars.MTG_CI_PR_WINDOWS_GROUPS": '["workspace-lib"]', "vars.MTG_CI_PR_WINDOWS_RUNNER": json.dumps(PR_POOL)}
        self.assertEqual(self.route("ubuntu-24.04", "eternal-integration", **event(), **today), LINUX_POOL)
        self.assertEqual(self.route("ubuntu-24.04", "eternal", **event(), **today), "ubuntu-24.04")
        self.assertEqual(self.route("windows-2025", "workspace-lib", **event(), **today), PR_POOL)
        for part in ("native-store", "native-store-1"):
            with self.subTest(part=part):
                self.assertEqual(self.route("windows-2025", part, **event(), **today), "windows-2025")

    def test_a_group_run_as_parts_routes_each_part_by_its_own_name(self):
        first = {"vars.MTG_CI_PR_WINDOWS_GROUPS": '["native-store"]', "vars.MTG_CI_PR_WINDOWS_RUNNER": json.dumps(PR_POOL)}
        self.assertEqual(self.route("windows-2025", "native-store", **event(), **first), PR_POOL)
        self.assertEqual(self.route("windows-2025", "native-store-1", **event(), **first), "windows-2025")
        both = {**first, "vars.MTG_CI_PR_WINDOWS_GROUPS": '["native-store", "native-store-1"]'}
        self.assertEqual(self.route("windows-2025", "native-store-1", **event(), **both), PR_POOL)

    def test_named_limited_groups_go_to_the_linux_runner_for_trusted_events(self):
        limited = self.expressions["rust-limited-groups"]
        settings = {"vars.MTG_CI_LINUX_GROUPS": '["limited-only-3", "limited"]', "vars.MTG_CI_LINUX_RUNNER": json.dumps(LINUX_POOL)}

        def route(system, group, **context):
            return evaluate(limited, **{"matrix.os": system, "matrix.group": group, **context})

        for trusted in (event(), event("push"), event("workflow_dispatch")):
            with self.subTest(event=trusted["github.event_name"]):
                self.assertEqual(route("ubuntu-24.04", "limited-only-3", **trusted, **settings), LINUX_POOL)
                self.assertEqual(route("ubuntu-24.04", "limited", **trusted, **settings), LINUX_POOL)
                self.assertEqual(route("ubuntu-24.04", "limited-only", **trusted, **settings), "ubuntu-24.04")
                self.assertEqual(route("ubuntu-24.04", "limited-model", **trusted, **settings), "ubuntu-24.04")
        self.assertEqual(route("ubuntu-24.04", "limited", **event(), **{"vars.MTG_CI_LINUX_GROUPS": '["limited"]'}),
                         "ubuntu-24.04")
        self.assertEqual(route("ubuntu-24.04", "limited", **event()), "ubuntu-24.04")
        # Linux settings never move a Windows Limited group, which keeps its own variable.
        self.assertEqual(route("windows-2025", "limited", **event("push"), **settings), "windows-2025")
        self.assertEqual(route("windows-2025", "limited", **event("push"), **EVERY_POOL), WINDOWS_POOL)

    def test_windows_routing_is_unchanged(self):
        self.assertEqual(self.route("windows-2025", "workspace-targets", **event(), **EVERY_POOL), PR_POOL)
        self.assertEqual(self.route("windows-2025", "workspace-lib", **event(), **EVERY_POOL), WINDOWS_POOL)
        self.assertEqual(self.route("windows-2025", "workspace-targets", **event("push"), **EVERY_POOL), WINDOWS_POOL)
        self.assertEqual(self.route("windows-2025", "workspace-targets", **event(), **EVERY_POOL,
                                    **{"inputs.windows_runner": '"windows-2025"'}), "windows-2025")
        self.assertEqual(self.route("windows-2025", "workspace-targets", **event()), "windows-2025")


class LightJobRoutingTests(unittest.TestCase):
    """detect-changes and the rust-tests gate: self-hosted only for a listed actor's trusted event."""

    @classmethod
    def setUpClass(cls):
        expressions = runs_on_expressions()
        cls.light = {name: expressions[name] for name in LIGHT_JOBS}

    def assertRoutes(self, expected, **context):
        for name, expression in self.light.items():
            for system in ("ubuntu-24.04", "windows-2025"):
                with self.subTest(job=name, system=system):
                    self.assertEqual(evaluate(expression, **{"matrix.os": system, **context}), expected)

    def test_both_jobs_share_one_expression(self):
        self.assertEqual(len(set(self.light.values())), 1)
        self.assertNotIn("repository_owner", self.light["detect-changes"])

    def test_unset_variable_keeps_the_hosted_runner(self):
        for trusted in (event(), event("push"), event("workflow_dispatch")):
            with self.subTest(event=trusted["github.event_name"]):
                self.assertRoutes("ubuntu-24.04", **trusted, **by(OWNER))
                self.assertRoutes("ubuntu-24.04", **trusted, **by(OWNER), **EVERY_POOL,
                                  **{"vars.MTG_CI_SELF_HOSTED_ACTORS": LIGHT["vars.MTG_CI_SELF_HOSTED_ACTORS"]})
                self.assertRoutes("ubuntu-24.04", **trusted, **by(OWNER),
                                  **{**LIGHT, "vars.MTG_CI_DETECT_RUNNER": ""})

    def test_a_listed_actor_runs_on_the_named_runner(self):
        for trusted in (event(), event("push"), event("workflow_dispatch")):
            for actor in (OWNER, "helper", "MAINTAINER"):
                with self.subTest(event=trusted["github.event_name"], actor=actor):
                    self.assertRoutes(DETECT_POOL, **trusted, **by(actor), **LIGHT)

    def test_an_unlisted_actor_stays_hosted(self):
        for actor, triggering in (("someone", None), ("maintaine", None), ("maintainer2", None), ("", None),
                                  (OWNER, "someone"), ("someone", OWNER)):
            with self.subTest(actor=actor, triggering=triggering):
                self.assertRoutes("ubuntu-24.04", **event(), **by(actor, triggering), **LIGHT)
                self.assertRoutes("ubuntu-24.04", **event("push"), **by(actor, triggering), **LIGHT)

    def test_a_fork_pull_request_stays_hosted_even_for_a_listed_actor(self):
        self.assertRoutes("ubuntu-24.04", **event(head="someone/fork"), **by(OWNER), **LIGHT)
        self.assertRoutes("ubuntu-24.04", **event(head="someone/fork"), **by("someone"), **LIGHT)

    def test_an_empty_or_malformed_list_admits_nobody_without_an_error(self):
        for value in ("", "[]", "null", f'"{OWNER}"', f"[{OWNER}]", f'["{OWNER}"', f' ["{OWNER}"]', OWNER,
                      f'{{"{OWNER}": 1}}'):
            with self.subTest(value=value):
                self.assertRoutes("ubuntu-24.04", **event(), **by(OWNER),
                                  **{**LIGHT, "vars.MTG_CI_SELF_HOSTED_ACTORS": value})

    def test_other_routing_variables_do_not_move_the_light_jobs(self):
        self.assertRoutes("ubuntu-24.04", **event("push"), **by(OWNER), **EVERY_POOL)


class DetectStepTests(unittest.TestCase):
    def test_the_group_set_step_works_without_the_github_cli(self):
        block = job_block("detect-changes", "rust-quality")
        self.assertIn("if command -v gh > /dev/null 2>&1; then", block)
        self.assertIn("python3 tools/ci/pr_lookup.py open-pr-head", block)
        self.assertIn('python3 tools/ci/pr_lookup.py pr-files "$1"', block)
        self.assertEqual(block.count("gh api"), 2)


def job_block(name, following):
    text = WORKFLOW.read_text(encoding="utf-8")
    return text[text.index(f"\n  {name}:\n"):text.index(f"\n  {following}:\n")]


def matrix_groups(block):
    listing = block[block.index("        group:\n"):]
    end = re.search(r"^        [a-z]|^    [a-z]", listing[len("        group:\n"):], re.MULTILINE)
    listing = listing[:len("        group:\n") + end.start()]
    return re.findall(r"^          - ([a-z0-9-]+)$", listing, re.MULTILINE)


ETERNAL_GROUPS = {"eternal", "eternal-integration"}


class MatrixTests(unittest.TestCase):
    def test_a_pull_request_and_its_windows_complement_cover_every_group_once(self):
        block = job_block("rust-test-groups", "limited-build")
        groups = matrix_groups(block)
        exclude = block[block.index("        exclude:"):].splitlines()[0]
        pull_request, complement = (set(re.findall(r'\{"os":"windows-2025","group":"([a-z0-9-]+)"\}', part))
                                    for part in exclude.split("'windows-complement'"))
        self.assertEqual(len(groups), len(set(groups)))
        self.assertEqual(pull_request | complement, set(groups))
        self.assertFalse(pull_request & complement)
        # Windows runs the Eternal catalog on main and in the complement, never in a pull request's own run.
        self.assertEqual({group for group in groups if group.startswith("eternal")}, ETERNAL_GROUPS)
        self.assertLessEqual(ETERNAL_GROUPS, pull_request)
        # Pull requests keep both native Store parts on Windows.
        self.assertLessEqual({"native-store", "native-store-1"}, complement)

    def test_native_store_1_runs_on_windows_only(self):
        # Every exclude list of a set with Linux jobs (pr and full) drops the Linux native-store-1; the
        # Windows-only sets have no Linux job to drop.
        block = job_block("rust-test-groups", "limited-build")
        exclude = block[block.index("        exclude:"):].splitlines()[0]
        pull_request, complement, full = (json.loads(part) for part in re.findall(r"'(\[.*?\])'", exclude))
        linux = {"os": "ubuntu-24.04", "group": "native-store-1"}
        self.assertIn(linux, pull_request)
        self.assertIn(linux, full)
        self.assertEqual([entry for entry in pull_request + complement + full if entry["os"] == "ubuntu-24.04"],
                         [linux, linux])

    def test_every_shard_of_a_sharded_step_has_its_own_matrix_group(self):
        # A shard count larger than the parts in the matrix would leave a shard's tests in no job, and a
        # smaller one would fail the extra parts. Parts are named <group>, <group>-1, ..., <group>-(n-1).
        limited = job_block("rust-limited-groups", "rust-tests")
        cases = [(limited, "limited-only", "--limited-only-workflow"),
                 (limited, "limited-model", "current_mixed_policy_v1::")]
        for block, family, marker in cases:
            with self.subTest(family=family):
                (line,) = [line for line in block.splitlines() if marker in line and "--shard-count" in line]
                count = int(re.search(r"--shard-count (\d+)", line).group(1))
                parts = [group for group in matrix_groups(block) if group == family or group.startswith(family + "-")]
                self.assertEqual(parts, [family] + [f"{family}-{index}" for index in range(1, count)])

    def test_each_native_store_command_runs_once_on_each_system(self):
        block = job_block("rust-test-groups", "limited-build")
        steps = [step for step in block.split("\n      - name: ") if "native-training-store-v2-production --" in step]
        library, stub = (next(step for step in steps if command in step)
                         for command in ("--lib native_store_production_capture_v2::",
                                         "--test native_cli_platform_stub_v2"))
        self.assertIn("if: matrix.group == 'native-store'\n", library)
        # Windows runs the stub in native-store-1; Linux, which has no native-store-1, in native-store.
        self.assertIn("if: matrix.group == 'native-store-1' || (matrix.group == 'native-store' && runner.os == 'Linux')",
                      stub)

    def test_the_eternal_catalog_runs_each_integration_suite_once(self):
        block = job_block("rust-test-groups", "limited-build")
        for suite in ("eternal_integration", "standard_integration"):
            with self.subTest(suite=suite):
                calls = [line for line in block.splitlines()
                         if f"run_rust_integration_tests.py {suite} --features eternal-fixtures" in line]
                self.assertEqual(len(calls), 1)
                self.assertNotIn("--shard", calls[0])

    def test_limited_only_shards_read_the_default_list_one_job_writes(self):
        build = job_block("limited-build", "rust-limited-groups")
        limited = job_block("rust-limited-groups", "rust-tests")
        self.assertEqual(matrix_groups(build.replace("        part:\n", "        group:\n")),
                         ["tests", "binaries", "default-list"])
        self.assertIn("--write-default-library-list default-library-list.json", build)
        self.assertIn("name: limited-default-list-${{ matrix.os }}-${{ github.run_attempt }}", build)
        self.assertIn("pattern: limited-default-list-${{ matrix.os }}-*", limited)
        (line,) = [line for line in limited.splitlines() if "--limited-only-workflow" in line]
        self.assertIn('--default-library-list "$list"', line)
        # The shared Limited build's own download pattern does not pick up the list.
        self.assertIn("pattern: limited-build-${{ matrix.os }}-*", limited)

    def test_the_parts_of_a_group_share_one_build_cache(self):
        block = job_block("rust-test-groups", "limited-build")
        self.assertIn("scope: tests-${{ env.MTG_CI_BUILD_GROUP }}", block)
        self.assertNotIn("scope: tests-${{ matrix.group }}", block)
        self.assertIn("matrix.group == env.MTG_CI_BUILD_GROUP", block)


if __name__ == "__main__":
    unittest.main()
