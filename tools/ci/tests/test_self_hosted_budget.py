"""The pull request routing budget sends no more groups to the self-hosted pool than it has free slots."""
import os
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

HERE = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(HERE))
import self_hosted_budget as budget  # noqa: E402

POOL = ["self-hosted", "mtg-ci-windows", "bn6"]


def job(status, labels, run_id=1):
    return {"status": status, "labels": labels, "run_id": run_id}


class BudgetTests(unittest.TestCase):
    def test_every_unfinished_job_not_on_a_hosted_image_counts(self):
        jobs = [job("queued", POOL), job("in_progress", ["self-hosted", "MTG-CI-WINDOWS", "bn6"]),
                job("completed", POOL),
                job("queued", ["self-hosted", "mtg-ci-windows"]), job("queued", ["self-hosted", "Windows", "X64"]),
                job("queued", ["Windows", "X64"]), job("queued", ["bn6"]), job("queued", ["self-hosted"]),
                job("queued", ["self-hosted", "mtg-ci-linux", "X64"]),                 # counted: cannot rule out
                job("queued", ["self-hosted", "heavy"]),
                job("queued", ["windows-2025"]), job("queued", ["ubuntu-24.04"]),      # hosted images
                job("queued", ["ubuntu-latest"]), job("queued", ["macos-15"]),
                job("queued", ["windows-2025-large"]), job("queued", ["ubuntu-24.04-xlarge"]),  # custom names
                job("queued", ["windows-2025.."]), job("queued", []),                           # counted
                job("in_progress", POOL, run_id=7)]
        self.assertEqual(budget.held_by_pool(jobs, POOL), 14)
        self.assertEqual(budget.held_by_pool(jobs, POOL, skip_run=7), 13)
        self.assertEqual(budget.held_by_pool(jobs, []), 0)
        self.assertEqual(budget.held_by_pool([job("in_progress", ["bn6"])], ["bn6"]), 1)

    def test_groups_this_run_lacks_do_not_take_a_slot(self):
        text, _ = self.run_main({"PR_GROUPS": '["eternal", "workspace-targets"]', "PR_RUNNER": '["bn6"]',
                                 "PR_SLOTS": "1", "PR_ELIGIBLE": '["workspace-targets", "workspace-lib"]'})
        self.assertEqual(text, 'groups=["workspace-targets"]\n')

    def test_a_run_that_moves_from_queued_to_running_between_reads_is_seen(self):
        order = []

        def fake_get(path, _budget):
            if "/runs?status=" in path:
                status = path.split("status=")[1].split("&")[0]
                order.append(status)
                # The run starts right after the first read of either list it can appear in.
                running = any(seen in ("queued", "in_progress") for seen in order[:-1])
                visible = (status == "in_progress") if running else (status == "queued")
                return {"workflow_runs": [{"id": 5}] if visible else []}
            return {"jobs": [job("in_progress", POOL, run_id=5)]}
        with patch.object(budget, "_get", side_effect=fake_get):
            self.assertEqual(budget.held_by_pool(budget.active_jobs("owner/project"), POOL), 1)
        self.assertEqual(order[-1], "in_progress")
        self.assertLess(order.index("queued"), len(order) - 1)

    def test_the_lookup_has_a_time_and_request_limit(self):
        tight = budget.Budget(seconds=60, requests=2)
        self.assertLessEqual(tight.spend(), 15.0)
        tight.spend()
        with self.assertRaises(TimeoutError):
            tight.spend()
        with self.assertRaises(TimeoutError):
            budget.Budget(seconds=0).spend()

    def test_groups_route_in_order_of_preference_up_to_the_free_slots(self):
        groups = ["workspace-targets", "workspace-lib", "native-store"]
        self.assertEqual(budget.choose(groups, None, 5), groups)
        self.assertEqual(budget.choose(groups, 2, 0), groups[:2])
        self.assertEqual(budget.choose(groups, 2, 1), groups[:1])
        self.assertEqual(budget.choose(groups, 2, 3), [])

    def test_settings_are_parsed_strictly(self):
        self.assertEqual(budget.parse_list('"one"'), ["one"])
        self.assertEqual(budget.parse_list(""), [])
        self.assertIsNone(budget.parse_slots(" "))
        self.assertEqual(budget.parse_slots("2"), 2)
        for bad in ('{"a": 1}', "[1]"):
            with self.subTest(bad=bad), self.assertRaises(ValueError):
                budget.parse_list(bad)
        with self.assertRaises(ValueError):
            budget.parse_slots("-1")

    def run_main(self, env, jobs=None, error=None):
        with tempfile.TemporaryDirectory() as tmp:
            out = Path(tmp) / "out"
            full = {"GITHUB_OUTPUT": str(out), "GITHUB_REPOSITORY": "owner/project", "GITHUB_RUN_ID": "9",
                    "GH_TOKEN": "x", **env}

            def fake(_repo):
                if error:
                    raise error
                return jobs or []
            with patch.dict(os.environ, full, clear=True), patch.object(budget, "active_jobs", side_effect=fake) as api, \
                    patch("sys.stdout"):
                self.assertEqual(budget.main(), 0)
            return out.read_text(), api.call_count

    def test_main_without_a_budget_routes_every_named_group_and_calls_no_api(self):
        text, calls = self.run_main({"PR_GROUPS": '["workspace-lib"]', "PR_RUNNER": '["self-hosted"]'})
        self.assertEqual(text, 'groups=["workspace-lib"]\n')
        self.assertEqual(calls, 0)

    def test_main_counts_the_pool_and_leaves_this_run_out(self):
        jobs = [job("in_progress", POOL), job("queued", POOL, run_id=9)]
        text, calls = self.run_main({"PR_GROUPS": '["workspace-targets", "workspace-lib"]', "PR_RUNNER": '["self-hosted", '
                                     '"mtg-ci-windows", "bn6"]', "PR_SLOTS": "2"}, jobs=jobs)
        self.assertEqual(text, 'groups=["workspace-targets"]\n')
        self.assertEqual(calls, 1)

    def test_any_error_keeps_every_group_hosted(self):
        for env, error in (({"PR_GROUPS": '["workspace-lib"]', "PR_RUNNER": '["bn6"]', "PR_SLOTS": "2"}, OSError("api down")),
                           ({"PR_GROUPS": '["workspace-lib"]', "PR_RUNNER": '["bn6"]', "PR_SLOTS": "2"}, TimeoutError("slow")),
                           ({"PR_GROUPS": "not json", "PR_SLOTS": "2"}, None),
                           ({"PR_GROUPS": '["workspace-lib"]', "PR_SLOTS": "two"}, None)):
            with self.subTest(env=env):
                text, _ = self.run_main(env, error=error)
                self.assertEqual(text, "groups=[]\n")


if __name__ == "__main__":
    unittest.main()
