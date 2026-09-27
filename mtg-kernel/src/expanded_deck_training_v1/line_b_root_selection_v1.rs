//! G115 line (b) teacher root selection (CODEX-G115-EXIT-PROPOSAL-20260927.md
//! v0.2 "One root per published game"; design entry section 3, countersigned
//! in the 11:23 dispositions, item 4).
//!
//! Eligible roots are decisions the learner policy made, of kind Surface, at
//! substep 0, with 2 to 8 legal actions. Each gets the rank
//! `SHA-256("mtg-kernel/line-b-teacher-root/v1\0" || root seed u64 LE ||
//! kernel_search_visible_key_v4(0))`; the smallest rank wins, ties by the
//! smaller physical decision id. The rank reads only the actor-visible key
//! (observation with step and physical ids zeroed, action semantics, public
//! history), never teacher values, winners or hidden state.
//!
//! The root is rebuilt by replaying the recorded trajectory from its episode
//! seed: the run driver hands trajectories to the update step as files, so a
//! session snapshot taken during collection could not reach the teacher. The
//! replay constructs the game exactly as collection does, checks every
//! recorded row against the live decision and the final terminal against the
//! recorded one, and holds only the current minimum's session. It draws no
//! RNG and never touches collected bytes.

use sha2::{Digest, Sha256};

use super::{episode_session_v1, err, seat, ExpandedEpisodeV1, ExpandedTrajectoryV1};
use crate::rl_session::{
    FastActorDecisionKindV1, FastActorDecisionV1, FastActorResponseV1, FastActorSessionV1,
};

pub(super) const LINE_B_ROOT_RANK_DOMAIN_V1: &[u8] = b"mtg-kernel/line-b-teacher-root/v1\0";
pub(super) const LINE_B_SEED_PREFIX_V1: &str = "g115-line-b-seed-v1|";
pub(super) const LINE_B_ROOT_MIN_ACTIONS_V1: u32 = 2;
pub(super) const LINE_B_ROOT_MAX_ACTIONS_V1: u32 = 8;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum LineBSeedKindV1 {
    Root,
    Rollout,
}

/// Codex's production seed label (11:23 dispositions, item 4): `slot` is the
/// game's slot in its update; zero padding 2/3/2 digits.
pub(super) fn line_b_seed_label_v1(
    block: u32,
    update: u32,
    slot: u32,
    kind: LineBSeedKindV1,
) -> String {
    let kind = match kind {
        LineBSeedKindV1::Root => "root",
        LineBSeedKindV1::Rollout => "rollout",
    };
    format!("screen-teacher/block/{block:02}/update/{update:03}/slot/{slot:02}/{kind}")
}

/// Unsigned big-endian first eight bytes of
/// `SHA-256(ASCII("g115-line-b-seed-v1|" + label))`.
pub(super) fn line_b_seed_v1(label: &str) -> u64 {
    let digest = Sha256::digest(format!("{LINE_B_SEED_PREFIX_V1}{label}").as_bytes());
    let mut first = [0_u8; 8];
    first.copy_from_slice(&digest[..8]);
    u64::from_be_bytes(first)
}

pub(super) fn line_b_root_rank_v1(root_seed: u64, visible_key: &[u8; 32]) -> [u8; 32] {
    let mut hash = Sha256::new();
    hash.update(LINE_B_ROOT_RANK_DOMAIN_V1);
    hash.update(root_seed.to_le_bytes());
    hash.update(visible_key);
    hash.finalize().into()
}

/// A row the learner policy produced: every row of a self-play episode, else
/// the learner seat's rows (the rule the update's learner groups use).
pub(super) fn line_b_learner_row_v1(episode: &ExpandedEpisodeV1, actor: u8) -> bool {
    episode.opponent.is_none() || actor == episode.learner_seat
}

pub(super) fn line_b_root_eligible_v1(decision: &FastActorDecisionV1, learner_row: bool) -> bool {
    learner_row
        && decision.decision_kind == FastActorDecisionKindV1::Surface
        && decision.substep_index == 0
        && (LINE_B_ROOT_MIN_ACTIONS_V1..=LINE_B_ROOT_MAX_ACTIONS_V1)
            .contains(&decision.legal_action_count)
}

/// One game's root: the replayed session at the root decision, before its
/// recorded action, and the root's identity.
#[derive(Clone)]
pub(super) struct LineBRootV1 {
    /// Index of the root's row in the trajectory's decisions.
    pub(super) decision_index: usize,
    pub(super) physical_decision_id: u64,
    pub(super) actor: u8,
    pub(super) rank: [u8; 32],
    pub(super) visible_key: [u8; 32],
    /// Eligible decisions in the game, the root included.
    pub(super) eligible_decisions: usize,
    pub(super) session: FastActorSessionV1,
}

/// Replays `trajectory` and returns its root under `root_seed`, or `None` when
/// the game has no eligible decision. Any disagreement between the replay and
/// the recorded rows or terminal is an error, never a skipped game.
pub(super) fn select_line_b_root_v1(
    trajectory: &ExpandedTrajectoryV1,
    root_seed: u64,
) -> Result<Option<LineBRootV1>, String> {
    let episode = &trajectory.episode;
    let configs = episode.configurations()?;
    let mut session = episode_session_v1(episode, &configs)?;
    let mut best: Option<LineBRootV1> = None;
    let mut eligible = 0_usize;
    for (index, row) in trajectory.decisions.iter().enumerate() {
        let FastActorResponseV1::Decision(decision) = session.current_response() else {
            return Err(format!(
                "line (b) root replay reached a terminal before recorded row {index}"
            ));
        };
        if decision.step != row.step
            || decision.physical_decision_id != row.physical_decision_id
            || decision.substep_index != row.substep_index
            || decision.substep_count != row.substep_count
            || seat(decision.acting_player) != row.actor
            || decision.legal_action_count as usize != row.logits.len()
        {
            return Err(format!(
                "line (b) root replay differs from recorded row {index}"
            ));
        }
        if line_b_root_eligible_v1(&decision, line_b_learner_row_v1(episode, row.actor)) {
            eligible += 1;
            let visible_key = session
                .kernel_search_visible_key_v4(0)
                .map_err(|error| format!("line (b) root visible key at row {index}: {error:?}"))?;
            let rank = line_b_root_rank_v1(root_seed, &visible_key);
            if best.as_ref().is_none_or(|current| {
                (rank, decision.physical_decision_id) < (current.rank, current.physical_decision_id)
            }) {
                best = Some(LineBRootV1 {
                    decision_index: index,
                    physical_decision_id: decision.physical_decision_id,
                    actor: row.actor,
                    rank,
                    visible_key,
                    eligible_decisions: 0,
                    session: session.clone(),
                });
            }
        }
        session
            .step(decision.episode_id, decision.step, row.selected)
            .map_err(err)?;
    }
    match session.current_response() {
        FastActorResponseV1::Terminal(terminal) if terminal == trajectory.terminal => {}
        _ => return Err("line (b) root replay does not end at the recorded terminal".into()),
    }
    Ok(best.map(|mut root| {
        root.eligible_decisions = eligible;
        root
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rl::PlayerSeatV1;

    #[test]
    fn production_seeds_and_labels_match_the_codex_manifest() {
        // Values from E:/mtg-g115-lineage-20260923/line-b-two-slot-seeds-v1.json
        // (32bb5a35), which pins 2 blocks x 200 updates x 10 game slots.
        for (block, update, slot, kind, seed) in [
            (
                0,
                0,
                0,
                LineBSeedKindV1::Root,
                14_225_360_668_983_011_589_u64,
            ),
            (
                0,
                0,
                0,
                LineBSeedKindV1::Rollout,
                13_800_005_743_277_661_060,
            ),
            (1, 199, 9, LineBSeedKindV1::Root, 9_666_773_713_970_933_699),
            (
                1,
                199,
                9,
                LineBSeedKindV1::Rollout,
                15_847_760_769_328_167_033,
            ),
            (
                1,
                42,
                7,
                LineBSeedKindV1::Rollout,
                9_428_169_789_892_316_021,
            ),
        ] {
            let label = line_b_seed_label_v1(block, update, slot, kind);
            assert_eq!(line_b_seed_v1(&label), seed, "{label}");
        }
        assert_eq!(
            line_b_seed_label_v1(1, 42, 7, LineBSeedKindV1::Root),
            "screen-teacher/block/01/update/042/slot/07/root"
        );
    }

    #[test]
    fn root_rank_is_the_domain_separated_visible_key_hash() {
        let key: [u8; 32] = core::array::from_fn(|i| i as u8);
        let hex = |bytes: [u8; 32]| bytes.iter().map(|b| format!("{b:02x}")).collect::<String>();
        // hashlib.sha256(b"mtg-kernel/line-b-teacher-root/v1\0" + seed LE + key)
        assert_eq!(
            hex(line_b_root_rank_v1(0x0123_4567_89ab_cdef, &key)),
            "6c72ec301d5c02255d2d3eb1fe02899d9b3ecbb611e46764df2d5ca4fc1c9da5"
        );
        assert_eq!(
            hex(line_b_root_rank_v1(14_225_360_668_983_011_589, &[0xab; 32])),
            "836517c6a67d62aa7a4c02c3b6eb351e16ef4512de0ae5917d5e4982982fe426"
        );
        assert_ne!(
            line_b_root_rank_v1(1, &key),
            line_b_root_rank_v1(2, &key),
            "the root seed must enter the rank"
        );
    }

    #[test]
    fn eligibility_is_learner_surface_substep_zero_and_two_to_eight_actions() {
        let decision = FastActorDecisionV1 {
            episode_id: 1,
            step: 7,
            environment_revision: 0,
            physical_decision_id: 5,
            substep_index: 0,
            substep_count: 1,
            acting_player: PlayerSeatV1::P0,
            decision_kind: FastActorDecisionKindV1::Surface,
            legal_action_count: 2,
        };
        for width in 0..=10 {
            let d = FastActorDecisionV1 {
                legal_action_count: width,
                ..decision
            };
            assert_eq!(line_b_root_eligible_v1(&d, true), (2..=8).contains(&width));
            assert!(!line_b_root_eligible_v1(&d, false));
        }
        for kind in [
            FastActorDecisionKindV1::AttackerInclusion,
            FastActorDecisionKindV1::BlockerInclusion,
        ] {
            let d = FastActorDecisionV1 {
                decision_kind: kind,
                ..decision
            };
            assert!(!line_b_root_eligible_v1(&d, true));
        }
        let d = FastActorDecisionV1 {
            substep_index: 1,
            substep_count: 2,
            ..decision
        };
        assert!(!line_b_root_eligible_v1(&d, true));
    }
}
