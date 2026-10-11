"""Reject invalid paired evidence without running search or altering pilot rows."""
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

path = Path(__file__).resolve().parents[2] / "tools/spy_reset/branch_report.py"
spec = importlib.util.spec_from_file_location("branch_report", path)
report = importlib.util.module_from_spec(spec)
spec.loader.exec_module(report)
CASES = [f"{lin}-o{i % 9}-t0-s{i // 9}" for lin in ("r1", "r2") for i in range(18)]
IDS = CASES + sorted(report.NAMED)
BINDINGS = {"r1": "a" * 64, "r2": "b" * 64}
EXPECTED = {k: dict(root_id=k, source=k.split("@")[0], budget=64000 if "@" in k else 128000,
                    expected_b1_tree="c" * 64,
                    named={"step": int(k.split("@")[1]), "search": 3, "row_sha256": "2" * 64,
                           "bank_binding_sha256": "3" * 64} if "@" in k else None)
            for k in IDS}


def root(k, length, wins, arm):
    lin, own, _, seat = EXPECTED[k]["source"].split("-")
    own, seat = int(own[1:]), int(seat[1:])
    identity = dict(id=EXPECTED[k]["source"], lineage=lin, own=own, seat=seat,
                    other=5, opp="other", seed=123, starting_player=0, runtime_rules="rules",
                    select_rule="rule", limits={"select_cap":128000},
                    bank_binding_sha256=BINDINGS[lin], source={"end": "win", "actions_sha256": "d" * 64},
                    anchor={"step": 50})
    witness = dict(anchor_step=50, step=int(k.split("@")[1]) if "@" in k else 50,
                   physical_decision_id=50, substep=0, k=3, obs_hash="e" * 64, menu_hash="f" * 64,
                   state_hash="1" * 16, selection_root=k, source_actions_sha256="d" * 64,
                   source_identity=identity)
    worlds = [dict(world=i, L=length, first_substep_share=length / 10, first_substep_controlled=length,
                   descendant_controlled=max(0, length - 1), support_after_first_fallback=0, win=i < wins,
                   first_unsupported_group=length, first_miss_group=None, valid=True,
                   end="win" if i < wins else "nonwin", world_state_hash=f"{i:016x}") for i in range(16)]
    return dict(kind="branch_root", root_id=k, source=EXPECTED[k]["source"], lineage=lin, own=own, seat=seat,
                arm=arm, retain=1 if arm == "B1" else 8, budget=EXPECTED[k]["budget"], root=witness,
                bank_binding_sha256=BINDINGS[lin], tree_hash="c" * 64, expected_b1_tree="c" * 64,
                b1_parity=True if arm == "B1" else None, selection_failed=False, timing={"selection": 10.0},
                named_input=EXPECTED[k]["named"],
                evaluation=worlds,
                simulations={"natural": {"count": 100, "revisits_to_tail_nodes": 0, "retained_tail_nodes": 0}},
                tree={"max_physical_depth": 5, "nodes": 50})


def panel(length, wins, arm):
    return {k: root(k, length, wins, arm) for k in IDS}


def compare(a, b):
    return report.compare(a, b, EXPECTED, BINDINGS, CASES)


class BranchReportTest(unittest.TestCase):
    def test_screen_requires_both_lineages(self):
        a, b = panel(3, 8, "B1"), panel(4, 8, "D8")
        self.assertEqual(compare(a, b)["disposition"], "promising mechanism screen")
        for k in [k for k in b if k.startswith("r2")]:
            b[k] = root(k, 2, 8, "D8")
        self.assertEqual(compare(a, b)["disposition"], "screen not met")

    def test_fewer_wins_fail_screen(self):
        self.assertEqual(compare(panel(3, 8, "B1"), panel(4, 7, "D8"))["disposition"], "screen not met")

    def test_missing_panel_and_named_roots_are_incomplete(self):
        for k in (CASES[0], *sorted(report.NAMED)):
            with self.subTest(k=k):
                a, b = panel(3, 8, "B1"), panel(4, 8, "D8")
                del a[k]
                del b[k]
                self.assertIn(k, compare(a, b)["incomplete"])

    def test_contract_corruption_is_incomplete(self):
        for key, value in (("arm", "B1"), ("retain", 1), ("budget", 1), ("source", "r2-o0-t0-s0"),
                           ("bank_binding_sha256", "9" * 64), ("own", 99), ("seat", 1)):
            with self.subTest(key=key):
                a, b = panel(3, 8, "B1"), panel(4, 8, "D8")
                b[CASES[0]][key] = value
                self.assertIn(CASES[0], compare(a, b)["incomplete"])

    def test_false_parity_or_wrong_tree_is_incomplete(self):
        for key, value in (("b1_parity", False), ("tree_hash", "8" * 64)):
            a, b = panel(3, 8, "B1"), panel(4, 8, "D8")
            a[CASES[0]][key] = value
            self.assertIn(CASES[0], compare(a, b)["incomplete"])

    def test_root_witness_difference_is_incomplete(self):
        a, b = panel(3, 8, "B1"), panel(4, 8, "D8")
        b[CASES[0]]["root"]["obs_hash"] = "7" * 64
        self.assertIn(CASES[0], compare(a, b)["incomplete"])

    def test_duplicate_invalid_or_mismatched_world_is_incomplete(self):
        for key, value in (("world", 1), ("valid", False), ("world_state_hash", "f" * 16),
                           ("L", float("nan")), ("end", "truncated"), ("end", "nonwin")):
            a, b = panel(3, 8, "B1"), panel(4, 8, "D8")
            b[CASES[0]]["evaluation"][0][key] = value
            self.assertEqual(compare(a, b)["disposition"], "incomplete")

    def test_world_order_does_not_break_pairing(self):
        a, b = panel(3, 8, "B1"), panel(4, 8, "D8")
        b[CASES[0]]["evaluation"].reverse()
        self.assertTrue(compare(a, b)["complete"])

    def test_substituted_or_extra_root_is_incomplete(self):
        a, b = panel(3, 8, "B1"), panel(4, 8, "D8")
        a["r1-o99-t0-s0"] = a.pop(CASES[0])
        b["r1-o99-t0-s0"] = b.pop(CASES[0])
        self.assertFalse(compare(a, b)["complete"])

    def test_declared_cohort_without_named_roots_is_rejected(self):
        incomplete = {k: v for k, v in EXPECTED.items() if k in CASES}
        with self.assertRaises(ValueError):
            report.compare({}, {}, incomplete, BINDINGS, CASES)

    def test_missing_metadata_and_bad_timing_fail_closed(self):
        for value in (0, float("inf")):
            a, b = panel(3, 8, "B1"), panel(4, 8, "D8")
            b[CASES[0]]["timing"]["selection"] = value
            self.assertFalse(compare(a, b)["complete"])
        a, b = panel(3, 8, "B1"), panel(4, 8, "D8")
        del b[CASES[0]]["root"]["source_identity"]
        self.assertFalse(compare(a, b)["complete"])

    def test_named_saved_input_pin_mismatch_is_incomplete(self):
        a, b = panel(3, 8, "B1"), panel(4, 8, "D8")
        k = sorted(report.NAMED)[0]
        b[k]["named_input"] = {**b[k]["named_input"], "row_sha256": "9" * 64}
        self.assertIn(k, compare(a, b)["incomplete"])

    def test_attribution_rejects_null_failure_flag(self):
        a = panel(3, 8, "B1")
        a[CASES[0]]["selection_failed"] = None
        self.assertFalse(report.attribution(a, EXPECTED, BINDINGS, CASES)["complete"])
        a[CASES[0]]["selection_failed"] = False
        a[CASES[0]]["timing"]["selection"] = float("nan")
        self.assertFalse(report.attribution(a, EXPECTED, BINDINGS, CASES)["complete"])

    def test_cli_incomplete_returns_failure_and_writes_disposition(self):
        with tempfile.TemporaryDirectory() as directory:
            folder = Path(directory)
            for name, data in (("b1", panel(3, 8, "B1")), ("d8", panel(4, 8, "D8"))):
                del data[sorted(report.NAMED)[0]]
                (folder / name).write_text("\n".join(json.dumps(r) for r in data.values()))
            (folder / "roots").write_text("\n".join(json.dumps(r) for r in EXPECTED.values()))
            (folder / "cases").write_text("\n".join(CASES))
            (folder / "bindings").write_text(json.dumps(BINDINGS))
            args = ["--roots", str(folder / "roots"), "--cases", str(folder / "cases"),
                    "--bindings", str(folder / "bindings"), "--out", str(folder / "result")]
            self.assertEqual(report.main(["compare", "--b1", str(folder / "b1"),
                                          "--d8", str(folder / "d8"), *args]), 2)
            self.assertEqual(json.loads((folder / "result").read_text())["disposition"], "incomplete")
            self.assertEqual(report.main(["attribution", "--rows", str(folder / "b1"), *args]), 2)


if __name__ == "__main__":
    unittest.main()
