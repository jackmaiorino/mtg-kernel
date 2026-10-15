//! Genuine Midnight Snack source. Meaning and public consumers remain separate.
use crate::{
    card_def::{
        ActivationTargetFilter, CardCapability, CardType, CostComponent, Keywords, TargetSpec,
        CARD_DEFS,
    },
    effect::{EffectOp, ExecCtx, ObjectRef, PlayerRef},
    event::{self, CommittedEvent, ProposedEvent},
    ids::PlayerId,
    mana::{Cost, ManaColor, Pip},
    state::{GameState, StackItem, StackItemKind, Zone},
};

pub fn activation() -> EffectOp {
    EffectOp::LoseLifeEqualControllerTurnGainsV1
}
pub(crate) fn raid() -> EffectOp {
    EffectOp::Conditional {
        cond: crate::effect::EffectCond::ControllerAttackedThisTurn,
        then: Box::new(EffectOp::CreateToken {
            token_def: crate::card_def::card_id_by_name("Food Token")
                .expect("existing genuine Food"),
            controller: PlayerRef::Controller,
        }),
        else_: Box::new(EffectOp::Sequence(Vec::new())),
    }
}
fn activation_is_exact(d: &crate::card_def::CardDef) -> bool {
    let [a] = d.activated_abilities else {
        return false;
    };
    a.cost
        == [
            CostComponent::Mana(Cost {
                generic: 2,
                pips: &[Pip::Colored(ManaColor::B)],
                x_count: 0,
            }),
            CostComponent::SacrificeSelf,
        ]
        && a.activation_zone == Zone::Battlefield
        && !a.sorcery_speed_only
        && a.target_spec == TargetSpec::TargetOpponent
        && a.activation_target_filter == ActivationTargetFilter::TargetSpecOnly
        && a.max_activations_per_turn.is_none()
        && a.face.is_none()
        && (a.effect)() == activation()
}
pub(crate) fn definition(id: u16) -> bool {
    cfg!(all(feature="limited-fdn-fixtures", not(feature="standard-magezero-fixtures")))
        && CARD_DEFS.get(usize::from(id)).is_some_and(exact_definition)
        && crate::trigger::triggers_for(id).len() == 1
        && crate::trigger::triggers_for(id).iter().all(|a|
            a.condition == crate::trigger::TriggerCondition::BeginningControllerEndStepAfterControllerAttackedThisTurn
                && a.home_zone == Zone::Battlefield && a.face_index == 0
                && !a.intervening_if_kicked && !a.intervening_if_controls_another_source_card
                && (a.effect)() == raid())
}

pub(crate) fn exact_definition(d: &crate::card_def::CardDef) -> bool {
    d.name == "Midnight Snack"
        && matches!(d.capability, CardCapability::Partial | CardCapability::Full)
        && d.types == [CardType::Enchantment]
        && d.subtypes.is_empty()
        && d.supertypes.is_empty()
        && d.cost.generic == 2
        && d.cost.x_count == 0
        && d.cost.pips == [Pip::Colored(ManaColor::B)]
        && d.colors == [ManaColor::B]
        && d.mana_value == 3
        && d.power.is_none()
        && d.toughness.is_none()
        && !d.is_land
        && !d.is_token
        && d.keywords == Keywords::NONE
        && d.target_spec == TargetSpec::None
        && d.alt_cost.is_none()
        && d.kicker_cost.is_none()
        && d.additional_cost.is_none()
        && d.flashback.is_none()
        && d.ward_cost.is_none()
        && d.starting_loyalty.is_none()
        && d.object_name == "Midnight Snack"
        && !d.spell_cannot_be_countered
        && !d.records_mana_spent
        && d.generic_cost_reduction.is_none()
        && d.equipment.is_none()
        && d.enters_with_plus_one_counters.is_none()
        && d.controlled_counter_keyword.is_none()
        && d.game_outcome_protection_v1.is_none()
        && d.controlled_creature_grant.is_none()
        && (d.mana_ability)().is_none()
        && d.produces_mana.is_empty()
        && d.plot_cost.is_none()
        && d.madness_cost.is_none()
        && d.escape.is_none()
        && d.mode2.is_none()
        && d.mode3.is_none()
        && d.additional_modes.is_empty()
        && d.kicked_target_spec.is_none()
        && d.mana_ability_choices.is_empty()
        && !d.enters_battlefield_tapped
        && d.enters_battlefield_tapped_unless.is_none()
        && d.mana_ability_def.is_none()
        && d.minimum_blockers == 1
        && d.omen.is_none()
        && d.bestow.is_none()
        && d.adventure.is_none()
        && !d.mana_ability_includes_chosen_color
        && d.as_enters_choose_color_other_than.is_none()
        && d.additional_mana_abilities.is_empty()
        && d.additional_mana_ability_conditions.is_empty()
        && d.attachment.is_none()
        && d.transform_face.is_none()
        && d.saga.is_none()
        && d.optional_additional_cost.is_none()
        && !d.changeling
        && !d.delve
        && !d.cant_be_blocked_by_monarchs_creatures
        && d.conditional_tap_yield.is_none()
        && d.enters_tapped_unless_controller.is_none()
        && d.restricted_mana_abilities.is_empty()
        && d.animation.is_none()
        && d.activated_ability_generic_reductions.is_empty()
        && d.combat_static_rules == crate::card_def::CombatStaticRulesDef::NONE
        && d.as_enters_color_choice_v1.is_none()
        && d.chosen_color_creature_boost_v1.is_none()
        && d.damage_replacement_v1.is_none()
        && d.starting_life_self_bonuses_v1.is_empty()
        && d.source_type_hexproof_v1.is_none()
        && d.other_controlled_nontoken_death_v1.is_none()
        && activation_is_exact(d)
        && d.token_counter_replacement_v48.is_none()
        && (d.spell_effect)()
            == Some(EffectOp::MoveObject {
                object: ObjectRef::ThisSource,
                to_zone: Zone::Battlefield,
            })
}

/// Read the existing independently validated current-turn boundary. No running
/// total, first-gain ordinal, public projection, or new journal is authority.
pub(crate) fn gained_this_turn(state: &GameState, player: PlayerId) -> Result<i32, String> {
    crate::life_loss_turn_v1::validate(state)?;
    let boundary = state
        .engine
        .event_history
        .iter()
        .rposition(|e| matches!(e, CommittedEvent::LifeLossTurnBeganV1 { .. }))
        .ok_or("Midnight Snack lacks actual current-turn boundary")?;
    if state.engine.event_history[boundary]
        != (CommittedEvent::LifeLossTurnBeganV1 {
            turn: state.turn_key_v1(),
            active_player: state.active_player,
        })
    {
        return Err("Midnight Snack turn boundary changed".into());
    }
    state.engine.event_history[boundary + 1..]
        .iter()
        .try_fold(0_i32, |total, e| match e {
            CommittedEvent::LifeGain {
                player: gained,
                amount,
            } if *gained == player && *amount > 0 => total
                .checked_add(*amount)
                .ok_or_else(|| "Midnight Snack gain sum overflow".into()),
            _ => Ok(total),
        })
}

/// A short-lived removal loan from the exact finalized original stack row.
/// Generic activation validation owns payment/source/targets and restore.
pub(crate) struct ResolutionOwner {
    item: StackItem,
}
pub(crate) fn acquire(
    state: &GameState,
    item: &StackItem,
) -> Result<Option<ResolutionOwner>, String> {
    if item.inline_effect != Some(activation()) {
        return Ok(None);
    }
    if state.stack.last() != Some(item)
        || item.kind != StackItemKind::ActivatedAbility
        || crate::engine::validated_stack_item_target_spec(item, state)?
            != Some(TargetSpec::TargetOpponent)
        || item.v4.activated_ability_index != Some(0)
        || !item
            .v4
            .ability_source_contract
            .is_some_and(|a| definition(a.card_def) && a.controller == item.controller)
    {
        return Err("Midnight Snack activation lost original definition/source/row".into());
    }
    gained_this_turn(state, item.controller)?;
    Ok(Some(ResolutionOwner { item: item.clone() }))
}
pub(crate) fn resolve(
    state: &mut GameState,
    owner: ResolutionOwner,
    ctx: &ExecCtx,
    legal: bool,
) -> Result<(), String> {
    let item = owner.item;
    if ctx.source != item.source
        || ctx.controller != item.controller
        || ctx.stack_item_id != Some(item.v4.stack_item_id)
        || ctx.targets != item.targets
        || ctx.ability_source_contract != item.v4.ability_source_contract
        || state
            .stack
            .iter()
            .any(|row| row.v4.stack_item_id == item.v4.stack_item_id)
    {
        return Err("Midnight Snack removal/context changed".into());
    }
    if !legal {
        return Ok(());
    }
    let [crate::state::Target::Player(target)] = ctx.targets.as_slice() else {
        return Err("Midnight Snack lost original opponent target".into());
    };
    if *target != ctx.controller.opponent() {
        return Err("Midnight Snack target changed".into());
    }
    let total = gained_this_turn(state, ctx.controller)?;
    state.players[target.index()]
        .life
        .checked_sub(total)
        .ok_or("Midnight Snack life-total overflow")?;
    event::propose_and_commit(state, ProposedEvent::life_loss(*target, total));
    Ok(())
}

#[cfg(test)]
#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
#[path = "midnight_snack_v1_tests.rs"]
mod tests;
