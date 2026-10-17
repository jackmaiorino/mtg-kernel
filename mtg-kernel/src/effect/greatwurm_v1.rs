//! Quilled Greatwurm native source, exact counter costs and final combat waves.
use super::*;
use serde::{Deserialize, Serialize};

/// Native storage kinds, in fixed menu order. No signed or unsigned width is lost.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CounterKindV1 {
    PlusOne,
    MinusOne,
    MinusToughness,
    Stun,
    Lore,
    Oil,
    Charge,
    Net,
    Bait,
    Soul,
    Incubation,
    Revival,
    Fellowship,
    Loyalty,
    Lifelink,
    Time,
    Finality,
    Stash,
}
pub const KINDS: [CounterKindV1; 18] = [
    CounterKindV1::PlusOne,
    CounterKindV1::MinusOne,
    CounterKindV1::MinusToughness,
    CounterKindV1::Stun,
    CounterKindV1::Lore,
    CounterKindV1::Oil,
    CounterKindV1::Charge,
    CounterKindV1::Net,
    CounterKindV1::Bait,
    CounterKindV1::Soul,
    CounterKindV1::Incubation,
    CounterKindV1::Revival,
    CounterKindV1::Fellowship,
    CounterKindV1::Loyalty,
    CounterKindV1::Lifelink,
    CounterKindV1::Time,
    CounterKindV1::Finality,
    CounterKindV1::Stash,
];
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UnitV1 {
    pub permanent: EffectObjectBinding,
    pub kind: CounterKindV1,
}

pub(crate) fn count(state: &GameState, id: ObjectId, kind: CounterKindV1) -> Result<u32, String> {
    let o = state.objects.try_get(id).ok_or("counter object missing")?;
    let c = o.counters;
    let signed = match kind {
        CounterKindV1::PlusOne => c.plus1_plus1,
        CounterKindV1::MinusOne => i32::from(c.minus1_minus1),
        CounterKindV1::MinusToughness => i32::from(c.minus0_minus1),
        CounterKindV1::Stun => i32::from(c.stun),
        CounterKindV1::Lore => i32::from(c.lore),
        CounterKindV1::Oil => i32::from(c.oil),
        CounterKindV1::Charge => c.charge,
        CounterKindV1::Net => c.net,
        CounterKindV1::Bait => c.bait,
        CounterKindV1::Soul => c.soul,
        CounterKindV1::Incubation => c.incubation,
        CounterKindV1::Revival => c.revival,
        CounterKindV1::Fellowship => c.fellowship,
        CounterKindV1::Stash => c.stash,
        CounterKindV1::Loyalty => {
            return Ok(crate::planeswalker_v1::loyalty(state, id).unwrap_or(0))
        }
        CounterKindV1::Lifelink => i32::from(o.v4.lifelink_keyword_counters),
        CounterKindV1::Time => return Ok(u32::from(o.v4.time_counters_v1)),
        CounterKindV1::Finality => {
            return Ok(u32::from(
                o.v4.creature_upgrade.as_ref().map_or(0, |u| u.finality),
            ))
        }
    };
    u32::try_from(signed).map_err(|_| "negative native counter count".into())
}
/// Called only by the consumed complete payment plan. No event/SBA/collection
/// occurs between units, even if an intermediate creature has zero toughness.
pub(crate) fn remove_unit(state: &mut GameState, unit: UnitV1) {
    use CounterKindV1::*;
    if unit.kind == Loyalty {
        crate::planeswalker_v1::pay_loyalty_cost(state, unit.permanent.object, -1);
        return;
    }
    let o = state.objects.get_mut(unit.permanent.object);
    match unit.kind {
        PlusOne => o.counters.plus1_plus1 -= 1,
        MinusOne => o.counters.minus1_minus1 -= 1,
        MinusToughness => o.counters.minus0_minus1 -= 1,
        Stun => o.counters.stun -= 1,
        Lore => o.counters.lore -= 1,
        Oil => o.counters.oil -= 1,
        Charge => o.counters.charge -= 1,
        Net => o.counters.net -= 1,
        Bait => o.counters.bait -= 1,
        Soul => o.counters.soul -= 1,
        Incubation => o.counters.incubation -= 1,
        Revival => o.counters.revival -= 1,
        Fellowship => o.counters.fellowship -= 1,
        Stash => o.counters.stash -= 1,
        Lifelink => o.v4.lifelink_keyword_counters -= 1,
        Time => o.v4.time_counters_v1 -= 1,
        Finality => {
            o.v4.creature_upgrade
                .as_mut()
                .expect("positive finality has storage")
                .finality -= 1
        }
        Loyalty => unreachable!(),
    }
}
pub(crate) fn units(
    state: &GameState,
    player: PlayerId,
    prefix: &[UnitV1],
) -> Result<Vec<UnitV1>, String> {
    let mut result = Vec::new();
    for &id in &state.players[player.index()].battlefield {
        let o = state
            .objects
            .try_get(id)
            .ok_or("counter roster object missing")?;
        if o.zone != Zone::Battlefield || o.controller != player {
            return Err("counter roster changed".into());
        }
        if !crate::engine::object_has_type(state, id, CardType::Creature) {
            continue;
        }
        let permanent = EffectObjectBinding {
            object: id,
            expected_zone: o.zone,
            expected_zone_change_count: o.zone_change_count,
        };
        for kind in KINDS {
            let unit = UnitV1 { permanent, kind };
            let used = u32::try_from(prefix.iter().filter(|old| **old == unit).count())
                .map_err(|_| "counter prefix width")?;
            if count(state, id, kind)?
                .checked_sub(used)
                .ok_or("counter prefix overspent")?
                > 0
            {
                result.push(unit);
            }
        }
    }
    Ok(result)
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CaptureV1 {
    pub creature: crate::state::AbilitySourceContractV4,
    pub observers: Vec<crate::state::AbilitySourceContractV4>,
    pub damage: Vec<u64>,
    pub damage_window_start: u64,
    pub damage_window_end: u64,
    pub amount: i32,
}
// A private pre-materialization wave roster. Merely setting is_combat on a
// proposal cannot bind it: only the engine's actual wave-finish callback can.
pub(crate) struct WaveV1 {
    sources: Vec<(
        crate::state::AbilitySourceContractV4,
        Vec<crate::state::AbilitySourceContractV4>,
        Vec<(Target, i32)>,
    )>,
}
pub(crate) fn capture_wave(
    state: &GameState,
    events: &[event::ProposedEvent],
) -> Result<WaveV1, String> {
    if !events
        .iter()
        .any(|e| matches!(e,event::ProposedEvent::Damage(d)if d.is_combat && d.amount>0))
    {
        return Ok(WaveV1 { sources: vec![] });
    }
    let observers = state
        .objects
        .iter()
        .filter_map(|(id, o)| {
            (o.zone == Zone::Battlefield
                && o.controller == state.active_player
                && o.v4.face_down_v1.is_none()
                && o.v4.face_index == 0
                && definition(o.card_def)
                && crate::continuous_characteristics_v1::printed_abilities_active(state, id))
            .then(|| crate::state::AbilitySourceContractV4::capture(state, id))
        })
        .collect::<Vec<_>>();
    let mut sources: Vec<(
        crate::state::AbilitySourceContractV4,
        Vec<crate::state::AbilitySourceContractV4>,
        Vec<(Target, i32)>,
    )> = vec![];
    if observers.is_empty() {
        return Ok(WaveV1 { sources });
    }
    for e in events {
        let event::ProposedEvent::Damage(d) = e else {
            continue;
        };
        if !d.is_combat || d.amount <= 0 {
            continue;
        }
        let o = state
            .objects
            .try_get(d.source)
            .ok_or("combat creature absent")?;
        if o.zone != Zone::Battlefield
            || o.controller != state.active_player
            || !crate::engine::object_has_type(state, d.source, CardType::Creature)
        {
            continue;
        }
        let original = crate::state::AbilitySourceContractV4::capture(state, d.source);
        if let Some((_, _, packets)) = sources.iter_mut().find(|(s, _, _)| *s == original) {
            packets.push((d.target, d.amount));
        } else {
            sources.push((original, observers.clone(), vec![(d.target, d.amount)]));
        }
    }
    for (_, _, packets) in &sources {
        packets
            .iter()
            .try_fold(0_i32, |sum, (_, amount)| sum.checked_add(*amount))
            .ok_or("combat sum overflow")?;
    }
    Ok(WaveV1 { sources })
}
impl WaveV1 {
    pub(crate) fn finish(self, state: &mut GameState, history_start: usize) -> Result<(), String> {
        let damage_window_end =
            u64::try_from(state.engine.event_history.len()).map_err(|_| "damage window width")?;
        for (creature, observers, packets) in self.sources {
            let mut damage = vec![];
            let mut next = history_start;
            let mut amount = 0_i32;
            for (target, points) in packets {
                let (index, _) = state
                    .engine
                    .event_history
                    .iter()
                    .enumerate()
                    .skip(next)
                    .find(|(_, e)| {
                        **e == event::CommittedEvent::Damage {
                            source: creature.source,
                            target,
                            amount: points,
                        }
                    })
                    .ok_or("final combat damage did not commit")?;
                next = index + 1;
                damage.push(u64::try_from(index).map_err(|_| "damage index width")?);
                amount = amount.checked_add(points).ok_or("damage amount overflow")?;
            }
            let receipt = event::CommittedEvent::GreatwurmCombatCaptureV1 {
                capture: Box::new(CaptureV1 {
                    creature,
                    observers,
                    damage,
                    damage_window_start: u64::try_from(history_start)
                        .map_err(|_| "damage window width")?,
                    damage_window_end,
                    amount,
                }),
            };
            state.engine.event_history.push(receipt.clone());
            state.engine.event_log.push(receipt);
        }
        Ok(())
    }
}
/// Called only by the existing actual native combat finish owner, including
/// the private Twinflame replacement owner. A generic event commit lacks it.
pub(crate) fn bind_wave(state: &mut GameState, log_start: usize) -> Result<(), String> {
    let events = state
        .engine
        .event_log
        .get(log_start..)
        .ok_or("combat log boundary")?;
    if !events
        .iter()
        .any(|e| matches!(e, event::CommittedEvent::GreatwurmCombatCaptureV1 { .. }))
    {
        return Ok(());
    }
    let events = events.to_vec();
    let mut indices = vec![];
    for event in &events {
        if let event::CommittedEvent::GreatwurmCombatCaptureV1 { capture } = event {
            if !capture.damage.iter().all(|i| {
                usize::try_from(*i)
                    .ok()
                    .and_then(|i| state.engine.event_history.get(i))
                    .is_some_and(|e| events.contains(e))
            }) {
                return Err("combat capture outside actual wave".into());
            }
            let rows = state
                .engine
                .event_history
                .iter()
                .enumerate()
                .filter_map(|(i, e)| (e == event).then_some(i as u64))
                .collect::<Vec<_>>();
            let [index] = rows.as_slice() else {
                return Err("combat capture erased or duplicated".into());
            };
            if indices.iter().any(|(old,_)|old==index) || state.engine.event_history.iter().any(|e| matches!(e,event::CommittedEvent::GreatwurmCombatBoundV1{capture_history_index,..}if capture_history_index==index)) {
                return Err("combat capture already bound".into());
            }
            indices.push((*index, capture.clone()));
        }
    }
    for (capture_history_index, original) in indices {
        state
            .engine
            .event_history
            .push(event::CommittedEvent::GreatwurmCombatBoundV1 {
                capture_history_index,
                original,
            });
    }
    Ok(())
}
fn capture(state: &GameState, index: u64) -> Result<&CaptureV1, String> {
    let i = usize::try_from(index).map_err(|_| "capture width")?;
    let Some(event::CommittedEvent::GreatwurmCombatCaptureV1 { capture }) =
        state.engine.event_history.get(i)
    else {
        return Err("combat capture absent".into());
    };
    if capture.amount <= 0 || capture.damage.is_empty() || capture.observers.is_empty()
        || capture.damage_window_start >= capture.damage_window_end
        || capture.damage_window_end > index
        || state.engine.event_history.iter().filter(|e| *e == &state.engine.event_history[i]).count() != 1
        || capture.creature.zone != Zone::Battlefield
        || state.engine.event_history.iter().filter(|e|matches!(e,event::CommittedEvent::GreatwurmCombatBoundV1{capture_history_index,original}if *capture_history_index==index && original==capture)).count()!=1 {
        return Err("combat wave binding missing or changed".into());
    }
    let mut sum = 0_i32;
    let mut previous = None;
    for &damage in &capture.damage {
        let n = usize::try_from(damage).map_err(|_| "damage width")?;
        let Some(event::CommittedEvent::Damage { source, amount, .. }) =
            state.engine.event_history.get(n)
        else {
            return Err("original damage missing".into());
        };
        if damage < capture.damage_window_start
            || damage >= capture.damage_window_end
            || n >= i
            || previous.is_some_and(|old| old >= n)
            || *source != capture.creature.source
            || *amount <= 0
        {
            return Err("combat damage order/source changed".into());
        }
        sum = sum.checked_add(*amount).ok_or("damage sum overflow")?;
        previous = Some(n);
    }
    if sum != capture.amount {
        return Err("combat damage amount changed".into());
    }
    for (n, s) in std::iter::once(&capture.creature)
        .chain(capture.observers.iter())
        .enumerate()
    {
        if s.zone != Zone::Battlefield
            || s.controller != capture.creature.controller
            || n > 0 && (!definition(s.card_def) || capture.observers[..n - 1].contains(s))
            || state.objects.try_get(s.source).is_none_or(|o| {
                o.owner != s.owner
                    || o.zone_change_count < s.zone_change_count
                    || !crate::engine::source_card_def_matches(state, *s, o.card_def)
                    || o.zone_change_count == s.zone_change_count && o.zone != s.zone
            })
        {
            return Err("original combat observer/creature incarnation changed".into());
        }
    }
    Ok(capture)
}
pub(crate) fn marker() -> EffectOp {
    EffectOp::BindGreatwurmCountersV1
}
pub(crate) fn recipe(op: &EffectOp) -> bool {
    matches!(
        op,
        EffectOp::BindGreatwurmCountersV1 | EffectOp::GreatwurmCountersV1 { .. }
    )
}
pub(crate) fn collect(
    state: &mut GameState,
    events: &[event::CommittedEvent],
) -> Result<Vec<crate::trigger::PendingTrigger>, String> {
    let mut pending = vec![];
    let mut consumed = vec![];
    for e in events {
        if !matches!(e, event::CommittedEvent::GreatwurmCombatCaptureV1 { .. }) {
            continue;
        }
        let positions = state
            .engine
            .event_history
            .iter()
            .enumerate()
            .filter_map(|(i, old)| (e == old).then_some(i as u64))
            .collect::<Vec<_>>();
        let [index] = positions.as_slice() else {
            return Err("combat capture duplicated/erased".into());
        };
        let c = capture(state, *index)?;
        for (observer, source) in c.observers.iter().enumerate() {
            let observer = u32::try_from(observer).map_err(|_| "observer width")?;
            if consumed.contains(&(*index,observer)) || state.engine.event_history.iter().any(|e|matches!(e,event::CommittedEvent::GreatwurmCollectedV1{capture_history_index,observer:o}if capture_history_index==index && *o==observer)){return Err("combat recollected".into());}
            pending.push(crate::trigger::PendingTrigger {
                controller: source.controller,
                source: source.source,
                effect: EffectOp::GreatwurmCountersV1 {
                    capture_history_index: *index,
                    observer,
                },
                is_madness_offer: false,
                kicked: false,
                target_spec: card_def::TargetSpec::None,
                targets: vec![],
                target_contracts: vec![],
                placement_ordered: false,
                source_contract: Some(*source),
                granted_by: None,
                optional_additional_cost_paid: None,
                paid_cost_refs: vec![],
            });
            consumed.push((*index, observer));
        }
    }
    for (capture_history_index, observer) in consumed {
        state
            .engine
            .event_history
            .push(event::CommittedEvent::GreatwurmCollectedV1 {
                capture_history_index,
                observer,
            });
    }
    Ok(pending)
}
pub(crate) fn validate_root(
    state: &GameState,
    op: &EffectOp,
    source: Option<crate::state::AbilitySourceContractV4>,
) -> Result<(), String> {
    let EffectOp::GreatwurmCountersV1 {
        capture_history_index,
        observer,
    } = op
    else {
        return if recipe(op) {
            Err("unbound combat template".into())
        } else {
            Ok(())
        };
    };
    let c = capture(state, *capture_history_index)?;
    if c.observers.get(*observer as usize).copied()!=source || state.engine.event_history.iter().filter(|e|matches!(e,event::CommittedEvent::GreatwurmCollectedV1{capture_history_index:i,observer:o}if i==capture_history_index && o==observer)).count()!=1{return Err("combat observer/program binding changed".into());}
    Ok(())
}
pub(crate) fn selected(state: &GameState, item: &StackItem) -> bool {
    state.engine.event_history.iter().any(|e|matches!(e,event::CommittedEvent::GreatwurmPublishedV1{item:row,..}if row.v4.stack_item_id==item.v4.stack_item_id))
        || item.kind==crate::state::StackItemKind::TriggeredAbility && item.v4.granted_by.is_none()
            && item.inline_effect.as_ref().is_some_and(recipe)
}
pub(crate) fn waiting(state: &GameState, ctx: &ExecCtx) -> bool {
    state.stack.iter().any(|r| {
        selected(state, r)
            && (r.source == ctx.source
                || r.v4.ability_source_contract.is_some()
                    && r.v4.ability_source_contract == ctx.ability_source_contract)
    })
}
pub(crate) fn validate_item(state: &GameState, item: &StackItem) -> Result<(), String> {
    let op = item.inline_effect.as_ref().ok_or("combat root erased")?;
    validate_root(state, op, item.v4.ability_source_contract)?;
    let source = item
        .v4
        .ability_source_contract
        .ok_or("combat source absent")?;
    if !recipe(op) || item.kind!=crate::state::StackItemKind::TriggeredAbility || item.source!=source.source || item.controller!=source.controller
        || item.is_copy || item.kicked || item.is_flashback || item.madness_offer || item.mode_chosen!=0 || item.v4.x_value!=0
        || item.v4.granted_by.is_some() || item.v4.source_contract.is_some() || item.v4.activated_ability_index.is_some()
        || item.v4.target_spec.is_some() || !item.targets.is_empty() || !item.v4.target_contracts.is_empty()
        || !item.discarded.is_empty() || !item.v4.paid_cost_refs.is_empty() || item.v4.optional_additional_cost_paid.is_some()
        || item.v4.hidden_ability_source.is_some() || item.v4.cauldron_grant.0.is_some()
        || state.engine.event_history.iter().any(|e|matches!(e,event::CommittedEvent::GreatwurmFinishedV1{stack_item_id}if *stack_item_id==item.v4.stack_item_id)){return Err("combat row/controller/recipe changed or consumed".into());}
    Ok(())
}
pub(crate) fn publish(state: &mut GameState, item: &StackItem) -> Result<(), String> {
    let Some(EffectOp::GreatwurmCountersV1 {
        capture_history_index,
        observer,
    }) = item.inline_effect
    else {
        return Ok(());
    };
    validate_item(state, item)?;
    if state.engine.event_history.iter().any(|e|matches!(e,event::CommittedEvent::GreatwurmPublishedV1{capture_history_index:i,observer:o,..}if *i==capture_history_index && *o==observer)){return Err("combat trigger already published".into());}
    state
        .engine
        .event_history
        .push(event::CommittedEvent::GreatwurmPublishedV1 {
            capture_history_index,
            observer,
            item: Box::new(item.clone()),
        });
    Ok(())
}
pub(crate) fn validate_published(state: &GameState, item: &StackItem) -> Result<(), String> {
    validate_item(state, item)?;
    let rows = state
        .engine
        .event_history
        .iter()
        .filter_map(|e| match e {
            event::CommittedEvent::GreatwurmPublishedV1 { item: row, .. }
                if row.v4.stack_item_id == item.v4.stack_item_id =>
            {
                Some(row.as_ref())
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    if rows != [item] {
        return Err("combat publication row changed".into());
    }
    Ok(())
}
pub(super) fn execute(
    op: &EffectOp,
    ctx: &ExecCtx,
    state: &mut GameState,
    proof: &NativeExecutionProofV1<'_>,
) -> Result<(), String> {
    if proof.ctx != ctx
        || *proof.owner.root != *op
        || proof.owner.item.v4.ability_source_contract != ctx.ability_source_contract
    {
        return Err("combat lexical owner mismatch".into());
    }
    validate_published(state, &proof.owner.item)?;
    let EffectOp::GreatwurmCountersV1 {
        capture_history_index,
        ..
    } = op
    else {
        return Err("combat unbound template".into());
    };
    let c = capture(state, *capture_history_index)?;
    let (creature, amount) = (c.creature, c.amount);
    if state.objects.try_get(creature.source).is_some_and(|o| {
        o.zone == Zone::Battlefield && o.zone_change_count == creature.zone_change_count
    }) {
        let controller = state.objects.get(creature.source).controller;
        let mut staged = state.clone();
        let mut counters = crate::standard_creatures_v1::CounterTransferV1::default();
        counters.counters.plus1_plus1 = amount;
        crate::standard_creatures_v1::place_counters_checked_v1(
            &mut staged,
            creature.source,
            controller,
            counters,
        )
        .ok_or("combat counter placement overflow")?;
        crate::engine::validate_battlefield_pt(&staged).map_err(|_| "combat PT envelope")?;
        *state = staged;
    }
    state
        .engine
        .event_history
        .push(event::CommittedEvent::GreatwurmFinishedV1 {
            stack_item_id: proof.owner.item.v4.stack_item_id,
        });
    Ok(())
}
pub(crate) fn refuse_legacy(state: &GameState) -> Result<(), String> {
    if crate::standard_cards_v1::has_battlefield_copy_origin_v1(state, definition)
        || state.objects.iter().any(|(_, o)| {
            matches!(o.zone, Zone::Battlefield | Zone::Stack | Zone::Graveyard)
                && o.v4.face_down_v1.is_none()
                && definition(o.card_def)
        })
        || state.engine.event_history.iter().any(|e| {
            matches!(
                e,
                event::CommittedEvent::GreatwurmCounterPaymentV1 { .. }
                    | event::CommittedEvent::GreatwurmCombatCaptureV1 { .. }
            )
        })
    {
        return Err("Greatwurm requires a distinct complete consumer".into());
    }
    Ok(())
}

pub(crate) fn definition(id: u16) -> bool {
    cfg!(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    )) && id == 476
        && card_def::CARD_DEFS.get(usize::from(id)).is_some_and(|d| {
            let card_def::CardDef {
                name: _,
                capability: _,
                cost: _,
                generic_cost_reduction: _,
                ward_cost: _,
                spell_cannot_be_countered: _,
                records_mana_spent: _,
                equipment: _,
                types: _,
                subtypes: _,
                supertypes: _,
                power: _,
                toughness: _,
                starting_loyalty: _,
                enters_with_plus_one_counters: _,
                controlled_counter_keyword: _,
                game_outcome_protection_v1: _,
                controlled_creature_grant: _,
                is_land: _,
                produces_mana: _,
                colors: _,
                target_spec: _,
                keywords: _,
                spell_effect: _,
                mana_ability: _,
                alt_cost: _,
                kicker_cost: _,
                additional_cost: _,
                flashback: _,
                activated_abilities: _,
                plot_cost: _,
                madness_cost: _,
                mode2: _,
                mode3: _,
                additional_modes: _,
                kicked_target_spec: _,
                is_token: _,
                escape: _,
                mana_ability_choices: _,
                enters_battlefield_tapped: _,
                mana_ability_def: _,
                minimum_blockers: _,
                omen: _,
                mana_value: _,
                mana_ability_includes_chosen_color: _,
                as_enters_choose_color_other_than: _,
                additional_mana_abilities: _,
                object_name: _,
                enters_battlefield_tapped_unless: _,
                attachment: _,
                transform_face: _,
                saga: _,
                optional_additional_cost: _,
                changeling: _,
                bestow: _,
                delve: _,
                adventure: _,
                cant_be_blocked_by_monarchs_creatures: _,
                conditional_tap_yield: _,
                enters_tapped_unless_controller: _,
                additional_mana_ability_conditions: _,
                restricted_mana_abilities: _,
                animation: _,
                activated_ability_generic_reductions: _,
                combat_static_rules: _,
                as_enters_color_choice_v1: _,
                chosen_color_creature_boost_v1: _,
                damage_replacement_v1: _,
                token_counter_replacement_v48: _,
                starting_life_self_bonuses_v1: _,
                source_type_hexproof_v1: _,
                other_controlled_nontoken_death_v1: _,
                creature_type_entry_v1: _,
            } = d;
            d.name == "Quilled Greatwurm"
                && d.capability == card_def::CardCapability::Partial
                && d.cost
                    == crate::mana::Cost {
                        pips: &[
                            crate::mana::Pip::Colored(crate::mana::ManaColor::G),
                            crate::mana::Pip::Colored(crate::mana::ManaColor::G),
                        ],
                        generic: 4,
                        x_count: 0,
                    }
                && d.generic_cost_reduction.is_none()
                && d.ward_cost.is_none()
                && !d.spell_cannot_be_countered
                && !d.records_mana_spent
                && d.equipment.is_none()
                && d.types == [CardType::Creature]
                && d.subtypes == [card_def::Subtype::Wurm]
                && d.supertypes.is_empty()
                && d.power == Some(7)
                && d.toughness == Some(7)
                && d.starting_loyalty.is_none()
                && d.enters_with_plus_one_counters.is_none()
                && d.controlled_counter_keyword.is_none()
                && d.game_outcome_protection_v1.is_none()
                && d.controlled_creature_grant.is_none()
                && !d.is_land
                && d.produces_mana.is_empty()
                && d.colors == [crate::mana::ManaColor::G]
                && d.target_spec == card_def::TargetSpec::None
                && d.keywords == Keywords::TRAMPLE
                && (d.spell_effect)()
                    == Some(EffectOp::MoveObject {
                        object: ObjectRef::ThisSource,
                        to_zone: Zone::Battlefield,
                    })
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
        })
}

#[cfg(all(
    test,
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
#[path = "greatwurm_v1_tests.rs"]
mod tests;
