use crate::OpaqueMtgoCompetitiveVisibleGameLogSemanticsV1;
use mtgo_blackbox_v1::{
    CheckedUntrustedMtgoCompetitivePlayerVisibleGameHistoryV1,
    MtgoCompetitivePlayerVisibleDecisionViewV1, MtgoVisibleGameLogSemanticEventViewV1,
};
use sha2::{Digest, Sha256};

const COMBINED_COMPETITIVE_VISIBLE_GAME_MEMORY_DOMAIN_V1: &[u8] =
    b"mtgo-combined-competitive-player-visible-game-memory-v1";

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
    game_log: OpaqueMtgoCompetitiveVisibleGameLogSemanticsV1,
    confirmed_decisions: CheckedUntrustedMtgoCompetitivePlayerVisibleGameHistoryV1,
    memory_commitment_sha256: String,
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
        self.game_log.match_identity_sha256_v1()
    }

    pub fn game_number_v1(&self) -> u8 {
        self.game_log.game_number_v1()
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
}

/// Requires both retained histories to describe the exact same event, match,
/// and numbered game. This does not guess a total ordering between MTGO log
/// sequence numbers and capture frame sequence numbers. The future kernel
/// importer must define that mapping explicitly and fixture-test it.
pub fn bind_competitive_player_visible_game_memory_v1(
    game_log: OpaqueMtgoCompetitiveVisibleGameLogSemanticsV1,
    confirmed_decisions: CheckedUntrustedMtgoCompetitivePlayerVisibleGameHistoryV1,
) -> Result<OpaqueMtgoCompetitivePlayerVisibleGameMemoryV1, String> {
    validate_combined_visible_game_lineage_v1(
        VisibleGameLineageRefV1 {
            event_kind: game_log.event_kind_v1(),
            event_identity_sha256: game_log.event_identity_sha256_v1(),
            match_identity_sha256: game_log.match_identity_sha256_v1(),
            game_number: game_log.game_number_v1(),
        },
        VisibleGameLineageRefV1 {
            event_kind: confirmed_decisions.event_kind_v1(),
            event_identity_sha256: confirmed_decisions.event_identity_sha256_v1(),
            match_identity_sha256: confirmed_decisions.match_identity_sha256_v1(),
            game_number: confirmed_decisions.game_number_v1(),
        },
    )?;
    let memory_commitment_sha256 = commitment_v1(&[
        game_log.binding_commitment_sha256_v1().as_bytes(),
        game_log
            .semantic_projection_commitment_sha256_v1()
            .as_bytes(),
        confirmed_decisions
            .history_commitment_sha256_v1()
            .as_bytes(),
        confirmed_decisions
            .policy_deployment_commitment_sha256_v1()
            .as_bytes(),
        game_log.event_identity_sha256_v1().as_bytes(),
        game_log.match_identity_sha256_v1().as_bytes(),
        &[game_log.game_number_v1()],
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
}
