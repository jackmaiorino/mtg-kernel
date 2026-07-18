#!/usr/bin/env python3
"""Generate Python-authoritative action tensorization goldens.

The Rust native action tensorizer reconstructs the model-input portion of a
Policy-v5 legal action from ``FlatScoringDecisionViewV1``.  This generator uses
the frozen Python feature implementation as the numerical and canonical-JSON
authority.  It also emits the compact scorer-side rows needed to construct an
equivalent Rust view without importing operational action metadata.
"""

from __future__ import annotations

import argparse
import copy
import hashlib
import json
import struct
import sys
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any, Iterable


REPO_ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO_ROOT / "python"))

from mtg_kernel_rl import features as f  # noqa: E402


OUTPUT = REPO_ROOT / "data" / "flat_policy_v1" / "python_action_features_v1.json"

ROLE_IDS = {
    "source": 0,
    "candidate": 1,
    "card": 2,
    "attacker": 3,
    "blocker": 4,
    "target_object": 5,
    "cards": 6,
    "pending_sources": 9,
}
ZONE_IDS = {name: index for index, name in enumerate(f.ZONES)}
COLOR_IDS = {name: index + 1 for index, name in enumerate(f.MANA_COLORS)}
CAST_MODE_IDS = {name: index + 1 for index, name in enumerate(f.CAST_MODES)}
COST_KIND_IDS = {name: index + 1 for index, name in enumerate(f.COST_KINDS)}
OPTIONAL_CHOICE_IDS = {
    name: index + 1 for index, name in enumerate(f.OPTIONAL_COST_CHOICES)
}

FLAG_PAY = 1 << 0
FLAG_CHANGE_TARGET = 1 << 1
FLAG_USE_COST = 1 << 2
FLAG_CAST_IT = 1 << 3
FLAG_VALUE = 1 << 4
FLAG_INCLUDE = 1 << 5


@dataclass
class Case:
    name: str
    action: dict[str, Any]
    object_order: list[dict[str, Any]] | None = None
    coverage: list[str] = field(default_factory=list)


def stable_ref(
    arena_id: int,
    card_db_id: int,
    owner: str,
    controller: str,
    zone: str,
    *,
    zone_change_count: int = 0,
) -> dict[str, Any]:
    return {
        "arena_id": arena_id,
        "card_db_id": card_db_id,
        "owner": owner,
        "controller": controller,
        "zone": zone,
        "zone_change_count": zone_change_count,
    }


def legal_action(
    semantic: dict[str, Any],
    *,
    selected_index: int = 0,
    stable_id: str = "legal-action-v5:native-flat-golden",
    display_text: str | None = "ignored metadata",
) -> dict[str, Any]:
    return {
        "schema_version": 5,
        "selected_index": selected_index,
        "stable_id": stable_id,
        "display_text": display_text,
        "semantic": semantic,
    }


def other_seat(actor: str) -> str:
    return "p1" if actor == "p0" else "p0"


def relative_id(seat: str, actor: str) -> int:
    if seat == actor:
        return 0
    if seat == other_seat(actor):
        return 1
    raise ValueError(f"invalid seat {seat!r} for actor {actor!r}")


def semantic_refs(semantic: dict[str, Any]) -> list[dict[str, Any]]:
    refs: list[dict[str, Any]] = []
    for role in ("source", "candidate", "card", "attacker", "blocker"):
        if role in semantic:
            refs.append(semantic[role])
    target = semantic.get("target")
    if target is not None and target["target_kind"] == "object":
        refs.append(target["object"])
    refs.extend(semantic.get("cards", []))
    refs.extend(semantic.get("attackers", []))
    refs.extend(semantic.get("blockers", []))
    refs.extend(semantic.get("pending_sources", []))
    return refs


def stable_key(ref: dict[str, Any]) -> tuple[int, int]:
    return (int(ref["arena_id"]), int(ref["zone_change_count"]))


def unique_refs(refs: Iterable[dict[str, Any]]) -> list[dict[str, Any]]:
    out: list[dict[str, Any]] = []
    seen: set[tuple[int, int]] = set()
    for ref in refs:
        key = stable_key(ref)
        if key not in seen:
            out.append(ref)
            seen.add(key)
    return out


def registry_for(case: Case) -> tuple[Any, list[dict[str, Any]]]:
    actor = case.action["semantic"]["actor"]
    semantic_order = unique_refs(semantic_refs(case.action["semantic"]))
    ordered = case.object_order or semantic_order
    if {stable_key(ref) for ref in ordered} != {stable_key(ref) for ref in semantic_order}:
        raise ValueError(f"{case.name}: object_order is not the semantic reference set")
    registry = f._NodeRegistry(actor, current_turn=0)
    for order, ref in enumerate(ordered):
        registry.add_ref_node(ref, "private_context", order, "private")
    return registry, ordered


def flat_refs(
    semantic: dict[str, Any], registry: Any
) -> list[dict[str, int]]:
    rows: list[dict[str, int]] = []

    def emit(role: str, order: int, associated: int, ref: dict[str, Any]) -> None:
        rows.append(
            {
                "action_index": 0,
                "projection_role_id": ROLE_IDS[role],
                "order_index": order,
                "associated_order": associated,
                "card_token": f._card_token(ref),
                "model_object_index": registry.resolve(ref),
            }
        )

    for role in ("source", "candidate", "card", "attacker", "blocker"):
        if role in semantic:
            emit(role, 0, 0, semantic[role])
    target = semantic.get("target")
    if target is not None and target["target_kind"] == "object":
        emit("target_object", 0, 0, target["object"])
    for role in ("cards", "attackers", "blockers"):
        for index, ref in enumerate(semantic.get(role, [])):
            emit(role, index, 0, ref)
    if semantic["action_kind"] == "order_triggers":
        for index, ref in enumerate(semantic["pending_sources"]):
            emit("pending_sources", index, int(semantic["order"][index]), ref)
    return rows


def flat_core(semantic: dict[str, Any], refs: list[dict[str, int]]) -> dict[str, Any]:
    kind = semantic["action_kind"]
    core: dict[str, Any] = {
        "kind": kind,
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
        "ref_len": len(refs),
    }
    for field in (
        "ability_index",
        "remaining",
        "mode_index",
        "mode_count",
        "option_index",
        "option_count",
        "selected_count",
        "min_targets",
        "max_targets",
        "number",
        "minimum",
        "maximum",
    ):
        if field in semantic:
            core[field] = int(semantic[field])
    if semantic.get("pay", False):
        core["flags"] |= FLAG_PAY
    if semantic.get("change_target", False):
        core["flags"] |= FLAG_CHANGE_TARGET
    if semantic.get("use_cost", False):
        core["flags"] |= FLAG_USE_COST
    if semantic.get("cast_it", False):
        core["flags"] |= FLAG_CAST_IT
    if semantic.get("value", False):
        core["flags"] |= FLAG_VALUE
    if semantic.get("include", False):
        core["flags"] |= FLAG_INCLUDE
    mana = semantic.get("mana_choice")
    if mana is not None:
        core["mana_choice"] = COLOR_IDS[mana]
    if "color" in semantic:
        core["color"] = COLOR_IDS[semantic["color"]]
    if "mode" in semantic:
        core["cast_mode"] = CAST_MODE_IDS[semantic["mode"]]
    if "cost_kind" in semantic:
        core["cost_kind"] = COST_KIND_IDS[semantic["cost_kind"]]
    if "choice" in semantic:
        core["optional_cost_choice"] = OPTIONAL_CHOICE_IDS[semantic["choice"]]
    target = semantic.get("target")
    if target is not None:
        if target["target_kind"] == "player":
            core["target_kind"] = 1
            core["target_player"] = relative_id(
                target["player"], semantic["actor"]
            ) + 1
        else:
            core["target_kind"] = 2
    return core


def f32_bits(values: Iterable[float]) -> list[int]:
    return [struct.unpack("<I", struct.pack("<f", float(value)))[0] for value in values]


def f32_le_hex(values: Iterable[float]) -> str:
    return b"".join(struct.pack("<f", float(value)) for value in values).hex()


def digest_blocks(payload: bytes) -> list[bytes]:
    return [
        hashlib.sha512(b"legal-action" + counter.to_bytes(4, "little") + payload).digest()
        for counter in range(6)
    ]


def block_features(blocks: list[bytes]) -> list[float]:
    out: list[float] = []
    for block in blocks:
        for offset in range(0, len(block), 4):
            chunk = int.from_bytes(block[offset : offset + 4], "little")
            out.append((float(chunk) / float(0xFFFF_FFFF)) * 2.0 - 1.0)
    return out


def case_payload(case: Case) -> dict[str, Any]:
    f.assert_action_classified(case.action)
    semantic = case.action["semantic"]
    actor = semantic["actor"]
    registry, ordered_objects = registry_for(case)
    action_features, ref_features, ref_tokens, ref_nodes = f._action_features(
        case.action, actor, registry
    )
    canonical = f._canonical_model_value(
        case.action,
        f.LEGAL_ACTION_SPEC,
        ("legal_action",),
        f._CanonicalContext(actor),
    )
    canonical_json = json.dumps(
        canonical, sort_keys=True, separators=(",", ":"), ensure_ascii=False
    )
    canonical_bytes = canonical_json.encode("utf-8")
    blocks = digest_blocks(canonical_bytes)
    independent_hash_features = block_features(blocks)
    if f32_bits(independent_hash_features) != f32_bits(
        action_features[f.ACTION_FEATURE_DIM - f.ACTION_HASH_DIM :]
    ):
        raise AssertionError(f"{case.name}: independent digest feature mismatch")
    refs = flat_refs(semantic, registry)
    core = flat_core(semantic, refs)
    objects = [
        {
            "card_token": f._card_token(ref),
            "owner": relative_id(ref["owner"], actor),
            "controller": relative_id(ref["controller"], actor),
            "zone": ZONE_IDS[ref["zone"]],
        }
        for ref in ordered_objects
    ]
    return {
        "name": case.name,
        "coverage": case.coverage,
        "flat_input": {"core": core, "objects": objects, "refs": refs},
        "canonical_json": canonical_json,
        "sha512_blocks_hex": [block.hex() for block in blocks],
        "full_feature_f32_le_hex": f32_le_hex(action_features),
        "action_ref_feature_f32_le_hex": f32_le_hex(
            value for row in ref_features for value in row
        ),
        "action_ref_card_ids": ref_tokens,
        "action_ref_node_indices": ref_nodes,
    }


def primary_cases() -> list[Case]:
    actor = "p0"
    opponent = "p1"
    source = stable_ref(10, 40, actor, actor, "Hand")
    target = stable_ref(11, 41, opponent, opponent, "Battlefield")
    candidate = stable_ref(12, 42, actor, actor, "Graveyard")
    semantic_rows = [
        {"action_kind": "pass", "actor": actor},
        {"action_kind": "play_land", "actor": actor, "source": source},
        {"action_kind": "cast_spell", "actor": actor, "source": source},
        {"action_kind": "activate_mana_ability", "actor": actor, "source": source, "mana_choice": "R"},
        {"action_kind": "activate_ability", "actor": actor, "source": source, "ability_index": 7},
        {"action_kind": "plot_spell", "actor": actor, "source": source},
        {"action_kind": "choose_target", "actor": actor, "source": source, "remaining": 2, "target": {"target_kind": "player", "player": opponent}},
        {"action_kind": "choose_cost_target", "actor": actor, "source": source, "cost_kind": "PutCounters", "remaining": 3, "candidate": candidate},
        {"action_kind": "choose_cast_mode", "actor": actor, "source": source, "mode": "Alternative"},
        {"action_kind": "choose_kicker", "actor": actor, "source": source, "pay": True},
        {"action_kind": "choose_spell_mode", "actor": actor, "source": source, "mode_index": 1, "mode_count": 2},
        {"action_kind": "choose_effect_option", "actor": actor, "source": source, "option_index": 2, "option_count": 4},
        {"action_kind": "choose_effect_target", "actor": actor, "source": source, "target": {"target_kind": "object", "object": target}, "selected_count": 1, "min_targets": 1, "max_targets": 3},
        {"action_kind": "finish_effect_selection", "actor": actor, "source": source, "selected_count": 2},
        {"action_kind": "choose_effect_color", "actor": actor, "source": source, "color": "G"},
        {"action_kind": "choose_effect_number", "actor": actor, "source": source, "number": 2, "minimum": -3, "maximum": 5},
        {"action_kind": "choose_effect_boolean", "actor": actor, "source": source, "value": True},
        {"action_kind": "finish_target_selection", "actor": actor, "source": source, "selected_count": 2},
        {"action_kind": "choose_optional_cost_use", "actor": actor, "use_cost": True},
        {"action_kind": "choose_optional_cost_which", "actor": actor, "choice": "SacrificeLand"},
        {"action_kind": "choose_spell_copy_payment", "actor": actor, "source": source, "pay": True},
        {"action_kind": "choose_spell_copy_retarget", "actor": actor, "source": source, "change_target": True},
        {"action_kind": "choose_madness_cast", "actor": actor, "card": source, "cast_it": True},
        {"action_kind": "discard", "actor": actor, "cards": [candidate, source]},
        {"action_kind": "choose_attacker_inclusion", "actor": actor, "attacker": target, "include": True},
        {"action_kind": "choose_blocker_inclusion", "actor": actor, "attacker": target, "blocker": candidate, "include": True},
        {"action_kind": "order_triggers", "actor": actor, "pending_sources": [source, candidate], "order": [1, 0]},
    ]
    cases: list[Case] = []
    for semantic in semantic_rows:
        kind = semantic["action_kind"]
        object_order = None
        if kind == "discard":
            # Node order is source then candidate, semantic order is candidate
            # then source, and canonical JSON sorts by the card-ref JSON key.
            object_order = [source, candidate]
        cases.append(
            Case(
                f"primary-{kind}",
                legal_action(semantic),
                object_order=object_order,
                coverage=["all-27-variants", kind],
            )
        )
    if len(cases) != len(f.ACTION_KINDS):
        raise AssertionError("primary case set must contain exactly 27 variants")
    if [case.action["semantic"]["action_kind"] for case in cases] != f.ACTION_KINDS:
        raise AssertionError("primary case order must match ACTION_KINDS")
    return cases


def supplementary_cases() -> list[Case]:
    cases: list[Case] = []

    def add(name: str, semantic: dict[str, Any], *coverage: str, object_order: list[dict[str, Any]] | None = None, **metadata: Any) -> None:
        cases.append(
            Case(
                name,
                legal_action(semantic, **metadata),
                object_order=object_order,
                coverage=list(coverage),
            )
        )

    p0 = "p0"
    p1 = "p1"
    source = stable_ref(100, 100, p0, p0, "Hand")
    target = stable_ref(101, 101, p1, p1, "Battlefield")

    boolean_rows = [
        ("kicker", "choose_kicker", "pay", source, "source"),
        ("effect", "choose_effect_boolean", "value", source, "source"),
        ("optional-use", "choose_optional_cost_use", "use_cost", None, None),
        ("copy-payment", "choose_spell_copy_payment", "pay", source, "source"),
        ("copy-retarget", "choose_spell_copy_retarget", "change_target", source, "source"),
        ("madness", "choose_madness_cast", "cast_it", source, "card"),
        ("attacker", "choose_attacker_inclusion", "include", target, "attacker"),
    ]
    for label, kind, flag, ref, ref_field in boolean_rows:
        for value in (False, True):
            semantic: dict[str, Any] = {"action_kind": kind, "actor": p0, flag: value}
            if ref_field is not None:
                semantic[ref_field] = ref
            add(
                f"boolean-{label}-{'true' if value else 'false'}",
                semantic,
                "booleans",
                flag,
            )
    blocker = stable_ref(102, 102, p0, p0, "Battlefield")
    for value in (False, True):
        add(
            f"boolean-blocker-{'true' if value else 'false'}",
            {"action_kind": "choose_blocker_inclusion", "actor": p0, "attacker": target, "blocker": blocker, "include": value},
            "booleans",
            "include",
        )

    for mana in [None, *f.MANA_COLORS]:
        add(
            f"mana-{'none' if mana is None else mana}",
            {"action_kind": "activate_mana_ability", "actor": p0, "source": source, "mana_choice": mana},
            "mana-choice",
            "all-colors",
        )
    for color in f.MANA_COLORS:
        add(
            f"effect-color-{color}",
            {"action_kind": "choose_effect_color", "actor": p0, "source": source, "color": color},
            "effect-color",
            "all-colors",
        )

    for label, target_value in (
        ("self", {"target_kind": "player", "player": p0}),
        ("opponent", {"target_kind": "player", "player": p1}),
        ("object", {"target_kind": "object", "object": target}),
    ):
        add(
            f"target-{label}",
            {"action_kind": "choose_target", "actor": p0, "source": source, "remaining": 1, "target": target_value},
            "targets",
            label,
        )

    arena = 200
    card_id = 200
    for owner in (p0, p1):
        for controller in (p0, p1):
            for zone in f.ZONES:
                ref = stable_ref(arena, card_id, owner, controller, zone)
                add(
                    f"stable-ref-{relative_id(owner, p0)}-{relative_id(controller, p0)}-{zone.lower()}",
                    {"action_kind": "cast_spell", "actor": p0, "source": ref},
                    "owner-controller-zone",
                )
                arena += 1
                card_id += 1

    for label, number in (("minimum", -(1 << 31)), ("maximum", (1 << 31) - 1)):
        add(
            f"i32-{label}",
            {"action_kind": "choose_effect_number", "actor": p0, "source": source, "number": number, "minimum": -(1 << 31), "maximum": (1 << 31) - 1},
            "i32-boundaries",
        )

    trigger_one = stable_ref(300, 300, p0, p0, "Stack")
    add(
        "triggers-one-identity",
        {"action_kind": "order_triggers", "actor": p0, "pending_sources": [trigger_one], "order": [0]},
        "trigger-order",
        "count-1",
    )
    trigger_seven = [
        stable_ref(310 + index, 310 + index, p0, p0, "Stack")
        for index in range(7)
    ]
    add(
        "triggers-seven-identity",
        {"action_kind": "order_triggers", "actor": p0, "pending_sources": trigger_seven, "order": list(range(7))},
        "trigger-order",
        "count-7",
        "identity",
    )
    add(
        "triggers-seven-permuted",
        {"action_kind": "order_triggers", "actor": p0, "pending_sources": trigger_seven, "order": [6, 0, 4, 2, 5, 1, 3]},
        "trigger-order",
        "count-7",
        "permutation",
        object_order=list(reversed(trigger_seven)),
    )

    low = stable_ref(400, 0, p0, p0, "Hand")
    high = stable_ref(401, 65_534, p0, p0, "Hand")
    add(
        "card-token-1",
        {"action_kind": "cast_spell", "actor": p0, "source": low},
        "card-token-boundary",
        "token-1",
    )
    add(
        "card-token-65535",
        {"action_kind": "cast_spell", "actor": p0, "source": high},
        "card-token-boundary",
        "token-65535",
    )

    meta_semantic = {"action_kind": "cast_spell", "actor": p0, "source": source}
    add(
        "metadata-invariance-a",
        copy.deepcopy(meta_semantic),
        "metadata-invariance",
        selected_index=0,
        stable_id="legal-action-v5:metadata-a",
        display_text="alpha",
    )
    add(
        "metadata-invariance-b",
        copy.deepcopy(meta_semantic),
        "metadata-invariance",
        selected_index=(1 << 32) - 1,
        stable_id="legal-action-v5:metadata-b",
        display_text=None,
    )
    p1_source = stable_ref(100, 100, p1, p1, "Hand")
    add(
        "actor-p1-relative-self",
        {"action_kind": "cast_spell", "actor": p1, "source": p1_source},
        "actor-p0-p1",
    )
    add(
        "actor-p0-relative-self",
        copy.deepcopy(meta_semantic),
        "actor-p0-p1",
    )

    for mode in f.CAST_MODES:
        add(
            f"cast-mode-{mode.lower()}",
            {"action_kind": "choose_cast_mode", "actor": p0, "source": source, "mode": mode},
            "cast-modes",
        )
    for cost in f.COST_KINDS:
        add(
            f"cost-kind-{cost}",
            {"action_kind": "choose_cost_target", "actor": p0, "source": source, "cost_kind": cost, "remaining": 1, "candidate": target},
            "cost-kinds",
        )
    for choice in f.OPTIONAL_COST_CHOICES:
        add(
            f"optional-choice-{choice}",
            {"action_kind": "choose_optional_cost_which", "actor": p0, "choice": choice},
            "optional-cost-choices",
        )
    return cases


def python_only_token_65536_case() -> dict[str, Any]:
    ref = stable_ref(9_999, 65_535, "p0", "p0", "Hand")
    action = legal_action(
        {"action_kind": "cast_spell", "actor": "p0", "source": ref},
        stable_id="legal-action-v5:python-token-65536",
    )
    case = Case("python-only-card-token-65536", action)
    f.assert_action_classified(action)
    registry, _ = registry_for(case)
    features, ref_features, ref_tokens, ref_nodes = f._action_features(
        action, "p0", registry
    )
    canonical = f._canonical_model_value(
        action,
        f.LEGAL_ACTION_SPEC,
        ("legal_action",),
        f._CanonicalContext("p0"),
    )
    canonical_json = json.dumps(
        canonical, sort_keys=True, separators=(",", ":"), ensure_ascii=False
    )
    return {
        "name": case.name,
        "status": "domain-coverage-blocker",
        "reason": "Python card_db_id 65535 maps to token 65536, which FlatScorerActionRefV1.card_token (u16) cannot represent",
        "python_card_db_id": 65_535,
        "python_card_token": 65_536,
        "canonical_json": canonical_json,
        "canonical_utf8_hex": canonical_json.encode("utf-8").hex(),
        "full_feature_f32_bits": f32_bits(features),
        "action_ref_feature_f32_bits": [f32_bits(row) for row in ref_features],
        "action_ref_card_ids": ref_tokens,
        "action_ref_node_indices": ref_nodes,
    }


def canonical_payload_bytes(value: Any) -> bytes:
    return json.dumps(
        value, sort_keys=True, separators=(",", ":"), ensure_ascii=False
    ).encode("utf-8")


def build_payload() -> dict[str, Any]:
    cases = [case_payload(case) for case in [*primary_cases(), *supplementary_cases()]]
    by_name = {case["name"]: case for case in cases}
    for left, right in (
        ("metadata-invariance-a", "metadata-invariance-b"),
        ("actor-p0-relative-self", "actor-p1-relative-self"),
    ):
        for field in (
            "canonical_json",
            "full_feature_f32_le_hex",
            "action_ref_feature_f32_le_hex",
            "action_ref_card_ids",
        ):
            if by_name[left][field] != by_name[right][field]:
                raise AssertionError(f"invariance pair {left}/{right} differs in {field}")

    features_path = REPO_ROOT / "python" / "mtg_kernel_rl" / "features.py"
    payload: dict[str, Any] = {
        "schema": "mtg-kernel-python-action-features-golden/v1",
        "authority": "python/mtg_kernel_rl/features.py",
        "authority_sha256": hashlib.sha256(features_path.read_bytes()).hexdigest(),
        "python_contracts": {
            "feature_schema_version": f.FEATURE_SCHEMA_VERSION,
            "feature_registry_version": f.FEATURE_REGISTRY_VERSION,
            "encoding_contract_version": f.ENCODING_CONTRACT_VERSION,
            "model_contract_version": f.MODEL_CONTRACT_VERSION,
        },
        "dimensions": {
            "action_explicit": f.ACTION_FEATURE_DIM - f.ACTION_HASH_DIM,
            "action_hash": f.ACTION_HASH_DIM,
            "action": f.ACTION_FEATURE_DIM,
            "action_ref": f.ACTION_REF_FEATURE_DIM,
        },
        "explicit_layout": [
            "action_kind[27]",
            "actor_relative[3]",
            "source_like_card_ref[13]",
            "target_kind[2]",
            "target_player_relative[3]",
            "ability_index/8",
            "remaining/8",
            "mode_index/8",
            "mode_count/8",
            "option_index/16",
            "option_count/16",
            "pay",
            "change_target",
            "use_cost",
            "cast_it",
            "cards_len/8",
            "attackers_len/16",
            "blockers_len/16",
            "pending_sources_len/16",
            "order_len/16",
            "selected_count/16",
            "min_targets/16",
            "max_targets/16",
            "number/16",
            "minimum/16",
            "maximum/16",
            "value",
            "mana_choice_present",
            "mana_choice[6]",
            "color[6]",
            "cast_mode[2]",
            "cost_kind[11]",
            "optional_cost_choice[3]",
        ],
        "action_ref_layout": [
            "role[10]",
            "card_ref[13]",
            "order_index/32",
            "associated_order/32",
        ],
        "hash_contract": {
            "namespace_ascii": "legal-action",
            "counter_encoding": "u32_le",
            "block_count": 6,
            "block_hash": "sha512",
            "chunk_encoding": "u32_le",
            "chunk_to_float": "f64(chunk)/f64(0xffffffff)*2.0-1.0 then one f32 cast",
        },
        "current_rust_card_token_max": 65_535,
        "domain_coverage_blockers": [python_only_token_65536_case()],
        "cases": cases,
    }
    payload["payload_sha256"] = hashlib.sha256(canonical_payload_bytes(payload)).hexdigest()
    return payload


def rendered_payload() -> bytes:
    return (
        json.dumps(build_payload(), indent=2, sort_keys=True, ensure_ascii=False) + "\n"
    ).encode("utf-8")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--check", action="store_true")
    parser.add_argument("--output", type=Path, default=OUTPUT)
    args = parser.parse_args()
    rendered = rendered_payload()
    if args.check:
        if not args.output.exists() or args.output.read_bytes() != rendered:
            print(f"stale Python action feature golden: {args.output}", file=sys.stderr)
            return 1
        return 0
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_bytes(rendered)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
