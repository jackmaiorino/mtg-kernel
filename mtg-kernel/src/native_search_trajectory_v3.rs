//! Fresh native collection receipts. Frozen trajectory V1/V2 bytes are not reused.
use crate::async_rollout::AsyncRolloutTerminalV1;
use crate::native_full_episode_trajectory_v1::{
    NativeFullEpisodeTrajectoryAccumulatorV1, NativeFullEpisodeTrajectoryDecisionRowV1,
    NativeFullEpisodeTrajectoryErrorV1, NativeFullEpisodeTrajectoryReceiptV1, NativeTrajectoryActorRoleV1,
};
use crate::native_full_episode_trajectory_v2::{
    NativeFullEpisodeTrajectoryErrorV2, NativeRunBoundFullEpisodeAccumulatorV2,
    NativeTrainingTrajectoryReceiptV2,
};
use crate::rl::PlayerSeatV1;
use crate::rl_session::{FastActorDecisionV1, SessionDeckHashesV1, SessionDeckIdsV1};
use sha2::{Digest, Sha256};

pub const NATIVE_SEARCH_TRAJECTORY_IDENTITY_V3: &str = "mtg-kernel-native-search-trajectory/v3";
pub const NATIVE_SEARCH_ACTION_CONTRACT_V3: &str = "flat-action-v4-visible-menu/v3";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NativeSearchTrajectoryReceiptV3 {
    metadata: NativeFullEpisodeTrajectoryReceiptV1,
    digest: [u8; 32],
    search_authority_digest: [u8; 32],
}

impl NativeSearchTrajectoryReceiptV3 {
    pub fn identity(&self) -> &'static str { NATIVE_SEARCH_TRAJECTORY_IDENTITY_V3 }
    pub fn action_contract(&self) -> &'static str { NATIVE_SEARCH_ACTION_CONTRACT_V3 }
    pub fn episode_index(&self) -> u64 { self.metadata.episode_index }
    pub fn trajectory_sha256(&self) -> [u8; 32] { self.digest }
    pub fn search_authority_sha256(&self) -> [u8; 32] { self.search_authority_digest }
    pub fn policy_step_count(&self) -> u64 { self.metadata.policy_step_count }
    pub fn physical_decision_count(&self) -> u64 { self.metadata.physical_decision_count }
    pub fn learner_policy_step_count(&self) -> u64 { self.metadata.learner_policy_step_count }
    pub fn opponent_policy_step_count(&self) -> u64 { self.metadata.opponent_policy_step_count }
}

#[derive(Clone, Copy)]
pub(crate) struct NativeSearchTrajectoryRowV3 {
    decision: FastActorDecisionV1,
    actor_role: NativeTrajectoryActorRoleV1,
    actor_physical_ordinal: u64,
    action_seed: u64,
    selected_index: u32,
    visible_menu_commitment: [u8; 16],
}

impl NativeSearchTrajectoryRowV3 {
    pub(crate) fn new(
        decision: FastActorDecisionV1,
        actor_role: NativeTrajectoryActorRoleV1,
        actor_physical_ordinal: u64,
        action_seed: u64,
        selected_index: u32,
        visible_menu_commitment: [u8; 16],
    ) -> Self {
        Self { decision, actor_role, actor_physical_ordinal, action_seed, selected_index, visible_menu_commitment }
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
        for value in [self.decision.episode_id, self.decision.step,
            self.decision.physical_decision_id, self.actor_physical_ordinal, self.action_seed] {
            bytes.extend_from_slice(&value.to_be_bytes());
        }
        bytes.push(match self.decision.acting_player { PlayerSeatV1::P0 => 0, PlayerSeatV1::P1 => 1 });
        bytes.push(match self.actor_role { NativeTrajectoryActorRoleV1::Learner => 0, NativeTrajectoryActorRoleV1::Opponent => 1 });
        for value in [self.decision.substep_index, self.decision.substep_count,
            self.decision.legal_action_count, self.selected_index] {
            bytes.extend_from_slice(&value.to_be_bytes());
        }
        bytes.extend_from_slice(&self.visible_menu_commitment);
        bytes
    }
}

pub(crate) struct NativeSearchTrajectoryAccumulatorV3 {
    metadata: NativeFullEpisodeTrajectoryAccumulatorV1,
    digest: Sha256,
    search_authority_digest: [u8; 32],
}

fn frame(digest: &mut Sha256, tag: &[u8], bytes: &[u8]) {
    digest.update((tag.len() as u32).to_be_bytes());
    digest.update(tag);
    digest.update((bytes.len() as u64).to_be_bytes());
    digest.update(bytes);
}

impl NativeSearchTrajectoryAccumulatorV3 {
    pub(crate) fn new(
        episode: u64, seed: u64, decks: &SessionDeckIdsV1, hashes: SessionDeckHashesV1,
        learner: PlayerSeatV1, search_authority_digest: [u8; 32],
    ) -> Result<Self, NativeFullEpisodeTrajectoryErrorV1> {
        let metadata = NativeFullEpisodeTrajectoryAccumulatorV1::new_v1(episode, seed, decks, hashes, learner)?;
        let mut digest = Sha256::new();
        frame(&mut digest, b"identity", NATIVE_SEARCH_TRAJECTORY_IDENTITY_V3.as_bytes());
        frame(&mut digest, b"episode", &episode.to_be_bytes());
        frame(&mut digest, b"seed", &seed.to_be_bytes());
        frame(&mut digest, b"deck0", decks[0].as_bytes());
        frame(&mut digest, b"deck1", decks[1].as_bytes());
        frame(&mut digest, b"hash0", &hashes[0].to_be_bytes());
        frame(&mut digest, b"hash1", &hashes[1].to_be_bytes());
        frame(&mut digest, b"learner", &[match learner { PlayerSeatV1::P0 => 0, PlayerSeatV1::P1 => 1 }]);
        frame(&mut digest, b"search_authority", &search_authority_digest);
        Ok(Self { metadata, digest, search_authority_digest })
    }

    fn preflight(&self, row: NativeSearchTrajectoryRowV3) -> Result<(), NativeFullEpisodeTrajectoryErrorV1> {
        self.metadata.preflight_candidate_v1(row.metadata())
    }
    fn record(&mut self, row: NativeSearchTrajectoryRowV3) -> Result<(), NativeFullEpisodeTrajectoryErrorV1> {
        self.metadata.record_accepted_v1(row.metadata())?;
        frame(&mut self.digest, b"decision", &row.bytes());
        Ok(())
    }
    fn finish(mut self, terminal: AsyncRolloutTerminalV1, hashes: SessionDeckHashesV1)
        -> Result<NativeSearchTrajectoryReceiptV3, NativeFullEpisodeTrajectoryErrorV1> {
        let mut metadata = self.metadata.finish_natural_v1(terminal, hashes)?;
        for (tag, value) in [
            (b"policy_steps".as_slice(), metadata.policy_step_count),
            (b"physical_decisions".as_slice(), metadata.physical_decision_count),
            (b"learner_steps".as_slice(), metadata.learner_policy_step_count),
            (b"opponent_steps".as_slice(), metadata.opponent_policy_step_count),
        ] { frame(&mut self.digest, tag, &value.to_be_bytes()); }
        frame(&mut self.digest, b"terminal", format!("{terminal:?}").as_bytes());
        // The metadata-only V1 digest is discarded, never exposed as provenance.
        metadata.trajectory_sha256 = [0; 32];
        Ok(NativeSearchTrajectoryReceiptV3 {
            metadata, digest: self.digest.finalize().into(), search_authority_digest: self.search_authority_digest,
        })
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum NativeLaneTrajectoryReceiptV3 {
    Legacy(NativeTrainingTrajectoryReceiptV2),
    Search(NativeSearchTrajectoryReceiptV3),
}

pub(crate) enum NativeLaneTrajectoryRowV3 {
    Legacy(NativeFullEpisodeTrajectoryDecisionRowV1),
    Search(NativeSearchTrajectoryRowV3),
}

pub(crate) enum NativeLaneTrajectoryAccumulatorV3 {
    Legacy(NativeRunBoundFullEpisodeAccumulatorV2),
    Search(NativeSearchTrajectoryAccumulatorV3),
}

impl NativeLaneTrajectoryRowV3 {
    pub(crate) fn new(
        fresh: bool, decision: FastActorDecisionV1, role: NativeTrajectoryActorRoleV1,
        ordinal: u64, seed: u64, selected: u32, commitment: [u8; 16],
    ) -> Self {
        if fresh { Self::Search(NativeSearchTrajectoryRowV3::new(decision, role, ordinal, seed, selected, commitment)) }
        else { Self::Legacy(NativeFullEpisodeTrajectoryDecisionRowV1 {
            row_ordinal: decision.step, actor_seat: decision.acting_player, actor_role: role,
            physical_decision_ordinal: decision.physical_decision_id, actor_physical_decision_ordinal: ordinal,
            substep_index: decision.substep_index, substep_count: decision.substep_count,
            action_seed: seed, legal_action_count: decision.legal_action_count, selected_index: selected,
            flat_action_v2_commitment: commitment,
        }) }
    }
}

impl NativeLaneTrajectoryAccumulatorV3 {
    pub(crate) fn is_search(&self) -> bool { matches!(self, Self::Search(_)) }
    pub(crate) fn preflight_candidate(&self, row: NativeLaneTrajectoryRowV3) -> Result<(), ()> {
        match (self, row) {
            (Self::Legacy(inner), NativeLaneTrajectoryRowV3::Legacy(row)) => inner.preflight_candidate(row).map_err(|_| ()),
            (Self::Search(inner), NativeLaneTrajectoryRowV3::Search(row)) => inner.preflight(row).map_err(|_| ()),
            _ => Err(()),
        }
    }
    pub(crate) fn record_accepted(&mut self, row: NativeLaneTrajectoryRowV3) -> Result<(), ()> {
        match (self, row) {
            (Self::Legacy(inner), NativeLaneTrajectoryRowV3::Legacy(row)) => inner.record_accepted(row).map_err(|_| ()),
            (Self::Search(inner), NativeLaneTrajectoryRowV3::Search(row)) => inner.record(row).map_err(|_| ()),
            _ => Err(()),
        }
    }
    pub(crate) fn finish_natural(self, terminal: AsyncRolloutTerminalV1, hashes: SessionDeckHashesV1)
        -> Result<NativeLaneTrajectoryReceiptV3, ()> {
        match self {
            Self::Legacy(inner) => inner.finish_natural(terminal, hashes).map(NativeLaneTrajectoryReceiptV3::Legacy).map_err(|_| ()),
            Self::Search(inner) => inner.finish(terminal, hashes).map(NativeLaneTrajectoryReceiptV3::Search).map_err(|_| ()),
        }
    }
}
