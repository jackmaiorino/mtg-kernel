#!/usr/bin/env python3
"""Fail-closed classifier for observation-diagnostics Diagnostic B.

The classifier is deliberately offline and stdlib-only.  It consumes the six
raw ``OBS_RELIANCE_JSON`` envelopes in manifest order, verifies their embedded
Rust payload hashes and fixed contracts, and applies only the predeclared
five-of-six plus pooled-mean sign rule.

No metric is majority-voted into an overall conclusion.
"""

from __future__ import annotations

import argparse
from dataclasses import dataclass
from decimal import Decimal, localcontext
from fractions import Fraction
import hashlib
import json
import math
import os
from pathlib import Path
import sys
from typing import Any, Iterable, Mapping, NoReturn, Sequence

try:
    from . import contract
except ImportError:  # Direct execution from this script directory.
    import contract


REPORT_SCHEMA = "mtg-kernel-observation-diagnostics-classification/v1"
POSITIVE_LABEL = "DIGEST-SIBLING-EFFECT-EXCEEDS"
NEGATIVE_LABEL = "DIRECT-SIBLING-EFFECT-EXCEEDS"
MIXED_LABEL = "MIXED"
CLASSIFICATION_RETRY_V1_EXECUTION_GIT_HEAD = (
    "c1cf5f1de05b64a4cae35c61862adc725df46837"
)

PROBE_CORPUS_IDENTITY = (
    "rally-mirror-splitmix64-modulo-fixed-256-post-tensorization-v1"
)
PROBE_CORPUS_DIGEST_IDENTITY = (
    "sha256-framed-thirteen-native-flat-tensors-v1"
)
PROBE_OUTPUT_DIGEST_IDENTITY = (
    "sha256-framed-role-condition-decision-logit-value-f32le-v1"
)
EXPECTED_CORPUS_SHA256 = (
    "72103ea367a662f76675a044ad4efcf4c52bf86d32630df88e5247cf79f5e5e0"
)
MODEL_ARCHITECTURE_VERSION = "kernel-policy-value-net-8"
MODEL_CONFIG_FINGERPRINT = (
    "f3836afa17acc74b4856fe18222345116f27c12fa5ad18c34b4dec3f04855251"
)
FEATURE_CONTRACT_DIGEST = (
    "bcc808186e40a1ad6aec679d8a386631cb1226379366a632603f0beb95b47396"
)
FEATURE_ENCODING_DIGEST = (
    "918e57a0796807e84310026de48d30b500813ef37d939462ea85b7255a39111c"
)

INTERVENTIONS = (
    "state_hash_permutation",
    "action_hash_permutation",
    "both_hash_permutation",
    "state_direct_permutation",
    "action_direct_permutation",
    "both_direct_permutation",
    "hash_zero_ablation",
    "direct_zero_ablation",
)
WITHIN_CONTRASTS = {
    "state_hash_minus_direct": (
        "state_hash_permutation",
        "state_direct_permutation",
    ),
    "action_hash_minus_direct": (
        "action_hash_permutation",
        "action_direct_permutation",
    ),
    "both_hash_minus_direct": (
        "both_hash_permutation",
        "both_direct_permutation",
    ),
    "zero_hash_minus_direct": (
        "hash_zero_ablation",
        "direct_zero_ablation",
    ),
}

METRICS = {
    "action_jensen_shannon_mean": {
        "pathway": "action_policy",
        "within_contrast": "action_hash_minus_direct",
        "within_field": "mean_jensen_shannon_hash_minus_direct",
        "effect_field": "jensen_shannon_nats",
        "effect_subfield": "mean",
        "training_field": "candidate_minus_g0_mean_jensen_shannon",
    },
    "action_centered_logit_rms_mean": {
        "pathway": "action_policy",
        "within_contrast": "action_hash_minus_direct",
        "within_field": "mean_centered_logit_rms_hash_minus_direct",
        "effect_field": "centered_logit_rms_delta",
        "effect_subfield": "mean",
        "training_field": "candidate_minus_g0_mean_centered_logit_rms",
    },
    "action_top_action_flip_fraction": {
        "pathway": "action_policy",
        "within_contrast": "action_hash_minus_direct",
        "within_field": "top_action_flip_fraction_hash_minus_direct",
        "effect_field": "top_action_flip_fraction",
        "effect_subfield": None,
        "training_field": "candidate_minus_g0_top_action_flip_fraction",
    },
    "state_value_rmse": {
        "pathway": "state_value",
        "within_contrast": "state_hash_minus_direct",
        "within_field": "value_rmse_hash_minus_direct",
        "effect_field": "value_rmse",
        "effect_subfield": None,
        "training_field": "candidate_minus_g0_value_rmse",
    },
}

PAYLOAD_KEYS = {
    "schema",
    "label",
    "test_identity",
    "model_architecture_version",
    "model_config_fingerprint",
    "feature_contract_digest",
    "feature_encoding_digest",
    "run_base_seed",
    "checkpoints",
    "feature_partition",
    "corpus",
    "permutation",
    "ingress_groups",
    "hash_to_direct_ingress_ratios",
    "functional_models",
    "candidate_minus_g0_functional_effects",
    "aggregate_output_stream_sha256",
    "repeat_aggregate_output_stream_bit_exact",
    "output_digest_identity",
    "nonclaims",
}
CHECKPOINT_KEYS = {
    "role",
    "generation_index",
    "run_sha256",
    "identity_bundle_sha256",
    "segment_ordinal",
    "segment_manifest_sha256",
    "parent_boundary_head_sha256",
    "boundary_head_sha256",
    "boundary_head_record_sha256",
    "checkpoint_manifest_sha256",
    "checkpoint_payload_sha256",
    "checkpoint_sidecar_sha256",
    "logical_state_sha256",
    "train_state_sha256",
    "model_parameter_sha256",
    "last_update_evidence_sha256",
    "adam_step",
}
FUNCTIONAL_MODEL_KEYS = {
    "role",
    "generation_index",
    "baseline_output_sha256",
    "repeat_baseline_bit_exact",
    "effects",
    "hash_minus_direct_contrasts",
}
EFFECT_KEYS = {
    "intervention",
    "intervention_output_sha256",
    "policy_decision_count",
    "jensen_shannon_nats",
    "centered_logit_rms_delta",
    "top_action_flip_count",
    "top_action_flip_fraction",
    "baseline_top_probability_delta_baseline_minus_intervened",
    "value_decision_count",
    "baseline_value_exact_zero_count",
    "intervened_value_exact_zero_count",
    "value_zero_transition_count",
    "value_absolute_delta",
    "value_rmse",
    "value_sign_flip_count",
    "value_sign_flip_fraction",
}
SUMMARY_KEYS = {"mean", "p50_nearest_rank", "p95_nearest_rank", "max"}
WITHIN_KEYS = {
    "name",
    "hash_intervention",
    "direct_intervention",
    "mean_jensen_shannon_hash_minus_direct",
    "mean_centered_logit_rms_hash_minus_direct",
    "top_action_flip_fraction_hash_minus_direct",
    "mean_value_absolute_delta_hash_minus_direct",
    "value_rmse_hash_minus_direct",
}
TRAINING_KEYS = {
    "intervention",
    "candidate_minus_g0_mean_jensen_shannon",
    "candidate_minus_g0_mean_centered_logit_rms",
    "candidate_minus_g0_top_action_flip_fraction",
    "candidate_minus_g0_mean_value_absolute_delta",
    "candidate_minus_g0_value_rmse",
}
COMPLETION_KEYS = {
    "build_receipt",
    "completed_utc",
    "cpu_only",
    "cross_pair_invariants",
    "elapsed_ms",
    "evidence_status",
    "executable",
    "fixed_pairs",
    "git_head",
    "git_status_clean_before_and_after",
    "invocation_count",
    "invocations",
    "label",
    "manifest",
    "output_inventory",
    "runner_source",
    "contract_source",
    "schema",
    "sequential_execution",
    "started_utc",
    "status",
    "timeout_seconds_per_pair",
    "payload_sha256",
}
INVOCATION_SUMMARY_KEYS = {
    "aggregate_output_stream_sha256",
    "candidate_generation",
    "envelope_sha256",
    "invocation_receipt",
    "name",
    "payload_sha256",
    "seed",
    "stderr_sha256",
    "stdout_sha256",
    "wall_time_ms",
}
INVOCATION_RECEIPT_KEYS = {
    "build_receipt",
    "command",
    "completed_utc",
    "contract_environment",
    "executable",
    "exit_code",
    "git_head",
    "label",
    "manifest_sha256",
    "pair",
    "probe",
    "schema",
    "started_utc",
    "status",
    "stderr",
    "stdout",
    "timed_out",
    "timeout_seconds",
    "wall_time_ms",
    "payload_sha256",
}
INVOCATION_PROBE_KEYS = {
    "aggregate_output_stream_sha256",
    "candidate_checkpoint",
    "contract_binding",
    "corpus_binding",
    "envelope",
    "g0_checkpoint",
    "marker_count",
    "payload",
    "timing",
    "timing_line",
    "timing_line_count",
}

ClassificationError = contract.DiagnosticError


@dataclass(frozen=True)
class ValidatedProbe:
    spec: contract.PairSpec
    path: Path
    report_sha256: str
    payload_sha256: str
    payload: Mapping[str, Any]
    checkpoints: Mapping[str, Mapping[str, Any]]
    functional_models: Mapping[str, Mapping[str, Any]]
    effects: Mapping[str, Mapping[str, Mapping[str, Any]]]
    within: Mapping[str, Mapping[str, Mapping[str, Any]]]
    training: Mapping[str, Mapping[str, Any]]


@dataclass(frozen=True)
class CompletionBinding:
    path: Path
    sha256: str
    payload_sha256: str
    git_head: str
    invocation_receipt_sha256_by_pair: Mapping[str, str]
    authoritative: bool


def fail(message: str) -> NoReturn:
    contract.fail(message)


def _exact(value: Any, expected: Iterable[str], where: str) -> Mapping[str, Any]:
    return contract.exact_keys(value, expected, where)


def _string(value: Any, where: str) -> str:
    if type(value) is not str or not value:
        fail(f"{where} must be a nonempty string")
    return value


def _boolean(value: Any, where: str) -> bool:
    if type(value) is not bool:
        fail(f"{where} must be Boolean")
    return value


def _natural(value: Any, where: str, *, positive: bool = False) -> int:
    return contract.require_natural(value, where, positive=positive)


def _number(
    value: Any,
    where: str,
    *,
    nonnegative: bool = False,
    canonical_f64: bool = True,
) -> Decimal:
    if isinstance(value, bool) or not isinstance(value, (int, Decimal)):
        fail(f"{where} must be a finite JSON number")
    result = Decimal(value)
    if not result.is_finite():
        fail(f"{where} must be finite")
    as_f64 = float(result)
    if not math.isfinite(as_f64):
        fail(f"{where} must be representable as a finite f64")
    if result != 0 and as_f64 == 0.0:
        fail(f"{where} must not underflow to zero as f64")
    if canonical_f64 and result != Decimal(str(as_f64)):
        fail(f"{where} must be the canonical shortest finite f64 value")
    if nonnegative and result < 0:
        fail(f"{where} must be nonnegative")
    return result


def _array(value: Any, where: str, *, length: int | None = None) -> list[Any]:
    result = contract.require_array(value, where)
    if length is not None and len(result) != length:
        fail(f"{where} must contain exactly {length} entries")
    return result


def _expect(value: Any, expected: Any, where: str) -> None:
    def exactly_equal(left: Any, right: Any) -> bool:
        if type(left) is not type(right):
            return False
        if isinstance(left, dict):
            return left.keys() == right.keys() and all(
                exactly_equal(left[key], right[key]) for key in left
            )
        if isinstance(left, list):
            return len(left) == len(right) and all(
                exactly_equal(left_item, right_item)
                for left_item, right_item in zip(left, right, strict=True)
            )
        return bool(left == right)

    if not exactly_equal(value, expected):
        fail(f"{where} mismatch: expected={expected!r} actual={value!r}")


def _index_unique(
    values: Any,
    *,
    key: str,
    expected: Iterable[str],
    where: str,
) -> dict[str, Mapping[str, Any]]:
    items = _array(values, where)
    indexed: dict[str, Mapping[str, Any]] = {}
    for index, raw in enumerate(items):
        item = raw if isinstance(raw, Mapping) else fail(f"{where}[{index}] must be an object")
        name = _string(item.get(key), f"{where}[{index}].{key}")
        if name in indexed:
            fail(f"{where} contains duplicate {key}={name!r}")
        indexed[name] = item
    expected_set = set(expected)
    if set(indexed) != expected_set:
        fail(
            f"{where} identities mismatch: "
            f"missing={sorted(expected_set - set(indexed))} "
            f"extra={sorted(set(indexed) - expected_set)}"
        )
    return indexed


def _sha(value: Any, where: str) -> str:
    return contract.require_sha256(value, where)


def _read_raw_envelope(path: Path) -> tuple[bytes, Mapping[str, Any], bytes]:
    try:
        raw = path.read_bytes()
    except OSError as error:
        raise ClassificationError(f"could not read {path}: {error}") from error
    if raw.endswith(b"\r\n"):
        document_raw = raw[:-2]
    elif raw.endswith(b"\n"):
        document_raw = raw[:-1]
    else:
        document_raw = raw
    if not document_raw or document_raw[:1].isspace() or document_raw[-1:].isspace():
        fail(f"{path} must contain one compact raw Rust envelope")

    envelope = contract.parse_json_bytes(document_raw, str(path))
    _exact(envelope, ("schema", "payload_sha256", "payload"), str(path))
    _expect(envelope["schema"], contract.PROBE_ENVELOPE_SCHEMA, f"{path}.schema")
    claimed = _sha(envelope["payload_sha256"], f"{path}.payload_sha256")
    prefix = (
        '{"schema":'
        + json.dumps(contract.PROBE_ENVELOPE_SCHEMA, ensure_ascii=False)
        + ',"payload_sha256":'
        + json.dumps(claimed)
        + ',"payload":'
    ).encode("utf-8")
    if not document_raw.startswith(prefix) or not document_raw.endswith(b"}"):
        fail(
            f"{path} is not the exact compact Rust ProbeEnvelopeV1 serialization; "
            "embedded payload bytes cannot be hash-verified"
        )
    payload_raw = document_raw[len(prefix) : -1]
    if hashlib.sha256(payload_raw).hexdigest() != claimed:
        fail(f"{path} embedded Rust payload SHA-256 mismatch")
    parsed_payload = contract.parse_json_bytes(payload_raw, f"{path}.payload")
    if parsed_payload != envelope["payload"]:
        fail(f"{path} embedded payload parse disagrees with envelope payload")
    return raw, envelope, payload_raw


def _validate_partition(value: Any, where: str) -> None:
    expected = {
        "state_feature_dim": 219,
        "state_direct_range": [0, 123],
        "state_observation_hash_range": [123, 219],
        "action_feature_dim": 195,
        "action_direct_range": [0, 99],
        "action_legal_hash_range": [99, 195],
        "hash_feature_dim_each": 96,
        "state_encoder_first_weight_shape": [64, 1499],
        "action_encoder_first_weight_shape": [64, 259],
        "structured_explicit_inputs_are_a_separate_bucket": True,
    }
    record = _exact(value, expected, where)
    for key, expected_value in expected.items():
        _expect(record[key], expected_value, f"{where}.{key}")


def _validate_corpus(value: Any, where: str) -> None:
    expected = {
        "identity": PROBE_CORPUS_IDENTITY,
        "digest_identity": PROBE_CORPUS_DIGEST_IDENTITY,
        "sha256": EXPECTED_CORPUS_SHA256,
        "deck_ids": ["Rally", "Rally"],
        "decision_count": 256,
        "episode_count": 4,
        "decisions_per_episode_cap": 64,
        "multi_action_decision_count": 256,
        "total_action_count": 1115,
        "base_episode_id": 880000,
        "base_environment_seed": 0x6D74672D68617368,
        "action_selection": "splitmix64-next-modulo-legal-action-count-v1",
    }
    record = _exact(value, expected, where)
    for key, expected_value in expected.items():
        _expect(record[key], expected_value, f"{where}.{key}")


def _expected_permutation_contract() -> dict[str, Any]:
    return {
        "state_block_mapping": "target-i-receives-source-(i+129)-mod-256",
        "state_donor_shift": 129,
        "action_block_mapping": (
            "within-decision-target-row-i-receives-source-(i+1)-mod-A"
        ),
        "forced_action_policy_metric_rule": (
            "A=1 excluded from policy metrics; included in value metrics"
        ),
        "zero_ablation_value": "positive-zero-f32",
        "integrity_controls": [
            "all non-target tensor fields bit-exact",
            "permuted block-bit multisets preserved exactly",
            "state hash donor differs for every decision",
            "permutation plus inverse restores exact corpus digest",
            "whole-action permutation rotates logits bit-exact and preserves value",
            "same-runtime repeated full-condition output stream is bit-exact",
        ],
    }


def _validate_permutation(value: Any, where: str) -> None:
    expected = _expected_permutation_contract()
    record = _exact(value, expected, where)
    for key, expected_value in expected.items():
        _expect(record[key], expected_value, f"{where}.{key}")


def _validate_summary(
    value: Any,
    where: str,
    *,
    nonnegative: bool,
) -> dict[str, Decimal]:
    record = _exact(value, SUMMARY_KEYS, where)
    numbers = {
        key: _number(record[key], f"{where}.{key}", nonnegative=nonnegative)
        for key in SUMMARY_KEYS
    }
    if numbers["p50_nearest_rank"] > numbers["p95_nearest_rank"]:
        fail(f"{where}.p50_nearest_rank exceeds p95_nearest_rank")
    if numbers["p95_nearest_rank"] > numbers["max"]:
        fail(f"{where}.p95_nearest_rank exceeds max")
    _require_f64_leq(
        numbers["mean"],
        numbers["max"],
        f"{where}.mean/max",
        "mean exceeds max beyond the f64 summation envelope",
    )
    return numbers


def _require_f64_leq(
    left: Decimal,
    right: Decimal,
    where: str,
    message: str,
    *,
    ulps: float = 512.0,
) -> None:
    if left <= right:
        return
    left_f64 = float(left)
    right_f64 = float(right)
    tolerance = max(math.ulp(left_f64), math.ulp(right_f64)) * ulps
    if left_f64 - right_f64 > tolerance:
        fail(f"{where}: {message}")


def _float_subtract(left: Decimal, right: Decimal) -> Decimal:
    """Reproduce one Rust f64 subtraction and return its exact decimal value."""

    return Decimal.from_float(float(left) - float(right))


def _exact_decimal_subtract(left: Decimal, right: Decimal) -> Decimal:
    """Subtract any two finite f64 decimal tokens without context rounding."""

    # A shortest finite-f64 decimal spans at most exponents +308..-324.
    # Eight hundred digits therefore retain the exact base-10 difference.
    with localcontext() as context:
        context.prec = 800
        return left - right


def _same_f64(actual: Decimal, expected: Decimal, where: str) -> None:
    if float(actual) != float(expected):
        fail(f"{where} fails the emitted f64 arithmetic cross-check")


def _validate_effect(
    value: Any,
    intervention: str,
    where: str,
) -> Mapping[str, Any]:
    record = _exact(value, EFFECT_KEYS, where)
    _expect(record["intervention"], intervention, f"{where}.intervention")
    _sha(record["intervention_output_sha256"], f"{where}.intervention_output_sha256")
    policy_count = _natural(
        record["policy_decision_count"],
        f"{where}.policy_decision_count",
        positive=True,
    )
    value_count = _natural(
        record["value_decision_count"],
        f"{where}.value_decision_count",
        positive=True,
    )
    if policy_count != 256 or value_count != 256:
        fail(f"{where} decision counts must both equal the fixed corpus size 256")

    js_summary = _validate_summary(
        record["jensen_shannon_nats"],
        f"{where}.jensen_shannon_nats",
        nonnegative=True,
    )
    _validate_summary(
        record["centered_logit_rms_delta"],
        f"{where}.centered_logit_rms_delta",
        nonnegative=True,
    )
    probability_delta_summary = _validate_summary(
        record["baseline_top_probability_delta_baseline_minus_intervened"],
        f"{where}.baseline_top_probability_delta_baseline_minus_intervened",
        nonnegative=False,
    )
    value_absolute_summary = _validate_summary(
        record["value_absolute_delta"],
        f"{where}.value_absolute_delta",
        nonnegative=True,
    )
    value_rmse = _number(
        record["value_rmse"],
        f"{where}.value_rmse",
        nonnegative=True,
    )
    js_upper_bound = Decimal.from_float(math.log(2.0))
    for key, metric in js_summary.items():
        _require_f64_leq(
            metric,
            js_upper_bound,
            f"{where}.jensen_shannon_nats.{key}",
            "Jensen-Shannon divergence exceeds ln(2)",
        )
    for key, metric in probability_delta_summary.items():
        _require_f64_leq(
            Decimal(-1),
            metric,
            f"{where}.baseline_top_probability_delta.{key}",
            "probability delta is below -1",
        )
        _require_f64_leq(
            metric,
            Decimal(1),
            f"{where}.baseline_top_probability_delta.{key}",
            "probability delta exceeds 1",
        )
    for key, metric in value_absolute_summary.items():
        _require_f64_leq(
            metric,
            Decimal(2),
            f"{where}.value_absolute_delta.{key}",
            "absolute value delta exceeds the tanh range",
        )
    _require_f64_leq(
        value_absolute_summary["mean"],
        value_rmse,
        f"{where}.value_absolute_delta.mean/value_rmse",
        "mean absolute error exceeds RMSE",
    )
    _require_f64_leq(
        value_rmse,
        value_absolute_summary["max"],
        f"{where}.value_rmse/value_absolute_delta.max",
        "RMSE exceeds maximum absolute error",
    )

    for key in (
        "top_action_flip_count",
        "baseline_value_exact_zero_count",
        "intervened_value_exact_zero_count",
        "value_zero_transition_count",
        "value_sign_flip_count",
    ):
        count = _natural(record[key], f"{where}.{key}")
        bound = policy_count if key == "top_action_flip_count" else value_count
        if count > bound:
            fail(f"{where}.{key} exceeds its decision count")
    flip_fraction = _number(
        record["top_action_flip_fraction"],
        f"{where}.top_action_flip_fraction",
        nonnegative=True,
    )
    sign_fraction = _number(
        record["value_sign_flip_fraction"],
        f"{where}.value_sign_flip_fraction",
        nonnegative=True,
    )
    if flip_fraction > 1 or sign_fraction > 1:
        fail(f"{where} flip fractions must be within [0,1]")
    _same_f64(
        flip_fraction,
        Decimal.from_float(record["top_action_flip_count"] / policy_count),
        f"{where}.top_action_flip_fraction",
    )
    _same_f64(
        sign_fraction,
        Decimal.from_float(record["value_sign_flip_count"] / value_count),
        f"{where}.value_sign_flip_fraction",
    )
    return record


def _effect_metric(
    effect: Mapping[str, Any],
    field: str,
    subfield: str | None,
    where: str,
) -> Decimal:
    raw = effect[field]
    if subfield is not None:
        if not isinstance(raw, Mapping):
            fail(f"{where}.{field} must be an object")
        raw = raw[subfield]
    return _number(raw, f"{where}.{field}" + (f".{subfield}" if subfield else ""))


def _validate_within(
    value: Any,
    name: str,
    effects: Mapping[str, Mapping[str, Any]],
    where: str,
) -> Mapping[str, Any]:
    record = _exact(value, WITHIN_KEYS, where)
    hash_name, direct_name = WITHIN_CONTRASTS[name]
    _expect(record["name"], name, f"{where}.name")
    _expect(record["hash_intervention"], hash_name, f"{where}.hash_intervention")
    _expect(record["direct_intervention"], direct_name, f"{where}.direct_intervention")
    field_bindings = (
        (
            "mean_jensen_shannon_hash_minus_direct",
            "jensen_shannon_nats",
            "mean",
        ),
        (
            "mean_centered_logit_rms_hash_minus_direct",
            "centered_logit_rms_delta",
            "mean",
        ),
        (
            "top_action_flip_fraction_hash_minus_direct",
            "top_action_flip_fraction",
            None,
        ),
        (
            "mean_value_absolute_delta_hash_minus_direct",
            "value_absolute_delta",
            "mean",
        ),
        ("value_rmse_hash_minus_direct", "value_rmse", None),
    )
    for output_field, effect_field, subfield in field_bindings:
        actual = _number(record[output_field], f"{where}.{output_field}")
        left = _effect_metric(
            effects[hash_name],
            effect_field,
            subfield,
            f"{where}.hash_effect",
        )
        right = _effect_metric(
            effects[direct_name],
            effect_field,
            subfield,
            f"{where}.direct_effect",
        )
        _same_f64(
            actual,
            _float_subtract(left, right),
            f"{where}.{output_field}",
        )
    return record


def _validate_training(
    value: Any,
    intervention: str,
    g0_effect: Mapping[str, Any],
    candidate_effect: Mapping[str, Any],
    where: str,
) -> Mapping[str, Any]:
    record = _exact(value, TRAINING_KEYS, where)
    _expect(record["intervention"], intervention, f"{where}.intervention")
    bindings = (
        (
            "candidate_minus_g0_mean_jensen_shannon",
            "jensen_shannon_nats",
            "mean",
        ),
        (
            "candidate_minus_g0_mean_centered_logit_rms",
            "centered_logit_rms_delta",
            "mean",
        ),
        (
            "candidate_minus_g0_top_action_flip_fraction",
            "top_action_flip_fraction",
            None,
        ),
        (
            "candidate_minus_g0_mean_value_absolute_delta",
            "value_absolute_delta",
            "mean",
        ),
        ("candidate_minus_g0_value_rmse", "value_rmse", None),
    )
    for output_field, effect_field, subfield in bindings:
        actual = _number(record[output_field], f"{where}.{output_field}")
        candidate = _effect_metric(
            candidate_effect, effect_field, subfield, f"{where}.candidate"
        )
        g0 = _effect_metric(g0_effect, effect_field, subfield, f"{where}.g0")
        _same_f64(
            actual,
            _float_subtract(candidate, g0),
            f"{where}.{output_field}",
        )
    return record


def _validate_ingress_groups(value: Any, where: str) -> None:
    expected = {
        "state_direct": ("state_encoder.0.weight", 1499, 0, 123),
        "state_hash": ("state_encoder.0.weight", 1499, 123, 219),
        "action_direct": ("action_encoder.0.weight", 259, 0, 99),
        "action_hash": ("action_encoder.0.weight", 259, 99, 195),
    }
    groups = _index_unique(value, key="name", expected=expected, where=where)
    group_keys = {
        "name",
        "tensor_name",
        "row_count",
        "input_dim",
        "column_begin_inclusive",
        "column_end_exclusive",
        "element_count",
        "weights",
        "adam_first_moments",
        "adam_second_moments",
    }
    section_keys = {"g0", "candidate", "candidate_minus_g0"}
    stat_keys = {
        "element_count",
        "nonzero_count",
        "f32le_sha256",
        "mean",
        "mean_absolute",
        "rms",
        "max_absolute",
    }
    delta_keys = {
        "element_count",
        "changed_bit_pattern_count",
        "mean",
        "mean_absolute",
        "rms",
        "max_absolute",
    }
    for name, (tensor, input_dim, begin, end) in expected.items():
        group = _exact(groups[name], group_keys, f"{where}.{name}")
        expected_scalars = {
            "tensor_name": tensor,
            "row_count": 64,
            "input_dim": input_dim,
            "column_begin_inclusive": begin,
            "column_end_exclusive": end,
            "element_count": 64 * (end - begin),
        }
        for key, expected_value in expected_scalars.items():
            _expect(group[key], expected_value, f"{where}.{name}.{key}")
        for section_name in ("weights", "adam_first_moments", "adam_second_moments"):
            section = _exact(
                group[section_name],
                section_keys,
                f"{where}.{name}.{section_name}",
            )
            for role in ("g0", "candidate"):
                stats = _exact(
                    section[role],
                    stat_keys,
                    f"{where}.{name}.{section_name}.{role}",
                )
                _expect(
                    stats["element_count"],
                    expected_scalars["element_count"],
                    f"{where}.{name}.{section_name}.{role}.element_count",
                )
                nonzero = _natural(
                    stats["nonzero_count"],
                    f"{where}.{name}.{section_name}.{role}.nonzero_count",
                )
                if nonzero > expected_scalars["element_count"]:
                    fail(f"{where}.{name}.{section_name}.{role}.nonzero_count too large")
                _sha(
                    stats["f32le_sha256"],
                    f"{where}.{name}.{section_name}.{role}.f32le_sha256",
                )
                for key in ("mean", "mean_absolute", "rms", "max_absolute"):
                    _number(
                        stats[key],
                        f"{where}.{name}.{section_name}.{role}.{key}",
                        nonnegative=key != "mean",
                    )
            delta = _exact(
                section["candidate_minus_g0"],
                delta_keys,
                f"{where}.{name}.{section_name}.candidate_minus_g0",
            )
            _expect(
                delta["element_count"],
                expected_scalars["element_count"],
                f"{where}.{name}.{section_name}.candidate_minus_g0.element_count",
            )
            changed = _natural(
                delta["changed_bit_pattern_count"],
                (
                    f"{where}.{name}.{section_name}."
                    "candidate_minus_g0.changed_bit_pattern_count"
                ),
            )
            if changed > expected_scalars["element_count"]:
                fail(f"{where}.{name}.{section_name} changed count too large")
            for key in ("mean", "mean_absolute", "rms", "max_absolute"):
                _number(
                    delta[key],
                    f"{where}.{name}.{section_name}.candidate_minus_g0.{key}",
                    nonnegative=key != "mean",
                )


def _validate_ingress_ratios(value: Any, where: str) -> None:
    expected = {
        "state": ("state_hash", "state_direct"),
        "action": ("action_hash", "action_direct"),
    }
    ratios = _index_unique(value, key="pathway", expected=expected, where=where)
    keys = {
        "pathway",
        "hash_group",
        "direct_group",
        "candidate_weight_rms_ratio",
        "candidate_minus_g0_weight_rms_ratio",
        "candidate_adam_first_moment_rms_ratio",
        "candidate_adam_second_moment_mean_ratio",
        "candidate_adam_second_moment_rms_ratio",
    }
    for pathway, (hash_group, direct_group) in expected.items():
        record = _exact(ratios[pathway], keys, f"{where}.{pathway}")
        _expect(record["hash_group"], hash_group, f"{where}.{pathway}.hash_group")
        _expect(record["direct_group"], direct_group, f"{where}.{pathway}.direct_group")
        for key in keys - {"pathway", "hash_group", "direct_group"}:
            if record[key] is not None:
                _number(
                    record[key],
                    f"{where}.{pathway}.{key}",
                    nonnegative=True,
                )


def validate_probe(path: Path, spec: contract.PairSpec) -> ValidatedProbe:
    raw, envelope, _ = _read_raw_envelope(path)
    payload = _exact(envelope["payload"], PAYLOAD_KEYS, f"{path}.payload")
    expected_scalars = {
        "schema": contract.PROBE_PAYLOAD_SCHEMA,
        "label": contract.LABEL,
        "test_identity": contract.PROBE_TEST,
        "model_architecture_version": MODEL_ARCHITECTURE_VERSION,
        "model_config_fingerprint": MODEL_CONFIG_FINGERPRINT,
        "feature_contract_digest": FEATURE_CONTRACT_DIGEST,
        "feature_encoding_digest": FEATURE_ENCODING_DIGEST,
        "output_digest_identity": PROBE_OUTPUT_DIGEST_IDENTITY,
    }
    for key, expected in expected_scalars.items():
        _expect(payload[key], expected, f"{path}.payload.{key}")
    _expect(
        payload["run_base_seed"],
        spec.seed,
        f"{path}.payload.run_base_seed",
    )
    _sha(
        payload["aggregate_output_stream_sha256"],
        f"{path}.payload.aggregate_output_stream_sha256",
    )
    _expect(
        payload["repeat_aggregate_output_stream_bit_exact"],
        True,
        f"{path}.payload.repeat_aggregate_output_stream_bit_exact",
    )
    nonclaims = _array(payload["nonclaims"], f"{path}.payload.nonclaims")
    if not nonclaims or any(type(item) is not str or not item for item in nonclaims):
        fail(f"{path}.payload.nonclaims must be nonempty strings")

    _validate_partition(payload["feature_partition"], f"{path}.payload.feature_partition")
    _validate_corpus(payload["corpus"], f"{path}.payload.corpus")
    _validate_permutation(payload["permutation"], f"{path}.payload.permutation")
    _validate_ingress_groups(payload["ingress_groups"], f"{path}.payload.ingress_groups")
    _validate_ingress_ratios(
        payload["hash_to_direct_ingress_ratios"],
        f"{path}.payload.hash_to_direct_ingress_ratios",
    )

    checkpoints = _index_unique(
        payload["checkpoints"],
        key="role",
        expected=("g0", "candidate"),
        where=f"{path}.payload.checkpoints",
    )
    for role, generation in (("g0", 0), ("candidate", spec.candidate_generation)):
        checkpoint = _exact(
            checkpoints[role],
            CHECKPOINT_KEYS,
            f"{path}.payload.checkpoints.{role}",
        )
        _expect(
            checkpoint["generation_index"],
            generation,
            f"{path}.payload.checkpoints.{role}.generation_index",
        )
        _expect(
            checkpoint["adam_step"],
            generation,
            f"{path}.payload.checkpoints.{role}.adam_step",
        )
        for key in (
            "run_sha256",
            "identity_bundle_sha256",
            "segment_manifest_sha256",
            "boundary_head_sha256",
            "boundary_head_record_sha256",
            "checkpoint_manifest_sha256",
            "checkpoint_payload_sha256",
            "checkpoint_sidecar_sha256",
            "logical_state_sha256",
            "train_state_sha256",
            "model_parameter_sha256",
        ):
            _sha(checkpoint[key], f"{path}.payload.checkpoints.{role}.{key}")
        _natural(
            checkpoint["segment_ordinal"],
            f"{path}.payload.checkpoints.{role}.segment_ordinal",
        )
        if role == "g0":
            _expect(
                checkpoint["segment_ordinal"],
                0,
                f"{path}.payload.checkpoints.g0.segment_ordinal",
            )
            for key in (
                "parent_boundary_head_sha256",
                "last_update_evidence_sha256",
            ):
                _expect(
                    checkpoint[key],
                    None,
                    f"{path}.payload.checkpoints.g0.{key}",
                )
        else:
            for key in (
                "parent_boundary_head_sha256",
                "last_update_evidence_sha256",
            ):
                _sha(
                    checkpoint[key],
                    f"{path}.payload.checkpoints.candidate.{key}",
                )
    _expect(
        checkpoints["candidate"]["run_sha256"],
        checkpoints["g0"]["run_sha256"],
        f"{path}.payload checkpoint run identity",
    )
    _expect(
        checkpoints["candidate"]["identity_bundle_sha256"],
        checkpoints["g0"]["identity_bundle_sha256"],
        f"{path}.payload checkpoint run identity bundle",
    )
    if (
        checkpoints["candidate"]["model_parameter_sha256"]
        == checkpoints["g0"]["model_parameter_sha256"]
    ):
        fail(f"{path} candidate parameters equal generation zero")
    for key in (
        "boundary_head_sha256",
        "boundary_head_record_sha256",
        "logical_state_sha256",
    ):
        if checkpoints["candidate"][key] == checkpoints["g0"][key]:
            fail(f"{path} candidate and generation zero share {key}")

    functional_models = _index_unique(
        payload["functional_models"],
        key="role",
        expected=("g0", "candidate"),
        where=f"{path}.payload.functional_models",
    )
    effects: dict[str, dict[str, Mapping[str, Any]]] = {}
    within: dict[str, dict[str, Mapping[str, Any]]] = {}
    for role, generation in (("g0", 0), ("candidate", spec.candidate_generation)):
        model = _exact(
            functional_models[role],
            FUNCTIONAL_MODEL_KEYS,
            f"{path}.payload.functional_models.{role}",
        )
        _expect(
            model["generation_index"],
            generation,
            f"{path}.payload.functional_models.{role}.generation_index",
        )
        _expect(
            model["repeat_baseline_bit_exact"],
            True,
            f"{path}.payload.functional_models.{role}.repeat_baseline_bit_exact",
        )
        _sha(
            model["baseline_output_sha256"],
            f"{path}.payload.functional_models.{role}.baseline_output_sha256",
        )
        indexed_effects = _index_unique(
            model["effects"],
            key="intervention",
            expected=INTERVENTIONS,
            where=f"{path}.payload.functional_models.{role}.effects",
        )
        effects[role] = {
            name: _validate_effect(
                indexed_effects[name],
                name,
                f"{path}.payload.functional_models.{role}.effects.{name}",
            )
            for name in INTERVENTIONS
        }
        indexed_within = _index_unique(
            model["hash_minus_direct_contrasts"],
            key="name",
            expected=WITHIN_CONTRASTS,
            where=(
                f"{path}.payload.functional_models.{role}."
                "hash_minus_direct_contrasts"
            ),
        )
        within[role] = {
            name: _validate_within(
                indexed_within[name],
                name,
                effects[role],
                (
                    f"{path}.payload.functional_models.{role}."
                    f"hash_minus_direct_contrasts.{name}"
                ),
            )
            for name in WITHIN_CONTRASTS
        }

    indexed_training = _index_unique(
        payload["candidate_minus_g0_functional_effects"],
        key="intervention",
        expected=INTERVENTIONS,
        where=f"{path}.payload.candidate_minus_g0_functional_effects",
    )
    training = {
        name: _validate_training(
            indexed_training[name],
            name,
            effects["g0"][name],
            effects["candidate"][name],
            f"{path}.payload.candidate_minus_g0_functional_effects.{name}",
        )
        for name in INTERVENTIONS
    }

    return ValidatedProbe(
        spec=spec,
        path=path,
        report_sha256=hashlib.sha256(raw).hexdigest(),
        payload_sha256=str(envelope["payload_sha256"]),
        payload=payload,
        checkpoints=checkpoints,
        functional_models=functional_models,
        effects=effects,
        within=within,
        training=training,
    )


def load_reports(paths: Sequence[Path]) -> list[ValidatedProbe]:
    if len(paths) != len(contract.PAIR_SPECS):
        fail(
            f"exactly {len(contract.PAIR_SPECS)} --report paths are required "
            "in frozen manifest order"
        )
    probes = [
        validate_probe(path.resolve(strict=True), spec)
        for path, spec in zip(paths, contract.PAIR_SPECS, strict=True)
    ]
    for field in (
        "report_sha256",
        "payload_sha256",
    ):
        values = [getattr(probe, field) for probe in probes]
        if len(set(values)) != len(values):
            fail(f"duplicate Diagnostic B input {field}")
    run_hashes = [probe.checkpoints["g0"]["run_sha256"] for probe in probes]
    if len(set(run_hashes)) != len(run_hashes):
        fail("duplicate checkpoint run identity across fixed pairs")
    return probes


def _verify_source_record(
    value: Any,
    *,
    expected_path: Path,
    where: str,
) -> Mapping[str, Any]:
    record = _exact(value, ("path", "sha256"), where)
    if not contract.same_path(record["path"], expected_path):
        fail(f"{where}.path does not name {expected_path}")
    expected_sha = contract.sha256_file(expected_path)
    _expect(record["sha256"], expected_sha, f"{where}.sha256")
    return record


def _relative_artifact_path(
    artifact_root: Path,
    relative: Any,
    where: str,
) -> Path:
    relative_string = _string(relative, where)
    relative_path = Path(relative_string)
    if relative_path.is_absolute() or ":" in relative_string:
        fail(f"{where} must be an artifact-root-relative path")
    resolved = (artifact_root / relative_path).resolve()
    try:
        common = os.path.commonpath((str(artifact_root), str(resolved)))
    except ValueError as error:
        raise ClassificationError(f"{where} is outside the artifact root") from error
    if common != str(artifact_root):
        fail(f"{where} escapes the artifact root")
    return resolved


def _verify_artifact_file_record(
    value: Any,
    *,
    artifact_root: Path,
    where: str,
    expected_relative: str | None = None,
) -> tuple[Path, Mapping[str, Any]]:
    record = _exact(value, ("byte_count", "path", "sha256"), where)
    path_string = _string(record["path"], f"{where}.path")
    if expected_relative is not None:
        _expect(path_string, expected_relative, f"{where}.path")
    path = _relative_artifact_path(artifact_root, path_string, f"{where}.path")
    byte_count = _natural(record["byte_count"], f"{where}.byte_count")
    _sha(record["sha256"], f"{where}.sha256")
    try:
        actual_size = path.stat().st_size
    except OSError as error:
        raise ClassificationError(f"could not stat {path}: {error}") from error
    if actual_size != byte_count:
        fail(f"{where}.byte_count mismatch")
    _expect(record["sha256"], contract.sha256_file(path), f"{where}.sha256")
    return path, record


def _verify_absolute_file_record(
    value: Any,
    *,
    where: str,
) -> tuple[Path, Mapping[str, Any]]:
    record = _exact(value, ("path", "sha256"), where)
    path = Path(_string(record["path"], f"{where}.path")).resolve(strict=True)
    _sha(record["sha256"], f"{where}.sha256")
    _expect(record["sha256"], contract.sha256_file(path), f"{where}.sha256")
    return path, record


def _completion_contract_binding() -> dict[str, Any]:
    return {
        "feature_contract_digest": FEATURE_CONTRACT_DIGEST,
        "feature_encoding_digest": FEATURE_ENCODING_DIGEST,
        "feature_partition": {
            "state_feature_dim": 219,
            "state_direct_range": [0, 123],
            "state_observation_hash_range": [123, 219],
            "action_feature_dim": 195,
            "action_direct_range": [0, 99],
            "action_legal_hash_range": [99, 195],
            "hash_feature_dim_each": 96,
            "state_encoder_first_weight_shape": [64, 1499],
            "action_encoder_first_weight_shape": [64, 259],
            "structured_explicit_inputs_are_a_separate_bucket": True,
        },
        "model_architecture_version": MODEL_ARCHITECTURE_VERSION,
        "model_config_fingerprint": MODEL_CONFIG_FINGERPRINT,
        "permutation_contract": _expected_permutation_contract(),
    }


def _validate_completion_corpus_binding(value: Any, where: str) -> Mapping[str, Any]:
    _validate_corpus(value, where)
    assert isinstance(value, Mapping)
    return value


def _validate_receipt_checkpoint(
    value: Any,
    *,
    role: str,
    generation: int,
    where: str,
) -> Mapping[str, Any]:
    record = _exact(value, CHECKPOINT_KEYS, where)
    _expect(record["role"], role, f"{where}.role")
    _expect(record["generation_index"], generation, f"{where}.generation_index")
    _expect(record["adam_step"], generation, f"{where}.adam_step")
    for key in (
        "run_sha256",
        "identity_bundle_sha256",
        "segment_manifest_sha256",
        "boundary_head_sha256",
        "boundary_head_record_sha256",
        "checkpoint_manifest_sha256",
        "checkpoint_payload_sha256",
        "checkpoint_sidecar_sha256",
        "logical_state_sha256",
        "train_state_sha256",
        "model_parameter_sha256",
    ):
        _sha(record[key], f"{where}.{key}")
    _natural(record["segment_ordinal"], f"{where}.segment_ordinal")
    if role == "g0":
        _expect(record["segment_ordinal"], 0, f"{where}.segment_ordinal")
        for key in ("parent_boundary_head_sha256", "last_update_evidence_sha256"):
            _expect(record[key], None, f"{where}.{key}")
    else:
        for key in ("parent_boundary_head_sha256", "last_update_evidence_sha256"):
            _sha(record[key], f"{where}.{key}")
    return record


def _verify_invocation_receipt(
    *,
    receipt_path: Path,
    receipt_record: Mapping[str, Any],
    artifact_root: Path,
    spec: contract.PairSpec,
    summary: Mapping[str, Any],
    completion: Mapping[str, Any],
    inventory: Mapping[str, Mapping[str, Any]],
) -> tuple[Mapping[str, Any], Mapping[str, Any]]:
    receipt = contract.read_json_document(receipt_path)
    _exact(receipt, INVOCATION_RECEIPT_KEYS, f"{spec.name} invocation receipt")
    contract.verify_payload_sha256(receipt, f"{spec.name} invocation receipt")
    _expect(
        receipt_record["sha256"],
        contract.sha256_file(receipt_path),
        f"{spec.name} invocation receipt SHA-256",
    )
    _expect(
        receipt["schema"],
        contract.INVOCATION_RECEIPT_SCHEMA,
        f"{spec.name} invocation schema",
    )
    _expect(receipt["label"], contract.LABEL, f"{spec.name} invocation label")
    _expect(receipt["status"], "VALID", f"{spec.name} invocation status")
    _expect(receipt["pair"], spec.as_record(), f"{spec.name} invocation pair")
    _expect(receipt["git_head"], completion["git_head"], f"{spec.name} git_head")
    _expect(
        receipt["manifest_sha256"],
        completion["manifest"]["sha256"],
        f"{spec.name} manifest_sha256",
    )
    _expect(
        receipt["build_receipt"],
        completion["build_receipt"],
        f"{spec.name} build_receipt",
    )
    _expect(
        receipt["executable"],
        completion["executable"],
        f"{spec.name} executable",
    )
    _expect(
        receipt["command"],
        contract.probe_command(Path(str(completion["executable"]["path"]))),
        f"{spec.name} command",
    )
    _expect(receipt["exit_code"], 0, f"{spec.name} exit_code")
    _expect(receipt["timed_out"], False, f"{spec.name} timed_out")
    _expect(receipt["timeout_seconds"], 120, f"{spec.name} timeout_seconds")
    _expect(receipt["wall_time_ms"], summary["wall_time_ms"], f"{spec.name} wall_time_ms")
    _natural(receipt["wall_time_ms"], f"{spec.name} wall_time_ms")
    _string(receipt["started_utc"], f"{spec.name}.started_utc")
    _string(receipt["completed_utc"], f"{spec.name}.completed_utc")
    _expect(
        receipt["contract_environment"],
        {
            "CUDA_VISIBLE_DEVICES": "",
            "OBS_RELIANCE_CANDIDATE_GEN": str(spec.candidate_generation),
            "OBS_RELIANCE_EXPECTED_BASE_SEED": str(spec.seed),
            "OBS_RELIANCE_STORE_ROOT": spec.store_root,
        },
        f"{spec.name} contract_environment",
    )

    stream_paths: dict[str, Path] = {}
    for stream in ("stdout", "stderr"):
        expected_relative = f"runs/{spec.name}/{stream}.log"
        stream_path, stream_record = _verify_artifact_file_record(
            receipt[stream],
            artifact_root=artifact_root,
            where=f"{spec.name} invocation {stream}",
            expected_relative=expected_relative,
        )
        stream_paths[stream] = stream_path
        _expect(
            stream_record["sha256"],
            summary[f"{stream}_sha256"],
            f"{spec.name} {stream} summary SHA-256",
        )
        _expect(
            stream_record,
            inventory[expected_relative],
            f"{spec.name} {stream} inventory record",
        )

    probe = _exact(
        receipt["probe"],
        INVOCATION_PROBE_KEYS,
        f"{spec.name} invocation probe",
    )
    _expect(probe["marker_count"], 1, f"{spec.name} marker_count")
    _expect(probe["timing_line_count"], 1, f"{spec.name} timing_line_count")
    timing = _exact(
        probe["timing"],
        ("authority", "corpus", "scoring", "total"),
        f"{spec.name} timing",
    )
    for key in ("authority", "corpus", "scoring", "total"):
        _natural(timing[key], f"{spec.name} timing.{key}")
    if timing["total"] < max(timing["authority"], timing["corpus"], timing["scoring"]):
        fail(f"{spec.name} timing total is smaller than a component")
    _expect(
        probe["timing_line"],
        (
            "OBS_RELIANCE_TIMING "
            f"authority_ms={timing['authority']} "
            f"corpus_ms={timing['corpus']} "
            f"scoring_ms={timing['scoring']} "
            f"total_ms={timing['total']}"
        ),
        f"{spec.name} timing_line",
    )
    _expect(
        probe["aggregate_output_stream_sha256"],
        summary["aggregate_output_stream_sha256"],
        f"{spec.name} aggregate output SHA-256",
    )
    _sha(
        probe["aggregate_output_stream_sha256"],
        f"{spec.name} aggregate output SHA-256",
    )
    _expect(
        probe["contract_binding"],
        completion["cross_pair_invariants"]["contract_binding"],
        f"{spec.name} contract_binding",
    )
    _expect(
        probe["corpus_binding"],
        completion["cross_pair_invariants"]["corpus_binding"],
        f"{spec.name} corpus_binding",
    )

    envelope_relative = f"runs/{spec.name}/probe-envelope.json"
    payload_relative = f"runs/{spec.name}/probe-payload.json"
    envelope_path, envelope_record = _verify_artifact_file_record(
        probe["envelope"],
        artifact_root=artifact_root,
        where=f"{spec.name} probe envelope",
        expected_relative=envelope_relative,
    )
    payload_path, payload_record = _verify_artifact_file_record(
        probe["payload"],
        artifact_root=artifact_root,
        where=f"{spec.name} probe payload",
        expected_relative=payload_relative,
    )
    _expect(envelope_record, inventory[envelope_relative], f"{spec.name} envelope inventory")
    _expect(payload_record, inventory[payload_relative], f"{spec.name} payload inventory")
    _expect(
        envelope_record["sha256"],
        summary["envelope_sha256"],
        f"{spec.name} envelope summary SHA-256",
    )
    _expect(
        payload_record["sha256"],
        summary["payload_sha256"],
        f"{spec.name} payload summary SHA-256",
    )

    try:
        from . import run_probe as bound_runner
    except ImportError:  # Direct execution from this script directory.
        import run_probe as bound_runner

    try:
        parsed_output = bound_runner.parse_probe_output(
            stream_paths["stdout"].read_bytes(),
            stream_paths["stderr"].read_bytes(),
        )
    except bound_runner.contract.DiagnosticError as exc:
        fail(f"{spec.name} bound process stream is invalid: {exc}")
    _expect(
        parsed_output.envelope_raw,
        envelope_path.read_bytes(),
        f"{spec.name} stdout/envelope exact bytes",
    )
    _expect(
        parsed_output.payload_raw,
        payload_path.read_bytes(),
        f"{spec.name} stdout/payload exact bytes",
    )
    _expect(parsed_output.timing, dict(timing), f"{spec.name} stdout timing")
    _expect(
        parsed_output.timing_line,
        probe["timing_line"],
        f"{spec.name} stdout timing line",
    )
    _expect(
        parsed_output.payload["aggregate_output_stream_sha256"],
        probe["aggregate_output_stream_sha256"],
        f"{spec.name} stdout aggregate output SHA-256",
    )
    g0 = _validate_receipt_checkpoint(
        probe["g0_checkpoint"],
        role="g0",
        generation=0,
        where=f"{spec.name} g0_checkpoint",
    )
    candidate = _validate_receipt_checkpoint(
        probe["candidate_checkpoint"],
        role="candidate",
        generation=spec.candidate_generation,
        where=f"{spec.name} candidate_checkpoint",
    )
    _expect(candidate["run_sha256"], g0["run_sha256"], f"{spec.name} run identity")
    return g0, candidate


def load_completion_receipt(
    path: Path,
    *,
    require_fixed_artifact_root: bool = True,
    verify_repository: bool = True,
    classification_retry_v1: bool = False,
) -> tuple[list[ValidatedProbe], CompletionBinding]:
    completion_path = path.resolve(strict=True)
    if completion_path.name != "completion-receipt.json":
        fail("completion receipt must be named completion-receipt.json")
    artifact_root = completion_path.parent.resolve()
    if require_fixed_artifact_root and not contract.same_path(
        artifact_root, contract.ARTIFACT_ROOT_WINDOWS
    ):
        fail(
            "completion receipt must be under the frozen artifact root "
            f"{contract.ARTIFACT_ROOT_WINDOWS}"
        )
    completion = contract.read_json_document(completion_path)
    _exact(completion, COMPLETION_KEYS, "completion receipt")
    contract.verify_payload_sha256(completion, "completion receipt")
    _expect(
        completion["schema"],
        contract.COMPLETION_RECEIPT_SCHEMA,
        "completion receipt.schema",
    )
    _expect(completion["label"], contract.LABEL, "completion receipt.label")
    _expect(completion["status"], "COMPLETE", "completion receipt.status")
    _expect(
        completion["evidence_status"],
        "DIAGNOSTIC-NON-EVIDENCE",
        "completion receipt.evidence_status",
    )
    _expect(
        completion["sequential_execution"],
        True,
        "completion receipt.sequential_execution",
    )
    _expect(
        completion["git_status_clean_before_and_after"],
        True,
        "completion receipt.git_status_clean_before_and_after",
    )
    _expect(
        completion["timeout_seconds_per_pair"],
        120,
        "completion receipt.timeout_seconds_per_pair",
    )
    _natural(completion["elapsed_ms"], "completion receipt.elapsed_ms")
    _string(completion["started_utc"], "completion receipt.started_utc")
    _string(completion["completed_utc"], "completion receipt.completed_utc")
    git_head = _string(completion["git_head"], "completion receipt.git_head")
    if contract.GIT_HEAD_RE.fullmatch(git_head) is None:
        fail("completion receipt.git_head must be lower-case Git SHA-1")
    if classification_retry_v1 and not verify_repository:
        fail(
            "classification retry v1 requires repository verification"
        )
    _expect(
        completion["cpu_only"],
        {
            "cargo_no_default_features": True,
            "cuda_visible_devices": "",
            "requested_cargo_features": [],
        },
        "completion receipt.cpu_only",
    )

    repo = Path(__file__).resolve().parents[2]
    manifest_path = repo / contract.MANIFEST_RELATIVE_PATH
    _verify_source_record(
        completion["manifest"],
        expected_path=manifest_path,
        where="completion receipt.manifest",
    )
    _verify_source_record(
        completion["runner_source"],
        expected_path=repo / "scripts" / "observation_diagnostics_v1" / "run_probe.py",
        where="completion receipt.runner_source",
    )
    _verify_source_record(
        completion["contract_source"],
        expected_path=Path(contract.__file__).resolve(),
        where="completion receipt.contract_source",
    )
    if verify_repository:
        current_head = contract.require_clean_worktree(repo)
        if not classification_retry_v1:
            _expect(
                git_head,
                current_head,
                "completion receipt.git_head/current HEAD",
            )
        else:
            _expect(
                git_head,
                CLASSIFICATION_RETRY_V1_EXECUTION_GIT_HEAD,
                "completion receipt.git_head/classification retry v1 "
                "execution HEAD",
            )

    build_receipt_path, build_receipt_record = _verify_absolute_file_record(
        completion["build_receipt"],
        where="completion receipt.build_receipt",
    )
    expected_build_receipt_path = (
        artifact_root / "build" / "build-receipt.json"
    ).resolve()
    if not contract.same_path(build_receipt_path, expected_build_receipt_path):
        fail(
            "completion receipt.build_receipt.path must be "
            f"{expected_build_receipt_path}"
        )
    build_document = contract.read_json_document(build_receipt_path)
    contract.verify_payload_sha256(build_document, "bound build receipt")
    _expect(
        build_document.get("schema"),
        contract.BUILD_RECEIPT_SCHEMA,
        "bound build receipt.schema",
    )
    _expect(
        build_document.get("label"),
        contract.LABEL,
        "bound build receipt.label",
    )
    _expect(
        build_document.get("git_head"),
        git_head,
        "bound build receipt.git_head",
    )
    _expect(
        build_document.get("manifest"),
        completion["manifest"],
        "bound build receipt.manifest",
    )
    build_executable = build_document.get("executable")
    if not isinstance(build_executable, Mapping):
        fail("bound build receipt.executable must be an object")
    _expect(
        build_executable.get("path"),
        completion["executable"]["path"],
        "bound build receipt executable path",
    )
    _expect(
        build_executable.get("sha256"),
        completion["executable"]["sha256"],
        "bound build receipt executable SHA-256",
    )
    executable_path, executable_record = _verify_absolute_file_record(
        completion["executable"],
        where="completion receipt.executable",
    )
    if not executable_path.is_file():
        fail("completion receipt executable is not a regular file")
    if verify_repository:
        try:
            from . import run_probe as bound_runner
        except ImportError:  # Direct execution from this script directory.
            import run_probe as bound_runner

        verified_executable, verified_build_binding = (
            bound_runner.verify_build_receipt(
                build_document,
                receipt_path=build_receipt_path,
                repo=repo,
                manifest=manifest_path,
                head=git_head,
            )
        )
        if not contract.same_path(verified_executable, executable_path):
            fail("full build-receipt verification resolved a different executable")
        _expect(
            verified_build_binding,
            completion["build_receipt"],
            "completion/full build-receipt binding",
        )
    del build_receipt_record, executable_record

    invariants = _exact(
        completion["cross_pair_invariants"],
        (
            "contract_binding",
            "corpus_binding",
            "identical_contract_and_config_identities",
            "identical_corpus_identity_and_sha256",
        ),
        "completion receipt.cross_pair_invariants",
    )
    _expect(
        invariants["identical_contract_and_config_identities"],
        True,
        "completion receipt identical contract identities",
    )
    _expect(
        invariants["identical_corpus_identity_and_sha256"],
        True,
        "completion receipt identical corpus identities",
    )
    _expect(
        invariants["contract_binding"],
        _completion_contract_binding(),
        "completion receipt contract_binding",
    )
    _validate_completion_corpus_binding(
        invariants["corpus_binding"],
        "completion receipt corpus_binding",
    )

    expected_pairs = [spec.as_record() for spec in contract.PAIR_SPECS]
    _expect(completion["fixed_pairs"], expected_pairs, "completion receipt.fixed_pairs")
    _expect(
        completion["invocation_count"],
        len(contract.PAIR_SPECS),
        "completion receipt.invocation_count",
    )
    summaries = _array(
        completion["invocations"],
        "completion receipt.invocations",
        length=len(contract.PAIR_SPECS),
    )

    inventory_values = _array(
        completion["output_inventory"],
        "completion receipt.output_inventory",
    )
    inventory: dict[str, Mapping[str, Any]] = {}
    expected_inventory_paths = {
        f"runs/{spec.name}/{filename}"
        for spec in contract.PAIR_SPECS
        for filename in (
            "stdout.log",
            "stderr.log",
            "probe-envelope.json",
            "probe-payload.json",
            "invocation-receipt.json",
        )
    }
    for index, value in enumerate(inventory_values):
        _, record = _verify_artifact_file_record(
            value,
            artifact_root=artifact_root,
            where=f"completion receipt.output_inventory[{index}]",
        )
        relative = str(record["path"])
        if relative in inventory:
            fail(f"completion receipt.output_inventory duplicates {relative}")
        inventory[relative] = record
    if set(inventory) != expected_inventory_paths:
        fail(
            "completion receipt.output_inventory path set mismatch: "
            f"missing={sorted(expected_inventory_paths - set(inventory))} "
            f"extra={sorted(set(inventory) - expected_inventory_paths)}"
        )

    report_paths: list[Path] = []
    invocation_hashes: dict[str, str] = {}
    receipt_checkpoints: list[
        tuple[Mapping[str, Any], Mapping[str, Any]]
    ] = []
    for index, (spec, raw_summary) in enumerate(
        zip(contract.PAIR_SPECS, summaries, strict=True)
    ):
        summary = _exact(
            raw_summary,
            INVOCATION_SUMMARY_KEYS,
            f"completion receipt.invocations[{index}]",
        )
        _expect(summary["name"], spec.name, f"{spec.name} summary.name")
        _expect(summary["seed"], spec.seed, f"{spec.name} summary.seed")
        _expect(
            summary["candidate_generation"],
            spec.candidate_generation,
            f"{spec.name} summary.candidate_generation",
        )
        for key in (
            "aggregate_output_stream_sha256",
            "envelope_sha256",
            "payload_sha256",
            "stderr_sha256",
            "stdout_sha256",
        ):
            _sha(summary[key], f"{spec.name} summary.{key}")
        _natural(summary["wall_time_ms"], f"{spec.name} summary.wall_time_ms")
        receipt_relative = f"runs/{spec.name}/invocation-receipt.json"
        receipt_path, receipt_record = _verify_artifact_file_record(
            summary["invocation_receipt"],
            artifact_root=artifact_root,
            where=f"{spec.name} summary.invocation_receipt",
            expected_relative=receipt_relative,
        )
        _expect(
            receipt_record,
            inventory[receipt_relative],
            f"{spec.name} invocation receipt inventory",
        )
        receipt_checkpoints.append(
            _verify_invocation_receipt(
                receipt_path=receipt_path,
                receipt_record=receipt_record,
                artifact_root=artifact_root,
                spec=spec,
                summary=summary,
                completion=completion,
                inventory=inventory,
            )
        )
        envelope_relative = f"runs/{spec.name}/probe-envelope.json"
        report_paths.append(
            _relative_artifact_path(
                artifact_root,
                envelope_relative,
                f"{spec.name} envelope path",
            )
        )
        invocation_hashes[spec.name] = str(receipt_record["sha256"])

    probes = load_reports(report_paths)
    for probe, summary, (receipt_g0, receipt_candidate) in zip(
        probes, summaries, receipt_checkpoints, strict=True
    ):
        _expect(
            probe.payload_sha256,
            summary["payload_sha256"],
            f"{probe.spec.name} classified payload SHA-256",
        )
        _expect(
            probe.report_sha256,
            summary["envelope_sha256"],
            f"{probe.spec.name} classified envelope SHA-256",
        )
        _expect(
            probe.payload["aggregate_output_stream_sha256"],
            summary["aggregate_output_stream_sha256"],
            f"{probe.spec.name} classified output-stream SHA-256",
        )
        _expect(
            probe.checkpoints["g0"],
            receipt_g0,
            f"{probe.spec.name} g0 envelope/invocation identity",
        )
        _expect(
            probe.checkpoints["candidate"],
            receipt_candidate,
            f"{probe.spec.name} candidate envelope/invocation identity",
        )
    return probes, CompletionBinding(
        path=completion_path,
        sha256=contract.sha256_file(completion_path),
        payload_sha256=str(completion["payload_sha256"]),
        git_head=git_head,
        invocation_receipt_sha256_by_pair=invocation_hashes,
        authoritative=require_fixed_artifact_root and verify_repository,
    )


def classify_contrasts(values: Sequence[Decimal]) -> dict[str, Any]:
    if len(values) != 6:
        fail("a Diagnostic B sign read requires exactly six contrasts")
    checked = [
        _number(value, f"contrast[{index}]", canonical_f64=False)
        for index, value in enumerate(values)
    ]
    positive = sum(value > 0 for value in checked)
    negative = sum(value < 0 for value in checked)
    zero = len(checked) - positive - negative
    exact_total = sum((Fraction(value) for value in checked), Fraction(0))
    exact_pooled_mean = exact_total / len(checked)
    with localcontext() as context:
        context.prec = 80
        pooled_mean = Decimal(exact_pooled_mean.numerator) / Decimal(
            exact_pooled_mean.denominator
        )
    if positive >= 5 and exact_pooled_mean > 0:
        label = POSITIVE_LABEL
    elif negative >= 5 and exact_pooled_mean < 0:
        label = NEGATIVE_LABEL
    else:
        label = MIXED_LABEL
    return {
        "label": label,
        "positive_count": positive,
        "negative_count": negative,
        "zero_count": zero,
        "pooled_mean": pooled_mean,
    }


def _metric_values(
    probes: Sequence[ValidatedProbe],
    metric: Mapping[str, Any],
) -> tuple[list[Decimal], list[Decimal], list[Decimal]]:
    name = str(metric["within_contrast"])
    field = str(metric["within_field"])
    candidate: list[Decimal] = []
    g0: list[Decimal] = []
    changes: list[Decimal] = []
    for probe in probes:
        candidate_value = _number(
            probe.within["candidate"][name][field],
            f"{probe.path}.candidate.{name}.{field}",
        )
        g0_value = _number(
            probe.within["g0"][name][field],
            f"{probe.path}.g0.{name}.{field}",
        )
        change = _exact_decimal_subtract(candidate_value, g0_value)

        hash_intervention, direct_intervention = WITHIN_CONTRASTS[name]
        training_field = str(metric["training_field"])
        training_change = _exact_decimal_subtract(
            _number(
                probe.training[hash_intervention][training_field],
                f"{probe.path}.{hash_intervention}.{training_field}",
            ),
            _number(
                probe.training[direct_intervention][training_field],
                f"{probe.path}.{direct_intervention}.{training_field}",
            ),
        )
        # The two algebraic routes differ only by f64 operation grouping in
        # Rust.  They must agree to a conservative few-ULP envelope.
        left = float(change)
        right = float(training_change)
        scale = max(abs(left), abs(right), 1.0)
        tolerance = max(math.ulp(scale) * 16.0, 1e-15)
        if not math.isclose(left, right, rel_tol=0.0, abs_tol=tolerance):
            fail(
                f"{probe.path} candidate-minus-g0 change-of-effect "
                f"cross-check failed for {name}.{field}"
            )
        candidate.append(candidate_value)
        g0.append(g0_value)
        changes.append(change)
    return candidate, g0, changes


def _read_report(
    values: Sequence[Decimal],
    probes: Sequence[ValidatedProbe],
) -> dict[str, Any]:
    result = classify_contrasts(values)
    result["raw_contrasts"] = [
        {
            "pair": probe.spec.name,
            "arm": probe.spec.arm,
            "seed": probe.spec.seed,
            "candidate_generation": probe.spec.candidate_generation,
            "contrast": value,
        }
        for probe, value in zip(probes, values, strict=True)
    ]
    return result


def build_report(
    probes: Sequence[ValidatedProbe],
    *,
    completion_binding: CompletionBinding | None = None,
) -> dict[str, Any]:
    if len(probes) != 6:
        fail("classification requires six validated fixed probes")
    classifications: dict[str, Any] = {}
    candidate_labels: list[str] = []
    training_change_labels: list[str] = []
    cross_scope_disagreement: dict[str, bool] = {}
    for metric_name, metric in METRICS.items():
        candidate, g0, changes = _metric_values(probes, metric)
        candidate_read = _read_report(candidate, probes)
        change_read = _read_report(changes, probes)
        candidate_labels.append(candidate_read["label"])
        training_change_labels.append(change_read["label"])
        cross_scope_disagreement[metric_name] = (
            candidate_read["label"] != change_read["label"]
        )
        classifications[metric_name] = {
            "pathway": metric["pathway"],
            "candidate_within_model": candidate_read,
            "candidate_minus_g0_change_of_effect": change_read,
            "generation_zero_reference_raw_contrasts": [
                {
                    "pair": probe.spec.name,
                    "contrast": value,
                }
                for probe, value in zip(probes, g0, strict=True)
            ],
        }

    source_path = Path(__file__).resolve()
    contract_path = Path(contract.__file__).resolve()
    authoritative = (
        completion_binding is not None and completion_binding.authoritative
    )
    if completion_binding is None:
        input_mode = "unbound-direct-envelopes-dev-only"
        classification_authority = "UNBOUND-DEV-ONLY"
    elif authoritative:
        input_mode = "authoritative-launch-completion-receipt"
        classification_authority = "AUTHORITATIVE-DIAGNOSTIC-READ"
    else:
        input_mode = "non-authoritative-completion-receipt-test-only"
        classification_authority = "NONAUTHORITATIVE-TEST-ONLY"
    report: dict[str, Any] = {
        "schema": REPORT_SCHEMA,
        "label": contract.LABEL,
        "classifier_source": {
            "path": str(source_path),
            "sha256": contract.sha256_file(source_path),
        },
        "classifier_source_sha256": contract.sha256_file(source_path),
        "contract_source": {
            "path": str(contract_path),
            "sha256": contract.sha256_file(contract_path),
        },
        "execution_manifest_sha256": contract.sha256_file(
            source_path.parents[2] / contract.MANIFEST_RELATIVE_PATH
        ),
        "input_mode": input_mode,
        "classification_authority": classification_authority,
        "authoritative_pair_store_binding": authoritative,
        "completion_receipt": (
            {
                "path": str(completion_binding.path),
                "sha256": completion_binding.sha256,
                "payload_sha256": completion_binding.payload_sha256,
                "git_head": completion_binding.git_head,
            }
            if completion_binding is not None
            else None
        ),
        "inputs": [
            {
                **probe.spec.as_record(),
                "report_path": str(probe.path),
                "report_sha256": probe.report_sha256,
                "rust_payload_sha256": probe.payload_sha256,
                "run_sha256": probe.checkpoints["g0"]["run_sha256"],
                "run_base_seed": probe.payload["run_base_seed"],
                "invocation_receipt_sha256": (
                    completion_binding.invocation_receipt_sha256_by_pair[
                        probe.spec.name
                    ]
                    if completion_binding is not None
                    else None
                ),
                "g0_checkpoint_identity": dict(probe.checkpoints["g0"]),
                "candidate_checkpoint_identity": dict(
                    probe.checkpoints["candidate"]
                ),
            }
            for probe in probes
        ],
        "fixed_shared_contract": {
            "probe_envelope_schema": contract.PROBE_ENVELOPE_SCHEMA,
            "probe_payload_schema": contract.PROBE_PAYLOAD_SCHEMA,
            "test_identity": contract.PROBE_TEST,
            "model_architecture_version": MODEL_ARCHITECTURE_VERSION,
            "model_config_fingerprint": MODEL_CONFIG_FINGERPRINT,
            "feature_contract_digest": FEATURE_CONTRACT_DIGEST,
            "feature_encoding_digest": FEATURE_ENCODING_DIGEST,
            "corpus_identity": PROBE_CORPUS_IDENTITY,
            "corpus_sha256": EXPECTED_CORPUS_SHA256,
            "decision_count": 256,
            "multi_action_decision_count": 256,
            "state_donor_shift": 129,
        },
        "interpretation_rule": {
            "pair_count": 6,
            "minimum_same_strict_sign_count": 5,
            "pooled_mean_must_have_same_strict_sign": True,
            "exact_ties_count_as": "zero",
            "metric_aggregation": "independent-no-majority-vote",
        },
        "classifications": classifications,
        "metric_label_disagreement": (
            len(set(candidate_labels)) > 1
            or len(set(training_change_labels)) > 1
        ),
        "metric_label_disagreement_by_scope": {
            "candidate_within_model": len(set(candidate_labels)) > 1,
            "candidate_minus_g0_change_of_effect": (
                len(set(training_change_labels)) > 1
            ),
        },
        "candidate_vs_training_change_label_disagreement_by_metric": (
            cross_scope_disagreement
        ),
        "any_label_disagreement_across_scopes_or_metrics": (
            len(set(candidate_labels + training_change_labels)) > 1
        ),
        "global_label": None,
        "nonclaims": [
            "No metric is converted into a causal percent reliance.",
            "No majority vote or global scientific label is licensed.",
            "Object, card, edge, group, and action-reference paths remain an unperturbed third bucket.",
            "A digest effect is not proof of hidden-information leakage, collision, a representation bottleneck, memorization, or poor generalization.",
            "A small digest effect does not prove structured-encoding sufficiency.",
            "No training, game-strength, promotion, equilibrium, multi-deck, BO3, human, or pro-level-play claim is licensed.",
        ],
    }
    return attach_payload_sha256(report)


def _decimal_token(value: Decimal) -> str:
    if not value.is_finite():
        fail("cannot serialize non-finite Decimal")
    if value == 0:
        return "0"
    sign, digits_tuple, exponent = value.as_tuple()
    digits = "".join(str(digit) for digit in digits_tuple)
    if exponent >= 0:
        body = digits + "0" * exponent
    else:
        split = len(digits) + exponent
        if split > 0:
            body = digits[:split] + "." + digits[split:]
        else:
            body = "0." + "0" * (-split) + digits
        body = body.rstrip("0").rstrip(".")
    return ("-" if sign else "") + body


def canonical_json_bytes(value: Any) -> bytes:
    """Serialize canonical JSON, retaining exact finite Decimal values."""

    def encode(item: Any) -> str:
        if item is None:
            return "null"
        if item is True:
            return "true"
        if item is False:
            return "false"
        if type(item) is int:
            return str(item)
        if isinstance(item, Decimal):
            return _decimal_token(item)
        if isinstance(item, str):
            return json.dumps(item, ensure_ascii=False, separators=(",", ":"))
        if isinstance(item, list):
            return "[" + ",".join(encode(child) for child in item) + "]"
        if isinstance(item, tuple):
            return "[" + ",".join(encode(child) for child in item) + "]"
        if isinstance(item, Mapping):
            if any(type(key) is not str for key in item):
                fail("canonical JSON object keys must be strings")
            return (
                "{"
                + ",".join(
                    json.dumps(key, ensure_ascii=False)
                    + ":"
                    + encode(item[key])
                    for key in sorted(item)
                )
                + "}"
            )
        fail(f"unsupported canonical JSON value type: {type(item).__name__}")

    return encode(value).encode("utf-8")


def payload_sha256(document: Mapping[str, Any]) -> str:
    payload = dict(document)
    payload.pop("payload_sha256", None)
    return hashlib.sha256(canonical_json_bytes(payload)).hexdigest()


def attach_payload_sha256(document: dict[str, Any]) -> dict[str, Any]:
    if "payload_sha256" in document:
        fail("payload_sha256 already exists")
    document["payload_sha256"] = payload_sha256(document)
    return document


def write_report_exclusive(path: Path, report: Mapping[str, Any]) -> None:
    raw = canonical_json_bytes(report) + b"\n"
    contract.write_exclusive(path, raw)
    if contract.sha256_file(path) != hashlib.sha256(raw).hexdigest():
        fail(f"classification bytes differ after writing {path}")


def _parse_args(argv: Sequence[str] | None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    inputs = parser.add_mutually_exclusive_group(required=True)
    inputs.add_argument(
        "--completion-receipt",
        type=Path,
        help=(
            "authoritative completion-receipt.json from run_probe.py; "
            "required for an official diagnostic read"
        ),
    )
    inputs.add_argument(
        "--report",
        type=Path,
        action="append",
        help=(
            "unbound development mode only: raw Rust envelope; repeat exactly "
            "six times in the fixed manifest order"
        ),
    )
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument(
        "--classification-retry-v1",
        action="store_true",
        help="use the one frozen classification-only retry authority",
    )
    return parser.parse_args(argv)


def main(argv: Sequence[str] | None = None) -> int:
    args = _parse_args(argv)
    completion_binding: CompletionBinding | None = None
    if args.completion_receipt is not None:
        probes, completion_binding = load_completion_receipt(
            args.completion_receipt,
            classification_retry_v1=args.classification_retry_v1,
        )
    else:
        if args.classification_retry_v1:
            fail("--classification-retry-v1 requires --completion-receipt")
        probes = load_reports(args.report)
    report = build_report(probes, completion_binding=completion_binding)
    write_report_exclusive(args.output.resolve(), report)
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except KeyboardInterrupt:
        raise SystemExit(130)
    except ClassificationError as error:
        print(f"{contract.LABEL} CLASSIFICATION_ABORT: {error}", file=sys.stderr)
        raise SystemExit(1)
