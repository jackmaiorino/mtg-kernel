//! Native end-turn disposal and cleanup. A journal is a replayable outcome,
//! never an instruction or a substitute for a live source contract.
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
            b"time-stop-cleanup/v1".hash(h);
            value.hash(h);
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CleanupV1 {
    disposal: u64,
    turn: u32,
    active: PlayerId,
    phase: PhaseV1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
enum PhaseV1 {
    BeforeCleanup,
    Discard,
    Expiration,
    Priority,
    GameOver,
}

/// Every input borrows only its actual earlier history prefix on restore.
/// The top history is absent from the bytes, avoiding recursive journals.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WorldV1 {
    history_count: u64,
    bytes: Vec<u8>,
    enclosing: crate::linked_aura_operation_v1::EndTurnReplayBorrowV1,
}
impl WorldV1 {
    fn capture(state: &GameState) -> Result<Self, String> {
        let mut original = state.clone();
        let history_count = u64::try_from(original.engine.event_history.len())
            .map_err(|_| "end-turn history exceeds u64")?;
        original.engine.event_history.clear();
        Ok(Self {
            history_count,
            bytes: serde_json::to_vec(&original).map_err(|e| e.to_string())?,
            enclosing: crate::linked_aura_operation_v1::capture_end_turn_replay_borrow_v1(state),
        })
    }
    fn restore(&self, state: &GameState, end: usize) -> Result<GameState, String> {
        let count = usize::try_from(self.history_count)
            .map_err(|_| "end-turn original prefix exceeds usize")?;
        if count > end {
            return Err("end-turn original prefix moved past its outcome".into());
        }
        let mut original: GameState =
            serde_json::from_slice(&self.bytes).map_err(|e| e.to_string())?;
        if !original.engine.event_history.is_empty() {
            return Err("end-turn world retained recursive history".into());
        }
        original.engine.event_history = state
            .engine
            .event_history
            .get(..count)
            .ok_or("end-turn original history is absent")?
            .to_vec()
            .into();
        Ok(original)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
enum OperationV1 {
    Dispose { item: Box<StackItem> },
    BeforeCleanup,
    Discard { chosen: Vec<ObjectId> },
    Expire,
    Repeat,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JournalV1 {
    original: WorldV1,
    operation: OperationV1,
    output: WorldV1,
}

fn journal(event: &CommittedEvent) -> Option<&JournalV1> {
    match event {
        CommittedEvent::TimeStopJournalV1 { journal } => Some(journal),
        _ => None,
    }
}

fn phase(state: &GameState) -> Result<&CleanupV1, String> {
    state
        .engine
        .time_stop_v1
        .0
        .as_ref()
        .ok_or_else(|| "end-turn cleanup owner is absent".into())
}

fn set_phase(state: &mut GameState, next: PhaseV1) -> Result<(), String> {
    state
        .engine
        .time_stop_v1
        .0
        .as_mut()
        .ok_or("end-turn cleanup owner is absent")?
        .phase = next;
    Ok(())
}

fn run(state: &mut GameState, operation: OperationV1) -> Result<(), String> {
    validate(state)?;
    let original = WorldV1::capture(state)?;
    let mut prepared = state.clone();
    apply(&mut prepared, &operation)?;
    if prepared.engine.halted.is_some() {
        return Err("end-turn native operation halted".into());
    }
    let output = WorldV1::capture(&prepared)?;
    prepared
        .engine
        .event_history
        .push(CommittedEvent::TimeStopJournalV1 {
            journal: Box::new(JournalV1 {
                original,
                operation,
                output,
            }),
        });
    *state = prepared;
    Ok(())
}

/// Validate all retained operations before their outcomes can close an older
/// printing's exact publication. Replays see only strictly earlier history.
pub(crate) fn validate(state: &GameState) -> Result<(), String> {
    if state
        .engine
        .event_log
        .iter()
        .any(|event| journal(event).is_some())
    {
        return Err("end-turn private journal entered trigger dispatch".into());
    }
    if !state
        .engine
        .event_history
        .has_any(&[crate::event_history_v1::EventKindV1::TimeStopJournalV1])
    {
        return if state.engine.time_stop_v1.is_empty() {
            Ok(())
        } else {
            Err("end-turn cleanup has no original native journal".into())
        };
    }
    let mut expected = StateV1::default();
    for (at, event) in state.engine.event_history.iter().enumerate() {
        let Some(saved) = journal(event) else {
            continue;
        };
        if usize::try_from(saved.output.history_count).ok() != Some(at)
            || saved.original.history_count > saved.output.history_count
        {
            return Err("end-turn original input/output index changed".into());
        }
        let mut replay = saved.original.restore(state, at)?;
        if replay.engine.time_stop_v1 != expected {
            return Err("end-turn operation changed its prior native phase".into());
        }
        let start = replay.engine.event_history.len();
        let expected_operation = match &saved.operation {
            OperationV1::Discard { chosen } => {
                crate::linked_aura_operation_v1::OperationV1::Step(Action::Discard(chosen.clone()))
            }
            _ => crate::linked_aura_operation_v1::OperationV1::Advance,
        };
        crate::linked_aura_operation_v1::with_end_turn_replay_borrow_v1(
            &mut replay,
            &saved.original.enclosing,
            &expected_operation,
            |replay| {
                apply(replay, &saved.operation)?;
                if replay.engine.halted.is_some()
                    || replay.engine.event_history.get(start..)
                        != state.engine.event_history.get(start..at)
                    || WorldV1::capture(replay)? != saved.output
                {
                    return Err("end-turn complete original operation replay differs".into());
                }
                Ok(())
            },
        )?;
        expected = replay.engine.time_stop_v1;
    }
    if state.engine.time_stop_v1 != expected {
        return Err("end-turn native cleanup phase was changed or erased".into());
    }
    if let Some(owner) = &expected.0 {
        if owner.turn != state.turn_key_v1() || owner.active != state.active_player {
            return Err("end-turn cleanup moved into another turn".into());
        }
        if !matches!(owner.phase, PhaseV1::BeforeCleanup | PhaseV1::GameOver)
            && state.step != Step::Cleanup
        {
            return Err("end-turn cleanup skipped its actual step".into());
        }
        if owner.phase == PhaseV1::GameOver && check_game_over(state).is_none() {
            return Err("end-turn terminal SBA loss was erased".into());
        }
        if owner.phase == PhaseV1::Discard {
            let pending = state
                .engine
                .pending_discard
                .as_ref()
                .ok_or("end-turn cleanup discard was erased")?;
            validate_discard_shape(state, pending)?;
            let (at, saved) = state
                .engine
                .event_history
                .iter()
                .enumerate()
                .rev()
                .find_map(|(at, event)| journal(event).map(|saved| (at, saved)))
                .ok_or("end-turn discard has no original transition")?;
            let output = saved.output.restore(state, at)?;
            if output.engine.pending_discard != state.engine.pending_discard
                || output.players[owner.active.index()].hand
                    != state.players[owner.active.index()].hand
            {
                return Err("end-turn discard changed its native original hand/obligation".into());
            }
        } else if owner.phase == PhaseV1::Expiration && state.engine.pending_discard.is_some() {
            return Err("end-turn expiration regained an unrelated discard".into());
        }
    }
    Ok(())
}

/// A truthful terminal disposition for an exact old publication. It grants no
/// right to execute the removed program and cannot close a reintroduced row.
pub(crate) fn disposed(state: &GameState, item: &StackItem) -> Result<bool, String> {
    if !state
        .engine
        .event_history
        .has_any(&[crate::event_history_v1::EventKindV1::TimeStopJournalV1])
    {
        return Ok(false);
    }
    let mut matches = 0usize;
    for (at, event) in state.engine.event_history.iter().enumerate() {
        let Some(saved) = journal(event) else {
            continue;
        };
        if !matches!(saved.operation, OperationV1::Dispose { .. }) {
            continue;
        }
        let input = saved.original.restore(state, at)?;
        matches += input.stack.iter().filter(|row| *row == item).count();
    }
    if matches == 0 {
        return Ok(false);
    }
    validate(state)?;
    if matches != 1
        || state
            .stack
            .iter()
            .any(|row| row.v4.stack_item_id == item.v4.stack_item_id)
    {
        return Err("end-turn disposed publication was duplicated or reintroduced".into());
    }
    Ok(true)
}

pub(crate) fn validate_enclosing_input(
    state: &GameState,
    original_stack: &[StackItem],
    original_history_len: usize,
    original_sha256: &str,
) -> Result<(), String> {
    let mut original = if original_stack == state.stack {
        state.clone()
    } else {
        let disposal = usize::try_from(phase(state)?.disposal)
            .map_err(|_| "end-turn disposal exceeds usize")?;
        let saved = state
            .engine
            .event_history
            .get(disposal)
            .and_then(journal)
            .ok_or("end-turn enclosing scope lost its original disposal")?;
        if !matches!(saved.operation, OperationV1::Dispose { .. }) {
            return Err("end-turn enclosing scope borrowed a cleanup transition".into());
        }
        let original = saved.original.restore(state, disposal)?;
        if original.stack != original_stack {
            return Err("end-turn enclosing scope changed its complete original stack".into());
        }
        original
    };
    if original_history_len > original.engine.event_history.len() {
        return Err("end-turn enclosing input history moved forward".into());
    }
    original.engine.event_history.truncate(original_history_len);
    if crate::linked_aura_operation_v1::original_sha256_for_end_turn_v1(&original)
        != original_sha256
    {
        return Err("end-turn enclosing original world/stack/history changed".into());
    }
    Ok(())
}

pub(crate) fn begin(state: &mut GameState, item: &StackItem) -> Result<(), String> {
    run(
        state,
        OperationV1::Dispose {
            item: Box::new(item.clone()),
        },
    )
}

/// Called before ordinary pending-trigger placement. Pre-cleanup SBA choices
/// keep their normal native decision path; no priority is created here.
pub(crate) fn drive(state: &mut GameState) -> Result<bool, String> {
    let Some(owner) = state.engine.time_stop_v1.0.as_ref() else {
        return Ok(false);
    };
    match owner.phase {
        PhaseV1::BeforeCleanup if state.pending_legend_rule_v1.is_none() => {
            run(state, OperationV1::BeforeCleanup)?;
            Ok(true)
        }
        PhaseV1::Expiration => {
            run(state, OperationV1::Expire)?;
            Ok(true)
        }
        _ => Ok(false),
    }
}

pub(crate) fn answer_discard(state: &mut GameState, chosen: Vec<ObjectId>) -> Result<(), String> {
    run(state, OperationV1::Discard { chosen })
}

pub(crate) fn at_priority(state: &GameState) -> bool {
    state
        .engine
        .time_stop_v1
        .0
        .as_ref()
        .is_some_and(|s| s.phase == PhaseV1::Priority)
}

/// Actual cleanup entry after an ending-turn operation. This is not an
/// EndStep occurrence and cannot authorize a beginning-of-end-step trigger.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CleanupBoundaryV1 {
    pub(crate) disposal: u64,
    pub(crate) entry: u64,
    pub(crate) turn: u32,
    pub(crate) active: PlayerId,
}

fn boundary_before(state: &GameState, before: usize) -> Result<Option<CleanupBoundaryV1>, String> {
    for (at, event) in state.engine.event_history[..before]
        .iter()
        .enumerate()
        .rev()
    {
        let Some(saved) = journal(event) else {
            continue;
        };
        if !matches!(
            saved.operation,
            OperationV1::BeforeCleanup | OperationV1::Repeat
        ) {
            continue;
        }
        let output = saved.output.restore(state, at)?;
        let Some(owner) = output.engine.time_stop_v1.0.as_ref() else {
            continue;
        };
        if output.step == Step::Cleanup && owner.phase != PhaseV1::BeforeCleanup {
            return Ok(Some(CleanupBoundaryV1 {
                disposal: owner.disposal,
                entry: u64::try_from(at).map_err(|_| "end-turn cleanup entry exceeds u64")?,
                turn: owner.turn,
                active: owner.active,
            }));
        }
    }
    Ok(None)
}

pub(crate) fn cleanup_boundary(state: &GameState) -> Result<Option<CleanupBoundaryV1>, String> {
    if state
        .engine
        .time_stop_v1
        .0
        .as_ref()
        .is_none_or(|owner| owner.phase != PhaseV1::Expiration)
    {
        return Ok(None);
    }
    validate(state)?;
    let boundary = boundary_before(state, state.engine.event_history.len())?
        .ok_or("end-turn cleanup lost its actual entry")?;
    if boundary.disposal != phase(state)?.disposal
        || boundary.turn != state.turn_key_v1()
        || boundary.active != state.active_player
        || state.step != Step::Cleanup
    {
        return Err("end-turn cleanup boundary changed its actual turn/owner".into());
    }
    Ok(Some(boundary))
}

pub(crate) fn validate_cleanup_boundary(
    state: &GameState,
    boundary: &CleanupBoundaryV1,
    before: u64,
) -> Result<(), String> {
    validate(state)?;
    let before = usize::try_from(before).map_err(|_| "end-turn cleanup boundary exceeds usize")?;
    if before > state.engine.event_history.len()
        || boundary_before(state, before)?.as_ref() != Some(boundary)
    {
        return Err("end-turn cleanup borrowed a foreign or stale native entry".into());
    }
    Ok(())
}

pub(crate) fn leave_cleanup(state: &mut GameState) -> Result<(), String> {
    run(state, OperationV1::Repeat)
}

fn apply(state: &mut GameState, operation: &OperationV1) -> Result<(), String> {
    match operation {
        OperationV1::Dispose { item } => dispose_stack(state, item),
        OperationV1::BeforeCleanup => before_cleanup(state),
        OperationV1::Discard { chosen } => discard(state, chosen),
        OperationV1::Expire => expire(state),
        OperationV1::Repeat => repeat(state),
    }
}

fn dispose_stack(state: &mut GameState, item: &StackItem) -> Result<(), String> {
    if state.stack.last() != Some(item)
        || item.kind != StackItemKind::Spell
        || !definition(state.objects.get(item.source).card_def)
        || item.inline_effect.is_some()
        || state.engine.pending_effect.is_some()
        || state.engine.pending_cast.is_some()
        || state.engine.pending_activation.is_some()
        || state.engine.pending_discard.is_some()
        || state.engine.pending_optional_cost.is_some()
        || state.engine.pending_optional_cost_sacrifice.is_some()
        || state.engine.pending_spell_copy.is_some()
        || !state.engine.pending_triggers.is_empty()
        || state.pending_legend_rule_v1.is_some()
        || !state.engine.event_log.is_empty()
        || state.engine.priority_passes != [true, true]
        || check_game_over(state).is_some()
        || state.engine.pending_land_play.is_some()
        || crate::london_mulligan_v1::has_pending(state)
    {
        return Err("end-turn resolution lost its exact native ready spell/window".into());
    }
    validate_original_owners(state)?;
    let rows = state.stack.clone();
    let mut ids = std::collections::BTreeSet::new();
    for row in &rows {
        if row.v4.stack_item_id == StackItemId::default() || !ids.insert(row.v4.stack_item_id) {
            return Err("end-turn stack identity is absent or duplicated".into());
        }
        validated_stack_item_target_spec(row, state)?;
    }
    // Physical spell cards enter exile together. Ability rows are exiled as
    // stack objects, not as their unrelated battlefield/source cards. Copies
    // also enter exile before the immediate SBA check makes them cease.
    let proposals = rows
        .iter()
        .filter(|row| row.kind == StackItemKind::Spell)
        .map(|row| ProposedEvent::zone_change(row.source, Zone::Exile))
        .collect();
    event::propose_and_commit_batch(state, proposals);
    if state.engine.halted.is_some() {
        return Err("end-turn stack exile failed".into());
    }
    state.stack.clear();
    // A real end-turn resolution replaces any ordinary cleanup priority window.
    ordinary_cleanup_v1::replaced_by_time_stop(state)?;
    let disposal = u64::try_from(state.engine.event_history.len())
        .map_err(|_| "end-turn disposal index exceeds u64")?;
    state.engine.time_stop_v1 = StateV1(Some(CleanupV1 {
        disposal,
        turn: state.turn_key_v1(),
        active: state.active_player,
        phase: PhaseV1::BeforeCleanup,
    }));
    Ok(())
}

fn validate_original_owners(state: &GameState) -> Result<(), String> {
    crate::drake_hatcher_v1::custody::validate(state)?;
    crate::fake_your_own_death_v50::spell_custody::validate_retained_grant_history(state)?;
    crate::fake_your_own_death_v50::death_custody::validate_retained_owners(state)?;
    crate::bloodthirsty_conqueror_v49::validate(state)?;
    crate::snack_wide_life_v59::validate(state)?;
    crate::fiendish_panda_v1::validate_state(state)?;
    crate::high_society_hunter_custody_v45::validate(state)?;
    crate::faebloom_trick_v43::validate_state(state)?;
    crate::ajani_full_v42::lifecycle::validate_state(state)?;
    crate::krenko_v1::validate_state(state)?;
    crate::fishing_pole_v39::lifecycle::validate_state(state)?;
    crate::ravenous_amulet_v1::validate_state(state)?;
    crate::source_power_activation_v1::validate_state(state)?;
    crate::raid_entry_v1::validate_state(state)?;
    crate::gorehorn_damage_v1::validate_state(state)?;
    crate::fixed_source_damage_v3::validate_state(state)?;
    crate::equipment_damage_source_v4::validate_state(state)?;
    crate::zul_ashur_v1::validate_state(state)?;
    effect::frenzied_goblin_resolution_v1::validate(state)?;
    effect::validate_inspiration_resolution(state)?;
    effect::crystal_prevention_v1::validate(state)?;
    effect::validate_vampire_gourmand_state(state)?;
    crate::arahbo_v1::validate_state(state)?;
    crate::garruks_uprising_v1::validate_state(state)?;
    crate::banishing_light_v1::validate_state(state)?;
    crate::authority_of_the_consuls_v1::validate_state(state)?;
    crate::cerberus_capture_v1::validate_state(state)?;
    crate::wildwood_capture_v1::validate_state(state)?;
    crate::ashroot_animist_v1::validate_state(state)?;
    crate::stromkirk_bloodthief_v1::validate_state(state)?;
    crate::extra_turn_v1::validate(state)?;
    event::validate_copied_token_abilities_v1(state)?;
    Ok(())
}

fn before_cleanup(state: &mut GameState) -> Result<(), String> {
    if phase(state)?.phase != PhaseV1::BeforeCleanup || state.pending_legend_rule_v1.is_some() {
        return Err("end-turn pre-cleanup SBA phase changed".into());
    }
    // CR 724.1b: spell copies entered exile with the rest of the stack;
    // CR 704 now makes those noncard objects cease before any priority.
    let disposal =
        usize::try_from(phase(state)?.disposal).map_err(|_| "end-turn disposal exceeds usize")?;
    let saved = state
        .engine
        .event_history
        .get(disposal)
        .and_then(journal)
        .ok_or("end-turn original disposal is absent")?;
    let original = saved.original.restore(state, disposal)?;
    for row in original
        .stack
        .iter()
        .filter(|row| row.kind == StackItemKind::Spell && row.is_copy)
    {
        event::cease_to_exist(state, row.source);
    }
    // No priority or placement, but retain newly created trigger captures.
    trigger::sba_fixed_point(state);
    let new = trigger::capture_triggers_without_sba(state);
    state.engine.pending_triggers.extend(new);
    if check_game_over(state).is_some() {
        return set_phase(state, PhaseV1::GameOver);
    }
    if state.pending_legend_rule_v1.is_some() {
        return Ok(());
    }
    state.engine.combat.reset_preserving_rules();
    state.step = Step::Cleanup;
    empty_mana(state);
    start_discard(state)
}

fn start_discard(state: &mut GameState) -> Result<(), String> {
    let player = state.active_player;
    let count = if event::niv_mizzet_visionary_removes_hand_limit_v1(state, player) {
        0
    } else {
        state.players[player.index()].hand.len().saturating_sub(7)
    };
    if count > 0 {
        let count = u32::try_from(count).map_err(|_| "end-turn cleanup discard exceeds u32")?;
        state.engine.pending_discard = Some(PendingDiscard {
            player,
            count,
            resume: DiscardResume::TimeStopCleanupV1,
        });
        set_phase(state, PhaseV1::Discard)
    } else {
        set_phase(state, PhaseV1::Expiration)
    }
}

fn discard(state: &mut GameState, chosen: &[ObjectId]) -> Result<(), String> {
    if phase(state)?.phase != PhaseV1::Discard {
        return Err("end-turn cleanup discard is not pending".into());
    }
    let pending = state
        .engine
        .pending_discard
        .as_ref()
        .ok_or("end-turn discard was erased")?;
    validate_discard_shape(state, pending)?;
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
        return Err("end-turn discard answer changed its original hand obligation".into());
    }
    state.engine.pending_discard = None;
    for &id in chosen {
        commit_discarded_card(state, id);
    }
    set_phase(state, PhaseV1::Expiration)
}

pub(crate) fn validate_discard(state: &GameState, pending: &PendingDiscard) -> Result<(), String> {
    validate(state)?;
    validate_discard_shape(state, pending)
}

fn validate_discard_shape(state: &GameState, pending: &PendingDiscard) -> Result<(), String> {
    let owner = phase(state)?;
    if owner.phase != PhaseV1::Discard
        || pending.resume != DiscardResume::TimeStopCleanupV1
        || pending.player != owner.active
        || state.engine.pending_discard.as_ref() != Some(pending)
        || pending.count == 0
        || usize::try_from(pending.count).ok()
            != state.players[pending.player.index()]
                .hand
                .len()
                .checked_sub(7)
        || event::niv_mizzet_visionary_removes_hand_limit_v1(state, pending.player)
    {
        return Err("end-turn original cleanup discard binding changed".into());
    }
    Ok(())
}

fn expire(state: &mut GameState) -> Result<(), String> {
    if phase(state)?.phase != PhaseV1::Expiration || state.engine.pending_discard.is_some() {
        return Err("end-turn expiration preceded its discard".into());
    }
    expire_cleanup_effects_v1(state).map_err(|_| "end-turn cleanup expiration failed")?;
    let waiting = state.engine.pending_triggers.clone();
    let (waiting, sba_changed, triggered) =
        trigger::collect_and_process_with_sba_result_v1(state, waiting);
    state.engine.pending_triggers = waiting;
    if check_game_over(state).is_some() {
        return set_phase(state, PhaseV1::GameOver);
    }
    if sba_changed
        || triggered
        || state.pending_legend_rule_v1.is_some()
        || !state.engine.pending_triggers.is_empty()
    {
        set_phase(state, PhaseV1::Priority)?;
        reset_priority(state);
        Ok(())
    } else {
        finish(state)
    }
}

fn repeat(state: &mut GameState) -> Result<(), String> {
    if phase(state)?.phase != PhaseV1::Priority
        || !state.stack.is_empty()
        || state.engine.priority_passes != [true, true]
        || !state.engine.pending_triggers.is_empty()
    {
        return Err("end-turn repeated cleanup lost its completed priority window".into());
    }
    empty_mana(state);
    start_discard(state)
}

fn finish(state: &mut GameState) -> Result<(), String> {
    if phase(state)?.phase != PhaseV1::Expiration || state.engine.pending_discard.is_some() {
        return Err("end-turn cleanup was skipped".into());
    }
    state.engine.time_stop_v1 = StateV1::default();
    // Let the ordinary cleanup-to-next-turn code choose extra turns and run
    // the real Untap entry exactly once, without fabricating an EndStep.
    Ok(())
}

fn empty_mana(state: &mut GameState) {
    for player in &mut state.players {
        player.mana_pool = [0; 6];
        player.restricted_mana_pool.0.clear();
    }
}

pub(crate) fn definition(id: u16) -> bool {
    cfg!(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    )) && id == 486
        && card_def::CARD_DEFS.get(usize::from(id)).is_some_and(|d| {
            d.name == "Time Stop"
                && d.capability == card_def::CardCapability::Partial
                && d.cost
                    == crate::mana::Cost {
                        generic: 4,
                        pips: &[
                            crate::mana::Pip::Colored(crate::mana::ManaColor::U),
                            crate::mana::Pip::Colored(crate::mana::ManaColor::U),
                        ],
                        x_count: 0,
                    }
                && d.generic_cost_reduction.is_none()
                && d.ward_cost.is_none()
                && !d.spell_cannot_be_countered
                && !d.records_mana_spent
                && d.equipment.is_none()
                && d.types == [CardType::Instant]
                && d.subtypes.is_empty()
                && d.supertypes.is_empty()
                && d.power.is_none()
                && d.toughness.is_none()
                && d.starting_loyalty.is_none()
                && d.enters_with_plus_one_counters.is_none()
                && d.controlled_counter_keyword.is_none()
                && d.game_outcome_protection_v1.is_none()
                && d.controlled_creature_grant.is_none()
                && !d.is_land
                && d.produces_mana.is_empty()
                && d.colors == [crate::mana::ManaColor::U]
                && d.target_spec == TargetSpec::None
                && d.keywords == card_def::Keywords::NONE
                && (d.spell_effect)()
                    == Some(effect::EffectOp::StandardV1(
                        crate::standard_cards_v1::StandardOpV1::EndTurnV1,
                    ))
                && (d.mana_ability)().is_none()
                && d.alt_cost.is_none()
                && d.kicker_cost.is_none()
                && d.additional_cost.is_none()
                && d.flashback.is_none()
                && d.activated_abilities.is_empty()
                && d.plot_cost.is_none()
                && d.madness_cost.is_none()
                && d.mode2.is_none()
                && d.mode3.is_none()
                && d.additional_modes.is_empty()
                && d.kicked_target_spec.is_none()
                && !d.is_token
                && d.escape.is_none()
                && d.mana_ability_choices.is_empty()
                && !d.enters_battlefield_tapped
                && d.mana_ability_def.is_none()
                && d.minimum_blockers == 1
                && d.omen.is_none()
                && d.mana_value == 6
                && !d.mana_ability_includes_chosen_color
                && d.as_enters_choose_color_other_than.is_none()
                && d.additional_mana_abilities.is_empty()
                && d.object_name == d.name
                && d.enters_battlefield_tapped_unless.is_none()
                && d.attachment.is_none()
                && d.transform_face.is_none()
                && d.saga.is_none()
                && d.optional_additional_cost.is_none()
                && !d.changeling
                && d.bestow.is_none()
                && !d.delve
                && d.adventure.is_none()
                && !d.cant_be_blocked_by_monarchs_creatures
                && d.conditional_tap_yield.is_none()
                && d.enters_tapped_unless_controller.is_none()
                && d.additional_mana_ability_conditions.is_empty()
                && d.restricted_mana_abilities.is_empty()
                && d.animation.is_none()
                && d.activated_ability_generic_reductions.is_empty()
                && d.combat_static_rules == card_def::CombatStaticRulesDef::NONE
                && d.as_enters_color_choice_v1.is_none()
                && d.chosen_color_creature_boost_v1.is_none()
                && d.damage_replacement_v1.is_none()
                && d.token_counter_replacement_v48.is_none()
                && d.starting_life_self_bonuses_v1.is_empty()
                && d.source_type_hexproof_v1.is_none()
                && d.other_controlled_nontoken_death_v1.is_none()
                && d.creature_type_entry_v1.is_none()
                && trigger::triggers_for(id).is_empty()
        })
}

/// Frozen consumers cannot omit an actual public end-turn outcome. Hidden
/// hands/libraries and face-down identities do not participate in selection.
pub(crate) fn refuse_legacy(state: &GameState) -> Result<(), String> {
    if !state.engine.time_stop_v1.is_empty()
        || state
            .engine
            .event_history
            .has_any(&[crate::event_history_v1::EventKindV1::TimeStopJournalV1])
        || state.objects.iter().any(|(_, object)| {
            matches!(
                object.zone,
                Zone::Battlefield | Zone::Stack | Zone::Graveyard | Zone::Exile
            ) && object.v4.face_down_v1.is_none()
                && definition(object.card_def)
        })
    {
        Err("Time Stop requires a complete end-turn consumer".into())
    } else {
        Ok(())
    }
}

/// Current-reference admission for the V68 whole-object sampler. Factual
/// original/output worlds remain immutable and are fully replay-validated;
/// their private former hand/library membership never filters a permutation.
pub(crate) fn sampler_conflicts_v68(state: &GameState, pool: &[ObjectId]) -> Result<bool, String> {
    validate(state)?;
    if state.engine.time_stop_v1.0.as_ref().is_some_and(|owner| {
        owner.phase == PhaseV1::Discard
            && state.players[owner.active.index()]
                .hand
                .iter()
                .any(|id| pool.contains(id))
    }) {
        return Ok(true);
    }
    for (at, event) in state.engine.event_history.iter().enumerate() {
        let Some(saved) = journal(event) else {
            continue;
        };
        if !matches!(saved.operation, OperationV1::Dispose { .. }) {
            continue;
        }
        let original = saved.original.restore(state, at)?;
        // before_cleanup reads precisely these original public virtual IDs.
        // They cannot enter the ordinary physical noncopy hidden-slot pool.
        if original.stack.iter().any(|row| {
            row.kind == StackItemKind::Spell && row.is_copy && pool.contains(&row.source)
        }) {
            return Ok(true);
        }
    }
    Ok(false)
}

#[derive(Clone, Copy)]
enum PublicScopeV69 {
    EndTurnV68,
    GenesisWaveV69,
    VivienV70,
    MuldrothaV71,
    TinybonesV72,
}
pub(crate) fn public_facts_v68(
    state: &GameState,
    viewer: PlayerId,
) -> Result<crate::time_stop_public_v68::FactsV68, String> {
    public_facts_for_scope_v69(state, viewer, PublicScopeV69::EndTurnV68)
}
pub(crate) fn public_facts_genesis_wave_v69(
    state: &GameState,
    viewer: PlayerId,
) -> Result<crate::time_stop_public_v68::FactsV68, String> {
    public_facts_for_scope_v69(state, viewer, PublicScopeV69::GenesisWaveV69)
}
pub(crate) fn public_facts_vivien_v70(
    state: &GameState,
    viewer: PlayerId,
) -> Result<crate::time_stop_public_v68::FactsV68, String> {
    public_facts_for_scope_v69(state, viewer, PublicScopeV69::VivienV70)
}
pub(crate) fn public_facts_muldrotha_v71(
    state: &GameState,
    viewer: PlayerId,
) -> Result<crate::time_stop_public_v68::FactsV68, String> {
    public_facts_for_scope_v69(state, viewer, PublicScopeV69::MuldrothaV71)
}
pub(crate) fn public_facts_tinybones_v72(
    state: &GameState,
    viewer: PlayerId,
) -> Result<crate::time_stop_public_v68::FactsV68, String> {
    public_facts_for_scope_v69(state, viewer, PublicScopeV69::TinybonesV72)
}
fn public_facts_for_scope_v69(
    state: &GameState,
    viewer: PlayerId,
    scope: PublicScopeV69,
) -> Result<crate::time_stop_public_v68::FactsV68, String> {
    use crate::time_stop_public_v68::{CleanupV68, DisposalV68, DisposedV68, FactsV68, PhaseV68};
    validate(state)?;
    ordinary_cleanup_v1::validate(state)?;
    let mut disposals = Vec::new();
    let mut compatibility_terminals = Vec::new();
    let mut indices = Vec::new();
    for (index, event) in state
        .engine
        .event_history
        .scan_for_v1(&[crate::event_history_v1::EventKindV1::TimeStopJournalV1])
        .iter()
        .enumerate()
    {
        let Some(saved) = journal(event) else {
            continue;
        };
        let OperationV1::Dispose { item } = &saved.operation else {
            continue;
        };
        let original = saved.original.restore(state, index)?;
        // Scope selection for an archived public event cannot depend on an
        // old private hand, known library card, or face-down look permission.
        // The row projector below still applies original public visibility.
        match scope {
            PublicScopeV69::EndTurnV68 => {
                crate::end_turn_observation_v68::require_supported(&original, None)?
            }
            PublicScopeV69::MuldrothaV71 => {
                crate::muldrotha_observation_v71::require_supported(&original, None)?
            }
            PublicScopeV69::TinybonesV72 => {
                crate::tinybones_observation_v72::require_supported(&original, None)?
            }
            PublicScopeV69::VivienV70 => {
                crate::vivien_observation_v70::require_supported(&original, None)?
            }
            PublicScopeV69::GenesisWaveV69 => {
                crate::genesis_wave_observation_v69::require_supported(&original, None)?
            }
        }
        for (position, row) in original.stack.iter().enumerate() {
            for target in compatibility_targets_v68(&original, row)? {
                compatibility_terminals.push(
                    crate::time_stop_public_v68::CompatibilityTerminalV68 {
                        target,
                        disposal_occurrence: u32::try_from(disposals.len())
                            .map_err(|_| "V68 disposal count exceeds u32")?,
                        original_stack_position: u32::try_from(position)
                            .map_err(|_| "V68 original stack position exceeds u32")?,
                        outcome: crate::time_stop_public_v68::TerminalOutcomeV68::Disposed,
                    },
                );
            }
        }
        let rows = original
            .stack
            .iter()
            .enumerate()
            .map(|(position, row)| {
                let position = u32::try_from(position)
                    .map_err(|_| "V68 original stack position exceeds u32")?;
                Ok(DisposedV68 {
                    original_stack_position: position,
                    original: match scope {
                        PublicScopeV69::EndTurnV68 => crate::rl::stack_item_public_end_turn_v68(
                            &original, viewer, position, row,
                        ),
                        PublicScopeV69::MuldrothaV71 => crate::rl::stack_item_public_muldrotha_v71(
                            &original, viewer, position, row,
                        ),
                        PublicScopeV69::TinybonesV72 => crate::rl::stack_item_public_tinybones_v72(
                            &original, viewer, position, row,
                        ),
                        PublicScopeV69::VivienV70 => crate::rl::stack_item_public_vivien_v70(
                            &original, viewer, position, row,
                        ),
                        PublicScopeV69::GenesisWaveV69 => {
                            crate::rl::stack_item_public_genesis_wave_v69(
                                &original, viewer, position, row,
                            )
                        }
                    }
                    .map_err(|e| e.0)?,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        let top = original
            .stack
            .last()
            .ok_or("V68 original ending stack is empty")?;
        if top != item.as_ref() {
            return Err("V68 original ending spell changed".into());
        }
        indices.push(u64::try_from(index).map_err(|_| "V68 disposal position exceeds u64")?);
        disposals.push(DisposalV68 {
            occurrence: u32::try_from(disposals.len())
                .map_err(|_| "V68 disposal count exceeds u32")?,
            ending_spell_position: u32::try_from(rows.len() - 1)
                .map_err(|_| "V68 ending spell position exceeds u32")?,
            original_step: original.step,
            original_active_player: original.active_player.into(),
            disposed_rows: rows,
        });
    }
    let cleanup = state
        .engine
        .time_stop_v1
        .0
        .as_ref()
        .map(|owner| -> Result<CleanupV68, String> {
            let occurrence = indices
                .iter()
                .position(|index| *index == owner.disposal)
                .ok_or("V68 cleanup lost its original disposal")?;
            Ok(CleanupV68 {
                disposal_occurrence: u32::try_from(occurrence)
                    .map_err(|_| "V68 cleanup occurrence exceeds u32")?,
                turn: owner.turn,
                active_player: owner.active.into(),
                phase: match owner.phase {
                    PhaseV1::BeforeCleanup => PhaseV68::BeforeCleanup,
                    PhaseV1::Discard => PhaseV68::Discard,
                    PhaseV1::Expiration => PhaseV68::Expiration,
                    PhaseV1::Priority => PhaseV68::Priority,
                    PhaseV1::GameOver => PhaseV68::GameOver,
                },
                discard_count: if owner.phase == PhaseV1::Discard {
                    Some(
                        state
                            .engine
                            .pending_discard
                            .as_ref()
                            .ok_or("V68 native cleanup discard erased")?
                            .count,
                    )
                } else {
                    None
                },
            })
        })
        .transpose()?;
    let visible_printings = state.objects.iter().filter_map(|(id, _)| crate::optional_trigger_public_v1::reference(state, id, Some(viewer)))
        .filter(|reference| matches!(reference.identity, crate::optional_trigger_public_v1::IdentityV1::Known { card_id } if card_id == 486)).collect();
    let value = FactsV68 {
        schema: crate::time_stop_public_v68::SCHEMA.into(),
        producer_sha256: crate::time_stop_public_v68::producer_sha256(),
        viewer: viewer.into(),
        meaning: crate::rules_meaning_time_stop_v68::printed_meaning()?,
        visible_printings,
        disposals,
        compatibility_terminals,
        cleanup,
    };
    crate::time_stop_public_v68::validate(&value)?;
    Ok(value)
}

/// Explicit source identity for the changed end-turn execution and closure
/// paths. No world, capability result or runtime validation is cached here.
// Match complete original publication values under the already replay-validated
// disposal. The returned role is only a public compatibility-row ordinal.
fn compatibility_targets_v68(
    original: &GameState,
    item: &StackItem,
) -> Result<Vec<crate::time_stop_public_v68::CompatibilityTargetV68>, String> {
    use crate::ajani_full_v42::lifecycle::JournalV42 as A;
    use crate::drake_hatcher_v1::custody::{journal as drake, JournalV1 as D};
    use crate::faebloom_trick_v43::JournalV43 as F;
    use crate::fishing_pole_v39::lifecycle::JournalV39 as P;
    use crate::time_stop_public_v68::CompatibilityTargetV68 as T;
    let h = &original.engine.event_history;
    let mut out = Vec::new();
    let ordinal = |value: usize| {
        u32::try_from(value).map_err(|_| "V68 compatibility ordinal exceeds u32".to_string())
    };
    let (mut dc, mut da, mut ra, mut aa, mut po, mut fs, mut fr) =
        (0usize, 0usize, 0usize, 0usize, 0usize, 0usize, 0usize);
    for (at, event) in h.iter().enumerate() {
        if let Some(record) = drake(event) {
            match &record.value {
                D::Captured { .. } => {
                    if h.iter().filter_map(drake).any(|r| matches!(&r.value,D::Published{capture,item:owned} if *capture==record.index && owned.as_ref()==item)) {
                    out.push(T::DrakeCombat{occurrence:ordinal(dc)?});
                }
                    dc += 1;
                }
                D::Paid { item: owned, .. } => {
                    if owned.as_ref() == item {
                        out.push(T::DrakeActivation {
                            occurrence: ordinal(da)?,
                        });
                    }
                    da += 1;
                }
                _ => {}
            }
        }
        match event {
            CommittedEvent::RavenousPaymentV1 { paid } => {
                if paid.item.as_ref() == item {
                    out.push(T::RavenousActivation {
                        occurrence: ordinal(ra)?,
                    });
                }
                ra += 1;
            }
            CommittedEvent::AjaniJournalV42 { journal } => {
                if let A::Paid { item: owned, .. } = journal.as_ref() {
                    if owned.as_ref() == item {
                        out.push(T::AjaniActivation {
                            position: ordinal(aa)?,
                        });
                    }
                    aa += 1;
                }
            }
            CommittedEvent::FishingPoleJournalV39 { journal } => {
                if matches!(
                    journal.as_ref(),
                    P::PaymentInput { .. } | P::Captured { .. }
                ) {
                    if h.iter().any(|e| matches!(e,CommittedEvent::FishingPoleJournalV39{journal} if matches!(journal.as_ref(),P::Published{origin,item:owned}if *origin==at as u64&&owned.as_ref()==item))){out.push(T::FishingOperation{position:ordinal(po)?});}
                    po += 1;
                }
            }
            CommittedEvent::FaebloomJournalV43 { journal } => match journal.as_ref() {
                F::Published { item: owned } => {
                    if owned.as_ref() == item {
                        out.push(T::FaebloomSpell {
                            ordinal: ordinal(fs)?,
                        });
                    }
                    fs += 1;
                }
                F::ReflexiveCaptured { .. } => {
                    if h.iter().any(|e| matches!(e,CommittedEvent::FaebloomJournalV43{journal} if matches!(journal.as_ref(),F::TriggerPublished{capture,item:owned,..}if *capture==at as u64&&owned.as_ref()==item))){out.push(T::FaebloomReflexive{ordinal:ordinal(fr)?});}
                    fr += 1;
                }
                _ => {}
            },
            _ => {}
        }
    }
    Ok(out)
}

pub(crate) fn source_sha256_v1() -> &'static str {
    static VALUE: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    VALUE
        .get_or_init(|| {
            use sha2::{Digest, Sha256};
            let mut hash = Sha256::new();
            hash.update(b"time-stop-native-semantic-source/v1");
            for (name, source) in [
                (
                    "data/limited/fdn_v1/cards_v1.json",
                    include_str!("../../../data/limited/fdn_v1/cards_v1.json"),
                ),
                ("mtg-kernel/build.rs", include_str!("../../build.rs")),
                (
                    "mtg-kernel/src/ajani_full_v42/lifecycle.rs",
                    include_str!("../ajani_full_v42/lifecycle.rs"),
                ),
                (
                    "mtg-kernel/src/arahbo_v1.rs",
                    include_str!("../arahbo_v1.rs"),
                ),
                (
                    "mtg-kernel/src/ashroot_dispatch_v2.rs",
                    include_str!("../ashroot_dispatch_v2.rs"),
                ),
                (
                    "mtg-kernel/src/authority_of_the_consuls_v1.rs",
                    include_str!("../authority_of_the_consuls_v1.rs"),
                ),
                (
                    "mtg-kernel/src/banishing_light_v1.rs",
                    include_str!("../banishing_light_v1.rs"),
                ),
                (
                    "mtg-kernel/src/bloodthirsty_conqueror_v49/custody.rs",
                    include_str!("../bloodthirsty_conqueror_v49/custody.rs"),
                ),
                (
                    "mtg-kernel/src/cerberus_capture_v1.rs",
                    include_str!("../cerberus_capture_v1.rs"),
                ),
                (
                    "mtg-kernel/src/drake_hatcher_v1/custody.rs",
                    include_str!("../drake_hatcher_v1/custody.rs"),
                ),
                (
                    "mtg-kernel/src/effect/frenzied_goblin_resolution_v1.rs",
                    include_str!("../effect/frenzied_goblin_resolution_v1.rs"),
                ),
                ("mtg-kernel/src/engine.rs", include_str!("../engine.rs")),
                (
                    "mtg-kernel/src/engine/time_stop_v1.rs",
                    include_str!("time_stop_v1.rs"),
                ),
                (
                    "mtg-kernel/src/equipment_damage_source_v4.rs",
                    include_str!("../equipment_damage_source_v4.rs"),
                ),
                ("mtg-kernel/src/event.rs", include_str!("../event.rs")),
                (
                    "mtg-kernel/src/event_history_v1.rs",
                    include_str!("../event_history_v1.rs"),
                ),
                (
                    "mtg-kernel/src/extra_turn_v1.rs",
                    include_str!("../extra_turn_v1.rs"),
                ),
                (
                    "mtg-kernel/src/faebloom_trick_v43.rs",
                    include_str!("../faebloom_trick_v43.rs"),
                ),
                (
                    "mtg-kernel/src/fake_your_own_death_v50/death_custody.rs",
                    include_str!("../fake_your_own_death_v50/death_custody.rs"),
                ),
                (
                    "mtg-kernel/src/fiendish_panda_v1/custody.rs",
                    include_str!("../fiendish_panda_v1/custody.rs"),
                ),
                (
                    "mtg-kernel/src/fishing_pole_v39/lifecycle.rs",
                    include_str!("../fishing_pole_v39/lifecycle.rs"),
                ),
                (
                    "mtg-kernel/src/fixed_source_damage_v3.rs",
                    include_str!("../fixed_source_damage_v3.rs"),
                ),
                (
                    "mtg-kernel/src/garruks_uprising_v1.rs",
                    include_str!("../garruks_uprising_v1.rs"),
                ),
                (
                    "mtg-kernel/src/gorehorn_damage_v1.rs",
                    include_str!("../gorehorn_damage_v1.rs"),
                ),
                (
                    "mtg-kernel/src/high_society_hunter_custody_v45.rs",
                    include_str!("../high_society_hunter_custody_v45.rs"),
                ),
                (
                    "mtg-kernel/src/krenko_v1.rs",
                    include_str!("../krenko_v1.rs"),
                ),
                (
                    "mtg-kernel/src/librarian_activation_v1.rs",
                    include_str!("../librarian_activation_v1.rs"),
                ),
                (
                    "mtg-kernel/src/linked_aura_operation_v1.rs",
                    include_str!("../linked_aura_operation_v1.rs"),
                ),
                (
                    "mtg-kernel/src/raid_entry_custody_v1.rs",
                    include_str!("../raid_entry_custody_v1.rs"),
                ),
                (
                    "mtg-kernel/src/ravenous_amulet_v1.rs",
                    include_str!("../ravenous_amulet_v1.rs"),
                ),
                ("mtg-kernel/src/rl.rs", include_str!("../rl.rs")),
                (
                    "mtg-kernel/src/rl_session/flat_action_v4/search_state/effect_refs.rs",
                    include_str!("../rl_session/flat_action_v4/search_state/effect_refs.rs"),
                ),
                (
                    "mtg-kernel/src/snack_wide_life_v59/custody.rs",
                    include_str!("../snack_wide_life_v59/custody.rs"),
                ),
                (
                    "mtg-kernel/src/source_power_activation_v1.rs",
                    include_str!("../source_power_activation_v1.rs"),
                ),
                (
                    "mtg-kernel/src/standard_cards_v1.rs",
                    include_str!("../standard_cards_v1.rs"),
                ),
                (
                    "mtg-kernel/src/stromkirk_dispatch_v1.rs",
                    include_str!("../stromkirk_dispatch_v1.rs"),
                ),
                ("mtg-kernel/src/trigger.rs", include_str!("../trigger.rs")),
                (
                    "mtg-kernel/src/vampire_gourmand_dispatch_v2.rs",
                    include_str!("../vampire_gourmand_dispatch_v2.rs"),
                ),
                (
                    "mtg-kernel/src/wildwood_capture_v1.rs",
                    include_str!("../wildwood_capture_v1.rs"),
                ),
                (
                    "mtg-kernel/src/zul_ashur_v1.rs",
                    include_str!("../zul_ashur_v1.rs"),
                ),
                (
                    "mtg-kernel/src/engine/ordinary_cleanup_v1.rs",
                    include_str!("ordinary_cleanup_v1.rs"),
                ),
                ("mtg-kernel/Cargo.toml", include_str!("../../Cargo.toml")),
                (
                    "mtg-kernel/src/akroma_family_public_v40.rs",
                    include_str!("../akroma_family_public_v40.rs"),
                ),
                (
                    "mtg-kernel/src/bin/kernel_limited_end_turn_env_v9.rs",
                    include_str!("../bin/kernel_limited_end_turn_env_v9.rs"),
                ),
                (
                    "mtg-kernel/src/current_mixed_policy_v1.rs",
                    include_str!("../current_mixed_policy_v1.rs"),
                ),
                (
                    "mtg-kernel/src/current_mixed_policy_v1/end_turn.rs",
                    include_str!("../current_mixed_policy_v1/end_turn.rs"),
                ),
                (
                    "mtg-kernel/src/current_mixed_policy_v1/normalize.rs",
                    include_str!("../current_mixed_policy_v1/normalize.rs"),
                ),
                (
                    "mtg-kernel/src/divine_actions_v44.rs",
                    include_str!("../divine_actions_v44.rs"),
                ),
                (
                    "mtg-kernel/src/end_turn_candidates_v68.rs",
                    include_str!("../end_turn_candidates_v68.rs"),
                ),
                (
                    "mtg-kernel/src/end_turn_observation_v68.rs",
                    include_str!("../end_turn_observation_v68.rs"),
                ),
                (
                    "mtg-kernel/src/flat_policy_v1.rs",
                    include_str!("../flat_policy_v1.rs"),
                ),
                (
                    "mtg-kernel/src/flat_policy_v2.rs",
                    include_str!("../flat_policy_v2.rs"),
                ),
                (
                    "mtg-kernel/src/human_bo3_v1.rs",
                    include_str!("../human_bo3_v1.rs"),
                ),
                (
                    "mtg-kernel/src/human_bo3_v1/end_turn_v68.rs",
                    include_str!("../human_bo3_v1/end_turn_v68.rs"),
                ),
                (
                    "mtg-kernel/src/human_bo3_v1/labels.rs",
                    include_str!("../human_bo3_v1/labels.rs"),
                ),
                (
                    "mtg-kernel/src/human_bo3_v1/limited_end_turn_v9.rs",
                    include_str!("../human_bo3_v1/limited_end_turn_v9.rs"),
                ),
                (
                    "mtg-kernel/src/human_bo3_v1/prepared_v57.rs",
                    include_str!("../human_bo3_v1/prepared_v57.rs"),
                ),
                (
                    "mtg-kernel/src/human_bo3_v1/ravenous_v51.rs",
                    include_str!("../human_bo3_v1/ravenous_v51.rs"),
                ),
                ("mtg-kernel/src/lib.rs", include_str!("../lib.rs")),
                (
                    "mtg-kernel/src/limited_end_turn_session_v9.rs",
                    include_str!("../limited_end_turn_session_v9.rs"),
                ),
                (
                    "mtg-kernel/src/limited_session_v1.rs",
                    include_str!("../limited_session_v1.rs"),
                ),
                (
                    "mtg-kernel/src/ravenous_observation_v51.rs",
                    include_str!("../ravenous_observation_v51.rs"),
                ),
                (
                    "mtg-kernel/src/ravenous_public_v51.rs",
                    include_str!("../ravenous_public_v51.rs"),
                ),
                (
                    "mtg-kernel/src/rl_session.rs",
                    include_str!("../rl_session.rs"),
                ),
                (
                    "mtg-kernel/src/rl_session/end_turn_v68.rs",
                    include_str!("../rl_session/end_turn_v68.rs"),
                ),
                (
                    "mtg-kernel/src/rl_session/limited_common_v1.rs",
                    include_str!("../rl_session/limited_common_v1.rs"),
                ),
                (
                    "mtg-kernel/src/rl_session/ravenous_v51.rs",
                    include_str!("../rl_session/ravenous_v51.rs"),
                ),
                (
                    "mtg-kernel/src/rl_variable_cast_v1.rs",
                    include_str!("../rl_variable_cast_v1.rs"),
                ),
                (
                    "mtg-kernel/src/rules_meaning_time_stop_v68.rs",
                    include_str!("../rules_meaning_time_stop_v68.rs"),
                ),
                (
                    "mtg-kernel/src/time_stop_public_v68.rs",
                    include_str!("../time_stop_public_v68.rs"),
                ),
                (
                    "python/tools/limited_end_turn_session_v9.py",
                    include_str!("../../../python/tools/limited_end_turn_session_v9.py"),
                ),
                (
                    "mtg-kernel/src/rl_session/flat_action_v4.rs",
                    include_str!("../rl_session/flat_action_v4.rs"),
                ),
                (
                    "mtg-kernel/src/rl_session/flat_action_v4/search_state.rs",
                    include_str!("../rl_session/flat_action_v4/search_state.rs"),
                ),
                (
                    "mtg-kernel/src/rl_session/flat_action_v4/search_state/sampler.rs",
                    include_str!("../rl_session/flat_action_v4/search_state/sampler.rs"),
                ),
                (
                    "mtg-kernel/src/cast_identity_v1.rs",
                    include_str!("../cast_identity_v1.rs"),
                ),
                (
                    "mtg-kernel/build_standard_v1.rs",
                    include_str!("../../build_standard_v1.rs"),
                ),
                ("mtg-kernel/src/card_def.rs", include_str!("../card_def.rs")),
                ("mtg-kernel/src/effect.rs", include_str!("../effect.rs")),
                (
                    "mtg-kernel/src/planeswalker_v1.rs",
                    include_str!("../planeswalker_v1.rs"),
                ),
                (
                    "mtg-kernel/src/rules_vector_v1/meaning/mod.rs",
                    include_str!("../rules_vector_v1/meaning/mod.rs"),
                ),
                (
                    "mtg-kernel/src/rules_vector_v1/meaning/targets.rs",
                    include_str!("../rules_vector_v1/meaning/targets.rs"),
                ),
                (
                    "mtg-kernel/src/effect/vivien_reid_v1.rs",
                    include_str!("../effect/vivien_reid_v1.rs"),
                ),
                (
                    "mtg-kernel/src/effect/squad_rallier_resolution_v1.rs",
                    include_str!("../effect/squad_rallier_resolution_v1.rs"),
                ),
                (
                    "mtg-kernel/src/creature_types_v60.rs",
                    include_str!("../creature_types_v60.rs"),
                ),
                (
                    "mtg-kernel/src/optional_trigger_public_v1.rs",
                    include_str!("../optional_trigger_public_v1.rs"),
                ),
                (
                    "mtg-kernel/src/genesis_wave_observation_v69.rs",
                    include_str!("../genesis_wave_observation_v69.rs"),
                ),
                (
                    "mtg-kernel/src/effect/genesis_wave_v1.rs",
                    include_str!("../effect/genesis_wave_v1.rs"),
                ),
                (
                    "mtg-kernel/src/genesis_wave_public_v69.rs",
                    include_str!("../genesis_wave_public_v69.rs"),
                ),
                (
                    "mtg-kernel/src/linked_aura_operation_v1/genesis_current_slots_v69.rs",
                    include_str!("../linked_aura_operation_v1/genesis_current_slots_v69.rs"),
                ),
                (
                    "mtg-kernel/src/rules_meaning_vivien_v70.rs",
                    include_str!("../rules_meaning_vivien_v70.rs"),
                ),
                (
                    "mtg-kernel/src/vivien_public_v70.rs",
                    include_str!("../vivien_public_v70.rs"),
                ),
                (
                    "mtg-kernel/src/vivien_observation_v70.rs",
                    include_str!("../vivien_observation_v70.rs"),
                ),
                (
                    "mtg-kernel/src/vivien_candidates_v70.rs",
                    include_str!("../vivien_candidates_v70.rs"),
                ),
                (
                    "mtg-kernel/src/rl_session/vivien_v70.rs",
                    include_str!("../rl_session/vivien_v70.rs"),
                ),
                (
                    "mtg-kernel/src/current_mixed_policy_v1/vivien.rs",
                    include_str!("../current_mixed_policy_v1/vivien.rs"),
                ),
                (
                    "mtg-kernel/src/human_bo3_v1/vivien_v70.rs",
                    include_str!("../human_bo3_v1/vivien_v70.rs"),
                ),
                (
                    "mtg-kernel/src/human_bo3_v1/limited_vivien_v11.rs",
                    include_str!("../human_bo3_v1/limited_vivien_v11.rs"),
                ),
                (
                    "mtg-kernel/src/limited_vivien_session_v11.rs",
                    include_str!("../limited_vivien_session_v11.rs"),
                ),
                (
                    "mtg-kernel/src/bin/kernel_limited_vivien_env_v11.rs",
                    include_str!("../bin/kernel_limited_vivien_env_v11.rs"),
                ),
                (
                    "mtg-kernel/src/current_mixed_policy_v1/canonical_semantic_booleans.rs",
                    include_str!("../current_mixed_policy_v1/canonical_semantic_booleans.rs"),
                ),
                (
                    "mtg-kernel/src/engine/muldrotha_v1.rs",
                    include_str!("muldrotha_v1.rs"),
                ),
                (
                    "mtg-kernel/src/current_mixed_policy_v1/roots.rs",
                    include_str!("../current_mixed_policy_v1/roots.rs"),
                ),
                (
                    "mtg-kernel/src/current_mixed_policy_v1/ordinary_removal_printing.rs",
                    include_str!("../current_mixed_policy_v1/ordinary_removal_printing.rs"),
                ),
                (
                    "mtg-kernel/src/engine/tinybones_v72.rs",
                    include_str!("tinybones_v72.rs"),
                ),
                (
                    "mtg-kernel/src/effect/liliana_dreadhorde_v1.rs",
                    include_str!("../effect/liliana_dreadhorde_v1.rs"),
                ),
                (
                    "mtg-kernel/src/muldrotha_public_v71.rs",
                    include_str!("../muldrotha_public_v71.rs"),
                ),
                (
                    "mtg-kernel/src/rules_meaning_muldrotha_v71.rs",
                    include_str!("../rules_meaning_muldrotha_v71.rs"),
                ),
                (
                    "mtg-kernel/src/muldrotha_observation_v71.rs",
                    include_str!("../muldrotha_observation_v71.rs"),
                ),
                (
                    "mtg-kernel/src/muldrotha_candidates_v71.rs",
                    include_str!("../muldrotha_candidates_v71.rs"),
                ),
                (
                    "mtg-kernel/src/rl_session/muldrotha_v71.rs",
                    include_str!("../rl_session/muldrotha_v71.rs"),
                ),
                (
                    "mtg-kernel/src/current_mixed_policy_v1/muldrotha.rs",
                    include_str!("../current_mixed_policy_v1/muldrotha.rs"),
                ),
                (
                    "mtg-kernel/src/human_bo3_v1/muldrotha_v71.rs",
                    include_str!("../human_bo3_v1/muldrotha_v71.rs"),
                ),
                (
                    "mtg-kernel/src/human_bo3_v1/limited_muldrotha_v12.rs",
                    include_str!("../human_bo3_v1/limited_muldrotha_v12.rs"),
                ),
                (
                    "mtg-kernel/src/limited_muldrotha_session_v12.rs",
                    include_str!("../limited_muldrotha_session_v12.rs"),
                ),
                (
                    "mtg-kernel/src/bin/kernel_limited_muldrotha_env_v12.rs",
                    include_str!("../bin/kernel_limited_muldrotha_env_v12.rs"),
                ),
                (
                    "python/tools/limited_muldrotha_session_v12.py",
                    include_str!("../../../python/tools/limited_muldrotha_session_v12.py"),
                ),
                (
                    "mtg-kernel/src/effect/chandra_flameshaper_v1.rs",
                    include_str!("../effect/chandra_flameshaper_v1.rs"),
                ),
                (
                    "mtg-kernel/src/effect/crystal_prevention_v1.rs",
                    include_str!("../effect/crystal_prevention_v1.rs"),
                ),
                (
                    "mtg-kernel/src/rules_meaning_tinybones_v72.rs",
                    include_str!("../rules_meaning_tinybones_v72.rs"),
                ),
                (
                    "mtg-kernel/src/tinybones_public_v72.rs",
                    include_str!("../tinybones_public_v72.rs"),
                ),
                (
                    "mtg-kernel/src/effect/kaito_cunning_v75.rs",
                    include_str!("../effect/kaito_cunning_v75.rs"),
                ),
                (
                    "mtg-kernel/src/rules_meaning_liliana_v73.rs",
                    include_str!("../rules_meaning_liliana_v73.rs"),
                ),
                (
                    "mtg-kernel/src/liliana_public_v73.rs",
                    include_str!("../liliana_public_v73.rs"),
                ),
                (
                    "mtg-kernel/src/effect/liliana_dreadhorde_public_v73.rs",
                    include_str!("../effect/liliana_dreadhorde_public_v73.rs"),
                ),
                (
                    "mtg-kernel/src/tinybones_observation_v72.rs",
                    include_str!("../tinybones_observation_v72.rs"),
                ),
                (
                    "mtg-kernel/src/tinybones_candidates_v72.rs",
                    include_str!("../tinybones_candidates_v72.rs"),
                ),
                (
                    "mtg-kernel/src/rules_meaning_chandra_v74.rs",
                    include_str!("../rules_meaning_chandra_v74.rs"),
                ),
                (
                    "mtg-kernel/src/chandra_public_v74.rs",
                    include_str!("../chandra_public_v74.rs"),
                ),
                (
                    "mtg-kernel/src/effect/chandra_flameshaper_public_v74.rs",
                    include_str!("../effect/chandra_flameshaper_public_v74.rs"),
                ),
                (
                    "mtg-kernel/src/rl_session/tinybones_v72.rs",
                    include_str!("../rl_session/tinybones_v72.rs"),
                ),
                (
                    "mtg-kernel/src/liliana_observation_v73.rs",
                    include_str!("../liliana_observation_v73.rs"),
                ),
                (
                    "mtg-kernel/src/liliana_candidates_v73.rs",
                    include_str!("../liliana_candidates_v73.rs"),
                ),
                (
                    "mtg-kernel/src/rl_session/liliana_v73.rs",
                    include_str!("../rl_session/liliana_v73.rs"),
                ),
            ] {
                hash.update((name.len() as u64).to_le_bytes());
                hash.update(name.as_bytes());
                hash.update((source.len() as u64).to_le_bytes());
                hash.update(source.as_bytes());
            }
            format!("{:x}", hash.finalize())
        })
        .as_str()
}

#[cfg(all(
    test,
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
#[path = "time_stop_v1_tests.rs"]
pub(crate) mod tests;
