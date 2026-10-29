"""Schedule rules of the G(i) generator against the nine-deck baseline it changes, without native execution."""
import hashlib
import json
from collections import Counter
from pathlib import Path
import tempfile
import unittest

import g_references_v1 as g
import nine_deck_baseline_v1 as ndb

REPO = Path(__file__).resolve().parents[3]
DECKS = REPO / "data" / "runtime_decks_v1.json"
PLAY = {"path": "C:/x/G1/play-import.json", "sha256": "1" * 64}


def pin(name):
    return {"path": f"C:/x/{name}.json", "sha256": hashlib.sha256(name.encode()).hexdigest()}


def g_spec(run, block, temp):
    return {"run": run, "block": block, "runtime_decks": str(DECKS), "play_import": PLAY,
            "start_checkpoint": None if block == 1 else pin(f"{run}-b{block - 1}"),
            "history": {str(end): pin(f"{run}-b{end}") for end in g.history_pool(block)},
            "update_backend": {"kind": "cpu"}, "collection_workers": 8, "preparation_workers": 8,
            "output_directory": str(Path(temp) / "out")}


def baseline(block, temp):
    return ndb.block_config({"run": "r1", "block": block, "runtime_decks": str(DECKS),
                             "initial_source": {"checkpoint": pin("t1")}, "fixed_opponent": {"checkpoint": pin("a48")},
                             "update_backend": {"kind": "cpu"}, "collection_workers": 8, "preparation_workers": 8,
                             "output_directory": str(Path(temp) / "out")})


def scheduled(config):
    """Every scheduled episode without the namespace-derived fields (id, seed) and the opponent assignment."""
    return Counter(json.dumps({k: v for k, v in item["episode"].items() if k not in ("id", "seed")}, sort_keys=True)
                   for update in config["iterations"] for item in update["episodes"])


class HistoryPoolTests(unittest.TestCase):
    def test_permanent_ends(self):
        self.assertEqual(g.PERMANENT_BLOCKS[:8], (1, 2, 4, 8, 16, 32, 48, 64))
        self.assertIn(247, g.PERMANENT_BLOCKS)
        self.assertIn(494, g.PERMANENT_BLOCKS)
        self.assertIn(741, g.PERMANENT_BLOCKS)
        self.assertEqual(len(g.PERMANENT_BLOCKS), 4 + 46 + 3)

    def test_pools(self):
        self.assertEqual(g.history_pool(1), [])
        self.assertEqual(g.history_pool(2), [])
        self.assertEqual(g.history_pool(3), [1])
        self.assertEqual(g.history_pool(4), [1, 2])
        self.assertEqual(g.history_pool(6), [1, 2, 4])
        self.assertEqual(g.history_pool(249), g.history_pool(248) + [247])
        largest = g.history_pool(741)
        self.assertEqual(len(largest), 52)
        self.assertEqual(largest, sorted(largest))
        self.assertTrue(all(end <= 739 for end in largest))

    def test_history_hash_is_separate_from_the_deal(self):
        digest = hashlib.sha256(b'["g-references-v1/history","G2",9,4,7,8]').digest()
        self.assertEqual(g.history_draw("G2", 9, 4, 7, 8), int.from_bytes(digest[:8], "little"))
        train = hashlib.sha256(b'["g-references-v1/train","G2",9,4,7,8]').digest()
        self.assertEqual(g.unit_seed("G2", 9, 4, 7, 8), int.from_bytes(train[:8], "little") & (2**63 - 1))
        self.assertNotEqual(g.history_draw("G2", 9, 4, 7, 8) & (2**63 - 1), g.unit_seed("G2", 9, 4, 7, 8))
        order = hashlib.sha256(b'["g-references-v1/order","G2",9,4,7,8]').hexdigest()
        self.assertEqual(g.order_key("G2", 9, 4, 7, 8), order)

    def test_draw_is_index_u_mod_n(self):
        for block in (3, 4, 100, 741):
            pool = g.history_pool(block)
            for unit in ((0, 0, 8), (5, 3, 9), (8, 8, 9)):
                u = g.history_draw("G5", block, *unit)
                self.assertEqual(g.unit_opponent("G5", block, unit),
                                 {"kind": "fixed", "id": f"G5-history-b{pool[u % len(pool)]:03d}"})

    def test_base_seeds(self):
        digest = hashlib.sha256(b'["g-references-v1/init","G3"]').digest()
        self.assertEqual(g.base_seed("G3"), int.from_bytes(digest[:8], "little") & (2**63 - 1))
        self.assertEqual(len({g.base_seed(run) for run in g.RUNS}), 8)
        with self.assertRaises(ValueError):
            g.base_seed("r1")


class RecipeTests(unittest.TestCase):
    """Only the four fields differ from the baseline: start, the fixed slot, the namespaces (DESIGN section 3)
    and the runtime profile (AMENDMENT-1.md)."""

    def test_only_the_four_fields_differ(self):
        with tempfile.TemporaryDirectory() as temp:
            for block in (1, 2, 3, 17, 741):
                ours, theirs = block_config(g_spec("G1", block, temp)), baseline(min(block, ndb.BLOCKS), temp)
                self.assertEqual(sorted(ours), sorted(theirs))
                changed = ("initial_source", "opponents", "iterations")
                self.assertEqual({k: v for k, v in ours.items() if k not in changed},
                                 {k: v for k, v in theirs.items() if k not in changed})
                # Every one of the 1,620 episodes carries the profile; the baseline's carry none.
                episodes = [item["episode"] for u in ours["iterations"] for item in u["episodes"]]
                self.assertEqual(len(episodes), 1620)
                self.assertTrue(all(e["runtime_rules"] == "resolution-boundary-v1" for e in episodes))
                self.assertFalse(any("runtime_rules" in item["episode"]
                                     for u in theirs["iterations"] for item in u["episodes"]))
                self.assertEqual(g.require_runtime_rules(ours), 1620)
                # Without the profile (tests only), the scheduled episodes as a multiset without id and seed,
                # and the update sizes, are the baseline's.
                plain = block_config({**g_spec("G1", block, temp), "runtime_rules": None})
                self.assertEqual(scheduled(plain), scheduled(theirs))
                self.assertNotEqual(scheduled(ours), scheduled(theirs))
                with self.assertRaises(ValueError):
                    g.require_runtime_rules(plain)
                self.assertEqual([len(u["episodes"]) for u in ours["iterations"]],
                                 [len(u["episodes"]) for u in theirs["iterations"]])
                # Opponent kinds by j are the baseline's, except fixed (j 8, 9): own history, or initial when empty.
                for update in ours["iterations"]:
                    for item in update["episodes"]:
                        repeat = int(item["episode"]["id"].split("-j")[1].split("-")[0])
                        kind = ndb.opponent_kind(repeat)
                        if kind != "fixed":
                            self.assertEqual(item["opponent"], {"kind": kind})
                        elif block <= 2:
                            self.assertEqual(item["opponent"], {"kind": "initial"})
                        else:
                            self.assertEqual(item["opponent"]["kind"], "fixed")
                            self.assertRegex(item["opponent"]["id"], r"^G1-history-b\d{3}$")

    def test_refusal_catches_one_missing_episode_and_no_deck_has_delve(self):
        with tempfile.TemporaryDirectory() as temp:
            config = block_config(g_spec("G2", 9, temp))
        config["iterations"][80]["episodes"][3]["episode"].pop("runtime_rules")
        with self.assertRaisesRegex(ValueError, "1 of 1620"):
            g.require_runtime_rules(config)
        config["iterations"][80]["episodes"][3]["episode"]["runtime_rules"] = "other"
        with self.assertRaises(ValueError):
            g.require_runtime_rules(config)
        self.assertEqual(g.delve_cards(ndb.load_decks(DECKS), REPO / "data" / "cards_v1.json"), [])

    def test_frozen_fields_are_the_baselines(self):
        with tempfile.TemporaryDirectory() as temp:
            config = block_config(g_spec("G4", 5, temp))
        for key, value in ndb.FROZEN.items():
            self.assertEqual(config[key], value)
        self.assertEqual(config["schema"], ndb.SCHEMA)

    def test_fresh_start_and_chaining(self):
        with tempfile.TemporaryDirectory() as temp:
            first = block_config(g_spec("G2", 1, temp))
            second = block_config(g_spec("G2", 2, temp))
            bad = g_spec("G2", 2, temp)
            bad["start_checkpoint"] = None
            with self.assertRaises(ValueError):
                g.block_config(bad)
            bad = g_spec("G2", 1, temp)
            bad["start_checkpoint"] = pin("x")
            with self.assertRaises(ValueError):
                g.block_config(bad)
        self.assertEqual(first["initial_source"], {"play_import": PLAY, "feature_transfer": g.FEATURE_TRANSFER,
                                                   "checkpoint": None})
        self.assertEqual(second["initial_source"]["checkpoint"], pin("G2-b1"))
        self.assertEqual(second["initial_source"]["play_import"], PLAY)

    def test_empty_pool_fallback(self):
        with tempfile.TemporaryDirectory() as temp:
            for block in (1, 2):
                config = block_config(g_spec("G7", block, temp))
                self.assertEqual(config["opponents"], [])
                kinds = Counter(item["opponent"]["kind"] for u in config["iterations"] for item in u["episodes"])
                self.assertEqual(kinds, {"current": 486, "initial": 1134})
                self.assertEqual(g.block_summary("G7", block)["episodes_by_kind"], {"current": 486, "initial": 1134})

    def test_largest_block_builds(self):
        with tempfile.TemporaryDirectory() as temp:
            config = block_config(g_spec("G8", 741, temp))
            data = json.dumps(config, separators=(",", ":")).encode()
        self.assertEqual(len(config["opponents"]), 52)
        self.assertLessEqual(len(config["opponents"]), 128)  # the config's opponent bound
        self.assertLess(len(data), 16 * 1024 * 1024)  # the native config size bound
        ids = {o["id"] for o in config["opponents"]}
        used = {item["opponent"]["id"] for u in config["iterations"] for item in u["episodes"] if "id" in item["opponent"]}
        self.assertLessEqual(used, ids)
        for opponent in config["opponents"]:
            self.assertEqual(opponent["source"]["play_import"], PLAY)
            self.assertEqual(opponent["source"]["feature_transfer"], g.FEATURE_TRANSFER)
        # The engine's 32-distinct-opponent cap applies per preparation call of at most 10 episodes.
        self.assertTrue(all(len(u["episodes"]) == 10 for u in config["iterations"]))
        self.assertLessEqual(g.block_summary("G8", 741)["max_distinct_history_per_update"], 5)

    def test_history_must_be_exactly_the_pool(self):
        with tempfile.TemporaryDirectory() as temp:
            spec = g_spec("G1", 20, temp)
            spec["history"].pop("16")
            with self.assertRaises(ValueError):
                g.block_config(spec)
            spec = g_spec("G1", 20, temp)
            spec["history"]["19"] = pin("x")
            with self.assertRaises(ValueError):
                g.block_config(spec)

    def test_block_balance(self):
        summary = g.block_summary("G3", 300)
        self.assertEqual(summary["updates"], 162)
        self.assertEqual(summary["learner_episodes_by_deck"], [180] * 9)
        self.assertEqual(summary["opponent_episodes_by_deck"], [180] * 9)
        self.assertEqual(summary["episodes_by_kind"], {"current": 486, "fixed": 324, "initial": 810})
        self.assertEqual(sum(summary["history_episodes_by_id"].values()), 324)
        self.assertEqual(summary["episodes_per_ordered_pair"], [20])

    def test_rejects_unknown_run_and_block(self):
        for args in (("r1", 1), ("G1", 0), ("G1", 742), ("G9", 3)):
            with self.assertRaises(ValueError):
                g.block_units(*args)

    def test_ids_distinct_and_seeds_disjoint_from_the_baseline(self):
        with tempfile.TemporaryDirectory() as temp:
            config = block_config(g_spec("G6", 33, temp))
        ids = [item["episode"]["id"] for u in config["iterations"] for item in u["episodes"]]
        self.assertEqual(len(set(ids)), 1620)
        ours = {g.unit_seed(run, block, o, t, j) for run in g.RUNS for block in (1, 2, 741)
                for o in range(9) for t in range(9) for j in range(10)}
        theirs = {row["seed"] for row in ndb.all_seeds()["train"]} | {row["seed"] for row in ndb.all_seeds()["dev"]}
        self.assertEqual(len(ours), 8 * 3 * 810)
        self.assertFalse(ours & theirs)


class NamespaceTests(unittest.TestCase):
    def test_namespaces_are_new(self):
        import g_references_namespaces_v1 as namespaces
        result = namespaces.namespace_check()
        self.assertTrue(result["pass"], result["clashes"])
        self.assertIn("nine-deck-baseline-v1/train", result["registered"])
        self.assertEqual(len(result["namespaces"]), 5)

    def test_descriptors_and_distinct_starts(self):
        with tempfile.TemporaryDirectory() as temp:
            folders = {}
            for index, run in enumerate(list(g.RUNS) + ["A", "B"]):
                folder = Path(temp) / run
                folder.mkdir()
                lineage = run if run in g.RUNS else f"fresh-{run}"
                seed = g.base_seed(run) if run in g.RUNS else index
                (folder / "initialization.json").write_text(json.dumps({
                    "lineage_id": lineage, "initializer": {"base_seed": seed},
                    "payload": {"model_parameter_sha256": f"{index:064d}", "sha256": f"{index:064x}"}}))
                (folder / "parameters.f32le").write_bytes(bytes([index]) * 8)
                folders[run] = folder
            ours = {run: folders[run] for run in g.RUNS}
            others = {"A": folders["A"], "B": folders["B"]}
            self.assertTrue(g.distinct_starts(ours, others)["pass"])
            descriptor = g.fresh_descriptor(folders["G1"])
            self.assertEqual(descriptor["schema"], g.FRESH_SOURCE_SCHEMA)
            self.assertEqual(descriptor["parameters"]["sha256"], hashlib.sha256(bytes([0]) * 8).hexdigest())
            (folders["B"] / "initialization.json").write_text(json.dumps({
                "lineage_id": "fresh-B", "initializer": {"base_seed": 1},
                "payload": {"model_parameter_sha256": f"{3:064d}", "sha256": "x"}}))
            self.assertFalse(g.distinct_starts(ours, others)["pass"])  # B repeats G4's payload

    def test_init_request(self):
        target = {key: f"value-{key}" for key in g.INIT_TARGET_KEYS}
        request = g.init_request("G2", {**target, "registry_card_count": 192, "card_token_rule": "x"})
        self.assertEqual(request, {"schema": "mtg-kernel-fresh-initialization-request/v1", "lineage_id": "G2",
                                   "base_seed": g.base_seed("G2"), "target": target})


def block_config(spec):
    return g.block_config(spec)


if __name__ == "__main__":
    unittest.main()
