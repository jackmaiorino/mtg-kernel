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

    def test_reference_keyword_creatures_resolve_all_forty_copies(self) -> None:
        deck = limited.parse_dck((FIXTURES / "FDN_reference_keyword_creatures.dck").read_text())
        ids = limited.resolve_mainboard(deck, self.registry)
        self.assertEqual(len(ids), 40)
        self.assertEqual(ids[33:], list(range(236, 243)))

    def test_reference_gainlands_lifegain_resolve_all_forty_copies(self) -> None:
        deck = limited.parse_dck((FIXTURES / "FDN_reference_gainlands_lifegain.dck").read_text())
        ids = limited.resolve_mainboard(deck, self.registry)
        self.assertEqual(len(ids), 40)
        self.assertEqual(ids[:8], list(range(243, 251)))
        self.assertEqual(ids[37:], [251, 252, 253])

    def test_reference_triggers_tricks_resolve_all_forty_copies(self) -> None:
        deck = limited.parse_dck((FIXTURES / "FDN_reference_triggers_tricks.dck").read_text())
        ids = limited.resolve_mainboard(deck, self.registry)
        self.assertEqual(len(ids), 40)
        self.assertEqual(ids[:12], list(range(254, 266)))

    def test_reference_equipment_search_resolves_all_forty_copies(self) -> None:
        deck = limited.parse_dck((FIXTURES / "FDN_reference_equipment_search.dck").read_text())
        ids = limited.resolve_mainboard(deck, self.registry)
        self.assertEqual(len(ids), 40)
        self.assertEqual(sorted(ids[32:]), list(range(268, 276)))

    def test_reference_tokens_auras_resolve_all_forty_copies(self) -> None:
        deck = limited.parse_dck((FIXTURES / "FDN_reference_tokens_auras.dck").read_text())
        ids = limited.resolve_mainboard(deck, self.registry)
        self.assertEqual(len(ids), 40)
        self.assertEqual(ids[:9], list(range(276, 285)))

    def test_reference_removal_tricks_resolve_all_forty_copies(self) -> None:
        deck = limited.parse_dck((FIXTURES / "FDN_reference_removal_tricks.dck").read_text())
        ids = limited.resolve_mainboard(deck, self.registry)
        self.assertEqual(len(ids), 40)
        self.assertEqual(sorted(set(ids) - set(range(289))), list(range(289, 307)))

    def test_reference_counters_threats_resolve_all_forty_copies(self) -> None:
        deck = limited.parse_dck((FIXTURES / "FDN_reference_counters_threats.dck").read_text())
        ids = limited.resolve_mainboard(deck, self.registry)
        self.assertEqual(len(ids), 40)
        self.assertEqual(ids[:10], list(range(307, 317)))

    def test_reference_library_search_resolves_all_forty_copies(self) -> None:
        deck = limited.parse_dck((FIXTURES / "FDN_reference_library_search.dck").read_text())
        ids = limited.resolve_mainboard(deck, self.registry)
        self.assertEqual(len(ids), 40)
        self.assertEqual(sorted(ids[36:]), list(range(317, 321)))

    def test_reference_simple_triggers_resolve_all_forty_copies(self) -> None:
        deck = limited.parse_dck((FIXTURES / "FDN_reference_simple_triggers.dck").read_text())
        ids = limited.resolve_mainboard(deck, self.registry)
        self.assertEqual(len(ids), 40)
        self.assertEqual(ids[:5], list(range(322, 327)))

    def test_reference_activated_combat_resolves_all_forty_copies(self) -> None:
        deck = limited.parse_dck((FIXTURES / "FDN_reference_activated_combat.dck").read_text())
        ids = limited.resolve_mainboard(deck, self.registry)
        self.assertEqual(len(ids), 40)
        self.assertEqual(ids[:3], list(range(327, 330)))

    def test_reference_surveil_resolves_all_forty_copies(self) -> None:
        deck = limited.parse_dck((FIXTURES / "FDN_reference_surveil.dck").read_text())
        ids = limited.resolve_mainboard(deck, self.registry)
        self.assertEqual(len(ids), 40)
        self.assertEqual(ids[:2], [330, 331])

    def test_reference_first_lifegain_resolves_all_forty_copies(self) -> None:
        deck = limited.parse_dck((FIXTURES / "FDN_reference_first_lifegain.dck").read_text())
        ids = limited.resolve_mainboard(deck, self.registry)
        self.assertEqual(len(ids), 40)
        self.assertEqual(ids[:2], [332, 333])

    def test_reference_graveyard_triggers_resolves_all_forty_copies(self) -> None:
        deck = limited.parse_dck((FIXTURES / "FDN_reference_graveyard_triggers.dck").read_text())
        ids = limited.resolve_mainboard(deck, self.registry)
        self.assertEqual(len(ids), 40)
        self.assertEqual(ids[:8], [334] * 4 + [335] * 4)

    def test_reference_static_team_resolves_all_forty_copies(self) -> None:
        deck = limited.parse_dck((FIXTURES / "FDN_reference_static_team.dck").read_text())
        ids = limited.resolve_mainboard(deck, self.registry)
        self.assertEqual(len(ids), 40)
        self.assertEqual(ids[:8], [336] * 4 + [337] * 4)

    def test_reference_prepared_families_resolves_all_forty_copies(self) -> None:
        deck = limited.parse_dck((FIXTURES / "FDN_reference_prepared_families.dck").read_text())
        ids = limited.resolve_mainboard(deck, self.registry)
        self.assertEqual(len(ids), 40)
        self.assertEqual(ids[:31], list(range(338, 369)))

    def test_reference_instant_sorcery_reducers_resolves_all_forty_copies(self) -> None:
        deck = limited.parse_dck((FIXTURES / "FDN_reference_instant_sorcery_reducers.dck").read_text())
        ids = limited.resolve_mainboard(deck, self.registry)
        self.assertEqual(len(ids), 40)
        self.assertEqual(ids[:8], [369] * 4 + [370] * 4)

    def test_reference_conditional_creatures_resolves_all_forty_copies(self) -> None:
        deck = limited.parse_dck((FIXTURES / "FDN_reference_conditional_flash_power_creatures.dck").read_text())
        ids = limited.resolve_mainboard(deck, self.registry)
        self.assertEqual(len(ids), 40)
        self.assertEqual(ids[:24], [371] * 4 + [372] * 4 + [373] * 4 + [374] * 4 + [375] * 4 + [376] * 4)

    def test_reference_following_creatures_spells_hare_resolves_all_forty_copies(self) -> None:
        deck = limited.parse_dck((FIXTURES / "FDN_reference_following_creatures_spells_and_hare.dck").read_text(encoding="utf-8"))
        ids = limited.resolve_mainboard(deck, self.registry)
        self.assertEqual(len(ids), 40)
        self.assertEqual(ids[:16], [i for i in range(377, 385) for _ in range(2)])

    def test_hare_copies_are_unrestricted_and_new_tokens_are_undeckable(self) -> None:
        deck = limited.parse_dck("5 Hare Apparent\n35 Plains\n")
        ids = limited.resolve_mainboard(deck, self.registry)
        self.assertEqual(len(ids), 40)
        self.assertEqual(ids[:5], [384] * 5)
        for name in ("Human Token", "Rabbit Token"):
            with self.subTest(name=name), self.assertRaisesRegex(ValueError, "token"):
                limited.resolve_mainboard(limited.parse_dck("39 Plains\n1 " + name), self.registry)

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

    def test_paladin_skyknight_sire_reference_and_appended_ids(self) -> None:
        deck = limited.parse_dck((FIXTURES / "FDN_reference_paladin_skyknight_sire.dck").read_text(encoding="utf-8"))
        resolved = limited.resolve_mainboard(deck, self.registry)
        self.assertEqual(len(resolved), 40)
        for name, card_id, copies in [
            ("Inspiring Paladin", 387, 4),
            ("Skyknight Squire", 388, 4),
            ("Sire of Seven Deaths", 389, 2),
        ]:
            self.assertEqual(self.registry[name].card_id, card_id)
            self.assertEqual(resolved.count(card_id), copies)

    def test_raid_entry_reference_and_appended_ids(self) -> None:
        deck = limited.parse_dck((FIXTURES / "FDN_reference_raid_entries.dck").read_text(encoding="utf-8"))
        resolved = limited.resolve_mainboard(deck, self.registry)
        self.assertEqual(len(resolved), 40)
        for name, card_id in [("Skyship Buccaneer", 390), ("Goblin Boarders", 391)]:
            self.assertEqual(self.registry[name].card_id, card_id)
            self.assertEqual(resolved.count(card_id), 4)
        self.assertEqual(self.registry["Gorehorn Raider"].card_id, 412)
        self.assertNotIn(412, resolved)
        self.assertNotIn("Strongbox Raider", self.registry)

    def test_original_decks_are_40_cards_and_fully_resolve(self) -> None:
        for filename, supported_copies, source_sha256 in [
            ("FDN_top_04956_UG.dck", 40, "bb618d6eaddf04b0a9e51e9a88cd512a04635e99ebca91b11ace4c305d634c86"),
            ("FDN_top_20626_WG.dck", 40, "be026f1c86e3aabcb294517188d0c5f4f0cdfa3c5e95ee9dedfc51d3cdf814f7"),
        ]:
            with self.subTest(filename=filename):
                self.assertEqual(hashlib.sha256((FIXTURES / filename).read_bytes()).hexdigest(), source_sha256)
                deck = limited.parse_dck((FIXTURES / filename).read_text(encoding="utf-8"))
                report = limited.inspect_deck(deck, self.registry)
                self.assertEqual(report["mainboard"]["copies"], 40)
                self.assertEqual(report["mainboard"]["supported_copies"], supported_copies)
                self.assertTrue(report["minimum_mainboard_size_met"])
                self.assertEqual(report["mainboard"]["registry_ready"], supported_copies == 40)
                if supported_copies == 40:
                    self.assertEqual(len(limited.resolve_mainboard(deck, self.registry)), 40)
                else:
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

    def test_condemn_return_reference_resolves_appended_spells(self) -> None:
        path = FIXTURES / "FDN_reference_condemn_return.dck"
        deck = limited.parse_dck(path.read_text(encoding="utf-8"))
        ids = limited.resolve_mainboard(deck, self.registry)
        self.assertEqual(len(ids), 40)
        self.assertEqual(ids.count(392), 4)
        self.assertEqual(ids.count(393), 4)
        self.assertEqual(self.registry["Condemn"].card_id, 392)
        self.assertEqual(self.registry["Run Away Together"].card_id, 393)

    def test_bloodwitch_reference_resolves_appended_creature(self) -> None:
        deck = limited.parse_dck((FIXTURES / "FDN_reference_bloodwitch.dck").read_text(encoding="utf-8"))
        ids = limited.resolve_mainboard(deck, self.registry)
        self.assertEqual(len(ids), 40)
        self.assertEqual(ids.count(394), 4)
        self.assertEqual(self.registry["Vengeful Bloodwitch"].card_id, 394)

    def test_divine_reference_keeps_prior_rows_and_resolves_appended_spell(self) -> None:
        deck = limited.parse_dck((FIXTURES / "FDN_reference_divine_resilience.dck").read_text(encoding="utf-8"))
        ids = limited.resolve_mainboard(deck, self.registry)
        self.assertEqual(len(ids), 40)
        self.assertEqual(ids[:4], [395] * 4)
        self.assertEqual(self.registry["Divine Resilience"].card_id, 395)
        self.assertEqual(self.registry["Vengeful Bloodwitch"].card_id, 394)

    def test_ooze_reference_appends_after_divine_without_renumbering(self) -> None:
        deck = limited.parse_dck((FIXTURES / "FDN_reference_scavenging_ooze.dck").read_text(encoding="utf-8"))
        ids = limited.resolve_mainboard(deck, self.registry)
        self.assertEqual(len(ids), 40)
        self.assertEqual(ids[:4], [396] * 4)
        self.assertEqual(self.registry["Scavenging Ooze"].card_id, 396)
        self.assertEqual(self.registry["Divine Resilience"].card_id, 395)

    def test_zombie_reference_appends_after_ooze_without_renumbering(self) -> None:
        deck = limited.parse_dck((FIXTURES / "FDN_reference_soul_shackled_zombie.dck").read_text(encoding="utf-8"))
        ids = limited.resolve_mainboard(deck, self.registry)
        self.assertEqual(len(ids), 40)
        self.assertEqual(ids[:4], [397] * 4)
        self.assertEqual(self.registry["Soul-Shackled Zombie"].card_id, 397)
        self.assertEqual(self.registry["Scavenging Ooze"].card_id, 396)

    def test_heartfire_reference_appends_after_zombie_without_renumbering(self) -> None:
        deck = limited.parse_dck((FIXTURES / "FDN_reference_heartfire_immolator.dck").read_text(encoding="utf-8"))
        ids = limited.resolve_mainboard(deck, self.registry)
        self.assertEqual(len(ids), 40)
        self.assertEqual(ids[:4], [398] * 4)
        self.assertEqual(self.registry["Heartfire Immolator"].card_id, 398)
        self.assertEqual(self.registry["Soul-Shackled Zombie"].card_id, 397)

    def test_next_source_batch_appends_and_internal_copy_is_not_a_booster(self) -> None:
        for slug, name, ident in [("searslicer_goblin", "Searslicer Goblin", 399),
                                  ("self_reflection", "Self-Reflection", 400),
                                  ("squad_rallier", "Squad Rallier", 402),
                                  ("sower_of_chaos", "Sower of Chaos", 403)]:
            deck = limited.parse_dck((FIXTURES / f"FDN_reference_{slug}.dck").read_text(encoding="utf-8"))
            ids = limited.resolve_mainboard(deck, self.registry)
            self.assertEqual(len(ids), 40)
            self.assertEqual(ids[:4], [ident] * 4)
            self.assertEqual(self.registry[name].card_id, ident)
        self.assertEqual(self.registry["Nameless Creature Copy Token"].card_id, 401)
        self.assertEqual(limited.card_status("Nameless Creature Copy Token", self.registry), "token")
        frozen_names = json.loads((FIXTURES / "booster_pool_v1.json").read_text(encoding="utf-8"))["cards"]
        self.assertNotIn("Nameless Creature Copy Token", [card["name"] for card in frozen_names])

    def test_later_source_batch_and_observer_sentinels_preserve_previous_ids(self) -> None:
        for slug, name, ident in [("frenzied_goblin", "Frenzied Goblin", 404),
                                  ("slumbering_cerberus", "Slumbering Cerberus", 405),
                                  ("wildwood_scourge", "Wildwood Scourge", 406)]:
            deck = limited.parse_dck((FIXTURES / f"FDN_reference_{slug}.dck").read_text(encoding="utf-8"))
            self.assertEqual(limited.resolve_mainboard(deck, self.registry)[:4], [ident] * 4)
            self.assertEqual(self.registry[name].card_id, ident)
        for name, ident in [("Face-down creature", 407), ("Face-down card", 408)]:
            self.assertEqual(self.registry[name].card_id, ident)
            with self.assertRaisesRegex(ValueError, "token"):
                limited.resolve_mainboard(limited.parse_dck("39 Plains\n1 " + name), self.registry)
        self.assertEqual(self.registry["Sower of Chaos"].card_id, 403)

    def test_authority_and_employment_candidate_preserves_prior_registry_ids(self) -> None:
        for slug, name, ident in [("authority_of_the_consuls", "Authority of the Consuls", 409),
                                  ("involuntary_employment", "Involuntary Employment", 410)]:
            deck = limited.parse_dck((FIXTURES / f"FDN_reference_{slug}.dck").read_text(encoding="utf-8"))
            self.assertEqual(limited.resolve_mainboard(deck, self.registry)[:4], [ident] * 4)
            self.assertEqual(self.registry[name].card_id, ident)
        self.assertEqual(self.registry["Wildwood Scourge"].card_id, 406)
        self.assertEqual(self.registry["Face-down card"].card_id, 408)

    def test_future_librarian_and_raid_candidates_append_after_employment(self) -> None:
        for slug, name, ident in [("mild_mannered_librarian", "Mild-Mannered Librarian", 411),
                                  ("gorehorn_raider", "Gorehorn Raider", 412),
                                  ("gutless_plunderer", "Gutless Plunderer", 413)]:
            deck = limited.parse_dck((FIXTURES / f"FDN_reference_{slug}.dck").read_text(encoding="utf-8"))
            self.assertEqual(limited.resolve_mainboard(deck, self.registry)[:4], [ident] * 4)
            self.assertEqual(len(limited.resolve_mainboard(deck, self.registry)), 40)
            self.assertEqual(self.registry[name].card_id, ident)
        self.assertEqual(self.registry["Involuntary Employment"].card_id, 410)

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
        self.assertEqual(sum(card["status"] == "full" for card in report["cards"]), 209)
        self.assertEqual(sum(card["fixture_copies"] > 0 and card["status"] != "full"
                             for card in report["cards"]), 0)

    def test_cli_refusal_has_no_materialized_ids_and_inspection_is_deterministic(self) -> None:
        args = ["--deck", str(FIXTURES / "FDN_top_04956_UG.dck")]
        stdout, stderr = io.StringIO(), io.StringIO()
        with tempfile.TemporaryDirectory() as tmp:
            unsupported = Path(tmp) / "unsupported.dck"
            unsupported.write_text("39 Forest\n1 Never Implemented\n", encoding="utf-8")
            with redirect_stdout(stdout), redirect_stderr(stderr):
                self.assertEqual(limited.main(["resolve", "--deck", str(unsupported)]), 2)
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

    def test_cli_resolves_both_unchanged_original_decks_in_copy_order(self) -> None:
        for filename in ["FDN_top_04956_UG.dck", "FDN_top_20626_WG.dck"]:
            with self.subTest(filename=filename):
                path = FIXTURES / filename
                output = io.StringIO()
                with redirect_stdout(output):
                    self.assertEqual(limited.main(["resolve", "--deck", str(path)]), 0)
                report = json.loads(output.getvalue())
                expected = limited.resolve_mainboard(limited.parse_dck(path.read_text(encoding="utf-8")), self.registry)
                self.assertEqual(report["card_ids"], expected)
                self.assertEqual(len(report["card_ids"]), 40)
                if "UG" in filename:
                    self.assertEqual(report["card_ids"].count(235), 2)

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
