import json
import subprocess
import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
TOOL = ROOT / "python/tools/generate_removal_counterspell_tags_v1.py"
OUTPUT = ROOT / "data/pauper_removal_counterspell_tags_v1.json"


class GenerateRemovalCounterspellTags(unittest.TestCase):
    def test_write_produces_a_schema_valid_file_with_known_cards(self):
        result = subprocess.run(
            [sys.executable, str(TOOL), "--write"], cwd=ROOT, capture_output=True, text=True,
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        document = json.loads(OUTPUT.read_text(encoding="utf-8"))
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
        subprocess.run([sys.executable, str(TOOL), "--write"], cwd=ROOT, check=True)
        result = subprocess.run(
            [sys.executable, str(TOOL), "--check"], cwd=ROOT, capture_output=True, text=True,
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)


if __name__ == "__main__":
    unittest.main()
