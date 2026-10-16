//! FDN161: optional exact-source Hand-only base-cost alternative.
//! Source history remains native mutable-world structure, not caller authority.
use super::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OptionsV1 {
    pub hand: StackSourceContractV4,
    pub source: AbilitySourceContractV4,
    pub controller: PlayerId,
    pub paid: bool,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChoiceV1 {
    pub options_history_index: u64,
    /// false pays ordinary costs, true replaces only the base mana cost.
    pub free: Option<bool>,
}
/// Native quote only. Neither serializable nor an execution permission. Its
/// sole constructor checks the same actual Hand/source before a disposable
/// native announcement; no exile or resolution permission is manufactured.
pub(super) struct QuoteV1 {
    spell: ObjectId,
    controller: PlayerId,
    source: AbilitySourceContractV4,
}
pub(super) fn active_source(
    state: &GameState,
    player: PlayerId,
) -> Option<AbilitySourceContractV4> {
    state.players[player.index()]
        .battlefield
        .iter()
        .copied()
        .find_map(|id| {
            let o = state.objects.get(id);
            (o.zone == Zone::Battlefield
                && o.controller == player
                && definition(o.card_def)
                && o.v4.face_index == 0
                && o.v4.face_down_v1.is_none()
                && crate::continuous_characteristics_v1::printed_abilities_active(state, id))
            .then(|| AbilitySourceContractV4::capture(state, id))
        })
}
fn hand(state: &GameState, spell: ObjectId, player: PlayerId) -> bool {
    state.objects.try_get(spell).is_some_and(|o| {
        o.zone == Zone::Hand
            && o.owner == player
            && o.controller == player
            && state.players[player.index()].hand.contains(&spell)
            && !o.v4.is_token
            && o.spell_copy_origin.is_none()
            && card_def::CARD_DEFS
                .get(o.card_def as usize)
                .is_some_and(|d| {
                    d.is_executable() && d.is_castable() && !d.has_type(CardType::Land)
                })
    })
}
pub(super) fn offered_free(state: &GameState, spell: ObjectId, player: PlayerId) -> bool {
    if !hand(state, spell, player) {
        return false;
    }
    let Some(source) = active_source(state, player) else {
        return false;
    };
    let quote = QuoteV1 {
        spell,
        controller: player,
        source,
    };
    let mut scene = state.clone();
    // The exact native announcement, with a freshly checked static source,
    // supplies the real pending spell forms/targets/costs on this projection.
    materialize_begin_cast_with_omniscience_quote_v1(&mut scene, player, spell, quote);
    let Some(pending) = scene.engine.pending_cast.as_ref() else {
        return false;
    };
    let d = &card_def::CARD_DEFS[scene.objects.get(spell).card_def as usize];
    [false, true]
        .into_iter()
        .filter(|k| !*k || d.kicker_cost.is_some())
        .any(|kicked| {
            (0..printed_spell_form_count(d))
                .any(|form| resolution_cast_v1::form_completable(&scene, pending, form, kicked))
        })
}
fn append_options(
    state: &mut GameState,
    spell: ObjectId,
    controller: PlayerId,
    source: AbilitySourceContractV4,
    paid: bool,
) -> ChoiceV1 {
    let options = OptionsV1 {
        hand: StackSourceContractV4::capture(state, spell, CastMethodV4::Normal),
        source,
        controller,
        paid,
    };
    let index = state.engine.event_history.len() as u64;
    state
        .engine
        .event_history
        .push(CommittedEvent::OmniscienceCastOptionsV1 { options });
    ChoiceV1 {
        options_history_index: index,
        free: if paid { None } else { Some(true) },
    }
}
pub(super) fn seed_quote(state: &mut GameState, quote: QuoteV1) -> ChoiceV1 {
    assert!(
        hand(state, quote.spell, quote.controller)
            && active_source(state, quote.controller) == Some(quote.source),
        "native Hand quote changed source"
    );
    append_options(state, quote.spell, quote.controller, quote.source, false)
}
pub(super) fn seed(state: &mut GameState, spell: ObjectId, player: PlayerId) -> Option<ChoiceV1> {
    if !offered_free(state, spell, player) {
        return None;
    }
    let source = active_source(state, player)?;
    let paid = is_castable_now(player, spell, CastMethodV4::Normal, state);
    Some(append_options(state, spell, player, source, paid))
}
fn options(state: &GameState, index: u64) -> Result<&OptionsV1, String> {
    let Some(CommittedEvent::OmniscienceCastOptionsV1 { options: o }) = usize::try_from(index)
        .ok()
        .and_then(|i| state.engine.event_history.get(i))
    else {
        return Err("Omniscience original option record absent".into());
    };
    let Some(live) = state.objects.try_get(o.hand.source) else {
        return Err("Omniscience original physical spell absent".into());
    };
    if o.hand.zone != Zone::Hand
        || o.hand.owner != o.controller
        || o.hand.controller != o.controller
        || o.hand.cast_method != CastMethodV4::Normal
        || o.hand.spell_cast_origin.is_some()
        || o.hand.finalized_cast_binding.is_some()
        || o.source.zone != Zone::Battlefield
        || o.source.controller != o.controller
        || !definition(o.source.card_def)
        || live.owner != o.hand.owner
        || live.card_def != o.hand.card_def
        || live.zone_change_count < o.hand.zone_change_count
    {
        return Err("Omniscience original hand/source contract changed".into());
    }
    let d = &card_def::CARD_DEFS[o.hand.card_def as usize];
    if !d.is_executable() || !d.is_castable() || d.has_type(CardType::Land) {
        return Err("Omniscience original hand printing changed".into());
    }
    validate_historical_ability_source_contract(state, o.source)?;
    Ok(o)
}
pub(super) fn waiting(p: &PendingCast) -> bool {
    p.omniscience_v1.is_some_and(|c| c.free.is_none())
}
pub(super) fn free_choice(c: Option<ChoiceV1>) -> bool {
    c.is_some_and(|c| c.free == Some(true))
}
fn route(index: u64, player: PlayerId, generation: u32) -> SpellCastRouteV4 {
    SpellCastRouteV4::OmniscienceHandV1 {
        options_history_index: index,
        holder: player,
        hand_generation: generation,
    }
}
pub(super) fn origin(choice: ChoiceV1, player: PlayerId, generation: u32) -> SpellCastRouteV4 {
    if choice.free == Some(true) {
        route(choice.options_history_index, player, generation)
    } else {
        SpellCastRouteV4::Hand
    }
}
/// Pure source-bound cost query. Full pending/finalized validation remains at
/// its original native entrypoints and does not recursively invoke the planner.
pub(super) fn free_cast_for(state: &GameState, spell: ObjectId, player: PlayerId) -> bool {
    let Some(live) = state.objects.try_get(spell) else {
        return false;
    };
    let Some(origin) = live.v4.spell_cast_origin else {
        return false;
    };
    let SpellCastRouteV4::OmniscienceHandV1 {
        options_history_index,
        holder,
        hand_generation,
    } = origin.route
    else {
        return false;
    };
    holder == player
        && live.zone == Zone::Stack
        && live.owner == player
        && live.controller == player
        && origin.origin_zone == Zone::Hand
        && origin.origin_zone_change_count == hand_generation
        && hand_generation.checked_add(1) == Some(live.zone_change_count)
        && options(state, options_history_index).is_ok_and(|o| {
            o.hand.source == spell
                && o.controller == player
                && o.hand.zone_change_count == hand_generation
        })
}
pub(super) fn validate(state: &GameState, p: &PendingCast) -> Result<(), String> {
    let Some(c) = p.omniscience_v1 else {
        return if p.source_contract.spell_cast_origin.is_some_and(|o|
            matches!(o.route, SpellCastRouteV4::OmniscienceHandV1 { .. })
            || state.engine.event_history.iter().any(|e| matches!(e, CommittedEvent::OmniscienceCastOptionsV1 { options } if options.hand.source == p.spell && options.hand.zone_change_count == o.origin_zone_change_count))) {
            Err("Omniscience pending choice erased".into())
        } else { Ok(()) };
    };
    let o = options(state, c.options_history_index)?;
    let cast = p
        .source_contract
        .spell_cast_origin
        .ok_or("Omniscience pending origin erased")?;
    if o.hand.source != p.spell
        || o.hand.card_def != p.source_contract.card_def
        || o.controller != p.controller
        || p.origin_zone != Zone::Hand
        || p.is_flashback
        || p.source_contract.cast_method != CastMethodV4::Normal
        || o.hand.zone_change_count != cast.origin_zone_change_count
        || cast.origin_zone != Zone::Hand
        || cast.route != origin(c, p.controller, o.hand.zone_change_count)
        || (c.free == Some(false) || c.free.is_none()) && !o.paid
    {
        return Err("Omniscience pending original choice/source changed".into());
    }
    Ok(())
}
pub(super) fn answer(state: &mut GameState, p: &PendingCast, index: u16) -> Result<(), String> {
    validate(state, p)?;
    let c = p.omniscience_v1.ok_or("Omniscience choice absent")?;
    let o = options(state, c.options_history_index)?;
    if c.free.is_some() || !o.paid || index > 1 {
        return Err("Omniscience choice already selected/outside menu".into());
    }
    let selected = ChoiceV1 {
        free: Some(index == 1),
        ..c
    };
    let d = &card_def::CARD_DEFS[p.source_contract.card_def as usize];
    let room = crate::standard_cards_v1::room_cast_mana_value(d, CastMethodV4::Normal).is_some();
    let mut source = p.source_contract;
    source
        .spell_cast_origin
        .as_mut()
        .ok_or("Omniscience origin erased")?
        .route = origin(selected, p.controller, o.hand.zone_change_count);
    state.objects.get_mut(p.spell).v4.spell_cast_origin = source.spell_cast_origin;
    let row = state
        .stack
        .last_mut()
        .ok_or("Omniscience native placeholder absent")?;
    row.v4.source_contract = Some(source);
    let live = state
        .engine
        .pending_cast
        .as_mut()
        .ok_or("Omniscience pending absent")?;
    live.source_contract = source;
    live.omniscience_v1 = Some(selected);
    live.cast_mode = if index == 1 && !room || d.alt_cost.is_none() {
        Some(CastMode::Normal)
    } else {
        None
    };
    live.x_value = if index == 1 || d.cost.x_count == 0 {
        Some(0)
    } else {
        None
    };
    Ok(())
}
pub(super) fn record_finalized(
    state: &mut GameState,
    p: &PendingCast,
) -> Result<Option<u64>, String> {
    let Some(choice) = p.omniscience_v1 else {
        return Ok(None);
    };
    validate(state, p)?;
    if choice.free.is_none() {
        return Err("Omniscience paid before cost choice".into());
    }
    let stack_item_id = state
        .stack
        .last()
        .ok_or("Omniscience paid placeholder absent")?
        .v4
        .stack_item_id;
    let index = state.engine.event_history.len() as u64;
    state
        .engine
        .event_history
        .push(CommittedEvent::OmniscienceCastSelectedV1 {
            choice,
            source: p.source_contract,
            stack_item_id,
        });
    Ok(Some(index))
}
/// The native paid receipt also outlives a physical spell for its Storm
/// producer. No later live-controller/zone lookup replaces the original cast.
pub(super) fn paid_contract(
    state: &GameState,
    contract: StackSourceContractV4,
) -> Result<StackItemId, String> {
    let index = contract
        .finalized_cast_binding
        .and_then(|b| b.omniscience_cast_v1)
        .ok_or("Omniscience finalized receipt erased")?;
    let Some(CommittedEvent::OmniscienceCastSelectedV1 {
        choice,
        source: original,
        stack_item_id,
    }) = usize::try_from(index)
        .ok()
        .and_then(|i| state.engine.event_history.get(i))
    else {
        return Err("Omniscience finalized receipt absent".into());
    };
    if state.engine.event_history.iter().filter(|e| matches!(e, CommittedEvent::OmniscienceCastSelectedV1 { stack_item_id:id, .. } if id == stack_item_id)).count() != 1 {
        return Err("Omniscience finalized receipt duplicated".into());
    }
    let o = options(state, choice.options_history_index)?;
    let mut before = contract;
    before.finalized_cast_binding = None;
    before.cast_method = original.cast_method;
    before
        .spell_cast_origin
        .as_mut()
        .ok_or("Omniscience finalized origin absent")?
        .finalized_method = None;
    if *original != before
        || o.hand.source != contract.source
        || o.controller != contract.controller
        || o.hand.card_def != contract.card_def
        || choice.free.is_none()
        || !o.paid && choice.free != Some(true)
        || contract.spell_cast_origin.is_none_or(|r| {
            r.origin_zone != Zone::Hand
                || r.origin_zone_change_count != o.hand.zone_change_count
                || r.route != origin(*choice, contract.controller, o.hand.zone_change_count)
        })
        || choice.free == Some(true)
            && !matches!(
                contract.cast_method,
                CastMethodV4::Normal | CastMethodV4::Omen | CastMethodV4::Alternative
            )
    {
        return Err("Omniscience finalized original cost/source changed".into());
    }
    Ok(*stack_item_id)
}
pub(super) fn validate_finalized(
    state: &GameState,
    item: &StackItem,
    pending: bool,
) -> Result<(), String> {
    if pending || item.is_copy {
        return Ok(());
    }
    let Some(source) = item.v4.source_contract else {
        return Ok(());
    };
    let selected = state.engine.event_history.iter().any(|e| matches!(e,
        CommittedEvent::OmniscienceCastSelectedV1 {stack_item_id,..} if *stack_item_id == item.v4.stack_item_id));
    let bound = source
        .finalized_cast_binding
        .and_then(|b| b.omniscience_cast_v1);
    let free = source
        .spell_cast_origin
        .is_some_and(|o| matches!(o.route, SpellCastRouteV4::OmniscienceHandV1 { .. }));
    if !selected && !free && bound.is_none() {
        return Ok(());
    }
    if paid_contract(state, source)? != item.v4.stack_item_id
        || source.controller != item.controller
    {
        return Err("Omniscience finalized native stack row changed".into());
    }
    if free && !resolution_cast_v1::form_allowed(state, item.source, source.cast_method) {
        return Err("Omniscience free alternative spell form changed".into());
    }
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
                CommittedEvent::OmniscienceCastOptionsV1 { .. }
                    | CommittedEvent::OmniscienceCastSelectedV1 { .. }
            )
        })
    {
        Err("Omniscience requires a complete successor".into())
    } else {
        Ok(())
    }
}

pub(crate) fn definition(id: u16) -> bool {
    cfg!(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    )) && id == 478
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
            d.name == "Omniscience"
                && d.capability == card_def::CardCapability::Partial
                && d.cost
                    == crate::mana::Cost {
                        pips: &[
                            crate::mana::Pip::Colored(crate::mana::ManaColor::U),
                            crate::mana::Pip::Colored(crate::mana::ManaColor::U),
                            crate::mana::Pip::Colored(crate::mana::ManaColor::U),
                        ],
                        generic: 7,
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
                && d.colors == [crate::mana::ManaColor::U]
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
                && d.mana_value == 10
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
