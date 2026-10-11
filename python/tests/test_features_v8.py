"""Successor JSON regressions, independent of unqualified Sire gameplay."""
from __future__ import annotations

import copy
import hashlib
import json
from pathlib import Path
import unittest

import torch

from mtg_kernel_rl import features_v7 as v7
from mtg_kernel_rl import features_v8 as v8
from fixtures import legal_actions, stable_ref
from test_features import seat_swapped
from test_features_v6 import TENSORS, action, renumber, successor, ward_decision


def observation_v8():
    obs = successor()
    obs["schema_version"] = 8
    return obs


def life_decision(*, abilities=False, queued=False):
    obs, actions = ward_decision(abilities=abilities, queued=queued)
    obs["schema_version"] = 8
    ext = obs["extensions"]
    if queued:
        ext["queued_ward_life_payments"] = ext.pop("queued_ward_payments")
        payment = ext["queued_ward_life_payments"][0]["payment"]
    else:
        ext["pending_ward_life_payment"] = ext.pop("pending_ward_payment")
        payment = ext["pending_ward_life_payment"]
        source = obs["projection"]["engine_context"]["pending_effect"]["source"]
        actions = [action(i, {"action_kind": "choose_effect_boolean", "source": source, "value": value})
                   for i, value in enumerate((False, True))]
    del payment["generic"]
    payment["life"] = 7
    return obs, actions


def hidden_trigger_decision(count=2):
    obs = observation_v8()
    p = obs["projection"]
    p["stack"] = []
    engine = p["engine_context"]
    engine.update(current_stage="pending_triggers", stack_nonempty=False,
                  pending_triggers=[{"source": None, "controller": "p0", "trigger_kind": "triggered_ability",
                                     "kicked": False} for _ in range(count)])
    source = stable_ref(990, 160, "p0", "Battlefield", 4)
    obs["extensions"]["historical_public_sources"] = [
        {"context": {"kind": "pending_trigger", "position": i}, "source": copy.deepcopy(source),
         "stack_item_kind": "triggered_ability"} for i in range(count)]
    obs["extensions"]["ward_life_card_traits"] = [
        {"source": source, "life": 7, "printed_face": "front", "effective": False}]
    if count == 1:
        actions = [action(0, {"action_kind": "choose_target", "source": source, "remaining": 1,
                              "target": {"target_kind": "player", "player": "p1"}})]
    else:
        actions = [action(i, {"action_kind": "order_triggers", "pending_sources": [source] * count,
                              "order": order}) for i, order in enumerate(([0, 1], [1, 0]))]
    return obs, actions


def trait_edges(encoded):
    # Extension subroles occupy the second public ordinal column.
    return torch.nonzero(encoded.edge_features[:, len(v8.EDGE_ROLES) + 1] == 40 / 64).flatten()


class FeaturesV8Tests(unittest.TestCase):
    def assert_tensors_equal(self, first, second):
        for name in TENSORS:
            self.assertTrue(torch.equal(getattr(first, name), getattr(second, name)), name)

    def test_exact_life_is_affordable_and_underpayment_refuses(self):
        obs, actions = life_decision()
        obs["projection"]["life_totals"][0] = 7
        v8.encode_decision(obs, actions)
        obs["projection"]["life_totals"][0] = 6
        with self.assertRaises(v8.FeatureSchemaError):
            v8.encode_decision(obs, actions)

    def test_zero_life_payment_refuses_like_engine(self):
        for queued in (False, True):
            obs, actions = life_decision(queued=queued)
            payment = (obs["extensions"]["queued_ward_life_payments"][0]["payment"] if queued
                       else obs["extensions"]["pending_ward_life_payment"])
            payment["life"] = 0
            with self.subTest(queued=queued), self.assertRaises(v8.FeatureSchemaError):
                v8.encode_decision(obs, actions)

    def test_life_menu_requires_both_bound_choices_in_engine_order(self):
        obs, actions = life_decision()
        for invalid in ([actions[0]], [actions[1]], [actions[1], actions[0]],
                        [actions[0], {**actions[1], "semantic": actions[0]["semantic"]}]):
            with self.subTest(invalid=invalid), self.assertRaises(v8.FeatureSchemaError):
                v8.encode_decision(obs, invalid)
        invalid = copy.deepcopy(actions)
        invalid[1]["semantic"]["source"]["zone_change_count"] += 1
        with self.assertRaises(v8.FeatureSchemaError):
            v8.encode_decision(obs, invalid)

    def test_exact_targeter_ordinal_and_fee_change_model_input(self):
        for abilities in (False, True):
            for queued in (False, True):
                with self.subTest(abilities=abilities, queued=queued):
                    obs, actions = life_decision(abilities=abilities, queued=queued)
                    first = v8.encode_decision(obs, actions)
                    payment = (obs["extensions"]["queued_ward_life_payments"][0]["payment"] if queued
                               else obs["extensions"]["pending_ward_life_payment"])
                    payment["targeting_stack_index"] = 1
                    second = v8.encode_decision(obs, actions)
                    self.assertFalse(torch.equal(first.state, second.state))
                    self.assertFalse(torch.equal(first.edge_features, second.edge_features))
                    if abilities:
                        self.assertTrue(torch.equal(first.edge_target_indices, second.edge_target_indices))
                    payment["life"] = 3
                    third = v8.encode_decision(obs, actions)
                    self.assertFalse(torch.equal(second.edge_features, third.edge_features))

    def test_pending_payment_rejects_wrong_payer_target_and_mixed_fee(self):
        for mutation in ("payer", "targeter", "incarnation", "mixed"):
            obs, actions = life_decision()
            fee = obs["extensions"]["pending_ward_life_payment"]
            if mutation == "payer":
                fee["payer"] = "p1"
            elif mutation == "targeter":
                fee["targeting_stack_index"] = 9
            elif mutation == "incarnation":
                obs["projection"]["stack"][0]["targets"][0]["object"] = stable_ref(3, 21, "p1", "Battlefield", 9)
            else:
                obs["extensions"]["pending_ward_payment"] = {"targeting_stack_index": 0, "payer": "p0", "generic": 2}
            with self.subTest(mutation=mutation), self.assertRaises(v8.FeatureSchemaError):
                v8.encode_decision(obs, actions)

    def test_queued_payment_cannot_share_trigger_with_generic_or_point_above_it(self):
        for mutation in ("mixed", "self_target", "payer", "duplicate"):
            obs, actions = life_decision(queued=True)
            rows = obs["extensions"]["queued_ward_life_payments"]
            if mutation == "mixed":
                obs["extensions"]["queued_ward_payments"] = [
                    {"stack_index": 2, "payment": {"targeting_stack_index": 0, "payer": "p0", "generic": 2}}]
            elif mutation == "self_target":
                rows[0]["payment"]["targeting_stack_index"] = 2
            elif mutation == "payer":
                rows[0]["payment"]["payer"] = "p1"
            else:
                rows.append(copy.deepcopy(rows[0]))
            with self.subTest(mutation=mutation), self.assertRaises(v8.FeatureSchemaError):
                v8.encode_decision(obs, actions)

    def test_printed_trait_suppression_fee_and_face_are_visible(self):
        obs = observation_v8()
        source = obs["projection"]["battlefield"][0][0]["stable"]
        trait = {"source": source, "life": 7, "printed_face": "front", "effective": True}
        obs["extensions"]["ward_life_card_traits"] = [trait]
        active = v8.encode_decision(obs, legal_actions())
        trait["effective"] = False
        suppressed = v8.encode_decision(obs, legal_actions())
        self.assertFalse(torch.equal(active.state, suppressed.state))
        self.assertFalse(torch.equal(active.edge_features, suppressed.edge_features))
        trait["life"] = 3
        lower = v8.encode_decision(obs, legal_actions())
        self.assertFalse(torch.equal(lower.edge_features, suppressed.edge_features))
        trait["printed_face"] = "back"
        back = v8.encode_decision(obs, legal_actions())
        self.assertFalse(torch.equal(back.edge_features, lower.edge_features))

    def test_trait_requires_exact_already_projected_source_and_effective_face(self):
        for mutation in ("hidden", "duplicate", "face", "incarnation", "effective_hand"):
            obs = observation_v8()
            trait = {"source": copy.deepcopy(obs["projection"]["battlefield"][0][0]["stable"]),
                     "life": 7, "printed_face": "front", "effective": True}
            obs["extensions"]["ward_life_card_traits"] = [trait]
            if mutation == "hidden":
                trait["source"] = stable_ref(991, 161, "p1", "Library")
                trait["effective"] = False
            elif mutation == "duplicate":
                obs["extensions"]["ward_life_card_traits"].append(copy.deepcopy(trait))
            elif mutation == "face":
                trait["printed_face"] = "back"
            elif mutation == "incarnation":
                trait["source"]["zone_change_count"] += 1
            else:
                trait["source"] = copy.deepcopy(obs["own_hand"][0]["stable"])
            with self.subTest(mutation=mutation), self.assertRaises(v8.FeatureSchemaError):
                v8.encode_decision(obs, legal_actions())

    def test_trait_order_and_operational_renumbering_are_invisible(self):
        obs, actions = life_decision(abilities=True)
        obs["extensions"]["ward_life_card_traits"] = [
            {"source": copy.deepcopy(cards[0]["stable"]), "life": fee, "printed_face": "front", "effective": True}
            for cards, fee in zip(obs["projection"]["battlefield"], (3, 7))]
        first = v8.encode_decision(obs, actions)
        obs["extensions"]["ward_life_card_traits"].reverse()
        self.assert_tensors_equal(first, v8.encode_decision(obs, actions))
        ids = {ref["arena_id"] for ref in v8._iter_card_refs_by_schema(obs, v8.OBSERVATION_SPEC)}
        ids.update(ref["arena_id"] for a in actions for ref in v8._iter_card_refs_by_schema(a, v8.LEGAL_ACTION_SPEC))
        mapping = {old: 1000 + i for i, old in enumerate(sorted(ids, reverse=True))}
        self.assert_tensors_equal(first, v8.encode_decision(renumber(obs, mapping), renumber(actions, mapping)))

    def test_shared_hidden_source_preserves_trigger_positions_and_all_trait_aliases(self):
        obs, actions = hidden_trigger_decision()
        encoded = v8.encode_decision(obs, actions)
        nodes = encoded.action_ref_node_indices.tolist()
        self.assertEqual(nodes[:2], nodes[2:])
        self.assertNotEqual(nodes[0], nodes[1])
        edges = trait_edges(encoded)
        self.assertEqual(len(edges), 2)
        self.assertEqual(set(encoded.edge_source_indices[edges].tolist()), set(nodes))
        self.assertTrue(torch.equal(encoded.edge_source_indices[edges], encoded.edge_target_indices[edges]))
        mapping = {ref["arena_id"]: 2000 + i for i, ref in enumerate(v8._iter_card_refs_by_schema(obs, v8.OBSERVATION_SPEC))}
        self.assert_tensors_equal(encoded, v8.encode_decision(renumber(obs, mapping), renumber(actions, mapping)))

    def test_single_hidden_trigger_target_selection_is_supported(self):
        obs, actions = hidden_trigger_decision(1)
        encoded = v8.encode_decision(obs, actions)
        edges = trait_edges(encoded)
        self.assertEqual(encoded.action_ref_node_indices[0], encoded.edge_source_indices[edges][0])
        source = actions[0]["semantic"]["source"]
        finished = [action(0, {"action_kind": "finish_target_selection", "source": source, "selected_count": 0})]
        self.assertEqual(v8.encode_decision(obs, finished).action_ref_node_indices[0], encoded.action_ref_node_indices[0])

    def test_visible_pending_trigger_modes_preserve_existing_semantics(self):
        obs = observation_v8()
        engine = obs["projection"]["engine_context"]
        source = obs["projection"]["battlefield"][0][0]["stable"]
        engine.update(current_stage="pending_triggers", pending_triggers=[
            {"source": source, "controller": "p0", "trigger_kind": "triggered_ability", "kicked": False}])
        actions = [action(i, {"action_kind": "choose_spell_mode", "source": source,
                              "mode_index": i, "mode_count": 2}) for i in range(2)]
        v8.encode_decision(obs, actions)
        actions[1] = action(1, {"action_kind": "choose_spell_mode", "source": obs["own_hand"][0]["stable"],
                                "mode_index": 1, "mode_count": 2})
        with self.assertRaises(v8.FeatureSchemaError):
            v8.encode_decision(obs, actions)

    def test_actor_relative_life_fees_traits_and_captured_positions(self):
        cases = [life_decision(abilities=True), life_decision(queued=True), hidden_trigger_decision()]
        for obs, actions in cases:
            with self.subTest(stage=obs["projection"]["engine_context"]["current_stage"]):
                self.assert_tensors_equal(v8.encode_decision(obs, actions),
                                          v8.encode_decision(*seat_swapped(obs, actions)))

    def test_traits_preserve_captured_controller_and_departed_incarnation(self):
        obs, actions = life_decision(abilities=True)
        captured = copy.deepcopy(obs["extensions"]["historical_public_sources"][0]["source"])
        live = obs["projection"]["battlefield"][0].pop(0)
        live["stable"]["controller"] = "p1"
        obs["projection"]["battlefield"][1].append(live)
        obs["extensions"]["ward_life_card_traits"] = [
            {"source": captured, "life": 7, "printed_face": "front", "effective": False},
            {"source": copy.deepcopy(live["stable"]), "life": 7, "printed_face": "front", "effective": True}]
        encoded = v8.encode_decision(obs, actions)
        self.assertEqual(len(trait_edges(encoded)), 2)
        live["stable"]["zone_change_count"] += 1
        obs["extensions"]["ward_life_card_traits"][1]["source"] = copy.deepcopy(live["stable"])
        departed = v8.encode_decision(obs, actions)
        self.assertEqual(len(trait_edges(departed)), 2)

    def test_hidden_trigger_actions_and_contexts_bind_exact_positions(self):
        for mutation in ("out_of_bounds", "wrong_controller", "wrong_source", "missing_position"):
            obs, actions = hidden_trigger_decision()
            history = obs["extensions"]["historical_public_sources"]
            if mutation == "out_of_bounds":
                history[1]["context"]["position"] = 2
            elif mutation == "wrong_controller":
                obs["projection"]["engine_context"]["pending_triggers"][1]["controller"] = "p1"
            elif mutation == "wrong_source":
                actions = copy.deepcopy(actions)
                actions[0]["semantic"]["pending_sources"][1] = stable_ref(992, 160, "p0", "Battlefield", 4)
            else:
                history.pop()
            with self.subTest(mutation=mutation), self.assertRaises(v8.FeatureSchemaError):
                v8.encode_decision(obs, actions)

    def test_optional_trait_and_payment_absence_is_omitted_not_null_or_empty(self):
        for name, value in (("pending_ward_life_payment", None), ("queued_ward_life_payments", []),
                            ("ward_life_card_traits", [])):
            obs = observation_v8()
            obs["extensions"][name] = value
            with self.subTest(name=name), self.assertRaises(v8.FeatureSchemaError):
                v8.encode_decision(obs, legal_actions())

    def test_successor_identity_and_generated_pins_match_actual_source(self):
        root = Path(__file__).resolve().parents[2]
        raw = (root / "data/flat_policy_v5/feature_contract_v5.json").read_bytes()
        descriptor = json.loads(raw)
        identity = (root / "data/flat_policy_v5/feature_identity.rs").read_text()
        self.assertEqual(descriptor["features_source_sha256"], hashlib.sha256(Path(v8.__file__).read_bytes()).hexdigest())
        self.assertEqual(descriptor["feature_contract_digest"], v8.feature_contract_fingerprint())
        self.assertEqual(descriptor["feature_encoding_digest"], v8.encoding_contract_fingerprint())
        self.assertIn(hashlib.sha256(raw).hexdigest(), identity)
        self.assertNotEqual(v8.feature_contract_fingerprint(), v7.feature_contract_fingerprint())
        self.assertNotEqual(v8.encoding_contract_fingerprint(), v7.encoding_contract_fingerprint())
        self.assertNotEqual(v8.MODEL_CONTRACT_VERSION, v7.MODEL_CONTRACT_VERSION)
        obs = observation_v8()
        encoded = v8.encode_decision(obs, legal_actions())
        self.assertEqual(encoded.state.shape[0], 219)
        with self.assertRaises(v7.FeatureSchemaError):
            v7.encode_decision(obs, legal_actions())
        with self.assertRaises(v8.FeatureSchemaError):
            v8.encode_decision(successor(), legal_actions())

    def test_known_face_down_knowledge_is_not_admitted_by_frozen_successors(self):
        from mtg_kernel_rl import features_v9 as v9

        obs = observation_v8()
        obs["known_face_down_cards"] = [{
            "object": stable_ref(991, 160, "p0", "Battlefield", 4),
            "card_db_id": 160,
            "card_name": "Known face-down card",
        }]
        with self.assertRaises(v8.FeatureSchemaError):
            v8.encode_decision(obs, legal_actions())
        raid = copy.deepcopy(obs)
        raid["schema_version"] = 9
        raid["extensions"] = {
            "predecessor": raid["extensions"],
            "attacked_this_turn": [False, False],
        }
        with self.assertRaises(v8.FeatureSchemaError):
            v9.encode_decision(raid, legal_actions())


if __name__ == "__main__":
    torch.set_num_threads(1)
    unittest.main()
