import json
import subprocess
import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
TOOL = ROOT / "python/tools/paired_sideboard_estimator_v1.py"


class PairedSideboardEstimator(unittest.TestCase):
    def _run(self, deltas, resample_count, seed, sidedness):
        deltas_path = Path(self.tmp) / "deltas.jsonl"
        deltas_path.write_text("\n".join(json.dumps({"delta": d}) for d in deltas), encoding="utf-8")
        result = subprocess.run(
            [
                sys.executable, str(TOOL),
                "--deltas-jsonl", str(deltas_path),
                "--resample-count", str(resample_count),
                "--seed", str(seed),
                "--sidedness", sidedness,
            ],
            cwd=ROOT, capture_output=True, text=True,
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        return json.loads(result.stdout)

    def setUp(self):
        import tempfile
        self.tmp = tempfile.mkdtemp()

    def test_mean_matches_hand_computed_value(self):
        report = self._run([1, 1, 0, -1, 1], resample_count=500, seed=1, sidedness="one_sided_lower")
        self.assertAlmostEqual(report["mean"], 0.4, places=12)

    def test_two_sided_brackets_a_constant_series_exactly(self):
        report = self._run([1] * 20, resample_count=500, seed=1, sidedness="two_sided")
        self.assertEqual(report["mean"], 1.0)
        self.assertEqual(report["lower"], 1.0)
        self.assertEqual(report["upper"], 1.0)

    def test_same_seed_reproduces_bit_identically(self):
        a = self._run([1, 0, -1, 1, 1, 0, 1], resample_count=1000, seed=99, sidedness="one_sided_lower")
        b = self._run([1, 0, -1, 1, 1, 0, 1], resample_count=1000, seed=99, sidedness="one_sided_lower")
        self.assertEqual(a, b)


if __name__ == "__main__":
    unittest.main()
