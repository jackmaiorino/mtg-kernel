from __future__ import annotations

from contextlib import redirect_stderr, redirect_stdout
from dataclasses import replace
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
import limited_pool_v1 as construction


class LimitedPoolTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.registry = limited.fdn_registry()

    def build(self, pool: str, deck: str, additions: str = "") -> construction.ConstructedDeck:
        added = limited.parse_dck(additions) if additions else None
        return construction.construct_deck(
            limited.parse_dck(pool), limited.parse_dck(deck),
            (*added.mainboard, *added.sideboard) if added else (), registry=self.registry,
        )

    def test_owned_fifth_copy_is_legal_across_acquisition_rows_and_zones(self) -> None:
        built = self.build("2 Llanowar Elves\n3 Llanowar Elves\nSB: 35 Forest",
                           "5 Llanowar Elves\n35 Forest")
        self.assertEqual(built.mainboard_card_ids,
                         (self.registry["Llanowar Elves"].card_id,) * 5
                         + (self.registry["Forest"].card_id,) * 35)
        self.assertEqual(built.deck.sideboard, ())

    def test_same_nonbasic_copy_cannot_be_spent_in_both_zones(self) -> None:
        with self.assertRaisesRegex(ValueError, "not owned or explicitly added: 1 Llanowar Elves"):
            self.build("4 Llanowar Elves\n36 Forest",
                       "4 Llanowar Elves\n36 Forest\nSB: 1 Llanowar Elves")
        with self.assertRaisesRegex(ValueError, "not owned or explicitly added: 1 Llanowar Elves"):
            self.build("4 Llanowar Elves\n36 Forest",
                       "2 Llanowar Elves\n3 Llanowar Elves\n35 Forest\nSB: 1 Forest")

    def test_unowned_card_and_unassigned_inventory_are_refused(self) -> None:
        with self.assertRaisesRegex(ValueError, "not owned or explicitly added: 1 Healer's Hawk"):
            self.build("40 Forest", "39 Forest\n1 Healer's Hawk\nSB: 1 Forest")
        with self.assertRaisesRegex(ValueError, "unassigned copies: 1 Llanowar Elves"):
            self.build("40 Forest\n1 Llanowar Elves", "40 Forest")

    def test_basics_require_explicit_exact_additions_and_owned_basics_are_counted(self) -> None:
        built = self.build("5 Llanowar Elves\n1 Forest\n2 Healer's Hawk",
                           "5 Llanowar Elves\n35 Forest\nSB: 2 Healer's Hawk", "34 Forest")
        self.assertEqual(len(built.mainboard_card_ids), 40)
        self.assertEqual(built.basic_additions, (limited.DeckEntry(34, "Forest"),))
        for additions, error in (("", "not owned or explicitly added: 34 Forest"),
                                 ("33 Forest", "not owned or explicitly added: 1 Forest"),
                                 ("35 Forest", "unassigned copies: 1 Forest")):
            with self.subTest(additions=additions), self.assertRaisesRegex(ValueError, error):
                self.build("5 Llanowar Elves\n1 Forest\n2 Healer's Hawk",
                           "5 Llanowar Elves\n35 Forest\nSB: 2 Healer's Hawk", additions)

    def test_all_five_basics_are_permitted_but_nonbasics_are_not(self) -> None:
        for name in limited.BASIC_LANDS:
            with self.subTest(name=name):
                built = self.build("1 Llanowar Elves", f"1 Llanowar Elves\n39 {name}", f"39 {name}")
                self.assertEqual(built.mainboard_card_ids[1:], (self.registry[name].card_id,) * 39)
        with self.assertRaisesRegex(ValueError, "basic additions permit only"):
            self.build("39 Forest", "39 Forest\n1 Llanowar Elves", "1 Llanowar Elves")

    def test_partition_keeps_name_annotations_rows_copies_and_reserve_order(self) -> None:
        pool = limited.parse_dck("5 [FDN:227] Llanowar Elves\n2 [FDN:142] Healer's Hawk\n3 Forest")
        partition = limited.parse_dck(
            "NAME:Ordered construction\n2 [FDN:227] Llanowar Elves\n10 [FDN:280] Forest\n"
            "3 Llanowar Elves\n25 Forest\nSB: 1 [FDN:142] Healer's Hawk\nSB: 1 Healer's Hawk"
        )
        built = construction.construct_deck(pool, partition, (limited.DeckEntry(32, "Forest"),),
                                             registry=self.registry)
        self.assertEqual(built.deck, partition)
        self.assertEqual(built.mainboard_card_ids, tuple(limited.resolve_mainboard(partition, self.registry)))
        self.assertEqual(built.deck.sideboard, (limited.DeckEntry(1, "Healer's Hawk", "FDN:142"),
                                               limited.DeckEntry(1, "Healer's Hawk")))

    def test_partial_inventory_can_remain_in_reserve_but_never_in_executable_mainboard(self) -> None:
        pool = "40 Forest\n1 Muldrotha, the Gravetide"
        built = self.build(pool, "40 Forest\nSB: 1 Muldrotha, the Gravetide")
        self.assertEqual(len(built.mainboard_card_ids), 40)
        self.assertNotIn(self.registry["Muldrotha, the Gravetide"].card_id, built.mainboard_card_ids)
        with self.assertRaisesRegex(ValueError, r"unsupported mainboard: Muldrotha, the Gravetide \(partial\)"):
            self.build(pool, "39 Forest\n1 Muldrotha, the Gravetide\nSB: 1 Forest")

    def test_no_effect_catalog_entries_are_also_refused_in_mainboard(self) -> None:
        registry = dict(self.registry)
        registry["Llanowar Elves"] = replace(registry["Llanowar Elves"], capability="no_effect")
        deck = limited.parse_dck("5 Llanowar Elves\n35 Forest")
        with self.assertRaisesRegex(ValueError, r"Llanowar Elves \(no_effect\)"):
            construction.construct_deck(deck, deck, registry=registry)

    def test_unknown_tokens_and_registered_out_of_pool_cards_are_refused(self) -> None:
        for name, error in (("Never Implemented", r"\(missing\)"),
                            ("Zombie Token", r"\(token\)"),
                            ("Annul", "outside the frozen FDN pool")):
            for zone in ("pool", "mainboard", "reserve", "additions"):
                with self.subTest(name=name, zone=zone), self.assertRaisesRegex(ValueError, error):
                    acquired = limited.parse_dck("40 Forest")
                    deck = acquired
                    bad = (limited.DeckEntry(1, name),)
                    additions = ()
                    if zone == "pool":
                        acquired = replace(acquired, sideboard=bad)
                    elif zone == "mainboard":
                        deck = replace(deck, mainboard=bad)
                    elif zone == "reserve":
                        deck = replace(deck, sideboard=bad)
                    else:
                        additions = bad
                    construction.construct_deck(acquired, deck, additions, registry=self.registry)

    def test_direct_api_rejects_invalid_counts_in_every_input_zone(self) -> None:
        for count in (0, -1, True, 1.5, "1"):
            for zone in ("pool", "mainboard", "reserve", "additions"):
                with self.subTest(count=count, zone=zone), self.assertRaisesRegex(ValueError, "positive integers"):
                    acquired = limited.parse_dck("40 Forest")
                    deck = acquired
                    bad = (limited.DeckEntry(count, "Forest"),)
                    additions = ()
                    if zone == "pool":
                        acquired = replace(acquired, mainboard=bad)
                    elif zone == "mainboard":
                        deck = replace(deck, mainboard=bad)
                    elif zone == "reserve":
                        deck = replace(deck, sideboard=bad)
                    else:
                        additions = bad
                    construction.construct_deck(acquired, deck, additions, registry=self.registry)

    def test_minimum_size_and_native_allocation_bound(self) -> None:
        with self.assertRaisesRegex(ValueError, "at least 40"):
            self.build("39 Forest", "39 Forest")
        with self.assertRaisesRegex(ValueError, "native 10000-card limit"):
            self.build("10001 Forest", "10001 Forest")
        self.assertEqual(len(self.build("41 Forest", "41 Forest").mainboard_card_ids), 41)

    def test_failed_build_does_not_consume_pool_or_change_previous_success(self) -> None:
        acquired = limited.parse_dck("5 Llanowar Elves\n35 Forest\n1 Healer's Hawk\n1 Muldrotha, the Gravetide")
        deck = limited.parse_dck("5 Llanowar Elves\n35 Forest\nSB: 1 Healer's Hawk\nSB: 1 Muldrotha, the Gravetide")
        catalog_before = dict(self.registry)
        built = construction.construct_deck(acquired, deck, registry=self.registry)
        prior_ids = built.mainboard_card_ids
        for bad in (replace(deck, sideboard=(limited.DeckEntry(2, "Healer's Hawk"),)),
                    limited.parse_dck("5 Llanowar Elves\n34 Forest\n1 Muldrotha, the Gravetide\n"
                                      "SB: 1 Healer's Hawk\nSB: 1 Forest")):
            with self.assertRaises(ValueError):
                construction.construct_deck(acquired, bad, registry=self.registry)
            self.assertEqual(self.registry, catalog_before)
            self.assertEqual(built.deck, deck)
            self.assertEqual(built.mainboard_card_ids, prior_ids)
            self.assertEqual(construction.construct_deck(acquired, deck, registry=self.registry), built)
        self.assertEqual(acquired, limited.parse_dck("5 Llanowar Elves\n35 Forest\n1 Healer's Hawk\n"
                                                   "1 Muldrotha, the Gravetide"))

    def test_success_copies_mutable_row_containers(self) -> None:
        rows = [limited.DeckEntry(40, "Forest")]
        deck = limited.ImportedDeck(None, rows, [])
        built = construction.construct_deck(deck, deck, registry=self.registry)
        rows.clear()
        self.assertEqual(built.deck.mainboard, (limited.DeckEntry(40, "Forest"),))
        self.assertEqual(len(built.mainboard_card_ids), 40)

    def test_cli_outputs_complete_partition_and_no_ids_on_failure(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            acquired = directory / "pool.dck"
            deck = directory / "deck.dck"
            added = directory / "basics.dck"
            acquired.write_text("5 Llanowar Elves\n1 Healer's Hawk", encoding="utf-8")
            deck.write_text("NAME:CLI\n5 Llanowar Elves\n35 Forest\nSB: 1 Healer's Hawk", encoding="utf-8")
            added.write_text("35 [FDN:280] Forest", encoding="utf-8")
            arguments = ["--pool", str(acquired), "--deck", str(deck), "--basic-additions", str(added)]
            output, errors = io.StringIO(), io.StringIO()
            with redirect_stdout(output), redirect_stderr(errors):
                self.assertEqual(construction.main(arguments), 0)
            result = json.loads(output.getvalue())
            self.assertEqual(result["schema"], "kernel_limited_pool_construction/v1")
            self.assertEqual(result["name"], "CLI")
            self.assertEqual(result["reserve"], [{"count": 1, "name": "Healer's Hawk", "printing": None}])
            self.assertEqual(result["basic_additions"][0]["printing"], "FDN:280")
            self.assertEqual(result["pool_sha256"], hashlib.sha256(acquired.read_bytes()).hexdigest())
            self.assertEqual(result["deck_sha256"], hashlib.sha256(deck.read_bytes()).hexdigest())
            self.assertEqual(result["basic_additions_sha256"], hashlib.sha256(added.read_bytes()).hexdigest())
            self.assertEqual(result["registry_sha256"],
                             hashlib.sha256((REPO_ROOT / "data/cards_v1.json").read_bytes()).hexdigest())
            self.assertEqual(result["registry_extensions_sha256"],
                             [hashlib.sha256((REPO_ROOT / "data/limited/fdn_v1/cards_v1.json").read_bytes()).hexdigest()])
            self.assertEqual(len(result["mainboard_card_ids"]), 40)
            self.assertEqual(errors.getvalue(), "")
            deck.write_text("6 Llanowar Elves\n34 Forest\nSB: 1 Healer's Hawk\nSB: 1 Forest", encoding="utf-8")
            output, errors = io.StringIO(), io.StringIO()
            with redirect_stdout(output), redirect_stderr(errors):
                self.assertEqual(construction.main(arguments), 2)
            self.assertEqual(output.getvalue(), "")
            self.assertIn("not owned or explicitly added: 1 Llanowar Elves", json.loads(errors.getvalue())["error"])
            self.assertEqual(acquired.read_text(encoding="utf-8"), "5 Llanowar Elves\n1 Healer's Hawk")


if __name__ == "__main__":
    unittest.main()
