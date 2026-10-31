"""FlatV8 (features_v11.py): the V8 inclusion contract on FlatV7.

W4 design (collab LANES/w4-combat-20261030/DESIGN.md v5.1, section 5): every
FlatV8 tensor is the FlatV7 tensor plus the include column (index 195) and
the pairing relation; the descriptor and the Rust identity match the actual
source.
"""
from __future__ import annotations

import copy
import hashlib
import json
from pathlib import Path
import unittest

import torch

from mtg_kernel_rl import features_v10 as v10
from mtg_kernel_rl import features_v11 as v11
from fixtures import legal_actions, stable_ref
from test_features_v10 import TENSORS, announce_actions, in_game, pregame


ROOT = Path(__file__).resolve().parents[2]


def inclusion_actions(kind: str, actor: str = "p0") -> list[dict]:
    attacker = stable_ref(901, 31, "p1", "Battlefield")
    blocker = stable_ref(201, 21, actor, "Battlefield")
    actions = []
    for index, include in enumerate((False, True)):
        semantic = {"action_kind": kind, "actor": actor, "include": include,
                    "attacker": attacker if kind == "choose_blocker_inclusion" else blocker}
        if kind == "choose_blocker_inclusion":
            semantic["blocker"] = blocker
        actions.append({"schema_version": 5, "selected_index": index,
                        "stable_id": f"legal-action-v5:{kind}:{index}", "display_text": None,
                        "semantic": semantic})
    return actions


class FeaturesV11Tests(unittest.TestCase):
    def assert_v7_plus_include(self, old, new, bits):
        for name in TENSORS:
            if name == "action_features":
                continue
            self.assertTrue(torch.equal(getattr(old, name), getattr(new, name)), name)
        self.assertEqual(new.action_features.shape[1], v11.ACTION_FEATURE_DIM)
        self.assertTrue(torch.equal(new.action_features[:, :v11.INCLUDE_COLUMN], old.action_features))
        self.assertEqual(new.action_features[:, v11.INCLUDE_COLUMN].tolist(), bits)

    def test_in_game_rows_are_v7_rows_with_a_zero_include_column(self):
        obs, actions = in_game(), legal_actions()
        old = v10.encode_decision(copy.deepcopy(obs), copy.deepcopy(actions))
        new = v11.encode_decision(copy.deepcopy(obs), copy.deepcopy(actions))
        self.assert_v7_plus_include(old, new, [0.0] * len(actions))
        self.assertEqual(tuple(new.pairing_node_indices.shape), (0, 2))
        self.assertEqual(new.schema.action_feature_dim, 196)
        self.assertEqual(new.schema.contract_digest, v11.feature_contract_fingerprint())
        self.assertEqual(new.schema.encoding_digest, v11.encoding_contract_fingerprint())
        self.assertNotEqual(new.schema.contract_digest, old.schema.contract_digest)

    def test_pregame_rows_are_v7_rows_with_a_zero_include_column(self):
        obs = pregame("p0")
        actions = announce_actions("p0", 0)
        old = v10.encode_decision(copy.deepcopy(obs), copy.deepcopy(actions))
        new = v11.encode_decision(copy.deepcopy(obs), copy.deepcopy(actions))
        self.assert_v7_plus_include(old, new, [0.0, 0.0])
        self.assertEqual(tuple(new.pairing_node_indices.shape), (0, 2))

    def test_include_bit_marks_only_the_include_row_of_an_inclusion_decision(self):
        for kind in v11.INCLUSION_ACTION_KINDS:
            self.assertEqual(v11.include_bits(inclusion_actions(kind)), [0.0, 1.0])
        self.assertEqual(v11.include_bits(legal_actions()), [0.0] * len(legal_actions()))
        # An include member on another kind is not an inclusion row.
        other = copy.deepcopy(legal_actions())
        other[0]["semantic"]["include"] = True
        self.assertEqual(v11.include_bits(other)[0], 0.0)

    def test_pending_blocks_are_accumulated_pairs_then_the_current_attackers_selection(self):
        blocker_a = stable_ref(201, 21, "p0", "Battlefield")
        blocker_b = stable_ref(202, 22, "p0", "Battlefield")
        blocker_c = stable_ref(203, 23, "p0", "Battlefield")
        first_attacker = stable_ref(901, 31, "p1", "Battlefield")
        current_attacker = stable_ref(902, 32, "p1", "Battlefield")
        obs = {"acting_player": "p0", "projection": {
            "surface_context": {"private_blockers": {"current_attacker": current_attacker,
                                                     "accumulated": [[blocker_a, first_attacker]],
                                                     "remaining": []}},
            "policy_surface_context": {"current_stage": "blocker_inclusion", "private_combat_selection": {
                "attacker": current_attacker, "candidate_index": 1, "candidate_count": 2,
                "selected": [blocker_b], "current_candidate": blocker_c, "remaining_after_current": []}}}}
        self.assertEqual(v11.pending_block_refs(obs),
                         [(blocker_a, first_attacker), (blocker_b, current_attacker)])
        # Attacker inclusion selects attackers, not blocks.
        attack = copy.deepcopy(obs)
        attack["projection"]["surface_context"]["private_blockers"] = None
        attack["projection"]["policy_surface_context"]["private_combat_selection"]["attacker"] = None
        self.assertEqual(v11.pending_block_refs(attack), [])
        # A pair must be an own blocker with an opposing attacker.
        reversed_pair = copy.deepcopy(obs)
        reversed_pair["projection"]["surface_context"]["private_blockers"]["accumulated"] = [
            [first_attacker, blocker_a]]
        with self.assertRaises(v11.FeatureSchemaError):
            v11.pending_block_refs(reversed_pair)

    def test_rust_blocker_parity_fixture(self):
        """Rust FlatV8 tensors (``flat_v8_tensors_include_bits_and_pairs_match_the_sessions``)
        equal this encoder's on the same actor-visible blocker declarations with pending blocks."""
        fixture = json.loads((ROOT / "data/flat_policy_v8/blocker_parity_fixture_v1.json").read_text())
        self.assertEqual(fixture["feature_contract_digest"], v11.feature_contract_fingerprint())
        self.assertGreater(len(fixture["cases"]), 0)
        for index, case in enumerate(fixture["cases"]):
            encoded = v11.encode_decision(case["observation"], case["legal_actions"])
            self.assertGreater(encoded.pairing_node_indices.shape[0], 0)
            for name in (*TENSORS, "pairing_node_indices"):
                expected = case["tensor"][name]
                actual = getattr(encoded, name)
                if actual.dtype == torch.float32:
                    expected_tensor = torch.tensor(expected, dtype=torch.float32)
                    self.assertTrue(torch.equal(actual.reshape(-1), expected_tensor), f"{name} {index}")
                else:
                    self.assertEqual(actual.reshape(-1).tolist(), expected, f"{name} {index}")

    def test_descriptor_and_rust_identity_match_the_source(self):
        directory = ROOT / "data" / "flat_policy_v8"
        descriptor_bytes = (directory / "feature_contract_v8.json").read_bytes()
        descriptor = json.loads(descriptor_bytes)
        source_hash = hashlib.sha256(Path(v11.__file__).read_bytes()).hexdigest()
        self.assertEqual(descriptor["features_source_sha256"], source_hash)
        self.assertEqual(descriptor["feature_contract_digest"], v11.feature_contract_fingerprint())
        self.assertEqual(descriptor["feature_encoding_digest"], v11.encoding_contract_fingerprint())
        self.assertEqual(descriptor["dimensions"]["action"], v11.ACTION_FEATURE_DIM)
        identity = (directory / "feature_identity.rs").read_text()
        for name, value in {
            "FEATURE_CONTRACT_DIGEST_V8": v11.feature_contract_fingerprint(),
            "FEATURE_ENCODING_DIGEST_V8": v11.encoding_contract_fingerprint(),
            "FEATURES_SOURCE_SHA256_V8": source_hash,
            "FEATURE_DESCRIPTOR_SHA256_V8": hashlib.sha256(descriptor_bytes).hexdigest(),
            "FEATURE_SCHEMA_VERSION_V8": v11.FEATURE_SCHEMA_VERSION,
            "FEATURE_REGISTRY_VERSION_V8": v11.FEATURE_REGISTRY_VERSION,
        }.items():
            self.assertIn(f'pub(crate) const {name}: &str = "{value}";', identity)
        # Never the identity of the FlatV7 generation it extends.
        self.assertNotEqual(v11.feature_contract_fingerprint(), v10.feature_contract_fingerprint())
        self.assertNotEqual(v11.encoding_contract_fingerprint(), v10.encoding_contract_fingerprint())


if __name__ == "__main__":
    unittest.main()
