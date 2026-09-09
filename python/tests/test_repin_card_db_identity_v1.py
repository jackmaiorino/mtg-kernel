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
import tempfile
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[2]
TOOLS = ROOT / "python" / "tools"
if str(TOOLS) not in sys.path:
    sys.path.insert(0, str(TOOLS))

import repin_card_db_identity_v1 as repin_tool  # noqa: E402


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


class ParseLiveHashProfileLayout(unittest.TestCase):
    def test_finds_a_release_only_out_dir(self):
        # The repo's CI (.github/workflows/ci.yml, python-tests job) only
        # ever runs `cargo build --release --locked --bin kernel_rl_env`,
        # never a plain (debug) `cargo build`, and never sets
        # CARGO_TARGET_DIR, so only target/release/build/mtg-kernel-*/out
        # exists there. This reproduces that shape with a synthetic target
        # directory (no debug/ subtree at all) instead of a real build.
        with tempfile.TemporaryDirectory() as tmp:
            target_dir = pathlib.Path(tmp)
            out_dir = target_dir / "release" / "build" / "mtg-kernel-abc" / "out"
            out_dir.mkdir(parents=True)
            generated = out_dir / "generated.rs"
            generated.write_text(
                "pub const KERNEL_CARDDB_HASH: u64 = 0xdead_beef_cafe_f00d;\n",
                encoding="utf-8",
            )
            live_hash, rs_path, profile = repin_tool.parse_live_hash(target_dir)
            self.assertEqual(live_hash, 0xDEAD_BEEF_CAFE_F00D)
            self.assertEqual(rs_path, generated)
            self.assertEqual(profile, "release")


if __name__ == "__main__":
    unittest.main()
