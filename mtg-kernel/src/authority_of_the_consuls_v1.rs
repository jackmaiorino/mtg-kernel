//! Prepared, unregistered Authority of the Consuls entry/static source.
use crate::card_def::{CardType, CARD_DEFS};
use crate::effect::{EffectOp, ExecCtx};
use crate::event::{self, CommittedEvent, ProposedEvent};
use crate::ids::{ObjectId, PlayerId};
use crate::state::{AbilitySourceContractV4, GameState, ObjectLinkV4, Zone};
use crate::trigger::{PendingTrigger, TriggerCondition};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AuthorityEntryCaptureV1 {
    pub history_index: u64,
    pub entry_history_index: u64,
    pub observer_authority_index: u64,
    pub source: AbilitySourceContractV4,
    pub entrant: ObjectLinkV4,
    pub entrant_controller: PlayerId,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AuthorityEntryOperationKindV1 {
    GainCommitted { life_gain_history_index: u64 },
    CounterCommitted,
}

fn pending_owners(state: &GameState) -> impl Iterator<Item = &PendingTrigger> {
    state
        .engine
        .pending_triggers
        .iter()
        .chain(state.legend_pending_v1.iter().flatten())
        .chain(
            state
                .pending_legend_rule_v1
                .iter()
                .flat_map(|pending| pending.waiting_triggers()),
        )
        .chain(
            state
                .standard_v1
                .iter()
                .filter_map(|standard| standard.resolution_play.as_ref())
                .flat_map(|resolution| resolution.deferred_triggers.iter()),
        )
}

fn owns_definition(card_def: u16) -> bool {
    CARD_DEFS.get(usize::from(card_def)).is_some_and(|def| {
        def.name == "Authority of the Consuls"
            && def.is_executable()
            && crate::trigger::triggers_for(card_def)
                .iter()
                .any(|ability| {
                    ability.condition == TriggerCondition::OpponentCreatureEntersV1
                        && ability.home_zone == Zone::Battlefield
                        && (ability.effect)() == EffectOp::BindAuthorityEntryGainLifeV1
                })
    })
}
fn enabled() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| {
        CARD_DEFS
            .iter()
            .enumerate()
            .any(|(id, _)| owns_definition(id as u16))
    })
}
pub(crate) fn active(state: &GameState, source: ObjectId) -> bool {
    let object = state.objects.get(source);
    object.zone == Zone::Battlefield
        && object.v4.face_index == 0
        && owns_definition(object.card_def)
        && crate::continuous_characteristics_v1::printed_abilities_active(state, source)
}

/// Transient ownership for one genuine simultaneous event batch. Static
/// replacement observers exist before entry; ETB observers exist after it.
pub(crate) struct EntryBatchV1 {
    pub(crate) observers: Vec<AbilitySourceContractV4>,
    entries: Vec<usize>,
}
impl EntryBatchV1 {
    pub(crate) fn capture(state: &GameState) -> Option<Self> {
        enabled().then(|| Self {
            observers: state
                .objects
                .iter()
                .filter(|(source, _)| active(state, *source))
                .map(|(source, _)| AbilitySourceContractV4::capture(state, source))
                .collect(),
            entries: Vec::new(),
        })
    }
    pub(crate) fn record(&mut self, index: usize) {
        self.entries.push(index);
    }
    pub(crate) fn finish(self, state: &mut GameState) {
        for index in self.entries {
            record_entry(state, index);
        }
    }
}
pub(crate) fn enters_tapped_from_observers(
    state: &GameState,
    object: ObjectId,
    controller: PlayerId,
    observers: &[AbilitySourceContractV4],
) -> bool {
    crate::engine::object_has_type(state, object, CardType::Creature)
        && observers
            .iter()
            .any(|source| source.controller != controller)
}

/// Shared by actual ZoneChange and CreateToken entry, before ETB publication.
pub(crate) fn enters_tapped(state: &GameState, object: ObjectId, controller: PlayerId) -> bool {
    enabled()
        && crate::engine::object_has_type(state, object, CardType::Creature)
        && state
            .objects
            .iter()
            .any(|(source, live)| live.controller != controller && active(state, source))
}

/// Capture only actual committed creature entries. The beneficiary and exact
/// source are fixed before later departure, control changes or event-log drain.
pub(crate) fn record_entry(state: &mut GameState, entry_history_index: usize) {
    if !enabled() {
        return;
    }
    let Some(entry) = state.engine.event_history.get(entry_history_index) else {
        return;
    };
    let Some(object) = crate::trigger::battlefield_entry_object(entry) else {
        return;
    };
    if !crate::engine::object_has_type(state, object, CardType::Creature) {
        return;
    }
    let entrant = state.objects.get(object);
    let entrant_controller = entrant.controller;
    let visibility = crate::authority_public_capture_v1::EntryVisibilityV1::capture(entrant);
    let entrant = ObjectLinkV4 {
        object,
        zone_change_count: entrant.zone_change_count,
    };
    let sources: Vec<_> = state
        .objects
        .iter()
        .filter(|(source, live)| live.controller != entrant_controller && active(state, *source))
        .map(|(source, _)| AbilitySourceContractV4::capture(state, source))
        .collect();
    if sources.is_empty() {
        return;
    }
    let observer_authority_index =
        u64::try_from(state.engine.event_history.len()).expect("bounded history");
    state
        .engine
        .event_history
        .push(CommittedEvent::AuthorityEntryObserversV1 {
            entry_history_index: u64::try_from(entry_history_index).expect("bounded history"),
            entrant,
            entrant_controller,
            visibility,
            sources: sources.clone(),
        });
    crate::authority_public_capture_v1::record_entry(state, observer_authority_index);
    for source in sources {
        let capture = AuthorityEntryCaptureV1 {
            history_index: u64::try_from(state.engine.event_history.len())
                .expect("bounded history"),
            entry_history_index: u64::try_from(entry_history_index).expect("bounded history"),
            observer_authority_index,
            source,
            entrant,
            entrant_controller,
        };
        let event = CommittedEvent::AuthorityEntryCaptureV1 { capture };
        state.engine.event_log.push(event.clone());
        state.engine.event_history.push(event);
    }
}

fn capture_at(state: &GameState, index: u64) -> Result<&AuthorityEntryCaptureV1, String> {
    let index = usize::try_from(index).map_err(|_| "entry capture index exceeds usize")?;
    let Some(CommittedEvent::AuthorityEntryCaptureV1 { capture }) =
        state.engine.event_history.get(index)
    else {
        return Err("opponent creature entry lost committed capture".into());
    };
    let entry_index =
        usize::try_from(capture.entry_history_index).map_err(|_| "entry index exceeds usize")?;
    let observer_index = usize::try_from(capture.observer_authority_index)
        .map_err(|_| "observer index exceeds usize")?;
    let Some(CommittedEvent::AuthorityEntryObserversV1 {
        entry_history_index,
        entrant,
        entrant_controller,
        sources,
        ..
    }) = state.engine.event_history.get(observer_index)
    else {
        return Err("opponent entry capture lost its independent observer authority".into());
    };
    if entry_index >= observer_index
        || observer_index >= index
        || *entry_history_index != capture.entry_history_index
        || *entrant != capture.entrant
        || *entrant_controller != capture.entrant_controller
        || sources
            .iter()
            .filter(|source| **source == capture.source)
            .count()
            != 1
    {
        return Err("opponent entry capture changed its original observer or beneficiary".into());
    }
    if usize::try_from(capture.history_index).ok() != Some(index)
        || entry_index >= index || !owns_definition(capture.source.card_def)
        || capture.source.zone != Zone::Battlefield
        || capture.source.controller == capture.entrant_controller
        || state.engine.event_history.get(entry_index).is_none_or(|event| {
            crate::trigger::battlefield_entry_object(event) != Some(capture.entrant.object)
        })
        || state.objects.try_get(capture.entrant.object).is_none_or(|live| {
            let moves = state.engine.event_history[entry_index + 1..].iter().filter(|event| {
                matches!(event, CommittedEvent::ZoneChange { object, .. } if *object == capture.entrant.object)
            }).count();
            u32::try_from(moves).ok().and_then(|moves| capture.entrant.zone_change_count.checked_add(moves))
                != Some(live.zone_change_count)
        })
    { return Err("opponent creature entry capture changed its exact occurrence".into()); }
    if let Some(CommittedEvent::CreateToken { controller, .. }) =
        state.engine.event_history.get(entry_index)
    {
        if *controller != capture.entrant_controller {
            return Err("token entry controller changed".into());
        }
    }
    Ok(capture)
}

pub(crate) fn collect(
    state: &mut GameState,
    events: &[CommittedEvent],
) -> Result<Vec<PendingTrigger>, ObjectId> {
    if !enabled() {
        return Ok(Vec::new());
    }
    validate_observed_entries(state)?;
    let mut pending = Vec::new();
    let mut used = std::collections::BTreeSet::new();
    // A mutable collection log cannot silently omit an outstanding committed
    // capture. Collection receipts authenticate each completed dispatch once.
    for event in &state.engine.event_history {
        let CommittedEvent::AuthorityEntryCaptureV1 { capture } = event else {
            continue;
        };
        let collected = state
            .engine
            .event_history
            .iter()
            .filter(|event| {
                matches!(event, CommittedEvent::AuthorityEntryCollectedV1 { capture_history_index }
                if *capture_history_index == capture.history_index)
            })
            .count();
        if collected > 1 || (collected == 0 && !events.contains(event)) {
            return Err(capture.source.source);
        }
    }
    for event in events {
        let CommittedEvent::AuthorityEntryCaptureV1 { capture } = event else {
            continue;
        };
        let invalid = || capture.source.source;
        let authority = capture_at(state, capture.history_index).map_err(|_| invalid())?;
        if authority != capture
            || !used.insert(capture.history_index)
            || state.engine.event_history.iter().any(|event| {
                matches!(event, CommittedEvent::AuthorityEntryCollectedV1 { capture_history_index }
                    if *capture_history_index == capture.history_index)
            })
        {
            return Err(invalid());
        }
        pending.push(PendingTrigger {
            source: capture.source.source,
            controller: capture.source.controller,
            source_contract: Some(capture.source),
            granted_by: None,
            effect: EffectOp::AuthorityEntryGainLifeV1 {
                capture_history_index: capture.history_index,
            },
            is_madness_offer: false,
            kicked: false,
            target_spec: crate::card_def::TargetSpec::None,
            targets: Vec::new(),
            target_contracts: Vec::new(),
            placement_ordered: false,
            optional_additional_cost_paid: None,
            paid_cost_refs: Vec::new(),
        });
    }
    for capture_history_index in used {
        state
            .engine
            .event_history
            .push(CommittedEvent::AuthorityEntryCollectedV1 {
                capture_history_index,
            });
    }
    Ok(pending)
}

pub(crate) fn validate_source(
    state: &GameState,
    effect: &EffectOp,
    source: Option<AbilitySourceContractV4>,
) -> Result<(), String> {
    let index = match effect {
        EffectOp::BindAuthorityEntryGainLifeV1 => {
            return Err("opponent entry template was not captured".into())
        }
        EffectOp::AuthorityEntryGainLifeV1 {
            capture_history_index,
        } => *capture_history_index,
        _ => return Ok(()),
    };
    let capture = capture_at(state, index)?;
    if source != Some(capture.source) || state.engine.event_history.iter().filter(|event| {
        matches!(event, CommittedEvent::AuthorityEntryCollectedV1 { capture_history_index } if *capture_history_index == index)
    }).count() != 1 { return Err("opponent entry trigger changed its captured source or collection".into()); }
    if state.engine.event_history.iter().any(|event| matches!(event,
        CommittedEvent::AuthorityEntryCompletedV1 { capture_history_index, .. } if *capture_history_index == index)) {
        return Err("completed opponent entry trigger was replayed".into());
    }
    Ok(())
}

fn publication(
    state: &GameState,
    capture_index: u64,
) -> Result<(usize, &crate::state::StackItem), String> {
    let publications: Vec<_> = state
        .engine
        .event_history
        .iter()
        .enumerate()
        .filter_map(|(index, event)| match event {
            CommittedEvent::AuthorityEntryPublishedV1 {
                capture_history_index,
                item,
            } if *capture_history_index == capture_index => Some((index, item)),
            _ => None,
        })
        .collect();
    let [published] = publications.as_slice() else {
        return Err("opponent entry trigger lost its exact publication".into());
    };
    let capture = capture_at(state, capture_index)?;
    let item = published.1;
    if item.source != capture.source.source
        || item.controller != capture.source.controller
        || item.v4.ability_source_contract != Some(capture.source)
        || item.v4.stack_item_id == crate::ids::StackItemId::default()
        || item.inline_effect
            != Some(EffectOp::AuthorityEntryGainLifeV1 {
                capture_history_index: capture_index,
            })
        || item.kind != crate::state::StackItemKind::TriggeredAbility
        || !item.targets.is_empty()
        || !item.v4.target_contracts.is_empty()
        || item.is_copy
        || item.is_flashback
        || item.madness_offer
    {
        return Err("opponent entry publication changed its original finalized trigger".into());
    }
    Ok(*published)
}

/// Called only after the genuine engine trigger publisher finalized the row.
pub(crate) fn publish(state: &mut GameState, item: &crate::state::StackItem) -> Result<(), String> {
    let Some(
        effect @ EffectOp::AuthorityEntryGainLifeV1 {
            capture_history_index,
        },
    ) = item.inline_effect.as_ref()
    else {
        return Ok(());
    };
    validate_source(state, effect, item.v4.ability_source_contract)?;
    if state.engine.event_history.iter().any(|event| matches!(event,
        CommittedEvent::AuthorityEntryPublishedV1 { capture_history_index: index, .. } if index == capture_history_index)) {
        return Err("opponent entry trigger was published twice".into());
    }
    state
        .engine
        .event_history
        .push(CommittedEvent::AuthorityEntryPublishedV1 {
            capture_history_index: *capture_history_index,
            item: item.clone(),
        });
    Ok(())
}

/// Called only at actual engine resolution/counter completion after removal.
pub(crate) fn finish(state: &mut GameState, item: &crate::state::StackItem) {
    let Some(
        effect @ EffectOp::AuthorityEntryGainLifeV1 {
            capture_history_index,
        },
    ) = item.inline_effect.as_ref()
    else {
        return;
    };
    if state.engine.halted.is_some() {
        return;
    }
    if validate_source(state, effect, item.v4.ability_source_contract).is_err()
        || item.v4.stack_item_id == crate::ids::StackItemId::default()
        || publication(state, *capture_history_index).map_or(true, |(_, original)| original != item)
    {
        state.engine.halted = Some((
            crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
            item.source,
        ));
        return;
    }
    let operations: Vec<_> = state
        .engine
        .event_history
        .iter()
        .enumerate()
        .filter_map(|(index, event)| {
            matches!(event, CommittedEvent::AuthorityEntryOperationV1 {
            capture_history_index: capture, stack_item_id, source, ..
        } if capture == capture_history_index && *stack_item_id == item.v4.stack_item_id
            && Some(*source) == item.v4.ability_source_contract)
            .then_some(index)
        })
        .collect();
    let [operation_index] = operations.as_slice() else {
        state.engine.halted = Some((
            crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
            item.source,
        ));
        return;
    };
    if validate_operation(state, *operation_index).is_err() {
        state.engine.halted = Some((
            crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
            item.source,
        ));
        return;
    }
    state
        .engine
        .event_history
        .push(CommittedEvent::AuthorityEntryCompletedV1 {
            capture_history_index: *capture_history_index,
            stack_item_id: item.v4.stack_item_id,
            operation_history_index: u64::try_from(*operation_index).expect("bounded history"),
        });
}

/// The accepted counter primitive calls this with its validated removed item.
/// A completion marker alone cannot impersonate this committed operation.
pub(crate) fn record_countered(state: &mut GameState, item: &crate::state::StackItem) {
    let Some(EffectOp::AuthorityEntryGainLifeV1 {
        capture_history_index,
    }) = item.inline_effect.as_ref()
    else {
        return;
    };
    let Some(source) = item.v4.ability_source_contract else {
        return;
    };
    if publication(state, *capture_history_index).map_or(true, |(_, original)| original != item) {
        state.engine.halted = Some((
            crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
            item.source,
        ));
        return;
    }
    state
        .engine
        .event_history
        .push(CommittedEvent::AuthorityEntryOperationV1 {
            capture_history_index: *capture_history_index,
            stack_item_id: item.v4.stack_item_id,
            source,
            kind: AuthorityEntryOperationKindV1::CounterCommitted,
        });
}

pub(crate) fn resolve(state: &mut GameState, ctx: &ExecCtx, capture_history_index: u64) {
    let effect = EffectOp::AuthorityEntryGainLifeV1 {
        capture_history_index,
    };
    if validate_source(state, &effect, ctx.ability_source_contract).is_err() {
        state.engine.halted = Some((
            crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
            ctx.source,
        ));
        return;
    }
    let capture = capture_at(state, capture_history_index).unwrap();
    if ctx.source != capture.source.source
        || ctx.controller != capture.source.controller
        || !ctx.targets.is_empty()
        || publication(state, capture_history_index).map_or(true, |(_, item)| {
            Some(item.v4.stack_item_id) != ctx.stack_item_id
                || item.v4.ability_source_contract != ctx.ability_source_contract
                || item.v4.target_contracts != ctx.target_contracts
        })
        || ctx
            .stack_item_id
            .is_none_or(|id| id == crate::ids::StackItemId::default())
    {
        state.engine.halted = Some((
            crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
            ctx.source,
        ));
        return;
    }
    let source = capture.source;
    let start = state.engine.event_history.len();
    event::propose_and_commit(state, ProposedEvent::life_gain(ctx.controller, 1));
    let gain_index = state.engine.event_history[start..]
        .iter()
        .position(|event| {
            matches!(event,
        CommittedEvent::LifeGain { player, amount } if *player == ctx.controller && *amount == 1)
        });
    let Some(gain_index) = gain_index else {
        state.engine.halted = Some((
            crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
            ctx.source,
        ));
        return;
    };
    state
        .engine
        .event_history
        .push(CommittedEvent::AuthorityEntryOperationV1 {
            capture_history_index,
            stack_item_id: ctx.stack_item_id.unwrap(),
            source,
            kind: AuthorityEntryOperationKindV1::GainCommitted {
                life_gain_history_index: u64::try_from(start + gain_index)
                    .expect("bounded history"),
            },
        });
}

fn validate_operation(state: &GameState, index: usize) -> Result<(), String> {
    let Some(CommittedEvent::AuthorityEntryOperationV1 {
        capture_history_index,
        stack_item_id,
        source,
        kind,
    }) = state.engine.event_history.get(index)
    else {
        return Err("completion lost its committed gain/counter operation".into());
    };
    let capture = capture_at(state, *capture_history_index)?;
    let (published_index, published) = publication(state, *capture_history_index)?;
    let collections: Vec<_> = state.engine.event_history.iter().enumerate().filter_map(|(index, event)| {
        matches!(event, CommittedEvent::AuthorityEntryCollectedV1 { capture_history_index: capture }
            if capture == capture_history_index).then_some(index)
    }).collect();
    let [collection] = collections.as_slice() else {
        return Err("operation lost its exact collection".into());
    };
    if *collection >= index
        || published_index <= *collection
        || published_index >= index
        || published.v4.stack_item_id != *stack_item_id
        || *source != capture.source
        || *stack_item_id == crate::ids::StackItemId::default()
    {
        return Err("operation changed its original source/stack/collection occurrence".into());
    }
    if let AuthorityEntryOperationKindV1::GainCommitted {
        life_gain_history_index,
    } = kind
    {
        let gain =
            usize::try_from(*life_gain_history_index).map_err(|_| "gain index exceeds usize")?;
        if gain <= published_index
            || gain >= index
            || !matches!(state.engine.event_history.get(gain),
            Some(CommittedEvent::LifeGain { player, amount }) if *player == source.controller && *amount == 1)
        {
            return Err("resolution operation lost its actual committed one-life gain".into());
        }
    }
    Ok(())
}

pub(crate) fn requires_successor(state: &GameState) -> bool {
    if !enabled() {
        return false;
    }
    state
        .objects
        .iter()
        .any(|(source, _)| active(state, source))
        || state.stack.iter().any(|item| {
            item.inline_effect.as_ref().is_some_and(|op| {
                matches!(
                    op,
                    EffectOp::BindAuthorityEntryGainLifeV1
                        | EffectOp::AuthorityEntryGainLifeV1 { .. }
                )
            })
        })
        || pending_owners(state).any(|pending| {
            matches!(
                pending.effect,
                EffectOp::BindAuthorityEntryGainLifeV1 | EffectOp::AuthorityEntryGainLifeV1 { .. }
            )
        })
        || state
            .engine
            .event_log
            .iter()
            .any(|event| matches!(event, CommittedEvent::AuthorityEntryCaptureV1 { .. }))
}

/// Common read-only action/restore boundary. Mutable pending and stack rows
/// must retain the independently committed original observer capture.
pub(crate) fn validate_state(state: &GameState) -> Result<(), String> {
    if !enabled() {
        return Ok(());
    }
    crate::authority_public_capture_v1::validate_facts(state)?;
    validate_observed_entries(state)
        .map_err(|_| "opponent entry observer authority is incomplete")?;
    for event in &state.engine.event_history {
        let CommittedEvent::AuthorityEntryCaptureV1 { capture } = event else {
            continue;
        };
        if capture_at(state, capture.history_index)? != capture {
            return Err("opponent entry capture changed".into());
        }
        let collected = state
            .engine
            .event_history
            .iter()
            .filter(|event| {
                matches!(event,
            CommittedEvent::AuthorityEntryCollectedV1 { capture_history_index }
                if *capture_history_index == capture.history_index)
            })
            .count();
        if collected > 1
            || (collected == 1 && state.engine.event_log.iter().any(|logged| logged == event))
            || (collected == 0
                && state
                    .engine
                    .event_log
                    .iter()
                    .filter(|logged| *logged == event)
                    .count()
                    != 1)
        {
            return Err("opponent entry capture collection is missing or repeated".into());
        }
        let completed = state
            .engine
            .event_history
            .iter()
            .filter(|event| {
                matches!(event,
            CommittedEvent::AuthorityEntryCompletedV1 { capture_history_index, .. }
            if *capture_history_index == capture.history_index)
            })
            .count();
        let operations = state
            .engine
            .event_history
            .iter()
            .filter(|event| {
                matches!(event,
            CommittedEvent::AuthorityEntryOperationV1 { capture_history_index, .. }
            if *capture_history_index == capture.history_index)
            })
            .count();
        let pending = pending_owners(state)
            .filter(|pending| {
                matches!(pending.effect,
            EffectOp::AuthorityEntryGainLifeV1 { capture_history_index }
            if capture_history_index == capture.history_index)
            })
            .count();
        let live = state
            .stack
            .iter()
            .filter(|item| {
                matches!(item.inline_effect,
            Some(EffectOp::AuthorityEntryGainLifeV1 { capture_history_index })
            if capture_history_index == capture.history_index)
            })
            .count();
        let publications = state
            .engine
            .event_history
            .iter()
            .filter(|event| {
                matches!(event,
            CommittedEvent::AuthorityEntryPublishedV1 { capture_history_index, .. }
            if *capture_history_index == capture.history_index)
            })
            .count();
        if completed > 1
            || operations != completed
            || publications != usize::from(live + completed > 0)
            || (collected == 0 && (completed != 0 || pending + live != 0))
            || (collected == 1 && pending + live != usize::from(completed == 0))
        {
            return Err("opponent entry trigger lost or replayed its dispatch ownership".into());
        }
    }
    for (completion_index, event) in state.engine.event_history.iter().enumerate() {
        if let CommittedEvent::AuthorityEntryCompletedV1 {
            capture_history_index,
            stack_item_id,
            operation_history_index,
        } = event
        {
            capture_at(state, *capture_history_index)?;
            let operation_index = usize::try_from(*operation_history_index)
                .map_err(|_| "operation index exceeds usize")?;
            validate_operation(state, operation_index)?;
            if operation_index >= completion_index
                || !matches!(state.engine.event_history.get(operation_index),
                Some(CommittedEvent::AuthorityEntryOperationV1 { capture_history_index: capture, stack_item_id: id, .. })
                if capture == capture_history_index && id == stack_item_id)
            {
                return Err("completion changed its exact committed operation".into());
            }
        }
        if matches!(event, CommittedEvent::AuthorityEntryOperationV1 { .. }) {
            validate_operation(state, completion_index)?;
        }
        if let CommittedEvent::AuthorityEntryPublishedV1 {
            capture_history_index,
            ..
        } = event
        {
            let (index, _) = publication(state, *capture_history_index)?;
            if index != completion_index || state.engine.event_history[..index].iter().filter(|event|
                matches!(event, CommittedEvent::AuthorityEntryCollectedV1 { capture_history_index: capture }
                    if capture == capture_history_index)).count() != 1 {
                return Err("opponent entry publication lost its prior collection".into());
            }
        }
    }
    if state.engine.event_log.iter().any(|event| {
        matches!(
            event,
            CommittedEvent::AuthorityEntryCompletedV1 { .. }
                | CommittedEvent::AuthorityEntryOperationV1 { .. }
                | CommittedEvent::AuthorityEntryCollectedV1 { .. }
                | CommittedEvent::AuthorityEntryPublishedV1 { .. }
        )
    }) {
        return Err("opponent entry completion was inserted into dispatch log".into());
    }
    for event in &state.engine.event_log {
        if let CommittedEvent::AuthorityEntryCaptureV1 { capture } = event {
            if capture_at(state, capture.history_index)? != capture {
                return Err("dispatch log contains a forged opponent entry capture".into());
            }
        }
    }
    for pending in pending_owners(state) {
        if let EffectOp::AuthorityEntryGainLifeV1 {
            capture_history_index,
        } = &pending.effect
        {
            validate_source(state, &pending.effect, pending.source_contract)?;
            let capture = capture_at(state, *capture_history_index)?;
            if pending.source != capture.source.source
                || pending.controller != capture.source.controller
            {
                return Err("opponent entry pending trigger changed its beneficiary".into());
            }
            crate::engine::validate_pending_trigger_identity(state, pending)?;
            if pending.placement_ordered {
                crate::engine::validate_pending_trigger(state, pending)?;
            }
        }
    }
    for item in &state.stack {
        if let Some(EffectOp::AuthorityEntryGainLifeV1 {
            capture_history_index,
        }) = &item.inline_effect
        {
            let capture = capture_at(state, *capture_history_index)?;
            if publication(state, *capture_history_index)?.1 != item {
                return Err("live opponent entry trigger changed its committed publication".into());
            }
            if item.source != capture.source.source || item.controller != capture.source.controller
            {
                return Err("opponent entry stack trigger changed its beneficiary".into());
            }
            crate::engine::validated_stack_item_target_spec(item, state)?;
        }
    }
    Ok(())
}

fn validate_observed_entries(state: &GameState) -> Result<(), ObjectId> {
    if state
        .engine
        .event_log
        .iter()
        .any(|event| matches!(event, CommittedEvent::AuthorityEntryObserversV1 { .. }))
    {
        return Err(ObjectId(0));
    }
    for (index, event) in state.engine.event_history.iter().enumerate() {
        let CommittedEvent::AuthorityEntryObserversV1 {
            entry_history_index,
            entrant,
            entrant_controller,
            sources,
            ..
        } = event
        else {
            continue;
        };
        if sources.is_empty()
            || usize::try_from(*entry_history_index)
                .ok()
                .is_none_or(|entry| {
                    entry >= index
                        || state.engine.event_history.get(entry).is_none_or(|event| {
                            crate::trigger::battlefield_entry_object(event) != Some(entrant.object)
                        })
                })
        {
            return Err(sources.first().map_or(ObjectId(0), |source| source.source));
        }
        for (source_index, source) in sources.iter().enumerate() {
            if !owns_definition(source.card_def)
                || source.zone != Zone::Battlefield
                || source.controller == *entrant_controller
                || sources[..source_index]
                    .iter()
                    .any(|prior| prior.source == source.source)
            {
                return Err(source.source);
            }
            let captures: Vec<_> = state
                .engine
                .event_history
                .iter()
                .filter_map(|event| match event {
                    CommittedEvent::AuthorityEntryCaptureV1 { capture }
                        if usize::try_from(capture.observer_authority_index).ok()
                            == Some(index)
                            && capture.source.source == source.source =>
                    {
                        Some(capture)
                    }
                    _ => None,
                })
                .collect();
            if captures.len() != 1
                || capture_at(state, captures[0].history_index).ok() != Some(captures[0])
            {
                return Err(source.source);
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
