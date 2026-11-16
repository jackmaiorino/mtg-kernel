"""The workflow's runs-on expressions send only trusted events to self-hosted runners.

The expressions are evaluated here with a small evaluator for the part of GitHub's expression language
they use (&&, ||, ==, !=, parentheses, string literals, context lookups, fromJSON and contains).
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

    def either(self):
        value = self.both()
        while self.peek() == "||":
            self.take()
            other = self.both()
            value = value if truthy(value) else other
        return value

    def both(self):
        value = self.comparison()
        while self.peek() == "&&":
            self.take()
            other = self.comparison()
            value = other if truthy(value) else value
        return value

    def comparison(self):
        value = self.operand()
        while self.peek() in ("==", "!="):
            negate = self.take() == "!="
            value = equal(value, self.operand()) != negate
        return value

    def operand(self):
        token = self.take()
        if token == "(":
            value = self.either()
            self.take(")")
            return value
        if token.startswith("'"):
            return token[1:-1].replace("''", "'")
        if self.peek() == "(":
            self.take("(")
            arguments = [self.either()]
            while self.peek() == ",":
                self.take()
                arguments.append(self.either())
            self.take(")")
            if token == "fromJSON":
                return json.loads(arguments[0])
            if token == "contains":
                return arguments[1] in arguments[0]
            raise ValueError(f"unsupported function {token}")
        return self.context.get(token, "")


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


def event(name="pull_request", head=REPOSITORY, **more):
    context = {"github.event_name": name, "github.repository": REPOSITORY}
    if name == "pull_request":
        context["github.event.pull_request.head.repo.full_name"] = head
    return {**context, **more}


EVERY_POOL = {"vars.MTG_CI_WINDOWS_RUNNER": json.dumps(WINDOWS_POOL), "vars.MTG_CI_PR_WINDOWS_RUNNER": json.dumps(PR_POOL),
              "vars.MTG_CI_PR_WINDOWS_GROUPS": '["workspace-targets", "eternal"]',
              "vars.MTG_CI_LINUX_RUNNER": json.dumps(LINUX_POOL), "vars.MTG_CI_LINUX_GROUPS": '["eternal", "workspace-targets"]'}


class EvaluatorTests(unittest.TestCase):
    def test_operators_follow_github_semantics(self):
        self.assertEqual(evaluate("a == 'x' && fromJSON(b) || c", a="X", b='["p", "q"]', c="hosted"), ["p", "q"])
        self.assertEqual(evaluate("a == 'x' && fromJSON(b) || c", a="y", b='"p"', c="hosted"), "hosted")
        self.assertEqual(evaluate("missing || 'it''s'"), "it's")
        self.assertTrue(evaluate("contains(fromJSON(list || '[]'), item) && (a != 'b')", list='["k"]', item="k", a="c"))
        self.assertFalse(evaluate("contains(fromJSON(list || '[]'), item)", item="k"))
        with self.assertRaises(ValueError):
            evaluate("a ? b : c")


class RoutingTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.expressions = runs_on_expressions()
        cls.groups = cls.expressions["rust-test-groups"]

    def route(self, system, group, **context):
        return evaluate(self.groups, **{"matrix.os": system, "matrix.group": group, **context})

    def test_every_expression_that_reads_a_setting_is_found(self):
        self.assertEqual(sorted(self.expressions), ["limited-build", "python-env-build", "python-tests",
                                                    "rust-limited-groups", "rust-test-groups"])

    def test_a_fork_pull_request_always_runs_on_hosted_runners(self):
        fork = event(head="someone/fork", **EVERY_POOL)
        for name, expression in self.expressions.items():
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

    def test_windows_routing_is_unchanged(self):
        self.assertEqual(self.route("windows-2025", "workspace-targets", **event(), **EVERY_POOL), PR_POOL)
        self.assertEqual(self.route("windows-2025", "workspace-lib", **event(), **EVERY_POOL), WINDOWS_POOL)
        self.assertEqual(self.route("windows-2025", "workspace-targets", **event("push"), **EVERY_POOL), WINDOWS_POOL)
        self.assertEqual(self.route("windows-2025", "workspace-targets", **event(), **EVERY_POOL,
                                    **{"inputs.windows_runner": '"windows-2025"'}), "windows-2025")
        self.assertEqual(self.route("windows-2025", "workspace-targets", **event()), "windows-2025")


if __name__ == "__main__":
    unittest.main()
