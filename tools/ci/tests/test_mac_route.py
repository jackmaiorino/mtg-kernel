"""The Mac routing step sends groups to the Mac runners only while they are alive and have free slots."""
import json
import os
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

HERE = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(HERE))
import mac_route  # noqa: E402

NOW = 1_800_000_000.0
MAC = ["self-hosted", "Linux", "ARM64", "fleet-mac-linux"]


def stamp(seconds_ago):
    import datetime
    return datetime.datetime.fromtimestamp(NOW - seconds_ago, datetime.timezone.utc).isoformat().replace("+00:00", "Z")


def beat(status="completed", conclusion="success", created=60, updated=50):
    return {"status": status, "conclusion": conclusion, "created_at": stamp(created), "updated_at": stamp(updated)}


def job(status, labels, run_id=1):
    return {"status": status, "labels": labels, "run_id": run_id}


ENV = {"MAC_GROUPS": '["limited-only", "standard", "python-shard-0"]', "MAC_SLOTS": "3",
       "MAC_ELIGIBLE": '["limited-only", "standard", "python-shard-0", "python-shard-1"]', "SCOPE": "pr",
       "PYTHON_RELEVANT": "true", "RUST_RELEVANT": "true", "GH_TOKEN": "x", "GITHUB_REPOSITORY": "owner/project",
       "GITHUB_RUN_ID": "9"}


class AvailabilityTests(unittest.TestCase):
    def test_a_recent_heartbeat_and_none_waiting_means_available(self):
        self.assertIsNone(mac_route.mac_available([beat(), beat(updated=700)], NOW))
        self.assertIsNone(mac_route.mac_available([beat("in_progress", None, created=30, updated=30), beat()], NOW))
        self.assertIsNone(mac_route.mac_available([beat("queued", None, created=120, updated=120), beat()], NOW))

    def test_a_stale_failed_or_missing_heartbeat_means_unavailable(self):
        self.assertIn("no successful", mac_route.mac_available([], NOW))
        self.assertIn("no successful", mac_route.mac_available([beat(conclusion="failure")], NOW))
        self.assertIn("ago", mac_route.mac_available([beat(updated=mac_route.MAX_HEARTBEAT_AGE + 1)], NOW))

    def test_a_heartbeat_waiting_for_a_runner_means_off_or_busy(self):
        waiting = beat("queued", None, created=mac_route.MAX_HEARTBEAT_WAIT + 1, updated=0)
        self.assertIn("waited", mac_route.mac_available([waiting, beat()], NOW))


class SlotTests(unittest.TestCase):
    def test_only_unfinished_jobs_a_mac_runner_could_take_count(self):
        jobs = [job("queued", MAC), job("in_progress", ["self-hosted", "linux", "arm64"]), job("queued", ["self-hosted"]),
                job("completed", MAC), job("queued", ["ubuntu-24.04"]), job("queued", ["self-hosted", "mtg-ci-windows"]),
                job("queued", ["self-hosted", "macOS", "ARM64", "fleet-mac"]), job("queued", []),
                job("in_progress", MAC, run_id=9)]
        self.assertEqual(mac_route.held_by_mac(jobs), 4)
        self.assertEqual(mac_route.held_by_mac(jobs, skip_run=9), 3)

    def test_only_groups_this_run_has_on_linux_are_candidates(self):
        groups, eligible = ["python-shard-0", "standard", "nope", "standard", "limited"], ["python-shard-0", "standard", "limited"]
        self.assertEqual(mac_route.wanted(groups, eligible, "pr", True, True), ["python-shard-0", "standard", "limited"])
        self.assertEqual(mac_route.wanted(groups, eligible, "full", False, True), ["standard", "limited"])
        self.assertEqual(mac_route.wanted(groups, eligible, "pr", True, False), ["python-shard-0"])
        for scope in ("draft", "windows-pr", "windows-complement", "windows-full", ""):
            self.assertEqual(mac_route.wanted(groups, eligible, scope, True, True), [])

    def test_scoped_checks_do_not_spend_slots_on_absent_job_families(self):
        groups = ["workspace-targets", "limited-store", "limited-model-1", "python-shard-0"]
        for checks, expected in (("rules-limited", ["limited-model-1"]), ("python", ["python-shard-0"]),
                                 ("trainer", []), ("docs", [])):
            with self.subTest(checks=checks):
                self.assertEqual(mac_route.wanted(groups, groups, "pr", True, True, checks=checks), expected)


class MainTests(unittest.TestCase):
    def run_main(self, env, runs=(), jobs=(), error=None):
        with tempfile.TemporaryDirectory() as folder:
            output = Path(folder) / "out"
            def get(path, budget):
                if error:
                    raise error
                return {"workflow_runs": list(runs)}
            with patch.dict(os.environ, {**env, "GITHUB_OUTPUT": str(output)}, clear=True), \
                    patch.object(mac_route, "_get", get), patch.object(mac_route, "active_jobs", lambda repo, budget: list(jobs)), \
                    patch.object(mac_route.datetime, "datetime", wraps=mac_route.datetime.datetime) as clock:
                clock.now.return_value = mac_route.datetime.datetime.fromtimestamp(NOW, mac_route.datetime.timezone.utc)
                self.assertEqual(mac_route.main(), 0)
            text = output.read_text(encoding="utf-8")
        self.assertTrue(text.startswith("groups=") and text.endswith("\n"))
        return json.loads(text[len("groups="):])

    def test_free_slots_take_groups_in_order(self):
        self.assertEqual(self.run_main(ENV, runs=[beat()]), ["limited-only", "standard", "python-shard-0"])
        self.assertEqual(self.run_main(ENV, runs=[beat()], jobs=[job("queued", MAC, run_id=4)]), ["limited-only", "standard"])
        self.assertEqual(self.run_main(ENV, runs=[beat()], jobs=[job("queued", MAC, run_id=4)] * 3), [])
        self.assertEqual(self.run_main({**ENV, "MAC_SLOTS": "1"}, runs=[beat()]), ["limited-only"])

    def test_every_doubt_routes_nothing(self):
        self.assertEqual(self.run_main({**ENV, "MAC_GROUPS": ""}, runs=[beat()]), [])
        self.assertEqual(self.run_main({**ENV, "MAC_SLOTS": ""}, runs=[beat()]), [])
        self.assertEqual(self.run_main({**ENV, "MAC_SLOTS": "0"}, runs=[beat()]), [])
        self.assertEqual(self.run_main({**ENV, "MAC_GROUPS": "not json"}, runs=[beat()]), [])
        self.assertEqual(self.run_main({**ENV, "MAC_GROUPS": '"standard"'}, runs=[beat()]), [])
        self.assertEqual(self.run_main({**ENV, "SCOPE": "draft"}, runs=[beat()]), [])
        self.assertEqual(self.run_main(ENV, runs=[]), [])
        self.assertEqual(self.run_main(ENV, runs=[beat(updated=3600)]), [])
        self.assertEqual(self.run_main(ENV, error=TimeoutError("slow")), [])


if __name__ == "__main__":
    unittest.main()
