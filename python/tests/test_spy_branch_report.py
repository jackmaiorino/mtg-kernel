"""B1/D8 comparison: frozen screen, pairing and incompleteness without running search."""
import copy
import importlib.util
from pathlib import Path
import unittest

path = Path(__file__).resolve().parents[2] / "tools/spy_reset/branch_report.py"
spec = importlib.util.spec_from_file_location("branch_report", path)
report = importlib.util.module_from_spec(spec)
spec.loader.exec_module(report)


def world(i, L, win):
    return dict(L=L, first_substep_share=L / 10, first_substep_controlled=L, descendant_controlled=max(0, L - 1),
                support_after_first_fallback=0, win=win, first_unsupported_group=L, first_miss_group=None,
                valid=True, world_state_hash=f"{i:016x}")


def root(root_id, L, wins):
    return dict(kind="branch_root", root_id=root_id, lineage=root_id[:2], own=0, selection_failed=False,
                timing={"selection": 10.0},
                evaluation=[world(i, L, i < wins) for i in range(16)],
                simulations={"natural": {"count": 100, "revisits_to_tail_nodes": 0, "retained_tail_nodes": 0}},
                tree={"max_physical_depth": 5, "nodes": 50})


def panel(L, wins):
    return {f"{lin}-o{i % 9}-t0-s{i // 9}": root(f"{lin}-o{i % 9}-t0-s{i // 9}", L, wins)
            for lin in ("r1", "r2") for i in range(18)}


class BranchReportTest(unittest.TestCase):
    def test_screen_requires_both_lineages(self):
        b1, d8 = panel(3, 8), panel(4, 8)
        self.assertEqual(report.compare(b1, d8)["disposition"], "promising mechanism screen")
        for k in [k for k in d8 if k.startswith("r2")]:
            d8[k] = root(k, 2, 8)
        out = report.compare(b1, d8)
        self.assertTrue(out["lineages"]["r1"]["screen"])
        self.assertFalse(out["lineages"]["r2"]["screen"])
        self.assertEqual(out["disposition"], "screen not met")

    def test_fewer_wins_fail_the_screen(self):
        out = report.compare(panel(3, 8), panel(4, 7))
        self.assertEqual(out["disposition"], "screen not met")

    def test_unpaired_worlds_and_missing_roots_are_incomplete(self):
        b1, d8 = panel(3, 8), panel(4, 8)
        k = "r1-o0-t0-s0"
        d8[k] = copy.deepcopy(d8[k])
        d8[k]["evaluation"][0]["world_state_hash"] = "f" * 16
        del d8["r2-o1-t0-s0"]
        out = report.compare(b1, d8)
        self.assertEqual(out["disposition"], "incomplete")
        self.assertEqual(len(out["incomplete"]), 2)

    def test_invalid_world_is_not_a_loss(self):
        b1, d8 = panel(3, 8), panel(4, 8)
        d8["r1-o0-t0-s0"]["evaluation"][15]["valid"] = False
        self.assertIn("r1-o0-t0-s0", report.compare(b1, d8)["incomplete"])


if __name__ == "__main__":
    unittest.main()
