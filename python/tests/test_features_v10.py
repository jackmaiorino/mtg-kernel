"""FlatV7 (features_v10.py): London pregame rows on the FlatV4 contract.

Pauper London mulligans v1 (docs/design/pauper_london_mulligans_v1.md,
section 5): in-game decisions encode exactly as V4; pregame decisions set
the reused columns; numeric choices are refused; the descriptor and the Rust
identity match the actual source.
"""
from __future__ import annotations

import copy
import hashlib
import json
from pathlib import Path
import unittest

import torch

from mtg_kernel_rl import features_v7 as v7
from mtg_kernel_rl import features_v10 as v10
from fixtures import legal_actions, observation, stable_ref


TENSORS = ("state", "object_features", "object_card_ids", "object_groups", "object_node_ids",
           "edge_features", "edge_source_indices", "edge_target_indices", "action_features",
           "action_ref_features", "action_ref_card_ids", "action_ref_action_indices", "action_ref_node_indices")


def in_game() -> dict:
    obs = observation()
    obs["schema_version"] = 6
    obs["extensions"] = {"pending_cast_object_cost": None, "decision_local_library": None,
                         "historical_public_sources": [], "pending_chosen_creature_cost": None,
                         "finalized_chosen_creature_costs": []}
    return obs


def hand(owner: str, first_arena: int) -> list[dict]:
    return [{"stable": stable_ref(first_arena + i, 30 + i, owner, "Hand"), "card_name": f"Card {i}"}
            for i in range(7)]


def pregame(actor: str = "p0", *, phase: str = "announce", counts=(0, 0), kept=(False, False),
            announced=(False, False), hand_size: int = 7) -> dict:
    obs = in_game()
    obs["acting_player"] = actor
    p = obs["projection"]
    p.update({"turn": 1, "phase": "untap", "active_player": "p0", "priority_player": "p0",
              "life_totals": [20, 20], "mana_pools": [[0] * 6, [0] * 6], "hand_counts": [7, 7],
              "library_counts": [53, 53], "battlefield": [[], []], "graveyards": [[], []], "exile": [],
              "stack": [], "continuous_effects": [], "object_relations": [], "exile_play_permissions": []})
    p["combat"] = {"attackers_declared": False, "blockers_declared": False, "ordered_attackers": [],
                   "attacker_to_ordered_blockers": []}
    for status in p["player_status"]:
        status.update({"lands_played_this_turn": 0, "spells_cast_this_turn": 0,
                       "draws_this_turn": 7 * (1 + max(counts))})
    p["engine_context"].update({"stack_nonempty": False, "stack_activity_since_priority_boundary": False})
    p["surface_context"].update({"stack_grew_since_round_open": False})
    me = 0 if actor == "p0" else 1
    p["hand_counts"][me] = hand_size
    obs["own_hand"] = hand(actor, 200 if actor == "p0" else 300)[:hand_size]
    obs["extensions"]["london_pregame"] = {"phase": phase, "mulligans_taken": list(counts), "kept": list(kept),
                                           "announced_mulligan": list(announced)}
    p["london_mulligans"] = {"phase": phase, "counts": list(counts), "kept": list(kept)}
    return obs


def announce_actions(actor: str, count: int) -> list[dict]:
    return [{"schema_version": 5, "selected_index": i, "stable_id": f"legal-action-v5:london-{i}",
             "display_text": None,
             "semantic": {"action_kind": "choose_london_mulligan", "actor": actor, "mulligan_count": count,
                          "mulligan": mulligan}}
            for i, mulligan in enumerate((False, True))]


def bottom_actions(obs: dict, remaining: int) -> list[dict]:
    return [{"schema_version": 5, "selected_index": i, "stable_id": f"legal-action-v5:bottom-{i}",
             "display_text": None,
             "semantic": {"action_kind": "choose_london_bottom", "actor": obs["acting_player"],
                          "remaining": remaining, "card": card["stable"]}}
            for i, card in enumerate(obs["own_hand"])]


class FeaturesV10Tests(unittest.TestCase):
    def assert_tensors_equal(self, left, right):
        for name in TENSORS:
            self.assertTrue(torch.equal(getattr(left, name), getattr(right, name)), name)

    def test_in_game_tensors_equal_v4(self):
        obs, actions = in_game(), legal_actions()
        old = v7.encode_decision(copy.deepcopy(obs), copy.deepcopy(actions))
        new = v10.encode_decision(copy.deepcopy(obs), copy.deepcopy(actions))
        self.assert_tensors_equal(old, new)
        self.assertEqual(new.schema.contract_digest, v10.feature_contract_fingerprint())
        self.assertNotEqual(new.schema.contract_digest, old.schema.contract_digest)
        self.assertEqual(new.state.shape, (v7.STATE_FEATURE_DIM,))

    def test_loss_bearing_in_game_row_keeps_has_lost(self):
        obs = in_game()
        obs["projection"]["player_status"][1]["has_lost"] = True
        old = v7.encode_decision(copy.deepcopy(obs), legal_actions())
        new = v10.encode_decision(copy.deepcopy(obs), legal_actions())
        self.assert_tensors_equal(old, new)
        self.assertEqual(new.state[v10.OPPONENT_HAS_LOST_COLUMN].item(), 1.0)
        self.assertEqual(new.state[v10.LONDON_PREGAME_STAGE_COLUMN].item(), 0.0)

    def test_numeric_choice_is_refused(self):
        obs = in_game()
        source = stable_ref(1, 30, "p0", "Hand")
        actions = [{"schema_version": 5, "selected_index": 0, "stable_id": "legal-action-v5:n",
                    "display_text": None,
                    "semantic": {"action_kind": "choose_effect_number", "actor": "p0", "source": source,
                                 "number": 1, "minimum": 0, "maximum": 2}}]
        with self.assertRaisesRegex(v10.FeatureSchemaError, "numeric choice"):
            v10.encode_decision(obs, actions)

    def test_column_layout_matches_the_v4_names(self):
        self.assertEqual(v10.ACTION_KINDS[15], "london_opening")
        self.assertEqual(v7.ACTION_KINDS[15], "choose_effect_number")
        self.assertEqual(v10.ENGINE_STAGES[9], "london_pregame")
        self.assertEqual((v10.SELF_HAS_LOST_COLUMN, v10.OPPONENT_HAS_LOST_COLUMN,
                          v10.LONDON_PREGAME_STAGE_COLUMN), (30, 49, 89))
        self.assertEqual((v10.REMAINING_COLUMN, v10.OPPONENT_KEPT_COLUMN, v10.OPPONENT_ANNOUNCED_COLUMN,
                          v10.OWN_MULLIGANS_COLUMN, v10.OPPONENT_MULLIGANS_COLUMN, v10.MULLIGAN_VALUE_COLUMN),
                         (49, 52, 53, 63, 66, 69))
        # The V4 state columns really are has_lost and halted.
        obs = in_game()
        obs["projection"]["player_status"][0]["has_lost"] = True
        obs["projection"]["engine_context"]["current_stage"] = "halted"
        state = v7.encode_decision(obs, legal_actions()).state
        self.assertEqual(state[v10.SELF_HAS_LOST_COLUMN].item(), 1.0)
        self.assertEqual(state[v10.LONDON_PREGAME_STAGE_COLUMN].item(), 1.0)

    def test_announcement_rows_and_state_for_both_seats(self):
        for actor in ("p0", "p1"):
            me = 0 if actor == "p0" else 1
            counts = [0, 0]
            counts[me] = 2
            counts[1 - me] = 1
            obs = pregame(actor, counts=tuple(counts))
            encoded = v10.encode_decision(obs, announce_actions(actor, 2))
            self.assertEqual(encoded.state[v10.LONDON_PREGAME_STAGE_COLUMN].item(), 1.0)
            self.assertEqual(encoded.action_features.shape, (2, v7.ACTION_FEATURE_DIM))
            for row, mulligan in zip(encoded.action_features, (0.0, 1.0)):
                self.assertEqual(row[15].item(), 1.0)
                self.assertEqual(sum(row[:27].tolist()), 1.0)
                self.assertEqual(row[v10.MULLIGAN_VALUE_COLUMN].item(), mulligan)
                self.assertAlmostEqual(row[v10.OWN_MULLIGANS_COLUMN].item(), 2 / 16)
                self.assertAlmostEqual(row[v10.OPPONENT_MULLIGANS_COLUMN].item(), 1 / 16)
            self.assertEqual(encoded.action_ref_features.shape[0], 0)
            # Only the actor's own hand objects are encoded.
            owners = {obj["stable"]["owner"] for obj in obs["own_hand"]}
            self.assertEqual(owners, {actor})
            self.assertEqual(encoded.object_features.shape[0], 7)

    def test_counterfactual_first_announcement_changes_explicit_columns(self):
        # The second announcer (p1, with p0 starting) sees p0's announcement.
        keep = v10.encode_decision(pregame("p1", kept=(True, False)), announce_actions("p1", 0))
        mull = v10.encode_decision(pregame("p1", announced=(True, False)), announce_actions("p1", 0))
        self.assertEqual((keep.state[30].item(), keep.state[49].item()), (0.0, 1.0))
        self.assertEqual((mull.state[30].item(), mull.state[49].item()), (1.0, 0.0))
        self.assertEqual((keep.action_features[0][52].item(), keep.action_features[0][53].item()), (1 / 16, 0.0))
        self.assertEqual((mull.action_features[0][52].item(), mull.action_features[0][53].item()), (0.0, 1 / 16))
        explicit = slice(0, v7.STATE_FEATURE_DIM - v7.STATE_HASH_DIM)
        self.assertFalse(torch.equal(keep.state[explicit], mull.state[explicit]))

    def test_bottom_rows_reference_exact_hand_objects(self):
        obs = pregame("p1", phase="bottom", counts=(0, 2), announced=(False, True))
        encoded = v10.encode_decision(obs, bottom_actions(obs, 2))
        self.assertEqual(encoded.action_features.shape[0], 7)
        self.assertEqual(encoded.action_ref_features.shape[0], 7)
        self.assertEqual(encoded.action_ref_action_indices.tolist(), list(range(7)))
        for row in encoded.action_features:
            self.assertEqual(row[15].item(), 1.0)
            self.assertAlmostEqual(row[v10.REMAINING_COLUMN].item(), 2 / 8)
            self.assertAlmostEqual(row[v10.OWN_MULLIGANS_COLUMN].item(), 2 / 16)
            self.assertEqual(row[v10.MULLIGAN_VALUE_COLUMN].item(), 0.0)
        # Each reference is the card role to a distinct own hand node.
        self.assertEqual(len(set(encoded.action_ref_node_indices.tolist())), 7)
        role = v7.ACTION_REF_ROLES.index("card")
        self.assertTrue(all(row[role].item() == 1.0 for row in encoded.action_ref_features))
        # After one bottom, one card is still to bottom.
        obs = pregame("p1", phase="bottom", counts=(0, 2), announced=(False, True), hand_size=6)
        encoded = v10.encode_decision(obs, bottom_actions(obs, 1))
        self.assertAlmostEqual(encoded.action_features[0][v10.REMAINING_COLUMN].item(), 1 / 8)

    def test_pregame_state_hash_differs_and_is_seat_relative(self):
        a = v10.encode_decision(pregame("p0", counts=(1, 0)), announce_actions("p0", 1))
        b = v10.encode_decision(pregame("p0", counts=(0, 1)), announce_actions("p0", 0))
        hashed = slice(v7.STATE_FEATURE_DIM - v7.STATE_HASH_DIM, v7.STATE_FEATURE_DIM)
        self.assertFalse(torch.equal(a.state[hashed], b.state[hashed]))
        canonical = v10.canonical_observation_v10(pregame("p1", counts=(1, 0)))
        self.assertEqual(canonical["extensions"]["london_pregame"]["mulligans_taken"], [0, 1])

    def test_inconsistent_pregame_inputs_are_rejected(self):
        bad = []
        obs = pregame("p0")
        bad.append((obs, announce_actions("p0", 1)))  # count differs
        obs = pregame("p0")
        bad.append((obs, announce_actions("p0", 0)[:1]))  # keep only
        obs = pregame("p0", kept=(True, False))
        bad.append((obs, announce_actions("p0", 0)))  # actor already kept
        obs = pregame("p0")
        obs["projection"]["london_mulligans"]["kept"] = [True, False]
        bad.append((obs, announce_actions("p0", 0)))  # projection differs
        obs = pregame("p0", phase="bottom", counts=(1, 0), announced=(True, False))
        foreign = bottom_actions(obs, 1)
        foreign[0]["semantic"]["card"] = stable_ref(999, 30, "p1", "Hand")
        bad.append((obs, foreign))  # opponent's card
        obs = pregame("p0", phase="bottom", counts=(1, 0), announced=(True, False))
        bad.append((obs, bottom_actions(obs, 2)))  # remaining differs
        obs = pregame("p0", phase="bottom", counts=(1, 0), announced=(True, False))
        bad.append((obs, legal_actions()))  # gameplay rows at a pregame decision
        obs = in_game()
        obs["projection"]["london_mulligans"] = {"phase": "announce", "counts": [0, 0], "kept": [False, False]}
        bad.append((obs, legal_actions()))  # projection without the extension
        for obs, actions in bad:
            with self.assertRaises(v10.FeatureSchemaError):
                v10.encode_decision(obs, actions)

    def test_rust_pregame_parity_fixture(self):
        """Rust V7 tensors (``flat_v7_pregame_parity_fixture_matches_rust_tensors``)
        equal this encoder's on the same actor-visible observation and menu."""
        root = Path(__file__).resolve().parents[2]
        fixture = json.loads((root / "data/flat_policy_v7/pregame_parity_fixture_v1.json").read_text())
        self.assertEqual(fixture["feature_contract_digest"], v10.feature_contract_fingerprint())
        self.assertEqual(len(fixture["cases"]), 8)
        for case in fixture["cases"]:
            encoded = v10.encode_decision(case["observation"], case["legal_actions"])
            for name in TENSORS:
                expected = case["tensor"][name]
                actual = getattr(encoded, name)
                if actual.dtype == torch.float32:
                    expected_tensor = torch.tensor(expected, dtype=torch.float32)
                    self.assertTrue(torch.equal(actual.reshape(-1), expected_tensor),
                                    f"{name} {case['starting_player']} {case['step']}")
                else:
                    self.assertEqual(actual.reshape(-1).tolist(), expected,
                                     f"{name} {case['starting_player']} {case['step']}")

    def test_descriptor_and_rust_identity_match_actual_source(self):
        root = Path(__file__).resolve().parents[2]
        raw = (root / "data/flat_policy_v7/feature_contract_v7.json").read_bytes()
        descriptor = json.loads(raw)
        identity = (root / "data/flat_policy_v7/feature_identity.rs").read_text()
        self.assertEqual(descriptor["features_source_sha256"], hashlib.sha256(Path(v10.__file__).read_bytes()).hexdigest())
        self.assertEqual(descriptor["feature_contract_digest"], v10.feature_contract_fingerprint())
        self.assertEqual(descriptor["feature_encoding_digest"], v10.encoding_contract_fingerprint())
        self.assertIn(hashlib.sha256(raw).hexdigest(), identity)
        v4 = json.loads((root / "data/flat_policy_v4/feature_contract_v4.json").read_bytes())
        self.assertEqual(descriptor["dimensions"], v4["dimensions"])
        self.assertNotEqual(descriptor["feature_contract_digest"], v4["feature_contract_digest"])
        self.assertNotEqual(descriptor["feature_encoding_digest"], v4["feature_encoding_digest"])


if __name__ == "__main__":
    unittest.main()
