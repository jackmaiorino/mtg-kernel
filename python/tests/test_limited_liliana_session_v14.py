"""V14 parser/process mocks only. No native execution or Full admission."""
from copy import deepcopy
import sys
import unittest
import test_limited_tinybones_session_v13 as inherited
from limited_liliana_session_v14 import LimitedLilianaClientV14,_expected_liliana_meaning,_validate_liliana_visible
from limited_tinybones_session_v13 import _validate_tinybones_visible
from limited_session_v1 import LimitedSessionError
INJECT=r'''
    if reply.get("state",{}).get("state_type")=="decision":
        d=reply["state"]["decision"]
        d["state"]["liliana"]=dict(viewer=d["actor"],active_player=d["actor"],turn_key=1,meaning=copy.deepcopy(LILIANA_MEANING),authorized_printings=[],battlefield=copy.deepcopy(d["state"]["tinybones"]["battlefield"]),activations=[],resolutions=[],deaths=[],current_draws=[])
        for row in d["action_meanings"]:row["liliana"]=dict(choices=[],activation=None)
'''
class LimitedLilianaClientV14Tests(unittest.TestCase):
    def setUp(self):
        inherited.LimitedTinybonesClientV13Tests.setUp(self)
        source=self.engine.read_text().replace("kernel_limited_tinybones_jsonl","kernel_limited_liliana_jsonl").replace("schema_version=13","schema_version=14").replace("mtg-kernel-limited-tinybones-decision/v13","mtg-kernel-limited-liliana-decision/v14")
        source=source.replace("    print(json.dumps(reply), flush=True)",INJECT+"\n    print(json.dumps(reply), flush=True)")
        self.engine.write_text("LILIANA_MEANING = "+repr(_expected_liliana_meaning(123,"b"*64))+"\n"+source)
    def tearDown(self):inherited.LimitedTinybonesClientV13Tests.tearDown(self)
    def client(self,mode="normal"):return LimitedLilianaClientV14([sys.executable,str(self.engine),mode],expected_card_db_hash=123)
    def decision(self):
        with self.client()as c:return c.reset(self.decks)["state"]["decision"]
    def validate(self,d):_validate_liliana_visible(d["state"],d["actor"],d["action_meanings"],d["actions"])
    def scene(self):
        d=self.decision();p=d["actor"];other="p1"if p=="p0"else"p0"
        source=dict(handle="original-liliana",name="Liliana, Dreadhorde General",owner=p,controller=p,zone="Battlefield")
        root=dict(original_source=source,current_original_source=None,captured_controller=p,original_source_relation=dict(object_role=5,incarnation_role=6,owner=p,controller=p,zone="Battlefield"))
        def relation(i):return dict(object_role=i,incarnation_role=i+10,owner=other,controller=other,zone="Battlefield")
        hart,pick,druid=relation(10),relation(11),relation(12)
        def scene(r,types):return dict(original=deepcopy(r),permanent_types=types,current_original=None)
        c=dict(occurrence=0,phase=2,actor=other,kind={"KeepPermanent":"Artifact"},candidates=[deepcopy(hart),deepcopy(pick)],exact_count=1,ordered=False,selected_prefix=[],current=True)
        rosters=[[],[]];rosters[0 if other=="p0"else 1]=[scene(hart,["Creature","Artifact"]),scene(pick,["Artifact"]),scene(druid,["Creature"])]
        r=dict(activation=0,active_player=p,controller=p,ultimate=True,rosters=rosters,selections=[c],chosen_sacrifices=[[],[]],keepers=[dict(permanent_type="Creature",permanent=deepcopy(hart))],graveyard_orders=[[],[]],completed_sacrifices=[],finished=False)
        f=d["state"]["liliana"]
        f["activations"]=[dict(occurrence=0,source=root,turn=1,ability_index=2,loyalty_before=12,loyalty_after=3,stage="Resolving",created_zombies=[])]
        f["resolutions"]=[r]
        for a in d["action_meanings"]:a["liliana"]=dict(choices=[deepcopy(c)],activation=None)
        return d
    def test_mock_both_actors_snapshot_restore_and_stale_prompt(self):
        for actor in ("p0","p1"):
            with self.client()as c:
                before=c.reset(self.decks,starting_player=actor);self.assertEqual(before["schema_version"],14)
                self.assertEqual(before["state"]["decision"]["state"]["liliana"]["viewer"],actor)
                saved=c.snapshot();fresh=c.restore(saved["snapshot_id"])
                with self.assertRaises(LimitedSessionError):c.step(before,0)
                c.step(fresh,0)
    def test_multitype_keeper_union_and_current_original_scene_context(self):
        d=self.scene();self.validate(d);f=d["state"]["liliana"];r=f["resolutions"][0]
        hart=r["keepers"][0]["permanent"]
        r["keepers"].append(dict(permanent_type="Artifact",permanent=deepcopy(hart)))
        r["selections"][0].update(current=False,selected_prefix=[deepcopy(hart)])
        r["completed_sacrifices"]=[deepcopy(o["original"])for rows in r["rosters"]for o in rows if o["original"]!=hart];r["finished"]=True
        for a in d["action_meanings"]:a["liliana"]["choices"]=[]
        self.validate(d)
        bad=deepcopy(d);bad["state"]["liliana"]["resolutions"][0]["completed_sacrifices"].append(deepcopy(hart))
        with self.assertRaises(LimitedSessionError):self.validate(bad)
        bad=deepcopy(d);bad["state"]["liliana"]["resolutions"][0]["keepers"][1]["permanent_type"]="Land"
        with self.assertRaises(LimitedSessionError):self.validate(bad)
    def test_original_roles_reject_hidden_identity_generations_and_journal_keys(self):
        d=self.scene();self.validate(d)
        for key in ("name","card_id","generation","history_index","private_world","object"):
            bad=deepcopy(d);bad["state"]["liliana"]["resolutions"][0]["rosters"][0 if d["actor"]=="p1"else 1][0]["original"][key]=0
            with self.assertRaises(LimitedSessionError):self.validate(bad)
    def test_choice_cardinality_prefix_current_context_and_simultaneous_batch_refuse(self):
        d=self.scene();self.validate(d)
        changes=(lambda r:r["selections"][0].update(exact_count=3),lambda r:r["selections"][0].update(selected_prefix=[r["selections"][0]["candidates"][0]]*2),lambda r:r.update(completed_sacrifices=[r["rosters"][0 if d["actor"]=="p1"else 1][1]["original"]]))
        for change in changes:
            bad=deepcopy(d);change(bad["state"]["liliana"]["resolutions"][0])
            with self.assertRaises(LimitedSessionError):self.validate(bad)
        bad=deepcopy(d);bad["action_meanings"][0]["liliana"]["choices"]=[]
        with self.assertRaises(LimitedSessionError):self.validate(bad)
    def test_original_observer_death_draw_counts_sourcegone_and_no_invented_per_death_link(self):
        d=self.scene();f=d["state"]["liliana"];root=deepcopy(f["activations"][0]["source"])
        creature=dict(object_role=30,incarnation_role=40,owner=d["actor"],controller=d["actor"],zone="Battlefield")
        f["deaths"]=[dict(occurrence=0,observer=root,creature=creature,collected=True)]
        f["current_draws"]=[dict(source=deepcopy(root),stage="Held",native_position=0)];self.validate(d)
        for change in (lambda f:f["deaths"][0].update(collected=False),lambda f:f["current_draws"].append(deepcopy(f["current_draws"][0])),lambda f:f["current_draws"][0].update(death=0)):
            bad=deepcopy(d);change(bad["state"]["liliana"])
            with self.assertRaises(LimitedSessionError):self.validate(bad)
    def test_exact_paid_loyalty_cost_widths_and_printed_token_meaning(self):
        d=self.scene();f=d["state"]["liliana"]
        c=dict(source=deepcopy(f["activations"][0]["source"]["original_source"]),ability_index=2,before=(1<<32)-1,after=(1<<32)-10,paid_before_publication=True,countering_refunds_cost=False)
        d["action_meanings"][0]["liliana"]["activation"]=c;self.validate(d)
        for key,value in (("before",1<<32),("before",True),("after",1),("countering_refunds_cost",True)):
            bad=deepcopy(d);bad["action_meanings"][0]["liliana"]["activation"][key]=value
            with self.assertRaises(LimitedSessionError):self.validate(bad)
        for change in (lambda m:m["printed"].update(zombie_colors=["W"]),lambda m:m["printed"].update(loyalty_costs=[1,-3,-9]),lambda m:m["ordered_rules"].reverse(),lambda m:m.update(complete_consumer_admitted=True)):
            bad=deepcopy(d);change(bad["state"]["liliana"]["meaning"])
            with self.assertRaises(LimitedSessionError):self.validate(bad)
    def test_inherited_tinybones_catalog_must_match_header(self):
        source=self.engine.read_text().replace('    print(json.dumps(reply), flush=True)', '    if reply.get("state",{}).get("state_type")=="decision":reply["state"]["decision"]["state"]["tinybones"]["meaning"]["card_db_hash"]=124\n    print(json.dumps(reply), flush=True)')
        self.engine.write_text(source)
        with self.client()as c:
            with self.assertRaisesRegex(LimitedSessionError,"Tinybones catalog differs from header"):c.reset(self.decks)
    def test_eighteen_counter_roster_and_v13_closed_refusal(self):
        d=self.scene();self.validate(d)
        with self.assertRaises(LimitedSessionError):_validate_tinybones_visible(d["state"],d["actor"],d["action_meanings"],d["actions"])
        bad=deepcopy(d);bad["state"]["liliana"]["battlefield"][0][0]["counters"]["stash"]=(1<<31)-1
        with self.assertRaises(LimitedSessionError):self.validate(bad)
        bad=deepcopy(d);bad["state"]["liliana"]["battlefield"][0][0]["counters"].pop("stash")
        with self.assertRaises(LimitedSessionError):self.validate(bad)
if __name__=="__main__":unittest.main()
