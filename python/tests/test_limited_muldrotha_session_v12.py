"""V12 process/parser mocks only. No Rust gameplay or admission is exercised."""
from copy import deepcopy
import sys
import unittest
import test_limited_vivien_session_v11 as inherited
from limited_muldrotha_session_v12 import (
    LimitedMuldrothaClientV12, _expected_muldrotha_meaning,
    _validate_muldrotha_visible, TYPES,
)
from limited_vivien_session_v11 import _validate_vivien_visible
from limited_session_v1 import LimitedSessionError

INJECT = r'''
    if reply.get("state",{}).get("state_type")=="decision":
        d=reply["state"]["decision"]
        d["state"]["muldrotha"]=dict(viewer=d["actor"],meaning=copy.deepcopy(MULDROTHA_MEANING),visible_printings=[],current=[],budgets=[],plays=[])
        for row in d["action_meanings"]:row["muldrotha_permission"]=None
'''

class LimitedMuldrothaClientV12Tests(unittest.TestCase):
    def setUp(self):
        inherited.LimitedVivienClientV11Tests.setUp(self)
        source = self.engine.read_text().replace(
            "kernel_limited_vivien_jsonl", "kernel_limited_muldrotha_jsonl"
        ).replace("schema_version=11", "schema_version=12").replace(
            "mtg-kernel-limited-vivien-decision/v11",
            "mtg-kernel-limited-muldrotha-decision/v12",
        )
        source = source.replace("    print(json.dumps(reply), flush=True)", INJECT + "\n    print(json.dumps(reply), flush=True)")
        self.engine.write_text("MULDROTHA_MEANING = " + repr(_expected_muldrotha_meaning(123, "a"*64)) + "\n" + source)

    def tearDown(self):
        inherited.LimitedVivienClientV11Tests.tearDown(self)

    def client(self, mode="normal"):
        return LimitedMuldrothaClientV12([sys.executable, str(self.engine), mode], expected_card_db_hash=123)

    def decision(self):
        with self.client() as c:
            return c.reset(self.decks)["state"]["decision"]

    def validate(self, d):
        _validate_muldrotha_visible(d["state"], d["actor"], d["action_meanings"], d["actions"])

    def permission(self):
        d = self.decision()
        p = d["actor"]
        source = dict(handle="muldrotha-original", name="Muldrotha, the Gravetide", owner=p, controller=p, zone="Battlefield")
        root = dict(original_source=source, current_original_source=deepcopy(source), captured_controller=p)
        card = dict(handle="graveyard-elf", name="Llanowar Elves", owner=p, controller=p, zone="Graveyard")
        permission = dict(source=root, permanent_type="Creature")
        f = d["state"]["muldrotha"]
        f["budgets"] = [dict(source=deepcopy(root), turn=1, active_own_turn=True, printed_abilities_active=True, unused=TYPES.copy(), spent=[])]
        f["plays"] = [dict(occurrence=0, card=card, current_original=deepcopy(card), controller=p, turn=1, land=False, ordinary_available=False, permissions=[permission], selected_menu_index=None, stage="PermissionChoice")]
        d["actions"] = [dict(action_index=0, label="Use Creature permission")]
        d["action_meanings"] = [deepcopy(d["action_meanings"][0])]
        d["action_meanings"][0]["muldrotha_permission"] = dict(occurrence=0, menu_index=0, permission=deepcopy(permission))
        return d

    def test_mock_v12_both_actors_handshake_restore_and_stale_prompt(self):
        for actor in ("p0", "p1"):
            with self.client() as c:
                before = c.reset(self.decks, starting_player=actor)
                self.assertEqual(before["schema_version"], 12)
                self.assertEqual(before["state"]["decision"]["state"]["muldrotha"]["viewer"], actor)
                saved = c.snapshot()
                fresh = c.restore(saved["snapshot_id"])
                with self.assertRaises(LimitedSessionError):
                    c.step(before, 0)
                c.step(fresh, 0)

    def test_exact_permission_binding_type_menu_and_original_source(self):
        d = self.permission()
        self.validate(d)
        changes = (
            lambda d: d["action_meanings"][0].update(muldrotha_permission=None),
            lambda d: d["action_meanings"][0]["muldrotha_permission"].update(menu_index=1),
            lambda d: d["action_meanings"][0]["muldrotha_permission"]["permission"].update(permanent_type="Artifact"),
            lambda d: d["state"]["muldrotha"]["plays"][0]["permissions"][0]["source"].update(captured_controller="p1" if d["actor"] == "p0" else "p0"),
        )
        for change in changes:
            bad = deepcopy(d)
            change(bad)
            with self.assertRaises(LimitedSessionError):
                self.validate(bad)

    def test_burnished_hart_native_creature_artifact_menu_accepts_actual_human_label_order(self):
        d = self.permission()
        play = d["state"]["muldrotha"]["plays"][0]
        play["card"]["name"] = "Burnished Hart"
        play["current_original"] = deepcopy(play["card"])
        creature = play["permissions"][0]
        artifact = deepcopy(creature)
        artifact["permanent_type"] = "Artifact"
        # Genuine native menu: Creature0, Artifact1. The Rust Human projector
        # sorts its actual labels to Artifact1, Creature0 and retains indexes.
        play["permissions"] = [creature, artifact]
        row = deepcopy(d["action_meanings"][0])
        d["action_meanings"] = [deepcopy(row), deepcopy(row)]
        d["action_meanings"][0]["muldrotha_permission"] = dict(occurrence=0, menu_index=1, permission=deepcopy(artifact))
        d["action_meanings"][1]["muldrotha_permission"] = dict(occurrence=0, menu_index=0, permission=deepcopy(creature))
        d["actions"] = [dict(action_index=i, label=f"Use the {kind} permission from Muldrotha, the Gravetide")
                        for i, kind in enumerate(("Artifact", "Creature"))]
        self.validate(d)
        for change in (
            lambda d: d["action_meanings"][1].update(muldrotha_permission=deepcopy(d["action_meanings"][0]["muldrotha_permission"])),
            lambda d: d["action_meanings"][0]["muldrotha_permission"].update(menu_index=2),
            lambda d: d["action_meanings"][0]["muldrotha_permission"].update(menu_index=0),
        ):
            bad=deepcopy(d)
            change(bad)
            with self.assertRaises(LimitedSessionError):self.validate(bad)

    def test_native_ordinary_zero_can_sort_after_typed_permission_without_rebinding(self):
        d = self.permission()
        play = d["state"]["muldrotha"]["plays"][0]
        # Ordinary/native Greatwurm and Muldrotha routes are distinct. This is
        # a transport-only menu fixture and does not qualify that printing.
        play["card"]["name"] = "Quilled Greatwurm"
        play["current_original"] = deepcopy(play["card"])
        play["ordinary_available"] = True
        permission=deepcopy(play["permissions"][0])
        row=deepcopy(d["action_meanings"][0])
        d["action_meanings"] = [deepcopy(row),deepcopy(row)]
        d["action_meanings"][0]["muldrotha_permission"] = dict(occurrence=0, menu_index=1, permission=permission)
        d["action_meanings"][1]["muldrotha_permission"] = dict(occurrence=0, menu_index=0, permission=None)
        d["actions"] = [dict(action_index=0,label="Use the Creature permission from Muldrotha, the Gravetide"),
                        dict(action_index=1,label="Use the ordinary graveyard play route")]
        self.validate(d)
        bad=deepcopy(d)
        bad["action_meanings"][1]["muldrotha_permission"]["menu_index"]=1
        with self.assertRaises(LimitedSessionError):self.validate(bad)

    def test_land_cannot_use_artifact_budget_and_no_implicit_ordinary_route(self):
        d = self.permission()
        d["state"]["muldrotha"]["plays"][0]["land"] = True
        with self.assertRaises(LimitedSessionError):
            self.validate(d)
        d = self.permission()
        d["state"]["muldrotha"]["plays"][0].update(selected_menu_index=0, stage="OrdinaryRoute")
        with self.assertRaises(LimitedSessionError):
            self.validate(d)

    def test_budget_omission_duplicate_and_printed_meaning_mutations_refuse(self):
        d = self.permission()
        for change in (
            lambda f: f["budgets"][0]["unused"].pop(),
            lambda f: f["budgets"][0]["spent"].append("Creature"),
            lambda f: f["meaning"]["ordered_rules"].reverse(),
            lambda f: f["meaning"]["printed"].update(power=7),
        ):
            bad = deepcopy(d)
            change(bad["state"]["muldrotha"])
            with self.assertRaises(LimitedSessionError):
                self.validate(bad)

    def test_private_history_world_and_raw_operands_cannot_enter_transport(self):
        for key in ("history_index", "options_history_index", "private_world", "native_stack_item_id"):
            d = self.permission()
            d["state"]["muldrotha"]["plays"][0][key] = 0
            with self.assertRaises(LimitedSessionError):
                self.validate(d)

    def test_inherited_vivien_and_full_counters_preserved_old_v11_refuses(self):
        d = self.permission()
        with self.assertRaises(LimitedSessionError):
            _validate_vivien_visible(d["state"], d["actor"], d["action_meanings"], d["actions"])
        d["state"]["call"]["battlefield"][0][0]["counters"].pop("fellowship")
        with self.assertRaises(LimitedSessionError):
            self.validate(d)

if __name__ == "__main__":
    unittest.main()
