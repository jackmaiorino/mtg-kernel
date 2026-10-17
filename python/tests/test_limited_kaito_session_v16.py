"""V16 process/parser mocks only. Native paid fixtures remain separate."""
from copy import deepcopy
import sys
import unittest
import test_limited_chandra_session_v15 as inherited
from limited_kaito_session_v16 import LimitedKaitoClientV16,_expected_kaito_meaning,_validate_kaito_visible
from limited_chandra_session_v15 import _validate_chandra_visible
from limited_session_v1 import LimitedSessionError
INJECT=r'''
    if reply.get("state",{}).get("state_type")=="decision":
        d=reply["state"]["decision"]
        d["state"]["kaito"]=dict(viewer=d["actor"],active_player=d["actor"],turn_key=1,meaning=copy.deepcopy(KAITO_MEANING),authorized_printings=[],battlefield=copy.deepcopy(d["state"]["tinybones"]["battlefield"]),activations=[],combats=[],emblems=[],casts=[],triggers=[],ninja_batches=[])
        for row in d["action_meanings"]:row["kaito"]=dict(activation=None,activations=[],triggers=[],selected_target=None,selected_discard=None)
'''
class LimitedKaitoClientV16Tests(unittest.TestCase):
    def setUp(self):
        inherited.LimitedChandraClientV15Tests.setUp(self)
        source=self.engine.read_text().replace("kernel_limited_chandra_jsonl","kernel_limited_kaito_jsonl").replace("schema_version=15","schema_version=16").replace("mtg-kernel-limited-chandra-decision/v15","mtg-kernel-limited-kaito-decision/v16")
        source=source.replace("    print(json.dumps(reply), flush=True)",INJECT+"\n    print(json.dumps(reply), flush=True)")
        self.engine.write_text("KAITO_MEANING = "+repr(_expected_kaito_meaning(123,"d"*64))+"\n"+source)
    def tearDown(self):inherited.LimitedChandraClientV15Tests.tearDown(self)
    def client(self):return LimitedKaitoClientV16([sys.executable,str(self.engine),"normal"],expected_card_db_hash=123)
    def decision(self):
        with self.client()as c:return c.reset(self.decks)["state"]["decision"]
    def validate(self,d):_validate_kaito_visible(d["state"],d["actor"],d["action_meanings"],d["actions"])
    def ref(self,d,h,name="Kaito, Cunning Infiltrator",zone="Battlefield"):
        return dict(handle=h,name=name,owner=d["actor"],controller=d["actor"],zone=zone)
    def role(self,d,i,zone="Battlefield"):
        return dict(object_role=i,incarnation_role=i+10,owner=d["actor"],controller=d["actor"],zone=zone)
    def root(self,d,i=30):return dict(original_source=self.ref(d,"original-kaito"),current_original_source=None,captured_controller=d["actor"],original_source_relation=self.role(d,i))
    def activation(self,d,index=0):
        return dict(occurrence=0,source=self.root(d),original_turn=1,ability_index=index,loyalty_before=9,loyalty_after=9+(1,-2,-9)[index],targets=[],stage="Published",draw="NotReached",discard=None,evasion=None)
    def context(self,d):
        f=d["state"]["kaito"]
        for row in d["action_meanings"]:row["kaito"].update(activations=deepcopy(f["activations"]),triggers=deepcopy(f["triggers"]))
    def test_mock_both_actors_snapshot_restore_and_stale_changed_packet(self):
        for actor in("p0","p1"):
            with self.client()as c:
                old=c.reset(self.decks,starting_player=actor);self.assertEqual(old["schema_version"],16)
                self.assertEqual(old["state"]["decision"]["state"]["kaito"]["viewer"],actor)
                saved=c.snapshot();fresh=c.restore(saved["snapshot_id"])
                with self.assertRaises(LimitedSessionError):c.step(old,0)
                bad=deepcopy(fresh);bad["state"]["decision"]["state"]["kaito"]["turn_key"]+=1
                with self.assertRaises(LimitedSessionError):c.step(bad,0)
                c.step(fresh,0)
    def test_optional_original_target_actual_draw_private_discard_and_anonymous_answer(self):
        for selected in(False,True):
            d=self.decision();f=d["state"]["kaito"];a=self.activation(d);f["activations"]=[a]
            if selected:a["targets"]=[dict(original=self.role(d,80),current_original=self.ref(d,"creature","Axgard Cavalry"),legal_now=True)]
            a.update(stage="WaitingDiscard",draw="Drawn",discard=dict(count=1,answered=False,private_choices=[self.ref(d,"hand-island","Island","Hand")],private_chosen=None),evasion=dict(target=deepcopy(a["targets"][0]["original"])if selected else None,active_on_original=selected))
            self.context(d);d["action_meanings"][0]["kaito"]["selected_discard"]=deepcopy(a["discard"]["private_choices"]);self.validate(d)
            bad=deepcopy(d);bad["state"]["kaito"]["activations"][0]["discard"]["private_choices"][0]["owner"]="p1"if d["actor"]=="p0"else"p0";self.context(bad)
            with self.assertRaises(LimitedSessionError):self.validate(bad)
            a.update(stage="Resolved");a["discard"].update(answered=True,private_choices=None,private_chosen=[self.role(d,90,"Hand")])
            for row in d["action_meanings"]:row["kaito"]["selected_discard"]=None
            self.context(d);self.validate(d)
            for key in("name","generation","arena_id","history_index","private_world"):
                bad=deepcopy(d);bad["state"]["kaito"]["activations"][0]["discard"]["private_chosen"][0][key]=0;self.context(bad)
                with self.assertRaises(LimitedSessionError):self.validate(bad)
    def test_illegal_target_fizzle_sourcegone_current_concealment_and_exact_u32_loyalty(self):
        d=self.decision();a=self.activation(d);d["state"]["kaito"]["activations"]=[a]
        a["targets"]=[dict(original=self.role(d,80),current_original=None,legal_now=False)];a["stage"]="Fizzled";self.context(d);self.validate(d)
        a["source"]["current_original_source"]=self.ref(d,"concealed-current","Face-down card");self.context(d);self.validate(d)
        q=dict(source=self.ref(d,"original-kaito"),ability_index=2,before=(1<<32)-1,after=(1<<32)-10,paid_before_publication=True,countering_refunds_cost=False);d["action_meanings"][0]["kaito"]["activation"]=q;self.validate(d)
        bad=deepcopy(d);bad["action_meanings"][0]["kaito"]["activation"]["before"]=1<<32
        with self.assertRaises(LimitedSessionError):self.validate(bad)
        bad=deepcopy(d);bad["state"]["kaito"]["activations"][0]["source"]["original_source"]["name"]="Face-down card";self.context(bad)
        with self.assertRaises(LimitedSessionError):self.validate(bad)
    def test_original_combat_mandatory_trigger_stage_position_and_inventory(self):
        d=self.decision();f=d["state"]["kaito"];root=self.root(d)
        f["combats"]=[dict(occurrence=0,observer=root,original_creature=self.role(d,80),current_original_creature=None,damaged_player="p1"if d["actor"]=="p0"else"p0",final_damage=2)]
        f["triggers"]=[dict(occurrence=0,origin={"Combat":0},source=deepcopy(root),stage="Held",native_position=0)];self.context(d);self.validate(d)
        for mutate in(lambda f:f["triggers"].clear(),lambda f:f["triggers"][0].update(origin={"Combat":1}),lambda f:f["triggers"][0].update(native_position=None),lambda f:f["combats"][0].update(final_damage=0)):
            bad=deepcopy(d);mutate(bad["state"]["kaito"]);self.context(bad)
            with self.assertRaises(LimitedSessionError):self.validate(bad)
        f["triggers"][0].update(stage="Countered",native_position=None);self.context(d);self.validate(d)
    def test_independent_emblems_shared_actual_cast_copies_not_casts_and_real_ninja_batches(self):
        d=self.decision();f=d["state"]["kaito"];a=self.activation(d,2);a["stage"]="Resolved";b=deepcopy(a);b["occurrence"]=1;f["activations"]=[a,b]
        f["emblems"]=[dict(occurrence=i,activation=i,controller=d["actor"])for i in range(2)]
        spell=self.role(d,100,"Stack")
        for i in range(2):
            f["casts"].append(dict(occurrence=i,emblem=i,actual_cast=0,original_spell=deepcopy(spell),current_original_spell=None,x_value=65535,kicked=True,cast_method="normal"))
            f["triggers"].append(dict(occurrence=i,origin={"EmblemCast":i},source=deepcopy(f["activations"][i]["source"]),stage="Resolved",native_position=None))
            f["ninja_batches"].append(dict(occurrence=i,publication={"Trigger":i},controller=d["actor"],created=[dict(original=self.ref(d,f"ninja{i}-{j}","Ninja Token"),current_original=None)for j in range(2)]))
        self.context(d);self.validate(d)
        for mutate in(lambda f:f["casts"][1].update(kicked=False),lambda f:f["casts"][1].update(actual_cast=2),lambda f:f["ninja_batches"].pop(),lambda f:f["ninja_batches"][0]["created"][0]["original"].update(name="Zombie Token")):
            bad=deepcopy(d);mutate(bad["state"]["kaito"]);self.context(bad)
            with self.assertRaises(LimitedSessionError):self.validate(bad)
    def test_closed_predecessor_refusal_native_eighteen_counters_and_printed_meaning(self):
        d=self.decision();self.validate(d)
        with self.assertRaises(LimitedSessionError):_validate_chandra_visible(d["state"],d["actor"],d["action_meanings"],d["actions"])
        for mutate in(lambda f:f["meaning"]["ordered_rules"].reverse(),lambda f:f["meaning"]["ninja"].update(keyword_bits=1),lambda f:f["meaning"].update(model_admitted=True),lambda f:f["battlefield"][0][0]["counters"].pop("stash")):
            bad=deepcopy(d);mutate(bad["state"]["kaito"])
            with self.assertRaises(LimitedSessionError):self.validate(bad)
    def test_all_later_catalogs_match_header(self):
        original=self.engine.read_text()
        for family in("tinybones","liliana","chandra","kaito"):
            self.engine.write_text(original.replace('    print(json.dumps(reply), flush=True)',f'    if reply.get("state",{{}}).get("state_type")=="decision":reply["state"]["decision"]["state"]["{family}"]["meaning"]["card_db_hash"]=124\n    print(json.dumps(reply), flush=True)'))
            with self.client()as c:
                with self.assertRaisesRegex(LimitedSessionError,"catalog differs from header"):c.reset(self.decks)
if __name__=="__main__":unittest.main()
