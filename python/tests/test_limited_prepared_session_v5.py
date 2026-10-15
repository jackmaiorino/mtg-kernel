from __future__ import annotations

from copy import deepcopy
from pathlib import Path
import sys
import tempfile
import unittest

REPO_ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO_ROOT / "python/tools"))
from limited_prepared_session_v5 import LimitedPreparedClientV5, _validate_prepared_visible, _expected_prepared_meanings, COUNTERS, FORM
from limited_elenda_session_v4 import _expected_meaning
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
    reply = dict(protocol="kernel_limited_prepared_jsonl", schema_version=5,
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
                         decision=dict(schema="mtg-kernel-limited-prepared-decision/v5",
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
    if not error and reply["state"]["state_type"] == "decision":
        decision=reply["state"]["decision"]
        old=decision["state"]
        rules=EXPECTED
        battlefield=[]
        for cards in old["predecessor"]["public"]["battlefield"]:
            rows=[]
            for card in cards:
                ref=card["card"]["stable"]
                counters={key:0 for key in COUNTER_KEYS}
                counters.update(plus_one=card["plus_one_counters"],soul=card["soul"])
                form=dict(names=[ref["name"]],mana_value=0,types=["Land"],supertypes=["Basic"],
                          subtype_ids=[],color_mask=0,base_power=None,base_toughness=None,power=None,toughness=None,
                          keyword_bits=0,ward_generic=0,minimum_blockers=1,printed_abilities_active=True,abilities_removed=False)
                rows.append(dict(source=ref,face_index=0,is_token=False,tapped=False,summoning_sick=False,
                                 marked_damage=card["marked_damage"],counters=counters,form=form,temporary_boosts=[],attachments=[]))
            battlefield.append(rows)
        families=dict(drake=dict(meanings=rules["drake"],battlefield=battlefield,combat=[],activations=[]),
                      spinner=dict(meaning=rules["spinner"],current=[],held=[],stack=[]),
                      quandary=dict(viewer=decision["actor"],meaning=rules["quandary"],current=[],held=[],stack=[]),
                      phoenix=dict(meaning=rules["phoenix"],ferocious_now=[False,False],current=[],spells=[],triggers=[]))
        decision.update(state=dict(common=old,prepared=families),action_meanings=[None]*len(decision["actions"]),london_mulligans=None,foundations_combat=None)
    if reply.get("state",{}).get("state_type")=="decision" and mode in ("negative-incubation","negative-stun","negative-optional"):
        facts=reply["state"]["decision"]["state"]["prepared"]
        row=facts["drake"]["battlefield"][0][0]
        if mode=="negative-optional":
            current={key:copy.deepcopy(row[key]) for key in ("source","is_token","tapped","summoning_sick","marked_damage","counters","form")}
            current["counters"]["stun"]=-1
            facts["spinner"]["current"]=[current]
        else:
            row["counters"]["incubation" if mode=="negative-incubation" else "stun"]=-1
    if mode == "bad-receipt" and op == "step":
        reply["receipt"]["action_index"] = True
    if mode == "stuck-generation" and op == "restore":
        reply["state"]["authority_generation"] -= 1
    if mode == "bad-meaning" and reply.get("state", {}).get("state_type") == "decision":
        reply["state"]["decision"]["state"]["common"]["elenda"]["meaning"]["ordered_rules"].reverse()
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
        del reply["state"]["decision"]["state"]["common"]["predecessor"]["public"]["battlefield"][0][0]["soul"]
    elif mode == "forged-width":
        reply["state"]["decision"]["state"]["common"]["predecessor"]["public"]["battlefield"][1][0]["marked_damage"] = 1 << 32
    elif mode == "wrong-schema":
        reply["state"]["decision"]["schema"] = "old/v1"
    elif mode == "duplicate":
        print('{"protocol":"a","protocol":"b"}', flush=True)
        continue
    print(json.dumps(reply), flush=True)
'''


class LimitedPreparedClientV5Test(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.engine = Path(self.temp.name) / "engine.py"
        prelude="EXPECTED = " + repr(_expected_prepared_meanings()) + "\nCOUNTER_KEYS = " + repr(tuple(COUNTERS)) + "\n"
        self.engine.write_text(prelude + ENGINE, encoding="utf-8")
        self.decks = (parse_dck("20 Forest\n20 Forest\nSB: 1 Island"),
                      parse_dck("41 Island"))

    def tearDown(self):
        self.temp.cleanup()

    def client(self, mode="normal", **kwargs):
        return LimitedPreparedClientV5(
            [sys.executable, str(self.engine), mode],
            expected_card_db_hash=123, **kwargs)

    def test_v5_receipt_generation_and_ordered_meaning_corruption_close_transport(self):
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
                        original["state"]["decision"]["state"]["common"]["predecessor"]["public"]["battlefield"][1][0][field])))):
                    for replacement in replacements:
                        with self.subTest(field=field, seat=seat, replacement=replacement):
                            changed = deepcopy(original)
                            changed["state"]["decision"]["state"]["common"]["predecessor"]["public"]["battlefield"][seat][0][field] = replacement
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
                changed["state"]["decision"]["state"]["common"]["predecessor"]["public"]["battlefield"][0][0]["soul"] = replacement
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
                LimitedPreparedClientV5(["does-not-exist"], expected_card_db_hash=value)
        for value in (float("nan"), float("inf"), 0):
            with self.assertRaises(ValueError):
                LimitedPreparedClientV5(["does-not-exist"], expected_card_db_hash=123,
                                      timeout_s=value)



    def test_prepared_full_roster_native_widths_meaning_and_actor_privacy(self):
        with self.client() as client:
            reply=client.reset(self.decks)
        decision=reply["state"]["decision"]
        def validate(d):
            _validate_prepared_visible(d["state"],d["actor"],d["action_meanings"],d["actions"])
        validate(decision)
        row=decision["state"]["prepared"]["drake"]["battlefield"][0][0]
        row["counters"].update(incubation=65539,soul=65537,plus_one=65538,loyalty=(1<<32)-1,finality=65535)
        row["marked_damage"]=(1<<32)-1
        inherited=decision["state"]["common"]["predecessor"]["public"]["battlefield"][0][0]
        inherited.update(marked_damage=(1<<32)-1,soul=65537,plus_one_counters=65538)
        validate(decision)
        for path,value in ((["counters","incubation"],True),(["counters","soul"],65537.0),
                           (["counters","charge"],1<<31),(["counters","loyalty"],1<<32),
                           (["counters","stun"],1<<15),(["counters","finality"],1<<16),
                           (["marked_damage"],1<<32),(["form","power"],False),
                           (["form","printed_abilities_active"],1)):
            changed=deepcopy(decision)
            target=changed["state"]["prepared"]["drake"]["battlefield"][0][0]
            for key in path[:-1]:target=target[key]
            target[path[-1]]=value
            with self.subTest(path=path),self.assertRaises(LimitedSessionError):validate(changed)
        for mutation in ("missing","duplicate","unknown","printed","program","foreign-private","annotation"):
            changed=deepcopy(decision);facts=changed["state"]["prepared"]
            if mutation=="missing":facts["drake"]["battlefield"][0].clear()
            elif mutation=="duplicate":facts["drake"]["battlefield"][0].append(deepcopy(row))
            elif mutation=="unknown":facts["drake"]["battlefield"][0][0]["private_snapshot"]={}
            elif mutation=="printed":facts["drake"]["meanings"][0]["printed"]["generic_mana"]=True
            elif mutation=="program":facts["phoenix"]["meaning"]["ordered_rules"][-1]["GraveyardReturn"]["ordered_steps"].reverse()
            elif mutation=="foreign-private":
                ref={"handle":"private","name":"Secret","owner":"p1","controller":"p1","zone":"Hand"}
                root={"original_source":deepcopy(row["source"]),"current_original_source":None,"captured_controller":"p1"}
                facts["quandary"]["stack"]=[{"stack_position":0,"root":root,"stage":{"AwaitingPayment":{
                    "payer":"p1","minimum":0,"maximum":1,"payer_hand":{"original_hand":[ref],"selected":[]}}}}]
            else:changed["action_meanings"]=[]
            with self.subTest(mutation=mutation),self.assertRaises(LimitedSessionError):validate(changed)

    def test_issued_prepared_full_values_and_parallel_annotations_are_immutable(self):
        with self.client() as client:
            original=client.reset(self.decks)
            sent=client.next_request
            for mutation in ("incubation","meaning","annotation","missing-roster"):
                changed=deepcopy(original);d=changed["state"]["decision"]
                if mutation=="incubation":d["state"]["prepared"]["drake"]["battlefield"][0][0]["counters"]["incubation"]=True
                elif mutation=="meaning":d["state"]["prepared"]["phoenix"]["meaning"]["ordered_rules"].reverse()
                elif mutation=="annotation":d["action_meanings"].reverse();d["action_meanings"][0]={}
                else:d["state"]["prepared"]["drake"]["battlefield"][0].clear()
                with self.subTest(mutation=mutation),self.assertRaises(LimitedSessionError):client.step(changed,0)
                self.assertEqual(client.next_request,sent)
            client.step(original,0)

    def test_negative_counter_quantities_close_owned_process_before_any_step(self):
        for mode in ("negative-incubation","negative-stun","negative-optional"):
            with self.subTest(mode=mode),self.client(mode) as client:
                with self.assertRaises(LimitedSessionError):client.reset(self.decks)
                self.assertTrue(client.closed)

    def test_all_counter_quantities_are_nonnegative_but_power_and_boost_are_signed(self):
        with self.client() as client:decision=client.reset(self.decks)["state"]["decision"]
        def validate(d):_validate_prepared_visible(d["state"],d["actor"],d["action_meanings"],d["actions"])
        for field in COUNTERS:
            changed=deepcopy(decision)
            changed["state"]["prepared"]["drake"]["battlefield"][0][0]["counters"][field]=-1
            with self.subTest(field=field),self.assertRaises(LimitedSessionError):validate(changed)
        good=deepcopy(decision)
        row=good["state"]["prepared"]["drake"]["battlefield"][0][0]
        row["form"].update(base_power=-(1<<31),power=-1,toughness=(1<<31)-1)
        row["temporary_boosts"]=[{"binding":"OriginalIncarnation","power":-3,"toughness":-4,
                                  "grant_haste":False,"until_end_of_turn":True}]
        validate(good)


if __name__ == "__main__":
    unittest.main()
