"""Tests of the B width pre-screen's single-read analyzer (screen note v1.2 sections 5, 6 and 8) on synthetic
driver records with a known effect and known pairing."""

from __future__ import annotations

import contextlib
import copy
import hashlib
import io
import json
import math
import random
import sys
import tempfile
import unittest
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
TOOLS = REPO_ROOT / "python" / "tools"
if str(TOOLS) not in sys.path:
    sys.path.insert(0, str(TOOLS))

import b_width_prescreen_read_v1 as reader  # noqa: E402

SHIPPED_CONFIG = REPO_ROOT / "data" / "b_width_prescreen_v1" / "config.json"
ANALYZER_SHA256 = hashlib.sha256((TOOLS / "b_width_prescreen_read_v1.py").read_bytes()).hexdigest()
STEPS, EVERY = 4, 2


def sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def write(path: Path, value: object) -> bytes:
    data = (json.dumps(value, indent=1) + "\n").encode("utf-8")
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(data)
    return data


class Tree:
    """A synthetic driver root: 8 chains, 3 decks (9 ordered pairs), 5 games per pair, 4 fitting seeds.

    Per holdout game the final w64 error is a seeded positive value; w128's is (1 - effect(chain)) times it and
    w32's ``w32_factor`` times it, so R is known. Step 0 is shared by w64 and w128.
    """

    def __init__(self, root: Path, *, effect: float = 0.1, chain_effects: dict[int, float] | None = None,
                 w32_factor: float = 1.25, excluded: tuple[tuple[int, str], ...] = (), draws: int = 400,
                 noise: bool = False) -> None:
        self.root = root
        config = json.loads(SHIPPED_CONFIG.read_bytes())
        config["decks"] = config["decks"][:3]
        config["analysis"]["bootstrap_draws"] = draws
        config["analysis"]["interval_lower_index"] = max(0, draws * 25 // 1000)
        config["analysis"]["interval_upper_index"] = draws - 1 - max(0, draws * 25 // 1000)
        config["analyzer"]["sha256"] = ANALYZER_SHA256
        self.config = config
        self.effect = {k: (chain_effects or {}).get(k, effect) for k in range(1, 9)}
        self.w32_factor = w32_factor
        self.excluded = set(excluded)
        self.noise = noise
        self.errors: dict[tuple[int, int, str], dict[str, float]] = {}
        self.pairs: dict[int, dict[tuple[int, int], list[str]]] = {}

    def config_bytes(self) -> bytes:
        return (json.dumps(self.config, indent=1) + "\n").encode("utf-8")

    def build(self) -> str:
        """Writes the tree; returns SECTION7.json's SHA-256."""
        config = self.config
        config_path = self.root / "config.json"
        config_path.write_bytes(self.config_bytes())
        config_sha = sha(self.config_bytes())
        fitting = config["fitting"]
        rng = random.Random(20261027)
        section7_chains = []
        for chain in config["chains"]:
            k = chain["index"]
            attempted = {"fit": [], "holdout": []}
            natural = {"fit": [], "holdout": []}
            excluded, holdout_games = [], []
            groups_of = {}
            pairs = {}
            for f in range(3):
                for o in range(3):
                    pairs[(f, o)] = []
                    for i in range(5):
                        game = f"bwps-k{k}-f{f}-o{o}-i{i}"
                        split = "fit" if i <= 2 else "holdout"
                        attempted[split].append(game)
                        if (k, game) in self.excluded:
                            excluded.append({"id": game, "split": split, "focal": f, "opposing": o,
                                             "terminal_classification": "truncated",
                                             "terminal_reason": "physical_decision_cap_reached:100000"})
                            continue
                        natural[split].append(game)
                        groups_of[game] = 10 + rng.randrange(40)
                        if split == "holdout":
                            holdout_games.append({"id": game, "focal": f, "opposing": o, "index": i,
                                                  "groups": groups_of[game]})
                            pairs[(f, o)].append(game)
            self.pairs[k] = pairs
            parent_state = sha(f"parent-{k}".encode())
            collection = {"schema": reader.COLLECTION_SCHEMA, "config_sha256": config_sha, "chain": k,
                          "source": chain["source"], "parent_state_sha256": parent_state,
                          "feature_contract_digest": config["feature_contract_digest"],
                          "feature_encoding_digest": config["feature_encoding_digest"]}
            collection_bytes = write(self.root / "collect" / f"k{k}" / "collection.json", collection)
            record = {"schema": reader.DATA_SCHEMA, "config_sha256": config_sha, "chain": k,
                      "chain_id": chain["id"], "parent_checkpoint_sha256": chain["source"]["checkpoint"]["sha256"],
                      "parent_state_sha256": parent_state, "collection_record_sha256": sha(collection_bytes),
                      "store_sha256": sha(b"store"), "attempted": attempted, "natural": natural,
                      "excluded": excluded, "fit_groups": sum(groups_of[g] for g in natural["fit"]),
                      "holdout_groups": sum(groups_of[g] for g in natural["holdout"]), "steps": STEPS,
                      "heldout_every_steps": EVERY, "outcome_source_sha256": sha(f"outcome-{k}".encode()),
                      "decision_source_sha256": sha(f"decision-{k}".encode()), "holdout_games": holdout_games}
            data_bytes = write(self.root / "data" / f"k{k}.json", record)
            section7_chains.append({"chain": k, "chain_id": chain["id"],
                                    "parent_checkpoint_sha256": record["parent_checkpoint_sha256"],
                                    "collection_record_sha256": sha(collection_bytes),
                                    "data_record_sha256": sha(data_bytes), "attempted": attempted,
                                    "natural": natural, "excluded": excluded,
                                    "outcome_source_sha256": record["outcome_source_sha256"],
                                    "decision_source_sha256": record["decision_source_sha256"],
                                    "fit_groups": record["fit_groups"], "steps": STEPS})
            base = {game: 0.2 + rng.random() for game in natural["holdout"]}
            preservation_seeds = []
            for s in fitting["fitting_seeds"]:
                minibatch_seed = reader.domain_seed(fitting["minibatch_seed_domain"], k, s)
                widen_seed = reader.domain_seed(fitting["widen_seed_domain"], k, s)
                child = sha(f"widened-{k}-{s}".encode())
                preservation_seeds.append({"seed": s, "widen_seed": widen_seed, "child_state_sha256": child,
                                           "inputs_compared": 1000, "mismatches": 0, "first_mismatch": None})
                for arm in reader.ARMS:
                    jitter = 1.0 + (0.01 * rng.random() if self.noise else 0.0)
                    factor = {"w64": 1.0, "w128": 1.0 - self.effect[k], "w32": self.w32_factor}[arm]
                    final = {game: base[game] * factor * (jitter if arm != "w64" else 1.0) for game in base}
                    self.errors[(k, s, arm)] = final
                    start = {game: 1.5 * base[game] * (1.1 if arm == "w32" else 1.0) for game in base}

                    def evaluation(after_step: int, values: dict[str, float]) -> dict:
                        rows = [{"game_id": game, "groups": groups_of[game],
                                 "value_squared_error_sum": values[game] * groups_of[game],
                                 "policy_nll_sum": 0.5 * groups_of[game]} for game in natural["holdout"]]
                        total = sum(row["value_squared_error_sum"] for row in rows)
                        return {"after_step": after_step, "value_mse": total / record["holdout_groups"],
                                "policy_nll_mean": 0.5, "games": rows}

                    middle = {game: (start[game] + final[game]) / 2 for game in base}
                    transformed = {"w64": parent_state, "w128": child, "w32": sha(f"w32-{k}".encode())}[arm]
                    transform = {"construction_version": reader.CONSTRUCTIONS[arm],
                                 "parent_state_sha256": parent_state, "child_state_sha256": transformed}
                    if arm == "w128":
                        transform["seed"] = widen_seed
                    fresh = sha(f"fresh-{k}-{s}-{arm}".encode())
                    report = {
                        "entry_version": "native-screen-value-fit-v1", "architecture": reader.ARCHITECTURES[arm],
                        "input_generation": "v4", "value_target_source": "terminal_return_v1",
                        "policy_term": "off-every-advantage-positive-zero-v1",
                        "config": {"value_target_source": "terminal_return_v1", "steps": STEPS,
                                   "batch_groups": fitting["batch_groups"],
                                   "learning_rate": fitting["learning_rate"],
                                   "value_coefficient": fitting["value_coefficient"],
                                   "minibatch_seed": minibatch_seed, "heldout_every_steps": EVERY,
                                   "forward_workers": 1},
                        "learning_rate_bits": reader.f32_bits(fitting["learning_rate"]),
                        "value_coefficient_bits": reader.f32_bits(fitting["value_coefficient"]),
                        "initial_state_sha256": fresh, "final_state_sha256": sha(f"final-{k}-{s}-{arm}".encode()),
                        "initial_adam_step": 0, "final_adam_step": STEPS,
                        "fit_game_ids": natural["fit"], "heldout_game_ids": natural["holdout"],
                        "fit_group_count": record["fit_groups"], "heldout_group_count": record["holdout_groups"],
                        "outcome_source_sha256": record["outcome_source_sha256"],
                        "decision_source_sha256": record["decision_source_sha256"],
                        "minibatch_order_sha256": sha(f"order-{k}-{s}".encode()),
                        "steps": [{"step": n, "loss": 0.3, "value_sum": 1.0, "policy_sum": 0.0, "adam_step": n}
                                  for n in range(1, STEPS + 1)],
                        "heldout": [evaluation(0, start if arm != "w128" else
                                               {g: 1.5 * base[g] for g in base}),
                                    evaluation(EVERY, middle), evaluation(STEPS, final)],
                    }
                    fit = {"schema": reader.FIT_SCHEMA, "config_sha256": config_sha,
                           "data_record_sha256": sha(data_bytes), "engine_commit": "0" * 40, "chain": k, "seed": s,
                           "arm": arm, "minibatch_seed": minibatch_seed,
                           "widen_seed": widen_seed if arm == "w128" else None,
                           "parent_state_sha256": parent_state, "transform": transform,
                           "transformed_state_sha256": transformed, "fresh_adam_state_sha256": fresh,
                           "report": report}
                    write(self.root / "fits" / f"k{k}-s{s}-{arm}.json", fit)
            write(self.root / "preserve" / f"k{k}.json",
                  {"schema": reader.PRESERVATION_SCHEMA, "config_sha256": config_sha, "engine_commit": "0" * 40,
                   "chain": k, "data_record_sha256": sha(data_bytes), "parent_state_sha256": parent_state,
                   "inputs": 1000, "seeds": preservation_seeds, "pass": True})
        section7 = {"schema": reader.SECTION7_SCHEMA, "note": config["note"], "config_sha256": config_sha,
                    "analyzer": config["analyzer"], "engine_commit": "0" * 40, "binary_sha256": "1" * 64,
                    "chains": section7_chains}
        return sha(write(self.root / "data" / "SECTION7.json", section7))

    def edit(self, relative: str, change) -> None:
        path = self.root / relative
        value = json.loads(path.read_bytes())
        change(value)
        write(path, value)


def run(tree: Tree, section7: str, *extra: str) -> tuple[int, dict | None, str]:
    out = tree.root / "READ.json"
    stdout, stderr = io.StringIO(), io.StringIO()
    with contextlib.redirect_stdout(stdout), contextlib.redirect_stderr(stderr):
        status = reader.main(["--config", str(tree.root / "config.json"), "--root", str(tree.root),
                              "--section7-sha256", section7, "--out", str(out), *extra])
    read = json.loads(out.read_bytes()) if out.exists() else None
    return status, read, stderr.getvalue()


def pooled_r(tree: Tree) -> tuple[float, dict[int, float]]:
    """An independent direct computation of section 5's R and per-chain R."""
    seeds = tree.config["fitting"]["fitting_seeds"]
    loss = {}
    for (k, s, arm), errors in tree.errors.items():
        pair_means = [sum(errors[g] for g in games) / len(games) for games in tree.pairs[k].values()]
        loss[(k, s, arm)] = sum(pair_means) / len(pair_means)
    chains = range(1, 9)
    pooled = {arm: sum(loss[(k, s, arm)] for k in chains for s in seeds) / (8 * len(seeds)) for arm in reader.ARMS}
    per_chain = {k: 1 - sum(loss[(k, s, "w128")] for s in seeds) / sum(loss[(k, s, "w64")] for s in seeds)
                 for k in chains}
    return 1 - pooled["w128"] / pooled["w64"], per_chain


class AnalyzerTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory()
        self.root = Path(self.temporary.name)

    def tearDown(self) -> None:
        self.temporary.cleanup()

    def test_generator_and_seed_derivations(self) -> None:
        stream = reader.SplitMix64(0)
        self.assertEqual(stream.next_u64(), 0xE220A8397B1DCDAF)
        self.assertEqual(reader.domain_seed("b-width-prescreen-bootstrap-v1", 0), 9237536898453982863)
        self.assertEqual(reader.domain_seed("b-width-prescreen-minibatch-v1", 1, 1), 15313765789897877101)
        stream = reader.SplitMix64(7)
        self.assertTrue(all(0 <= stream.index(5) < 5 for _ in range(1000)))
        self.assertEqual(reader.f32_bits(0.0001), 953267991)

    def test_shipped_config_pins_this_analyzer(self) -> None:
        config = json.loads(SHIPPED_CONFIG.read_bytes())
        reader.check_config(config)
        self.assertEqual(config["analyzer"]["sha256"], ANALYZER_SHA256)
        self.assertEqual(config["analyzer"]["path"], "python/tools/b_width_prescreen_read_v1.py")

    def test_a_known_effect_is_recovered_and_funds(self) -> None:
        tree = Tree(self.root, effect=0.1)
        status, read, _ = run(tree, tree.build())
        self.assertEqual(status, 0)
        self.assertEqual(read["verdict"], "FUND_B", read.get("void_detail"))
        self.assertAlmostEqual(read["R"], 0.1, places=12)
        self.assertAlmostEqual(read["interval"][0], 0.1, places=12)
        self.assertAlmostEqual(read["interval"][1], 0.1, places=12)
        self.assertGreater(read["L"]["w32"], read["L"]["w64"])
        self.assertEqual([check["check"] for check in read["checks"]],
                         ["data", "attrition", "fits", "pairing", "function_preservation", "numerical_validity",
                          "sensitivity_control", "numerical_validity"])

    def test_a_small_or_null_effect_is_not_funded(self) -> None:
        for effect, lower_positive in ((0.01, True), (0.0, False)):
            with self.subTest(effect=effect):
                root = self.root / f"e{effect}"
                root.mkdir()
                tree = Tree(root, effect=effect)
                status, read, _ = run(tree, tree.build())
                self.assertEqual((status, read["verdict"]), (0, "NOT_FUNDED"))
                self.assertIn("not funded now", read["statement"])
                self.assertEqual(read["interval"][0] > 0, lower_positive)
                self.assertTrue(math.isfinite(read["R"]))

    def test_known_pairing_with_chain_effects_matches_a_direct_computation(self) -> None:
        effects = {k: 0.02 * (k - 1) for k in range(1, 9)}
        tree = Tree(self.root, chain_effects=effects, noise=True)
        status, read, _ = run(tree, tree.build())
        self.assertEqual(status, 0)
        expected, per_chain = pooled_r(tree)
        self.assertAlmostEqual(read["R"], expected, places=12)
        for k in range(1, 9):
            self.assertAlmostEqual(read["descriptive"]["per_chain_R"][str(k)], per_chain[k], places=12)
        self.assertLessEqual(read["interval"][0], read["interval"][1])
        self.assertLess(read["interval"][0], expected)
        self.assertGreater(read["interval"][1], expected)
        self.assertIsNotNone(read["descriptive"]["per_chain_t_interval_df7"])

    def test_bootstrap_draw_order_is_the_notes(self) -> None:
        effects = {k: 0.03 * k for k in range(1, 9)}
        tree = Tree(self.root, chain_effects=effects, noise=True, draws=2)
        tree.config["analysis"]["interval_lower_index"] = 0
        tree.config["analysis"]["interval_upper_index"] = 1
        status, read, _ = run(tree, tree.build())
        self.assertEqual(status, 0, read)
        # Reimplemented from the note's text: 8 chain occurrences; then per occurrence its 4 seed draws, then per
        # ordered pair in deck order its game draws; pairs weigh equally.
        analysis = tree.config["analysis"]
        stream = reader.SplitMix64(reader.domain_seed(analysis["bootstrap_seed_domain"], 0))
        seeds = tree.config["fitting"]["fitting_seeds"]
        values = []
        for _ in range(2):
            occurrences = [1 + stream.index(8) for _ in range(8)]
            sums = {"w64": 0.0, "w128": 0.0}
            for k in occurrences:
                drawn_seeds = [seeds[stream.index(4)] for _ in range(4)]
                drawn_games = {}
                for pair in [(f, o) for f in range(3) for o in range(3)]:
                    games = tree.pairs[k][pair]
                    drawn_games[pair] = [games[stream.index(len(games))] for _ in games]
                for s in drawn_seeds:
                    for arm in sums:
                        errors = tree.errors[(k, s, arm)]
                        sums[arm] += sum(sum(errors[g] for g in games) / len(games)
                                         for games in drawn_games.values()) / 9
            values.append(1 - sums["w128"] / sums["w64"])
        expected = sorted(values)
        self.assertAlmostEqual(read["interval"][0], expected[0], places=12)
        self.assertAlmostEqual(read["interval"][1], expected[1], places=12)

    def void_read(self, tree: Tree, section7: str | None = None) -> dict:
        status, read, _ = run(tree, section7 or sha((tree.root / "data" / "SECTION7.json").read_bytes()))
        self.assertEqual(status, 0)
        self.assertEqual(read["verdict"], "VOID")
        self.assertNotIn("R", read)
        return read

    def test_void_paths_fire(self) -> None:
        cases = {
            "missing report": ("fits", lambda t: (t.root / "fits" / "k3-s2-w32.json").unlink()),
            "duplicate report": ("fits", lambda t: (t.root / "fits" / "k3-s2-w32 (copy).json").write_bytes(
                (t.root / "fits" / "k3-s2-w32.json").read_bytes())),
            "nonzero initial Adam step": ("fits", lambda t: t.edit(
                "fits/k1-s1-w128.json", lambda v: v["report"].__setitem__("initial_adam_step", 3))),
            "unrecorded fresh-Adam state": ("fits", lambda t: t.edit(
                "fits/k1-s1-w64.json", lambda v: v.__setitem__("fresh_adam_state_sha256", "0" * 64))),
            "duplicate held-out row": ("fits", lambda t: t.edit(
                "fits/k2-s1-w64.json", lambda v: v["report"]["heldout"][-1]["games"].append(
                    v["report"]["heldout"][-1]["games"][0]))),
            "other config": ("fits", lambda t: t.edit(
                "fits/k2-s3-w32.json", lambda v: v.__setitem__("config_sha256", "0" * 64))),
            "minibatch order differs": ("pairing", lambda t: t.edit(
                "fits/k4-s1-w32.json", lambda v: v["report"].__setitem__("minibatch_order_sha256", "0" * 64))),
            "step-0 bits differ": ("function_preservation", lambda t: t.edit(
                "fits/k5-s4-w128.json", lambda v: v["report"]["heldout"][0]["games"][0].__setitem__(
                    "value_squared_error_sum",
                    math.nextafter(v["report"]["heldout"][0]["games"][0]["value_squared_error_sum"], 9.0)))),
            "forward mismatch": ("function_preservation", lambda t: t.edit(
                "preserve/k6.json", lambda v: v["seeds"][1].__setitem__("mismatches", 1))),
            "non-finite error": ("numerical_validity", lambda t: t.edit(
                "fits/k7-s1-w128.json", lambda v: v["report"]["heldout"][-1]["games"][0].__setitem__(
                    "value_squared_error_sum", None))),
        }
        for name, (check, damage) in cases.items():
            with self.subTest(name):
                root = self.root / name.replace(" ", "-")
                root.mkdir()
                tree = Tree(root)
                section7 = tree.build()
                damage(tree)
                read = self.void_read(tree, section7)
                self.assertEqual(read["void_check"], check, read["void_detail"])

    def test_signed_zero_is_not_bit_identical(self) -> None:
        tree = Tree(self.root)
        section7 = tree.build()
        # Every seed's w64 keeps +0.0 (so w64's step-0 reports still agree); w128 has -0.0.
        for s in tree.config["fitting"]["fitting_seeds"]:
            for arm, zero in (("w64", 0.0), ("w128", -0.0)):
                tree.edit(f"fits/k1-s{s}-{arm}.json", lambda v, zero=zero: v["report"]["heldout"][0]["games"][0]
                          .__setitem__("policy_nll_sum", zero))
        self.assertEqual(self.void_read(tree, section7)["void_check"], "function_preservation")

    def test_void_on_data_attrition_and_sensitivity(self) -> None:
        root = self.root / "section7"
        root.mkdir()
        tree = Tree(root)
        tree.build()
        self.assertEqual(self.void_read(tree, "0" * 64)["void_check"], "data")

        root = self.root / "attrition"
        root.mkdir()
        tree = Tree(root, excluded=((2, "bwps-k2-f0-o1-i3"), (2, "bwps-k2-f1-o1-i4")))
        tree.config["analysis"]["max_chain_nonnatural_holdout"] = 1
        read = self.void_read(tree, tree.build())
        self.assertEqual(read["void_check"], "attrition")

        root = self.root / "empty-pair"
        root.mkdir()
        tree = Tree(root, excluded=((5, "bwps-k5-f2-o0-i3"), (5, "bwps-k5-f2-o0-i4")))
        self.assertEqual(self.void_read(tree, tree.build())["void_check"], "attrition")

        root = self.root / "excluded-ok"
        root.mkdir()
        tree = Tree(root, excluded=((5, "bwps-k5-f2-o0-i3"), (3, "bwps-k3-f0-o0-i1")))
        status, read, _ = run(tree, tree.build())
        self.assertEqual(read["verdict"], "FUND_B", read.get("void_detail"))
        self.assertEqual(len(read["exclusions"]), 2)

        root = self.root / "sensitivity"
        root.mkdir()
        tree = Tree(root, w32_factor=0.9)
        self.assertEqual(self.void_read(tree, tree.build())["void_check"], "sensitivity_control")

    def test_read_once_rerender_and_refusals(self) -> None:
        tree = Tree(self.root)
        section7 = tree.build()
        status, first, _ = run(tree, section7)
        self.assertEqual(status, 0)
        written = (self.root / "READ.json").read_bytes()
        status, _, error = run(tree, section7)
        self.assertEqual(status, 2)
        self.assertIn("read once", error)
        status, _, _ = run(tree, section7, "--rerender")
        self.assertEqual(status, 0)
        self.assertEqual((self.root / "READ.json").read_bytes(), written)
        tree.edit("fits/k1-s1-w32.json", lambda v: v["report"]["steps"][0].__setitem__("loss", 0.25))
        status, _, error = run(tree, section7, "--rerender")
        self.assertEqual(status, 2)
        self.assertIn("inputs differ", error)
        self.assertEqual((self.root / "READ.json").read_bytes(), written)

    def test_refuses_a_config_that_does_not_pin_this_analyzer(self) -> None:
        tree = Tree(self.root)
        tree.config["analyzer"]["sha256"] = "0" * 64
        section7 = tree.build()
        status, read, error = run(tree, section7)
        self.assertEqual((status, read), (2, None))
        self.assertIn("SHA-256", error)
        tree = Tree(self.root)
        tree.config["fitting"]["momentum"] = 0.9
        tree.build()
        self.assertEqual(run(tree, section7)[0], 2)
        config = copy.deepcopy(json.loads(SHIPPED_CONFIG.read_bytes()))
        config["analysis"]["bootstrap_draw_order"] = "pairs-first"
        with self.assertRaises(reader.Refused):
            reader.check_config(config)


if __name__ == "__main__":
    unittest.main()
