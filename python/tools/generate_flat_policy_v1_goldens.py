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

RUST_INTERNAL_ACTION_REF_ROLES = [
    "source",
    "candidate",
    "card",
    "attacker",
    "blocker",
    "target_object",
    "cards",
    "pending_sources",
]
ACTION_REF_ROLE_CROSSWALK_VERSION = 1


def _sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


# Deliberately literal: every normalized leaf emitted by classification_registry()
# appears below exactly once.  A named ``*_derivation`` destination means the
# leaf participates in constructing or validating typed topology rather than
# occupying a scalar field.  Do not replace these declarations with path-shape
# inference; missing, extra, duplicate, and reclassified leaves must fail closed.
def _destination_rows(
    classification: str, destination: str, paths: str
) -> tuple[tuple[str, str, str], ...]:
    return tuple(
        (path, classification, destination)
        for path in paths.splitlines()
        if path
    )


_DESTINATION_ROWS = (
    *_destination_rows(
        FORBIDDEN,
        'absent',
        """legal_action.display_text
legal_action.display_text.<present>
legal_action.stable_id
observation.known_hand_cards.[].[].card_name
observation.known_library_cards.[].[].card.card_name
observation.own_hand.[].card_name
observation.projection.battlefield.[].[].card_name
observation.projection.exile.[].card_name
observation.projection.graveyards.[].[].card_name
observation.visible_projection_hash""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatActionCoreV1.ability_index',
        """<variant:activate_ability>.legal_action.semantic.ability_index""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatActionCoreV1.cast_mode',
        """<variant:choose_cast_mode>.legal_action.semantic.mode""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatActionCoreV1.color',
        """<variant:choose_effect_color>.legal_action.semantic.color""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatActionCoreV1.cost_kind',
        """<variant:choose_cost_target>.legal_action.semantic.cost_kind""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatActionCoreV1.flags.cast_it',
        """<variant:choose_madness_cast>.legal_action.semantic.cast_it""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatActionCoreV1.flags.change_target',
        """<variant:choose_spell_copy_retarget>.legal_action.semantic.change_target""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatActionCoreV1.flags.include',
        """<variant:choose_attacker_inclusion>.legal_action.semantic.include
<variant:choose_blocker_inclusion>.legal_action.semantic.include""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatActionCoreV1.flags.pay',
        """<variant:choose_kicker>.legal_action.semantic.pay
<variant:choose_spell_copy_payment>.legal_action.semantic.pay""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatActionCoreV1.flags.use_cost',
        """<variant:choose_optional_cost_use>.legal_action.semantic.use_cost""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatActionCoreV1.flags.value',
        """<variant:choose_effect_boolean>.legal_action.semantic.value""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatActionCoreV1.kind',
        """<variant:activate_ability>.legal_action.semantic.action_kind
<variant:activate_mana_ability>.legal_action.semantic.action_kind
<variant:cast_spell>.legal_action.semantic.action_kind
<variant:choose_attacker_inclusion>.legal_action.semantic.action_kind
<variant:choose_blocker_inclusion>.legal_action.semantic.action_kind
<variant:choose_cast_mode>.legal_action.semantic.action_kind
<variant:choose_cost_target>.legal_action.semantic.action_kind
<variant:choose_effect_boolean>.legal_action.semantic.action_kind
<variant:choose_effect_color>.legal_action.semantic.action_kind
<variant:choose_effect_number>.legal_action.semantic.action_kind
<variant:choose_effect_option>.legal_action.semantic.action_kind
<variant:choose_effect_target>.legal_action.semantic.action_kind
<variant:choose_kicker>.legal_action.semantic.action_kind
<variant:choose_madness_cast>.legal_action.semantic.action_kind
<variant:choose_optional_cost_use>.legal_action.semantic.action_kind
<variant:choose_optional_cost_which>.legal_action.semantic.action_kind
<variant:choose_spell_copy_payment>.legal_action.semantic.action_kind
<variant:choose_spell_copy_retarget>.legal_action.semantic.action_kind
<variant:choose_spell_mode>.legal_action.semantic.action_kind
<variant:choose_target>.legal_action.semantic.action_kind
<variant:discard>.legal_action.semantic.action_kind
<variant:finish_effect_selection>.legal_action.semantic.action_kind
<variant:finish_target_selection>.legal_action.semantic.action_kind
<variant:order_triggers>.legal_action.semantic.action_kind
<variant:pass>.legal_action.semantic.action_kind
<variant:play_land>.legal_action.semantic.action_kind
<variant:plot_spell>.legal_action.semantic.action_kind
legal_action.semantic.action_kind""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatActionCoreV1.mana_choice',
        """<variant:activate_mana_ability>.legal_action.semantic.mana_choice
<variant:activate_mana_ability>.legal_action.semantic.mana_choice.<present>""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatActionCoreV1.max_targets',
        """<variant:choose_effect_target>.legal_action.semantic.max_targets""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatActionCoreV1.maximum',
        """<variant:choose_effect_number>.legal_action.semantic.maximum""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatActionCoreV1.min_targets',
        """<variant:choose_effect_target>.legal_action.semantic.min_targets""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatActionCoreV1.minimum',
        """<variant:choose_effect_number>.legal_action.semantic.minimum""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatActionCoreV1.mode_count',
        """<variant:choose_spell_mode>.legal_action.semantic.mode_count""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatActionCoreV1.mode_index',
        """<variant:choose_spell_mode>.legal_action.semantic.mode_index""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatActionCoreV1.number',
        """<variant:choose_effect_number>.legal_action.semantic.number""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatActionCoreV1.option_count',
        """<variant:choose_effect_option>.legal_action.semantic.option_count""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatActionCoreV1.option_index',
        """<variant:choose_effect_option>.legal_action.semantic.option_index""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatActionCoreV1.optional_cost_choice',
        """<variant:choose_optional_cost_which>.legal_action.semantic.choice""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatActionCoreV1.remaining',
        """<variant:choose_cost_target>.legal_action.semantic.remaining
<variant:choose_target>.legal_action.semantic.remaining""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatActionCoreV1.selected_count',
        """<variant:choose_effect_target>.legal_action.semantic.selected_count
<variant:finish_effect_selection>.legal_action.semantic.selected_count
<variant:finish_target_selection>.legal_action.semantic.selected_count""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatActionCoreV1.target_kind',
        """<variant:choose_effect_target>.<variant:object>.legal_action.semantic.target.target_kind
<variant:choose_effect_target>.<variant:player>.legal_action.semantic.target.target_kind
<variant:choose_effect_target>.legal_action.semantic.target.target_kind
<variant:choose_target>.<variant:object>.legal_action.semantic.target.target_kind
<variant:choose_target>.<variant:player>.legal_action.semantic.target.target_kind
<variant:choose_target>.legal_action.semantic.target.target_kind""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatActionCoreV1.target_player',
        """<variant:choose_effect_target>.<variant:player>.legal_action.semantic.target.player
<variant:choose_target>.<variant:player>.legal_action.semantic.target.player""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatActionRefV1.associated_order',
        """<variant:order_triggers>.legal_action.semantic.order.[]""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatActionRefV1.card_token',
        """<variant:activate_ability>.legal_action.semantic.source.card_db_id
<variant:activate_mana_ability>.legal_action.semantic.source.card_db_id
<variant:cast_spell>.legal_action.semantic.source.card_db_id
<variant:choose_attacker_inclusion>.legal_action.semantic.attacker.card_db_id
<variant:choose_blocker_inclusion>.legal_action.semantic.attacker.card_db_id
<variant:choose_blocker_inclusion>.legal_action.semantic.blocker.card_db_id
<variant:choose_cast_mode>.legal_action.semantic.source.card_db_id
<variant:choose_cost_target>.legal_action.semantic.candidate.card_db_id
<variant:choose_cost_target>.legal_action.semantic.source.card_db_id
<variant:choose_effect_boolean>.legal_action.semantic.source.card_db_id
<variant:choose_effect_color>.legal_action.semantic.source.card_db_id
<variant:choose_effect_number>.legal_action.semantic.source.card_db_id
<variant:choose_effect_option>.legal_action.semantic.source.card_db_id
<variant:choose_effect_target>.<variant:object>.legal_action.semantic.target.object.card_db_id
<variant:choose_effect_target>.legal_action.semantic.source.card_db_id
<variant:choose_kicker>.legal_action.semantic.source.card_db_id
<variant:choose_madness_cast>.legal_action.semantic.card.card_db_id
<variant:choose_spell_copy_payment>.legal_action.semantic.source.card_db_id
<variant:choose_spell_copy_retarget>.legal_action.semantic.source.card_db_id
<variant:choose_spell_mode>.legal_action.semantic.source.card_db_id
<variant:choose_target>.<variant:object>.legal_action.semantic.target.object.card_db_id
<variant:choose_target>.legal_action.semantic.source.card_db_id
<variant:discard>.legal_action.semantic.cards.[].card_db_id
<variant:finish_effect_selection>.legal_action.semantic.source.card_db_id
<variant:finish_target_selection>.legal_action.semantic.source.card_db_id
<variant:order_triggers>.legal_action.semantic.pending_sources.[].card_db_id
<variant:play_land>.legal_action.semantic.source.card_db_id
<variant:plot_spell>.legal_action.semantic.source.card_db_id""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatActionRefV1.object_index via FlatActionObjectV1.canonical_key',
        """<variant:activate_ability>.legal_action.semantic.source.controller
<variant:activate_ability>.legal_action.semantic.source.owner
<variant:activate_ability>.legal_action.semantic.source.zone
<variant:activate_mana_ability>.legal_action.semantic.source.controller
<variant:activate_mana_ability>.legal_action.semantic.source.owner
<variant:activate_mana_ability>.legal_action.semantic.source.zone
<variant:cast_spell>.legal_action.semantic.source.controller
<variant:cast_spell>.legal_action.semantic.source.owner
<variant:cast_spell>.legal_action.semantic.source.zone
<variant:choose_attacker_inclusion>.legal_action.semantic.attacker.controller
<variant:choose_attacker_inclusion>.legal_action.semantic.attacker.owner
<variant:choose_attacker_inclusion>.legal_action.semantic.attacker.zone
<variant:choose_blocker_inclusion>.legal_action.semantic.attacker.controller
<variant:choose_blocker_inclusion>.legal_action.semantic.attacker.owner
<variant:choose_blocker_inclusion>.legal_action.semantic.attacker.zone
<variant:choose_blocker_inclusion>.legal_action.semantic.blocker.controller
<variant:choose_blocker_inclusion>.legal_action.semantic.blocker.owner
<variant:choose_blocker_inclusion>.legal_action.semantic.blocker.zone
<variant:choose_cast_mode>.legal_action.semantic.source.controller
<variant:choose_cast_mode>.legal_action.semantic.source.owner
<variant:choose_cast_mode>.legal_action.semantic.source.zone
<variant:choose_cost_target>.legal_action.semantic.candidate.controller
<variant:choose_cost_target>.legal_action.semantic.candidate.owner
<variant:choose_cost_target>.legal_action.semantic.candidate.zone
<variant:choose_cost_target>.legal_action.semantic.source.controller
<variant:choose_cost_target>.legal_action.semantic.source.owner
<variant:choose_cost_target>.legal_action.semantic.source.zone
<variant:choose_effect_boolean>.legal_action.semantic.source.controller
<variant:choose_effect_boolean>.legal_action.semantic.source.owner
<variant:choose_effect_boolean>.legal_action.semantic.source.zone
<variant:choose_effect_color>.legal_action.semantic.source.controller
<variant:choose_effect_color>.legal_action.semantic.source.owner
<variant:choose_effect_color>.legal_action.semantic.source.zone
<variant:choose_effect_number>.legal_action.semantic.source.controller
<variant:choose_effect_number>.legal_action.semantic.source.owner
<variant:choose_effect_number>.legal_action.semantic.source.zone
<variant:choose_effect_option>.legal_action.semantic.source.controller
<variant:choose_effect_option>.legal_action.semantic.source.owner
<variant:choose_effect_option>.legal_action.semantic.source.zone
<variant:choose_effect_target>.<variant:object>.legal_action.semantic.target.object.controller
<variant:choose_effect_target>.<variant:object>.legal_action.semantic.target.object.owner
<variant:choose_effect_target>.<variant:object>.legal_action.semantic.target.object.zone
<variant:choose_effect_target>.legal_action.semantic.source.controller
<variant:choose_effect_target>.legal_action.semantic.source.owner
<variant:choose_effect_target>.legal_action.semantic.source.zone
<variant:choose_kicker>.legal_action.semantic.source.controller
<variant:choose_kicker>.legal_action.semantic.source.owner
<variant:choose_kicker>.legal_action.semantic.source.zone
<variant:choose_madness_cast>.legal_action.semantic.card.controller
<variant:choose_madness_cast>.legal_action.semantic.card.owner
<variant:choose_madness_cast>.legal_action.semantic.card.zone
<variant:choose_spell_copy_payment>.legal_action.semantic.source.controller
<variant:choose_spell_copy_payment>.legal_action.semantic.source.owner
<variant:choose_spell_copy_payment>.legal_action.semantic.source.zone
<variant:choose_spell_copy_retarget>.legal_action.semantic.source.controller
<variant:choose_spell_copy_retarget>.legal_action.semantic.source.owner
<variant:choose_spell_copy_retarget>.legal_action.semantic.source.zone
<variant:choose_spell_mode>.legal_action.semantic.source.controller
<variant:choose_spell_mode>.legal_action.semantic.source.owner
<variant:choose_spell_mode>.legal_action.semantic.source.zone
<variant:choose_target>.<variant:object>.legal_action.semantic.target.object.controller
<variant:choose_target>.<variant:object>.legal_action.semantic.target.object.owner
<variant:choose_target>.<variant:object>.legal_action.semantic.target.object.zone
<variant:choose_target>.legal_action.semantic.source.controller
<variant:choose_target>.legal_action.semantic.source.owner
<variant:choose_target>.legal_action.semantic.source.zone
<variant:discard>.legal_action.semantic.cards.[].controller
<variant:discard>.legal_action.semantic.cards.[].owner
<variant:discard>.legal_action.semantic.cards.[].zone
<variant:finish_effect_selection>.legal_action.semantic.source.controller
<variant:finish_effect_selection>.legal_action.semantic.source.owner
<variant:finish_effect_selection>.legal_action.semantic.source.zone
<variant:finish_target_selection>.legal_action.semantic.source.controller
<variant:finish_target_selection>.legal_action.semantic.source.owner
<variant:finish_target_selection>.legal_action.semantic.source.zone
<variant:order_triggers>.legal_action.semantic.pending_sources.[].controller
<variant:order_triggers>.legal_action.semantic.pending_sources.[].owner
<variant:order_triggers>.legal_action.semantic.pending_sources.[].zone
<variant:play_land>.legal_action.semantic.source.controller
<variant:play_land>.legal_action.semantic.source.owner
<variant:play_land>.legal_action.semantic.source.zone
<variant:plot_spell>.legal_action.semantic.source.controller
<variant:plot_spell>.legal_action.semantic.source.owner
<variant:plot_spell>.legal_action.semantic.source.zone""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatCompletedDungeonV1.dungeon_id',
        """observation.projection.player_status.[].dungeon.completed_dungeons.[]""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatContextPathElementV1.value(kind=LegalColor)',
        """<variant:color>.observation.projection.engine_context.pending_effect.choice.legal_colors.[]""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatContextPathElementV1.value(kind=StructuralPath)',
        """<variant:boolean>.observation.projection.engine_context.pending_effect.choice.structural_path.[]
<variant:color>.observation.projection.engine_context.pending_effect.choice.structural_path.[]
<variant:number>.observation.projection.engine_context.pending_effect.choice.structural_path.[]
<variant:options>.observation.projection.engine_context.pending_effect.choice.structural_path.[]
<variant:targets>.observation.projection.engine_context.pending_effect.choice.structural_path.[]""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatContextRelationDataV1.controller',
        """observation.projection.engine_context.pending_triggers.[].controller""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatContextRelationDataV1.kicked',
        """observation.projection.engine_context.pending_triggers.[].kicked""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatContextRelationDataV1.target_kind',
        """<variant:targets>.<variant:object>.observation.projection.engine_context.pending_effect.choice.legal_targets.[].target_kind
<variant:targets>.<variant:object>.observation.projection.engine_context.pending_effect.choice.selected_targets.[].target_kind
<variant:targets>.<variant:player>.observation.projection.engine_context.pending_effect.choice.legal_targets.[].target_kind
<variant:targets>.<variant:player>.observation.projection.engine_context.pending_effect.choice.selected_targets.[].target_kind
<variant:targets>.observation.projection.engine_context.pending_effect.choice.legal_targets.[].target_kind
<variant:targets>.observation.projection.engine_context.pending_effect.choice.selected_targets.[].target_kind""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatContextRelationDataV1.target_player',
        """<variant:targets>.<variant:player>.observation.projection.engine_context.pending_effect.choice.legal_targets.[].player
<variant:targets>.<variant:player>.observation.projection.engine_context.pending_effect.choice.selected_targets.[].player""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatContextRelationDataV1.trigger_kind',
        """observation.projection.engine_context.pending_triggers.[].trigger_kind""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatEffectRelationDataV1.add_color_mask',
        """observation.projection.continuous_effects.[].add_color_mask""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatEffectRelationDataV1.add_keyword_mask',
        """observation.projection.continuous_effects.[].add_keyword_mask""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatEffectRelationDataV1.add_landwalk_mask',
        """observation.projection.continuous_effects.[].add_landwalk_mask""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatEffectRelationDataV1.affected_player',
        """observation.projection.continuous_effects.[].affected_players.[]""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatEffectRelationDataV1.controller',
        """observation.projection.continuous_effects.[].controller
observation.projection.continuous_effects.[].controller.<present>""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatEffectRelationDataV1.damage_cannot_be_prevented',
        """observation.projection.continuous_effects.[].damage_cannot_be_prevented""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatEffectRelationDataV1.duration',
        """observation.projection.continuous_effects.[].duration""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatEffectRelationDataV1.global',
        """observation.projection.continuous_effects.[].global""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatEffectRelationDataV1.grants_haste',
        """observation.projection.continuous_effects.[].grants_haste""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatEffectRelationDataV1.layers',
        """observation.projection.continuous_effects.[].layers""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatEffectRelationDataV1.minimum_blockers',
        """observation.projection.continuous_effects.[].minimum_blockers
observation.projection.continuous_effects.[].minimum_blockers.<present>""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatEffectRelationDataV1.power_delta',
        """observation.projection.continuous_effects.[].power_delta""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatEffectRelationDataV1.prevent_damage_from_color_mask',
        """observation.projection.continuous_effects.[].prevent_damage_from_color_mask""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatEffectRelationDataV1.remove_color_mask',
        """observation.projection.continuous_effects.[].remove_color_mask""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatEffectRelationDataV1.remove_keyword_mask',
        """observation.projection.continuous_effects.[].remove_keyword_mask""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatEffectRelationDataV1.remove_landwalk_mask',
        """observation.projection.continuous_effects.[].remove_landwalk_mask""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatEffectRelationDataV1.set_power',
        """observation.projection.continuous_effects.[].set_power
observation.projection.continuous_effects.[].set_power.<present>""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatEffectRelationDataV1.set_toughness',
        """observation.projection.continuous_effects.[].set_toughness
observation.projection.continuous_effects.[].set_toughness.<present>""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatEffectRelationDataV1.toughness_delta',
        """observation.projection.continuous_effects.[].toughness_delta""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatEffectRelationDataV1.ward_generic_delta',
        """observation.projection.continuous_effects.[].ward_generic_delta""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatEffectSubtypeChangeV1.subtype_id(kind=Add)',
        """observation.projection.continuous_effects.[].add_subtype_ids.[]""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatEffectSubtypeChangeV1.subtype_id(kind=Remove)',
        """observation.projection.continuous_effects.[].remove_subtype_ids.[]""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatEngineGlobalsV1.current_stage',
        """observation.projection.engine_context.current_stage""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatEngineGlobalsV1.last_mana_ability_activator',
        """observation.projection.engine_context.last_mana_ability_activator_since_priority_boundary
observation.projection.engine_context.last_mana_ability_activator_since_priority_boundary.<present>""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatEngineGlobalsV1.mana_activity_since_priority_boundary',
        """observation.projection.engine_context.mana_activity_since_priority_boundary""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatEngineGlobalsV1.pending_activation',
        """observation.projection.engine_context.pending_activation.<present>""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatEngineGlobalsV1.pending_cast',
        """observation.projection.engine_context.pending_cast.<present>""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatEngineGlobalsV1.pending_discard',
        """observation.projection.engine_context.pending_discard.<present>""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatEngineGlobalsV1.pending_effect',
        """observation.projection.engine_context.pending_effect.<present>""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatEngineGlobalsV1.pending_optional_cost',
        """observation.projection.engine_context.pending_optional_cost.<present>""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatEngineGlobalsV1.pending_optional_sacrifice',
        """observation.projection.engine_context.pending_optional_cost_sacrifice.<present>""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatEngineGlobalsV1.pending_spell_copy',
        """observation.projection.engine_context.pending_spell_copy.<present>""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatEngineGlobalsV1.priority_passes[]',
        """observation.projection.engine_context.priority_passes.[]""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatEngineGlobalsV1.stack_activity_since_priority_boundary',
        """observation.projection.engine_context.stack_activity_since_priority_boundary""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatEngineGlobalsV1.stack_nonempty',
        """observation.projection.engine_context.stack_nonempty""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatGlobalsV1.active_player',
        """observation.projection.active_player""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatGlobalsV1.attackers_declared',
        """observation.projection.combat.attackers_declared""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatGlobalsV1.blockers_declared',
        """observation.projection.combat.blockers_declared""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatGlobalsV1.initiative',
        """observation.projection.initiative
observation.projection.initiative.<present>""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatGlobalsV1.phase',
        """observation.projection.phase""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatGlobalsV1.players[].hand_count',
        """observation.projection.hand_counts.[]""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatGlobalsV1.players[].library_count',
        """observation.projection.library_counts.[]""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatGlobalsV1.players[].life',
        """observation.projection.life_totals.[]""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatGlobalsV1.players[].mana[]',
        """observation.projection.mana_pools.[].[]""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatGlobalsV1.priority_player',
        """observation.projection.priority_player""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatObjectAbilityUseV1.ability_index',
        """observation.projection.battlefield.[].[].ability_uses_this_turn.[].ability_index
observation.projection.exile.[].ability_uses_this_turn.[].ability_index
observation.projection.graveyards.[].[].ability_uses_this_turn.[].ability_index""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatObjectAbilityUseV1.ability_kind',
        """observation.projection.battlefield.[].[].ability_uses_this_turn.[].ability_kind
observation.projection.exile.[].ability_uses_this_turn.[].ability_kind
observation.projection.graveyards.[].[].ability_uses_this_turn.[].ability_kind""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatObjectAbilityUseV1.uses',
        """observation.projection.battlefield.[].[].ability_uses_this_turn.[].uses
observation.projection.exile.[].ability_uses_this_turn.[].uses
observation.projection.graveyards.[].[].ability_uses_this_turn.[].uses""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatObjectCoreV1.base_power',
        """observation.projection.battlefield.[].[].characteristics.base_power
observation.projection.battlefield.[].[].characteristics.base_power.<present>
observation.projection.exile.[].characteristics.base_power
observation.projection.exile.[].characteristics.base_power.<present>
observation.projection.graveyards.[].[].characteristics.base_power
observation.projection.graveyards.[].[].characteristics.base_power.<present>""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatObjectCoreV1.base_toughness',
        """observation.projection.battlefield.[].[].characteristics.base_toughness
observation.projection.battlefield.[].[].characteristics.base_toughness.<present>
observation.projection.exile.[].characteristics.base_toughness
observation.projection.exile.[].characteristics.base_toughness.<present>
observation.projection.graveyards.[].[].characteristics.base_toughness
observation.projection.graveyards.[].[].characteristics.base_toughness.<present>""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatObjectCoreV1.card_token',
        """observation.known_hand_cards.[].[].stable.card_db_id
observation.known_library_cards.[].[].card.stable.card_db_id
observation.own_hand.[].stable.card_db_id
observation.projection.battlefield.[].[].stable.card_db_id
observation.projection.exile.[].stable.card_db_id
observation.projection.graveyards.[].[].stable.card_db_id
observation.projection.stack.[].source.card_db_id""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatObjectCoreV1.chosen_color',
        """observation.projection.battlefield.[].[].chosen_color
observation.projection.battlefield.[].[].chosen_color.<present>
observation.projection.exile.[].chosen_color
observation.projection.exile.[].chosen_color.<present>
observation.projection.graveyards.[].[].chosen_color
observation.projection.graveyards.[].[].chosen_color.<present>""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatObjectCoreV1.controller',
        """observation.known_hand_cards.[].[].stable.controller
observation.known_library_cards.[].[].card.stable.controller
observation.own_hand.[].stable.controller
observation.projection.battlefield.[].[].stable.controller
observation.projection.exile.[].stable.controller
observation.projection.graveyards.[].[].stable.controller
observation.projection.stack.[].source.controller""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatObjectCoreV1.counters[lore]',
        """observation.projection.battlefield.[].[].counters.lore
observation.projection.exile.[].counters.lore
observation.projection.graveyards.[].[].counters.lore""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatObjectCoreV1.counters[minus0_minus1]',
        """observation.projection.battlefield.[].[].counters.minus0_minus1
observation.projection.exile.[].counters.minus0_minus1
observation.projection.graveyards.[].[].counters.minus0_minus1""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatObjectCoreV1.counters[minus1_minus1]',
        """observation.projection.battlefield.[].[].counters.minus1_minus1
observation.projection.exile.[].counters.minus1_minus1
observation.projection.graveyards.[].[].counters.minus1_minus1""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatObjectCoreV1.counters[plus1_plus1]',
        """observation.projection.battlefield.[].[].counters.plus1_plus1
observation.projection.exile.[].counters.plus1_plus1
observation.projection.graveyards.[].[].counters.plus1_plus1""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatObjectCoreV1.counters[stun]',
        """observation.projection.battlefield.[].[].counters.stun
observation.projection.exile.[].counters.stun
observation.projection.graveyards.[].[].counters.stun""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatObjectCoreV1.damage',
        """observation.projection.battlefield.[].[].damage
observation.projection.exile.[].damage
observation.projection.graveyards.[].[].damage""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatObjectCoreV1.effective_color_mask',
        """observation.projection.battlefield.[].[].characteristics.effective_color_mask
observation.projection.exile.[].characteristics.effective_color_mask
observation.projection.graveyards.[].[].characteristics.effective_color_mask""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatObjectCoreV1.effective_power',
        """observation.projection.battlefield.[].[].characteristics.effective_power
observation.projection.battlefield.[].[].characteristics.effective_power.<present>
observation.projection.exile.[].characteristics.effective_power
observation.projection.exile.[].characteristics.effective_power.<present>
observation.projection.graveyards.[].[].characteristics.effective_power
observation.projection.graveyards.[].[].characteristics.effective_power.<present>""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatObjectCoreV1.effective_toughness',
        """observation.projection.battlefield.[].[].characteristics.effective_toughness
observation.projection.battlefield.[].[].characteristics.effective_toughness.<present>
observation.projection.exile.[].characteristics.effective_toughness
observation.projection.exile.[].characteristics.effective_toughness.<present>
observation.projection.graveyards.[].[].characteristics.effective_toughness
observation.projection.graveyards.[].[].characteristics.effective_toughness.<present>""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatObjectCoreV1.entered_battlefield_turn',
        """observation.projection.battlefield.[].[].entered_battlefield_turn
observation.projection.battlefield.[].[].entered_battlefield_turn.<present>
observation.projection.exile.[].entered_battlefield_turn
observation.projection.exile.[].entered_battlefield_turn.<present>
observation.projection.graveyards.[].[].entered_battlefield_turn
observation.projection.graveyards.[].[].entered_battlefield_turn.<present>""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatObjectCoreV1.face_index',
        """observation.projection.battlefield.[].[].face_index
observation.projection.exile.[].face_index
observation.projection.graveyards.[].[].face_index""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatObjectCoreV1.is_token',
        """observation.projection.battlefield.[].[].is_token
observation.projection.exile.[].is_token
observation.projection.graveyards.[].[].is_token""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatObjectCoreV1.keyword_flags[deathtouch]',
        """observation.projection.battlefield.[].[].characteristics.effective_keywords.deathtouch
observation.projection.exile.[].characteristics.effective_keywords.deathtouch
observation.projection.graveyards.[].[].characteristics.effective_keywords.deathtouch""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatObjectCoreV1.keyword_flags[defender]',
        """observation.projection.battlefield.[].[].characteristics.effective_keywords.defender
observation.projection.exile.[].characteristics.effective_keywords.defender
observation.projection.graveyards.[].[].characteristics.effective_keywords.defender""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatObjectCoreV1.keyword_flags[double_strike]',
        """observation.projection.battlefield.[].[].characteristics.effective_keywords.double_strike
observation.projection.exile.[].characteristics.effective_keywords.double_strike
observation.projection.graveyards.[].[].characteristics.effective_keywords.double_strike""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatObjectCoreV1.keyword_flags[first_strike]',
        """observation.projection.battlefield.[].[].characteristics.effective_keywords.first_strike
observation.projection.exile.[].characteristics.effective_keywords.first_strike
observation.projection.graveyards.[].[].characteristics.effective_keywords.first_strike""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatObjectCoreV1.keyword_flags[flying]',
        """observation.projection.battlefield.[].[].characteristics.effective_keywords.flying
observation.projection.exile.[].characteristics.effective_keywords.flying
observation.projection.graveyards.[].[].characteristics.effective_keywords.flying""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatObjectCoreV1.keyword_flags[haste]',
        """observation.projection.battlefield.[].[].characteristics.effective_keywords.haste
observation.projection.exile.[].characteristics.effective_keywords.haste
observation.projection.graveyards.[].[].characteristics.effective_keywords.haste""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatObjectCoreV1.keyword_flags[hexproof]',
        """observation.projection.battlefield.[].[].characteristics.effective_keywords.hexproof
observation.projection.exile.[].characteristics.effective_keywords.hexproof
observation.projection.graveyards.[].[].characteristics.effective_keywords.hexproof""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatObjectCoreV1.keyword_flags[indestructible]',
        """observation.projection.battlefield.[].[].characteristics.effective_keywords.indestructible
observation.projection.exile.[].characteristics.effective_keywords.indestructible
observation.projection.graveyards.[].[].characteristics.effective_keywords.indestructible""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatObjectCoreV1.keyword_flags[lifelink]',
        """observation.projection.battlefield.[].[].characteristics.effective_keywords.lifelink
observation.projection.exile.[].characteristics.effective_keywords.lifelink
observation.projection.graveyards.[].[].characteristics.effective_keywords.lifelink""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatObjectCoreV1.keyword_flags[menace]',
        """observation.projection.battlefield.[].[].characteristics.effective_keywords.menace
observation.projection.exile.[].characteristics.effective_keywords.menace
observation.projection.graveyards.[].[].characteristics.effective_keywords.menace""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatObjectCoreV1.keyword_flags[protection_from_monocolored]',
        """observation.projection.battlefield.[].[].characteristics.effective_keywords.protection_from_monocolored
observation.projection.exile.[].characteristics.effective_keywords.protection_from_monocolored
observation.projection.graveyards.[].[].characteristics.effective_keywords.protection_from_monocolored""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatObjectCoreV1.keyword_flags[reach]',
        """observation.projection.battlefield.[].[].characteristics.effective_keywords.reach
observation.projection.exile.[].characteristics.effective_keywords.reach
observation.projection.graveyards.[].[].characteristics.effective_keywords.reach""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatObjectCoreV1.keyword_flags[trample]',
        """observation.projection.battlefield.[].[].characteristics.effective_keywords.trample
observation.projection.exile.[].characteristics.effective_keywords.trample
observation.projection.graveyards.[].[].characteristics.effective_keywords.trample""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatObjectCoreV1.keyword_flags[vigilance]',
        """observation.projection.battlefield.[].[].characteristics.effective_keywords.vigilance
observation.projection.exile.[].characteristics.effective_keywords.vigilance
observation.projection.graveyards.[].[].characteristics.effective_keywords.vigilance""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatObjectCoreV1.landwalk_mask',
        """observation.projection.battlefield.[].[].characteristics.effective_keywords.landwalk_mask
observation.projection.exile.[].characteristics.effective_keywords.landwalk_mask
observation.projection.graveyards.[].[].characteristics.effective_keywords.landwalk_mask""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatObjectCoreV1.minimum_blockers',
        """observation.projection.battlefield.[].[].characteristics.effective_keywords.minimum_blockers
observation.projection.exile.[].characteristics.effective_keywords.minimum_blockers
observation.projection.graveyards.[].[].characteristics.effective_keywords.minimum_blockers""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatObjectCoreV1.owner',
        """observation.known_hand_cards.[].[].stable.owner
observation.known_library_cards.[].[].card.stable.owner
observation.own_hand.[].stable.owner
observation.projection.battlefield.[].[].stable.owner
observation.projection.exile.[].stable.owner
observation.projection.graveyards.[].[].stable.owner
observation.projection.stack.[].source.owner""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatObjectCoreV1.plotted_turn',
        """observation.projection.battlefield.[].[].plotted_turn
observation.projection.battlefield.[].[].plotted_turn.<present>
observation.projection.exile.[].plotted_turn
observation.projection.exile.[].plotted_turn.<present>
observation.projection.graveyards.[].[].plotted_turn
observation.projection.graveyards.[].[].plotted_turn.<present>""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatObjectCoreV1.skip_next_untap',
        """observation.projection.battlefield.[].[].skip_next_untap
observation.projection.exile.[].skip_next_untap
observation.projection.graveyards.[].[].skip_next_untap""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatObjectCoreV1.summoning_sick',
        """observation.projection.battlefield.[].[].summoning_sick
observation.projection.exile.[].summoning_sick
observation.projection.graveyards.[].[].summoning_sick""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatObjectCoreV1.tapped',
        """observation.projection.battlefield.[].[].tapped
observation.projection.exile.[].tapped
observation.projection.graveyards.[].[].tapped""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatObjectCoreV1.type_flags[artifact]',
        """observation.projection.battlefield.[].[].characteristics.type_flags.artifact
observation.projection.exile.[].characteristics.type_flags.artifact
observation.projection.graveyards.[].[].characteristics.type_flags.artifact""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatObjectCoreV1.type_flags[creature]',
        """observation.projection.battlefield.[].[].characteristics.type_flags.creature
observation.projection.exile.[].characteristics.type_flags.creature
observation.projection.graveyards.[].[].characteristics.type_flags.creature""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatObjectCoreV1.type_flags[enchantment]',
        """observation.projection.battlefield.[].[].characteristics.type_flags.enchantment
observation.projection.exile.[].characteristics.type_flags.enchantment
observation.projection.graveyards.[].[].characteristics.type_flags.enchantment""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatObjectCoreV1.type_flags[instant]',
        """observation.projection.battlefield.[].[].characteristics.type_flags.instant
observation.projection.exile.[].characteristics.type_flags.instant
observation.projection.graveyards.[].[].characteristics.type_flags.instant""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatObjectCoreV1.type_flags[land]',
        """observation.projection.battlefield.[].[].characteristics.type_flags.land
observation.projection.exile.[].characteristics.type_flags.land
observation.projection.graveyards.[].[].characteristics.type_flags.land""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatObjectCoreV1.type_flags[sorcery]',
        """observation.projection.battlefield.[].[].characteristics.type_flags.sorcery
observation.projection.exile.[].characteristics.type_flags.sorcery
observation.projection.graveyards.[].[].characteristics.type_flags.sorcery""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatObjectCoreV1.ward_generic',
        """observation.projection.battlefield.[].[].characteristics.effective_keywords.ward_generic
observation.projection.exile.[].characteristics.effective_keywords.ward_generic
observation.projection.graveyards.[].[].characteristics.effective_keywords.ward_generic""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatObjectCoreV1.zone',
        """observation.known_hand_cards.[].[].stable.zone
observation.known_library_cards.[].[].card.stable.zone
observation.own_hand.[].stable.zone
observation.projection.battlefield.[].[].stable.zone
observation.projection.exile.[].stable.zone
observation.projection.graveyards.[].[].stable.zone
observation.projection.stack.[].source.zone""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatObjectGoadV1.expires_after_turns',
        """observation.projection.battlefield.[].[].goaded_by.[].expires_at_turn
observation.projection.exile.[].goaded_by.[].expires_at_turn
observation.projection.graveyards.[].[].goaded_by.[].expires_at_turn""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatObjectGoadV1.player',
        """observation.projection.battlefield.[].[].goaded_by.[].player
observation.projection.exile.[].goaded_by.[].player
observation.projection.graveyards.[].[].goaded_by.[].player""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatObjectSubtypeV1.subtype_id',
        """observation.projection.battlefield.[].[].characteristics.effective_subtype_ids.[]
observation.projection.exile.[].characteristics.effective_subtype_ids.[]
observation.projection.graveyards.[].[].characteristics.effective_subtype_ids.[]""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatPendingActivationGlobalsV1.ability_index',
        """observation.projection.engine_context.pending_activation.ability_index""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatPendingActivationGlobalsV1.controller',
        """observation.projection.engine_context.pending_activation.controller""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatPendingActivationGlobalsV1.discard_paid_present',
        """observation.projection.engine_context.pending_activation.cost_discard_paid.<present>""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatPendingActivationGlobalsV1.source_present',
        """observation.projection.engine_context.pending_activation.source.<present>""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatPendingCastGlobalsV1.cast_mode',
        """observation.projection.engine_context.pending_cast.cast_mode
observation.projection.engine_context.pending_cast.cast_mode.<present>""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatPendingCastGlobalsV1.controller',
        """observation.projection.engine_context.pending_cast.controller""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatPendingCastGlobalsV1.discarded_present',
        """observation.projection.engine_context.pending_cast.additional_cost_discarded.<present>""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatPendingCastGlobalsV1.is_flashback',
        """observation.projection.engine_context.pending_cast.is_flashback""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatPendingCastGlobalsV1.kicked',
        """observation.projection.engine_context.pending_cast.kicked
observation.projection.engine_context.pending_cast.kicked.<present>""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatPendingCastGlobalsV1.mode_chosen',
        """observation.projection.engine_context.pending_cast.mode_chosen
observation.projection.engine_context.pending_cast.mode_chosen.<present>""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatPendingCastGlobalsV1.origin_zone',
        """observation.projection.engine_context.pending_cast.origin_zone""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatPendingCastGlobalsV1.source_present',
        """observation.projection.engine_context.pending_cast.source.<present>""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatPendingDiscardGlobalsV1.count',
        """observation.projection.engine_context.pending_discard.count""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatPendingDiscardGlobalsV1.player',
        """observation.projection.engine_context.pending_discard.player""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatPendingDiscardGlobalsV1.resume_source_present',
        """observation.projection.engine_context.pending_discard.resume_source.<present>""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatPendingDiscardGlobalsV1.resume_stage',
        """observation.projection.engine_context.pending_discard.resume_stage""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatPendingEffectChoiceV1::Boolean.purpose',
        """<variant:boolean>.observation.projection.engine_context.pending_effect.choice.purpose""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatPendingEffectChoiceV1::Boolean.player',
        """<variant:boolean>.observation.projection.engine_context.pending_effect.choice.player""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatPendingEffectChoiceV1::Color.player',
        """<variant:color>.observation.projection.engine_context.pending_effect.choice.player""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatPendingEffectChoiceV1::Number.player',
        """<variant:number>.observation.projection.engine_context.pending_effect.choice.player""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatPendingEffectChoiceV1::Options.player',
        """<variant:options>.observation.projection.engine_context.pending_effect.choice.player""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatPendingEffectChoiceV1::Targets.player',
        """<variant:targets>.observation.projection.engine_context.pending_effect.choice.player""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatPendingEffectChoiceV1::Targets.purpose',
        """<variant:targets>.observation.projection.engine_context.pending_effect.choice.purpose""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatPendingEffectChoiceV1::Boolean.default',
        """<variant:boolean>.observation.projection.engine_context.pending_effect.choice.default
<variant:boolean>.observation.projection.engine_context.pending_effect.choice.default.<present>""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatPendingEffectChoiceV1::Number.maximum',
        """<variant:number>.observation.projection.engine_context.pending_effect.choice.maximum""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatPendingEffectChoiceV1::Number.minimum',
        """<variant:number>.observation.projection.engine_context.pending_effect.choice.minimum""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatPendingEffectChoiceV1::Options.option_count',
        """<variant:options>.observation.projection.engine_context.pending_effect.choice.option_count""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatPendingEffectChoiceV1::Targets.can_finish',
        """<variant:targets>.observation.projection.engine_context.pending_effect.choice.can_finish""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatPendingEffectChoiceV1::Targets.max_targets',
        """<variant:targets>.observation.projection.engine_context.pending_effect.choice.max_targets""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatPendingEffectChoiceV1::Targets.min_targets',
        """<variant:targets>.observation.projection.engine_context.pending_effect.choice.min_targets""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatPendingEffectChoiceV1::Targets.ordered',
        """<variant:targets>.observation.projection.engine_context.pending_effect.choice.ordered""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatPendingEffectGlobalsV1.choice',
        """observation.projection.engine_context.pending_effect.choice.<present>""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatPendingEffectGlobalsV1.choice discriminant',
        """<variant:boolean>.observation.projection.engine_context.pending_effect.choice.choice_kind
<variant:color>.observation.projection.engine_context.pending_effect.choice.choice_kind
<variant:number>.observation.projection.engine_context.pending_effect.choice.choice_kind
<variant:options>.observation.projection.engine_context.pending_effect.choice.choice_kind
<variant:targets>.observation.projection.engine_context.pending_effect.choice.choice_kind
observation.projection.engine_context.pending_effect.choice.choice_kind""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatPendingEffectGlobalsV1.controller',
        """observation.projection.engine_context.pending_effect.controller""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatPendingEffectGlobalsV1.source_present',
        """observation.projection.engine_context.pending_effect.source.<present>""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatPendingOptionalCostGlobalsV1.discard_cards',
        """observation.projection.engine_context.pending_optional_cost.discard_cards""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatPendingOptionalCostGlobalsV1.discard_payable',
        """observation.projection.engine_context.pending_optional_cost.discard_payable""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatPendingOptionalCostGlobalsV1.player',
        """observation.projection.engine_context.pending_optional_cost.player""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatPendingOptionalCostGlobalsV1.sacrifice_lands',
        """observation.projection.engine_context.pending_optional_cost.sacrifice_lands""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatPendingOptionalCostGlobalsV1.sacrifice_payable',
        """observation.projection.engine_context.pending_optional_cost.sacrifice_payable""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatPendingOptionalCostGlobalsV1.source_present',
        """observation.projection.engine_context.pending_optional_cost.source.<present>""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatPendingOptionalCostGlobalsV1.spell_resume_source_present',
        """observation.projection.engine_context.pending_optional_cost.spell_resume_source.<present>""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatPendingOptionalCostGlobalsV1.spell_resume_zone',
        """observation.projection.engine_context.pending_optional_cost.spell_resume_zone
observation.projection.engine_context.pending_optional_cost.spell_resume_zone.<present>""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatPendingOptionalSacrificeGlobalsV1.player',
        """observation.projection.engine_context.pending_optional_cost_sacrifice.player""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatPendingOptionalSacrificeGlobalsV1.remaining',
        """observation.projection.engine_context.pending_optional_cost_sacrifice.remaining""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatPendingOptionalSacrificeGlobalsV1.source_present',
        """observation.projection.engine_context.pending_optional_cost_sacrifice.source.<present>""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatPendingOptionalSacrificeGlobalsV1.spell_resume_source_present',
        """observation.projection.engine_context.pending_optional_cost_sacrifice.spell_resume_source.<present>""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatPendingOptionalSacrificeGlobalsV1.spell_resume_zone',
        """observation.projection.engine_context.pending_optional_cost_sacrifice.spell_resume_zone
observation.projection.engine_context.pending_optional_cost_sacrifice.spell_resume_zone.<present>""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatPendingSpellCopyGlobalsV1.copy_present',
        """observation.projection.engine_context.pending_spell_copy.copy.<present>""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatPendingSpellCopyGlobalsV1.inherited_target_kind',
        """<variant:object>.observation.projection.engine_context.pending_spell_copy.inherited_target.target_kind
<variant:player>.observation.projection.engine_context.pending_spell_copy.inherited_target.target_kind
observation.projection.engine_context.pending_spell_copy.inherited_target.target_kind""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatPendingSpellCopyGlobalsV1.inherited_target_player',
        """<variant:player>.observation.projection.engine_context.pending_spell_copy.inherited_target.player""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatPendingSpellCopyGlobalsV1.parent_present',
        """observation.projection.engine_context.pending_spell_copy.parent.<present>""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatPendingSpellCopyGlobalsV1.player',
        """observation.projection.engine_context.pending_spell_copy.player""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatPendingSpellCopyGlobalsV1.stage',
        """observation.projection.engine_context.pending_spell_copy.stage""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatPermissionRelationDataV1.expiry',
        """<variant:end_of_turn>.observation.projection.exile_play_permissions.[].expiry.expiry_kind
<variant:until_holders_next_turn>.observation.projection.exile_play_permissions.[].expiry.expiry_kind
observation.projection.exile_play_permissions.[].expiry.expiry_kind""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatPermissionRelationDataV1.holder',
        """observation.projection.exile_play_permissions.[].holder""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatPermissionRelationDataV1.holder_turn_started',
        """<variant:until_holders_next_turn>.observation.projection.exile_play_permissions.[].expiry.holder_turn_started""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatPermissionRelationDataV1.play_or_cast',
        """observation.projection.exile_play_permissions.[].play_or_cast""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatPlayerGlobalsV1.draws_this_turn',
        """observation.projection.player_status.[].draws_this_turn""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatPlayerGlobalsV1.drew_from_empty',
        """observation.projection.player_status.[].drew_from_empty""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatPlayerGlobalsV1.dungeon_id',
        """observation.projection.player_status.[].dungeon.dungeon_id
observation.projection.player_status.[].dungeon.dungeon_id.<present>""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatPlayerGlobalsV1.has_lost',
        """observation.projection.player_status.[].has_lost""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatPlayerGlobalsV1.lands_played_this_turn',
        """observation.projection.player_status.[].lands_played_this_turn""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatPlayerGlobalsV1.room_id',
        """observation.projection.player_status.[].dungeon.room_id
observation.projection.player_status.[].dungeon.room_id.<present>""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatPlayerGlobalsV1.spells_cast_this_turn',
        """observation.projection.player_status.[].spells_cast_this_turn""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatPolicySurfaceGlobalsV1.candidate_count',
        """observation.projection.policy_surface_context.private_combat_selection.candidate_count""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatPolicySurfaceGlobalsV1.candidate_index',
        """observation.projection.policy_surface_context.private_combat_selection.candidate_index""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatPolicySurfaceGlobalsV1.current_stage',
        """observation.projection.policy_surface_context.current_stage""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatPolicySurfaceGlobalsV1.private_combat_attacker_present',
        """observation.projection.policy_surface_context.private_combat_selection.attacker.<present>""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatPolicySurfaceGlobalsV1.private_combat_present',
        """observation.projection.policy_surface_context.private_combat_selection.<present>""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatRelationV1.role(object_relation_kind)',
        """<variant:attached_to>.observation.projection.object_relations.[].relation_kind
<variant:exiled_by>.observation.projection.object_relations.[].relation_kind
observation.projection.object_relations.[].relation_kind""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatStackRelationDataV1.cast_method',
        """observation.projection.stack.[].cast_method
observation.projection.stack.[].cast_method.<present>""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatStackRelationDataV1.controller',
        """observation.projection.stack.[].controller""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatStackRelationDataV1.face_index',
        """observation.projection.stack.[].face_index""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatStackRelationDataV1.is_copy',
        """observation.projection.stack.[].is_copy""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatStackRelationDataV1.is_flashback',
        """observation.projection.stack.[].is_flashback""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatStackRelationDataV1.kicked',
        """observation.projection.stack.[].kicked""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatStackRelationDataV1.madness_offer',
        """observation.projection.stack.[].madness_offer""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatStackRelationDataV1.mode_chosen',
        """observation.projection.stack.[].mode_chosen""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatStackRelationDataV1.stack_item_kind',
        """observation.projection.stack.[].stack_item_kind""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatStackRelationDataV1.target_kind',
        """<variant:object>.observation.projection.stack.[].targets.[].target_kind
<variant:player>.observation.projection.stack.[].targets.[].target_kind
observation.projection.stack.[].targets.[].target_kind""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatStackRelationDataV1.target_object_controller',
        """<variant:object>.observation.projection.stack.[].targets.[].object.controller""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatStackRelationDataV1.target_player',
        """<variant:player>.observation.projection.stack.[].targets.[].player""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatStackRelationDataV1.x_value',
        """observation.projection.stack.[].x_value""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatSurfaceGlobalsV1.combat_priority_rearmed_by_mana_activity',
        """observation.projection.surface_context.combat_priority_rearmed_by_mana_activity""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatSurfaceGlobalsV1.combat_priority_rearmed_by_stack_activity',
        """observation.projection.surface_context.combat_priority_rearmed_by_stack_activity""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatSurfaceGlobalsV1.combat_priority_spent[]',
        """observation.projection.surface_context.combat_priority_spent.[]""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatSurfaceGlobalsV1.current_stage',
        """observation.projection.surface_context.current_stage""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatSurfaceGlobalsV1.madness_cast_reprompt_source_present',
        """observation.projection.surface_context.madness_cast_reprompt_source.<present>""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatSurfaceGlobalsV1.mana_activity_since_last_stack_change',
        """observation.projection.surface_context.mana_activity_since_last_stack_change""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatSurfaceGlobalsV1.mana_activity_since_round_open',
        """observation.projection.surface_context.mana_activity_since_round_open""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatSurfaceGlobalsV1.private_blockers_present',
        """observation.projection.surface_context.private_blockers.<present>""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatSurfaceGlobalsV1.private_discard_remaining_needed',
        """observation.projection.surface_context.private_discard.remaining_needed""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatSurfaceGlobalsV1.private_optional_discard_payable',
        """observation.projection.surface_context.private_optional_cost.discard_payable""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatSurfaceGlobalsV1.private_optional_sacrifice_payable',
        """observation.projection.surface_context.private_optional_cost.sacrifice_payable""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatSurfaceGlobalsV1.private_optional_stage',
        """observation.projection.surface_context.private_optional_cost.stage""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatSurfaceGlobalsV1.stack_grew_since_round_open',
        """observation.projection.surface_context.stack_grew_since_round_open""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'FlatSurfaceGlobalsV1.stack_length_changed_since_observed',
        """observation.projection.surface_context.stack_length_changed_since_observed
observation.projection.surface_context.stack_length_changed_since_observed.<present>""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'actor_relative_frame_to_FlatGlobalsV1.acting_player',
        """observation.acting_player""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'actor_relative_self_constant',
        """<variant:activate_ability>.legal_action.semantic.actor
<variant:activate_mana_ability>.legal_action.semantic.actor
<variant:cast_spell>.legal_action.semantic.actor
<variant:choose_attacker_inclusion>.legal_action.semantic.actor
<variant:choose_blocker_inclusion>.legal_action.semantic.actor
<variant:choose_cast_mode>.legal_action.semantic.actor
<variant:choose_cost_target>.legal_action.semantic.actor
<variant:choose_effect_boolean>.legal_action.semantic.actor
<variant:choose_effect_color>.legal_action.semantic.actor
<variant:choose_effect_number>.legal_action.semantic.actor
<variant:choose_effect_option>.legal_action.semantic.actor
<variant:choose_effect_target>.legal_action.semantic.actor
<variant:choose_kicker>.legal_action.semantic.actor
<variant:choose_madness_cast>.legal_action.semantic.actor
<variant:choose_optional_cost_use>.legal_action.semantic.actor
<variant:choose_optional_cost_which>.legal_action.semantic.actor
<variant:choose_spell_copy_payment>.legal_action.semantic.actor
<variant:choose_spell_copy_retarget>.legal_action.semantic.actor
<variant:choose_spell_mode>.legal_action.semantic.actor
<variant:choose_target>.legal_action.semantic.actor
<variant:discard>.legal_action.semantic.actor
<variant:finish_effect_selection>.legal_action.semantic.actor
<variant:finish_target_selection>.legal_action.semantic.actor
<variant:order_triggers>.legal_action.semantic.actor
<variant:pass>.legal_action.semantic.actor
<variant:play_land>.legal_action.semantic.actor
<variant:plot_spell>.legal_action.semantic.actor""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'combat_attacker_relation_derivation_via_typed_object_resolution',
        """observation.projection.combat.ordered_attackers.[].card_db_id
observation.projection.combat.ordered_attackers.[].controller
observation.projection.combat.ordered_attackers.[].owner
observation.projection.combat.ordered_attackers.[].zone""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'combat_blocker_relation_derivation_via_typed_object_resolution',
        """observation.projection.combat.attacker_to_ordered_blockers.[].0.card_db_id
observation.projection.combat.attacker_to_ordered_blockers.[].0.controller
observation.projection.combat.attacker_to_ordered_blockers.[].0.owner
observation.projection.combat.attacker_to_ordered_blockers.[].0.zone
observation.projection.combat.attacker_to_ordered_blockers.[].1.[].card_db_id
observation.projection.combat.attacker_to_ordered_blockers.[].1.[].controller
observation.projection.combat.attacker_to_ordered_blockers.[].1.[].owner
observation.projection.combat.attacker_to_ordered_blockers.[].1.[].zone""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'effect_relation_object_index_derivation_via_typed_object_resolution',
        """observation.projection.continuous_effects.[].affected_objects.[].card_db_id
observation.projection.continuous_effects.[].affected_objects.[].controller
observation.projection.continuous_effects.[].affected_objects.[].owner
observation.projection.continuous_effects.[].affected_objects.[].zone
observation.projection.continuous_effects.[].source.card_db_id
observation.projection.continuous_effects.[].source.controller
observation.projection.continuous_effects.[].source.owner
observation.projection.continuous_effects.[].source.zone""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'effect_source_relation_presence_derivation',
        """observation.projection.continuous_effects.[].source.<present>""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'known_library_position_derivation_to_FlatObjectCoreV1.visible_ordinal_and_FlatRelationV1.secondary_order',
        """observation.known_library_cards.[].[].position""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'object_relation_endpoint_derivation_via_typed_object_resolution',
        """<variant:attached_to>.observation.projection.object_relations.[].attached_to.card_db_id
<variant:attached_to>.observation.projection.object_relations.[].attached_to.controller
<variant:attached_to>.observation.projection.object_relations.[].attached_to.owner
<variant:attached_to>.observation.projection.object_relations.[].attached_to.zone
<variant:attached_to>.observation.projection.object_relations.[].object.card_db_id
<variant:attached_to>.observation.projection.object_relations.[].object.controller
<variant:attached_to>.observation.projection.object_relations.[].object.owner
<variant:attached_to>.observation.projection.object_relations.[].object.zone
<variant:exiled_by>.observation.projection.object_relations.[].exiled_by.card_db_id
<variant:exiled_by>.observation.projection.object_relations.[].exiled_by.controller
<variant:exiled_by>.observation.projection.object_relations.[].exiled_by.owner
<variant:exiled_by>.observation.projection.object_relations.[].exiled_by.zone
<variant:exiled_by>.observation.projection.object_relations.[].object.card_db_id
<variant:exiled_by>.observation.projection.object_relations.[].object.controller
<variant:exiled_by>.observation.projection.object_relations.[].object.owner
<variant:exiled_by>.observation.projection.object_relations.[].object.zone""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'paid_cost_relation_derivation_via_historical_object_resolution',
        """observation.projection.stack.[].paid_cost_refs.[].card_db_id
observation.projection.stack.[].paid_cost_refs.[].controller
observation.projection.stack.[].paid_cost_refs.[].owner
observation.projection.stack.[].paid_cost_refs.[].zone""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'pending_activation_relation_derivation',
        """<variant:object>.observation.projection.engine_context.pending_activation.chosen_targets.[].object.card_db_id
<variant:object>.observation.projection.engine_context.pending_activation.chosen_targets.[].object.controller
<variant:object>.observation.projection.engine_context.pending_activation.chosen_targets.[].object.owner
<variant:object>.observation.projection.engine_context.pending_activation.chosen_targets.[].object.zone
<variant:object>.observation.projection.engine_context.pending_activation.chosen_targets.[].target_kind
<variant:player>.observation.projection.engine_context.pending_activation.chosen_targets.[].player
<variant:player>.observation.projection.engine_context.pending_activation.chosen_targets.[].target_kind
observation.projection.engine_context.pending_activation.chosen_targets.[].target_kind
observation.projection.engine_context.pending_activation.cost_discard_paid.[].card_db_id
observation.projection.engine_context.pending_activation.cost_discard_paid.[].controller
observation.projection.engine_context.pending_activation.cost_discard_paid.[].owner
observation.projection.engine_context.pending_activation.cost_discard_paid.[].zone
observation.projection.engine_context.pending_activation.source.card_db_id
observation.projection.engine_context.pending_activation.source.controller
observation.projection.engine_context.pending_activation.source.owner
observation.projection.engine_context.pending_activation.source.zone""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'pending_cast_relation_derivation',
        """<variant:object>.observation.projection.engine_context.pending_cast.chosen_targets.[].object.card_db_id
<variant:object>.observation.projection.engine_context.pending_cast.chosen_targets.[].object.controller
<variant:object>.observation.projection.engine_context.pending_cast.chosen_targets.[].object.owner
<variant:object>.observation.projection.engine_context.pending_cast.chosen_targets.[].object.zone
<variant:object>.observation.projection.engine_context.pending_cast.chosen_targets.[].target_kind
<variant:player>.observation.projection.engine_context.pending_cast.chosen_targets.[].player
<variant:player>.observation.projection.engine_context.pending_cast.chosen_targets.[].target_kind
observation.projection.engine_context.pending_cast.additional_cost_discarded.[].card_db_id
observation.projection.engine_context.pending_cast.additional_cost_discarded.[].controller
observation.projection.engine_context.pending_cast.additional_cost_discarded.[].owner
observation.projection.engine_context.pending_cast.additional_cost_discarded.[].zone
observation.projection.engine_context.pending_cast.chosen_targets.[].target_kind
observation.projection.engine_context.pending_cast.sacrifice_chosen.[].card_db_id
observation.projection.engine_context.pending_cast.sacrifice_chosen.[].controller
observation.projection.engine_context.pending_cast.sacrifice_chosen.[].owner
observation.projection.engine_context.pending_cast.sacrifice_chosen.[].zone
observation.projection.engine_context.pending_cast.source.card_db_id
observation.projection.engine_context.pending_cast.source.controller
observation.projection.engine_context.pending_cast.source.owner
observation.projection.engine_context.pending_cast.source.zone""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'pending_discard_resume_relation_derivation',
        """observation.projection.engine_context.pending_discard.resume_source.card_db_id
observation.projection.engine_context.pending_discard.resume_source.controller
observation.projection.engine_context.pending_discard.resume_source.owner
observation.projection.engine_context.pending_discard.resume_source.zone""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'pending_effect_source_relation_derivation',
        """observation.projection.engine_context.pending_effect.source.card_db_id
observation.projection.engine_context.pending_effect.source.controller
observation.projection.engine_context.pending_effect.source.owner
observation.projection.engine_context.pending_effect.source.zone""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'pending_effect_target_relation_derivation_via_typed_object_resolution',
        """<variant:targets>.<variant:object>.observation.projection.engine_context.pending_effect.choice.legal_targets.[].object.card_db_id
<variant:targets>.<variant:object>.observation.projection.engine_context.pending_effect.choice.legal_targets.[].object.controller
<variant:targets>.<variant:object>.observation.projection.engine_context.pending_effect.choice.legal_targets.[].object.owner
<variant:targets>.<variant:object>.observation.projection.engine_context.pending_effect.choice.legal_targets.[].object.zone
<variant:targets>.<variant:object>.observation.projection.engine_context.pending_effect.choice.selected_targets.[].object.card_db_id
<variant:targets>.<variant:object>.observation.projection.engine_context.pending_effect.choice.selected_targets.[].object.controller
<variant:targets>.<variant:object>.observation.projection.engine_context.pending_effect.choice.selected_targets.[].object.owner
<variant:targets>.<variant:object>.observation.projection.engine_context.pending_effect.choice.selected_targets.[].object.zone""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'pending_optional_cost_relation_derivation',
        """observation.projection.engine_context.pending_optional_cost.source.card_db_id
observation.projection.engine_context.pending_optional_cost.source.controller
observation.projection.engine_context.pending_optional_cost.source.owner
observation.projection.engine_context.pending_optional_cost.source.zone
observation.projection.engine_context.pending_optional_cost.spell_resume_source.card_db_id
observation.projection.engine_context.pending_optional_cost.spell_resume_source.controller
observation.projection.engine_context.pending_optional_cost.spell_resume_source.owner
observation.projection.engine_context.pending_optional_cost.spell_resume_source.zone""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'pending_optional_sacrifice_relation_derivation',
        """observation.projection.engine_context.pending_optional_cost_sacrifice.chosen.[].card_db_id
observation.projection.engine_context.pending_optional_cost_sacrifice.chosen.[].controller
observation.projection.engine_context.pending_optional_cost_sacrifice.chosen.[].owner
observation.projection.engine_context.pending_optional_cost_sacrifice.chosen.[].zone
observation.projection.engine_context.pending_optional_cost_sacrifice.source.card_db_id
observation.projection.engine_context.pending_optional_cost_sacrifice.source.controller
observation.projection.engine_context.pending_optional_cost_sacrifice.source.owner
observation.projection.engine_context.pending_optional_cost_sacrifice.source.zone
observation.projection.engine_context.pending_optional_cost_sacrifice.spell_resume_source.card_db_id
observation.projection.engine_context.pending_optional_cost_sacrifice.spell_resume_source.controller
observation.projection.engine_context.pending_optional_cost_sacrifice.spell_resume_source.owner
observation.projection.engine_context.pending_optional_cost_sacrifice.spell_resume_source.zone""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'pending_spell_copy_relation_derivation',
        """<variant:object>.observation.projection.engine_context.pending_spell_copy.inherited_target.object.card_db_id
<variant:object>.observation.projection.engine_context.pending_spell_copy.inherited_target.object.controller
<variant:object>.observation.projection.engine_context.pending_spell_copy.inherited_target.object.owner
<variant:object>.observation.projection.engine_context.pending_spell_copy.inherited_target.object.zone
observation.projection.engine_context.pending_spell_copy.copy.card_db_id
observation.projection.engine_context.pending_spell_copy.copy.controller
observation.projection.engine_context.pending_spell_copy.copy.owner
observation.projection.engine_context.pending_spell_copy.copy.zone
observation.projection.engine_context.pending_spell_copy.parent.card_db_id
observation.projection.engine_context.pending_spell_copy.parent.controller
observation.projection.engine_context.pending_spell_copy.parent.owner
observation.projection.engine_context.pending_spell_copy.parent.zone""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'pending_trigger_source_presence_derivation',
        """observation.projection.engine_context.pending_triggers.[].source.<present>""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'pending_trigger_source_relation_derivation',
        """observation.projection.engine_context.pending_triggers.[].source.card_db_id
observation.projection.engine_context.pending_triggers.[].source.controller
observation.projection.engine_context.pending_triggers.[].source.owner
observation.projection.engine_context.pending_triggers.[].source.zone""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'permission_relation_object_index_derivation_via_typed_object_resolution',
        """observation.projection.exile_play_permissions.[].object.card_db_id
observation.projection.exile_play_permissions.[].object.controller
observation.projection.exile_play_permissions.[].object.owner
observation.projection.exile_play_permissions.[].object.zone""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'private_combat_relation_derivation_via_typed_object_resolution',
        """observation.projection.policy_surface_context.private_combat_selection.attacker.card_db_id
observation.projection.policy_surface_context.private_combat_selection.attacker.controller
observation.projection.policy_surface_context.private_combat_selection.attacker.owner
observation.projection.policy_surface_context.private_combat_selection.attacker.zone
observation.projection.policy_surface_context.private_combat_selection.current_candidate.card_db_id
observation.projection.policy_surface_context.private_combat_selection.current_candidate.controller
observation.projection.policy_surface_context.private_combat_selection.current_candidate.owner
observation.projection.policy_surface_context.private_combat_selection.current_candidate.zone
observation.projection.policy_surface_context.private_combat_selection.remaining_after_current.[].card_db_id
observation.projection.policy_surface_context.private_combat_selection.remaining_after_current.[].controller
observation.projection.policy_surface_context.private_combat_selection.remaining_after_current.[].owner
observation.projection.policy_surface_context.private_combat_selection.remaining_after_current.[].zone
observation.projection.policy_surface_context.private_combat_selection.selected.[].card_db_id
observation.projection.policy_surface_context.private_combat_selection.selected.[].controller
observation.projection.policy_surface_context.private_combat_selection.selected.[].owner
observation.projection.policy_surface_context.private_combat_selection.selected.[].zone""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'private_context_relation_derivation_via_typed_object_resolution',
        """observation.projection.surface_context.madness_cast_reprompt_source.card_db_id
observation.projection.surface_context.madness_cast_reprompt_source.controller
observation.projection.surface_context.madness_cast_reprompt_source.owner
observation.projection.surface_context.madness_cast_reprompt_source.zone
observation.projection.surface_context.private_blockers.accumulated.[].0.card_db_id
observation.projection.surface_context.private_blockers.accumulated.[].0.controller
observation.projection.surface_context.private_blockers.accumulated.[].0.owner
observation.projection.surface_context.private_blockers.accumulated.[].0.zone
observation.projection.surface_context.private_blockers.accumulated.[].1.card_db_id
observation.projection.surface_context.private_blockers.accumulated.[].1.controller
observation.projection.surface_context.private_blockers.accumulated.[].1.owner
observation.projection.surface_context.private_blockers.accumulated.[].1.zone
observation.projection.surface_context.private_blockers.current_attacker.<present>
observation.projection.surface_context.private_blockers.current_attacker.card_db_id
observation.projection.surface_context.private_blockers.current_attacker.controller
observation.projection.surface_context.private_blockers.current_attacker.owner
observation.projection.surface_context.private_blockers.current_attacker.zone
observation.projection.surface_context.private_blockers.remaining.[].0.card_db_id
observation.projection.surface_context.private_blockers.remaining.[].0.controller
observation.projection.surface_context.private_blockers.remaining.[].0.owner
observation.projection.surface_context.private_blockers.remaining.[].0.zone
observation.projection.surface_context.private_blockers.remaining.[].1.[].card_db_id
observation.projection.surface_context.private_blockers.remaining.[].1.[].controller
observation.projection.surface_context.private_blockers.remaining.[].1.[].owner
observation.projection.surface_context.private_blockers.remaining.[].1.[].zone
observation.projection.surface_context.private_discard.chosen.[].card_db_id
observation.projection.surface_context.private_discard.chosen.[].controller
observation.projection.surface_context.private_discard.chosen.[].owner
observation.projection.surface_context.private_discard.chosen.[].zone
observation.projection.surface_context.private_discard.remaining_choices.[].card_db_id
observation.projection.surface_context.private_discard.remaining_choices.[].controller
observation.projection.surface_context.private_discard.remaining_choices.[].owner
observation.projection.surface_context.private_discard.remaining_choices.[].zone""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'private_discard_globals_and_context_presence_derivation',
        """observation.projection.surface_context.private_discard.<present>""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'private_optional_cost_globals_presence_derivation',
        """observation.projection.surface_context.private_optional_cost.<present>""",
    ),
    *_destination_rows(
        MODEL_INPUT,
        'stack_target_object_relation_derivation_via_typed_object_resolution',
        """<variant:object>.observation.projection.stack.[].targets.[].object.card_db_id
<variant:object>.observation.projection.stack.[].targets.[].object.owner
<variant:object>.observation.projection.stack.[].targets.[].object.zone""",
    ),
    *_destination_rows(
        OPERATIONAL_ONLY,
        'FlatActionDecisionBindingV1.bound_policy_step_count contract validation',
        """observation.step_index""",
    ),
    *_destination_rows(
        OPERATIONAL_ONLY,
        'FlatActionDecisionBindingV1.card_db_hash contract validation',
        """observation.card_db_hash""",
    ),
    *_destination_rows(
        OPERATIONAL_ONLY,
        'FlatActionDecisionBindingV1.physical_decision_id contract validation',
        """observation.physical_decision_id""",
    ),
    *_destination_rows(
        OPERATIONAL_ONLY,
        'FlatActionDecisionBindingV1.substep_count contract validation',
        """observation.substep_count""",
    ),
    *_destination_rows(
        OPERATIONAL_ONLY,
        'FlatActionDecisionBindingV1.substep_index contract validation',
        """observation.substep_index""",
    ),
    *_destination_rows(
        OPERATIONAL_ONLY,
        'ObservationV5 schema contract validation',
        """observation.schema_version""",
    ),
    *_destination_rows(
        OPERATIONAL_ONLY,
        'action_object_index_derivation_via_private_incarnation_resolution',
        """<variant:activate_ability>.legal_action.semantic.source.arena_id
<variant:activate_ability>.legal_action.semantic.source.zone_change_count
<variant:activate_mana_ability>.legal_action.semantic.source.arena_id
<variant:activate_mana_ability>.legal_action.semantic.source.zone_change_count
<variant:cast_spell>.legal_action.semantic.source.arena_id
<variant:cast_spell>.legal_action.semantic.source.zone_change_count
<variant:choose_attacker_inclusion>.legal_action.semantic.attacker.arena_id
<variant:choose_attacker_inclusion>.legal_action.semantic.attacker.zone_change_count
<variant:choose_blocker_inclusion>.legal_action.semantic.attacker.arena_id
<variant:choose_blocker_inclusion>.legal_action.semantic.attacker.zone_change_count
<variant:choose_blocker_inclusion>.legal_action.semantic.blocker.arena_id
<variant:choose_blocker_inclusion>.legal_action.semantic.blocker.zone_change_count
<variant:choose_cast_mode>.legal_action.semantic.source.arena_id
<variant:choose_cast_mode>.legal_action.semantic.source.zone_change_count
<variant:choose_cost_target>.legal_action.semantic.candidate.arena_id
<variant:choose_cost_target>.legal_action.semantic.candidate.zone_change_count
<variant:choose_cost_target>.legal_action.semantic.source.arena_id
<variant:choose_cost_target>.legal_action.semantic.source.zone_change_count
<variant:choose_effect_boolean>.legal_action.semantic.source.arena_id
<variant:choose_effect_boolean>.legal_action.semantic.source.zone_change_count
<variant:choose_effect_color>.legal_action.semantic.source.arena_id
<variant:choose_effect_color>.legal_action.semantic.source.zone_change_count
<variant:choose_effect_number>.legal_action.semantic.source.arena_id
<variant:choose_effect_number>.legal_action.semantic.source.zone_change_count
<variant:choose_effect_option>.legal_action.semantic.source.arena_id
<variant:choose_effect_option>.legal_action.semantic.source.zone_change_count
<variant:choose_effect_target>.<variant:object>.legal_action.semantic.target.object.arena_id
<variant:choose_effect_target>.<variant:object>.legal_action.semantic.target.object.zone_change_count
<variant:choose_effect_target>.legal_action.semantic.source.arena_id
<variant:choose_effect_target>.legal_action.semantic.source.zone_change_count
<variant:choose_kicker>.legal_action.semantic.source.arena_id
<variant:choose_kicker>.legal_action.semantic.source.zone_change_count
<variant:choose_madness_cast>.legal_action.semantic.card.arena_id
<variant:choose_madness_cast>.legal_action.semantic.card.zone_change_count
<variant:choose_spell_copy_payment>.legal_action.semantic.source.arena_id
<variant:choose_spell_copy_payment>.legal_action.semantic.source.zone_change_count
<variant:choose_spell_copy_retarget>.legal_action.semantic.source.arena_id
<variant:choose_spell_copy_retarget>.legal_action.semantic.source.zone_change_count
<variant:choose_spell_mode>.legal_action.semantic.source.arena_id
<variant:choose_spell_mode>.legal_action.semantic.source.zone_change_count
<variant:choose_target>.<variant:object>.legal_action.semantic.target.object.arena_id
<variant:choose_target>.<variant:object>.legal_action.semantic.target.object.zone_change_count
<variant:choose_target>.legal_action.semantic.source.arena_id
<variant:choose_target>.legal_action.semantic.source.zone_change_count
<variant:discard>.legal_action.semantic.cards.[].arena_id
<variant:discard>.legal_action.semantic.cards.[].zone_change_count
<variant:finish_effect_selection>.legal_action.semantic.source.arena_id
<variant:finish_effect_selection>.legal_action.semantic.source.zone_change_count
<variant:finish_target_selection>.legal_action.semantic.source.arena_id
<variant:finish_target_selection>.legal_action.semantic.source.zone_change_count
<variant:order_triggers>.legal_action.semantic.pending_sources.[].arena_id
<variant:order_triggers>.legal_action.semantic.pending_sources.[].zone_change_count
<variant:play_land>.legal_action.semantic.source.arena_id
<variant:play_land>.legal_action.semantic.source.zone_change_count
<variant:plot_spell>.legal_action.semantic.source.arena_id
<variant:plot_spell>.legal_action.semantic.source.zone_change_count""",
    ),
    *_destination_rows(
        OPERATIONAL_ONLY,
        'attachment_relation_derivation_via_private_arena_resolution',
        """observation.projection.battlefield.[].[].attachments.[]
observation.projection.exile.[].attachments.[]
observation.projection.graveyards.[].[].attachments.[]""",
    ),
    *_destination_rows(
        OPERATIONAL_ONLY,
        'ordered_action_contract_validation',
        """legal_action.schema_version""",
    ),
    *_destination_rows(
        OPERATIONAL_ONLY,
        'ordered_action_index_validation',
        """legal_action.selected_index""",
    ),
    *_destination_rows(
        OPERATIONAL_ONLY,
        'permission_incarnation_validation',
        """observation.projection.exile_play_permissions.[].zone_change_generation""",
    ),
    *_destination_rows(
        OPERATIONAL_ONLY,
        'turn_relation_and_expiry_normalization',
        """observation.projection.turn""",
    ),
    *_destination_rows(
        OPERATIONAL_ONLY,
        'typed_object_index_derivation_via_private_incarnation_resolution',
        """<variant:attached_to>.observation.projection.object_relations.[].attached_to.arena_id
<variant:attached_to>.observation.projection.object_relations.[].attached_to.zone_change_count
<variant:attached_to>.observation.projection.object_relations.[].object.arena_id
<variant:attached_to>.observation.projection.object_relations.[].object.zone_change_count
<variant:exiled_by>.observation.projection.object_relations.[].exiled_by.arena_id
<variant:exiled_by>.observation.projection.object_relations.[].exiled_by.zone_change_count
<variant:exiled_by>.observation.projection.object_relations.[].object.arena_id
<variant:exiled_by>.observation.projection.object_relations.[].object.zone_change_count
<variant:object>.observation.projection.engine_context.pending_activation.chosen_targets.[].object.arena_id
<variant:object>.observation.projection.engine_context.pending_activation.chosen_targets.[].object.zone_change_count
<variant:object>.observation.projection.engine_context.pending_cast.chosen_targets.[].object.arena_id
<variant:object>.observation.projection.engine_context.pending_cast.chosen_targets.[].object.zone_change_count
<variant:object>.observation.projection.engine_context.pending_spell_copy.inherited_target.object.arena_id
<variant:object>.observation.projection.engine_context.pending_spell_copy.inherited_target.object.zone_change_count
<variant:object>.observation.projection.stack.[].targets.[].object.arena_id
<variant:object>.observation.projection.stack.[].targets.[].object.zone_change_count
<variant:targets>.<variant:object>.observation.projection.engine_context.pending_effect.choice.legal_targets.[].object.arena_id
<variant:targets>.<variant:object>.observation.projection.engine_context.pending_effect.choice.legal_targets.[].object.zone_change_count
<variant:targets>.<variant:object>.observation.projection.engine_context.pending_effect.choice.selected_targets.[].object.arena_id
<variant:targets>.<variant:object>.observation.projection.engine_context.pending_effect.choice.selected_targets.[].object.zone_change_count
observation.known_hand_cards.[].[].stable.arena_id
observation.known_hand_cards.[].[].stable.zone_change_count
observation.known_library_cards.[].[].card.stable.arena_id
observation.known_library_cards.[].[].card.stable.zone_change_count
observation.own_hand.[].stable.arena_id
observation.own_hand.[].stable.zone_change_count
observation.projection.battlefield.[].[].stable.arena_id
observation.projection.battlefield.[].[].stable.zone_change_count
observation.projection.combat.attacker_to_ordered_blockers.[].0.arena_id
observation.projection.combat.attacker_to_ordered_blockers.[].0.zone_change_count
observation.projection.combat.attacker_to_ordered_blockers.[].1.[].arena_id
observation.projection.combat.attacker_to_ordered_blockers.[].1.[].zone_change_count
observation.projection.combat.ordered_attackers.[].arena_id
observation.projection.combat.ordered_attackers.[].zone_change_count
observation.projection.continuous_effects.[].affected_objects.[].arena_id
observation.projection.continuous_effects.[].affected_objects.[].zone_change_count
observation.projection.continuous_effects.[].source.arena_id
observation.projection.continuous_effects.[].source.zone_change_count
observation.projection.engine_context.pending_activation.cost_discard_paid.[].arena_id
observation.projection.engine_context.pending_activation.cost_discard_paid.[].zone_change_count
observation.projection.engine_context.pending_activation.source.arena_id
observation.projection.engine_context.pending_activation.source.zone_change_count
observation.projection.engine_context.pending_cast.additional_cost_discarded.[].arena_id
observation.projection.engine_context.pending_cast.additional_cost_discarded.[].zone_change_count
observation.projection.engine_context.pending_cast.sacrifice_chosen.[].arena_id
observation.projection.engine_context.pending_cast.sacrifice_chosen.[].zone_change_count
observation.projection.engine_context.pending_cast.source.arena_id
observation.projection.engine_context.pending_cast.source.zone_change_count
observation.projection.engine_context.pending_discard.resume_source.arena_id
observation.projection.engine_context.pending_discard.resume_source.zone_change_count
observation.projection.engine_context.pending_effect.source.arena_id
observation.projection.engine_context.pending_effect.source.zone_change_count
observation.projection.engine_context.pending_optional_cost.source.arena_id
observation.projection.engine_context.pending_optional_cost.source.zone_change_count
observation.projection.engine_context.pending_optional_cost.spell_resume_source.arena_id
observation.projection.engine_context.pending_optional_cost.spell_resume_source.zone_change_count
observation.projection.engine_context.pending_optional_cost_sacrifice.chosen.[].arena_id
observation.projection.engine_context.pending_optional_cost_sacrifice.chosen.[].zone_change_count
observation.projection.engine_context.pending_optional_cost_sacrifice.source.arena_id
observation.projection.engine_context.pending_optional_cost_sacrifice.source.zone_change_count
observation.projection.engine_context.pending_optional_cost_sacrifice.spell_resume_source.arena_id
observation.projection.engine_context.pending_optional_cost_sacrifice.spell_resume_source.zone_change_count
observation.projection.engine_context.pending_spell_copy.copy.arena_id
observation.projection.engine_context.pending_spell_copy.copy.zone_change_count
observation.projection.engine_context.pending_spell_copy.parent.arena_id
observation.projection.engine_context.pending_spell_copy.parent.zone_change_count
observation.projection.engine_context.pending_triggers.[].source.arena_id
observation.projection.engine_context.pending_triggers.[].source.zone_change_count
observation.projection.exile.[].stable.arena_id
observation.projection.exile.[].stable.zone_change_count
observation.projection.exile_play_permissions.[].object.arena_id
observation.projection.exile_play_permissions.[].object.zone_change_count
observation.projection.graveyards.[].[].stable.arena_id
observation.projection.graveyards.[].[].stable.zone_change_count
observation.projection.policy_surface_context.private_combat_selection.attacker.arena_id
observation.projection.policy_surface_context.private_combat_selection.attacker.zone_change_count
observation.projection.policy_surface_context.private_combat_selection.current_candidate.arena_id
observation.projection.policy_surface_context.private_combat_selection.current_candidate.zone_change_count
observation.projection.policy_surface_context.private_combat_selection.remaining_after_current.[].arena_id
observation.projection.policy_surface_context.private_combat_selection.remaining_after_current.[].zone_change_count
observation.projection.policy_surface_context.private_combat_selection.selected.[].arena_id
observation.projection.policy_surface_context.private_combat_selection.selected.[].zone_change_count
observation.projection.stack.[].paid_cost_refs.[].arena_id
observation.projection.stack.[].paid_cost_refs.[].zone_change_count
observation.projection.stack.[].source.arena_id
observation.projection.stack.[].source.zone_change_count
observation.projection.surface_context.madness_cast_reprompt_source.arena_id
observation.projection.surface_context.madness_cast_reprompt_source.zone_change_count
observation.projection.surface_context.private_blockers.accumulated.[].0.arena_id
observation.projection.surface_context.private_blockers.accumulated.[].0.zone_change_count
observation.projection.surface_context.private_blockers.accumulated.[].1.arena_id
observation.projection.surface_context.private_blockers.accumulated.[].1.zone_change_count
observation.projection.surface_context.private_blockers.current_attacker.arena_id
observation.projection.surface_context.private_blockers.current_attacker.zone_change_count
observation.projection.surface_context.private_blockers.remaining.[].0.arena_id
observation.projection.surface_context.private_blockers.remaining.[].0.zone_change_count
observation.projection.surface_context.private_blockers.remaining.[].1.[].arena_id
observation.projection.surface_context.private_blockers.remaining.[].1.[].zone_change_count
observation.projection.surface_context.private_discard.chosen.[].arena_id
observation.projection.surface_context.private_discard.chosen.[].zone_change_count
observation.projection.surface_context.private_discard.remaining_choices.[].arena_id
observation.projection.surface_context.private_discard.remaining_choices.[].zone_change_count""",
    ),
    *_destination_rows(
        OPERATIONAL_ONLY,
        'typed_observation_operational_metadata_not_materialized',
        """observation.kernel_version
observation.policy_surface_version
observation.surface_version""",
    ),
    *_destination_rows(
        OPERATIONAL_ONLY,
        'typed_observation_order_metadata_not_materialized',
        """observation.projection.continuous_effects.[].timestamp
observation.projection.stack.[].stack_index""",
    ),
)


def _build_destination_registry(
    rows: tuple[tuple[str, str, str], ...] = _DESTINATION_ROWS,
) -> dict[str, tuple[str, str]]:
    registry: dict[str, tuple[str, str]] = {}
    for path, classification, destination in rows:
        if path in registry:
            raise AssertionError(f'duplicate destination declaration: {path}')
        if not path or path != path.strip() or not destination:
            raise AssertionError(f'invalid destination declaration: {path!r}')
        if classification not in (MODEL_INPUT, OPERATIONAL_ONLY, FORBIDDEN):
            raise AssertionError(
                f'invalid destination classification for {path}: {classification}'
            )
        if (destination == 'absent') != (classification == FORBIDDEN):
            raise AssertionError(
                f'forbidden/destination mismatch for {path}: '
                f'{classification} -> {destination}'
            )
        if classification == MODEL_INPUT and destination.startswith(
            'FlatActionObjectV1'
        ):
            raise AssertionError(f'private action-object destination for {path}')
        registry[path] = (classification, destination)
    return registry


DESTINATION_REGISTRY = _build_destination_registry()
_DISPOSITIONS = {
    MODEL_INPUT: 'model_input',
    OPERATIONAL_ONLY: 'operational_validation',
    FORBIDDEN: 'forbidden',
}


def _destination(path: str, classification: str) -> tuple[str, str]:
    try:
        expected_classification, destination = DESTINATION_REGISTRY[path]
    except KeyError as exc:
        raise AssertionError(f'undeclared normalized semantic path: {path}') from exc
    if classification != expected_classification:
        raise AssertionError(
            f'classification drift for {path}: '
            f'{classification} != {expected_classification}'
        )
    return destination, _DISPOSITIONS[classification]


def _assert_destination_coverage(
    authoritative: dict[str, str],
    declared: dict[str, tuple[str, str]] = DESTINATION_REGISTRY,
) -> None:
    missing = sorted(set(authoritative) - set(declared))
    extra = sorted(set(declared) - set(authoritative))
    if missing or extra:
        raise AssertionError(
            "destination registry coverage drift: "
            f"missing={missing}, extra={extra}"
        )


def _validate_inventory_entries(entries: list[dict[str, str]]) -> None:
    paths = [entry["path"] for entry in entries]
    if len(paths) != len(set(paths)):
        raise AssertionError("duplicate path in generated destination inventory")
    for entry in entries:
        path = entry["path"]
        classification = entry["classification"]
        destination = entry["destination"]
        if not destination:
            raise AssertionError(f"empty destination in generated inventory: {path}")
        if (destination == "absent") != (classification == FORBIDDEN):
            raise AssertionError(
                f"forbidden/destination mismatch in generated inventory: {path}"
            )
        if classification == MODEL_INPUT and destination.startswith(
            "FlatActionObjectV1"
        ):
            raise AssertionError(
                f"private action-object destination in generated inventory: {path}"
            )


def _inventory() -> dict[str, Any]:
    registry = classification_registry()
    _assert_destination_coverage(registry)
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
    _validate_inventory_entries(entries)
    counts = {
        classification: sum(entry["classification"] == classification for entry in entries)
        for classification in (MODEL_INPUT, OPERATIONAL_ONLY, FORBIDDEN)
    }
    if sum(counts.values()) != len(entries):
        raise AssertionError(
            f"unrecognized classifications in generated inventory: {counts}"
        )
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


def _action_ref_role_crosswalk() -> dict[str, Any]:
    projection_ids = _indexed(ACTION_REF_ROLES)
    assert ACTION_REF_ROLES == [
        "source",
        "candidate",
        "card",
        "attacker",
        "blocker",
        "target_object",
        "cards",
        "attackers",
        "blockers",
        "pending_sources",
    ]
    return {
        "schema": "flat-policy-action-ref-role-crosswalk-v1",
        "mapping_version": ACTION_REF_ROLE_CROSSWALK_VERSION,
        "rust_internal_width": len(RUST_INTERNAL_ACTION_REF_ROLES),
        "python_projection_width": len(ACTION_REF_ROLES),
        "entries": [
            {
                "role": role,
                "rust_internal_id": internal_id,
                "python_projection_id": projection_ids[role],
            }
            for internal_id, role in enumerate(RUST_INTERNAL_ACTION_REF_ROLES)
        ],
        "projection_only": [
            {"role": role, "python_projection_id": projection_ids[role]}
            for role in ("attackers", "blockers")
        ],
    }


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
    action_ref_role_crosswalk = _action_ref_role_crosswalk()
    mapping_contract = {
        "action_ref_role_crosswalk": action_ref_role_crosswalk,
        "enum_maps": enum_maps,
    }
    fixture_cases = [
        {
            "name": "burn_seed_11_initial",
            "deck_ids": ["Burn", "Burn"],
            "seed": 11,
            "episode_id": 90001,
            "decision_index": 0,
            "counts": [7, 0, 0, 0, 0, 0, 0, 0, 3, 2, 2],
            "model_typed_debug_sha256": "8998b6e1cdec83d52a3d1d5fc47a19195eb938a7580b0728dabffc4b9d5f0749",
            "action_objects_operational_debug_sha256": "caea06d6b76d7d8238d5fc4a426b81b10cef34afe2177821fcc4c7486725c9c7",
        },
        {
            "name": "rally_seed_23_initial",
            "deck_ids": ["Rally", "Rally"],
            "seed": 23,
            "episode_id": 90002,
            "decision_index": 0,
            "counts": [7, 0, 0, 0, 0, 0, 0, 0, 5, 4, 4],
            "model_typed_debug_sha256": "73b41a8669de8ca0492a29edabab26ecb88955249e7714cc37bde7f69795ecc9",
            "action_objects_operational_debug_sha256": "56e018d1a0e72785d71fc253b172c141005cb2c2e1aae017976d7079b231c42c",
        },
        {
            "name": "burn_rally_seed_37_initial",
            "deck_ids": ["Burn", "Rally"],
            "seed": 37,
            "episode_id": 90003,
            "decision_index": 0,
            "counts": [7, 0, 0, 0, 0, 0, 0, 0, 4, 3, 3],
            "model_typed_debug_sha256": "743c23d4ba6dfd083e6a306c57515a4ab265469888509beabf3cc2c7e1032f4c",
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
            "model_typed_debug_sha256": "10dee73c86f363a8a235a16ccdbdb40f1834ff70cb59101cfb6b6f28d65d6ef6",
        },
        {
            "name": "rally_seed_81702_first_relation",
            "deck_ids": ["Rally", "Rally"],
            "seed": 81702,
            "episode_id": 90102,
            "decision_index": 4,
            "selection_policy": "splitmix64_mod_width_include_true_for_combat_v1",
            "counts": [9, 2, 1, 0, 0, 0, 0, 0, 2, 2, 1],
            "model_typed_debug_sha256": "16257d501d29e2e778cf55150b1644c922fd07ea81a199fce134cb3f5f75ffd8",
        },
    ]
    payload: dict[str, Any] = {
        "schema": "flat-policy-v1-independent-goldens-v1",
        "producer_parent_commit": "3de6d2c450d4d32f120f70034324bfc72e1e3339",
        "feature_contract_digest": inventory["feature_contract_digest"],
        "encoding_contract_digest": inventory["encoding_contract_digest"],
        "mapping_sha256": hashlib.sha256(
            json.dumps(mapping_contract, sort_keys=True, separators=(",", ":")).encode()
        ).hexdigest(),
        "inventory_sha256": hashlib.sha256(
            json.dumps(inventory, sort_keys=True, separators=(",", ":")).encode()
        ).hexdigest(),
        "card_catalog_sha256": _sha256(CARDS_PATH),
        "enum_maps": enum_maps,
        "action_ref_role_crosswalk": action_ref_role_crosswalk,
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
