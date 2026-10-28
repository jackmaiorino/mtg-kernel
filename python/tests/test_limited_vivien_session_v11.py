"""V11 parser/process mocks only. No Rust or native qualification is executed."""
from copy import deepcopy
import sys
import unittest
import test_limited_genesis_wave_session_v10 as inherited
from limited_vivien_session_v11 import LimitedVivienClientV11,_expected_vivien_meaning,_validate_vivien_visible,_contexts
from limited_genesis_wave_session_v10 import _validate_genesis_wave_visible
from limited_session_v1 import LimitedSessionError
INJECT=r'''
    if reply.get("state",{}).get("state_type")=="decision":
        d=reply["state"]["decision"]
        d["state"]["vivien"]=dict(viewer=d["actor"],meaning=copy.deepcopy(VIVIEN_MEANING),visible_printings=[],current=[],activations=[],emblems=[])
        for r in d["action_meanings"]:r["vivien_choices"]=[];r["vivien_activation"]=None
'''
class LimitedVivienClientV11Tests(unittest.TestCase):
    def setUp(self):
        inherited.LimitedGenesisWaveClientV10Tests.setUp(self)
        source=self.engine.read_text().replace("kernel_limited_genesis_wave_jsonl","kernel_limited_vivien_jsonl").replace("schema_version=10","schema_version=11").replace("mtg-kernel-limited-genesis-wave-decision/v10","mtg-kernel-limited-vivien-decision/v11")
        source=source.replace("    print(json.dumps(reply), flush=True)",INJECT+"\n    print(json.dumps(reply), flush=True)")
        self.engine.write_text("VIVIEN_MEANING = "+repr(_expected_vivien_meaning(123,"a"*64))+"\n"+source)
    def tearDown(self):inherited.LimitedGenesisWaveClientV10Tests.tearDown(self)
    def client(self,mode="normal"):return LimitedVivienClientV11([sys.executable,str(self.engine),mode],expected_card_db_hash=123)
    def decision(self):
        with self.client()as c:return c.reset(self.decks)["state"]["decision"]
    def validate(self,d):_validate_vivien_visible(d["state"],d["actor"],d["action_meanings"],d["actions"])
    def look(self):
        d=self.decision();p=d["actor"];r=dict(handle="vivien-original",name="Vivien Reid",owner=p,controller=p,zone="Battlefield")
        a=dict(occurrence=0,source=dict(original_source=r,current_original_source=deepcopy(r),captured_controller=p),ability_index=0,loyalty_before=5,loyalty_after=6,original_targets=[],stage="Looking")
        d["state"]["vivien"]["activations"]=[a]
        for row in d["action_meanings"]:row["vivien_choices"]=_contexts([a])
        return d
    def test_mock_v11_handshake_both_actors_restore_and_stale_prompt(self):
        for p in ("p0","p1"):
            with self.client()as c:
                before=c.reset(self.decks,starting_player=p);self.assertEqual(before["schema_version"],11)
                self.assertEqual(before["state"]["decision"]["state"]["vivien"]["viewer"],p)
                saved=c.snapshot();fresh=c.restore(saved["snapshot_id"])
                with self.assertRaises(LimitedSessionError):c.step(before,0)
                c.step(fresh,0)
    def test_complete_meaning_order_and_u32_loyalty_width(self):
        d=self.look();self.validate(d)
        cost=dict(source=deepcopy(d["state"]["vivien"]["activations"][0]["source"]["original_source"]),ability_index=0,before=5,after=6,paid_before_publication=True,countering_refunds_cost=False)
        d["action_meanings"][0]["vivien_activation"]=cost
        self.validate(d)
        bad=deepcopy(d);bad["action_meanings"][0]["vivien_activation"]["after"]=7
        with self.assertRaises(LimitedSessionError):self.validate(bad)

        a=d["state"]["vivien"]["activations"][0];a.update(loyalty_before=(1<<32)-2,loyalty_after=(1<<32)-1)
        self.validate(d)
        for change in (lambda a:a.update(loyalty_after=1<<32),lambda a:a.update(loyalty_after=True),lambda a:a.update(loyalty_before=5,loyalty_after=7),lambda a:a.update(ability_index=3)):
            bad=deepcopy(d);change(bad["state"]["vivien"]["activations"][0])
            with self.assertRaises(LimitedSessionError):self.validate(bad)
        d=self.look();d["state"]["vivien"]["meaning"]["ordered_rules"].reverse()
        with self.assertRaises(LimitedSessionError):self.validate(d)
    def test_look_action_context_cannot_be_erased_or_claim_another_ability(self):
        d=self.look();d["action_meanings"][0]["vivien_choices"]=[]
        with self.assertRaises(LimitedSessionError):self.validate(d)
        d=self.look();d["state"]["vivien"]["activations"][0]["ability_index"]=2
        with self.assertRaises(LimitedSessionError):self.validate(d)
    def test_emblem_is_distinct_from_departed_unrecorded_or_disposed(self):
        d=self.look();a=d["state"]["vivien"]["activations"][0];a.update(ability_index=2,loyalty_before=8,loyalty_after=0,stage="EmblemCreated")
        d["state"]["vivien"]["emblems"]=[dict(occurrence=0,activation=0,controller=d["actor"],timestamp=123)]
        for row in d["action_meanings"]:row["vivien_choices"]=[]
        self.validate(d)
        for change in (lambda f:f["emblems"].clear(),lambda f:f["activations"][0].update(stage="Disposed"),lambda f:f["emblems"][0].update(activation=1)):
            bad=deepcopy(d);change(bad["state"]["vivien"])
            with self.assertRaises(LimitedSessionError):self.validate(bad)
    def test_unknown_private_world_journal_and_raw_identity_fields_refuse(self):
        for key in ("private_world","history_index","native_stack_item_id","remaining_library"):
            d=self.look();d["state"]["vivien"]["activations"][0][key]=[]
            with self.assertRaises(LimitedSessionError):self.validate(d)
    def test_inherited_full_counter_roster_and_legacy_v10_refusal(self):
        d=self.look()
        with self.assertRaises(LimitedSessionError):_validate_genesis_wave_visible(d["state"],d["actor"],d["action_meanings"],d["actions"])
        d["state"]["call"]["battlefield"][0][0]["counters"].pop("fellowship")
        with self.assertRaises(LimitedSessionError):self.validate(d)
if __name__=="__main__":unittest.main()
