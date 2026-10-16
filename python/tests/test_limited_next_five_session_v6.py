"""Schema-6 protocol tests. Every server/game state here is an explicit mock."""
from __future__ import annotations

from copy import deepcopy
from pathlib import Path
import sys
import tempfile
import unittest

REPO_ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO_ROOT / "python/tools"))
from limited_next_five_session_v6 import (
    LimitedNextFiveClientV6, _validate_next_five_visible, _expected_next_five_meanings, _validate_play_context,
)
from limited_prepared_session_v5 import _expected_prepared_meanings, COUNTERS
from limited_decks_v1 import parse_dck
from limited_session_v1 import LimitedSessionError
from test_limited_prepared_session_v5 import ENGINE as PREPARED_MOCK


def mock_facts(actor="p0"):
    meaning=_expected_next_five_meanings()
    return {"viewer":actor,"meanings":meaning,"snack":{
        "viewer":actor,"meaning":deepcopy(meaning[0]["ordered_rules"][0]["Snack"]["meaning"]),
        "attacked_this_turn":[False,False],"gained_this_turn":[0,0],"authorized_printings":[],
        "current":[],"current_food":[],"casts":[],"held_raid":[],"published_raid":[],"activations":[]},
        "authorized":[],"current":[],"casts":[],"triggers":[],"aura_copy":None,"infernal_deaths":[],
        "angel_replacements":[],"angel_mana":[[],[]],"goblin_quotes":[],"goblin_ordering":None,
        "goblin_ordering_options":[],"goblin_completed":[]}


def ref(handle,name="Mock public source",owner="p0",controller="p0",zone="Battlefield"):
    return dict(handle=handle,name=name,owner=owner,controller=controller,zone=zone)


def root(source):
    return {"original_source":source,"current_original_source":None,"captured_controller":source["controller"]}


# Reuse only the existing explicitly mocked inherited layout. New V59 facts are
# appended before encoding; none of these bytes are native-engine evidence.
INJECT = r'''
    if reply.get("state",{}).get("state_type")=="decision":
        d=reply["state"]["decision"]
        f=copy.deepcopy(NEXT_FACTS)
        f["viewer"]=f["snack"]["viewer"]=d["actor"]
        d["state"]["next_five"]=f
        if mode=="bad-next-viewer": f["viewer"]="p1" if d["actor"]=="p0" else "p0"
        if mode=="bad-next-root": f["meanings"][4]["ordered_rules"][0]["Cast"]["ordered"].reverse()
        if mode=="raw-next-id": f["authorized"]=[["Goblin",dict(handle="c9",name="Goblin",owner="p0",controller="p0",zone="Hand",arena_id=9)]]
        if mode=="bad-episode": reply["state"]["episode_id"]+=1
        if mode=="foreign-hand": d["state"]["common"]["predecessor"]["own_hand"]=[dict(stable=dict(handle="c9",name="Secret",owner="p1",controller="p1",zone="Hand"))]
    if mode=="late-timeout" and op=="step": time.sleep(2)
    if mode=="extra-reply": reply["private_world"]={}
'''


class LimitedNextFiveClientV6Test(unittest.TestCase):
    def setUp(self):
        self.temp=tempfile.TemporaryDirectory()
        self.engine=Path(self.temp.name)/"mock_engine.py"
        source=PREPARED_MOCK.replace("kernel_limited_prepared_jsonl","kernel_limited_next_five_jsonl").replace("schema_version=5","schema_version=6").replace("mtg-kernel-limited-prepared-decision/v5","mtg-kernel-limited-next-five-decision/v6")
        source=source.replace("    print(json.dumps(reply), flush=True)",INJECT+"\n    print(json.dumps(reply), flush=True)")
        prelude="EXPECTED = "+repr(_expected_prepared_meanings())+"\nCOUNTER_KEYS = "+repr(tuple(COUNTERS))+"\nNEXT_FACTS = "+repr(mock_facts())+"\n"
        self.engine.write_text(prelude+source,encoding="utf-8")
        self.decks=(parse_dck("40 Forest"),parse_dck("40 Island"))
    def tearDown(self):
        self.temp.cleanup()
    def client(self,mode="normal",**kwargs):
        return LimitedNextFiveClientV6([sys.executable,str(self.engine),mode],expected_card_db_hash=123,**kwargs)
    def visible(self):
        with self.client() as client:
            return client.reset(self.decks)["state"]["decision"]["state"]
    def validate(self,visible,actor="p0"):
        _validate_next_five_visible(visible,actor,[None,None],[{},{}])

    def test_mock_protocol_actor_payment_receipt_and_terminal(self):
        for actor in ("p0","p1"):
            with self.subTest(actor=actor), self.client() as client:
                reply=client.reset(self.decks,episode_id=7,starting_player=actor)
                self.assertEqual(reply["state"]["decision"]["state"]["next_five"]["viewer"],actor)
                end=client.step(reply,1)
                self.assertEqual(end["receipt"],{"prompt_seq":1,"action_index":1})
                self.assertEqual(end["state"]["classification"],"natural")
                with self.assertRaises(LimitedSessionError): client.step(reply,1)

    def test_mock_restore_new_generation_and_whole_issued_prompt(self):
        with self.client() as client:
            old=client.reset(self.decks)
            saved=client.snapshot()
            restored=client.restore(saved["snapshot_id"])
            self.assertGreater(restored["state"]["authority_generation"],old["state"]["authority_generation"])
            sent=client.next_request
            with self.assertRaises(LimitedSessionError): client.step(old,0)
            self.assertEqual(sent,client.next_request)
            for replacement in (False,0.0,1<<32):
                changed=deepcopy(restored)
                changed["state"]["decision"]["state"]["prepared"]["drake"]["battlefield"][0][0]["marked_damage"]=replacement
                with self.assertRaises(LimitedSessionError): client.step(changed,0)
                self.assertEqual(sent,client.next_request)
            client.step(deepcopy(restored),0)

    def test_mock_foreign_and_reset_snapshot_refusal_before_request(self):
        with self.client() as first, self.client() as second:
            first.reset(self.decks)
            saved=first.snapshot()
            second.reset(self.decks)
            sent=second.next_request
            with self.assertRaises(LimitedSessionError): second.restore(saved["snapshot_id"])
            self.assertEqual(sent,second.next_request)
            first.reset(self.decks)
            sent=first.next_request
            with self.assertRaises(LimitedSessionError): first.restore(saved["snapshot_id"])
            self.assertEqual(sent,first.next_request)

    def test_mock_server_error_preserves_current_prompt_and_snapshot(self):
        with self.client() as client:
            old=client.reset(self.decks)
            saved=client.snapshot()
            with self.assertRaisesRegex(LimitedSessionError,"unsupported_deck"):
                client.reset((self.decks[0],parse_dck("40 Missing")))
            self.assertFalse(client.closed)
            client.step(old,0)
            client.restore(saved["snapshot_id"])

    def test_mock_corruption_closes_owned_process(self):
        modes=("wrong-protocol","wrong-request","wrong-catalog","wrong-catalog-error","bad-index","bad-actor",
               "wrong-schema","schema2","duplicate","missing-exact","forged-width","bad-next-viewer",
               "bad-next-root","raw-next-id","bad-episode","foreign-hand","extra-reply")
        for mode in modes:
            with self.subTest(mode=mode),self.client(mode,timeout_s=5) as client:
                with self.assertRaises(LimitedSessionError): client.reset(self.decks)
                self.assertTrue(client.closed)
                self.assertIsNotNone(client.process.poll())

    def test_mock_step_receipt_generation_terminal_and_response_deadline(self):
        for mode,operation in (("bad-receipt","step"),("stuck-generation","restore"),("forged-terminal","step"),("late-timeout","step")):
            with self.subTest(mode=mode),self.client(mode,timeout_s=5) as client:
                reply=client.reset(self.decks)
                saved=client.snapshot()
                if mode=="late-timeout": client.timeout_s=0.05  # after the real mock-startup boundary
                with self.assertRaises(LimitedSessionError):
                    client.restore(saved["snapshot_id"]) if operation=="restore" else client.step(reply,0)
                self.assertTrue(client.closed)

    def test_ordered_rules_complete_printing_and_no_catalog_or_private_operands(self):
        visible=self.visible()
        self.validate(visible)
        for family in range(5):
            for mutation in ("order","field","type","id"):
                changed=deepcopy(visible); meaning=changed["next_five"]["meanings"][family]
                if mutation=="order": meaning["ordered_rules"]=[]
                elif mutation=="field": del meaning["printed"]["mana"]
                elif mutation=="type": meaning["printed"]["mana_value"]=float(meaning["printed"]["mana_value"])
                else: meaning["card_id"]=452+family
                with self.subTest(family=family,mutation=mutation),self.assertRaises(LimitedSessionError): self.validate(changed)
        for path in (("meaning",),("meaning","food")):
            changed=deepcopy(visible); node=changed["next_five"]["snack"]
            for part in path: node=node[part]
            node["selector"]={"private":"forged"}
            with self.assertRaises(LimitedSessionError): self.validate(changed)

    def test_exact_all_object_battlefield_widths_and_current_form_consistency(self):
        visible=self.visible()
        self.validate(visible)
        for path,value in ((["soul"],1<<31),(["incubation"],-1),(["time"],256),(["finality"],65536),(["loyalty"],1<<32),(["plus_one"],True)):
            changed=deepcopy(visible)
            changed["prepared"]["drake"]["battlefield"][0][0]["counters"][path[0]]=value
            with self.subTest(path=path,value=value),self.assertRaises(LimitedSessionError): self.validate(changed)
        changed=deepcopy(visible); row=changed["prepared"]["drake"]["battlefield"][0][0]
        current={key:deepcopy(row[key]) for key in ("source","is_token","tapped","summoning_sick","marked_damage","counters","form")}
        current["family"]="Giada"; changed["next_five"]["current"]=[current]
        self.validate(changed)
        current["marked_damage"]=1
        with self.assertRaises(LimitedSessionError): self.validate(changed)

    def test_staged_goblin_x_target_selection_and_final_quote_are_distinct(self):
        visible=self.visible(); f=visible["next_five"]
        spell=ref("goblin-stack","Goblin Negotiation",zone="Stack")
        target=ref("victim","Known creature",owner="p1",controller="p1")
        row=dict(family="Goblin",source=spell,original_controller="p0",is_copy=False,payment_pending=True,x=None,targets=[])
        f["casts"]=[row]; self.validate(visible)
        row["targets"]=[{"Object":target}]; self.validate(visible)
        row["x"]=3; self.validate(visible)
        row["payment_pending"]=False
        f["goblin_quotes"]=[dict(source=spell,original_controller="p0",original_x=3,target={"Object":target},
            target_legal_now=True,creature_lethal_damage_now=1,quote_can_change_before_resolution=True)]
        self.validate(visible)
        for key,value in (("x",False),("x",65536),("x",None),("targets",[]),("targets",[{"Object":target}]*2)):
            changed=deepcopy(visible);changed["next_five"]["casts"][0][key]=value
            with self.subTest(key=key,value=value),self.assertRaises(LimitedSessionError): self.validate(changed)
        changed=deepcopy(visible);changed["next_five"]["goblin_quotes"][0]["creature_lethal_damage_now"]=1<<63
        with self.assertRaises(LimitedSessionError): self.validate(changed)
        f["goblin_quotes"][0].update(target_legal_now=False,creature_lethal_damage_now=None)
        self.validate(visible)

    def test_expanded_aura_each_host_may_repeat_and_accepted_answer_remains_pending(self):
        visible=self.visible(); f=visible["next_five"]
        source=ref("replica","Extravagant Replication")
        host=deepcopy(visible["prepared"]["drake"]["battlefield"][0][0]["source"])
        aura=dict(source=root(source),target={"Object":ref("aura","Witness Protection")},template_name="Witness Protection",
            template_face=0,chooser="p0",expanded_count=4,chosen_hosts=[host,deepcopy(host)],candidate_hosts=[host],
            accepted_host_awaiting_advance=deepcopy(host),remaining_count=2,untargeted=True)
        f["aura_copy"]=aura;self.validate(visible)
        for key,value in (("remaining_count",1),("candidate_hosts",[host,host]),("untargeted",False),
                          ("accepted_host_awaiting_advance",ref("foreign")),("template_face",256),
                          ("chooser","p1"),("expanded_count",4.0)):
            changed=deepcopy(visible);changed["next_five"]["aura_copy"][key]=value
            with self.subTest(key=key),self.assertRaises(LimitedSessionError): self.validate(changed)
        # Hostlessness has no fabricated host or attachment and remains representable.
        aura.update(expanded_count=0,chosen_hosts=[],candidate_hosts=[],accepted_host_awaiting_advance=None,remaining_count=0)
        self.validate(visible)

    def test_infernal_owner_return_and_retained_death_controller_are_independent(self):
        visible=self.visible(); f=visible["next_five"]
        original=ref("original-vessel","Infernal Vessel",owner="p1",controller="p0")
        grave=ref("grave-vessel","Infernal Vessel",owner="p1",controller="p1",zone="Graveyard")
        death=dict(occurrence=0,source=root(original),captured_not_demon=True,collection_completed=True,
            exact_graveyard_card_now=grave,owner_on_return="p1",return_incarnation_available_now=True)
        f["infernal_deaths"]=[death];self.validate(visible)
        for key,value in (("owner_on_return","p0"),("captured_not_demon",False),("occurrence",1),
                          ("exact_graveyard_card_now",None),("collection_completed",1)):
            changed=deepcopy(visible);changed["next_five"]["infernal_deaths"][0][key]=value
            with self.subTest(key=key),self.assertRaises(LimitedSessionError): self.validate(changed)
        death.update(exact_graveyard_card_now=None,return_incarnation_available_now=False)
        self.validate(visible)
        f["triggers"]=[dict(family="Infernal",occurrence=0,source=root(original),stage="Published",placement_ordered=True,targets=[])]
        self.validate(visible)
        f["triggers"][0]["source"]["captured_controller"]="p1"
        with self.assertRaises(LimitedSessionError): self.validate(visible)

    def test_giada_original_angel_roster_and_departed_restricted_mana(self):
        visible=self.visible();f=visible["next_five"]
        giada=deepcopy(visible["prepared"]["drake"]["battlefield"][1][0]["source"])
        other=deepcopy(giada);other.update(handle="angel",name="Public Angel")
        extra=deepcopy(visible["prepared"]["drake"]["battlefield"][1][0]);extra["source"]=other
        visible["prepared"]["drake"]["battlefield"][1].append(extra)
        old=deepcopy(visible["common"]["predecessor"]["public"]["battlefield"][1][0]);old["card"]["stable"]=other
        visible["common"]["predecessor"]["public"]["battlefield"][1].append(old)
        f["angel_replacements"]=[dict(source=giada,controller="p1",current_angels=[giada,other],original_angel_count_now=2,self_entry_excluded=True)]
        f["angel_mana"]=[[],[dict(color="W",only_cast_angel_spell=True,current_original_source=None)]]
        self.validate(visible)
        for key,value in (("original_angel_count_now",1),("controller","p0"),("self_entry_excluded",False)):
            changed=deepcopy(visible);changed["next_five"]["angel_replacements"][0][key]=value
            with self.subTest(key=key),self.assertRaises(LimitedSessionError): self.validate(changed)
        for key,value in (("color","U"),("only_cast_angel_spell",False),("current_original_source",{**giada,"zone":"Hand"})):
            changed=deepcopy(visible);changed["next_five"]["angel_mana"][1][0][key]=value
            with self.subTest(key=key),self.assertRaises(LimitedSessionError): self.validate(changed)

    def test_snack_original_controller_gains_and_each_payment_stage(self):
        visible=self.visible();snack=visible["next_five"]["snack"]
        snack["gained_this_turn"]=[7,11]
        original=ref("snack-old","Midnight Snack",owner="p0",controller="p1")
        row=dict(source=root(original),stage="ChoosingOpponentBeforeCosts",target=None,target_legal_now=None,
                 original_controller_gained_this_turn_now=11)
        snack["activations"]=[row]; self.validate(visible)
        row.update(stage="OpponentChosenAwaitingCosts",target="p0"); self.validate(visible)
        row.update(stage="PaidAndSacrificedOnStack",target_legal_now=False); self.validate(visible)
        for key,value in (("target","p1"),("original_controller_gained_this_turn_now",7),("target_legal_now",None)):
            changed=deepcopy(visible);changed["next_five"]["snack"]["activations"][0][key]=value
            with self.subTest(key=key),self.assertRaises(LimitedSessionError): self.validate(changed)
        snack["held_raid"]=[dict(source=root(original),placement_ordered=False,on_stack=False)]
        snack["published_raid"]=[dict(source=root(original),placement_ordered=True,on_stack=True)]
        self.validate(visible)

    def test_goblin_affected_chooser_actual_excess_exact_width_and_safe_option_order(self):
        visible=self.visible();f=visible["next_five"]
        source=ref("goblin","Goblin Negotiation",owner="p1",controller="p1",zone="Stack")
        target=ref("affected","Known Creature",owner="p1",controller="p0")
        f["goblin_ordering"]=dict(source=source,target=target,controller="p1",chooser="p0",original_x=3,lethal_damage=2,remaining_damage=6)
        f["goblin_ordering_options"]=[dict(kind="shield",source=ref("shield"),shield_remaining=2),dict(kind="multiplier",source=ref("tyrant"),shield_remaining=None)]
        f["goblin_completed"]=[dict(source=source,target=target,controller="p1",original_x=65535,lethal_damage=0,
            actual_damage=(1<<31)-1,excess=(1<<31)-1,tokens_created=(1<<64)-1)]
        self.validate(visible)
        for group,key,value in (("goblin_ordering","chooser","p1"),("goblin_ordering","lethal_damage",1<<63),
                                ("goblin_completed","tokens_created",1<<64),("goblin_completed","actual_damage",True),
                                ("goblin_completed","excess",3),("goblin_ordering_options","source",{**source,"arena_id":3})):
            changed=deepcopy(visible);node=changed["next_five"][group]
            if type(node) is list: node=node[0]
            node[key]=value
            with self.subTest(group=group,key=key),self.assertRaises(LimitedSessionError): self.validate(changed)
        f["goblin_ordering"]=None
        with self.assertRaises(LimitedSessionError): self.validate(visible)

    def test_new_fact_grammar_is_closed_at_each_nested_record(self):
        visible=self.visible();self.validate(visible)
        for key in ("raw_world","producer_sha256","card_db_hash","event_history","native_index"):
            changed=deepcopy(visible);changed["next_five"][key]={}
            with self.subTest(key=key),self.assertRaises(LimitedSessionError): self.validate(changed)
        changed=deepcopy(visible)
        changed["next_five"]["authorized"]=[["Infernal",{**ref("s"),"generation":7}]]
        with self.assertRaises(LimitedSessionError): self.validate(changed)
        changed=deepcopy(visible);changed["next_five"]["authorized"]=[["Infernal",ref("c1","Changed identity")]]
        with self.assertRaises(LimitedSessionError): self.validate(changed)

    def test_protocol_contexts_and_invalid_inputs_cannot_be_implicitly_coerced(self):
        for catalog in (True,-1,1<<64):
            with self.assertRaises(LimitedSessionError): LimitedNextFiveClientV6(["not-started"],expected_card_db_hash=catalog)
        with self.client() as client:
            reply=client.reset(self.decks);sent=client.next_request
            for index in (True,0.0,-1,2):
                with self.assertRaises(LimitedSessionError): client.step(reply,index)
            self.assertEqual(client.next_request,sent)
            client.step(reply,0)

    def test_london_and_combat_context_use_complete_typed_safe_values(self):
        decision={"london_mulligans":{"phase":"keep_or_mulligan","counts":[0,7],"kept":[True,False]},
                  "foundations_combat":{"phase":"damage","assignments":[{"source":ref("attacker"),
                    "recipient":{"target_kind":"player","player":"p1"},"amount":(1<<31)-1}]}}
        _validate_play_context(decision)
        decision["foundations_combat"]["assignments"][0]["recipient"]={"target_kind":"object","object":ref("blocker")}
        _validate_play_context(decision)
        for group,key,value in (("london_mulligans","counts",[False,7]),("london_mulligans","kept",[True]),
                                ("foundations_combat","assignments",[dict(source=ref("attacker"),recipient={"target_kind":"player","player":"p1"},amount=1<<31)])):
            changed=deepcopy(decision);changed[group][key]=value
            with self.subTest(group=group,key=key),self.assertRaises(LimitedSessionError): _validate_play_context(changed)
        changed=deepcopy(decision);changed["foundations_combat"]["assignments"][0]["recipient"]["object"]["arena_id"]=9
        with self.assertRaises(LimitedSessionError): _validate_play_context(changed)


if __name__ == "__main__":
    unittest.main()
