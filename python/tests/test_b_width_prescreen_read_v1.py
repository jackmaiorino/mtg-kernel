"""Tests of the B width pre-screen's single-read analyzer (screen note v1.2 sections 5, 6 and 8) on synthetic
driver records built with the note's real step rule, a real store layout, pinned parent files, and a known effect
and pairing."""

from __future__ import annotations

import contextlib
import copy
import hashlib
import io
import json
import math
import random
import shutil
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

REPO_ROOT = Path(__file__).resolve().parents[2]
TOOLS = REPO_ROOT / "python" / "tools"
if str(TOOLS) not in sys.path:
    sys.path.insert(0, str(TOOLS))

import b_width_prescreen_read_v1 as reader  # noqa: E402

SHIPPED_CONFIG = REPO_ROOT / "data" / "b_width_prescreen_v1" / "config.json"
ANALYZER_SHA256 = hashlib.sha256((TOOLS / "b_width_prescreen_read_v1.py").read_bytes()).hexdigest()
NOW = 1_800_000_000
ENGINE = "e" * 40
BINARY = "1" * 64
DECKS = 3


def sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def write(path: Path, value: object) -> bytes:
    data = (json.dumps(value, indent=1) + "\n").encode("utf-8")
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(data)
    return data


def load(path: Path) -> dict:
    return json.loads(path.read_bytes())


class Tree:
    """A synthetic run root: 8 chains, 3 decks (9 ordered pairs), 5 games per pair, 4 fitting seeds, the shipped
    config's fitting constants (so the real step rule), a store whose games begin with their group counts, and
    pinned parent and import files.

    Each retained holdout game's final w64 error is ``base(game)``; w128's is
    ``base * (1 - chain_effect - pair_effect) * jitter`` and w32's ``base * w32_factor``; ``override`` can replace
    any value. Step 0 is shared by w64 and w128.
    """

    def __init__(self, root: Path, *, effect: float = 0.1, chain_effects: dict[int, float] | None = None,
                 pair_effects: dict[tuple[int, int], float] | None = None, w32_factor: float = 1.25,
                 excluded: tuple[tuple[int, str], ...] = (), draws: int = 400, noise: bool = False,
                 uniform_base: bool = False, override=None, analysis: dict | None = None) -> None:
        self.root = root
        config = json.loads(SHIPPED_CONFIG.read_bytes())
        config["decks"] = config["decks"][:DECKS]
        config["analysis"]["bootstrap_draws"] = draws
        config["analysis"]["interval_lower_index"] = draws * 25 // 1000
        config["analysis"]["interval_upper_index"] = draws - 1 - draws * 25 // 1000
        config["analysis"].update(analysis or {})
        config["analyzer"]["sha256"] = ANALYZER_SHA256
        self.config = config
        self.effect = {k: (chain_effects or {}).get(k, effect) for k in range(1, 9)}
        self.pair_effects = pair_effects or {}
        self.w32_factor = w32_factor
        self.excluded = set(excluded)
        self.noise = noise
        self.uniform_base = uniform_base
        self.override = override
        self.errors: dict[tuple[int, int, str], dict[str, float]] = {}
        self.pairs: dict[int, dict[tuple[int, int], list[str]]] = {}

    def clone(self, root: Path) -> "Tree":
        shutil.copytree(self.root, root)
        other = copy.copy(self)
        other.root = root
        return other

    def path(self, relative: str) -> Path:
        return self.root / relative

    def edit(self, relative: str, change) -> None:
        value = load(self.path(relative))
        change(value)
        write(self.path(relative), value)

    def build(self) -> str:
        """Writes the tree; returns SECTION7.json's SHA-256."""
        config = self.config
        rng = random.Random(20261027)
        parents = self.root / "parents"
        parents.mkdir(parents=True, exist_ok=True)
        init_bytes = b"synthetic initialization parameters"
        (parents / "init.f32le").write_bytes(init_bytes)
        import_bytes = write(parents / "import.json", {"schema": "mtg-kernel-fresh-initialization-source/v1",
                                                       "parameters": {"path": str(parents / "init.f32le"),
                                                                      "sha256": sha(init_bytes)}})
        for chain in config["chains"]:
            k = chain["index"]
            checkpoint = f"synthetic b512 checkpoint {k}".encode()
            (parents / f"r2-s{k}-b512.json").write_bytes(checkpoint)
            chain["source"]["checkpoint"] = {"path": str(parents / f"r2-s{k}-b512.json"), "sha256": sha(checkpoint)}
            chain["source"]["play_import"] = {"path": str(parents / "import.json"), "sha256": sha(import_bytes)}
        config_bytes = write(self.root / "config.json", config)
        config_sha = sha(config_bytes)
        launch_bytes = write(self.root / "LAUNCH.json", {
            "schema": reader.LAUNCH_SCHEMA, "config_sha256": config_sha, "launched_at_unix": NOW - 3600,
            "host": "SYNTHETIC", "binary_sha256": BINARY, "engine_commit": ENGINE})
        fitting = config["fitting"]
        section7_chains = []
        for chain in config["chains"]:
            k = chain["index"]
            store = bytearray(reader.STORE_MAGIC)
            games = []
            attempted = {"fit": [], "holdout": []}
            natural = {"fit": [], "holdout": []}
            excluded, holdout_games, groups_of = [], [], {}
            pairs = {}
            for game_id, split, focal, opposing in reader.expected_schedule(config, k):
                pairs.setdefault((focal, opposing), [])
                attempted[split].append(game_id)
                index = int(game_id.rsplit("-i", 1)[1])
                plan = {"id": game_id, "chain": k, "focal": focal, "opposing": opposing, "index": index,
                        "split": split, "seed": reader.domain_seed("b-width-prescreen-collect-v1", k, focal,
                                                                   opposing, index),
                        "focal_seat": 0, "starting_player": (index + focal + opposing) % 2}
                if (k, game_id) in self.excluded:
                    excluded.append({"id": game_id, "split": split, "focal": focal, "opposing": opposing,
                                     "terminal_classification": "truncated",
                                     "terminal_reason": "physical_decision_cap_reached:100000"})
                    games.append({"plan": plan, "natural": False, "terminal": {"terminal_classification":
                                                                               "truncated"},
                                  "failure": None, "groups": 0, "substeps": 0, "offset": len(store), "length": 0,
                                  "sha256": None})
                    continue
                groups = 10 + rng.randrange(40)
                body = groups.to_bytes(4, "little") + rng.randbytes(12)
                games.append({"plan": plan, "natural": True, "terminal": {"terminal_classification": "natural"},
                              "failure": None, "groups": groups, "substeps": groups, "offset": len(store),
                              "length": len(body), "sha256": sha(body)})
                store += body
                natural[split].append(game_id)
                groups_of[game_id] = groups
                if split == "holdout":
                    holdout_games.append({"id": game_id, "focal": focal, "opposing": opposing, "index": index,
                                          "groups": groups})
                    pairs[(focal, opposing)].append(game_id)
            self.pairs[k] = pairs
            store = bytes(store)
            (self.root / "collect" / f"k{k}").mkdir(parents=True, exist_ok=True)
            (self.root / "collect" / f"k{k}" / "games.bin").write_bytes(store)
            parent_state = sha(f"parent-{k}".encode())
            collection = {"schema": reader.COLLECTION_SCHEMA, "config_sha256": config_sha, "engine_commit": ENGINE,
                          "binary_sha256": BINARY, "chain": k, "source": chain["source"],
                          "parent_state_sha256": parent_state,
                          "feature_contract_digest": config["feature_contract_digest"],
                          "feature_encoding_digest": config["feature_encoding_digest"],
                          "store_sha256": sha(store), "store_bytes": len(store), "games": games}
            collection_bytes = write(self.root / "collect" / f"k{k}" / "collection.json", collection)
            fit_groups = sum(groups_of[g] for g in natural["fit"])
            holdout_groups = sum(groups_of[g] for g in natural["holdout"])
            steps, every = reader.step_rule(fitting, fit_groups)
            points = reader.evaluation_steps(steps, every)
            record = {"schema": reader.DATA_SCHEMA, "config_sha256": config_sha, "chain": k,
                      "chain_id": chain["id"], "parent_checkpoint_sha256": chain["source"]["checkpoint"]["sha256"],
                      "parent_state_sha256": parent_state, "collection_record_sha256": sha(collection_bytes),
                      "store_sha256": sha(store), "attempted": attempted, "natural": natural,
                      "excluded": excluded, "fit_groups": fit_groups, "holdout_groups": holdout_groups,
                      "steps": steps, "heldout_every_steps": every,
                      "outcome_source_sha256": sha(f"outcome-{k}".encode()),
                      "decision_source_sha256": sha(f"decision-{k}".encode()), "holdout_games": holdout_games}
            data_bytes = write(self.root / "data" / f"k{k}.json", record)
            section7_chains.append({
                "chain": k, "chain_id": chain["id"], "parent_checkpoint_sha256": record["parent_checkpoint_sha256"],
                "collection_record_sha256": sha(collection_bytes), "store_sha256": sha(store),
                "data_record_sha256": sha(data_bytes), "attempted": attempted, "natural": natural,
                "excluded": excluded, "outcome_source_sha256": record["outcome_source_sha256"],
                "decision_source_sha256": record["decision_source_sha256"], "fit_groups": fit_groups,
                "steps": steps})
            base = {game: 1.0 if self.uniform_base else 0.2 + rng.random() for game in natural["holdout"]}
            pair_of = {game: pair for pair, members in pairs.items() for game in members}
            preservation_seeds = []
            for s in fitting["fitting_seeds"]:
                minibatch_seed = reader.domain_seed(fitting["minibatch_seed_domain"], k, s)
                widen_seed = reader.domain_seed(fitting["widen_seed_domain"], k, s)
                child = sha(f"widened-{k}-{s}".encode())
                preservation_seeds.append({"seed": s, "widen_seed": widen_seed, "child_state_sha256": child,
                                           "inputs_compared": fit_groups + holdout_groups + 7, "mismatches": 0,
                                           "first_mismatch": None})
                for arm in reader.ARMS:
                    final = {}
                    for game in base:
                        jitter = 1.0 + (0.05 * rng.random() if self.noise and arm != "w64" else 0.0)
                        factor = {"w64": 1.0,
                                  "w128": 1.0 - self.effect[k] - self.pair_effects.get(pair_of[game], 0.0),
                                  "w32": self.w32_factor}[arm]
                        value = base[game] * factor * jitter
                        if self.override:
                            value = self.override(k, s, arm, game, value)
                        final[game] = value
                    self.errors[(k, s, arm)] = final
                    start = {game: 1.5 * base[game] * (1.1 if arm == "w32" else 1.0) for game in base}

                    def evaluation(after_step: int, values: dict[str, float]) -> dict:
                        rows = [{"game_id": game, "groups": groups_of[game],
                                 "value_squared_error_sum": values[game] * groups_of[game],
                                 "policy_nll_sum": 0.5 * groups_of[game]} for game in natural["holdout"]]
                        total = sum(row["value_squared_error_sum"] for row in rows)
                        return {"after_step": after_step, "value_mse": total / holdout_groups,
                                "policy_nll_mean": 0.5, "games": rows}

                    heldout = []
                    for point in points:
                        weight = point / steps
                        values = {g: start[g] + (final[g] - start[g]) * weight for g in base}
                        heldout.append(evaluation(point, start if point == 0 else
                                                  final if point == steps else values))
                    transformed = {"w64": parent_state, "w128": child, "w32": sha(f"w32-{k}".encode())}[arm]
                    transform = {"construction_version": reader.CONSTRUCTIONS[arm],
                                 "parent_state_sha256": parent_state, "child_state_sha256": transformed}
                    if arm == "w128":
                        transform["seed"] = widen_seed
                    fresh = sha(f"fresh-{k}-{s}-{arm}".encode()) if arm == "w128" else sha(f"fresh-{k}-{arm}".encode())
                    report = {
                        "entry_version": "native-screen-value-fit-v1", "architecture": reader.ARCHITECTURES[arm],
                        "input_generation": "v4", "value_target_source": "terminal_return_v1",
                        "policy_term": "off-every-advantage-positive-zero-v1",
                        "config": {"value_target_source": "terminal_return_v1", "steps": steps,
                                   "batch_groups": fitting["batch_groups"],
                                   "learning_rate": fitting["learning_rate"],
                                   "value_coefficient": fitting["value_coefficient"],
                                   "minibatch_seed": minibatch_seed, "heldout_every_steps": every,
                                   "forward_workers": 1},
                        "learning_rate_bits": reader.f32_bits(fitting["learning_rate"]),
                        "value_coefficient_bits": reader.f32_bits(fitting["value_coefficient"]),
                        "initial_state_sha256": fresh, "final_state_sha256": sha(f"final-{k}-{s}-{arm}".encode()),
                        "initial_adam_step": 0, "final_adam_step": steps,
                        "fit_game_ids": natural["fit"], "heldout_game_ids": natural["holdout"],
                        "fit_group_count": fit_groups, "heldout_group_count": holdout_groups,
                        "outcome_source_sha256": record["outcome_source_sha256"],
                        "decision_source_sha256": record["decision_source_sha256"],
                        "minibatch_order_sha256": sha(f"order-{k}-{s}".encode()),
                        "steps": [{"step": n, "loss": 0.3, "value_sum": 1.0, "policy_sum": 0.0, "adam_step": n}
                                  for n in range(1, steps + 1)],
                        "heldout": heldout,
                    }
                    if arm == "w128":
                        report["heldout"][0] = evaluation(0, {g: 1.5 * base[g] for g in base})
                    fit = {"schema": reader.FIT_SCHEMA, "config_sha256": config_sha,
                           "data_record_sha256": sha(data_bytes), "engine_commit": ENGINE,
                           "binary_sha256": BINARY, "chain": k, "seed": s,
                           "arm": arm, "minibatch_seed": minibatch_seed,
                           "widen_seed": widen_seed if arm == "w128" else None,
                           "parent_state_sha256": parent_state, "transform": transform,
                           "transformed_state_sha256": transformed, "fresh_adam_state_sha256": fresh,
                           "report": report}
                    write(self.root / "fits" / f"k{k}-s{s}-{arm}.json", fit)
            write(self.root / "preserve" / f"k{k}.json",
                  {"schema": reader.PRESERVATION_SCHEMA, "config_sha256": config_sha, "engine_commit": ENGINE,
                   "binary_sha256": BINARY, "chain": k, "data_record_sha256": sha(data_bytes), "parent_state_sha256": parent_state,
                   "inputs": fit_groups + holdout_groups + 7, "seeds": preservation_seeds, "pass": True})
        section7 = {"schema": reader.SECTION7_SCHEMA, "note": config["note"], "config_sha256": config_sha,
                    "analyzer": config["analyzer"], "engine_commit": ENGINE, "binary_sha256": BINARY,
                    "launch": {"sha256": sha(launch_bytes), "launched_at_unix": NOW - 3600},
                    "chains": section7_chains}
        return sha(write(self.root / "data" / "SECTION7.json", section7))

    def resign(self, sync_fields: bool = True) -> str:
        """After an edit, recomputes every hash link (collection, data, section 7 and the records that bind the
        data record), so only the edited rule can fail. Returns SECTION7.json's new SHA-256."""
        section7 = load(self.path("data/SECTION7.json"))
        for entry in section7["chains"]:
            k = entry["chain"]
            collection_sha = sha(self.path(f"collect/k{k}/collection.json").read_bytes())
            self.edit(f"data/k{k}.json", lambda v: v.__setitem__("collection_record_sha256", collection_sha))
            data_bytes = self.path(f"data/k{k}.json").read_bytes()
            entry["collection_record_sha256"] = collection_sha
            entry["data_record_sha256"] = sha(data_bytes)
            if sync_fields:
                record = json.loads(data_bytes)
                for field in ("attempted", "natural", "excluded", "outcome_source_sha256",
                              "decision_source_sha256", "fit_groups", "steps", "store_sha256"):
                    entry[field] = record[field]
            for path in list(self.path("fits").glob(f"k{k}-s*.json")) + [self.path(f"preserve/k{k}.json")]:
                if path.exists():
                    value = load(path)
                    value["data_record_sha256"] = sha(data_bytes)
                    write(path, value)
        return sha(write(self.path("data/SECTION7.json"), section7))


def run(tree: Tree, section7: str, *extra: str) -> tuple[int, dict | None, str]:
    stdout, stderr = io.StringIO(), io.StringIO()
    with contextlib.redirect_stdout(stdout), contextlib.redirect_stderr(stderr):
        status = reader.main(["--config", str(tree.root / "config.json"), "--root", str(tree.root),
                              "--section7-sha256", section7, *extra])
    path = tree.root / reader.READ_NAME
    read = json.loads(path.read_bytes()) if path.exists() else None
    return status, read, stderr.getvalue()


def pair_weighted(tree: Tree) -> tuple[float, dict[int, float], dict[str, float]]:
    """An independent direct computation of section 5: per fit, ordered pairs weigh equally and games weigh
    equally within a pair."""
    seeds = tree.config["fitting"]["fitting_seeds"]
    loss = {}
    for (k, s, arm), errors in tree.errors.items():
        pair_means = [sum(errors[g] for g in games) / len(games) for games in tree.pairs[k].values()]
        loss[(k, s, arm)] = sum(pair_means) / len(pair_means)
    chains = range(1, 9)
    pooled = {arm: sum(loss[(k, s, arm)] for k in chains for s in seeds) / (8 * len(seeds)) for arm in reader.ARMS}
    per_chain = {k: 1 - sum(loss[(k, s, "w128")] for s in seeds) / sum(loss[(k, s, "w64")] for s in seeds)
                 for k in chains}
    return 1 - pooled["w128"] / pooled["w64"], per_chain, pooled


def game_weighted_r(tree: Tree) -> float:
    """The plain mean over games, which the note does not use (the swap the tests must catch)."""
    pooled = {}
    for arm in ("w64", "w128"):
        values = [value for (k, s, a), errors in tree.errors.items() if a == arm for value in errors.values()]
        pooled[arm] = sum(values) / len(values)
    return 1 - pooled["w128"] / pooled["w64"]


def manual_bootstrap(tree: Tree, draws: int) -> list[float]:
    """The bootstrap reimplemented from the note's text: per draw, 8 chain occurrences; then per occurrence, in
    order, its 4 seed draws and then, per ordered pair in deck order, its game draws; pairs weigh equally."""
    analysis = tree.config["analysis"]
    stream = reader.SplitMix64(reader.domain_seed(analysis["bootstrap_seed_domain"], 0))
    seeds = tree.config["fitting"]["fitting_seeds"]
    pairs = [(f, o) for f in range(DECKS) for o in range(DECKS)]
    values = []
    for _ in range(draws):
        occurrences = [1 + stream.index(8) for _ in range(8)]
        sums = {"w64": 0.0, "w128": 0.0}
        for k in occurrences:
            drawn_seeds = [seeds[stream.index(4)] for _ in range(4)]
            drawn_games = {}
            for pair in pairs:
                games = tree.pairs[k][pair]
                drawn_games[pair] = [games[stream.index(len(games))] for _ in games]
            for s in drawn_seeds:
                for arm in sums:
                    errors = tree.errors[(k, s, arm)]
                    sums[arm] += sum(sum(errors[g] for g in games) / len(games)
                                     for games in drawn_games.values()) / len(pairs)
        values.append(1 - sums["w128"] / sums["w64"])
    return values


class AnalyzerTests(unittest.TestCase):
    template: Tree
    template_section7: str

    @classmethod
    def setUpClass(cls) -> None:
        cls.saved_time = reader.current_time
        reader.current_time = lambda: NOW
        cls.class_temporary = tempfile.TemporaryDirectory()
        cls.template = Tree(Path(cls.class_temporary.name) / "template")
        cls.template_section7 = cls.template.build()

    @classmethod
    def tearDownClass(cls) -> None:
        reader.current_time = cls.saved_time
        cls.class_temporary.cleanup()

    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory()
        self.root = Path(self.temporary.name)

    def tearDown(self) -> None:
        self.temporary.cleanup()

    def fresh(self, name: str, **options) -> tuple[Tree, str]:
        tree = Tree(self.root / name, **options)
        return tree, tree.build()

    def copy(self, name: str) -> Tree:
        return self.template.clone(self.root / name)

    def void_read(self, tree: Tree, section7: str) -> dict:
        status, read, error = run(tree, section7)
        self.assertEqual(status, 0, error)
        self.assertEqual(read["verdict"], "VOID", read.get("void_detail"))
        self.assertNotIn("R", read)
        return read

    def test_generator_seeds_and_step_rule(self) -> None:
        self.assertEqual(reader.SplitMix64(0).next_u64(), 0xE220A8397B1DCDAF)
        self.assertEqual(reader.domain_seed("b-width-prescreen-bootstrap-v1", 0), 9237536898453982863)
        self.assertEqual(reader.domain_seed("b-width-prescreen-minibatch-v1", 1, 1), 15313765789897877101)
        stream = reader.SplitMix64(7)
        self.assertTrue(all(0 <= stream.index(5) < 5 for _ in range(1000)))
        self.assertEqual(reader.f32_bits(0.0001), 953267991)
        fitting = json.loads(SHIPPED_CONFIG.read_bytes())["fitting"]
        # Section 4 at the note's estimate of 30,000 fit groups: about 940 steps, every 117.
        self.assertEqual(reader.step_rule(fitting, 30_000), (938, 117))
        self.assertEqual(reader.step_rule(fitting, 256), (8, 1))
        self.assertEqual(reader.evaluation_steps(26, 3), [0, 3, 6, 9, 12, 15, 18, 21, 24, 26])
        self.assertEqual(reader.evaluation_steps(24, 3), [0, 3, 6, 9, 12, 15, 18, 21, 24])

    def test_shipped_config_pins_this_analyzer(self) -> None:
        config = json.loads(SHIPPED_CONFIG.read_bytes())
        reader.check_config(config)
        self.assertEqual(config["analyzer"]["sha256"], ANALYZER_SHA256)
        self.assertEqual(config["analyzer"]["path"], "python/tools/b_width_prescreen_read_v1.py")
        self.assertEqual(config["analysis"]["wall_cap_hours"], 48)

    def test_a_known_effect_is_recovered_and_funds(self) -> None:
        tree = self.copy("known")
        status, read, error = run(tree, self.template_section7)
        self.assertEqual(status, 0, error)
        self.assertEqual(read["verdict"], "FUND_B", read.get("void_detail"))
        self.assertAlmostEqual(read["R"], 0.1, places=12)
        self.assertAlmostEqual(read["interval"][0], 0.1, places=12)
        self.assertAlmostEqual(read["interval"][1], 0.1, places=12)
        self.assertGreater(read["L"]["w32"], read["L"]["w64"])
        self.assertEqual([check["check"] for check in read["checks"]],
                         ["data", "wall_cap", "attrition", "fits", "pairing", "function_preservation",
                          "numerical_validity", "sensitivity_control", "numerical_validity"])
        self.assertAlmostEqual(read["wall_cap"]["elapsed_hours"], 1.0)

    def test_pair_weighting_with_unequal_pairs_matches_a_direct_computation(self) -> None:
        # Unequal pair sizes from exclusions (pair (0, 1) keeps one holdout game in every chain, pair (2, 2) one
        # in chain 3), chain effects, per-pair effects and noise.
        excluded = tuple((k, f"bwps-k{k}-f0-o1-i4") for k in range(1, 9)) + ((3, "bwps-k3-f2-o2-i3"),)
        tree, section7 = self.fresh(
            "weighting", chain_effects={k: 0.01 * k for k in range(1, 9)},
            pair_effects={(0, 1): 0.30, (2, 2): -0.10, (1, 0): 0.05}, noise=True, excluded=excluded, draws=2,
            analysis={"interval_lower_index": 0, "interval_upper_index": 1,
                      "max_pooled_nonnatural_holdout_fraction": 0.1})
        status, read, error = run(tree, section7)
        self.assertEqual(status, 0, error)
        expected, per_chain, pooled = pair_weighted(tree)
        self.assertAlmostEqual(read["R"], expected, places=12)
        for arm in reader.ARMS:
            self.assertAlmostEqual(read["L"][arm], pooled[arm], places=12)
        for k in range(1, 9):
            self.assertAlmostEqual(read["descriptive"]["per_chain_R"][str(k)], per_chain[k], places=12)
        draws = sorted(manual_bootstrap(tree, 2))
        self.assertAlmostEqual(read["interval"][0], draws[0], places=12)
        self.assertAlmostEqual(read["interval"][1], draws[1], places=12)
        # A plain mean over games gives a different answer here, so the swap cannot pass this test.
        self.assertGreater(abs(game_weighted_r(tree) - expected), 1e-3)

    def test_explicit_losses_with_unequal_pairs(self) -> None:
        # Every w64 error is 1. In every chain pair (0, 0) keeps one holdout game, with effect 0.5; the other 8
        # pairs keep two, with effect 0. Pair-weighted: L(w128) = 1 - 0.5/9 and R = 1/18. A plain mean over the 17
        # games would give L(w128) = 16.5/17 and R = 1/34.
        excluded = tuple((k, f"bwps-k{k}-f0-o0-i4") for k in range(1, 9))
        tree, section7 = self.fresh("explicit", effect=0.0, pair_effects={(0, 0): 0.5}, uniform_base=True,
                                    excluded=excluded, analysis={"max_pooled_nonnatural_holdout_fraction": 0.1})
        status, read, error = run(tree, section7)
        self.assertEqual(status, 0, error)
        self.assertAlmostEqual(read["L"]["w64"], 1.0, places=12)
        self.assertAlmostEqual(read["L"]["w128"], 1.0 - 0.5 / 9, places=12)
        self.assertAlmostEqual(read["R"], 1.0 / 18, places=12)
        self.assertAlmostEqual(game_weighted_r(tree), 1.0 / 34, places=12)

    def test_decision_boundaries(self) -> None:
        config = json.loads(SHIPPED_CONFIG.read_bytes())
        cases = [
            (0.02, [0.001, 0.05], "FUND_B"),        # R exactly the minimum funds
            (0.05, [0.0, 0.09], "NOT_FUNDED"),      # a lower bound of exactly 0 does not fund
            (0.05, [-0.01, 0.09], "NOT_FUNDED"),    # a lower bound below 0 does not fund
            (0.0199999, [0.001, 0.05], "NOT_FUNDED"),
            (0.03, [0.001, 0.05], "FUND_B"),
        ]
        for r, interval, verdict in cases:
            with self.subTest(r=r, interval=interval):
                self.assertEqual(reader.verdict_of(config, {"R": r, "interval": interval})[0], verdict)
        for effect, lower_positive in ((0.01, True), (0.0, False)):
            with self.subTest(effect=effect):
                tree, section7 = self.fresh(f"e{effect}", effect=effect)
                status, read, _ = run(tree, section7)
                self.assertEqual((status, read["verdict"]), (0, "NOT_FUNDED"))
                self.assertIn("not funded now", read["statement"])
                self.assertEqual(read["interval"][0] > 0, lower_positive)

    def test_void_rules_fire_one_by_one(self) -> None:
        def store_flip(t: Tree) -> str:
            path = t.path("collect/k2/games.bin")
            data = bytearray(path.read_bytes())
            data[-1] ^= 1
            path.write_bytes(bytes(data))
            return t.resign()

        def store_resigned(t: Tree) -> str:
            # The store's hash links are consistent, but one game's bytes differ from its entry.
            path = t.path("collect/k2/games.bin")
            data = bytearray(path.read_bytes())
            data[-1] ^= 1
            path.write_bytes(bytes(data))
            store_sha = sha(bytes(data))
            t.edit("collect/k2/collection.json", lambda v: v.__setitem__("store_sha256", store_sha))
            t.edit("data/k2.json", lambda v: v.__setitem__("store_sha256", store_sha))
            return t.resign()

        def group_count(t: Tree) -> str:
            t.edit("collect/k3/collection.json", lambda v: v["games"][0].__setitem__("groups", 999))
            return t.resign()

        def data_field(field, value):
            def damage(t: Tree) -> str:
                t.edit("data/k4.json", lambda v: v.__setitem__(field, value(v)))
                return t.resign()
            return damage

        def section7_field(t: Tree) -> str:
            t.resign()
            t.edit("data/SECTION7.json", lambda v: v["chains"][0].__setitem__("fit_groups", 1))
            return sha(t.path("data/SECTION7.json").read_bytes())

        def collection_field(change):
            def damage(t: Tree) -> str:
                t.edit("collect/k1/collection.json", change)
                return t.resign()
            return damage

        def fit_edit(name, change):
            def damage(t: Tree) -> str:
                t.edit(f"fits/{name}", change)
                return t.resign()
            return damage

        def report_edit(name, change):
            return fit_edit(name, lambda v: change(v["report"]))

        def preserve_edit(change):
            def damage(t: Tree) -> str:
                t.edit("preserve/k6.json", change)
                return t.resign()
            return damage

        def launch_late(t: Tree) -> str:
            t.edit("LAUNCH.json", lambda v: v.__setitem__("launched_at_unix", NOW - 48 * 3600 - 1))
            launch = sha(t.path("LAUNCH.json").read_bytes())
            t.edit("data/SECTION7.json", lambda v: v.__setitem__(
                "launch", {"sha256": launch, "launched_at_unix": NOW - 48 * 3600 - 1}))
            return t.resign()

        def remove_held_out_row(v):
            v["heldout"][-1]["games"].pop()

        def duplicate_held_out_row(v):
            v["heldout"][-1]["games"][-1] = v["heldout"][-1]["games"][0]

        def drop_final(v):
            v["heldout"].pop()

        def w64_child(v):
            v["transformed_state_sha256"] = "0" * 64
            v["transform"]["child_state_sha256"] = "0" * 64

        def widen_seed(v):
            v["widen_seed"] = 1
            v["transform"]["seed"] = 1

        def w64_fresh(v):
            v["fresh_adam_state_sha256"] = "0" * 64
            v["report"]["initial_state_sha256"] = "0" * 64

        def step0_differs(v):
            row = v["report"]["heldout"][0]["games"][0]
            row["value_squared_error_sum"] = math.nextafter(row["value_squared_error_sum"], 1e9)

        def w64_step0_across_seeds(v):
            v["report"]["heldout"][0]["policy_nll_mean"] = 0.25

        def fit_inputs_differ(t: Tree) -> str:
            for path in t.path("fits").glob("k5-s*.json"):
                value = load(path)
                value["report"]["outcome_source_sha256"] = "0" * 64
                write(path, value)
            return t.resign()

        def preservation_inputs_short(t: Tree) -> str:
            data = load(t.path("data/k6.json"))
            short = data["fit_groups"] + data["holdout_groups"] - 1
            t.edit("preserve/k6.json", lambda v: (v.__setitem__("inputs", short),
                                                  [row.__setitem__("inputs_compared", short)
                                                   for row in v["seeds"]]))
            return t.resign()

        def null_error(v):
            v["heldout"][-1]["games"][0]["value_squared_error_sum"] = None

        def zero_w64_chain(t: Tree) -> str:
            for s in (1, 2, 3, 4):
                for arm in ("w64", "w128", "w32"):
                    t.edit(f"fits/k2-s{s}-{arm}.json", lambda v, arm=arm: [
                        row.__setitem__("value_squared_error_sum", 0.0 if arm == "w64" else row[
                            "value_squared_error_sum"]) for row in v["report"]["heldout"][-1]["games"]])
            return t.resign()

        def zero_w64_everywhere(t: Tree) -> str:
            for path in t.path("fits").glob("*-w64.json"):
                value = load(path)
                for row in value["report"]["heldout"][-1]["games"]:
                    row["value_squared_error_sum"] = 0.0
                write(path, value)
            return t.resign()

        def extra_file(t: Tree) -> str:
            (t.path("fits") / "k3-s2-w32 (copy).json").write_bytes(t.path("fits/k3-s2-w32.json").read_bytes())
            return t.resign()

        def refusal_file(t: Tree) -> str:
            write(t.path("fits/k3-s2-w32.refused.json"), {"error": "BatchLargerThanFitSet"})
            return t.resign()

        def refusal_temporary(t: Tree) -> str:
            # A refusal whose publication was interrupted (truncated temporary file) is still a refusal.
            (t.path("fits") / ".k3-s2-w32.refused.json.tmp-9").write_bytes(b"{")
            return t.resign()

        def launch_binary(t: Tree) -> str:
            t.edit("LAUNCH.json", lambda v: v.__setitem__("binary_sha256", "0" * 64))
            launch = sha(t.path("LAUNCH.json").read_bytes())
            t.edit("data/SECTION7.json", lambda v: v["launch"].__setitem__("sha256", launch))
            return t.resign()

        def missing(relative):
            def damage(t: Tree) -> str:
                t.path(relative).unlink()
                return t.resign()
            return damage

        cases = {
            # Data.
            "parent hash recorded differently": ("data", collection_field(
                lambda v: v["source"]["checkpoint"].__setitem__("sha256", "0" * 64))),
            "feature digest": ("data", collection_field(lambda v: v.__setitem__("feature_contract_digest", "x"))),
            "collection engine commit": ("data", collection_field(lambda v: v.__setitem__("engine_commit", "f"))),
            "collection binary": ("data", collection_field(lambda v: v.__setitem__("binary_sha256", "0" * 64))),
            "launch build differs from section 7": ("data", launch_binary),
            "collection record hash": ("data", lambda t: (t.edit(
                "collect/k1/collection.json", lambda v: v.__setitem__("store_bytes", 0)),
                sha(t.path("data/SECTION7.json").read_bytes()))[1]),
            "data record against section 7": ("data", section7_field),
            "store bytes": ("data", store_flip),
            "stored game bytes": ("data", store_resigned),
            "store group count": ("data", group_count),
            "attempted ids": ("data", data_field("attempted", lambda v: {"fit": v["attempted"]["fit"][1:],
                                                                         "holdout": v["attempted"]["holdout"]})),
            "natural plus excluded": ("data", data_field("natural", lambda v: {
                "fit": v["natural"]["fit"], "holdout": v["natural"]["holdout"][1:]})),
            "step rule steps": ("data", data_field("steps", lambda v: v["steps"] + 1)),
            "step rule cadence": ("data", data_field("heldout_every_steps", lambda v: v["heldout_every_steps"] + 1)),
            "fit group total": ("data", data_field("fit_groups", lambda v: v["fit_groups"] + 1)),
            # The 48-hour cap.
            "wall cap": ("wall_cap", launch_late),
            "launch record": ("wall_cap", lambda t: (t.edit("LAUNCH.json", lambda v: v.__setitem__("host", "x")),
                                                     t.resign())[1]),
            # Fits.
            "missing report": ("fits", missing("fits/k3-s2-w32.json")),
            "duplicate report": ("fits", extra_file),
            "entry refusal": ("fits", refusal_file),
            "interrupted refusal publication": ("fits", refusal_temporary),
            "fit binary": ("fits", fit_edit("k2-s3-w32.json", lambda v: v.__setitem__("binary_sha256", "0" * 64))),
            "other config": ("fits", fit_edit("k2-s3-w32.json", lambda v: v.__setitem__("config_sha256", "0" * 64))),
            "fit engine commit": ("fits", fit_edit("k2-s3-w32.json", lambda v: v.__setitem__("engine_commit", "f"))),
            "nonzero initial Adam step": ("fits", report_edit("k1-s1-w128.json",
                                                              lambda v: v.__setitem__("initial_adam_step", 3))),
            "report steps": ("fits", report_edit("k1-s2-w64.json",
                                                 lambda v: v["config"].__setitem__("steps", v["config"]["steps"] + 1))),
            "report cadence": ("fits", report_edit("k1-s2-w64.json", lambda v: v["config"].__setitem__(
                "heldout_every_steps", v["config"]["heldout_every_steps"] + 1))),
            "report learning rate": ("fits", report_edit("k1-s2-w64.json",
                                                         lambda v: v.__setitem__("learning_rate_bits", 1))),
            "report fit group count": ("fits", report_edit("k1-s3-w32.json",
                                                           lambda v: v.__setitem__("fit_group_count", 1))),
            "report held-out group count": ("fits", report_edit("k1-s3-w32.json",
                                                                lambda v: v.__setitem__("heldout_group_count", 1))),
            "missing final evaluation": ("fits", report_edit("k2-s1-w128.json", drop_final)),
            "missing held-out row": ("fits", report_edit("k2-s1-w64.json", remove_held_out_row)),
            "duplicate held-out row": ("fits", report_edit("k2-s1-w64.json", duplicate_held_out_row)),
            "unrecorded fresh-Adam state": ("fits", fit_edit("k1-s1-w64.json",
                                                             lambda v: v.__setitem__("fresh_adam_state_sha256",
                                                                                     "0" * 64))),
            "transform chain": ("fits", fit_edit("k1-s1-w32.json",
                                                 lambda v: v.__setitem__("transformed_state_sha256", "0" * 64))),
            "w64 is not the parent": ("fits", fit_edit("k1-s1-w64.json", w64_child)),
            "widen seed": ("fits", fit_edit("k1-s1-w128.json", widen_seed)),
            # Pairing.
            "minibatch order differs": ("pairing", report_edit("k4-s1-w32.json", lambda v: v.__setitem__(
                "minibatch_order_sha256", "0" * 64))),
            "fit ids differ across arms": ("pairing", report_edit("k4-s2-w32.json", lambda v: v.__setitem__(
                "fit_game_ids", v["fit_game_ids"][::-1]))),
            "source hash differs across arms": ("pairing", report_edit("k4-s2-w128.json", lambda v: v.__setitem__(
                "decision_source_sha256", "0" * 64))),
            "w64 step 0 differs across seeds": ("pairing", fit_edit("k4-s3-w64.json", w64_step0_across_seeds)),
            "w64 fresh state differs across seeds": ("pairing", fit_edit("k4-s3-w64.json", w64_fresh)),
            "fit inputs against the data record": ("pairing", fit_inputs_differ),
            # Function preservation.
            "missing preservation record": ("function_preservation", missing("preserve/k6.json")),
            "forward mismatch": ("function_preservation", preserve_edit(
                lambda v: v["seeds"][1].__setitem__("mismatches", 1))),
            "preservation not passed": ("function_preservation", preserve_edit(lambda v: v.__setitem__("pass", False))),
            "preservation engine commit": ("function_preservation", preserve_edit(
                lambda v: v.__setitem__("engine_commit", "f"))),
            "preservation binary": ("function_preservation", preserve_edit(
                lambda v: v.__setitem__("binary_sha256", "0" * 64))),
            "preserved child differs": ("function_preservation", preserve_edit(
                lambda v: v["seeds"][2].__setitem__("child_state_sha256", "0" * 64))),
            "preservation skipped inputs": ("function_preservation", preservation_inputs_short),
            "step-0 bits differ": ("function_preservation", fit_edit("k5-s4-w128.json", step0_differs)),
            # Numerical validity.
            "non-finite error": ("numerical_validity", report_edit("k7-s1-w128.json", null_error)),
            "row group count": ("numerical_validity", report_edit("k7-s1-w128.json", lambda v: v["heldout"][-1][
                "games"][0].__setitem__("groups", v["heldout"][-1]["games"][0]["groups"] + 1))),
            "zero chain denominator": ("numerical_validity", zero_w64_chain),
            "zero pooled denominator": ("numerical_validity", zero_w64_everywhere),
        }
        for name, (check, damage) in cases.items():
            with self.subTest(name):
                tree = self.copy(name.replace(" ", "-"))
                section7 = damage(tree)
                read = self.void_read(tree, section7)
                self.assertEqual(read["void_check"], check, read["void_detail"])

    def test_parent_and_import_bytes_are_verified(self) -> None:
        for name, target in (("parent", "parents/r2-s3-b512.json"), ("import input", "parents/init.f32le")):
            with self.subTest(name):
                tree, section7 = self.fresh(name.replace(" ", "-"))
                tree.path(target).write_bytes(b"changed")
                self.assertEqual(self.void_read(tree, section7)["void_check"], "data")

    def test_nested_import_inputs_bind_the_read(self) -> None:
        tree, section7 = self.fresh("nested")
        status, _, error = run(tree, section7)
        self.assertEqual(status, 0, error)
        config = load(tree.path("config.json"))
        before = reader.inputs_sha256(config, sha(tree.path("config.json").read_bytes()), ANALYZER_SHA256, section7,
                                      tree.root)
        tree.path("parents/init.f32le").write_bytes(b"changed nested import input")
        after = reader.inputs_sha256(config, sha(tree.path("config.json").read_bytes()), ANALYZER_SHA256, section7,
                                     tree.root)
        self.assertNotEqual(before, after)
        status, _, error = run(tree, section7, "--rerender")
        self.assertEqual(status, 2)
        self.assertIn("inputs differ", error)

    def test_an_interrupted_read_publication_never_yields_two_reads(self) -> None:
        # A crash between writing the read and publishing it: the complete temporary file stays, READ.json does
        # not exist.
        tree = self.copy("interrupted-read")
        with mock.patch.object(reader.os, "link", side_effect=OSError("simulated crash before publication")):
            status, read, error = run(tree, self.template_section7)
        self.assertEqual((status, read), (2, None))
        temporaries = list(tree.root.glob(".READ.json.tmp-*"))
        self.assertEqual(len(temporaries), 1)
        rendered = temporaries[0].read_bytes()
        # The next run publishes that read as is (its read time included) instead of rendering another.
        reader.current_time = lambda: NOW + 600
        try:
            status, read, error = run(tree, self.template_section7)
        finally:
            reader.current_time = lambda: NOW
        self.assertEqual(status, 0, error)
        self.assertEqual(tree.path(reader.READ_NAME).read_bytes(), rendered)
        self.assertEqual(read["read_at_unix"], NOW)
        self.assertEqual(list(tree.root.glob(".READ.json.tmp-*")), [])
        status, _, error = run(tree, self.template_section7)
        self.assertEqual(status, 2)
        self.assertIn("has been read", error)

        # An unpublished read of inputs that have since changed is refused, not replaced by a new read.
        tree = self.copy("interrupted-read-changed")
        with mock.patch.object(reader.os, "link", side_effect=OSError("simulated crash before publication")):
            self.assertEqual(run(tree, self.template_section7)[0], 2)
        tree.edit("fits/k1-s1-w32.json", lambda v: v["report"]["steps"][0].__setitem__("loss", 0.25))
        status, read, error = run(tree, self.template_section7)
        self.assertEqual((status, read), (2, None))
        self.assertIn("unpublished read of other inputs", error)

        # A truncated temporary (a crash while writing) is not a read: it is removed and the root is read.
        tree = self.copy("truncated-read")
        tree.path(".READ.json.tmp-5").write_bytes(b'{"schema": "mtg-kernel-b-width')
        status, read, error = run(tree, self.template_section7)
        self.assertEqual(status, 0, error)
        self.assertEqual(read["verdict"], "FUND_B")
        self.assertFalse(tree.path(".READ.json.tmp-5").exists())

    def test_signed_zero_is_not_bit_identical(self) -> None:
        tree = self.copy("signed-zero")
        # Every seed's w64 keeps +0.0 (so w64's step-0 reports still agree); w128 has -0.0.
        for s in (1, 2, 3, 4):
            for arm, zero in (("w64", 0.0), ("w128", -0.0)):
                tree.edit(f"fits/k1-s{s}-{arm}.json", lambda v, zero=zero: v["report"]["heldout"][0]["games"][0]
                          .__setitem__("policy_nll_sum", zero))
        self.assertEqual(self.void_read(tree, tree.resign())["void_check"], "function_preservation")

    def test_void_on_attrition(self) -> None:
        cases = {
            "pooled": (dict(excluded=((2, "bwps-k2-f0-o1-i3"),),
                            analysis={"max_pooled_nonnatural_holdout_fraction": 0.0})),
            "chain holdout": (dict(excluded=((2, "bwps-k2-f0-o1-i3"), (2, "bwps-k2-f1-o1-i4")),
                                   analysis={"max_chain_nonnatural_holdout": 1})),
            "chain fit": (dict(excluded=((4, "bwps-k4-f0-o1-i0"), (4, "bwps-k4-f1-o1-i2")),
                               analysis={"max_chain_nonnatural_fit": 1})),
            "empty pair": (dict(excluded=((5, "bwps-k5-f2-o0-i3"), (5, "bwps-k5-f2-o0-i4")))),
        }
        for name, options in cases.items():
            with self.subTest(name):
                tree, section7 = self.fresh(name.replace(" ", "-"), **options)
                self.assertEqual(self.void_read(tree, section7)["void_check"], "attrition")
        tree, section7 = self.fresh("excluded-ok", excluded=((5, "bwps-k5-f2-o0-i3"), (3, "bwps-k3-f0-o0-i1")))
        status, read, _ = run(tree, section7)
        self.assertEqual(read["verdict"], "FUND_B", read.get("void_detail"))
        self.assertEqual(len(read["exclusions"]), 2)
        # The pooled denominator is the scheduled holdout count (8 chains x 9 pairs x 2), not the retained one.
        attrition = [check for check in read["checks"] if check["check"] == "attrition"][0]
        self.assertEqual(attrition["attempted_holdout"], 144)

    def test_void_on_sensitivity_and_bootstrap_denominator(self) -> None:
        tree, section7 = self.fresh("sensitivity", w32_factor=0.9)
        self.assertEqual(self.void_read(tree, section7)["void_check"], "sensitivity_control")
        # w64 is positive only on one holdout game per chain, in seed 1: every chain's own loss is positive, but
        # some bootstrap draws resample no positive game, so their w64 denominator is 0.
        positive = {k: f"bwps-k{k}-f0-o0-i3" for k in range(1, 9)}

        def override(k, s, arm, game, value):
            return value if (s == 1 and game == positive[k]) else 0.0

        tree, section7 = self.fresh("bootstrap-denominator", override=override, draws=2000)
        read = self.void_read(tree, section7)
        self.assertEqual(read["void_check"], "numerical_validity")
        self.assertIn("bootstrap draw", read["void_detail"])

    def test_interrupted_publications_are_ignored_and_listed(self) -> None:
        tree = self.copy("temporary")
        (tree.path("fits") / ".k1-s1-w64.json.tmp-1234").write_bytes(b"{")
        status, read, error = run(tree, self.template_section7)
        self.assertEqual(status, 0, error)
        self.assertEqual(read["verdict"], "FUND_B", read.get("void_detail"))
        self.assertEqual(read["ignored_interrupted_publications"], [".k1-s1-w64.json.tmp-1234"])

    def test_read_once_per_root_rerender_and_refusals(self) -> None:
        tree = self.copy("once")
        export = self.root / "exports" / "read.json"
        status, first, error = run(tree, self.template_section7, "--out", str(export))
        self.assertEqual(status, 0, error)
        written = tree.path(reader.READ_NAME).read_bytes()
        self.assertEqual(export.read_bytes(), written)
        # A second read of the same root is refused, whatever --out says.
        for extra in ((), ("--out", str(self.root / "exports" / "other.json"))):
            status, _, error = run(tree, self.template_section7, *extra)
            self.assertEqual(status, 2)
            self.assertIn("has been read", error)
        self.assertFalse((self.root / "exports" / "other.json").exists())
        # A re-render reproduces the bytes at the recorded read time, even after the 48 hours.
        reader.current_time = lambda: NOW + 72 * 3600
        try:
            status, _, error = run(tree, self.template_section7, "--rerender")
        finally:
            reader.current_time = lambda: NOW
        self.assertEqual(status, 0, error)
        self.assertEqual(tree.path(reader.READ_NAME).read_bytes(), written)
        # Changed inputs, the store included, refuse a re-render and leave the read alone.
        path = tree.path("collect/k1/games.bin")
        path.write_bytes(path.read_bytes() + b"x")
        status, _, error = run(tree, self.template_section7, "--rerender")
        self.assertEqual(status, 2)
        self.assertIn("inputs differ", error)
        self.assertEqual(tree.path(reader.READ_NAME).read_bytes(), written)

    def test_refusals_write_nothing(self) -> None:
        tree = self.copy("refusals")
        cases = [
            ("section 7 hash", ("--section7-sha256", "0" * 64)),
            ("export in a record folder", ("--out", str(tree.path("fits/read.json")))),
            ("export in data", ("--out", str(tree.path("data/read.json")))),
            ("re-render without a read", ("--rerender",)),
        ]
        for name, extra in cases:
            with self.subTest(name):
                arguments = ["--config", str(tree.path("config.json")), "--root", str(tree.root)]
                if "--section7-sha256" not in extra:
                    arguments += ["--section7-sha256", self.template_section7]
                with contextlib.redirect_stderr(io.StringIO()), contextlib.redirect_stdout(io.StringIO()):
                    status = reader.main([*arguments, *extra])
                self.assertEqual(status, 2)
                self.assertFalse(tree.path(reader.READ_NAME).exists())
        config = load(tree.path("config.json"))
        config["analyzer"]["sha256"] = "0" * 64
        write(tree.path("config.json"), config)
        status, read, error = run(tree, self.template_section7)
        self.assertEqual((status, read), (2, None))
        self.assertIn("SHA-256", error)
        config = json.loads(SHIPPED_CONFIG.read_bytes())
        config["fitting"]["momentum"] = 0.9
        with self.assertRaises(reader.Refused):
            reader.check_config(config)
        config = json.loads(SHIPPED_CONFIG.read_bytes())
        config["analysis"]["bootstrap_draw_order"] = "pairs-first"
        with self.assertRaises(reader.Refused):
            reader.check_config(config)


if __name__ == "__main__":
    unittest.main()
