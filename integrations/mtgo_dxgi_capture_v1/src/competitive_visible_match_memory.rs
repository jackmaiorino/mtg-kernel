use crate::{
    OpaqueMtgoCompetitiveMatchVisibleGameLogSnapshotV1,
    OpaqueMtgoCompetitiveVisibleGameLogSemanticsV1,
};
use mtgo_blackbox_v1::{
    ActionSemanticV1, CardPrivateV1, CheckedUntrustedMtgoCompetitivePlayerVisibleGameHistoryV1,
    KnownLibraryCardV4, MtgoCompetitiveEventKindV1, MtgoCompetitivePlayerVisibleDecisionViewV1,
    MtgoVisibleGameLogEventKindV1, MtgoVisibleGameLogPlayerRoleV1,
    MtgoVisibleGameLogSemanticEventViewV1, ObservationV5, PlayerSeatV1, ZoneIndependentStepV1,
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
/// }
/// ```
pub struct MtgoCompetitiveExternalVisibleObservationV1<'a> {
    observation: &'a ObservationV5,
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
    use mtgo_blackbox_v1::MtgoCompetitiveEventKindV1::{Challenge, League};

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
