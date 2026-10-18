from __future__ import annotations

from copy import deepcopy
from pathlib import Path
import sys
import tempfile
import unittest

REPO_ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO_ROOT / "python/tools"))
from limited_elenda_session_v4 import LimitedElendaClientV4, _validate_battlefield, _validate_elenda_visible
from limited_decks_v1 import parse_dck
from limited_session_v1 import LimitedSessionError

ENGINE = r'''
import copy, json, sys, time
mode = sys.argv[1]
MEANING = {'root': 'CreatureSpellAndPrintedStatics', 'printed': {'generic_mana': 2, 'colored_pips': ['W', 'B'], 'types': ['Creature'], 'subtypes': ['Vampire', 'Knight'], 'supertypes': ['Legendary'], 'colors': ['W', 'B'], 'power': 4, 'toughness': 4}, 'home': 'BattlefieldPrintedFrontFace', 'active_printed_ability_required': True, 'evaluation': 'CurrentControllerAndWorldOnEveryQuery', 'supported_starting_life': 20, 'bonus_combination': 'AddEverySatisfiedSelfBonus', 'ordered_rules': [{'Keyword': 'Lifelink'}, {'HexproofFromSourceType': {'targeter': 'OpponentOfCurrentController', 'required_source_type': 'Instant', 'source_type_basis': 'EffectiveSelectedFormOrOriginalSourceCharacteristics', 'ordered_checks': ['TargetSelection', 'ResolutionRevalidation']}}, {'ConditionalSelfBonus': {'condition': {'CurrentControllerLifeAtLeastTheirStartingLifePlus': {'amount': 1}}, 'ordered_grants': [{'ModifyPowerToughness': {'power': 1, 'toughness': 1}}, {'GainKeyword': 'Menace'}]}}, {'ConditionalSelfBonus': {'condition': {'CurrentControllerLifeAtLeastTheirStartingLifePlus': {'amount': 10}}, 'ordered_grants': [{'ModifyPowerToughness': {'power': 5, 'toughness': 5}}]}}]}
generation = 0
prompt = 0
episode = 0
state = {"state_type": "unsupported", "episode_id": 0, "code": "no_session"}
snapshots = {}
for line in sys.stdin:
    request = json.loads(line)
    op = request["request_type"]
    reply = dict(protocol="kernel_limited_elenda_jsonl", schema_version=4,
                 request_id=request["request_id"], card_db_hash=123,
                 priority_mode="engine_windows_v1", combat_rules="foundations_v1",
                 mulligan_rules="london_v1", response_type="state")
    error = None
    if op == "reset":
        if request["expected_card_db_hash"] != 123:
            error = "catalog_mismatch"
        elif any(row["name"] == "Missing" for d in request["decks"] for row in d["cards"]):
            error = "unsupported_deck"
        else:
            episode = request["episode_id"]
            generation += 1
            prompt += 1
            snapshots.clear()
            state = dict(state_type="decision", episode_id=episode,
                         authority_generation=generation,
                         decision=dict(schema="mtg-kernel-limited-elenda-decision/v4",
                                       actor=request["starting_player"], prompt_seq=prompt,
                                       state={"own_hand": [], "public": {"battlefield": [
                                           [{"card": {"stable": {"handle": "c1", "name": "Forest", "owner": "p0", "controller": "p0", "zone": "Battlefield"}}, "soul": 0, "plus_one_counters": 0, "marked_damage": 0}],
                                           [{"card": {"stable": {"handle": "c2", "name": "Island", "owner": "p1", "controller": "p1", "zone": "Battlefield"}}, "soul": (1 << 31) - 1, "plus_one_counters": 32768, "marked_damage": 65536}]]}},
                                       actions=[{"action_index": 0, "label": "Keep this hand"},
                                                {"action_index": 1, "label": "Mulligan"}]))
    elif op == "step":
        d = state["decision"]
        if (request["episode_id"], request["authority_generation"], request["actor"],
            request["prompt_seq"]) != (episode, generation, d["actor"], prompt):
            error = "stale_prompt"
        elif mode == "unsupported":
            state = dict(state_type="unsupported", episode_id=episode,
                         code="unsupported_common_input")
        else:
            state = dict(state_type="terminal", episode_id=episode, classification=mode
                         if mode in ("truncated", "halted") else "natural",
                         code="decision_cap" if mode == "truncated" else
                         ("fail_closed" if mode == "halted" else "natural_game_over"),
                         winner=None, policy_steps=1, physical_decisions=2)
    elif op == "snapshot":
        snapshots[len(snapshots) + 1] = copy.deepcopy(state)
        reply["snapshot_id"] = len(snapshots)
    elif op == "restore":
        state = copy.deepcopy(snapshots[request["snapshot_id"]])
        generation += 1
        prompt += 1
        state["authority_generation"] = generation
        state["decision"]["prompt_seq"] = prompt
    if error:
        reply.update(response_type="error", code=error)
    else:
        reply["state"] = copy.deepcopy(state)
    if not error and reply["state"]["state_type"] == "terminal":
        reply["state"]["outcome"] = "truncated" if mode == "truncated" else ("halted" if mode == "halted" else "draw")
    reply.update(kernel_version="fixture-transport", card_count=446, producer_sha256="a" * 64)
    if not error and op == "step":
        reply["receipt"] = dict(prompt_seq=request["prompt_seq"], action_index=request["action_index"])
    if not error and reply["state"]["state_type"] == "decision":
        reply["state"]["decision"]["state"] = {"predecessor": reply["state"]["decision"]["state"], "elenda": {"meaning": MEANING, "current": []}}
    if mode == "bad-receipt" and op == "step":
        reply["receipt"]["action_index"] = True
    if mode == "stuck-generation" and op == "restore":
        reply["state"]["authority_generation"] -= 1
    if mode == "bad-meaning" and reply.get("state", {}).get("state_type") == "decision":
        reply["state"]["decision"]["state"]["elenda"]["meaning"]["ordered_rules"].reverse()
    if mode == "timeout":
        time.sleep(2)
    if mode == "wrong-protocol":
        reply["protocol"] = "kernel_limited_jsonl"
    elif mode == "wrong-request":
        reply["request_id"] = "foreign"
    elif mode == "wrong-catalog":
        reply["card_db_hash"] = 124
    elif mode == "wrong-catalog-error":
        reply.update(card_db_hash=124, response_type="error", code="unsupported_deck")
    elif mode == "forged-terminal" and op == "step":
        reply["state"]["code"] = "fail_closed"
    elif mode == "bad-actor":
        reply["state"]["decision"]["actor"] = "A"
    elif mode == "bad-index":
        reply["state"]["decision"]["actions"][1]["action_index"] = 0
    elif mode == "schema2":
        reply["schema_version"] = 2
    elif mode == "missing-exact":
        del reply["state"]["decision"]["state"]["predecessor"]["public"]["battlefield"][0][0]["soul"]
    elif mode == "forged-width":
        reply["state"]["decision"]["state"]["predecessor"]["public"]["battlefield"][1][0]["marked_damage"] = 1 << 32
    elif mode == "wrong-schema":
        reply["state"]["decision"]["schema"] = "old/v1"
    elif mode == "duplicate":
        print('{"protocol":"a","protocol":"b"}', flush=True)
        continue
    print(json.dumps(reply), flush=True)
'''


class LimitedElendaClientV4Test(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.engine = Path(self.temp.name) / "engine.py"
        self.engine.write_text(ENGINE, encoding="utf-8")
        self.decks = (parse_dck("20 Forest\n20 Forest\nSB: 1 Island"),
                      parse_dck("41 Island"))

    def tearDown(self):
        self.temp.cleanup()

    def client(self, mode="normal", **kwargs):
        return LimitedElendaClientV4(
            [sys.executable, str(self.engine), mode],
            expected_card_db_hash=123, **kwargs)

    def test_v4_receipt_generation_and_ordered_meaning_corruption_close_transport(self):
        for mode, operation in (("bad-receipt", "step"), ("stuck-generation", "restore")):
            with self.subTest(mode=mode), self.client(mode) as client:
                reply = client.reset(self.decks)
                saved = client.snapshot()
                with self.assertRaises(LimitedSessionError):
                    client.step(reply, 0) if operation == "step" else client.restore(saved["snapshot_id"])
                self.assertTrue(client.closed)
        with self.client("bad-meaning") as client:
            with self.assertRaises(LimitedSessionError):
                client.reset(self.decks)
            self.assertTrue(client.closed)

    def test_current_elenda_traits_exact_types_safe_refs_and_immutable_order(self):
        meaning = {'root': 'CreatureSpellAndPrintedStatics', 'printed': {'generic_mana': 2, 'colored_pips': ['W', 'B'], 'types': ['Creature'], 'subtypes': ['Vampire', 'Knight'], 'supertypes': ['Legendary'], 'colors': ['W', 'B'], 'power': 4, 'toughness': 4}, 'home': 'BattlefieldPrintedFrontFace', 'active_printed_ability_required': True, 'evaluation': 'CurrentControllerAndWorldOnEveryQuery', 'supported_starting_life': 20, 'bonus_combination': 'AddEverySatisfiedSelfBonus', 'ordered_rules': [{'Keyword': 'Lifelink'}, {'HexproofFromSourceType': {'targeter': 'OpponentOfCurrentController', 'required_source_type': 'Instant', 'source_type_basis': 'EffectiveSelectedFormOrOriginalSourceCharacteristics', 'ordered_checks': ['TargetSelection', 'ResolutionRevalidation']}}, {'ConditionalSelfBonus': {'condition': {'CurrentControllerLifeAtLeastTheirStartingLifePlus': {'amount': 1}}, 'ordered_grants': [{'ModifyPowerToughness': {'power': 1, 'toughness': 1}}, {'GainKeyword': 'Menace'}]}}, {'ConditionalSelfBonus': {'condition': {'CurrentControllerLifeAtLeastTheirStartingLifePlus': {'amount': 10}}, 'ordered_grants': [{'ModifyPowerToughness': {'power': 5, 'toughness': 5}}]}}]}
        source = {"handle": "c1", "name": "Elenda, Saint of Dusk", "owner": "p0", "controller": "p0", "zone": "Battlefield"}
        traits = {"token": False, "effective_name": "Elenda, Saint of Dusk", "effective_types": ["Creature"],
                  "effective_supertypes": ["Legendary"], "effective_subtype_ids": [1, 2], "effective_color_mask": 3,
                  "starting_life": 20, "controller_life": 30, "printed_abilities_active": True,
                  "ordered_bonuses": [{"minimum_above_start": 1, "condition_satisfied": True, "applied": True, "power": 1, "toughness": 1, "grants_menace": True},
                                      {"minimum_above_start": 10, "condition_satisfied": True, "applied": True, "power": 5, "toughness": 5, "grants_menace": False}],
                  "applied_power_bonus": 6, "applied_toughness_bonus": 6, "effective_power": 10, "effective_toughness": 10,
                  "effective_lifelink": True, "effective_menace": True, "hexproof_from_opponents_instants": True}
        visible = {"predecessor": {"public": {"battlefield": [[{"card": {"stable": source}, "marked_damage": 70000, "plus_one_counters": 65538, "soul": 65537}], []]}},
                   "elenda": {"meaning": meaning, "current": [{"source": deepcopy(source), "traits": traits}]}}
        _validate_elenda_visible(visible)
        for field, value in (("token", 0), ("effective_lifelink", 1), ("controller_life", True), ("effective_power", 10.0), ("applied_toughness_bonus", 1 << 31), ("starting_life", 21), ("effective_subtype_ids", [True]), ("effective_types", ["unknown"])):
            changed = deepcopy(visible); changed["elenda"]["current"][0]["traits"][field] = value
            with self.subTest(field=field), self.assertRaises(LimitedSessionError):
                _validate_elenda_visible(changed)
        for change in ("reference", "bonus-order", "meaning-type", "private-field"):
            changed = deepcopy(visible)
            if change == "reference": changed["elenda"]["current"][0]["source"]["handle"] = "foreign"
            elif change == "bonus-order": changed["elenda"]["current"][0]["traits"]["ordered_bonuses"].reverse()
            elif change == "meaning-type": changed["elenda"]["meaning"]["supported_starting_life"] = 20.0
            else: changed["elenda"]["current"][0]["traits"]["private_snapshot"] = {}
            with self.subTest(change=change), self.assertRaises(LimitedSessionError):
                _validate_elenda_visible(changed)
        # A genuine nameless copied neutral face has a valid empty printed name.
        empty = deepcopy(visible); empty["predecessor"]["public"]["battlefield"][0][0]["card"]["stable"]["name"] = ""
        _validate_battlefield(empty["predecessor"])

    def test_reset_step_binds_the_actual_actor_and_terminal(self):
        with self.client() as client:
            reply = client.reset(self.decks, episode_id=7, starting_player="p1")
            self.assertEqual(reply["state"]["decision"]["actor"], "p1")
            terminal = client.step(reply, 0)
            self.assertEqual(terminal["state"]["classification"], "natural")
            with self.assertRaisesRegex(LimitedSessionError, "unavailable"):
                client.step(reply, 0)

    def test_restore_invalidates_old_prompt_even_when_visible_menu_is_identical(self):
        with self.client() as client:
            original = client.reset(self.decks)
            snapshot = client.snapshot()
            client.step(original, 0)
            restored = client.restore(snapshot["snapshot_id"])
            self.assertEqual(original["state"]["decision"]["actions"],
                             restored["state"]["decision"]["actions"])
            sent = client.next_request
            with self.assertRaisesRegex(LimitedSessionError, "stale"):
                client.step(original, 0)
            self.assertEqual(client.next_request, sent)
            self.assertEqual(client.step(restored, 0)["state"]["state_type"], "terminal")

    def test_failed_reset_preserves_current_prompt_and_snapshots(self):
        with self.client() as client:
            original = client.reset(self.decks)
            snapshot = client.snapshot()
            with self.assertRaisesRegex(LimitedSessionError, "unsupported_deck"):
                client.reset((self.decks[0], parse_dck("40 Missing")))
            self.assertFalse(client.closed)
            self.assertEqual(client.step(original, 0)["state"]["state_type"], "terminal")
            client.restore(snapshot["snapshot_id"])

    def test_reset_invalidates_snapshot_handles_before_any_restore_request(self):
        with self.client() as client:
            client.reset(self.decks)
            snapshot = client.snapshot()
            client.reset(self.decks)
            sent = client.next_request
            with self.assertRaisesRegex(LimitedSessionError, "current game"):
                client.restore(snapshot["snapshot_id"])
            self.assertEqual(client.next_request, sent)

    def test_mutated_reply_cannot_change_the_private_current_prompt(self):
        with self.client() as client:
            original = client.reset(self.decks)
            changed = deepcopy(original)
            changed["state"]["decision"]["actor"] = "p1"
            changed["state"]["decision"]["actions"][0]["label"] = "Forged choice"
            sent = client.next_request
            with self.assertRaisesRegex(LimitedSessionError, "stale"):
                client.step(changed, 0)
            self.assertEqual(client.next_request, sent)
            client.step(original, 0)

    def test_issued_prompt_numeric_types_cannot_change_before_transport(self):
        with self.client() as client:
            original = client.reset(self.decks)
            sent = client.next_request
            for field in ("soul", "plus_one_counters", "marked_damage"):
                for seat, replacements in ((0, (False, 0.0)), (1, (True, float(
                        original["state"]["decision"]["state"]["predecessor"]["public"]["battlefield"][1][0][field])))):
                    for replacement in replacements:
                        with self.subTest(field=field, seat=seat, replacement=replacement):
                            changed = deepcopy(original)
                            changed["state"]["decision"]["state"]["predecessor"]["public"]["battlefield"][seat][0][field] = replacement
                            with self.assertRaisesRegex(LimitedSessionError, "stale"):
                                client.step(changed, 0)
                            self.assertEqual(client.next_request, sent)
            for action, replacement in ((0, False), (0, 0.0), (1, True), (1, 1.0)):
                changed = deepcopy(original)
                changed["state"]["decision"]["actions"][action]["action_index"] = replacement
                with self.subTest(action=action, replacement=replacement):
                    with self.assertRaisesRegex(LimitedSessionError, "stale"):
                        client.step(changed, 0)
                    self.assertEqual(client.next_request, sent)
            changed = deepcopy(original)
            self.assertEqual(changed["state"]["decision"]["prompt_seq"], 1)
            changed["state"]["decision"]["prompt_seq"] = True
            with self.assertRaisesRegex(LimitedSessionError, "stale"):
                client.step(changed, 0)
            self.assertEqual(client.next_request, sent)
            self.assertFalse(client.closed)

            def reordered(value):
                if type(value) is dict:
                    return {key: reordered(item) for key, item in reversed(list(value.items()))}
                if type(value) is list:
                    return [reordered(item) for item in value]
                return value

            # Deep copies and JSON object key order preserve the issued prompt.
            valid = reordered(deepcopy(original))
            self.assertEqual(client.step(valid, 0)["state"]["state_type"], "terminal")

    def test_non_json_issued_prompt_values_refuse_without_sending(self):
        with self.client() as client:
            original = client.reset(self.decks)
            sent = client.next_request
            for replacement in (object(), set(), (), float("nan"), float("inf"), {0: "value"}):
                changed = deepcopy(original)
                changed["state"]["decision"]["state"]["predecessor"]["public"]["battlefield"][0][0]["soul"] = replacement
                with self.subTest(replacement=type(replacement).__name__):
                    with self.assertRaisesRegex(LimitedSessionError, "stale"):
                        client.step(changed, 0)
                    self.assertEqual(client.next_request, sent)
            changed = deepcopy(original)
            changed["state"]["decision"][False] = changed["state"]["decision"].pop("prompt_seq")
            with self.assertRaisesRegex(LimitedSessionError, "stale"):
                client.step(changed, 0)
            self.assertEqual(client.next_request, sent)
            self.assertFalse(client.closed)
            client.step(deepcopy(original), 0)

    def test_invalid_action_indices_send_nothing(self):
        with self.client() as client:
            original = client.reset(self.decks)
            sent = client.next_request
            for index in (-1, 2, True, 0.0):
                with self.subTest(index=index):
                    with self.assertRaises(LimitedSessionError):
                        client.step(original, index)
            self.assertEqual(client.next_request, sent)
            client.step(original, 0)

    def test_unsupported_is_preserved_without_applying_a_fallback(self):
        with self.client("unsupported") as client:
            reply = client.step(client.reset(self.decks), 0)
            self.assertEqual(reply["state"]["code"], "unsupported_common_input")
            sent = client.next_request
            with self.assertRaisesRegex(LimitedSessionError, "unavailable"):
                client.step(reply, 0)
            self.assertEqual(client.next_request, sent)
            self.assertFalse(client.closed)

    def test_truncated_and_halted_terminals_are_not_natural_games(self):
        for mode in ("truncated", "halted"):
            with self.subTest(mode=mode), self.client(mode) as client:
                reply = client.step(client.reset(self.decks), 0)
                self.assertEqual(reply["state"]["classification"], mode)

    def test_protocol_corruption_and_timeout_close_owned_process(self):
        for mode in ("wrong-protocol", "wrong-request", "wrong-catalog", "wrong-catalog-error", "bad-actor",
                     "bad-index", "wrong-schema", "schema2", "missing-exact", "forged-width", "duplicate", "timeout"):
            with self.subTest(mode=mode), self.client(
                    mode, timeout_s=0.1 if mode == "timeout" else 5) as client:
                with self.assertRaises(LimitedSessionError):
                    client.reset(self.decks)
                self.assertTrue(client.closed)
                self.assertIsNotNone(client.process.poll())

    def test_contradictory_terminal_cannot_be_reported_as_a_natural_game(self):
        with self.client("forged-terminal") as client:
            reply = client.reset(self.decks)
            with self.assertRaisesRegex(LimitedSessionError, "classification/code"):
                client.step(reply, 0)
            self.assertTrue(client.closed)

    def test_invalid_constructor_arguments_do_not_launch(self):
        for value in (True, -1, 1 << 64):
            with self.assertRaises(LimitedSessionError):
                LimitedElendaClientV4(["does-not-exist"], expected_card_db_hash=value)
        for value in (float("nan"), float("inf"), 0):
            with self.assertRaises(ValueError):
                LimitedElendaClientV4(["does-not-exist"], expected_card_db_hash=123,
                                      timeout_s=value)


    def test_exact_roster_requires_every_field_and_native_integer_bounds(self):
        def visible():
            return {"public": {"battlefield": [[{
                "card": {"stable": {"handle": "c1", "name": "Artifact", "owner": "p0", "controller": "p0", "zone": "Battlefield"}},
                "soul": 0, "plus_one_counters": 0, "marked_damage": 0,
            }], []]}}
        good = visible()
        _validate_battlefield(good)
        limits = {"soul": (0, (1 << 31) - 1),
                  "plus_one_counters": (0, (1 << 31) - 1),
                  "marked_damage": (0, (1 << 32) - 1)}
        for field, (low, high) in limits.items():
            for value in (low, high, 32768, 65536):
                changed = visible()
                changed["public"]["battlefield"][0][0][field] = value
                _validate_battlefield(changed)
            for value in (None, True, False, 0.0, "0", low - 1, high + 1):
                with self.subTest(field=field, value=value):
                    changed = visible()
                    changed["public"]["battlefield"][0][0][field] = value
                    with self.assertRaises(LimitedSessionError):
                        _validate_battlefield(changed)
            changed = visible()
            del changed["public"]["battlefield"][0][0][field]
            with self.assertRaises(LimitedSessionError):
                _validate_battlefield(changed)
        for roster in (None, [], [[]], [[], [], []], [[], None], [[], [False]]):
            with self.subTest(roster=roster), self.assertRaises(LimitedSessionError):
                _validate_battlefield({"public": {"battlefield": roster}})
        for field, value in (("handle", ""), ("controller", "p1"), ("owner", "x"), ("zone", "Hand")):
            changed = visible()
            changed["public"]["battlefield"][0][0]["card"]["stable"][field] = value
            with self.assertRaises(LimitedSessionError):
                _validate_battlefield(changed)
        changed = visible()
        changed["public"]["battlefield"][0].append(deepcopy(changed["public"]["battlefield"][0][0]))
        with self.assertRaises(LimitedSessionError):
            _validate_battlefield(changed)


if __name__ == "__main__":
    unittest.main()
