//! Definition-owned private Curator look/partition/opponent-pile program.
//! No predecessor public projection or caller-authored operation supplies authority.
use super::*;
use crate::{
    state::StackItemKind,
    trigger::{TriggerCondition, TriggeredAbilityDef},
};
pub(crate) fn definition(id: u16) -> bool {
    cfg!(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    )) && id == 472
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
            d.name == "Curator of Destinies"
                && d.capability == card_def::CardCapability::Partial
                && d.cost
                    == crate::mana::Cost {
                        pips: &[
                            crate::mana::Pip::Colored(crate::mana::ManaColor::U),
                            crate::mana::Pip::Colored(crate::mana::ManaColor::U),
                        ],
                        generic: 4,
                        x_count: 0,
                    }
                && d.generic_cost_reduction.is_none()
                && d.ward_cost.is_none()
                && d.spell_cannot_be_countered
                && !d.records_mana_spent
                && d.equipment.is_none()
                && d.types == [CardType::Creature]
                && d.subtypes == [card_def::Subtype::Sphinx]
                && d.supertypes.is_empty()
                && d.power == Some(5)
                && d.toughness == Some(5)
                && d.starting_loyalty.is_none()
                && d.enters_with_plus_one_counters.is_none()
                && d.controlled_counter_keyword.is_none()
                && d.game_outcome_protection_v1.is_none()
                && d.controlled_creature_grant.is_none()
                && !d.is_land
                && d.produces_mana.is_empty()
                && d.colors == [crate::mana::ManaColor::U]
                && d.target_spec == card_def::TargetSpec::None
                && d.keywords == Keywords::FLYING
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

pub(crate) fn template() -> EffectOp {
    EffectOp::BindCuratorDestiniesV1
}
pub(crate) const TRIGGERS: [TriggeredAbilityDef; 1] = [TriggeredAbilityDef {
    condition: TriggerCondition::Etb,
    home_zone: Zone::Battlefield,
    intervening_if_kicked: false,
    intervening_if_controls_another_source_card: false,
    face_index: 0,
    effect: template,
}];
pub(crate) fn record_entry(state: &mut GameState, entry: usize) {
    let object = match state.engine.event_history.get(entry) {
        Some(event::CommittedEvent::ZoneChange {
            object,
            to: Zone::Battlefield,
            from,
            ..
        }) if *from != Zone::Battlefield => *object,
        Some(event::CommittedEvent::CreateToken { object, .. }) => *object,
        _ => return,
    };
    let live = state.objects.get(object);
    if live.zone != Zone::Battlefield
        || live.v4.face_index != 0
        || !definition(live.card_def)
        || !crate::continuous_characteristics_v1::printed_abilities_active(state, object)
    {
        return;
    }
    let source = AbilitySourceContractV4::capture(state, object);
    let receipt = event::CommittedEvent::CuratorEnteredV1 {
        entry_history_index: entry as u64,
        source,
    };
    state.engine.event_log.push(receipt.clone());
    state.engine.event_history.push(receipt);
}
fn entry_source(state: &GameState, entry: u64) -> Result<AbilitySourceContractV4, String> {
    let rows = state
        .engine
        .event_history
        .iter()
        .enumerate()
        .filter_map(|(i, e)| match e {
            event::CommittedEvent::CuratorEnteredV1 {
                entry_history_index,
                source,
            } if *entry_history_index == entry => Some((i, *source)),
            _ => None,
        })
        .collect::<Vec<_>>();
    let [(position, source)] = rows.as_slice() else {
        return Err("Curator entry receipt erased or duplicated".into());
    };
    let mut expected = entry.checked_add(1).ok_or("Curator entry index overflow")?;
    if let Some(event::CommittedEvent::CopiedTokenAbilitiesEnteredV1 {
        source: copied,
        creation_history_index,
        instances,
    }) = state.engine.event_history.get(expected as usize)
    {
        if copied != source || *creation_history_index != entry || *instances == 0 {
            return Err("Curator copied entry receipt changed".into());
        }
        expected += 1;
    }
    if *position as u64 != expected
        || source.zone != Zone::Battlefield
        || !definition(source.card_def)
    {
        return Err("Curator entry/source position changed".into());
    }
    crate::engine::validate_historical_ability_source_contract(state, *source)?;
    match state.engine.event_history.get(entry as usize) {
        Some(event::CommittedEvent::ZoneChange {
            object,
            to: Zone::Battlefield,
            from,
            ..
        }) if *object == source.source
            && *from != Zone::Battlefield
            && source.zone_change_count > 0 => {}
        Some(event::CommittedEvent::CreateToken {
            object,
            token_def,
            controller,
            ..
        }) if *object == source.source
            && *token_def == source.card_def
            && *controller == source.controller
            && source.zone_change_count == 0 => {}
        _ => return Err("Curator receipt lost actual entry incarnation".into()),
    }
    Ok(*source)
}
pub(crate) fn collect(
    state: &mut GameState,
    events: &[event::CommittedEvent],
) -> Result<Vec<crate::trigger::PendingTrigger>, String> {
    let mut found = Vec::new();
    let mut consumed = Vec::new();
    for event in events {
        let event::CommittedEvent::CuratorEnteredV1 {
            entry_history_index,
            ..
        } = event
        else {
            continue;
        };
        if consumed.contains(entry_history_index) || state.engine.event_history.iter().any(|e|matches!(e,event::CommittedEvent::CuratorCollectedV1{entry_history_index:at} if at==entry_history_index)) {return Err("Curator entry collected twice".into())}
        let source = entry_source(state, *entry_history_index)?;
        if !state.engine.event_history.iter().any(|e| e == event) {
            return Err("Curator event/history disagree".into());
        }
        found.push(crate::trigger::PendingTrigger {
            controller: source.controller,
            source: source.source,
            effect: EffectOp::CuratorDestiniesV1 {
                entry_history_index: *entry_history_index,
            },
            is_madness_offer: false,
            kicked: false,
            target_spec: card_def::TargetSpec::None,
            targets: Vec::new(),
            target_contracts: Vec::new(),
            placement_ordered: false,
            source_contract: Some(source),
            granted_by: None,
            optional_additional_cost_paid: None,
            paid_cost_refs: Vec::new(),
        });
        consumed.push(*entry_history_index);
    }
    for entry_history_index in consumed {
        state
            .engine
            .event_history
            .push(event::CommittedEvent::CuratorCollectedV1 {
                entry_history_index,
            });
    }
    Ok(found)
}
pub(crate) fn selected(state: &GameState, item: &StackItem) -> bool {
    matches!(item.inline_effect,Some(EffectOp::CuratorDestiniesV1{..}|EffectOp::BindCuratorDestiniesV1))
        || state.engine.event_history.iter().any(|e|matches!(e,event::CommittedEvent::CuratorPublishedV1{item:original,..} if original.v4.stack_item_id==item.v4.stack_item_id))
}
pub(crate) fn waiting(state: &GameState, ctx: &ExecCtx) -> bool {
    state.stack.iter().any(|row| {
        selected(state, row)
            && (row.source == ctx.source || Some(row.v4.stack_item_id) == ctx.stack_item_id)
    })
}
pub(crate) fn waiting_item(state: &GameState, item: &StackItem) -> bool {
    state.stack.iter().any(|row| {
        selected(state, row)
            && (row.source == item.source || row.v4.stack_item_id == item.v4.stack_item_id)
    })
}
pub(crate) fn pending_selected(state: &GameState, p: &EffectContinuation) -> bool {
    selected(state, &p.resolving_item)
        || waiting(state, &p.ctx)
        || state.stack.iter().any(|row| {
            selected(state, row)
                && (row.source == p.resolving_item.source
                    || row.v4.stack_item_id == p.resolving_item.v4.stack_item_id)
        })
}
fn validate_origin(
    state: &GameState,
    item: &StackItem,
    require_published: bool,
) -> Result<u64, String> {
    let Some(EffectOp::CuratorDestiniesV1 {
        entry_history_index,
    }) = item.inline_effect
    else {
        return Err("Curator lost bound native root".into());
    };
    let source = entry_source(state, entry_history_index)?;
    if item.kind!=StackItemKind::TriggeredAbility || item.source!=source.source || item.controller!=source.controller || item.v4.ability_source_contract!=Some(source)
        || item.v4.source_contract.is_some() || item.v4.granted_by.is_some() || item.v4.activated_ability_index.is_some()
        || item.v4.hidden_ability_source.is_some() || item.v4.madness_source_contract.is_some() || item.v4.cauldron_grant.0.is_some()
        || item.v4.cast_method.is_some() || item.v4.target_spec.is_some() || !item.v4.target_contracts.is_empty() || item.v4.face_index!=0 || item.v4.x_value!=0
        || !item.targets.is_empty() || !item.discarded.is_empty() || !item.v4.paid_cost_refs.is_empty() || item.v4.optional_additional_cost_paid.is_some()
        || item.is_copy || item.is_flashback || item.kicked || item.madness_offer || item.mode_chosen!=0
        || state.engine.event_history.iter().filter(|e|matches!(e,event::CommittedEvent::CuratorCollectedV1{entry_history_index:at} if *at==entry_history_index)).count()!=1
        || state.engine.event_history.iter().any(|e|matches!(e,event::CommittedEvent::CuratorFinishedV1{entry_history_index:at} if *at==entry_history_index))
    {return Err("Curator source/controller/program occurrence changed or consumed".into())}
    let published = state
        .engine
        .event_history
        .iter()
        .filter_map(|e| match e {
            event::CommittedEvent::CuratorPublishedV1 {
                entry_history_index: at,
                item: row,
            } if *at == entry_history_index || row.v4.stack_item_id == item.v4.stack_item_id => {
                Some(row.as_ref())
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    if item.v4.stack_item_id != StackItemId::default()
        && (require_published || !published.is_empty())
        && published.as_slice() != [item]
    {
        return Err("Curator original published row changed".into());
    }
    Ok(entry_history_index)
}
pub(crate) fn validate_trigger(state: &GameState, item: &StackItem) -> Result<u64, String> {
    validate_origin(state, item, true)
}
pub(crate) fn publish(state: &mut GameState, item: &StackItem) -> Result<(), String> {
    if !matches!(
        item.inline_effect,
        Some(EffectOp::CuratorDestiniesV1 { .. })
    ) {
        return Ok(());
    }
    if state.stack.last() != Some(item)
        || item.v4.stack_item_id == StackItemId::default()
        || state
            .stack
            .iter()
            .filter(|r| r.v4.stack_item_id == item.v4.stack_item_id)
            .count()
            != 1
    {
        return Err("Curator publication lost its unique actual stack row".into());
    }
    let entry_history_index = validate_origin(state, item, false)?;
    if state.engine.event_history.iter().any(|e|matches!(e,event::CommittedEvent::CuratorPublishedV1{entry_history_index:at,item:row} if *at==entry_history_index || row.v4.stack_item_id==item.v4.stack_item_id)){return Err("Curator native row published twice".into())}
    state
        .engine
        .event_history
        .push(event::CommittedEvent::CuratorPublishedV1 {
            entry_history_index,
            item: Box::new(item.clone()),
        });
    Ok(())
}
pub(crate) fn finish(state: &mut GameState, item: &StackItem) -> Result<(), String> {
    let entry_history_index = validate_trigger(state, item)?;
    state
        .engine
        .event_history
        .push(event::CommittedEvent::CuratorFinishedV1 {
            entry_history_index,
        });
    Ok(())
}
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChoiceV1 {
    player: PlayerId,
    entry: u64,
    original_len: u32,
    prefix: Vec<EffectObjectBinding>,
    phase: u8,
    down: Vec<EffectObjectBinding>,
    choose_down: Option<bool>,
}
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AnswerV1 {
    Cards(Vec<EffectObjectBinding>),
    Pile(bool),
}
fn subset(
    original: &[EffectObjectBinding],
    selected: &[EffectObjectBinding],
) -> Result<(), String> {
    if selected.iter().any(|b| !original.contains(b))
        || selected
            .iter()
            .map(|b| b.object)
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            != selected.len()
    {
        return Err("Curator changed selected card/incarnation or duplicated it".into());
    }
    Ok(())
}
fn validate_choice_data(
    state: &GameState,
    p: &EffectContinuation,
    c: &ChoiceV1,
) -> Result<(), String> {
    if c.player != p.ctx.controller
        || p.resolving_item.inline_effect
            != Some(EffectOp::CuratorDestiniesV1 {
                entry_history_index: c.entry,
            })
        || c.phase > 2
        || usize::try_from(c.original_len).ok()
            != Some(state.players[c.player.index()].library.len())
        || c.prefix != bind_library_top(state, c.player, 5)
    {
        return Err("Curator changed original library/controller/stage".into());
    }
    for b in &c.prefix {
        validate_effect_object_binding(state, *b)?;
        if state.objects.get(b.object).owner != c.player {
            return Err("Curator library owner changed".into());
        }
    }
    subset(&c.prefix, &c.down)?;
    if c.phase == 0 {
        if !c.down.is_empty() || c.choose_down.is_some() {
            return Err("Curator future pile answered early".into());
        }
    } else {
        if c.down
            != c.prefix
                .iter()
                .copied()
                .filter(|b| c.down.contains(b))
                .collect::<Vec<_>>()
        {
            return Err("Curator private pile changed original relative order".into());
        }
        for b in c.prefix.iter().filter(|b| !c.down.contains(b)) {
            if !state
                .known_library_cards(c.player.opponent(), c.player)
                .iter()
                .any(|k| {
                    k.object == b.object && k.zone_change_count == b.expected_zone_change_count
                })
            {
                return Err("Curator face-up pile was not revealed".into());
            }
        }
        if (c.phase == 1) != c.choose_down.is_none() {
            return Err("Curator pile choice stage changed".into());
        }
    }
    Ok(())
}
fn validate_choice(state: &GameState, p: &EffectContinuation, c: &ChoiceV1) -> Result<(), String> {
    validate_choice_data(state, p, c)?;
    let rows = state
        .engine
        .event_history
        .iter()
        .filter_map(|e| match e {
            event::CommittedEvent::CuratorProgressV1 {
                entry_history_index,
                choice,
            } if *entry_history_index == c.entry => Some(choice.as_ref()),
            _ => None,
        })
        .collect::<Vec<_>>();
    if rows.len() != usize::from(c.phase) + 1
        || rows.last().copied() != Some(c)
        || rows.iter().enumerate().any(|(i, old)| {
            usize::from(old.phase) != i
                || old.player != c.player
                || old.prefix != c.prefix
                || old.original_len != c.original_len
                || old.phase == 0 && (!old.down.is_empty() || old.choose_down.is_some())
                || old.phase == 1 && (old.choose_down.is_some() || old.down != c.down)
                || old.phase == 2 && old.choose_down.is_none()
        })
    {
        return Err("Curator lost its issued native checkpoint".into());
    }
    Ok(())
}
fn graveyard(c: &ChoiceV1) -> Vec<EffectObjectBinding> {
    c.prefix
        .iter()
        .copied()
        .filter(|b| c.down.contains(b) != c.choose_down.unwrap())
        .collect()
}
fn candidates(c: &ChoiceV1) -> Vec<EffectObjectBinding> {
    if c.phase == 0 {
        c.prefix.clone()
    } else {
        graveyard(c)
    }
}
fn validate_answer(c: &ChoiceV1, a: &AnswerV1) -> Result<(), String> {
    match (c.phase, a) {
        (0, AnswerV1::Cards(cards)) => subset(&c.prefix, cards),
        (1, AnswerV1::Pile(_)) => Ok(()),
        (2, AnswerV1::Cards(cards)) => {
            let original = graveyard(c);
            subset(&original, cards)?;
            if original.len() != cards.len() {
                return Err("Curator graveyard permutation incomplete".into());
            }
            Ok(())
        }
        _ => Err("Curator answer belongs to another stage".into()),
    }
}
fn candidate(b: EffectObjectBinding) -> EffectTargetCandidate {
    EffectTargetCandidate {
        target: Target::Object(b.object),
        expected_object: Some(b),
    }
}
pub(crate) fn answer(p: &mut EffectContinuation, c: ChoiceV1, a: AnswerV1) -> Result<(), String> {
    validate_answer(&c, &a)?;
    let frame = EffectFrame::CuratorV1 {
        choice: c,
        answer: a,
    };
    p.answered_choice_guard = Some(EffectAnsweredChoiceGuard::StandardSelection {
        frame: Box::new(frame.clone()),
    });
    p.frames.push(frame);
    Ok(())
}
fn pile_choice(state: &GameState, entry: u64, down: u8, up: u8) -> Result<ChoiceV1, String> {
    let c = state
        .engine
        .event_history
        .iter()
        .rev()
        .find_map(|e| match e {
            event::CommittedEvent::CuratorProgressV1 {
                entry_history_index,
                choice,
            } if *entry_history_index == entry => Some(choice.as_ref()),
            _ => None,
        })
        .ok_or("Curator private issued pile absent")?;
    if c.phase != 1
        || usize::from(down) != c.down.len()
        || usize::from(up) != c.prefix.len() - c.down.len()
    {
        return Err("Curator public pile sizes/stage changed".into());
    }
    Ok(c.clone())
}
pub(crate) fn pile_answer(
    state: &GameState,
    p: &mut EffectContinuation,
    down: u8,
    up: u8,
    value: bool,
) -> Result<(), String> {
    let Some(EffectOp::CuratorDestiniesV1 {
        entry_history_index: entry,
    }) = p.resolving_item.inline_effect
    else {
        return Err("Curator pile lost original private entry root".into());
    };
    let c = pile_choice(state, entry, down, up)?;
    validate_choice(state, p, &c)?;
    answer(p, c, AnswerV1::Pile(value))
}
fn stage(state: &mut GameState, p: &mut EffectContinuation, c: ChoiceV1) -> Result<bool, String> {
    validate_choice_data(state, p, &c)?;
    let previous = state
        .engine
        .event_history
        .iter()
        .filter_map(|e| match e {
            event::CommittedEvent::CuratorProgressV1 {
                entry_history_index,
                choice,
            } if *entry_history_index == c.entry => Some(choice.as_ref()),
            _ => None,
        })
        .collect::<Vec<_>>();
    if previous.len() != usize::from(c.phase)
        || previous.last().is_some_and(|old| {
            old.phase + 1 != c.phase
                || old.prefix != c.prefix
                || old.player != c.player
                || old.original_len != c.original_len
                || c.phase == 2 && (old.down != c.down || old.choose_down.is_some())
        })
    {
        return Err("Curator native stage skipped or replayed".into());
    }
    state
        .engine
        .event_history
        .push(event::CommittedEvent::CuratorProgressV1 {
            entry_history_index: c.entry,
            choice: Box::new(c.clone()),
        });
    if c.phase == 1 {
        p.choice = Some(PendingEffectChoice::ChooseBoolean {
            player: c.player.opponent(),
            path: Vec::new(),
            default: Some(false),
            purpose: EffectBooleanChoicePurpose::CuratorPileV1 {
                face_down_count: c.down.len() as u8,
                face_up_count: (c.prefix.len() - c.down.len()) as u8,
            },
        });
        return Ok(true);
    }
    let rows = candidates(&c);
    let count = u16::try_from(rows.len()).map_err(|_| "Curator choice width")?;
    if c.phase == 2
        && (rows.len() < 2 || !crate::engine::graveyard_order_choice_exposed(state, c.player))
    {
        return apply(state, p, c, AnswerV1::Cards(rows));
    }
    p.choice = Some(PendingEffectChoice::SelectTargets {
        player: c.player,
        path: Vec::new(),
        selected: Vec::new(),
        legal: rows.into_iter().map(candidate).collect(),
        min_targets: if c.phase == 0 { 0 } else { count },
        max_targets: count,
        ordered: c.phase == 2,
        purpose: EffectTargetSelectionPurpose::CuratorCardsV1 { choice: c },
    });
    Ok(true)
}
pub(crate) fn start(state: &mut GameState, p: &mut EffectContinuation) -> Result<bool, String> {
    let entry = validate_trigger(state, &p.resolving_item)?;
    let player = p.ctx.controller;
    let prefix = bind_library_top(state, player, 5);
    let original_len = u32::try_from(state.players[player.index()].library.len())
        .map_err(|_| "Curator library width")?;
    state.reveal_library_top(player, player, prefix.len());
    stage(
        state,
        p,
        ChoiceV1 {
            player,
            entry,
            original_len,
            prefix,
            phase: 0,
            down: Vec::new(),
            choose_down: None,
        },
    )
}
pub(crate) fn apply(
    state: &mut GameState,
    p: &mut EffectContinuation,
    mut c: ChoiceV1,
    a: AnswerV1,
) -> Result<bool, String> {
    validate_choice(state, p, &c)?;
    validate_answer(&c, &a)?;
    match a {
        AnswerV1::Cards(cards) if c.phase == 0 => {
            c.down = c
                .prefix
                .iter()
                .copied()
                .filter(|b| cards.contains(b))
                .collect();
            c.phase = 1;
            for (position, b) in c.prefix.iter().enumerate() {
                if !c.down.contains(b) {
                    state.reveal_library_position(c.player.opponent(), c.player, position);
                }
            }
            stage(state, p, c)
        }
        AnswerV1::Pile(value) => {
            c.choose_down = Some(value);
            c.phase = 2;
            stage(state, p, c)
        }
        AnswerV1::Cards(order) => {
            let hand = c
                .prefix
                .iter()
                .copied()
                .filter(|b| c.down.contains(b) == c.choose_down.unwrap())
                .collect::<Vec<_>>();
            // Resolve the chosen hand pile first, then its owner's ordered
            // graveyard pile. The order decision precedes either native move.
            let mut final_state = state.clone();
            commit_zone_change_batch(&mut final_state, &hand, Zone::Hand, true)?;
            commit_zone_change_batch(&mut final_state, &order, Zone::Graveyard, true)?;
            if final_state.engine.halted.is_some() {
                return Err("Curator native pile move halted".into());
            }
            finish(&mut final_state, &p.resolving_item)?;
            *state = final_state;
            Ok(false)
        }
    }
}
pub(crate) fn validate(state: &GameState, p: &EffectContinuation) -> Result<(), String> {
    validate_trigger(state, &p.resolving_item)?;
    let mut expected = crystal_prevention_v1::context(&p.resolving_item);
    expected.niv_damage_origin_v1 =
        event::niv_damage_origin_for_saved_item_v1(state, &p.resolving_item);
    if p.ctx != expected {
        return Err("Curator complete captured context changed".into());
    }
    match (&p.choice, &p.answered_choice_guard, p.frames.as_slice()) {
        (
            None,
            None,
            [EffectFrame::Program {
                op:
                    EffectOp::CuratorDestiniesV1 {
                        entry_history_index,
                    },
                path,
            }],
        ) if path.is_empty()
            && !state.engine.event_history.iter().any(|e|matches!(e,event::CommittedEvent::CuratorProgressV1{entry_history_index:at,..} if at==entry_history_index))
            && p.resolving_item.inline_effect
                == Some(EffectOp::CuratorDestiniesV1 {
                    entry_history_index: *entry_history_index,
                }) =>
        {
            Ok(())
        }
        (
            Some(PendingEffectChoice::SelectTargets {
                player,
                path,
                selected,
                legal,
                min_targets,
                max_targets,
                ordered,
                purpose: EffectTargetSelectionPurpose::CuratorCardsV1 { choice: c },
            }),
            None,
            [],
        ) => {
            validate_choice(state, p, c)?;
            if c.phase != 0 && c.phase != 2 {
                return Err("Curator cards stage changed".into());
            }
            let rows = candidates(c);
            let selected_rows = selected
                .iter()
                .map(|r| r.expected_object.ok_or("Curator selected binding missing"))
                .collect::<Result<Vec<_>, _>>()?;
            subset(&rows, &selected_rows)?;
            let expected = rows
                .iter()
                .copied()
                .filter(|b| !selected_rows.contains(b))
                .map(candidate)
                .collect::<Vec<_>>();
            if *player != c.player
                || !path.is_empty()
                || usize::from(*min_targets) != if c.phase == 0 { 0 } else { rows.len() }
                || usize::from(*max_targets) != rows.len()
                || *ordered != (c.phase == 2)
                || *legal != expected
                || selected.iter().any(|r| {
                    r.expected_object
                        .is_none_or(|b| r.target != Target::Object(b.object))
                })
            {
                return Err("Curator exact private candidates/player/order changed".into());
            }
            Ok(())
        }
        (Some(PendingEffectChoice::ChooseBoolean {player,path,default:Some(false),purpose:EffectBooleanChoicePurpose::CuratorPileV1{face_down_count,face_up_count}}),None,[])=>{
            let Some(EffectOp::CuratorDestiniesV1 { entry_history_index }) = p.resolving_item.inline_effect else {
                return Err("Curator pile lost original private entry root".into());
            };
            let c=pile_choice(state,entry_history_index,*face_down_count,*face_up_count)?;validate_choice(state,p,&c)?;
            if *player!=c.player.opponent() || !path.is_empty(){return Err("Curator opponent pile chooser changed".into())}Ok(())
        },
        (
            None,
            Some(EffectAnsweredChoiceGuard::StandardSelection { frame }),
            [actual @ EffectFrame::CuratorV1 { choice, answer }],
        ) if frame.as_ref() == actual => {
            validate_choice(state, p, choice)?;
            validate_answer(choice, answer)
        }
        _ => Err("Curator continuation stage erased, substituted or malformed".into()),
    }
}
pub(crate) fn refuse_legacy(state: &GameState) -> Result<(), String> {
    if state.objects.iter().any(|(_, o)| {
        matches!(o.zone, Zone::Battlefield | Zone::Stack)
            && o.v4.face_down_v1.is_none()
            && definition(o.card_def)
    }) || state
        .engine
        .event_history
        .iter()
        .any(|e| matches!(e, event::CommittedEvent::CuratorEnteredV1 { .. }))
    {
        return Err("Curator private piles require a distinct complete consumer".into());
    }
    Ok(())
}

#[cfg(all(
    test,
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
#[path = "curator_v1_tests.rs"]
mod tests;
