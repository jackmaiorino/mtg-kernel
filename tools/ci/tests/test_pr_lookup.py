"""The no-gh workflow path returns the same pull request data as gh."""
import contextlib
import io
import json
import os
import sys
import unittest
from pathlib import Path
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import ci_scope  # noqa: E402
import pr_lookup  # noqa: E402

REPOSITORY = "owner/project"
REF = "topic/x"
BASE = "0" * 40


def pull(number, sha, repo=REPOSITORY, draft=False, ref=REF):
    return {"number": number, "state": "open", "draft": draft, "base": {"sha": BASE},
            "head": {"sha": sha, "ref": ref, "repo": {"full_name": repo} if repo else None}}


class PrLookupTests(unittest.TestCase):
    def test_open_pr_head_picks_the_first_ready_same_repository_pull_request(self):
        pulls = [pull(1, "a" * 40, repo="someone/fork"), pull(2, "b" * 40, draft=True), pull(3, "c" * 40, repo=None),
                 pull(4, "d" * 40, ref="different"), pull(5, "e" * 40), pull(6, "f" * 40)]
        self.assertEqual(pr_lookup.open_pr_head(pulls, REPOSITORY, REF), f"5 {'e' * 40} {BASE}")
        self.assertEqual(pr_lookup.open_pr_head(pulls[:4], REPOSITORY, REF), "")
        self.assertEqual(pr_lookup.open_pr_head([], REPOSITORY, REF), "")

    def test_open_pr_head_rejects_invalid_ready_identity(self):
        for record in (pull(True, "a" * 40), pull(0, "a" * 40), pull(1, "bad"),
                       {**pull(1, "a" * 40), "base": {"sha": "g" * 40}}):
            with self.subTest(record=record), self.assertRaises(ValueError):
                pr_lookup.open_pr_head([record], REPOSITORY, REF)

    def run_main(self, argv, pages):
        calls = []

        def get(path):
            calls.append(path)
            return pages.pop(0)

        environment = {"GITHUB_REPOSITORY": REPOSITORY, "GITHUB_REPOSITORY_OWNER": "owner", "GITHUB_REF_NAME": REF,
                       "GH_TOKEN": "unused"}
        out = io.StringIO()
        with mock.patch.dict(os.environ, environment), mock.patch.object(pr_lookup, "get", get), \
                contextlib.redirect_stdout(out), contextlib.redirect_stderr(io.StringIO()):
            code = pr_lookup.main(argv)
        return code, out.getvalue(), calls

    def test_main_queries_and_paginates(self):
        code, out, calls = self.run_main(["open-pr-head"], [[pull(7, "f" * 40)]])
        self.assertEqual((code, out), (0, f"7 {'f' * 40} {BASE}\n"))
        self.assertEqual(calls, ["repos/owner/project/pulls?state=open&head=owner:topic/x"])
        first = [{"status": "modified", "filename": f"f{index}"} for index in range(100)]
        renamed = {"status": "renamed", "filename": "docs/moved.md", "previous_filename": "tools/stage4a/café\nstep.py"}
        code, out, calls = self.run_main(["pr-files", "12"], [first, [renamed]])
        self.assertEqual(code, 0)
        self.assertEqual(json.loads(out), [first, [renamed]])
        self.assertEqual(ci_scope.pull_request_paths(json.loads(out))[-2:],
                         [renamed["filename"], renamed["previous_filename"]])
        self.assertEqual(calls, [f"repos/owner/project/pulls/12/files?per_page=100&page={page}" for page in (1, 2)])
        self.assertEqual(self.run_main(["pr-files", "x"], [])[0], 2)

    def test_invalid_files_page_is_not_a_complete_inventory(self):
        first = [{"status": "modified", "filename": "a.py"}] * 100
        code, out, _ = self.run_main(["pr-files", "12"], [first, {"message": "unexpected payload"}])
        self.assertEqual((code, out), (2, ""))


if __name__ == "__main__":
    unittest.main()
