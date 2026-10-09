//! Fresh native collection receipts. Frozen trajectory V1/V2 bytes are not reused.
use crate::async_rollout::AsyncRolloutTerminalV1;
use crate::native_full_episode_trajectory_v1::{
    NativeFullEpisodeTrajectoryAccumulatorV1, NativeFullEpisodeTrajectoryDecisionRowV1,
    NativeFullEpisodeTrajectoryErrorV1, NativeFullEpisodeTrajectoryReceiptV1,
    NativeTrajectoryActorRoleV1,
};
use crate::native_full_episode_trajectory_v2::{
    NativeRunBoundFullEpisodeAccumulatorV2, NativeTrainingTrajectoryReceiptV2,
};
use crate::rl::PlayerSeatV1;
use crate::rl_session::{FastActorDecisionV1, SessionDeckHashesV1, SessionDeckIdsV1};
use sha2::{Digest, Sha256};

pub const NATIVE_SEARCH_TRAJECTORY_IDENTITY_V3: &str = "mtg-kernel-native-search-trajectory/v3";
pub const NATIVE_SEARCH_ACTION_CONTRACT_V3: &str = "flat-action-v4-visible-menu/v3";
pub const NATIVE_SEARCH_REJECTION_RULE_V3: &str =
    "hidden-reference-conflict-canonical-visible-first-fixed-be-fields-no-retry/v1";

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct NativeSearchTrajectoryReceiptV3 {
    metadata: NativeFullEpisodeTrajectoryReceiptV1,
    digest: [u8; 32],
    search_authority_digest: [u8; 32],
    rejected_determinizations: u64,
}

impl std::fmt::Debug for NativeSearchTrajectoryReceiptV3 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NativeSearchTrajectoryReceiptV3")
            .field("identity", &self.identity())
            .field("episode_index", &self.episode_index())
            .field("trajectory_sha256", &self.digest)
            .field("policy_step_count", &self.policy_step_count())
            .field("rejected_determinizations", &self.rejected_determinizations)
            .finish_non_exhaustive()
    }
}

impl NativeSearchTrajectoryReceiptV3 {
    pub fn identity(&self) -> &'static str {
        NATIVE_SEARCH_TRAJECTORY_IDENTITY_V3
    }
    pub fn action_contract(&self) -> &'static str {
        NATIVE_SEARCH_ACTION_CONTRACT_V3
    }
    pub fn episode_index(&self) -> u64 {
        self.metadata.episode_index
    }
    pub fn trajectory_sha256(&self) -> [u8; 32] {
        self.digest
    }
    pub fn search_authority_sha256(&self) -> [u8; 32] {
        self.search_authority_digest
    }
    pub fn policy_step_count(&self) -> u64 {
        self.metadata.policy_step_count
    }
    pub fn physical_decision_count(&self) -> u64 {
        self.metadata.physical_decision_count
    }
    pub fn learner_policy_step_count(&self) -> u64 {
        self.metadata.learner_policy_step_count
    }
    pub fn opponent_policy_step_count(&self) -> u64 {
        self.metadata.opponent_policy_step_count
    }
    pub fn rejected_determinizations(&self) -> u64 {
        self.rejected_determinizations
    }
}

#[derive(Clone, Copy)]
pub(crate) struct NativeSearchTrajectoryRowV3 {
    decision: FastActorDecisionV1,
    actor_role: NativeTrajectoryActorRoleV1,
    actor_physical_ordinal: u64,
    action_seed: u64,
    selected_index: u32,
    visible_menu_commitment: [u8; 16],
    rejected_determinization: bool,
}

impl NativeSearchTrajectoryRowV3 {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        decision: FastActorDecisionV1,
        actor_role: NativeTrajectoryActorRoleV1,
        actor_physical_ordinal: u64,
        action_seed: u64,
        selected_index: u32,
        visible_menu_commitment: [u8; 16],
        rejected_determinization: bool,
    ) -> Self {
        Self {
            decision,
            actor_role,
            actor_physical_ordinal,
            action_seed,
            selected_index,
            visible_menu_commitment,
            rejected_determinization,
        }
    }

    // Reuse only the existing count/group/provenance validator. Its dummy
    // commitment and digest never enter a fresh receipt or any persisted stream.
    fn metadata(self) -> NativeFullEpisodeTrajectoryDecisionRowV1 {
        NativeFullEpisodeTrajectoryDecisionRowV1 {
            row_ordinal: self.decision.step,
            actor_seat: self.decision.acting_player,
            actor_role: self.actor_role,
            physical_decision_ordinal: self.decision.physical_decision_id,
            actor_physical_decision_ordinal: self.actor_physical_ordinal,
            substep_index: self.decision.substep_index,
            substep_count: self.decision.substep_count,
            action_seed: self.action_seed,
            legal_action_count: self.decision.legal_action_count,
            selected_index: self.selected_index,
            flat_action_v2_commitment: [0; 16],
        }
    }

    fn bytes(self) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(NATIVE_SEARCH_ACTION_CONTRACT_V3.as_bytes());
        for value in [
            self.decision.episode_id,
            self.decision.step,
            self.decision.physical_decision_id,
            self.actor_physical_ordinal,
            self.action_seed,
        ] {
            bytes.extend_from_slice(&value.to_be_bytes());
        }
        bytes.push(match self.decision.acting_player {
            PlayerSeatV1::P0 => 0,
            PlayerSeatV1::P1 => 1,
        });
        bytes.push(match self.actor_role {
            NativeTrajectoryActorRoleV1::Learner => 0,
            NativeTrajectoryActorRoleV1::Opponent => 1,
        });
        for value in [
            self.decision.substep_index,
            self.decision.substep_count,
            self.decision.legal_action_count,
            self.selected_index,
        ] {
            bytes.extend_from_slice(&value.to_be_bytes());
        }
        bytes.extend_from_slice(&self.visible_menu_commitment);
        bytes.push(u8::from(self.rejected_determinization));
        bytes
    }
}

pub(crate) struct NativeSearchTrajectoryAccumulatorV3 {
    episode_index: u64,
    metadata: NativeFullEpisodeTrajectoryAccumulatorV1,
    digest: Sha256,
    search_authority_digest: [u8; 32],
    rejected_determinizations: u64,
}

fn frame(digest: &mut Sha256, tag: &[u8], bytes: &[u8]) {
    digest.update((tag.len() as u32).to_be_bytes());
    digest.update(tag);
    digest.update((bytes.len() as u64).to_be_bytes());
    digest.update(bytes);
}

impl NativeSearchTrajectoryAccumulatorV3 {
    pub(crate) fn new(
        episode: u64,
        seed: u64,
        decks: &SessionDeckIdsV1,
        hashes: SessionDeckHashesV1,
        learner: PlayerSeatV1,
        search_authority_digest: [u8; 32],
    ) -> Result<Self, NativeFullEpisodeTrajectoryErrorV1> {
        let metadata = NativeFullEpisodeTrajectoryAccumulatorV1::new_v1(
            episode, seed, decks, hashes, learner,
        )?;
        let mut digest = Sha256::new();
        frame(
            &mut digest,
            b"identity",
            NATIVE_SEARCH_TRAJECTORY_IDENTITY_V3.as_bytes(),
        );
        frame(&mut digest, b"episode", &episode.to_be_bytes());
        frame(&mut digest, b"seed", &seed.to_be_bytes());
        frame(&mut digest, b"deck0", decks[0].as_bytes());
        frame(&mut digest, b"deck1", decks[1].as_bytes());
        frame(&mut digest, b"hash0", &hashes[0].to_be_bytes());
        frame(&mut digest, b"hash1", &hashes[1].to_be_bytes());
        frame(
            &mut digest,
            b"learner",
            &[match learner {
                PlayerSeatV1::P0 => 0,
                PlayerSeatV1::P1 => 1,
            }],
        );
        frame(&mut digest, b"search_authority", &search_authority_digest);
        frame(
            &mut digest,
            b"rejection_rule",
            NATIVE_SEARCH_REJECTION_RULE_V3.as_bytes(),
        );
        Ok(Self {
            episode_index: episode,
            metadata,
            digest,
            search_authority_digest,
            rejected_determinizations: 0,
        })
    }

    fn preflight(
        &self,
        row: NativeSearchTrajectoryRowV3,
    ) -> Result<(), NativeFullEpisodeTrajectoryErrorV1> {
        if row.decision.episode_id != self.episode_index {
            return Err(NativeFullEpisodeTrajectoryErrorV1::EpisodeMismatch);
        }
        if row.rejected_determinization && row.actor_role != NativeTrajectoryActorRoleV1::Opponent {
            return Err(NativeFullEpisodeTrajectoryErrorV1::ActorRoleMismatch);
        }
        self.metadata.preflight_candidate_v1(row.metadata())
    }
    fn record(
        &mut self,
        row: NativeSearchTrajectoryRowV3,
    ) -> Result<(), NativeFullEpisodeTrajectoryErrorV1> {
        self.preflight(row)?;
        self.metadata.record_accepted_v1(row.metadata())?;
        self.rejected_determinizations += u64::from(row.rejected_determinization);
        frame(&mut self.digest, b"decision", &row.bytes());
        Ok(())
    }
    fn finish(
        mut self,
        terminal: AsyncRolloutTerminalV1,
        hashes: SessionDeckHashesV1,
    ) -> Result<NativeSearchTrajectoryReceiptV3, NativeFullEpisodeTrajectoryErrorV1> {
        let mut metadata = self.metadata.finish_natural_v1(terminal, hashes)?;
        for (tag, value) in [
            (b"policy_steps".as_slice(), metadata.policy_step_count),
            (
                b"physical_decisions".as_slice(),
                metadata.physical_decision_count,
            ),
            (
                b"learner_steps".as_slice(),
                metadata.learner_policy_step_count,
            ),
            (
                b"opponent_steps".as_slice(),
                metadata.opponent_policy_step_count,
            ),
        ] {
            frame(&mut self.digest, tag, &value.to_be_bytes());
        }
        let mut terminal_bytes = Vec::new();
        terminal_bytes.extend_from_slice(&terminal.episode_id.to_be_bytes());
        terminal_bytes.push(match terminal.terminal_outcome {
            crate::rl::TerminalOutcomeV1::P0Win => 0,
            crate::rl::TerminalOutcomeV1::P1Win => 1,
            crate::rl::TerminalOutcomeV1::Draw => 2,
            crate::rl::TerminalOutcomeV1::Truncated => 3,
            crate::rl::TerminalOutcomeV1::Halted => 4,
        });
        terminal_bytes.push(match terminal.terminal_classification {
            crate::rl::TerminalClassificationV1::Natural => 0,
            crate::rl::TerminalClassificationV1::Truncated => 1,
            crate::rl::TerminalClassificationV1::Halted => 2,
        });
        terminal_bytes.push(match terminal.terminal_code {
            crate::rl::TerminalSafeCodeV2::NaturalGameOver => 0,
            crate::rl::TerminalSafeCodeV2::DecisionCap => 1,
            crate::rl::TerminalSafeCodeV2::FailClosed => 2,
        });
        terminal_bytes.push(match terminal.winner {
            None => 0,
            Some(PlayerSeatV1::P0) => 1,
            Some(PlayerSeatV1::P1) => 2,
        });
        for reward in terminal.terminal_reward {
            terminal_bytes.extend_from_slice(&reward.to_be_bytes());
        }
        terminal_bytes.extend_from_slice(&terminal.policy_step_count.to_be_bytes());
        terminal_bytes.extend_from_slice(&terminal.physical_decision_count.to_be_bytes());
        frame(&mut self.digest, b"terminal", &terminal_bytes);
        // The metadata-only V1 digest is discarded, never exposed as provenance.
        metadata.trajectory_sha256 = [0; 32];
        Ok(NativeSearchTrajectoryReceiptV3 {
            metadata,
            digest: self.digest.finalize().into(),
            search_authority_digest: self.search_authority_digest,
            rejected_determinizations: self.rejected_determinizations,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rl::{TerminalClassificationV1, TerminalOutcomeV1, TerminalSafeCodeV2};
    use crate::rl_session::FastActorDecisionKindV1;
    fn row() -> NativeSearchTrajectoryRowV3 {
        NativeSearchTrajectoryRowV3::new(
            FastActorDecisionV1 {
                episode_id: 23,
                step: 0,
                environment_revision: 0,
                physical_decision_id: 0,
                substep_index: 0,
                substep_count: 1,
                acting_player: PlayerSeatV1::P1,
                decision_kind: FastActorDecisionKindV1::Surface,
                legal_action_count: 2,
            },
            NativeTrajectoryActorRoleV1::Opponent,
            0,
            71,
            0,
            [0x5a; 16],
            false,
        )
    }
    fn accumulator(authority: [u8; 32]) -> NativeSearchTrajectoryAccumulatorV3 {
        NativeSearchTrajectoryAccumulatorV3::new(
            23,
            91,
            &["Burn".to_string(), "Burn".to_string()],
            [17, 19],
            PlayerSeatV1::P0,
            authority,
        )
        .unwrap()
    }
    fn finish(
        row: NativeSearchTrajectoryRowV3,
        authority: [u8; 32],
    ) -> NativeSearchTrajectoryReceiptV3 {
        let mut accumulator = accumulator(authority);
        accumulator.record(row).unwrap();
        accumulator
            .finish(
                AsyncRolloutTerminalV1 {
                    episode_id: 23,
                    terminal_outcome: TerminalOutcomeV1::P1Win,
                    terminal_classification: TerminalClassificationV1::Natural,
                    terminal_code: TerminalSafeCodeV2::NaturalGameOver,
                    winner: Some(PlayerSeatV1::P1),
                    terminal_reward: [-1, 1],
                    policy_step_count: 1,
                    physical_decision_count: 1,
                },
                [17, 19],
            )
            .unwrap()
    }
    #[test]
    fn fresh_receipt_binds_menu_search_identity_and_lawful_rejection() {
        let row = row();
        let first = finish(row, [7; 32]);
        assert_eq!(first, finish(row, [7; 32]));
        let mut changed = row;
        changed.visible_menu_commitment[0] ^= 1;
        assert_ne!(
            first.trajectory_sha256(),
            finish(changed, [7; 32]).trajectory_sha256()
        );
        assert_ne!(
            first.trajectory_sha256(),
            finish(row, [8; 32]).trajectory_sha256()
        );
        let mut rejected = row;
        rejected.rejected_determinization = true;
        let receipt = finish(rejected, [7; 32]);
        assert_eq!(receipt.rejected_determinizations(), 1);
        assert_ne!(first.trajectory_sha256(), receipt.trajectory_sha256());
        assert_eq!(receipt.opponent_policy_step_count(), 1);
    }
    #[test]
    fn fresh_receipt_rejects_foreign_episode_bad_index_and_group() {
        let accumulator = accumulator([7; 32]);
        let mut foreign = row();
        foreign.decision.episode_id += 1;
        assert_eq!(
            accumulator.preflight(foreign),
            Err(NativeFullEpisodeTrajectoryErrorV1::EpisodeMismatch)
        );
        let mut invalid = row();
        invalid.selected_index = 2;
        assert_eq!(
            accumulator.preflight(invalid),
            Err(NativeFullEpisodeTrajectoryErrorV1::SelectedIndexOutOfRange)
        );
        let mut invalid = row();
        invalid.decision.substep_count = 0;
        assert_eq!(
            accumulator.preflight(invalid),
            Err(NativeFullEpisodeTrajectoryErrorV1::MalformedPhysicalGroup)
        );
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NativeLaneTrajectoryReceiptV3 {
    Legacy(NativeTrainingTrajectoryReceiptV2),
    Search(NativeSearchTrajectoryReceiptV3),
}

#[derive(Clone, Copy)]
pub(crate) enum NativeLaneTrajectoryRowV3 {
    Legacy(NativeFullEpisodeTrajectoryDecisionRowV1),
    Search(NativeSearchTrajectoryRowV3),
}

pub(crate) enum NativeLaneTrajectoryAccumulatorV3 {
    Legacy(NativeRunBoundFullEpisodeAccumulatorV2),
    Search(NativeSearchTrajectoryAccumulatorV3),
}

impl NativeLaneTrajectoryRowV3 {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        fresh: bool,
        decision: FastActorDecisionV1,
        role: NativeTrajectoryActorRoleV1,
        ordinal: u64,
        seed: u64,
        selected: u32,
        commitment: [u8; 16],
        rejected: bool,
    ) -> Self {
        if fresh {
            Self::Search(NativeSearchTrajectoryRowV3::new(
                decision, role, ordinal, seed, selected, commitment, rejected,
            ))
        } else {
            Self::Legacy(NativeFullEpisodeTrajectoryDecisionRowV1 {
                row_ordinal: decision.step,
                actor_seat: decision.acting_player,
                actor_role: role,
                physical_decision_ordinal: decision.physical_decision_id,
                actor_physical_decision_ordinal: ordinal,
                substep_index: decision.substep_index,
                substep_count: decision.substep_count,
                action_seed: seed,
                legal_action_count: decision.legal_action_count,
                selected_index: selected,
                flat_action_v2_commitment: commitment,
            })
        }
    }
}

impl NativeLaneTrajectoryAccumulatorV3 {
    pub(crate) fn is_search(&self) -> bool {
        matches!(self, Self::Search(_))
    }
    pub(crate) fn preflight_candidate(&self, row: NativeLaneTrajectoryRowV3) -> Result<(), ()> {
        match (self, row) {
            (Self::Legacy(inner), NativeLaneTrajectoryRowV3::Legacy(row)) => {
                inner.preflight_candidate(row).map_err(|_| ())
            }
            (Self::Search(inner), NativeLaneTrajectoryRowV3::Search(row)) => {
                inner.preflight(row).map_err(|_| ())
            }
            _ => Err(()),
        }
    }
    pub(crate) fn record_accepted(&mut self, row: NativeLaneTrajectoryRowV3) -> Result<(), ()> {
        match (self, row) {
            (Self::Legacy(inner), NativeLaneTrajectoryRowV3::Legacy(row)) => {
                inner.record_accepted(row).map_err(|_| ())
            }
            (Self::Search(inner), NativeLaneTrajectoryRowV3::Search(row)) => {
                inner.record(row).map_err(|_| ())
            }
            _ => Err(()),
        }
    }
    pub(crate) fn finish_natural(
        self,
        terminal: AsyncRolloutTerminalV1,
        hashes: SessionDeckHashesV1,
    ) -> Result<NativeLaneTrajectoryReceiptV3, ()> {
        match self {
            Self::Legacy(inner) => inner
                .finish_natural(terminal, hashes)
                .map(NativeLaneTrajectoryReceiptV3::Legacy)
                .map_err(|_| ()),
            Self::Search(inner) => inner
                .finish(terminal, hashes)
                .map(NativeLaneTrajectoryReceiptV3::Search)
                .map_err(|_| ()),
        }
    }
}
