//! Native owner ordering for actual simultaneous graveyard arrivals.
//!
//! An engine operation owns the unresolved batch. Its private working copy
//! can continue speculatively after suspension, but only the checkpoint just
//! at the order boundary becomes visible. Answers replay the original
//! operation and order its actual arrivals before trigger collection resumes.

use crate::effect::EffectObjectBinding;
use crate::engine::{Action, Decision, UnsupportedMechanic};
use crate::event::{CommittedEvent, ProposedEvent};
use crate::ids::{ObjectId, PlayerId};
use crate::state::{GameState, ObjectLinkV4, Target, Zone};
use serde::{Deserialize, Serialize};
use std::hash::{Hash, Hasher};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BatchV1 {
    Effect,
    StateBasedDeaths,
    SacrificeCost,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ChoiceV1 {
    batch: BatchV1,
    history_len: usize,
    owner: PlayerId,
    arrivals: [Vec<ArrivalCardV1>; 2],
    chosen: Vec<ArrivalCardV1>,
    remaining: Vec<ArrivalCardV1>,
}

impl ChoiceV1 {
    fn decision(&self) -> Decision {
        let original = &self.arrivals[self.owner.index()];
        let count = original.len() as u16;
        Decision::ChooseEffectTargets {
            player: self.owner,
            source: original[0].binding.object,
            selected_count: self.chosen.len() as u16,
            min_targets: count,
            max_targets: count,
            can_finish: false,
            legal_targets: self
                .remaining
                .iter()
                .map(|card| Target::Object(card.binding.object))
                .collect(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ArrivalCardV1 {
    binding: EffectObjectBinding,
    printing: u16,
    controller: PlayerId,
}

impl ArrivalCardV1 {
    fn public_ref(self, owner: PlayerId) -> crate::rl::CardStableRefV1 {
        crate::rl::CardStableRefV1 {
            arena_id: self.binding.object.0,
            card_db_id: self.printing,
            owner: owner.into(),
            controller: self.controller.into(),
            zone: Zone::Graveyard,
            zone_change_count: self.binding.expected_zone_change_count,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct AnswerV1 {
    choice: ChoiceV1,
    card: ArrivalCardV1,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
enum OperationV1 {
    Advance,
    Step(Action),
    EffectBatch(Vec<ProposedEvent>),
    StateBasedActions,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PendingV1 {
    original: Box<GameState>,
    operation: OperationV1,
    answers: Vec<AnswerV1>,
    choice: ChoiceV1,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RunnerV1 {
    answers: Vec<AnswerV1>,
    consumed: usize,
    checkpoint: Option<(Box<GameState>, ChoiceV1)>,
    text_scenes: Vec<TextSceneV1>,
    quoting: bool,
    invalid: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct StateV1 {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pending: Option<Box<PendingV1>>,
    #[serde(skip)]
    runner: Option<Box<RunnerV1>>,
}

impl StateV1 {
    pub(crate) fn is_empty(&self) -> bool {
        self.pending.is_none()
    }
}

impl Hash for StateV1 {
    fn hash<H: Hasher>(&self, state: &mut H) {
        if let Some(pending) = &self.pending {
            b"native-graveyard-order/v1".hash(state);
            serde_json::to_vec(pending)
                .expect("typed graveyard order checkpoint serializes")
                .hash(state);
        }
    }
}

pub(crate) fn enabled(state: &GameState) -> bool {
    let rules = state.engine.runtime_rules;
    cfg!(feature = "eternal-fixtures")
        && rules.order_graveyard_batches_v1
        && !rules.auto_unobservable_graveyard_order
}

pub(crate) fn owns_operation(state: &GameState) -> bool {
    (state.engine.runtime_rules.order_graveyard_batches_v1
        || !state.engine.graveyard_order_v1.is_empty())
        && state.engine.graveyard_order_v1.runner.is_none()
}

pub(crate) fn paused(state: &GameState) -> bool {
    state
        .engine
        .graveyard_order_v1
        .runner
        .as_ref()
        .is_some_and(|runner| runner.checkpoint.is_some())
}

pub(crate) fn paused_decision(state: &GameState) -> Option<Decision> {
    state
        .engine
        .graveyard_order_v1
        .runner
        .as_ref()?
        .checkpoint
        .as_ref()
        .map(|(_, choice)| choice.decision())
}

fn replay(
    original: &GameState,
    operation: &OperationV1,
    answers: &[AnswerV1],
) -> Result<(GameState, Option<Decision>), String> {
    if !enabled(original) || original.engine.graveyard_order_v1 != StateV1::default() {
        return Err("graveyard order lost its original session and operation".into());
    }
    if crate::linked_aura_operation_v1::has_pending(original) {
        return Err("graveyard order cannot replay an unresolved linked entry operation".into());
    }
    let mut staged = original.clone();
    staged.engine.graveyard_order_v1.runner = Some(Box::new(RunnerV1 {
        answers: answers.to_vec(),
        consumed: 0,
        checkpoint: None,
        text_scenes: Vec::new(),
        quoting: false,
        invalid: false,
    }));
    let result = match operation {
        OperationV1::Advance => Ok(Some(crate::engine::advance_until_decision(&mut staged))),
        OperationV1::Step(action) => {
            crate::engine::step(&mut staged, action.clone()).map(|()| None)
        }
        OperationV1::EffectBatch(events) => {
            crate::event::propose_and_commit_batch(&mut staged, events.clone());
            Ok(None)
        }
        OperationV1::StateBasedActions => {
            crate::trigger::sba_fixed_point(&mut staged);
            Ok(None)
        }
    };
    let runner = staged
        .engine
        .graveyard_order_v1
        .runner
        .take()
        .ok_or("graveyard order lost its private operation owner")?;
    if runner.consumed != answers.len() {
        return Err("graveyard order replay did not consume every exact answer".into());
    }
    if let Some((mut snapshot, choice)) = runner.checkpoint {
        let decision = choice.decision();
        snapshot.engine.graveyard_order_v1.pending = Some(Box::new(PendingV1 {
            original: Box::new(original.clone()),
            operation: operation.clone(),
            answers: answers.to_vec(),
            choice,
        }));
        return Ok((*snapshot, Some(decision)));
    }
    if runner.invalid {
        return Err("native graveyard order could not authenticate its batch".into());
    }
    result.map(|decision| (staged, decision))
}

fn validate_pending(state: &GameState, pending: &PendingV1) -> Result<(), String> {
    let (rebuilt, _) = replay(&pending.original, &pending.operation, &pending.answers)?;
    if rebuilt != *state {
        return Err("graveyard order checkpoint differs from its native replay".into());
    }
    Ok(())
}

pub(crate) struct PublicBoundaryV1 {
    pub(crate) choice: Option<crate::premodern_ordered_public_v1::OrderChoiceV1>,
    pub(crate) unplaced: [Vec<ObjectLinkV4>; 2],
}

/// The whole pending operation is authenticated before reading its projection.
/// Quote-stage identities are carried from the actual post-replacement
/// arrival and disclosed only to its owner at the ordering prompt.
pub(crate) fn public_boundary(
    state: &GameState,
    viewer: PlayerId,
) -> Result<Option<PublicBoundaryV1>, String> {
    if !enabled(state) {
        return Err("Premodern ordered observation requires its native reset profile".into());
    }
    let Some(pending) = state.engine.graveyard_order_v1.pending.as_ref() else {
        return Ok(None);
    };
    validate_pending(state, pending)?;
    let choice = &pending.choice;
    let unplaced = choice.arrivals.each_ref().map(|arrivals| {
        if arrivals.len() < 2 {
            return Vec::new();
        }
        arrivals
            .iter()
            .map(|card| ObjectLinkV4 {
                object: card.binding.object,
                zone_change_count: card.binding.expected_zone_change_count,
            })
            .collect()
    });
    let view =
        (choice.owner == viewer).then(|| crate::premodern_ordered_public_v1::OrderChoiceV1 {
            batch: choice.batch,
            chosen_bottom_first: choice
                .chosen
                .iter()
                .map(|card| card.public_ref(viewer))
                .collect(),
            native_candidates: choice
                .remaining
                .iter()
                .map(|card| card.public_ref(viewer))
                .collect(),
        });
    Ok(Some(PublicBoundaryV1 {
        choice: view,
        unplaced,
    }))
}

pub(crate) fn advance(state: &mut GameState) -> Decision {
    let result = if let Some(pending) = state.engine.graveyard_order_v1.pending.as_ref() {
        validate_pending(state, pending).map(|()| pending.choice.decision())
    } else {
        replay(state, &OperationV1::Advance, &[]).map(|(next, decision)| {
            *state = next;
            decision.expect("native advance returns a decision")
        })
    };
    result.unwrap_or_else(|_| {
        let (mechanic, source) = halt(state);
        Decision::Halted { mechanic, source }
    })
}

pub(crate) fn step(state: &mut GameState, action: Action) -> Result<(), String> {
    let (next, _) = if let Some(pending) = state.engine.graveyard_order_v1.pending.as_ref() {
        validate_pending(state, pending)?;
        let Action::ChooseEffectTarget(Target::Object(object)) = action else {
            return Err("only an exact graveyard ordering choice is available".into());
        };
        let card = pending
            .choice
            .remaining
            .iter()
            .find(|card| card.binding.object == object)
            .ok_or("graveyard order choice is not in the native batch")?;
        let mut answers = pending.answers.clone();
        answers.push(AnswerV1 {
            choice: pending.choice.clone(),
            card: *card,
        });
        replay(&pending.original, &pending.operation, &answers)?
    } else {
        replay(state, &OperationV1::Step(action), &[])?
    };
    *state = next;
    Ok(())
}

pub(crate) fn halt(state: &mut GameState) -> (UnsupportedMechanic, ObjectId) {
    let reason = (UnsupportedMechanic::InvalidEffectContinuation, ObjectId(0));
    if let Some(runner) = state.engine.graveyard_order_v1.runner.as_mut() {
        runner.invalid = true;
    }
    state.engine.halted = Some(reason);
    reason
}

fn direct(state: &mut GameState, operation: OperationV1) {
    if !state.engine.graveyard_order_v1.is_empty() {
        return;
    }
    match replay(state, &operation, &[]) {
        Ok((next, _)) => *state = next,
        Err(_) => {
            halt(state);
        }
    }
}

pub(crate) fn effect_batch(state: &mut GameState, events: Vec<ProposedEvent>) {
    direct(state, OperationV1::EffectBatch(events));
}

pub(crate) fn state_based_actions(state: &mut GameState) {
    direct(state, OperationV1::StateBasedActions);
}

fn pick(state: &mut GameState, choice: ChoiceV1) -> Result<Option<ArrivalCardV1>, String> {
    let runner = state
        .engine
        .graveyard_order_v1
        .runner
        .as_ref()
        .ok_or("graveyard order batch has no native engine operation owner")?;
    if runner.checkpoint.is_some() {
        return Ok(None);
    }
    if let Some(answer) = runner.answers.get(runner.consumed) {
        if answer.choice != choice || !choice.remaining.contains(&answer.card) {
            return Err("graveyard order answer changed its actual owner or batch".into());
        }
        let card = answer.card;
        state
            .engine
            .graveyard_order_v1
            .runner
            .as_mut()
            .unwrap()
            .consumed += 1;
        return Ok(Some(card));
    }
    // The original deck and the replay tail are private. Legacy projections
    // refuse this mode before opening any of its exact choice operands.
    let mut snapshot = state.clone();
    snapshot.engine.graveyard_order_v1 = StateV1::default();
    crate::linked_aura_operation_v1::clear_runtime_for_graveyard_checkpoint(&mut snapshot);
    state
        .engine
        .graveyard_order_v1
        .runner
        .as_mut()
        .unwrap()
        .checkpoint = Some((Box::new(snapshot), choice));
    Ok(None)
}

pub(crate) type TextTopV1 = (ObjectLinkV4, u16);

#[derive(Debug, Clone, PartialEq, Eq)]
struct TextSceneV1 {
    before: [Option<TextTopV1>; 2],
    entering: Option<EntryTextV1>,
    deferred_triggers: Vec<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct EntryTextV1 {
    final_top: [Option<TextTopV1>; 2],
    arrivals: Vec<ObjectLinkV4>,
}

pub(crate) fn batch_text_top(
    state: &GameState,
    host: ObjectLinkV4,
    owner: PlayerId,
) -> Option<Option<TextTopV1>> {
    let scene = state
        .engine
        .graveyard_order_v1
        .runner
        .as_ref()?
        .text_scenes
        .last()?;
    Some(match &scene.entering {
        Some(entry) if entry.arrivals.contains(&host) => entry.final_top[owner.index()],
        _ => scene.before[owner.index()],
    })
}

/// When a donor itself is arriving later in this batch, a trigger can bind
/// that donor's real public incarnation only after the last move.
pub(crate) fn quoting_entry_scene(state: &GameState) -> bool {
    state
        .engine
        .graveyard_order_v1
        .runner
        .as_ref()
        .is_some_and(|runner| runner.quoting)
}

pub(crate) fn defer_donated_entry(
    state: &mut GameState,
    marker: CommittedEvent,
) -> Result<bool, String> {
    let Some(scene) = state
        .engine
        .graveyard_order_v1
        .runner
        .as_mut()
        .and_then(|runner| runner.text_scenes.last_mut())
        .filter(|scene| scene.entering.is_some())
    else {
        return Ok(false);
    };
    let index = u64::try_from(state.engine.event_history.len())
        .map_err(|_| "ordered entry history index exceeds u64")?;
    scene.deferred_triggers.push(index);
    state.engine.event_history.push(marker);
    Ok(true)
}

type ArrivalsV1 = [Vec<(usize, ArrivalCardV1)>; 2];

pub(crate) struct OrderedEntrySceneV1 {
    original: ArrivalsV1,
    ordered: ArrivalsV1,
    text: EntryTextV1,
}

pub(crate) fn needs_ordered_entry_scene(state: &GameState, events: &[ProposedEvent]) -> bool {
    enabled(state)
        && !quoting_entry_scene(state)
        && events.iter().any(|event| {
            matches!(event,
            ProposedEvent::ZoneChange(change) if change.to_zone == Zone::Graveyard)
        })
        && events.iter().any(|event| {
            matches!(event,
            ProposedEvent::ZoneChange(change) if change.to_zone == Zone::Battlefield)
                || matches!(event, ProposedEvent::CreateToken(_))
        })
        && (state
            .players
            .iter()
            .flat_map(|player| player.battlefield.iter().copied())
            .any(|id| {
                crate::full_text_v1::owns_definition(
                    &crate::card_def::CARD_DEFS[usize::from(state.objects.get(id).card_def)],
                )
            })
            || events.iter().any(|event| match event {
                ProposedEvent::ZoneChange(change) if change.to_zone == Zone::Battlefield => {
                    crate::full_text_v1::owns_definition(
                        &crate::card_def::CARD_DEFS
                            [usize::from(state.objects.get(change.object).card_def)],
                    )
                }
                ProposedEvent::CreateToken(token) => crate::full_text_v1::owns_definition(
                    &crate::card_def::CARD_DEFS[usize::from(token.token_def)],
                ),
                _ => false,
            }))
}

pub(crate) fn quote_scene(state: &GameState) -> Result<GameState, String> {
    let mut quote = state.clone();
    quote
        .engine
        .graveyard_order_v1
        .runner
        .as_mut()
        .ok_or("graveyard entry quote lost its native operation")?
        .quoting = true;
    Ok(quote)
}

/// One lexical native batch. Observe its actual committed zone changes
/// because destination-only replacements can still run during materialization.
pub(crate) struct PreparedBatchV1 {
    batch: BatchV1,
    history_start: usize,
    possible_arrivals: Vec<EffectObjectBinding>,
}

impl PreparedBatchV1 {
    pub(crate) fn capture(
        state: &GameState,
        events: &[ProposedEvent],
        batch: BatchV1,
    ) -> Result<Option<Self>, String> {
        if !enabled(state) {
            return Ok(None);
        }
        let mut possible_arrivals = Vec::new();
        for event in events {
            let ProposedEvent::ZoneChange(change) = event else {
                continue;
            };
            if change.to_zone != Zone::Graveyard {
                continue;
            }
            let live = state
                .objects
                .try_get(change.object)
                .ok_or("graveyard batch member does not exist")?;
            if live.zone == Zone::Graveyard
                || live.v4.is_token
                || live.spell_copy_origin.is_some()
                || crate::card_def::CARD_DEFS[usize::from(live.card_def)].is_token
            {
                continue;
            }
            let binding = EffectObjectBinding {
                object: change.object,
                expected_zone: live.zone,
                expected_zone_change_count: live.zone_change_count,
            };
            if possible_arrivals.contains(&binding) {
                return Err("graveyard order batch repeats a physical card".into());
            }
            possible_arrivals.push(binding);
        }
        if possible_arrivals.is_empty() {
            return Ok(None);
        }
        Ok(Some(Self {
            batch,
            history_start: state.engine.event_history.len(),
            possible_arrivals,
        }))
    }

    pub(crate) fn begin(
        &self,
        state: &mut GameState,
        ordered_scene: Option<&OrderedEntrySceneV1>,
    ) -> Result<(), String> {
        let text = [PlayerId::P0, PlayerId::P1].map(|owner| {
            state
                .engine
                .graveyard_order_v1
                .runner
                .as_ref()
                .and_then(|runner| runner.text_scenes.last())
                .map(|scene| scene.before[owner.index()])
                .unwrap_or_else(|| crate::full_text_v1::graveyard_text_top(state, owner))
        });
        state
            .engine
            .graveyard_order_v1
            .runner
            .as_mut()
            .ok_or("graveyard order batch lost its native operation")?
            .text_scenes
            .push(TextSceneV1 {
                before: text,
                entering: ordered_scene.map(|scene| scene.text.clone()),
                deferred_triggers: Vec::new(),
            });
        Ok(())
    }

    fn actual_arrivals(&self, state: &GameState) -> Result<ArrivalsV1, String> {
        let mut actual: ArrivalsV1 = Default::default();
        for owner in [PlayerId::P0, PlayerId::P1] {
            for (slot, &object) in state.players[owner.index()].graveyard.iter().enumerate() {
                let Some(before) = self
                    .possible_arrivals
                    .iter()
                    .find(|member| member.object == object)
                else {
                    continue;
                };
                let live = state.objects.get(object);
                if before.expected_zone_change_count.checked_add(1) != Some(live.zone_change_count)
                    || !state
                        .engine
                        .event_history
                        .iter()
                        .skip(self.history_start)
                        .any(|event| {
                            matches!(event, CommittedEvent::ZoneChange {
                            object: moved, from, to: Zone::Graveyard, ..
                        } if *moved == object && *from == before.expected_zone)
                        })
                {
                    return Err("graveyard arrival lost its native zone-change occurrence".into());
                }
                actual[owner.index()].push((
                    slot,
                    ArrivalCardV1 {
                        binding: EffectObjectBinding {
                            object,
                            expected_zone: Zone::Graveyard,
                            expected_zone_change_count: live.zone_change_count,
                        },
                        printing: crate::rl::projected_card_def(
                            state,
                            object,
                            live.zone_change_count,
                        ),
                        controller: live.controller,
                    },
                ));
            }
        }
        Ok(actual)
    }

    fn choose_order(
        &self,
        state: &mut GameState,
        actual: &ArrivalsV1,
    ) -> Result<Option<ArrivalsV1>, String> {
        let active = state.active_player;
        let mut ordered: ArrivalsV1 = Default::default();
        let arrivals: [Vec<_>; 2] = actual
            .each_ref()
            .map(|rows| rows.iter().map(|(_, card)| *card).collect());
        for owner in [active, active.opponent()] {
            let rows = &actual[owner.index()];
            let mut remaining = arrivals[owner.index()].clone();
            let mut chosen = Vec::new();
            u16::try_from(remaining.len())
                .map_err(|_| "graveyard order exceeds native action width")?;
            while remaining.len() > 1 {
                let choice = ChoiceV1 {
                    batch: self.batch,
                    history_len: state.engine.event_history.len(),
                    owner,
                    arrivals: arrivals.clone(),
                    chosen: chosen.clone(),
                    remaining: remaining.clone(),
                };
                let Some(card) = pick(state, choice)? else {
                    return Ok(None);
                };
                remaining.retain(|candidate| *candidate != card);
                chosen.push(card);
            }
            chosen.extend(remaining);
            ordered[owner.index()] = rows.iter().map(|(slot, _)| *slot).zip(chosen).collect();
        }
        Ok(Some(ordered))
    }

    pub(crate) fn prepare_ordered_entry(
        &self,
        state: &mut GameState,
        mut quote: GameState,
    ) -> Result<Option<OrderedEntrySceneV1>, String> {
        if quote.engine.halted.is_some() || crate::linked_aura_operation_v1::has_pending(&quote) {
            return Err("graveyard entry quote could not complete its native batch".into());
        }
        let original = self.actual_arrivals(&quote)?;
        let Some(ordered) = self.choose_order(state, &original)? else {
            return Ok(None);
        };
        apply_order(&mut quote, &ordered);
        let final_top = [PlayerId::P0, PlayerId::P1]
            .map(|owner| crate::full_text_v1::graveyard_text_top(&quote, owner));
        let arrivals = self.battlefield_arrivals(&quote);
        Ok(Some(OrderedEntrySceneV1 {
            original,
            ordered,
            text: EntryTextV1 {
                final_top,
                arrivals,
            },
        }))
    }

    fn battlefield_arrivals(&self, state: &GameState) -> Vec<ObjectLinkV4> {
        state
            .engine
            .event_history
            .iter()
            .skip(self.history_start)
            .filter_map(crate::trigger::battlefield_entry_object)
            .filter_map(|object| {
                let live = state.objects.try_get(object)?;
                (live.zone == Zone::Battlefield).then_some(ObjectLinkV4 {
                    object,
                    zone_change_count: live.zone_change_count,
                })
            })
            .collect()
    }

    pub(crate) fn finish(
        self,
        state: &mut GameState,
        ordered_scene: Option<OrderedEntrySceneV1>,
    ) -> Result<(), String> {
        let runner = state
            .engine
            .graveyard_order_v1
            .runner
            .as_mut()
            .ok_or("graveyard order batch lost its native operation")?;
        let scene = runner
            .text_scenes
            .pop()
            .ok_or("graveyard order batch lost its frozen departure scene")?;
        if runner.quoting || state.engine.halted.is_some() || paused(state) {
            return Ok(());
        }
        let actual = self.actual_arrivals(state)?;
        let expected_top = ordered_scene.as_ref().map(|plan| plan.text.final_top);
        let ordered = if let Some(plan) = ordered_scene {
            if plan.original != actual || plan.text.arrivals != self.battlefield_arrivals(state) {
                return Err("graveyard entry quote differs from actual native arrivals".into());
            }
            plan.ordered
        } else {
            let Some(ordered) = self.choose_order(state, &actual)? else {
                return Ok(());
            };
            ordered
        };
        apply_order(state, &ordered);
        if expected_top.is_some_and(|expected| {
            [PlayerId::P0, PlayerId::P1]
                .map(|owner| crate::full_text_v1::graveyard_text_top(state, owner))
                != expected
        }) {
            return Err("graveyard entry quote lost its selected final text".into());
        }
        for index in scene.deferred_triggers {
            crate::full_text_v1::record_batch_entry_trigger(state, index)?;
        }
        Ok(())
    }
}

fn apply_order(state: &mut GameState, ordered: &ArrivalsV1) {
    for (player, rows) in state.players.iter_mut().zip(ordered) {
        for (slot, card) in rows {
            player.graveyard[*slot] = card.binding.object;
        }
    }
}

pub(crate) fn refuse_legacy(state: &GameState) -> Result<(), String> {
    if state.engine.runtime_rules.order_graveyard_batches_v1
        || !state.engine.graveyard_order_v1.is_empty()
    {
        return Err("native graveyard ordering requires its distinct public consumer".into());
    }
    Ok(())
}

#[cfg(all(test, feature = "eternal-fixtures"))]
mod tests {
    use super::*;
    use crate::engine::{advance_until_decision, step};
    use crate::policy_observation_v6::tests::put;
    use crate::state::Step;

    fn ready(active: PlayerId) -> GameState {
        let mut state = GameState::new_from_libraries_with_graveyard_order_v1(
            &[],
            &[],
            crate::rl::card_name,
            60_011,
            active,
        );
        state.step = Step::Main1;
        state
    }

    fn pick_next(state: &mut GameState, owner: PlayerId, ids: &[ObjectId], pick: ObjectId) {
        let Decision::ChooseEffectTargets {
            player,
            legal_targets,
            can_finish,
            ..
        } = advance_until_decision(state)
        else {
            panic!("owner order must suspend the native batch");
        };
        assert_eq!(player, owner);
        assert_eq!(
            legal_targets,
            ids.iter().copied().map(Target::Object).collect::<Vec<_>>()
        );
        assert!(!can_finish);
        step(state, Action::ChooseEffectTarget(Target::Object(pick))).unwrap();
    }

    #[test]
    fn actual_effect_arrivals_ignore_replaced_cards_and_keep_native_move_order() {
        for owner in [PlayerId::P0, PlayerId::P1] {
            let mut state = ready(owner);
            let old = put(&mut state, owner, "Forest", Zone::Graveyard);
            let first = put(&mut state, owner, "Island", Zone::Library);
            let second = put(&mut state, owner, "Swamp", Zone::Library);
            put(&mut state, owner, "Dauthi Voidwalker", Zone::Battlefield);
            let replaced = put(&mut state, owner.opponent(), "Forest", Zone::Library);
            let history_start = state.engine.event_history.len();
            // Dauthi's replacement runs during materialization, after the
            // ordinary replacement pass has prepared all three proposals.
            let events = vec![
                ProposedEvent::zone_change(first, Zone::Graveyard),
                ProposedEvent::zone_change(replaced, Zone::Graveyard),
                ProposedEvent::zone_change(second, Zone::Graveyard),
            ];
            crate::event::propose_and_commit_batch(&mut state, events);
            assert_eq!(state.objects.get(replaced).zone, Zone::Exile);
            assert!(crate::eternal_e9b_v1::has_void_counter(&state, replaced));
            let json = serde_json::to_vec(&state).unwrap();
            assert!(refuse_legacy(&state).is_err());
            assert!(step(
                &mut state,
                Action::ChooseEffectTarget(Target::Object(replaced))
            )
            .is_err());
            assert_eq!(serde_json::to_vec(&state).unwrap(), json);
            let mut restored: GameState = serde_json::from_slice(&json).unwrap();
            assert_eq!(state.state_hash(), restored.state_hash());
            pick_next(&mut restored, owner, &[first, second], second);
            assert_eq!(
                restored.players[owner.index()].graveyard,
                [old, second, first]
            );
            let moved = restored
                .engine
                .event_history
                .iter()
                .skip(history_start)
                .filter_map(|row| match row {
                    CommittedEvent::ZoneChange { object, to, .. } => Some((*object, *to)),
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(
                moved,
                [
                    (first, Zone::Graveyard),
                    (replaced, Zone::Exile),
                    (second, Zone::Graveyard)
                ]
            );
        }
    }

    #[test]
    fn simultaneous_sba_deaths_order_by_owner_in_apnap_order() {
        for active in [PlayerId::P0, PlayerId::P1] {
            let mut state = ready(active);
            let mut groups = Vec::new();
            for owner in [active.opponent(), active] {
                let first = put(&mut state, owner, "Triskelion", Zone::Battlefield);
                let second = put(&mut state, owner, "Palinchron", Zone::Battlefield);
                for id in [first, second] {
                    state.objects.get_mut(id).damage = 100;
                }
                groups.push((owner, first, second));
            }
            crate::trigger::sba_fixed_point(&mut state);
            for &(owner, first, second) in groups.iter().rev() {
                pick_next(&mut state, owner, &[first, second], second);
            }
            assert!(state.engine.graveyard_order_v1.is_empty());
            for (owner, first, second) in groups {
                assert_eq!(state.players[owner.index()].graveyard, [second, first]);
            }
        }
    }

    #[test]
    fn only_already_illegal_auras_order_with_the_current_sba_deaths() {
        let owner = PlayerId::P0;
        let mut state = ready(owner);
        let victim = put(&mut state, owner, "Llanowar Elves", Zone::Battlefield);
        let invalid = put(&mut state, owner, "Ethereal Armor", Zone::Battlefield);
        let attached = put(&mut state, owner, "Ethereal Armor", Zone::Battlefield);
        let victim_generation = state.objects.get(victim).zone_change_count;
        let aura_generation = state.objects.get(attached).zone_change_count;
        state
            .attach_object_exact(attached, aura_generation, victim, victim_generation)
            .unwrap();
        state.objects.get_mut(victim).damage = 100;

        crate::trigger::sba_fixed_point(&mut state);
        assert_eq!(state.objects.get(attached).zone, Zone::Battlefield);
        pick_next(&mut state, owner, &[victim, invalid], invalid);
        assert!(state.engine.graveyard_order_v1.is_empty());
        assert_eq!(
            state.players[owner.index()].graveyard,
            [invalid, victim, attached]
        );
    }

    #[test]
    fn legend_choice_and_current_deaths_resume_one_ordered_sba_pass() {
        let owner = PlayerId::P1;
        let mut state = ready(owner);
        let keep = put(
            &mut state,
            owner,
            "Akroma, Angel of Wrath",
            Zone::Battlefield,
        );
        let lose = put(
            &mut state,
            owner,
            "Akroma, Angel of Wrath",
            Zone::Battlefield,
        );
        let dying = put(&mut state, owner, "Llanowar Elves", Zone::Battlefield);
        state.objects.get_mut(dying).damage = 100;

        crate::trigger::sba_fixed_point(&mut state);
        assert!(matches!(advance_until_decision(&mut state),
            Decision::ChooseLegendPermanent { player, candidates }
                if player == owner && candidates == [keep, lose]));
        step(&mut state, Action::ChooseLegendPermanent(keep)).unwrap();
        let pending = serde_json::to_vec(&state).unwrap();
        let mut restored: GameState = serde_json::from_slice(&pending).unwrap();
        pick_next(&mut restored, owner, &[lose, dying], dying);
        assert!(restored.engine.graveyard_order_v1.is_empty());
        assert_eq!(restored.objects.get(keep).zone, Zone::Battlefield);
        assert_eq!(restored.players[owner.index()].graveyard, [dying, lose]);
    }

    #[test]
    fn paid_fireblast_orders_actual_sacrificed_mountains_before_publication() {
        use crate::engine::{CastMode, CostKind};
        use crate::state::CastMethodV4;

        let owner = PlayerId::P0;
        let mut state = ready(owner);
        let first = put(&mut state, owner, "Mountain", Zone::Battlefield);
        let second = put(&mut state, owner, "Mountain", Zone::Battlefield);
        let fireblast = put(&mut state, owner, "Fireblast", Zone::Hand);
        assert!(matches!(advance_until_decision(&mut state),
            Decision::CastSpellOrPass { castable_spells, .. } if castable_spells.contains(&fireblast)));
        step(&mut state, Action::CastSpell(fireblast)).unwrap();
        let target = Target::Player(owner.opponent());
        let mut ordered = false;
        for _ in 0..6 {
            match advance_until_decision(&mut state) {
                Decision::ChooseTargets {
                    spell,
                    legal_targets,
                    ..
                } => {
                    assert_eq!(spell, fireblast);
                    assert!(legal_targets.contains(&target));
                    step(&mut state, Action::ChooseTarget(target)).unwrap();
                }
                Decision::ChooseCastMode { options, .. } => {
                    assert!(options.contains(&CastMode::Alternative));
                    step(&mut state, Action::ChooseCastMode(CastMode::Alternative)).unwrap();
                }
                Decision::ChooseCostTargets {
                    cost_kind: CostKind::SacrificeLands,
                    candidates,
                    ..
                } => {
                    step(&mut state, Action::ChooseCostTarget(candidates[0])).unwrap();
                }
                Decision::ChooseEffectTargets { .. } => {
                    pick_next(&mut state, owner, &[first, second], second);
                    ordered = true;
                    break;
                }
                decision => panic!("unexpected Fireblast decision: {decision:?}"),
            }
        }
        assert!(ordered);
        assert_eq!(state.players[owner.index()].graveyard, [second, first]);
        assert_eq!(state.stack.last().unwrap().source, fireblast);
        assert_eq!(
            state.stack.last().unwrap().v4.cast_method,
            Some(CastMethodV4::Alternative)
        );
        for _ in 0..2 {
            assert!(matches!(
                advance_until_decision(&mut state),
                Decision::CastSpellOrPass { .. }
            ));
            step(&mut state, Action::Pass).unwrap();
        }
        assert!(matches!(
            advance_until_decision(&mut state),
            Decision::CastSpellOrPass { .. }
        ));
        assert_eq!(state.players[owner.opponent().index()].life, 16);
    }

    #[test]
    fn absent_runtime_and_pending_bytes_remain_omitted() {
        let legacy = GameState::new_from_libraries(&[], &[], crate::rl::card_name, 60_011);
        let mut selected = legacy.clone();
        selected.engine.runtime_rules = crate::engine::RuntimeRulesV1::OWNER_ORDERED_GRAVEYARD_V1;
        let json = serde_json::to_value(&legacy).unwrap();
        assert!(json["engine"].get("runtime_rules").is_none());
        assert!(json["engine"].get("graveyard_order_v1").is_none());
        assert_ne!(legacy.state_hash(), selected.state_hash());
    }
}
