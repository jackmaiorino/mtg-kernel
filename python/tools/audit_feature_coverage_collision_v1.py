#!/usr/bin/env python3
"""Audit frozen Net8 feature-fixture coverage and representation collisions.

This diagnostic is deliberately record-only.  It imports the feature schema
with a stub ``torch`` module, consumes the two checked-in Python authority
goldens, and never constructs a model, launches gameplay, or selects a device.
"""

from __future__ import annotations

import argparse
import copy
import hashlib
import importlib.util
import json
import struct
import sys
import types
from collections import defaultdict
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any, Iterable


REPO_ROOT = Path(__file__).resolve().parents[2]
FEATURES_PATH = REPO_ROOT / "python" / "mtg_kernel_rl" / "features.py"
FULL_GOLDEN_PATH = (
    REPO_ROOT / "data" / "flat_policy_v2" / "python_full_features_v2.json"
)
ACTION_GOLDEN_PATH = (
    REPO_ROOT / "data" / "flat_policy_v2" / "python_action_features_v2.json"
)
OUTPUT_PATH = (
    REPO_ROOT
    / "data"
    / "flat_policy_v2"
    / "feature_coverage_collision_audit_v1.json"
)

SCHEMA = "mtg-kernel-feature-coverage-collision-audit/v1"
POSITIVE_MARKER = "FEATURE_COVERAGE_COLLISION_AUDIT_V1_OK"
VALID_STATUSES = (
    "INVALID",
    "COLLISION-DETECTED",
    "COVERAGE-INCOMPLETE",
    "HASH-DEPENDENCE-CANDIDATE",
    "STRUCTURED-DISTINGUISHABLE",
)

EXPECTED_EQUIVALENCE_GROUPS = {
    "observation": [
        ["burn-mirror-opening", "synthetic-actor-seat-swap"],
    ],
    "action": [
        ["boolean-optional-use-true", "primary-choose_optional_cost_use"],
        ["optional-choice-SacrificeLand", "primary-choose_optional_cost_which"],
        [
            "actor-p0-relative-self",
            "actor-p1-relative-self",
            "metadata-invariance-a",
            "metadata-invariance-b",
        ],
    ],
}

FLOAT_TENSOR_FIELDS = {
    "state",
    "object_features",
    "edge_features",
    "action_features",
    "action_ref_features",
}


class AuditError(RuntimeError):
    """The checked authority or audit contract is malformed."""


def canonical_bytes(value: Any) -> bytes:
    return json.dumps(
        value,
        sort_keys=True,
        separators=(",", ":"),
        ensure_ascii=False,
        allow_nan=False,
    ).encode("utf-8")


def pretty_json(value: Any) -> bytes:
    return (
        json.dumps(
            value,
            indent=2,
            sort_keys=True,
            ensure_ascii=False,
            allow_nan=False,
        )
        + "\n"
    ).encode("utf-8")


def _sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def _strict_pairs(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
    result: dict[str, Any] = {}
    for key, value in pairs:
        if key in result:
            raise AuditError(f"duplicate JSON object key: {key!r}")
        result[key] = value
    return result


def _reject_nonfinite(token: str) -> Any:
    raise AuditError(f"non-finite JSON token: {token}")


def _loads_json_strict(text: str, *, label: str) -> Any:
    try:
        return json.loads(
            text,
            object_pairs_hook=_strict_pairs,
            parse_constant=_reject_nonfinite,
        )
    except (AuditError, json.JSONDecodeError) as exc:
        raise AuditError(f"{label}: malformed JSON: {exc}") from exc


def _load_json_strict(path: Path) -> Any:
    return _loads_json_strict(path.read_text(encoding="utf-8"), label=str(path))


def _load_features_without_torch() -> types.ModuleType:
    module_name = "_net8_feature_coverage_audit_features"
    module_spec = importlib.util.spec_from_file_location(module_name, FEATURES_PATH)
    if module_spec is None or module_spec.loader is None:
        raise AuditError("failed to load the frozen feature authority")
    module = importlib.util.module_from_spec(module_spec)
    prior_torch = sys.modules.get("torch")
    sys.modules[module_name] = module
    sys.modules["torch"] = types.ModuleType("torch")
    try:
        module_spec.loader.exec_module(module)
    finally:
        if prior_torch is None:
            sys.modules.pop("torch", None)
        else:
            sys.modules["torch"] = prior_torch
        sys.modules.pop(module_name, None)
    return module


def _validate_payload_hash(document: dict[str, Any], *, label: str) -> None:
    payload = dict(document)
    expected = payload.pop("payload_sha256", None)
    actual = _sha256(canonical_bytes(payload))
    if expected != actual:
        raise AuditError(
            f"{label}: payload SHA-256 mismatch: expected={expected!r} actual={actual}"
        )


def _validate_unique_case_names(document: dict[str, Any], *, label: str) -> None:
    names = [case.get("name") for case in document.get("cases", [])]
    if not names or any(type(name) is not str or not name for name in names):
        raise AuditError(f"{label}: every case must have a nonempty string name")
    if len(names) != len(set(names)):
        raise AuditError(f"{label}: duplicate case name")


@dataclass(frozen=True)
class Atom:
    atom_id: str
    classification: str
    kind: str
    enum: tuple[str, ...] = ()


@dataclass
class Witness:
    occurrence_count: int = 0
    cases: set[str] = field(default_factory=set)
    values: dict[bytes, Any] = field(default_factory=dict)

    def add(self, case_name: str, value: Any) -> None:
        key = canonical_bytes(value)
        self.occurrence_count += 1
        self.cases.add(case_name)
        self.values[key] = value


def _path_text(path: tuple[str, ...]) -> str:
    return ".".join(path)


def _walk_declared_atoms(
    feature_module: types.ModuleType,
    spec: Any,
    path: tuple[str, ...],
) -> list[Atom]:
    atoms: list[Atom] = []

    def walk(current: Any, current_path: tuple[str, ...]) -> None:
        if isinstance(current, feature_module.ScalarSpec):
            atoms.append(
                Atom(
                    _path_text(current_path),
                    current.classification,
                    current.kind,
                    tuple(current.enum),
                )
            )
            return
        if isinstance(current, feature_module.OptionalSpec):
            atoms.append(
                Atom(
                    _path_text(current_path + ("<present>",)),
                    current.classification,
                    "presence",
                )
            )
            walk(current.item, current_path)
            return
        if isinstance(current, feature_module.ListSpec):
            walk(current.item, current_path + ("[]",))
            return
        if isinstance(current, feature_module.TupleSpec):
            for index, child in enumerate(current.items):
                walk(child, current_path + (f"[{index}]",))
            return
        if isinstance(current, feature_module.ObjectSpec):
            for key in sorted(current.fields):
                walk(current.fields[key], current_path + (key,))
            return
        if isinstance(current, feature_module.VariantSpec):
            for variant in sorted(current.variants):
                walk(
                    current.variants[variant],
                    current_path + (f"<{current.tag}={variant}>",),
                )
            return
        raise AuditError(f"unsupported feature spec: {type(current).__name__}")

    walk(spec, path)
    by_id: dict[str, Atom] = {}
    for atom in atoms:
        if atom.atom_id in by_id:
            raise AuditError(f"declared atom identity collapsed: {atom.atom_id}")
        by_id[atom.atom_id] = atom
    return sorted(atoms, key=lambda atom: atom.atom_id)


def _spec_has_model_atom(feature_module: types.ModuleType, spec: Any) -> bool:
    return any(
        atom.classification == feature_module.MODEL_INPUT
        for atom in _walk_declared_atoms(feature_module, spec, ("_",))
    )


def _validate_canonical_scalar(
    feature_module: types.ModuleType,
    spec: Any,
    value: Any,
    path: tuple[str, ...],
) -> None:
    label = _path_text(path)
    if spec.kind == "seat":
        if value not in ("self", "opponent"):
            raise AuditError(f"{label}: canonical seat must be self/opponent")
        return
    if spec.kind == "bool":
        if type(value) is not bool:
            raise AuditError(f"{label}: canonical bool has wrong type")
        return
    if spec.kind == "int":
        if type(value) is not int:
            raise AuditError(f"{label}: canonical integer has wrong type")
        if spec.minimum is not None and value < spec.minimum:
            raise AuditError(f"{label}: canonical integer is below minimum")
        if spec.maximum is not None and value > spec.maximum:
            raise AuditError(f"{label}: canonical integer exceeds maximum")
        return
    if spec.kind == "str":
        if type(value) is not str or (spec.nonempty and not value):
            raise AuditError(f"{label}: canonical string has wrong shape")
        return
    if spec.kind == "enum":
        if type(value) is not str or value not in spec.enum:
            raise AuditError(f"{label}: canonical enum value is outside its domain")
        return
    raise AuditError(f"{label}: unsupported canonical scalar kind {spec.kind!r}")


def _walk_observed_atoms(
    feature_module: types.ModuleType,
    value: Any,
    spec: Any,
    path: tuple[str, ...],
    *,
    case_name: str,
    actor: str,
    canonical: bool,
    witnesses: dict[str, Witness],
) -> None:
    def record(current_path: tuple[str, ...], current_spec: Any, item: Any) -> None:
        if current_spec.classification != feature_module.MODEL_INPUT:
            return
        if current_spec.kind == "seat" and item in ("p0", "p1"):
            item = "self" if item == actor else "opponent"
        witnesses[_path_text(current_path)].add(case_name, item)

    if isinstance(spec, feature_module.ScalarSpec):
        if canonical:
            _validate_canonical_scalar(feature_module, spec, value, path)
        record(path, spec, value)
        return
    if isinstance(spec, feature_module.OptionalSpec):
        if spec.classification == feature_module.MODEL_INPUT:
            witnesses[_path_text(path + ("<present>",))].add(
                case_name, value is not None
            )
        if value is not None:
            _walk_observed_atoms(
                feature_module,
                value,
                spec.item,
                path,
                case_name=case_name,
                actor=actor,
                canonical=canonical,
                witnesses=witnesses,
            )
        return
    if isinstance(spec, feature_module.ListSpec):
        if not isinstance(value, list):
            raise AuditError(f"{_path_text(path)}: expected list")
        for child in value:
            _walk_observed_atoms(
                feature_module,
                child,
                spec.item,
                path + ("[]",),
                case_name=case_name,
                actor=actor,
                canonical=canonical,
                witnesses=witnesses,
            )
        return
    if isinstance(spec, feature_module.TupleSpec):
        if not isinstance(value, list) or len(value) != len(spec.items):
            raise AuditError(f"{_path_text(path)}: expected canonical tuple/list")
        for index, (child, child_spec) in enumerate(zip(value, spec.items)):
            _walk_observed_atoms(
                feature_module,
                child,
                child_spec,
                path + (f"[{index}]",),
                case_name=case_name,
                actor=actor,
                canonical=canonical,
                witnesses=witnesses,
            )
        return
    if isinstance(spec, feature_module.ObjectSpec):
        if not isinstance(value, dict):
            raise AuditError(f"{_path_text(path)}: expected object")
        if canonical:
            expected = {
                key
                for key, child_spec in spec.fields.items()
                if _spec_has_model_atom(feature_module, child_spec)
            }
            if set(value) != expected:
                raise AuditError(
                    f"{_path_text(path)}: canonical fields mismatch: "
                    f"missing={sorted(expected - set(value))} "
                    f"extra={sorted(set(value) - expected)}"
                )
            keys = sorted(expected)
        else:
            keys = sorted(spec.fields)
        for key in keys:
            _walk_observed_atoms(
                feature_module,
                value[key],
                spec.fields[key],
                path + (key,),
                case_name=case_name,
                actor=actor,
                canonical=canonical,
                witnesses=witnesses,
            )
        return
    if isinstance(spec, feature_module.VariantSpec):
        if not isinstance(value, dict):
            raise AuditError(f"{_path_text(path)}: expected variant object")
        variant = value.get(spec.tag)
        if variant not in spec.variants:
            raise AuditError(
                f"{_path_text(path)}: unsupported {spec.tag} variant {variant!r}"
            )
        _walk_observed_atoms(
            feature_module,
            value,
            spec.variants[variant],
            path + (f"<{spec.tag}={variant}>",),
            case_name=case_name,
            actor=actor,
            canonical=canonical,
            witnesses=witnesses,
        )
        return
    raise AuditError(f"unsupported observed spec: {type(spec).__name__}")


def _sorted_values(values: Iterable[Any]) -> list[Any]:
    return sorted(values, key=canonical_bytes)


def _coverage_report(
    feature_module: types.ModuleType,
    atoms: list[Atom],
    witnesses: dict[str, Witness],
) -> dict[str, Any]:
    model_atoms = [
        atom
        for atom in atoms
        if atom.classification == feature_module.MODEL_INPUT
    ]
    unknown = sorted(set(witnesses) - {atom.atom_id for atom in model_atoms})
    if unknown:
        raise AuditError(f"observed undeclared model-input atoms: {unknown}")
    atom_rows: list[dict[str, Any]] = []
    unwitnessed: list[str] = []
    boolean_polarity_gaps: list[dict[str, Any]] = []
    optional_presence_gaps: list[dict[str, Any]] = []
    enum_domain_gaps: list[dict[str, Any]] = []
    seat_category_gaps: list[dict[str, Any]] = []
    for atom in model_atoms:
        witness = witnesses.get(atom.atom_id, Witness())
        values = _sorted_values(witness.values.values())
        row: dict[str, Any] = {
            "atom_id": atom.atom_id,
            "case_count": len(witness.cases),
            "classification": atom.classification,
            "distinct_value_count": len(values),
            "kind": atom.kind,
            "occurrence_count": witness.occurrence_count,
            "status": "WITNESSED" if witness.occurrence_count else "UNWITNESSED",
        }
        if atom.kind in ("bool", "presence", "enum", "seat"):
            row["observed_values"] = values
        atom_rows.append(row)
        if not witness.occurrence_count:
            unwitnessed.append(atom.atom_id)
        if atom.kind in ("bool", "presence"):
            missing = [value for value in (False, True) if value not in values]
            if missing:
                gap = {
                    "atom_id": atom.atom_id,
                    "missing_values": missing,
                    "observed_values": values,
                }
                if atom.kind == "presence":
                    optional_presence_gaps.append(gap)
                else:
                    boolean_polarity_gaps.append(gap)
        elif atom.kind == "enum":
            missing = sorted(set(atom.enum) - set(values))
            if missing:
                enum_domain_gaps.append(
                    {
                        "atom_id": atom.atom_id,
                        "missing_values": missing,
                        "observed_values": values,
                    }
                )
        elif atom.kind == "seat":
            missing = sorted({"self", "opponent"} - set(values))
            if missing:
                seat_category_gaps.append(
                    {
                        "atom_id": atom.atom_id,
                        "missing_values": missing,
                        "observed_values": values,
                    }
                )
    classification_counts: dict[str, int] = defaultdict(int)
    for atom in atoms:
        classification_counts[atom.classification] += 1
    return {
        "atom_id_grammar": (
            "dot-separated schema path; [] denotes any list item; [n] a tuple "
            "position; <tag=value> retains selected VariantSpec context"
        ),
        "atoms": atom_rows,
        "boolean_polarity_gaps": boolean_polarity_gaps,
        "classification_counts": dict(sorted(classification_counts.items())),
        "covered_model_input_atoms": len(model_atoms) - len(unwitnessed),
        "declared_model_input_atoms": len(model_atoms),
        "enum_domain_gaps_diagnostic_only": enum_domain_gaps,
        "optional_presence_gaps": optional_presence_gaps,
        "required_coverage_complete": not (
            unwitnessed or boolean_polarity_gaps or optional_presence_gaps
        ),
        "seat_category_gaps_diagnostic_only": seat_category_gaps,
        "unwitnessed_model_input_atoms": unwitnessed,
    }


def _digest_blocks(namespace: str, payload: bytes) -> list[bytes]:
    return [
        hashlib.sha512(
            namespace.encode("ascii") + counter.to_bytes(4, "little") + payload
        ).digest()
        for counter in range(6)
    ]


def _digest_f32_tail(namespace: str, payload: bytes) -> bytes:
    output = bytearray()
    for block in _digest_blocks(namespace, payload):
        for offset in range(0, len(block), 4):
            word = int.from_bytes(block[offset : offset + 4], "little")
            value = (float(word) / float(0xFFFF_FFFF)) * 2.0 - 1.0
            output.extend(struct.pack("<f", value))
    return bytes(output)


def _decode_hex(value: Any, *, label: str) -> bytes:
    if type(value) is not str:
        raise AuditError(f"{label}: expected hexadecimal string")
    try:
        return bytes.fromhex(value)
    except ValueError as exc:
        raise AuditError(f"{label}: invalid hexadecimal data") from exc


def _trim_f32_rows(
    payload: dict[str, Any],
    *,
    original_columns: int,
    kept_columns: int,
    label: str,
) -> dict[str, Any]:
    shape = payload.get("shape")
    if shape == [original_columns]:
        rows = 1
        new_shape = [kept_columns]
    elif (
        isinstance(shape, list)
        and len(shape) == 2
        and type(shape[0]) is int
        and shape[1] == original_columns
    ):
        rows = shape[0]
        new_shape = [rows, kept_columns]
    else:
        raise AuditError(f"{label}: unexpected f32 tensor shape {shape!r}")
    raw = _decode_hex(payload.get("f32_le_hex"), label=label)
    expected_bytes = rows * original_columns * 4
    if len(raw) != expected_bytes:
        raise AuditError(
            f"{label}: expected {expected_bytes} f32 bytes, found {len(raw)}"
        )
    kept = b"".join(
        raw[
            row * original_columns * 4 : row * original_columns * 4
            + kept_columns * 4
        ]
        for row in range(rows)
    )
    return {"shape": new_shape, "f32_le_hex": kept.hex()}


def _observation_representation_keys(
    case: dict[str, Any],
    *,
    state_dim: int,
    state_direct_dim: int,
    action_dim: int,
    action_direct_dim: int,
) -> tuple[bytes, bytes]:
    tensors = case.get("tensors")
    if not isinstance(tensors, dict):
        raise AuditError(f"{case.get('name')}: missing full tensor map")
    # action_ref_card_ids is validation-only transport.  Net8 gathers the
    # linked object hidden state through action_ref_node_indices instead.
    consumed = {
        key: value
        for key, value in tensors.items()
        if key != "action_ref_card_ids"
    }
    complete = canonical_bytes(consumed)
    structured = copy.deepcopy(consumed)
    structured["state"] = _trim_f32_rows(
        tensors["state"],
        original_columns=state_dim,
        kept_columns=state_direct_dim,
        label=f"{case['name']}.state",
    )
    structured["action_features"] = _trim_f32_rows(
        tensors["action_features"],
        original_columns=action_dim,
        kept_columns=action_direct_dim,
        label=f"{case['name']}.action_features",
    )
    return canonical_bytes(structured), complete


def _action_representation_keys(
    case: dict[str, Any],
    *,
    action_dim: int,
    action_direct_dim: int,
) -> tuple[bytes, bytes]:
    full = _decode_hex(
        case.get("full_feature_f32_le_hex"),
        label=f"{case.get('name')}.full_feature_f32_le_hex",
    )
    if len(full) != action_dim * 4:
        raise AuditError(f"{case.get('name')}: action feature width mismatch")
    shared = {
        "action_ref_feature_f32_le_hex": case.get(
            "action_ref_feature_f32_le_hex"
        ),
        "action_ref_node_indices": case.get("action_ref_node_indices"),
        "objects": case.get("flat_input", {}).get("objects"),
    }
    structured = dict(shared)
    structured["action_feature_f32_le_hex"] = full[: action_direct_dim * 4].hex()
    complete = dict(shared)
    complete["action_feature_f32_le_hex"] = full.hex()
    return canonical_bytes(structured), canonical_bytes(complete)


def _collision_groups(records: list[dict[str, Any]], key: str) -> list[dict[str, Any]]:
    groups: dict[bytes, list[dict[str, Any]]] = defaultdict(list)
    for record in records:
        groups[record[key]].append(record)
    collisions: list[dict[str, Any]] = []
    for signature, members in groups.items():
        canonical_values = {member["canonical"] for member in members}
        if len(canonical_values) <= 1:
            continue
        collisions.append(
            {
                "canonical_sha256": sorted(_sha256(value) for value in canonical_values),
                "cases": sorted(member["name"] for member in members),
                "signature_sha256": _sha256(signature),
            }
        )
    return sorted(collisions, key=lambda group: group["cases"])


def _equivalence_groups(records: list[dict[str, Any]]) -> list[list[str]]:
    groups: dict[bytes, list[str]] = defaultdict(list)
    for record in records:
        groups[record["canonical"]].append(record["name"])
    return sorted(
        [sorted(names) for names in groups.values() if len(names) > 1],
        key=lambda names: names,
    )


def _validate_equivalence_groups(
    observed: dict[str, list[list[str]]],
    expected: dict[str, list[list[str]]] = EXPECTED_EQUIVALENCE_GROUPS,
) -> None:
    normalized_expected = {
        scope: sorted([sorted(group) for group in groups])
        for scope, groups in expected.items()
    }
    if observed != normalized_expected:
        raise AuditError(
            "canonical equivalence allowlist drift: "
            f"expected={normalized_expected} observed={observed}"
        )


def _structured_alias_groups(records: list[dict[str, Any]]) -> list[dict[str, Any]]:
    groups: dict[bytes, list[dict[str, Any]]] = defaultdict(list)
    for record in records:
        groups[record["structured"]].append(record)
    aliases: list[dict[str, Any]] = []
    for signature, members in groups.items():
        canonical_values = {member["canonical"] for member in members}
        if len(canonical_values) <= 1:
            continue
        aliases.append(
            {
                "canonical_sha256": sorted(_sha256(value) for value in canonical_values),
                "cases": sorted(member["name"] for member in members),
                "complete_representation_distinct": len(
                    {member["complete"] for member in members}
                )
                > 1,
                "structured_signature_sha256": _sha256(signature),
            }
        )
    return sorted(aliases, key=lambda group: group["cases"])


def _case_identity_rows(records: list[dict[str, Any]]) -> list[dict[str, Any]]:
    return [
        {
            "canonical_sha256": _sha256(record["canonical"]),
            "complete_representation_sha256": _sha256(record["complete"]),
            "name": record["name"],
            "quantized_tail_sha256": _sha256(record["quantized"]),
            "raw_digest_sha256": _sha256(record["raw_digest"]),
            "structured_signature_sha256": _sha256(record["structured"]),
        }
        for record in sorted(records, key=lambda item: item["name"])
    ]


def _validate_authorities(
    feature_module: types.ModuleType,
    full: dict[str, Any],
    action: dict[str, Any],
) -> None:
    if full.get("schema") != "mtg-kernel-python-full-features-golden/v2":
        raise AuditError("unexpected full-feature golden schema")
    if action.get("schema") != "mtg-kernel-python-action-features-golden/v2":
        raise AuditError("unexpected action-feature golden schema")
    _validate_payload_hash(full, label="full-feature golden")
    _validate_payload_hash(action, label="action-feature golden")
    _validate_unique_case_names(full, label="full-feature golden")
    _validate_unique_case_names(action, label="action-feature golden")
    features_sha = _sha256(FEATURES_PATH.read_bytes())
    for label, document in (("full", full), ("action", action)):
        if document.get("authority") != "python/mtg_kernel_rl/features.py":
            raise AuditError(f"{label}: unexpected feature authority path")
        if document.get("authority_sha256") != features_sha:
            raise AuditError(f"{label}: frozen feature authority SHA-256 mismatch")
        contracts = document.get("python_contracts")
        expected_contracts = {
            "encoding_contract_version": feature_module.ENCODING_CONTRACT_VERSION,
            "feature_registry_version": feature_module.FEATURE_REGISTRY_VERSION,
            "feature_schema_version": feature_module.FEATURE_SCHEMA_VERSION,
            "model_contract_version": feature_module.MODEL_CONTRACT_VERSION,
        }
        if contracts != expected_contracts:
            raise AuditError(f"{label}: Python contract identity mismatch")


def _build_records_and_coverage(
    feature_module: types.ModuleType,
    full: dict[str, Any],
    action: dict[str, Any],
) -> tuple[
    dict[str, dict[str, Any]],
    dict[str, list[dict[str, Any]]],
]:
    state_dim = feature_module.STATE_FEATURE_DIM
    state_direct_dim = state_dim - feature_module.STATE_HASH_DIM
    action_dim = feature_module.ACTION_FEATURE_DIM
    action_direct_dim = action_dim - feature_module.ACTION_HASH_DIM
    records: dict[str, list[dict[str, Any]]] = {
        "observation": [],
        "action": [],
    }
    observation_witnesses: dict[str, Witness] = defaultdict(Witness)
    action_witnesses: dict[str, Witness] = defaultdict(Witness)

    for case in full["cases"]:
        name = case["name"]
        canonical_text = case.get("canonical_observation_json")
        if type(canonical_text) is not str:
            raise AuditError(f"{name}: missing canonical observation JSON")
        canonical_value = _loads_json_strict(
            canonical_text, label=f"{name}.canonical_observation_json"
        )
        canonical = canonical_bytes(canonical_value)
        if canonical != canonical_text.encode("utf-8"):
            raise AuditError(f"{name}: observation JSON is not canonical")
        raw_blocks = _digest_blocks("observation-state", canonical)
        stored_blocks = [
            _decode_hex(value, label=f"{name}.state_sha512_blocks_hex")
            for value in case.get("state_sha512_blocks_hex", [])
        ]
        if stored_blocks != raw_blocks:
            raise AuditError(f"{name}: observation raw digest mismatch")
        quantized = _digest_f32_tail("observation-state", canonical)
        state_tensor = case["tensors"]["state"]
        state_bytes = _decode_hex(
            state_tensor.get("f32_le_hex"), label=f"{name}.state"
        )
        if state_tensor.get("shape") != [state_dim] or len(state_bytes) != state_dim * 4:
            raise AuditError(f"{name}: state tensor dimension mismatch")
        if state_bytes[state_direct_dim * 4 :] != quantized:
            raise AuditError(f"{name}: state quantized digest tail mismatch")
        structured, complete = _observation_representation_keys(
            case,
            state_dim=state_dim,
            state_direct_dim=state_direct_dim,
            action_dim=action_dim,
            action_direct_dim=action_direct_dim,
        )
        records["observation"].append(
            {
                "canonical": canonical,
                "complete": complete,
                "name": name,
                "quantized": quantized,
                "raw_digest": b"".join(raw_blocks),
                "structured": structured,
            }
        )
        rust_fixture = case.get("rust_fixture")
        if (
            not isinstance(rust_fixture, dict)
            or rust_fixture.get("schema") != "native-flat-full-v2-rust-fixture-v1"
        ):
            raise AuditError(f"{name}: malformed Rust fixture")
        observation = rust_fixture.get("observation")
        try:
            feature_module.OBSERVATION_SPEC.validate(observation, ("observation",))
        except Exception as exc:
            raise AuditError(f"{name}: raw observation failed schema validation") from exc
        _walk_observed_atoms(
            feature_module,
            observation,
            feature_module.OBSERVATION_SPEC,
            ("observation",),
            case_name=name,
            actor=observation["acting_player"],
            canonical=False,
            witnesses=observation_witnesses,
        )

    for case in action["cases"]:
        name = case["name"]
        canonical_text = case.get("canonical_json")
        if type(canonical_text) is not str:
            raise AuditError(f"{name}: missing canonical action JSON")
        canonical_value = _loads_json_strict(
            canonical_text, label=f"{name}.canonical_json"
        )
        canonical = canonical_bytes(canonical_value)
        if canonical != canonical_text.encode("utf-8"):
            raise AuditError(f"{name}: action JSON is not canonical")
        raw_blocks = _digest_blocks("legal-action", canonical)
        stored_blocks = [
            _decode_hex(value, label=f"{name}.sha512_blocks_hex")
            for value in case.get("sha512_blocks_hex", [])
        ]
        if stored_blocks != raw_blocks:
            raise AuditError(f"{name}: action raw digest mismatch")
        quantized = _digest_f32_tail("legal-action", canonical)
        full_features = _decode_hex(
            case.get("full_feature_f32_le_hex"),
            label=f"{name}.full_feature_f32_le_hex",
        )
        if len(full_features) != action_dim * 4:
            raise AuditError(f"{name}: action tensor dimension mismatch")
        if full_features[action_direct_dim * 4 :] != quantized:
            raise AuditError(f"{name}: action quantized digest tail mismatch")
        structured, complete = _action_representation_keys(
            case,
            action_dim=action_dim,
            action_direct_dim=action_direct_dim,
        )
        records["action"].append(
            {
                "canonical": canonical,
                "complete": complete,
                "name": name,
                "quantized": quantized,
                "raw_digest": b"".join(raw_blocks),
                "structured": structured,
            }
        )
        semantic = canonical_value.get("semantic")
        actor = semantic.get("actor") if isinstance(semantic, dict) else None
        if actor not in ("self", "opponent"):
            raise AuditError(f"{name}: canonical action lacks relative actor")
        _walk_observed_atoms(
            feature_module,
            canonical_value,
            feature_module.LEGAL_ACTION_SPEC,
            ("legal_action",),
            case_name=name,
            actor=actor,
            canonical=True,
            witnesses=action_witnesses,
        )

    coverage = {
        "observation": _coverage_report(
            feature_module,
            _walk_declared_atoms(
                feature_module,
                feature_module.OBSERVATION_SPEC,
                ("observation",),
            ),
            observation_witnesses,
        ),
        "action": _coverage_report(
            feature_module,
            _walk_declared_atoms(
                feature_module,
                feature_module.LEGAL_ACTION_SPEC,
                ("legal_action",),
            ),
            action_witnesses,
        ),
    }
    return coverage, records


def build_report(
    *,
    full_path: Path = FULL_GOLDEN_PATH,
    action_path: Path = ACTION_GOLDEN_PATH,
) -> dict[str, Any]:
    feature_module = _load_features_without_torch()
    full = _load_json_strict(full_path)
    action = _load_json_strict(action_path)
    if not isinstance(full, dict) or not isinstance(action, dict):
        raise AuditError("golden roots must be JSON objects")
    _validate_authorities(feature_module, full, action)
    coverage, records = _build_records_and_coverage(feature_module, full, action)

    equivalences = {
        scope: _equivalence_groups(scope_records)
        for scope, scope_records in records.items()
    }
    _validate_equivalence_groups(equivalences)
    collisions = {
        level: {
            scope: _collision_groups(scope_records, key)
            for scope, scope_records in records.items()
        }
        for level, key in (
            ("raw_digest", "raw_digest"),
            ("quantized_tail", "quantized"),
            ("complete_representation", "complete"),
        )
    }
    collision_count = sum(
        len(groups)
        for level in collisions.values()
        for groups in level.values()
    )
    coverage_complete = all(
        scope["required_coverage_complete"] for scope in coverage.values()
    )
    structured_aliases = {
        scope: _structured_alias_groups(scope_records)
        for scope, scope_records in records.items()
    }

    if collision_count:
        status = "COLLISION-DETECTED"
        reasons = [f"{collision_count} distinct-canonical collision group(s)"]
    elif not coverage_complete:
        status = "COVERAGE-INCOMPLETE"
        reasons = [
            "one or more required model-input atom or boolean/optional category "
            "has no checked-corpus witness"
        ]
    elif any(structured_aliases.values()):
        status = "HASH-DEPENDENCE-CANDIDATE"
        reasons = [
            "a distinct-canonical checked pair shares its structured signature "
            "and is separated by the digest-bearing complete representation"
        ]
    else:
        status = "STRUCTURED-DISTINGUISHABLE"
        reasons = [
            "every distinct-canonical checked record has a distinct structured "
            "signature"
        ]
    if status not in VALID_STATUSES:
        raise AssertionError(status)

    source_path = Path(__file__).resolve()
    report: dict[str, Any] = {
        "authority": {
            "audit_source_path": str(source_path.relative_to(REPO_ROOT)).replace(
                "\\", "/"
            ),
            "audit_source_sha256": _sha256(source_path.read_bytes()),
            "encoding_contract_fingerprint": (
                feature_module.encoding_contract_fingerprint()
            ),
            "feature_contract_fingerprint": (
                feature_module.feature_contract_fingerprint()
            ),
            "features_path": "python/mtg_kernel_rl/features.py",
            "features_sha256": _sha256(FEATURES_PATH.read_bytes()),
            "python_contracts": {
                "encoding_contract_version": (
                    feature_module.ENCODING_CONTRACT_VERSION
                ),
                "feature_registry_version": feature_module.FEATURE_REGISTRY_VERSION,
                "feature_schema_version": feature_module.FEATURE_SCHEMA_VERSION,
                "model_contract_version": feature_module.MODEL_CONTRACT_VERSION,
            },
        },
        "collisions": collisions,
        "corpus": {
            "action_case_count": len(action["cases"]),
            "observation_case_count": len(full["cases"]),
        },
        "coverage": coverage,
        "decision": {
            "precedence": list(VALID_STATUSES),
            "reasons": reasons,
            "status": status,
        },
        "dimensions": {
            "action": feature_module.ACTION_FEATURE_DIM,
            "action_direct": (
                feature_module.ACTION_FEATURE_DIM - feature_module.ACTION_HASH_DIM
            ),
            "action_digest": feature_module.ACTION_HASH_DIM,
            "action_ref": feature_module.ACTION_REF_FEATURE_DIM,
            "edge": feature_module.EDGE_FEATURE_DIM,
            "object": feature_module.OBJECT_FEATURE_DIM,
            "object_groups": len(feature_module.OBJECT_GROUPS),
            "state": feature_module.STATE_FEATURE_DIM,
            "state_direct": (
                feature_module.STATE_FEATURE_DIM - feature_module.STATE_HASH_DIM
            ),
            "state_digest": feature_module.STATE_HASH_DIM,
        },
        "digest_construction": {
            "block_count": 6,
            "block_hash": "sha512",
            "chunk_encoding": "u32_le",
            "chunk_to_float": (
                "f64(chunk)/f64(0xffffffff)*2.0-1.0 then one f32 cast"
            ),
            "counter_encoding": "u32_le",
            "namespaces_ascii": {
                "action": "legal-action",
                "observation": "observation-state",
            },
            "semantics": "whole-record fingerprint; not per-field bucket hashing",
        },
        "equivalences": {
            "expected_intentional_groups": EXPECTED_EQUIVALENCE_GROUPS,
            "observed_intentional_groups": equivalences,
        },
        "inputs": [
            {
                "case_count": len(full["cases"]),
                "path": str(full_path.relative_to(REPO_ROOT)).replace("\\", "/"),
                "schema": full["schema"],
                "sha256": _sha256(full_path.read_bytes()),
            },
            {
                "case_count": len(action["cases"]),
                "path": str(action_path.relative_to(REPO_ROOT)).replace("\\", "/"),
                "schema": action["schema"],
                "sha256": _sha256(action_path.read_bytes()),
            },
        ],
        "non_claims": [
            "diagnostic-only; no training, qualification, promotion, or game-strength claim",
            "no collision means identity preservation only over the checked corpus",
            "no claim that a cryptographic fingerprint is learnable or harmless",
            "no observation-leakage, model-capacity, equilibrium, BO3, sideboarding, human, or pro-level-play verdict",
            "enum-domain and seat-category gaps are diagnostic because schema domains can contain contextually inadmissible combinations",
        ],
        "representation_signature_contract": {
            "complete": (
                "all checked Net8-consumed tensor inputs, including state/action "
                "digest tails"
            ),
            "excluded_transport": [
                "action_ref_card_ids (validated but not consumed by Net8 forward)"
            ],
            "structured": (
                "the same consumed inputs with state[123:219] and each "
                "action[99:195] digest tail removed"
            ),
        },
        "representation_identities": {
            scope: _case_identity_rows(scope_records)
            for scope, scope_records in records.items()
        },
        "schema": SCHEMA,
        "status": status,
        "structured_alias_groups": structured_aliases,
    }
    report["payload_sha256"] = _sha256(canonical_bytes(report))
    return report


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--check", action="store_true")
    parser.add_argument("--output", type=Path, default=OUTPUT_PATH)
    args = parser.parse_args()
    try:
        report = build_report()
        rendered = pretty_json(report)
        if args.check:
            if not args.output.exists():
                print(f"missing audit report: {args.output}", file=sys.stderr)
                return 1
            if args.output.read_bytes() != rendered:
                print(f"stale audit report: {args.output}", file=sys.stderr)
                return 1
        else:
            args.output.parent.mkdir(parents=True, exist_ok=True)
            args.output.write_bytes(rendered)
        print(
            f"{POSITIVE_MARKER} status={report['status']} "
            f"payload_sha256={report['payload_sha256']}"
        )
        return 0
    except (AuditError, OSError) as exc:
        print(f"feature coverage/collision audit failed: {exc}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
