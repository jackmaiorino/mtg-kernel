use crate::{
    OpaqueMtgoCompetitiveMatchVisibleGameLogSnapshotV1,
    OpaqueMtgoCompetitiveVisibleGameLogSemanticsV1,
};
use mtgo_blackbox_v1::{
    CheckedUntrustedMtgoCompetitivePlayerVisibleGameHistoryV1, MtgoCompetitiveEventKindV1,
    MtgoCompetitivePlayerVisibleDecisionViewV1, MtgoVisibleGameLogSemanticEventViewV1,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const COMBINED_COMPETITIVE_VISIBLE_GAME_MEMORY_DOMAIN_V1: &[u8] =
    b"mtgo-combined-competitive-player-visible-game-memory-v1";

pub const MTGO_COMPETITIVE_EXTERNAL_PUBLIC_HISTORY_SCHEMA_V1: u32 = 1;

/// Ordering contract for the kernel-facing public-history seam.
///
/// MTGO Game Log sequence numbers and admitted capture-frame sequence numbers
/// are independent clocks. V1 therefore preserves exact order within each
/// source and makes no claim about the relative order of an event in one
/// stream and a decision in the other.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MtgoCompetitiveExternalPublicHistoryOrderingV1 {
    SeparateOrderedStreamsNoCrossSourceTotalOrder,
}

/// Player-visible lineage and bounds supplied before either history stream.
///
/// This header contains commitments and counts only. It has no path, source
/// UUID, raw markup, pixel, coordinate, process, input, entry, or spending
/// capability. `game_log_is_complete_current_state` is permanently false:
/// public log events supplement, but cannot replace, the exact current visible
/// observation reconstructed from the admitted client frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MtgoCompetitiveExternalPublicHistoryHeaderV1<'a> {
    pub schema_version: u32,
    pub event_kind: MtgoCompetitiveEventKindV1,
    pub event_identity_sha256: &'a str,
    pub match_identity_sha256: &'a str,
    pub game_number: u8,
    pub policy_deployment_commitment_sha256: &'a str,
    pub source_memory_commitment_sha256: &'a str,
    pub confirmed_decision_count: usize,
    pub public_event_count: usize,
    pub ordering: MtgoCompetitiveExternalPublicHistoryOrderingV1,
    pub player_visible_information_only: bool,
    pub game_log_is_complete_current_state: bool,
}

/// Kernel-owned consumer boundary for one exact game's player-visible public
/// history. Implementations receive two separate ordered streams. The adapter
/// deliberately provides no callback that claims a cross-source ordering.
///
/// A consumer result is ordinary data, not adapter authority. Implementing
/// this trait cannot send MTGO input, enter an event, or spend resources.
pub trait MtgoCompetitiveExternalPublicHistoryConsumerV1 {
    type Output;

    fn begin_public_history_v1(
        &mut self,
        header: MtgoCompetitiveExternalPublicHistoryHeaderV1<'_>,
    ) -> Result<(), String>;

    fn consume_confirmed_decision_v1(
        &mut self,
        decision: MtgoCompetitivePlayerVisibleDecisionViewV1<'_>,
    ) -> Result<(), String>;

    fn finish_confirmed_decision_stream_v1(&mut self) -> Result<(), String>;

    fn consume_public_game_log_event_v1(
        &mut self,
        event: MtgoVisibleGameLogSemanticEventViewV1<'_>,
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
            schema_version: MTGO_COMPETITIVE_EXTERNAL_PUBLIC_HISTORY_SCHEMA_V1,
            event_kind: lineage.event_kind,
            event_identity_sha256: lineage.event_identity_sha256,
            match_identity_sha256: lineage.match_identity_sha256,
            game_number: lineage.game_number,
            policy_deployment_commitment_sha256: self
                .confirmed_decisions
                .policy_deployment_commitment_sha256_v1(),
            source_memory_commitment_sha256: &self.memory_commitment_sha256,
            confirmed_decision_count: self.confirmed_decision_count_v1(),
            public_event_count: self.public_event_count_v1(),
            ordering: MtgoCompetitiveExternalPublicHistoryOrderingV1::SeparateOrderedStreamsNoCrossSourceTotalOrder,
            player_visible_information_only: true,
            game_log_is_complete_current_state: false,
        })?;

        for index in 0..self.confirmed_decision_count_v1() {
            let decision = self.confirmed_decision_v1(index).ok_or_else(|| {
                "confirmed decision stream changed while it was being visited".to_owned()
            })?;
            consumer.consume_confirmed_decision_v1(decision)?;
        }
        consumer.finish_confirmed_decision_stream_v1()?;

        for index in 0..self.public_event_count_v1() {
            let event = self.public_event_v1(index).ok_or_else(|| {
                "public Game Log stream changed while it was being visited".to_owned()
            })?;
            consumer.consume_public_game_log_event_v1(event)?;
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

fn commitment_v1(parts: &[&[u8]]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(
        u64::try_from(COMBINED_COMPETITIVE_VISIBLE_GAME_MEMORY_DOMAIN_V1.len())
            .expect("static domain length fits u64")
            .to_be_bytes(),
    );
    hasher.update(COMBINED_COMPETITIVE_VISIBLE_GAME_MEMORY_DOMAIN_V1);
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
