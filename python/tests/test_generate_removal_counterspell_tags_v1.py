"""Tests for python/tools/generate_removal_counterspell_tags_v1.py.

The write-mode tests point the tool's `ROOT`/`REGISTRY_PATH`/`OUTPUT_PATH` at
a temporary directory copy of `data/cards_v1.json` (the same isolation
`test_repin_card_db_identity_v1.py`'s `RewriteProtectedFile` test uses via
`mock.patch.object` on the tool module) instead of running `--write` against
the real repository, so the suite never dirties the tracked
`data/pauper_removal_counterspell_tags_v1.json` and the idempotence test can
actually fail if `--check` regresses. A separate `--check`-only test runs
against the real, committed file with no preceding `--write`, so it is the
one test that would actually catch silent drift between `data/cards_v1.json`
and the tracked tag file.
"""

import json
import pathlib
import shutil
import subprocess
import sys
import tempfile
import unittest
from unittest import mock

ROOT = pathlib.Path(__file__).resolve().parents[2]
TOOLS = ROOT / "python" / "tools"
if str(TOOLS) not in sys.path:
    sys.path.insert(0, str(TOOLS))

import generate_removal_counterspell_tags_v1 as tags_tool  # noqa: E402

TOOL_SCRIPT = ROOT / "python/tools/generate_removal_counterspell_tags_v1.py"
COMMITTED_OUTPUT = ROOT / "data/pauper_removal_counterspell_tags_v1.json"


def _isolated_repo_root() -> tempfile.TemporaryDirectory:
    tmp = tempfile.TemporaryDirectory()
    data_dir = pathlib.Path(tmp.name) / "data"
    data_dir.mkdir(parents=True)
    shutil.copy2(ROOT / "data/cards_v1.json", data_dir / "cards_v1.json")
    return tmp


def _patched_tool(tmp_root: pathlib.Path):
    return mock.patch.multiple(
        tags_tool,
        ROOT=tmp_root,
        REGISTRY_PATH=tmp_root / "data/cards_v1.json",
        OUTPUT_PATH=tmp_root / "data/pauper_removal_counterspell_tags_v1.json",
    )


class GenerateRemovalCounterspellTags(unittest.TestCase):
    def test_write_produces_a_schema_valid_file_with_known_cards(self):
        with _isolated_repo_root() as tmp:
            tmp_root = pathlib.Path(tmp)
            with _patched_tool(tmp_root):
                self.assertEqual(tags_tool.main(["--write"]), 0)
                document = json.loads(
                    (tmp_root / "data/pauper_removal_counterspell_tags_v1.json").read_text(
                        encoding="utf-8"
                    )
                )
        self.assertEqual(document["schema"], "kernel_removal_counterspell_tags/v1")
        by_name = {row["name"]: row for row in document["cards"]}
        self.assertIn("Annul", by_name)
        self.assertTrue(by_name["Annul"]["is_counterspell"])
        self.assertFalse(by_name["Annul"]["requires_target"] and not by_name["Annul"]["is_counterspell"])
        cards = json.load(open(ROOT / "data/cards_v1.json", encoding="utf-8"))["cards"]
        # Scoped to Instant/Sorcery: mtg_kernel::card_def::CardDef.target_spec
        # is the *spell's own cast-time target* (verified against every
        # card_id in mtg-kernel/tests/removal_counterspell_tags_v1.rs). A
        # Creature/Artifact/Enchantment with a "destroy_target" mechanics tag
        # (e.g. Gorilla Shaman) expresses that target on a later activated
        # ability or ETB trigger, not on the permanent spell's own casting,
        # so its live target_spec is None and it is correctly absent from
        # this file; see IS_A_SPELL_TYPES in the generator for the full
        # reasoning.
        removal_names = {
            c["name"]
            for c in cards
            if "destroy_target" in c.get("mechanics", [])
            and {"Instant", "Sorcery"} & set(c.get("types", []))
        }
        self.assertTrue(removal_names, "the live registry must have at least one destroy_target instant/sorcery to test against")
        for name in removal_names:
            self.assertIn(name, by_name, f"{name} has destroy_target but is missing from the tag file")
            self.assertTrue(by_name[name]["requires_target"])

    def test_check_mode_is_idempotent_against_a_freshly_written_file(self):
        with _isolated_repo_root() as tmp:
            tmp_root = pathlib.Path(tmp)
            with _patched_tool(tmp_root):
                self.assertEqual(tags_tool.main(["--write"]), 0)
                self.assertEqual(tags_tool.main(["--check"]), 0)

    def test_check_mode_passes_against_the_committed_file_without_writing(self):
        # No `--write` anywhere in this test: it runs `--check` against
        # whatever is already tracked in git, so it is the one test in this
        # suite that can actually catch `data/pauper_removal_counterspell_
        # tags_v1.json` drifting out of sync with `data/cards_v1.json`.
        before = COMMITTED_OUTPUT.read_bytes()
        result = subprocess.run(
            [sys.executable, str(TOOL_SCRIPT), "--check"], cwd=ROOT, capture_output=True, text=True,
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(
            COMMITTED_OUTPUT.read_bytes(),
            before,
            "--check must never modify the committed tag file",
        )


if __name__ == "__main__":
    unittest.main()
