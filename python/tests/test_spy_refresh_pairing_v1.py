"""Reject scientific input mismatches without running games or search."""
import copy
import importlib.util
from pathlib import Path
import unittest

path = Path(__file__).resolve().parents[2] / "tools/spy_reset/refresh_pilot.py"
spec = importlib.util.spec_from_file_location("refresh_pairing", path)
pilot = importlib.util.module_from_spec(spec)
spec.loader.exec_module(pilot)


def inputs():
    bindings = {"r1": "1" * 64, "r2": "2" * 64}
    banks = {lineage: {} for lineage in bindings}
    rows = []
    for cid in pilot.cases():
        lineage, own, other, seat = cid.split("-")
        row = dict(id=cid, lineage=lineage, own=int(own[1:]), other=int(other[1:]),
                   seat=int(seat[1:]), opp="fixed", seed=101, starting_player=0,
                   runtime_rules="resolution-boundary-v1", select_rule="fpu-1.5", limits={"cap": 512_000})
        row["source"] = dict(end="win", steps=20, eligible_anchors=3, actions_sha256="a" * 64)
        row["anchor"] = dict(index=0, step=7, physical_decision_id=7, k=2,
                             obs_hash="b" * 64, menu_hash="c" * 64, state_hash="d" * 16)
        bank = copy.deepcopy(row)
        bank.update(usable=True, guides={"S": {"end": "win", "starts": [{"k": 2}],
                                               "search": {"execution": {"executed": [[1, 0]]}}},
                                         "P": {"end": "win"}})
        banks[lineage][cid] = bank
        search = dict(search=0, step=7, reason="anchor", budget=128_000, transitions=128_000,
                      incomplete=False, faults=[], tree={"hash": "e" * 64})
        decision = dict(step=7, physical_decision_id=7, substep=0, chooser="new_root",
                        spy_self_offered=False, dr_giant_offered=False,
                        spy_self_chosen=False, dr_giant_chosen=False)
        arm = dict(valid=True, end="win", transitions_used=128_000, searches=[search],
                   decisions=[decision], learner_starts=1)
        r_arm = copy.deepcopy(arm)
        second = dict(decision, step=8, physical_decision_id=8, chooser="reused")
        r_arm["decisions"].append(second)
        row.update(kind="refresh_source", bank_binding_sha256=bindings[lineage], anchor_step=7,
                   allocation=dict(total=512_000, anchor=128_000, refresh=64_000),
                   arms={"R": r_arm, "S128": copy.deepcopy(arm)})
        rows.append(row)
    return banks, rows, bindings


class PairedInputTests(unittest.TestCase):
    def test_complete_authenticated_input_preserves_control_rule(self):
        result = pilot.analyze(*inputs())
        self.assertTrue(result["complete"])
        self.assertEqual(result["disposition"], "advance to the refreshed-bank proposal")

    def test_missing_row_is_incomplete_without_replacement(self):
        banks, rows, bindings = inputs()
        result = pilot.analyze(banks, rows[1:], bindings)
        self.assertFalse(result["complete"])
        self.assertEqual(result["missing"], [rows[0]["id"]])

    def test_invalid_row_cannot_be_replaced_by_later_valid_or_error_row(self):
        for kind in ("refresh_source", "error"):
            banks, rows, bindings = inputs()
            earlier = copy.deepcopy(rows[0])
            earlier["kind"] = kind
            earlier["arms"]["R"]["valid"] = False
            with self.assertRaises(ValueError):
                pilot.analyze(banks, [earlier, *rows], bindings)

    def test_unknown_id_or_missing_frozen_binding_refuses_analysis(self):
        banks, rows, bindings = inputs()
        with self.assertRaises(ValueError):
            pilot.analyze(banks, [*rows, {"id": "unknown"}], bindings)
        with self.assertRaises(ValueError):
            pilot.analyze(banks, rows)

    def test_source_anchor_policy_and_numeric_identity_mismatches_are_reported(self):
        changes = [(None, "seed", 102), (None, "lineage", "r2"),
                   (None, "bank_binding_sha256", "0" * 64),
                   ("source", "actions_sha256", "f" * 64),
                   ("anchor", "obs_hash", "f" * 64), ("anchor", "menu_hash", "f" * 64),
                   ("anchor", "state_hash", "f" * 16), ("anchor", "physical_decision_id", 8),
                   ("anchor", "index", False)]
        for section, field, value in changes:
            with self.subTest(field=field):
                banks, rows, bindings = inputs()
                (rows[0] if section is None else rows[0][section])[field] = value
                result = pilot.analyze(banks, rows, bindings)
                self.assertFalse(result["complete"])
                self.assertEqual(result["invalid"][0]["id"], rows[0]["id"])
                self.assertEqual(result["disposition"], "incomplete: no advance")

    def test_invalid_guide_outcomes_budget_and_anchor_search_cannot_advance(self):
        for change in ("valid", "end", "allocation", "transitions", "shared_tree"):
            with self.subTest(change=change):
                banks, rows, bindings = inputs()
                row = rows[0]
                if change == "valid":
                    row["arms"]["R"]["valid"] = 1
                elif change == "end":
                    row["arms"]["R"]["end"] = "nonnatural"
                elif change == "allocation":
                    row["allocation"]["total"] += 1
                elif change == "transitions":
                    row["arms"]["R"]["transitions_used"] += 1
                else:
                    row["arms"]["S128"]["searches"][0]["tree"]["hash"] = "f" * 64
                result = pilot.analyze(banks, rows, bindings)
                self.assertFalse(result["complete"])
                self.assertTrue(result["invalid"])


if __name__ == "__main__":
    unittest.main()
