//! Stable RL-facing contracts for the kernel-owned trainer/runner boundary.
//!
//! This module is intentionally data-shaped. It exposes a perspective-safe
//! observation projection, structured legal-action ids, versioned JSONL episode
//! records, generic Burn/Rally deck-pair construction, and a backward-compatible
//! deterministic Burn-mirror rollout helper. It does not make any learning or
//! strength claim.

use crate::card_def::{
    preflight_fully_supported_deck, CardType, DeckPreflightError, Keywords, CARD_DEFS,
    KERNEL_CARDDB_HASH,
};
use crate::engine::{
    self, Action, CastMode, CostKind, Decision, OptionalCostChoice, PlayOrCast,
    PlayPermissionExpiry, UntilEndOfTurnEffect,
};
use crate::event::{self, ProposedEvent};
use crate::ids::{ObjectId, PlayerId};
use crate::mana::ManaColor;
use crate::policy_surface_v5::{
    PolicyActionV5, PolicyDecisionV5, PolicySurfaceContextIdsV5, PolicySurfaceStageV5,
    PolicySurfaceV5, POLICY_SURFACE_VERSION,
};
use crate::rl_session::{RlEpisodeSessionV1, RlSessionResponseV1};
use crate::runtime_decks::runtime_deck_by_id;
use crate::state::{
    CastMethodV4, DungeonStateV4, GameObject, GameState, PaidCostRefV4, SplitMix64, StackItem,
    StackItemKind, Target, Zone, DIAGNOSTIC_STATE_HASH_ALGORITHM,
};
use crate::surface_v2::{SurfaceAction, SurfaceDecision, H2_PREDICATE_VERSION};
use crate::KERNEL_VERSION;
use serde::de::{self, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize};
use std::collections::{BTreeSet, HashSet};
use std::fmt;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};

pub const OBSERVATION_SCHEMA_VERSION_V1: u32 = 1;
pub const OBSERVATION_SCHEMA_VERSION: u32 = 4;
pub const LEGAL_ACTION_SCHEMA_VERSION: u32 = 4;
pub const OBSERVATION_SCHEMA_VERSION_V5: u32 = 5;
pub const LEGAL_ACTION_SCHEMA_VERSION_V5: u32 = 5;
pub const AUDIT_EPISODE_SCHEMA_VERSION: u32 = 10;
pub const POLICY_EPISODE_SCHEMA_VERSION: u32 = 5;
pub const MANIFEST_SCHEMA_VERSION: u32 = 8;
pub const DEFAULT_MAX_PHYSICAL_DECISIONS: u64 = 200_000;
pub const DEFAULT_MAX_POLICY_STEPS: u64 = 25_600_000;
pub const BURN_MIRROR_MATCHUP: &str = "burn_mirror";
pub const AUDIT_EPISODE_JSONL_FILENAME: &str = "audit_episodes.jsonl";
pub const POLICY_EPISODE_JSONL_FILENAME: &str = "policy_episodes.jsonl";
pub const MANIFEST_FILENAME: &str = "manifest.json";

const MAX_SUBSET_OBJECTS: usize = 12;
const MAX_TRIGGER_ORDER_OBJECTS: usize = 7;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RlContractError(pub String);

impl fmt::Display for RlContractError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for RlContractError {}

impl From<std::io::Error> for RlContractError {
    fn from(value: std::io::Error) -> Self {
        RlContractError(value.to_string())
    }
}

impl From<serde_json::Error> for RlContractError {
    fn from(value: serde_json::Error) -> Self {
        RlContractError(value.to_string())
    }
}

type Result<T> = std::result::Result<T, RlContractError>;

/// A raw JSON value whose deserializer rejects duplicate object keys at every
/// nesting level. `serde_json::Value` otherwise accepts duplicates using
/// last-key-wins semantics, which is unsafe at the policy artifact boundary.
struct StrictJsonValue(serde_json::Value);

impl<'de> Deserialize<'de> for StrictJsonValue {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct StrictJsonValueVisitor;

        impl<'de> Visitor<'de> for StrictJsonValueVisitor {
            type Value = StrictJsonValue;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a JSON value without duplicate object keys")
            }

            fn visit_bool<E>(self, value: bool) -> std::result::Result<Self::Value, E> {
                Ok(StrictJsonValue(serde_json::Value::Bool(value)))
            }

            fn visit_i64<E>(self, value: i64) -> std::result::Result<Self::Value, E> {
                Ok(StrictJsonValue(serde_json::Value::Number(value.into())))
            }

            fn visit_u64<E>(self, value: u64) -> std::result::Result<Self::Value, E> {
                Ok(StrictJsonValue(serde_json::Value::Number(value.into())))
            }

            fn visit_f64<E>(self, value: f64) -> std::result::Result<Self::Value, E>
            where
                E: de::Error,
            {
                serde_json::Number::from_f64(value)
                    .map(serde_json::Value::Number)
                    .map(StrictJsonValue)
                    .ok_or_else(|| E::custom("non-finite JSON number"))
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E> {
                Ok(StrictJsonValue(serde_json::Value::String(
                    value.to_string(),
                )))
            }

            fn visit_string<E>(self, value: String) -> std::result::Result<Self::Value, E> {
                Ok(StrictJsonValue(serde_json::Value::String(value)))
            }

            fn visit_none<E>(self) -> std::result::Result<Self::Value, E> {
                Ok(StrictJsonValue(serde_json::Value::Null))
            }

            fn visit_unit<E>(self) -> std::result::Result<Self::Value, E> {
                Ok(StrictJsonValue(serde_json::Value::Null))
            }

            fn visit_seq<A>(self, mut sequence: A) -> std::result::Result<Self::Value, A::Error>
            where
                A: SeqAccess<'de>,
            {
                let mut values = Vec::new();
                while let Some(value) = sequence.next_element::<StrictJsonValue>()? {
                    values.push(value.0);
                }
                Ok(StrictJsonValue(serde_json::Value::Array(values)))
            }

            fn visit_map<A>(self, mut map: A) -> std::result::Result<Self::Value, A::Error>
            where
                A: MapAccess<'de>,
            {
                let mut values = serde_json::Map::new();
                while let Some(key) = map.next_key::<String>()? {
                    if values.contains_key(&key) {
                        return Err(<A::Error as de::Error>::custom("duplicate JSON object key"));
                    }
                    let value = map.next_value::<StrictJsonValue>()?;
                    values.insert(key, value.0);
                }
                Ok(StrictJsonValue(serde_json::Value::Object(values)))
            }
        }

        deserializer.deserialize_any(StrictJsonValueVisitor)
    }
}

pub(crate) fn parse_strict_json_value(input: &str) -> serde_json::Result<serde_json::Value> {
    serde_json::from_str::<StrictJsonValue>(input).map(|value| value.0)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlayerSeatV1 {
    P0,
    P1,
}

impl From<PlayerId> for PlayerSeatV1 {
    fn from(value: PlayerId) -> Self {
        match value {
            PlayerId::P0 => PlayerSeatV1::P0,
            PlayerId::P1 => PlayerSeatV1::P1,
            _ => panic!("unsupported player id {}", value.0),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CountersV1 {
    pub plus1_plus1: i16,
    pub minus1_minus1: i16,
    pub minus0_minus1: i16,
    pub stun: i16,
    pub lore: i16,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CardStableRefV1 {
    pub arena_id: u32,
    pub card_db_id: u16,
    pub owner: PlayerSeatV1,
    pub controller: PlayerSeatV1,
    pub zone: Zone,
    pub zone_change_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CardPublicV1 {
    pub stable: CardStableRefV1,
    pub card_name: String,
    pub tapped: bool,
    pub summoning_sick: bool,
    pub damage: u16,
    pub counters: CountersV1,
    pub attachments: Vec<u32>,
    pub plotted_turn: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CardPrivateV1 {
    pub stable: CardStableRefV1,
    pub card_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct KnownLibraryCardV4 {
    pub position: u32,
    pub card: CardPrivateV1,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CardTypeFlagsV2 {
    pub land: bool,
    pub creature: bool,
    pub instant: bool,
    pub sorcery: bool,
    pub artifact: bool,
    pub enchantment: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct KeywordFlagsV2 {
    pub flying: bool,
    pub reach: bool,
    pub haste: bool,
    pub vigilance: bool,
    pub trample: bool,
    pub first_strike: bool,
    pub double_strike: bool,
    pub deathtouch: bool,
    pub menace: bool,
    pub defender: bool,
    pub lifelink: bool,
    pub hexproof: bool,
    pub indestructible: bool,
    pub protection_from_monocolored: bool,
    pub ward_generic: u16,
    pub minimum_blockers: u8,
    pub landwalk_mask: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CardCharacteristicsV2 {
    pub type_flags: CardTypeFlagsV2,
    pub base_power: Option<i32>,
    pub base_toughness: Option<i32>,
    pub effective_power: Option<i32>,
    pub effective_toughness: Option<i32>,
    pub effective_color_mask: u8,
    pub effective_subtype_ids: Vec<u16>,
    pub effective_keywords: KeywordFlagsV2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AbilityUsePublicV4 {
    pub ability_kind: crate::state::AbilityKindV4,
    pub ability_index: u16,
    pub uses: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct GoadPublicV4 {
    pub player: PlayerSeatV1,
    pub expires_at_turn: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CardPublicV2 {
    pub stable: CardStableRefV1,
    pub card_name: String,
    pub tapped: bool,
    pub summoning_sick: bool,
    pub damage: u16,
    pub counters: CountersV1,
    pub attachments: Vec<u32>,
    pub plotted_turn: Option<u32>,
    pub is_token: bool,
    pub face_index: u8,
    pub chosen_color: Option<ManaColor>,
    pub entered_battlefield_turn: Option<u32>,
    pub ability_uses_this_turn: Vec<AbilityUsePublicV4>,
    pub skip_next_untap: bool,
    pub goaded_by: Vec<GoadPublicV4>,
    pub characteristics: CardCharacteristicsV2,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "target_kind", rename_all = "snake_case")]
pub enum TargetRefV1 {
    Player { player: PlayerSeatV1 },
    Object { object: CardStableRefV1 },
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct StackItemPublicV1 {
    pub stack_index: u32,
    pub source: CardStableRefV1,
    pub controller: PlayerSeatV1,
    pub targets: Vec<TargetRefV1>,
    pub is_trigger_or_ability: bool,
    pub is_flashback: bool,
    pub mode_chosen: u8,
    pub madness_offer: bool,
    pub kicked: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StackItemKindV2 {
    Spell,
    ActivatedAbility,
    TriggeredAbility,
    MadnessOffer,
}

impl From<StackItemKind> for StackItemKindV2 {
    fn from(value: StackItemKind) -> Self {
        match value {
            StackItemKind::Spell => StackItemKindV2::Spell,
            StackItemKind::ActivatedAbility => StackItemKindV2::ActivatedAbility,
            StackItemKind::TriggeredAbility => StackItemKindV2::TriggeredAbility,
            StackItemKind::MadnessOffer => StackItemKindV2::MadnessOffer,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct StackItemPublicV2 {
    pub stack_index: u32,
    pub source: CardStableRefV1,
    pub controller: PlayerSeatV1,
    pub targets: Vec<TargetRefV1>,
    pub stack_item_kind: StackItemKindV2,
    pub is_copy: bool,
    pub is_flashback: bool,
    pub mode_chosen: u8,
    pub madness_offer: bool,
    pub kicked: bool,
    pub cast_method: Option<CastMethodV4>,
    pub face_index: u8,
    pub x_value: u16,
    pub paid_cost_refs: Vec<CardStableRefV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PlayerStatusV1 {
    pub has_lost: bool,
    pub lands_played_this_turn: u8,
    pub drew_from_empty: bool,
    pub draws_this_turn: u32,
    pub spells_cast_this_turn: u16,
    pub dungeon: DungeonStateV4,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PublicObservationProjectionV1 {
    pub turn: u32,
    pub phase: ZoneIndependentStepV1,
    pub active_player: PlayerSeatV1,
    pub priority_player: PlayerSeatV1,
    pub life_totals: [i32; 2],
    pub mana_pools: [[u8; 6]; 2],
    pub hand_counts: [usize; 2],
    pub library_counts: [usize; 2],
    pub player_status: [PlayerStatusV1; 2],
    pub battlefield: [Vec<CardPublicV1>; 2],
    pub graveyards: [Vec<CardPublicV1>; 2],
    pub exile: Vec<CardPublicV1>,
    pub stack: Vec<StackItemPublicV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CombatStatePublicV2 {
    pub attackers_declared: bool,
    pub blockers_declared: bool,
    pub ordered_attackers: Vec<CardStableRefV1>,
    pub attacker_to_ordered_blockers: Vec<(CardStableRefV1, Vec<CardStableRefV1>)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EffectDurationV2 {
    EndOfTurn,
    UntilControllersNextTurn,
    WhileAttached,
    WhileSourcePresent,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ContinuousEffectPublicV2 {
    pub source: Option<CardStableRefV1>,
    pub controller: Option<PlayerSeatV1>,
    pub affected_objects: Vec<CardStableRefV1>,
    pub affected_players: Vec<PlayerSeatV1>,
    pub global: bool,
    pub layers: u8,
    pub timestamp: u64,
    pub duration: EffectDurationV2,
    pub power_delta: i32,
    pub toughness_delta: i32,
    pub grants_haste: bool,
    pub set_power: Option<i32>,
    pub set_toughness: Option<i32>,
    pub add_color_mask: u8,
    pub remove_color_mask: u8,
    pub add_subtype_ids: Vec<u16>,
    pub remove_subtype_ids: Vec<u16>,
    pub add_keyword_mask: u32,
    pub remove_keyword_mask: u32,
    pub ward_generic_delta: i16,
    pub minimum_blockers: Option<u8>,
    pub add_landwalk_mask: u8,
    pub remove_landwalk_mask: u8,
    pub prevent_damage_from_color_mask: u8,
    pub damage_cannot_be_prevented: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "relation_kind", rename_all = "snake_case")]
pub enum ObjectRelationPublicV4 {
    AttachedTo {
        object: CardStableRefV1,
        attached_to: CardStableRefV1,
    },
    ExiledBy {
        object: CardStableRefV1,
        exiled_by: CardStableRefV1,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlayOrCastV2 {
    Play,
    Cast,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "expiry_kind", rename_all = "snake_case")]
pub enum PlayPermissionExpiryV2 {
    EndOfTurn,
    UntilHoldersNextTurn { holder_turn_started: bool },
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ExilePlayPermissionPublicV2 {
    pub object: CardStableRefV1,
    pub holder: PlayerSeatV1,
    pub play_or_cast: PlayOrCastV2,
    pub zone_change_generation: u32,
    pub expiry: PlayPermissionExpiryV2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EngineDecisionStageV2 {
    Priority,
    PendingCast,
    PendingActivation,
    PendingDiscard,
    PendingOptionalCost,
    PendingOptionalCostSacrifice,
    PendingSpellCopy,
    PendingEffect,
    PendingTriggers,
    Halted,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PendingCastSemanticV2 {
    pub source: Option<CardStableRefV1>,
    pub controller: PlayerSeatV1,
    pub chosen_targets: Vec<TargetRefV1>,
    pub is_flashback: bool,
    pub cast_mode: Option<CastMode>,
    pub additional_cost_discarded: Option<Vec<CardStableRefV1>>,
    pub mode_chosen: Option<u8>,
    pub origin_zone: Zone,
    pub sacrifice_chosen: Vec<CardStableRefV1>,
    pub kicked: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PendingActivationSemanticV2 {
    pub source: Option<CardStableRefV1>,
    pub controller: PlayerSeatV1,
    pub ability_index: u8,
    pub chosen_targets: Vec<TargetRefV1>,
    pub cost_discard_paid: Option<Vec<CardStableRefV1>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiscardResumeSemanticV2 {
    None,
    FinishCast,
    FinishActivation,
    FinishSpellResolution,
    FinishOptionalCost,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PendingDiscardSemanticV2 {
    pub player: PlayerSeatV1,
    pub count: u32,
    pub resume_stage: DiscardResumeSemanticV2,
    pub resume_source: Option<CardStableRefV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PendingOptionalCostSemanticV2 {
    pub player: PlayerSeatV1,
    pub source: Option<CardStableRefV1>,
    pub discard_cards: u8,
    pub sacrifice_lands: u8,
    pub discard_payable: bool,
    pub sacrifice_payable: bool,
    pub spell_resume_source: Option<CardStableRefV1>,
    pub spell_resume_zone: Option<Zone>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PendingOptionalCostSacrificeSemanticV2 {
    pub player: PlayerSeatV1,
    pub source: Option<CardStableRefV1>,
    pub remaining: u8,
    pub chosen: Vec<CardStableRefV1>,
    pub spell_resume_source: Option<CardStableRefV1>,
    pub spell_resume_zone: Option<Zone>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpellCopyStageV2 {
    Payment,
    Retarget,
    Target,
}

impl From<engine::SpellCopyStage> for SpellCopyStageV2 {
    fn from(value: engine::SpellCopyStage) -> Self {
        match value {
            engine::SpellCopyStage::Payment => SpellCopyStageV2::Payment,
            engine::SpellCopyStage::Retarget => SpellCopyStageV2::Retarget,
            engine::SpellCopyStage::Target => SpellCopyStageV2::Target,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PendingSpellCopySemanticV2 {
    pub parent: Option<CardStableRefV1>,
    pub player: PlayerSeatV1,
    pub inherited_target: TargetRefV1,
    pub stage: SpellCopyStageV2,
    pub copy: Option<CardStableRefV1>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TargetSelectionPurposeV4 {
    EffectTargets,
    CardSelection,
    PermanentSelection,
    PlayerSelection,
    DamageDivision,
    CostPayment,
    LibraryOrder,
    SearchResult,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BooleanChoicePurposeV4 {
    OptionalEffect,
    Shuffle,
    PayCost,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "choice_kind", rename_all = "snake_case")]
pub enum PendingEffectChoiceSemanticV4 {
    Options {
        player: PlayerSeatV1,
        structural_path: Vec<u16>,
        option_count: u16,
    },
    Targets {
        player: PlayerSeatV1,
        structural_path: Vec<u16>,
        selected_targets: Vec<TargetRefV1>,
        legal_targets: Vec<TargetRefV1>,
        min_targets: u16,
        max_targets: u16,
        can_finish: bool,
        ordered: bool,
        purpose: TargetSelectionPurposeV4,
    },
    Color {
        player: PlayerSeatV1,
        structural_path: Vec<u16>,
        legal_colors: Vec<ManaColor>,
    },
    Number {
        player: PlayerSeatV1,
        structural_path: Vec<u16>,
        minimum: i32,
        maximum: i32,
    },
    Boolean {
        player: PlayerSeatV1,
        structural_path: Vec<u16>,
        default: Option<bool>,
        purpose: BooleanChoicePurposeV4,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PendingEffectSemanticV4 {
    pub source: Option<CardStableRefV1>,
    pub controller: PlayerSeatV1,
    pub choice: Option<PendingEffectChoiceSemanticV4>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PendingTriggerKindV2 {
    TriggeredAbility,
    MadnessOffer,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PendingTriggerSemanticV2 {
    pub source: Option<CardStableRefV1>,
    pub controller: PlayerSeatV1,
    pub trigger_kind: PendingTriggerKindV2,
    pub kicked: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EngineContextV2 {
    pub priority_passes: [bool; 2],
    pub stack_nonempty: bool,
    pub stack_activity_since_priority_boundary: bool,
    pub mana_activity_since_priority_boundary: bool,
    pub last_mana_ability_activator_since_priority_boundary: Option<PlayerSeatV1>,
    pub current_stage: EngineDecisionStageV2,
    pub pending_cast: Option<PendingCastSemanticV2>,
    pub pending_activation: Option<PendingActivationSemanticV2>,
    pub pending_discard: Option<PendingDiscardSemanticV2>,
    pub pending_optional_cost: Option<PendingOptionalCostSemanticV2>,
    pub pending_optional_cost_sacrifice: Option<PendingOptionalCostSacrificeSemanticV2>,
    pub pending_spell_copy: Option<PendingSpellCopySemanticV2>,
    pub pending_effect: Option<PendingEffectSemanticV4>,
    pub pending_triggers: Vec<PendingTriggerSemanticV2>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SurfaceDecisionStageV2 {
    Priority,
    DeclareBlockersForAttacker,
    DiscardPick,
    OptionalCostUse,
    OptionalCostWhich,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PrivateBlockersContextV2 {
    pub current_attacker: Option<CardStableRefV1>,
    pub accumulated: Vec<(CardStableRefV1, CardStableRefV1)>,
    pub remaining: Vec<(CardStableRefV1, Vec<CardStableRefV1>)>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PrivateDiscardContextV2 {
    pub chosen: Vec<CardStableRefV1>,
    pub remaining_choices: Vec<CardStableRefV1>,
    pub remaining_needed: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PrivateOptionalCostContextV2 {
    pub discard_payable: bool,
    pub sacrifice_payable: bool,
    pub stage: SurfaceDecisionStageV2,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct HarnessSurfaceContextV2 {
    pub current_stage: SurfaceDecisionStageV2,
    pub combat_priority_spent: [bool; 2],
    pub combat_priority_rearmed_by_stack_activity: bool,
    pub combat_priority_rearmed_by_mana_activity: bool,
    pub stack_grew_since_round_open: bool,
    pub mana_activity_since_round_open: bool,
    pub stack_length_changed_since_observed: Option<bool>,
    pub mana_activity_since_last_stack_change: bool,
    pub madness_cast_reprompt_source: Option<CardStableRefV1>,
    pub private_blockers: Option<PrivateBlockersContextV2>,
    pub private_discard: Option<PrivateDiscardContextV2>,
    pub private_optional_cost: Option<PrivateOptionalCostContextV2>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicObservationProjectionV2 {
    pub turn: u32,
    pub phase: ZoneIndependentStepV1,
    pub active_player: PlayerSeatV1,
    pub priority_player: PlayerSeatV1,
    pub initiative: Option<PlayerSeatV1>,
    pub life_totals: [i32; 2],
    pub mana_pools: [[u8; 6]; 2],
    pub hand_counts: [usize; 2],
    pub library_counts: [usize; 2],
    pub player_status: [PlayerStatusV1; 2],
    pub battlefield: [Vec<CardPublicV2>; 2],
    pub graveyards: [Vec<CardPublicV2>; 2],
    pub exile: Vec<CardPublicV2>,
    pub stack: Vec<StackItemPublicV2>,
    pub combat: CombatStatePublicV2,
    pub continuous_effects: Vec<ContinuousEffectPublicV2>,
    pub object_relations: Vec<ObjectRelationPublicV4>,
    pub exile_play_permissions: Vec<ExilePlayPermissionPublicV2>,
    pub engine_context: EngineContextV2,
    pub surface_context: HarnessSurfaceContextV2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ZoneIndependentStepV1 {
    Untap,
    Upkeep,
    Draw,
    Main1,
    BeginCombat,
    DeclareAttackers,
    DeclareBlockers,
    CombatDamage,
    EndCombat,
    Main2,
    End,
    Cleanup,
}

impl From<crate::state::Step> for ZoneIndependentStepV1 {
    fn from(value: crate::state::Step) -> Self {
        match value {
            crate::state::Step::Untap => ZoneIndependentStepV1::Untap,
            crate::state::Step::Upkeep => ZoneIndependentStepV1::Upkeep,
            crate::state::Step::Draw => ZoneIndependentStepV1::Draw,
            crate::state::Step::Main1 => ZoneIndependentStepV1::Main1,
            crate::state::Step::BeginCombat => ZoneIndependentStepV1::BeginCombat,
            crate::state::Step::DeclareAttackers => ZoneIndependentStepV1::DeclareAttackers,
            crate::state::Step::DeclareBlockers => ZoneIndependentStepV1::DeclareBlockers,
            crate::state::Step::CombatDamage => ZoneIndependentStepV1::CombatDamage,
            crate::state::Step::EndCombat => ZoneIndependentStepV1::EndCombat,
            crate::state::Step::Main2 => ZoneIndependentStepV1::Main2,
            crate::state::Step::End => ZoneIndependentStepV1::End,
            crate::state::Step::Cleanup => ZoneIndependentStepV1::Cleanup,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObservationV1 {
    pub schema_version: u32,
    pub kernel_version: String,
    pub surface_version: u32,
    pub card_db_hash: u64,
    pub acting_player: PlayerSeatV1,
    pub step_index: u64,
    pub projection: PublicObservationProjectionV1,
    pub own_hand: Vec<CardPrivateV1>,
    pub visible_projection_hash: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObservationV2 {
    pub schema_version: u32,
    pub kernel_version: String,
    pub surface_version: u32,
    pub card_db_hash: u64,
    pub acting_player: PlayerSeatV1,
    pub step_index: u64,
    pub projection: PublicObservationProjectionV2,
    pub own_hand: Vec<CardPrivateV1>,
    /// Acting-observer-only positional knowledge, indexed by library owner
    /// `[P0, P1]`. The other observer's knowledge matrix row is never
    /// serialized.
    pub known_library_cards: [Vec<KnownLibraryCardV4>; 2],
    /// Acting-observer-only revealed hand identities, indexed by hand owner.
    pub known_hand_cards: [Vec<CardPrivateV1>; 2],
    pub visible_projection_hash: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PrivateCombatSelectionV5 {
    pub attacker: Option<CardStableRefV1>,
    pub candidate_index: u32,
    pub candidate_count: u32,
    pub selected: Vec<CardStableRefV1>,
    pub current_candidate: CardStableRefV1,
    pub remaining_after_current: Vec<CardStableRefV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PolicySurfaceContextV5 {
    pub current_stage: PolicySurfaceStageV5,
    pub private_combat_selection: Option<PrivateCombatSelectionV5>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicObservationProjectionV5 {
    #[serde(flatten)]
    pub surface: PublicObservationProjectionV2,
    pub policy_surface_context: PolicySurfaceContextV5,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObservationV5 {
    pub schema_version: u32,
    pub kernel_version: String,
    pub surface_version: u32,
    pub policy_surface_version: u32,
    pub card_db_hash: u64,
    pub acting_player: PlayerSeatV1,
    pub step_index: u64,
    pub physical_decision_id: u64,
    pub substep_index: u32,
    pub substep_count: u32,
    pub projection: PublicObservationProjectionV5,
    pub own_hand: Vec<CardPrivateV1>,
    pub known_library_cards: [Vec<KnownLibraryCardV4>; 2],
    pub known_hand_cards: [Vec<CardPrivateV1>; 2],
    pub visible_projection_hash: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "action_kind", rename_all = "snake_case")]
pub enum ActionSemanticV1 {
    Pass {
        actor: PlayerSeatV1,
    },
    PlayLand {
        actor: PlayerSeatV1,
        source: CardStableRefV1,
    },
    CastSpell {
        actor: PlayerSeatV1,
        source: CardStableRefV1,
    },
    ActivateManaAbility {
        actor: PlayerSeatV1,
        source: CardStableRefV1,
        mana_choice: Option<ManaColor>,
    },
    ActivateAbility {
        actor: PlayerSeatV1,
        source: CardStableRefV1,
        ability_index: u8,
    },
    PlotSpell {
        actor: PlayerSeatV1,
        source: CardStableRefV1,
    },
    ChooseTarget {
        actor: PlayerSeatV1,
        source: CardStableRefV1,
        remaining: u8,
        target: TargetRefV1,
    },
    ChooseCostTarget {
        actor: PlayerSeatV1,
        source: CardStableRefV1,
        cost_kind: CostKind,
        remaining: u8,
        candidate: CardStableRefV1,
    },
    ChooseCastMode {
        actor: PlayerSeatV1,
        source: CardStableRefV1,
        mode: CastMode,
    },
    ChooseKicker {
        actor: PlayerSeatV1,
        source: CardStableRefV1,
        pay: bool,
    },
    ChooseSpellMode {
        actor: PlayerSeatV1,
        source: CardStableRefV1,
        mode_index: u8,
        mode_count: u8,
    },
    ChooseEffectOption {
        actor: PlayerSeatV1,
        source: CardStableRefV1,
        option_index: u16,
        option_count: u16,
    },
    ChooseEffectTarget {
        actor: PlayerSeatV1,
        source: CardStableRefV1,
        target: TargetRefV1,
        selected_count: u16,
        min_targets: u16,
        max_targets: u16,
    },
    FinishEffectSelection {
        actor: PlayerSeatV1,
        source: CardStableRefV1,
        selected_count: u16,
    },
    ChooseEffectColor {
        actor: PlayerSeatV1,
        source: CardStableRefV1,
        color: ManaColor,
    },
    ChooseEffectNumber {
        actor: PlayerSeatV1,
        source: CardStableRefV1,
        number: i32,
        minimum: i32,
        maximum: i32,
    },
    ChooseEffectBoolean {
        actor: PlayerSeatV1,
        source: CardStableRefV1,
        value: bool,
    },
    FinishTargetSelection {
        actor: PlayerSeatV1,
        source: CardStableRefV1,
        selected_count: u16,
    },
    ChooseOptionalCostUse {
        actor: PlayerSeatV1,
        use_cost: bool,
    },
    ChooseOptionalCostWhich {
        actor: PlayerSeatV1,
        choice: OptionalCostChoice,
    },
    ChooseSpellCopyPayment {
        actor: PlayerSeatV1,
        source: CardStableRefV1,
        pay: bool,
    },
    ChooseSpellCopyRetarget {
        actor: PlayerSeatV1,
        source: CardStableRefV1,
        change_target: bool,
    },
    ChooseMadnessCast {
        actor: PlayerSeatV1,
        card: CardStableRefV1,
        cast_it: bool,
    },
    Discard {
        actor: PlayerSeatV1,
        cards: Vec<CardStableRefV1>,
    },
    DeclareAttackers {
        actor: PlayerSeatV1,
        attackers: Vec<CardStableRefV1>,
    },
    DeclareBlockersForAttacker {
        actor: PlayerSeatV1,
        attacker: CardStableRefV1,
        blockers: Vec<CardStableRefV1>,
    },
    ChooseAttackerInclusion {
        actor: PlayerSeatV1,
        attacker: CardStableRefV1,
        include: bool,
    },
    ChooseBlockerInclusion {
        actor: PlayerSeatV1,
        attacker: CardStableRefV1,
        blocker: CardStableRefV1,
        include: bool,
    },
    OrderTriggers {
        actor: PlayerSeatV1,
        pending_sources: Vec<CardStableRefV1>,
        order: Vec<usize>,
    },
    Ambiguous {
        reason: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LegalActionV1 {
    pub schema_version: u32,
    pub selected_index: u32,
    /// Per-decision semantic transport identifier. This is not a global
    /// model action vocabulary and not a one-shot decision token; callers
    /// bind it with `episode_id` and `expected_step`.
    pub stable_id: String,
    pub semantic: ActionSemanticV1,
    pub display_text: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LegalActionCandidateV1 {
    pub record: LegalActionV1,
    pub surface_action: SurfaceAction,
}

/// Shared ordered action core used by both the wire-facing policy surface and
/// the in-process fast actor.  It deliberately contains no transport index,
/// stable id, display text, observation, or serialization concern.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CoreSurfaceActionCandidateV1 {
    pub(crate) semantic: ActionSemanticV1,
    pub(crate) surface_action: SurfaceAction,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LegalActionV5 {
    pub schema_version: u32,
    pub selected_index: u32,
    pub stable_id: String,
    pub semantic: ActionSemanticV1,
    pub display_text: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PolicyLegalActionCandidateV5 {
    pub record: LegalActionV5,
    pub policy_action: PolicyActionV5,
}

/// Policy-v5 action semantics and executable action in their single canonical
/// order, before any wire record or stable id is materialized.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CorePolicyActionCandidateV1 {
    pub(crate) semantic: ActionSemanticV1,
    pub(crate) policy_action: PolicyActionV5,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LibrarySetupV1 {
    pub setup_name: String,
    pub shuffle_algorithm: String,
    pub opening_hand_policy: String,
    pub env_seed: u64,
    pub deck_hashes: [u64; 2],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TerminalOutcomeV1 {
    P0Win,
    P1Win,
    Draw,
    Truncated,
    Halted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TerminalClassificationV1 {
    Natural,
    Truncated,
    Halted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TerminalSafeCodeV2 {
    NaturalGameOver,
    DecisionCap,
    FailClosed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "record_type", rename_all = "snake_case")]
#[allow(clippy::large_enum_variant)]
pub enum EpisodeRecordV1 {
    Header {
        schema_version: u32,
        diagnostic_state_hash_algorithm: String,
        stream_safety: String,
        kernel_version: String,
        surface_version: u32,
        policy_surface_version: u32,
        environment_hash_algorithm: String,
        card_db_hash: u64,
        matchup: String,
        episode_id: u64,
        game_id: String,
        env_seed: u64,
        policy_seed: u64,
        deck_identifiers: [String; 2],
        library_setup: LibrarySetupV1,
    },
    Decision {
        schema_version: u32,
        episode_id: u64,
        step: u64,
        physical_decision_id: u64,
        substep_index: u32,
        substep_count: u32,
        acting_player: PlayerSeatV1,
        observation: Box<ObservationV5>,
        observation_projection_hash: u64,
        diagnostic_state_hash: u64,
        environment_hash: u64,
        legal_actions: Vec<LegalActionV5>,
        selected_index: u32,
        selected_action_id: String,
        reward: [i32; 2],
    },
    Terminal {
        schema_version: u32,
        episode_id: u64,
        terminal_outcome: TerminalOutcomeV1,
        terminal_classification: TerminalClassificationV1,
        winner: Option<PlayerSeatV1>,
        terminal_reward: [i32; 2],
        terminal_reason: String,
        policy_step_count: u64,
        physical_decision_count: u64,
        diagnostic_state_hash: u64,
        environment_hash: u64,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "record_type", rename_all = "snake_case")]
pub enum PolicyEpisodeRecordV2 {
    Header {
        schema_version: u32,
        stream_safety: String,
        kernel_version: String,
        surface_version: u32,
        policy_surface_version: u32,
        card_db_hash: u64,
        matchup: String,
        episode_id: u64,
        episode_key: String,
        deck_identifiers: [String; 2],
    },
    Decision {
        schema_version: u32,
        episode_id: u64,
        step: u64,
        physical_decision_id: u64,
        substep_index: u32,
        substep_count: u32,
        acting_player: PlayerSeatV1,
        observation: Box<ObservationV5>,
        legal_actions: Vec<LegalActionV5>,
        selected_index: u32,
        selected_action_id: String,
        reward: [i32; 2],
    },
    Terminal {
        schema_version: u32,
        episode_id: u64,
        terminal_outcome: TerminalOutcomeV1,
        terminal_classification: TerminalClassificationV1,
        terminal_code: TerminalSafeCodeV2,
        winner: Option<PlayerSeatV1>,
        terminal_reward: [i32; 2],
        policy_step_count: u64,
        physical_decision_count: u64,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EpisodeTerminalSummaryV1 {
    pub episode_id: u64,
    pub outcome: TerminalOutcomeV1,
    pub classification: TerminalClassificationV1,
    pub winner: Option<PlayerSeatV1>,
    pub terminal_reward: [i32; 2],
    pub terminal_reason: String,
    pub policy_step_count: u64,
    pub physical_decision_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EpisodeRunV1 {
    pub audit_records: Vec<EpisodeRecordV1>,
    pub policy_records: Vec<PolicyEpisodeRecordV2>,
    pub terminal: EpisodeTerminalSummaryV1,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SeedManifestV1 {
    pub base_seed: u64,
    pub derivation: String,
    pub episode_ids: Vec<u64>,
    pub env_seeds: Vec<u64>,
    pub policy_seeds: Vec<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OutputFilesV1 {
    pub policy_episode_jsonl: String,
    pub audit_episode_jsonl: String,
    pub manifest_json: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeckManifestV1 {
    pub deck_identifiers: [String; 2],
    pub deck_hashes: [u64; 2],
    pub card_count: usize,
    pub provenance: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GitDirtyFlagV1 {
    Clean,
    Dirty,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GitMetadataV1 {
    pub commit: String,
    pub dirty: GitDirtyFlagV1,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunAggregateV1 {
    pub p0_wins: u64,
    pub p1_wins: u64,
    pub draws: u64,
    pub truncated: u64,
    pub halted: u64,
    pub total_policy_steps: u64,
    pub total_physical_decisions: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StreamManifestV1 {
    pub filename: String,
    pub policy_safe: bool,
    pub contains_hidden_state_diagnostics: bool,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VariableMetadataV1 {
    pub out_dir: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunManifestV1 {
    pub schema_version: u32,
    pub diagnostic_state_hash_algorithm: String,
    pub kernel_version: String,
    pub surface_version: u32,
    pub policy_surface_version: u32,
    pub card_db_hash: u64,
    pub matchup: String,
    pub game_count: u64,
    pub max_physical_decisions: u64,
    pub max_policy_steps: u64,
    pub cli_args: Vec<String>,
    pub seed: SeedManifestV1,
    pub output_files: OutputFilesV1,
    pub streams: Vec<StreamManifestV1>,
    pub deck: DeckManifestV1,
    pub git: GitMetadataV1,
    pub aggregate: RunAggregateV1,
    pub variable_metadata: VariableMetadataV1,
}

pub fn burn_deck_ids() -> Vec<u16> {
    runtime_deck_by_id("Burn")
        .expect("Burn is frozen in the runtime deck catalog")
        .card_ids
        .to_vec()
}

pub fn rally_deck_ids() -> Vec<u16> {
    runtime_deck_by_id("Rally")
        .expect("Rally is frozen in the runtime deck catalog")
        .card_ids
        .to_vec()
}

pub fn shuffled(ids: &[u16], rng: &mut SplitMix64) -> Vec<u16> {
    let mut v = ids.to_vec();
    for i in (1..v.len()).rev() {
        let j = (rng.next_u64() % (i as u64 + 1)) as usize;
        v.swap(i, j);
    }
    v
}

pub fn build_deck_pair_state(
    seed: u64,
    p0_deck: &[u16],
    p1_deck: &[u16],
) -> std::result::Result<GameState, DeckPreflightError> {
    // Both complete decks are admitted before RNG consumption or state
    // construction, so a bad second seat cannot partially shuffle/mutate an
    // environment that callers might otherwise retain.
    preflight_fully_supported_deck(p0_deck)?;
    preflight_fully_supported_deck(p1_deck)?;
    let mut shuffle_rng = SplitMix64::seed(seed);
    let lib0 = shuffled(p0_deck, &mut shuffle_rng);
    let lib1 = shuffled(p1_deck, &mut shuffle_rng);
    let mut state = GameState::new_from_libraries(&lib0, &lib1, card_name, seed);
    for _ in 0..7 {
        event::propose_and_commit(&mut state, ProposedEvent::draw(PlayerId::P0));
        event::propose_and_commit(&mut state, ProposedEvent::draw(PlayerId::P1));
    }
    Ok(state)
}

pub fn build_burn_mirror_state(seed: u64) -> GameState {
    let deck = burn_deck_ids();
    build_deck_pair_state(seed, &deck, &deck)
        .expect("the cataloged Burn deck is fully supported and token-free")
}

pub fn derive_env_seed(base_seed: u64, episode_id: u64) -> u64 {
    derive_seed(base_seed, episode_id, 0x4556_5f52_4c5f_7631)
}

pub fn derive_policy_seed(base_seed: u64, episode_id: u64) -> u64 {
    derive_seed(base_seed, episode_id, 0x504f_4c49_4359_7631)
}

fn derive_seed(base_seed: u64, episode_id: u64, stream: u64) -> u64 {
    let mut rng =
        SplitMix64::seed(base_seed ^ stream ^ episode_id.wrapping_mul(0x9E37_79B9_7F4A_7C15));
    rng.next_u64()
}

pub fn card_name(card_id: u16) -> String {
    CARD_DEFS[card_id as usize].name.to_string()
}

pub fn observe_v1(
    state: &GameState,
    acting_player: PlayerId,
    step_index: u64,
) -> Result<ObservationV1> {
    let projection = PublicObservationProjectionV1 {
        turn: state.turn,
        phase: state.step.into(),
        active_player: state.active_player.into(),
        priority_player: state.priority_player.into(),
        life_totals: [state.players[0].life, state.players[1].life],
        mana_pools: [state.players[0].mana_pool, state.players[1].mana_pool],
        hand_counts: [state.players[0].hand.len(), state.players[1].hand.len()],
        library_counts: [
            state.players[0].library.len(),
            state.players[1].library.len(),
        ],
        player_status: [
            player_status_v1(&state.players[0]),
            player_status_v1(&state.players[1]),
        ],
        battlefield: [
            public_cards(state, &state.players[0].battlefield)?,
            public_cards(state, &state.players[1].battlefield)?,
        ],
        graveyards: [
            public_cards(state, &state.players[0].graveyard)?,
            public_cards(state, &state.players[1].graveyard)?,
        ],
        exile: public_cards(state, &state.exile)?,
        stack: stack_public_v1(state)?,
    };
    let own_hand = state.players[acting_player.index()]
        .hand
        .iter()
        .map(|&id| private_card(state, id))
        .collect::<Result<Vec<_>>>()?;
    let mut obs = ObservationV1 {
        schema_version: OBSERVATION_SCHEMA_VERSION_V1,
        kernel_version: KERNEL_VERSION.to_string(),
        surface_version: H2_PREDICATE_VERSION,
        card_db_hash: KERNEL_CARDDB_HASH,
        acting_player: acting_player.into(),
        step_index,
        projection,
        own_hand,
        visible_projection_hash: 0,
    };
    obs.visible_projection_hash = visible_projection_hash(&obs)?;
    Ok(obs)
}

pub fn observe_v2(
    state: &GameState,
    surface: &crate::surface_v2::HarnessSurfaceV2,
    acting_player: PlayerId,
    step_index: u64,
) -> Result<ObservationV2> {
    let mut observation = build_observation_v2(state, surface, acting_player, step_index)?;
    observation.visible_projection_hash = visible_projection_hash_v2(&observation)?;
    Ok(observation)
}

fn build_observation_v2(
    state: &GameState,
    surface: &crate::surface_v2::HarnessSurfaceV2,
    acting_player: PlayerId,
    step_index: u64,
) -> Result<ObservationV2> {
    let projection = PublicObservationProjectionV2 {
        turn: state.turn,
        phase: state.step.into(),
        active_player: state.active_player.into(),
        priority_player: state.priority_player.into(),
        initiative: state.initiative.map(Into::into),
        life_totals: [state.players[0].life, state.players[1].life],
        mana_pools: [state.players[0].mana_pool, state.players[1].mana_pool],
        hand_counts: [state.players[0].hand.len(), state.players[1].hand.len()],
        library_counts: [
            state.players[0].library.len(),
            state.players[1].library.len(),
        ],
        player_status: [
            player_status_v1(&state.players[0]),
            player_status_v1(&state.players[1]),
        ],
        battlefield: [
            public_cards_v2(state, &state.players[0].battlefield)?,
            public_cards_v2(state, &state.players[1].battlefield)?,
        ],
        graveyards: [
            public_cards_v2(state, &state.players[0].graveyard)?,
            public_cards_v2(state, &state.players[1].graveyard)?,
        ],
        exile: public_cards_v2(state, &state.exile)?,
        stack: stack_public_v2(state, acting_player)?,
        combat: combat_public_v2(state)?,
        continuous_effects: continuous_effects_public_v2(state, acting_player)?,
        object_relations: object_relations_public_v4(state, acting_player)?,
        exile_play_permissions: exile_play_permissions_public_v2(state)?,
        engine_context: engine_context_v2(state, acting_player)?,
        surface_context: surface_context_v2(state, surface, acting_player)?,
    };
    let own_hand = state.players[acting_player.index()]
        .hand
        .iter()
        .map(|&id| private_card(state, id))
        .collect::<Result<Vec<_>>>()?;
    Ok(ObservationV2 {
        schema_version: OBSERVATION_SCHEMA_VERSION,
        kernel_version: KERNEL_VERSION.to_string(),
        surface_version: H2_PREDICATE_VERSION,
        card_db_hash: KERNEL_CARDDB_HASH,
        acting_player: acting_player.into(),
        step_index,
        projection,
        own_hand,
        known_library_cards: known_library_cards_v4(state, acting_player)?,
        known_hand_cards: known_hand_cards_v4(state, acting_player)?,
        visible_projection_hash: 0,
    })
}

pub fn observe_policy_v5(
    state: &GameState,
    surface: &PolicySurfaceV5,
    acting_player: PlayerId,
    step_index: u64,
    physical_decision_id: u64,
    substep_index: u32,
    substep_count: u32,
) -> Result<ObservationV5> {
    let mut observation = build_policy_observation_v5(
        state,
        surface,
        acting_player,
        step_index,
        physical_decision_id,
        substep_index,
        substep_count,
    )?;
    observation.visible_projection_hash = visible_projection_hash_v5(&observation)?;
    Ok(observation)
}

/// Builds the complete typed policy observation without serializing it solely
/// to populate the artifact-integrity hash. This is restricted to the flat
/// encoder, which classifies `visible_projection_hash` as forbidden input and
/// consumes only typed fields before the observation is dropped.
pub(crate) fn observe_policy_v5_unhashed_for_flat_policy(
    state: &GameState,
    surface: &PolicySurfaceV5,
    acting_player: PlayerId,
    step_index: u64,
    physical_decision_id: u64,
    substep_index: u32,
    substep_count: u32,
) -> Result<ObservationV5> {
    build_policy_observation_v5(
        state,
        surface,
        acting_player,
        step_index,
        physical_decision_id,
        substep_index,
        substep_count,
    )
}

fn build_policy_observation_v5(
    state: &GameState,
    surface: &PolicySurfaceV5,
    acting_player: PlayerId,
    step_index: u64,
    physical_decision_id: u64,
    substep_index: u32,
    substep_count: u32,
) -> Result<ObservationV5> {
    #[cfg(test)]
    TEST_POLICY_V5_OBSERVATIONS.with(|calls| calls.set(calls.get().saturating_add(1)));

    if substep_count == 0 || substep_index >= substep_count {
        return Err(RlContractError(format!(
            "invalid physical decision substep {substep_index}/{substep_count}"
        )));
    }
    // V5 wraps the complete V2 projection but does not expose V2's standalone
    // projection hash. Building V2 without hashing avoids serializing the same
    // large projection twice. The public artifact constructor hashes the
    // completed V5 observation once; the flat typed path explicitly skips it.
    let base = build_observation_v2(state, surface.harness_surface(), acting_player, step_index)?;
    let policy_surface_context = policy_surface_context_v5(
        state,
        surface
            .scan_context_for(acting_player)
            .map_err(RlContractError)?,
    )?;
    Ok(ObservationV5 {
        schema_version: OBSERVATION_SCHEMA_VERSION_V5,
        kernel_version: base.kernel_version,
        surface_version: base.surface_version,
        policy_surface_version: POLICY_SURFACE_VERSION,
        card_db_hash: base.card_db_hash,
        acting_player: base.acting_player,
        step_index,
        physical_decision_id,
        substep_index,
        substep_count,
        projection: PublicObservationProjectionV5 {
            surface: base.projection,
            policy_surface_context,
        },
        own_hand: base.own_hand,
        known_library_cards: base.known_library_cards,
        known_hand_cards: base.known_hand_cards,
        visible_projection_hash: 0,
    })
}

fn policy_surface_context_v5(
    state: &GameState,
    context: PolicySurfaceContextIdsV5,
) -> Result<PolicySurfaceContextV5> {
    let private_combat_selection = context
        .private_combat_selection
        .map(|private| {
            Ok::<PrivateCombatSelectionV5, RlContractError>(PrivateCombatSelectionV5 {
                attacker: private.attacker.map(|id| card_ref(state, id)).transpose()?,
                candidate_index: private.candidate_index,
                candidate_count: private.candidate_count,
                selected: private
                    .selected
                    .into_iter()
                    .map(|id| card_ref(state, id))
                    .collect::<Result<Vec<_>>>()?,
                current_candidate: card_ref(state, private.current_candidate)?,
                remaining_after_current: private
                    .remaining_after_current
                    .into_iter()
                    .map(|id| card_ref(state, id))
                    .collect::<Result<Vec<_>>>()?,
            })
        })
        .transpose()?;
    Ok(PolicySurfaceContextV5 {
        current_stage: context.current_stage,
        private_combat_selection,
    })
}

pub fn make_legal_action_v1(
    selected_index: u32,
    semantic: ActionSemanticV1,
    display_text: Option<String>,
) -> Result<LegalActionV1> {
    if let ActionSemanticV1::Ambiguous { reason } = &semantic {
        return Err(RlContractError(format!(
            "ambiguous legal action representation refused: {reason}"
        )));
    }
    let hash = stable_hash_json(&semantic)?;
    Ok(LegalActionV1 {
        schema_version: LEGAL_ACTION_SCHEMA_VERSION,
        selected_index,
        stable_id: format!("legal-action-v4:{hash:016x}"),
        semantic,
        display_text,
    })
}

pub fn make_legal_action_v5(
    selected_index: u32,
    semantic: ActionSemanticV1,
    display_text: Option<String>,
) -> Result<LegalActionV5> {
    #[cfg(test)]
    TEST_POLICY_V5_STABLE_ACTIONS.with(|calls| calls.set(calls.get().saturating_add(1)));

    validate_policy_action_semantic_v5(&semantic)?;
    let hash = stable_hash_json(&semantic)?;
    Ok(LegalActionV5 {
        schema_version: LEGAL_ACTION_SCHEMA_VERSION_V5,
        selected_index,
        stable_id: format!("legal-action-v5:{hash:016x}"),
        semantic,
        display_text,
    })
}

pub fn legal_action_candidates_v5(
    decision: &PolicyDecisionV5,
    state: &GameState,
) -> Result<Vec<PolicyLegalActionCandidateV5>> {
    let core = core_policy_action_candidates_v5(decision, state)?;
    let out = core
        .into_iter()
        .enumerate()
        .map(|(selected_index, candidate)| {
            Ok(PolicyLegalActionCandidateV5 {
                record: make_legal_action_v5(
                    u32::try_from(selected_index).map_err(|_| {
                        RlContractError("policy action index exceeds u32".to_string())
                    })?,
                    candidate.semantic,
                    None,
                )?,
                policy_action: candidate.policy_action,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    ensure_unique_policy_action_ids(&out)?;
    Ok(out)
}

pub(crate) fn core_policy_action_candidates_v5(
    decision: &PolicyDecisionV5,
    state: &GameState,
) -> Result<Vec<CorePolicyActionCandidateV1>> {
    let out: Vec<CorePolicyActionCandidateV1> = match decision {
        PolicyDecisionV5::Surface(surface_decision) => {
            core_surface_action_candidates_v1(surface_decision, state)?
                .into_iter()
                .map(|candidate| CorePolicyActionCandidateV1 {
                    semantic: candidate.semantic,
                    policy_action: PolicyActionV5::Surface(candidate.surface_action),
                })
                .collect()
        }
        PolicyDecisionV5::AttackerInclusion {
            player, attacker, ..
        } => {
            let actor = (*player).into();
            let attacker_ref = card_ref(state, *attacker)?;
            [false, true]
                .into_iter()
                .map(|include| {
                    let semantic = ActionSemanticV1::ChooseAttackerInclusion {
                        actor,
                        attacker: attacker_ref.clone(),
                        include,
                    };
                    CorePolicyActionCandidateV1 {
                        semantic,
                        policy_action: PolicyActionV5::ChooseAttackerInclusion {
                            actor: *player,
                            attacker: *attacker,
                            include,
                        },
                    }
                })
                .collect()
        }
        PolicyDecisionV5::BlockerInclusion {
            player,
            attacker,
            blocker,
            ..
        } => {
            let actor = (*player).into();
            let attacker_ref = card_ref(state, *attacker)?;
            let blocker_ref = card_ref(state, *blocker)?;
            [false, true]
                .into_iter()
                .map(|include| {
                    let semantic = ActionSemanticV1::ChooseBlockerInclusion {
                        actor,
                        attacker: attacker_ref.clone(),
                        blocker: blocker_ref.clone(),
                        include,
                    };
                    CorePolicyActionCandidateV1 {
                        semantic,
                        policy_action: PolicyActionV5::ChooseBlockerInclusion {
                            actor: *player,
                            attacker: *attacker,
                            blocker: *blocker,
                            include,
                        },
                    }
                })
                .collect()
        }
    };
    validate_core_policy_action_candidates_v5(&out)?;
    Ok(out)
}

fn validate_policy_action_semantic_v5(semantic: &ActionSemanticV1) -> Result<()> {
    if let ActionSemanticV1::Ambiguous { reason } = semantic {
        return Err(RlContractError(format!(
            "ambiguous legal action representation refused: {reason}"
        )));
    }
    if matches!(
        semantic,
        ActionSemanticV1::DeclareAttackers { .. }
            | ActionSemanticV1::DeclareBlockersForAttacker { .. }
    ) {
        return Err(RlContractError(
            "legacy aggregate combat semantic is forbidden on policy surface v5".to_string(),
        ));
    }
    Ok(())
}

pub(crate) fn validate_core_policy_action_candidates_v5(
    actions: &[CorePolicyActionCandidateV1],
) -> Result<()> {
    for action in actions {
        validate_policy_action_semantic_v5(&action.semantic)?;
    }

    // Avoid a second allocation on ordinary small policy windows while still
    // keeping pathological trigger-order windows linear rather than quadratic.
    const LINEAR_SCAN_LIMIT: usize = 32;
    if actions.len() <= LINEAR_SCAN_LIMIT {
        for (index, action) in actions.iter().enumerate() {
            if actions[..index]
                .iter()
                .any(|prior| prior.semantic == action.semantic)
            {
                return Err(RlContractError(
                    "duplicate policy action semantic within one decision".to_string(),
                ));
            }
        }
    } else {
        let mut seen = HashSet::with_capacity(actions.len());
        for action in actions {
            if !seen.insert(&action.semantic) {
                return Err(RlContractError(
                    "duplicate policy action semantic within one decision".to_string(),
                ));
            }
        }
    }
    Ok(())
}

pub fn legal_action_candidates_v1(
    decision: &SurfaceDecision,
    state: &GameState,
) -> Result<Vec<LegalActionCandidateV1>> {
    let core = core_surface_action_candidates_v1(decision, state)?;
    let out = core
        .into_iter()
        .enumerate()
        .map(|(selected_index, candidate)| {
            Ok(LegalActionCandidateV1 {
                record: make_legal_action_v1(
                    u32::try_from(selected_index).map_err(|_| {
                        RlContractError("surface action index exceeds u32".to_string())
                    })?,
                    candidate.semantic,
                    None,
                )?,
                surface_action: candidate.surface_action,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    ensure_unique_action_ids(&out)?;
    Ok(out)
}

fn core_surface_action_candidates_v1(
    decision: &SurfaceDecision,
    state: &GameState,
) -> Result<Vec<CoreSurfaceActionCandidateV1>> {
    let mut out = Vec::new();
    match decision {
        SurfaceDecision::Decision(decision) => match decision {
            Decision::CastSpellOrPass {
                player,
                castable_spells,
                mana_abilities,
                land_drops,
                activatable_abilities,
                plot_actions,
            } => {
                let actor = (*player).into();
                for &id in castable_spells {
                    push_action(
                        &mut out,
                        ActionSemanticV1::CastSpell {
                            actor,
                            source: card_ref(state, id)?,
                        },
                        SurfaceAction::Action(Action::CastSpell(id)),
                    )?;
                }
                for &id in mana_abilities {
                    push_action(
                        &mut out,
                        ActionSemanticV1::ActivateManaAbility {
                            actor,
                            source: card_ref(state, id)?,
                            mana_choice: {
                                let choices = CARD_DEFS[state.objects.get(id).card_def as usize]
                                    .produces_mana;
                                if choices.len() == 1 {
                                    Some(choices[0])
                                } else {
                                    None
                                }
                            },
                        },
                        SurfaceAction::Action(Action::ActivateManaAbility(id)),
                    )?;
                }
                for &id in land_drops {
                    push_action(
                        &mut out,
                        ActionSemanticV1::PlayLand {
                            actor,
                            source: card_ref(state, id)?,
                        },
                        SurfaceAction::Action(Action::PlayLand(id)),
                    )?;
                }
                for &(id, ability_index) in activatable_abilities {
                    push_action(
                        &mut out,
                        ActionSemanticV1::ActivateAbility {
                            actor,
                            source: card_ref(state, id)?,
                            ability_index,
                        },
                        SurfaceAction::Action(Action::ActivateAbility(id, ability_index)),
                    )?;
                }
                for &id in plot_actions {
                    push_action(
                        &mut out,
                        ActionSemanticV1::PlotSpell {
                            actor,
                            source: card_ref(state, id)?,
                        },
                        SurfaceAction::Action(Action::PlotSpell(id)),
                    )?;
                }
                push_action(
                    &mut out,
                    ActionSemanticV1::Pass { actor },
                    SurfaceAction::Action(Action::Pass),
                )?;
            }
            Decision::ChooseTargets {
                player,
                spell,
                remaining,
                legal_targets,
            } => {
                let actor = (*player).into();
                let source = card_ref(state, *spell)?;
                for &target in legal_targets {
                    push_action(
                        &mut out,
                        ActionSemanticV1::ChooseTarget {
                            actor,
                            source: source.clone(),
                            remaining: *remaining,
                            target: target_ref(state, target)?,
                        },
                        SurfaceAction::Action(Action::ChooseTarget(target)),
                    )?;
                }
            }
            Decision::ChooseCostTargets {
                player,
                source,
                cost_kind,
                remaining,
                candidates,
            } => {
                let actor = (*player).into();
                let source_ref = card_ref(state, *source)?;
                for &candidate in candidates {
                    push_action(
                        &mut out,
                        ActionSemanticV1::ChooseCostTarget {
                            actor,
                            source: source_ref.clone(),
                            cost_kind: *cost_kind,
                            remaining: *remaining,
                            candidate: card_ref(state, candidate)?,
                        },
                        SurfaceAction::Action(Action::ChooseCostTarget(candidate)),
                    )?;
                }
            }
            Decision::ChooseCastMode {
                player,
                spell,
                options,
            } => {
                let actor = (*player).into();
                let source = card_ref(state, *spell)?;
                for &mode in options {
                    push_action(
                        &mut out,
                        ActionSemanticV1::ChooseCastMode {
                            actor,
                            source: source.clone(),
                            mode,
                        },
                        SurfaceAction::Action(Action::ChooseCastMode(mode)),
                    )?;
                }
            }
            Decision::ChooseKicker { player, spell } => {
                let actor = (*player).into();
                let source = card_ref(state, *spell)?;
                for pay in [false, true] {
                    push_action(
                        &mut out,
                        ActionSemanticV1::ChooseKicker {
                            actor,
                            source: source.clone(),
                            pay,
                        },
                        SurfaceAction::Action(Action::ChooseKicker(pay)),
                    )?;
                }
            }
            Decision::ChooseSpellMode {
                player,
                spell,
                mode_count,
            } => {
                let actor = (*player).into();
                let source = card_ref(state, *spell)?;
                for mode_index in 0..*mode_count {
                    push_action(
                        &mut out,
                        ActionSemanticV1::ChooseSpellMode {
                            actor,
                            source: source.clone(),
                            mode_index,
                            mode_count: *mode_count,
                        },
                        SurfaceAction::Action(Action::ChooseSpellMode(mode_index)),
                    )?;
                }
            }
            Decision::ChooseEffectOption {
                player,
                source,
                option_count,
            } => {
                let actor = (*player).into();
                let source = card_ref(state, *source)?;
                for option_index in 0..*option_count {
                    push_action(
                        &mut out,
                        ActionSemanticV1::ChooseEffectOption {
                            actor,
                            source: source.clone(),
                            option_index,
                            option_count: *option_count,
                        },
                        SurfaceAction::Action(Action::ChooseEffectOption(option_index)),
                    )?;
                }
            }
            Decision::ChooseEffectTargets {
                player,
                source,
                selected_count,
                min_targets,
                max_targets,
                legal_targets,
                can_finish,
            } => {
                let actor = (*player).into();
                let source = card_ref(state, *source)?;
                for &target in legal_targets {
                    push_action(
                        &mut out,
                        ActionSemanticV1::ChooseEffectTarget {
                            actor,
                            source: source.clone(),
                            target: target_ref(state, target)?,
                            selected_count: *selected_count,
                            min_targets: *min_targets,
                            max_targets: *max_targets,
                        },
                        SurfaceAction::Action(Action::ChooseEffectTarget(target)),
                    )?;
                }
                if *can_finish {
                    push_action(
                        &mut out,
                        ActionSemanticV1::FinishEffectSelection {
                            actor,
                            source,
                            selected_count: *selected_count,
                        },
                        SurfaceAction::Action(Action::FinishEffectSelection),
                    )?;
                }
            }
            Decision::ChooseEffectBoolean { player, source, .. } => {
                let actor = (*player).into();
                let source = card_ref(state, *source)?;
                // Schema-v4's canonical Boolean ordering follows the
                // existing ChooseKicker convention: false/No, then true/Yes.
                // Legacy AIRL displays chooseUse as Yes then No; the semantic
                // values agree even though candidate indices are reversed.
                for value in [false, true] {
                    push_action(
                        &mut out,
                        ActionSemanticV1::ChooseEffectBoolean {
                            actor,
                            source: source.clone(),
                            value,
                        },
                        SurfaceAction::Action(Action::ChooseEffectBoolean(value)),
                    )?;
                }
            }
            Decision::ChooseOptionalCost {
                player,
                discard_payable,
                sacrifice_payable,
            } => {
                let actor = (*player).into();
                match (*discard_payable, *sacrifice_payable) {
                    (false, false) => {
                        for use_cost in [false, true] {
                            push_action(
                                &mut out,
                                ActionSemanticV1::ChooseOptionalCostUse { actor, use_cost },
                                SurfaceAction::Action(Action::ChooseOptionalCostStage(use_cost)),
                            )?;
                        }
                    }
                    (true, true) => {
                        for (choice, use_it) in [
                            (OptionalCostChoice::Discard, true),
                            (OptionalCostChoice::SacrificeLand, false),
                        ] {
                            push_action(
                                &mut out,
                                ActionSemanticV1::ChooseOptionalCostWhich { actor, choice },
                                SurfaceAction::Action(Action::ChooseOptionalCostStage(use_it)),
                            )?;
                        }
                    }
                    other => {
                        return Err(RlContractError(format!(
                            "unsupported surfaced ChooseOptionalCost flags {other:?}; expected H2 use-gate or which-gate sentinel"
                        )));
                    }
                }
            }
            Decision::ChooseSpellCopyPayment { player, spell } => {
                let actor = (*player).into();
                let source = card_ref(state, *spell)?;
                for pay in [true, false] {
                    push_action(
                        &mut out,
                        ActionSemanticV1::ChooseSpellCopyPayment {
                            actor,
                            source: source.clone(),
                            pay,
                        },
                        SurfaceAction::Action(Action::ChooseSpellCopyPayment(pay)),
                    )?;
                }
            }
            Decision::ChooseSpellCopyRetarget { player, copy } => {
                let actor = (*player).into();
                let source = card_ref(state, *copy)?;
                for change_target in [true, false] {
                    push_action(
                        &mut out,
                        ActionSemanticV1::ChooseSpellCopyRetarget {
                            actor,
                            source: source.clone(),
                            change_target,
                        },
                        SurfaceAction::Action(Action::ChooseSpellCopyRetarget(change_target)),
                    )?;
                }
            }
            Decision::ChooseMadnessCast { player, card } => {
                let actor = (*player).into();
                let card = card_ref(state, *card)?;
                for cast_it in [false, true] {
                    push_action(
                        &mut out,
                        ActionSemanticV1::ChooseMadnessCast {
                            actor,
                            card: card.clone(),
                            cast_it,
                        },
                        SurfaceAction::Action(Action::ChooseMadnessCast(cast_it)),
                    )?;
                }
            }
            Decision::Discard {
                player,
                count,
                choices,
            } => {
                if *count != 1 {
                    return Err(RlContractError(format!(
                        "surface discard contract expected count=1 after H2 reshape, got count={count}"
                    )));
                }
                let actor = (*player).into();
                for &id in choices {
                    push_action(
                        &mut out,
                        ActionSemanticV1::Discard {
                            actor,
                            cards: vec![card_ref(state, id)?],
                        },
                        SurfaceAction::Action(Action::Discard(vec![id])),
                    )?;
                }
            }
            Decision::DeclareAttackers { player, eligible } => {
                let actor = (*player).into();
                for attackers in subsets(eligible)? {
                    let attacker_refs = attackers
                        .iter()
                        .map(|&id| card_ref(state, id))
                        .collect::<Result<Vec<_>>>()?;
                    push_action(
                        &mut out,
                        ActionSemanticV1::DeclareAttackers {
                            actor,
                            attackers: attacker_refs,
                        },
                        SurfaceAction::Action(Action::DeclareAttackers(attackers)),
                    )?;
                }
            }
            Decision::DeclareBlockers { .. } => {
                return Err(RlContractError(
                    "raw DeclareBlockers is not a HarnessSurfaceV2 decision; expected DeclareBlockersForAttacker reshape".to_string(),
                ));
            }
            Decision::OrderTriggers { player, pending } => {
                let actor = (*player).into();
                let pending_sources = pending
                    .iter()
                    .map(|p| card_ref(state, p.source))
                    .collect::<Result<Vec<_>>>()?;
                for order in permutations(pending.len())? {
                    push_action(
                        &mut out,
                        ActionSemanticV1::OrderTriggers {
                            actor,
                            pending_sources: pending_sources.clone(),
                            order: order.clone(),
                        },
                        SurfaceAction::Action(Action::OrderTriggers(order)),
                    )?;
                }
            }
            Decision::GameOver { .. } | Decision::Halted { .. } => {}
        },
        SurfaceDecision::DeclareBlockersForAttacker {
            attacker,
            legal_blockers,
        } => {
            let actor = state.objects.get(*attacker).controller.opponent().into();
            let attacker_ref = card_ref(state, *attacker)?;
            for blockers in subsets(legal_blockers)? {
                let blocker_refs = blockers
                    .iter()
                    .map(|&id| card_ref(state, id))
                    .collect::<Result<Vec<_>>>()?;
                push_action(
                    &mut out,
                    ActionSemanticV1::DeclareBlockersForAttacker {
                        actor,
                        attacker: attacker_ref.clone(),
                        blockers: blocker_refs,
                    },
                    SurfaceAction::DeclareBlockersForAttacker(blockers),
                )?;
            }
        }
    }
    Ok(out)
}

pub fn acting_player_for_surface_decision(
    decision: &SurfaceDecision,
    state: &GameState,
) -> Option<PlayerId> {
    match decision {
        SurfaceDecision::Decision(decision) => match decision {
            Decision::CastSpellOrPass { player, .. }
            | Decision::ChooseTargets { player, .. }
            | Decision::ChooseCostTargets { player, .. }
            | Decision::ChooseCastMode { player, .. }
            | Decision::ChooseKicker { player, .. }
            | Decision::ChooseSpellMode { player, .. }
            | Decision::ChooseEffectOption { player, .. }
            | Decision::ChooseEffectTargets { player, .. }
            | Decision::ChooseEffectBoolean { player, .. }
            | Decision::ChooseOptionalCost { player, .. }
            | Decision::ChooseSpellCopyPayment { player, .. }
            | Decision::ChooseSpellCopyRetarget { player, .. }
            | Decision::ChooseMadnessCast { player, .. }
            | Decision::Discard { player, .. }
            | Decision::DeclareAttackers { player, .. }
            | Decision::DeclareBlockers { player, .. }
            | Decision::OrderTriggers { player, .. } => Some(*player),
            Decision::GameOver { .. } | Decision::Halted { .. } => None,
        },
        SurfaceDecision::DeclareBlockersForAttacker { attacker, .. } => {
            Some(state.objects.get(*attacker).controller.opponent())
        }
    }
}

pub fn record_burn_mirror_episode(
    episode_id: u64,
    env_seed: u64,
    policy_seed: u64,
    max_physical_decisions: u64,
) -> Result<EpisodeRunV1> {
    record_burn_mirror_episode_with_limits(
        episode_id,
        env_seed,
        policy_seed,
        max_physical_decisions,
        max_physical_decisions.saturating_mul(128).max(1),
    )
}

pub fn record_burn_mirror_episode_with_limits(
    episode_id: u64,
    env_seed: u64,
    policy_seed: u64,
    max_physical_decisions: u64,
    max_policy_steps: u64,
) -> Result<EpisodeRunV1> {
    let mut session = RlEpisodeSessionV1::reset_with_limits(
        episode_id,
        env_seed,
        max_physical_decisions,
        max_policy_steps,
    );
    let mut policy_rng = SplitMix64::seed(policy_seed);
    let deck_hash = burn_deck_hash();
    let game_id =
        format!("burn_mirror_env_{env_seed:016x}_policy_{policy_seed:016x}_game_{episode_id:06}");
    let mut audit_records = vec![EpisodeRecordV1::Header {
        schema_version: AUDIT_EPISODE_SCHEMA_VERSION,
        diagnostic_state_hash_algorithm: DIAGNOSTIC_STATE_HASH_ALGORITHM.to_string(),
        stream_safety: "privileged_audit_contains_hidden_state_diagnostics".to_string(),
        kernel_version: KERNEL_VERSION.to_string(),
        surface_version: H2_PREDICATE_VERSION,
        policy_surface_version: POLICY_SURFACE_VERSION,
        environment_hash_algorithm: crate::policy_surface_v5::POLICY_ENVIRONMENT_HASH_ALGORITHM
            .to_string(),
        card_db_hash: KERNEL_CARDDB_HASH,
        matchup: BURN_MIRROR_MATCHUP.to_string(),
        episode_id,
        game_id: game_id.clone(),
        env_seed,
        policy_seed,
        deck_identifiers: deck_identifiers(),
        library_setup: LibrarySetupV1 {
            setup_name: "burn_mirror_splitmix64_v1".to_string(),
            shuffle_algorithm: "splitmix64_fisher_yates_sequential_p0_then_p1".to_string(),
            opening_hand_policy: "seven alternating event draws starting with P0".to_string(),
            env_seed,
            deck_hashes: [deck_hash, deck_hash],
        },
    }];
    let mut policy_records = vec![PolicyEpisodeRecordV2::Header {
        schema_version: POLICY_EPISODE_SCHEMA_VERSION,
        stream_safety: "policy_safe_model_visible_v5".to_string(),
        kernel_version: KERNEL_VERSION.to_string(),
        surface_version: H2_PREDICATE_VERSION,
        policy_surface_version: POLICY_SURFACE_VERSION,
        card_db_hash: KERNEL_CARDDB_HASH,
        matchup: BURN_MIRROR_MATCHUP.to_string(),
        episode_id,
        episode_key: format!("burn_mirror_episode_{episode_id:06}"),
        deck_identifiers: deck_identifiers(),
    }];
    loop {
        match session.current_response() {
            RlSessionResponseV1::Decision(decision) => {
                let selected_index = rng_below(&mut policy_rng, decision.legal_actions.len());
                let selected_action_id = decision.legal_actions[selected_index].stable_id.clone();
                let legal_actions = decision.legal_actions.clone();
                let observation = (*decision.observation).clone();
                audit_records.push(EpisodeRecordV1::Decision {
                    schema_version: AUDIT_EPISODE_SCHEMA_VERSION,
                    episode_id,
                    step: decision.step,
                    physical_decision_id: decision.physical_decision_id,
                    substep_index: decision.substep_index,
                    substep_count: decision.substep_count,
                    acting_player: decision.acting_player,
                    observation_projection_hash: observation.visible_projection_hash,
                    diagnostic_state_hash: session.diagnostic_state_hash(),
                    environment_hash: session.privileged_environment_hash(),
                    observation: Box::new(observation.clone()),
                    legal_actions: legal_actions.clone(),
                    selected_index: selected_index as u32,
                    selected_action_id: selected_action_id.clone(),
                    reward: [0, 0],
                });
                policy_records.push(PolicyEpisodeRecordV2::Decision {
                    schema_version: POLICY_EPISODE_SCHEMA_VERSION,
                    episode_id,
                    step: decision.step,
                    physical_decision_id: decision.physical_decision_id,
                    substep_index: decision.substep_index,
                    substep_count: decision.substep_count,
                    acting_player: decision.acting_player,
                    observation: Box::new(observation),
                    legal_actions,
                    selected_index: selected_index as u32,
                    selected_action_id: selected_action_id.clone(),
                    reward: [0, 0],
                });
                session.step(
                    episode_id,
                    decision.step,
                    selected_index as u32,
                    &selected_action_id,
                )?;
            }
            RlSessionResponseV1::Terminal(terminal) => {
                let summary = terminal.into();
                push_terminal(
                    &mut audit_records,
                    &summary,
                    session.diagnostic_state_hash(),
                    session.privileged_environment_hash(),
                );
                push_policy_terminal(&mut policy_records, &summary);
                return Ok(EpisodeRunV1 {
                    audit_records,
                    policy_records,
                    terminal: summary,
                });
            }
        }
    }
}

pub fn build_rollout_records(
    games: u64,
    base_seed: u64,
    max_physical_decisions: u64,
) -> Result<(
    Vec<EpisodeRecordV1>,
    Vec<PolicyEpisodeRecordV2>,
    Vec<EpisodeTerminalSummaryV1>,
)> {
    build_rollout_records_with_limits(
        games,
        base_seed,
        max_physical_decisions,
        max_physical_decisions.saturating_mul(128).max(1),
    )
}

pub fn build_rollout_records_with_limits(
    games: u64,
    base_seed: u64,
    max_physical_decisions: u64,
    max_policy_steps: u64,
) -> Result<(
    Vec<EpisodeRecordV1>,
    Vec<PolicyEpisodeRecordV2>,
    Vec<EpisodeTerminalSummaryV1>,
)> {
    let mut audit_records = Vec::new();
    let mut policy_records = Vec::new();
    let mut summaries = Vec::new();
    for episode_id in 0..games {
        let env_seed = derive_env_seed(base_seed, episode_id);
        let policy_seed = derive_policy_seed(base_seed, episode_id);
        let run = record_burn_mirror_episode_with_limits(
            episode_id,
            env_seed,
            policy_seed,
            max_physical_decisions,
            max_policy_steps,
        )?;
        audit_records.extend(run.audit_records);
        policy_records.extend(run.policy_records);
        summaries.push(run.terminal);
    }
    Ok((audit_records, policy_records, summaries))
}

pub fn build_run_manifest(
    games: u64,
    base_seed: u64,
    max_physical_decisions: u64,
    cli_args: Vec<String>,
    out_dir: &Path,
    summaries: &[EpisodeTerminalSummaryV1],
    git: GitMetadataV1,
) -> Result<RunManifestV1> {
    build_run_manifest_with_limits(
        games,
        base_seed,
        max_physical_decisions,
        max_physical_decisions.saturating_mul(128).max(1),
        cli_args,
        out_dir,
        summaries,
        git,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn build_run_manifest_with_limits(
    games: u64,
    base_seed: u64,
    max_physical_decisions: u64,
    max_policy_steps: u64,
    cli_args: Vec<String>,
    out_dir: &Path,
    summaries: &[EpisodeTerminalSummaryV1],
    git: GitMetadataV1,
) -> Result<RunManifestV1> {
    validate_manifest_inputs(games, summaries)?;
    if summaries.iter().any(|summary| {
        summary.policy_step_count > max_policy_steps
            || summary.physical_decision_count > max_physical_decisions
    }) {
        return Err(RlContractError(
            "terminal summary exceeds the requested policy/physical decision caps".to_string(),
        ));
    }
    let deck_hash = burn_deck_hash();
    Ok(RunManifestV1 {
        schema_version: MANIFEST_SCHEMA_VERSION,
        diagnostic_state_hash_algorithm: DIAGNOSTIC_STATE_HASH_ALGORITHM.to_string(),
        kernel_version: KERNEL_VERSION.to_string(),
        surface_version: H2_PREDICATE_VERSION,
        policy_surface_version: POLICY_SURFACE_VERSION,
        card_db_hash: KERNEL_CARDDB_HASH,
        matchup: BURN_MIRROR_MATCHUP.to_string(),
        game_count: games,
        max_physical_decisions,
        max_policy_steps,
        cli_args,
        seed: SeedManifestV1 {
            base_seed,
            derivation: "env_seed=splitmix64(base_seed ^ ENV_STREAM ^ episode_id*golden_ratio); policy_seed=splitmix64(base_seed ^ POLICY_STREAM ^ episode_id*golden_ratio)".to_string(),
            episode_ids: (0..games).collect(),
            env_seeds: (0..games).map(|episode_id| derive_env_seed(base_seed, episode_id)).collect(),
            policy_seeds: (0..games).map(|episode_id| derive_policy_seed(base_seed, episode_id)).collect(),
        },
        output_files: OutputFilesV1 {
            policy_episode_jsonl: POLICY_EPISODE_JSONL_FILENAME.to_string(),
            audit_episode_jsonl: AUDIT_EPISODE_JSONL_FILENAME.to_string(),
            manifest_json: MANIFEST_FILENAME.to_string(),
        },
        streams: vec![
            StreamManifestV1 {
                filename: POLICY_EPISODE_JSONL_FILENAME.to_string(),
                policy_safe: true,
                contains_hidden_state_diagnostics: false,
                description: "model-visible v2 observations, ordered legal actions, selected transport action, rewards, and terminal records only".to_string(),
            },
            StreamManifestV1 {
                filename: AUDIT_EPISODE_JSONL_FILENAME.to_string(),
                policy_safe: false,
                contains_hidden_state_diagnostics: true,
                description: "privileged deterministic audit stream with env/policy seeds and hidden-state diagnostic hashes for parity debugging".to_string(),
            },
        ],
        deck: DeckManifestV1 {
            deck_identifiers: deck_identifiers(),
            deck_hashes: [deck_hash, deck_hash],
            card_count: 60,
            provenance: "kernel Burn mainboard copied from Deck - Mono-Red Burn.dek sideboard=false entries".to_string(),
        },
        git,
        aggregate: aggregate_summaries(summaries),
        variable_metadata: VariableMetadataV1 {
            out_dir: out_dir.display().to_string(),
        },
    })
}

pub fn git_metadata() -> GitMetadataV1 {
    let commit = std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown".to_string());
    let dirty = std::process::Command::new("git")
        .args(["status", "--porcelain"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| {
            if s.trim().is_empty() {
                GitDirtyFlagV1::Clean
            } else {
                GitDirtyFlagV1::Dirty
            }
        })
        .unwrap_or(GitDirtyFlagV1::Unknown);
    GitMetadataV1 { commit, dirty }
}

/// Parses and validates a privileged audit JSONL stream. Validation is
/// deliberately fail-closed: every episode must begin with a current-schema
/// header naming the exact diagnostic hash algorithm, and must end with its
/// matching terminal record before another header begins.
pub fn parse_audit_episode_jsonl(input: &str) -> Result<Vec<EpisodeRecordV1>> {
    let mut records = Vec::new();
    for (line_index, line) in input.lines().enumerate() {
        if line.trim().is_empty() {
            return Err(RlContractError(format!(
                "audit JSONL line {} is empty",
                line_index + 1
            )));
        }
        let raw = parse_strict_json_value(line).map_err(|_| {
            RlContractError(format!(
                "invalid audit JSONL record on line {}",
                line_index + 1
            ))
        })?;
        let record: EpisodeRecordV1 = serde_json::from_value(raw.clone()).map_err(|_| {
            RlContractError(format!(
                "audit JSONL record on line {} does not exactly match the audit schema",
                line_index + 1
            ))
        })?;
        let canonical = serde_json::to_value(&record).map_err(|_| {
            RlContractError(format!(
                "could not canonicalize audit JSONL record on line {}",
                line_index + 1
            ))
        })?;
        if raw != canonical {
            return Err(RlContractError(format!(
                "audit JSONL record on line {} does not exactly match the audit schema",
                line_index + 1
            )));
        }
        records.push(record);
    }
    validate_audit_episode_records(&records)?;
    Ok(records)
}

pub fn read_audit_episode_jsonl(path: &Path) -> Result<Vec<EpisodeRecordV1>> {
    let file = File::open(path)?;
    let mut input = String::new();
    for line in BufReader::new(file).lines() {
        input.push_str(&line?);
        input.push('\n');
    }
    parse_audit_episode_jsonl(&input)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct OpenPhysicalGroupV5 {
    physical_decision_id: u64,
    next_substep_index: u32,
    substep_count: u32,
    actor: PlayerSeatV1,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum FrozenCombatScanKindV5 {
    Attackers,
    Blockers { attacker: CardStableRefV1 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FrozenCombatScanV5 {
    kind: FrozenCombatScanKindV5,
    ordered_candidates: Vec<CardStableRefV1>,
    selected: Vec<CardStableRefV1>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct EpisodeValidationCursorV5 {
    episode_id: u64,
    kernel_version: String,
    card_db_hash: u64,
    next_step: u64,
    next_physical_decision_id: u64,
    open_group: Option<OpenPhysicalGroupV5>,
    combat_scan: Option<FrozenCombatScanV5>,
}

impl EpisodeValidationCursorV5 {
    fn new(episode_id: u64, kernel_version: String, card_db_hash: u64) -> Self {
        Self {
            episode_id,
            kernel_version,
            card_db_hash,
            next_step: 0,
            next_physical_decision_id: 0,
            open_group: None,
            combat_scan: None,
        }
    }

    fn validate_observation_provenance(
        &self,
        context: &str,
        observation: &ObservationV5,
    ) -> Result<()> {
        if observation.kernel_version != self.kernel_version
            || observation.card_db_hash != self.card_db_hash
        {
            return Err(RlContractError(format!(
                "{context} observation kernel/card-db provenance differs from its episode header"
            )));
        }
        Ok(())
    }

    fn accept_policy_payload(
        &mut self,
        context: &str,
        observation: &ObservationV5,
        legal_actions: &[LegalActionV5],
        selected_index: u32,
    ) -> Result<()> {
        let policy = &observation.projection.policy_surface_context;
        if policy.current_stage == PolicySurfaceStageV5::Surface {
            if self.combat_scan.is_some() {
                return Err(RlContractError(format!(
                    "{context} changed from a combat scan to surface stage mid-group"
                )));
            }
            return Ok(());
        }
        let private = policy.private_combat_selection.as_ref().ok_or_else(|| {
            RlContractError(format!("{context} combat scan is missing private context"))
        })?;
        let kind = match policy.current_stage {
            PolicySurfaceStageV5::AttackerInclusion => FrozenCombatScanKindV5::Attackers,
            PolicySurfaceStageV5::BlockerInclusion => FrozenCombatScanKindV5::Blockers {
                attacker: private.attacker.clone().ok_or_else(|| {
                    RlContractError(format!("{context} blocker scan lacks fixed attacker"))
                })?,
            },
            PolicySurfaceStageV5::Surface => unreachable!(),
        };
        let mut ordered = Vec::with_capacity(1 + private.remaining_after_current.len());
        ordered.push(private.current_candidate.clone());
        ordered.extend(private.remaining_after_current.iter().cloned());

        if private.candidate_index == 0 {
            if !private.selected.is_empty() || self.combat_scan.is_some() {
                return Err(RlContractError(format!(
                    "{context} combat scan must start with an empty selected prefix"
                )));
            }
            self.combat_scan = Some(FrozenCombatScanV5 {
                kind: kind.clone(),
                ordered_candidates: ordered.clone(),
                selected: Vec::new(),
            });
        }
        let frozen = self.combat_scan.as_mut().ok_or_else(|| {
            RlContractError(format!("{context} continues without a frozen combat scan"))
        })?;
        let index = private.candidate_index as usize;
        if frozen.kind != kind
            || private.selected != frozen.selected
            || frozen.ordered_candidates.get(index) != Some(&private.current_candidate)
            || frozen.ordered_candidates[(index + 1)..] != private.remaining_after_current
            || frozen.ordered_candidates.len() != private.candidate_count as usize
        {
            return Err(RlContractError(format!(
                "{context} combat scan stage, fixed attacker, candidate suffix, or selected history drifted"
            )));
        }
        let selected_action = legal_actions
            .get(selected_index as usize)
            .ok_or_else(|| RlContractError(format!("{context} selected action is out of range")))?;
        let include = match &selected_action.semantic {
            ActionSemanticV1::ChooseAttackerInclusion { include, .. }
            | ActionSemanticV1::ChooseBlockerInclusion { include, .. } => *include,
            _ => {
                return Err(RlContractError(format!(
                    "{context} combat scan selected a non-inclusion action"
                )));
            }
        };
        if include {
            frozen.selected.push(private.current_candidate.clone());
        }
        if private.candidate_index + 1 == private.candidate_count {
            self.combat_scan = None;
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn accept_decision(
        &mut self,
        context: &str,
        episode_id: u64,
        step: u64,
        physical_decision_id: u64,
        substep_index: u32,
        substep_count: u32,
        actor: PlayerSeatV1,
    ) -> Result<()> {
        if episode_id != self.episode_id || step != self.next_step {
            return Err(RlContractError(format!(
                "{context} has out-of-order episode_id/step ({episode_id}, {step})"
            )));
        }
        if substep_count == 0 || substep_index >= substep_count {
            return Err(RlContractError(format!(
                "{context} has invalid substep {substep_index}/{substep_count}"
            )));
        }
        match self.open_group {
            Some(group)
                if physical_decision_id == group.physical_decision_id
                    && substep_index == group.next_substep_index
                    && substep_count == group.substep_count
                    && actor == group.actor => {}
            Some(_) => {
                return Err(RlContractError(format!(
                    "{context} drifts actor, physical id, substep index, or frozen substep count within an open group"
                )));
            }
            None if physical_decision_id == self.next_physical_decision_id
                && substep_index == 0 => {}
            None => {
                return Err(RlContractError(format!(
                    "{context} must start physical decision {} at substep 0",
                    self.next_physical_decision_id
                )));
            }
        }
        self.next_step += 1;
        if substep_index + 1 == substep_count {
            self.next_physical_decision_id += 1;
            self.open_group = None;
        } else {
            self.open_group = Some(OpenPhysicalGroupV5 {
                physical_decision_id,
                next_substep_index: substep_index + 1,
                substep_count,
                actor,
            });
        }
        Ok(())
    }

    fn validate_terminal(
        &self,
        context: &str,
        episode_id: u64,
        policy_step_count: u64,
        physical_decision_count: u64,
    ) -> Result<()> {
        if self.open_group.is_some() {
            return Err(RlContractError(format!(
                "{context} cannot terminate during a partial physical decision"
            )));
        }
        if self.combat_scan.is_some() {
            return Err(RlContractError(format!(
                "{context} cannot terminate with incomplete combat scan context"
            )));
        }
        if episode_id != self.episode_id
            || policy_step_count != self.next_step
            || physical_decision_count != self.next_physical_decision_id
        {
            return Err(RlContractError(format!(
                "{context} terminal policy/physical counts do not match the validated stream"
            )));
        }
        Ok(())
    }
}

pub fn validate_audit_episode_records(records: &[EpisodeRecordV1]) -> Result<()> {
    if records.is_empty() {
        return Err(RlContractError("audit stream is empty".to_string()));
    }
    let mut current_episode: Option<EpisodeValidationCursorV5> = None;
    let mut seen_episode_ids = BTreeSet::new();
    for (record_index, record) in records.iter().enumerate() {
        match record {
            EpisodeRecordV1::Header {
                schema_version,
                diagnostic_state_hash_algorithm,
                environment_hash_algorithm,
                surface_version,
                policy_surface_version,
                stream_safety,
                kernel_version,
                card_db_hash,
                episode_id,
                ..
            } => {
                if current_episode.is_some() {
                    return Err(RlContractError(format!(
                        "audit header at record {record_index} begins before the previous episode terminal"
                    )));
                }
                validate_audit_schema_version(*schema_version, record_index)?;
                if diagnostic_state_hash_algorithm != DIAGNOSTIC_STATE_HASH_ALGORITHM {
                    return Err(RlContractError(format!(
                        "unsupported diagnostic_state_hash_algorithm at audit record {record_index}"
                    )));
                }
                if environment_hash_algorithm
                    != crate::policy_surface_v5::POLICY_ENVIRONMENT_HASH_ALGORITHM
                    || *surface_version != H2_PREDICATE_VERSION
                    || *policy_surface_version != POLICY_SURFACE_VERSION
                    || stream_safety != "privileged_audit_contains_hidden_state_diagnostics"
                {
                    return Err(RlContractError(format!(
                        "audit header provenance mismatch at record {record_index}"
                    )));
                }
                if !seen_episode_ids.insert(*episode_id) {
                    return Err(RlContractError(format!(
                        "duplicate audit episode_id {episode_id} at record {record_index}"
                    )));
                }
                current_episode = Some(EpisodeValidationCursorV5::new(
                    *episode_id,
                    kernel_version.clone(),
                    *card_db_hash,
                ));
            }
            EpisodeRecordV1::Decision {
                schema_version,
                episode_id,
                step,
                physical_decision_id,
                substep_index,
                substep_count,
                acting_player,
                observation,
                observation_projection_hash,
                legal_actions,
                selected_index,
                selected_action_id,
                ..
            } => {
                validate_audit_schema_version(*schema_version, record_index)?;
                let context = format!("audit record {record_index}");
                if observation.physical_decision_id != *physical_decision_id
                    || observation.substep_index != *substep_index
                    || observation.substep_count != *substep_count
                {
                    return Err(RlContractError(format!(
                        "{context} outer grouping fields do not match the observation"
                    )));
                }
                if let Some(cursor) = current_episode.as_ref() {
                    cursor.validate_observation_provenance(&context, observation)?;
                }
                current_episode
                    .as_mut()
                    .ok_or_else(|| {
                        RlContractError(format!(
                            "audit decision at record {record_index} appears before a header"
                        ))
                    })?
                    .accept_decision(
                        &context,
                        *episode_id,
                        *step,
                        *physical_decision_id,
                        *substep_index,
                        *substep_count,
                        *acting_player,
                    )?;
                validate_episode_decision_payload(
                    &context,
                    *step,
                    *acting_player,
                    observation,
                    legal_actions,
                    *selected_index,
                    selected_action_id,
                )?;
                current_episode
                    .as_mut()
                    .expect("audit decision header was validated above")
                    .accept_policy_payload(&context, observation, legal_actions, *selected_index)?;
                if *observation_projection_hash != observation.visible_projection_hash {
                    return Err(RlContractError(format!(
                        "audit observation_projection_hash mismatch at record {record_index}"
                    )));
                }
            }
            EpisodeRecordV1::Terminal {
                schema_version,
                episode_id,
                terminal_outcome,
                terminal_classification,
                winner,
                terminal_reward,
                policy_step_count,
                physical_decision_count,
                ..
            } => {
                validate_audit_schema_version(*schema_version, record_index)?;
                current_episode
                    .as_ref()
                    .ok_or_else(|| {
                        RlContractError(format!(
                            "audit terminal at record {record_index} appears before a header"
                        ))
                    })?
                    .validate_terminal(
                        &format!("audit record {record_index}"),
                        *episode_id,
                        *policy_step_count,
                        *physical_decision_count,
                    )?;
                validate_terminal_tuple(
                    *episode_id,
                    *terminal_outcome,
                    *terminal_classification,
                    *winner,
                    *terminal_reward,
                )?;
                current_episode = None;
            }
        }
    }
    if let Some(cursor) = current_episode {
        return Err(RlContractError(format!(
            "audit episode {} is missing its terminal record",
            cursor.episode_id
        )));
    }
    Ok(())
}

fn validate_audit_schema_version(schema_version: u32, record_index: usize) -> Result<()> {
    if schema_version != AUDIT_EPISODE_SCHEMA_VERSION {
        return Err(RlContractError(format!(
            "unsupported audit schema_version at record {record_index}"
        )));
    }
    Ok(())
}

pub fn parse_policy_episode_jsonl(input: &str) -> Result<Vec<PolicyEpisodeRecordV2>> {
    let mut records = Vec::new();
    for (line_index, line) in input.lines().enumerate() {
        if line.trim().is_empty() {
            return Err(RlContractError(format!(
                "policy JSONL line {} is empty",
                line_index + 1
            )));
        }
        let raw = serde_json::from_str::<StrictJsonValue>(line).map_err(|error| {
            RlContractError(format!(
                "invalid policy JSONL record on line {}: {error}",
                line_index + 1
            ))
        })?;
        let record: PolicyEpisodeRecordV2 =
            serde_json::from_value(raw.0.clone()).map_err(|error| {
                RlContractError(format!(
                    "invalid policy JSONL record on line {}: {error}",
                    line_index + 1
                ))
            })?;
        let canonical = serde_json::to_value(&record).map_err(|error| {
            RlContractError(format!(
                "could not canonicalize policy JSONL record on line {}: {error}",
                line_index + 1
            ))
        })?;
        if raw.0 != canonical {
            return Err(RlContractError(format!(
                "policy JSONL record on line {} does not exactly match the policy schema; unknown, forbidden, or omitted fields are rejected",
                line_index + 1
            )));
        }
        records.push(record);
    }
    validate_policy_episode_records(&records)?;
    Ok(records)
}

pub fn read_policy_episode_jsonl(path: &Path) -> Result<Vec<PolicyEpisodeRecordV2>> {
    let file = File::open(path)?;
    let mut input = String::new();
    for line in BufReader::new(file).lines() {
        input.push_str(&line?);
        input.push('\n');
    }
    parse_policy_episode_jsonl(&input)
}

pub fn validate_policy_episode_records(records: &[PolicyEpisodeRecordV2]) -> Result<()> {
    if records.is_empty() {
        return Err(RlContractError("policy stream is empty".to_string()));
    }
    let mut current_episode: Option<EpisodeValidationCursorV5> = None;
    let mut seen_episode_ids = BTreeSet::new();
    for (record_index, record) in records.iter().enumerate() {
        match record {
            PolicyEpisodeRecordV2::Header {
                schema_version,
                surface_version,
                policy_surface_version,
                stream_safety,
                kernel_version,
                card_db_hash,
                episode_id,
                ..
            } => {
                if current_episode.is_some() {
                    return Err(RlContractError(format!(
                        "policy header at record {record_index} begins before the previous episode terminal"
                    )));
                }
                validate_policy_schema_version(*schema_version, record_index)?;
                if *surface_version != H2_PREDICATE_VERSION
                    || *policy_surface_version != POLICY_SURFACE_VERSION
                    || stream_safety != "policy_safe_model_visible_v5"
                {
                    return Err(RlContractError(format!(
                        "policy header provenance mismatch at record {record_index}"
                    )));
                }
                if !seen_episode_ids.insert(*episode_id) {
                    return Err(RlContractError(format!(
                        "duplicate policy episode_id {episode_id} at record {record_index}"
                    )));
                }
                current_episode = Some(EpisodeValidationCursorV5::new(
                    *episode_id,
                    kernel_version.clone(),
                    *card_db_hash,
                ));
            }
            PolicyEpisodeRecordV2::Decision {
                schema_version,
                episode_id,
                step,
                physical_decision_id,
                substep_index,
                substep_count,
                acting_player,
                observation,
                legal_actions,
                selected_index,
                selected_action_id,
                ..
            } => {
                validate_policy_schema_version(*schema_version, record_index)?;
                let context = format!("policy record {record_index}");
                if observation.physical_decision_id != *physical_decision_id
                    || observation.substep_index != *substep_index
                    || observation.substep_count != *substep_count
                {
                    return Err(RlContractError(format!(
                        "{context} outer grouping fields do not match the observation"
                    )));
                }
                if let Some(cursor) = current_episode.as_ref() {
                    cursor.validate_observation_provenance(&context, observation)?;
                }
                current_episode
                    .as_mut()
                    .ok_or_else(|| {
                        RlContractError(format!(
                            "policy decision at record {record_index} appears before a header"
                        ))
                    })?
                    .accept_decision(
                        &context,
                        *episode_id,
                        *step,
                        *physical_decision_id,
                        *substep_index,
                        *substep_count,
                        *acting_player,
                    )?;
                validate_episode_decision_payload(
                    &context,
                    *step,
                    *acting_player,
                    observation,
                    legal_actions,
                    *selected_index,
                    selected_action_id,
                )?;
                current_episode
                    .as_mut()
                    .expect("policy decision header was validated above")
                    .accept_policy_payload(&context, observation, legal_actions, *selected_index)?;
            }
            PolicyEpisodeRecordV2::Terminal {
                schema_version,
                episode_id,
                terminal_outcome,
                terminal_classification,
                terminal_code,
                winner,
                terminal_reward,
                policy_step_count,
                physical_decision_count,
            } => {
                validate_policy_schema_version(*schema_version, record_index)?;
                current_episode
                    .as_ref()
                    .ok_or_else(|| {
                        RlContractError(format!(
                            "policy terminal at record {record_index} appears before a header"
                        ))
                    })?
                    .validate_terminal(
                        &format!("policy record {record_index}"),
                        *episode_id,
                        *policy_step_count,
                        *physical_decision_count,
                    )?;
                validate_terminal_tuple(
                    *episode_id,
                    *terminal_outcome,
                    *terminal_classification,
                    *winner,
                    *terminal_reward,
                )?;
                if *terminal_code != terminal_safe_code_for_classification(*terminal_classification)
                {
                    return Err(RlContractError(format!(
                        "policy terminal_code mismatch at record {record_index}"
                    )));
                }
                current_episode = None;
            }
        }
    }
    if let Some(cursor) = current_episode {
        return Err(RlContractError(format!(
            "policy episode {} is missing its terminal record",
            cursor.episode_id
        )));
    }
    Ok(())
}

fn validate_policy_schema_version(schema_version: u32, record_index: usize) -> Result<()> {
    if schema_version != POLICY_EPISODE_SCHEMA_VERSION {
        return Err(RlContractError(format!(
            "unsupported policy schema_version {schema_version} at record {record_index}; expected {POLICY_EPISODE_SCHEMA_VERSION}"
        )));
    }
    Ok(())
}

fn validate_episode_decision_payload(
    context: &str,
    step: u64,
    acting_player: PlayerSeatV1,
    observation: &ObservationV5,
    legal_actions: &[LegalActionV5],
    selected_index: u32,
    selected_action_id: &str,
) -> Result<()> {
    if observation.schema_version != OBSERVATION_SCHEMA_VERSION_V5
        || observation.step_index != step
        || observation.acting_player != acting_player
        || observation.surface_version != H2_PREDICATE_VERSION
        || observation.policy_surface_version != POLICY_SURFACE_VERSION
        || observation.substep_count == 0
        || observation.substep_index >= observation.substep_count
    {
        return Err(RlContractError(format!(
            "{context} observation metadata mismatch"
        )));
    }
    if observation.visible_projection_hash != visible_projection_hash_v5(observation)? {
        return Err(RlContractError(format!(
            "{context} observation visible_projection_hash mismatch"
        )));
    }
    let policy_context = &observation.projection.policy_surface_context;
    let private = policy_context.private_combat_selection.as_ref();
    match policy_context.current_stage {
        PolicySurfaceStageV5::Surface => {
            if private.is_some()
                || observation.substep_index != 0
                || observation.substep_count != 1
                || legal_actions.iter().any(|action| {
                    matches!(
                        action.semantic,
                        ActionSemanticV1::ChooseAttackerInclusion { .. }
                            | ActionSemanticV1::ChooseBlockerInclusion { .. }
                            | ActionSemanticV1::DeclareAttackers { .. }
                            | ActionSemanticV1::DeclareBlockersForAttacker { .. }
                    )
                })
            {
                return Err(RlContractError(format!(
                    "{context} surface-stage decision contains combat-scan payload"
                )));
            }
        }
        PolicySurfaceStageV5::AttackerInclusion | PolicySurfaceStageV5::BlockerInclusion => {
            let private = private.ok_or_else(|| {
                RlContractError(format!(
                    "{context} combat-scan stage is missing private selection context"
                ))
            })?;
            if private.candidate_index != observation.substep_index
                || private.candidate_count != observation.substep_count
                || private.remaining_after_current.len()
                    != (private.candidate_count - private.candidate_index - 1) as usize
                || private.selected.len() > private.candidate_index as usize
            {
                return Err(RlContractError(format!(
                    "{context} combat-scan context does not partition the frozen candidate sequence"
                )));
            }
            let mut refs = BTreeSet::new();
            for reference in private
                .selected
                .iter()
                .chain(std::iter::once(&private.current_candidate))
                .chain(private.remaining_after_current.iter())
            {
                let key = serde_json::to_string(reference)?;
                if !refs.insert(key) {
                    return Err(RlContractError(format!(
                        "{context} combat-scan context contains duplicate stable references"
                    )));
                }
            }
            if legal_actions.len() != 2 {
                return Err(RlContractError(format!(
                    "{context} combat scan must expose exactly two Boolean actions"
                )));
            }
            let expected_actor = acting_player;
            let valid_pair = match policy_context.current_stage {
                PolicySurfaceStageV5::AttackerInclusion => {
                    private.attacker.is_none()
                        && matches!(
                            &legal_actions[0].semantic,
                            ActionSemanticV1::ChooseAttackerInclusion {
                                actor,
                                attacker,
                                include: false,
                            } if *actor == expected_actor
                                && attacker == &private.current_candidate
                        )
                        && matches!(
                            &legal_actions[1].semantic,
                            ActionSemanticV1::ChooseAttackerInclusion {
                                actor,
                                attacker,
                                include: true,
                            } if *actor == expected_actor
                                && attacker == &private.current_candidate
                        )
                }
                PolicySurfaceStageV5::BlockerInclusion => {
                    let Some(fixed_attacker) = private.attacker.as_ref() else {
                        return Err(RlContractError(format!(
                            "{context} blocker scan is missing its fixed attacker"
                        )));
                    };
                    matches!(
                        &legal_actions[0].semantic,
                        ActionSemanticV1::ChooseBlockerInclusion {
                            actor,
                            attacker,
                            blocker,
                            include: false,
                        } if *actor == expected_actor
                            && attacker == fixed_attacker
                            && blocker == &private.current_candidate
                    ) && matches!(
                        &legal_actions[1].semantic,
                        ActionSemanticV1::ChooseBlockerInclusion {
                            actor,
                            attacker,
                            blocker,
                            include: true,
                        } if *actor == expected_actor
                            && attacker == fixed_attacker
                            && blocker == &private.current_candidate
                    )
                }
                PolicySurfaceStageV5::Surface => unreachable!(),
            };
            if !valid_pair {
                return Err(RlContractError(format!(
                    "{context} combat scan actions must be the exact [include:false, include:true] pair bound to the current candidate"
                )));
            }
        }
    }
    let mut stable_ids = BTreeSet::new();
    for (index, action) in legal_actions.iter().enumerate() {
        if action.schema_version != LEGAL_ACTION_SCHEMA_VERSION_V5
            || action.selected_index as usize != index
        {
            return Err(RlContractError(format!(
                "{context} legal action metadata mismatch at action {index}"
            )));
        }
        let expected = make_legal_action_v5(
            action.selected_index,
            action.semantic.clone(),
            action.display_text.clone(),
        )?;
        if action.stable_id != expected.stable_id || !stable_ids.insert(&action.stable_id) {
            return Err(RlContractError(format!(
                "{context} legal action stable_id mismatch or duplicate at action {index}"
            )));
        }
    }
    let selected = legal_actions.get(selected_index as usize).ok_or_else(|| {
        RlContractError(format!(
            "{context} selected_index {selected_index} is out of range"
        ))
    })?;
    if selected.stable_id != selected_action_id {
        return Err(RlContractError(format!(
            "{context} selected_action_id mismatch"
        )));
    }
    Ok(())
}

fn validate_terminal_tuple(
    episode_id: u64,
    outcome: TerminalOutcomeV1,
    classification: TerminalClassificationV1,
    winner: Option<PlayerSeatV1>,
    reward: [i32; 2],
) -> Result<()> {
    let valid = matches!(
        (outcome, classification, winner, reward),
        (
            TerminalOutcomeV1::P0Win,
            TerminalClassificationV1::Natural,
            Some(PlayerSeatV1::P0),
            [1, -1],
        ) | (
            TerminalOutcomeV1::P1Win,
            TerminalClassificationV1::Natural,
            Some(PlayerSeatV1::P1),
            [-1, 1],
        ) | (
            TerminalOutcomeV1::Draw,
            TerminalClassificationV1::Natural,
            None,
            [0, 0]
        ) | (
            TerminalOutcomeV1::Truncated,
            TerminalClassificationV1::Truncated,
            None,
            [0, 0]
        ) | (
            TerminalOutcomeV1::Halted,
            TerminalClassificationV1::Halted,
            None,
            [0, 0]
        )
    );
    if valid {
        Ok(())
    } else {
        Err(RlContractError(format!(
            "invalid terminal tuple for episode {episode_id}: outcome={outcome:?} classification={classification:?} winner={winner:?} reward={reward:?}"
        )))
    }
}

fn terminal_safe_code_for_classification(
    classification: TerminalClassificationV1,
) -> TerminalSafeCodeV2 {
    match classification {
        TerminalClassificationV1::Natural => TerminalSafeCodeV2::NaturalGameOver,
        TerminalClassificationV1::Truncated => TerminalSafeCodeV2::DecisionCap,
        TerminalClassificationV1::Halted => TerminalSafeCodeV2::FailClosed,
    }
}

pub fn parse_run_manifest_json(input: &str) -> Result<RunManifestV1> {
    let raw = parse_strict_json_value(input)
        .map_err(|_| RlContractError("invalid run manifest JSON".to_string()))?;
    let manifest: RunManifestV1 = serde_json::from_value(raw.clone()).map_err(|_| {
        RlContractError("run manifest does not exactly match the manifest schema".to_string())
    })?;
    let canonical = serde_json::to_value(&manifest)
        .map_err(|_| RlContractError("could not canonicalize run manifest".to_string()))?;
    if raw != canonical {
        return Err(RlContractError(
            "run manifest does not exactly match the manifest schema".to_string(),
        ));
    }
    validate_run_manifest(&manifest)?;
    Ok(manifest)
}

pub fn read_run_manifest(path: &Path) -> Result<RunManifestV1> {
    let mut input = String::new();
    for line in BufReader::new(File::open(path)?).lines() {
        input.push_str(&line?);
        input.push('\n');
    }
    parse_run_manifest_json(&input)
}

pub fn validate_run_manifest(manifest: &RunManifestV1) -> Result<()> {
    if manifest.schema_version != MANIFEST_SCHEMA_VERSION {
        return Err(RlContractError(
            "unsupported manifest schema_version".to_string(),
        ));
    }
    if manifest.diagnostic_state_hash_algorithm != DIAGNOSTIC_STATE_HASH_ALGORITHM {
        return Err(RlContractError(
            "unsupported diagnostic_state_hash_algorithm in manifest".to_string(),
        ));
    }
    if manifest.surface_version != H2_PREDICATE_VERSION
        || manifest.policy_surface_version != POLICY_SURFACE_VERSION
    {
        return Err(RlContractError(
            "manifest surface provenance does not match H2/policy-v5".to_string(),
        ));
    }
    Ok(())
}

pub fn validate_rollout_artifact_bundle(
    audit_records: &[EpisodeRecordV1],
    policy_records: &[PolicyEpisodeRecordV2],
    manifest: &RunManifestV1,
) -> Result<()> {
    validate_audit_episode_records(audit_records)?;
    validate_policy_episode_records(policy_records)?;
    validate_run_manifest(manifest)?;
    if audit_records.len() != policy_records.len() {
        return Err(RlContractError(format!(
            "audit/policy record-count mismatch: {} != {}",
            audit_records.len(),
            policy_records.len()
        )));
    }

    let mut episode_ids = Vec::new();
    let mut env_seeds = Vec::new();
    let mut policy_seeds = Vec::new();
    let mut aggregate = RunAggregateV1 {
        p0_wins: 0,
        p1_wins: 0,
        draws: 0,
        truncated: 0,
        halted: 0,
        total_policy_steps: 0,
        total_physical_decisions: 0,
    };
    for (record_index, (audit, policy)) in
        audit_records.iter().zip(policy_records.iter()).enumerate()
    {
        match (audit, policy) {
            (
                EpisodeRecordV1::Header {
                    stream_safety: audit_safety,
                    kernel_version: audit_kernel,
                    surface_version: audit_surface,
                    policy_surface_version: audit_policy_surface,
                    environment_hash_algorithm,
                    card_db_hash: audit_card_db,
                    matchup: audit_matchup,
                    episode_id: audit_episode,
                    game_id,
                    env_seed,
                    policy_seed,
                    deck_identifiers: audit_decks,
                    library_setup,
                    ..
                },
                PolicyEpisodeRecordV2::Header {
                    stream_safety: policy_safety,
                    kernel_version: policy_kernel,
                    surface_version: policy_surface,
                    policy_surface_version,
                    card_db_hash: policy_card_db,
                    matchup: policy_matchup,
                    episode_id: policy_episode,
                    episode_key,
                    deck_identifiers: policy_decks,
                    ..
                },
            ) => {
                if audit_safety != "privileged_audit_contains_hidden_state_diagnostics"
                    || policy_safety != "policy_safe_model_visible_v5"
                    || environment_hash_algorithm
                        != crate::policy_surface_v5::POLICY_ENVIRONMENT_HASH_ALGORITHM
                {
                    return Err(RlContractError(format!(
                        "stream_safety mismatch at paired header record {record_index}"
                    )));
                }
                if audit_kernel != policy_kernel
                    || audit_surface != policy_surface
                    || audit_policy_surface != policy_surface_version
                    || audit_card_db != policy_card_db
                    || audit_matchup != policy_matchup
                    || audit_episode != policy_episode
                    || audit_decks != policy_decks
                {
                    return Err(RlContractError(format!(
                        "audit/policy shared header mismatch at record {record_index}"
                    )));
                }
                if audit_kernel != &manifest.kernel_version
                    || audit_surface != &manifest.surface_version
                    || audit_policy_surface != &manifest.policy_surface_version
                    || audit_card_db != &manifest.card_db_hash
                    || audit_matchup != &manifest.matchup
                    || audit_decks != &manifest.deck.deck_identifiers
                    || library_setup.deck_hashes != manifest.deck.deck_hashes
                {
                    return Err(RlContractError(format!(
                        "header/manifest provenance mismatch at record {record_index}"
                    )));
                }
                if library_setup.env_seed != *env_seed
                    || game_id
                        != &format!(
                            "burn_mirror_env_{env_seed:016x}_policy_{policy_seed:016x}_game_{audit_episode:06}"
                        )
                    || episode_key != &format!("burn_mirror_episode_{policy_episode:06}")
                {
                    return Err(RlContractError(format!(
                        "episode header identity mismatch at record {record_index}"
                    )));
                }
                episode_ids.push(*audit_episode);
                env_seeds.push(*env_seed);
                policy_seeds.push(*policy_seed);
            }
            (
                EpisodeRecordV1::Decision {
                    episode_id: audit_episode,
                    step: audit_step,
                    physical_decision_id: audit_physical_id,
                    substep_index: audit_substep_index,
                    substep_count: audit_substep_count,
                    acting_player: audit_actor,
                    observation: audit_observation,
                    legal_actions: audit_actions,
                    selected_index: audit_selected_index,
                    selected_action_id: audit_selected_id,
                    reward: audit_reward,
                    ..
                },
                PolicyEpisodeRecordV2::Decision {
                    episode_id: policy_episode,
                    step: policy_step,
                    physical_decision_id: policy_physical_id,
                    substep_index: policy_substep_index,
                    substep_count: policy_substep_count,
                    acting_player: policy_actor,
                    observation: policy_observation,
                    legal_actions: policy_actions,
                    selected_index: policy_selected_index,
                    selected_action_id: policy_selected_id,
                    reward: policy_reward,
                    ..
                },
            ) => {
                if audit_episode != policy_episode
                    || audit_step != policy_step
                    || audit_physical_id != policy_physical_id
                    || audit_substep_index != policy_substep_index
                    || audit_substep_count != policy_substep_count
                    || audit_actor != policy_actor
                    || audit_observation != policy_observation
                    || audit_actions != policy_actions
                    || audit_selected_index != policy_selected_index
                    || audit_selected_id != policy_selected_id
                    || audit_reward != policy_reward
                {
                    return Err(RlContractError(format!(
                        "audit/policy decision mismatch at paired record {record_index}"
                    )));
                }
                if audit_observation.kernel_version != manifest.kernel_version
                    || audit_observation.surface_version != manifest.surface_version
                    || audit_observation.policy_surface_version != manifest.policy_surface_version
                    || audit_observation.card_db_hash != manifest.card_db_hash
                {
                    return Err(RlContractError(format!(
                        "decision/manifest provenance mismatch at paired record {record_index}"
                    )));
                }
            }
            (
                EpisodeRecordV1::Terminal {
                    episode_id: audit_episode,
                    terminal_outcome: audit_outcome,
                    terminal_classification: audit_classification,
                    winner: audit_winner,
                    terminal_reward: audit_reward,
                    policy_step_count: audit_policy_steps,
                    physical_decision_count: audit_physical_decisions,
                    ..
                },
                PolicyEpisodeRecordV2::Terminal {
                    episode_id: policy_episode,
                    terminal_outcome: policy_outcome,
                    terminal_classification: policy_classification,
                    winner: policy_winner,
                    terminal_reward: policy_reward,
                    policy_step_count: policy_steps,
                    physical_decision_count: policy_physical_decisions,
                    ..
                },
            ) => {
                if audit_episode != policy_episode
                    || audit_outcome != policy_outcome
                    || audit_classification != policy_classification
                    || audit_winner != policy_winner
                    || audit_reward != policy_reward
                    || audit_policy_steps != policy_steps
                    || audit_physical_decisions != policy_physical_decisions
                {
                    return Err(RlContractError(format!(
                        "audit/policy terminal mismatch at paired record {record_index}"
                    )));
                }
                if *audit_policy_steps > manifest.max_policy_steps
                    || *audit_physical_decisions > manifest.max_physical_decisions
                {
                    return Err(RlContractError(format!(
                        "episode {audit_episode} policy/physical counts exceed manifest caps"
                    )));
                }
                aggregate.total_policy_steps += audit_policy_steps;
                aggregate.total_physical_decisions += audit_physical_decisions;
                match audit_outcome {
                    TerminalOutcomeV1::P0Win => aggregate.p0_wins += 1,
                    TerminalOutcomeV1::P1Win => aggregate.p1_wins += 1,
                    TerminalOutcomeV1::Draw => aggregate.draws += 1,
                    TerminalOutcomeV1::Truncated => aggregate.truncated += 1,
                    TerminalOutcomeV1::Halted => aggregate.halted += 1,
                }
            }
            _ => {
                return Err(RlContractError(format!(
                    "audit/policy record-type mismatch at paired record {record_index}"
                )));
            }
        }
    }

    if manifest.game_count != episode_ids.len() as u64
        || manifest.seed.episode_ids != episode_ids
        || manifest.seed.env_seeds != env_seeds
        || manifest.seed.policy_seeds != policy_seeds
        || manifest.aggregate != aggregate
    {
        return Err(RlContractError(
            "manifest counts, episode ids, seeds, or aggregate do not match the streams"
                .to_string(),
        ));
    }
    for ((episode_id, env_seed), policy_seed) in episode_ids
        .iter()
        .zip(env_seeds.iter())
        .zip(policy_seeds.iter())
    {
        if derive_env_seed(manifest.seed.base_seed, *episode_id) != *env_seed
            || derive_policy_seed(manifest.seed.base_seed, *episode_id) != *policy_seed
        {
            return Err(RlContractError(format!(
                "manifest seed derivation mismatch for episode {episode_id}"
            )));
        }
    }
    if manifest.output_files.policy_episode_jsonl != POLICY_EPISODE_JSONL_FILENAME
        || manifest.output_files.audit_episode_jsonl != AUDIT_EPISODE_JSONL_FILENAME
        || manifest.output_files.manifest_json != MANIFEST_FILENAME
    {
        return Err(RlContractError(
            "manifest output filenames do not match the artifact contract".to_string(),
        ));
    }
    if manifest.streams.len() != 2 {
        return Err(RlContractError(
            "manifest must describe exactly the policy and audit streams".to_string(),
        ));
    }
    let policy_stream = manifest
        .streams
        .iter()
        .find(|stream| stream.filename == POLICY_EPISODE_JSONL_FILENAME)
        .ok_or_else(|| RlContractError("manifest is missing the policy stream".to_string()))?;
    let audit_stream = manifest
        .streams
        .iter()
        .find(|stream| stream.filename == AUDIT_EPISODE_JSONL_FILENAME)
        .ok_or_else(|| RlContractError("manifest is missing the audit stream".to_string()))?;
    if !policy_stream.policy_safe
        || policy_stream.contains_hidden_state_diagnostics
        || audit_stream.policy_safe
        || !audit_stream.contains_hidden_state_diagnostics
    {
        return Err(RlContractError(
            "manifest stream safety flags do not match the artifact contract".to_string(),
        ));
    }
    Ok(())
}

pub fn read_and_validate_rollout_artifacts(
    out_dir: &Path,
) -> Result<(
    Vec<EpisodeRecordV1>,
    Vec<PolicyEpisodeRecordV2>,
    RunManifestV1,
)> {
    let audit = read_audit_episode_jsonl(&out_dir.join(AUDIT_EPISODE_JSONL_FILENAME))?;
    let policy = read_policy_episode_jsonl(&out_dir.join(POLICY_EPISODE_JSONL_FILENAME))?;
    let manifest = read_run_manifest(&out_dir.join(MANIFEST_FILENAME))?;
    validate_rollout_artifact_bundle(&audit, &policy, &manifest)?;
    Ok((audit, policy, manifest))
}

pub fn write_rollout_artifacts(
    out_dir: &Path,
    audit_records: &[EpisodeRecordV1],
    policy_records: &[PolicyEpisodeRecordV2],
    manifest: &RunManifestV1,
) -> Result<()> {
    validate_rollout_artifact_bundle(audit_records, policy_records, manifest)?;
    if manifest.variable_metadata.out_dir != out_dir.display().to_string() {
        return Err(RlContractError(format!(
            "manifest out_dir {:?} does not match write destination {:?}",
            manifest.variable_metadata.out_dir,
            out_dir.display().to_string()
        )));
    }
    fs::create_dir_all(out_dir)?;
    write_jsonl_atomic(&out_dir.join(AUDIT_EPISODE_JSONL_FILENAME), audit_records)?;
    write_jsonl_atomic(&out_dir.join(POLICY_EPISODE_JSONL_FILENAME), policy_records)?;
    write_json_pretty_atomic(&out_dir.join(MANIFEST_FILENAME), manifest)?;
    Ok(())
}

pub fn validate_selected_action(
    actions: &[LegalActionCandidateV1],
    selected_index: usize,
    selected_id: &str,
) -> Result<()> {
    let Some(action) = actions.get(selected_index) else {
        return Err(RlContractError(format!(
            "selected action index {selected_index} out of range {}",
            actions.len()
        )));
    };
    if action.record.selected_index as usize != selected_index {
        return Err(RlContractError(format!(
            "selected action transport index mismatch: vector index {selected_index}, record index {}",
            action.record.selected_index
        )));
    }
    if action.record.stable_id != selected_id {
        return Err(RlContractError(format!(
            "selected action id mismatch: expected {}, got {selected_id}",
            action.record.stable_id
        )));
    }
    Ok(())
}

pub fn burn_deck_hash() -> u64 {
    runtime_deck_by_id("Burn")
        .expect("Burn is frozen in the runtime deck catalog")
        .runtime_deck_hash
}

pub fn rally_deck_hash() -> u64 {
    runtime_deck_by_id("Rally")
        .expect("Rally is frozen in the runtime deck catalog")
        .runtime_deck_hash
}

fn deck_identifiers() -> [String; 2] {
    [
        "mono_red_burn_mainboard_v1".to_string(),
        "mono_red_burn_mainboard_v1".to_string(),
    ]
}

fn validate_manifest_inputs(games: u64, summaries: &[EpisodeTerminalSummaryV1]) -> Result<()> {
    if games != summaries.len() as u64 {
        return Err(RlContractError(format!(
            "manifest game_count {games} does not match terminal summary count {}",
            summaries.len()
        )));
    }
    for summary in summaries {
        validate_terminal_summary(summary)?;
    }
    let aggregate = aggregate_summaries(summaries);
    let counted_games = aggregate.p0_wins
        + aggregate.p1_wins
        + aggregate.draws
        + aggregate.truncated
        + aggregate.halted;
    if counted_games != games {
        return Err(RlContractError(format!(
            "manifest aggregate terminal counts sum to {counted_games}, expected {games}"
        )));
    }
    Ok(())
}

fn validate_terminal_summary(summary: &EpisodeTerminalSummaryV1) -> Result<()> {
    let valid = matches!(
        (
            summary.outcome,
            summary.classification,
            summary.winner,
            summary.terminal_reward,
        ),
        (
            TerminalOutcomeV1::P0Win,
            TerminalClassificationV1::Natural,
            Some(PlayerSeatV1::P0),
            [1, -1],
        ) | (
            TerminalOutcomeV1::P1Win,
            TerminalClassificationV1::Natural,
            Some(PlayerSeatV1::P1),
            [-1, 1],
        ) | (
            TerminalOutcomeV1::Draw,
            TerminalClassificationV1::Natural,
            None,
            [0, 0]
        ) | (
            TerminalOutcomeV1::Truncated,
            TerminalClassificationV1::Truncated,
            None,
            [0, 0]
        ) | (
            TerminalOutcomeV1::Halted,
            TerminalClassificationV1::Halted,
            None,
            [0, 0]
        )
    );
    if valid {
        Ok(())
    } else {
        Err(RlContractError(format!(
            "invalid terminal tuple for episode {}: outcome={:?} classification={:?} winner={:?} reward={:?}",
            summary.episode_id,
            summary.outcome,
            summary.classification,
            summary.winner,
            summary.terminal_reward
        )))
    }
}

fn aggregate_summaries(summaries: &[EpisodeTerminalSummaryV1]) -> RunAggregateV1 {
    let mut aggregate = RunAggregateV1 {
        p0_wins: 0,
        p1_wins: 0,
        draws: 0,
        truncated: 0,
        halted: 0,
        total_policy_steps: 0,
        total_physical_decisions: 0,
    };
    for summary in summaries {
        aggregate.total_policy_steps += summary.policy_step_count;
        aggregate.total_physical_decisions += summary.physical_decision_count;
        match summary.outcome {
            TerminalOutcomeV1::P0Win => aggregate.p0_wins += 1,
            TerminalOutcomeV1::P1Win => aggregate.p1_wins += 1,
            TerminalOutcomeV1::Draw => aggregate.draws += 1,
            TerminalOutcomeV1::Truncated => aggregate.truncated += 1,
            TerminalOutcomeV1::Halted => aggregate.halted += 1,
        }
    }
    aggregate
}

fn terminal_safe_code(summary: &EpisodeTerminalSummaryV1) -> TerminalSafeCodeV2 {
    match summary.classification {
        TerminalClassificationV1::Natural => TerminalSafeCodeV2::NaturalGameOver,
        TerminalClassificationV1::Truncated => TerminalSafeCodeV2::DecisionCap,
        TerminalClassificationV1::Halted => TerminalSafeCodeV2::FailClosed,
    }
}

fn push_terminal(
    records: &mut Vec<EpisodeRecordV1>,
    summary: &EpisodeTerminalSummaryV1,
    diagnostic_state_hash: u64,
    environment_hash: u64,
) {
    records.push(EpisodeRecordV1::Terminal {
        schema_version: AUDIT_EPISODE_SCHEMA_VERSION,
        episode_id: summary.episode_id,
        terminal_outcome: summary.outcome,
        terminal_classification: summary.classification,
        winner: summary.winner,
        terminal_reward: summary.terminal_reward,
        terminal_reason: summary.terminal_reason.clone(),
        policy_step_count: summary.policy_step_count,
        physical_decision_count: summary.physical_decision_count,
        diagnostic_state_hash,
        environment_hash,
    });
}

fn push_policy_terminal(
    records: &mut Vec<PolicyEpisodeRecordV2>,
    summary: &EpisodeTerminalSummaryV1,
) {
    records.push(PolicyEpisodeRecordV2::Terminal {
        schema_version: POLICY_EPISODE_SCHEMA_VERSION,
        episode_id: summary.episode_id,
        terminal_outcome: summary.outcome,
        terminal_classification: summary.classification,
        terminal_code: terminal_safe_code(summary),
        winner: summary.winner,
        terminal_reward: summary.terminal_reward,
        policy_step_count: summary.policy_step_count,
        physical_decision_count: summary.physical_decision_count,
    });
}

fn player_status_v1(player: &crate::state::PlayerState) -> PlayerStatusV1 {
    PlayerStatusV1 {
        has_lost: player.has_lost,
        lands_played_this_turn: player.lands_played_this_turn,
        drew_from_empty: player.drew_from_empty,
        draws_this_turn: player.draws_this_turn,
        spells_cast_this_turn: player.spells_cast_this_turn,
        dungeon: player.dungeon.clone(),
    }
}

fn card_ref(state: &GameState, id: ObjectId) -> Result<CardStableRefV1> {
    let object = state
        .objects
        .try_get(id)
        .ok_or_else(|| RlContractError(format!("object id {} missing", id.0)))?;
    Ok(CardStableRefV1 {
        arena_id: id.0,
        card_db_id: object.card_def,
        owner: object.owner.into(),
        controller: object.controller.into(),
        zone: object.zone,
        zone_change_count: object.zone_change_count,
    })
}

fn detached_resolving_source_ref(
    state: &GameState,
    id: ObjectId,
    context: &'static str,
) -> Result<CardStableRefV1> {
    let object = state
        .objects
        .try_get(id)
        .ok_or_else(|| RlContractError(format!("{context} source object is missing")))?;
    if object.zone != Zone::Stack {
        return Err(RlContractError(format!(
            "{context} source must retain its public Stack zone marker"
        )));
    }
    card_ref(state, id)
}

fn object_is_live_in_zone_index(state: &GameState, id: ObjectId) -> Result<bool> {
    let object = state
        .objects
        .try_get(id)
        .ok_or_else(|| RlContractError(format!("object id {} missing", id.0)))?;
    Ok(match object.zone {
        Zone::Library => state.players[object.owner.index()].library.contains(&id),
        Zone::Hand => state.players[object.owner.index()].hand.contains(&id),
        Zone::Battlefield => state.players[object.controller.index()]
            .battlefield
            .contains(&id),
        Zone::Graveyard => state.players[object.owner.index()].graveyard.contains(&id),
        Zone::Exile => state.exile.contains(&id),
        Zone::Stack => state.stack.iter().any(|item| item.source == id),
        Zone::Command => state.command.contains(&id),
    })
}

fn visible_card_ref(
    state: &GameState,
    id: ObjectId,
    acting_player: PlayerId,
) -> Result<Option<CardStableRefV1>> {
    let object = state
        .objects
        .try_get(id)
        .ok_or_else(|| RlContractError(format!("object id {} missing", id.0)))?;
    let visible = object_is_live_in_zone_index(state, id)?
        && match object.zone {
            Zone::Battlefield | Zone::Graveyard | Zone::Exile | Zone::Stack | Zone::Command => true,
            Zone::Hand => object.owner == acting_player,
            Zone::Library => false,
        };
    if visible {
        Ok(Some(card_ref(state, id)?))
    } else {
        Ok(None)
    }
}

fn visible_card_refs(
    state: &GameState,
    ids: &[ObjectId],
    acting_player: PlayerId,
) -> Result<Vec<CardStableRefV1>> {
    let mut out = Vec::new();
    for &id in ids {
        if let Some(card) = visible_card_ref(state, id, acting_player)? {
            out.push(card);
        }
    }
    Ok(out)
}

fn paid_cost_card_refs(refs: &[PaidCostRefV4], acting_player: PlayerId) -> Vec<CardStableRefV1> {
    refs.iter()
        .copied()
        .filter(|paid| paid.visible_to(acting_player))
        .map(|paid| CardStableRefV1 {
            arena_id: paid.object.0,
            card_db_id: paid.card_def,
            owner: paid.owner.into(),
            controller: paid.controller.into(),
            zone: paid.zone,
            zone_change_count: paid.zone_change_count,
        })
        .collect()
}

fn target_ref_visible(
    state: &GameState,
    target: Target,
    acting_player: PlayerId,
) -> Result<Option<TargetRefV1>> {
    match target {
        Target::Player(player) => Ok(Some(TargetRefV1::Player {
            player: player.into(),
        })),
        Target::Object(object) => Ok(visible_card_ref(state, object, acting_player)?
            .map(|object| TargetRefV1::Object { object })),
    }
}

fn target_refs_visible(
    state: &GameState,
    targets: &[Target],
    acting_player: PlayerId,
) -> Result<Vec<TargetRefV1>> {
    let mut out = Vec::new();
    for &target in targets {
        if let Some(target) = target_ref_visible(state, target, acting_player)? {
            out.push(target);
        }
    }
    Ok(out)
}

/// A pending effect selection may name a card in an otherwise-hidden zone
/// only when this observer knows that exact incarnation. Winding Way's
/// public reveal satisfies this for both observers; future private look or
/// hand selections therefore fail closed for a non-chooser instead of
/// leaking candidate identities through the generic pending context.
fn effect_target_ref_visible(
    state: &GameState,
    target: Target,
    acting_player: PlayerId,
) -> Result<TargetRefV1> {
    let Target::Object(object_id) = target else {
        let Target::Player(player) = target else {
            unreachable!("Target has only player and object variants")
        };
        return Ok(TargetRefV1::Player {
            player: player.into(),
        });
    };
    let object = state
        .objects
        .try_get(object_id)
        .ok_or_else(|| RlContractError(format!("object id {} missing", object_id.0)))?;
    if let Some(visible) = visible_card_ref(state, object_id, acting_player)? {
        return Ok(TargetRefV1::Object { object: visible });
    }

    let known = match object.zone {
        Zone::Library => state
            .known_library_cards(acting_player, object.owner)
            .iter()
            .any(|entry| {
                entry.object == object_id
                    && entry.zone_change_count == object.zone_change_count
                    && state.players[object.owner.index()]
                        .library
                        .get(entry.position as usize)
                        .is_some_and(|&candidate| candidate == object_id)
            }),
        Zone::Hand => state
            .known_hand_cards(acting_player, object.owner)
            .iter()
            .any(|entry| {
                entry.object == object_id && entry.zone_change_count == object.zone_change_count
            }),
        _ => false,
    };
    if !known {
        return Err(RlContractError(format!(
            "pending effect target object {} is hidden from {:?}",
            object_id.0, acting_player
        )));
    }
    Ok(TargetRefV1::Object {
        object: card_ref(state, object_id)?,
    })
}

fn public_card(state: &GameState, id: ObjectId) -> Result<CardPublicV1> {
    let object = state
        .objects
        .try_get(id)
        .ok_or_else(|| RlContractError(format!("object id {} missing", id.0)))?;
    Ok(CardPublicV1 {
        stable: card_ref(state, id)?,
        card_name: card_name(object.card_def),
        tapped: object.tapped,
        summoning_sick: object.summoning_sick,
        damage: object.damage,
        counters: CountersV1 {
            plus1_plus1: object.counters.plus1_plus1,
            minus1_minus1: object.counters.minus1_minus1,
            minus0_minus1: object.counters.minus0_minus1,
            stun: object.counters.stun,
            lore: object.counters.lore,
        },
        attachments: object.attachments.iter().map(|id| id.0).collect(),
        plotted_turn: object.plotted_turn,
    })
}

fn public_card_v2(state: &GameState, id: ObjectId) -> Result<CardPublicV2> {
    let object = state
        .objects
        .try_get(id)
        .ok_or_else(|| RlContractError(format!("object id {} missing", id.0)))?;
    Ok(CardPublicV2 {
        stable: card_ref(state, id)?,
        card_name: card_name(object.card_def),
        tapped: object.tapped,
        summoning_sick: object.summoning_sick,
        damage: object.damage,
        counters: CountersV1 {
            plus1_plus1: object.counters.plus1_plus1,
            minus1_minus1: object.counters.minus1_minus1,
            minus0_minus1: object.counters.minus0_minus1,
            stun: object.counters.stun,
            lore: object.counters.lore,
        },
        attachments: object.attachments.iter().map(|id| id.0).collect(),
        plotted_turn: object.plotted_turn,
        is_token: object.v4.is_token,
        face_index: object.v4.face_index,
        chosen_color: object.v4.chosen_color,
        entered_battlefield_turn: object.v4.entered_battlefield_turn,
        ability_uses_this_turn: object
            .v4
            .ability_uses_this_turn
            .iter()
            .map(|entry| AbilityUsePublicV4 {
                ability_kind: entry.ability_kind,
                ability_index: entry.ability_index,
                uses: entry.uses,
            })
            .collect(),
        skip_next_untap: object.v4.skip_next_untap,
        goaded_by: object
            .v4
            .goaded_by
            .iter()
            .map(|entry| GoadPublicV4 {
                player: entry.player.into(),
                expires_at_turn: entry.expires_at_turn,
            })
            .collect(),
        characteristics: card_characteristics_v2(state, id),
    })
}

fn public_cards(state: &GameState, ids: &[ObjectId]) -> Result<Vec<CardPublicV1>> {
    ids.iter().map(|&id| public_card(state, id)).collect()
}

fn public_cards_v2(state: &GameState, ids: &[ObjectId]) -> Result<Vec<CardPublicV2>> {
    ids.iter().map(|&id| public_card_v2(state, id)).collect()
}

fn private_card(state: &GameState, id: ObjectId) -> Result<CardPrivateV1> {
    let object = state
        .objects
        .try_get(id)
        .ok_or_else(|| RlContractError(format!("object id {} missing", id.0)))?;
    Ok(CardPrivateV1 {
        stable: card_ref(state, id)?,
        card_name: card_name(object.card_def),
    })
}

fn known_library_cards_v4(
    state: &GameState,
    observer: PlayerId,
) -> Result<[Vec<KnownLibraryCardV4>; 2]> {
    let mut result: [Vec<KnownLibraryCardV4>; 2] = std::array::from_fn(|_| Vec::new());
    for owner in [PlayerId::P0, PlayerId::P1] {
        for entry in state.known_library_cards(observer, owner) {
            let library = &state.players[owner.index()].library;
            let Some(&object) = library.get(entry.position as usize) else {
                return Err(RlContractError(format!(
                    "known library position {} is outside {:?}'s library",
                    entry.position, owner
                )));
            };
            if object != entry.object
                || state.objects.get(object).zone_change_count != entry.zone_change_count
            {
                return Err(RlContractError(
                    "known library entry does not match the live object incarnation".to_string(),
                ));
            }
            result[owner.index()].push(KnownLibraryCardV4 {
                position: entry.position,
                card: private_card(state, object)?,
            });
        }
    }
    Ok(result)
}

fn known_hand_cards_v4(state: &GameState, observer: PlayerId) -> Result<[Vec<CardPrivateV1>; 2]> {
    let mut result: [Vec<CardPrivateV1>; 2] = std::array::from_fn(|_| Vec::new());
    for owner in [PlayerId::P0, PlayerId::P1] {
        if owner == observer {
            continue;
        }
        for entry in state.known_hand_cards(observer, owner) {
            let object = state.objects.try_get(entry.object).ok_or_else(|| {
                RlContractError(format!("known hand object {} is missing", entry.object))
            })?;
            if object.owner != owner
                || object.zone != Zone::Hand
                || object.zone_change_count != entry.zone_change_count
                || !state.players[owner.index()].hand.contains(&entry.object)
            {
                return Err(RlContractError(
                    "known hand entry does not match the live object incarnation".to_string(),
                ));
            }
            result[owner.index()].push(private_card(state, entry.object)?);
        }
    }
    Ok(result)
}

fn card_characteristics_v2(state: &GameState, id: ObjectId) -> CardCharacteristicsV2 {
    let object = state.objects.get(id);
    let def = &CARD_DEFS[object.card_def as usize];
    let base_power = def.power.map(i32::from);
    let base_toughness = def.toughness.map(i32::from);
    let has_pt = base_power.is_some() || base_toughness.is_some();
    CardCharacteristicsV2 {
        type_flags: CardTypeFlagsV2 {
            land: def.has_type(CardType::Land),
            creature: def.has_type(CardType::Creature),
            instant: def.has_type(CardType::Instant),
            sorcery: def.has_type(CardType::Sorcery),
            artifact: def.has_type(CardType::Artifact),
            enchantment: def.has_type(CardType::Enchantment),
        },
        base_power,
        base_toughness,
        effective_power: has_pt.then(|| engine::effective_power(state, id)),
        effective_toughness: has_pt.then(|| engine::effective_toughness(state, id)),
        effective_color_mask: object.v4.effective_color_mask,
        effective_subtype_ids: object.v4.effective_subtype_ids.clone(),
        effective_keywords: KeywordFlagsV2 {
            flying: engine::has_effective_keyword(state, id, Keywords::FLYING),
            reach: engine::has_effective_keyword(state, id, Keywords::REACH),
            haste: engine::has_effective_keyword(state, id, Keywords::HASTE),
            vigilance: engine::has_effective_keyword(state, id, Keywords::VIGILANCE),
            trample: engine::has_effective_keyword(state, id, Keywords::TRAMPLE),
            first_strike: engine::has_effective_keyword(state, id, Keywords::FIRST_STRIKE),
            double_strike: engine::has_effective_keyword(state, id, Keywords::DOUBLE_STRIKE),
            deathtouch: engine::has_effective_keyword(state, id, Keywords::DEATHTOUCH),
            menace: engine::has_effective_keyword(state, id, Keywords::MENACE),
            defender: engine::has_effective_keyword(state, id, Keywords::DEFENDER),
            lifelink: engine::has_effective_keyword(state, id, Keywords::LIFELINK),
            hexproof: engine::has_effective_keyword(state, id, Keywords::HEXPROOF),
            indestructible: engine::has_effective_keyword(state, id, Keywords::INDESTRUCTIBLE),
            protection_from_monocolored: engine::has_effective_keyword(
                state,
                id,
                Keywords::PROTECTION_FROM_MONOCOLORED,
            ),
            ward_generic: object.v4.ward_generic,
            minimum_blockers: object.v4.minimum_blockers_override.unwrap_or_else(|| {
                if !def.has_type(CardType::Creature) {
                    0
                } else if engine::has_effective_keyword(state, id, Keywords::MENACE) {
                    2
                } else {
                    1
                }
            }),
            landwalk_mask: object.v4.landwalk_mask,
        },
    }
}

fn combat_public_v2(state: &GameState) -> Result<CombatStatePublicV2> {
    let live_attackers = state
        .engine
        .combat
        .attackers
        .iter()
        .copied()
        .filter_map(|id| match object_is_live_in_zone_index(state, id) {
            Ok(true) => Some(Ok(id)),
            Ok(false) => None,
            Err(err) => Some(Err(err)),
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(CombatStatePublicV2 {
        attackers_declared: state.engine.combat.attackers_declared,
        blockers_declared: state.engine.combat.blockers_declared,
        ordered_attackers: live_attackers
            .iter()
            .map(|&id| card_ref(state, id))
            .collect::<Result<Vec<_>>>()?,
        attacker_to_ordered_blockers: state
            .engine
            .combat
            .blocked_by
            .iter()
            .filter(|(attacker, _)| live_attackers.contains(attacker))
            .map(|(attacker, blockers)| {
                Ok((
                    card_ref(state, *attacker)?,
                    blockers
                        .iter()
                        .copied()
                        .filter_map(|id| match object_is_live_in_zone_index(state, id) {
                            Ok(true) => Some(card_ref(state, id)),
                            Ok(false) => None,
                            Err(err) => Some(Err(err)),
                        })
                        .collect::<Result<Vec<_>>>()?,
                ))
            })
            .collect::<Result<Vec<_>>>()?,
    })
}

fn object_relations_public_v4(
    state: &GameState,
    acting_player: PlayerId,
) -> Result<Vec<ObjectRelationPublicV4>> {
    let mut out = Vec::new();
    for (id, object) in state.objects.iter() {
        let Some(source) = visible_card_ref(state, id, acting_player)? else {
            continue;
        };
        if let Some(link) = object.v4.attached_to {
            let target = state.objects.try_get(link.object).ok_or_else(|| {
                RlContractError("attached_to relation points at a missing object".to_string())
            })?;
            if target.zone_change_count != link.zone_change_count {
                return Err(RlContractError(
                    "attached_to relation points at a stale object incarnation".to_string(),
                ));
            }
            if let Some(attached_to) = visible_card_ref(state, link.object, acting_player)? {
                out.push(ObjectRelationPublicV4::AttachedTo {
                    object: source.clone(),
                    attached_to,
                });
            }
        }
        if let Some(link) = object.v4.exiled_by {
            let target = state.objects.try_get(link.object).ok_or_else(|| {
                RlContractError("exiled_by relation points at a missing object".to_string())
            })?;
            if target.zone_change_count != link.zone_change_count {
                return Err(RlContractError(
                    "exiled_by relation points at a stale object incarnation".to_string(),
                ));
            }
            if let Some(exiled_by) = visible_card_ref(state, link.object, acting_player)? {
                out.push(ObjectRelationPublicV4::ExiledBy {
                    object: source,
                    exiled_by,
                });
            }
        }
    }
    Ok(out)
}

fn continuous_effects_public_v2(
    state: &GameState,
    acting_player: PlayerId,
) -> Result<Vec<ContinuousEffectPublicV2>> {
    let mut out = Vec::new();
    for effect in &state.engine.until_end_of_turn {
        match effect {
            UntilEndOfTurnEffect::SyntheticMarker(_) => {}
            UntilEndOfTurnEffect::ResolvedSetEffect {
                object_ids,
                layer,
                timestamp,
                duration,
                power,
                toughness,
                grant_haste,
            } => {
                let duration = match duration {
                    engine::EffectDuration::EndOfTurn => EffectDurationV2::EndOfTurn,
                };
                let affected_objects = visible_card_refs(state, object_ids, acting_player)?;
                if affected_objects.is_empty() {
                    continue;
                }
                out.push(ContinuousEffectPublicV2 {
                    source: None,
                    controller: None,
                    affected_objects,
                    affected_players: Vec::new(),
                    global: false,
                    layers: layer.0,
                    timestamp: *timestamp,
                    duration,
                    power_delta: *power,
                    toughness_delta: *toughness,
                    grants_haste: *grant_haste,
                    set_power: None,
                    set_toughness: None,
                    add_color_mask: 0,
                    remove_color_mask: 0,
                    add_subtype_ids: Vec::new(),
                    remove_subtype_ids: Vec::new(),
                    add_keyword_mask: if *grant_haste { Keywords::HASTE.0 } else { 0 },
                    remove_keyword_mask: 0,
                    ward_generic_delta: 0,
                    minimum_blockers: None,
                    add_landwalk_mask: 0,
                    remove_landwalk_mask: 0,
                    prevent_damage_from_color_mask: 0,
                    damage_cannot_be_prevented: false,
                });
            }
        }
    }
    Ok(out)
}

fn exile_play_permissions_public_v2(state: &GameState) -> Result<Vec<ExilePlayPermissionPublicV2>> {
    let mut out = Vec::new();
    for perm in &state.engine.exile_play_permissions {
        if engine::active_permission_for(perm.holder, perm.object, state).is_none() {
            continue;
        }
        out.push(ExilePlayPermissionPublicV2 {
            object: card_ref(state, perm.object)?,
            holder: perm.holder.into(),
            play_or_cast: match perm.play_or_cast {
                PlayOrCast::Play => PlayOrCastV2::Play,
                PlayOrCast::Cast => PlayOrCastV2::Cast,
            },
            zone_change_generation: perm.zone_change_generation,
            expiry: match perm.expiry {
                PlayPermissionExpiry::EndOfTurn => PlayPermissionExpiryV2::EndOfTurn,
                PlayPermissionExpiry::UntilHoldersNextTurn {
                    holder_turn_started,
                } => PlayPermissionExpiryV2::UntilHoldersNextTurn {
                    holder_turn_started,
                },
            },
        });
    }
    Ok(out)
}

fn engine_context_v2(state: &GameState, acting_player: PlayerId) -> Result<EngineContextV2> {
    let current_stage = if state.engine.halted.is_some() {
        EngineDecisionStageV2::Halted
    } else if state.engine.pending_cast.is_some() {
        EngineDecisionStageV2::PendingCast
    } else if state.engine.pending_activation.is_some() {
        EngineDecisionStageV2::PendingActivation
    } else if state.engine.pending_discard.is_some() {
        EngineDecisionStageV2::PendingDiscard
    } else if state.engine.pending_optional_cost.is_some() {
        EngineDecisionStageV2::PendingOptionalCost
    } else if state.engine.pending_optional_cost_sacrifice.is_some() {
        EngineDecisionStageV2::PendingOptionalCostSacrifice
    } else if state.engine.pending_spell_copy.is_some() {
        EngineDecisionStageV2::PendingSpellCopy
    } else if state.engine.pending_effect.is_some() {
        EngineDecisionStageV2::PendingEffect
    } else if !state.engine.pending_triggers.is_empty() {
        EngineDecisionStageV2::PendingTriggers
    } else {
        EngineDecisionStageV2::Priority
    };

    let mana_activity_since_priority_boundary =
        state.engine.mana_ability_activations != state.engine.mana_ability_count_at_round_open;

    Ok(EngineContextV2 {
        priority_passes: state.engine.priority_passes,
        stack_nonempty: !state.stack.is_empty(),
        stack_activity_since_priority_boundary: state.stack.len()
            != state.engine.stack_len_at_round_open,
        mana_activity_since_priority_boundary,
        last_mana_ability_activator_since_priority_boundary:
            if mana_activity_since_priority_boundary {
                state.engine.last_mana_ability_activator.map(Into::into)
            } else {
                None
            },
        current_stage,
        pending_cast: state
            .engine
            .pending_cast
            .as_ref()
            .map(|p| pending_cast_semantic_v2(state, acting_player, p))
            .transpose()?,
        pending_activation: state
            .engine
            .pending_activation
            .as_ref()
            .map(|p| pending_activation_semantic_v2(state, acting_player, p))
            .transpose()?,
        pending_discard: state
            .engine
            .pending_discard
            .as_ref()
            .map(|p| pending_discard_semantic_v2(state, acting_player, p))
            .transpose()?,
        pending_optional_cost: state
            .engine
            .pending_optional_cost
            .as_ref()
            .map(|p| pending_optional_cost_semantic_v2(state, acting_player, p))
            .transpose()?,
        pending_optional_cost_sacrifice: state
            .engine
            .pending_optional_cost_sacrifice
            .as_ref()
            .map(|p| pending_optional_cost_sacrifice_semantic_v2(state, acting_player, p))
            .transpose()?,
        pending_spell_copy: state
            .engine
            .pending_spell_copy
            .as_ref()
            .map(|p| pending_spell_copy_semantic_v2(state, acting_player, p))
            .transpose()?,
        pending_effect: state
            .engine
            .pending_effect
            .as_ref()
            .map(|pending| pending_effect_semantic_v4(state, acting_player, pending))
            .transpose()?,
        pending_triggers: state
            .engine
            .pending_triggers
            .iter()
            .map(|p| {
                Ok(PendingTriggerSemanticV2 {
                    source: visible_card_ref(state, p.source, acting_player)?,
                    controller: p.controller.into(),
                    trigger_kind: if p.is_madness_offer {
                        PendingTriggerKindV2::MadnessOffer
                    } else {
                        PendingTriggerKindV2::TriggeredAbility
                    },
                    kicked: p.kicked,
                })
            })
            .collect::<Result<Vec<_>>>()?,
    })
}

fn pending_effect_semantic_v4(
    state: &GameState,
    acting_player: PlayerId,
    pending: &crate::effect::EffectContinuation,
) -> Result<PendingEffectSemanticV4> {
    crate::effect::validate_pending_effect_choice(state).map_err(RlContractError)?;
    let choice = pending
        .choice
        .as_ref()
        .map(|choice| -> Result<PendingEffectChoiceSemanticV4> {
            match choice {
                crate::effect::PendingEffectChoice::ChooseOption {
                    player,
                    path,
                    options,
                    ..
                } => Ok(PendingEffectChoiceSemanticV4::Options {
                    player: (*player).into(),
                    structural_path: path.clone(),
                    option_count: options.len().try_into().map_err(|_| {
                        RlContractError("effect option count exceeds u16".to_string())
                    })?,
                }),
                crate::effect::PendingEffectChoice::SelectTargets {
                    player,
                    path,
                    selected,
                    legal,
                    min_targets,
                    max_targets,
                    ordered,
                    purpose,
                } => {
                    let chooser_private = matches!(
                        purpose,
                        crate::effect::EffectTargetSelectionPurpose::OrderMilledIntoGraveyard
                            | crate::effect::EffectTargetSelectionPurpose::OrderLookedLibraryTop {
                                ..
                            }
                            | crate::effect::EffectTargetSelectionPurpose::PutHandCardOnLibraryTop {
                                ..
                            }
                            | crate::effect::EffectTargetSelectionPurpose::ScryLibrary { .. }
                            | crate::effect::EffectTargetSelectionPurpose::SearchLibraryToHand {
                                ..
                            }
                    ) && acting_player != *player;
                    let search_for_chooser = matches!(
                        purpose,
                        crate::effect::EffectTargetSelectionPurpose::SearchLibraryToHand { .. }
                    ) && acting_player == *player;
                    let redact_search_shape = matches!(
                        purpose,
                        crate::effect::EffectTargetSelectionPurpose::SearchLibraryToHand { .. }
                    ) && acting_player != *player;
                    let visible_targets = |candidates: &[crate::effect::EffectTargetCandidate]| {
                        if chooser_private {
                            return Ok(Vec::new());
                        }
                        candidates
                            .iter()
                            .map(|candidate| {
                                if search_for_chooser {
                                    // A library search grants its chooser
                                    // temporary access to every matching
                                    // physical object without installing
                                    // persistent library-order knowledge.
                                    // This direct projection is therefore
                                    // confined to the chooser and prompt.
                                    target_ref(state, candidate.target)
                                } else {
                                    effect_target_ref_visible(
                                        state,
                                        candidate.target,
                                        acting_player,
                                    )
                                }
                            })
                            .collect::<Result<Vec<_>>>()
                    };
                    Ok(PendingEffectChoiceSemanticV4::Targets {
                        player: (*player).into(),
                        structural_path: path.clone(),
                        // Cards physically in a private library remain
                        // chooser-private. Fixed-cardinality reorder/scry
                        // prompts retain their public shape; a whole-library
                        // search also redacts its match count, leaving only
                        // the typed envelope for a non-chooser.
                        selected_targets: visible_targets(selected)?,
                        legal_targets: visible_targets(legal)?,
                        min_targets: if redact_search_shape { 0 } else { *min_targets },
                        max_targets: if redact_search_shape { 0 } else { *max_targets },
                        can_finish: if redact_search_shape {
                            true
                        } else {
                            selected.len() >= usize::from(*min_targets)
                        },
                        ordered: *ordered,
                        purpose: match purpose {
                            crate::effect::EffectTargetSelectionPurpose::OrderIntoGraveyard {
                                ..
                            }
                            | crate::effect::EffectTargetSelectionPurpose::OrderMilledIntoGraveyard => {
                                TargetSelectionPurposeV4::CardSelection
                            }
                            crate::effect::EffectTargetSelectionPurpose::OrderLookedLibraryTop {
                                ..
                            }
                            | crate::effect::EffectTargetSelectionPurpose::PutHandCardOnLibraryTop {
                                ..
                            } => TargetSelectionPurposeV4::LibraryOrder,
                            crate::effect::EffectTargetSelectionPurpose::ScryLibrary {
                                stage,
                                ..
                            } => match stage {
                                crate::effect::ScrySelectionStage::ChooseBottomSubset => {
                                    TargetSelectionPurposeV4::CardSelection
                                }
                                crate::effect::ScrySelectionStage::OrderBottom { .. }
                                | crate::effect::ScrySelectionStage::OrderRetainedTop { .. } => {
                                    TargetSelectionPurposeV4::LibraryOrder
                                }
                            },
                            crate::effect::EffectTargetSelectionPurpose::SearchLibraryToHand {
                                ..
                            } => TargetSelectionPurposeV4::SearchResult,
                        },
                    })
                }
                crate::effect::PendingEffectChoice::ChooseBoolean {
                    player,
                    path,
                    default,
                    purpose,
                } => Ok(PendingEffectChoiceSemanticV4::Boolean {
                    player: (*player).into(),
                    structural_path: path.clone(),
                    default: *default,
                    purpose: match purpose {
                        crate::effect::EffectBooleanChoicePurpose::ShuffleLibrary { .. } => {
                            BooleanChoicePurposeV4::Shuffle
                        }
                    },
                }),
            }
        })
        .transpose()?;
    Ok(PendingEffectSemanticV4 {
        // A resolving stack item's source was already public as part of that
        // stack item. Reuse the same unconditional public source reference even
        // if the underlying card has since moved to a normally hidden zone.
        source: Some(card_ref(state, pending.resolving_item.source)?),
        controller: pending.resolving_item.controller.into(),
        choice,
    })
}

fn pending_cast_semantic_v2(
    state: &GameState,
    acting_player: PlayerId,
    p: &engine::PendingCast,
) -> Result<PendingCastSemanticV2> {
    engine::validate_pending_cast(state, p)
        .map_err(|error| RlContractError(format!("invalid pending cast: {error}")))?;
    Ok(PendingCastSemanticV2 {
        source: visible_card_ref(state, p.spell, acting_player)?,
        controller: p.controller.into(),
        chosen_targets: target_refs_visible(state, &p.targets_chosen, acting_player)?,
        is_flashback: p.is_flashback,
        cast_mode: p.cast_mode,
        additional_cost_discarded: match &p.additional_cost_discarded {
            Some(ids) => Some(visible_card_refs(state, ids, acting_player)?),
            None => None,
        },
        mode_chosen: p.mode_chosen,
        origin_zone: p.origin_zone,
        sacrifice_chosen: visible_card_refs(state, &p.sacrifice_chosen, acting_player)?,
        kicked: p.kicked,
    })
}

fn pending_activation_semantic_v2(
    state: &GameState,
    acting_player: PlayerId,
    p: &engine::PendingActivation,
) -> Result<PendingActivationSemanticV2> {
    engine::validate_pending_activation(state, p)
        .map_err(|error| RlContractError(format!("invalid pending activation: {error}")))?;
    Ok(PendingActivationSemanticV2 {
        source: visible_card_ref(state, p.source, acting_player)?,
        controller: p.controller.into(),
        ability_index: p.ability_index,
        chosen_targets: target_refs_visible(state, &p.targets_chosen, acting_player)?,
        cost_discard_paid: match &p.cost_discard_paid {
            Some(ids) => Some(visible_card_refs(state, ids, acting_player)?),
            None => None,
        },
    })
}

fn pending_discard_semantic_v2(
    state: &GameState,
    acting_player: PlayerId,
    p: &engine::PendingDiscard,
) -> Result<PendingDiscardSemanticV2> {
    engine::validate_pending_discard_binding(state, p)
        .map_err(|(_, error)| RlContractError(format!("invalid pending discard: {error}")))?;
    let (resume_stage, resume_source) = match &p.resume {
        engine::DiscardResume::None => (DiscardResumeSemanticV2::None, None),
        engine::DiscardResume::FinishCast { .. } => (DiscardResumeSemanticV2::FinishCast, None),
        engine::DiscardResume::FinishActivation { .. } => {
            (DiscardResumeSemanticV2::FinishActivation, None)
        }
        // These sources were public stack objects before resolution popped
        // them. The choice remains inside that uninterrupted resolution, so
        // preserve the historical public incarnation while it is detached.
        engine::DiscardResume::FinishSpellResolution { source, .. } => (
            DiscardResumeSemanticV2::FinishSpellResolution,
            Some(detached_resolving_source_ref(
                state,
                *source,
                "pending discard spell resolution",
            )?),
        ),
        engine::DiscardResume::FinishOptionalCost {
            source,
            spell_resume,
            ..
        } => (
            DiscardResumeSemanticV2::FinishOptionalCost,
            match spell_resume {
                Some((resume_source, _)) => {
                    if resume_source != source {
                        return Err(RlContractError(
                            "pending discard optional-cost source binding mismatch".to_string(),
                        ));
                    }
                    Some(detached_resolving_source_ref(
                        state,
                        *source,
                        "pending discard optional cost",
                    )?)
                }
                None => visible_card_ref(state, *source, acting_player)?,
            },
        ),
    };
    Ok(PendingDiscardSemanticV2 {
        player: p.player.into(),
        count: p.count,
        resume_stage,
        resume_source,
    })
}

fn pending_optional_cost_semantic_v2(
    state: &GameState,
    acting_player: PlayerId,
    p: &engine::PendingOptionalCost,
) -> Result<PendingOptionalCostSemanticV2> {
    // MayPayCostThen is staged from a resolving public stack item after the
    // item was popped and before its deferred post-resolution zone move.
    let (source, spell_resume_source, spell_resume_zone) = match p.spell_resume {
        Some((resume_source, zone)) => {
            if resume_source != p.source {
                return Err(RlContractError(
                    "pending optional-cost source binding mismatch".to_string(),
                ));
            }
            let source = detached_resolving_source_ref(state, p.source, "pending optional cost")?;
            (Some(source.clone()), Some(source), Some(zone))
        }
        None => (
            visible_card_ref(state, p.source, acting_player)?,
            None,
            None,
        ),
    };
    Ok(PendingOptionalCostSemanticV2 {
        player: p.player.into(),
        source,
        discard_cards: p.discard,
        sacrifice_lands: p.sacrifice_lands,
        discard_payable: p.discard_payable,
        sacrifice_payable: p.sacrifice_payable,
        spell_resume_source,
        spell_resume_zone,
    })
}

fn pending_optional_cost_sacrifice_semantic_v2(
    state: &GameState,
    acting_player: PlayerId,
    p: &engine::PendingOptionalCostSacrifice,
) -> Result<PendingOptionalCostSacrificeSemanticV2> {
    // This is the same uninterrupted public resolving-source context carried
    // forward from PendingOptionalCost.
    let (source, spell_resume_source, spell_resume_zone) = match p.spell_resume {
        Some((resume_source, zone)) => {
            if resume_source != p.source {
                return Err(RlContractError(
                    "pending optional-cost sacrifice source binding mismatch".to_string(),
                ));
            }
            let source =
                detached_resolving_source_ref(state, p.source, "pending optional-cost sacrifice")?;
            (Some(source.clone()), Some(source), Some(zone))
        }
        None => (
            visible_card_ref(state, p.source, acting_player)?,
            None,
            None,
        ),
    };
    Ok(PendingOptionalCostSacrificeSemanticV2 {
        player: p.player.into(),
        source,
        remaining: p.remaining,
        chosen: visible_card_refs(state, &p.chosen, acting_player)?,
        spell_resume_source,
        spell_resume_zone,
    })
}

fn pending_spell_copy_semantic_v2(
    state: &GameState,
    acting_player: PlayerId,
    p: &engine::PendingSpellCopy,
) -> Result<PendingSpellCopySemanticV2> {
    let parent = engine::validate_pending_spell_copy(state, p)
        .map_err(|error| RlContractError(format!("invalid pending spell copy: {error}")))?;
    let mut inherited_targets = stack_target_refs(state, &parent)?;
    if inherited_targets.len() != 1 {
        return Err(RlContractError(
            "pending spell copy parent must project exactly one historical target".to_string(),
        ));
    }
    Ok(PendingSpellCopySemanticV2 {
        parent: visible_card_ref(state, p.resolving_source, acting_player)?,
        player: p.player.into(),
        inherited_target: inherited_targets.remove(0),
        stage: p.stage.into(),
        copy: p
            .copy_source
            .map(|id| visible_card_ref(state, id, acting_player))
            .transpose()?
            .flatten(),
    })
}

fn surface_context_v2(
    state: &GameState,
    surface: &crate::surface_v2::HarnessSurfaceV2,
    acting_player: PlayerId,
) -> Result<HarnessSurfaceContextV2> {
    let raw = surface.public_context();
    let current_stage = if raw.blockers.is_some() {
        SurfaceDecisionStageV2::DeclareBlockersForAttacker
    } else if raw.discard.is_some() {
        SurfaceDecisionStageV2::DiscardPick
    } else if let Some(optional) = raw.optional_cost.as_ref() {
        match optional.stage {
            crate::surface_v2::OptionalCostStagePublicV2::Use => {
                SurfaceDecisionStageV2::OptionalCostUse
            }
            crate::surface_v2::OptionalCostStagePublicV2::Which => {
                SurfaceDecisionStageV2::OptionalCostWhich
            }
        }
    } else {
        SurfaceDecisionStageV2::Priority
    };
    let private_blockers = match raw.blockers.as_ref() {
        Some(blockers) if acting_player == state.active_player.opponent() => {
            Some(private_blockers_context_v2(state, blockers)?)
        }
        _ => None,
    };
    let private_discard = match raw.discard.as_ref() {
        Some(discard) if acting_player == discard.player => Some(PrivateDiscardContextV2 {
            chosen: visible_card_refs(state, &discard.chosen, acting_player)?,
            remaining_choices: visible_card_refs(state, &discard.remaining_choices, acting_player)?,
            remaining_needed: discard.remaining_needed,
        }),
        _ => None,
    };
    let private_optional_cost = match raw.optional_cost.as_ref() {
        Some(optional) if acting_player == optional.player => Some(PrivateOptionalCostContextV2 {
            discard_payable: optional.discard_payable,
            sacrifice_payable: optional.sacrifice_payable,
            stage: match optional.stage {
                crate::surface_v2::OptionalCostStagePublicV2::Use => {
                    SurfaceDecisionStageV2::OptionalCostUse
                }
                crate::surface_v2::OptionalCostStagePublicV2::Which => {
                    SurfaceDecisionStageV2::OptionalCostWhich
                }
            },
        }),
        _ => None,
    };

    Ok(HarnessSurfaceContextV2 {
        current_stage,
        combat_priority_spent: raw.combat_priority_spent,
        combat_priority_rearmed_by_stack_activity: state.stack.len()
            != raw.combat_priority_stack_len_seen,
        combat_priority_rearmed_by_mana_activity: state.engine.mana_ability_activations
            != raw.combat_priority_mana_count_seen,
        stack_grew_since_round_open: state.stack.len() > raw.round_opening_stack_len,
        mana_activity_since_round_open: state.engine.mana_ability_activations
            != raw.combat_round_opening_mana_count,
        stack_length_changed_since_observed: raw
            .last_seen_stack_len
            .map(|last_len| last_len != state.stack.len()),
        mana_activity_since_last_stack_change: state.engine.mana_ability_activations
            != raw.mana_count_at_last_stack_change,
        madness_cast_reprompt_source: match raw.madness_cast_reprompt_exemption {
            Some(source) => visible_card_ref(state, source, acting_player)?,
            None => None,
        },
        private_blockers,
        private_discard,
        private_optional_cost,
    })
}

fn private_blockers_context_v2(
    state: &GameState,
    blockers: &crate::surface_v2::BlockersReshapePublicV2,
) -> Result<PrivateBlockersContextV2> {
    Ok(PrivateBlockersContextV2 {
        current_attacker: blockers
            .current_attacker
            .map(|id| card_ref(state, id))
            .transpose()?,
        accumulated: blockers
            .accumulated
            .iter()
            .map(|(attacker, blocker)| {
                Ok((card_ref(state, *attacker)?, card_ref(state, *blocker)?))
            })
            .collect::<Result<Vec<_>>>()?,
        remaining: blockers
            .remaining
            .iter()
            .map(|(attacker, legal_blockers)| {
                Ok((
                    card_ref(state, *attacker)?,
                    legal_blockers
                        .iter()
                        .map(|&blocker| card_ref(state, blocker))
                        .collect::<Result<Vec<_>>>()?,
                ))
            })
            .collect::<Result<Vec<_>>>()?,
    })
}

fn stack_public_v1(state: &GameState) -> Result<Vec<StackItemPublicV1>> {
    state
        .stack
        .iter()
        .enumerate()
        .map(|(stack_index, item)| stack_item_public_v1(state, stack_index as u32, item))
        .collect()
}

fn stack_item_public_v1(
    state: &GameState,
    stack_index: u32,
    item: &StackItem,
) -> Result<StackItemPublicV1> {
    Ok(StackItemPublicV1 {
        stack_index,
        source: card_ref(state, item.source)?,
        controller: item.controller.into(),
        targets: stack_target_refs(state, item)?,
        is_trigger_or_ability: item.kind != StackItemKind::Spell,
        is_flashback: item.is_flashback,
        mode_chosen: item.mode_chosen,
        madness_offer: item.madness_offer,
        kicked: item.kicked,
    })
}

fn stack_public_v2(state: &GameState, acting_player: PlayerId) -> Result<Vec<StackItemPublicV2>> {
    state
        .stack
        .iter()
        .enumerate()
        .map(|(stack_index, item)| {
            stack_item_public_v2(state, acting_player, stack_index as u32, item)
        })
        .collect()
}

fn stack_item_public_v2(
    state: &GameState,
    acting_player: PlayerId,
    stack_index: u32,
    item: &StackItem,
) -> Result<StackItemPublicV2> {
    if (item.kind == StackItemKind::Spell) != item.v4.cast_method.is_some() {
        return Err(RlContractError(
            "spell stack items require a cast method and abilities must not carry one".to_string(),
        ));
    }
    Ok(StackItemPublicV2 {
        stack_index,
        source: card_ref(state, item.source)?,
        controller: item.controller.into(),
        // Announced stack targets are public historical facts. Project the
        // frozen target contract rather than rebuilding a reference from a
        // potentially later incarnation currently reusing the same arena id.
        targets: stack_target_refs(state, item)?,
        stack_item_kind: item.kind.into(),
        is_copy: item.is_copy,
        is_flashback: item.is_flashback,
        mode_chosen: item.mode_chosen,
        madness_offer: item.madness_offer,
        kicked: item.kicked,
        cast_method: item.v4.cast_method,
        face_index: item.v4.face_index,
        x_value: item.v4.x_value,
        paid_cost_refs: paid_cost_card_refs(&item.v4.paid_cost_refs, acting_player),
    })
}

fn stack_target_refs(state: &GameState, item: &StackItem) -> Result<Vec<TargetRefV1>> {
    crate::engine::validated_stack_item_target_spec(item, state)
        .map_err(|error| RlContractError(format!("invalid stack target metadata: {error}")))?;
    if item.targets.len() != item.v4.target_contracts.len() {
        return Err(RlContractError(
            "stack target vector/contract length mismatch".to_string(),
        ));
    }
    item.targets
        .iter()
        .copied()
        .zip(item.v4.target_contracts.iter().copied())
        .map(|(target, contract)| {
            if contract.target() != target {
                return Err(RlContractError(
                    "stack target vector/contract identity mismatch".to_string(),
                ));
            }
            Ok(match contract {
                crate::state::StackTargetContractV4::Player(player) => TargetRefV1::Player {
                    player: player.into(),
                },
                crate::state::StackTargetContractV4::Object {
                    object,
                    card_def,
                    owner,
                    controller,
                    zone,
                    zone_change_count,
                    ..
                } => {
                    if usize::from(card_def) >= crate::card_def::CARD_DEFS.len() {
                        return Err(RlContractError(
                            "stack target contract references an unknown card definition"
                                .to_string(),
                        ));
                    }
                    let historical = state.objects.try_get(object).ok_or_else(|| {
                        RlContractError(
                            "stack target contract references an unknown arena object".to_string(),
                        )
                    })?;
                    if historical.card_def != card_def || historical.owner != owner {
                        return Err(RlContractError(
                            "stack target contract changed stable arena identity".to_string(),
                        ));
                    }
                    TargetRefV1::Object {
                        object: CardStableRefV1 {
                            arena_id: object.0,
                            card_db_id: card_def,
                            owner: owner.into(),
                            controller: controller.into(),
                            zone,
                            zone_change_count,
                        },
                    }
                }
            })
        })
        .collect()
}

fn target_ref(state: &GameState, target: Target) -> Result<TargetRefV1> {
    match target {
        Target::Player(player) => Ok(TargetRefV1::Player {
            player: player.into(),
        }),
        Target::Object(object) => Ok(TargetRefV1::Object {
            object: card_ref(state, object)?,
        }),
    }
}

fn visible_projection_hash(observation: &ObservationV1) -> Result<u64> {
    #[derive(Serialize)]
    struct ObservationHashInput<'a> {
        schema_version: u32,
        kernel_version: &'a str,
        surface_version: u32,
        card_db_hash: u64,
        acting_player: PlayerSeatV1,
        step_index: u64,
        projection: &'a PublicObservationProjectionV1,
        own_hand: &'a [CardPrivateV1],
    }

    stable_hash_json(&ObservationHashInput {
        schema_version: observation.schema_version,
        kernel_version: &observation.kernel_version,
        surface_version: observation.surface_version,
        card_db_hash: observation.card_db_hash,
        acting_player: observation.acting_player,
        step_index: observation.step_index,
        projection: &observation.projection,
        own_hand: &observation.own_hand,
    })
}

fn visible_projection_hash_v2(observation: &ObservationV2) -> Result<u64> {
    #[cfg(test)]
    TEST_VISIBLE_PROJECTION_HASH_V2_CALLS.with(|calls| calls.set(calls.get().saturating_add(1)));

    #[derive(Serialize)]
    struct ObservationHashInput<'a> {
        schema_version: u32,
        kernel_version: &'a str,
        surface_version: u32,
        card_db_hash: u64,
        acting_player: PlayerSeatV1,
        step_index: u64,
        projection: &'a PublicObservationProjectionV2,
        own_hand: &'a [CardPrivateV1],
        known_library_cards: &'a [Vec<KnownLibraryCardV4>; 2],
        known_hand_cards: &'a [Vec<CardPrivateV1>; 2],
    }

    stable_hash_json(&ObservationHashInput {
        schema_version: observation.schema_version,
        kernel_version: &observation.kernel_version,
        surface_version: observation.surface_version,
        card_db_hash: observation.card_db_hash,
        acting_player: observation.acting_player,
        step_index: observation.step_index,
        projection: &observation.projection,
        own_hand: &observation.own_hand,
        known_library_cards: &observation.known_library_cards,
        known_hand_cards: &observation.known_hand_cards,
    })
}

fn visible_projection_hash_v5(observation: &ObservationV5) -> Result<u64> {
    #[cfg(test)]
    TEST_VISIBLE_PROJECTION_HASH_V5_CALLS.with(|calls| calls.set(calls.get().saturating_add(1)));

    #[derive(Serialize)]
    struct ObservationHashInput<'a> {
        schema_version: u32,
        kernel_version: &'a str,
        surface_version: u32,
        policy_surface_version: u32,
        card_db_hash: u64,
        acting_player: PlayerSeatV1,
        step_index: u64,
        physical_decision_id: u64,
        substep_index: u32,
        substep_count: u32,
        projection: &'a PublicObservationProjectionV5,
        own_hand: &'a [CardPrivateV1],
        known_library_cards: &'a [Vec<KnownLibraryCardV4>; 2],
        known_hand_cards: &'a [Vec<CardPrivateV1>; 2],
    }

    stable_hash_json(&ObservationHashInput {
        schema_version: observation.schema_version,
        kernel_version: &observation.kernel_version,
        surface_version: observation.surface_version,
        policy_surface_version: observation.policy_surface_version,
        card_db_hash: observation.card_db_hash,
        acting_player: observation.acting_player,
        step_index: observation.step_index,
        physical_decision_id: observation.physical_decision_id,
        substep_index: observation.substep_index,
        substep_count: observation.substep_count,
        projection: &observation.projection,
        own_hand: &observation.own_hand,
        known_library_cards: &observation.known_library_cards,
        known_hand_cards: &observation.known_hand_cards,
    })
}

fn push_action(
    out: &mut Vec<CoreSurfaceActionCandidateV1>,
    semantic: ActionSemanticV1,
    surface_action: SurfaceAction,
) -> Result<()> {
    out.push(CoreSurfaceActionCandidateV1 {
        semantic,
        surface_action,
    });
    Ok(())
}

fn ensure_unique_action_ids(actions: &[LegalActionCandidateV1]) -> Result<()> {
    let mut seen = BTreeSet::new();
    for action in actions {
        if !seen.insert(action.record.stable_id.clone()) {
            return Err(RlContractError(format!(
                "duplicate stable legal action id within one decision: {}",
                action.record.stable_id
            )));
        }
    }
    Ok(())
}

fn ensure_unique_policy_action_ids(actions: &[PolicyLegalActionCandidateV5]) -> Result<()> {
    let mut seen = BTreeSet::new();
    for action in actions {
        if !seen.insert(action.record.stable_id.clone()) {
            return Err(RlContractError(format!(
                "duplicate stable legal action id within one policy decision: {}",
                action.record.stable_id
            )));
        }
    }
    Ok(())
}

fn subsets(ids: &[ObjectId]) -> Result<Vec<Vec<ObjectId>>> {
    if ids.len() > MAX_SUBSET_OBJECTS {
        return Err(RlContractError(format!(
            "legal subset decision has {} candidates, exceeding fail-closed cap {MAX_SUBSET_OBJECTS}",
            ids.len()
        )));
    }
    let count = 1usize << ids.len();
    let mut out = Vec::with_capacity(count);
    for mask in 0..count {
        let mut picked = Vec::new();
        for (i, &id) in ids.iter().enumerate() {
            if (mask & (1usize << i)) != 0 {
                picked.push(id);
            }
        }
        out.push(picked);
    }
    Ok(out)
}

fn permutations(n: usize) -> Result<Vec<Vec<usize>>> {
    if n > MAX_TRIGGER_ORDER_OBJECTS {
        return Err(RlContractError(format!(
            "trigger order decision has {n} pending triggers, exceeding fail-closed cap {MAX_TRIGGER_ORDER_OBJECTS}"
        )));
    }
    let mut current: Vec<usize> = (0..n).collect();
    let mut out = Vec::new();
    permute_from(0, &mut current, &mut out);
    Ok(out)
}

fn permute_from(start: usize, current: &mut Vec<usize>, out: &mut Vec<Vec<usize>>) {
    if start == current.len() {
        out.push(current.clone());
        return;
    }
    for i in start..current.len() {
        current.swap(start, i);
        permute_from(start + 1, current, out);
        current.swap(start, i);
    }
}

fn rng_below(rng: &mut SplitMix64, n: usize) -> usize {
    debug_assert!(n > 0);
    (rng.next_u64() % n as u64) as usize
}

fn stable_hash_json<T: Serialize>(value: &T) -> Result<u64> {
    let mut writer = Fnv1a64Writer::new();
    serde_json::to_writer(&mut writer, value)?;
    Ok(writer.finish())
}

#[cfg(test)]
thread_local! {
    static TEST_VISIBLE_PROJECTION_HASH_V2_CALLS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    static TEST_VISIBLE_PROJECTION_HASH_V5_CALLS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    static TEST_POLICY_V5_OBSERVATIONS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    static TEST_POLICY_V5_STABLE_ACTIONS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
pub(crate) fn reset_test_policy_v5_materialization_calls() {
    TEST_POLICY_V5_OBSERVATIONS.with(|calls| calls.set(0));
    TEST_POLICY_V5_STABLE_ACTIONS.with(|calls| calls.set(0));
}

#[cfg(test)]
pub(crate) fn test_policy_v5_materialization_calls() -> (u64, u64) {
    (
        TEST_POLICY_V5_OBSERVATIONS.with(std::cell::Cell::get),
        TEST_POLICY_V5_STABLE_ACTIONS.with(std::cell::Cell::get),
    )
}

#[cfg(test)]
pub(crate) fn reset_test_visible_projection_hash_calls() {
    TEST_VISIBLE_PROJECTION_HASH_V2_CALLS.with(|calls| calls.set(0));
    TEST_VISIBLE_PROJECTION_HASH_V5_CALLS.with(|calls| calls.set(0));
}

#[cfg(test)]
pub(crate) fn test_visible_projection_hash_calls() -> (u64, u64) {
    (
        TEST_VISIBLE_PROJECTION_HASH_V2_CALLS.with(std::cell::Cell::get),
        TEST_VISIBLE_PROJECTION_HASH_V5_CALLS.with(std::cell::Cell::get),
    )
}

struct Fnv1a64Writer(u64);

impl Fnv1a64Writer {
    const fn new() -> Self {
        Self(0xcbf29ce484222325)
    }

    const fn finish(self) -> u64 {
        self.0
    }
}

impl Write for Fnv1a64Writer {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0 = fnv1a64_continue(self.0, bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
fn fnv1a64(bytes: &[u8]) -> u64 {
    fnv1a64_continue(0xcbf29ce484222325, bytes)
}

fn fnv1a64_continue(mut hash: u64, bytes: &[u8]) -> u64 {
    for &b in bytes {
        hash ^= b as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

fn write_jsonl_atomic<T: Serialize>(path: &Path, records: &[T]) -> Result<()> {
    write_atomic(path, |writer| {
        for record in records {
            serde_json::to_writer(&mut *writer, record)?;
            writer.write_all(b"\n")?;
        }
        Ok(())
    })
}

fn write_json_pretty_atomic(path: &Path, manifest: &RunManifestV1) -> Result<()> {
    write_atomic(path, |writer| {
        serde_json::to_writer_pretty(&mut *writer, manifest)?;
        writer.write_all(b"\n")?;
        Ok(())
    })
}

fn write_atomic(
    path: &Path,
    write_fn: impl FnOnce(&mut BufWriter<File>) -> Result<()>,
) -> Result<()> {
    let tmp = tmp_path(path);
    if tmp.exists() {
        fs::remove_file(&tmp)?;
    }
    {
        let file = File::create(&tmp)?;
        let mut writer = BufWriter::new(file);
        write_fn(&mut writer)?;
        writer.flush()?;
    }
    if path.exists() {
        fs::remove_file(path)?;
    }
    fs::rename(tmp, path)?;
    Ok(())
}

fn tmp_path(path: &Path) -> PathBuf {
    let file_name = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("artifact");
    path.with_file_name(format!("{file_name}.tmp"))
}

#[allow(dead_code)]
fn _assert_game_object_is_visible_data(_: &GameObject) {}

#[cfg(test)]
mod policy_v5_artifact_tests {
    use super::*;
    use crate::card_def::card_id_by_name;
    use crate::policy_surface_v5::PolicySurfaceV5;
    use crate::state::{Counters, ObjectStateV4, Step};

    #[test]
    fn policy_v5_hashes_only_the_final_observation() {
        let state = GameState::new_from_libraries(&[], &[], card_name, 77);
        let surface = PolicySurfaceV5::new();

        reset_test_visible_projection_hash_calls();
        let v2 = observe_v2(&state, surface.harness_surface(), PlayerId::P0, 0).unwrap();
        assert_ne!(v2.visible_projection_hash, 0);
        assert_eq!(test_visible_projection_hash_calls(), (1, 0));

        reset_test_visible_projection_hash_calls();
        let v5 = observe_policy_v5(&state, &surface, PlayerId::P0, 0, 0, 0, 1).unwrap();
        assert_ne!(v5.visible_projection_hash, 0);
        assert_eq!(test_visible_projection_hash_calls(), (0, 1));

        reset_test_visible_projection_hash_calls();
        let mut flat =
            observe_policy_v5_unhashed_for_flat_policy(&state, &surface, PlayerId::P0, 0, 0, 0, 1)
                .unwrap();
        assert_eq!(flat.visible_projection_hash, 0);
        assert_eq!(test_visible_projection_hash_calls(), (0, 0));
        flat.visible_projection_hash = v5.visible_projection_hash;
        assert_eq!(flat, v5);
    }

    #[test]
    fn streaming_stable_hash_matches_the_canonical_json_bytes() {
        let value = serde_json::json!({
            "array": [null, true, -17, "escaped\ntext"],
            "nested": {"unicode": "Black Lotus \u{1f3b4}"},
        });
        let canonical = serde_json::to_vec(&value).unwrap();
        assert_eq!(stable_hash_json(&value).unwrap(), fnv1a64(&canonical));
    }

    fn scan_records() -> Vec<PolicyEpisodeRecordV2> {
        let mut state = GameState::new_from_libraries(&[], &[], card_name, 77);
        state.step = Step::DeclareAttackers;
        state.active_player = PlayerId::P0;
        state.priority_player = PlayerId::P0;
        let card_def = card_id_by_name("Voldaren Epicure").unwrap();
        for _ in 0..3 {
            let id = state.objects.push(GameObject {
                card_def,
                name: "Voldaren Epicure".to_string(),
                owner: PlayerId::P0,
                controller: PlayerId::P0,
                zone: Zone::Battlefield,
                tapped: false,
                summoning_sick: false,
                damage: 0,
                counters: Counters::default(),
                attachments: Vec::new(),
                v4: ObjectStateV4::from_card_def(card_def),
                spell_copy_origin: None,
                plotted_turn: None,
                zone_change_count: 0,
            });
            state.players[0].battlefield.push(id);
        }
        let mut surface = PolicySurfaceV5::new();
        let mut records = vec![PolicyEpisodeRecordV2::Header {
            schema_version: POLICY_EPISODE_SCHEMA_VERSION,
            stream_safety: "policy_safe_model_visible_v5".to_string(),
            kernel_version: KERNEL_VERSION.to_string(),
            surface_version: H2_PREDICATE_VERSION,
            policy_surface_version: POLICY_SURFACE_VERSION,
            card_db_hash: KERNEL_CARDDB_HASH,
            matchup: "test".to_string(),
            episode_id: 0,
            episode_key: "test-0".to_string(),
            deck_identifiers: ["Burn".to_string(), "Burn".to_string()],
        }];
        for (step, selected_index) in [1usize, 0, 1].into_iter().enumerate() {
            let decision = surface.next_decision(&mut state).unwrap();
            let (substep_index, substep_count) = decision.substep();
            let observation = observe_policy_v5(
                &state,
                &surface,
                PlayerId::P0,
                step as u64,
                0,
                substep_index,
                substep_count,
            )
            .unwrap();
            let actions = legal_action_candidates_v5(&decision, &state).unwrap();
            let selected_action_id = actions[selected_index].record.stable_id.clone();
            records.push(PolicyEpisodeRecordV2::Decision {
                schema_version: POLICY_EPISODE_SCHEMA_VERSION,
                episode_id: 0,
                step: step as u64,
                physical_decision_id: 0,
                substep_index,
                substep_count,
                acting_player: PlayerSeatV1::P0,
                observation: Box::new(observation),
                legal_actions: actions.iter().map(|action| action.record.clone()).collect(),
                selected_index: selected_index as u32,
                selected_action_id,
                reward: [0, 0],
            });
            surface
                .apply(&mut state, actions[selected_index].policy_action.clone())
                .unwrap();
        }
        records.push(PolicyEpisodeRecordV2::Terminal {
            schema_version: POLICY_EPISODE_SCHEMA_VERSION,
            episode_id: 0,
            terminal_outcome: TerminalOutcomeV1::Truncated,
            terminal_classification: TerminalClassificationV1::Truncated,
            terminal_code: TerminalSafeCodeV2::DecisionCap,
            winner: None,
            terminal_reward: [0, 0],
            policy_step_count: 3,
            physical_decision_count: 1,
        });
        records
    }

    #[test]
    fn artifact_grouping_fsm_rejects_outer_actor_count_history_and_partial_terminal_drift() {
        let valid = scan_records();
        validate_policy_episode_records(&valid).unwrap();

        let mut outer = valid.clone();
        let PolicyEpisodeRecordV2::Decision {
            physical_decision_id,
            ..
        } = &mut outer[1]
        else {
            unreachable!()
        };
        *physical_decision_id = 1;
        assert!(validate_policy_episode_records(&outer)
            .unwrap_err()
            .to_string()
            .contains("outer grouping fields"));

        let mut actor = valid.clone();
        let PolicyEpisodeRecordV2::Decision {
            acting_player,
            observation,
            ..
        } = &mut actor[2]
        else {
            unreachable!()
        };
        *acting_player = PlayerSeatV1::P1;
        observation.acting_player = PlayerSeatV1::P1;
        assert!(validate_policy_episode_records(&actor)
            .unwrap_err()
            .to_string()
            .contains("drifts actor"));

        let mut count = valid.clone();
        let PolicyEpisodeRecordV2::Decision {
            substep_count,
            observation,
            ..
        } = &mut count[2]
        else {
            unreachable!()
        };
        *substep_count = 4;
        observation.substep_count = 4;
        assert!(validate_policy_episode_records(&count)
            .unwrap_err()
            .to_string()
            .contains("frozen substep count"));

        let mut history = valid.clone();
        let PolicyEpisodeRecordV2::Decision { observation, .. } = &mut history[2] else {
            unreachable!()
        };
        observation
            .projection
            .policy_surface_context
            .private_combat_selection
            .as_mut()
            .unwrap()
            .selected
            .clear();
        observation.visible_projection_hash = visible_projection_hash_v5(observation).unwrap();
        assert!(validate_policy_episode_records(&history)
            .unwrap_err()
            .to_string()
            .contains("selected history drifted"));

        let partial = vec![valid[0].clone(), valid[1].clone(), valid[4].clone()];
        assert!(validate_policy_episode_records(&partial)
            .unwrap_err()
            .to_string()
            .contains("partial physical decision"));

        let mut wrong_header = valid.clone();
        let PolicyEpisodeRecordV2::Header {
            policy_surface_version,
            ..
        } = &mut wrong_header[0]
        else {
            unreachable!()
        };
        *policy_surface_version = 4;
        assert!(validate_policy_episode_records(&wrong_header)
            .unwrap_err()
            .to_string()
            .contains("header provenance mismatch"));

        let mut wrong_stage = valid.clone();
        let PolicyEpisodeRecordV2::Decision { observation, .. } = &mut wrong_stage[1] else {
            unreachable!()
        };
        observation.projection.policy_surface_context.current_stage = PolicySurfaceStageV5::Surface;
        observation
            .projection
            .policy_surface_context
            .private_combat_selection = None;
        observation.visible_projection_hash = visible_projection_hash_v5(observation).unwrap();
        assert!(validate_policy_episode_records(&wrong_stage)
            .unwrap_err()
            .to_string()
            .contains("surface-stage decision contains combat-scan payload"));

        let mut reversed = valid.clone();
        let PolicyEpisodeRecordV2::Decision {
            legal_actions,
            selected_index,
            selected_action_id,
            ..
        } = &mut reversed[1]
        else {
            unreachable!()
        };
        legal_actions.swap(0, 1);
        legal_actions[0].selected_index = 0;
        legal_actions[1].selected_index = 1;
        *selected_index = 0;
        *selected_action_id = legal_actions[0].stable_id.clone();
        assert!(validate_policy_episode_records(&reversed)
            .unwrap_err()
            .to_string()
            .contains("exact [include:false, include:true] pair"));
    }

    #[test]
    fn v5_constructor_rejects_legacy_aggregate_combat_semantics() {
        let records = scan_records();
        let PolicyEpisodeRecordV2::Decision { observation, .. } = &records[1] else {
            unreachable!()
        };
        let current = observation
            .projection
            .policy_surface_context
            .private_combat_selection
            .as_ref()
            .unwrap()
            .current_candidate
            .clone();
        assert!(make_legal_action_v5(
            0,
            ActionSemanticV1::DeclareAttackers {
                actor: PlayerSeatV1::P0,
                attackers: vec![current],
            },
            None,
        )
        .unwrap_err()
        .to_string()
        .contains("legacy aggregate combat semantic"));
    }
}
