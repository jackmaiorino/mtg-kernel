import tempfile
import io
import json
import time
import unittest
import urllib.error
import urllib.request
from pathlib import Path
from unittest.mock import patch

import cloud_lease as lease
import diagnostic_create


class CloudLeaseOwnershipTests(unittest.TestCase):
    def test_provider_diagnostic_keeps_safe_status_and_redacts_credentials(self):
        with tempfile.TemporaryDirectory() as folder:
            diagnostic_create.DiagnosedProvider.control = Path(folder)
            api = diagnostic_create.DiagnosedProvider("memory-only-secret")
            error = urllib.error.HTTPError("https://example.invalid", 500, "failure", {},
                    io.BytesIO(json.dumps({"error": "no capacity", "detail": "memory-only-secret",
                                           "env": {"RUNPOD_API_KEY": "must-not-save"}}).encode()))
            with patch.object(diagnostic_create.urllib.request, "urlopen", side_effect=error):
                with self.assertRaisesRegex(RuntimeError, "provider HTTP 500"):
                    api._call(urllib.request.Request("https://example.invalid"))
            receipt = next(Path(folder).glob("create-http-error-*.json")).read_text()
            self.assertNotIn("memory-only-secret", receipt)
            self.assertNotIn("must-not-save", receipt)
            self.assertEqual(json.loads(receipt)["http_status"], 500)
            self.assertEqual(json.loads(receipt)["safe_error_fields"]["error"], "no capacity")

    def test_replacement_preserves_absolute_deadline_and_cumulative_cap(self):
        with tempfile.TemporaryDirectory() as folder:
            key = Path(folder) / "existing.pub"
            key.write_text("ssh-ed25519 ZmFrZQ== existing")
            now, deadline = 10000, 20000
            root = Path(folder) / "replacement"
            result = lease.prepare(root, "search-sharing-allocation-20261013-replacement", key,
                                   "US-MD-1", now, deadline_epoch=deadline, prior_conservative_usd=1)
            saved = lease.read(root / "lease/lease.json")
            self.assertEqual(saved["deadline_epoch"], deadline)
            self.assertEqual(saved["total_cap_usd"], 5)
            self.assertEqual(saved["increment_cap_usd"], 4)
            self.assertEqual(saved["prior_conservative_usd"], 1)
            self.assertLessEqual(result["projected_cumulative_conservative_usd"], 5)
            self.assertEqual(lease.lease_guard.decision(saved, deadline - 600, 1)["reason"], "deadline")
            for options in ({"prior_conservative_usd": 1},
                            {"deadline_epoch": now + 14401, "prior_conservative_usd": 1},
                            {"deadline_epoch": deadline, "prior_conservative_usd": 4},
                            {"deadline_epoch": deadline, "prior_conservative_usd": float("nan")}):
                rejected = Path(folder) / "rejected"
                with self.assertRaises(ValueError):
                    lease.prepare(rejected, "search-sharing-allocation-20261013-rejected", key,
                                  "US-MD-1", now, **options)
                self.assertFalse(rejected.exists())

    def control(self, folder):
        root = Path(folder) / "control"
        key = Path(folder) / "existing.pub"
        key.write_text("ssh-ed25519 ZmFrZQ== existing")
        lease.prepare(root, "search-sharing-allocation-20261013-test", key, "US-MD-1")
        pid, creation = lease.host_slots_v1.self_identity()
        lease.write(root / "guardian-ready.json", {
            "name": "search-sharing-allocation-20261013-test", "pid": pid,
            "creation_time": creation, "epoch": time.time()})
        return root

    def test_reconciled_owned_pod_does_not_create_again_or_persist_credentials(self):
        with tempfile.TemporaryDirectory() as folder:
            root = self.control(folder)
            pod = {"id": "owned123", "name": "search-sharing-allocation-20261013-test",
                   "costPerHr": 0.96, "desiredStatus": "RUNNING",
                   "env": {"RUNPOD_API_KEY": "must-never-be-persisted"}}
            with patch.object(lease.external_guard, "api_key", return_value="memory-only-key"), \
                    patch.object(lease.external_guard, "list_pods", return_value=[pod]), \
                    patch.object(lease.prepare_lease.Provider, "create") as create:
                result = lease.create(root, True)
            create.assert_not_called()
            self.assertEqual(result["id"], "owned123")
            text = (root / "lease/created-pod.json").read_text()
            self.assertNotIn("must-never-be-persisted", text)
            self.assertNotIn("memory-only-key", text)

    def test_ambiguous_or_foreign_identity_never_creates_or_copies(self):
        with tempfile.TemporaryDirectory() as folder:
            root = self.control(folder)
            pods = [{"id": i, "name": "search-sharing-allocation-20261013-test"}
                    for i in ("a", "b")]
            with patch.object(lease.external_guard, "api_key", return_value="memory-only-key"), \
                    patch.object(lease.external_guard, "list_pods", return_value=pods), \
                    patch.object(lease.prepare_lease.Provider, "create") as create:
                with self.assertRaisesRegex(ValueError, "ambiguous"):
                    lease.create(root, True)
            create.assert_not_called()
            lease.write(root / "connection.json", {"pod_id": "foreign123"})
            with patch.object(lease.subprocess, "run") as scp:
                with self.assertRaisesRegex(ValueError, "another Pod"):
                    lease.preserve(root, "owned123")
            scp.assert_not_called()
            with self.assertRaises(ValueError):
                lease.safe_pod({"id": "foreign123", "name": "another-lane"}, "owned-lane")


if __name__ == "__main__":
    unittest.main()
