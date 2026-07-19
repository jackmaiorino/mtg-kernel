from __future__ import annotations

import hashlib
from pathlib import Path
import subprocess
import sys
import unittest


ROOT = Path(__file__).resolve().parents[2]
GENERATOR = ROOT / "python" / "tools" / "generate_native_policy_loss_reduction_rung_v1.py"
ARTIFACT = (
    ROOT
    / "data"
    / "native_policy_train_step_v1"
    / "loss_reduction_intermediate_rung_v1.json"
)
EXPECTED_GENERATOR_SHA256 = (
    "486ef731d60cc7fa6044c4c3b2b00823915bc8b6179cd87a251de0c2c6bbf2a4"
)
EXPECTED_ARTIFACT_SHA256 = (
    "472b0b56eb772b7b78401d2ea676121f215ab970ef62e012adb611f7c9f0adc1"
)


class NativePolicyLossReductionRungV1Tests(unittest.TestCase):
    def test_intermediate_rung_is_hash_pinned_and_recomputable(self) -> None:
        self.assertEqual(
            hashlib.sha256(GENERATOR.read_bytes()).hexdigest(),
            EXPECTED_GENERATOR_SHA256,
        )
        self.assertEqual(
            hashlib.sha256(ARTIFACT.read_bytes()).hexdigest(),
            EXPECTED_ARTIFACT_SHA256,
        )
        subprocess.run(
            [sys.executable, str(GENERATOR), "--check"],
            cwd=ROOT,
            check=True,
        )


if __name__ == "__main__":
    unittest.main()
