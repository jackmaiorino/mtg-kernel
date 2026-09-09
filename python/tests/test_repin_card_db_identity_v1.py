"""Tests for python/tools/repin_card_db_identity_v1.py.

`--check` needs a built mtg-kernel crate (it reads the generated
`KERNEL_CARDDB_HASH` constant rather than recomputing it). If the caller's
environment already names a Cargo target dir via `CARGO_TARGET_DIR`, forward
it to the tool through `MTG_KERNEL_CARGO_TARGET_DIR` so the same build is
reused instead of falling back to the tool's "target" default.
"""

import os
import pathlib
import subprocess
import sys
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[2]


class RepinTool(unittest.TestCase):
    def test_check_mode_passes_on_a_consistent_tree(self):
        env = dict(os.environ)
        cargo_target_dir = os.environ.get("CARGO_TARGET_DIR")
        if cargo_target_dir:
            env["MTG_KERNEL_CARGO_TARGET_DIR"] = cargo_target_dir
        result = subprocess.run(
            [sys.executable, "python/tools/repin_card_db_identity_v1.py", "--check"],
            cwd=ROOT,
            capture_output=True,
            text=True,
            env=env,
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("card_db_hash", result.stdout)


if __name__ == "__main__":
    unittest.main()
