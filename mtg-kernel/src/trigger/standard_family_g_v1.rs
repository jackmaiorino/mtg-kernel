//! MageZero Standard family G: creatures with triggered abilities. Kept in
//! its own file so parallel Standard batches touch `trigger.rs` only through
//! the `triggers_for` and target-spec tables.

use super::{etb_trigger, TriggerCondition, TriggeredAbilityDef};
use crate::card_def::{CardType, Subtype};
use crate::effect::{
    CreatureSacrificeFilter, EffectObjectBinding, EffectOp, ObjectRef, PlayerRef, TargetRef,
};
use crate::ids::{ObjectId, PlayerId};
use crate::state::{GameState, Zone};

fn create_named_token(name: &str) -> EffectOp {
    EffectOp::CreateToken {
        token_def: crate::card_def::card_id_by_name(name)
            .unwrap_or_else(|| panic!("{name} in CARD_DEFS")),
        controller: PlayerRef::Controller,
    }
}

fn investigate_effect() -> EffectOp {
    create_named_token("Clue Token")
}

fn create_map_effect() -> EffectOp {
    create_named_token("Map Token")
}

fn source_explores_effect() -> EffectOp {
    EffectOp::ExploreTarget {
        object: ObjectRef::ThisSource,
    }
}

/// Novice Inspector: "When this creature enters, investigate."
pub(super) const NOVICE_INSPECTOR_TRIGGERS: [TriggeredAbilityDef; 1] =
    [etb_trigger(investigate_effect)];

/// Sentinel of the Nameless City: "Whenever this creature enters or attacks,
/// create a Map token."
pub(super) const SENTINEL_OF_THE_NAMELESS_CITY_TRIGGERS: [TriggeredAbilityDef; 2] = [
    etb_trigger(create_map_effect),
    TriggeredAbilityDef {
        condition: TriggerCondition::Attacks,
        ..etb_trigger(create_map_effect)
    },
];

/// Cenote Scout: "When this creature enters, it explores."
pub(super) const CENOTE_SCOUT_TRIGGERS: [TriggeredAbilityDef; 1] =
    [etb_trigger(source_explores_effect)];

fn target_player_sacrifices_creature_effect() -> EffectOp {
    EffectOp::SacrificeCreature {
        player: PlayerRef::Target(0),
        filter: CreatureSacrificeFilter::Any,
    }
}

fn exile_from_target_opponents_hand_effect() -> EffectOp {
    EffectOp::RevealHandChooseNonlandToLinkedExile {
        player: PlayerRef::Target(0),
    }
}

/// Gatekeeper of Malakir: "When this creature enters, if it was kicked,
/// target player sacrifices a creature."
pub(super) const GATEKEEPER_OF_MALAKIR_TRIGGERS: [TriggeredAbilityDef; 1] = [TriggeredAbilityDef {
    face_index: 0,
    condition: TriggerCondition::Etb,
    home_zone: Zone::Battlefield,
    intervening_if_kicked: true,
    intervening_if_controls_another_source_card: false,
    effect: target_player_sacrifices_creature_effect,
}];

/// Deep-Cavern Bat: "When this creature enters, look at target opponent's
/// hand. You may exile a nonland card from it until this creature leaves the
/// battlefield." The return is the linked exile's own duration, not a
/// trigger (see `effect::LinkedHandExileKind`).
pub(super) const DEEP_CAVERN_BAT_TRIGGERS: [TriggeredAbilityDef; 1] =
    [etb_trigger(exile_from_target_opponents_hand_effect)];

fn damage_opponent_one_effect() -> EffectOp {
    EffectOp::DealDamage {
        target: TargetRef::Opponent,
        amount: 1,
    }
}

/// Razorkin Needlehead: "Whenever an opponent draws a card, this creature
/// deals 1 damage to them." Its first strike during your turn is a static
/// (`standard_statics_v1`).
pub(super) const RAZORKIN_NEEDLEHEAD_TRIGGERS: [TriggeredAbilityDef; 1] = [TriggeredAbilityDef {
    condition: TriggerCondition::OpponentDraws,
    ..etb_trigger(damage_opponent_one_effect)
}];

fn counter_on_source_effect() -> EffectOp {
    EffectOp::BindPlusOnePlusOneCounterToTriggerSource
}

fn rookie_counter_and_investigate_effect() -> EffectOp {
    EffectOp::BindEntrantOutgrowsSourceThen {
        then: Box::new(EffectOp::Sequence(vec![
            EffectOp::BindPlusOnePlusOneCounterToTriggerSource,
            investigate_effect(),
        ])),
    }
}

fn adaptive_oil_counter_effect() -> EffectOp {
    EffectOp::BindEntrantOutgrowsSourceThen {
        then: Box::new(EffectOp::BindOilCounterToTriggerSource),
    }
}

/// The distribution with nothing to distribute. It is also the definition's
/// stand-in: the real program depends on the dying incarnation's counters,
/// so it is built by `quirion_beastcaller_dies_effect` when the trigger is
/// created.
fn empty_distribution_effect() -> EffectOp {
    EffectOp::DistributePlusOneCounters {
        total: 0,
        allocations: Vec::new(),
        finalized: true,
    }
}

/// Quirion Beastcaller: "Whenever you cast a creature spell, put a +1/+1
/// counter on this creature. When this creature dies, distribute X +1/+1
/// counters among any number of target creatures you control, where X is
/// the number of +1/+1 counters on this creature."
pub(super) const QUIRION_BEASTCALLER_TRIGGERS: [TriggeredAbilityDef; 2] = [
    TriggeredAbilityDef {
        condition: TriggerCondition::CastCreatureSpell,
        ..etb_trigger(counter_on_source_effect)
    },
    TriggeredAbilityDef {
        condition: TriggerCondition::LeftBattlefieldToGraveyard,
        ..etb_trigger(empty_distribution_effect)
    },
];

/// Freeze X from the departing incarnation. Allocation is announced during
/// trigger placement before opponents get priority.
pub(super) fn quirion_beastcaller_dies_effect(
    state: &GameState,
    source: ObjectId,
    _controller: PlayerId,
) -> EffectOp {
    let total = state
        .objects
        .get(source)
        .zone_change_count
        .checked_sub(1)
        .and_then(|departed| state.counter_lki_for(source, departed))
        .map_or(0, |counters| counters.plus1_plus1.max(0) as u32);
    EffectOp::DistributePlusOneCounters {
        total,
        allocations: Vec::new(),
        finalized: total == 0,
    }
}

pub(super) fn is_quirion_beastcaller_dies_effect(effect: &EffectOp) -> bool {
    matches!(effect, EffectOp::DistributePlusOneCounters { .. })
}

fn that_player_loses_half_life_effect() -> EffectOp {
    EffectOp::LoseHalfLifeRoundedUp {
        player: PlayerRef::Opponent,
    }
}

fn return_tapped_with_two_stun_effect() -> EffectOp {
    EffectOp::ReturnSourceFromGraveyardTappedWithStunCounters { stun: 2 }
}

/// Unstoppable Slasher: "Whenever this creature deals combat damage to a
/// player, they lose half their life, rounded up. When this creature dies,
/// if it had no counters on it, return it to the battlefield tapped under
/// its owner's control with two stun counters on it." In a two-player game
/// the damaged player is always the controller's opponent.
pub(super) const UNSTOPPABLE_SLASHER_TRIGGERS: [TriggeredAbilityDef; 2] = [
    TriggeredAbilityDef {
        condition: TriggerCondition::DealsCombatDamageToPlayer,
        ..etb_trigger(that_player_loses_half_life_effect)
    },
    TriggeredAbilityDef {
        condition: TriggerCondition::DiesWithoutCounters,
        ..etb_trigger(return_tapped_with_two_stun_effect)
    },
];

/// Ascendant Packleader: "Whenever you cast a spell with mana value 4 or
/// greater, put a +1/+1 counter on this creature." Its conditional entry
/// counter is in `standard_statics_v1`.
pub(super) const ASCENDANT_PACKLEADER_TRIGGERS: [TriggeredAbilityDef; 1] = [TriggeredAbilityDef {
    condition: TriggerCondition::CastSpellManaValueAtLeast(4),
    ..etb_trigger(counter_on_source_effect)
}];

/// Sharp-Eyed Rookie: "Whenever a creature you control enters, if its power
/// is greater than this creature's power or its toughness is greater than
/// this creature's toughness, put a +1/+1 counter on this creature and
/// investigate."
pub(super) const SHARP_EYED_ROOKIE_TRIGGERS: [TriggeredAbilityDef; 1] = [TriggeredAbilityDef {
    condition: TriggerCondition::ControlledCreatureEntersOutgrowingSource { another: false },
    ..etb_trigger(rookie_counter_and_investigate_effect)
}];

/// Evolving Adaptive: "Whenever another creature you control enters, if
/// that creature has greater power or toughness than this creature, put an
/// oil counter on this creature." Its entry oil counter and +1/+1 per oil
/// counter are in `standard_statics_v1`.
pub(super) const EVOLVING_ADAPTIVE_TRIGGERS: [TriggeredAbilityDef; 1] = [TriggeredAbilityDef {
    condition: TriggerCondition::ControlledCreatureEntersOutgrowingSource { another: true },
    ..etb_trigger(adaptive_oil_counter_effect)
}];

fn create_attacking_human_effect() -> EffectOp {
    EffectOp::CreateTokenTappedAndAttacking {
        token_def: crate::card_def::card_id_by_name("Human Token")
            .unwrap_or_else(|| panic!("Human Token in CARD_DEFS")),
    }
}

/// Adeline, Resplendent Cathar: "Whenever you attack, for each opponent,
/// create a 1/1 white Human creature token that's tapped and attacking that
/// player or a planeswalker they control." One opponent here, and no
/// planeswalkers attack. Its power is a characteristic-defining ability in
/// `standard_statics_v1`.
pub(super) const ADELINE_RESPLENDENT_CATHAR_TRIGGERS: [TriggeredAbilityDef; 1] =
    [TriggeredAbilityDef {
        condition: TriggerCondition::ControllerAttacks,
        ..etb_trigger(create_attacking_human_effect)
    }];

fn damage_target_opponent_one_effect() -> EffectOp {
    EffectOp::DealDamage {
        target: TargetRef::Target(0),
        amount: 1,
    }
}

/// Hired Claw: "Whenever you attack with one or more Lizards, this creature
/// deals 1 damage to target opponent." Its once-a-turn counter ability is
/// gated in `standard_statics_v1::activation_condition_met`.
pub(super) const HIRED_CLAW_TRIGGERS: [TriggeredAbilityDef; 1] = [TriggeredAbilityDef {
    condition: TriggerCondition::ControllerAttacksWithSubtype(Subtype::Lizard),
    ..etb_trigger(damage_target_opponent_one_effect)
}];

fn return_restricted_creature_effect() -> EffectOp {
    EffectOp::ReturnTargetCreatureCardRestrictedWhileSourceControlled { target_index: 0 }
}

/// Extraction Specialist: "When this creature enters, return target
/// creature card with mana value 2 or less from your graveyard to the
/// battlefield. That creature can't attack or block for as long as you
/// control this creature."
pub(super) const EXTRACTION_SPECIALIST_TRIGGERS: [TriggeredAbilityDef; 1] =
    [etb_trigger(return_restricted_creature_effect)];

/// Hullbreaker Horror: "Whenever you cast a spell, choose up to one -- •
/// Return target spell you don't control to its owner's hand. • Return
/// target nonland permanent to its owner's hand." The third mode is the
/// "up to one" choice of neither.
pub(super) const HULLBREAKER_HORROR_TRIGGERS: [TriggeredAbilityDef; 1] = [TriggeredAbilityDef {
    condition: TriggerCondition::CastSpell,
    ..etb_trigger(hullbreaker_horror_effect)
}];

pub(super) fn hullbreaker_horror_modes() -> Vec<(crate::card_def::TargetSpec, EffectOp)> {
    use crate::card_def::TargetSpec;
    use crate::effect::EffectCond;
    vec![
        (
            TargetSpec::SpellYouDontControl,
            EffectOp::Conditional {
                cond: EffectCond::TargetInZone(0, Zone::Stack),
                then: Box::new(EffectOp::MoveObject {
                    object: ObjectRef::Target(0),
                    to_zone: Zone::Hand,
                }),
                else_: Box::new(EffectOp::Sequence(vec![])),
            },
        ),
        (
            TargetSpec::NonlandPermanent,
            EffectOp::MoveObject {
                object: ObjectRef::Target(0),
                to_zone: Zone::Hand,
            },
        ),
        (TargetSpec::None, EffectOp::Sequence(vec![])),
    ]
}

/// The pending-trigger marker `trigger::unselected_trigger_modes` replaces
/// with one of the modes above.
pub(super) fn hullbreaker_horror_effect() -> EffectOp {
    EffectOp::Choice {
        controller: PlayerRef::Controller,
        options: hullbreaker_horror_modes()
            .into_iter()
            .map(|(_, effect)| effect)
            .collect(),
    }
}

/// Flying and a Map on entry.
pub(super) const SPYGLASS_SIREN_TRIGGERS: [TriggeredAbilityDef; 1] =
    [etb_trigger(create_map_effect)];

fn reveal_top_lose_life() -> EffectOp {
    EffectOp::RevealTopCardToHandLoseLifeEqualToManaValue
}
pub(super) const DARK_CONFIDANT_TRIGGERS: [TriggeredAbilityDef; 1] = [TriggeredAbilityDef {
    condition: TriggerCondition::BeginningOfUpkeep {
        controller_only: true,
    },
    ..etb_trigger(reveal_top_lose_life)
}];

fn gain_two_life() -> EffectOp {
    EffectOp::GainLife {
        player: PlayerRef::Controller,
        amount: 2,
    }
}
fn opponent_loses_two_life() -> EffectOp {
    EffectOp::LoseLife {
        player: PlayerRef::Opponent,
        amount: 2,
    }
}
pub(super) const SHEOLDRED_TRIGGERS: [TriggeredAbilityDef; 2] = [
    TriggeredAbilityDef {
        condition: TriggerCondition::ControllerDraws,
        ..etb_trigger(gain_two_life)
    },
    TriggeredAbilityDef {
        condition: TriggerCondition::OpponentDraws,
        ..etb_trigger(opponent_loses_two_life)
    },
];

fn surveil_one_effect() -> EffectOp {
    EffectOp::Surveil {
        player: PlayerRef::Controller,
        count: 1,
    }
}
pub(super) const FAERIE_DREAMTHIEF_TRIGGERS: [TriggeredAbilityDef; 1] =
    [etb_trigger(surveil_one_effect)];

fn wurmlet_effect() -> EffectOp {
    EffectOp::Sequence(vec![
        EffectOp::GainLife {
            player: PlayerRef::Controller,
            amount: 1,
        },
        EffectOp::CreatureUpgrade(
            crate::standard_creatures_v1::CreatureEffectV1::WurmletCounterIfFirstResolution,
        ),
    ])
}
pub(super) const TEETHING_WURMLET_TRIGGERS: [TriggeredAbilityDef; 1] = [TriggeredAbilityDef {
    condition: TriggerCondition::ControlledArtifactEnters,
    ..etb_trigger(wurmlet_effect)
}];
fn draw_one_effect() -> EffectOp {
    EffectOp::DrawCards {
        player: PlayerRef::Controller,
        count: 1,
    }
}
pub(super) const SURRAK_TRIGGERS: [TriggeredAbilityDef; 1] = [TriggeredAbilityDef {
    condition: TriggerCondition::ControlledCreatureOrCreatureSpellBecomesTargetOfOpponent,
    ..etb_trigger(draw_one_effect)
}];

fn golem_token_effect() -> EffectOp {
    create_named_token("Golem Token")
}
pub(super) const SANDSTORM_SALVAGER_TRIGGERS: [TriggeredAbilityDef; 1] =
    [etb_trigger(golem_token_effect)];
fn white_vampire_token_effect() -> EffectOp {
    create_named_token("Vampire Token")
}
fn draw_one_lose_one_effect() -> EffectOp {
    EffectOp::Sequence(vec![
        EffectOp::DrawCards {
            player: PlayerRef::Controller,
            count: 1,
        },
        EffectOp::LoseLife {
            player: PlayerRef::Controller,
            amount: 1,
        },
    ])
}
pub(super) const PREACHER_TRIGGERS: [TriggeredAbilityDef; 2] = [
    TriggeredAbilityDef {
        condition: TriggerCondition::AttacksPlayerWithMostLife,
        ..etb_trigger(white_vampire_token_effect)
    },
    TriggeredAbilityDef {
        condition: TriggerCondition::AttacksIfControllerMostLife,
        ..etb_trigger(draw_one_lose_one_effect)
    },
];
