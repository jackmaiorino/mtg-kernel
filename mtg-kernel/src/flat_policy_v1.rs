//! Complete actor-relative typed policy input for the standalone fast actor.
//!
//! Unlike [`crate::rl_session::FlatActionDecisionSliceV1`], this module owns
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
use crate::policy_surface_v5::PolicySurfaceStageV5;
use crate::rl::{
    BooleanChoicePurposeV4, CardPrivateV1, CardPublicV2, CardStableRefV1, ContinuousEffectPublicV2,
    DiscardResumeSemanticV2, EffectDurationV2, EngineDecisionStageV2, ExilePlayPermissionPublicV2,
    ObjectRelationPublicV4, ObservationV5, PendingEffectChoiceSemanticV4, PendingTriggerKindV2,
    PlayOrCastV2, PlayPermissionExpiryV2, PlayerSeatV1, SpellCopyStageV2, StackItemKindV2,
    SurfaceDecisionStageV2, TargetRefV1, TargetSelectionPurposeV4, ZoneIndependentStepV1,
};
use crate::rl_session::{
    FastActorDecisionV1, FastActorSessionV1, FlatActionCoreV1, FlatActionDecisionBindingV1,
    FlatActionDecisionSliceBuffersV1, FlatActionDecisionSliceErrorV1, FlatActionObjectGroupV1,
    FlatActionObjectV1, FlatActionRefV1, FLAT_ACTION_MAX_TRIGGER_ORDER_REFS_V1,
};
use crate::state::{AbilityKindV4, CastMethodV4};
use crate::{mana::ManaColor, state::Zone};

pub const FLAT_POLICY_TYPED_LAYOUT_VERSION_V1: u32 = 1;
pub const FLAT_POLICY_FEATURE_INVENTORY_VERSION_V1: u32 = 1;
pub const FLAT_POLICY_ENUM_MAPPING_VERSION_V1: u32 = 1;
pub const FLAT_POLICY_OBJECT_GROUP_MAPPING_VERSION_V1: u32 = 1;
pub const FLAT_POLICY_RELATION_ROLE_MAPPING_VERSION_V1: u32 = 1;
pub const FLAT_POLICY_CONTEXT_SUBROLE_MAPPING_VERSION_V1: u32 = 1;

const HISTORICAL_STACK_TARGET_KIND_V1: u8 = 1;
const HISTORICAL_PAID_COST_KIND_V1: u8 = 2;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum FlatRelativePlayerV1 {
    #[default]
    SelfPlayer = 0,
    Opponent = 1,
    None = 2,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum FlatZoneV1 {
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
pub enum FlatPhaseV1 {
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
pub enum FlatManaColorV1 {
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
pub enum FlatTurnRelationV1 {
    #[default]
    Absent = 0,
    ThisTurn = 1,
    EarlierTurn = 2,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum FlatObjectGroupV1 {
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
pub enum FlatObjectSourceKindV1 {
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
pub enum FlatRelationRoleV1 {
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
pub enum FlatTargetKindV1 {
    #[default]
    None = 0,
    Player = 1,
    Object = 2,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum FlatContextElementKindV1 {
    #[default]
    StructuralPath = 0,
    LegalColor = 1,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum FlatEffectSubtypeChangeKindV1 {
    #[default]
    Add = 0,
    Remove = 1,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum FlatContextKindV1 {
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
pub enum FlatContextSubroleV1 {
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
pub struct FlatPlayerGlobalsV1 {
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
pub struct FlatPendingCastGlobalsV1 {
    pub source_present: bool,
    pub controller: FlatRelativePlayerV1,
    pub chosen_target_count: u32,
    pub is_flashback: bool,
    pub cast_mode: u8,
    pub discarded_present: bool,
    pub discarded_count: u32,
    pub mode_chosen: Option<u8>,
    pub origin_zone: FlatZoneV1,
    pub sacrificed_count: u32,
    pub kicked: Option<bool>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct FlatPendingActivationGlobalsV1 {
    pub source_present: bool,
    pub controller: FlatRelativePlayerV1,
    pub ability_index: u8,
    pub chosen_target_count: u32,
    pub discard_paid_present: bool,
    pub discard_paid_count: u32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct FlatPendingDiscardGlobalsV1 {
    pub player: FlatRelativePlayerV1,
    pub count: u32,
    pub resume_stage: u8,
    pub resume_source_present: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct FlatPendingOptionalCostGlobalsV1 {
    pub player: FlatRelativePlayerV1,
    pub source_present: bool,
    pub discard_cards: u8,
    pub sacrifice_lands: u8,
    pub discard_payable: bool,
    pub sacrifice_payable: bool,
    pub spell_resume_source_present: bool,
    pub spell_resume_zone: Option<FlatZoneV1>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct FlatPendingOptionalSacrificeGlobalsV1 {
    pub player: FlatRelativePlayerV1,
    pub source_present: bool,
    pub remaining: u8,
    pub chosen_count: u32,
    pub spell_resume_source_present: bool,
    pub spell_resume_zone: Option<FlatZoneV1>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct FlatPendingSpellCopyGlobalsV1 {
    pub parent_present: bool,
    pub player: FlatRelativePlayerV1,
    pub inherited_target_kind: FlatTargetKindV1,
    pub inherited_target_player: FlatRelativePlayerV1,
    pub stage: u8,
    pub copy_present: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FlatPendingEffectChoiceV1 {
    Options {
        player: FlatRelativePlayerV1,
        path_start: u32,
        path_count: u32,
        option_count: u16,
    },
    Targets {
        player: FlatRelativePlayerV1,
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
        player: FlatRelativePlayerV1,
        path_start: u32,
        path_count: u32,
        legal_color_start: u32,
        legal_color_count: u32,
    },
    Number {
        player: FlatRelativePlayerV1,
        path_start: u32,
        path_count: u32,
        minimum: i32,
        maximum: i32,
    },
    Boolean {
        player: FlatRelativePlayerV1,
        path_start: u32,
        path_count: u32,
        default: Option<bool>,
        purpose: u8,
    },
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct FlatPendingEffectGlobalsV1 {
    pub source_present: bool,
    pub controller: FlatRelativePlayerV1,
    pub choice: Option<FlatPendingEffectChoiceV1>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct FlatEngineGlobalsV1 {
    pub priority_passes: [bool; 2],
    pub stack_nonempty: bool,
    pub stack_activity_since_priority_boundary: bool,
    pub mana_activity_since_priority_boundary: bool,
    pub last_mana_ability_activator: FlatRelativePlayerV1,
    pub current_stage: u8,
    pub pending_cast: Option<FlatPendingCastGlobalsV1>,
    pub pending_activation: Option<FlatPendingActivationGlobalsV1>,
    pub pending_discard: Option<FlatPendingDiscardGlobalsV1>,
    pub pending_optional_cost: Option<FlatPendingOptionalCostGlobalsV1>,
    pub pending_optional_sacrifice: Option<FlatPendingOptionalSacrificeGlobalsV1>,
    pub pending_spell_copy: Option<FlatPendingSpellCopyGlobalsV1>,
    pub pending_effect: Option<FlatPendingEffectGlobalsV1>,
    pub pending_trigger_count: u32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct FlatSurfaceGlobalsV1 {
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
pub struct FlatPolicySurfaceGlobalsV1 {
    pub current_stage: u8,
    pub private_combat_present: bool,
    pub private_combat_attacker_present: bool,
    pub candidate_index: u32,
    pub candidate_count: u32,
    pub selected_count: u32,
    pub remaining_count: u32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct FlatGlobalsV1 {
    pub acting_player: FlatRelativePlayerV1,
    pub phase: FlatPhaseV1,
    pub active_player: FlatRelativePlayerV1,
    pub priority_player: FlatRelativePlayerV1,
    pub initiative: FlatRelativePlayerV1,
    pub players: [FlatPlayerGlobalsV1; 2],
    pub attackers_declared: bool,
    pub blockers_declared: bool,
    pub engine: FlatEngineGlobalsV1,
    pub surface: FlatSurfaceGlobalsV1,
    pub policy_surface: FlatPolicySurfaceGlobalsV1,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct FlatObjectCoreV1 {
    pub card_token: u32,
    pub group: FlatObjectGroupV1,
    pub source_kind: FlatObjectSourceKindV1,
    pub visible_ordinal: u32,
    pub owner: FlatRelativePlayerV1,
    pub controller: FlatRelativePlayerV1,
    pub zone: Option<FlatZoneV1>,
    pub card_details_present: bool,
    pub tapped: bool,
    pub summoning_sick: bool,
    pub damage: u16,
    pub counters: [i16; 5],
    pub plotted_turn: FlatTurnRelationV1,
    pub is_token: bool,
    pub face_index: u8,
    pub chosen_color: Option<FlatManaColorV1>,
    pub entered_battlefield_turn: FlatTurnRelationV1,
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
pub struct FlatObjectSubtypeV1 {
    pub object_index: u32,
    pub order: u32,
    pub subtype_id: u16,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct FlatObjectAbilityUseV1 {
    pub object_index: u32,
    pub order: u32,
    pub ability_kind: u8,
    pub ability_index: u16,
    pub uses: u16,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct FlatObjectGoadV1 {
    pub object_index: u32,
    pub order: u32,
    pub player: FlatRelativePlayerV1,
    pub expires_after_turns: u32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct FlatCompletedDungeonV1 {
    pub player: FlatRelativePlayerV1,
    pub order: u32,
    pub dungeon_id: u16,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct FlatEffectSubtypeChangeV1 {
    pub effect_order: u32,
    pub kind: FlatEffectSubtypeChangeKindV1,
    pub order: u32,
    pub subtype_id: u16,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct FlatContextPathElementV1 {
    pub context: FlatContextKindV1,
    pub context_order: u32,
    pub kind: FlatContextElementKindV1,
    pub order: u32,
    pub value: u16,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct FlatStackRelationDataV1 {
    pub controller: FlatRelativePlayerV1,
    pub stack_item_kind: u8,
    pub is_copy: bool,
    pub is_flashback: bool,
    pub mode_chosen: u8,
    pub madness_offer: bool,
    pub kicked: bool,
    pub cast_method: u8,
    pub face_index: u8,
    pub x_value: u16,
    pub target_kind: FlatTargetKindV1,
    pub target_player: FlatRelativePlayerV1,
    /// Announcement-time controller provenance for an object target. This is
    /// intentionally independent of the current controller on the resolved
    /// live object row because control can change without a zone change.
    pub target_object_controller: FlatRelativePlayerV1,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct FlatEffectRelationDataV1 {
    pub controller: FlatRelativePlayerV1,
    pub affected_player: FlatRelativePlayerV1,
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
pub struct FlatPermissionRelationDataV1 {
    pub holder: FlatRelativePlayerV1,
    pub play_or_cast: u8,
    pub expiry: u8,
    pub holder_turn_started: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct FlatContextRelationDataV1 {
    pub context: FlatContextKindV1,
    pub subrole: FlatContextSubroleV1,
    pub target_kind: FlatTargetKindV1,
    pub target_player: FlatRelativePlayerV1,
    pub controller: FlatRelativePlayerV1,
    pub trigger_kind: u8,
    pub kicked: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum FlatRelationPayloadV1 {
    #[default]
    None,
    Stack(FlatStackRelationDataV1),
    CombatAttacker {
        was_blocked: bool,
    },
    Effect(FlatEffectRelationDataV1),
    Permission(FlatPermissionRelationDataV1),
    Context(FlatContextRelationDataV1),
    Known {
        owner: FlatRelativePlayerV1,
    },
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct FlatRelationV1 {
    pub role: FlatRelationRoleV1,
    pub source_object: Option<u32>,
    pub target_object: Option<u32>,
    pub primary_order: u32,
    pub secondary_order: u32,
    pub associated_order: u32,
    pub payload: FlatRelationPayloadV1,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FlatDecisionBindingV1 {
    pub action_binding: FlatActionDecisionBindingV1,
    pub typed_layout_version: u32,
    pub feature_inventory_version: u32,
    pub enum_mapping_version: u32,
    pub object_group_mapping_version: u32,
    pub relation_role_mapping_version: u32,
    pub context_subrole_mapping_version: u32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FlatDecisionV1 {
    pub binding: FlatDecisionBindingV1,
    pub globals: FlatGlobalsV1,
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

pub struct FlatDecisionBuffersV1<'a> {
    pub objects: &'a mut [FlatObjectCoreV1],
    pub relations: &'a mut [FlatRelationV1],
    pub object_subtypes: &'a mut [FlatObjectSubtypeV1],
    pub ability_uses: &'a mut [FlatObjectAbilityUseV1],
    pub goads: &'a mut [FlatObjectGoadV1],
    pub completed_dungeons: &'a mut [FlatCompletedDungeonV1],
    pub effect_subtype_changes: &'a mut [FlatEffectSubtypeChangeV1],
    pub context_path_elements: &'a mut [FlatContextPathElementV1],
    pub actions: &'a mut [FlatActionCoreV1],
    pub action_refs: &'a mut [FlatActionRefV1],
    /// Operational/binding-only PR27 table.  `FlatActionRefV1::object_index`
    /// indexes this table and never the model-visible `objects` table.
    pub action_objects: &'a mut [FlatActionObjectV1],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlatDecisionErrorV1 {
    Action(FlatActionDecisionSliceErrorV1),
    ObservationContract,
    InvalidReference,
    InconsistentReference,
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

impl From<FlatActionDecisionSliceErrorV1> for FlatDecisionErrorV1 {
    fn from(value: FlatActionDecisionSliceErrorV1) -> Self {
        Self::Action(value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PrivateObjectKeyV1 {
    arena_id: u32,
    zone_change_count: u32,
    card_token: u32,
    owner: FlatRelativePlayerV1,
    controller: FlatRelativePlayerV1,
    zone: FlatZoneV1,
    historical_kind: u8,
}

#[derive(Default)]
pub struct FlatDecisionEncoderV1 {
    cached_binding: Option<FlatActionDecisionBindingV1>,
    globals: FlatGlobalsV1,
    objects: Vec<FlatObjectCoreV1>,
    object_keys: Vec<Option<PrivateObjectKeyV1>>,
    relations: Vec<FlatRelationV1>,
    object_subtypes: Vec<FlatObjectSubtypeV1>,
    ability_uses: Vec<FlatObjectAbilityUseV1>,
    goads: Vec<FlatObjectGoadV1>,
    completed_dungeons: Vec<FlatCompletedDungeonV1>,
    effect_subtype_changes: Vec<FlatEffectSubtypeChangeV1>,
    context_path_elements: Vec<FlatContextPathElementV1>,
    actions: Vec<FlatActionCoreV1>,
    action_refs: Vec<FlatActionRefV1>,
    action_objects: Vec<FlatActionObjectV1>,
}

fn relative_player(seat: PlayerSeatV1, actor: PlayerSeatV1) -> FlatRelativePlayerV1 {
    if seat == actor {
        FlatRelativePlayerV1::SelfPlayer
    } else {
        FlatRelativePlayerV1::Opponent
    }
}

fn optional_relative_player(
    seat: Option<PlayerSeatV1>,
    actor: PlayerSeatV1,
) -> FlatRelativePlayerV1 {
    seat.map_or(FlatRelativePlayerV1::None, |value| {
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

fn flat_zone(zone: Zone) -> FlatZoneV1 {
    match zone {
        Zone::Library => FlatZoneV1::Library,
        Zone::Hand => FlatZoneV1::Hand,
        Zone::Battlefield => FlatZoneV1::Battlefield,
        Zone::Graveyard => FlatZoneV1::Graveyard,
        Zone::Stack => FlatZoneV1::Stack,
        Zone::Exile => FlatZoneV1::Exile,
        Zone::Command => FlatZoneV1::Command,
    }
}

fn flat_phase(phase: ZoneIndependentStepV1) -> FlatPhaseV1 {
    match phase {
        ZoneIndependentStepV1::Untap => FlatPhaseV1::Untap,
        ZoneIndependentStepV1::Upkeep => FlatPhaseV1::Upkeep,
        ZoneIndependentStepV1::Draw => FlatPhaseV1::Draw,
        ZoneIndependentStepV1::Main1 => FlatPhaseV1::Main1,
        ZoneIndependentStepV1::BeginCombat => FlatPhaseV1::BeginCombat,
        ZoneIndependentStepV1::DeclareAttackers => FlatPhaseV1::DeclareAttackers,
        ZoneIndependentStepV1::DeclareBlockers => FlatPhaseV1::DeclareBlockers,
        ZoneIndependentStepV1::CombatDamage => FlatPhaseV1::CombatDamage,
        ZoneIndependentStepV1::EndCombat => FlatPhaseV1::EndCombat,
        ZoneIndependentStepV1::Main2 => FlatPhaseV1::Main2,
        ZoneIndependentStepV1::End => FlatPhaseV1::End,
        ZoneIndependentStepV1::Cleanup => FlatPhaseV1::Cleanup,
    }
}

fn flat_color(color: ManaColor) -> FlatManaColorV1 {
    match color {
        ManaColor::W => FlatManaColorV1::White,
        ManaColor::U => FlatManaColorV1::Blue,
        ManaColor::B => FlatManaColorV1::Black,
        ManaColor::R => FlatManaColorV1::Red,
        ManaColor::G => FlatManaColorV1::Green,
        ManaColor::C => FlatManaColorV1::Colorless,
    }
}

fn turn_relation(
    value: Option<u32>,
    current_turn: u32,
) -> Result<FlatTurnRelationV1, FlatDecisionErrorV1> {
    match value {
        None => Ok(FlatTurnRelationV1::Absent),
        Some(value) if value == current_turn => Ok(FlatTurnRelationV1::ThisTurn),
        Some(value) if value < current_turn => Ok(FlatTurnRelationV1::EarlierTurn),
        Some(_) => Err(FlatDecisionErrorV1::FutureTurnRelation),
    }
}

fn card_token(card_db_id: u16) -> u32 {
    u32::from(card_db_id) + 1
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
) -> (FlatTargetKindV1, FlatRelativePlayerV1) {
    match target {
        TargetRefV1::Player { player } => {
            (FlatTargetKindV1::Player, relative_player(*player, actor))
        }
        TargetRefV1::Object { .. } => (FlatTargetKindV1::Object, FlatRelativePlayerV1::None),
    }
}

fn usize_u32(value: usize) -> Result<u32, FlatDecisionErrorV1> {
    u32::try_from(value).map_err(|_| FlatDecisionErrorV1::CheckedIntegerRange)
}

fn usize_u64(value: usize) -> Result<u64, FlatDecisionErrorV1> {
    u64::try_from(value).map_err(|_| FlatDecisionErrorV1::CheckedIntegerRange)
}

fn context_object_ordinal(context: FlatContextKindV1, order: u32) -> u32 {
    0x8000_0000 | (u32::from(context as u8) << 16) | order
}

impl FlatDecisionEncoderV1 {
    fn clear_typed_cache(&mut self) {
        self.cached_binding = None;
        self.globals = FlatGlobalsV1::default();
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
    }

    fn private_key(
        stable: &CardStableRefV1,
        actor: PlayerSeatV1,
        historical_kind: u8,
    ) -> PrivateObjectKeyV1 {
        PrivateObjectKeyV1 {
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
    ) -> Result<u32, FlatDecisionErrorV1> {
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
            Err(FlatDecisionErrorV1::InconsistentReference)
        } else {
            Err(FlatDecisionErrorV1::InvalidReference)
        }
    }

    fn resolve_reference(
        &self,
        stable: &CardStableRefV1,
        actor: PlayerSeatV1,
    ) -> Result<u32, FlatDecisionErrorV1> {
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
                    return Err(FlatDecisionErrorV1::InconsistentReference);
                }
                if found.is_none() {
                    found = Some(usize_u32(index)?);
                }
            }
        }
        found.ok_or(FlatDecisionErrorV1::InvalidReference)
    }

    fn resolve_historical_stack_target(
        &self,
        stable: &CardStableRefV1,
        actor: PlayerSeatV1,
    ) -> Result<u32, FlatDecisionErrorV1> {
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
                    return Err(FlatDecisionErrorV1::InconsistentReference);
                }
                if found.is_none() {
                    found = Some(usize_u32(index)?);
                }
            }
        }
        if !matches!(wanted.zone, FlatZoneV1::Battlefield | FlatZoneV1::Stack) {
            return Err(FlatDecisionErrorV1::InvalidReference);
        }
        found.ok_or(FlatDecisionErrorV1::InvalidReference)
    }

    fn add_private_card(
        &mut self,
        card: &CardPrivateV1,
        actor: PlayerSeatV1,
        group: FlatObjectGroupV1,
        source_kind: FlatObjectSourceKindV1,
        ordinal: u32,
        historical_kind: u8,
    ) -> Result<u32, FlatDecisionErrorV1> {
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
        group: FlatObjectGroupV1,
        source_kind: FlatObjectSourceKindV1,
        ordinal: u32,
        historical_kind: u8,
    ) -> Result<u32, FlatDecisionErrorV1> {
        let wanted = Self::private_key(stable, actor, historical_kind);
        for (index, key) in self.object_keys.iter().enumerate() {
            let Some(key) = key else { continue };
            if key.arena_id == wanted.arena_id
                && key.zone_change_count == wanted.zone_change_count
                && (key.historical_kind == historical_kind
                    || key.historical_kind == 0
                    || historical_kind == 0)
            {
                if key.card_token != wanted.card_token
                    || key.owner != wanted.owner
                    || key.zone != wanted.zone
                    || (historical_kind != HISTORICAL_STACK_TARGET_KIND_V1
                        && key.controller != wanted.controller)
                {
                    return Err(FlatDecisionErrorV1::InconsistentReference);
                }
                return usize_u32(index);
            }
        }
        if historical_kind == HISTORICAL_STACK_TARGET_KIND_V1
            && !matches!(wanted.zone, FlatZoneV1::Battlefield | FlatZoneV1::Stack)
        {
            return Err(FlatDecisionErrorV1::InvalidReference);
        }
        let index = usize_u32(self.objects.len())?;
        self.objects.push(FlatObjectCoreV1 {
            card_token: wanted.card_token,
            group,
            source_kind,
            visible_ordinal: ordinal,
            owner: wanted.owner,
            controller: wanted.controller,
            zone: Some(wanted.zone),
            ..FlatObjectCoreV1::default()
        });
        self.object_keys.push(Some(wanted));
        Ok(index)
    }

    fn add_public_card(
        &mut self,
        card: &CardPublicV2,
        actor: PlayerSeatV1,
        group: FlatObjectGroupV1,
        ordinal: u32,
        current_turn: u32,
    ) -> Result<u32, FlatDecisionErrorV1> {
        let index = self.add_stable(
            &card.stable,
            actor,
            group,
            FlatObjectSourceKindV1::Card,
            ordinal,
            0,
        )?;
        let object = self
            .objects
            .get_mut(usize::try_from(index).map_err(|_| FlatDecisionErrorV1::CheckedIntegerRange)?)
            .ok_or(FlatDecisionErrorV1::InvalidReference)?;
        if object.card_details_present {
            return Ok(index);
        }
        let characteristics = &card.characteristics;
        let keywords = &characteristics.effective_keywords;
        let subtype_start = usize_u32(self.object_subtypes.len())?;
        for (order, &subtype_id) in characteristics.effective_subtype_ids.iter().enumerate() {
            self.object_subtypes.push(FlatObjectSubtypeV1 {
                object_index: index,
                order: usize_u32(order)?,
                subtype_id,
            });
        }
        let ability_use_start = usize_u32(self.ability_uses.len())?;
        for (order, ability) in card.ability_uses_this_turn.iter().enumerate() {
            self.ability_uses.push(FlatObjectAbilityUseV1 {
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
                .ok_or(FlatDecisionErrorV1::FutureTurnRelation)?;
            self.goads.push(FlatObjectGoadV1 {
                object_index: index,
                order: usize_u32(order)?,
                player: relative_player(goad.player, actor),
                expires_after_turns,
            });
        }
        *object = FlatObjectCoreV1 {
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
        group: FlatObjectGroupV1,
        source_kind: FlatObjectSourceKindV1,
        ordinal: u32,
    ) -> Result<u32, FlatDecisionErrorV1> {
        let index = usize_u32(self.objects.len())?;
        self.objects.push(FlatObjectCoreV1 {
            group,
            source_kind,
            visible_ordinal: ordinal,
            owner: FlatRelativePlayerV1::None,
            controller: FlatRelativePlayerV1::None,
            ..FlatObjectCoreV1::default()
        });
        self.object_keys.push(None);
        Ok(index)
    }

    fn append_context_elements(
        &mut self,
        context: FlatContextKindV1,
        context_order: u32,
        kind: FlatContextElementKindV1,
        values: impl IntoIterator<Item = u16>,
    ) -> Result<(u32, u32), FlatDecisionErrorV1> {
        let start = usize_u32(self.context_path_elements.len())?;
        let mut count = 0_u32;
        for value in values {
            self.context_path_elements.push(FlatContextPathElementV1 {
                context,
                context_order,
                kind,
                order: count,
                value,
            });
            count = count
                .checked_add(1)
                .ok_or(FlatDecisionErrorV1::CheckedIntegerRange)?;
        }
        Ok((start, count))
    }

    fn build_globals(&mut self, observation: &ObservationV5) -> Result<(), FlatDecisionErrorV1> {
        let actor = observation.acting_player;
        let p = &observation.projection.surface;
        let seats = [actor, opponent(actor)];
        let mut players = [FlatPlayerGlobalsV1::default(); 2];
        for (relative_index, seat) in seats.into_iter().enumerate() {
            let absolute_index = seat_index(seat);
            let status = &p.player_status[absolute_index];
            let completed_dungeon_start = usize_u32(self.completed_dungeons.len())?;
            for (order, &dungeon_id) in status.dungeon.completed_dungeons.iter().enumerate() {
                self.completed_dungeons.push(FlatCompletedDungeonV1 {
                    player: relative_player(seat, actor),
                    order: usize_u32(order)?,
                    dungeon_id,
                });
            }
            players[relative_index] = FlatPlayerGlobalsV1 {
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
                |pending| -> Result<FlatPendingCastGlobalsV1, FlatDecisionErrorV1> {
                    Ok(FlatPendingCastGlobalsV1 {
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
                |pending| -> Result<FlatPendingActivationGlobalsV1, FlatDecisionErrorV1> {
                    Ok(FlatPendingActivationGlobalsV1 {
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
                .map(|pending| FlatPendingDiscardGlobalsV1 {
                    player: relative_player(pending.player, actor),
                    count: pending.count,
                    resume_stage: discard_resume_id(pending.resume_stage),
                    resume_source_present: pending.resume_source.is_some(),
                });
        let pending_optional_cost =
            engine
                .pending_optional_cost
                .as_ref()
                .map(|pending| FlatPendingOptionalCostGlobalsV1 {
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
                |pending| -> Result<FlatPendingOptionalSacrificeGlobalsV1, FlatDecisionErrorV1> {
                    Ok(FlatPendingOptionalSacrificeGlobalsV1 {
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
            FlatPendingSpellCopyGlobalsV1 {
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
                        FlatContextKindV1::PendingEffect,
                        0,
                        FlatContextElementKindV1::StructuralPath,
                        structural_path.iter().copied(),
                    )?;
                    Some(FlatPendingEffectChoiceV1::Options {
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
                        FlatContextKindV1::PendingEffect,
                        0,
                        FlatContextElementKindV1::StructuralPath,
                        structural_path.iter().copied(),
                    )?;
                    Some(FlatPendingEffectChoiceV1::Targets {
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
                        FlatContextKindV1::PendingEffect,
                        0,
                        FlatContextElementKindV1::StructuralPath,
                        structural_path.iter().copied(),
                    )?;
                    let (legal_color_start, legal_color_count) = self.append_context_elements(
                        FlatContextKindV1::PendingEffect,
                        0,
                        FlatContextElementKindV1::LegalColor,
                        legal_colors
                            .iter()
                            .map(|&color| u16::from(flat_color(color) as u8)),
                    )?;
                    Some(FlatPendingEffectChoiceV1::Color {
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
                        FlatContextKindV1::PendingEffect,
                        0,
                        FlatContextElementKindV1::StructuralPath,
                        structural_path.iter().copied(),
                    )?;
                    Some(FlatPendingEffectChoiceV1::Number {
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
                        FlatContextKindV1::PendingEffect,
                        0,
                        FlatContextElementKindV1::StructuralPath,
                        structural_path.iter().copied(),
                    )?;
                    Some(FlatPendingEffectChoiceV1::Boolean {
                        player: relative_player(*player, actor),
                        path_start,
                        path_count,
                        default: *default,
                        purpose: boolean_purpose_id(*purpose),
                    })
                }
            };
            Some(FlatPendingEffectGlobalsV1 {
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
        self.globals = FlatGlobalsV1 {
            acting_player: FlatRelativePlayerV1::SelfPlayer,
            phase: flat_phase(p.phase),
            active_player: relative_player(p.active_player, actor),
            priority_player: relative_player(p.priority_player, actor),
            initiative: optional_relative_player(p.initiative, actor),
            players,
            attackers_declared: p.combat.attackers_declared,
            blockers_declared: p.combat.blockers_declared,
            engine: FlatEngineGlobalsV1 {
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
            surface: FlatSurfaceGlobalsV1 {
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
            policy_surface: FlatPolicySurfaceGlobalsV1 {
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
        group: FlatObjectGroupV1,
        ordinal: u32,
    ) -> Result<u32, FlatDecisionErrorV1> {
        self.objects
            .iter()
            .enumerate()
            .find(|(_, object)| object.group == group && object.visible_ordinal == ordinal)
            .map(|(index, _)| usize_u32(index))
            .transpose()?
            .ok_or(FlatDecisionErrorV1::InvalidReference)
    }

    fn resolve_arena(&self, arena_id: u32) -> Result<u32, FlatDecisionErrorV1> {
        let mut result = None;
        for (index, key) in self.object_keys.iter().enumerate() {
            let Some(key) = key else { continue };
            if key.historical_kind == 0 && key.arena_id == arena_id {
                if result.is_some() {
                    return Err(FlatDecisionErrorV1::InconsistentReference);
                }
                result = Some(usize_u32(index)?);
            }
        }
        result.ok_or(FlatDecisionErrorV1::InvalidReference)
    }

    fn ensure_context_ref(
        &mut self,
        stable: &CardStableRefV1,
        actor: PlayerSeatV1,
        group: FlatObjectGroupV1,
        source_kind: FlatObjectSourceKindV1,
        ordinal: u32,
        allow_detached: bool,
    ) -> Result<u32, FlatDecisionErrorV1> {
        match self.resolve_live(stable, actor) {
            Ok(index) => Ok(index),
            Err(FlatDecisionErrorV1::InvalidReference) if allow_detached => {
                self.add_stable(stable, actor, group, source_kind, ordinal, 0)
            }
            Err(error) => Err(error),
        }
    }

    fn register_objects(&mut self, observation: &ObservationV5) -> Result<(), FlatDecisionErrorV1> {
        let actor = observation.acting_player;
        let opponent = opponent(actor);
        let p = &observation.projection.surface;
        let turn = p.turn;

        for (order, card) in observation.own_hand.iter().enumerate() {
            self.add_private_card(
                card,
                actor,
                FlatObjectGroupV1::SelfHand,
                FlatObjectSourceKindV1::Card,
                usize_u32(order)?,
                0,
            )?;
        }
        for (seat, group) in [
            (actor, FlatObjectGroupV1::SelfBattlefield),
            (opponent, FlatObjectGroupV1::OpponentBattlefield),
        ] {
            for (order, card) in p.battlefield[seat_index(seat)].iter().enumerate() {
                self.add_public_card(card, actor, group, usize_u32(order)?, turn)?;
            }
        }
        for (seat, group) in [
            (actor, FlatObjectGroupV1::SelfGraveyard),
            (opponent, FlatObjectGroupV1::OpponentGraveyard),
        ] {
            for (order, card) in p.graveyards[seat_index(seat)].iter().enumerate() {
                self.add_public_card(card, actor, group, usize_u32(order)?, turn)?;
            }
        }
        for (order, card) in p.exile.iter().enumerate() {
            self.add_public_card(
                card,
                actor,
                FlatObjectGroupV1::Exile,
                usize_u32(order)?,
                turn,
            )?;
        }
        for (order, item) in p.stack.iter().enumerate() {
            self.add_stable(
                &item.source,
                actor,
                FlatObjectGroupV1::Stack,
                FlatObjectSourceKindV1::Stack,
                usize_u32(order)?,
                0,
            )?;
        }
        for order in 0..p.combat.ordered_attackers.len() {
            self.add_context_object(
                FlatObjectGroupV1::Combat,
                FlatObjectSourceKindV1::Combat,
                usize_u32(order)?,
            )?;
        }
        for order in 0..p.continuous_effects.len() {
            self.add_context_object(
                FlatObjectGroupV1::ContinuousEffect,
                FlatObjectSourceKindV1::Effect,
                usize_u32(order)?,
            )?;
        }
        for order in 0..p.exile_play_permissions.len() {
            self.add_context_object(
                FlatObjectGroupV1::Permission,
                FlatObjectSourceKindV1::Permission,
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
                    FlatObjectGroupV1::Attachment,
                    FlatObjectSourceKindV1::Attachment,
                    attachment_context_order,
                )?;
                attachment_context_order = attachment_context_order
                    .checked_add(1)
                    .ok_or(FlatDecisionErrorV1::CheckedIntegerRange)?;
            }
        }
        for (stack_order, item) in p.stack.iter().enumerate() {
            for (target_order, target) in item.targets.iter().enumerate() {
                if let TargetRefV1::Object { object } = target {
                    let ordinal = usize_u32(
                        stack_order
                            .checked_mul(65_536)
                            .and_then(|v| v.checked_add(target_order))
                            .ok_or(FlatDecisionErrorV1::CheckedIntegerRange)?,
                    )?;
                    self.add_stable(
                        object,
                        actor,
                        FlatObjectGroupV1::HistoricalStackTarget,
                        FlatObjectSourceKindV1::Target,
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
                    FlatObjectGroupV1::CombatBlock,
                    FlatObjectSourceKindV1::Combat,
                    combat_block_order,
                )?;
                combat_block_order = combat_block_order
                    .checked_add(1)
                    .ok_or(FlatDecisionErrorV1::CheckedIntegerRange)?;
            }
        }

        let mut pending_ordinal = 0_u32;
        let engine = &p.engine_context;
        let mut register_detached =
            |this: &mut Self, value: Option<&CardStableRefV1>| -> Result<(), FlatDecisionErrorV1> {
                if let Some(stable) = value {
                    this.ensure_context_ref(
                        stable,
                        actor,
                        FlatObjectGroupV1::PendingContext,
                        FlatObjectSourceKindV1::Pending,
                        pending_ordinal,
                        true,
                    )?;
                    pending_ordinal = pending_ordinal
                        .checked_add(1)
                        .ok_or(FlatDecisionErrorV1::CheckedIntegerRange)?;
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
                .map(|_| FlatContextKindV1::PendingCast),
            engine
                .pending_activation
                .as_ref()
                .map(|_| FlatContextKindV1::PendingActivation),
            engine
                .pending_discard
                .as_ref()
                .map(|_| FlatContextKindV1::PendingDiscard),
            engine
                .pending_optional_cost
                .as_ref()
                .map(|_| FlatContextKindV1::PendingOptionalCost),
            engine
                .pending_optional_cost_sacrifice
                .as_ref()
                .map(|_| FlatContextKindV1::PendingOptionalCostSacrifice),
            engine
                .pending_spell_copy
                .as_ref()
                .map(|_| FlatContextKindV1::PendingSpellCopy),
            engine
                .pending_effect
                .as_ref()
                .map(|_| FlatContextKindV1::PendingEffect),
        ]
        .into_iter()
        .flatten()
        {
            self.add_context_object(
                FlatObjectGroupV1::PendingContext,
                FlatObjectSourceKindV1::Pending,
                context_object_ordinal(context, 0),
            )?;
        }
        for (order, _) in engine.pending_triggers.iter().enumerate() {
            self.add_context_object(
                FlatObjectGroupV1::PendingContext,
                FlatObjectSourceKindV1::Pending,
                context_object_ordinal(FlatContextKindV1::PendingTrigger, usize_u32(order)?),
            )?;
        }
        let surface = &p.surface_context;
        for context in [
            surface
                .madness_cast_reprompt_source
                .as_ref()
                .map(|_| FlatContextKindV1::MadnessCastReprompt),
            surface
                .private_blockers
                .as_ref()
                .map(|_| FlatContextKindV1::PrivateBlockers),
            surface
                .private_discard
                .as_ref()
                .map(|_| FlatContextKindV1::PrivateDiscard),
            surface
                .private_optional_cost
                .as_ref()
                .map(|_| FlatContextKindV1::PrivateOptionalCost),
            observation
                .projection
                .policy_surface_context
                .private_combat_selection
                .as_ref()
                .map(|_| FlatContextKindV1::PrivateCombatSelection),
        ]
        .into_iter()
        .flatten()
        {
            self.add_context_object(
                FlatObjectGroupV1::PrivateContext,
                FlatObjectSourceKindV1::Private,
                context_object_ordinal(context, 0),
            )?;
        }

        for (relative_owner, seat) in [actor, opponent].into_iter().enumerate() {
            let group = if relative_owner == 0 {
                FlatObjectGroupV1::KnownSelfLibrary
            } else {
                FlatObjectGroupV1::KnownOpponentLibrary
            };
            for entry in &observation.known_library_cards[seat_index(seat)] {
                self.add_private_card(
                    &entry.card,
                    actor,
                    group,
                    FlatObjectSourceKindV1::KnownLibrary,
                    entry.position,
                    0,
                )?;
            }
        }
        for (relative_owner, seat) in [actor, opponent].into_iter().enumerate() {
            let group = if relative_owner == 0 {
                FlatObjectGroupV1::KnownSelfHand
            } else {
                FlatObjectGroupV1::KnownOpponentHand
            };
            for (order, card) in observation.known_hand_cards[seat_index(seat)]
                .iter()
                .enumerate()
            {
                self.add_private_card(
                    card,
                    actor,
                    group,
                    FlatObjectSourceKindV1::KnownHand,
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
                    FlatObjectGroupV1::HistoricalPaidCost,
                    FlatObjectSourceKindV1::PaidCost,
                    paid_order,
                    HISTORICAL_PAID_COST_KIND_V1,
                )?;
                paid_order = paid_order
                    .checked_add(1)
                    .ok_or(FlatDecisionErrorV1::CheckedIntegerRange)?;
            }
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)] // Keeps the audited typed row fields explicit at each producer site.
    fn push_relation(
        &mut self,
        role: FlatRelationRoleV1,
        source_object: Option<u32>,
        target_object: Option<u32>,
        primary_order: u32,
        secondary_order: u32,
        associated_order: u32,
        payload: FlatRelationPayloadV1,
    ) {
        self.relations.push(FlatRelationV1 {
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
    ) -> FlatRelationPayloadV1 {
        let (target_kind, target_player) = target.map_or(
            (FlatTargetKindV1::None, FlatRelativePlayerV1::None),
            |target| target_parts(target, actor),
        );
        let target_object_controller = match target {
            Some(TargetRefV1::Object { object }) => relative_player(object.controller, actor),
            _ => FlatRelativePlayerV1::None,
        };
        FlatRelationPayloadV1::Stack(FlatStackRelationDataV1 {
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
    ) -> FlatRelationPayloadV1 {
        FlatRelationPayloadV1::Effect(FlatEffectRelationDataV1 {
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
    ) -> FlatPermissionRelationDataV1 {
        let (expiry, holder_turn_started) = match permission.expiry {
            PlayPermissionExpiryV2::EndOfTurn => (0, false),
            PlayPermissionExpiryV2::UntilHoldersNextTurn {
                holder_turn_started,
            } => (1, holder_turn_started),
        };
        FlatPermissionRelationDataV1 {
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
        context: FlatContextKindV1,
        subrole: FlatContextSubroleV1,
        actor: PlayerSeatV1,
        target: Option<&TargetRefV1>,
        controller: Option<PlayerSeatV1>,
        trigger_kind: Option<PendingTriggerKindV2>,
        kicked: bool,
    ) -> FlatRelationPayloadV1 {
        let (target_kind, target_player) = target.map_or(
            (FlatTargetKindV1::None, FlatRelativePlayerV1::None),
            |value| target_parts(value, actor),
        );
        FlatRelationPayloadV1::Context(FlatContextRelationDataV1 {
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
        role: FlatRelationRoleV1,
        context: FlatContextKindV1,
        subrole: FlatContextSubroleV1,
        stable: Option<&CardStableRefV1>,
        primary_order: u32,
        secondary_order: u32,
        associated_order: u32,
        controller: Option<PlayerSeatV1>,
        trigger_kind: Option<PendingTriggerKindV2>,
        kicked: bool,
    ) -> Result<(), FlatDecisionErrorV1> {
        let Some(stable) = stable else { return Ok(()) };
        let object = self.resolve_reference(stable, actor)?;
        let context_order = if context == FlatContextKindV1::PendingTrigger {
            primary_order
        } else {
            0
        };
        let context_object = self.context_object_index(
            if role == FlatRelationRoleV1::PendingContext {
                FlatObjectGroupV1::PendingContext
            } else {
                FlatObjectGroupV1::PrivateContext
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
        context: FlatContextKindV1,
        subrole: FlatContextSubroleV1,
        target: &TargetRefV1,
        primary_order: u32,
        controller: Option<PlayerSeatV1>,
    ) -> Result<(), FlatDecisionErrorV1> {
        let target_object = match target {
            TargetRefV1::Object { object } => Some(self.resolve_reference(object, actor)?),
            TargetRefV1::Player { .. } => None,
        };
        let context_object = self.context_object_index(
            FlatObjectGroupV1::PendingContext,
            context_object_ordinal(context, 0),
        )?;
        self.push_relation(
            FlatRelationRoleV1::PendingContext,
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

    fn build_relations(&mut self, observation: &ObservationV5) -> Result<(), FlatDecisionErrorV1> {
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
                self.context_object_index(FlatObjectGroupV1::Attachment, attachment_order)?;
            self.push_relation(
                FlatRelationRoleV1::Attachment,
                Some(context),
                Some(host),
                attachment_order,
                0,
                0,
                FlatRelationPayloadV1::None,
            );
            self.push_relation(
                FlatRelationRoleV1::Attachment,
                Some(context),
                Some(attachment),
                attachment_order,
                0,
                1,
                FlatRelationPayloadV1::None,
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
                    object_relations.push((FlatRelationRoleV1::AttachedTo, source, target));
                }
                ObjectRelationPublicV4::ExiledBy { object, exiled_by } => {
                    let source = self.resolve_live(object, actor)?;
                    let target = self.resolve_live(exiled_by, actor)?;
                    object_relations.push((FlatRelationRoleV1::ExiledBy, source, target));
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
                FlatRelationPayloadV1::None,
            );
        }
        for (stack_order, item) in p.stack.iter().enumerate() {
            let stack_order = usize_u32(stack_order)?;
            let source = self.resolve_live(&item.source, actor)?;
            self.push_relation(
                FlatRelationRoleV1::StackTarget,
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
                    FlatRelationRoleV1::StackTarget,
                    Some(source),
                    target_object,
                    stack_order,
                    usize_u32(target_order)?
                        .checked_add(1)
                        .ok_or(FlatDecisionErrorV1::CheckedIntegerRange)?,
                    0,
                    Self::stack_payload(item, actor, Some(target)),
                );
            }
            for (paid_order, paid) in item.paid_cost_refs.iter().enumerate() {
                let paid = self.resolve_reference(paid, actor)?;
                self.push_relation(
                    FlatRelationRoleV1::PaidCost,
                    Some(source),
                    Some(paid),
                    stack_order,
                    usize_u32(paid_order)?,
                    0,
                    FlatRelationPayloadV1::None,
                );
            }
        }
        let blocked = &p.combat.attacker_to_ordered_blockers;
        for (attacker_order, attacker) in p.combat.ordered_attackers.iter().enumerate() {
            let attacker_order = usize_u32(attacker_order)?;
            let object = self.resolve_live(attacker, actor)?;
            let was_blocked = blocked.iter().any(|(candidate, _)| {
                candidate.arena_id == attacker.arena_id
                    && candidate.zone_change_count == attacker.zone_change_count
            });
            self.push_relation(
                FlatRelationRoleV1::CombatAttacker,
                Some(self.context_object_index(FlatObjectGroupV1::Combat, attacker_order)?),
                Some(object),
                attacker_order,
                0,
                0,
                FlatRelationPayloadV1::CombatAttacker { was_blocked },
            );
        }
        let mut combat_block_order = 0_u32;
        for (attacker_order, (attacker, blockers)) in blocked.iter().enumerate() {
            let attacker = self.resolve_live(attacker, actor)?;
            for (blocker_order, blocker) in blockers.iter().enumerate() {
                let blocker = self.resolve_live(blocker, actor)?;
                let context =
                    self.context_object_index(FlatObjectGroupV1::CombatBlock, combat_block_order)?;
                self.push_relation(
                    FlatRelationRoleV1::CombatBlocker,
                    Some(context),
                    Some(attacker),
                    usize_u32(attacker_order)?,
                    usize_u32(blocker_order)?,
                    0,
                    FlatRelationPayloadV1::None,
                );
                self.push_relation(
                    FlatRelationRoleV1::CombatBlocker,
                    Some(context),
                    Some(blocker),
                    usize_u32(attacker_order)?,
                    usize_u32(blocker_order)?,
                    1,
                    FlatRelationPayloadV1::None,
                );
                combat_block_order = combat_block_order
                    .checked_add(1)
                    .ok_or(FlatDecisionErrorV1::CheckedIntegerRange)?;
            }
        }
        for (effect_order, effect) in p.continuous_effects.iter().enumerate() {
            let effect_order = usize_u32(effect_order)?;
            let context =
                self.context_object_index(FlatObjectGroupV1::ContinuousEffect, effect_order)?;
            let source = effect
                .source
                .as_ref()
                .map(|source| self.resolve_live(source, actor))
                .transpose()?;
            self.push_relation(
                FlatRelationRoleV1::EffectSource,
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
                    FlatRelationRoleV1::EffectAffected,
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
                    FlatRelationRoleV1::EffectAffected,
                    Some(context),
                    None,
                    effect_order,
                    usize_u32(affected_order)?,
                    1,
                    Self::effect_payload(effect, actor, Some(affected)),
                );
            }
            for (order, &subtype_id) in effect.add_subtype_ids.iter().enumerate() {
                self.effect_subtype_changes.push(FlatEffectSubtypeChangeV1 {
                    effect_order,
                    kind: FlatEffectSubtypeChangeKindV1::Add,
                    order: usize_u32(order)?,
                    subtype_id,
                });
            }
            for (order, &subtype_id) in effect.remove_subtype_ids.iter().enumerate() {
                self.effect_subtype_changes.push(FlatEffectSubtypeChangeV1 {
                    effect_order,
                    kind: FlatEffectSubtypeChangeKindV1::Remove,
                    order: usize_u32(order)?,
                    subtype_id,
                });
            }
        }
        let mut permissions = Vec::with_capacity(p.exile_play_permissions.len());
        for permission in &p.exile_play_permissions {
            if permission.zone_change_generation != permission.object.zone_change_count {
                return Err(FlatDecisionErrorV1::InconsistentReference);
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
                FlatRelationRoleV1::Permission,
                Some(self.context_object_index(FlatObjectGroupV1::Permission, order)?),
                Some(object),
                order,
                0,
                0,
                FlatRelationPayloadV1::Permission(payload),
            );
        }
        for (relative_owner, seat) in [actor, opponent].into_iter().enumerate() {
            for entry in &observation.known_library_cards[seat_index(seat)] {
                let object = self.resolve_reference(&entry.card.stable, actor)?;
                self.push_relation(
                    FlatRelationRoleV1::KnownLibrary,
                    Some(object),
                    Some(object),
                    usize_u32(relative_owner)?,
                    entry.position,
                    0,
                    FlatRelationPayloadV1::Known {
                        owner: relative_player(seat, actor),
                    },
                );
            }
            for (reveal_order, card) in observation.known_hand_cards[seat_index(seat)]
                .iter()
                .enumerate()
            {
                let object = self.resolve_reference(&card.stable, actor)?;
                self.push_relation(
                    FlatRelationRoleV1::KnownHand,
                    Some(object),
                    Some(object),
                    usize_u32(relative_owner)?,
                    usize_u32(reveal_order)?,
                    0,
                    FlatRelationPayloadV1::Known {
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
    ) -> Result<(), FlatDecisionErrorV1> {
        let actor = observation.acting_player;
        let engine = &observation.projection.surface.engine_context;
        if let Some(pending) = &engine.pending_cast {
            self.push_context_ref(
                actor,
                FlatRelationRoleV1::PendingContext,
                FlatContextKindV1::PendingCast,
                FlatContextSubroleV1::PendingCastSource,
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
                    FlatContextKindV1::PendingCast,
                    FlatContextSubroleV1::PendingCastChosenTarget,
                    target,
                    usize_u32(order)?,
                    Some(pending.controller),
                )?;
            }
            if let Some(discarded) = &pending.additional_cost_discarded {
                for (order, stable) in discarded.iter().enumerate() {
                    self.push_context_ref(
                        actor,
                        FlatRelationRoleV1::PendingContext,
                        FlatContextKindV1::PendingCast,
                        FlatContextSubroleV1::PendingCastDiscarded,
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
                    FlatRelationRoleV1::PendingContext,
                    FlatContextKindV1::PendingCast,
                    FlatContextSubroleV1::PendingCastSacrificed,
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
                FlatRelationRoleV1::PendingContext,
                FlatContextKindV1::PendingActivation,
                FlatContextSubroleV1::PendingActivationSource,
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
                    FlatContextKindV1::PendingActivation,
                    FlatContextSubroleV1::PendingActivationChosenTarget,
                    target,
                    usize_u32(order)?,
                    Some(pending.controller),
                )?;
            }
            if let Some(discarded) = &pending.cost_discard_paid {
                for (order, stable) in discarded.iter().enumerate() {
                    self.push_context_ref(
                        actor,
                        FlatRelationRoleV1::PendingContext,
                        FlatContextKindV1::PendingActivation,
                        FlatContextSubroleV1::PendingActivationDiscarded,
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
                FlatRelationRoleV1::PendingContext,
                FlatContextKindV1::PendingDiscard,
                FlatContextSubroleV1::PendingDiscardResumeSource,
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
                FlatRelationRoleV1::PendingContext,
                FlatContextKindV1::PendingOptionalCost,
                FlatContextSubroleV1::PendingOptionalCostSource,
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
                FlatRelationRoleV1::PendingContext,
                FlatContextKindV1::PendingOptionalCost,
                FlatContextSubroleV1::PendingOptionalCostSpellResumeSource,
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
                FlatRelationRoleV1::PendingContext,
                FlatContextKindV1::PendingOptionalCostSacrifice,
                FlatContextSubroleV1::PendingOptionalSacrificeSource,
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
                    FlatRelationRoleV1::PendingContext,
                    FlatContextKindV1::PendingOptionalCostSacrifice,
                    FlatContextSubroleV1::PendingOptionalSacrificeChosen,
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
                FlatRelationRoleV1::PendingContext,
                FlatContextKindV1::PendingOptionalCostSacrifice,
                FlatContextSubroleV1::PendingOptionalSacrificeSpellResumeSource,
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
                FlatRelationRoleV1::PendingContext,
                FlatContextKindV1::PendingSpellCopy,
                FlatContextSubroleV1::PendingSpellCopyParent,
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
                FlatContextKindV1::PendingSpellCopy,
                FlatContextSubroleV1::PendingSpellCopyInheritedTarget,
                &pending.inherited_target,
                0,
                Some(pending.player),
            )?;
            self.push_context_ref(
                actor,
                FlatRelationRoleV1::PendingContext,
                FlatContextKindV1::PendingSpellCopy,
                FlatContextSubroleV1::PendingSpellCopyCopy,
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
                FlatRelationRoleV1::PendingContext,
                FlatContextKindV1::PendingEffect,
                FlatContextSubroleV1::PendingEffectSource,
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
                        FlatContextKindV1::PendingEffect,
                        FlatContextSubroleV1::PendingEffectSelectedTarget,
                        target,
                        usize_u32(order)?,
                        Some(pending.controller),
                    )?;
                }
                for (order, target) in legal_targets.iter().enumerate() {
                    self.push_context_target(
                        actor,
                        FlatContextKindV1::PendingEffect,
                        FlatContextSubroleV1::PendingEffectLegalTarget,
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
                FlatObjectGroupV1::PendingContext,
                context_object_ordinal(FlatContextKindV1::PendingTrigger, order),
            )?;
            self.push_relation(
                FlatRelationRoleV1::PendingContext,
                Some(context_object),
                object,
                order,
                0,
                0,
                Self::context_payload(
                    FlatContextKindV1::PendingTrigger,
                    FlatContextSubroleV1::PendingTriggerSource,
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
    ) -> Result<(), FlatDecisionErrorV1> {
        let actor = observation.acting_player;
        let surface = &observation.projection.surface.surface_context;
        self.push_context_ref(
            actor,
            FlatRelationRoleV1::PrivateContext,
            FlatContextKindV1::MadnessCastReprompt,
            FlatContextSubroleV1::MadnessCastRepromptSource,
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
                FlatRelationRoleV1::PrivateContext,
                FlatContextKindV1::PrivateBlockers,
                FlatContextSubroleV1::PrivateBlockersCurrentAttacker,
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
                        FlatContextSubroleV1::PrivateBlockersAccumulatedAttacker,
                        attacker,
                        0,
                    ),
                    (
                        FlatContextSubroleV1::PrivateBlockersAccumulatedBlocker,
                        blocker,
                        1,
                    ),
                ] {
                    self.push_context_ref(
                        actor,
                        FlatRelationRoleV1::PrivateContext,
                        FlatContextKindV1::PrivateBlockers,
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
                    FlatRelationRoleV1::PrivateContext,
                    FlatContextKindV1::PrivateBlockers,
                    FlatContextSubroleV1::PrivateBlockersRemainingAttacker,
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
                        FlatRelationRoleV1::PrivateContext,
                        FlatContextKindV1::PrivateBlockers,
                        FlatContextSubroleV1::PrivateBlockersRemainingBlocker,
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
                    FlatRelationRoleV1::PrivateContext,
                    FlatContextKindV1::PrivateDiscard,
                    FlatContextSubroleV1::PrivateDiscardChosen,
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
                    FlatRelationRoleV1::PrivateContext,
                    FlatContextKindV1::PrivateDiscard,
                    FlatContextSubroleV1::PrivateDiscardRemainingChoice,
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
                FlatRelationRoleV1::PrivateContext,
                FlatContextKindV1::PrivateCombatSelection,
                FlatContextSubroleV1::PrivateCombatAttacker,
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
                    FlatRelationRoleV1::PrivateContext,
                    FlatContextKindV1::PrivateCombatSelection,
                    FlatContextSubroleV1::PrivateCombatSelected,
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
                FlatRelationRoleV1::PrivateContext,
                FlatContextKindV1::PrivateCombatSelection,
                FlatContextSubroleV1::PrivateCombatCurrentCandidate,
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
                    FlatRelationRoleV1::PrivateContext,
                    FlatContextKindV1::PrivateCombatSelection,
                    FlatContextSubroleV1::PrivateCombatRemainingCandidate,
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
    ) -> Result<(), FlatDecisionErrorV1> {
        self.clear_typed_cache();
        let action_count = usize::try_from(expected.legal_action_count)
            .map_err(|_| FlatDecisionErrorV1::CheckedIntegerRange)?;
        let max_refs = action_count
            .checked_mul(FLAT_ACTION_MAX_TRIGGER_ORDER_REFS_V1)
            .ok_or(FlatDecisionErrorV1::CheckedIntegerRange)?;
        self.actions
            .resize(action_count, FlatActionCoreV1::default());
        self.action_refs
            .resize(max_refs, FlatActionRefV1::default());
        self.action_objects
            .resize(max_refs, FlatActionObjectV1::default());
        let action_slice = session.encode_current_flat_action_slice_v1(
            expected,
            &mut FlatActionDecisionSliceBuffersV1 {
                actions: &mut self.actions,
                refs: &mut self.action_refs,
                objects: &mut self.action_objects,
            },
        )?;
        self.actions.truncate(
            usize::try_from(action_slice.active_action_count)
                .map_err(|_| FlatDecisionErrorV1::CheckedIntegerRange)?,
        );
        self.action_refs.truncate(
            usize::try_from(action_slice.active_ref_count)
                .map_err(|_| FlatDecisionErrorV1::CheckedIntegerRange)?,
        );
        self.action_objects
            .truncate(usize::from(action_slice.active_object_count));

        let observation = session.flat_policy_observation_v1(expected)?;
        if observation.schema_version != 5
            || observation.acting_player != expected.acting_player
            || observation.step_index != expected.step
            || observation.physical_decision_id != expected.physical_decision_id
            || observation.substep_index != expected.substep_index
            || observation.substep_count != expected.substep_count
            || observation.card_db_hash != action_slice.binding.card_db_hash
        {
            return Err(FlatDecisionErrorV1::ObservationContract);
        }
        self.build_globals(&observation)?;
        self.register_objects(&observation)?;
        self.build_relations(&observation)?;
        self.validate_cached_tables()?;
        self.cached_binding = Some(action_slice.binding);
        Ok(())
    }

    fn validate_cached_tables(&self) -> Result<(), FlatDecisionErrorV1> {
        let object_count = usize_u32(self.objects.len())?;
        for relation in &self.relations {
            if relation
                .source_object
                .is_some_and(|index| index >= object_count)
                || relation
                    .target_object
                    .is_some_and(|index| index >= object_count)
            {
                return Err(FlatDecisionErrorV1::InvalidReference);
            }
        }
        for row in &self.object_subtypes {
            if row.object_index >= object_count {
                return Err(FlatDecisionErrorV1::InvalidReference);
            }
        }
        for row in &self.ability_uses {
            if row.object_index >= object_count {
                return Err(FlatDecisionErrorV1::InvalidReference);
            }
        }
        for row in &self.goads {
            if row.object_index >= object_count {
                return Err(FlatDecisionErrorV1::InvalidReference);
            }
        }
        for (index, object) in self.objects.iter().enumerate() {
            let index = usize_u32(index)?;
            let subtype_end = object
                .subtype_start
                .checked_add(object.subtype_count)
                .ok_or(FlatDecisionErrorV1::CheckedIntegerRange)?;
            let ability_end = object
                .ability_use_start
                .checked_add(object.ability_use_count)
                .ok_or(FlatDecisionErrorV1::CheckedIntegerRange)?;
            let goad_end = object
                .goad_start
                .checked_add(object.goad_count)
                .ok_or(FlatDecisionErrorV1::CheckedIntegerRange)?;
            if subtype_end > usize_u32(self.object_subtypes.len())?
                || ability_end > usize_u32(self.ability_uses.len())?
                || goad_end > usize_u32(self.goads.len())?
                || self.object_subtypes[usize::try_from(object.subtype_start)
                    .map_err(|_| FlatDecisionErrorV1::CheckedIntegerRange)?
                    ..usize::try_from(subtype_end)
                        .map_err(|_| FlatDecisionErrorV1::CheckedIntegerRange)?]
                    .iter()
                    .any(|row| row.object_index != index)
                || self.ability_uses[usize::try_from(object.ability_use_start)
                    .map_err(|_| FlatDecisionErrorV1::CheckedIntegerRange)?
                    ..usize::try_from(ability_end)
                        .map_err(|_| FlatDecisionErrorV1::CheckedIntegerRange)?]
                    .iter()
                    .any(|row| row.object_index != index)
                || self.goads[usize::try_from(object.goad_start)
                    .map_err(|_| FlatDecisionErrorV1::CheckedIntegerRange)?
                    ..usize::try_from(goad_end)
                        .map_err(|_| FlatDecisionErrorV1::CheckedIntegerRange)?]
                    .iter()
                    .any(|row| row.object_index != index)
            {
                return Err(FlatDecisionErrorV1::InvalidReference);
            }
        }
        for (action_object_index, action_object) in self.action_objects.iter().enumerate() {
            let matching_model_objects = self
                .objects
                .iter()
                .zip(&self.object_keys)
                .filter(|(object, key)| {
                    let Some(key) = key else { return false };
                    let group_matches = match action_object.group {
                        FlatActionObjectGroupV1::SelfHand => {
                            object.group == FlatObjectGroupV1::SelfHand
                        }
                        FlatActionObjectGroupV1::KnownOpponentHand => {
                            object.group == FlatObjectGroupV1::KnownOpponentHand
                        }
                        FlatActionObjectGroupV1::SelfBattlefield => {
                            object.group == FlatObjectGroupV1::SelfBattlefield
                        }
                        FlatActionObjectGroupV1::OpponentBattlefield => {
                            object.group == FlatObjectGroupV1::OpponentBattlefield
                        }
                        FlatActionObjectGroupV1::SelfGraveyard => {
                            object.group == FlatObjectGroupV1::SelfGraveyard
                        }
                        FlatActionObjectGroupV1::OpponentGraveyard => {
                            object.group == FlatObjectGroupV1::OpponentGraveyard
                        }
                        FlatActionObjectGroupV1::Exile => object.group == FlatObjectGroupV1::Exile,
                        FlatActionObjectGroupV1::Stack => {
                            object.group == FlatObjectGroupV1::Stack
                                || (object.group == FlatObjectGroupV1::PendingContext
                                    && object.source_kind == FlatObjectSourceKindV1::Pending)
                        }
                        FlatActionObjectGroupV1::Command => false,
                        FlatActionObjectGroupV1::KnownSelfLibrary => {
                            object.group == FlatObjectGroupV1::KnownSelfLibrary
                        }
                        FlatActionObjectGroupV1::KnownOpponentLibrary => {
                            object.group == FlatObjectGroupV1::KnownOpponentLibrary
                        }
                    };
                    let ordinal_matches = object.group == FlatObjectGroupV1::PendingContext
                        || object.visible_ordinal == u32::from(action_object.actor_visible_ordinal);
                    group_matches
                        && ordinal_matches
                        && key.card_token == u32::from(action_object.card_token)
                        && key.owner as u8 == action_object.owner_relative
                        && key.controller as u8 == action_object.controller_relative
                        && key.zone as u8 == action_object.zone
                        && key.zone_change_count == action_object.zone_change_count
                })
                .count();
            if matching_model_objects != 1
                || self.action_refs.iter().any(|reference| {
                    usize::from(reference.object_index) == action_object_index
                        && reference.card_token != action_object.card_token
                })
            {
                return Err(FlatDecisionErrorV1::InvalidReference);
            }
        }
        if self
            .action_refs
            .iter()
            .any(|reference| usize::from(reference.object_index) >= self.action_objects.len())
        {
            return Err(FlatDecisionErrorV1::InvalidReference);
        }
        Ok(())
    }

    fn ensure_cache(
        &mut self,
        session: &FastActorSessionV1,
        expected: FastActorDecisionV1,
    ) -> Result<FlatActionDecisionBindingV1, FlatDecisionErrorV1> {
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
                session.flat_policy_validate_cached_binding_v1(expected, binding)?;
                return Ok(binding);
            }
        }
        self.build_cache(session, expected)?;
        self.cached_binding
            .ok_or(FlatDecisionErrorV1::ObservationContract)
    }
}

macro_rules! require_capacity {
    ($buffer:expr, $source:expr, $variant:ident) => {
        if $buffer.len() < $source.len() {
            return Err(FlatDecisionErrorV1::$variant {
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
    pub fn encode_current_flat_decision_v1(
        &self,
        expected: FastActorDecisionV1,
        encoder: &mut FlatDecisionEncoderV1,
        buffers: &mut FlatDecisionBuffersV1<'_>,
    ) -> Result<FlatDecisionV1, FlatDecisionErrorV1> {
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

        let decision = FlatDecisionV1 {
            binding: FlatDecisionBindingV1 {
                action_binding,
                typed_layout_version: FLAT_POLICY_TYPED_LAYOUT_VERSION_V1,
                feature_inventory_version: FLAT_POLICY_FEATURE_INVENTORY_VERSION_V1,
                enum_mapping_version: FLAT_POLICY_ENUM_MAPPING_VERSION_V1,
                object_group_mapping_version: FLAT_POLICY_OBJECT_GROUP_MAPPING_VERSION_V1,
                relation_role_mapping_version: FLAT_POLICY_RELATION_ROLE_MAPPING_VERSION_V1,
                context_subrole_mapping_version: FLAT_POLICY_CONTEXT_SUBROLE_MAPPING_VERSION_V1,
            },
            globals: encoder.globals,
            active_object_count: usize_u32(encoder.objects.len())?,
            active_relation_count: usize_u32(encoder.relations.len())?,
            active_object_subtype_count: usize_u32(encoder.object_subtypes.len())?,
            active_ability_use_count: usize_u32(encoder.ability_uses.len())?,
            active_goad_count: usize_u32(encoder.goads.len())?,
            active_completed_dungeon_count: usize_u32(encoder.completed_dungeons.len())?,
            active_effect_subtype_change_count: usize_u32(encoder.effect_subtype_changes.len())?,
            active_context_path_element_count: usize_u32(encoder.context_path_elements.len())?,
            active_action_count: usize_u32(encoder.actions.len())?,
            active_action_ref_count: usize_u32(encoder.action_refs.len())?,
            active_action_object_count: usize_u32(encoder.action_objects.len())?,
        };

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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rl::{
        CardCharacteristicsV2, CardTypeFlagsV2, CountersV1, GoadPublicV4, KeywordFlagsV2,
        StackItemPublicV2,
    };
    use crate::rl_session::FastActorResponseV1;

    fn expected(session: &FastActorSessionV1) -> FastActorDecisionV1 {
        let FastActorResponseV1::Decision(expected) = session.current_response() else {
            panic!("expected live decision");
        };
        expected
    }

    fn one_row_encoder(
        session: &FastActorSessionV1,
        expected: FastActorDecisionV1,
    ) -> FlatDecisionEncoderV1 {
        let mut actions = vec![FlatActionCoreV1::default(); 64];
        let mut refs = vec![FlatActionRefV1::default(); 256];
        let mut objects = vec![FlatActionObjectV1::default(); 128];
        let slice = session
            .encode_current_flat_action_slice_v1(
                expected,
                &mut FlatActionDecisionSliceBuffersV1 {
                    actions: &mut actions,
                    refs: &mut refs,
                    objects: &mut objects,
                },
            )
            .unwrap();
        FlatDecisionEncoderV1 {
            cached_binding: Some(slice.binding),
            globals: FlatGlobalsV1::default(),
            objects: vec![FlatObjectCoreV1::default()],
            object_keys: vec![None],
            relations: vec![FlatRelationV1::default()],
            object_subtypes: vec![FlatObjectSubtypeV1::default()],
            ability_uses: vec![FlatObjectAbilityUseV1::default()],
            goads: vec![FlatObjectGoadV1::default()],
            completed_dungeons: vec![FlatCompletedDungeonV1::default()],
            effect_subtype_changes: vec![FlatEffectSubtypeChangeV1::default()],
            context_path_elements: vec![FlatContextPathElementV1::default()],
            actions: vec![FlatActionCoreV1::default()],
            action_refs: vec![FlatActionRefV1::default()],
            action_objects: vec![FlatActionObjectV1::default()],
        }
    }

    fn materialize_observation(
        observation: &ObservationV5,
    ) -> Result<FlatDecisionEncoderV1, FlatDecisionErrorV1> {
        let mut encoder = FlatDecisionEncoderV1::default();
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

    fn assert_same_public_tables(a: &FlatDecisionEncoderV1, b: &FlatDecisionEncoderV1) {
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
        let mut coalescing = FlatDecisionEncoderV1::default();
        let historical_index = coalescing
            .add_stable(
                &coalesced_stable,
                actor,
                FlatObjectGroupV1::HistoricalStackTarget,
                FlatObjectSourceKindV1::Target,
                0,
                HISTORICAL_STACK_TARGET_KIND_V1,
            )
            .unwrap();
        let later_live_index = coalescing
            .add_stable(
                &coalesced_stable,
                actor,
                FlatObjectGroupV1::PendingContext,
                FlatObjectSourceKindV1::Pending,
                1,
                0,
            )
            .unwrap();
        assert_eq!(historical_index, later_live_index);
        assert_eq!(coalescing.objects.len(), 1);

        let session = FastActorSessionV1::reset_with_limits(90_024, 124, 128, 16_384);
        let expected = expected(&session);
        let mut observation = session.flat_policy_observation_v1(expected).unwrap();
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
                relation.role == FlatRelationRoleV1::StackTarget && relation.secondary_order == 1
            })
            .unwrap();
        let target_object = target_relation.target_object.unwrap();
        assert_eq!(
            changed_controller.objects[usize::try_from(target_object).unwrap()].group,
            FlatObjectGroupV1::SelfBattlefield
        );
        assert!(matches!(
            target_relation.payload,
            FlatRelationPayloadV1::Stack(FlatStackRelationDataV1 {
                target_object_controller: FlatRelativePlayerV1::Opponent,
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
            assert_eq!(error, FlatDecisionErrorV1::InconsistentReference);
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
            assert_eq!(error, FlatDecisionErrorV1::InvalidReference);
        }
    }

    #[test]
    fn set_like_relation_inputs_have_one_canonical_typed_order() {
        let session = FastActorSessionV1::reset_with_limits(90_025, 125, 128, 16_384);
        let expected = expected(&session);
        let mut observation = session.flat_policy_observation_v1(expected).unwrap();
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
                object.group == FlatObjectGroupV1::Exile
                    && object.card_token == card_token(exile_a.card_db_id)
            })
            .map(|index| u32::try_from(index).unwrap())
            .unwrap();
        let permission_tie_breaks: Vec<_> = baseline
            .relations
            .iter()
            .filter(|relation| {
                relation.role == FlatRelationRoleV1::Permission
                    && relation.target_object == Some(exile_a_index)
            })
            .map(|relation| match relation.payload {
                FlatRelationPayloadV1::Permission(payload) => (
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
                (FlatRelativePlayerV1::Opponent, 0, 0, false),
                (FlatRelativePlayerV1::SelfPlayer, 1, 0, false),
                (FlatRelativePlayerV1::SelfPlayer, 0, 1, false),
                (FlatRelativePlayerV1::SelfPlayer, 0, 1, true),
                (FlatRelativePlayerV1::SelfPlayer, 0, 0, false),
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
        let session = FastActorSessionV1::reset_with_limits(90_026, 126, 128, 16_384);
        let expected = expected(&session);
        let mut observation = session.flat_policy_observation_v1(expected).unwrap();
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
        let session = FastActorSessionV1::reset_with_limits(90_021, 121, 128, 16_384);
        let expected = expected(&session);
        let observation = session.flat_policy_observation_v1(expected).unwrap();
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
        encoder: &FlatDecisionEncoderV1,
    ) -> (Vec<FlatRelationV1>, Vec<FlatEffectSubtypeChangeV1>) {
        (
            encoder
                .relations
                .iter()
                .copied()
                .filter(|row| {
                    matches!(
                        row.role,
                        FlatRelationRoleV1::EffectSource | FlatRelationRoleV1::EffectAffected
                    )
                })
                .collect(),
            encoder.effect_subtype_changes.clone(),
        )
    }

    #[test]
    fn every_model_effect_field_is_explicit_while_timestamp_is_operational_only() {
        let session = FastActorSessionV1::reset_with_limits(90_022, 122, 128, 16_384);
        let expected = expected(&session);
        let mut observation = session.flat_policy_observation_v1(expected).unwrap();
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
            row.role == FlatRelationRoleV1::EffectAffected
                && row.target_object.is_none()
                && matches!(
                    row.payload,
                    FlatRelationPayloadV1::Effect(FlatEffectRelationDataV1 {
                        affected_player: FlatRelativePlayerV1::Opponent,
                        ..
                    })
                )
        }));
    }

    #[test]
    fn actor_seat_swap_preserves_the_actor_relative_initial_state_and_effect_rows() {
        let session = FastActorSessionV1::reset_with_limits(90_023, 123, 128, 16_384);
        let expected = expected(&session);
        let mut observation = session.flat_policy_observation_v1(expected).unwrap();
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
            FlatRelativePlayerV1::SelfPlayer
        );
        assert!(baseline
            .objects
            .iter()
            .filter(|object| object.group == FlatObjectGroupV1::SelfHand)
            .all(|object| {
                object.owner == FlatRelativePlayerV1::SelfPlayer
                    && object.controller == FlatRelativePlayerV1::SelfPlayer
            }));
        assert!(baseline.relations.iter().any(|relation| {
            matches!(
                relation.payload,
                FlatRelationPayloadV1::Effect(FlatEffectRelationDataV1 {
                    controller: FlatRelativePlayerV1::SelfPlayer,
                    affected_player: FlatRelativePlayerV1::Opponent,
                    ..
                })
            )
        }));
    }

    #[test]
    fn every_table_reports_its_exact_capacity_before_any_publication() {
        let session = FastActorSessionV1::reset_with_limits(90_020, 120, 128, 16_384);
        let expected = expected(&session);
        let mut encoder = one_row_encoder(&session, expected);

        macro_rules! assert_short {
            ($field:ident, $error:expr) => {{
                let poison_object = FlatObjectCoreV1 {
                    card_token: 65_536,
                    ..FlatObjectCoreV1::default()
                };
                let poison_relation = FlatRelationV1 {
                    primary_order: u32::MAX,
                    ..FlatRelationV1::default()
                };
                let mut objects = [poison_object];
                let mut relations = [poison_relation];
                let mut object_subtypes = [FlatObjectSubtypeV1 {
                    subtype_id: u16::MAX,
                    ..FlatObjectSubtypeV1::default()
                }];
                let mut ability_uses = [FlatObjectAbilityUseV1 {
                    uses: u16::MAX,
                    ..FlatObjectAbilityUseV1::default()
                }];
                let mut goads = [FlatObjectGoadV1 {
                    expires_after_turns: u32::MAX,
                    ..FlatObjectGoadV1::default()
                }];
                let mut completed_dungeons = [FlatCompletedDungeonV1 {
                    dungeon_id: u16::MAX,
                    ..FlatCompletedDungeonV1::default()
                }];
                let mut effect_subtype_changes = [FlatEffectSubtypeChangeV1 {
                    subtype_id: u16::MAX,
                    ..FlatEffectSubtypeChangeV1::default()
                }];
                let mut context_path_elements = [FlatContextPathElementV1 {
                    value: u16::MAX,
                    ..FlatContextPathElementV1::default()
                }];
                let mut actions = [FlatActionCoreV1 {
                    flags: u16::MAX,
                    ..FlatActionCoreV1::default()
                }];
                let mut action_refs = [FlatActionRefV1 {
                    action_index: u32::MAX,
                    ..FlatActionRefV1::default()
                }];
                let mut action_objects = [FlatActionObjectV1 {
                    card_token: u16::MAX,
                    ..FlatActionObjectV1::default()
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
                let mut buffers = FlatDecisionBuffersV1 {
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
                        .encode_current_flat_decision_v1(expected, &mut encoder, &mut buffers,)
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
            FlatDecisionErrorV1::InsufficientObjectCapacity {
                required: 1,
                available: 0
            }
        );
        assert_short!(
            relations,
            FlatDecisionErrorV1::InsufficientRelationCapacity {
                required: 1,
                available: 0
            }
        );
        assert_short!(
            object_subtypes,
            FlatDecisionErrorV1::InsufficientObjectSubtypeCapacity {
                required: 1,
                available: 0
            }
        );
        assert_short!(
            ability_uses,
            FlatDecisionErrorV1::InsufficientAbilityUseCapacity {
                required: 1,
                available: 0
            }
        );
        assert_short!(
            goads,
            FlatDecisionErrorV1::InsufficientGoadCapacity {
                required: 1,
                available: 0
            }
        );
        assert_short!(
            completed_dungeons,
            FlatDecisionErrorV1::InsufficientCompletedDungeonCapacity {
                required: 1,
                available: 0
            }
        );
        assert_short!(
            effect_subtype_changes,
            FlatDecisionErrorV1::InsufficientEffectSubtypeCapacity {
                required: 1,
                available: 0
            }
        );
        assert_short!(
            context_path_elements,
            FlatDecisionErrorV1::InsufficientContextPathCapacity {
                required: 1,
                available: 0
            }
        );
        assert_short!(
            actions,
            FlatDecisionErrorV1::InsufficientActionCapacity {
                required: 1,
                available: 0
            }
        );
        assert_short!(
            action_refs,
            FlatDecisionErrorV1::InsufficientActionRefCapacity {
                required: 1,
                available: 0
            }
        );
        assert_short!(
            action_objects,
            FlatDecisionErrorV1::InsufficientActionObjectCapacity {
                required: 1,
                available: 0
            }
        );
    }
}
