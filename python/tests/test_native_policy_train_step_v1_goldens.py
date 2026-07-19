from __future__ import annotations

from pathlib import Path
import subprocess
import sys
import unittest


ROOT = Path(__file__).resolve().parents[2]


class NativePolicyTrainStepV1GoldenTests(unittest.TestCase):
    def test_checked_torch_authority_fixture_has_current_portable_bindings(self) -> None:
        subprocess.run(
            [
                sys.executable,
                str(ROOT / "python" / "tools" / "generate_native_policy_train_step_v1_goldens.py"),
                "--check",
            ],
            cwd=ROOT,
            check=True,
        )


if __name__ == "__main__":
    unittest.main()
