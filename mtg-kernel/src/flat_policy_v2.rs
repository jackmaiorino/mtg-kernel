//! Explicit V2 actor-relative typed policy input for the standalone fast actor.
//!
//! Unlike [`crate::rl_session::FlatActionDecisionSliceV2`], this module owns
//! the state side of the model-input contract: globals, all twenty ordered
//! object groups, all fourteen relation roles, and the variable-width typed
//! auxiliary tables.  The existing flat-action cache remains the sole source
//! of action rows and consume authority; this module neither reconstructs nor
//! reinterprets that cache.
//!
//! Raw arena ids, zone-incarnation counters, card names, display text, stable
//! ids, and observation hashes are deliberately absent from every public type
//! below.  They are used only inside the producer to resolve and validate
//! references before publication.

use crate::engine::CastMode;
use crate::flat_action_contract_v2::{
    FLAT_ACTION_CONTRACT_SEMANTIC_SHA256_V2, FLAT_ACTION_CONTRACT_SOURCE_SHA256_V2,
};
use crate::policy_surface_v5::PolicySurfaceStageV5;
use crate::rl::{
    BooleanChoicePurposeV4, CardPrivateV1, CardPublicV2, CardStableRefV1, ContinuousEffectPublicV2,
    DiscardResumeSemanticV2, EffectDurationV2, EngineDecisionStageV2, ExilePlayPermissionPublicV2,
    ObjectRelationPublicV4, ObservationV5, PendingEffectChoiceSemanticV4, PendingTriggerKindV2,
    PlayOrCastV2, PlayPermissionExpiryV2, PlayerSeatV1, SpellCopyStageV2, StackItemKindV2,
    SurfaceDecisionStageV2, TargetRefV1, TargetSelectionPurposeV4, ZoneIndependentStepV1,
};
use crate::rl_session::{
    FastActorDecisionV1, FastActorSessionV1, FlatActionCoreV1, FlatActionDecisionBindingV2,
    FlatActionDecisionSliceBuffersV2, FlatActionDecisionSliceErrorV1, FlatActionKindV1,
    FlatActionObjectGroupV1, FlatActionObjectV2, FlatActionRefRoleV1, FlatActionRefV2,
    FLAT_ACTION_MAX_TRIGGER_ORDER_REFS_V1,
};
use crate::state::{AbilityKindV4, CastMethodV4};
use crate::{mana::ManaColor, state::Zone};

pub const FLAT_POLICY_TYPED_LAYOUT_VERSION_V2: u32 = 2;
pub const FLAT_POLICY_FEATURE_INVENTORY_VERSION_V2: u32 = 2;
pub const FLAT_POLICY_ENUM_MAPPING_VERSION_V2: u32 = 1;
pub const FLAT_POLICY_OBJECT_GROUP_MAPPING_VERSION_V2: u32 = 1;
pub const FLAT_POLICY_RELATION_ROLE_MAPPING_VERSION_V2: u32 = 1;
pub const FLAT_POLICY_CONTEXT_SUBROLE_MAPPING_VERSION_V2: u32 = 1;
pub const FLAT_SCORER_PACKET_VERSION_V2: u32 = 2;
pub const FLAT_SCORER_ACTION_REF_VERSION_V2: u32 = 2;
pub const FLAT_SCORER_VISIBLE_MANIFEST_VERSION_V2: u32 = 2;
pub const FLAT_SCORER_VISIBLE_MANIFEST_V2: &str = "globals,objects,relations,object_subtypes,ability_uses,goads,completed_dungeons,effect_subtype_changes,context_path_elements,actions,action_refs";

include!(concat!(env!("OUT_DIR"), "/flat_policy_contract_v2.rs"));

const HISTORICAL_STACK_TARGET_KIND_V1: u8 = 1;
const HISTORICAL_PAID_COST_KIND_V1: u8 = 2;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum FlatRelativePlayerV2 {
    #[default]
    SelfPlayer = 0,
    Opponent = 1,
    None = 2,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum FlatZoneV2 {
    #[default]
    Library = 0,
    Hand = 1,
    Battlefield = 2,
    Graveyard = 3,
    Stack = 4,
    Exile = 5,
    Command = 6,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum FlatPhaseV2 {
    #[default]
    Untap = 0,
    Upkeep = 1,
    Draw = 2,
    Main1 = 3,
    BeginCombat = 4,
    DeclareAttackers = 5,
    DeclareBlockers = 6,
    CombatDamage = 7,
    EndCombat = 8,
    Main2 = 9,
    End = 10,
    Cleanup = 11,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum FlatManaColorV2 {
    #[default]
    White = 0,
    Blue = 1,
    Black = 2,
    Red = 3,
    Green = 4,
    Colorless = 5,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum FlatTurnRelationV2 {
    #[default]
    Absent = 0,
    ThisTurn = 1,
    EarlierTurn = 2,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum FlatObjectGroupV2 {
    #[default]
    SelfHand = 0,
    SelfBattlefield = 1,
    OpponentBattlefield = 2,
    SelfGraveyard = 3,
    OpponentGraveyard = 4,
    Exile = 5,
    Stack = 6,
    Combat = 7,
    ContinuousEffect = 8,
    Permission = 9,
    Attachment = 10,
    HistoricalStackTarget = 11,
    CombatBlock = 12,
    PendingContext = 13,
    PrivateContext = 14,
    KnownSelfLibrary = 15,
    KnownOpponentLibrary = 16,
    KnownSelfHand = 17,
    KnownOpponentHand = 18,
    HistoricalPaidCost = 19,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum FlatObjectSourceKindV2 {
    #[default]
    Card = 0,
    Stack = 1,
    Combat = 2,
    Effect = 3,
    Permission = 4,
    Attachment = 5,
    Target = 6,
    Pending = 7,
    Private = 8,
    KnownLibrary = 9,
    KnownHand = 10,
    PaidCost = 11,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum FlatRelationRoleV2 {
    #[default]
    Attachment = 0,
    StackTarget = 1,
    CombatAttacker = 2,
    CombatBlocker = 3,
    EffectAffected = 4,
    EffectSource = 5,
    Permission = 6,
    PendingContext = 7,
    PrivateContext = 8,
    KnownLibrary = 9,
    KnownHand = 10,
    AttachedTo = 11,
    ExiledBy = 12,
    PaidCost = 13,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum FlatTargetKindV2 {
    #[default]
    None = 0,
    Player = 1,
    Object = 2,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum FlatContextElementKindV2 {
    #[default]
    StructuralPath = 0,
    LegalColor = 1,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum FlatEffectSubtypeChangeKindV2 {
    #[default]
    Add = 0,
    Remove = 1,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum FlatContextKindV2 {
    #[default]
    PendingCast = 0,
    PendingActivation = 1,
    PendingDiscard = 2,
    PendingOptionalCost = 3,
    PendingOptionalCostSacrifice = 4,
    PendingSpellCopy = 5,
    PendingEffect = 6,
    PendingTrigger = 7,
    MadnessCastReprompt = 8,
    PrivateBlockers = 9,
    PrivateDiscard = 10,
    PrivateOptionalCost = 11,
    PrivateCombatSelection = 12,
}

/// Fixed ids for every card-reference position in pending/private state.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum FlatContextSubroleV2 {
    #[default]
    PendingCastSource = 0,
    PendingCastChosenTarget = 1,
    PendingCastDiscarded = 2,
    PendingCastSacrificed = 3,
    PendingActivationSource = 4,
    PendingActivationChosenTarget = 5,
    PendingActivationDiscarded = 6,
    PendingDiscardResumeSource = 7,
    PendingOptionalCostSource = 8,
    PendingOptionalCostSpellResumeSource = 9,
    PendingOptionalSacrificeSource = 10,
    PendingOptionalSacrificeChosen = 11,
    PendingOptionalSacrificeSpellResumeSource = 12,
    PendingSpellCopyParent = 13,
    PendingSpellCopyInheritedTarget = 14,
    PendingSpellCopyCopy = 15,
    PendingEffectSource = 16,
    PendingEffectSelectedTarget = 17,
    PendingEffectLegalTarget = 18,
    PendingTriggerSource = 19,
    MadnessCastRepromptSource = 20,
    PrivateBlockersCurrentAttacker = 21,
    PrivateBlockersAccumulatedAttacker = 22,
    PrivateBlockersAccumulatedBlocker = 23,
    PrivateBlockersRemainingAttacker = 24,
    PrivateBlockersRemainingBlocker = 25,
    PrivateDiscardChosen = 26,
    PrivateDiscardRemainingChoice = 27,
    PrivateCombatAttacker = 28,
    PrivateCombatSelected = 29,
    PrivateCombatCurrentCandidate = 30,
    PrivateCombatRemainingCandidate = 31,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct FlatPlayerGlobalsV2 {
    pub life: i32,
    pub mana: [u8; 6],
    pub hand_count: u64,
    pub library_count: u64,
    pub has_lost: bool,
    pub lands_played_this_turn: u8,
    pub drew_from_empty: bool,
    pub draws_this_turn: u32,
    pub spells_cast_this_turn: u16,
    pub dungeon_id: Option<u16>,
    pub room_id: Option<u16>,
    pub completed_dungeon_start: u32,
    pub completed_dungeon_count: u32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct FlatPendingCastGlobalsV2 {
    pub source_present: bool,
    pub controller: FlatRelativePlayerV2,
    pub chosen_target_count: u32,
    pub is_flashback: bool,
    pub cast_mode: u8,
    pub discarded_present: bool,
    pub discarded_count: u32,
    pub mode_chosen: Option<u8>,
    pub origin_zone: FlatZoneV2,
    pub sacrificed_count: u32,
    pub kicked: Option<bool>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct FlatPendingActivationGlobalsV2 {
    pub source_present: bool,
    pub controller: FlatRelativePlayerV2,
    pub ability_index: u8,
    pub chosen_target_count: u32,
    pub discard_paid_present: bool,
    pub discard_paid_count: u32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct FlatPendingDiscardGlobalsV2 {
    pub player: FlatRelativePlayerV2,
    pub count: u32,
    pub resume_stage: u8,
    pub resume_source_present: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct FlatPendingOptionalCostGlobalsV2 {
    pub player: FlatRelativePlayerV2,
    pub source_present: bool,
    pub discard_cards: u8,
    pub sacrifice_lands: u8,
    pub discard_payable: bool,
    pub sacrifice_payable: bool,
    pub spell_resume_source_present: bool,
    pub spell_resume_zone: Option<FlatZoneV2>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct FlatPendingOptionalSacrificeGlobalsV2 {
    pub player: FlatRelativePlayerV2,
    pub source_present: bool,
    pub remaining: u8,
    pub chosen_count: u32,
    pub spell_resume_source_present: bool,
    pub spell_resume_zone: Option<FlatZoneV2>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct FlatPendingSpellCopyGlobalsV2 {
    pub parent_present: bool,
    pub player: FlatRelativePlayerV2,
    pub inherited_target_kind: FlatTargetKindV2,
    pub inherited_target_player: FlatRelativePlayerV2,
    pub stage: u8,
    pub copy_present: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FlatPendingEffectChoiceV2 {
    Options {
        player: FlatRelativePlayerV2,
        path_start: u32,
        path_count: u32,
        option_count: u16,
    },
    Targets {
        player: FlatRelativePlayerV2,
        path_start: u32,
        path_count: u32,
        selected_count: u32,
        legal_count: u32,
        min_targets: u16,
        max_targets: u16,
        can_finish: bool,
        ordered: bool,
        purpose: u8,
    },
    Color {
        player: FlatRelativePlayerV2,
        path_start: u32,
        path_count: u32,
        legal_color_start: u32,
        legal_color_count: u32,
    },
    Number {
        player: FlatRelativePlayerV2,
        path_start: u32,
        path_count: u32,
        minimum: i32,
        maximum: i32,
    },
    Boolean {
        player: FlatRelativePlayerV2,
        path_start: u32,
        path_count: u32,
        default: Option<bool>,
        purpose: u8,
    },
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct FlatPendingEffectGlobalsV2 {
    pub source_present: bool,
    pub controller: FlatRelativePlayerV2,
    pub choice: Option<FlatPendingEffectChoiceV2>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct FlatEngineGlobalsV2 {
    pub priority_passes: [bool; 2],
    pub stack_nonempty: bool,
    pub stack_activity_since_priority_boundary: bool,
    pub mana_activity_since_priority_boundary: bool,
    pub last_mana_ability_activator: FlatRelativePlayerV2,
    pub current_stage: u8,
    pub pending_cast: Option<FlatPendingCastGlobalsV2>,
    pub pending_activation: Option<FlatPendingActivationGlobalsV2>,
    pub pending_discard: Option<FlatPendingDiscardGlobalsV2>,
    pub pending_optional_cost: Option<FlatPendingOptionalCostGlobalsV2>,
    pub pending_optional_sacrifice: Option<FlatPendingOptionalSacrificeGlobalsV2>,
    pub pending_spell_copy: Option<FlatPendingSpellCopyGlobalsV2>,
    pub pending_effect: Option<FlatPendingEffectGlobalsV2>,
    pub pending_trigger_count: u32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct FlatSurfaceGlobalsV2 {
    pub current_stage: u8,
    pub combat_priority_spent: [bool; 2],
    pub combat_priority_rearmed_by_stack_activity: bool,
    pub combat_priority_rearmed_by_mana_activity: bool,
    pub stack_grew_since_round_open: bool,
    pub mana_activity_since_round_open: bool,
    pub stack_length_changed_since_observed: Option<bool>,
    pub mana_activity_since_last_stack_change: bool,
    pub madness_cast_reprompt_source_present: bool,
    pub private_blockers_present: bool,
    pub private_discard_remaining_needed: Option<u32>,
    pub private_discard_chosen_count: u32,
    pub private_discard_remaining_count: u32,
    pub private_optional_discard_payable: Option<bool>,
    pub private_optional_sacrifice_payable: Option<bool>,
    pub private_optional_stage: Option<u8>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct FlatPolicySurfaceGlobalsV2 {
    pub current_stage: u8,
    pub private_combat_present: bool,
    pub private_combat_attacker_present: bool,
    pub candidate_index: u32,
    pub candidate_count: u32,
    pub selected_count: u32,
    pub remaining_count: u32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct FlatGlobalsV2 {
    pub acting_player: FlatRelativePlayerV2,
    pub phase: FlatPhaseV2,
    pub active_player: FlatRelativePlayerV2,
    pub priority_player: FlatRelativePlayerV2,
    pub initiative: FlatRelativePlayerV2,
    pub players: [FlatPlayerGlobalsV2; 2],
    pub attackers_declared: bool,
    pub blockers_declared: bool,
    pub engine: FlatEngineGlobalsV2,
    pub surface: FlatSurfaceGlobalsV2,
    pub policy_surface: FlatPolicySurfaceGlobalsV2,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct FlatObjectCoreV2 {
    pub card_token: u32,
    pub group: FlatObjectGroupV2,
    pub source_kind: FlatObjectSourceKindV2,
    pub visible_ordinal: u32,
    pub owner: FlatRelativePlayerV2,
    pub controller: FlatRelativePlayerV2,
    pub zone: Option<FlatZoneV2>,
    pub card_details_present: bool,
    pub tapped: bool,
    pub summoning_sick: bool,
    pub damage: u16,
    pub counters: [i16; 5],
    pub plotted_turn: FlatTurnRelationV2,
    pub is_token: bool,
    pub face_index: u8,
    pub chosen_color: Option<FlatManaColorV2>,
    pub entered_battlefield_turn: FlatTurnRelationV2,
    pub skip_next_untap: bool,
    pub type_flags: [bool; 6],
    pub base_power: Option<i32>,
    pub base_toughness: Option<i32>,
    pub effective_power: Option<i32>,
    pub effective_toughness: Option<i32>,
    pub effective_color_mask: u8,
    pub keyword_flags: [bool; 14],
    pub ward_generic: u16,
    pub minimum_blockers: u8,
    pub landwalk_mask: u8,
    pub subtype_start: u32,
    pub subtype_count: u32,
    pub ability_use_start: u32,
    pub ability_use_count: u32,
    pub goad_start: u32,
    pub goad_count: u32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct FlatObjectSubtypeV2 {
    pub object_index: u32,
    pub order: u32,
    pub subtype_id: u16,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct FlatObjectAbilityUseV2 {
    pub object_index: u32,
    pub order: u32,
    pub ability_kind: u8,
    pub ability_index: u16,
    pub uses: u16,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct FlatObjectGoadV2 {
    pub object_index: u32,
    pub order: u32,
    pub player: FlatRelativePlayerV2,
    pub expires_after_turns: u32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct FlatCompletedDungeonV2 {
    pub player: FlatRelativePlayerV2,
    pub order: u32,
    pub dungeon_id: u16,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct FlatEffectSubtypeChangeV2 {
    pub effect_order: u32,
    pub kind: FlatEffectSubtypeChangeKindV2,
    pub order: u32,
    pub subtype_id: u16,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct FlatContextPathElementV2 {
    pub context: FlatContextKindV2,
    pub context_order: u32,
    pub kind: FlatContextElementKindV2,
    pub order: u32,
    pub value: u16,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct FlatStackRelationDataV2 {
    pub controller: FlatRelativePlayerV2,
    pub stack_item_kind: u8,
    pub is_copy: bool,
    pub is_flashback: bool,
    pub mode_chosen: u8,
    pub madness_offer: bool,
    pub kicked: bool,
    pub cast_method: u8,
    pub face_index: u8,
    pub x_value: u16,
    pub target_kind: FlatTargetKindV2,
    pub target_player: FlatRelativePlayerV2,
    /// Announcement-time controller provenance for an object target. This is
    /// intentionally independent of the current controller on the resolved
    /// live object row because control can change without a zone change.
    pub target_object_controller: FlatRelativePlayerV2,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct FlatEffectRelationDataV2 {
    pub controller: FlatRelativePlayerV2,
    pub affected_player: FlatRelativePlayerV2,
    pub global: bool,
    pub layers: u8,
    pub duration: u8,
    pub power_delta: i32,
    pub toughness_delta: i32,
    pub grants_haste: bool,
    pub set_power: Option<i32>,
    pub set_toughness: Option<i32>,
    pub add_color_mask: u8,
    pub remove_color_mask: u8,
    pub add_keyword_mask: u32,
    pub remove_keyword_mask: u32,
    pub ward_generic_delta: i16,
    pub minimum_blockers: Option<u8>,
    pub add_landwalk_mask: u8,
    pub remove_landwalk_mask: u8,
    pub prevent_damage_from_color_mask: u8,
    pub damage_cannot_be_prevented: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct FlatPermissionRelationDataV2 {
    pub holder: FlatRelativePlayerV2,
    pub play_or_cast: u8,
    pub expiry: u8,
    pub holder_turn_started: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct FlatContextRelationDataV2 {
    pub context: FlatContextKindV2,
    pub subrole: FlatContextSubroleV2,
    pub target_kind: FlatTargetKindV2,
    pub target_player: FlatRelativePlayerV2,
    pub controller: FlatRelativePlayerV2,
    pub trigger_kind: u8,
    pub kicked: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum FlatRelationPayloadV2 {
    #[default]
    None,
    Stack(FlatStackRelationDataV2),
    CombatAttacker {
        /// `None` means absent from `attacker_to_ordered_blockers`; `Some(i)`
        /// means present at exact mapping index `i`, including an empty list.
        blocked_order: Option<u32>,
    },
    Effect(FlatEffectRelationDataV2),
    Permission(FlatPermissionRelationDataV2),
    Context(FlatContextRelationDataV2),
    Known {
        owner: FlatRelativePlayerV2,
    },
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct FlatRelationV2 {
    pub role: FlatRelationRoleV2,
    pub source_object: Option<u32>,
    pub target_object: Option<u32>,
    pub primary_order: u32,
    pub secondary_order: u32,
    pub associated_order: u32,
    pub payload: FlatRelationPayloadV2,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FlatPolicyContractDigestsV2 {
    /// Canonical SHA-256 over every generated enum map and the action-reference
    /// role crosswalk. This is semantic identity, not a source-control hash.
    pub mapping_sha256: [u8; 32],
    /// Canonical SHA-256 over the complete classified feature inventory. The
    /// inventory transitively binds the Python feature/encoding contracts and
    /// their authoritative source digest.
    pub feature_inventory_sha256: [u8; 32],
    /// Frozen V1 typed-layout source digest reused by the V2 producer.
    pub base_typed_layout_sha256: [u8; 32],
    /// Byte-for-byte SHA-256 over this explicit V2 typed-layout source file.
    pub overlay_typed_layout_sha256: [u8; 32],
    /// Domain-separated SHA-256 over base and overlay layout digests.
    pub typed_layout_sha256: [u8; 32],
    /// Raw-file SHA-256 over the dedicated narrow V2 action-contract source.
    pub action_contract_source_sha256: [u8; 32],
    /// Canonical-JSON SHA-256 over the V2 action-contract semantics.
    pub action_contract_sha256: [u8; 32],
}

pub const FLAT_POLICY_CONTRACT_DIGESTS_V2: FlatPolicyContractDigestsV2 =
    FlatPolicyContractDigestsV2 {
        mapping_sha256: FLAT_POLICY_MAPPING_SHA256_V2,
        feature_inventory_sha256: FLAT_POLICY_FEATURE_INVENTORY_SHA256_V2,
        base_typed_layout_sha256: FLAT_POLICY_BASE_TYPED_LAYOUT_SHA256_V2,
        overlay_typed_layout_sha256: FLAT_POLICY_OVERLAY_TYPED_LAYOUT_SHA256_V2,
        typed_layout_sha256: FLAT_POLICY_TYPED_LAYOUT_SHA256_V2,
        action_contract_source_sha256: FLAT_ACTION_CONTRACT_SOURCE_SHA256_V2,
        action_contract_sha256: FLAT_ACTION_CONTRACT_SEMANTIC_SHA256_V2,
    };

/// Converts the Rust action-slice's compact eight-role vocabulary into the
/// Python feature projection's ten-role vocabulary.
///
/// The internal map has no plural `attackers` or `blockers` rows. Those remain
/// projection-only ids 7 and 8, so internal `PendingSources` 7 maps to
/// projection `pending_sources` 9. The exhaustive match deliberately makes a
/// new Rust role a compile-time update point rather than silently reusing its
/// discriminant.
pub const fn flat_action_ref_projection_role_id_v2(role: FlatActionRefRoleV1) -> u8 {
    match role {
        FlatActionRefRoleV1::Source => FLAT_ACTION_REF_INTERNAL_TO_PROJECTION_V2[0],
        FlatActionRefRoleV1::Candidate => FLAT_ACTION_REF_INTERNAL_TO_PROJECTION_V2[1],
        FlatActionRefRoleV1::Card => FLAT_ACTION_REF_INTERNAL_TO_PROJECTION_V2[2],
        FlatActionRefRoleV1::Attacker => FLAT_ACTION_REF_INTERNAL_TO_PROJECTION_V2[3],
        FlatActionRefRoleV1::Blocker => FLAT_ACTION_REF_INTERNAL_TO_PROJECTION_V2[4],
        FlatActionRefRoleV1::TargetObject => FLAT_ACTION_REF_INTERNAL_TO_PROJECTION_V2[5],
        FlatActionRefRoleV1::Cards => FLAT_ACTION_REF_INTERNAL_TO_PROJECTION_V2[6],
        FlatActionRefRoleV1::PendingSources => FLAT_ACTION_REF_INTERNAL_TO_PROJECTION_V2[7],
    }
}

/// Model-visible action reference for the scored-policy boundary.
///
/// Unlike [`FlatActionRefV2`], `model_object_index` addresses the typed
/// [`FlatObjectCoreV2`] table. The operational action-object table and its
/// zone-incarnation counters never cross this boundary.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct FlatScorerActionRefV2 {
    pub action_index: u32,
    pub projection_role_id: u8,
    pub order_index: u16,
    pub associated_order: u16,
    pub card_token: u32,
    pub model_object_index: u32,
}

/// Digest-covered scorer vocabulary projected from the operational action
/// slice. The exhaustive conversion below makes every internal action-kind
/// addition a compile-time scorer-contract update point.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum FlatScorerActionKindV2 {
    #[default]
    Pass = 0,
    PlayLand = 1,
    CastSpell = 2,
    ActivateManaAbility = 3,
    ActivateAbility = 4,
    PlotSpell = 5,
    ChooseTarget = 6,
    ChooseCostTarget = 7,
    ChooseCastMode = 8,
    ChooseKicker = 9,
    ChooseSpellMode = 10,
    ChooseEffectOption = 11,
    ChooseEffectTarget = 12,
    FinishEffectSelection = 13,
    ChooseEffectColor = 14,
    ChooseEffectNumber = 15,
    ChooseEffectBoolean = 16,
    FinishTargetSelection = 17,
    ChooseOptionalCostUse = 18,
    ChooseOptionalCostWhich = 19,
    ChooseSpellCopyPayment = 20,
    ChooseSpellCopyRetarget = 21,
    ChooseMadnessCast = 22,
    Discard = 23,
    ChooseAttackerInclusion = 24,
    ChooseBlockerInclusion = 25,
    OrderTriggers = 26,
}

impl From<FlatActionKindV1> for FlatScorerActionKindV2 {
    fn from(kind: FlatActionKindV1) -> Self {
        match kind {
            FlatActionKindV1::Pass => Self::Pass,
            FlatActionKindV1::PlayLand => Self::PlayLand,
            FlatActionKindV1::CastSpell => Self::CastSpell,
            FlatActionKindV1::ActivateManaAbility => Self::ActivateManaAbility,
            FlatActionKindV1::ActivateAbility => Self::ActivateAbility,
            FlatActionKindV1::PlotSpell => Self::PlotSpell,
            FlatActionKindV1::ChooseTarget => Self::ChooseTarget,
            FlatActionKindV1::ChooseCostTarget => Self::ChooseCostTarget,
            FlatActionKindV1::ChooseCastMode => Self::ChooseCastMode,
            FlatActionKindV1::ChooseKicker => Self::ChooseKicker,
            FlatActionKindV1::ChooseSpellMode => Self::ChooseSpellMode,
            FlatActionKindV1::ChooseEffectOption => Self::ChooseEffectOption,
            FlatActionKindV1::ChooseEffectTarget => Self::ChooseEffectTarget,
            FlatActionKindV1::FinishEffectSelection => Self::FinishEffectSelection,
            FlatActionKindV1::ChooseEffectColor => Self::ChooseEffectColor,
            FlatActionKindV1::ChooseEffectNumber => Self::ChooseEffectNumber,
            FlatActionKindV1::ChooseEffectBoolean => Self::ChooseEffectBoolean,
            FlatActionKindV1::FinishTargetSelection => Self::FinishTargetSelection,
            FlatActionKindV1::ChooseOptionalCostUse => Self::ChooseOptionalCostUse,
            FlatActionKindV1::ChooseOptionalCostWhich => Self::ChooseOptionalCostWhich,
            FlatActionKindV1::ChooseSpellCopyPayment => Self::ChooseSpellCopyPayment,
            FlatActionKindV1::ChooseSpellCopyRetarget => Self::ChooseSpellCopyRetarget,
            FlatActionKindV1::ChooseMadnessCast => Self::ChooseMadnessCast,
            FlatActionKindV1::Discard => Self::Discard,
            FlatActionKindV1::ChooseAttackerInclusion => Self::ChooseAttackerInclusion,
            FlatActionKindV1::ChooseBlockerInclusion => Self::ChooseBlockerInclusion,
            FlatActionKindV1::OrderTriggers => Self::OrderTriggers,
        }
    }
}

/// Complete model-visible scalar action row. This deliberately mirrors rather
/// than aliases [`FlatActionCoreV1`], so the typed-layout digest closes every
/// field that can cross the scorer boundary.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct FlatScorerActionCoreV2 {
    pub kind: FlatScorerActionKindV2,
    pub flags: u16,
    pub ability_index: u8,
    pub remaining: u8,
    pub mode_index: u8,
    pub mode_count: u8,
    pub option_index: u16,
    pub option_count: u16,
    pub selected_count: u16,
    pub min_targets: u16,
    pub max_targets: u16,
    pub number: i32,
    pub minimum: i32,
    pub maximum: i32,
    pub mana_choice: u8,
    pub color: u8,
    pub cast_mode: u8,
    pub cost_kind: u8,
    pub optional_cost_choice: u8,
    pub target_kind: u8,
    pub target_player: u8,
    pub ref_start: u32,
    pub ref_len: u16,
}

impl From<FlatActionCoreV1> for FlatScorerActionCoreV2 {
    fn from(action: FlatActionCoreV1) -> Self {
        let FlatActionCoreV1 {
            kind,
            flags,
            ability_index,
            remaining,
            mode_index,
            mode_count,
            option_index,
            option_count,
            selected_count,
            min_targets,
            max_targets,
            number,
            minimum,
            maximum,
            mana_choice,
            color,
            cast_mode,
            cost_kind,
            optional_cost_choice,
            target_kind,
            target_player,
            ref_start,
            ref_len,
        } = action;
        Self {
            kind: kind.into(),
            flags,
            ability_index,
            remaining,
            mode_index,
            mode_count,
            option_index,
            option_count,
            selected_count,
            min_targets,
            max_targets,
            number,
            minimum,
            maximum,
            mana_choice,
            color,
            cast_mode,
            cost_kind,
            optional_cost_choice,
            target_kind,
            target_player,
            ref_start,
            ref_len,
        }
    }
}

/// Complete model-visible surface of one scored decision packet.
///
/// The private fields and explicit constructor make additions compile-time
/// review points. In particular this view cannot carry a decision binding,
/// authority-bearing action object, seed, zone-change counter, or candidate
/// commitment. [`FLAT_SCORER_VISIBLE_MANIFEST_V2`] names these accessors in
/// their canonical order, and the typed-layout source digest binds both the
/// manifest and this implementation.
#[derive(Clone, Copy)]
pub struct FlatScoringDecisionViewV2<'a> {
    globals: &'a FlatGlobalsV2,
    objects: &'a [FlatObjectCoreV2],
    relations: &'a [FlatRelationV2],
    object_subtypes: &'a [FlatObjectSubtypeV2],
    ability_uses: &'a [FlatObjectAbilityUseV2],
    goads: &'a [FlatObjectGoadV2],
    completed_dungeons: &'a [FlatCompletedDungeonV2],
    effect_subtype_changes: &'a [FlatEffectSubtypeChangeV2],
    context_path_elements: &'a [FlatContextPathElementV2],
    actions: &'a [FlatScorerActionCoreV2],
    action_refs: &'a [FlatScorerActionRefV2],
}

impl<'a> FlatScoringDecisionViewV2<'a> {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        globals: &'a FlatGlobalsV2,
        objects: &'a [FlatObjectCoreV2],
        relations: &'a [FlatRelationV2],
        object_subtypes: &'a [FlatObjectSubtypeV2],
        ability_uses: &'a [FlatObjectAbilityUseV2],
        goads: &'a [FlatObjectGoadV2],
        completed_dungeons: &'a [FlatCompletedDungeonV2],
        effect_subtype_changes: &'a [FlatEffectSubtypeChangeV2],
        context_path_elements: &'a [FlatContextPathElementV2],
        actions: &'a [FlatScorerActionCoreV2],
        action_refs: &'a [FlatScorerActionRefV2],
    ) -> Self {
        Self {
            globals,
            objects,
            relations,
            object_subtypes,
            ability_uses,
            goads,
            completed_dungeons,
            effect_subtype_changes,
            context_path_elements,
            actions,
            action_refs,
        }
    }

    pub fn globals(self) -> &'a FlatGlobalsV2 {
        self.globals
    }

    pub fn objects(self) -> &'a [FlatObjectCoreV2] {
        self.objects
    }

    pub fn relations(self) -> &'a [FlatRelationV2] {
        self.relations
    }

    pub fn object_subtypes(self) -> &'a [FlatObjectSubtypeV2] {
        self.object_subtypes
    }

    pub fn ability_uses(self) -> &'a [FlatObjectAbilityUseV2] {
        self.ability_uses
    }

    pub fn goads(self) -> &'a [FlatObjectGoadV2] {
        self.goads
    }

    pub fn completed_dungeons(self) -> &'a [FlatCompletedDungeonV2] {
        self.completed_dungeons
    }

    pub fn effect_subtype_changes(self) -> &'a [FlatEffectSubtypeChangeV2] {
        self.effect_subtype_changes
    }

    pub fn context_path_elements(self) -> &'a [FlatContextPathElementV2] {
        self.context_path_elements
    }

    pub fn actions(self) -> &'a [FlatScorerActionCoreV2] {
        self.actions
    }

    pub fn action_refs(self) -> &'a [FlatScorerActionRefV2] {
        self.action_refs
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FlatDecisionBindingV2 {
    pub action_binding: FlatActionDecisionBindingV2,
    pub typed_layout_version: u32,
    pub feature_inventory_version: u32,
    pub enum_mapping_version: u32,
    pub object_group_mapping_version: u32,
    pub relation_role_mapping_version: u32,
    pub context_subrole_mapping_version: u32,
    pub action_ref_projection_role_mapping_version: u32,
    pub contract_digests: FlatPolicyContractDigestsV2,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FlatDecisionV2 {
    pub binding: FlatDecisionBindingV2,
    pub globals: FlatGlobalsV2,
    pub active_object_count: u32,
    pub active_relation_count: u32,
    pub active_object_subtype_count: u32,
    pub active_ability_use_count: u32,
    pub active_goad_count: u32,
    pub active_completed_dungeon_count: u32,
    pub active_effect_subtype_change_count: u32,
    pub active_context_path_element_count: u32,
    pub active_action_count: u32,
    pub active_action_ref_count: u32,
    pub active_action_object_count: u32,
}

pub struct FlatDecisionBuffersV2<'a> {
    pub objects: &'a mut [FlatObjectCoreV2],
    pub relations: &'a mut [FlatRelationV2],
    pub object_subtypes: &'a mut [FlatObjectSubtypeV2],
    pub ability_uses: &'a mut [FlatObjectAbilityUseV2],
    pub goads: &'a mut [FlatObjectGoadV2],
    pub completed_dungeons: &'a mut [FlatCompletedDungeonV2],
    pub effect_subtype_changes: &'a mut [FlatEffectSubtypeChangeV2],
    pub context_path_elements: &'a mut [FlatContextPathElementV2],
    pub actions: &'a mut [FlatActionCoreV1],
    pub action_refs: &'a mut [FlatActionRefV2],
    /// Operational/binding-only PR27 table.  `FlatActionRefV2::object_index`
    /// indexes this table and never the model-visible `objects` table.
    pub action_objects: &'a mut [FlatActionObjectV2],
}

/// Crate-private ownership bridge for the scored rollout. Unlike
/// [`FlatDecisionBuffersV2`], these destinations are swapped with the
/// encoder's already-validated scorer-safe tables instead of copied.
pub(crate) struct FlatScoringOwnedBuffersV2<'a> {
    pub objects: &'a mut Vec<FlatObjectCoreV2>,
    pub relations: &'a mut Vec<FlatRelationV2>,
    pub object_subtypes: &'a mut Vec<FlatObjectSubtypeV2>,
    pub ability_uses: &'a mut Vec<FlatObjectAbilityUseV2>,
    pub goads: &'a mut Vec<FlatObjectGoadV2>,
    pub completed_dungeons: &'a mut Vec<FlatCompletedDungeonV2>,
    pub effect_subtype_changes: &'a mut Vec<FlatEffectSubtypeChangeV2>,
    pub context_path_elements: &'a mut Vec<FlatContextPathElementV2>,
    pub actions: &'a mut Vec<FlatScorerActionCoreV2>,
    pub action_refs: &'a mut Vec<FlatScorerActionRefV2>,
}

/// Test-only ownership bridge for declared synthetic full-tensor fixtures.
///
/// This deliberately reuses the production observation encoder instead of
/// hand-maintaining a second flat-table construction in the fixture emitter.
#[cfg(test)]
pub(crate) struct FlatObservationOwnedTablesV2 {
    pub globals: FlatGlobalsV2,
    pub objects: Vec<FlatObjectCoreV2>,
    pub relations: Vec<FlatRelationV2>,
    pub object_subtypes: Vec<FlatObjectSubtypeV2>,
    pub ability_uses: Vec<FlatObjectAbilityUseV2>,
    pub goads: Vec<FlatObjectGoadV2>,
    pub completed_dungeons: Vec<FlatCompletedDungeonV2>,
    pub effect_subtype_changes: Vec<FlatEffectSubtypeChangeV2>,
    pub context_path_elements: Vec<FlatContextPathElementV2>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlatDecisionErrorV2 {
    Action(FlatActionDecisionSliceErrorV1),
    ObservationContract,
    InvalidReference,
    InconsistentReference,
    ScorerBindingMismatch,
    FutureTurnRelation,
    CheckedIntegerRange,
    InsufficientObjectCapacity { required: usize, available: usize },
    InsufficientRelationCapacity { required: usize, available: usize },
    InsufficientObjectSubtypeCapacity { required: usize, available: usize },
    InsufficientAbilityUseCapacity { required: usize, available: usize },
    InsufficientGoadCapacity { required: usize, available: usize },
    InsufficientCompletedDungeonCapacity { required: usize, available: usize },
    InsufficientEffectSubtypeCapacity { required: usize, available: usize },
    InsufficientContextPathCapacity { required: usize, available: usize },
    InsufficientActionCapacity { required: usize, available: usize },
    InsufficientActionRefCapacity { required: usize, available: usize },
    InsufficientActionObjectCapacity { required: usize, available: usize },
}

impl From<FlatActionDecisionSliceErrorV1> for FlatDecisionErrorV2 {
    fn from(value: FlatActionDecisionSliceErrorV1) -> Self {
        Self::Action(value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PrivateObjectKeyV2 {
    arena_id: u32,
    zone_change_count: u32,
    card_token: u32,
    owner: FlatRelativePlayerV2,
    controller: FlatRelativePlayerV2,
    zone: FlatZoneV2,
    historical_kind: u8,
}

#[derive(Default)]
pub struct FlatDecisionEncoderV2 {
    cached_binding: Option<FlatActionDecisionBindingV2>,
    globals: FlatGlobalsV2,
    objects: Vec<FlatObjectCoreV2>,
    object_keys: Vec<Option<PrivateObjectKeyV2>>,
    relations: Vec<FlatRelationV2>,
    object_subtypes: Vec<FlatObjectSubtypeV2>,
    ability_uses: Vec<FlatObjectAbilityUseV2>,
    goads: Vec<FlatObjectGoadV2>,
    completed_dungeons: Vec<FlatCompletedDungeonV2>,
    effect_subtype_changes: Vec<FlatEffectSubtypeChangeV2>,
    context_path_elements: Vec<FlatContextPathElementV2>,
    actions: Vec<FlatActionCoreV1>,
    action_refs: Vec<FlatActionRefV2>,
    action_objects: Vec<FlatActionObjectV2>,
    action_object_to_model_object: Vec<u32>,
    claimed_model_objects: Vec<bool>,
    scorer_actions: Vec<FlatScorerActionCoreV2>,
    scorer_action_refs: Vec<FlatScorerActionRefV2>,
}

fn relative_player(seat: PlayerSeatV1, actor: PlayerSeatV1) -> FlatRelativePlayerV2 {
    if seat == actor {
        FlatRelativePlayerV2::SelfPlayer
    } else {
        FlatRelativePlayerV2::Opponent
    }
}

fn optional_relative_player(
    seat: Option<PlayerSeatV1>,
    actor: PlayerSeatV1,
) -> FlatRelativePlayerV2 {
    seat.map_or(FlatRelativePlayerV2::None, |value| {
        relative_player(value, actor)
    })
}

fn seat_index(seat: PlayerSeatV1) -> usize {
    match seat {
        PlayerSeatV1::P0 => 0,
        PlayerSeatV1::P1 => 1,
    }
}

fn opponent(seat: PlayerSeatV1) -> PlayerSeatV1 {
    match seat {
        PlayerSeatV1::P0 => PlayerSeatV1::P1,
        PlayerSeatV1::P1 => PlayerSeatV1::P0,
    }
}

fn flat_zone(zone: Zone) -> FlatZoneV2 {
    match zone {
        Zone::Library => FlatZoneV2::Library,
        Zone::Hand => FlatZoneV2::Hand,
        Zone::Battlefield => FlatZoneV2::Battlefield,
        Zone::Graveyard => FlatZoneV2::Graveyard,
        Zone::Stack => FlatZoneV2::Stack,
        Zone::Exile => FlatZoneV2::Exile,
        Zone::Command => FlatZoneV2::Command,
    }
}

fn flat_phase(phase: ZoneIndependentStepV1) -> FlatPhaseV2 {
    match phase {
        ZoneIndependentStepV1::Untap => FlatPhaseV2::Untap,
        ZoneIndependentStepV1::Upkeep => FlatPhaseV2::Upkeep,
        ZoneIndependentStepV1::Draw => FlatPhaseV2::Draw,
        ZoneIndependentStepV1::Main1 => FlatPhaseV2::Main1,
        ZoneIndependentStepV1::BeginCombat => FlatPhaseV2::BeginCombat,
        ZoneIndependentStepV1::DeclareAttackers => FlatPhaseV2::DeclareAttackers,
        ZoneIndependentStepV1::DeclareBlockers => FlatPhaseV2::DeclareBlockers,
        ZoneIndependentStepV1::CombatDamage => FlatPhaseV2::CombatDamage,
        ZoneIndependentStepV1::EndCombat => FlatPhaseV2::EndCombat,
        ZoneIndependentStepV1::Main2 => FlatPhaseV2::Main2,
        ZoneIndependentStepV1::End => FlatPhaseV2::End,
        ZoneIndependentStepV1::Cleanup => FlatPhaseV2::Cleanup,
    }
}

fn flat_color(color: ManaColor) -> FlatManaColorV2 {
    match color {
        ManaColor::W => FlatManaColorV2::White,
        ManaColor::U => FlatManaColorV2::Blue,
        ManaColor::B => FlatManaColorV2::Black,
        ManaColor::R => FlatManaColorV2::Red,
        ManaColor::G => FlatManaColorV2::Green,
        ManaColor::C => FlatManaColorV2::Colorless,
    }
}

fn turn_relation(
    value: Option<u32>,
    current_turn: u32,
) -> Result<FlatTurnRelationV2, FlatDecisionErrorV2> {
    match value {
        None => Ok(FlatTurnRelationV2::Absent),
        Some(value) if value == current_turn => Ok(FlatTurnRelationV2::ThisTurn),
        Some(value) if value < current_turn => Ok(FlatTurnRelationV2::EarlierTurn),
        Some(_) => Err(FlatDecisionErrorV2::FutureTurnRelation),
    }
}

fn card_token(card_db_id: u16) -> u32 {
    u32::from(card_db_id) + 1
}

fn canonical_json_u16_lexical_key(value: u16) -> [u8; 5] {
    let mut reversed = [0_u8; 5];
    let mut value = value;
    let mut length = 0;
    loop {
        reversed[length] = b'0' + u8::try_from(value % 10).expect("one decimal digit fits u8");
        length += 1;
        value /= 10;
        if value == 0 {
            break;
        }
    }
    let mut lexical = [0_u8; 5];
    for index in 0..length {
        lexical[index] = reversed[length - index - 1];
    }
    lexical
}

fn canonical_relative_player_string_order(seat: PlayerSeatV1, actor: PlayerSeatV1) -> u8 {
    match relative_player(seat, actor) {
        // Python sorts canonical JSON strings, where "opponent" precedes
        // "self". `None` cannot occur in a stable card reference.
        FlatRelativePlayerV2::Opponent => 0,
        FlatRelativePlayerV2::SelfPlayer => 1,
        FlatRelativePlayerV2::None => 2,
    }
}

fn canonical_zone_string_order(zone: Zone) -> u8 {
    match zone {
        Zone::Battlefield => 0,
        Zone::Command => 1,
        Zone::Exile => 2,
        Zone::Graveyard => 3,
        Zone::Hand => 4,
        Zone::Library => 5,
        Zone::Stack => 6,
    }
}

fn canonical_known_hand_cards(cards: &[CardPrivateV1], actor: PlayerSeatV1) -> Vec<&CardPrivateV1> {
    let mut canonical: Vec<_> = cards.iter().collect();
    canonical.sort_by_key(|card| {
        (
            canonical_json_u16_lexical_key(card.stable.card_db_id),
            canonical_relative_player_string_order(card.stable.controller, actor),
            canonical_relative_player_string_order(card.stable.owner, actor),
            canonical_zone_string_order(card.stable.zone),
        )
    });
    canonical
}

fn cast_mode_id(value: CastMode) -> u8 {
    match value {
        CastMode::Normal => 1,
        CastMode::Alternative => 2,
    }
}

fn cast_method_id(value: CastMethodV4) -> u8 {
    match value {
        CastMethodV4::Normal => 1,
        CastMethodV4::Alternative => 2,
        CastMethodV4::Flashback => 3,
        CastMethodV4::Madness => 4,
        CastMethodV4::Plotted => 5,
        CastMethodV4::Escape => 6,
        CastMethodV4::Bestow => 7,
        CastMethodV4::Omen => 8,
    }
}

fn engine_stage_id(value: EngineDecisionStageV2) -> u8 {
    match value {
        EngineDecisionStageV2::Priority => 0,
        EngineDecisionStageV2::PendingCast => 1,
        EngineDecisionStageV2::PendingActivation => 2,
        EngineDecisionStageV2::PendingDiscard => 3,
        EngineDecisionStageV2::PendingOptionalCost => 4,
        EngineDecisionStageV2::PendingOptionalCostSacrifice => 5,
        EngineDecisionStageV2::PendingSpellCopy => 6,
        EngineDecisionStageV2::PendingEffect => 7,
        EngineDecisionStageV2::PendingTriggers => 8,
        EngineDecisionStageV2::Halted => 9,
    }
}

fn surface_stage_id(value: SurfaceDecisionStageV2) -> u8 {
    match value {
        SurfaceDecisionStageV2::Priority => 0,
        SurfaceDecisionStageV2::DeclareBlockersForAttacker => 1,
        SurfaceDecisionStageV2::DiscardPick => 2,
        SurfaceDecisionStageV2::OptionalCostUse => 3,
        SurfaceDecisionStageV2::OptionalCostWhich => 4,
    }
}

fn policy_stage_id(value: PolicySurfaceStageV5) -> u8 {
    match value {
        PolicySurfaceStageV5::Surface => 0,
        PolicySurfaceStageV5::AttackerInclusion => 1,
        PolicySurfaceStageV5::BlockerInclusion => 2,
    }
}

fn discard_resume_id(value: DiscardResumeSemanticV2) -> u8 {
    match value {
        DiscardResumeSemanticV2::None => 0,
        DiscardResumeSemanticV2::FinishCast => 1,
        DiscardResumeSemanticV2::FinishActivation => 2,
        DiscardResumeSemanticV2::FinishSpellResolution => 3,
        DiscardResumeSemanticV2::FinishOptionalCost => 4,
    }
}

fn spell_copy_stage_id(value: SpellCopyStageV2) -> u8 {
    match value {
        SpellCopyStageV2::Payment => 0,
        SpellCopyStageV2::Retarget => 1,
        SpellCopyStageV2::Target => 2,
    }
}

fn stack_kind_id(value: StackItemKindV2) -> u8 {
    match value {
        StackItemKindV2::Spell => 0,
        StackItemKindV2::ActivatedAbility => 1,
        StackItemKindV2::TriggeredAbility => 2,
        StackItemKindV2::MadnessOffer => 3,
    }
}

fn effect_duration_id(value: EffectDurationV2) -> u8 {
    match value {
        EffectDurationV2::EndOfTurn => 0,
        EffectDurationV2::UntilControllersNextTurn => 1,
        EffectDurationV2::WhileAttached => 2,
        EffectDurationV2::WhileSourcePresent => 3,
    }
}

fn target_purpose_id(value: TargetSelectionPurposeV4) -> u8 {
    match value {
        TargetSelectionPurposeV4::EffectTargets => 0,
        TargetSelectionPurposeV4::CardSelection => 1,
        TargetSelectionPurposeV4::PermanentSelection => 2,
        TargetSelectionPurposeV4::PlayerSelection => 3,
        TargetSelectionPurposeV4::DamageDivision => 4,
        TargetSelectionPurposeV4::CostPayment => 5,
        TargetSelectionPurposeV4::LibraryOrder => 6,
        TargetSelectionPurposeV4::SearchResult => 7,
    }
}

fn boolean_purpose_id(value: BooleanChoicePurposeV4) -> u8 {
    match value {
        BooleanChoicePurposeV4::OptionalEffect => 0,
        BooleanChoicePurposeV4::Shuffle => 1,
        BooleanChoicePurposeV4::PayCost => 2,
    }
}

fn ability_kind_id(value: AbilityKindV4) -> u8 {
    match value {
        AbilityKindV4::Mana => 0,
        AbilityKindV4::Activated => 1,
    }
}

fn target_parts(
    target: &TargetRefV1,
    actor: PlayerSeatV1,
) -> (FlatTargetKindV2, FlatRelativePlayerV2) {
    match target {
        TargetRefV1::Player { player } => {
            (FlatTargetKindV2::Player, relative_player(*player, actor))
        }
        TargetRefV1::Object { .. } => (FlatTargetKindV2::Object, FlatRelativePlayerV2::None),
    }
}

fn usize_u32(value: usize) -> Result<u32, FlatDecisionErrorV2> {
    u32::try_from(value).map_err(|_| FlatDecisionErrorV2::CheckedIntegerRange)
}

fn usize_u64(value: usize) -> Result<u64, FlatDecisionErrorV2> {
    u64::try_from(value).map_err(|_| FlatDecisionErrorV2::CheckedIntegerRange)
}

fn context_object_ordinal(context: FlatContextKindV2, order: u32) -> u32 {
    0x8000_0000 | (u32::from(context as u8) << 16) | order
}

impl FlatDecisionEncoderV2 {
    fn clear_typed_cache(&mut self) {
        self.cached_binding = None;
        self.globals = FlatGlobalsV2::default();
        self.objects.clear();
        self.object_keys.clear();
        self.relations.clear();
        self.object_subtypes.clear();
        self.ability_uses.clear();
        self.goads.clear();
        self.completed_dungeons.clear();
        self.effect_subtype_changes.clear();
        self.context_path_elements.clear();
        self.actions.clear();
        self.action_refs.clear();
        self.action_objects.clear();
        self.action_object_to_model_object.clear();
        self.claimed_model_objects.clear();
        self.scorer_actions.clear();
        self.scorer_action_refs.clear();
    }

    fn private_key(
        stable: &CardStableRefV1,
        actor: PlayerSeatV1,
        historical_kind: u8,
    ) -> PrivateObjectKeyV2 {
        PrivateObjectKeyV2 {
            arena_id: stable.arena_id,
            zone_change_count: stable.zone_change_count,
            card_token: card_token(stable.card_db_id),
            owner: relative_player(stable.owner, actor),
            controller: relative_player(stable.controller, actor),
            zone: flat_zone(stable.zone),
            historical_kind,
        }
    }

    fn resolve_live(
        &self,
        stable: &CardStableRefV1,
        actor: PlayerSeatV1,
    ) -> Result<u32, FlatDecisionErrorV2> {
        let wanted = Self::private_key(stable, actor, 0);
        let mut same_incarnation = false;
        for (index, key) in self.object_keys.iter().enumerate() {
            let Some(key) = key else { continue };
            if key.arena_id == wanted.arena_id
                && key.zone_change_count == wanted.zone_change_count
                && key.historical_kind == 0
            {
                same_incarnation = true;
                if key.card_token == wanted.card_token
                    && key.owner == wanted.owner
                    && key.controller == wanted.controller
                    && key.zone == wanted.zone
                {
                    return usize_u32(index);
                }
            }
        }
        if same_incarnation {
            Err(FlatDecisionErrorV2::InconsistentReference)
        } else {
            Err(FlatDecisionErrorV2::InvalidReference)
        }
    }

    fn resolve_reference(
        &self,
        stable: &CardStableRefV1,
        actor: PlayerSeatV1,
    ) -> Result<u32, FlatDecisionErrorV2> {
        if let Ok(index) = self.resolve_live(stable, actor) {
            return Ok(index);
        }
        let wanted = Self::private_key(stable, actor, 0);
        let mut found = None;
        for (index, key) in self.object_keys.iter().enumerate() {
            let Some(key) = key else { continue };
            if key.arena_id == wanted.arena_id && key.zone_change_count == wanted.zone_change_count
            {
                if key.card_token != wanted.card_token
                    || key.owner != wanted.owner
                    || key.controller != wanted.controller
                    || key.zone != wanted.zone
                {
                    return Err(FlatDecisionErrorV2::InconsistentReference);
                }
                if found.is_none() {
                    found = Some(usize_u32(index)?);
                }
            }
        }
        found.ok_or(FlatDecisionErrorV2::InvalidReference)
    }

    fn resolve_historical_stack_target(
        &self,
        stable: &CardStableRefV1,
        actor: PlayerSeatV1,
    ) -> Result<u32, FlatDecisionErrorV2> {
        let wanted = Self::private_key(stable, actor, HISTORICAL_STACK_TARGET_KIND_V1);
        let mut found = None;
        for (index, key) in self.object_keys.iter().enumerate() {
            let Some(key) = key else { continue };
            if key.arena_id == wanted.arena_id
                && key.zone_change_count == wanted.zone_change_count
                && matches!(key.historical_kind, 0 | HISTORICAL_STACK_TARGET_KIND_V1)
            {
                if key.card_token != wanted.card_token
                    || key.owner != wanted.owner
                    || key.zone != wanted.zone
                {
                    return Err(FlatDecisionErrorV2::InconsistentReference);
                }
                if found.is_none() {
                    found = Some(usize_u32(index)?);
                }
            }
        }
        if !matches!(wanted.zone, FlatZoneV2::Battlefield | FlatZoneV2::Stack) {
            return Err(FlatDecisionErrorV2::InvalidReference);
        }
        found.ok_or(FlatDecisionErrorV2::InvalidReference)
    }

    fn resolve_paid_cost_reference(
        &self,
        stable: &CardStableRefV1,
        actor: PlayerSeatV1,
    ) -> Result<u32, FlatDecisionErrorV2> {
        let wanted = Self::private_key(stable, actor, HISTORICAL_PAID_COST_KIND_V1);
        let mut found = None;
        for (index, key) in self.object_keys.iter().enumerate() {
            let Some(key) = key else { continue };
            if key.arena_id == wanted.arena_id
                && key.zone_change_count == wanted.zone_change_count
                && matches!(key.historical_kind, 0 | HISTORICAL_PAID_COST_KIND_V1)
            {
                if key.card_token != wanted.card_token
                    || key.owner != wanted.owner
                    || key.controller != wanted.controller
                    || key.zone != wanted.zone
                {
                    return Err(FlatDecisionErrorV2::InconsistentReference);
                }
                if found.replace(usize_u32(index)?).is_some() {
                    return Err(FlatDecisionErrorV2::InconsistentReference);
                }
            }
        }
        found.ok_or(FlatDecisionErrorV2::InvalidReference)
    }

    fn add_private_card(
        &mut self,
        card: &CardPrivateV1,
        actor: PlayerSeatV1,
        group: FlatObjectGroupV2,
        source_kind: FlatObjectSourceKindV2,
        ordinal: u32,
        historical_kind: u8,
    ) -> Result<u32, FlatDecisionErrorV2> {
        self.add_stable(
            &card.stable,
            actor,
            group,
            source_kind,
            ordinal,
            historical_kind,
        )
    }

    fn add_stable(
        &mut self,
        stable: &CardStableRefV1,
        actor: PlayerSeatV1,
        group: FlatObjectGroupV2,
        source_kind: FlatObjectSourceKindV2,
        ordinal: u32,
        historical_kind: u8,
    ) -> Result<u32, FlatDecisionErrorV2> {
        let wanted = Self::private_key(stable, actor, historical_kind);
        for (index, key) in self.object_keys.iter().enumerate() {
            let Some(key) = key else { continue };
            if key.arena_id == wanted.arena_id && key.zone_change_count == wanted.zone_change_count
            {
                if key.card_token != wanted.card_token
                    || key.owner != wanted.owner
                    || key.zone != wanted.zone
                {
                    return Err(FlatDecisionErrorV2::InconsistentReference);
                }
                if key.historical_kind != historical_kind
                    && key.historical_kind != 0
                    && historical_kind != 0
                {
                    continue;
                }
                if historical_kind != HISTORICAL_STACK_TARGET_KIND_V1
                    && key.controller != wanted.controller
                {
                    return Err(FlatDecisionErrorV2::InconsistentReference);
                }
                return usize_u32(index);
            }
        }
        if historical_kind == HISTORICAL_STACK_TARGET_KIND_V1
            && !matches!(wanted.zone, FlatZoneV2::Battlefield | FlatZoneV2::Stack)
        {
            return Err(FlatDecisionErrorV2::InvalidReference);
        }
        let index = usize_u32(self.objects.len())?;
        self.objects.push(FlatObjectCoreV2 {
            card_token: wanted.card_token,
            group,
            source_kind,
            visible_ordinal: ordinal,
            owner: wanted.owner,
            controller: wanted.controller,
            zone: Some(wanted.zone),
            ..FlatObjectCoreV2::default()
        });
        self.object_keys.push(Some(wanted));
        Ok(index)
    }

    fn add_public_card(
        &mut self,
        card: &CardPublicV2,
        actor: PlayerSeatV1,
        group: FlatObjectGroupV2,
        ordinal: u32,
        current_turn: u32,
    ) -> Result<u32, FlatDecisionErrorV2> {
        let index = self.add_stable(
            &card.stable,
            actor,
            group,
            FlatObjectSourceKindV2::Card,
            ordinal,
            0,
        )?;
        let object = self
            .objects
            .get_mut(usize::try_from(index).map_err(|_| FlatDecisionErrorV2::CheckedIntegerRange)?)
            .ok_or(FlatDecisionErrorV2::InvalidReference)?;
        if object.card_details_present {
            return Ok(index);
        }
        let characteristics = &card.characteristics;
        let keywords = &characteristics.effective_keywords;
        let subtype_start = usize_u32(self.object_subtypes.len())?;
        for (order, &subtype_id) in characteristics.effective_subtype_ids.iter().enumerate() {
            self.object_subtypes.push(FlatObjectSubtypeV2 {
                object_index: index,
                order: usize_u32(order)?,
                subtype_id,
            });
        }
        let ability_use_start = usize_u32(self.ability_uses.len())?;
        for (order, ability) in card.ability_uses_this_turn.iter().enumerate() {
            self.ability_uses.push(FlatObjectAbilityUseV2 {
                object_index: index,
                order: usize_u32(order)?,
                ability_kind: ability_kind_id(ability.ability_kind),
                ability_index: ability.ability_index,
                uses: ability.uses,
            });
        }
        let goad_start = usize_u32(self.goads.len())?;
        let mut canonical_goads = card.goaded_by.clone();
        canonical_goads.sort_unstable_by_key(|goad| {
            (
                relative_player(goad.player, actor) as u8,
                goad.expires_at_turn,
            )
        });
        for (order, goad) in canonical_goads.iter().enumerate() {
            let expires_after_turns = goad
                .expires_at_turn
                .checked_sub(current_turn)
                .ok_or(FlatDecisionErrorV2::FutureTurnRelation)?;
            self.goads.push(FlatObjectGoadV2 {
                object_index: index,
                order: usize_u32(order)?,
                player: relative_player(goad.player, actor),
                expires_after_turns,
            });
        }
        *object = FlatObjectCoreV2 {
            card_details_present: true,
            tapped: card.tapped,
            summoning_sick: card.summoning_sick,
            damage: card.damage,
            counters: [
                card.counters.plus1_plus1,
                card.counters.minus1_minus1,
                card.counters.minus0_minus1,
                card.counters.stun,
                card.counters.lore,
            ],
            plotted_turn: turn_relation(card.plotted_turn, current_turn)?,
            is_token: card.is_token,
            face_index: card.face_index,
            chosen_color: card.chosen_color.map(flat_color),
            entered_battlefield_turn: turn_relation(card.entered_battlefield_turn, current_turn)?,
            skip_next_untap: card.skip_next_untap,
            type_flags: [
                characteristics.type_flags.land,
                characteristics.type_flags.creature,
                characteristics.type_flags.instant,
                characteristics.type_flags.sorcery,
                characteristics.type_flags.artifact,
                characteristics.type_flags.enchantment,
            ],
            base_power: characteristics.base_power,
            base_toughness: characteristics.base_toughness,
            effective_power: characteristics.effective_power,
            effective_toughness: characteristics.effective_toughness,
            effective_color_mask: characteristics.effective_color_mask,
            keyword_flags: [
                keywords.flying,
                keywords.reach,
                keywords.haste,
                keywords.vigilance,
                keywords.trample,
                keywords.first_strike,
                keywords.double_strike,
                keywords.deathtouch,
                keywords.menace,
                keywords.defender,
                keywords.lifelink,
                keywords.hexproof,
                keywords.indestructible,
                keywords.protection_from_monocolored,
            ],
            ward_generic: keywords.ward_generic,
            minimum_blockers: keywords.minimum_blockers,
            landwalk_mask: keywords.landwalk_mask,
            subtype_start,
            subtype_count: usize_u32(characteristics.effective_subtype_ids.len())?,
            ability_use_start,
            ability_use_count: usize_u32(card.ability_uses_this_turn.len())?,
            goad_start,
            goad_count: usize_u32(canonical_goads.len())?,
            ..*object
        };
        Ok(index)
    }

    fn add_context_object(
        &mut self,
        group: FlatObjectGroupV2,
        source_kind: FlatObjectSourceKindV2,
        ordinal: u32,
    ) -> Result<u32, FlatDecisionErrorV2> {
        let index = usize_u32(self.objects.len())?;
        self.objects.push(FlatObjectCoreV2 {
            group,
            source_kind,
            visible_ordinal: ordinal,
            owner: FlatRelativePlayerV2::None,
            controller: FlatRelativePlayerV2::None,
            ..FlatObjectCoreV2::default()
        });
        self.object_keys.push(None);
        Ok(index)
    }

    fn append_context_elements(
        &mut self,
        context: FlatContextKindV2,
        context_order: u32,
        kind: FlatContextElementKindV2,
        values: impl IntoIterator<Item = u16>,
    ) -> Result<(u32, u32), FlatDecisionErrorV2> {
        let start = usize_u32(self.context_path_elements.len())?;
        let mut count = 0_u32;
        for value in values {
            self.context_path_elements.push(FlatContextPathElementV2 {
                context,
                context_order,
                kind,
                order: count,
                value,
            });
            count = count
                .checked_add(1)
                .ok_or(FlatDecisionErrorV2::CheckedIntegerRange)?;
        }
        Ok((start, count))
    }

    fn build_globals(&mut self, observation: &ObservationV5) -> Result<(), FlatDecisionErrorV2> {
        let actor = observation.acting_player;
        let p = &observation.projection.surface;
        let seats = [actor, opponent(actor)];
        let mut players = [FlatPlayerGlobalsV2::default(); 2];
        for (relative_index, seat) in seats.into_iter().enumerate() {
            let absolute_index = seat_index(seat);
            let status = &p.player_status[absolute_index];
            let completed_dungeon_start = usize_u32(self.completed_dungeons.len())?;
            for (order, &dungeon_id) in status.dungeon.completed_dungeons.iter().enumerate() {
                self.completed_dungeons.push(FlatCompletedDungeonV2 {
                    player: relative_player(seat, actor),
                    order: usize_u32(order)?,
                    dungeon_id,
                });
            }
            players[relative_index] = FlatPlayerGlobalsV2 {
                life: p.life_totals[absolute_index],
                mana: p.mana_pools[absolute_index],
                hand_count: usize_u64(p.hand_counts[absolute_index])?,
                library_count: usize_u64(p.library_counts[absolute_index])?,
                has_lost: status.has_lost,
                lands_played_this_turn: status.lands_played_this_turn,
                drew_from_empty: status.drew_from_empty,
                draws_this_turn: status.draws_this_turn,
                spells_cast_this_turn: status.spells_cast_this_turn,
                dungeon_id: status.dungeon.dungeon_id,
                room_id: status.dungeon.room_id,
                completed_dungeon_start,
                completed_dungeon_count: usize_u32(status.dungeon.completed_dungeons.len())?,
            };
        }

        let engine = &p.engine_context;
        let pending_cast = engine
            .pending_cast
            .as_ref()
            .map(
                |pending| -> Result<FlatPendingCastGlobalsV2, FlatDecisionErrorV2> {
                    Ok(FlatPendingCastGlobalsV2 {
                        source_present: pending.source.is_some(),
                        controller: relative_player(pending.controller, actor),
                        chosen_target_count: usize_u32(pending.chosen_targets.len())?,
                        is_flashback: pending.is_flashback,
                        cast_mode: pending.cast_mode.map_or(0, cast_mode_id),
                        discarded_present: pending.additional_cost_discarded.is_some(),
                        discarded_count: usize_u32(
                            pending
                                .additional_cost_discarded
                                .as_ref()
                                .map_or(0, Vec::len),
                        )?,
                        mode_chosen: pending.mode_chosen,
                        origin_zone: flat_zone(pending.origin_zone),
                        sacrificed_count: usize_u32(pending.sacrifice_chosen.len())?,
                        kicked: pending.kicked,
                    })
                },
            )
            .transpose()?;
        let pending_activation = engine
            .pending_activation
            .as_ref()
            .map(
                |pending| -> Result<FlatPendingActivationGlobalsV2, FlatDecisionErrorV2> {
                    Ok(FlatPendingActivationGlobalsV2 {
                        source_present: pending.source.is_some(),
                        controller: relative_player(pending.controller, actor),
                        ability_index: pending.ability_index,
                        chosen_target_count: usize_u32(pending.chosen_targets.len())?,
                        discard_paid_present: pending.cost_discard_paid.is_some(),
                        discard_paid_count: usize_u32(
                            pending.cost_discard_paid.as_ref().map_or(0, Vec::len),
                        )?,
                    })
                },
            )
            .transpose()?;
        let pending_discard =
            engine
                .pending_discard
                .as_ref()
                .map(|pending| FlatPendingDiscardGlobalsV2 {
                    player: relative_player(pending.player, actor),
                    count: pending.count,
                    resume_stage: discard_resume_id(pending.resume_stage),
                    resume_source_present: pending.resume_source.is_some(),
                });
        let pending_optional_cost =
            engine
                .pending_optional_cost
                .as_ref()
                .map(|pending| FlatPendingOptionalCostGlobalsV2 {
                    player: relative_player(pending.player, actor),
                    source_present: pending.source.is_some(),
                    discard_cards: pending.discard_cards,
                    sacrifice_lands: pending.sacrifice_lands,
                    discard_payable: pending.discard_payable,
                    sacrifice_payable: pending.sacrifice_payable,
                    spell_resume_source_present: pending.spell_resume_source.is_some(),
                    spell_resume_zone: pending.spell_resume_zone.map(flat_zone),
                });
        let pending_optional_sacrifice = engine
            .pending_optional_cost_sacrifice
            .as_ref()
            .map(
                |pending| -> Result<FlatPendingOptionalSacrificeGlobalsV2, FlatDecisionErrorV2> {
                    Ok(FlatPendingOptionalSacrificeGlobalsV2 {
                        player: relative_player(pending.player, actor),
                        source_present: pending.source.is_some(),
                        remaining: pending.remaining,
                        chosen_count: usize_u32(pending.chosen.len())?,
                        spell_resume_source_present: pending.spell_resume_source.is_some(),
                        spell_resume_zone: pending.spell_resume_zone.map(flat_zone),
                    })
                },
            )
            .transpose()?;
        let pending_spell_copy = engine.pending_spell_copy.as_ref().map(|pending| {
            let (inherited_target_kind, inherited_target_player) =
                target_parts(&pending.inherited_target, actor);
            FlatPendingSpellCopyGlobalsV2 {
                parent_present: pending.parent.is_some(),
                player: relative_player(pending.player, actor),
                inherited_target_kind,
                inherited_target_player,
                stage: spell_copy_stage_id(pending.stage),
                copy_present: pending.copy.is_some(),
            }
        });
        let pending_effect = if let Some(pending) = &engine.pending_effect {
            let choice = match &pending.choice {
                None => None,
                Some(PendingEffectChoiceSemanticV4::Options {
                    player,
                    structural_path,
                    option_count,
                }) => {
                    let (path_start, path_count) = self.append_context_elements(
                        FlatContextKindV2::PendingEffect,
                        0,
                        FlatContextElementKindV2::StructuralPath,
                        structural_path.iter().copied(),
                    )?;
                    Some(FlatPendingEffectChoiceV2::Options {
                        player: relative_player(*player, actor),
                        path_start,
                        path_count,
                        option_count: *option_count,
                    })
                }
                Some(PendingEffectChoiceSemanticV4::Targets {
                    player,
                    structural_path,
                    selected_targets,
                    legal_targets,
                    min_targets,
                    max_targets,
                    can_finish,
                    ordered,
                    purpose,
                }) => {
                    let (path_start, path_count) = self.append_context_elements(
                        FlatContextKindV2::PendingEffect,
                        0,
                        FlatContextElementKindV2::StructuralPath,
                        structural_path.iter().copied(),
                    )?;
                    Some(FlatPendingEffectChoiceV2::Targets {
                        player: relative_player(*player, actor),
                        path_start,
                        path_count,
                        selected_count: usize_u32(selected_targets.len())?,
                        legal_count: usize_u32(legal_targets.len())?,
                        min_targets: *min_targets,
                        max_targets: *max_targets,
                        can_finish: *can_finish,
                        ordered: *ordered,
                        purpose: target_purpose_id(*purpose),
                    })
                }
                Some(PendingEffectChoiceSemanticV4::Color {
                    player,
                    structural_path,
                    legal_colors,
                }) => {
                    let (path_start, path_count) = self.append_context_elements(
                        FlatContextKindV2::PendingEffect,
                        0,
                        FlatContextElementKindV2::StructuralPath,
                        structural_path.iter().copied(),
                    )?;
                    let (legal_color_start, legal_color_count) = self.append_context_elements(
                        FlatContextKindV2::PendingEffect,
                        0,
                        FlatContextElementKindV2::LegalColor,
                        legal_colors
                            .iter()
                            .map(|&color| u16::from(flat_color(color) as u8)),
                    )?;
                    Some(FlatPendingEffectChoiceV2::Color {
                        player: relative_player(*player, actor),
                        path_start,
                        path_count,
                        legal_color_start,
                        legal_color_count,
                    })
                }
                Some(PendingEffectChoiceSemanticV4::Number {
                    player,
                    structural_path,
                    minimum,
                    maximum,
                }) => {
                    let (path_start, path_count) = self.append_context_elements(
                        FlatContextKindV2::PendingEffect,
                        0,
                        FlatContextElementKindV2::StructuralPath,
                        structural_path.iter().copied(),
                    )?;
                    Some(FlatPendingEffectChoiceV2::Number {
                        player: relative_player(*player, actor),
                        path_start,
                        path_count,
                        minimum: *minimum,
                        maximum: *maximum,
                    })
                }
                Some(PendingEffectChoiceSemanticV4::Boolean {
                    player,
                    structural_path,
                    default,
                    purpose,
                }) => {
                    let (path_start, path_count) = self.append_context_elements(
                        FlatContextKindV2::PendingEffect,
                        0,
                        FlatContextElementKindV2::StructuralPath,
                        structural_path.iter().copied(),
                    )?;
                    Some(FlatPendingEffectChoiceV2::Boolean {
                        player: relative_player(*player, actor),
                        path_start,
                        path_count,
                        default: *default,
                        purpose: boolean_purpose_id(*purpose),
                    })
                }
            };
            Some(FlatPendingEffectGlobalsV2 {
                source_present: pending.source.is_some(),
                controller: relative_player(pending.controller, actor),
                choice,
            })
        } else {
            None
        };
        let actor_index = seat_index(actor);
        let opponent_index = seat_index(opponent(actor));
        let surface = &p.surface_context;
        let policy = &observation.projection.policy_surface_context;
        let private_combat = policy.private_combat_selection.as_ref();
        self.globals = FlatGlobalsV2 {
            acting_player: FlatRelativePlayerV2::SelfPlayer,
            phase: flat_phase(p.phase),
            active_player: relative_player(p.active_player, actor),
            priority_player: relative_player(p.priority_player, actor),
            initiative: optional_relative_player(p.initiative, actor),
            players,
            attackers_declared: p.combat.attackers_declared,
            blockers_declared: p.combat.blockers_declared,
            engine: FlatEngineGlobalsV2 {
                priority_passes: [
                    engine.priority_passes[actor_index],
                    engine.priority_passes[opponent_index],
                ],
                stack_nonempty: engine.stack_nonempty,
                stack_activity_since_priority_boundary: engine
                    .stack_activity_since_priority_boundary,
                mana_activity_since_priority_boundary: engine.mana_activity_since_priority_boundary,
                last_mana_ability_activator: optional_relative_player(
                    engine.last_mana_ability_activator_since_priority_boundary,
                    actor,
                ),
                current_stage: engine_stage_id(engine.current_stage),
                pending_cast,
                pending_activation,
                pending_discard,
                pending_optional_cost,
                pending_optional_sacrifice,
                pending_spell_copy,
                pending_effect,
                pending_trigger_count: usize_u32(engine.pending_triggers.len())?,
            },
            surface: FlatSurfaceGlobalsV2 {
                current_stage: surface_stage_id(surface.current_stage),
                combat_priority_spent: [
                    surface.combat_priority_spent[actor_index],
                    surface.combat_priority_spent[opponent_index],
                ],
                combat_priority_rearmed_by_stack_activity: surface
                    .combat_priority_rearmed_by_stack_activity,
                combat_priority_rearmed_by_mana_activity: surface
                    .combat_priority_rearmed_by_mana_activity,
                stack_grew_since_round_open: surface.stack_grew_since_round_open,
                mana_activity_since_round_open: surface.mana_activity_since_round_open,
                stack_length_changed_since_observed: surface.stack_length_changed_since_observed,
                mana_activity_since_last_stack_change: surface
                    .mana_activity_since_last_stack_change,
                madness_cast_reprompt_source_present: surface
                    .madness_cast_reprompt_source
                    .is_some(),
                private_blockers_present: surface.private_blockers.is_some(),
                private_discard_remaining_needed: surface
                    .private_discard
                    .as_ref()
                    .map(|private| private.remaining_needed),
                private_discard_chosen_count: usize_u32(
                    surface
                        .private_discard
                        .as_ref()
                        .map_or(0, |p| p.chosen.len()),
                )?,
                private_discard_remaining_count: usize_u32(
                    surface
                        .private_discard
                        .as_ref()
                        .map_or(0, |p| p.remaining_choices.len()),
                )?,
                private_optional_discard_payable: surface
                    .private_optional_cost
                    .as_ref()
                    .map(|private| private.discard_payable),
                private_optional_sacrifice_payable: surface
                    .private_optional_cost
                    .as_ref()
                    .map(|private| private.sacrifice_payable),
                private_optional_stage: surface
                    .private_optional_cost
                    .as_ref()
                    .map(|private| surface_stage_id(private.stage)),
            },
            policy_surface: FlatPolicySurfaceGlobalsV2 {
                current_stage: policy_stage_id(policy.current_stage),
                private_combat_present: private_combat.is_some(),
                private_combat_attacker_present: private_combat
                    .is_some_and(|private| private.attacker.is_some()),
                candidate_index: private_combat.map_or(0, |private| private.candidate_index),
                candidate_count: private_combat.map_or(0, |private| private.candidate_count),
                selected_count: usize_u32(
                    private_combat.map_or(0, |private| private.selected.len()),
                )?,
                remaining_count: usize_u32(
                    private_combat.map_or(0, |private| private.remaining_after_current.len()),
                )?,
            },
        };
        Ok(())
    }

    fn context_object_index(
        &self,
        group: FlatObjectGroupV2,
        ordinal: u32,
    ) -> Result<u32, FlatDecisionErrorV2> {
        self.objects
            .iter()
            .enumerate()
            .find(|(_, object)| object.group == group && object.visible_ordinal == ordinal)
            .map(|(index, _)| usize_u32(index))
            .transpose()?
            .ok_or(FlatDecisionErrorV2::InvalidReference)
    }

    fn resolve_arena(&self, arena_id: u32) -> Result<u32, FlatDecisionErrorV2> {
        let mut result = None;
        for (index, key) in self.object_keys.iter().enumerate() {
            let Some(key) = key else { continue };
            if key.historical_kind == 0 && key.arena_id == arena_id {
                if result.is_some() {
                    return Err(FlatDecisionErrorV2::InconsistentReference);
                }
                result = Some(usize_u32(index)?);
            }
        }
        result.ok_or(FlatDecisionErrorV2::InvalidReference)
    }

    fn ensure_context_ref(
        &mut self,
        stable: &CardStableRefV1,
        actor: PlayerSeatV1,
        group: FlatObjectGroupV2,
        source_kind: FlatObjectSourceKindV2,
        ordinal: u32,
        allow_detached: bool,
    ) -> Result<u32, FlatDecisionErrorV2> {
        match self.resolve_live(stable, actor) {
            Ok(index) => Ok(index),
            Err(FlatDecisionErrorV2::InvalidReference) if allow_detached => {
                self.add_stable(stable, actor, group, source_kind, ordinal, 0)
            }
            Err(error) => Err(error),
        }
    }

    fn register_objects(&mut self, observation: &ObservationV5) -> Result<(), FlatDecisionErrorV2> {
        let actor = observation.acting_player;
        let opponent = opponent(actor);
        let p = &observation.projection.surface;
        let turn = p.turn;

        for (order, card) in observation.own_hand.iter().enumerate() {
            self.add_private_card(
                card,
                actor,
                FlatObjectGroupV2::SelfHand,
                FlatObjectSourceKindV2::Card,
                usize_u32(order)?,
                0,
            )?;
        }
        for (seat, group) in [
            (actor, FlatObjectGroupV2::SelfBattlefield),
            (opponent, FlatObjectGroupV2::OpponentBattlefield),
        ] {
            for (order, card) in p.battlefield[seat_index(seat)].iter().enumerate() {
                self.add_public_card(card, actor, group, usize_u32(order)?, turn)?;
            }
        }
        for (seat, group) in [
            (actor, FlatObjectGroupV2::SelfGraveyard),
            (opponent, FlatObjectGroupV2::OpponentGraveyard),
        ] {
            for (order, card) in p.graveyards[seat_index(seat)].iter().enumerate() {
                self.add_public_card(card, actor, group, usize_u32(order)?, turn)?;
            }
        }
        for (order, card) in p.exile.iter().enumerate() {
            self.add_public_card(
                card,
                actor,
                FlatObjectGroupV2::Exile,
                usize_u32(order)?,
                turn,
            )?;
        }
        for (order, item) in p.stack.iter().enumerate() {
            self.add_stable(
                &item.source,
                actor,
                FlatObjectGroupV2::Stack,
                FlatObjectSourceKindV2::Stack,
                usize_u32(order)?,
                0,
            )?;
        }
        for order in 0..p.combat.ordered_attackers.len() {
            self.add_context_object(
                FlatObjectGroupV2::Combat,
                FlatObjectSourceKindV2::Combat,
                usize_u32(order)?,
            )?;
        }
        for order in 0..p.continuous_effects.len() {
            self.add_context_object(
                FlatObjectGroupV2::ContinuousEffect,
                FlatObjectSourceKindV2::Effect,
                usize_u32(order)?,
            )?;
        }
        for order in 0..p.exile_play_permissions.len() {
            self.add_context_object(
                FlatObjectGroupV2::Permission,
                FlatObjectSourceKindV2::Permission,
                usize_u32(order)?,
            )?;
        }
        let public_cards = p.battlefield[seat_index(actor)]
            .iter()
            .chain(p.battlefield[seat_index(opponent)].iter())
            .chain(p.graveyards[seat_index(actor)].iter())
            .chain(p.graveyards[seat_index(opponent)].iter())
            .chain(p.exile.iter());
        let mut attachment_context_order = 0_u32;
        for card in public_cards {
            for _ in &card.attachments {
                self.add_context_object(
                    FlatObjectGroupV2::Attachment,
                    FlatObjectSourceKindV2::Attachment,
                    attachment_context_order,
                )?;
                attachment_context_order = attachment_context_order
                    .checked_add(1)
                    .ok_or(FlatDecisionErrorV2::CheckedIntegerRange)?;
            }
        }
        for (stack_order, item) in p.stack.iter().enumerate() {
            for (target_order, target) in item.targets.iter().enumerate() {
                if let TargetRefV1::Object { object } = target {
                    let ordinal = usize_u32(
                        stack_order
                            .checked_mul(65_536)
                            .and_then(|v| v.checked_add(target_order))
                            .ok_or(FlatDecisionErrorV2::CheckedIntegerRange)?,
                    )?;
                    self.add_stable(
                        object,
                        actor,
                        FlatObjectGroupV2::HistoricalStackTarget,
                        FlatObjectSourceKindV2::Target,
                        ordinal,
                        HISTORICAL_STACK_TARGET_KIND_V1,
                    )?;
                }
            }
        }
        let mut combat_block_order = 0_u32;
        for (_, blockers) in &p.combat.attacker_to_ordered_blockers {
            for _ in blockers {
                self.add_context_object(
                    FlatObjectGroupV2::CombatBlock,
                    FlatObjectSourceKindV2::Combat,
                    combat_block_order,
                )?;
                combat_block_order = combat_block_order
                    .checked_add(1)
                    .ok_or(FlatDecisionErrorV2::CheckedIntegerRange)?;
            }
        }

        let mut pending_ordinal = 0_u32;
        let engine = &p.engine_context;
        let mut register_detached =
            |this: &mut Self, value: Option<&CardStableRefV1>| -> Result<(), FlatDecisionErrorV2> {
                if let Some(stable) = value {
                    this.ensure_context_ref(
                        stable,
                        actor,
                        FlatObjectGroupV2::PendingContext,
                        FlatObjectSourceKindV2::Pending,
                        pending_ordinal,
                        true,
                    )?;
                    pending_ordinal = pending_ordinal
                        .checked_add(1)
                        .ok_or(FlatDecisionErrorV2::CheckedIntegerRange)?;
                }
                Ok(())
            };
        if let Some(pending) = &engine.pending_discard {
            register_detached(self, pending.resume_source.as_ref())?;
        }
        if let Some(pending) = &engine.pending_optional_cost {
            register_detached(self, pending.source.as_ref())?;
            register_detached(self, pending.spell_resume_source.as_ref())?;
        }
        if let Some(pending) = &engine.pending_optional_cost_sacrifice {
            register_detached(self, pending.source.as_ref())?;
            register_detached(self, pending.spell_resume_source.as_ref())?;
        }

        for context in [
            engine
                .pending_cast
                .as_ref()
                .map(|_| FlatContextKindV2::PendingCast),
            engine
                .pending_activation
                .as_ref()
                .map(|_| FlatContextKindV2::PendingActivation),
            engine
                .pending_discard
                .as_ref()
                .map(|_| FlatContextKindV2::PendingDiscard),
            engine
                .pending_optional_cost
                .as_ref()
                .map(|_| FlatContextKindV2::PendingOptionalCost),
            engine
                .pending_optional_cost_sacrifice
                .as_ref()
                .map(|_| FlatContextKindV2::PendingOptionalCostSacrifice),
            engine
                .pending_spell_copy
                .as_ref()
                .map(|_| FlatContextKindV2::PendingSpellCopy),
            engine
                .pending_effect
                .as_ref()
                .map(|_| FlatContextKindV2::PendingEffect),
        ]
        .into_iter()
        .flatten()
        {
            self.add_context_object(
                FlatObjectGroupV2::PendingContext,
                FlatObjectSourceKindV2::Pending,
                context_object_ordinal(context, 0),
            )?;
        }
        for (order, _) in engine.pending_triggers.iter().enumerate() {
            self.add_context_object(
                FlatObjectGroupV2::PendingContext,
                FlatObjectSourceKindV2::Pending,
                context_object_ordinal(FlatContextKindV2::PendingTrigger, usize_u32(order)?),
            )?;
        }
        let surface = &p.surface_context;
        for context in [
            surface
                .madness_cast_reprompt_source
                .as_ref()
                .map(|_| FlatContextKindV2::MadnessCastReprompt),
            surface
                .private_blockers
                .as_ref()
                .map(|_| FlatContextKindV2::PrivateBlockers),
            surface
                .private_discard
                .as_ref()
                .map(|_| FlatContextKindV2::PrivateDiscard),
            surface
                .private_optional_cost
                .as_ref()
                .map(|_| FlatContextKindV2::PrivateOptionalCost),
            observation
                .projection
                .policy_surface_context
                .private_combat_selection
                .as_ref()
                .map(|_| FlatContextKindV2::PrivateCombatSelection),
        ]
        .into_iter()
        .flatten()
        {
            self.add_context_object(
                FlatObjectGroupV2::PrivateContext,
                FlatObjectSourceKindV2::Private,
                context_object_ordinal(context, 0),
            )?;
        }

        for (relative_owner, seat) in [actor, opponent].into_iter().enumerate() {
            let group = if relative_owner == 0 {
                FlatObjectGroupV2::KnownSelfLibrary
            } else {
                FlatObjectGroupV2::KnownOpponentLibrary
            };
            for entry in &observation.known_library_cards[seat_index(seat)] {
                self.add_private_card(
                    &entry.card,
                    actor,
                    group,
                    FlatObjectSourceKindV2::KnownLibrary,
                    entry.position,
                    0,
                )?;
            }
        }
        for (relative_owner, seat) in [actor, opponent].into_iter().enumerate() {
            let group = if relative_owner == 0 {
                FlatObjectGroupV2::KnownSelfHand
            } else {
                FlatObjectGroupV2::KnownOpponentHand
            };
            for (order, card) in
                canonical_known_hand_cards(&observation.known_hand_cards[seat_index(seat)], actor)
                    .into_iter()
                    .enumerate()
            {
                self.add_private_card(
                    card,
                    actor,
                    group,
                    FlatObjectSourceKindV2::KnownHand,
                    usize_u32(order)?,
                    0,
                )?;
            }
        }
        let mut paid_order = 0_u32;
        for item in &p.stack {
            for paid in &item.paid_cost_refs {
                self.add_stable(
                    paid,
                    actor,
                    FlatObjectGroupV2::HistoricalPaidCost,
                    FlatObjectSourceKindV2::PaidCost,
                    paid_order,
                    HISTORICAL_PAID_COST_KIND_V1,
                )?;
                paid_order = paid_order
                    .checked_add(1)
                    .ok_or(FlatDecisionErrorV2::CheckedIntegerRange)?;
            }
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)] // Keeps the audited typed row fields explicit at each producer site.
    fn push_relation(
        &mut self,
        role: FlatRelationRoleV2,
        source_object: Option<u32>,
        target_object: Option<u32>,
        primary_order: u32,
        secondary_order: u32,
        associated_order: u32,
        payload: FlatRelationPayloadV2,
    ) {
        self.relations.push(FlatRelationV2 {
            role,
            source_object,
            target_object,
            primary_order,
            secondary_order,
            associated_order,
            payload,
        });
    }

    fn stack_payload(
        item: &crate::rl::StackItemPublicV2,
        actor: PlayerSeatV1,
        target: Option<&TargetRefV1>,
    ) -> FlatRelationPayloadV2 {
        let (target_kind, target_player) = target.map_or(
            (FlatTargetKindV2::None, FlatRelativePlayerV2::None),
            |target| target_parts(target, actor),
        );
        let target_object_controller = match target {
            Some(TargetRefV1::Object { object }) => relative_player(object.controller, actor),
            _ => FlatRelativePlayerV2::None,
        };
        FlatRelationPayloadV2::Stack(FlatStackRelationDataV2 {
            controller: relative_player(item.controller, actor),
            stack_item_kind: stack_kind_id(item.stack_item_kind),
            is_copy: item.is_copy,
            is_flashback: item.is_flashback,
            mode_chosen: item.mode_chosen,
            madness_offer: item.madness_offer,
            kicked: item.kicked,
            cast_method: item.cast_method.map_or(0, cast_method_id),
            face_index: item.face_index,
            x_value: item.x_value,
            target_kind,
            target_player,
            target_object_controller,
        })
    }

    fn effect_payload(
        effect: &ContinuousEffectPublicV2,
        actor: PlayerSeatV1,
        affected_player: Option<PlayerSeatV1>,
    ) -> FlatRelationPayloadV2 {
        FlatRelationPayloadV2::Effect(FlatEffectRelationDataV2 {
            controller: optional_relative_player(effect.controller, actor),
            affected_player: optional_relative_player(affected_player, actor),
            global: effect.global,
            layers: effect.layers,
            duration: effect_duration_id(effect.duration),
            power_delta: effect.power_delta,
            toughness_delta: effect.toughness_delta,
            grants_haste: effect.grants_haste,
            set_power: effect.set_power,
            set_toughness: effect.set_toughness,
            add_color_mask: effect.add_color_mask,
            remove_color_mask: effect.remove_color_mask,
            add_keyword_mask: effect.add_keyword_mask,
            remove_keyword_mask: effect.remove_keyword_mask,
            ward_generic_delta: effect.ward_generic_delta,
            minimum_blockers: effect.minimum_blockers,
            add_landwalk_mask: effect.add_landwalk_mask,
            remove_landwalk_mask: effect.remove_landwalk_mask,
            prevent_damage_from_color_mask: effect.prevent_damage_from_color_mask,
            damage_cannot_be_prevented: effect.damage_cannot_be_prevented,
        })
    }

    fn permission_payload(
        permission: &ExilePlayPermissionPublicV2,
        actor: PlayerSeatV1,
    ) -> FlatPermissionRelationDataV2 {
        let (expiry, holder_turn_started) = match permission.expiry {
            PlayPermissionExpiryV2::EndOfTurn => (0, false),
            PlayPermissionExpiryV2::UntilHoldersNextTurn {
                holder_turn_started,
            } => (1, holder_turn_started),
        };
        FlatPermissionRelationDataV2 {
            holder: relative_player(permission.holder, actor),
            play_or_cast: match permission.play_or_cast {
                PlayOrCastV2::Play => 0,
                PlayOrCastV2::Cast => 1,
            },
            expiry,
            holder_turn_started,
        }
    }

    fn context_payload(
        context: FlatContextKindV2,
        subrole: FlatContextSubroleV2,
        actor: PlayerSeatV1,
        target: Option<&TargetRefV1>,
        controller: Option<PlayerSeatV1>,
        trigger_kind: Option<PendingTriggerKindV2>,
        kicked: bool,
    ) -> FlatRelationPayloadV2 {
        let (target_kind, target_player) = target.map_or(
            (FlatTargetKindV2::None, FlatRelativePlayerV2::None),
            |value| target_parts(value, actor),
        );
        FlatRelationPayloadV2::Context(FlatContextRelationDataV2 {
            context,
            subrole,
            target_kind,
            target_player,
            controller: optional_relative_player(controller, actor),
            trigger_kind: trigger_kind.map_or(0, |kind| match kind {
                PendingTriggerKindV2::TriggeredAbility => 1,
                PendingTriggerKindV2::MadnessOffer => 2,
            }),
            kicked,
        })
    }

    #[allow(clippy::too_many_arguments)] // Context union fields stay explicit instead of hiding meaning in tuples.
    fn push_context_ref(
        &mut self,
        actor: PlayerSeatV1,
        role: FlatRelationRoleV2,
        context: FlatContextKindV2,
        subrole: FlatContextSubroleV2,
        stable: Option<&CardStableRefV1>,
        primary_order: u32,
        secondary_order: u32,
        associated_order: u32,
        controller: Option<PlayerSeatV1>,
        trigger_kind: Option<PendingTriggerKindV2>,
        kicked: bool,
    ) -> Result<(), FlatDecisionErrorV2> {
        let Some(stable) = stable else { return Ok(()) };
        let object = self.resolve_reference(stable, actor)?;
        let context_order = if context == FlatContextKindV2::PendingTrigger {
            primary_order
        } else {
            0
        };
        let context_object = self.context_object_index(
            if role == FlatRelationRoleV2::PendingContext {
                FlatObjectGroupV2::PendingContext
            } else {
                FlatObjectGroupV2::PrivateContext
            },
            context_object_ordinal(context, context_order),
        )?;
        self.push_relation(
            role,
            Some(context_object),
            Some(object),
            primary_order,
            secondary_order,
            associated_order,
            Self::context_payload(
                context,
                subrole,
                actor,
                None,
                controller,
                trigger_kind,
                kicked,
            ),
        );
        Ok(())
    }

    fn push_context_target(
        &mut self,
        actor: PlayerSeatV1,
        context: FlatContextKindV2,
        subrole: FlatContextSubroleV2,
        target: &TargetRefV1,
        primary_order: u32,
        controller: Option<PlayerSeatV1>,
    ) -> Result<(), FlatDecisionErrorV2> {
        let target_object = match target {
            TargetRefV1::Object { object } => Some(self.resolve_reference(object, actor)?),
            TargetRefV1::Player { .. } => None,
        };
        let context_object = self.context_object_index(
            FlatObjectGroupV2::PendingContext,
            context_object_ordinal(context, 0),
        )?;
        self.push_relation(
            FlatRelationRoleV2::PendingContext,
            Some(context_object),
            target_object,
            primary_order,
            0,
            0,
            Self::context_payload(
                context,
                subrole,
                actor,
                Some(target),
                controller,
                None,
                false,
            ),
        );
        Ok(())
    }

    fn build_relations(&mut self, observation: &ObservationV5) -> Result<(), FlatDecisionErrorV2> {
        let actor = observation.acting_player;
        let opponent = opponent(actor);
        let p = &observation.projection.surface;

        let actor_relative_cards = p.battlefield[seat_index(actor)]
            .iter()
            .chain(p.battlefield[seat_index(opponent)].iter())
            .chain(p.graveyards[seat_index(actor)].iter())
            .chain(p.graveyards[seat_index(opponent)].iter())
            .chain(p.exile.iter());
        let mut attachment_pairs = Vec::new();
        for card in actor_relative_cards {
            let host = self.resolve_live(&card.stable, actor)?;
            for &attachment_arena in &card.attachments {
                let attachment = self.resolve_arena(attachment_arena)?;
                attachment_pairs.push((host, attachment));
            }
        }
        attachment_pairs.sort_unstable();
        for (attachment_order, (host, attachment)) in attachment_pairs.into_iter().enumerate() {
            let attachment_order = usize_u32(attachment_order)?;
            let context =
                self.context_object_index(FlatObjectGroupV2::Attachment, attachment_order)?;
            self.push_relation(
                FlatRelationRoleV2::Attachment,
                Some(context),
                Some(host),
                attachment_order,
                0,
                0,
                FlatRelationPayloadV2::None,
            );
            self.push_relation(
                FlatRelationRoleV2::Attachment,
                Some(context),
                Some(attachment),
                attachment_order,
                0,
                1,
                FlatRelationPayloadV2::None,
            );
        }
        let mut object_relations = Vec::with_capacity(p.object_relations.len());
        for relation in &p.object_relations {
            match relation {
                ObjectRelationPublicV4::AttachedTo {
                    object,
                    attached_to,
                } => {
                    let source = self.resolve_live(object, actor)?;
                    let target = self.resolve_live(attached_to, actor)?;
                    object_relations.push((FlatRelationRoleV2::AttachedTo, source, target));
                }
                ObjectRelationPublicV4::ExiledBy { object, exiled_by } => {
                    let source = self.resolve_live(object, actor)?;
                    let target = self.resolve_live(exiled_by, actor)?;
                    object_relations.push((FlatRelationRoleV2::ExiledBy, source, target));
                }
            }
        }
        object_relations
            .sort_unstable_by_key(|(role, source, target)| (*role as u8, *source, *target));
        for (order, (role, source, target)) in object_relations.into_iter().enumerate() {
            self.push_relation(
                role,
                Some(source),
                Some(target),
                usize_u32(order)?,
                0,
                0,
                FlatRelationPayloadV2::None,
            );
        }
        for (stack_order, item) in p.stack.iter().enumerate() {
            let stack_order = usize_u32(stack_order)?;
            let source = self.resolve_live(&item.source, actor)?;
            self.push_relation(
                FlatRelationRoleV2::StackTarget,
                Some(source),
                Some(source),
                stack_order,
                0,
                0,
                Self::stack_payload(item, actor, None),
            );
            for (target_order, target) in item.targets.iter().enumerate() {
                let target_object = match target {
                    TargetRefV1::Object { object } => {
                        Some(self.resolve_historical_stack_target(object, actor)?)
                    }
                    TargetRefV1::Player { .. } => None,
                };
                self.push_relation(
                    FlatRelationRoleV2::StackTarget,
                    Some(source),
                    target_object,
                    stack_order,
                    usize_u32(target_order)?
                        .checked_add(1)
                        .ok_or(FlatDecisionErrorV2::CheckedIntegerRange)?,
                    0,
                    Self::stack_payload(item, actor, Some(target)),
                );
            }
            for (paid_order, paid) in item.paid_cost_refs.iter().enumerate() {
                let paid = self.resolve_paid_cost_reference(paid, actor)?;
                self.push_relation(
                    FlatRelationRoleV2::PaidCost,
                    Some(source),
                    Some(paid),
                    stack_order,
                    usize_u32(paid_order)?,
                    0,
                    FlatRelationPayloadV2::None,
                );
            }
        }
        let blocked = &p.combat.attacker_to_ordered_blockers;
        for (mapping_order, (candidate, _)) in blocked.iter().enumerate() {
            self.resolve_live(candidate, actor)?;
            if blocked[..mapping_order].iter().any(|(prior, _)| {
                prior.arena_id == candidate.arena_id
                    && prior.zone_change_count == candidate.zone_change_count
            }) || !p.combat.ordered_attackers.iter().any(|attacker| {
                attacker.arena_id == candidate.arena_id
                    && attacker.zone_change_count == candidate.zone_change_count
            }) {
                return Err(FlatDecisionErrorV2::InconsistentReference);
            }
        }
        for (attacker_order, attacker) in p.combat.ordered_attackers.iter().enumerate() {
            let attacker_order = usize_u32(attacker_order)?;
            let object = self.resolve_live(attacker, actor)?;
            let blocked_order = blocked
                .iter()
                .position(|(candidate, _)| {
                    candidate.arena_id == attacker.arena_id
                        && candidate.zone_change_count == attacker.zone_change_count
                })
                .map(usize_u32)
                .transpose()?;
            self.push_relation(
                FlatRelationRoleV2::CombatAttacker,
                Some(self.context_object_index(FlatObjectGroupV2::Combat, attacker_order)?),
                Some(object),
                attacker_order,
                0,
                0,
                FlatRelationPayloadV2::CombatAttacker { blocked_order },
            );
        }
        let mut combat_block_order = 0_u32;
        for (attacker_order, (attacker, blockers)) in blocked.iter().enumerate() {
            let attacker = self.resolve_live(attacker, actor)?;
            for (blocker_order, blocker) in blockers.iter().enumerate() {
                let blocker = self.resolve_live(blocker, actor)?;
                let context =
                    self.context_object_index(FlatObjectGroupV2::CombatBlock, combat_block_order)?;
                self.push_relation(
                    FlatRelationRoleV2::CombatBlocker,
                    Some(context),
                    Some(attacker),
                    usize_u32(attacker_order)?,
                    usize_u32(blocker_order)?,
                    0,
                    FlatRelationPayloadV2::None,
                );
                self.push_relation(
                    FlatRelationRoleV2::CombatBlocker,
                    Some(context),
                    Some(blocker),
                    usize_u32(attacker_order)?,
                    usize_u32(blocker_order)?,
                    1,
                    FlatRelationPayloadV2::None,
                );
                combat_block_order = combat_block_order
                    .checked_add(1)
                    .ok_or(FlatDecisionErrorV2::CheckedIntegerRange)?;
            }
        }
        for (effect_order, effect) in p.continuous_effects.iter().enumerate() {
            let effect_order = usize_u32(effect_order)?;
            let context =
                self.context_object_index(FlatObjectGroupV2::ContinuousEffect, effect_order)?;
            let source = effect
                .source
                .as_ref()
                .map(|source| self.resolve_live(source, actor))
                .transpose()?;
            self.push_relation(
                FlatRelationRoleV2::EffectSource,
                Some(context),
                source,
                effect_order,
                0,
                0,
                Self::effect_payload(effect, actor, None),
            );
            let mut affected_objects = effect
                .affected_objects
                .iter()
                .map(|affected| self.resolve_live(affected, actor))
                .collect::<Result<Vec<_>, _>>()?;
            affected_objects.sort_unstable();
            for (affected_order, affected) in affected_objects.into_iter().enumerate() {
                self.push_relation(
                    FlatRelationRoleV2::EffectAffected,
                    Some(context),
                    Some(affected),
                    effect_order,
                    usize_u32(affected_order)?,
                    0,
                    Self::effect_payload(effect, actor, None),
                );
            }
            let mut affected_players = effect.affected_players.clone();
            affected_players.sort_unstable_by_key(|&player| relative_player(player, actor) as u8);
            for (affected_order, affected) in affected_players.into_iter().enumerate() {
                self.push_relation(
                    FlatRelationRoleV2::EffectAffected,
                    Some(context),
                    None,
                    effect_order,
                    usize_u32(affected_order)?,
                    1,
                    Self::effect_payload(effect, actor, Some(affected)),
                );
            }
            for (order, &subtype_id) in effect.add_subtype_ids.iter().enumerate() {
                self.effect_subtype_changes.push(FlatEffectSubtypeChangeV2 {
                    effect_order,
                    kind: FlatEffectSubtypeChangeKindV2::Add,
                    order: usize_u32(order)?,
                    subtype_id,
                });
            }
            for (order, &subtype_id) in effect.remove_subtype_ids.iter().enumerate() {
                self.effect_subtype_changes.push(FlatEffectSubtypeChangeV2 {
                    effect_order,
                    kind: FlatEffectSubtypeChangeKindV2::Remove,
                    order: usize_u32(order)?,
                    subtype_id,
                });
            }
        }
        let mut permissions = Vec::with_capacity(p.exile_play_permissions.len());
        for permission in &p.exile_play_permissions {
            if permission.zone_change_generation != permission.object.zone_change_count {
                return Err(FlatDecisionErrorV2::InconsistentReference);
            }
            let object = self.resolve_live(&permission.object, actor)?;
            permissions.push((object, Self::permission_payload(permission, actor)));
        }
        permissions.sort_unstable_by_key(|(object, payload)| {
            (
                *object,
                std::cmp::Reverse(payload.holder as u8),
                std::cmp::Reverse(payload.play_or_cast),
                std::cmp::Reverse(payload.expiry),
                payload.holder_turn_started,
            )
        });
        for (order, (object, payload)) in permissions.into_iter().enumerate() {
            let order = usize_u32(order)?;
            self.push_relation(
                FlatRelationRoleV2::Permission,
                Some(self.context_object_index(FlatObjectGroupV2::Permission, order)?),
                Some(object),
                order,
                0,
                0,
                FlatRelationPayloadV2::Permission(payload),
            );
        }
        for (relative_owner, seat) in [actor, opponent].into_iter().enumerate() {
            for entry in &observation.known_library_cards[seat_index(seat)] {
                let object = self.resolve_reference(&entry.card.stable, actor)?;
                self.push_relation(
                    FlatRelationRoleV2::KnownLibrary,
                    Some(object),
                    Some(object),
                    usize_u32(relative_owner)?,
                    entry.position,
                    0,
                    FlatRelationPayloadV2::Known {
                        owner: relative_player(seat, actor),
                    },
                );
            }
            for (reveal_order, card) in
                canonical_known_hand_cards(&observation.known_hand_cards[seat_index(seat)], actor)
                    .into_iter()
                    .enumerate()
            {
                let object = self.resolve_reference(&card.stable, actor)?;
                self.push_relation(
                    FlatRelationRoleV2::KnownHand,
                    Some(object),
                    Some(object),
                    usize_u32(relative_owner)?,
                    usize_u32(reveal_order)?,
                    0,
                    FlatRelationPayloadV2::Known {
                        owner: relative_player(seat, actor),
                    },
                );
            }
        }
        self.build_pending_relations(observation)?;
        self.build_private_relations(observation)?;
        Ok(())
    }

    fn build_pending_relations(
        &mut self,
        observation: &ObservationV5,
    ) -> Result<(), FlatDecisionErrorV2> {
        let actor = observation.acting_player;
        let engine = &observation.projection.surface.engine_context;
        if let Some(pending) = &engine.pending_cast {
            self.push_context_ref(
                actor,
                FlatRelationRoleV2::PendingContext,
                FlatContextKindV2::PendingCast,
                FlatContextSubroleV2::PendingCastSource,
                pending.source.as_ref(),
                0,
                0,
                0,
                Some(pending.controller),
                None,
                false,
            )?;
            for (order, target) in pending.chosen_targets.iter().enumerate() {
                self.push_context_target(
                    actor,
                    FlatContextKindV2::PendingCast,
                    FlatContextSubroleV2::PendingCastChosenTarget,
                    target,
                    usize_u32(order)?,
                    Some(pending.controller),
                )?;
            }
            if let Some(discarded) = &pending.additional_cost_discarded {
                for (order, stable) in discarded.iter().enumerate() {
                    self.push_context_ref(
                        actor,
                        FlatRelationRoleV2::PendingContext,
                        FlatContextKindV2::PendingCast,
                        FlatContextSubroleV2::PendingCastDiscarded,
                        Some(stable),
                        usize_u32(order)?,
                        0,
                        0,
                        Some(pending.controller),
                        None,
                        false,
                    )?;
                }
            }
            for (order, stable) in pending.sacrifice_chosen.iter().enumerate() {
                self.push_context_ref(
                    actor,
                    FlatRelationRoleV2::PendingContext,
                    FlatContextKindV2::PendingCast,
                    FlatContextSubroleV2::PendingCastSacrificed,
                    Some(stable),
                    usize_u32(order)?,
                    0,
                    0,
                    Some(pending.controller),
                    None,
                    false,
                )?;
            }
        }
        if let Some(pending) = &engine.pending_activation {
            self.push_context_ref(
                actor,
                FlatRelationRoleV2::PendingContext,
                FlatContextKindV2::PendingActivation,
                FlatContextSubroleV2::PendingActivationSource,
                pending.source.as_ref(),
                0,
                0,
                0,
                Some(pending.controller),
                None,
                false,
            )?;
            for (order, target) in pending.chosen_targets.iter().enumerate() {
                self.push_context_target(
                    actor,
                    FlatContextKindV2::PendingActivation,
                    FlatContextSubroleV2::PendingActivationChosenTarget,
                    target,
                    usize_u32(order)?,
                    Some(pending.controller),
                )?;
            }
            if let Some(discarded) = &pending.cost_discard_paid {
                for (order, stable) in discarded.iter().enumerate() {
                    self.push_context_ref(
                        actor,
                        FlatRelationRoleV2::PendingContext,
                        FlatContextKindV2::PendingActivation,
                        FlatContextSubroleV2::PendingActivationDiscarded,
                        Some(stable),
                        usize_u32(order)?,
                        0,
                        0,
                        Some(pending.controller),
                        None,
                        false,
                    )?;
                }
            }
        }
        if let Some(pending) = &engine.pending_discard {
            self.push_context_ref(
                actor,
                FlatRelationRoleV2::PendingContext,
                FlatContextKindV2::PendingDiscard,
                FlatContextSubroleV2::PendingDiscardResumeSource,
                pending.resume_source.as_ref(),
                0,
                0,
                0,
                Some(pending.player),
                None,
                false,
            )?;
        }
        if let Some(pending) = &engine.pending_optional_cost {
            self.push_context_ref(
                actor,
                FlatRelationRoleV2::PendingContext,
                FlatContextKindV2::PendingOptionalCost,
                FlatContextSubroleV2::PendingOptionalCostSource,
                pending.source.as_ref(),
                0,
                0,
                0,
                Some(pending.player),
                None,
                false,
            )?;
            self.push_context_ref(
                actor,
                FlatRelationRoleV2::PendingContext,
                FlatContextKindV2::PendingOptionalCost,
                FlatContextSubroleV2::PendingOptionalCostSpellResumeSource,
                pending.spell_resume_source.as_ref(),
                0,
                0,
                0,
                Some(pending.player),
                None,
                false,
            )?;
        }
        if let Some(pending) = &engine.pending_optional_cost_sacrifice {
            self.push_context_ref(
                actor,
                FlatRelationRoleV2::PendingContext,
                FlatContextKindV2::PendingOptionalCostSacrifice,
                FlatContextSubroleV2::PendingOptionalSacrificeSource,
                pending.source.as_ref(),
                0,
                0,
                0,
                Some(pending.player),
                None,
                false,
            )?;
            for (order, stable) in pending.chosen.iter().enumerate() {
                self.push_context_ref(
                    actor,
                    FlatRelationRoleV2::PendingContext,
                    FlatContextKindV2::PendingOptionalCostSacrifice,
                    FlatContextSubroleV2::PendingOptionalSacrificeChosen,
                    Some(stable),
                    usize_u32(order)?,
                    0,
                    0,
                    Some(pending.player),
                    None,
                    false,
                )?;
            }
            self.push_context_ref(
                actor,
                FlatRelationRoleV2::PendingContext,
                FlatContextKindV2::PendingOptionalCostSacrifice,
                FlatContextSubroleV2::PendingOptionalSacrificeSpellResumeSource,
                pending.spell_resume_source.as_ref(),
                0,
                0,
                0,
                Some(pending.player),
                None,
                false,
            )?;
        }
        if let Some(pending) = &engine.pending_spell_copy {
            self.push_context_ref(
                actor,
                FlatRelationRoleV2::PendingContext,
                FlatContextKindV2::PendingSpellCopy,
                FlatContextSubroleV2::PendingSpellCopyParent,
                pending.parent.as_ref(),
                0,
                0,
                0,
                Some(pending.player),
                None,
                false,
            )?;
            self.push_context_target(
                actor,
                FlatContextKindV2::PendingSpellCopy,
                FlatContextSubroleV2::PendingSpellCopyInheritedTarget,
                &pending.inherited_target,
                0,
                Some(pending.player),
            )?;
            self.push_context_ref(
                actor,
                FlatRelationRoleV2::PendingContext,
                FlatContextKindV2::PendingSpellCopy,
                FlatContextSubroleV2::PendingSpellCopyCopy,
                pending.copy.as_ref(),
                0,
                0,
                0,
                Some(pending.player),
                None,
                false,
            )?;
        }
        if let Some(pending) = &engine.pending_effect {
            self.push_context_ref(
                actor,
                FlatRelationRoleV2::PendingContext,
                FlatContextKindV2::PendingEffect,
                FlatContextSubroleV2::PendingEffectSource,
                pending.source.as_ref(),
                0,
                0,
                0,
                Some(pending.controller),
                None,
                false,
            )?;
            if let Some(PendingEffectChoiceSemanticV4::Targets {
                selected_targets,
                legal_targets,
                ..
            }) = &pending.choice
            {
                for (order, target) in selected_targets.iter().enumerate() {
                    self.push_context_target(
                        actor,
                        FlatContextKindV2::PendingEffect,
                        FlatContextSubroleV2::PendingEffectSelectedTarget,
                        target,
                        usize_u32(order)?,
                        Some(pending.controller),
                    )?;
                }
                for (order, target) in legal_targets.iter().enumerate() {
                    self.push_context_target(
                        actor,
                        FlatContextKindV2::PendingEffect,
                        FlatContextSubroleV2::PendingEffectLegalTarget,
                        target,
                        usize_u32(order)?,
                        Some(pending.controller),
                    )?;
                }
            }
        }
        for (order, trigger) in engine.pending_triggers.iter().enumerate() {
            let order = usize_u32(order)?;
            let object = trigger
                .source
                .as_ref()
                .map(|source| self.resolve_reference(source, actor))
                .transpose()?;
            let context_object = self.context_object_index(
                FlatObjectGroupV2::PendingContext,
                context_object_ordinal(FlatContextKindV2::PendingTrigger, order),
            )?;
            self.push_relation(
                FlatRelationRoleV2::PendingContext,
                Some(context_object),
                object,
                order,
                0,
                0,
                Self::context_payload(
                    FlatContextKindV2::PendingTrigger,
                    FlatContextSubroleV2::PendingTriggerSource,
                    actor,
                    None,
                    Some(trigger.controller),
                    Some(trigger.trigger_kind),
                    trigger.kicked,
                ),
            );
        }
        Ok(())
    }

    fn build_private_relations(
        &mut self,
        observation: &ObservationV5,
    ) -> Result<(), FlatDecisionErrorV2> {
        let actor = observation.acting_player;
        let surface = &observation.projection.surface.surface_context;
        self.push_context_ref(
            actor,
            FlatRelationRoleV2::PrivateContext,
            FlatContextKindV2::MadnessCastReprompt,
            FlatContextSubroleV2::MadnessCastRepromptSource,
            surface.madness_cast_reprompt_source.as_ref(),
            0,
            0,
            0,
            None,
            None,
            false,
        )?;
        if let Some(private) = &surface.private_blockers {
            self.push_context_ref(
                actor,
                FlatRelationRoleV2::PrivateContext,
                FlatContextKindV2::PrivateBlockers,
                FlatContextSubroleV2::PrivateBlockersCurrentAttacker,
                private.current_attacker.as_ref(),
                0,
                0,
                0,
                None,
                None,
                false,
            )?;
            for (pair_order, (attacker, blocker)) in private.accumulated.iter().enumerate() {
                for (subrole, stable, associated_order) in [
                    (
                        FlatContextSubroleV2::PrivateBlockersAccumulatedAttacker,
                        attacker,
                        0,
                    ),
                    (
                        FlatContextSubroleV2::PrivateBlockersAccumulatedBlocker,
                        blocker,
                        1,
                    ),
                ] {
                    self.push_context_ref(
                        actor,
                        FlatRelationRoleV2::PrivateContext,
                        FlatContextKindV2::PrivateBlockers,
                        subrole,
                        Some(stable),
                        usize_u32(pair_order)?,
                        0,
                        associated_order,
                        None,
                        None,
                        false,
                    )?;
                }
            }
            for (attacker_order, (attacker, blockers)) in private.remaining.iter().enumerate() {
                self.push_context_ref(
                    actor,
                    FlatRelationRoleV2::PrivateContext,
                    FlatContextKindV2::PrivateBlockers,
                    FlatContextSubroleV2::PrivateBlockersRemainingAttacker,
                    Some(attacker),
                    usize_u32(attacker_order)?,
                    0,
                    0,
                    None,
                    None,
                    false,
                )?;
                for (blocker_order, blocker) in blockers.iter().enumerate() {
                    self.push_context_ref(
                        actor,
                        FlatRelationRoleV2::PrivateContext,
                        FlatContextKindV2::PrivateBlockers,
                        FlatContextSubroleV2::PrivateBlockersRemainingBlocker,
                        Some(blocker),
                        usize_u32(attacker_order)?,
                        usize_u32(blocker_order)?,
                        0,
                        None,
                        None,
                        false,
                    )?;
                }
            }
        }
        if let Some(private) = &surface.private_discard {
            for (order, stable) in private.chosen.iter().enumerate() {
                self.push_context_ref(
                    actor,
                    FlatRelationRoleV2::PrivateContext,
                    FlatContextKindV2::PrivateDiscard,
                    FlatContextSubroleV2::PrivateDiscardChosen,
                    Some(stable),
                    usize_u32(order)?,
                    0,
                    0,
                    None,
                    None,
                    false,
                )?;
            }
            for (order, stable) in private.remaining_choices.iter().enumerate() {
                self.push_context_ref(
                    actor,
                    FlatRelationRoleV2::PrivateContext,
                    FlatContextKindV2::PrivateDiscard,
                    FlatContextSubroleV2::PrivateDiscardRemainingChoice,
                    Some(stable),
                    usize_u32(order)?,
                    0,
                    0,
                    None,
                    None,
                    false,
                )?;
            }
        }
        if let Some(private) = &observation
            .projection
            .policy_surface_context
            .private_combat_selection
        {
            self.push_context_ref(
                actor,
                FlatRelationRoleV2::PrivateContext,
                FlatContextKindV2::PrivateCombatSelection,
                FlatContextSubroleV2::PrivateCombatAttacker,
                private.attacker.as_ref(),
                0,
                0,
                0,
                None,
                None,
                false,
            )?;
            for (order, stable) in private.selected.iter().enumerate() {
                self.push_context_ref(
                    actor,
                    FlatRelationRoleV2::PrivateContext,
                    FlatContextKindV2::PrivateCombatSelection,
                    FlatContextSubroleV2::PrivateCombatSelected,
                    Some(stable),
                    usize_u32(order)?,
                    0,
                    0,
                    None,
                    None,
                    false,
                )?;
            }
            self.push_context_ref(
                actor,
                FlatRelationRoleV2::PrivateContext,
                FlatContextKindV2::PrivateCombatSelection,
                FlatContextSubroleV2::PrivateCombatCurrentCandidate,
                Some(&private.current_candidate),
                private.candidate_index,
                0,
                0,
                None,
                None,
                false,
            )?;
            for (order, stable) in private.remaining_after_current.iter().enumerate() {
                self.push_context_ref(
                    actor,
                    FlatRelationRoleV2::PrivateContext,
                    FlatContextKindV2::PrivateCombatSelection,
                    FlatContextSubroleV2::PrivateCombatRemainingCandidate,
                    Some(stable),
                    usize_u32(order)?,
                    0,
                    0,
                    None,
                    None,
                    false,
                )?;
            }
        }
        Ok(())
    }

    fn build_cache(
        &mut self,
        session: &FastActorSessionV1,
        expected: FastActorDecisionV1,
    ) -> Result<(), FlatDecisionErrorV2> {
        self.clear_typed_cache();
        let action_count = usize::try_from(expected.legal_action_count)
            .map_err(|_| FlatDecisionErrorV2::CheckedIntegerRange)?;
        let max_refs = action_count
            .checked_mul(FLAT_ACTION_MAX_TRIGGER_ORDER_REFS_V1)
            .ok_or(FlatDecisionErrorV2::CheckedIntegerRange)?;
        self.actions
            .resize(action_count, FlatActionCoreV1::default());
        self.action_refs
            .resize(max_refs, FlatActionRefV2::default());
        self.action_objects
            .resize(max_refs, FlatActionObjectV2::default());
        let action_slice = session.encode_current_flat_action_slice_v2(
            expected,
            &mut FlatActionDecisionSliceBuffersV2 {
                actions: &mut self.actions,
                refs: &mut self.action_refs,
                objects: &mut self.action_objects,
            },
        )?;
        self.actions.truncate(
            usize::try_from(action_slice.active_action_count)
                .map_err(|_| FlatDecisionErrorV2::CheckedIntegerRange)?,
        );
        self.action_refs.truncate(
            usize::try_from(action_slice.active_ref_count)
                .map_err(|_| FlatDecisionErrorV2::CheckedIntegerRange)?,
        );
        self.action_objects
            .truncate(usize::from(action_slice.active_object_count));

        let observation = session.flat_policy_observation_v2(expected)?;
        if observation.schema_version != 5
            || observation.acting_player != expected.acting_player
            || observation.step_index != expected.step
            || observation.physical_decision_id != expected.physical_decision_id
            || observation.substep_index != expected.substep_index
            || observation.substep_count != expected.substep_count
            || observation.card_db_hash != action_slice.binding.card_db_hash
        {
            return Err(FlatDecisionErrorV2::ObservationContract);
        }
        self.build_globals(&observation)?;
        self.register_objects(&observation)?;
        self.build_relations(&observation)?;
        self.validate_cached_tables()?;
        self.cached_binding = Some(action_slice.binding);
        Ok(())
    }

    fn validate_cached_tables(&mut self) -> Result<(), FlatDecisionErrorV2> {
        let object_count = usize_u32(self.objects.len())?;
        if self.object_keys.len() != self.objects.len() {
            return Err(FlatDecisionErrorV2::InvalidReference);
        }
        let mut blocked_mapping_count = 0_u32;
        for (relation_index, relation) in self.relations.iter().enumerate() {
            if relation
                .source_object
                .is_some_and(|index| index >= object_count)
                || relation
                    .target_object
                    .is_some_and(|index| index >= object_count)
            {
                return Err(FlatDecisionErrorV2::InvalidReference);
            }
            match (relation.role, relation.payload) {
                (
                    FlatRelationRoleV2::CombatAttacker,
                    FlatRelationPayloadV2::CombatAttacker {
                        blocked_order: Some(order),
                    },
                ) => {
                    if self.relations[..relation_index].iter().any(|prior| {
                        matches!(
                            prior.payload,
                            FlatRelationPayloadV2::CombatAttacker {
                                blocked_order: Some(prior_order)
                            } if prior_order == order
                        )
                    }) {
                        return Err(FlatDecisionErrorV2::InconsistentReference);
                    }
                    blocked_mapping_count = blocked_mapping_count
                        .checked_add(1)
                        .ok_or(FlatDecisionErrorV2::CheckedIntegerRange)?;
                }
                (
                    FlatRelationRoleV2::CombatAttacker,
                    FlatRelationPayloadV2::CombatAttacker {
                        blocked_order: None,
                    },
                ) => {}
                (FlatRelationRoleV2::CombatAttacker, _)
                | (_, FlatRelationPayloadV2::CombatAttacker { .. }) => {
                    return Err(FlatDecisionErrorV2::InconsistentReference);
                }
                _ => {}
            }
        }
        if self
            .relations
            .iter()
            .any(|relation| match relation.payload {
                FlatRelationPayloadV2::CombatAttacker {
                    blocked_order: Some(order),
                } => order >= blocked_mapping_count,
                _ => false,
            })
        {
            return Err(FlatDecisionErrorV2::InconsistentReference);
        }
        for relation in self
            .relations
            .iter()
            .filter(|relation| relation.role == FlatRelationRoleV2::CombatBlocker)
        {
            if relation.primary_order >= blocked_mapping_count
                || relation.payload != FlatRelationPayloadV2::None
                || relation.associated_order > 1
            {
                return Err(FlatDecisionErrorV2::InconsistentReference);
            }
            let matching_pair_count = self
                .relations
                .iter()
                .filter(|candidate| {
                    candidate.role == FlatRelationRoleV2::CombatBlocker
                        && candidate.source_object == relation.source_object
                        && candidate.primary_order == relation.primary_order
                        && candidate.secondary_order == relation.secondary_order
                        && candidate.associated_order != relation.associated_order
                        && candidate.associated_order <= 1
                })
                .count();
            if matching_pair_count != 1 {
                return Err(FlatDecisionErrorV2::InconsistentReference);
            }
        }
        for row in &self.object_subtypes {
            if row.object_index >= object_count {
                return Err(FlatDecisionErrorV2::InvalidReference);
            }
        }
        for row in &self.ability_uses {
            if row.object_index >= object_count {
                return Err(FlatDecisionErrorV2::InvalidReference);
            }
        }
        for row in &self.goads {
            if row.object_index >= object_count {
                return Err(FlatDecisionErrorV2::InvalidReference);
            }
        }
        for (index, object) in self.objects.iter().enumerate() {
            if self.object_keys[index].is_some_and(|key| {
                !(1..=65_536).contains(&key.card_token) || key.card_token != object.card_token
            }) {
                return Err(FlatDecisionErrorV2::InvalidReference);
            }
            let index = usize_u32(index)?;
            let subtype_end = object
                .subtype_start
                .checked_add(object.subtype_count)
                .ok_or(FlatDecisionErrorV2::CheckedIntegerRange)?;
            let ability_end = object
                .ability_use_start
                .checked_add(object.ability_use_count)
                .ok_or(FlatDecisionErrorV2::CheckedIntegerRange)?;
            let goad_end = object
                .goad_start
                .checked_add(object.goad_count)
                .ok_or(FlatDecisionErrorV2::CheckedIntegerRange)?;
            if subtype_end > usize_u32(self.object_subtypes.len())?
                || ability_end > usize_u32(self.ability_uses.len())?
                || goad_end > usize_u32(self.goads.len())?
                || self.object_subtypes[usize::try_from(object.subtype_start)
                    .map_err(|_| FlatDecisionErrorV2::CheckedIntegerRange)?
                    ..usize::try_from(subtype_end)
                        .map_err(|_| FlatDecisionErrorV2::CheckedIntegerRange)?]
                    .iter()
                    .any(|row| row.object_index != index)
                || self.ability_uses[usize::try_from(object.ability_use_start)
                    .map_err(|_| FlatDecisionErrorV2::CheckedIntegerRange)?
                    ..usize::try_from(ability_end)
                        .map_err(|_| FlatDecisionErrorV2::CheckedIntegerRange)?]
                    .iter()
                    .any(|row| row.object_index != index)
                || self.goads[usize::try_from(object.goad_start)
                    .map_err(|_| FlatDecisionErrorV2::CheckedIntegerRange)?
                    ..usize::try_from(goad_end)
                        .map_err(|_| FlatDecisionErrorV2::CheckedIntegerRange)?]
                    .iter()
                    .any(|row| row.object_index != index)
            {
                return Err(FlatDecisionErrorV2::InvalidReference);
            }
        }
        let mut action_object_to_model_object =
            std::mem::take(&mut self.action_object_to_model_object);
        action_object_to_model_object.clear();
        let mut claimed_model_objects = std::mem::take(&mut self.claimed_model_objects);
        claimed_model_objects.clear();
        let mut scorer_actions = std::mem::take(&mut self.scorer_actions);
        scorer_actions.clear();
        let mut scorer_action_refs = std::mem::take(&mut self.scorer_action_refs);
        scorer_action_refs.clear();
        let scorer_validation = (|| -> Result<(), FlatDecisionErrorV2> {
            if self
                .action_objects
                .iter()
                .any(|object| !(1..=65_536).contains(&object.card_token))
                || self
                    .action_refs
                    .iter()
                    .any(|reference| !(1..=65_536).contains(&reference.card_token))
            {
                return Err(FlatDecisionErrorV2::InvalidReference);
            }
            scorer_actions
                .try_reserve(self.actions.len())
                .map_err(|_| FlatDecisionErrorV2::CheckedIntegerRange)?;
            scorer_actions.extend(
                self.actions
                    .iter()
                    .copied()
                    .map(FlatScorerActionCoreV2::from),
            );
            action_object_to_model_object
                .try_reserve(self.action_objects.len())
                .map_err(|_| FlatDecisionErrorV2::CheckedIntegerRange)?;
            claimed_model_objects
                .try_reserve(self.objects.len())
                .map_err(|_| FlatDecisionErrorV2::CheckedIntegerRange)?;
            claimed_model_objects.resize(self.objects.len(), false);
            for (action_object_index, action_object) in self.action_objects.iter().enumerate() {
                let mut matching_model_objects = self
                    .objects
                    .iter()
                    .zip(&self.object_keys)
                    .enumerate()
                    .filter_map(|(model_object_index, (object, key))| {
                        let Some(key) = key else { return None };
                        let group_matches = match action_object.group {
                            FlatActionObjectGroupV1::SelfHand => {
                                object.group == FlatObjectGroupV2::SelfHand
                            }
                            FlatActionObjectGroupV1::KnownOpponentHand => {
                                object.group == FlatObjectGroupV2::KnownOpponentHand
                            }
                            FlatActionObjectGroupV1::SelfBattlefield => {
                                object.group == FlatObjectGroupV2::SelfBattlefield
                            }
                            FlatActionObjectGroupV1::OpponentBattlefield => {
                                object.group == FlatObjectGroupV2::OpponentBattlefield
                            }
                            FlatActionObjectGroupV1::SelfGraveyard => {
                                object.group == FlatObjectGroupV2::SelfGraveyard
                            }
                            FlatActionObjectGroupV1::OpponentGraveyard => {
                                object.group == FlatObjectGroupV2::OpponentGraveyard
                            }
                            FlatActionObjectGroupV1::Exile => {
                                object.group == FlatObjectGroupV2::Exile
                            }
                            FlatActionObjectGroupV1::Stack => {
                                object.group == FlatObjectGroupV2::Stack
                                    || (object.group == FlatObjectGroupV2::PendingContext
                                        && object.source_kind == FlatObjectSourceKindV2::Pending)
                            }
                            FlatActionObjectGroupV1::Command => false,
                            FlatActionObjectGroupV1::KnownSelfLibrary => {
                                object.group == FlatObjectGroupV2::KnownSelfLibrary
                            }
                            FlatActionObjectGroupV1::KnownOpponentLibrary => {
                                object.group == FlatObjectGroupV2::KnownOpponentLibrary
                            }
                        };
                        let ordinal_matches = object.group == FlatObjectGroupV2::PendingContext
                            || object.visible_ordinal
                                == u32::from(action_object.actor_visible_ordinal);
                        (group_matches
                            && ordinal_matches
                            && key.card_token == action_object.card_token
                            && key.owner as u8 == action_object.owner_relative
                            && key.controller as u8 == action_object.controller_relative
                            && key.zone as u8 == action_object.zone
                            && key.zone_change_count == action_object.zone_change_count)
                            .then_some(model_object_index)
                    });
                let Some(model_object_index) = matching_model_objects.next() else {
                    return Err(FlatDecisionErrorV2::InvalidReference);
                };
                if matching_model_objects.next().is_some()
                    || claimed_model_objects[model_object_index]
                    || self.action_refs.iter().any(|reference| {
                        usize::from(reference.object_index) == action_object_index
                            && reference.card_token != action_object.card_token
                    })
                {
                    return Err(FlatDecisionErrorV2::InvalidReference);
                }
                claimed_model_objects[model_object_index] = true;
                action_object_to_model_object.push(usize_u32(model_object_index)?);
            }
            if self
                .action_refs
                .iter()
                .any(|reference| usize::from(reference.object_index) >= self.action_objects.len())
            {
                return Err(FlatDecisionErrorV2::InvalidReference);
            }
            scorer_action_refs
                .try_reserve(self.action_refs.len())
                .map_err(|_| FlatDecisionErrorV2::CheckedIntegerRange)?;
            for reference in &self.action_refs {
                let model_object_index = *action_object_to_model_object
                    .get(usize::from(reference.object_index))
                    .ok_or(FlatDecisionErrorV2::InvalidReference)?;
                let model_object = self
                    .objects
                    .get(
                        usize::try_from(model_object_index)
                            .map_err(|_| FlatDecisionErrorV2::CheckedIntegerRange)?,
                    )
                    .ok_or(FlatDecisionErrorV2::InvalidReference)?;
                if reference.action_index >= usize_u32(self.actions.len())?
                    || model_object.card_token != reference.card_token
                {
                    return Err(FlatDecisionErrorV2::InvalidReference);
                }
                scorer_action_refs.push(FlatScorerActionRefV2 {
                    action_index: reference.action_index,
                    projection_role_id: flat_action_ref_projection_role_id_v2(reference.role),
                    order_index: reference.order_index,
                    associated_order: reference.associated_order,
                    card_token: reference.card_token,
                    model_object_index,
                });
            }
            Ok(())
        })();
        self.action_object_to_model_object = action_object_to_model_object;
        self.claimed_model_objects = claimed_model_objects;
        self.scorer_actions = scorer_actions;
        self.scorer_action_refs = scorer_action_refs;
        scorer_validation
    }

    #[cfg(test)]
    fn cached_scorer_action_refs_v2(
        &self,
        binding: FlatActionDecisionBindingV2,
    ) -> Result<&[FlatScorerActionRefV2], FlatDecisionErrorV2> {
        if self.cached_binding != Some(binding) {
            return Err(FlatDecisionErrorV2::ScorerBindingMismatch);
        }
        Ok(&self.scorer_action_refs)
    }

    fn ensure_cache(
        &mut self,
        session: &FastActorSessionV1,
        expected: FastActorDecisionV1,
    ) -> Result<FlatActionDecisionBindingV2, FlatDecisionErrorV2> {
        if let Some(binding) = self.cached_binding {
            let describes_expected = binding.episode_id == expected.episode_id
                && binding.environment_revision == expected.environment_revision
                && binding.bound_policy_step_count == expected.step
                && binding.physical_decision_id == expected.physical_decision_id
                && binding.substep_index == expected.substep_index
                && binding.substep_count == expected.substep_count
                && binding.acting_player
                    == match expected.acting_player {
                        PlayerSeatV1::P0 => 0,
                        PlayerSeatV1::P1 => 1,
                    }
                && binding.legal_action_count == expected.legal_action_count;
            if describes_expected {
                session.flat_policy_validate_cached_binding_v2(expected, binding)?;
                return Ok(binding);
            }
        }
        self.build_cache(session, expected)?;
        self.cached_binding
            .ok_or(FlatDecisionErrorV2::ObservationContract)
    }

    fn cached_decision_v2(
        &self,
        action_binding: FlatActionDecisionBindingV2,
    ) -> Result<FlatDecisionV2, FlatDecisionErrorV2> {
        if self.cached_binding != Some(action_binding) {
            return Err(FlatDecisionErrorV2::ScorerBindingMismatch);
        }
        Ok(FlatDecisionV2 {
            binding: FlatDecisionBindingV2 {
                action_binding,
                typed_layout_version: FLAT_POLICY_TYPED_LAYOUT_VERSION_V2,
                feature_inventory_version: FLAT_POLICY_FEATURE_INVENTORY_VERSION_V2,
                enum_mapping_version: FLAT_POLICY_ENUM_MAPPING_VERSION_V2,
                object_group_mapping_version: FLAT_POLICY_OBJECT_GROUP_MAPPING_VERSION_V2,
                relation_role_mapping_version: FLAT_POLICY_RELATION_ROLE_MAPPING_VERSION_V2,
                context_subrole_mapping_version: FLAT_POLICY_CONTEXT_SUBROLE_MAPPING_VERSION_V2,
                action_ref_projection_role_mapping_version:
                    FLAT_ACTION_REF_PROJECTION_ROLE_MAPPING_VERSION_V2,
                contract_digests: FLAT_POLICY_CONTRACT_DIGESTS_V2,
            },
            globals: self.globals,
            active_object_count: usize_u32(self.objects.len())?,
            active_relation_count: usize_u32(self.relations.len())?,
            active_object_subtype_count: usize_u32(self.object_subtypes.len())?,
            active_ability_use_count: usize_u32(self.ability_uses.len())?,
            active_goad_count: usize_u32(self.goads.len())?,
            active_completed_dungeon_count: usize_u32(self.completed_dungeons.len())?,
            active_effect_subtype_change_count: usize_u32(self.effect_subtype_changes.len())?,
            active_context_path_element_count: usize_u32(self.context_path_elements.len())?,
            active_action_count: usize_u32(self.actions.len())?,
            active_action_ref_count: usize_u32(self.action_refs.len())?,
            active_action_object_count: usize_u32(self.action_objects.len())?,
        })
    }
}

#[cfg(test)]
pub(crate) fn encode_observation_owned_tables_for_fixture_v2(
    observation: &ObservationV5,
) -> Result<FlatObservationOwnedTablesV2, FlatDecisionErrorV2> {
    let mut encoder = FlatDecisionEncoderV2::default();
    encoder.build_globals(observation)?;
    encoder.register_objects(observation)?;
    encoder.build_relations(observation)?;
    encoder.validate_cached_tables()?;
    Ok(FlatObservationOwnedTablesV2 {
        globals: encoder.globals,
        objects: encoder.objects,
        relations: encoder.relations,
        object_subtypes: encoder.object_subtypes,
        ability_uses: encoder.ability_uses,
        goads: encoder.goads,
        completed_dungeons: encoder.completed_dungeons,
        effect_subtype_changes: encoder.effect_subtype_changes,
        context_path_elements: encoder.context_path_elements,
    })
}

macro_rules! require_capacity {
    ($buffer:expr, $source:expr, $variant:ident) => {
        if $buffer.len() < $source.len() {
            return Err(FlatDecisionErrorV2::$variant {
                required: $source.len(),
                available: $buffer.len(),
            });
        }
    };
}

impl FastActorSessionV1 {
    /// Produces the complete typed actor-relative model input for the exact
    /// current fast-actor decision.
    ///
    /// Pass one validates the session authority, builds or revalidates the
    /// encoder-owned immutable cache, and checks every destination capacity.
    /// Pass two copies active prefixes only.  Consequently every error leaves
    /// every caller-owned buffer byte-for-byte unchanged, and long-to-short
    /// reuse never clears or publishes stale tails.
    pub fn encode_current_flat_decision_v2(
        &self,
        expected: FastActorDecisionV1,
        encoder: &mut FlatDecisionEncoderV2,
        buffers: &mut FlatDecisionBuffersV2<'_>,
    ) -> Result<FlatDecisionV2, FlatDecisionErrorV2> {
        let action_binding = encoder.ensure_cache(self, expected)?;

        require_capacity!(buffers.objects, encoder.objects, InsufficientObjectCapacity);
        require_capacity!(
            buffers.relations,
            encoder.relations,
            InsufficientRelationCapacity
        );
        require_capacity!(
            buffers.object_subtypes,
            encoder.object_subtypes,
            InsufficientObjectSubtypeCapacity
        );
        require_capacity!(
            buffers.ability_uses,
            encoder.ability_uses,
            InsufficientAbilityUseCapacity
        );
        require_capacity!(buffers.goads, encoder.goads, InsufficientGoadCapacity);
        require_capacity!(
            buffers.completed_dungeons,
            encoder.completed_dungeons,
            InsufficientCompletedDungeonCapacity
        );
        require_capacity!(
            buffers.effect_subtype_changes,
            encoder.effect_subtype_changes,
            InsufficientEffectSubtypeCapacity
        );
        require_capacity!(
            buffers.context_path_elements,
            encoder.context_path_elements,
            InsufficientContextPathCapacity
        );
        require_capacity!(buffers.actions, encoder.actions, InsufficientActionCapacity);
        require_capacity!(
            buffers.action_refs,
            encoder.action_refs,
            InsufficientActionRefCapacity
        );
        require_capacity!(
            buffers.action_objects,
            encoder.action_objects,
            InsufficientActionObjectCapacity
        );

        let decision = encoder.cached_decision_v2(action_binding)?;

        buffers.objects[..encoder.objects.len()].copy_from_slice(&encoder.objects);
        buffers.relations[..encoder.relations.len()].copy_from_slice(&encoder.relations);
        buffers.object_subtypes[..encoder.object_subtypes.len()]
            .copy_from_slice(&encoder.object_subtypes);
        buffers.ability_uses[..encoder.ability_uses.len()].copy_from_slice(&encoder.ability_uses);
        buffers.goads[..encoder.goads.len()].copy_from_slice(&encoder.goads);
        buffers.completed_dungeons[..encoder.completed_dungeons.len()]
            .copy_from_slice(&encoder.completed_dungeons);
        buffers.effect_subtype_changes[..encoder.effect_subtype_changes.len()]
            .copy_from_slice(&encoder.effect_subtype_changes);
        buffers.context_path_elements[..encoder.context_path_elements.len()]
            .copy_from_slice(&encoder.context_path_elements);
        buffers.actions[..encoder.actions.len()].copy_from_slice(&encoder.actions);
        buffers.action_refs[..encoder.action_refs.len()].copy_from_slice(&encoder.action_refs);
        buffers.action_objects[..encoder.action_objects.len()]
            .copy_from_slice(&encoder.action_objects);
        Ok(decision)
    }

    /// Builds the exact same decision as [`Self::encode_current_flat_decision_v2`]
    /// but transfers only scorer-visible tables by ownership. All validation
    /// completes before the first swap, so an error leaves every destination
    /// untouched. The operational action authority tables stay encoder-local.
    pub(crate) fn encode_current_flat_scoring_decision_owned_v2(
        &self,
        expected: FastActorDecisionV1,
        encoder: &mut FlatDecisionEncoderV2,
        buffers: &mut FlatScoringOwnedBuffersV2<'_>,
    ) -> Result<FlatDecisionV2, FlatDecisionErrorV2> {
        let action_binding = encoder.ensure_cache(self, expected)?;
        if encoder.scorer_actions.len() != encoder.actions.len()
            || encoder.scorer_action_refs.len() != encoder.action_refs.len()
        {
            return Err(FlatDecisionErrorV2::ScorerBindingMismatch);
        }
        let decision = encoder.cached_decision_v2(action_binding)?;

        std::mem::swap(&mut encoder.objects, buffers.objects);
        std::mem::swap(&mut encoder.relations, buffers.relations);
        std::mem::swap(&mut encoder.object_subtypes, buffers.object_subtypes);
        std::mem::swap(&mut encoder.ability_uses, buffers.ability_uses);
        std::mem::swap(&mut encoder.goads, buffers.goads);
        std::mem::swap(&mut encoder.completed_dungeons, buffers.completed_dungeons);
        std::mem::swap(
            &mut encoder.effect_subtype_changes,
            buffers.effect_subtype_changes,
        );
        std::mem::swap(
            &mut encoder.context_path_elements,
            buffers.context_path_elements,
        );
        std::mem::swap(&mut encoder.scorer_actions, buffers.actions);
        std::mem::swap(&mut encoder.scorer_action_refs, buffers.action_refs);

        // The cached authority describes tables that are now packet-owned.
        // Force a full rebuild before any subsequent encode or cache read.
        encoder.cached_binding = None;
        Ok(decision)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rl::{
        CardCharacteristicsV2, CardTypeFlagsV2, CountersV1, GoadPublicV4, KeywordFlagsV2,
        StackItemPublicV2,
    };
    use crate::rl_session::{FastActorResponseV1, CANONICAL_BURN_DECK_ID};

    fn expected(session: &FastActorSessionV1) -> FastActorDecisionV1 {
        let FastActorResponseV1::Decision(expected) = session.current_response() else {
            panic!("expected live decision");
        };
        expected
    }

    fn v2_session(episode_id: u64, environment_seed: u64) -> FastActorSessionV1 {
        FastActorSessionV1::reset_with_decks_and_limits_flat_action_v2(
            episode_id,
            environment_seed,
            128,
            16_384,
            [
                CANONICAL_BURN_DECK_ID.to_string(),
                CANONICAL_BURN_DECK_ID.to_string(),
            ],
        )
        .unwrap()
    }

    fn one_row_encoder(
        session: &FastActorSessionV1,
        expected: FastActorDecisionV1,
    ) -> FlatDecisionEncoderV2 {
        let mut actions = vec![FlatActionCoreV1::default(); 64];
        let mut refs = vec![FlatActionRefV2::default(); 256];
        let mut objects = vec![FlatActionObjectV2::default(); 128];
        let slice = session
            .encode_current_flat_action_slice_v2(
                expected,
                &mut FlatActionDecisionSliceBuffersV2 {
                    actions: &mut actions,
                    refs: &mut refs,
                    objects: &mut objects,
                },
            )
            .unwrap();
        FlatDecisionEncoderV2 {
            cached_binding: Some(slice.binding),
            globals: FlatGlobalsV2::default(),
            objects: vec![FlatObjectCoreV2::default()],
            object_keys: vec![None],
            relations: vec![FlatRelationV2::default()],
            object_subtypes: vec![FlatObjectSubtypeV2::default()],
            ability_uses: vec![FlatObjectAbilityUseV2::default()],
            goads: vec![FlatObjectGoadV2::default()],
            completed_dungeons: vec![FlatCompletedDungeonV2::default()],
            effect_subtype_changes: vec![FlatEffectSubtypeChangeV2::default()],
            context_path_elements: vec![FlatContextPathElementV2::default()],
            actions: vec![FlatActionCoreV1::default()],
            action_refs: vec![FlatActionRefV2::default()],
            action_objects: vec![FlatActionObjectV2::default()],
            action_object_to_model_object: Vec::new(),
            claimed_model_objects: Vec::new(),
            scorer_actions: vec![FlatScorerActionCoreV2::default()],
            scorer_action_refs: Vec::new(),
        }
    }

    fn materialize_observation(
        observation: &ObservationV5,
    ) -> Result<FlatDecisionEncoderV2, FlatDecisionErrorV2> {
        let mut encoder = FlatDecisionEncoderV2::default();
        encoder.build_globals(observation)?;
        encoder.register_objects(observation)?;
        encoder.build_relations(observation)?;
        encoder.validate_cached_tables()?;
        Ok(encoder)
    }

    fn synthetic_stable(
        arena_id: u32,
        card_db_id: u16,
        owner: PlayerSeatV1,
        controller: PlayerSeatV1,
        zone: Zone,
    ) -> CardStableRefV1 {
        CardStableRefV1 {
            arena_id,
            card_db_id,
            owner,
            controller,
            zone,
            zone_change_count: 0,
        }
    }

    fn synthetic_public(stable: CardStableRefV1) -> CardPublicV2 {
        CardPublicV2 {
            stable,
            card_name: String::new(),
            tapped: false,
            summoning_sick: false,
            damage: 0,
            counters: CountersV1 {
                plus1_plus1: 0,
                minus1_minus1: 0,
                minus0_minus1: 0,
                stun: 0,
                lore: 0,
            },
            attachments: Vec::new(),
            plotted_turn: None,
            is_token: false,
            face_index: 0,
            chosen_color: None,
            entered_battlefield_turn: None,
            ability_uses_this_turn: Vec::new(),
            skip_next_untap: false,
            goaded_by: Vec::new(),
            characteristics: CardCharacteristicsV2 {
                type_flags: CardTypeFlagsV2 {
                    land: false,
                    creature: true,
                    instant: false,
                    sorcery: false,
                    artifact: false,
                    enchantment: false,
                },
                base_power: Some(1),
                base_toughness: Some(1),
                effective_power: Some(1),
                effective_toughness: Some(1),
                effective_color_mask: 0,
                effective_subtype_ids: Vec::new(),
                effective_keywords: KeywordFlagsV2 {
                    flying: false,
                    reach: false,
                    haste: false,
                    vigilance: false,
                    trample: false,
                    first_strike: false,
                    double_strike: false,
                    deathtouch: false,
                    menace: false,
                    defender: false,
                    lifelink: false,
                    hexproof: false,
                    indestructible: false,
                    protection_from_monocolored: false,
                    ward_generic: 0,
                    minimum_blockers: 0,
                    landwalk_mask: 0,
                },
            },
        }
    }

    fn flip_stable_seats(stable: &mut CardStableRefV1) {
        stable.owner = opponent(stable.owner);
        stable.controller = opponent(stable.controller);
    }

    fn flip_target_seats(target: &mut TargetRefV1) {
        match target {
            TargetRefV1::Player { player } => *player = opponent(*player),
            TargetRefV1::Object { object } => flip_stable_seats(object),
        }
    }

    /// Exact seat transform for the bounded initial-state fixtures in this
    /// module. Their pending/private contexts are empty; every populated
    /// absolute-seat field used by the semantic regression fixtures is
    /// transformed below.
    fn flip_bounded_observation_seats(observation: &mut ObservationV5) {
        let surface = &observation.projection.surface;
        assert!(surface.engine_context.pending_cast.is_none());
        assert!(surface.engine_context.pending_activation.is_none());
        assert!(surface.engine_context.pending_discard.is_none());
        assert!(surface.engine_context.pending_optional_cost.is_none());
        assert!(surface
            .engine_context
            .pending_optional_cost_sacrifice
            .is_none());
        assert!(surface.engine_context.pending_spell_copy.is_none());
        assert!(surface.engine_context.pending_effect.is_none());
        assert!(surface.engine_context.pending_triggers.is_empty());
        assert!(surface.surface_context.private_blockers.is_none());
        assert!(surface.surface_context.private_discard.is_none());
        assert!(surface.surface_context.private_optional_cost.is_none());
        assert!(observation
            .projection
            .policy_surface_context
            .private_combat_selection
            .is_none());

        observation.acting_player = opponent(observation.acting_player);
        let surface = &mut observation.projection.surface;
        surface.active_player = opponent(surface.active_player);
        surface.priority_player = opponent(surface.priority_player);
        surface.initiative = surface.initiative.map(opponent);
        surface.life_totals.swap(0, 1);
        surface.mana_pools.swap(0, 1);
        surface.hand_counts.swap(0, 1);
        surface.library_counts.swap(0, 1);
        surface.player_status.swap(0, 1);
        surface.battlefield.swap(0, 1);
        surface.graveyards.swap(0, 1);
        surface.engine_context.priority_passes.swap(0, 1);
        surface
            .engine_context
            .last_mana_ability_activator_since_priority_boundary = surface
            .engine_context
            .last_mana_ability_activator_since_priority_boundary
            .map(opponent);
        surface.surface_context.combat_priority_spent.swap(0, 1);

        for card in surface
            .battlefield
            .iter_mut()
            .chain(surface.graveyards.iter_mut())
            .flat_map(|zone| zone.iter_mut())
            .chain(surface.exile.iter_mut())
        {
            flip_stable_seats(&mut card.stable);
            for goad in &mut card.goaded_by {
                goad.player = opponent(goad.player);
            }
            card.goaded_by
                .sort_unstable_by_key(|goad| seat_index(goad.player));
        }
        for item in &mut surface.stack {
            flip_stable_seats(&mut item.source);
            item.controller = opponent(item.controller);
            for target in &mut item.targets {
                flip_target_seats(target);
            }
            for paid in &mut item.paid_cost_refs {
                flip_stable_seats(paid);
            }
        }
        for relation in &mut surface.object_relations {
            match relation {
                ObjectRelationPublicV4::AttachedTo {
                    object,
                    attached_to,
                } => {
                    flip_stable_seats(object);
                    flip_stable_seats(attached_to);
                }
                ObjectRelationPublicV4::ExiledBy { object, exiled_by } => {
                    flip_stable_seats(object);
                    flip_stable_seats(exiled_by);
                }
            }
        }
        for attacker in &mut surface.combat.ordered_attackers {
            flip_stable_seats(attacker);
        }
        for (attacker, blockers) in &mut surface.combat.attacker_to_ordered_blockers {
            flip_stable_seats(attacker);
            for blocker in blockers {
                flip_stable_seats(blocker);
            }
        }
        for effect in &mut surface.continuous_effects {
            if let Some(source) = &mut effect.source {
                flip_stable_seats(source);
            }
            effect.controller = effect.controller.map(opponent);
            for affected in &mut effect.affected_objects {
                flip_stable_seats(affected);
            }
            for affected in &mut effect.affected_players {
                *affected = opponent(*affected);
            }
            effect
                .affected_players
                .sort_unstable_by_key(|&player| seat_index(player));
        }
        for permission in &mut surface.exile_play_permissions {
            flip_stable_seats(&mut permission.object);
            permission.holder = opponent(permission.holder);
        }

        for card in &mut observation.own_hand {
            flip_stable_seats(&mut card.stable);
        }
        observation.known_library_cards.swap(0, 1);
        for entries in &mut observation.known_library_cards {
            for entry in entries {
                flip_stable_seats(&mut entry.card.stable);
            }
        }
        observation.known_hand_cards.swap(0, 1);
        for cards in &mut observation.known_hand_cards {
            for card in cards {
                flip_stable_seats(&mut card.stable);
            }
        }
    }

    fn assert_same_public_tables(a: &FlatDecisionEncoderV2, b: &FlatDecisionEncoderV2) {
        assert_eq!(a.globals, b.globals);
        assert_eq!(a.objects, b.objects);
        assert_eq!(a.relations, b.relations);
        assert_eq!(a.object_subtypes, b.object_subtypes);
        assert_eq!(a.ability_uses, b.ability_uses);
        assert_eq!(a.goads, b.goads);
        assert_eq!(a.completed_dungeons, b.completed_dungeons);
        assert_eq!(a.effect_subtype_changes, b.effect_subtype_changes);
        assert_eq!(a.context_path_elements, b.context_path_elements);
    }

    #[test]
    fn historical_stack_target_keeps_announcement_controller_after_live_control_change() {
        let actor = PlayerSeatV1::P0;
        let coalesced_stable = synthetic_stable(90_023, 1, actor, actor, Zone::Battlefield);
        let mut coalescing = FlatDecisionEncoderV2::default();
        let historical_index = coalescing
            .add_stable(
                &coalesced_stable,
                actor,
                FlatObjectGroupV2::HistoricalStackTarget,
                FlatObjectSourceKindV2::Target,
                0,
                HISTORICAL_STACK_TARGET_KIND_V1,
            )
            .unwrap();
        let later_live_index = coalescing
            .add_stable(
                &coalesced_stable,
                actor,
                FlatObjectGroupV2::PendingContext,
                FlatObjectSourceKindV2::Pending,
                1,
                0,
            )
            .unwrap();
        assert_eq!(historical_index, later_live_index);
        assert_eq!(coalescing.objects.len(), 1);

        let session = v2_session(90_024, 124);
        let expected = expected(&session);
        let mut observation = session.flat_policy_observation_v2(expected).unwrap();
        let actor = observation.acting_player;
        let other = opponent(actor);
        let live_target = synthetic_stable(90_024, 1, actor, actor, Zone::Battlefield);
        let stack_source = synthetic_stable(90_025, 2, actor, actor, Zone::Stack);
        observation.projection.surface.battlefield[seat_index(actor)]
            .push(synthetic_public(live_target.clone()));
        let mut historical_target = live_target.clone();
        historical_target.controller = other;
        observation
            .projection
            .surface
            .stack
            .push(StackItemPublicV2 {
                stack_index: 0,
                source: stack_source,
                controller: actor,
                targets: vec![TargetRefV1::Object {
                    object: historical_target.clone(),
                }],
                stack_item_kind: StackItemKindV2::Spell,
                is_copy: false,
                is_flashback: false,
                mode_chosen: 0,
                madness_offer: false,
                kicked: false,
                cast_method: Some(CastMethodV4::Normal),
                face_index: 0,
                x_value: 0,
                paid_cost_refs: Vec::new(),
            });

        let changed_controller = materialize_observation(&observation).unwrap();
        let target_relation = changed_controller
            .relations
            .iter()
            .find(|relation| {
                relation.role == FlatRelationRoleV2::StackTarget && relation.secondary_order == 1
            })
            .unwrap();
        let target_object = target_relation.target_object.unwrap();
        assert_eq!(
            changed_controller.objects[usize::try_from(target_object).unwrap()].group,
            FlatObjectGroupV2::SelfBattlefield
        );
        assert!(matches!(
            target_relation.payload,
            FlatRelationPayloadV2::Stack(FlatStackRelationDataV2 {
                target_object_controller: FlatRelativePlayerV2::Opponent,
                ..
            })
        ));

        let mut seat_swapped = observation.clone();
        flip_bounded_observation_seats(&mut seat_swapped);
        let seat_swapped = materialize_observation(&seat_swapped).unwrap();
        assert_same_public_tables(&changed_controller, &seat_swapped);

        let mut live_controller = observation.clone();
        let TargetRefV1::Object { object } =
            &mut live_controller.projection.surface.stack[0].targets[0]
        else {
            unreachable!()
        };
        object.controller = actor;
        let live_controller = materialize_observation(&live_controller).unwrap();
        assert_eq!(changed_controller.objects, live_controller.objects);
        assert_ne!(changed_controller.relations, live_controller.relations);

        for conflict in 0..3 {
            let mut invalid = observation.clone();
            let TargetRefV1::Object { object } =
                &mut invalid.projection.surface.stack[0].targets[0]
            else {
                unreachable!()
            };
            match conflict {
                0 => object.card_db_id ^= 1,
                1 => object.owner = other,
                2 => object.zone = Zone::Graveyard,
                _ => unreachable!(),
            }
            let error = match materialize_observation(&invalid) {
                Ok(_) => panic!("historical target conflict {conflict} was accepted"),
                Err(error) => error,
            };
            assert_eq!(error, FlatDecisionErrorV2::InconsistentReference);
        }

        for detached_zone in [
            Zone::Library,
            Zone::Hand,
            Zone::Graveyard,
            Zone::Exile,
            Zone::Command,
        ] {
            let mut invalid = observation.clone();
            invalid.projection.surface.battlefield[seat_index(actor)].clear();
            let TargetRefV1::Object { object } =
                &mut invalid.projection.surface.stack[0].targets[0]
            else {
                unreachable!()
            };
            object.zone = detached_zone;
            let error = match materialize_observation(&invalid) {
                Ok(_) => panic!("detached historical target in {detached_zone:?} was accepted"),
                Err(error) => error,
            };
            assert_eq!(error, FlatDecisionErrorV2::InvalidReference);
        }
    }

    #[test]
    fn overlapping_stack_target_and_paid_cost_use_distinct_historical_rows() {
        let session = v2_session(90_026, 126);
        let expected = expected(&session);
        let mut observation = session.flat_policy_observation_v2(expected).unwrap();
        let actor = observation.acting_player;
        let other = opponent(actor);
        let stack_source = synthetic_stable(90_026, 1, actor, actor, Zone::Stack);
        let historical_target = synthetic_stable(90_027, 2, actor, other, Zone::Battlefield);
        let mut paid_cost = historical_target.clone();
        paid_cost.controller = actor;
        observation
            .projection
            .surface
            .stack
            .push(StackItemPublicV2 {
                stack_index: 0,
                source: stack_source,
                controller: actor,
                targets: vec![TargetRefV1::Object {
                    object: historical_target,
                }],
                stack_item_kind: StackItemKindV2::Spell,
                is_copy: false,
                is_flashback: false,
                mode_chosen: 0,
                madness_offer: false,
                kicked: false,
                cast_method: Some(CastMethodV4::Normal),
                face_index: 0,
                x_value: 0,
                paid_cost_refs: vec![paid_cost],
            });

        let encoded = materialize_observation(&observation).unwrap();
        let target_index = encoded
            .relations
            .iter()
            .find(|relation| {
                relation.role == FlatRelationRoleV2::StackTarget && relation.secondary_order == 1
            })
            .and_then(|relation| relation.target_object)
            .unwrap();
        let paid_index = encoded
            .relations
            .iter()
            .find(|relation| relation.role == FlatRelationRoleV2::PaidCost)
            .and_then(|relation| relation.target_object)
            .unwrap();

        assert_ne!(target_index, paid_index);
        assert_eq!(
            encoded.objects[usize::try_from(target_index).unwrap()].group,
            FlatObjectGroupV2::HistoricalStackTarget
        );
        assert_eq!(
            encoded.objects[usize::try_from(paid_index).unwrap()].group,
            FlatObjectGroupV2::HistoricalPaidCost
        );
        assert_eq!(
            encoded.objects[usize::try_from(target_index).unwrap()].controller,
            FlatRelativePlayerV2::Opponent
        );
        assert_eq!(
            encoded.objects[usize::try_from(paid_index).unwrap()].controller,
            FlatRelativePlayerV2::SelfPlayer
        );

        for conflict in 0..3 {
            let mut invalid = observation.clone();
            let paid = &mut invalid.projection.surface.stack[0].paid_cost_refs[0];
            match conflict {
                0 => paid.card_db_id ^= 1,
                1 => paid.owner = other,
                2 => paid.zone = Zone::Stack,
                _ => unreachable!(),
            }
            let error = match materialize_observation(&invalid) {
                Ok(_) => panic!("cross-kind immutable conflict {conflict} was accepted"),
                Err(error) => error,
            };
            assert_eq!(error, FlatDecisionErrorV2::InconsistentReference);
        }
    }

    #[test]
    fn paid_cost_controller_conflict_with_live_object_fails_at_resolver() {
        let session = v2_session(90_027, 127);
        let expected = expected(&session);
        let mut observation = session.flat_policy_observation_v2(expected).unwrap();
        let actor = observation.acting_player;
        let other = opponent(actor);
        let live = synthetic_stable(90_028, 2, actor, actor, Zone::Battlefield);
        observation.projection.surface.battlefield[seat_index(actor)]
            .push(synthetic_public(live.clone()));
        let mut conflicting_paid_cost = live;
        conflicting_paid_cost.controller = other;
        observation
            .projection
            .surface
            .stack
            .push(StackItemPublicV2 {
                stack_index: u32::try_from(observation.projection.surface.stack.len()).unwrap(),
                source: synthetic_stable(90_029, 3, actor, actor, Zone::Stack),
                controller: actor,
                targets: Vec::new(),
                stack_item_kind: StackItemKindV2::Spell,
                is_copy: false,
                is_flashback: false,
                mode_chosen: 0,
                madness_offer: false,
                kicked: false,
                cast_method: Some(CastMethodV4::Normal),
                face_index: 0,
                x_value: 0,
                paid_cost_refs: vec![conflicting_paid_cost],
            });

        let error = match materialize_observation(&observation) {
            Ok(_) => panic!("live-versus-paid-cost controller conflict was accepted"),
            Err(error) => error,
        };
        assert_eq!(error, FlatDecisionErrorV2::InconsistentReference);
    }

    #[test]
    fn set_like_relation_inputs_have_one_canonical_typed_order() {
        let session = v2_session(90_025, 125);
        let expected = expected(&session);
        let mut observation = session.flat_policy_observation_v2(expected).unwrap();
        let actor = observation.acting_player;
        let other = opponent(actor);
        let turn = observation.projection.surface.turn;
        let self_host = synthetic_stable(91_000, 1, actor, actor, Zone::Battlefield);
        let self_attachment = synthetic_stable(91_001, 2, actor, actor, Zone::Battlefield);
        let other_host = synthetic_stable(91_002, 3, other, other, Zone::Battlefield);
        let other_attachment = synthetic_stable(91_003, 4, other, other, Zone::Battlefield);
        let exile_a = synthetic_stable(91_004, 5, actor, actor, Zone::Exile);
        let exile_b = synthetic_stable(91_005, 6, actor, actor, Zone::Exile);

        let mut self_host_card = synthetic_public(self_host.clone());
        self_host_card.attachments = vec![other_attachment.arena_id, self_attachment.arena_id];
        self_host_card.goaded_by = vec![
            GoadPublicV4 {
                player: PlayerSeatV1::P0,
                expires_at_turn: turn + 1,
            },
            GoadPublicV4 {
                player: PlayerSeatV1::P1,
                expires_at_turn: turn + 1,
            },
        ];
        observation.projection.surface.battlefield[seat_index(actor)] =
            vec![self_host_card, synthetic_public(self_attachment.clone())];
        observation.projection.surface.battlefield[seat_index(other)] = vec![
            synthetic_public(other_host.clone()),
            synthetic_public(other_attachment.clone()),
        ];
        observation.projection.surface.exile = vec![
            synthetic_public(exile_a.clone()),
            synthetic_public(exile_b.clone()),
        ];
        observation.projection.surface.object_relations = vec![
            ObjectRelationPublicV4::ExiledBy {
                object: self_attachment.clone(),
                exiled_by: other_host.clone(),
            },
            ObjectRelationPublicV4::AttachedTo {
                object: other_attachment.clone(),
                attached_to: self_host.clone(),
            },
        ];
        observation
            .projection
            .surface
            .continuous_effects
            .push(ContinuousEffectPublicV2 {
                source: Some(self_host.clone()),
                controller: Some(actor),
                affected_objects: vec![other_host.clone(), self_attachment.clone()],
                affected_players: vec![PlayerSeatV1::P0, PlayerSeatV1::P1],
                global: false,
                layers: 1,
                timestamp: 1,
                duration: EffectDurationV2::EndOfTurn,
                power_delta: 1,
                toughness_delta: 0,
                grants_haste: false,
                set_power: None,
                set_toughness: None,
                add_color_mask: 0,
                remove_color_mask: 0,
                add_subtype_ids: Vec::new(),
                remove_subtype_ids: Vec::new(),
                add_keyword_mask: 0,
                remove_keyword_mask: 0,
                ward_generic_delta: 0,
                minimum_blockers: None,
                add_landwalk_mask: 0,
                remove_landwalk_mask: 0,
                prevent_damage_from_color_mask: 0,
                damage_cannot_be_prevented: false,
            });
        observation.projection.surface.exile_play_permissions = vec![
            ExilePlayPermissionPublicV2 {
                object: exile_b.clone(),
                holder: other,
                play_or_cast: PlayOrCastV2::Cast,
                zone_change_generation: exile_b.zone_change_count,
                expiry: PlayPermissionExpiryV2::UntilHoldersNextTurn {
                    holder_turn_started: true,
                },
            },
            ExilePlayPermissionPublicV2 {
                object: exile_a.clone(),
                holder: actor,
                play_or_cast: PlayOrCastV2::Play,
                zone_change_generation: exile_a.zone_change_count,
                expiry: PlayPermissionExpiryV2::EndOfTurn,
            },
            ExilePlayPermissionPublicV2 {
                object: exile_a.clone(),
                holder: actor,
                play_or_cast: PlayOrCastV2::Play,
                zone_change_generation: exile_a.zone_change_count,
                expiry: PlayPermissionExpiryV2::UntilHoldersNextTurn {
                    holder_turn_started: true,
                },
            },
            ExilePlayPermissionPublicV2 {
                object: exile_a.clone(),
                holder: other,
                play_or_cast: PlayOrCastV2::Play,
                zone_change_generation: exile_a.zone_change_count,
                expiry: PlayPermissionExpiryV2::EndOfTurn,
            },
            ExilePlayPermissionPublicV2 {
                object: exile_a.clone(),
                holder: actor,
                play_or_cast: PlayOrCastV2::Cast,
                zone_change_generation: exile_a.zone_change_count,
                expiry: PlayPermissionExpiryV2::EndOfTurn,
            },
            ExilePlayPermissionPublicV2 {
                object: exile_a.clone(),
                holder: actor,
                play_or_cast: PlayOrCastV2::Play,
                zone_change_generation: exile_a.zone_change_count,
                expiry: PlayPermissionExpiryV2::UntilHoldersNextTurn {
                    holder_turn_started: false,
                },
            },
        ];
        let baseline = materialize_observation(&observation).unwrap();
        let exile_a_index = baseline
            .objects
            .iter()
            .position(|object| {
                object.group == FlatObjectGroupV2::Exile
                    && object.card_token == card_token(exile_a.card_db_id)
            })
            .map(|index| u32::try_from(index).unwrap())
            .unwrap();
        let permission_tie_breaks: Vec<_> = baseline
            .relations
            .iter()
            .filter(|relation| {
                relation.role == FlatRelationRoleV2::Permission
                    && relation.target_object == Some(exile_a_index)
            })
            .map(|relation| match relation.payload {
                FlatRelationPayloadV2::Permission(payload) => (
                    payload.holder,
                    payload.play_or_cast,
                    payload.expiry,
                    payload.holder_turn_started,
                ),
                _ => unreachable!(),
            })
            .collect();
        assert_eq!(
            permission_tie_breaks,
            vec![
                (FlatRelativePlayerV2::Opponent, 0, 0, false),
                (FlatRelativePlayerV2::SelfPlayer, 1, 0, false),
                (FlatRelativePlayerV2::SelfPlayer, 0, 1, false),
                (FlatRelativePlayerV2::SelfPlayer, 0, 1, true),
                (FlatRelativePlayerV2::SelfPlayer, 0, 0, false),
            ]
        );

        let mut seat_swapped = observation.clone();
        flip_bounded_observation_seats(&mut seat_swapped);
        let seat_swapped = materialize_observation(&seat_swapped).unwrap();
        assert_same_public_tables(&baseline, &seat_swapped);

        let mut reordered = observation;
        reordered.projection.surface.battlefield[seat_index(actor)][0]
            .attachments
            .reverse();
        reordered.projection.surface.object_relations.reverse();
        reordered.projection.surface.continuous_effects[0]
            .affected_objects
            .reverse();
        reordered
            .projection
            .surface
            .exile_play_permissions
            .reverse();
        let canonical = materialize_observation(&reordered).unwrap();
        assert_same_public_tables(&baseline, &canonical);
    }

    #[test]
    fn attachment_and_goad_rows_are_exact_under_valid_actor_seat_swap() {
        let session = v2_session(90_026, 126);
        let expected = expected(&session);
        let mut observation = session.flat_policy_observation_v2(expected).unwrap();
        let actor = observation.acting_player;
        let other = opponent(actor);
        let turn = observation.projection.surface.turn;
        let self_host = synthetic_stable(92_000, 1, actor, actor, Zone::Battlefield);
        let self_attachment = synthetic_stable(92_001, 2, actor, actor, Zone::Battlefield);
        let other_host = synthetic_stable(92_002, 3, other, other, Zone::Battlefield);
        let other_attachment = synthetic_stable(92_003, 4, other, other, Zone::Battlefield);
        let goads = vec![
            GoadPublicV4 {
                player: PlayerSeatV1::P0,
                expires_at_turn: turn + 1,
            },
            GoadPublicV4 {
                player: PlayerSeatV1::P1,
                expires_at_turn: turn + 1,
            },
        ];
        let mut self_host_card = synthetic_public(self_host);
        self_host_card.attachments = vec![self_attachment.arena_id];
        self_host_card.goaded_by = goads.clone();
        let mut other_host_card = synthetic_public(other_host);
        other_host_card.attachments = vec![other_attachment.arena_id];
        other_host_card.goaded_by = goads;
        observation.projection.surface.battlefield[seat_index(actor)] =
            vec![self_host_card, synthetic_public(self_attachment)];
        observation.projection.surface.battlefield[seat_index(other)] =
            vec![other_host_card, synthetic_public(other_attachment)];
        let baseline = materialize_observation(&observation).unwrap();

        let mut swapped = observation;
        flip_bounded_observation_seats(&mut swapped);

        let transformed = materialize_observation(&swapped).unwrap();
        assert_same_public_tables(&baseline, &transformed);
    }

    #[test]
    fn forbidden_names_hashes_and_raw_reference_ids_never_reach_public_rows() {
        let session = v2_session(90_021, 121);
        let expected = expected(&session);
        let observation = session.flat_policy_observation_v2(expected).unwrap();
        let baseline = materialize_observation(&observation).unwrap();
        let mut mutated = observation;
        mutated.kernel_version = "forbidden-kernel-text-mutation".to_string();
        mutated.visible_projection_hash ^= u64::MAX;
        mutated.own_hand[0].card_name = "forbidden-card-name-mutation".to_string();
        mutated.own_hand[0].stable.arena_id ^= u32::MAX;
        mutated.own_hand[0].stable.zone_change_count ^= u32::MAX;
        let changed = materialize_observation(&mutated).unwrap();
        assert_same_public_tables(&baseline, &changed);

        mutated.own_hand[0].stable.card_db_id ^= 1;
        let model_identity_changed = materialize_observation(&mutated).unwrap();
        assert_ne!(baseline.objects, model_identity_changed.objects);
    }

    fn effect_signature(
        encoder: &FlatDecisionEncoderV2,
    ) -> (Vec<FlatRelationV2>, Vec<FlatEffectSubtypeChangeV2>) {
        (
            encoder
                .relations
                .iter()
                .copied()
                .filter(|row| {
                    matches!(
                        row.role,
                        FlatRelationRoleV2::EffectSource | FlatRelationRoleV2::EffectAffected
                    )
                })
                .collect(),
            encoder.effect_subtype_changes.clone(),
        )
    }

    #[test]
    fn every_model_effect_field_is_explicit_while_timestamp_is_operational_only() {
        let session = v2_session(90_022, 122);
        let expected = expected(&session);
        let mut observation = session.flat_policy_observation_v2(expected).unwrap();
        let source = observation.own_hand[0].stable.clone();
        observation
            .projection
            .surface
            .continuous_effects
            .push(ContinuousEffectPublicV2 {
                source: Some(source.clone()),
                controller: Some(observation.acting_player),
                affected_objects: vec![source],
                affected_players: vec![opponent(observation.acting_player)],
                global: false,
                layers: 1,
                timestamp: 7,
                duration: EffectDurationV2::EndOfTurn,
                power_delta: 1,
                toughness_delta: 2,
                grants_haste: false,
                set_power: None,
                set_toughness: None,
                add_color_mask: 1,
                remove_color_mask: 2,
                add_subtype_ids: vec![3],
                remove_subtype_ids: vec![4],
                add_keyword_mask: 8,
                remove_keyword_mask: 16,
                ward_generic_delta: 1,
                minimum_blockers: None,
                add_landwalk_mask: 1,
                remove_landwalk_mask: 2,
                prevent_damage_from_color_mask: 4,
                damage_cannot_be_prevented: false,
            });
        let baseline = effect_signature(&materialize_observation(&observation).unwrap());
        for mutation in 0..22 {
            let mut changed = observation.clone();
            let effect = &mut changed.projection.surface.continuous_effects[0];
            match mutation {
                0 => effect.source = None,
                1 => effect.controller = None,
                2 => effect.affected_objects.clear(),
                3 => effect.affected_players.clear(),
                4 => effect.global = true,
                5 => effect.layers += 1,
                6 => effect.duration = EffectDurationV2::WhileSourcePresent,
                7 => effect.power_delta += 1,
                8 => effect.toughness_delta += 1,
                9 => effect.grants_haste = true,
                10 => effect.set_power = Some(3),
                11 => effect.set_toughness = Some(4),
                12 => effect.add_color_mask ^= 4,
                13 => effect.remove_color_mask ^= 4,
                14 => effect.add_subtype_ids.push(5),
                15 => effect.remove_subtype_ids.push(6),
                16 => effect.add_keyword_mask ^= 32,
                17 => effect.remove_keyword_mask ^= 64,
                18 => effect.ward_generic_delta += 1,
                19 => effect.minimum_blockers = Some(2),
                20 => effect.add_landwalk_mask ^= 4,
                21 => effect.remove_landwalk_mask ^= 4,
                _ => unreachable!(),
            }
            assert_ne!(
                baseline,
                effect_signature(&materialize_observation(&changed).unwrap()),
                "effect mutation {mutation}"
            );
        }
        for mutation in 0..2 {
            let mut changed = observation.clone();
            let effect = &mut changed.projection.surface.continuous_effects[0];
            if mutation == 0 {
                effect.prevent_damage_from_color_mask ^= 8;
            } else {
                effect.damage_cannot_be_prevented = true;
            }
            assert_ne!(
                baseline,
                effect_signature(&materialize_observation(&changed).unwrap())
            );
        }
        let mut timestamp_only = observation.clone();
        timestamp_only.projection.surface.continuous_effects[0].timestamp ^= u64::MAX;
        assert_eq!(
            baseline,
            effect_signature(&materialize_observation(&timestamp_only).unwrap())
        );

        let effect = &mut observation.projection.surface.continuous_effects[0];
        effect.source = None;
        effect.affected_objects.clear();
        effect.global = false;
        let player_only = materialize_observation(&observation).unwrap();
        assert!(player_only.relations.iter().any(|row| {
            row.role == FlatRelationRoleV2::EffectAffected
                && row.target_object.is_none()
                && matches!(
                    row.payload,
                    FlatRelationPayloadV2::Effect(FlatEffectRelationDataV2 {
                        affected_player: FlatRelativePlayerV2::Opponent,
                        ..
                    })
                )
        }));
    }

    #[test]
    fn actor_seat_swap_preserves_the_actor_relative_initial_state_and_effect_rows() {
        let session = v2_session(90_023, 123);
        let expected = expected(&session);
        let mut observation = session.flat_policy_observation_v2(expected).unwrap();
        let actor = observation.acting_player;
        let source = observation.own_hand[0].stable.clone();

        // Keep this transform deliberately bounded to an initial projection so
        // every absolute-seat field below is accounted for explicitly.
        let surface = &observation.projection.surface;
        assert!(surface.battlefield.iter().all(Vec::is_empty));
        assert!(surface.graveyards.iter().all(Vec::is_empty));
        assert!(surface.exile.is_empty());
        assert!(surface.stack.is_empty());
        assert!(surface.combat.ordered_attackers.is_empty());
        assert!(surface.combat.attacker_to_ordered_blockers.is_empty());
        assert!(surface.object_relations.is_empty());
        assert!(surface.exile_play_permissions.is_empty());
        assert!(surface.engine_context.pending_cast.is_none());
        assert!(surface.engine_context.pending_activation.is_none());
        assert!(surface.engine_context.pending_discard.is_none());
        assert!(surface.engine_context.pending_optional_cost.is_none());
        assert!(surface
            .engine_context
            .pending_optional_cost_sacrifice
            .is_none());
        assert!(surface.engine_context.pending_spell_copy.is_none());
        assert!(surface.engine_context.pending_effect.is_none());
        assert!(surface.engine_context.pending_triggers.is_empty());
        assert!(surface.surface_context.private_blockers.is_none());
        assert!(surface.surface_context.private_discard.is_none());
        assert!(surface.surface_context.private_optional_cost.is_none());
        assert!(observation
            .projection
            .policy_surface_context
            .private_combat_selection
            .is_none());
        assert!(observation.known_library_cards.iter().all(Vec::is_empty));
        assert!(observation.known_hand_cards.iter().all(Vec::is_empty));

        observation
            .projection
            .surface
            .continuous_effects
            .push(ContinuousEffectPublicV2 {
                source: Some(source.clone()),
                controller: Some(actor),
                affected_objects: vec![source],
                affected_players: vec![PlayerSeatV1::P0, PlayerSeatV1::P1],
                global: false,
                layers: 1,
                timestamp: 19,
                duration: EffectDurationV2::EndOfTurn,
                power_delta: 1,
                toughness_delta: 0,
                grants_haste: false,
                set_power: None,
                set_toughness: None,
                add_color_mask: 0,
                remove_color_mask: 0,
                add_subtype_ids: Vec::new(),
                remove_subtype_ids: Vec::new(),
                add_keyword_mask: 0,
                remove_keyword_mask: 0,
                ward_generic_delta: 0,
                minimum_blockers: None,
                add_landwalk_mask: 0,
                remove_landwalk_mask: 0,
                prevent_damage_from_color_mask: 0,
                damage_cannot_be_prevented: false,
            });
        let baseline = materialize_observation(&observation).unwrap();

        let mut swapped = observation;
        flip_bounded_observation_seats(&mut swapped);

        let transformed = materialize_observation(&swapped).unwrap();
        assert_same_public_tables(&baseline, &transformed);
        assert_eq!(
            baseline.globals.acting_player,
            FlatRelativePlayerV2::SelfPlayer
        );
        assert!(baseline
            .objects
            .iter()
            .filter(|object| object.group == FlatObjectGroupV2::SelfHand)
            .all(|object| {
                object.owner == FlatRelativePlayerV2::SelfPlayer
                    && object.controller == FlatRelativePlayerV2::SelfPlayer
            }));
        assert!(baseline.relations.iter().any(|relation| {
            matches!(
                relation.payload,
                FlatRelationPayloadV2::Effect(FlatEffectRelationDataV2 {
                    controller: FlatRelativePlayerV2::SelfPlayer,
                    affected_player: FlatRelativePlayerV2::Opponent,
                    ..
                })
            )
        }));
    }

    #[test]
    fn scorer_action_refs_are_exactly_remapped_and_binding_checked() {
        let session = v2_session(90_019, 119);
        let expected = expected(&session);
        let mut encoder = FlatDecisionEncoderV2::default();
        let mut objects = vec![FlatObjectCoreV2::default(); 512];
        let mut relations = vec![FlatRelationV2::default(); 2_048];
        let mut object_subtypes = vec![FlatObjectSubtypeV2::default(); 2_048];
        let mut ability_uses = vec![FlatObjectAbilityUseV2::default(); 512];
        let mut goads = vec![FlatObjectGoadV2::default(); 512];
        let mut completed_dungeons = vec![FlatCompletedDungeonV2::default(); 128];
        let mut effect_subtype_changes = vec![FlatEffectSubtypeChangeV2::default(); 512];
        let mut context_path_elements = vec![FlatContextPathElementV2::default(); 512];
        let mut actions = vec![FlatActionCoreV1::default(); 128];
        let mut action_refs = vec![FlatActionRefV2::default(); 1_024];
        let mut action_objects = vec![FlatActionObjectV2::default(); 1_024];
        let encoded = session
            .encode_current_flat_decision_v2(
                expected,
                &mut encoder,
                &mut FlatDecisionBuffersV2 {
                    objects: &mut objects,
                    relations: &mut relations,
                    object_subtypes: &mut object_subtypes,
                    ability_uses: &mut ability_uses,
                    goads: &mut goads,
                    completed_dungeons: &mut completed_dungeons,
                    effect_subtype_changes: &mut effect_subtype_changes,
                    context_path_elements: &mut context_path_elements,
                    actions: &mut actions,
                    action_refs: &mut action_refs,
                    action_objects: &mut action_objects,
                },
            )
            .unwrap();
        let safe_refs = encoder
            .cached_scorer_action_refs_v2(encoded.binding.action_binding)
            .unwrap();
        assert_eq!(safe_refs.len(), encoded.active_action_ref_count as usize);
        assert!(!safe_refs.is_empty());
        for (operational, safe) in action_refs.iter().zip(safe_refs) {
            let model_object = objects[safe.model_object_index as usize];
            assert_eq!(safe.action_index, operational.action_index);
            assert_eq!(
                safe.projection_role_id,
                flat_action_ref_projection_role_id_v2(operational.role)
            );
            assert_eq!(safe.order_index, operational.order_index);
            assert_eq!(safe.associated_order, operational.associated_order);
            assert_eq!(safe.card_token, operational.card_token);
            assert_eq!(model_object.card_token, safe.card_token);
        }

        let mut stale = encoded.binding.action_binding;
        stale.bound_policy_step_count ^= 1;
        assert_eq!(
            encoder.cached_scorer_action_refs_v2(stale),
            Err(FlatDecisionErrorV2::ScorerBindingMismatch)
        );
    }

    #[test]
    fn scorer_action_projection_is_field_exact_and_kind_exhaustive() {
        let action = FlatActionCoreV1 {
            kind: FlatActionKindV1::OrderTriggers,
            flags: 0x1234,
            ability_index: 1,
            remaining: 2,
            mode_index: 3,
            mode_count: 4,
            option_index: 5,
            option_count: 6,
            selected_count: 7,
            min_targets: 8,
            max_targets: 9,
            number: -10,
            minimum: -11,
            maximum: 12,
            mana_choice: 13,
            color: 14,
            cast_mode: 15,
            cost_kind: 16,
            optional_cost_choice: 17,
            target_kind: 18,
            target_player: 19,
            ref_start: 20,
            ref_len: 21,
        };
        assert_eq!(
            FlatScorerActionCoreV2::from(action),
            FlatScorerActionCoreV2 {
                kind: FlatScorerActionKindV2::OrderTriggers,
                flags: 0x1234,
                ability_index: 1,
                remaining: 2,
                mode_index: 3,
                mode_count: 4,
                option_index: 5,
                option_count: 6,
                selected_count: 7,
                min_targets: 8,
                max_targets: 9,
                number: -10,
                minimum: -11,
                maximum: 12,
                mana_choice: 13,
                color: 14,
                cast_mode: 15,
                cost_kind: 16,
                optional_cost_choice: 17,
                target_kind: 18,
                target_player: 19,
                ref_start: 20,
                ref_len: 21,
            }
        );

        let kinds = [
            (FlatActionKindV1::Pass, FlatScorerActionKindV2::Pass),
            (FlatActionKindV1::PlayLand, FlatScorerActionKindV2::PlayLand),
            (
                FlatActionKindV1::CastSpell,
                FlatScorerActionKindV2::CastSpell,
            ),
            (
                FlatActionKindV1::ActivateManaAbility,
                FlatScorerActionKindV2::ActivateManaAbility,
            ),
            (
                FlatActionKindV1::ActivateAbility,
                FlatScorerActionKindV2::ActivateAbility,
            ),
            (
                FlatActionKindV1::PlotSpell,
                FlatScorerActionKindV2::PlotSpell,
            ),
            (
                FlatActionKindV1::ChooseTarget,
                FlatScorerActionKindV2::ChooseTarget,
            ),
            (
                FlatActionKindV1::ChooseCostTarget,
                FlatScorerActionKindV2::ChooseCostTarget,
            ),
            (
                FlatActionKindV1::ChooseCastMode,
                FlatScorerActionKindV2::ChooseCastMode,
            ),
            (
                FlatActionKindV1::ChooseKicker,
                FlatScorerActionKindV2::ChooseKicker,
            ),
            (
                FlatActionKindV1::ChooseSpellMode,
                FlatScorerActionKindV2::ChooseSpellMode,
            ),
            (
                FlatActionKindV1::ChooseEffectOption,
                FlatScorerActionKindV2::ChooseEffectOption,
            ),
            (
                FlatActionKindV1::ChooseEffectTarget,
                FlatScorerActionKindV2::ChooseEffectTarget,
            ),
            (
                FlatActionKindV1::FinishEffectSelection,
                FlatScorerActionKindV2::FinishEffectSelection,
            ),
            (
                FlatActionKindV1::ChooseEffectColor,
                FlatScorerActionKindV2::ChooseEffectColor,
            ),
            (
                FlatActionKindV1::ChooseEffectNumber,
                FlatScorerActionKindV2::ChooseEffectNumber,
            ),
            (
                FlatActionKindV1::ChooseEffectBoolean,
                FlatScorerActionKindV2::ChooseEffectBoolean,
            ),
            (
                FlatActionKindV1::FinishTargetSelection,
                FlatScorerActionKindV2::FinishTargetSelection,
            ),
            (
                FlatActionKindV1::ChooseOptionalCostUse,
                FlatScorerActionKindV2::ChooseOptionalCostUse,
            ),
            (
                FlatActionKindV1::ChooseOptionalCostWhich,
                FlatScorerActionKindV2::ChooseOptionalCostWhich,
            ),
            (
                FlatActionKindV1::ChooseSpellCopyPayment,
                FlatScorerActionKindV2::ChooseSpellCopyPayment,
            ),
            (
                FlatActionKindV1::ChooseSpellCopyRetarget,
                FlatScorerActionKindV2::ChooseSpellCopyRetarget,
            ),
            (
                FlatActionKindV1::ChooseMadnessCast,
                FlatScorerActionKindV2::ChooseMadnessCast,
            ),
            (FlatActionKindV1::Discard, FlatScorerActionKindV2::Discard),
            (
                FlatActionKindV1::ChooseAttackerInclusion,
                FlatScorerActionKindV2::ChooseAttackerInclusion,
            ),
            (
                FlatActionKindV1::ChooseBlockerInclusion,
                FlatScorerActionKindV2::ChooseBlockerInclusion,
            ),
            (
                FlatActionKindV1::OrderTriggers,
                FlatScorerActionKindV2::OrderTriggers,
            ),
        ];
        for (index, (operational, scorer)) in kinds.into_iter().enumerate() {
            assert_eq!(FlatScorerActionKindV2::from(operational), scorer);
            assert_eq!(scorer as usize, index);
        }
    }

    #[test]
    fn owned_scoring_encode_swaps_validated_tables_and_reuses_allocations() {
        let session = v2_session(90_030, 130);
        let expected = expected(&session);
        let mut encoder = FlatDecisionEncoderV2::default();
        let mut objects = Vec::new();
        let mut relations = Vec::new();
        let mut object_subtypes = Vec::new();
        let mut ability_uses = Vec::new();
        let mut goads = Vec::new();
        let mut completed_dungeons = Vec::new();
        let mut effect_subtype_changes = Vec::new();
        let mut context_path_elements = Vec::new();
        let mut actions = Vec::new();
        let mut action_refs = Vec::new();

        let first = session
            .encode_current_flat_scoring_decision_owned_v2(
                expected,
                &mut encoder,
                &mut FlatScoringOwnedBuffersV2 {
                    objects: &mut objects,
                    relations: &mut relations,
                    object_subtypes: &mut object_subtypes,
                    ability_uses: &mut ability_uses,
                    goads: &mut goads,
                    completed_dungeons: &mut completed_dungeons,
                    effect_subtype_changes: &mut effect_subtype_changes,
                    context_path_elements: &mut context_path_elements,
                    actions: &mut actions,
                    action_refs: &mut action_refs,
                },
            )
            .unwrap();
        assert!(!objects.is_empty());
        assert!(!actions.is_empty());
        let first_object_allocation = objects.as_ptr();
        let first_action_allocation = actions.as_ptr();
        assert_eq!(
            encoder.cached_scorer_action_refs_v2(first.binding.action_binding),
            Err(FlatDecisionErrorV2::ScorerBindingMismatch)
        );

        let second = session
            .encode_current_flat_scoring_decision_owned_v2(
                expected,
                &mut encoder,
                &mut FlatScoringOwnedBuffersV2 {
                    objects: &mut objects,
                    relations: &mut relations,
                    object_subtypes: &mut object_subtypes,
                    ability_uses: &mut ability_uses,
                    goads: &mut goads,
                    completed_dungeons: &mut completed_dungeons,
                    effect_subtype_changes: &mut effect_subtype_changes,
                    context_path_elements: &mut context_path_elements,
                    actions: &mut actions,
                    action_refs: &mut action_refs,
                },
            )
            .unwrap();
        assert_eq!(second, first);
        assert_eq!(encoder.objects.as_ptr(), first_object_allocation);
        assert_eq!(encoder.scorer_actions.as_ptr(), first_action_allocation);

        let third = session
            .encode_current_flat_scoring_decision_owned_v2(
                expected,
                &mut encoder,
                &mut FlatScoringOwnedBuffersV2 {
                    objects: &mut objects,
                    relations: &mut relations,
                    object_subtypes: &mut object_subtypes,
                    ability_uses: &mut ability_uses,
                    goads: &mut goads,
                    completed_dungeons: &mut completed_dungeons,
                    effect_subtype_changes: &mut effect_subtype_changes,
                    context_path_elements: &mut context_path_elements,
                    actions: &mut actions,
                    action_refs: &mut action_refs,
                },
            )
            .unwrap();
        assert_eq!(third, first);
        assert_eq!(objects.as_ptr(), first_object_allocation);
        assert_eq!(actions.as_ptr(), first_action_allocation);
    }

    #[test]
    fn owned_scoring_encode_matches_copy_path_after_poisoned_reuse() {
        let session = v2_session(90_031, 131);
        let expected = expected(&session);
        let mut copy_encoder = FlatDecisionEncoderV2::default();
        let mut copy_objects = vec![FlatObjectCoreV2::default(); 512];
        let mut copy_relations = vec![FlatRelationV2::default(); 2_048];
        let mut copy_object_subtypes = vec![FlatObjectSubtypeV2::default(); 2_048];
        let mut copy_ability_uses = vec![FlatObjectAbilityUseV2::default(); 512];
        let mut copy_goads = vec![FlatObjectGoadV2::default(); 512];
        let mut copy_completed_dungeons = vec![FlatCompletedDungeonV2::default(); 128];
        let mut copy_effect_subtype_changes = vec![FlatEffectSubtypeChangeV2::default(); 512];
        let mut copy_context_path_elements = vec![FlatContextPathElementV2::default(); 512];
        let mut copy_actions = vec![FlatActionCoreV1::default(); 128];
        let mut copy_action_refs = vec![FlatActionRefV2::default(); 1_024];
        let mut copy_action_objects = vec![FlatActionObjectV2::default(); 1_024];
        let copy_decision = session
            .encode_current_flat_decision_v2(
                expected,
                &mut copy_encoder,
                &mut FlatDecisionBuffersV2 {
                    objects: &mut copy_objects,
                    relations: &mut copy_relations,
                    object_subtypes: &mut copy_object_subtypes,
                    ability_uses: &mut copy_ability_uses,
                    goads: &mut copy_goads,
                    completed_dungeons: &mut copy_completed_dungeons,
                    effect_subtype_changes: &mut copy_effect_subtype_changes,
                    context_path_elements: &mut copy_context_path_elements,
                    actions: &mut copy_actions,
                    action_refs: &mut copy_action_refs,
                    action_objects: &mut copy_action_objects,
                },
            )
            .unwrap();

        let expected_objects = copy_objects[..copy_decision.active_object_count as usize].to_vec();
        let expected_relations =
            copy_relations[..copy_decision.active_relation_count as usize].to_vec();
        let expected_object_subtypes =
            copy_object_subtypes[..copy_decision.active_object_subtype_count as usize].to_vec();
        let expected_ability_uses =
            copy_ability_uses[..copy_decision.active_ability_use_count as usize].to_vec();
        let expected_goads = copy_goads[..copy_decision.active_goad_count as usize].to_vec();
        let expected_completed_dungeons = copy_completed_dungeons
            [..copy_decision.active_completed_dungeon_count as usize]
            .to_vec();
        let expected_effect_subtype_changes = copy_effect_subtype_changes
            [..copy_decision.active_effect_subtype_change_count as usize]
            .to_vec();
        let expected_context_path_elements = copy_context_path_elements
            [..copy_decision.active_context_path_element_count as usize]
            .to_vec();
        let expected_actions = copy_actions[..copy_decision.active_action_count as usize]
            .iter()
            .copied()
            .map(FlatScorerActionCoreV2::from)
            .collect::<Vec<_>>();
        let expected_action_refs = copy_encoder
            .cached_scorer_action_refs_v2(copy_decision.binding.action_binding)
            .unwrap()
            .to_vec();

        let mut owned_encoder = FlatDecisionEncoderV2::default();
        let mut objects = vec![FlatObjectCoreV2::default()];
        let mut relations = vec![FlatRelationV2::default()];
        let mut object_subtypes = vec![FlatObjectSubtypeV2::default()];
        let mut ability_uses = vec![FlatObjectAbilityUseV2::default()];
        let mut goads = vec![FlatObjectGoadV2::default()];
        let mut completed_dungeons = vec![FlatCompletedDungeonV2::default()];
        let mut effect_subtype_changes = vec![FlatEffectSubtypeChangeV2::default()];
        let mut context_path_elements = vec![FlatContextPathElementV2::default()];
        let mut actions = vec![FlatScorerActionCoreV2::default()];
        let mut action_refs = vec![FlatScorerActionRefV2::default()];

        macro_rules! encode_owned {
            () => {
                session.encode_current_flat_scoring_decision_owned_v2(
                    expected,
                    &mut owned_encoder,
                    &mut FlatScoringOwnedBuffersV2 {
                        objects: &mut objects,
                        relations: &mut relations,
                        object_subtypes: &mut object_subtypes,
                        ability_uses: &mut ability_uses,
                        goads: &mut goads,
                        completed_dungeons: &mut completed_dungeons,
                        effect_subtype_changes: &mut effect_subtype_changes,
                        context_path_elements: &mut context_path_elements,
                        actions: &mut actions,
                        action_refs: &mut action_refs,
                    },
                )
            };
        }

        let owned_decision = encode_owned!().unwrap();
        assert_eq!(owned_decision, copy_decision);
        assert_eq!(
            (
                &objects,
                &relations,
                &object_subtypes,
                &ability_uses,
                &goads,
                &completed_dungeons,
                &effect_subtype_changes,
                &context_path_elements,
                &actions,
                &action_refs,
            ),
            (
                &expected_objects,
                &expected_relations,
                &expected_object_subtypes,
                &expected_ability_uses,
                &expected_goads,
                &expected_completed_dungeons,
                &expected_effect_subtype_changes,
                &expected_context_path_elements,
                &expected_actions,
                &expected_action_refs,
            )
        );

        objects.push(FlatObjectCoreV2::default());
        relations.push(FlatRelationV2::default());
        object_subtypes.push(FlatObjectSubtypeV2::default());
        ability_uses.push(FlatObjectAbilityUseV2::default());
        goads.push(FlatObjectGoadV2::default());
        completed_dungeons.push(FlatCompletedDungeonV2::default());
        effect_subtype_changes.push(FlatEffectSubtypeChangeV2::default());
        context_path_elements.push(FlatContextPathElementV2::default());
        actions.push(FlatScorerActionCoreV2::default());
        action_refs.push(FlatScorerActionRefV2::default());

        let reused_decision = encode_owned!().unwrap();
        assert_eq!(reused_decision, copy_decision);
        assert_eq!(
            (
                &objects,
                &relations,
                &object_subtypes,
                &ability_uses,
                &goads,
                &completed_dungeons,
                &effect_subtype_changes,
                &context_path_elements,
                &actions,
                &action_refs,
            ),
            (
                &expected_objects,
                &expected_relations,
                &expected_object_subtypes,
                &expected_ability_uses,
                &expected_goads,
                &expected_completed_dungeons,
                &expected_effect_subtype_changes,
                &expected_context_path_elements,
                &expected_actions,
                &expected_action_refs,
            )
        );
    }

    #[test]
    fn owned_scoring_encode_is_destination_atomic_on_late_binding_error() {
        let session = v2_session(90_032, 132);
        let expected = expected(&session);
        let mut encoder = FlatDecisionEncoderV2::default();
        encoder.ensure_cache(&session, expected).unwrap();
        assert!(encoder.scorer_actions.pop().is_some());

        let mut objects = vec![FlatObjectCoreV2::default()];
        let mut relations = vec![FlatRelationV2::default()];
        let mut object_subtypes = vec![FlatObjectSubtypeV2::default()];
        let mut ability_uses = vec![FlatObjectAbilityUseV2::default()];
        let mut goads = vec![FlatObjectGoadV2::default()];
        let mut completed_dungeons = vec![FlatCompletedDungeonV2::default()];
        let mut effect_subtype_changes = vec![FlatEffectSubtypeChangeV2::default()];
        let mut context_path_elements = vec![FlatContextPathElementV2::default()];
        let mut actions = vec![FlatScorerActionCoreV2::default()];
        let mut action_refs = vec![FlatScorerActionRefV2::default()];
        let before = (
            objects.clone(),
            relations.clone(),
            object_subtypes.clone(),
            ability_uses.clone(),
            goads.clone(),
            completed_dungeons.clone(),
            effect_subtype_changes.clone(),
            context_path_elements.clone(),
            actions.clone(),
            action_refs.clone(),
        );

        assert_eq!(
            session
                .encode_current_flat_scoring_decision_owned_v2(
                    expected,
                    &mut encoder,
                    &mut FlatScoringOwnedBuffersV2 {
                        objects: &mut objects,
                        relations: &mut relations,
                        object_subtypes: &mut object_subtypes,
                        ability_uses: &mut ability_uses,
                        goads: &mut goads,
                        completed_dungeons: &mut completed_dungeons,
                        effect_subtype_changes: &mut effect_subtype_changes,
                        context_path_elements: &mut context_path_elements,
                        actions: &mut actions,
                        action_refs: &mut action_refs,
                    },
                )
                .unwrap_err(),
            FlatDecisionErrorV2::ScorerBindingMismatch
        );
        assert_eq!(
            (
                objects,
                relations,
                object_subtypes,
                ability_uses,
                goads,
                completed_dungeons,
                effect_subtype_changes,
                context_path_elements,
                actions,
                action_refs,
            ),
            before
        );
    }

    #[test]
    fn every_table_reports_its_exact_capacity_before_any_publication() {
        let session = v2_session(90_020, 120);
        let expected = expected(&session);
        let mut encoder = one_row_encoder(&session, expected);

        macro_rules! assert_short {
            ($field:ident, $error:expr) => {{
                let poison_object = FlatObjectCoreV2 {
                    card_token: 65_536,
                    ..FlatObjectCoreV2::default()
                };
                let poison_relation = FlatRelationV2 {
                    primary_order: u32::MAX,
                    ..FlatRelationV2::default()
                };
                let mut objects = [poison_object];
                let mut relations = [poison_relation];
                let mut object_subtypes = [FlatObjectSubtypeV2 {
                    subtype_id: u16::MAX,
                    ..FlatObjectSubtypeV2::default()
                }];
                let mut ability_uses = [FlatObjectAbilityUseV2 {
                    uses: u16::MAX,
                    ..FlatObjectAbilityUseV2::default()
                }];
                let mut goads = [FlatObjectGoadV2 {
                    expires_after_turns: u32::MAX,
                    ..FlatObjectGoadV2::default()
                }];
                let mut completed_dungeons = [FlatCompletedDungeonV2 {
                    dungeon_id: u16::MAX,
                    ..FlatCompletedDungeonV2::default()
                }];
                let mut effect_subtype_changes = [FlatEffectSubtypeChangeV2 {
                    subtype_id: u16::MAX,
                    ..FlatEffectSubtypeChangeV2::default()
                }];
                let mut context_path_elements = [FlatContextPathElementV2 {
                    value: u16::MAX,
                    ..FlatContextPathElementV2::default()
                }];
                let mut actions = [FlatActionCoreV1 {
                    flags: u16::MAX,
                    ..FlatActionCoreV1::default()
                }];
                let mut action_refs = [FlatActionRefV2 {
                    action_index: u32::MAX,
                    ..FlatActionRefV2::default()
                }];
                let mut action_objects = [FlatActionObjectV2 {
                    card_token: u32::MAX,
                    ..FlatActionObjectV2::default()
                }];
                let before = (
                    objects,
                    relations,
                    object_subtypes,
                    ability_uses,
                    goads,
                    completed_dungeons,
                    effect_subtype_changes,
                    context_path_elements,
                    actions,
                    action_refs,
                    action_objects,
                );
                let mut buffers = FlatDecisionBuffersV2 {
                    objects: &mut objects,
                    relations: &mut relations,
                    object_subtypes: &mut object_subtypes,
                    ability_uses: &mut ability_uses,
                    goads: &mut goads,
                    completed_dungeons: &mut completed_dungeons,
                    effect_subtype_changes: &mut effect_subtype_changes,
                    context_path_elements: &mut context_path_elements,
                    actions: &mut actions,
                    action_refs: &mut action_refs,
                    action_objects: &mut action_objects,
                };
                buffers.$field = &mut [];
                assert_eq!(
                    session
                        .encode_current_flat_decision_v2(expected, &mut encoder, &mut buffers,)
                        .unwrap_err(),
                    $error
                );
                assert_eq!(
                    (
                        objects,
                        relations,
                        object_subtypes,
                        ability_uses,
                        goads,
                        completed_dungeons,
                        effect_subtype_changes,
                        context_path_elements,
                        actions,
                        action_refs,
                        action_objects,
                    ),
                    before
                );
            }};
        }

        assert_short!(
            objects,
            FlatDecisionErrorV2::InsufficientObjectCapacity {
                required: 1,
                available: 0
            }
        );
        assert_short!(
            relations,
            FlatDecisionErrorV2::InsufficientRelationCapacity {
                required: 1,
                available: 0
            }
        );
        assert_short!(
            object_subtypes,
            FlatDecisionErrorV2::InsufficientObjectSubtypeCapacity {
                required: 1,
                available: 0
            }
        );
        assert_short!(
            ability_uses,
            FlatDecisionErrorV2::InsufficientAbilityUseCapacity {
                required: 1,
                available: 0
            }
        );
        assert_short!(
            goads,
            FlatDecisionErrorV2::InsufficientGoadCapacity {
                required: 1,
                available: 0
            }
        );
        assert_short!(
            completed_dungeons,
            FlatDecisionErrorV2::InsufficientCompletedDungeonCapacity {
                required: 1,
                available: 0
            }
        );
        assert_short!(
            effect_subtype_changes,
            FlatDecisionErrorV2::InsufficientEffectSubtypeCapacity {
                required: 1,
                available: 0
            }
        );
        assert_short!(
            context_path_elements,
            FlatDecisionErrorV2::InsufficientContextPathCapacity {
                required: 1,
                available: 0
            }
        );
        assert_short!(
            actions,
            FlatDecisionErrorV2::InsufficientActionCapacity {
                required: 1,
                available: 0
            }
        );
        assert_short!(
            action_refs,
            FlatDecisionErrorV2::InsufficientActionRefCapacity {
                required: 1,
                available: 0
            }
        );
        assert_short!(
            action_objects,
            FlatDecisionErrorV2::InsufficientActionObjectCapacity {
                required: 1,
                available: 0
            }
        );
    }
}

#[cfg(test)]
mod v2_tests {
    use super::*;
    use crate::flat_policy_v1::{
        flat_action_ref_projection_role_id_v1, FLAT_ACTION_REF_INTERNAL_TO_PROJECTION_V1,
        FLAT_ACTION_REF_PROJECTION_ROLE_MAPPING_VERSION_V1,
    };
    use crate::rl_session::{
        FastActorResponseV1, FlatActionDecisionSliceErrorV1, CANONICAL_BURN_DECK_ID,
        FLAT_ACTION_CANDIDATE_COMMITMENT_VERSION_V2, FLAT_ACTION_CARD_TOKEN_MAPPING_VERSION_V2,
        FLAT_ACTION_DECISION_SLICE_VERSION_V2,
    };
    use sha2::{Digest, Sha512};

    fn expected(session: &FastActorSessionV1) -> FastActorDecisionV1 {
        let FastActorResponseV1::Decision(expected) = session.current_response() else {
            panic!("expected live V2 decision");
        };
        expected
    }

    fn v2_session(episode_id: u64) -> FastActorSessionV1 {
        FastActorSessionV1::reset_with_decks_and_limits_flat_action_v2(
            episode_id,
            0x5eed,
            128,
            16_384,
            [
                CANONICAL_BURN_DECK_ID.to_string(),
                CANONICAL_BURN_DECK_ID.to_string(),
            ],
        )
        .unwrap()
    }

    fn combat_encoder(
        observation: &ObservationV5,
    ) -> Result<FlatDecisionEncoderV2, FlatDecisionErrorV2> {
        let mut encoder = FlatDecisionEncoderV2::default();
        encoder.build_globals(observation)?;
        encoder.register_objects(observation)?;
        encoder.build_relations(observation)?;
        encoder.validate_cached_tables()?;
        Ok(encoder)
    }

    fn attacker_orders(encoder: &FlatDecisionEncoderV2) -> Vec<Option<u32>> {
        let view = FlatScoringDecisionViewV2::new(
            &encoder.globals,
            &encoder.objects,
            &encoder.relations,
            &encoder.object_subtypes,
            &encoder.ability_uses,
            &encoder.goads,
            &encoder.completed_dungeons,
            &encoder.effect_subtype_changes,
            &encoder.context_path_elements,
            &encoder.scorer_actions,
            &encoder.scorer_action_refs,
        );
        view.relations()
            .iter()
            .filter_map(|relation| match (relation.role, relation.payload) {
                (
                    FlatRelationRoleV2::CombatAttacker,
                    FlatRelationPayloadV2::CombatAttacker { blocked_order },
                ) => Some(blocked_order),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn action_ref_role_crosswalk_exactly_reuses_the_v1_authority() {
        const EXPECTED: [u8; 8] = [0, 1, 2, 3, 4, 5, 6, 9];
        const ROLES: [FlatActionRefRoleV1; 8] = [
            FlatActionRefRoleV1::Source,
            FlatActionRefRoleV1::Candidate,
            FlatActionRefRoleV1::Card,
            FlatActionRefRoleV1::Attacker,
            FlatActionRefRoleV1::Blocker,
            FlatActionRefRoleV1::TargetObject,
            FlatActionRefRoleV1::Cards,
            FlatActionRefRoleV1::PendingSources,
        ];

        assert_eq!(FLAT_ACTION_REF_INTERNAL_TO_PROJECTION_V2, EXPECTED);
        assert_eq!(FLAT_ACTION_REF_INTERNAL_TO_PROJECTION_V1, EXPECTED);
        assert_eq!(
            FLAT_ACTION_REF_PROJECTION_ROLE_MAPPING_VERSION_V2,
            FLAT_ACTION_REF_PROJECTION_ROLE_MAPPING_VERSION_V1
        );
        for (index, role) in ROLES.into_iter().enumerate() {
            assert_eq!(flat_action_ref_projection_role_id_v2(role), EXPECTED[index]);
            assert_eq!(
                flat_action_ref_projection_role_id_v2(role),
                flat_action_ref_projection_role_id_v1(role)
            );
        }
    }

    #[test]
    fn blocked_order_distinguishes_absent_present_empty_and_mapping_permutations() {
        let golden: serde_json::Value =
            serde_json::from_str(include_str!("../../data/flat_policy_v2/goldens_v2.json"))
                .unwrap();
        let expected_orders = |name: &str| {
            golden["blocked_order_by_ordered_attacker"][name]
                .as_array()
                .unwrap()
                .iter()
                .map(|value| value.as_u64().map(|order| u32::try_from(order).unwrap()))
                .collect::<Vec<_>>()
        };
        let session = v2_session(92_001);
        let mut observation = session
            .flat_policy_observation_v2(expected(&session))
            .unwrap();
        let first = observation.own_hand[0].stable.clone();
        let second = observation.own_hand[1].stable.clone();
        observation.projection.surface.combat.ordered_attackers =
            vec![first.clone(), second.clone()];

        observation
            .projection
            .surface
            .combat
            .attacker_to_ordered_blockers = Vec::new();
        assert_eq!(
            attacker_orders(&combat_encoder(&observation).unwrap()),
            vec![None, None]
        );

        observation
            .projection
            .surface
            .combat
            .attacker_to_ordered_blockers = vec![(first.clone(), Vec::new())];
        assert_eq!(
            attacker_orders(&combat_encoder(&observation).unwrap()),
            expected_orders("absent")
        );

        observation
            .projection
            .surface
            .combat
            .attacker_to_ordered_blockers =
            vec![(first.clone(), Vec::new()), (second.clone(), Vec::new())];
        let forward = attacker_orders(&combat_encoder(&observation).unwrap());
        assert_eq!(forward, expected_orders("present_empty_forward"));

        observation
            .projection
            .surface
            .combat
            .attacker_to_ordered_blockers =
            vec![(second.clone(), Vec::new()), (first.clone(), Vec::new())];
        let reverse = attacker_orders(&combat_encoder(&observation).unwrap());
        assert_eq!(reverse, expected_orders("present_empty_reverse"));
        assert_ne!(forward, reverse);

        observation
            .projection
            .surface
            .combat
            .attacker_to_ordered_blockers =
            vec![(first.clone(), Vec::new()), (first.clone(), Vec::new())];
        assert!(matches!(
            combat_encoder(&observation),
            Err(FlatDecisionErrorV2::InconsistentReference)
        ));
    }

    #[test]
    fn blocked_order_validation_rejects_duplicate_and_gapped_orders() {
        let session = v2_session(92_002);
        let mut observation = session
            .flat_policy_observation_v2(expected(&session))
            .unwrap();
        let first = observation.own_hand[0].stable.clone();
        let second = observation.own_hand[1].stable.clone();
        observation.projection.surface.combat.ordered_attackers =
            vec![first.clone(), second.clone()];
        observation
            .projection
            .surface
            .combat
            .attacker_to_ordered_blockers = vec![(first, Vec::new()), (second, Vec::new())];

        let mut duplicate = combat_encoder(&observation).unwrap();
        for relation in &mut duplicate.relations {
            if let FlatRelationPayloadV2::CombatAttacker { blocked_order } = &mut relation.payload {
                *blocked_order = Some(0);
            }
        }
        assert_eq!(
            duplicate.validate_cached_tables(),
            Err(FlatDecisionErrorV2::InconsistentReference)
        );

        let mut gap = combat_encoder(&observation).unwrap();
        let mut seen = 0;
        for relation in &mut gap.relations {
            if let FlatRelationPayloadV2::CombatAttacker { blocked_order } = &mut relation.payload {
                *blocked_order = Some(if seen == 0 { 0 } else { 2 });
                seen += 1;
            }
        }
        assert_eq!(
            gap.validate_cached_tables(),
            Err(FlatDecisionErrorV2::InconsistentReference)
        );
    }

    #[test]
    fn known_hand_set_order_matches_python_canonical_json_order() {
        let session = v2_session(92_005);
        let mut observation = session
            .flat_policy_observation_v2(expected(&session))
            .unwrap();
        let actor = observation.acting_player;
        let owner = opponent(actor);
        let mut lexical_first = observation.own_hand[0].clone();
        lexical_first.stable.arena_id = 900_010;
        lexical_first.stable.card_db_id = 10;
        lexical_first.stable.owner = owner;
        lexical_first.stable.controller = owner;
        lexical_first.stable.zone = Zone::Hand;
        let mut lexical_second = observation.own_hand[1].clone();
        lexical_second.stable.arena_id = 900_002;
        lexical_second.stable.card_db_id = 2;
        lexical_second.stable.owner = owner;
        lexical_second.stable.controller = owner;
        lexical_second.stable.zone = Zone::Hand;
        observation.known_hand_cards[seat_index(owner)] =
            vec![lexical_second.clone(), lexical_first.clone()];

        let forward = combat_encoder(&observation).unwrap();
        let known_tokens: Vec<_> = forward
            .objects
            .iter()
            .filter(|object| object.group == FlatObjectGroupV2::KnownOpponentHand)
            .map(|object| object.card_token)
            .collect();
        // Python's set-like normalization sorts the compact JSON strings, so
        // decimal "10" precedes decimal "2" rather than sorting numerically.
        assert_eq!(known_tokens, vec![11, 3]);

        observation.known_hand_cards[seat_index(owner)] = vec![lexical_first, lexical_second];
        let reverse = combat_encoder(&observation).unwrap();
        assert_eq!(forward.objects, reverse.objects);
        assert_eq!(forward.relations, reverse.relations);
    }

    fn golden_string<'a>(value: &'a serde_json::Value, field: &str) -> &'a str {
        value
            .get(field)
            .and_then(serde_json::Value::as_str)
            .unwrap_or_else(|| panic!("V2 golden field {field:?} must be a string"))
    }

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|byte| format!("{byte:02x}")).collect()
    }

    #[test]
    fn rust_recomputes_python_authority_red_pair_sha512_words_and_f32_bits() {
        let golden: serde_json::Value =
            serde_json::from_str(include_str!("../../data/flat_policy_v2/goldens_v2.json"))
                .unwrap();
        let full_cases = golden.get("full_observation_cases").unwrap();
        let model_cases = golden.get("model_canonical_cases").unwrap();
        let absent_full = golden_string(full_cases, "absent");
        let left_full = golden_string(full_cases, "present_empty_forward");
        let right_full = golden_string(full_cases, "present_empty_reverse");
        let absent = golden_string(model_cases, "absent");
        let left = golden_string(model_cases, "present_empty_forward");
        let right = golden_string(model_cases, "present_empty_reverse");
        assert_ne!(absent.as_bytes(), left.as_bytes());
        assert_ne!(left.as_bytes(), right.as_bytes());

        for payload in [absent_full, left_full, right_full, absent, left, right] {
            let parsed: serde_json::Value = serde_json::from_str(payload).unwrap();
            assert_eq!(serde_json::to_string(&parsed).unwrap(), payload);
        }
        let absent_mapping = serde_json::from_str::<serde_json::Value>(absent_full).unwrap();
        let mut left_observation = serde_json::from_str::<serde_json::Value>(left_full).unwrap();
        let mut right_observation = serde_json::from_str::<serde_json::Value>(right_full).unwrap();
        assert_eq!(left_observation["schema_version"].as_u64(), Some(5));
        assert!(left_observation["projection"].is_object());
        assert!(left_observation["own_hand"].is_array());
        assert!(left_observation["known_library_cards"].is_array());
        assert!(left_observation["known_hand_cards"].is_array());
        assert_eq!(
            absent_mapping["projection"]["combat"]["attacker_to_ordered_blockers"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            left_observation["projection"]["combat"]["attacker_to_ordered_blockers"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
        left_observation["projection"]["combat"]["attacker_to_ordered_blockers"] =
            serde_json::Value::String("<red-pair-field>".to_string());
        right_observation["projection"]["combat"]["attacker_to_ordered_blockers"] =
            serde_json::Value::String("<red-pair-field>".to_string());
        assert_eq!(left_observation, right_observation);

        let red_pair = golden.get("red_pair").unwrap();
        for (side, payload) in [("left", left), ("right", right)] {
            let expected = red_pair.get(side).unwrap();
            let expected_blocks = expected["sha512_blocks_hex"].as_array().unwrap();
            let expected_words = expected["u32_words"].as_array().unwrap();
            let expected_f32_bits = expected["f32_bits_hex"].as_array().unwrap();
            assert_eq!(expected_blocks.len(), 6);
            assert_eq!(expected_words.len(), 96);
            assert_eq!(expected_f32_bits.len(), 96);

            let mut word_index = 0;
            for counter in 0_u32..6 {
                let mut hasher = Sha512::new();
                hasher.update(b"observation-state");
                hasher.update(counter.to_le_bytes());
                hasher.update(payload.as_bytes());
                let block = hasher.finalize();
                assert_eq!(
                    hex(&block),
                    expected_blocks[usize::try_from(counter).unwrap()]
                        .as_str()
                        .unwrap()
                );
                for chunk in block.chunks_exact(4) {
                    let word = u32::from_le_bytes(chunk.try_into().unwrap());
                    assert_eq!(expected_words[word_index].as_u64(), Some(u64::from(word)));
                    let feature = ((f64::from(word) / f64::from(u32::MAX)) * 2.0 - 1.0) as f32;
                    assert_eq!(
                        format!("{:08x}", feature.to_bits()),
                        expected_f32_bits[word_index].as_str().unwrap()
                    );
                    word_index += 1;
                }
            }
            assert_eq!(word_index, 96);
        }

        let boundary = golden.get("card_token_boundary").unwrap();
        assert_eq!(boundary["u16_minus_one_card_db_id"].as_u64(), Some(65_534));
        assert_eq!(boundary["u16_minus_one_v2_token"].as_u64(), Some(65_535));
        assert_eq!(boundary["u16_max_card_db_id"].as_u64(), Some(65_535));
        assert_eq!(boundary["u16_max_v2_token"].as_u64(), Some(65_536));
        assert_eq!(card_token(u16::MAX), 65_536);
    }

    fn one_action_encoder(card_token: u32) -> FlatDecisionEncoderV2 {
        FlatDecisionEncoderV2 {
            objects: vec![FlatObjectCoreV2 {
                card_token,
                group: FlatObjectGroupV2::SelfHand,
                source_kind: FlatObjectSourceKindV2::Card,
                visible_ordinal: 0,
                ..FlatObjectCoreV2::default()
            }],
            object_keys: vec![Some(PrivateObjectKeyV2 {
                arena_id: 1,
                zone_change_count: 0,
                card_token,
                owner: FlatRelativePlayerV2::SelfPlayer,
                controller: FlatRelativePlayerV2::SelfPlayer,
                zone: FlatZoneV2::Hand,
                historical_kind: 0,
            })],
            actions: vec![FlatActionCoreV1 {
                ref_len: 1,
                ..FlatActionCoreV1::default()
            }],
            action_refs: vec![FlatActionRefV2 {
                action_index: 0,
                role: FlatActionRefRoleV1::Source,
                order_index: 0,
                associated_order: 0,
                card_token,
                object_index: 0,
            }],
            action_objects: vec![FlatActionObjectV2 {
                card_token,
                group: FlatActionObjectGroupV1::SelfHand,
                actor_visible_ordinal: 0,
                owner_relative: FlatRelativePlayerV2::SelfPlayer as u8,
                controller_relative: FlatRelativePlayerV2::SelfPlayer as u8,
                zone: FlatZoneV2::Hand as u8,
                zone_change_count: 0,
            }],
            ..FlatDecisionEncoderV2::default()
        }
    }

    #[test]
    fn scorer_action_ref_preserves_token_65536_and_rejects_padding_or_overflow() {
        let mut max = one_action_encoder(65_536);
        max.validate_cached_tables().unwrap();
        assert_eq!(max.scorer_action_refs[0].card_token, 65_536);

        for invalid in [0, 65_537] {
            let mut encoder = one_action_encoder(invalid);
            assert_eq!(
                encoder.validate_cached_tables(),
                Err(FlatDecisionErrorV2::InvalidReference)
            );
            assert!(encoder.scorer_action_refs.is_empty());
        }
    }

    #[test]
    fn explicit_v2_decision_binding_and_owned_tables_are_complete() {
        let session = v2_session(92_003);
        let expected_decision = expected(&session);
        let mut encoder = FlatDecisionEncoderV2::default();
        let mut objects = Vec::new();
        let mut relations = Vec::new();
        let mut object_subtypes = Vec::new();
        let mut ability_uses = Vec::new();
        let mut goads = Vec::new();
        let mut completed_dungeons = Vec::new();
        let mut effect_subtype_changes = Vec::new();
        let mut context_path_elements = Vec::new();
        let mut actions = Vec::new();
        let mut action_refs = Vec::new();
        let decision = session
            .encode_current_flat_scoring_decision_owned_v2(
                expected_decision,
                &mut encoder,
                &mut FlatScoringOwnedBuffersV2 {
                    objects: &mut objects,
                    relations: &mut relations,
                    object_subtypes: &mut object_subtypes,
                    ability_uses: &mut ability_uses,
                    goads: &mut goads,
                    completed_dungeons: &mut completed_dungeons,
                    effect_subtype_changes: &mut effect_subtype_changes,
                    context_path_elements: &mut context_path_elements,
                    actions: &mut actions,
                    action_refs: &mut action_refs,
                },
            )
            .unwrap();
        assert_eq!(
            decision.binding.action_binding.slice_version,
            FLAT_ACTION_DECISION_SLICE_VERSION_V2
        );
        assert_eq!(
            decision.binding.action_binding.card_token_mapping_version,
            FLAT_ACTION_CARD_TOKEN_MAPPING_VERSION_V2
        );
        assert_eq!(
            decision.binding.action_binding.candidate_commitment_version,
            FLAT_ACTION_CANDIDATE_COMMITMENT_VERSION_V2
        );
        assert_eq!(decision.binding.typed_layout_version, 2);
        assert_eq!(decision.binding.feature_inventory_version, 2);
        assert_eq!(decision.active_object_count as usize, objects.len());
        assert_eq!(decision.active_relation_count as usize, relations.len());
        assert_eq!(decision.active_action_count as usize, actions.len());
        assert_eq!(decision.active_action_ref_count as usize, action_refs.len());
        assert!(action_refs
            .iter()
            .all(|reference| (1..=65_536).contains(&reference.card_token)));

        let view = FlatScoringDecisionViewV2::new(
            &decision.globals,
            &objects,
            &relations,
            &object_subtypes,
            &ability_uses,
            &goads,
            &completed_dungeons,
            &effect_subtype_changes,
            &context_path_elements,
            &actions,
            &action_refs,
        );
        assert_eq!(
            view.actions().len(),
            expected_decision.legal_action_count as usize
        );
        assert_eq!(view.action_refs(), action_refs);

        let v1_session = FastActorSessionV1::reset_with_limits(92_004, 0x5eed, 128, 16_384);
        let mut v2_encoder = FlatDecisionEncoderV2::default();
        let mut empty_actions = [];
        let mut empty_refs = [];
        let mut empty_objects = [];
        assert_eq!(
            v1_session.encode_current_flat_decision_v2(
                expected(&v1_session),
                &mut v2_encoder,
                &mut FlatDecisionBuffersV2 {
                    objects: &mut [],
                    relations: &mut [],
                    object_subtypes: &mut [],
                    ability_uses: &mut [],
                    goads: &mut [],
                    completed_dungeons: &mut [],
                    effect_subtype_changes: &mut [],
                    context_path_elements: &mut [],
                    actions: &mut empty_actions,
                    action_refs: &mut empty_refs,
                    action_objects: &mut empty_objects,
                },
            ),
            Err(FlatDecisionErrorV2::Action(
                FlatActionDecisionSliceErrorV1::CorruptCurrentBinding
            ))
        );
    }
}
