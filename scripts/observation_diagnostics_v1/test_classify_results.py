from __future__ import annotations

from dataclasses import replace
from decimal import Decimal
import hashlib
import json
from pathlib import Path
import tempfile
from types import SimpleNamespace
import unittest

try:
    from . import classify_results as subject
    from . import contract
    from . import run_probe
except ImportError:  # Direct execution from this script directory.
    import classify_results as subject
    import contract
    import run_probe


def digest(marker: str) -> str:
    return hashlib.sha256(marker.encode("utf-8")).hexdigest()


def summary(value: float) -> dict[str, float]:
    return {
        "mean": value,
        "p50_nearest_rank": value,
        "p95_nearest_rank": value,
        "max": value,
    }


def effect(
    pair_index: int,
    role: str,
    intervention: str,
    *,
    action_step: int,
    state_step: int,
) -> dict[str, object]:
    js = 0.2
    centered = 0.3
    flip_count = 64
    value_rmse = 0.1
    if intervention == "action_hash_permutation":
        js += action_step * 0.001
        centered += action_step * 0.002
        flip_count += action_step
    if intervention == "state_hash_permutation":
        value_rmse += state_step * 0.001
    if intervention in ("action_hash_permutation", "action_direct_permutation"):
        value_rmse = 0.0
    flip_fraction = flip_count / 256
    return {
        "intervention": intervention,
        "intervention_output_sha256": digest(
            f"effect-{pair_index}-{role}-{intervention}"
        ),
        "policy_decision_count": 256,
        "jensen_shannon_nats": summary(js),
        "centered_logit_rms_delta": summary(centered),
        "top_action_flip_count": flip_count,
        "top_action_flip_fraction": flip_fraction,
        "baseline_top_probability_delta_baseline_minus_intervened": summary(0.0),
        "value_decision_count": 256,
        "baseline_value_exact_zero_count": 0,
        "intervened_value_exact_zero_count": 0,
        "value_zero_transition_count": 0,
        "value_absolute_delta": summary(value_rmse),
        "value_rmse": value_rmse,
        "value_sign_flip_count": 0,
        "value_sign_flip_fraction": 0.0,
    }


def effect_metric(
    record: dict[str, object], field: str, subfield: str | None
) -> float:
    value = record[field]
    if subfield is not None:
        assert isinstance(value, dict)
        value = value[subfield]
    assert isinstance(value, (int, float))
    return float(value)


def within_contrasts(
    effects: dict[str, dict[str, object]],
) -> list[dict[str, object]]:
    result: list[dict[str, object]] = []
    bindings = (
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
    for name, (hash_name, direct_name) in subject.WITHIN_CONTRASTS.items():
        record: dict[str, object] = {
            "name": name,
            "hash_intervention": hash_name,
            "direct_intervention": direct_name,
        }
        for output, field, subfield in bindings:
            record[output] = effect_metric(
                effects[hash_name], field, subfield
            ) - effect_metric(effects[direct_name], field, subfield)
        result.append(record)
    return result


def training_contrasts(
    g0: dict[str, dict[str, object]],
    candidate: dict[str, dict[str, object]],
) -> list[dict[str, object]]:
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
    result: list[dict[str, object]] = []
    for intervention in subject.INTERVENTIONS:
        record: dict[str, object] = {"intervention": intervention}
        for output, field, subfield in bindings:
            record[output] = effect_metric(
                candidate[intervention], field, subfield
            ) - effect_metric(g0[intervention], field, subfield)
        result.append(record)
    return result


def tensor_stats(marker: str, element_count: int) -> dict[str, object]:
    return {
        "element_count": element_count,
        "nonzero_count": 0,
        "f32le_sha256": digest(marker),
        "mean": 0.0,
        "mean_absolute": 0.0,
        "rms": 0.0,
        "max_absolute": 0.0,
    }


def delta_stats(element_count: int) -> dict[str, object]:
    return {
        "element_count": element_count,
        "changed_bit_pattern_count": 0,
        "mean": 0.0,
        "mean_absolute": 0.0,
        "rms": 0.0,
        "max_absolute": 0.0,
    }


def section(marker: str, element_count: int) -> dict[str, object]:
    return {
        "g0": tensor_stats(f"{marker}-g0", element_count),
        "candidate": tensor_stats(f"{marker}-candidate", element_count),
        "candidate_minus_g0": delta_stats(element_count),
    }


def ingress_groups() -> list[dict[str, object]]:
    specifications = (
        ("state_direct", "state_encoder.0.weight", 1499, 0, 123),
        ("state_hash", "state_encoder.0.weight", 1499, 123, 219),
        ("action_direct", "action_encoder.0.weight", 259, 0, 99),
        ("action_hash", "action_encoder.0.weight", 259, 99, 195),
    )
    result: list[dict[str, object]] = []
    for name, tensor, input_dim, begin, end in specifications:
        element_count = 64 * (end - begin)
        result.append(
            {
                "name": name,
                "tensor_name": tensor,
                "row_count": 64,
                "input_dim": input_dim,
                "column_begin_inclusive": begin,
                "column_end_exclusive": end,
                "element_count": element_count,
                "weights": section(f"{name}-weights", element_count),
                "adam_first_moments": section(f"{name}-m1", element_count),
                "adam_second_moments": section(f"{name}-m2", element_count),
            }
        )
    return result


def functional_model(
    pair_index: int,
    role: str,
    generation: int,
    *,
    action_step: int,
    state_step: int,
) -> tuple[dict[str, object], dict[str, dict[str, object]]]:
    effects = {
        name: effect(
            pair_index,
            role,
            name,
            action_step=action_step,
            state_step=state_step,
        )
        for name in subject.INTERVENTIONS
    }
    return (
        {
            "role": role,
            "generation_index": generation,
            "baseline_output_sha256": digest(
                f"baseline-{pair_index}-{role}"
            ),
            "repeat_baseline_bit_exact": True,
            "effects": list(effects.values()),
            "hash_minus_direct_contrasts": within_contrasts(effects),
        },
        effects,
    )


def payload_for(
    spec: contract.PairSpec,
    pair_index: int,
    *,
    candidate_action_step: int = 1,
    g0_action_step: int = 0,
    candidate_state_step: int = 1,
    g0_state_step: int = 0,
) -> dict[str, object]:
    g0_model, g0_effects = functional_model(
        pair_index,
        "g0",
        0,
        action_step=g0_action_step,
        state_step=g0_state_step,
    )
    candidate_model, candidate_effects = functional_model(
        pair_index,
        "candidate",
        spec.candidate_generation,
        action_step=candidate_action_step,
        state_step=candidate_state_step,
    )
    run_sha = digest(f"run-{pair_index}")
    checkpoints = []
    for role, generation in (
        ("g0", 0),
        ("candidate", spec.candidate_generation),
    ):
        checkpoints.append(
            {
                "role": role,
                "generation_index": generation,
                "run_sha256": run_sha,
                "identity_bundle_sha256": digest(
                    f"identity-bundle-{pair_index}-{role}"
                ),
                "segment_ordinal": generation // 64,
                "segment_manifest_sha256": digest(
                    f"segment-manifest-{pair_index}-{role}"
                ),
                "parent_boundary_head_sha256": (
                    None
                    if role == "g0"
                    else digest(f"parent-boundary-{pair_index}-{role}")
                ),
                "boundary_head_sha256": digest(
                    f"boundary-head-{pair_index}-{role}"
                ),
                "boundary_head_record_sha256": digest(
                    f"boundary-record-{pair_index}-{role}"
                ),
                "checkpoint_manifest_sha256": digest(
                    f"manifest-{pair_index}-{role}"
                ),
                "checkpoint_payload_sha256": digest(
                    f"checkpoint-payload-{pair_index}-{role}"
                ),
                "checkpoint_sidecar_sha256": digest(
                    f"checkpoint-sidecar-{pair_index}-{role}"
                ),
                "logical_state_sha256": digest(
                    f"logical-state-{pair_index}-{role}"
                ),
                "train_state_sha256": digest(
                    f"train-state-{pair_index}-{role}"
                ),
                "model_parameter_sha256": digest(
                    f"parameters-{pair_index}-{role}"
                ),
                "last_update_evidence_sha256": (
                    None
                    if role == "g0"
                    else digest(f"last-update-{pair_index}-{role}")
                ),
                "adam_step": generation,
            }
        )
    return {
        "schema": contract.PROBE_PAYLOAD_SCHEMA,
        "label": contract.LABEL,
        "test_identity": contract.PROBE_TEST,
        "model_architecture_version": subject.MODEL_ARCHITECTURE_VERSION,
        "model_config_fingerprint": subject.MODEL_CONFIG_FINGERPRINT,
        "feature_contract_digest": subject.FEATURE_CONTRACT_DIGEST,
        "feature_encoding_digest": subject.FEATURE_ENCODING_DIGEST,
        "run_base_seed": spec.seed,
        "checkpoints": checkpoints,
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
        "corpus": {
            "identity": subject.PROBE_CORPUS_IDENTITY,
            "digest_identity": subject.PROBE_CORPUS_DIGEST_IDENTITY,
            "sha256": subject.EXPECTED_CORPUS_SHA256,
            "deck_ids": ["Rally", "Rally"],
            "decision_count": 256,
            "episode_count": 4,
            "decisions_per_episode_cap": 64,
            "multi_action_decision_count": 256,
            "total_action_count": 1115,
            "base_episode_id": 880000,
            "base_environment_seed": 0x6D74672D68617368,
            "action_selection": "splitmix64-next-modulo-legal-action-count-v1",
        },
        "permutation": {
            "state_block_mapping": (
                "target-i-receives-source-(i+129)-mod-256"
            ),
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
        },
        "ingress_groups": ingress_groups(),
        "hash_to_direct_ingress_ratios": [
            {
                "pathway": pathway,
                "hash_group": f"{pathway}_hash",
                "direct_group": f"{pathway}_direct",
                "candidate_weight_rms_ratio": None,
                "candidate_minus_g0_weight_rms_ratio": None,
                "candidate_adam_first_moment_rms_ratio": None,
                "candidate_adam_second_moment_mean_ratio": None,
                "candidate_adam_second_moment_rms_ratio": None,
            }
            for pathway in ("state", "action")
        ],
        "functional_models": [g0_model, candidate_model],
        "candidate_minus_g0_functional_effects": training_contrasts(
            g0_effects, candidate_effects
        ),
        "aggregate_output_stream_sha256": digest(f"output-{pair_index}"),
        "repeat_aggregate_output_stream_bit_exact": True,
        "output_digest_identity": subject.PROBE_OUTPUT_DIGEST_IDENTITY,
        "nonclaims": ["fixture nonclaim"],
    }


def envelope_bytes(payload: dict[str, object]) -> bytes:
    payload_raw = json.dumps(
        payload,
        ensure_ascii=False,
        allow_nan=True,
        separators=(",", ":"),
    ).encode("utf-8")
    payload_sha = hashlib.sha256(payload_raw).hexdigest()
    return (
        b'{"schema":'
        + json.dumps(contract.PROBE_ENVELOPE_SCHEMA).encode("utf-8")
        + b',"payload_sha256":'
        + json.dumps(payload_sha).encode("utf-8")
        + b',"payload":'
        + payload_raw
        + b"}\n"
    )


def write_fixture(
    root: Path,
    spec: contract.PairSpec,
    pair_index: int,
    **steps: int,
) -> Path:
    path = root / f"{spec.name}.json"
    path.write_bytes(envelope_bytes(payload_for(spec, pair_index, **steps)))
    return path


def file_record(path: Path, root: Path) -> dict[str, object]:
    return {
        "byte_count": path.stat().st_size,
        "path": path.relative_to(root).as_posix(),
        "sha256": contract.sha256_file(path),
    }


def source_record(path: Path) -> dict[str, str]:
    return {"path": str(path), "sha256": contract.sha256_file(path)}


def write_completion_fixture(root: Path, *, valid_stdout: bool = True) -> Path:
    repo = Path(subject.__file__).resolve().parents[2]
    build_receipt_path = root / "build" / "build-receipt.json"
    build_receipt_path.parent.mkdir()
    executable_path = root / "build" / "probe.exe"
    executable_path.write_bytes(b"fixture-executable\n")
    head = "a" * 40
    manifest_record = source_record(repo / contract.MANIFEST_RELATIVE_PATH)
    executable_record = {
        "path": str(executable_path.resolve()),
        "sha256": contract.sha256_file(executable_path),
    }
    build_document: dict[str, object] = {
        "schema": contract.BUILD_RECEIPT_SCHEMA,
        "label": contract.LABEL,
        "git_head": head,
        "manifest": manifest_record,
        "executable": {
            "compiler_artifact_target_kind": ["lib"],
            **executable_record,
        },
    }
    contract.attach_payload_sha256(build_document)
    build_receipt_path.write_bytes(contract.record_bytes(build_document))
    build_record = {
        "path": str(build_receipt_path.resolve()),
        "sha256": contract.sha256_file(build_receipt_path),
    }
    contract_binding: dict[str, object] | None = None
    corpus_binding: dict[str, object] | None = None
    summaries: list[dict[str, object]] = []
    inventory: list[dict[str, object]] = []
    manifest_sha = contract.sha256_file(repo / contract.MANIFEST_RELATIVE_PATH)

    for index, spec in enumerate(contract.PAIR_SPECS):
        invocation_root = root / "runs" / spec.name
        invocation_root.mkdir(parents=True)
        payload = payload_for(spec, index)
        runner_identity = run_probe.verify_probe_payload(payload, spec)
        if contract_binding is None:
            contract_binding = runner_identity["contract_binding"]
        else:
            assert contract_binding == runner_identity["contract_binding"]
        if corpus_binding is None:
            corpus_binding = runner_identity["corpus_binding"]
        payload_raw = json.dumps(
            payload,
            ensure_ascii=False,
            allow_nan=False,
            separators=(",", ":"),
        ).encode("utf-8")
        payload_sha = hashlib.sha256(payload_raw).hexdigest()
        envelope_raw = (
            b'{"schema":'
            + json.dumps(contract.PROBE_ENVELOPE_SCHEMA).encode("utf-8")
            + b',"payload_sha256":'
            + json.dumps(payload_sha).encode("utf-8")
            + b',"payload":'
            + payload_raw
            + b"}"
        )
        envelope_path = invocation_root / "probe-envelope.json"
        payload_path = invocation_root / "probe-payload.json"
        stdout_path = invocation_root / "stdout.log"
        stderr_path = invocation_root / "stderr.log"
        envelope_path.write_bytes(envelope_raw)
        payload_path.write_bytes(payload_raw)
        timing = {"authority": 1, "corpus": 2, "scoring": 3, "total": 6}
        timing_line = (
            "OBS_RELIANCE_TIMING "
            "authority_ms=1 corpus_ms=2 scoring_ms=3 total_ms=6"
        )
        stdout_path.write_bytes(
            (
                b"\nrunning 1 test\n"
                + run_probe.HARNESS_MARKER_PREFIX
                + envelope_raw
                + b"\n"
                + timing_line.encode("ascii")
                + b"\nok\n"
            )
            if valid_stdout
            else b"fixture stdout\n"
        )
        stderr_path.write_bytes(b"")
        envelope_record = file_record(envelope_path, root)
        payload_record = file_record(payload_path, root)
        stdout_record = file_record(stdout_path, root)
        stderr_record = file_record(stderr_path, root)
        invocation: dict[str, object] = {
            "build_receipt": build_record,
            "command": contract.probe_command(executable_path.resolve()),
            "completed_utc": "2026-07-26T00:00:01+00:00",
            "contract_environment": {
                "CUDA_VISIBLE_DEVICES": "",
                "OBS_RELIANCE_CANDIDATE_GEN": str(
                    spec.candidate_generation
                ),
                "OBS_RELIANCE_EXPECTED_BASE_SEED": str(spec.seed),
                "OBS_RELIANCE_STORE_ROOT": spec.store_root,
            },
            "executable": executable_record,
            "exit_code": 0,
            "git_head": head,
            "label": contract.LABEL,
            "manifest_sha256": manifest_sha,
            "pair": spec.as_record(),
            "probe": {
                "aggregate_output_stream_sha256": payload[
                    "aggregate_output_stream_sha256"
                ],
                "candidate_checkpoint": runner_identity["candidate_checkpoint"],
                "contract_binding": contract_binding,
                "corpus_binding": runner_identity["corpus_binding"],
                "envelope": envelope_record,
                "g0_checkpoint": runner_identity["g0_checkpoint"],
                "marker_count": 1,
                "payload": payload_record,
                "timing": timing,
                "timing_line": timing_line,
                "timing_line_count": 1,
            },
            "schema": contract.INVOCATION_RECEIPT_SCHEMA,
            "started_utc": "2026-07-26T00:00:00+00:00",
            "status": "VALID",
            "stderr": stderr_record,
            "stdout": stdout_record,
            "timed_out": False,
            "timeout_seconds": 120,
            "wall_time_ms": 7,
        }
        contract.attach_payload_sha256(invocation)
        invocation_path = invocation_root / "invocation-receipt.json"
        invocation_path.write_bytes(contract.record_bytes(invocation))
        invocation_record = file_record(invocation_path, root)
        inventory.extend(
            (
                stdout_record,
                stderr_record,
                envelope_record,
                payload_record,
                invocation_record,
            )
        )
        summaries.append(
            {
                "aggregate_output_stream_sha256": payload[
                    "aggregate_output_stream_sha256"
                ],
                "candidate_generation": spec.candidate_generation,
                "envelope_sha256": envelope_record["sha256"],
                "invocation_receipt": invocation_record,
                "name": spec.name,
                "payload_sha256": payload_sha,
                "seed": spec.seed,
                "stderr_sha256": stderr_record["sha256"],
                "stdout_sha256": stdout_record["sha256"],
                "wall_time_ms": 7,
            }
        )
    assert corpus_binding is not None
    assert contract_binding is not None
    completion: dict[str, object] = {
        "build_receipt": build_record,
        "completed_utc": "2026-07-26T00:01:00+00:00",
        "cpu_only": {
            "cargo_no_default_features": True,
            "cuda_visible_devices": "",
            "requested_cargo_features": [],
        },
        "cross_pair_invariants": {
            "contract_binding": contract_binding,
            "corpus_binding": corpus_binding,
            "identical_contract_and_config_identities": True,
            "identical_corpus_identity_and_sha256": True,
        },
        "elapsed_ms": 42,
        "evidence_status": "DIAGNOSTIC-NON-EVIDENCE",
        "executable": executable_record,
        "fixed_pairs": [spec.as_record() for spec in contract.PAIR_SPECS],
        "git_head": head,
        "git_status_clean_before_and_after": True,
        "invocation_count": 6,
        "invocations": summaries,
        "label": contract.LABEL,
        "manifest": manifest_record,
        "output_inventory": sorted(inventory, key=lambda item: item["path"]),
        "runner_source": source_record(
            repo / "scripts" / "observation_diagnostics_v1" / "run_probe.py"
        ),
        "contract_source": source_record(Path(contract.__file__).resolve()),
        "schema": contract.COMPLETION_RECEIPT_SCHEMA,
        "sequential_execution": True,
        "started_utc": "2026-07-26T00:00:00+00:00",
        "status": "COMPLETE",
        "timeout_seconds_per_pair": 120,
    }
    contract.attach_payload_sha256(completion)
    completion_path = root / "completion-receipt.json"
    completion_path.write_bytes(contract.record_bytes(completion))
    return completion_path


class SignRuleTests(unittest.TestCase):
    def classify(self, values: list[int]) -> dict[str, object]:
        return subject.classify_contrasts([Decimal(value) for value in values])

    def test_positive_consistency(self) -> None:
        result = self.classify([1, 1, 1, 1, 1, -1])
        self.assertEqual(result["label"], subject.POSITIVE_LABEL)
        self.assertEqual(result["positive_count"], 5)
        self.assertEqual(result["negative_count"], 1)

    def test_negative_consistency(self) -> None:
        result = self.classify([-1, -1, -1, -1, -1, 1])
        self.assertEqual(result["label"], subject.NEGATIVE_LABEL)

    def test_four_of_six_is_mixed(self) -> None:
        self.assertEqual(
            self.classify([1, 1, 1, 1, -1, -1])["label"],
            subject.MIXED_LABEL,
        )

    def test_exact_ties_are_zero_and_mixed(self) -> None:
        result = self.classify([0, 0, 0, 0, 0, 0])
        self.assertEqual(result["label"], subject.MIXED_LABEL)
        self.assertEqual(result["zero_count"], 6)
        self.assertEqual(result["pooled_mean"], Decimal(0))

    def test_five_positive_requires_positive_pooled_mean(self) -> None:
        result = self.classify([1, 1, 1, 1, 1, -100])
        self.assertEqual(result["positive_count"], 5)
        self.assertLess(result["pooled_mean"], 0)
        self.assertEqual(result["label"], subject.MIXED_LABEL)

    def test_five_negative_requires_negative_pooled_mean(self) -> None:
        result = self.classify([-1, -1, -1, -1, -1, 100])
        self.assertEqual(result["negative_count"], 5)
        self.assertGreater(result["pooled_mean"], 0)
        self.assertEqual(result["label"], subject.MIXED_LABEL)

    def test_large_cancellation_uses_exact_sign_sum(self) -> None:
        result = subject.classify_contrasts(
            [
                Decimal("1e30"),
                Decimal("0.1"),
                Decimal("0.1"),
                Decimal("0.1"),
                Decimal("0.1"),
                Decimal("-1e30"),
            ]
        )
        self.assertEqual(result["positive_count"], 5)
        self.assertGreater(result["pooled_mean"], 0)
        self.assertEqual(result["label"], subject.POSITIVE_LABEL)

    def test_metric_numbers_must_be_finite_nonunderflowing_f64(self) -> None:
        with self.assertRaisesRegex(
            subject.ClassificationError,
            "representable as a finite f64",
        ):
            subject._number(Decimal("1e9999"), "overflow")
        with self.assertRaisesRegex(
            subject.ClassificationError,
            "underflow to zero",
        ):
            subject._number(Decimal("1e-9999"), "underflow")
        with self.assertRaisesRegex(
            subject.ClassificationError,
            "canonical shortest finite f64",
        ):
            subject._number(Decimal("1.00000000000000000001"), "noncanonical")

    def test_derived_decimal_contrasts_are_exact_beyond_default_precision(self) -> None:
        metric = subject.METRICS["action_centered_logit_rms_mean"]
        name = metric["within_contrast"]
        within_field = metric["within_field"]
        training_field = metric["training_field"]
        hash_intervention, direct_intervention = subject.WITHIN_CONTRASTS[name]
        probes = [
            SimpleNamespace(
                path=Path(f"fixture-{index}.json"),
                within={
                    "candidate": {
                        name: {
                            within_field: Decimal("1.0"),
                        }
                    },
                    "g0": {
                        name: {
                            within_field: Decimal("5e-324"),
                        }
                    },
                },
                training={
                    hash_intervention: {
                        training_field: Decimal("1.0"),
                    },
                    direct_intervention: {
                        training_field: Decimal("0.0"),
                    },
                },
            )
            for index in range(6)
        ]
        _, _, changes = subject._metric_values(probes, metric)
        with subject.localcontext() as context:
            context.prec = 800
            expected = Decimal("1.0") - Decimal("5e-324")
        self.assertEqual(changes[0], expected)
        self.assertNotEqual(changes[0], Decimal(1))
        self.assertEqual(
            subject.classify_contrasts(changes)["label"],
            subject.POSITIVE_LABEL,
        )

    def test_exact_expectation_rejects_boolean_integer_aliases_recursively(self) -> None:
        with self.assertRaisesRegex(subject.ClassificationError, "mismatch"):
            subject._expect({"nested": {"flag": 1}}, {"nested": {"flag": True}}, "flag")

    def test_distribution_summaries_must_be_empirically_ordered(self) -> None:
        with self.assertRaisesRegex(
            subject.ClassificationError,
            "p50_nearest_rank exceeds p95_nearest_rank",
        ):
            subject._validate_summary(
                {
                    "mean": Decimal("0.5"),
                    "p50_nearest_rank": Decimal("0.9"),
                    "p95_nearest_rank": Decimal("0.1"),
                    "max": Decimal("1.0"),
                },
                "summary",
                nonnegative=True,
            )
        with self.assertRaisesRegex(
            subject.ClassificationError,
            "mean exceeds max",
        ):
            subject._validate_summary(
                {
                    "mean": Decimal("2.0"),
                    "p50_nearest_rank": Decimal("0.1"),
                    "p95_nearest_rank": Decimal("0.9"),
                    "max": Decimal("1.0"),
                },
                "summary",
                nonnegative=True,
            )


class EnvelopeValidationTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory()
        self.root = Path(self.temporary.name)

    def tearDown(self) -> None:
        self.temporary.cleanup()

    def six(
        self,
        *,
        candidate_steps: list[int] | None = None,
        g0_steps: list[int] | None = None,
    ) -> list[Path]:
        candidate_steps = candidate_steps or [1] * 6
        g0_steps = g0_steps or [0] * 6
        return [
            write_fixture(
                self.root,
                spec,
                index,
                candidate_action_step=candidate_steps[index],
                candidate_state_step=candidate_steps[index],
                g0_action_step=g0_steps[index],
                g0_state_step=g0_steps[index],
            )
            for index, spec in enumerate(contract.PAIR_SPECS)
        ]

    def rewrite_payload(self, path: Path, mutator: object) -> None:
        raw = path.read_bytes().rstrip(b"\n")
        envelope = json.loads(raw)
        mutator(envelope["payload"])  # type: ignore[operator]
        path.write_bytes(envelope_bytes(envelope["payload"]))

    def test_valid_six_reports_classify_each_metric_and_scope(self) -> None:
        paths = self.six(candidate_steps=[1, 1, 1, 1, 1, -1])
        probes = subject.load_reports(paths)
        report = subject.build_report(probes)
        self.assertIsNone(report["global_label"])
        self.assertEqual(report["classification_authority"], "UNBOUND-DEV-ONLY")
        self.assertFalse(report["authoritative_pair_store_binding"])
        self.assertEqual(
            report["contract_source"]["sha256"],
            contract.sha256_file(Path(contract.__file__).resolve()),
        )
        self.assertEqual(len(report["classifications"]), 4)
        for metric in report["classifications"].values():
            self.assertEqual(
                metric["candidate_within_model"]["label"],
                subject.POSITIVE_LABEL,
            )
            self.assertEqual(
                metric["candidate_minus_g0_change_of_effect"]["label"],
                subject.POSITIVE_LABEL,
            )
            self.assertEqual(
                len(metric["candidate_within_model"]["raw_contrasts"]),
                6,
            )
        self.assertEqual(
            report["payload_sha256"],
            subject.payload_sha256(report),
        )

    def test_candidate_and_change_of_effect_are_separate_reads(self) -> None:
        # Candidate effects are positive in all pairs, but generation zero is
        # still larger, so training changes have the opposite sign.
        paths = self.six(candidate_steps=[1] * 6, g0_steps=[2] * 6)
        report = subject.build_report(subject.load_reports(paths))
        for metric in report["classifications"].values():
            self.assertEqual(
                metric["candidate_within_model"]["label"],
                subject.POSITIVE_LABEL,
            )
            self.assertEqual(
                metric["candidate_minus_g0_change_of_effect"]["label"],
                subject.NEGATIVE_LABEL,
            )
        self.assertFalse(report["metric_label_disagreement"])
        self.assertFalse(
            report["metric_label_disagreement_by_scope"]["candidate_within_model"]
        )
        self.assertFalse(
            report["metric_label_disagreement_by_scope"][
                "candidate_minus_g0_change_of_effect"
            ]
        )
        self.assertTrue(
            all(
                report[
                    "candidate_vs_training_change_label_disagreement_by_metric"
                ].values()
            )
        )
        self.assertTrue(report["any_label_disagreement_across_scopes_or_metrics"])

    def test_wrong_generation_is_rejected(self) -> None:
        paths = self.six()

        def mutate(payload: dict[str, object]) -> None:
            payload["checkpoints"][1]["generation_index"] += 1  # type: ignore[index,operator]

        self.rewrite_payload(paths[0], mutate)
        with self.assertRaisesRegex(subject.ClassificationError, "generation_index"):
            subject.load_reports(paths)

    def test_wrong_base_seed_is_rejected(self) -> None:
        paths = self.six()

        def mutate(payload: dict[str, object]) -> None:
            payload["run_base_seed"] = 1

        self.rewrite_payload(paths[0], mutate)
        with self.assertRaisesRegex(subject.ClassificationError, "run_base_seed"):
            subject.load_reports(paths)

    def test_duplicate_role_is_rejected(self) -> None:
        paths = self.six()

        def mutate(payload: dict[str, object]) -> None:
            payload["functional_models"][1]["role"] = "g0"  # type: ignore[index]

        self.rewrite_payload(paths[0], mutate)
        with self.assertRaisesRegex(subject.ClassificationError, "duplicate role"):
            subject.load_reports(paths)

    def test_corpus_drift_is_rejected(self) -> None:
        paths = self.six()

        def mutate(payload: dict[str, object]) -> None:
            payload["corpus"]["sha256"] = digest("drift")  # type: ignore[index]

        self.rewrite_payload(paths[0], mutate)
        with self.assertRaisesRegex(subject.ClassificationError, "corpus.sha256"):
            subject.load_reports(paths)

    def test_nonfinite_number_is_rejected(self) -> None:
        paths = self.six()

        def mutate(payload: dict[str, object]) -> None:
            model = payload["functional_models"][1]  # type: ignore[index]
            model["effects"][0]["value_rmse"] = float("nan")  # type: ignore[index]

        self.rewrite_payload(paths[0], mutate)
        with self.assertRaisesRegex(subject.ClassificationError, "non-finite"):
            subject.load_reports(paths)

    def test_duplicate_payload_is_rejected(self) -> None:
        paths = self.six()
        first = json.loads(paths[0].read_bytes())
        duplicated_run = first["payload"]["checkpoints"][0]["run_sha256"]

        def mutate(payload: dict[str, object]) -> None:
            for checkpoint in payload["checkpoints"]:  # type: ignore[index,union-attr]
                checkpoint["run_sha256"] = duplicated_run

        self.rewrite_payload(paths[2], mutate)
        with self.assertRaisesRegex(subject.ClassificationError, "duplicate"):
            subject.load_reports(paths)

    def test_payload_hash_tamper_is_rejected(self) -> None:
        paths = self.six()
        raw = paths[0].read_bytes()
        paths[0].write_bytes(raw.replace(b'"fixture nonclaim"', b'"tampered"', 1))
        with self.assertRaisesRegex(subject.ClassificationError, "SHA-256 mismatch"):
            subject.load_reports(paths)

    def test_malformed_metric_crosscheck_is_rejected(self) -> None:
        paths = self.six()

        def mutate(payload: dict[str, object]) -> None:
            model = payload["functional_models"][1]  # type: ignore[index]
            contrast = model["hash_minus_direct_contrasts"][1]  # type: ignore[index]
            contrast["value_rmse_hash_minus_direct"] = 99.0  # type: ignore[index]

        self.rewrite_payload(paths[0], mutate)
        with self.assertRaisesRegex(subject.ClassificationError, "cross-check"):
            subject.load_reports(paths)

    def test_jensen_shannon_summary_cannot_exceed_log_two(self) -> None:
        paths = self.six()

        def mutate(payload: dict[str, object]) -> None:
            model = payload["functional_models"][0]  # type: ignore[index]
            effect_record = model["effects"][0]  # type: ignore[index]
            effect_record["jensen_shannon_nats"] = summary(0.8)  # type: ignore[index]

        self.rewrite_payload(paths[0], mutate)
        with self.assertRaisesRegex(
            subject.ClassificationError,
            r"Jensen-Shannon divergence exceeds ln\(2\)",
        ):
            subject.load_reports(paths)

    def test_value_mae_cannot_exceed_rmse(self) -> None:
        paths = self.six()

        def mutate(payload: dict[str, object]) -> None:
            model = payload["functional_models"][0]  # type: ignore[index]
            effect_record = model["effects"][0]  # type: ignore[index]
            effect_record["value_absolute_delta"] = summary(0.9)  # type: ignore[index]
            effect_record["value_rmse"] = 0.1  # type: ignore[index]

        self.rewrite_payload(paths[0], mutate)
        with self.assertRaisesRegex(
            subject.ClassificationError,
            "mean absolute error exceeds RMSE",
        ):
            subject.load_reports(paths)

    def test_output_is_exclusive_canonical_json_with_payload_hash(self) -> None:
        report = subject.build_report(subject.load_reports(self.six()))
        output = self.root / "classification.json"
        subject.write_report_exclusive(output, report)
        raw = output.read_bytes()
        self.assertEqual(raw, subject.canonical_json_bytes(report) + b"\n")
        parsed = contract.parse_json_bytes(raw, str(output))
        self.assertEqual(
            parsed["payload_sha256"],
            subject.payload_sha256(parsed),
        )
        with self.assertRaisesRegex(subject.ClassificationError, "overwrite"):
            subject.write_report_exclusive(output, report)

    def test_requires_exactly_six_reports(self) -> None:
        with self.assertRaisesRegex(subject.ClassificationError, "exactly 6"):
            subject.load_reports(self.six()[:5])

    def test_relaxed_completion_load_is_explicitly_nonauthoritative(self) -> None:
        completion_path = write_completion_fixture(self.root)
        probes, binding = subject.load_completion_receipt(
            completion_path,
            require_fixed_artifact_root=False,
            verify_repository=False,
        )
        report = subject.build_report(probes, completion_binding=binding)
        self.assertFalse(binding.authoritative)
        self.assertEqual(
            report["classification_authority"],
            "NONAUTHORITATIVE-TEST-ONLY",
        )
        self.assertFalse(report["authoritative_pair_store_binding"])
        self.assertEqual(
            report["completion_receipt"]["sha256"],
            contract.sha256_file(completion_path),
        )
        self.assertTrue(
            all(
                item["invocation_receipt_sha256"] is not None
                for item in report["inputs"]
            )
        )
        self.assertIn("boundary_head_sha256", report["inputs"][0]["g0_checkpoint_identity"])
        authoritative_report = subject.build_report(
            probes,
            completion_binding=replace(binding, authoritative=True),
        )
        self.assertEqual(
            authoritative_report["classification_authority"],
            "AUTHORITATIVE-DIAGNOSTIC-READ",
        )
        self.assertTrue(authoritative_report["authoritative_pair_store_binding"])

    def test_classifier_contract_matches_actual_runner_binding(self) -> None:
        spec = contract.PAIR_SPECS[0]
        identity = run_probe.verify_probe_payload(payload_for(spec, 0), spec)
        runner_binding = identity["contract_binding"]
        self.assertIn("permutation_contract", runner_binding)
        self.assertEqual(
            subject._completion_contract_binding(),
            runner_binding,
        )

    def test_completion_receipt_store_binding_drift_is_rejected(self) -> None:
        completion_path = write_completion_fixture(self.root)
        completion = json.loads(completion_path.read_bytes())
        invocation_record = completion["invocations"][0]["invocation_receipt"]
        invocation_path = self.root / invocation_record["path"]
        invocation = json.loads(invocation_path.read_bytes())
        invocation["contract_environment"]["OBS_RELIANCE_STORE_ROOT"] = "wrong"
        invocation.pop("payload_sha256")
        contract.attach_payload_sha256(invocation)
        invocation_path.write_bytes(contract.record_bytes(invocation))
        # Keep every enclosing hash binding internally valid so rejection is
        # specifically caused by the exact Store path contract.
        new_record = file_record(invocation_path, self.root)
        completion["invocations"][0]["invocation_receipt"] = new_record
        for index, record in enumerate(completion["output_inventory"]):
            if record["path"] == new_record["path"]:
                completion["output_inventory"][index] = new_record
        completion.pop("payload_sha256")
        contract.attach_payload_sha256(completion)
        completion_path.write_bytes(contract.record_bytes(completion))
        with self.assertRaisesRegex(
            subject.ClassificationError,
            "contract_environment",
        ):
            subject.load_completion_receipt(
                completion_path,
                require_fixed_artifact_root=False,
                verify_repository=False,
            )

    def test_authoritative_completion_reparses_bound_process_streams(self) -> None:
        completion_path = write_completion_fixture(
            self.root,
            valid_stdout=False,
        )
        with self.assertRaisesRegex(
            subject.ClassificationError,
            "OBS_RELIANCE_JSON marker",
        ):
            subject.load_completion_receipt(
                completion_path,
                require_fixed_artifact_root=False,
                verify_repository=False,
            )


if __name__ == "__main__":
    unittest.main()
