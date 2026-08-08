"""In-memory producer fixtures for packaging unit tests (not test discovery)."""

from __future__ import annotations

import hashlib
import json
import math
from typing import Any

try:
    from scripts.action_ingress_admission_v1 import contract, run_probe
except ModuleNotFoundError:
    import contract  # type: ignore[no-redef]
    import run_probe  # type: ignore[no-redef]


def _rows(metric_names: tuple[str, str]) -> list[dict[str, Any]]:
    rows: list[dict[str, Any]] = []
    remaining = 1_115
    for decision in range(256):
        count = 4 if decision < 255 else remaining
        remaining -= count
        for action in range(count):
            rows.append(
                {
                    "decision_index": decision,
                    "action_index": action,
                    metric_names[0]: 1.23456789012345e-12 + action,
                    metric_names[1]: 0.5 + action,
                }
            )
    assert remaining == 0
    return rows


def payload_for(
    spec: contract.ModelSpec,
    *,
    polarity: str = "positive",
) -> dict[str, Any]:
    if polarity == "positive":
        direct_values = (1e-9, 0.25, 64, 0.25)
        digest_values = (2e-9, 0.5, 128, 0.5)
    elif polarity == "negative":
        direct_values = (2e-9, 0.5, 128, 0.5)
        digest_values = (1e-9, 0.25, 64, 0.25)
    elif polarity == "mixed":
        direct_values = (1e-9, 0.5, 64, 0.25)
        digest_values = (2e-9, 0.25, 64, 0.25)
    else:
        raise ValueError(polarity)

    def effect(name: str, values: tuple[float, float, int, float]) -> dict[str, Any]:
        js, centered, flips, fraction = values
        return {
            "name": name,
            "output_sha256": hashlib.sha256(name.encode()).hexdigest(),
            "multi_action_decision_count": 256,
            "mean_jensen_shannon_nats": js,
            "mean_centered_logit_rms_delta": centered,
            "top_action_flip_count": flips,
            "top_action_flip_fraction": fraction,
            "value_bits_invariant": True,
        }

    direct = effect("repaired_direct_sibling_rotation", direct_values)
    digest = effect("repaired_digest_sibling_rotation", digest_values)
    contrast = {
        "mean_jensen_shannon_nats": (
            digest["mean_jensen_shannon_nats"] - direct["mean_jensen_shannon_nats"]
        ),
        "mean_centered_logit_rms_delta": (
            digest["mean_centered_logit_rms_delta"]
            - direct["mean_centered_logit_rms_delta"]
        ),
        "top_action_flip_fraction": (
            digest["top_action_flip_fraction"]
            - direct["top_action_flip_fraction"]
        ),
    }
    if spec.kind == "raw":
        model = {
            "role": spec.identity,
            "kind": "frozen-common-model-snapshot",
            "generation_index": 0,
            "model_parameter_sha256": contract.COMMON_SNAPSHOT_PARAMETER_STREAM_SHA256,
            "parameter_manifest_sha256": contract.COMMON_SNAPSHOT_PARAMETER_STREAM_SHA256,
            "initialization_seed": contract.COMMON_SNAPSHOT_SEED,
            "snapshot_identity": "mtg-kernel-python-authoritative-common-model-snapshot-v1",
            "snapshot_manifest_file_sha256": contract.COMMON_MANIFEST_SHA256,
            "snapshot_payload_sha256": contract.COMMON_PARAMETERS_SHA256,
            "named_parameter_stream_sha256": contract.COMMON_SNAPSHOT_PARAMETER_STREAM_SHA256,
            "provenance": None,
            "prior_baseline_output_digest_identity": None,
            "prior_baseline_output_sha256": None,
        }
        prior_sha = None
        prior_exact = None
    else:
        provenance = dict(spec.provenance or {})
        provenance.pop("generation_index", None)
        model = {
            "role": spec.identity,
            "kind": "validated-native-training-store-generation-zero",
            "generation_index": 0,
            "model_parameter_sha256": spec.model_parameter_sha256,
            "parameter_manifest_sha256": spec.model_parameter_sha256,
            "initialization_seed": None,
            "snapshot_identity": None,
            "snapshot_manifest_file_sha256": None,
            "snapshot_payload_sha256": None,
            "named_parameter_stream_sha256": None,
            "provenance": provenance,
            "prior_baseline_output_digest_identity": contract.OUTPUT_DIGEST_IDENTITY,
            "prior_baseline_output_sha256": spec.baseline_output_sha256,
        }
        prior_sha = spec.baseline_output_sha256
        prior_exact = True

    statistic_rows = _rows(("direct_squared_norm", "digest_squared_norm"))
    direct_sum = sum(row["direct_squared_norm"] for row in statistic_rows)
    digest_sum = sum(row["digest_squared_norm"] for row in statistic_rows)
    ingress_digest_rows = [
        {
            "decision_index": row["decision_index"],
            "action_index": row["action_index"],
            "sha256": hashlib.sha256(
                (
                    f"{spec.identity}:"
                    f"{row['decision_index']}:{row['action_index']}"
                ).encode()
            ).hexdigest(),
        }
        for row in statistic_rows
    ]
    contribution_rows = _rows(
        ("direct_contribution_rms", "digest_contribution_rms")
    )
    direct_contribution_rms = math.sqrt(
        sum(row["direct_contribution_rms"] ** 2 for row in contribution_rows)
        / 1_115
    )
    digest_contribution_rms = math.sqrt(
        sum(row["digest_contribution_rms"] ** 2 for row in contribution_rows)
        / 1_115
    )
    payload = {
        "schema": contract.PROBE_PAYLOAD_SCHEMA,
        "label": contract.LABEL,
        "test_identity": contract.PROBE_TEST,
        "model": model,
        "corpus": {
            "identity": contract.CORPUS_IDENTITY,
            "digest_identity": "sha256-framed-thirteen-native-flat-tensors-v1",
            "sha256": contract.CORPUS_SHA256,
            "expected_sha256": contract.CORPUS_SHA256,
            "decision_count": 256,
            "episode_count": 4,
            "multi_action_decision_count": 256,
            "total_action_count": 1_115,
        },
        "transform": {
            "structured_repair_identity": contract.TRANSFORM_IDENTITY,
            "slot": 69,
            "effect_boolean_rule": "retain-frozen-value-bit",
            "attacker_inclusion_rule": "include-true-one-else-positive-zero",
            "blocker_inclusion_rule": "include-true-one-else-positive-zero",
            "digest_gate_identity": contract.GATE_IDENTITY,
            "scientific_gate_modes": ["FULL", "ZERO"],
            "scaled_gate_scientific_read": False,
        },
        "gate": {
            "full_copies_digest_without_multiplication": True,
            "zero_uses_exact_positive_zero": True,
            "zero_stress_mapping": (
                "within-decision-dst-j-receives-src-(j+1)-mod-n-upstream-then-ZERO"
            ),
            "zero_stress_equals_ordinary_zero": True,
            "invalid_scale_bits_fail_closed": True,
        },
        "admission": {
            "admitted": True,
            "corpus_digest_matches": True,
            "pre_transform_binding": {
                "identity": (
                    "sha256-length-framed-retained-action-semantic-operational-"
                    "core-ref-object-scorer-projection-canonical-json-tail-v1"
                ),
                "transcript_encoding": (
                    "ordered typed rows; atom=u32be(label_len)||label||"
                    "u64be(value_len)||value; integer and f32-bit arrays are "
                    "little-endian; JSON and digest blocks are raw bytes"
                ),
                "all_rows_passed": True,
                "decision_count": 256,
                "row_count": 1_115,
                "action_reference_count": contract.CORPUS_ACTION_REFERENCE_COUNT,
                "operational_object_count": 2_000,
                "action_object_projection_count": 2_000,
                "live_session_semantics_to_core_refs_revalidated_at_capture": True,
                "live_session_semantics_to_core_refs_revalidated_pre_transform": True,
                "typed_semantics_exact": True,
                "production_v2_binding_exact": True,
                "operational_core_refs_exact": True,
                "scorer_core_refs_exact": True,
                "operational_object_to_scorer_model_object_exact": True,
                "zone_change_count_retained_in_operational_identity": True,
                "count_and_order_exact": True,
                "action_kind_exact": True,
                "action_core_exact": True,
                "action_references_exact": True,
                "canonical_model_json_exact": True,
                "canonical_model_digest_exact": True,
                "frozen_digest_tail_exact": True,
                "capture_sha256": "4" * 64,
                "revalidated_sha256": "4" * 64,
                "capture_matches_revalidation": True,
            },
            "bitwise_comparison_identity": "ieee754-f32-to_bits-exact-v1",
            "exact_forward_capture_identity": (
                "native-policy-value-net8-exact-pre-action-encoder-ingress-v1"
            ),
            "exact_forward_schema_version": "actor-relative-v5-python-4",
            "exact_forward_registry_version": (
                "rust-observation-v5-action-v5-registry-4"
            ),
            "exact_forward_contract_digest": (
                "bcc808186e40a1ad6aec679d8a386631cb1226379366a632603f0beb95b47396"
            ),
            "exact_forward_encoding_digest": (
                "918e57a0796807e84310026de48d30b500813ef37d939462ea85b7255a39111c"
            ),
            "exact_forward_hidden_dim": 64,
            "exact_forward_schema_matches_frozen_contract": True,
            "exact_forward_capture_decision_count": 256,
            "exact_forward_pooled_value_count": 1_115 * 64,
            "canonical_semantics_pairwise_distinct": True,
            "repaired_zero_ingress_pairwise_distinct": True,
            "semantic_inclusion_pairs_complete_one_to_one": True,
            "semantic_inclusion_pair_direct_slot69_only": True,
            "semantic_inclusion_pair_pooled_refs_bit_exact": True,
            "repaired_zero_ingress_dim": 163,
            "repaired_zero_ingress_row_count": 1_115,
            "repaired_zero_ingress_sha256": hashlib.sha256(
                spec.identity.encode()
            ).hexdigest(),
            "repaired_zero_ingress_row_digest_identity": "sha256-f32le-163-v1",
            "repaired_zero_ingress_row_digests": ingress_digest_rows,
            "attacker_false_true_pair_count": 3,
            "blocker_false_true_pair_count": 2,
            "attacker_pairs_witnessed": True,
            "blocker_pairs_witnessed": True,
            "non_action_tensors_bit_exact": True,
            "zero_stress_bit_exact": True,
            "zero_stress_tensors_bit_exact": True,
            "zero_stress_outputs_bit_exact": True,
            "every_action_only_intervention_value_bits_invariant": True,
            "model_parameters_bit_exact_before_after": True,
        },
        "input_statistics": {
            "source_condition": "repaired/FULL",
            "direct_value_count": 1_115 * 99,
            "digest_value_count": 1_115 * 96,
            "direct_value_rms": math.sqrt(direct_sum / (1_115 * 99)),
            "digest_value_rms": math.sqrt(digest_sum / (1_115 * 96)),
            "mean_direct_squared_norm": direct_sum / 1_115,
            "mean_digest_squared_norm": digest_sum / 1_115,
            "per_action_row": statistic_rows,
        },
        "first_layer_contribution_rms": {
            "source_condition": "repaired/FULL",
            "tensor_name": "action_encoder.0.weight",
            "accumulator": (
                "exact-positive-zero-f32-forward-column-order-bias-excluded"
            ),
            "hidden_dim": 64,
            "direct_contribution_rms": direct_contribution_rms,
            "digest_contribution_rms": digest_contribution_rms,
            "per_action_row": contribution_rows,
        },
        "effects": {
            "direct_sibling": direct,
            "digest_sibling": digest,
            "repaired_full_vs_repaired_zero": effect(
                "repaired_full_vs_repaired_zero", (0.1, 0.2, 32, 0.125)
            ),
            "digest_minus_direct": contrast,
            "descriptive_label": run_probe._expected_label(spec.identity, contrast),
        },
        "output_digests": {
            "digest_identity": contract.OUTPUT_DIGEST_IDENTITY,
            "baseline_frozen_full": "1" * 64,
            "repaired_full": "2" * 64,
            "repaired_zero": "3" * 64,
            "prior_baseline_reproduced_sha256": prior_sha,
            "prior_baseline_exact_match": prior_exact,
            "repeated_baseline_frozen_full_bit_exact": True,
            "repeated_repaired_full_bit_exact": True,
            "repeated_repaired_zero_bit_exact": True,
            "zero_stress_equals_repaired_zero": True,
            "repair_only_value_bits_invariant": True,
        },
        "nonclaims": ["fixture-only nonclaim"],
    }
    return payload


def probe_streams(payload: dict[str, Any]) -> tuple[bytes, bytes, bytes, bytes]:
    payload_raw = json.dumps(
        payload,
        ensure_ascii=False,
        allow_nan=False,
        separators=(",", ":"),
    ).encode()
    payload_sha = hashlib.sha256(payload_raw).hexdigest()
    envelope = {
        "schema": contract.PROBE_ENVELOPE_SCHEMA,
        "payload_sha256": payload_sha,
        "payload": payload,
    }
    envelope_raw = json.dumps(
        envelope,
        ensure_ascii=False,
        allow_nan=False,
        separators=(",", ":"),
    ).encode()
    stdout = (
        b"\nrunning 1 test\n"
        + run_probe.HARNESS_PREFIX
        + envelope_raw
        + b"\nok\n\ntest result: ok. 1 passed; 0 failed\n"
    )
    return stdout, b"", envelope_raw, payload_raw
