from __future__ import annotations

import importlib.util
import os
from pathlib import Path
import unittest
from unittest import mock


ROOT = Path(__file__).resolve().parents[2]
MODULE_PATH = (
    ROOT
    / "python"
    / "tools"
    / "evaluate_native_policy_loss_reduction_true_k512_v1.py"
)
SPEC = importlib.util.spec_from_file_location("true_k512_loss_gate_v1", MODULE_PATH)
if SPEC is None or SPEC.loader is None:
    raise RuntimeError("true-K512 gate module cannot be loaded")
gate = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(gate)


class TrueK512LossGateLogicV1Tests(unittest.TestCase):
    def test_sum_margin_floor_is_fail_closed_without_tolerance_change(self) -> None:
        expected = 100.0
        allowed = gate.LOSS_ABSOLUTE_TOLERANCE + (
            gate.LOSS_RELATIVE_TOLERANCE * abs(expected)
        )
        exact_floor = gate._comparison_record(
            "policy_sum", expected, expected + allowed / gate.SUM_MARGIN_FLOOR
        )
        self.assertTrue(exact_floor["tolerance_holds"])
        self.assertGreaterEqual(
            exact_floor["margin_ratio_allowed_over_delta_f64"],
            gate.SUM_MARGIN_FLOOR,
        )
        self.assertTrue(exact_floor["gate_holds"])

        below_floor = gate._comparison_record(
            "value_sum", expected, expected + allowed / 4.0
        )
        self.assertTrue(below_floor["tolerance_holds"])
        self.assertFalse(below_floor["margin_floor_holds"])
        self.assertFalse(below_floor["gate_holds"])

        loss_same_margin = gate._comparison_record(
            "loss", expected, expected + allowed / 4.0
        )
        self.assertTrue(loss_same_margin["tolerance_holds"])
        self.assertFalse(loss_same_margin["margin_floor_applies"])
        self.assertTrue(loss_same_margin["gate_holds"])

    def test_terminal_diagnostic_names_every_triggered_sum_channel(self) -> None:
        comparisons = {
            "policy_sum": gate._comparison_record("policy_sum", 1.0, 1.00004),
            "value_sum": gate._comparison_record("value_sum", 100.0, 100.0015),
            "loss": gate._comparison_record("loss", 0.25, 0.25),
        }
        record = gate._gate_record(comparisons)
        self.assertEqual(record["status"], "repair_required")
        self.assertEqual(record["diagnostic_code"], gate.REPAIR_DIAGNOSTIC)
        self.assertIn("policy_sum:margin_below_5x", record["triggered_conditions"])
        self.assertIn("value_sum:margin_below_5x", record["triggered_conditions"])
        self.assertFalse(record["silent_tolerance_loosening_permitted"])
        self.assertEqual(len(record["predeclared_repair_paths"]), 2)

    def test_portable_f32_reduction_is_ordered_and_count_sensitive(self) -> None:
        policy = [gate._f32(0.1), gate._f32(-0.2), gate._f32(0.3)]
        value = [gate._f32(1.0), gate._f32(0.25), gate._f32(4.0)]
        policy_sum, value_sum, loss = gate._sequential_reduction(policy, value)
        self.assertEqual(
            gate._bits_hex(policy_sum),
            gate._bits_hex(
                gate._f32_add(gate._f32_add(0.0, policy[0]), gate._f32_add(policy[1], 0.0))
                + policy[2]
            ),
        )
        expected_loss = gate._f32_div(
            gate._f32_add(
                policy_sum,
                gate._f32_mul(gate.VALUE_COEFFICIENT, value_sum),
            ),
            gate._f32(3.0),
        )
        self.assertEqual(gate._bits_hex(loss), gate._bits_hex(expected_loss))

    def test_python_git_lookup_strips_ambient_git_routing(self) -> None:
        with mock.patch.dict(
            os.environ,
            {
                "GIT_DIR": "malicious-dir",
                "gIt_work_tree": "malicious-worktree",
                "MTG_GIT_DIR": "preserved-non-routing-name",
            },
            clear=False,
        ):
            sanitized = gate._sanitized_git_environment()
        self.assertFalse(any(name[:4].lower() == "git_" for name in sanitized))
        self.assertEqual(sanitized["MTG_GIT_DIR"], "preserved-non-routing-name")


if __name__ == "__main__":
    unittest.main()
