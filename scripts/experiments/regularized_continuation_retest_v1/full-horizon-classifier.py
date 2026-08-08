#!/usr/bin/env python3
"""Fail-closed classifier for the frozen full-horizon continuation panel.

The request contains exactly 21 H2H terminal-stream files:

* candidate generations 64, 128, 256 at 512 pairs;
* candidate and beta-zero-control generations 384 and 512 at 2048 pairs;
* training seeds 970001, 970002, 970003 and evaluation seed 982001.

The stream format is the Rust ``mtg-kernel-head-to-head-terminal-stream/v1``
artifact emitted by ``H2H_OUTCOME_JSON``.  This program deliberately retains
only hashes, paths, counts, and derived terminal-order statistics.  It never
copies raw episode rows into its output.

All strength and advancement quantities below use terminal W/D/L ordering
only.  Nonterminal diagnostics, if present in a source artifact, are ignored.
"""

from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
import math
import os
import sys
import tempfile
from pathlib import Path
from typing import Any, Dict, Iterable, List, Mapping, Sequence, Tuple


REQUEST_SCHEMA = "regularized-continuation-full-horizon-classifier-request/v1"
OUTPUT_SCHEMA = "regularized-continuation-full-horizon-classification/v1"
STREAM_SCHEMA = "mtg-kernel-head-to-head-terminal-stream/v1"
SEEDS = (970001, 970002, 970003)
DIAGNOSTIC_GENS = (64, 128, 256)
ENDPOINT_GENS = (384, 512)
ALL_GENS = (64, 128, 256, 384, 512)
EVAL_SEED = 982001
OPPONENT_GENERATION = 384
DIAGNOSTIC_PAIRS = 512
ENDPOINT_PAIRS = 2048
EXPECTED_STREAM_COUNT = 21
ALPHA = 0.05
C_TRUNCATION = 0.5
DELTA_WORTHWHILE = 0.003
DELTA_PROMOTE = 0.0
MAX_N = 2048
OVERALL_STABILITY_FLOOR = -44
SEAT_STABILITY_FLOOR = -31
ENDPOINT_TOLERANCE = -0.01
COLLAPSE_THRESHOLD = -0.025
EXPECTED_RALLY_DECK_HASH_U64 = 909447583901160127
EXPECTED_OPPONENT_RUN_SHA256 = "2c9b7423004428c0e2bb138afafc15ec65957f6bd98c4587bea704fbf9549aae"
EXPECTED_OPPONENT_CHECKPOINT_SHA256 = "4bd38cf3a9af3fb03fb04428fbc4286d4635007e848c7b9f0740122e430cbba8"
EXPECTED_OPPONENT_PAYLOAD_SHA256 = "a6c87366b2da9fc33923abab3c0e22d70c884cd9420477df3a475117be6beb99"
EXPECTED_OPPONENT_MODEL_SHA256 = "db58dbe3f1f76b5bdf3bae4de657711dc818393b2bf1eeae88c02d8866b4d01d"
EXPECTED_EB_CS_REFERENCE_SHA256 = "ffae17bdc020578a34d7cc420e138951fcb587531cf5191c978384a4bd4b73ef"


class ClassifierError(ValueError):
    """An input or derived-state violation that must fail closed."""


def _is_int(value: Any) -> bool:
    return isinstance(value, int) and not isinstance(value, bool)


def _is_number(value: Any) -> bool:
    return isinstance(value, (int, float)) and not isinstance(value, bool) and math.isfinite(float(value))


def _require(condition: bool, message: str) -> None:
    if not condition:
        raise ClassifierError(message)


def _sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def _canonical_json_sha256(value: Any) -> str:
    data = json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=False).encode("utf-8")
    return hashlib.sha256(data).hexdigest()


def _string(value: Any, label: str) -> str:
    _require(isinstance(value, str) and bool(value), f"{label} must be a nonempty string")
    return value


def _exact_int(value: Any, expected: int, label: str) -> None:
    _require(_is_int(value) and value == expected, f"{label} must equal {expected}, got {value!r}")


def _load_json(path: Path, label: str) -> Any:
    _require(path.is_file(), f"{label} does not exist as a file: {path}")
    try:
        with path.open("r", encoding="utf-8") as stream:
            return json.load(stream)
    except (OSError, json.JSONDecodeError) as exc:
        raise ClassifierError(f"could not read {label} {path}: {exc}") from exc


def _rank_bucket(rank: int) -> str:
    return {1: "wins", 0: "draws", -1: "losses"}[rank]


def _sign(value: int) -> int:
    return 1 if value > 0 else -1 if value < 0 else 0


def _empty_counts() -> Dict[str, Dict[str, int]]:
    return {
        "overall": {"wins": 0, "losses": 0, "draws": 0},
        "P0": {"wins": 0, "losses": 0, "draws": 0},
        "P1": {"wins": 0, "losses": 0, "draws": 0},
    }


def _counts(rows: Sequence[Mapping[str, Any]]) -> Dict[str, Dict[str, int]]:
    result = _empty_counts()
    for row in rows:
        rank = row["terminal_order_rank"]
        bucket = _rank_bucket(rank)
        result["overall"][bucket] += 1
        result[row["learner_seat"]][bucket] += 1
    return result


def _published_counts(artifact: Mapping[str, Any], recomputed: Mapping[str, Mapping[str, int]], label: str) -> None:
    published = artifact.get("learner_outcomes")
    _require(isinstance(published, dict), f"{label}.learner_outcomes must be an object")
    for scope in ("overall", "P0", "P1"):
        _require(isinstance(published.get(scope), dict), f"{label}.learner_outcomes.{scope} missing")
        for bucket in ("wins", "losses", "draws"):
            value = published[scope].get(bucket)
            _require(_is_int(value) and value == recomputed[scope][bucket],
                     f"{label} published {scope}.{bucket} disagrees with terminal rows")


def _direct_stats(counts: Mapping[str, Mapping[str, int]]) -> Dict[str, Any]:
    out: Dict[str, Any] = {}
    for scope in ("overall", "P0", "P1"):
        c = counts[scope]
        games = c["wins"] + c["losses"] + c["draws"]
        _require(games > 0, f"{scope} has no games")
        out[scope] = {
            "wins": c["wins"],
            "losses": c["losses"],
            "draws": c["draws"],
            "games": games,
            "score": (c["wins"] + 0.5 * c["draws"]) / games,
        }
    return out


def _leg_comparison(left: Sequence[Mapping[str, Any]], right: Sequence[Mapping[str, Any]]) -> Dict[str, Any]:
    _require(len(left) == len(right), "paired streams have different row counts")
    result: Dict[str, Dict[str, int]] = {
        scope: {"better": 0, "worse": 0, "tied": 0} for scope in ("overall", "P0", "P1")
    }
    for left_row, right_row in zip(left, right):
        scope = left_row["learner_seat"]
        relation = _sign(left_row["terminal_order_rank"] - right_row["terminal_order_rank"])
        bucket = "better" if relation > 0 else "worse" if relation < 0 else "tied"
        result["overall"][bucket] += 1
        result[scope][bucket] += 1
    for scope in result:
        entry = result[scope]
        entry["net"] = entry["better"] - entry["worse"]
        denominator = entry["better"] + entry["worse"] + entry["tied"]
        entry["effect"] = entry["net"] / denominator
    return result


def _pair_scores(left: Sequence[Mapping[str, Any]], right: Sequence[Mapping[str, Any]]) -> Dict[str, Any]:
    _require(len(left) == len(right) and len(left) % 2 == 0, "paired stream rows must form complete seat-swapped pairs")
    overall: List[float] = []
    by_seat: Dict[str, List[float]] = {"P0": [], "P1": []}
    for index in range(0, len(left), 2):
        legs = []
        for offset in (0, 1):
            left_row = left[index + offset]
            right_row = right[index + offset]
            relation = _sign(left_row["terminal_order_rank"] - right_row["terminal_order_rank"])
            legs.append(relation)
            by_seat[left_row["learner_seat"]].append(float(relation))
        overall.append((legs[0] + legs[1]) / 2.0)

    def summarize(values: Sequence[float]) -> Dict[str, Any]:
        histogram = {"-1": 0, "-0.5": 0, "0": 0, "0.5": 0, "1": 0}
        for value in values:
            key = str(int(value)) if value in (-1.0, 0.0, 1.0) else str(value)
            histogram[key] += 1
        return {
            "clusters": len(values),
            "mean": sum(values) / len(values) if values else None,
            "score_histogram": histogram,
        }

    return {
        "overall": summarize(overall),
        "P0": summarize(by_seat["P0"]),
        "P1": summarize(by_seat["P1"]),
        "overall_scores": overall,
    }


def _row_identity(row: Mapping[str, Any]) -> Tuple[Any, ...]:
    decks = row["deck_hashes_u64"]
    return (row["episode_index"], row["pair_index"], row["environment_seed"], row["learner_seat"], row["opponent_pool_member"], json.dumps(decks, separators=(",", ":")))


def _validate_rows(artifact: Mapping[str, Any], expected_pairs: int, label: str) -> List[Dict[str, Any]]:
    rows = artifact.get("episodes")
    _require(isinstance(rows, list), f"{label}.episodes must be a list")
    _exact_int(len(rows), 2 * expected_pairs, f"{label}.episodes length")
    normalized: List[Dict[str, Any]] = []
    for index, row in enumerate(rows):
        _require(isinstance(row, dict), f"{label}.episodes[{index}] must be an object")
        for key in ("episode_index", "pair_index", "environment_seed", "learner_seat", "deck_hashes_u64", "opponent_pool_member", "terminal_order_rank"):
            _require(key in row, f"{label}.episodes[{index}] missing {key}")
        _exact_int(row["episode_index"], index, f"{label} episode_index[{index}]")
        _exact_int(row["pair_index"], index // 2, f"{label} pair_index[{index}]")
        _require(row["learner_seat"] == ("P0" if index % 2 == 0 else "P1"), f"{label} seat/order mismatch at row {index}")
        _require(_is_int(row["environment_seed"]) and row["environment_seed"] >= 0, f"{label} invalid environment_seed at row {index}")
        _require(row["deck_hashes_u64"] == [EXPECTED_RALLY_DECK_HASH_U64, EXPECTED_RALLY_DECK_HASH_U64],
                 f"{label} is not the frozen Rally mirror at row {index}")
        _require(row["opponent_pool_member"] == "Primary",
                 f"{label} did not retain the pure promoted(2) Primary component at row {index}")
        _require(_is_int(row["terminal_order_rank"]) and row["terminal_order_rank"] in (-1, 0, 1),
                 f"{label} invalid terminal rank at row {index}")
        normalized.append({
            "episode_index": row["episode_index"],
            "pair_index": row["pair_index"],
            "environment_seed": row["environment_seed"],
            "learner_seat": row["learner_seat"],
            "deck_hashes_u64": list(row["deck_hashes_u64"]),
            "opponent_pool_member": row["opponent_pool_member"],
            "terminal_order_rank": row["terminal_order_rank"],
        })
    for pair in range(expected_pairs):
        _require(normalized[2 * pair]["environment_seed"] == normalized[2 * pair + 1]["environment_seed"],
                 f"{label} CRN seat-swapped pair {pair} does not share one environment seed")
    return normalized


def _validate_artifact(path: Path, entry: Mapping[str, Any]) -> Dict[str, Any]:
    label = str(entry.get("id", path))
    artifact = _load_json(path, label)
    _require(isinstance(artifact, dict), f"{label} root must be an object")
    _require(artifact.get("schema") == STREAM_SCHEMA, f"{label} has unexpected stream schema")
    _exact_int(artifact.get("evaluation_base_seed"), EVAL_SEED, f"{label} evaluation_base_seed")
    training_seed = entry["training_seed"]
    generation = entry["generation"]
    pairs = entry["pairs"]
    _exact_int(artifact.get("pair_count"), pairs, f"{label} pair_count")
    _exact_int(artifact.get("episode_count"), 2 * pairs, f"{label} episode_count")
    candidate = artifact.get("candidate")
    opponent = artifact.get("opponent")
    runtime = artifact.get("runtime")
    _require(isinstance(candidate, dict), f"{label}.candidate missing")
    _require(isinstance(opponent, dict), f"{label}.opponent missing")
    _require(isinstance(runtime, dict), f"{label}.runtime missing")
    _exact_int(candidate.get("generation"), generation, f"{label} candidate generation")
    _exact_int(opponent.get("generation"), OPPONENT_GENERATION, f"{label} opponent generation")
    _require(opponent.get("run_sha256") == EXPECTED_OPPONENT_RUN_SHA256,
             f"{label} opponent run is not promoted(2)")
    _require(opponent.get("checkpoint_manifest_sha256") == EXPECTED_OPPONENT_CHECKPOINT_SHA256,
             f"{label} opponent checkpoint is not promoted(2) generation 384")
    _require(opponent.get("checkpoint_payload_sha256") == EXPECTED_OPPONENT_PAYLOAD_SHA256,
             f"{label} opponent payload is not promoted(2) generation 384")
    _require(opponent.get("model_parameter_sha256") == EXPECTED_OPPONENT_MODEL_SHA256,
             f"{label} opponent model is not promoted(2) generation 384")
    _require(runtime.get("environment_randomization_v2") is True, f"{label} is not envrand-v2")
    _require(runtime.get("all_natural") is True, f"{label} contains non-natural terminal completion")
    _exact_int(runtime.get("worker_count"), 2, f"{label} worker_count")
    _exact_int(runtime.get("sessions_per_worker"), 32, f"{label} sessions_per_worker")
    _exact_int(runtime.get("broker_batch_target"), 16, f"{label} broker_batch_target")
    for field in ("run_sha256", "identity_bundle_sha256", "checkpoint_manifest_sha256", "checkpoint_payload_sha256", "model_parameter_sha256"):
        _string(candidate.get(field), f"{label}.candidate.{field}")
    for field in ("run_sha256", "checkpoint_manifest_sha256", "checkpoint_payload_sha256", "model_parameter_sha256"):
        _string(opponent.get(field), f"{label}.opponent.{field}")
    rows = _validate_rows(artifact, pairs, label)
    recomputed = _counts(rows)
    _published_counts(artifact, recomputed, label)
    return {
        "id": label,
        "path": str(path.resolve()),
        "sha256": _sha256_file(path),
        "training_seed": training_seed,
        "arm": entry["arm"],
        "generation": generation,
        "pairs": pairs,
        "rows": rows,
        "counts": recomputed,
        "direct": _direct_stats(recomputed),
        "candidate_identity": {key: candidate[key] for key in ("run_sha256", "identity_bundle_sha256", "generation", "checkpoint_manifest_sha256", "checkpoint_payload_sha256", "model_parameter_sha256")},
        "opponent_identity": {key: opponent[key] for key in ("run_sha256", "generation", "checkpoint_manifest_sha256", "checkpoint_payload_sha256", "model_parameter_sha256")},
        "runtime": {key: runtime[key] for key in ("worker_count", "sessions_per_worker", "broker_batch_target", "environment_randomization_v2", "all_natural")},
        "leg_outcome_hashes": [_canonical_json_sha256(row) for row in rows],
    }


def _validate_request(request: Mapping[str, Any], request_path: Path) -> Dict[Tuple[int, str, int], Dict[str, Any]]:
    _require(request.get("schema") == REQUEST_SCHEMA, "request has an unexpected schema")
    _exact_int(request.get("evaluation_base_seed"), EVAL_SEED, "request evaluation_base_seed")
    entries = request.get("streams")
    _require(isinstance(entries, list), "request streams must be a list")
    _exact_int(len(entries), EXPECTED_STREAM_COUNT, "request stream count")
    seen = set()
    expected_keys = {(seed, "candidate", generation) for seed in SEEDS for generation in ALL_GENS}
    expected_keys |= {(seed, "control", generation) for seed in SEEDS for generation in ENDPOINT_GENS}
    records: Dict[Tuple[int, str, int], Dict[str, Any]] = {}
    for index, entry in enumerate(entries):
        _require(isinstance(entry, dict), f"request streams[{index}] must be an object")
        for key in ("id", "path", "sha256", "arm", "training_seed", "generation", "pairs"):
            _require(key in entry, f"request streams[{index}] missing {key}")
        _string(entry["id"], f"request streams[{index}].id")
        _require(entry["id"] not in seen, f"duplicate stream id {entry['id']!r}")
        seen.add(entry["id"])
        _require(entry["arm"] in ("candidate", "control"), f"invalid arm at streams[{index}]")
        _require(_is_int(entry["training_seed"]) and entry["training_seed"] in SEEDS, f"invalid training seed at streams[{index}]")
        _require(_is_int(entry["generation"]) and entry["generation"] in ALL_GENS, f"invalid generation at streams[{index}]")
        expected_pairs = DIAGNOSTIC_PAIRS if entry["generation"] in DIAGNOSTIC_GENS else ENDPOINT_PAIRS
        _exact_int(entry["pairs"], expected_pairs, f"streams[{index}] pairs")
        key = (entry["training_seed"], entry["arm"], entry["generation"])
        _require(key in expected_keys, f"unexpected stream identity {key}")
        _require(key not in records, f"duplicate stream identity {key}")
        raw_path = Path(entry["path"])
        path = raw_path if raw_path.is_absolute() else request_path.parent / raw_path
        record = _validate_artifact(path, entry)
        _require(entry.get("sha256") == record["sha256"], f"stream SHA-256 mismatch in {entry['id']}")
        records[key] = record
    _require(set(records) == expected_keys, f"request does not enumerate the exact required 21 stream identities")

    reference: Tuple[Any, ...] | None = None
    opponent_identity: Dict[str, Any] | None = None
    runtime_reference: Dict[str, Any] | None = None
    endpoint_records = [record for record in records.values() if record["pairs"] == ENDPOINT_PAIRS]
    _require(endpoint_records, "request has no 2048-pair endpoint stream")
    reference = tuple(_row_identity(row) for row in endpoint_records[0]["rows"])
    for record in records.values():
        rows = record["rows"]
        identities = tuple(_row_identity(row) for row in rows)
        _require(identities == reference[:len(identities)], f"CRN row identity mismatch in {record['id']}")
        if opponent_identity is None:
            opponent_identity = record["opponent_identity"]
        else:
            _require(record["opponent_identity"] == opponent_identity, f"opponent promoted(2) identity mismatch in {record['id']}")
        if runtime_reference is None:
            runtime_reference = record["runtime"]
        else:
            _require(record["runtime"] == runtime_reference, f"frozen runtime binding mismatch in {record['id']}")

    for seed in SEEDS:
        for arm, generations in (("candidate", ALL_GENS), ("control", ENDPOINT_GENS)):
            lineage = [records[(seed, arm, generation)]["candidate_identity"] for generation in generations]
            run_identity = (lineage[0]["run_sha256"], lineage[0]["identity_bundle_sha256"])
            _require(all((identity["run_sha256"], identity["identity_bundle_sha256"]) == run_identity
                         for identity in lineage),
                     f"{arm} seed {seed} streams do not belong to one Store lineage")
        for generation in ENDPOINT_GENS:
            candidate = records[(seed, "candidate", generation)]
            control = records[(seed, "control", generation)]
            _require(candidate["candidate_identity"]["checkpoint_manifest_sha256"] != control["candidate_identity"]["checkpoint_manifest_sha256"] or
                     candidate["candidate_identity"]["model_parameter_sha256"] != control["candidate_identity"]["model_parameter_sha256"],
                     f"candidate and beta-zero control checkpoints are not distinct for seed {seed} generation {generation}")
    return records


def _comparison_report(left: Mapping[str, Any], right: Mapping[str, Any]) -> Dict[str, Any]:
    legs = _leg_comparison(left["rows"], right["rows"])
    clusters = _pair_scores(left["rows"], right["rows"])
    clusters.pop("overall_scores")
    return {"leg_comparison": legs, "cluster_effect": clusters}


def _load_eb_reference(path: Path) -> Any:
    _require(path.is_file(), f"EB-CS reference does not exist: {path}")
    _require(_sha256_file(path) == EXPECTED_EB_CS_REFERENCE_SHA256,
             "EB-CS reference SHA-256 does not match the countersigned V3 implementation")
    spec = importlib.util.spec_from_file_location("eb_cs_reference_v1_for_full_horizon", path)
    _require(spec is not None and spec.loader is not None, "could not load EB-CS reference module")
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    try:
        spec.loader.exec_module(module)
    except Exception as exc:  # noqa: BLE001
        raise ClassifierError(f"could not import EB-CS reference: {exc}") from exc
    for name in ("compute_eb_cs_trajectory", "gate_decision"):
        _require(hasattr(module, name), f"EB-CS reference lacks {name}")
    return module


def _v3_gate(eb: Any, scores: Sequence[float]) -> Dict[str, Any]:
    _require(len(scores) == MAX_N, f"V3 endpoint requires exactly {MAX_N} clusters")
    _require(all(score in (-1.0, -0.5, 0.0, 0.5, 1.0) for score in scores), "V3 score stream is outside the exact five-value alphabet")
    try:
        trajectory = eb.compute_eb_cs_trajectory(scores, alpha=ALPHA, c=C_TRUNCATION)
        decision = eb.gate_decision(
            trajectory,
            max_n=MAX_N,
            delta_promote=DELTA_PROMOTE,
            delta_worthwhile=DELTA_WORTHWHILE,
            gate_class="ACCUMULATION",
        )
    except Exception as exc:  # noqa: BLE001
        raise ClassifierError(f"EB-CS reference rejected the endpoint cluster stream: {exc}") from exc
    endpoint = trajectory[-1]
    return {
        "verdict": decision.verdict,
        "decision_n": decision.decision_n,
        "reason": decision.reason,
        "endpoint": {
            "n": endpoint.n,
            "delta_hat_running": endpoint.delta_hat_running,
            "cs_delta_lower": endpoint.cs_delta_lower,
            "cs_delta_upper": endpoint.cs_delta_upper,
        },
        "parameters": {
            "gate_class": "ACCUMULATION",
            "alpha": ALPHA,
            "c": C_TRUNCATION,
            "delta_worthwhile": DELTA_WORTHWHILE,
            "delta_promote": DELTA_PROMOTE,
            "max_N": MAX_N,
        },
    }


def _parent_drift(request: Mapping[str, Any], disposition: str) -> Dict[str, Any]:
    report = request.get("parent_drift_report")
    _require(isinstance(report, dict), "parent_drift_report must be an object")
    values = report.get("R512")
    _require(isinstance(values, dict), "parent_drift_report.R512 must be an object keyed by training seed")
    normalized: Dict[str, float] = {}
    for seed in SEEDS:
        value = values.get(str(seed), values.get(seed))
        _require(_is_number(value) and math.isfinite(float(value)) and float(value) >= 0.0,
                 f"parent drift R512 for seed {seed} must be finite and nonnegative")
        normalized[str(seed)] = float(value)
    licensed = any(value >= 0.75 for value in normalized.values())
    escalation_available = report.get("escalation_available")
    _require(escalation_available is False, "this campaign must bind no next-larger screen-eligible beta")
    route_eligible = disposition not in ("ADVANCE", "COLLAPSE-NOT-REPRODUCED")
    if not route_eligible:
        status = "NOT-APPLICABLE"
    elif licensed and escalation_available:
        status = "DRIFT-TRIGGER-MET"
    elif licensed:
        status = "DRIFT-TRIGGER-MET-NO-ELIGIBLE-ESCALATION"
    else:
        status = "NO-LATE-DRIFT"
    return {
        "status": status,
        "R512": normalized,
        "late_drift_trigger_met": licensed if route_eligible else False,
        "escalation_available": escalation_available,
        "escalation_unavailable_reason": report.get("escalation_unavailable_reason"),
        "nonclaim": "R512 is only the late-drift trigger. It does not establish that a next-larger coefficient was screen-eligible or available.",
    }


def classify(request: Mapping[str, Any], request_path: Path, eb_path: Path) -> Dict[str, Any]:
    records = _validate_request(request, request_path)
    eb = _load_eb_reference(eb_path)
    by_seed: Dict[str, Any] = {}
    all_v3: Dict[int, Dict[str, Any]] = {}
    collapse_effects: Dict[str, float] = {}
    candidate_endpoint_effects: List[float] = []
    pooled_p1_scores: List[float] = []
    for seed in SEEDS:
        candidate_g512 = records[(seed, "candidate", 512)]
        candidate_g384 = records[(seed, "candidate", 384)]
        control_g512 = records[(seed, "control", 512)]
        control_g384 = records[(seed, "control", 384)]
        candidate_control_384 = _comparison_report(candidate_g384, control_g384)
        candidate_control_512 = _comparison_report(candidate_g512, control_g512)
        candidate_stability = _comparison_report(candidate_g512, candidate_g384)
        control_stability = _comparison_report(control_g512, control_g384)
        v3_scores = _pair_scores(candidate_g512["rows"], control_g512["rows"])["overall_scores"]
        v3 = _v3_gate(eb, v3_scores)
        all_v3[seed] = v3
        collapse_effects[str(seed)] = control_stability["cluster_effect"]["overall"]["mean"]
        candidate_endpoint_effects.append(candidate_control_512["cluster_effect"]["overall"]["mean"])
        for candidate_row, control_row in zip(candidate_g512["rows"], control_g512["rows"]):
            if candidate_row["learner_seat"] == "P1":
                pooled_p1_scores.append(float(_sign(candidate_row["terminal_order_rank"] - control_row["terminal_order_rank"])))
        by_seed[str(seed)] = {
            "candidate_direct": candidate_g512["direct"],
            "control_direct": control_g512["direct"],
            "candidate_minus_control": {"g384": candidate_control_384, "g512": candidate_control_512},
            "candidate_g512_minus_g384": candidate_stability,
            "control_g512_minus_g384": control_stability,
            "v3_accumulation": v3,
            "inputs": {
                "candidate_g384": {key: candidate_g384[key] for key in ("path", "sha256", "leg_outcome_hashes")},
                "candidate_g512": {key: candidate_g512[key] for key in ("path", "sha256", "leg_outcome_hashes")},
                "control_g384": {key: control_g384[key] for key in ("path", "sha256", "leg_outcome_hashes")},
                "control_g512": {key: control_g512[key] for key in ("path", "sha256", "leg_outcome_hashes")},
            },
        }

    collapse_seeds = [seed for seed, effect in collapse_effects.items() if effect <= COLLAPSE_THRESHOLD]
    collapse_reproduced = len(collapse_seeds) >= 2
    v3_successes = [seed for seed in SEEDS if all_v3[seed]["verdict"] == "SUCCESS"]
    non_success = [seed for seed in SEEDS if seed not in v3_successes]
    clause1 = len(v3_successes) >= 2
    clause2 = len(non_success) == 0 or (len(non_success) == 1 and all_v3[non_success[0]]["verdict"] != "HARM" and candidate_endpoint_effects[SEEDS.index(non_success[0])] >= ENDPOINT_TOLERANCE)
    clause3 = all(
        by_seed[str(seed)]["candidate_g512_minus_g384"]["leg_comparison"]["overall"]["net"] >= OVERALL_STABILITY_FLOOR and
        by_seed[str(seed)]["candidate_g512_minus_g384"]["leg_comparison"]["P0"]["net"] >= SEAT_STABILITY_FLOOR and
        by_seed[str(seed)]["candidate_g512_minus_g384"]["leg_comparison"]["P1"]["net"] >= SEAT_STABILITY_FLOOR
        for seed in SEEDS
    )
    pooled_p1_effect = sum(pooled_p1_scores) / len(pooled_p1_scores)
    clause4 = pooled_p1_effect >= ENDPOINT_TOLERANCE
    clause5_candidates = [seed for seed in SEEDS if by_seed[str(seed)]["candidate_direct"]["overall"]["score"] >= 0.50]
    clause5 = bool(clause5_candidates)
    clauses = {
        "two_of_three_v3_success": clause1,
        "remaining_seed_not_harm_and_endpoint_at_least_minus_0_01": clause2,
        "all_candidate_late_stability_floors": clause3,
        "pooled_candidate_P1_effect_at_least_minus_0_01": clause4,
        "at_least_one_candidate_g512_direct_score_at_least_0_50": clause5,
    }
    h4_seeds = []
    for seed in SEEDS:
        stability = by_seed[str(seed)]["candidate_g512_minus_g384"]["leg_comparison"]
        p0 = stability["P0"]["net"]
        p1 = stability["P1"]["net"]
        if p1 < SEAT_STABILITY_FLOOR and p0 >= SEAT_STABILITY_FLOOR and (p1 - p0) < 0:
            h4_seeds.append(seed)
    h4 = len(h4_seeds) >= 2
    if not collapse_reproduced:
        disposition = "COLLAPSE-NOT-REPRODUCED"
    elif all(clauses.values()):
        disposition = "ADVANCE"
    elif clause3:
        disposition = "STABLE-NO-STRENGTH"
    else:
        disposition = "NO-ADVANCEMENT"
    nomination = None
    if disposition == "ADVANCE":
        # Overall advancement licenses the three endpoint policies. Clause 2
        # explicitly admits one non-SUCCESS, non-HARM seed, so the frozen
        # highest-score nomination comparison includes all three.
        nomination_seed = sorted(SEEDS, key=lambda seed: (-by_seed[str(seed)]["candidate_direct"]["overall"]["score"], seed))[0]
        nomination = {
            "training_seed": nomination_seed,
            "generation": 512,
            "fixed_panel_score": by_seed[str(nomination_seed)]["candidate_direct"]["overall"]["score"],
            "tie_break": "lower training seed",
        }
    input_records = []
    for key in sorted(records, key=lambda item: (item[0], item[1], item[2])):
        record = records[key]
        input_records.append({
            "id": record["id"],
            "training_seed": record["training_seed"],
            "arm": record["arm"],
            "generation": record["generation"],
            "pairs": record["pairs"],
            "path": record["path"],
            "sha256": record["sha256"],
            "leg_outcome_hashes": record["leg_outcome_hashes"],
        })
    direct_panel = {
        str(seed): {
            arm: {
                str(generation): records[(seed, arm, generation)]["direct"]
                for generation in (ALL_GENS if arm == "candidate" else ENDPOINT_GENS)
            }
            for arm in ("candidate", "control")
        }
        for seed in SEEDS
    }
    return {
        "schema": OUTPUT_SCHEMA,
        "disposition": disposition,
        "strength_signal": "terminal W/L/D only; terminal order W>D>L",
        "development_only": True,
        "nonclaims": [
            "This classifier does not claim pro-level strength or external tournament strength.",
            "No nonterminal statistic enters a strength, promotion, collapse, or advancement decision.",
            "The V3 reads and selection panel are development evidence and are not formal held-out strength results.",
        ],
        "frozen_parameters": {
            "training_seeds": list(SEEDS),
            "evaluation_base_seed": EVAL_SEED,
            "opponent_generation": OPPONENT_GENERATION,
            "candidate_diagnostic_generations": list(DIAGNOSTIC_GENS),
            "candidate_endpoint_generations": list(ENDPOINT_GENS),
            "diagnostic_pairs": DIAGNOSTIC_PAIRS,
            "endpoint_pairs": ENDPOINT_PAIRS,
            "alpha": ALPHA,
            "c": C_TRUNCATION,
            "delta_worthwhile": DELTA_WORTHWHILE,
            "delta_promote": DELTA_PROMOTE,
            "max_N": MAX_N,
            "collapse_threshold": COLLAPSE_THRESHOLD,
            "stability_floors": {"overall_net": OVERALL_STABILITY_FLOOR, "each_seat_net": SEAT_STABILITY_FLOOR},
        },
        "collapse_reproduction": {
            "threshold": COLLAPSE_THRESHOLD,
            "effects_control_g512_minus_g384": collapse_effects,
            "qualifying_seeds": [int(seed) for seed in collapse_seeds],
            "required_qualifying_seed_count": 2,
            "passed": collapse_reproduced,
            "interpretation": "If false, the causal read is void in both directions; this is not a claim that regularization failed.",
        },
        "per_seed": by_seed,
        "direct_panel": direct_panel,
        "v3_summary": {
            "success_seeds": v3_successes,
            "verdicts": {str(seed): all_v3[seed]["verdict"] for seed in SEEDS},
        },
        "advancement": {
            "clauses": clauses,
            "all_five_pass": all(clauses.values()),
            "causal_prerequisite_and_all_five_pass": collapse_reproduced and all(clauses.values()),
            "pooled_candidate_P1_effect": pooled_p1_effect,
            "candidate_endpoint_effects": {str(seed): candidate_endpoint_effects[index] for index, seed in enumerate(SEEDS)},
        },
        "h4": {
            "mechanical_condition_met": h4,
            "reopens": collapse_reproduced and h4,
            "qualifying_seeds": h4_seeds,
            "nonclaim": "H4 is a route condition only; it is not evidence of improved strength.",
        },
        "nomination": nomination,
        "parent_drift_and_escalation": _parent_drift(request, disposition),
        "classifier_request": {"path": str(request_path.resolve()), "sha256": _sha256_file(request_path)},
        "eb_cs_reference": {"path": str(eb_path.resolve()), "sha256": _sha256_file(eb_path)},
        "inputs": input_records,
    }


def _write_create_new(path: Path, document: Mapping[str, Any]) -> None:
    _require(not path.exists(), f"classifier output already exists: {path}")
    _require(path.parent.is_dir(), f"classifier output parent directory does not exist: {path.parent}")
    data = (json.dumps(document, indent=2, sort_keys=False) + "\n").encode("utf-8")
    try:
        with path.open("xb") as stream:
            stream.write(data)
            stream.flush()
            os.fsync(stream.fileno())
    except FileExistsError as exc:
        raise ClassifierError(f"classifier output was not create-new: {path}") from exc


def _fixture_artifact(seed: int, arm: str, generation: int, pairs: int, ranks: Sequence[int]) -> Dict[str, Any]:
    rows = []
    for index in range(2 * pairs):
        rows.append({
            "episode_index": index,
            "pair_index": index // 2,
            "environment_seed": 500000 + index // 2,
            "learner_seat": "P0" if index % 2 == 0 else "P1",
            "deck_hashes_u64": [EXPECTED_RALLY_DECK_HASH_U64, EXPECTED_RALLY_DECK_HASH_U64],
            "opponent_pool_member": "Primary",
            "terminal_order_rank": ranks[index],
        })
    counts = _counts(rows)
    return {
        "schema": STREAM_SCHEMA,
        "evaluation_base_seed": EVAL_SEED,
        "pair_count": pairs,
        "episode_count": 2 * pairs,
        "candidate": {
            "run_sha256": f"run-{seed}-{arm}",
            "identity_bundle_sha256": f"identity-{seed}-{arm}",
            "generation": generation,
            "checkpoint_manifest_sha256": f"manifest-{seed}-{arm}-{generation}",
            "checkpoint_payload_sha256": f"payload-{seed}-{arm}-{generation}",
            "model_parameter_sha256": f"params-{seed}-{arm}-{generation}",
        },
        "opponent": {
            "run_sha256": EXPECTED_OPPONENT_RUN_SHA256,
            "generation": OPPONENT_GENERATION,
            "checkpoint_manifest_sha256": EXPECTED_OPPONENT_CHECKPOINT_SHA256,
            "checkpoint_payload_sha256": EXPECTED_OPPONENT_PAYLOAD_SHA256,
            "model_parameter_sha256": EXPECTED_OPPONENT_MODEL_SHA256,
        },
        "runtime": {
            "worker_count": 2,
            "sessions_per_worker": 32,
            "broker_batch_target": 16,
            "environment_randomization_v2": True,
            "all_natural": True,
        },
        "learner_outcomes": counts,
        "episodes": rows,
    }


def _run_self_test() -> int:
    """Exercise advancing, collapse-not-reproduced, and CRN mismatch paths."""
    here = Path(__file__).resolve()
    eb_path = Path(r"C:\Users\Jack\IdeaProjects\collab\eb_cs_reference_v1.py")
    with tempfile.TemporaryDirectory(prefix="full-horizon-classifier-test-") as temp:
        root = Path(temp)

        def make_fixture(collapse: bool) -> Path:
            entries = []
            for seed in SEEDS:
                for generation in ALL_GENS:
                    pairs = DIAGNOSTIC_PAIRS if generation in DIAGNOSTIC_GENS else ENDPOINT_PAIRS
                    ranks = [1] * (2 * pairs)
                    path = root / f"candidate-{seed}-{generation}.json"
                    path.write_text(json.dumps(_fixture_artifact(seed, "candidate", generation, pairs, ranks)), encoding="utf-8")
                    entries.append({"id": path.stem, "path": str(path), "sha256": _sha256_file(path), "arm": "candidate", "training_seed": seed, "generation": generation, "pairs": pairs})
                for generation in ENDPOINT_GENS:
                    pairs = ENDPOINT_PAIRS
                    if collapse:
                        ranks = [1] * (2 * pairs)
                    else:
                        ranks = [1] * (2 * pairs) if generation == 384 else [-1] * (2 * pairs)
                    path = root / f"control-{seed}-{generation}.json"
                    path.write_text(json.dumps(_fixture_artifact(seed, "control", generation, pairs, ranks)), encoding="utf-8")
                    entries.append({"id": path.stem, "path": str(path), "sha256": _sha256_file(path), "arm": "control", "training_seed": seed, "generation": generation, "pairs": pairs})
            request = {
                "schema": REQUEST_SCHEMA,
                "evaluation_base_seed": EVAL_SEED,
                "streams": entries,
                "parent_drift_report": {
                    "R512": {str(seed): 0.5 for seed in SEEDS},
                    "escalation_available": False,
                    "escalation_unavailable_reason": "self-test has no larger beta",
                },
            }
            path = root / ("request-collapse.json" if collapse else "request-advance.json")
            path.write_text(json.dumps(request), encoding="utf-8")
            return path

        advancing_request = make_fixture(collapse=False)
        advancing = classify(json.loads(advancing_request.read_text(encoding="utf-8")), advancing_request, eb_path)
        _require(advancing["disposition"] == "ADVANCE", "self-test advancing fixture did not advance")
        _require(advancing["nomination"]["training_seed"] == 970001, "self-test nomination tie-break failed")

        collapse_request = make_fixture(collapse=True)
        collapse = classify(json.loads(collapse_request.read_text(encoding="utf-8")), collapse_request, eb_path)
        _require(collapse["disposition"] == "COLLAPSE-NOT-REPRODUCED", "self-test collapse fixture did not stop closed")

        mismatch_path = root / "candidate-970001-512.json"
        mismatch = json.loads(mismatch_path.read_text(encoding="utf-8"))
        mismatch["episodes"][0]["environment_seed"] += 1
        mismatch_path.write_text(json.dumps(mismatch), encoding="utf-8")
        try:
            classify(json.loads(advancing_request.read_text(encoding="utf-8")), advancing_request, eb_path)
        except ClassifierError as exc:
            _require("CRN" in str(exc), "self-test CRN mismatch reported the wrong failure")
        else:
            raise ClassifierError("self-test CRN mismatch was accepted")

        hash_request = make_fixture(collapse=False)
        hash_mismatch_path = root / "control-970003-384.json"
        hash_mismatch_path.write_text(hash_mismatch_path.read_text(encoding="utf-8") + "\n", encoding="utf-8")
        try:
            classify(json.loads(hash_request.read_text(encoding="utf-8")), hash_request, eb_path)
        except ClassifierError as exc:
            _require("SHA-256" in str(exc), "self-test stream hash mismatch reported the wrong failure")
        else:
            raise ClassifierError("self-test stream hash mismatch was accepted")
    print("SELF-TEST PASS: advancing, collapse-not-reproduced, CRN mismatch, and stream hash mismatch")
    return 0


def main(argv: Sequence[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Fail-closed full-horizon terminal-stream classifier")
    parser.add_argument("--request", help="JSON request enumerating exactly 21 terminal streams")
    parser.add_argument("--output", help="create-new JSON classification output")
    parser.add_argument("--eb-cs-reference", required=False, help="path to eb_cs_reference_v1.py")
    parser.add_argument("--self-test", action="store_true", help="run internal bounded fixtures")
    args = parser.parse_args(argv)
    if args.self_test:
        return _run_self_test()
    if not args.request or not args.output or not args.eb_cs_reference:
        parser.error("--request, --output, and --eb-cs-reference are required unless --self-test is used")
    try:
        request_path = Path(args.request).resolve()
        output_path = Path(args.output).resolve()
        eb_path = Path(args.eb_cs_reference).resolve()
        request = _load_json(request_path, "request")
        _require(isinstance(request, dict), "request root must be an object")
        result = classify(request, request_path, eb_path)
        _write_create_new(output_path, result)
        print(f"CLASSIFICATION {result['disposition']} output={output_path}")
        return 0
    except (ClassifierError, OSError, KeyError, TypeError, ValueError) as exc:
        print(f"FAIL-CLOSED: {exc}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
