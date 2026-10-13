//! Private paid loyalty and atomic removed-row authority. Original committed
//! inputs are trusted provenance; this is not whole-journal tamper resistance.
use crate::card_def::{Keywords, TargetSpec};
use crate::effect::{EffectObjectBinding, ExecCtx};
use crate::engine::{PendingActivation, UntilEndOfTurnEffect};
use crate::event::{CommittedEvent, ProposedEvent};
use crate::ids::{PlayerId, StackItemId};
use crate::state::{
    AbilityKindV4, AbilitySourceContractV4, GameState, StackItem, StackItemKind,
    StackTargetContractV4, Target, Zone,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum JournalV42 {
    Turn {
        index: u64,
        turn: u32,
        active: PlayerId,
    },
    PaymentInput {
        index: u64,
        epoch: u64,
        pending: Box<PendingActivation>,
        source: AbilitySourceContractV4,
        before: u32,
        after: u32,
    },
    Paid {
        index: u64,
        input: u64,
        item: Box<StackItem>,
    },
    RemovalInput {
        index: u64,
        paid: u64,
        item: Box<StackItem>,
        countered: bool,
        life: Option<i32>,
        target: Option<EffectObjectBinding>,
        before: Option<i32>,
    },
    Operation {
        index: u64,
        input: u64,
        events: Vec<u64>,
        boost: Option<UntilEndOfTurnEffect>,
    },
    Closed {
        index: u64,
        operation: u64,
    },
    Cleanup {
        index: u64,
        boosts: Vec<UntilEndOfTurnEffect>,
    },
}
fn rows(state: &GameState) -> impl DoubleEndedIterator<Item = (u64, &JournalV42)> {
    state
        .engine
        .event_history
        .iter()
        .enumerate()
        .filter_map(|(i, e)| match e {
            CommittedEvent::AjaniJournalV42 { journal } => Some((i as u64, journal.as_ref())),
            _ => None,
        })
}
fn at(state: &GameState, i: u64) -> Result<&JournalV42, String> {
    match state
        .engine
        .event_history
        .get(usize::try_from(i).map_err(|_| "Ajani index overflow")?)
    {
        Some(CommittedEvent::AjaniJournalV42 { journal }) => Ok(journal),
        _ => Err("Ajani original occurrence absent".into()),
    }
}
fn index(j: &JournalV42) -> u64 {
    match j {
        JournalV42::Turn { index, .. }
        | JournalV42::PaymentInput { index, .. }
        | JournalV42::Paid { index, .. }
        | JournalV42::RemovalInput { index, .. }
        | JournalV42::Operation { index, .. }
        | JournalV42::Closed { index, .. }
        | JournalV42::Cleanup { index, .. } => *index,
    }
}
fn append(state: &mut GameState, j: JournalV42) {
    state
        .engine
        .event_history
        .push(CommittedEvent::AjaniJournalV42 {
            journal: Box::new(j),
        });
}
fn relevant(state: &GameState) -> bool {
    state
        .objects
        .iter()
        .any(|(_, o)| super::selected_definition(o.card_def))
        || rows(state).next().is_some()
}
pub(crate) fn initialize_entry(state: &mut GameState, object: crate::ids::ObjectId) {
    if super::selected_definition(state.objects.get(object).card_def)
        && !rows(state).any(|(_, j)| matches!(j, JournalV42::Turn { .. }))
    {
        begin_turn(state);
    }
}
pub(crate) fn begin_turn(state: &mut GameState) {
    if !relevant(state) {
        return;
    }
    append(
        state,
        JournalV42::Turn {
            index: state.engine.event_history.len() as u64,
            turn: state.turn_key_v1(),
            active: state.active_player,
        },
    );
}
fn epoch(state: &GameState) -> Result<u64, String> {
    let (i, j) = rows(state)
        .rev()
        .find(|(_, j)| matches!(j, JournalV42::Turn { .. }))
        .ok_or("Ajani genuine turn anchor absent")?;
    if !matches!(j,JournalV42::Turn{turn,active,..} if *turn==state.turn_key_v1() && *active==state.active_player)
    {
        return Err("Ajani turn anchor stale".into());
    }
    Ok(i)
}
pub(crate) fn used(state: &GameState, source: crate::ids::ObjectId) -> bool {
    let Ok(epoch) = epoch(state) else {
        return false;
    };
    let zcc = state.objects.get(source).zone_change_count;
    rows(state).any(|(_,j)| matches!(j, JournalV42::PaymentInput{epoch:e,source:s,..} if *e==epoch && s.source==source && s.zone_change_count==zcc))
}
fn selected(item: &StackItem) -> bool {
    item.v4
        .ability_source_contract
        .is_some_and(|s| super::selected_definition(s.card_def))
}
pub(crate) fn tracked(state: &GameState, item: &StackItem) -> bool {
    selected(item) || matches!(item.inline_effect,Some(crate::effect::EffectOp::AjaniLoyaltyV42{..})) || rows(state).any(|(_,j)|matches!(j,JournalV42::Paid{item:p,..} if p.v4.stack_item_id==item.v4.stack_item_id))
}
fn shape(state: &GameState, item: &StackItem) -> Result<(), String> {
    let source = item
        .v4
        .ability_source_contract
        .ok_or("Ajani original source absent")?;
    super::validate_definition(source.card_def)?;
    let i = item
        .v4
        .activated_ability_index
        .ok_or("Ajani ability index absent")?;
    let ability = crate::card_def::CARD_DEFS[usize::from(source.card_def)]
        .activated_abilities
        .get(usize::from(i))
        .ok_or("Ajani ability out of range")?;
    let expected = crate::state::StackStateV4 {
        stack_item_id: item.v4.stack_item_id,
        ability_source_contract: Some(source),
        activated_ability_index: Some(i),
        target_spec: Some(ability.target_spec),
        target_contracts: item.v4.target_contracts.clone(),
        ..Default::default()
    };
    if item.kind != StackItemKind::ActivatedAbility
        || item.source != source.source
        || item.controller != source.controller
        || source.zone != Zone::Battlefield
        || item.inline_effect != Some((ability.effect)())
        || item.v4 != expected
        || item.v4.stack_item_id == StackItemId::default()
        || item.is_copy
        || item.is_flashback
        || item.kicked
        || item.madness_offer
        || item.mode_chosen != 0
        || !item.discarded.is_empty()
        || item.targets.len() != item.v4.target_contracts.len()
        || item.targets.len() > usize::from(i != 2)
        || (i == 1 && item.targets.len() != 1)
    {
        return Err("Ajani finalized root metadata changed".into());
    }
    for (t, c) in item.targets.iter().zip(&item.v4.target_contracts) {
        if !matches!((t,c), (Target::Object(id),StackTargetContractV4::Object{object,zone:Zone::Battlefield,..}) if id==object)
        {
            return Err("Ajani target contract changed".into());
        }
    }
    crate::engine::validate_historical_ability_source_contract(state, source)
}
pub(crate) struct PaymentOwner {
    pending: PendingActivation,
    source: AbilitySourceContractV4,
    epoch: u64,
    before: u32,
    after: u32,
}
pub(crate) fn acquire_payment(state: &GameState) -> Result<Option<PaymentOwner>, String> {
    let Some(p) = state.engine.pending_activation.as_ref() else {
        return Ok(None);
    };
    let source = AbilitySourceContractV4::capture(state, p.source);
    if !super::selected_definition(source.card_def) {
        return Ok(None);
    }
    validate_state(state)?;
    super::validate_definition(source.card_def)?;
    crate::engine::validate_pending_activation(state, p)?;
    if used(state, p.source) {
        return Err("Ajani loyalty already used this turn".into());
    }
    let before =
        crate::planeswalker_v1::loyalty(state, p.source).ok_or("Ajani loyalty ledger absent")?;
    let after = match p.ability_index {
        0 => before
            .checked_add(
                crate::standard_cards_v1::scale_counters(state, p.controller, 1).max(0) as u32,
            )
            .ok_or("Ajani loyalty overflow")?,
        1 => before.checked_sub(3).ok_or("Ajani -3 unpaid")?,
        2 => before.checked_sub(8).ok_or("Ajani -8 unpaid")?,
        _ => return Err("Ajani unknown loyalty root".into()),
    };
    if p.ability_index == 2 {
        super::token_definition()?;
    }
    Ok(Some(PaymentOwner {
        pending: p.clone(),
        source,
        epoch: epoch(state)?,
        before,
        after,
    }))
}
pub(crate) fn record_paid(
    state: &mut GameState,
    id: StackItemId,
    owner: Option<PaymentOwner>,
) -> Result<(), String> {
    let Some(o) = owner else {
        return Ok(());
    };
    let item = state
        .stack
        .iter()
        .find(|i| i.v4.stack_item_id == id)
        .ok_or("Ajani paid publication absent")?
        .clone();
    shape(state, &item)?;
    if item.v4.ability_source_contract != Some(o.source)
        || item.v4.activated_ability_index != Some(o.pending.ability_index)
        || item.targets != o.pending.targets_chosen
        || item.v4.target_contracts != o.pending.target_contracts
        || crate::planeswalker_v1::loyalty(state, item.source) != Some(o.after)
        || !state
            .objects
            .get(item.source)
            .v4
            .ability_uses_this_turn
            .iter()
            .any(|e| {
                e.ability_kind == AbilityKindV4::Activated
                    && e.ability_index == u16::from(o.pending.ability_index)
                    && e.uses == 1
            })
    {
        return Err("Ajani actual paid loyalty/publication changed".into());
    }
    let input = state.engine.event_history.len() as u64;
    append(
        state,
        JournalV42::PaymentInput {
            index: input,
            epoch: o.epoch,
            pending: Box::new(o.pending),
            source: o.source,
            before: o.before,
            after: o.after,
        },
    );
    append(
        state,
        JournalV42::Paid {
            index: input + 1,
            input,
            item: Box::new(item),
        },
    );
    Ok(())
}
fn paid(state: &GameState, id: StackItemId) -> Result<(u64, &StackItem), String> {
    let matches = rows(state)
        .filter_map(|(i, j)| match j {
            JournalV42::Paid { item, .. } if item.v4.stack_item_id == id => {
                Some((i, item.as_ref()))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    let [(i, item)] = matches.as_slice() else {
        return Err("Ajani unique original paid row absent".into());
    };
    Ok((*i, *item))
}
pub(crate) fn validate_item(state: &GameState, item: &StackItem) -> Result<(), String> {
    if !tracked(state, item) {
        return Ok(());
    }
    shape(state, item)?;
    let (p, original) = paid(state, item.v4.stack_item_id)?;
    if original != item
        || rows(state).any(|(_, j)| matches!(j,JournalV42::RemovalInput{paid,..} if *paid==p))
    {
        return Err("Ajani live row changed/replayed".into());
    }
    Ok(())
}
pub(crate) struct Owner {
    item: StackItem,
    paid: u64,
}
pub(crate) fn acquire(state: &GameState, item: &StackItem) -> Result<Option<Owner>, String> {
    if !tracked(state, item) {
        return Ok(None);
    }
    validate_item(state, item)?;
    if state
        .stack
        .iter()
        .filter(|i| i.v4.stack_item_id == item.v4.stack_item_id)
        .collect::<Vec<_>>()
        != [item]
    {
        return Err("Ajani unique live loan absent".into());
    }
    Ok(Some(Owner {
        item: item.clone(),
        paid: paid(state, item.v4.stack_item_id)?.0,
    }))
}
pub(crate) fn record_removed(
    state: &mut GameState,
    owner: Option<&Owner>,
    countered: bool,
) -> Result<(), String> {
    let Some(o) = owner else {
        return Ok(());
    };
    if state
        .stack
        .iter()
        .any(|i| i.v4.stack_item_id == o.item.v4.stack_item_id)
        || paid(state, o.item.v4.stack_item_id)?.1 != &o.item
    {
        return Err("Ajani actual removed owner absent".into());
    }
    let i = o.item.v4.activated_ability_index.unwrap();
    let source = o.item.v4.ability_source_contract.unwrap();
    let target = if !countered
        && i != 2
        && o.item.targets.first().is_some_and(|_| {
            crate::engine::effect_target_is_legal_from_ability_source(
                state,
                source,
                o.item.controller,
                TargetSpec::Creature,
                &o.item.targets,
                0,
            )
        }) {
        match o.item.v4.target_contracts[0] {
            StackTargetContractV4::Object {
                object,
                zone_change_count,
                zone: Zone::Battlefield,
                ..
            } => state
                .objects
                .try_get(object)
                .filter(|x| x.zone == Zone::Battlefield && x.zone_change_count == zone_change_count)
                .map(|_| EffectObjectBinding {
                    object,
                    expected_zone: Zone::Battlefield,
                    expected_zone_change_count: zone_change_count,
                }),
            _ => None,
        }
    } else {
        None
    };
    let before = target.map(|t| state.objects.get(t.object).counters.plus1_plus1);
    append(
        state,
        JournalV42::RemovalInput {
            index: state.engine.event_history.len() as u64,
            paid: o.paid,
            item: Box::new(o.item.clone()),
            countered,
            life: (!countered && i == 2).then(|| {
                crate::engine::evaluate_dynamic_value(state, super::LIFE_COUNT, o.item.controller)
            }),
            target,
            before,
        },
    );
    Ok(())
}
pub(crate) fn complete(
    state: &mut GameState,
    owner: Owner,
    ctx: Option<&ExecCtx>,
) -> Result<(), String> {
    let inputs = rows(state)
        .filter_map(|(i, j)| match j {
            JournalV42::RemovalInput { paid, .. } if *paid == owner.paid => Some((i, j.clone())),
            _ => None,
        })
        .collect::<Vec<_>>();
    let [(input, j)] = inputs.as_slice() else {
        return Err("Ajani actual removal input absent".into());
    };
    let JournalV42::RemovalInput {
        item,
        countered,
        life,
        target,
        ..
    } = j
    else {
        unreachable!()
    };
    if item.as_ref() != &owner.item || *countered != ctx.is_none() {
        return Err("Ajani removed loan/context changed".into());
    }
    if let Some(ctx) = ctx {
        if ctx.stack_item_id != Some(item.v4.stack_item_id)
            || ctx.source != item.source
            || ctx.controller != item.controller
            || ctx.ability_source_contract != item.v4.ability_source_contract
            || ctx.targets != item.targets
            || ctx.target_contracts != item.v4.target_contracts
        {
            return Err("Ajani resolution context changed".into());
        }
    }
    let start = state.engine.event_history.len();
    let mut boost = None;
    if !countered {
        match item.v4.activated_ability_index.unwrap() {
            0 => {
                if let Some(t) = target {
                    crate::event::add_plus_one_counters(state, t.object, item.controller, 1)?;
                }
            }
            1 => {
                if let Some(t) = target {
                    let at = state.engine.until_end_of_turn.len();
                    crate::effect::install_temporary_boost(
                        state,
                        *t,
                        0,
                        0,
                        Keywords::FLYING | Keywords::DOUBLE_STRIKE,
                    );
                    boost = state.engine.until_end_of_turn.get(at).cloned();
                    if boost.is_none() {
                        return Err("Ajani temporary keyword operation absent".into());
                    }
                }
            }
            2 => {
                let token = super::token_definition()?;
                let count =
                    usize::try_from(life.ok_or("Ajani original life sample absent")?.max(0))
                        .map_err(|_| "Ajani token count not representable")?;
                crate::event::batch_without_linked_aura_v1(
                    state,
                    (0..count)
                        .map(|_| ProposedEvent::create_token(token, item.controller))
                        .collect(),
                );
            }
            _ => return Err("Ajani unknown removed root".into()),
        }
    }
    if state.engine.halted.is_some() {
        return Err("Ajani atomic operation halted".into());
    }
    let events = (start..state.engine.event_history.len())
        .filter(|i| {
            matches!(
                state.engine.event_history[*i],
                CommittedEvent::PlusOneCountersAdded { .. } | CommittedEvent::CreateToken { .. }
            )
        })
        .map(|i| i as u64)
        .collect();
    let operation = state.engine.event_history.len() as u64;
    append(
        state,
        JournalV42::Operation {
            index: operation,
            input: *input,
            events,
            boost,
        },
    );
    append(
        state,
        JournalV42::Closed {
            index: operation + 1,
            operation,
        },
    );
    validate_state(state)
}
pub(crate) fn cleanup(state: &mut GameState) -> Result<(), String> {
    validate_state(state)?;
    let boosts = rows(state)
        .filter_map(|(_, j)| match j {
            JournalV42::Operation { boost: Some(b), .. }
                if state.engine.until_end_of_turn.contains(b) =>
            {
                Some(b.clone())
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    if !boosts.is_empty() {
        append(
            state,
            JournalV42::Cleanup {
                index: state.engine.event_history.len() as u64,
                boosts,
            },
        );
    }
    Ok(())
}
pub(crate) fn validate_state(state: &GameState) -> Result<(), String> {
    if state
        .engine
        .event_log
        .iter()
        .any(|e| matches!(e, CommittedEvent::AjaniJournalV42 { .. }))
    {
        return Err("private Ajani journal in transient log".into());
    }
    for item in &state.stack {
        validate_item(state, item)?;
    }
    for (i, j) in rows(state) {
        if index(j) != i {
            return Err("Ajani occurrence index changed".into());
        }
        match j {
            JournalV42::Turn { .. } => {}
            JournalV42::PaymentInput {
                epoch,
                pending,
                source,
                before,
                after,
                ..
            } => {
                if *epoch >= i
                    || !matches!(at(state, *epoch)?, JournalV42::Turn { .. })
                    || pending.source != source.source
                    || pending.source_zone_change_count != source.zone_change_count
                    || pending.controller != source.controller
                {
                    return Err("Ajani original payment source/turn changed".into());
                }
                super::validate_definition(source.card_def)?;
                let expected = match pending.ability_index {
                    0 => after.checked_sub(*before).is_some(),
                    1 => before.checked_sub(3) == Some(*after),
                    2 => before.checked_sub(8) == Some(*after),
                    _ => false,
                };
                if !expected || rows(state).filter(|(_,j)|matches!(j,JournalV42::PaymentInput{epoch:e,source:s,..} if e==epoch && s.source==source.source && s.zone_change_count==source.zone_change_count)).count()!=1 || rows(state).filter(|(_,j)|matches!(j,JournalV42::Paid{input,..} if *input==i)).count()!=1 {return Err("Ajani payment duplicate/erased".into());}
            }
            JournalV42::Paid { input, item, .. } => {
                shape(state, item)?;
                let JournalV42::PaymentInput {
                    pending, source, ..
                } = at(state, *input)?
                else {
                    return Err("Ajani paid input lost".into());
                };
                if input.checked_add(1) != Some(i)
                    || item.v4.ability_source_contract != Some(*source)
                    || item.v4.activated_ability_index != Some(pending.ability_index)
                    || item.targets != pending.targets_chosen
                    || item.v4.target_contracts != pending.target_contracts
                {
                    return Err("Ajani paid input rewritten".into());
                }
                let live = state
                    .stack
                    .iter()
                    .filter(|x| x.v4.stack_item_id == item.v4.stack_item_id)
                    .collect::<Vec<_>>();
                let removed = rows(state)
                    .filter(|(_, j)| matches!(j,JournalV42::RemovalInput{paid,..} if *paid==i))
                    .collect::<Vec<_>>();
                if !((removed.is_empty() && live == [item.as_ref()])
                    || (removed.len() == 1 && live.is_empty()))
                {
                    return Err("Ajani mandatory paid owner erased/replayed".into());
                }
            }
            JournalV42::RemovalInput {
                paid,
                item,
                countered,
                life,
                target,
                before,
                ..
            } => {
                let JournalV42::Paid { item: original, .. } = at(state, *paid)? else {
                    return Err("Ajani removal original paid row lost".into());
                };
                if *paid >= i
                    || item != original
                    || (*countered && (life.is_some() || target.is_some() || before.is_some()))
                    || life.is_some() != (!countered && item.v4.activated_ability_index == Some(2))
                    || target.is_some() != before.is_some()
                {
                    return Err("Ajani original removal changed".into());
                }
                if let Some(t) = target {
                    if t.expected_zone != Zone::Battlefield
                        || !matches!(item.v4.target_contracts.first(),Some(StackTargetContractV4::Object{object,zone:Zone::Battlefield,zone_change_count,..}) if *object==t.object && *zone_change_count==t.expected_zone_change_count)
                    {
                        return Err(
                            "Ajani removed target differs from original paid incarnation".into(),
                        );
                    }
                }
                if rows(state)
                    .filter(|(_, j)| matches!(j,JournalV42::Operation{input,..} if *input==i))
                    .count()
                    != 1
                {
                    return Err("Ajani removed operation erased/replayed".into());
                }
            }
            JournalV42::Operation {
                input,
                events,
                boost,
                ..
            } => {
                let JournalV42::RemovalInput {
                    item,
                    countered,
                    life,
                    target,
                    ..
                } = at(state, *input)?
                else {
                    return Err("Ajani operation original input absent".into());
                };
                if *input >= i
                    || rows(state)
                        .filter(
                            |(_, j)| matches!(j,JournalV42::Closed{operation,..} if *operation==i),
                        )
                        .count()
                        != 1
                {
                    return Err("Ajani operation not uniquely closed".into());
                }
                let ability = item.v4.activated_ability_index.unwrap();
                if *countered || target.is_none() && ability != 2 {
                    if !events.is_empty() || boost.is_some() {
                        return Err("Ajani no-op forged effect".into());
                    }
                } else if ability == 0 {
                    if boost.is_some()
                        || events.len() != 1
                        || !matches!(state.engine.event_history.get(events[0] as usize),Some(CommittedEvent::PlusOneCountersAdded{object,zone_change_count,player,count}) if Some(*object)==target.map(|t|t.object) && Some(*zone_change_count)==target.map(|t|t.expected_zone_change_count) && *player==item.controller && *count==1)
                    {
                        return Err("Ajani actual counter occurrence lost".into());
                    }
                } else if ability == 1 {
                    let Some(b) = boost else {
                        return Err("Ajani keywords operation erased".into());
                    };
                    if !events.is_empty()
                        || !matches!(b,UntilEndOfTurnEffect::ResolvedObjectKeywordEffect{object_id,object_zone_change_count,layer,duration,keywords,..} if *layer==crate::engine::Layers::ABILITY_ADDING && *duration==crate::engine::EffectDuration::EndOfTurn && Some(*object_id)==target.map(|t|t.object) && Some(*object_zone_change_count)==target.map(|t|t.expected_zone_change_count) && *keywords==(Keywords::FLYING|Keywords::DOUBLE_STRIKE))
                    {
                        return Err("Ajani exact keyword recipient changed".into());
                    }
                    let expired=rows(state).filter(|(_,j)|matches!(j,JournalV42::Cleanup{boosts,..} if boosts.contains(b))).count();
                    if expired > 1
                        || (expired == 0
                            && state
                                .engine
                                .until_end_of_turn
                                .iter()
                                .filter(|x| *x == b)
                                .count()
                                != 1)
                        || (expired == 1 && state.engine.until_end_of_turn.contains(b))
                    {
                        return Err("Ajani temporary grant erased/resurrected".into());
                    }
                } else {
                    if boost.is_some()
                        || events.len()
                            != usize::try_from(life.unwrap().max(0))
                                .map_err(|_| "Ajani count overflow")?
                    {
                        return Err("Ajani life/token count changed".into());
                    }
                    let token = super::token_definition()?;
                    for at in events {
                        if !matches!(state.engine.event_history.get(*at as usize),Some(CommittedEvent::CreateToken{token_def,controller,..}) if *token_def==token && *controller==item.controller)
                        {
                            return Err("Ajani actual token occurrence lost".into());
                        }
                    }
                }
                if events
                    .iter()
                    .enumerate()
                    .any(|(n, at)| *at <= *input || *at >= i || n > 0 && events[n - 1] >= *at)
                {
                    return Err("Ajani effect occurrence order changed".into());
                }
            }
            JournalV42::Closed { operation, .. } => {
                if operation.checked_add(1) != Some(i)
                    || !matches!(at(state, *operation)?, JournalV42::Operation { .. })
                {
                    return Err("Ajani completion forged".into());
                }
            }
            JournalV42::Cleanup { boosts, .. } => {
                if boosts.is_empty()
                    || boosts.iter().enumerate().any(|(n, b)| {
                        boosts[..n].contains(b)
                            || !rows(state).any(|(at, j)| {
                                at < i
                                    && matches!(j,JournalV42::Operation{boost:Some(x),..} if x==b)
                            })
                    })
                {
                    return Err("Ajani cleanup roster changed".into());
                }
            }
        }
    }
    Ok(())
}
pub(crate) fn requires_successor(state: &GameState) -> bool {
    state
        .objects
        .iter()
        .any(|(_, o)| o.zone == Zone::Battlefield && super::selected_definition(o.card_def))
        || rows(state).any(|(_, j)| !matches!(j, JournalV42::Turn { .. }))
}
pub(crate) fn historical_source(
    state: &GameState,
    item: &StackItem,
) -> Result<Option<crate::rl::CardStableRefV1>, String> {
    if !tracked(state, item) {
        return Ok(None);
    }
    validate_item(state, item)?;
    if state
        .stack
        .iter()
        .filter(|row| row.v4.stack_item_id == item.v4.stack_item_id)
        .collect::<Vec<_>>()
        != [item]
    {
        return Err("Ajani historical source lacks unique complete live row".into());
    }
    let source = item
        .v4
        .ability_source_contract
        .ok_or("Ajani historical source absent")?;
    Ok(Some(crate::rl::CardStableRefV1 {
        arena_id: source.source.0,
        card_db_id: source.card_def,
        owner: source.owner.into(),
        controller: source.controller.into(),
        zone: source.zone,
        zone_change_count: source.zone_change_count,
    }))
}
