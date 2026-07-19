//! I/O-free, worker-local full-episode trajectory commitment for the native
//! trainer.  The accumulator owns only bounded counters, one open-group
//! descriptor, and one SHA-256 state; accepted decision rows are never
//! retained.  Its byte contract is frozen as
//! `mtg-kernel-native-full-episode-trajectory-sha256-v1`.

use crate::async_rollout::AsyncRolloutTerminalV1;
use crate::rl::{PlayerSeatV1, TerminalClassificationV1, TerminalOutcomeV1, TerminalSafeCodeV2};
use crate::rl_session::{SessionDeckHashesV1, SessionDeckIdsV1};
use sha2::{Digest, Sha256};

pub(crate) const NATIVE_FULL_EPISODE_TRAJECTORY_IDENTITY_V1: &str =
    "mtg-kernel-native-full-episode-trajectory-sha256-v1";

const MAX_DECK_ID_BYTES_V1: usize = 64;
const MAX_LEGAL_ACTION_COUNT_V1: u32 = 64;

const fn atom_encoded_len_v1(tag: &str, payload_len: usize) -> usize {
    4 + tag.len() + 8 + payload_len
}

const DECISION_ROW_PAYLOAD_LEN_V1: usize = atom_encoded_len_v1("row_ordinal_u64be", 8)
    + atom_encoded_len_v1("actor_seat_u8", 1)
    + atom_encoded_len_v1("actor_role_u8", 1)
    + atom_encoded_len_v1("physical_decision_ordinal_u64be", 8)
    + atom_encoded_len_v1("actor_physical_decision_ordinal_u64be", 8)
    + atom_encoded_len_v1("substep_index_u32be", 4)
    + atom_encoded_len_v1("substep_count_u32be", 4)
    + atom_encoded_len_v1("action_seed_u64be", 8)
    + atom_encoded_len_v1("legal_action_count_u32be", 4)
    + atom_encoded_len_v1("selected_index_u32be", 4)
    + atom_encoded_len_v1("flat_action_v2_commitment_raw16", 16);

const TERMINAL_ROW_PAYLOAD_LEN_V1: usize = atom_encoded_len_v1("terminal_outcome_u8", 1)
    + atom_encoded_len_v1("winner_option_u8", 1)
    + atom_encoded_len_v1("terminal_classification_u8", 1)
    + atom_encoded_len_v1("terminal_code_u8", 1)
    + atom_encoded_len_v1("policy_step_count_u64be", 8)
    + atom_encoded_len_v1("physical_decision_count_u64be", 8)
    + atom_encoded_len_v1("learner_policy_step_count_u64be", 8)
    + atom_encoded_len_v1("opponent_policy_step_count_u64be", 8)
    + atom_encoded_len_v1("learner_physical_decision_count_u64be", 8)
    + atom_encoded_len_v1("opponent_physical_decision_count_u64be", 8);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NativeTrajectoryActorRoleV1 {
    Learner,
    Opponent,
}

impl NativeTrajectoryActorRoleV1 {
    const fn code_v1(self) -> u8 {
        match self {
            Self::Learner => 0,
            Self::Opponent => 1,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct NativeFullEpisodeTrajectoryDecisionRowV1 {
    pub(crate) row_ordinal: u64,
    pub(crate) actor_seat: PlayerSeatV1,
    pub(crate) actor_role: NativeTrajectoryActorRoleV1,
    pub(crate) physical_decision_ordinal: u64,
    pub(crate) actor_physical_decision_ordinal: u64,
    pub(crate) substep_index: u32,
    pub(crate) substep_count: u32,
    pub(crate) action_seed: u64,
    pub(crate) legal_action_count: u32,
    pub(crate) selected_index: u32,
    /// The fixed array makes every non-16-byte Rust input unrepresentable;
    /// artifact parsers still reject malformed lengths at their boundary.
    pub(crate) flat_action_v2_commitment: [u8; 16],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct NativeFullEpisodeTrajectoryReceiptV1 {
    pub(crate) episode_index: u64,
    pub(crate) environment_seed: u64,
    pub(crate) deck_hashes: SessionDeckHashesV1,
    pub(crate) learner_seat: PlayerSeatV1,
    pub(crate) trajectory_sha256: [u8; 32],
    pub(crate) policy_step_count: u64,
    pub(crate) physical_decision_count: u64,
    pub(crate) learner_policy_step_count: u64,
    pub(crate) opponent_policy_step_count: u64,
    pub(crate) learner_physical_decision_count: u64,
    pub(crate) opponent_physical_decision_count: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NativeFullEpisodeTrajectoryErrorV1 {
    InvalidDeckId,
    EpisodeMismatch,
    EmptyDecisionStream,
    RowOrdinalMismatch,
    ActorRoleMismatch,
    MalformedPhysicalGroup,
    InvalidLegalActionCount,
    SelectedIndexOutOfRange,
    CounterOverflow,
    NonNaturalTerminal,
    TerminalProvenanceMismatch,
    TerminalCountMismatch,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct NativeOpenTrajectoryGroupV1 {
    actor_seat: PlayerSeatV1,
    actor_role: NativeTrajectoryActorRoleV1,
    physical_decision_ordinal: u64,
    actor_physical_decision_ordinal: u64,
    substep_count: u32,
    next_substep_index: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct NativeTrajectoryDecisionTransitionV1 {
    next_row_ordinal: u64,
    learner_policy_step_count: u64,
    opponent_policy_step_count: u64,
    learner_physical_decision_count: u64,
    opponent_physical_decision_count: u64,
    open_group: Option<NativeOpenTrajectoryGroupV1>,
}

pub(crate) struct NativeFullEpisodeTrajectoryAccumulatorV1 {
    episode_index: u64,
    environment_seed: u64,
    deck_hashes: SessionDeckHashesV1,
    learner_seat: PlayerSeatV1,
    hasher: Sha256,
    next_row_ordinal: u64,
    learner_policy_step_count: u64,
    opponent_policy_step_count: u64,
    learner_physical_decision_count: u64,
    opponent_physical_decision_count: u64,
    open_group: Option<NativeOpenTrajectoryGroupV1>,
}

impl NativeFullEpisodeTrajectoryAccumulatorV1 {
    pub(crate) fn new_v1(
        episode_index: u64,
        environment_seed: u64,
        deck_ids: &SessionDeckIdsV1,
        deck_hashes: SessionDeckHashesV1,
        learner_seat: PlayerSeatV1,
    ) -> Result<Self, NativeFullEpisodeTrajectoryErrorV1> {
        if deck_ids.iter().any(|deck_id| !valid_deck_id_v1(deck_id)) {
            return Err(NativeFullEpisodeTrajectoryErrorV1::InvalidDeckId);
        }

        let mut hasher = Sha256::new();
        hash_atom_v1(
            &mut hasher,
            "domain",
            NATIVE_FULL_EPISODE_TRAJECTORY_IDENTITY_V1.as_bytes(),
        );
        hash_atom_v1(
            &mut hasher,
            "episode_index_u64be",
            &episode_index.to_be_bytes(),
        );
        hash_atom_v1(
            &mut hasher,
            "environment_seed_u64be",
            &environment_seed.to_be_bytes(),
        );
        hash_atom_v1(&mut hasher, "deck_p0_id_utf8", deck_ids[0].as_bytes());
        hash_atom_v1(
            &mut hasher,
            "deck_p0_hash_u64be",
            &deck_hashes[0].to_be_bytes(),
        );
        hash_atom_v1(&mut hasher, "deck_p1_id_utf8", deck_ids[1].as_bytes());
        hash_atom_v1(
            &mut hasher,
            "deck_p1_hash_u64be",
            &deck_hashes[1].to_be_bytes(),
        );
        hash_atom_v1(
            &mut hasher,
            "learner_seat_u8",
            &[player_seat_code_v1(learner_seat)],
        );

        Ok(Self {
            episode_index,
            environment_seed,
            deck_hashes,
            learner_seat,
            hasher,
            next_row_ordinal: 0,
            learner_policy_step_count: 0,
            opponent_policy_step_count: 0,
            learner_physical_decision_count: 0,
            opponent_physical_decision_count: 0,
            open_group: None,
        })
    }

    /// Validates a candidate without changing the digest or counters.  Worker
    /// lanes use this immediately before engine mutation, then call
    /// `record_accepted_v1` only after the engine accepts the same action.
    pub(crate) fn preflight_candidate_v1(
        &self,
        row: NativeFullEpisodeTrajectoryDecisionRowV1,
    ) -> Result<(), NativeFullEpisodeTrajectoryErrorV1> {
        self.transition_v1(row).map(|_| ())
    }

    pub(crate) fn record_accepted_v1(
        &mut self,
        row: NativeFullEpisodeTrajectoryDecisionRowV1,
    ) -> Result<(), NativeFullEpisodeTrajectoryErrorV1> {
        let transition = self.transition_v1(row)?;

        hash_atom_header_v1(
            &mut self.hasher,
            "decision_row",
            DECISION_ROW_PAYLOAD_LEN_V1,
        );
        hash_atom_v1(
            &mut self.hasher,
            "row_ordinal_u64be",
            &row.row_ordinal.to_be_bytes(),
        );
        hash_atom_v1(
            &mut self.hasher,
            "actor_seat_u8",
            &[player_seat_code_v1(row.actor_seat)],
        );
        hash_atom_v1(
            &mut self.hasher,
            "actor_role_u8",
            &[row.actor_role.code_v1()],
        );
        hash_atom_v1(
            &mut self.hasher,
            "physical_decision_ordinal_u64be",
            &row.physical_decision_ordinal.to_be_bytes(),
        );
        hash_atom_v1(
            &mut self.hasher,
            "actor_physical_decision_ordinal_u64be",
            &row.actor_physical_decision_ordinal.to_be_bytes(),
        );
        hash_atom_v1(
            &mut self.hasher,
            "substep_index_u32be",
            &row.substep_index.to_be_bytes(),
        );
        hash_atom_v1(
            &mut self.hasher,
            "substep_count_u32be",
            &row.substep_count.to_be_bytes(),
        );
        hash_atom_v1(
            &mut self.hasher,
            "action_seed_u64be",
            &row.action_seed.to_be_bytes(),
        );
        hash_atom_v1(
            &mut self.hasher,
            "legal_action_count_u32be",
            &row.legal_action_count.to_be_bytes(),
        );
        hash_atom_v1(
            &mut self.hasher,
            "selected_index_u32be",
            &row.selected_index.to_be_bytes(),
        );
        hash_atom_v1(
            &mut self.hasher,
            "flat_action_v2_commitment_raw16",
            &row.flat_action_v2_commitment,
        );

        self.next_row_ordinal = transition.next_row_ordinal;
        self.learner_policy_step_count = transition.learner_policy_step_count;
        self.opponent_policy_step_count = transition.opponent_policy_step_count;
        self.learner_physical_decision_count = transition.learner_physical_decision_count;
        self.opponent_physical_decision_count = transition.opponent_physical_decision_count;
        self.open_group = transition.open_group;
        Ok(())
    }

    pub(crate) fn finish_natural_v1(
        mut self,
        terminal: AsyncRolloutTerminalV1,
        terminal_deck_hashes: SessionDeckHashesV1,
    ) -> Result<NativeFullEpisodeTrajectoryReceiptV1, NativeFullEpisodeTrajectoryErrorV1> {
        if terminal.episode_id != self.episode_index {
            return Err(NativeFullEpisodeTrajectoryErrorV1::EpisodeMismatch);
        }
        if self.next_row_ordinal == 0 {
            return Err(NativeFullEpisodeTrajectoryErrorV1::EmptyDecisionStream);
        }
        if self.open_group.is_some() {
            return Err(NativeFullEpisodeTrajectoryErrorV1::MalformedPhysicalGroup);
        }
        if terminal_deck_hashes != self.deck_hashes {
            return Err(NativeFullEpisodeTrajectoryErrorV1::TerminalProvenanceMismatch);
        }
        let (outcome_code, winner_code, classification_code, terminal_code) =
            natural_terminal_codes_v1(terminal)?;
        let policy_step_count = self
            .learner_policy_step_count
            .checked_add(self.opponent_policy_step_count)
            .ok_or(NativeFullEpisodeTrajectoryErrorV1::CounterOverflow)?;
        let physical_decision_count = self
            .learner_physical_decision_count
            .checked_add(self.opponent_physical_decision_count)
            .ok_or(NativeFullEpisodeTrajectoryErrorV1::CounterOverflow)?;
        if terminal.policy_step_count != policy_step_count
            || terminal.physical_decision_count != physical_decision_count
            || self.next_row_ordinal != policy_step_count
        {
            return Err(NativeFullEpisodeTrajectoryErrorV1::TerminalCountMismatch);
        }

        hash_atom_header_v1(
            &mut self.hasher,
            "terminal_row",
            TERMINAL_ROW_PAYLOAD_LEN_V1,
        );
        hash_atom_v1(&mut self.hasher, "terminal_outcome_u8", &[outcome_code]);
        hash_atom_v1(&mut self.hasher, "winner_option_u8", &[winner_code]);
        hash_atom_v1(
            &mut self.hasher,
            "terminal_classification_u8",
            &[classification_code],
        );
        hash_atom_v1(&mut self.hasher, "terminal_code_u8", &[terminal_code]);
        hash_atom_v1(
            &mut self.hasher,
            "policy_step_count_u64be",
            &policy_step_count.to_be_bytes(),
        );
        hash_atom_v1(
            &mut self.hasher,
            "physical_decision_count_u64be",
            &physical_decision_count.to_be_bytes(),
        );
        hash_atom_v1(
            &mut self.hasher,
            "learner_policy_step_count_u64be",
            &self.learner_policy_step_count.to_be_bytes(),
        );
        hash_atom_v1(
            &mut self.hasher,
            "opponent_policy_step_count_u64be",
            &self.opponent_policy_step_count.to_be_bytes(),
        );
        hash_atom_v1(
            &mut self.hasher,
            "learner_physical_decision_count_u64be",
            &self.learner_physical_decision_count.to_be_bytes(),
        );
        hash_atom_v1(
            &mut self.hasher,
            "opponent_physical_decision_count_u64be",
            &self.opponent_physical_decision_count.to_be_bytes(),
        );

        Ok(NativeFullEpisodeTrajectoryReceiptV1 {
            episode_index: self.episode_index,
            environment_seed: self.environment_seed,
            deck_hashes: self.deck_hashes,
            learner_seat: self.learner_seat,
            trajectory_sha256: self.hasher.finalize().into(),
            policy_step_count,
            physical_decision_count,
            learner_policy_step_count: self.learner_policy_step_count,
            opponent_policy_step_count: self.opponent_policy_step_count,
            learner_physical_decision_count: self.learner_physical_decision_count,
            opponent_physical_decision_count: self.opponent_physical_decision_count,
        })
    }

    fn transition_v1(
        &self,
        row: NativeFullEpisodeTrajectoryDecisionRowV1,
    ) -> Result<NativeTrajectoryDecisionTransitionV1, NativeFullEpisodeTrajectoryErrorV1> {
        if row.row_ordinal != self.next_row_ordinal {
            return Err(NativeFullEpisodeTrajectoryErrorV1::RowOrdinalMismatch);
        }
        let expected_role = if row.actor_seat == self.learner_seat {
            NativeTrajectoryActorRoleV1::Learner
        } else {
            NativeTrajectoryActorRoleV1::Opponent
        };
        if row.actor_role != expected_role {
            return Err(NativeFullEpisodeTrajectoryErrorV1::ActorRoleMismatch);
        }
        if row.legal_action_count == 0 || row.legal_action_count > MAX_LEGAL_ACTION_COUNT_V1 {
            return Err(NativeFullEpisodeTrajectoryErrorV1::InvalidLegalActionCount);
        }
        if row.selected_index >= row.legal_action_count {
            return Err(NativeFullEpisodeTrajectoryErrorV1::SelectedIndexOutOfRange);
        }
        if row.substep_count == 0 || row.substep_index >= row.substep_count {
            return Err(NativeFullEpisodeTrajectoryErrorV1::MalformedPhysicalGroup);
        }

        let mut transition = NativeTrajectoryDecisionTransitionV1 {
            next_row_ordinal: self
                .next_row_ordinal
                .checked_add(1)
                .ok_or(NativeFullEpisodeTrajectoryErrorV1::CounterOverflow)?,
            learner_policy_step_count: self.learner_policy_step_count,
            opponent_policy_step_count: self.opponent_policy_step_count,
            learner_physical_decision_count: self.learner_physical_decision_count,
            opponent_physical_decision_count: self.opponent_physical_decision_count,
            open_group: self.open_group,
        };
        match row.actor_role {
            NativeTrajectoryActorRoleV1::Learner => {
                transition.learner_policy_step_count = transition
                    .learner_policy_step_count
                    .checked_add(1)
                    .ok_or(NativeFullEpisodeTrajectoryErrorV1::CounterOverflow)?;
            }
            NativeTrajectoryActorRoleV1::Opponent => {
                transition.opponent_policy_step_count = transition
                    .opponent_policy_step_count
                    .checked_add(1)
                    .ok_or(NativeFullEpisodeTrajectoryErrorV1::CounterOverflow)?;
            }
        }

        match self.open_group {
            None => {
                let expected_physical_ordinal = self
                    .learner_physical_decision_count
                    .checked_add(self.opponent_physical_decision_count)
                    .ok_or(NativeFullEpisodeTrajectoryErrorV1::CounterOverflow)?;
                let expected_actor_ordinal = match row.actor_role {
                    NativeTrajectoryActorRoleV1::Learner => self.learner_physical_decision_count,
                    NativeTrajectoryActorRoleV1::Opponent => self.opponent_physical_decision_count,
                };
                if row.substep_index != 0
                    || row.physical_decision_ordinal != expected_physical_ordinal
                    || row.actor_physical_decision_ordinal != expected_actor_ordinal
                {
                    return Err(NativeFullEpisodeTrajectoryErrorV1::MalformedPhysicalGroup);
                }
            }
            Some(open)
                if open.actor_seat == row.actor_seat
                    && open.actor_role == row.actor_role
                    && open.physical_decision_ordinal == row.physical_decision_ordinal
                    && open.actor_physical_decision_ordinal
                        == row.actor_physical_decision_ordinal
                    && open.substep_count == row.substep_count
                    && open.next_substep_index == row.substep_index => {}
            Some(_) => {
                return Err(NativeFullEpisodeTrajectoryErrorV1::MalformedPhysicalGroup);
            }
        }

        let group_complete = row
            .substep_index
            .checked_add(1)
            .is_some_and(|next| next == row.substep_count);
        if group_complete {
            match row.actor_role {
                NativeTrajectoryActorRoleV1::Learner => {
                    transition.learner_physical_decision_count = transition
                        .learner_physical_decision_count
                        .checked_add(1)
                        .ok_or(NativeFullEpisodeTrajectoryErrorV1::CounterOverflow)?;
                }
                NativeTrajectoryActorRoleV1::Opponent => {
                    transition.opponent_physical_decision_count = transition
                        .opponent_physical_decision_count
                        .checked_add(1)
                        .ok_or(NativeFullEpisodeTrajectoryErrorV1::CounterOverflow)?;
                }
            }
            transition.open_group = None;
        } else {
            transition.open_group = Some(NativeOpenTrajectoryGroupV1 {
                actor_seat: row.actor_seat,
                actor_role: row.actor_role,
                physical_decision_ordinal: row.physical_decision_ordinal,
                actor_physical_decision_ordinal: row.actor_physical_decision_ordinal,
                substep_count: row.substep_count,
                next_substep_index: row
                    .substep_index
                    .checked_add(1)
                    .ok_or(NativeFullEpisodeTrajectoryErrorV1::CounterOverflow)?,
            });
        }
        Ok(transition)
    }
}

fn valid_deck_id_v1(deck_id: &str) -> bool {
    let bytes = deck_id.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= MAX_DECK_ID_BYTES_V1
        && bytes.iter().all(|byte| (0x20..=0x7e).contains(byte))
}

const fn player_seat_code_v1(seat: PlayerSeatV1) -> u8 {
    match seat {
        PlayerSeatV1::P0 => 0,
        PlayerSeatV1::P1 => 1,
    }
}

fn natural_terminal_codes_v1(
    terminal: AsyncRolloutTerminalV1,
) -> Result<(u8, u8, u8, u8), NativeFullEpisodeTrajectoryErrorV1> {
    match (
        terminal.terminal_outcome,
        terminal.winner,
        terminal.terminal_classification,
        terminal.terminal_code,
    ) {
        (
            TerminalOutcomeV1::P0Win,
            Some(PlayerSeatV1::P0),
            TerminalClassificationV1::Natural,
            TerminalSafeCodeV2::NaturalGameOver,
        ) => Ok((0, 1, 0, 0)),
        (
            TerminalOutcomeV1::P1Win,
            Some(PlayerSeatV1::P1),
            TerminalClassificationV1::Natural,
            TerminalSafeCodeV2::NaturalGameOver,
        ) => Ok((1, 2, 0, 0)),
        (
            TerminalOutcomeV1::Draw,
            None,
            TerminalClassificationV1::Natural,
            TerminalSafeCodeV2::NaturalGameOver,
        ) => Ok((2, 0, 0, 0)),
        _ => Err(NativeFullEpisodeTrajectoryErrorV1::NonNaturalTerminal),
    }
}

fn hash_atom_header_v1(hasher: &mut Sha256, tag: &str, payload_len: usize) {
    let tag_len = u32::try_from(tag.len()).expect("trajectory atom tag length fits u32");
    let payload_len = u64::try_from(payload_len).expect("trajectory atom payload length fits u64");
    hasher.update(tag_len.to_be_bytes());
    hasher.update(tag.as_bytes());
    hasher.update(payload_len.to_be_bytes());
}

fn hash_atom_v1(hasher: &mut Sha256, tag: &str, payload: &[u8]) {
    hash_atom_header_v1(hasher, tag, payload.len());
    hasher.update(payload);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decision_row_v1() -> NativeFullEpisodeTrajectoryDecisionRowV1 {
        NativeFullEpisodeTrajectoryDecisionRowV1 {
            row_ordinal: 0,
            actor_seat: PlayerSeatV1::P0,
            actor_role: NativeTrajectoryActorRoleV1::Learner,
            physical_decision_ordinal: 0,
            actor_physical_decision_ordinal: 0,
            substep_index: 0,
            substep_count: 1,
            action_seed: 0x0102_0304_0506_0708,
            legal_action_count: 1,
            selected_index: 0,
            flat_action_v2_commitment: [0x5a; 16],
        }
    }

    fn natural_terminal_v1() -> AsyncRolloutTerminalV1 {
        AsyncRolloutTerminalV1 {
            episode_id: 0x1112_1314_1516_1718,
            terminal_outcome: TerminalOutcomeV1::P0Win,
            terminal_classification: TerminalClassificationV1::Natural,
            terminal_code: TerminalSafeCodeV2::NaturalGameOver,
            winner: Some(PlayerSeatV1::P0),
            terminal_reward: [1, -1],
            policy_step_count: 1,
            physical_decision_count: 1,
        }
    }

    fn append_atom_v1(stream: &mut Vec<u8>, tag: &str, payload: &[u8]) {
        stream.extend_from_slice(&u32::try_from(tag.len()).unwrap().to_be_bytes());
        stream.extend_from_slice(tag.as_bytes());
        stream.extend_from_slice(&u64::try_from(payload.len()).unwrap().to_be_bytes());
        stream.extend_from_slice(payload);
    }

    #[test]
    fn exact_one_row_stream_matches_independent_byte_assembly() {
        let episode_index = 0x1112_1314_1516_1718;
        let environment_seed = 0x2122_2324_2526_2728;
        let deck_ids = ["Burn".to_string(), "Rally".to_string()];
        let deck_hashes = [0x3132_3334_3536_3738, 0x4142_4344_4546_4748];
        let row = decision_row_v1();
        let mut accumulator = NativeFullEpisodeTrajectoryAccumulatorV1::new_v1(
            episode_index,
            environment_seed,
            &deck_ids,
            deck_hashes,
            PlayerSeatV1::P0,
        )
        .unwrap();
        accumulator.preflight_candidate_v1(row).unwrap();
        accumulator.record_accepted_v1(row).unwrap();
        let receipt = accumulator
            .finish_natural_v1(natural_terminal_v1(), deck_hashes)
            .unwrap();

        let mut stream = Vec::new();
        append_atom_v1(
            &mut stream,
            "domain",
            b"mtg-kernel-native-full-episode-trajectory-sha256-v1",
        );
        append_atom_v1(
            &mut stream,
            "episode_index_u64be",
            &episode_index.to_be_bytes(),
        );
        append_atom_v1(
            &mut stream,
            "environment_seed_u64be",
            &environment_seed.to_be_bytes(),
        );
        append_atom_v1(&mut stream, "deck_p0_id_utf8", b"Burn");
        append_atom_v1(
            &mut stream,
            "deck_p0_hash_u64be",
            &deck_hashes[0].to_be_bytes(),
        );
        append_atom_v1(&mut stream, "deck_p1_id_utf8", b"Rally");
        append_atom_v1(
            &mut stream,
            "deck_p1_hash_u64be",
            &deck_hashes[1].to_be_bytes(),
        );
        append_atom_v1(&mut stream, "learner_seat_u8", &[0]);

        let mut decision_payload = Vec::new();
        append_atom_v1(
            &mut decision_payload,
            "row_ordinal_u64be",
            &0u64.to_be_bytes(),
        );
        append_atom_v1(&mut decision_payload, "actor_seat_u8", &[0]);
        append_atom_v1(&mut decision_payload, "actor_role_u8", &[0]);
        append_atom_v1(
            &mut decision_payload,
            "physical_decision_ordinal_u64be",
            &0u64.to_be_bytes(),
        );
        append_atom_v1(
            &mut decision_payload,
            "actor_physical_decision_ordinal_u64be",
            &0u64.to_be_bytes(),
        );
        append_atom_v1(
            &mut decision_payload,
            "substep_index_u32be",
            &0u32.to_be_bytes(),
        );
        append_atom_v1(
            &mut decision_payload,
            "substep_count_u32be",
            &1u32.to_be_bytes(),
        );
        append_atom_v1(
            &mut decision_payload,
            "action_seed_u64be",
            &row.action_seed.to_be_bytes(),
        );
        append_atom_v1(
            &mut decision_payload,
            "legal_action_count_u32be",
            &1u32.to_be_bytes(),
        );
        append_atom_v1(
            &mut decision_payload,
            "selected_index_u32be",
            &0u32.to_be_bytes(),
        );
        append_atom_v1(
            &mut decision_payload,
            "flat_action_v2_commitment_raw16",
            &[0x5a; 16],
        );
        assert_eq!(decision_payload.len(), DECISION_ROW_PAYLOAD_LEN_V1);
        append_atom_v1(&mut stream, "decision_row", &decision_payload);

        let mut terminal_payload = Vec::new();
        append_atom_v1(&mut terminal_payload, "terminal_outcome_u8", &[0]);
        append_atom_v1(&mut terminal_payload, "winner_option_u8", &[1]);
        append_atom_v1(&mut terminal_payload, "terminal_classification_u8", &[0]);
        append_atom_v1(&mut terminal_payload, "terminal_code_u8", &[0]);
        append_atom_v1(
            &mut terminal_payload,
            "policy_step_count_u64be",
            &1u64.to_be_bytes(),
        );
        append_atom_v1(
            &mut terminal_payload,
            "physical_decision_count_u64be",
            &1u64.to_be_bytes(),
        );
        append_atom_v1(
            &mut terminal_payload,
            "learner_policy_step_count_u64be",
            &1u64.to_be_bytes(),
        );
        append_atom_v1(
            &mut terminal_payload,
            "opponent_policy_step_count_u64be",
            &0u64.to_be_bytes(),
        );
        append_atom_v1(
            &mut terminal_payload,
            "learner_physical_decision_count_u64be",
            &1u64.to_be_bytes(),
        );
        append_atom_v1(
            &mut terminal_payload,
            "opponent_physical_decision_count_u64be",
            &0u64.to_be_bytes(),
        );
        assert_eq!(terminal_payload.len(), TERMINAL_ROW_PAYLOAD_LEN_V1);
        append_atom_v1(&mut stream, "terminal_row", &terminal_payload);

        let independently_assembled_sha256: [u8; 32] = Sha256::digest(stream).into();
        assert_eq!(receipt.trajectory_sha256, independently_assembled_sha256);
        assert_eq!(receipt.environment_seed, environment_seed);
        assert_eq!(receipt.deck_hashes, deck_hashes);
        assert_eq!(receipt.learner_seat, PlayerSeatV1::P0);
        assert_eq!(receipt.learner_policy_step_count, 1);
        assert_eq!(receipt.opponent_policy_step_count, 0);
        assert_eq!(receipt.learner_physical_decision_count, 1);
        assert_eq!(receipt.opponent_physical_decision_count, 0);
    }

    #[test]
    fn rejects_role_width_group_and_terminal_mismatches() {
        let deck_ids = ["Burn".to_string(), "Rally".to_string()];
        let accumulator = || {
            NativeFullEpisodeTrajectoryAccumulatorV1::new_v1(
                0x1112_1314_1516_1718,
                7,
                &deck_ids,
                [11, 13],
                PlayerSeatV1::P0,
            )
            .unwrap()
        };

        let mut wrong_role = decision_row_v1();
        wrong_role.actor_role = NativeTrajectoryActorRoleV1::Opponent;
        assert_eq!(
            accumulator().preflight_candidate_v1(wrong_role),
            Err(NativeFullEpisodeTrajectoryErrorV1::ActorRoleMismatch)
        );

        let mut width = decision_row_v1();
        width.legal_action_count = 65;
        assert_eq!(
            accumulator().preflight_candidate_v1(width),
            Err(NativeFullEpisodeTrajectoryErrorV1::InvalidLegalActionCount)
        );

        let mut incomplete = decision_row_v1();
        incomplete.substep_count = 2;
        let mut open = accumulator();
        open.record_accepted_v1(incomplete).unwrap();
        assert_eq!(
            open.finish_natural_v1(natural_terminal_v1(), [11, 13]),
            Err(NativeFullEpisodeTrajectoryErrorV1::MalformedPhysicalGroup)
        );

        let mut wrong_provenance = accumulator();
        wrong_provenance
            .record_accepted_v1(decision_row_v1())
            .unwrap();
        assert_eq!(
            wrong_provenance.finish_natural_v1(natural_terminal_v1(), [11, 99]),
            Err(NativeFullEpisodeTrajectoryErrorV1::TerminalProvenanceMismatch)
        );

        let mut non_natural = natural_terminal_v1();
        non_natural.terminal_outcome = TerminalOutcomeV1::Truncated;
        non_natural.terminal_classification = TerminalClassificationV1::Truncated;
        non_natural.terminal_code = TerminalSafeCodeV2::DecisionCap;
        non_natural.winner = None;
        let mut accepted = accumulator();
        accepted.record_accepted_v1(decision_row_v1()).unwrap();
        assert_eq!(
            accepted.finish_natural_v1(non_natural, [11, 13]),
            Err(NativeFullEpisodeTrajectoryErrorV1::NonNaturalTerminal)
        );
    }
}

#[cfg(test)]
#[path = "native_full_episode_trajectory_v1_goldens.rs"]
mod portable_goldens;
