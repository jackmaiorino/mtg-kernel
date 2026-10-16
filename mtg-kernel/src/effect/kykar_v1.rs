//! Definition-owned Kykar modes and one-shot next-end-step exile return.
//! Receipts are native mutable-world history, not a public execution capability.
use super::*;

pub(crate) fn definition(id: u16) -> bool {
    cfg!(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    )) && id == 474
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
            d.name == "Kykar, Zephyr Awakener"
                && d.capability == card_def::CardCapability::Partial
                && d.cost
                    == crate::mana::Cost {
                        pips: &[
                            crate::mana::Pip::Colored(crate::mana::ManaColor::W),
                            crate::mana::Pip::Colored(crate::mana::ManaColor::U),
                        ],
                        generic: 2,
                        x_count: 0,
                    }
                && d.generic_cost_reduction.is_none()
                && d.ward_cost.is_none()
                && !d.spell_cannot_be_countered
                && !d.records_mana_spent
                && d.equipment.is_none()
                && d.types == [CardType::Creature]
                && d.subtypes == [card_def::Subtype::Bird, card_def::Subtype::Wizard]
                && d.supertypes == [card_def::Supertype::Legendary]
                && d.power == Some(3)
                && d.toughness == Some(4)
                && d.starting_loyalty.is_none()
                && d.enters_with_plus_one_counters.is_none()
                && d.controlled_counter_keyword.is_none()
                && d.game_outcome_protection_v1.is_none()
                && d.controlled_creature_grant.is_none()
                && !d.is_land
                && d.produces_mana.is_empty()
                && d.colors == [crate::mana::ManaColor::W, crate::mana::ManaColor::U]
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
                && d.mana_value == 4
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

pub(crate) fn token_definition(id: u16) -> bool {
    cfg!(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    )) && id == 475
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
            d.name == "Kykar Spirit Token"
                && d.capability == card_def::CardCapability::Partial
                && d.cost
                    == crate::mana::Cost {
                        pips: &[],
                        generic: 0,
                        x_count: 0,
                    }
                && d.generic_cost_reduction.is_none()
                && d.ward_cost.is_none()
                && !d.spell_cannot_be_countered
                && !d.records_mana_spent
                && d.equipment.is_none()
                && d.types == [CardType::Creature]
                && d.subtypes == [card_def::Subtype::Spirit]
                && d.supertypes.is_empty()
                && d.power == Some(1)
                && d.toughness == Some(1)
                && d.starting_loyalty.is_none()
                && d.enters_with_plus_one_counters.is_none()
                && d.controlled_counter_keyword.is_none()
                && d.game_outcome_protection_v1.is_none()
                && d.controlled_creature_grant.is_none()
                && !d.is_land
                && d.produces_mana.is_empty()
                && d.colors == [crate::mana::ManaColor::W]
                && d.target_spec == card_def::TargetSpec::None
                && d.keywords == Keywords::FLYING
                && (d.spell_effect)().is_none()
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
                && d.is_token
                && d.escape.is_none()
                && d.mana_ability_choices.is_empty()
                && !d.enters_battlefield_tapped
                && d.mana_ability_def.is_none()
                && d.minimum_blockers == 1
                && d.omen.is_none()
                && d.mana_value == 0
                && !d.mana_ability_includes_chosen_color
                && d.as_enters_choose_color_other_than.is_none()
                && d.additional_mana_abilities.is_empty()
                && d.object_name == "Spirit Token"
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

pub(crate) fn marker() -> EffectOp {
    EffectOp::KykarChooseModeV1
}
pub(crate) fn modes() -> Vec<(card_def::TargetSpec, EffectOp)> {
    vec![
        (
            card_def::TargetSpec::AnotherControlledCreature,
            EffectOp::KykarExileV1,
        ),
        (card_def::TargetSpec::None, EffectOp::KykarSpiritV1),
    ]
}
pub(crate) fn recipe(op: &EffectOp) -> bool {
    matches!(
        op,
        EffectOp::KykarChooseModeV1
            | EffectOp::KykarExileV1
            | EffectOp::KykarSpiritV1
            | EffectOp::KykarReturnV1 { .. }
    )
}
fn family(item: &StackItem) -> bool {
    item.kind == crate::state::StackItemKind::TriggeredAbility
        && item.v4.granted_by.is_none()
        && item
            .v4
            .ability_source_contract
            .is_some_and(|s| definition(s.card_def))
}
fn authenticated_ward(state: &GameState, item: &StackItem) -> bool {
    let mut rows = state
        .stack
        .iter()
        .filter(|r| r.v4.stack_item_id == item.v4.stack_item_id);
    family(item)
        && rows.next().is_none_or(|r| r == item)
        && rows.next().is_none()
        && matches!(
            item.inline_effect,
            Some(EffectOp::CounterUnlessPaysGeneric { .. })
        )
        && validated_ward_observation_targeter(state, item).is_ok()
}
fn row(state: &GameState, item: &StackItem) -> bool {
    family(item) && !authenticated_ward(state, item)
}
pub(crate) fn item_selected(state: &GameState, item: &StackItem) -> bool {
    if authenticated_ward(state, item) {
        return false;
    }
    family(item)
        || item.inline_effect.as_ref().is_some_and(recipe)
        || state.stack.iter().any(|r| {
            row(state, r)
                && (r.v4.stack_item_id == item.v4.stack_item_id || r.source == item.source)
        })
}
fn authentic_ward_pending(state: &GameState, p: &EffectContinuation) -> bool {
    if !authenticated_ward(state, &p.resolving_item) {
        return false;
    }
    let mut ctx = crystal_prevention_v1::context(&p.resolving_item);
    ctx.niv_damage_origin_v1 = event::niv_damage_origin_for_saved_item_v1(state, &p.resolving_item);
    if ctx != p.ctx {
        return false;
    }
    match (&p.choice, p.frames.as_slice(), &p.answered_choice_guard) {
        (
            Some(PendingEffectChoice::ChooseBoolean {
                player,
                path,
                default: Some(false),
                purpose:
                    EffectBooleanChoicePurpose::CounterUnlessPaysGeneric {
                        ward_target,
                        targeting_stack_item,
                        player: payer,
                        generic,
                    },
            }),
            [],
            None,
        ) if player == payer && path.is_empty() => {
            validate_ward_resolving_binding(p, *ward_target, *targeting_stack_item, *generic)
                .is_ok()
                && validate_counter_unless_pays_generic(
                    state,
                    *ward_target,
                    p.ctx.controller,
                    *targeting_stack_item,
                    *payer,
                    *generic,
                    false,
                    true,
                )
                .is_ok()
        }
        (
            None,
            [frame @ EffectFrame::ResolveCounterUnlessPaysGeneric { .. }],
            Some(EffectAnsweredChoiceGuard::CounterUnlessPaysGeneric { frame: saved }),
        ) if **saved == *frame => validate_counter_unless_pays_frame(state, p, frame).is_ok(),
        _ => false,
    }
}
pub(crate) fn pending_selected(state: &GameState, p: &EffectContinuation) -> bool {
    !authentic_ward_pending(state, p)
        && (item_selected(state, &p.resolving_item) || waiting(state, &p.ctx))
}
pub(crate) fn waiting(state: &GameState, ctx: &ExecCtx) -> bool {
    state
        .stack
        .iter()
        .any(|r| row(state, r) && r.source == ctx.source)
}

pub(crate) fn validate_schedule(
    state: &GameState,
    index: u64,
) -> Result<
    (
        crate::state::AbilitySourceContractV4,
        crate::state::ObjectLinkV4,
        PlayerId,
    ),
    String,
> {
    let i = usize::try_from(index).map_err(|_| "Kykar schedule index width")?;
    let Some(event::CommittedEvent::KykarExiledV1 {
        source,
        exiled,
        owner,
        departure_history_index,
    }) = state.engine.event_history.get(i)
    else {
        return Err("Kykar return lost its original exile receipt".into());
    };
    let departure =
        usize::try_from(*departure_history_index).map_err(|_| "Kykar departure index width")?;
    if !definition(source.card_def)
        || source.zone != Zone::Battlefield
        || departure >= i
        || !matches!(state.engine.event_history.get(departure), Some(event::CommittedEvent::ZoneChange {object, from:Zone::Battlefield,to:Zone::Exile,controller_before}) if *object==exiled.object && *controller_before==source.controller)
        || !state
            .objects
            .try_get(exiled.object)
            .is_some_and(|o| o.owner == *owner)
    {
        return Err("Kykar exile receipt lost source, owner or physical departure".into());
    }
    if state.engine.event_history.iter().filter(|e| matches!(e,event::CommittedEvent::KykarExiledV1{departure_history_index:at,..}if at==departure_history_index)).count()!=1
        || state.engine.event_history[departure+1..i].iter().any(|e| matches!(e,event::CommittedEvent::BeginningEndStep{..}) || matches!(e,event::CommittedEvent::ZoneChange{object,..}|event::CommittedEvent::Draw{object:Some(object),..}if *object==exiled.object)) {
        return Err("Kykar original exile receipt duplicated or reordered".into())
    }
    // Count only actual later moves of this physical object. A newer incarnation
    // cannot retroactively authenticate a different captured exile generation.
    let later = state.engine.event_history[departure + 1..]
        .iter()
        .filter(
            |e| matches!(e,event::CommittedEvent::ZoneChange{object,..} if *object==exiled.object) || matches!(e,event::CommittedEvent::Draw{object:Some(object),..}if *object==exiled.object),
        )
        .count();
    let live = state.objects.get(exiled.object);
    if u32::try_from(later)
        .ok()
        .and_then(|n| live.zone_change_count.checked_sub(n))
        != Some(exiled.zone_change_count)
    {
        return Err("Kykar exile incarnation changed".into());
    }
    Ok((*source, *exiled, *owner))
}
pub(crate) fn validate_root(
    state: &GameState,
    op: &EffectOp,
    source: Option<crate::state::AbilitySourceContractV4>,
) -> Result<(), String> {
    if !recipe(op) {
        return Ok(());
    }
    let source = source.ok_or("Kykar root lost original source contract")?;
    if !definition(source.card_def) || source.zone != Zone::Battlefield {
        return Err("Kykar source contract changed".into());
    }
    if let EffectOp::KykarReturnV1 {
        schedule_history_index,
    } = op
    {
        let (original, _, _) = validate_schedule(state, *schedule_history_index)?;
        let collected: Vec<_> = state
            .engine
            .event_history
            .iter()
            .filter_map(|e| match e {
                event::CommittedEvent::KykarDelayedCollectedV1 {
                    schedule_history_index: i,
                    end_step_history_index,
                } if i == schedule_history_index => Some(*end_step_history_index),
                _ => None,
            })
            .collect();
        let first_end = state
            .engine
            .event_history
            .iter()
            .enumerate()
            .skip(usize::try_from(*schedule_history_index).map_err(|_| "Kykar schedule width")? + 1)
            .find_map(|(i, e)| {
                matches!(e, event::CommittedEvent::BeginningEndStep { .. }).then_some(i as u64)
            });
        if original != source
            || first_end != collected.first().copied()
            || collected.len() != 1
            || collected[0] <= *schedule_history_index
            || !matches!(
                usize::try_from(collected[0])
                    .ok()
                    .and_then(|i| state.engine.event_history.get(i)),
                Some(event::CommittedEvent::BeginningEndStep { .. })
            )
        {
            return Err("Kykar return lost its one-shot end-step capture".into());
        }
    }
    Ok(())
}
pub(crate) fn collect(
    state: &mut GameState,
    events: &[event::CommittedEvent],
) -> Result<Vec<crate::trigger::PendingTrigger>, String> {
    if !events
        .iter()
        .any(|e| matches!(e, event::CommittedEvent::BeginningEndStep { .. }))
    {
        return Ok(Vec::new());
    }
    let mut due = Vec::new();
    for (i, e) in state.engine.event_history.iter().enumerate() {
        if !matches!(e, event::CommittedEvent::KykarExiledV1 { .. }) {
            continue;
        }
        let index = u64::try_from(i).map_err(|_| "Kykar history width")?;
        validate_schedule(state, index)?;
        let consumed=state.engine.event_history.iter().filter(|e|matches!(e,event::CommittedEvent::KykarDelayedCollectedV1{schedule_history_index,..}if *schedule_history_index==index)).count();
        if consumed > 1 {
            return Err("Kykar delayed trigger duplicated".into());
        }
        if consumed == 1 {
            let (source, _, _) = validate_schedule(state, index)?;
            validate_root(
                state,
                &EffectOp::KykarReturnV1 {
                    schedule_history_index: index,
                },
                Some(source),
            )?;
            continue;
        }
        if let Some((end, _)) = state
            .engine
            .event_history
            .iter()
            .enumerate()
            .skip(i + 1)
            .find(|(_, e)| matches!(e, event::CommittedEvent::BeginningEndStep { .. }))
        {
            // Native step entry immediately collects its begin event. A missing
            // one-shot marker cannot be repaired by a later equal-valued step:
            // BeginningEndStep values do not carry an occurrence identifier.
            if state.engine.event_history[end + 1..]
                .iter()
                .any(|e| matches!(e, event::CommittedEvent::BeginningEndStep { .. }))
            {
                return Err("Kykar unconsumed first end step is no longer current".into());
            }
            // This retains the native event-input value check; by itself it is
            // not proof of an event's history position.
            if !events.contains(&state.engine.event_history[end]) {
                return Err("Kykar unconsumed end-step occurrence lost actual event input".into());
            }
            due.push((
                index,
                u64::try_from(end).map_err(|_| "Kykar end-step width")?,
            ));
        }
    }
    let mut result = Vec::new();
    for (index, end) in due {
        let (source, _, _) = validate_schedule(state, index)?;
        state
            .engine
            .event_history
            .push(event::CommittedEvent::KykarDelayedCollectedV1 {
                schedule_history_index: index,
                end_step_history_index: end,
            });
        result.push(crate::trigger::PendingTrigger {
            controller: source.controller,
            source: source.source,
            effect: EffectOp::KykarReturnV1 {
                schedule_history_index: index,
            },
            is_madness_offer: false,
            kicked: false,
            target_spec: card_def::TargetSpec::None,
            targets: vec![],
            target_contracts: vec![],
            placement_ordered: false,
            source_contract: Some(source),
            granted_by: None,
            optional_additional_cost_paid: None,
            paid_cost_refs: vec![],
        });
    }
    Ok(result)
}
pub(super) fn execute(
    op: &EffectOp,
    ctx: &ExecCtx,
    state: &mut GameState,
    proof: &NativeExecutionProofV1<'_>,
) -> Result<(), String> {
    validate_root(state, op, ctx.ability_source_contract)?;
    let source = ctx.ability_source_contract.ok_or("Kykar source lost")?;
    if source.source != ctx.source || source.controller != ctx.controller {
        return Err("Kykar captured controller changed".into());
    }
    match op {
        EffectOp::KykarSpiritV1 => {
            let token_def = card_def::card_id_by_name("Kykar Spirit Token")
                .ok_or("Kykar Spirit definition absent")?;
            if !token_definition(token_def) {
                return Err("Kykar Spirit printing changed".into());
            }
            let permit = TokenPermitV1 { proof, token_def };
            event::commit_kykar_spirit_owned_v1(state, permit)?;
        }
        EffectOp::KykarExileV1 => {
            if !ctx.target_incarnation_matches(0, state)
                || !crate::engine::effect_target_is_legal_from_ability_source(
                    state,
                    source,
                    ctx.controller,
                    card_def::TargetSpec::AnotherControlledCreature,
                    &ctx.targets,
                    0,
                )
            {
                return Ok(());
            }
            let Some(Target::Object(object)) = ctx.targets.first().copied() else {
                return Err("Kykar target lost".into());
            };
            let owner = state.objects.get(object).owner;
            let generation = state
                .objects
                .get(object)
                .zone_change_count
                .checked_add(1)
                .ok_or("Kykar exile generation overflow")?;
            let start = state.engine.event_history.len();
            event::propose_and_commit(
                state,
                event::ProposedEvent::zone_change(object, Zone::Exile),
            );
            let Some(departure)=state.engine.event_history.iter().enumerate().skip(start).find_map(|(i,e)| matches!(e,event::CommittedEvent::ZoneChange{object:o,from:Zone::Battlefield,to:Zone::Exile,controller_before}if *o==object && *controller_before==ctx.controller).then_some(i)) else {return Ok(())};
            if state.objects.get(object).zone != Zone::Exile
                || state.objects.get(object).zone_change_count != generation
            {
                return Ok(());
            }
            state
                .engine
                .event_history
                .push(event::CommittedEvent::KykarExiledV1 {
                    source,
                    exiled: crate::state::ObjectLinkV4 {
                        object,
                        zone_change_count: generation,
                    },
                    owner,
                    departure_history_index: u64::try_from(departure)
                        .map_err(|_| "Kykar departure width")?,
                });
        }
        EffectOp::KykarReturnV1 {
            schedule_history_index,
        } => {
            let (_, exiled, owner) = validate_schedule(state, *schedule_history_index)?;
            if state.objects.try_get(exiled.object).is_some_and(|o| {
                o.zone == Zone::Exile
                    && o.zone_change_count == exiled.zone_change_count
                    && !o.v4.is_token
            }) {
                event::propose_and_commit(
                    state,
                    event::ProposedEvent::zone_change_to_battlefield_under_controller(
                        exiled.object,
                        owner,
                    ),
                );
            }
        }
        _ => return Err("Kykar unselected mode cannot execute".into()),
    }
    Ok(())
}
pub(crate) fn refuse_legacy(state: &GameState) -> Result<(), String> {
    if crate::standard_cards_v1::has_battlefield_copy_origin_v1(state, definition) {
        return Err("Kykar copied origin requires a distinct complete consumer".into());
    }
    if state.objects.iter().any(|(_, o)| {
        matches!(o.zone, Zone::Battlefield | Zone::Stack)
            && o.v4.face_down_v1.is_none()
            && (definition(o.card_def) || token_definition(o.card_def))
    }) || state.stack.iter().any(|r| row(state, r))
        || state
            .engine
            .pending_triggers
            .iter()
            .any(|p| p.source_contract.is_some_and(|s| definition(s.card_def)))
        || state.engine.event_history.iter().any(|e| {
            matches!(
                e,
                event::CommittedEvent::KykarExiledV1 { .. }
                    | event::CommittedEvent::KykarDelayedCollectedV1 { .. }
            )
        })
    {
        return Err("Kykar modes/delayed exile require a distinct complete consumer".into());
    }
    Ok(())
}

#[cfg(all(
    test,
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
#[path = "kykar_v1_tests.rs"]
mod tests;

pub(crate) struct TokenPermitV1<'a, 'b> {
    proof: &'a NativeExecutionProofV1<'b>,
    token_def: u16,
}
impl TokenPermitV1<'_, '_> {
    pub(crate) fn events(&self) -> Vec<event::ProposedEvent> {
        vec![event::ProposedEvent::create_token(
            self.token_def,
            self.proof.ctx.controller,
        )]
    }
    pub(crate) fn allows(&self, state: &GameState, events: &[event::ProposedEvent]) -> bool {
        token_definition(self.token_def)
            && *self.proof.owner.root == EffectOp::KykarSpiritV1
            && self.proof.owner.ctx == *self.proof.ctx
            && events == self.events()
            && self
                .proof
                .owner
                .validate_entry(
                    state,
                    &self.proof.owner.item,
                    self.proof.ctx,
                    &EffectOp::KykarSpiritV1,
                )
                .is_ok()
    }
}
