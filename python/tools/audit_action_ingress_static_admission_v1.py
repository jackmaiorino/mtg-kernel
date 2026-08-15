#!/usr/bin/env python3
"""Audit the additive static portion of Net8 action-ingress admission v1.

The checked 115-case action golden remains the frozen encoding authority.  A
single player-target witness is derived from its primary effect-target case,
classified and encoded through the frozen feature module, and then the
kind-conditioned Boolean slot-69 counterfactual is applied to cloned action
rows.  This tool does not import Torch, construct a model, or train.
"""

from __future__ import annotations

import argparse
import copy
import importlib.util
import json
import math
import struct
import sys
from collections import defaultdict
from pathlib import Path
from typing import Any, Iterable


REPO_ROOT = Path(__file__).resolve().parents[2]
AUDIT_PATH = REPO_ROOT / "python" / "tools" / "audit_feature_coverage_collision_v1.py"
ACTION_GOLDEN_PATH = (
    REPO_ROOT / "data" / "flat_policy_v2" / "python_action_features_v2.json"
)
OUTPUT_PATH = (
    REPO_ROOT
    / "data"
    / "action_ingress_admission_v1"
    / "static-admission.json"
)

SCHEMA = "mtg-kernel-action-ingress-static-admission/v1"
POSITIVE_MARKER = "ACTION_INGRESS_STATIC_ADMISSION_V1_OK"
ADMITTED_STATUS = "STATIC-ADMITTED"
STATUS_PRECEDENCE = (
    "INVALID",
    "COLLISION-DETECTED",
    "COVERAGE-INCOMPLETE",
    "STRUCTURED-ALIAS-DETECTED",
    "INVARIANT-FAILED",
    ADMITTED_STATUS,
)
TRANSFORM_IDENTITY = (
    "net8-action-kind-conditioned-boolean-slot69-counterfactual-v1"
)
COMBINED_CORPUS_IDENTITY = "net8-action-ingress-static-checked-116-v1"
EXPECTED_COMBINED_IDENTITY_SHA256 = (
    "7a8cc702393253fdb2dfe61bcca648cbc8684cd1527e0d30a37b242baeb20a6e"
)
SUPPLEMENTAL_NAME = (
    "supplemental-primary-choose-effect-target-player-self-v1"
)
BASE_CASE_NAME = "primary-choose_effect_target"
ACTION_DIM = 195
ACTION_DIRECT_DIM = 99
ACTION_DIGEST_DIM = 96
ACTION_REF_DIM = 25
BOOLEAN_SLOT = 69

EXPECTED_FROZEN_SHA256 = {
    # Live-file pin: the ratified CURRENT profile of features.py (commit
    # 92edf3b, "Fix historical stack-source encoder arena collision"). The
    # HISTORICAL profile this pin previously named was
    # "fce419176dbd15e2b911e5c5f688bb390e731e3817da142571f38b1a7cc778eb";
    # that commit's fix is confined to _NodeRegistry's state-side stack
    # source registration in _objects(), which this audit's supplemental
    # case never calls (it drives _action_features/_canonical_model_value
    # directly against a synthetic classifier_input with no stack), and it
    # leaves ENCODING_CONTRACT_VERSION/FEATURE_CONTRACT_DIGEST_V1 unchanged
    # (see that commit message). The other entries in this dict remain the
    # HISTORICAL sealed reference artifacts this profile's corpus/coverage
    # pins were generated against; only this one entry tracks the live file.
    "python/mtg_kernel_rl/features.py": (
        "5d82f5b87a6819076c903390230015da456f914828890d9c5384af410f21be1c"
    ),
    "python/mtg_kernel_rl/model.py": (
        "2e3e830d4212b8c8f8085861b2508c49a6d7192b9621cef087dd396e22d12c59"
    ),
    "data/common_model_snapshot_v1/manifest.json": (
        "d5d296f5d4ee1f7e40a6005f1e1dd328b2885f6b95f0c6968c6bf1b87351c7cc"
    ),
    "data/common_model_snapshot_v1/parameters.f32le": (
        "79f715b11ccce80ac66cc832bfdc0c963a8a20f27f7b492fdfbb433c008a90a5"
    ),
    "data/flat_policy_v2/python_action_features_v2.json": (
        "6fab4b246b052e6b8404520d945b630e24ce60323a8fa4c6e78fdc17d3f9a3b8"
    ),
}
EXPECTED_EQUIVALENCE_GROUPS = [
    ["actor-p0-relative-self", "actor-p1-relative-self", "metadata-invariance-a", "metadata-invariance-b"],
    ["boolean-optional-use-true", "primary-choose_optional_cost_use"],
    ["optional-choice-SacrificeLand", "primary-choose_optional_cost_which"],
]
EXPECTED_FROZEN_STRUCTURED_ALIAS_CASES = [
    ["boolean-attacker-false", "boolean-attacker-true"],
    ["boolean-blocker-false", "boolean-blocker-true"],
]
# The frozen action golden (data/flat_policy_v2/python_action_features_v2.json,
# pinned above, untouched) bakes in its own "authority_sha256" field: the
# exact features.py hash it was generated under. That is sealed provenance
# about the HISTORICAL profile, permanently distinct from
# EXPECTED_FROZEN_SHA256["python/mtg_kernel_rl/features.py"] above, which now
# tracks the ratified CURRENT live file. The two must not be collapsed into
# one constant: the golden's own claim about its own history does not change
# just because the live file legitimately moved forward.
EXPECTED_ACTION_GOLDEN_FEATURES_AUTHORITY_SHA256_V1 = (
    "fce419176dbd15e2b911e5c5f688bb390e731e3817da142571f38b1a7cc778eb"
)
SUPPLEMENTAL_WITNESS_ATOMS = [
    (
        "legal_action.semantic.<action_kind=choose_effect_target>."
        "target.<target_kind=player>.player"
    ),
    (
        "legal_action.semantic.<action_kind=choose_effect_target>."
        "target.<target_kind=player>.target_kind"
    ),
]


class AdmissionError(RuntimeError):
    """A frozen authority or static-admission invariant is malformed."""


def _load_audit_module() -> Any:
    name = "_action_ingress_static_admission_audit_dependency"
    spec = importlib.util.spec_from_file_location(name, AUDIT_PATH)
    if spec is None or spec.loader is None:
        raise AdmissionError("failed to load feature coverage audit dependency")
    module = importlib.util.module_from_spec(spec)
    sys.modules[name] = module
    try:
        spec.loader.exec_module(module)
    finally:
        sys.modules.pop(name, None)
    return module


def _relative_seat(value: str, actor: str) -> int:
    if value == actor:
        return 0
    if {value, actor} == {"p0", "p1"}:
        return 1
    raise AdmissionError(f"invalid actor-relative seat: value={value!r} actor={actor!r}")


def _f32_bytes(values: Iterable[float], *, label: str) -> bytes:
    output = bytearray()
    for index, value in enumerate(values):
        if not math.isfinite(float(value)):
            raise AdmissionError(f"{label}[{index}]: non-finite float")
        output.extend(struct.pack("<f", float(value)))
    return bytes(output)


def _changed_f32_coordinates(left: bytes, right: bytes) -> list[int]:
    if len(left) != len(right) or len(left) % 4:
        raise AdmissionError("cannot compare differently shaped f32 payloads")
    return [
        index
        for index in range(len(left) // 4)
        if left[index * 4 : index * 4 + 4]
        != right[index * 4 : index * 4 + 4]
    ]


def adapt_action_features(
    full_feature_bytes: bytes,
    canonical_action: dict[str, Any],
    *,
    repair: bool,
) -> bytes:
    """Clone a frozen row and optionally apply the exact slot-69 repair."""

    if len(full_feature_bytes) != ACTION_DIM * 4:
        raise AdmissionError("action feature row does not have 195 f32 values")
    output = bytearray(full_feature_bytes)
    if not repair:
        return bytes(output)
    semantic = canonical_action.get("semantic")
    if not isinstance(semantic, dict):
        raise AdmissionError("canonical action lacks semantic object")
    kind = semantic.get("action_kind")
    if kind in ("choose_attacker_inclusion", "choose_blocker_inclusion"):
        include = semantic.get("include")
        if type(include) is not bool:
            raise AdmissionError(f"{kind}: include must be a canonical Boolean")
        bits = 0x3F80_0000 if include else 0
        struct.pack_into("<I", output, BOOLEAN_SLOT * 4, bits)
    return bytes(output)


def _validate_frozen_inputs(audit: Any, features: Any, action: dict[str, Any]) -> list[dict[str, Any]]:
    inputs: list[dict[str, Any]] = []
    for relative_path, expected_sha256 in EXPECTED_FROZEN_SHA256.items():
        path = REPO_ROOT / relative_path
        if not path.is_file():
            raise AdmissionError(f"missing frozen authority: {relative_path}")
        actual_sha256 = audit._sha256(path.read_bytes())
        if actual_sha256 != expected_sha256:
            raise AdmissionError(
                f"{relative_path}: frozen SHA-256 mismatch: "
                f"expected={expected_sha256} actual={actual_sha256}"
            )
        inputs.append({"path": relative_path, "sha256": actual_sha256})

    if action.get("schema") != "mtg-kernel-python-action-features-golden/v2":
        raise AdmissionError("unexpected action golden schema")
    audit._validate_payload_hash(action, label="action golden")
    audit._validate_unique_case_names(action, label="action golden")
    if len(action.get("cases", [])) != 115:
        raise AdmissionError("frozen action corpus must contain exactly 115 cases")
    if action.get("authority") != "python/mtg_kernel_rl/features.py":
        raise AdmissionError("action golden has unexpected authority path")
    if action.get("authority_sha256") != EXPECTED_ACTION_GOLDEN_FEATURES_AUTHORITY_SHA256_V1:
        raise AdmissionError("action golden feature-authority binding drifted")
    expected_dimensions = {
        "action": ACTION_DIM,
        "action_explicit": ACTION_DIRECT_DIM,
        "action_hash": ACTION_DIGEST_DIM,
        "action_ref": ACTION_REF_DIM,
    }
    if action.get("dimensions") != expected_dimensions:
        raise AdmissionError("action golden dimensions drifted")
    expected_contracts = {
        "encoding_contract_version": features.ENCODING_CONTRACT_VERSION,
        "feature_registry_version": features.FEATURE_REGISTRY_VERSION,
        "feature_schema_version": features.FEATURE_SCHEMA_VERSION,
        "model_contract_version": features.MODEL_CONTRACT_VERSION,
    }
    if action.get("python_contracts") != expected_contracts:
        raise AdmissionError("action golden Python contract identity drifted")
    return inputs


def _supplemental_case(
    audit: Any,
    features: Any,
    base_case: dict[str, Any],
) -> tuple[dict[str, Any], dict[str, Any]]:
    base_canonical_text = base_case.get("canonical_json")
    if type(base_canonical_text) is not str:
        raise AdmissionError(f"{BASE_CASE_NAME}: missing canonical JSON")
    base_canonical = audit._loads_json_strict(
        base_canonical_text,
        label=f"{BASE_CASE_NAME}.canonical_json",
    )
    semantic = base_canonical.get("semantic")
    expected_base = {
        "action_kind": "choose_effect_target",
        "actor": "self",
        "max_targets": 3,
        "min_targets": 1,
        "selected_count": 1,
    }
    if not isinstance(semantic, dict) or any(
        semantic.get(key) != value for key, value in expected_base.items()
    ):
        raise AdmissionError("primary effect-target base semantics drifted")
    if semantic.get("source") != {
        "card_db_id": 40,
        "controller": "self",
        "owner": "self",
        "zone": "Hand",
    }:
        raise AdmissionError("primary effect-target source drifted")

    expected_canonical = copy.deepcopy(base_canonical)
    expected_canonical["semantic"]["target"] = {
        "player": "self",
        "target_kind": "player",
    }

    actor = "p0"
    raw_source = {
        "arena_id": 10,
        "card_db_id": 40,
        "owner": actor,
        "controller": actor,
        "zone": "Hand",
        "zone_change_count": 0,
    }
    classifier_input = {
        "schema_version": 5,
        "selected_index": 0,
        "stable_id": "legal-action-v5:native-flat-golden",
        "display_text": "ignored metadata",
        "semantic": {
            "action_kind": "choose_effect_target",
            "actor": actor,
            "source": raw_source,
            "target": {"target_kind": "player", "player": actor},
            "selected_count": semantic["selected_count"],
            "min_targets": semantic["min_targets"],
            "max_targets": semantic["max_targets"],
        },
    }
    try:
        features.assert_action_classified(classifier_input)
        registry = features._NodeRegistry(actor, current_turn=0)
        registry.add_ref_node(raw_source, "private_context", 0, "private")
        action_features, ref_features, ref_tokens, ref_nodes = (
            features._action_features(classifier_input, actor, registry)
        )
        encoded_canonical = features._canonical_model_value(
            classifier_input,
            features.LEGAL_ACTION_SPEC,
            ("legal_action",),
            features._CanonicalContext(actor),
        )
    except Exception as exc:
        raise AdmissionError(
            "frozen classifier/encoder rejected supplemental case"
        ) from exc
    if encoded_canonical != expected_canonical:
        raise AdmissionError("supplemental canonical derivation drifted")

    canonical = audit.canonical_bytes(encoded_canonical)
    action_bytes = _f32_bytes(action_features, label="supplemental.action_features")
    ref_bytes = _f32_bytes(
        (value for row in ref_features for value in row),
        label="supplemental.action_ref_features",
    )
    if len(action_bytes) != ACTION_DIM * 4:
        raise AdmissionError("supplemental action feature width drifted")
    if len(ref_features) != 1 or len(ref_bytes) != ACTION_REF_DIM * 4:
        raise AdmissionError("supplemental action-reference tensor shape drifted")
    if ref_tokens != [41] or ref_nodes != [0]:
        raise AdmissionError("supplemental action-reference identity drifted")
    blocks = audit._digest_blocks("legal-action", canonical)
    quantized_tail = audit._digest_f32_tail("legal-action", canonical)
    if action_bytes[ACTION_DIRECT_DIM * 4 :] != quantized_tail:
        raise AdmissionError("supplemental encoded digest tail mismatch")

    base_flat = base_case.get("flat_input")
    if not isinstance(base_flat, dict):
        raise AdmissionError("primary effect-target flat input is missing")
    source_refs = [
        copy.deepcopy(row)
        for row in base_flat.get("refs", [])
        if row.get("projection_role_id") == 0
    ]
    if len(source_refs) != 1:
        raise AdmissionError("primary effect-target must have one source ref")
    source_ref = source_refs[0]
    source_index = source_ref.get("model_object_index")
    objects = base_flat.get("objects")
    if (
        type(source_index) is not int
        or not isinstance(objects, list)
        or source_index < 0
        or source_index >= len(objects)
    ):
        raise AdmissionError("primary effect-target source object index is invalid")
    source_ref["model_object_index"] = 0
    source_ref["action_index"] = 0
    core = copy.deepcopy(base_flat.get("core"))
    if not isinstance(core, dict):
        raise AdmissionError("primary effect-target flat core is missing")
    core.update(
        {
            "target_kind": 1,
            "target_player": 1,
            "ref_start": 0,
            "ref_len": 1,
        }
    )
    flat_input = {
        "core": core,
        "objects": [copy.deepcopy(objects[source_index])],
        "refs": [source_ref],
    }
    if (
        core["target_kind"],
        core["target_player"],
        core["ref_len"],
    ) != (1, 1, 1):
        raise AdmissionError("supplemental flat-core target encoding drifted")
    if source_ref.get("card_token") != ref_tokens[0]:
        raise AdmissionError("supplemental source token disagrees with encoder")

    case = {
        "action_ref_card_ids": ref_tokens,
        "action_ref_feature_f32_le_hex": ref_bytes.hex(),
        "action_ref_node_indices": ref_nodes,
        "canonical_json": canonical.decode("utf-8"),
        "coverage": ["supplemental-player-effect-target"],
        "flat_input": flat_input,
        "full_feature_f32_le_hex": action_bytes.hex(),
        "name": SUPPLEMENTAL_NAME,
        "sha512_blocks_hex": [block.hex() for block in blocks],
    }
    report_case = {
        "canonical_json": case["canonical_json"],
        "classifier_input": classifier_input,
        "derivation": {
            "base_case": BASE_CASE_NAME,
            "retained_fields": [
                "semantic.action_kind",
                "semantic.actor",
                "semantic.source",
                "semantic.selected_count",
                "semantic.min_targets",
                "semantic.max_targets",
            ],
            "replacement": {
                "semantic.target": {
                    "player": "self",
                    "target_kind": "player",
                }
            },
        },
        "digests": {
            "quantized_tail_sha256": audit._sha256(quantized_tail),
            "raw_digest_sha256": audit._sha256(b"".join(blocks)),
            "sha512_blocks_hex": case["sha512_blocks_hex"],
        },
        "flat_input": flat_input,
        "name": SUPPLEMENTAL_NAME,
        "tensors": {
            "action_features": {
                "f32_le_hex": action_bytes.hex(),
                "shape": [ACTION_DIM],
            },
            "action_ref_card_ids": {"shape": [1], "u32_values": ref_tokens},
            "action_ref_features": {
                "f32_le_hex": ref_bytes.hex(),
                "shape": [1, ACTION_REF_DIM],
            },
            "action_ref_node_indices": {
                "shape": [1],
                "u32_values": ref_nodes,
            },
        },
    }
    return case, report_case


def _record_case(
    audit: Any,
    case: dict[str, Any],
) -> tuple[dict[str, Any], dict[str, Any]]:
    name = case.get("name")
    canonical_text = case.get("canonical_json")
    if type(name) is not str or not name or type(canonical_text) is not str:
        raise AdmissionError("action case lacks name or canonical JSON")
    canonical_value = audit._loads_json_strict(
        canonical_text,
        label=f"{name}.canonical_json",
    )
    canonical = audit.canonical_bytes(canonical_value)
    if canonical != canonical_text.encode("utf-8"):
        raise AdmissionError(f"{name}: action JSON is not canonical")
    raw_blocks = audit._digest_blocks("legal-action", canonical)
    stored_blocks = [
        audit._decode_hex(value, label=f"{name}.sha512_blocks_hex")
        for value in case.get("sha512_blocks_hex", [])
    ]
    if stored_blocks != raw_blocks:
        raise AdmissionError(f"{name}: raw action digest mismatch")
    quantized = audit._digest_f32_tail("legal-action", canonical)
    frozen = audit._decode_hex(
        case.get("full_feature_f32_le_hex"),
        label=f"{name}.full_feature_f32_le_hex",
    )
    if len(frozen) != ACTION_DIM * 4:
        raise AdmissionError(f"{name}: action tensor width mismatch")
    if frozen[ACTION_DIRECT_DIM * 4 :] != quantized:
        raise AdmissionError(f"{name}: quantized digest tail mismatch")
    if adapt_action_features(frozen, canonical_value, repair=False) != frozen:
        raise AdmissionError(f"{name}: disabled adapter changed frozen bytes")
    repaired = adapt_action_features(frozen, canonical_value, repair=True)

    frozen_case = copy.deepcopy(case)
    repaired_case = copy.deepcopy(case)
    frozen_case["full_feature_f32_le_hex"] = frozen.hex()
    repaired_case["full_feature_f32_le_hex"] = repaired.hex()
    frozen_non_feature_payload = {
        key: value
        for key, value in frozen_case.items()
        if key != "full_feature_f32_le_hex"
    }
    repaired_non_feature_payload = {
        key: value
        for key, value in repaired_case.items()
        if key != "full_feature_f32_le_hex"
    }
    if frozen_non_feature_payload != repaired_non_feature_payload:
        raise AdmissionError(f"{name}: adapter changed non-feature payload")
    frozen_structured, frozen_complete = audit._action_representation_keys(
        frozen_case,
        action_dim=ACTION_DIM,
        action_direct_dim=ACTION_DIRECT_DIM,
    )
    repaired_structured, repaired_complete = audit._action_representation_keys(
        repaired_case,
        action_dim=ACTION_DIM,
        action_direct_dim=ACTION_DIRECT_DIM,
    )
    record = {
        "canonical": canonical,
        "canonical_value": canonical_value,
        "complete": repaired_complete,
        "frozen": frozen,
        "frozen_complete": frozen_complete,
        "frozen_structured": frozen_structured,
        "name": name,
        "non_feature_payload_unchanged": True,
        "quantized": quantized,
        "raw_digest": b"".join(raw_blocks),
        "repaired": repaired,
        "structured": repaired_structured,
    }
    identity = {
        "canonical_sha256": audit._sha256(canonical),
        "frozen_action_features_sha256": audit._sha256(frozen),
        "frozen_complete_representation_sha256": audit._sha256(frozen_complete),
        "name": name,
        "quantized_tail_sha256": audit._sha256(quantized),
        "raw_digest_sha256": audit._sha256(b"".join(raw_blocks)),
        "repaired_action_features_sha256": audit._sha256(repaired),
        "repaired_complete_representation_sha256": audit._sha256(repaired_complete),
        "repaired_structured_signature_sha256": audit._sha256(
            repaired_structured
        ),
    }
    return record, identity


def _coverage(
    audit: Any,
    features: Any,
    records: list[dict[str, Any]],
) -> dict[str, Any]:
    witnesses: dict[str, Any] = defaultdict(audit.Witness)
    for record in records:
        canonical_value = record["canonical_value"]
        semantic = canonical_value.get("semantic")
        actor = semantic.get("actor") if isinstance(semantic, dict) else None
        if actor not in ("self", "opponent"):
            raise AdmissionError(f"{record['name']}: canonical actor is invalid")
        audit._walk_observed_atoms(
            features,
            canonical_value,
            features.LEGAL_ACTION_SPEC,
            ("legal_action",),
            case_name=record["name"],
            actor=actor,
            canonical=True,
            witnesses=witnesses,
        )
    return audit._coverage_report(
        features,
        audit._walk_declared_atoms(
            features,
            features.LEGAL_ACTION_SPEC,
            ("legal_action",),
        ),
        witnesses,
    )


def _coverage_summary(coverage: dict[str, Any]) -> dict[str, Any]:
    return {
        key: value
        for key, value in coverage.items()
        if key != "atoms"
    }


def _combat_repair_evidence(
    audit: Any,
    records_by_name: dict[str, dict[str, Any]],
) -> tuple[list[dict[str, Any]], bool]:
    rows: list[dict[str, Any]] = []
    valid = True
    for label in ("attacker", "blocker"):
        false_name = f"boolean-{label}-false"
        true_name = f"boolean-{label}-true"
        false_record = records_by_name.get(false_name)
        true_record = records_by_name.get(true_name)
        if false_record is None or true_record is None:
            raise AdmissionError(f"missing checked {label} Boolean pair")
        false_direct = false_record["repaired"][: ACTION_DIRECT_DIM * 4]
        true_direct = true_record["repaired"][: ACTION_DIRECT_DIM * 4]
        differing = _changed_f32_coordinates(false_direct, true_direct)
        false_changes = _changed_f32_coordinates(
            false_record["frozen"], false_record["repaired"]
        )
        true_changes = _changed_f32_coordinates(
            true_record["frozen"], true_record["repaired"]
        )
        frozen_direct_equal = (
            false_record["frozen"][: ACTION_DIRECT_DIM * 4]
            == true_record["frozen"][: ACTION_DIRECT_DIM * 4]
        )
        row_valid = (
            differing == [BOOLEAN_SLOT]
            and false_changes == []
            and true_changes == [BOOLEAN_SLOT]
            and frozen_direct_equal
            and false_record["repaired"][BOOLEAN_SLOT * 4 : BOOLEAN_SLOT * 4 + 4]
            == b"\0\0\0\0"
            and true_record["repaired"][BOOLEAN_SLOT * 4 : BOOLEAN_SLOT * 4 + 4]
            == struct.pack("<I", 0x3F80_0000)
        )
        preservation = {
            "action_references": (
                false_record["non_feature_payload_unchanged"]
                and true_record["non_feature_payload_unchanged"]
            ),
            "canonical_json": True,
            "non69_direct_bits": (
                all(index == BOOLEAN_SLOT for index in false_changes)
                and all(index == BOOLEAN_SLOT for index in true_changes)
            ),
            "quantized_digest_tail": all(
                record["frozen"][ACTION_DIRECT_DIM * 4 :]
                == record["repaired"][ACTION_DIRECT_DIM * 4 :]
                for record in (false_record, true_record)
            ),
            "raw_digest": True,
        }
        for record in (false_record, true_record):
            row_valid = row_valid and (
                record["frozen"][ACTION_DIRECT_DIM * 4 :]
                == record["repaired"][ACTION_DIRECT_DIM * 4 :]
            )
        row_valid = row_valid and all(preservation.values())
        valid = valid and row_valid
        rows.append(
            {
                "cases": [false_name, true_name],
                "false_case_changed_coordinates": false_changes,
                "frozen_direct_prefix_equal": frozen_direct_equal,
                "invariants_passed": row_valid,
                "preservation": preservation,
                "repaired_direct_differing_coordinates": differing,
                "slot69_false_f32_le_hex": false_direct[
                    BOOLEAN_SLOT * 4 : BOOLEAN_SLOT * 4 + 4
                ].hex(),
                "slot69_true_f32_le_hex": true_direct[
                    BOOLEAN_SLOT * 4 : BOOLEAN_SLOT * 4 + 4
                ].hex(),
                "true_case_changed_coordinates": true_changes,
            }
        )
    return rows, valid


def _decision_status(
    *,
    collision_count: int,
    coverage_admitted: bool,
    repaired_alias_count: int,
    invariants_passed: bool,
) -> str:
    if collision_count:
        return "COLLISION-DETECTED"
    if not coverage_admitted:
        return "COVERAGE-INCOMPLETE"
    if repaired_alias_count:
        return "STRUCTURED-ALIAS-DETECTED"
    if not invariants_passed:
        return "INVARIANT-FAILED"
    return ADMITTED_STATUS


def build_report(
    *,
    action_path: Path = ACTION_GOLDEN_PATH,
) -> dict[str, Any]:
    audit = _load_audit_module()
    try:
        features = audit._load_features_without_torch()
        action = audit._load_json_strict(action_path)
    except Exception as exc:
        if isinstance(exc, AdmissionError):
            raise
        raise AdmissionError(f"failed to load frozen action authorities: {exc}") from exc
    if not isinstance(action, dict):
        raise AdmissionError("action golden root must be a JSON object")
    if action_path.resolve() != ACTION_GOLDEN_PATH.resolve():
        raise AdmissionError("alternate action-golden paths are not admitted")
    inputs = _validate_frozen_inputs(audit, features, action)

    by_name = {case["name"]: case for case in action["cases"]}
    if len(by_name) != 115 or BASE_CASE_NAME not in by_name:
        raise AdmissionError("frozen action case identities drifted")
    supplemental, supplemental_report = _supplemental_case(
        audit,
        features,
        by_name[BASE_CASE_NAME],
    )
    if supplemental["name"] in by_name:
        raise AdmissionError("supplemental case name collides with frozen corpus")

    frozen_records: list[dict[str, Any]] = []
    combined_records: list[dict[str, Any]] = []
    identity_rows: list[dict[str, Any]] = []
    for case in action["cases"]:
        record, identity = _record_case(audit, case)
        frozen_records.append(record)
        combined_records.append(record)
        identity_rows.append(identity)
    supplemental_record, supplemental_identity = _record_case(
        audit,
        supplemental,
    )
    combined_records.append(supplemental_record)
    identity_rows.append(supplemental_identity)
    if len(combined_records) != 116:
        raise AdmissionError("combined action corpus must contain exactly 116 cases")

    frozen_coverage = _coverage(audit, features, frozen_records)
    combined_coverage = _coverage(audit, features, combined_records)
    if (
        frozen_coverage["covered_model_input_atoms"],
        frozen_coverage["declared_model_input_atoms"],
        frozen_coverage["unwitnessed_model_input_atoms"],
    ) != (200, 202, SUPPLEMENTAL_WITNESS_ATOMS):
        raise AdmissionError("frozen 115-case coverage baseline drifted")
    supplemental_atoms = sorted(
        set(frozen_coverage["unwitnessed_model_input_atoms"])
        - set(combined_coverage["unwitnessed_model_input_atoms"])
    )
    if supplemental_atoms != sorted(SUPPLEMENTAL_WITNESS_ATOMS):
        raise AdmissionError("supplemental case did not witness exactly two atoms")

    observed_equivalences = audit._equivalence_groups(combined_records)
    expected_equivalences = sorted(
        [sorted(group) for group in EXPECTED_EQUIVALENCE_GROUPS]
    )
    if observed_equivalences != expected_equivalences:
        raise AdmissionError(
            "canonical equivalence groups drifted: "
            f"expected={expected_equivalences} observed={observed_equivalences}"
        )

    collisions = {
        "raw_digest": audit._collision_groups(combined_records, "raw_digest"),
        "quantized_tail": audit._collision_groups(combined_records, "quantized"),
        "frozen_complete_representation": audit._collision_groups(
            combined_records,
            "frozen_complete",
        ),
        "repaired_complete_representation": audit._collision_groups(
            combined_records,
            "complete",
        ),
    }
    collision_count = sum(len(groups) for groups in collisions.values())
    frozen_alias_records = [
        {
            **record,
            "complete": record["frozen_complete"],
            "structured": record["frozen_structured"],
        }
        for record in combined_records
    ]
    frozen_aliases = audit._structured_alias_groups(frozen_alias_records)
    frozen_alias_cases = [alias["cases"] for alias in frozen_aliases]
    if frozen_alias_cases != EXPECTED_FROZEN_STRUCTURED_ALIAS_CASES:
        raise AdmissionError(
            "frozen structured alias baseline drifted: "
            f"expected={EXPECTED_FROZEN_STRUCTURED_ALIAS_CASES} "
            f"observed={frozen_alias_cases}"
        )
    repaired_aliases = audit._structured_alias_groups(combined_records)

    records_by_name = {record["name"]: record for record in combined_records}
    repair_rows, repair_pairs_valid = _combat_repair_evidence(
        audit,
        records_by_name,
    )
    changed_by_case = {
        record["name"]: _changed_f32_coordinates(
            record["frozen"],
            record["repaired"],
        )
        for record in combined_records
    }
    expected_changed = {
        record["name"]: [BOOLEAN_SLOT]
        for record in combined_records
        if record["canonical_value"]["semantic"]["action_kind"]
        in ("choose_attacker_inclusion", "choose_blocker_inclusion")
        and record["canonical_value"]["semantic"]["include"] is True
    }
    mutation_scope_valid = all(
        changed == expected_changed.get(name, [])
        for name, changed in changed_by_case.items()
    )
    frozen_no_repair_identity = all(
        adapt_action_features(
            record["frozen"],
            record["canonical_value"],
            repair=False,
        )
        == record["frozen"]
        for record in frozen_records
    )
    combat_case_preservation = all(
        all(row["preservation"].values()) for row in repair_rows
    )

    coverage_admitted = (
        combined_coverage["covered_model_input_atoms"] == 202
        and combined_coverage["declared_model_input_atoms"] == 202
        and combined_coverage["unwitnessed_model_input_atoms"] == []
        and combined_coverage["boolean_polarity_gaps"] == []
        and combined_coverage["optional_presence_gaps"] == []
        and combined_coverage["required_coverage_complete"] is True
    )
    invariants_passed = (
        repair_pairs_valid
        and combat_case_preservation
        and mutation_scope_valid
        and frozen_no_repair_identity
        and observed_equivalences == expected_equivalences
    )
    status = _decision_status(
        collision_count=collision_count,
        coverage_admitted=coverage_admitted,
        repaired_alias_count=len(repaired_aliases),
        invariants_passed=invariants_passed,
    )

    identity_rows.sort(key=lambda row: row["name"])
    identity_digest = audit._sha256(audit.canonical_bytes(identity_rows))
    if identity_digest != EXPECTED_COMBINED_IDENTITY_SHA256:
        raise AdmissionError(
            "combined 116-case identity SHA-256 mismatch: "
            f"expected={EXPECTED_COMBINED_IDENTITY_SHA256} "
            f"actual={identity_digest}"
        )
    source_path = Path(__file__).resolve()
    report: dict[str, Any] = {
        "admission_criteria": {
            "canonical_equivalence_allowlist_exact": (
                observed_equivalences == expected_equivalences
            ),
            "combat_case_preservation": combat_case_preservation,
            "collisions_empty": collision_count == 0,
            "coverage_exact_202_of_202": coverage_admitted,
            "frozen_no_repair_byte_identity": frozen_no_repair_identity,
            "mutation_scope_exact": mutation_scope_valid,
            "repair_pair_invariants": repair_pairs_valid,
            "repaired_structured_aliases_empty": repaired_aliases == [],
        },
        "authority": {
            "audit_dependency_path": str(AUDIT_PATH.relative_to(REPO_ROOT)).replace(
                "\\", "/"
            ),
            "audit_dependency_sha256": audit._sha256(AUDIT_PATH.read_bytes()),
            "encoding_contract_fingerprint": (
                features.encoding_contract_fingerprint()
            ),
            "feature_contract_fingerprint": (
                features.feature_contract_fingerprint()
            ),
            "static_tool_path": str(source_path.relative_to(REPO_ROOT)).replace(
                "\\", "/"
            ),
            "static_tool_sha256": audit._sha256(source_path.read_bytes()),
        },
        "canonical_equivalences": {
            "expected_groups": expected_equivalences,
            "observed_groups": observed_equivalences,
            "unexpected_groups": [],
        },
        "collisions": collisions,
        "combat_boolean_repair": {
            "case_mutations": [
                {
                    "changed_coordinates": changed_by_case[name],
                    "name": name,
                }
                for name in sorted(changed_by_case)
                if changed_by_case[name]
            ],
            "pairs": repair_rows,
        },
        "corpus": {
            "case_identity_digest_contract": (
                "sha256(canonical-json(case_identity_rows sorted "
                "lexicographically by name))"
            ),
            "case_identity_rows": identity_rows,
            "combined_case_count": len(combined_records),
            "combined_identity": COMBINED_CORPUS_IDENTITY,
            "combined_identity_sha256": identity_digest,
            "expected_combined_identity_sha256": (
                EXPECTED_COMBINED_IDENTITY_SHA256
            ),
            "frozen_case_count": len(frozen_records),
            "supplemental_case_count": 1,
        },
        "coverage": {
            "augmented_116_case": _coverage_summary(combined_coverage),
            "frozen_115_case": _coverage_summary(frozen_coverage),
            "supplemental_witness_atoms": supplemental_atoms,
        },
        "decision": {
            "precedence": list(STATUS_PRECEDENCE),
            "status": status,
        },
        "dimensions": {
            "action": ACTION_DIM,
            "action_digest": ACTION_DIGEST_DIM,
            "action_direct": ACTION_DIRECT_DIM,
            "action_ref": ACTION_REF_DIM,
            "boolean_slot": BOOLEAN_SLOT,
        },
        "inputs": inputs,
        "frozen_no_repair": {
            "byte_identical_case_count": (
                len(frozen_records) if frozen_no_repair_identity else 0
            ),
            "case_count": len(frozen_records),
        },
        "non_claims": [
            "static diagnostic only; no training, model promotion, or production encoding change",
            "no collision or alias claim outside the checked 116-case corpus",
            "observation coverage is not an admission criterion",
            "no game-strength, generalization, robustness, human-play, or pro-level-play claim",
        ],
        "schema": SCHEMA,
        "status": status,
        "structured_aliases": {
            "frozen_expected": frozen_aliases,
            "repaired": repaired_aliases,
        },
        "supplemental_case": supplemental_report,
        "transform": {
            "action_digest_mutation": "forbidden",
            "action_reference_mutation": "forbidden",
            "coordinate": BOOLEAN_SLOT,
            "direct_coordinate_mutation": "slot 69 only for combat inclusion kinds",
            "false_f32_le_hex": "00000000",
            "identity": TRANSFORM_IDENTITY,
            "true_f32_le_hex": "0000803f",
        },
    }
    report["payload_sha256"] = audit._sha256(audit.canonical_bytes(report))
    return report


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--check", action="store_true")
    parser.add_argument("--output", type=Path, default=OUTPUT_PATH)
    args = parser.parse_args()
    try:
        audit = _load_audit_module()
        report = build_report()
        rendered = audit.pretty_json(report)
        if report["status"] != ADMITTED_STATUS:
            print(
                f"static action-ingress admission rejected: {report['status']}",
                file=sys.stderr,
            )
            return 1
        if args.check:
            if not args.output.is_file():
                print(f"missing static admission report: {args.output}", file=sys.stderr)
                return 1
            if args.output.read_bytes() != rendered:
                print(f"stale static admission report: {args.output}", file=sys.stderr)
                return 1
        else:
            args.output.parent.mkdir(parents=True, exist_ok=True)
            args.output.write_bytes(rendered)
        print(
            f"{POSITIVE_MARKER} status={report['status']} "
            f"combined_identity_sha256="
            f"{report['corpus']['combined_identity_sha256']} "
            f"payload_sha256={report['payload_sha256']}"
        )
        return 0
    except (AdmissionError, OSError) as exc:
        print(f"static action-ingress admission failed: {exc}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
