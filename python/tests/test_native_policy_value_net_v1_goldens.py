from __future__ import annotations

from pathlib import Path
import subprocess
import sys
import unittest


ROOT = Path(__file__).resolve().parents[2]
GENERATOR = (
    ROOT / "python" / "tools" / "generate_native_policy_value_net_v1_goldens.py"
)


class NativePolicyValueNetV1GoldenTests(unittest.TestCase):
    def test_checked_in_fixture_matches_torch_model_authority(self) -> None:
        completed = subprocess.run(
            [sys.executable, str(GENERATOR), "--check"],
            cwd=ROOT,
            capture_output=True,
            text=True,
            check=False,
        )
        self.assertEqual(
            completed.returncode,
            0,
            f"native model golden check failed:\n"
            f"stdout={completed.stdout}\nstderr={completed.stderr}",
        )


if __name__ == "__main__":
    unittest.main()
