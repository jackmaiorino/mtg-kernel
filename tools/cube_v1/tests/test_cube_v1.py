"""Unit tests for tools/cube_v1/pool.py and tools/cube_v1/deal.py."""

from __future__ import annotations

import copy
import hashlib
import importlib.util
import io
import json
import os
from pathlib import Path
import subprocess
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


class CiRegistrationTests(unittest.TestCase):
    def test_cube_v1_registered_in_ci_tools_scope_commands(self):
        ci_scope_path = CUBE_V1_DIR.parent / "ci" / "ci_scope.py"
        spec = importlib.util.spec_from_file_location("ci_scope", ci_scope_path)
        self.assertIsNotNone(spec)
        assert spec is not None and spec.loader is not None
        ci_scope = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(ci_scope)

        self.assertIn("cube_v1", ci_scope.TOOLS_DIRS)
        self.assertEqual(ci_scope.scope_of_files(["tools/cube_v1/deal.py"]), "tools")
        expected = "uv run --no-sync python -m unittest discover -v -s tools/cube_v1/tests -p 'test_*.py'"
        self.assertIn(expected, ci_scope.TOOLS_COMMANDS)


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


class LfOutputTests(unittest.TestCase):
    """pool.py and deal.py write UTF-8 bytes with LF line endings on every OS (file and stdout)."""

    def _run(self, script: str, args: list[str]) -> bytes:
        proc = subprocess.run(
            [sys.executable, str(CUBE_V1_DIR / script), *args],
            capture_output=True,
            check=False,
        )
        self.assertEqual(proc.returncode, 0, proc.stderr.decode("utf-8", "replace"))
        return proc.stdout

    def test_pool_and_deal_write_lf_bytes_to_file_and_stdout(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            pool_args = [
                "--cube-list",
                str(FIXTURES_DIR / "cube_list.json"),
                "--gaps",
                str(FIXTURES_DIR / "gaps.json"),
                "--id-export",
                str(FIXTURES_DIR / "id_export.json"),
                "--fence",
                str(FIXTURES_DIR / "fence.json"),
            ]
            pool_file = tmp_path / "pool.json"
            self.assertEqual(self._run("pool.py", pool_args + ["--out", str(pool_file)]), b"")
            pool_stdout = self._run("pool.py", pool_args)
            pool_bytes = pool_file.read_bytes()
            for data in (pool_bytes, pool_stdout):
                self.assertNotIn(b"\r", data)
                self.assertIn(b"\n", data)
                data.decode("utf-8")
            self.assertEqual(pool_bytes, pool_stdout)

            deal_args = [
                "--pool",
                str(pool_file),
                "--constants",
                str(FIXTURES_DIR / "constants.json"),
                "--namespace",
                "train",
                "--run",
                "lf",
                "--update-start",
                "0",
                "--update-end",
                "2",
            ]
            deal_file = tmp_path / "deal.json"
            self.assertEqual(self._run("deal.py", deal_args + ["--out", str(deal_file)]), b"")
            deal_stdout = self._run("deal.py", deal_args)
            deal_bytes = deal_file.read_bytes()
            for data in (deal_bytes, deal_stdout):
                self.assertNotIn(b"\r", data)
                self.assertIn(b"\n", data)
                data.decode("utf-8")
            self.assertEqual(deal_bytes, deal_stdout)


class CanonicalDeckHashTests(unittest.TestCase):
    def test_canonical_text_is_sorted_name_count_lines_with_lf(self):
        text = deal.canonical_deck_text(["Island", "Brainstorm", "Island", "Ancestral Recall"])
        self.assertEqual(text, "Ancestral Recall 1\nBrainstorm 1\nIsland 2\n")

    def test_hash_is_the_sha256_of_the_stated_text(self):
        decks = [["Island", "Brainstorm", "Island"], ["Mountain", "Lightning Bolt"]]
        expected_text = "Brainstorm 1\nIsland 2\n\nLightning Bolt 1\nMountain 1\n\n"
        self.assertEqual(
            deal.canonical_decks_sha256(decks),
            hashlib.sha256(expected_text.encode("utf-8")).hexdigest(),
        )
        # Non-ASCII names hash as UTF-8.
        self.assertEqual(
            deal.canonical_decks_sha256([["Lim-Dûl's Vault"]]),
            hashlib.sha256("Lim-Dûl's Vault 1\n\n".encode("utf-8")).hexdigest(),
        )

    def test_hash_ignores_card_order_and_file_formatting(self):
        deck = ["Island", "Brainstorm", "Island", "Ancestral Recall", "Swamp"]
        reference = deal.canonical_decks_sha256([deck])
        self.assertEqual(deal.canonical_decks_sha256([list(reversed(deck))]), reference)
        self.assertEqual(deal.canonical_decks_sha256([sorted(deck)]), reference)
        # The same deck stored compact with LF, or indented with CRLF, hashes the same.
        compact = json.dumps({"cards": deck}, separators=(",", ":")).encode("utf-8")
        indented = json.dumps({"cards": deck, "pool_sha256": "f" * 64}, indent=4).replace("\n", "\r\n").encode("utf-8")
        for raw in (compact, indented):
            self.assertEqual(deal.canonical_decks_sha256([json.loads(raw.decode("utf-8"))["cards"]]), reference)

    def test_hash_depends_on_counts_names_and_deck_order(self):
        a = ["Island", "Brainstorm"]
        b = ["Mountain", "Lightning Bolt"]
        reference = deal.canonical_decks_sha256([a, b])
        self.assertNotEqual(deal.canonical_decks_sha256([b, a]), reference)
        self.assertNotEqual(deal.canonical_decks_sha256([a + ["Island"], b]), reference)
        self.assertNotEqual(deal.canonical_decks_sha256([a + b]), reference)
        self.assertNotEqual(deal.canonical_decks_sha256([a]), reference)

    def test_refuses_empty_decks_and_bad_names(self):
        with self.assertRaises(ValueError):
            deal.canonical_decks_sha256([[]])
        for bad in ("", "  ", "Island\n", "Is\r\nland", 7, None):
            with self.assertRaises(ValueError):
                deal.canonical_deck_text(["Island", bad])

    def test_deck_ids_translate_to_names_through_the_pool(self):
        pool_doc = pool.build_pool_from_paths(
            FIXTURES_DIR / "cube_list.json",
            FIXTURES_DIR / "gaps.json",
            FIXTURES_DIR / "id_export.json",
            FIXTURES_DIR / "fence.json",
        )
        names_by_id = deal.card_names_by_id(pool_doc)
        schedule = deal.build_schedule(
            pool_doc, _read_fixture("constants.json"), namespace="golden", run="hash", update_start=0, update_end=1
        )
        deck = schedule["iterations"][0]["episodes"][0]["episode"]["registered"][0]["mainboard"]
        names = deal.deck_card_names(deck, names_by_id)
        self.assertEqual(len(names), len(deck))
        self.assertEqual(names, [names_by_id[cid] for cid in deck])
        # Basic lands translate too.
        basic_names = {card["name"] for card in pool_doc["basics"]}
        self.assertTrue(basic_names & set(names))
        with self.assertRaisesRegex(ValueError, "not in the pool"):
            deal.deck_card_names([65535], names_by_id)


class ReadBDealerTests(unittest.TestCase):
    def setUp(self):
        self.pool_doc = pool.build_pool_from_paths(
            FIXTURES_DIR / "cube_list.json",
            FIXTURES_DIR / "gaps.json",
            FIXTURES_DIR / "id_export.json",
            FIXTURES_DIR / "fence.json",
        )
        self.pool_state = deal.validate_pool(self.pool_doc)
        self.constants = deal.validate_constants(_read_fixture("constants.json"))
        self.basic_ids = set(self.pool_state["basic_id_by_color"].values())
        self.spell_by_id = {c["card_id"]: c for c in self.pool_doc["spells"]}
        self.land_ids = {c["card_id"] for c in self.pool_doc["nonbasic_lands"]}
        # A drafted deck D: one dealt deck of the pair dealer stands in for a drafted deck.
        deck_a, _ = deal.deal_pair(self.pool_state, self.constants, "fixture", "drafted", 0, 0)
        self.drafted = list(deck_a["mainboard"])

    def _nonbasic(self, ids):
        return {cid for cid in ids if cid not in self.basic_ids}

    def test_opponent_excludes_drafted_nonbasic_cards(self):
        drafted_nonbasic = self._nonbasic(self.drafted)
        self.assertTrue(drafted_nonbasic)
        for index in range(40):
            opponent = deal.deal_read_b_opponent(
                self.pool_state, self.constants, self.drafted, "read-b", "r1", index
            )["mainboard"]
            self.assertFalse(self._nonbasic(opponent) & drafted_nonbasic, f"index {index}")
            # Same constants as the pair dealer: deck size and composition.
            self.assertEqual(len(opponent), self.constants["deck_size"])
            self.assertEqual(sum(1 for c in opponent if c in self.spell_by_id), self.constants["nonland_spells"])
            self.assertEqual(sum(1 for c in opponent if c in self.land_ids), self.constants["nonbasic_lands"])

    def test_exclusion_binds_where_the_unexcluded_deal_would_overlap(self):
        # With D holding only basics nothing is excluded; that deal's opponent O0 is then put in D.
        # The new opponent for the same key must avoid every nonbasic card of O0.
        basics_only = sorted(self.basic_ids)
        o0 = deal.deal_read_b_opponent(self.pool_state, self.constants, basics_only, "read-b", "r1", 7)
        o0_nonbasic = self._nonbasic(o0["mainboard"])
        drafted = basics_only + sorted(o0_nonbasic)
        o1 = deal.deal_read_b_opponent(self.pool_state, self.constants, drafted, "read-b", "r1", 7)
        self.assertFalse(self._nonbasic(o1["mainboard"]) & o0_nonbasic)
        # The colours are drawn before the exclusion applies, so they do not move.
        self.assertEqual(o0["label"], o1["label"])

    def test_refuses_a_drafted_deck_outside_the_pool_and_an_exhausted_pool(self):
        with self.assertRaisesRegex(ValueError, "neither a card of the active pool nor a basic land"):
            deal.build_read_b(self.pool_doc, self.constants, self.drafted + [410], "read-b", "r1", 0)
        for bad in ([], "1,2", [1.5], [True]):
            with self.assertRaises(ValueError):
                deal.build_read_b(self.pool_doc, self.constants, bad, "read-b", "r1", 0)
        with self.assertRaisesRegex(ValueError, "index"):
            deal.build_read_b(self.pool_doc, self.constants, self.drafted, "read-b", "r1", -1)
        everything = sorted(self.spell_by_id) + sorted(self.land_ids)
        with self.assertRaisesRegex(ValueError, "only 0 remain"):
            deal.build_read_b(self.pool_doc, self.constants, everything, "read-b", "r1", 0)

    def test_deterministic_and_keyed_by_namespace_run_and_index(self):
        first = deal.build_read_b(self.pool_doc, self.constants, self.drafted, "read-b", "r1", 3)
        again = deal.build_read_b(
            copy.deepcopy(self.pool_doc), dict(self.constants), list(reversed(self.drafted)), "read-b", "r1", 3
        )
        self.assertEqual(first, again)
        self.assertEqual(deal.serialize_json(first), deal.serialize_json(again))
        keys = [("read-b", "r1", i) for i in range(12)] + [("read-b", "r2", 3), ("golden", "r1", 3)]
        opponents = {
            tuple(deal.deal_read_b_opponent(self.pool_state, self.constants, self.drafted, *key)["mainboard"])
            for key in keys
        }
        self.assertGreater(len(opponents), 1)

    def test_four_games_two_on_the_play_two_on_the_draw_with_distinct_seeds(self):
        doc = deal.build_read_b(self.pool_doc, self.constants, self.drafted, "read-b", "r1", 5)
        self.assertEqual(doc["schema"], "cube_deal_read_b/v1")
        self.assertEqual(len(doc["iterations"]), 1)
        episodes = doc["iterations"][0]["episodes"]
        self.assertEqual(len(episodes), 4)
        drafted = doc["drafted_deck"]
        opponent = doc["opponent_deck"]
        self.assertEqual(drafted["mainboard"], sorted(self.drafted))
        on_play = on_draw = 0
        seeds = set()
        ids = set()
        for slot, entry in enumerate(episodes):
            ep = entry["episode"]
            self.assertEqual(entry["opponent"], {"kind": "current"})
            self.assertEqual(ep["starting_player"], 0)
            seat = ep["learner_seat"]
            self.assertEqual(ep["registered"][seat], drafted)
            self.assertEqual(ep["registered"][1 - seat], opponent)
            self.assertEqual(ep["selected"], ep["registered"])
            if seat == 0:
                on_play += 1
            else:
                on_draw += 1
            self.assertEqual(ep["seed"], deal.seed(["cube_deal_b_v1", "read-b", "r1", 5, slot]))
            self.assertEqual(ep["id"], f"6:read-b/2:r1/b5/j{slot}/s{seat}")
            seeds.add(ep["seed"])
            ids.add(ep["id"])
        self.assertEqual((on_play, on_draw), (2, 2))
        self.assertEqual(len(seeds), 4)
        self.assertEqual(len(ids), 4)
        # The read (b) key prefix and seeds are distinct from the pair dealer's for the same strings.
        schedule = deal.build_schedule(self.pool_doc, self.constants, "read-b", "r1", 5, 6)
        pair_seeds = {e["episode"]["seed"] for e in schedule["iterations"][0]["episodes"]}
        self.assertFalse(seeds & pair_seeds)

    def test_base_config_stamping_follows_the_drafted_deck_seat(self):
        base = {
            "schema": "mtg-kernel-native-expanded-training-run/v1",
            "iterations": [
                {
                    "episodes": [
                        {
                            "episode": {
                                "learner_seat": 0,
                                "max_physical_decisions": 16384,
                                "max_policy_steps": 32768,
                                "mulligan_modes": ["learned", "keep_seven"],
                            }
                        }
                    ]
                }
            ],
            "output_directory": "out/read-b",
        }
        stamped = deal.build_read_b(self.pool_doc, self.constants, self.drafted, "read-b", "r1", 1, base_config=base)
        self.assertEqual(set(stamped), set(base))
        self.assertEqual(stamped["output_directory"], "out/read-b")
        for entry in stamped["iterations"][0]["episodes"]:
            ep = entry["episode"]
            self.assertEqual((ep["max_physical_decisions"], ep["max_policy_steps"]), (16384, 32768))
            self.assertEqual(ep["mulligan_modes"][ep["learner_seat"]], "learned")
            self.assertEqual(ep["registered"][ep["learner_seat"]]["mainboard"], sorted(self.drafted))

    def test_cli_read_b_is_byte_deterministic_and_validates_flags(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            pool_path = tmp_path / "pool.json"
            pool_path.write_bytes(pool.serialize_json(self.pool_doc).encode("utf-8"))
            deck_list = tmp_path / "deck_list.json"
            deck_list.write_bytes(json.dumps(self.drafted).encode("utf-8"))
            deck_obj = tmp_path / "deck_obj.json"
            deck_obj.write_bytes(json.dumps({"label": "x", "mainboard": self.drafted}, indent=1).encode("utf-8"))
            common = [
                "--pool",
                str(pool_path),
                "--constants",
                str(FIXTURES_DIR / "constants.json"),
                "--namespace",
                "read-b",
                "--run",
                "r1",
                "--index",
                "2",
            ]
            outputs = []
            for deck_path, hash_seed in ((deck_list, "1"), (deck_list, "42"), (deck_obj, "7")):
                env = os.environ.copy()
                env["PYTHONHASHSEED"] = hash_seed
                proc = subprocess.run(
                    [sys.executable, str(CUBE_V1_DIR / "deal.py"), *common, "--read-b-deck", str(deck_path)],
                    env=env,
                    capture_output=True,
                    check=False,
                )
                self.assertEqual(proc.returncode, 0, proc.stderr.decode("utf-8", "replace"))
                self.assertNotIn(b"\r", proc.stdout)
                outputs.append(json.loads(proc.stdout.decode("utf-8")))
            self.assertEqual(outputs[0], outputs[1])
            # Deck file formatting changes only the recorded file hash.
            strip = lambda d: {k: v for k, v in d.items() if k != "drafted_deck_sha256"}  # noqa: E731
            self.assertEqual(strip(outputs[0]), strip(outputs[2]))
            self.assertEqual(
                strip(outputs[0]),
                strip(deal.build_read_b(self.pool_doc, self.constants, self.drafted, "read-b", "r1", 2,
                                        pool_sha256=outputs[0]["pool_sha256"],
                                        constants_sha256=outputs[0]["constants_sha256"])),
            )

            without_index = common[:-2]
            refused = [
                # --read-b-deck without --index.
                without_index + ["--read-b-deck", str(deck_list)],
                # --read-b-deck with an update range.
                common + ["--read-b-deck", str(deck_list), "--update-start", "0", "--update-end", "1"],
                # --index without --read-b-deck.
                common + ["--update-start", "0", "--update-end", "1"],
                # Neither mode.
                without_index,
            ]
            for args in refused:
                with redirect_stderr(io.StringIO()), self.assertRaises(SystemExit) as raised:
                    deal.main(args)
                self.assertEqual(raised.exception.code, 2, args)


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
        def _run_subprocess(script: str, args: list[str], hash_seed: str) -> None:
            env = os.environ.copy()
            env["PYTHONHASHSEED"] = hash_seed
            proc = subprocess.run(
                [sys.executable, str(CUBE_V1_DIR / script), *args],
                env=env,
                capture_output=True,
                text=True,
                check=False,
            )
            self.assertEqual(
                proc.returncode,
                0,
                f"{script} (PYTHONHASHSEED={hash_seed}) failed: {proc.stderr}",
            )

        pool_a = self.tmp_path / "pool_a.json"
        pool_b = self.tmp_path / "pool_b.json"
        for out_path, hash_seed in ((pool_a, "1"), (pool_b, "42")):
            _run_subprocess(
                "pool.py",
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
                ],
                hash_seed,
            )
        self.assertEqual(pool_a.read_bytes(), pool_b.read_bytes())

        out_a = self.tmp_path / "run_a.json"
        out_b = self.tmp_path / "run_b.json"
        for pool_in, out_path, hash_seed in ((pool_a, out_a, "1"), (pool_b, out_b, "42")):
            _run_subprocess(
                "deal.py",
                [
                    "--pool",
                    str(pool_in),
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
                ],
                hash_seed,
            )
        self.assertEqual(out_a.read_bytes(), out_b.read_bytes())

        # Sanity check that changing run changes output bytes.
        out_c = self.tmp_path / "run_c.json"
        _run_subprocess(
            "deal.py",
            [
                "--pool",
                str(pool_a),
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
            ],
            "1",
        )
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

    def test_guarantee_within_episode_no_nonbasic_card_appears_in_both_decks(self):
        # Basic lands are explicitly exempt (both decks may hold copies of the same basic
        # land type); verify nothing other than the five basic lands is exempt.
        export_doc = _read_fixture("id_export.json")
        five_basic_ids = {
            c["card_id"] for c in export_doc["cards"] if c["name"] in pool.BASIC_COLORS
        }
        self.assertEqual(len(five_basic_ids), 5)
        self.assertEqual(self.basic_ids, five_basic_ids)

        three_color_constants = {
            "deck_size": 12,
            "colours_per_seat": 3,
            "nonland_spells": 5,
            "nonbasic_lands": 2,
            "basic_split": "even_first_colour_extra",
            "pairs_per_update": 4,
        }
        for run_name, constants, expect_shared_basics in (
            ("disjoint-2c", self.constants_doc, False),
            ("disjoint-3c", three_color_constants, True),
        ):
            schedule = deal.build_schedule(
                self.pool_doc,
                constants,
                namespace="train",
                run=run_name,
                update_start=0,
                update_end=8,
            )
            expected_nonbasics = constants["nonland_spells"] + constants["nonbasic_lands"]
            shared_basics_seen = False
            for iteration in schedule["iterations"]:
                for entry in iteration["episodes"]:
                    ep = entry["episode"]
                    for key in ("registered", "selected"):
                        deck0 = ep[key][0]["mainboard"]
                        deck1 = ep[key][1]["mainboard"]
                        overlap = set(deck0) & set(deck1)
                        if overlap:
                            shared_basics_seen = True
                        # Only the five basic lands may overlap across the two decks.
                        self.assertTrue(
                            overlap.issubset(five_basic_ids),
                            f"non-basic card(s) appeared in both decks of {ep['id']} ({key}): "
                            f"{sorted(overlap - five_basic_ids)}",
                        )
                        # Non-basic cards remain singleton within each deck and match expected count.
                        nonbasic0 = [cid for cid in deck0 if cid not in five_basic_ids]
                        nonbasic1 = [cid for cid in deck1 if cid not in five_basic_ids]
                        self.assertEqual(len(nonbasic0), expected_nonbasics)
                        self.assertEqual(len(nonbasic1), expected_nonbasics)
                        self.assertEqual(len(nonbasic0), len(set(nonbasic0)))
                        self.assertEqual(len(nonbasic1), len(set(nonbasic1)))
            if expect_shared_basics:
                self.assertTrue(shared_basics_seen)

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
                "pairs_per_update": 2,
            },
            {
                "deck_size": 8,
                "colours_per_seat": 1,
                "nonland_spells": 4,
                "nonbasic_lands": 0,
                "basic_split": "even_first_colour_extra",
                "pairs_per_update": 2,
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

    def test_episode_ids_never_collide_across_namespace_run_boundaries(self):
        pairs = [
            (("train", "cube-r1"), ("train-cube", "r1")),
            (("a/1:b", "c"), ("a", "b/1:c")),
            (("a:b", "c"), ("a", "b:c")),
        ]
        for (ns_a, run_a), (ns_b, run_b) in pairs:
            with self.subTest(a=(ns_a, run_a), b=(ns_b, run_b)):
                sched_a = deal.build_schedule(
                    self.pool_doc,
                    self.constants_doc,
                    namespace=ns_a,
                    run=run_a,
                    update_start=0,
                    update_end=3,
                )
                sched_b = deal.build_schedule(
                    self.pool_doc,
                    self.constants_doc,
                    namespace=ns_b,
                    run=run_b,
                    update_start=0,
                    update_end=3,
                )
                ids_a = {e["episode"]["id"] for it in sched_a["iterations"] for e in it["episodes"]}
                ids_b = {e["episode"]["id"] for it in sched_b["iterations"] for e in it["episodes"]}
                seeds_a = {e["episode"]["seed"] for it in sched_a["iterations"] for e in it["episodes"]}
                seeds_b = {e["episode"]["seed"] for it in sched_b["iterations"] for e in it["episodes"]}
                self.assertEqual(ids_a & ids_b, set())
                self.assertEqual(seeds_a & seeds_b, set())

    def test_episode_id_length_limit_counts_utf8_bytes(self):
        # "\u00e9" is 1 Python character and 2 UTF-8 bytes.
        # "110:" (4) + 55*2 (110) + "/2:r1" (5) + "/u0/j0/s0" (9) = 128 UTF-8 bytes (73 chars).
        ns_exact = "\u00e9" * 55
        ok_id = deal.format_episode_id(ns_exact, "r1", 0, 0, 0)
        self.assertEqual(len(ok_id), 73)
        self.assertEqual(len(ok_id.encode("utf-8")), 128)
        sched_ok = deal.build_schedule(
            self.pool_doc,
            self.constants_doc,
            namespace=ns_exact,
            run="r1",
            update_start=0,
            update_end=1,
        )
        for entry in sched_ok["iterations"][0]["episodes"]:
            self.assertLessEqual(len(entry["episode"]["id"].encode("utf-8")), 128)

        # 56 copies -> 74 Python characters (<= 128) but 130 UTF-8 bytes (> 128): must be rejected.
        ns_too_long = "\u00e9" * 56
        raw_candidate = f"{len(ns_too_long.encode('utf-8'))}:{ns_too_long}/2:r1/u0/j0/s0"
        self.assertLessEqual(len(raw_candidate), 128)
        self.assertGreater(len(raw_candidate.encode("utf-8")), 128)
        with self.assertRaisesRegex(ValueError, "UTF-8 bytes"):
            deal.format_episode_id(ns_too_long, "r1", 0, 0, 0)
        with self.assertRaisesRegex(ValueError, "UTF-8 bytes"):
            deal.build_schedule(
                self.pool_doc,
                self.constants_doc,
                namespace=ns_too_long,
                run="r1",
                update_start=0,
                update_end=1,
            )

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

    def test_learner_holds_first_and_second_drawn_decks_equally(self):
        pool_state = deal.validate_pool(self.pool_doc)
        constants = deal.validate_constants(self.constants_doc)
        update_start, update_end = 3, 8
        schedule = deal.build_schedule(
            self.pool_doc,
            self.constants_doc,
            namespace="train",
            run="balance",
            update_start=update_start,
            update_end=update_end,
        )
        held = {"first": 0, "second": 0}
        for offset, iteration in enumerate(schedule["iterations"]):
            update = update_start + offset
            episodes = iteration["episodes"]
            for p in range(constants["pairs_per_update"]):
                deck_a, deck_b = deal.deal_pair(pool_state, constants, "train", "balance", update, p)
                self.assertNotEqual(deck_a, deck_b)
                ep0 = episodes[2 * p]["episode"]
                ep1 = episodes[2 * p + 1]["episode"]
                learner0 = ep0["registered"][ep0["learner_seat"]]
                learner1 = ep1["registered"][ep1["learner_seat"]]
                # The learner holds one deck in both slots: deck_a in even pairs, deck_b in odd pairs.
                self.assertEqual(learner0, learner1)
                self.assertEqual(learner0, deck_a if p % 2 == 0 else deck_b)
                held["first" if learner0 == deck_a else "second"] += 1
                # The opponent holds the other deck in both slots.
                self.assertEqual(ep0["registered"][1 - ep0["learner_seat"]], deck_b if p % 2 == 0 else deck_a)
                self.assertEqual(ep1["registered"][1 - ep1["learner_seat"]], deck_b if p % 2 == 0 else deck_a)
                # Seat 0 starts: each deck is on the play exactly once per pair.
                self.assertEqual(ep0["starting_player"], 0)
                self.assertEqual(ep1["starting_player"], 0)
                on_play = [ep0["registered"][0], ep1["registered"][0]]
                self.assertIn(deck_a, on_play)
                self.assertIn(deck_b, on_play)
        pairs = (update_end - update_start) * constants["pairs_per_update"]
        self.assertEqual(held, {"first": pairs // 2, "second": pairs // 2})

    def test_refuses_odd_pairs_per_update(self):
        for odd in (1, 3, 59):
            with self.assertRaisesRegex(ValueError, "pairs_per_update must be even"):
                deal.validate_constants(dict(self.constants_doc, pairs_per_update=odd))
        with self.assertRaisesRegex(ValueError, "pairs_per_update must be even"):
            deal.build_schedule(
                self.pool_doc,
                dict(self.constants_doc, pairs_per_update=3),
                namespace="train",
                run="odd",
                update_start=0,
                update_end=1,
            )
        self.assertEqual(deal.validate_constants(dict(self.constants_doc, pairs_per_update=60))["pairs_per_update"], 60)

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
                                "opening": "london-v1",
                                "mulligan_modes": ["learned", "keep_seven"],
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
        base_snapshot = copy.deepcopy(base)
        stamped = deal.build_schedule(
            self.pool_doc,
            self.constants_doc,
            namespace="train",
            run="base-test",
            update_start=0,
            update_end=2,
            base_config=base,
        )
        self.assertEqual(base, base_snapshot)
        self.assertEqual(set(stamped.keys()), set(base.keys()))
        self.assertEqual(len(stamped["iterations"]), 2)
        for iteration in stamped["iterations"]:
            episodes = iteration["episodes"]
            for p in range(self.constants_doc["pairs_per_update"]):
                ep0 = episodes[2 * p]["episode"]
                ep1 = episodes[2 * p + 1]["episode"]
                self.assertEqual(ep0["max_physical_decisions"], 4000)
                self.assertEqual(ep0["max_policy_steps"], 40000)
                self.assertEqual(ep0["runtime_rules"], "resolution-boundary-v1")
                self.assertEqual(ep0["opening"], "london-v1")
                self.assertEqual(ep1["opening"], "london-v1")
                self.assertEqual(ep0["learner_seat"], 0)
                self.assertEqual(ep1["learner_seat"], 1)
                # Asymmetric mulligan_modes swap with the decks so learner_seat stays "learned".
                self.assertEqual(ep0["mulligan_modes"], ["learned", "keep_seven"])
                self.assertEqual(ep1["mulligan_modes"], ["keep_seven", "learned"])
                self.assertEqual(ep0["mulligan_modes"][ep0["learner_seat"]], "learned")
                self.assertEqual(ep1["mulligan_modes"][ep1["learner_seat"]], "learned")

        # Also verify stamping when the base template itself has learner_seat=1.
        base_seat1 = copy.deepcopy(base)
        base_seat1["iterations"][0]["episodes"][0]["episode"]["learner_seat"] = 1
        base_seat1["iterations"][0]["episodes"][0]["episode"]["mulligan_modes"] = ["keep_seven", "learned"]
        stamped_seat1 = deal.build_schedule(
            self.pool_doc,
            self.constants_doc,
            namespace="train",
            run="base-test-s1",
            update_start=0,
            update_end=1,
            base_config=base_seat1,
        )
        ep0_s1 = stamped_seat1["iterations"][0]["episodes"][0]["episode"]
        ep1_s1 = stamped_seat1["iterations"][0]["episodes"][1]["episode"]
        self.assertEqual(ep0_s1["mulligan_modes"], ["learned", "keep_seven"])
        self.assertEqual(ep1_s1["mulligan_modes"], ["keep_seven", "learned"])

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
