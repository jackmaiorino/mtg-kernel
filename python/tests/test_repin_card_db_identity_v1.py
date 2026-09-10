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
import time
import unittest
from unittest import mock

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


class FindOutDirCandidatesPicksNewestGeneratedFile(unittest.TestCase):
    """Task 11 fix round 1: on Windows/NTFS, overwriting a file's content in
    place does not bump its *parent directory's* own mtime (only adding,
    removing, or renaming an entry does). `find_out_dir_candidates` used to
    sort candidate `out` directories by the directory's own mtime, so once a
    session's `cargo test`/`cargo check` churn had created more than one
    `mtg-kernel-<hash>/out` directory under the same target dir, it could
    hand back a stale `card_defs.rs` from whichever directory happened to be
    *created* last, even though a *different* directory's file had actually
    been *regenerated* more recently. Confirmed by hand during Task 11
    (`E:/cargo-target-pauper-meta`): the directory with the newer own mtime
    held the staler generated content.

    This synthesizes exactly that inverted layout -- the "older" directory's
    own mtime is newer than the "newer" directory's, but the "newer"
    directory's file content and file mtime are genuinely the more recent
    ones -- and asserts the file wins on both `find_out_dir_candidates`'s
    ordering and `parse_live_hash`'s selection.
    """

    def test_newest_generated_file_wins_even_in_an_older_directory(self):
        with tempfile.TemporaryDirectory() as tmp:
            target_dir = pathlib.Path(tmp)
            older_dir = target_dir / "debug" / "build" / "mtg-kernel-older" / "out"
            newer_dir = target_dir / "debug" / "build" / "mtg-kernel-newer" / "out"
            older_dir.mkdir(parents=True)
            newer_dir.mkdir(parents=True)

            older_file = older_dir / "card_defs.rs"
            newer_file = newer_dir / "card_defs.rs"
            older_file.write_text(
                "pub const KERNEL_CARDDB_HASH: u64 = 0x1111_1111_1111_1111;\n",
                encoding="utf-8",
            )
            newer_file.write_text(
                "pub const KERNEL_CARDDB_HASH: u64 = 0x2222_2222_2222_2222;\n",
                encoding="utf-8",
            )

            now = time.time()
            # Inverted on purpose: the "newer" out directory's own mtime is
            # set *older* than the "older" one's (an NTFS directory whose
            # mtime was never bumped by a later in-place file rewrite),
            # while its file content is genuinely the more recently written
            # one (a real regeneration).
            os.utime(newer_dir, (now - 1000, now - 1000))
            os.utime(older_dir, (now - 10, now - 10))
            os.utime(older_file, (now - 2000, now - 2000))
            os.utime(newer_file, (now, now))

            candidates = repin_tool.find_out_dir_candidates(target_dir)
            self.assertEqual(
                candidates[0],
                (newer_dir, "debug"),
                "the directory whose *file* is newest must sort first, "
                "even though its own directory mtime is older",
            )

            live_hash, rs_path, profile = repin_tool.parse_live_hash(target_dir)
            self.assertEqual(live_hash, 0x2222_2222_2222_2222)
            self.assertEqual(rs_path, newer_file)
            self.assertEqual(profile, "debug")


class RewriteProtectedFile(unittest.TestCase):
    """Task 3: FROZEN_CARD_DB_HASH_U64_HEX_PAUPER_META_W1 is the one
    identifier in TASK3_REWRITABLE_IDENTIFIERS_V1, so --write must rewrite
    it like any other pin site while leaving the two PROTECTED_IDENTIFIERS_V1
    constants (FROZEN_CARD_DB_HASH_U64_HEX_CURRENT_V1,
    FROZEN_CARD_DB_HASH_U64_HEX_V2) alone.

    Builds a synthetic file in a temp dir standing in for
    native_training_store_run_v2.rs, then calls the tool's own
    `rewrite_files()` directly (the way the release-layout test above calls
    `parse_live_hash()` directly), with `repin_card_db_identity_v1.REPO_ROOT`
    patched to the temp dir so the file's path resolves to
    PROTECTED_FILE_V1 exactly, the same way `find_occurrences`/
    `rewrite_files` see the real file during a real --write.
    """

    def test_allow_listed_identifier_is_rewritten_protected_ones_are_not(self):
        old_value = 0xAAAA_BBBB_CCCC_DDDD
        new_value = 0x1234_5678_9ABC_DEF0
        old_grouped = repin_tool.grouped_spelling(old_value)
        old_bare = repin_tool.bare_spelling(old_value)
        new_grouped = repin_tool.grouped_spelling(new_value)
        new_bare = repin_tool.bare_spelling(new_value)

        with tempfile.TemporaryDirectory() as tmp:
            repo_root = pathlib.Path(tmp)
            protected_path = repo_root / repin_tool.PROTECTED_FILE_V1
            protected_path.parent.mkdir(parents=True)
            protected_path.write_text(
                "const FROZEN_CARD_DB_HASH_U64_HEX_V2: &str = "
                f'"{old_bare}";\n'
                "const FROZEN_CARD_DB_HASH_U64_HEX_CURRENT_V1: &str = "
                f'"{old_bare}";\n'
                "const FROZEN_CARD_DB_HASH_U64_HEX_PAUPER_META_W1: &str = "
                f'"{old_bare}";\n',
                encoding="utf-8",
            )

            with mock.patch.object(repin_tool, "REPO_ROOT", repo_root):
                changed = repin_tool.rewrite_files(
                    [protected_path], old_grouped, old_bare, new_grouped, new_bare
                )

            self.assertEqual(changed, [repin_tool.PROTECTED_FILE_V1])
            rewritten_lines = protected_path.read_text(encoding="utf-8").splitlines()
            self.assertEqual(
                rewritten_lines[0],
                f'const FROZEN_CARD_DB_HASH_U64_HEX_V2: &str = "{old_bare}";',
                "protected identifier FROZEN_CARD_DB_HASH_U64_HEX_V2 must stay untouched",
            )
            self.assertEqual(
                rewritten_lines[1],
                f'const FROZEN_CARD_DB_HASH_U64_HEX_CURRENT_V1: &str = "{old_bare}";',
                "protected identifier FROZEN_CARD_DB_HASH_U64_HEX_CURRENT_V1 must stay untouched",
            )
            self.assertEqual(
                rewritten_lines[2],
                f'const FROZEN_CARD_DB_HASH_U64_HEX_PAUPER_META_W1: &str = "{new_bare}";',
                "allow-listed identifier FROZEN_CARD_DB_HASH_U64_HEX_PAUPER_META_W1 "
                "must be rewritten to the live hash",
            )


if __name__ == "__main__":
    unittest.main()
