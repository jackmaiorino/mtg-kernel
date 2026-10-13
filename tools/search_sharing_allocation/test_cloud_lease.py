import tempfile
import time
import unittest
from pathlib import Path
from unittest.mock import patch

import cloud_lease as lease


class CloudLeaseOwnershipTests(unittest.TestCase):
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
