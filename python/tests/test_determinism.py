from __future__ import annotations

import random
import unittest

import torch

from mtg_kernel_rl.determinism import (
    EVALUATOR_ACTION_SEED_DERIVATION_VERSION,
    EVALUATOR_SEED_DERIVATION_VERSION,
    TRAINER_SEED_DERIVATION_VERSION,
    EvaluatorSeedDerivation,
    TrainerSeedDerivation,
    configure_torch_determinism,
    derive_evaluation_action_seed,
    derive_evaluation_bootstrap_seed,
    derive_evaluation_env_seed,
    derive_model_init_seed,
    derive_train_env_seed,
    derive_train_learner_action_seed,
    derive_train_opponent_action_seed,
)
from mtg_kernel_rl.features import encode_decision
from mtg_kernel_rl.model import INITIALIZER_RUNNER_FIXED_V1, INITIALIZER_TRAINER_SEEDED_V1, KernelPolicyValueNet

from fixtures import legal_actions, observation


class TrainerDeterminismTest(unittest.TestCase):
    def test_sha256_evaluator_action_seed_goldens_and_separation(self) -> None:
        self.assertEqual(
            EVALUATOR_ACTION_SEED_DERIVATION_VERSION,
            "kernel-python-rl-evaluator-action-sha256-v2",
        )
        vectors = (
            ((71_501, 0, "p0", 0, 0), 0x25F1_DB21_FAA0_5C9C),
            ((71_501, 0, "p1", 0, 0), 0x66D1_90AE_9083_6DF9),
            ((71_501, 0, "p0", 1, 0), 0x2D8A_C6AB_C582_27C9),
            ((71_501, 0, "p0", 0, 1), 0x7549_9CDD_5D95_E8BB),
            ((71_501, 1, "p0", 0, 0), 0x708D_F9E8_8868_9E0B),
            ((0, 0, "p0", 0, 0), 0x2824_03F5_A3AC_1C4D),
            (((1 << 63) - 1, (1 << 63) - 1, "p1", (1 << 63) - 1, (1 << 32) - 1), 0x1B7F_46B3_EB34_F726),
        )
        for args, expected in vectors:
            with self.subTest(args=args):
                self.assertEqual(derive_evaluation_action_seed(*args), expected)
        self.assertEqual(len({derive_evaluation_action_seed(*args) for args, _expected in vectors}), len(vectors))
        self.assertNotEqual(
            derive_evaluation_action_seed(71_501, 0, "p0", 0, 0),
            derive_evaluation_env_seed(71_501, 0),
        )
        for bad in (True, -1, 2**63):
            with self.subTest(base_seed=bad), self.assertRaises((TypeError, ValueError)):
                derive_evaluation_action_seed(bad, 0, "p0", 0, 0)  # type: ignore[arg-type]
            with self.subTest(pair_index=bad), self.assertRaises((TypeError, ValueError)):
                derive_evaluation_action_seed(0, bad, "p0", 0, 0)  # type: ignore[arg-type]
            with self.subTest(local_decision_index=bad), self.assertRaises((TypeError, ValueError)):
                derive_evaluation_action_seed(0, 0, "p0", bad, 0)  # type: ignore[arg-type]
            with self.subTest(substep_index=bad), self.assertRaises((TypeError, ValueError)):
                derive_evaluation_action_seed(0, 0, "p0", 0, bad)  # type: ignore[arg-type]
        for bad_seat in ("P0", "candidate", "", 0, True, None):
            with self.subTest(physical_seat=bad_seat), self.assertRaises((TypeError, ValueError)):
                derive_evaluation_action_seed(0, 0, bad_seat, 0, 0)  # type: ignore[arg-type]
        with self.assertRaises(ValueError):
            derive_evaluation_action_seed(0, 0, "p0", 0, 2**32)

    def test_sha256_evaluator_seed_known_vectors_and_frozen_contract(self) -> None:
        self.assertEqual(EVALUATOR_SEED_DERIVATION_VERSION, "kernel-python-rl-evaluator-sha256-v2")
        self.assertEqual(derive_evaluation_bootstrap_seed(71501), 0x42AE_257C_63EA_6815)
        self.assertEqual(derive_evaluation_env_seed(71501, 0), 0x0E60_BEC6_8DE0_ED27)
        self.assertEqual(derive_evaluation_env_seed(71501, 1), 0x5A6F_945C_7A2D_532E)
        self.assertEqual(derive_evaluation_env_seed(71501, 32), 0x6A30_D51A_55C8_59A5)
        self.assertEqual(
            EvaluatorSeedDerivation().namespaces,
            (
                "evaluation-bootstrap/base_seed",
                "evaluation-env/base_seed/pair_index",
            ),
        )
        self.assertEqual(TRAINER_SEED_DERIVATION_VERSION, "kernel-python-rl-trainer-sha256-v2")
        self.assertEqual(
            TrainerSeedDerivation().namespaces,
            (
                "model-init/base_seed",
                "train-env/base_seed/pair_index",
                "train-learner-action-group/base_seed/episode_index/learner_physical_decision_index -> train-learner-action-substep/group_seed/substep_index",
                "train-opponent-action-group/base_seed/episode_index/opponent_physical_decision_index -> train-opponent-action-substep/group_seed/substep_index",
            ),
        )
        self.assertNotEqual(derive_evaluation_env_seed(71501, 0), derive_train_env_seed(71501, 0))
        for bad in (True, -1, 2**63):
            with self.subTest(bad=bad), self.assertRaises((TypeError, ValueError)):
                derive_evaluation_bootstrap_seed(bad)  # type: ignore[arg-type]
            with self.subTest(pair_index=bad), self.assertRaises((TypeError, ValueError)):
                derive_evaluation_env_seed(71501, bad)  # type: ignore[arg-type]

    def test_sha256_trainer_seed_known_vectors_and_separation(self) -> None:
        self.assertEqual(derive_model_init_seed(71501), 4755442154187158375)
        self.assertEqual(derive_train_env_seed(71501, 0), 5293664275683392565)
        self.assertEqual(derive_train_env_seed(71501, 1), 7386941714276895345)
        self.assertEqual(derive_train_learner_action_seed(71501, 2, 3, 0), 730580081334587889)
        self.assertEqual(derive_train_learner_action_seed(71501, 2, 3, 1), 6181545364031931902)
        self.assertEqual(derive_train_opponent_action_seed(71501, 2, 3, 0), 871712426949739466)
        self.assertEqual(derive_train_env_seed(71501, 0), derive_train_env_seed(71501, 0))
        self.assertNotEqual(derive_train_env_seed(71501, 0), derive_train_env_seed(71501, 1))
        self.assertNotEqual(derive_train_env_seed(71501, 3), derive_train_learner_action_seed(71501, 3, 0, 0))
        self.assertNotEqual(derive_train_env_seed(1, 256), derive_train_env_seed(256, 1))
        for bad in (True, -1, 2**63):
            with self.subTest(bad=bad), self.assertRaises((TypeError, ValueError)):
                derive_model_init_seed(bad)  # type: ignore[arg-type]
        for derive in (derive_train_learner_action_seed, derive_train_opponent_action_seed):
            with self.subTest(derive=derive.__name__), self.assertRaises(ValueError):
                derive(0, 0, 0, 2**32)

    def test_paired_env_seeds_and_actor_local_action_counters(self) -> None:
        seeds = [derive_train_env_seed(71501, episode // 2) for episode in range(6)]
        self.assertEqual(seeds[0], seeds[1])
        self.assertEqual(seeds[2], seeds[3])
        self.assertEqual(seeds[4], seeds[5])
        self.assertNotEqual(seeds[1], seeds[2])
        self.assertEqual(
            derive_train_learner_action_seed(71501, 4, 0, 0),
            derive_train_learner_action_seed(71501, 4, 0, 0),
        )
        self.assertNotEqual(
            derive_train_learner_action_seed(71501, 4, 0, 0),
            derive_train_learner_action_seed(71501, 4, 0, 1),
        )

    def test_seeded_model_init_repeats_differs_and_ignores_global_rng(self) -> None:
        configure_torch_determinism()
        encoded = encode_decision(observation(), legal_actions())
        config = KernelPolicyValueNet.from_encoded(encoded).config
        seed = derive_model_init_seed(71501)
        torch.manual_seed(999)
        random.seed(999)
        model_a = KernelPolicyValueNet(config, initializer=INITIALIZER_TRAINER_SEEDED_V1, initializer_seed=seed)
        torch.rand(17)
        random.random()
        model_b = KernelPolicyValueNet(config, initializer=INITIALIZER_TRAINER_SEEDED_V1, initializer_seed=seed)
        model_c = KernelPolicyValueNet(config, initializer=INITIALIZER_TRAINER_SEEDED_V1, initializer_seed=seed + 1)
        for key in model_a.state_dict():
            self.assertTrue(torch.equal(model_a.state_dict()[key], model_b.state_dict()[key]))
        self.assertTrue(any(not torch.equal(model_a.state_dict()[key], model_c.state_dict()[key]) for key in model_a.state_dict()))
        fixed_a = KernelPolicyValueNet.from_encoded(encoded)
        fixed_b = KernelPolicyValueNet(config, initializer=INITIALIZER_RUNNER_FIXED_V1)
        for key in fixed_a.state_dict():
            self.assertTrue(torch.equal(fixed_a.state_dict()[key], fixed_b.state_dict()[key]))


if __name__ == "__main__":
    unittest.main()
