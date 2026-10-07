"""Storage admission checks for the Windows local/SSH training dispatcher."""
import json
import unittest
from unittest.mock import patch

import public_training_dispatch_v2 as dispatch


class DispatcherStorageAdmissionTests(unittest.TestCase):
    def test_reserve_boundary_is_exact(self):
        dispatch.require_disk_reserve(dispatch.DISK_RESERVE_BYTES)
        with self.assertRaisesRegex(ValueError, "60 GiB"):
            dispatch.require_disk_reserve(dispatch.DISK_RESERVE_BYTES - 1)

    def test_local_and_remote_preflight_refuse_below_reserve(self):
        for host, drive in (("desktop", "D"), ("computehost", "C")):
            with self.subTest(host=host):
                report = dict(host=dispatch.HOSTNAMES[host], active=[],
                              disk_free_bytes=dispatch.DISK_RESERVE_BYTES - 1, gpu=[])
                if host == "desktop":
                    context = patch.object(dispatch.subprocess, "check_output",
                                           return_value=json.dumps(report))
                else:
                    context = patch.object(dispatch, "ssh_ps", return_value=json.dumps(report))
                with context, self.assertRaisesRegex(ValueError, "60 GiB"):
                    dispatch.preflight(host, [], drive)

    def test_local_and_remote_preflight_accept_exact_reserve(self):
        for host, drive in (("desktop", "D"), ("computehost", "C")):
            with self.subTest(host=host):
                report = dict(host=dispatch.HOSTNAMES[host], active=[],
                              disk_free_bytes=dispatch.DISK_RESERVE_BYTES, gpu=[])
                if host == "desktop":
                    context = patch.object(dispatch.subprocess, "check_output",
                                           return_value=json.dumps(report))
                else:
                    context = patch.object(dispatch, "ssh_ps", return_value=json.dumps(report))
                with context:
                    result = dispatch.preflight(host, [], drive)
                self.assertEqual(result["disk_free_bytes"], dispatch.DISK_RESERVE_BYTES)


if __name__ == "__main__":
    unittest.main()
