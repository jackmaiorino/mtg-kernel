"""Frozen feature authorities reject Standard facts without changing their bytes."""
from __future__ import annotations

import copy
import unittest

from mtg_kernel_rl import features, features_v6, features_v7
from fixtures import observation


class StandardFeatureRefusalV1Tests(unittest.TestCase):
    def test_frozen_encoders_refuse_standard_projection_fields(self) -> None:
        for authority in (features, features_v6, features_v7):
            original = observation()
            if authority is not features:
                original["schema_version"] = 6
                original["extensions"] = {
                    "pending_cast_object_cost": None,
                    "decision_local_library": None,
                    "historical_public_sources": [],
                    "pending_chosen_creature_cost": None,
                    "finalized_chosen_creature_costs": [],
                }
            authority.assert_observation_classified(original)
            for field, value in (
                ("poison_counters", [0, 1]),
                ("poison_prevention", [None, {"prevent_remaining": True}]),
                ("restricted_mana", [[], [{"amount": 1}]]),
                ("creatures_attacked_this_turn", [1, 0]),
                ("ninja_emblems", [1, 0]),
            ):
                with self.subTest(authority=authority.__name__, field=field):
                    extended = copy.deepcopy(original)
                    extended["projection"][field] = value
                    with self.assertRaisesRegex(authority.FeatureSchemaError, field):
                        authority.assert_observation_classified(extended)
                    with self.assertRaisesRegex(authority.FeatureSchemaError, field):
                        authority.encode_decision(extended, [])


if __name__ == "__main__":
    unittest.main()
