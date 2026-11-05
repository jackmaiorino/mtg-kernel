//! Lockstep training-mode copy behind the bridge's `x_kernel_flat_v4`.
//!
//! The protocol surface stays on `RlEpisodeSessionV1`. This copy is a
//! `FastActorSessionV1` in flat-action V3 mode (the session type and mode the
//! Phase 1 lineage trained and was evaluated on), created with the same
//! catalog decks, seed and limits and stepped with the same executed actions.
//! `verify` compares game state for equality after every step. The copy only
//! encodes; it never drives the game.
//!
//! Flat-action V3 normalizes candidates (`rl_session/flat_action_v3.rs`
//! `normalize_candidates`): search targets reordered, effect sources replaced
//! by their historical record with the same arena id, required-goad
//! exclusions and infeasible menace blocks removed. `map_rows_to_candidates_v1`
//! maps each training row back to the raw candidate id.

use crate::rl::{ActionSemanticV1, CardStableRefV1};
use crate::rl_session::{
    FastActorResponseV1, FastActorSessionV1, RlEpisodeSessionV1, RlSessionDecisionV1,
    RlSessionResponseV1, SessionDeckIdsV1,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum FlatV4LockstepErrorV1 {
    Reset(String),
    NoCurrentDecision,
    RowMap(&'static str),
    SelectionNotInTrainingList { candidate_id: u32 },
    FastStep(String),
    Diverged(&'static str),
}

impl FlatV4LockstepErrorV1 {
    /// Halted reason: the engine could not produce the model input.
    pub(crate) fn halt_reason(&self) -> String {
        let detail = match self {
            Self::Reset(message) => format!("reset:{message}"),
            Self::NoCurrentDecision => "no_current_decision".to_string(),
            Self::RowMap(class) => format!("row_map:{class}"),
            Self::SelectionNotInTrainingList { candidate_id } => {
                format!("selection_not_in_training_list:{candidate_id}")
            }
            Self::FastStep(message) => format!("fast_step:{message}"),
            Self::Diverged(what) => format!("diverged:{what}"),
        };
        format!("engine_contract_failure:flat_v4_lockstep:{detail}")
    }
}

/// The six effect variants whose `source` flat-action V3 relabels (mirror of
/// `rl_session/flat_action_v3.rs` `effect_source_mut`, which is private).
fn effect_source_v1(semantic: &ActionSemanticV1) -> Option<&CardStableRefV1> {
    match semantic {
        ActionSemanticV1::ChooseEffectOption { source, .. }
        | ActionSemanticV1::ChooseEffectTarget { source, .. }
        | ActionSemanticV1::FinishEffectSelection { source, .. }
        | ActionSemanticV1::ChooseEffectColor { source, .. }
        | ActionSemanticV1::ChooseEffectNumber { source, .. }
        | ActionSemanticV1::ChooseEffectBoolean { source, .. } => Some(source),
        _ => None,
    }
}

fn with_effect_source_v1(
    semantic: &ActionSemanticV1,
    replacement: &CardStableRefV1,
) -> ActionSemanticV1 {
    let mut copy = semantic.clone();
    match &mut copy {
        ActionSemanticV1::ChooseEffectOption { source, .. }
        | ActionSemanticV1::ChooseEffectTarget { source, .. }
        | ActionSemanticV1::FinishEffectSelection { source, .. }
        | ActionSemanticV1::ChooseEffectColor { source, .. }
        | ActionSemanticV1::ChooseEffectNumber { source, .. }
        | ActionSemanticV1::ChooseEffectBoolean { source, .. } => *source = replacement.clone(),
        _ => {}
    }
    copy
}

/// Equal semantics, or equal after giving the raw effect source the row's
/// historical record when both name the same arena id.
pub(crate) fn semantics_equivalent_v1(raw: &ActionSemanticV1, row: &ActionSemanticV1) -> bool {
    if raw == row {
        return true;
    }
    match (effect_source_v1(raw), effect_source_v1(row)) {
        (Some(raw_source), Some(row_source)) if raw_source.arena_id == row_source.arena_id => {
            with_effect_source_v1(raw, row_source) == *row
        }
        _ => false,
    }
}

/// For each training row, the unique raw candidate id it executes.
pub(crate) fn map_rows_to_candidates_v1(
    raw: &[ActionSemanticV1],
    rows: &[ActionSemanticV1],
) -> Result<Vec<u32>, FlatV4LockstepErrorV1> {
    let mut used = vec![false; raw.len()];
    let mut map = Vec::with_capacity(rows.len());
    for row in rows {
        let mut matches = raw
            .iter()
            .enumerate()
            .filter(|(_, candidate)| semantics_equivalent_v1(candidate, row));
        let Some((index, _)) = matches.next() else {
            return Err(FlatV4LockstepErrorV1::RowMap("unmatched_row"));
        };
        if matches.next().is_some() {
            return Err(FlatV4LockstepErrorV1::RowMap("ambiguous_row"));
        }
        if std::mem::replace(&mut used[index], true) {
            return Err(FlatV4LockstepErrorV1::RowMap("duplicate_candidate"));
        }
        map.push(
            u32::try_from(index).map_err(|_| FlatV4LockstepErrorV1::RowMap("candidate_index"))?,
        );
    }
    Ok(map)
}

#[derive(Clone)]
pub(crate) struct FlatV4LockstepV1 {
    episode_id: u64,
    fast: FastActorSessionV1,
}

impl FlatV4LockstepV1 {
    pub(crate) fn reset(
        episode_id: u64,
        env_seed: u64,
        max_physical_decisions: u64,
        max_policy_steps: u64,
        deck_ids: SessionDeckIdsV1,
    ) -> Result<Self, FlatV4LockstepErrorV1> {
        let fast = FastActorSessionV1::reset_with_decks_and_limits_flat_action_v3(
            episode_id,
            env_seed,
            max_physical_decisions,
            max_policy_steps,
            deck_ids,
        )
        .map_err(|error| FlatV4LockstepErrorV1::Reset(error.message))?;
        Ok(Self { episode_id, fast })
    }

    pub(crate) fn fast_session(&self) -> &FastActorSessionV1 {
        &self.fast
    }

    pub(crate) fn enable_spellbench_neutral_choices(&mut self) {
        self.fast.enable_spellbench_neutral_choices();
    }

    #[cfg(test)]
    pub(crate) fn fast_session_mut_for_test(&mut self) -> &mut FastActorSessionV1 {
        &mut self.fast
    }

    /// Exact lockstep check: same response kind and decision header (or the
    /// identical terminal), same counters, equal game state.
    pub(crate) fn verify(
        &self,
        raw: &RlEpisodeSessionV1,
        raw_response: &RlSessionResponseV1,
    ) -> Result<(), FlatV4LockstepErrorV1> {
        match (raw_response, self.fast.current_response()) {
            (
                RlSessionResponseV1::Decision(raw_decision),
                FastActorResponseV1::Decision(fast_decision),
            ) => {
                if raw_decision.step != fast_decision.step
                    || raw_decision.acting_player != fast_decision.acting_player
                    || raw_decision.physical_decision_id != fast_decision.physical_decision_id
                    || raw_decision.substep_index != fast_decision.substep_index
                    || raw_decision.substep_count != fast_decision.substep_count
                {
                    return Err(FlatV4LockstepErrorV1::Diverged("decision_header"));
                }
            }
            (
                RlSessionResponseV1::Terminal(raw_terminal),
                FastActorResponseV1::Terminal(fast_terminal),
            ) => {
                if *raw_terminal != fast_terminal {
                    return Err(FlatV4LockstepErrorV1::Diverged("terminal"));
                }
            }
            _ => return Err(FlatV4LockstepErrorV1::Diverged("response_kind")),
        }
        if raw.policy_step_count() != self.fast.policy_step_count()
            || raw.physical_decision_count() != self.fast.physical_decision_count()
        {
            return Err(FlatV4LockstepErrorV1::Diverged("counters"));
        }
        if raw.game_state() != self.fast.game_state() {
            return Err(FlatV4LockstepErrorV1::Diverged("game_state"));
        }
        Ok(())
    }

    /// Row r of the V4 tensor is the copy's candidate r; returns its raw id.
    pub(crate) fn row_candidate_ids(
        &self,
        raw_decision: &RlSessionDecisionV1,
    ) -> Result<Vec<u32>, FlatV4LockstepErrorV1> {
        let rows = self
            .fast
            .diagnostic_current_action_semantics()
            .ok_or(FlatV4LockstepErrorV1::NoCurrentDecision)?;
        let raw: Vec<ActionSemanticV1> = raw_decision
            .legal_actions
            .iter()
            .map(|action| action.semantic.clone())
            .collect();
        map_rows_to_candidates_v1(&raw, &rows)
    }

    /// Steps the copy with the row executing `candidate_id`. Leaves the copy
    /// untouched when the training list does not contain the selection.
    pub(crate) fn mirror_step(
        &mut self,
        row_candidate_ids: &[u32],
        expected_step: u64,
        candidate_id: u32,
    ) -> Result<(), FlatV4LockstepErrorV1> {
        let row = row_candidate_ids
            .iter()
            .position(|id| *id == candidate_id)
            .ok_or(FlatV4LockstepErrorV1::SelectionNotInTrainingList { candidate_id })?;
        let row = u32::try_from(row).map_err(|_| FlatV4LockstepErrorV1::RowMap("row_index"))?;
        self.fast
            .step(self.episode_id, expected_step, row)
            .map_err(|error| FlatV4LockstepErrorV1::FastStep(error.message))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rl::{PlayerSeatV1, TargetRefV1};
    use crate::rl_session::RlSessionResponseV1;
    use crate::state::{SplitMix64, Zone};

    const DECKS: [&str; 8] = [
        "Wildfire", "Rally", "Affinity", "Elves", "Spy", "Burn", "CawGates", "Faeries",
    ];

    fn card(arena_id: u32, zone: Zone, zone_change_count: u32) -> CardStableRefV1 {
        CardStableRefV1 {
            arena_id,
            card_db_id: 7,
            owner: PlayerSeatV1::P0,
            controller: PlayerSeatV1::P0,
            zone,
            zone_change_count,
        }
    }

    fn effect_target(source: &CardStableRefV1, target_id: u32) -> ActionSemanticV1 {
        ActionSemanticV1::ChooseEffectTarget {
            actor: PlayerSeatV1::P0,
            source: source.clone(),
            target: TargetRefV1::Object {
                object: card(target_id, Zone::Battlefield, 1),
            },
            selected_count: 0,
            min_targets: 1,
            max_targets: 1,
        }
    }

    #[test]
    fn row_map_follows_reordering_filtering_and_effect_source_relabels() {
        let live = card(40, Zone::Stack, 3);
        let historical = card(40, Zone::Graveyard, 2);
        let raw = vec![
            effect_target(&live, 1),
            effect_target(&live, 2),
            ActionSemanticV1::Pass {
                actor: PlayerSeatV1::P0,
            },
        ];
        // Training mode relabels the source, reverses the targets, drops Pass.
        let rows = vec![effect_target(&historical, 2), effect_target(&historical, 1)];
        assert_eq!(map_rows_to_candidates_v1(&raw, &rows).unwrap(), vec![1, 0]);
    }

    #[test]
    fn row_map_rejects_unmatched_ambiguous_and_foreign_source_rows() {
        let live = card(40, Zone::Stack, 3);
        let other = card(41, Zone::Graveyard, 2);
        let raw = vec![effect_target(&live, 1), effect_target(&live, 1)];
        assert_eq!(
            map_rows_to_candidates_v1(&raw, &[effect_target(&live, 1)]),
            Err(FlatV4LockstepErrorV1::RowMap("ambiguous_row"))
        );
        assert_eq!(
            map_rows_to_candidates_v1(&raw[..1], &[effect_target(&other, 1)]),
            Err(FlatV4LockstepErrorV1::RowMap("unmatched_row"))
        );
        assert!(!semantics_equivalent_v1(
            &effect_target(&live, 1),
            &effect_target(&live, 2)
        ));
    }

    #[test]
    fn lockstep_follows_the_raw_session_through_whole_games_on_every_catalog_deck() {
        for (index, deck) in DECKS.iter().enumerate() {
            for seed in [11_u64, 29, 47] {
                let episode_id = 0xB0B0_0000 + index as u64 * 1_000 + seed;
                let decks: SessionDeckIdsV1 = [deck.to_string(), deck.to_string()];
                let mut raw = RlEpisodeSessionV1::reset_with_decks_and_limits(
                    episode_id,
                    seed,
                    2_000,
                    200_000,
                    decks.clone(),
                )
                .unwrap();
                let mut lockstep =
                    FlatV4LockstepV1::reset(episode_id, seed, 2_000, 200_000, decks).unwrap();
                let mut rng = SplitMix64::seed(seed ^ 0x5EED);
                let mut steps = 0_u64;
                loop {
                    let response = raw.current_response();
                    lockstep.verify(&raw, &response).unwrap();
                    let RlSessionResponseV1::Decision(decision) = response else {
                        break;
                    };
                    let rows = lockstep.row_candidate_ids(&decision).unwrap();
                    let pick = rows[(rng.next_u64() % rows.len() as u64) as usize];
                    let chosen = &decision.legal_actions[pick as usize];
                    raw.step(decision.episode_id, decision.step, pick, &chosen.stable_id)
                        .unwrap();
                    lockstep.mirror_step(&rows, decision.step, pick).unwrap();
                    steps += 1;
                }
                assert!(steps > 20, "{deck} seed {seed}: only {steps} steps");
            }
        }
    }

    #[test]
    fn mirror_step_refuses_a_selection_the_training_list_dropped() {
        let decks: SessionDeckIdsV1 = ["Burn".to_string(), "Burn".to_string()];
        let mut lockstep = FlatV4LockstepV1::reset(5, 5, 400, 4_000, decks).unwrap();
        let before = lockstep.fast_session().policy_step_count();
        assert_eq!(
            lockstep.mirror_step(&[1, 0], 0, 2),
            Err(FlatV4LockstepErrorV1::SelectionNotInTrainingList { candidate_id: 2 })
        );
        assert_eq!(lockstep.fast_session().policy_step_count(), before);
        assert_eq!(
            FlatV4LockstepErrorV1::SelectionNotInTrainingList { candidate_id: 2 }.halt_reason(),
            "engine_contract_failure:flat_v4_lockstep:selection_not_in_training_list:2"
        );
    }

    #[test]
    fn verify_detects_a_copy_of_a_different_game() {
        let decks: SessionDeckIdsV1 = ["Rally".to_string(), "Rally".to_string()];
        let raw = RlEpisodeSessionV1::reset_with_decks_and_limits(9, 11, 400, 4_000, decks.clone())
            .unwrap();
        let other = FlatV4LockstepV1::reset(9, 12, 400, 4_000, decks).unwrap();
        let error = other.verify(&raw, &raw.current_response()).unwrap_err();
        assert!(
            matches!(error, FlatV4LockstepErrorV1::Diverged(_)),
            "{error:?}"
        );
    }
}
