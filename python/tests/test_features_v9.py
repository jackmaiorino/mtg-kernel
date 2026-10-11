"""Public Raid JSON/tensor checks; engine history authentication is separate."""
from __future__ import annotations

import copy
import unittest

import torch

from fixtures import legal_actions
from mtg_kernel_rl import features_v8 as v8
from mtg_kernel_rl import features_v9 as v9
from test_features import renumber_all_arena_references, seat_swapped
from test_features_v6 import TENSORS
from test_features_v8 import life_decision, observation_v8


def successor(base=None, flags=None):
    obs = copy.deepcopy(observation_v8() if base is None else base)
    obs["schema_version"] = 9
    obs["extensions"] = {"predecessor": obs["extensions"],
                         "attacked_this_turn": [False, False] if flags is None else flags}
    return obs


class FeaturesV9Tests(unittest.TestCase):
    def assert_tensors_equal(self, first, second):
        for name in TENSORS:
            self.assertTrue(torch.equal(getattr(first, name), getattr(second, name)), name)

    def test_flags_change_state_after_combat_without_changing_predecessor_tensors(self):
        obs = successor(flags=[True, False])
        prior = v8.encode_decision(observation_v8(), legal_actions())
        encoded = v9.encode_decision(obs, legal_actions())
        self.assertTrue(torch.equal(encoded.state[:-2], prior.state))
        self.assertEqual(encoded.state[-2:].tolist(), [1.0, 0.0])
        self.assertEqual(encoded.schema.state_dim, prior.schema.state_dim + 2)
        for name in TENSORS[1:]:
            self.assertTrue(torch.equal(getattr(encoded, name), getattr(prior, name)), name)
        obs["extensions"]["attacked_this_turn"] = [False, False]
        absent = v9.encode_decision(obs, legal_actions())
        self.assertFalse(torch.equal(encoded.state, absent.state))
        self.assertTrue(torch.equal(encoded.state[:-2], absent.state[:-2]))

    def test_both_seats_have_identical_actor_relative_encodings(self):
        obs = successor(flags=[True, False])
        actions = legal_actions()
        swapped, swapped_actions = seat_swapped(obs, actions)
        swapped["extensions"]["attacked_this_turn"] = [False, True]
        self.assert_tensors_equal(v9.encode_decision(obs, actions),
                                  v9.encode_decision(swapped, swapped_actions))

    def test_opponent_history_and_arena_renumbering_are_actor_relative(self):
        obs, actions = seat_swapped(observation_v8(), legal_actions())
        # Acting P1 observes the active P0's successful declaration.
        obs["projection"]["active_player"] = "p0"
        obs = successor(obs, [True, False])
        encoded = v9.encode_decision(obs, actions)
        self.assertEqual(encoded.state[-2:].tolist(), [0.0, 1.0])
        self.assert_tensors_equal(encoded, v9.encode_decision(renumber_all_arena_references(obs),
                                                             renumber_all_arena_references(actions)))

    def test_new_and_inherited_unknown_fields_refuse(self):
        obs = successor()
        bad = []
        for flags in (None, [], [False], [False, False, False], [0, False], [True, True],
                      [False, True], "false", (False, False)):
            candidate = copy.deepcopy(obs)
            candidate["extensions"]["attacked_this_turn"] = flags
            bad.append(candidate)
        for target in ("root", "extensions", "predecessor"):
            candidate = copy.deepcopy(obs)
            parent = (candidate if target == "root" else candidate["extensions"] if target == "extensions"
                      else candidate["extensions"]["predecessor"])
            parent["hidden_history"] = [123]
            bad.append(candidate)
        for key in ("attacked_this_turn", "predecessor"):
            candidate = copy.deepcopy(obs)
            del candidate["extensions"][key]
            bad.append(candidate)
        for candidate in bad:
            with self.subTest(candidate=candidate), self.assertRaises(v9.FeatureSchemaError):
                v9.encode_decision(candidate, legal_actions())

    def test_life_ward_validation_remains_required(self):
        base, actions = life_decision()
        encoded = v9.encode_decision(successor(base, [True, False]), actions)
        self.assertEqual(encoded.state[-2:].tolist(), [1.0, 0.0])
        base["projection"]["life_totals"][0] = 6
        with self.assertRaises(v9.FeatureSchemaError):
            v9.encode_decision(successor(base), actions)

    def test_explicit_successor_identities_and_predecessor_refusal(self):
        obs = successor()
        encoded = v9.encode_decision(obs, legal_actions())
        prior = v8.encode_decision(observation_v8(), legal_actions())
        self.assertNotEqual(encoded.schema.version, prior.schema.version)
        self.assertNotEqual(encoded.schema.registry_version, prior.schema.registry_version)
        self.assertNotEqual(encoded.schema.contract_digest, prior.schema.contract_digest)
        self.assertNotEqual(encoded.schema.encoding_digest, prior.schema.encoding_digest)
        self.assertNotEqual(v9.model_contract_fingerprint(encoded.schema), v8.model_contract_fingerprint(prior.schema))
        with self.assertRaises(v8.FeatureSchemaError):
            v8.encode_decision(obs, legal_actions())
        with self.assertRaises(v9.FeatureSchemaError):
            v9.encode_decision(observation_v8(), legal_actions())


if __name__ == "__main__":
    unittest.main()
