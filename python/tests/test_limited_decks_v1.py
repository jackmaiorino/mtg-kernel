from __future__ import annotations

from contextlib import redirect_stderr, redirect_stdout
import hashlib
import io
import json
from pathlib import Path
import sys
import tempfile
import unittest


REPO_ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO_ROOT / "python/tools"))
import limited_decks_v1 as limited


FIXTURES = REPO_ROOT / "data/limited/fdn_v1"


class LimitedDeckTest(unittest.TestCase):
    def setUp(self) -> None:
        self.registry = limited.fdn_registry()

    def test_registry_extension_keeps_all_original_ids_and_resolves_the_new_batch(self) -> None:
        cards = limited.load_json((REPO_ROOT / "data/cards_v1.json").read_bytes())["cards"]
        original_names = json.dumps([card["name"] for card in cards[:162]],
                                    ensure_ascii=False, separators=(",", ":")).encode()
        self.assertEqual(hashlib.sha256(original_names).hexdigest(),
                         "396e3458a989e9b40ca59dabb389bda9dd14b9fa367a871ee84129f292e0c156")
        names = ["Plains", "Healer's Hawk", "Fleeting Distraction", "Cathar Commando",
                 "Spectral Sailor", "Treetop Snarespinner"]
        deck = limited.parse_dck("\n".join(f"10 {name}" for name in names))
        self.assertEqual(limited.resolve_mainboard(deck, self.registry),
                         [self.registry[name].card_id for name in names for _ in range(10)])

    def test_dwynen_resolves_with_the_resumable_legend_rule(self) -> None:
        deck = limited.parse_dck("38 Forest\n2 Dwynen, Gilt-Leaf Daen\n")
        self.assertEqual(limited.card_status("Dwynen, Gilt-Leaf Daen", self.registry), "full")
        self.assertEqual(limited.resolve_mainboard(deck, self.registry),
                         [self.registry["Forest"].card_id] * 38
                         + [self.registry["Dwynen, Gilt-Leaf Daen"].card_id] * 2)

    def test_reference_planeswalker_is_partial_and_refused(self) -> None:
        deck = limited.parse_dck((FIXTURES / "FDN_reference_planeswalker.dck").read_text())
        self.assertEqual(limited.card_status("Ajani, Caller of the Pride", self.registry), "partial")
        with self.assertRaisesRegex(ValueError, "partial"):
            limited.resolve_mainboard(deck, self.registry)

    def test_extension_rejects_duplicates_and_preserves_base_card_ids(self) -> None:
        base = (REPO_ROOT / "data/cards_v1.json").read_bytes()
        extension = (FIXTURES / "cards_v1.json").read_bytes()
        base_registry = limited.registry_from_json(limited.load_json(base))
        self.assertEqual(len(base_registry), 192)
        self.assertNotIn("Plains", base_registry)
        for name, card in base_registry.items():
            self.assertEqual(self.registry[name], card)
        self.assertEqual(self.registry["Plains"].card_id, 192)
        with self.assertRaisesRegex(ValueError, "duplicate name"):
            limited.combined_registry(base, [extension, extension])
        with self.assertRaisesRegex(ValueError, "registry version 2"):
            limited.combined_registry(base, [b'{"version":3,"cards":[]}'])

    def test_real_decks_are_40_cards_and_report_missing_behavior(self) -> None:
        for filename, supported_copies, source_sha256 in [
            ("FDN_top_04956_UG.dck", 34, "bb618d6eaddf04b0a9e51e9a88cd512a04635e99ebca91b11ace4c305d634c86"),
            ("FDN_top_20626_WG.dck", 34, "be026f1c86e3aabcb294517188d0c5f4f0cdfa3c5e95ee9dedfc51d3cdf814f7"),
        ]:
            with self.subTest(filename=filename):
                self.assertEqual(hashlib.sha256((FIXTURES / filename).read_bytes()).hexdigest(), source_sha256)
                deck = limited.parse_dck((FIXTURES / filename).read_text(encoding="utf-8"))
                report = limited.inspect_deck(deck, self.registry)
                self.assertEqual(report["mainboard"]["copies"], 40)
                self.assertEqual(report["mainboard"]["supported_copies"], supported_copies)
                self.assertTrue(report["minimum_mainboard_size_met"])
                self.assertFalse(report["mainboard"]["registry_ready"])
                with self.assertRaisesRegex(ValueError, "unsupported mainboard"):
                    limited.resolve_mainboard(deck, self.registry)

    def test_printings_repeat_counts_and_sideboard_are_preserved(self) -> None:
        deck = limited.parse_dck(
            "// comment\r\nNAME:Test\r\n5 [FDN:280] Forest\r\n"
            "35 Forest\r\nSB: 1 Never Implemented\r\n"
        )
        report = limited.inspect_deck(deck, self.registry)
        self.assertEqual(deck.mainboard[0].printing, "FDN:280")
        self.assertEqual(report["mainboard"]["unique_cards"], 1)
        self.assertTrue(report["mainboard"]["registry_ready"])
        self.assertFalse(report["sideboard"]["registry_ready"])
        self.assertEqual(limited.resolve_mainboard(deck, self.registry), [self.registry["Forest"].card_id] * 40)

    def test_arbitrary_decks_keep_row_and_copy_order_and_allow_over_40(self) -> None:
        deck = limited.parse_dck("20 Island\n21 Forest\n")
        ids = limited.resolve_mainboard(deck, self.registry)
        self.assertEqual(ids, [self.registry["Island"].card_id] * 20 + [self.registry["Forest"].card_id] * 21)
        with self.assertRaisesRegex(ValueError, "at least 40"):
            limited.resolve_mainboard(limited.parse_dck("39 Forest"), self.registry)

    def test_unknown_partial_no_effect_and_token_are_all_rejected(self) -> None:
        registry = limited.registry_from_json({"version": 2, "cards": [
            {"name": "Supported", "engine_capability": "full"},
            {"name": "Partial", "engine_capability": "partial"},
            {"name": "Absent Capability"},
            {"name": "Token", "engine_capability": "full", "is_token": True},
        ]})
        for name, status in [("Unknown", "missing"), ("Partial", "partial"),
                             ("Absent Capability", "no_effect"), ("Token", "token")]:
            with self.subTest(name=name):
                deck = limited.parse_dck(f"39 Supported\n1 {name}")
                self.assertEqual(limited.card_status(name, registry), status)
                with self.assertRaisesRegex(ValueError, status):
                    limited.resolve_mainboard(deck, registry)

    def test_bad_deck_rows_do_not_disappear_silently(self) -> None:
        for text in ("", "0 Forest", "-1 Forest", "1.5 Forest", "Forest", "1 [FDN:280]",
                     "NAME:", "NAME:A\nNAME:B\n40 Forest", "SB: 40 Forest"):
            with self.subTest(text=text), self.assertRaises(ValueError):
                limited.parse_dck(text)

    def test_registry_errors_and_duplicate_json_keys_are_rejected(self) -> None:
        for value in ({"version": 1, "cards": []}, {"version": 2, "cards": [None]},
                      {"version": 2, "cards": [{"name": "A"}, {"name": "A"}]},
                      {"version": 2, "cards": [{"name": "A", "is_token": "false"}]},
                      {"version": 2, "cards": [{"name": "A", "engine_capability": "assumed"}]}):
            with self.subTest(value=value), self.assertRaises(ValueError):
                limited.registry_from_json(value)
        for raw in (b'{"version":2,"version":1}', b'{"version":NaN}', b'[]'):
            with self.subTest(raw=raw), self.assertRaises(ValueError):
                limited.load_json(raw)

    def test_inventory_includes_basics_and_fixture_cards_beyond_reference(self) -> None:
        report = limited.inventory(["Island"], self.registry, [limited.parse_dck("40 Unknown")])
        by_name = {card["name"]: card for card in report["cards"]}
        self.assertEqual(report["reference_card_count"], 1)
        self.assertEqual(report["required_card_count"], 6)
        self.assertEqual(by_name["Plains"]["status"], "full")
        self.assertFalse(by_name["Unknown"]["in_reference"])
        self.assertEqual(by_name["Unknown"]["fixture_copies"], 40)
        for names in ([], ["A", "A"], [42]):
            with self.subTest(names=names), self.assertRaises(ValueError):
                limited.inventory(names, self.registry, [])

    def test_real_reference_already_contains_all_basic_lands(self) -> None:
        document = limited.load_json((FIXTURES / "card_names.json").read_bytes())
        names = document["card_names"]
        self.assertEqual(len(names), 286)
        self.assertTrue(set(limited.BASIC_LANDS).issubset(names))
        decks = [limited.parse_dck(path.read_text(encoding="utf-8")) for path in sorted(FIXTURES.glob("FDN_top_*.dck"))]
        report = limited.inventory(names, self.registry, decks)
        self.assertEqual(report["reference_card_count"], report["required_card_count"])
        self.assertEqual(sum(card["status"] == "full" for card in report["cards"]), 34)
        self.assertEqual(sum(card["fixture_copies"] > 0 and card["status"] != "full"
                             for card in report["cards"]), 9)

    def test_cli_refusal_has_no_materialized_ids_and_inspection_is_deterministic(self) -> None:
        args = ["--deck", str(FIXTURES / "FDN_top_04956_UG.dck")]
        stdout, stderr = io.StringIO(), io.StringIO()
        with redirect_stdout(stdout), redirect_stderr(stderr):
            self.assertEqual(limited.main(["resolve", *args]), 2)
        self.assertEqual(stdout.getvalue(), "")
        self.assertIn("unsupported mainboard", json.loads(stderr.getvalue())["error"])
        outputs = []
        for _ in range(2):
            output = io.StringIO()
            with redirect_stdout(output):
                self.assertEqual(limited.main(["inspect", *args]), 0)
            outputs.append(output.getvalue())
        self.assertEqual(*outputs)
        report = json.loads(outputs[0])
        self.assertEqual(report["deck_sha256"], hashlib.sha256((FIXTURES / "FDN_top_04956_UG.dck").read_bytes()).hexdigest())

    def test_cli_resolves_supported_deck_and_reports_its_registry_identity(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            deck_path = Path(tmp) / "deck.dck"
            deck_path.write_text("20 Forest\n20 Island\nSB: 1 Missing", encoding="utf-8")
            output = io.StringIO()
            with redirect_stdout(output):
                self.assertEqual(limited.main(["resolve", "--deck", str(deck_path)]), 0)
            report = json.loads(output.getvalue())
            self.assertEqual(len(report["card_ids"]), 40)
            self.assertEqual(report["card_id_order"], "dck-row-then-copy/v1")
            self.assertEqual(report["registry_sha256"], hashlib.sha256((REPO_ROOT / "data/cards_v1.json").read_bytes()).hexdigest())
            self.assertEqual(report["registry_extensions_sha256"],
                             [hashlib.sha256((FIXTURES / "cards_v1.json").read_bytes()).hexdigest()])


if __name__ == "__main__":
    unittest.main()
