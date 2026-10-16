//! Unregistered native source recipe. This file has no lib.rs module or
//! registry/trigger dispatch entry until the reviewed real Call459 prerequisite
//! is composed and Artist can append460. No consumer permission is conferred.
use crate::{
    card_def::{CardCapability, CardType, Keywords, Subtype, TargetSpec},
    effect::{EffectCond, EffectOp, ObjectRef, PlayerRef},
    mana::{ManaColor, Pip},
    state::Zone,
};

/// The captured trigger controller supplies Controller/Opponent through ExecCtx.
/// Recheck Raid at resolution, then offer the affected opponent's non-target
/// sacrifice, followed by their private discard and finally three life loss.
pub(crate) fn raid() -> EffectOp {
    EffectOp::Conditional {
        cond: EffectCond::ControllerAttackedThisTurn,
        then: Box::new(EffectOp::MaySacrificeNonlandOtherwiseV1 {
            player: PlayerRef::Opponent,
            unpaid: Box::new(EffectOp::MayDiscardCardOtherwiseV1 {
                player: PlayerRef::Opponent,
                unpaid: Box::new(EffectOp::LoseLife {
                    player: PlayerRef::Opponent,
                    amount: 3,
                }),
            }),
        }),
        else_: Box::new(EffectOp::Sequence(Vec::new())),
    }
}

pub(crate) fn exact_definition(d: &crate::card_def::CardDef) -> bool {
    d.name == "Perforating Artist"
        && matches!(d.capability, CardCapability::Partial | CardCapability::Full)
        && d.types == [CardType::Creature]
        && d.subtypes == [Subtype::Devil]
        && d.supertypes.is_empty()
        && d.cost.generic == 1
        && d.cost.x_count == 0
        && d.cost.pips == [Pip::Colored(ManaColor::B), Pip::Colored(ManaColor::R)]
        && d.colors == [ManaColor::B, ManaColor::R]
        && d.mana_value == 3
        && d.power == Some(3)
        && d.toughness == Some(2)
        && !d.is_land
        && !d.is_token
        && d.keywords == Keywords::DEATHTOUCH
        && d.target_spec == TargetSpec::None
        && d.alt_cost.is_none()
        && d.kicker_cost.is_none()
        && d.additional_cost.is_none()
        && d.flashback.is_none()
        && d.ward_cost.is_none()
        && d.starting_loyalty.is_none()
        && d.object_name == "Perforating Artist"
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
        && d.activated_abilities.is_empty()
        && d.token_counter_replacement_v48.is_none()
        && (d.spell_effect)()
            == Some(EffectOp::MoveObject {
                object: ObjectRef::ThisSource,
                to_zone: Zone::Battlefield,
            })
}
