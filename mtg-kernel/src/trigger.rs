//! State-based actions and trigger collection.
//!
//! After every resolution (a spell/ability resolving, a land entering, a
//! turn-based draw -- anything that produced `CommittedEvent`s), the engine
//! calls `collect_and_process`: it matches the already-committed events,
//! runs SBAs to a fixed point, then matches any events those SBAs created.
//! The combined newly-pending triggers are returned in APNAP order for
//! `engine.rs` to place on the stack (or, if 2+ share a controller, to ask
//! that controller to order via `engine::Decision::OrderTriggers`).

use crate::card_def::{
    CardType, DynamicValueDef, Keywords, OptionalAdditionalCostDef, Subtype, TargetSpec,
};
use crate::effect::{
    CardTypePredicate, EffectCond, EffectObjectBinding, EffectOp, ObjectRef, PlayerRef, TargetRef,
};
use crate::event::CommittedEvent;
use crate::ids::{ObjectId, PlayerId};
use crate::state::{
    AbilitySourceContractV4, GameState, InitiativeTriggerKindV1, PaidCostRefV4,
    StackTargetContractV4, Target, UndercityRoomV1, Zone,
};
use serde::{Deserialize, Serialize};

#[cfg(feature = "standard-magezero-fixtures")]
mod standard_family_g_v1;

/// Trigger conditions this increment's kernel can match.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TriggerCondition {
    /// The permanent itself enters the battlefield (Voldaren Epicure).
    Etb,
    /// The permanent itself enters while its controller controls the named
    /// number of other permanents with the effective subtype. This is the
    /// trigger-time half of an intervening-if condition.
    EtbControlsOtherSubtypeCount {
        subtype: Subtype,
        minimum_count: u8,
    },
    /// The permanent itself enters while its controller has at least this
    /// many creature cards in their own graveyard. This is the trigger-time
    /// half of an intervening-if condition; the matching resolution-time
    /// recheck is `effect::EffectCond::ControllerGraveyardCreatureCardsAtLeast`.
    /// Webweaver Changeling is the first consumer.
    EtbIfGraveyardCreatureCardsAtLeast(u8),
    /// The permanent itself deals damage. Unused by any Burn 16 card this
    /// increment (kept from increment 2's shape -- see `trigger_matches`).
    DealsDamage,
    /// The controller casts an instant or sorcery spell -- any such
    /// spell, not just ones this permanent's controller controls the
    /// *source* of (Guttersnipe and Murmuring Mystic: "whenever you cast an
    /// instant or sorcery spell"). Matched against
    /// `CommittedEvent::SpellCast`, logged by `engine::finalize_cast`.
    CastInstantOrSorcery,
    /// The controller casts a noncreature spell -- any such spell, not just
    /// ones this permanent's controller controls the *source* of (Kessig
    /// Flamebreather: "whenever you cast a noncreature spell"). The
    /// noncreature predicate is the same `selected_spell_types` check
    /// Black Mage's Rod's equipment-granted trigger uses.
    CastNoncreatureSpell,
    /// This exact source spell is cast. Creature cast triggers function
    /// while their source is on the stack.
    CastSelf,
    /// The controller draws their `n`th card since the current turn began
    /// (Sneaky Snacker: "whenever you draw your third card in a turn").
    /// This ability functions from the graveyard (`home_zone: Graveyard`
    /// on its `TriggeredAbilityDef`), so unlike every other condition
    /// here it's checked with the source in the graveyard, not the
    /// battlefield.
    DrawNth(u32),
    /// This permanent moves from the battlefield to its owner's graveyard
    /// (700.4 "dies", generalized to any battlefield -> graveyard
    /// transition regardless of card type -- Clockwork Percussionist's
    /// death and Experimental Synthesizer's own sacrifice-cost ability both
    /// have this exact event shape, and neither card's Java source
    /// distinguishes "dies" from "put into a graveyard from the
    /// battlefield" in a way this pool needs to model separately). Checked
    /// with the source *currently in the graveyard* (`home_zone: Graveyard`
    /// -- see `Etb`'s doc for why the outer gate uses the post-event zone,
    /// not a "functions from" zone: by the time `collect_and_process` looks,
    /// the object has already moved there).
    LeftBattlefieldToGraveyard,
    /// This permanent leaves the battlefield for any destination. Unlike a
    /// dies trigger, this includes exile, hand, and library moves.
    LeftBattlefield,
    /// This permanent's controller sacrifices another permanent that had
    /// the named effective subtype immediately before leaving.
    SacrificeAnotherWithSubtype(Subtype),
    /// This permanent's controller sacrifices another permanent, with no
    /// subtype restriction (Gixian Infiltrator: "whenever you sacrifice
    /// another permanent").
    SacrificeAnotherPermanent,
    /// This creature deals combat damage to a player. The committed marker
    /// carries the source's exact zone-change generation.
    DealsCombatDamageToPlayer,
    /// 505.2, matched against `event::CommittedEvent::UpkeepBegan`: the
    /// beginning of an Upkeep step. `controller_only` selects "at the
    /// beginning of *your* upkeep" (Delver of Secrets, `true`) versus "at
    /// the beginning of *each* upkeep" (`false`, no pool card yet, kept for
    /// the shape); either way, exactly one Upkeep step happens per turn
    /// (the active player's), so a `false` source still fires only on
    /// turns where that upkeep is the active player's own.
    BeginningOfUpkeep {
        controller_only: bool,
    },
    /// Each successful draw by the source's controller, including every
    /// individual card in a multi-card draw.
    ControllerDraws,
    /// A different creature enters under the source's controller. A subtype
    /// filter restricts the entrant rather than the observing permanent.
    OtherControlledCreatureEnters {
        subtype: Option<Subtype>,
    },
    /// Declared as an attacker. Being put onto the battlefield attacking
    /// does not satisfy this event.
    Attacks,
    /// The source's controller declares one or more attackers ("whenever
    /// you attack", Adeline, Resplendent Cathar).
    ControllerAttacks,
    /// The source's controller declares attackers including at least one
    /// with this subtype ("whenever you attack with one or more Lizards",
    /// Hired Claw).
    ControllerAttacksWithSubtype(Subtype),
    ControlledLandEnters,
    ControllerGainsLife,
    ControllerAddedPlusOneCountersToSelf {
        max_per_turn: Option<u16>,
    },
    /// Exact attack declaration with a trigger-time intervening threshold gate.
    AttacksWithControllerGraveyardCardCountAtLeast(u8),
    BeginningControllerEndStepIfCreatureDied,
    BeginningControllerEndStep,
    /// The controller casts a creature spell (Quirion Beastcaller).
    CastCreatureSpell,
    /// The controller casts any spell (Hullbreaker Horror).
    CastSpell,
    /// The controller casts a spell whose mana value on the stack is at
    /// least this (Ascendant Packleader).
    CastSpellManaValueAtLeast(u16),
    /// A creature enters under the source's controller with greater power or
    /// greater toughness than the source has at that moment: the trigger-time
    /// half of Sharp-Eyed Rookie's and Evolving Adaptive's intervening if.
    /// `another` excludes the source itself.
    ControlledCreatureEntersOutgrowingSource {
        another: bool,
    },
    /// Each successful draw by the source controller's opponent (Razorkin
    /// Needlehead: "whenever an opponent draws a card").
    OpponentDraws,
    /// The source dies, and the battlefield incarnation that died had no
    /// counters of any kind (Unstoppable Slasher's intervening if). Reads
    /// `GameState::counter_lki_for`, which only `standard-magezero-fixtures`
    /// builds record.
    DiesWithoutCounters,
    /// The creature this Equipment is attached to deals combat damage to a
    /// player (Goldvein Pick). The committed marker's source incarnation
    /// must be the Equipment's exact current host.
    EquippedCreatureDealsCombatDamageToPlayer,
    /// Valiant: this permanent becomes the target of a spell
    /// or ability its controller controls for the first time each turn.
    /// Matched against `CommittedEvent::Targeted`, which every cast,
    /// activation, trigger placement and copy retarget logs once per
    /// targeted battlefield incarnation. "First time each turn" uses the
    /// same per-turn use ledger as Exemplar of Light's capped trigger.
    BecomesTargetOfControllerSpellOrAbilityFirstTimeEachTurn,
    /// Warp's delayed trigger, modeled on the warped incarnation itself: the
    /// beginning of the next end step after this permanent resolved from a
    /// warp cast (`ObjectStateV4::warped_v1`, cleared by any zone change).
    /// Warp permanents are cast at sorcery speed, so the next end step is
    /// always in the same turn.
    BeginningEndStepAfterWarp,
    /// "When this card becomes plotted" (`home_zone: Exile`): the plot
    /// special action's own move from hand to exile. `plot_spell` stamps
    /// `plotted_turn` before collecting triggers, and nothing else moves a
    /// card into exile with that stamp, so the move event plus the stamp
    /// identifies the plotting.
    BecomesPlotted,
    /// "Whenever you commit a crime" (`CommittedEvent::CrimeCommitted`).
    ControllerCommitsCrime,
    /// Training: this creature attacks alongside another attacking creature
    /// with greater power (`CommittedEvent::DeclaredAttacker`, read against
    /// the full declared attacker set).
    AttacksWithGreaterPowerAttacker,
    /// "...or transforms into [this front face]" (`CommittedEvent::
    /// Transformed` to face zero).
    TransformsIntoFrontFace,
    /// "At the beginning of your end step, if you descended this turn"
    /// (a permanent card you owned went to your graveyard this turn).
    BeginningControllerEndStepIfDescended,
    /// "Whenever a creature you control becomes the target of a spell or
    /// ability an opponent controls" (`CommittedEvent::Targeted`).
    ControlledCreatureBecomesTargetOfOpponent,
    /// "At the beginning of combat on your turn"
    /// (`CommittedEvent::BeginningOfCombat`).
    BeginningOfControllerCombat,
    /// Unearth's "exile it at the beginning of the next end step"
    /// (`ObjectStateV4::unearthed_v1`).
    BeginningEndStepAfterUnearth,
    /// Flurry: "Whenever you cast your second spell each turn"
    /// (`CommittedEvent::SpellCast`, read against the caster's
    /// `spells_cast_this_turn` immediately after the cast).
    ControllerCastsSecondSpellEachTurn,
    /// Impending: "At the beginning of your end step, remove a time counter
    /// from it", collected only while it has one.
    BeginningControllerEndStepWithTimeCounter,
    /// Enduring: "When this dies, if it was a creature" (a battlefield to
    /// graveyard `ZoneChange` preceded by
    /// `CommittedEvent::WasCreatureBeforeLeavingBattlefield`).
    DiesIfWasCreature,
    /// "Whenever a creature you control deals combat damage to a player".
    ControlledCreatureDealsCombatDamageToPlayer,
    /// "Whenever one or more other creatures you control with power N or
    /// less enter ... This ability triggers only once each turn."
    OtherControlledCreatureWithPowerAtMostEntersOncePerTurn(i32),
    /// Global first actual gain, independent of this permanent's entry time.
    ControllerFirstLifeGain {
        own_turn_only: bool,
    },
    /// One cast trigger for the controller's noncreature spell OR a spell
    /// with the named subtype. A spell satisfying both still triggers once.
    CastNoncreatureOrSubtype(Subtype),
    /// One declaration by the controller containing at least this many
    /// creatures. The observing source need not attack.
    ControllerAttacksWithAtLeastCreatures(u8),
    /// The controller casts any spell during another player's turn. This is
    /// a trigger-time restriction, not an intervening-if resolution gate.
    CastSpellDuringOpponentsTurn,
}

pub struct TriggeredAbilityDef {
    pub condition: TriggerCondition,
    /// Which zone the source must be in for this ability to function.
    /// Every keyworded/triggered ability in MTG works from the
    /// battlefield unless its reminder text says otherwise (like Sneaky
    /// Snacker's "from your graveyard").
    pub home_zone: Zone,
    /// 603.4 intervening-if gate checked when the event happens. An unkicked
    /// Goblin Bushwhacker must not create a stack item at all (as opposed to
    /// creating a trigger whose effect later resolves to a no-op).
    pub intervening_if_kicked: bool,
    /// Faerie Miscreant's trigger-time 603.4 gate. This is deliberately a
    /// same-definition board predicate rather than a card-name branch. The
    /// matching resolution-time gate lives in `EffectCond`.
    pub intervening_if_controls_another_source_card: bool,
    /// The printed face whose text carries this ability: 0 for the front
    /// face (every ability but a transforming card's back-face ones), 1 for
    /// the back face. Only a battlefield permanent showing that face
    /// triggers it.
    pub face_index: u8,
    pub effect: fn() -> EffectOp,
}

pub(crate) fn materialize_trigger_effect(
    trigger: &TriggeredAbilityDef,
    source: ObjectId,
    state: &GameState,
) -> EffectOp {
    materialize_trigger_source_program((trigger.effect)(), source, state)
}

fn materialize_trigger_source_program(
    effect: EffectOp,
    source: ObjectId,
    state: &GameState,
) -> EffectOp {
    match effect {
        EffectOp::Conditional { cond, then, else_ } => EffectOp::Conditional {
            cond,
            then: Box::new(materialize_trigger_source_program(*then, source, state)),
            else_: Box::new(materialize_trigger_source_program(*else_, source, state)),
        },
        EffectOp::BindTemporaryBoostToTriggerSource { power, toughness } => {
            let live = state.objects.get(source);
            EffectOp::BoostBoundObjectUntilEndOfTurn {
                object: EffectObjectBinding {
                    object: source,
                    expected_zone: live.zone,
                    expected_zone_change_count: live.zone_change_count,
                },
                power,
                toughness,
            }
        }
        EffectOp::BindPlusOnePlusOneCounterToTriggerSource => {
            let live = state.objects.get(source);
            EffectOp::PutPlusOnePlusOneCounterOnBoundObject {
                object: EffectObjectBinding {
                    object: source,
                    expected_zone: live.zone,
                    expected_zone_change_count: live.zone_change_count,
                },
            }
        }
        EffectOp::BindWarpExileToTriggerSource => {
            let live = state.objects.get(source);
            EffectOp::WarpExileBoundObject {
                object: EffectObjectBinding {
                    object: source,
                    expected_zone: live.zone,
                    expected_zone_change_count: live.zone_change_count,
                },
            }
        }
        EffectOp::BindDoublePlusOneCountersToTriggerSource => {
            let live = state.objects.get(source);
            EffectOp::DoublePlusOneCountersOnBoundObject {
                object: EffectObjectBinding {
                    object: source,
                    expected_zone: live.zone,
                    expected_zone_change_count: live.zone_change_count,
                },
            }
        }
        EffectOp::BindOilCounterToTriggerSource => {
            let live = state.objects.get(source);
            EffectOp::PutOilCounterOnBoundObject {
                object: EffectObjectBinding {
                    object: source,
                    expected_zone: live.zone,
                    expected_zone_change_count: live.zone_change_count,
                },
            }
        }
        EffectOp::Sequence(steps) if steps.iter().any(contains_source_binding_template) => {
            EffectOp::Sequence(
                steps
                    .into_iter()
                    .map(|step| materialize_trigger_source_program(step, source, state))
                    .collect(),
            )
        }
        EffectOp::MaterializeStormCopies => {
            crate::engine::materialize_storm_copy_binding(state, source)
                .map(|binding| EffectOp::CreateStormCopies { binding })
                .unwrap_or(EffectOp::MaterializeStormCopies)
        }
        effect => effect,
    }
}

/// Whether a program still holds a source-binding template that
/// `materialize_trigger_source_program` must replace.
fn contains_source_binding_template(effect: &EffectOp) -> bool {
    match effect {
        EffectOp::BindTemporaryBoostToTriggerSource { .. }
        | EffectOp::BindPlusOnePlusOneCounterToTriggerSource
        | EffectOp::BindDoublePlusOneCountersToTriggerSource
        | EffectOp::BindOilCounterToTriggerSource => true,
        EffectOp::Sequence(steps) => steps.iter().any(contains_source_binding_template),
        _ => false,
    }
}

fn battlefield_entry_object(event: &CommittedEvent) -> Option<ObjectId> {
    match event {
        CommittedEvent::ZoneChange {
            object,
            to: Zone::Battlefield,
            ..
        }
        | CommittedEvent::CreateToken { object, .. } => Some(*object),
        _ => None,
    }
}

fn materialize_trigger_event_effect(
    trigger: &TriggeredAbilityDef,
    source: ObjectId,
    state: &GameState,
    event: &CommittedEvent,
) -> EffectOp {
    if let EffectOp::BindEntrantOutgrowsSourceThen { then } = (trigger.effect)() {
        if let Some(object) = battlefield_entry_object(event) {
            let live = state.objects.get(source);
            return EffectOp::IfEntrantOutgrowsSourceThen {
                entrant: EffectObjectBinding {
                    object,
                    expected_zone: Zone::Battlefield,
                    expected_zone_change_count: state.objects.get(object).zone_change_count,
                },
                source: EffectObjectBinding {
                    object: source,
                    expected_zone: live.zone,
                    expected_zone_change_count: live.zone_change_count,
                },
                then: Box::new(materialize_trigger_source_program(*then, source, state)),
            };
        }
    }
    if matches!(
        (trigger.effect)(),
        EffectOp::BindPlusOnePlusOneCounterToTriggerEventObject
    ) {
        if let Some(object) = battlefield_entry_object(event) {
            return EffectOp::PutPlusOnePlusOneCounterOnTriggerEventObject {
                object: EffectObjectBinding {
                    object,
                    expected_zone: Zone::Battlefield,
                    expected_zone_change_count: state.objects.get(object).zone_change_count,
                },
            };
        }
    }
    if let EffectOp::BindConvokedCreatureCountToLookTop { count, max_taken } = (trigger.effect)() {
        return EffectOp::LookTopTakeCreaturesManaValueAtMostThenShuffle {
            count,
            max_taken,
            max_mana_value: u16::from(state.objects.get(source).v4.convoked_creatures_v1),
        };
    }
    #[cfg(feature = "standard-magezero-fixtures")]
    if matches!(
        (trigger.effect)(),
        EffectOp::BindDamageOpponentEqualToSourceLastPower
    ) {
        return EffectOp::DealDamage {
            target: TargetRef::Opponent,
            amount: crate::standard_keywords_v1::power_before_leaving(state, source)
                .unwrap_or(0)
                .max(0),
        };
    }
    if matches!(
        (trigger.effect)(),
        EffectOp::BindPlusOneCounterOnAnotherTargetToTriggerTarget
    ) {
        if let CommittedEvent::Targeted { target, .. } = event {
            return EffectOp::PutPlusOnePlusOneCounterOnTargetOtherThan {
                other_than: *target,
            };
        }
    }
    if matches!((trigger.effect)(), EffectOp::BindIncubateToTriggerSpell) {
        if let CommittedEvent::SpellCast { spell, .. } = event {
            return EffectOp::Incubate {
                amount: spell_mana_value_on_stack(state, *spell),
            };
        }
    }
    materialize_trigger_effect(trigger, source, state)
}

/// A spell's mana value on the stack, including its announced X.
fn spell_mana_value_on_stack(state: &GameState, spell: ObjectId) -> u16 {
    state
        .stack
        .iter()
        .find(|item| item.source == spell)
        .map_or(0, |item| crate::engine::stack_spell_mana_value(state, item))
}

const fn etb_trigger(effect: fn() -> EffectOp) -> TriggeredAbilityDef {
    TriggeredAbilityDef {
        condition: TriggerCondition::Etb,
        home_zone: Zone::Battlefield,
        intervening_if_kicked: false,
        intervening_if_controls_another_source_card: false,
        face_index: 0,
        effect,
    }
}

const GAIN_ONE_LIFE_TRIGGERS: [TriggeredAbilityDef; 1] = [etb_trigger(gain_one_life_effect)];
const AJANIS_PRIDEMATE_TRIGGERS: [TriggeredAbilityDef; 1] = [TriggeredAbilityDef {
    condition: TriggerCondition::ControllerGainsLife,
    ..etb_trigger(writhing_chrysalis_counter_marker_effect)
}];
const MARAUDING_BLIGHT_PRIEST_TRIGGERS: [TriggeredAbilityDef; 1] = [TriggeredAbilityDef {
    condition: TriggerCondition::ControllerGainsLife,
    ..etb_trigger(opponent_loses_one_life_effect)
}];
const SANGUINE_SYPHONER_TRIGGERS: [TriggeredAbilityDef; 1] = [TriggeredAbilityDef {
    condition: TriggerCondition::Attacks,
    ..etb_trigger(sanguine_syphoner_effect)
}];

const HELPFUL_HUNTER_TRIGGERS: [TriggeredAbilityDef; 1] =
    [etb_trigger(ichor_wellspring_draw_effect)];
const PRIDEFUL_PARENT_TRIGGERS: [TriggeredAbilityDef; 1] = [etb_trigger(prideful_parent_effect)];
const ICEWIND_ELEMENTAL_TRIGGERS: [TriggeredAbilityDef; 1] =
    [etb_trigger(icewind_elemental_effect)];

const DREADWING_SCAVENGER_TRIGGERS: [TriggeredAbilityDef; 2] = [
    etb_trigger(icewind_elemental_effect),
    TriggeredAbilityDef {
        condition: TriggerCondition::Attacks,
        ..etb_trigger(icewind_elemental_effect)
    },
];

const MISCHIEVOUS_PUP_TRIGGERS: [TriggeredAbilityDef; 1] = [etb_trigger(mischievous_pup_effect)];

fn mischievous_pup_effect() -> EffectOp {
    EffectOp::MoveAllTargets {
        to_zone: Zone::Hand,
    }
}

const FELIDAR_SAVIOR_TRIGGERS: [TriggeredAbilityDef; 1] = [etb_trigger(felidar_savior_effect)];

const ARMASAUR_GUIDE_TRIGGERS: [TriggeredAbilityDef; 1] = [TriggeredAbilityDef {
    condition: TriggerCondition::ControllerAttacksWithAtLeastCreatures(3),
    ..etb_trigger(armasaur_guide_effect)
}];

const ARBITER_OF_WOE_TRIGGERS: [TriggeredAbilityDef; 1] = [etb_trigger(arbiter_of_woe_effect)];

fn arbiter_of_woe_effect() -> EffectOp {
    EffectOp::Sequence(vec![
        burglar_rat_effect(),
        EffectOp::LoseLife {
            player: PlayerRef::Opponent,
            amount: 2,
        },
        ichor_wellspring_draw_effect(),
        EffectOp::GainLife {
            player: PlayerRef::Controller,
            amount: 2,
        },
    ])
}

fn armasaur_guide_effect() -> EffectOp {
    EffectOp::AddCountersToTarget {
        target_index: 0,
        optional: false,
        plus1_plus1: 1,
        lifelink: 0,
        stun: 0,
    }
}

fn felidar_savior_effect() -> EffectOp {
    EffectOp::Sequence(
        (0..2)
            .map(|target_index| EffectOp::Conditional {
                cond: EffectCond::TargetIsLegalForAbility {
                    index: target_index,
                    spec: TargetSpec::UpToTwoOtherControlledCreatures,
                },
                then: Box::new(EffectOp::AddCountersToTarget {
                    target_index,
                    optional: true,
                    plus1_plus1: 1,
                    lifelink: 0,
                    stun: 0,
                }),
                else_: Box::new(EffectOp::Sequence(vec![])),
            })
            .collect(),
    )
}
const BURGLAR_RAT_TRIGGERS: [TriggeredAbilityDef; 1] = [etb_trigger(burglar_rat_effect)];
const INFESTATION_SAGE_TRIGGERS: [TriggeredAbilityDef; 1] = [TriggeredAbilityDef {
    condition: TriggerCondition::LeftBattlefieldToGraveyard,
    home_zone: Zone::Graveyard,
    ..etb_trigger(infestation_sage_effect)
}];
const WARY_THESPIAN_TRIGGERS: [TriggeredAbilityDef; 2] = [
    etb_trigger(conduit_pylons_etb_effect),
    TriggeredAbilityDef {
        condition: TriggerCondition::LeftBattlefieldToGraveyard,
        home_zone: Zone::Graveyard,
        ..etb_trigger(conduit_pylons_etb_effect)
    },
];
const SPITFIRE_LAGAC_TRIGGERS: [TriggeredAbilityDef; 1] = [TriggeredAbilityDef {
    condition: TriggerCondition::ControlledLandEnters,
    ..etb_trigger(kessig_flamebreather_effect)
}];
const ELEMENTALIST_ADEPT_TRIGGERS: [TriggeredAbilityDef; 1] = [TriggeredAbilityDef {
    condition: TriggerCondition::CastNoncreatureSpell,
    ..etb_trigger(prowess_effect)
}];
const LIGHTSHELL_DUO_TRIGGERS: [TriggeredAbilityDef; 2] = [
    etb_trigger(lightshell_duo_etb_effect),
    TriggeredAbilityDef {
        condition: TriggerCondition::CastNoncreatureSpell,
        ..etb_trigger(prowess_effect)
    },
];
const CEPHALID_INKMAGE_TRIGGERS: [TriggeredAbilityDef; 1] =
    [etb_trigger(cephalid_inkmage_etb_effect)];
const BILLOWING_SHRIEKMASS_TRIGGERS: [TriggeredAbilityDef; 1] =
    [etb_trigger(billowing_shriekmass_effect)];
const APOTHECARY_STOMPER_TRIGGERS: [TriggeredAbilityDef; 1] =
    [etb_trigger(apothecary_stomper_effect)];

fn billowing_shriekmass_effect() -> EffectOp {
    EffectOp::MillCards {
        player: PlayerRef::Controller,
        count: 3,
    }
}

fn apothecary_stomper_modes() -> Vec<(TargetSpec, EffectOp)> {
    vec![
        (
            TargetSpec::ControlledCreature,
            EffectOp::AddPlusOnePlusOneCounters {
                object: ObjectRef::Target(0),
                count: 2,
            },
        ),
        (
            TargetSpec::None,
            EffectOp::GainLife {
                player: PlayerRef::Controller,
                amount: 4,
            },
        ),
    ]
}

fn apothecary_stomper_effect() -> EffectOp {
    EffectOp::Choice {
        controller: PlayerRef::Controller,
        options: apothecary_stomper_modes()
            .into_iter()
            .map(|(_, effect)| effect)
            .collect(),
    }
}
const CRYPT_FEASTER_TRIGGERS: [TriggeredAbilityDef; 1] = [TriggeredAbilityDef {
    condition: TriggerCondition::AttacksWithControllerGraveyardCardCountAtLeast(7),
    ..etb_trigger(crypt_feaster_threshold_effect)
}];
const ERUDITE_WIZARD_TRIGGERS: [TriggeredAbilityDef; 1] = [TriggeredAbilityDef {
    condition: TriggerCondition::DrawNth(2),
    ..etb_trigger(writhing_chrysalis_counter_marker_effect)
}];
const PHYREXIAN_ARENA_TRIGGERS: [TriggeredAbilityDef; 1] = [TriggeredAbilityDef {
    condition: TriggerCondition::BeginningOfUpkeep {
        controller_only: true,
    },
    ..etb_trigger(phyrexian_arena_effect)
}];
const GLEAMING_BARRIER_TRIGGERS: [TriggeredAbilityDef; 1] = [TriggeredAbilityDef {
    condition: TriggerCondition::LeftBattlefieldToGraveyard,
    home_zone: Zone::Graveyard,
    ..etb_trigger(gleaming_barrier_effect)
}];
const BIGFIN_BOUNCER_TRIGGERS: [TriggeredAbilityDef; 1] = [etb_trigger(bigfin_bouncer_effect)];
const TRAGIC_BANSHEE_TRIGGERS: [TriggeredAbilityDef; 1] = [etb_trigger(tragic_banshee_effect)];
const BATTLESONG_BERSERKER_TRIGGERS: [TriggeredAbilityDef; 1] = [TriggeredAbilityDef {
    condition: TriggerCondition::ControllerAttacks,
    ..etb_trigger(battlesong_berserker_effect)
}];
const SCRAWLING_CRAWLER_TRIGGERS: [TriggeredAbilityDef; 2] = [
    TriggeredAbilityDef {
        condition: TriggerCondition::BeginningOfUpkeep {
            controller_only: true,
        },
        ..etb_trigger(scrawling_crawler_upkeep_effect)
    },
    TriggeredAbilityDef {
        condition: TriggerCondition::OpponentDraws,
        ..etb_trigger(scrawling_crawler_draw_effect)
    },
];
const GRAPPLING_KRAKEN_TRIGGERS: [TriggeredAbilityDef; 1] = [TriggeredAbilityDef {
    condition: TriggerCondition::ControlledLandEnters,
    ..etb_trigger(grappling_kraken_effect)
}];
const RUNE_SCARRED_DEMON_TRIGGERS: [TriggeredAbilityDef; 1] =
    [etb_trigger(rune_scarred_demon_effect)];
const TATYOVA_BENTHIC_DRUID_TRIGGERS: [TriggeredAbilityDef; 1] = [TriggeredAbilityDef {
    condition: TriggerCondition::ControlledLandEnters,
    ..etb_trigger(tatyova_benthic_druid_effect)
}];

/// Prowess (702.108a): "Whenever you cast a noncreature spell, this
/// creature gets +1/+1 until end of turn."
fn prowess_effect() -> EffectOp {
    EffectOp::BindTemporaryBoostToTriggerSource {
        power: 1,
        toughness: 1,
    }
}

/// Threshold is an intervening-if clause, so it is rechecked on resolution.
fn crypt_feaster_threshold_effect() -> EffectOp {
    EffectOp::Conditional {
        cond: EffectCond::ControllerGraveyardCardCountAtLeast(7),
        then: Box::new(EffectOp::BindTemporaryBoostToTriggerSource {
            power: 2,
            toughness: 0,
        }),
        else_: Box::new(EffectOp::Sequence(vec![])),
    }
}

fn phyrexian_arena_effect() -> EffectOp {
    EffectOp::Sequence(vec![
        ichor_wellspring_draw_effect(),
        EffectOp::LoseLife {
            player: PlayerRef::Controller,
            amount: 1,
        },
    ])
}

/// Rune-Scarred Demon: "When this creature enters, search your library for
/// a card, put it into your hand, then shuffle." The found card is not
/// revealed, matching Grim Tutor's search.
fn rune_scarred_demon_effect() -> EffectOp {
    EffectOp::SearchLibraryToHand {
        player: PlayerRef::Controller,
        filter: crate::effect::LibraryCardFilter::AnyCard,
    }
}

/// Tatyova, Benthic Druid: "Whenever a land you control enters, you gain 1
/// life and draw a card."
fn tatyova_benthic_druid_effect() -> EffectOp {
    EffectOp::Sequence(vec![gain_one_life_effect(), ichor_wellspring_draw_effect()])
}

fn gleaming_barrier_effect() -> EffectOp {
    EffectOp::CreateToken {
        token_def: crate::card_def::card_id_by_name("Treasure Token")
            .expect("Treasure Token in CARD_DEFS"),
        controller: PlayerRef::Controller,
    }
}

fn bigfin_bouncer_effect() -> EffectOp {
    EffectOp::MoveObject {
        object: ObjectRef::Target(0),
        to_zone: Zone::Hand,
    }
}

fn tragic_banshee_effect() -> EffectOp {
    let boost = |amount| EffectOp::PumpTargetUntilEndOfTurnDynamic {
        target: TargetRef::Target(0),
        power: DynamicValueDef::Fixed(amount),
        toughness: DynamicValueDef::Fixed(amount),
    };
    EffectOp::Conditional {
        cond: EffectCond::CreatureDiedThisTurn,
        then: Box::new(boost(-13)),
        else_: Box::new(boost(-1)),
    }
}

fn grappling_kraken_effect() -> EffectOp {
    EffectOp::Sequence(vec![
        EffectOp::TapObject {
            object: ObjectRef::Target(0),
        },
        EffectOp::AddCountersToTarget {
            target_index: 0,
            optional: false,
            plus1_plus1: 0,
            lifelink: 0,
            stun: 1,
        },
    ])
}

fn battlesong_berserker_effect() -> EffectOp {
    EffectOp::Sequence(vec![
        EffectOp::PumpTargetUntilEndOfTurnDynamic {
            target: TargetRef::Target(0),
            power: DynamicValueDef::Fixed(1),
            toughness: DynamicValueDef::Fixed(0),
        },
        EffectOp::GrantKeywordTargetUntilEndOfTurn {
            object: ObjectRef::Target(0),
            keyword: Keywords::MENACE,
        },
    ])
}

fn scrawling_crawler_upkeep_effect() -> EffectOp {
    // The trigger's controller is the active player for this upkeep.
    // Sequential drawing therefore follows active-player, nonactive-player order.
    EffectOp::Sequence(vec![
        EffectOp::DrawCards {
            player: PlayerRef::Controller,
            count: 1,
        },
        EffectOp::DrawCards {
            player: PlayerRef::Opponent,
            count: 1,
        },
    ])
}

fn scrawling_crawler_draw_effect() -> EffectOp {
    EffectOp::LoseLife {
        player: PlayerRef::Opponent,
        amount: 1,
    }
}

const DRAGON_TRAINER_TRIGGERS: [TriggeredAbilityDef; 1] = [etb_trigger(dragon_trainer_effect)];
const RESOLUTE_REINFORCEMENTS_TRIGGERS: [TriggeredAbilityDef; 1] =
    [etb_trigger(resolute_reinforcements_effect)];
const ELFSWORN_GIANT_TRIGGERS: [TriggeredAbilityDef; 1] = [TriggeredAbilityDef {
    condition: TriggerCondition::ControlledLandEnters,
    ..etb_trigger(elfsworn_giant_effect)
}];
const EAGER_TRUFFLESNOUT_TRIGGERS: [TriggeredAbilityDef; 1] = [TriggeredAbilityDef {
    condition: TriggerCondition::DealsCombatDamageToPlayer,
    ..etb_trigger(generous_ent_effect)
}];
const RITE_OF_THE_DRAGONCALLER_TRIGGERS: [TriggeredAbilityDef; 1] = [TriggeredAbilityDef {
    condition: TriggerCondition::CastInstantOrSorcery,
    ..etb_trigger(rite_of_the_dragoncaller_effect)
}];

fn prideful_parent_effect() -> EffectOp {
    EffectOp::CreateToken {
        token_def: crate::card_def::card_id_by_name("Cat Token").expect("Cat Token in CARD_DEFS"),
        controller: PlayerRef::Controller,
    }
}

fn create_controller_token_effect(name: &str) -> EffectOp {
    EffectOp::CreateToken {
        token_def: crate::card_def::card_id_by_name(name)
            .unwrap_or_else(|| panic!("{name} in CARD_DEFS")),
        controller: PlayerRef::Controller,
    }
}

fn icewind_elemental_effect() -> EffectOp {
    EffectOp::Sequence(vec![
        EffectOp::DrawCards {
            player: PlayerRef::Controller,
            count: 1,
        },
        EffectOp::DiscardCards {
            player: PlayerRef::Controller,
            count: 1,
        },
    ])
}

fn burglar_rat_effect() -> EffectOp {
    // "Each opponent discards a card"; the kernel is strictly 1v1 and the
    // discarding player chooses.
    EffectOp::DiscardCards {
        player: PlayerRef::Opponent,
        count: 1,
    }
}

fn infestation_sage_effect() -> EffectOp {
    EffectOp::CreateToken {
        token_def: crate::card_def::card_id_by_name("Insect Token")
            .expect("Insect Token in CARD_DEFS"),
        controller: PlayerRef::Controller,
    }
}

fn dragon_trainer_effect() -> EffectOp {
    create_controller_token_effect("Dragon Token")
}

fn resolute_reinforcements_effect() -> EffectOp {
    create_controller_token_effect("Soldier Token")
}

fn elfsworn_giant_effect() -> EffectOp {
    create_controller_token_effect("Elf Warrior Token")
}

fn rite_of_the_dragoncaller_effect() -> EffectOp {
    create_controller_token_effect("Dragon 5/5 Token")
}

fn opponent_loses_one_life_effect() -> EffectOp {
    EffectOp::LoseLife {
        player: PlayerRef::Opponent,
        amount: 1,
    }
}

fn sanguine_syphoner_effect() -> EffectOp {
    EffectOp::Sequence(vec![
        opponent_loses_one_life_effect(),
        gain_one_life_effect(),
    ])
}
const DAZZLING_ANGEL_TRIGGERS: [TriggeredAbilityDef; 1] = [TriggeredAbilityDef {
    condition: TriggerCondition::OtherControlledCreatureEnters { subtype: None },
    ..etb_trigger(gain_one_life_effect)
}];
const CLINQUANT_SKYMAGE_TRIGGERS: [TriggeredAbilityDef; 1] = [TriggeredAbilityDef {
    condition: TriggerCondition::ControllerDraws,
    ..etb_trigger(writhing_chrysalis_counter_marker_effect)
}];
const MISCHIEVOUS_MYSTIC_TRIGGERS: [TriggeredAbilityDef; 1] = [TriggeredAbilityDef {
    condition: TriggerCondition::DrawNth(2),
    ..etb_trigger(mischievous_mystic_effect)
}];
const HOMUNCULUS_HORDE_TRIGGERS: [TriggeredAbilityDef; 1] = [TriggeredAbilityDef {
    condition: TriggerCondition::DrawNth(2),
    ..etb_trigger(homunculus_horde_effect)
}];

const KOMA_WORLD_EATER_TRIGGERS: [TriggeredAbilityDef; 1] = [TriggeredAbilityDef {
    condition: TriggerCondition::DealsCombatDamageToPlayer,
    ..etb_trigger(koma_coils_effect)
}];

const KIORA_RISING_TIDE_TRIGGERS: [TriggeredAbilityDef; 2] = [
    etb_trigger(kiora_draw_discard_effect),
    TriggeredAbilityDef {
        condition: TriggerCondition::AttacksWithControllerGraveyardCardCountAtLeast(7),
        ..etb_trigger(kiora_threshold_effect)
    },
];

const CACKLING_PROWLER_TRIGGERS: [TriggeredAbilityDef; 1] = [TriggeredAbilityDef {
    condition: TriggerCondition::BeginningControllerEndStepIfCreatureDied,
    ..etb_trigger(prowler_morbid_effect)
}];
const WARDENS_OF_THE_CYCLE_TRIGGERS: [TriggeredAbilityDef; 1] = [TriggeredAbilityDef {
    condition: TriggerCondition::BeginningControllerEndStepIfCreatureDied,
    ..etb_trigger(wardens_of_the_cycle_effect)
}];

fn wardens_of_the_cycle_modes() -> Vec<(TargetSpec, EffectOp)> {
    [
        EffectOp::GainLife {
            player: PlayerRef::Controller,
            amount: 2,
        },
        EffectOp::Sequence(vec![
            EffectOp::DrawCards {
                player: PlayerRef::Controller,
                count: 1,
            },
            EffectOp::LoseLife {
                player: PlayerRef::Controller,
                amount: 1,
            },
        ]),
    ]
    .into_iter()
    .map(|branch| {
        (
            TargetSpec::None,
            EffectOp::Conditional {
                cond: EffectCond::CreatureDiedThisTurn,
                then: Box::new(branch),
                else_: Box::new(EffectOp::Sequence(vec![])),
            },
        )
    })
    .collect()
}

fn wardens_of_the_cycle_effect() -> EffectOp {
    EffectOp::Choice {
        controller: PlayerRef::Controller,
        options: wardens_of_the_cycle_modes()
            .into_iter()
            .map(|(_, effect)| effect)
            .collect(),
    }
}

const SYLVAN_SCAVENGING_TRIGGERS: [TriggeredAbilityDef; 1] = [TriggeredAbilityDef {
    condition: TriggerCondition::BeginningControllerEndStep,
    ..etb_trigger(sylvan_scavenging_effect)
}];

fn sylvan_scavenging_modes() -> Vec<(TargetSpec, EffectOp)> {
    vec![
        (
            TargetSpec::ControlledCreature,
            EffectOp::AddPlusOnePlusOneCounters {
                object: ObjectRef::Target(0),
                count: 1,
            },
        ),
        (
            TargetSpec::None,
            EffectOp::Conditional {
                cond: EffectCond::ControlsCreaturePowerAtLeast(4),
                then: Box::new(EffectOp::CreateToken {
                    token_def: crate::card_def::card_id_by_name("Raccoon Token")
                        .expect("Raccoon Token in CARD_DEFS"),
                    controller: PlayerRef::Controller,
                }),
                else_: Box::new(EffectOp::Sequence(vec![])),
            },
        ),
    ]
}

fn celestial_armor_effect() -> EffectOp {
    EffectOp::Sequence(vec![
        EffectOp::AttachSourceToTarget {
            object: ObjectRef::Target(0),
        },
        EffectOp::GrantKeywordTargetUntilEndOfTurn {
            object: ObjectRef::Target(0),
            keyword: Keywords::HEXPROOF | Keywords::INDESTRUCTIBLE,
        },
    ])
}

const CELESTIAL_ARMOR_TRIGGERS: [TriggeredAbilityDef; 1] = [TriggeredAbilityDef {
    condition: TriggerCondition::Etb,
    home_zone: Zone::Battlefield,
    intervening_if_kicked: false,
    intervening_if_controls_another_source_card: false,
    face_index: 0,
    effect: celestial_armor_effect,
}];

fn sylvan_scavenging_effect() -> EffectOp {
    EffectOp::Choice {
        controller: PlayerRef::Controller,
        options: sylvan_scavenging_modes()
            .into_iter()
            .map(|(_, effect)| effect)
            .collect(),
    }
}

/// Trigger programs chosen when the event happens instead of being returned
/// by `triggers_for` (Moon-Circuit Hacker skips its discard on the turn it
/// entered). Variants replace the owning trigger's default inventory program.
pub fn event_time_trigger_programs(card_def: u16, condition: TriggerCondition) -> Vec<EffectOp> {
    let Some(card) = crate::card_def::CARD_DEFS.get(card_def as usize) else {
        return Vec::new();
    };
    if card.name == "Moon-Circuit Hacker"
        && matches!(condition, TriggerCondition::DealsCombatDamageToPlayer)
    {
        [false, true]
            .into_iter()
            .map(moon_circuit_hacker_combat_effect_for_entered_this_turn)
            .collect()
    } else {
        Vec::new()
    }
}

/// Whether this trigger's program is built when the trigger is created, from
/// the dying incarnation's last-known +1/+1 counters and its controller's
/// creatures (Quirion Beastcaller), so the definition's own program is an
/// empty stand-in. The rules-vector extractor describes it through this.
pub fn distributes_last_known_plus_one_counters(
    card_def: u16,
    condition: TriggerCondition,
) -> bool {
    #[cfg(feature = "standard-magezero-fixtures")]
    {
        crate::card_def::CARD_DEFS
            .get(card_def as usize)
            .is_some_and(|card| card.name == "Quirion Beastcaller")
            && matches!(condition, TriggerCondition::LeftBattlefieldToGraveyard)
    }
    #[cfg(not(feature = "standard-magezero-fixtures"))]
    {
        let _ = (card_def, condition);
        false
    }
}

/// A definition-owned modal program waiting for its placement-time choice.
/// The root is only a pending-trigger marker. It must never reach the stack
/// or the resolution interpreter; selection replaces it with one branch.
pub fn unselected_trigger_modes(
    card_def: u16,
    effect: &EffectOp,
) -> Option<Vec<(TargetSpec, EffectOp)>> {
    let card = crate::card_def::CARD_DEFS.get(card_def as usize)?;
    if card.name == "Apothecary Stomper" && *effect == apothecary_stomper_effect() {
        return Some(apothecary_stomper_modes());
    }
    if card.name == "Wardens of the Cycle" && *effect == wardens_of_the_cycle_effect() {
        return Some(wardens_of_the_cycle_modes());
    }
    #[cfg(feature = "standard-magezero-fixtures")]
    if card.name == "Hullbreaker Horror"
        && *effect == standard_family_g_v1::hullbreaker_horror_effect()
    {
        return Some(standard_family_g_v1::hullbreaker_horror_modes());
    }
    (card.name == "Sylvan Scavenging" && *effect == sylvan_scavenging_effect())
        .then(sylvan_scavenging_modes)
}

fn prowler_morbid_effect() -> EffectOp {
    EffectOp::Conditional {
        cond: EffectCond::CreatureDiedThisTurn,
        then: Box::new(EffectOp::BindPlusOnePlusOneCounterToTriggerSource),
        else_: Box::new(EffectOp::Sequence(vec![])),
    }
}

fn kiora_draw_discard_effect() -> EffectOp {
    EffectOp::Sequence(vec![
        EffectOp::DrawCards {
            player: PlayerRef::Controller,
            count: 2,
        },
        EffectOp::DiscardCards {
            player: PlayerRef::Controller,
            count: 2,
        },
    ])
}

fn kiora_threshold_effect() -> EffectOp {
    let token_def = crate::card_def::card_id_by_name("Scion of the Deep Token")
        .expect("Scion of the Deep Token in CARD_DEFS");
    EffectOp::Conditional {
        cond: EffectCond::ControllerGraveyardCardCountAtLeast(7),
        then: Box::new(EffectOp::Choice {
            controller: PlayerRef::Controller,
            options: vec![
                EffectOp::Sequence(vec![]),
                EffectOp::CreateToken {
                    token_def,
                    controller: PlayerRef::Controller,
                },
            ],
        }),
        else_: Box::new(EffectOp::Sequence(vec![])),
    }
}

fn koma_coils_effect() -> EffectOp {
    let token_def = crate::card_def::card_id_by_name("Koma's Coil Token")
        .expect("Koma's Coil Token in CARD_DEFS");
    EffectOp::Sequence(vec![
        EffectOp::CreateToken {
            token_def,
            controller: PlayerRef::Controller,
        };
        4
    ])
}

fn homunculus_horde_effect() -> EffectOp {
    let token_def = crate::card_def::card_id_by_name("Homunculus Horde Token")
        .expect("Homunculus Horde Token in CARD_DEFS");
    EffectOp::CreateToken {
        token_def,
        controller: PlayerRef::Controller,
    }
}

fn mischievous_mystic_effect() -> EffectOp {
    let token_def =
        crate::card_def::card_id_by_name("Faerie Token").expect("Faerie Token in CARD_DEFS");
    EffectOp::CreateToken {
        token_def,
        controller: PlayerRef::Controller,
    }
}
const DWYNENS_ELITE_TRIGGERS: [TriggeredAbilityDef; 1] = [TriggeredAbilityDef {
    condition: TriggerCondition::EtbControlsOtherSubtypeCount {
        subtype: Subtype::Elf,
        minimum_count: 1,
    },
    ..etb_trigger(dwynens_elite_effect)
}];
const GOOD_FORTUNE_UNICORN_TRIGGERS: [TriggeredAbilityDef; 1] = [TriggeredAbilityDef {
    condition: TriggerCondition::OtherControlledCreatureEnters { subtype: None },
    ..etb_trigger(entering_creature_counter_effect)
}];
const GUARDED_HEIR_TRIGGERS: [TriggeredAbilityDef; 1] = [etb_trigger(guarded_heir_effect)];
const YOUTHFUL_VALKYRIE_TRIGGERS: [TriggeredAbilityDef; 1] = [TriggeredAbilityDef {
    condition: TriggerCondition::OtherControlledCreatureEnters {
        subtype: Some(Subtype::Angel),
    },
    ..etb_trigger(writhing_chrysalis_counter_marker_effect)
}];

const BEAST_KIN_RANGER_TRIGGERS: [TriggeredAbilityDef; 1] = [TriggeredAbilityDef {
    condition: TriggerCondition::OtherControlledCreatureEnters { subtype: None },
    ..etb_trigger(beast_kin_ranger_effect)
}];
const MOSSBORN_HYDRA_TRIGGERS: [TriggeredAbilityDef; 1] = [TriggeredAbilityDef {
    condition: TriggerCondition::ControlledLandEnters,
    ..etb_trigger(double_counter_marker_effect)
}];

const EXEMPLAR_OF_LIGHT_TRIGGERS: [TriggeredAbilityDef; 2] = [
    TriggeredAbilityDef {
        condition: TriggerCondition::ControllerGainsLife,
        ..etb_trigger(writhing_chrysalis_counter_marker_effect)
    },
    TriggeredAbilityDef {
        condition: TriggerCondition::ControllerAddedPlusOneCountersToSelf {
            max_per_turn: Some(1),
        },
        ..etb_trigger(ichor_wellspring_draw_effect)
    },
];

const SUN_BLESSED_HEALER_TRIGGERS: [TriggeredAbilityDef; 1] = [TriggeredAbilityDef {
    intervening_if_kicked: true,
    ..etb_trigger(sun_blessed_healer_effect)
}];

const VANGUARD_SERAPH_TRIGGERS: [TriggeredAbilityDef; 1] = [TriggeredAbilityDef {
    condition: TriggerCondition::ControllerFirstLifeGain {
        own_turn_only: false,
    },
    ..etb_trigger(vanguard_seraph_effect)
}];

const CAT_COLLECTOR_TRIGGERS: [TriggeredAbilityDef; 2] = [
    etb_trigger(generous_ent_effect),
    TriggeredAbilityDef {
        condition: TriggerCondition::ControllerFirstLifeGain {
            own_turn_only: true,
        },
        ..etb_trigger(prideful_parent_effect)
    },
];

fn vanguard_seraph_effect() -> EffectOp {
    EffectOp::Surveil {
        player: PlayerRef::Controller,
        count: 1,
    }
}

fn sun_blessed_healer_effect() -> EffectOp {
    EffectOp::Conditional {
        cond: EffectCond::WasKicked,
        then: Box::new(EffectOp::ReturnTargetPermanentToBattlefield { target_index: 0 }),
        else_: Box::new(EffectOp::Sequence(vec![])),
    }
}

const ELVISH_REGROWER_TRIGGERS: [TriggeredAbilityDef; 1] = [etb_trigger(elvish_regrower_effect)];
const VAMPIRE_SOULCALLER_TRIGGERS: [TriggeredAbilityDef; 1] = [etb_trigger(elvish_regrower_effect)];
const AFFECTIONATE_INDRIK_TRIGGERS: [TriggeredAbilityDef; 1] =
    [etb_trigger(affectionate_indrik_effect)];
fn affectionate_indrik_effect() -> EffectOp {
    EffectOp::Choice {
        controller: PlayerRef::Controller,
        options: vec![
            EffectOp::Sequence(vec![]),
            EffectOp::FightObjects {
                first: ObjectRef::ThisSource,
                second: ObjectRef::Target(0),
                target_spec: TargetSpec::OpponentControlledCreature,
            },
        ],
    }
}
fn elvish_regrower_effect() -> EffectOp {
    EffectOp::MoveAllTargets {
        to_zone: Zone::Hand,
    }
}
const AMBUSH_WOLF_TRIGGERS: [TriggeredAbilityDef; 1] = [etb_trigger(ambush_wolf_effect)];
fn ambush_wolf_effect() -> EffectOp {
    EffectOp::MoveAllTargets {
        to_zone: Zone::Exile,
    }
}

fn double_counter_marker_effect() -> EffectOp {
    EffectOp::BindDoublePlusOneCountersToTriggerSource
}

const DWYNEN_TRIGGERS: [TriggeredAbilityDef; 1] = [TriggeredAbilityDef {
    condition: TriggerCondition::Attacks,
    ..etb_trigger(dwynen_attack_effect)
}];

fn beast_kin_ranger_effect() -> EffectOp {
    EffectOp::BindTemporaryBoostToTriggerSource {
        power: 1,
        toughness: 0,
    }
}

fn dwynen_attack_effect() -> EffectOp {
    EffectOp::GainLifeByAttackingSubtypeCount {
        player: PlayerRef::Controller,
        subtype: Subtype::Elf,
    }
}

fn gain_one_life_effect() -> EffectOp {
    EffectOp::GainLife {
        player: PlayerRef::Controller,
        amount: 1,
    }
}

fn entering_creature_counter_effect() -> EffectOp {
    EffectOp::BindPlusOnePlusOneCounterToTriggerEventObject
}

fn dwynens_elite_effect() -> EffectOp {
    EffectOp::Conditional {
        cond: EffectCond::ControlsOtherIncarnationSubtypeCount {
            subtype: Subtype::Elf,
            minimum_count: 1,
        },
        then: Box::new(EffectOp::CreateToken {
            token_def: crate::card_def::card_id_by_name("Elf Warrior Token")
                .expect("FDN Elf Warrior token"),
            controller: PlayerRef::Controller,
        }),
        else_: Box::new(EffectOp::Sequence(Vec::new())),
    }
}

fn guarded_heir_effect() -> EffectOp {
    let token_def = crate::card_def::card_id_by_name("Knight Token").expect("FDN Knight token");
    EffectOp::Sequence(vec![
        EffectOp::CreateToken {
            token_def,
            controller: PlayerRef::Controller,
        },
        EffectOp::CreateToken {
            token_def,
            controller: PlayerRef::Controller,
        },
    ])
}

fn guttersnipe_effect() -> EffectOp {
    // Guttersnipe deals 2 damage to each opponent.
    EffectOp::DealDamage {
        target: TargetRef::Opponent,
        amount: 2,
    }
}

fn murmuring_mystic_effect() -> EffectOp {
    // Create a 1/1 blue Bird Illusion creature token with flying. Token
    // characteristics live in the generated CardDef; the trigger uses the
    // same generic CreateToken leaf as Voldaren Epicure and Rally cards.
    let bird_illusion = crate::card_def::card_id_by_name("Bird Illusion Token")
        .expect("Bird Illusion Token in CARD_DEFS");
    EffectOp::CreateToken {
        token_def: bird_illusion,
        controller: PlayerRef::Controller,
    }
}

fn voldaren_epicure_effect() -> EffectOp {
    // It deals 1 damage to each opponent. Create a Blood token.
    let blood_token =
        crate::card_def::card_id_by_name("Blood Token").expect("Blood Token in CARD_DEFS");
    EffectOp::Sequence(vec![
        EffectOp::DealDamage {
            target: TargetRef::Opponent,
            amount: 1,
        },
        EffectOp::CreateToken {
            token_def: blood_token,
            controller: PlayerRef::Controller,
        },
    ])
}

fn generous_ent_effect() -> EffectOp {
    let food = crate::card_def::card_id_by_name("Food Token").expect("Food Token in CARD_DEFS");
    EffectOp::CreateToken {
        token_def: food,
        controller: PlayerRef::Controller,
    }
}

fn gingerbread_cabin_effect() -> EffectOp {
    let food = crate::card_def::card_id_by_name("Food Token").expect("Food Token in CARD_DEFS");
    EffectOp::Conditional {
        cond: EffectCond::ControlsOtherSubtypeCount {
            subtype: Subtype::Forest,
            minimum_count: 3,
        },
        then: Box::new(EffectOp::CreateToken {
            token_def: food,
            controller: PlayerRef::Controller,
        }),
        else_: Box::new(EffectOp::Sequence(Vec::new())),
    }
}

fn writhing_chrysalis_cast_effect() -> EffectOp {
    let spawn = crate::card_def::card_id_by_name("Eldrazi Spawn Token")
        .expect("Eldrazi Spawn Token in CARD_DEFS");
    EffectOp::Sequence(vec![
        EffectOp::CreateToken {
            token_def: spawn,
            controller: PlayerRef::Controller,
        },
        EffectOp::CreateToken {
            token_def: spawn,
            controller: PlayerRef::Controller,
        },
    ])
}

fn writhing_chrysalis_counter_marker_effect() -> EffectOp {
    EffectOp::BindPlusOnePlusOneCounterToTriggerSource
}

const BRINEBORN_CUTTHROAT_TRIGGERS: [TriggeredAbilityDef; 1] = [TriggeredAbilityDef {
    condition: TriggerCondition::CastSpellDuringOpponentsTurn,
    ..etb_trigger(writhing_chrysalis_counter_marker_effect)
}];

fn blood_fountain_effect() -> EffectOp {
    let blood = crate::card_def::card_id_by_name("Blood Token").expect("Blood Token in CARD_DEFS");
    EffectOp::CreateToken {
        token_def: blood,
        controller: PlayerRef::Controller,
    }
}

fn gain_three_life_effect() -> EffectOp {
    EffectOp::GainLife {
        player: PlayerRef::Controller,
        amount: 3,
    }
}

fn kessig_flamebreather_effect() -> EffectOp {
    // Whenever you cast a noncreature spell, it deals 1 damage to each
    // opponent.
    EffectOp::DealDamage {
        target: TargetRef::Opponent,
        amount: 1,
    }
}

fn gixian_infiltrator_effect() -> EffectOp {
    // Whenever you sacrifice another permanent, put a +1/+1 counter on
    // Gixian Infiltrator -- the same `BindPlusOnePlusOneCounterToTriggerSource`
    // shape Writhing Chrysalis's own sacrifice trigger uses.
    EffectOp::BindPlusOnePlusOneCounterToTriggerSource
}

fn webweaver_changeling_effect() -> EffectOp {
    // "When Webweaver Changeling enters the battlefield, if there are
    // three or more creature cards in your graveyard, you gain 5 life" --
    // the resolution-time half of the intervening-if is rechecked here,
    // same shape Gingerbread Cabin's own EtbControlsOtherSubtypeCount
    // pair uses.
    EffectOp::Conditional {
        cond: EffectCond::ControllerGraveyardCreatureCardsAtLeast(3),
        then: Box::new(EffectOp::GainLife {
            player: PlayerRef::Controller,
            amount: 5,
        }),
        else_: Box::new(EffectOp::Sequence(Vec::new())),
    }
}

fn glint_hawk_effect() -> EffectOp {
    // "When Glint Hawk enters the battlefield, sacrifice it unless you
    // return an artifact you control to its owner's hand."
    EffectOp::MayPayCostThen {
        discard: 0,
        sacrifice_lands: 0,
        return_permanent: Some(crate::card_def::PermanentFilterDef::Artifact),
        then: Box::new(EffectOp::Sequence(Vec::new())),
        otherwise: Some(Box::new(EffectOp::Sacrifice {
            object: ObjectRef::ThisSource,
        })),
    }
}

fn gatecreeper_vine_effect() -> EffectOp {
    EffectOp::SearchLibraryToHand {
        player: PlayerRef::Controller,
        filter: crate::effect::LibraryCardFilter::BasicLandOrGate,
    }
}

fn balustrade_spy_effect() -> EffectOp {
    EffectOp::RevealUntilCardTypeAndMill {
        player: PlayerRef::Target(0),
        card_type: CardType::Land,
    }
}

fn lotleth_giant_effect() -> EffectOp {
    EffectOp::DealDamageDynamic {
        target: TargetRef::Target(0),
        amount: DynamicValueDef::ControllerGraveyardCardsWithType(CardType::Creature),
    }
}

fn mesmeric_fiend_exile_effect() -> EffectOp {
    EffectOp::RevealHandChooseNonlandToLinkedExile {
        player: PlayerRef::Target(0),
    }
}

fn mesmeric_fiend_return_effect() -> EffectOp {
    EffectOp::ReturnLinkedExiledCardToOwnersHand
}

fn sneaky_snacker_effect() -> EffectOp {
    // Return Sneaky Snacker from your graveyard to the battlefield tapped.
    EffectOp::Sequence(vec![
        EffectOp::MoveObject {
            object: crate::effect::ObjectRef::ThisSource,
            to_zone: Zone::Battlefield,
        },
        EffectOp::TapObject {
            object: crate::effect::ObjectRef::ThisSource,
        },
    ])
}

const GUTTERSNIPE_TRIGGERS: [TriggeredAbilityDef; 1] = [TriggeredAbilityDef {
    condition: TriggerCondition::CastInstantOrSorcery,
    home_zone: Zone::Battlefield,
    intervening_if_kicked: false,
    intervening_if_controls_another_source_card: false,
    face_index: 0,
    effect: guttersnipe_effect,
}];
const MURMURING_MYSTIC_TRIGGERS: [TriggeredAbilityDef; 1] = [TriggeredAbilityDef {
    condition: TriggerCondition::CastInstantOrSorcery,
    home_zone: Zone::Battlefield,
    intervening_if_kicked: false,
    intervening_if_controls_another_source_card: false,
    face_index: 0,
    effect: murmuring_mystic_effect,
}];
const VOLDAREN_EPICURE_TRIGGERS: [TriggeredAbilityDef; 1] = [TriggeredAbilityDef {
    condition: TriggerCondition::Etb,
    home_zone: Zone::Battlefield,
    intervening_if_kicked: false,
    intervening_if_controls_another_source_card: false,
    face_index: 0,
    effect: voldaren_epicure_effect,
}];
const GENEROUS_ENT_TRIGGERS: [TriggeredAbilityDef; 1] = [TriggeredAbilityDef {
    condition: TriggerCondition::Etb,
    home_zone: Zone::Battlefield,
    intervening_if_kicked: false,
    intervening_if_controls_another_source_card: false,
    face_index: 0,
    effect: generous_ent_effect,
}];
const GINGERBREAD_CABIN_TRIGGERS: [TriggeredAbilityDef; 1] = [TriggeredAbilityDef {
    condition: TriggerCondition::EtbControlsOtherSubtypeCount {
        subtype: Subtype::Forest,
        minimum_count: 3,
    },
    home_zone: Zone::Battlefield,
    intervening_if_kicked: false,
    intervening_if_controls_another_source_card: false,
    face_index: 0,
    effect: gingerbread_cabin_effect,
}];
const WRITHING_CHRYSALIS_TRIGGERS: [TriggeredAbilityDef; 2] = [
    TriggeredAbilityDef {
        condition: TriggerCondition::CastSelf,
        home_zone: Zone::Stack,
        intervening_if_kicked: false,
        intervening_if_controls_another_source_card: false,
        face_index: 0,
        effect: writhing_chrysalis_cast_effect,
    },
    TriggeredAbilityDef {
        condition: TriggerCondition::SacrificeAnotherWithSubtype(Subtype::Eldrazi),
        home_zone: Zone::Battlefield,
        intervening_if_kicked: false,
        intervening_if_controls_another_source_card: false,
        face_index: 0,
        effect: writhing_chrysalis_counter_marker_effect,
    },
];
const BLOOD_FOUNTAIN_TRIGGERS: [TriggeredAbilityDef; 1] = [TriggeredAbilityDef {
    condition: TriggerCondition::Etb,
    home_zone: Zone::Battlefield,
    intervening_if_kicked: false,
    intervening_if_controls_another_source_card: false,
    face_index: 0,
    effect: blood_fountain_effect,
}];
const SAGU_WILDLING_TRIGGERS: [TriggeredAbilityDef; 1] = [TriggeredAbilityDef {
    condition: TriggerCondition::Etb,
    home_zone: Zone::Battlefield,
    intervening_if_kicked: false,
    intervening_if_controls_another_source_card: false,
    face_index: 0,
    effect: gain_three_life_effect,
}];
/// Prowess: whenever you cast a noncreature spell, this creature gets +1/+1
/// until end of turn.
const PROWESS_TRIGGER: TriggeredAbilityDef = TriggeredAbilityDef {
    condition: TriggerCondition::CastNoncreatureSpell,
    ..etb_trigger(prowess_effect)
};

/// Haste, prowess, and Valiant: exile the top card of your library; until
/// end of turn, you may play it.
const EMBERHEART_CHALLENGER_TRIGGERS: [TriggeredAbilityDef; 2] = [
    PROWESS_TRIGGER,
    TriggeredAbilityDef {
        condition: TriggerCondition::BecomesTargetOfControllerSpellOrAbilityFirstTimeEachTurn,
        ..etb_trigger(experimental_synthesizer_impulse_effect)
    },
];

/// Warp's "at the beginning of the next end step, exile this creature".
const WARP_EXILE_TRIGGER: TriggeredAbilityDef = TriggeredAbilityDef {
    condition: TriggerCondition::BeginningEndStepAfterWarp,
    ..etb_trigger(warp_exile_effect)
};

fn warp_exile_effect() -> EffectOp {
    EffectOp::BindWarpExileToTriggerSource
}

/// When it enters or transforms into Brutal Cathar, exile target creature an
/// opponent controls until it leaves the battlefield (Journey to Nowhere's
/// linked exile). Daybound; Moonrage Brute is its nightbound back face.
const BRUTAL_CATHAR_TRIGGERS: [TriggeredAbilityDef; 3] = [
    etb_trigger(journey_to_nowhere_etb_effect),
    TriggeredAbilityDef {
        condition: TriggerCondition::TransformsIntoFrontFace,
        ..etb_trigger(journey_to_nowhere_etb_effect)
    },
    TriggeredAbilityDef {
        condition: TriggerCondition::LeftBattlefield,
        home_zone: Zone::Graveyard,
        ..etb_trigger(journey_to_nowhere_ltb_effect)
    },
];

fn knight_errant_of_eos_effect() -> EffectOp {
    EffectOp::BindConvokedCreatureCountToLookTop {
        count: 6,
        max_taken: 2,
    }
}

/// Convoke. When it enters, look at the top six cards, reveal up to two
/// creature cards with mana value X or less (X = creatures that convoked
/// it), put them into hand, then shuffle.
const KNIGHT_ERRANT_OF_EOS_TRIGGERS: [TriggeredAbilityDef; 1] =
    [etb_trigger(knight_errant_of_eos_effect)];

const MONASTERY_SWIFTSPEAR_TRIGGERS: [TriggeredAbilityDef; 1] = [PROWESS_TRIGGER];

fn valiant_counter_effect() -> EffectOp {
    EffectOp::BindPlusOnePlusOneCounterToTriggerSource
}

fn heartfire_hero_dies_effect() -> EffectOp {
    EffectOp::BindDamageOpponentEqualToSourceLastPower
}

/// Valiant: a +1/+1 counter on it. When it dies, it deals damage equal to
/// its power to each opponent.
const HEARTFIRE_HERO_TRIGGERS: [TriggeredAbilityDef; 2] = [
    TriggeredAbilityDef {
        condition: TriggerCondition::BecomesTargetOfControllerSpellOrAbilityFirstTimeEachTurn,
        ..etb_trigger(valiant_counter_effect)
    },
    TriggeredAbilityDef {
        condition: TriggerCondition::LeftBattlefieldToGraveyard,
        home_zone: Zone::Graveyard,
        ..etb_trigger(heartfire_hero_dies_effect)
    },
];

fn slickshot_show_off_effect() -> EffectOp {
    EffectOp::BindTemporaryBoostToTriggerSource {
        power: 2,
        toughness: 0,
    }
}

/// Flying, haste. Whenever you cast a noncreature spell, +2/+0 until end of
/// turn. Plot {1}{R}.
const SLICKSHOT_SHOW_OFF_TRIGGERS: [TriggeredAbilityDef; 1] = [TriggeredAbilityDef {
    condition: TriggerCondition::CastNoncreatureSpell,
    ..etb_trigger(slickshot_show_off_effect)
}];

fn battle_cry_effect() -> EffectOp {
    EffectOp::PumpOtherAttackingCreaturesUntilEndOfTurn {
        power: 1,
        toughness: 0,
    }
}

fn bat_token_effect() -> EffectOp {
    EffectOp::CreateToken {
        token_def: crate::card_def::card_id_by_name("Bat Token").expect("Bat Token in CARD_DEFS"),
        controller: PlayerRef::Controller,
    }
}

/// Battle cry. When it enters or dies, create a 1/1 black Bat creature
/// token with flying.
const SANGUINE_EVANGELIST_TRIGGERS: [TriggeredAbilityDef; 3] = [
    TriggeredAbilityDef {
        condition: TriggerCondition::Attacks,
        ..etb_trigger(battle_cry_effect)
    },
    etb_trigger(bat_token_effect),
    TriggeredAbilityDef {
        condition: TriggerCondition::LeftBattlefieldToGraveyard,
        home_zone: Zone::Graveyard,
        ..etb_trigger(bat_token_effect)
    },
];

fn darkstar_augur_offspring_effect() -> EffectOp {
    let token_def = crate::card_def::card_id_by_name("Darkstar Augur Offspring Token")
        .expect("Darkstar Augur Offspring Token in CARD_DEFS");
    EffectOp::Conditional {
        cond: EffectCond::WasKicked,
        then: Box::new(EffectOp::CreateToken {
            token_def,
            controller: PlayerRef::Controller,
        }),
        else_: Box::new(EffectOp::Sequence(vec![])),
    }
}

fn darkstar_augur_upkeep_effect() -> EffectOp {
    EffectOp::RevealTopCardToHandLoseLifeEqualToManaValue
}

/// Offspring {B}, flying. At the beginning of your upkeep, reveal the top
/// card of your library and put it into your hand; you lose life equal to
/// its mana value.
const DARKSTAR_AUGUR_TRIGGERS: [TriggeredAbilityDef; 2] = [
    TriggeredAbilityDef {
        intervening_if_kicked: true,
        ..etb_trigger(darkstar_augur_offspring_effect)
    },
    TriggeredAbilityDef {
        condition: TriggerCondition::BeginningOfUpkeep {
            controller_only: true,
        },
        ..etb_trigger(darkstar_augur_upkeep_effect)
    },
];

#[cfg(feature = "standard-magezero-fixtures")]
fn controller_descended_this_turn(state: &GameState, controller: PlayerId) -> bool {
    crate::standard_keywords_v1::descended_this_turn(state, controller)
}

#[cfg(not(feature = "standard-magezero-fixtures"))]
fn controller_descended_this_turn(_state: &GameState, _controller: PlayerId) -> bool {
    false
}

#[cfg(feature = "standard-magezero-fixtures")]
fn source_was_creature_before_leaving(state: &GameState, source: ObjectId) -> bool {
    let current = state.objects.get(source).zone_change_count;
    current > 0
        && crate::standard_keywords_v1::was_creature_before_leaving(state, source, current - 1)
}

#[cfg(not(feature = "standard-magezero-fixtures"))]
fn source_was_creature_before_leaving(_state: &GameState, _source: ObjectId) -> bool {
    false
}

fn scry_one_effect() -> EffectOp {
    EffectOp::Scry {
        player: PlayerRef::Controller,
        count: 1,
    }
}

/// Flying, lifelink. At the beginning of your end step, if you descended
/// this turn, scry 1.
const RUIN_LURKER_BAT_TRIGGERS: [TriggeredAbilityDef; 1] = [TriggeredAbilityDef {
    condition: TriggerCondition::BeginningControllerEndStepIfDescended,
    ..etb_trigger(scry_one_effect)
}];

fn pawpatch_recruit_offspring_effect() -> EffectOp {
    let token_def = crate::card_def::card_id_by_name("Pawpatch Recruit Offspring Token")
        .expect("Pawpatch Recruit Offspring Token in CARD_DEFS");
    EffectOp::Conditional {
        cond: EffectCond::WasKicked,
        then: Box::new(EffectOp::CreateToken {
            token_def,
            controller: PlayerRef::Controller,
        }),
        else_: Box::new(EffectOp::Sequence(vec![])),
    }
}

fn pawpatch_recruit_targeted_effect() -> EffectOp {
    EffectOp::BindPlusOneCounterOnAnotherTargetToTriggerTarget
}

/// Offspring {2}, trample. Whenever a creature you control becomes the
/// target of a spell or ability an opponent controls, put a +1/+1 counter
/// on target creature you control other than that creature.
const PAWPATCH_RECRUIT_TRIGGERS: [TriggeredAbilityDef; 2] = [
    TriggeredAbilityDef {
        intervening_if_kicked: true,
        ..etb_trigger(pawpatch_recruit_offspring_effect)
    },
    TriggeredAbilityDef {
        condition: TriggerCondition::ControlledCreatureBecomesTargetOfOpponent,
        ..etb_trigger(pawpatch_recruit_targeted_effect)
    },
];

fn manifold_mouse_offspring_effect() -> EffectOp {
    let token_def = crate::card_def::card_id_by_name("Manifold Mouse Offspring Token")
        .expect("Manifold Mouse Offspring Token in CARD_DEFS");
    EffectOp::Conditional {
        cond: EffectCond::WasKicked,
        then: Box::new(EffectOp::CreateToken {
            token_def,
            controller: PlayerRef::Controller,
        }),
        else_: Box::new(EffectOp::Sequence(vec![])),
    }
}

fn manifold_mouse_combat_effect() -> EffectOp {
    EffectOp::Choice {
        controller: PlayerRef::Controller,
        options: vec![
            EffectOp::GrantKeywordTargetUntilEndOfTurn {
                object: ObjectRef::Target(0),
                keyword: Keywords::DOUBLE_STRIKE,
            },
            EffectOp::GrantKeywordTargetUntilEndOfTurn {
                object: ObjectRef::Target(0),
                keyword: Keywords::TRAMPLE,
            },
        ],
    }
}

/// Offspring {2}. At the beginning of combat on your turn, target Mouse you
/// control gains your choice of double strike or trample until end of turn.
const MANIFOLD_MOUSE_TRIGGERS: [TriggeredAbilityDef; 2] = [
    TriggeredAbilityDef {
        intervening_if_kicked: true,
        ..etb_trigger(manifold_mouse_offspring_effect)
    },
    TriggeredAbilityDef {
        condition: TriggerCondition::BeginningOfControllerCombat,
        ..etb_trigger(manifold_mouse_combat_effect)
    },
];

fn yotian_frontliner_attack_effect() -> EffectOp {
    EffectOp::PumpTargetUntilEndOfTurnDynamic {
        target: TargetRef::Target(0),
        power: DynamicValueDef::Fixed(1),
        toughness: DynamicValueDef::Fixed(1),
    }
}

/// Whenever it attacks, another target creature you control gets +1/+1
/// until end of turn. Unearth {W} (a graveyard activated ability; its exile
/// at the next end step shares warp's delayed exile).
const YOTIAN_FRONTLINER_TRIGGERS: [TriggeredAbilityDef; 2] = [
    TriggeredAbilityDef {
        condition: TriggerCondition::Attacks,
        ..etb_trigger(yotian_frontliner_attack_effect)
    },
    TriggeredAbilityDef {
        condition: TriggerCondition::BeginningEndStepAfterUnearth,
        ..etb_trigger(warp_exile_effect)
    },
];

fn cori_steel_cutter_flurry_effect() -> EffectOp {
    let monk = crate::card_def::card_id_by_name("Monk Token").expect("Monk Token in CARD_DEFS");
    // "You may attach this Equipment to it": the choice is made as the
    // ability resolves, before the token exists, which no information
    // separates from choosing just after.
    EffectOp::Choice {
        controller: PlayerRef::Controller,
        options: vec![
            EffectOp::CreateTokenAndAttachSource { token_def: monk },
            EffectOp::CreateToken {
                token_def: monk,
                controller: PlayerRef::Controller,
            },
        ],
    }
}

/// Equipped creature gets +1/+1 and has trample and haste. Flurry: whenever
/// you cast your second spell each turn, create a 1/1 white Monk with
/// prowess; you may attach this Equipment to it. Equip {1}{R}.
const CORI_STEEL_CUTTER_TRIGGERS: [TriggeredAbilityDef; 1] = [TriggeredAbilityDef {
    condition: TriggerCondition::ControllerCastsSecondSpellEachTurn,
    ..etb_trigger(cori_steel_cutter_flurry_effect)
}];

fn graveyard_trespasser_effect() -> EffectOp {
    EffectOp::ExileGraveyardTargetsDrainPerCreature { max_targets: 1 }
}

fn graveyard_glutton_effect() -> EffectOp {
    EffectOp::ExileGraveyardTargetsDrainPerCreature { max_targets: 2 }
}

/// Ward—Discard a card (both faces). Whenever it enters or attacks, exile
/// up to one target card from a graveyard; if a creature card was exiled,
/// drain 1. Daybound. Graveyard Glutton (4/4, nightbound): the same on
/// entering or attacking with up to two cards, draining 1 per creature card.
const GRAVEYARD_TRESPASSER_TRIGGERS: [TriggeredAbilityDef; 4] = [
    etb_trigger(graveyard_trespasser_effect),
    TriggeredAbilityDef {
        condition: TriggerCondition::Attacks,
        ..etb_trigger(graveyard_trespasser_effect)
    },
    TriggeredAbilityDef {
        face_index: 1,
        ..etb_trigger(graveyard_glutton_effect)
    },
    TriggeredAbilityDef {
        condition: TriggerCondition::Attacks,
        face_index: 1,
        ..etb_trigger(graveyard_glutton_effect)
    },
];

fn overlord_of_the_mistmoors_effect() -> EffectOp {
    let insect = crate::card_def::card_id_by_name("White Insect Token")
        .expect("White Insect Token in CARD_DEFS");
    EffectOp::Sequence(vec![
        EffectOp::CreateToken {
            token_def: insect,
            controller: PlayerRef::Controller,
        },
        EffectOp::CreateToken {
            token_def: insect,
            controller: PlayerRef::Controller,
        },
    ])
}

fn remove_time_counter_effect() -> EffectOp {
    EffectOp::RemoveTimeCounterFromSource
}

/// Impending 4—{2}{W}{W}. Whenever it enters or attacks, create two 2/1
/// white Insect creature tokens with flying.
const OVERLORD_OF_THE_MISTMOORS_TRIGGERS: [TriggeredAbilityDef; 3] = [
    etb_trigger(overlord_of_the_mistmoors_effect),
    TriggeredAbilityDef {
        condition: TriggerCondition::Attacks,
        ..etb_trigger(overlord_of_the_mistmoors_effect)
    },
    TriggeredAbilityDef {
        condition: TriggerCondition::BeginningControllerEndStepWithTimeCounter,
        ..etb_trigger(remove_time_counter_effect)
    },
];

fn enduring_return_effect() -> EffectOp {
    EffectOp::ReturnSourceAsEnduringEnchantment
}

/// "When this dies, if it was a creature, return it to the battlefield
/// under its owner's control. It's an enchantment."
const ENDURING_RETURN_TRIGGER: TriggeredAbilityDef = TriggeredAbilityDef {
    condition: TriggerCondition::DiesIfWasCreature,
    home_zone: Zone::Graveyard,
    ..etb_trigger(enduring_return_effect)
};

fn draw_one_effect() -> EffectOp {
    EffectOp::DrawCards {
        player: PlayerRef::Controller,
        count: 1,
    }
}

/// Flash. Whenever a creature you control deals combat damage to a player,
/// draw a card. Enduring.
const ENDURING_CURIOSITY_TRIGGERS: [TriggeredAbilityDef; 2] = [
    TriggeredAbilityDef {
        condition: TriggerCondition::ControlledCreatureDealsCombatDamageToPlayer,
        ..etb_trigger(draw_one_effect)
    },
    ENDURING_RETURN_TRIGGER,
];

/// Lifelink. Whenever one or more other creatures you control with power 2
/// or less enter, draw a card (once each turn). Enduring.
const ENDURING_INNOCENCE_TRIGGERS: [TriggeredAbilityDef; 2] = [
    TriggeredAbilityDef {
        condition: TriggerCondition::OtherControlledCreatureWithPowerAtMostEntersOncePerTurn(2),
        ..etb_trigger(draw_one_effect)
    },
    ENDURING_RETURN_TRIGGER,
];

fn chrome_host_seedshark_effect() -> EffectOp {
    EffectOp::BindIncubateToTriggerSpell
}

/// Flying. Whenever you cast a noncreature spell, incubate X, where X is
/// that spell's mana value.
const CHROME_HOST_SEEDSHARK_TRIGGERS: [TriggeredAbilityDef; 1] = [TriggeredAbilityDef {
    condition: TriggerCondition::CastNoncreatureSpell,
    ..etb_trigger(chrome_host_seedshark_effect)
}];

fn training_effect() -> EffectOp {
    EffectOp::PutPlusOnePlusOneCounter {
        object: ObjectRef::ThisSource,
    }
}

/// Training. ({2}{W}, remove two +1/+1 counters: destroy target artifact
/// or enchantment is an activated ability.)
const HOPEFUL_INITIATE_TRIGGERS: [TriggeredAbilityDef; 1] = [TriggeredAbilityDef {
    condition: TriggerCondition::AttacksWithGreaterPowerAttacker,
    ..etb_trigger(training_effect)
}];

fn forsaken_miner_crime_effect() -> EffectOp {
    // The optional payment must be the program root; the return re-checks
    // that the card is still the graveyard incarnation that triggered.
    EffectOp::MayPayManaThen {
        player: PlayerRef::Controller,
        colored: vec![crate::mana::ManaColor::B],
        generic: 0,
        then: Box::new(EffectOp::Conditional {
            cond: EffectCond::SourceStillInTriggerZone,
            then: Box::new(EffectOp::MoveObject {
                object: ObjectRef::ThisSource,
                to_zone: Zone::Battlefield,
            }),
            else_: Box::new(EffectOp::Sequence(vec![])),
        }),
    }
}

/// Can't block (`standard_keywords_v1::cant_block`). Whenever you commit a
/// crime, you may pay {B}; if you do, return it from your graveyard to the
/// battlefield.
const FORSAKEN_MINER_TRIGGERS: [TriggeredAbilityDef; 1] = [TriggeredAbilityDef {
    condition: TriggerCondition::ControllerCommitsCrime,
    home_zone: Zone::Graveyard,
    ..etb_trigger(forsaken_miner_crime_effect)
}];

fn aloe_alchemist_plotted_effect() -> EffectOp {
    EffectOp::Sequence(vec![
        EffectOp::PumpTargetUntilEndOfTurnDynamic {
            target: TargetRef::Target(0),
            power: DynamicValueDef::Fixed(3),
            toughness: DynamicValueDef::Fixed(2),
        },
        EffectOp::GrantKeywordTargetUntilEndOfTurn {
            object: ObjectRef::Target(0),
            keyword: Keywords::TRAMPLE,
        },
    ])
}

/// When it becomes plotted, target creature gets +3/+2 and gains trample
/// until end of turn. Plot {1}{G}.
const ALOE_ALCHEMIST_TRIGGERS: [TriggeredAbilityDef; 1] = [TriggeredAbilityDef {
    condition: TriggerCondition::BecomesPlotted,
    home_zone: Zone::Exile,
    ..etb_trigger(aloe_alchemist_plotted_effect)
}];

fn iridescent_vinelasher_offspring_effect() -> EffectOp {
    let token_def = crate::card_def::card_id_by_name("Iridescent Vinelasher Offspring Token")
        .expect("Iridescent Vinelasher Offspring Token in CARD_DEFS");
    EffectOp::Conditional {
        cond: EffectCond::WasKicked,
        then: Box::new(EffectOp::CreateToken {
            token_def,
            controller: PlayerRef::Controller,
        }),
        else_: Box::new(EffectOp::Sequence(vec![])),
    }
}

fn iridescent_vinelasher_landfall_effect() -> EffectOp {
    EffectOp::DealDamage {
        target: TargetRef::Target(0),
        amount: 1,
    }
}

/// Offspring {2} (kicker's optional cost): when it enters, if the offspring
/// cost was paid, create a 1/1 token copy of it. The copy is a separate
/// 1/1 token definition with the same name, types and landfall ability.
/// Landfall: 1 damage to target opponent.
const IRIDESCENT_VINELASHER_TRIGGERS: [TriggeredAbilityDef; 2] = [
    TriggeredAbilityDef {
        intervening_if_kicked: true,
        ..etb_trigger(iridescent_vinelasher_offspring_effect)
    },
    TriggeredAbilityDef {
        condition: TriggerCondition::ControlledLandEnters,
        ..etb_trigger(iridescent_vinelasher_landfall_effect)
    },
];

fn nova_hellkite_etb_effect() -> EffectOp {
    EffectOp::DealDamage {
        target: TargetRef::Target(0),
        amount: 1,
    }
}

/// Flying, haste; when it enters, 1 damage to target creature an opponent
/// controls; Warp {2}{R}.
const NOVA_HELLKITE_TRIGGERS: [TriggeredAbilityDef; 2] =
    [etb_trigger(nova_hellkite_etb_effect), WARP_EXILE_TRIGGER];

const KESSIG_FLAMEBREATHER_TRIGGERS: [TriggeredAbilityDef; 1] = [TriggeredAbilityDef {
    condition: TriggerCondition::CastNoncreatureSpell,
    home_zone: Zone::Battlefield,
    intervening_if_kicked: false,
    intervening_if_controls_another_source_card: false,
    face_index: 0,
    effect: kessig_flamebreather_effect,
}];

fn balmor_cast_effect() -> EffectOp {
    EffectOp::BoostControlledCreaturesUntilEndOfTurn {
        power: 1,
        toughness: 0,
        keywords: Keywords::TRAMPLE,
    }
}

const BALMOR_TRIGGERS: [TriggeredAbilityDef; 1] = [TriggeredAbilityDef {
    condition: TriggerCondition::CastInstantOrSorcery,
    ..etb_trigger(balmor_cast_effect)
}];

const FIRESPITTER_WHELP_TRIGGERS: [TriggeredAbilityDef; 1] = [TriggeredAbilityDef {
    condition: TriggerCondition::CastNoncreatureOrSubtype(Subtype::Dragon),
    ..etb_trigger(kessig_flamebreather_effect)
}];
const GIXIAN_INFILTRATOR_TRIGGERS: [TriggeredAbilityDef; 1] = [TriggeredAbilityDef {
    condition: TriggerCondition::SacrificeAnotherPermanent,
    home_zone: Zone::Battlefield,
    intervening_if_kicked: false,
    intervening_if_controls_another_source_card: false,
    face_index: 0,
    effect: gixian_infiltrator_effect,
}];
const WEBWEAVER_CHANGELING_TRIGGERS: [TriggeredAbilityDef; 1] = [TriggeredAbilityDef {
    condition: TriggerCondition::EtbIfGraveyardCreatureCardsAtLeast(3),
    home_zone: Zone::Battlefield,
    intervening_if_kicked: false,
    intervening_if_controls_another_source_card: false,
    face_index: 0,
    effect: webweaver_changeling_effect,
}];
const GLINT_HAWK_TRIGGERS: [TriggeredAbilityDef; 1] = [TriggeredAbilityDef {
    condition: TriggerCondition::Etb,
    home_zone: Zone::Battlefield,
    intervening_if_kicked: false,
    intervening_if_controls_another_source_card: false,
    face_index: 0,
    effect: glint_hawk_effect,
}];
const GATECREEPER_VINE_TRIGGERS: [TriggeredAbilityDef; 1] = [TriggeredAbilityDef {
    condition: TriggerCondition::Etb,
    home_zone: Zone::Battlefield,
    intervening_if_kicked: false,
    intervening_if_controls_another_source_card: false,
    face_index: 0,
    effect: gatecreeper_vine_effect,
}];
const BALUSTRADE_SPY_TRIGGERS: [TriggeredAbilityDef; 1] = [TriggeredAbilityDef {
    condition: TriggerCondition::Etb,
    home_zone: Zone::Battlefield,
    intervening_if_kicked: false,
    intervening_if_controls_another_source_card: false,
    face_index: 0,
    effect: balustrade_spy_effect,
}];
const LOTLETH_GIANT_TRIGGERS: [TriggeredAbilityDef; 1] = [TriggeredAbilityDef {
    condition: TriggerCondition::Etb,
    home_zone: Zone::Battlefield,
    intervening_if_kicked: false,
    intervening_if_controls_another_source_card: false,
    face_index: 0,
    effect: lotleth_giant_effect,
}];
const MESMERIC_FIEND_TRIGGERS: [TriggeredAbilityDef; 2] = [
    TriggeredAbilityDef {
        condition: TriggerCondition::Etb,
        home_zone: Zone::Battlefield,
        intervening_if_kicked: false,
        intervening_if_controls_another_source_card: false,
        face_index: 0,
        effect: mesmeric_fiend_exile_effect,
    },
    TriggeredAbilityDef {
        condition: TriggerCondition::LeftBattlefield,
        // Leave triggers are matched from battlefield last-known
        // information and therefore ignore the source's post-event zone.
        home_zone: Zone::Graveyard,
        intervening_if_kicked: false,
        intervening_if_controls_another_source_card: false,
        face_index: 0,
        effect: mesmeric_fiend_return_effect,
    },
];
const GAIN_THREE_LIFE_ETB_TRIGGERS: [TriggeredAbilityDef; 1] = [TriggeredAbilityDef {
    condition: TriggerCondition::Etb,
    home_zone: Zone::Battlefield,
    intervening_if_kicked: false,
    intervening_if_controls_another_source_card: false,
    face_index: 0,
    effect: gain_three_life_effect,
}];
const SNEAKY_SNACKER_TRIGGERS: [TriggeredAbilityDef; 1] = [TriggeredAbilityDef {
    condition: TriggerCondition::DrawNth(3),
    home_zone: Zone::Graveyard,
    intervening_if_kicked: false,
    intervening_if_controls_another_source_card: false,
    face_index: 0,
    effect: sneaky_snacker_effect,
}];

fn burning_tree_emissary_effect() -> EffectOp {
    // When Burning-Tree Emissary enters the battlefield, add {R}{G}.
    EffectOp::AddMana {
        player: PlayerRef::Controller,
        colors: vec![crate::mana::ManaColor::R, crate::mana::ManaColor::G],
    }
}

fn clockwork_percussionist_dies_effect() -> EffectOp {
    // When Clockwork Percussionist dies, exile the top card of your
    // library. You may play it until the end of your next turn.
    EffectOp::ImpulseDraw {
        count: 1,
        duration: crate::effect::ImpulseDuration::UntilOwnersNextTurn,
    }
}

fn ichor_wellspring_draw_effect() -> EffectOp {
    EffectOp::DrawCards {
        player: PlayerRef::Controller,
        count: 1,
    }
}

fn job_select_effect() -> EffectOp {
    let hero = crate::card_def::card_id_by_name("Hero Token").expect("Hero Token in CARD_DEFS");
    EffectOp::CreateTokenAndAttachSource { token_def: hero }
}

fn nihil_spellbomb_graveyard_effect() -> EffectOp {
    EffectOp::MayPayManaThen {
        player: PlayerRef::Controller,
        colored: vec![crate::mana::ManaColor::B],
        generic: 0,
        then: Box::new(EffectOp::DrawCards {
            player: PlayerRef::Controller,
            count: 1,
        }),
    }
}

fn faerie_miscreant_effect() -> EffectOp {
    EffectOp::Conditional {
        cond: EffectCond::ControlsAnotherSourceCard,
        then: Box::new(EffectOp::DrawCards {
            player: PlayerRef::Controller,
            count: 1,
        }),
        else_: Box::new(EffectOp::Sequence(vec![])),
    }
}

fn faerie_seer_effect() -> EffectOp {
    EffectOp::Scry {
        player: PlayerRef::Controller,
        count: 2,
    }
}

fn outlaw_medic_dies_effect() -> EffectOp {
    EffectOp::DrawCards {
        player: PlayerRef::Controller,
        count: 1,
    }
}

fn adventuring_gear_landfall_effect() -> EffectOp {
    // "Landfall -- Whenever a land you control enters, equipped creature
    // gets +2/+2 until end of turn."
    EffectOp::BoostAttachedCreatureUntilEndOfTurn {
        power: 2,
        toughness: 2,
    }
}

fn goldvein_pick_combat_damage_effect() -> EffectOp {
    // "Whenever equipped creature deals combat damage to a player, create a
    // Treasure token."
    EffectOp::CreateToken {
        token_def: crate::card_def::card_id_by_name("Treasure Token")
            .expect("Treasure Token in CARD_DEFS"),
        controller: PlayerRef::Controller,
    }
}

fn solemn_simulacrum_etb_effect() -> EffectOp {
    // "When this creature enters, you may search your library for a basic
    // land card, put that card onto the battlefield tapped, then shuffle."
    // The zero-card selection is the "may".
    EffectOp::SearchLibraryToBattlefieldTapped {
        player: PlayerRef::Controller,
        filter: crate::effect::LibraryCardFilter::BasicLand,
    }
}

fn campus_guide_etb_effect() -> EffectOp {
    // "When this creature enters, you may search your library for a basic
    // land card, reveal it, then shuffle and put that card on top." The
    // zero-card selection is the "may".
    EffectOp::SearchLibraryCardsToDestination {
        player: PlayerRef::Controller,
        filter: crate::effect::LibraryCardFilter::BasicLand,
        max_targets: 1,
        destination: crate::effect::LibrarySearchDestinationV1::LibraryTopAfterShuffle,
    }
}

fn solemn_simulacrum_dies_effect() -> EffectOp {
    // "When this creature dies, you may draw a card."
    EffectOp::Choice {
        controller: PlayerRef::Controller,
        options: vec![
            EffectOp::Sequence(vec![]),
            EffectOp::DrawCards {
                player: PlayerRef::Controller,
                count: 1,
            },
        ],
    }
}

fn refurbished_familiar_etb_effect() -> EffectOp {
    // The kernel is strictly 1v1. The opponent chooses and discards one
    // card when possible; otherwise the Familiar's controller draws one.
    EffectOp::Conditional {
        cond: EffectCond::OpponentHasCardsInHand,
        then: Box::new(EffectOp::DiscardCards {
            player: PlayerRef::Opponent,
            count: 1,
        }),
        else_: Box::new(EffectOp::DrawCards {
            player: PlayerRef::Controller,
            count: 1,
        }),
    }
}

fn squadron_hawk_etb_effect() -> EffectOp {
    let hawk =
        crate::card_def::card_id_by_name("Squadron Hawk").expect("Squadron Hawk in CARD_DEFS");
    EffectOp::SearchLibraryToHandUpTo {
        player: PlayerRef::Controller,
        filter: crate::effect::LibraryCardFilter::CardDefinition(hawk),
        max_targets: 3,
    }
}

fn bind_the_monster_etb_effect() -> EffectOp {
    EffectOp::TapAttachedCreatureAndDamageControllerByPower
}

fn harrier_strix_etb_effect() -> EffectOp {
    EffectOp::TapObject {
        object: ObjectRef::Target(0),
    }
}

/// Bojuka Bog: "When Bojuka Bog enters the battlefield, exile all cards
/// from target player's graveyard." Reuses `EffectOp::ExilePlayersGraveyard`
/// (Nihil Spellbomb's activated-ability effect).
fn bojuka_bog_etb_effect() -> EffectOp {
    EffectOp::ExilePlayersGraveyard {
        player: PlayerRef::Target(0),
    }
}

/// Conduit Pylons: "When Conduit Pylons enters the battlefield, surveil 1."
fn conduit_pylons_etb_effect() -> EffectOp {
    EffectOp::Surveil {
        player: PlayerRef::Controller,
        count: 1,
    }
}

fn lightshell_duo_etb_effect() -> EffectOp {
    EffectOp::Surveil {
        player: PlayerRef::Controller,
        count: 2,
    }
}

fn cephalid_inkmage_etb_effect() -> EffectOp {
    EffectOp::Surveil {
        player: PlayerRef::Controller,
        count: 3,
    }
}

fn humbling_elder_etb_effect() -> EffectOp {
    EffectOp::PumpTargetUntilEndOfTurnDynamic {
        target: TargetRef::Target(0),
        power: DynamicValueDef::Fixed(-2),
        toughness: DynamicValueDef::Fixed(0),
    }
}

/// Meteor Golem: "When this creature enters, destroy target nonland
/// permanent an opponent controls."
fn meteor_golem_etb_effect() -> EffectOp {
    EffectOp::Conditional {
        cond: EffectCond::TargetInZone(0, Zone::Battlefield),
        then: Box::new(EffectOp::DestroyObject {
            object: ObjectRef::Target(0),
        }),
        else_: Box::new(EffectOp::Sequence(vec![])),
    }
}

/// Reclamation Sage: "When this creature enters, you may destroy target
/// artifact or enchantment." The target is chosen when the trigger is put
/// on the stack; the controller decides whether to destroy on resolution,
/// with declining as the first printed option (Kiora's optional shape).
fn reclamation_sage_etb_effect() -> EffectOp {
    EffectOp::Choice {
        controller: PlayerRef::Controller,
        options: vec![EffectOp::Sequence(vec![]), meteor_golem_etb_effect()],
    }
}

fn moon_circuit_hacker_combat_effect_for_entered_this_turn(entered_this_turn: bool) -> EffectOp {
    let after_draw = if entered_this_turn {
        EffectOp::Sequence(vec![])
    } else {
        EffectOp::DiscardCards {
            player: PlayerRef::Controller,
            count: 1,
        }
    };
    EffectOp::Choice {
        controller: PlayerRef::Controller,
        options: vec![
            EffectOp::Sequence(vec![]),
            EffectOp::Sequence(vec![
                EffectOp::DrawCards {
                    player: PlayerRef::Controller,
                    count: 1,
                },
                after_draw,
            ]),
        ],
    }
}

fn moon_circuit_hacker_combat_effect() -> EffectOp {
    // Definition inventory placeholder. Event matching freezes the actual
    // source-incarnation fact into one of the two canonical programs below.
    moon_circuit_hacker_combat_effect_for_entered_this_turn(false)
}

fn ninja_of_the_deep_hours_combat_effect() -> EffectOp {
    EffectOp::Choice {
        controller: PlayerRef::Controller,
        options: vec![
            EffectOp::Sequence(vec![]),
            EffectOp::DrawCards {
                player: PlayerRef::Controller,
                count: 1,
            },
        ],
    }
}

fn saiba_cryptomancer_etb_effect() -> EffectOp {
    EffectOp::BackupTarget {
        target: ObjectRef::Target(0),
        keyword: Keywords::HEXPROOF,
    }
}

fn spellstutter_sprite_etb_effect() -> EffectOp {
    EffectOp::MoveObject {
        object: ObjectRef::Target(0),
        to_zone: Zone::Graveyard,
    }
}

fn lembas_etb_effect() -> EffectOp {
    EffectOp::Sequence(vec![
        EffectOp::Scry {
            player: PlayerRef::Controller,
            count: 1,
        },
        EffectOp::DrawCards {
            player: PlayerRef::Controller,
            count: 1,
        },
    ])
}

fn lembas_graveyard_effect() -> EffectOp {
    EffectOp::ShuffleTriggerSourceIntoOwnersLibrary
}

fn weather_the_storm_cast_effect() -> EffectOp {
    EffectOp::MaterializeStormCopies
}

fn journey_to_nowhere_etb_effect() -> EffectOp {
    EffectOp::ExileTargetLinkedToSource {
        object: crate::effect::ObjectRef::Target(0),
    }
}

fn journey_to_nowhere_ltb_effect() -> EffectOp {
    EffectOp::ReturnObjectsExiledBySource
}

fn masked_vandal_etb_effect() -> EffectOp {
    EffectOp::MayExileFromPlayersGraveyardMatchingThen {
        player: PlayerRef::Controller,
        card_type: CardType::Creature,
        then: Box::new(EffectOp::MoveObject {
            object: ObjectRef::Target(0),
            to_zone: Zone::Exile,
        }),
    }
}

fn troublemaker_ouphe_etb_effect() -> EffectOp {
    EffectOp::Conditional {
        cond: EffectCond::OptionalAdditionalCostPaid(OptionalAdditionalCostDef::Bargain),
        then: Box::new(EffectOp::MoveObject {
            object: ObjectRef::Target(0),
            to_zone: Zone::Exile,
        }),
        else_: Box::new(EffectOp::Sequence(Vec::new())),
    }
}

fn vitu_ghazi_inspector_etb_effect() -> EffectOp {
    EffectOp::Conditional {
        cond: EffectCond::OptionalAdditionalCostPaid(OptionalAdditionalCostDef::CollectEvidence {
            minimum_mana_value: 6,
        }),
        then: Box::new(EffectOp::Sequence(vec![
            EffectOp::PutPlusOnePlusOneCounter {
                object: ObjectRef::Target(0),
            },
            EffectOp::GainLife {
                player: PlayerRef::Controller,
                amount: 2,
            },
        ])),
        else_: Box::new(EffectOp::Sequence(Vec::new())),
    }
}

fn avenging_hunter_etb_effect() -> EffectOp {
    EffectOp::TakeInitiative {
        player: PlayerRef::Controller,
    }
}

fn azure_fleet_admiral_etb_effect() -> EffectOp {
    EffectOp::BecomeMonarch
}

fn delver_of_secrets_effect() -> EffectOp {
    // At the beginning of your upkeep, look at the top card of your
    // library. You may reveal that card. If an instant or sorcery card is
    // revealed this way, transform Delver of Secrets.
    EffectOp::LookAtTopMayRevealThen {
        predicate: CardTypePredicate::InstantOrSorcery,
        then: Box::new(EffectOp::TransformSourceInPlace),
    }
}

fn experimental_synthesizer_impulse_effect() -> EffectOp {
    // When Experimental Synthesizer enters or leaves the battlefield, exile
    // the top card of your library. Until end of turn, you may play that
    // card. Both triggers share this one effect -- the card's text is
    // identical either way.
    EffectOp::ImpulseDraw {
        count: 1,
        duration: crate::effect::ImpulseDuration::EndOfTurn,
    }
}

fn goblin_bushwhacker_effect() -> EffectOp {
    // When this creature enters, if it was kicked, creatures you control
    // get +1/+0 and gain haste until end of turn.
    EffectOp::Conditional {
        cond: EffectCond::WasKicked,
        then: Box::new(EffectOp::PumpControlled {
            filter: crate::effect::CreatureFilter::AnyControlled,
            power: 1,
            toughness: 0,
            grant_haste: true,
        }),
        else_: Box::new(EffectOp::Sequence(vec![])),
    }
}

const BURNING_TREE_EMISSARY_TRIGGERS: [TriggeredAbilityDef; 1] = [TriggeredAbilityDef {
    condition: TriggerCondition::Etb,
    home_zone: Zone::Battlefield,
    intervening_if_kicked: false,
    intervening_if_controls_another_source_card: false,
    face_index: 0,
    effect: burning_tree_emissary_effect,
}];
const CLOCKWORK_PERCUSSIONIST_TRIGGERS: [TriggeredAbilityDef; 1] = [TriggeredAbilityDef {
    condition: TriggerCondition::LeftBattlefieldToGraveyard,
    home_zone: Zone::Graveyard,
    intervening_if_kicked: false,
    intervening_if_controls_another_source_card: false,
    face_index: 0,
    effect: clockwork_percussionist_dies_effect,
}];
const ICHOR_WELLSPRING_TRIGGERS: [TriggeredAbilityDef; 2] = [
    TriggeredAbilityDef {
        condition: TriggerCondition::Etb,
        home_zone: Zone::Battlefield,
        intervening_if_kicked: false,
        intervening_if_controls_another_source_card: false,
        face_index: 0,
        effect: ichor_wellspring_draw_effect,
    },
    TriggeredAbilityDef {
        condition: TriggerCondition::LeftBattlefieldToGraveyard,
        home_zone: Zone::Graveyard,
        intervening_if_kicked: false,
        intervening_if_controls_another_source_card: false,
        face_index: 0,
        effect: ichor_wellspring_draw_effect,
    },
];
const CRYOGEN_RELIC_TRIGGERS: [TriggeredAbilityDef; 2] = [
    TriggeredAbilityDef {
        condition: TriggerCondition::Etb,
        home_zone: Zone::Battlefield,
        intervening_if_kicked: false,
        intervening_if_controls_another_source_card: false,
        face_index: 0,
        effect: ichor_wellspring_draw_effect,
    },
    TriggeredAbilityDef {
        condition: TriggerCondition::LeftBattlefield,
        home_zone: Zone::Graveyard,
        intervening_if_kicked: false,
        intervening_if_controls_another_source_card: false,
        face_index: 0,
        effect: ichor_wellspring_draw_effect,
    },
];
const JOB_SELECT_TRIGGERS: [TriggeredAbilityDef; 1] = [TriggeredAbilityDef {
    condition: TriggerCondition::Etb,
    home_zone: Zone::Battlefield,
    intervening_if_kicked: false,
    intervening_if_controls_another_source_card: false,
    face_index: 0,
    effect: job_select_effect,
}];
const NIHIL_SPELLBOMB_TRIGGERS: [TriggeredAbilityDef; 1] = [TriggeredAbilityDef {
    condition: TriggerCondition::LeftBattlefieldToGraveyard,
    home_zone: Zone::Graveyard,
    intervening_if_kicked: false,
    intervening_if_controls_another_source_card: false,
    face_index: 0,
    effect: nihil_spellbomb_graveyard_effect,
}];
const EXPERIMENTAL_SYNTHESIZER_TRIGGERS: [TriggeredAbilityDef; 2] = [
    TriggeredAbilityDef {
        condition: TriggerCondition::Etb,
        home_zone: Zone::Battlefield,
        intervening_if_kicked: false,
        intervening_if_controls_another_source_card: false,
        face_index: 0,
        effect: experimental_synthesizer_impulse_effect,
    },
    TriggeredAbilityDef {
        condition: TriggerCondition::LeftBattlefield,
        // Leave-the-battlefield conditions use last-known information and
        // therefore deliberately ignore this post-event zone gate.
        home_zone: Zone::Graveyard,
        intervening_if_kicked: false,
        intervening_if_controls_another_source_card: false,
        face_index: 0,
        effect: experimental_synthesizer_impulse_effect,
    },
];
const GOBLIN_BUSHWHACKER_TRIGGERS: [TriggeredAbilityDef; 1] = [TriggeredAbilityDef {
    condition: TriggerCondition::Etb,
    home_zone: Zone::Battlefield,
    intervening_if_kicked: true,
    intervening_if_controls_another_source_card: false,
    face_index: 0,
    effect: goblin_bushwhacker_effect,
}];

const FAERIE_MISCREANT_TRIGGERS: [TriggeredAbilityDef; 1] = [TriggeredAbilityDef {
    condition: TriggerCondition::Etb,
    home_zone: Zone::Battlefield,
    intervening_if_kicked: false,
    intervening_if_controls_another_source_card: true,
    face_index: 0,
    effect: faerie_miscreant_effect,
}];

const FAERIE_SEER_TRIGGERS: [TriggeredAbilityDef; 1] = [TriggeredAbilityDef {
    condition: TriggerCondition::Etb,
    home_zone: Zone::Battlefield,
    intervening_if_kicked: false,
    intervening_if_controls_another_source_card: false,
    face_index: 0,
    effect: faerie_seer_effect,
}];

const OUTLAW_MEDIC_TRIGGERS: [TriggeredAbilityDef; 1] = [TriggeredAbilityDef {
    condition: TriggerCondition::LeftBattlefieldToGraveyard,
    home_zone: Zone::Graveyard,
    intervening_if_kicked: false,
    intervening_if_controls_another_source_card: false,
    face_index: 0,
    effect: outlaw_medic_dies_effect,
}];

const ADVENTURING_GEAR_TRIGGERS: [TriggeredAbilityDef; 1] = [TriggeredAbilityDef {
    condition: TriggerCondition::ControlledLandEnters,
    home_zone: Zone::Battlefield,
    intervening_if_kicked: false,
    intervening_if_controls_another_source_card: false,
    face_index: 0,
    effect: adventuring_gear_landfall_effect,
}];

const GOLDVEIN_PICK_TRIGGERS: [TriggeredAbilityDef; 1] = [TriggeredAbilityDef {
    condition: TriggerCondition::EquippedCreatureDealsCombatDamageToPlayer,
    home_zone: Zone::Battlefield,
    intervening_if_kicked: false,
    intervening_if_controls_another_source_card: false,
    face_index: 0,
    effect: goldvein_pick_combat_damage_effect,
}];

const CAMPUS_GUIDE_TRIGGERS: [TriggeredAbilityDef; 1] = [TriggeredAbilityDef {
    face_index: 0,
    condition: TriggerCondition::Etb,
    home_zone: Zone::Battlefield,
    intervening_if_kicked: false,
    intervening_if_controls_another_source_card: false,
    effect: campus_guide_etb_effect,
}];

const SOLEMN_SIMULACRUM_TRIGGERS: [TriggeredAbilityDef; 2] = [
    TriggeredAbilityDef {
        condition: TriggerCondition::Etb,
        home_zone: Zone::Battlefield,
        intervening_if_kicked: false,
        intervening_if_controls_another_source_card: false,
        face_index: 0,
        effect: solemn_simulacrum_etb_effect,
    },
    TriggeredAbilityDef {
        condition: TriggerCondition::LeftBattlefieldToGraveyard,
        home_zone: Zone::Graveyard,
        intervening_if_kicked: false,
        intervening_if_controls_another_source_card: false,
        face_index: 0,
        effect: solemn_simulacrum_dies_effect,
    },
];

const REFURBISHED_FAMILIAR_TRIGGERS: [TriggeredAbilityDef; 1] = [TriggeredAbilityDef {
    condition: TriggerCondition::Etb,
    home_zone: Zone::Battlefield,
    intervening_if_kicked: false,
    intervening_if_controls_another_source_card: false,
    face_index: 0,
    effect: refurbished_familiar_etb_effect,
}];

const SQUADRON_HAWK_TRIGGERS: [TriggeredAbilityDef; 1] = [TriggeredAbilityDef {
    condition: TriggerCondition::Etb,
    home_zone: Zone::Battlefield,
    intervening_if_kicked: false,
    intervening_if_controls_another_source_card: false,
    face_index: 0,
    effect: squadron_hawk_etb_effect,
}];

const BIND_THE_MONSTER_TRIGGERS: [TriggeredAbilityDef; 1] = [TriggeredAbilityDef {
    condition: TriggerCondition::Etb,
    home_zone: Zone::Battlefield,
    intervening_if_kicked: false,
    intervening_if_controls_another_source_card: false,
    face_index: 0,
    effect: bind_the_monster_etb_effect,
}];

const HARRIER_STRIX_TRIGGERS: [TriggeredAbilityDef; 1] = [TriggeredAbilityDef {
    condition: TriggerCondition::Etb,
    home_zone: Zone::Battlefield,
    intervening_if_kicked: false,
    intervening_if_controls_another_source_card: false,
    face_index: 0,
    effect: harrier_strix_etb_effect,
}];

const BOJUKA_BOG_TRIGGERS: [TriggeredAbilityDef; 1] = [TriggeredAbilityDef {
    condition: TriggerCondition::Etb,
    home_zone: Zone::Battlefield,
    intervening_if_kicked: false,
    intervening_if_controls_another_source_card: false,
    face_index: 0,
    effect: bojuka_bog_etb_effect,
}];

const CONDUIT_PYLONS_TRIGGERS: [TriggeredAbilityDef; 1] = [TriggeredAbilityDef {
    condition: TriggerCondition::Etb,
    home_zone: Zone::Battlefield,
    intervening_if_kicked: false,
    intervening_if_controls_another_source_card: false,
    face_index: 0,
    effect: conduit_pylons_etb_effect,
}];

const HUMBLING_ELDER_TRIGGERS: [TriggeredAbilityDef; 1] = [TriggeredAbilityDef {
    condition: TriggerCondition::Etb,
    home_zone: Zone::Battlefield,
    intervening_if_kicked: false,
    intervening_if_controls_another_source_card: false,
    face_index: 0,
    effect: humbling_elder_etb_effect,
}];

/// Dauntless Veteran: "Whenever this creature attacks, creatures you
/// control get +1/+1 until end of turn."
fn dauntless_veteran_attack_effect() -> EffectOp {
    EffectOp::BoostControlledCreaturesUntilEndOfTurn {
        power: 1,
        toughness: 1,
        keywords: Keywords::NONE,
    }
}

const DAUNTLESS_VETERAN_TRIGGERS: [TriggeredAbilityDef; 1] = [TriggeredAbilityDef {
    condition: TriggerCondition::Attacks,
    ..etb_trigger(dauntless_veteran_attack_effect)
}];

/// Crackling Cyclops: "Whenever you cast a noncreature spell, this creature
/// gets +3/+0 until end of turn."
fn crackling_cyclops_effect() -> EffectOp {
    EffectOp::BindTemporaryBoostToTriggerSource {
        power: 3,
        toughness: 0,
    }
}

const CRACKLING_CYCLOPS_TRIGGERS: [TriggeredAbilityDef; 1] = [TriggeredAbilityDef {
    condition: TriggerCondition::CastNoncreatureSpell,
    ..etb_trigger(crackling_cyclops_effect)
}];

const METEOR_GOLEM_TRIGGERS: [TriggeredAbilityDef; 1] = [TriggeredAbilityDef {
    face_index: 0,
    condition: TriggerCondition::Etb,
    home_zone: Zone::Battlefield,
    intervening_if_kicked: false,
    intervening_if_controls_another_source_card: false,
    effect: meteor_golem_etb_effect,
}];

const RECLAMATION_SAGE_TRIGGERS: [TriggeredAbilityDef; 1] = [TriggeredAbilityDef {
    face_index: 0,
    condition: TriggerCondition::Etb,
    home_zone: Zone::Battlefield,
    intervening_if_kicked: false,
    intervening_if_controls_another_source_card: false,
    effect: reclamation_sage_etb_effect,
}];

const MOON_CIRCUIT_HACKER_TRIGGERS: [TriggeredAbilityDef; 1] = [TriggeredAbilityDef {
    condition: TriggerCondition::DealsCombatDamageToPlayer,
    home_zone: Zone::Battlefield,
    intervening_if_kicked: false,
    intervening_if_controls_another_source_card: false,
    face_index: 0,
    effect: moon_circuit_hacker_combat_effect,
}];

const NINJA_OF_THE_DEEP_HOURS_TRIGGERS: [TriggeredAbilityDef; 1] = [TriggeredAbilityDef {
    condition: TriggerCondition::DealsCombatDamageToPlayer,
    home_zone: Zone::Battlefield,
    intervening_if_kicked: false,
    intervening_if_controls_another_source_card: false,
    face_index: 0,
    effect: ninja_of_the_deep_hours_combat_effect,
}];

const SAIBA_CRYPTOMANCER_TRIGGERS: [TriggeredAbilityDef; 1] = [TriggeredAbilityDef {
    condition: TriggerCondition::Etb,
    home_zone: Zone::Battlefield,
    intervening_if_kicked: false,
    intervening_if_controls_another_source_card: false,
    face_index: 0,
    effect: saiba_cryptomancer_etb_effect,
}];

const SPELLSTUTTER_SPRITE_TRIGGERS: [TriggeredAbilityDef; 1] = [TriggeredAbilityDef {
    condition: TriggerCondition::Etb,
    home_zone: Zone::Battlefield,
    intervening_if_kicked: false,
    intervening_if_controls_another_source_card: false,
    face_index: 0,
    effect: spellstutter_sprite_etb_effect,
}];

const LEMBAS_TRIGGERS: [TriggeredAbilityDef; 2] = [
    TriggeredAbilityDef {
        condition: TriggerCondition::Etb,
        home_zone: Zone::Battlefield,
        intervening_if_kicked: false,
        intervening_if_controls_another_source_card: false,
        face_index: 0,
        effect: lembas_etb_effect,
    },
    TriggeredAbilityDef {
        condition: TriggerCondition::LeftBattlefieldToGraveyard,
        home_zone: Zone::Graveyard,
        intervening_if_kicked: false,
        intervening_if_controls_another_source_card: false,
        face_index: 0,
        effect: lembas_graveyard_effect,
    },
];

const WEATHER_THE_STORM_TRIGGERS: [TriggeredAbilityDef; 1] = [TriggeredAbilityDef {
    condition: TriggerCondition::CastSelf,
    home_zone: Zone::Stack,
    intervening_if_kicked: false,
    intervening_if_controls_another_source_card: false,
    face_index: 0,
    effect: weather_the_storm_cast_effect,
}];

const JOURNEY_TO_NOWHERE_TRIGGERS: [TriggeredAbilityDef; 2] = [
    TriggeredAbilityDef {
        condition: TriggerCondition::Etb,
        home_zone: Zone::Battlefield,
        intervening_if_kicked: false,
        intervening_if_controls_another_source_card: false,
        face_index: 0,
        effect: journey_to_nowhere_etb_effect,
    },
    TriggeredAbilityDef {
        condition: TriggerCondition::LeftBattlefield,
        // Leave-the-battlefield triggers use the historical battlefield
        // incarnation and deliberately ignore this post-event zone gate.
        home_zone: Zone::Graveyard,
        intervening_if_kicked: false,
        intervening_if_controls_another_source_card: false,
        face_index: 0,
        effect: journey_to_nowhere_ltb_effect,
    },
];
const MASKED_VANDAL_TRIGGERS: [TriggeredAbilityDef; 1] = [TriggeredAbilityDef {
    condition: TriggerCondition::Etb,
    home_zone: Zone::Battlefield,
    intervening_if_kicked: false,
    intervening_if_controls_another_source_card: false,
    face_index: 0,
    effect: masked_vandal_etb_effect,
}];

const TROUBLEMAKER_OUPHE_TRIGGERS: [TriggeredAbilityDef; 1] = [TriggeredAbilityDef {
    condition: TriggerCondition::Etb,
    home_zone: Zone::Battlefield,
    intervening_if_kicked: false,
    intervening_if_controls_another_source_card: false,
    face_index: 0,
    effect: troublemaker_ouphe_etb_effect,
}];

const VITU_GHAZI_INSPECTOR_TRIGGERS: [TriggeredAbilityDef; 1] = [TriggeredAbilityDef {
    condition: TriggerCondition::Etb,
    home_zone: Zone::Battlefield,
    intervening_if_kicked: false,
    intervening_if_controls_another_source_card: false,
    face_index: 0,
    effect: vitu_ghazi_inspector_etb_effect,
}];

const AVENGING_HUNTER_TRIGGERS: [TriggeredAbilityDef; 1] = [TriggeredAbilityDef {
    condition: TriggerCondition::Etb,
    home_zone: Zone::Battlefield,
    intervening_if_kicked: false,
    intervening_if_controls_another_source_card: false,
    face_index: 0,
    effect: avenging_hunter_etb_effect,
}];

const AZURE_FLEET_ADMIRAL_TRIGGERS: [TriggeredAbilityDef; 1] = [TriggeredAbilityDef {
    condition: TriggerCondition::Etb,
    home_zone: Zone::Battlefield,
    intervening_if_kicked: false,
    intervening_if_controls_another_source_card: false,
    face_index: 0,
    effect: azure_fleet_admiral_etb_effect,
}];

const DELVER_OF_SECRETS_TRIGGERS: [TriggeredAbilityDef; 1] = [TriggeredAbilityDef {
    condition: TriggerCondition::BeginningOfUpkeep {
        controller_only: true,
    },
    home_zone: Zone::Battlefield,
    intervening_if_kicked: false,
    intervening_if_controls_another_source_card: false,
    face_index: 0,
    effect: delver_of_secrets_effect,
}];

/// The pool's implemented triggered abilities, matched by card name (ids are
/// codegen-assigned from `cards_v1.json`'s array order and not worth
/// duplicating as constants here -- see `build.rs`'s module doc on id
/// stability). Every other card in the pool has no triggered ability
/// implemented and falls through to `&[]`.
///
/// Memoized per definition id: the dispatch below is a pure function of the
/// static `CARD_DEFS` entry, but its by-name `match` is a long chain of string
/// compares, and trigger collection calls it for every object in every zone on
/// every committed event batch.
pub fn triggers_for(card_def: u16) -> &'static [TriggeredAbilityDef] {
    static TABLE: std::sync::OnceLock<Box<[&'static [TriggeredAbilityDef]]>> =
        std::sync::OnceLock::new();
    let table = TABLE.get_or_init(|| {
        (0..crate::card_def::CARD_DEFS.len())
            .map(|index| triggers_for_uncached(index as u16))
            .collect()
    });
    table.get(card_def as usize).copied().unwrap_or(&[])
}

fn triggers_for_uncached(card_def: u16) -> &'static [TriggeredAbilityDef] {
    let Some(card) = crate::card_def::CARD_DEFS.get(card_def as usize) else {
        return &[];
    };
    if !card.is_executable() {
        return &[];
    }
    if card.equipment.is_some_and(|equipment| equipment.job_select) {
        return &JOB_SELECT_TRIGGERS;
    }
    match card.name {
        "Celestial Armor" => &CELESTIAL_ARMOR_TRIGGERS,
        "Exemplar of Light" => &EXEMPLAR_OF_LIGHT_TRIGGERS,
        "Vanguard Seraph" => &VANGUARD_SERAPH_TRIGGERS,
        "Cat Collector" => &CAT_COLLECTOR_TRIGGERS,
        "Sun-Blessed Healer" => &SUN_BLESSED_HEALER_TRIGGERS,
        "Mossborn Hydra" => &MOSSBORN_HYDRA_TRIGGERS,
        "Beast-Kin Ranger" => &BEAST_KIN_RANGER_TRIGGERS,
        "Dwynen, Gilt-Leaf Daen" => &DWYNEN_TRIGGERS,
        "Blossoming Sands" | "Thornwood Falls" | "Bloodfell Caves" | "Dismal Backwater"
        | "Jungle Hollow" | "Rugged Highlands" | "Scoured Barrens" | "Swiftwater Cliffs"
        | "Tranquil Cove" | "Wind-Scarred Crag" => &GAIN_ONE_LIFE_TRIGGERS,
        "Ajani's Pridemate" => &AJANIS_PRIDEMATE_TRIGGERS,
        "Marauding Blight-Priest" => &MARAUDING_BLIGHT_PRIEST_TRIGGERS,
        "Sanguine Syphoner" => &SANGUINE_SYPHONER_TRIGGERS,
        "Helpful Hunter" => &HELPFUL_HUNTER_TRIGGERS,
        "Prideful Parent" => &PRIDEFUL_PARENT_TRIGGERS,
        "Icewind Elemental" => &ICEWIND_ELEMENTAL_TRIGGERS,
        "Dreadwing Scavenger" => &DREADWING_SCAVENGER_TRIGGERS,
        "Mischievous Pup" => &MISCHIEVOUS_PUP_TRIGGERS,
        "Felidar Savior" => &FELIDAR_SAVIOR_TRIGGERS,
        "Armasaur Guide" => &ARMASAUR_GUIDE_TRIGGERS,
        "Arbiter of Woe" => &ARBITER_OF_WOE_TRIGGERS,
        "Burglar Rat" => &BURGLAR_RAT_TRIGGERS,
        "Infestation Sage" => &INFESTATION_SAGE_TRIGGERS,
        "Wary Thespian" => &WARY_THESPIAN_TRIGGERS,
        "Firebrand Archer" => &KESSIG_FLAMEBREATHER_TRIGGERS,
        "Spitfire Lagac" => &SPITFIRE_LAGAC_TRIGGERS,
        "Dragon Trainer" => &DRAGON_TRAINER_TRIGGERS,
        "Resolute Reinforcements" => &RESOLUTE_REINFORCEMENTS_TRIGGERS,
        "Elfsworn Giant" => &ELFSWORN_GIANT_TRIGGERS,
        "Eager Trufflesnout" => &EAGER_TRUFFLESNOUT_TRIGGERS,
        "Rite of the Dragoncaller" => &RITE_OF_THE_DRAGONCALLER_TRIGGERS,
        "Elementalist Adept" => &ELEMENTALIST_ADEPT_TRIGGERS,
        "Lightshell Duo" => &LIGHTSHELL_DUO_TRIGGERS,
        "Cephalid Inkmage" => &CEPHALID_INKMAGE_TRIGGERS,
        "Billowing Shriekmass" => &BILLOWING_SHRIEKMASS_TRIGGERS,
        "Apothecary Stomper" => &APOTHECARY_STOMPER_TRIGGERS,
        "Wardens of the Cycle" => &WARDENS_OF_THE_CYCLE_TRIGGERS,
        "Crypt Feaster" => &CRYPT_FEASTER_TRIGGERS,
        "Erudite Wizard" => &ERUDITE_WIZARD_TRIGGERS,
        "Phyrexian Arena" => &PHYREXIAN_ARENA_TRIGGERS,
        "Gleaming Barrier" => &GLEAMING_BARRIER_TRIGGERS,
        "Angel of Finality" => &BOJUKA_BOG_TRIGGERS,
        "Bigfin Bouncer" => &BIGFIN_BOUNCER_TRIGGERS,
        "Tragic Banshee" => &TRAGIC_BANSHEE_TRIGGERS,
        "Battlesong Berserker" => &BATTLESONG_BERSERKER_TRIGGERS,
        "Scrawling Crawler" => &SCRAWLING_CRAWLER_TRIGGERS,
        "Grappling Kraken" => &GRAPPLING_KRAKEN_TRIGGERS,
        "Rune-Scarred Demon" => &RUNE_SCARRED_DEMON_TRIGGERS,
        "Elvish Regrower" => &ELVISH_REGROWER_TRIGGERS,
        "Vampire Soulcaller" => &VAMPIRE_SOULCALLER_TRIGGERS,
        "Affectionate Indrik" => &AFFECTIONATE_INDRIK_TRIGGERS,
        "Ambush Wolf" => &AMBUSH_WOLF_TRIGGERS,
        "Tatyova, Benthic Druid" => &TATYOVA_BENTHIC_DRUID_TRIGGERS,
        "Dazzling Angel" => &DAZZLING_ANGEL_TRIGGERS,
        "Clinquant Skymage" => &CLINQUANT_SKYMAGE_TRIGGERS,
        "Mischievous Mystic" => &MISCHIEVOUS_MYSTIC_TRIGGERS,
        "Homunculus Horde" | "Homunculus Horde Token" => &HOMUNCULUS_HORDE_TRIGGERS,
        "Koma, World-Eater" => &KOMA_WORLD_EATER_TRIGGERS,
        "Kiora, the Rising Tide" => &KIORA_RISING_TIDE_TRIGGERS,
        "Cackling Prowler" => &CACKLING_PROWLER_TRIGGERS,
        "Sylvan Scavenging" => &SYLVAN_SCAVENGING_TRIGGERS,
        "Dwynen's Elite" => &DWYNENS_ELITE_TRIGGERS,
        "Good-Fortune Unicorn" => &GOOD_FORTUNE_UNICORN_TRIGGERS,
        "Guarded Heir" => &GUARDED_HEIR_TRIGGERS,
        "Youthful Valkyrie" => &YOUTHFUL_VALKYRIE_TRIGGERS,
        "Guttersnipe" => &GUTTERSNIPE_TRIGGERS,
        "Murmuring Mystic" => &MURMURING_MYSTIC_TRIGGERS,
        "Voldaren Epicure" => &VOLDAREN_EPICURE_TRIGGERS,
        "Generous Ent" => &GENEROUS_ENT_TRIGGERS,
        "Gingerbread Cabin" => &GINGERBREAD_CABIN_TRIGGERS,
        "Writhing Chrysalis" => &WRITHING_CHRYSALIS_TRIGGERS,
        "Blood Fountain" => &BLOOD_FOUNTAIN_TRIGGERS,
        "Sagu Wildling" => &SAGU_WILDLING_TRIGGERS,
        "Kessig Flamebreather" => &KESSIG_FLAMEBREATHER_TRIGGERS,
        "Balmor, Battlemage Captain" => &BALMOR_TRIGGERS,
        "Firespitter Whelp" => &FIRESPITTER_WHELP_TRIGGERS,
        "Gixian Infiltrator" => &GIXIAN_INFILTRATOR_TRIGGERS,
        "Brineborn Cutthroat" => &BRINEBORN_CUTTHROAT_TRIGGERS,
        "Webweaver Changeling" => &WEBWEAVER_CHANGELING_TRIGGERS,
        "Glint Hawk" => &GLINT_HAWK_TRIGGERS,
        "Gatecreeper Vine" => &GATECREEPER_VINE_TRIGGERS,
        "Balustrade Spy" => &BALUSTRADE_SPY_TRIGGERS,
        "Lotleth Giant" => &LOTLETH_GIANT_TRIGGERS,
        "Mesmeric Fiend" => &MESMERIC_FIEND_TRIGGERS,
        "Healer of the Glade" | "Spinewoods Paladin" => &GAIN_THREE_LIFE_ETB_TRIGGERS,
        "Sneaky Snacker" => &SNEAKY_SNACKER_TRIGGERS,
        "Burning-Tree Emissary" => &BURNING_TREE_EMISSARY_TRIGGERS,
        "Clockwork Percussionist" => &CLOCKWORK_PERCUSSIONIST_TRIGGERS,
        "Ichor Wellspring" => &ICHOR_WELLSPRING_TRIGGERS,
        "Cryogen Relic" => &CRYOGEN_RELIC_TRIGGERS,
        "Nihil Spellbomb" => &NIHIL_SPELLBOMB_TRIGGERS,
        "Experimental Synthesizer" => &EXPERIMENTAL_SYNTHESIZER_TRIGGERS,
        "Goblin Bushwhacker" => &GOBLIN_BUSHWHACKER_TRIGGERS,
        "Faerie Miscreant" => &FAERIE_MISCREANT_TRIGGERS,
        "Faerie Seer" => &FAERIE_SEER_TRIGGERS,
        "Outlaw Medic" => &OUTLAW_MEDIC_TRIGGERS,
        "Solemn Simulacrum" => &SOLEMN_SIMULACRUM_TRIGGERS,
        "Campus Guide" => &CAMPUS_GUIDE_TRIGGERS,
        "Adventuring Gear" => &ADVENTURING_GEAR_TRIGGERS,
        "Goldvein Pick" => &GOLDVEIN_PICK_TRIGGERS,
        "Refurbished Familiar" => &REFURBISHED_FAMILIAR_TRIGGERS,
        "Squadron Hawk" => &SQUADRON_HAWK_TRIGGERS,
        "Bind the Monster" => &BIND_THE_MONSTER_TRIGGERS,
        "Harrier Strix" => &HARRIER_STRIX_TRIGGERS,
        "Bojuka Bog" => &BOJUKA_BOG_TRIGGERS,
        "Conduit Pylons" => &CONDUIT_PYLONS_TRIGGERS,
        "Elegant Parlor" | "Lush Portico" | "Underground Mortuary" => &CONDUIT_PYLONS_TRIGGERS,
        "Humbling Elder" => &HUMBLING_ELDER_TRIGGERS,
        "Meteor Golem" => &METEOR_GOLEM_TRIGGERS,
        "Dauntless Veteran" => &DAUNTLESS_VETERAN_TRIGGERS,
        "Crackling Cyclops" => &CRACKLING_CYCLOPS_TRIGGERS,
        "Reclamation Sage" => &RECLAMATION_SAGE_TRIGGERS,
        "Moon-Circuit Hacker" => &MOON_CIRCUIT_HACKER_TRIGGERS,
        "Ninja of the Deep Hours" => &NINJA_OF_THE_DEEP_HOURS_TRIGGERS,
        "Saiba Cryptomancer" => &SAIBA_CRYPTOMANCER_TRIGGERS,
        "Spellstutter Sprite" => &SPELLSTUTTER_SPRITE_TRIGGERS,
        "Journey to Nowhere" => &JOURNEY_TO_NOWHERE_TRIGGERS,
        "Lembas" => &LEMBAS_TRIGGERS,
        "Weather the Storm" => &WEATHER_THE_STORM_TRIGGERS,
        "Masked Vandal" => &MASKED_VANDAL_TRIGGERS,
        "Troublemaker Ouphe" => &TROUBLEMAKER_OUPHE_TRIGGERS,
        "Vitu-Ghazi Inspector" => &VITU_GHAZI_INSPECTOR_TRIGGERS,
        "Avenging Hunter" => &AVENGING_HUNTER_TRIGGERS,
        "Azure Fleet Admiral" => &AZURE_FLEET_ADMIRAL_TRIGGERS,
        "Delver of Secrets" => &DELVER_OF_SECRETS_TRIGGERS,
        "Emberheart Challenger" => &EMBERHEART_CHALLENGER_TRIGGERS,
        "Nova Hellkite" => &NOVA_HELLKITE_TRIGGERS,
        "Iridescent Vinelasher" | "Iridescent Vinelasher Offspring Token" => {
            &IRIDESCENT_VINELASHER_TRIGGERS
        }
        "Aloe Alchemist" => &ALOE_ALCHEMIST_TRIGGERS,
        "Forsaken Miner" => &FORSAKEN_MINER_TRIGGERS,
        "Hopeful Initiate" => &HOPEFUL_INITIATE_TRIGGERS,
        "Chrome Host Seedshark" => &CHROME_HOST_SEEDSHARK_TRIGGERS,
        "Brutal Cathar" => &BRUTAL_CATHAR_TRIGGERS,
        "Knight-Errant of Eos" => &KNIGHT_ERRANT_OF_EOS_TRIGGERS,
        "Monastery Swiftspear" => &MONASTERY_SWIFTSPEAR_TRIGGERS,
        "Heartfire Hero" => &HEARTFIRE_HERO_TRIGGERS,
        "Slickshot Show-Off" => &SLICKSHOT_SHOW_OFF_TRIGGERS,
        "Sanguine Evangelist" => &SANGUINE_EVANGELIST_TRIGGERS,
        "Darkstar Augur" | "Darkstar Augur Offspring Token" => &DARKSTAR_AUGUR_TRIGGERS,
        "Ruin-Lurker Bat" => &RUIN_LURKER_BAT_TRIGGERS,
        "Pawpatch Recruit" | "Pawpatch Recruit Offspring Token" => &PAWPATCH_RECRUIT_TRIGGERS,
        "Manifold Mouse" | "Manifold Mouse Offspring Token" => &MANIFOLD_MOUSE_TRIGGERS,
        "Yotian Frontliner" => &YOTIAN_FRONTLINER_TRIGGERS,
        "Cori-Steel Cutter" => &CORI_STEEL_CUTTER_TRIGGERS,
        "Graveyard Trespasser" => &GRAVEYARD_TRESPASSER_TRIGGERS,
        "Overlord of the Mistmoors" => &OVERLORD_OF_THE_MISTMOORS_TRIGGERS,
        "Enduring Curiosity" => &ENDURING_CURIOSITY_TRIGGERS,
        "Enduring Innocence" => &ENDURING_INNOCENCE_TRIGGERS,
        "Monk Token" => &MONASTERY_SWIFTSPEAR_TRIGGERS,
        #[cfg(feature = "standard-magezero-fixtures")]
        "Novice Inspector" => &standard_family_g_v1::NOVICE_INSPECTOR_TRIGGERS,
        #[cfg(feature = "standard-magezero-fixtures")]
        "Sentinel of the Nameless City" => {
            &standard_family_g_v1::SENTINEL_OF_THE_NAMELESS_CITY_TRIGGERS
        }
        #[cfg(feature = "standard-magezero-fixtures")]
        "Cenote Scout" => &standard_family_g_v1::CENOTE_SCOUT_TRIGGERS,
        #[cfg(feature = "standard-magezero-fixtures")]
        "Gatekeeper of Malakir" => &standard_family_g_v1::GATEKEEPER_OF_MALAKIR_TRIGGERS,
        #[cfg(feature = "standard-magezero-fixtures")]
        "Deep-Cavern Bat" => &standard_family_g_v1::DEEP_CAVERN_BAT_TRIGGERS,
        #[cfg(feature = "standard-magezero-fixtures")]
        "Razorkin Needlehead" => &standard_family_g_v1::RAZORKIN_NEEDLEHEAD_TRIGGERS,
        #[cfg(feature = "standard-magezero-fixtures")]
        "Quirion Beastcaller" => &standard_family_g_v1::QUIRION_BEASTCALLER_TRIGGERS,
        #[cfg(feature = "standard-magezero-fixtures")]
        "Ascendant Packleader" => &standard_family_g_v1::ASCENDANT_PACKLEADER_TRIGGERS,
        #[cfg(feature = "standard-magezero-fixtures")]
        "Sharp-Eyed Rookie" => &standard_family_g_v1::SHARP_EYED_ROOKIE_TRIGGERS,
        #[cfg(feature = "standard-magezero-fixtures")]
        "Evolving Adaptive" => &standard_family_g_v1::EVOLVING_ADAPTIVE_TRIGGERS,
        #[cfg(feature = "standard-magezero-fixtures")]
        "Unstoppable Slasher" => &standard_family_g_v1::UNSTOPPABLE_SLASHER_TRIGGERS,
        #[cfg(feature = "standard-magezero-fixtures")]
        "Adeline, Resplendent Cathar" => &standard_family_g_v1::ADELINE_RESPLENDENT_CATHAR_TRIGGERS,
        #[cfg(feature = "standard-magezero-fixtures")]
        "Hired Claw" => &standard_family_g_v1::HIRED_CLAW_TRIGGERS,
        #[cfg(feature = "standard-magezero-fixtures")]
        "Extraction Specialist" => &standard_family_g_v1::EXTRACTION_SPECIALIST_TRIGGERS,
        #[cfg(feature = "standard-magezero-fixtures")]
        "Hullbreaker Horror" => &standard_family_g_v1::HULLBREAKER_HORROR_TRIGGERS,
        _ => &[],
    }
}

/// Definition-owned target specification for the pool's triggered abilities.
pub fn trigger_target_spec(card_def: u16) -> TargetSpec {
    let Some(card) = crate::card_def::CARD_DEFS.get(card_def as usize) else {
        return TargetSpec::None;
    };
    match card.name {
        "Celestial Armor" => TargetSpec::ControlledCreature,
        "Sun-Blessed Healer" => TargetSpec::NonlandPermanentCardInOwnGraveyardManaValueAtMost(2),
        "Elvish Regrower" => TargetSpec::PermanentCardInOwnGraveyard,
        "Mischievous Pup" => TargetSpec::UpToOneOtherControlledPermanent,
        "Felidar Savior" => TargetSpec::UpToTwoOtherControlledCreatures,
        "Armasaur Guide" => TargetSpec::ControlledCreature,
        "Vampire Soulcaller" => TargetSpec::CreatureCardInOwnGraveyard,
        "Affectionate Indrik" => TargetSpec::OpponentControlledCreature,
        "Ambush Wolf" => TargetSpec::UpToOneCardInGraveyards,
        "Balustrade Spy" => TargetSpec::AnyPlayer,
        "Lotleth Giant" => TargetSpec::TargetOpponent,
        "Harrier Strix" => TargetSpec::AnyPermanent,
        "Bojuka Bog" | "Angel of Finality" => TargetSpec::AnyPlayer,
        "Bigfin Bouncer" | "Nova Hellkite" | "Tragic Banshee" | "Grappling Kraken" => {
            TargetSpec::OpponentControlledCreature
        }
        "Humbling Elder" => TargetSpec::OpponentControlledCreature,
        "Battlesong Berserker" => TargetSpec::ControlledCreature,
        "Meteor Golem" => TargetSpec::OpponentNonlandPermanent,
        "Reclamation Sage" => TargetSpec::ArtifactOrEnchantmentPermanent,
        "Saiba Cryptomancer" | "Aloe Alchemist" => TargetSpec::Creature,
        "Spellstutter Sprite" => TargetSpec::SpellManaValueAtMostControlledSubtypes {
            first: Subtype::Faerie,
            second: Some(Subtype::FaerieAllCaps),
        },
        "Masked Vandal" | "Troublemaker Ouphe" => {
            TargetSpec::OpponentArtifactOrEnchantmentPermanent
        }
        "Vitu-Ghazi Inspector" => TargetSpec::Creature,
        #[cfg(feature = "standard-magezero-fixtures")]
        "Gatekeeper of Malakir" => TargetSpec::AnyPlayer,
        #[cfg(feature = "standard-magezero-fixtures")]
        "Deep-Cavern Bat" | "Hired Claw" => TargetSpec::TargetOpponent,
        #[cfg(feature = "standard-magezero-fixtures")]
        "Extraction Specialist" => TargetSpec::CreatureCardInOwnGraveyardManaValueAtMost(2),
        _ => TargetSpec::None,
    }
}

/// Per-trigger target specs for Standard cards whose triggers do not all
/// share one spec (`trigger_target_spec` is keyed by card name only).
fn standard_trigger_target_spec(name: &str, effect: &EffectOp) -> Option<TargetSpec> {
    if !cfg!(feature = "standard-magezero-fixtures") {
        return None;
    }
    match name {
        "Iridescent Vinelasher" | "Iridescent Vinelasher Offspring Token" => {
            Some(if *effect == iridescent_vinelasher_landfall_effect() {
                TargetSpec::TargetOpponent
            } else {
                TargetSpec::None
            })
        }
        "Pawpatch Recruit" | "Pawpatch Recruit Offspring Token" => Some(
            if matches!(
                effect,
                EffectOp::PutPlusOnePlusOneCounterOnTargetOtherThan { .. }
            ) {
                TargetSpec::AnotherControlledCreature
            } else {
                TargetSpec::None
            },
        ),
        "Manifold Mouse" | "Manifold Mouse Offspring Token" => {
            Some(if *effect == manifold_mouse_combat_effect() {
                TargetSpec::ControlledCreatureWithSubtype(crate::card_def::Subtype::Mouse)
            } else {
                TargetSpec::None
            })
        }
        "Graveyard Trespasser" => Some(match effect {
            EffectOp::ExileGraveyardTargetsDrainPerCreature { max_targets: 1 } => {
                TargetSpec::UpToOneCardInGraveyards
            }
            EffectOp::ExileGraveyardTargetsDrainPerCreature { max_targets: 2 } => {
                TargetSpec::UpToTwoCardsInGraveyards
            }
            _ => TargetSpec::None,
        }),
        "Yotian Frontliner" => Some(if *effect == yotian_frontliner_attack_effect() {
            TargetSpec::AnotherControlledCreature
        } else {
            TargetSpec::None
        }),
        "Brutal Cathar" => Some(if *effect == journey_to_nowhere_etb_effect() {
            TargetSpec::OpponentControlledCreature
        } else {
            TargetSpec::None
        }),
        _ => None,
    }
}

pub(crate) fn required_optional_additional_cost_for_trigger(
    card_def: u16,
    effect: &EffectOp,
) -> Option<OptionalAdditionalCostDef> {
    let card = crate::card_def::CARD_DEFS.get(card_def as usize)?;
    match card.name {
        "Troublemaker Ouphe" if *effect == troublemaker_ouphe_etb_effect() => {
            Some(OptionalAdditionalCostDef::Bargain)
        }
        "Vitu-Ghazi Inspector" if *effect == vitu_ghazi_inspector_etb_effect() => {
            Some(OptionalAdditionalCostDef::CollectEvidence {
                minimum_mana_value: 6,
            })
        }
        _ => None,
    }
}

/// Authenticates the finite set of effects a definition-owned trigger can
/// place on the stack, including Moon-Circuit Hacker's event-frozen branch.
fn source_bound_trigger_program_matches(template: &EffectOp, effect: &EffectOp) -> bool {
    if template == effect {
        return true;
    }
    match (template, effect) {
        (
            EffectOp::Conditional { cond, then, else_ },
            EffectOp::Conditional {
                cond: actual_cond,
                then: actual_then,
                else_: actual_else,
            },
        ) => {
            cond == actual_cond
                && source_bound_trigger_program_matches(then, actual_then)
                && source_bound_trigger_program_matches(else_, actual_else)
        }
        (
            EffectOp::BindTemporaryBoostToTriggerSource { power, toughness },
            EffectOp::BoostBoundObjectUntilEndOfTurn {
                power: actual_power,
                toughness: actual_toughness,
                ..
            },
        ) => power == actual_power && toughness == actual_toughness,
        (EffectOp::Sequence(template_steps), EffectOp::Sequence(actual_steps)) => {
            template_steps.len() == actual_steps.len()
                && template_steps
                    .iter()
                    .zip(actual_steps)
                    .all(|(template, actual)| {
                        source_bound_trigger_program_matches(template, actual)
                    })
        }
        (
            EffectOp::BindEntrantOutgrowsSourceThen { then },
            EffectOp::IfEntrantOutgrowsSourceThen {
                then: actual_then, ..
            },
        ) => source_bound_trigger_program_matches(then, actual_then),
        (EffectOp::BindOilCounterToTriggerSource, EffectOp::PutOilCounterOnBoundObject { .. }) => {
            true
        }
        (
            EffectOp::BindPlusOnePlusOneCounterToTriggerSource,
            EffectOp::PutPlusOnePlusOneCounterOnBoundObject { .. },
        )
        | (
            EffectOp::BindPlusOnePlusOneCounterToTriggerEventObject,
            EffectOp::PutPlusOnePlusOneCounterOnTriggerEventObject { .. },
        )
        | (
            EffectOp::BindDoublePlusOneCountersToTriggerSource,
            EffectOp::DoublePlusOneCountersOnBoundObject { .. },
        )
        | (EffectOp::BindWarpExileToTriggerSource, EffectOp::WarpExileBoundObject { .. })
        | (EffectOp::BindIncubateToTriggerSpell, EffectOp::Incubate { .. })
        | (
            EffectOp::BindPlusOneCounterOnAnotherTargetToTriggerTarget,
            EffectOp::PutPlusOnePlusOneCounterOnTargetOtherThan { .. },
        )
        | (
            EffectOp::BindDamageOpponentEqualToSourceLastPower,
            EffectOp::DealDamage {
                target: TargetRef::Opponent,
                ..
            },
        ) => true,
        (
            EffectOp::BindConvokedCreatureCountToLookTop { count, max_taken },
            EffectOp::LookTopTakeCreaturesManaValueAtMostThenShuffle {
                count: actual_count,
                max_taken: actual_max_taken,
                ..
            },
        ) => count == actual_count && max_taken == actual_max_taken,
        _ => false,
    }
}

pub fn trigger_effect_matches(card_def: u16, effect: &EffectOp) -> bool {
    let Some(card) = crate::card_def::CARD_DEFS.get(card_def as usize) else {
        return false;
    };
    if card.name == "Wardens of the Cycle"
        && wardens_of_the_cycle_modes()
            .iter()
            .any(|(_, branch)| branch == effect)
    {
        return true;
    }
    if card.name == "Apothecary Stomper"
        && apothecary_stomper_modes()
            .iter()
            .any(|(_, branch)| branch == effect)
    {
        return true;
    }
    if card.name == "Sylvan Scavenging"
        && sylvan_scavenging_modes()
            .iter()
            .any(|(_, branch)| branch == effect)
    {
        return true;
    }
    if card.name == "Moon-Circuit Hacker"
        && [false, true].into_iter().any(|entered| {
            moon_circuit_hacker_combat_effect_for_entered_this_turn(entered) == *effect
        })
    {
        return true;
    }
    #[cfg(feature = "standard-magezero-fixtures")]
    if card.name == "Quirion Beastcaller"
        && standard_family_g_v1::is_quirion_beastcaller_dies_effect(effect)
    {
        return true;
    }
    #[cfg(feature = "standard-magezero-fixtures")]
    if card.name == "Hullbreaker Horror"
        && standard_family_g_v1::hullbreaker_horror_modes()
            .iter()
            .any(|(_, branch)| branch == effect)
    {
        return true;
    }
    if triggers_for(card_def)
        .iter()
        .any(|trigger| source_bound_trigger_program_matches(&(trigger.effect)(), effect))
    {
        return true;
    }

    if card.name == "Weather the Storm" && matches!(effect, EffectOp::CreateStormCopies { .. }) {
        return true;
    }
    if card.name == "Avenging Hunter" && matches!(effect, EffectOp::ResolveInitiativeTrigger { .. })
    {
        return true;
    }
    triggers_for(card_def)
        .iter()
        .any(|definition| (definition.effect)() == *effect)
        || card.saga.as_ref().is_some_and(|saga| {
            saga.chapter_effects
                .iter()
                .any(|chapter_effect| chapter_effect() == *effect)
        })
}

pub fn target_spec_for_trigger(card_def: u16, effect: &EffectOp) -> Option<TargetSpec> {
    // The monarch end-step draw trigger is engine-owned like Initiative's
    // Undercity trigger, but (unlike Avenging Hunter's fixed Initiative
    // source) its source is never one fixed card: Azure Fleet Admiral's ETB
    // grants it, but a later combat-damage transfer (`engine::
    // deal_combat_damage`) can rebind `EngineState::monarch_source` to
    // whichever creature's controller took the crown next. So this bypasses
    // the per-card `trigger_effect_matches`/`trigger_target_spec` dispatch
    // entirely rather than name-gating on one card the way the "Avenging
    // Hunter" branch below does -- the trigger never targets, regardless of
    // `card_def`.
    if matches!(effect, EffectOp::ResolveMonarchTrigger { .. }) {
        return Some(TargetSpec::None);
    }
    if !trigger_effect_matches(card_def, effect) {
        return None;
    }
    // Warp's delayed exile never targets, whatever its card's other
    // triggers do.
    if matches!(effect, EffectOp::WarpExileBoundObject { .. }) {
        return Some(TargetSpec::None);
    }
    let card = crate::card_def::CARD_DEFS.get(card_def as usize)?;
    if card.name == "Apothecary Stomper" {
        return Some(
            apothecary_stomper_modes()
                .iter()
                .find(|(_, branch)| branch == effect)
                .map_or(TargetSpec::None, |(spec, _)| *spec),
        );
    }
    #[cfg(feature = "standard-magezero-fixtures")]
    if card.name == "Hullbreaker Horror" {
        return Some(
            standard_family_g_v1::hullbreaker_horror_modes()
                .iter()
                .find(|(_, branch)| branch == effect)
                .map_or(TargetSpec::None, |(spec, _)| *spec),
        );
    }
    if card.name == "Sylvan Scavenging" {
        return Some(
            sylvan_scavenging_modes()
                .iter()
                .find(|(_, branch)| branch == effect)
                .map_or(TargetSpec::None, |(spec, _)| *spec),
        );
    }
    Some(
        if let Some(spec) = standard_trigger_target_spec(card.name, effect) {
            spec
        } else if card.name == "Mesmeric Fiend" && *effect == mesmeric_fiend_exile_effect() {
            TargetSpec::TargetOpponent
        } else if card.name == "Journey to Nowhere" && *effect == journey_to_nowhere_etb_effect() {
            TargetSpec::CreatureOtherThanSource
        } else if card.name == "Avenging Hunter" {
            match effect {
                EffectOp::ResolveInitiativeTrigger { binding } => match binding.kind {
                    InitiativeTriggerKindV1::UndercityRoom(UndercityRoomV1::Forge)
                    | InitiativeTriggerKindV1::UndercityRoom(UndercityRoomV1::Arena) => {
                        TargetSpec::Creature
                    }
                    InitiativeTriggerKindV1::UndercityRoom(UndercityRoomV1::Trap) => {
                        TargetSpec::AnyPlayer
                    }
                    _ => TargetSpec::None,
                },
                _ => trigger_target_spec(card_def),
            }
        } else {
            trigger_target_spec(card_def)
        },
    )
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PendingTrigger {
    pub controller: PlayerId,
    pub source: ObjectId,
    pub effect: EffectOp,
    /// True iff this is a Madness triggered-ability offer (`engine::
    /// apply_discard`'s Madness branch), not one of this module's
    /// card-def-matched triggers (`triggers_for`). Threaded through the
    /// same APNAP grouping/`Decision::OrderTriggers` machinery as any other
    /// trigger (603.3b makes no distinction), but `effect` is a meaningless
    /// placeholder (`EffectOp::Sequence(vec![])`) for one of these --
    /// `engine::push_trigger_onto_stack` reads this flag to leave the
    /// resulting `StackItem`'s `inline_effect` as `None` and set its own
    /// `madness_offer` instead (see that field's doc). Always `false` for a
    /// real `triggers_for`-matched trigger.
    pub is_madness_offer: bool,
    /// True iff this trigger's own `source` is the object that just
    /// resolved from a kicked cast (`engine::EngineState::
    /// pending_kicked_source`, consumed by `collect_and_process`) -- Goblin
    /// Bushwhacker's ETB trigger reads this via `engine::
    /// push_trigger_onto_stack` copying it onto the trigger's own
    /// `state::StackItem::kicked`, then `effect::ExecCtx::kicked` at
    /// resolution. `false` for every other trigger.
    pub kicked: bool,
    /// Definition-owned target shape selected while this trigger is put on
    /// the stack. Existing untargeted triggers retain `None`.
    #[serde(default)]
    pub target_spec: TargetSpec,
    /// Target prefix chosen while placing this trigger on the stack.
    #[serde(default)]
    pub targets: Vec<Target>,
    /// Exact target-incarnation contracts parallel to `targets`.
    #[serde(default)]
    pub target_contracts: Vec<StackTargetContractV4>,
    /// True after the trigger's simultaneous-controller group has either
    /// been ordered by its controller or proven singleton.
    #[serde(default)]
    pub placement_ordered: bool,
    /// Exact historical source incarnation captured when the trigger was
    /// created. This lets independent and linked abilities remain valid
    /// across later zone changes of the same physical card.
    #[serde(default)]
    pub source_contract: Option<AbilitySourceContractV4>,
    /// Exact Equipment incarnation that granted this trigger to `source`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub granted_by: Option<AbilitySourceContractV4>,
    /// Exact cast-scoped optional additional cost inherited by this ETB.
    /// This is absent for every trigger without an intervening-if cost gate.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub optional_additional_cost_paid: Option<OptionalAdditionalCostDef>,
    /// Historical physical-card payment provenance copied from the producing
    /// spell's stack incarnation.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub paid_cost_refs: Vec<PaidCostRefV4>,
}

pub(crate) fn creature_dies_to_state_based_actions(
    toughness: i32,
    marked_damage: i64,
    deathtouch_damage: bool,
    indestructible: bool,
) -> bool {
    toughness <= 0
        || ((marked_damage >= i64::from(toughness) || (marked_damage > 0 && deathtouch_damage))
            && !indestructible)
}

/// Runs state-based actions to a fixed point: repeat the full SBA sweep
/// until one pass makes no change (704.3).
pub fn sba_fixed_point(state: &mut GameState) {
    sba_fixed_point_with_protected_triggers(state, &[]);
}

pub(crate) fn saga_final_chapter_is_pending(
    state: &GameState,
    source: ObjectId,
    source_zone_change_count: u32,
    final_effect: &EffectOp,
    protected_triggers: &[PendingTrigger],
) -> bool {
    let matches_pending = |pending: &PendingTrigger| {
        pending.source == source
            && pending.effect == *final_effect
            && pending
                .source_contract
                .is_some_and(|contract| contract.zone_change_count == source_zone_change_count)
    };
    protected_triggers.iter().any(matches_pending)
        || state.engine.pending_triggers.iter().any(matches_pending)
        || state.stack.iter().any(|item| {
            item.source == source
                && item.kind == crate::state::StackItemKind::TriggeredAbility
                && item.inline_effect.as_ref() == Some(final_effect)
                && item
                    .v4
                    .ability_source_contract
                    .is_some_and(|contract| contract.zone_change_count == source_zone_change_count)
        })
        || state.engine.event_log.iter().any(|event| {
            matches!(
                event,
                CommittedEvent::SagaChapter {
                    source: event_source,
                    source_zone_change_count: event_generation,
                    chapter,
                    ..
                } if *event_source == source
                    && *event_generation == source_zone_change_count
                    && usize::from(*chapter)
                        == crate::card_def::CARD_DEFS[state.objects.get(source).card_def as usize]
                            .saga
                            .as_ref()
                            .map_or(0, |saga| saga.chapter_effects.len())
            )
        })
}

/// `CARD_DEFS[i].is_token`, packed densely so the per-SBA-pass token sweep
/// over every object does not stride through full `CardDef` entries.
fn token_card_defs() -> &'static [bool] {
    static TABLE: std::sync::OnceLock<Box<[bool]>> = std::sync::OnceLock::new();
    TABLE.get_or_init(|| {
        crate::card_def::CARD_DEFS
            .iter()
            .map(|card| card.is_token)
            .collect()
    })
}

fn sba_fixed_point_with_protected_triggers(
    state: &mut GameState,
    protected_triggers: &[PendingTrigger],
) {
    loop {
        if crate::legend_rule_v1::stage(state, protected_triggers) {
            return;
        }
        #[cfg(feature = "standard-magezero-fixtures")]
        {
            crate::standard_keywords_v1::start_your_engines(state);
            crate::standard_keywords_v1::sync_day_night(state);
        }
        let mut changed = false;

        // 704.5g: a creature with toughness 0 or less is put into its
        // owner's graveyard. 704.5h: a creature with lethal damage marked
        // is destroyed.
        let mut dying = Vec::new();
        for (id, obj) in state.objects.iter() {
            if obj.zone != Zone::Battlefield {
                continue;
            }
            if crate::planeswalker_v1::zero_loyalty(state, id) {
                dying.push(id);
                continue;
            }
            if !crate::engine::object_has_type(state, id, crate::card_def::CardType::Creature) {
                continue;
            }
            let toughness = crate::engine::effective_toughness(state, id);
            let indestructible = crate::engine::has_effective_keyword(
                state,
                id,
                crate::card_def::Keywords::INDESTRUCTIBLE,
            );
            // 704.5g is a put-into-graveyard action and ignores
            // indestructible. 704.5h destroys for lethal damage, so
            // indestructible prevents only that branch.
            if creature_dies_to_state_based_actions(
                toughness,
                i64::from(obj.damage),
                obj.v4.deathtouch_damage,
                indestructible,
            ) {
                dying.push(id);
            }
        }
        for id in dying {
            crate::event::commit(
                state,
                crate::event::ProposedEvent::zone_change(id, Zone::Graveyard),
            );
            changed = true;
        }

        // 704.5m: an Aura attached to an illegal object or player, or not
        // attached at all, is put into its owner's graveyard. The current
        // attachment grammar supports creature Auras only and requires both
        // exact-incarnation directions of the relation to agree.
        let invalid_auras = state
            .objects
            .iter()
            .filter_map(|(id, aura)| {
                if aura.zone != Zone::Battlefield {
                    return None;
                }
                let definition = &crate::card_def::CARD_DEFS[aura.card_def as usize];
                if !definition
                    .attachment
                    .is_some_and(crate::card_def::AttachmentDef::is_creature_aura)
                {
                    return None;
                }
                let valid = aura.v4.attached_to.is_some_and(|link| {
                    state.objects.try_get(link.object).is_some_and(|host| {
                        host.zone == Zone::Battlefield
                            && host.zone_change_count == link.zone_change_count
                            && crate::engine::object_has_type(
                                state,
                                link.object,
                                crate::card_def::CardType::Creature,
                            )
                            && host.attachments.contains(&id)
                    })
                });
                (!valid).then_some(id)
            })
            .collect::<Vec<_>>();
        for id in invalid_auras {
            crate::event::commit(
                state,
                crate::event::ProposedEvent::zone_change(id, Zone::Graveyard),
            );
            changed = true;
        }

        // 704.5s: after a Saga's final chapter ability leaves the stack, its
        // controller sacrifices it. The chapter trigger is protected across
        // the pre-placement SBA checkpoint by its exact source incarnation.
        let completed_sagas = state
            .objects
            .iter()
            .filter_map(|(id, object)| {
                if object.zone != Zone::Battlefield || object.v4.face_index != 0 {
                    return None;
                }
                let saga = crate::card_def::CARD_DEFS[object.card_def as usize]
                    .saga
                    .as_ref()?;
                if object.counters.lore < saga.chapter_effects.len() as i16 {
                    return None;
                }
                let final_effect = (saga.chapter_effects.last().copied()?)();
                (!saga_final_chapter_is_pending(
                    state,
                    id,
                    object.zone_change_count,
                    &final_effect,
                    protected_triggers,
                ))
                .then_some(id)
            })
            .collect::<Vec<_>>();
        for id in completed_sagas {
            crate::event::commit(
                state,
                crate::event::ProposedEvent::zone_change(id, Zone::Graveyard),
            );
            changed = true;
        }

        // 704.5a: a player with 0 or less life loses. 704.5b: a player who
        // attempted to draw from an empty library loses. 704.5c: a player
        // with ten or more poison counters loses.
        for p in [PlayerId::P0, PlayerId::P1] {
            let ps = &mut state.players[p.index()];
            if !ps.has_lost && (ps.life <= 0 || ps.drew_from_empty || ps.poison_counters.0 >= 10) {
                ps.has_lost = true;
                changed = true;
            }
        }

        // 111.8/704.5d: a token in any zone other than the battlefield
        // ceases to exist -- most commonly a sacrificed/died Blood Token
        // ending up back in the graveyard's card-count for the rest of the
        // game, which it never does in a real game (root-caused against
        // the real v3 corpus: `kernel_gy` carrying a stray "Blood Token"
        // entry the trace's own graveyard snapshot never has, many turns
        // after the token was created and then activated/sacrificed).
        let token_defs = token_card_defs();
        let leaving: Vec<ObjectId> = state
            .objects
            .iter()
            .filter(|(_, obj)| obj.zone != Zone::Battlefield && token_defs[obj.card_def as usize])
            .map(|(id, _)| id)
            .collect();
        for id in leaving {
            if crate::event::cease_to_exist(state, id) {
                changed = true;
            }
        }

        if !changed {
            break;
        }
    }
}

/// Matches the events already in `state.engine.event_log`, runs SBAs to a
/// fixed point, then matches any events created by those SBAs. Returns the
/// combined newly-triggered abilities in APNAP order (active player's
/// triggers first).
pub fn collect_and_process(state: &mut GameState) -> Vec<PendingTrigger> {
    collect_and_process_with_waiting(state, Vec::new())
}

pub(crate) fn collect_and_process_with_waiting(
    state: &mut GameState,
    mut waiting: Vec<PendingTrigger>,
) -> Vec<PendingTrigger> {
    if state.pending_legend_rule_v1.is_some() {
        return Vec::new();
    }
    match crate::life_gain_turn_v1::take_captures(state) {
        Ok(captures) => waiting.extend(captures),
        Err(source) => {
            state.engine.halted = Some((
                crate::engine::UnsupportedMechanic::InvalidFirstLifeGainHistory,
                source,
            ));
            return Vec::new();
        }
    }
    let events: Vec<CommittedEvent> = state.engine.event_log.drain(..).collect();
    // Single-shot: `engine::resolve_top_of_stack` set this immediately
    // before the resolution whose events we're about to match, explicitly
    // (`Some`/`None`) every single time -- taking it here means it can never
    // carry over into a later, unrelated `collect_and_process` call (see
    // `EngineState::pending_kicked_source`'s doc).
    let kicked_source = state.engine.pending_kicked_source.take();
    #[cfg(feature = "standard-magezero-fixtures")]
    crate::standard_keywords_v1::note_life_loss(state, &events);

    // Trigger conditions are evaluated at the moment their event happens,
    // before the following SBA check (603.2/704.3). In particular, an ETB
    // trigger must not disappear merely because its source dies during that
    // check, so match the pre-SBA batch while its sources still occupy the
    // zones from which their abilities function.
    waiting.extend(triggers_from_events(state, &events, kicked_source));
    let mut new_triggers = waiting;

    // Conversely, SBAs can create new trigger events themselves. Lethal
    // combat damage, for example, moves Clockwork Percussionist to the
    // graveyard here; its dies trigger belongs to this same trigger-placement
    // checkpoint, not some later action. Match that second batch only after
    // the fixed point, then order both batches together under 603.3b.
    sba_fixed_point_with_protected_triggers(state, &new_triggers);
    if state.pending_legend_rule_v1.is_some() {
        return Vec::new();
    }
    let sba_events: Vec<CommittedEvent> = state.engine.event_log.drain(..).collect();
    #[cfg(feature = "standard-magezero-fixtures")]
    crate::standard_keywords_v1::note_life_loss(state, &sba_events);
    new_triggers.extend(triggers_from_events(state, &sba_events, None));

    // 603.3d: a triggered ability requiring targets is removed from the
    // stack-placement queue when no complete legal target assignment exists.
    // This check belongs after the SBA fixed point, at the actual placement
    // checkpoint, rather than at the earlier event-matching snapshot. The
    // targets must be legal for the trigger's own source (protection,
    // "other than this"), the same set its target decision will offer.
    new_triggers
        .retain(|pending| crate::engine::pending_trigger_targets_can_complete(pending, state));

    order_apnap(new_triggers, state.active_player)
}

/// Per-definition flag: whether an object of this definition can produce a
/// trigger in `triggers_from_events`'s per-object pass (a Saga chapter, a
/// definition-owned triggered ability, or a generic ward cost). Indexed like
/// `CARD_DEFS`.
fn card_defs_with_event_triggers() -> &'static [bool] {
    static TABLE: std::sync::OnceLock<Box<[bool]>> = std::sync::OnceLock::new();
    TABLE.get_or_init(|| {
        crate::card_def::CARD_DEFS
            .iter()
            .enumerate()
            .map(|(index, card)| {
                card.is_executable()
                    && (card.saga.is_some()
                        || !triggers_for(index as u16).is_empty()
                        || card.ward_cost.is_some())
            })
            .collect()
    })
}

fn triggers_from_events(
    state: &mut GameState,
    events: &[CommittedEvent],
    kicked_source: Option<ObjectId>,
) -> Vec<PendingTrigger> {
    let mut uses = state.trigger_uses_v1.clone().unwrap_or_default();
    uses.retain(|entry| {
        entry.turn == state.turn
            && entry.active_player == state.active_player
            && state
                .objects
                .try_get(entry.source.object)
                .is_some_and(|object| {
                    object.zone == Zone::Battlefield
                        && object.zone_change_count == entry.source.zone_change_count
                })
    });
    if events.is_empty() {
        // Nothing can match an empty batch; only the use-ledger pruning
        // above is observable.
        uses.sort_by_key(|entry| (entry.source.object, entry.ability_index));
        state.trigger_uses_v1 = (!uses.is_empty()).then_some(uses);
        return Vec::new();
    }
    let draws_this_turn_at = draws_this_turn_snapshot(events, state);
    let mut new_triggers = Vec::new();
    let may_trigger = card_defs_with_event_triggers();
    for (id, obj) in state.objects.iter() {
        // Most objects (lands, vanilla creatures, every library card of a
        // definition with no triggered ability) can never match; skip them
        // without touching their large `CardDef` entry.
        if !may_trigger[obj.card_def as usize] {
            continue;
        }
        let card = &crate::card_def::CARD_DEFS[obj.card_def as usize];
        if !card.is_executable() {
            continue;
        }
        if let Some(saga) = card.saga.as_ref() {
            for ev in events {
                let CommittedEvent::SagaChapter {
                    source,
                    source_zone_change_count,
                    controller,
                    chapter,
                } = ev
                else {
                    continue;
                };
                if *source != id
                    || *chapter == 0
                    || usize::from(*chapter) > saga.chapter_effects.len()
                    || obj.zone_change_count < *source_zone_change_count
                    || (obj.zone_change_count == *source_zone_change_count
                        && (obj.zone != Zone::Battlefield || obj.v4.face_index != 0))
                {
                    continue;
                }
                let effect = (saga.chapter_effects[usize::from(*chapter - 1)])();
                new_triggers.push(PendingTrigger {
                    controller: *controller,
                    source: id,
                    target_spec: target_spec_for_trigger(obj.card_def, &effect)
                        .expect("Saga chapter effect is definition-owned"),
                    effect,
                    is_madness_offer: false,
                    kicked: false,
                    targets: Vec::new(),
                    target_contracts: Vec::new(),
                    placement_ordered: false,
                    source_contract: Some(AbilitySourceContractV4 {
                        source: id,
                        card_def: obj.card_def,
                        owner: obj.owner,
                        controller: *controller,
                        zone: Zone::Battlefield,
                        zone_change_count: *source_zone_change_count,
                        attached_to: None,
                    }),
                    granted_by: None,
                    optional_additional_cost_paid: None,
                    paid_cost_refs: Vec::new(),
                });
            }
        }
        for (ability_index, def) in triggers_for(obj.card_def).iter().enumerate() {
            // These abilities were captured at the actual life-gain commit,
            // before later operations can change battlefield membership.
            if matches!(
                def.condition,
                TriggerCondition::ControllerFirstLifeGain { .. }
            ) {
                continue;
            }
            let uses_leave_lki = matches!(
                def.condition,
                TriggerCondition::LeftBattlefieldToGraveyard
                    | TriggerCondition::LeftBattlefield
                    | TriggerCondition::DiesWithoutCounters
            );
            // Enduring keeps a graveyard-incarnation binding for its return,
            // but its death ability and controller come from the battlefield.
            let uses_death_lki =
                uses_leave_lki || def.condition == TriggerCondition::DiesIfWasCreature;
            if !uses_leave_lki
                && (obj.zone != def.home_zone
                    || !crate::continuous_characteristics_v1::printed_abilities_active(state, id))
            {
                continue;
            }
            // A `TriggeredAbilityDef` names one printed face's ability text
            // (Delver of Secrets: the trigger prints on the front half only,
            // Insectile Aberration carries none). `face_index` can only be
            // nonzero for a battlefield permanent with a `transform_face`,
            // and any zone change resets it to 0 (`reset_for_zone_change`),
            // so this is a no-op for every `uses_leave_lki`/non-battlefield
            // home zone case; it mirrors the same "front face only"
            // exclusion the Saga chapter/completion paths already apply
            // unconditionally (`obj.v4.face_index != 0` at this file's own
            // SBA and chapter-matching sites).
            if obj.v4.face_index != def.face_index {
                continue;
            }
            for (i, ev) in events.iter().enumerate() {
                if uses_death_lki && i.checked_sub(1).and_then(|index| events.get(index)).is_some_and(|event| matches!(event, CommittedEvent::PrintedAbilitiesRemovedBeforeZoneChange { object, .. } if *object == id)) {
                    continue;
                }
                let event_controller = match ev {
                    CommittedEvent::ZoneChange {
                        object,
                        from: Zone::Battlefield,
                        controller_before,
                        ..
                    } if *object == id && uses_death_lki => *controller_before,
                    _ => obj.controller,
                };
                if trigger_matches(
                    def.condition,
                    ev,
                    id,
                    event_controller,
                    state,
                    draws_this_turn_at[i],
                ) {
                    let kicked = Some(id) == kicked_source;
                    if def.intervening_if_kicked && !kicked {
                        continue;
                    }
                    if def.intervening_if_controls_another_source_card {
                        let source_def = obj.card_def;
                        let controls_another = state.players[event_controller.index()]
                            .battlefield
                            .iter()
                            .copied()
                            .any(|other| {
                                other != id && state.objects.get(other).card_def == source_def
                            });
                        if !controls_another {
                            continue;
                        }
                    }
                    let effect = if card.name == "Moon-Circuit Hacker"
                        && matches!(def.condition, TriggerCondition::DealsCombatDamageToPlayer)
                    {
                        moon_circuit_hacker_combat_effect_for_entered_this_turn(
                            obj.v4.entered_battlefield_turn == Some(state.turn),
                        )
                    } else {
                        #[cfg(feature = "standard-magezero-fixtures")]
                        if card.name == "Quirion Beastcaller"
                            && matches!(def.condition, TriggerCondition::LeftBattlefieldToGraveyard)
                        {
                            standard_family_g_v1::quirion_beastcaller_dies_effect(
                                state,
                                id,
                                event_controller,
                            )
                        } else {
                            materialize_trigger_event_effect(def, id, state, ev)
                        }
                        #[cfg(not(feature = "standard-magezero-fixtures"))]
                        materialize_trigger_event_effect(def, id, state, ev)
                    };
                    let required_optional_cost =
                        required_optional_additional_cost_for_trigger(obj.card_def, &effect);
                    let (paid_optional_cost, paid_cost_refs) =
                        if let Some(required) = required_optional_cost {
                            let matching = events
                                .iter()
                                .filter_map(|event| match event {
                                    CommittedEvent::OptionalAdditionalCostPaid {
                                        source,
                                        kind,
                                        paid_cost_refs,
                                    } if *source == id => Some((*kind, paid_cost_refs.clone())),
                                    _ => None,
                                })
                                .collect::<Vec<_>>();
                            let [(kind, refs)] = matching.as_slice() else {
                                continue;
                            };
                            if *kind != required {
                                continue;
                            }
                            (Some(required), refs.clone())
                        } else {
                            (None, Vec::new())
                        };
                    let target_spec =
                        target_spec_for_trigger(obj.card_def, &effect).unwrap_or(TargetSpec::None);
                    if let Some(maximum) = per_turn_trigger_cap(def.condition) {
                        let ability_index =
                            u16::try_from(ability_index).expect("bounded definition abilities");
                        let source = crate::state::ObjectLinkV4 {
                            object: id,
                            zone_change_count: obj.zone_change_count,
                        };
                        if let Some(entry) = uses.iter_mut().find(|entry| {
                            entry.source == source && entry.ability_index == ability_index
                        }) {
                            if entry.uses >= maximum {
                                continue;
                            }
                            entry.uses += 1;
                        } else {
                            if maximum == 0 {
                                continue;
                            }
                            uses.push(crate::state::TriggerUseV1 {
                                source,
                                ability_index,
                                turn: state.turn,
                                active_player: state.active_player,
                                uses: 1,
                            });
                        }
                    }
                    let source_contract = match ev {
                        CommittedEvent::ZoneChange {
                            object,
                            from,
                            controller_before,
                            ..
                        } if *object == id && uses_leave_lki => {
                            let Some(zone_change_count) = obj.zone_change_count.checked_sub(1)
                            else {
                                continue;
                            };
                            Some(AbilitySourceContractV4 {
                                source: id,
                                card_def: obj.card_def,
                                owner: obj.owner,
                                controller: *controller_before,
                                zone: *from,
                                zone_change_count,
                                attached_to: None,
                            })
                        }
                        _ if obj.zone == Zone::Stack => None,
                        _ => {
                            let mut contract = AbilitySourceContractV4::capture(state, id);
                            contract.controller = event_controller;
                            Some(contract)
                        }
                    };
                    new_triggers.push(PendingTrigger {
                        controller: event_controller,
                        source: id,
                        granted_by: None,
                        effect,
                        is_madness_offer: false,
                        kicked,
                        target_spec,
                        targets: Vec::new(),
                        target_contracts: Vec::new(),
                        placement_ordered: false,
                        source_contract,
                        optional_additional_cost_paid: paid_optional_cost,
                        paid_cost_refs,
                    });
                }
            }
        }
        if obj.zone == Zone::Battlefield
            && crate::continuous_characteristics_v1::printed_abilities_active(state, id)
        {
            let ward_cost = card.ward_cost.filter(|cost| match cost {
                crate::card_def::WardCostDef::BackFacePayLife(_) => obj.v4.face_index == 1,
                crate::card_def::WardCostDef::DiscardCard => true,
                _ => obj.v4.face_index == 0,
            });
            if let Some(ward_cost) = ward_cost {
                for event in events {
                    let CommittedEvent::Targeted {
                        target,
                        target_zone_change_count,
                        targeting_stack_item,
                        targeting_controller,
                    } = event
                    else {
                        continue;
                    };
                    if *target != id
                        || *target_zone_change_count != obj.zone_change_count
                        || *targeting_controller == obj.controller
                    {
                        continue;
                    }
                    let ward_target = crate::state::StackTargetContractV4::capture(
                        state,
                        crate::state::Target::Object(id),
                    );
                    new_triggers.push(PendingTrigger {
                        controller: obj.controller,
                        source: id,
                        granted_by: None,
                        effect: match ward_cost {
                            crate::card_def::WardCostDef::Generic(generic) => {
                                EffectOp::CounterUnlessPaysGeneric {
                                    ward_target,
                                    targeting_stack_item: *targeting_stack_item,
                                    generic,
                                }
                            }
                            crate::card_def::WardCostDef::CollectEvidence(minimum_mana_value) => {
                                EffectOp::CounterUnlessCollectsEvidence {
                                    ward_target,
                                    targeting_stack_item: *targeting_stack_item,
                                    minimum_mana_value,
                                }
                            }
                            crate::card_def::WardCostDef::DiscardCard => {
                                EffectOp::CounterUnlessDiscardsCard {
                                    ward_target,
                                    targeting_stack_item: *targeting_stack_item,
                                }
                            }
                            crate::card_def::WardCostDef::BackFacePayLife(life) => {
                                EffectOp::CounterUnlessPaysLife {
                                    ward_target,
                                    targeting_stack_item: *targeting_stack_item,
                                    life,
                                }
                            }
                        },
                        is_madness_offer: false,
                        kicked: false,
                        target_spec: TargetSpec::None,
                        targets: Vec::new(),
                        target_contracts: Vec::new(),
                        placement_ordered: false,
                        source_contract: Some(AbilitySourceContractV4::capture(state, id)),
                        optional_additional_cost_paid: None,
                        paid_cost_refs: Vec::new(),
                    });
                }
            }
        }
    }

    // Ward granted by another permanent (Coppercoat Vanguard) is the
    // warded creature's own ability, one trigger per grant.
    #[cfg(feature = "standard-magezero-fixtures")]
    for event in events {
        let CommittedEvent::Targeted {
            target,
            target_zone_change_count,
            targeting_stack_item,
            targeting_controller,
        } = event
        else {
            continue;
        };
        let Some(live) = state.objects.try_get(*target) else {
            continue;
        };
        if live.zone != Zone::Battlefield
            || live.zone_change_count != *target_zone_change_count
            || *targeting_controller == live.controller
        {
            continue;
        }
        let controller = live.controller;
        for generic in crate::standard_statics_v1::granted_ward_generics(state, *target) {
            let ward_target = crate::state::StackTargetContractV4::capture(
                state,
                crate::state::Target::Object(*target),
            );
            new_triggers.push(PendingTrigger {
                controller,
                source: *target,
                granted_by: None,
                effect: EffectOp::CounterUnlessPaysGeneric {
                    ward_target,
                    targeting_stack_item: *targeting_stack_item,
                    generic,
                },
                is_madness_offer: false,
                kicked: false,
                target_spec: TargetSpec::None,
                targets: Vec::new(),
                target_contracts: Vec::new(),
                placement_ordered: false,
                source_contract: Some(AbilitySourceContractV4::capture(state, *target)),
                optional_additional_cost_paid: None,
                paid_cost_refs: Vec::new(),
            });
        }
    }

    // Attachment-granted abilities belong to the equipped creature. Each
    // Equipment grants a separate trigger, and its exact incarnation is
    // carried independently as provenance.
    for event in events {
        let CommittedEvent::SpellCast {
            spell,
            controller: caster,
        } = event
        else {
            continue;
        };
        if selected_spell_types(state, *spell).contains(&CardType::Creature) {
            continue;
        }
        for (host, host_live) in state.objects.iter() {
            if host_live.controller != *caster
                || host_live.zone != Zone::Battlefield
                || !crate::engine::object_has_type(state, host, CardType::Creature)
            {
                continue;
            }
            for &equipment in &host_live.attachments {
                let equipment_live = state.objects.get(equipment);
                let exact_relation = equipment_live.v4.attached_to.is_some_and(|link| {
                    link.object == host
                        && link.zone_change_count == host_live.zone_change_count
                        && equipment_live.zone == Zone::Battlefield
                });
                let Some(profile) =
                    crate::card_def::CARD_DEFS[equipment_live.card_def as usize].equipment
                else {
                    continue;
                };
                if !exact_relation
                    || profile.noncreature_spell_damage_to_each_opponent == 0
                    || !crate::continuous_characteristics_v1::grant_survives(
                        state,
                        host,
                        equipment_live.v4.layer_timestamp.unwrap_or(0),
                    )
                {
                    continue;
                }
                new_triggers.push(PendingTrigger {
                    controller: *caster,
                    source: host,
                    source_contract: Some(AbilitySourceContractV4::capture(state, host)),
                    granted_by: Some(AbilitySourceContractV4::capture(state, equipment)),
                    effect: EffectOp::DealDamage {
                        target: TargetRef::Opponent,
                        amount: i32::from(profile.noncreature_spell_damage_to_each_opponent),
                    },
                    is_madness_offer: false,
                    kicked: false,
                    target_spec: TargetSpec::None,
                    targets: Vec::new(),
                    target_contracts: Vec::new(),
                    placement_ordered: false,
                    optional_additional_cost_paid: None,
                    paid_cost_refs: Vec::new(),
                });
            }
        }
    }

    for event in events {
        let CommittedEvent::InitiativeTrigger { binding } = event else {
            continue;
        };
        let binding = *binding;
        let effect = EffectOp::ResolveInitiativeTrigger { binding };
        let target_spec =
            target_spec_for_trigger(binding.source.card_def, &effect).unwrap_or(TargetSpec::None);
        new_triggers.push(PendingTrigger {
            controller: binding.player,
            source: binding.source.source,
            effect,
            is_madness_offer: false,
            kicked: false,
            target_spec,
            targets: Vec::new(),
            target_contracts: Vec::new(),
            placement_ordered: false,
            source_contract: Some(binding.source),
            granted_by: None,
            optional_additional_cost_paid: None,
            paid_cost_refs: Vec::new(),
        });
    }

    for event in events {
        let CommittedEvent::MonarchTrigger { binding } = event else {
            continue;
        };
        let binding = *binding;
        new_triggers.push(PendingTrigger {
            controller: binding.player,
            source: binding.source.source,
            effect: EffectOp::ResolveMonarchTrigger { binding },
            is_madness_offer: false,
            kicked: false,
            target_spec: TargetSpec::None,
            targets: Vec::new(),
            target_contracts: Vec::new(),
            placement_ordered: false,
            source_contract: Some(binding.source),
            granted_by: None,
            optional_additional_cost_paid: None,
            paid_cost_refs: Vec::new(),
        });
    }

    uses.sort_by_key(|entry| (entry.source.object, entry.ability_index));
    state.trigger_uses_v1 = (!uses.is_empty()).then_some(uses);
    new_triggers
}

fn selected_spell_types(state: &GameState, spell: ObjectId) -> &'static [CardType] {
    let object = state.objects.get(spell);
    let definition = &crate::card_def::CARD_DEFS[object.card_def as usize];
    if object
        .v4
        .spell_cast_origin
        .and_then(|origin| origin.finalized_method)
        == Some(crate::state::CastMethodV4::Omen)
    {
        // `CastMethodV4::Omen` is shared between a real Omen card's
        // alternative form and an Adventure card's named spell (Fang
        // Dragon's Forktail Sweep) -- see `engine::supported_adventure`'s
        // doc. `CardDef::adventure`/`CardDef::omen` are mutually exclusive
        // per card, so check the Adventure definition first.
        crate::engine::supported_adventure(definition)
            .map(|adventure| adventure.types)
            .or_else(|| crate::engine::supported_omen(definition).map(|omen| omen.types))
            .unwrap_or(&[])
    } else {
        definition.types
    }
}

fn selected_spell_has_subtype(state: &GameState, spell: ObjectId, subtype: Subtype) -> bool {
    let object = state.objects.get(spell);
    if object
        .v4
        .spell_cast_origin
        .and_then(|origin| origin.finalized_method)
        == Some(crate::state::CastMethodV4::Omen)
    {
        // Supported Adventure/Omen spell faces have no subtypes. Their
        // printed creature face must not supply subtype authority.
        false
    } else {
        subtype.is_in_subtype_ids(&crate::engine::effective_subtype_ids(state, spell))
    }
}

/// For each event in `events` (already committed, in commit order), the
/// value `draws_this_turn` genuinely held *at the moment that specific
/// event was committed* -- not `state`'s current (post-batch) value.
///
/// Root-caused against `game_20260713_002147_0002.txt`: `EffectOp::
/// DrawCards`'s loop (Faithless Looting's "draw two cards") commits both
/// draws into `event_log` before `collect_and_process` ever runs (both
/// resolve as one atomic ability, 608.2h -- correctly so; a trigger check
/// belongs *after* the whole ability resolves, not spliced mid-resolution,
/// 603.3/704.3), so by the time this function inspects `state`, `draws_
/// this_turn` already reflects *both* draws. Checking every event in the
/// batch against that single final value made `TriggerCondition::
/// DrawNth(3)` (Sneaky Snacker) miss entirely whenever a 2-draw batch
/// jumped straight over 3 (2 -> 4): neither event's *own* moment (3, then
/// 4) was ever actually tested, only the batch's final value (4) tested
/// twice. 608.2h itself settles that each drawn card is still a distinct,
/// sequential event ("the player draws that many cards, in that order"),
/// so a "your Nth draw this turn" condition must see each one's own true
/// historical count -- reconstructed here (not threaded through
/// `CommittedEvent::Draw` itself, which stays a plain, serializable
/// snapshot-free record) by walking `events` forward per player: the
/// count immediately *before* this batch is `state`'s current value minus
/// however many of this player's own `Draw` events are in this same
/// batch, then each of that player's `Draw` events in commit order adds
/// exactly 1.
fn draws_this_turn_snapshot(events: &[CommittedEvent], state: &GameState) -> Vec<u32> {
    let batch_draws = |p: PlayerId| {
        events
            .iter()
            .filter(
                |e| matches!(e, CommittedEvent::Draw { player, object: Some(_) } if *player == p),
            )
            .count() as u32
    };
    let mut running = [
        state.players[0].draws_this_turn - batch_draws(PlayerId::P0),
        state.players[1].draws_this_turn - batch_draws(PlayerId::P1),
    ];
    events
        .iter()
        .map(|ev| match ev {
            CommittedEvent::Draw {
                player,
                object: Some(_),
            } => {
                running[player.index()] += 1;
                running[player.index()]
            }
            _ => 0, // unused by any non-DrawNth trigger_matches arm
        })
        .collect()
}

/// `draws_this_turn_at_event`: only meaningful for a `CommittedEvent::Draw`
/// (see `draws_this_turn_snapshot`'s doc for why this can't just read
/// `state` live) -- unused, and irrelevant, for every other event/condition
/// pairing.
/// How many times per turn a triggered ability may trigger, tracked in
/// `GameState::trigger_uses_v1`. `None` means unlimited.
fn per_turn_trigger_cap(condition: TriggerCondition) -> Option<u16> {
    match condition {
        TriggerCondition::ControllerAddedPlusOneCountersToSelf { max_per_turn } => max_per_turn,
        TriggerCondition::BecomesTargetOfControllerSpellOrAbilityFirstTimeEachTurn => Some(1),
        TriggerCondition::OtherControlledCreatureWithPowerAtMostEntersOncePerTurn(_) => Some(1),
        _ => None,
    }
}

fn trigger_matches(
    cond: TriggerCondition,
    ev: &CommittedEvent,
    source: ObjectId,
    controller: PlayerId,
    state: &GameState,
    draws_this_turn_at_event: u32,
) -> bool {
    match (cond, ev) {
        (TriggerCondition::ControllerGainsLife, CommittedEvent::LifeGain { player, amount }) => {
            *player == controller && *amount > 0
        }
        (
            TriggerCondition::ControllerAddedPlusOneCountersToSelf { .. },
            CommittedEvent::PlusOneCountersAdded {
                object,
                zone_change_count,
                player,
                count,
            },
        ) => {
            *object == source
                && *zone_change_count == state.objects.get(source).zone_change_count
                && *player == controller
                && *count > 0
        }
        (TriggerCondition::ControlledLandEnters, event) => battlefield_entry_object(event)
            .is_some_and(|object| {
                state.objects.get(object).controller == controller
                    && crate::engine::object_has_type(state, object, CardType::Land)
            }),
        (
            TriggerCondition::ControllerDraws,
            CommittedEvent::Draw {
                player,
                object: Some(_),
            },
        ) => *player == controller,
        (
            TriggerCondition::CastCreatureSpell,
            CommittedEvent::SpellCast {
                spell,
                controller: caster,
            },
        ) => {
            *caster == controller
                && selected_spell_types(state, *spell)
                    .contains(&crate::card_def::CardType::Creature)
        }
        (
            TriggerCondition::CastSpell,
            CommittedEvent::SpellCast {
                controller: caster, ..
            },
        ) => *caster == controller,
        (
            TriggerCondition::CastSpellDuringOpponentsTurn,
            CommittedEvent::SpellCast {
                controller: caster, ..
            },
        ) => *caster == controller && state.active_player != controller,
        (
            TriggerCondition::CastSpellManaValueAtLeast(minimum),
            CommittedEvent::SpellCast {
                spell,
                controller: caster,
            },
        ) => {
            *caster == controller
                && state
                    .stack
                    .iter()
                    .find(|item| {
                        item.kind == crate::state::StackItemKind::Spell && item.source == *spell
                    })
                    .is_some_and(|item| {
                        crate::engine::stack_spell_mana_value(state, item) >= minimum
                    })
        }
        (TriggerCondition::ControlledCreatureEntersOutgrowingSource { another }, event) => {
            let Some(object) = battlefield_entry_object(event) else {
                return false;
            };
            let entrant = state.objects.get(object);
            (!another || object != source)
                && entrant.zone == Zone::Battlefield
                && entrant.controller == controller
                && crate::engine::object_has_type(state, object, CardType::Creature)
                && (crate::engine::effective_power(state, object)
                    > crate::engine::effective_power(state, source)
                    || crate::engine::effective_toughness(state, object)
                        > crate::engine::effective_toughness(state, source))
        }
        (
            TriggerCondition::OpponentDraws,
            CommittedEvent::Draw {
                player,
                object: Some(_),
            },
        ) => *player != controller,
        (TriggerCondition::OtherControlledCreatureEnters { subtype }, event) => {
            let Some(object) = battlefield_entry_object(event) else {
                return false;
            };
            let entrant = state.objects.get(object);
            object != source
                && entrant.controller == controller
                && crate::engine::object_has_type(state, object, CardType::Creature)
                && subtype.is_none_or(|subtype| {
                    subtype.is_in_subtype_ids(&crate::engine::effective_subtype_ids(state, object))
                })
        }
        (
            TriggerCondition::Etb,
            CommittedEvent::ZoneChange {
                object,
                to: Zone::Battlefield,
                ..
            },
        ) => *object == source,
        (
            TriggerCondition::EtbControlsOtherSubtypeCount {
                subtype,
                minimum_count,
            },
            CommittedEvent::ZoneChange {
                object,
                to: Zone::Battlefield,
                ..
            },
        ) => {
            if *object != source {
                return false;
            }
            let count = state.players[controller.index()]
                .battlefield
                .iter()
                .copied()
                .filter(|candidate| *candidate != source)
                .filter(|candidate| {
                    subtype
                        .is_in_subtype_ids(&crate::engine::effective_subtype_ids(state, *candidate))
                })
                .count();
            count >= usize::from(minimum_count)
        }
        (
            TriggerCondition::EtbIfGraveyardCreatureCardsAtLeast(minimum_count),
            CommittedEvent::ZoneChange {
                object,
                to: Zone::Battlefield,
                ..
            },
        ) => {
            if *object != source {
                return false;
            }
            let count = state.players[controller.index()]
                .graveyard
                .iter()
                .filter(|&&id| {
                    let candidate = state.objects.get(id);
                    !candidate.v4.is_token
                        && crate::card_def::CARD_DEFS[candidate.card_def as usize]
                            .has_type(crate::card_def::CardType::Creature)
                })
                .count();
            count >= usize::from(minimum_count)
        }
        (
            TriggerCondition::Attacks,
            CommittedEvent::DeclaredAttacker {
                source: event_source,
                source_zone_change_count,
                controller: event_controller,
            },
        ) => {
            *event_source == source
                && *event_controller == controller
                && state.objects.get(source).zone_change_count == *source_zone_change_count
        }
        (
            TriggerCondition::ControllerAttacks,
            CommittedEvent::ControllerAttacked {
                source: event_source,
                source_zone_change_count,
                controller: event_controller,
            },
        ) => {
            *event_source == source
                && *event_controller == controller
                && state.objects.get(source).zone_change_count == *source_zone_change_count
        }
        (
            TriggerCondition::ControllerAttacksWithAtLeastCreatures(minimum),
            CommittedEvent::ControllerAttacked {
                source: event_source,
                source_zone_change_count,
                controller: event_controller,
            },
        ) => {
            *event_source == source
                && *event_controller == controller
                && state.objects.get(source).zone_change_count == *source_zone_change_count
                && state.engine.combat.attackers.len() >= usize::from(minimum)
        }
        (
            TriggerCondition::ControllerAttacksWithSubtype(subtype),
            CommittedEvent::ControllerAttacked {
                source: event_source,
                source_zone_change_count,
                controller: event_controller,
            },
        ) => {
            *event_source == source
                && *event_controller == controller
                && state.objects.get(source).zone_change_count == *source_zone_change_count
                && state.engine.combat.attackers.iter().any(|&attacker| {
                    state.objects.get(attacker).controller == controller
                        && crate::engine::has_effective_subtype(state, attacker, subtype)
                })
        }
        (
            TriggerCondition::BeginningControllerEndStepIfCreatureDied,
            CommittedEvent::BeginningEndStep {
                active_player,
                creature_died_this_turn,
            },
        ) => *active_player == controller && *creature_died_this_turn,
        (
            TriggerCondition::BeginningControllerEndStep,
            CommittedEvent::BeginningEndStep { active_player, .. },
        ) => *active_player == controller,
        (TriggerCondition::BeginningEndStepAfterWarp, CommittedEvent::BeginningEndStep { .. }) => {
            state.objects.get(source).v4.warped_v1
        }
        (TriggerCondition::DealsDamage, CommittedEvent::Damage { source: s, .. }) => *s == source,
        (
            TriggerCondition::AttacksWithControllerGraveyardCardCountAtLeast(minimum),
            CommittedEvent::DeclaredAttacker {
                source: event_source,
                source_zone_change_count,
                controller: event_controller,
            },
        ) => {
            *event_source == source
                && *event_controller == controller
                && state.objects.get(source).zone_change_count == *source_zone_change_count
                && crate::effect::controller_graveyard_card_count(state, controller)
                    >= usize::from(minimum)
        }
        (
            TriggerCondition::DealsCombatDamageToPlayer,
            CommittedEvent::CombatDamageToPlayer {
                source: event_source,
                source_zone_change_count,
                ..
            },
        ) => {
            *event_source == source
                && state.objects.get(source).zone_change_count == *source_zone_change_count
        }
        (
            TriggerCondition::EquippedCreatureDealsCombatDamageToPlayer,
            CommittedEvent::CombatDamageToPlayer {
                source: event_source,
                source_zone_change_count,
                ..
            },
        ) => {
            state.objects.get(source).v4.attached_to
                == Some(crate::state::ObjectLinkV4 {
                    object: *event_source,
                    zone_change_count: *source_zone_change_count,
                })
        }
        (
            TriggerCondition::CastInstantOrSorcery,
            CommittedEvent::SpellCast {
                spell,
                controller: caster,
            },
        ) => {
            *caster == controller && {
                let types = selected_spell_types(state, *spell);
                types.contains(&crate::card_def::CardType::Instant)
                    || types.contains(&crate::card_def::CardType::Sorcery)
            }
        }
        (
            TriggerCondition::CastNoncreatureSpell,
            CommittedEvent::SpellCast {
                spell,
                controller: caster,
            },
        ) => {
            *caster == controller
                && !selected_spell_types(state, *spell)
                    .contains(&crate::card_def::CardType::Creature)
        }
        (
            TriggerCondition::CastNoncreatureOrSubtype(subtype),
            CommittedEvent::SpellCast {
                spell,
                controller: caster,
            },
        ) => {
            *caster == controller
                && (!selected_spell_types(state, *spell).contains(&CardType::Creature)
                    || selected_spell_has_subtype(state, *spell, subtype))
        }
        (
            TriggerCondition::CastSelf,
            CommittedEvent::SpellCast {
                spell,
                controller: caster,
            },
        ) => *spell == source && *caster == controller,
        (
            TriggerCondition::DrawNth(n),
            CommittedEvent::Draw {
                player,
                object: Some(_),
            },
        ) => *player == controller && draws_this_turn_at_event == n,
        (
            TriggerCondition::LeftBattlefieldToGraveyard,
            CommittedEvent::ZoneChange {
                object,
                from: Zone::Battlefield,
                to: Zone::Graveyard,
                ..
            },
        ) => *object == source,
        (
            TriggerCondition::LeftBattlefield,
            CommittedEvent::ZoneChange {
                object,
                from: Zone::Battlefield,
                ..
            },
        ) => *object == source,
        (
            TriggerCondition::DiesWithoutCounters,
            CommittedEvent::ZoneChange {
                object,
                from: Zone::Battlefield,
                to: Zone::Graveyard,
                ..
            },
        ) => {
            *object == source
                && state
                    .objects
                    .get(source)
                    .zone_change_count
                    .checked_sub(1)
                    .is_some_and(|departed| state.counter_lki_for(source, departed).is_none())
        }
        (
            TriggerCondition::SacrificeAnotherWithSubtype(subtype),
            CommittedEvent::Sacrificed {
                object,
                controller_before,
                effective_subtype_ids_before,
            },
        ) => {
            *object != source
                && *controller_before == controller
                && subtype.is_in_subtype_ids(effective_subtype_ids_before)
        }
        (
            TriggerCondition::SacrificeAnotherPermanent,
            CommittedEvent::Sacrificed {
                object,
                controller_before,
                ..
            },
        ) => *object != source && *controller_before == controller,
        (
            TriggerCondition::BeginningOfUpkeep { controller_only },
            CommittedEvent::UpkeepBegan { player },
        ) => !controller_only || *player == controller,
        (
            TriggerCondition::ControllerCommitsCrime,
            CommittedEvent::CrimeCommitted { player, .. },
        ) => *player == controller,
        (
            TriggerCondition::ControlledCreatureBecomesTargetOfOpponent,
            CommittedEvent::Targeted {
                target,
                target_zone_change_count,
                targeting_controller,
                ..
            },
        ) => {
            *targeting_controller != controller
                && state.objects.try_get(*target).is_some_and(|object| {
                    object.zone == Zone::Battlefield
                        && object.controller == controller
                        && object.zone_change_count == *target_zone_change_count
                })
                && crate::engine::object_has_type(
                    state,
                    *target,
                    crate::card_def::CardType::Creature,
                )
        }
        (
            TriggerCondition::ControllerCastsSecondSpellEachTurn,
            CommittedEvent::SpellCast {
                controller: caster, ..
            },
        ) => *caster == controller && state.players[caster.index()].spells_cast_this_turn == 2,
        (
            TriggerCondition::BeginningControllerEndStepWithTimeCounter,
            CommittedEvent::BeginningEndStep { active_player, .. },
        ) => *active_player == controller && state.objects.get(source).v4.time_counters_v1 > 0,
        (
            TriggerCondition::DiesIfWasCreature,
            CommittedEvent::ZoneChange {
                object,
                from: Zone::Battlefield,
                to: Zone::Graveyard,
                ..
            },
        ) => *object == source && source_was_creature_before_leaving(state, source),
        (
            TriggerCondition::ControlledCreatureDealsCombatDamageToPlayer,
            CommittedEvent::CombatDamageToPlayer {
                source: damage_source,
                ..
            },
        ) => state.objects.try_get(*damage_source).is_some_and(|object| {
            object.controller == controller
                && crate::engine::object_has_type(
                    state,
                    *damage_source,
                    crate::card_def::CardType::Creature,
                )
        }),
        (
            TriggerCondition::OtherControlledCreatureWithPowerAtMostEntersOncePerTurn(maximum),
            event,
        ) => battlefield_entry_object(event).is_some_and(|object| {
            object != source
                && state.objects.get(object).controller == controller
                && state.objects.get(object).zone == Zone::Battlefield
                && crate::engine::object_has_type(
                    state,
                    object,
                    crate::card_def::CardType::Creature,
                )
                && crate::engine::effective_power(state, object) <= maximum
        }),
        (
            TriggerCondition::BeginningOfControllerCombat,
            CommittedEvent::BeginningOfCombat { active_player },
        ) => *active_player == controller,
        (
            TriggerCondition::BeginningEndStepAfterUnearth,
            CommittedEvent::BeginningEndStep { .. },
        ) => state.objects.get(source).v4.unearthed_v1,
        (
            TriggerCondition::BeginningControllerEndStepIfDescended,
            CommittedEvent::BeginningEndStep { active_player, .. },
        ) => *active_player == controller && controller_descended_this_turn(state, controller),
        (
            TriggerCondition::TransformsIntoFrontFace,
            CommittedEvent::Transformed {
                object,
                face_index: 0,
            },
        ) => *object == source,
        (
            TriggerCondition::AttacksWithGreaterPowerAttacker,
            CommittedEvent::DeclaredAttacker {
                source: event_source,
                source_zone_change_count,
                controller: event_controller,
            },
        ) => {
            *event_source == source
                && *event_controller == controller
                && state.objects.get(source).zone_change_count == *source_zone_change_count
                && {
                    let power = crate::engine::effective_power(state, source);
                    state.engine.combat.attackers.iter().any(|&other| {
                        other != source && crate::engine::effective_power(state, other) > power
                    })
                }
        }
        (
            TriggerCondition::BecomesPlotted,
            CommittedEvent::ZoneChange {
                object,
                from: Zone::Hand,
                to: Zone::Exile,
                ..
            },
        ) => *object == source && state.objects.get(source).plotted_turn == Some(state.turn),
        (
            TriggerCondition::BecomesTargetOfControllerSpellOrAbilityFirstTimeEachTurn,
            CommittedEvent::Targeted {
                target,
                target_zone_change_count,
                targeting_controller,
                ..
            },
        ) => {
            *target == source
                && *target_zone_change_count == state.objects.get(source).zone_change_count
                && *targeting_controller == controller
        }
        _ => false,
    }
}

/// 603.3b: each player puts triggered abilities they control on the stack
/// in an order of their choice, in APNAP order (active player's group
/// first). This only groups; a real per-player choice within a group of 2+
/// is `engine::Decision::OrderTriggers`.
pub fn order_apnap(triggers: Vec<PendingTrigger>, active_player: PlayerId) -> Vec<PendingTrigger> {
    let (mut active, mut other): (Vec<_>, Vec<_>) = triggers
        .into_iter()
        .partition(|t| t.controller == active_player);
    active.append(&mut other);
    active
}

/// An `actor_visible_ordinal`/registry `visible_ordinal` value that is
/// provably greater than any ordinal a *real* `HistoricalPublicSource` row
/// can carry for this exact state, on both the action side
/// (`rl_session/flat_action_v3.rs`'s `extension_object`, which assigns a
/// `PendingEffect`-context row the position of its entry within
/// `PolicyObservationExtensionsV6::historical_public_sources`, 0-based) and
/// the registry side (`flat_policy_v2.rs`'s `register_extensions_v3`,
/// which assigns `add_validated_historical_source_v3` the row's *raw*
/// `Stack { stack_index }` -- up to `state.stack.len() - 1`, with possible
/// gaps wherever an intervening item is a spell -- or, for the single
/// `PendingEffect` row, `state.stack.len()` itself).
///
/// The real ordinals used by either mechanism therefore never exceed
/// `state.stack.len()`; `1 + state.stack.len()` is strictly greater than
/// every one of them regardless of stack shape (how many real rows exist,
/// which raw stack indices they occupy, or whether a `PendingEffect` row
/// is present), so a pending-trigger-derived row built from it can never
/// collide -- neither in the registry's own `(group, visible_ordinal)`
/// uniqueness the tensorizer enforces
/// (`native_flat_tensorizer_v2.rs`'s `build_object_projection_for_rows_v2`,
/// `NativeFlatTensorErrorV2::ObjectOrder`) nor in the action-side
/// `v3_action_objects` authority match. This is a safe upper bound, not an
/// attempt to reproduce `rl.rs`'s `policy_observation_extensions_v6` row
/// count: it does not need to match the real ordinal namespace, only to
/// stay outside it.
pub(crate) fn historical_public_source_ordinal_ceiling_v1(
    state: &crate::state::GameState,
) -> Option<u32> {
    u32::try_from(state.stack.len()).ok()?.checked_add(1)
}

/// V4 fresh-lineage helper, additive beside [`pending_trigger_choose_targets_gate_v1`]
/// (which stays exactly as committed for the V3/frozen path). Factors out
/// only the *hiddenness* half of that gate's predicate -- the trigger's live
/// `source` sits in a zone `pending.controller` cannot resolve it from
/// without prior knowledge, and carries no matching knowledge entry for its
/// exact live incarnation -- without the ChooseTargets-only decision-shape
/// constraints (`pending_triggers[0]`, `target_spec`/`targets` length, APNAP
/// group ordering). The V7 observation-extensions producer
/// (`policy_observation_v7::policy_observation_extensions_v7`) and the V4
/// action-slice component resolver (`rl_session::flat_action_v4`) both call
/// this instead of re-deriving the check, so the two layers cannot
/// independently drift, mirroring how both V3 layers already share
/// [`pending_trigger_choose_targets_gate_v1`] itself.
///
/// Two hidden shapes are recognized:
///
/// - `Zone::Library`: `EffectOp::ShuffleTriggerSourceIntoOwnersLibrary`'s
///   shape (the original case this helper covered). Checked against
///   `state.library_knowledge[pending.controller][live.owner]`.
/// - `Zone::Hand` with `live.owner != pending.controller`: the Initiative
///   mechanic's shape (real gameplay: campaign yardstick-cumulative3-b-001,
///   chunk-13-end-seat0, matches[10], Elves vs Terror, match seed
///   3641832355271763964, game 2, step 414). `event::log_initiative_trigger`
///   freezes the granting permanent's identity into `PendingTrigger::source`/
///   `source_contract` but reassigns `controller` to whichever player
///   currently holds the initiative (its own `source.controller = player`),
///   which can differ from the granting permanent's owner once the
///   initiative transfers to an opponent via combat damage
///   (`engine::deal_combat_damage`'s `InitiativeTriggerKindV1::CombatTransfer`).
///   The granting permanent itself is free to return to its owner's hand
///   through any unrelated effect in the meantime, which is exactly the
///   production shape: Avenging Hunter granted the initiative, Terror later
///   took it via combat damage, and Avenging Hunter itself was back in
///   Elves' hand by the time Terror's own frozen-source Undercity trigger
///   needed a target. Checked against
///   `state.hand_knowledge[pending.controller][live.owner]`, the same
///   knowledge table the ordinary `KnownOpponentHand` action-object arm
///   already consults (`rl_session::known_opponent_hand_canonical_ordinal_v1`),
///   so a genuinely revealed hand card still resolves through the ordinary
///   path instead of this fallback.
///
/// A same-controller hand (`live.owner == pending.controller`) is never
/// hidden: that is the ordinary `SelfHand` arm's business, which needs no
/// knowledge table at all.
///
/// Returns `false` (never hidden) when `pending.source` is not a live
/// object at all -- a defensive default, not a case any real caller should
/// ever hit for a `PendingTrigger` still present in `state.engine.pending_triggers`.
pub(crate) fn pending_trigger_hidden_source_v1(
    state: &crate::state::GameState,
    pending: &PendingTrigger,
) -> bool {
    let Some(live) = state.objects.try_get(pending.source) else {
        return false;
    };
    match live.zone {
        Zone::Library => !state.library_knowledge[pending.controller.index()][live.owner.index()]
            .iter()
            .any(|entry| {
                entry.object == pending.source && entry.zone_change_count == live.zone_change_count
            }),
        Zone::Hand if live.owner != pending.controller => !state.hand_knowledge
            [pending.controller.index()][live.owner.index()]
        .iter()
        .any(|entry| {
            entry.object == pending.source && entry.zone_change_count == live.zone_change_count
        }),
        _ => false,
    }
}

/// Whether the currently active decision is `Decision::ChooseTargets` for
/// `pending_triggers[0]` (`engine.rs`'s `drain_pending_triggers_or_decide`
/// never surfaces that decision for any other pending trigger), and, if
/// so, that trigger's live `source`, its frozen `source_contract`
/// (`PendingTrigger::source_contract` -- the identity
/// `EffectOp::ShuffleTriggerSourceIntoOwnersLibrary`, `effect.rs`, leaves
/// behind once the live object has moved into its owner's library), and
/// the exact ordinal a `HistoricalPublicSource`-tagged reference to it
/// must carry: [`historical_public_source_ordinal_ceiling_v1`] (an ordinal
/// no real historical row can ever carry, for any stack shape) plus this
/// trigger's own position in `pending_triggers` (always `0` today, since
/// only `pending_triggers[0]` can ever reach this gate, but named
/// explicitly so a second simultaneous, equally-hidden trigger could not
/// silently collide with the first if a future change ever let this gate
/// consider a later position).
///
/// This is the single gate predicate shared by the V3 action-slice
/// encoder (`rl_session.rs`'s `flat_visible_action_object_components_v1`,
/// layer A, which describes the hidden reference using the returned
/// contract and ordinal) and the V3 scoring reconciliation
/// (`flat_policy_v2.rs`'s
/// `append_pending_trigger_frozen_source_authority_v3`, layer B, which
/// registers and authorizes that description against the registry). Both
/// call this helper instead of re-deriving the gate or the ordinal
/// themselves, so the two layers cannot independently drift.
///
/// Returns `None` when there is no pending trigger, when the leading
/// same-controller group still needs `Decision::OrderTriggers` instead
/// (2+ pending triggers, not yet placement-ordered), when
/// `pending_triggers[0]` already has enough targets (so whatever decision
/// is active, it is not this one), when that trigger lost its
/// `source_contract`, or -- critically -- when the trigger's live source
/// is not actually hidden: its live object must be in `Zone::Library`
/// *and* `state.library_knowledge[controller][owner]` must carry no entry
/// for its exact live incarnation. Decision shape alone (a `ChooseTargets`
/// for `pending_triggers[0]` with a `source_contract`) is not enough --
/// every pending trigger with any targeted effect carries a
/// `source_contract` (`PendingTrigger::source_contract`'s doc), including
/// ordinary triggers whose source never left the battlefield and
/// leaves-the-battlefield triggers whose frozen contract deliberately
/// freezes the *departure* zone/generation (`trigger.rs`'s
/// `uses_leave_lki` triggers capture `zone: from` and `zone_change_count:
/// live - 1`) even though the source is now public in the graveyard. Only
/// `pending_triggers[0]` is ever considered, so no previously-succeeding
/// decision -- an `OrderTriggers` decision, a `ChooseTargets` for a
/// trigger whose source is still visible on the battlefield or in the
/// graveyard, or a `ChooseTargets` for a trigger whose source is in the
/// library but already known to its controller through
/// `library_knowledge` -- can gain a row through this gate.
pub(crate) fn pending_trigger_choose_targets_gate_v1(
    state: &crate::state::GameState,
) -> Option<(crate::ids::ObjectId, AbilitySourceContractV4, u32)> {
    let pending_triggers = &state.engine.pending_triggers;
    let first = pending_triggers.first()?;
    let controller = first.controller;
    let group_len = pending_triggers
        .iter()
        .take_while(|trigger| trigger.controller == controller)
        .count();
    let group_is_ordered = pending_triggers[..group_len]
        .iter()
        .all(|trigger| trigger.placement_ordered);
    if group_len >= 2 && !group_is_ordered {
        // `Decision::OrderTriggers` is active instead, not `ChooseTargets`.
        return None;
    }
    let trigger_position: u32 = 0;
    let pending = &pending_triggers[0];
    let need = crate::engine::target_count(pending.target_spec);
    if pending.targets.len() >= usize::from(need) {
        return None;
    }
    let contract = pending.source_contract?;
    let live = state.objects.try_get(pending.source)?;
    if live.zone != Zone::Library {
        // Resolvable through the ordinary path (battlefield, graveyard,
        // exile, stack, or a revealed hand) -- not this gate's business.
        return None;
    }
    let known = state.library_knowledge[pending.controller.index()][live.owner.index()]
        .iter()
        .any(|entry| {
            entry.object == pending.source && entry.zone_change_count == live.zone_change_count
        });
    if known {
        // In the library, but the controller already knows exactly where
        // -- the ordinary `KnownSelfLibrary`/`KnownOpponentLibrary` path
        // resolves it without help.
        return None;
    }
    let ordinal =
        historical_public_source_ordinal_ceiling_v1(state)?.checked_add(trigger_position)?;
    Some((pending.source, contract, ordinal))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::PlayerId;
    use crate::state::GameState;

    #[test]
    fn opponent_turn_cast_condition_counts_all_spell_types_for_both_seats() {
        let elf = crate::card_def::card_id_by_name("Llanowar Elves").unwrap();
        let instant = crate::card_def::CARD_DEFS
            .iter()
            .position(|definition| definition.has_type(CardType::Instant))
            .unwrap() as u16;
        for controller in [PlayerId::P0, PlayerId::P1] {
            let mut state = GameState::new_from_libraries(
                &[elf, instant],
                &[elf, instant],
                |id| crate::card_def::CARD_DEFS[id as usize].name.into(),
                958,
            );
            let source = state.players[controller.index()].library[0];
            for active_player in [PlayerId::P0, PlayerId::P1] {
                state.active_player = active_player;
                for caster in [PlayerId::P0, PlayerId::P1] {
                    for spell in state.players[caster.index()].library.iter().copied() {
                        let event = CommittedEvent::SpellCast {
                            spell,
                            controller: caster,
                        };
                        let replay: GameState =
                            serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
                        for candidate in [&state, &replay] {
                            assert_eq!(
                                trigger_matches(
                                    TriggerCondition::CastSpellDuringOpponentsTurn,
                                    &event,
                                    source,
                                    controller,
                                    candidate,
                                    0,
                                ),
                                caster == controller && active_player != controller,
                            );
                        }
                    }
                }
            }
            assert!(!trigger_matches(
                TriggerCondition::CastSpellDuringOpponentsTurn,
                &CommittedEvent::LifeGain {
                    player: controller,
                    amount: 1
                },
                source,
                controller,
                &state,
                0,
            ));
        }
    }

    #[test]
    fn opponent_turn_counter_keeps_original_source_after_restore_and_zone_change() {
        let elf = crate::card_def::card_id_by_name("Llanowar Elves").unwrap();
        for controller in [PlayerId::P0, PlayerId::P1] {
            let mut state =
                GameState::new_from_libraries(&[elf; 4], &[elf; 4], |_| "elf".into(), 959);
            let source = state.players[controller.index()].library[0];
            crate::event::propose_and_commit(
                &mut state,
                crate::event::ProposedEvent::zone_change(source, Zone::Battlefield),
            );
            let effect = materialize_trigger_source_program(
                (BRINEBORN_CUTTHROAT_TRIGGERS[0].effect)(),
                source,
                &state,
            );
            let ctx = crate::effect::ExecCtx::no_targets(source, controller);
            let mut replay: GameState =
                serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
            // The turn restriction has already been evaluated. Neither a
            // subsequent control change nor a turn change cancels the counter.
            state.active_player = controller;
            state.objects.get_mut(source).controller = controller.opponent();
            replay.active_player = state.active_player;
            replay.objects.get_mut(source).controller = controller.opponent();
            for candidate in [&mut state, &mut replay] {
                candidate.players[controller.index()]
                    .battlefield
                    .retain(|object| *object != source);
                candidate.players[controller.opponent().index()]
                    .battlefield
                    .push(source);
            }
            crate::effect::execute(&effect, &ctx, &mut state);
            crate::effect::execute(&effect, &ctx, &mut replay);
            assert_eq!(state.objects.get(source).counters.plus1_plus1, 1);
            assert_eq!(
                serde_json::to_value(&state).unwrap(),
                serde_json::to_value(&replay).unwrap()
            );
            for candidate in [&mut state, &mut replay] {
                crate::event::propose_and_commit(
                    candidate,
                    crate::event::ProposedEvent::zone_change(source, Zone::Hand),
                );
                crate::effect::execute(&effect, &ctx, candidate);
                assert_eq!(candidate.objects.get(source).counters.plus1_plus1, 0);
                crate::event::propose_and_commit(
                    candidate,
                    crate::event::ProposedEvent::zone_change(source, Zone::Battlefield),
                );
                crate::effect::execute(&effect, &ctx, candidate);
                assert_eq!(candidate.objects.get(source).counters.plus1_plus1, 0);
                assert!(candidate.engine.halted.is_none());
            }
            assert_eq!(
                serde_json::to_value(&state).unwrap(),
                serde_json::to_value(&replay).unwrap()
            );
        }
    }

    #[test]
    fn attack_count_condition_uses_declaration_and_source_incarnation() {
        let elf = crate::card_def::card_id_by_name("Llanowar Elves").unwrap();
        for controller in [PlayerId::P0, PlayerId::P1] {
            let mut state =
                GameState::new_from_libraries(&[elf; 5], &[elf; 5], |_| "elf".into(), 1);
            let objects = state.players[controller.index()].library.clone();
            let source = objects[0];
            state.objects.get_mut(source).zone_change_count = 7;
            let marker = CommittedEvent::ControllerAttacked {
                source,
                source_zone_change_count: 7,
                controller,
            };
            let condition = TriggerCondition::ControllerAttacksWithAtLeastCreatures(3);
            // The source is deliberately absent from the declared set.
            for count in 0..=4 {
                state.engine.combat.attackers = objects[1..1 + count].to_vec();
                assert_eq!(
                    trigger_matches(condition, &marker, source, controller, &state, 0),
                    count >= 3
                );
            }
            assert!(!trigger_matches(
                condition,
                &marker,
                source,
                controller.opponent(),
                &state,
                0
            ));
            state.objects.get_mut(source).zone_change_count = 8;
            assert!(!trigger_matches(
                condition, &marker, source, controller, &state, 0
            ));
            state.objects.get_mut(source).zone_change_count = 7;
            // Being put into combat alone supplies no declaration marker.
            assert!(!trigger_matches(
                condition,
                &CommittedEvent::DeclaredAttacker {
                    source,
                    source_zone_change_count: 7,
                    controller
                },
                source,
                controller,
                &state,
                0
            ));
            let replay: GameState =
                serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
            assert!(trigger_matches(
                condition, &marker, source, controller, &replay, 0
            ));
        }
    }

    #[test]
    fn cast_union_is_one_predicate_and_selected_face_does_not_inherit_subtypes() {
        let note = crate::card_def::card_id_by_name("Mental Note").unwrap();
        let fang = crate::card_def::card_id_by_name("Fang Dragon").unwrap();
        let mut state = GameState::new_from_libraries(
            &[note, fang],
            &[note],
            |id| crate::card_def::CARD_DEFS[id as usize].name.into(),
            1,
        );
        let note_object = state.players[0].library[0];
        let dragon = state.players[0].library[1];
        // A synthetic noncreature Dragon deliberately satisfies both terms.
        state.objects.get_mut(note_object).v4.effective_subtype_ids =
            vec![Subtype::Dragon.stable_id()];
        assert!(selected_spell_has_subtype(
            &state,
            note_object,
            Subtype::Dragon
        ));
        assert!(trigger_matches(
            TriggerCondition::CastNoncreatureOrSubtype(Subtype::Dragon),
            &CommittedEvent::SpellCast {
                spell: note_object,
                controller: PlayerId::P0
            },
            dragon,
            PlayerId::P0,
            &state,
            0,
        ));
        assert!(selected_spell_has_subtype(&state, dragon, Subtype::Dragon));
        state.objects.get_mut(dragon).v4.spell_cast_origin =
            Some(crate::state::SpellCastOriginV4 {
                origin_zone: Zone::Hand,
                origin_zone_change_count: 0,
                route: crate::state::SpellCastRouteV4::Hand,
                finalized_method: Some(crate::state::CastMethodV4::Omen),
            });
        assert!(!selected_spell_has_subtype(&state, dragon, Subtype::Dragon));
        assert_eq!(selected_spell_types(&state, dragon), &[CardType::Sorcery]);
    }

    // Lethal-damage creature death is exercised end-to-end in
    // `engine::tests::lethal_damage_kills_creature_via_sba`, using a real
    // `CARD_DEFS` creature (card-def ids here are synthetic and don't map
    // to real cards).

    #[test]
    fn sba_declares_loss_at_zero_life() {
        let mut state = GameState::new_from_libraries(&[1], &[2], |c| format!("card-{c}"), 1);
        state.players[0].life = 0;
        sba_fixed_point(&mut state);
        assert!(state.players[0].has_lost);
    }

    #[test]
    fn sba_declares_loss_on_drew_from_empty() {
        let mut state = GameState::new_from_libraries(&[1], &[2], |c| format!("card-{c}"), 1);
        state.players[1].drew_from_empty = true;
        sba_fixed_point(&mut state);
        assert!(state.players[1].has_lost);
    }

    #[test]
    fn indestructible_prevents_lethal_destruction_but_not_zero_toughness() {
        assert!(!creature_dies_to_state_based_actions(3, 3, false, true));
        assert!(creature_dies_to_state_based_actions(3, 3, false, false));
        assert!(creature_dies_to_state_based_actions(0, 0, false, true));
        assert!(creature_dies_to_state_based_actions(3, 1, true, false));
        assert!(!creature_dies_to_state_based_actions(3, 1, true, true));
    }

    /// Root-cause regression for the V4 action-encoder gap found via
    /// yardstick-cumulative3-b-001 chunk-13-end-seat0 matches[10] (Elves vs
    /// Terror, match seed 3641832355271763964, game 2, step 414):
    /// `pending_trigger_hidden_source_v1` recognized only a source shuffled
    /// into its owner's library, never a source sitting in its OWNER's hand
    /// while a DIFFERENT player (the pending trigger's controller) is the
    /// one resolving it -- the shape the Initiative mechanic produces once
    /// the initiative transfers to an opponent via combat damage while the
    /// granting permanent itself returns to its owner's hand through an
    /// unrelated effect. See [`pending_trigger_hidden_source_v1`]'s own doc
    /// comment for the full production trace.
    #[test]
    fn hidden_source_v1_recognizes_an_opponent_relative_hand_not_just_a_shuffled_library() {
        use crate::policy_observation_v6::tests::{put, ready_state};
        use crate::state::AbilitySourceContractV4;

        let mut state = ready_state();
        let source = put(&mut state, PlayerId::P0, "Myr Enforcer", Zone::Battlefield);
        let contract = AbilitySourceContractV4::capture(&state, source);
        let pending = PendingTrigger {
            controller: PlayerId::P1,
            source,
            granted_by: None,
            effect: EffectOp::Sequence(vec![]),
            is_madness_offer: false,
            kicked: false,
            target_spec: TargetSpec::Creature,
            targets: Vec::new(),
            target_contracts: Vec::new(),
            placement_ordered: false,
            source_contract: Some(contract),
            optional_additional_cost_paid: None,
            paid_cost_refs: Vec::new(),
        };
        // Still on the battlefield: visible, never hidden.
        assert!(!pending_trigger_hidden_source_v1(&state, &pending));

        state.players[PlayerId::P0.index()]
            .battlefield
            .retain(|&id| id != source);
        {
            let live = state.objects.get_mut(source);
            assert_eq!(live.zone, Zone::Battlefield);
            live.zone = Zone::Hand;
            live.zone_change_count += 1;
        }
        state.players[PlayerId::P0.index()].hand.push(source);

        // In its owner's (P0's) hand, unknown to the trigger's controller
        // (P1): the real-gameplay shape, and the exact defect this fix
        // closes. Before the fix this returned `false` (only `Zone::Library`
        // was ever considered hidden), so `frozen_pending_trigger_semantic_v4`
        // never substituted the frozen source and the ordinary Hand-zone arm
        // failed closed with `HiddenActionReference`.
        assert!(pending_trigger_hidden_source_v1(&state, &pending));

        // Once the controller is told about it (a reveal effect, say), it is
        // resolvable through the ordinary `KnownOpponentHand` path again,
        // exactly like a known library card already is.
        state.hand_knowledge[PlayerId::P1.index()][PlayerId::P0.index()].push(
            crate::state::HandKnowledgeEntry {
                object: source,
                zone_change_count: state.objects.get(source).zone_change_count,
            },
        );
        assert!(!pending_trigger_hidden_source_v1(&state, &pending));
    }

    #[test]
    fn apnap_orders_active_player_triggers_first() {
        let a = PendingTrigger {
            controller: PlayerId::P1,
            source: ObjectId(1),
            granted_by: None,
            effect: EffectOp::Sequence(vec![]),
            is_madness_offer: false,
            kicked: false,
            target_spec: TargetSpec::None,
            targets: Vec::new(),
            target_contracts: Vec::new(),
            placement_ordered: false,
            source_contract: None,
            optional_additional_cost_paid: None,
            paid_cost_refs: Vec::new(),
        };
        let b = PendingTrigger {
            controller: PlayerId::P0,
            source: ObjectId(2),
            granted_by: None,
            effect: EffectOp::Sequence(vec![]),
            is_madness_offer: false,
            kicked: false,
            target_spec: TargetSpec::None,
            targets: Vec::new(),
            target_contracts: Vec::new(),
            placement_ordered: false,
            source_contract: None,
            optional_additional_cost_paid: None,
            paid_cost_refs: Vec::new(),
        };
        let ordered = order_apnap(vec![a.clone(), b.clone()], PlayerId::P0);
        assert_eq!(ordered, vec![b, a]);
    }
}
