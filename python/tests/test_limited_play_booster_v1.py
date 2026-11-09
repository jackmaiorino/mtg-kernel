from collections import Counter
from contextlib import redirect_stdout, redirect_stderr
from copy import deepcopy
from dataclasses import asdict
import hashlib
import io
import json
from pathlib import Path
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "tools"))
import fdn_booster_pool_v1 as target
import limited_decks_v1 as limited
import limited_play_booster_v1 as booster
import limited_draft_build_v1 as build
from limited_draft_v1 import DraftSession


def finish(session):
    while not session.complete:
        for seat in range(session.seat_count):
            view = session.view(seat)
            session.pick(seat, view.prompt, 0)
    return session


def plans(session):
    """A deterministic legal 40-card example, not a deck-strength heuristic."""
    catalog = limited.fdn_registry()
    partitions, additions = [], []
    for seat in range(session.seat_count):
        main, reserve = [], []
        for row in session.acquired_pool(seat).mainboard:
            (main if len(main) < 23 and catalog[row.name].capability == "full" else reserve).append(row)
        basics = (limited.DeckEntry(40 - len(main), "Forest"),)
        partitions.append(limited.ImportedDeck(f"Seat {seat}", (*main, *basics), tuple(reserve)))
        additions.append(basics)
    return tuple(partitions), tuple(additions)


class BoosterTests(unittest.TestCase):
    def test_fixed_seed_vector_and_frozen_name_coverage(self):
        packs = booster.generate_packs(42)
        raw = json.dumps([[asdict(p) for p in r] for r in packs], sort_keys=True, separators=(",", ":")).encode()
        self.assertEqual(hashlib.sha256(raw).hexdigest(),
                         "ae8d8a05047c06ce3be8daf6e4b7d53c1447006790a6aaf9a006e0fcddd4264b")
        self.assertEqual(packs, booster.generate_packs(42))
        self.assertNotEqual(packs, booster.generate_packs(43))
        names = {card["name"] for rows in booster._categories().values() for card in rows}
        self.assertEqual(names, set(limited.load_json(target.TARGET.read_bytes())["card_names"]))

    def test_slot_rules_include_guests_and_keep_lands_and_foils_separate(self):
        metadata = {c["name"]: c for c in limited.load_json(target.TARGET.read_bytes())["cards"]}
        saw_guest = saw_no_guest = saw_foil_land = saw_borderless = False
        for seed in range(30):
            for round_packs in booster.generate_packs(seed):
                self.assertEqual(len(round_packs), 8)
                for pack in round_packs:
                    rows = pack.mainboard
                    self.assertEqual(len(rows), 14)
                    self.assertFalse(pack.sideboard)
                    commons, uncommons = rows[:7], rows[7:10]
                    guests = [r for r in commons if metadata[r.name]["set"] == "SPG"]
                    saw_guest |= bool(guests)
                    saw_no_guest |= not guests
                    self.assertLessEqual(len(guests), 1)
                    self.assertEqual(len({r.name for r in commons}), 7)
                    self.assertTrue(all(metadata[r.name]["rarity"] == "common" for r in commons if r not in guests))
                    self.assertEqual(len({r.name for r in uncommons}), 3)
                    self.assertTrue(all(metadata[r.name]["rarity"] == "uncommon" for r in uncommons))
                    self.assertIn(metadata[rows[10].name]["rarity"], ("rare", "mythic"))
                    self.assertTrue(all(metadata[r.name]["set"] == "FDN" for r in rows[7:]))
                    self.assertTrue(rows[12].printing.endswith(":foil"))
                    self.assertTrue(all(not r.printing.endswith(":foil") for r in rows[:12]))
                    self.assertIn(rows[13].name, (*limited.BASIC_LANDS, *booster.DUALS))
                    saw_foil_land |= rows[13].printing.endswith(":foil")
                    saw_borderless |= any(292 <= int(r.printing.split(":")[1]) <= 361 for r in rows)
        self.assertTrue(saw_guest and saw_no_guest and saw_foil_land and saw_borderless)

    def test_seeds_and_pod_sizes_refuse_invalid_inputs(self):
        for seed in (-1, 2**64, 0.5, True, None):
            with self.subTest(seed=seed), self.assertRaises(ValueError):
                booster.generate_packs(seed)
        for seats in (0, 1, 9, 2.0, True):
            with self.subTest(seats=seats), self.assertRaises(ValueError):
                booster.generate_packs(1, seats)
        for seats in (2, 3, 8):
            self.assertEqual(len(booster.generate_packs(1, seats)[0]), seats)

    def test_full_eight_seat_draft_build_and_replay_conserve_every_copy(self):
        session = finish(booster.start_draft(42))
        self.assertEqual(len(session.record()["picks"]), 8 * 3 * 14)
        opened = Counter(r.name for packs in booster.generate_packs(42) for p in packs for r in p.mainboard)
        acquired = Counter(r.name for seat in range(8) for r in session.acquired_pool(seat).mainboard)
        self.assertEqual(acquired, opened)
        for seat in range(8):
            self.assertEqual(len(session.acquired_pool(seat).mainboard), 42)
        record = build.record_build(session, *plans(session))
        replayed = build.replay_build(json.loads(build.canonical_bytes(record)))
        self.assertEqual(build.canonical_bytes(replayed), build.canonical_bytes(record))
        self.assertTrue(all(len(seat["mainboard_card_ids"]) == 40 for seat in replayed["built"]))
        second = finish(booster.start_draft(42))
        self.assertEqual(build.record_build(second, *plans(second)), record)

    def test_seed_binding_is_stronger_than_supplied_pack_replay(self):
        session = finish(booster.start_draft(7, 2))
        record = session.record()
        forged = DraftSession(booster.generate_packs(7, 2), seed=8, provenance=record["provenance"])
        forged_record = finish(forged).record()
        DraftSession.replay(forged_record)  # valid supplied packs, but a false generator seed
        with self.assertRaisesRegex(ValueError, "seeded pack generator"):
            booster.replay_draft(forged_record)
        record["picks"][0]["copy_id"][2] += 1
        with self.assertRaises(ValueError):
            booster.replay_draft(record)

    def test_build_replay_rejects_changed_inventory_ids_seats_and_registry(self):
        session = finish(booster.start_draft(10, 2))
        record = build.record_build(session, *plans(session))
        mutations = [
            lambda r: r["built"][0]["mainboard_card_ids"].__setitem__(0, 65535),
            lambda r: r["built"][0]["mainboard_card_ids"].__setitem__(0, True),
            lambda r: r["built"][0]["deck"]["mainboard"][0].__setitem__("count", 20),
            lambda r: r["built"][0].__setitem__("seat", 1),
            lambda r: r["built"].pop(),
            lambda r: r["registry"].__setitem__("data/cards_v1.json", "0" * 64),
            lambda r: r["draft"].__setitem__("provenance", "other"),
        ]
        for mutate in mutations:
            changed = deepcopy(record)
            mutate(changed)
            with self.subTest(mutate=mutate), self.assertRaises(ValueError):
                build.replay_build(changed)
        incomplete = booster.start_draft(10, 2)
        with self.assertRaisesRegex(ValueError, "completed draft"):
            build.record_build(incomplete, *plans(session))

    def test_cli_generates_builds_and_replays_without_overwriting(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            draft_path, plans_path, built_path = (root / name for name in ("draft.json", "plans.json", "built.json"))
            self.assertEqual(build.main(["generate", "--seed", "71", "--seats", "2", "--out", str(draft_path)]), 0)
            initial = draft_path.read_bytes()
            with redirect_stderr(io.StringIO()):
                self.assertEqual(build.main(["generate", "--seed", "72", "--out", str(draft_path)]), 2)
            self.assertEqual(draft_path.read_bytes(), initial)
            session = finish(booster.replay_draft(limited.load_json(initial)))
            draft_path.write_bytes(build.canonical_bytes(session.record()))
            partitions, additions = plans(session)
            plans_path.write_text(json.dumps([{"deck": asdict(deck), "basic_additions": [asdict(r) for r in basics]}
                                              for deck, basics in zip(partitions, additions)]), encoding="utf-8")
            self.assertEqual(build.main(["build", "--draft", str(draft_path), "--partitions", str(plans_path),
                                         "--out", str(built_path)]), 0)
            output = io.StringIO()
            with redirect_stdout(output):
                self.assertEqual(build.main(["replay", str(built_path)]), 0)
            self.assertEqual(output.getvalue().strip(), hashlib.sha256(built_path.read_bytes().rstrip(b"\n")).hexdigest())


if __name__ == "__main__":
    unittest.main()
