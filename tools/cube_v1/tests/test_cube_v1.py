"""Unit tests for tools/cube_v1/pool.py and tools/cube_v1/deal.py."""

from __future__ import annotations

import copy
import hashlib
import io
import json
from pathlib import Path
import sys
import tempfile
import unittest
from contextlib import redirect_stderr, redirect_stdout

CUBE_V1_DIR = Path(__file__).resolve().parent.parent
EX1_DIR = CUBE_V1_DIR.parent / "ex1"
FIXTURES_DIR = Path(__file__).resolve().parent / "fixtures"

sys.path.insert(0, str(CUBE_V1_DIR))
sys.path.insert(0, str(EX1_DIR))

import deal  # noqa: E402
import pool  # noqa: E402
import qual  # noqa: E402


def _read_fixture(name: str) -> object:
    return json.loads((FIXTURES_DIR / name).read_text(encoding="utf-8"))


def _sha256_file(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


class PoolTests(unittest.TestCase):
    def test_build_pool_separates_sorts_hashes_and_excludes_gaps_and_fence(self):
        built = pool.build_pool_from_paths(
            FIXTURES_DIR / "cube_list.json",
            FIXTURES_DIR / "gaps.json",
            FIXTURES_DIR / "id_export.json",
            FIXTURES_DIR / "fence.json",
            expected_carddb_hash="1234567890abcdef",
        )
        self.assertEqual(built["schema"], "cube_pool/v1")
        self.assertEqual(built["kernel_carddb_hash"], "1234567890abcdef")

        # Every input sha256 is recorded verbatim.
        self.assertEqual(
            built["input_sha256"],
            {
                "cube_list": _sha256_file(FIXTURES_DIR / "cube_list.json"),
                "gaps": _sha256_file(FIXTURES_DIR / "gaps.json"),
                "id_export": _sha256_file(FIXTURES_DIR / "id_export.json"),
                "fence": _sha256_file(FIXTURES_DIR / "fence.json"),
            },
        )

        # Spells, nonbasic lands and basics are separated and sorted by ascending card_id.
        spell_ids = [c["card_id"] for c in built["spells"]]
        land_ids = [c["card_id"] for c in built["nonbasic_lands"]]
        basic_ids = [c["card_id"] for c in built["basics"]]
        self.assertEqual(spell_ids, sorted(spell_ids))
        self.assertEqual(land_ids, sorted(land_ids))
        self.assertEqual(basic_ids, sorted(basic_ids))
        self.assertEqual(len(built["spells"]), 29)
        self.assertEqual(len(built["nonbasic_lands"]), 8)
        self.assertEqual(len(built["basics"]), 5)
        self.assertTrue(all(not c["is_land"] for c in built["spells"]))
        self.assertTrue(all(c["is_land"] for c in built["nonbasic_lands"]))
        self.assertTrue(all(c["is_land"] for c in built["basics"]))

        # Neither gap cards nor fenced cards appear anywhere in the active pool.
        all_names = {c["name"] for c in built["spells"] + built["nonbasic_lands"] + built["basics"]}
        for excluded in (
            "Fixture Partial 1",
            "Fixture Fdn Only 1",
            "Fixture Missing 1",
            "Fixture Fenced Spell",
            "Fixture Fenced Land",
        ):
            self.assertNotIn(excluded, all_names)

    def test_empty_fence_file_is_accepted(self):
        with tempfile.TemporaryDirectory() as tmp:
            empty_fence = Path(tmp) / "empty_fence.json"
            empty_fence.write_text("[]\n", encoding="utf-8")
            built = pool.build_pool_from_paths(
                FIXTURES_DIR / "cube_list.json",
                FIXTURES_DIR / "gaps.json",
                FIXTURES_DIR / "id_export.json",
                empty_fence,
            )
            spell_names = {c["name"] for c in built["spells"]}
            land_names = {c["name"] for c in built["nonbasic_lands"]}
            self.assertIn("Fixture Fenced Spell", spell_names)
            self.assertIn("Fixture Fenced Land", land_names)
            self.assertEqual(built["fenced"], [])

    def test_refuses_fenced_name_not_in_cube_list(self):
        with tempfile.TemporaryDirectory() as tmp:
            bad_fence = Path(tmp) / "bad_fence.json"
            bad_fence.write_text(
                json.dumps([{"name": "Not In Cube", "reason": "unknown card"}]),
                encoding="utf-8",
            )
            with self.assertRaisesRegex(ValueError, "not in the cube list"):
                pool.build_pool_from_paths(
                    FIXTURES_DIR / "cube_list.json",
                    FIXTURES_DIR / "gaps.json",
                    FIXTURES_DIR / "id_export.json",
                    bad_fence,
                )

    def test_refuses_pool_card_with_no_definition(self):
        export_doc = copy.deepcopy(_read_fixture("id_export.json"))
        for entry in export_doc["cards"]:
            if entry["name"] == "Fixture White 1":
                entry["has_definition"] = False
                entry["card_id"] = None
                break
        with self.assertRaisesRegex(ValueError, "no definition"):
            pool.build_pool(
                _read_fixture("cube_list.json"),
                _read_fixture("gaps.json"),
                export_doc,
                _read_fixture("fence.json"),
                input_sha256={"cube_list": "a", "gaps": "b", "id_export": "c", "fence": "d"},
            )

    def test_refuses_mismatched_carddb_hash_cli_and_api(self):
        with self.assertRaisesRegex(ValueError, "differs from expected"):
            pool.build_pool_from_paths(
                FIXTURES_DIR / "cube_list.json",
                FIXTURES_DIR / "gaps.json",
                FIXTURES_DIR / "id_export.json",
                FIXTURES_DIR / "fence.json",
                expected_carddb_hash="ffffffffffffffff",
            )
        err = io.StringIO()
        with redirect_stderr(err):
            rc = pool.main(
                [
                    "--cube-list",
                    str(FIXTURES_DIR / "cube_list.json"),
                    "--gaps",
                    str(FIXTURES_DIR / "gaps.json"),
                    "--id-export",
                    str(FIXTURES_DIR / "id_export.json"),
                    "--fence",
                    str(FIXTURES_DIR / "fence.json"),
                    "--carddb-hash",
                    "ffffffffffffffff",
                ]
            )
        self.assertEqual(rc, 1)
        self.assertIn("differs from expected", err.getvalue())

    def test_cli_writes_output_file(self):
        with tempfile.TemporaryDirectory() as tmp:
            out_path = Path(tmp) / "pool.json"
            rc = pool.main(
                [
                    "--cube-list",
                    str(FIXTURES_DIR / "cube_list.json"),
                    "--gaps",
                    str(FIXTURES_DIR / "gaps.json"),
                    "--id-export",
                    str(FIXTURES_DIR / "id_export.json"),
                    "--fence",
                    str(FIXTURES_DIR / "fence.json"),
                    "--carddb-hash",
                    "1234567890abcdef",
                    "--out",
                    str(out_path),
                ]
            )
            self.assertEqual(rc, 0)
            written = json.loads(out_path.read_text(encoding="utf-8"))
            self.assertEqual(written["kernel_carddb_hash"], "1234567890abcdef")
            self.assertEqual(len(written["spells"]), 29)


class DealGuaranteeTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.tmp_path = Path(self.tmp.name)
        self.pool_path = self.tmp_path / "pool.json"
        self.pool_doc = pool.build_pool_from_paths(
            FIXTURES_DIR / "cube_list.json",
            FIXTURES_DIR / "gaps.json",
            FIXTURES_DIR / "id_export.json",
            FIXTURES_DIR / "fence.json",
            expected_carddb_hash="1234567890abcdef",
        )
        self.pool_path.write_text(pool.serialize_json(self.pool_doc), encoding="utf-8")
        self.constants_path = FIXTURES_DIR / "constants.json"
        self.constants_doc = _read_fixture("constants.json")
        self.basic_ids = {c["card_id"] for c in self.pool_doc["basics"]}
        self.basic_color_by_id = {c["card_id"]: c["color"] for c in self.pool_doc["basics"]}
        self.spell_by_id = {c["card_id"]: c for c in self.pool_doc["spells"]}
        self.nonbasic_land_ids = {c["card_id"] for c in self.pool_doc["nonbasic_lands"]}

    def tearDown(self):
        self.tmp.cleanup()

    def test_guarantee_byte_identical_determinism(self):
        out_a = self.tmp_path / "run_a.json"
        out_b = self.tmp_path / "run_b.json"
        for out_path in (out_a, out_b):
            rc = deal.main(
                [
                    "--pool",
                    str(self.pool_path),
                    "--constants",
                    str(self.constants_path),
                    "--namespace",
                    "train",
                    "--run",
                    "r01",
                    "--update-start",
                    "0",
                    "--update-end",
                    "3",
                    "--out",
                    str(out_path),
                ]
            )
            self.assertEqual(rc, 0)
        self.assertEqual(out_a.read_bytes(), out_b.read_bytes())

        # Sanity check that changing run changes output bytes.
        out_c = self.tmp_path / "run_c.json"
        rc = deal.main(
            [
                "--pool",
                str(self.pool_path),
                "--constants",
                str(self.constants_path),
                "--namespace",
                "train",
                "--run",
                "r02",
                "--update-start",
                "0",
                "--update-end",
                "3",
                "--out",
                str(out_c),
            ]
        )
        self.assertEqual(rc, 0)
        self.assertNotEqual(out_a.read_bytes(), out_c.read_bytes())

    def test_guarantee_no_fenced_card_appears(self):
        # Fenced card ids from id_export.json: 410 (colourless spell) and 411 (nonbasic land).
        fenced_ids = {410, 411}
        schedule = deal.build_schedule_from_paths(
            self.pool_path,
            self.constants_path,
            namespace="train",
            run="fence-check",
            update_start=0,
            update_end=6,
        )
        for iteration in schedule["iterations"]:
            for entry in iteration["episodes"]:
                ep = entry["episode"]
                for deck in ep["registered"] + ep["selected"]:
                    self.assertTrue(
                        fenced_ids.isdisjoint(deck["mainboard"]),
                        f"fenced card leaked into {ep['id']}: {deck['mainboard']}",
                    )

        # Control: with an empty fence, both 410 and 411 are eligible and appear across the same range.
        empty_fence = self.tmp_path / "empty_fence.json"
        empty_fence.write_text("[]\n", encoding="utf-8")
        unfenced_pool = pool.build_pool_from_paths(
            FIXTURES_DIR / "cube_list.json",
            FIXTURES_DIR / "gaps.json",
            FIXTURES_DIR / "id_export.json",
            empty_fence,
        )
        unfenced_schedule = deal.build_schedule(
            unfenced_pool,
            self.constants_doc,
            namespace="train",
            run="fence-check",
            update_start=0,
            update_end=6,
        )
        seen_unfenced = {
            cid
            for it in unfenced_schedule["iterations"]
            for entry in it["episodes"]
            for deck in entry["episode"]["registered"]
            for cid in deck["mainboard"]
        }
        self.assertTrue(fenced_ids.issubset(seen_unfenced))

    def test_guarantee_within_episode_no_cube_card_appears_in_both_decks(self):
        schedule = deal.build_schedule_from_paths(
            self.pool_path,
            self.constants_path,
            namespace="train",
            run="disjoint-check",
            update_start=0,
            update_end=8,
        )
        for iteration in schedule["iterations"]:
            for entry in iteration["episodes"]:
                ep = entry["episode"]
                deck0 = ep["registered"][0]["mainboard"]
                deck1 = ep["registered"][1]["mainboard"]
                nonbasic0 = [cid for cid in deck0 if cid not in self.basic_ids]
                nonbasic1 = [cid for cid in deck1 if cid not in self.basic_ids]
                # Singleton within each deck.
                self.assertEqual(len(nonbasic0), len(set(nonbasic0)))
                self.assertEqual(len(nonbasic1), len(set(nonbasic1)))
                # Disjoint across the two decks of the episode.
                self.assertEqual(set(nonbasic0) & set(nonbasic1), set())

    def test_guarantee_exact_deck_size_and_composition_and_basic_split(self):
        # Test multiple constant sets (no hard-coded 40 or 2).
        configs = [
            {
                "deck_size": 10,
                "colours_per_seat": 2,
                "nonland_spells": 5,
                "nonbasic_lands": 2,
                "basic_split": "even_first_colour_extra",
                "pairs_per_update": 4,
            },
            {
                "deck_size": 12,
                "colours_per_seat": 3,
                "nonland_spells": 5,
                "nonbasic_lands": 2,
                "basic_split": "even_first_colour_extra",
                "pairs_per_update": 3,
            },
            {
                "deck_size": 8,
                "colours_per_seat": 1,
                "nonland_spells": 4,
                "nonbasic_lands": 0,
                "basic_split": "even_first_colour_extra",
                "pairs_per_update": 3,
            },
        ]
        for idx, constants in enumerate(configs):
            schedule = deal.build_schedule(
                self.pool_doc,
                constants,
                namespace="train",
                run=f"size-{idx}",
                update_start=0,
                update_end=3,
            )
            expected_basics = constants["deck_size"] - constants["nonland_spells"] - constants["nonbasic_lands"]
            base_q, rem = divmod(expected_basics, constants["colours_per_seat"])
            expected_basic_counts = [base_q + (1 if i < rem else 0) for i in range(constants["colours_per_seat"])]

            for iteration in schedule["iterations"]:
                self.assertEqual(len(iteration["episodes"]), 2 * constants["pairs_per_update"])
                for entry in iteration["episodes"]:
                    ep = entry["episode"]
                    for deck in ep["registered"]:
                        mainboard = deck["mainboard"]
                        self.assertEqual(len(mainboard), constants["deck_size"])
                        self.assertEqual(deck["sideboard"], [])
                        seat_colors = list(deck["label"].rsplit("-", 1)[-1])
                        self.assertEqual(len(seat_colors), constants["colours_per_seat"])
                        self.assertEqual(len(set(seat_colors)), constants["colours_per_seat"])

                        spells = [cid for cid in mainboard if cid in self.spell_by_id]
                        lands = [cid for cid in mainboard if cid in self.nonbasic_land_ids]
                        basics = [cid for cid in mainboard if cid in self.basic_ids]
                        self.assertEqual(len(spells), constants["nonland_spells"])
                        self.assertEqual(len(lands), constants["nonbasic_lands"])
                        self.assertEqual(len(basics), expected_basics)

                        # Every nonland spell's colours lie inside the seat's colours (colourless allowed).
                        seat_color_set = set(seat_colors)
                        for cid in spells:
                            self.assertTrue(set(self.spell_by_id[cid]["colors"]).issubset(seat_color_set))

                        # Basics follow even_first_colour_extra in the seat's colour order.
                        actual_counts = [
                            sum(1 for cid in basics if self.basic_color_by_id[cid] == color)
                            for color in seat_colors
                        ]
                        self.assertEqual(actual_counts, expected_basic_counts)

    def test_guarantee_heldout_namespace_never_collides_on_id_or_seed(self):
        train = deal.build_schedule_from_paths(
            self.pool_path,
            self.constants_path,
            namespace="train-cube-v1",
            run="cube-r1",
            update_start=0,
            update_end=10,
        )
        heldout = deal.build_schedule_from_paths(
            self.pool_path,
            self.constants_path,
            namespace="heldout-cube-v1",
            run="cube-r1",
            update_start=0,
            update_end=10,
        )
        train_ids = {e["episode"]["id"] for it in train["iterations"] for e in it["episodes"]}
        heldout_ids = {e["episode"]["id"] for it in heldout["iterations"] for e in it["episodes"]}
        train_seeds = {e["episode"]["seed"] for it in train["iterations"] for e in it["episodes"]}
        heldout_seeds = {e["episode"]["seed"] for it in heldout["iterations"] for e in it["episodes"]}

        expected_count = 10 * 2 * self.constants_doc["pairs_per_update"]
        self.assertEqual(len(train_ids), expected_count)
        self.assertEqual(len(heldout_ids), expected_count)
        self.assertEqual(len(train_seeds), expected_count)
        self.assertEqual(len(heldout_seeds), expected_count)
        self.assertEqual(train_ids & heldout_ids, set())
        self.assertEqual(train_seeds & heldout_seeds, set())

    def test_both_seat_orders_and_qual_seed_parity_and_run_config_shape(self):
        schedule = deal.build_schedule_from_paths(
            self.pool_path,
            self.constants_path,
            namespace="train",
            run="order-check",
            update_start=5,
            update_end=7,
        )
        self.assertEqual(len(schedule["iterations"]), 2)
        for offset, iteration in enumerate(schedule["iterations"]):
            update = 5 + offset
            episodes = iteration["episodes"]
            self.assertEqual(len(episodes), 2 * self.constants_doc["pairs_per_update"])
            for p in range(self.constants_doc["pairs_per_update"]):
                e0 = episodes[2 * p]
                e1 = episodes[2 * p + 1]
                ep0, ep1 = e0["episode"], e1["episode"]

                self.assertEqual(e0["opponent"], {"kind": "current"})
                self.assertEqual(e1["opponent"], {"kind": "current"})
                self.assertEqual(ep0["postboard"], False)
                self.assertEqual(ep1["postboard"], False)
                self.assertEqual(ep0["selected"], ep0["registered"])
                self.assertEqual(ep1["selected"], ep1["registered"])
                self.assertEqual(ep0["starting_player"], 0)
                self.assertEqual(ep1["starting_player"], 0)
                self.assertEqual(ep0["learner_seat"], 0)
                self.assertEqual(ep1["learner_seat"], 1)

                # Both seat orders: [A, B] then [B, A].
                self.assertEqual(ep0["registered"][0], ep1["registered"][1])
                self.assertEqual(ep0["registered"][1], ep1["registered"][0])

                # Seed matches tools/ex1/qual.py::seed bit for bit.
                self.assertEqual(ep0["seed"], qual.seed(["train", "order-check", update, 2 * p]))
                self.assertEqual(ep1["seed"], qual.seed(["train", "order-check", update, 2 * p + 1]))

    def test_refuses_invalid_or_incomplete_constants(self):
        for key in deal.REQUIRED_CONSTANT_KEYS:
            bad = dict(self.constants_doc)
            del bad[key]
            with self.assertRaises(ValueError):
                deal.validate_constants(bad)

        extra = dict(self.constants_doc, unexpected=1)
        with self.assertRaisesRegex(ValueError, "unknown keys"):
            deal.validate_constants(extra)

        bad_split = dict(self.constants_doc, basic_split="weighted")
        with self.assertRaisesRegex(ValueError, "basic_split"):
            deal.validate_constants(bad_split)

        overflow = dict(self.constants_doc, nonland_spells=9, nonbasic_lands=2)
        with self.assertRaisesRegex(ValueError, "exceeds deck_size"):
            deal.validate_constants(overflow)

        bool_size = dict(self.constants_doc, deck_size=True)
        with self.assertRaises(ValueError):
            deal.validate_constants(bool_size)

    def test_base_config_stamping_preserves_run_config_fields(self):
        base = {
            "schema": "mtg-kernel-native-expanded-training-run/v1",
            "initial_source": {"play_import": {"path": "import.json", "sha256": "0" * 64}, "checkpoint": None},
            "opponents": [],
            "iterations": [
                {
                    "episodes": [
                        {
                            "episode": {
                                "id": "placeholder",
                                "seed": 1,
                                "starting_player": 0,
                                "learner_seat": 0,
                                "registered": [],
                                "selected": [],
                                "postboard": False,
                                "max_physical_decisions": 4000,
                                "max_policy_steps": 40000,
                                "runtime_rules": "resolution-boundary-v1",
                            },
                            "opponent": {"kind": "current"},
                        }
                    ]
                }
            ],
            "learning_rate": 0.0001,
            "value_coefficient": 0.5,
            "output_directory": "out/cube",
        }
        stamped = deal.build_schedule(
            self.pool_doc,
            self.constants_doc,
            namespace="train",
            run="base-test",
            update_start=0,
            update_end=2,
            base_config=base,
        )
        self.assertEqual(set(stamped.keys()), set(base.keys()))
        self.assertEqual(len(stamped["iterations"]), 2)
        ep0 = stamped["iterations"][0]["episodes"][0]["episode"]
        self.assertEqual(ep0["max_physical_decisions"], 4000)
        self.assertEqual(ep0["max_policy_steps"], 40000)
        self.assertEqual(ep0["runtime_rules"], "resolution-boundary-v1")

    def test_cli_stdout_and_error_exit(self):
        buf = io.StringIO()
        with redirect_stdout(buf):
            rc = deal.main(
                [
                    "--pool",
                    str(self.pool_path),
                    "--constants",
                    str(self.constants_path),
                    "--namespace",
                    "train",
                    "--run",
                    "stdout-test",
                    "--update-start",
                    "0",
                    "--update-end",
                    "1",
                ]
            )
        self.assertEqual(rc, 0)
        parsed = json.loads(buf.getvalue())
        self.assertEqual(len(parsed["iterations"]), 1)

        err = io.StringIO()
        with redirect_stderr(err):
            rc_bad = deal.main(
                [
                    "--pool",
                    str(self.pool_path),
                    "--constants",
                    str(self.constants_path),
                    "--namespace",
                    "train",
                    "--run",
                    "bad-range",
                    "--update-start",
                    "3",
                    "--update-end",
                    "3",
                ]
            )
        self.assertEqual(rc_bad, 1)
        self.assertIn("update_end", err.getvalue())


if __name__ == "__main__":
    unittest.main()
