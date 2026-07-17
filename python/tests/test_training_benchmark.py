from __future__ import annotations

import json
import tempfile
import unittest
from pathlib import Path
from unittest import mock

from mtg_kernel_rl.artifacts import canonical_json_bytes, read_json_file, sha256_file, write_bytes_atomic, write_json_atomic
from mtg_kernel_rl.phase_profile import KERNEL_PHASES, KERNEL_PROFILE_CLOCK, KERNEL_PROFILE_PREFIX, KERNEL_PROFILE_SCHEMA
from mtg_kernel_rl.training_benchmark import BENCHMARK_SCHEMA, _source_record, benchmark_training

COMMIT = "7735d368117c20211c66e72cb5efc71e1bd4d74f"


def profile_record(resets: int = 2, steps: int = 11) -> str:
    requests = resets + steps
    phases = {
        phase: {"count": 0, "total_ns": 0, "max_ns": 0}
        for phase in KERNEL_PHASES
    }
    for phase in ("parse", "decode", "retry", "response", "serialize", "write_flush"):
        phases[phase] = {"count": requests, "total_ns": requests, "max_ns": 1}
    phases["reset"] = {"count": resets, "total_ns": resets, "max_ns": 1}
    for phase in ("step_validation", "step_integrity", "step_selection", "step_apply"):
        phases[phase] = {"count": steps, "total_ns": steps, "max_ns": 1}
    phases["advance"] = {"count": requests, "total_ns": requests, "max_ns": 1}
    for phase in ("observe", "actions", "postbind"):
        phases[phase] = {"count": steps, "total_ns": steps, "max_ns": 1}
    value = {
        "schema": KERNEL_PROFILE_SCHEMA,
        "clock": KERNEL_PROFILE_CLOCK,
        "request_lines": requests,
        "response_lines": requests,
        "reset_requests": resets,
        "step_requests": steps,
        "phases": phases,
    }
    return KERNEL_PROFILE_PREFIX + json.dumps(value, sort_keys=True, separators=(",", ":")) + "\n"


def fake_train(**kwargs):
    store = Path(kwargs["out_dir"])
    store.mkdir(parents=True)
    recorder = kwargs["phase_recorder"]
    if recorder is not None:
        recorder.add_kernel_stderr(profile_record())
        phase_counts = {
            "ipc_encode": 13,
            "ipc_write_flush": 13,
            "ipc_wait_read": 13,
            "ipc_decode": 13,
            "ipc_validate": 13,
            "feature_tensor": 5,
            "model_forward": 4,
            "action_sample": 11,
            "trajectory_hash": 9,
            "loss_build": 1,
            "backward": 1,
            "optimizer": 1,
            "checkpoint_build": 2,
            "publication": 2,
        }
        for phase, count in phase_counts.items():
            for _ in range(count):
                with recorder.measure(phase):
                    pass
    env_sha = sha256_file(kwargs["env_bin"])
    run = {
        "schema": "kernel_rl_training_run/v9",
        "package": {"name": "mtg-kernel-rl", "version": "0.5.0"},
        "algorithm": {"name": "terminal_reinforce_value/v3"},
        "environment": {
            "binary_sha256": env_sha,
            "deck_ids": ["Rally", "Rally"],
            "deck_hashes": [908_320_065_233_343_167, 908_320_065_233_343_167],
        },
        "protocol": {"protocol": "kernel_rl_jsonl", "protocol_version": 5},
        "protocol_provenance": {"kernel_version": "fake"},
        "model": {"config": {"hidden_dim": 8}, "contract_fingerprint": "a" * 64},
        "feature_contract": {
            "feature_schema_version": 12,
            "feature_registry_version": 8,
            "feature_contract_digest": "b" * 64,
            "feature_encoding_digest": "c" * 64,
        },
        "initializer": {"name": "fake"},
        "optimizer": {"algorithm": "adam", "lr": kwargs["learning_rate"]},
        "samplers": {"learner": {"name": "fake"}, "opponent": {"name": "fake"}},
        "schedule": {"batch_episodes": kwargs["batch_episodes"]},
        "trainer": {
            "base_seed": kwargs["base_seed"],
            "value_coef": kwargs["value_coef"],
            "max_physical_decisions": kwargs["max_physical_decisions"],
            "max_policy_steps": kwargs["max_policy_steps"],
        },
        "seed_derivation": {"name": "fake"},
        "compatibility": {"cpu_only": True},
    }
    write_json_atomic(store / "run.json", run)
    update0 = {"update": 0, "episode_summaries": [], "learner_policy_step_count": 0, "learner_physical_decision_count": 0}
    update1 = {
        "update": 1,
        "optimizer_step": True,
        "episode_summaries": [
            {"policy_step_count": 5, "physical_decision_count": 4},
            {"policy_step_count": 6, "physical_decision_count": 5},
        ],
        "learner_policy_step_count": 4,
        "learner_physical_decision_count": 3,
    }
    write_bytes_atomic(
        store / "updates.jsonl",
        canonical_json_bytes(update0) + canonical_json_bytes(update1),
    )
    return {
        "run_digest": sha256_file(store / "run.json"),
        "completed_update": 1,
        "next_episode": 2,
        "optimizer_step_count": 1,
        "head": "e" * 64,
        "logical_state_sha256": "f" * 64,
    }


class FakeChain:
    def __init__(self, root: Path):
        self.run_record = read_json_file(root / "run.json")
        self.update_records = tuple(
            json.loads(line)
            for line in (root / "updates.jsonl").read_text(encoding="utf-8").splitlines()
        )


class FakeTrainingStore:
    def __init__(self, root: str | Path):
        self.root = Path(root)

    def validate_latest(self) -> FakeChain:
        return FakeChain(self.root)


class TrainingBenchmarkTest(unittest.TestCase):
    def test_manifest_is_versioned_relative_and_contains_trials(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_name:
            tmp = Path(tmp_name)
            env_bin = tmp / "kernel_rl_env.exe"
            env_bin.write_bytes(b"benchmark-binary")
            out = tmp / "benchmark"
            with (
                mock.patch("mtg_kernel_rl.training_benchmark.train", side_effect=fake_train),
                mock.patch("mtg_kernel_rl.training_benchmark.TrainingStore", FakeTrainingStore),
            ):
                manifest = benchmark_training(
                    env_bin=env_bin,
                    out_dir=out,
                    git_commit=COMMIT,
                    profile_mode="off",
                    deck_id="Rally",
                    trials=2,
                    until_update=1,
                    batch_episodes=2,
                    base_seed=71501,
                    learning_rate=0.001,
                    value_coef=0.5,
                    max_physical_decisions=8,
                    max_policy_steps=16,
                )
            self.assertEqual(manifest["schema"], BENCHMARK_SCHEMA)
            self.assertEqual(
                manifest["source"]["repo_state_verification"],
                "user_supplied_unverified/v1",
            )
            self.assertEqual(manifest["workload"]["profile_mode"], "off")
            self.assertEqual(
                manifest["workload"]["throughput_role"],
                "primary_uninstrumented/v1",
            )
            self.assertIsNone(manifest["trials"][0]["phase_profile"])
            self.assertFalse(manifest["workload"]["steady_state_claim"])
            self.assertEqual(manifest["workload"]["warmup_updates_excluded"], 0)
            self.assertEqual(len(manifest["trials"]), 2)
            self.assertEqual(manifest["trials"][0]["training_store"], "trial-000/training-store")
            self.assertEqual(manifest["aggregate"]["counts"]["episodes"], 4)
            self.assertEqual(manifest["aggregate"]["counts"]["policy_steps"], 22)
            self.assertEqual(
                read_json_file(out / "benchmark.json"),
                manifest,
            )
            serialized = json.dumps(manifest, sort_keys=True)
            self.assertNotIn(str(tmp), serialized)
            self.assertNotIn("file:", serialized.lower())

    def test_phase_mode_is_diagnostic_and_cross_bound_to_work(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_name:
            tmp = Path(tmp_name)
            env_bin = tmp / "kernel_rl_env.exe"
            env_bin.write_bytes(b"benchmark-binary")
            with (
                mock.patch("mtg_kernel_rl.training_benchmark.train", side_effect=fake_train),
                mock.patch("mtg_kernel_rl.training_benchmark.TrainingStore", FakeTrainingStore),
            ):
                manifest = benchmark_training(
                    env_bin=env_bin,
                    out_dir=tmp / "benchmark",
                    git_commit=COMMIT,
                    profile_mode="phase_v1",
                    deck_id="Rally",
                    trials=1,
                    until_update=1,
                    batch_episodes=2,
                    base_seed=71501,
                    learning_rate=0.001,
                    value_coef=0.5,
                    max_physical_decisions=8,
                    max_policy_steps=16,
                )
            self.assertEqual(
                manifest["workload"]["throughput_role"],
                "diagnostic_phase_attribution/v1",
            )
            self.assertEqual(
                manifest["trials"][0]["phase_profile"]["kernel_records"][0][
                    "request_lines"
                ],
                13,
            )

    def test_missing_profile_record_fails_benchmark_collection(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_name:
            tmp = Path(tmp_name)
            env_bin = tmp / "kernel_rl_env"
            env_bin.write_bytes(b"benchmark-binary")

            def no_profile(**kwargs):
                return {"completed_update": 1}

            with mock.patch("mtg_kernel_rl.training_benchmark.train", side_effect=no_profile):
                with self.assertRaises(ValueError):
                    benchmark_training(
                        env_bin=env_bin,
                        out_dir=tmp / "benchmark",
                        git_commit=COMMIT,
                        profile_mode="phase_v1",
                        deck_id="Rally",
                        trials=1,
                        until_update=1,
                        batch_episodes=2,
                        base_seed=71501,
                        learning_rate=0.001,
                        value_coef=0.5,
                        max_physical_decisions=8,
                        max_policy_steps=16,
                    )

    def test_authoritative_store_validation_failure_rejects_manifest(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_name:
            tmp = Path(tmp_name)
            env_bin = tmp / "kernel_rl_env"
            env_bin.write_bytes(b"benchmark-binary")
            broken_store = mock.Mock()
            broken_store.validate_latest.side_effect = ValueError("corrupt authoritative chain")
            with (
                mock.patch("mtg_kernel_rl.training_benchmark.train", side_effect=fake_train),
                mock.patch("mtg_kernel_rl.training_benchmark.TrainingStore", return_value=broken_store),
            ):
                with self.assertRaisesRegex(ValueError, "corrupt authoritative chain"):
                    benchmark_training(
                        env_bin=env_bin,
                        out_dir=tmp / "benchmark",
                        git_commit=COMMIT,
                        profile_mode="off",
                        deck_id="Rally",
                        trials=1,
                        until_update=1,
                        batch_episodes=2,
                        base_seed=71501,
                        learning_rate=0.001,
                        value_coef=0.5,
                        max_physical_decisions=8,
                        max_policy_steps=16,
                    )

    def test_authoritative_run_and_phase_count_mismatches_fail_closed(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_name:
            tmp = Path(tmp_name)
            env_bin = tmp / "kernel_rl_env"
            env_bin.write_bytes(b"benchmark-binary")

            def wrong_binary_run(**kwargs):
                result = fake_train(**kwargs)
                run_path = Path(kwargs["out_dir"]) / "run.json"
                run = read_json_file(run_path)
                run["environment"]["binary_sha256"] = "0" * 64
                write_json_atomic(run_path, run)
                result["run_digest"] = sha256_file(run_path)
                return result

            with (
                mock.patch(
                    "mtg_kernel_rl.training_benchmark.train", side_effect=wrong_binary_run
                ),
                mock.patch(
                    "mtg_kernel_rl.training_benchmark.TrainingStore", FakeTrainingStore
                ),
            ):
                with self.assertRaisesRegex(ValueError, "binary digest mismatch"):
                    benchmark_training(
                        env_bin=env_bin,
                        out_dir=tmp / "wrong-binary",
                        git_commit=COMMIT,
                        profile_mode="off",
                        deck_id="Rally",
                        trials=1,
                        until_update=1,
                        batch_episodes=2,
                        base_seed=71501,
                        learning_rate=0.001,
                        value_coef=0.5,
                        max_physical_decisions=8,
                        max_policy_steps=16,
                    )

            def wrong_profile_work(**kwargs):
                result = fake_train(**kwargs)
                updates = Path(kwargs["out_dir"]) / "updates.jsonl"
                records = [
                    json.loads(line)
                    for line in updates.read_text(encoding="utf-8").splitlines()
                ]
                records[1]["episode_summaries"][0]["policy_step_count"] = 4
                write_bytes_atomic(
                    updates,
                    b"".join(canonical_json_bytes(record) for record in records),
                )
                return result

            with (
                mock.patch(
                    "mtg_kernel_rl.training_benchmark.train", side_effect=wrong_profile_work
                ),
                mock.patch(
                    "mtg_kernel_rl.training_benchmark.TrainingStore", FakeTrainingStore
                ),
            ):
                with self.assertRaisesRegex(ValueError, "does not bind"):
                    benchmark_training(
                        env_bin=env_bin,
                        out_dir=tmp / "wrong-profile",
                        git_commit=COMMIT,
                        profile_mode="phase_v1",
                        deck_id="Rally",
                        trials=1,
                        until_update=1,
                        batch_episodes=2,
                        base_seed=71501,
                        learning_rate=0.001,
                        value_coef=0.5,
                        max_physical_decisions=8,
                        max_policy_steps=16,
                    )

    def test_source_claim_can_be_verified_without_publishing_repo_path(self) -> None:
        completed = [
            mock.Mock(stdout=COMMIT + "\n"),
            mock.Mock(stdout=""),
        ]
        with mock.patch("mtg_kernel_rl.training_benchmark.subprocess.run", side_effect=completed):
            record = _source_record(COMMIT, Path.cwd())
        self.assertEqual(
            record["repo_state_verification"], "git_head_and_clean_worktree/v1"
        )
        self.assertEqual(record["executed_python_source_binding"], "unverified/v1")
        self.assertEqual(record["binary_source_binding"], "unverified/v1")
        self.assertNotIn("root", record)

        dirty = [mock.Mock(stdout=COMMIT + "\n"), mock.Mock(stdout=" M file\n")]
        with mock.patch("mtg_kernel_rl.training_benchmark.subprocess.run", side_effect=dirty):
            with self.assertRaisesRegex(ValueError, "not clean"):
                _source_record(COMMIT, Path.cwd())

    def test_commit_deck_and_trial_inputs_fail_closed(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_name:
            tmp = Path(tmp_name)
            env_bin = tmp / "kernel_rl_env"
            env_bin.write_bytes(b"x")
            for commit, deck, trials in (("bad", "Rally", 1), (COMMIT, "rally", 1), (COMMIT, "Rally", 0)):
                with self.subTest(commit=commit, deck=deck, trials=trials):
                    with self.assertRaises(ValueError):
                        benchmark_training(
                            env_bin=env_bin,
                            out_dir=tmp / f"out-{trials}-{deck}",
                            git_commit=commit,
                            profile_mode="off",
                            deck_id=deck,
                            trials=trials,
                            until_update=1,
                            batch_episodes=2,
                            base_seed=71501,
                            learning_rate=0.001,
                            value_coef=0.5,
                            max_physical_decisions=8,
                            max_policy_steps=16,
                        )


if __name__ == "__main__":
    unittest.main()
