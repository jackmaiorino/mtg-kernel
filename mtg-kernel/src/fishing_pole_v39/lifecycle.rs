//! Original named-cost and untap custody. Private loans are never serialized.
use super::selected_definition;
use crate::card_def::{CardType, Keywords, Subtype, TargetSpec, CARD_DEFS};
use crate::effect::{EffectOp, ExecCtx};
use crate::event::{CommittedEvent, ProposedEvent};
use crate::ids::{ObjectId, StackItemId};
use crate::state::{AbilitySourceContractV4, GameState, StackItem, StackItemKind, Zone};
use crate::trigger::PendingTrigger;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum JournalV39 {
    Untapped {
        host: AbilitySourceContractV4,
        poles: Vec<AbilitySourceContractV4>,
    },
    Captured {
        untap: u64,
        pole: AbilitySourceContractV4,
    },
    Collected {
        capture: u64,
    },
    PaymentInput {
        host: AbilitySourceContractV4,
        /// Event-time public mask of the actual host, not its hidden printing.
        host_face_down: Option<bool>,
        pole: AbilitySourceContractV4,
        ability_index: u8,
        taps: [u64; 2],
    },
    Published {
        origin: u64,
        item: Box<StackItem>,
    },
    OriginalRemoval {
        origin: u64,
        item: Box<StackItem>,
        countered: bool,
        bait: Option<i32>,
        departure: Option<u64>,
    },
    Removed {
        input: u64,
        origin: u64,
        item: Box<StackItem>,
        countered: bool,
        bait: Option<i32>,
        departure: Option<u64>,
    },
    BaitChanged {
        origin: u64,
        pole: AbilitySourceContractV4,
        before: i32,
        after: i32,
    },
    Completed {
        origin: u64,
        removal: u64,
        change: Option<u64>,
        token: Option<u64>,
    },
}
fn at(state: &GameState, index: u64) -> Result<&JournalV39, String> {
    let index = usize::try_from(index).map_err(|_| "Fishing journal index overflow")?;
    match state.engine.event_history.get(index) {
        Some(CommittedEvent::FishingPoleJournalV39 { journal }) => Ok(journal),
        _ => Err("Fishing original journal absent".into()),
    }
}
fn append(state: &mut GameState, journal: JournalV39) -> u64 {
    let index = state.engine.event_history.len() as u64;
    state
        .engine
        .event_history
        .push(CommittedEvent::FishingPoleJournalV39 {
            journal: Box::new(journal),
        });
    index
}
fn source(state: &GameState, original: AbilitySourceContractV4) -> Result<(), String> {
    if original.zone != Zone::Battlefield || !selected_definition(original.card_def) {
        return Err("Fishing source is not the original battlefield Pole".into());
    }
    crate::engine::validate_historical_ability_source_contract(state, original)
}
fn capture(state: &GameState, index: u64) -> Result<AbilitySourceContractV4, String> {
    let JournalV39::Captured { untap, pole } = at(state, index)? else {
        return Err("Fishing trigger lost original untap capture".into());
    };
    let JournalV39::Untapped { host, poles } = at(state, *untap)? else {
        return Err("Fishing trigger lost independent original untap".into());
    };
    if *untap >= index
        || poles.iter().filter(|original| *original == pole).count() != 1
        || pole.source == host.source
        || pole.attached_to
            != Some(crate::state::ObjectLinkV4 {
                object: host.source,
                zone_change_count: host.zone_change_count,
            })
    {
        return Err("Fishing untap changed original host/observer/controller".into());
    }
    source(state, *pole)?;
    crate::engine::validate_historical_ability_source_contract(state, *host)?;
    Ok(*pole)
}
fn origin_pole(state: &GameState, origin: u64) -> Result<AbilitySourceContractV4, String> {
    match at(state, origin)? {
        JournalV39::Captured { .. } => capture(state, origin),
        JournalV39::PaymentInput {
            host, pole, taps, ..
        } => {
            source(state, *pole)?;
            crate::engine::validate_historical_ability_source_contract(state, *host)?;
            if host.zone != Zone::Battlefield
                || host.controller != pole.controller
                || host.source == pole.source
                || pole.attached_to
                    != Some(crate::state::ObjectLinkV4 {
                        object: host.source,
                        zone_change_count: host.zone_change_count,
                    })
                || taps[0] >= taps[1]
                || taps[1] >= origin
                || !matches!(state.engine.event_history.get(taps[0] as usize), Some(CommittedEvent::Tap {object}) if *object == host.source)
                || !matches!(state.engine.event_history.get(taps[1] as usize), Some(CommittedEvent::Tap {object}) if *object == pole.source)
            {
                return Err("Fishing payment lost exact two original taps and grant".into());
            }
            Ok(*pole)
        }
        _ => Err("Fishing publication has no capture/payment input".into()),
    }
}
fn effect_origin(effect: &EffectOp) -> Option<u64> {
    match effect {
        EffectOp::FishingPoleUntapV39 {
            capture_history_index,
        } => Some(*capture_history_index),
        _ => None,
    }
}
pub(crate) fn validate_source(
    state: &GameState,
    effect: &EffectOp,
    contract: Option<AbilitySourceContractV4>,
) -> Result<(), String> {
    if matches!(effect, EffectOp::BindFishingPoleUntapV39) {
        return Err("Fishing printed template lacks actual untap capture".into());
    }
    if let Some(index) = effect_origin(effect) {
        if contract != Some(capture(state, index)?) {
            return Err("Fishing trigger beneficiary/source rewrite".into());
        }
    }
    Ok(())
}
/// Called only after an actual true-to-false tapped transition, before later
/// untaps, SBA or collection. Host ability suppression cannot erase the Pole's
/// own printed trigger; suppression of the Pole itself can.
pub(crate) fn record_untap(state: &mut GameState, host: AbilitySourceContractV4) {
    let poles = state
        .objects
        .iter()
        .filter(|(id, live)| {
            live.zone == Zone::Battlefield
                && selected_definition(live.card_def)
                && live.v4.attached_to
                    == Some(crate::state::ObjectLinkV4 {
                        object: host.source,
                        zone_change_count: host.zone_change_count,
                    })
                && crate::engine::object_has_type(state, host.source, CardType::Creature)
                && crate::continuous_characteristics_v1::printed_abilities_active(state, *id)
        })
        .map(|(id, _)| AbilitySourceContractV4::capture(state, id))
        .collect::<Vec<_>>();
    if poles.is_empty() {
        return;
    }
    let untap = append(
        state,
        JournalV39::Untapped {
            host,
            poles: poles.clone(),
        },
    );
    for pole in poles {
        let event = CommittedEvent::FishingPoleJournalV39 {
            journal: Box::new(JournalV39::Captured { untap, pole }),
        };
        state.engine.event_history.push(event.clone());
        state.engine.event_log.push(event);
    }
}
pub(crate) fn collect(
    state: &mut GameState,
    events: &[CommittedEvent],
) -> Result<Vec<PendingTrigger>, ObjectId> {
    let mut pending = Vec::new();
    for event in events {
        let CommittedEvent::FishingPoleJournalV39 { journal } = event else {
            continue;
        };
        let JournalV39::Captured { pole, .. } = journal.as_ref() else {
            return Err(ObjectId(0));
        };
        let occurrences = state
            .engine
            .event_history
            .iter()
            .enumerate()
            .filter(|(_, original)| *original == event)
            .map(|(i, _)| i as u64)
            .collect::<Vec<_>>();
        let [index] = occurrences.as_slice() else {
            return Err(pole.source);
        };
        if capture(state, *index).is_err() || state.engine.event_history.iter().any(|e| matches!(e,
            CommittedEvent::FishingPoleJournalV39 {journal} if matches!(journal.as_ref(),JournalV39::Collected {capture} if *capture == *index))) {
            return Err(pole.source);
        }
        pending.push(PendingTrigger {
            controller: pole.controller,
            source: pole.source,
            effect: EffectOp::FishingPoleUntapV39 {
                capture_history_index: *index,
            },
            source_contract: Some(*pole),
            target_spec: TargetSpec::None,
            is_madness_offer: false,
            kicked: false,
            targets: Vec::new(),
            target_contracts: Vec::new(),
            placement_ordered: false,
            granted_by: None,
            optional_additional_cost_paid: None,
            paid_cost_refs: Vec::new(),
        });
        append(state, JournalV39::Collected { capture: *index });
    }
    Ok(pending)
}
fn publications(state: &GameState, origin: u64) -> Vec<(u64, &StackItem)> {
    state
        .engine
        .event_history
        .iter()
        .enumerate()
        .filter_map(|(i, event)| match event {
            CommittedEvent::FishingPoleJournalV39 { journal } => match journal.as_ref() {
                JournalV39::Published {
                    origin: original,
                    item,
                } if *original == origin => Some((i as u64, item.as_ref())),
                _ => None,
            },
            _ => None,
        })
        .collect()
}
fn shape(state: &GameState, origin: u64, item: &StackItem) -> Result<(), String> {
    let pole = origin_pole(state, origin)?;
    if item.v4.stack_item_id == StackItemId::default()
        || item.is_copy
        || item.is_flashback
        || item.kicked
        || item.madness_offer
        || !item.targets.is_empty()
        || !item.v4.target_contracts.is_empty()
        || !item.discarded.is_empty()
        || item.mode_chosen != 0
        || item.v4.cauldron_grant.0.is_some()
        || item.v4.source_contract.is_some()
        || item.v4.optional_additional_cost_paid.is_some()
    {
        return Err("Fishing finalized metadata changed".into());
    }
    match at(state, origin)? {
        JournalV39::Captured { .. } => {
            if item.kind != StackItemKind::TriggeredAbility
                || item.source != pole.source
                || item.controller != pole.controller
                || item.v4.ability_source_contract != Some(pole)
                || item.v4.granted_by.is_some()
                || item.v4.activated_ability_index.is_some()
                || item.v4.target_spec.is_some()
                || item.inline_effect.as_ref().and_then(effect_origin) != Some(origin)
                || !item.v4.paid_cost_refs.is_empty()
            {
                return Err("Fishing mandatory trigger changed its captured root".into());
            }
        }
        JournalV39::PaymentInput {
            host,
            ability_index,
            ..
        } => {
            if item.kind != StackItemKind::ActivatedAbility
                || item.source != host.source
                || item.controller != host.controller
                || item.v4.ability_source_contract != Some(*host)
                || item.v4.granted_by != Some(pole)
                || item.v4.activated_ability_index != Some(*ability_index)
                || item.v4.target_spec != Some(TargetSpec::None)
                || item.inline_effect != Some(EffectOp::FishingPoleAddBaitV39)
                || item.v4.paid_cost_refs.len() != 1
                || item.v4.paid_cost_refs[0].object != pole.source
            {
                return Err("Fishing bait activation changed original named grant/payment".into());
            }
        }
        _ => unreachable!(),
    }
    let expected = crate::state::StackStateV4 {
        stack_item_id: item.v4.stack_item_id,
        ability_source_contract: item.v4.ability_source_contract,
        activated_ability_index: item.v4.activated_ability_index,
        target_spec: item.v4.target_spec,
        granted_by: item.v4.granted_by,
        paid_cost_refs: item.v4.paid_cost_refs.clone(),
        ..Default::default()
    };
    if item.v4 != expected {
        return Err("Fishing unrelated finalized authority metadata changed".into());
    }
    if let JournalV39::PaymentInput { .. } = at(state, origin)? {
        let paid = &item.v4.paid_cost_refs[0];
        if paid.card_def != pole.card_def
            || paid.owner != pole.owner
            || paid.controller != pole.controller
            || paid.zone != Zone::Battlefield
            || paid.zone_change_count != pole.zone_change_count
            || paid.visible_to_mask != 3
            || paid.power_lki.is_some()
        {
            return Err("Fishing named payment incarnation/owner/visibility changed".into());
        }
    }
    Ok(())
}
pub(crate) fn publish_trigger(state: &mut GameState, item: &StackItem) -> Result<(), String> {
    let Some(origin) = item.inline_effect.as_ref().and_then(effect_origin) else {
        return Ok(());
    };
    publish(state, origin, item)
}
fn publish(state: &mut GameState, origin: u64, item: &StackItem) -> Result<(), String> {
    shape(state, origin, item)?;
    if !publications(state, origin).is_empty()
        || state.stack.iter().filter(|row| *row == item).count() != 1
    {
        return Err("Fishing publication lacks unique original live row".into());
    }
    append(
        state,
        JournalV39::Published {
            origin,
            item: Box::new(item.clone()),
        },
    );
    Ok(())
}
pub(crate) fn record_paid(
    state: &mut GameState,
    id: StackItemId,
    start: usize,
    host: AbilitySourceContractV4,
    pole: Option<AbilitySourceContractV4>,
    ability_index: u8,
) -> Result<(), String> {
    let item = state
        .stack
        .iter()
        .find(|item| item.v4.stack_item_id == id)
        .ok_or("Fishing paid row absent")?
        .clone();
    if item.inline_effect != Some(EffectOp::FishingPoleAddBaitV39) {
        return Ok(());
    }
    let pole = pole.ok_or("Fishing selected grant absent")?;
    let tap = |object| {
        state.engine.event_history[start..]
            .iter()
            .enumerate()
            .filter_map(|(i, e)| {
                matches!(e,CommittedEvent::Tap {object: tapped} if *tapped == object)
                    .then_some((start + i) as u64)
            })
            .collect::<Vec<_>>()
    };
    let host_taps = tap(host.source);
    let pole_taps = tap(pole.source);
    let ([host_tap], [pole_tap]) = (host_taps.as_slice(), pole_taps.as_slice()) else {
        return Err("Fishing payment lacks unique original taps".into());
    };
    let host_face_down = state
        .objects
        .get(host.source)
        .v4
        .face_down_v1
        .map(|face| face.disguised);
    let origin = append(
        state,
        JournalV39::PaymentInput {
            host,
            host_face_down,
            pole,
            ability_index,
            taps: [*host_tap, *pole_tap],
        },
    );
    publish(state, origin, &item)
}
fn tracked(item: &StackItem) -> bool {
    item.inline_effect == Some(EffectOp::FishingPoleAddBaitV39)
        || item
            .inline_effect
            .as_ref()
            .and_then(effect_origin)
            .is_some()
        || item
            .v4
            .granted_by
            .is_some_and(|pole| selected_definition(pole.card_def))
}
pub(crate) fn validate_item(state: &GameState, item: &StackItem) -> Result<(), String> {
    let pubs = state
        .engine
        .event_history
        .iter()
        .filter_map(|event| match event {
            CommittedEvent::FishingPoleJournalV39 { journal } => match journal.as_ref() {
                JournalV39::Published {
                    origin,
                    item: original,
                } if original.v4.stack_item_id == item.v4.stack_item_id => {
                    Some((*origin, original.as_ref()))
                }
                _ => None,
            },
            _ => None,
        })
        .collect::<Vec<_>>();
    if !tracked(item) && pubs.is_empty() {
        return Ok(());
    }
    let [(origin, original)] = pubs.as_slice() else {
        return Err("Fishing live row lacks original publication".into());
    };
    shape(state, *origin, item)?;
    if *original != item || state.engine.event_history.iter().any(|event| matches!(event,
        CommittedEvent::FishingPoleJournalV39 {journal} if matches!(journal.as_ref(),JournalV39::Removed {origin: removed,..} if *removed == *origin))) {
        return Err("Fishing live row changed or replayed".into());
    }
    Ok(())
}
/// Only actual Fishing publications acquire historical public source transport.
/// A host's hidden printing is never inferred from a newer arena incarnation.
pub(crate) fn public_stack_source(
    state: &GameState,
    item: &StackItem,
) -> Result<Option<(AbilitySourceContractV4, Option<bool>)>, String> {
    if !tracked(item) {
        return Ok(None);
    }
    validate_state(state)?;
    validate_item(state, item)?;
    let origins = state
        .engine
        .event_history
        .iter()
        .filter_map(|event| match event {
            CommittedEvent::FishingPoleJournalV39 { journal } => match journal.as_ref() {
                JournalV39::Published {
                    origin,
                    item: original,
                } if original.as_ref() == item => Some(*origin),
                _ => None,
            },
            _ => None,
        })
        .collect::<Vec<_>>();
    let [origin] = origins.as_slice() else {
        return Err("Fishing historical projection lost unique original publication".into());
    };
    match at(state, *origin)? {
        JournalV39::Captured { .. } => Ok(Some((origin_pole(state, *origin)?, None))),
        JournalV39::PaymentInput {
            host,
            host_face_down,
            ..
        } => Ok(Some((*host, *host_face_down))),
        _ => Err("Fishing historical projection lost original source visibility".into()),
    }
}
pub(crate) struct OwnerV39 {
    origin: u64,
    item: StackItem,
}
pub(crate) fn acquire(state: &GameState, item: &StackItem) -> Result<Option<OwnerV39>, String> {
    if !tracked(item) {
        return Ok(None);
    }
    validate_item(state, item)?;
    if state
        .stack
        .iter()
        .filter(|row| row.v4.stack_item_id == item.v4.stack_item_id && *row == item)
        .count()
        != 1
    {
        return Err("Fishing private loan lacks unique actual live row".into());
    }
    let origin = state
        .engine
        .event_history
        .iter()
        .find_map(|e| match e {
            CommittedEvent::FishingPoleJournalV39 { journal } => match journal.as_ref() {
                JournalV39::Published {
                    origin,
                    item: original,
                } if original.as_ref() == item => Some(*origin),
                _ => None,
            },
            _ => None,
        })
        .ok_or("Fishing original publication absent")?;
    Ok(Some(OwnerV39 {
        origin,
        item: item.clone(),
    }))
}
fn same_pole(state: &GameState, pole: AbilitySourceContractV4) -> bool {
    state.objects.try_get(pole.source).is_some_and(|live| {
        live.zone == Zone::Battlefield && live.zone_change_count == pole.zone_change_count
    })
}
fn departure(state: &GameState, pole: AbilitySourceContractV4, since: u64) -> Option<u64> {
    state.engine.event_history.iter().enumerate().skip(since as usize).find_map(|(i,e)|
        matches!(e,CommittedEvent::ZoneChange {object,from:Zone::Battlefield,..} if *object==pole.source).then_some(i as u64))
}
pub(crate) fn removed_input(
    state: &mut GameState,
    owner: Option<&OwnerV39>,
    countered: bool,
) -> Result<(), String> {
    let Some(owner) = owner else {
        return Ok(());
    };
    if state
        .stack
        .iter()
        .any(|item| item.v4.stack_item_id == owner.item.v4.stack_item_id)
    {
        return Err("Fishing operation still owns live row".into());
    }
    let pole = origin_pole(state, owner.origin)?;
    let bait = same_pole(state, pole).then(|| state.objects.get(pole.source).counters.bait);
    let departed = (!same_pole(state, pole))
        .then(|| departure(state, pole, owner.origin))
        .flatten();
    if bait.is_none() && departed.is_none() {
        return Err("Fishing no-op lacks actual original Pole departure".into());
    }
    append(
        state,
        JournalV39::OriginalRemoval {
            origin: owner.origin,
            item: Box::new(owner.item.clone()),
            countered,
            bait,
            departure: departed,
        },
    );
    Ok(())
}
pub(crate) fn fish_definition() -> Result<u16, String> {
    let id = crate::card_def::card_id_by_name("Fish Token")
        .ok_or("Fishing requires actual no-ability Fish Token")?;
    let d = &CARD_DEFS[usize::from(id)];
    if d.object_name != "Fish"
        || !d.is_executable()
        || !d.has_full_support()
        || !d.is_token
        || d.is_land
        || d.types != [CardType::Creature]
        || d.subtypes != [Subtype::Fish]
        || d.colors != [crate::mana::ManaColor::U]
        || (d.power, d.toughness) != (Some(1), Some(1))
        || d.keywords != Keywords::NONE
        || d.mana_value != 0
        || d.cost.generic != 0
        || d.cost.x_count != 0
        || !d.cost.pips.is_empty()
        || !d.supertypes.is_empty()
        || !d.activated_abilities.is_empty()
        || !crate::trigger::triggers_for(id).is_empty()
        || !d.produces_mana.is_empty()
        || d.changeling
        || d.transform_face.is_some()
        || d.animation.is_some()
    {
        return Err("Fishing Fish blueprint differs from printed no-ability blue 1/1".into());
    }
    Ok(id)
}
pub(crate) fn complete(
    state: &mut GameState,
    owner: OwnerV39,
    ctx: Option<&ExecCtx>,
) -> Result<(), String> {
    let removals = state
        .engine
        .event_history
        .iter()
        .enumerate()
        .filter_map(|(i, e)| match e {
            CommittedEvent::FishingPoleJournalV39 { journal } => match journal.as_ref() {
                JournalV39::OriginalRemoval {
                    origin,
                    item,
                    countered,
                    bait,
                    ..
                } if *origin == owner.origin && item.as_ref() == &owner.item => {
                    Some((i as u64, *countered, *bait))
                }
                _ => None,
            },
            _ => None,
        })
        .collect::<Vec<_>>();
    let [(removal, countered, bait)] = removals.as_slice() else {
        return Err("Fishing completion lacks genuine unique removal input".into());
    };
    if *countered != ctx.is_none() {
        return Err("Fishing completion changed actual resolution/counter".into());
    }
    if let Some(ctx) = ctx {
        if ctx.stack_item_id != Some(owner.item.v4.stack_item_id)
            || ctx.source != owner.item.source
            || ctx.controller != owner.item.controller
            || ctx.ability_source_contract != owner.item.v4.ability_source_contract
            || !ctx.targets.is_empty()
            || !ctx.target_contracts.is_empty()
        {
            return Err("Fishing removed context changed".into());
        }
    }
    let original_removal = *removal;
    let JournalV39::OriginalRemoval {
        origin,
        item,
        countered: original_countered,
        bait: original_bait,
        departure,
    } = at(state, original_removal)?.clone()
    else {
        unreachable!()
    };
    let removal = append(
        state,
        JournalV39::Removed {
            input: original_removal,
            origin,
            item,
            countered: original_countered,
            bait: original_bait,
            departure,
        },
    );
    let pole = origin_pole(state, owner.origin)?;
    let is_trigger = matches!(at(state, owner.origin)?, JournalV39::Captured { .. });
    let mut change = None;
    let mut token = None;
    if !countered {
        if let Some(before) = bait {
            let after = if is_trigger {
                if *before > 0 {
                    Some(*before - 1)
                } else {
                    None
                }
            } else {
                Some(before.checked_add(1).ok_or("Fishing bait overflow")?)
            };
            if let Some(after) = after {
                if !same_pole(state, pole)
                    || state.objects.get(pole.source).counters.bait != *before
                {
                    return Err("Fishing actual counter operation changed".into());
                }
                let fish = is_trigger.then(fish_definition).transpose()?;
                state.objects.get_mut(pole.source).counters.bait = after;
                change = Some(append(
                    state,
                    JournalV39::BaitChanged {
                        origin: owner.origin,
                        pole,
                        before: *before,
                        after,
                    },
                ));
                if let Some(fish) = fish {
                    let start = state.engine.event_history.len();
                    crate::event::batch_without_linked_aura_v1(
                        state,
                        vec![ProposedEvent::create_token(fish, owner.item.controller)],
                    );
                    token=state.engine.event_history[start..].iter().enumerate().find_map(|(i,e)|
                        matches!(e,CommittedEvent::CreateToken {token_def,controller,..} if *token_def==fish && *controller==owner.item.controller).then_some((start+i) as u64));
                    if token.is_none() {
                        return Err("Fishing actual Fish creation absent".into());
                    }
                }
            }
        }
    }
    append(
        state,
        JournalV39::Completed {
            origin: owner.origin,
            removal,
            change,
            token,
        },
    );
    validate_state(state)
}

fn rows(state: &GameState) -> impl Iterator<Item = (u64, &JournalV39)> {
    state
        .engine
        .event_history
        .iter()
        .enumerate()
        .filter_map(|(i, event)| match event {
            CommittedEvent::FishingPoleJournalV39 { journal } => Some((i as u64, journal.as_ref())),
            _ => None,
        })
}
pub(crate) fn validate_state(state: &GameState) -> Result<(), String> {
    for event in &state.engine.event_log {
        if let CommittedEvent::FishingPoleJournalV39 { journal } = event {
            if !matches!(journal.as_ref(), JournalV39::Captured { .. })
                || state
                    .engine
                    .event_history
                    .iter()
                    .filter(|original| *original == event)
                    .count()
                    != 1
            {
                return Err("Fishing transient log forged private history/capture".into());
            }
        }
    }
    for item in &state.stack {
        validate_item(state, item)?;
    }
    for (index, journal) in rows(state) {
        match journal {
            JournalV39::Untapped { host, poles } => {
                if poles.is_empty()
                    || poles
                        .iter()
                        .enumerate()
                        .any(|(i, p)| poles[..i].contains(p))
                {
                    return Err("Fishing original untap observer roster changed".into());
                }
                crate::engine::validate_historical_ability_source_contract(state, *host)?;
                for pole in poles {
                    let captures=rows(state).filter(|(_,row)| matches!(row,JournalV39::Captured {untap,pole: original} if *untap==index && original==pole)).count();
                    if captures != 1 {
                        return Err("Fishing original untap lost mandatory observer capture".into());
                    }
                }
            }
            JournalV39::Captured { .. } => {
                let pole = capture(state, index)?;
                let collected = rows(state)
                    .filter_map(|(i, row)| {
                        matches!(row,JournalV39::Collected {capture} if *capture==index)
                            .then_some(i)
                    })
                    .collect::<Vec<_>>();
                let logged=state.engine.event_log.iter().filter(|event| matches!(event,CommittedEvent::FishingPoleJournalV39 {journal: original} if original.as_ref()==journal)).count();
                let pending = state
                    .engine
                    .pending_triggers
                    .iter()
                    .chain(state.legend_pending_v1.iter().flatten())
                    .chain(
                        state
                            .pending_legend_rule_v1
                            .iter()
                            .flat_map(|p| p.waiting_triggers()),
                    )
                    .chain(
                        state
                            .standard_v1
                            .iter()
                            .filter_map(|s| s.resolution_play.as_ref())
                            .flat_map(|p| p.deferred_triggers.iter()),
                    )
                    .filter(|p| effect_origin(&p.effect) == Some(index))
                    .collect::<Vec<_>>();
                for p in &pending {
                    if p.source != pole.source
                        || p.controller != pole.controller
                        || p.source_contract != Some(pole)
                        || p.granted_by.is_some()
                        || p.target_spec != TargetSpec::None
                        || !p.targets.is_empty()
                        || !p.target_contracts.is_empty()
                        || p.is_madness_offer
                        || p.kicked
                        || p.optional_additional_cost_paid.is_some()
                        || !p.paid_cost_refs.is_empty()
                    {
                        return Err(
                            "Fishing pending trigger changed original beneficiary/root".into()
                        );
                    }
                }
                let pubs = publications(state, index);
                if collected.is_empty() {
                    if logged != 1 || !pending.is_empty() || !pubs.is_empty() {
                        return Err("Fishing uncollected occurrence lost its sole log owner".into());
                    }
                } else if collected.len() != 1
                    || collected[0] <= index
                    || logged != 0
                    || (pubs.is_empty() && pending.len() != 1)
                    || (!pubs.is_empty() && !pending.is_empty())
                {
                    return Err("Fishing collected occurrence lost/replayed mandatory owner".into());
                }
            }
            JournalV39::PaymentInput { .. } => {
                origin_pole(state, index)?;
                if publications(state, index).len() != 1 {
                    return Err("Fishing paid input lost its publication".into());
                }
            }
            JournalV39::Published { origin, item } => {
                let pubs = publications(state, *origin);
                if pubs.as_slice() != [(index, item.as_ref())] || *origin >= index {
                    return Err("Fishing publication duplicated or reordered".into());
                }
                shape(state, *origin, item)?;
                let live = state
                    .stack
                    .iter()
                    .filter(|row| row.v4.stack_item_id == item.v4.stack_item_id)
                    .collect::<Vec<_>>();
                let removed = rows(state)
                    .filter_map(|(at, row)| match row {
                        JournalV39::Removed {
                            input,
                            origin: original,
                            item: original_item,
                            countered,
                            bait,
                            departure,
                        } if original == origin => Some((
                            at,
                            *input,
                            original_item.as_ref(),
                            *countered,
                            *bait,
                            *departure,
                        )),
                        _ => None,
                    })
                    .collect::<Vec<_>>();
                let complete = rows(state)
                    .filter_map(|(at, row)| match row {
                        JournalV39::Completed {
                            origin: original,
                            removal,
                            change,
                            token,
                        } if original == origin => Some((at, *removal, *change, *token)),
                        _ => None,
                    })
                    .collect::<Vec<_>>();
                if removed.is_empty() && complete.is_empty() {
                    if rows(state).any(|(_,row)|matches!(row,JournalV39::OriginalRemoval {origin: saved,..} if saved==origin)) {return Err("Fishing actual removal lost mandatory operation/closure".into());}
                    if live.as_slice() != [item.as_ref()] {
                        return Err("Fishing published mandatory row erased/rewritten".into());
                    }
                    continue;
                }
                let [(removal, input, original, countered, bait, departed)] = removed.as_slice()
                else {
                    return Err("Fishing closure lost unique independent actual removal".into());
                };
                let [(closed, completion_removal, change, token)] = complete.as_slice() else {
                    return Err("Fishing removed row lost unique actual completion".into());
                };
                if !live.is_empty()
                    || *original != item.as_ref()
                    || *removal <= index
                    || *closed <= *removal
                    || *completion_removal != *removal
                {
                    return Err("Fishing original row reappeared or completion changed".into());
                }
                let inputs = rows(state)
                    .filter_map(|(at, row)| match row {
                        JournalV39::OriginalRemoval {
                            origin: saved,
                            item: saved_item,
                            countered: saved_countered,
                            bait: saved_bait,
                            departure: saved_departure,
                        } if saved == origin => Some((
                            at,
                            saved_item.as_ref(),
                            *saved_countered,
                            *saved_bait,
                            *saved_departure,
                        )),
                        _ => None,
                    })
                    .collect::<Vec<_>>();
                if inputs.as_slice() != [(*input, *original, *countered, *bait, *departed)]
                    || *input <= index
                    || *input >= *removal
                {
                    return Err("Fishing closure lost independent actual removal occurrence".into());
                }
                let pole = origin_pole(state, *origin)?;
                if bait.is_some() == departed.is_some() {
                    return Err("Fishing removal changed live-vs-departed sample".into());
                }
                if let Some(departure) = departed {
                    if *departure <= *origin
                        || *departure >= *removal
                        || !matches!(state.engine.event_history.get(*departure as usize),Some(CommittedEvent::ZoneChange {object,from:Zone::Battlefield,..}) if *object==pole.source)
                    {
                        return Err("Fishing no-op skipped original Pole departure proof".into());
                    }
                }
                if let Some(before) = bait {
                    let previous = rows(state)
                        .filter_map(|(at, row)| match row {
                            JournalV39::BaitChanged {
                                pole: original,
                                after,
                                ..
                            } if at < *removal
                                && original.source == pole.source
                                && original.zone_change_count == pole.zone_change_count =>
                            {
                                Some(*after)
                            }
                            _ => None,
                        })
                        .last()
                        .unwrap_or(0);
                    if *before != previous {
                        return Err("Fishing removal rewrote actual counter balance".into());
                    }
                }
                let changes=rows(state).filter_map(|(at,row)| matches!(row,JournalV39::BaitChanged {origin: original,..} if original==origin).then_some(at)).collect::<Vec<_>>();
                if changes.as_slice() != change.as_slice() {
                    return Err("Fishing actual bait change orphaned or duplicated".into());
                }
                if bait.is_some_and(|n| n < 0) {
                    return Err("Fishing original counter sample negative".into());
                }
                let trigger = matches!(at(state, *origin)?, JournalV39::Captured { .. });
                if !*countered && !trigger && bait == &Some(i32::MAX) {
                    return Err("Fishing completed an overflowing bait operation".into());
                }
                let expected = if *countered {
                    None
                } else {
                    bait.and_then(|before| {
                        if trigger {
                            (before > 0).then_some(before - 1)
                        } else {
                            before.checked_add(1)
                        }
                    })
                };
                match (expected, change) {
                    (None, None) => {}
                    (Some(after), Some(change))
                        if *change > *removal
                            && *change < *closed
                            && matches!(at(state,*change)?,JournalV39::BaitChanged {origin: original,pole: original_pole,before,after: actual}
                            if *original==*origin && *original_pole==pole && Some(*before)==*bait && *actual==after) =>
                        {}
                    _ => return Err("Fishing closure skipped/forged real bait operation".into()),
                }
                if trigger && expected.is_some() {
                    let Some(token) = token else {
                        return Err("Fishing paid mandatory trigger skipped Fish".into());
                    };
                    if *token <= change.unwrap()
                        || *token >= *closed
                        || !matches!(state.engine.event_history.get(*token as usize),Some(CommittedEvent::CreateToken {token_def,controller,..}) if *token_def==fish_definition()? && *controller==item.controller)
                    {
                        return Err(
                            "Fishing closure forged Fish identity/controller/operation".into()
                        );
                    }
                } else if token.is_some() {
                    return Err("Fishing no-op/countered/activation manufactured Fish".into());
                }
            }
            JournalV39::Collected { capture: index } => {
                capture(state, *index)?;
            }
            JournalV39::OriginalRemoval { origin, .. }
            | JournalV39::Removed { origin, .. }
            | JournalV39::BaitChanged { origin, .. }
            | JournalV39::Completed { origin, .. } => {
                if publications(state, *origin).len() != 1 {
                    return Err("Fishing operation journal orphaned".into());
                }
            }
        }
    }
    for (object, live) in state.objects.iter() {
        if live.counters.bait < 0 {
            return Err("Fishing bait count negative".into());
        }
        if live.zone != Zone::Battlefield || (live.counters.bait==0 && !selected_definition(live.card_def) && !rows(state).any(|(_,row)|matches!(row,JournalV39::BaitChanged {pole,..} if pole.source==object && pole.zone_change_count==live.zone_change_count))) {
            continue;
        }
        let mut balance = 0;
        for (_, row) in rows(state) {
            if let JournalV39::BaitChanged {
                pole,
                before,
                after,
                ..
            } = row
            {
                if pole.source == object && pole.zone_change_count == live.zone_change_count {
                    if *before != balance {
                        return Err("Fishing counter operation rewrote original balance".into());
                    }
                    balance = *after;
                }
            }
        }
        if live.counters.bait != balance {
            return Err("Fishing current bait differs from actual operations".into());
        }
    }
    Ok(())
}
pub(crate) fn requires_successor(state: &GameState) -> bool {
    state.objects.iter().any(|(_, live)| {
        live.counters.bait != 0
            || (live.zone == Zone::Battlefield && selected_definition(live.card_def))
    }) || rows(state).next().is_some()
}
