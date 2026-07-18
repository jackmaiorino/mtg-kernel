"""Generate the fail-closed flat-policy-v1 field inventory and schema goldens.

This tool is intentionally Python-authoritative for feature classification and
enum ordering.  It emits one declared typed destination for every classified
leaf and rejects missing or duplicate assignments.  Runtime Burn/Rally row
fixtures, source binding, and Rust tests separately check the producer; this
generator does not infer Rust semantics merely from source text.  It owns the
version/provenance envelope and the hand-authored enum and optional-value
vectors used by those tests.
"""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
from typing import Any

from mtg_kernel_rl.features import (
    ACTION_KINDS,
    ACTION_REF_ROLES,
    BOOLEAN_CHOICE_PURPOSES,
    CAST_METHODS,
    CAST_MODES,
    COST_KINDS,
    EDGE_ROLES,
    ENCODING_CONTRACT_VERSION,
    ENGINE_STAGES,
    EXPIRY_KINDS,
    FEATURE_REGISTRY_VERSION,
    FEATURE_SCHEMA_VERSION,
    FORBIDDEN,
    MANA_COLORS,
    MODEL_INPUT,
    OBJECT_GROUPS,
    OBJECT_SOURCE_KINDS,
    OPERATIONAL_ONLY,
    OPTIONAL_COST_CHOICES,
    PHASES,
    PLAY_OR_CAST,
    POLICY_SURFACE_STAGES,
    SPELL_COPY_STAGES,
    STACK_KINDS,
    SURFACE_STAGES,
    TARGET_KINDS,
    TARGET_SELECTION_PURPOSES,
    ZONES,
    classification_registry,
    encoding_contract_fingerprint,
    feature_contract_fingerprint,
)

ROOT = Path(__file__).resolve().parents[2]
INVENTORY_PATH = ROOT / "data" / "flat_policy_v1" / "feature_inventory_v1.json"
GOLDENS_PATH = ROOT / "data" / "flat_policy_v1" / "goldens_v1.json"
RUST_SOURCE = ROOT / "mtg-kernel" / "src" / "flat_policy_v1.rs"
CARDS_PATH = ROOT / "data" / "cards_v1.json"


def _sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def _destination(path: str, classification: str) -> tuple[str, str]:
    if classification == FORBIDDEN:
        return "absent", "forbidden"
    if classification == OPERATIONAL_ONLY:
        if path.endswith(".arena_id") or path.endswith(".zone_change_count"):
            return "private_reference_validation", "operational_validation"
        if path.endswith(".timestamp") or path.endswith(".turn"):
            return "private_order_and_turn_normalization", "operational_validation"
        if "zone_change_generation" in path:
            return "permission_incarnation_validation", "operational_validation"
        if "attachments.[]" in path:
            return "private_attachment_resolution", "operational_validation"
        return "FlatDecisionBindingV1", "operational_binding"

    if path.startswith("legal_action") or ".legal_action." in path:
        if path.endswith(".semantic.actor"):
            return "actor_relative_self_constant", "model_input"
        if any(token in path for token in (".source.", ".candidate.", ".card.", ".attacker.", ".blocker.", ".target.object.", ".cards.[]", ".pending_sources.[]")):
            if path.endswith(".card_db_id"):
                return "FlatActionRefV1.card_token", "model_input"
            return "FlatObjectCoreV1 via operational_reference_resolution", "model_input"
        return "FlatActionCoreV1", "model_input"

    if ".completed_dungeons.[]" in path:
        return "FlatCompletedDungeonV1", "model_input"
    if ".effective_subtype_ids.[]" in path:
        return "FlatObjectSubtypeV1", "model_input"
    if ".ability_uses_this_turn.[]" in path:
        return "FlatObjectAbilityUseV1", "model_input"
    if ".goaded_by.[]" in path:
        return "FlatObjectGoadV1", "model_input"
    if ".continuous_effects.[].add_subtype_ids.[]" in path or ".continuous_effects.[].remove_subtype_ids.[]" in path:
        return "FlatEffectSubtypeChangeV1", "model_input"
    if ".structural_path.[]" in path or ".legal_colors.[]" in path:
        return "FlatContextPathElementV1", "model_input"

    object_prefixes = (
        "observation.own_hand.[]",
        "observation.known_library_cards.[]",
        "observation.known_hand_cards.[]",
        "observation.projection.battlefield.[]",
        "observation.projection.graveyards.[]",
        "observation.projection.exile.[]",
    )
    if path.startswith(object_prefixes):
        if ".position" in path:
            return "FlatRelationV1.KnownLibrary", "model_input"
        return "FlatObjectCoreV1", "model_input"

    relation_markers = (
        ".projection.stack.",
        ".projection.combat.ordered_attackers",
        ".projection.combat.attacker_to_ordered_blockers",
        ".projection.continuous_effects.",
        ".projection.object_relations.",
        ".projection.exile_play_permissions.",
        ".engine_context.pending_",
        ".surface_context.madness_cast_reprompt_source",
        ".surface_context.private_blockers",
        ".surface_context.private_discard.chosen",
        ".surface_context.private_discard.remaining_choices",
        ".policy_surface_context.private_combat_selection.attacker",
        ".policy_surface_context.private_combat_selection.selected",
        ".policy_surface_context.private_combat_selection.current_candidate",
        ".policy_surface_context.private_combat_selection.remaining_after_current",
    )
    if any(marker in path for marker in relation_markers):
        scalar_global_markers = (
            ".<present>",
            ".current_stage",
            ".candidate_index",
            ".candidate_count",
            ".remaining_needed",
            ".discard_payable",
            ".sacrifice_payable",
            ".stage",
        )
        if any(marker in path for marker in scalar_global_markers) and not any(
            ref in path for ref in (".source.", ".object.", ".card_db_id", ".owner", ".controller", ".zone")
        ):
            return "FlatGlobalsV1", "model_input"
        return "FlatRelationV1", "model_input"
    return "FlatGlobalsV1", "model_input"


def _inventory() -> dict[str, Any]:
    registry = classification_registry()
    entries = []
    for path, classification in sorted(registry.items()):
        destination, disposition = _destination(path, classification)
        entries.append(
            {
                "path": path,
                "classification": classification,
                "disposition": disposition,
                "destination": destination,
            }
        )
    assert len(entries) == len({entry["path"] for entry in entries})
    assert all(entry["destination"] for entry in entries)
    assert all(
        (entry["destination"] == "absent") == (entry["classification"] == FORBIDDEN)
        for entry in entries
    )
    assert all(
        "FlatActionObjectV1" not in entry["destination"]
        for entry in entries
        if entry["classification"] == MODEL_INPUT
    )
    counts = {
        classification: sum(entry["classification"] == classification for entry in entries)
        for classification in (MODEL_INPUT, OPERATIONAL_ONLY, FORBIDDEN)
    }
    assert sum(counts.values()) == len(entries)
    return {
        "schema": "flat-policy-feature-inventory-v1",
        "feature_schema_version": FEATURE_SCHEMA_VERSION,
        "feature_registry_version": FEATURE_REGISTRY_VERSION,
        "encoding_contract_version": ENCODING_CONTRACT_VERSION,
        "feature_contract_digest": feature_contract_fingerprint(),
        "encoding_contract_digest": encoding_contract_fingerprint(),
        "authoritative_features_sha256": _sha256(ROOT / "python" / "mtg_kernel_rl" / "features.py"),
        "rust_typed_layout_sha256": _sha256(RUST_SOURCE),
        "counts": counts,
        "entries": entries,
    }


def _indexed(values: list[str]) -> dict[str, int]:
    return {value: index for index, value in enumerate(values)}


def _goldens(inventory: dict[str, Any]) -> dict[str, Any]:
    enum_maps = {
        "phase": _indexed(PHASES),
        "zone": _indexed(ZONES),
        "mana_color": _indexed(MANA_COLORS),
        "object_group": _indexed(OBJECT_GROUPS),
        "object_source_kind": _indexed(OBJECT_SOURCE_KINDS),
        "relation_role": _indexed(EDGE_ROLES),
        "action_kind": _indexed(ACTION_KINDS),
        "action_ref_role": _indexed(ACTION_REF_ROLES),
        "engine_stage": _indexed(ENGINE_STAGES),
        "surface_stage": _indexed(SURFACE_STAGES),
        "policy_surface_stage": _indexed(POLICY_SURFACE_STAGES),
        "stack_kind": _indexed(STACK_KINDS),
        "cast_method": {value: index + 1 for index, value in enumerate(CAST_METHODS)},
        "cast_mode": {value: index + 1 for index, value in enumerate(CAST_MODES)},
        "cost_kind": _indexed(COST_KINDS),
        "optional_cost_choice": _indexed(OPTIONAL_COST_CHOICES),
        "spell_copy_stage": _indexed(SPELL_COPY_STAGES),
        "target_kind": {"none": 0, **{value: index + 1 for index, value in enumerate(TARGET_KINDS)}},
        "target_purpose": _indexed(TARGET_SELECTION_PURPOSES),
        "boolean_purpose": _indexed(BOOLEAN_CHOICE_PURPOSES),
        "play_or_cast": _indexed(PLAY_OR_CAST),
        "expiry": _indexed(EXPIRY_KINDS),
    }
    fixture_cases = [
        {
            "name": "burn_seed_11_initial",
            "deck_ids": ["Burn", "Burn"],
            "seed": 11,
            "episode_id": 90001,
            "decision_index": 0,
            "counts": [7, 0, 0, 0, 0, 0, 0, 0, 3, 2, 2],
            "model_typed_debug_sha256": "5efd11f47b80fae409f90778dffca4d079a9e9a15cee8c6a71feab34cb3c1bb9",
            "action_objects_operational_debug_sha256": "caea06d6b76d7d8238d5fc4a426b81b10cef34afe2177821fcc4c7486725c9c7",
        },
        {
            "name": "rally_seed_23_initial",
            "deck_ids": ["Rally", "Rally"],
            "seed": 23,
            "episode_id": 90002,
            "decision_index": 0,
            "counts": [7, 0, 0, 0, 0, 0, 0, 0, 5, 4, 4],
            "model_typed_debug_sha256": "e640d6cfc88f288d537fffc512f7a1e485a917620bed440592eb865b7c91e76d",
            "action_objects_operational_debug_sha256": "56e018d1a0e72785d71fc253b172c141005cb2c2e1aae017976d7079b231c42c",
        },
        {
            "name": "burn_rally_seed_37_initial",
            "deck_ids": ["Burn", "Rally"],
            "seed": 37,
            "episode_id": 90003,
            "decision_index": 0,
            "counts": [7, 0, 0, 0, 0, 0, 0, 0, 4, 3, 3],
            "model_typed_debug_sha256": "d9ef784b79b942edac11f46d6a181b1ce48d872b95fe874fa009007b50ce70b2",
            "action_objects_operational_debug_sha256": "5ead8f61d3257c0d290675d820fe494e60d9745a63225907b37dd4a11d3c2963",
        },
        {
            "name": "burn_seed_81701_first_relation",
            "deck_ids": ["Burn", "Burn"],
            "seed": 81701,
            "episode_id": 90101,
            "decision_index": 6,
            "selection_policy": "splitmix64_mod_width_include_true_for_combat_v1",
            "counts": [12, 7, 5, 0, 0, 0, 0, 0, 6, 6, 6],
            "model_typed_debug_sha256": "3e9ccf68fdeb68b4f75ca25e4a6cfccdbe608f97418ae6cac2aeb7579ef680a1",
        },
        {
            "name": "rally_seed_81702_first_relation",
            "deck_ids": ["Rally", "Rally"],
            "seed": 81702,
            "episode_id": 90102,
            "decision_index": 4,
            "selection_policy": "splitmix64_mod_width_include_true_for_combat_v1",
            "counts": [9, 2, 1, 0, 0, 0, 0, 0, 2, 2, 1],
            "model_typed_debug_sha256": "d457fa1a69ab80e568e5fb43beacaff1618b82b2b5c996d9f5866612436e1284",
        },
    ]
    payload: dict[str, Any] = {
        "schema": "flat-policy-v1-independent-goldens-v1",
        "producer_parent_commit": "13644f30d33c7ab80f01c9d7c71fe59980f0c285",
        "feature_contract_digest": inventory["feature_contract_digest"],
        "encoding_contract_digest": inventory["encoding_contract_digest"],
        "inventory_sha256": hashlib.sha256(
            json.dumps(inventory, sort_keys=True, separators=(",", ":")).encode()
        ).hexdigest(),
        "card_catalog_sha256": _sha256(CARDS_PATH),
        "enum_maps": enum_maps,
        "hand_authored_vectors": {
            "relative_players": {"self": 0, "opponent": 1, "none": 2},
            "turn_relation": {"absent": 0, "this_turn": 1, "earlier_turn": 2},
            "optional_presence": [None, False, True],
            "trigger_order_lengths": list(range(8)),
            "empty_counts": {
                "objects": 0,
                "relations": 0,
                "subtypes": 0,
                "ability_uses": 0,
                "goads": 0,
                "completed_dungeons": 0,
                "effect_subtype_changes": 0,
                "context_path_elements": 0,
            },
        },
        "runtime_fixture_cases": fixture_cases,
    }
    payload["payload_sha256"] = hashlib.sha256(
        json.dumps(payload, sort_keys=True, separators=(",", ":")).encode()
    ).hexdigest()
    return payload


def _encoded(payload: dict[str, Any]) -> bytes:
    return (json.dumps(payload, indent=2, sort_keys=True, ensure_ascii=False) + "\n").encode()


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    inventory = _inventory()
    outputs = {
        INVENTORY_PATH: _encoded(inventory),
        GOLDENS_PATH: _encoded(_goldens(inventory)),
    }
    if args.check:
        stale = [str(path.relative_to(ROOT)) for path, data in outputs.items() if not path.exists() or path.read_bytes() != data]
        if stale:
            raise SystemExit("stale flat-policy-v1 generated files: " + ", ".join(stale))
        return 0
    for path, data in outputs.items():
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(data)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
