//! Distinct public wide-life successor. Original replay inputs stay native/private.
use crate::{
    event::CommittedEvent as Event,
    rl::{CardStableRefV1, PlayerSeatV1, RlContractError},
    snack_wide_life_v59::{self as native, custody::JournalV59},
    state::{AbilitySourceContractV4, GameState, StackItem, Zone},
    trigger::PendingTrigger,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
pub const SCHEMA: &str = "mtg-kernel/next-five-wide-life/public/v59";
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RuleV59 {
    OnePositiveOccurrenceWithExactU32Amount,
    FinalNativeLifeCheckedI32BeforeResolutionRemoval,
    ActualPostLossConquerorObserverAndCapturedController,
    MandatoryTargetlessSavedAmountGain,
    CounterDoesNotEvaluateOrCommitFutureGain,
    OnePandaCounterInstructionForOneActualGain,
    NativeCounterReplacementsStillApply,
    FirstGainCountsOccurrenceNotAmount,
    WideLossSetsActualLossTurnFlag,
    CompleteNativeInputRowRemovalAndOutputValidation,
}
pub fn rules() -> Vec<RuleV59> {
    use RuleV59::*;
    vec![
        OnePositiveOccurrenceWithExactU32Amount,
        FinalNativeLifeCheckedI32BeforeResolutionRemoval,
        ActualPostLossConquerorObserverAndCapturedController,
        MandatoryTargetlessSavedAmountGain,
        CounterDoesNotEvaluateOrCommitFutureGain,
        OnePandaCounterInstructionForOneActualGain,
        NativeCounterReplacementsStillApply,
        FirstGainCountsOccurrenceNotAmount,
        WideLossSetsActualLossTurnFlag,
        CompleteNativeInputRowRemovalAndOutputValidation,
    ]
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum KindV59 {
    Loss,
    Gain,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ObserverV59 {
    Conqueror,
    Panda,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PhaseV59 {
    Fresh,
    Held,
    Published,
    Resolved,
    Countered,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LifeV59 {
    /// Ordinal among actual public wide life events, never a native history index.
    pub operation: usize,
    pub kind: KindV59,
    pub player: PlayerSeatV1,
    pub amount: u32,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConquerorV59 {
    pub occurrence: usize,
    pub loss_operations: Vec<usize>,
    pub source: CardStableRefV1,
    pub current_original_source: Option<CardStableRefV1>,
    pub original_controller: PlayerSeatV1,
    pub losing_opponent: PlayerSeatV1,
    pub saved_actual_amount: u32,
    pub phase: PhaseV59,
    pub actual_gain: Option<u32>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PandaV59 {
    pub occurrence: usize,
    pub gain_operation: usize,
    pub source: CardStableRefV1,
    pub current_original_source: Option<CardStableRefV1>,
    pub original_controller: PlayerSeatV1,
    pub life_gained: u32,
    pub phase: PhaseV59,
    pub operation_attempted: bool,
    pub actual_counters_added: i32,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TriggerOrderV59 {
    pub controller: PlayerSeatV1,
    pub group_position: usize,
    pub observer: ObserverV59,
    pub occurrence: usize,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FactsV59 {
    pub schema: String,
    pub producer_sha256: String,
    pub card_db_hash: u64,
    pub ordered_rules: Vec<RuleV59>,
    pub life: Vec<LifeV59>,
    pub conqueror: Vec<ConquerorV59>,
    pub panda: Vec<PandaV59>,
    pub trigger_order: Vec<TriggerOrderV59>,
}
pub fn producer_sha256() -> String {
    static HASH: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    HASH.get_or_init(|| {
        let mut h = Sha256::new();
        for input in [
            SCHEMA,
            include_str!("snack_wide_public_v59.rs"),
            include_str!("snack_wide_life_v59.rs"),
            include_str!("snack_wide_life_v59/custody.rs"),
            include_str!("linked_aura_operation_v1.rs"),
            include_str!("fiendish_panda_v1.rs"),
            include_str!("fiendish_panda_v1/custody.rs"),
            include_str!("bloodthirsty_conqueror_v49.rs"),
            include_str!("optional_trigger_public_v1.rs"),
            include_str!("life_gain_turn_v1.rs"),
            include_str!("life_loss_turn_v1.rs"),
            include_str!("midnight_snack_v1.rs"),
            include_str!("event.rs"),
            include_str!("engine.rs"),
            include_str!("effect.rs"),
            include_str!("trigger.rs"),
            include_str!("state.rs"),
            include_str!("effect/thousand_year_storm_v1.rs"),
            include_str!("effect/standard_copy.rs"),
            include_str!("standard_spell_copy.rs"),
            include_str!("event_history_v1.rs"),
        ] {
            h.update((input.len() as u64).to_le_bytes());
            h.update(input.as_bytes());
        }
        h.update(crate::conqueror_public_v49::producer_sha256().as_bytes());
        h.update(crate::panda_public_v47::producer_sha256().as_bytes());
        h.finalize().iter().map(|b| format!("{b:02x}")).collect()
    })
    .clone()
}
fn reference(source: AbilitySourceContractV4) -> CardStableRefV1 {
    CardStableRefV1 {
        arena_id: source.source.0,
        card_db_id: source.card_def,
        owner: source.owner.into(),
        controller: source.controller.into(),
        zone: source.zone,
        zone_change_count: source.zone_change_count,
    }
}
fn current(
    state: &GameState,
    source: AbilitySourceContractV4,
) -> Result<Option<CardStableRefV1>, RlContractError> {
    state
        .objects
        .try_get(source.source)
        .filter(|o| {
            o.zone == Zone::Battlefield
                && o.owner == source.owner
                && o.zone_change_count == source.zone_change_count
        })
        .map(|o| crate::sower_restriction_capture_v1::masked_ref(source.source, o))
        .transpose()
}
fn event(state: &GameState, at: u64) -> Result<&Event, RlContractError> {
    state
        .engine
        .event_history
        .get(usize::try_from(at).map_err(|_| RlContractError("wide index exceeds usize".into()))?)
        .ok_or_else(|| RlContractError("wide original event absent".into()))
}
fn panda_capture(effect: &crate::effect::EffectOp) -> Option<u64> {
    match effect {
        crate::effect::EffectOp::FiendishPandaCounterFromCaptureV1 {
            capture_history_index,
        } => Some(*capture_history_index),
        _ => None,
    }
}
fn selected_source(
    state: &GameState,
    effect: &crate::effect::EffectOp,
) -> Result<Option<AbilitySourceContractV4>, RlContractError> {
    if let Some(capture) = native::custody::binding(effect) {
        return native::root_at(state, capture)
            .map(|r| Some(r.0))
            .map_err(RlContractError);
    }
    if let Some(capture) = panda_capture(effect) {
        if let Event::FiendishPandaWideGainCaptureV59 { source, .. } = event(state, capture)? {
            return Ok(Some(*source));
        }
    }
    Ok(None)
}
/// These helpers only project genuinely native-owned wide rows. They mint no execution permission.
pub(crate) fn pending_source_reference(
    state: &GameState,
    pending: &PendingTrigger,
) -> Result<Option<CardStableRefV1>, RlContractError> {
    let Some(source) = selected_source(state, &pending.effect)? else {
        return Ok(None);
    };
    native::validate(state).map_err(RlContractError)?;
    crate::fiendish_panda_v1::validate_state(state).map_err(RlContractError)?;
    if !crate::optional_trigger_public_v1::held(state)
        .iter()
        .any(|p| **p == *pending)
        || pending.source != source.source
        || pending.controller != source.controller
    {
        return Err(RlContractError(
            "wide source requires the complete actual held row".into(),
        ));
    }
    Ok(Some(reference(source)))
}
pub(crate) fn stack_source_reference(
    state: &GameState,
    item: &StackItem,
) -> Result<Option<CardStableRefV1>, RlContractError> {
    let Some(effect) = item.inline_effect.as_ref() else {
        return Ok(None);
    };
    let Some(source) = selected_source(state, effect)? else {
        return Ok(None);
    };
    native::validate(state).map_err(RlContractError)?;
    crate::fiendish_panda_v1::validate_state(state).map_err(RlContractError)?;
    if state
        .stack
        .iter()
        .filter(|row| row.v4.stack_item_id == item.v4.stack_item_id)
        .count()
        != 1
        || !state.stack.iter().any(|row| row == item)
        || item.source != source.source
        || item.controller != source.controller
    {
        return Err(RlContractError(
            "wide source requires the complete actual live row".into(),
        ));
    }
    Ok(Some(reference(source)))
}
pub fn capture(state: &GameState) -> Result<FactsV59, RlContractError> {
    native::validate(state).map_err(RlContractError)?;
    crate::bloodthirsty_conqueror_v49::validate(state).map_err(RlContractError)?;
    crate::fiendish_panda_v1::validate_state(state).map_err(RlContractError)?;
    // Exact immutable printed guards are independent of all current hidden identities.
    crate::conqueror_public_v49::meanings()?;
    let panda_id = crate::card_def::card_id_by_name("Fiendish Panda")
        .ok_or_else(|| RlContractError("Panda printing absent".into()))?;
    crate::rules_meaning_fiendish_panda_v1::printed_meaning(
        panda_id,
        crate::rules_meaning_fiendish_panda_v1::RootV1::ControllerLifeGain,
    )
    .map_err(RlContractError)?;
    let events = state
        .engine
        .event_history
        .iter()
        .enumerate()
        .filter_map(|(at, e)| match e {
            Event::LifeLossWideV59 { player, amount } => {
                Some((at, KindV59::Loss, *player, *amount))
            }
            Event::LifeGainWideV59 { player, amount } => {
                Some((at, KindV59::Gain, *player, *amount))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    let life = events
        .iter()
        .enumerate()
        .map(|(operation, (_, kind, player, amount))| LifeV59 {
            operation,
            kind: *kind,
            player: (*player).into(),
            amount: *amount,
        })
        .collect();
    let ordinal = |at: u64| -> Result<usize, RlContractError> {
        events
            .iter()
            .position(|e| u64::try_from(e.0) == Ok(at))
            .ok_or_else(|| RlContractError("wide public life operation absent".into()))
    };
    let held = crate::optional_trigger_public_v1::held(state);
    let mut conqueror = Vec::new();
    let mut panda = Vec::new();
    let mut conq_indices = Vec::new();
    let mut panda_indices = Vec::new();
    for (at, e) in state.engine.event_history.iter().enumerate() {
        let capture =
            u64::try_from(at).map_err(|_| RlContractError("wide capture exceeds u64".into()))?;
        match e {
            Event::ConquerorCapturedV59 {
                batch_history_index,
                ..
            } => {
                let (source, amount) = native::root_at(state, capture).map_err(RlContractError)?;
                let Event::ConquerorLossBatchV59 { batch } = event(state, *batch_history_index)?
                else {
                    unreachable!("validated wide batch")
                };
                let loss_operations = batch
                    .losses
                    .iter()
                    .filter(|loss| loss.player == source.controller.opponent())
                    .map(|loss| ordinal(loss.history_index))
                    .collect::<Result<_, _>>()?;
                let mut phase = if held
                    .iter()
                    .any(|p| native::custody::binding(&p.effect) == Some(capture))
                {
                    PhaseV59::Held
                } else {
                    PhaseV59::Fresh
                };
                let mut actual_gain = None;
                let published=state.engine.event_history.iter().enumerate().find_map(|(i,e)|match e {
                    Event::ConquerorJournalV59 { journal } if matches!(journal.as_ref(),JournalV59::Published {capture:c,..}if*c==capture)=>Some(i as u64),_=>None});
                if let Some(publication) = published {
                    phase = PhaseV59::Published;
                    if let Some((removal, countered)) = state
                        .engine
                        .event_history
                        .iter()
                        .enumerate()
                        .find_map(|(i, e)| match e {
                            Event::ConquerorJournalV59 { journal } => match journal.as_ref() {
                                JournalV59::Removed {
                                    publication: p,
                                    countered,
                                    ..
                                } if *p == publication => Some((i as u64, *countered)),
                                _ => None,
                            },
                            _ => None,
                        })
                    {
                        phase = if countered {
                            PhaseV59::Countered
                        } else {
                            PhaseV59::Resolved
                        };
                        let gain = state
                            .engine
                            .event_history
                            .iter()
                            .find_map(|e| match e {
                                Event::ConquerorJournalV59 { journal } => match journal.as_ref() {
                                    JournalV59::Closed { removal: r, gain } if *r == removal => {
                                        Some(*gain)
                                    }
                                    _ => None,
                                },
                                _ => None,
                            })
                            .ok_or_else(|| RlContractError("wide completion absent".into()))?;
                        if let Some(gain) = gain {
                            let Event::ConquerorJournalV59 { journal } = event(state, gain)? else {
                                unreachable!("validated wide gain")
                            };
                            let JournalV59::Gain {
                                gain_history_index, ..
                            } = journal.as_ref()
                            else {
                                unreachable!("validated wide gain")
                            };
                            let Event::LifeGainWideV59 { player, amount } =
                                event(state, *gain_history_index)?
                            else {
                                unreachable!("validated wide gain event")
                            };
                            if *player != source.controller {
                                return Err(RlContractError("wide beneficiary changed".into()));
                            }
                            actual_gain = Some(*amount);
                        }
                    }
                }
                conq_indices.push(capture);
                conqueror.push(ConquerorV59 {
                    occurrence: conqueror.len(),
                    loss_operations,
                    source: reference(source),
                    current_original_source: current(state, source)?,
                    original_controller: source.controller.into(),
                    losing_opponent: source.controller.opponent().into(),
                    saved_actual_amount: amount,
                    phase,
                    actual_gain,
                });
            }
            Event::FiendishPandaWideGainCaptureV59 {
                source,
                gain_history_index,
                amount,
            } => {
                use crate::fiendish_panda_v1::custody::{JournalV1 as J, RemovalV1};
                let mut phase = if held
                    .iter()
                    .any(|p| panda_capture(&p.effect) == Some(capture))
                {
                    PhaseV59::Held
                } else {
                    PhaseV59::Fresh
                };
                let mut operation_attempted = false;
                let mut actual_counters_added = 0i32;
                let published=state.engine.event_history.iter().enumerate().find_map(|(i,e)|match e {Event::FiendishPandaJournalV1 {journal}if matches!(journal.as_ref(),J::Published {capture:c,..}if*c==capture)=>Some(i as u64),_=>None});
                if let Some(publication) = published {
                    phase = PhaseV59::Published;
                    if let Some((removal, kind)) = state
                        .engine
                        .event_history
                        .iter()
                        .enumerate()
                        .find_map(|(i, e)| match e {
                            Event::FiendishPandaJournalV1 { journal } => match journal.as_ref() {
                                J::Removed {
                                    publication: p,
                                    kind,
                                    ..
                                } if *p == publication => Some((i as u64, *kind)),
                                _ => None,
                            },
                            _ => None,
                        })
                    {
                        phase = match kind {
                            RemovalV1::Resolved => PhaseV59::Resolved,
                            RemovalV1::Countered => PhaseV59::Countered,
                            RemovalV1::NoLegalTarget => {
                                return Err(RlContractError(
                                    "wide Panda counter cannot fizzle".into(),
                                ))
                            }
                        };
                        let operation = state
                            .engine
                            .event_history
                            .iter()
                            .find_map(|e| match e {
                                Event::FiendishPandaJournalV1 { journal } => match journal.as_ref()
                                {
                                    J::Closed {
                                        removal: r,
                                        operation,
                                    } if *r == removal => Some(*operation),
                                    _ => None,
                                },
                                _ => None,
                            })
                            .ok_or_else(|| {
                                RlContractError("wide Panda completion absent".into())
                            })?;
                        if let Some(operation) = operation {
                            operation_attempted = true;
                            let Event::FiendishPandaJournalV1 { journal } =
                                event(state, operation)?
                            else {
                                unreachable!("validated wide Panda operation")
                            };
                            let J::Operation { input, .. } = journal.as_ref() else {
                                unreachable!("validated wide Panda operation")
                            };
                            let start = usize::try_from(*input).map_err(|_| {
                                RlContractError("wide Panda operation exceeds usize".into())
                            })?;
                            let end = usize::try_from(operation).map_err(|_| {
                                RlContractError("wide Panda operation exceeds usize".into())
                            })?;
                            for e in &state.engine.event_history[start + 1..end] {
                                if let Event::PlusOneCountersAdded {
                                    object,
                                    zone_change_count,
                                    count,
                                    ..
                                } = e
                                {
                                    if *object == source.source
                                        && *zone_change_count == source.zone_change_count
                                    {
                                        actual_counters_added = actual_counters_added
                                            .checked_add(*count)
                                            .ok_or_else(|| {
                                                RlContractError(
                                                    "wide Panda public counter sum overflow".into(),
                                                )
                                            })?;
                                    }
                                }
                            }
                        }
                    }
                }
                panda_indices.push(capture);
                panda.push(PandaV59 {
                    occurrence: panda.len(),
                    gain_operation: ordinal(*gain_history_index)?,
                    source: reference(*source),
                    current_original_source: current(state, *source)?,
                    original_controller: source.controller.into(),
                    life_gained: *amount,
                    phase,
                    operation_attempted,
                    actual_counters_added,
                });
            }
            _ => {}
        }
    }
    let mut trigger_order = Vec::new();
    let mut positions = [0usize; 2];
    for pending in &state.engine.pending_triggers {
        let group_position = positions[pending.controller.index()];
        positions[pending.controller.index()] += 1;
        let selected = native::custody::binding(&pending.effect)
            .and_then(|at| conq_indices.iter().position(|v| *v == at))
            .map(|occurrence| (ObserverV59::Conqueror, occurrence))
            .or_else(|| {
                panda_capture(&pending.effect)
                    .and_then(|at| panda_indices.iter().position(|v| *v == at))
                    .map(|occurrence| (ObserverV59::Panda, occurrence))
            });
        if let Some((observer, occurrence)) = selected {
            trigger_order.push(TriggerOrderV59 {
                controller: pending.controller.into(),
                group_position,
                observer,
                occurrence,
            });
        }
    }
    let facts = FactsV59 {
        schema: SCHEMA.into(),
        producer_sha256: producer_sha256(),
        card_db_hash: crate::card_def::KERNEL_CARDDB_HASH,
        ordered_rules: rules(),
        life,
        conqueror,
        panda,
        trigger_order,
    };
    validate_artifact(&facts)?;
    Ok(facts)
}
pub fn validate(state: &GameState, facts: &FactsV59) -> Result<(), RlContractError> {
    if *facts != capture(state)? {
        return Err(RlContractError(
            "wide life facts differ from complete current native state".into(),
        ));
    }
    Ok(())
}

/// Supplemental shape checks never replace complete current native recapture.
pub fn validate_artifact(facts: &FactsV59) -> Result<(), RlContractError> {
    let fail = || RlContractError("wide life artifact relation changed".into());
    if facts.schema != SCHEMA
        || facts.producer_sha256 != producer_sha256()
        || facts.card_db_hash != crate::card_def::KERNEL_CARDDB_HASH
        || facts.ordered_rules != rules()
    {
        return Err(fail());
    }
    for (at, row) in facts.life.iter().enumerate() {
        if row.operation != at || row.amount <= i32::MAX.unsigned_abs() {
            return Err(fail());
        }
    }
    let current_valid = |original: &CardStableRefV1, current: &Option<CardStableRefV1>| {
        current.as_ref().is_none_or(|now| {
            now.arena_id == original.arena_id
                && now.zone_change_count == original.zone_change_count
                && now.owner == original.owner
                && now.zone == Zone::Battlefield
        })
    };
    for (at, row) in facts.conqueror.iter().enumerate() {
        if row.occurrence != at
            || row.loss_operations.is_empty()
            || row.source.controller != row.original_controller
            || row.source.zone != Zone::Battlefield
            || row.original_controller == row.losing_opponent
            || !crate::bloodthirsty_conqueror_v49::owns_definition(row.source.card_db_id)
            || !current_valid(&row.source, &row.current_original_source)
            || row
                .loss_operations
                .iter()
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                != row.loss_operations.len()
        {
            return Err(fail());
        }
        let mut amount = 0u32;
        for operation in &row.loss_operations {
            let loss = facts.life.get(*operation).ok_or_else(fail)?;
            if loss.kind != KindV59::Loss || loss.player != row.losing_opponent {
                return Err(fail());
            }
            amount = amount.checked_add(loss.amount).ok_or_else(fail)?;
        }
        if row.saved_actual_amount != amount
            || row.actual_gain
                != if row.phase == PhaseV59::Resolved {
                    Some(amount)
                } else {
                    None
                }
        {
            return Err(fail());
        }
    }
    for (at, row) in facts.panda.iter().enumerate() {
        let gain = facts.life.get(row.gain_operation).ok_or_else(fail)?;
        if row.occurrence != at
            || gain.kind != KindV59::Gain
            || gain.player != row.original_controller
            || gain.amount != row.life_gained
            || row.source.controller != row.original_controller
            || row.source.zone != Zone::Battlefield
            || !crate::fiendish_panda_v1::owns_definition(row.source.card_db_id)
            || !current_valid(&row.source, &row.current_original_source)
            || row.actual_counters_added < 0
            || row.operation_attempted != (row.phase == PhaseV59::Resolved)
            || !row.operation_attempted && row.actual_counters_added != 0
        {
            return Err(fail());
        }
    }
    let mut seen = std::collections::BTreeSet::new();
    for order in &facts.trigger_order {
        let (phase, controller) = match order.observer {
            ObserverV59::Conqueror => {
                let row = facts.conqueror.get(order.occurrence).ok_or_else(fail)?;
                (row.phase, row.original_controller)
            }
            ObserverV59::Panda => {
                let row = facts.panda.get(order.occurrence).ok_or_else(fail)?;
                (row.phase, row.original_controller)
            }
        };
        if !seen.insert((order.controller, order.group_position))
            || phase != PhaseV59::Held
            || controller != order.controller
        {
            return Err(fail());
        }
    }
    Ok(())
}

#[cfg(all(
    test,
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
mod tests {
    use super::*;
    use crate::{ids::PlayerId, next_five_public_v59::tests as fixture};
    fn paid(actor: PlayerId, hidden: &str) -> GameState {
        let mut state = fixture::ready(actor);
        // Independent private library operand is installed before genuine casts.
        crate::policy_observation_v6::tests::put(
            &mut state,
            actor.opponent(),
            hidden,
            Zone::Library,
        );
        for player in [actor, actor.opponent()] {
            crate::policy_observation_v6::tests::put(
                &mut state,
                player,
                "Herald of Eternal Dawn",
                Zone::Battlefield,
            );
        }
        let snack = fixture::cast(&mut state, actor, "Midnight Snack", None, None);
        fixture::settle(&mut state);
        fixture::cast(&mut state, actor, "Bloodthirsty Conqueror", None, None);
        fixture::settle(&mut state);
        fixture::cast(&mut state, actor, "Fiendish Panda", None, None);
        fixture::settle(&mut state);
        crate::snack_wide_life_v59::tests::gain_input(&mut state, actor);
        crate::midnight_snack_v1::tests::activate_paid(&mut state, actor, snack);
        fixture::settle(&mut state);
        state
    }
    #[test]
    fn paid_wide_public_exact_occurrences_restore_and_forgery_refusals_both_seats() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let state = paid(actor, "Forest");
            let bytes = serde_json::to_vec(&state).unwrap();
            let facts = capture(&state).unwrap();
            validate(&state, &facts).unwrap();
            validate_artifact(&facts).unwrap();
            assert_eq!(facts.life.len(), 2);
            assert_eq!(facts.life[0].kind, KindV59::Loss);
            assert_eq!(facts.life[1].kind, KindV59::Gain);
            assert_eq!(facts.life[0].amount, 2_200_000_000);
            assert_eq!(facts.conqueror.len(), 1);
            assert_eq!(facts.panda.len(), 1);
            assert_eq!(facts.conqueror[0].actual_gain, Some(2_200_000_000));
            assert_eq!(facts.conqueror[0].phase, PhaseV59::Resolved);
            assert_eq!(facts.panda[0].life_gained, 2_200_000_000);
            assert_eq!(facts.panda[0].actual_counters_added, 1);
            let restored: GameState = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(capture(&restored).unwrap(), facts);
            let serialized = serde_json::to_value(&facts).unwrap();
            let parsed: FactsV59 = serde_json::from_value(serialized.clone()).unwrap();
            assert_eq!(parsed, facts);
            let mut extra = serialized;
            extra
                .as_object_mut()
                .unwrap()
                .insert("private_world".into(), serde_json::json!({}));
            assert!(serde_json::from_value::<FactsV59>(extra).is_err());
            for variant in 0..3 {
                let mut changed = facts.clone();
                match variant {
                    0 => changed.conqueror[0].saved_actual_amount -= 1,
                    1 => changed.panda[0].gain_operation = 0,
                    _ => changed.panda[0].source.zone_change_count += 1,
                }
                assert!(validate(&state, &changed).is_err());
            }
            let text = serde_json::to_string(&facts).unwrap();
            for forbidden in [
                "event_history",
                "event_log",
                "history_index",
                "original_world",
                "inline_effect",
                "linked_borrow",
            ] {
                assert!(!text.contains(forbidden));
            }
            assert!(crate::conqueror_public_v49::capture(&state).is_err());
            assert!(crate::panda_public_v47::capture(&state).is_err());
            assert_eq!(serde_json::to_vec(&state).unwrap(), bytes);
        }
    }
    #[test]
    fn independent_prepaid_hidden_worlds_keep_wide_facts_identical_both_seats() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let a = paid(actor, "Forest");
            let b = paid(actor, "Island");
            assert_ne!(
                serde_json::to_vec(&a).unwrap(),
                serde_json::to_vec(&b).unwrap()
            );
            assert_eq!(capture(&a).unwrap(), capture(&b).unwrap());
            for viewer in [actor, actor.opponent()] {
                assert_eq!(
                    crate::next_five_public_v59::capture(&a, viewer).unwrap(),
                    crate::next_five_public_v59::capture(&b, viewer).unwrap()
                );
            }
        }
    }
}
