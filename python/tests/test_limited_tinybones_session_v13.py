"""V13 parser/process mocks only. No native execution or Full admission."""
from copy import deepcopy
import sys
import unittest
import test_limited_muldrotha_session_v12 as inherited
from limited_tinybones_session_v13 import LimitedTinybonesClientV13,_expected_tinybones_meaning,_validate_tinybones_visible
from limited_muldrotha_session_v12 import _validate_muldrotha_visible
from limited_session_v1 import LimitedSessionError

INJECT=r'''
    if reply.get("state",{}).get("state_type")=="decision":
        d=reply["state"]["decision"]
        d["state"]["tinybones"]=dict(viewer=d["actor"],active_player=d["actor"],turn_key=1,meaning=copy.deepcopy(TINYBONES_MEANING),authorized_printings=[],battlefield=[[],[]],discards=[],triggers=[],stash=[],plays=[],activations=[])
        d["state"]["tinybones"]["battlefield"]=copy.deepcopy(d["state"]["call"]["battlefield"])
        for rows in d["state"]["tinybones"]["battlefield"]:
            for row in rows:row["counters"]["stash"]=0
        for row in d["action_meanings"]:row["tinybones"]=dict(triggers=[],plays=[])
'''
class LimitedTinybonesClientV13Tests(unittest.TestCase):
    def setUp(self):
        inherited.LimitedMuldrothaClientV12Tests.setUp(self)
        source=self.engine.read_text().replace("kernel_limited_muldrotha_jsonl","kernel_limited_tinybones_jsonl").replace("schema_version=12","schema_version=13").replace("mtg-kernel-limited-muldrotha-decision/v12","mtg-kernel-limited-tinybones-decision/v13")
        source=source.replace("    print(json.dumps(reply), flush=True)",INJECT+"\n    print(json.dumps(reply), flush=True)")
        self.engine.write_text("TINYBONES_MEANING = "+repr(_expected_tinybones_meaning(123,"a"*64))+"\n"+source)
    def tearDown(self):inherited.LimitedMuldrothaClientV12Tests.tearDown(self)
    def client(self,mode="normal"):return LimitedTinybonesClientV13([sys.executable,str(self.engine),mode],expected_card_db_hash=123)
    def decision(self):
        with self.client()as c:return c.reset(self.decks)["state"]["decision"]
    def validate(self,d):_validate_tinybones_visible(d["state"],d["actor"],d["action_meanings"],d["actions"])
    def stash(self):
        d=self.decision();p=d["actor"];other="p1"if p=="p0"else"p0"
        card=dict(handle="stashed-current",name="Think Twice",owner=other,controller=other,zone="Exile")
        source=dict(handle="old-tiny",name="Tinybones, Bauble Burglar",owner=p,controller=p,zone="Battlefield")
        root=dict(original_source=source,current_original_source=None,captured_controller=p)
        original=dict(object_role=10,incarnation_role=30,owner=other,controller=other,zone="Hand")
        stashed=dict(object_role=10,incarnation_role=31,owner=other,controller=other,zone="Exile")
        f=d["state"]["tinybones"]
        f["discards"]=[dict(occurrence=0,original_card=original,discarder=other,outcome_zone="Graveyard",current_original_outcome=None,observers=[deepcopy(root)])]
        f["triggers"]=[dict(discard=0,observer=0,source=deepcopy(root),stage="Resolved",native_position=None,stashed=stashed)]
        counters=deepcopy(d["state"]["call"]["battlefield"][0][0]["counters"]);counters["stash"]=1
        current=deepcopy(root);current["original_source"]["handle"]="different-current-tiny";current["current_original_source"]=deepcopy(current["original_source"])
        f["stash"]=[dict(card=card,counters=counters,can_play=[p=="p0",p=="p1"],any_type_mana=[p=="p0",p=="p1"],current_grant_sources=[[deepcopy(current)]if p=="p0"else[],[deepcopy(current)]if p=="p1"else[]])]
        for row in d["action_meanings"]:row["tinybones"]=dict(triggers=deepcopy(f["triggers"]),plays=[])
        return d
    def test_mock_v13_both_actors_restore_generation_and_stale_prompt(self):
        for actor in ("p0","p1"):
            with self.client()as c:
                before=c.reset(self.decks,starting_player=actor);self.assertEqual(before["schema_version"],13)
                self.assertEqual(before["state"]["decision"]["state"]["tinybones"]["viewer"],actor)
                saved=c.snapshot();fresh=c.restore(saved["snapshot_id"])
                with self.assertRaises(LimitedSessionError):c.step(before,0)
                c.step(fresh,0)
    def test_anonymous_historical_roles_and_current_independent_source(self):
        d=self.stash();self.validate(d)
        self.assertNotEqual(d["state"]["tinybones"]["triggers"][0]["source"]["original_source"]["handle"],d["state"]["tinybones"]["stash"][0]["current_grant_sources"][0 if d["actor"]=="p0"else 1][0]["original_source"]["handle"])
        for key in ("name","card_id","generation","history_index","private_world"):
            bad=deepcopy(d);bad["state"]["tinybones"]["discards"][0]["original_card"][key]=0
            with self.assertRaises(LimitedSessionError):self.validate(bad)
    def test_exact_eighteen_counter_widths_adjacent_i32_and_u32_loyalty(self):
        d=self.stash()
        for amount in ((1<<31)-2,(1<<31)-1):
            d["state"]["tinybones"]["stash"][0]["counters"].update(stash=amount,fellowship=(1<<31)-1,charge=(1<<31)-1,loyalty=(1<<32)-1);self.validate(d)
        for key,value in (("stash",1<<31),("stash",True),("loyalty",1<<32),("revival",-1)):
            bad=deepcopy(d);bad["state"]["tinybones"]["stash"][0]["counters"][key]=value
            with self.assertRaises(LimitedSessionError):self.validate(bad)
        bad=deepcopy(d);bad["state"]["tinybones"]["stash"][0]["counters"].pop("stash")
        with self.assertRaises(LimitedSessionError):self.validate(bad)
    def test_authoritative_counter_roster_cannot_disagree_with_inherited_prefix(self):
        d=self.decision();self.validate(d)
        for change in (lambda f:f["battlefield"][0].pop(),lambda f:f["battlefield"][0][0]["counters"].update(charge=1),lambda f:f["battlefield"][0][0].update(tapped=not f["battlefield"][0][0]["tapped"])):
            bad=deepcopy(d);change(bad["state"]["tinybones"])
            with self.assertRaises(LimitedSessionError):self.validate(bad)
    def test_trigger_binding_positions_and_action_context_refuse(self):
        d=self.stash();self.validate(d)
        changes=(lambda f:f["triggers"][0].update(discard=1),lambda f:f["triggers"][0].update(stage="Published"),lambda f:f["triggers"][0]["stashed"].update(object_role=11),lambda f:f["discards"][0].update(discarder=d["actor"]))
        for change in changes:
            bad=deepcopy(d);change(bad["state"]["tinybones"])
            with self.assertRaises(LimitedSessionError):self.validate(bad)
        bad=deepcopy(d);bad["action_meanings"][0]["tinybones"]["triggers"]=[]
        with self.assertRaises(LimitedSessionError):self.validate(bad)
    def test_paid_announced_forms_and_original_controller_not_rewritten(self):
        d=self.stash();f=d["state"]["tinybones"];root=f["stash"][0]["current_grant_sources"][0 if d["actor"]=="p0"else 1][0]
        play=dict(occurrence=0,original_card=deepcopy(f["stash"][0]["card"]),permission_source=deepcopy(root),actor=d["actor"],original_turn=1,stash=1,land=False,stage="Paid",announced_form="normal",paid_form="normal",paid_x=256,variable_targeting_kicked=False)
        f["plays"]=[play]
        for row in d["action_meanings"]:row["tinybones"]["plays"]=deepcopy(f["plays"])
        self.validate(d)
        for key,value in (("paid_x",None),("paid_x",65536),("paid_form",None),("announced_form",None)):
            bad=deepcopy(d);bad["state"]["tinybones"]["plays"][0][key]=value
            with self.assertRaises(LimitedSessionError):self.validate(bad)
    def test_old_v12_refuses_and_printed_meaning_is_closed(self):
        d=self.stash()
        with self.assertRaises(LimitedSessionError):_validate_muldrotha_visible(d["state"],d["actor"],d["action_meanings"],d["actions"])
        for change in (lambda m:m["ordered_rules"].reverse(),lambda m:m["printed"].update(activation_generic_mana=4),lambda m:m["printed"].update(subtypes=["Rogue"]),lambda m:m.update(complete_consumer_admitted=True)):
            bad=deepcopy(d);change(bad["state"]["tinybones"]["meaning"])
            with self.assertRaises(LimitedSessionError):self.validate(bad)
if __name__=="__main__":unittest.main()
