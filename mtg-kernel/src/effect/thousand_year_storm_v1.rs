//! Native FDN248 Thousand-Year Storm: finalized cast snapshots and independent copies.
//! No predecessor public projection or caller-authored operation supplies authority.
use super::*;
pub(crate) fn definition(id: u16) -> bool {
    cfg!(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    )) && id == 485
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
            d.name == "Thousand-Year Storm"
                && d.capability == card_def::CardCapability::Partial
                && d.cost
                    == crate::mana::Cost {
                        pips: &[
                            crate::mana::Pip::Colored(crate::mana::ManaColor::U),
                            crate::mana::Pip::Colored(crate::mana::ManaColor::R),
                        ],
                        generic: 4,
                        x_count: 0,
                    }
                && d.generic_cost_reduction.is_none()
                && d.ward_cost.is_none()
                && !d.spell_cannot_be_countered
                && !d.records_mana_spent
                && d.equipment.is_none()
                && d.types == [CardType::Enchantment]
                && d.subtypes.is_empty()
                && d.supertypes.is_empty()
                && d.power.is_none()
                && d.toughness.is_none()
                && d.starting_loyalty.is_none()
                && d.enters_with_plus_one_counters.is_none()
                && d.controlled_counter_keyword.is_none()
                && d.game_outcome_protection_v1.is_none()
                && d.controlled_creature_grant.is_none()
                && !d.is_land
                && d.produces_mana.is_empty()
                && d.colors == [crate::mana::ManaColor::U, crate::mana::ManaColor::R]
                && d.target_spec == card_def::TargetSpec::None
                && d.keywords == Keywords::NONE
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

/// Only successful physical casts enter this history. Copies never call the cast hook.
/// The catalog-scoped roster includes earlier casts before an observer enters.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum JournalV1 {
    Cast {
        event: u64,
        spell: Box<StackItem>,
        observers: Vec<AbilitySourceContractV4>,
    },
    Collected {
        cast: u64,
    },
    Published {
        cast: u64,
        item: Box<StackItem>,
    },
    Copy {
        trigger: StackItemId,
        ordinal: u16,
        item: Box<StackItem>,
    },
}
fn enabled() -> bool {
    card_def::card_id_by_name("Thousand-Year Storm").is_some_and(definition)
}
fn append(state: &mut GameState, journal: JournalV1, visible_to_collector: bool) {
    let event = event::CommittedEvent::ThousandYearStormV1 { journal };
    if visible_to_collector {
        state.engine.event_log.push(event.clone());
    }
    state.engine.event_history.push(event);
}
fn at(state: &GameState, index: u64) -> Result<&JournalV1, String> {
    match state
        .engine
        .event_history
        .get(usize::try_from(index).map_err(|_| "Storm index width")?)
    {
        Some(event::CommittedEvent::ThousandYearStormV1 { journal }) => Ok(journal),
        _ => Err("Storm original cast receipt absent".into()),
    }
}
fn instant_or_sorcery(spell: &StackItem) -> Result<bool, String> {
    let source = spell
        .v4
        .source_contract
        .ok_or("Storm spell source absent")?;
    let d = card_def::CARD_DEFS
        .get(usize::from(source.card_def))
        .ok_or("Storm spell definition absent")?;
    let types = if source.cast_method == crate::state::CastMethodV4::Omen {
        d.omen_spell_form()
            .ok_or("Storm alternative form absent")?
            .1
    } else {
        d.types
    };
    Ok(types.contains(&CardType::Instant) || types.contains(&CardType::Sorcery))
}
pub(crate) fn record_cast(state: &mut GameState, spell: ObjectId) -> Result<(), String> {
    if !enabled() {
        return Ok(());
    }
    let row = state
        .stack
        .iter()
        .find(|r| r.kind == crate::state::StackItemKind::Spell && r.source == spell)
        .ok_or("Storm finalized cast row absent")?
        .clone();
    if !instant_or_sorcery(&row)? {
        return Ok(());
    }
    crate::engine::validate_copy_snapshot(state, &row)?;
    if row.is_copy {
        return Err("Storm copies cannot be recorded as casts".into());
    }
    let event = state
        .engine
        .event_history
        .len()
        .checked_sub(1)
        .ok_or("Storm cast event absent")?;
    if state.engine.event_history[event]
        != (event::CommittedEvent::SpellCast {
            spell,
            controller: row.controller,
        })
    {
        return Err("Storm cast hook is not adjacent to actual cast".into());
    }
    let mut observers = vec![];
    for (id, o) in state.objects.iter() {
        if o.zone == Zone::Battlefield
            && o.controller == row.controller
            && o.v4.face_index == 0
            && o.v4.face_down_v1.is_none()
            && definition(o.card_def)
            && crate::continuous_characteristics_v1::printed_abilities_active(state, id)
        {
            observers.push(AbilitySourceContractV4::capture(state, id));
        }
    }
    append(
        state,
        JournalV1::Cast {
            event: event as u64,
            spell: Box::new(row),
            observers,
        },
        true,
    );
    Ok(())
}
fn cast_at(
    state: &GameState,
    index: u64,
) -> Result<(&StackItem, &[AbilitySourceContractV4]), String> {
    let JournalV1::Cast {
        event,
        spell,
        observers,
    } = at(state, index)?
    else {
        return Err("Storm receipt is not a cast".into());
    };
    if event.checked_add(1)!=Some(index)||spell.is_copy||!instant_or_sorcery(spell)?
        ||state.engine.event_history.get(*event as usize)!=Some(&event::CommittedEvent::SpellCast{spell:spell.source,controller:spell.controller})
        ||state.engine.event_history.iter().filter(|e|matches!(e,event::CommittedEvent::ThousandYearStormV1{journal:JournalV1::Cast{event:i,..}}if i==event)).count()!=1
        ||!observers.windows(2).all(|p|p[0].source<p[1].source) {return Err("Storm cast/observer occurrence changed".into())}
    if state.engine.event_history.iter().filter(|e|matches!(e,event::CommittedEvent::ThousandYearStormV1{journal:JournalV1::Cast{spell:other,..}} if other.v4.stack_item_id==spell.v4.stack_item_id)).count()!=1 {
        return Err("Storm actual finalized cast was replayed".into());
    }
    crate::engine::validate_copy_snapshot(state, spell)?;
    for source in observers {
        if !definition(source.card_def)
            || source.zone != Zone::Battlefield
            || source.controller != spell.controller
        {
            return Err("Storm original observer changed".into());
        }
        crate::engine::validate_historical_ability_source_contract(state, *source)?;
    }
    Ok((spell, observers))
}
fn count(state: &GameState, index: u64) -> Result<u16, String> {
    let (spell, _) = cast_at(state, index)?;
    let start = state.engine.event_history[..index as usize]
        .iter()
        .rposition(|e| matches!(e, event::CommittedEvent::LifeLossTurnBeganV1 { .. }))
        .map_or(0, |i| i + 1);
    let mut count = 0u16;
    for (i, e) in state
        .engine
        .event_history
        .iter()
        .enumerate()
        .take(index as usize)
        .skip(start)
    {
        if let event::CommittedEvent::ThousandYearStormV1 {
            journal: JournalV1::Cast { spell: prior, .. },
        } = e
        {
            cast_at(state, i as u64)?;
            if prior.controller == spell.controller {
                count = count
                    .checked_add(1)
                    .ok_or("Storm copy count exceeds native path width")?;
            }
        }
    }
    Ok(count)
}
fn program(state: &GameState, index: u64) -> Result<EffectOp, String> {
    let (spell, _) = cast_at(state, index)?;
    let mut ops = vec![EffectOp::ThousandYearStormCaptureV1 { cast: index }];
    for _ in 0..count(state, index)? {
        ops.push(EffectOp::CopySpellSnapshot {
            spell: Box::new(spell.clone()),
        });
    }
    Ok(EffectOp::Sequence(ops))
}
pub(crate) fn capture(op: &EffectOp) -> Option<u64> {
    match op {
        EffectOp::Sequence(ops) => match ops.first() {
            Some(EffectOp::ThousandYearStormCaptureV1 { cast }) => Some(*cast),
            _ => None,
        },
        _ => None,
    }
}
pub(crate) fn recipe(op: &EffectOp) -> bool {
    capture(op).is_some()
        || matches!(
            op,
            EffectOp::BindThousandYearStormV1 | EffectOp::ThousandYearStormCaptureV1 { .. }
        )
}
pub(crate) fn collect(
    state: &mut GameState,
    events: &[event::CommittedEvent],
) -> Result<Vec<crate::trigger::PendingTrigger>, String> {
    let mut out = vec![];
    let mut seen = vec![];
    for e in events {
        let event::CommittedEvent::ThousandYearStormV1 {
            journal: JournalV1::Cast { event, .. },
        } = e
        else {
            continue;
        };
        let index = event.checked_add(1).ok_or("Storm occurrence width")?;
        if state.engine.event_history.get(index as usize)!=Some(e)||seen.contains(&index)
            ||state.engine.event_history.iter().any(|e|matches!(e,event::CommittedEvent::ThousandYearStormV1{journal:JournalV1::Collected{cast}}if *cast==index)) {return Err("Storm cast erased, duplicated or recollected".into())}
        let (_, observers) = cast_at(state, index)?;
        for source in observers {
            out.push(crate::trigger::PendingTrigger {
                controller: source.controller,
                source: source.source,
                effect: program(state, index)?,
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
        }
        seen.push(index);
    }
    for cast in seen {
        append(state, JournalV1::Collected { cast }, false)
    }
    Ok(out)
}
pub(crate) fn validate_item(state: &GameState, item: &StackItem) -> Result<(), String> {
    let index = capture(item.inline_effect.as_ref().ok_or("Storm root absent")?)
        .ok_or("Storm root substituted")?;
    let source = item
        .v4
        .ability_source_contract
        .ok_or("Storm captured observer absent")?;
    if item.kind!=crate::state::StackItemKind::TriggeredAbility||item.source!=source.source||item.controller!=source.controller
        ||!cast_at(state,index)?.1.contains(&source)||item.inline_effect.as_ref()!=Some(&program(state,index)?)
        ||!item.targets.is_empty()||!item.v4.target_contracts.is_empty()||item.v4.target_spec.is_some()
        ||item.is_copy||item.is_flashback||item.madness_offer||item.kicked||item.mode_chosen!=0
        ||!item.discarded.is_empty()||!item.v4.paid_cost_refs.is_empty()||item.v4.optional_additional_cost_paid.is_some()
        ||item.v4.source_contract.is_some()||item.v4.cast_method.is_some()||item.v4.activated_ability_index.is_some()
        ||item.v4.granted_by.is_some()||item.v4.hidden_ability_source.is_some()||item.v4.cauldron_grant.0.is_some()||item.v4.x_value!=0
        ||state.engine.event_history.iter().filter(|e|matches!(e,event::CommittedEvent::ThousandYearStormV1{journal:JournalV1::Collected{cast}}if *cast==index)).count()!=1 {
        return Err("Storm original source/controller/program metadata changed".into())
    }
    Ok(())
}
pub(crate) fn publish(state: &mut GameState, item: &StackItem) -> Result<(), String> {
    if capture(
        item.inline_effect
            .as_ref()
            .unwrap_or(&EffectOp::Sequence(vec![])),
    )
    .is_none()
    {
        return Ok(());
    }
    validate_item(state, item)?;
    let cast = capture(item.inline_effect.as_ref().unwrap()).unwrap();
    if state.engine.event_history.iter().any(|e|matches!(e,event::CommittedEvent::ThousandYearStormV1{journal:JournalV1::Published{cast:c,item:r}}if *c==cast&&r.v4.ability_source_contract==item.v4.ability_source_contract)){return Err("Storm trigger published twice".into())}
    append(
        state,
        JournalV1::Published {
            cast,
            item: Box::new(item.clone()),
        },
        false,
    );
    Ok(())
}
fn published_id(state: &GameState, id: StackItemId) -> bool {
    state.engine.event_history.iter().any(|e|matches!(e,event::CommittedEvent::ThousandYearStormV1{journal:JournalV1::Published{item,..}}if item.v4.stack_item_id==id))
}
pub(crate) fn selected(state: &GameState, item: &StackItem) -> bool {
    item.inline_effect.as_ref().is_some_and(recipe) || published_id(state, item.v4.stack_item_id)
}
pub(crate) fn pending_selected(state: &GameState, p: &EffectContinuation) -> bool {
    selected(state, &p.resolving_item)
        || p.ctx
            .stack_item_id
            .is_some_and(|id| published_id(state, id))
}
pub(crate) fn waiting(state: &GameState, ctx: &ExecCtx) -> bool {
    state.stack.iter().any(|r| {
        selected(state, r)
            && (r.source == ctx.source
                || ctx.ability_source_contract.is_some()
                    && ctx.ability_source_contract == r.v4.ability_source_contract)
    })
}
pub(crate) fn validate_published(state: &GameState, item: &StackItem) -> Result<(), String> {
    validate_item(state, item)?;
    let found = state
        .engine
        .event_history
        .iter()
        .filter_map(|e| match e {
            event::CommittedEvent::ThousandYearStormV1 {
                journal: JournalV1::Published { item: r, .. },
            } if r.v4.stack_item_id == item.v4.stack_item_id => Some(r.as_ref()),
            _ => None,
        })
        .collect::<Vec<_>>();
    if found != [item] {
        return Err("Storm full original published row changed".into());
    }
    Ok(())
}
fn copies(state: &GameState, item: &StackItem) -> Result<u16, String> {
    let mut n = 0u16;
    for e in &state.engine.event_history {
        if let event::CommittedEvent::ThousandYearStormV1 {
            journal:
                JournalV1::Copy {
                    trigger,
                    ordinal,
                    item: copy,
                },
        } = e
        {
            if *trigger != item.v4.stack_item_id {
                continue;
            }
            n = n.checked_add(1).ok_or("Storm copy ordinal overflow")?;
            let original = cast_at(
                state,
                capture(item.inline_effect.as_ref().ok_or("Storm root")?).ok_or("Storm root")?,
            )?
            .0;
            let contract = copy.v4.source_contract.ok_or("Storm copied source lost")?;
            let origin = contract
                .spell_copy_origin
                .ok_or("Storm copy lost native origin")?;
            if *ordinal != n
                || !copy.is_copy
                || copy.controller != item.controller
                || origin.parent != original.source
                || origin.parent_stack_zone_change_count
                    != original
                        .v4
                        .source_contract
                        .ok_or("Storm original source")?
                        .zone_change_count
                || copy.v4.stack_item_id == item.v4.stack_item_id
            {
                return Err("Storm original copy sequence changed".into());
            }
            let mut expected = original.clone();
            expected.source = copy.source;
            expected.controller = item.controller;
            expected.targets = copy.targets.clone();
            expected.is_copy = true;
            expected.is_flashback = false;
            expected.v4.stack_item_id = copy.v4.stack_item_id;
            expected.v4.cast_method = Some(crate::source_type_hexproof_v1::copy_method(
                original.v4.cast_method.unwrap_or_default(),
            ));
            expected.v4.source_contract = copy.v4.source_contract;
            expected.v4.target_contracts = copy.v4.target_contracts.clone();
            expected.v4.optional_additional_cost_paid = None;
            if expected != **copy {
                return Err(
                    "Storm copy changed copied modes, X or other original stack choices".into(),
                );
            }
            crate::engine::validate_copy_snapshot(state, copy)?;
        }
    }
    Ok(n)
}
pub(crate) fn note_copy(
    state: &mut GameState,
    p: &EffectContinuation,
    path: &[u16],
    before: usize,
) -> Result<(), String> {
    if !selected(state, &p.resolving_item) {
        return Ok(());
    }
    validate_published(state, &p.resolving_item)?;
    let ordinal = copies(state, &p.resolving_item)?
        .checked_add(1)
        .ok_or("Storm ordinal overflow")?;
    if path != [ordinal] || state.stack.len() != before + 1 {
        return Err("Storm child copy is not the next original slot".into());
    }
    let item = state
        .stack
        .last()
        .ok_or("Storm created copy absent")?
        .clone();
    append(
        state,
        JournalV1::Copy {
            trigger: p.resolving_item.v4.stack_item_id,
            ordinal,
            item: Box::new(item),
        },
        false,
    );
    Ok(())
}
pub(crate) fn validate_continuation(
    state: &GameState,
    p: &EffectContinuation,
) -> Result<(), String> {
    if !pending_selected(state, p) {
        return Ok(());
    }
    validate_published(state, &p.resolving_item)?;
    let index = capture(p.resolving_item.inline_effect.as_ref().unwrap()).unwrap();
    let n = count(state, index)?;
    let done = copies(state, &p.resolving_item)?;
    if done > n {
        return Err("Storm produced too many copies".into());
    }
    let ordinal = done.checked_add(1);
    let choice = match p.choice.as_ref() {
        Some(PendingEffectChoice::SelectTargets {
            purpose: EffectTargetSelectionPurpose::CopyTarget { choice },
            ..
        }) => Some(choice),
        _ => None,
    };
    let answered = match p.frames.last() {
        Some(EffectFrame::CopyTarget { choice, .. }) => Some(choice),
        _ => None,
    };
    let choice = choice
        .or(answered)
        .ok_or("Storm lost its mandatory remaining copy checkpoint")?;
    if ordinal != choice.path.first().copied()
        || choice.path.len() != 1
        || choice.spell.as_ref() != cast_at(state, index)?.0
    {
        return Err("Storm copy checkpoint changed".into());
    }
    let root = program(state, index)?;
    let EffectOp::Sequence(ops) = root else {
        unreachable!()
    };
    let expected = (usize::from(done) + 2..ops.len())
        .rev()
        .map(|i| EffectFrame::Program {
            op: ops[i].clone(),
            path: vec![i as u16],
        })
        .collect::<Vec<_>>();
    if choice.remaining != expected
        || p.frames
            != if answered.is_some() {
                let mut x = expected.clone();
                x.push(p.frames.last().unwrap().clone());
                x
            } else {
                expected
            }
    {
        return Err("Storm remaining copy sequence changed".into());
    }
    Ok(())
}
pub(crate) fn refuse_legacy(state: &GameState) -> Result<(), String> {
    if state.objects.iter().any(|(_,o)|matches!(o.zone,Zone::Battlefield|Zone::Stack)&&o.v4.face_down_v1.is_none()&&definition(o.card_def))
        ||state.engine.event_history.iter().any(|e|matches!(e,event::CommittedEvent::ThousandYearStormV1{journal:JournalV1::Cast{observers,..}}if !observers.is_empty())) {
        return Err("Thousand-Year Storm requires its distinct complete native consumer".into())
    }
    Ok(())
}

pub(crate) fn exact_cast_tail(state: &GameState, index: usize, spell: &StackItem) -> bool {
    index.checked_add(2) == Some(state.engine.event_history.len())
        && matches!(state.engine.event_history.last(),Some(event::CommittedEvent::ThousandYearStormV1{journal:JournalV1::Cast{event,spell:captured,..}})if *event==index as u64 && captured.as_ref()==spell)
}

/// Structural validation of the new occurrence roster, within the existing raw-world contract.
pub(crate) fn validate_state(state: &GameState) -> Result<(), String> {
    for (index, e) in state.engine.event_history.iter().enumerate() {
        let event::CommittedEvent::ThousandYearStormV1 { journal } = e else {
            continue;
        };
        match journal {
            JournalV1::Cast { .. } => {
                cast_at(state, index as u64)?;
            }
            JournalV1::Collected { cast } => {
                cast_at(state, *cast)?;
                if *cast>=index as u64||state.engine.event_history.iter().filter(|e|matches!(e,event::CommittedEvent::ThousandYearStormV1{journal:JournalV1::Collected{cast:c}}if c==cast)).count()!=1{return Err("Storm collection order or uniqueness changed".into())}
            }
            JournalV1::Published { cast, item } => {
                if *cast >= index as u64
                    || capture(
                        item.inline_effect
                            .as_ref()
                            .ok_or("Storm publication root absent")?,
                    ) != Some(*cast)
                {
                    return Err("Storm published occurrence changed".into());
                }
                validate_published(state, item)?;
            }
            JournalV1::Copy { trigger, .. } => {
                let rows = state.engine.event_history[..index]
                    .iter()
                    .filter_map(|e| match e {
                        event::CommittedEvent::ThousandYearStormV1 {
                            journal: JournalV1::Published { item, .. },
                        } if item.v4.stack_item_id == *trigger => Some(item.as_ref()),
                        _ => None,
                    })
                    .collect::<Vec<_>>();
                let [row] = rows.as_slice() else {
                    return Err("Storm copy lost original published trigger".into());
                };
                let n = count(state, capture(row.inline_effect.as_ref().unwrap()).unwrap())?;
                if copies(state, row)? > n {
                    return Err("Storm created too many copies".into());
                }
            }
        }
    }
    for e in &state.engine.event_log {
        if let event::CommittedEvent::ThousandYearStormV1 { journal } = e {
            if !matches!(journal, JournalV1::Cast { .. })
                || state
                    .engine
                    .event_history
                    .iter()
                    .filter(|r| *r == e)
                    .count()
                    != 1
            {
                return Err("Storm private journal leaked or collector receipt changed".into());
            }
        }
    }
    Ok(())
}

/// Only an authentic still-published copy root can retain this original pair's
/// departure LKI after the physical producing spell has already left the stack.
pub(crate) fn departure_contracts(
    state: &GameState,
    object: ObjectId,
) -> Vec<StackTargetContractV4> {
    let mut found = vec![];
    for item in &state.stack {
        if !selected(state, item) || validate_published(state, item).is_err() {
            continue;
        }
        let Some(index) = item.inline_effect.as_ref().and_then(capture) else {
            continue;
        };
        let Ok((spell, _)) = cast_at(state, index) else {
            continue;
        };
        if spell.v4.target_spec != Some(card_def::TargetSpec::TwoCreaturesWithDifferentControllers)
        {
            continue;
        }
        for (&target, &contract) in spell.targets.iter().zip(&spell.v4.target_contracts) {
            if target == Target::Object(object)
                && crate::engine::target_contract_matches_live(state, target, contract)
                && !found.contains(&contract)
            {
                found.push(contract)
            }
        }
    }
    found
}
