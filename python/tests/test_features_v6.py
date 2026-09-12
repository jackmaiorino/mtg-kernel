from __future__ import annotations

import copy
import hashlib
import json
from pathlib import Path
import random
import unittest

import torch

from mtg_kernel_rl import features as frozen
from mtg_kernel_rl import features_v6 as v6
from fixtures import legal_actions, observation, public_card, stable_ref


TENSORS = ("state", "object_features", "object_card_ids", "object_groups", "object_node_ids",
           "edge_features", "edge_source_indices", "edge_target_indices", "action_features",
           "action_ref_features", "action_ref_card_ids", "action_ref_action_indices", "action_ref_node_indices")


def successor() -> dict:
    obs = observation()
    obs["schema_version"] = 6
    obs["extensions"] = {"pending_cast_object_cost": None, "decision_local_library": None,
                         "historical_public_sources": []}
    return obs


def action(index: int, semantic: dict) -> dict:
    return {"schema_version": 5, "selected_index": index, "stable_id": f"legal-action-v5:v6-test-{index}",
            "display_text": None, "semantic": {"actor": "p0", **semantic}}


def search_decision(*, selected: bool = False, historical_zone: str = "Battlefield") -> tuple[dict, list]:
    obs = successor()
    source = stable_ref(90, 110, "p0", historical_zone, 2)
    obs["projection"]["stack"] = []
    engine = obs["projection"]["engine_context"]
    engine["stack_nonempty"] = False
    refs = [stable_ref(101, 120, zone="Library"), stable_ref(102, 120, zone="Library"),
            stable_ref(103, 121, zone="Library")]
    picked = [{"target_kind": "object", "object": refs[0]}] if selected else []
    legal = [{"target_kind": "object", "object": ref} for ref in refs[1 if selected else 0:]]
    choice = {"choice_kind": "targets", "player": "p0", "structural_path": [0],
              "selected_targets": picked, "legal_targets": legal, "min_targets": 0,
              "max_targets": 3, "can_finish": True, "ordered": False, "purpose": "search_result"}
    engine.update(current_stage="pending_effect", pending_effect={"source": source, "controller": "p0", "choice": choice})
    obs["extensions"]["decision_local_library"] = {
        "chooser": "p0", "library_owner": "p0", "cards": [{"stable": ref, "card_name": ""} for ref in refs]}
    obs["extensions"]["historical_public_sources"] = [
        {"context": {"kind": "pending_effect"}, "source": source, "stack_item_kind": "activated_ability"}]
    actions = [action(i, {"action_kind": "choose_effect_target", "source": source, "target": target,
                          "selected_count": len(picked), "min_targets": 0, "max_targets": 3}) for i, target in enumerate(legal)]
    actions.append(action(len(actions), {"action_kind": "finish_effect_selection", "source": source, "selected_count": len(picked)}))
    return obs, actions


def escape_decision(selected_count: int = 0, required: int = 2) -> tuple[dict, list]:
    obs = successor()
    p = obs["projection"]
    source = copy.deepcopy(p["stack"][-1]["source"])
    p["stack"][-1]["cast_method"] = "escape"
    p["graveyards"][0] += [public_card(60, 132, "p0", "Graveyard"), public_card(61, 133, "p0", "Graveyard")]
    graveyard = [card["stable"] for card in p["graveyards"][0]]
    pending = {"source": source, "controller": "p0", "chosen_targets": [], "is_flashback": False,
               "cast_mode": "Normal", "additional_cost_discarded": None, "mode_chosen": None,
               "origin_zone": "Graveyard", "sacrifice_chosen": [], "kicked": False}
    p["engine_context"].update(current_stage="pending_cast", pending_cast=pending)
    cost = {"source": source, "controller": "p0", "cast_method": "escape", "cost_kind": "ExileFromGraveyard",
            "required_count": required, "selected": graveyard[:selected_count], "remaining_count": required - selected_count}
    obs["extensions"]["pending_cast_object_cost"] = cost
    actions = [action(i, {"action_kind": "choose_cost_target", "source": source, "cost_kind": "ExileFromGraveyard",
                          "remaining": cost["remaining_count"], "candidate": ref}) for i, ref in enumerate(graveyard[selected_count:])]
    return obs, actions


def renumber(value, mapping):
    if isinstance(value, dict):
        return {key: (mapping[child] if key == "arena_id" else child + 19 if key in ("zone_change_count", "zone_change_generation")
                      else renumber(child, mapping)) for key, child in value.items()}
    if isinstance(value, list):
        return [renumber(child, mapping) for child in value]
    return value


class FeaturesV6Tests(unittest.TestCase):
    def assert_tensors_equal(self, left, right):
        for name in TENSORS:
            self.assertTrue(torch.equal(getattr(left, name), getattr(right, name)), name)

    def test_frozen_source_unchanged_and_common_dimensions_transfer_explicit(self):
        self.assertEqual(hashlib.sha256(Path(frozen.__file__).read_bytes()).hexdigest(),
                         "5d82f5b87a6819076c903390230015da456f914828890d9c5384af410f21be1c")
        old = frozen.encode_decision(observation(), legal_actions())
        new = v6.encode_decision(successor(), legal_actions())
        for name in TENSORS[1:]:
            self.assertTrue(torch.equal(getattr(old, name), getattr(new, name)), name)
        self.assertTrue(torch.equal(old.state[:-96], new.state[:-96]))
        self.assertFalse(torch.equal(old.state[-96:], new.state[-96:]))
        self.assertEqual((new.schema.state_dim, new.schema.object_feature_dim, new.schema.edge_feature_dim,
                          new.schema.action_feature_dim, new.schema.action_ref_feature_dim, new.schema.object_group_count),
                         (219, 98, 41, 195, 25, 20))
        with self.assertRaises(frozen.FeatureSchemaError):
            frozen.encode_decision(successor(), legal_actions())

    def test_search_operational_renumbering_and_duplicate_class_features(self):
        for selected in (False, True):
            obs, actions = search_decision(selected=selected)
            encoded = v6.encode_decision(obs, actions)
            refs = list(v6._iter_card_refs(obs))
            ids = sorted({ref["arena_id"] for ref in refs})
            mapping = dict(zip(ids, reversed(range(1000, 1000 + len(ids)))))
            self.assert_tensors_equal(encoded, v6.encode_decision(renumber(obs, mapping), renumber(actions, mapping)))
            indices = [i for i, token in enumerate(encoded.object_card_ids.tolist()) if token == 121]
            self.assertEqual(len(indices), 2)
            self.assertTrue(torch.equal(encoded.object_features[indices[0]], encoded.object_features[indices[1]]))
            canonical = json.dumps(v6.canonical_observation_v6(obs), sort_keys=True)
            self.assertNotIn("arena_id", canonical)
            self.assertNotIn("zone_change_count", canonical)
            self.assertNotIn("card_name", canonical)

    def test_search_rejects_hidden_or_inconsistent_authority(self):
        mutations = [
            lambda o: o["extensions"]["decision_local_library"].update(chooser="p1"),
            lambda o: o["extensions"]["decision_local_library"].update(library_owner="p1"),
            lambda o: o["extensions"]["decision_local_library"]["cards"][0].update(position=3),
            lambda o: o["extensions"]["decision_local_library"]["cards"].append({"stable": stable_ref(999, 200, zone="Library"), "card_name": ""}),
            lambda o: o["extensions"].update(decision_local_library=None),
            lambda o: o["projection"]["engine_context"]["pending_effect"]["choice"].update(purpose="card_selection"),
            lambda o: o["extensions"]["decision_local_library"]["cards"].reverse(),
        ]
        for mutate in mutations:
            obs, actions = search_decision()
            mutate(obs)
            with self.subTest(mutation=mutate), self.assertRaises(v6.FeatureSchemaError):
                v6.encode_decision(obs, actions)
        obs, actions = search_decision()
        actions[0], actions[2] = actions[2], actions[0]
        for index, item in enumerate(actions):
            item["selected_index"] = index
        with self.assertRaises(v6.FeatureSchemaError):
            v6.encode_decision(obs, actions)

    def test_search_producer_recanonicalization_erases_hidden_permutation_and_id_ties(self):
        for selected in (False, True):
            obs, actions = search_decision(selected=selected)
            original = v6.encode_decision(obs, actions)
            ids = sorted({ref["arena_id"] for ref in v6._iter_card_refs(obs)})
            for seed in range(8):
                rng = random.Random(seed)
                assigned = list(range(500, 500 + len(ids)))
                rng.shuffle(assigned)
                mapping = dict(zip(ids, assigned))
                changed, changed_actions = renumber(obs, mapping), renumber(actions, mapping)
                choice = changed["projection"]["engine_context"]["pending_effect"]["choice"]
                cards = changed["extensions"]["decision_local_library"]["cards"]
                ranks = {v6._stable_key(target["object"]): i for i, target in enumerate(choice["selected_targets"])}
                rng.shuffle(cards)
                cards.sort(key=lambda c: (v6.visible_library_card_key_v6(c["stable"]), ranks.get(v6._stable_key(c["stable"]), v6.U64), c["stable"]["arena_id"]))
                choice["legal_targets"].sort(key=lambda target: (v6.visible_library_card_key_v6(target["object"]), target["object"]["arena_id"]))
                target_actions, finish = changed_actions[:-1], changed_actions[-1]
                target_actions.sort(key=lambda a: (v6.visible_library_card_key_v6(a["semantic"]["target"]["object"]), a["semantic"]["target"]["object"]["arena_id"]))
                changed_actions = target_actions + [finish]
                for index, candidate in enumerate(changed_actions):
                    candidate["selected_index"] = index
                self.assert_tensors_equal(original, v6.encode_decision(changed, changed_actions))
        invalid, invalid_actions = search_decision(selected=True)
        cards = invalid["extensions"]["decision_local_library"]["cards"]
        cards[0], cards[1] = cards[1], cards[0]
        with self.assertRaises(v6.FeatureSchemaError):
            v6.encode_decision(invalid, invalid_actions)

    def test_coherently_forged_opponent_library_still_rejected(self):
        obs, actions = search_decision(selected=True)
        obs["extensions"]["decision_local_library"]["library_owner"] = "p1"
        # Alter every duplicate JSON binding consistently. Identity mismatch
        # checks alone cannot catch this unauthorized expansion of search scope.
        for ref in v6._iter_card_refs([obs, actions]):
            if ref["zone"] == "Library":
                ref["owner"] = "p1"
                ref["controller"] = "p1"
        with self.assertRaisesRegex(v6.FeatureSchemaError, "chooser's own library"):
            v6.encode_decision(obs, actions)

    def test_known_library_exact_node_is_reused(self):
        obs, actions = search_decision()
        known = copy.deepcopy(obs["extensions"]["decision_local_library"]["cards"][0])
        obs["known_library_cards"][0] = [{"position": 4, "card": known}]
        encoded = v6.encode_decision(obs, actions)
        self.assertEqual(encoded.object_card_ids.tolist().count(121), 2)
        self.assertIn(v6.OBJECT_GROUPS.index("known_library_self"), encoded.object_groups.tolist())

    def test_historical_context_accepts_destroyed_and_announced_hand_sources(self):
        for zone in ("Battlefield", "Hand", "Stack"):
            obs, actions = search_decision(historical_zone=zone)
            if zone == "Stack":
                obs["extensions"]["historical_public_sources"][0]["stack_item_kind"] = "spell"
            v6.encode_decision(obs, actions)
        obs, actions = search_decision()
        live = public_card(90, 110, "p0")
        live["stable"]["zone_change_count"] = 3
        obs["projection"]["battlefield"][0].append(live)
        encoded = v6.encode_decision(obs, actions)
        self.assertEqual(encoded.object_card_ids.tolist().count(111), 2)

    def test_historical_source_forgery_and_context_mismatch_rejected(self):
        mutations = [
            lambda o: o["extensions"]["historical_public_sources"].clear(),
            lambda o: o["extensions"]["historical_public_sources"][0].update(source=stable_ref(90, 999, "p0", "Battlefield", 2)),
            lambda o: o["extensions"]["historical_public_sources"][0].update(context={"kind": "stack", "stack_index": 10}),
            lambda o: o["extensions"]["historical_public_sources"].append(copy.deepcopy(o["extensions"]["historical_public_sources"][0])),
        ]
        for mutate in mutations:
            obs, actions = search_decision()
            mutate(obs)
            with self.assertRaises(v6.FeatureSchemaError):
                v6.encode_decision(obs, actions)

    def test_existing_detached_context_precedes_new_history_with_same_ordinal(self):
        obs = successor()
        projection = obs["projection"]
        historical = stable_ref(90, 110, zone="Battlefield", zone_change_count=2)
        item = projection["stack"][0]
        item.update(source=historical, stack_item_kind="activated_ability", cast_method=None)
        projection["engine_context"].update(current_stage="pending_discard", pending_discard={
            "player": "p0", "count": 1, "resume_stage": "finish_spell_resolution",
            "resume_source": stable_ref(91, 111, zone="Stack")})
        projection["surface_context"].update(current_stage="discard_pick", private_discard={
            "chosen": [], "remaining_choices": [card["stable"] for card in obs["own_hand"]], "remaining_needed": 1})
        obs["extensions"]["historical_public_sources"] = [
            {"context": {"kind": "stack", "stack_index": 0}, "source": historical, "stack_item_kind": "activated_ability"}]
        encoded = v6.encode_decision(obs, [action(0, {"action_kind": "discard", "cards": [obs["own_hand"][0]["stable"]]})])
        ids = encoded.object_card_ids.tolist()
        old_index, new_index = ids.index(112), ids.index(111)
        self.assertLess(old_index, new_index)
        self.assertEqual(encoded.object_features[old_index][-1].item(), 0.0)
        self.assertEqual(encoded.object_features[new_index][-1].item(), 0.0)

    def test_unrelated_hidden_library_and_unannounced_hand_cannot_gain_history_authority(self):
        for zone, kind in (("Library", "activated_ability"), ("Hand", "triggered_ability")):
            obs, actions = search_decision(historical_zone=zone)
            obs["extensions"]["historical_public_sources"][0]["stack_item_kind"] = kind
            with self.assertRaises(v6.FeatureSchemaError):
                v6.encode_decision(obs, actions)

    def test_escape_value_context_changes_without_pooling_actions(self):
        zero, zero_actions = escape_decision()
        one, one_actions = escape_decision(1)
        more, more_actions = escape_decision(required=3)
        a, b, c = (v6.encode_decision(o, acts) for o, acts in ((zero, zero_actions), (one, one_actions), (more, more_actions)))
        self.assertFalse(torch.equal(a.state, b.state))
        self.assertFalse(torch.equal(a.state, c.state))
        self.assertEqual(len(b.edge_features), len(a.edge_features) + 1)
        self.assertEqual(zero["projection"]["engine_context"]["pending_cast"]["sacrifice_chosen"], [])

    def test_escape_bad_prefix_and_candidate_rejected(self):
        mutations = [
            lambda o: o["extensions"]["pending_cast_object_cost"].update(remaining_count=7),
            lambda o: o["extensions"]["pending_cast_object_cost"].update(cast_method="flashback"),
            lambda o: o["extensions"]["pending_cast_object_cost"].update(selected=[o["projection"]["graveyards"][1][0]["stable"]]),
            lambda o: o["extensions"]["pending_cast_object_cost"].update(selected=[o["extensions"]["pending_cast_object_cost"]["source"]]),
            lambda o: o["extensions"].update(pending_cast_object_cost=None),
        ]
        for mutate in mutations:
            obs, actions = escape_decision(1)
            mutate(obs)
            with self.assertRaises(v6.FeatureSchemaError):
                v6.encode_decision(obs, actions)
        obs, actions = escape_decision(1)
        actions[0]["semantic"]["candidate"] = obs["extensions"]["pending_cast_object_cost"]["selected"][0]
        with self.assertRaises(v6.FeatureSchemaError):
            v6.encode_decision(obs, actions)

    def test_descriptor_and_rust_identity_match_actual_source(self):
        root = Path(__file__).resolve().parents[2]
        raw = (root / "data/flat_policy_v3/feature_contract_v3.json").read_bytes()
        descriptor = json.loads(raw)
        identity = (root / "data/flat_policy_v3/feature_identity.rs").read_text()
        self.assertEqual(descriptor["features_source_sha256"], hashlib.sha256(Path(v6.__file__).read_bytes()).hexdigest())
        self.assertEqual(descriptor["feature_contract_digest"], v6.feature_contract_fingerprint())
        self.assertEqual(descriptor["feature_encoding_digest"], v6.encoding_contract_fingerprint())
        self.assertIn(hashlib.sha256(raw).hexdigest(), identity)


if __name__ == "__main__":
    unittest.main()
