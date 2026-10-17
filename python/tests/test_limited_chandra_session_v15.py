"""V15 parser/process mocks. These do not qualify native execution."""
from copy import deepcopy
import sys
import unittest
import test_limited_liliana_session_v14 as inherited
from limited_chandra_session_v15 import LimitedChandraClientV15,_expected_chandra_meaning,_validate_chandra_visible
from limited_liliana_session_v14 import _validate_liliana_visible
from limited_session_v1 import LimitedSessionError
INJECT=r'''
    if reply.get("state",{}).get("state_type")=="decision":
        d=reply["state"]["decision"]
        d["state"]["chandra"]=dict(viewer=d["actor"],active_player=d["actor"],turn_key=1,meaning=copy.deepcopy(CHANDRA_MEANING),authorized_printings=[],battlefield=copy.deepcopy(d["state"]["tinybones"]["battlefield"]),pending_allocation=None,activations=[],exiles=[],pending_damage=None,copied_tokens=[],end_steps=[])
        for row in d["action_meanings"]:row["chandra"]=dict(selected_damage_option=None,selected_allocation_target=None,selected_exile=None,copy_target=None,activation=None,allocation=None,exiles=[],damage=None)
'''
class LimitedChandraClientV15Tests(unittest.TestCase):
    def setUp(self):
        inherited.LimitedLilianaClientV14Tests.setUp(self)
        source=self.engine.read_text().replace("kernel_limited_liliana_jsonl","kernel_limited_chandra_jsonl").replace("schema_version=14","schema_version=15").replace("mtg-kernel-limited-liliana-decision/v14","mtg-kernel-limited-chandra-decision/v15")
        source=source.replace("    print(json.dumps(reply), flush=True)",INJECT+"\n    print(json.dumps(reply), flush=True)")
        self.engine.write_text("CHANDRA_MEANING = "+repr(_expected_chandra_meaning(123,"c"*64))+"\n"+source)
    def tearDown(self):inherited.LimitedLilianaClientV14Tests.tearDown(self)
    def client(self):return LimitedChandraClientV15([sys.executable,str(self.engine),"normal"],expected_card_db_hash=123)
    def decision(self):
        with self.client()as c:return c.reset(self.decks)["state"]["decision"]
    def validate(self,d):_validate_chandra_visible(d["state"],d["actor"],d["action_meanings"],d["actions"])
    def ref(self,d,h,name="Axgard Cavalry",zone="Battlefield"):
        return dict(handle=h,name=name,owner=d["actor"],controller=d["actor"],zone=zone)
    def root(self,d,h="original-chandra",name="Chandra, Flameshaper",role=30):
        p=d["actor"]
        return dict(original_source=self.ref(d,h,name),current_original_source=None,captured_controller=p,original_source_relation=dict(object_role=role,incarnation_role=role+10,owner=p,controller=p,zone="Battlefield"))
    def activation(self,d,index):
        return dict(occurrence=0,source=self.root(d),ability_index=index,loyalty_before=6,loyalty_after=6+(2,1,-4)[index],targets=[],stage="Published",damage_source=None)
    def context(self,d):
        f=d["state"]["chandra"]
        for row in d["action_meanings"]:row["chandra"].update(allocation=deepcopy(f["pending_allocation"]),exiles=deepcopy([e for e in f["exiles"]if e["choice_current"]]),damage=deepcopy(f["pending_damage"]))
    def test_mock_both_actors_current_snapshot_restore_stale_and_changed_packet(self):
        for actor in("p0","p1"):
            with self.client()as c:
                before=c.reset(self.decks,starting_player=actor);self.assertEqual(before["schema_version"],15)
                self.assertEqual(before["state"]["decision"]["state"]["chandra"]["viewer"],actor)
                saved=c.snapshot();fresh=c.restore(saved["snapshot_id"])
                with self.assertRaises(LimitedSessionError):c.step(before,0)
                forged=deepcopy(fresh);forged["state"]["decision"]["state"]["chandra"]["turn_key"]+=1
                with self.assertRaises(LimitedSessionError):c.step(forged,0)
                c.step(fresh,0)
    def test_prepaid_zero_and_eight_distinct_contracts_unit_conservation_and_u32_quote(self):
        d=self.decision();f=d["state"]["chandra"]
        alloc=dict(source=self.root(d),loyalty_before=(1<<32)-1,targets=[],remaining_units=8,finished=False,can_finish_zero=True,candidates=[])
        f["pending_allocation"]=alloc;self.context(d);self.validate(d)
        for i in range(8):
            ref=self.ref(d,"target"+str(i));r=dict(object_role=100+i,incarnation_role=200+i,owner=d["actor"],controller=d["actor"],zone="Battlefield")
            alloc["targets"].append(dict(original=r,current_original=ref,allocated_damage=1,legal_now=True))
        alloc.update(remaining_units=0,finished=True,can_finish_zero=False);self.context(d);self.validate(d)
        for mutate in(lambda a:a.update(remaining_units=1),lambda a:a["targets"][0].update(allocated_damage=0),lambda a:a["targets"].append(deepcopy(a["targets"][0])),lambda a:a.update(loyalty_before=1<<32)):
            bad=deepcopy(d);mutate(bad["state"]["chandra"]["pending_allocation"]);self.context(bad)
            with self.assertRaises(LimitedSessionError):self.validate(bad)
    def test_damage_declared_native_index_survives_alphabetical_display_order(self):
        d=self.decision();f=d["state"]["chandra"];a=self.activation(d,2)
        a["targets"]=[dict(original=dict(object_role=100,incarnation_role=200,owner=d["actor"],controller=d["actor"],zone="Battlefield"),current_original=self.ref(d,"dragon","Shivan Dragon"),allocated_damage=8,legal_now=True)]
        a["damage_source"]=dict(source=deepcopy(a["source"]),historical=True,color_mask=8,lifelink=False,deathtouch=False,werewolf=False);f["activations"]=[a]
        f["pending_damage"]=dict(activation=0,chooser=d["actor"],recipient=self.ref(d,"dragon","Shivan Dragon"),original_amount=8,remaining_amount=8,simultaneous_waiting=0,options=[dict(menu_index=0,kind="Shield",source=None,shield_remaining=3),dict(menu_index=1,kind="Multiplier",source=None,shield_remaining=None)])
        row=deepcopy(d["action_meanings"][0]);d["action_meanings"]=[deepcopy(row),deepcopy(row)]
        d["actions"]=[dict(action_index=0,label="Apply Multiplier"),dict(action_index=1,label="Apply Shield")]
        d["action_meanings"][0]["chandra"]["selected_damage_option"]=1;d["action_meanings"][1]["chandra"]["selected_damage_option"]=0;self.context(d);self.validate(d)
        for value in(None,1,2):
            bad=deepcopy(d);bad["action_meanings"][1]["chandra"]["selected_damage_option"]=value
            with self.assertRaises(LimitedSessionError):self.validate(bad)
        bad=deepcopy(d);bad["state"]["chandra"]["activations"][0]["targets"][0]["legal_now"]=False;self.validate(bad)
    def test_exact_exile_original_index_sourcegone_grant_cost_land_quota_expiry(self):
        d=self.decision();f=d["state"]["chandra"];f["activations"]=[self.activation(d,0)]
        cards=[dict(original=self.ref(d,"exile"+str(i),name,"Exile"),current_original=None)for i,name in enumerate(("Forest","Think Twice","Mountain"))]
        f["exiles"]=[dict(activation=0,cards=cards,choice_current=True,selected_prefix=[],chosen=None,grant_for_land=None,permission_present=False,expired=False)]
        row=deepcopy(d["action_meanings"][0]);d["action_meanings"]=[deepcopy(row)for _ in cards];d["actions"]=[dict(action_index=i,label=name)for i,name in enumerate(("Forest","Mountain","Think Twice"))]
        for row,index in zip(d["action_meanings"],(0,2,1)):row["chandra"]["selected_exile"]=index
        self.context(d);self.validate(d)
        for row in d["action_meanings"]:row["chandra"]["selected_exile"]=None
        e=f["exiles"][0];e.update(choice_current=False,chosen=1,grant_for_land=False,permission_present=True);self.context(d);self.validate(d)
        e.update(expired=True,permission_present=False);self.validate(d)
        bad=deepcopy(d);bad["state"]["chandra"]["exiles"][0]["permission_present"]=True
        with self.assertRaises(LimitedSessionError):self.validate(bad)
    def test_copiable_token_haste_optional_multiplicity_and_anonymous_endstep_binding(self):
        d=self.decision();f=d["state"]["chandra"];root=self.root(d,"copied-hawk","Healer's Hawk",70)
        f["copied_tokens"]=[dict(occurrence=0,source=deepcopy(root),sacrifice_instances=2,current_haste=True)]
        f["end_steps"]=[dict(occurrence=0,observers=[dict(token=0,source=deepcopy(root),instances=2)],collected=True,published_count=2,finished_count=1)];self.validate(d)
        f["copied_tokens"][0]["current_haste"]=None;f["end_steps"][0]["finished_count"]=2;self.validate(d)
        for mutate in(lambda f:f["end_steps"][0]["observers"][0]["source"]["original_source_relation"].update(incarnation_role=999),lambda f:f["copied_tokens"][0].update(sacrifice_instances=0),lambda f:f["end_steps"][0].update(finished_count=3)):
            bad=deepcopy(d);mutate(bad["state"]["chandra"])
            with self.assertRaises(LimitedSessionError):self.validate(bad)
    def test_inherited_catalogs_match_header_including_tinybones_and_liliana(self):
        original=self.engine.read_text()
        for family in("tinybones","liliana","chandra"):
            source=original.replace('    print(json.dumps(reply), flush=True)',f'    if reply.get("state",{{}}).get("state_type")=="decision":reply["state"]["decision"]["state"]["{family}"]["meaning"]["card_db_hash"]=124\n    print(json.dumps(reply), flush=True)')
            self.engine.write_text(source)
            with self.client()as c:
                with self.assertRaisesRegex(LimitedSessionError,"catalog differs from header"):c.reset(self.decks)
    def test_old_v14_refusal_eighteen_counters_closed_meaning_and_private_roles(self):
        d=self.decision();self.validate(d)
        with self.assertRaises(LimitedSessionError):_validate_liliana_visible(d["state"],d["actor"],d["action_meanings"],d["actions"])
        for mutate in(lambda f:f["meaning"]["ordered_rules"].reverse(),lambda f:f["meaning"]["printed"].update(minus_four_damage=7),lambda f:f["meaning"].update(complete_consumer_admitted=True),lambda f:f["battlefield"][0][0]["counters"].pop("stash")):
            bad=deepcopy(d);mutate(bad["state"]["chandra"])
            with self.assertRaises(LimitedSessionError):self.validate(bad)
        d["state"]["chandra"]["activations"]=[self.activation(d,0)]
        for key in("card_id","generation","history_index","private_world","arena_id"):
            bad=deepcopy(d);bad["state"]["chandra"]["activations"][0]["source"]["original_source_relation"][key]=0
            with self.assertRaises(LimitedSessionError):self.validate(bad)
if __name__=="__main__":unittest.main()
