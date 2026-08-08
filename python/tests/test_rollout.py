from __future__ import annotations

import filecmp
import json
import random
import tempfile
import unittest
from pathlib import Path
from unittest import mock

import torch

import mtg_kernel_rl.cli as cli_mod
import mtg_kernel_rl.rollout as rollout_mod
from mtg_kernel_rl.action_sampling import fixed_categorical_sampler_contract
from mtg_kernel_rl.client import Decision
from mtg_kernel_rl.path_safety import OUTPUT_LOCK_FILE_NAME
from mtg_kernel_rl.rollout import (
    RUNNER_ACTION_SELECTION_CONTRACT,
    RUNNER_ARTIFACT_SCHEMA_VERSION,
    RUNNER_SEED_DERIVATION_CONTRACT,
    V1_RUNNER_ARTIFACT_SCHEMA_VERSION,
    run_episodes,
    select_action,
)
from mtg_kernel_rl.runner_store import validate_runner_artifacts
from mtg_kernel_rl.sampled_evaluation_store import ACTION_SELECTION_CONTRACT as EVALUATOR_ACTION_SELECTION_CONTRACT
from mtg_kernel_rl.trainer import train
from mtg_kernel_rl.training_store import TrainingStore
from mtg_kernel_rl.training_store import TRAINER_ACTION_SELECTION_CONTRACT

from fixtures import DECK_HASHES, DECK_IDS, PROVENANCE, actor_observation, combat_decision_response, fake_launcher, legal_actions


class _FixedModelPolicy:
    def logits_value(self, _encoded):  # type: ignore[no-untyped-def]
        return torch.tensor([0.0, 1.0, 2.0], dtype=torch.float32), torch.tensor(0.0, dtype=torch.float32)


class _BinaryFixedModelPolicy:
    def logits_value(self, _encoded):  # type: ignore[no-untyped-def]
        return torch.tensor([0.0, 1.0], dtype=torch.float32), torch.tensor(0.0, dtype=torch.float32)


def _trained_policy_fixture(root: Path, launcher: Path, name: str) -> tuple[Path, str]:
    store = root / name
    train(
        env_bin=launcher,
        out_dir=store,
        base_seed=71_501,
        until_update=1,
        batch_episodes=2,
        learning_rate=0.001,
        value_coef=0.5,
        max_physical_decisions=8,
        max_policy_steps=16,
    )
    return store, TrainingStore(store).validate_latest().head.head


class RolloutTest(unittest.TestCase):
    def test_cli_preserves_uniform_run_and_accepts_checkpoint_binding(self) -> None:
        common = [
            "run",
            "--env-bin",
            "kernel_rl_env",
            "--out-dir",
            "runner-out",
            "--episodes",
            "2",
            "--base-seed",
            "71501",
            "--max-physical-decisions",
            "8",
            "--max-policy-steps",
            "16",
        ]
        uniform = cli_mod.build_parser().parse_args([*common, "--p0", "uniform", "--p1", "uniform"])
        self.assertIsNone(uniform.training_store)
        self.assertIsNone(uniform.expected_policy_head)
        neural = cli_mod.build_parser().parse_args(
            [
                *common,
                "--p0",
                "sampled",
                "--p1",
                "greedy",
                "--training-store",
                "training-store",
                "--expected-policy-head",
                "a" * 64,
            ]
        )
        self.assertEqual(neural.training_store, Path("training-store"))
        self.assertEqual(neural.expected_policy_head, "a" * 64)

    def test_neural_policies_fail_closed_without_exact_trained_head(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_name:
            tmp = Path(tmp_name)
            launcher = fake_launcher(tmp, "valid")
            with self.assertRaisesRegex(ValueError, "require training_store and expected_policy_head"):
                run_episodes(
                    env_bin=launcher,
                    out_dir=tmp / "missing-store-output",
                    episodes=1,
                    base_seed=71_501,
                    max_physical_decisions=8,
                    max_policy_steps=16,
                    p0="greedy",
                    p1="uniform",
                )
            self.assertFalse((tmp / "missing-store-output").exists())

            update_zero = tmp / "update-zero-store"
            train(
                env_bin=launcher,
                out_dir=update_zero,
                base_seed=71_501,
                until_update=0,
                batch_episodes=2,
                learning_rate=0.001,
                value_coef=0.5,
                max_physical_decisions=8,
                max_policy_steps=16,
            )
            zero_head = TrainingStore(update_zero).validate_latest().head.head
            with self.assertRaisesRegex(ValueError, "trained head after update zero"):
                run_episodes(
                    env_bin=launcher,
                    out_dir=tmp / "untrained-output",
                    episodes=1,
                    base_seed=71_501,
                    max_physical_decisions=8,
                    max_policy_steps=16,
                    p0="sampled",
                    p1="sampled",
                    training_store=update_zero,
                    expected_policy_head=zero_head,
                )
            self.assertFalse((tmp / "untrained-output").exists())

            trained_store, trained_head = _trained_policy_fixture(tmp, launcher, "trained-store")
            wrong_head = ("0" if trained_head[0] != "0" else "1") + trained_head[1:]
            with self.assertRaisesRegex(ValueError, "does not match the validated training head"):
                run_episodes(
                    env_bin=launcher,
                    out_dir=tmp / "wrong-head-output",
                    episodes=1,
                    base_seed=71_501,
                    max_physical_decisions=8,
                    max_policy_steps=16,
                    p0="greedy",
                    p1="uniform",
                    training_store=trained_store,
                    expected_policy_head=wrong_head,
                )
            self.assertFalse((tmp / "wrong-head-output").exists())

    def test_post_execution_model_reattestation_failure_publishes_no_authoritative_artifacts(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_name:
            tmp = Path(tmp_name)
            launcher = fake_launcher(tmp, "valid")
            store, head = _trained_policy_fixture(tmp, launcher, "reattestation-store")
            out = tmp / "reattestation-output"
            original_load_policy = rollout_mod.ValidatedChain.load_policy
            load_count = 0

            def load_policy_with_second_read_tamper(chain, ref=None):  # type: ignore[no-untyped-def]
                nonlocal load_count
                snapshot = original_load_policy(chain, ref)
                load_count += 1
                if load_count == 2:
                    with torch.no_grad():
                        snapshot.model.card_embedding.weight[1, 0].add_(1.0)
                return snapshot

            with mock.patch.object(
                rollout_mod.ValidatedChain,
                "load_policy",
                new=load_policy_with_second_read_tamper,
            ):
                with self.assertRaises(ValueError) as raised:
                    run_episodes(
                        env_bin=launcher,
                        out_dir=out,
                        episodes=1,
                        base_seed=71_501,
                        max_physical_decisions=8,
                        max_policy_steps=16,
                        p0="greedy",
                        p1="uniform",
                        training_store=store,
                        expected_policy_head=head,
                    )
            self.assertEqual(load_count, 2)
            self.assertEqual(
                str(raised.exception),
                "runner policy model tensor card_embedding.weight differs from its checkpoint attestation",
            )
            self.assertEqual({path.name for path in out.iterdir()}, {OUTPUT_LOCK_FILE_NAME})
            self.assertFalse((out / "episodes.jsonl").exists())
            self.assertFalse((out / "run.json").exists())

    def test_runner_policy_seeds_use_physical_group_then_substep_leaf(self) -> None:
        responses = [
            combat_decision_response("r0", 0, substep, substep)
            for substep in (0, 1)
        ]
        decisions = [
            Decision(
                response["episode_id"],
                response["step"],
                response["physical_decision_id"],
                response["substep_index"],
                response["substep_count"],
                response["acting_player"],
                response["observation"],
                response["legal_actions"],
                response["provenance"],
                tuple(response["deck_ids"]),
                tuple(response["deck_hashes"]),
            )
            for response in responses
        ]
        sampled_seeds: list[int] = []

        def capture_seed(_logits: torch.Tensor, seed: int) -> int:
            sampled_seeds.append(seed)
            return 0

        with mock.patch.object(rollout_mod, "sample_fixed_categorical", side_effect=capture_seed):
            for decision in decisions:
                self.assertEqual(
                    select_action(
                        decision,
                        policy="sampled",
                        base_seed=71_501,
                        episode=0,
                        model_policy=_BinaryFixedModelPolicy(),  # type: ignore[arg-type]
                    ),
                    0,
                )
        self.assertEqual(sampled_seeds, [826_393_902, 1_701_545_383])
        self.assertEqual(
            [
                select_action(
                    decision,
                    policy="uniform",
                    base_seed=2,
                    episode=0,
                    model_policy=_BinaryFixedModelPolicy(),  # type: ignore[arg-type]
                )
                for decision in decisions
            ],
            [0, 1],
        )

    def test_deck_identity_failures_publish_no_runner_artifacts(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_name:
            tmp = Path(tmp_name)
            for scenario in ("deck_id_drift", "deck_hash_shape", "deck_hash_drift"):
                with self.subTest(scenario=scenario):
                    out = tmp / scenario
                    with self.assertRaises(Exception):
                        run_episodes(
                            env_bin=fake_launcher(tmp, scenario),
                            out_dir=out,
                            episodes=1,
                            base_seed=71_501,
                            max_physical_decisions=8,
                            max_policy_steps=16,
                            p0="uniform",
                            p1="uniform",
                        )
                    self.assertTrue(out.exists())
                    self.assertFalse((out / "run.json").exists())
                    self.assertFalse((out / "episodes.jsonl").exists())

    def test_sampled_selector_has_fixed_goldens_and_preserves_global_rng(self) -> None:
        cases = (
            (0, 0, "p0"),
            (0, 0, "p1"),
            (1, 0, "p0"),
            (0, 1, "p0"),
        )
        random.seed(123_456)
        torch.manual_seed(234_567)
        python_state = random.getstate()
        torch_state = torch.get_rng_state().clone()
        selected: list[int] = []
        for episode, step, actor in cases:
            decision = Decision(
                episode,
                step,
                step,
                0,
                1,
                actor,
                actor_observation(actor, step),
                legal_actions(actor),
                dict(PROVENANCE),
                DECK_IDS,
                DECK_HASHES,
            )
            selected.append(
                select_action(
                    decision,
                    policy="sampled",
                    base_seed=71_501,
                    episode=episode,
                    model_policy=_FixedModelPolicy(),  # type: ignore[arg-type]
                )
            )
        self.assertEqual(selected, [2, 2, 2, 2])
        self.assertEqual(random.getstate(), python_state)
        self.assertTrue(torch.equal(torch.get_rng_state(), torch_state))

    def test_three_lanes_share_equal_nonaliased_categorical_contracts(self) -> None:
        cores = (
            EVALUATOR_ACTION_SELECTION_CONTRACT["categorical_sampler"],
            TRAINER_ACTION_SELECTION_CONTRACT["categorical_sampler"],
            RUNNER_ACTION_SELECTION_CONTRACT["sampled"]["categorical_sampler"],
        )
        expected = fixed_categorical_sampler_contract()
        for core in cores:
            self.assertEqual(core, expected)
        self.assertEqual(len({id(core) for core in cores}), 3)
        self.assertEqual(len({id(core["decimal_softmax"]) for core in cores}), 3)
        self.assertEqual(len({id(core["probability_mass"]) for core in cores}), 3)

        fresh = fixed_categorical_sampler_contract()
        fresh["decimal_softmax"]["exp_precision_digits"] = 1
        fresh["probability_mass"]["total"] = "2**1"
        for core in cores:
            self.assertEqual(core, expected)

    def test_fake_rollout_artifacts_are_byte_deterministic_and_terminal_only(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_name:
            tmp = Path(tmp_name)
            launcher = fake_launcher(tmp, "valid")
            out_a = tmp / "a"
            out_b = tmp / "b"
            run_episodes(env_bin=launcher, out_dir=out_a, episodes=2, base_seed=71501, max_physical_decisions=8, max_policy_steps=16, p0="uniform", p1="uniform")
            run_episodes(env_bin=launcher, out_dir=out_b, episodes=2, base_seed=71501, max_physical_decisions=8, max_policy_steps=16, p0="uniform", p1="uniform")
            self.assertTrue(filecmp.cmp(out_a / "run.json", out_b / "run.json", shallow=False))
            self.assertTrue(filecmp.cmp(out_a / "episodes.jsonl", out_b / "episodes.jsonl", shallow=False))
            run_text = (out_a / "run.json").read_text(encoding="utf-8")
            episodes_text = (out_a / "episodes.jsonl").read_text(encoding="utf-8")
            self.assertNotIn(str(out_a), run_text)
            self.assertNotIn("observation", run_text)
            self.assertNotIn("legal_actions", run_text)
            self.assertIn('"halted":0', run_text)
            self.assertIn('"truncated":0', run_text)
            self.assertIn('"terminal_outcome"', episodes_text)
            manifest_a = json.loads(run_text)
            episode_rows = [json.loads(line) for line in episodes_text.splitlines()]

            def recursive_keys(value: object) -> set[str]:
                if isinstance(value, dict):
                    return set(value) | {
                        key
                        for child in value.values()
                        for key in recursive_keys(child)
                    }
                if isinstance(value, list):
                    return {
                        key
                        for child in value
                        for key in recursive_keys(child)
                    }
                return set()

            for artifact in [manifest_a, *episode_rows]:
                self.assertTrue(
                    {"environment_hash", "environment_hash_algorithm"}.isdisjoint(
                        recursive_keys(artifact)
                    )
                )
            self.assertEqual(tuple(manifest_a["environment"]["deck_ids"]), DECK_IDS)
            self.assertEqual(tuple(manifest_a["environment"]["deck_hashes"]), DECK_HASHES)
            self.assertTrue(all(tuple(row["deck_ids"]) == DECK_IDS for row in episode_rows))
            self.assertTrue(all(tuple(row["deck_hashes"]) == DECK_HASHES for row in episode_rows))
            store, head = _trained_policy_fixture(tmp, launcher, "mixed-policy-store")
            manifest = run_episodes(
                env_bin=launcher,
                out_dir=tmp / "mixed",
                episodes=2,
                base_seed=71_502,
                max_physical_decisions=8,
                max_policy_steps=16,
                p0="uniform",
                p1="greedy",
                training_store=store,
                expected_policy_head=head,
            )
            self.assertEqual(
                manifest["action_selection"],
                {
                    "p0": RUNNER_ACTION_SELECTION_CONTRACT["uniform"],
                    "p1": RUNNER_ACTION_SELECTION_CONTRACT["greedy"],
                },
            )

    def test_sampled_rollout_v5_contract_is_checkpoint_backed_repeatable_and_legacy_versions_remain_distinct(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_name:
            tmp = Path(tmp_name)
            launcher = fake_launcher(tmp, "valid")
            store, head = _trained_policy_fixture(tmp, launcher, "sampled-policy-store")
            out_a = tmp / "sampled-a"
            out_b = tmp / "sampled-b"
            with mock.patch.object(
                rollout_mod.KernelPolicyValueNet,
                "from_encoded",
                side_effect=AssertionError("runner must not construct a fresh model"),
            ):
                manifest_a = run_episodes(
                    env_bin=launcher,
                    out_dir=out_a,
                    episodes=4,
                    base_seed=71_501,
                    max_physical_decisions=8,
                    max_policy_steps=16,
                    p0="sampled",
                    p1="sampled",
                    training_store=store,
                    expected_policy_head=head,
                )
                manifest_b = run_episodes(
                    env_bin=launcher,
                    out_dir=out_b,
                    episodes=4,
                    base_seed=71_501,
                    max_physical_decisions=8,
                    max_policy_steps=16,
                    p0="sampled",
                    p1="sampled",
                    training_store=store,
                    expected_policy_head=head,
                )
            self.assertEqual(manifest_a, manifest_b)
            self.assertTrue(filecmp.cmp(out_a / "run.json", out_b / "run.json", shallow=False))
            self.assertTrue(filecmp.cmp(out_a / "episodes.jsonl", out_b / "episodes.jsonl", shallow=False))
            self.assertEqual(manifest_a["artifact_schema_version"], RUNNER_ARTIFACT_SCHEMA_VERSION)
            self.assertEqual(RUNNER_ARTIFACT_SCHEMA_VERSION, 5)
            self.assertEqual(manifest_a["policy_source"]["mode"], "validated_training_head")
            self.assertEqual(manifest_a["policy_source"]["snapshot"]["head"], head)
            self.assertEqual(manifest_a["policy_source"]["snapshot"]["update"], 1)
            self.assertEqual(validate_runner_artifacts(out_a).policy_head, head)
            self.assertEqual(
                manifest_a["action_selection"],
                {
                    "p0": RUNNER_ACTION_SELECTION_CONTRACT["sampled"],
                    "p1": RUNNER_ACTION_SELECTION_CONTRACT["sampled"],
                },
            )
            self.assertEqual(manifest_a["seed_derivation"], RUNNER_SEED_DERIVATION_CONTRACT)
            self.assertEqual(V1_RUNNER_ARTIFACT_SCHEMA_VERSION, 1)
            self.assertNotEqual(manifest_a["artifact_schema_version"], V1_RUNNER_ARTIFACT_SCHEMA_VERSION)
            self.assertNotEqual(manifest_a["artifact_schema_version"], 2)
            self.assertNotEqual(manifest_a["artifact_schema_version"], 3)
            self.assertNotEqual(manifest_a["artifact_schema_version"], 4)


if __name__ == "__main__":
    unittest.main()
