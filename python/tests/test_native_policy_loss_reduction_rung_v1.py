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
    "ad73d06792605703a071dda5fb6366fbc0f4f866841faec12480dc9f7a4a787b"
)
EXPECTED_ARTIFACT_SHA256 = (
    "537f86c8f09b3529fb985efc46306dc139b9ce1cfee1fb32515886d6a7fe2cd7"
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
