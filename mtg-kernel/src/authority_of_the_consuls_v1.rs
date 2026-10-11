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
    pub source: AbilitySourceContractV4,
    pub entrant: ObjectLinkV4,
    pub entrant_controller: PlayerId,
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
fn active(state: &GameState, source: ObjectId) -> bool {
    let object = state.objects.get(source);
    object.zone == Zone::Battlefield
        && object.v4.face_index == 0
        && owns_definition(object.card_def)
        && crate::continuous_characteristics_v1::printed_abilities_active(state, source)
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
    let entrant = ObjectLinkV4 {
        object,
        zone_change_count: entrant.zone_change_count,
    };
    let sources: Vec<_> = state
        .objects
        .iter()
        .filter_map(|(source, live)| {
            (live.controller != entrant_controller && active(state, source))
                .then(|| AbilitySourceContractV4::capture(state, source))
        })
        .collect();
    for source in sources {
        let capture = AuthorityEntryCaptureV1 {
            history_index: u64::try_from(state.engine.event_history.len())
                .expect("bounded history"),
            entry_history_index: u64::try_from(entry_history_index).expect("bounded history"),
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
    Ok(())
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
    {
        state.engine.halted = Some((
            crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
            ctx.source,
        ));
        return;
    }
    event::propose_and_commit(state, ProposedEvent::life_gain(ctx.controller, 1));
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
        || state.engine.pending_triggers.iter().any(|pending| {
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
    }
    for pending in &state.engine.pending_triggers {
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
            if item.source != capture.source.source || item.controller != capture.source.controller
            {
                return Err("opponent entry stack trigger changed its beneficiary".into());
            }
            crate::engine::validated_stack_item_target_spec(item, state)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
