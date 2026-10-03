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
from fixtures import combat_decision_response, legal_actions, observation, public_card, stable_ref


TENSORS = ("state", "object_features", "object_card_ids", "object_groups", "object_node_ids",
           "edge_features", "edge_source_indices", "edge_target_indices", "action_features",
           "action_ref_features", "action_ref_card_ids", "action_ref_action_indices", "action_ref_node_indices")


def successor() -> dict:
    obs = observation()
    obs["schema_version"] = 6
    obs["extensions"] = {"pending_cast_object_cost": None, "decision_local_library": None,
                         "historical_public_sources": [], "pending_chosen_creature_cost": None,
                         "finalized_chosen_creature_costs": []}
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


def initiative_source_decision() -> tuple[dict, list]:
    """Coherent JSON regression for event::log_initiative_trigger's snapshot.

    Actual engine fixture parity is checked separately by the native emitter.
    """
    obs, actions = search_decision()
    source = obs["extensions"]["historical_public_sources"][0]["source"]
    source.update(card_db_id=1, owner="p1", controller="p0")
    obs["extensions"]["historical_public_sources"][0]["stack_item_kind"] = "triggered_ability"
    live = public_card(90, 1, "p1")
    live["stable"]["zone_change_count"] = 2
    obs["projection"]["battlefield"][1].append(live)
    return obs, actions


def renumber(value, mapping):
    if isinstance(value, dict):
        return {key: (mapping[child] if key == "arena_id" else child + 19 if key in ("zone_change_count", "zone_change_generation")
                      else renumber(child, mapping)) for key, child in value.items()}
    if isinstance(value, list):
        return [renumber(child, mapping) for child in value]
    return value


def ward_decision(*, abilities: bool = False, queued: bool = False) -> tuple[dict, list]:
    obs = successor()
    p = obs["projection"]
    source = copy.deepcopy(p["battlefield"][1][0]["stable"])
    template = copy.deepcopy(p["stack"][0])
    stack = []
    for index in range(2):
        item = copy.deepcopy(template)
        item.update(stack_index=index, controller="p0", targets=[{"target_kind": "object", "object": source}])
        if abilities:
            item.update(stack_item_kind="activated_ability", cast_method=None,
                        source=copy.deepcopy(p["battlefield"][0][0]["stable"]))
        else:
            item["source"] = stable_ref(700 + index, 30 + index, "p0", "Stack")
        stack.append(item)
    p["stack"] = stack
    history = [{"context": {"kind": "stack", "stack_index": index},
                "source": copy.deepcopy(item["source"]), "stack_item_kind": "activated_ability"}
               for index, item in enumerate(stack)] if abilities else []
    payment = {"targeting_stack_index": 0, "payer": "p0", "generic": 2}
    if queued:
        trigger = copy.deepcopy(template)
        trigger.update(stack_index=2, source=source, controller="p1", stack_item_kind="triggered_ability",
                       cast_method=None, targets=[])
        stack.append(trigger)
        history.append({"context": {"kind": "stack", "stack_index": 2},
                        "source": source, "stack_item_kind": "triggered_ability"})
        obs["extensions"]["queued_ward_payments"] = [{"stack_index": 2, "payment": payment}]
        actions = legal_actions()
    else:
        choice = {"choice_kind": "boolean", "player": "p0", "structural_path": [],
                  "default": False, "purpose": "pay_cost"}
        p["engine_context"].update(current_stage="pending_effect",
                                    pending_effect={"source": source, "controller": "p1", "choice": choice})
        history.append({"context": {"kind": "pending_effect"}, "source": source,
                        "stack_item_kind": "triggered_ability"})
        obs["extensions"]["pending_ward_payment"] = payment
        actions = [action(index, {"action_kind": "choose_effect_boolean", "source": source, "value": value})
                   for index, value in enumerate((True, False))]
    obs["extensions"]["historical_public_sources"] = history
    return obs, actions


class FeaturesV6Tests(unittest.TestCase):
    def test_ward_binding_distinguishes_simultaneous_spells_and_same_source_abilities(self):
        for abilities in (False, True):
            for queued in (False, True):
                with self.subTest(abilities=abilities, queued=queued):
                    obs, actions = ward_decision(abilities=abilities, queued=queued)
                    first = v6.encode_decision(obs, actions)
                    payment = (obs["extensions"]["queued_ward_payments"][0]["payment"] if queued
                               else obs["extensions"]["pending_ward_payment"])
                    payment["targeting_stack_index"] = 1
                    second = v6.encode_decision(obs, actions)
                    self.assertFalse(torch.equal(first.state, second.state))
                    self.assertFalse(torch.equal(first.edge_features, second.edge_features))
                    if abilities:
                        # Physical source rows alias, but the stack instances must not.
                        self.assertTrue(torch.equal(first.edge_target_indices, second.edge_target_indices))
                    payment["generic"] = 3
                    cost = v6.encode_decision(obs, actions)
                    self.assertFalse(torch.equal(second.state, cost.state))
                    self.assertFalse(torch.equal(second.edge_features, cost.edge_features))

    def test_ward_operational_renumbering_is_invisible(self):
        for abilities in (False, True):
            obs, actions = ward_decision(abilities=abilities)
            ids = {ref["arena_id"] for ref in v6._iter_card_refs_by_schema(obs, v6.OBSERVATION_SPEC)}
            mapping = {value: 10000 + index for index, value in enumerate(sorted(ids, reverse=True))}
            changed = renumber(obs, mapping)
            changed_actions = renumber(actions, mapping)
            self.assert_tensors_equal(v6.encode_decision(obs, actions), v6.encode_decision(changed, changed_actions))
            self.assertEqual(v6.canonical_observation_v6(obs), v6.canonical_observation_v6(changed))

    def test_ward_missing_members_stay_absent_and_noncanonical_nulls_are_rejected(self):
        obs = successor()
        canonical = v6.canonical_observation_v6(obs)
        self.assertNotIn("pending_ward_payment", canonical["extensions"])
        self.assertNotIn("queued_ward_payments", canonical["extensions"])
        for name, value in (("pending_ward_payment", None), ("queued_ward_payments", [])):
            malformed = copy.deepcopy(obs)
            malformed["extensions"][name] = value
            with self.assertRaises(v6.FeatureSchemaError):
                v6.encode_decision(malformed, legal_actions())

    def test_ward_binding_rejects_bad_stack_payer_and_prompt_context(self):
        for key, value in (("targeting_stack_index", 99), ("payer", "p1"), ("generic", 256)):
            obs, actions = ward_decision()
            obs["extensions"]["pending_ward_payment"][key] = value
            with self.assertRaises(v6.FeatureSchemaError):
                v6.encode_decision(obs, actions)
        obs, actions = ward_decision()
        obs["projection"]["engine_context"]["pending_effect"]["choice"]["purpose"] = "generic"
        with self.assertRaises(v6.FeatureSchemaError):
            v6.encode_decision(obs, actions)
        obs, actions = ward_decision(queued=True)
        obs["extensions"]["queued_ward_payments"][0]["stack_index"] = 0
        with self.assertRaises(v6.FeatureSchemaError):
            v6.encode_decision(obs, actions)

    def test_chosen_creature_zone_changes_value_input_and_rejects_other_controller(self):
        obs, _ = escape_decision()
        obs["extensions"]["pending_cast_object_cost"] = None
        obs["projection"]["stack"][-1]["cast_method"] = "normal"
        pending = obs["projection"]["engine_context"]["pending_cast"]
        pending["origin_zone"] = "Hand"
        records = []
        for zone in ("Battlefield", "Hand"):
            changed = copy.deepcopy(obs)
            changed["extensions"]["pending_chosen_creature_cost"] = {
                "source": copy.deepcopy(pending["source"]), "controller": "p0", "selected_zone": zone}
            v6.assert_observation_classified(changed)
            records.append(torch.tensor(v6._state_features(changed)))
        self.assertFalse(torch.equal(*records))
        changed["extensions"]["pending_chosen_creature_cost"]["controller"] = "p1"
        with self.assertRaises(v6.FeatureSchemaError):
            v6.assert_observation_classified(changed)

    def test_visible_paid_power_changes_value_input_and_binds_exact_spell(self):
        obs = successor()
        item = obs["projection"]["stack"][0]
        chosen = copy.deepcopy(obs["projection"]["battlefield"][0][0]["stable"])
        item["paid_cost_refs"] = [chosen]
        record = {"stack_index": 0, "source": copy.deepcopy(item["source"]),
                  "chosen": copy.deepcopy(chosen), "power_lki": -2}
        obs["extensions"]["finalized_chosen_creature_costs"] = [record]
        encoded = v6.encode_decision(obs, legal_actions())
        record["power_lki"] = 7
        different = v6.encode_decision(obs, legal_actions())
        self.assertFalse(torch.equal(encoded.state, different.state))
        for name in TENSORS[1:]:
            self.assertTrue(torch.equal(getattr(encoded, name), getattr(different, name)), name)
        for key, value in (("stack_index", 999), ("source", chosen), ("chosen", item["source"]), ("power_lki", 2**31)):
            forged = copy.deepcopy(obs)
            forged["extensions"]["finalized_chosen_creature_costs"][0][key] = value
            with self.assertRaises(v6.FeatureSchemaError):
                v6.encode_decision(forged, legal_actions())
        duplicate = copy.deepcopy(obs)
        duplicate["extensions"]["finalized_chosen_creature_costs"].append(copy.deepcopy(record))
        with self.assertRaises(v6.FeatureSchemaError):
            v6.encode_decision(duplicate, legal_actions())

    def test_old_v6_extension_shape_is_rejected(self):
        stale = successor()
        del stale["extensions"]["pending_chosen_creature_cost"]
        del stale["extensions"]["finalized_chosen_creature_costs"]
        with self.assertRaises(v6.FeatureSchemaError):
            v6.encode_decision(stale, legal_actions())

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

    def test_initiative_snapshot_keeps_live_and_captured_controllers_distinct(self):
        obs, actions = initiative_source_decision()
        encoded = v6.encode_decision(obs, actions)
        rows = [index for index, token in enumerate(encoded.object_card_ids.tolist()) if token == 2]
        self.assertEqual(len(rows), 2)
        live, historical = rows
        self.assertEqual(encoded.object_groups[live].item(), v6.OBJECT_GROUPS.index("opponent_battlefield"))
        self.assertEqual(encoded.object_groups[historical].item(), v6.OBJECT_GROUPS.index("pending_context"))
        self.assertFalse(torch.equal(encoded.object_features[live], encoded.object_features[historical]))
        self.assertEqual(encoded.action_ref_node_indices[0].item(), historical)
        self.assertEqual(obs["projection"]["battlefield"][1][-1]["stable"]["controller"], "p1")
        old = copy.deepcopy(obs)
        old["schema_version"] = 5
        del old["extensions"]
        with self.assertRaises(frozen.FeatureSchemaError):
            frozen.encode_decision(old, actions)

    def test_historical_controller_exception_cannot_change_immutable_or_unrelated_refs(self):
        for field, value in (("card_db_id", 999), ("owner", "p0"), ("zone", "Graveyard")):
            obs, actions = initiative_source_decision()
            obs["extensions"]["historical_public_sources"][0]["source"][field] = value
            with self.assertRaisesRegex(v6.FeatureSchemaError, "immutable"):
                v6.encode_decision(obs, actions)
        obs, actions = initiative_source_decision()
        captured = copy.deepcopy(obs["extensions"]["historical_public_sources"][0]["source"])
        obs["projection"]["continuous_effects"][0]["source"] = captured
        with self.assertRaises(v6.FeatureSchemaError):
            v6.encode_decision(obs, actions)

    def test_public_graveyard_stack_target_reuses_live_or_keeps_detached_history(self):
        for detached in (False, True):
            obs = successor()
            target = copy.deepcopy(obs["projection"]["graveyards"][1][0]["stable"])
            obs["projection"]["stack"][0]["targets"] = [{"target_kind": "object", "object": target}]
            if detached:
                # The target left the public graveyard. Its hidden destination
                # and current incarnation are deliberately absent from input.
                obs["projection"]["graveyards"][1].clear()
            encoded = v6.encode_decision(obs, legal_actions())
            rows = [index for index, token in enumerate(encoded.object_card_ids.tolist()) if token == target["card_db_id"] + 1]
            self.assertEqual(len(rows), 1)
            group = "stack_target" if detached else "opponent_graveyard"
            self.assertEqual(encoded.object_groups[rows[0]].item(), v6.OBJECT_GROUPS.index(group))
            old = copy.deepcopy(obs)
            old["schema_version"] = 5
            del old["extensions"]
            with self.assertRaises(frozen.FeatureSchemaError):
                frozen.encode_decision(old, legal_actions())

    def test_graveyard_history_does_not_grant_hidden_zone_or_identity_forgery(self):
        for hidden_zone in ("Hand", "Library"):
            obs = successor()
            target = stable_ref(99, 32, "p1", hidden_zone)
            obs["projection"]["stack"][0]["targets"] = [{"target_kind": "object", "object": target}]
            with self.assertRaisesRegex(v6.FeatureSchemaError, "provenance"):
                v6.encode_decision(obs, legal_actions())
        for field, value in (("card_db_id", 999), ("owner", "p0"), ("zone", "Battlefield")):
            obs = successor()
            target = copy.deepcopy(obs["projection"]["graveyards"][1][0]["stable"])
            target[field] = value
            obs["projection"]["stack"][0]["targets"] = [{"target_kind": "object", "object": target}]
            with self.assertRaises(v6.FeatureSchemaError):
                v6.encode_decision(obs, legal_actions())

    def test_explicit_creature_choice_cost_retains_legacy_width_and_distinct_hash(self):
        obs, _ = escape_decision()
        obs["extensions"]["pending_cast_object_cost"] = None
        obs["projection"]["stack"][-1]["cast_method"] = "normal"
        obs["projection"]["engine_context"]["pending_cast"]["origin_zone"] = "Hand"
        source = obs["projection"]["engine_context"]["pending_cast"]["source"]
        obs["own_hand"][0]["stable"]["card_db_id"] = 20
        self.assertEqual(v6.COST_ONE_HOT_KINDS_V6, frozen.COST_KINDS)
        for candidate in (obs["projection"]["battlefield"][0][0]["stable"], obs["own_hand"][0]["stable"]):
            obs["extensions"]["pending_chosen_creature_cost"] = {
                "source": source, "controller": "p0", "selected_zone": candidate["zone"]}
            offer = action(0, {"action_kind": "choose_cost_target", "source": source,
                               "candidate": candidate, "cost_kind": "ChooseCreatureOrRevealCreature", "remaining": 1})
            encoded = v6.encode_decision(obs, [offer])
            self.assertEqual(encoded.action_features.shape, (1, 195))
            start = v6.ACTION_FEATURE_DIM - v6.ACTION_HASH_DIM - len(v6.OPTIONAL_COST_CHOICES) - 11
            self.assertTrue(torch.equal(encoded.action_features[0, start:start + 11], torch.zeros(11)))
            legacy = copy.deepcopy(offer)
            legacy["semantic"]["cost_kind"] = "SacrificeCreatures"
            old_category = v6.encode_decision(obs, [legacy])
            self.assertEqual(old_category.action_features[0, start:start + 11].sum().item(), 1.0)
            self.assertFalse(torch.equal(encoded.action_features[0, -v6.ACTION_HASH_DIM:],
                                         old_category.action_features[0, -v6.ACTION_HASH_DIM:]))
            with self.assertRaises(frozen.FeatureSchemaError):
                frozen.assert_action_classified(offer)
            unknown = copy.deepcopy(offer)
            unknown["semantic"]["cost_kind"] = "UnknownCreatureCost"
            with self.assertRaises(v6.FeatureSchemaError):
                v6.encode_decision(obs, [unknown])

    def test_pending_cast_option_uses_ordinary_source_without_historical_context(self):
        obs, _ = escape_decision()
        obs["extensions"]["pending_cast_object_cost"] = None
        obs["projection"]["stack"][-1]["cast_method"] = "normal"
        obs["projection"]["engine_context"]["pending_cast"]["origin_zone"] = "Hand"
        source = obs["projection"]["engine_context"]["pending_cast"]["source"]
        choices = [action(i, {"action_kind": "choose_effect_option", "source": source,
                              "option_index": i, "option_count": 2}) for i in range(2)]
        encoded = v6.encode_decision(obs, choices)
        self.assertEqual(encoded.action_features.shape, (2, 195))
        self.assertEqual(obs["extensions"]["historical_public_sources"], [])

    def test_goad_requires_include_at_each_attacker_prefix(self):
        for cursor in range(3):
            response = combat_decision_response("v6-goad", 1, cursor, cursor,
                                                selected_indices=tuple(range(cursor)))
            obs, pair = response["observation"], response["legal_actions"]
            obs["schema_version"] = 6
            obs["extensions"] = copy.deepcopy(successor()["extensions"])
            current = obs["projection"]["policy_surface_context"]["private_combat_selection"]["current_candidate"]
            card = next(card for card in obs["projection"]["battlefield"][0] if card["stable"] == current)
            card["goaded_by"] = [{"player": "p1", "expires_at_turn": obs["projection"]["turn"]}]
            include_only = [copy.deepcopy(pair[1])]
            include_only[0]["selected_index"] = 0
            encoded = v6.encode_decision(obs, include_only)
            self.assertEqual(encoded.action_features.shape[0], 1)
            with self.assertRaises(v6.FeatureSchemaError):
                v6.encode_decision(obs, pair)
            excluded = copy.deepcopy(include_only)
            excluded[0]["semantic"]["include"] = False
            with self.assertRaises(v6.FeatureSchemaError):
                v6.encode_decision(obs, excluded)
            old = copy.deepcopy(obs)
            old["schema_version"] = 5
            del old["extensions"]
            frozen.encode_decision(old, pair)
            with self.assertRaises(frozen.FeatureSchemaError):
                frozen.encode_decision(old, include_only)

    def test_singleton_goad_mask_requires_current_visible_active_goad(self):
        response = combat_decision_response("v6-goad-negative", 1, 0, 0)
        obs, pair = response["observation"], response["legal_actions"]
        obs["schema_version"] = 6
        obs["extensions"] = copy.deepcopy(successor()["extensions"])
        single = [copy.deepcopy(pair[1])]
        single[0]["selected_index"] = 0
        current = obs["projection"]["policy_surface_context"]["private_combat_selection"]["current_candidate"]
        card = next(card for card in obs["projection"]["battlefield"][0] if card["stable"] == current)
        for goads in ([], [{"player": "p0", "expires_at_turn": obs["projection"]["turn"]}]):
            card["goaded_by"] = goads
            v6.encode_decision(obs, pair)
            with self.assertRaises(v6.FeatureSchemaError):
                v6.encode_decision(obs, single)
        card["goaded_by"] = [{"player": "p0", "expires_at_turn": obs["projection"]["turn"] + 1}]
        v6.encode_decision(obs, single)

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
