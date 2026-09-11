from __future__ import annotations

import pathlib
import subprocess
import sys
import tempfile
import unittest
from xml.etree import ElementTree as ET

ROOT = pathlib.Path(__file__).resolve().parents[2]
LIST = ROOT / "docs/research/pauper_meta_decklists_2026-09-09/Affinity__887998.txt"
URZATRON_LIST = ROOT / "docs/research/pauper_meta_decklists_2026-09-09/Urzatron__888074.txt"
TOOL = "python/tools/write_dek_from_mtgo_list_v1.py"


class WriteDek(unittest.TestCase):
    def test_substitutions_and_counts(self) -> None:
        out = pathlib.Path(tempfile.mkdtemp()) / "t.dek"
        # Glint Hawk is not yet registered in data/cards_v1.json (a later
        # task in this wave adds it); --allow-pending is the documented
        # escape hatch (task-4-brief.md step 4) for exactly this case.
        subprocess.run(
            [
                sys.executable,
                TOOL,
                str(LIST),
                str(out),
                "--substitute",
                "Utrom Monitor=Glint Hawk",
                "--substitute",
                "Sewer-veillance Cam=Cryogen Relic",
                "--allow-pending",
                "Glint Hawk",
            ],
            cwd=ROOT,
            check=True,
        )
        rows = ET.parse(out).getroot().findall("Cards")
        main = sum(int(r.get("Quantity")) for r in rows if r.get("Sideboard") == "false")
        side = sum(int(r.get("Quantity")) for r in rows if r.get("Sideboard") == "true")
        names = {r.get("Name") for r in rows}
        self.assertEqual((main, side), (60, 15))
        self.assertIn("Glint Hawk", names)
        self.assertNotIn("Utrom Monitor", names)
        self.assertIn("Cryogen Relic", names)
        self.assertNotIn("Sewer-veillance Cam", names)

    def test_urzatron_substitutions_and_counts(self) -> None:
        # Malevolent Rumble is this wave's own card (not yet registered when
        # this test runs; a later task in this wave registers it);
        # --allow-pending is the documented escape hatch for exactly this
        # case. Blue Elemental Blast is already registered (wave 1), so its
        # substitution needs no --allow-pending: the tool's registry check
        # applies to a --substitute target's new name, not to names copied
        # through from the source list.
        out = pathlib.Path(tempfile.mkdtemp()) / "t.dek"
        subprocess.run(
            [
                sys.executable,
                TOOL,
                str(URZATRON_LIST),
                str(out),
                "--substitute",
                "Giant's Boulder=Malevolent Rumble",
                "--substitute",
                "Call Damage Control=Blue Elemental Blast",
                "--allow-pending",
                "Malevolent Rumble",
            ],
            cwd=ROOT,
            check=True,
        )
        rows = ET.parse(out).getroot().findall("Cards")
        main = sum(int(r.get("Quantity")) for r in rows if r.get("Sideboard") == "false")
        side = sum(int(r.get("Quantity")) for r in rows if r.get("Sideboard") == "true")
        self.assertEqual((main, side), (60, 15))
        main_names = {r.get("Name") for r in rows if r.get("Sideboard") == "false"}
        side_names = {r.get("Name") for r in rows if r.get("Sideboard") == "true"}
        self.assertNotIn("Giant's Boulder", main_names)
        self.assertNotIn("Call Damage Control", side_names)
        rumble_qty = sum(
            int(r.get("Quantity"))
            for r in rows
            if r.get("Name") == "Malevolent Rumble" and r.get("Sideboard") == "false"
        )
        self.assertEqual(rumble_qty, 4)
        beb_qty = sum(
            int(r.get("Quantity"))
            for r in rows
            if r.get("Name") == "Blue Elemental Blast" and r.get("Sideboard") == "true"
        )
        self.assertEqual(beb_qty, 2)

    def test_substitution_target_not_registered_and_not_pending_is_an_error(self) -> None:
        # Re-pinned for the pauper-meta-cards-v1 card lane's wave 1 (Task 13,
        # identity finalisation): this test's original substitution target,
        # Glint Hawk, was itself registered by Task 8 of this same wave, so
        # it stopped being an example of "not registered and not pending"
        # (the tool now accepts it with no --allow-pending needed, which is
        # correct new behavior, not a bug -- see
        # test_substitutions_and_counts above, which still passes
        # --allow-pending harmlessly for the same now-registered card).
        # Mulldrifter is confirmed absent from data/cards_v1.json (see
        # docs/research/pauper_meta_gap_after_w1_2026-09.json) and is not
        # this wave's own card, so it is a stable "still unregistered"
        # example for this negative test.
        out = pathlib.Path(tempfile.mkdtemp()) / "t.dek"
        result = subprocess.run(
            [
                sys.executable,
                TOOL,
                str(LIST),
                str(out),
                "--substitute",
                "Utrom Monitor=Mulldrifter",
            ],
            cwd=ROOT,
            capture_output=True,
            text=True,
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse(out.exists())

    def test_substitution_source_absent_from_list_is_an_error(self) -> None:
        out = pathlib.Path(tempfile.mkdtemp()) / "t.dek"
        result = subprocess.run(
            [
                sys.executable,
                TOOL,
                str(LIST),
                str(out),
                "--substitute",
                "Card Not In This List=Glint Hawk",
                "--allow-pending",
                "Glint Hawk",
            ],
            cwd=ROOT,
            capture_output=True,
            text=True,
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse(out.exists())

    def test_header_and_line_endings_match_existing_files(self) -> None:
        out = pathlib.Path(tempfile.mkdtemp()) / "t.dek"
        subprocess.run(
            [sys.executable, TOOL, str(LIST), str(out),
             "--substitute", "Utrom Monitor=Glint Hawk",
             "--substitute", "Sewer-veillance Cam=Cryogen Relic",
             "--allow-pending", "Glint Hawk"],
            cwd=ROOT,
            check=True,
        )
        existing = (ROOT / "oracle/xmage/decks/Pauper/Deck - Elves.dek").read_bytes()
        existing_header = existing.split(b"\r\n")[0:4]
        actual = out.read_bytes()
        actual_header = actual.split(b"\r\n")[0:4]
        self.assertEqual(actual_header, existing_header)
        self.assertNotIn(b"\r\r\n", actual)
        # Every line terminator in the file is CRLF (no bare LF).
        body = actual.replace(b"\r\n", b"")
        self.assertNotIn(b"\n", body)


if __name__ == "__main__":
    unittest.main()
