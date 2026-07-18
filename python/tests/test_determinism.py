from __future__ import annotations

import hashlib
import json
import random
import unittest
from pathlib import Path

import torch

from mtg_kernel_rl.determinism import (
    EVALUATOR_ACTION_SEED_DERIVATION_VERSION,
    EVALUATOR_SEED_DERIVATION_VERSION,
    TRAINER_SEED_DERIVATION_VERSION,
    EvaluatorSeedDerivation,
    TrainerSeedDerivation,
    _trainer_seed,
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
    def test_native_trainer_schedule_cross_language_goldens(self) -> None:
        path = Path(__file__).resolve().parents[2] / "data" / "native_trainer_schedule_v1_goldens.json"
        raw = path.read_bytes()
        self.assertEqual(
            hashlib.sha256(raw).hexdigest(),
            "6b2e1edbbe49b4e02f98794f9057f5c2bb8e3079d2ba8cb3e2a4b9ea6c34867c",
        )
        payload = json.loads(raw)
        self.assertEqual(payload["schema"], "mtg_kernel_native_trainer_schedule_goldens/v1")
        self.assertEqual(
            payload["schedule_version"],
            "mtg-kernel-native-trainer-schedule-sha256-v1",
        )
        self.assertEqual(payload["python_reference_seed_version"], TRAINER_SEED_DERIVATION_VERSION)
        str_probe = payload["str_atom_probe"]
        self.assertEqual(
            _trainer_seed(
                str_probe["namespace"],
                [(str_probe["field_name"], str_probe["field_value"])],
            ),
            str_probe["seed"],
        )
        self.assertTrue({0, 1, 71_501, (1 << 63) - 1}.issubset({row["base_seed"] for row in payload["vectors"]}))
        self.assertTrue(set(range(4)).issubset({row["episode_index"] for row in payload["vectors"]}))
        self.assertTrue(any(row["episode_index"] == 1 << 62 for row in payload["vectors"]))
        self.assertTrue(
            any(
                row["base_seed"] == (1 << 63) - 1
                and row["episode_index"] == (1 << 63) - 1
                and row["learner_physical_decision_index"] == (1 << 63) - 1
                and row["opponent_physical_decision_index"] == (1 << 63) - 1
                and row["substep_index"] == (1 << 32) - 1
                for row in payload["vectors"]
            )
        )
        self.assertTrue(
            any(
                row["learner_physical_decision_index"]
                != row["opponent_physical_decision_index"]
                for row in payload["vectors"]
            )
        )
        for vector in payload["vectors"]:
            with self.subTest(vector=vector):
                episode = vector["episode_index"]
                expected_seat = "p0" if episode % 2 == 0 else "p1"
                self.assertIn(vector["learner_seat"], ("p0", "p1"))
                self.assertEqual(vector["learner_seat"], expected_seat)
                self.assertEqual(vector["pair_index"], episode // 2)
                self.assertEqual(derive_model_init_seed(vector["base_seed"]), vector["model_init_seed"])
                self.assertEqual(
                    derive_train_env_seed(vector["base_seed"], vector["pair_index"]),
                    vector["environment_seed"],
                )
                self.assertEqual(
                    _trainer_seed(
                        "train-learner-action-group",
                        [
                            ("base_seed", vector["base_seed"]),
                            ("episode_index", episode),
                            (
                                "learner_physical_decision_index",
                                vector["learner_physical_decision_index"],
                            ),
                        ],
                    ),
                    vector["learner_group_seed"],
                )
                self.assertEqual(
                    derive_train_learner_action_seed(
                        vector["base_seed"],
                        episode,
                        vector["learner_physical_decision_index"],
                        vector["substep_index"],
                    ),
                    vector["learner_action_seed"],
                )
                self.assertEqual(
                    _trainer_seed(
                        "train-opponent-action-group",
                        [
                            ("base_seed", vector["base_seed"]),
                            ("episode_index", episode),
                            (
                                "opponent_physical_decision_index",
                                vector["opponent_physical_decision_index"],
                            ),
                        ],
                    ),
                    vector["opponent_group_seed"],
                )
                self.assertEqual(
                    derive_train_opponent_action_seed(
                        vector["base_seed"],
                        episode,
                        vector["opponent_physical_decision_index"],
                        vector["substep_index"],
                    ),
                    vector["opponent_action_seed"],
                )
                if (
                    vector["learner_physical_decision_index"]
                    == vector["opponent_physical_decision_index"]
                ):
                    self.assertNotEqual(vector["learner_group_seed"], vector["opponent_group_seed"])
                    self.assertNotEqual(vector["learner_action_seed"], vector["opponent_action_seed"])

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
        self.assertNotEqual(
            derive_train_learner_action_seed(71501, 4, 7, 0),
            derive_train_opponent_action_seed(71501, 4, 7, 0),
        )
        later_learner = _trainer_seed(
            "train-learner-action-group",
            [
                ("base_seed", 71501),
                ("episode_index", 4),
                ("learner_physical_decision_index", 7),
            ],
        )
        later_opponent = _trainer_seed(
            "train-opponent-action-group",
            [
                ("base_seed", 71501),
                ("episode_index", 4),
                ("opponent_physical_decision_index", 7),
            ],
        )
        for prior_width in (1, 2, 3, 17, (1 << 32) - 1):
            derive_train_learner_action_seed(71501, 4, 6, prior_width - 1)
            derive_train_opponent_action_seed(71501, 4, 6, prior_width - 1)
            self.assertEqual(
                _trainer_seed(
                    "train-learner-action-group",
                    [
                        ("base_seed", 71501),
                        ("episode_index", 4),
                        ("learner_physical_decision_index", 7),
                    ],
                ),
                later_learner,
            )
            self.assertEqual(
                _trainer_seed(
                    "train-opponent-action-group",
                    [
                        ("base_seed", 71501),
                        ("episode_index", 4),
                        ("opponent_physical_decision_index", 7),
                    ],
                ),
                later_opponent,
            )

    def test_trainer_schedule_raw_domains_fail_closed(self) -> None:
        invalid_u63 = (True, -1, 1 << 63)
        for bad in invalid_u63:
            with self.subTest(base_seed=bad), self.assertRaises((TypeError, ValueError)):
                derive_train_env_seed(bad, 0)  # type: ignore[arg-type]
            with self.subTest(pair_index=bad), self.assertRaises((TypeError, ValueError)):
                derive_train_env_seed(0, bad)  # type: ignore[arg-type]
            for derive in (derive_train_learner_action_seed, derive_train_opponent_action_seed):
                with self.subTest(derive=derive.__name__, base_seed=bad), self.assertRaises(
                    (TypeError, ValueError)
                ):
                    derive(bad, 0, 0, 0)  # type: ignore[arg-type]
                with self.subTest(derive=derive.__name__, episode_index=bad), self.assertRaises(
                    (TypeError, ValueError)
                ):
                    derive(0, bad, 0, 0)  # type: ignore[arg-type]
                with self.subTest(derive=derive.__name__, group_index=bad), self.assertRaises(
                    (TypeError, ValueError)
                ):
                    derive(0, 0, bad, 0)  # type: ignore[arg-type]
        for bad in (True, -1, 1 << 32):
            for derive in (derive_train_learner_action_seed, derive_train_opponent_action_seed):
                with self.subTest(derive=derive.__name__, substep_index=bad), self.assertRaises(
                    (TypeError, ValueError)
                ):
                    derive(0, 0, 0, bad)  # type: ignore[arg-type]

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
