"""Closed V10 parser/process mocks. These never execute Rust gameplay."""
from copy import deepcopy
import sys
import unittest
import test_limited_end_turn_session_v9 as inherited
from limited_genesis_wave_session_v10 import LimitedGenesisWaveClientV10,_validate_genesis_wave_visible,_expected_genesis_wave_meaning,_wave_contexts
from limited_end_turn_session_v9 import _validate_end_turn_visible
from limited_session_v1 import LimitedSessionError
INJECT=r'''
    if reply.get("state",{}).get("state_type")=="decision":
        d=reply["state"]["decision"]
        d["state"]["genesis_wave"]=dict(viewer=d["actor"],meaning=copy.deepcopy(WAVE_MEANING),visible_printings=[],waves=[])
        for r in d["action_meanings"]:r["genesis_waves"]=[]
'''
class LimitedGenesisWaveClientV10Tests(unittest.TestCase):
    def setUp(self):
        inherited.LimitedEndTurnClientV9Tests.setUp(self)
        source=self.engine.read_text().replace("kernel_limited_end_turn_jsonl","kernel_limited_genesis_wave_jsonl").replace("schema_version=9","schema_version=10").replace("mtg-kernel-limited-end-turn-decision/v9","mtg-kernel-limited-genesis-wave-decision/v10")
        source=source.replace("    print(json.dumps(reply), flush=True)",INJECT+"\n    print(json.dumps(reply), flush=True)")
        self.engine.write_text("WAVE_MEANING = "+repr(_expected_genesis_wave_meaning(123,"e"*64))+"\n"+source)
    def tearDown(self):inherited.LimitedEndTurnClientV9Tests.tearDown(self)
    def client(self,mode="normal"):
        return LimitedGenesisWaveClientV10([sys.executable,str(self.engine),mode],expected_card_db_hash=123)
    def decision(self):
        with self.client()as c:return c.reset(self.decks)["state"]["decision"]
    def validate(self,d):_validate_genesis_wave_visible(d["state"],d["actor"],d["action_meanings"],d["actions"])
    def wave(self):
        d=self.decision();p=d["actor"]
        source=dict(handle="genesis-original",name="Genesis Wave",owner=p,controller=p,zone="Stack")
        card=dict(handle="revealed-0",name="Forest",owner=p,controller=p,zone="Library")
        wave=dict(occurrence=0,stage="Subset",source=source,controller=p,x_value=65535,is_copy=False,is_flashback=False,cast_method="normal",revealed=[dict(card=card,mana_value=0,permanent=True,eligible=True,aura=False)],selected_indices=[],attachments=[],failed_aura_indices=[],remainder_indices=[],answer_indices=[],answer_host=None,answer_waits_for_advance=False,spell_departure=None)
        d["state"]["genesis_wave"]["waves"]=[wave]
        for row in d["action_meanings"]:row["genesis_waves"]=_wave_contexts([deepcopy(wave)])
        return d
    def test_mock_v10_actor_restore_stale_and_receipt_contract(self):
        for p in ("p0","p1"):
            with self.client()as c:
                d=c.reset(self.decks,starting_player=p);self.assertEqual(d["schema_version"],10)
                self.assertEqual(d["state"]["decision"]["state"]["genesis_wave"]["viewer"],p)
                saved=c.snapshot();fresh=c.restore(saved["snapshot_id"])
                with self.assertRaises(LimitedSessionError):c.step(d,0)
                c.step(fresh,0)
    def test_complete_program_and_native_u16_width_are_preserved(self):
        d=self.wave();self.validate(d)
        for change in (lambda w:w.update(x_value=65536),lambda w:w.update(x_value=True),lambda w:w["revealed"][0].update(mana_value=65536),lambda w:w.update(selected_indices=[1]),lambda w:w.update(answer_indices=[0,0])):
            bad=deepcopy(d);change(bad["state"]["genesis_wave"]["waves"][0])
            with self.assertRaises(LimitedSessionError):self.validate(bad)
    def test_private_library_journal_and_unknown_fields_are_refused(self):
        for field in ("original_library","remaining_library","history_index","private_world","native_stack_item_id"):
            d=self.wave();d["state"]["genesis_wave"]["waves"][0][field]=[]
            with self.assertRaises(LimitedSessionError,msg=field):self.validate(d)
        d=self.wave();d["state"]["genesis_wave"]["meaning"]["ordered_rules"].reverse()
        with self.assertRaises(LimitedSessionError):self.validate(d)
    def test_candidate_full_context_and_original_subset_cannot_be_erased(self):
        d=self.wave();d["action_meanings"][0]["genesis_waves"]=[]
        with self.assertRaises(LimitedSessionError):self.validate(d)
        d=self.wave();d["state"]["genesis_wave"]["waves"][0]["selected_indices"]=[0,0]
        with self.assertRaises(LimitedSessionError):self.validate(d)
        d=self.wave();d["state"]["call"]["battlefield"][0][0]["counters"].pop("fellowship")
        with self.assertRaises(LimitedSessionError):self.validate(d)
    def test_legacy_v9_refuses_new_scope_and_inherited_context_is_mandatory(self):
        d=self.wave()
        with self.assertRaises(LimitedSessionError):_validate_end_turn_visible(d["state"],d["actor"],d["action_meanings"],d["actions"])
        d["state"].pop("end_turn")
        with self.assertRaises(LimitedSessionError):self.validate(d)
    def test_closed_aura_host_and_terminal_values_match_current_context(self):
        d=self.wave();w=d["state"]["genesis_wave"]["waves"][0];w["stage"]={"AuraHost":{"selected_index":0}};w["revealed"][0].update(aura=True);w["selected_indices"]=[0]
        for row in d["action_meanings"]:row["genesis_waves"]=_wave_contexts([deepcopy(w)])
        self.validate(d)
        bad=deepcopy(d);bad["state"]["genesis_wave"]["waves"][0]["stage"]={"AuraHost":{"selected_index":65536}}
        with self.assertRaises(LimitedSessionError):self.validate(bad)
        bad=deepcopy(d);bad["state"]["genesis_wave"]["waves"][0]["spell_departure"]="Graveyard"
        with self.assertRaises(LimitedSessionError):self.validate(bad)
if __name__=="__main__":unittest.main()
