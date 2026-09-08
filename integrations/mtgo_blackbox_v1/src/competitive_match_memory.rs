use crate::{
    CheckedUntrustedMtgoCompetitiveGameplayPostconditionV1,
    CheckedUntrustedMtgoDirectVisibleGameplayPostconditionV1,
    CheckedUntrustedMtgoPlayerVisibleConfirmedCombatDecisionV1,
    CheckedUntrustedMtgoPlayerVisibleGameplayPostconditionV1, MtgoCompetitiveEventKindV1,
    MtgoContractErrorV1, MtgoPlayerVisibleConfirmedDuelDecisionV1, MtgoPlayerVisibleDuelActionV1,
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::HashSet;

pub const MTGO_COMPETITIVE_PLAYER_VISIBLE_GAME_HISTORY_SCHEMA_V1: u32 = 1;

const PLAYER_VISIBLE_GAME_HISTORY_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-player-visible-game-history-v1";
const MAX_PLAYER_VISIBLE_GAME_DECISIONS_V1: usize = 4_096;

#[derive(Serialize)]
struct MtgoCompetitivePlayerVisibleDecisionRecordV1 {
    sequence: u64,
    source_frame_id: u64,
    source_frame_sequence: u64,
    source_frame_sha256: String,
    after_frame_id: u64,
    after_frame_sequence: u64,
    player_visible_decision: MtgoPlayerVisibleConfirmedDuelDecisionV1,
    decision_commitment_sha256: String,
    selection_commitment_sha256: String,
    visible_postcondition_commitment_sha256: String,
}

#[derive(Serialize)]
struct MtgoCompetitivePlayerVisibleCombatDecisionRecordV1 {
    sequence: u64,
    source_frame_id: u64,
    source_frame_sequence: u64,
    source_visible_result_sha256: String,
    after_frame_id: u64,
    after_frame_sequence: u64,
    confirmed_combat_decision: CheckedUntrustedMtgoPlayerVisibleConfirmedCombatDecisionV1,
    decision_commitment_sha256: String,
    visible_postcondition_commitment_sha256: String,
}

#[derive(Serialize)]
#[serde(untagged)]
enum MtgoCompetitivePlayerVisibleHistoryEntryRecordV1 {
    OrdinaryDecision(MtgoCompetitivePlayerVisibleDecisionRecordV1),
    CombatDecision(MtgoCompetitivePlayerVisibleCombatDecisionRecordV1),
}

impl MtgoCompetitivePlayerVisibleHistoryEntryRecordV1 {
    fn sequence_v1(&self) -> u64 {
        match self {
            Self::OrdinaryDecision(record) => record.sequence,
            Self::CombatDecision(record) => record.sequence,
        }
    }

    fn source_frame_sequence_v1(&self) -> u64 {
        match self {
            Self::OrdinaryDecision(record) => record.source_frame_sequence,
            Self::CombatDecision(record) => record.source_frame_sequence,
        }
    }

    fn after_frame_sequence_v1(&self) -> u64 {
        match self {
            Self::OrdinaryDecision(record) => record.after_frame_sequence,
            Self::CombatDecision(record) => record.after_frame_sequence,
        }
    }

    fn decision_commitment_sha256_v1(&self) -> &str {
        match self {
            Self::OrdinaryDecision(record) => &record.decision_commitment_sha256,
            Self::CombatDecision(record) => &record.decision_commitment_sha256,
        }
    }

    fn selection_commitment_sha256_v1(&self) -> &str {
        match self {
            Self::OrdinaryDecision(record) => &record.selection_commitment_sha256,
            Self::CombatDecision(record) => record
                .confirmed_combat_decision
                .decision_commitment_sha256_v1(),
        }
    }

    fn visible_postcondition_commitment_sha256_v1(&self) -> &str {
        match self {
            Self::OrdinaryDecision(record) => &record.visible_postcondition_commitment_sha256,
            Self::CombatDecision(record) => &record.visible_postcondition_commitment_sha256,
        }
    }
}

#[derive(Serialize)]
struct MtgoCompetitivePlayerVisibleGameHistoryRecordV1 {
    schema_version: u32,
    history_id: String,
    event_kind: MtgoCompetitiveEventKindV1,
    event_identity_sha256: String,
    match_identity_sha256: String,
    game_number: u8,
    policy_deployment_commitment_sha256: String,
    decisions: Vec<MtgoCompetitivePlayerVisibleHistoryEntryRecordV1>,
    safe_for_model_scoring: bool,
    safe_for_input: bool,
    permits_event_entry: bool,
    permits_spending: bool,
}

/// Read-only, coordinate-free view of one model decision whose selected
/// player-visible action received its declared newer visible postcondition.
///
/// The view contains only sanitized player-visible state and action values. It
/// contains no aggregate kernel observation, stable object identifiers,
/// pixels, rectangles, process handles, input primitives, or opponent-hidden
/// state source.
pub struct MtgoCompetitivePlayerVisibleDecisionViewV1<'a> {
    record: &'a MtgoCompetitivePlayerVisibleDecisionRecordV1,
}

pub struct MtgoCompetitivePlayerVisibleCombatDecisionViewV1<'a> {
    record: &'a MtgoCompetitivePlayerVisibleCombatDecisionRecordV1,
}

impl<'a> MtgoCompetitivePlayerVisibleCombatDecisionViewV1<'a> {
    pub fn sequence_v1(&self) -> u64 {
        self.record.sequence
    }

    pub fn confirmed_combat_decision_v1(
        &self,
    ) -> &CheckedUntrustedMtgoPlayerVisibleConfirmedCombatDecisionV1 {
        &self.record.confirmed_combat_decision
    }
}

pub enum MtgoCompetitivePlayerVisibleHistoryEntryViewV1<'a> {
    OrdinaryDecision(MtgoCompetitivePlayerVisibleDecisionViewV1<'a>),
    CombatDecision(MtgoCompetitivePlayerVisibleCombatDecisionViewV1<'a>),
}

impl MtgoCompetitivePlayerVisibleHistoryEntryViewV1<'_> {
    pub fn sequence_v1(&self) -> u64 {
        match self {
            Self::OrdinaryDecision(view) => view.sequence_v1(),
            Self::CombatDecision(view) => view.sequence_v1(),
        }
    }
}

pub struct MtgoCompetitivePlayerVisibleCombatHistoryContextV1 {
    pub event_kind: MtgoCompetitiveEventKindV1,
    pub event_identity_sha256: String,
    pub match_identity_sha256: String,
    pub game_number: u8,
    pub policy_deployment_commitment_sha256: String,
    pub source_frame_id: u64,
    pub source_frame_sequence: u64,
    pub after_frame_id: u64,
    pub after_frame_sequence: u64,
    pub visible_postcondition_commitment_sha256: String,
}

impl<'a> MtgoCompetitivePlayerVisibleDecisionViewV1<'a> {
    pub fn sequence_v1(&self) -> u64 {
        self.record.sequence
    }

    /// Returns the retained decision as owned player-visible state plus its one
    /// confirmed selected action. Kernel and adapter identifiers remain in the
    /// private history record and cannot cross this public seam.
    pub fn player_visible_decision_v1(&self) -> &MtgoPlayerVisibleConfirmedDuelDecisionV1 {
        &self.record.player_visible_decision
    }
}

/// Move-only player-visible semantic memory for one exact game lineage.
///
/// Only confirmed gameplay postconditions can extend the record. This closes
/// the previous adapter gap where the competitive game session retained only
/// an action count and family. It remains checked-untrusted and deliberately
/// cannot score a sideboard decision or authorize input. Opponent-only
/// transitions and the terminal game result still require their own visible
/// observation sources before a complete match-memory encoder can be admitted.
///
/// ```compile_fail
/// use mtgo_blackbox_v1::CheckedUntrustedMtgoCompetitivePlayerVisibleGameHistoryV1;
/// fn cannot_act(value: &CheckedUntrustedMtgoCompetitivePlayerVisibleGameHistoryV1) {
///     let _ = value.input_command();
///     let _ = value.sideboard_score();
/// }
/// ```
///
/// ```compile_fail
/// use mtgo_blackbox_v1::CheckedUntrustedMtgoCompetitivePlayerVisibleGameHistoryV1;
/// fn require_clone<T: Clone>() {}
/// require_clone::<CheckedUntrustedMtgoCompetitivePlayerVisibleGameHistoryV1>();
/// ```
pub struct CheckedUntrustedMtgoCompetitivePlayerVisibleGameHistoryV1 {
    record: MtgoCompetitivePlayerVisibleGameHistoryRecordV1,
    history_commitment_sha256: String,
}

impl CheckedUntrustedMtgoCompetitivePlayerVisibleGameHistoryV1 {
    pub fn event_kind_v1(&self) -> MtgoCompetitiveEventKindV1 {
        self.record.event_kind
    }

    pub fn event_identity_sha256_v1(&self) -> &str {
        &self.record.event_identity_sha256
    }

    pub fn match_identity_sha256_v1(&self) -> &str {
        &self.record.match_identity_sha256
    }

    pub fn game_number_v1(&self) -> u8 {
        self.record.game_number
    }

    pub fn policy_deployment_commitment_sha256_v1(&self) -> &str {
        &self.record.policy_deployment_commitment_sha256
    }

    pub fn decision_count_v1(&self) -> usize {
        self.record.decisions.len()
    }

    pub fn decision_v1(
        &self,
        index: usize,
    ) -> Option<MtgoCompetitivePlayerVisibleDecisionViewV1<'_>> {
        match self.record.decisions.get(index)? {
            MtgoCompetitivePlayerVisibleHistoryEntryRecordV1::OrdinaryDecision(record) => {
                Some(MtgoCompetitivePlayerVisibleDecisionViewV1 { record })
            }
            MtgoCompetitivePlayerVisibleHistoryEntryRecordV1::CombatDecision(_) => None,
        }
    }

    pub fn entry_v1(
        &self,
        index: usize,
    ) -> Option<MtgoCompetitivePlayerVisibleHistoryEntryViewV1<'_>> {
        match self.record.decisions.get(index)? {
            MtgoCompetitivePlayerVisibleHistoryEntryRecordV1::OrdinaryDecision(record) => Some(
                MtgoCompetitivePlayerVisibleHistoryEntryViewV1::OrdinaryDecision(
                    MtgoCompetitivePlayerVisibleDecisionViewV1 { record },
                ),
            ),
            MtgoCompetitivePlayerVisibleHistoryEntryRecordV1::CombatDecision(record) => Some(
                MtgoCompetitivePlayerVisibleHistoryEntryViewV1::CombatDecision(
                    MtgoCompetitivePlayerVisibleCombatDecisionViewV1 { record },
                ),
            ),
        }
    }

    pub(crate) fn last_after_frame_sequence_v1(&self) -> u64 {
        self.record
            .decisions
            .last()
            .expect("validated visible decision history is nonempty")
            .after_frame_sequence_v1()
    }

    pub fn history_commitment_sha256_v1(&self) -> &str {
        &self.history_commitment_sha256
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
}

pub fn begin_checked_untrusted_competitive_player_visible_game_history_v1(
    history_id: &str,
    confirmed: CheckedUntrustedMtgoCompetitiveGameplayPostconditionV1,
) -> Result<CheckedUntrustedMtgoCompetitivePlayerVisibleGameHistoryV1, MtgoContractErrorV1> {
    validate_identifier_v1(history_id)?;
    let record = MtgoCompetitivePlayerVisibleGameHistoryRecordV1 {
        schema_version: MTGO_COMPETITIVE_PLAYER_VISIBLE_GAME_HISTORY_SCHEMA_V1,
        history_id: history_id.to_owned(),
        event_kind: confirmed.event_kind(),
        event_identity_sha256: confirmed.event_identity_sha256_v1().to_owned(),
        match_identity_sha256: confirmed.match_identity_sha256_v1().to_owned(),
        game_number: confirmed.game_number(),
        policy_deployment_commitment_sha256: confirmed.deployment_commitment_sha256_v1().to_owned(),
        decisions: vec![
            MtgoCompetitivePlayerVisibleHistoryEntryRecordV1::OrdinaryDecision(decision_record_v1(
                1, &confirmed,
            )?),
        ],
        safe_for_model_scoring: false,
        safe_for_input: false,
        permits_event_entry: false,
        permits_spending: false,
    };
    finish_v1(record)
}

pub fn append_checked_untrusted_competitive_player_visible_game_history_v1(
    mut history: CheckedUntrustedMtgoCompetitivePlayerVisibleGameHistoryV1,
    confirmed: CheckedUntrustedMtgoCompetitiveGameplayPostconditionV1,
) -> Result<CheckedUntrustedMtgoCompetitivePlayerVisibleGameHistoryV1, MtgoContractErrorV1> {
    if history.record.decisions.len() >= MAX_PLAYER_VISIBLE_GAME_DECISIONS_V1 {
        return Err(error_v1(
            "competitive_player_visible_history_limit",
            "the per-game visible decision history reached its fixed bound",
        ));
    }
    if history.record.event_kind != confirmed.event_kind()
        || history.record.event_identity_sha256 != confirmed.event_identity_sha256_v1()
        || history.record.match_identity_sha256 != confirmed.match_identity_sha256_v1()
        || history.record.game_number != confirmed.game_number()
        || history.record.policy_deployment_commitment_sha256
            != confirmed.deployment_commitment_sha256_v1()
    {
        return Err(error_v1(
            "competitive_player_visible_history_lineage",
            "confirmed gameplay must retain the exact event, match, game, and deployment",
        ));
    }
    let prior = history
        .record
        .decisions
        .last()
        .expect("validated history is nonempty");
    if confirmed.source_frame_sequence_v1() < prior.after_frame_sequence_v1()
        || confirmed.source_frame_sequence_v1() <= prior.source_frame_sequence_v1()
        || confirmed.after_frame_sequence() <= prior.after_frame_sequence_v1()
    {
        return Err(error_v1(
            "competitive_player_visible_history_order",
            "confirmed gameplay decisions must form one monotonic visible frame lineage",
        ));
    }
    if history.record.decisions.iter().any(|decision| {
        decision.decision_commitment_sha256_v1() == confirmed.decision_commitment_sha256_v1()
            || decision.selection_commitment_sha256_v1()
                == confirmed.selection_commitment_sha256_v1()
            || decision.visible_postcondition_commitment_sha256_v1()
                == confirmed.confirmation_commitment_sha256()
    }) {
        return Err(error_v1(
            "competitive_player_visible_history_duplicate",
            "a decision, selection, or visible postcondition cannot be appended twice",
        ));
    }
    let sequence = u64::try_from(history.record.decisions.len() + 1).map_err(|_| {
        error_v1(
            "competitive_player_visible_history_sequence",
            "decision sequence overflow",
        )
    })?;
    history.record.decisions.push(
        MtgoCompetitivePlayerVisibleHistoryEntryRecordV1::OrdinaryDecision(decision_record_v1(
            sequence, &confirmed,
        )?),
    );
    finish_v1(history.record)
}

/// Starts the same player-visible history from the all-family visible-only
/// postcondition contract. This path contains no aggregate kernel semantic or
/// legacy action-calibration identity.
pub fn begin_checked_untrusted_competitive_player_visible_game_history_from_player_visible_postcondition_v1(
    history_id: &str,
    confirmed: CheckedUntrustedMtgoPlayerVisibleGameplayPostconditionV1,
) -> Result<CheckedUntrustedMtgoCompetitivePlayerVisibleGameHistoryV1, MtgoContractErrorV1> {
    validate_identifier_v1(history_id)?;
    let record = MtgoCompetitivePlayerVisibleGameHistoryRecordV1 {
        schema_version: MTGO_COMPETITIVE_PLAYER_VISIBLE_GAME_HISTORY_SCHEMA_V1,
        history_id: history_id.to_owned(),
        event_kind: confirmed.event_kind_v1(),
        event_identity_sha256: confirmed.event_identity_sha256_v1().to_owned(),
        match_identity_sha256: confirmed.match_identity_sha256_v1().to_owned(),
        game_number: confirmed.game_number_v1(),
        policy_deployment_commitment_sha256: confirmed.deployment_commitment_sha256_v1().to_owned(),
        decisions: vec![
            MtgoCompetitivePlayerVisibleHistoryEntryRecordV1::OrdinaryDecision(
                player_visible_postcondition_decision_record_v1(1, &confirmed)?,
            ),
        ],
        safe_for_model_scoring: false,
        safe_for_input: false,
        permits_event_entry: false,
        permits_spending: false,
    };
    finish_v1(record)
}

/// Appends one all-family visible-only confirmation while preserving the same
/// event, match, game, deployment, and monotonic visible-frame lineage.
pub fn append_checked_untrusted_competitive_player_visible_game_history_from_player_visible_postcondition_v1(
    mut history: CheckedUntrustedMtgoCompetitivePlayerVisibleGameHistoryV1,
    confirmed: CheckedUntrustedMtgoPlayerVisibleGameplayPostconditionV1,
) -> Result<CheckedUntrustedMtgoCompetitivePlayerVisibleGameHistoryV1, MtgoContractErrorV1> {
    if history.record.decisions.len() >= MAX_PLAYER_VISIBLE_GAME_DECISIONS_V1 {
        return Err(error_v1(
            "competitive_player_visible_history_limit",
            "the per-game visible decision history reached its fixed bound",
        ));
    }
    if history.record.event_kind != confirmed.event_kind_v1()
        || history.record.event_identity_sha256 != confirmed.event_identity_sha256_v1()
        || history.record.match_identity_sha256 != confirmed.match_identity_sha256_v1()
        || history.record.game_number != confirmed.game_number_v1()
        || history.record.policy_deployment_commitment_sha256
            != confirmed.deployment_commitment_sha256_v1()
    {
        return Err(error_v1(
            "competitive_player_visible_history_lineage",
            "confirmed gameplay must retain the exact event, match, game, and deployment",
        ));
    }
    let prior = history
        .record
        .decisions
        .last()
        .expect("validated history is nonempty");
    if confirmed.source_frame_sequence_v1() < prior.after_frame_sequence_v1()
        || confirmed.source_frame_sequence_v1() <= prior.source_frame_sequence_v1()
        || confirmed.after_frame_sequence_v1() <= prior.after_frame_sequence_v1()
    {
        return Err(error_v1(
            "competitive_player_visible_history_order",
            "confirmed gameplay decisions must form one monotonic visible frame lineage",
        ));
    }
    if history.record.decisions.iter().any(|decision| {
        decision.decision_commitment_sha256_v1() == confirmed.decision_commitment_sha256_v1()
            || decision.selection_commitment_sha256_v1()
                == confirmed.selection_commitment_sha256_v1()
            || decision.visible_postcondition_commitment_sha256_v1()
                == confirmed.confirmation_commitment_sha256_v1()
    }) {
        return Err(error_v1(
            "competitive_player_visible_history_duplicate",
            "a decision, selection, or visible postcondition cannot be appended twice",
        ));
    }
    let sequence = u64::try_from(history.record.decisions.len() + 1).map_err(|_| {
        error_v1(
            "competitive_player_visible_history_sequence",
            "decision sequence overflow",
        )
    })?;
    history.record.decisions.push(
        MtgoCompetitivePlayerVisibleHistoryEntryRecordV1::OrdinaryDecision(
            player_visible_postcondition_decision_record_v1(sequence, &confirmed)?,
        ),
    );
    finish_v1(history.record)
}

/// Starts the same player-visible history from a direct client action that was
/// confirmed by a newer composed-frame transition. The history records only
/// the selected visible action and observed visible result, never the client
/// object or dispatch mechanism.
pub fn begin_checked_untrusted_competitive_player_visible_game_history_from_direct_visible_postcondition_v1(
    history_id: &str,
    confirmed: CheckedUntrustedMtgoDirectVisibleGameplayPostconditionV1,
) -> Result<CheckedUntrustedMtgoCompetitivePlayerVisibleGameHistoryV1, MtgoContractErrorV1> {
    validate_identifier_v1(history_id)?;
    let record = MtgoCompetitivePlayerVisibleGameHistoryRecordV1 {
        schema_version: MTGO_COMPETITIVE_PLAYER_VISIBLE_GAME_HISTORY_SCHEMA_V1,
        history_id: history_id.to_owned(),
        event_kind: confirmed.event_kind_v1(),
        event_identity_sha256: confirmed.event_identity_sha256_v1().to_owned(),
        match_identity_sha256: confirmed.match_identity_sha256_v1().to_owned(),
        game_number: confirmed.game_number_v1(),
        policy_deployment_commitment_sha256: confirmed.deployment_commitment_sha256_v1().to_owned(),
        decisions: vec![
            MtgoCompetitivePlayerVisibleHistoryEntryRecordV1::OrdinaryDecision(
                direct_visible_postcondition_decision_record_v1(1, &confirmed)?,
            ),
        ],
        safe_for_model_scoring: false,
        safe_for_input: false,
        permits_event_entry: false,
        permits_spending: false,
    };
    finish_v1(record)
}

/// Appends one direct client action only after its newer player-visible result
/// has been confirmed, while retaining the exact event, match, game,
/// deployment, and visible-frame lineage.
pub fn append_checked_untrusted_competitive_player_visible_game_history_from_direct_visible_postcondition_v1(
    mut history: CheckedUntrustedMtgoCompetitivePlayerVisibleGameHistoryV1,
    confirmed: CheckedUntrustedMtgoDirectVisibleGameplayPostconditionV1,
) -> Result<CheckedUntrustedMtgoCompetitivePlayerVisibleGameHistoryV1, MtgoContractErrorV1> {
    if history.record.decisions.len() >= MAX_PLAYER_VISIBLE_GAME_DECISIONS_V1 {
        return Err(error_v1(
            "competitive_player_visible_history_limit",
            "the per-game visible decision history reached its fixed bound",
        ));
    }
    if history.record.event_kind != confirmed.event_kind_v1()
        || history.record.event_identity_sha256 != confirmed.event_identity_sha256_v1()
        || history.record.match_identity_sha256 != confirmed.match_identity_sha256_v1()
        || history.record.game_number != confirmed.game_number_v1()
        || history.record.policy_deployment_commitment_sha256
            != confirmed.deployment_commitment_sha256_v1()
    {
        return Err(error_v1(
            "competitive_player_visible_history_lineage",
            "confirmed gameplay must retain the exact event, match, game, and deployment",
        ));
    }
    let prior = history
        .record
        .decisions
        .last()
        .expect("validated history is nonempty");
    if confirmed.source_frame_sequence_v1() < prior.after_frame_sequence_v1()
        || confirmed.source_frame_sequence_v1() <= prior.source_frame_sequence_v1()
        || confirmed.after_frame_sequence_v1() <= prior.after_frame_sequence_v1()
    {
        return Err(error_v1(
            "competitive_player_visible_history_order",
            "confirmed gameplay decisions must form one monotonic visible frame lineage",
        ));
    }
    if history.record.decisions.iter().any(|decision| {
        decision.decision_commitment_sha256_v1() == confirmed.decision_commitment_sha256_v1()
            || decision.selection_commitment_sha256_v1()
                == confirmed.selection_commitment_sha256_v1()
            || decision.visible_postcondition_commitment_sha256_v1()
                == confirmed.confirmation_commitment_sha256_v1()
    }) {
        return Err(error_v1(
            "competitive_player_visible_history_duplicate",
            "a decision, selection, or visible postcondition cannot be appended twice",
        ));
    }
    let sequence = u64::try_from(history.record.decisions.len() + 1).map_err(|_| {
        error_v1(
            "competitive_player_visible_history_sequence",
            "decision sequence overflow",
        )
    })?;
    history.record.decisions.push(
        MtgoCompetitivePlayerVisibleHistoryEntryRecordV1::OrdinaryDecision(
            direct_visible_postcondition_decision_record_v1(sequence, &confirmed)?,
        ),
    );
    finish_v1(history.record)
}

pub fn begin_checked_untrusted_competitive_player_visible_game_history_from_combat_v1(
    history_id: &str,
    context: MtgoCompetitivePlayerVisibleCombatHistoryContextV1,
    confirmed: CheckedUntrustedMtgoPlayerVisibleConfirmedCombatDecisionV1,
) -> Result<CheckedUntrustedMtgoCompetitivePlayerVisibleGameHistoryV1, MtgoContractErrorV1> {
    validate_identifier_v1(history_id)?;
    let event_kind = context.event_kind;
    let event_identity_sha256 = context.event_identity_sha256.clone();
    let match_identity_sha256 = context.match_identity_sha256.clone();
    let game_number = context.game_number;
    let policy_deployment_commitment_sha256 = context.policy_deployment_commitment_sha256.clone();
    let combat_record = combat_decision_record_v1(1, context, confirmed)?;
    let record = MtgoCompetitivePlayerVisibleGameHistoryRecordV1 {
        schema_version: MTGO_COMPETITIVE_PLAYER_VISIBLE_GAME_HISTORY_SCHEMA_V1,
        history_id: history_id.to_owned(),
        event_kind,
        event_identity_sha256,
        match_identity_sha256,
        game_number,
        policy_deployment_commitment_sha256,
        decisions: vec![
            MtgoCompetitivePlayerVisibleHistoryEntryRecordV1::CombatDecision(combat_record),
        ],
        safe_for_model_scoring: false,
        safe_for_input: false,
        permits_event_entry: false,
        permits_spending: false,
    };
    finish_v1(record)
}

pub fn append_checked_untrusted_competitive_player_visible_game_history_from_combat_v1(
    mut history: CheckedUntrustedMtgoCompetitivePlayerVisibleGameHistoryV1,
    context: MtgoCompetitivePlayerVisibleCombatHistoryContextV1,
    confirmed: CheckedUntrustedMtgoPlayerVisibleConfirmedCombatDecisionV1,
) -> Result<CheckedUntrustedMtgoCompetitivePlayerVisibleGameHistoryV1, MtgoContractErrorV1> {
    if history.record.decisions.len() >= MAX_PLAYER_VISIBLE_GAME_DECISIONS_V1 {
        return Err(error_v1(
            "competitive_player_visible_history_limit",
            "the per-game visible decision history reached its fixed bound",
        ));
    }
    if history.record.event_kind != context.event_kind
        || history.record.event_identity_sha256 != context.event_identity_sha256
        || history.record.match_identity_sha256 != context.match_identity_sha256
        || history.record.game_number != context.game_number
        || history.record.policy_deployment_commitment_sha256
            != context.policy_deployment_commitment_sha256
        || confirmed.deployment_commitment_sha256_v1()
            != history.record.policy_deployment_commitment_sha256
    {
        return Err(error_v1(
            "competitive_player_visible_history_lineage",
            "confirmed combat must retain the exact event, match, game, and deployment",
        ));
    }
    let prior = history
        .record
        .decisions
        .last()
        .expect("validated history is nonempty");
    if context.source_frame_sequence < prior.after_frame_sequence_v1()
        || context.source_frame_sequence <= prior.source_frame_sequence_v1()
        || context.after_frame_sequence <= prior.after_frame_sequence_v1()
    {
        return Err(error_v1(
            "competitive_player_visible_history_order",
            "confirmed combat must form one monotonic visible frame lineage",
        ));
    }
    if history.record.decisions.iter().any(|entry| {
        entry.decision_commitment_sha256_v1() == confirmed.decision_commitment_sha256_v1()
            || entry.visible_postcondition_commitment_sha256_v1()
                == context.visible_postcondition_commitment_sha256
    }) {
        return Err(error_v1(
            "competitive_player_visible_history_duplicate",
            "a combat transaction or visible postcondition cannot be appended twice",
        ));
    }
    let sequence = u64::try_from(history.record.decisions.len() + 1).map_err(|_| {
        error_v1(
            "competitive_player_visible_history_sequence",
            "decision sequence overflow",
        )
    })?;
    history.record.decisions.push(
        MtgoCompetitivePlayerVisibleHistoryEntryRecordV1::CombatDecision(
            combat_decision_record_v1(sequence, context, confirmed)?,
        ),
    );
    finish_v1(history.record)
}

fn combat_decision_record_v1(
    sequence: u64,
    context: MtgoCompetitivePlayerVisibleCombatHistoryContextV1,
    confirmed: CheckedUntrustedMtgoPlayerVisibleConfirmedCombatDecisionV1,
) -> Result<MtgoCompetitivePlayerVisibleCombatDecisionRecordV1, MtgoContractErrorV1> {
    if context.source_frame_id == 0
        || context.after_frame_id == 0
        || context.source_frame_id == context.after_frame_id
        || context.source_frame_sequence == 0
        || context.after_frame_sequence <= context.source_frame_sequence
        || confirmed.source_visible_state_v1().acting_player
            != crate::MtgoPlayerRelativeRoleV1::SeatedPlayer
    {
        return Err(error_v1(
            "competitive_player_visible_combat_history_source",
            "confirmed combat source and after-frame identities are inconsistent",
        ));
    }
    for digest in [
        context.event_identity_sha256.as_str(),
        context.match_identity_sha256.as_str(),
        context.policy_deployment_commitment_sha256.as_str(),
        context.visible_postcondition_commitment_sha256.as_str(),
        confirmed.source_visible_result_sha256_v1(),
        confirmed.final_visible_result_sha256_v1(),
        confirmed.decision_commitment_sha256_v1(),
    ] {
        require_sha256_v1(digest)?;
    }
    if confirmed.deployment_commitment_sha256_v1() != context.policy_deployment_commitment_sha256 {
        return Err(error_v1(
            "competitive_player_visible_combat_history_deployment",
            "confirmed combat belongs to another model deployment",
        ));
    }
    Ok(MtgoCompetitivePlayerVisibleCombatDecisionRecordV1 {
        sequence,
        source_frame_id: context.source_frame_id,
        source_frame_sequence: context.source_frame_sequence,
        source_visible_result_sha256: confirmed.source_visible_result_sha256_v1().to_owned(),
        after_frame_id: context.after_frame_id,
        after_frame_sequence: context.after_frame_sequence,
        decision_commitment_sha256: confirmed.decision_commitment_sha256_v1().to_owned(),
        visible_postcondition_commitment_sha256: context.visible_postcondition_commitment_sha256,
        confirmed_combat_decision: confirmed,
    })
}

/// Checks private deployment and visible-frame continuity before an existing
/// history is replayed into a scorer. The underlying frame sequence remains
/// sealed; callers receive only success or one non-sensitive failure reason.
#[doc(hidden)]
pub fn validate_competitive_player_visible_game_history_for_scoring_v1(
    history: &CheckedUntrustedMtgoCompetitivePlayerVisibleGameHistoryV1,
    deployment_commitment_sha256: &str,
    current_frame_sequence: u64,
) -> Result<(), MtgoContractErrorV1> {
    require_sha256_v1(deployment_commitment_sha256)?;
    if history.record.policy_deployment_commitment_sha256 != deployment_commitment_sha256 {
        return Err(error_v1(
            "competitive_player_visible_history_scoring_deployment",
            "confirmed decision history belongs to another model deployment",
        ));
    }
    if current_frame_sequence <= history.last_after_frame_sequence_v1() {
        return Err(error_v1(
            "competitive_player_visible_history_scoring_frame",
            "current duel perception is not newer than the last confirmed visible action",
        ));
    }
    Ok(())
}

/// Joins the private confirmed-decision ledger to the exact-game actuator
/// session before public history is replayed into a scorer. The frame values
/// are checked but never exposed through the player-visible consumer API.
#[doc(hidden)]
pub fn validate_competitive_player_visible_game_history_for_session_v1(
    history: &CheckedUntrustedMtgoCompetitivePlayerVisibleGameHistoryV1,
    deployment_commitment_sha256: &str,
    expected_confirmed_action_count: u64,
    expected_last_confirmed_frame_sequence: u64,
    current_frame_sequence: u64,
) -> Result<(), MtgoContractErrorV1> {
    validate_competitive_player_visible_game_history_for_scoring_v1(
        history,
        deployment_commitment_sha256,
        current_frame_sequence,
    )?;
    validate_competitive_player_visible_game_history_session_checkpoint_v1(
        history,
        deployment_commitment_sha256,
        expected_confirmed_action_count,
        expected_last_confirmed_frame_sequence,
    )
}

/// Checks the private deployment, count, and last confirmed sequence at an
/// action boundary where no newer scoring frame exists yet. Unlike the
/// scoring validator, this never asks the caller to invent a future frame.
#[doc(hidden)]
pub fn validate_competitive_player_visible_game_history_session_checkpoint_v1(
    history: &CheckedUntrustedMtgoCompetitivePlayerVisibleGameHistoryV1,
    deployment_commitment_sha256: &str,
    expected_confirmed_action_count: u64,
    expected_last_confirmed_frame_sequence: u64,
) -> Result<(), MtgoContractErrorV1> {
    require_sha256_v1(deployment_commitment_sha256)?;
    if history.record.policy_deployment_commitment_sha256 != deployment_commitment_sha256 {
        return Err(error_v1(
            "competitive_player_visible_history_session_deployment",
            "confirmed decision history belongs to another model deployment",
        ));
    }
    if u64::try_from(history.record.decisions.len()).ok() != Some(expected_confirmed_action_count)
        || history.last_after_frame_sequence_v1() != expected_last_confirmed_frame_sequence
    {
        return Err(error_v1(
            "competitive_player_visible_history_session",
            "confirmed decision history does not exactly match the actuator session",
        ));
    }
    Ok(())
}

fn decision_record_v1(
    sequence: u64,
    confirmed: &CheckedUntrustedMtgoCompetitiveGameplayPostconditionV1,
) -> Result<MtgoCompetitivePlayerVisibleDecisionRecordV1, MtgoContractErrorV1> {
    if confirmed.source_frame_id_v1() == 0
        || confirmed.after_frame_id() == 0
        || confirmed.source_frame_id_v1() == confirmed.after_frame_id()
        || confirmed.source_frame_sequence_v1() == 0
        || confirmed.after_frame_sequence() <= confirmed.source_frame_sequence_v1()
        || confirmed
            .player_visible_decision_v1()
            .current_state
            .acting_player
            != player_visible_action_actor_v1(
                &confirmed.player_visible_decision_v1().selected_action,
            )
    {
        return Err(error_v1(
            "competitive_player_visible_history_source",
            "confirmed gameplay source and after-frame identities are inconsistent",
        ));
    }
    for digest in [
        confirmed.source_frame_sha256_v1(),
        confirmed.decision_commitment_sha256_v1(),
        confirmed.selection_commitment_sha256_v1(),
        confirmed.deployment_commitment_sha256_v1(),
        confirmed.confirmation_commitment_sha256(),
    ] {
        require_sha256_v1(digest)?;
    }
    Ok(MtgoCompetitivePlayerVisibleDecisionRecordV1 {
        sequence,
        source_frame_id: confirmed.source_frame_id_v1(),
        source_frame_sequence: confirmed.source_frame_sequence_v1(),
        source_frame_sha256: confirmed.source_frame_sha256_v1().to_owned(),
        after_frame_id: confirmed.after_frame_id(),
        after_frame_sequence: confirmed.after_frame_sequence(),
        player_visible_decision: confirmed.player_visible_decision_v1().clone(),
        decision_commitment_sha256: confirmed.decision_commitment_sha256_v1().to_owned(),
        selection_commitment_sha256: confirmed.selection_commitment_sha256_v1().to_owned(),
        visible_postcondition_commitment_sha256: confirmed
            .confirmation_commitment_sha256()
            .to_owned(),
    })
}

fn player_visible_postcondition_decision_record_v1(
    sequence: u64,
    confirmed: &CheckedUntrustedMtgoPlayerVisibleGameplayPostconditionV1,
) -> Result<MtgoCompetitivePlayerVisibleDecisionRecordV1, MtgoContractErrorV1> {
    if confirmed.source_frame_id_v1() == 0
        || confirmed.after_frame_id_v1() == 0
        || confirmed.source_frame_id_v1() == confirmed.after_frame_id_v1()
        || confirmed.source_frame_sequence_v1() == 0
        || confirmed.after_frame_sequence_v1() <= confirmed.source_frame_sequence_v1()
        || confirmed
            .player_visible_decision_v1()
            .current_state
            .acting_player
            != player_visible_action_actor_v1(
                &confirmed.player_visible_decision_v1().selected_action,
            )
    {
        return Err(error_v1(
            "competitive_player_visible_history_source",
            "confirmed gameplay source and after-frame identities are inconsistent",
        ));
    }
    for digest in [
        confirmed.source_frame_sha256_v1(),
        confirmed.decision_commitment_sha256_v1(),
        confirmed.selection_commitment_sha256_v1(),
        confirmed.deployment_commitment_sha256_v1(),
        confirmed.confirmation_commitment_sha256_v1(),
    ] {
        require_sha256_v1(digest)?;
    }
    Ok(MtgoCompetitivePlayerVisibleDecisionRecordV1 {
        sequence,
        source_frame_id: confirmed.source_frame_id_v1(),
        source_frame_sequence: confirmed.source_frame_sequence_v1(),
        source_frame_sha256: confirmed.source_frame_sha256_v1().to_owned(),
        after_frame_id: confirmed.after_frame_id_v1(),
        after_frame_sequence: confirmed.after_frame_sequence_v1(),
        player_visible_decision: confirmed.player_visible_decision_v1().clone(),
        decision_commitment_sha256: confirmed.decision_commitment_sha256_v1().to_owned(),
        selection_commitment_sha256: confirmed.selection_commitment_sha256_v1().to_owned(),
        visible_postcondition_commitment_sha256: confirmed
            .confirmation_commitment_sha256_v1()
            .to_owned(),
    })
}

fn direct_visible_postcondition_decision_record_v1(
    sequence: u64,
    confirmed: &CheckedUntrustedMtgoDirectVisibleGameplayPostconditionV1,
) -> Result<MtgoCompetitivePlayerVisibleDecisionRecordV1, MtgoContractErrorV1> {
    if confirmed.source_frame_id_v1() == 0
        || confirmed.after_frame_id_v1() == 0
        || confirmed.source_frame_id_v1() == confirmed.after_frame_id_v1()
        || confirmed.source_frame_sequence_v1() == 0
        || confirmed.after_frame_sequence_v1() <= confirmed.source_frame_sequence_v1()
        || confirmed
            .player_visible_decision_v1()
            .current_state
            .acting_player
            != player_visible_action_actor_v1(
                &confirmed.player_visible_decision_v1().selected_action,
            )
    {
        return Err(error_v1(
            "competitive_player_visible_history_source",
            "confirmed gameplay source and after-frame identities are inconsistent",
        ));
    }
    for digest in [
        confirmed.source_frame_sha256_v1(),
        confirmed.decision_commitment_sha256_v1(),
        confirmed.selection_commitment_sha256_v1(),
        confirmed.deployment_commitment_sha256_v1(),
        confirmed.confirmation_commitment_sha256_v1(),
    ] {
        require_sha256_v1(digest)?;
    }
    Ok(MtgoCompetitivePlayerVisibleDecisionRecordV1 {
        sequence,
        source_frame_id: confirmed.source_frame_id_v1(),
        source_frame_sequence: confirmed.source_frame_sequence_v1(),
        source_frame_sha256: confirmed.source_frame_sha256_v1().to_owned(),
        after_frame_id: confirmed.after_frame_id_v1(),
        after_frame_sequence: confirmed.after_frame_sequence_v1(),
        player_visible_decision: confirmed.player_visible_decision_v1().clone(),
        decision_commitment_sha256: confirmed.decision_commitment_sha256_v1().to_owned(),
        selection_commitment_sha256: confirmed.selection_commitment_sha256_v1().to_owned(),
        visible_postcondition_commitment_sha256: confirmed
            .confirmation_commitment_sha256_v1()
            .to_owned(),
    })
}

fn finish_v1(
    record: MtgoCompetitivePlayerVisibleGameHistoryRecordV1,
) -> Result<CheckedUntrustedMtgoCompetitivePlayerVisibleGameHistoryV1, MtgoContractErrorV1> {
    if record.decisions.is_empty()
        || record.decisions.len() > MAX_PLAYER_VISIBLE_GAME_DECISIONS_V1
        || !(1..=3).contains(&record.game_number)
    {
        return Err(error_v1(
            "competitive_player_visible_history_shape",
            "game number or decision count is invalid",
        ));
    }
    for digest in [
        record.event_identity_sha256.as_str(),
        record.match_identity_sha256.as_str(),
        record.policy_deployment_commitment_sha256.as_str(),
    ] {
        require_sha256_v1(digest)?;
    }
    let mut decision_ids = HashSet::new();
    let mut selection_ids = HashSet::new();
    let mut postconditions = HashSet::new();
    for (index, decision) in record.decisions.iter().enumerate() {
        if usize::try_from(decision.sequence_v1()).ok() != Some(index + 1)
            || !decision_ids.insert(decision.decision_commitment_sha256_v1())
            || !selection_ids.insert(decision.selection_commitment_sha256_v1())
            || !postconditions.insert(decision.visible_postcondition_commitment_sha256_v1())
        {
            return Err(error_v1(
                "competitive_player_visible_history_decision_order",
                "decision sequence and commitments must be canonical and unique",
            ));
        }
    }
    let encoded = serde_json::to_vec(&record).map_err(|error| {
        error_v1(
            "competitive_player_visible_history_serialization",
            error.to_string(),
        )
    })?;
    let mut hasher = Sha256::new();
    hasher.update(PLAYER_VISIBLE_GAME_HISTORY_DOMAIN_V1);
    hasher.update((encoded.len() as u64).to_be_bytes());
    hasher.update(encoded);
    Ok(CheckedUntrustedMtgoCompetitivePlayerVisibleGameHistoryV1 {
        record,
        history_commitment_sha256: format!("{:x}", hasher.finalize()),
    })
}

fn player_visible_action_actor_v1(
    action: &MtgoPlayerVisibleDuelActionV1,
) -> crate::MtgoPlayerRelativeRoleV1 {
    match action {
        MtgoPlayerVisibleDuelActionV1::Pass { actor }
        | MtgoPlayerVisibleDuelActionV1::PlayLand { actor, .. }
        | MtgoPlayerVisibleDuelActionV1::CastSpell { actor, .. }
        | MtgoPlayerVisibleDuelActionV1::ActivateManaAbility { actor, .. }
        | MtgoPlayerVisibleDuelActionV1::ActivateAbility { actor, .. }
        | MtgoPlayerVisibleDuelActionV1::PlotSpell { actor, .. }
        | MtgoPlayerVisibleDuelActionV1::ChooseTarget { actor, .. }
        | MtgoPlayerVisibleDuelActionV1::ChooseCostTarget { actor, .. }
        | MtgoPlayerVisibleDuelActionV1::ChooseCastMode { actor, .. }
        | MtgoPlayerVisibleDuelActionV1::ChooseKicker { actor, .. }
        | MtgoPlayerVisibleDuelActionV1::ChooseSpellMode { actor, .. }
        | MtgoPlayerVisibleDuelActionV1::ChooseEffectOption { actor, .. }
        | MtgoPlayerVisibleDuelActionV1::ChooseEffectTarget { actor, .. }
        | MtgoPlayerVisibleDuelActionV1::FinishEffectSelection { actor, .. }
        | MtgoPlayerVisibleDuelActionV1::ChooseEffectColor { actor, .. }
        | MtgoPlayerVisibleDuelActionV1::ChooseEffectNumber { actor, .. }
        | MtgoPlayerVisibleDuelActionV1::ChooseEffectBoolean { actor, .. }
        | MtgoPlayerVisibleDuelActionV1::FinishTargetSelection { actor, .. }
        | MtgoPlayerVisibleDuelActionV1::ChooseOptionalCostUse { actor, .. }
        | MtgoPlayerVisibleDuelActionV1::ChooseOptionalCostWhich { actor, .. }
        | MtgoPlayerVisibleDuelActionV1::ChooseSpellCopyPayment { actor, .. }
        | MtgoPlayerVisibleDuelActionV1::ChooseSpellCopyRetarget { actor, .. }
        | MtgoPlayerVisibleDuelActionV1::ChooseMadnessCast { actor, .. }
        | MtgoPlayerVisibleDuelActionV1::Discard { actor, .. }
        | MtgoPlayerVisibleDuelActionV1::DeclareAttackers { actor, .. }
        | MtgoPlayerVisibleDuelActionV1::DeclareBlockersForAttacker { actor, .. }
        | MtgoPlayerVisibleDuelActionV1::ChooseAttackerInclusion { actor, .. }
        | MtgoPlayerVisibleDuelActionV1::ChooseBlockerInclusion { actor, .. }
        | MtgoPlayerVisibleDuelActionV1::OrderTriggers { actor, .. } => *actor,
    }
}

fn validate_identifier_v1(value: &str) -> Result<(), MtgoContractErrorV1> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
    {
        return Err(error_v1("competitive_player_visible_history_id", value));
    }
    Ok(())
}

fn require_sha256_v1(value: &str) -> Result<(), MtgoContractErrorV1> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
    {
        return Err(error_v1("competitive_player_visible_history_digest", value));
    }
    Ok(())
}

fn error_v1(code: &'static str, detail: impl Into<String>) -> MtgoContractErrorV1 {
    MtgoContractErrorV1::new(code, detail)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::competitive_gameplay_scope::{
        competitive_gameplay_postcondition_for_match_memory_test_v1,
        later_competitive_gameplay_postcondition_for_match_memory_test_v1,
    };

    #[test]
    fn confirmed_player_visible_decision_is_retained_without_authority() {
        let history = begin_checked_untrusted_competitive_player_visible_game_history_v1(
            "league-match-game-one-visible-history-v1",
            competitive_gameplay_postcondition_for_match_memory_test_v1(),
        )
        .unwrap();
        assert_eq!(history.event_kind_v1(), MtgoCompetitiveEventKindV1::League);
        assert_eq!(history.game_number_v1(), 1);
        assert_eq!(history.decision_count_v1(), 1);
        assert_eq!(history.history_commitment_sha256_v1().len(), 64);
        let decision = history.decision_v1(0).unwrap();
        assert_eq!(decision.sequence_v1(), 1);
        let visible_input = decision.player_visible_decision_v1();
        assert!(matches!(
            visible_input.selected_action,
            crate::MtgoPlayerVisibleDuelActionV1::PlayLand { .. }
        ));
        assert_eq!(
            visible_input.current_state.acting_player,
            crate::MtgoPlayerRelativeRoleV1::SeatedPlayer
        );
        let json = serde_json::to_value(visible_input).unwrap();
        fn reject_forbidden_keys(value: &serde_json::Value) {
            match value {
                serde_json::Value::Object(fields) => {
                    for (key, child) in fields {
                        assert!(
                            !matches!(
                                key.as_str(),
                                "arena_id" | "card_db_id" | "zone_change_count" | "engine_context"
                            ),
                            "forbidden history field: {key}"
                        );
                        reject_forbidden_keys(child);
                    }
                }
                serde_json::Value::Array(values) => {
                    for child in values {
                        reject_forbidden_keys(child);
                    }
                }
                _ => {}
            }
        }
        reject_forbidden_keys(&json);
        assert!(decision.record.after_frame_sequence > decision.record.source_frame_sequence);
        assert!(!history.safe_for_model_scoring_v1());
        assert!(!history.safe_for_input_v1());
        assert!(!history.permits_event_entry_v1());
        assert!(!history.permits_spending_v1());
    }

    #[test]
    fn duplicate_confirmed_decision_cannot_extend_the_history() {
        let first = competitive_gameplay_postcondition_for_match_memory_test_v1();
        let history = begin_checked_untrusted_competitive_player_visible_game_history_v1(
            "league-match-game-one-visible-history-v1",
            first,
        )
        .unwrap();
        let error = append_checked_untrusted_competitive_player_visible_game_history_v1(
            history,
            competitive_gameplay_postcondition_for_match_memory_test_v1(),
        )
        .err()
        .unwrap();
        assert!(matches!(
            error.code(),
            "competitive_player_visible_history_order"
                | "competitive_player_visible_history_duplicate"
        ));
    }

    #[test]
    fn later_confirmed_decision_extends_the_exact_game_lineage() {
        let history = begin_checked_untrusted_competitive_player_visible_game_history_v1(
            "league-match-game-one-visible-history-v1",
            competitive_gameplay_postcondition_for_match_memory_test_v1(),
        )
        .unwrap();
        let prior_commitment = history.history_commitment_sha256_v1().to_owned();
        let history = append_checked_untrusted_competitive_player_visible_game_history_v1(
            history,
            later_competitive_gameplay_postcondition_for_match_memory_test_v1(),
        )
        .unwrap();
        assert_eq!(history.decision_count_v1(), 2);
        assert_ne!(history.history_commitment_sha256_v1(), prior_commitment);
        assert_eq!(history.decision_v1(1).unwrap().sequence_v1(), 2);
    }

    #[test]
    fn scoring_history_requires_exact_deployment_and_newer_current_frame() {
        let confirmed = competitive_gameplay_postcondition_for_match_memory_test_v1();
        let deployment = confirmed.deployment_commitment_sha256_v1().to_owned();
        let after_frame_sequence = confirmed.after_frame_sequence();
        let history = begin_checked_untrusted_competitive_player_visible_game_history_v1(
            "league-match-game-one-visible-history-v1",
            confirmed,
        )
        .unwrap();

        validate_competitive_player_visible_game_history_for_scoring_v1(
            &history,
            &deployment,
            after_frame_sequence + 1,
        )
        .unwrap();
        assert_eq!(
            validate_competitive_player_visible_game_history_for_scoring_v1(
                &history,
                &"f".repeat(64),
                after_frame_sequence + 1,
            )
            .unwrap_err()
            .code(),
            "competitive_player_visible_history_scoring_deployment"
        );
        assert_eq!(
            validate_competitive_player_visible_game_history_for_scoring_v1(
                &history,
                &deployment,
                after_frame_sequence,
            )
            .unwrap_err()
            .code(),
            "competitive_player_visible_history_scoring_frame"
        );
    }

    #[test]
    fn actuator_session_requires_exact_confirmed_count_and_terminal_frame() {
        let confirmed = competitive_gameplay_postcondition_for_match_memory_test_v1();
        let deployment = confirmed.deployment_commitment_sha256_v1().to_owned();
        let after_frame_sequence = confirmed.after_frame_sequence();
        let history = begin_checked_untrusted_competitive_player_visible_game_history_v1(
            "league-match-game-one-visible-history-v1",
            confirmed,
        )
        .unwrap();

        validate_competitive_player_visible_game_history_for_session_v1(
            &history,
            &deployment,
            1,
            after_frame_sequence,
            after_frame_sequence + 1,
        )
        .unwrap();
        validate_competitive_player_visible_game_history_session_checkpoint_v1(
            &history,
            &deployment,
            1,
            after_frame_sequence,
        )
        .unwrap();
        for (count, terminal) in [(0, after_frame_sequence), (1, after_frame_sequence + 1)] {
            assert_eq!(
                validate_competitive_player_visible_game_history_for_session_v1(
                    &history,
                    &deployment,
                    count,
                    terminal,
                    after_frame_sequence + 2,
                )
                .unwrap_err()
                .code(),
                "competitive_player_visible_history_session"
            );
            assert_eq!(
                validate_competitive_player_visible_game_history_session_checkpoint_v1(
                    &history,
                    &deployment,
                    count,
                    terminal,
                )
                .unwrap_err()
                .code(),
                "competitive_player_visible_history_session"
            );
        }
    }
}
