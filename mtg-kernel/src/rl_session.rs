//! Interactive RL session protocol for deck-identified kernel environments.
//!
//! This module owns the reset/step state machine used by both the JSONL
//! process wrapper and the batch rollout recorder, so action validation and
//! terminal classification cannot drift between interactive and offline use.
//! Schema v5 carries ordered physical-seat deck identity on the wire. Exact
//! canonical `Burn` and `Rally` ids may be combined in any ordered pair; every
//! other id fails before an active session is created or replaced.

use crate::card_def::KERNEL_CARDDB_HASH;
use crate::engine::{CastMode, CostKind, Decision, OptionalCostChoice};
use crate::ids::{ObjectId, PlayerId};
use crate::mana::ManaColor;
use crate::phase_profile::{measure_optional, RlPhaseProfileV1, RlPhaseV1};
use crate::policy_surface_v5::{
    FastActorInPlaceApplyErrorV1, PolicyActionV5, PolicyDecisionV5, PolicySurfaceContextIdsV5,
    PolicySurfaceV5, POLICY_ENVIRONMENT_HASH_ALGORITHM, POLICY_SURFACE_VERSION,
};
use crate::rl::{
    build_deck_pair_state, core_policy_action_candidates_v5, legal_action_candidates_v5,
    observe_policy_v5, parse_strict_json_value, ActionSemanticV1, CardStableRefV1,
    CorePolicyActionCandidateV1, EpisodeTerminalSummaryV1, LegalActionV5, ObservationV5,
    PlayerSeatV1, PolicyLegalActionCandidateV5, RlContractError, TargetRefV1,
    TerminalClassificationV1, TerminalOutcomeV1, TerminalSafeCodeV2,
};
use crate::runtime_decks::{runtime_deck_by_id, RuntimeDeckDefinition};
use crate::state::{Target, Zone};
use crate::surface_v2::{SuppressionAuditMode, SurfaceDecision, H2_PREDICATE_VERSION};
use crate::KERNEL_VERSION;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::fmt;

pub const RL_SESSION_SCHEMA_VERSION: u32 = 5;
pub const RL_SESSION_PROTOCOL_VERSION: u32 = 5;
pub const RL_SESSION_PROTOCOL_NAME: &str = "kernel_rl_jsonl";
pub const FAST_ACTOR_CORE_ENVIRONMENT_HASH_ALGORITHM: &str =
    "fnv1a64-serde-json-fast-actor-core-environment-v1";
pub const CANONICAL_BURN_DECK_ID: &str = "Burn";
pub const CANONICAL_RALLY_DECK_ID: &str = "Rally";

pub type SessionDeckIdsV1 = [String; 2];
pub type SessionDeckHashesV1 = [u64; 2];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RlSessionProvenanceV1 {
    pub protocol: String,
    pub protocol_version: u32,
    pub schema_version: u32,
    pub kernel_version: String,
    pub surface_version: u32,
    pub policy_surface_version: u32,
    pub card_db_hash: u64,
}

impl RlSessionProvenanceV1 {
    pub fn current() -> Self {
        RlSessionProvenanceV1 {
            protocol: RL_SESSION_PROTOCOL_NAME.to_string(),
            protocol_version: RL_SESSION_PROTOCOL_VERSION,
            schema_version: RL_SESSION_SCHEMA_VERSION,
            kernel_version: KERNEL_VERSION.to_string(),
            surface_version: H2_PREDICATE_VERSION,
            policy_surface_version: POLICY_SURFACE_VERSION,
            card_db_hash: KERNEL_CARDDB_HASH,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RlSessionDecisionV1 {
    pub schema_version: u32,
    pub deck_ids: SessionDeckIdsV1,
    pub deck_hashes: SessionDeckHashesV1,
    pub episode_id: u64,
    pub step: u64,
    pub physical_decision_id: u64,
    pub substep_index: u32,
    pub substep_count: u32,
    pub acting_player: PlayerSeatV1,
    pub observation: Box<ObservationV5>,
    pub legal_actions: Vec<LegalActionV5>,
    pub reward: [i32; 2],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RlSessionTerminalV1 {
    pub schema_version: u32,
    pub deck_ids: SessionDeckIdsV1,
    pub deck_hashes: SessionDeckHashesV1,
    pub episode_id: u64,
    pub terminal_outcome: TerminalOutcomeV1,
    pub terminal_classification: TerminalClassificationV1,
    pub terminal_code: TerminalSafeCodeV2,
    pub winner: Option<PlayerSeatV1>,
    pub terminal_reward: [i32; 2],
    pub terminal_reason: String,
    pub policy_step_count: u64,
    pub physical_decision_count: u64,
}

impl From<RlSessionTerminalV1> for EpisodeTerminalSummaryV1 {
    fn from(value: RlSessionTerminalV1) -> Self {
        EpisodeTerminalSummaryV1 {
            episode_id: value.episode_id,
            outcome: value.terminal_outcome,
            classification: value.terminal_classification,
            winner: value.winner,
            terminal_reward: value.terminal_reward,
            terminal_reason: value.terminal_reason,
            policy_step_count: value.policy_step_count,
            physical_decision_count: value.physical_decision_count,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RlSessionResponseV1 {
    Decision(RlSessionDecisionV1),
    Terminal(RlSessionTerminalV1),
}

/// Allocation-light, in-process decision metadata for actor loops.
///
/// This type deliberately has no serde implementation and is not part of the
/// JSONL protocol. The only action handle exposed to an actor is its dense,
/// ordered index.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FastActorDecisionKindV1 {
    Surface,
    AttackerInclusion,
    BlockerInclusion,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FastActorDecisionV1 {
    pub episode_id: u64,
    pub step: u64,
    pub environment_revision: u64,
    pub physical_decision_id: u64,
    pub substep_index: u32,
    pub substep_count: u32,
    pub acting_player: PlayerSeatV1,
    pub decision_kind: FastActorDecisionKindV1,
    pub legal_action_count: u32,
}

/// Contract identifier for the deliberately partial action/binding encoder.
///
/// This slice contains no globals, full object state, relations, model input,
/// or scorer claim. It must not be relabeled as `FlatDecisionV1`.
pub const FLAT_ACTION_DECISION_SLICE_VERSION_V1: u32 = 1;
pub const FLAT_ACTION_MAX_TRIGGER_ORDER_REFS_V1: usize = 7;
pub const FLAT_ACTION_REF_ROLE_MAPPING_VERSION_V1: u32 = 1;
pub const FLAT_ACTION_CARD_TOKEN_MAPPING_VERSION_V1: u32 = 1;
pub const FLAT_ACTION_CANDIDATE_COMMITMENT_VERSION_V1: u32 = 1;

/// Schema-v5 action-kind vocabulary used by the scalar mapper. The current
/// executable flat session subset deliberately rejects `ChooseEffectColor`,
/// `ChooseEffectNumber`, and `FinishTargetSelection`: schema-v5 reserves those
/// semantic rows, but policy-v5 has no executable `Action` for them yet.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[repr(u8)]
pub enum FlatActionKindV1 {
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

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[repr(u8)]
pub enum FlatActionRefRoleV1 {
    #[default]
    Source = 0,
    Candidate = 1,
    Card = 2,
    Attacker = 3,
    Blocker = 4,
    TargetObject = 5,
    Cards = 6,
    PendingSources = 7,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum FlatActionObjectGroupV1 {
    #[default]
    SelfHand = 0,
    KnownOpponentHand = 1,
    SelfBattlefield = 2,
    OpponentBattlefield = 3,
    SelfGraveyard = 4,
    OpponentGraveyard = 5,
    Exile = 6,
    Stack = 7,
    Command = 8,
    KnownSelfLibrary = 9,
    KnownOpponentLibrary = 10,
}

pub const FLAT_ACTION_FLAG_PAY_V1: u16 = 1 << 0;
pub const FLAT_ACTION_FLAG_CHANGE_TARGET_V1: u16 = 1 << 1;
pub const FLAT_ACTION_FLAG_USE_COST_V1: u16 = 1 << 2;
pub const FLAT_ACTION_FLAG_CAST_IT_V1: u16 = 1 << 3;
pub const FLAT_ACTION_FLAG_VALUE_V1: u16 = 1 << 4;
pub const FLAT_ACTION_FLAG_INCLUDE_V1: u16 = 1 << 5;

/// Exact fixed-width scalar portion of one ordered policy-v5 action.
///
/// Enum fields use zero for absence and one-based ids for present values.
/// Card/object semantics live in the ragged reference table.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FlatActionCoreV1 {
    pub kind: FlatActionKindV1,
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

/// Actor-visible object identity used only by the partial action slice.
/// Raw arena ids are deliberately absent.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FlatActionObjectV1 {
    pub card_token: u16,
    pub group: FlatActionObjectGroupV1,
    pub actor_visible_ordinal: u16,
    pub owner_relative: u8,
    pub controller_relative: u8,
    pub zone: u8,
    pub zone_change_count: u32,
}

impl FlatActionObjectV1 {
    fn canonical_key(self) -> (u8, u16, u32, u16, u8, u8, u8) {
        (
            self.group as u8,
            self.actor_visible_ordinal,
            self.zone_change_count,
            self.card_token,
            self.owner_relative,
            self.controller_relative,
            self.zone,
        )
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FlatActionRefV1 {
    pub action_index: u32,
    pub role: FlatActionRefRoleV1,
    pub order_index: u16,
    pub associated_order: u16,
    pub card_token: u16,
    pub object_index: u16,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FlatActionDecisionBindingV1 {
    pub slice_version: u32,
    pub ref_role_mapping_version: u32,
    pub card_token_mapping_version: u32,
    pub candidate_commitment_version: u32,
    pub card_db_hash: u64,
    pub episode_id: u64,
    pub environment_revision: u64,
    pub bound_policy_step_count: u64,
    pub physical_decision_id: u64,
    pub bound_physical_decision_count: u64,
    pub substep_index: u32,
    pub substep_count: u32,
    pub acting_player: u8,
    pub decision_kind: u8,
    pub legal_action_count: u32,
    /// First 128 bits of SHA-256 over the versioned ordered compact action,
    /// reference, and referenced-object records. This is a stale-result guard,
    /// not a collision-proof authorization token or artifact digest.
    pub candidate_order_commitment: [u8; 16],
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FlatActionDecisionSliceV1 {
    pub binding: FlatActionDecisionBindingV1,
    pub active_action_count: u32,
    pub active_ref_count: u32,
    pub active_object_count: u16,
}

/// Audit-only shape record for a live private flat-action decision.
///
/// This exposes counts, never raw arena ids, private card identities, or cache
/// storage. It is intended for environment-only diagnostics and is not a
/// model-input or artifact contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg(any(test, feature = "flat-action-diagnostic"))]
pub struct FlatActionDecisionDiagnosticV1 {
    pub arena_object_count: u32,
    pub action_count: u32,
    pub ref_count: u32,
    pub referenced_object_count: u16,
}

pub struct FlatActionDecisionSliceBuffersV1<'a> {
    pub actions: &'a mut [FlatActionCoreV1],
    pub refs: &'a mut [FlatActionRefV1],
    pub objects: &'a mut [FlatActionObjectV1],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlatActionDecisionSliceErrorV1 {
    NoCurrentDecision,
    StaleEpisodeBinding,
    StaleEnvironmentRevision,
    DecisionMetadataMismatch,
    CorruptCurrentBinding,
    ActingPlayerMismatch,
    UnsupportedActionSemantic,
    InvalidTriggerOrder,
    InvalidActionRange,
    InvalidDecisionRelation,
    InvalidActionReference,
    HiddenActionReference,
    CheckedIntegerRange,
    DuplicateCanonicalObject,
    InsufficientActionCapacity { required: usize, available: usize },
    InsufficientRefCapacity { required: usize, available: usize },
    InsufficientObjectCapacity { required: usize, available: usize },
}

/// Non-wire response for [`FastActorSessionV1`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FastActorResponseV1 {
    Decision(FastActorDecisionV1),
    Terminal(RlSessionTerminalV1),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RlSessionErrorCode {
    EpisodeAlreadyTerminal,
    EpisodeIdMismatch,
    ExpectedStepMismatch,
    SelectedIndexOutOfRange,
    SelectedActionIdMismatch,
    SelectedActionIdUnknown,
    StaleEnvironmentBinding,
    UnsupportedDeck,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RlSessionError {
    pub code: RlSessionErrorCode,
    pub message: String,
}

impl fmt::Display for RlSessionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}: {}", self.code, self.message)
    }
}

impl std::error::Error for RlSessionError {}

impl From<RlSessionError> for RlContractError {
    fn from(value: RlSessionError) -> Self {
        RlContractError(value.to_string())
    }
}

#[derive(Debug, Clone)]
struct CurrentDecisionV1 {
    actor: PlayerId,
    physical_decision_id: u64,
    substep_index: u32,
    substep_count: u32,
    observation: ObservationV5,
    candidates: Vec<PolicyLegalActionCandidateV5>,
    environment_revision: u64,
    bound_policy_step_count: u64,
    bound_physical_decision_count: u64,
}

#[derive(Debug, Clone)]
struct FastActorCurrentDecisionV1 {
    actor: PlayerId,
    decision_kind: FastActorDecisionKindV1,
    /// Exact private decision that produced `candidates`. The flat encoder
    /// validates candidate context against this value without rebuilding the
    /// allocating policy-v5 candidate vector.
    origin_decision: PolicyDecisionV5,
    physical_decision_id: u64,
    substep_index: u32,
    substep_count: u32,
    candidates: Vec<CorePolicyActionCandidateV1>,
    environment_revision: u64,
    bound_policy_step_count: u64,
    bound_physical_decision_count: u64,
    /// Private, exact compact rows and v1 commitment for this immutable
    /// decision. The cache is constructed only from the session-owned state
    /// and candidate vector when the decision is published. Public v1 rows
    /// and commitment bytes remain unchanged; this is not a wire-contract
    /// version change.
    flat_action_cache: Option<FlatActionDecisionCacheV1>,
    /// Exact construction failure for a decision outside the partial flat
    /// action slice. The ordinary FastActor path remains usable; flat encode
    /// returns this unchanged v1 error and flat consume has no cache authority.
    flat_action_cache_error: Option<FlatActionDecisionSliceErrorV1>,
}

fn flat_player_id_v1(seat: PlayerSeatV1) -> PlayerId {
    match seat {
        PlayerSeatV1::P0 => PlayerId::P0,
        PlayerSeatV1::P1 => PlayerId::P1,
    }
}

fn flat_relative_seat_v1(
    seat: PlayerSeatV1,
    actor: PlayerSeatV1,
) -> Result<u8, FlatActionDecisionSliceErrorV1> {
    if seat == actor {
        Ok(0)
    } else if seat
        == match actor {
            PlayerSeatV1::P0 => PlayerSeatV1::P1,
            PlayerSeatV1::P1 => PlayerSeatV1::P0,
        }
    {
        Ok(1)
    } else {
        Err(FlatActionDecisionSliceErrorV1::ActingPlayerMismatch)
    }
}

fn flat_mana_color_v1(color: ManaColor) -> u8 {
    match color {
        ManaColor::W => 1,
        ManaColor::U => 2,
        ManaColor::B => 3,
        ManaColor::R => 4,
        ManaColor::G => 5,
        ManaColor::C => 6,
    }
}

fn flat_cast_mode_v1(mode: CastMode) -> u8 {
    match mode {
        CastMode::Normal => 1,
        CastMode::Alternative => 2,
    }
}

fn flat_cost_kind_v1(kind: CostKind) -> u8 {
    match kind {
        CostKind::SacrificeLands => 1,
        CostKind::SacrificePermanents => 2,
        CostKind::SacrificeCreatures => 3,
        CostKind::SacrificeArtifacts => 4,
        CostKind::DiscardCards => 5,
        CostKind::ExileFromGraveyard => 6,
        CostKind::TapPermanents => 7,
        CostKind::ReturnPermanentsToHand => 8,
        CostKind::PayLife => 9,
        CostKind::RemoveCounters => 10,
        CostKind::PutCounters => 11,
    }
}

fn flat_optional_cost_choice_v1(choice: OptionalCostChoice) -> u8 {
    match choice {
        OptionalCostChoice::Decline => 1,
        OptionalCostChoice::Discard => 2,
        OptionalCostChoice::SacrificeLand => 3,
    }
}

fn flat_action_kind_id_v1(kind: FlatActionKindV1) -> u8 {
    match kind {
        FlatActionKindV1::Pass => 0,
        FlatActionKindV1::PlayLand => 1,
        FlatActionKindV1::CastSpell => 2,
        FlatActionKindV1::ActivateManaAbility => 3,
        FlatActionKindV1::ActivateAbility => 4,
        FlatActionKindV1::PlotSpell => 5,
        FlatActionKindV1::ChooseTarget => 6,
        FlatActionKindV1::ChooseCostTarget => 7,
        FlatActionKindV1::ChooseCastMode => 8,
        FlatActionKindV1::ChooseKicker => 9,
        FlatActionKindV1::ChooseSpellMode => 10,
        FlatActionKindV1::ChooseEffectOption => 11,
        FlatActionKindV1::ChooseEffectTarget => 12,
        FlatActionKindV1::FinishEffectSelection => 13,
        FlatActionKindV1::ChooseEffectColor => 14,
        FlatActionKindV1::ChooseEffectNumber => 15,
        FlatActionKindV1::ChooseEffectBoolean => 16,
        FlatActionKindV1::FinishTargetSelection => 17,
        FlatActionKindV1::ChooseOptionalCostUse => 18,
        FlatActionKindV1::ChooseOptionalCostWhich => 19,
        FlatActionKindV1::ChooseSpellCopyPayment => 20,
        FlatActionKindV1::ChooseSpellCopyRetarget => 21,
        FlatActionKindV1::ChooseMadnessCast => 22,
        FlatActionKindV1::Discard => 23,
        FlatActionKindV1::ChooseAttackerInclusion => 24,
        FlatActionKindV1::ChooseBlockerInclusion => 25,
        FlatActionKindV1::OrderTriggers => 26,
    }
}

/// Stable accelerator/reference vocabulary. Callers must bind
/// [`FLAT_ACTION_REF_ROLE_MAPPING_VERSION_V1`] rather than relying on Rust
/// discriminant layout.
fn flat_action_ref_role_id_v1(role: FlatActionRefRoleV1) -> u8 {
    match role {
        FlatActionRefRoleV1::Source => 0,
        FlatActionRefRoleV1::Candidate => 1,
        FlatActionRefRoleV1::Card => 2,
        FlatActionRefRoleV1::Attacker => 3,
        FlatActionRefRoleV1::Blocker => 4,
        FlatActionRefRoleV1::TargetObject => 5,
        FlatActionRefRoleV1::Cards => 6,
        FlatActionRefRoleV1::PendingSources => 7,
    }
}

fn flat_decision_kind_id_v1(kind: FastActorDecisionKindV1) -> u8 {
    match kind {
        FastActorDecisionKindV1::Surface => 0,
        FastActorDecisionKindV1::AttackerInclusion => 1,
        FastActorDecisionKindV1::BlockerInclusion => 2,
    }
}

fn flat_zone_v1(zone: Zone) -> u8 {
    match zone {
        Zone::Library => 0,
        Zone::Hand => 1,
        Zone::Battlefield => 2,
        Zone::Graveyard => 3,
        Zone::Stack => 4,
        Zone::Exile => 5,
        Zone::Command => 6,
    }
}

fn flat_action_core_and_refs_v1<F>(
    semantic: &ActionSemanticV1,
    expected_actor: PlayerSeatV1,
    ref_start: u32,
    mut emit_ref: F,
) -> Result<FlatActionCoreV1, FlatActionDecisionSliceErrorV1>
where
    F: FnMut(
        FlatActionRefRoleV1,
        u16,
        u16,
        &CardStableRefV1,
    ) -> Result<(), FlatActionDecisionSliceErrorV1>,
{
    let mut core = FlatActionCoreV1 {
        ref_start,
        ..FlatActionCoreV1::default()
    };
    let mut ref_count = 0_usize;
    let mut push_ref = |role: FlatActionRefRoleV1,
                        order: usize,
                        associated_order: usize,
                        reference: &CardStableRefV1|
     -> Result<(), FlatActionDecisionSliceErrorV1> {
        let order = u16::try_from(order)
            .map_err(|_| FlatActionDecisionSliceErrorV1::CheckedIntegerRange)?;
        let associated_order = u16::try_from(associated_order)
            .map_err(|_| FlatActionDecisionSliceErrorV1::CheckedIntegerRange)?;
        emit_ref(role, order, associated_order, reference)?;
        ref_count = ref_count
            .checked_add(1)
            .ok_or(FlatActionDecisionSliceErrorV1::CheckedIntegerRange)?;
        Ok(())
    };
    let check_actor = |actor: PlayerSeatV1| {
        if actor == expected_actor {
            Ok(())
        } else {
            Err(FlatActionDecisionSliceErrorV1::ActingPlayerMismatch)
        }
    };
    match semantic {
        ActionSemanticV1::Pass { actor } => {
            check_actor(*actor)?;
            core.kind = FlatActionKindV1::Pass;
        }
        ActionSemanticV1::PlayLand { actor, source } => {
            check_actor(*actor)?;
            core.kind = FlatActionKindV1::PlayLand;
            push_ref(FlatActionRefRoleV1::Source, 0, 0, source)?;
        }
        ActionSemanticV1::CastSpell { actor, source } => {
            check_actor(*actor)?;
            core.kind = FlatActionKindV1::CastSpell;
            push_ref(FlatActionRefRoleV1::Source, 0, 0, source)?;
        }
        ActionSemanticV1::ActivateManaAbility {
            actor,
            source,
            mana_choice,
        } => {
            check_actor(*actor)?;
            core.kind = FlatActionKindV1::ActivateManaAbility;
            core.mana_choice = mana_choice.map(flat_mana_color_v1).unwrap_or(0);
            push_ref(FlatActionRefRoleV1::Source, 0, 0, source)?;
        }
        ActionSemanticV1::ActivateAbility {
            actor,
            source,
            ability_index,
        } => {
            check_actor(*actor)?;
            core.kind = FlatActionKindV1::ActivateAbility;
            core.ability_index = *ability_index;
            push_ref(FlatActionRefRoleV1::Source, 0, 0, source)?;
        }
        ActionSemanticV1::PlotSpell { actor, source } => {
            check_actor(*actor)?;
            core.kind = FlatActionKindV1::PlotSpell;
            push_ref(FlatActionRefRoleV1::Source, 0, 0, source)?;
        }
        ActionSemanticV1::ChooseTarget {
            actor,
            source,
            remaining,
            target,
        } => {
            check_actor(*actor)?;
            if *remaining == 0 {
                return Err(FlatActionDecisionSliceErrorV1::InvalidActionRange);
            }
            core.kind = FlatActionKindV1::ChooseTarget;
            core.remaining = *remaining;
            push_ref(FlatActionRefRoleV1::Source, 0, 0, source)?;
            match target {
                TargetRefV1::Player { player } => {
                    core.target_kind = 1;
                    core.target_player = flat_relative_seat_v1(*player, expected_actor)? + 1;
                }
                TargetRefV1::Object { object } => {
                    core.target_kind = 2;
                    push_ref(FlatActionRefRoleV1::TargetObject, 0, 0, object)?;
                }
            }
        }
        ActionSemanticV1::ChooseCostTarget {
            actor,
            source,
            cost_kind,
            remaining,
            candidate,
        } => {
            check_actor(*actor)?;
            if *remaining == 0 {
                return Err(FlatActionDecisionSliceErrorV1::InvalidActionRange);
            }
            core.kind = FlatActionKindV1::ChooseCostTarget;
            core.cost_kind = flat_cost_kind_v1(*cost_kind);
            core.remaining = *remaining;
            push_ref(FlatActionRefRoleV1::Source, 0, 0, source)?;
            push_ref(FlatActionRefRoleV1::Candidate, 0, 0, candidate)?;
        }
        ActionSemanticV1::ChooseCastMode {
            actor,
            source,
            mode,
        } => {
            check_actor(*actor)?;
            core.kind = FlatActionKindV1::ChooseCastMode;
            core.cast_mode = flat_cast_mode_v1(*mode);
            push_ref(FlatActionRefRoleV1::Source, 0, 0, source)?;
        }
        ActionSemanticV1::ChooseKicker { actor, source, pay } => {
            check_actor(*actor)?;
            core.kind = FlatActionKindV1::ChooseKicker;
            if *pay {
                core.flags |= FLAT_ACTION_FLAG_PAY_V1;
            }
            push_ref(FlatActionRefRoleV1::Source, 0, 0, source)?;
        }
        ActionSemanticV1::ChooseSpellMode {
            actor,
            source,
            mode_index,
            mode_count,
        } => {
            check_actor(*actor)?;
            if *mode_count == 0 || *mode_index >= *mode_count {
                return Err(FlatActionDecisionSliceErrorV1::InvalidActionRange);
            }
            core.kind = FlatActionKindV1::ChooseSpellMode;
            core.mode_index = *mode_index;
            core.mode_count = *mode_count;
            push_ref(FlatActionRefRoleV1::Source, 0, 0, source)?;
        }
        ActionSemanticV1::ChooseEffectOption {
            actor,
            source,
            option_index,
            option_count,
        } => {
            check_actor(*actor)?;
            if *option_count < 2 || *option_index >= *option_count {
                return Err(FlatActionDecisionSliceErrorV1::InvalidActionRange);
            }
            core.kind = FlatActionKindV1::ChooseEffectOption;
            core.option_index = *option_index;
            core.option_count = *option_count;
            push_ref(FlatActionRefRoleV1::Source, 0, 0, source)?;
        }
        ActionSemanticV1::ChooseEffectTarget {
            actor,
            source,
            target,
            selected_count,
            min_targets,
            max_targets,
        } => {
            check_actor(*actor)?;
            if *min_targets > *max_targets || *selected_count >= *max_targets {
                return Err(FlatActionDecisionSliceErrorV1::InvalidActionRange);
            }
            core.kind = FlatActionKindV1::ChooseEffectTarget;
            core.selected_count = *selected_count;
            core.min_targets = *min_targets;
            core.max_targets = *max_targets;
            push_ref(FlatActionRefRoleV1::Source, 0, 0, source)?;
            match target {
                TargetRefV1::Player { player } => {
                    core.target_kind = 1;
                    core.target_player = flat_relative_seat_v1(*player, expected_actor)? + 1;
                }
                TargetRefV1::Object { object } => {
                    core.target_kind = 2;
                    push_ref(FlatActionRefRoleV1::TargetObject, 0, 0, object)?;
                }
            }
        }
        ActionSemanticV1::FinishEffectSelection {
            actor,
            source,
            selected_count,
        } => {
            check_actor(*actor)?;
            core.kind = FlatActionKindV1::FinishEffectSelection;
            core.selected_count = *selected_count;
            push_ref(FlatActionRefRoleV1::Source, 0, 0, source)?;
        }
        ActionSemanticV1::ChooseEffectColor {
            actor,
            source,
            color,
        } => {
            check_actor(*actor)?;
            core.kind = FlatActionKindV1::ChooseEffectColor;
            core.color = flat_mana_color_v1(*color);
            push_ref(FlatActionRefRoleV1::Source, 0, 0, source)?;
        }
        ActionSemanticV1::ChooseEffectNumber {
            actor,
            source,
            number,
            minimum,
            maximum,
        } => {
            check_actor(*actor)?;
            if *minimum > *maximum || *number < *minimum || *number > *maximum {
                return Err(FlatActionDecisionSliceErrorV1::InvalidActionRange);
            }
            core.kind = FlatActionKindV1::ChooseEffectNumber;
            core.number = *number;
            core.minimum = *minimum;
            core.maximum = *maximum;
            push_ref(FlatActionRefRoleV1::Source, 0, 0, source)?;
        }
        ActionSemanticV1::ChooseEffectBoolean {
            actor,
            source,
            value,
        } => {
            check_actor(*actor)?;
            core.kind = FlatActionKindV1::ChooseEffectBoolean;
            if *value {
                core.flags |= FLAT_ACTION_FLAG_VALUE_V1;
            }
            push_ref(FlatActionRefRoleV1::Source, 0, 0, source)?;
        }
        ActionSemanticV1::FinishTargetSelection {
            actor,
            source,
            selected_count,
        } => {
            check_actor(*actor)?;
            core.kind = FlatActionKindV1::FinishTargetSelection;
            core.selected_count = *selected_count;
            push_ref(FlatActionRefRoleV1::Source, 0, 0, source)?;
        }
        ActionSemanticV1::ChooseOptionalCostUse { actor, use_cost } => {
            check_actor(*actor)?;
            core.kind = FlatActionKindV1::ChooseOptionalCostUse;
            if *use_cost {
                core.flags |= FLAT_ACTION_FLAG_USE_COST_V1;
            }
        }
        ActionSemanticV1::ChooseOptionalCostWhich { actor, choice } => {
            check_actor(*actor)?;
            core.kind = FlatActionKindV1::ChooseOptionalCostWhich;
            core.optional_cost_choice = flat_optional_cost_choice_v1(*choice);
        }
        ActionSemanticV1::ChooseSpellCopyPayment { actor, source, pay } => {
            check_actor(*actor)?;
            core.kind = FlatActionKindV1::ChooseSpellCopyPayment;
            if *pay {
                core.flags |= FLAT_ACTION_FLAG_PAY_V1;
            }
            push_ref(FlatActionRefRoleV1::Source, 0, 0, source)?;
        }
        ActionSemanticV1::ChooseSpellCopyRetarget {
            actor,
            source,
            change_target,
        } => {
            check_actor(*actor)?;
            core.kind = FlatActionKindV1::ChooseSpellCopyRetarget;
            if *change_target {
                core.flags |= FLAT_ACTION_FLAG_CHANGE_TARGET_V1;
            }
            push_ref(FlatActionRefRoleV1::Source, 0, 0, source)?;
        }
        ActionSemanticV1::ChooseMadnessCast {
            actor,
            card,
            cast_it,
        } => {
            check_actor(*actor)?;
            core.kind = FlatActionKindV1::ChooseMadnessCast;
            if *cast_it {
                core.flags |= FLAT_ACTION_FLAG_CAST_IT_V1;
            }
            push_ref(FlatActionRefRoleV1::Card, 0, 0, card)?;
        }
        ActionSemanticV1::Discard { actor, cards } => {
            check_actor(*actor)?;
            if cards.len() != 1 {
                return Err(FlatActionDecisionSliceErrorV1::InvalidActionRange);
            }
            core.kind = FlatActionKindV1::Discard;
            for (index, card) in cards.iter().enumerate() {
                push_ref(FlatActionRefRoleV1::Cards, index, 0, card)?;
            }
        }
        ActionSemanticV1::ChooseAttackerInclusion {
            actor,
            attacker,
            include,
        } => {
            check_actor(*actor)?;
            core.kind = FlatActionKindV1::ChooseAttackerInclusion;
            if *include {
                core.flags |= FLAT_ACTION_FLAG_INCLUDE_V1;
            }
            push_ref(FlatActionRefRoleV1::Attacker, 0, 0, attacker)?;
        }
        ActionSemanticV1::ChooseBlockerInclusion {
            actor,
            attacker,
            blocker,
            include,
        } => {
            check_actor(*actor)?;
            core.kind = FlatActionKindV1::ChooseBlockerInclusion;
            if *include {
                core.flags |= FLAT_ACTION_FLAG_INCLUDE_V1;
            }
            push_ref(FlatActionRefRoleV1::Attacker, 0, 0, attacker)?;
            push_ref(FlatActionRefRoleV1::Blocker, 0, 0, blocker)?;
        }
        ActionSemanticV1::OrderTriggers {
            actor,
            pending_sources,
            order,
        } => {
            check_actor(*actor)?;
            let count = pending_sources.len();
            if count == 0 || count > FLAT_ACTION_MAX_TRIGGER_ORDER_REFS_V1 || order.len() != count {
                return Err(FlatActionDecisionSliceErrorV1::InvalidTriggerOrder);
            }
            let mut seen = 0_u8;
            for &position in order {
                if position >= count {
                    return Err(FlatActionDecisionSliceErrorV1::InvalidTriggerOrder);
                }
                let bit = 1_u8 << position;
                if seen & bit != 0 {
                    return Err(FlatActionDecisionSliceErrorV1::InvalidTriggerOrder);
                }
                seen |= bit;
            }
            if seen != ((1_u16 << count) - 1) as u8 {
                return Err(FlatActionDecisionSliceErrorV1::InvalidTriggerOrder);
            }
            core.kind = FlatActionKindV1::OrderTriggers;
            for (index, source) in pending_sources.iter().enumerate() {
                push_ref(
                    FlatActionRefRoleV1::PendingSources,
                    index,
                    order[index],
                    source,
                )?;
            }
        }
        ActionSemanticV1::DeclareAttackers { .. }
        | ActionSemanticV1::DeclareBlockersForAttacker { .. }
        | ActionSemanticV1::Ambiguous { .. } => {
            return Err(FlatActionDecisionSliceErrorV1::UnsupportedActionSemantic);
        }
    }
    core.ref_len = u16::try_from(ref_count)
        .map_err(|_| FlatActionDecisionSliceErrorV1::CheckedIntegerRange)?;
    Ok(core)
}

fn flat_stack_action_object_ordinal_v1(
    state: &crate::state::GameState,
    object_id: ObjectId,
    controller: PlayerId,
) -> Result<usize, FlatActionDecisionSliceErrorV1> {
    let mut stack_ordinal = None;
    for (ordinal, item) in state.stack.iter().enumerate() {
        if item.source == object_id && stack_ordinal.replace(ordinal).is_some() {
            return Err(FlatActionDecisionSliceErrorV1::InvalidActionReference);
        }
    }

    let mut detached_matches = 0_u8;
    let mut validate_detached =
        |source: ObjectId, player: PlayerId, spell_resume: Option<(ObjectId, Zone)>| {
            let resume_source = spell_resume.map(|(resume_source, _)| resume_source);
            if source != object_id && resume_source != Some(object_id) {
                return Ok(());
            }
            if source != object_id
                || player != controller
                || !matches!(
                    spell_resume,
                    Some((resume_source, Zone::Graveyard | Zone::Exile))
                        if resume_source == object_id
                )
            {
                return Err(FlatActionDecisionSliceErrorV1::InvalidActionReference);
            }
            detached_matches = detached_matches
                .checked_add(1)
                .ok_or(FlatActionDecisionSliceErrorV1::CheckedIntegerRange)?;
            Ok(())
        };
    if let Some(pending) = &state.engine.pending_optional_cost {
        validate_detached(pending.source, pending.player, pending.spell_resume)?;
    }
    if let Some(pending) = &state.engine.pending_optional_cost_sacrifice {
        validate_detached(pending.source, pending.player, pending.spell_resume)?;
    }

    match (stack_ordinal, detached_matches) {
        (Some(ordinal), 0) => Ok(ordinal),
        (None, 1) => {
            let appears_in_an_ordinary_zone =
                [PlayerId::P0, PlayerId::P1].into_iter().any(|player| {
                    let zones = &state.players[player.index()];
                    zones.library.contains(&object_id)
                        || zones.hand.contains(&object_id)
                        || zones.battlefield.contains(&object_id)
                        || zones.graveyard.contains(&object_id)
                }) || state.exile.contains(&object_id)
                    || state.command.contains(&object_id);
            if appears_in_an_ordinary_zone {
                return Err(FlatActionDecisionSliceErrorV1::InvalidActionReference);
            }
            // A resolving spell is popped from the top before its optional
            // cost suspends resolution. Its prior canonical stack ordinal is
            // therefore exactly the remaining stack length.
            Ok(state.stack.len())
        }
        _ => Err(FlatActionDecisionSliceErrorV1::InvalidActionReference),
    }
}

fn flat_visible_action_object_v1(
    state: &crate::state::GameState,
    actor: PlayerId,
    reference: &CardStableRefV1,
) -> Result<FlatActionObjectV1, FlatActionDecisionSliceErrorV1> {
    let object_id = ObjectId(reference.arena_id);
    let object = state
        .objects
        .try_get(object_id)
        .ok_or(FlatActionDecisionSliceErrorV1::InvalidActionReference)?;
    let owner: PlayerSeatV1 = object.owner.into();
    let controller: PlayerSeatV1 = object.controller.into();
    if object.card_def != reference.card_db_id
        || owner != reference.owner
        || controller != reference.controller
        || object.zone != reference.zone
        || object.zone_change_count != reference.zone_change_count
    {
        return Err(FlatActionDecisionSliceErrorV1::InvalidActionReference);
    }
    let position = |objects: &[ObjectId]| objects.iter().position(|&id| id == object_id);
    let (group, ordinal) = match object.zone {
        Zone::Hand if object.owner == actor => (
            FlatActionObjectGroupV1::SelfHand,
            position(&state.players[actor.index()].hand)
                .ok_or(FlatActionDecisionSliceErrorV1::InvalidActionReference)?,
        ),
        Zone::Hand => {
            if position(&state.players[object.owner.index()].hand).is_none() {
                return Err(FlatActionDecisionSliceErrorV1::InvalidActionReference);
            }
            let ordinal = state.hand_knowledge[actor.index()][object.owner.index()]
                .iter()
                .position(|entry| {
                    entry.object == object_id && entry.zone_change_count == object.zone_change_count
                })
                .ok_or(FlatActionDecisionSliceErrorV1::HiddenActionReference)?;
            (FlatActionObjectGroupV1::KnownOpponentHand, ordinal)
        }
        Zone::Battlefield => {
            let ordinal = position(&state.players[object.controller.index()].battlefield)
                .ok_or(FlatActionDecisionSliceErrorV1::InvalidActionReference)?;
            (
                if object.controller == actor {
                    FlatActionObjectGroupV1::SelfBattlefield
                } else {
                    FlatActionObjectGroupV1::OpponentBattlefield
                },
                ordinal,
            )
        }
        Zone::Graveyard => {
            let ordinal = position(&state.players[object.owner.index()].graveyard)
                .ok_or(FlatActionDecisionSliceErrorV1::InvalidActionReference)?;
            (
                if object.owner == actor {
                    FlatActionObjectGroupV1::SelfGraveyard
                } else {
                    FlatActionObjectGroupV1::OpponentGraveyard
                },
                ordinal,
            )
        }
        Zone::Exile => (
            FlatActionObjectGroupV1::Exile,
            position(&state.exile).ok_or(FlatActionDecisionSliceErrorV1::InvalidActionReference)?,
        ),
        Zone::Stack => (
            FlatActionObjectGroupV1::Stack,
            flat_stack_action_object_ordinal_v1(state, object_id, object.controller)?,
        ),
        Zone::Command => (
            FlatActionObjectGroupV1::Command,
            position(&state.command)
                .ok_or(FlatActionDecisionSliceErrorV1::InvalidActionReference)?,
        ),
        Zone::Library => {
            let knowledge = state.library_knowledge[actor.index()][object.owner.index()]
                .iter()
                .find(|entry| {
                    entry.object == object_id && entry.zone_change_count == object.zone_change_count
                })
                .ok_or(FlatActionDecisionSliceErrorV1::HiddenActionReference)?;
            let library_position = usize::try_from(knowledge.position)
                .map_err(|_| FlatActionDecisionSliceErrorV1::CheckedIntegerRange)?;
            if state.players[object.owner.index()]
                .library
                .get(library_position)
                != Some(&object_id)
            {
                return Err(FlatActionDecisionSliceErrorV1::InvalidActionReference);
            }
            (
                if object.owner == actor {
                    FlatActionObjectGroupV1::KnownSelfLibrary
                } else {
                    FlatActionObjectGroupV1::KnownOpponentLibrary
                },
                library_position,
            )
        }
    };
    Ok(FlatActionObjectV1 {
        card_token: object
            .card_def
            .checked_add(1)
            .ok_or(FlatActionDecisionSliceErrorV1::CheckedIntegerRange)?,
        group,
        actor_visible_ordinal: u16::try_from(ordinal)
            .map_err(|_| FlatActionDecisionSliceErrorV1::CheckedIntegerRange)?,
        owner_relative: flat_relative_seat_v1(owner, actor.into())?,
        controller_relative: flat_relative_seat_v1(controller, actor.into())?,
        zone: flat_zone_v1(object.zone),
        zone_change_count: object.zone_change_count,
    })
}

#[cfg(test)]
fn flat_current_ref_for_object_v1(
    current: &FastActorCurrentDecisionV1,
    state: &crate::state::GameState,
    object_id: ObjectId,
) -> Result<Option<FlatActionObjectV1>, FlatActionDecisionSliceErrorV1> {
    let actor: PlayerSeatV1 = current.actor.into();
    let mut found = None;
    for candidate in &current.candidates {
        flat_action_core_and_refs_v1(&candidate.semantic, actor, 0, |_, _, _, reference| {
            if reference.arena_id == object_id.0 && found.is_none() {
                found = Some(flat_visible_action_object_v1(
                    state,
                    current.actor,
                    reference,
                )?);
            }
            Ok(())
        })?;
        if found.is_some() {
            break;
        }
    }
    Ok(found)
}

fn flat_validate_controller_zone_v1(
    state: &crate::state::GameState,
    acting_player: PlayerId,
    reference: &CardStableRefV1,
    expected_controller: PlayerId,
    expected_zone: Zone,
) -> Result<(), FlatActionDecisionSliceErrorV1> {
    flat_visible_action_object_v1(state, acting_player, reference)?;
    let object = state
        .objects
        .try_get(ObjectId(reference.arena_id))
        .ok_or(FlatActionDecisionSliceErrorV1::InvalidActionReference)?;
    if object.controller != expected_controller || object.zone != expected_zone {
        return Err(FlatActionDecisionSliceErrorV1::InvalidDecisionRelation);
    }
    Ok(())
}

fn flat_validate_owner_controller_zone_v1(
    state: &crate::state::GameState,
    acting_player: PlayerId,
    reference: &CardStableRefV1,
    expected_owner: PlayerId,
    expected_controller: PlayerId,
    expected_zone: Zone,
) -> Result<(), FlatActionDecisionSliceErrorV1> {
    flat_validate_controller_zone_v1(
        state,
        acting_player,
        reference,
        expected_controller,
        expected_zone,
    )?;
    let object = state
        .objects
        .try_get(ObjectId(reference.arena_id))
        .ok_or(FlatActionDecisionSliceErrorV1::InvalidActionReference)?;
    if object.owner != expected_owner {
        return Err(FlatActionDecisionSliceErrorV1::InvalidDecisionRelation);
    }
    Ok(())
}

fn flat_validate_current_decision_relations_v1(
    current: &FastActorCurrentDecisionV1,
    state: &crate::state::GameState,
) -> Result<(), FlatActionDecisionSliceErrorV1> {
    if current.substep_count == 0 || current.substep_index >= current.substep_count {
        return Err(FlatActionDecisionSliceErrorV1::InvalidDecisionRelation);
    }
    let actor: PlayerSeatV1 = current.actor.into();
    match current.decision_kind {
        FastActorDecisionKindV1::Surface => {
            if current.substep_index != 0
                || current.substep_count != 1
                || current.candidates.iter().any(|candidate| {
                    matches!(
                        candidate.semantic,
                        ActionSemanticV1::ChooseAttackerInclusion { .. }
                            | ActionSemanticV1::ChooseBlockerInclusion { .. }
                    )
                })
            {
                return Err(FlatActionDecisionSliceErrorV1::InvalidDecisionRelation);
            }
        }
        FastActorDecisionKindV1::AttackerInclusion => {
            let [first, second] = current.candidates.as_slice() else {
                return Err(FlatActionDecisionSliceErrorV1::InvalidDecisionRelation);
            };
            let (
                ActionSemanticV1::ChooseAttackerInclusion {
                    actor: first_actor,
                    attacker: first_attacker,
                    include: false,
                },
                ActionSemanticV1::ChooseAttackerInclusion {
                    actor: second_actor,
                    attacker: second_attacker,
                    include: true,
                },
            ) = (&first.semantic, &second.semantic)
            else {
                return Err(FlatActionDecisionSliceErrorV1::InvalidDecisionRelation);
            };
            if *first_actor != actor || *second_actor != actor || first_attacker != second_attacker
            {
                return Err(FlatActionDecisionSliceErrorV1::InvalidDecisionRelation);
            }
            flat_validate_controller_zone_v1(
                state,
                current.actor,
                first_attacker,
                current.actor,
                Zone::Battlefield,
            )?;
        }
        FastActorDecisionKindV1::BlockerInclusion => {
            let [first, second] = current.candidates.as_slice() else {
                return Err(FlatActionDecisionSliceErrorV1::InvalidDecisionRelation);
            };
            let (
                ActionSemanticV1::ChooseBlockerInclusion {
                    actor: first_actor,
                    attacker: first_attacker,
                    blocker: first_blocker,
                    include: false,
                },
                ActionSemanticV1::ChooseBlockerInclusion {
                    actor: second_actor,
                    attacker: second_attacker,
                    blocker: second_blocker,
                    include: true,
                },
            ) = (&first.semantic, &second.semantic)
            else {
                return Err(FlatActionDecisionSliceErrorV1::InvalidDecisionRelation);
            };
            if *first_actor != actor
                || *second_actor != actor
                || first_attacker != second_attacker
                || first_blocker != second_blocker
            {
                return Err(FlatActionDecisionSliceErrorV1::InvalidDecisionRelation);
            }
            flat_validate_controller_zone_v1(
                state,
                current.actor,
                first_attacker,
                current.actor.opponent(),
                Zone::Battlefield,
            )?;
            flat_validate_controller_zone_v1(
                state,
                current.actor,
                first_blocker,
                current.actor,
                Zone::Battlefield,
            )?;
        }
    }

    for candidate in &current.candidates {
        match &candidate.semantic {
            ActionSemanticV1::ActivateManaAbility { source, .. }
            | ActionSemanticV1::ActivateAbility { source, .. } => {
                flat_validate_controller_zone_v1(
                    state,
                    current.actor,
                    source,
                    current.actor,
                    Zone::Battlefield,
                )?;
            }
            ActionSemanticV1::PlotSpell { source, .. } => {
                flat_validate_owner_controller_zone_v1(
                    state,
                    current.actor,
                    source,
                    current.actor,
                    current.actor,
                    Zone::Hand,
                )?;
            }
            ActionSemanticV1::Discard { cards, .. } => {
                for card in cards {
                    flat_validate_owner_controller_zone_v1(
                        state,
                        current.actor,
                        card,
                        current.actor,
                        current.actor,
                        Zone::Hand,
                    )?;
                }
            }
            ActionSemanticV1::ChooseMadnessCast { card, .. } => {
                flat_validate_owner_controller_zone_v1(
                    state,
                    current.actor,
                    card,
                    current.actor,
                    current.actor,
                    Zone::Exile,
                )?;
            }
            _ => {}
        }
    }
    Ok(())
}

fn flat_target_matches_v1(reference: &TargetRefV1, target: Target) -> bool {
    match (reference, target) {
        (TargetRefV1::Player { player }, Target::Player(actual)) => {
            flat_player_id_v1(*player) == actual
        }
        (TargetRefV1::Object { object }, Target::Object(actual)) => {
            ObjectId(object.arena_id) == actual
        }
        _ => false,
    }
}

fn flat_ref_matches_object_v1(reference: &CardStableRefV1, object: ObjectId) -> bool {
    ObjectId(reference.arena_id) == object
}

fn flat_trigger_order_rank_v1(order: &[usize]) -> Option<usize> {
    if order.is_empty() || order.len() > FLAT_ACTION_MAX_TRIGGER_ORDER_REFS_V1 {
        return None;
    }
    let mut current = [0_usize; FLAT_ACTION_MAX_TRIGGER_ORDER_REFS_V1];
    for (index, slot) in current[..order.len()].iter_mut().enumerate() {
        *slot = index;
    }
    let mut rank = 0_usize;
    for start in 0..order.len() {
        let branch = current[start..order.len()]
            .iter()
            .position(|value| *value == order[start])?;
        let subtree_size = (1..(order.len() - start)).product::<usize>();
        rank = rank.checked_add(branch.checked_mul(subtree_size)?)?;
        current.swap(start, start + branch);
    }
    Some(rank)
}

/// Validates every candidate field against the exact private decision that
/// produced it. This closes the gap where an executable `Action` omits
/// contextual fields such as source, remaining count, option count, or the
/// Madness card. The check intentionally does not rebuild the allocating
/// policy-v5 candidate vector.
fn flat_validate_origin_decision_v1(
    current: &FastActorCurrentDecisionV1,
    state: &crate::state::GameState,
) -> Result<(), FlatActionDecisionSliceErrorV1> {
    use crate::engine::Action;
    use crate::surface::SurfaceAction;

    let invalid = || FlatActionDecisionSliceErrorV1::InvalidDecisionRelation;
    match &current.origin_decision {
        PolicyDecisionV5::AttackerInclusion {
            player,
            attacker,
            candidate_index,
            candidate_count,
        } => {
            if current.decision_kind != FastActorDecisionKindV1::AttackerInclusion
                || current.actor != *player
                || current.substep_index != *candidate_index
                || current.substep_count != *candidate_count
            {
                return Err(invalid());
            }
            for (index, candidate) in current.candidates.iter().enumerate() {
                let include = index == 1;
                if !matches!(
                    (&candidate.semantic, &candidate.policy_action),
                    (
                        ActionSemanticV1::ChooseAttackerInclusion {
                            actor,
                            attacker: semantic_attacker,
                            include: semantic_include,
                        },
                        PolicyActionV5::ChooseAttackerInclusion {
                            actor: action_actor,
                            attacker: action_attacker,
                            include: action_include,
                        }
                    ) if flat_player_id_v1(*actor) == *player
                        && flat_ref_matches_object_v1(semantic_attacker, *attacker)
                        && *semantic_include == include
                        && *action_actor == *player
                        && *action_attacker == *attacker
                        && *action_include == include
                ) {
                    return Err(invalid());
                }
            }
            return Ok(());
        }
        PolicyDecisionV5::BlockerInclusion {
            player,
            attacker,
            blocker,
            candidate_index,
            candidate_count,
        } => {
            if current.decision_kind != FastActorDecisionKindV1::BlockerInclusion
                || current.actor != *player
                || current.substep_index != *candidate_index
                || current.substep_count != *candidate_count
            {
                return Err(invalid());
            }
            for (index, candidate) in current.candidates.iter().enumerate() {
                let include = index == 1;
                if !matches!(
                    (&candidate.semantic, &candidate.policy_action),
                    (
                        ActionSemanticV1::ChooseBlockerInclusion {
                            actor,
                            attacker: semantic_attacker,
                            blocker: semantic_blocker,
                            include: semantic_include,
                        },
                        PolicyActionV5::ChooseBlockerInclusion {
                            actor: action_actor,
                            attacker: action_attacker,
                            blocker: action_blocker,
                            include: action_include,
                        }
                    ) if flat_player_id_v1(*actor) == *player
                        && flat_ref_matches_object_v1(semantic_attacker, *attacker)
                        && flat_ref_matches_object_v1(semantic_blocker, *blocker)
                        && *semantic_include == include
                        && *action_actor == *player
                        && *action_attacker == *attacker
                        && *action_blocker == *blocker
                        && *action_include == include
                ) {
                    return Err(invalid());
                }
            }
            return Ok(());
        }
        PolicyDecisionV5::Surface(_) => {
            if current.decision_kind != FastActorDecisionKindV1::Surface
                || current.substep_index != 0
                || current.substep_count != 1
            {
                return Err(invalid());
            }
        }
    }

    let PolicyDecisionV5::Surface(origin) = &current.origin_decision else {
        unreachable!("inclusion decisions returned above")
    };
    let SurfaceDecision::Decision(origin) = origin else {
        return Err(FlatActionDecisionSliceErrorV1::UnsupportedActionSemantic);
    };
    let actor_matches = |actor: PlayerSeatV1, player: PlayerId| flat_player_id_v1(actor) == player;
    let candidates = current.candidates.as_slice();

    match origin {
        Decision::CastSpellOrPass {
            player,
            castable_spells,
            mana_abilities,
            land_drops,
            activatable_abilities,
            plot_actions,
        } => {
            let expected_count = castable_spells
                .len()
                .checked_add(mana_abilities.len())
                .and_then(|count| count.checked_add(land_drops.len()))
                .and_then(|count| count.checked_add(activatable_abilities.len()))
                .and_then(|count| count.checked_add(plot_actions.len()))
                .and_then(|count| count.checked_add(1))
                .ok_or_else(invalid)?;
            if current.actor != *player || candidates.len() != expected_count {
                return Err(invalid());
            }
            let mut cursor = 0_usize;
            for object in castable_spells {
                let candidate = &candidates[cursor];
                if !matches!(
                    (&candidate.semantic, &candidate.policy_action),
                    (
                        ActionSemanticV1::CastSpell { actor, source },
                        PolicyActionV5::Surface(SurfaceAction::Action(Action::CastSpell(action)))
                    ) if actor_matches(*actor, *player)
                        && flat_ref_matches_object_v1(source, *object)
                        && action == object
                ) {
                    return Err(invalid());
                }
                cursor += 1;
            }
            for object in mana_abilities {
                let state_object = state.objects.try_get(*object).ok_or_else(invalid)?;
                let choices = crate::card_def::CARD_DEFS
                    .get(usize::from(state_object.card_def))
                    .ok_or_else(invalid)?
                    .produces_mana;
                let expected_choice = (choices.len() == 1).then_some(choices[0]);
                let candidate = &candidates[cursor];
                if !matches!(
                    (&candidate.semantic, &candidate.policy_action),
                    (
                        ActionSemanticV1::ActivateManaAbility {
                            actor,
                            source,
                            mana_choice,
                        },
                        PolicyActionV5::Surface(SurfaceAction::Action(
                            Action::ActivateManaAbility(action)
                        ))
                    ) if actor_matches(*actor, *player)
                        && flat_ref_matches_object_v1(source, *object)
                        && *mana_choice == expected_choice
                        && action == object
                ) {
                    return Err(invalid());
                }
                cursor += 1;
            }
            for object in land_drops {
                let candidate = &candidates[cursor];
                if !matches!(
                    (&candidate.semantic, &candidate.policy_action),
                    (
                        ActionSemanticV1::PlayLand { actor, source },
                        PolicyActionV5::Surface(SurfaceAction::Action(Action::PlayLand(action)))
                    ) if actor_matches(*actor, *player)
                        && flat_ref_matches_object_v1(source, *object)
                        && action == object
                ) {
                    return Err(invalid());
                }
                cursor += 1;
            }
            for (object, expected_ability_index) in activatable_abilities {
                let candidate = &candidates[cursor];
                if !matches!(
                    (&candidate.semantic, &candidate.policy_action),
                    (
                        ActionSemanticV1::ActivateAbility {
                            actor,
                            source,
                            ability_index,
                        },
                        PolicyActionV5::Surface(SurfaceAction::Action(Action::ActivateAbility(
                            action,
                            action_ability_index,
                        )))
                    ) if actor_matches(*actor, *player)
                        && flat_ref_matches_object_v1(source, *object)
                        && ability_index == expected_ability_index
                        && action == object
                        && action_ability_index == expected_ability_index
                ) {
                    return Err(invalid());
                }
                cursor += 1;
            }
            for object in plot_actions {
                let candidate = &candidates[cursor];
                if !matches!(
                    (&candidate.semantic, &candidate.policy_action),
                    (
                        ActionSemanticV1::PlotSpell { actor, source },
                        PolicyActionV5::Surface(SurfaceAction::Action(Action::PlotSpell(action)))
                    ) if actor_matches(*actor, *player)
                        && flat_ref_matches_object_v1(source, *object)
                        && action == object
                ) {
                    return Err(invalid());
                }
                cursor += 1;
            }
            if !matches!(
                (&candidates[cursor].semantic, &candidates[cursor].policy_action),
                (
                    ActionSemanticV1::Pass { actor },
                    PolicyActionV5::Surface(SurfaceAction::Action(Action::Pass))
                ) if actor_matches(*actor, *player)
            ) {
                return Err(invalid());
            }
        }
        Decision::ChooseTargets {
            player,
            spell,
            remaining,
            legal_targets,
        } => {
            if current.actor != *player || candidates.len() != legal_targets.len() {
                return Err(invalid());
            }
            for (candidate, target) in candidates.iter().zip(legal_targets) {
                if !matches!(
                    &candidate.semantic,
                    ActionSemanticV1::ChooseTarget {
                        actor,
                        source,
                        remaining: semantic_remaining,
                        target: semantic_target,
                    } if actor_matches(*actor, *player)
                        && flat_ref_matches_object_v1(source, *spell)
                        && semantic_remaining == remaining
                        && flat_target_matches_v1(semantic_target, *target)
                ) {
                    return Err(invalid());
                }
            }
        }
        Decision::ChooseCostTargets {
            player,
            source,
            cost_kind,
            remaining,
            candidates: origin_candidates,
        } => {
            if current.actor != *player || candidates.len() != origin_candidates.len() {
                return Err(invalid());
            }
            for (candidate, expected_candidate) in candidates.iter().zip(origin_candidates) {
                if !matches!(
                    &candidate.semantic,
                    ActionSemanticV1::ChooseCostTarget {
                        actor,
                        source: semantic_source,
                        cost_kind: semantic_cost_kind,
                        remaining: semantic_remaining,
                        candidate: semantic_candidate,
                    } if actor_matches(*actor, *player)
                        && flat_ref_matches_object_v1(semantic_source, *source)
                        && semantic_cost_kind == cost_kind
                        && semantic_remaining == remaining
                        && flat_ref_matches_object_v1(semantic_candidate, *expected_candidate)
                ) {
                    return Err(invalid());
                }
            }
        }
        Decision::ChooseCastMode {
            player,
            spell,
            options,
        } => {
            if current.actor != *player || candidates.len() != options.len() {
                return Err(invalid());
            }
            for (candidate, expected_mode) in candidates.iter().zip(options) {
                if !matches!(
                    &candidate.semantic,
                    ActionSemanticV1::ChooseCastMode {
                        actor,
                        source,
                        mode,
                    } if actor_matches(*actor, *player)
                        && flat_ref_matches_object_v1(source, *spell)
                        && mode == expected_mode
                ) {
                    return Err(invalid());
                }
            }
        }
        Decision::ChooseKicker { player, spell } => {
            if current.actor != *player || candidates.len() != 2 {
                return Err(invalid());
            }
            for (index, candidate) in candidates.iter().enumerate() {
                if !matches!(
                    &candidate.semantic,
                    ActionSemanticV1::ChooseKicker { actor, source, pay }
                        if actor_matches(*actor, *player)
                            && flat_ref_matches_object_v1(source, *spell)
                            && *pay == (index == 1)
                ) {
                    return Err(invalid());
                }
            }
        }
        Decision::ChooseSpellMode {
            player,
            spell,
            mode_count,
        } => {
            if current.actor != *player || candidates.len() != usize::from(*mode_count) {
                return Err(invalid());
            }
            for (index, candidate) in candidates.iter().enumerate() {
                if !matches!(
                    &candidate.semantic,
                    ActionSemanticV1::ChooseSpellMode {
                        actor,
                        source,
                        mode_index,
                        mode_count: semantic_mode_count,
                    } if actor_matches(*actor, *player)
                        && flat_ref_matches_object_v1(source, *spell)
                        && usize::from(*mode_index) == index
                        && semantic_mode_count == mode_count
                ) {
                    return Err(invalid());
                }
            }
        }
        Decision::ChooseEffectOption {
            player,
            source,
            option_count,
        } => {
            if current.actor != *player || candidates.len() != usize::from(*option_count) {
                return Err(invalid());
            }
            for (index, candidate) in candidates.iter().enumerate() {
                if !matches!(
                    &candidate.semantic,
                    ActionSemanticV1::ChooseEffectOption {
                        actor,
                        source: semantic_source,
                        option_index,
                        option_count: semantic_option_count,
                    } if actor_matches(*actor, *player)
                        && flat_ref_matches_object_v1(semantic_source, *source)
                        && usize::from(*option_index) == index
                        && semantic_option_count == option_count
                ) {
                    return Err(invalid());
                }
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
            let expected_count = legal_targets.len() + usize::from(*can_finish);
            if current.actor != *player || candidates.len() != expected_count {
                return Err(invalid());
            }
            for (candidate, expected_target) in
                candidates[..legal_targets.len()].iter().zip(legal_targets)
            {
                if !matches!(
                    &candidate.semantic,
                    ActionSemanticV1::ChooseEffectTarget {
                        actor,
                        source: semantic_source,
                        target,
                        selected_count: semantic_selected_count,
                        min_targets: semantic_min_targets,
                        max_targets: semantic_max_targets,
                    } if actor_matches(*actor, *player)
                        && flat_ref_matches_object_v1(semantic_source, *source)
                        && flat_target_matches_v1(target, *expected_target)
                        && semantic_selected_count == selected_count
                        && semantic_min_targets == min_targets
                        && semantic_max_targets == max_targets
                ) {
                    return Err(invalid());
                }
            }
            if *can_finish
                && !matches!(
                    &candidates[legal_targets.len()].semantic,
                    ActionSemanticV1::FinishEffectSelection {
                        actor,
                        source: semantic_source,
                        selected_count: semantic_selected_count,
                    } if actor_matches(*actor, *player)
                        && flat_ref_matches_object_v1(semantic_source, *source)
                        && semantic_selected_count == selected_count
                )
            {
                return Err(invalid());
            }
        }
        Decision::ChooseEffectBoolean { player, source, .. } => {
            if current.actor != *player || candidates.len() != 2 {
                return Err(invalid());
            }
            for (index, candidate) in candidates.iter().enumerate() {
                if !matches!(
                    &candidate.semantic,
                    ActionSemanticV1::ChooseEffectBoolean {
                        actor,
                        source: semantic_source,
                        value,
                    } if actor_matches(*actor, *player)
                        && flat_ref_matches_object_v1(semantic_source, *source)
                        && *value == (index == 1)
                ) {
                    return Err(invalid());
                }
            }
        }
        Decision::ChooseOptionalCost {
            player,
            discard_payable,
            sacrifice_payable,
        } => {
            if current.actor != *player || candidates.len() != 2 {
                return Err(invalid());
            }
            let valid = match (*discard_payable, *sacrifice_payable) {
                (false, false) => candidates.iter().enumerate().all(|(index, candidate)| {
                    matches!(
                        candidate.semantic,
                        ActionSemanticV1::ChooseOptionalCostUse { actor, use_cost }
                            if actor_matches(actor, *player) && use_cost == (index == 1)
                    )
                }),
                (true, true) => matches!(
                    (&candidates[0].semantic, &candidates[1].semantic),
                    (
                        ActionSemanticV1::ChooseOptionalCostWhich {
                            actor: first_actor,
                            choice: OptionalCostChoice::Discard,
                        },
                        ActionSemanticV1::ChooseOptionalCostWhich {
                            actor: second_actor,
                            choice: OptionalCostChoice::SacrificeLand,
                        }
                    ) if actor_matches(*first_actor, *player)
                        && actor_matches(*second_actor, *player)
                ),
                _ => false,
            };
            if !valid {
                return Err(invalid());
            }
        }
        Decision::ChooseSpellCopyPayment { player, spell } => {
            if current.actor != *player || candidates.len() != 2 {
                return Err(invalid());
            }
            for (index, candidate) in candidates.iter().enumerate() {
                if !matches!(
                    &candidate.semantic,
                    ActionSemanticV1::ChooseSpellCopyPayment { actor, source, pay }
                        if actor_matches(*actor, *player)
                            && flat_ref_matches_object_v1(source, *spell)
                            && *pay == (index == 0)
                ) {
                    return Err(invalid());
                }
            }
        }
        Decision::ChooseSpellCopyRetarget { player, copy } => {
            if current.actor != *player || candidates.len() != 2 {
                return Err(invalid());
            }
            for (index, candidate) in candidates.iter().enumerate() {
                if !matches!(
                    &candidate.semantic,
                    ActionSemanticV1::ChooseSpellCopyRetarget {
                        actor,
                        source,
                        change_target,
                    } if actor_matches(*actor, *player)
                        && flat_ref_matches_object_v1(source, *copy)
                        && *change_target == (index == 0)
                ) {
                    return Err(invalid());
                }
            }
        }
        Decision::ChooseMadnessCast { player, card } => {
            if current.actor != *player || candidates.len() != 2 {
                return Err(invalid());
            }
            for (index, candidate) in candidates.iter().enumerate() {
                if !matches!(
                    &candidate.semantic,
                    ActionSemanticV1::ChooseMadnessCast {
                        actor,
                        card: semantic_card,
                        cast_it,
                    } if actor_matches(*actor, *player)
                        && flat_ref_matches_object_v1(semantic_card, *card)
                        && *cast_it == (index == 1)
                ) {
                    return Err(invalid());
                }
            }
        }
        Decision::Discard {
            player,
            count,
            choices,
        } => {
            if current.actor != *player || *count != 1 || candidates.len() != choices.len() {
                return Err(invalid());
            }
            for (candidate, expected_card) in candidates.iter().zip(choices) {
                if !matches!(
                    &candidate.semantic,
                    ActionSemanticV1::Discard { actor, cards }
                        if actor_matches(*actor, *player)
                            && cards.len() == 1
                            && flat_ref_matches_object_v1(&cards[0], *expected_card)
                ) {
                    return Err(invalid());
                }
            }
        }
        Decision::OrderTriggers { player, pending } => {
            let expected_count = (1..=pending.len()).product::<usize>();
            if current.actor != *player
                || pending.is_empty()
                || pending.len() > FLAT_ACTION_MAX_TRIGGER_ORDER_REFS_V1
                || candidates.len() != expected_count
            {
                return Err(invalid());
            }
            for (candidate_index, candidate) in candidates.iter().enumerate() {
                let ActionSemanticV1::OrderTriggers {
                    actor,
                    pending_sources,
                    order,
                } = &candidate.semantic
                else {
                    return Err(invalid());
                };
                if !actor_matches(*actor, *player)
                    || pending_sources.len() != pending.len()
                    || pending_sources
                        .iter()
                        .zip(pending)
                        .any(|(reference, trigger)| {
                            !flat_ref_matches_object_v1(reference, trigger.source)
                        })
                    || flat_trigger_order_rank_v1(order) != Some(candidate_index)
                {
                    return Err(invalid());
                }
            }
        }
        Decision::DeclareAttackers { .. }
        | Decision::DeclareBlockers { .. }
        | Decision::GameOver { .. }
        | Decision::Halted { .. } => {
            return Err(FlatActionDecisionSliceErrorV1::UnsupportedActionSemantic);
        }
    }
    Ok(())
}

fn flat_validate_semantic_policy_pair_v1(
    candidate: &CorePolicyActionCandidateV1,
) -> Result<(), FlatActionDecisionSliceErrorV1> {
    use crate::engine::Action;
    use crate::surface::SurfaceAction;

    let paired = match (&candidate.semantic, &candidate.policy_action) {
        (
            ActionSemanticV1::Pass { .. },
            PolicyActionV5::Surface(SurfaceAction::Action(Action::Pass)),
        ) => true,
        (
            ActionSemanticV1::PlayLand { source, .. },
            PolicyActionV5::Surface(SurfaceAction::Action(Action::PlayLand(actual))),
        )
        | (
            ActionSemanticV1::CastSpell { source, .. },
            PolicyActionV5::Surface(SurfaceAction::Action(Action::CastSpell(actual))),
        )
        | (
            ActionSemanticV1::ActivateManaAbility { source, .. },
            PolicyActionV5::Surface(SurfaceAction::Action(Action::ActivateManaAbility(actual))),
        )
        | (
            ActionSemanticV1::PlotSpell { source, .. },
            PolicyActionV5::Surface(SurfaceAction::Action(Action::PlotSpell(actual))),
        ) => ObjectId(source.arena_id) == *actual,
        (
            ActionSemanticV1::ActivateAbility {
                source,
                ability_index,
                ..
            },
            PolicyActionV5::Surface(SurfaceAction::Action(Action::ActivateAbility(
                actual,
                actual_index,
            ))),
        ) => ObjectId(source.arena_id) == *actual && ability_index == actual_index,
        (
            ActionSemanticV1::ChooseTarget { target, .. },
            PolicyActionV5::Surface(SurfaceAction::Action(Action::ChooseTarget(actual))),
        ) => flat_target_matches_v1(target, *actual),
        (
            ActionSemanticV1::ChooseCostTarget { candidate, .. },
            PolicyActionV5::Surface(SurfaceAction::Action(Action::ChooseCostTarget(actual))),
        ) => ObjectId(candidate.arena_id) == *actual,
        (
            ActionSemanticV1::ChooseCastMode { mode, .. },
            PolicyActionV5::Surface(SurfaceAction::Action(Action::ChooseCastMode(actual))),
        ) => mode == actual,
        (
            ActionSemanticV1::ChooseKicker { pay, .. },
            PolicyActionV5::Surface(SurfaceAction::Action(Action::ChooseKicker(actual))),
        ) => pay == actual,
        (
            ActionSemanticV1::ChooseSpellMode { mode_index, .. },
            PolicyActionV5::Surface(SurfaceAction::Action(Action::ChooseSpellMode(actual))),
        ) => mode_index == actual,
        (
            ActionSemanticV1::ChooseEffectOption { option_index, .. },
            PolicyActionV5::Surface(SurfaceAction::Action(Action::ChooseEffectOption(actual))),
        ) => option_index == actual,
        (
            ActionSemanticV1::ChooseEffectTarget { target, .. },
            PolicyActionV5::Surface(SurfaceAction::Action(Action::ChooseEffectTarget(actual))),
        ) => flat_target_matches_v1(target, *actual),
        (
            ActionSemanticV1::FinishEffectSelection { .. },
            PolicyActionV5::Surface(SurfaceAction::Action(Action::FinishEffectSelection)),
        ) => true,
        (
            ActionSemanticV1::ChooseEffectBoolean { value, .. },
            PolicyActionV5::Surface(SurfaceAction::Action(Action::ChooseEffectBoolean(actual))),
        ) => value == actual,
        (
            ActionSemanticV1::ChooseOptionalCostUse { use_cost, .. },
            PolicyActionV5::Surface(SurfaceAction::Action(Action::ChooseOptionalCostStage(actual))),
        ) => use_cost == actual,
        (
            ActionSemanticV1::ChooseOptionalCostWhich { choice, .. },
            PolicyActionV5::Surface(SurfaceAction::Action(Action::ChooseOptionalCostStage(actual))),
        ) => match choice {
            OptionalCostChoice::Discard => *actual,
            OptionalCostChoice::SacrificeLand => !*actual,
            OptionalCostChoice::Decline => false,
        },
        (
            ActionSemanticV1::ChooseSpellCopyPayment { pay, .. },
            PolicyActionV5::Surface(SurfaceAction::Action(Action::ChooseSpellCopyPayment(actual))),
        ) => pay == actual,
        (
            ActionSemanticV1::ChooseSpellCopyRetarget { change_target, .. },
            PolicyActionV5::Surface(SurfaceAction::Action(Action::ChooseSpellCopyRetarget(actual))),
        ) => change_target == actual,
        (
            ActionSemanticV1::ChooseMadnessCast { cast_it, .. },
            PolicyActionV5::Surface(SurfaceAction::Action(Action::ChooseMadnessCast(actual))),
        ) => cast_it == actual,
        (
            ActionSemanticV1::Discard { cards, .. },
            PolicyActionV5::Surface(SurfaceAction::Action(Action::Discard(actual))),
        ) => {
            cards.len() == actual.len()
                && cards
                    .iter()
                    .zip(actual)
                    .all(|(card, actual)| ObjectId(card.arena_id) == *actual)
        }
        (
            ActionSemanticV1::ChooseAttackerInclusion {
                actor,
                attacker,
                include,
            },
            PolicyActionV5::ChooseAttackerInclusion {
                actor: actual_actor,
                attacker: actual_attacker,
                include: actual_include,
            },
        ) => {
            flat_player_id_v1(*actor) == *actual_actor
                && ObjectId(attacker.arena_id) == *actual_attacker
                && include == actual_include
        }
        (
            ActionSemanticV1::ChooseBlockerInclusion {
                actor,
                attacker,
                blocker,
                include,
            },
            PolicyActionV5::ChooseBlockerInclusion {
                actor: actual_actor,
                attacker: actual_attacker,
                blocker: actual_blocker,
                include: actual_include,
            },
        ) => {
            flat_player_id_v1(*actor) == *actual_actor
                && ObjectId(attacker.arena_id) == *actual_attacker
                && ObjectId(blocker.arena_id) == *actual_blocker
                && include == actual_include
        }
        (
            ActionSemanticV1::OrderTriggers { order, .. },
            PolicyActionV5::Surface(SurfaceAction::Action(Action::OrderTriggers(actual))),
        ) => order == actual,
        (
            ActionSemanticV1::ChooseEffectColor { .. }
            | ActionSemanticV1::ChooseEffectNumber { .. }
            | ActionSemanticV1::FinishTargetSelection { .. },
            _,
        ) => return Err(FlatActionDecisionSliceErrorV1::UnsupportedActionSemantic),
        (
            ActionSemanticV1::DeclareAttackers { .. }
            | ActionSemanticV1::DeclareBlockersForAttacker { .. }
            | ActionSemanticV1::Ambiguous { .. },
            _,
        ) => return Err(FlatActionDecisionSliceErrorV1::UnsupportedActionSemantic),
        _ => false,
    };
    if paired {
        Ok(())
    } else {
        Err(FlatActionDecisionSliceErrorV1::InvalidDecisionRelation)
    }
}

#[cfg(test)]
#[derive(Debug, Clone, Copy)]
struct FlatActionPreflightV1 {
    binding: FlatActionDecisionBindingV1,
    action_count: usize,
    ref_count: usize,
    object_count: usize,
}

/// Session-private materialization of the exact public v1 rows and binding.
///
/// This cache is deliberately not exposed as a reusable caller token. It is
/// cloned with snapshots and replaced with every new decision. Encode and
/// consume validate the live session-owned semantics and referenced object
/// meanings against these rows before trusting the cached commitment.
#[derive(Debug, Clone, PartialEq, Eq)]
struct FlatActionDecisionCacheV1 {
    binding: FlatActionDecisionBindingV1,
    actions: Vec<FlatActionCoreV1>,
    refs: Vec<FlatActionRefV1>,
    objects: Vec<FlatActionObjectV1>,
    scratch_unindexed_refs: Vec<FlatUnindexedActionRefV1>,
    scratch_resolved_objects: Vec<FlatResolvedActionObjectV1>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FlatUnindexedActionRefV1 {
    action_index: u32,
    role: FlatActionRefRoleV1,
    order_index: u16,
    associated_order: u16,
    object: FlatActionObjectV1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FlatResolvedActionObjectV1 {
    arena_id: u32,
    object: FlatActionObjectV1,
}

struct FlatActionCommitmentHasherV1(Sha256);

impl FlatActionCommitmentHasherV1 {
    fn new(actor: PlayerSeatV1, action_count: u32, ref_count: u32, object_count: u16) -> Self {
        #[cfg(test)]
        TEST_FLAT_ACTION_COMMITMENT_CONSTRUCTIONS
            .with(|calls| calls.set(calls.get().saturating_add(1)));
        let mut hash = Sha256::new();
        hash.update(b"mtg-kernel-flat-action-candidate-order-v1\0");
        hash.update(FLAT_ACTION_DECISION_SLICE_VERSION_V1.to_le_bytes());
        hash.update(FLAT_ACTION_REF_ROLE_MAPPING_VERSION_V1.to_le_bytes());
        hash.update(FLAT_ACTION_CARD_TOKEN_MAPPING_VERSION_V1.to_le_bytes());
        hash.update(FLAT_ACTION_CANDIDATE_COMMITMENT_VERSION_V1.to_le_bytes());
        hash.update(KERNEL_CARDDB_HASH.to_le_bytes());
        hash.update([match actor {
            PlayerSeatV1::P0 => 0,
            PlayerSeatV1::P1 => 1,
        }]);
        hash.update(action_count.to_le_bytes());
        hash.update(ref_count.to_le_bytes());
        hash.update(object_count.to_le_bytes());
        Self(hash)
    }

    fn update_object(&mut self, object_index: u16, object: FlatActionObjectV1) {
        self.0.update(b"O");
        self.0.update(object_index.to_le_bytes());
        self.0.update(object.card_token.to_le_bytes());
        self.0.update([object.group as u8]);
        self.0.update(object.actor_visible_ordinal.to_le_bytes());
        self.0.update([object.owner_relative]);
        self.0.update([object.controller_relative]);
        self.0.update([object.zone]);
        self.0.update(object.zone_change_count.to_le_bytes());
    }

    fn update_action(&mut self, action_index: u32, action: FlatActionCoreV1) {
        self.0.update(b"A");
        self.0.update(action_index.to_le_bytes());
        self.0.update([flat_action_kind_id_v1(action.kind)]);
        self.0.update(action.flags.to_le_bytes());
        self.0.update([action.ability_index]);
        self.0.update([action.remaining]);
        self.0.update([action.mode_index]);
        self.0.update([action.mode_count]);
        self.0.update(action.option_index.to_le_bytes());
        self.0.update(action.option_count.to_le_bytes());
        self.0.update(action.selected_count.to_le_bytes());
        self.0.update(action.min_targets.to_le_bytes());
        self.0.update(action.max_targets.to_le_bytes());
        self.0.update(action.number.to_le_bytes());
        self.0.update(action.minimum.to_le_bytes());
        self.0.update(action.maximum.to_le_bytes());
        self.0.update([action.mana_choice]);
        self.0.update([action.color]);
        self.0.update([action.cast_mode]);
        self.0.update([action.cost_kind]);
        self.0.update([action.optional_cost_choice]);
        self.0.update([action.target_kind]);
        self.0.update([action.target_player]);
        self.0.update(action.ref_start.to_le_bytes());
        self.0.update(action.ref_len.to_le_bytes());
    }

    fn update_ref(&mut self, reference: FlatActionRefV1, object: FlatActionObjectV1) {
        self.0.update(b"R");
        self.0.update(reference.action_index.to_le_bytes());
        self.0.update([flat_action_ref_role_id_v1(reference.role)]);
        self.0.update(reference.order_index.to_le_bytes());
        self.0.update(reference.associated_order.to_le_bytes());
        self.0.update(reference.card_token.to_le_bytes());
        self.0.update(reference.object_index.to_le_bytes());
        // Bind the meaning of object_index, not only its integer slot.
        self.0.update(object.card_token.to_le_bytes());
        self.0.update([object.group as u8]);
        self.0.update(object.actor_visible_ordinal.to_le_bytes());
        self.0.update([object.owner_relative]);
        self.0.update([object.controller_relative]);
        self.0.update([object.zone]);
        self.0.update(object.zone_change_count.to_le_bytes());
    }

    fn finish(self) -> [u8; 16] {
        let digest = self.0.finalize();
        let mut commitment = [0_u8; 16];
        commitment.copy_from_slice(&digest[..16]);
        commitment
    }
}

#[cfg(test)]
fn flat_canonical_object_index_v1(
    current: &FastActorCurrentDecisionV1,
    state: &crate::state::GameState,
    object: FlatActionObjectV1,
) -> Result<u16, FlatActionDecisionSliceErrorV1> {
    let key = object.canonical_key();
    let mut lower_count = 0_usize;
    let mut exact_count = 0_usize;
    for (object_id, _) in state.objects.iter() {
        let Some(candidate) = flat_current_ref_for_object_v1(current, state, object_id)? else {
            continue;
        };
        match candidate.canonical_key().cmp(&key) {
            std::cmp::Ordering::Less => {
                lower_count = lower_count
                    .checked_add(1)
                    .ok_or(FlatActionDecisionSliceErrorV1::CheckedIntegerRange)?;
            }
            std::cmp::Ordering::Equal => {
                exact_count = exact_count
                    .checked_add(1)
                    .ok_or(FlatActionDecisionSliceErrorV1::CheckedIntegerRange)?;
            }
            std::cmp::Ordering::Greater => {}
        }
    }
    if exact_count != 1 {
        return Err(FlatActionDecisionSliceErrorV1::DuplicateCanonicalObject);
    }
    u16::try_from(lower_count).map_err(|_| FlatActionDecisionSliceErrorV1::CheckedIntegerRange)
}

#[cfg(test)]
fn flat_for_each_canonical_object_v1<F>(
    current: &FastActorCurrentDecisionV1,
    state: &crate::state::GameState,
    object_count: usize,
    mut visit: F,
) -> Result<(), FlatActionDecisionSliceErrorV1>
where
    F: FnMut(u16, FlatActionObjectV1) -> Result<(), FlatActionDecisionSliceErrorV1>,
{
    for expected_index in 0..object_count {
        let mut selected = None;
        for (object_id, _) in state.objects.iter() {
            let Some(candidate) = flat_current_ref_for_object_v1(current, state, object_id)? else {
                continue;
            };
            if usize::from(flat_canonical_object_index_v1(current, state, candidate)?)
                == expected_index
                && selected.replace(candidate).is_some()
            {
                return Err(FlatActionDecisionSliceErrorV1::DuplicateCanonicalObject);
            }
        }
        let selected = selected.ok_or(FlatActionDecisionSliceErrorV1::InvalidActionReference)?;
        visit(
            u16::try_from(expected_index)
                .map_err(|_| FlatActionDecisionSliceErrorV1::CheckedIntegerRange)?,
            selected,
        )?;
    }
    Ok(())
}

fn flat_validate_current_binding_header_v1(
    session: &FastActorSessionV1,
    current: &FastActorCurrentDecisionV1,
) -> Result<(), FlatActionDecisionSliceErrorV1> {
    if current.environment_revision != session.environment_revision
        || current.bound_policy_step_count != session.policy_step_count
        || current.bound_physical_decision_count != session.physical_decision_count
    {
        return Err(FlatActionDecisionSliceErrorV1::CorruptCurrentBinding);
    }
    if current.candidates.is_empty() {
        return Err(FlatActionDecisionSliceErrorV1::InvalidDecisionRelation);
    }
    flat_validate_current_decision_relations_v1(current, &session.state)
}

fn flat_action_binding_v1(
    session: &FastActorSessionV1,
    current: &FastActorCurrentDecisionV1,
    legal_action_count: u32,
    candidate_order_commitment: [u8; 16],
) -> FlatActionDecisionBindingV1 {
    FlatActionDecisionBindingV1 {
        slice_version: FLAT_ACTION_DECISION_SLICE_VERSION_V1,
        ref_role_mapping_version: FLAT_ACTION_REF_ROLE_MAPPING_VERSION_V1,
        card_token_mapping_version: FLAT_ACTION_CARD_TOKEN_MAPPING_VERSION_V1,
        candidate_commitment_version: FLAT_ACTION_CANDIDATE_COMMITMENT_VERSION_V1,
        card_db_hash: KERNEL_CARDDB_HASH,
        episode_id: session.episode_id,
        environment_revision: session.environment_revision,
        bound_policy_step_count: current.bound_policy_step_count,
        physical_decision_id: current.physical_decision_id,
        bound_physical_decision_count: current.bound_physical_decision_count,
        substep_index: current.substep_index,
        substep_count: current.substep_count,
        acting_player: current.actor.0,
        decision_kind: flat_decision_kind_id_v1(current.decision_kind),
        legal_action_count,
        candidate_order_commitment,
    }
}

fn flat_action_commitment_from_rows_v1(
    actor: PlayerSeatV1,
    actions: &[FlatActionCoreV1],
    refs: &[FlatActionRefV1],
    objects: &[FlatActionObjectV1],
) -> Result<[u8; 16], FlatActionDecisionSliceErrorV1> {
    let action_count = u32::try_from(actions.len())
        .map_err(|_| FlatActionDecisionSliceErrorV1::CheckedIntegerRange)?;
    let ref_count = u32::try_from(refs.len())
        .map_err(|_| FlatActionDecisionSliceErrorV1::CheckedIntegerRange)?;
    let object_count = u16::try_from(objects.len())
        .map_err(|_| FlatActionDecisionSliceErrorV1::CheckedIntegerRange)?;
    let mut commitment =
        FlatActionCommitmentHasherV1::new(actor, action_count, ref_count, object_count);
    for (object_index, object) in objects.iter().copied().enumerate() {
        commitment.update_object(
            u16::try_from(object_index)
                .map_err(|_| FlatActionDecisionSliceErrorV1::CheckedIntegerRange)?,
            object,
        );
    }
    for (action_index, action) in actions.iter().copied().enumerate() {
        let ref_start = usize::try_from(action.ref_start)
            .map_err(|_| FlatActionDecisionSliceErrorV1::CheckedIntegerRange)?;
        let ref_end = ref_start
            .checked_add(usize::from(action.ref_len))
            .ok_or(FlatActionDecisionSliceErrorV1::CheckedIntegerRange)?;
        let action_refs = refs
            .get(ref_start..ref_end)
            .ok_or(FlatActionDecisionSliceErrorV1::InvalidActionReference)?;
        for reference in action_refs.iter().copied() {
            let object = *objects
                .get(usize::from(reference.object_index))
                .ok_or(FlatActionDecisionSliceErrorV1::InvalidActionReference)?;
            commitment.update_ref(reference, object);
        }
        commitment.update_action(
            u32::try_from(action_index)
                .map_err(|_| FlatActionDecisionSliceErrorV1::CheckedIntegerRange)?,
            action,
        );
    }
    Ok(commitment.finish())
}

/// Builds the exact public v1 compact rows from referenced objects only.
///
/// The prior implementation discovered referenced objects by repeatedly
/// walking every arena object. This resolver instead validates each stable
/// reference directly, deduplicates only those referenced objects, sorts the
/// resulting public rows by the unchanged v1 canonical key, and binds the
/// unchanged v1 SHA-256 commitment once when the private decision is created.
fn flat_build_action_cache_v1(
    session: &FastActorSessionV1,
    current: &FastActorCurrentDecisionV1,
    reusable: Option<FlatActionDecisionCacheV1>,
) -> Result<FlatActionDecisionCacheV1, FlatActionDecisionSliceErrorV1> {
    flat_validate_current_binding_header_v1(session, current)?;

    let action_count_u32 = u32::try_from(current.candidates.len())
        .map_err(|_| FlatActionDecisionSliceErrorV1::CheckedIntegerRange)?;
    let actor: PlayerSeatV1 = current.actor.into();
    let (mut actions, mut refs, mut objects, mut unindexed_refs, mut resolved_objects) =
        if let Some(mut reusable) = reusable {
            reusable.actions.clear();
            reusable.refs.clear();
            reusable.objects.clear();
            reusable.scratch_unindexed_refs.clear();
            reusable.scratch_resolved_objects.clear();
            (
                reusable.actions,
                reusable.refs,
                reusable.objects,
                reusable.scratch_unindexed_refs,
                reusable.scratch_resolved_objects,
            )
        } else {
            (
                Vec::with_capacity(current.candidates.len()),
                Vec::new(),
                Vec::new(),
                Vec::new(),
                Vec::new(),
            )
        };
    if actions.capacity() < current.candidates.len() {
        actions.reserve(current.candidates.len());
    }

    for (action_index, candidate) in current.candidates.iter().enumerate() {
        flat_validate_semantic_policy_pair_v1(candidate)?;
        let action_index = u32::try_from(action_index)
            .map_err(|_| FlatActionDecisionSliceErrorV1::CheckedIntegerRange)?;
        let ref_start = u32::try_from(unindexed_refs.len())
            .map_err(|_| FlatActionDecisionSliceErrorV1::CheckedIntegerRange)?;
        let core = flat_action_core_and_refs_v1(
            &candidate.semantic,
            actor,
            ref_start,
            |role, order_index, associated_order, reference| {
                let object =
                    flat_visible_action_object_v1(&session.state, current.actor, reference)?;
                if let Some(previous) = resolved_objects
                    .iter()
                    .find(|candidate| candidate.arena_id == reference.arena_id)
                {
                    if previous.object != object {
                        return Err(FlatActionDecisionSliceErrorV1::InvalidActionReference);
                    }
                } else {
                    if resolved_objects
                        .iter()
                        .any(|candidate| candidate.object.canonical_key() == object.canonical_key())
                    {
                        return Err(FlatActionDecisionSliceErrorV1::DuplicateCanonicalObject);
                    }
                    resolved_objects.push(FlatResolvedActionObjectV1 {
                        arena_id: reference.arena_id,
                        object,
                    });
                }
                unindexed_refs.push(FlatUnindexedActionRefV1 {
                    action_index,
                    role,
                    order_index,
                    associated_order,
                    object,
                });
                Ok(())
            },
        )?;
        actions.push(core);
    }
    flat_validate_origin_decision_v1(current, &session.state)?;

    resolved_objects.sort_unstable_by_key(|candidate| candidate.object.canonical_key());
    if resolved_objects
        .windows(2)
        .any(|pair| pair[0].object.canonical_key() == pair[1].object.canonical_key())
    {
        return Err(FlatActionDecisionSliceErrorV1::DuplicateCanonicalObject);
    }
    objects.extend(resolved_objects.iter().map(|candidate| candidate.object));

    if refs.capacity() < unindexed_refs.len() {
        refs.reserve(unindexed_refs.len());
    }
    for reference in unindexed_refs.iter().copied() {
        let key = reference.object.canonical_key();
        let object_index = objects
            .binary_search_by_key(&key, |candidate| candidate.canonical_key())
            .map_err(|_| FlatActionDecisionSliceErrorV1::InvalidActionReference)?;
        refs.push(FlatActionRefV1 {
            action_index: reference.action_index,
            role: reference.role,
            order_index: reference.order_index,
            associated_order: reference.associated_order,
            card_token: reference.object.card_token,
            object_index: u16::try_from(object_index)
                .map_err(|_| FlatActionDecisionSliceErrorV1::CheckedIntegerRange)?,
        });
    }
    let commitment = flat_action_commitment_from_rows_v1(actor, &actions, &refs, &objects)?;
    let binding = flat_action_binding_v1(session, current, action_count_u32, commitment);
    unindexed_refs.clear();
    resolved_objects.clear();
    Ok(FlatActionDecisionCacheV1 {
        binding,
        actions,
        refs,
        objects,
        scratch_unindexed_refs: unindexed_refs,
        scratch_resolved_objects: resolved_objects,
    })
}

/// Revalidates the live private decision against its exact cached public rows
/// without rescanning the arena or recomputing SHA-256. This pass is
/// allocation-free and touches only current candidates and their references.
fn flat_validate_action_cache_v1(
    session: &FastActorSessionV1,
    current: &FastActorCurrentDecisionV1,
    cache: &FlatActionDecisionCacheV1,
) -> Result<(), FlatActionDecisionSliceErrorV1> {
    flat_validate_current_binding_header_v1(session, current)?;
    let actor: PlayerSeatV1 = current.actor.into();
    // Preserve the v1 fail-closed error precedence: first validate every
    // semantic/policy pair and visible reference, then validate the exact
    // origin context, and only then compare compact rows with the cache.
    let mut validated_ref_count = 0_usize;
    for candidate in &current.candidates {
        flat_validate_semantic_policy_pair_v1(candidate)?;
        let ref_start = u32::try_from(validated_ref_count)
            .map_err(|_| FlatActionDecisionSliceErrorV1::CheckedIntegerRange)?;
        let core = flat_action_core_and_refs_v1(
            &candidate.semantic,
            actor,
            ref_start,
            |_, _, _, reference| {
                flat_visible_action_object_v1(&session.state, current.actor, reference)?;
                Ok(())
            },
        )?;
        validated_ref_count = validated_ref_count
            .checked_add(usize::from(core.ref_len))
            .ok_or(FlatActionDecisionSliceErrorV1::CheckedIntegerRange)?;
    }
    flat_validate_origin_decision_v1(current, &session.state)?;

    let mut ref_cursor = 0_usize;
    for (action_index, candidate) in current.candidates.iter().enumerate() {
        let action_index_u32 = u32::try_from(action_index)
            .map_err(|_| FlatActionDecisionSliceErrorV1::CheckedIntegerRange)?;
        let ref_start = u32::try_from(ref_cursor)
            .map_err(|_| FlatActionDecisionSliceErrorV1::CheckedIntegerRange)?;
        let core = flat_action_core_and_refs_v1(
            &candidate.semantic,
            actor,
            ref_start,
            |role, order_index, associated_order, reference| {
                let object =
                    flat_visible_action_object_v1(&session.state, current.actor, reference)?;
                let key = object.canonical_key();
                let object_index = cache
                    .objects
                    .binary_search_by_key(&key, |candidate| candidate.canonical_key())
                    .map_err(|_| FlatActionDecisionSliceErrorV1::CorruptCurrentBinding)?;
                let actual = FlatActionRefV1 {
                    action_index: action_index_u32,
                    role,
                    order_index,
                    associated_order,
                    card_token: object.card_token,
                    object_index: u16::try_from(object_index)
                        .map_err(|_| FlatActionDecisionSliceErrorV1::CheckedIntegerRange)?,
                };
                if cache.refs.get(ref_cursor).copied() != Some(actual) {
                    return Err(FlatActionDecisionSliceErrorV1::CorruptCurrentBinding);
                }
                ref_cursor = ref_cursor
                    .checked_add(1)
                    .ok_or(FlatActionDecisionSliceErrorV1::CheckedIntegerRange)?;
                Ok(())
            },
        )?;
        if cache.actions.get(action_index).copied() != Some(core) {
            return Err(FlatActionDecisionSliceErrorV1::CorruptCurrentBinding);
        }
    }
    if validated_ref_count != ref_cursor
        || ref_cursor != cache.refs.len()
        || current.candidates.len() != cache.actions.len()
        || cache
            .objects
            .windows(2)
            .any(|pair| pair[0].canonical_key() >= pair[1].canonical_key())
    {
        return Err(FlatActionDecisionSliceErrorV1::CorruptCurrentBinding);
    }
    let legal_action_count = u32::try_from(current.candidates.len())
        .map_err(|_| FlatActionDecisionSliceErrorV1::CheckedIntegerRange)?;
    let expected_binding = flat_action_binding_v1(
        session,
        current,
        legal_action_count,
        cache.binding.candidate_order_commitment,
    );
    if cache.binding != expected_binding {
        return Err(FlatActionDecisionSliceErrorV1::CorruptCurrentBinding);
    }
    Ok(())
}

/// Consume-side validation of the same private cache. All failures collapse
/// to a stale binding at the public boundary, so this path can compare live
/// rows while it validates them instead of repeating encode's error-precedence
/// pass. It remains refs-only, allocation-free, and SHA-free.
fn flat_validate_action_cache_for_consume_v1(
    session: &FastActorSessionV1,
    current: &FastActorCurrentDecisionV1,
    cache: &FlatActionDecisionCacheV1,
) -> Result<(), FlatActionDecisionSliceErrorV1> {
    flat_validate_current_binding_header_v1(session, current)?;
    let actor: PlayerSeatV1 = current.actor.into();
    let mut ref_cursor = 0_usize;
    for (action_index, candidate) in current.candidates.iter().enumerate() {
        flat_validate_semantic_policy_pair_v1(candidate)?;
        let action_index_u32 = u32::try_from(action_index)
            .map_err(|_| FlatActionDecisionSliceErrorV1::CheckedIntegerRange)?;
        let ref_start = u32::try_from(ref_cursor)
            .map_err(|_| FlatActionDecisionSliceErrorV1::CheckedIntegerRange)?;
        let core = flat_action_core_and_refs_v1(
            &candidate.semantic,
            actor,
            ref_start,
            |role, order_index, associated_order, reference| {
                let object =
                    flat_visible_action_object_v1(&session.state, current.actor, reference)?;
                let key = object.canonical_key();
                let object_index = cache
                    .objects
                    .binary_search_by_key(&key, |candidate| candidate.canonical_key())
                    .map_err(|_| FlatActionDecisionSliceErrorV1::CorruptCurrentBinding)?;
                let actual = FlatActionRefV1 {
                    action_index: action_index_u32,
                    role,
                    order_index,
                    associated_order,
                    card_token: object.card_token,
                    object_index: u16::try_from(object_index)
                        .map_err(|_| FlatActionDecisionSliceErrorV1::CheckedIntegerRange)?,
                };
                if cache.refs.get(ref_cursor).copied() != Some(actual) {
                    return Err(FlatActionDecisionSliceErrorV1::CorruptCurrentBinding);
                }
                ref_cursor = ref_cursor
                    .checked_add(1)
                    .ok_or(FlatActionDecisionSliceErrorV1::CheckedIntegerRange)?;
                Ok(())
            },
        )?;
        if cache.actions.get(action_index).copied() != Some(core) {
            return Err(FlatActionDecisionSliceErrorV1::CorruptCurrentBinding);
        }
    }
    flat_validate_origin_decision_v1(current, &session.state)?;
    if ref_cursor != cache.refs.len()
        || current.candidates.len() != cache.actions.len()
        || cache
            .objects
            .windows(2)
            .any(|pair| pair[0].canonical_key() >= pair[1].canonical_key())
    {
        return Err(FlatActionDecisionSliceErrorV1::CorruptCurrentBinding);
    }
    let legal_action_count = u32::try_from(current.candidates.len())
        .map_err(|_| FlatActionDecisionSliceErrorV1::CheckedIntegerRange)?;
    if cache.binding
        != flat_action_binding_v1(
            session,
            current,
            legal_action_count,
            cache.binding.candidate_order_commitment,
        )
    {
        return Err(FlatActionDecisionSliceErrorV1::CorruptCurrentBinding);
    }
    Ok(())
}

/// Frozen correctness-first reference retained only for parity tests.
///
/// This deliberately repeats whole-arena scans and nested canonical-index
/// scans. Production encode and consume must never call it.
#[cfg(test)]
fn flat_action_reference_preflight_v1(
    session: &FastActorSessionV1,
    current: &FastActorCurrentDecisionV1,
) -> Result<FlatActionPreflightV1, FlatActionDecisionSliceErrorV1> {
    if current.environment_revision != session.environment_revision
        || current.bound_policy_step_count != session.policy_step_count
        || current.bound_physical_decision_count != session.physical_decision_count
    {
        return Err(FlatActionDecisionSliceErrorV1::CorruptCurrentBinding);
    }
    if current.candidates.is_empty() {
        return Err(FlatActionDecisionSliceErrorV1::InvalidDecisionRelation);
    }
    flat_validate_current_decision_relations_v1(current, &session.state)?;

    let action_count = current.candidates.len();
    let action_count_u32 = u32::try_from(action_count)
        .map_err(|_| FlatActionDecisionSliceErrorV1::CheckedIntegerRange)?;
    let actor: PlayerSeatV1 = current.actor.into();
    let mut ref_count = 0_usize;
    for candidate in &current.candidates {
        flat_validate_semantic_policy_pair_v1(candidate)?;
        let ref_start = u32::try_from(ref_count)
            .map_err(|_| FlatActionDecisionSliceErrorV1::CheckedIntegerRange)?;
        let core = flat_action_core_and_refs_v1(
            &candidate.semantic,
            actor,
            ref_start,
            |_, _, _, reference| {
                flat_visible_action_object_v1(&session.state, current.actor, reference)?;
                Ok(())
            },
        )?;
        ref_count = ref_count
            .checked_add(usize::from(core.ref_len))
            .ok_or(FlatActionDecisionSliceErrorV1::CheckedIntegerRange)?;
    }
    flat_validate_origin_decision_v1(current, &session.state)?;
    let ref_count_u32 = u32::try_from(ref_count)
        .map_err(|_| FlatActionDecisionSliceErrorV1::CheckedIntegerRange)?;

    let mut object_count = 0_usize;
    for (object_id, _) in session.state.objects.iter() {
        let Some(row) = flat_current_ref_for_object_v1(current, &session.state, object_id)? else {
            continue;
        };
        for (prior_id, _) in session.state.objects.iter() {
            if prior_id >= object_id {
                break;
            }
            if flat_current_ref_for_object_v1(current, &session.state, prior_id)?
                .is_some_and(|prior| prior.canonical_key() == row.canonical_key())
            {
                return Err(FlatActionDecisionSliceErrorV1::DuplicateCanonicalObject);
            }
        }
        object_count = object_count
            .checked_add(1)
            .ok_or(FlatActionDecisionSliceErrorV1::CheckedIntegerRange)?;
    }
    let object_count_u16 = u16::try_from(object_count)
        .map_err(|_| FlatActionDecisionSliceErrorV1::CheckedIntegerRange)?;

    let mut commitment =
        FlatActionCommitmentHasherV1::new(actor, action_count_u32, ref_count_u32, object_count_u16);
    flat_for_each_canonical_object_v1(
        current,
        &session.state,
        object_count,
        |object_index, object| {
            commitment.update_object(object_index, object);
            Ok(())
        },
    )?;
    let mut ref_cursor = 0_usize;
    for (action_index, candidate) in current.candidates.iter().enumerate() {
        let action_index_u32 = u32::try_from(action_index)
            .map_err(|_| FlatActionDecisionSliceErrorV1::CheckedIntegerRange)?;
        let ref_start = u32::try_from(ref_cursor)
            .map_err(|_| FlatActionDecisionSliceErrorV1::CheckedIntegerRange)?;
        let core = flat_action_core_and_refs_v1(
            &candidate.semantic,
            actor,
            ref_start,
            |role, order_index, associated_order, reference| {
                let object =
                    flat_visible_action_object_v1(&session.state, current.actor, reference)?;
                let object_index = flat_canonical_object_index_v1(current, &session.state, object)?;
                commitment.update_ref(
                    FlatActionRefV1 {
                        action_index: action_index_u32,
                        role,
                        order_index,
                        associated_order,
                        card_token: object.card_token,
                        object_index,
                    },
                    object,
                );
                ref_cursor = ref_cursor
                    .checked_add(1)
                    .ok_or(FlatActionDecisionSliceErrorV1::CheckedIntegerRange)?;
                Ok(())
            },
        )?;
        commitment.update_action(action_index_u32, core);
    }
    if ref_cursor != ref_count {
        return Err(FlatActionDecisionSliceErrorV1::InvalidActionReference);
    }

    Ok(FlatActionPreflightV1 {
        binding: FlatActionDecisionBindingV1 {
            slice_version: FLAT_ACTION_DECISION_SLICE_VERSION_V1,
            ref_role_mapping_version: FLAT_ACTION_REF_ROLE_MAPPING_VERSION_V1,
            card_token_mapping_version: FLAT_ACTION_CARD_TOKEN_MAPPING_VERSION_V1,
            candidate_commitment_version: FLAT_ACTION_CANDIDATE_COMMITMENT_VERSION_V1,
            card_db_hash: KERNEL_CARDDB_HASH,
            episode_id: session.episode_id,
            environment_revision: session.environment_revision,
            bound_policy_step_count: current.bound_policy_step_count,
            physical_decision_id: current.physical_decision_id,
            bound_physical_decision_count: current.bound_physical_decision_count,
            substep_index: current.substep_index,
            substep_count: current.substep_count,
            acting_player: current.actor.0,
            decision_kind: flat_decision_kind_id_v1(current.decision_kind),
            legal_action_count: action_count_u32,
            candidate_order_commitment: commitment.finish(),
        },
        action_count,
        ref_count,
        object_count,
    })
}

#[cfg(test)]
fn flat_action_reference_materialization_v1(
    session: &FastActorSessionV1,
    current: &FastActorCurrentDecisionV1,
) -> Result<FlatActionDecisionCacheV1, FlatActionDecisionSliceErrorV1> {
    let preflight = flat_action_reference_preflight_v1(session, current)?;
    let actor: PlayerSeatV1 = current.actor.into();
    let mut objects = vec![FlatActionObjectV1::default(); preflight.object_count];
    let mut object_cursor = 0_usize;
    for (object_id, _) in session.state.objects.iter() {
        if let Some(row) = flat_current_ref_for_object_v1(current, &session.state, object_id)? {
            objects[object_cursor] = row;
            object_cursor += 1;
        }
    }
    objects.sort_unstable_by_key(|row| row.canonical_key());

    let mut actions = vec![FlatActionCoreV1::default(); preflight.action_count];
    let mut refs = vec![FlatActionRefV1::default(); preflight.ref_count];
    let mut ref_cursor = 0_usize;
    for (action_index, candidate) in current.candidates.iter().enumerate() {
        let action_index_u32 = u32::try_from(action_index)
            .map_err(|_| FlatActionDecisionSliceErrorV1::CheckedIntegerRange)?;
        let ref_start = u32::try_from(ref_cursor)
            .map_err(|_| FlatActionDecisionSliceErrorV1::CheckedIntegerRange)?;
        let core = flat_action_core_and_refs_v1(
            &candidate.semantic,
            actor,
            ref_start,
            |role, order_index, associated_order, reference| {
                let object =
                    flat_visible_action_object_v1(&session.state, current.actor, reference)?;
                let object_index = objects
                    .iter()
                    .position(|candidate| *candidate == object)
                    .ok_or(FlatActionDecisionSliceErrorV1::InvalidActionReference)?;
                refs[ref_cursor] = FlatActionRefV1 {
                    action_index: action_index_u32,
                    role,
                    order_index,
                    associated_order,
                    card_token: object.card_token,
                    object_index: u16::try_from(object_index)
                        .map_err(|_| FlatActionDecisionSliceErrorV1::CheckedIntegerRange)?,
                };
                ref_cursor += 1;
                Ok(())
            },
        )?;
        actions[action_index] = core;
    }
    if object_cursor != preflight.object_count || ref_cursor != preflight.ref_count {
        return Err(FlatActionDecisionSliceErrorV1::InvalidActionReference);
    }
    Ok(FlatActionDecisionCacheV1 {
        binding: preflight.binding,
        actions,
        refs,
        objects,
        scratch_unindexed_refs: Vec::new(),
        scratch_resolved_objects: Vec::new(),
    })
}

fn flat_validate_expected_decision_v1(
    session: &FastActorSessionV1,
    current: &FastActorCurrentDecisionV1,
    expected: FastActorDecisionV1,
) -> Result<(), FlatActionDecisionSliceErrorV1> {
    if expected.episode_id != session.episode_id {
        return Err(FlatActionDecisionSliceErrorV1::StaleEpisodeBinding);
    }
    if expected.environment_revision != session.environment_revision {
        return Err(FlatActionDecisionSliceErrorV1::StaleEnvironmentRevision);
    }
    let legal_action_count = u32::try_from(current.candidates.len())
        .map_err(|_| FlatActionDecisionSliceErrorV1::CheckedIntegerRange)?;
    if expected.step != session.policy_step_count
        || expected.physical_decision_id != current.physical_decision_id
        || expected.substep_index != current.substep_index
        || expected.substep_count != current.substep_count
        || expected.acting_player != PlayerSeatV1::from(current.actor)
        || expected.decision_kind != current.decision_kind
        || expected.legal_action_count != legal_action_count
    {
        return Err(FlatActionDecisionSliceErrorV1::DecisionMetadataMismatch);
    }
    Ok(())
}

/// One-use proof that an action came from the private fast actor's exact
/// current candidate vector. Its constructor is private to this module; the
/// policy surface can consume it but no sibling module can forge one from an
/// arbitrary `PolicyActionV5`.
pub(crate) struct FastActorCurrentCandidateProofV1 {
    owner_surface: *const PolicySurfaceV5,
    action: PolicyActionV5,
    current_revision: u64,
    next_revision: u64,
}

impl FastActorCurrentCandidateProofV1 {
    fn from_current(
        surface: &PolicySurfaceV5,
        current: &FastActorCurrentDecisionV1,
        selected_index: usize,
        next_revision: u64,
    ) -> Option<Self> {
        let selected = current.candidates.get(selected_index)?;
        Some(Self {
            owner_surface: std::ptr::from_ref(surface),
            action: selected.policy_action.clone(),
            current_revision: current.environment_revision,
            next_revision,
        })
    }

    pub(crate) fn into_parts(self) -> (*const PolicySurfaceV5, PolicyActionV5, u64, u64) {
        (
            self.owner_surface,
            self.action,
            self.current_revision,
            self.next_revision,
        )
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum FastActorApplyPathV1 {
    InPlace,
    #[cfg(test)]
    CloneReference,
}

#[derive(Clone)]
pub struct RlEpisodeSessionV1 {
    deck_ids: SessionDeckIdsV1,
    deck_hashes: SessionDeckHashesV1,
    episode_id: u64,
    max_physical_decisions: u64,
    max_policy_steps: u64,
    state: crate::state::GameState,
    surface: PolicySurfaceV5,
    environment_revision: u64,
    policy_step_count: u64,
    physical_decision_count: u64,
    current: Option<CurrentDecisionV1>,
    terminal: Option<RlSessionTerminalV1>,
}

#[derive(Clone)]
pub struct RlEpisodeSessionSnapshotV5(RlEpisodeSessionV1);

/// In-process actor lane that preserves the v5 policy surface and transition
/// semantics while omitting observations, visible hashes, stable/display
/// strings, and all JSON/Python work.
#[derive(Clone)]
pub struct FastActorSessionV1 {
    deck_ids: SessionDeckIdsV1,
    deck_hashes: SessionDeckHashesV1,
    episode_id: u64,
    max_physical_decisions: u64,
    max_policy_steps: u64,
    state: crate::state::GameState,
    surface: PolicySurfaceV5,
    environment_revision: u64,
    policy_step_count: u64,
    physical_decision_count: u64,
    current: Option<FastActorCurrentDecisionV1>,
    flat_action_cache_spare: Option<FlatActionDecisionCacheV1>,
    terminal: Option<RlSessionTerminalV1>,
}

#[derive(Clone)]
pub struct FastActorSessionSnapshotV1(FastActorSessionV1);

impl RlEpisodeSessionV1 {
    pub fn reset(episode_id: u64, env_seed: u64, max_physical_decisions: u64) -> Self {
        let max_policy_steps = max_physical_decisions.saturating_mul(128).max(1);
        Self::reset_with_limits(
            episode_id,
            env_seed,
            max_physical_decisions,
            max_policy_steps,
        )
    }

    pub fn reset_with_limits(
        episode_id: u64,
        env_seed: u64,
        max_physical_decisions: u64,
        max_policy_steps: u64,
    ) -> Self {
        Self::reset_with_decks_and_limits(
            episode_id,
            env_seed,
            max_physical_decisions,
            max_policy_steps,
            canonical_burn_mirror_deck_ids(),
        )
        .expect("the built-in Burn/Burn deck pair is supported")
    }

    pub fn reset_with_decks(
        episode_id: u64,
        env_seed: u64,
        max_physical_decisions: u64,
        deck_ids: SessionDeckIdsV1,
    ) -> Result<Self, RlSessionError> {
        let max_policy_steps = max_physical_decisions.saturating_mul(128).max(1);
        Self::reset_with_decks_and_limits(
            episode_id,
            env_seed,
            max_physical_decisions,
            max_policy_steps,
            deck_ids,
        )
    }

    pub fn reset_with_decks_and_limits(
        episode_id: u64,
        env_seed: u64,
        max_physical_decisions: u64,
        max_policy_steps: u64,
        deck_ids: SessionDeckIdsV1,
    ) -> Result<Self, RlSessionError> {
        Self::reset_with_decks_and_limits_profiled(
            episode_id,
            env_seed,
            max_physical_decisions,
            max_policy_steps,
            deck_ids,
            None,
        )
    }

    fn reset_with_decks_and_limits_profiled(
        episode_id: u64,
        env_seed: u64,
        max_physical_decisions: u64,
        max_policy_steps: u64,
        deck_ids: SessionDeckIdsV1,
        profile: Option<&mut RlPhaseProfileV1>,
    ) -> Result<Self, RlSessionError> {
        Self::reset_with_decks_and_limits_profiled_in_audit_mode(
            episode_id,
            env_seed,
            max_physical_decisions,
            max_policy_steps,
            deck_ids,
            profile,
            SuppressionAuditMode::Off,
        )
    }

    fn reset_with_decks_and_limits_profiled_in_audit_mode(
        episode_id: u64,
        env_seed: u64,
        max_physical_decisions: u64,
        max_policy_steps: u64,
        deck_ids: SessionDeckIdsV1,
        mut profile: Option<&mut RlPhaseProfileV1>,
        suppression_audit_mode: SuppressionAuditMode,
    ) -> Result<Self, RlSessionError> {
        let mut session = measure_optional(&mut profile, RlPhaseV1::Reset, || {
            let resolved_decks = resolve_runtime_decks(&deck_ids)?;
            let deck_hashes = resolved_decks.map(|deck| deck.runtime_deck_hash);
            let state = build_deck_pair_state(
                env_seed,
                resolved_decks[0].card_ids,
                resolved_decks[1].card_ids,
            )
            .map_err(|_| {
                session_error(
                    RlSessionErrorCode::UnsupportedDeck,
                    "runtime deck catalog failed full-support preflight",
                )
            })?;
            Ok(RlEpisodeSessionV1 {
                deck_ids,
                deck_hashes,
                episode_id,
                max_physical_decisions,
                max_policy_steps,
                state,
                surface: if suppression_audit_mode == SuppressionAuditMode::Off {
                    PolicySurfaceV5::new_for_session()
                } else {
                    PolicySurfaceV5::new_with_suppression_audit_mode(suppression_audit_mode)
                },
                environment_revision: 0,
                policy_step_count: 0,
                physical_decision_count: 0,
                current: None,
                terminal: None,
            })
        })?;
        session.advance_to_decision_or_terminal_profiled(profile);
        Ok(session)
    }

    pub fn current_response(&self) -> RlSessionResponseV1 {
        if let Some(terminal) = &self.terminal {
            return RlSessionResponseV1::Terminal(terminal.clone());
        }
        let current = self
            .current
            .as_ref()
            .expect("session has either a current decision or terminal");
        RlSessionResponseV1::Decision(RlSessionDecisionV1 {
            schema_version: RL_SESSION_SCHEMA_VERSION,
            deck_ids: self.deck_ids.clone(),
            deck_hashes: self.deck_hashes,
            episode_id: self.episode_id,
            step: self.policy_step_count,
            physical_decision_id: current.physical_decision_id,
            substep_index: current.substep_index,
            substep_count: current.substep_count,
            acting_player: current.actor.into(),
            observation: Box::new(current.observation.clone()),
            legal_actions: current
                .candidates
                .iter()
                .map(|c| c.record.clone())
                .collect(),
            reward: [0, 0],
        })
    }

    pub fn policy_step_count(&self) -> u64 {
        self.policy_step_count
    }

    pub fn physical_decision_count(&self) -> u64 {
        self.physical_decision_count
    }

    pub fn diagnostic_state_hash(&self) -> u64 {
        self.state.diagnostic_state_hash()
    }

    pub fn privileged_environment_hash(&self) -> u64 {
        self.compute_environment_hash(self.current.as_ref())
            .expect("session environment serializes")
    }

    /// Exact audit hash over the shared actor-relevant state, surface, binding,
    /// metadata, and ordered action semantics. Unlike the JSONL environment
    /// hash, this intentionally excludes ObservationV5 and stable action ids.
    pub fn privileged_core_environment_hash(&self) -> u64 {
        let current = self
            .current
            .as_ref()
            .map(|decision| CoreEnvironmentDecisionRefV1 {
                actor: decision.actor,
                physical_decision_id: decision.physical_decision_id,
                substep_index: decision.substep_index,
                substep_count: decision.substep_count,
                environment_revision: decision.environment_revision,
                bound_policy_step_count: decision.bound_policy_step_count,
                bound_physical_decision_count: decision.bound_physical_decision_count,
                legal_action_semantics: decision
                    .candidates
                    .iter()
                    .map(|candidate| &candidate.record.semantic)
                    .collect(),
            });
        compute_core_environment_hash(
            &self.state,
            &self.surface,
            self.environment_revision,
            self.policy_step_count,
            self.physical_decision_count,
            current,
        )
        .expect("session core environment serializes")
    }

    pub fn snapshot_v5(&self) -> RlEpisodeSessionSnapshotV5 {
        RlEpisodeSessionSnapshotV5(self.clone())
    }

    pub fn restore_v5(&mut self, snapshot: &RlEpisodeSessionSnapshotV5) {
        *self = snapshot.0.clone();
    }

    pub fn step(
        &mut self,
        episode_id: u64,
        expected_step: u64,
        selected_index: u32,
        selected_action_id: &str,
    ) -> Result<RlSessionResponseV1, RlSessionError> {
        self.apply_step_profiled(
            episode_id,
            expected_step,
            selected_index,
            selected_action_id,
            None,
        )?;
        Ok(self.current_response())
    }

    fn apply_step_profiled(
        &mut self,
        episode_id: u64,
        expected_step: u64,
        selected_index: u32,
        selected_action_id: &str,
        mut profile: Option<&mut RlPhaseProfileV1>,
    ) -> Result<(), RlSessionError> {
        measure_optional(&mut profile, RlPhaseV1::StepValidation, || {
            if episode_id != self.episode_id {
                return Err(session_error(
                    RlSessionErrorCode::EpisodeIdMismatch,
                    "step request episode_id does not match the active episode",
                ));
            }
            if expected_step != self.policy_step_count {
                return Err(session_error(
                    RlSessionErrorCode::ExpectedStepMismatch,
                    "step request expected_step does not match the active decision step",
                ));
            }
            if self.terminal.is_some() {
                return Err(session_error(
                    RlSessionErrorCode::EpisodeAlreadyTerminal,
                    "episode is already terminal; reset before stepping again",
                ));
            }
            Ok(())
        })?;
        let current = self
            .current
            .as_ref()
            .expect("nonterminal session has current decision");
        let next_environment_revision =
            measure_optional(&mut profile, RlPhaseV1::StepIntegrity, || {
                if current.environment_revision != self.environment_revision
                    || current.bound_policy_step_count != self.policy_step_count
                    || current.bound_physical_decision_count != self.physical_decision_count
                {
                    return Err(session_error(
                        RlSessionErrorCode::StaleEnvironmentBinding,
                        "active decision no longer matches its owned environment binding",
                    ));
                }
                self.environment_revision.checked_add(1).ok_or_else(|| {
                    session_error(
                        RlSessionErrorCode::StaleEnvironmentBinding,
                        "owned environment revision exhausted",
                    )
                })
            })?;
        let (policy_action, completes_physical) = measure_optional(
            &mut profile,
            RlPhaseV1::StepSelection,
            || {
                let selected_index_usize = selected_index as usize;
                let Some(selected) = current.candidates.get(selected_index_usize) else {
                    return Err(session_error(
                        RlSessionErrorCode::SelectedIndexOutOfRange,
                        "selected_index is outside the current legal action list",
                    ));
                };
                if selected.record.stable_id != selected_action_id {
                    let code = if current
                        .candidates
                        .iter()
                        .any(|candidate| candidate.record.stable_id == selected_action_id)
                    {
                        RlSessionErrorCode::SelectedActionIdMismatch
                    } else {
                        RlSessionErrorCode::SelectedActionIdUnknown
                    };
                    return Err(session_error(
                        code,
                        "selected_index and selected_action_id do not identify the same current action",
                    ));
                }
                Ok((
                    selected.policy_action.clone(),
                    current.substep_index + 1 == current.substep_count,
                ))
            },
        )?;
        measure_optional(&mut profile, RlPhaseV1::StepApply, || {
            self.surface.apply_owned(
                &mut self.state,
                policy_action,
                self.environment_revision,
                next_environment_revision,
            )
        })
        .map_err(|_| {
            session_error(
                RlSessionErrorCode::StaleEnvironmentBinding,
                "selected action no longer matches the active policy environment",
            )
        })?;
        self.current = None;
        self.environment_revision = next_environment_revision;
        self.policy_step_count += 1;
        if completes_physical {
            self.physical_decision_count += 1;
        }
        self.advance_to_decision_or_terminal_profiled(profile);
        Ok(())
    }

    #[cfg(test)]
    fn advance_to_decision_or_terminal(&mut self) {
        self.advance_to_decision_or_terminal_profiled(None);
    }

    fn advance_to_decision_or_terminal_profiled(
        &mut self,
        mut profile: Option<&mut RlPhaseProfileV1>,
    ) {
        self.current = None;
        let surfaced = match measure_optional(&mut profile, RlPhaseV1::Advance, || {
            self.surface
                .next_decision_owned(&mut self.state, self.environment_revision)
        }) {
            Ok(decision) => decision,
            Err(_) => {
                self.terminal = Some(halted_terminal(
                    &self.deck_ids,
                    self.deck_hashes,
                    self.episode_id,
                    "fail_closed:policy_surface_environment".to_string(),
                    self.policy_step_count,
                    self.physical_decision_count,
                ));
                return;
            }
        };
        match &surfaced {
            PolicyDecisionV5::Surface(SurfaceDecision::Decision(Decision::GameOver { winner })) => {
                self.terminal = Some(terminal_from_winner(
                    &self.deck_ids,
                    self.deck_hashes,
                    self.episode_id,
                    *winner,
                    "game_over".to_string(),
                    self.policy_step_count,
                    self.physical_decision_count,
                ));
                return;
            }
            PolicyDecisionV5::Surface(SurfaceDecision::Decision(Decision::Halted {
                mechanic,
                source,
            })) => {
                self.terminal = Some(halted_terminal(
                    &self.deck_ids,
                    self.deck_hashes,
                    self.episode_id,
                    format!("engine_halted:{mechanic:?}:source:{}", source.0),
                    self.policy_step_count,
                    self.physical_decision_count,
                ));
                return;
            }
            _ => {}
        }
        let (substep_index, substep_count) = surfaced.substep();
        if substep_index == 0 && self.physical_decision_count >= self.max_physical_decisions {
            let _ = self.surface.discard_unanswered_scan();
            self.terminal = Some(truncated_terminal(
                &self.deck_ids,
                self.deck_hashes,
                self.episode_id,
                format!(
                    "physical_decision_cap_reached:{}",
                    self.max_physical_decisions
                ),
                self.policy_step_count,
                self.physical_decision_count,
            ));
            return;
        }
        let remaining_group_steps = u64::from(substep_count - substep_index);
        if self.policy_step_count.saturating_add(remaining_group_steps) > self.max_policy_steps {
            if substep_index == 0 {
                let _ = self.surface.discard_unanswered_scan();
            }
            self.terminal = Some(truncated_terminal(
                &self.deck_ids,
                self.deck_hashes,
                self.episode_id,
                format!("policy_step_cap_reached:{}", self.max_policy_steps),
                self.policy_step_count,
                self.physical_decision_count,
            ));
            return;
        }
        let Some(actor) = surfaced.actor(&self.state) else {
            self.terminal = Some(halted_terminal(
                &self.deck_ids,
                self.deck_hashes,
                self.episode_id,
                "fail_closed:nonterminal decision without acting player".to_string(),
                self.policy_step_count,
                self.physical_decision_count,
            ));
            return;
        };
        let observation = match measure_optional(&mut profile, RlPhaseV1::Observe, || {
            observe_policy_v5(
                &self.state,
                &self.surface,
                actor,
                self.policy_step_count,
                self.physical_decision_count,
                substep_index,
                substep_count,
            )
        }) {
            Ok(observation) => observation,
            Err(err) => {
                self.terminal = Some(halted_terminal(
                    &self.deck_ids,
                    self.deck_hashes,
                    self.episode_id,
                    format!("fail_closed:observation:{err}"),
                    self.policy_step_count,
                    self.physical_decision_count,
                ));
                return;
            }
        };
        let candidates = match measure_optional(&mut profile, RlPhaseV1::Actions, || {
            legal_action_candidates_v5(&surfaced, &self.state)
        }) {
            Ok(candidates) => candidates,
            Err(err) => {
                self.terminal = Some(halted_terminal(
                    &self.deck_ids,
                    self.deck_hashes,
                    self.episode_id,
                    format!("fail_closed:{err}"),
                    self.policy_step_count,
                    self.physical_decision_count,
                ));
                return;
            }
        };
        if candidates.is_empty() {
            self.terminal = Some(halted_terminal(
                &self.deck_ids,
                self.deck_hashes,
                self.episode_id,
                "fail_closed:nonterminal decision produced zero legal actions".to_string(),
                self.policy_step_count,
                self.physical_decision_count,
            ));
            return;
        }
        measure_optional(&mut profile, RlPhaseV1::Postbind, || {
            self.current = Some(CurrentDecisionV1 {
                actor,
                physical_decision_id: self.physical_decision_count,
                substep_index,
                substep_count,
                observation,
                candidates,
                environment_revision: self.environment_revision,
                bound_policy_step_count: self.policy_step_count,
                bound_physical_decision_count: self.physical_decision_count,
            });
        });
    }

    fn compute_environment_hash(&self, current: Option<&CurrentDecisionV1>) -> Result<u64, String> {
        #[cfg(test)]
        TEST_EXACT_ENVIRONMENT_HASH_CALLS.with(|calls| calls.set(calls.get().saturating_add(1)));

        #[derive(Serialize)]
        struct PolicyEnvironmentEnvelopeV1 {
            schema_version: u32,
            hash_algorithm: &'static str,
            diagnostic_state_hash_algorithm: &'static str,
            diagnostic_state_hash: u64,
            harness_surface_context: crate::surface_v2::HarnessSurfacePublicContextV2,
            policy_surface_context: crate::policy_surface_v5::PolicySurfaceContextIdsV5,
            policy_step_count: u64,
            physical_decision_count: u64,
            current_actor: Option<PlayerId>,
            physical_decision_id: Option<u64>,
            substep_index: Option<u32>,
            substep_count: Option<u32>,
            observation_projection_hash: Option<u64>,
            legal_action_ids: Vec<String>,
        }

        let envelope = PolicyEnvironmentEnvelopeV1 {
            schema_version: 1,
            hash_algorithm: POLICY_ENVIRONMENT_HASH_ALGORITHM,
            diagnostic_state_hash_algorithm: crate::state::DIAGNOSTIC_STATE_HASH_ALGORITHM,
            diagnostic_state_hash: self.state.diagnostic_state_hash(),
            harness_surface_context: self.surface.harness_public_context(),
            policy_surface_context: self.surface.privileged_scan_context()?,
            policy_step_count: self.policy_step_count,
            physical_decision_count: self.physical_decision_count,
            current_actor: current.map(|decision| decision.actor),
            physical_decision_id: current.map(|decision| decision.physical_decision_id),
            substep_index: current.map(|decision| decision.substep_index),
            substep_count: current.map(|decision| decision.substep_count),
            observation_projection_hash: current
                .map(|decision| decision.observation.visible_projection_hash),
            legal_action_ids: current
                .map(|decision| {
                    decision
                        .candidates
                        .iter()
                        .map(|candidate| candidate.record.stable_id.clone())
                        .collect()
                })
                .unwrap_or_default(),
        };
        let bytes = serde_json::to_vec(&envelope).map_err(|err| err.to_string())?;
        Ok(fnv1a64(&bytes))
    }
}

impl FastActorSessionV1 {
    pub fn reset(episode_id: u64, env_seed: u64, max_physical_decisions: u64) -> Self {
        let max_policy_steps = max_physical_decisions.saturating_mul(128).max(1);
        Self::reset_with_limits(
            episode_id,
            env_seed,
            max_physical_decisions,
            max_policy_steps,
        )
    }

    pub fn reset_with_limits(
        episode_id: u64,
        env_seed: u64,
        max_physical_decisions: u64,
        max_policy_steps: u64,
    ) -> Self {
        Self::reset_with_decks_and_limits(
            episode_id,
            env_seed,
            max_physical_decisions,
            max_policy_steps,
            canonical_burn_mirror_deck_ids(),
        )
        .expect("the built-in Burn/Burn deck pair is supported")
    }

    pub fn reset_with_decks(
        episode_id: u64,
        env_seed: u64,
        max_physical_decisions: u64,
        deck_ids: SessionDeckIdsV1,
    ) -> Result<Self, RlSessionError> {
        let max_policy_steps = max_physical_decisions.saturating_mul(128).max(1);
        Self::reset_with_decks_and_limits(
            episode_id,
            env_seed,
            max_physical_decisions,
            max_policy_steps,
            deck_ids,
        )
    }

    pub fn reset_with_decks_and_limits(
        episode_id: u64,
        env_seed: u64,
        max_physical_decisions: u64,
        max_policy_steps: u64,
        deck_ids: SessionDeckIdsV1,
    ) -> Result<Self, RlSessionError> {
        let resolved_decks = resolve_runtime_decks(&deck_ids)?;
        let deck_hashes = resolved_decks.map(|deck| deck.runtime_deck_hash);
        let state = build_deck_pair_state(
            env_seed,
            resolved_decks[0].card_ids,
            resolved_decks[1].card_ids,
        )
        .map_err(|_| {
            session_error(
                RlSessionErrorCode::UnsupportedDeck,
                "runtime deck catalog failed full-support preflight",
            )
        })?;
        let mut session = FastActorSessionV1 {
            deck_ids,
            deck_hashes,
            episode_id,
            max_physical_decisions,
            max_policy_steps,
            state,
            surface: PolicySurfaceV5::new_for_session(),
            environment_revision: 0,
            policy_step_count: 0,
            physical_decision_count: 0,
            current: None,
            flat_action_cache_spare: None,
            terminal: None,
        };
        session.advance_to_decision_or_terminal();
        Ok(session)
    }

    pub fn current_response(&self) -> FastActorResponseV1 {
        if let Some(terminal) = &self.terminal {
            return FastActorResponseV1::Terminal(terminal.clone());
        }
        let current = self
            .current
            .as_ref()
            .expect("session has either a current decision or terminal");
        FastActorResponseV1::Decision(FastActorDecisionV1 {
            episode_id: self.episode_id,
            step: self.policy_step_count,
            environment_revision: self.environment_revision,
            physical_decision_id: current.physical_decision_id,
            substep_index: current.substep_index,
            substep_count: current.substep_count,
            acting_player: current.actor.into(),
            decision_kind: current.decision_kind,
            legal_action_count: u32::try_from(current.candidates.len())
                .expect("fast actor candidate count was checked when bound"),
        })
    }

    /// Audit-only counts for the live flat-action cache and backing arena.
    #[cfg(any(test, feature = "flat-action-diagnostic"))]
    pub fn diagnostic_current_flat_action_shape_v1(
        &self,
    ) -> Option<FlatActionDecisionDiagnosticV1> {
        let current = self.current.as_ref()?;
        let cache = current.flat_action_cache.as_ref()?;
        Some(FlatActionDecisionDiagnosticV1 {
            arena_object_count: u32::try_from(self.state.objects.iter().count()).ok()?,
            action_count: u32::try_from(cache.actions.len()).ok()?,
            ref_count: u32::try_from(cache.refs.len()).ok()?,
            referenced_object_count: u16::try_from(cache.objects.len()).ok()?,
        })
    }

    /// Audit-only access to the already-constructed binding for timing the
    /// consume path independently of public-row copying. Production actors
    /// obtain this binding from [`Self::encode_current_flat_action_slice_v1`].
    #[cfg(any(test, feature = "flat-action-diagnostic"))]
    pub fn diagnostic_current_flat_action_binding_v1(&self) -> Option<FlatActionDecisionBindingV1> {
        self.current
            .as_ref()?
            .flat_action_cache
            .as_ref()
            .map(|cache| cache.binding)
    }

    /// Audit-only hash-cost probe over already-materialized private rows.
    ///
    /// Normal encode and consume never call this method. It exists so a
    /// diagnostic can quantify the once-per-new-decision v1 SHA cost without
    /// conflating it with reference resolution or environment transitions.
    #[cfg(any(test, feature = "flat-action-diagnostic"))]
    pub fn diagnostic_recompute_flat_action_commitment_v1(
        &self,
    ) -> Result<[u8; 16], FlatActionDecisionSliceErrorV1> {
        let current = self
            .current
            .as_ref()
            .ok_or(FlatActionDecisionSliceErrorV1::NoCurrentDecision)?;
        if let Some(error) = current.flat_action_cache_error {
            return Err(error);
        }
        let cache = current
            .flat_action_cache
            .as_ref()
            .ok_or(FlatActionDecisionSliceErrorV1::CorruptCurrentBinding)?;
        flat_action_commitment_from_rows_v1(
            current.actor.into(),
            &cache.actions,
            &cache.refs,
            &cache.objects,
        )
    }

    /// Audit-only reconstruction of the current private cache using its own
    /// retained vector capacities. This leaves the public binding and rows
    /// unchanged and exists solely to attribute steady-state cache allocation
    /// and construction cost separately from environment transitions.
    #[cfg(any(test, feature = "flat-action-diagnostic"))]
    pub fn diagnostic_rebuild_current_flat_action_cache_v1(
        &mut self,
    ) -> Result<[u8; 16], FlatActionDecisionSliceErrorV1> {
        let mut current = self
            .current
            .take()
            .ok_or(FlatActionDecisionSliceErrorV1::NoCurrentDecision)?;
        let reusable = current.flat_action_cache.take();
        let rebuilt = flat_build_action_cache_v1(self, &current, reusable);
        match rebuilt {
            Ok(cache) => {
                let commitment = cache.binding.candidate_order_commitment;
                current.flat_action_cache = Some(cache);
                current.flat_action_cache_error = None;
                self.current = Some(current);
                Ok(commitment)
            }
            Err(error) => {
                current.flat_action_cache_error = Some(error);
                self.current = Some(current);
                Err(error)
            }
        }
    }

    /// Encodes only the exact current executable action binding and ordered
    /// action semantics. Full state globals, object features, relations, and
    /// scorer inputs are deliberately outside this partial contract. The
    /// schema-only `ChooseEffectColor`, `ChooseEffectNumber`, and
    /// `FinishTargetSelection` rows fail closed until policy-v5 has matching
    /// executable actions.
    pub fn encode_current_flat_action_slice_v1(
        &self,
        expected: FastActorDecisionV1,
        buffers: &mut FlatActionDecisionSliceBuffersV1<'_>,
    ) -> Result<FlatActionDecisionSliceV1, FlatActionDecisionSliceErrorV1> {
        let current = self
            .current
            .as_ref()
            .ok_or(FlatActionDecisionSliceErrorV1::NoCurrentDecision)?;
        flat_validate_expected_decision_v1(self, current, expected)?;
        if let Some(error) = current.flat_action_cache_error {
            return Err(error);
        }
        let cache = current
            .flat_action_cache
            .as_ref()
            .ok_or(FlatActionDecisionSliceErrorV1::CorruptCurrentBinding)?;
        // Pass one validates the live private semantics, decision context,
        // actor-visible references, and their exact cached rows without
        // rescanning unrelated arena objects or recomputing SHA-256.
        flat_validate_action_cache_v1(self, current, cache)?;
        let action_count = cache.actions.len();
        let ref_count = cache.refs.len();
        let object_count = cache.objects.len();

        if buffers.actions.len() < action_count {
            return Err(FlatActionDecisionSliceErrorV1::InsufficientActionCapacity {
                required: action_count,
                available: buffers.actions.len(),
            });
        }
        if buffers.refs.len() < ref_count {
            return Err(FlatActionDecisionSliceErrorV1::InsufficientRefCapacity {
                required: ref_count,
                available: buffers.refs.len(),
            });
        }
        if buffers.objects.len() < object_count {
            return Err(FlatActionDecisionSliceErrorV1::InsufficientObjectCapacity {
                required: object_count,
                available: buffers.objects.len(),
            });
        }

        // Pass two copies only the active prefixes from the validated private
        // cache. Caller-owned tails remain byte-for-byte untouched.
        buffers.actions[..action_count].copy_from_slice(&cache.actions);
        buffers.refs[..ref_count].copy_from_slice(&cache.refs);
        buffers.objects[..object_count].copy_from_slice(&cache.objects);

        Ok(FlatActionDecisionSliceV1 {
            binding: cache.binding,
            active_action_count: cache.binding.legal_action_count,
            active_ref_count: u32::try_from(ref_count)
                .expect("flat reference count passed u32 cache construction"),
            active_object_count: u16::try_from(object_count)
                .expect("flat object count passed u16 cache construction"),
        })
    }

    /// Applies an index only if the complete flat action binding, including
    /// the ordered 128-bit compact-candidate commitment, still matches this
    /// session's private current decision. Live semantics and referenced
    /// object meanings are revalidated against the private cache, but the
    /// whole-arena preflight and SHA-256 commitment are not recomputed.
    pub fn consume_current_flat_action_slice_v1(
        &mut self,
        binding: FlatActionDecisionBindingV1,
        selected_index: u32,
    ) -> Result<FastActorResponseV1, RlSessionError> {
        let cached_binding = {
            let current = self.current.as_ref().ok_or_else(|| {
                session_error(
                    RlSessionErrorCode::StaleEnvironmentBinding,
                    "flat action result has no active decision to consume",
                )
            })?;
            let cache = current.flat_action_cache.as_ref().ok_or_else(|| {
                session_error(
                    RlSessionErrorCode::StaleEnvironmentBinding,
                    "active decision has no private flat action cache",
                )
            })?;
            flat_validate_action_cache_for_consume_v1(self, current, cache).map_err(|_| {
                session_error(
                    RlSessionErrorCode::StaleEnvironmentBinding,
                    "active decision cannot reproduce its private flat action rows",
                )
            })?;
            cache.binding
        };
        if binding != cached_binding {
            return Err(session_error(
                RlSessionErrorCode::StaleEnvironmentBinding,
                "flat action result does not match the complete active decision binding",
            ));
        }
        self.step(
            binding.episode_id,
            binding.bound_policy_step_count,
            selected_index,
        )
    }

    pub fn policy_step_count(&self) -> u64 {
        self.policy_step_count
    }

    pub fn physical_decision_count(&self) -> u64 {
        self.physical_decision_count
    }

    pub fn diagnostic_state_hash(&self) -> u64 {
        self.state.diagnostic_state_hash()
    }

    /// Audit-only counterpart to
    /// [`RlEpisodeSessionV1::privileged_core_environment_hash`]. It is never
    /// computed by reset/step and therefore does not tax the actor loop.
    pub fn privileged_core_environment_hash(&self) -> u64 {
        let current = self
            .current
            .as_ref()
            .map(|decision| CoreEnvironmentDecisionRefV1 {
                actor: decision.actor,
                physical_decision_id: decision.physical_decision_id,
                substep_index: decision.substep_index,
                substep_count: decision.substep_count,
                environment_revision: decision.environment_revision,
                bound_policy_step_count: decision.bound_policy_step_count,
                bound_physical_decision_count: decision.bound_physical_decision_count,
                legal_action_semantics: decision
                    .candidates
                    .iter()
                    .map(|candidate| &candidate.semantic)
                    .collect(),
            });
        compute_core_environment_hash(
            &self.state,
            &self.surface,
            self.environment_revision,
            self.policy_step_count,
            self.physical_decision_count,
            current,
        )
        .expect("fast actor core environment serializes")
    }

    /// Audit-only copy of the current canonical semantic action order.
    ///
    /// The fast actor deliberately omits semantic records from its hot
    /// response. Benchmarks that need to prove full-v5 parity may call this
    /// outside their timed loop; reset and step never invoke it.
    pub fn diagnostic_current_action_semantics(&self) -> Option<Vec<ActionSemanticV1>> {
        self.current.as_ref().map(|decision| {
            decision
                .candidates
                .iter()
                .map(|candidate| candidate.semantic.clone())
                .collect()
        })
    }

    pub fn snapshot_v1(&self) -> FastActorSessionSnapshotV1 {
        FastActorSessionSnapshotV1(self.clone())
    }

    pub fn restore_v1(&mut self, snapshot: &FastActorSessionSnapshotV1) {
        *self = snapshot.0.clone();
    }

    pub fn step(
        &mut self,
        episode_id: u64,
        expected_step: u64,
        selected_index: u32,
    ) -> Result<FastActorResponseV1, RlSessionError> {
        self.step_with_apply_path(
            episode_id,
            expected_step,
            selected_index,
            FastActorApplyPathV1::InPlace,
        )
    }

    #[cfg(test)]
    fn step_clone_reference(
        &mut self,
        episode_id: u64,
        expected_step: u64,
        selected_index: u32,
    ) -> Result<FastActorResponseV1, RlSessionError> {
        self.step_with_apply_path(
            episode_id,
            expected_step,
            selected_index,
            FastActorApplyPathV1::CloneReference,
        )
    }

    fn step_with_apply_path(
        &mut self,
        episode_id: u64,
        expected_step: u64,
        selected_index: u32,
        apply_path: FastActorApplyPathV1,
    ) -> Result<FastActorResponseV1, RlSessionError> {
        if episode_id != self.episode_id {
            return Err(session_error(
                RlSessionErrorCode::EpisodeIdMismatch,
                "step request episode_id does not match the active episode",
            ));
        }
        if expected_step != self.policy_step_count {
            return Err(session_error(
                RlSessionErrorCode::ExpectedStepMismatch,
                "step request expected_step does not match the active decision step",
            ));
        }
        if self.terminal.is_some() {
            return Err(session_error(
                RlSessionErrorCode::EpisodeAlreadyTerminal,
                "episode is already terminal; reset before stepping again",
            ));
        }

        let current = self
            .current
            .as_ref()
            .expect("nonterminal session has current decision");
        if current.environment_revision != self.environment_revision
            || current.bound_policy_step_count != self.policy_step_count
            || current.bound_physical_decision_count != self.physical_decision_count
        {
            return Err(session_error(
                RlSessionErrorCode::StaleEnvironmentBinding,
                "active decision no longer matches its owned environment binding",
            ));
        }
        let next_environment_revision =
            self.environment_revision.checked_add(1).ok_or_else(|| {
                session_error(
                    RlSessionErrorCode::StaleEnvironmentBinding,
                    "owned environment revision exhausted",
                )
            })?;
        let next_policy_step_count = self.policy_step_count.checked_add(1).ok_or_else(|| {
            session_error(
                RlSessionErrorCode::StaleEnvironmentBinding,
                "owned policy step counter exhausted",
            )
        })?;
        let completes_physical =
            current.substep_index.checked_add(1) == Some(current.substep_count);
        let next_physical_decision_count = if completes_physical {
            self.physical_decision_count.checked_add(1).ok_or_else(|| {
                session_error(
                    RlSessionErrorCode::StaleEnvironmentBinding,
                    "owned physical decision counter exhausted",
                )
            })?
        } else {
            self.physical_decision_count
        };
        let selected_index = selected_index as usize;
        if current.candidates.get(selected_index).is_none() {
            return Err(session_error(
                RlSessionErrorCode::SelectedIndexOutOfRange,
                "selected_index is outside the current legal action list",
            ));
        }

        let apply_result = match apply_path {
            FastActorApplyPathV1::InPlace => {
                let proof = FastActorCurrentCandidateProofV1::from_current(
                    &self.surface,
                    current,
                    selected_index,
                    next_environment_revision,
                )
                .expect("selected candidate was checked above");
                self.surface
                    .apply_fast_actor_current_candidate_in_place(&mut self.state, proof)
            }
            #[cfg(test)]
            FastActorApplyPathV1::CloneReference => self
                .surface
                .apply_owned(
                    &mut self.state,
                    current.candidates[selected_index].policy_action.clone(),
                    self.environment_revision,
                    next_environment_revision,
                )
                .map_err(|_| FastActorInPlaceApplyErrorV1::RejectedBeforeMutation),
        };
        if let Err(error) = apply_result {
            if error == FastActorInPlaceApplyErrorV1::InternalApplyFailure
                && apply_path == FastActorApplyPathV1::InPlace
            {
                // A bound action can fail here only if an internal invariant
                // changed after prevalidation. Do not expose a retryable
                // decision from a potentially partially mutated environment.
                self.current = None;
                self.terminal = Some(halted_terminal(
                    &self.deck_ids,
                    self.deck_hashes,
                    self.episode_id,
                    "fail_closed:fast_actor_in_place_apply".to_string(),
                    self.policy_step_count,
                    self.physical_decision_count,
                ));
                return Err(session_error(
                    RlSessionErrorCode::StaleEnvironmentBinding,
                    "prevalidated fast actor action failed internally; episode halted",
                ));
            }
            return Err(session_error(
                RlSessionErrorCode::StaleEnvironmentBinding,
                "selected action no longer matches the active policy environment",
            ));
        }
        let mut completed = self
            .current
            .take()
            .expect("successful fast actor apply retains the consumed decision");
        self.flat_action_cache_spare = completed.flat_action_cache.take();
        self.environment_revision = next_environment_revision;
        self.policy_step_count = next_policy_step_count;
        self.physical_decision_count = next_physical_decision_count;
        self.advance_to_decision_or_terminal();
        Ok(self.current_response())
    }

    fn advance_to_decision_or_terminal(&mut self) {
        self.current = None;
        let surfaced = match self
            .surface
            .next_decision_owned(&mut self.state, self.environment_revision)
        {
            Ok(decision) => decision,
            Err(_) => {
                self.terminal = Some(halted_terminal(
                    &self.deck_ids,
                    self.deck_hashes,
                    self.episode_id,
                    "fail_closed:policy_surface_environment".to_string(),
                    self.policy_step_count,
                    self.physical_decision_count,
                ));
                return;
            }
        };
        match &surfaced {
            PolicyDecisionV5::Surface(SurfaceDecision::Decision(Decision::GameOver { winner })) => {
                self.terminal = Some(terminal_from_winner(
                    &self.deck_ids,
                    self.deck_hashes,
                    self.episode_id,
                    *winner,
                    "game_over".to_string(),
                    self.policy_step_count,
                    self.physical_decision_count,
                ));
                return;
            }
            PolicyDecisionV5::Surface(SurfaceDecision::Decision(Decision::Halted {
                mechanic,
                source,
            })) => {
                self.terminal = Some(halted_terminal(
                    &self.deck_ids,
                    self.deck_hashes,
                    self.episode_id,
                    format!("engine_halted:{mechanic:?}:source:{}", source.0),
                    self.policy_step_count,
                    self.physical_decision_count,
                ));
                return;
            }
            _ => {}
        }
        let (substep_index, substep_count) = surfaced.substep();
        if substep_index == 0 && self.physical_decision_count >= self.max_physical_decisions {
            let _ = self.surface.discard_unanswered_scan();
            self.terminal = Some(truncated_terminal(
                &self.deck_ids,
                self.deck_hashes,
                self.episode_id,
                format!(
                    "physical_decision_cap_reached:{}",
                    self.max_physical_decisions
                ),
                self.policy_step_count,
                self.physical_decision_count,
            ));
            return;
        }
        let remaining_group_steps = u64::from(substep_count - substep_index);
        if self.policy_step_count.saturating_add(remaining_group_steps) > self.max_policy_steps {
            if substep_index == 0 {
                let _ = self.surface.discard_unanswered_scan();
            }
            self.terminal = Some(truncated_terminal(
                &self.deck_ids,
                self.deck_hashes,
                self.episode_id,
                format!("policy_step_cap_reached:{}", self.max_policy_steps),
                self.policy_step_count,
                self.physical_decision_count,
            ));
            return;
        }
        let Some(actor) = surfaced.actor(&self.state) else {
            self.terminal = Some(halted_terminal(
                &self.deck_ids,
                self.deck_hashes,
                self.episode_id,
                "fail_closed:nonterminal decision without acting player".to_string(),
                self.policy_step_count,
                self.physical_decision_count,
            ));
            return;
        };
        let candidates = match core_policy_action_candidates_v5(&surfaced, &self.state) {
            Ok(candidates) => candidates,
            Err(err) => {
                self.terminal = Some(halted_terminal(
                    &self.deck_ids,
                    self.deck_hashes,
                    self.episode_id,
                    format!("fail_closed:{err}"),
                    self.policy_step_count,
                    self.physical_decision_count,
                ));
                return;
            }
        };
        if candidates.is_empty() {
            self.terminal = Some(halted_terminal(
                &self.deck_ids,
                self.deck_hashes,
                self.episode_id,
                "fail_closed:nonterminal decision produced zero legal actions".to_string(),
                self.policy_step_count,
                self.physical_decision_count,
            ));
            return;
        }
        if u32::try_from(candidates.len()).is_err() {
            self.terminal = Some(halted_terminal(
                &self.deck_ids,
                self.deck_hashes,
                self.episode_id,
                "fail_closed:policy action index exceeds u32".to_string(),
                self.policy_step_count,
                self.physical_decision_count,
            ));
            return;
        }
        let decision_kind = match &surfaced {
            PolicyDecisionV5::Surface(_) => FastActorDecisionKindV1::Surface,
            PolicyDecisionV5::AttackerInclusion { .. } => {
                FastActorDecisionKindV1::AttackerInclusion
            }
            PolicyDecisionV5::BlockerInclusion { .. } => FastActorDecisionKindV1::BlockerInclusion,
        };
        let mut current = FastActorCurrentDecisionV1 {
            actor,
            decision_kind,
            origin_decision: surfaced,
            physical_decision_id: self.physical_decision_count,
            substep_index,
            substep_count,
            candidates,
            environment_revision: self.environment_revision,
            bound_policy_step_count: self.policy_step_count,
            bound_physical_decision_count: self.physical_decision_count,
            flat_action_cache: None,
            flat_action_cache_error: None,
        };
        let reusable_cache = self.flat_action_cache_spare.take();
        match flat_build_action_cache_v1(self, &current, reusable_cache) {
            Ok(cache) => current.flat_action_cache = Some(cache),
            Err(error) => current.flat_action_cache_error = Some(error),
        }
        self.current = Some(current);
    }
}

#[derive(Serialize)]
struct CoreEnvironmentDecisionRefV1<'a> {
    actor: PlayerId,
    physical_decision_id: u64,
    substep_index: u32,
    substep_count: u32,
    environment_revision: u64,
    bound_policy_step_count: u64,
    bound_physical_decision_count: u64,
    legal_action_semantics: Vec<&'a ActionSemanticV1>,
}

fn compute_core_environment_hash(
    state: &crate::state::GameState,
    surface: &PolicySurfaceV5,
    environment_revision: u64,
    policy_step_count: u64,
    physical_decision_count: u64,
    current: Option<CoreEnvironmentDecisionRefV1<'_>>,
) -> Result<u64, String> {
    #[derive(Serialize)]
    struct CoreEnvironmentEnvelopeV1<'a> {
        schema_version: u32,
        hash_algorithm: &'static str,
        diagnostic_state_hash_algorithm: &'static str,
        diagnostic_state_hash: u64,
        harness_surface_context: crate::surface_v2::HarnessSurfacePublicContextV2,
        policy_surface_context: PolicySurfaceContextIdsV5,
        environment_revision: u64,
        policy_step_count: u64,
        physical_decision_count: u64,
        current: Option<CoreEnvironmentDecisionRefV1<'a>>,
    }

    let envelope = CoreEnvironmentEnvelopeV1 {
        schema_version: 1,
        hash_algorithm: FAST_ACTOR_CORE_ENVIRONMENT_HASH_ALGORITHM,
        diagnostic_state_hash_algorithm: crate::state::DIAGNOSTIC_STATE_HASH_ALGORITHM,
        diagnostic_state_hash: state.diagnostic_state_hash(),
        harness_surface_context: surface.harness_public_context(),
        policy_surface_context: surface.privileged_scan_context()?,
        environment_revision,
        policy_step_count,
        physical_decision_count,
        current,
    };
    let bytes = serde_json::to_vec(&envelope).map_err(|err| err.to_string())?;
    Ok(fnv1a64(&bytes))
}

fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for &byte in bytes {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

#[cfg(test)]
thread_local! {
    static TEST_EXACT_ENVIRONMENT_HASH_CALLS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    static TEST_FLAT_ACTION_COMMITMENT_CONSTRUCTIONS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
fn reset_test_exact_environment_hash_calls() {
    TEST_EXACT_ENVIRONMENT_HASH_CALLS.with(|calls| calls.set(0));
}

#[cfg(test)]
fn test_exact_environment_hash_calls() -> u64 {
    TEST_EXACT_ENVIRONMENT_HASH_CALLS.with(std::cell::Cell::get)
}

#[cfg(test)]
fn reset_test_flat_action_commitment_constructions() {
    TEST_FLAT_ACTION_COMMITMENT_CONSTRUCTIONS.with(|calls| calls.set(0));
}

#[cfg(test)]
fn test_flat_action_commitment_constructions() -> u64 {
    TEST_FLAT_ACTION_COMMITMENT_CONSTRUCTIONS.with(std::cell::Cell::get)
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "request_type", rename_all = "snake_case", deny_unknown_fields)]
pub enum KernelRlRequestV1 {
    Reset {
        schema_version: u32,
        request_id: String,
        deck_ids: SessionDeckIdsV1,
        episode_id: u64,
        env_seed: u64,
        max_physical_decisions: u64,
        max_policy_steps: u64,
    },
    Step {
        schema_version: u32,
        request_id: String,
        episode_id: u64,
        expected_step: u64,
        selected_index: u32,
        selected_action_id: String,
    },
}

impl KernelRlRequestV1 {
    fn request_id(&self) -> &str {
        match self {
            KernelRlRequestV1::Reset { request_id, .. }
            | KernelRlRequestV1::Step { request_id, .. } => request_id,
        }
    }

    fn schema_version(&self) -> u32 {
        match self {
            KernelRlRequestV1::Reset { schema_version, .. }
            | KernelRlRequestV1::Step { schema_version, .. } => *schema_version,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KernelRlErrorV1 {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "response_type", rename_all = "snake_case")]
pub enum KernelRlResponseV1 {
    Decision {
        schema_version: u32,
        request_id: String,
        provenance: RlSessionProvenanceV1,
        deck_ids: SessionDeckIdsV1,
        deck_hashes: SessionDeckHashesV1,
        episode_id: u64,
        step: u64,
        physical_decision_id: u64,
        substep_index: u32,
        substep_count: u32,
        acting_player: PlayerSeatV1,
        observation: Box<ObservationV5>,
        legal_actions: Vec<LegalActionV5>,
        reward: [i32; 2],
    },
    Terminal {
        schema_version: u32,
        request_id: String,
        provenance: RlSessionProvenanceV1,
        deck_ids: SessionDeckIdsV1,
        deck_hashes: SessionDeckHashesV1,
        episode_id: u64,
        terminal_outcome: TerminalOutcomeV1,
        terminal_classification: TerminalClassificationV1,
        terminal_code: TerminalSafeCodeV2,
        winner: Option<PlayerSeatV1>,
        terminal_reward: [i32; 2],
        terminal_reason: String,
        policy_step_count: u64,
        physical_decision_count: u64,
    },
    Error {
        schema_version: u32,
        request_id: Option<String>,
        error: KernelRlErrorV1,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CachedProtocolExchangeV1 {
    // Request ids are process-unique except for one immediate identical retry.
    // The cache is deliberately one entry so stale-step safety comes from the
    // episode_id/expected_step preconditions, not an unbounded replay table.
    request_id: String,
    request: KernelRlRequestV1,
    response_line: String,
}

enum RetryDispositionV1 {
    Cached,
    ReuseMismatch(Box<KernelRlResponseV1>),
}

#[derive(Default)]
pub struct KernelRlJsonlServerV1 {
    session: Option<RlEpisodeSessionV1>,
    last_exchange: Option<CachedProtocolExchangeV1>,
}

impl KernelRlJsonlServerV1 {
    pub fn new() -> Self {
        KernelRlJsonlServerV1::default()
    }

    pub fn handle_line(&mut self, line: &str) -> String {
        self.handle_line_impl(line, None)
    }

    pub fn handle_line_profiled(&mut self, line: &str, profile: &mut RlPhaseProfileV1) -> String {
        self.handle_line_impl(line, Some(profile))
    }

    fn handle_line_impl(
        &mut self,
        line: &str,
        mut profile: Option<&mut RlPhaseProfileV1>,
    ) -> String {
        if let Some(profile) = profile.as_deref_mut() {
            profile.request_lines = profile.request_lines.saturating_add(1);
        }
        let value = match measure_optional(&mut profile, RlPhaseV1::Parse, || {
            parse_strict_json_value(line)
        }) {
            Ok(value) => value,
            Err(_) => {
                if serde_json::from_str::<serde::de::IgnoredAny>(line).is_ok() {
                    return serialize_response_profiled(
                        error_response(
                            None,
                            "malformed_request",
                            "request does not match the v5 protocol schema",
                        ),
                        &mut profile,
                    );
                }
                return serialize_response_profiled(
                    error_response(None, "malformed_json", "request line is not valid JSON"),
                    &mut profile,
                );
            }
        };
        let request_id = request_id_from_value(&value);
        let request = match measure_optional(&mut profile, RlPhaseV1::Decode, || {
            serde_json::from_value::<KernelRlRequestV1>(value)
        }) {
            Ok(request) => request,
            Err(_) => {
                return serialize_response_profiled(
                    error_response(
                        request_id,
                        "malformed_request",
                        "request does not match the v5 protocol schema",
                    ),
                    &mut profile,
                );
            }
        };
        if let Some(profile) = profile.as_deref_mut() {
            match &request {
                KernelRlRequestV1::Reset { .. } => {
                    profile.reset_requests = profile.reset_requests.saturating_add(1)
                }
                KernelRlRequestV1::Step { .. } => {
                    profile.step_requests = profile.step_requests.saturating_add(1)
                }
            }
        }
        let retry = measure_optional(&mut profile, RlPhaseV1::Retry, || {
            self.last_exchange.as_ref().and_then(|cached| {
                if cached.request_id != request.request_id() {
                    return None;
                }
                if cached.request == request {
                    Some(RetryDispositionV1::Cached)
                } else {
                    Some(RetryDispositionV1::ReuseMismatch(Box::new(error_response(
                        Some(request.request_id().to_string()),
                        "request_id_reuse_mismatch",
                        "request_id was reused for a different immediate request payload",
                    ))))
                }
            })
        });
        if let Some(retry) = retry {
            return match retry {
                RetryDispositionV1::Cached => {
                    measure_optional(&mut profile, RlPhaseV1::Response, || ());
                    let response_line =
                        measure_optional(&mut profile, RlPhaseV1::Serialize, || {
                            self.last_exchange
                                .as_ref()
                                .expect("cached retry has an exchange")
                                .response_line
                                .clone()
                        });
                    if let Some(profile) = profile.as_deref_mut() {
                        profile.response_lines = profile.response_lines.saturating_add(1);
                    }
                    response_line
                }
                RetryDispositionV1::ReuseMismatch(response) => {
                    let response =
                        measure_optional(&mut profile, RlPhaseV1::Response, || *response);
                    serialize_response_profiled(response, &mut profile)
                }
            };
        }
        let should_cache = request.schema_version() == RL_SESSION_SCHEMA_VERSION;
        let request_for_cache = request.clone();
        let response = self.handle_request_profiled(request, profile.as_deref_mut());
        let response_line = serialize_response_profiled(response, &mut profile);
        if should_cache {
            self.last_exchange = Some(CachedProtocolExchangeV1 {
                request_id: request_for_cache.request_id().to_string(),
                request: request_for_cache,
                response_line: response_line.clone(),
            });
        }
        response_line
    }

    fn handle_request_profiled(
        &mut self,
        request: KernelRlRequestV1,
        mut profile: Option<&mut RlPhaseProfileV1>,
    ) -> KernelRlResponseV1 {
        match request {
            KernelRlRequestV1::Reset {
                schema_version,
                request_id,
                deck_ids,
                episode_id,
                env_seed,
                max_physical_decisions,
                max_policy_steps,
            } => {
                if schema_version != RL_SESSION_SCHEMA_VERSION {
                    return measure_optional(&mut profile, RlPhaseV1::Response, || {
                        error_response(
                            Some(request_id),
                            "schema_version_mismatch",
                            "unsupported request schema_version",
                        )
                    });
                }
                let session = match RlEpisodeSessionV1::reset_with_decks_and_limits_profiled(
                    episode_id,
                    env_seed,
                    max_physical_decisions,
                    max_policy_steps,
                    deck_ids,
                    profile.as_deref_mut(),
                ) {
                    Ok(session) => session,
                    Err(err) => {
                        return measure_optional(&mut profile, RlPhaseV1::Response, || {
                            error_response(
                                Some(request_id),
                                session_error_code(&err.code),
                                &err.message,
                            )
                        });
                    }
                };
                let response = measure_optional(&mut profile, RlPhaseV1::Response, || {
                    session_response_to_protocol(request_id, session.current_response())
                });
                self.session = Some(session);
                response
            }
            KernelRlRequestV1::Step {
                schema_version,
                request_id,
                episode_id,
                expected_step,
                selected_index,
                selected_action_id,
            } => {
                if schema_version != RL_SESSION_SCHEMA_VERSION {
                    return measure_optional(&mut profile, RlPhaseV1::Response, || {
                        error_response(
                            Some(request_id),
                            "schema_version_mismatch",
                            "unsupported request schema_version",
                        )
                    });
                }
                let Some(session) = self.session.as_mut() else {
                    return measure_optional(&mut profile, RlPhaseV1::Response, || {
                        error_response(
                            Some(request_id),
                            "step_before_reset",
                            "step request received before reset",
                        )
                    });
                };
                match session.apply_step_profiled(
                    episode_id,
                    expected_step,
                    selected_index,
                    &selected_action_id,
                    profile.as_deref_mut(),
                ) {
                    Ok(()) => measure_optional(&mut profile, RlPhaseV1::Response, || {
                        session_response_to_protocol(request_id, session.current_response())
                    }),
                    Err(err) => measure_optional(&mut profile, RlPhaseV1::Response, || {
                        error_response(
                            Some(request_id),
                            session_error_code(&err.code),
                            &err.message,
                        )
                    }),
                }
            }
        }
    }
}

fn session_response_to_protocol(
    request_id: String,
    response: RlSessionResponseV1,
) -> KernelRlResponseV1 {
    match response {
        RlSessionResponseV1::Decision(decision) => KernelRlResponseV1::Decision {
            schema_version: RL_SESSION_SCHEMA_VERSION,
            request_id,
            provenance: RlSessionProvenanceV1::current(),
            deck_ids: decision.deck_ids,
            deck_hashes: decision.deck_hashes,
            episode_id: decision.episode_id,
            step: decision.step,
            physical_decision_id: decision.physical_decision_id,
            substep_index: decision.substep_index,
            substep_count: decision.substep_count,
            acting_player: decision.acting_player,
            observation: decision.observation,
            legal_actions: decision.legal_actions,
            reward: decision.reward,
        },
        RlSessionResponseV1::Terminal(terminal) => KernelRlResponseV1::Terminal {
            schema_version: RL_SESSION_SCHEMA_VERSION,
            request_id,
            provenance: RlSessionProvenanceV1::current(),
            deck_ids: terminal.deck_ids,
            deck_hashes: terminal.deck_hashes,
            episode_id: terminal.episode_id,
            terminal_outcome: terminal.terminal_outcome,
            terminal_classification: terminal.terminal_classification,
            terminal_code: terminal.terminal_code,
            winner: terminal.winner,
            terminal_reward: terminal.terminal_reward,
            terminal_reason: terminal.terminal_reason,
            policy_step_count: terminal.policy_step_count,
            physical_decision_count: terminal.physical_decision_count,
        },
    }
}

fn serialize_response(response: KernelRlResponseV1) -> String {
    serde_json::to_string(&response).expect("protocol response serializes")
}

fn serialize_response_profiled(
    response: KernelRlResponseV1,
    profile: &mut Option<&mut RlPhaseProfileV1>,
) -> String {
    let line = measure_optional(profile, RlPhaseV1::Serialize, || {
        serialize_response(response)
    });
    if let Some(profile) = profile.as_deref_mut() {
        profile.response_lines = profile.response_lines.saturating_add(1);
    }
    line
}

fn request_id_from_value(value: &Value) -> Option<String> {
    value
        .get("request_id")
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn error_response(request_id: Option<String>, code: &str, message: &str) -> KernelRlResponseV1 {
    KernelRlResponseV1::Error {
        schema_version: RL_SESSION_SCHEMA_VERSION,
        request_id,
        error: KernelRlErrorV1 {
            code: code.to_string(),
            message: message.to_string(),
        },
    }
}

fn session_error(code: RlSessionErrorCode, message: &str) -> RlSessionError {
    RlSessionError {
        code,
        message: message.to_string(),
    }
}

fn session_error_code(code: &RlSessionErrorCode) -> &'static str {
    match code {
        RlSessionErrorCode::EpisodeAlreadyTerminal => "episode_already_terminal",
        RlSessionErrorCode::EpisodeIdMismatch => "episode_id_mismatch",
        RlSessionErrorCode::ExpectedStepMismatch => "expected_step_mismatch",
        RlSessionErrorCode::SelectedIndexOutOfRange => "selected_index_out_of_range",
        RlSessionErrorCode::SelectedActionIdMismatch => "selected_action_id_mismatch",
        RlSessionErrorCode::SelectedActionIdUnknown => "selected_action_id_unknown",
        RlSessionErrorCode::StaleEnvironmentBinding => "stale_environment_binding",
        RlSessionErrorCode::UnsupportedDeck => "unsupported_deck",
    }
}

fn canonical_burn_mirror_deck_ids() -> SessionDeckIdsV1 {
    [
        CANONICAL_BURN_DECK_ID.to_string(),
        CANONICAL_BURN_DECK_ID.to_string(),
    ]
}

fn resolve_runtime_decks(
    deck_ids: &SessionDeckIdsV1,
) -> Result<[&'static RuntimeDeckDefinition; 2], RlSessionError> {
    let mut resolved = [None, None];
    for (seat, deck_id) in deck_ids.iter().enumerate() {
        let Some(deck) = runtime_deck_by_id(deck_id) else {
            return Err(session_error(
                RlSessionErrorCode::UnsupportedDeck,
                &format!(
                    "unsupported deck_id for seat {seat}; supported exact canonical ids are {CANONICAL_BURN_DECK_ID:?} and {CANONICAL_RALLY_DECK_ID:?}"
                ),
            ));
        };
        resolved[seat] = Some(deck);
    }
    Ok([
        resolved[0].expect("both deck seats resolve"),
        resolved[1].expect("both deck seats resolve"),
    ])
}

fn terminal_from_winner(
    deck_ids: &SessionDeckIdsV1,
    deck_hashes: SessionDeckHashesV1,
    episode_id: u64,
    winner: Option<PlayerId>,
    terminal_reason: String,
    policy_step_count: u64,
    physical_decision_count: u64,
) -> RlSessionTerminalV1 {
    let (terminal_outcome, terminal_reward) = match winner {
        Some(PlayerId::P0) => (TerminalOutcomeV1::P0Win, [1, -1]),
        Some(PlayerId::P1) => (TerminalOutcomeV1::P1Win, [-1, 1]),
        None => (TerminalOutcomeV1::Draw, [0, 0]),
        Some(other) => panic!("unsupported winner player id {}", other.0),
    };
    RlSessionTerminalV1 {
        schema_version: RL_SESSION_SCHEMA_VERSION,
        deck_ids: deck_ids.clone(),
        deck_hashes,
        episode_id,
        terminal_outcome,
        terminal_classification: TerminalClassificationV1::Natural,
        terminal_code: TerminalSafeCodeV2::NaturalGameOver,
        winner: winner.map(Into::into),
        terminal_reward,
        terminal_reason,
        policy_step_count,
        physical_decision_count,
    }
}

fn halted_terminal(
    deck_ids: &SessionDeckIdsV1,
    deck_hashes: SessionDeckHashesV1,
    episode_id: u64,
    terminal_reason: String,
    policy_step_count: u64,
    physical_decision_count: u64,
) -> RlSessionTerminalV1 {
    RlSessionTerminalV1 {
        schema_version: RL_SESSION_SCHEMA_VERSION,
        deck_ids: deck_ids.clone(),
        deck_hashes,
        episode_id,
        terminal_outcome: TerminalOutcomeV1::Halted,
        terminal_classification: TerminalClassificationV1::Halted,
        terminal_code: TerminalSafeCodeV2::FailClosed,
        winner: None,
        terminal_reward: [0, 0],
        terminal_reason,
        policy_step_count,
        physical_decision_count,
    }
}

fn truncated_terminal(
    deck_ids: &SessionDeckIdsV1,
    deck_hashes: SessionDeckHashesV1,
    episode_id: u64,
    terminal_reason: String,
    policy_step_count: u64,
    physical_decision_count: u64,
) -> RlSessionTerminalV1 {
    RlSessionTerminalV1 {
        schema_version: RL_SESSION_SCHEMA_VERSION,
        deck_ids: deck_ids.clone(),
        deck_hashes,
        episode_id,
        terminal_outcome: TerminalOutcomeV1::Truncated,
        terminal_classification: TerminalClassificationV1::Truncated,
        terminal_code: TerminalSafeCodeV2::DecisionCap,
        winner: None,
        terminal_reward: [0, 0],
        terminal_reason,
        policy_step_count,
        physical_decision_count,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::card_def::card_id_by_name;
    use crate::effect::EffectOp;
    use crate::engine::PendingOptionalCostSacrifice;
    use crate::policy_surface_v5::{
        reset_test_exact_surface_hash_calls, test_exact_surface_hash_calls,
    };
    use crate::rl::{
        card_name, make_legal_action_v5, reset_test_policy_v5_materialization_calls,
        test_policy_v5_materialization_calls, validate_core_policy_action_candidates_v5,
        ActionSemanticV1,
    };
    use crate::state::{Counters, GameObject, GameState, ObjectStateV4, SplitMix64, Step, Zone};
    use std::collections::HashSet;

    fn attacker_state(count: usize) -> GameState {
        let mut state = GameState::new_from_libraries(&[], &[], card_name, 91);
        state.step = Step::DeclareAttackers;
        state.active_player = PlayerId::P0;
        state.priority_player = PlayerId::P0;
        state.engine.combat.attackers_declared = false;
        let card_def = card_id_by_name("Voldaren Epicure").unwrap();
        for _ in 0..count {
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
        state
    }

    fn blocker_state(count: usize) -> GameState {
        let mut state = attacker_state(1);
        let attacker = state.players[0].battlefield[0];
        state.step = Step::DeclareBlockers;
        state.priority_player = PlayerId::P1;
        state.engine.combat.attackers_declared = true;
        state.engine.combat.attackers = vec![attacker];
        state.engine.combat.blockers_declared = false;
        let card_def = card_id_by_name("Voldaren Epicure").unwrap();
        for _ in 0..count {
            let id = state.objects.push(GameObject {
                card_def,
                name: "Voldaren Epicure".to_string(),
                owner: PlayerId::P1,
                controller: PlayerId::P1,
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
            state.players[1].battlefield.push(id);
        }
        state
    }

    fn add_battlefield_object(
        state: &mut GameState,
        player: PlayerId,
        name: &str,
    ) -> crate::ids::ObjectId {
        let card_def = card_id_by_name(name).unwrap();
        let id = state.objects.push(GameObject {
            card_def,
            name: name.to_string(),
            owner: player,
            controller: player,
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
        state.players[player.index()].battlefield.push(id);
        id
    }

    fn attacker_session(
        count: usize,
        max_physical_decisions: u64,
        max_policy_steps: u64,
    ) -> RlEpisodeSessionV1 {
        let mut session =
            RlEpisodeSessionV1::reset_with_limits(23, 91, max_physical_decisions, max_policy_steps);
        session.state = attacker_state(count);
        session.surface = PolicySurfaceV5::new();
        session.environment_revision = 0;
        session.policy_step_count = 0;
        session.physical_decision_count = 0;
        session.current = None;
        session.terminal = None;
        session.advance_to_decision_or_terminal();
        session
    }

    fn fast_attacker_session(
        count: usize,
        max_physical_decisions: u64,
        max_policy_steps: u64,
    ) -> FastActorSessionV1 {
        let mut session =
            FastActorSessionV1::reset_with_limits(23, 91, max_physical_decisions, max_policy_steps);
        session.state = attacker_state(count);
        session.surface = PolicySurfaceV5::new();
        session.environment_revision = 0;
        session.policy_step_count = 0;
        session.physical_decision_count = 0;
        session.current = None;
        session.terminal = None;
        session.advance_to_decision_or_terminal();
        session
    }

    fn fast_blocker_session(
        count: usize,
        max_physical_decisions: u64,
        max_policy_steps: u64,
    ) -> FastActorSessionV1 {
        let mut session =
            FastActorSessionV1::reset_with_limits(23, 91, max_physical_decisions, max_policy_steps);
        session.state = blocker_state(count);
        session.surface = PolicySurfaceV5::new();
        session.environment_revision = 0;
        session.policy_step_count = 0;
        session.physical_decision_count = 0;
        session.current = None;
        session.terminal = None;
        session.advance_to_decision_or_terminal();
        session
    }

    fn fast_actor_audit_bytes(session: &FastActorSessionV1) -> Vec<u8> {
        serde_json::to_vec(&(
            (
                &session.deck_ids,
                session.deck_hashes,
                session.episode_id,
                session.max_physical_decisions,
                session.max_policy_steps,
            ),
            (
                &session.state,
                session.surface.harness_public_context(),
                session.surface.privileged_scan_context().unwrap(),
            ),
            (
                session.environment_revision,
                session.policy_step_count,
                session.physical_decision_count,
                session.privileged_core_environment_hash(),
                format!("{:?}", session.current_response()),
                format!(
                    "{:?}",
                    session
                        .current
                        .as_ref()
                        .map(|decision| &decision.candidates)
                ),
                &session.terminal,
            ),
        ))
        .unwrap()
    }

    fn assert_fast_actor_rejection_is_byte_atomic(
        session: &mut FastActorSessionV1,
        expected_code: RlSessionErrorCode,
        request: impl FnOnce(&mut FastActorSessionV1) -> Result<FastActorResponseV1, RlSessionError>,
    ) {
        let before = fast_actor_audit_bytes(session);
        let error = request(session).unwrap_err();
        assert_eq!(error.code, expected_code);
        assert_eq!(
            fast_actor_audit_bytes(session),
            before,
            "rejected fast-actor input mutated the audit snapshot"
        );
    }

    fn action_at(response: &RlSessionResponseV1, action_index: usize) -> (u64, u32, String) {
        let RlSessionResponseV1::Decision(decision) = response else {
            panic!("expected decision");
        };
        let action = &decision.legal_actions[action_index];
        (
            decision.step,
            action.selected_index,
            action.stable_id.clone(),
        )
    }

    fn first_action(response: &RlSessionResponseV1) -> (u64, u32, String) {
        action_at(response, 0)
    }

    #[test]
    fn policy_cap_preflights_the_whole_combat_group_and_exact_fit_admits_it() {
        let below_group = attacker_session(3, 8, 2);
        assert_eq!(below_group.policy_step_count(), 0);
        assert_eq!(below_group.physical_decision_count(), 0);
        assert!(below_group.current.is_none());
        assert!(!below_group.surface.scan_active());
        let RlSessionResponseV1::Terminal(terminal) = below_group.current_response() else {
            panic!("policy cap below the group must truncate before exposing it");
        };
        assert_eq!(terminal.policy_step_count, 0);
        assert_eq!(terminal.physical_decision_count, 0);
        assert_eq!(terminal.terminal_code, TerminalSafeCodeV2::DecisionCap);
        assert_eq!(terminal.terminal_reason, "policy_step_cap_reached:2");

        let mut exact_fit = attacker_session(3, 8, 3);
        assert!(exact_fit.surface.scan_active());
        for expected_substep in 0..3 {
            let revision_before = exact_fit.environment_revision;
            let response = exact_fit.current_response();
            let RlSessionResponseV1::Decision(decision) = &response else {
                panic!("exact-fit cap must admit every combat substep");
            };
            assert_eq!(decision.step, u64::from(expected_substep));
            assert_eq!(decision.physical_decision_id, 0);
            assert_eq!(decision.substep_index, expected_substep);
            assert_eq!(decision.substep_count, 3);
            assert_eq!(decision.legal_actions.len(), 2);
            let (step, index, id) = first_action(&response);
            exact_fit.step(23, step, index, &id).unwrap();
            assert_eq!(
                exact_fit.environment_revision,
                revision_before + 1,
                "every successful policy step advances exactly one owned revision"
            );
            if let Some(current) = &exact_fit.current {
                assert_eq!(current.environment_revision, exact_fit.environment_revision);
                assert_eq!(current.bound_policy_step_count, exact_fit.policy_step_count);
                assert_eq!(
                    current.bound_physical_decision_count,
                    exact_fit.physical_decision_count
                );
            }
        }
        assert_eq!(exact_fit.policy_step_count(), 3);
        assert_eq!(exact_fit.physical_decision_count(), 1);
        assert!(!exact_fit.surface.scan_active());
        let RlSessionResponseV1::Terminal(terminal) = exact_fit.current_response() else {
            panic!("the empty-library combat fixture must terminate after the admitted group");
        };
        assert_eq!(terminal.policy_step_count, 3);
        assert_eq!(terminal.physical_decision_count, 1);
        assert_eq!(terminal.terminal_code, TerminalSafeCodeV2::NaturalGameOver);
        assert_eq!(terminal.terminal_reason, "game_over");
    }

    #[test]
    fn mid_combat_snapshot_restores_binding_response_and_next_group_transition() {
        let mut session = attacker_session(3, 8, 8);
        let start = session.current_response();
        let RlSessionResponseV1::Decision(start_decision) = &start else {
            panic!("expected first attacker inclusion");
        };
        assert_eq!(start_decision.physical_decision_id, 0);
        assert_eq!(start_decision.substep_index, 0);
        assert_eq!(start_decision.substep_count, 3);
        let (step, index, id) = action_at(&start, 1);
        session.step(23, step, index, &id).unwrap();

        let snapshot = session.snapshot_v5();
        let revision_before = session.environment_revision;
        let response_before = serde_json::to_vec(&session.current_response()).unwrap();
        let environment_before = session.privileged_environment_hash();
        assert_eq!(session.policy_step_count(), 1);
        assert_eq!(session.physical_decision_count(), 0);
        let RlSessionResponseV1::Decision(mid_decision) = session.current_response() else {
            panic!("snapshot must be mid-combat");
        };
        assert_eq!(mid_decision.step, 1);
        assert_eq!(mid_decision.physical_decision_id, 0);
        assert_eq!(mid_decision.substep_index, 1);
        assert_eq!(mid_decision.substep_count, 3);
        let (step, index, id) = first_action(&RlSessionResponseV1::Decision(mid_decision));

        let advanced = session.step(23, step, index, &id).unwrap();
        let advanced_bytes = serde_json::to_vec(&advanced).unwrap();
        let advanced_environment = session.privileged_environment_hash();
        assert_eq!(session.environment_revision, revision_before + 1);
        let RlSessionResponseV1::Decision(advanced_decision) = &advanced else {
            panic!("second answer must advance within the same combat group");
        };
        assert_eq!(advanced_decision.step, 2);
        assert_eq!(advanced_decision.physical_decision_id, 0);
        assert_eq!(advanced_decision.substep_index, 2);
        assert_eq!(advanced_decision.substep_count, 3);
        assert_ne!(advanced_environment, environment_before);

        session.restore_v5(&snapshot);
        assert_eq!(session.environment_revision, revision_before);
        assert_eq!(
            serde_json::to_vec(&session.current_response()).unwrap(),
            response_before
        );
        assert_eq!(session.privileged_environment_hash(), environment_before);
        assert_eq!(session.policy_step_count(), 1);
        assert_eq!(session.physical_decision_count(), 0);
        assert!(session.surface.scan_active());

        let replayed = session.step(23, step, index, &id).unwrap();
        assert_eq!(serde_json::to_vec(&replayed).unwrap(), advanced_bytes);
        assert_eq!(session.privileged_environment_hash(), advanced_environment);
    }

    #[test]
    fn session_snapshot_restore_reproduces_response_hash_and_next_transition() {
        let mut session = RlEpisodeSessionV1::reset(17, 991, 64);
        let snapshot = session.snapshot_v5();
        let response_before = serde_json::to_vec(&session.current_response()).unwrap();
        let environment_before = session.privileged_environment_hash();
        let (step, index, id) = first_action(&session.current_response());

        let first_result =
            serde_json::to_vec(&session.step(17, step, index, &id).unwrap()).unwrap();
        assert_ne!(session.privileged_environment_hash(), environment_before);

        session.restore_v5(&snapshot);
        assert_eq!(
            serde_json::to_vec(&session.current_response()).unwrap(),
            response_before
        );
        assert_eq!(session.privileged_environment_hash(), environment_before);
        assert_eq!(
            serde_json::to_vec(&session.step(17, step, index, &id).unwrap()).unwrap(),
            first_result
        );
    }

    #[test]
    fn privileged_exact_audit_detects_private_state_surface_and_counter_drift_then_restores() {
        let mut session = RlEpisodeSessionV1::reset(5, 1234, 64);
        let snapshot = session.snapshot_v5();
        let exact = session.privileged_environment_hash();

        session.state.players[0].life -= 1;
        assert_ne!(session.privileged_environment_hash(), exact);

        session.restore_v5(&snapshot);
        session.policy_step_count += 1;
        assert_ne!(session.privileged_environment_hash(), exact);

        session.restore_v5(&snapshot);
        session.surface.reset_harness_context_for_test();
        assert_ne!(session.privileged_environment_hash(), exact);

        session.restore_v5(&snapshot);
        assert_eq!(session.privileged_environment_hash(), exact);
    }

    #[test]
    fn owned_revision_and_counter_poisoning_precede_invalid_selection_then_restore() {
        let mut session = RlEpisodeSessionV1::reset(5, 1234, 64);
        let snapshot = session.snapshot_v5();
        let (step, index, id) = first_action(&session.current_response());

        session.current.as_mut().unwrap().environment_revision += 1;
        let err = session
            .step(5, step, u32::MAX, "unknown-action")
            .unwrap_err();
        assert_eq!(err.code, RlSessionErrorCode::StaleEnvironmentBinding);
        assert_eq!(
            err.message,
            "active decision no longer matches its owned environment binding"
        );
        assert!(!err.message.contains("0x"));

        session.restore_v5(&snapshot);
        session.policy_step_count += 1;
        let err = session
            .step(5, step + 1, u32::MAX, "unknown-action")
            .unwrap_err();
        assert_eq!(err.code, RlSessionErrorCode::StaleEnvironmentBinding);

        session.restore_v5(&snapshot);
        session.physical_decision_count += 1;
        let err = session
            .step(5, step, u32::MAX, "unknown-action")
            .unwrap_err();
        assert_eq!(err.code, RlSessionErrorCode::StaleEnvironmentBinding);

        session.restore_v5(&snapshot);
        assert!(session.step(5, step, index, &id).is_ok());
    }

    #[test]
    fn owned_revision_overflow_fails_before_selection_and_is_nonmutating() {
        let mut session = RlEpisodeSessionV1::reset(5, 1234, 64);
        session.environment_revision = u64::MAX;
        session.current.as_mut().unwrap().environment_revision = u64::MAX;
        let response_before = serde_json::to_vec(&session.current_response()).unwrap();
        let state_before = session.state.clone();
        let harness_before = session.surface.harness_public_context();
        let scan_before = session.surface.privileged_scan_context().unwrap();

        let err = session
            .step(5, session.policy_step_count, u32::MAX, "unknown-action")
            .unwrap_err();
        assert_eq!(err.code, RlSessionErrorCode::StaleEnvironmentBinding);
        assert_eq!(err.message, "owned environment revision exhausted");
        assert_eq!(session.environment_revision, u64::MAX);
        assert_eq!(session.policy_step_count, 0);
        assert_eq!(session.physical_decision_count, 0);
        assert_eq!(session.state, state_before);
        assert_eq!(session.surface.harness_public_context(), harness_before);
        assert_eq!(
            session.surface.privileged_scan_context().unwrap(),
            scan_before
        );
        assert_eq!(
            serde_json::to_vec(&session.current_response()).unwrap(),
            response_before
        );
    }

    #[test]
    fn owned_reset_and_combat_steps_compute_no_exact_hashes_but_audit_remains_available() {
        reset_test_exact_environment_hash_calls();
        reset_test_exact_surface_hash_calls();

        let mut session = attacker_session(3, 8, 8);
        assert_eq!(test_exact_environment_hash_calls(), 0);
        assert_eq!(test_exact_surface_hash_calls(), 0);

        for _ in 0..3 {
            let response = session.current_response();
            let (step, index, id) = first_action(&response);
            session.step(23, step, index, &id).unwrap();
        }
        assert_eq!(test_exact_environment_hash_calls(), 0);
        assert_eq!(
            test_exact_surface_hash_calls(),
            0,
            "owned combat scans must never enter the standalone exact-hash path"
        );

        let _ = session.privileged_environment_hash();
        assert_eq!(test_exact_environment_hash_calls(), 1);
        assert_eq!(test_exact_surface_hash_calls(), 0);
    }

    #[test]
    fn failed_final_owned_combat_commit_does_not_advance_revision_or_counters() {
        let mut session = attacker_session(1, 8, 8);
        let snapshot = session.snapshot_v5();
        let response = session.current_response();
        let (step, include_index, include_id) = action_at(&response, 1);

        session.state.players[0].battlefield.clear();
        let tampered_state = session.state.clone();
        let response_before = serde_json::to_vec(&session.current_response()).unwrap();
        let revision_before = session.environment_revision;
        let error = session
            .step(23, step, include_index, &include_id)
            .unwrap_err();
        assert_eq!(error.code, RlSessionErrorCode::StaleEnvironmentBinding);
        assert_eq!(session.environment_revision, revision_before);
        assert_eq!(session.policy_step_count, 0);
        assert_eq!(session.physical_decision_count, 0);
        assert_eq!(session.state, tampered_state);
        assert_eq!(
            serde_json::to_vec(&session.current_response()).unwrap(),
            response_before
        );

        session.restore_v5(&snapshot);
        assert!(session.step(23, step, include_index, &include_id).is_ok());
    }

    #[test]
    fn fast_actor_cap_snapshot_retry_overflow_and_final_commit_are_fail_closed() {
        let full_below = attacker_session(3, 8, 2);
        let fast_below = fast_attacker_session(3, 8, 2);
        let RlSessionResponseV1::Terminal(full_terminal) = full_below.current_response() else {
            panic!("full cap fixture must terminate");
        };
        let FastActorResponseV1::Terminal(fast_terminal) = fast_below.current_response() else {
            panic!("fast cap fixture must terminate");
        };
        assert_eq!(fast_terminal, full_terminal);
        assert!(!fast_below.surface.scan_active());

        let mut session = fast_attacker_session(3, 8, 3);
        let response_before = session.current_response();
        let state_before = session.state.clone();
        let surface_before = session.surface.clone();
        let binding_before = session.privileged_core_environment_hash();
        let error = session.step(23, 0, u32::MAX).unwrap_err();
        assert_eq!(error.code, RlSessionErrorCode::SelectedIndexOutOfRange);
        assert_eq!(session.current_response(), response_before);
        assert_eq!(session.state, state_before);
        assert_eq!(
            session.surface.harness_public_context(),
            surface_before.harness_public_context()
        );
        assert_eq!(session.privileged_core_environment_hash(), binding_before);

        let snapshot = session.snapshot_v1();
        session.current.as_mut().unwrap().environment_revision += 1;
        let error = session.step(23, 0, u32::MAX).unwrap_err();
        assert_eq!(error.code, RlSessionErrorCode::StaleEnvironmentBinding);
        session.restore_v1(&snapshot);
        let advanced = session.step(23, 0, 1).unwrap();
        let FastActorResponseV1::Decision(advanced) = advanced else {
            panic!("first combat answer must expose the second substep");
        };
        assert_eq!(advanced.step, 1);
        assert_eq!(advanced.physical_decision_id, 0);
        assert_eq!(advanced.substep_index, 1);
        let advanced_hash = session.privileged_core_environment_hash();
        session.restore_v1(&snapshot);
        assert_eq!(
            session.step(23, 0, 1).unwrap(),
            FastActorResponseV1::Decision(advanced)
        );
        assert_eq!(session.privileged_core_environment_hash(), advanced_hash);

        session.restore_v1(&snapshot);
        session.environment_revision = u64::MAX;
        session.current.as_mut().unwrap().environment_revision = u64::MAX;
        let overflow_before = session.privileged_core_environment_hash();
        let error = session.step(23, 0, u32::MAX).unwrap_err();
        assert_eq!(error.code, RlSessionErrorCode::StaleEnvironmentBinding);
        assert_eq!(error.message, "owned environment revision exhausted");
        assert_eq!(session.privileged_core_environment_hash(), overflow_before);

        let mut final_commit = fast_attacker_session(1, 8, 8);
        let final_snapshot = final_commit.snapshot_v1();
        final_commit.state.players[0].battlefield.clear();
        let tampered_state = final_commit.state.clone();
        let response_before = final_commit.current_response();
        let audit_before = fast_actor_audit_bytes(&final_commit);
        let revision_before = final_commit.environment_revision;
        let error = final_commit.step(23, 0, 1).unwrap_err();
        assert_eq!(error.code, RlSessionErrorCode::StaleEnvironmentBinding);
        assert_eq!(final_commit.environment_revision, revision_before);
        assert_eq!(final_commit.policy_step_count, 0);
        assert_eq!(final_commit.physical_decision_count, 0);
        assert_eq!(final_commit.state, tampered_state);
        assert_eq!(final_commit.current_response(), response_before);
        assert_eq!(fast_actor_audit_bytes(&final_commit), audit_before);
        final_commit.restore_v1(&final_snapshot);
        assert!(final_commit.step(23, 0, 1).is_ok());
    }

    #[test]
    fn fast_actor_final_blocker_tamper_is_nonpanicking_and_byte_atomic() {
        let mut session = fast_blocker_session(1, 8, 8);
        let snapshot = session.snapshot_v1();
        let blocker = session.state.players[1].battlefield[0];
        session.state.players[1].battlefield.clear();
        session.state.objects.get_mut(blocker).zone = Zone::Graveyard;
        let response_before = session.current_response();
        let audit_before = fast_actor_audit_bytes(&session);

        let caught =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| session.step(23, 0, 1)));
        assert!(
            caught.is_ok(),
            "stale final blocker aggregate must not panic"
        );
        let error = caught.unwrap().unwrap_err();
        assert_eq!(error.code, RlSessionErrorCode::StaleEnvironmentBinding);
        assert_eq!(session.current_response(), response_before);
        assert_eq!(fast_actor_audit_bytes(&session), audit_before);

        session.restore_v1(&snapshot);
        assert!(session.step(23, 0, 1).is_ok());
    }

    #[test]
    fn fast_actor_rejected_inputs_are_byte_atomic_before_in_place_apply() {
        let mut session = fast_attacker_session(3, 8, 8);
        let mut candidate_payload_tamper = session.clone();
        let payload_before = fast_actor_audit_bytes(&candidate_payload_tamper);
        candidate_payload_tamper
            .current
            .as_mut()
            .unwrap()
            .candidates[0]
            .policy_action = PolicyActionV5::Surface(crate::surface_v2::SurfaceAction::Action(
            crate::engine::Action::Pass,
        ));
        assert_ne!(
            fast_actor_audit_bytes(&candidate_payload_tamper),
            payload_before,
            "audit bytes must bind the full executable candidate payload"
        );
        assert_fast_actor_rejection_is_byte_atomic(
            &mut candidate_payload_tamper,
            RlSessionErrorCode::StaleEnvironmentBinding,
            |candidate_payload_tamper| candidate_payload_tamper.step(23, 0, 0),
        );

        assert_fast_actor_rejection_is_byte_atomic(
            &mut session,
            RlSessionErrorCode::EpisodeIdMismatch,
            |session| session.step(24, 0, 0),
        );
        assert_fast_actor_rejection_is_byte_atomic(
            &mut session,
            RlSessionErrorCode::ExpectedStepMismatch,
            |session| session.step(23, 1, 0),
        );
        assert_fast_actor_rejection_is_byte_atomic(
            &mut session,
            RlSessionErrorCode::SelectedIndexOutOfRange,
            |session| session.step(23, 0, u32::MAX),
        );

        let mut stale = session.clone();
        stale.current.as_mut().unwrap().environment_revision += 1;
        assert_fast_actor_rejection_is_byte_atomic(
            &mut stale,
            RlSessionErrorCode::StaleEnvironmentBinding,
            |stale| stale.step(23, 0, u32::MAX),
        );

        let mut revision_overflow = session.clone();
        revision_overflow.environment_revision = u64::MAX;
        revision_overflow
            .current
            .as_mut()
            .unwrap()
            .environment_revision = u64::MAX;
        assert_fast_actor_rejection_is_byte_atomic(
            &mut revision_overflow,
            RlSessionErrorCode::StaleEnvironmentBinding,
            |revision_overflow| revision_overflow.step(23, 0, u32::MAX),
        );

        let mut policy_overflow = session.clone();
        policy_overflow.policy_step_count = u64::MAX;
        policy_overflow
            .current
            .as_mut()
            .unwrap()
            .bound_policy_step_count = u64::MAX;
        assert_fast_actor_rejection_is_byte_atomic(
            &mut policy_overflow,
            RlSessionErrorCode::StaleEnvironmentBinding,
            |policy_overflow| policy_overflow.step(23, u64::MAX, u32::MAX),
        );

        let mut physical_overflow = fast_attacker_session(1, u64::MAX, 8);
        physical_overflow.physical_decision_count = u64::MAX;
        physical_overflow
            .current
            .as_mut()
            .unwrap()
            .bound_physical_decision_count = u64::MAX;
        assert_fast_actor_rejection_is_byte_atomic(
            &mut physical_overflow,
            RlSessionErrorCode::StaleEnvironmentBinding,
            |physical_overflow| physical_overflow.step(23, 0, u32::MAX),
        );

        session.step(23, 0, 1).unwrap();
        assert_fast_actor_rejection_is_byte_atomic(
            &mut session,
            RlSessionErrorCode::ExpectedStepMismatch,
            |session| session.step(23, 0, 1),
        );

        let mut capped = fast_attacker_session(3, 0, 8);
        assert_fast_actor_rejection_is_byte_atomic(
            &mut capped,
            RlSessionErrorCode::EpisodeAlreadyTerminal,
            |capped| capped.step(23, 0, 0),
        );
    }

    #[test]
    fn fast_actor_candidate_proof_cannot_cross_same_revision_surfaces() {
        let source = fast_attacker_session(3, 8, 8);
        let current = source.current.as_ref().unwrap();
        let proof = FastActorCurrentCandidateProofV1::from_current(
            &source.surface,
            current,
            1,
            source.environment_revision + 1,
        )
        .unwrap();
        let mut other = source.clone();
        let before = fast_actor_audit_bytes(&other);

        let error = other
            .surface
            .apply_fast_actor_current_candidate_in_place(&mut other.state, proof)
            .unwrap_err();
        assert_eq!(error, FastActorInPlaceApplyErrorV1::RejectedBeforeMutation);
        assert_eq!(fast_actor_audit_bytes(&other), before);
    }

    #[test]
    fn fast_actor_reset_and_steps_materialize_zero_observations_or_stable_ids() {
        reset_test_policy_v5_materialization_calls();
        let mut fast = fast_attacker_session(3, 8, 8);
        assert_eq!(test_policy_v5_materialization_calls(), (0, 0));
        for step in 0..3 {
            fast.step(23, step, 1).unwrap();
        }
        assert_eq!(test_policy_v5_materialization_calls(), (0, 0));

        let _full = attacker_session(3, 8, 8);
        let (observations, stable_actions) = test_policy_v5_materialization_calls();
        assert!(observations > 0);
        assert!(stable_actions > 0);
    }

    #[test]
    fn matched_rally_priority_menu_preserves_each_physical_mana_and_blood_source() {
        let mut state = GameState::new_from_libraries(&[], &[], card_name, 91);
        let mountain_a = add_battlefield_object(&mut state, PlayerId::P0, "Mountain");
        let mountain_b = add_battlefield_object(&mut state, PlayerId::P0, "Mountain");
        let furnace = add_battlefield_object(&mut state, PlayerId::P0, "Great Furnace");
        let blood_a = add_battlefield_object(&mut state, PlayerId::P0, "Blood Token");
        let blood_b = add_battlefield_object(&mut state, PlayerId::P0, "Blood Token");
        let decision =
            PolicyDecisionV5::Surface(SurfaceDecision::Decision(Decision::CastSpellOrPass {
                player: PlayerId::P0,
                castable_spells: Vec::new(),
                mana_abilities: vec![mountain_a, mountain_b, furnace],
                land_drops: Vec::new(),
                activatable_abilities: vec![(blood_a, 0), (blood_b, 0)],
                plot_actions: Vec::new(),
            }));
        let candidates = core_policy_action_candidates_v5(&decision, &state).unwrap();
        let mana_sources: Vec<_> = candidates
            .iter()
            .filter_map(|candidate| match &candidate.semantic {
                ActionSemanticV1::ActivateManaAbility { source, .. } => Some(source.arena_id),
                _ => None,
            })
            .collect();
        let blood_sources: Vec<_> = candidates
            .iter()
            .filter_map(|candidate| match &candidate.semantic {
                ActionSemanticV1::ActivateAbility { source, .. } => Some(source.arena_id),
                _ => None,
            })
            .collect();
        assert_eq!(
            mana_sources,
            vec![mountain_a.0, mountain_b.0, furnace.0],
            "two Mountains and Great Furnace must remain separate canonical actions"
        );
        assert_eq!(
            blood_sources,
            vec![blood_a.0, blood_b.0],
            "each Blood token must remain a separate canonical source action"
        );
        assert_eq!(
            candidates
                .iter()
                .map(|candidate| &candidate.semantic)
                .collect::<HashSet<_>>()
                .len(),
            candidates.len(),
            "source-distinct actions must not collapse semantically"
        );
    }

    #[test]
    fn shared_core_and_full_v5_fail_identically_on_forbidden_or_duplicate_semantics() {
        reset_test_policy_v5_materialization_calls();
        let state = attacker_state(1);
        let object = state.players[0].battlefield[0];

        let legacy =
            PolicyDecisionV5::Surface(SurfaceDecision::Decision(Decision::DeclareAttackers {
                player: PlayerId::P0,
                eligible: vec![object],
            }));
        let fast_legacy = core_policy_action_candidates_v5(&legacy, &state).unwrap_err();
        assert_eq!(test_policy_v5_materialization_calls(), (0, 0));
        let full_legacy = legal_action_candidates_v5(&legacy, &state).unwrap_err();
        assert_eq!(fast_legacy, full_legacy);
        assert_eq!(
            fast_legacy.0,
            "legacy aggregate combat semantic is forbidden on policy surface v5"
        );
        assert_eq!(test_policy_v5_materialization_calls(), (0, 0));

        let duplicate =
            PolicyDecisionV5::Surface(SurfaceDecision::Decision(Decision::CastSpellOrPass {
                player: PlayerId::P0,
                castable_spells: vec![object, object],
                mana_abilities: Vec::new(),
                land_drops: Vec::new(),
                activatable_abilities: Vec::new(),
                plot_actions: Vec::new(),
            }));
        let fast_duplicate = core_policy_action_candidates_v5(&duplicate, &state).unwrap_err();
        assert_eq!(test_policy_v5_materialization_calls(), (0, 0));
        let full_duplicate = legal_action_candidates_v5(&duplicate, &state).unwrap_err();
        assert_eq!(fast_duplicate, full_duplicate);
        assert_eq!(
            fast_duplicate.0,
            "duplicate policy action semantic within one decision"
        );
        assert_eq!(test_policy_v5_materialization_calls(), (0, 0));

        let ambiguous_semantic = ActionSemanticV1::Ambiguous {
            reason: "audit_fixture".to_string(),
        };
        let ambiguous_core = [CorePolicyActionCandidateV1 {
            semantic: ambiguous_semantic.clone(),
            policy_action: crate::policy_surface_v5::PolicyActionV5::Surface(
                crate::surface_v2::SurfaceAction::Action(crate::engine::Action::Pass),
            ),
        }];
        let fast_ambiguous =
            validate_core_policy_action_candidates_v5(&ambiguous_core).unwrap_err();
        assert_eq!(test_policy_v5_materialization_calls(), (0, 0));
        let full_ambiguous = make_legal_action_v5(0, ambiguous_semantic, None).unwrap_err();
        assert_eq!(fast_ambiguous, full_ambiguous);
        assert_eq!(
            fast_ambiguous.0,
            "ambiguous legal action representation refused: audit_fixture"
        );
        assert_eq!(test_policy_v5_materialization_calls(), (0, 1));
    }

    fn prove_fast_actor_parity(deck_id: &str, seed: u64) {
        let episode_id = seed ^ 0xFA57_AC70_0000_0001;
        let deck_ids = [deck_id.to_string(), deck_id.to_string()];
        let mut full = RlEpisodeSessionV1::reset_with_decks_and_limits(
            episode_id,
            seed,
            4_096,
            524_288,
            deck_ids.clone(),
        )
        .unwrap();
        // FastActorResponseV1 deliberately has no serde/wire materialization.
        // JSONL bytes are therefore stabilized by an independent full-v5
        // replay, while the fast lane is compared below at exact core state,
        // candidate payload/order, counters, and terminal outcome.
        let mut full_jsonl_replay = RlEpisodeSessionV1::reset_with_decks_and_limits(
            episode_id,
            seed,
            4_096,
            524_288,
            deck_ids.clone(),
        )
        .unwrap();
        let mut fast = FastActorSessionV1::reset_with_decks_and_limits(
            episode_id, seed, 4_096, 524_288, deck_ids,
        )
        .unwrap();
        let mut clone_reference = fast.clone();
        let mut policy_rng = SplitMix64::seed(seed ^ 0xA66A_E6A7_E000_0005);
        let mut saw_attacker_inclusion = false;
        let mut saw_blocker_inclusion = false;

        for _ in 0..524_289 {
            let full_response = full.current_response();
            assert_eq!(
                serde_json::to_vec(&full_jsonl_replay.current_response()).unwrap(),
                serde_json::to_vec(&full_response).unwrap(),
                "{deck_id} independent full-v5 JSONL replay diverged before selection"
            );
            assert_eq!(
                fast.current_response(),
                clone_reference.current_response(),
                "{deck_id} in-place response diverged from clone reference"
            );
            assert_eq!(
                fast.state, clone_reference.state,
                "{deck_id} in-place state diverged from clone reference"
            );
            assert_eq!(
                fast.surface.harness_public_context(),
                clone_reference.surface.harness_public_context(),
                "{deck_id} in-place H2 context diverged from clone reference"
            );
            assert_eq!(
                fast.surface.privileged_scan_context().unwrap(),
                clone_reference.surface.privileged_scan_context().unwrap(),
                "{deck_id} in-place scan context diverged from clone reference"
            );
            assert_eq!(
                fast.privileged_core_environment_hash(),
                clone_reference.privileged_core_environment_hash(),
                "{deck_id} in-place core hash diverged from clone reference"
            );
            assert_eq!(full.state, fast.state, "{deck_id} state diverged");
            assert_eq!(
                full.surface.harness_public_context(),
                fast.surface.harness_public_context(),
                "{deck_id} public surface context diverged"
            );
            assert_eq!(
                full.surface.privileged_scan_context().unwrap(),
                fast.surface.privileged_scan_context().unwrap(),
                "{deck_id} private scan context diverged"
            );
            assert_eq!(full.environment_revision, fast.environment_revision);
            assert_eq!(full.policy_step_count, fast.policy_step_count);
            assert_eq!(full.physical_decision_count, fast.physical_decision_count);
            assert_eq!(full.diagnostic_state_hash(), fast.diagnostic_state_hash());
            assert_eq!(
                full.privileged_core_environment_hash(),
                fast.privileged_core_environment_hash(),
                "{deck_id} shared core environment hash diverged"
            );

            match (full_response, fast.current_response()) {
                (
                    RlSessionResponseV1::Terminal(full_terminal),
                    FastActorResponseV1::Terminal(fast_terminal),
                ) => {
                    assert_eq!(fast_terminal, full_terminal);
                    if deck_id == CANONICAL_RALLY_DECK_ID {
                        assert!(
                            saw_attacker_inclusion,
                            "Rally parity trace must exercise attacker decomposition"
                        );
                        assert!(
                            saw_blocker_inclusion,
                            "Rally parity trace must exercise blocker decomposition"
                        );
                    }
                    return;
                }
                (
                    RlSessionResponseV1::Decision(full_decision),
                    FastActorResponseV1::Decision(fast_decision),
                ) => {
                    assert_eq!(fast_decision.episode_id, full_decision.episode_id);
                    assert_eq!(fast_decision.step, full_decision.step);
                    assert_eq!(
                        fast_decision.physical_decision_id,
                        full_decision.physical_decision_id
                    );
                    assert_eq!(fast_decision.substep_index, full_decision.substep_index);
                    assert_eq!(fast_decision.substep_count, full_decision.substep_count);
                    assert_eq!(fast_decision.acting_player, full_decision.acting_player);
                    assert_eq!(
                        usize::try_from(fast_decision.legal_action_count).unwrap(),
                        full_decision.legal_actions.len()
                    );

                    let full_current = full.current.as_ref().unwrap();
                    let fast_current = fast.current.as_ref().unwrap();
                    assert_eq!(full_current.candidates.len(), fast_current.candidates.len());
                    for (wire, core) in full_current.candidates.iter().zip(&fast_current.candidates)
                    {
                        assert_eq!(wire.record.semantic, core.semantic);
                        assert_eq!(wire.policy_action, core.policy_action);
                    }
                    saw_attacker_inclusion |= fast_current.candidates.iter().any(|candidate| {
                        matches!(
                            candidate.semantic,
                            ActionSemanticV1::ChooseAttackerInclusion { .. }
                        )
                    });
                    saw_blocker_inclusion |= fast_current.candidates.iter().any(|candidate| {
                        matches!(
                            candidate.semantic,
                            ActionSemanticV1::ChooseBlockerInclusion { .. }
                        )
                    });
                    let selected_index = if matches!(
                        fast_current
                            .candidates
                            .get(1)
                            .map(|candidate| &candidate.semantic),
                        Some(
                            ActionSemanticV1::ChooseAttackerInclusion { include: true, .. }
                                | ActionSemanticV1::ChooseBlockerInclusion { include: true, .. }
                        )
                    ) {
                        1usize
                    } else {
                        (policy_rng.next_u64() % fast_current.candidates.len() as u64) as usize
                    };
                    let selected = &full_decision.legal_actions[selected_index];
                    let full_next = full
                        .step(
                            episode_id,
                            full_decision.step,
                            selected.selected_index,
                            &selected.stable_id,
                        )
                        .unwrap();
                    let replay_next = full_jsonl_replay
                        .step(
                            episode_id,
                            full_decision.step,
                            selected.selected_index,
                            &selected.stable_id,
                        )
                        .unwrap();
                    assert_eq!(
                        serde_json::to_vec(&replay_next).unwrap(),
                        serde_json::to_vec(&full_next).unwrap(),
                        "{deck_id} independent full-v5 JSONL replay diverged after selection"
                    );
                    let fast_next = fast
                        .step(
                            episode_id,
                            fast_decision.step,
                            u32::try_from(selected_index).unwrap(),
                        )
                        .unwrap();
                    let reference_next = clone_reference
                        .step_clone_reference(
                            episode_id,
                            fast_decision.step,
                            u32::try_from(selected_index).unwrap(),
                        )
                        .unwrap();
                    assert_eq!(
                        fast_next, reference_next,
                        "{deck_id} in-place next response diverged from clone reference"
                    );
                }
                _ => panic!("{deck_id} full and fast terminal states diverged"),
            }
        }
        panic!("{deck_id} fast actor parity trace exceeded its policy-step bound");
    }

    #[test]
    fn fast_actor_matches_full_v5_core_and_full_replay_jsonl_is_stable() {
        prove_fast_actor_parity(CANONICAL_BURN_DECK_ID, 81_701);
        prove_fast_actor_parity(CANONICAL_RALLY_DECK_ID, 81_702);
    }

    fn reset_in_suppression_audit_mode(
        episode_id: u64,
        env_seed: u64,
        deck_id: &str,
        mode: SuppressionAuditMode,
    ) -> RlEpisodeSessionV1 {
        RlEpisodeSessionV1::reset_with_decks_and_limits_profiled_in_audit_mode(
            episode_id,
            env_seed,
            4_096,
            524_288,
            [deck_id.to_string(), deck_id.to_string()],
            None,
            mode,
        )
        .unwrap()
    }

    fn prove_suppression_audit_mode_equivalence(deck_id: &str, seed: u64) {
        let episode_id = seed ^ 0xA11D_1700_0000_0001;
        let mut sessions = [
            reset_in_suppression_audit_mode(episode_id, seed, deck_id, SuppressionAuditMode::Full),
            reset_in_suppression_audit_mode(
                episode_id,
                seed,
                deck_id,
                SuppressionAuditMode::CountOnly,
            ),
            reset_in_suppression_audit_mode(episode_id, seed, deck_id, SuppressionAuditMode::Off),
        ];
        let mut policy_rng = SplitMix64::seed(seed ^ 0x50A1_CE00_0000_0005);
        let mut reached_terminal = false;
        let mut policy_steps_seen = 0u64;
        let mut midrun_hash_checkpoint_seen = false;

        for _ in 0..524_289 {
            let reference = sessions[0].current_response();
            let reference_bytes = serde_json::to_vec(&reference).unwrap();
            let reference_context = sessions[0].surface.harness_public_context();
            let compare_privileged_hashes = policy_steps_seen == 0
                || policy_steps_seen == 32
                || matches!(&reference, RlSessionResponseV1::Terminal(_));
            let reference_hashes = compare_privileged_hashes.then(|| {
                (
                    sessions[0].diagnostic_state_hash(),
                    sessions[0].privileged_environment_hash(),
                )
            });
            midrun_hash_checkpoint_seen |= policy_steps_seen == 32;

            for (index, session) in sessions.iter().enumerate().skip(1) {
                assert_eq!(
                    serde_json::to_vec(&session.current_response()).unwrap(),
                    reference_bytes,
                    "{deck_id} profile-off session response bytes diverged in audit mode {index}"
                );
                assert_eq!(
                    session.surface.harness_public_context(),
                    reference_context,
                    "{deck_id} public H2 context diverged in audit mode {index}"
                );
                if let Some((reference_state_hash, reference_environment_hash)) = reference_hashes {
                    assert_eq!(
                        session.diagnostic_state_hash(),
                        reference_state_hash,
                        "{deck_id} diagnostic state hash diverged in audit mode {index}"
                    );
                    assert_eq!(
                        session.privileged_environment_hash(),
                        reference_environment_hash,
                        "{deck_id} stable action/environment binding diverged in audit mode {index}"
                    );
                }
            }

            let RlSessionResponseV1::Decision(decision) = reference else {
                reached_terminal = true;
                break;
            };
            assert!(!decision.legal_actions.is_empty());
            let selected_index =
                (policy_rng.next_u64() % decision.legal_actions.len() as u64) as usize;
            let selected = &decision.legal_actions[selected_index];
            let expected_next = sessions[0]
                .step(
                    episode_id,
                    decision.step,
                    selected.selected_index,
                    &selected.stable_id,
                )
                .unwrap();
            let expected_next_bytes = serde_json::to_vec(&expected_next).unwrap();
            for (index, session) in sessions.iter_mut().enumerate().skip(1) {
                let next = session
                    .step(
                        episode_id,
                        decision.step,
                        selected.selected_index,
                        &selected.stable_id,
                    )
                    .unwrap();
                assert_eq!(
                    serde_json::to_vec(&next).unwrap(),
                    expected_next_bytes,
                    "{deck_id} decisions, actions, or stable ids diverged in audit mode {index}"
                );
            }
            policy_steps_seen = policy_steps_seen.saturating_add(1);
        }
        assert!(
            reached_terminal,
            "{deck_id} fixture did not reach a terminal response"
        );
        assert!(
            midrun_hash_checkpoint_seen,
            "{deck_id} fixture did not reach its privileged mid-run hash checkpoint"
        );

        let full_surface = sessions[0].surface.harness_surface();
        let count_surface = sessions[1].surface.harness_surface();
        let off_surface = sessions[2].surface.harness_surface();
        assert_eq!(
            count_surface.suppression_counts(),
            full_surface.suppression_counts(),
            "{deck_id} CountOnly reason counts must exactly match Full records"
        );
        assert_eq!(
            full_surface.suppression_counts().total(),
            full_surface.suppressions().len() as u64
        );
        assert!(
            !full_surface.suppressions().is_empty(),
            "{deck_id} fixture must exercise at least one suppression"
        );
        assert!(count_surface.suppressions().is_empty());
        assert_eq!(off_surface.suppression_counts().total(), 0);
        assert!(off_surface.suppressions().is_empty());
        assert_eq!(
            sessions[1].diagnostic_state_hash(),
            sessions[0].diagnostic_state_hash(),
            "{deck_id} terminal state hash differs for CountOnly"
        );
        assert_eq!(
            sessions[2].diagnostic_state_hash(),
            sessions[0].diagnostic_state_hash(),
            "{deck_id} terminal state hash differs for Off"
        );
    }

    #[test]
    fn suppression_audit_modes_are_byte_and_semantics_inert_for_burn_and_rally() {
        prove_suppression_audit_mode_equivalence(CANONICAL_BURN_DECK_ID, 71_501);
        prove_suppression_audit_mode_equivalence(CANONICAL_RALLY_DECK_ID, 71_502);
    }

    #[test]
    fn suppression_audit_mode_does_not_change_integrity_error_precedence() {
        for mode in [
            SuppressionAuditMode::Full,
            SuppressionAuditMode::CountOnly,
            SuppressionAuditMode::Off,
        ] {
            let episode_id = 71_503;
            let mut session =
                reset_in_suppression_audit_mode(episode_id, 19_991, CANONICAL_RALLY_DECK_ID, mode);
            let RlSessionResponseV1::Decision(decision) = session.current_response() else {
                panic!("Rally fixture must begin with a decision");
            };
            session.current.as_mut().unwrap().environment_revision += 1;
            let error = session
                .step(episode_id, decision.step, u32::MAX, "unknown-action")
                .unwrap_err();
            assert_eq!(error.code, RlSessionErrorCode::StaleEnvironmentBinding);
        }
    }

    fn flat_test_ref(state: &GameState, id: ObjectId) -> CardStableRefV1 {
        let object = state.objects.get(id);
        CardStableRefV1 {
            arena_id: id.0,
            card_db_id: object.card_def,
            owner: object.owner.into(),
            controller: object.controller.into(),
            zone: object.zone,
            zone_change_count: object.zone_change_count,
        }
    }

    fn flat_current_decision(session: &FastActorSessionV1) -> FastActorDecisionV1 {
        match session.current_response() {
            FastActorResponseV1::Decision(decision) => decision,
            FastActorResponseV1::Terminal(_) => {
                panic!("flat action fixture unexpectedly terminated")
            }
        }
    }

    fn flat_install_origin_decision(
        session: &mut FastActorSessionV1,
        origin_decision: PolicyDecisionV5,
    ) {
        let actor = origin_decision
            .actor(&session.state)
            .expect("flat origin fixture must be nonterminal");
        let (substep_index, substep_count) = origin_decision.substep();
        let decision_kind = match &origin_decision {
            PolicyDecisionV5::Surface(_) => FastActorDecisionKindV1::Surface,
            PolicyDecisionV5::AttackerInclusion { .. } => {
                FastActorDecisionKindV1::AttackerInclusion
            }
            PolicyDecisionV5::BlockerInclusion { .. } => FastActorDecisionKindV1::BlockerInclusion,
        };
        let candidates = core_policy_action_candidates_v5(&origin_decision, &session.state)
            .expect("flat origin fixture must produce canonical candidates");
        let current = session.current.as_mut().expect("flat fixture is active");
        current.actor = actor;
        current.decision_kind = decision_kind;
        current.origin_decision = origin_decision;
        current.substep_index = substep_index;
        current.substep_count = substep_count;
        current.candidates = candidates;
        flat_refresh_action_cache_fixture(session);
    }

    fn flat_refresh_action_cache_fixture(session: &mut FastActorSessionV1) {
        let mut current = session.current.take().expect("flat fixture is active");
        let reusable_cache = current.flat_action_cache.take();
        let cache = flat_build_action_cache_v1(session, &current, reusable_cache)
            .expect("flat fixture must produce a valid private cache");
        current.flat_action_cache = Some(cache);
        current.flat_action_cache_error = None;
        session.current = Some(current);
    }

    #[test]
    fn flat_action_slice_encodes_and_consumes_detached_highway_robbery_cost_target() {
        let mut state = GameState::new_from_libraries(&[], &[], card_name, 82_120);
        let source_card = card_id_by_name("Highway Robbery").unwrap();
        let source = state.objects.push(GameObject {
            card_def: source_card,
            name: "Highway Robbery".to_string(),
            owner: PlayerId::P0,
            controller: PlayerId::P0,
            zone: Zone::Stack,
            tapped: false,
            summoning_sick: false,
            damage: 0,
            counters: Counters::default(),
            attachments: Vec::new(),
            v4: ObjectStateV4::from_card_def(source_card),
            spell_copy_origin: None,
            plotted_turn: None,
            zone_change_count: 2,
        });
        let first_land = add_battlefield_object(&mut state, PlayerId::P0, "Mountain");
        let second_land = add_battlefield_object(&mut state, PlayerId::P0, "Mountain");
        state.engine.pending_optional_cost_sacrifice = Some(PendingOptionalCostSacrifice {
            player: PlayerId::P0,
            source,
            remaining: 1,
            chosen: Vec::new(),
            then: EffectOp::Sequence(Vec::new()),
            spell_resume: Some((source, Zone::Graveyard)),
        });
        assert!(state.stack.is_empty());

        let mut session = FastActorSessionV1::reset_with_limits(82_120, 41_120, 256, 32_768);
        session.state = state;
        session.surface = PolicySurfaceV5::new();
        session.environment_revision = 0;
        session.policy_step_count = 0;
        session.physical_decision_count = 0;
        session.current = None;
        session.flat_action_cache_spare = None;
        session.terminal = None;
        session.advance_to_decision_or_terminal();

        let decision = flat_current_decision(&session);
        assert_eq!(decision.legal_action_count, 2);
        let current = session.current.as_ref().unwrap();
        assert_eq!(current.flat_action_cache_error, None);
        let cache = current
            .flat_action_cache
            .as_ref()
            .expect("detached resolving source must be representable");
        assert_eq!(
            cache,
            &flat_action_reference_materialization_v1(&session, current)
                .expect("frozen on-demand materialization must admit the same rows")
        );

        let mut actions = [FlatActionCoreV1::default(); 4];
        let mut refs = [FlatActionRefV1::default(); 8];
        let mut objects = [FlatActionObjectV1::default(); 4];
        let encoded = session
            .encode_current_flat_action_slice_v1(
                decision,
                &mut FlatActionDecisionSliceBuffersV1 {
                    actions: &mut actions,
                    refs: &mut refs,
                    objects: &mut objects,
                },
            )
            .unwrap();
        assert_eq!(encoded.active_action_count, 2);
        assert_eq!(encoded.active_ref_count, 4);
        assert_eq!(encoded.active_object_count, 3);
        assert!(actions[..2]
            .iter()
            .all(|action| action.kind == FlatActionKindV1::ChooseCostTarget));
        let source_rows: Vec<_> = refs[..4]
            .iter()
            .filter(|reference| reference.role == FlatActionRefRoleV1::Source)
            .map(|reference| objects[usize::from(reference.object_index)])
            .collect();
        assert_eq!(source_rows.len(), 2);
        assert!(source_rows.iter().all(|object| {
            object.group == FlatActionObjectGroupV1::Stack
                && object.actor_visible_ordinal == 0
                && object.zone == flat_zone_v1(Zone::Stack)
        }));

        let mut mismatched = session.clone();
        mismatched
            .state
            .engine
            .pending_optional_cost_sacrifice
            .as_mut()
            .unwrap()
            .spell_resume = None;
        let before_state = mismatched.state.clone();
        let before_response = mismatched.current_response();
        let encode_error = mismatched
            .encode_current_flat_action_slice_v1(
                decision,
                &mut FlatActionDecisionSliceBuffersV1 {
                    actions: &mut actions,
                    refs: &mut refs,
                    objects: &mut objects,
                },
            )
            .unwrap_err();
        assert_eq!(
            encode_error,
            FlatActionDecisionSliceErrorV1::InvalidActionReference
        );
        let error = mismatched
            .consume_current_flat_action_slice_v1(encoded.binding, 0)
            .unwrap_err();
        assert_eq!(error.code, RlSessionErrorCode::StaleEnvironmentBinding);
        assert_eq!(mismatched.state, before_state);
        assert_eq!(mismatched.current_response(), before_response);

        session
            .consume_current_flat_action_slice_v1(encoded.binding, 0)
            .unwrap();
        assert_eq!(session.state.objects.get(source).zone, Zone::Graveyard);
        assert!(session.state.players[0].graveyard.contains(&source));
        assert!(
            !session.state.players[0].battlefield.contains(&first_land)
                ^ !session.state.players[0].battlefield.contains(&second_land)
        );
    }

    #[test]
    fn flat_action_refs_only_cache_matches_frozen_arena_scan_reference() {
        let mut checked = 0_usize;
        let mut adversarial_fixture = None;
        for episode_offset in 0..8_u64 {
            let mut session = FastActorSessionV1::reset_with_decks_and_limits(
                82_100 + episode_offset,
                41_000 + episode_offset,
                256,
                32_768,
                [
                    CANONICAL_RALLY_DECK_ID.to_string(),
                    CANONICAL_RALLY_DECK_ID.to_string(),
                ],
            )
            .unwrap();
            for decision_index in 0..128_u64 {
                let FastActorResponseV1::Decision(decision) = session.current_response() else {
                    break;
                };
                let current = session.current.as_ref().unwrap();
                let reference = flat_action_reference_materialization_v1(&session, current)
                    .expect("frozen whole-arena reference must admit the live decision");
                let cached = current
                    .flat_action_cache
                    .as_ref()
                    .expect("live decision owns its private cache");
                assert_eq!(cached, &reference, "decision {checked}");
                flat_validate_action_cache_v1(&session, current, cached)
                    .expect("refs-only validation must admit the reference-equivalent cache");
                if adversarial_fixture.is_none() && !cached.refs.is_empty() {
                    adversarial_fixture = Some(session.clone());
                }
                let binding = cached.binding;
                checked += 1;
                let selected = u32::try_from(
                    (episode_offset * 131 + decision_index * 17)
                        % u64::from(decision.legal_action_count),
                )
                .unwrap();
                if matches!(
                    session
                        .consume_current_flat_action_slice_v1(binding, selected)
                        .unwrap(),
                    FastActorResponseV1::Terminal(_)
                ) {
                    break;
                }
            }
        }
        assert!(
            checked >= 128,
            "insufficient live Rally parity coverage: {checked}"
        );

        let mut corrupted = adversarial_fixture.expect("Rally parity walk must reach a reference");
        let referenced_id = {
            let current = corrupted.current.as_ref().unwrap();
            let actor: PlayerSeatV1 = current.actor.into();
            let mut referenced_id = None;
            for candidate in &current.candidates {
                flat_action_core_and_refs_v1(
                    &candidate.semantic,
                    actor,
                    0,
                    |_, _, _, reference| {
                        referenced_id.get_or_insert(ObjectId(reference.arena_id));
                        Ok(())
                    },
                )
                .unwrap();
            }
            referenced_id.expect("selected fixture has at least one reference")
        };
        let corrupted_card = corrupted
            .state
            .objects
            .get(referenced_id)
            .card_def
            .wrapping_add(1);
        corrupted.state.objects.get_mut(referenced_id).card_def = corrupted_card;
        let current = corrupted.current.as_ref().unwrap();
        let reference_error = flat_action_reference_preflight_v1(&corrupted, current).unwrap_err();
        let cached_error = flat_validate_action_cache_v1(
            &corrupted,
            current,
            current.flat_action_cache.as_ref().unwrap(),
        )
        .unwrap_err();
        assert_eq!(cached_error, reference_error);
        assert_eq!(
            cached_error,
            FlatActionDecisionSliceErrorV1::InvalidActionReference
        );
    }

    #[test]
    fn flat_action_encode_and_consume_do_not_rehash_the_current_decision() {
        let mut session = FastActorSessionV1::reset_with_decks_and_limits(
            82_109,
            41_109,
            256,
            32_768,
            [
                CANONICAL_RALLY_DECK_ID.to_string(),
                CANONICAL_RALLY_DECK_ID.to_string(),
            ],
        )
        .unwrap();
        let decision = flat_current_decision(&session);
        let mut actions = [FlatActionCoreV1::default(); 64];
        let mut refs = [FlatActionRefV1::default(); 256];
        let mut objects = [FlatActionObjectV1::default(); 128];

        reset_test_flat_action_commitment_constructions();
        let mut encoded = None;
        for _ in 0..16 {
            encoded = Some(
                session
                    .encode_current_flat_action_slice_v1(
                        decision,
                        &mut FlatActionDecisionSliceBuffersV1 {
                            actions: &mut actions,
                            refs: &mut refs,
                            objects: &mut objects,
                        },
                    )
                    .unwrap(),
            );
        }
        assert_eq!(test_flat_action_commitment_constructions(), 0);

        let probed = session
            .diagnostic_recompute_flat_action_commitment_v1()
            .unwrap();
        assert_eq!(probed, encoded.unwrap().binding.candidate_order_commitment);
        assert_eq!(test_flat_action_commitment_constructions(), 1);

        reset_test_flat_action_commitment_constructions();
        let binding = encoded.unwrap().binding;
        let response = session
            .consume_current_flat_action_slice_v1(
                binding,
                binding.legal_action_count.saturating_sub(1),
            )
            .unwrap();
        assert!(matches!(response, FastActorResponseV1::Decision(_)));
        // One hash constructs the newly published next decision. The consumed
        // decision's cached v1 commitment was not recomputed.
        assert_eq!(test_flat_action_commitment_constructions(), 1);
    }

    fn flat_test_core(
        semantic: &ActionSemanticV1,
        actor: PlayerSeatV1,
    ) -> Result<FlatActionCoreV1, FlatActionDecisionSliceErrorV1> {
        flat_action_core_and_refs_v1(semantic, actor, 0, |_, _, _, _| Ok(()))
    }

    fn poison_flat_action() -> FlatActionCoreV1 {
        FlatActionCoreV1 {
            kind: FlatActionKindV1::OrderTriggers,
            flags: u16::MAX,
            ability_index: u8::MAX,
            remaining: u8::MAX,
            mode_index: u8::MAX,
            mode_count: u8::MAX,
            option_index: u16::MAX,
            option_count: u16::MAX,
            selected_count: u16::MAX,
            min_targets: u16::MAX,
            max_targets: u16::MAX,
            number: i32::MIN,
            minimum: i32::MIN,
            maximum: i32::MAX,
            mana_choice: u8::MAX,
            color: u8::MAX,
            cast_mode: u8::MAX,
            cost_kind: u8::MAX,
            optional_cost_choice: u8::MAX,
            target_kind: u8::MAX,
            target_player: u8::MAX,
            ref_start: u32::MAX,
            ref_len: u16::MAX,
        }
    }

    fn poison_flat_ref() -> FlatActionRefV1 {
        FlatActionRefV1 {
            action_index: u32::MAX,
            role: FlatActionRefRoleV1::PendingSources,
            order_index: u16::MAX,
            associated_order: u16::MAX,
            card_token: u16::MAX,
            object_index: u16::MAX,
        }
    }

    fn poison_flat_object() -> FlatActionObjectV1 {
        FlatActionObjectV1 {
            card_token: u16::MAX,
            group: FlatActionObjectGroupV1::Command,
            actor_visible_ordinal: u16::MAX,
            owner_relative: u8::MAX,
            controller_relative: u8::MAX,
            zone: u8::MAX,
            zone_change_count: u32::MAX,
        }
    }

    #[test]
    fn flat_action_slice_preflights_capacity_and_preserves_poisoned_tails() {
        let session = FastActorSessionV1::reset_with_limits(81_001, 91, 128, 16_384);
        let decision = flat_current_decision(&session);
        let mut actions = vec![poison_flat_action(); 64];
        let mut refs = vec![poison_flat_ref(); 256];
        let mut objects = vec![poison_flat_object(); 128];
        let action_before = actions.clone();
        let refs_before = refs.clone();
        let objects_before = objects.clone();
        let mut buffers = FlatActionDecisionSliceBuffersV1 {
            actions: &mut actions,
            refs: &mut refs,
            objects: &mut objects,
        };
        let mut stale_episode = decision;
        stale_episode.episode_id -= 1;
        assert_eq!(
            session.encode_current_flat_action_slice_v1(stale_episode, &mut buffers),
            Err(FlatActionDecisionSliceErrorV1::StaleEpisodeBinding)
        );
        assert_eq!(actions, action_before);
        assert_eq!(refs, refs_before);
        assert_eq!(objects, objects_before);

        let mut wrong_step = decision;
        wrong_step.step += 1;
        let mut wrong_physical_id = decision;
        wrong_physical_id.physical_decision_id += 1;
        let mut wrong_substep_index = decision;
        wrong_substep_index.substep_index += 1;
        let mut wrong_substep_count = decision;
        wrong_substep_count.substep_count += 1;
        let mut wrong_actor = decision;
        wrong_actor.acting_player = match wrong_actor.acting_player {
            PlayerSeatV1::P0 => PlayerSeatV1::P1,
            PlayerSeatV1::P1 => PlayerSeatV1::P0,
        };
        let mut wrong_kind = decision;
        wrong_kind.decision_kind = FastActorDecisionKindV1::AttackerInclusion;
        let mut wrong_legal_count = decision;
        wrong_legal_count.legal_action_count += 1;
        for mismatch in [
            wrong_step,
            wrong_physical_id,
            wrong_substep_index,
            wrong_substep_count,
            wrong_actor,
            wrong_kind,
            wrong_legal_count,
        ] {
            assert_eq!(
                session.encode_current_flat_action_slice_v1(
                    mismatch,
                    &mut FlatActionDecisionSliceBuffersV1 {
                        actions: &mut actions,
                        refs: &mut refs,
                        objects: &mut objects,
                    },
                ),
                Err(FlatActionDecisionSliceErrorV1::DecisionMetadataMismatch)
            );
            assert_eq!(actions, action_before);
            assert_eq!(refs, refs_before);
            assert_eq!(objects, objects_before);
        }

        let mut buffers = FlatActionDecisionSliceBuffersV1 {
            actions: &mut actions,
            refs: &mut refs,
            objects: &mut objects,
        };
        let mut stale_revision = decision;
        stale_revision.environment_revision += 1;
        assert_eq!(
            session.encode_current_flat_action_slice_v1(stale_revision, &mut buffers),
            Err(FlatActionDecisionSliceErrorV1::StaleEnvironmentRevision)
        );
        assert_eq!(actions, action_before);
        assert_eq!(refs, refs_before);
        assert_eq!(objects, objects_before);

        let mut buffers = FlatActionDecisionSliceBuffersV1 {
            actions: &mut actions,
            refs: &mut refs,
            objects: &mut objects,
        };
        let encoded = session
            .encode_current_flat_action_slice_v1(decision, &mut buffers)
            .unwrap();
        assert!(encoded.active_action_count > 0);
        assert!(encoded.active_ref_count > 0);
        assert!(encoded.active_object_count > 0);
        assert_eq!(
            &actions[encoded.active_action_count as usize..],
            &action_before[encoded.active_action_count as usize..]
        );
        assert_eq!(
            &refs[encoded.active_ref_count as usize..],
            &refs_before[encoded.active_ref_count as usize..]
        );
        assert_eq!(
            &objects[usize::from(encoded.active_object_count)..],
            &objects_before[usize::from(encoded.active_object_count)..]
        );

        let action_required = encoded.active_action_count as usize;
        let ref_required = encoded.active_ref_count as usize;
        let object_required = usize::from(encoded.active_object_count);
        for missing in 0..3 {
            let mut short_actions = vec![poison_flat_action(); action_required];
            let mut short_refs = vec![poison_flat_ref(); ref_required];
            let mut short_objects = vec![poison_flat_object(); object_required];
            match missing {
                0 => {
                    short_actions.pop();
                }
                1 => {
                    short_refs.pop();
                }
                2 => {
                    short_objects.pop();
                }
                _ => unreachable!(),
            };
            let actions_before = short_actions.clone();
            let refs_before = short_refs.clone();
            let objects_before = short_objects.clone();
            let mut buffers = FlatActionDecisionSliceBuffersV1 {
                actions: &mut short_actions,
                refs: &mut short_refs,
                objects: &mut short_objects,
            };
            let error = session
                .encode_current_flat_action_slice_v1(decision, &mut buffers)
                .unwrap_err();
            assert!(matches!(
                (missing, error),
                (
                    0,
                    FlatActionDecisionSliceErrorV1::InsufficientActionCapacity { .. }
                ) | (
                    1,
                    FlatActionDecisionSliceErrorV1::InsufficientRefCapacity { .. }
                ) | (
                    2,
                    FlatActionDecisionSliceErrorV1::InsufficientObjectCapacity { .. }
                )
            ));
            assert_eq!(short_actions, actions_before);
            assert_eq!(short_refs, refs_before);
            assert_eq!(short_objects, objects_before);
        }
    }

    #[test]
    fn flat_action_slice_consume_binds_every_field_and_snapshot_revision() {
        let mut session = FastActorSessionV1::reset_with_limits(81_029, 129, 128, 16_384);
        let decision_a = flat_current_decision(&session);
        let mut actions = [FlatActionCoreV1::default(); 64];
        let mut refs = [FlatActionRefV1::default(); 256];
        let mut objects = [FlatActionObjectV1::default(); 128];
        let encoded_a = session
            .encode_current_flat_action_slice_v1(
                decision_a,
                &mut FlatActionDecisionSliceBuffersV1 {
                    actions: &mut actions,
                    refs: &mut refs,
                    objects: &mut objects,
                },
            )
            .unwrap();
        let binding_a = encoded_a.binding;
        let response_before = session.current_response();
        let state_hash_before = session.diagnostic_state_hash();
        let environment_hash_before = session.privileged_core_environment_hash();

        let mut corruptions = Vec::new();
        macro_rules! corrupt_binding {
            ($field:ident) => {{
                let mut binding = binding_a;
                binding.$field = binding.$field.wrapping_add(1);
                corruptions.push(binding);
            }};
        }
        corrupt_binding!(slice_version);
        corrupt_binding!(ref_role_mapping_version);
        corrupt_binding!(card_token_mapping_version);
        corrupt_binding!(candidate_commitment_version);
        corrupt_binding!(card_db_hash);
        corrupt_binding!(episode_id);
        corrupt_binding!(environment_revision);
        corrupt_binding!(bound_policy_step_count);
        corrupt_binding!(physical_decision_id);
        corrupt_binding!(bound_physical_decision_count);
        corrupt_binding!(substep_index);
        corrupt_binding!(substep_count);
        corrupt_binding!(acting_player);
        corrupt_binding!(decision_kind);
        corrupt_binding!(legal_action_count);
        let mut corrupt_commitment = binding_a;
        corrupt_commitment.candidate_order_commitment[7] ^= 0x80;
        corruptions.push(corrupt_commitment);

        for binding in corruptions {
            let error = session
                .consume_current_flat_action_slice_v1(binding, 0)
                .unwrap_err();
            assert_eq!(error.code, RlSessionErrorCode::StaleEnvironmentBinding);
            assert_eq!(session.current_response(), response_before);
            assert_eq!(session.diagnostic_state_hash(), state_hash_before);
            assert_eq!(
                session.privileged_core_environment_hash(),
                environment_hash_before
            );
        }

        let error = session
            .consume_current_flat_action_slice_v1(binding_a, binding_a.legal_action_count)
            .unwrap_err();
        assert_eq!(error.code, RlSessionErrorCode::SelectedIndexOutOfRange);
        assert_eq!(session.current_response(), response_before);
        assert_eq!(session.diagnostic_state_hash(), state_hash_before);

        let snapshot_a = session.snapshot_v1();
        let selected_pass = binding_a.legal_action_count - 1;
        let expected_after_a = session
            .consume_current_flat_action_slice_v1(binding_a, selected_pass)
            .unwrap();
        let FastActorResponseV1::Decision(decision_b) = session.current_response() else {
            panic!("pass fixture unexpectedly terminated")
        };
        let encoded_b = session
            .encode_current_flat_action_slice_v1(
                decision_b,
                &mut FlatActionDecisionSliceBuffersV1 {
                    actions: &mut actions,
                    refs: &mut refs,
                    objects: &mut objects,
                },
            )
            .unwrap();
        assert_ne!(encoded_b.binding, binding_a);

        session.restore_v1(&snapshot_a);
        let restored_hash = session.privileged_core_environment_hash();
        let error = session
            .consume_current_flat_action_slice_v1(encoded_b.binding, 0)
            .unwrap_err();
        assert_eq!(error.code, RlSessionErrorCode::StaleEnvironmentBinding);
        assert_eq!(session.privileged_core_environment_hash(), restored_hash);
        assert_eq!(
            session
                .consume_current_flat_action_slice_v1(binding_a, selected_pass)
                .unwrap(),
            expected_after_a
        );
    }

    #[test]
    fn flat_action_slice_rejects_origin_context_drift_before_publish() {
        use crate::effect::EffectOp;
        use crate::trigger::PendingTrigger;

        let assert_rejected = |session: &FastActorSessionV1| {
            let mut actions = [poison_flat_action(); 16];
            let mut refs = [poison_flat_ref(); 32];
            let mut objects = [poison_flat_object(); 16];
            let actions_before = actions;
            let refs_before = refs;
            let objects_before = objects;
            assert_eq!(
                session.encode_current_flat_action_slice_v1(
                    flat_current_decision(session),
                    &mut FlatActionDecisionSliceBuffersV1 {
                        actions: &mut actions,
                        refs: &mut refs,
                        objects: &mut objects,
                    },
                ),
                Err(FlatActionDecisionSliceErrorV1::InvalidDecisionRelation)
            );
            assert_eq!(actions, actions_before);
            assert_eq!(refs, refs_before);
            assert_eq!(objects, objects_before);
        };

        let mut target_base = FastActorSessionV1::reset_with_limits(81_030, 130, 128, 16_384);
        let actor_id = target_base.current.as_ref().unwrap().actor;
        let opponent = actor_id.opponent();
        let hand = target_base.state.players[actor_id.index()].hand.clone();
        flat_install_origin_decision(
            &mut target_base,
            PolicyDecisionV5::Surface(SurfaceDecision::Decision(Decision::ChooseTargets {
                player: actor_id,
                spell: hand[0],
                remaining: 1,
                legal_targets: vec![Target::Player(opponent)],
            })),
        );
        let mut wrong_target_source = target_base.clone();
        let wrong_source = flat_test_ref(&wrong_target_source.state, hand[1]);
        let ActionSemanticV1::ChooseTarget { source, .. } =
            &mut wrong_target_source.current.as_mut().unwrap().candidates[0].semantic
        else {
            unreachable!()
        };
        *source = wrong_source;
        assert_rejected(&wrong_target_source);

        let mut wrong_remaining = target_base;
        let ActionSemanticV1::ChooseTarget { remaining, .. } =
            &mut wrong_remaining.current.as_mut().unwrap().candidates[0].semantic
        else {
            unreachable!()
        };
        *remaining = 2;
        assert_rejected(&wrong_remaining);

        let mut effect_base = FastActorSessionV1::reset_with_limits(81_031, 131, 128, 16_384);
        let actor_id = effect_base.current.as_ref().unwrap().actor;
        let hand = effect_base.state.players[actor_id.index()].hand.clone();
        flat_install_origin_decision(
            &mut effect_base,
            PolicyDecisionV5::Surface(SurfaceDecision::Decision(Decision::ChooseEffectTargets {
                player: actor_id,
                source: hand[0],
                selected_count: 0,
                min_targets: 1,
                max_targets: 2,
                legal_targets: vec![Target::Player(actor_id)],
                can_finish: false,
            })),
        );
        let mut wrong_effect_counts = effect_base.clone();
        let ActionSemanticV1::ChooseEffectTarget {
            selected_count,
            max_targets,
            ..
        } = &mut wrong_effect_counts.current.as_mut().unwrap().candidates[0].semantic
        else {
            unreachable!()
        };
        *selected_count = 1;
        *max_targets = 3;
        assert_rejected(&wrong_effect_counts);

        let mut wrong_effect_source = effect_base;
        let replacement = flat_test_ref(&wrong_effect_source.state, hand[1]);
        let ActionSemanticV1::ChooseEffectTarget { source, .. } =
            &mut wrong_effect_source.current.as_mut().unwrap().candidates[0].semantic
        else {
            unreachable!()
        };
        *source = replacement;
        assert_rejected(&wrong_effect_source);

        for (episode_id, retarget) in [(81_032, false), (81_033, true)] {
            let mut copy = FastActorSessionV1::reset_with_limits(episode_id, 132, 128, 16_384);
            let actor_id = copy.current.as_ref().unwrap().actor;
            let hand = copy.state.players[actor_id.index()].hand.clone();
            let origin = if retarget {
                Decision::ChooseSpellCopyRetarget {
                    player: actor_id,
                    copy: hand[0],
                }
            } else {
                Decision::ChooseSpellCopyPayment {
                    player: actor_id,
                    spell: hand[0],
                }
            };
            flat_install_origin_decision(
                &mut copy,
                PolicyDecisionV5::Surface(SurfaceDecision::Decision(origin)),
            );
            let replacement = flat_test_ref(&copy.state, hand[1]);
            match &mut copy.current.as_mut().unwrap().candidates[0].semantic {
                ActionSemanticV1::ChooseSpellCopyPayment { source, .. }
                | ActionSemanticV1::ChooseSpellCopyRetarget { source, .. } => {
                    *source = replacement;
                }
                _ => unreachable!(),
            }
            assert_rejected(&copy);
        }

        let mut madness = FastActorSessionV1::reset_with_limits(81_034, 134, 128, 16_384);
        let actor_id = madness.current.as_ref().unwrap().actor;
        let exiled = madness.state.players[actor_id.index()].hand[..2].to_vec();
        for object_id in &exiled {
            madness.state.players[actor_id.index()]
                .hand
                .retain(|candidate| candidate != object_id);
            madness.state.exile.push(*object_id);
            let object = madness.state.objects.get_mut(*object_id);
            object.zone = Zone::Exile;
            object.zone_change_count += 1;
        }
        flat_install_origin_decision(
            &mut madness,
            PolicyDecisionV5::Surface(SurfaceDecision::Decision(Decision::ChooseMadnessCast {
                player: actor_id,
                card: exiled[0],
            })),
        );
        let replacement = flat_test_ref(&madness.state, exiled[1]);
        for candidate in &mut madness.current.as_mut().unwrap().candidates {
            let ActionSemanticV1::ChooseMadnessCast { card, .. } = &mut candidate.semantic else {
                unreachable!()
            };
            *card = replacement.clone();
        }
        assert_rejected(&madness);

        let mut triggers = FastActorSessionV1::reset_with_limits(81_035, 135, 128, 16_384);
        let actor_id = triggers.current.as_ref().unwrap().actor;
        let hand = triggers.state.players[actor_id.index()].hand.clone();
        let pending = hand[..3]
            .iter()
            .map(|source| PendingTrigger {
                controller: actor_id,
                source: *source,
                effect: EffectOp::Sequence(Vec::new()),
                is_madness_offer: false,
                kicked: false,
            })
            .collect();
        flat_install_origin_decision(
            &mut triggers,
            PolicyDecisionV5::Surface(SurfaceDecision::Decision(Decision::OrderTriggers {
                player: actor_id,
                pending,
            })),
        );
        let replacement = flat_test_ref(&triggers.state, hand[3]);
        let ActionSemanticV1::OrderTriggers {
            pending_sources, ..
        } = &mut triggers.current.as_mut().unwrap().candidates[5].semantic
        else {
            unreachable!()
        };
        *pending_sources.first_mut().unwrap() = replacement;
        assert_eq!(
            flat_trigger_order_rank_v1(&[2, 0, 1]),
            Some(5),
            "fixture must exercise a non-self-inverse permutation"
        );
        assert_rejected(&triggers);
    }

    #[test]
    fn flat_action_slice_failure_preserves_a_prior_successful_encoding() {
        use crate::engine::Action;
        use crate::surface::SurfaceAction;

        let mut session = FastActorSessionV1::reset_with_limits(81_036, 136, 128, 16_384);
        let actor = session.current.as_ref().unwrap().actor;
        let spell = session.state.players[actor.index()].hand[0];
        flat_install_origin_decision(
            &mut session,
            PolicyDecisionV5::Surface(SurfaceDecision::Decision(Decision::ChooseSpellMode {
                player: actor,
                spell,
                mode_count: 2,
            })),
        );
        let mut actions = [poison_flat_action(); 8];
        let mut refs = [poison_flat_ref(); 8];
        let mut objects = [poison_flat_object(); 8];
        session
            .encode_current_flat_action_slice_v1(
                flat_current_decision(&session),
                &mut FlatActionDecisionSliceBuffersV1 {
                    actions: &mut actions,
                    refs: &mut refs,
                    objects: &mut objects,
                },
            )
            .unwrap();
        let actions_after_success = actions;
        let refs_after_success = refs;
        let objects_after_success = objects;

        let mut malformed_range = session.clone();
        let ActionSemanticV1::ChooseSpellMode { mode_count, .. } =
            &mut malformed_range.current.as_mut().unwrap().candidates[0].semantic
        else {
            unreachable!()
        };
        *mode_count = 0;
        assert_eq!(
            malformed_range.encode_current_flat_action_slice_v1(
                flat_current_decision(&malformed_range),
                &mut FlatActionDecisionSliceBuffersV1 {
                    actions: &mut actions,
                    refs: &mut refs,
                    objects: &mut objects,
                },
            ),
            Err(FlatActionDecisionSliceErrorV1::InvalidActionRange)
        );
        assert_eq!(actions, actions_after_success);
        assert_eq!(refs, refs_after_success);
        assert_eq!(objects, objects_after_success);

        let mut mismatched_pair = session;
        mismatched_pair.current.as_mut().unwrap().candidates[0].policy_action =
            PolicyActionV5::Surface(SurfaceAction::Action(Action::Pass));
        assert_eq!(
            mismatched_pair.encode_current_flat_action_slice_v1(
                flat_current_decision(&mismatched_pair),
                &mut FlatActionDecisionSliceBuffersV1 {
                    actions: &mut actions,
                    refs: &mut refs,
                    objects: &mut objects,
                },
            ),
            Err(FlatActionDecisionSliceErrorV1::InvalidDecisionRelation)
        );
        assert_eq!(actions, actions_after_success);
        assert_eq!(refs, refs_after_success);
        assert_eq!(objects, objects_after_success);
    }

    #[test]
    fn flat_action_slice_reuses_long_buffers_across_actor_and_episode_without_tail_leakage() {
        let mut long = FastActorSessionV1::reset_with_limits(81_037, 137, 128, 16_384);
        let long_actor = long.current.as_ref().unwrap().actor;
        let hand = long.state.players[long_actor.index()].hand.clone();
        flat_install_origin_decision(
            &mut long,
            PolicyDecisionV5::Surface(SurfaceDecision::Decision(Decision::ChooseEffectTargets {
                player: long_actor,
                source: hand[0],
                selected_count: 0,
                min_targets: 1,
                max_targets: 5,
                legal_targets: hand[1..5].iter().copied().map(Target::Object).collect(),
                can_finish: false,
            })),
        );
        let mut actions = [poison_flat_action(); 16];
        let mut refs = [poison_flat_ref(); 32];
        let mut objects = [poison_flat_object(); 16];
        let encoded_long = long
            .encode_current_flat_action_slice_v1(
                flat_current_decision(&long),
                &mut FlatActionDecisionSliceBuffersV1 {
                    actions: &mut actions,
                    refs: &mut refs,
                    objects: &mut objects,
                },
            )
            .unwrap();
        assert_eq!(encoded_long.active_action_count, 4);
        assert_eq!(encoded_long.active_ref_count, 8);
        assert_eq!(encoded_long.active_object_count, 5);
        let actions_after_long = actions;
        let refs_after_long = refs;
        let objects_after_long = objects;

        let mut short = FastActorSessionV1::reset_with_limits(81_038, 138, 128, 16_384);
        let short_actor = long_actor.opponent();
        flat_install_origin_decision(
            &mut short,
            PolicyDecisionV5::Surface(SurfaceDecision::Decision(Decision::CastSpellOrPass {
                player: short_actor,
                castable_spells: Vec::new(),
                mana_abilities: Vec::new(),
                land_drops: Vec::new(),
                activatable_abilities: Vec::new(),
                plot_actions: Vec::new(),
            })),
        );
        let encoded_short = short
            .encode_current_flat_action_slice_v1(
                flat_current_decision(&short),
                &mut FlatActionDecisionSliceBuffersV1 {
                    actions: &mut actions,
                    refs: &mut refs,
                    objects: &mut objects,
                },
            )
            .unwrap();
        assert_eq!(encoded_short.active_action_count, 1);
        assert_eq!(encoded_short.active_ref_count, 0);
        assert_eq!(encoded_short.active_object_count, 0);
        assert_ne!(
            encoded_short.binding.episode_id,
            encoded_long.binding.episode_id
        );
        assert_ne!(
            encoded_short.binding.acting_player,
            encoded_long.binding.acting_player
        );
        assert_eq!(&actions[1..], &actions_after_long[1..]);
        assert_eq!(refs, refs_after_long);
        assert_eq!(objects, objects_after_long);

        let mut fresh_actions = [FlatActionCoreV1::default(); 16];
        let mut fresh_refs = [FlatActionRefV1::default(); 32];
        let mut fresh_objects = [FlatActionObjectV1::default(); 16];
        let fresh = short
            .encode_current_flat_action_slice_v1(
                flat_current_decision(&short),
                &mut FlatActionDecisionSliceBuffersV1 {
                    actions: &mut fresh_actions,
                    refs: &mut fresh_refs,
                    objects: &mut fresh_objects,
                },
            )
            .unwrap();
        assert_eq!(fresh, encoded_short);
        assert_eq!(fresh_actions[0], actions[0]);
    }

    #[test]
    fn flat_action_slice_rejects_hidden_reference_before_publish() {
        use crate::engine::Action;
        use crate::surface::SurfaceAction;

        let mut session = FastActorSessionV1::reset_with_limits(81_005, 95, 128, 16_384);
        let actor_id = session.current.as_ref().unwrap().actor;
        let actor: PlayerSeatV1 = actor_id.into();
        let hidden_id = session.state.players[actor_id.opponent().index()].library[0];
        let hidden = flat_test_ref(&session.state, hidden_id);
        session.current.as_mut().unwrap().candidates = vec![CorePolicyActionCandidateV1 {
            semantic: ActionSemanticV1::CastSpell {
                actor,
                source: hidden,
            },
            policy_action: PolicyActionV5::Surface(SurfaceAction::Action(Action::CastSpell(
                hidden_id,
            ))),
        }];
        let decision = flat_current_decision(&session);

        let mut actions = [poison_flat_action(); 2];
        let mut refs = [poison_flat_ref(); 2];
        let mut objects = [poison_flat_object(); 2];
        let actions_before = actions;
        let refs_before = refs;
        let objects_before = objects;
        let error = session
            .encode_current_flat_action_slice_v1(
                decision,
                &mut FlatActionDecisionSliceBuffersV1 {
                    actions: &mut actions,
                    refs: &mut refs,
                    objects: &mut objects,
                },
            )
            .unwrap_err();
        assert_eq!(error, FlatActionDecisionSliceErrorV1::HiddenActionReference);
        assert_eq!(actions, actions_before);
        assert_eq!(refs, refs_before);
        assert_eq!(objects, objects_before);
    }

    #[test]
    fn flat_action_slice_rejects_every_stable_ref_identity_mismatch_before_publish() {
        use crate::engine::Action;
        use crate::surface::SurfaceAction;

        let base_session = FastActorSessionV1::reset_with_limits(81_010, 104, 128, 16_384);
        let actor_id = base_session.current.as_ref().unwrap().actor;
        let actor: PlayerSeatV1 = actor_id.into();
        let object_id = base_session.state.players[actor_id.index()].hand[0];
        let base = flat_test_ref(&base_session.state, object_id);
        let mut bad_arena = base.clone();
        bad_arena.arena_id = u32::MAX;
        let mut bad_card = base.clone();
        bad_card.card_db_id = bad_card.card_db_id.wrapping_add(1);
        let mut bad_owner = base.clone();
        bad_owner.owner = actor_id.opponent().into();
        let mut bad_controller = base.clone();
        bad_controller.controller = actor_id.opponent().into();
        let mut bad_zone = base.clone();
        bad_zone.zone = Zone::Battlefield;
        let mut bad_incarnation = base;
        bad_incarnation.zone_change_count += 1;

        for reference in [
            bad_arena,
            bad_card,
            bad_owner,
            bad_controller,
            bad_zone,
            bad_incarnation,
        ] {
            let mut session = base_session.clone();
            session.current.as_mut().unwrap().candidates = vec![CorePolicyActionCandidateV1 {
                semantic: ActionSemanticV1::CastSpell {
                    actor,
                    source: reference.clone(),
                },
                policy_action: PolicyActionV5::Surface(SurfaceAction::Action(Action::CastSpell(
                    ObjectId(reference.arena_id),
                ))),
            }];
            let mut actions = [poison_flat_action(); 2];
            let mut refs = [poison_flat_ref(); 2];
            let mut objects = [poison_flat_object(); 2];
            let actions_before = actions;
            let refs_before = refs;
            let objects_before = objects;
            assert_eq!(
                session.encode_current_flat_action_slice_v1(
                    flat_current_decision(&session),
                    &mut FlatActionDecisionSliceBuffersV1 {
                        actions: &mut actions,
                        refs: &mut refs,
                        objects: &mut objects,
                    },
                ),
                Err(FlatActionDecisionSliceErrorV1::InvalidActionReference)
            );
            assert_eq!(actions, actions_before);
            assert_eq!(refs, refs_before);
            assert_eq!(objects, objects_before);
        }
    }

    #[test]
    fn flat_action_slice_preserves_candidate_order_and_canonical_object_order() {
        use crate::engine::Action;
        use crate::surface::SurfaceAction;

        let mut session = FastActorSessionV1::reset_with_limits(81_006, 96, 128, 16_384);
        let actor_id = session.current.as_ref().unwrap().actor;
        let actor: PlayerSeatV1 = actor_id.into();
        let hand = &session.state.players[actor_id.index()].hand;
        let first = flat_test_ref(&session.state, hand[0]);
        let second = flat_test_ref(&session.state, hand[1]);
        session.current.as_mut().unwrap().candidates = vec![
            CorePolicyActionCandidateV1 {
                semantic: ActionSemanticV1::CastSpell {
                    actor,
                    source: first,
                },
                policy_action: PolicyActionV5::Surface(SurfaceAction::Action(Action::CastSpell(
                    hand[0],
                ))),
            },
            CorePolicyActionCandidateV1 {
                semantic: ActionSemanticV1::CastSpell {
                    actor,
                    source: second,
                },
                policy_action: PolicyActionV5::Surface(SurfaceAction::Action(Action::CastSpell(
                    hand[1],
                ))),
            },
            CorePolicyActionCandidateV1 {
                semantic: ActionSemanticV1::Pass { actor },
                policy_action: PolicyActionV5::Surface(SurfaceAction::Action(Action::Pass)),
            },
        ];
        session.current.as_mut().unwrap().origin_decision =
            PolicyDecisionV5::Surface(SurfaceDecision::Decision(Decision::CastSpellOrPass {
                player: actor_id,
                castable_spells: vec![hand[0], hand[1]],
                mana_abilities: Vec::new(),
                land_drops: Vec::new(),
                activatable_abilities: Vec::new(),
                plot_actions: Vec::new(),
            }));
        flat_refresh_action_cache_fixture(&mut session);
        let mut reordered = session.clone();
        reordered.current.as_mut().unwrap().candidates.swap(0, 1);
        let PolicyDecisionV5::Surface(SurfaceDecision::Decision(Decision::CastSpellOrPass {
            castable_spells,
            ..
        })) = &mut reordered.current.as_mut().unwrap().origin_decision
        else {
            unreachable!()
        };
        castable_spells.swap(0, 1);
        flat_refresh_action_cache_fixture(&mut reordered);
        let decision = flat_current_decision(&session);
        let reordered_decision = flat_current_decision(&reordered);

        let mut actions_a = [FlatActionCoreV1::default(); 3];
        let mut refs_a = [FlatActionRefV1::default(); 2];
        let mut objects_a = [FlatActionObjectV1::default(); 2];
        let mut actions_b = actions_a;
        let mut refs_b = refs_a;
        let mut objects_b = objects_a;
        let encoded_a = session
            .encode_current_flat_action_slice_v1(
                decision,
                &mut FlatActionDecisionSliceBuffersV1 {
                    actions: &mut actions_a,
                    refs: &mut refs_a,
                    objects: &mut objects_a,
                },
            )
            .unwrap();
        let encoded_b = reordered
            .encode_current_flat_action_slice_v1(
                reordered_decision,
                &mut FlatActionDecisionSliceBuffersV1 {
                    actions: &mut actions_b,
                    refs: &mut refs_b,
                    objects: &mut objects_b,
                },
            )
            .unwrap();

        assert_ne!(
            encoded_a.binding.candidate_order_commitment,
            encoded_b.binding.candidate_order_commitment
        );
        let mut binding_a = encoded_a.binding;
        let mut binding_b = encoded_b.binding;
        binding_a.candidate_order_commitment = [0; 16];
        binding_b.candidate_order_commitment = [0; 16];
        assert_eq!(binding_a, binding_b);
        assert_eq!(encoded_a.active_action_count, encoded_b.active_action_count);
        assert_eq!(encoded_a.active_ref_count, encoded_b.active_ref_count);
        assert_eq!(encoded_a.active_object_count, encoded_b.active_object_count);
        assert_eq!(objects_a, objects_b);
        assert_eq!(
            [actions_a[0].kind, actions_a[1].kind, actions_a[2].kind],
            [
                FlatActionKindV1::CastSpell,
                FlatActionKindV1::CastSpell,
                FlatActionKindV1::Pass
            ]
        );
        assert_eq!(
            [actions_b[0].kind, actions_b[1].kind, actions_b[2].kind],
            [
                FlatActionKindV1::CastSpell,
                FlatActionKindV1::CastSpell,
                FlatActionKindV1::Pass
            ]
        );
        assert_eq!([refs_a[0].object_index, refs_a[1].object_index], [0, 1]);
        assert_eq!([refs_b[0].object_index, refs_b[1].object_index], [1, 0]);
        assert_eq!([refs_a[0].action_index, refs_a[1].action_index], [0, 1]);
        assert_eq!([refs_b[0].action_index, refs_b[1].action_index], [0, 1]);

        let state_hash_before = session.diagnostic_state_hash();
        let environment_hash_before = session.privileged_core_environment_hash();
        let error = session
            .consume_current_flat_action_slice_v1(encoded_b.binding, 0)
            .unwrap_err();
        assert_eq!(error.code, RlSessionErrorCode::StaleEnvironmentBinding);
        assert_eq!(session.diagnostic_state_hash(), state_hash_before);
        assert_eq!(
            session.privileged_core_environment_hash(),
            environment_hash_before
        );
    }

    #[test]
    fn flat_action_slice_is_inert_to_unknown_opponent_library_identity() {
        let session = FastActorSessionV1::reset_with_decks_and_limits(
            81_002,
            92,
            128,
            16_384,
            [
                CANONICAL_RALLY_DECK_ID.to_string(),
                CANONICAL_RALLY_DECK_ID.to_string(),
            ],
        )
        .unwrap();
        let actor = session.current.as_ref().unwrap().actor;
        let opponent = actor.opponent();
        assert!(
            session.state.library_knowledge[actor.index()][opponent.index()].is_empty(),
            "reset fixture unexpectedly knows opponent library identities"
        );
        let mut mutated = session.clone();
        let first = mutated.state.players[opponent.index()].library[0];
        let second = mutated.state.players[opponent.index()].library[1];
        let first_card = mutated.state.objects.get(first).card_def;
        let second_card = mutated.state.objects.get(second).card_def;
        mutated.state.objects.get_mut(first).card_def = second_card;
        mutated.state.objects.get_mut(second).card_def = first_card;

        let mut actions_a = vec![FlatActionCoreV1::default(); 64];
        let mut refs_a = vec![FlatActionRefV1::default(); 256];
        let mut objects_a = vec![FlatActionObjectV1::default(); 128];
        let mut actions_b = actions_a.clone();
        let mut refs_b = refs_a.clone();
        let mut objects_b = objects_a.clone();
        let decision_a = flat_current_decision(&session);
        let decision_b = flat_current_decision(&mutated);
        let encoded_a = session
            .encode_current_flat_action_slice_v1(
                decision_a,
                &mut FlatActionDecisionSliceBuffersV1 {
                    actions: &mut actions_a,
                    refs: &mut refs_a,
                    objects: &mut objects_a,
                },
            )
            .unwrap();
        let encoded_b = mutated
            .encode_current_flat_action_slice_v1(
                decision_b,
                &mut FlatActionDecisionSliceBuffersV1 {
                    actions: &mut actions_b,
                    refs: &mut refs_b,
                    objects: &mut objects_b,
                },
            )
            .unwrap();
        assert_eq!(encoded_a, encoded_b);
        assert_eq!(
            &actions_a[..encoded_a.active_action_count as usize],
            &actions_b[..encoded_b.active_action_count as usize]
        );
        assert_eq!(
            &refs_a[..encoded_a.active_ref_count as usize],
            &refs_b[..encoded_b.active_ref_count as usize]
        );
        assert_eq!(
            &objects_a[..usize::from(encoded_a.active_object_count)],
            &objects_b[..usize::from(encoded_b.active_object_count)]
        );
    }

    #[test]
    fn flat_action_slice_is_inert_to_unknown_opponent_hand_identity() {
        let session = FastActorSessionV1::reset_with_limits(81_008, 102, 128, 16_384);
        let actor = session.current.as_ref().unwrap().actor;
        let opponent = actor.opponent();
        assert!(session.state.hand_knowledge[actor.index()][opponent.index()].is_empty());
        let mut mutated = session.clone();
        let first = mutated.state.players[opponent.index()].hand[0];
        let second = mutated.state.players[opponent.index()].hand[1];
        let first_card = mutated.state.objects.get(first).card_def;
        let second_card = mutated.state.objects.get(second).card_def;
        mutated.state.objects.get_mut(first).card_def = second_card;
        mutated.state.objects.get_mut(second).card_def = first_card;

        let mut actions_a = [FlatActionCoreV1::default(); 64];
        let mut refs_a = [FlatActionRefV1::default(); 256];
        let mut objects_a = [FlatActionObjectV1::default(); 128];
        let mut actions_b = actions_a;
        let mut refs_b = refs_a;
        let mut objects_b = objects_a;
        let encoded_a = session
            .encode_current_flat_action_slice_v1(
                flat_current_decision(&session),
                &mut FlatActionDecisionSliceBuffersV1 {
                    actions: &mut actions_a,
                    refs: &mut refs_a,
                    objects: &mut objects_a,
                },
            )
            .unwrap();
        let encoded_b = mutated
            .encode_current_flat_action_slice_v1(
                flat_current_decision(&mutated),
                &mut FlatActionDecisionSliceBuffersV1 {
                    actions: &mut actions_b,
                    refs: &mut refs_b,
                    objects: &mut objects_b,
                },
            )
            .unwrap();
        assert_eq!(encoded_a, encoded_b);
        assert_eq!(
            &actions_a[..encoded_a.active_action_count as usize],
            &actions_b[..encoded_b.active_action_count as usize]
        );
        assert_eq!(
            &refs_a[..encoded_a.active_ref_count as usize],
            &refs_b[..encoded_b.active_ref_count as usize]
        );
        assert_eq!(
            &objects_a[..usize::from(encoded_a.active_object_count)],
            &objects_b[..usize::from(encoded_b.active_object_count)]
        );
    }

    #[test]
    fn flat_action_slice_admits_only_actor_known_hand_and_library_refs() {
        use crate::engine::Action;
        use crate::surface::SurfaceAction;

        let mut session = FastActorSessionV1::reset_with_limits(81_009, 103, 128, 16_384);
        let actor_id = session.current.as_ref().unwrap().actor;
        let opponent = actor_id.opponent();
        let actor: PlayerSeatV1 = actor_id.into();
        let source_id = session.state.players[actor_id.index()].hand[0];
        let known_hand_id = session.state.players[opponent.index()].hand[0];
        let known_opponent_library_id = session.state.players[opponent.index()].library[0];
        let known_self_library_id = session.state.players[actor_id.index()].library[0];
        session
            .state
            .reveal_hand_card(actor_id, opponent, known_hand_id)
            .unwrap();
        session.state.reveal_library_top(actor_id, opponent, 1);
        session.state.reveal_library_top(actor_id, actor_id, 1);
        let source = flat_test_ref(&session.state, source_id);
        let known_hand = flat_test_ref(&session.state, known_hand_id);
        let known_opponent_library = flat_test_ref(&session.state, known_opponent_library_id);
        let known_self_library = flat_test_ref(&session.state, known_self_library_id);
        session.current.as_mut().unwrap().candidates = vec![
            CorePolicyActionCandidateV1 {
                semantic: ActionSemanticV1::ChooseEffectTarget {
                    actor,
                    source: source.clone(),
                    target: TargetRefV1::Object { object: known_hand },
                    selected_count: 0,
                    min_targets: 1,
                    max_targets: 4,
                },
                policy_action: PolicyActionV5::Surface(SurfaceAction::Action(
                    Action::ChooseEffectTarget(Target::Object(known_hand_id)),
                )),
            },
            CorePolicyActionCandidateV1 {
                semantic: ActionSemanticV1::ChooseEffectTarget {
                    actor,
                    source: source.clone(),
                    target: TargetRefV1::Object {
                        object: known_self_library,
                    },
                    selected_count: 0,
                    min_targets: 1,
                    max_targets: 4,
                },
                policy_action: PolicyActionV5::Surface(SurfaceAction::Action(
                    Action::ChooseEffectTarget(Target::Object(known_self_library_id)),
                )),
            },
            CorePolicyActionCandidateV1 {
                semantic: ActionSemanticV1::ChooseEffectTarget {
                    actor,
                    source,
                    target: TargetRefV1::Object {
                        object: known_opponent_library,
                    },
                    selected_count: 0,
                    min_targets: 1,
                    max_targets: 4,
                },
                policy_action: PolicyActionV5::Surface(SurfaceAction::Action(
                    Action::ChooseEffectTarget(Target::Object(known_opponent_library_id)),
                )),
            },
        ];
        session.current.as_mut().unwrap().origin_decision =
            PolicyDecisionV5::Surface(SurfaceDecision::Decision(Decision::ChooseEffectTargets {
                player: actor_id,
                source: source_id,
                selected_count: 0,
                min_targets: 1,
                max_targets: 4,
                legal_targets: vec![
                    Target::Object(known_hand_id),
                    Target::Object(known_self_library_id),
                    Target::Object(known_opponent_library_id),
                ],
                can_finish: false,
            }));
        flat_refresh_action_cache_fixture(&mut session);
        let mut actions = [FlatActionCoreV1::default(); 3];
        let mut refs = [FlatActionRefV1::default(); 6];
        let mut objects = [FlatActionObjectV1::default(); 4];
        let encoded = session
            .encode_current_flat_action_slice_v1(
                flat_current_decision(&session),
                &mut FlatActionDecisionSliceBuffersV1 {
                    actions: &mut actions,
                    refs: &mut refs,
                    objects: &mut objects,
                },
            )
            .unwrap();
        assert_eq!(encoded.active_ref_count, 6);
        assert_eq!(encoded.active_object_count, 4);
        assert_eq!(
            objects.map(|object| object.group),
            [
                FlatActionObjectGroupV1::SelfHand,
                FlatActionObjectGroupV1::KnownOpponentHand,
                FlatActionObjectGroupV1::KnownSelfLibrary,
                FlatActionObjectGroupV1::KnownOpponentLibrary,
            ]
        );
        assert!(objects
            .iter()
            .all(|object| object.actor_visible_ordinal == 0));
        assert_eq!(encoded.binding.card_db_hash, KERNEL_CARDDB_HASH);
        assert_eq!(
            encoded.binding.card_token_mapping_version,
            FLAT_ACTION_CARD_TOKEN_MAPPING_VERSION_V1
        );
    }

    #[test]
    fn flat_action_slice_supports_every_order_trigger_length_one_through_seven() {
        for count in 1..=7 {
            let session =
                FastActorSessionV1::reset_with_limits(81_003 + count as u64, 93, 128, 16_384);
            let actor_id = session.current.as_ref().unwrap().actor;
            let actor: PlayerSeatV1 = actor_id.into();
            let pending_sources: Vec<_> = session.state.players[actor_id.index()].hand[..count]
                .iter()
                .map(|&id| flat_test_ref(&session.state, id))
                .collect();
            let order: Vec<_> = if count == 3 {
                vec![2, 0, 1]
            } else {
                (0..count).rev().collect()
            };
            let expected_order = order.clone();
            let semantic = ActionSemanticV1::OrderTriggers {
                actor,
                pending_sources: pending_sources.clone(),
                order,
            };
            let mut refs = Vec::new();
            let core = flat_action_core_and_refs_v1(
                &semantic,
                actor,
                0,
                |role, order_index, associated_order, reference| {
                    refs.push((role, order_index, associated_order, reference.clone()));
                    Ok(())
                },
            )
            .unwrap();
            assert_eq!(core.kind, FlatActionKindV1::OrderTriggers);
            assert_eq!(core.ref_len, count as u16);
            for (index, (role, order_index, associated_order, reference)) in refs.iter().enumerate()
            {
                assert_eq!(*role, FlatActionRefRoleV1::PendingSources);
                assert_eq!(*order_index, index as u16);
                // `associated_order` is the raw permutation entry aligned to
                // this pending-source index, not the inverse placement.
                assert_eq!(*associated_order, expected_order[index] as u16);
                assert_eq!(reference, &pending_sources[index]);
            }
            assert!(flat_trigger_order_rank_v1(&expected_order).is_some());
        }
        assert_eq!(flat_trigger_order_rank_v1(&[2, 0, 1]), Some(5));
    }

    #[test]
    fn flat_action_slice_rejects_every_malformed_trigger_order_before_publish() {
        use crate::engine::Action;
        use crate::surface::SurfaceAction;

        let cases = [
            (0_usize, Vec::new()),
            (1, Vec::new()),
            (3, vec![0, 0, 2]),
            (3, vec![0, 1, 3]),
            (8, (0..8).collect()),
        ];
        for (case_index, (pending_count, order)) in cases.into_iter().enumerate() {
            let mut session =
                FastActorSessionV1::reset_with_limits(81_020 + case_index as u64, 98, 128, 16_384);
            let actor_id = session.current.as_ref().unwrap().actor;
            let actor: PlayerSeatV1 = actor_id.into();
            let hand = &session.state.players[actor_id.index()].hand;
            let pending_sources = (0..pending_count)
                .map(|index| flat_test_ref(&session.state, hand[index % hand.len()]))
                .collect();
            session.current.as_mut().unwrap().candidates = vec![CorePolicyActionCandidateV1 {
                semantic: ActionSemanticV1::OrderTriggers {
                    actor,
                    pending_sources,
                    order: order.clone(),
                },
                policy_action: PolicyActionV5::Surface(SurfaceAction::Action(
                    Action::OrderTriggers(order),
                )),
            }];
            let decision = flat_current_decision(&session);
            let mut actions = [poison_flat_action(); 2];
            let mut refs = [poison_flat_ref(); 8];
            let mut objects = [poison_flat_object(); 8];
            let actions_before = actions;
            let refs_before = refs;
            let objects_before = objects;
            assert_eq!(
                session.encode_current_flat_action_slice_v1(
                    decision,
                    &mut FlatActionDecisionSliceBuffersV1 {
                        actions: &mut actions,
                        refs: &mut refs,
                        objects: &mut objects,
                    },
                ),
                Err(FlatActionDecisionSliceErrorV1::InvalidTriggerOrder)
            );
            assert_eq!(actions, actions_before);
            assert_eq!(refs, refs_before);
            assert_eq!(objects, objects_before);
        }
    }

    #[test]
    fn flat_action_slice_encodes_policy_v5_inclusion_actions() {
        for (session, expected_kind, expected_ref_len) in [
            (
                fast_attacker_session(3, 8, 8),
                FlatActionKindV1::ChooseAttackerInclusion,
                1,
            ),
            (
                fast_blocker_session(2, 8, 8),
                FlatActionKindV1::ChooseBlockerInclusion,
                2,
            ),
        ] {
            let response = session.current_response();
            let FastActorResponseV1::Decision(decision) = response else {
                panic!("inclusion fixture unexpectedly terminated");
            };
            let mut actions = [FlatActionCoreV1::default(); 2];
            let mut refs = [FlatActionRefV1::default(); 4];
            let mut objects = [FlatActionObjectV1::default(); 4];
            let encoded = session
                .encode_current_flat_action_slice_v1(
                    decision,
                    &mut FlatActionDecisionSliceBuffersV1 {
                        actions: &mut actions,
                        refs: &mut refs,
                        objects: &mut objects,
                    },
                )
                .unwrap();
            assert_eq!(encoded.active_action_count, 2);
            assert_eq!(actions[0].kind, expected_kind);
            assert_eq!(actions[1].kind, expected_kind);
            assert_eq!(actions[0].ref_len, expected_ref_len);
            assert_eq!(actions[1].ref_len, expected_ref_len);
            assert_eq!(actions[0].flags & FLAT_ACTION_FLAG_INCLUDE_V1, 0);
            assert_eq!(
                actions[1].flags & FLAT_ACTION_FLAG_INCLUDE_V1,
                FLAT_ACTION_FLAG_INCLUDE_V1
            );
        }
    }

    #[test]
    fn flat_action_core_matches_hand_authored_all_kind_golden() {
        let session = FastActorSessionV1::reset_with_limits(81_004, 94, 128, 16_384);
        let actor_id = session.current.as_ref().unwrap().actor;
        let actor: PlayerSeatV1 = actor_id.into();
        let hand = &session.state.players[actor_id.index()].hand;
        let a = flat_test_ref(&session.state, hand[0]);
        let b = flat_test_ref(&session.state, hand[1]);
        let semantics = vec![
            ActionSemanticV1::Pass { actor },
            ActionSemanticV1::PlayLand {
                actor,
                source: a.clone(),
            },
            ActionSemanticV1::CastSpell {
                actor,
                source: a.clone(),
            },
            ActionSemanticV1::ActivateManaAbility {
                actor,
                source: a.clone(),
                mana_choice: Some(ManaColor::R),
            },
            ActionSemanticV1::ActivateAbility {
                actor,
                source: a.clone(),
                ability_index: 7,
            },
            ActionSemanticV1::PlotSpell {
                actor,
                source: a.clone(),
            },
            ActionSemanticV1::ChooseTarget {
                actor,
                source: a.clone(),
                remaining: 2,
                target: TargetRefV1::Player { player: actor },
            },
            ActionSemanticV1::ChooseCostTarget {
                actor,
                source: a.clone(),
                cost_kind: CostKind::SacrificeLands,
                remaining: 3,
                candidate: b.clone(),
            },
            ActionSemanticV1::ChooseCastMode {
                actor,
                source: a.clone(),
                mode: CastMode::Alternative,
            },
            ActionSemanticV1::ChooseKicker {
                actor,
                source: a.clone(),
                pay: true,
            },
            ActionSemanticV1::ChooseSpellMode {
                actor,
                source: a.clone(),
                mode_index: 1,
                mode_count: 2,
            },
            ActionSemanticV1::ChooseEffectOption {
                actor,
                source: a.clone(),
                option_index: 3,
                option_count: 5,
            },
            ActionSemanticV1::ChooseEffectTarget {
                actor,
                source: a.clone(),
                target: TargetRefV1::Object { object: b.clone() },
                selected_count: 1,
                min_targets: 1,
                max_targets: 3,
            },
            ActionSemanticV1::FinishEffectSelection {
                actor,
                source: a.clone(),
                selected_count: 2,
            },
            ActionSemanticV1::ChooseEffectColor {
                actor,
                source: a.clone(),
                color: ManaColor::G,
            },
            ActionSemanticV1::ChooseEffectNumber {
                actor,
                source: a.clone(),
                number: 5,
                minimum: -2,
                maximum: 9,
            },
            ActionSemanticV1::ChooseEffectBoolean {
                actor,
                source: a.clone(),
                value: true,
            },
            ActionSemanticV1::FinishTargetSelection {
                actor,
                source: a.clone(),
                selected_count: 4,
            },
            ActionSemanticV1::ChooseOptionalCostUse {
                actor,
                use_cost: true,
            },
            ActionSemanticV1::ChooseOptionalCostWhich {
                actor,
                choice: OptionalCostChoice::SacrificeLand,
            },
            ActionSemanticV1::ChooseSpellCopyPayment {
                actor,
                source: a.clone(),
                pay: true,
            },
            ActionSemanticV1::ChooseSpellCopyRetarget {
                actor,
                source: a.clone(),
                change_target: true,
            },
            ActionSemanticV1::ChooseMadnessCast {
                actor,
                card: a.clone(),
                cast_it: true,
            },
            ActionSemanticV1::Discard {
                actor,
                cards: vec![a.clone()],
            },
            ActionSemanticV1::ChooseAttackerInclusion {
                actor,
                attacker: a.clone(),
                include: true,
            },
            ActionSemanticV1::ChooseBlockerInclusion {
                actor,
                attacker: a.clone(),
                blocker: b.clone(),
                include: true,
            },
            ActionSemanticV1::OrderTriggers {
                actor,
                pending_sources: vec![a.clone(), b.clone()],
                order: vec![1, 0],
            },
        ];
        let mut actual_actions = Vec::new();
        let mut actual_refs = Vec::new();
        let mut ref_start = 0_u32;
        for semantic in &semantics {
            let core = flat_action_core_and_refs_v1(
                semantic,
                actor,
                ref_start,
                |role, order_index, associated_order, reference| {
                    actual_refs.push((role, order_index, associated_order, reference.clone()));
                    Ok(())
                },
            )
            .unwrap();
            ref_start += u32::from(core.ref_len);
            actual_actions.push(core);
        }

        let expected_actions = vec![
            FlatActionCoreV1 {
                kind: FlatActionKindV1::Pass,
                ..FlatActionCoreV1::default()
            },
            FlatActionCoreV1 {
                kind: FlatActionKindV1::PlayLand,
                ref_len: 1,
                ..FlatActionCoreV1::default()
            },
            FlatActionCoreV1 {
                kind: FlatActionKindV1::CastSpell,
                ref_start: 1,
                ref_len: 1,
                ..FlatActionCoreV1::default()
            },
            FlatActionCoreV1 {
                kind: FlatActionKindV1::ActivateManaAbility,
                mana_choice: 4,
                ref_start: 2,
                ref_len: 1,
                ..FlatActionCoreV1::default()
            },
            FlatActionCoreV1 {
                kind: FlatActionKindV1::ActivateAbility,
                ability_index: 7,
                ref_start: 3,
                ref_len: 1,
                ..FlatActionCoreV1::default()
            },
            FlatActionCoreV1 {
                kind: FlatActionKindV1::PlotSpell,
                ref_start: 4,
                ref_len: 1,
                ..FlatActionCoreV1::default()
            },
            FlatActionCoreV1 {
                kind: FlatActionKindV1::ChooseTarget,
                remaining: 2,
                target_kind: 1,
                target_player: 1,
                ref_start: 5,
                ref_len: 1,
                ..FlatActionCoreV1::default()
            },
            FlatActionCoreV1 {
                kind: FlatActionKindV1::ChooseCostTarget,
                remaining: 3,
                cost_kind: 1,
                ref_start: 6,
                ref_len: 2,
                ..FlatActionCoreV1::default()
            },
            FlatActionCoreV1 {
                kind: FlatActionKindV1::ChooseCastMode,
                cast_mode: 2,
                ref_start: 8,
                ref_len: 1,
                ..FlatActionCoreV1::default()
            },
            FlatActionCoreV1 {
                kind: FlatActionKindV1::ChooseKicker,
                flags: 1,
                ref_start: 9,
                ref_len: 1,
                ..FlatActionCoreV1::default()
            },
            FlatActionCoreV1 {
                kind: FlatActionKindV1::ChooseSpellMode,
                mode_index: 1,
                mode_count: 2,
                ref_start: 10,
                ref_len: 1,
                ..FlatActionCoreV1::default()
            },
            FlatActionCoreV1 {
                kind: FlatActionKindV1::ChooseEffectOption,
                option_index: 3,
                option_count: 5,
                ref_start: 11,
                ref_len: 1,
                ..FlatActionCoreV1::default()
            },
            FlatActionCoreV1 {
                kind: FlatActionKindV1::ChooseEffectTarget,
                selected_count: 1,
                min_targets: 1,
                max_targets: 3,
                target_kind: 2,
                ref_start: 12,
                ref_len: 2,
                ..FlatActionCoreV1::default()
            },
            FlatActionCoreV1 {
                kind: FlatActionKindV1::FinishEffectSelection,
                selected_count: 2,
                ref_start: 14,
                ref_len: 1,
                ..FlatActionCoreV1::default()
            },
            FlatActionCoreV1 {
                kind: FlatActionKindV1::ChooseEffectColor,
                color: 5,
                ref_start: 15,
                ref_len: 1,
                ..FlatActionCoreV1::default()
            },
            FlatActionCoreV1 {
                kind: FlatActionKindV1::ChooseEffectNumber,
                number: 5,
                minimum: -2,
                maximum: 9,
                ref_start: 16,
                ref_len: 1,
                ..FlatActionCoreV1::default()
            },
            FlatActionCoreV1 {
                kind: FlatActionKindV1::ChooseEffectBoolean,
                flags: 16,
                ref_start: 17,
                ref_len: 1,
                ..FlatActionCoreV1::default()
            },
            FlatActionCoreV1 {
                kind: FlatActionKindV1::FinishTargetSelection,
                selected_count: 4,
                ref_start: 18,
                ref_len: 1,
                ..FlatActionCoreV1::default()
            },
            FlatActionCoreV1 {
                kind: FlatActionKindV1::ChooseOptionalCostUse,
                flags: 4,
                ref_start: 19,
                ..FlatActionCoreV1::default()
            },
            FlatActionCoreV1 {
                kind: FlatActionKindV1::ChooseOptionalCostWhich,
                optional_cost_choice: 3,
                ref_start: 19,
                ..FlatActionCoreV1::default()
            },
            FlatActionCoreV1 {
                kind: FlatActionKindV1::ChooseSpellCopyPayment,
                flags: 1,
                ref_start: 19,
                ref_len: 1,
                ..FlatActionCoreV1::default()
            },
            FlatActionCoreV1 {
                kind: FlatActionKindV1::ChooseSpellCopyRetarget,
                flags: 2,
                ref_start: 20,
                ref_len: 1,
                ..FlatActionCoreV1::default()
            },
            FlatActionCoreV1 {
                kind: FlatActionKindV1::ChooseMadnessCast,
                flags: 8,
                ref_start: 21,
                ref_len: 1,
                ..FlatActionCoreV1::default()
            },
            FlatActionCoreV1 {
                kind: FlatActionKindV1::Discard,
                ref_start: 22,
                ref_len: 1,
                ..FlatActionCoreV1::default()
            },
            FlatActionCoreV1 {
                kind: FlatActionKindV1::ChooseAttackerInclusion,
                flags: 32,
                ref_start: 23,
                ref_len: 1,
                ..FlatActionCoreV1::default()
            },
            FlatActionCoreV1 {
                kind: FlatActionKindV1::ChooseBlockerInclusion,
                flags: 32,
                ref_start: 24,
                ref_len: 2,
                ..FlatActionCoreV1::default()
            },
            FlatActionCoreV1 {
                kind: FlatActionKindV1::OrderTriggers,
                ref_start: 26,
                ref_len: 2,
                ..FlatActionCoreV1::default()
            },
        ];
        assert_eq!(actual_actions, expected_actions);

        let expected_refs = vec![
            (FlatActionRefRoleV1::Source, 0, 0, a.clone()),
            (FlatActionRefRoleV1::Source, 0, 0, a.clone()),
            (FlatActionRefRoleV1::Source, 0, 0, a.clone()),
            (FlatActionRefRoleV1::Source, 0, 0, a.clone()),
            (FlatActionRefRoleV1::Source, 0, 0, a.clone()),
            (FlatActionRefRoleV1::Source, 0, 0, a.clone()),
            (FlatActionRefRoleV1::Source, 0, 0, a.clone()),
            (FlatActionRefRoleV1::Candidate, 0, 0, b.clone()),
            (FlatActionRefRoleV1::Source, 0, 0, a.clone()),
            (FlatActionRefRoleV1::Source, 0, 0, a.clone()),
            (FlatActionRefRoleV1::Source, 0, 0, a.clone()),
            (FlatActionRefRoleV1::Source, 0, 0, a.clone()),
            (FlatActionRefRoleV1::Source, 0, 0, a.clone()),
            (FlatActionRefRoleV1::TargetObject, 0, 0, b.clone()),
            (FlatActionRefRoleV1::Source, 0, 0, a.clone()),
            (FlatActionRefRoleV1::Source, 0, 0, a.clone()),
            (FlatActionRefRoleV1::Source, 0, 0, a.clone()),
            (FlatActionRefRoleV1::Source, 0, 0, a.clone()),
            (FlatActionRefRoleV1::Source, 0, 0, a.clone()),
            (FlatActionRefRoleV1::Source, 0, 0, a.clone()),
            (FlatActionRefRoleV1::Source, 0, 0, a.clone()),
            (FlatActionRefRoleV1::Card, 0, 0, a.clone()),
            (FlatActionRefRoleV1::Cards, 0, 0, a.clone()),
            (FlatActionRefRoleV1::Attacker, 0, 0, a.clone()),
            (FlatActionRefRoleV1::Attacker, 0, 0, a.clone()),
            (FlatActionRefRoleV1::Blocker, 0, 0, b.clone()),
            (FlatActionRefRoleV1::PendingSources, 0, 1, a),
            (FlatActionRefRoleV1::PendingSources, 1, 0, b),
        ];
        assert_eq!(actual_refs, expected_refs);
    }

    #[test]
    fn flat_action_enum_optional_and_false_encodings_match_fixed_ids() {
        let session = FastActorSessionV1::reset_with_limits(81_007, 97, 128, 16_384);
        let actor_id = session.current.as_ref().unwrap().actor;
        let actor: PlayerSeatV1 = actor_id.into();
        let hand = &session.state.players[actor_id.index()].hand;
        let a = flat_test_ref(&session.state, hand[0]);
        let b = flat_test_ref(&session.state, hand[1]);

        let kinds = [
            FlatActionKindV1::Pass,
            FlatActionKindV1::PlayLand,
            FlatActionKindV1::CastSpell,
            FlatActionKindV1::ActivateManaAbility,
            FlatActionKindV1::ActivateAbility,
            FlatActionKindV1::PlotSpell,
            FlatActionKindV1::ChooseTarget,
            FlatActionKindV1::ChooseCostTarget,
            FlatActionKindV1::ChooseCastMode,
            FlatActionKindV1::ChooseKicker,
            FlatActionKindV1::ChooseSpellMode,
            FlatActionKindV1::ChooseEffectOption,
            FlatActionKindV1::ChooseEffectTarget,
            FlatActionKindV1::FinishEffectSelection,
            FlatActionKindV1::ChooseEffectColor,
            FlatActionKindV1::ChooseEffectNumber,
            FlatActionKindV1::ChooseEffectBoolean,
            FlatActionKindV1::FinishTargetSelection,
            FlatActionKindV1::ChooseOptionalCostUse,
            FlatActionKindV1::ChooseOptionalCostWhich,
            FlatActionKindV1::ChooseSpellCopyPayment,
            FlatActionKindV1::ChooseSpellCopyRetarget,
            FlatActionKindV1::ChooseMadnessCast,
            FlatActionKindV1::Discard,
            FlatActionKindV1::ChooseAttackerInclusion,
            FlatActionKindV1::ChooseBlockerInclusion,
            FlatActionKindV1::OrderTriggers,
        ];
        assert_eq!(
            kinds.map(flat_action_kind_id_v1),
            std::array::from_fn::<_, 27, _>(|index| index as u8)
        );
        let roles = [
            FlatActionRefRoleV1::Source,
            FlatActionRefRoleV1::Candidate,
            FlatActionRefRoleV1::Card,
            FlatActionRefRoleV1::Attacker,
            FlatActionRefRoleV1::Blocker,
            FlatActionRefRoleV1::TargetObject,
            FlatActionRefRoleV1::Cards,
            FlatActionRefRoleV1::PendingSources,
        ];
        assert_eq!(
            roles.map(flat_action_ref_role_id_v1),
            [0, 1, 2, 3, 4, 5, 6, 7]
        );

        for (choice, expected) in [
            (None, 0),
            (Some(ManaColor::W), 1),
            (Some(ManaColor::U), 2),
            (Some(ManaColor::B), 3),
            (Some(ManaColor::R), 4),
            (Some(ManaColor::G), 5),
            (Some(ManaColor::C), 6),
        ] {
            let core = flat_test_core(
                &ActionSemanticV1::ActivateManaAbility {
                    actor,
                    source: a.clone(),
                    mana_choice: choice,
                },
                actor,
            )
            .unwrap();
            assert_eq!(core.mana_choice, expected);
        }
        for (color, expected) in [
            (ManaColor::W, 1),
            (ManaColor::U, 2),
            (ManaColor::B, 3),
            (ManaColor::R, 4),
            (ManaColor::G, 5),
            (ManaColor::C, 6),
        ] {
            let core = flat_test_core(
                &ActionSemanticV1::ChooseEffectColor {
                    actor,
                    source: a.clone(),
                    color,
                },
                actor,
            )
            .unwrap();
            assert_eq!(core.color, expected);
        }
        for (mode, expected) in [(CastMode::Normal, 1), (CastMode::Alternative, 2)] {
            let core = flat_test_core(
                &ActionSemanticV1::ChooseCastMode {
                    actor,
                    source: a.clone(),
                    mode,
                },
                actor,
            )
            .unwrap();
            assert_eq!(core.cast_mode, expected);
        }
        for (kind, expected) in [
            (CostKind::SacrificeLands, 1),
            (CostKind::SacrificePermanents, 2),
            (CostKind::SacrificeCreatures, 3),
            (CostKind::SacrificeArtifacts, 4),
            (CostKind::DiscardCards, 5),
            (CostKind::ExileFromGraveyard, 6),
            (CostKind::TapPermanents, 7),
            (CostKind::ReturnPermanentsToHand, 8),
            (CostKind::PayLife, 9),
            (CostKind::RemoveCounters, 10),
            (CostKind::PutCounters, 11),
        ] {
            let core = flat_test_core(
                &ActionSemanticV1::ChooseCostTarget {
                    actor,
                    source: a.clone(),
                    cost_kind: kind,
                    remaining: 1,
                    candidate: b.clone(),
                },
                actor,
            )
            .unwrap();
            assert_eq!(core.cost_kind, expected);
        }
        for (choice, expected) in [
            (OptionalCostChoice::Decline, 1),
            (OptionalCostChoice::Discard, 2),
            (OptionalCostChoice::SacrificeLand, 3),
        ] {
            let core = flat_test_core(
                &ActionSemanticV1::ChooseOptionalCostWhich { actor, choice },
                actor,
            )
            .unwrap();
            assert_eq!(core.optional_cost_choice, expected);
        }
        let opponent: PlayerSeatV1 = actor_id.opponent().into();
        let opponent_target = flat_test_core(
            &ActionSemanticV1::ChooseTarget {
                actor,
                source: a.clone(),
                remaining: 1,
                target: TargetRefV1::Player { player: opponent },
            },
            actor,
        )
        .unwrap();
        assert_eq!(
            (opponent_target.target_kind, opponent_target.target_player),
            (1, 2)
        );

        let false_semantics = [
            ActionSemanticV1::ChooseKicker {
                actor,
                source: a.clone(),
                pay: false,
            },
            ActionSemanticV1::ChooseEffectBoolean {
                actor,
                source: a.clone(),
                value: false,
            },
            ActionSemanticV1::ChooseOptionalCostUse {
                actor,
                use_cost: false,
            },
            ActionSemanticV1::ChooseSpellCopyPayment {
                actor,
                source: a.clone(),
                pay: false,
            },
            ActionSemanticV1::ChooseSpellCopyRetarget {
                actor,
                source: a.clone(),
                change_target: false,
            },
            ActionSemanticV1::ChooseMadnessCast {
                actor,
                card: a.clone(),
                cast_it: false,
            },
            ActionSemanticV1::ChooseAttackerInclusion {
                actor,
                attacker: a.clone(),
                include: false,
            },
            ActionSemanticV1::ChooseBlockerInclusion {
                actor,
                attacker: a,
                blocker: b,
                include: false,
            },
        ];
        for semantic in false_semantics {
            assert_eq!(flat_test_core(&semantic, actor).unwrap().flags, 0);
        }
    }

    #[test]
    fn flat_action_slice_rejects_schema_only_unexecutable_kinds_before_publish() {
        use crate::engine::Action;
        use crate::surface::SurfaceAction;

        let base = FastActorSessionV1::reset_with_limits(81_039, 139, 128, 16_384);
        let actor_id = base.current.as_ref().unwrap().actor;
        let actor: PlayerSeatV1 = actor_id.into();
        let source = flat_test_ref(&base.state, base.state.players[actor_id.index()].hand[0]);
        let semantics = [
            ActionSemanticV1::ChooseEffectColor {
                actor,
                source: source.clone(),
                color: ManaColor::R,
            },
            ActionSemanticV1::ChooseEffectNumber {
                actor,
                source: source.clone(),
                number: 2,
                minimum: 1,
                maximum: 3,
            },
            ActionSemanticV1::FinishTargetSelection {
                actor,
                source,
                selected_count: 1,
            },
        ];
        for semantic in semantics {
            let mut session = base.clone();
            session.current.as_mut().unwrap().candidates = vec![CorePolicyActionCandidateV1 {
                semantic,
                policy_action: PolicyActionV5::Surface(SurfaceAction::Action(Action::Pass)),
            }];
            let mut actions = [poison_flat_action(); 2];
            let mut refs = [poison_flat_ref(); 2];
            let mut objects = [poison_flat_object(); 2];
            let actions_before = actions;
            let refs_before = refs;
            let objects_before = objects;
            assert_eq!(
                session.encode_current_flat_action_slice_v1(
                    flat_current_decision(&session),
                    &mut FlatActionDecisionSliceBuffersV1 {
                        actions: &mut actions,
                        refs: &mut refs,
                        objects: &mut objects,
                    },
                ),
                Err(FlatActionDecisionSliceErrorV1::UnsupportedActionSemantic)
            );
            assert_eq!(actions, actions_before);
            assert_eq!(refs, refs_before);
            assert_eq!(objects, objects_before);
        }
    }

    #[test]
    fn flat_action_candidate_commitment_matches_independent_pass_vector() {
        let mut session = FastActorSessionV1::reset_with_limits(81_040, 140, 128, 16_384);
        flat_install_origin_decision(
            &mut session,
            PolicyDecisionV5::Surface(SurfaceDecision::Decision(Decision::CastSpellOrPass {
                player: PlayerId::P0,
                castable_spells: Vec::new(),
                mana_abilities: Vec::new(),
                land_drops: Vec::new(),
                activatable_abilities: Vec::new(),
                plot_actions: Vec::new(),
            })),
        );
        let mut actions = [FlatActionCoreV1::default(); 1];
        let mut refs = [];
        let mut objects = [];
        let encoded = session
            .encode_current_flat_action_slice_v1(
                flat_current_decision(&session),
                &mut FlatActionDecisionSliceBuffersV1 {
                    actions: &mut actions,
                    refs: &mut refs,
                    objects: &mut objects,
                },
            )
            .unwrap();
        assert_eq!(actions, [FlatActionCoreV1::default()]);
        // Independently generated with Python hashlib/struct over the
        // documented little-endian v1 byte stream and frozen CardDB hash.
        assert_eq!(
            encoded.binding.candidate_order_commitment,
            [
                0xf1, 0xe2, 0x01, 0xba, 0xcd, 0x3d, 0xff, 0x30, 0x6f, 0x30, 0x7d, 0xc7, 0xa8, 0x6d,
                0x17, 0xa2,
            ]
        );
    }
}
