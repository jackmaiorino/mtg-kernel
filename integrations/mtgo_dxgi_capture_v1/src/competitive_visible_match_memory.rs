use crate::{
    OpaqueMtgoCompetitiveMatchVisibleGameLogSnapshotV1,
    OpaqueMtgoCompetitiveVisibleGameLogSemanticsV1,
};
use mtgo_blackbox_v1::{
    CheckedUntrustedMtgoCompetitivePlayerVisibleGameHistoryV1,
    CheckedUntrustedMtgoPlayerVisibleConfirmedCombatDecisionV1, MtgoCompetitiveEventKindV1,
    MtgoCompetitivePlayerVisibleDecisionViewV1, MtgoCompetitivePlayerVisibleHistoryEntryViewV1,
    MtgoPlayerVisibleAttackerInclusionDecisionV1, MtgoPlayerVisibleCombatModelDecisionRecordV1,
    MtgoPlayerVisibleConfirmedDuelDecisionV1, MtgoPlayerVisibleDuelActionV1,
    MtgoPlayerVisibleDuelStateV1, MtgoPlayerVisibleMultiAttackerBlockerChoiceV1,
    MtgoPlayerVisibleMultiAttackerBlockerModelDecisionV1, MtgoPlayerVisiblePreparedCombatKindV1,
    MtgoPlayerVisibleSingleAttackerBlockerInclusionDecisionV1, MtgoVisibleGameLogEventKindV1,
    MtgoVisibleGameLogPlayerRoleV1, MtgoVisibleGameLogSemanticEventViewV1,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const COMBINED_COMPETITIVE_VISIBLE_GAME_MEMORY_DOMAIN_V1: &[u8] =
    b"mtgo-combined-competitive-player-visible-game-memory-v1";
const COMPETITIVE_VISIBLE_GAME_OUTCOME_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-player-visible-game-outcome-v1";
const COMPETITIVE_COMPLETED_MATCH_HISTORY_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-player-visible-completed-match-history-v1";

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

/// Narrow model-facing value for one confirmed visible decision. Adapter frame
/// numbers, provenance, kernel object references, and internal card IDs remain
/// private. The owned input uses only sanitized visible-object ordinals.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::MtgoCompetitiveExternalConfirmedDecisionV1;
/// fn cannot_read_adapter_provenance(value: &MtgoCompetitiveExternalConfirmedDecisionV1) {
///     let _ = value.source_frame_sequence_v1();
///     let _ = value.decision_commitment_sha256_v1();
///     let _ = value.observation_v1();
///     let _ = value.selected_semantic_v1();
/// }
/// ```
pub struct MtgoCompetitiveExternalConfirmedDecisionV1 {
    within_source_position: u64,
    player_visible_decision: MtgoPlayerVisibleConfirmedDuelDecisionV1,
}

/// One exact player-visible model call within a composite combat declaration.
/// Commitments, capture identities, client objects, and input receipts are
/// deliberately absent from this kernel-facing value.
pub enum MtgoCompetitiveExternalCombatModelDecisionV1 {
    AttackerInclusion {
        model_input: MtgoPlayerVisibleAttackerInclusionDecisionV1,
        selected_index: u32,
        selected_action: MtgoPlayerVisibleDuelActionV1,
    },
    SingleAttackerBlockerInclusion {
        model_input: MtgoPlayerVisibleSingleAttackerBlockerInclusionDecisionV1,
        selected_index: u32,
        selected_action: MtgoPlayerVisibleDuelActionV1,
    },
    MultiAttackerBlockerChoice {
        model_input: MtgoPlayerVisibleMultiAttackerBlockerModelDecisionV1,
        selected_index: u32,
        selected_choice: MtgoPlayerVisibleMultiAttackerBlockerChoiceV1,
    },
}

/// Narrow model-facing value for one completed combat transaction. It owns
/// only the visible states and the exact visible choices supplied to and made
/// by the model. Physical clicks and all adapter lineage remain private.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::MtgoCompetitiveExternalConfirmedCombatDecisionV1;
/// fn cannot_read_combat_transport(value: &MtgoCompetitiveExternalConfirmedCombatDecisionV1) {
///     let _ = value.decision_commitment_sha256_v1();
///     let _ = value.source_frame_sequence_v1();
///     let _ = value.confirmed_transitions_v1();
/// }
/// ```
pub struct MtgoCompetitiveExternalConfirmedCombatDecisionV1 {
    within_source_position: u64,
    combat_kind: MtgoPlayerVisiblePreparedCombatKindV1,
    source_visible_state: MtgoPlayerVisibleDuelStateV1,
    final_visible_state: MtgoPlayerVisibleDuelStateV1,
    model_decisions: Vec<MtgoCompetitiveExternalCombatModelDecisionV1>,
}

impl MtgoCompetitiveExternalConfirmedCombatDecisionV1 {
    pub fn within_source_position_v1(&self) -> u64 {
        self.within_source_position
    }

    pub fn combat_kind_v1(&self) -> MtgoPlayerVisiblePreparedCombatKindV1 {
        self.combat_kind
    }

    pub fn source_visible_state_v1(&self) -> &MtgoPlayerVisibleDuelStateV1 {
        &self.source_visible_state
    }

    pub fn final_visible_state_v1(&self) -> &MtgoPlayerVisibleDuelStateV1 {
        &self.final_visible_state
    }

    pub fn model_decisions_v1(&self) -> &[MtgoCompetitiveExternalCombatModelDecisionV1] {
        &self.model_decisions
    }
}

impl MtgoCompetitiveExternalConfirmedDecisionV1 {
    pub fn within_source_position_v1(&self) -> u64 {
        self.within_source_position
    }

    pub fn player_visible_decision_v1(&self) -> &MtgoPlayerVisibleConfirmedDuelDecisionV1 {
        &self.player_visible_decision
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
        decision: MtgoCompetitiveExternalConfirmedDecisionV1,
    ) -> Result<(), String>;

    fn consume_confirmed_combat_decision_v1(
        &mut self,
        _decision: MtgoCompetitiveExternalConfirmedCombatDecisionV1,
    ) -> Result<(), String> {
        Err("the external history consumer does not support visible combat transactions".to_owned())
    }

    fn finish_confirmed_decision_stream_v1(&mut self) -> Result<(), String>;

    fn consume_public_game_log_event_v1(
        &mut self,
        event: MtgoCompetitiveExternalPublicGameLogEventV1<'_>,
    ) -> Result<(), String>;

    fn finish_public_game_log_stream_v1(&mut self) -> Result<(), String>;

    fn finish_public_history_v1(&mut self) -> Result<Self::Output, String>;
}

/// Player-visible bounds for one complete earlier-game history supplied to a
/// pregame or sideboard head. Event mode and all adapter lineage stay outside
/// this callback because they are not needed to choose a game action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MtgoCompetitiveExternalCompletedMatchHistoryHeaderV1 {
    pub completed_game_count: usize,
}

/// One completed game's visible identity inside a best-of-three history. The
/// two source streams that follow retain independent clocks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MtgoCompetitiveExternalCompletedGameHeaderV1 {
    pub game_number: u8,
    pub winner: MtgoCompetitivePlayerRelativeGameWinnerV1,
    pub confirmed_decision_count: usize,
    pub public_event_count: usize,
}

/// Kernel-owned consumer boundary for the ordered completed games in one
/// match. `begin_completed_match_history_v1` is an authoritative replacement,
/// including when its count is zero, so a scorer must discard any prefix from
/// an earlier match. Within each game, confirmed decisions and rendered Game
/// Log facts remain separate ordered streams. No callback exposes event or
/// match IDs, commitments, raw text, paths, pixels, coordinates, or input
/// authority.
pub trait MtgoCompetitiveExternalCompletedMatchHistoryConsumerV1 {
    type Output;

    fn begin_completed_match_history_v1(
        &mut self,
        header: MtgoCompetitiveExternalCompletedMatchHistoryHeaderV1,
    ) -> Result<(), String>;

    fn begin_completed_game_v1(
        &mut self,
        header: MtgoCompetitiveExternalCompletedGameHeaderV1,
    ) -> Result<(), String>;

    fn consume_confirmed_decision_v1(
        &mut self,
        decision: MtgoCompetitiveExternalConfirmedDecisionV1,
    ) -> Result<(), String>;

    fn consume_confirmed_combat_decision_v1(
        &mut self,
        _decision: MtgoCompetitiveExternalConfirmedCombatDecisionV1,
    ) -> Result<(), String> {
        Err(
            "the external completed-history consumer does not support visible combat transactions"
                .to_owned(),
        )
    }

    fn finish_confirmed_decision_stream_v1(&mut self) -> Result<(), String>;

    fn consume_public_game_log_event_v1(
        &mut self,
        event: MtgoCompetitiveExternalPublicGameLogEventV1<'_>,
    ) -> Result<(), String>;

    fn finish_public_game_log_stream_v1(&mut self) -> Result<(), String>;

    fn finish_completed_game_v1(&mut self) -> Result<(), String>;

    fn finish_completed_match_history_v1(&mut self) -> Result<Self::Output, String>;
}

/// Explicitly replaces any previously imported completed-game prefix with an
/// empty game-one prefix. Calling no completed-history callback at all would
/// let a reused scorer accidentally retain an earlier match.
pub(crate) fn visit_empty_external_completed_match_history_v1<C>(
    consumer: &mut C,
) -> Result<C::Output, String>
where
    C: MtgoCompetitiveExternalCompletedMatchHistoryConsumerV1,
{
    consumer.begin_completed_match_history_v1(
        MtgoCompetitiveExternalCompletedMatchHistoryHeaderV1 {
            completed_game_count: 0,
        },
    )?;
    consumer.finish_completed_match_history_v1()
}

fn consume_ongoing_history_entry_v1<C>(
    consumer: &mut C,
    entry: MtgoCompetitivePlayerVisibleHistoryEntryViewV1<'_>,
) -> Result<(), String>
where
    C: MtgoCompetitiveExternalPublicHistoryConsumerV1,
{
    match entry {
        MtgoCompetitivePlayerVisibleHistoryEntryViewV1::OrdinaryDecision(decision) => consumer
            .consume_confirmed_decision_v1(MtgoCompetitiveExternalConfirmedDecisionV1 {
                within_source_position: decision.sequence_v1(),
                player_visible_decision: decision.player_visible_decision_v1().clone(),
            }),
        MtgoCompetitivePlayerVisibleHistoryEntryViewV1::CombatDecision(decision) => consumer
            .consume_confirmed_combat_decision_v1(external_combat_decision_v1(
                decision.sequence_v1(),
                decision.confirmed_combat_decision_v1(),
            )),
    }
}

fn consume_completed_history_entry_v1<C>(
    consumer: &mut C,
    entry: MtgoCompetitivePlayerVisibleHistoryEntryViewV1<'_>,
) -> Result<(), String>
where
    C: MtgoCompetitiveExternalCompletedMatchHistoryConsumerV1,
{
    match entry {
        MtgoCompetitivePlayerVisibleHistoryEntryViewV1::OrdinaryDecision(decision) => consumer
            .consume_confirmed_decision_v1(MtgoCompetitiveExternalConfirmedDecisionV1 {
                within_source_position: decision.sequence_v1(),
                player_visible_decision: decision.player_visible_decision_v1().clone(),
            }),
        MtgoCompetitivePlayerVisibleHistoryEntryViewV1::CombatDecision(decision) => consumer
            .consume_confirmed_combat_decision_v1(external_combat_decision_v1(
                decision.sequence_v1(),
                decision.confirmed_combat_decision_v1(),
            )),
    }
}

fn external_combat_decision_v1(
    within_source_position: u64,
    decision: &CheckedUntrustedMtgoPlayerVisibleConfirmedCombatDecisionV1,
) -> MtgoCompetitiveExternalConfirmedCombatDecisionV1 {
    let model_decisions = decision
        .model_decisions_v1()
        .iter()
        .map(|model_decision| match model_decision {
            MtgoPlayerVisibleCombatModelDecisionRecordV1::AttackerInclusion {
                model_input,
                selected_index,
                selected_action,
                ..
            } => MtgoCompetitiveExternalCombatModelDecisionV1::AttackerInclusion {
                model_input: model_input.clone(),
                selected_index: *selected_index,
                selected_action: selected_action.clone(),
            },
            MtgoPlayerVisibleCombatModelDecisionRecordV1::SingleAttackerBlockerInclusion {
                model_input,
                selected_index,
                selected_action,
                ..
            } => MtgoCompetitiveExternalCombatModelDecisionV1::SingleAttackerBlockerInclusion {
                model_input: model_input.clone(),
                selected_index: *selected_index,
                selected_action: selected_action.clone(),
            },
            MtgoPlayerVisibleCombatModelDecisionRecordV1::MultiAttackerBlockerChoice {
                model_input,
                selected_index,
                selected_choice,
                ..
            } => MtgoCompetitiveExternalCombatModelDecisionV1::MultiAttackerBlockerChoice {
                model_input: model_input.clone(),
                selected_index: *selected_index,
                selected_choice: selected_choice.clone(),
            },
        })
        .collect();
    MtgoCompetitiveExternalConfirmedCombatDecisionV1 {
        within_source_position,
        combat_kind: decision.combat_kind_v1(),
        source_visible_state: decision.source_visible_state_v1().clone(),
        final_visible_state: decision.final_visible_state_v1().clone(),
        model_decisions,
    }
}

/// Replays the complete current player-visible history snapshot for one
/// in-progress League or Challenge game. A consumer must replace its prior
/// imported MTGO history at `begin_public_history_v1`; repeated refreshes are
/// authoritative snapshots, not append-only deltas. This prevents duplicate
/// recurrent updates when the retained Game Log is reread before each score.
///
/// The optional confirmed-decision stream is absent before the model's first
/// in-game action. Game Log events remain eligible in that state, so the first
/// decision does not require a fabricated model action or adapter identifier.
impl OpaqueMtgoCompetitiveMatchVisibleGameLogSnapshotV1 {
    pub fn visit_ongoing_external_public_history_v1<C>(
        &self,
        confirmed_decisions: Option<&CheckedUntrustedMtgoCompetitivePlayerVisibleGameHistoryV1>,
        consumer: &mut C,
    ) -> Result<C::Output, String>
    where
        C: MtgoCompetitiveExternalPublicHistoryConsumerV1,
    {
        validate_optional_visible_decision_history_lineage_v1(
            VisibleGameLineageRefV1 {
                event_kind: self.event_kind_v1(),
                event_identity_sha256: self.event_identity_sha256_v1(),
                match_identity_sha256: self.match_identity_sha256_v1(),
                game_number: self.game_number_v1(),
            },
            confirmed_decisions,
        )?;
        let confirmed_decision_count = confirmed_decisions
            .map(CheckedUntrustedMtgoCompetitivePlayerVisibleGameHistoryV1::decision_count_v1)
            .unwrap_or(0);
        consumer.begin_public_history_v1(MtgoCompetitiveExternalPublicHistoryHeaderV1 {
            event_kind: self.event_kind_v1(),
            game_number: self.game_number_v1(),
            confirmed_decision_count,
            public_event_count: self.event_count_v1(),
        })?;

        if let Some(decisions) = confirmed_decisions {
            for index in 0..confirmed_decision_count {
                let entry = decisions.entry_v1(index).ok_or_else(|| {
                    "confirmed decision stream changed while it was being visited".to_owned()
                })?;
                consume_ongoing_history_entry_v1(consumer, entry)?;
            }
        }
        consumer.finish_confirmed_decision_stream_v1()?;

        for index in 0..self.event_count_v1() {
            let event = self.event_v1(index).ok_or_else(|| {
                "public Game Log stream changed while it was being visited".to_owned()
            })?;
            consumer.consume_public_game_log_event_v1(
                MtgoCompetitiveExternalPublicGameLogEventV1 { event },
            )?;
        }
        consumer.finish_public_game_log_stream_v1()?;
        consumer.finish_public_history_v1()
    }
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
    confirmed_decisions: Option<CheckedUntrustedMtgoCompetitivePlayerVisibleGameHistoryV1>,
    policy_deployment_commitment_sha256: String,
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

/// Move-only complete public history for the games already finished in one
/// League or Challenge match. A game-two decision owns exactly game one. A
/// game-three decision owns games one and two. The private outcomes retain the
/// exact adapter lineage while consumers receive only player-visible facts.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoCompetitiveCompletedMatchHistoryV1;
/// let _forged = OpaqueMtgoCompetitiveCompletedMatchHistoryV1 {};
/// ```
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoCompetitiveCompletedMatchHistoryV1;
/// fn require_clone<T: Clone>() {}
/// require_clone::<OpaqueMtgoCompetitiveCompletedMatchHistoryV1>();
/// ```
pub struct OpaqueMtgoCompetitiveCompletedMatchHistoryV1 {
    outcomes: Vec<OpaqueMtgoCompetitiveVisibleGameOutcomeV1>,
    history_commitment_sha256: String,
}

pub(crate) struct MtgoCompetitiveVisibleGameOutcomeLineageV1<'a> {
    pub event_kind: MtgoCompetitiveEventKindV1,
    pub event_identity_sha256: &'a str,
    pub match_identity_sha256: &'a str,
    pub game_number: u8,
    pub source_memory_commitment_sha256: &'a str,
    pub outcome_commitment_sha256: &'a str,
    pub winner: MtgoCompetitivePlayerRelativeGameWinnerV1,
    policy_deployment_commitment_sha256: &'a str,
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
            policy_deployment_commitment_sha256: &self.memory.policy_deployment_commitment_sha256,
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

impl OpaqueMtgoCompetitiveCompletedMatchHistoryV1 {
    pub fn completed_game_count_v1(&self) -> usize {
        self.outcomes.len()
    }

    pub fn ready_for_kernel_auxiliary_history_import_v1(&self) -> bool {
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

    pub fn visit_external_completed_match_history_v1<C>(
        &self,
        consumer: &mut C,
    ) -> Result<C::Output, String>
    where
        C: MtgoCompetitiveExternalCompletedMatchHistoryConsumerV1,
    {
        validate_completed_match_history_v1(&self.outcomes)?;
        consumer.begin_completed_match_history_v1(
            MtgoCompetitiveExternalCompletedMatchHistoryHeaderV1 {
                completed_game_count: self.outcomes.len(),
            },
        )?;
        for outcome in &self.outcomes {
            let memory = &outcome.memory;
            let lineage = memory.game_log.lineage_v1();
            consumer.begin_completed_game_v1(MtgoCompetitiveExternalCompletedGameHeaderV1 {
                game_number: lineage.game_number,
                winner: outcome.winner,
                confirmed_decision_count: memory.confirmed_decision_count_v1(),
                public_event_count: memory.public_event_count_v1(),
            })?;
            for index in 0..memory.confirmed_decision_count_v1() {
                let entry = memory.confirmed_entry_v1(index).ok_or_else(|| {
                    "completed-match confirmed decision stream changed while visited".to_owned()
                })?;
                consume_completed_history_entry_v1(consumer, entry)?;
            }
            consumer.finish_confirmed_decision_stream_v1()?;
            for index in 0..memory.public_event_count_v1() {
                let event = memory.public_event_v1(index).ok_or_else(|| {
                    "completed-match Game Log stream changed while visited".to_owned()
                })?;
                consumer.consume_public_game_log_event_v1(
                    MtgoCompetitiveExternalPublicGameLogEventV1 { event },
                )?;
            }
            consumer.finish_public_game_log_stream_v1()?;
            consumer.finish_completed_game_v1()?;
        }
        consumer.finish_completed_match_history_v1()
    }

    pub(crate) fn latest_lineage_v1(
        &self,
    ) -> Result<MtgoCompetitiveVisibleGameOutcomeLineageV1<'_>, String> {
        validate_completed_match_history_v1(&self.outcomes)?;
        self.outcomes
            .last()
            .map(OpaqueMtgoCompetitiveVisibleGameOutcomeV1::lineage_v1)
            .ok_or_else(|| "completed-match history is empty".to_owned())
    }

    pub(crate) fn history_commitment_sha256_v1(&self) -> &str {
        &self.history_commitment_sha256
    }

    pub(crate) fn policy_deployment_commitment_sha256_v1(&self) -> Result<&str, String> {
        validate_completed_match_history_v1(&self.outcomes)?;
        self.outcomes
            .first()
            .map(|outcome| outcome.memory.policy_deployment_commitment_sha256.as_str())
            .ok_or_else(|| "completed-match history is empty".to_owned())
    }

    pub(crate) fn player_relative_win_counts_v1(&self) -> Result<(u8, u8), String> {
        validate_completed_match_history_v1(&self.outcomes)?;
        let mut acting_player_wins = 0_u8;
        let mut opponent_wins = 0_u8;
        for outcome in &self.outcomes {
            match outcome.winner {
                MtgoCompetitivePlayerRelativeGameWinnerV1::ActingPlayer => {
                    acting_player_wins = acting_player_wins
                        .checked_add(1)
                        .ok_or("acting-player visible win count overflow")?;
                }
                MtgoCompetitivePlayerRelativeGameWinnerV1::Opponent => {
                    opponent_wins = opponent_wins
                        .checked_add(1)
                        .ok_or("opponent visible win count overflow")?;
                }
            }
        }
        Ok((acting_player_wins, opponent_wins))
    }

    pub(crate) fn advance_next_game_log_baseline_v1(
        self,
        runtime: &crate::OpaqueMtgoCompetitiveEventRuntimeV1,
    ) -> Result<
        (
            OpaqueMtgoCompetitiveCompletedMatchHistoryV1,
            crate::OpaqueMtgoCompetitiveVisibleGameLogBaselineV1,
        ),
        String,
    > {
        validate_completed_match_history_v1(&self.outcomes)?;
        let latest = self
            .outcomes
            .last()
            .ok_or("completed-match history is empty")?;
        let snapshot = match &latest.memory.game_log {
            OpaqueMtgoCompetitivePlayerVisibleGameLogSourceV1::MatchScoped(snapshot) => snapshot,
            OpaqueMtgoCompetitivePlayerVisibleGameLogSourceV1::LegacyProcessEpoch(_) => {
                return Err(
                    "legacy process-epoch Game Log cannot advance a competitive match".to_owned(),
                )
            }
        };
        let baseline = crate::probe::advance_competitive_visible_game_log_baseline_v1(
            snapshot.match_lease_v1(),
            runtime,
        )?;
        Ok((self, baseline))
    }
}

/// Starts a complete best-of-three public history from the visibly completed
/// first game. A game-two outcome cannot start a history because that would
/// silently discard game one.
pub fn begin_competitive_completed_match_history_v1(
    outcome: OpaqueMtgoCompetitiveVisibleGameOutcomeV1,
) -> Result<OpaqueMtgoCompetitiveCompletedMatchHistoryV1, String> {
    if outcome.lineage_v1().game_number != 1 {
        return Err("completed-match history must start with game one".to_owned());
    }
    finish_completed_match_history_v1(vec![outcome])
}

/// Appends the exact next completed game. Only game two may be appended because
/// a best-of-three game three has no later pregame or sideboard decision.
pub fn append_competitive_completed_match_history_v1(
    mut history: OpaqueMtgoCompetitiveCompletedMatchHistoryV1,
    outcome: OpaqueMtgoCompetitiveVisibleGameOutcomeV1,
) -> Result<OpaqueMtgoCompetitiveCompletedMatchHistoryV1, String> {
    validate_completed_match_history_v1(&history.outcomes)?;
    history.outcomes.push(outcome);
    finish_completed_match_history_v1(history.outcomes)
}

fn finish_completed_match_history_v1(
    outcomes: Vec<OpaqueMtgoCompetitiveVisibleGameOutcomeV1>,
) -> Result<OpaqueMtgoCompetitiveCompletedMatchHistoryV1, String> {
    validate_completed_match_history_v1(&outcomes)?;
    let mut parts = Vec::with_capacity(outcomes.len() * 3 + 1);
    parts.push((outcomes.len() as u64).to_be_bytes().to_vec());
    for outcome in &outcomes {
        let lineage = outcome.lineage_v1();
        parts.push(vec![lineage.game_number]);
        parts.push(lineage.source_memory_commitment_sha256.as_bytes().to_vec());
        parts.push(lineage.outcome_commitment_sha256.as_bytes().to_vec());
    }
    let refs = parts.iter().map(Vec::as_slice).collect::<Vec<_>>();
    let history_commitment_sha256 =
        commitment_with_domain_v1(COMPETITIVE_COMPLETED_MATCH_HISTORY_DOMAIN_V1, &refs);
    Ok(OpaqueMtgoCompetitiveCompletedMatchHistoryV1 {
        outcomes,
        history_commitment_sha256,
    })
}

fn validate_completed_match_history_v1(
    outcomes: &[OpaqueMtgoCompetitiveVisibleGameOutcomeV1],
) -> Result<(), String> {
    let lineages = outcomes
        .iter()
        .map(OpaqueMtgoCompetitiveVisibleGameOutcomeV1::lineage_v1)
        .collect::<Vec<_>>();
    validate_completed_match_history_lineages_v1(&lineages)
}

fn validate_completed_match_history_lineages_v1(
    lineages: &[MtgoCompetitiveVisibleGameOutcomeLineageV1<'_>],
) -> Result<(), String> {
    if lineages.is_empty() || lineages.len() > 2 {
        return Err("completed-match history must contain one or two games".to_owned());
    }
    let first = &lineages[0];
    for (index, lineage) in lineages.iter().enumerate() {
        let expected_game = u8::try_from(index + 1)
            .map_err(|_| "completed-match history game index overflow".to_owned())?;
        if lineage.event_kind != first.event_kind
            || lineage.event_identity_sha256 != first.event_identity_sha256
            || lineage.match_identity_sha256 != first.match_identity_sha256
            || lineage.game_number != expected_game
            || lineage.policy_deployment_commitment_sha256
                != first.policy_deployment_commitment_sha256
        {
            return Err(
                "completed-match history changed event, match, or exact game order".to_owned(),
            );
        }
    }
    Ok(())
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

    fn public_event_v1(&self, index: usize) -> Option<MtgoVisibleGameLogSemanticEventViewV1<'_>> {
        self.game_log.event_v1(index)
    }

    pub fn confirmed_decision_count_v1(&self) -> usize {
        self.confirmed_decisions.as_ref().map_or(
            0,
            CheckedUntrustedMtgoCompetitivePlayerVisibleGameHistoryV1::decision_count_v1,
        )
    }

    fn confirmed_decision_v1(
        &self,
        index: usize,
    ) -> Option<MtgoCompetitivePlayerVisibleDecisionViewV1<'_>> {
        self.confirmed_decisions
            .as_ref()
            .and_then(|decisions| decisions.decision_v1(index))
    }

    fn confirmed_entry_v1(
        &self,
        index: usize,
    ) -> Option<MtgoCompetitivePlayerVisibleHistoryEntryViewV1<'_>> {
        self.confirmed_decisions
            .as_ref()
            .and_then(|decisions| decisions.entry_v1(index))
    }

    pub fn game_number_v1(&self) -> u8 {
        self.game_log.lineage_v1().game_number
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
            let entry = self.confirmed_entry_v1(index).ok_or_else(|| {
                "confirmed decision stream changed while it was being visited".to_owned()
            })?;
            consume_ongoing_history_entry_v1(consumer, entry)?;
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
        let confirmed_decisions = self
            .confirmed_decisions
            .ok_or("zero-action visible game has no confirmed decision history to extract")?;
        match self.game_log {
            OpaqueMtgoCompetitivePlayerVisibleGameLogSourceV1::MatchScoped(source) => {
                Ok(((*source).into_match_lease_v1(), confirmed_decisions))
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
        Some(confirmed_decisions),
        None,
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
        Some(confirmed_decisions),
        None,
    )
}

/// Joins a terminal match-scoped Game Log to zero or more confirmed model
/// decisions. The explicit deployment commitment makes a zero-action game
/// possible without fabricating a model decision, for example when the
/// opponent concedes before the model receives priority.
pub(crate) fn bind_optional_match_scoped_competitive_player_visible_game_memory_v1(
    game_log: OpaqueMtgoCompetitiveMatchVisibleGameLogSnapshotV1,
    confirmed_decisions: Option<CheckedUntrustedMtgoCompetitivePlayerVisibleGameHistoryV1>,
    policy_deployment_commitment_sha256: &str,
) -> Result<OpaqueMtgoCompetitivePlayerVisibleGameMemoryV1, String> {
    bind_competitive_player_visible_game_memory_source_v1(
        OpaqueMtgoCompetitivePlayerVisibleGameLogSourceV1::MatchScoped(Box::new(game_log)),
        confirmed_decisions,
        Some(policy_deployment_commitment_sha256),
    )
}

fn bind_competitive_player_visible_game_memory_source_v1(
    game_log: OpaqueMtgoCompetitivePlayerVisibleGameLogSourceV1,
    confirmed_decisions: Option<CheckedUntrustedMtgoCompetitivePlayerVisibleGameHistoryV1>,
    explicit_policy_deployment_commitment_sha256: Option<&str>,
) -> Result<OpaqueMtgoCompetitivePlayerVisibleGameMemoryV1, String> {
    let game_log_lineage = game_log.lineage_v1();
    validate_optional_visible_decision_history_lineage_v1(
        game_log_lineage,
        confirmed_decisions.as_ref(),
    )?;
    let policy_deployment_commitment_sha256 = resolve_visible_game_policy_deployment_v1(
        confirmed_decisions
            .as_ref()
            .map(CheckedUntrustedMtgoCompetitivePlayerVisibleGameHistoryV1::policy_deployment_commitment_sha256_v1),
        explicit_policy_deployment_commitment_sha256,
    )?;
    let decision_history_commitment_sha256 = confirmed_decisions
        .as_ref()
        .map(
            CheckedUntrustedMtgoCompetitivePlayerVisibleGameHistoryV1::history_commitment_sha256_v1,
        )
        .unwrap_or("");
    let memory_commitment_sha256 = commitment_v1(&[
        game_log.source_commitment_sha256_v1().as_bytes(),
        game_log
            .semantic_projection_commitment_sha256_v1()
            .as_bytes(),
        decision_history_commitment_sha256.as_bytes(),
        policy_deployment_commitment_sha256.as_bytes(),
        game_log_lineage.event_identity_sha256.as_bytes(),
        game_log_lineage.match_identity_sha256.as_bytes(),
        &[game_log_lineage.game_number],
        b"dual_source_public_history_ready_for_explicit_kernel_import_no_scoring_no_input",
    ]);
    Ok(OpaqueMtgoCompetitivePlayerVisibleGameMemoryV1 {
        game_log,
        confirmed_decisions,
        policy_deployment_commitment_sha256,
        memory_commitment_sha256,
    })
}

fn resolve_visible_game_policy_deployment_v1(
    confirmed_history_deployment_sha256: Option<&str>,
    explicit_policy_deployment_commitment_sha256: Option<&str>,
) -> Result<String, String> {
    let deployment = match (
        confirmed_history_deployment_sha256,
        explicit_policy_deployment_commitment_sha256,
    ) {
        (Some(history), None) => history,
        (Some(history), Some(explicit)) if history == explicit => explicit,
        (None, Some(explicit)) => explicit,
        (Some(_), Some(_)) => {
            return Err("confirmed decisions changed the exact model deployment".to_owned())
        }
        (None, None) => {
            return Err("zero-action visible game lacks its exact model deployment".to_owned())
        }
    };
    if deployment.len() != 64
        || !deployment
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
    {
        return Err("visible game has an invalid model deployment commitment".to_owned());
    }
    Ok(deployment.to_owned())
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

fn validate_optional_visible_decision_history_lineage_v1(
    log: VisibleGameLineageRefV1<'_>,
    decisions: Option<&CheckedUntrustedMtgoCompetitivePlayerVisibleGameHistoryV1>,
) -> Result<(), String> {
    let Some(decisions) = decisions else {
        return Ok(());
    };
    validate_combined_visible_game_lineage_v1(
        log,
        VisibleGameLineageRefV1 {
            event_kind: decisions.event_kind_v1(),
            event_identity_sha256: decisions.event_identity_sha256_v1(),
            match_identity_sha256: decisions.match_identity_sha256_v1(),
            game_number: decisions.game_number_v1(),
        },
    )
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

    #[derive(Default)]
    struct EmptyCompletedHistoryConsumerV1 {
        began_with_count: Option<usize>,
        finished: bool,
    }

    impl MtgoCompetitiveExternalCompletedMatchHistoryConsumerV1 for EmptyCompletedHistoryConsumerV1 {
        type Output = (usize, bool);

        fn begin_completed_match_history_v1(
            &mut self,
            header: MtgoCompetitiveExternalCompletedMatchHistoryHeaderV1,
        ) -> Result<(), String> {
            self.began_with_count = Some(header.completed_game_count);
            Ok(())
        }

        fn begin_completed_game_v1(
            &mut self,
            _header: MtgoCompetitiveExternalCompletedGameHeaderV1,
        ) -> Result<(), String> {
            Err("empty history unexpectedly began a game".to_owned())
        }

        fn consume_confirmed_decision_v1(
            &mut self,
            _decision: MtgoCompetitiveExternalConfirmedDecisionV1,
        ) -> Result<(), String> {
            Err("empty history unexpectedly emitted a decision".to_owned())
        }

        fn finish_confirmed_decision_stream_v1(&mut self) -> Result<(), String> {
            Err("empty history unexpectedly finished a decision stream".to_owned())
        }

        fn consume_public_game_log_event_v1(
            &mut self,
            _event: MtgoCompetitiveExternalPublicGameLogEventV1<'_>,
        ) -> Result<(), String> {
            Err("empty history unexpectedly emitted a Game Log event".to_owned())
        }

        fn finish_public_game_log_stream_v1(&mut self) -> Result<(), String> {
            Err("empty history unexpectedly finished a Game Log stream".to_owned())
        }

        fn finish_completed_game_v1(&mut self) -> Result<(), String> {
            Err("empty history unexpectedly finished a game".to_owned())
        }

        fn finish_completed_match_history_v1(&mut self) -> Result<Self::Output, String> {
            self.finished = true;
            Ok((self.began_with_count.unwrap_or(usize::MAX), self.finished))
        }
    }

    #[test]
    fn game_one_completed_history_visit_explicitly_resets_a_reused_consumer() {
        let mut consumer = EmptyCompletedHistoryConsumerV1::default();
        assert_eq!(
            visit_empty_external_completed_match_history_v1(&mut consumer).unwrap(),
            (0, true)
        );
    }

    #[test]
    fn zero_action_visible_game_requires_exact_valid_deployment() {
        let deployment = "a".repeat(64);
        assert_eq!(
            resolve_visible_game_policy_deployment_v1(None, Some(&deployment)).unwrap(),
            deployment
        );
        assert!(resolve_visible_game_policy_deployment_v1(None, None).is_err());
        assert!(resolve_visible_game_policy_deployment_v1(None, Some("not-a-digest")).is_err());
        assert!(resolve_visible_game_policy_deployment_v1(
            Some(&"a".repeat(64)),
            Some(&"b".repeat(64)),
        )
        .is_err());
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
    fn ongoing_history_accepts_no_confirmed_decisions_before_first_action() {
        let lineage = VisibleGameLineageRefV1 {
            event_kind: League,
            event_identity_sha256: "event",
            match_identity_sha256: "match",
            game_number: 1,
        };
        validate_optional_visible_decision_history_lineage_v1(lineage, None).unwrap();
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

    #[test]
    fn completed_match_history_requires_exact_prefix_of_one_match() {
        let event = "1".repeat(64);
        let match_id = "2".repeat(64);
        let other = "3".repeat(64);
        let memory = "4".repeat(64);
        let outcome = "5".repeat(64);
        let deployment = "6".repeat(64);
        let lineage = |event_kind, event_id, match_id, game_number| {
            MtgoCompetitiveVisibleGameOutcomeLineageV1 {
                event_kind,
                event_identity_sha256: event_id,
                match_identity_sha256: match_id,
                game_number,
                source_memory_commitment_sha256: &memory,
                outcome_commitment_sha256: &outcome,
                winner: MtgoCompetitivePlayerRelativeGameWinnerV1::ActingPlayer,
                policy_deployment_commitment_sha256: &deployment,
            }
        };
        validate_completed_match_history_lineages_v1(&[lineage(League, &event, &match_id, 1)])
            .unwrap();
        validate_completed_match_history_lineages_v1(&[
            lineage(League, &event, &match_id, 1),
            lineage(League, &event, &match_id, 2),
        ])
        .unwrap();

        assert!(validate_completed_match_history_lineages_v1(&[]).is_err());
        assert!(validate_completed_match_history_lineages_v1(&[
            lineage(League, &event, &match_id, 1),
            lineage(League, &event, &match_id, 2),
            lineage(League, &event, &match_id, 3),
        ])
        .is_err());
        for invalid in [
            vec![lineage(League, &event, &match_id, 2)],
            vec![
                lineage(League, &event, &match_id, 1),
                lineage(League, &event, &match_id, 1),
            ],
            vec![
                lineage(League, &event, &match_id, 1),
                lineage(Challenge, &event, &match_id, 2),
            ],
            vec![
                lineage(League, &event, &match_id, 1),
                lineage(League, &other, &match_id, 2),
            ],
            vec![
                lineage(League, &event, &match_id, 1),
                lineage(League, &event, &other, 2),
            ],
        ] {
            assert!(validate_completed_match_history_lineages_v1(&invalid).is_err());
        }

        let other_deployment = "7".repeat(64);
        let first = lineage(League, &event, &match_id, 1);
        let second = MtgoCompetitiveVisibleGameOutcomeLineageV1 {
            event_kind: League,
            event_identity_sha256: &event,
            match_identity_sha256: &match_id,
            game_number: 2,
            source_memory_commitment_sha256: &memory,
            outcome_commitment_sha256: &outcome,
            winner: MtgoCompetitivePlayerRelativeGameWinnerV1::Opponent,
            policy_deployment_commitment_sha256: &other_deployment,
        };
        assert!(validate_completed_match_history_lineages_v1(&[first, second]).is_err());
    }
}
