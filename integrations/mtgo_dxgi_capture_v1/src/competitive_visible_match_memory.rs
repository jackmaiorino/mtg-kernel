use crate::{
    OpaqueMtgoCompetitiveMatchVisibleGameLogSnapshotV1,
    OpaqueMtgoCompetitiveVisibleGameLogSemanticsV1,
};
use mtgo_blackbox_v1::{
    ActionSemanticV1, CardPrivateV1, CardPublicV2, CardStableRefV1,
    CheckedUntrustedMtgoCompetitivePlayerVisibleGameHistoryV1, KnownLibraryCardV4,
    MtgoCompetitiveEventKindV1, MtgoCompetitivePlayerVisibleDecisionViewV1,
    MtgoVisibleGameLogEventKindV1, MtgoVisibleGameLogPlayerRoleV1,
    MtgoVisibleGameLogSemanticEventViewV1, ObjectRelationPublicV4, ObservationV5, PlayerSeatV1,
    StackItemKindV2, StackItemPublicV2, TargetRefV1, ZoneIndependentStepV1,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const COMBINED_COMPETITIVE_VISIBLE_GAME_MEMORY_DOMAIN_V1: &[u8] =
    b"mtgo-combined-competitive-player-visible-game-memory-v1";
const COMPETITIVE_VISIBLE_GAME_OUTCOME_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-player-visible-game-outcome-v1";

pub const MTGO_COMPETITIVE_EXTERNAL_PUBLIC_HISTORY_SCHEMA_V1: u32 = 1;

/// Ordering contract for the kernel-facing public-history seam.
///
/// MTGO Game Log sequence numbers and admitted capture-frame sequence numbers
/// are independent clocks. V1 therefore preserves exact order within each
/// source and requires the kernel to encode the source role explicitly. It
/// makes no claim about the relative order of an event in one stream and a
/// decision in the other. Inventing a cross-source gameplay chronology is not
/// required for import and would be invalid without a shared visible clock.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MtgoCompetitiveExternalPublicHistoryOrderingV1 {
    SeparateOrderedStreamsNoCrossSourceTotalOrder,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MtgoCompetitivePlayerRelativeGameWinnerV1 {
    ActingPlayer,
    Opponent,
}

/// Player-visible lineage and bounds supplied before either history stream.
///
/// This header contains game facts and counts only. Event and match identity,
/// deployment and source commitments, paths, source UUIDs, raw markup, pixels,
/// coordinates, process data, and authority stay outside the model-facing
/// callback. The separate-stream ordering contract and the rule that public
/// log events supplement, but cannot replace, the exact current visible
/// observation are type-level API invariants rather than model inputs.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::MtgoCompetitiveExternalPublicHistoryHeaderV1;
/// fn cannot_tensorize_protocol_metadata(value: &MtgoCompetitiveExternalPublicHistoryHeaderV1) {
///     let _ = value.schema_version;
///     let _ = value.ordering;
///     let _ = value.player_visible_information_only;
///     let _ = value.game_log_is_complete_current_state;
/// }
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MtgoCompetitiveExternalPublicHistoryHeaderV1 {
    pub event_kind: MtgoCompetitiveEventKindV1,
    pub game_number: u8,
    pub confirmed_decision_count: usize,
    pub public_event_count: usize,
}

/// Narrow model-facing view of one confirmed visible decision. Adapter frame
/// numbers and provenance commitments remain private. The observation is the
/// same validated player-visible gameplay input already scored by the kernel;
/// its local contract metadata must not be tensorized as game information.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::MtgoCompetitiveExternalConfirmedDecisionV1;
/// fn cannot_read_adapter_provenance(value: &MtgoCompetitiveExternalConfirmedDecisionV1<'_>) {
///     let _ = value.source_frame_sequence_v1();
///     let _ = value.decision_commitment_sha256_v1();
/// }
/// ```
pub struct MtgoCompetitiveExternalConfirmedDecisionV1<'a> {
    decision: MtgoCompetitivePlayerVisibleDecisionViewV1<'a>,
}

/// Model-facing subset of one validated gameplay observation. Contract
/// versions, card-database hash, adapter step and substep IDs, projection hash,
/// and other local metadata have no getter and cannot become history features.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::MtgoCompetitiveExternalVisibleObservationV1;
/// fn cannot_read_local_contract_metadata(value: &MtgoCompetitiveExternalVisibleObservationV1<'_>) {
///     let _ = value.schema_version;
///     let _ = value.card_db_hash;
///     let _ = value.step_index;
///     let _ = value.visible_projection_hash;
///     let _ = value.public_projection_v1();
///     let _ = value.player_status_v1();
///     let _ = value.continuous_effects_v1();
///     let _ = value.engine_context_v1();
///     let _ = value.surface_context_v1();
///     let _ = value.policy_surface_context_v1();
///     let _ = value.exile_play_permissions_v1();
/// }
/// ```
pub struct MtgoCompetitiveExternalVisibleObservationV1<'a> {
    observation: &'a ObservationV5,
}

/// Minimal view of one card whose identity is visibly available in a public
/// zone. Zone-specific engine state cannot be read through this type.
pub struct MtgoCompetitiveExternalVisiblePublicZoneCardV1<'a> {
    card: &'a CardPublicV2,
}

/// ```compile_fail
/// use mtgo_dxgi_capture_v1::MtgoCompetitiveExternalVisiblePublicZoneCardV1;
/// fn cannot_apply_battlefield_state_to_other_zones(
///     value: &MtgoCompetitiveExternalVisiblePublicZoneCardV1<'_>,
/// ) {
///     let _ = value.tapped_v1();
///     let _ = value.marked_damage_v1();
///     let _ = value.plus_one_plus_one_counter_count_v1();
/// }
/// ```
impl MtgoCompetitiveExternalVisiblePublicZoneCardV1<'_> {
    pub fn object_ref_v1(&self) -> &CardStableRefV1 {
        &self.card.stable
    }

    pub fn card_name_v1(&self) -> &str {
        &self.card.card_name
    }
}

/// Field-by-field view of one visible battlefield object. It intentionally
/// omits engine-maintained timestamps, ability-use bookkeeping, skip-untap
/// state, goad expiry, and the full characteristic record. The stable
/// reference is adapter-local identity for linking visible objects and legal
/// actions, never an MTGO internal object identifier.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::MtgoCompetitiveExternalVisibleBattlefieldCardV1;
/// fn cannot_read_engine_bookkeeping(value: &MtgoCompetitiveExternalVisibleBattlefieldCardV1<'_>) {
///     let _ = value.summoning_sick;
///     let _ = value.entered_battlefield_turn;
///     let _ = value.ability_uses_this_turn;
///     let _ = value.goaded_by;
///     let _ = value.characteristics;
/// }
/// ```
pub struct MtgoCompetitiveExternalVisibleBattlefieldCardV1<'a> {
    card: &'a CardPublicV2,
}

impl MtgoCompetitiveExternalVisibleBattlefieldCardV1<'_> {
    pub fn object_ref_v1(&self) -> &CardStableRefV1 {
        &self.card.stable
    }

    pub fn card_name_v1(&self) -> &str {
        &self.card.card_name
    }

    pub fn tapped_v1(&self) -> bool {
        self.card.tapped
    }

    pub fn marked_damage_v1(&self) -> u16 {
        self.card.damage
    }

    pub fn plus_one_plus_one_counter_count_v1(&self) -> i16 {
        self.card.counters.plus1_plus1
    }

    pub fn minus_one_minus_one_counter_count_v1(&self) -> i16 {
        self.card.counters.minus1_minus1
    }

    pub fn minus_zero_minus_one_counter_count_v1(&self) -> i16 {
        self.card.counters.minus0_minus1
    }

    pub fn stun_counter_count_v1(&self) -> i16 {
        self.card.counters.stun
    }

    pub fn lore_counter_count_v1(&self) -> i16 {
        self.card.counters.lore
    }

    pub fn is_token_v1(&self) -> bool {
        self.card.is_token
    }

    pub fn visible_face_index_v1(&self) -> u8 {
        self.card.face_index
    }

    pub fn visible_effective_power_v1(&self) -> Option<i32> {
        self.card.characteristics.effective_power
    }

    pub fn visible_effective_toughness_v1(&self) -> Option<i32> {
        self.card.characteristics.effective_toughness
    }
}

/// Field-by-field view of one item displayed on MTGO's stack. Paid-cost
/// internals and engine resume state are not exposed.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::MtgoCompetitiveExternalVisibleStackItemV1;
/// fn cannot_read_stack_internals(value: &MtgoCompetitiveExternalVisibleStackItemV1<'_>) {
///     let _ = value.paid_cost_refs;
///     let _ = value.cast_method;
///     let _ = value.mode_chosen;
/// }
/// ```
pub struct MtgoCompetitiveExternalVisibleStackItemV1<'a> {
    item: &'a StackItemPublicV2,
}

impl MtgoCompetitiveExternalVisibleStackItemV1<'_> {
    pub fn visible_stack_position_v1(&self) -> u32 {
        self.item.stack_index
    }

    pub fn source_object_ref_v1(&self) -> &CardStableRefV1 {
        &self.item.source
    }

    pub fn controller_v1(&self) -> PlayerSeatV1 {
        self.item.controller
    }

    pub fn visible_targets_v1(&self) -> &[TargetRefV1] {
        &self.item.targets
    }

    pub fn item_kind_v1(&self) -> StackItemKindV2 {
        self.item.stack_item_kind
    }
}

impl MtgoCompetitiveExternalVisibleObservationV1<'_> {
    pub fn acting_player_v1(&self) -> PlayerSeatV1 {
        self.observation.acting_player
    }

    pub fn turn_v1(&self) -> u32 {
        self.observation.projection.surface.turn
    }

    pub fn phase_v1(&self) -> ZoneIndependentStepV1 {
        self.observation.projection.surface.phase
    }

    pub fn active_player_v1(&self) -> PlayerSeatV1 {
        self.observation.projection.surface.active_player
    }

    pub fn priority_player_v1(&self) -> PlayerSeatV1 {
        self.observation.projection.surface.priority_player
    }

    pub fn initiative_v1(&self) -> Option<PlayerSeatV1> {
        self.observation.projection.surface.initiative
    }

    pub fn life_totals_v1(&self) -> [i32; 2] {
        self.observation.projection.surface.life_totals
    }

    pub fn mana_pools_v1(&self) -> [[u8; 6]; 2] {
        self.observation.projection.surface.mana_pools
    }

    pub fn hand_counts_v1(&self) -> [usize; 2] {
        self.observation.projection.surface.hand_counts
    }

    pub fn library_counts_v1(&self) -> [usize; 2] {
        self.observation.projection.surface.library_counts
    }

    pub fn battlefield_card_count_v1(&self, seat: PlayerSeatV1) -> usize {
        self.observation.projection.surface.battlefield[seat_index_v1(seat)].len()
    }

    pub fn battlefield_card_v1(
        &self,
        seat: PlayerSeatV1,
        index: usize,
    ) -> Option<MtgoCompetitiveExternalVisibleBattlefieldCardV1<'_>> {
        self.observation.projection.surface.battlefield[seat_index_v1(seat)]
            .get(index)
            .map(|card| MtgoCompetitiveExternalVisibleBattlefieldCardV1 { card })
    }

    pub fn graveyard_card_count_v1(&self, seat: PlayerSeatV1) -> usize {
        self.observation.projection.surface.graveyards[seat_index_v1(seat)].len()
    }

    pub fn graveyard_card_v1(
        &self,
        seat: PlayerSeatV1,
        index: usize,
    ) -> Option<MtgoCompetitiveExternalVisiblePublicZoneCardV1<'_>> {
        self.observation.projection.surface.graveyards[seat_index_v1(seat)]
            .get(index)
            .map(|card| MtgoCompetitiveExternalVisiblePublicZoneCardV1 { card })
    }

    pub fn exile_card_count_v1(&self) -> usize {
        self.observation.projection.surface.exile.len()
    }

    pub fn exile_card_v1(
        &self,
        index: usize,
    ) -> Option<MtgoCompetitiveExternalVisiblePublicZoneCardV1<'_>> {
        self.observation
            .projection
            .surface
            .exile
            .get(index)
            .map(|card| MtgoCompetitiveExternalVisiblePublicZoneCardV1 { card })
    }

    pub fn stack_item_count_v1(&self) -> usize {
        self.observation.projection.surface.stack.len()
    }

    pub fn stack_item_v1(
        &self,
        index: usize,
    ) -> Option<MtgoCompetitiveExternalVisibleStackItemV1<'_>> {
        self.observation
            .projection
            .surface
            .stack
            .get(index)
            .map(|item| MtgoCompetitiveExternalVisibleStackItemV1 { item })
    }

    pub fn ordered_attackers_v1(&self) -> &[CardStableRefV1] {
        &self.observation.projection.surface.combat.ordered_attackers
    }

    pub fn attackers_declared_v1(&self) -> bool {
        self.observation
            .projection
            .surface
            .combat
            .attackers_declared
    }

    pub fn blockers_declared_v1(&self) -> bool {
        self.observation.projection.surface.combat.blockers_declared
    }

    pub fn blocker_assignment_count_v1(&self) -> usize {
        self.observation
            .projection
            .surface
            .combat
            .attacker_to_ordered_blockers
            .len()
    }

    pub fn blocker_assignment_v1(
        &self,
        index: usize,
    ) -> Option<(&CardStableRefV1, &[CardStableRefV1])> {
        self.observation
            .projection
            .surface
            .combat
            .attacker_to_ordered_blockers
            .get(index)
            .map(|(attacker, blockers)| (attacker, blockers.as_slice()))
    }

    pub fn visible_object_relations_v1(&self) -> &[ObjectRelationPublicV4] {
        &self.observation.projection.surface.object_relations
    }

    pub fn own_hand_v1(&self) -> &[CardPrivateV1] {
        &self.observation.own_hand
    }

    pub fn known_library_cards_v1(&self) -> &[Vec<KnownLibraryCardV4>; 2] {
        &self.observation.known_library_cards
    }

    pub fn known_hand_cards_v1(&self) -> &[Vec<CardPrivateV1>; 2] {
        &self.observation.known_hand_cards
    }
}

fn seat_index_v1(seat: PlayerSeatV1) -> usize {
    match seat {
        PlayerSeatV1::P0 => 0,
        PlayerSeatV1::P1 => 1,
    }
}

impl MtgoCompetitiveExternalConfirmedDecisionV1<'_> {
    pub fn within_source_position_v1(&self) -> u64 {
        self.decision.sequence_v1()
    }

    pub fn visible_observation_v1(&self) -> MtgoCompetitiveExternalVisibleObservationV1<'_> {
        MtgoCompetitiveExternalVisibleObservationV1 {
            observation: self.decision.observation_v1(),
        }
    }

    pub fn selected_semantic_v1(&self) -> &ActionSemanticV1 {
        self.decision.selected_semantic_v1()
    }
}

/// Narrow model-facing view of one fact rendered by MTGO's Game Log. Raw text
/// hashes and all transport provenance remain private to the adapter.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::MtgoCompetitiveExternalPublicGameLogEventV1;
/// fn cannot_read_transport_provenance(value: &MtgoCompetitiveExternalPublicGameLogEventV1<'_>) {
///     let _ = value.source_visible_text_sha256_v1();
///     let _ = value.source_record_commitment_sha256_v1();
/// }
/// ```
pub struct MtgoCompetitiveExternalPublicGameLogEventV1<'a> {
    event: MtgoVisibleGameLogSemanticEventViewV1<'a>,
}

impl MtgoCompetitiveExternalPublicGameLogEventV1<'_> {
    pub fn within_source_position_v1(&self) -> u32 {
        self.event.source_sequence_v1()
    }

    pub fn kind_v1(&self) -> MtgoVisibleGameLogEventKindV1 {
        self.event.kind_v1()
    }

    pub fn actor_role_v1(&self) -> Option<MtgoVisibleGameLogPlayerRoleV1> {
        self.event.actor_role_v1()
    }

    pub fn turn_number_v1(&self) -> Option<u32> {
        self.event.turn_number_v1()
    }

    pub fn primary_count_v1(&self) -> Option<u8> {
        self.event.primary_count_v1()
    }

    pub fn secondary_count_v1(&self) -> Option<u8> {
        self.event.secondary_count_v1()
    }

    pub fn visible_card_name_count_v1(&self) -> usize {
        self.event.visible_card_name_count_v1()
    }

    pub fn visible_card_name_v1(&self, index: usize) -> Option<&str> {
        self.event.visible_card_name_v1(index)
    }
}

/// Kernel-owned consumer boundary for one exact game's player-visible public
/// history. Implementations receive two separate ordered streams. The adapter
/// deliberately provides no callback that claims a cross-source ordering. A
/// conforming importer retains separate source-role and within-source sequence
/// features rather than treating callback order as gameplay chronology.
///
/// A consumer result is ordinary data, not adapter authority. Implementing
/// this trait cannot send MTGO input, enter an event, or spend resources.
pub trait MtgoCompetitiveExternalPublicHistoryConsumerV1 {
    type Output;

    fn begin_public_history_v1(
        &mut self,
        header: MtgoCompetitiveExternalPublicHistoryHeaderV1,
    ) -> Result<(), String>;

    fn consume_confirmed_decision_v1(
        &mut self,
        decision: MtgoCompetitiveExternalConfirmedDecisionV1<'_>,
    ) -> Result<(), String>;

    fn finish_confirmed_decision_stream_v1(&mut self) -> Result<(), String>;

    fn consume_public_game_log_event_v1(
        &mut self,
        event: MtgoCompetitiveExternalPublicGameLogEventV1<'_>,
    ) -> Result<(), String>;

    fn finish_public_game_log_stream_v1(&mut self) -> Result<(), String>;

    fn finish_public_history_v1(&mut self) -> Result<Self::Output, String>;
}

#[derive(Clone, Copy)]
struct VisibleGameLineageRefV1<'a> {
    event_kind: mtgo_blackbox_v1::MtgoCompetitiveEventKindV1,
    event_identity_sha256: &'a str,
    match_identity_sha256: &'a str,
    game_number: u8,
}

/// Move-only union of the two player-visible history sources for one exact
/// League or Challenge game: confirmed model decisions and MTGO's public Game
/// Log events. It is an import source for a future kernel history encoder, not
/// a scoring or input capability.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoCompetitivePlayerVisibleGameMemoryV1;
/// let _forged = OpaqueMtgoCompetitivePlayerVisibleGameMemoryV1 {};
/// ```
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoCompetitivePlayerVisibleGameMemoryV1;
/// fn require_clone<T: Clone>() {}
/// require_clone::<OpaqueMtgoCompetitivePlayerVisibleGameMemoryV1>();
/// ```
pub struct OpaqueMtgoCompetitivePlayerVisibleGameMemoryV1 {
    game_log: OpaqueMtgoCompetitivePlayerVisibleGameLogSourceV1,
    confirmed_decisions: CheckedUntrustedMtgoCompetitivePlayerVisibleGameHistoryV1,
    memory_commitment_sha256: String,
}

/// Move-only proof that one exact prior-game visible history ended with one
/// player-relative winner. Only the winner is public game information. Match,
/// log, decision, and adapter provenance remain private for a later exact
/// sideboard binder.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoCompetitiveVisibleGameOutcomeV1;
/// let _forged = OpaqueMtgoCompetitiveVisibleGameOutcomeV1 {};
/// ```
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoCompetitiveVisibleGameOutcomeV1;
/// fn require_clone<T: Clone>() {}
/// require_clone::<OpaqueMtgoCompetitiveVisibleGameOutcomeV1>();
/// ```
pub struct OpaqueMtgoCompetitiveVisibleGameOutcomeV1 {
    memory: OpaqueMtgoCompetitivePlayerVisibleGameMemoryV1,
    winner: MtgoCompetitivePlayerRelativeGameWinnerV1,
    outcome_commitment_sha256: String,
}

pub(crate) struct MtgoCompetitiveVisibleGameOutcomeLineageV1<'a> {
    pub event_kind: MtgoCompetitiveEventKindV1,
    pub event_identity_sha256: &'a str,
    pub match_identity_sha256: &'a str,
    pub game_number: u8,
    pub source_memory_commitment_sha256: &'a str,
    pub outcome_commitment_sha256: &'a str,
    pub winner: MtgoCompetitivePlayerRelativeGameWinnerV1,
}

impl OpaqueMtgoCompetitiveVisibleGameOutcomeV1 {
    pub fn winner_v1(&self) -> MtgoCompetitivePlayerRelativeGameWinnerV1 {
        self.winner
    }

    pub fn safe_for_model_scoring_v1(&self) -> bool {
        false
    }

    pub fn safe_for_input_v1(&self) -> bool {
        false
    }

    pub fn permits_event_entry_v1(&self) -> bool {
        false
    }

    pub fn permits_spending_v1(&self) -> bool {
        false
    }

    pub(crate) fn lineage_v1(&self) -> MtgoCompetitiveVisibleGameOutcomeLineageV1<'_> {
        let lineage = self.memory.game_log.lineage_v1();
        MtgoCompetitiveVisibleGameOutcomeLineageV1 {
            event_kind: lineage.event_kind,
            event_identity_sha256: lineage.event_identity_sha256,
            match_identity_sha256: lineage.match_identity_sha256,
            game_number: lineage.game_number,
            source_memory_commitment_sha256: &self.memory.memory_commitment_sha256,
            outcome_commitment_sha256: &self.outcome_commitment_sha256,
            winner: self.winner,
        }
    }

    pub fn into_match_log_lease_and_confirmed_decisions_v1(
        self,
    ) -> Result<
        (
            crate::OpaqueMtgoCompetitiveMatchVisibleGameLogLeaseV1,
            CheckedUntrustedMtgoCompetitivePlayerVisibleGameHistoryV1,
        ),
        String,
    > {
        self.memory
            .into_match_log_lease_and_confirmed_decisions_v1()
    }
}

enum OpaqueMtgoCompetitivePlayerVisibleGameLogSourceV1 {
    LegacyProcessEpoch(Box<OpaqueMtgoCompetitiveVisibleGameLogSemanticsV1>),
    MatchScoped(Box<OpaqueMtgoCompetitiveMatchVisibleGameLogSnapshotV1>),
}

impl OpaqueMtgoCompetitivePlayerVisibleGameLogSourceV1 {
    fn event_count_v1(&self) -> usize {
        match self {
            Self::LegacyProcessEpoch(source) => source.event_count_v1(),
            Self::MatchScoped(source) => source.event_count_v1(),
        }
    }

    fn event_v1(&self, index: usize) -> Option<MtgoVisibleGameLogSemanticEventViewV1<'_>> {
        match self {
            Self::LegacyProcessEpoch(source) => source.event_v1(index),
            Self::MatchScoped(source) => source.event_v1(index),
        }
    }

    fn lineage_v1(&self) -> VisibleGameLineageRefV1<'_> {
        match self {
            Self::LegacyProcessEpoch(source) => VisibleGameLineageRefV1 {
                event_kind: source.event_kind_v1(),
                event_identity_sha256: source.event_identity_sha256_v1(),
                match_identity_sha256: source.match_identity_sha256_v1(),
                game_number: source.game_number_v1(),
            },
            Self::MatchScoped(source) => VisibleGameLineageRefV1 {
                event_kind: source.event_kind_v1(),
                event_identity_sha256: source.event_identity_sha256_v1(),
                match_identity_sha256: source.match_identity_sha256_v1(),
                game_number: source.game_number_v1(),
            },
        }
    }

    fn source_commitment_sha256_v1(&self) -> &str {
        match self {
            Self::LegacyProcessEpoch(source) => source.binding_commitment_sha256_v1(),
            Self::MatchScoped(source) => source.snapshot_commitment_sha256_v1(),
        }
    }

    fn semantic_projection_commitment_sha256_v1(&self) -> &str {
        match self {
            Self::LegacyProcessEpoch(source) => source.semantic_projection_commitment_sha256_v1(),
            Self::MatchScoped(source) => source.semantic_projection_commitment_sha256_v1(),
        }
    }
}

impl OpaqueMtgoCompetitivePlayerVisibleGameMemoryV1 {
    pub fn public_event_count_v1(&self) -> usize {
        self.game_log.event_count_v1()
    }

    pub fn public_event_v1(
        &self,
        index: usize,
    ) -> Option<MtgoVisibleGameLogSemanticEventViewV1<'_>> {
        self.game_log.event_v1(index)
    }

    pub fn confirmed_decision_count_v1(&self) -> usize {
        self.confirmed_decisions.decision_count_v1()
    }

    pub fn confirmed_decision_v1(
        &self,
        index: usize,
    ) -> Option<MtgoCompetitivePlayerVisibleDecisionViewV1<'_>> {
        self.confirmed_decisions.decision_v1(index)
    }

    pub fn match_identity_sha256_v1(&self) -> &str {
        self.game_log.lineage_v1().match_identity_sha256
    }

    pub fn game_number_v1(&self) -> u8 {
        self.game_log.lineage_v1().game_number
    }

    pub fn policy_deployment_commitment_sha256_v1(&self) -> &str {
        self.confirmed_decisions
            .policy_deployment_commitment_sha256_v1()
    }

    pub fn memory_commitment_sha256_v1(&self) -> &str {
        &self.memory_commitment_sha256
    }

    pub fn ready_for_kernel_history_import_v1(&self) -> bool {
        true
    }

    pub fn safe_for_model_scoring_v1(&self) -> bool {
        false
    }

    pub fn safe_for_input_v1(&self) -> bool {
        false
    }

    pub fn permits_event_entry_v1(&self) -> bool {
        false
    }

    pub fn permits_spending_v1(&self) -> bool {
        false
    }

    /// Consumes one exact game memory and requires one terminal public winner.
    /// Games one and two may feed a later sideboard decision. Game three has
    /// no subsequent best-of-three sideboard.
    pub fn into_visible_game_outcome_v1(
        self,
    ) -> Result<OpaqueMtgoCompetitiveVisibleGameOutcomeV1, String> {
        let lineage = self.game_log.lineage_v1();
        if !(1..=2).contains(&lineage.game_number) {
            return Err("visible game outcome has no following sideboard game".to_owned());
        }
        let winner =
            derive_visible_game_winner_v1((0..self.public_event_count_v1()).map(|index| {
                let event = self
                    .public_event_v1(index)
                    .expect("public event count and lookup are consistent");
                (event.kind_v1(), event.actor_role_v1())
            }))?;
        let winner_byte = match winner {
            MtgoCompetitivePlayerRelativeGameWinnerV1::ActingPlayer => 1_u8,
            MtgoCompetitivePlayerRelativeGameWinnerV1::Opponent => 2_u8,
        };
        let outcome_commitment_sha256 = commitment_with_domain_v1(
            COMPETITIVE_VISIBLE_GAME_OUTCOME_DOMAIN_V1,
            &[
                self.memory_commitment_sha256.as_bytes(),
                lineage.event_identity_sha256.as_bytes(),
                lineage.match_identity_sha256.as_bytes(),
                &[lineage.game_number],
                &[winner_byte],
                b"one_exact_player_visible_won_game_event_no_model_no_input",
            ],
        );
        Ok(OpaqueMtgoCompetitiveVisibleGameOutcomeV1 {
            memory: self,
            winner,
            outcome_commitment_sha256,
        })
    }

    /// Visits the two exact public-history streams without inventing a shared
    /// clock. The confirmed-decision stream is completed before the Game Log
    /// stream begins, but that callback order is serialization order only and
    /// carries no gameplay-time ordering claim between the sources.
    pub fn visit_external_public_history_v1<C>(&self, consumer: &mut C) -> Result<C::Output, String>
    where
        C: MtgoCompetitiveExternalPublicHistoryConsumerV1,
    {
        let lineage = self.game_log.lineage_v1();
        consumer.begin_public_history_v1(MtgoCompetitiveExternalPublicHistoryHeaderV1 {
            event_kind: lineage.event_kind,
            game_number: lineage.game_number,
            confirmed_decision_count: self.confirmed_decision_count_v1(),
            public_event_count: self.public_event_count_v1(),
        })?;

        for index in 0..self.confirmed_decision_count_v1() {
            let decision = self.confirmed_decision_v1(index).ok_or_else(|| {
                "confirmed decision stream changed while it was being visited".to_owned()
            })?;
            consumer.consume_confirmed_decision_v1(MtgoCompetitiveExternalConfirmedDecisionV1 {
                decision,
            })?;
        }
        consumer.finish_confirmed_decision_stream_v1()?;

        for index in 0..self.public_event_count_v1() {
            let event = self.public_event_v1(index).ok_or_else(|| {
                "public Game Log stream changed while it was being visited".to_owned()
            })?;
            consumer.consume_public_game_log_event_v1(
                MtgoCompetitiveExternalPublicGameLogEventV1 { event },
            )?;
        }
        consumer.finish_public_game_log_stream_v1()?;
        consumer.finish_public_history_v1()
    }

    /// Consumes a match-scoped game memory after its immutable public views
    /// have been imported and returns the exact log lease plus confirmed
    /// decision history. The lease can then be consumed only by the matching
    /// Sideboarding baseline. Legacy process-epoch sources cannot advance.
    pub fn into_match_log_lease_and_confirmed_decisions_v1(
        self,
    ) -> Result<
        (
            crate::OpaqueMtgoCompetitiveMatchVisibleGameLogLeaseV1,
            CheckedUntrustedMtgoCompetitivePlayerVisibleGameHistoryV1,
        ),
        String,
    > {
        match self.game_log {
            OpaqueMtgoCompetitivePlayerVisibleGameLogSourceV1::MatchScoped(source) => {
                Ok(((*source).into_match_lease_v1(), self.confirmed_decisions))
            }
            OpaqueMtgoCompetitivePlayerVisibleGameLogSourceV1::LegacyProcessEpoch(_) => Err(
                "legacy process-epoch visible Game Log memory cannot advance a match".to_owned(),
            ),
        }
    }
}

/// Requires both retained histories to describe the exact same event, match,
/// and numbered game. This does not guess a total ordering between MTGO log
/// sequence numbers and capture frame sequence numbers. The future kernel
/// importer must define that mapping explicitly and fixture-test it.
pub fn bind_competitive_player_visible_game_memory_v1(
    game_log: OpaqueMtgoCompetitiveVisibleGameLogSemanticsV1,
    confirmed_decisions: CheckedUntrustedMtgoCompetitivePlayerVisibleGameHistoryV1,
) -> Result<OpaqueMtgoCompetitivePlayerVisibleGameMemoryV1, String> {
    bind_competitive_player_visible_game_memory_source_v1(
        OpaqueMtgoCompetitivePlayerVisibleGameLogSourceV1::LegacyProcessEpoch(Box::new(game_log)),
        confirmed_decisions,
    )
}

/// Joins the match-scoped per-game visible log snapshot to the exact confirmed
/// decision history for that same League or Challenge game.
pub fn bind_match_scoped_competitive_player_visible_game_memory_v1(
    game_log: OpaqueMtgoCompetitiveMatchVisibleGameLogSnapshotV1,
    confirmed_decisions: CheckedUntrustedMtgoCompetitivePlayerVisibleGameHistoryV1,
) -> Result<OpaqueMtgoCompetitivePlayerVisibleGameMemoryV1, String> {
    bind_competitive_player_visible_game_memory_source_v1(
        OpaqueMtgoCompetitivePlayerVisibleGameLogSourceV1::MatchScoped(Box::new(game_log)),
        confirmed_decisions,
    )
}

fn bind_competitive_player_visible_game_memory_source_v1(
    game_log: OpaqueMtgoCompetitivePlayerVisibleGameLogSourceV1,
    confirmed_decisions: CheckedUntrustedMtgoCompetitivePlayerVisibleGameHistoryV1,
) -> Result<OpaqueMtgoCompetitivePlayerVisibleGameMemoryV1, String> {
    let game_log_lineage = game_log.lineage_v1();
    validate_combined_visible_game_lineage_v1(
        game_log_lineage,
        VisibleGameLineageRefV1 {
            event_kind: confirmed_decisions.event_kind_v1(),
            event_identity_sha256: confirmed_decisions.event_identity_sha256_v1(),
            match_identity_sha256: confirmed_decisions.match_identity_sha256_v1(),
            game_number: confirmed_decisions.game_number_v1(),
        },
    )?;
    let memory_commitment_sha256 = commitment_v1(&[
        game_log.source_commitment_sha256_v1().as_bytes(),
        game_log
            .semantic_projection_commitment_sha256_v1()
            .as_bytes(),
        confirmed_decisions
            .history_commitment_sha256_v1()
            .as_bytes(),
        confirmed_decisions
            .policy_deployment_commitment_sha256_v1()
            .as_bytes(),
        game_log_lineage.event_identity_sha256.as_bytes(),
        game_log_lineage.match_identity_sha256.as_bytes(),
        &[game_log_lineage.game_number],
        b"dual_source_public_history_ready_for_explicit_kernel_import_no_scoring_no_input",
    ]);
    Ok(OpaqueMtgoCompetitivePlayerVisibleGameMemoryV1 {
        game_log,
        confirmed_decisions,
        memory_commitment_sha256,
    })
}

fn validate_combined_visible_game_lineage_v1(
    log: VisibleGameLineageRefV1<'_>,
    decisions: VisibleGameLineageRefV1<'_>,
) -> Result<(), String> {
    if log.event_kind != decisions.event_kind
        || log.event_identity_sha256 != decisions.event_identity_sha256
        || log.match_identity_sha256 != decisions.match_identity_sha256
        || log.game_number != decisions.game_number
    {
        return Err(
            "Game Log and confirmed decisions describe different competitive event, match, or game"
                .to_owned(),
        );
    }
    Ok(())
}

fn derive_visible_game_winner_v1(
    events: impl IntoIterator<
        Item = (
            MtgoVisibleGameLogEventKindV1,
            Option<MtgoVisibleGameLogPlayerRoleV1>,
        ),
    >,
) -> Result<MtgoCompetitivePlayerRelativeGameWinnerV1, String> {
    let mut winner = None;
    let mut terminal_seen = false;
    for (kind, actor) in events {
        if terminal_seen {
            return Err("visible Game Log has semantic events after the game winner".to_owned());
        }
        match kind {
            MtgoVisibleGameLogEventKindV1::WonGame => {
                let current = match actor {
                    Some(MtgoVisibleGameLogPlayerRoleV1::ActingPlayer) => {
                        MtgoCompetitivePlayerRelativeGameWinnerV1::ActingPlayer
                    }
                    Some(MtgoVisibleGameLogPlayerRoleV1::Opponent) => {
                        MtgoCompetitivePlayerRelativeGameWinnerV1::Opponent
                    }
                    None => return Err("visible won-game event has no player role".to_owned()),
                };
                if winner.replace(current).is_some() {
                    return Err("visible Game Log has more than one game winner".to_owned());
                }
                terminal_seen = true;
            }
            MtgoVisibleGameLogEventKindV1::WonMatch
            | MtgoVisibleGameLogEventKindV1::ForcedComplete => {
                return Err("terminal match history cannot start a sideboard decision".to_owned())
            }
            _ => {}
        }
    }
    winner.ok_or_else(|| "visible Game Log has no game winner".to_owned())
}

fn commitment_v1(parts: &[&[u8]]) -> String {
    commitment_with_domain_v1(COMBINED_COMPETITIVE_VISIBLE_GAME_MEMORY_DOMAIN_V1, parts)
}

fn commitment_with_domain_v1(domain: &[u8], parts: &[&[u8]]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(
        u64::try_from(domain.len())
            .expect("static domain length fits u64")
            .to_be_bytes(),
    );
    hasher.update(domain);
    for part in parts {
        hasher.update(
            u64::try_from(part.len())
                .expect("in-memory commitment part length fits u64")
                .to_be_bytes(),
        );
        hasher.update(part);
    }
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;
    use mtg_kernel::rl::{CardCharacteristicsV2, CardTypeFlagsV2, CountersV1, KeywordFlagsV2};
    use mtg_kernel::rl_session::{RlEpisodeSessionV1, RlSessionResponseV1};
    use mtg_kernel::state::Zone;
    use mtgo_blackbox_v1::MtgoCompetitiveEventKindV1::{Challenge, League};

    fn visible_card_v1(zone: Zone) -> CardPublicV2 {
        CardPublicV2 {
            stable: CardStableRefV1 {
                arena_id: 77,
                card_db_id: 1,
                owner: PlayerSeatV1::P0,
                controller: PlayerSeatV1::P0,
                zone,
                zone_change_count: 2,
            },
            card_name: "Visible test creature".to_owned(),
            tapped: true,
            summoning_sick: true,
            damage: 2,
            counters: CountersV1 {
                plus1_plus1: 1,
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
            entered_battlefield_turn: Some(3),
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
                base_power: Some(2),
                base_toughness: Some(2),
                effective_power: Some(3),
                effective_toughness: Some(3),
                effective_color_mask: 1,
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
                    minimum_blockers: 1,
                    landwalk_mask: 0,
                },
            },
        }
    }

    #[test]
    fn external_observation_exposes_visible_zones_field_by_field() {
        let session = RlEpisodeSessionV1::reset_with_limits(7, 1, 128, 16_384);
        let RlSessionResponseV1::Decision(decision) = session.current_response() else {
            panic!("opening position must be a decision");
        };
        let mut observation = (*decision.observation).clone();
        let battlefield_card = visible_card_v1(Zone::Battlefield);
        let battlefield_ref = battlefield_card.stable.clone();
        let mut graveyard_card = visible_card_v1(Zone::Graveyard);
        graveyard_card.stable.zone_change_count = 3;
        graveyard_card.stable.zone = Zone::Graveyard;
        let mut exile_card = visible_card_v1(Zone::Exile);
        exile_card.stable.zone_change_count = 4;
        exile_card.stable.zone = Zone::Exile;
        observation.projection.surface.battlefield[0] = vec![battlefield_card];
        observation.projection.surface.graveyards[0] = vec![graveyard_card];
        observation.projection.surface.exile = vec![exile_card];
        observation.projection.surface.stack = vec![StackItemPublicV2 {
            stack_index: 0,
            source: battlefield_ref.clone(),
            controller: PlayerSeatV1::P0,
            targets: vec![TargetRefV1::Player {
                player: PlayerSeatV1::P1,
            }],
            stack_item_kind: StackItemKindV2::ActivatedAbility,
            is_copy: false,
            is_flashback: false,
            mode_chosen: 0,
            madness_offer: false,
            kicked: false,
            cast_method: None,
            face_index: 0,
            x_value: 0,
            paid_cost_refs: vec![battlefield_ref.clone()],
        }];
        observation.projection.surface.combat.ordered_attackers = vec![battlefield_ref.clone()];
        observation
            .projection
            .surface
            .combat
            .attacker_to_ordered_blockers = vec![(battlefield_ref.clone(), Vec::new())];
        observation.projection.surface.object_relations =
            vec![ObjectRelationPublicV4::AttachedTo {
                object: battlefield_ref.clone(),
                attached_to: battlefield_ref,
            }];

        let view = MtgoCompetitiveExternalVisibleObservationV1 {
            observation: &observation,
        };
        assert_eq!(view.battlefield_card_count_v1(PlayerSeatV1::P0), 1);
        let battlefield = view
            .battlefield_card_v1(PlayerSeatV1::P0, 0)
            .expect("visible battlefield card");
        assert_eq!(battlefield.card_name_v1(), "Visible test creature");
        assert!(battlefield.tapped_v1());
        assert_eq!(battlefield.marked_damage_v1(), 2);
        assert_eq!(battlefield.plus_one_plus_one_counter_count_v1(), 1);
        assert_eq!(battlefield.visible_effective_power_v1(), Some(3));
        assert_eq!(battlefield.visible_effective_toughness_v1(), Some(3));

        assert_eq!(view.graveyard_card_count_v1(PlayerSeatV1::P0), 1);
        assert_eq!(
            view.graveyard_card_v1(PlayerSeatV1::P0, 0)
                .expect("visible graveyard card")
                .card_name_v1(),
            "Visible test creature"
        );
        assert_eq!(view.exile_card_count_v1(), 1);
        assert_eq!(
            view.exile_card_v1(0)
                .expect("visible exile card")
                .card_name_v1(),
            "Visible test creature"
        );
        let stack = view.stack_item_v1(0).expect("visible stack item");
        assert_eq!(stack.visible_stack_position_v1(), 0);
        assert_eq!(stack.controller_v1(), PlayerSeatV1::P0);
        assert_eq!(stack.visible_targets_v1().len(), 1);
        assert_eq!(stack.item_kind_v1(), StackItemKindV2::ActivatedAbility);
        assert_eq!(view.ordered_attackers_v1().len(), 1);
        assert_eq!(view.blocker_assignment_count_v1(), 1);
        assert_eq!(view.visible_object_relations_v1().len(), 1);
    }

    #[test]
    fn combined_memory_requires_one_exact_event_match_and_game() {
        let event = "1".repeat(64);
        let match_id = "2".repeat(64);
        let substituted_identity = "3".repeat(64);
        let lineage = |event_kind, event_identity_sha256, match_identity_sha256, game_number| {
            VisibleGameLineageRefV1 {
                event_kind,
                event_identity_sha256,
                match_identity_sha256,
                game_number,
            }
        };
        validate_combined_visible_game_lineage_v1(
            lineage(League, &event, &match_id, 2),
            lineage(League, &event, &match_id, 2),
        )
        .unwrap();
        for substitution in 0..4 {
            let result = match substitution {
                0 => validate_combined_visible_game_lineage_v1(
                    lineage(League, &event, &match_id, 2),
                    lineage(Challenge, &event, &match_id, 2),
                ),
                1 => validate_combined_visible_game_lineage_v1(
                    lineage(League, &event, &match_id, 2),
                    lineage(League, &substituted_identity, &match_id, 2),
                ),
                2 => validate_combined_visible_game_lineage_v1(
                    lineage(League, &event, &match_id, 2),
                    lineage(League, &event, &substituted_identity, 2),
                ),
                _ => validate_combined_visible_game_lineage_v1(
                    lineage(League, &event, &match_id, 2),
                    lineage(League, &event, &match_id, 3),
                ),
            };
            assert!(result.is_err());
        }
    }

    #[test]
    fn combined_memory_commitment_is_framed_and_domain_separated() {
        let left = commitment_v1(&[b"ab", b"c"]);
        let right = commitment_v1(&[b"a", b"bc"]);
        assert_eq!(left.len(), 64);
        assert_ne!(left, right);
    }

    #[test]
    fn visible_game_outcome_requires_one_final_player_relative_winner() {
        use MtgoVisibleGameLogEventKindV1::{OpeningHand, WonGame, WonMatch};
        use MtgoVisibleGameLogPlayerRoleV1::{ActingPlayer, Opponent};

        assert_eq!(
            derive_visible_game_winner_v1([
                (OpeningHand, Some(ActingPlayer)),
                (WonGame, Some(Opponent)),
            ])
            .unwrap(),
            MtgoCompetitivePlayerRelativeGameWinnerV1::Opponent
        );
        for events in [
            vec![(OpeningHand, Some(ActingPlayer))],
            vec![(WonGame, None)],
            vec![(WonGame, Some(ActingPlayer)), (OpeningHand, Some(Opponent))],
            vec![(WonMatch, Some(ActingPlayer))],
        ] {
            assert!(derive_visible_game_winner_v1(events).is_err());
        }
    }

    #[test]
    fn external_history_ordering_declares_two_independent_clocks() {
        assert_eq!(MTGO_COMPETITIVE_EXTERNAL_PUBLIC_HISTORY_SCHEMA_V1, 1);
        assert_eq!(
            serde_json::to_string(
                &MtgoCompetitiveExternalPublicHistoryOrderingV1::SeparateOrderedStreamsNoCrossSourceTotalOrder
            )
            .unwrap(),
            "\"separate_ordered_streams_no_cross_source_total_order\""
        );
    }
}
