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
REPOSITORY, OWNER = "owner/project", "maintainer"
ALLOWED = json.dumps([OWNER, "helper"])
WINDOWS_POOL, PR_POOL, LINUX_POOL = ["self-hosted", "windows-pool"], ["self-hosted", "pr-pool"], ["self-hosted", "linux-pool"]


def truthy(value):
    return value not in ("", None, False, 0)


def number(value):
    """GitHub's conversion for a loose comparison: null 0, booleans 1 and 0, strings parsed (NaN if not a number)."""
    if value is None:
        return 0.0
    if isinstance(value, (bool, int, float)):
        return float(value)
    if isinstance(value, str):
        try:
            return float(value.strip() or 0)
        except ValueError:
            return float("nan")
    return float("nan")


def equal(left, right):
    if isinstance(left, str) and isinstance(right, str):
        return left.lower() == right.lower()
    if isinstance(left, (list, dict)) or isinstance(right, (list, dict)):
        return left is right
    if type(left) is type(right):
        return left == right
    # Different primitive types compare as numbers: true == '1', null == '0', 100 == '1e2'.
    return number(left) == number(right)


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

    def either(self, resolve=True):
        value = self.both(resolve)
        while self.peek() == "||":
            self.take()
            use_other = resolve and not truthy(value)
            other = self.both(use_other)
            if use_other:
                value = other
        return value

    def both(self, resolve):
        value = self.comparison(resolve)
        while self.peek() == "&&":
            self.take()
            use_other = resolve and truthy(value)
            other = self.comparison(use_other)
            if use_other:
                value = other
        return value

    def comparison(self, resolve):
        value = self.operand(resolve)
        while self.peek() in ("==", "!="):
            negate = self.take() == "!="
            other = self.operand(resolve)
            if resolve:
                value = equal(value, other) != negate
        return value

    def operand(self, resolve):
        token = self.take()
        if token == "(":
            value = self.either(resolve)
            self.take(")")
            return value
        if token.startswith("'"):
            return token[1:-1].replace("''", "'")
        if self.peek() == "(":
            self.take("(")
            arguments = [self.either(resolve)]
            while self.peek() == ",":
                self.take()
                arguments.append(self.either(resolve))
            self.take(")")
            # Parse skipped branches without evaluating any functions in them.
            if token not in ("fromJSON", "format", "contains", "startsWith", "endsWith", "join"):
                raise ValueError(f"unsupported function {token}")
            return call(token, arguments) if resolve else None
        return self.context.get(token, "")


def text(value):
    """GitHub's string conversion of a primitive."""
    if value is None:
        return ""
    if isinstance(value, bool):
        return "true" if value else "false"
    if isinstance(value, float) and value.is_integer():
        return str(int(value))
    return value if isinstance(value, str) else ("Array" if isinstance(value, list) else "Object" if isinstance(value, dict) else str(value))


def call(name, arguments):
    if name == "fromJSON":
        return json.loads(arguments[0])
    if name == "join":
        return arguments[1].join(text(element) for element in arguments[0]) if isinstance(arguments[0], list) \
            else text(arguments[0])
    if name == "format":
        return re.sub(r"\{(\d+)\}", lambda match: str(arguments[1 + int(match.group(1))]), arguments[0])
    if isinstance(arguments[0], str):
        # GitHub compares strings case-insensitively; a missing value is ''.
        search, item = arguments[0].lower(), str(arguments[1]).lower()
        return {"contains": item in search, "startsWith": search.startswith(item), "endsWith": search.endswith(item)}[name]
    if name == "contains" and isinstance(arguments[0], list):
        # Array membership uses the == comparison (case-insensitive for strings); an object never equals a name.
        return any(equal(element, arguments[1]) for element in arguments[0])
    raise ValueError(f"{name} of a {type(arguments[0]).__name__}")


def evaluate(expression, **context):
    evaluator = Evaluator(expression, context)
    value = evaluator.either()
    if evaluator.peek() is not None:
        raise ValueError("trailing tokens")
    return value


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


def event(name="pull_request", head=REPOSITORY, actor=OWNER, triggering=None, **more):
    # Trusted by default: the actor and the triggering actor are both in the self-hosted allow-list.
    context = {"github.event_name": name, "github.repository": REPOSITORY, "github.actor": actor,
               "github.triggering_actor": triggering or actor, "vars.MTG_CI_SELF_HOSTED_ACTORS": ALLOWED}
    if name == "pull_request":
        context["github.event.pull_request.head.repo.full_name"] = head
    return {**context, **more}


EVERY_POOL = {"vars.MTG_CI_WINDOWS_RUNNER": json.dumps(WINDOWS_POOL), "vars.MTG_CI_PR_WINDOWS_RUNNER": json.dumps(PR_POOL),
              "vars.MTG_CI_PR_WINDOWS_GROUPS": '["workspace-targets", "eternal"]',
              "needs.detect-changes.outputs.pr_windows_groups": '["workspace-targets", "eternal"]',
              "vars.MTG_CI_LINUX_RUNNER": json.dumps(LINUX_POOL), "vars.MTG_CI_LINUX_GROUPS": '["eternal", "workspace-targets"]'}


# Trusted except for the actor list: an unlisted actor, triggering actor or bot, and a missing, empty or malformed list.
UNLISTED = [event(actor="someone"), event("push", actor="someone"), event("workflow_dispatch", actor="github-actions[bot]"),
            event(actor="maintaine"), event(actor="maintainer2"), event(triggering="someone"),
            event("push", actor="someone", triggering=OWNER)]
BAD_LISTS = [event(**{"vars.MTG_CI_SELF_HOSTED_ACTORS": value})
             for value in ("", "[]", "null", f'"{OWNER}"', f'"{OWNER}x"', f'["{OWNER}"', f' ["{OWNER}"]', OWNER,
                           f'{{"{OWNER}": 1}}', f'[["{OWNER}"]]', f'[{{"{OWNER}": true}}]',
                           f'["{OWNER}","helper"]', f'["{OWNER}" , "helper"]')]

# Jobs on MTG_CI_DETECT_RUNNER: light, generic jobs whose runner does not depend on the matrix.
LIGHT_JOBS = ("detect-changes", "rust-tests")
DETECT_POOL = ["self-hosted", "fleet-mac-linux"]
LIGHT = {"vars.MTG_CI_DETECT_RUNNER": json.dumps(DETECT_POOL), "vars.MTG_CI_SELF_HOSTED_ACTORS": ALLOWED}


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

    def test_and_or_short_circuit_function_calls(self):
        self.assertEqual(evaluate("missing && fromJSON(setting) || 'hosted'", setting="["), "hosted")
        self.assertEqual(evaluate("'hosted' || fromJSON(setting)", setting="["), "hosted")
        with self.assertRaises(json.JSONDecodeError):
            evaluate("'selected' && fromJSON(setting)", setting="[")

    def test_and_or_short_circuit_and_string_functions(self):
        self.assertEqual(evaluate("a && fromJSON(b) || 'hosted'", b="not json"), "hosted")
        with self.assertRaises(ValueError):
            evaluate("a && fromJSON(b) || 'hosted'", a="x", b="not json")
        self.assertTrue(evaluate("startsWith(a, '[') && endsWith(a, ']') && contains(a, format('\"{0}\"', b))",
                                 a='["Who"]', b="who"))
        self.assertFalse(evaluate("startsWith(a, '[')"))
        self.assertTrue(evaluate("contains(fromJSON(a), b)", a='["Who"]', b="wHO"))
        self.assertFalse(evaluate("contains(fromJSON(a), b)", a='[{"who": true}, ["who"]]', b="who"))


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

    def test_every_listed_actor_runs_on_self_hosted_runners(self):
        for actor in (OWNER, "helper", OWNER.upper()):
            with self.subTest(actor=actor):
                self.assertEqual(self.route("ubuntu-24.04", "eternal", **event(actor=actor), **EVERY_POOL), LINUX_POOL)
                self.assertEqual(self.route("windows-2025", "workspace-lib", **event("push", actor=actor), **EVERY_POOL),
                                 WINDOWS_POOL)

    def test_only_listed_actors_run_on_self_hosted_runners(self):
        # A same-repository branch pushed by any account outside MTG_CI_SELF_HOSTED_ACTORS (a collaborator, a
        # bot), a re-run by one, and a missing, empty or malformed list all stay hosted. No owner comparison:
        # in an organization github.repository_owner is the organization.
        for expression in self.expressions.values():
            self.assertNotIn("repository_owner", expression)
        for untrusted in UNLISTED + BAD_LISTS:
            for name, expression in self.expressions.items():
                for system in ("ubuntu-24.04", "windows-2025"):
                    with self.subTest(job=name, system=system, event=untrusted["github.event_name"]):
                        expected = "ubuntu-24.04" if name in LIGHT_JOBS else system
                        self.assertEqual(evaluate(expression, **{"matrix.os": system, "matrix.group": "eternal",
                                                                 "vars.MTG_CI_DETECT_RUNNER": json.dumps(DETECT_POOL),
                                                                 **EVERY_POOL, **untrusted}), expected)

    def test_a_non_string_member_never_admits_a_coerced_name(self):
        # Array contains() compares loosely (true == '1', null == '0', 100 == '1e2'); the rejoin check refuses any
        # list with a non-string member, so such a list admits nobody.
        for value, actor in ((f'["{OWNER}", true]', "1"), (f'["{OWNER}", null]', "0"), (f'["{OWNER}", 100]', "1e2"),
                             (f'["{OWNER}", 8]', "8")):
            with self.subTest(value=value):
                self.assertTrue(evaluate("contains(fromJSON(list), name)", list=value, name=actor))
                for context in (event(triggering=actor), event()):
                    self.assertEqual(self.route("ubuntu-24.04", "eternal",
                                                **{**context, **EVERY_POOL, "vars.MTG_CI_SELF_HOSTED_ACTORS": value}),
                                     "ubuntu-24.04")

    def test_a_bracketed_invalid_list_fails_closed(self):
        # GitHub fails the job when fromJSON cannot parse, before any runner takes it. A fork pull request
        # never parses the list, so it stays hosted.
        for value in (f"[{OWNER}]", f'["{OWNER}" garbage]'):
            with self.subTest(value=value):
                bad = {"vars.MTG_CI_SELF_HOSTED_ACTORS": value}
                with self.assertRaises(ValueError):
                    self.route("ubuntu-24.04", "eternal", **{**event(), **EVERY_POOL, **bad})
                self.assertEqual(self.route("ubuntu-24.04", "eternal", **{**event(head="someone/fork"), **EVERY_POOL, **bad}),
                                 "ubuntu-24.04")

    def test_a_manual_heartbeat_needs_a_listed_actor(self):
        text = (WORKFLOW.parent / "mac-heartbeat.yml").read_text(encoding="utf-8")
        condition, = re.findall(r"^    if: (.+)$", text, re.MULTILINE)
        self.assertTrue(evaluate(condition, **{"github.event_name": "schedule"}))
        self.assertTrue(evaluate(condition, **event("workflow_dispatch")))
        self.assertTrue(evaluate(condition, **event("workflow_dispatch", actor="helper")))
        for untrusted in (event("workflow_dispatch", actor="someone"), event("workflow_dispatch", triggering="someone"),
                          event("workflow_dispatch", **{"vars.MTG_CI_SELF_HOSTED_ACTORS": ""}),
                          event("workflow_dispatch", **{"vars.MTG_CI_SELF_HOSTED_ACTORS": f'["{OWNER}", 1]'})):
            self.assertFalse(evaluate(condition, **untrusted))

    def test_pull_request_windows_groups_follow_the_budget_output(self):
        # The variable names the candidates; only the groups detect-changes chose within the budget route.
        settings = {**EVERY_POOL, "vars.MTG_CI_PR_WINDOWS_GROUPS": '["workspace-lib", "workspace-targets"]',
                    "needs.detect-changes.outputs.pr_windows_groups": '["workspace-targets"]'}
        self.assertEqual(self.route("windows-2025", "workspace-targets", **event(), **settings), PR_POOL)
        self.assertEqual(self.route("windows-2025", "workspace-lib", **event(), **settings), WINDOWS_POOL)
        none = {**settings, "needs.detect-changes.outputs.pr_windows_groups": ""}
        self.assertEqual(self.route("windows-2025", "workspace-targets", **event(), **none), WINDOWS_POOL)

    def test_the_budget_step_reads_the_routing_variables_for_trusted_pull_requests_only(self):
        block = job_block("detect-changes", "rust-quality")
        budget = block.index("      - name: Choose the self-hosted pull request groups")
        self.assertLess(block.index("      - name: Choose the checks"), budget)
        step = block[budget:].split("\n      - name: ", 1)[0]
        condition, = re.findall(r"^        if: (.+)$", step, re.MULTILINE)
        context = event(**{"steps.scope.outputs.scope": "pr",
                           "steps.classify.outputs.rust_relevant": "true",
                           "steps.checks.outputs.checks": "all"})
        self.assertTrue(evaluate(condition, **context))
        for override in ({"steps.scope.outputs.scope": "draft"},
                         {"steps.classify.outputs.rust_relevant": "false"},
                         {"steps.checks.outputs.checks": "trainer"},
                         {"steps.checks.outputs.checks": "workflow"},
                         {"github.event_name": "workflow_dispatch"},
                         {"github.actor": "contributor"},
                         {"github.triggering_actor": "contributor"},
                         {"vars.MTG_CI_SELF_HOSTED_ACTORS": ""},
                         {"github.event.pull_request.head.repo.full_name": "someone/fork"}):
            with self.subTest(override=override):
                self.assertFalse(evaluate(condition, **{**context, **override}))
        for line in ("PR_GROUPS: ${{ vars.MTG_CI_PR_WINDOWS_GROUPS }}", "PR_RUNNER: ${{ vars.MTG_CI_PR_WINDOWS_RUNNER }}",
                     "PR_SLOTS: ${{ vars.MTG_CI_PR_WINDOWS_SLOTS }}", "run: python3 tools/ci/self_hosted_budget.py"):
            self.assertIn(line, step)
        self.assertIn("pr_windows_groups: ${{ steps.budget.outputs.groups }}", block)
        self.assertIn("      actions: read\n", block)
        self.assertIn("timeout-minutes: 2\n        continue-on-error: true\n", step)

    def test_named_linux_groups_go_to_the_linux_runner_for_trusted_events(self):
        for trusted in (event(), event("push"), event("workflow_dispatch")):
            with self.subTest(event=trusted["github.event_name"]):
                self.assertEqual(self.route("ubuntu-24.04", "eternal", **trusted, **EVERY_POOL), LINUX_POOL)
                self.assertEqual(self.route("ubuntu-24.04", "standard", **trusted, **EVERY_POOL), "ubuntu-24.04")

    def test_linux_stays_hosted_unless_both_settings_are_present(self):
        cases = [{}, {"vars.MTG_CI_LINUX_GROUPS": '["eternal"]'}, {"vars.MTG_CI_LINUX_RUNNER": json.dumps(LINUX_POOL)},
                 {"vars.MTG_CI_LINUX_GROUPS": "[]", "vars.MTG_CI_LINUX_RUNNER": json.dumps(LINUX_POOL)},
                 {"vars.MTG_CI_WINDOWS_RUNNER": json.dumps(WINDOWS_POOL), "vars.MTG_CI_PR_WINDOWS_RUNNER": json.dumps(PR_POOL),
                  "vars.MTG_CI_PR_WINDOWS_GROUPS": '["eternal"]', "needs.detect-changes.outputs.pr_windows_groups": '["eternal"]',
                  "inputs.windows_runner": json.dumps(WINDOWS_POOL)}]
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
                 "needs.detect-changes.outputs.pr_windows_groups": '["workspace-lib"]',
                 "vars.MTG_CI_PR_WINDOWS_RUNNER": json.dumps(PR_POOL)}
        self.assertEqual(self.route("ubuntu-24.04", "eternal-integration", **event(), **today), LINUX_POOL)
        self.assertEqual(self.route("ubuntu-24.04", "eternal", **event(), **today), "ubuntu-24.04")
        self.assertEqual(self.route("windows-2025", "workspace-lib", **event(), **today), PR_POOL)
        for part in ("native-store", "native-store-1"):
            with self.subTest(part=part):
                self.assertEqual(self.route("windows-2025", part, **event(), **today), "windows-2025")

    def test_a_group_run_as_parts_routes_each_part_by_its_own_name(self):
        first = {"needs.detect-changes.outputs.pr_windows_groups": '["native-store"]',
                 "vars.MTG_CI_PR_WINDOWS_RUNNER": json.dumps(PR_POOL)}
        self.assertEqual(self.route("windows-2025", "native-store", **event(), **first), PR_POOL)
        self.assertEqual(self.route("windows-2025", "native-store-1", **event(), **first), "windows-2025")
        both = {**first, "needs.detect-changes.outputs.pr_windows_groups": '["native-store", "native-store-1"]'}
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
                self.assertRoutes("ubuntu-24.04", **{**trusted, **by(OWNER)})
                self.assertRoutes("ubuntu-24.04", **{**trusted, **by(OWNER), **EVERY_POOL,
                                                     "vars.MTG_CI_SELF_HOSTED_ACTORS": LIGHT["vars.MTG_CI_SELF_HOSTED_ACTORS"]})
                self.assertRoutes("ubuntu-24.04", **{**trusted, **by(OWNER), **LIGHT, "vars.MTG_CI_DETECT_RUNNER": ""})

    def test_a_listed_actor_runs_on_the_named_runner(self):
        for trusted in (event(), event("push"), event("workflow_dispatch")):
            for actor in (OWNER, "helper", "MAINTAINER"):
                with self.subTest(event=trusted["github.event_name"], actor=actor):
                    self.assertRoutes(DETECT_POOL, **{**trusted, **by(actor), **LIGHT})

    def test_an_unlisted_actor_stays_hosted(self):
        for actor, triggering in (("someone", None), ("maintaine", None), ("maintainer2", None), ("", None),
                                  (OWNER, "someone"), ("someone", OWNER)):
            with self.subTest(actor=actor, triggering=triggering):
                self.assertRoutes("ubuntu-24.04", **{**event(), **by(actor, triggering), **LIGHT})
                self.assertRoutes("ubuntu-24.04", **{**event("push"), **by(actor, triggering), **LIGHT})

    def test_a_fork_pull_request_stays_hosted_even_for_a_listed_actor(self):
        self.assertRoutes("ubuntu-24.04", **{**event(head="someone/fork"), **by(OWNER), **LIGHT})
        self.assertRoutes("ubuntu-24.04", **{**event(head="someone/fork"), **by("someone"), **LIGHT})

    def test_an_empty_or_unbracketed_list_admits_nobody_without_an_error(self):
        # A bare JSON string would make contains() a substring test, so the value must be bracketed.
        for value in ("", "[]", "null", f'"{OWNER}"', f'"{OWNER}x"', f'["{OWNER}"', f' ["{OWNER}"]', OWNER,
                      f'{{"{OWNER}": 1}}', f'[["{OWNER}"]]', f'[{{"{OWNER}": true}}]', f'["{OWNER}","helper"]',
                      f'["{OWNER}" , "helper"]', f'["{OWNER}",  "helper"]'):
            with self.subTest(value=value):
                self.assertRoutes("ubuntu-24.04", **{**event(), **by(OWNER), **LIGHT,
                                                     "vars.MTG_CI_SELF_HOSTED_ACTORS": value})

    def test_a_non_string_member_never_admits_a_coerced_name(self):
        # Array contains() compares loosely: true == '1', null == '0', 100 == '1e2'. The rejoin check refuses
        # any list with a non-string member, so such a list admits nobody (GitHub's parser also reads [010]
        # as [8]; a number never survives the rejoin either way).
        for value, actor in ((f'["{OWNER}", true]', "1"), (f'["{OWNER}", null]', "0"), (f'["{OWNER}", 100]', "1e2"),
                             (f'["{OWNER}", 8]', "8"), (f'["{OWNER}", ["1"]]', "1")):
            with self.subTest(value=value):
                listed = {**LIGHT, "vars.MTG_CI_SELF_HOSTED_ACTORS": value}
                self.assertTrue(evaluate("contains(fromJSON(list), name)", list=value, name=actor) or "[" in value[2:])
                self.assertRoutes("ubuntu-24.04", **{**event(), **by(OWNER, actor), **listed})
                self.assertRoutes("ubuntu-24.04", **{**event(), **by(OWNER), **listed})

    def test_only_the_exact_list_form_admits(self):
        for value in (f'["{OWNER}"]', f'["{OWNER}", "helper"]', f'["helper", "{OWNER}"]', f'["{OWNER.upper()}"]'):
            with self.subTest(value=value):
                self.assertRoutes(DETECT_POOL, **{**event(), **by(OWNER), **LIGHT,
                                                  "vars.MTG_CI_SELF_HOSTED_ACTORS": value})

    def test_a_bracketed_invalid_list_fails_closed(self):
        # GitHub fails the job when fromJSON cannot parse: the job never reaches a self-hosted runner. A fork
        # pull request or an unset runner variable never parses the list, so stays hosted. (Any other event,
        # listed or not, parses it and fails closed.)
        for value in (f"[{OWNER}]", f'["{OWNER}" garbage]'):
            with self.subTest(value=value):
                bad = {**LIGHT, "vars.MTG_CI_SELF_HOSTED_ACTORS": value}
                for expression in self.light.values():
                    with self.assertRaises(ValueError):
                        evaluate(expression, **{**event(), **by(OWNER), **bad})
                self.assertRoutes("ubuntu-24.04", **{**event(head="someone/fork"), **by(OWNER), **bad})
                self.assertRoutes("ubuntu-24.04", **{**event(), **by(OWNER), **bad, "vars.MTG_CI_DETECT_RUNNER": ""})

    def test_other_routing_variables_do_not_move_the_light_jobs(self):
        self.assertRoutes("ubuntu-24.04", **{**event("push"), **by(OWNER), **EVERY_POOL})


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

    def test_the_budget_step_knows_exactly_the_pull_request_windows_groups(self):
        block = job_block("rust-test-groups", "limited-build")
        groups = matrix_groups(block)
        exclude = block[block.index("        exclude:"):].splitlines()[0]
        pull_request = set(re.findall(r'\{"os":"windows-2025","group":"([a-z0-9-]+)"\}',
                                      exclude.split("'windows-complement'")[0]))
        detect = job_block("detect-changes", "rust-quality")
        (line,) = [line for line in detect.splitlines() if "PR_ELIGIBLE:" in line]
        self.assertEqual(json.loads(line.split("PR_ELIGIBLE:", 1)[1].strip().strip("'")),
                         [group for group in groups if group not in pull_request])

    def test_the_parts_of_a_group_share_one_build_cache(self):
        block = job_block("rust-test-groups", "limited-build")
        self.assertIn("scope: tests-${{ env.MTG_CI_BUILD_GROUP }}", block)
        self.assertNotIn("scope: tests-${{ matrix.group }}", block)
        self.assertIn("matrix.group == env.MTG_CI_BUILD_GROUP", block)


MAC_POOL = ["self-hosted", "Linux", "ARM64", "fleet-mac-linux"]
MAC_JOBS = ("rust-test-groups", "rust-limited-groups", "python-tests")


class MacRoutingTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.expressions = runs_on_expressions()

    def route(self, job, system, group, **context):
        return evaluate(self.expressions[job], **{"matrix.os": system, "matrix.group": group, **context})

    def test_chosen_linux_groups_go_to_the_mac_for_listed_actors_only(self):
        chosen = {"needs.detect-changes.outputs.mac_groups": '["limited-only", "standard", "python-shard-0"]',
                  "vars.MTG_CI_MAC_GROUPS": '["limited-only", "standard", "python-shard-0"]'}
        for job, group in (("rust-test-groups", "standard"), ("rust-limited-groups", "limited-only"),
                           ("python-tests", "python-shard-0")):
            with self.subTest(job=job):
                for trusted in (event(), event("push"), event("workflow_dispatch")):
                    self.assertEqual(self.route(job, "ubuntu-24.04", group, **trusted, **chosen), MAC_POOL)
                for untrusted in [event(head="someone/fork")] + UNLISTED + BAD_LISTS:
                    self.assertEqual(self.route(job, "ubuntu-24.04", group, **untrusted, **chosen), "ubuntu-24.04")
                # A group detect-changes did not choose, or no choice at all, stays hosted.
                self.assertEqual(self.route(job, "ubuntu-24.04", group + "-x", **event(), **chosen), "ubuntu-24.04")
                self.assertEqual(self.route(job, "ubuntu-24.04", group, **event()), "ubuntu-24.04")
                # The step output alone never routes: the variable must name the group too.
                output_only = {"needs.detect-changes.outputs.mac_groups": chosen["needs.detect-changes.outputs.mac_groups"]}
                self.assertEqual(self.route(job, "ubuntu-24.04", group, **event(), **output_only), "ubuntu-24.04")
                self.assertEqual(self.route(job, "ubuntu-24.04", group, **event(), **output_only,
                                            **{"vars.MTG_CI_MAC_GROUPS": "[]"}), "ubuntu-24.04")
                # The Mac never takes a Windows row.
                self.assertEqual(self.route(job, "windows-2025", group, **event(), **chosen), "windows-2025")

    def test_the_mac_comes_before_the_linux_pool(self):
        both = {"needs.detect-changes.outputs.mac_groups": '["eternal"]', "vars.MTG_CI_MAC_GROUPS": '["eternal"]',
                "vars.MTG_CI_LINUX_GROUPS": '["eternal"]',
                "vars.MTG_CI_LINUX_RUNNER": json.dumps(LINUX_POOL)}
        self.assertEqual(self.route("rust-test-groups", "ubuntu-24.04", "eternal", **event(), **both), MAC_POOL)

    def test_malformed_mac_setting_keeps_linux_on_hosted_runners(self):
        settings = {"vars.MTG_CI_MAC_GROUPS": '["python-shard-0"',
                    "needs.detect-changes.outputs.mac_groups": "[]"}
        for job, group in zip(MAC_JOBS, ("standard", "limited-only", "python-shard-0")):
            with self.subTest(job=job):
                self.assertEqual(self.route(job, "ubuntu-24.04", group, **event(), **settings), "ubuntu-24.04")

    def test_the_mac_step_runs_for_listed_actors_from_this_repository_only(self):
        block = job_block("detect-changes", "rust-quality")
        index = block.index("      - name: Choose the Mac groups")
        self.assertLess(block.index("      - name: Choose the checks"), index)
        self.assertLess(block.index("      - name: Choose the self-hosted pull request groups"), index)
        self.assertEqual(block.count("      - name: Check out merge ref with parents"), 1)
        step = block[index:]
        condition, = re.findall(r"^        if: (.+)$", step.split("\n      - name: ", 1)[0], re.MULTILINE)
        settings = {"vars.MTG_CI_MAC_GROUPS": '["standard"]'}
        self.assertTrue(evaluate(condition, **event(), **settings))
        self.assertTrue(evaluate(condition, **event("push"), **settings))
        self.assertFalse(evaluate(condition, **event()))
        for untrusted in [event(head="someone/fork")] + UNLISTED + BAD_LISTS:
            with self.subTest(event=untrusted):
                self.assertFalse(evaluate(condition, **untrusted, **settings))
        self.assertIn("timeout-minutes: 2\n        continue-on-error: true\n", step)
        self.assertIn("run: python3 tools/ci/mac_route.py", step)
        self.assertIn("CHECKS: ${{ steps.checks.outputs.checks || 'all' }}", step)
        self.assertIn("mac_groups: ${{ steps.mac.outputs.groups }}", block)

    def test_every_eligible_group_is_a_linux_row(self):
        detect = job_block("detect-changes", "rust-quality")
        (line,) = [line for line in detect.splitlines() if "MAC_ELIGIBLE:" in line]
        eligible = json.loads(line.split("MAC_ELIGIBLE:", 1)[1].strip().strip("'"))
        rows = set(matrix_groups(job_block("rust-test-groups", "limited-build"))) - {"native-store-1"}
        rows |= set(matrix_groups(job_block("rust-limited-groups", "rust-tests")))
        rows |= {"python-shard-0", "python-shard-1"}
        self.assertTrue(eligible)
        self.assertLessEqual(set(eligible), rows)

    def test_python_shards_carry_their_group_names(self):
        block = job_block("python-tests", "hosted-fallback")
        for shard in (0, 1, 2):
            self.assertIn(f"- {{shard: {shard}, group: python-shard-{shard}}}", block)
        # An arm64 runner builds its own binaries instead of the shared x86-64 ones.
        self.assertIn("if: runner.arch == 'ARM64'", block)
        self.assertEqual(block.count("if: runner.arch != 'ARM64'"), 2)


if __name__ == "__main__":
    unittest.main()
