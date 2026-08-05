from __future__ import annotations

import importlib.util
from pathlib import Path
import unittest


MODULE_PATH = Path(__file__).with_name("run_pool3_gate_v4.py")
SPEC = importlib.util.spec_from_file_location("run_pool3_gate_v4", MODULE_PATH)
GATE = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
SPEC.loader.exec_module(GATE)


def row(pair: int, seat: str, reward: int) -> dict:
    return {
        "pair_environment_seed_u64_hex": f"{pair + 1:016x}",
        "episode_id": pair * 2 + int(seat == "p1"),
        "candidate_seat": seat,
        "candidate_terminal_reward": reward,
    }


class Pool3GateV4Tests(unittest.TestCase):
    def panel(self, candidate_rewards: list[int], baseline_rewards: list[int]):
        candidate = {}
        baseline = {}
        for index, (candidate_reward, baseline_reward) in enumerate(
            zip(candidate_rewards, baseline_rewards)
        ):
            pair = index // 2
            seat = "p0" if index % 2 == 0 else "p1"
            key = (pair, index, seat)
            candidate[key] = row(pair, seat, candidate_reward)
            baseline[key] = row(pair, seat, baseline_reward)
        return candidate, baseline

    def test_terminal_order_gate_passes_at_exact_floors(self):
        candidate, baseline = self.panel(
            [-1] * 16 + [0] * 8,
            [0] * 16 + [0] * 8,
        )
        result = GATE.adjudicate(candidate, baseline, GATE.FORMAL_GATES)
        self.assertEqual(result["terminal_order"]["nets"], {"overall": -16, "p0": -8, "p1": -8})
        self.assertTrue(result["pass"])

    def test_each_seat_floor_is_independent(self):
        candidate, baseline = self.panel([-1, 0] * 13, [0, 0] * 13)
        result = GATE.adjudicate(candidate, baseline, GATE.FORMAL_GATES)
        self.assertEqual(result["terminal_order"]["nets"]["p0"], -13)
        self.assertFalse(result["gates"]["p0_terminal_order_net_floor"])
        self.assertFalse(result["pass"])

    def test_bridge_has_no_strength_gate(self):
        candidate, baseline = self.panel([-1, -1], [1, 1])
        result = GATE.adjudicate(candidate, baseline, None)
        self.assertIsNone(result["gates"])
        self.assertTrue(result["pass"])

    def test_receipt_mismatch_is_rejected(self):
        candidate, baseline = self.panel([1, 0], [0, 0])
        baseline[next(iter(baseline))]["pair_environment_seed_u64_hex"] = "f" * 16
        with self.assertRaisesRegex(ValueError, "matched receipt differs"):
            GATE.adjudicate(candidate, baseline, GATE.FORMAL_GATES)


if __name__ == "__main__":
    unittest.main()
