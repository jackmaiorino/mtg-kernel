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
    CopyTokenV36 {
        source: crate::state::AbilitySourceContractV4,
        controller: crate::ids::PlayerId,
    },
}

impl Hash for OperationV1 {
    fn hash<H: Hasher>(&self, hasher: &mut H) {
        serde_json::to_vec(self)
            .expect("typed linked operation serializes")
            .hash(hasher);
    }
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum GeneralAnswerV36 {
    Host(AnswerV1),
    Color {
        choice: crate::heraldic_banner_v1::ChoiceV1,
        color: crate::mana::ManaColor,
    },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum GeneralChoiceV36 {
    Host {
        choice: HostChoiceV1,
        capture: u64,
    },
    Color {
        choice: crate::heraldic_banner_v1::ChoiceV1,
        capture: u32,
    },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeneralCheckpointV36 {
    pub original: Box<GameState>,
    pub operation: OperationV1,
    pub answers: Vec<GeneralAnswerV36>,
    pub choice: GeneralChoiceV36,
    pub operation_history_index: u32,
}
#[derive(Debug, Clone, PartialEq, Eq)]
struct ColorPlanV36 {
    choice: crate::heraldic_banner_v1::ChoiceV1,
    capture: u32,
    color: Option<crate::mana::ManaColor>,
    answer: Option<u32>,
    committed: Option<u32>,
}

/// Runtime ownership cannot be deserialized or constructed outside this
/// module. It exists only inside one synchronous engine invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
struct RunnerV1 {
    original_stack: Vec<crate::state::StackItem>,
    original_history_len: usize,
    batch_depth: usize,
    deferred_departures: Vec<(crate::ids::ObjectId, u32)>,
    answers: Vec<GeneralAnswerV36>,
    cursor: usize,
    checkpoint: Option<(Box<GameState>, GeneralChoiceV36)>,
    operation: OperationV1,
    original_sha256: String,
    banner_start: Option<u32>,
    color_plans: Vec<ColorPlanV36>,
    used_banner: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StateV1 {
    pub pending: Option<Box<CheckpointV1>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending_general_v36: Option<Box<GeneralCheckpointV36>>,
    #[serde(skip)]
    runtime: Option<Box<RunnerV1>>,
}
impl StateV1 {
    pub fn is_empty(&self) -> bool {
        self.pending.is_none() && self.pending_general_v36.is_none()
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
        if let Some(pending) = &self.pending_general_v36 {
            "banner-entry-operation/v36".hash(hasher);
            serde_json::to_vec(pending)
                .expect("owned Banner checkpoint serializes")
                .hash(hasher);
        }
    }
}

pub(crate) fn locally_owned(state: &GameState) -> bool {
    state.engine.linked_aura_operation_v1.runtime.is_some()
}
pub fn has_pending(state: &GameState) -> bool {
    state.engine.linked_aura_operation_v1.pending.is_some() || has_banner_pending(state)
}
pub(crate) fn has_banner_pending(state: &GameState) -> bool {
    state
        .engine
        .linked_aura_operation_v1
        .pending_general_v36
        .is_some()
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
    return_departures(state, &departures);
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
        OperationV1::CopyTokenV36 { source, controller } => {
            let live = state
                .objects
                .try_get(source.source)
                .ok_or("Banner copy template missing")?;
            if live.zone != crate::state::Zone::Battlefield
                || live.zone_change_count != source.zone_change_count
                || live.owner != source.owner
                || live.card_def != source.card_def
                || live.v4.face_down_v1.is_some()
                || !crate::heraldic_banner_v1::owns_definition(source.card_def)
            {
                return Err("Banner copy lost actual original template incarnation".into());
            }
            crate::standard_cards_v1::create_token_copy_without_banner_owner(
                state,
                source.source,
                *controller,
            );
            Ok(None)
        }
        OperationV1::Commit(event) => {
            crate::event::commit_without_linked_aura_v1(state, event.clone());
            Ok(None)
        }
    }
}
fn potential_operation(state: &GameState) -> bool {
    potentially_returns_aura(state) || crate::heraldic_banner_v1::potentially_enters(state)
}

fn operation_has_color_entry(state: &GameState, operation: &OperationV1) -> bool {
    let selected = |event: &ProposedEvent| {
        crate::heraldic_banner_v1::choice(state, event, 0).is_ok_and(|choice| choice.is_some())
    };
    match operation {
        OperationV1::Event(event) | OperationV1::Commit(event) => selected(event),
        OperationV1::Batch(events) => events.iter().any(selected),
        _ => false,
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
    let answers = answers
        .iter()
        .cloned()
        .map(GeneralAnswerV36::Host)
        .collect::<Vec<_>>();
    replay_general(original, operation, &answers, false)
}
fn replay_general(
    original: &GameState,
    operation: &OperationV1,
    answers: &[GeneralAnswerV36],
    used_banner: bool,
) -> Result<(GameState, Option<Decision>), String> {
    if has_pending(original) || locally_owned(original) {
        return Err("entry operation cannot recursively own another public checkpoint".into());
    }
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
        operation: operation.clone(),
        original_sha256: original_sha256(original),
        banner_start: None,
        color_plans: Vec::new(),
        used_banner,
    }));
    if potentially_returns_aura(original) {
        staged
            .engine
            .event_history
            .push(CommittedEvent::BanishingAuraOperationStartedV1 {
                operation: Box::new(operation.clone()),
                original_sha256: original_sha256(original),
            });
    }
    let result = invoke(&mut staged, operation);
    let runner = staged
        .engine
        .linked_aura_operation_v1
        .runtime
        .take()
        .ok_or("entry operation lost synchronous owner")?;
    if runner.cursor != answers.len() {
        return Err("entry replay did not consume every confirmed answer".into());
    }
    if let Some((mut checkpoint, choice)) = runner.checkpoint {
        if runner.used_banner {
            checkpoint
                .engine
                .linked_aura_operation_v1
                .pending_general_v36 = Some(Box::new(GeneralCheckpointV36 {
                original: Box::new(original.clone()),
                operation: operation.clone(),
                answers: answers.to_vec(),
                choice,
                operation_history_index: runner
                    .banner_start
                    .ok_or("Banner checkpoint lost actual operation")?,
            }));
        } else {
            let GeneralChoiceV36::Host { choice, capture } = choice else {
                return Err("legacy host checkpoint changed kind".into());
            };
            let answers = answers
                .iter()
                .map(|answer| match answer {
                    GeneralAnswerV36::Host(host) => Ok(host.clone()),
                    _ => Err("legacy host checkpoint contains color answer"),
                })
                .collect::<Result<Vec<_>, _>>()?;
            checkpoint.engine.linked_aura_operation_v1.pending = Some(Box::new(CheckpointV1 {
                original: Box::new(original.clone()),
                operation: operation.clone(),
                answers,
                choice,
                capture_history_index: capture,
            }));
        }
        return Ok((*checkpoint, None));
    }
    let result = result?;
    if let Some(operation) = runner.banner_start {
        if runner.color_plans.is_empty()
            || runner
                .color_plans
                .iter()
                .any(|plan| plan.committed.is_none())
        {
            return Err("Banner operation ended without all actual entries".into());
        }
        let mut entries: Vec<u32> = runner
            .color_plans
            .iter()
            .map(|plan| plan.committed.unwrap())
            .collect();
        entries.sort_unstable();
        staged
            .engine
            .event_history
            .push(CommittedEvent::BannerEntryOperationCompletedV1 { operation, entries });
    }
    Ok((staged, result))
}

/// The first unanswered return snapshots the exact departure point. Later
/// work may run only in the disposable candidate, never the published state.
pub(crate) fn request(state: &mut GameState, choice: HostChoiceV1) -> Result<bool, String> {
    let at = crate::linked_aura_return_v1::record_capture(state, choice.clone())?;
    let Some(host) = choose(state, choice.clone(), at)? else {
        return Ok(false);
    };
    let start = state.engine.event_history.len();
    crate::linked_aura_return_v1::answer(state, &choice, host)?;
    record_answer(state, &choice, at, host, start)?;
    Ok(true)
}

fn record_answer(
    state: &mut GameState,
    choice: &HostChoiceV1,
    at: u64,
    host: EffectObjectBinding,
    start: usize,
) -> Result<(), String> {
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
            host,
            return_history_index: u64::try_from(returned).map_err(|_| "Aura return exceeds u64")?,
        });
    Ok(())
}

/// Choosing does not move any returning member. In a simultaneous return all
/// menus are captured first, then owners answer in APNAP order (CR 610.3d).
fn choose(
    state: &mut GameState,
    choice: HostChoiceV1,
    at: u64,
) -> Result<Option<EffectObjectBinding>, String> {
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
        return Ok(None);
    }
    if let Some(answer) = runner.answers.get(runner.cursor).cloned() {
        let GeneralAnswerV36::Host(answer) = answer else {
            return Err("entry replay changed its confirmed choice kind".into());
        };
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
        Ok(Some(answer.host))
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
            .checkpoint = Some((
            Box::new(snapshot),
            GeneralChoiceV36::Host {
                choice,
                capture: at,
            },
        ));
        Ok(None)
    }
}

/// All selected until-event effects triggered by the same event return in one
/// entry batch. A creature entering in that batch cannot be a chosen Aura host
/// because host legality is determined before any member enters (303.4f).
pub(crate) fn return_departures(state: &mut GameState, departures: &[(crate::ids::ObjectId, u32)]) {
    use crate::banishing_light_v1::NoReturnV1;
    use crate::state::Zone;
    let mut records = Vec::new();
    state.engine.linked_exile_records.retain(|record| {
        let selected = crate::banishing_light_v1::owns_definition(record.source.card_def)
            && departures.contains(&(record.source.source, record.source.zone_change_count));
        if selected {
            records.push(*record);
        }
        !selected
    });
    if records.is_empty() {
        return;
    }
    let result = (|| -> Result<(), String> {
        let mut reasons = vec![None; records.len()];
        let mut choices = Vec::new();
        for (index, record) in records.iter().copied().enumerate() {
            if crate::banishing_light_v1::original_token(state, record)? {
                reasons[index] = Some(NoReturnV1::TokenCannotReturn);
            } else if !state.objects.try_get(record.exiled).is_some_and(|live| {
                live.zone == Zone::Exile
                    && live.zone_change_count == record.exiled_zone_change_count
                    && state.exile.contains(&record.exiled)
            }) {
                reasons[index] = Some(NoReturnV1::RecipientDeparted);
            } else if crate::card_def::CARD_DEFS[state.objects.get(record.exiled).card_def as usize]
                .attachment
                .is_some()
            {
                let choice = crate::linked_aura_return_v1::prepare(state, record)?;
                choices.push((index, choice));
            }
        }
        // Preserve same-owner source order; opposite owners follow APNAP,
        // independently of the order in which source departures were listed.
        choices.sort_by_key(|(_, choice)| choice.owner != state.active_player);
        let mut captured = Vec::new();
        for (index, choice) in choices {
            let at = crate::linked_aura_return_v1::record_capture(state, choice.clone())?;
            if choice.original_candidates.is_empty() {
                reasons[index] = Some(NoReturnV1::AuraNoLegalHost {
                    capture_history_index: at,
                });
            } else {
                captured.push((index, choice, at));
            }
        }
        let mut selected = Vec::new();
        for (index, choice, at) in captured {
            let Some(host) = choose(state, choice.clone(), at)? else {
                return Ok(());
            };
            selected.push((index, choice, at, host));
        }
        let start = state.engine.event_history.len();
        let events = records
            .iter()
            .enumerate()
            .filter(|(index, _)| reasons[*index].is_none())
            .map(|(_, record)| {
                ProposedEvent::zone_change_preserving_known_identity(
                    record.exiled,
                    Zone::Battlefield,
                )
            })
            .collect();
        let attachments = selected
            .iter()
            .map(|(_, choice, _, host)| (choice.aura, *host))
            .collect();
        crate::event::batch_with_linked_aura_attachments_v1(state, events, attachments);
        for (_, choice, at, host) in selected {
            record_answer(state, &choice, at, host, start)?;
        }
        for (record, reason) in records.into_iter().zip(reasons) {
            crate::banishing_light_v1::record_expiry(state, record, start, reason);
        }
        Ok(())
    })();
    if result.is_err() {
        state.engine.halted = Some((
            crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
            departures[0].0,
        ));
    }
}

/// Complete checkpoint reconstruction authenticates source incarnation,
/// owner, menu, already committed choices, actual journal and operation tail.
pub fn validate(state: &GameState) -> Result<(), String> {
    if let Some(pending) = state
        .engine
        .linked_aura_operation_v1
        .pending_general_v36
        .as_ref()
    {
        if locally_owned(state) || state.engine.linked_aura_operation_v1.pending.is_some() {
            return Err("Banner checkpoint has foreign runtime/second owner".into());
        }
        if !matches!(state.engine.event_history.get(pending.operation_history_index as usize), Some(CommittedEvent::BannerEntryOperationStartedV1 { operation, original_sha256: digest }) if **operation == pending.operation && *digest == original_sha256(&pending.original))
        {
            return Err("Banner checkpoint lost independently committed operation".into());
        }
        let (expected, _) = replay_general(
            &pending.original,
            &pending.operation,
            &pending.answers,
            true,
        )?;
        if expected != *state || !has_banner_pending(&expected) {
            return Err("Banner checkpoint/original operation changed".into());
        }
        return Ok(());
    }
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
    if let Some(pending) = state
        .engine
        .linked_aura_operation_v1
        .pending_general_v36
        .as_ref()
    {
        return Ok(match &pending.choice {
            GeneralChoiceV36::Color { choice, .. } => Decision::ChooseEntryColorV36 {
                player: choice.controller,
                entry_ordinal: choice.ordinal,
                colors: crate::heraldic_banner_v1::COLORS,
            },
            GeneralChoiceV36::Host { choice, .. } => Decision::ChooseEffectTargets {
                player: choice.owner,
                source: choice.aura.object,
                selected_count: 0,
                min_targets: 1,
                max_targets: 1,
                can_finish: false,
                legal_targets: choice
                    .original_candidates
                    .iter()
                    .map(|binding| Target::Object(binding.object))
                    .collect(),
            },
        });
    }

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
    if let Some(pending) = state
        .engine
        .linked_aura_operation_v1
        .pending_general_v36
        .as_ref()
    {
        validate(state)?;
        let answer = match (&pending.choice, action) {
            (GeneralChoiceV36::Color { choice, .. }, Action::ChooseEffectOption(index)) => {
                GeneralAnswerV36::Color {
                    choice: choice.clone(),
                    color: *crate::heraldic_banner_v1::COLORS
                        .get(index as usize)
                        .ok_or("color index outside WUBRG")?,
                }
            }
            (
                GeneralChoiceV36::Host { choice, .. },
                Action::ChooseEffectTarget(Target::Object(id)),
            ) => GeneralAnswerV36::Host(AnswerV1 {
                choice: choice.clone(),
                host: *choice
                    .original_candidates
                    .iter()
                    .find(|binding| binding.object == id)
                    .ok_or("host outside original entry menu")?,
            }),
            _ => {
                return Err(
                    "only the exact pre-entry owner choice may advance this operation".into(),
                )
            }
        };
        let mut answers = pending.answers.clone();
        answers.push(answer);
        let (next, _) = replay_general(&pending.original, &pending.operation, &answers, true)?;
        *state = next;
        return Ok(());
    }
    crate::heraldic_banner_v1::validate_state(state)?;
    if locally_owned(state) || (!has_pending(state) && !potential_operation(state)) {
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
                .map(|pending| pending.choice.aura.object)
                .unwrap_or(crate::ids::ObjectId(0)),
        });
    }
    if crate::heraldic_banner_v1::validate_state(state).is_err() {
        return Decision::Halted {
            mechanic: crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
            source: crate::ids::ObjectId(0),
        };
    }
    if locally_owned(state) || !potential_operation(state) {
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
    !locally_owned(state)
        && (has_pending(state)
            || potential_operation(state)
            || crate::card_def::CARD_DEFS
                .iter()
                .enumerate()
                .any(|(id, _)| crate::heraldic_banner_v1::owns_definition(id as u16)))
}

pub(crate) fn event(state: &mut GameState, operation: OperationV1) -> bool {
    if locally_owned(state) {
        return false;
    }
    if crate::heraldic_banner_v1::validate_state(state).is_err() {
        return true;
    }
    if !has_pending(state)
        && !potential_operation(state)
        && !operation_has_color_entry(state, &operation)
    {
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

/// Capture the complete actual entry roster before any member is committed.
/// Answers run in APNAP order; proposal ordinals retain the original roster.
pub(crate) fn prepare_entry_colors(
    state: &mut GameState,
    events: &[ProposedEvent],
) -> Result<bool, String> {
    let mut choices = Vec::new();
    for event in events {
        if let Some(choice) = crate::heraldic_banner_v1::choice(state, event, 0)? {
            choices.push(choice);
        }
    }
    if choices.is_empty() {
        return Ok(true);
    }
    let runner = state
        .engine
        .linked_aura_operation_v1
        .runtime
        .as_ref()
        .ok_or("Banner as-enters choice has no actual operation owner")?;
    if runner.checkpoint.is_some() {
        return Ok(false);
    }
    // A batch pre-captures these proposals. Its per-member commit consumes
    // an existing plan, never asks a second choice for the same occurrence.
    if choices.len() == 1
        && runner.color_plans.iter().any(|p| {
            p.committed.is_none() && p.color.is_some() && p.choice.proposal == choices[0].proposal
        })
    {
        return Ok(true);
    }
    let first = runner.color_plans.len();
    let mut additions = Vec::new();
    for (offset, mut choice) in choices.into_iter().enumerate() {
        choice.ordinal =
            u32::try_from(first + offset).map_err(|_| "Banner entry ordinal overflow")?;
        additions.push(choice);
    }
    if state
        .engine
        .linked_aura_operation_v1
        .runtime
        .as_ref()
        .unwrap()
        .banner_start
        .is_none()
    {
        let index = u32::try_from(state.engine.event_history.len())
            .map_err(|_| "Banner operation index overflow")?;
        let runtime = state
            .engine
            .linked_aura_operation_v1
            .runtime
            .as_mut()
            .unwrap();
        runtime.banner_start = Some(index);
        runtime.used_banner = true;
        state
            .engine
            .event_history
            .push(CommittedEvent::BannerEntryOperationStartedV1 {
                operation: Box::new(runtime.operation.clone()),
                original_sha256: runtime.original_sha256.clone(),
            });
    }
    let operation = state
        .engine
        .linked_aura_operation_v1
        .runtime
        .as_ref()
        .unwrap()
        .banner_start
        .unwrap();
    for choice in additions {
        let capture = u32::try_from(state.engine.event_history.len())
            .map_err(|_| "Banner capture index overflow")?;
        state
            .engine
            .event_history
            .push(CommittedEvent::BannerEntryColorCapturedV1 {
                operation,
                choice: Box::new(choice.clone()),
            });
        state
            .engine
            .linked_aura_operation_v1
            .runtime
            .as_mut()
            .unwrap()
            .color_plans
            .push(ColorPlanV36 {
                choice,
                capture,
                color: None,
                answer: None,
                committed: None,
            });
    }
    let mut indices = (first
        ..state
            .engine
            .linked_aura_operation_v1
            .runtime
            .as_ref()
            .unwrap()
            .color_plans
            .len())
        .collect::<Vec<_>>();
    indices.sort_by_key(|&index| {
        let p = &state
            .engine
            .linked_aura_operation_v1
            .runtime
            .as_ref()
            .unwrap()
            .color_plans[index];
        (p.choice.controller != state.active_player, p.choice.ordinal)
    });
    for index in indices {
        let runtime = state
            .engine
            .linked_aura_operation_v1
            .runtime
            .as_ref()
            .unwrap();
        let plan = runtime.color_plans[index].clone();
        if let Some(answer) = runtime.answers.get(runtime.cursor).cloned() {
            let GeneralAnswerV36::Color { choice, color } = answer else {
                return Err("Banner replay changed its confirmed choice kind".into());
            };
            if choice != plan.choice || !crate::heraldic_banner_v1::COLORS.contains(&color) {
                return Err("Banner replay changed an actual entry/color answer".into());
            }
            let at = u32::try_from(state.engine.event_history.len())
                .map_err(|_| "Banner answer index overflow")?;
            state
                .engine
                .event_history
                .push(CommittedEvent::BannerEntryColorChosenV1 {
                    capture: plan.capture,
                    color,
                });
            let runtime = state
                .engine
                .linked_aura_operation_v1
                .runtime
                .as_mut()
                .unwrap();
            runtime.cursor += 1;
            runtime.color_plans[index].color = Some(color);
            runtime.color_plans[index].answer = Some(at);
        } else {
            let mut snapshot = state.clone();
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
                    .any(|i| i.v4.stack_item_id == original.v4.stack_item_id);
                let departed = snapshot.engine.event_history[runner.original_history_len..].iter().any(|event|
                    matches!(event, CommittedEvent::ZoneChange { object, from: crate::state::Zone::Stack, .. } if *object == original.source));
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
                .checkpoint = Some((
                Box::new(snapshot),
                GeneralChoiceV36::Color {
                    choice: plan.choice,
                    capture: plan.capture,
                },
            ));
            return Ok(false);
        }
    }
    Ok(true)
}

/// Called after the actual allocator/move and before entry observers.
pub(crate) fn commit_entry_color(
    state: &mut GameState,
    proposal: &ProposedEvent,
    object: crate::ids::ObjectId,
    entry: usize,
) -> Result<(), String> {
    if !crate::heraldic_banner_v1::owns_definition(state.objects.get(object).card_def) {
        return Ok(());
    }
    let runner = state
        .engine
        .linked_aura_operation_v1
        .runtime
        .as_ref()
        .ok_or("Banner committed entry lost its operation owner")?;
    let index = runner
        .color_plans
        .iter()
        .position(|p| p.committed.is_none() && p.choice.proposal == *proposal)
        .ok_or("Banner committed entry was not in its actual pre-entry roster")?;
    let plan = runner.color_plans[index].clone();
    let color = plan
        .color
        .ok_or("Banner entry committed before its choice")?;
    let entered = crate::heraldic_banner_v1::EnteredV1 {
        operation: runner
            .banner_start
            .ok_or("Banner entry lost operation occurrence")?,
        capture: plan.capture,
        answer: plan.answer.ok_or("Banner entry lost chosen occurrence")?,
        entry: u32::try_from(entry).map_err(|_| "Banner entry index overflow")?,
        object: crate::state::AbilitySourceContractV4::capture(state, object),
        color,
    };
    state.objects.get_mut(object).v4.chosen_color = Some(color);
    let marker =
        u32::try_from(state.engine.event_history.len()).map_err(|_| "Banner marker overflow")?;
    state
        .engine
        .event_history
        .push(CommittedEvent::BannerEntryColorCommittedV1 {
            entered: Box::new(entered),
        });
    state
        .engine
        .linked_aura_operation_v1
        .runtime
        .as_mut()
        .unwrap()
        .color_plans[index]
        .committed = Some(marker);
    Ok(())
}
