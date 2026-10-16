"""Closed V8 parser/process mock tests. These do not run the Rust native wire."""
from copy import deepcopy
import sys
import unittest
import test_limited_graveyard_return_session_v7 as legacy_fixture
from limited_call_session_v8 import LimitedCallClientV8, _validate_call_visible, _expected_call_meaning, CALL_COUNTERS
from limited_session_v1 import LimitedSessionError
INJECT=r'''
    if reply.get("state",{}).get("state_type")=="decision":
        d=reply["state"]["decision"]
        roster=[];role=0
        for seat,rows in enumerate(d["state"]["graveyard_return"]["battlefield_counters"]):
            current=[]
            for row in rows:
                relation=dict(object_role=role,incarnation_role=role,owner=row["card"]["owner"],controller=row["card"]["controller"],zone="Battlefield");role+=1
                current.append(dict(card=copy.deepcopy(row["card"]),relation=relation,is_token=False,tapped=False,summoning_sick=False,marked_damage=0,counters=dict(**row["counters"],fellowship=0),form=dict(names=[row["card"]["name"]],mana_value=0,types=["Land"],supertypes=["Basic"],subtype_ids=[],color_mask=0,base_power=None,base_toughness=None,power=None,toughness=None,keyword_bits=0,ward_generic=0,minimum_blockers=1,printed_abilities_active=True,abilities_removed=False)))
            roster.append(current)
        d["state"]["call"]=dict(viewer=d["actor"],meaning=copy.deepcopy(CALL_MEANING),visible_printings=[],battlefield=roster,casts=[],deaths=[],returns=[])
        for r in d["action_meanings"]:r["call_returns"]=[]
'''
class LimitedCallClientV8Tests(unittest.TestCase):
    def setUp(self):
        legacy_fixture.LimitedGraveyardReturnClientV7Test.setUp(self)
        source=self.engine.read_text().replace("kernel_limited_graveyard_return_jsonl","kernel_limited_call_jsonl").replace("schema_version=7","schema_version=8").replace("mtg-kernel-limited-graveyard-return-decision/v7","mtg-kernel-limited-call-decision/v8")
        source=source.replace("    print(json.dumps(reply), flush=True)",INJECT+"\n    print(json.dumps(reply), flush=True)")
        self.engine.write_text("CALL_MEANING = "+repr(_expected_call_meaning(123,"c"*64))+"\n"+source)
    def tearDown(self):legacy_fixture.LimitedGraveyardReturnClientV7Test.tearDown(self)
    def client(self,mode="normal"):
        return LimitedCallClientV8([sys.executable,str(self.engine),mode],expected_card_db_hash=123)
    def decision(self):
        with self.client()as c:return c.reset(self.decks)["state"]["decision"]
    def validate(self,d):_validate_call_visible(d["state"],d["actor"],d["action_meanings"],d["actions"])
    def test_mock_both_actor_payment_receipt_restore_and_stale_prompt_contract(self):
        for actor in ("p0","p1"):
            with self.client()as c:
                d=c.reset(self.decks,starting_player=actor);self.assertEqual(d["schema_version"],8);self.assertEqual(d["state"]["decision"]["state"]["call"]["viewer"],actor)
                saved=c.snapshot();restored=c.restore(saved["snapshot_id"])
                with self.assertRaises(LimitedSessionError):c.step(d,0)
                c.step(restored,0)
    def test_complete17_counter_widths_required_and_no_extra_fields(self):
        d=self.decision();row=d["state"]["call"]["battlefield"][0][0]
        self.assertEqual(len(row["counters"]),17)
        for key in CALL_COUNTERS:
            bad=deepcopy(d);del bad["state"]["call"]["battlefield"][0][0]["counters"][key]
            with self.assertRaises(LimitedSessionError,msg=key):self.validate(bad)
        row["counters"]["fellowship"]=(1<<31)-1;self.validate(d)
        row["counters"]["fellowship"]=1<<31
        with self.assertRaises(LimitedSessionError):self.validate(d)
    def test_program_viewer_native_identity_and_candidate_scope_forgery_refuse(self):
        for mode in range(7):
            d=self.decision()
            if mode==0:d["state"]["call"]["meaning"]["ordered_rules"].reverse()
            elif mode==1:d["state"]["call"]["viewer"]="foreign"
            elif mode==2:d["state"]["call"]["private_world"]={}
            elif mode==3:d["action_meanings"][0]["call_returns"]=[None]
            elif mode==4:d["state"]["call"]["meaning"]["model_admitted"]=True
            elif mode==5:d["state"]["call"]["battlefield"][0][0]["relation"]["owner"]="p1"
            else:d["state"]["call"]["battlefield"][0][0]["relation"]["zone"]="Hand"
            with self.assertRaises(LimitedSessionError):self.validate(d)
    def test_same_closed_native_cardtype_and_no_unsigned_narrowing(self):
        d=self.decision();row=d["state"]["call"]["battlefield"][0][0];row["form"]["types"]=["Battle"]
        with self.assertRaises(LimitedSessionError):self.validate(d)
        d=self.decision();row=d["state"]["call"]["battlefield"][0][0];row["counters"]["loyalty"]=(1<<32)-1;d["state"]["graveyard_return"]["battlefield_counters"][0][0]["counters"]["loyalty"]=(1<<32)-1;d["state"]["prepared"]["drake"]["battlefield"][0][0]["counters"]["loyalty"]=(1<<32)-1;self.validate(d)
if __name__=="__main__":unittest.main()
