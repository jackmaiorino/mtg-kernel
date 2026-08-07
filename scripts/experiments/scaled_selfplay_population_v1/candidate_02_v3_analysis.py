#!/usr/bin/env python3
"""Independent reconstruction and V3 analysis for population candidate 02."""

from __future__ import annotations

import argparse
import dataclasses
import hashlib
import importlib.util
import json
import math
import sys
from collections import Counter
from pathlib import Path
from typing import Any


SPEC_SCHEMA = "scaled-selfplay-candidate-02-v3-spec/v1"
PLAN_SCHEMA = "scaled-selfplay-candidate-02-v3-plan/v2"
RECEIPT_SCHEMA = "scaled-selfplay-candidate-02-v3-chunk-receipt/v1"
ANALYSIS_SCHEMA = "scaled-selfplay-candidate-02-v3-analysis/v2"
VERIFICATION_SCHEMA = "scaled-selfplay-candidate-02-v3-existing-verification/v1"
OUTCOME_SCHEMA = "mtg-kernel-head-to-head-terminal-stream/v1"
PYTHON_REFERENCE_SEED_VERSION = "kernel-python-rl-trainer-sha256-v2"
TRAIN_ENV_NAMESPACE = "train-env"
U63_MAX = (1 << 63) - 1


def require(condition: bool, message: str) -> None:
    if not condition:
        raise ValueError(message)


def no_duplicate_object(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
    value: dict[str, Any] = {}
    for key, item in pairs:
        require(key not in value, f"duplicate JSON key: {key}")
        value[key] = item
    return value


def load_json_strict(path: Path) -> dict[str, Any]:
    with path.open("r", encoding="utf-8-sig") as stream:
        value = json.load(stream, object_pairs_hook=no_duplicate_object)
    require(type(value) is dict, f"JSON root must be an object: {path}")
    return value


def canonical_bytes(value: Any) -> bytes:
    return (
        json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(",", ":"))
        + "\n"
    ).encode("utf-8")


def sha256_bytes(value: bytes) -> str:
    return hashlib.sha256(value).hexdigest()


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def expect_keys(value: dict[str, Any], expected: set[str], label: str) -> None:
    actual = set(value)
    require(actual == expected, f"{label} keys differ: missing={sorted(expected-actual)} extra={sorted(actual-expected)}")


def exact_int(value: Any, label: str, minimum: int | None = None) -> int:
    require(type(value) is int, f"{label} must be an exact integer")
    if minimum is not None:
        require(value >= minimum, f"{label} must be >= {minimum}")
    return value


def exact_number(value: Any, label: str) -> float:
    require(type(value) in (int, float), f"{label} must be a number")
    converted = float(value)
    require(math.isfinite(converted), f"{label} must be finite")
    return converted


def validate_file_record(record: dict[str, Any], label: str, within: Path | None = None) -> Path:
    expect_keys(record, {"path", "bytes", "sha256"}, label)
    require(type(record["path"]) is str, f"{label}.path must be a string")
    exact_int(record["bytes"], f"{label}.bytes", 0)
    require(type(record["sha256"]) is str and len(record["sha256"]) == 64, f"{label}.sha256 is invalid")
    path = Path(record["path"]).resolve()
    require(path.is_file(), f"{label} is missing: {path}")
    if within is not None:
        require(path.is_relative_to(within.resolve()), f"{label} escapes run root")
    require(path.stat().st_size == record["bytes"], f"{label} byte count changed")
    require(sha256_file(path) == record["sha256"], f"{label} hash changed")
    return path


def write_new_json(path: Path, value: dict[str, Any]) -> None:
    import os

    encoded = canonical_bytes(value)
    with path.open("xb") as stream:
        stream.write(encoded)
        stream.flush()
        os.fsync(stream.fileno())


def load_reference(spec: dict[str, Any]):
    reference_path = Path(spec["contract"]["reference_path"]).resolve()
    require(reference_path.is_file(), "V3 reference implementation is missing")
    require(sha256_file(reference_path) == spec["contract"]["reference_sha256"], "V3 reference hash mismatch")
    module_spec = importlib.util.spec_from_file_location("candidate_02_v3_independent_reference", reference_path)
    require(module_spec is not None and module_spec.loader is not None, "cannot load V3 reference")
    module = importlib.util.module_from_spec(module_spec)
    sys.modules[module_spec.name] = module
    module_spec.loader.exec_module(module)
    return module


def chunk_count(spec: dict[str, Any]) -> int:
    maximum = exact_int(spec["max_N"], "max_N", 1)
    size = exact_int(spec["chunk_pair_count"], "chunk_pair_count", 1)
    require(maximum % size == 0, "max_N must be divisible by chunk_pair_count")
    return maximum // size


def chunk_seed(spec: dict[str, Any], mode: str, index: int) -> int:
    return exact_int(spec[mode]["first_evaluation_seed"], f"{mode}.first_evaluation_seed", 0) + index * exact_int(
        spec["evaluation_seed_stride"], "evaluation_seed_stride", 1
    )


def schedule_identifier(spec: dict[str, Any], mode: str, cluster_index: int) -> str:
    chunk_index, pair_index = divmod(cluster_index, exact_int(spec["chunk_pair_count"], "chunk_pair_count", 1))
    candidate = spec["candidate"]
    parent = spec["opponent_and_control"]
    return (
        f"candidate-02-v3/v1;mode={mode};cluster_index={cluster_index};"
        f"evaluation_seed={chunk_seed(spec, mode, chunk_index)};pair_index={pair_index};"
        f"candidate_run={candidate['run_sha256']};candidate_generation={candidate['generation']};"
        f"control_run={parent['run_sha256']};control_generation={parent['generation']};"
        f"opponent_run={parent['run_sha256']};opponent_generation={parent['generation']}"
    )


def schedule_identifiers(spec: dict[str, Any], mode: str) -> list[str]:
    return [schedule_identifier(spec, mode, index) for index in range(exact_int(spec["max_N"], "max_N", 1))]


def append_atom(hasher: Any, tag: str, payload: bytes) -> None:
    tag_bytes = tag.encode("utf-8")
    hasher.update(len(tag_bytes).to_bytes(4, "big"))
    hasher.update(tag_bytes)
    hasher.update(len(payload).to_bytes(8, "big"))
    hasher.update(payload)


def trainer_environment_seed(base_seed: int, pair_index: int) -> int:
    exact_int(base_seed, "evaluation seed", 0)
    exact_int(pair_index, "pair index", 0)
    require(base_seed <= U63_MAX and pair_index <= U63_MAX, "trainer schedule input exceeds u63")
    hasher = hashlib.sha256()
    append_atom(hasher, "version", PYTHON_REFERENCE_SEED_VERSION.encode("utf-8"))
    append_atom(hasher, "namespace", TRAIN_ENV_NAMESPACE.encode("utf-8"))
    for name, value in (("base_seed", base_seed), ("pair_index", pair_index)):
        append_atom(hasher, "field-name", name.encode("utf-8"))
        append_atom(hasher, "u63", value.to_bytes(8, "big"))
    return int.from_bytes(hasher.digest()[:8], "big") & U63_MAX


def validate_spec(path: Path) -> tuple[dict[str, Any], Any]:
    spec = load_json_strict(path)
    require(spec.get("schema") == SPEC_SCHEMA, "unexpected spec schema")
    require(spec["gate_class"] == "LARGE-EFFECT", "gate class changed")
    require(exact_number(spec["alpha"], "alpha") == 0.00875, "alpha changed")
    require(exact_number(spec["c"], "c") == 0.5, "c changed")
    require(exact_number(spec["delta_promote"], "delta_promote") == 0.01, "promotion threshold changed")
    require(exact_number(spec["delta_worthwhile"], "delta_worthwhile") == 0.01, "worthwhile threshold changed")
    require(spec["conditional_mean_stability"] == "IID-MIXTURE", "conditional mean class changed")
    require(spec["joint_component_law"] == "P((Primary,Primary))=1", "joint component law changed")
    chunk_count(spec)
    expected_decks = spec["expected_rally_deck_hashes_u64"]
    require(type(expected_decks) is list and len(expected_decks) == 2, "expected Rally deck binding is invalid")
    for index, value in enumerate(expected_decks):
        exact_int(value, f"expected_rally_deck_hashes_u64[{index}]", 0)
    for contract_name in ("v3", "erratum", "test_vectors", "native_schedule_goldens"):
        contract_path = Path(spec["contract"][f"{contract_name}_path"]).resolve()
        require(contract_path.is_file(), f"{contract_name} contract authority is missing")
        require(
            sha256_file(contract_path) == spec["contract"][f"{contract_name}_sha256"],
            f"{contract_name} contract authority hash mismatch",
        )
    reference = load_reference(spec)
    for mode in ("screen", "initial", "confirmation"):
        identifiers = schedule_identifiers(spec, mode)
        require(
            reference.canonical_ordered_identifier_sha256(identifiers) == spec[mode]["pre_outcome_schedule_sha256"],
            f"{mode} schedule hash mismatch",
        )
    initial_seeds = {chunk_seed(spec, "initial", index) for index in range(chunk_count(spec))}
    confirmation_seeds = {chunk_seed(spec, "confirmation", index) for index in range(chunk_count(spec))}
    require(initial_seeds.isdisjoint(confirmation_seeds), "initial and confirmation schedules overlap")
    for binding_name in ("freshness_manifest", "campaign_ledger"):
        validate_file_record(spec[binding_name], binding_name)
    freshness = load_json_strict(Path(spec["freshness_manifest"]["path"]))
    require(freshness.get("schema") == "scaled-selfplay-candidate-02-v3-freshness/v1", "freshness schema mismatch")
    require(exact_int(freshness["candidate_slot"], "freshness candidate slot") == 2, "freshness slot changed")
    for index, record in enumerate(freshness["nomination_evidence"]):
        validate_file_record(record, f"freshness.nomination_evidence[{index}]")
    for mode, seeds in (("initial", initial_seeds), ("confirmation", confirmation_seeds)):
        require(freshness["formal_schedule_sha256"][mode] == spec[mode]["pre_outcome_schedule_sha256"], "freshness schedule hash mismatch")
        for interval in freshness["excluded_evaluation_seed_intervals"]:
            start = exact_int(interval["start_inclusive"], "freshness interval start", 0)
            end = exact_int(interval["end_inclusive"], "freshness interval end", start)
            require(all(not (start <= seed <= end) for seed in seeds), f"{mode} schedule overlaps revealed interval")
    ledger_text = Path(spec["campaign_ledger"]["path"]).read_text(encoding="utf-8")
    for required_text in (
        "candidate-02-population-g1536-initial",
        "candidate-02-population-g1536-confirm",
        "| candidates | 0.00875 | N | assigned to fixed lineage-970002 generation-1536",
        spec["initial"]["pre_outcome_schedule_sha256"],
        spec["confirmation"]["pre_outcome_schedule_sha256"],
    ):
        require(required_text in ledger_text, "campaign ledger does not bind candidate-02 slot, alpha, and schedules")
    return spec, reference


def validate_identity(actual: dict[str, Any], expected: dict[str, Any], label: str, has_bundle: bool) -> None:
    keys = {"checkpoint_manifest_sha256", "checkpoint_payload_sha256", "generation", "model_parameter_sha256", "run_sha256"}
    if has_bundle:
        keys.add("identity_bundle_sha256")
    expect_keys(actual, keys, label)
    mapping = {
        "checkpoint_manifest_sha256": "checkpoint_sha256",
        "checkpoint_payload_sha256": "state_sha256",
        "generation": "generation",
        "model_parameter_sha256": "model_parameter_sha256",
        "run_sha256": "run_sha256",
    }
    for actual_key, expected_key in mapping.items():
        require(actual[actual_key] == expected[expected_key], f"{label}.{actual_key} mismatch")
    if has_bundle:
        require(actual["identity_bundle_sha256"] == expected["identity_bundle_sha256"], f"{label}.identity bundle mismatch")


def validate_count_object(value: dict[str, Any], label: str) -> dict[str, int]:
    expect_keys(value, {"wins", "losses", "draws"}, label)
    return {key: exact_int(value[key], f"{label}.{key}", 0) for key in ("wins", "losses", "draws")}


def validate_outcome(
    path: Path,
    spec: dict[str, Any],
    mode: str,
    chunk_index: int,
    arm_name: str,
) -> list[dict[str, Any]]:
    outcome = load_json_strict(path)
    expect_keys(
        outcome,
        {"candidate", "episode_count", "episodes", "evaluation_base_seed", "learner_outcomes", "opponent", "pair_count", "runtime", "schema"},
        f"{arm_name} outcome",
    )
    require(outcome["schema"] == OUTCOME_SCHEMA, f"{arm_name} outcome schema mismatch")
    pair_count = exact_int(spec["chunk_pair_count"], "chunk_pair_count", 1)
    evaluation_seed = chunk_seed(spec, mode, chunk_index)
    require(exact_int(outcome["evaluation_base_seed"], "evaluation_base_seed") == evaluation_seed, "wrong evaluation seed")
    require(exact_int(outcome["pair_count"], "pair_count") == pair_count, "wrong pair count")
    require(exact_int(outcome["episode_count"], "episode_count") == pair_count * 2, "wrong episode count")
    runtime = outcome["runtime"]
    expect_keys(runtime, {"all_natural", "broker_batch_target", "environment_randomization_v2", "sessions_per_worker", "worker_count"}, "runtime")
    require(runtime["all_natural"] is True and runtime["environment_randomization_v2"] is True, "runtime is not natural envrand-v2")
    for key in ("broker_batch_target", "sessions_per_worker", "worker_count"):
        exact_int(runtime[key], f"runtime.{key}", 1)
    parent = spec["opponent_and_control"]
    candidate = spec["candidate"] if arm_name == "candidate" else parent
    validate_identity(outcome["candidate"], candidate, f"{arm_name}.candidate", True)
    validate_identity(outcome["opponent"], parent, f"{arm_name}.opponent", False)
    rows = outcome["episodes"]
    require(type(rows) is list and len(rows) == pair_count * 2, "episode list length mismatch")
    expected_decks = spec["expected_rally_deck_hashes_u64"]
    seat_counts = {"P0": {"wins": 0, "losses": 0, "draws": 0}, "P1": {"wins": 0, "losses": 0, "draws": 0}}
    for episode_index, row in enumerate(rows):
        require(type(row) is dict, f"episode {episode_index} must be an object")
        expect_keys(row, {"deck_hashes_u64", "environment_seed", "episode_index", "learner_seat", "opponent_pool_member", "pair_index", "terminal_order_rank"}, f"episode {episode_index}")
        pair_index, leg_index = divmod(episode_index, 2)
        seat = "P0" if leg_index == 0 else "P1"
        require(exact_int(row["episode_index"], f"episode {episode_index}.episode_index") == episode_index, "episode row order changed")
        require(exact_int(row["pair_index"], f"episode {episode_index}.pair_index") == pair_index, "pair index mismatch")
        require(type(row["learner_seat"]) is str and row["learner_seat"] == seat, "seat order mismatch")
        require(row["opponent_pool_member"] == "Primary", "opponent component mismatch")
        require(type(row["deck_hashes_u64"]) is list and row["deck_hashes_u64"] == expected_decks, "Rally deck hash mismatch")
        expected_seed = trainer_environment_seed(evaluation_seed, pair_index)
        require(exact_int(row["environment_seed"], f"episode {episode_index}.environment_seed") == expected_seed, "derived environment seed mismatch")
        rank = exact_int(row["terminal_order_rank"], f"episode {episode_index}.terminal_order_rank")
        require(rank in (-1, 0, 1), "terminal rank is outside {-1,0,1}")
        seat_counts[seat]["wins" if rank == 1 else "losses" if rank == -1 else "draws"] += 1
    outcomes = outcome["learner_outcomes"]
    expect_keys(outcomes, {"P0", "P1", "overall"}, "learner_outcomes")
    reported_p0 = validate_count_object(outcomes["P0"], "learner_outcomes.P0")
    reported_p1 = validate_count_object(outcomes["P1"], "learner_outcomes.P1")
    reported_overall = validate_count_object(outcomes["overall"], "learner_outcomes.overall")
    require(reported_p0 == seat_counts["P0"] and reported_p1 == seat_counts["P1"], "seat W/L/D summary mismatch")
    combined = {key: seat_counts["P0"][key] + seat_counts["P1"][key] for key in seat_counts["P0"]}
    require(reported_overall == combined, "overall W/L/D summary mismatch")
    return rows


def normalize_arm(record: dict[str, Any], label: str, run_root: Path) -> tuple[dict[str, Any], Path]:
    expect_keys(record, {"label", "candidate_index", "opponent_index", "pair_count", "evaluation_seed", "exit_code", "wall_seconds", "stdout", "stderr", "outcome"}, label)
    require(type(record["label"]) is str, f"{label}.label must be a string")
    for key in ("candidate_index", "opponent_index", "pair_count", "evaluation_seed", "exit_code"):
        exact_int(record[key], f"{label}.{key}", 0)
    exact_number(record["wall_seconds"], f"{label}.wall_seconds")
    stdout = validate_file_record(record["stdout"], f"{label}.stdout", run_root)
    stderr = validate_file_record(record["stderr"], f"{label}.stderr", run_root)
    outcome = validate_file_record(record["outcome"], f"{label}.outcome", run_root)
    require(record["exit_code"] == 0, f"{label} exited nonzero")
    require(stderr.stat().st_size == 0, f"{label} stderr is nonempty")
    return record, outcome


def validate_plan(plan_path: Path, spec_path: Path, spec: dict[str, Any], mode: str, run_root: Path) -> dict[str, Any]:
    plan = load_json_strict(plan_path)
    required = {
        "schema", "mode", "gate_id", "git", "toolchain", "executable", "spec", "candidate", "opponent_and_control",
        "gpu_ordinal", "terminal_reward_only", "screen", "countersign", "initial_verification", "gate", "joint_component_law",
        "pre_outcome_schedule_sha256", "first_evaluation_seed", "evaluation_seed_stride", "arm_order",
        "expected_rally_deck_hashes_u64", "chunk_plan",
    }
    expect_keys(plan, required, "gate plan")
    require(plan["schema"] == PLAN_SCHEMA and plan["mode"] == mode, "gate plan identity mismatch")
    require(plan["gate_id"] == spec[mode]["gate_id"], "gate id mismatch")
    require(plan["terminal_reward_only"] is True, "terminal-only declaration missing")
    require(plan["candidate"] == spec["candidate"] and plan["opponent_and_control"] == spec["opponent_and_control"], "plan policy identity mismatch")
    require(plan["joint_component_law"] == spec["joint_component_law"], "plan component law mismatch")
    require(plan["pre_outcome_schedule_sha256"] == spec[mode]["pre_outcome_schedule_sha256"], "plan schedule hash mismatch")
    require(plan["expected_rally_deck_hashes_u64"] == spec["expected_rally_deck_hashes_u64"], "plan Rally deck mismatch")
    expected_gate = {
        key: spec[key]
        for key in (
            "gate_class", "alpha", "c", "delta_promote", "delta_worthwhile",
            "max_N", "chunk_pair_count", "concurrent_chunks", "conditional_mean_stability",
        )
    }
    require(plan["gate"] == expected_gate, "plan gate parameters changed")
    require(plan["first_evaluation_seed"] == spec[mode]["first_evaluation_seed"], "plan first evaluation seed changed")
    require(plan["evaluation_seed_stride"] == spec["evaluation_seed_stride"], "plan evaluation seed stride changed")
    require(plan["executable"]["sha256"] == spec["executable"]["sha256"], "plan executable hash mismatch")
    require(plan["executable"]["source_commit"] == spec["executable"]["source_commit"], "plan executable source commit mismatch")
    spec_record_path = validate_file_record(plan["spec"], "plan.spec")
    require(spec_record_path == spec_path.resolve(), "plan points to a different spec")
    if mode == "screen":
        require(plan["screen"] is None and plan["countersign"] is None, "screen plan cannot claim formal release records")
    else:
        validate_file_record(plan["screen"], "plan.screen")
        validate_file_record(plan["countersign"], "plan.countersign")
    if mode in ("screen", "initial"):
        require(plan["initial_verification"] is None, "initial plan unexpectedly has prior-gate verification")
    else:
        require(type(plan["initial_verification"]) is dict, "confirmation lacks independent initial verification")
        validation_path = validate_file_record(plan["initial_verification"], "plan.initial_verification")
        validation = load_json_strict(validation_path)
        require(validation.get("schema") == VERIFICATION_SCHEMA and validation.get("decision") == "VERIFIED-SUCCESS", "initial verification is not a verified success")
    chunks = plan["chunk_plan"]
    require(type(chunks) is list and len(chunks) == chunk_count(spec), "chunk plan size mismatch")
    for index, chunk in enumerate(chunks):
        expect_keys(chunk, {"chunk_index", "evaluation_seed", "global_cluster_start", "global_cluster_end_exclusive"}, f"chunk_plan[{index}]")
        require(exact_int(chunk["chunk_index"], "planned chunk index") == index, "chunk plan order changed")
        require(exact_int(chunk["evaluation_seed"], "planned evaluation seed") == chunk_seed(spec, mode, index), "planned evaluation seed mismatch")
        start = index * spec["chunk_pair_count"]
        require(chunk["global_cluster_start"] == start and chunk["global_cluster_end_exclusive"] == start + spec["chunk_pair_count"], "planned cluster interval mismatch")
    return plan


def leg_count_template() -> dict[str, dict[str, int]]:
    return {seat: {"favorable": 0, "tied": 0, "unfavorable": 0} for seat in ("P0", "P1")}


def count_legs(legs: list[tuple[int, int]]) -> tuple[dict[str, int], dict[str, dict[str, int]]]:
    by_seat = leg_count_template()
    overall = {"favorable": 0, "tied": 0, "unfavorable": 0}
    for pair in legs:
        for seat, score in zip(("P0", "P1"), pair, strict=True):
            label = "favorable" if score > 0 else "unfavorable" if score < 0 else "tied"
            by_seat[seat][label] += 1
            overall[label] += 1
    return overall, by_seat


def reconstruct(run_root: Path, spec_path: Path, mode: str) -> tuple[dict[str, Any], Any, list[float], list[tuple[int, int]], list[str], list[dict[str, Any]], dict[str, Any]]:
    run_root = run_root.resolve()
    spec, reference = validate_spec(spec_path.resolve())
    require(mode in ("screen", "initial", "confirmation"), "invalid mode")
    plan_path = run_root / "gate-plan.json"
    require(plan_path.is_file(), "gate plan is missing")
    plan = validate_plan(plan_path, spec_path, spec, mode, run_root)
    receipt_paths = sorted(run_root.glob("chunk-*-receipt.json"))
    require(receipt_paths, "no durable chunk receipts found")
    scores: list[float] = []
    leg_scores: list[tuple[int, int]] = []
    identifiers: list[str] = []
    raw_authorities: list[dict[str, Any]] = []
    receipt_authorities: list[dict[str, Any]] = []
    seen_pair_seeds: set[int] = set()
    for expected_index, receipt_path in enumerate(receipt_paths):
        require(receipt_path.name == f"chunk-{expected_index:03d}-receipt.json", "chunk receipts are not a contiguous ordered prefix")
        receipt = load_json_strict(receipt_path)
        expect_keys(receipt, {"schema", "chunk_index", "evaluation_seed", "candidate_arm", "control_arm"}, f"chunk receipt {expected_index}")
        require(receipt["schema"] == RECEIPT_SCHEMA, "chunk receipt schema mismatch")
        require(exact_int(receipt["chunk_index"], "receipt chunk index") == expected_index, "receipt chunk index mismatch")
        evaluation_seed = chunk_seed(spec, mode, expected_index)
        require(exact_int(receipt["evaluation_seed"], "receipt evaluation seed") == evaluation_seed, "receipt evaluation seed mismatch")
        candidate_record, candidate_path = normalize_arm(receipt["candidate_arm"], "candidate arm", run_root)
        control_record, control_path = normalize_arm(receipt["control_arm"], "control arm", run_root)
        for arm_name, record, expected_candidate_index in (("candidate", candidate_record, 0), ("control", control_record, 1)):
            require(record["candidate_index"] == expected_candidate_index and record["opponent_index"] == 1, f"{arm_name} arm role changed")
            require(record["pair_count"] == spec["chunk_pair_count"] and record["evaluation_seed"] == evaluation_seed, f"{arm_name} arm schedule changed")
            require(record["label"] == f"chunk-{expected_index:03d}-{arm_name}", f"{arm_name} arm label changed")
        candidate_rows = validate_outcome(candidate_path, spec, mode, expected_index, "candidate")
        control_rows = validate_outcome(control_path, spec, mode, expected_index, "control")
        for pair_index in range(spec["chunk_pair_count"]):
            pair_legs: list[int] = []
            expected_seed = trainer_environment_seed(evaluation_seed, pair_index)
            require(expected_seed not in seen_pair_seeds, "environment seed reused across acquired clusters")
            seen_pair_seeds.add(expected_seed)
            for leg_index in range(2):
                row_index = pair_index * 2 + leg_index
                candidate_row = candidate_rows[row_index]
                control_row = control_rows[row_index]
                for key in ("episode_index", "pair_index", "environment_seed", "learner_seat", "deck_hashes_u64", "opponent_pool_member"):
                    require(candidate_row[key] == control_row[key], f"candidate/control CRN mismatch at chunk {expected_index} row {row_index}")
                delta = candidate_row["terminal_order_rank"] - control_row["terminal_order_rank"]
                pair_legs.append((delta > 0) - (delta < 0))
            leg_pair = (pair_legs[0], pair_legs[1])
            leg_scores.append(leg_pair)
            scores.append((leg_pair[0] + leg_pair[1]) / 2.0)
            identifiers.append(schedule_identifier(spec, mode, expected_index * spec["chunk_pair_count"] + pair_index))
        raw_authorities.extend(
            [
                {"chunk_index": expected_index, "arm": "candidate", **candidate_record["outcome"]},
                {"chunk_index": expected_index, "arm": "control", **control_record["outcome"]},
            ]
        )
        receipt_authorities.append({"chunk_index": expected_index, "path": str(receipt_path.resolve()), "bytes": receipt_path.stat().st_size, "sha256": sha256_file(receipt_path)})
    require(len(scores) <= spec["max_N"], "acquired clusters exceed max_N")
    expected_prefix = schedule_identifiers(spec, mode)[: len(scores)]
    require(identifiers == expected_prefix, "acquired identifiers are not the frozen prefix")
    authorities = {"plan": {"path": str(plan_path.resolve()), "bytes": plan_path.stat().st_size, "sha256": sha256_file(plan_path)}, "receipts": receipt_authorities}
    return spec, reference, scores, leg_scores, identifiers, raw_authorities, authorities


def trajectory_records(trajectory: list[Any], full: bool) -> list[dict[str, Any]]:
    selected = trajectory if full else trajectory[-1:]
    return [dataclasses.asdict(point) for point in selected]


def build_analysis(run_root: Path, spec_path: Path, mode: str, full_trajectory: bool) -> dict[str, Any]:
    spec, reference, scores, leg_scores, identifiers, raw_authorities, authorities = reconstruct(run_root, spec_path, mode)
    trajectory = reference.compute_eb_cs_trajectory(scores, alpha=spec["alpha"], c=spec["c"])
    decision = reference.gate_decision(
        trajectory,
        max_n=spec["max_N"],
        delta_promote=spec["delta_promote"],
        delta_worthwhile=spec["delta_worthwhile"],
        gate_class=spec["gate_class"],
    )
    require(decision.decision_n is not None, "V3 reference returned no decision N")
    decision_n = decision.decision_n
    point = trajectory[decision_n - 1]
    decision_scores = scores[:decision_n]
    acquired_overall, acquired_by_seat = count_legs(leg_scores)
    decision_overall, decision_by_seat = count_legs(leg_scores[:decision_n])
    records = trajectory_records(trajectory, full_trajectory)
    inferential_core = {
        "mode": mode,
        "gate_id": spec[mode]["gate_id"],
        "decision": decision.verdict,
        "decision_N": decision_n,
        "acquired_N": len(scores),
        "delta_hat": math.fsum(decision_scores) / decision_n,
        "cs_delta_lower": point.cs_delta_lower,
        "cs_delta_upper": point.cs_delta_upper,
        "acquired_stream_sha256": reference.canonical_stream_sha256(scores),
        "decision_prefix_stream_sha256": reference.canonical_stream_sha256(decision_scores),
        "acquired_schedule_prefix_sha256": reference.canonical_ordered_identifier_sha256(identifiers),
        "score_counts_at_acquired_N": dict(sorted(Counter(str(score) for score in scores).items())),
        "score_counts_at_decision_N": dict(sorted(Counter(str(score) for score in decision_scores).items())),
        "leg_counts_at_acquired_N": acquired_overall,
        "leg_counts_by_seat_at_acquired_N": acquired_by_seat,
        "leg_counts_at_decision_N": decision_overall,
        "leg_counts_by_seat_at_decision_N": decision_by_seat,
    }
    return {
        "schema": ANALYSIS_SCHEMA,
        "gate_id": spec[mode]["gate_id"],
        "mode": mode,
        "gate_class": spec["gate_class"],
        "alpha": spec["alpha"],
        "c": spec["c"],
        "delta_promote": spec["delta_promote"],
        "delta_worthwhile": spec["delta_worthwhile"],
        "max_N": spec["max_N"],
        "acquired_N": len(scores),
        "decision_N": decision_n,
        "post_decision_acquired_clusters_excluded": len(scores) - decision_n,
        "decision": decision.verdict,
        "decision_reason": decision.reason,
        "delta_hat": inferential_core["delta_hat"],
        "cs_delta_lower": point.cs_delta_lower,
        "cs_delta_upper": point.cs_delta_upper,
        "score_counts_at_acquired_N": inferential_core["score_counts_at_acquired_N"],
        "score_counts_at_decision_N": inferential_core["score_counts_at_decision_N"],
        "leg_counts_at_acquired_N": acquired_overall,
        "leg_counts_by_seat_at_acquired_N": acquired_by_seat,
        "leg_counts_at_decision_N": decision_overall,
        "leg_counts_by_seat_at_decision_N": decision_by_seat,
        "pre_outcome_schedule_sha256": spec[mode]["pre_outcome_schedule_sha256"],
        "acquired_schedule_prefix_sha256": inferential_core["acquired_schedule_prefix_sha256"],
        "acquired_stream_sha256": inferential_core["acquired_stream_sha256"],
        "decision_prefix_stream_sha256": inferential_core["decision_prefix_stream_sha256"],
        "trajectory_complete": full_trajectory,
        "trajectory_records": records,
        "trajectory_records_sha256": sha256_bytes(canonical_bytes(records)),
        "inferential_core_sha256": sha256_bytes(canonical_bytes(inferential_core)),
        "raw_authorities": raw_authorities,
        "analysis_authorities": authorities,
        "terminal_reward_only": True,
        "opponent_component": "Primary",
    }


def verify_existing(run_root: Path, spec_path: Path, retained_path: Path, output: Path) -> None:
    retained = load_json_strict(retained_path.resolve())
    require(retained.get("schema") == ANALYSIS_SCHEMA, "retained analysis schema mismatch")
    require(retained.get("trajectory_complete") is True, "retained analysis lacks the full trajectory")
    mode = retained.get("mode")
    require(mode == "initial", "only an initial gate can authorize confirmation")
    recomputed = build_analysis(run_root.resolve(), spec_path.resolve(), mode, True)
    recomputed_bytes = canonical_bytes(recomputed)
    retained_bytes = retained_path.resolve().read_bytes()
    require(recomputed_bytes == retained_bytes, "recomputed initial analysis differs from retained analysis")
    require(recomputed["decision"] == "SUCCESS", "recomputed initial gate did not succeed")
    result = {
        "schema": VERIFICATION_SCHEMA,
        "decision": "VERIFIED-SUCCESS",
        "initial_run_root": str(run_root.resolve()),
        "retained_analysis": {"path": str(retained_path.resolve()), "bytes": len(retained_bytes), "sha256": sha256_bytes(retained_bytes)},
        "recomputed_analysis_sha256": sha256_bytes(recomputed_bytes),
        "raw_authority_count": len(recomputed["raw_authorities"]),
        "receipt_count": len(recomputed["analysis_authorities"]["receipts"]),
        "decision_N": recomputed["decision_N"],
        "acquired_stream_sha256": recomputed["acquired_stream_sha256"],
        "decision_prefix_stream_sha256": recomputed["decision_prefix_stream_sha256"],
    }
    write_new_json(output.resolve(), result)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    subparsers = parser.add_subparsers(dest="command", required=True)
    analyze_parser = subparsers.add_parser("analyze")
    analyze_parser.add_argument("--run-root", required=True, type=Path)
    analyze_parser.add_argument("--spec", required=True, type=Path)
    analyze_parser.add_argument("--mode", choices=("screen", "initial", "confirmation"), required=True)
    analyze_parser.add_argument("--output", required=True, type=Path)
    analyze_parser.add_argument("--trajectory", choices=("full", "endpoint"), default="full")
    verify_parser = subparsers.add_parser("verify-existing")
    verify_parser.add_argument("--run-root", required=True, type=Path)
    verify_parser.add_argument("--spec", required=True, type=Path)
    verify_parser.add_argument("--retained-analysis", required=True, type=Path)
    verify_parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    if args.command == "analyze":
        analysis = build_analysis(args.run_root, args.spec, args.mode, args.trajectory == "full")
        write_new_json(args.output.resolve(), analysis)
        print(args.output.resolve())
    else:
        verify_existing(args.run_root, args.spec, args.retained_analysis, args.output)
        print(args.output.resolve())
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
