//! Ordinary CR 514 cleanup. Only the current step is retained, never a private
//! historical world or a disposed-stack outcome. The active player's hand is
//! bound only while the actual discard decision is outstanding.
use super::*;
use std::hash::{Hash, Hasher};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct StateV1(Option<CleanupV1>);
impl StateV1 {
    pub fn is_empty(&self) -> bool {
        self.0.is_none()
    }
}
impl Hash for StateV1 {
    fn hash<H: Hasher>(&self, h: &mut H) {
        if let Some(value) = &self.0 {
            b"ordinary-cleanup-current/v1".hash(h);
            value.hash(h);
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CleanupV1 {
    turn: u32,
    active: PlayerId,
    end_step: u64,
    phase: PhaseV1,
    stage_index: u64,
    hand: Vec<(ObjectId, u32)>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
enum PhaseV1 {
    Discard,
    Expiration,
    Priority,
    GameOver,
    Finished,
    EndedByTimeStop,
    /// Native completion of End in a catalog without beginning-End triggers.
    /// This is not a retroactive beginning-of-step occurrence.
    EndCompleted,
}
/// Scalar native transition evidence only. No hand, object or world is retained.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StageV1 {
    turn: u32,
    active: PlayerId,
    end_step: u64,
    phase: PhaseV1,
}
fn terminal(phase: PhaseV1) -> bool {
    matches!(
        phase,
        PhaseV1::Finished | PhaseV1::EndedByTimeStop | PhaseV1::GameOver
    )
}

fn set_phase(state: &mut GameState, phase: PhaseV1) -> Result<(), String> {
    let index = u64::try_from(state.engine.event_history.len())
        .map_err(|_| "ordinary cleanup stage exceeds u64")?;
    let saved = state
        .engine
        .ordinary_cleanup_v1
        .0
        .as_mut()
        .ok_or("ordinary cleanup is absent")?;
    saved.phase = phase;
    saved.stage_index = index;
    let stage = StageV1 {
        turn: saved.turn,
        active: saved.active,
        end_step: saved.end_step,
        phase,
    };
    state
        .engine
        .event_history
        .push(CommittedEvent::OrdinaryCleanupStageV1 { stage });
    Ok(())
}
fn validate_stages(state: &GameState) -> Result<Option<(usize, &StageV1)>, String> {
    use crate::event_history_v1::EventKindV1;
    if state
        .engine
        .event_log
        .iter()
        .any(|event| matches!(event, CommittedEvent::OrdinaryCleanupStageV1 { .. }))
    {
        return Err("ordinary cleanup stage escaped its native history".into());
    }
    let mut last: Option<(usize, &StageV1)> = None;
    let mut end_step = None;
    let mut turn = None;
    for (index, event) in state
        .engine
        .event_history
        .scan_for_v1(&[
            EventKindV1::BeginningEndStep,
            EventKindV1::LifeLossTurnBeganV1,
            EventKindV1::OrdinaryCleanupStageV1,
        ])
        .iter()
        .enumerate()
    {
        match event {
            CommittedEvent::BeginningEndStep { active_player, .. } => {
                end_step = Some((index, *active_player));
            }
            CommittedEvent::LifeLossTurnBeganV1 {
                turn: occurrence,
                active_player,
            } => {
                turn = Some((*occurrence, *active_player));
                end_step = None;
            }
            CommittedEvent::OrdinaryCleanupStageV1 { stage } => {
                let end = usize::try_from(stage.end_step)
                    .map_err(|_| "ordinary cleanup End entry exceeds usize")?;
                if stage.phase == PhaseV1::EndCompleted {
                    if end != index {
                        return Err(
                            "ordinary cleanup completed End changed its own occurrence".into()
                        );
                    }
                    end_step = Some((index, stage.active));
                }
                if (end >= index && stage.phase != PhaseV1::EndCompleted)
                    || end_step != Some((end, stage.active))
                    || turn.is_some_and(|turn| turn != (stage.turn, stage.active))
                {
                    return Err("ordinary cleanup stage borrowed a foreign native End step".into());
                }
                if let Some((_, previous)) = last {
                    if (previous.turn, previous.active, previous.end_step)
                        == (stage.turn, stage.active, stage.end_step)
                    {
                        if !matches!(
                            (previous.phase, stage.phase),
                            (
                                PhaseV1::EndCompleted,
                                PhaseV1::Discard | PhaseV1::Expiration
                            ) | (PhaseV1::Discard, PhaseV1::Expiration)
                                | (
                                    PhaseV1::Expiration,
                                    PhaseV1::Priority | PhaseV1::GameOver | PhaseV1::Finished
                                )
                                | (
                                    PhaseV1::Priority,
                                    PhaseV1::Discard
                                        | PhaseV1::Expiration
                                        | PhaseV1::EndedByTimeStop
                                )
                        ) {
                            return Err("ordinary cleanup stage sequence changed".into());
                        }
                    } else if !terminal(previous.phase)
                        || !matches!(
                            stage.phase,
                            PhaseV1::Discard | PhaseV1::Expiration | PhaseV1::EndCompleted
                        )
                        || previous.end_step >= stage.end_step
                    {
                        return Err("ordinary cleanup did not close its preceding step".into());
                    }
                } else if !matches!(
                    stage.phase,
                    PhaseV1::Discard | PhaseV1::Expiration | PhaseV1::EndCompleted
                ) {
                    return Err("ordinary cleanup did not begin with its discard obligation".into());
                }
                if stage.phase == PhaseV1::EndedByTimeStop
                    && !matches!(
                        state.engine.event_history.get(index + 1),
                        Some(CommittedEvent::TimeStopJournalV1 { .. })
                    )
                {
                    return Err(
                        "ordinary cleanup end-turn replacement lost its native disposal outcome"
                            .into(),
                    );
                }
                last = Some((index, stage));
            }
            _ => {}
        }
    }
    Ok(last)
}
fn current_hand(state: &GameState, player: PlayerId) -> Vec<(ObjectId, u32)> {
    state.players[player.index()]
        .hand
        .iter()
        .map(|&id| (id, state.objects.get(id).zone_change_count))
        .collect()
}
fn discard_count(state: &GameState) -> usize {
    if event::niv_mizzet_visionary_removes_hand_limit_v1(state, state.active_player) {
        0
    } else {
        state.players[state.active_player.index()]
            .hand
            .len()
            .saturating_sub(7)
    }
}
fn last_end_step(state: &GameState) -> Option<usize> {
    let index = state
        .engine
        .event_history
        .iter()
        .enumerate()
        .rfind(|(index, _)| is_end_origin(state, *index, state.active_player))?
        .0;
    if !is_end_origin(state, index, state.active_player)
        || state
            .engine
            .event_history
            .iter()
            .skip(index + 1)
            .any(|event| matches!(event, CommittedEvent::LifeLossTurnBeganV1 { .. }))
    {
        return None;
    }
    Some(index)
}

fn is_end_origin(state: &GameState, index: usize, active: PlayerId) -> bool {
    match state.engine.event_history.get(index) {
        Some(CommittedEvent::BeginningEndStep { active_player, .. }) => *active_player == active,
        Some(CommittedEvent::OrdinaryCleanupStageV1 { stage }) => {
            stage.phase == PhaseV1::EndCompleted
                && stage.active == active
                && stage.turn == state.turn_key_v1()
                && usize::try_from(stage.end_step).ok() == Some(index)
        }
        _ => false,
    }
}

pub(crate) fn validate(state: &GameState) -> Result<(), String> {
    let latest = validate_stages(state)?;
    let Some(saved) = state.engine.ordinary_cleanup_v1.0.as_ref() else {
        // A direct low-level Cleanup setup has no native End-step entry. An
        // actual issued ordinary Cleanup cannot erase its transient owner.
        // Finished+None exists only inside the uninterrupted drive-to-next-step
        // loop. It cannot authorize an exposed continuation in this Cleanup.
        // GameOver retains its owner rather than reopening any continuation.
        if latest.is_some_and(|(_, stage)| {
            !terminal(stage.phase)
                || stage.phase == PhaseV1::GameOver
                || (stage.phase == PhaseV1::Finished
                    && state.step == Step::Cleanup
                    && (stage.turn, stage.active) == (state.turn_key_v1(), state.active_player)
                    && usize::try_from(stage.end_step).ok() == last_end_step(state))
        }) {
            return Err("ordinary cleanup current native stage was erased".into());
        }
        return Ok(());
    };
    if !latest.is_some_and(|(index, stage)| {
        u64::try_from(index).ok() == Some(saved.stage_index)
            && (stage.turn, stage.active, stage.end_step, stage.phase)
                == (saved.turn, saved.active, saved.end_step, saved.phase)
            && !matches!(stage.phase, PhaseV1::Finished | PhaseV1::EndedByTimeStop)
    }) {
        return Err("ordinary cleanup current phase differs from its native transition".into());
    }
    if !state.engine.time_stop_v1.is_empty()
        || saved.turn != state.turn_key_v1()
        || saved.active != state.active_player
        || state.step != Step::Cleanup
        || usize::try_from(saved.end_step).ok() != last_end_step(state)
        || !is_end_origin(state, saved.end_step as usize, saved.active)
    {
        return Err("ordinary cleanup changed its current turn or native end-step entry".into());
    }
    match saved.phase {
        PhaseV1::Discard => {
            let pending = state
                .engine
                .pending_discard
                .as_ref()
                .ok_or("ordinary cleanup discard was erased")?;
            if saved.hand != current_hand(state, saved.active)
                || pending.resume != DiscardResume::None
                || pending.player != saved.active
                || pending.count == 0
                || usize::try_from(pending.count).ok() != Some(discard_count(state))
                || state.engine.pending_effect.is_some()
                || state.engine.pending_cast.is_some()
                || state.engine.pending_activation.is_some()
                || !state.stack.is_empty()
            {
                return Err(
                    "ordinary cleanup discard lost its exact current hand obligation".into(),
                );
            }
        }
        PhaseV1::Expiration => {
            if !saved.hand.is_empty()
                || state.engine.pending_discard.is_some()
                || discard_count(state) != 0
                || !state.stack.is_empty()
                || state.engine.pending_effect.is_some()
                || state.engine.pending_cast.is_some()
                || state.engine.pending_activation.is_some()
                || state.engine.priority_passes != [false, false]
            {
                return Err("ordinary cleanup expiration preceded its hand discard".into());
            }
        }
        PhaseV1::Priority => {
            if !saved.hand.is_empty() {
                return Err("ordinary cleanup retained a completed hand binding".into());
            }
        }
        PhaseV1::Finished | PhaseV1::EndedByTimeStop | PhaseV1::EndCompleted => {
            return Err("ordinary cleanup retained a closed stage".into())
        }
        PhaseV1::GameOver => {
            if !saved.hand.is_empty() || check_game_over(state).is_none() {
                return Err("ordinary cleanup game-over stage changed".into());
            }
        }
    }
    Ok(())
}

/// The only new hidden references are current hand incarnations. They are
/// released immediately after the answer, so later unknown current slots can
/// still be sampled using the existing lawful slot permutation.
pub(crate) fn conflicts(state: &GameState, pool: &[ObjectId]) -> bool {
    state
        .engine
        .ordinary_cleanup_v1
        .0
        .as_ref()
        .is_some_and(|saved| {
            saved.hand.iter().any(|(id, generation)| {
                pool.contains(id) && state.objects.get(*id).zone_change_count == *generation
            })
        })
}
pub(crate) fn at_priority(state: &GameState) -> bool {
    state
        .engine
        .ordinary_cleanup_v1
        .0
        .as_ref()
        .is_some_and(|saved| saved.phase == PhaseV1::Priority)
}
pub(crate) fn owns_discard(state: &GameState, pending: &PendingDiscard) -> bool {
    state
        .engine
        .ordinary_cleanup_v1
        .0
        .as_ref()
        .is_some_and(|saved| saved.phase == PhaseV1::Discard)
        && pending.resume == DiscardResume::None
}
pub(crate) fn validate_discard(state: &GameState, pending: &PendingDiscard) -> Result<(), String> {
    validate(state)?;
    if !owns_discard(state, pending) || state.engine.pending_discard.as_ref() != Some(pending) {
        return Err("ordinary cleanup discard is not the current native decision".into());
    }
    Ok(())
}

pub(crate) fn begin(state: &mut GameState) -> Result<(), String> {
    if state.step != Step::End
        || !state.engine.ordinary_cleanup_v1.is_empty()
        || !state.engine.time_stop_v1.is_empty()
        || !state.stack.is_empty()
        || state.engine.priority_passes != [true, true]
        || state.engine.pending_discard.is_some()
        || !state.engine.pending_triggers.is_empty()
        || state.pending_legend_rule_v1.is_some()
        || state.engine.pending_effect.is_some()
        || state.engine.pending_cast.is_some()
        || state.engine.pending_activation.is_some()
        || state.engine.pending_spell_copy.is_some()
        || state.engine.pending_optional_cost.is_some()
        || state.engine.pending_optional_cost_sacrifice.is_some()
    {
        return Err("ordinary cleanup lost its actual completed End step".into());
    }
    let end_step = match last_end_step(state) {
        Some(index) => index,
        None if !cfg!(any(
            feature = "limited-fdn-fixtures",
            feature = "standard-magezero-fixtures"
        )) =>
        {
            // Default games retain their old history when cleanup is ordinary.
            // An actual Fiery replacement can be installed during End itself.
            // Bind its cleanup here, after the genuine empty-stack pass round,
            // without inventing an earlier BeginningEndStep event.
            let index = state.engine.event_history.len();
            let stage = StageV1 {
                turn: state.turn_key_v1(),
                active: state.active_player,
                end_step: u64::try_from(index)
                    .map_err(|_| "ordinary cleanup completion exceeds u64")?,
                phase: PhaseV1::EndCompleted,
            };
            state
                .engine
                .event_history
                .push(CommittedEvent::OrdinaryCleanupStageV1 { stage });
            index
        }
        None => return Err("ordinary cleanup has no actual beginning End step".into()),
    };
    state.engine.ordinary_cleanup_v1 = StateV1(Some(CleanupV1 {
        turn: state.turn_key_v1(),
        active: state.active_player,
        end_step: u64::try_from(end_step).map_err(|_| "ordinary cleanup entry exceeds u64")?,
        phase: PhaseV1::Expiration,
        stage_index: 0,
        hand: Vec::new(),
    }));
    state.step = Step::Cleanup;
    empty_mana(state);
    start_discard(state)
}
fn start_discard(state: &mut GameState) -> Result<(), String> {
    // The completed End/cleanup priority window belongs to the previous
    // boundary. Preserve the ordinary step-entry reset before a discard DTO
    // is exposed; this does not grant priority during CR 514.1/514.2.
    reset_priority(state);
    let count = discard_count(state);
    let player = state.active_player;
    if count > 0 {
        let count = u32::try_from(count).map_err(|_| "ordinary cleanup discard exceeds u32")?;
        let hand = current_hand(state, player);
        state
            .engine
            .ordinary_cleanup_v1
            .0
            .as_mut()
            .ok_or("ordinary cleanup is absent")?
            .hand = hand;
        state.engine.pending_discard = Some(PendingDiscard {
            player,
            count,
            resume: DiscardResume::None,
        });
        set_phase(state, PhaseV1::Discard)
    } else {
        set_phase(state, PhaseV1::Expiration)
    }
}
pub(crate) fn answer_discard(state: &mut GameState, chosen: Vec<ObjectId>) -> Result<(), String> {
    let pending = state
        .engine
        .pending_discard
        .as_ref()
        .ok_or("ordinary cleanup discard was erased")?;
    validate_discard(state, pending)?;
    if chosen.len() != pending.count as usize
        || chosen
            .iter()
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            != chosen.len()
        || chosen
            .iter()
            .any(|id| !state.players[pending.player.index()].hand.contains(id))
    {
        return Err("ordinary cleanup discard answer changed".into());
    }
    state.engine.pending_discard = None;
    for id in chosen {
        commit_discarded_card(state, id);
    }
    state
        .engine
        .ordinary_cleanup_v1
        .0
        .as_mut()
        .ok_or("ordinary cleanup is absent")?
        .hand
        .clear();
    set_phase(state, PhaseV1::Expiration)
}
pub(crate) fn drive(state: &mut GameState) -> Result<bool, String> {
    if state
        .engine
        .ordinary_cleanup_v1
        .0
        .as_ref()
        .is_none_or(|saved| saved.phase != PhaseV1::Expiration)
    {
        return Ok(false);
    }
    validate(state)?;
    expire_cleanup_effects_v1(state).map_err(|_| "ordinary cleanup expiration failed")?;
    let waiting = state.engine.pending_triggers.clone();
    let (waiting, sba_changed, triggered) =
        trigger::collect_and_process_with_sba_result_v1(state, waiting);
    state.engine.pending_triggers = waiting;
    if check_game_over(state).is_some() {
        set_phase(state, PhaseV1::GameOver)?;
    } else if sba_changed
        || triggered
        || state.pending_legend_rule_v1.is_some()
        || !state.engine.pending_triggers.is_empty()
    {
        set_phase(state, PhaseV1::Priority)?;
        reset_priority(state);
    } else {
        set_phase(state, PhaseV1::Finished)?;
        state.engine.ordinary_cleanup_v1 = StateV1::default();
    }
    Ok(true)
}
pub(crate) fn repeat(state: &mut GameState) -> Result<(), String> {
    validate(state)?;
    if !at_priority(state)
        || !state.stack.is_empty()
        || state.engine.priority_passes != [true, true]
        || !state.engine.pending_triggers.is_empty()
        || state.pending_legend_rule_v1.is_some()
    {
        return Err("ordinary repeated cleanup lost its completed priority window".into());
    }
    empty_mana(state);
    start_discard(state)
}
/// Called only by the exact native Time Stop resolution, after its original
/// full stack/source validation. This scalar closure grants no disposal authority.
pub(super) fn replaced_by_time_stop(state: &mut GameState) -> Result<(), String> {
    if state.engine.ordinary_cleanup_v1.is_empty() {
        return Ok(());
    }
    if !at_priority(state) {
        return Err("end-turn replaced an unfinished ordinary discard".into());
    }
    set_phase(state, PhaseV1::EndedByTimeStop)?;
    state.engine.ordinary_cleanup_v1 = StateV1::default();
    Ok(())
}
fn empty_mana(state: &mut GameState) {
    for player in &mut state.players {
        player.mana_pool = [0; 6];
        player.restricted_mana_pool.0.clear();
    }
}
