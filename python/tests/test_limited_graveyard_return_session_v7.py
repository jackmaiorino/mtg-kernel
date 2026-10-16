"""Schema-7 protocol tests. All server worlds and special scenes are explicit mocks."""
from copy import deepcopy
from pathlib import Path
import sys
import tempfile
import unittest

REPO_ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO_ROOT / "python/tools"))
from limited_graveyard_return_session_v7 import (
    LimitedGraveyardReturnClientV7, _expected_graveyard_return_meanings,
    _validate_graveyard_return_visible, GR_COUNTERS,
)
from limited_prepared_session_v5 import _expected_prepared_meanings, COUNTERS
from limited_decks_v1 import parse_dck
from limited_session_v1 import LimitedSessionError
from test_limited_next_five_session_v6 import mock_facts, ref, root, INJECT as NEXT_INJECT
from test_limited_prepared_session_v5 import ENGINE as PREPARED_MOCK


def empty_candidate():
    return dict(drake_activation=None, programs=[], selected_graveyard_target=None,
                nine_lives_deaths=[], nine_lives_schedules=[])


INJECT = r'''
    if mode=="late-producer" and op=="step": reply["producer_sha256"]="b"*64
    if reply.get("state",{}).get("state_type")=="decision":
        d=reply["state"]["decision"]
        inherited=d["state"]["prepared"]["drake"]["battlefield"]
        counters=[[dict(card=copy.deepcopy(row["source"]),counters=dict(**row["counters"],revival=0)) for row in rows] for rows in inherited]
        d["state"]["graveyard_return"]=dict(viewer=d["actor"],meanings=copy.deepcopy(GR_MEANINGS),authorized=[],current=[],
            battlefield_counters=counters,casts=[],triggers=[],alesha_departures=[],nine_lives_deaths=[],nine_lives_schedules=[])
        d["action_meanings"]=[copy.deepcopy(EMPTY_CANDIDATE) for _ in d["actions"]]
        if mode=="bad-gr-viewer": d["state"]["graveyard_return"]["viewer"]="foreign"
        if mode=="bad-gr-meaning": d["state"]["graveyard_return"]["meanings"][0]["ordered_rules"].reverse()
        if mode=="missing-gr-annotations": d["action_meanings"]=[None]*len(d["actions"])
        if mode=="missing-revival": del counters[0][0]["counters"]["revival"]
        if mode=="old-v6": reply["protocol"]="kernel_limited_next_five_jsonl";reply["schema_version"]=6
'''


class LimitedGraveyardReturnClientV7Test(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.engine = Path(self.temp.name) / "mock_engine.py"
        source = PREPARED_MOCK.replace("kernel_limited_prepared_jsonl", "kernel_limited_graveyard_return_jsonl")
        source = source.replace("schema_version=5", "schema_version=7")
        source = source.replace("mtg-kernel-limited-prepared-decision/v5", "mtg-kernel-limited-graveyard-return-decision/v7")
        source = source.replace("    print(json.dumps(reply), flush=True)", NEXT_INJECT + INJECT + "\n    print(json.dumps(reply), flush=True)")
        prelude = "EXPECTED = " + repr(_expected_prepared_meanings()) + "\nCOUNTER_KEYS = " + repr(tuple(COUNTERS))
        prelude += "\nNEXT_FACTS = " + repr(mock_facts()) + "\nGR_MEANINGS = " + repr(_expected_graveyard_return_meanings())
        prelude += "\nEMPTY_CANDIDATE = " + repr(empty_candidate()) + "\n"
        self.engine.write_text(prelude + source, encoding="utf-8")
        self.decks = (parse_dck("40 Forest"), parse_dck("40 Island"))

    def tearDown(self):
        self.temp.cleanup()

    def client(self, mode="normal", **kwargs):
        return LimitedGraveyardReturnClientV7([sys.executable, str(self.engine), mode], expected_card_db_hash=123, **kwargs)

    def decision(self):
        with self.client() as client:
            return client.reset(self.decks)["state"]["decision"]

    def validate(self, d):
        _validate_graveyard_return_visible(d["state"], d["actor"], d["action_meanings"], d["actions"])

    def test_mock_both_actor_routes_complete_meanings_payment_receipt_and_terminal(self):
        for actor in ("p0", "p1"):
            with self.subTest(actor=actor), self.client() as client:
                reply = client.reset(self.decks, episode_id=7, starting_player=actor)
                d = reply["state"]["decision"]
                self.assertEqual(d["state"]["graveyard_return"]["viewer"], actor)
                self.assertEqual(len(d["state"]["next_five"]["meanings"]), 5)
                self.assertEqual(len(d["state"]["graveyard_return"]["meanings"]), 2)
                self.assertEqual(len(d["actions"]), len(d["action_meanings"]))
                end = client.step(reply, 1)
                self.assertEqual(end["receipt"], dict(prompt_seq=1, action_index=1))
                self.assertEqual(end["state"]["classification"], "natural")

    def test_mock_snapshot_restore_generation_and_whole_prompt_refuse_before_request(self):
        with self.client() as client:
            old = client.reset(self.decks)
            saved = client.snapshot()
            restored = client.restore(saved["snapshot_id"])
            self.assertGreater(restored["state"]["authority_generation"], old["state"]["authority_generation"])
            sent = client.next_request
            for changed in (old, deepcopy(restored)):
                if changed is not old:
                    changed["state"]["decision"]["action_meanings"][0]["programs"] = [{}]
                with self.assertRaises(LimitedSessionError):
                    client.step(changed, 0)
                self.assertEqual(client.next_request, sent)
            client.step(deepcopy(restored), 0)

    def test_mock_foreign_and_reset_snapshot_and_failed_reset_preserve_prompt(self):
        with self.client() as first, self.client() as second:
            old = first.reset(self.decks)
            saved = first.snapshot()
            second.reset(self.decks)
            with self.assertRaises(LimitedSessionError):
                second.restore(saved["snapshot_id"])
            with self.assertRaisesRegex(LimitedSessionError, "unsupported_deck"):
                first.reset((self.decks[0], parse_dck("40 Missing")))
            first.step(old, 0)
            first.restore(saved["snapshot_id"])
            first.reset(self.decks)
            with self.assertRaises(LimitedSessionError):
                first.restore(saved["snapshot_id"])

    def test_mock_corruption_closes_owned_process_and_old_v6_refuses(self):
        for mode in ("old-v6", "wrong-protocol", "wrong-request", "wrong-catalog", "wrong-catalog-error",
                     "wrong-schema", "schema2", "duplicate", "bad-index", "bad-actor", "foreign-hand",
                     "bad-next-viewer", "bad-next-root", "raw-next-id", "bad-episode", "extra-reply",
                     "bad-gr-viewer", "bad-gr-meaning", "missing-gr-annotations", "missing-revival"):
            with self.subTest(mode=mode), self.client(mode, timeout_s=5) as client:
                with self.assertRaises(LimitedSessionError):
                    client.reset(self.decks)
                self.assertTrue(client.closed)
                self.assertIsNotNone(client.process.poll())

    def test_mock_bad_receipt_generation_terminal_and_deadline(self):
        for mode, operation in (("bad-receipt", "step"), ("stuck-generation", "restore"),
                                ("forged-terminal", "step"), ("late-timeout", "step"), ("late-producer", "step")):
            with self.subTest(mode=mode), self.client(mode, timeout_s=5) as client:
                reply = client.reset(self.decks)
                saved = client.snapshot()
                if mode == "late-timeout":
                    client.timeout_s = 0.05
                with self.assertRaises(LimitedSessionError):
                    client.restore(saved["snapshot_id"]) if operation == "restore" else client.step(reply, 0)
                self.assertTrue(client.closed)

    def test_all_sixteen_counter_fields_exact_width_and_inherited_consistency(self):
        d = self.decision()
        self.validate(d)
        row = d["state"]["graveyard_return"]["battlefield_counters"][0][0]
        self.assertEqual(len(row["counters"]), 16)
        for key, spec in GR_COUNTERS.items():
            for value in (False, 0.0, spec[1]-1, spec[2]+1):
                changed = deepcopy(d)
                changed["state"]["graveyard_return"]["battlefield_counters"][0][0]["counters"][key] = value
                with self.subTest(key=key, value=value), self.assertRaises(LimitedSessionError):
                    self.validate(changed)
        row["counters"]["revival"] = (1 << 31)-1
        self.validate(d)
        row["counters"]["plus_one"] += 1
        with self.assertRaises(LimitedSessionError):
            self.validate(d)

    def raid(self, departure=False):
        d = self.decision()
        source = ref("alesha-old", "Alesha, Who Laughs at Fate")
        target = ref("grave-target", "Known creature", zone="Graveyard")
        program = dict(AleshaRaid=dict(power=dict(amount=4, source="ExactDepartureLki" if departure else "CurrentOriginalIncarnation"),
            captured_controller_attacked_this_turn=True, may_return_at_resolution=True, optional_choice="AwaitingCostlessReturnChoice"))
        if departure:
            r = root(source)
        else:
            source = deepcopy(d["state"]["prepared"]["drake"]["battlefield"][0][0]["source"])
            r = root(source)
            r["current_original_source"] = deepcopy(source)
        selected = dict(original=target, current_exact_graveyard_card=deepcopy(target), legal_now=True)
        trigger = dict(family="Alesha", stage="Resolving", position=0, source=r, program=program, targets=[selected])
        d["state"]["graveyard_return"]["triggers"] = [trigger]
        for a in d["action_meanings"]:
            a["programs"] = [deepcopy(trigger)]
        d["action_meanings"][0]["selected_graveyard_target"] = deepcopy(selected)
        return d

    def test_raid_exact_target_current_vs_departure_power_and_resolution_may(self):
        for departure in (False, True):
            d = self.raid(departure)
            self.validate(d)
            for field, value in (("may_return_at_resolution", False), ("optional_choice", {}),
                                 ("power", dict(amount=1 << 31, source="ExactDepartureLki"))):
                changed = deepcopy(d)
                changed["state"]["graveyard_return"]["triggers"][0]["program"]["AleshaRaid"][field] = value
                with self.subTest(departure=departure, field=field), self.assertRaises(LimitedSessionError):
                    self.validate(changed)
            changed = deepcopy(d)
            changed["action_meanings"][0]["selected_graveyard_target"]["current_exact_graveyard_card"] = None
            with self.assertRaises(LimitedSessionError):
                self.validate(changed)

    def test_nine_lives_death_and_schedule_formula_and_candidate_alignment(self):
        d = self.decision()
        source = root(ref("dead", "Nine-Lives Familiar"))
        death = dict(occurrence=0, source=source, departure_revival=8, return_base_before_replacements=7,
                     collection_completed=True, exact_graveyard_card_now=ref("grave", "Nine-Lives Familiar", zone="Graveyard"))
        schedule = {k:v for k,v in death.items() if k != "collection_completed"}
        schedule["next_future_any_player_end_step"] = True
        f = d["state"]["graveyard_return"]
        f["nine_lives_deaths"] = [death]
        f["nine_lives_schedules"] = [schedule]
        for row in d["action_meanings"]:
            row["nine_lives_deaths"] = deepcopy(f["nine_lives_deaths"])
            row["nine_lives_schedules"] = deepcopy(f["nine_lives_schedules"])
        self.validate(d)
        for key, value in (("return_base_before_replacements", 8), ("departure_revival", 0),
                           ("next_future_any_player_end_step", False)):
            changed = deepcopy(d)
            changed["state"]["graveyard_return"]["nine_lives_schedules"][0][key] = value
            with self.subTest(key=key), self.assertRaises(LimitedSessionError):
                self.validate(changed)
        changed = deepcopy(d)
        changed["action_meanings"].pop()
        with self.assertRaises(LimitedSessionError):
            self.validate(changed)
        changed = deepcopy(d)
        changed["action_meanings"][0]["nine_lives_schedules"].clear()
        with self.assertRaises(LimitedSessionError):
            self.validate(changed)

    def test_closed_complete_meanings_no_printing_hash_or_private_authority(self):
        d = self.decision()
        for family in (0, 1):
            for mutation in ("order", "missing", "type", "private"):
                changed = deepcopy(d)
                meaning = changed["state"]["graveyard_return"]["meanings"][family]
                if mutation == "order": meaning["ordered_rules"].reverse()
                elif mutation == "missing": del meaning["printed"]["colored_mana"]
                elif mutation == "type": meaning["printed"]["mana_value"] = 3.0
                else: meaning["producer_sha256"] = "a"*64
                with self.subTest(family=family, mutation=mutation), self.assertRaises(LimitedSessionError):
                    self.validate(changed)

    def test_shared_references_across_actions_and_state_refuse_raw_and_alias_forgery(self):
        d = self.raid(True)
        for mutation in ("raw", "alias", "unknown"):
            changed = deepcopy(d)
            r = changed["action_meanings"][0]["selected_graveyard_target"]["original"]
            if mutation == "raw": r["arena_id"] = 7
            elif mutation == "alias": r["name"] = "Different identity"
            else: changed["action_meanings"][0]["pending_world"] = {}
            with self.subTest(mutation=mutation), self.assertRaises(LimitedSessionError):
                self.validate(changed)

    def test_drake_annotation_retains_exact_predecessor_cost_contract(self):
        d = self.decision()
        source = d["state"]["prepared"]["drake"]["battlefield"][0][0]["source"]
        d["action_meanings"][0]["drake_activation"] = dict(source=source, kind="Incubation", count=3,
            before=9, after=6, any_priority=True, costs_paid_before_stack=True, countering_refunds_costs=False)
        d["actions"][0]["label"] = "Activate Forest: remove 3 incubation counters (9 to 6); create a 2/2 blue Drake token with flying"
        # The inherited validator also authenticates the current counter source.
        with self.assertRaises(LimitedSessionError):
            self.validate(d)
        d["state"]["prepared"]["drake"]["battlefield"][0][0]["counters"]["incubation"] = 9
        d["state"]["graveyard_return"]["battlefield_counters"][0][0]["counters"]["incubation"] = 9
        self.validate(d)
        d["action_meanings"][0]["drake_activation"]["countering_refunds_costs"] = True
        with self.assertRaises(LimitedSessionError):
            self.validate(d)


if __name__ == "__main__":
    unittest.main()
