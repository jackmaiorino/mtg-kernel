//! Closed Loot printing, effective land allowance and bound top-six program.
//! No predecessor public projection or caller-authored operation supplies authority.
use super::*;
pub(crate) fn definition(id: u16) -> bool {
    cfg!(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    )) && id == 473
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
            d.name == "Loot, Exuberant Explorer"
                && d.capability == card_def::CardCapability::Partial
                && d.cost
                    == crate::mana::Cost {
                        pips: &[crate::mana::Pip::Colored(crate::mana::ManaColor::G)],
                        generic: 2,
                        x_count: 0,
                    }
                && d.generic_cost_reduction.is_none()
                && d.ward_cost.is_none()
                && !d.spell_cannot_be_countered
                && !d.records_mana_spent
                && d.equipment.is_none()
                && d.types == [CardType::Creature]
                && d.subtypes == [card_def::Subtype::Beast, card_def::Subtype::Noble]
                && d.supertypes == [card_def::Supertype::Legendary]
                && d.power == Some(1)
                && d.toughness == Some(4)
                && d.starting_loyalty.is_none()
                && d.enters_with_plus_one_counters.is_none()
                && d.controlled_counter_keyword.is_none()
                && d.game_outcome_protection_v1.is_none()
                && d.controlled_creature_grant.is_none()
                && !d.is_land
                && d.produces_mana.is_empty()
                && d.colors == [crate::mana::ManaColor::G]
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
                && d.activated_abilities.len() == 1
                && ability_is_printed(&d.activated_abilities[0])
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
                && d.mana_value == 3
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

pub(crate) fn program() -> EffectOp {
    EffectOp::LookTopSelectV1 {
        player: PlayerRef::Controller,
        count: LibraryLookCount::Fixed(6),
        rule: LibraryPickRule {
            pick: 1,
            choose_rest_order: false,
            selection: Some(LibraryPickSelectionV1 {
                without_mana_cost: crate::engine::FreeCastV1(false),
                filter: LibraryPickFilterV1::CreatureManaValueAtMostControlledLands,
                optional: true,
                destination: Zone::Battlefield,
                reveal_selected: true,
            }),
        },
        pick_x: false,
    }
}
pub(crate) fn is_recipe(op: &EffectOp) -> bool {
    *op == program()
}
fn ability_is_printed(a: &card_def::ActivatedAbilityDef) -> bool {
    a.cost
        == [
            card_def::CostComponent::Mana(crate::mana::Cost {
                pips: &[
                    crate::mana::Pip::Colored(crate::mana::ManaColor::G),
                    crate::mana::Pip::Colored(crate::mana::ManaColor::G),
                ],
                generic: 4,
                x_count: 0,
            }),
            card_def::CostComponent::Tap,
        ]
        && a.target_spec == card_def::TargetSpec::None
        && (a.effect)() == program()
        && a.activation_zone == Zone::Battlefield
        && !a.sorcery_speed_only
        && a.activation_target_filter == card_def::ActivationTargetFilter::TargetSpecOnly
        && a.max_activations_per_turn.is_none()
        && a.face.is_none()
}
// This is a total per-turn allowance, not a consumed per-source permission.
// Losing or gaining a source never refunds the player's physical play count.
pub(crate) fn validate_private_program(
    state: &GameState,
    p: &EffectContinuation,
) -> Result<(), String> {
    let item = &p.resolving_item;
    let started = item
        .v4
        .squad_rallier_started_v1
        .0
        .ok_or("Loot lost native paid-library start")?;
    let mut expected = crystal_prevention_v1::context(item);
    expected.niv_damage_origin_v1 = event::niv_damage_origin_for_saved_item_v1(state, item);
    if item.kind != crate::state::StackItemKind::ActivatedAbility
        || item.v4.activated_ability_index != Some(0)
        || item.v4.granted_by.is_some()
        || item.inline_effect != Some(program())
        || p.ctx != expected
        || Some(started.source) != item.v4.ability_source_contract
        || started.stack_item_id != item.v4.stack_item_id
        || !definition(started.source.card_def)
    {
        return Err("Loot changed original paid ability/context".into());
    }
    match state
        .engine
        .event_history
        .get(started.first_receipt_history_index as usize)
    {
        Some(event::CommittedEvent::SquadRallierResolutionBoundaryV1 { receipt })
            if receipt.ordinal == 0
                && receipt.source == started.source
                && receipt.stack_item_id == started.stack_item_id
                && receipt.pending.as_ref().is_some_and(|initial| {
                    initial.resolving_item == *item
                        && initial.ctx == p.ctx
                        && initial.frames
                            == [EffectFrame::Program {
                                op: program(),
                                path: vec![],
                            }]
                }) =>
        {
            Ok(())
        }
        _ => Err("Loot lost its actual initial native receipt".into()),
    }
}
pub(crate) fn land_allowance(state: &GameState, player: PlayerId) -> usize {
    1 + state.players[player.index()]
        .battlefield
        .iter()
        .filter(|&&id| {
            let o = state.objects.get(id);
            o.zone == Zone::Battlefield
                && o.controller == player
                && o.v4.face_index == 0
                && definition(o.card_def)
                && crate::continuous_characteristics_v1::printed_abilities_active(state, id)
        })
        .count()
}
pub(crate) fn controlled_lands(state: &GameState, player: PlayerId) -> usize {
    state.players[player.index()]
        .battlefield
        .iter()
        .filter(|&&id| crate::engine::object_has_type(state, id, CardType::Land))
        .count()
}
fn source_row(item: &StackItem) -> bool {
    item.kind == crate::state::StackItemKind::ActivatedAbility
        && item.v4.activated_ability_index == Some(0)
        && item.v4.granted_by.is_none()
        && item
            .v4
            .ability_source_contract
            .is_some_and(|s| definition(s.card_def))
}
pub(crate) fn waiting(state: &GameState, ctx: &ExecCtx) -> bool {
    state.stack.iter().any(|r| {
        source_row(r) && (r.source == ctx.source || Some(r.v4.stack_item_id) == ctx.stack_item_id)
    })
}
pub(crate) fn item_selected(state: &GameState, item: &StackItem) -> bool {
    source_row(item)
        || state.stack.iter().any(|r| {
            source_row(r)
                && (r.source == item.source || r.v4.stack_item_id == item.v4.stack_item_id)
        })
}
pub(crate) fn required(state: &GameState, p: &EffectContinuation) -> bool {
    source_row(&p.resolving_item)
        || p.resolving_item
            .inline_effect
            .as_ref()
            .is_some_and(is_recipe)
        || state.stack.iter().any(|r| {
            source_row(r)
                && (r.v4.stack_item_id == p.resolving_item.v4.stack_item_id
                    || Some(r.v4.stack_item_id) == p.ctx.stack_item_id)
        })
}
pub(crate) fn pending_selected(state: &GameState, p: &EffectContinuation) -> bool {
    item_selected(state, &p.resolving_item) || waiting(state, &p.ctx)
}
pub(crate) fn refuse_legacy(state: &GameState) -> Result<(), String> {
    if state.objects.iter().any(|(_, o)| {
        matches!(o.zone, Zone::Battlefield | Zone::Stack)
            && o.v4.face_down_v1.is_none()
            && definition(o.card_def)
    }) || state.engine.event_history.iter().any(|e| {
        matches!(e,
            event::CommittedEvent::SquadRallierResolutionBoundaryV1 { receipt }
                if definition(receipt.source.card_def))
    }) {
        return Err(
            "Loot land allowance/private library mechanics require a distinct complete consumer"
                .into(),
        );
    }
    Ok(())
}
#[cfg(all(
    test,
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
#[path = "loot_v1_tests.rs"]
mod tests;
