"""Explicit remote destination checks without SSH, CUDA or native execution."""
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import Mock, patch

import public_training_dispatch_v2 as dispatch


class RemoteHostBindingTests(unittest.TestCase):
    def setUp(self):
        self.binding = dict(expected_hostname="HALEYSPC", input_root="C:/mtg-node/haley-inputs")
        self.placement = dict(host="computehost", gpu_ordinal=0, gpu_uuid="GPU-H0", workers=4,
                              remote_binding=self.binding)

    def report(self, hostname):
        return json.dumps(dict(host=hostname, active=[], disk_free_bytes=dispatch.DISK_RESERVE_BYTES,
                               gpu=["0, GPU-H0, 4096, 0"]))

    def test_explicit_preflight_accepts_expected_destination(self):
        with patch.object(dispatch, "ssh_ps", return_value=self.report("HALEYSPC")):
            result = dispatch.preflight("computehost", [self.placement], "C")
        self.assertEqual(result["host"], "HALEYSPC")

    def test_explicit_preflight_refuses_wrong_destination(self):
        with patch.object(dispatch, "ssh_ps", return_value=self.report("COMPUTEHOST")):
            with self.assertRaisesRegex(ValueError, "different host"):
                dispatch.preflight("computehost", [self.placement], "C")

    def test_remote_jobs_cannot_mix_bindings(self):
        other = dict(self.placement, remote_binding=dict(self.binding, input_root="C:/other-inputs"))
        with patch.object(dispatch, "ssh_ps") as ssh:
            with self.assertRaisesRegex(ValueError, "disagree"):
                dispatch.preflight("computehost", [self.placement, other], "C")
            ssh.assert_not_called()

    def test_input_root_must_be_absolute_local_windows_path(self):
        for root in ("relative", "C:/inputs/../other", "//server/share/inputs"):
            with self.subTest(root=root), self.assertRaisesRegex(ValueError, "absolute local Windows"):
                dispatch.host_binding("computehost", [dict(self.placement,
                    remote_binding=dict(self.binding, input_root=root))])

    def test_worker_refuses_wrong_host_before_gpu_or_native_work(self):
        with patch.object(dispatch.platform, "node", return_value="COMPUTEHOST"), \
                patch.object(dispatch, "gpu_uuid") as gpu, patch.object(dispatch.subprocess, "Popen") as spawn:
            with self.assertRaisesRegex(ValueError, "wrong host"):
                dispatch.worker(dict(placement=self.placement))
            gpu.assert_not_called()
            spawn.assert_not_called()

    def test_worker_refuses_wrong_input_runtime_binding(self):
        for environment in ({}, {dispatch.REMOTE_INPUT_ENV: self.binding["input_root"], "CUDA_PATH": "C:/other/runtime"}):
            with self.subTest(environment=environment), patch.dict(os.environ, environment, clear=True), \
                    patch.object(dispatch.platform, "node", return_value="HALEYSPC"), \
                    patch.object(dispatch.subprocess, "Popen") as spawn:
                with self.assertRaisesRegex(ValueError, "input/runtime binding"):
                    dispatch.worker(dict(placement=self.placement))
                spawn.assert_not_called()

    def test_worker_completes_with_explicit_binding_and_physical_gpu_zero(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            binary = root/"trainer.exe"
            binary.write_bytes(b"fixture; never executed")
            output = root/"outputs"
            output.mkdir()
            (output/"completion.json").write_text("{}", encoding="utf-8")
            request = root/"request.json"
            request.write_text(json.dumps(dict(execution_gpu_ordinal=0, collector_workers=4,
                                               output_directory=str(output))), encoding="utf-8")
            spec = dict(placement=self.placement, binary=dispatch.pin(binary), request=dispatch.pin(request), wall_seconds=30)
            child = Mock(pid=1234)
            child.poll.return_value = None
            child.wait.return_value = 0
            environment = {dispatch.REMOTE_INPUT_ENV: self.binding["input_root"],
                           "CUDA_PATH": self.binding["input_root"]+"/runtime"}
            with patch.dict(os.environ, environment, clear=True), \
                    patch.object(dispatch.platform, "node", return_value="HALEYSPC"), \
                    patch.object(dispatch, "gpu_uuid", return_value="GPU-H0") as gpu, \
                    patch.object(dispatch.subprocess, "check_output", return_value="1234, GPU-H0\n"), \
                    patch.object(dispatch.subprocess, "Popen", return_value=child):
                execution = dispatch.worker(spec)
            self.assertEqual(execution["exit_code"], 0)
            self.assertEqual(execution["placement"], self.placement)
            self.assertEqual(execution["observed_gpu_uuid"], "GPU-H0")
            gpu.assert_called_once_with(0)

    def test_legacy_defaults_need_no_explicit_input_binding(self):
        self.assertEqual(dispatch.host_binding("computehost", []),
                         dict(expected_hostname="COMPUTEHOST", input_root=dispatch.REMOTE_INPUT))
        placement = dict(self.placement)
        placement.pop("remote_binding")
        with patch.dict(os.environ, {}, clear=True), \
                patch.object(dispatch.platform, "node", return_value="COMPUTEHOST"), \
                patch.object(dispatch, "gpu_uuid", side_effect=RuntimeError("legacy GPU check reached")):
            with self.assertRaisesRegex(RuntimeError, "legacy GPU check reached"):
                dispatch.worker(dict(placement=placement))


if __name__ == "__main__":
    unittest.main()
