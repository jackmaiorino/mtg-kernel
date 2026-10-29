"""EX1 design v3.3 section 3: the engine's target fixtures equal the Python mirror (tools/ex1/target_ref.py)."""
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[2]))
from tools.ex1 import target_ref  # noqa: E402


class Ex1TargetMirrorTests(unittest.TestCase):
    def test_committed_fixtures_equal_the_mirror(self):
        self.assertEqual(target_ref.FIXTURE_PATH.read_text(encoding="utf-8"), target_ref.render())

    def test_target_shares_only_the_student_mass_on_the_supported_set(self):
        bits = [target_ref.f32bits(x) for x in (3.0, 0.0, -2.0)]
        result = target_ref.target(bits, [100, 100, 2], [0, 100, 2], [0, 0, 0])
        self.assertEqual(result["support"], [0, 1])
        self.assertEqual(result["q"][2], result["p"][2])
        self.assertAlmostEqual(result["q"][0] + result["q"][1], result["mass"], places=15)
        self.assertEqual(target_ref.target(bits, [100, 7, 0], [1, 1, 0], [0, 0, 0]), ("no_target", "support"))

    def test_placement_uses_the_frozen_rates(self):
        self.assertEqual(target_ref.RATES, {"target": 0.155, "cast_or_activate": 0.0311, "other": 0.0261})
        hits = sum(target_ref.placement("r", 0, 1, d, "target") for d in range(20000))
        self.assertLess(abs(hits / 20000 - 0.155), 0.01)


if __name__ == "__main__":
    unittest.main()
