//! Definition-owned, committed source-power authority. No field is added to
//! ordinary snapshots, and no receipt is emitted for predecessor abilities.
use crate::card_def::{CardType, CARD_DEFS};
use crate::effect::{EffectObjectBinding, EffectOp, ExecCtx};
use crate::event::{self, CommittedEvent, HistoricalDamageSourceV1, ProposedEvent};
use crate::ids::{ObjectId, StackItemId};
use crate::standard_cards_v1::StandardOpV1;
use crate::state::{GameState, StackItem, StackItemKind, Target, Zone};

fn is_program(effect: Option<&EffectOp>) -> bool {
    matches!(
        effect,
        Some(EffectOp::StandardV1(StandardOpV1::SourceLastPowerDamageV1))
    )
}

pub(crate) fn needs_stats(state: &GameState, object: ObjectId) -> bool {
    state.objects.try_get(object).is_some_and(|live| {
        CARD_DEFS
            .get(usize::from(live.card_def))
            .is_some_and(|def| {
                def.activated_abilities
                    .iter()
                    .any(|ability| is_program(Some(&(ability.effect)())))
            })
    })
}

fn capture_damage_source(state: &GameState, object: ObjectId) -> HistoricalDamageSourceV1 {
    HistoricalDamageSourceV1 {
        source: crate::state::AbilitySourceContractV4::capture(state, object),
        color_mask: crate::engine::object_color_mask(state, object),
        lifelink: crate::engine::has_effective_keyword(
            state,
            object,
            crate::card_def::Keywords::LIFELINK,
        ),
        deathtouch: crate::engine::has_effective_keyword(
            state,
            object,
            crate::card_def::Keywords::DEATHTOUCH,
        ),
        werewolf: crate::engine::has_effective_subtype(
            state,
            object,
            crate::card_def::Subtype::Werewolf,
        ),
    }
}

pub(crate) fn before_departure(state: &mut GameState, source: ObjectId) {
    if !needs_stats(state, source) {
        return;
    }
    crate::creature_stats_lki_v1::record_creature_stats_lki(state, source);
    let damage_source = capture_damage_source(state, source);
    state
        .engine
        .event_history
        .push(CommittedEvent::SourceDamageBeforeLeavingV1 { damage_source });
}

/// Runs in the atomic payment/publication action. The existing departure
/// cache was sampled at the actual zone change, after mana payment. Freeze
/// it beside the complete finalized activation before handing out priority.
pub(crate) fn record_paid(state: &mut GameState, id: StackItemId) {
    let item = state
        .stack
        .iter()
        .find(|item| item.v4.stack_item_id == id)
        .unwrap();
    if !is_program(item.inline_effect.as_ref()) {
        return;
    }
    let source = item
        .v4
        .ability_source_contract
        .expect("paid source contract");
    let binding = EffectObjectBinding {
        object: source.source,
        expected_zone: source.zone,
        expected_zone_change_count: source.zone_change_count,
    };
    let (power, toughness) =
        crate::creature_stats_lki_v1::current_or_last_creature_stats(state, binding)
            .expect("source-sacrifice payment recorded exact departing creature stats");
    let damage_source = state
        .engine
        .event_history
        .iter()
        .find_map(|event| match event {
            CommittedEvent::SourceDamageBeforeLeavingV1 { damage_source }
                if damage_source.source == source =>
            {
                Some(*damage_source)
            }
            _ => None,
        })
        .expect("atomic source sacrifice captured historical damage characteristics");
    state
        .engine
        .event_history
        .push(CommittedEvent::SourcePowerActivationCommittedV1 {
            item: Box::new(item.clone()),
            power,
            toughness,
            damage_source,
        });
}

/// The captured printed definition still selects this mandatory authority even
/// when a restored item erases its redundant inline program.
fn selected_definition_uses_program(item: &StackItem) -> bool {
    if item.kind != StackItemKind::ActivatedAbility {
        return false;
    }
    let Some(source) = item.v4.ability_source_contract else {
        return false;
    };
    let Some(index) = item.v4.activated_ability_index else {
        return false;
    };
    CARD_DEFS
        .get(usize::from(source.card_def))
        .and_then(|def| def.activated_abilities.get(usize::from(index)))
        .is_some_and(|ability| is_program(Some(&(ability.effect)())))
}

pub(crate) fn validate_item(state: &GameState, item: &StackItem) -> Result<(), String> {
    if !is_program(item.inline_effect.as_ref()) {
        if selected_definition_uses_program(item)
            || state.engine.event_history.iter().any(|event| {
                matches!(event,
                CommittedEvent::SourcePowerActivationCommittedV1 { item: saved, .. }
                if saved.v4.stack_item_id == item.v4.stack_item_id)
            })
        {
            return Err(
                "source-power activation changed its selected definition-owned program".into(),
            );
        }
        return Ok(());
    }
    if state.engine.event_history.iter().any(|event|matches!(event,
        CommittedEvent::SourcePowerActivationFinishedV1 {stack_item_id} if *stack_item_id==item.v4.stack_item_id)) {
        return Err("source-power activation was already finished".into());
    }
    if item.kind != StackItemKind::ActivatedAbility
        || state.engine.event_log.iter().any(|event| {
            matches!(
                event,
                CommittedEvent::SourcePowerActivationCommittedV1 { .. }
                    | CommittedEvent::SourceDamageBeforeLeavingV1 { .. }
            )
        })
    {
        return Err("source-power activation has invalid producer/log authority".into());
    }
    let mut receipts = state
        .engine
        .event_history
        .iter()
        .filter_map(|event| match event {
            CommittedEvent::SourcePowerActivationCommittedV1 {
                item: saved,
                power,
                toughness,
                damage_source,
            } if saved.v4.stack_item_id == item.v4.stack_item_id => {
                Some((saved, *power, *toughness, *damage_source))
            }
            _ => None,
        });
    let (saved, power, toughness, captured_damage_source) = receipts
        .next()
        .ok_or("source-power activation lost committed payment")?;
    if receipts.next().is_some() || saved.as_ref() != item {
        return Err("source-power activation differs from its unique committed payment".into());
    }
    if state
        .stack
        .iter()
        .filter(|live| live.v4.stack_item_id == item.v4.stack_item_id)
        .count()
        != 1
    {
        return Err("source-power activation lacks unique live stack incarnation".into());
    }
    let source = item
        .v4
        .ability_source_contract
        .ok_or("source-power activation lost source")?;
    let captures = state
        .engine
        .event_history
        .iter()
        .filter_map(|event| match event {
            CommittedEvent::SourceDamageBeforeLeavingV1 {
                damage_source: captured,
            } if captured.source.source == source.source
                && captured.source.zone_change_count == source.zone_change_count =>
            {
                Some(*captured)
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    if captures.as_slice() != [captured_damage_source] || captured_damage_source.source != source {
        return Err("historical damage source differs from committed departure".into());
    }
    if damage_source(state, item.source, item.v4.stack_item_id)? != captured_damage_source {
        return Err("historical damage source changed".into());
    }
    let entries = state
        .creature_stats_lki_v1
        .as_ref()
        .ok_or("source-power activation lost departure stats")?;
    let mut matches = entries.iter().filter(|entry| {
        entry.source.object == source.source
            && entry.source.zone_change_count == source.zone_change_count
    });
    let stats = matches
        .next()
        .ok_or("source-power activation lost exact departure stats")?;
    if matches.next().is_some()
        || (stats.power, stats.toughness) != (power, toughness)
        || source.zone != Zone::Battlefield
        || source.controller != item.controller
        || item
            .v4
            .paid_cost_refs
            .iter()
            .filter(|paid| paid.object == source.source)
            .count()
            != 1
    {
        return Err("source-power activation departure stats or source payment changed".into());
    }
    Ok(())
}

pub(crate) fn finish(state: &mut GameState, item: &StackItem) {
    if is_program(item.inline_effect.as_ref()) {
        state
            .engine
            .event_history
            .push(CommittedEvent::SourcePowerActivationFinishedV1 {
                stack_item_id: item.v4.stack_item_id,
            });
    }
}

pub(crate) fn validate_state(state: &GameState) -> Result<(), String> {
    // The operation owns a mandatory payment receipt. Receipt presence cannot
    // decide whether its live activation is validated: deletion must refuse
    // before any action mutates priority, mana, targets or continuation state.
    for item in &state.stack {
        validate_item(state, item)?;
    }
    if state.engine.event_log.iter().any(|event| {
        matches!(
            event,
            CommittedEvent::SourcePowerActivationCommittedV1 { .. }
                | CommittedEvent::SourceDamageBeforeLeavingV1 { .. }
                | CommittedEvent::SourcePowerActivationFinishedV1 { .. }
        )
    }) {
        return Err("historical activation authority appeared in transient event log".into());
    }
    let mut seen = Vec::new();
    for (receipt_index, event) in state.engine.event_history.iter().enumerate() {
        let CommittedEvent::SourcePowerActivationCommittedV1 { item, .. } = event else {
            continue;
        };
        let id = item.v4.stack_item_id;
        if seen.contains(&id) {
            return Err("duplicate source-power activation receipt".into());
        }
        seen.push(id);
        let finishes = state
            .engine
            .event_history
            .iter()
            .filter(|event| {
                matches!(event,
            CommittedEvent::SourcePowerActivationFinishedV1 {stack_item_id} if *stack_item_id==id)
            })
            .count();
        if state.engine.event_history[..receipt_index]
            .iter()
            .any(|event| {
                matches!(event,
            CommittedEvent::SourcePowerActivationFinishedV1 {stack_item_id} if *stack_item_id==id)
            })
        {
            return Err("source-power completion precedes its committed payment".into());
        }
        let live = state
            .stack
            .iter()
            .filter(|candidate| candidate.v4.stack_item_id == id)
            .collect::<Vec<_>>();
        match (finishes, live.as_slice()) {
            (0, [candidate]) => validate_item(state, candidate)?,
            (1, []) => {}
            _ => return Err("source-power activation ownership was erased or replayed".into()),
        }
    }
    if state.engine.event_history.iter().any(|event|matches!(event,
        CommittedEvent::SourcePowerActivationFinishedV1 {stack_item_id} if !seen.contains(stack_item_id))) {
        return Err("source-power completion has no committed payment".into());
    }
    Ok(())
}

/// Internal ephemeral proposals must refer to one actual committed payment.
/// No caller may supply mutable replacement facts beside an unrelated id.
pub(crate) fn damage_source(
    state: &GameState,
    source: ObjectId,
    id: StackItemId,
) -> Result<HistoricalDamageSourceV1, String> {
    let mut receipts =
        state
            .engine
            .event_history
            .iter()
            .enumerate()
            .filter_map(|(index, event)| match event {
                CommittedEvent::SourcePowerActivationCommittedV1 {
                    item,
                    damage_source,
                    ..
                } if item.v4.stack_item_id == id && item.source == source => {
                    Some((index, *damage_source))
                }
                _ => None,
            });
    let (receipt_index, facts) = receipts
        .next()
        .ok_or("historical damage lost its committed activation")?;
    if receipts.next().is_some() {
        return Err("historical damage has duplicate committed activation".into());
    }
    let captures = state
        .engine
        .event_history
        .iter()
        .enumerate()
        .filter_map(|(index, event)| match event {
            CommittedEvent::SourceDamageBeforeLeavingV1 { damage_source }
                if damage_source.source.source == source
                    && damage_source.source.zone_change_count == facts.source.zone_change_count =>
            {
                Some((index, *damage_source))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    let [(capture_index, captured)] = captures.as_slice() else {
        return Err("historical damage departure authority changed".into());
    };
    if *captured!=facts || *capture_index>=receipt_index || !state.engine.event_history[*capture_index+1..receipt_index].iter().any(|event|matches!(event,
        CommittedEvent::ZoneChange {object,from:Zone::Battlefield,controller_before,..} if *object==source && *controller_before==facts.source.controller)) {
        return Err("historical damage lost committed departure ordering".into());
    }
    Ok(facts)
}

pub(crate) fn legal_targets(state: &GameState, facts: HistoricalDamageSourceV1) -> Vec<Target> {
    crate::engine::legal_targets_for_controller(
        crate::card_def::TargetSpec::CreatureOrPlaneswalker,
        &[],
        facts.source.controller,
        state,
    )
    .into_iter()
    .filter(|target| {
        let Target::Object(target) = *target else {
            return false;
        };
        !protected(state, facts, target)
            && !crate::standard_legends_v1::hexproof_from_historical_color(
                state,
                target,
                facts.color_mask,
                facts.source.controller,
            )
    })
    .collect()
}

pub(crate) fn protected(
    state: &GameState,
    facts: HistoricalDamageSourceV1,
    target: ObjectId,
) -> bool {
    state.objects.try_get(target).is_some_and(|object| {
        object.zone == Zone::Battlefield
            && (crate::engine::has_effective_keyword(
                state,
                target,
                crate::card_def::Keywords::PROTECTION_FROM_MONOCOLORED,
            ) && (facts.color_mask & 0x1f).count_ones() == 1
                || crate::standard_legends_v1::katilda_protected_from_historical(
                    state,
                    target,
                    facts.werewolf,
                ))
    })
}

pub(crate) fn current_targets(state: &GameState, source: ObjectId) -> Vec<Target> {
    legal_targets(state, capture_damage_source(state, source))
}

pub(crate) fn execute(ctx: &ExecCtx, state: &mut GameState) {
    let Some(Target::Object(target)) = ctx.targets.first().copied() else {
        return;
    };
    if !ctx.target_incarnation_matches(0, state)
        || state.objects.get(target).zone != Zone::Battlefield
        || !(crate::engine::object_has_type(state, target, CardType::Creature)
            || crate::engine::object_has_type(state, target, CardType::Planeswalker))
    {
        return;
    }
    let power = state
        .engine
        .event_history
        .iter()
        .find_map(|event| match event {
            CommittedEvent::SourcePowerActivationCommittedV1 { item, power, .. }
                if Some(item.v4.stack_item_id) == ctx.stack_item_id
                    && item.source == ctx.source
                    && item.controller == ctx.controller
                    && item.v4.ability_source_contract == ctx.ability_source_contract =>
            {
                Some(*power)
            }
            _ => None,
        })
        .expect("validated definition-owned source-power receipt")
        .max(0);
    if power > 0 {
        let mut proposal = ProposedEvent::damage(ctx.source, Target::Object(target), power);
        if let ProposedEvent::Damage(damage) = &mut proposal {
            damage.historical_source = ctx.stack_item_id;
        }
        event::propose_and_commit(state, proposal);
    }
}

#[cfg(test)]
#[path = "source_power_activation_v1_legacy_tests.rs"]
mod legacy_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::card_def::card_id_by_name;
    use crate::ids::PlayerId;
    #[test]
    fn registered_elf_stats_boundary_is_incarnation_exact_and_old_path_emits_no_receipt() {
        // Registered stand-in verifies only existing stats/LKI mechanics.
        // It never pretends to be a printed Heartfire activation.
        for actor in [PlayerId::P0, PlayerId::P1] {
            let elf = card_id_by_name("Llanowar Elves").unwrap();
            let mut state = GameState::new_from_libraries_with_starting_player_v1(
                &[elf; 40],
                &[elf; 40],
                |id| CARD_DEFS[id as usize].object_name.into(),
                110_309,
                actor,
            );
            let source = state.players[actor.index()].library[0];
            event::propose_and_commit(
                &mut state,
                ProposedEvent::zone_change(source, Zone::Battlefield),
            );
            let binding = EffectObjectBinding {
                object: source,
                expected_zone: Zone::Battlefield,
                expected_zone_change_count: state.objects.get(source).zone_change_count,
            };
            assert!(!needs_stats(&state, source));
            event::add_plus_one_counters(&mut state, source, actor, 2).unwrap();
            crate::effect::install_temporary_boost(
                &mut state,
                binding,
                -5,
                0,
                crate::card_def::Keywords::NONE,
            );
            assert_eq!(
                crate::creature_stats_lki_v1::current_or_last_creature_stats(&state, binding),
                Some((-2, 3))
            );
            crate::creature_stats_lki_v1::record_creature_stats_lki(&mut state, source);
            event::propose_and_commit(
                &mut state,
                ProposedEvent::zone_change(source, Zone::Graveyard),
            );
            let encoded = serde_json::to_vec(&state).unwrap();
            state = serde_json::from_slice(&encoded).unwrap();
            assert_eq!(
                crate::creature_stats_lki_v1::current_or_last_creature_stats(&state, binding),
                Some((-2, 3))
            );
            event::propose_and_commit(
                &mut state,
                ProposedEvent::zone_change(source, Zone::Battlefield),
            );
            event::add_plus_one_counters(&mut state, source, actor, 20).unwrap();
            assert_eq!(
                crate::creature_stats_lki_v1::current_or_last_creature_stats(&state, binding),
                Some((-2, 3))
            );
            assert!(state.engine.event_history.iter().all(|event| !matches!(
                event,
                CommittedEvent::SourcePowerActivationCommittedV1 { .. }
            )));
        }
    }
}
