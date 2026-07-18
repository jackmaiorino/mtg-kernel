"""Generate frozen, privacy-safe Decimal oracle vectors for fast sampler V1."""

from __future__ import annotations

import argparse
from decimal import (
    Context,
    Decimal,
    DivisionByZero,
    InvalidOperation,
    Overflow,
    ROUND_HALF_EVEN,
    localcontext,
)
import hashlib
import json
from pathlib import Path
import struct
import sys
from typing import Iterable


ORACLE_VERSION = "decimal-softmax-hamilton-splitmix64-v1"
WIDTH_EVIDENCE_SCHEMA = "kernel_rally_all_policy_legal_action_width_histogram/v1"
WIDTH_EVIDENCE_SCOPE = "all_sampled_policy_decisions_in_rally_vs_rally_not_learner_only"
EMPTY_SHA256 = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
RALLY_RAW_SOURCE_ARTIFACT_SHA256 = "682198c7e169a67a2c885dd8362db0c67c329b8cb1e6390f4fbc905c3f9bd7ee"
RALLY_RAW_SOURCE_ARTIFACT_SIZE_BYTES = 64_453
RALLY_SOURCE_HEAD = "d71dca82dfe36292328ecbc4962a0d6764d9ca5c"
RALLY_SOURCE_MANIFEST_FILE_COUNT = 132
RALLY_SOURCE_MANIFEST_SHA256 = "09e816949de05d76cf37148e015eb973b4f6568e256e755e5b727480df56d9d3"
RALLY_ENVIRONMENT_BINARY_SHA256 = "b81b5ad88e6f728922b8635405aead28588066b2563cdd9644439100715d4c51"
RALLY_BENCHMARK_BINARY_SHA256 = "04802ed2cb953b6ef0f071f42304221de16fd9f411b8decc025ffbfa56b1fbe8"
SOURCE_SHA256_ROLES = {
    "benchmark_rust_source",
    "benchmark_wrapper",
    "feature_encoder",
    "kernel_cargo_manifest",
    "python_package_init",
    "python_project_manifest",
    "rl_client",
    "workspace_cargo_lock",
    "workspace_cargo_manifest",
}
KERNEL_CONTRACT_FIELDS = {
    "card_db_hash",
    "kernel_version",
    "policy_surface_version",
    "protocol",
    "protocol_version",
    "schema_version",
    "surface_version",
}
FEATURE_CONTRACT_FIELDS = {
    "action_feature_dim",
    "action_ref_feature_dim",
    "contract_digest",
    "edge_feature_dim",
    "encoding_digest",
    "object_feature_dim",
    "object_group_count",
    "registry_version",
    "state_dim",
    "version",
}
OUTPUT_RELATIVE = Path("data/fast_sampler_decimal_oracle_v1.json")
REPOSITORY_ROOT = Path(__file__).resolve().parents[2]
OUTPUT = REPOSITORY_ROOT / OUTPUT_RELATIVE
MASS_TOTAL = 1 << 64
MASK64 = MASS_TOTAL - 1
EXP_CUTOFF = Decimal("-128")
SELECTION_SEED_START = 0
SELECTION_SEED_END = 4096
PROVISIONAL_SYNTHETIC_HISTOGRAM = (
    (1, 8),
    (2, 20),
    (3, 20),
    (4, 15),
    (5, 14),
    (6, 8),
    (7, 5),
    (8, 3),
    (9, 2),
    (10, 3),
    (12, 1),
    (15, 1),
)


def decimal_context(precision: int) -> Context:
    return Context(
        prec=precision,
        rounding=ROUND_HALF_EVEN,
        Emin=-999_999,
        Emax=999_999,
        capitals=1,
        clamp=0,
        flags=[],
        traps=[InvalidOperation, DivisionByZero, Overflow],
    )


DELTA_CONTEXT = decimal_context(256)
EXP_CONTEXT = decimal_context(80)


def canonical_json_bytes(value: object) -> bytes:
    return json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=True).encode("utf-8")


def strict_json_loads(value: bytes) -> object:
    def object_pairs(pairs: list[tuple[str, object]]) -> dict[str, object]:
        output: dict[str, object] = {}
        for key, item in pairs:
            if key in output:
                raise ValueError(f"duplicate JSON key: {key}")
            output[key] = item
        return output

    def reject_constant(value: str) -> object:
        raise ValueError(f"nonfinite JSON number is forbidden: {value}")

    return json.loads(value, object_pairs_hook=object_pairs, parse_constant=reject_constant)


def sha256_hex(value: bytes) -> str:
    return hashlib.sha256(value).hexdigest()


def is_json_integer(value: object, *, minimum: int = 0, maximum: int | None = None) -> bool:
    return (
        type(value) is int
        and value >= minimum
        and (maximum is None or value <= maximum)
    )


def is_lower_sha256(value: object) -> bool:
    return (
        isinstance(value, str)
        and len(value) == 64
        and all(character in "0123456789abcdef" for character in value)
    )


def require_exact_object(value: object, fields: set[str], label: str) -> dict[str, object]:
    if not isinstance(value, dict) or set(value) != fields:
        raise ValueError(f"width evidence {label} has an invalid shape")
    return value


def parse_repo_relative_path(raw: str) -> Path:
    if (
        not raw
        or raw.startswith("/")
        or "\\" in raw
        or ":" in raw
        or any(
            part in {"", ".", ".."} or part.casefold() == ".git"
            for part in raw.split("/")
        )
    ):
        raise ValueError("evidence path must be a nonempty repo-relative slash path")
    path = Path(raw)
    if path.is_absolute():
        raise ValueError("evidence path must stay within the repository")
    resolved = (REPOSITORY_ROOT / path).resolve()
    try:
        resolved.relative_to(REPOSITORY_ROOT)
    except ValueError as error:
        raise ValueError("evidence path must stay within the repository") from error
    return path


def bits_to_f32(bits: int) -> float:
    return struct.unpack("<f", struct.pack("<I", bits))[0]


def f32_bits(value: float) -> int:
    return struct.unpack("<I", struct.pack("<f", value))[0]


def decimal_coefficient(value: Decimal) -> tuple[int, int]:
    sign, digits, exponent = value.as_tuple()
    if sign:
        raise ValueError("oracle weight must be non-negative")
    coefficient = 0
    for digit in digits:
        coefficient = coefficient * 10 + digit
    return coefficient, int(exponent)


def oracle_masses(logit_bits: Iterable[int]) -> list[int]:
    exact_logits = [Decimal.from_float(bits_to_f32(bits)) for bits in logit_bits]
    maximum = max(exact_logits)
    with localcontext(DELTA_CONTEXT):
        deltas = [value - maximum for value in exact_logits]
    with localcontext(EXP_CONTEXT):
        weights = [Decimal(0) if delta < EXP_CUTOFF else delta.exp() for delta in deltas]
    coefficients = [decimal_coefficient(weight) for weight in weights]
    minimum_exponent = min(exponent for coefficient, exponent in coefficients if coefficient)
    exact_weights = [
        coefficient * (10 ** (exponent - minimum_exponent)) if coefficient else 0
        for coefficient, exponent in coefficients
    ]
    total = sum(exact_weights)
    apportioned: list[int] = []
    remainders: list[int] = []
    for weight in exact_weights:
        quotient, remainder = divmod(weight * MASS_TOTAL, total)
        apportioned.append(quotient)
        remainders.append(remainder)
    residual = MASS_TOTAL - sum(apportioned)
    for index in sorted(range(len(apportioned)), key=lambda item: (-remainders[item], item))[:residual]:
        apportioned[index] += 1
    if sum(apportioned) != MASS_TOTAL:
        raise ValueError("Decimal oracle failed to preserve exact mass")
    return apportioned


def splitmix64_first(seed: int) -> int:
    mixed = (seed + 0x9E37_79B9_7F4A_7C15) & MASK64
    mixed = ((mixed ^ (mixed >> 30)) * 0xBF58_476D_1CE4_E5B9) & MASK64
    mixed = ((mixed ^ (mixed >> 27)) * 0x94D0_49BB_1331_11EB) & MASK64
    return (mixed ^ (mixed >> 31)) & MASK64


def select_mass(masses: Iterable[int], draw: int) -> int:
    cumulative = 0
    for index, mass in enumerate(masses):
        cumulative += mass
        if draw < cumulative:
            return index
    raise ValueError("Python oracle inverse CDF was not total")


def representative_logits(width: int) -> list[int]:
    values = []
    for index in range(width):
        numerator = ((index * 37 + width * 11) % 97) - 48
        perturbation = (((index * 19 + width * 7) % 7) - 3) / 1024.0
        values.append(f32_bits(numerator / 29.0 + perturbation))
    return values


def make_case(
    name: str,
    classification: str,
    bits: Iterable[int],
    width_profile_count: int = 0,
) -> dict[str, object]:
    frozen_bits = list(bits)
    return {
        "name": name,
        "classification": classification,
        "width_profile_count": width_profile_count,
        "logit_bits_hex": [f"{value:08x}" for value in frozen_bits],
        "decimal_mass": [str(value) for value in oracle_masses(frozen_bits)],
    }


def histogram_statistics(histogram: list[tuple[int, int]]) -> dict[str, object]:
    sample_count = sum(count for _, count in histogram)
    weighted_sum = sum(width * count for width, count in histogram)
    nearest_rank_target = (95 * sample_count + 99) // 100
    cumulative = 0
    nearest_rank_p95 = 0
    for width, count in histogram:
        cumulative += count
        if cumulative >= nearest_rank_target:
            nearest_rank_p95 = width
            break
    return {
        "sample_count": sample_count,
        "mean": weighted_sum / sample_count,
        "nearest_rank_p95": nearest_rank_p95,
        "maximum": histogram[-1][0],
    }


def validate_histogram(raw: object, sample_decisions: object) -> list[tuple[int, int]]:
    if type(sample_decisions) is not int or sample_decisions <= 0:
        raise ValueError("width evidence sample_decisions must be a positive integer")
    if not isinstance(raw, list) or not raw:
        raise ValueError("width evidence histogram must be a nonempty list")
    histogram: list[tuple[int, int]] = []
    for item in raw:
        if not isinstance(item, dict) or set(item) != {"width", "policy_decision_count"}:
            raise ValueError("width evidence histogram entries have an invalid shape")
        width = item["width"]
        count = item["policy_decision_count"]
        if type(width) is not int or not 1 <= width <= 64:
            raise ValueError("width evidence contains a width outside the admitted 1..64 range")
        if type(count) is not int or count <= 0:
            raise ValueError("width evidence counts must be positive integers")
        histogram.append((width, count))
    if histogram != sorted(histogram) or len({width for width, _ in histogram}) != len(histogram):
        raise ValueError("width evidence histogram must be strictly sorted by width")
    if sum(count for _, count in histogram) != sample_decisions:
        raise ValueError("width evidence histogram does not sum to sample_decisions")
    return histogram


def provisional_synthetic_width_profile() -> tuple[dict[str, object], list[tuple[int, int]]]:
    histogram = list(PROVISIONAL_SYNTHETIC_HISTOGRAM)
    return (
        {
            "status": "provisional_synthetic",
            "claim_eligible": False,
            "scope": "provisional_synthetic_not_observed_policy_decisions",
            "source_artifact": None,
            "source_artifact_sha256": None,
            "source_schema": None,
            "source_aggregate_record_sha256": None,
            "source_provenance_class": None,
            "source_performance_gate_valid": None,
            "source_performance_rates_included": None,
            "source_coverage_scope": None,
            "raw_source_artifact_sha256": None,
            "raw_source_artifact_size_bytes": None,
            "source_head_before": None,
            "source_head_after": None,
            "source_worktree_state_before": None,
            "source_worktree_state_after": None,
            "source_status_sha256_before": None,
            "source_status_sha256_after": None,
            "source_manifest_file_count": None,
            "source_manifest_sha256_before": None,
            "source_manifest_sha256_after": None,
            "bound_build_source_manifest_sha256_before": None,
            "bound_build_source_manifest_sha256_after": None,
            "source_attestations_stable": None,
            "environment_binary_sha256_prebuild": None,
            "environment_binary_sha256_postbuild": None,
            "environment_binary_sha256_before": None,
            "environment_binary_sha256_after": None,
            "benchmark_binary_sha256_prebuild": None,
            "benchmark_binary_sha256_postbuild": None,
            "benchmark_binary_sha256_before": None,
            "benchmark_binary_sha256_after": None,
            "binary_attestations_stable": None,
            "formal_binary_source_attestation_present": None,
            "compiled_input_closure_attested": None,
            "statistics": histogram_statistics(histogram),
            "histogram": [
                {"width": width, "policy_decision_count": count} for width, count in histogram
            ],
            "final_all_nine_deck_gate": "deferred",
        },
        histogram,
    )


def observed_width_profile(raw_path: str) -> tuple[dict[str, object], list[tuple[int, int]]]:
    relative_path = parse_repo_relative_path(raw_path)
    artifact_bytes = (REPOSITORY_ROOT / relative_path).read_bytes()
    artifact = strict_json_loads(artifact_bytes)
    if not isinstance(artifact, dict) or set(artifact) != {
        "schema_version",
        "aggregate_record_sha256",
        "record",
    }:
        raise ValueError("width evidence envelope has an invalid shape")
    if artifact["schema_version"] != WIDTH_EVIDENCE_SCHEMA:
        raise ValueError("width evidence schema is not the required Rally aggregate schema")
    record = artifact["record"]
    if not isinstance(record, dict):
        raise ValueError("width evidence record must be an object")
    expected_record_digest = sha256_hex(canonical_json_bytes(record))
    if artifact["aggregate_record_sha256"] != expected_record_digest:
        raise ValueError("width evidence aggregate record digest does not match")
    required_provenance = {
        "benchmark_binary_sha256",
        "legal_action_width_histogram_scope",
        "matchup",
        "sample_decisions",
        "base_seed",
        "seed_derivation",
        "episode_ids",
        "natural_terminal_count",
        "game_limits",
        "kernel_contract",
        "feature_contract",
        "source_sha256s",
        "environment_binary_sha256",
        "source_artifact_sha256",
        "source_artifact_size_bytes",
        "source_head",
        "source_head_before",
        "source_head_after",
        "source_worktree_state_before",
        "source_worktree_state_after",
        "source_status_sha256_before",
        "source_status_sha256_after",
        "source_manifest_file_count",
        "source_manifest_sha256_before",
        "source_manifest_sha256_after",
        "bound_build_source_manifest_sha256_before",
        "bound_build_source_manifest_sha256_after",
        "source_attestations_stable",
        "environment_binary_sha256_prebuild",
        "environment_binary_sha256_postbuild",
        "environment_binary_sha256_before",
        "environment_binary_sha256_after",
        "benchmark_binary_sha256_prebuild",
        "benchmark_binary_sha256_postbuild",
        "benchmark_binary_sha256_before",
        "benchmark_binary_sha256_after",
        "binary_attestations_stable",
        "formal_binary_source_attestation_present",
        "compiled_input_closure_attested",
        "provenance_class",
        "performance_gate_valid",
        "performance_invalid_reasons",
        "performance_rates_included",
        "coverage_scope",
        "legal_action_width_histogram",
    }
    if set(record) != required_provenance:
        raise ValueError("width evidence public provenance has an invalid shape")
    if record["legal_action_width_histogram_scope"] != WIDTH_EVIDENCE_SCOPE:
        raise ValueError("width evidence scope is not all Rally policy decisions")
    matchup = require_exact_object(
        record["matchup"],
        {"p0_deck_id", "p1_deck_id", "p0_deck_hash_u64", "p1_deck_hash_u64"},
        "matchup",
    )
    if matchup["p0_deck_id"] != "Rally" or matchup["p1_deck_id"] != "Rally":
        raise ValueError("width evidence matchup must be Rally versus Rally")
    for field in ("p0_deck_hash_u64", "p1_deck_hash_u64"):
        if not is_json_integer(matchup[field], maximum=MASK64):
            raise ValueError("width evidence matchup must bind unsigned 64-bit deck hashes")
    if not is_json_integer(record["base_seed"], maximum=MASK64):
        raise ValueError("width evidence base_seed must be an unsigned 64-bit integer")
    if record["seed_derivation"] != "splitmix64(base_seed_xor_episode_id)":
        raise ValueError("width evidence seed derivation contract drifted")
    episode_ids = record["episode_ids"]
    if (
        not isinstance(episode_ids, list)
        or not episode_ids
        or any(not is_json_integer(value, maximum=MASK64) for value in episode_ids)
        or episode_ids != sorted(set(episode_ids))
    ):
        raise ValueError("width evidence episode IDs must be unique sorted uint64 integers")
    if not is_json_integer(record["natural_terminal_count"], maximum=len(episode_ids)):
        raise ValueError("width evidence natural terminal count is invalid")
    game_limits = require_exact_object(
        record["game_limits"],
        {"max_physical_decisions", "max_policy_steps"},
        "game limits",
    )
    if any(not is_json_integer(value, minimum=1) for value in game_limits.values()):
        raise ValueError("width evidence game limits must be positive integers")
    kernel_contract = require_exact_object(
        record["kernel_contract"], KERNEL_CONTRACT_FIELDS, "kernel contract"
    )
    if (
        not is_json_integer(kernel_contract["card_db_hash"], maximum=MASK64)
        or not isinstance(kernel_contract["kernel_version"], str)
        or not kernel_contract["kernel_version"]
        or kernel_contract["protocol"] != "kernel_rl_jsonl"
        or any(
            not is_json_integer(kernel_contract[field])
            for field in (
                "policy_surface_version",
                "protocol_version",
                "schema_version",
                "surface_version",
            )
        )
    ):
        raise ValueError("width evidence kernel contract values are invalid")
    feature_contract = require_exact_object(
        record["feature_contract"], FEATURE_CONTRACT_FIELDS, "feature contract"
    )
    for field in ("contract_digest", "encoding_digest"):
        if not is_lower_sha256(feature_contract[field]):
            raise ValueError("width evidence feature contract digests must be lowercase SHA256")
    for field in (
        "action_feature_dim",
        "action_ref_feature_dim",
        "edge_feature_dim",
        "object_feature_dim",
        "object_group_count",
        "state_dim",
    ):
        if not is_json_integer(feature_contract[field], minimum=1):
            raise ValueError("width evidence feature dimensions must be positive integers")
    for field in ("registry_version", "version"):
        if not isinstance(feature_contract[field], str) or not feature_contract[field]:
            raise ValueError("width evidence feature version strings must be nonempty")
    source_sha256s = require_exact_object(
        record["source_sha256s"], SOURCE_SHA256_ROLES, "source digest map"
    )
    if any(not is_lower_sha256(value) for value in source_sha256s.values()):
        raise ValueError("width evidence source digests must be lowercase SHA256")
    for field in (
        "environment_binary_sha256",
        "benchmark_binary_sha256",
        "environment_binary_sha256_prebuild",
        "environment_binary_sha256_postbuild",
        "environment_binary_sha256_before",
        "environment_binary_sha256_after",
        "benchmark_binary_sha256_prebuild",
        "benchmark_binary_sha256_postbuild",
        "benchmark_binary_sha256_before",
        "benchmark_binary_sha256_after",
        "source_status_sha256_before",
        "source_status_sha256_after",
        "source_manifest_sha256_before",
        "source_manifest_sha256_after",
        "bound_build_source_manifest_sha256_before",
        "bound_build_source_manifest_sha256_after",
    ):
        if not is_lower_sha256(record[field]):
            raise ValueError("width evidence attestation digests must be lowercase SHA256")
    if not is_lower_sha256(record["source_artifact_sha256"]):
        raise ValueError("width evidence source artifact digest must be lowercase SHA256")
    if not (
        isinstance(record["source_head"], str)
        and len(record["source_head"]) == 40
        and all(character in "0123456789abcdef" for character in record["source_head"])
    ):
        raise ValueError("width evidence source HEAD must be a lowercase 40-hex commit")
    if (
        record["source_artifact_sha256"] != RALLY_RAW_SOURCE_ARTIFACT_SHA256
        or record["source_artifact_size_bytes"] != RALLY_RAW_SOURCE_ARTIFACT_SIZE_BYTES
        or record["source_head"] != RALLY_SOURCE_HEAD
        or record["source_head_before"] != RALLY_SOURCE_HEAD
        or record["source_head_after"] != RALLY_SOURCE_HEAD
        or record["source_worktree_state_before"] != "clean"
        or record["source_worktree_state_after"] != "clean"
        or record["source_status_sha256_before"] != EMPTY_SHA256
        or record["source_status_sha256_after"] != EMPTY_SHA256
        or record["source_manifest_file_count"] != RALLY_SOURCE_MANIFEST_FILE_COUNT
        or record["source_manifest_sha256_before"] != RALLY_SOURCE_MANIFEST_SHA256
        or record["source_manifest_sha256_after"] != RALLY_SOURCE_MANIFEST_SHA256
        or record["bound_build_source_manifest_sha256_before"]
        != RALLY_SOURCE_MANIFEST_SHA256
        or record["bound_build_source_manifest_sha256_after"]
        != RALLY_SOURCE_MANIFEST_SHA256
        or record["source_attestations_stable"] is not True
        or record["environment_binary_sha256"] != RALLY_ENVIRONMENT_BINARY_SHA256
        or record["environment_binary_sha256_prebuild"]
        != RALLY_ENVIRONMENT_BINARY_SHA256
        or record["environment_binary_sha256_postbuild"]
        != RALLY_ENVIRONMENT_BINARY_SHA256
        or record["environment_binary_sha256_before"]
        != RALLY_ENVIRONMENT_BINARY_SHA256
        or record["environment_binary_sha256_after"]
        != RALLY_ENVIRONMENT_BINARY_SHA256
        or record["benchmark_binary_sha256"] != RALLY_BENCHMARK_BINARY_SHA256
        or record["benchmark_binary_sha256_prebuild"] != RALLY_BENCHMARK_BINARY_SHA256
        or record["benchmark_binary_sha256_postbuild"] != RALLY_BENCHMARK_BINARY_SHA256
        or record["benchmark_binary_sha256_before"] != RALLY_BENCHMARK_BINARY_SHA256
        or record["benchmark_binary_sha256_after"] != RALLY_BENCHMARK_BINARY_SHA256
        or record["binary_attestations_stable"] is not True
        or record["formal_binary_source_attestation_present"] is not False
        or record["compiled_input_closure_attested"] is not False
    ):
        raise ValueError("width evidence source and binary attestations are invalid or unstable")
    if (
        record["provenance_class"]
        != "deterministic_workload_shape_only_not_performance_evidence"
        or record["performance_gate_valid"] is not False
        or record["performance_invalid_reasons"]
        != [
            "one_or_more_common_window_timing_bounds_failed",
            "one_or_more_external_interference_bounds_failed",
        ]
        or record["performance_rates_included"] is not False
        or record["coverage_scope"] != "rally_vs_rally_only_not_nine_deck_coverage"
    ):
        raise ValueError("width evidence must remain workload-only and performance-invalid")
    histogram = validate_histogram(
        record["legal_action_width_histogram"],
        record["sample_decisions"],
    )
    return (
        {
            "status": "observed_provenance_bound",
            "claim_eligible": True,
            "scope": WIDTH_EVIDENCE_SCOPE,
            "source_artifact": relative_path.as_posix(),
            "source_artifact_sha256": sha256_hex(artifact_bytes),
            "source_schema": WIDTH_EVIDENCE_SCHEMA,
            "source_aggregate_record_sha256": expected_record_digest,
            "source_provenance_class": record["provenance_class"],
            "source_performance_gate_valid": record["performance_gate_valid"],
            "source_performance_rates_included": record["performance_rates_included"],
            "source_coverage_scope": record["coverage_scope"],
            "raw_source_artifact_sha256": record["source_artifact_sha256"],
            "raw_source_artifact_size_bytes": record["source_artifact_size_bytes"],
            "source_head_before": record["source_head_before"],
            "source_head_after": record["source_head_after"],
            "source_worktree_state_before": record["source_worktree_state_before"],
            "source_worktree_state_after": record["source_worktree_state_after"],
            "source_status_sha256_before": record["source_status_sha256_before"],
            "source_status_sha256_after": record["source_status_sha256_after"],
            "source_manifest_file_count": record["source_manifest_file_count"],
            "source_manifest_sha256_before": record["source_manifest_sha256_before"],
            "source_manifest_sha256_after": record["source_manifest_sha256_after"],
            "bound_build_source_manifest_sha256_before": record[
                "bound_build_source_manifest_sha256_before"
            ],
            "bound_build_source_manifest_sha256_after": record[
                "bound_build_source_manifest_sha256_after"
            ],
            "source_attestations_stable": record["source_attestations_stable"],
            "environment_binary_sha256_prebuild": record[
                "environment_binary_sha256_prebuild"
            ],
            "environment_binary_sha256_postbuild": record[
                "environment_binary_sha256_postbuild"
            ],
            "environment_binary_sha256_before": record["environment_binary_sha256_before"],
            "environment_binary_sha256_after": record["environment_binary_sha256_after"],
            "benchmark_binary_sha256_prebuild": record["benchmark_binary_sha256_prebuild"],
            "benchmark_binary_sha256_postbuild": record["benchmark_binary_sha256_postbuild"],
            "benchmark_binary_sha256_before": record["benchmark_binary_sha256_before"],
            "benchmark_binary_sha256_after": record["benchmark_binary_sha256_after"],
            "binary_attestations_stable": record["binary_attestations_stable"],
            "formal_binary_source_attestation_present": record[
                "formal_binary_source_attestation_present"
            ],
            "compiled_input_closure_attested": record["compiled_input_closure_attested"],
            "statistics": histogram_statistics(histogram),
            "histogram": [
                {"width": width, "policy_decision_count": count} for width, count in histogram
            ],
            "final_all_nine_deck_gate": "deferred",
        },
        histogram,
    )


def oracle_cases(histogram: list[tuple[int, int]]) -> list[dict[str, object]]:
    cases = [
        make_case(
            f"width-profile-width-{width}",
            "width-profile-representative",
            representative_logits(width),
            count,
        )
        for width, count in histogram
    ]
    cases.extend(
        [
            make_case("equal-tie-order", "adversarial", [f32_bits(0.0)] * 4),
            make_case(
                "repeated-weight-legal-order",
                "adversarial",
                [f32_bits(value) for value in (0.0, -1.0, 0.0, -1.0)],
            ),
            make_case(
                "q8-halfway-ties",
                "adversarial",
                [f32_bits(value) for value in (0.0, -1 / 512, -3 / 512, -5 / 512, -7 / 512)],
            ),
            make_case(
                "q8-halfway-neighbors",
                "adversarial",
                [
                    f32_bits(0.0),
                    f32_bits(-1 / 512 + 2**-25),
                    f32_bits(-1 / 512 - 2**-25),
                    f32_bits(-3 / 512 + 2**-24),
                    f32_bits(-3 / 512 - 2**-24),
                ],
            ),
            make_case(
                "clamp-neighborhood",
                "adversarial",
                [f32_bits(value) for value in (0.0, -15.998046875, -16.0, -16.001953125, -17.0)],
            ),
            make_case(
                "legacy-cutoff-neighborhood",
                "adversarial",
                [f32_bits(value) for value in (0.0, -127.999, -128.0, -128.001, -129.0)],
            ),
            make_case(
                "finite-extremes",
                "adversarial",
                [0x7F7FFFFF, 0xFF7FFFFF, f32_bits(0.0), f32_bits(-1.0)],
            ),
            make_case(
                "signed-zero-and-subnormal",
                "adversarial",
                [0x00000000, 0x80000000, 0x00000001, 0x80000001],
            ),
            make_case(
                "large-nearby-finite",
                "adversarial",
                [f32_bits(value) for value in (16_777_216.0, 16_777_215.0, 16_777_214.0, 16_777_200.0)],
            ),
            make_case(
                "maximum-admitted-width",
                "boundary",
                [f32_bits(-(((index * 37) % 4097) / 256.0)) for index in range(64)],
            ),
        ]
    )
    return cases


def independent_rng_and_selection_goldens(cases: list[dict[str, object]]) -> dict[str, object]:
    draws = [splitmix64_first(seed) for seed in range(SELECTION_SEED_START, SELECTION_SEED_END)]
    draw_bytes = b"".join(struct.pack("<Q", draw) for draw in draws)
    selected_indices = bytearray()
    explicit_selected = []
    explicit_seeds = {0, 1, 2, 3, 255, 4095}
    for case in cases:
        masses = [int(value) for value in case["decimal_mass"]]
        for seed, draw in enumerate(draws):
            selected = select_mass(masses, draw)
            selected_indices.append(selected)
            if seed in explicit_seeds and case["name"] in {
                cases[0]["name"],
                "q8-halfway-neighbors",
                "maximum-admitted-width",
            }:
                explicit_selected.append(
                    {
                        "case_name": case["name"],
                        "seed": str(seed),
                        "draw_hex": f"{draw:016x}",
                        "selected_index": selected,
                    }
                )
    explicit_draw_seeds = (0, 1, 2, 3, (1 << 63) - 1, MASK64)
    return {
        "producer": "independent Python integer implementation in generator",
        "seed_range": {
            "inclusive_start": SELECTION_SEED_START,
            "exclusive_end": SELECTION_SEED_END,
        },
        "splitmix_first_draws": {
            "encoding": "seed-order little-endian u64 bytes encoded as lowercase hex",
            "bytes_hex": draw_bytes.hex(),
            "sha256": sha256_hex(draw_bytes),
            "explicit": [
                {"seed": str(seed), "draw_hex": f"{splitmix64_first(seed):016x}"}
                for seed in explicit_draw_seeds
            ],
        },
        "decimal_selected_indices": {
            "encoding": "case-major then seed-major selected index as one u8",
            "sha256": sha256_hex(bytes(selected_indices)),
            "explicit": explicit_selected,
        },
    }


def build_payload(
    width_profile: dict[str, object],
    histogram: list[tuple[int, int]],
) -> dict[str, object]:
    cases = oracle_cases(histogram)
    return {
        "schema_version": 2,
        "oracle_sampler_version": ORACLE_VERSION,
        "generator": "python/tools/generate_fast_sampler_oracle_v1.py",
        "workload_width_profile": width_profile,
        "predeclared_candidate_bounds": {
            "maximum_total_variation": 0.00125,
            "width_profile_weighted_mean_total_variation": 0.0005,
            "maximum_legacy_to_candidate_kl_nats": 0.00001,
            "aggregate_selected_index_agreement": 0.9985,
        },
        "independent_rng_and_selection_goldens": independent_rng_and_selection_goldens(cases),
        "cases": cases,
    }


def render(width_profile: dict[str, object], histogram: list[tuple[int, int]]) -> str:
    return json.dumps(build_payload(width_profile, histogram), indent=2, ensure_ascii=True) + "\n"


def main() -> int:
    parser = argparse.ArgumentParser()
    source = parser.add_mutually_exclusive_group(required=True)
    source.add_argument("--width-evidence")
    source.add_argument("--provisional-synthetic", action="store_true")
    parser.add_argument("--check", action="store_true")
    parser.add_argument("--stdout", action="store_true")
    arguments = parser.parse_args()
    if arguments.provisional_synthetic:
        width_profile, histogram = provisional_synthetic_width_profile()
    else:
        width_profile, histogram = observed_width_profile(arguments.width_evidence)
    rendered = render(width_profile, histogram)
    if arguments.stdout:
        sys.stdout.write(rendered)
        return 0
    if arguments.check:
        if not OUTPUT.is_file() or OUTPUT.read_text(encoding="utf-8") != rendered:
            print("FAST_SAMPLER_DECIMAL_ORACLE: STALE", file=sys.stderr)
            return 1
        print("FAST_SAMPLER_DECIMAL_ORACLE: PASS")
        return 0
    OUTPUT.write_text(rendered, encoding="utf-8", newline="\n")
    print(f"wrote {OUTPUT_RELATIVE.as_posix()}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
