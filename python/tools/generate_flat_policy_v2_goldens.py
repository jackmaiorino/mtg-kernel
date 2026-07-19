from __future__ import annotations

import argparse
import copy
import hashlib
import importlib.util
import json
from pathlib import Path
import re
import struct
import sys
import types
from typing import Any


ROOT = Path(__file__).resolve().parents[2]
GOLDENS = ROOT / "data" / "flat_policy_v2" / "goldens_v2.json"
INVENTORY = ROOT / "data" / "flat_policy_v2" / "feature_inventory_v2.json"
ACTION_CONTRACT = ROOT / "data" / "flat_policy_v2" / "action_contract_v2.json"
TOPOLOGY_AUDIT = ROOT / "data" / "flat_policy_v2" / "ordered_topology_audit_v2.md"
FEATURES = ROOT / "python" / "mtg_kernel_rl" / "features.py"
FIXTURES = ROOT / "python" / "tests" / "fixtures.py"
BASE_INVENTORY = ROOT / "data" / "flat_policy_v1" / "feature_inventory_v1.json"
BASE_GOLDENS = ROOT / "data" / "flat_policy_v1" / "goldens_v1.json"
BASE_LAYOUT = ROOT / "mtg-kernel" / "src" / "flat_policy_v1.rs"
OVERLAY_LAYOUT = ROOT / "mtg-kernel" / "src" / "flat_policy_v2.rs"
CARD_DEF_SOURCE = ROOT / "mtg-kernel" / "src" / "card_def.rs"
# Frozen by mtg-kernel/src/card_def.rs::card_db_hash_v5_is_frozen and checked
# again by the Rust production-commitment golden test.
KERNEL_CARDDB_HASH = 0xA06F_A956_6106_F0EA
ACTION_REF_INTERNAL_ENTRIES = [
    {"role": "source", "rust_internal_id": 0, "python_projection_id": 0},
    {"role": "candidate", "rust_internal_id": 1, "python_projection_id": 1},
    {"role": "card", "rust_internal_id": 2, "python_projection_id": 2},
    {"role": "attacker", "rust_internal_id": 3, "python_projection_id": 3},
    {"role": "blocker", "rust_internal_id": 4, "python_projection_id": 4},
    {"role": "target_object", "rust_internal_id": 5, "python_projection_id": 5},
    {"role": "cards", "rust_internal_id": 6, "python_projection_id": 6},
    {"role": "pending_sources", "rust_internal_id": 7, "python_projection_id": 9},
]
ACTION_REF_PROJECTION_ONLY = [
    {"role": "attackers", "python_projection_id": 7},
    {"role": "blockers", "python_projection_id": 8},
]


def assert_kernel_card_db_hash_authority() -> None:
    matches = re.findall(
        r"assert_eq!\(KERNEL_CARDDB_HASH,\s*0x([0-9a-fA-F_]+)\)",
        CARD_DEF_SOURCE.read_text(encoding="utf-8"),
    )
    if len(matches) != 1:
        raise AssertionError("card_def.rs must expose one frozen KERNEL_CARDDB_HASH assertion")
    if int(matches[0].replace("_", ""), 16) != KERNEL_CARDDB_HASH:
        raise AssertionError("generator KERNEL_CARDDB_HASH drifted from card_def.rs frozen authority")


def validated_action_ref_role_mapping(
    base_goldens: dict[str, Any], action_contract: dict[str, Any]
) -> dict[str, Any]:
    crosswalk = base_goldens["action_ref_role_crosswalk"]
    expected_crosswalk = {
        "entries": ACTION_REF_INTERNAL_ENTRIES,
        "mapping_version": 1,
        "projection_only": ACTION_REF_PROJECTION_ONLY,
        "python_projection_width": 10,
        "rust_internal_width": 8,
        "schema": "flat-policy-action-ref-role-crosswalk-v1",
    }
    if crosswalk != expected_crosswalk:
        raise AssertionError("V1 action-reference role crosswalk authority drift")

    authority = action_contract["reference_role_mapping"]
    if set(authority) != {
        "authority_path",
        "authority_schema",
        "mapping_version",
        "canonical_sha256",
        "compatibility",
    }:
        raise AssertionError("V2 action-reference mapping authority fields drift")
    crosswalk_sha256 = sha256_hex(canonical_json_bytes(crosswalk))
    if authority != {
        "authority_path": "data/flat_policy_v1/goldens_v1.json#action_ref_role_crosswalk",
        "authority_schema": "flat-policy-action-ref-role-crosswalk-v1",
        "mapping_version": 1,
        "canonical_sha256": crosswalk_sha256,
        "compatibility": "v2_exactly_reuses_all_v1_internal_and_projection_role_ids",
    }:
        raise AssertionError(
            "V2 action-reference mapping must exactly bind the validated V1 authority"
        )
    return {
        "authority_path": authority["authority_path"],
        "authority_schema": authority["authority_schema"],
        "mapping_version": authority["mapping_version"],
        "canonical_sha256": crosswalk_sha256,
        "internal_to_projection": [
            entry["python_projection_id"] for entry in crosswalk["entries"]
        ],
        "entries": copy.deepcopy(crosswalk["entries"]),
        "projection_only": copy.deepcopy(crosswalk["projection_only"]),
    }


def canonical_json_bytes(value: object) -> bytes:
    return json.dumps(
        value,
        sort_keys=True,
        separators=(",", ":"),
        ensure_ascii=False,
    ).encode("utf-8")


def pretty_json(value: object) -> str:
    return json.dumps(value, indent=2, ensure_ascii=False) + "\n"


def sha256_hex(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def load_module_without_torch(name: str, path: Path) -> types.ModuleType:
    module_spec = importlib.util.spec_from_file_location(name, path)
    if module_spec is None or module_spec.loader is None:
        raise AssertionError(f"failed to load {path}")
    module = importlib.util.module_from_spec(module_spec)
    prior_torch = sys.modules.get("torch")
    sys.modules[name] = module
    sys.modules["torch"] = types.ModuleType("torch")
    try:
        module_spec.loader.exec_module(module)
    finally:
        if prior_torch is None:
            sys.modules.pop("torch", None)
        else:
            sys.modules["torch"] = prior_torch
        sys.modules.pop(name, None)
    return module


def load_plain_module(name: str, path: Path) -> types.ModuleType:
    module_spec = importlib.util.spec_from_file_location(name, path)
    if module_spec is None or module_spec.loader is None:
        raise AssertionError(f"failed to load {path}")
    module = importlib.util.module_from_spec(module_spec)
    sys.modules[name] = module
    try:
        module_spec.loader.exec_module(module)
    finally:
        sys.modules.pop(name, None)
    return module


def _full_observation_cases(features: types.ModuleType, fixtures: types.ModuleType) -> tuple[dict[str, str], dict[str, str]]:
    base = fixtures.observation()
    battlefield = base["projection"]["battlefield"][0]
    first = copy.deepcopy(battlefield[0]["stable"])
    second = copy.deepcopy(battlefield[1]["stable"])
    base["projection"]["combat"]["ordered_attackers"] = [first, second]

    mappings = {
        "absent": [[first, []]],
        "present_empty_forward": [[first, []], [second, []]],
        "present_empty_reverse": [[second, []], [first, []]],
    }
    full_cases: dict[str, str] = {}
    model_cases: dict[str, str] = {}
    raw_values: dict[str, dict[str, Any]] = {}
    for name, mapping in mappings.items():
        observation = copy.deepcopy(base)
        observation["projection"]["combat"]["attacker_to_ordered_blockers"] = mapping
        features.assert_observation_classified(observation)
        raw_values[name] = observation
        full_cases[name] = canonical_json_bytes(observation).decode("utf-8")
        canonical = features._canonical_model_value(
            observation,
            features.OBSERVATION_SPEC,
            ("observation",),
            features._CanonicalContext(observation["acting_player"], observation),
        )
        model_cases[name] = canonical_json_bytes(canonical).decode("utf-8")

    ignored_path = ("projection", "combat", "attacker_to_ordered_blockers")
    def without_mapping(value: dict[str, Any]) -> dict[str, Any]:
        result = copy.deepcopy(value)
        cursor: Any = result
        for part in ignored_path[:-1]:
            cursor = cursor[part]
        cursor[ignored_path[-1]] = "<red-pair-field>"
        return result

    if without_mapping(raw_values["present_empty_forward"]) != without_mapping(
        raw_values["present_empty_reverse"]
    ):
        raise AssertionError("red-pair source observations differ outside the ordered mapping")
    return full_cases, model_cases


def _state_hash_record(payload: bytes) -> dict[str, object]:
    blocks: list[str] = []
    words: list[int] = []
    f32_bits: list[str] = []
    for counter in range(6):
        digest = hashlib.sha512(
            b"observation-state" + counter.to_bytes(4, "little") + payload
        ).digest()
        blocks.append(digest.hex())
        for offset in range(0, len(digest), 4):
            word = int.from_bytes(digest[offset : offset + 4], "little")
            words.append(word)
            feature = (float(word) / float(0xFFFF_FFFF)) * 2.0 - 1.0
            f32_bits.append(f"{int.from_bytes(struct.pack('<f', feature), 'little'):08x}")
    if len(words) != 96:
        raise AssertionError("six SHA-512 blocks must produce 96 u32 words")
    return {
        "sha512_blocks_hex": blocks,
        "u32_words": words,
        "f32_bits_hex": f32_bits,
    }


def _u8(value: int) -> bytes:
    return struct.pack("<B", value)


def _u16(value: int) -> bytes:
    return struct.pack("<H", value)


def _u32(value: int) -> bytes:
    return struct.pack("<I", value)


def _u64(value: int) -> bytes:
    return struct.pack("<Q", value)


def _i32(value: int) -> bytes:
    return struct.pack("<i", value)


def _serialize_object_v2(index: int, row: dict[str, int]) -> bytes:
    return b"".join(
        (
            b"O",
            _u16(index),
            _u32(row["card_token"]),
            _u8(row["group"]),
            _u16(row["actor_visible_ordinal"]),
            _u8(row["owner_relative"]),
            _u8(row["controller_relative"]),
            _u8(row["zone"]),
            _u32(row["zone_change_count"]),
        )
    )


def _serialize_action(index: int, row: dict[str, int]) -> bytes:
    return b"".join(
        (
            b"A",
            _u32(index),
            _u8(row["kind"]),
            _u16(row["flags"]),
            _u8(row["ability_index"]),
            _u8(row["remaining"]),
            _u8(row["mode_index"]),
            _u8(row["mode_count"]),
            _u16(row["option_index"]),
            _u16(row["option_count"]),
            _u16(row["selected_count"]),
            _u16(row["min_targets"]),
            _u16(row["max_targets"]),
            _i32(row["number"]),
            _i32(row["minimum"]),
            _i32(row["maximum"]),
            _u8(row["mana_choice"]),
            _u8(row["color"]),
            _u8(row["cast_mode"]),
            _u8(row["cost_kind"]),
            _u8(row["optional_cost_choice"]),
            _u8(row["target_kind"]),
            _u8(row["target_player"]),
            _u32(row["ref_start"]),
            _u16(row["ref_len"]),
        )
    )


def _serialize_ref_v2(row: dict[str, int], obj: dict[str, int]) -> bytes:
    return b"".join(
        (
            b"R",
            _u32(row["action_index"]),
            _u8(row["role"]),
            _u16(row["order_index"]),
            _u16(row["associated_order"]),
            _u32(row["card_token"]),
            _u16(row["object_index"]),
            _u32(obj["card_token"]),
            _u8(obj["group"]),
            _u16(obj["actor_visible_ordinal"]),
            _u8(obj["owner_relative"]),
            _u8(obj["controller_relative"]),
            _u8(obj["zone"]),
            _u32(obj["zone_change_count"]),
        )
    )


def _serialize_object_v1(index: int, row: dict[str, int]) -> bytes:
    return b"".join(
        (
            b"O",
            _u16(index),
            _u16(row["card_token"]),
            _u8(row["group"]),
            _u16(row["actor_visible_ordinal"]),
            _u8(row["owner_relative"]),
            _u8(row["controller_relative"]),
            _u8(row["zone"]),
            _u32(row["zone_change_count"]),
        )
    )


def _serialize_ref_v1(row: dict[str, int], obj: dict[str, int]) -> bytes:
    return b"".join(
        (
            b"R",
            _u32(row["action_index"]),
            _u8(row["role"]),
            _u16(row["order_index"]),
            _u16(row["associated_order"]),
            _u16(row["card_token"]),
            _u16(row["object_index"]),
            _u16(obj["card_token"]),
            _u8(obj["group"]),
            _u16(obj["actor_visible_ordinal"]),
            _u8(obj["owner_relative"]),
            _u8(obj["controller_relative"]),
            _u8(obj["zone"]),
            _u32(obj["zone_change_count"]),
        )
    )


def _header(domain: bytes, versions: tuple[int, int, int, int], card_db_hash: int, actor: int, action_count: int, ref_count: int, object_count: int) -> bytes:
    return b"".join(
        (
            domain,
            *(_u32(version) for version in versions),
            _u64(card_db_hash),
            _u8(actor),
            _u32(action_count),
            _u32(ref_count),
            _u16(object_count),
        )
    )


def _commitment_record(stream: bytes) -> dict[str, str]:
    digest = hashlib.sha256(stream).digest()
    return {
        "stream_hex": stream.hex(),
        "sha256_hex": digest.hex(),
        "commitment_hex": digest[:16].hex(),
    }


def _assemble_v2_commitment_fixture(
    domain: bytes,
    card_db_hash: int,
    actor_seat: int,
    objects: list[dict[str, int]],
    actions: list[dict[str, int]],
    refs: list[dict[str, int]],
    claim_scope: str,
) -> dict[str, object]:
    header = _header(
        domain,
        (2, 1, 2, 2),
        card_db_hash,
        actor_seat,
        len(actions),
        len(refs),
        len(objects),
    )
    object_rows = [_serialize_object_v2(index, row) for index, row in enumerate(objects)]
    action_rows = [_serialize_action(index, row) for index, row in enumerate(actions)]
    reference_rows = [_serialize_ref_v2(row, objects[row["object_index"]]) for row in refs]
    stream = header + b"".join(object_rows)
    consumed_refs = 0
    for action_index, action in enumerate(actions):
        ref_start = action["ref_start"]
        ref_end = ref_start + action["ref_len"]
        if ref_start != consumed_refs or ref_end > len(refs):
            raise RuntimeError("golden action reference slices must be contiguous and in bounds")
        for ref_index in range(ref_start, ref_end):
            if refs[ref_index]["action_index"] != action_index:
                raise RuntimeError("golden reference action_index disagrees with its action slice")
            stream += reference_rows[ref_index]
        stream += action_rows[action_index]
        consumed_refs = ref_end
    if consumed_refs != len(refs):
        raise RuntimeError("golden action reference slices must consume every reference")
    return {
        "claim_scope": claim_scope,
        "actor_seat": actor_seat,
        "card_db_hash": card_db_hash,
        "action_count": len(actions),
        "ref_count": len(refs),
        "object_count": len(objects),
        "objects": objects,
        "actions": actions,
        "references": refs,
        "header_hex": header.hex(),
        "object_rows_hex": [row.hex() for row in object_rows],
        "reference_rows_hex": [row.hex() for row in reference_rows],
        "action_rows_hex": [row.hex() for row in action_rows],
        **_commitment_record(stream),
    }


def _action_commitment_goldens(action_contract: dict[str, Any], card_db_hash: int) -> dict[str, object]:
    domain = action_contract["candidate_commitment"]["domain_utf8"].encode("utf-8")
    stress_objects = [
        {
            "card_token": 65_535,
            "group": 6,
            "actor_visible_ordinal": 0x1234,
            "owner_relative": 0,
            "controller_relative": 1,
            "zone": 5,
            "zone_change_count": 0x12345678,
        },
        {
            "card_token": 65_536,
            "group": 8,
            "actor_visible_ordinal": 0xABCD,
            "owner_relative": 1,
            "controller_relative": 0,
            "zone": 7,
            "zone_change_count": 0x89ABCDEF,
        },
    ]
    stress_actions = [
        {
            "kind": 26,
            "flags": 0x1234,
            "ability_index": 1,
            "remaining": 2,
            "mode_index": 3,
            "mode_count": 4,
            "option_index": 0x0506,
            "option_count": 0x0708,
            "selected_count": 0x090A,
            "min_targets": 0x0B0C,
            "max_targets": 0x0D0E,
            "number": -123_456_789,
            "minimum": -234_567_890,
            "maximum": 345_678_901,
            "mana_choice": 15,
            "color": 16,
            "cast_mode": 17,
            "cost_kind": 18,
            "optional_cost_choice": 19,
            "target_kind": 20,
            "target_player": 21,
            "ref_start": 0,
            "ref_len": 0,
        },
        {
            "kind": 1,
            "flags": 0xABCD,
            "ability_index": 22,
            "remaining": 23,
            "mode_index": 24,
            "mode_count": 25,
            "option_index": 0x1617,
            "option_count": 0x1819,
            "selected_count": 0x1A1B,
            "min_targets": 0x1C1D,
            "max_targets": 0x1E1F,
            "number": 456_789_012,
            "minimum": -567_890_123,
            "maximum": 678_901_234,
            "mana_choice": 26,
            "color": 27,
            "cast_mode": 28,
            "cost_kind": 29,
            "optional_cost_choice": 30,
            "target_kind": 31,
            "target_player": 32,
            "ref_start": 0,
            "ref_len": 3,
        },
        {
            "kind": 1,
            "flags": 0x2468,
            "ability_index": 33,
            "remaining": 34,
            "mode_index": 35,
            "mode_count": 36,
            "option_index": 0x2021,
            "option_count": 0x2223,
            "selected_count": 0x2425,
            "min_targets": 0x2627,
            "max_targets": 0x2829,
            "number": -789_012_345,
            "minimum": 890_123_456,
            "maximum": 901_234_567,
            "mana_choice": 37,
            "color": 38,
            "cast_mode": 39,
            "cost_kind": 40,
            "optional_cost_choice": 41,
            "target_kind": 42,
            "target_player": 43,
            "ref_start": 3,
            "ref_len": 1,
        },
    ]
    stress_refs = [
        {
            "action_index": 1,
            "role": 2,
            "order_index": 0,
            "associated_order": 0x0304,
            "card_token": 65_535,
            "object_index": 0,
        },
        {
            "action_index": 1,
            "role": 7,
            "order_index": 1,
            "associated_order": 0x1314,
            "card_token": 65_536,
            "object_index": 1,
        },
        {
            "action_index": 1,
            "role": 2,
            "order_index": 2,
            "associated_order": 0x2324,
            "card_token": 65_535,
            "object_index": 0,
        },
        {
            "action_index": 2,
            "role": 7,
            "order_index": 0,
            "associated_order": 0x3334,
            "card_token": 65_536,
            "object_index": 1,
        },
    ]
    serializer_stress = _assemble_v2_commitment_fixture(
        domain,
        card_db_hash,
        1,
        stress_objects,
        stress_actions,
        stress_refs,
        "raw serializer field-order stress only; rows are intentionally not production-semantic",
    )

    # These rows are a synthetic concatenation of three independently valid
    # production semantic mappings.  They deliberately are not claimed to be
    # one homogeneous/reachable FastActor decision: the 0+3+1 slices exist to
    # pin ragged stream ordering and the distinct 3/4/2 count fields.
    semantic_objects = [
        {
            "card_token": 65_535,
            "group": 7,
            "actor_visible_ordinal": 0,
            "owner_relative": 0,
            "controller_relative": 0,
            "zone": 4,
            "zone_change_count": 0x12345678,
        },
        {
            "card_token": 65_536,
            "group": 2,
            "actor_visible_ordinal": 1,
            "owner_relative": 0,
            "controller_relative": 0,
            "zone": 2,
            "zone_change_count": 0x89ABCDEF,
        },
    ]
    zero_fields = {
        "ability_index": 0,
        "remaining": 0,
        "mode_index": 0,
        "mode_count": 0,
        "option_index": 0,
        "option_count": 0,
        "selected_count": 0,
        "min_targets": 0,
        "max_targets": 0,
        "number": 0,
        "minimum": 0,
        "maximum": 0,
        "mana_choice": 0,
        "color": 0,
        "cast_mode": 0,
        "cost_kind": 0,
        "optional_cost_choice": 0,
        "target_kind": 0,
        "target_player": 0,
    }
    semantic_actions = [
        {
            "kind": 18,
            "flags": 4,
            **zero_fields,
            "ref_start": 0,
            "ref_len": 0,
        },
        {
            "kind": 26,
            "flags": 0,
            **zero_fields,
            "ref_start": 0,
            "ref_len": 3,
        },
        {
            "kind": 4,
            "flags": 0,
            **zero_fields,
            "ability_index": 3,
            "ref_start": 3,
            "ref_len": 1,
        },
    ]
    semantic_refs = [
        {
            "action_index": 1,
            "role": 7,
            "order_index": 0,
            "associated_order": 2,
            "card_token": 65_535,
            "object_index": 0,
        },
        {
            "action_index": 1,
            "role": 7,
            "order_index": 1,
            "associated_order": 0,
            "card_token": 65_536,
            "object_index": 1,
        },
        {
            "action_index": 1,
            "role": 7,
            "order_index": 2,
            "associated_order": 1,
            "card_token": 65_535,
            "object_index": 0,
        },
        {
            "action_index": 2,
            "role": 0,
            "order_index": 0,
            "associated_order": 0,
            "card_token": 65_536,
            "object_index": 1,
        },
    ]
    production_semantic = _assemble_v2_commitment_fixture(
        domain,
        card_db_hash,
        1,
        semantic_objects,
        semantic_actions,
        semantic_refs,
        "synthetic concatenation of individually production-derived rows; not one reachable FastActor decision",
    )

    legacy_object = {
        "card_token": 65_535,
        "group": 6,
        "actor_visible_ordinal": 7,
        "owner_relative": 0,
        "controller_relative": 1,
        "zone": 5,
        "zone_change_count": 9,
    }
    legacy_action = {
        "kind": 0,
        "flags": 0,
        "ability_index": 0,
        "remaining": 0,
        "mode_index": 0,
        "mode_count": 0,
        "option_index": 0,
        "option_count": 0,
        "selected_count": 0,
        "min_targets": 0,
        "max_targets": 0,
        "number": 0,
        "minimum": 0,
        "maximum": 0,
        "mana_choice": 0,
        "color": 0,
        "cast_mode": 0,
        "cost_kind": 0,
        "optional_cost_choice": 0,
        "target_kind": 0,
        "target_player": 0,
        "ref_start": 0,
        "ref_len": 1,
    }
    legacy_ref = {
        "action_index": 0,
        "role": 2,
        "order_index": 3,
        "associated_order": 4,
        "card_token": 65_535,
        "object_index": 0,
    }
    v1_header = _header(
        b"mtg-kernel-flat-action-candidate-order-v1\0",
        (1, 1, 1, 1),
        card_db_hash,
        0,
        1,
        1,
        1,
    )
    v1_stream = (
        v1_header
        + _serialize_object_v1(0, legacy_object)
        + _serialize_ref_v1(legacy_ref, legacy_object)
        + _serialize_action(0, legacy_action)
    )
    v2_header = _header(domain, (2, 1, 2, 2), card_db_hash, 0, 1, 1, 1)
    v2_stream = (
        v2_header
        + _serialize_object_v2(0, legacy_object)
        + _serialize_ref_v2(legacy_ref, legacy_object)
        + _serialize_action(0, legacy_action)
    )
    return {
        "serialization_authority": "python_struct_little_endian_v1",
        "card_db_hash_authority": {
            "source": "mtg-kernel/src/card_def.rs::card_db_hash_v5_is_frozen",
            "value_hex": f"{card_db_hash:016x}",
        },
        "serializer_stress_v2": serializer_stress,
        "production_semantic_ragged_v2": production_semantic,
        "frozen_v1_comparison": _commitment_record(v1_stream),
        "same_representable_rows_v2": _commitment_record(v2_stream),
    }


def build_artifacts() -> tuple[dict[str, object], dict[str, object]]:
    assert_kernel_card_db_hash_authority()
    features = load_module_without_torch("_flat_policy_v2_generator_features", FEATURES)
    fixtures = load_plain_module("_flat_policy_v2_generator_fixtures", FIXTURES)
    full_cases, model_cases = _full_observation_cases(features, fixtures)

    action_contract_bytes = ACTION_CONTRACT.read_bytes()
    action_contract = json.loads(action_contract_bytes)
    base_inventory_bytes = BASE_INVENTORY.read_bytes()
    base_inventory = json.loads(base_inventory_bytes)
    base_goldens = json.loads(BASE_GOLDENS.read_text(encoding="utf-8"))
    action_ref_role_mapping = validated_action_ref_role_mapping(base_goldens, action_contract)
    inventory = json.loads(INVENTORY.read_text(encoding="utf-8"))
    inventory["base_inventory"] = {
        "schema": base_inventory["schema"],
        "source_sha256": sha256_hex(base_inventory_bytes),
        "canonical_sha256": sha256_hex(canonical_json_bytes(base_inventory)),
    }
    base_layout_sha256 = sha256_hex(BASE_LAYOUT.read_bytes())
    if base_layout_sha256 != base_inventory["rust_typed_layout_sha256"]:
        raise AssertionError("V1 typed-layout source digest drift")
    overlay_layout_sha256 = sha256_hex(OVERLAY_LAYOUT.read_bytes())
    composite = hashlib.sha256(
        b"mtg-kernel-flat-policy-typed-layout-v2\0"
        + bytes.fromhex(base_layout_sha256)
        + bytes.fromhex(overlay_layout_sha256)
    ).hexdigest()
    inventory["typed_layout"] = {
        "domain": "mtg-kernel-flat-policy-typed-layout-v2\u0000".encode().decode("unicode_escape"),
        "base_sha256": base_layout_sha256,
        "overlay_sha256": overlay_layout_sha256,
        "composite_sha256": composite,
    }
    inventory["action_contract"] = {
        "path": "data/flat_policy_v2/action_contract_v2.json",
        "schema": action_contract["schema"],
        "source_sha256": sha256_hex(action_contract_bytes),
        "canonical_sha256": sha256_hex(canonical_json_bytes(action_contract)),
    }
    inventory["unchanged_contracts"] = {
        "mapping_sha256": base_goldens["mapping_sha256"],
        "action_ref_role_crosswalk_sha256": action_ref_role_mapping["canonical_sha256"],
        "authoritative_features_sha256": base_inventory["authoritative_features_sha256"],
        "feature_contract_digest": base_inventory["feature_contract_digest"],
        "encoding_contract_digest": base_inventory["encoding_contract_digest"],
    }
    inventory["topology_audit"] = {
        "path": "data/flat_policy_v2/ordered_topology_audit_v2.md",
        "source_sha256": sha256_hex(TOPOLOGY_AUDIT.read_bytes()),
    }

    action_goldens = _action_commitment_goldens(action_contract, KERNEL_CARDDB_HASH)
    goldens: dict[str, object] = {
        "schema": "flat-policy-v2-independent-goldens-v1",
        "inventory_sha256": sha256_hex(canonical_json_bytes(inventory)),
        "action_contract_binding": {
            "source_sha256": inventory["action_contract"]["source_sha256"],
            "canonical_sha256": inventory["action_contract"]["canonical_sha256"],
        },
        "action_ref_role_mapping": action_ref_role_mapping,
        "hash_contract": {
            "namespace": "observation-state",
            "counter_encoding": "u32_le",
            "sha512_block_count": 6,
            "word_encoding": "u32_le",
            "word_count": 96,
            "feature_formula": "(f64(word)/f64(0xffffffff))*2.0-1.0",
            "feature_cast": "ieee754_binary32_round_to_nearest_even",
        },
        "observation_authority": {
            "schema": "python.mtg_kernel_rl.features.OBSERVATION_SPEC",
            "semantic_validator": "assert_observation_classified",
            "canonicalizer": "_canonical_model_value_then_canonical_json_utf8",
            "source_fixture": "python/tests/fixtures.py:observation",
            "red_pair_only_difference": "observation.projection.combat.attacker_to_ordered_blockers",
        },
        "full_observation_cases": full_cases,
        "model_canonical_cases": model_cases,
        "blocked_order_by_ordered_attacker": {
            "absent": [0, None],
            "present_empty_forward": [0, 1],
            "present_empty_reverse": [1, 0],
        },
        "red_pair": {
            "left_case": "present_empty_forward",
            "right_case": "present_empty_reverse",
            "left": _state_hash_record(model_cases["present_empty_forward"].encode("utf-8")),
            "right": _state_hash_record(model_cases["present_empty_reverse"].encode("utf-8")),
        },
        "action_commitment": action_goldens,
        "card_token_boundary": {
            "u16_minus_one_card_db_id": 65_534,
            "u16_minus_one_v2_token": 65_535,
            "u16_max_card_db_id": 65_535,
            "u16_max_v2_token": 65_536,
            "v1_u16_max_card_db_id_disposition": "reject_checked_integer_range",
        },
    }
    payload = canonical_json_bytes(goldens)
    goldens["payload_sha256"] = sha256_hex(payload)
    return inventory, goldens


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    inventory, goldens = build_artifacts()
    expected_inventory = pretty_json(inventory)
    expected_goldens = pretty_json(goldens)
    if args.check:
        if INVENTORY.read_text(encoding="utf-8") != expected_inventory:
            raise SystemExit(f"{INVENTORY} is stale; regenerate it")
        if GOLDENS.read_text(encoding="utf-8") != expected_goldens:
            raise SystemExit(f"{GOLDENS} is stale; regenerate it")
        return
    INVENTORY.write_text(expected_inventory, encoding="utf-8", newline="\n")
    GOLDENS.write_text(expected_goldens, encoding="utf-8", newline="\n")


if __name__ == "__main__":
    main()
