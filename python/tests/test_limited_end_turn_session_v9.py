"""Explicit process/parser mocks for V9. These do not execute Rust gameplay."""
from copy import deepcopy
import sys
import unittest
import test_limited_call_session_v8 as inherited
from limited_end_turn_session_v9 import LimitedEndTurnClientV9, _validate_end_turn_visible, _expected_end_turn_meaning
from limited_session_v1 import LimitedSessionError
INJECT=r'''
    if reply.get("state",{}).get("state_type")=="decision":
        d=reply["state"]["decision"]
        d["state"]["end_turn"]=dict(viewer=d["actor"],meaning=copy.deepcopy(END_TURN_MEANING),visible_printings=[],disposals=[],cleanup=None,compatibility_terminals=[])
        for r in d["action_meanings"]:r.update(end_turn_cleanup=None,disposed_stack_outcomes=[],compatibility_terminals=[])
'''
class LimitedEndTurnClientV9Tests(unittest.TestCase):
    def setUp(self):
        inherited.LimitedCallClientV8Tests.setUp(self)
        source=self.engine.read_text().replace("kernel_limited_call_jsonl","kernel_limited_end_turn_jsonl").replace("schema_version=8","schema_version=9").replace("mtg-kernel-limited-call-decision/v8","mtg-kernel-limited-end-turn-decision/v9")
        source=source.replace("    print(json.dumps(reply), flush=True)",INJECT+"\n    print(json.dumps(reply), flush=True)")
        self.engine.write_text("END_TURN_MEANING = "+repr(_expected_end_turn_meaning(123,"d"*64))+"\n"+source)
    def tearDown(self):inherited.LimitedCallClientV8Tests.tearDown(self)
    def client(self,mode="normal"):
        return LimitedEndTurnClientV9([sys.executable,str(self.engine),mode],expected_card_db_hash=123)
    def decision(self):
        with self.client()as c:return c.reset(self.decks)["state"]["decision"]
    def validate(self,d):_validate_end_turn_visible(d["state"],d["actor"],d["action_meanings"],d["actions"])
    def disposed(self):
        d=self.decision();ref=dict(handle="archive-0",name="Time Stop",owner=d["actor"],controller=d["actor"],zone="Stack")
        original=dict(stack_index=0,source=ref,controller=d["actor"],targets=[],stack_item_kind="spell",is_copy=False,is_flashback=False,mode_chosen=0,madness_offer=False,kicked=False,cast_method="normal",face_index=0,x_value=65535,paid_cost_refs=[])
        disposal=dict(occurrence=0,ending_spell_position=0,original_step="Main1",original_active_player=d["actor"],disposed_rows=[dict(original_stack_position=0,original=original)])
        d["state"]["end_turn"]["disposals"]=[disposal]
        for row in d["action_meanings"]:row["disposed_stack_outcomes"]=deepcopy([disposal])
        return d
    def test_mock_distinct_handshake_both_actor_restore_stale_and_receipt(self):
        for actor in ("p0","p1"):
            with self.client()as c:
                d=c.reset(self.decks,starting_player=actor);self.assertEqual(d["schema_version"],9)
                self.assertEqual(d["state"]["decision"]["state"]["end_turn"]["viewer"],actor)
                saved=c.snapshot();restored=c.restore(saved["snapshot_id"])
                with self.assertRaises(LimitedSessionError):c.step(d,0)
                c.step(restored,0)
    def test_complete_disposed_row_width_and_native_terminal_order(self):
        d=self.disposed();self.validate(d)
        for mutate in (lambda x:x["state"]["end_turn"]["disposals"][0].update(ending_spell_position=1),
                       lambda x:x["state"]["end_turn"]["disposals"][0]["disposed_rows"][0]["original"].update(x_value=65536),
                       lambda x:x["state"]["end_turn"]["disposals"][0]["disposed_rows"][0]["original"].update(private_world={}),
                       lambda x:x["state"]["end_turn"].update(disposed=True),
                       lambda x:x["state"]["end_turn"]["disposals"][0]["disposed_rows"][0].update(original_stack_position=True)):
            bad=deepcopy(d);mutate(bad)
            with self.assertRaises(LimitedSessionError):self.validate(bad)
    def test_cleanup_bound_to_actual_disposal_and_every_candidate(self):
        d=self.disposed();cleanup=dict(disposal_occurrence=0,turn=(1<<32)-1,active_player=d["actor"],phase="Discard",discard_count=5)
        d["state"]["end_turn"]["cleanup"]=cleanup
        for row in d["action_meanings"]:row["end_turn_cleanup"]=deepcopy(cleanup)
        self.validate(d)
        for field,value in (("disposal_occurrence",1),("discard_count",0),("phase","Finished"),("turn",1<<32)):
            bad=deepcopy(d);bad["state"]["end_turn"]["cleanup"][field]=value
            with self.assertRaises(LimitedSessionError):self.validate(bad)
        bad=deepcopy(d);bad["action_meanings"][0]["disposed_stack_outcomes"]=[]
        with self.assertRaises(LimitedSessionError):self.validate(bad)
    def test_complete_ordered_meaning_private_fields_and_missing_context_refuse(self):
        for mutate in (lambda x:x["state"]["end_turn"]["meaning"]["ordered_rules"].reverse(),
                       lambda x:x["state"]["end_turn"]["meaning"].update(sampler_admitted=True),
                       lambda x:x["state"]["end_turn"].update(viewer="foreign"),
                       lambda x:x["action_meanings"][0].pop("end_turn_cleanup"),
                       lambda x:x["state"]["end_turn"].update(original_history=[])):
            d=self.decision();mutate(d)
            with self.assertRaises(LimitedSessionError):self.validate(d)
    def test_disposed_compatibility_role_cannot_invent_history_or_another_outcome(self):
        for target in ({"DrakeCombat":{"occurrence":0}},{"UnknownFamily":{"position":0}}):
            d=self.disposed();terminal=dict(target=target,disposal_occurrence=0,original_stack_position=0,outcome="Disposed")
            d["state"]["end_turn"]["compatibility_terminals"]=[terminal]
            for row in d["action_meanings"]:row["compatibility_terminals"]=[deepcopy(terminal)]
            with self.assertRaises(LimitedSessionError):self.validate(d)
    def test_inherited_complete_seventeen_counters_remain_required(self):
        d=self.decision();self.validate(d)
        self.assertEqual(len(d["state"]["call"]["battlefield"][0][0]["counters"]),17)
        for key in inherited.CALL_COUNTERS:
            bad=deepcopy(d);bad["state"]["call"]["battlefield"][0][0]["counters"].pop(key)
            with self.assertRaises(LimitedSessionError):self.validate(bad)
if __name__=="__main__":unittest.main()
