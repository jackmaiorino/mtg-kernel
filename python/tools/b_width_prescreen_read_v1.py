"""The B width pre-screen's single read (collab-lead LANES/b-width-prescreen-20261027/SCREEN-NOTE.md v1.2,
frozen at 12aab0f2).

Standard library only. Every constant comes from the driver's config, whose ``analyzer.sha256`` must equal this
file's own SHA-256; the analyzer refuses to run otherwise. Inputs are the driver's records under one root
(``mtg-kernel`` ``expanded_deck_training_v1::b_width_prescreen_v1``) and the SHA-256 of ``data/SECTION7.json`` as the
note's section 7 records it.

The checks run in the note's section 6 order (data, attrition, fits, pairing, function preservation, numerical
validity, sensitivity control, decision). Any failure makes the read VOID; an invalid analysis is never "not funded".
The read file is written once. A second run over the same inputs is refused unless ``--rerender`` says it re-renders
that read; a re-render must reproduce the existing file's bytes exactly and never overwrites a different read.

Exit status: 0 when a read (any verdict) was written or re-rendered, 2 when the analyzer refused to run.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import struct
import sys
from pathlib import Path

READ_SCHEMA = "mtg-kernel-b-width-prescreen-read/v1"
CONFIG_SCHEMA = "mtg-kernel-b-width-prescreen-config/v1"
DATA_SCHEMA = "mtg-kernel-b-width-prescreen-data/v1"
COLLECTION_SCHEMA = "mtg-kernel-b-width-prescreen-collection/v1"
SECTION7_SCHEMA = "mtg-kernel-b-width-prescreen-section7/v1"
PRESERVATION_SCHEMA = "mtg-kernel-b-width-prescreen-preservation/v1"
FIT_SCHEMA = "mtg-kernel-b-width-prescreen-fit/v1"
DRAW_ORDER = "occurrence-seeds-then-pairs-v1"
ARMS = ("w64", "w128", "w32")
ARCHITECTURES = {"w64": "kernel-policy-value-net-8", "w128": "kernel-policy-value-net-8w128",
                 "w32": "kernel-policy-value-net-8"}
CONSTRUCTIONS = {"w64": "identity", "w128": "net8-to-net8w128-function-preserving-widening-v1",
                 "w32": "net8-w32-upper-half-fan-out-zeroed-control-v1"}
MASK64 = (1 << 64) - 1
CONFIG_KEYS = {
    "": {"schema", "note", "input_generation", "feature_contract_digest", "feature_encoding_digest", "decks",
         "collection", "chains", "arms", "fitting", "qualification", "analysis", "analyzer"},
    "collection": {"games_per_pair", "fit_indices", "holdout_indices", "seed_domain", "focal_seat",
                   "starting_player_rule", "seat_rule_basis", "max_physical_decisions", "max_policy_steps",
                   "runtime_rules", "postboard", "collection_sampler", "training_math"},
    "fitting": {"value_target_source", "learning_rate", "value_coefficient", "batch_groups", "epochs",
                "heldout_curve_points", "fitting_seeds", "minibatch_seed_domain", "widen_seed_domain"},
    "analysis": {"bootstrap_draws", "bootstrap_seed_domain", "bootstrap_seed_index", "bootstrap_draw_order",
                 "interval_lower_index", "interval_upper_index", "fund_lower_bound_above", "fund_min_point_r",
                 "t_critical_df7", "max_pooled_nonnatural_holdout_fraction", "max_chain_nonnatural_holdout",
                 "max_chain_nonnatural_fit", "min_holdout_games_per_pair"},
    "analyzer": {"path", "sha256"},
}


class Refused(Exception):
    """The analyzer does not run (no read is written)."""


class Void(Exception):
    """A section 6 check failed: the read is VOID."""

    def __init__(self, check: str, detail: object) -> None:
        super().__init__(f"{check}: {detail}")
        self.check = check
        self.detail = detail


def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def domain_seed(domain: str, *parts: int) -> int:
    """First 8 bytes, little-endian, of SHA-256 of the UTF-8 string ``domain|p1|p2...`` (decimal integers)."""
    text = "|".join([domain, *(str(part) for part in parts)])
    return int.from_bytes(hashlib.sha256(text.encode("utf-8")).digest()[:8], "little")


class SplitMix64:
    def __init__(self, seed: int) -> None:
        self.state = seed & MASK64

    def next_u64(self) -> int:
        self.state = (self.state + 0x9E3779B97F4A7C15) & MASK64
        z = self.state
        z = ((z ^ (z >> 30)) * 0xBF58476D1CE4E5B9) & MASK64
        z = ((z ^ (z >> 27)) * 0x94D049BB133111EB) & MASK64
        return z ^ (z >> 31)

    def index(self, n: int) -> int:
        """An index in [0, n): ``(next_u64 * n) >> 64``."""
        return (self.next_u64() * n) >> 64


def f32_bits(value: float) -> int:
    return struct.unpack("<I", struct.pack("<f", value))[0]


def bits_view(value: object) -> object:
    """A JSON value with every float replaced by its exact hex form, so equality is bit equality (sign of zero
    included)."""
    if isinstance(value, float):
        return ("f64", value.hex())
    if isinstance(value, list):
        return [bits_view(item) for item in value]
    if isinstance(value, dict):
        return {key: bits_view(item) for key, item in value.items()}
    return value


def finite(value: object) -> bool:
    return isinstance(value, (int, float)) and not isinstance(value, bool) and math.isfinite(value)


def number(value: float) -> object:
    """A float for the read file: non-finite values become strings, so the file stays valid JSON."""
    return value if math.isfinite(value) else repr(value)


def require(ok: bool, check: str, detail: object) -> None:
    if not ok:
        raise Void(check, detail)


def load_json(path: Path, check: str) -> tuple[dict, bytes]:
    try:
        data = path.read_bytes()
    except OSError as error:
        raise Void(check, f"{path}: {error}") from None
    try:
        return json.loads(data), data
    except json.JSONDecodeError as error:
        raise Void(check, f"{path}: {error}") from None


def check_config(config: dict) -> None:
    for section, keys in CONFIG_KEYS.items():
        actual = config if section == "" else config.get(section)
        if not isinstance(actual, dict) or set(actual) != keys:
            raise Refused(f"config section {section or 'top level'!r} does not have exactly the expected fields")
    if config["schema"] != CONFIG_SCHEMA:
        raise Refused("config schema differs")
    analysis = config["analysis"]
    if analysis["bootstrap_draw_order"] != DRAW_ORDER:
        raise Refused("unsupported bootstrap draw order")
    if not 0 <= analysis["interval_lower_index"] < analysis["interval_upper_index"] < analysis["bootstrap_draws"]:
        raise Refused("invalid interval indices")
    if config["arms"]["names"] != list(ARMS):
        raise Refused("arms must be w64, w128 and w32")


class Inputs:
    """Every input file read, with its SHA-256, for the read's input binding."""

    def __init__(self, root: Path) -> None:
        self.root = root
        self.files: dict[str, str] = {}

    def read(self, relative: str, check: str) -> tuple[dict, bytes]:
        value, data = load_json(self.root / relative, check)
        self.files[relative] = sha256_bytes(data)
        return value, data


def pairs_of(config: dict) -> list[tuple[int, int]]:
    decks = len(config["decks"])
    return [(focal, opposing) for focal in range(decks) for opposing in range(decks)]


def game_error(row: dict, check: str) -> float:
    groups = row.get("groups")
    total = row.get("value_squared_error_sum")
    require(isinstance(groups, int) and groups > 0, check, f"{row.get('game_id')}: group count {groups!r}")
    require(finite(total), check, f"{row.get('game_id')}: value_squared_error_sum {total!r}")
    value = total / groups
    require(math.isfinite(value), check, f"{row.get('game_id')}: per-game error {value!r}")
    return value


def pair_weighted_loss(errors: dict[str, float], pair_games: dict[tuple[int, int], list[str]]) -> float:
    """Equal-weight mean over ordered pairs of the mean per-game error of each pair's retained holdout games."""
    return sum(sum(errors[game] for game in games) / len(games) for games in pair_games.values()) / len(pair_games)


def analyze(config: dict, config_sha256: str, root: Path, section7_sha256: str) -> tuple[dict, dict]:
    """Returns (read body without verdict fields, input file hashes); raises Void on a failed check."""
    inputs = Inputs(root)
    checks: list[dict] = []
    body: dict = {"checks": checks}
    chains = [chain["index"] for chain in config["chains"]]
    seeds = config["fitting"]["fitting_seeds"]
    analysis = config["analysis"]
    pairs = pairs_of(config)

    # 1. Data.
    section7, section7_bytes = inputs.read("data/SECTION7.json", "data")
    require(sha256_bytes(section7_bytes) == section7_sha256, "data",
            "SECTION7.json differs from the hash section 7 records")
    require(section7.get("schema") == SECTION7_SCHEMA and section7.get("config_sha256") == config_sha256
            and section7.get("analyzer") == config["analyzer"], "data",
            "SECTION7.json is not this config's and analyzer's")
    frozen = {entry["chain"]: entry for entry in section7.get("chains", [])}
    require(sorted(frozen) == sorted(chains) and len(frozen) == len(section7.get("chains", [])), "data",
            "SECTION7.json does not list exactly the config's chains")
    data: dict[int, dict] = {}
    data_sha: dict[int, str] = {}
    for chain in config["chains"]:
        k = chain["index"]
        record, record_bytes = inputs.read(f"data/k{k}.json", "data")
        collection, collection_bytes = inputs.read(f"collect/k{k}/collection.json", "data")
        entry = frozen[k]
        parent = chain["source"]["checkpoint"]["sha256"]
        require(record.get("schema") == DATA_SCHEMA and record.get("config_sha256") == config_sha256
                and record.get("chain") == k, "data", f"chain {k}: data record is not this config's")
        require(sha256_bytes(record_bytes) == entry["data_record_sha256"], "data",
                f"chain {k}: data record differs from section 7")
        require(sha256_bytes(collection_bytes) == entry["collection_record_sha256"]
                == record["collection_record_sha256"], "data", f"chain {k}: collection record differs from section 7")
        require(collection.get("schema") == COLLECTION_SCHEMA and collection.get("config_sha256") == config_sha256
                and collection.get("chain") == k, "data", f"chain {k}: collection record is not this config's")
        require(parent == record["parent_checkpoint_sha256"] == entry["parent_checkpoint_sha256"]
                == collection["source"]["checkpoint"]["sha256"], "data", f"chain {k}: parent hash differs")
        require(collection["parent_state_sha256"] == record["parent_state_sha256"], "data",
                f"chain {k}: parent state differs")
        require(collection["feature_contract_digest"] == config["feature_contract_digest"]
                and collection["feature_encoding_digest"] == config["feature_encoding_digest"], "data",
                f"chain {k}: feature digests differ from section 1")
        for field in ("attempted", "natural", "excluded", "outcome_source_sha256", "decision_source_sha256",
                      "fit_groups", "steps"):
            require(record[field] == entry[field], "data", f"chain {k}: {field} differs from section 7")
        data[k] = record
        data_sha[k] = sha256_bytes(record_bytes)
    checks.append({"check": "data", "pass": True})

    # 2. Attrition.
    excluded_rows = []
    attempted_holdout = sum(len(data[k]["attempted"]["holdout"]) for k in chains)
    excluded_holdout = 0
    pair_games: dict[int, dict[tuple[int, int], list[str]]] = {}
    for k in chains:
        record = data[k]
        by_split = {"fit": 0, "holdout": 0}
        for row in record["excluded"]:
            by_split[row["split"]] += 1
            excluded_rows.append({"chain": k, "id": row["id"], "split": row["split"],
                                  "pair": [row["focal"], row["opposing"]],
                                  "terminal_classification": row["terminal_classification"],
                                  "terminal_reason": row["terminal_reason"]})
        excluded_holdout += by_split["holdout"]
        require(by_split["holdout"] <= analysis["max_chain_nonnatural_holdout"], "attrition",
                f"chain {k}: {by_split['holdout']} non-natural holdout games")
        require(by_split["fit"] <= analysis["max_chain_nonnatural_fit"], "attrition",
                f"chain {k}: {by_split['fit']} non-natural fit games")
        groups = {(f, o): [] for f, o in pairs}
        for game in record["holdout_games"]:
            groups[(game["focal"], game["opposing"])].append(game["id"])
        require([game["id"] for game in record["holdout_games"]] == record["natural"]["holdout"], "attrition",
                f"chain {k}: holdout games differ from the retained holdout ids")
        for pair, games in groups.items():
            require(len(games) >= max(1, analysis["min_holdout_games_per_pair"]), "attrition",
                    f"chain {k}: ordered pair {pair} keeps {len(games)} holdout games")
        pair_games[k] = groups
    require(excluded_holdout <= analysis["max_pooled_nonnatural_holdout_fraction"] * attempted_holdout, "attrition",
            f"{excluded_holdout} of {attempted_holdout} holdout games are non-natural")
    body["exclusions"] = excluded_rows
    checks.append({"check": "attrition", "pass": True, "excluded_holdout": excluded_holdout,
                   "attempted_holdout": attempted_holdout})

    # 3. Fits.
    expected = {f"k{k}-s{s}-{arm}.json" for k in chains for s in seeds for arm in ARMS}
    fits_dir = root / "fits"
    present = {path.name for path in fits_dir.iterdir()} if fits_dir.is_dir() else set()
    require(present == expected, "fits",
            {"missing": sorted(expected - present), "unexpected": sorted(present - expected)})
    fitting = config["fitting"]
    lr_bits, vc_bits = f32_bits(fitting["learning_rate"]), f32_bits(fitting["value_coefficient"])
    fits: dict[tuple[int, int, str], dict] = {}
    for k in chains:
        record = data[k]
        retained = record["natural"]["holdout"]
        for s in seeds:
            for arm in ARMS:
                fit, _ = inputs.read(f"fits/k{k}-s{s}-{arm}.json", "fits")
                where = f"chain {k} seed {s} {arm}"
                report = fit.get("report", {})
                cfg = report.get("config", {})
                require(fit.get("schema") == FIT_SCHEMA and fit.get("config_sha256") == config_sha256
                        and fit.get("data_record_sha256") == data_sha[k] and fit.get("chain") == k
                        and fit.get("seed") == s and fit.get("arm") == arm, "fits", f"{where}: not this fit's record")
                minibatch_seed = domain_seed(fitting["minibatch_seed_domain"], k, s)
                require(cfg.get("value_target_source") == fitting["value_target_source"]
                        == report.get("value_target_source")
                        and cfg.get("steps") == record["steps"] and cfg.get("batch_groups") == fitting["batch_groups"]
                        and report.get("learning_rate_bits") == lr_bits
                        and report.get("value_coefficient_bits") == vc_bits
                        and cfg.get("minibatch_seed") == minibatch_seed == fit.get("minibatch_seed")
                        and cfg.get("heldout_every_steps") == record["heldout_every_steps"]
                        and report.get("initial_adam_step") == 0
                        and report.get("architecture") == ARCHITECTURES[arm]
                        and report.get("input_generation") == config["input_generation"], "fits",
                        f"{where}: config differs from section 4")
                heldout = report.get("heldout", [])
                require(len(heldout) >= 2 and heldout[0].get("after_step") == 0
                        and heldout[-1].get("after_step") == record["steps"]
                        and len(report.get("steps", [])) == record["steps"], "fits",
                        f"{where}: missing its final evaluation")
                for evaluation in heldout:
                    ids = [row.get("game_id") for row in evaluation.get("games", [])]
                    require(len(ids) == len(set(ids)) and sorted(ids) == sorted(retained), "fits",
                            f"{where}: held-out rows at step {evaluation.get('after_step')} are not the retained "
                            "holdout games, each once")
                transform = fit.get("transform", {})
                parent_state = record["parent_state_sha256"]
                require(report.get("initial_state_sha256") == fit.get("fresh_adam_state_sha256")
                        and transform.get("construction_version") == CONSTRUCTIONS[arm]
                        and transform.get("parent_state_sha256") == fit.get("parent_state_sha256") == parent_state
                        and transform.get("child_state_sha256") == fit.get("transformed_state_sha256"), "fits",
                        f"{where}: state hash chain broken")
                if arm == "w64":
                    require(fit["transformed_state_sha256"] == parent_state and fit.get("widen_seed") is None,
                            "fits", f"{where}: w64 is not the parent")
                elif arm == "w128":
                    widen_seed = domain_seed(fitting["widen_seed_domain"], k, s)
                    require(fit.get("widen_seed") == widen_seed == transform.get("seed"), "fits",
                            f"{where}: widen seed differs from section 3")
                fits[(k, s, arm)] = fit
    checks.append({"check": "fits", "pass": True, "reports": len(fits)})

    # 4. Pairing.
    for k in chains:
        reference = fits[(k, seeds[0], "w64")]["report"]
        for s in seeds:
            order = {fits[(k, s, arm)]["report"]["minibatch_order_sha256"] for arm in ARMS}
            require(len(order) == 1, "pairing", f"chain {k} seed {s}: minibatch orders differ across arms")
            for arm in ARMS:
                report = fits[(k, s, arm)]["report"]
                for field in ("fit_game_ids", "heldout_game_ids", "outcome_source_sha256",
                              "decision_source_sha256"):
                    require(report[field] == reference[field], "pairing",
                            f"chain {k} seed {s} {arm}: {field} differs within the chain")
            require(bits_view(fits[(k, s, "w64")]["report"]["heldout"][0])
                    == bits_view(reference["heldout"][0]), "pairing",
                    f"chain {k} seed {s}: w64's step-0 report differs across seeds")
        require(reference["fit_game_ids"] == data[k]["natural"]["fit"]
                and reference["heldout_game_ids"] == data[k]["natural"]["holdout"]
                and reference["outcome_source_sha256"] == data[k]["outcome_source_sha256"]
                and reference["decision_source_sha256"] == data[k]["decision_source_sha256"], "pairing",
                f"chain {k}: fit inputs differ from the data record")
    checks.append({"check": "pairing", "pass": True})

    # 5. Function preservation.
    for k in chains:
        preservation, _ = inputs.read(f"preserve/k{k}.json", "function_preservation")
        require(preservation.get("schema") == PRESERVATION_SCHEMA
                and preservation.get("config_sha256") == config_sha256
                and preservation.get("data_record_sha256") == data_sha[k] and preservation.get("chain") == k
                and preservation.get("pass") is True and finite(preservation.get("inputs"))
                and preservation["inputs"] > 0, "function_preservation", f"chain {k}: no passed forward comparison")
        by_seed = {row.get("seed"): row for row in preservation.get("seeds", [])}
        require(sorted(by_seed) == sorted(seeds), "function_preservation", f"chain {k}: seeds differ")
        for s in seeds:
            row, fit = by_seed[s], fits[(k, s, "w128")]
            require(row.get("mismatches") == 0 and row.get("inputs_compared") == preservation["inputs"]
                    and row.get("widen_seed") == fit["widen_seed"]
                    and row.get("child_state_sha256") == fit["transformed_state_sha256"], "function_preservation",
                    f"chain {k} seed {s}: forward outputs not bit-identical on every input")
            require(bits_view(fit["report"]["heldout"][0]["games"])
                    == bits_view(fits[(k, s, "w64")]["report"]["heldout"][0]["games"]), "function_preservation",
                    f"chain {k} seed {s}: step-0 per-game held-out reports differ")
    checks.append({"check": "function_preservation", "pass": True})

    # Per-game errors and losses (section 5), with their numerical validity.
    errors: dict[tuple[int, int, str], dict[str, float]] = {}
    losses: dict[tuple[int, int, str], float] = {}
    for key, fit in fits.items():
        final = fit["report"]["heldout"][-1]
        groups = {game["id"]: game["groups"] for game in data[key[0]]["holdout_games"]}
        rows = {}
        for row in final["games"]:
            require(row.get("groups") == groups.get(row.get("game_id")), "numerical_validity",
                    f"{key}: {row.get('game_id')} group count differs from the data record")
            rows[row["game_id"]] = game_error(row, "numerical_validity")
        errors[key] = rows
        losses[key] = pair_weighted_loss(rows, pair_games[key[0]])
        require(math.isfinite(losses[key]), "numerical_validity", f"{key}: loss {losses[key]!r}")
    pooled = {arm: sum(losses[(k, s, arm)] for k in chains for s in seeds) / (len(chains) * len(seeds))
              for arm in ARMS}
    require(all(math.isfinite(value) for value in pooled.values()) and pooled["w64"] > 0, "numerical_validity",
            {arm: number(value) for arm, value in pooled.items()})
    r_point = 1.0 - pooled["w128"] / pooled["w64"]
    per_chain = {}
    for k in chains:
        mean = {arm: sum(losses[(k, s, arm)] for s in seeds) / len(seeds) for arm in ("w64", "w128")}
        require(math.isfinite(mean["w64"]) and mean["w64"] > 0, "numerical_validity",
                f"chain {k}: w64 loss {mean['w64']!r}")
        per_chain[k] = 1.0 - mean["w128"] / mean["w64"]
        require(math.isfinite(per_chain[k]), "numerical_validity", f"chain {k}: R {per_chain[k]!r}")
    require(math.isfinite(r_point), "numerical_validity", f"R {r_point!r}")
    checks.append({"check": "numerical_validity", "pass": True, "stage": "observed"})

    # 6. Sensitivity control.
    require(pooled["w32"] > pooled["w64"], "sensitivity_control",
            {"L_w32": number(pooled["w32"]), "L_w64": number(pooled["w64"])})
    checks.append({"check": "sensitivity_control", "pass": True})

    # 7. Decision: the bootstrap.
    draws = bootstrap(config, chains, seeds, pairs, pair_games, errors)
    ordered = sorted(draws)
    interval = [ordered[analysis["interval_lower_index"]], ordered[analysis["interval_upper_index"]]]
    require(all(math.isfinite(value) for value in interval), "numerical_validity", "interval endpoint")
    checks.append({"check": "numerical_validity", "pass": True, "stage": "bootstrap",
                   "draws": len(draws)})
    body.update({
        "L": {arm: pooled[arm] for arm in ARMS},
        "R": r_point,
        "interval": interval,
        "bootstrap_draws": len(draws),
        "descriptive": descriptive(config, chains, seeds, data, fits, pair_games, per_chain, pooled, interval),
    })
    return body, inputs.files


def bootstrap(config: dict, chains: list[int], seeds: list[int], pairs: list[tuple[int, int]],
              pair_games: dict[int, dict[tuple[int, int], list[str]]],
              errors: dict[tuple[int, int, str], dict[str, float]]) -> list[float]:
    """The note's section 5 bootstrap: R for each draw, in draw order."""
    analysis = config["analysis"]
    stream = SplitMix64(domain_seed(analysis["bootstrap_seed_domain"], analysis["bootstrap_seed_index"]))
    # Per chain, the retained holdout games in pair order, and each arm's per-game errors in that order.
    order = {k: [game for pair in pairs for game in pair_games[k][pair]] for k in chains}
    vectors = {(k, s, arm): [errors[(k, s, arm)][game] for game in order[k]]
               for k in chains for s in seeds for arm in ("w64", "w128")}
    results = []
    for _ in range(analysis["bootstrap_draws"]):
        occurrences = [chains[stream.index(len(chains))] for _ in chains]
        totals = {"w64": 0.0, "w128": 0.0}
        for k in occurrences:
            drawn_seeds = [seeds[stream.index(len(seeds))] for _ in seeds]
            weights = []
            for pair in pairs:
                games = pair_games[k][pair]
                counts = [0] * len(games)
                for _ in games:
                    counts[stream.index(len(games))] += 1
                scale = 1.0 / (len(games) * len(pairs))
                weights.extend(count * scale for count in counts)
            cache: dict[tuple[int, str], float] = {}
            for s in drawn_seeds:
                for arm in ("w64", "w128"):
                    if (s, arm) not in cache:
                        cache[(s, arm)] = math.fsum(w * e for w, e in zip(weights, vectors[(k, s, arm)]))
                    totals[arm] += cache[(s, arm)]
        count = len(occurrences) * len(seeds)
        w64, w128 = totals["w64"] / count, totals["w128"] / count
        require(math.isfinite(w64) and w64 > 0, "numerical_validity", f"bootstrap draw {len(results)}: L_w64 {w64!r}")
        value = 1.0 - w128 / w64
        require(math.isfinite(value), "numerical_validity", f"bootstrap draw {len(results)}: R {value!r}")
        results.append(value)
    return results


def descriptive(config: dict, chains: list[int], seeds: list[int], data: dict, fits: dict, pair_games: dict,
                per_chain: dict[int, float], pooled: dict[str, float], interval: list[float]) -> dict:
    """Section 6's descriptive items. None of them enters the verdict."""
    analysis = config["analysis"]
    values = [per_chain[k] for k in chains]
    t_interval = None
    if len(values) == 8:
        mean = sum(values) / len(values)
        sd = math.sqrt(sum((value - mean) ** 2 for value in values) / (len(values) - 1))
        half = analysis["t_critical_df7"] * sd / math.sqrt(len(values))
        t_interval = {"mean": mean, "lower": mean - half, "upper": mean + half}
    curves = {}
    curve_minimum = {arm: [] for arm in ARMS}
    for (k, s, arm), fit in fits.items():
        points = []
        for evaluation in fit["report"]["heldout"]:
            rows = {row["game_id"]: row["value_squared_error_sum"] / row["groups"] for row in evaluation["games"]}
            points.append([evaluation["after_step"], pair_weighted_loss(rows, pair_games[k])])
        curves[f"k{k}-s{s}-{arm}"] = points
        curve_minimum[arm].append(min(point[1] for point in points))
    minima = {arm: sum(values) / len(values) for arm, values in curve_minimum.items()}

    def rank(losses: dict[str, float]) -> list[str]:
        return sorted(ARMS, key=lambda arm: losses[arm])

    group_mse = {arm: sum(fits[(k, s, arm)]["report"]["heldout"][-1]["value_mse"] for k in chains for s in seeds)
                 / (len(chains) * len(seeds)) for arm in ARMS}
    policy_nll = {arm: sum(fits[(k, s, arm)]["report"]["heldout"][-1]["policy_nll_mean"] for k in chains
                           for s in seeds) / (len(chains) * len(seeds)) for arm in ARMS}
    fit_loss = {}
    for arm in ARMS:
        total, count = 0.0, 0
        for k in chains:
            epoch = -(-data[k]["fit_groups"] // config["fitting"]["batch_groups"])
            for s in seeds:
                steps = fits[(k, s, arm)]["report"]["steps"][-epoch:]
                total += sum(step["loss"] for step in steps)
                count += len(steps)
        fit_loss[arm] = total / count
    bootstrap_excludes_zero = interval[0] > 0 or interval[1] < 0
    t_excludes_zero = t_interval is not None and (t_interval["lower"] > 0 or t_interval["upper"] < 0)
    return {
        "per_chain_R": {str(k): per_chain[k] for k in chains},
        "per_chain_t_interval_df7": t_interval,
        "bootstrap_and_t_agree_on_excluding_zero": bootstrap_excludes_zero == t_excludes_zero,
        "endpoint_sensitivity": {
            "final_minus_curve_minimum": {arm: pooled[arm] - minima[arm] for arm in ARMS},
            "curve_minimum": minima,
            "ordering_final": rank(pooled),
            "ordering_curve_minima": rank(minima),
            "ordering_differs": rank(pooled) != rank(minima),
            "curve_minimum_rule": "per fit, the minimum pair-weighted held-out loss over its evaluation points; "
                                  "pooled as the mean over chains and seeds",
        },
        "heldout_curves": curves,
        "fit_set_loss_last_epoch": fit_loss,
        "group_weighted_heldout_mse": group_mse,
        "heldout_policy_nll": policy_nll,
    }


def verdict_of(config: dict, body: dict) -> tuple[str, str]:
    analysis = config["analysis"]
    if body["interval"][0] > analysis["fund_lower_bound_above"] and body["R"] >= analysis["fund_min_point_r"]:
        return "FUND_B", "Fund B: advance B to its reviewed study design (scope: note's top section)."
    return ("NOT_FUNDED", "B is not funded now; this diagnostic gave no positive evidence at this budget. "
            "This does not mean width cannot help continued RL.")


def render(config: dict, config_sha256: str, analyzer_sha256: str, root: Path, section7_sha256: str) -> dict:
    read: dict = {"schema": READ_SCHEMA, "analyzer_sha256": analyzer_sha256, "config_sha256": config_sha256,
                  "section7_sha256": section7_sha256}
    try:
        body, files = analyze(config, config_sha256, root, section7_sha256)
    except (Void, KeyError, TypeError, AttributeError, IndexError, ValueError, ZeroDivisionError) as error:
        void = error if isinstance(error, Void) else Void("malformed_input", f"{type(error).__name__}: {error}")
        read.update({"verdict": "VOID", "void_check": void.check, "void_detail": void.detail,
                     "statement": "VOID: a section 6 check failed; this is not a 'not funded' reading."})
        files = {}
    else:
        verdict, statement = verdict_of(config, body)
        read.update({"verdict": verdict, "statement": statement, "R": body["R"], "interval": body["interval"]})
        read.update({key: value for key, value in body.items() if key not in ("R", "interval")})
    read["inputs"] = files
    read["inputs_sha256"] = inputs_sha256(config_sha256, analyzer_sha256, section7_sha256, root)
    return read


def inputs_sha256(config_sha256: str, analyzer_sha256: str, section7_sha256: str, root: Path) -> str:
    """Binds the read to every file under the root's data, collect records, preserve and fits folders."""
    listing = []
    for folder in ("data", "preserve", "fits"):
        directory = root / folder
        if directory.is_dir():
            listing += [(path.relative_to(root).as_posix(), sha256_bytes(path.read_bytes()))
                        for path in sorted(directory.iterdir()) if path.is_file()]
    collect = root / "collect"
    if collect.is_dir():
        listing += [(path.relative_to(root).as_posix(), sha256_bytes(path.read_bytes()))
                    for path in sorted(collect.glob("k*/collection.json"))]
    document = {"config": config_sha256, "analyzer": analyzer_sha256, "section7": section7_sha256, "files": listing}
    return sha256_bytes(json.dumps(document, sort_keys=True).encode("utf-8"))


def sanitized(value: object) -> object:
    """The read with every non-finite float as a string, so the file stays valid JSON."""
    if isinstance(value, float):
        return number(value)
    if isinstance(value, list):
        return [sanitized(item) for item in value]
    if isinstance(value, dict):
        return {key: sanitized(item) for key, item in value.items()}
    return value


def read_bytes(read: dict) -> bytes:
    return (json.dumps(sanitized(read), indent=1, allow_nan=False) + "\n").encode("utf-8")


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--config", type=Path, required=True)
    parser.add_argument("--root", type=Path, required=True)
    parser.add_argument("--section7-sha256", required=True, help="SHA-256 of data/SECTION7.json from the note")
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--rerender", action="store_true",
                        help="re-render an existing read over identical inputs; it must reproduce its bytes")
    arguments = parser.parse_args(argv)
    try:
        config_bytes = arguments.config.read_bytes()
        config = json.loads(config_bytes)
        check_config(config)
        analyzer_sha256 = sha256_bytes(Path(__file__).read_bytes())
        if config["analyzer"]["sha256"] != analyzer_sha256:
            raise Refused("this analyzer's SHA-256 is not the one the config pins")
        config_sha256 = sha256_bytes(config_bytes)
        exists = arguments.out.exists()
        if exists and not arguments.rerender:
            raise Refused(f"{arguments.out} exists: the screen is read once (use --rerender only to re-render it)")
        if arguments.rerender and not exists:
            raise Refused("--rerender needs the existing read")
        if arguments.rerender:
            previous = json.loads(arguments.out.read_bytes())
            current = inputs_sha256(config_sha256, analyzer_sha256, arguments.section7_sha256, arguments.root)
            if previous.get("inputs_sha256") != current:
                raise Refused("the inputs differ from the existing read's; a re-render needs identical inputs")
        read = render(config, config_sha256, analyzer_sha256, arguments.root, arguments.section7_sha256)
        data = read_bytes(read)
        if arguments.rerender:
            if arguments.out.read_bytes() != data:
                raise Refused("the re-rendered read differs from the existing one; nothing was written")
        else:
            arguments.out.parent.mkdir(parents=True, exist_ok=True)
            with open(arguments.out, "xb") as handle:
                handle.write(data)
    except (Refused, OSError, ValueError, KeyError, TypeError) as error:
        print(f"b width prescreen read refused: {error}", file=sys.stderr)
        return 2
    print(json.dumps({"verdict": read["verdict"], "R": read.get("R"), "interval": read.get("interval"),
                      "out": str(arguments.out)}))
    return 0


if __name__ == "__main__":
    sys.exit(main())
