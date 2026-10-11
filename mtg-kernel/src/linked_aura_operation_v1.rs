//! An immediate linked-Aura return borrows the genuine operation which caused
//! departure. Only the checkpoint at the owner choice is published. Resuming
//! deterministically reruns that operation with the already confirmed answers;
//! speculative tails are never installed in the game or its committed journal.
use crate::effect::EffectObjectBinding;
use crate::engine::{Action, Decision};
use crate::event::{CommittedEvent, ProposedEvent};
use crate::linked_aura_return_v1::HostChoiceV1;
use crate::state::{GameState, Target};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::hash::{Hash, Hasher};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum OperationV1 {
    Step(Action),
    Advance,
    Event(ProposedEvent),
    Batch(Vec<ProposedEvent>),
    Commit(ProposedEvent),
    Effect {
        op: crate::effect::EffectOp,
        ctx: crate::effect::ExecCtx,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnswerV1 {
    pub choice: HostChoiceV1,
    pub host: EffectObjectBinding,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckpointV1 {
    pub original: Box<GameState>,
    pub operation: OperationV1,
    pub answers: Vec<AnswerV1>,
    pub choice: HostChoiceV1,
    pub capture_history_index: u64,
}

/// Runtime ownership cannot be deserialized or constructed outside this
/// module. It exists only inside one synchronous engine invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
struct RunnerV1 {
    original_stack: Vec<crate::state::StackItem>,
    original_history_len: usize,
    batch_depth: usize,
    deferred_departures: Vec<(crate::ids::ObjectId, u32)>,
    answers: Vec<AnswerV1>,
    cursor: usize,
    checkpoint: Option<(Box<GameState>, HostChoiceV1, u64)>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StateV1 {
    pub pending: Option<Box<CheckpointV1>>,
    #[serde(skip)]
    runtime: Option<Box<RunnerV1>>,
}
impl StateV1 {
    pub fn is_empty(&self) -> bool {
        self.pending.is_none()
    }
}
impl Hash for StateV1 {
    fn hash<H: Hasher>(&self, hasher: &mut H) {
        if let Some(pending) = &self.pending {
            "linked-aura-operation/v1".hash(hasher);
            serde_json::to_vec(pending)
                .expect("owned checkpoint serializes")
                .hash(hasher);
        }
    }
}

pub(crate) fn locally_owned(state: &GameState) -> bool {
    state.engine.linked_aura_operation_v1.runtime.is_some()
}
pub fn has_pending(state: &GameState) -> bool {
    state.engine.linked_aura_operation_v1.pending.is_some()
}
fn potentially_returns_aura(state: &GameState) -> bool {
    state
        .engine
        .linked_exile_records
        .iter()
        .any(|link| crate::banishing_light_v1::original_token(state, *link).is_ok())
}
/// Separate simultaneous departures complete before their immediate linked
/// returns ask for attachment legality on the resulting battlefield.
pub(crate) fn begin_batch(state: &mut GameState) {
    if let Some(runtime) = state.engine.linked_aura_operation_v1.runtime.as_mut() {
        runtime.batch_depth += 1;
    }
}
pub(crate) fn defer_departure(
    state: &mut GameState,
    source: crate::ids::ObjectId,
    generation: u32,
) -> bool {
    let scoped = state.engine.linked_exile_records.iter().any(|link| {
        link.source.source == source
            && link.source.zone_change_count == generation
            && crate::banishing_light_v1::owns_definition(link.source.card_def)
    });
    let Some(runtime) = state.engine.linked_aura_operation_v1.runtime.as_mut() else {
        return false;
    };
    if !scoped || runtime.batch_depth == 0 {
        return false;
    }
    if !runtime.deferred_departures.contains(&(source, generation)) {
        runtime.deferred_departures.push((source, generation));
    }
    true
}
pub(crate) fn end_batch(state: &mut GameState) {
    let Some(runtime) = state.engine.linked_aura_operation_v1.runtime.as_mut() else {
        return;
    };
    runtime.batch_depth = runtime
        .batch_depth
        .checked_sub(1)
        .expect("paired actual batch owner");
    if runtime.batch_depth != 0 {
        return;
    }
    let departures = std::mem::take(&mut runtime.deferred_departures);
    for (source, generation) in departures {
        crate::effect::return_cards_exiled_until_source_leaves(state, source, generation);
    }
}

fn invoke(state: &mut GameState, operation: &OperationV1) -> Result<Option<Decision>, String> {
    match operation {
        OperationV1::Step(action) => {
            crate::engine::step_without_linked_aura_v1(state, action.clone())?;
            Ok(None)
        }
        OperationV1::Advance => Ok(Some(crate::engine::advance_without_linked_aura_v1(state))),
        OperationV1::Event(event) => {
            crate::event::propose_without_linked_aura_v1(state, event.clone());
            Ok(None)
        }
        OperationV1::Batch(events) => {
            crate::event::batch_without_linked_aura_v1(state, events.clone());
            Ok(None)
        }
        OperationV1::Effect { op, ctx } => {
            crate::effect::execute_without_linked_aura_v1(op, ctx, state);
            Ok(None)
        }
        OperationV1::Commit(event) => {
            crate::event::commit_without_linked_aura_v1(state, event.clone());
            Ok(None)
        }
    }
}
fn original_sha256(state: &GameState) -> String {
    Sha256::digest(serde_json::to_vec(state).expect("genuine origin serializes"))
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn replay(
    original: &GameState,
    operation: &OperationV1,
    answers: &[AnswerV1],
) -> Result<(GameState, Option<Decision>), String> {
    if has_pending(original) || locally_owned(original) {
        return Err("linked Aura operation cannot recursively own another checkpoint".into());
    }
    // Authenticate the genuine starting state before granting a synchronous
    // private loan. No validator is bypassed by restored external state.
    crate::effect::validate_pending_effect_choice(original)?;
    let mut staged = original.clone();
    staged.engine.linked_aura_operation_v1.runtime = Some(Box::new(RunnerV1 {
        original_stack: original.stack.clone(),
        original_history_len: original.engine.event_history.len(),
        batch_depth: 0,
        deferred_departures: Vec::new(),
        answers: answers.to_vec(),
        cursor: 0,
        checkpoint: None,
    }));
    staged
        .engine
        .event_history
        .push(CommittedEvent::BanishingAuraOperationStartedV1 {
            operation: Box::new(operation.clone()),
            original_sha256: original_sha256(original),
        });
    let result = invoke(&mut staged, operation);
    let runner = staged
        .engine
        .linked_aura_operation_v1
        .runtime
        .take()
        .ok_or("linked Aura operation lost synchronous owner")?;
    if runner.cursor != answers.len() {
        return Err("linked Aura replay did not consume every original answer".into());
    }
    if let Some((mut checkpoint, choice, capture_history_index)) = runner.checkpoint {
        checkpoint.engine.linked_aura_operation_v1.pending = Some(Box::new(CheckpointV1 {
            original: Box::new(original.clone()),
            operation: operation.clone(),
            answers: answers.to_vec(),
            choice,
            capture_history_index,
        }));
        return Ok((*checkpoint, None));
    }
    Ok((staged, result?))
}

/// The first unanswered return snapshots the exact departure point. Later
/// work may run only in the disposable candidate, never the published state.
pub(crate) fn request(state: &mut GameState, choice: HostChoiceV1) -> Result<bool, String> {
    crate::linked_aura_return_v1::validate(state, &choice)?;
    if choice.original_candidates.is_empty() {
        return Err("empty host menu must use the no-return path".into());
    }
    let runner = state
        .engine
        .linked_aura_operation_v1
        .runtime
        .as_ref()
        .ok_or("nonempty linked Aura return has no genuine operation owner")?;
    if runner.checkpoint.is_some() {
        return Ok(false);
    }
    let answer = runner.answers.get(runner.cursor).cloned();
    let at = crate::linked_aura_return_v1::record_capture(state, choice.clone())?;
    if let Some(answer) = answer {
        if answer.choice != choice || !choice.original_candidates.contains(&answer.host) {
            return Err("linked Aura replay changed an already committed owner answer".into());
        }
        state
            .engine
            .linked_aura_operation_v1
            .runtime
            .as_mut()
            .unwrap()
            .cursor += 1;
        let start = state.engine.event_history.len();
        crate::linked_aura_return_v1::answer(state, &choice, answer.host)?;
        let returned = state.engine.event_history[start..]
            .iter()
            .position(|event| {
                matches!(event,
            CommittedEvent::ZoneChange { object, from: crate::state::Zone::Exile,
                to: crate::state::Zone::Battlefield, .. } if *object == choice.aura.object)
            })
            .map(|offset| start + offset)
            .ok_or("linked Aura selected return did not actually occur")?;
        state
            .engine
            .event_history
            .push(CommittedEvent::BanishingAuraHostChosenV1 {
                capture_history_index: at,
                host: answer.host,
                return_history_index: u64::try_from(returned)
                    .map_err(|_| "Aura return exceeds u64")?,
            });
        Ok(true)
    } else {
        let mut snapshot = state.clone();
        // The genuine resolving row was removed by the ordinary engine before
        // its effects. Borrow that original row for visible stack membership
        // during the owner choice; never reconstruct a fabricated definition.
        let runner = state
            .engine
            .linked_aura_operation_v1
            .runtime
            .as_ref()
            .unwrap();
        if let Some(original) = runner.original_stack.last() {
            let absent = !snapshot
                .stack
                .iter()
                .any(|item| item.v4.stack_item_id == original.v4.stack_item_id);
            let departed=snapshot.engine.event_history[runner.original_history_len..].iter().any(|event|
                matches!(event,CommittedEvent::ZoneChange {object,from:crate::state::Zone::Stack,..} if *object==original.source));
            if absent && !departed {
                snapshot.stack.insert(
                    runner
                        .original_stack
                        .len()
                        .saturating_sub(1)
                        .min(snapshot.stack.len()),
                    original.clone(),
                );
            }
        }
        snapshot.engine.linked_aura_operation_v1 = StateV1::default();
        state
            .engine
            .linked_aura_operation_v1
            .runtime
            .as_mut()
            .unwrap()
            .checkpoint = Some((Box::new(snapshot), choice, at));
        Ok(false)
    }
}

/// Complete checkpoint reconstruction authenticates source incarnation,
/// owner, menu, already committed choices, actual journal and operation tail.
pub fn validate(state: &GameState) -> Result<(), String> {
    let Some(pending) = state.engine.linked_aura_operation_v1.pending.as_ref() else {
        return Ok(());
    };
    if locally_owned(state) || pending.choice.original_candidates.is_empty() {
        return Err("linked Aura public checkpoint has invalid ownership".into());
    }
    let start = pending.original.engine.event_history.len();
    if !matches!(state.engine.event_history.get(start),Some(CommittedEvent::BanishingAuraOperationStartedV1 {operation,original_sha256:digest})
        if **operation==pending.operation && *digest==original_sha256(&pending.original))
    {
        return Err(
            "linked Aura checkpoint lost its independently committed original operation".into(),
        );
    }
    let (expected, _) = replay(&pending.original, &pending.operation, &pending.answers)?;
    if expected != *state || !has_pending(&expected) {
        return Err("linked Aura checkpoint or genuine operation binding changed".into());
    }
    Ok(())
}
fn decision(state: &GameState) -> Result<Decision, String> {
    validate(state)?;
    let pending = state
        .engine
        .linked_aura_operation_v1
        .pending
        .as_ref()
        .ok_or("linked Aura choice is absent")?;
    Ok(Decision::ChooseEffectTargets {
        player: pending.choice.owner,
        source: pending.choice.aura.object,
        selected_count: 0,
        min_targets: 1,
        max_targets: 1,
        can_finish: false,
        legal_targets: pending
            .choice
            .original_candidates
            .iter()
            .map(|binding| Target::Object(binding.object))
            .collect(),
    })
}
pub(crate) fn step(state: &mut GameState, action: Action) -> Result<(), String> {
    if locally_owned(state) || (!has_pending(state) && !potentially_returns_aura(state)) {
        return crate::engine::step_without_linked_aura_v1(state, action);
    }
    if let Some(pending) = state.engine.linked_aura_operation_v1.pending.as_ref() {
        validate(state)?;
        let Action::ChooseEffectTarget(Target::Object(host)) = action else {
            return Err("only the Aura owner may choose its untargeted legal attachment".into());
        };
        let binding = pending
            .choice
            .original_candidates
            .iter()
            .find(|binding| binding.object == host)
            .copied()
            .ok_or("linked Aura attachment is outside the current exact menu")?;
        let mut answers = pending.answers.clone();
        answers.push(AnswerV1 {
            choice: pending.choice.clone(),
            host: binding,
        });
        let (next, _) = replay(&pending.original, &pending.operation, &answers)?;
        *state = next;
        return Ok(());
    }
    let (next, _) = replay(state, &OperationV1::Step(action), &[])?;
    *state = next;
    Ok(())
}
pub(crate) fn advance(state: &mut GameState) -> Decision {
    if has_pending(state) {
        return decision(state).unwrap_or(Decision::Halted {
            mechanic: crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
            source: state
                .engine
                .linked_aura_operation_v1
                .pending
                .as_ref()
                .unwrap()
                .choice
                .aura
                .object,
        });
    }
    if locally_owned(state) || !potentially_returns_aura(state) {
        return crate::engine::advance_without_linked_aura_v1(state);
    }
    match replay(state, &OperationV1::Advance, &[]) {
        Ok((next, result)) => {
            *state = next;
            result.unwrap_or_else(|| decision(state).expect("new exact owner checkpoint"))
        }
        Err(_) => Decision::Halted {
            mechanic: crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
            source: crate::ids::ObjectId(0),
        },
    }
}
pub(crate) fn needs_event_owner(state: &GameState) -> bool {
    !locally_owned(state) && (has_pending(state) || potentially_returns_aura(state))
}

pub(crate) fn event(state: &mut GameState, operation: OperationV1) -> bool {
    if locally_owned(state) || (!has_pending(state) && !potentially_returns_aura(state)) {
        return false;
    }
    if has_pending(state) {
        return true; // No event may mutate an externally suspended owner.
    }
    if let Ok((next, _)) = replay(state, &operation, &[]) {
        *state = next;
    }
    true
}
