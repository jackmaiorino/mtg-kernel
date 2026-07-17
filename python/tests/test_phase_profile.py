from __future__ import annotations

import copy
import json
import os
import tempfile
import unittest
from pathlib import Path

from fixtures import fake_launcher
from mtg_kernel_rl.client import KernelRlClient
from mtg_kernel_rl.phase_profile import (
    KERNEL_PHASES,
    KERNEL_PROFILE_CLOCK,
    KERNEL_PROFILE_PREFIX,
    KERNEL_PROFILE_SCHEMA,
    PhaseRecorder,
    parse_kernel_profile_stderr,
)
from mtg_kernel_rl.trainer import train


def valid_kernel_profile() -> dict:
    phases = {
        phase: {"count": 0, "total_ns": 0, "max_ns": 0}
        for phase in KERNEL_PHASES
    }
    for phase in ("parse", "decode", "retry", "reset", "response", "serialize", "write_flush"):
        phases[phase] = {"count": 1, "total_ns": 7, "max_ns": 7}
    return {
        "schema": KERNEL_PROFILE_SCHEMA,
        "clock": KERNEL_PROFILE_CLOCK,
        "request_lines": 1,
        "response_lines": 1,
        "reset_requests": 1,
        "step_requests": 0,
        "phases": phases,
    }


def profile_stderr(value: dict) -> str:
    return KERNEL_PROFILE_PREFIX + json.dumps(value, sort_keys=True, separators=(",", ":")) + "\n"


class PhaseProfileTest(unittest.TestCase):
    def test_strict_kernel_profile_accepts_one_complete_record(self) -> None:
        self.assertEqual(
            parse_kernel_profile_stderr(profile_stderr(valid_kernel_profile())),
            valid_kernel_profile(),
        )

    def test_missing_duplicate_malformed_and_unknown_records_fail(self) -> None:
        valid = profile_stderr(valid_kernel_profile())
        for bad in (
            "",
            valid + valid,
            "MTG_KERNEL_PROFILE_V1 {}\n",
            KERNEL_PROFILE_PREFIX + "{\n",
            "noise\n" + valid,
        ):
            with self.subTest(bad=bad[:30]):
                with self.assertRaises(ValueError):
                    parse_kernel_profile_stderr(bad)

        unknown = valid_kernel_profile()
        unknown["future"] = 1
        with self.assertRaises(ValueError):
            parse_kernel_profile_stderr(profile_stderr(unknown))
        duplicate_json = profile_stderr(valid_kernel_profile()).rstrip()[:-1] + ',"schema":"kernel_rl_phase_profile/v1"}\n'
        with self.assertRaises(ValueError):
            parse_kernel_profile_stderr(duplicate_json)

    def test_count_and_counter_invariants_fail_closed(self) -> None:
        cases = []
        mismatch = valid_kernel_profile()
        mismatch["response_lines"] = 0
        cases.append(mismatch)
        mismatch = copy.deepcopy(valid_kernel_profile())
        mismatch["phases"]["parse"]["max_ns"] = 8
        cases.append(mismatch)
        mismatch = copy.deepcopy(valid_kernel_profile())
        mismatch["phases"]["retry"]["count"] = 0
        cases.append(mismatch)
        mismatch = copy.deepcopy(valid_kernel_profile())
        mismatch["phases"]["future"] = {"count": 0, "total_ns": 0, "max_ns": 0}
        cases.append(mismatch)
        mismatch = copy.deepcopy(valid_kernel_profile())
        mismatch["step_requests"] = 1
        mismatch["phases"]["step_selection"] = {"count": 1, "total_ns": 1, "max_ns": 1}
        cases.append(mismatch)
        mismatch = copy.deepcopy(valid_kernel_profile())
        mismatch["phases"]["postbind"] = {"count": 1, "total_ns": 1, "max_ns": 1}
        cases.append(mismatch)
        mismatch = copy.deepcopy(valid_kernel_profile())
        mismatch["phases"]["advance"] = {"count": 2, "total_ns": 2, "max_ns": 1}
        cases.append(mismatch)
        for value in cases:
            with self.subTest(value=value):
                with self.assertRaises(ValueError):
                    parse_kernel_profile_stderr(profile_stderr(value))

    def test_python_recorder_is_external_and_client_covers_ipc_path(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_name:
            launcher = fake_launcher(Path(tmp_name), "valid")
            recorder = PhaseRecorder()
            with KernelRlClient(launcher, phase_recorder=recorder) as client:
                client.reset(
                    episode_id=0,
                    env_seed=1,
                    max_physical_decisions=8,
                    max_policy_steps=16,
                )
            snapshot = recorder.snapshot()
            self.assertEqual(snapshot["kernel_records"], [])
            for phase in (
                "ipc_encode",
                "ipc_write_flush",
                "ipc_wait_read",
                "ipc_decode",
                "ipc_validate",
            ):
                self.assertEqual(snapshot["python_phases"][phase]["count"], 1)

    def test_profiled_client_fails_collection_when_record_is_missing(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_name:
            launcher = fake_launcher(Path(tmp_name), "valid")
            recorder = PhaseRecorder()
            client = KernelRlClient(
                launcher,
                phase_recorder=recorder,
                kernel_phase_profile=True,
            )
            client.reset(
                episode_id=0,
                env_seed=1,
                max_physical_decisions=8,
                max_policy_steps=16,
            )
            with self.assertRaises(ValueError):
                client.close()

    def test_profile_collection_failure_does_not_mask_active_exception(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_name:
            launcher = fake_launcher(Path(tmp_name), "valid")
            recorder = PhaseRecorder()
            with self.assertRaisesRegex(RuntimeError, "primary failure"):
                with KernelRlClient(
                    launcher,
                    phase_recorder=recorder,
                    kernel_phase_profile=True,
                ) as client:
                    client.reset(
                        episode_id=0,
                        env_seed=1,
                        max_physical_decisions=8,
                        max_policy_steps=16,
                    )
                    raise RuntimeError("primary failure")

    def test_python_profile_on_training_store_is_byte_identical(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_name:
            tmp = Path(tmp_name)
            launcher = fake_launcher(tmp, "train_pair")
            plain = tmp / "plain"
            profiled = tmp / "profiled"
            options = {
                "env_bin": launcher,
                "base_seed": 71501,
                "until_update": 1,
                "batch_episodes": 2,
                "learning_rate": 0.001,
                "value_coef": 0.5,
                "max_physical_decisions": 8,
                "max_policy_steps": 16,
            }
            plain_result = train(out_dir=plain, **options)
            recorder = PhaseRecorder()
            profiled_result = train(
                out_dir=profiled,
                phase_recorder=recorder,
                **options,
            )
            self.assertEqual(profiled_result, plain_result)
            plain_files = {
                path.relative_to(plain).as_posix(): path.read_bytes()
                for path in plain.rglob("*")
                if path.is_file()
            }
            profiled_files = {
                path.relative_to(profiled).as_posix(): path.read_bytes()
                for path in profiled.rglob("*")
                if path.is_file()
            }
            self.assertEqual(profiled_files, plain_files)
            snapshot = recorder.snapshot()
            self.assertGreater(snapshot["python_phases"]["ipc_wait_read"]["count"], 0)
            self.assertGreater(snapshot["python_phases"]["model_forward"]["count"], 0)
            self.assertEqual(snapshot["kernel_records"], [])

    def test_real_kernel_profile_on_bootstrap_store_is_byte_identical(self) -> None:
        env_value = os.environ.get("MTG_KERNEL_RL_ENV_BIN")
        if not env_value:
            self.skipTest("MTG_KERNEL_RL_ENV_BIN not set")
        env_bin = Path(env_value)
        self.assertTrue(env_bin.is_file(), f"MTG_KERNEL_RL_ENV_BIN is not a file: {env_bin}")
        with tempfile.TemporaryDirectory() as tmp_name:
            tmp = Path(tmp_name)
            plain = tmp / "plain-real"
            profiled = tmp / "profiled-real"
            options = {
                "env_bin": env_bin,
                "base_seed": 71501,
                "until_update": 0,
                "batch_episodes": 2,
                "learning_rate": 0.001,
                "value_coef": 0.5,
                "max_physical_decisions": 64,
                "max_policy_steps": 8_192,
            }
            plain_result = train(out_dir=plain, **options)
            recorder = PhaseRecorder()
            profiled_result = train(
                out_dir=profiled,
                phase_recorder=recorder,
                kernel_phase_profile=True,
                **options,
            )
            self.assertEqual(profiled_result, plain_result)
            plain_files = {
                path.relative_to(plain).as_posix(): path.read_bytes()
                for path in plain.rglob("*")
                if path.is_file()
            }
            profiled_files = {
                path.relative_to(profiled).as_posix(): path.read_bytes()
                for path in profiled.rglob("*")
                if path.is_file()
            }
            self.assertEqual(profiled_files, plain_files)
            snapshot = recorder.snapshot()
            self.assertEqual(len(snapshot["kernel_records"]), 1)
            self.assertEqual(snapshot["kernel_records"][0]["request_lines"], 1)
            self.assertEqual(snapshot["kernel_records"][0]["reset_requests"], 1)
            self.assertEqual(snapshot["kernel_records"][0]["step_requests"], 0)


if __name__ == "__main__":
    unittest.main()
