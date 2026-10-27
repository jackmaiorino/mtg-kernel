//! Triggered abilities of the wave E5a cards: the printed trigger tables,
//! their conditions, and the triggers engine hooks queue (Forbidden
//! Orchard's mana, The Endstone's land plays) or that begin an upkeep for
//! another player (Oath of Druids).

use super::{
    active_named, active_permanents, controls_three_artifacts, definition, e5a, e5a_state,
    is_colorless, noncreature_spells_cast_this_turn, token, E5aOpV1, E5aTriggerV1, QueuedTriggerV1,
    CHALICE_OF_THE_VOID, COUNCIL_OF_REEDS, ELDRAZI_SPAWN_TOKEN, FORBIDDEN_ORCHARD,
    GLARING_FLESHRAKER, INVENTORS_FAIR, OATH_OF_DRUIDS, SCRAWLING_CRAWLER, TALON_GATES,
    THE_ENDSTONE, THE_FANTASTICAR, UGIN,
};
use crate::card_def::{CardType, TargetSpec};
use crate::effect::{EffectOp, PlayerRef, TargetRef};
use crate::event::CommittedEvent;
use crate::ids::{ObjectId, PlayerId};
use crate::standard_cards_v1::{StandardOpV1, StandardTargetV1};
use crate::state::{AbilitySourceContractV4, GameState, Zone};
use crate::trigger::{PendingTrigger, TriggerCondition, TriggeredAbilityDef};

const fn trigger(condition: TriggerCondition, effect: fn() -> EffectOp) -> TriggeredAbilityDef {
    TriggeredAbilityDef {
        condition,
        home_zone: Zone::Battlefield,
        intervening_if_kicked: false,
        intervening_if_controls_another_source_card: false,
        face_index: 0,
        effect,
    }
}

const fn e5a_trigger(condition: E5aTriggerV1, effect: fn() -> EffectOp) -> TriggeredAbilityDef {
    trigger(
        TriggerCondition::StandardV1(crate::standard_cards_v1::StandardTriggerV1::EternalV1(
            crate::eternal_cards_v1::EternalTriggerV1::E5aV1(condition),
        )),
        effect,
    )
}

/// "Counter that spell": Vexing Bauble's trigger template, which the
/// Vintage module binds to the cast spell as the trigger is put on the
/// stack (`eternal_vintage_v1::materialize_event`).
fn counter_cast_spell() -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::EternalV1(
        crate::eternal_cards_v1::EternalOpV1::VintageV1(
            crate::eternal_vintage_v1::VintageOpV1::BindCounterCastSpellV1,
        ),
    ))
}

/// Glaring Fleshraker: "create a 0/1 colorless Eldrazi Spawn creature
/// token with 'Sacrifice this token: Add {C}.'"
fn create_spawn() -> EffectOp {
    EffectOp::CreateToken {
        token_def: token(ELDRAZI_SPAWN_TOKEN),
        controller: PlayerRef::Controller,
    }
}

/// Glaring Fleshraker: "this creature deals 1 damage to each opponent."
fn damage_each_opponent() -> EffectOp {
    EffectOp::DealDamage {
        target: TargetRef::Opponent,
        amount: 1,
    }
}

/// Ugin: "exile up to one target permanent that's one or more colors."
fn ugin_exile() -> EffectOp {
    e5a(E5aOpV1::ExileColoredTargetV1)
}

/// Council of Reeds: "create a token that's a copy of Council of Reeds."
fn council_copy() -> EffectOp {
    e5a(E5aOpV1::CouncilCopyV1)
}

/// Inventors' Fair: "you gain 1 life."
fn inventors_fair_life() -> EffectOp {
    e5a(E5aOpV1::GainLifeIfThreeArtifactsV1)
}

/// Scrawling Crawler: "each player draws a card", the active player first.
fn each_player_draws() -> EffectOp {
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

/// Scrawling Crawler: "Whenever an opponent draws a card, that player
/// loses 1 life."
fn opponent_loses_one() -> EffectOp {
    EffectOp::LoseLife {
        player: PlayerRef::Opponent,
        amount: 1,
    }
}

fn draw_one() -> EffectOp {
    EffectOp::DrawCards {
        player: PlayerRef::Controller,
        count: 1,
    }
}

/// The Endstone: "your life total becomes half your starting life total,
/// rounded up."
fn endstone_life() -> EffectOp {
    e5a(E5aOpV1::LifeBecomesHalfStartingV1)
}

/// The Fantasticar: "you may have The Fantasticar become an artifact
/// creature until end of turn."
fn fantasticar_animate() -> EffectOp {
    EffectOp::Choice {
        controller: PlayerRef::Controller,
        options: vec![
            e5a(E5aOpV1::AnimateSourceUntilEndOfTurnV1),
            EffectOp::Sequence(Vec::new()),
        ],
    }
}

/// The Fantasticar: "you may sacrifice The Fantasticar. If you do, create
/// four 4/4 colorless Construct artifact creature tokens with flying and
/// haste."
fn fantasticar_constructs() -> EffectOp {
    EffectOp::Choice {
        controller: PlayerRef::Controller,
        options: vec![
            e5a(E5aOpV1::SacrificeSourceForConstructsV1),
            EffectOp::Sequence(Vec::new()),
        ],
    }
}

/// Talon Gates of Madara: "When this land enters, up to one target
/// creature phases out."
fn talon_gates_phase_out() -> EffectOp {
    e5a(E5aOpV1::PhaseOutTargetV1)
}

const CHALICE_TRIGGERS: [TriggeredAbilityDef; 1] = [e5a_trigger(
    E5aTriggerV1::SpellCastManaValueEqualsChargeCountersV1,
    counter_cast_spell,
)];
const FLESHRAKER_TRIGGERS: [TriggeredAbilityDef; 2] = [
    e5a_trigger(E5aTriggerV1::ControllerCastsColorlessSpellV1, create_spawn),
    e5a_trigger(
        E5aTriggerV1::OtherColorlessControlledCreatureEntersV1,
        damage_each_opponent,
    ),
];
/// Ugin's cast trigger functions while Ugin is on the stack.
const UGIN_TRIGGERS: [TriggeredAbilityDef; 2] = [
    TriggeredAbilityDef {
        home_zone: Zone::Stack,
        ..trigger(TriggerCondition::CastSelf, ugin_exile)
    },
    e5a_trigger(E5aTriggerV1::ControllerCastsColorlessSpellV1, ugin_exile),
];
const COUNCIL_TRIGGERS: [TriggeredAbilityDef; 1] = [e5a_trigger(
    E5aTriggerV1::ControllerCombatIfCastNoncreatureSpellV1,
    council_copy,
)];
const INVENTORS_FAIR_TRIGGERS: [TriggeredAbilityDef; 1] = [e5a_trigger(
    E5aTriggerV1::ControllerUpkeepIfThreeArtifactsV1,
    inventors_fair_life,
)];
const CRAWLER_TRIGGERS: [TriggeredAbilityDef; 2] = [
    trigger(
        TriggerCondition::BeginningOfUpkeep {
            controller_only: true,
        },
        each_player_draws,
    ),
    trigger(TriggerCondition::OpponentDraws, opponent_loses_one),
];
const ENDSTONE_TRIGGERS: [TriggeredAbilityDef; 2] = [
    trigger(TriggerCondition::CastSpell, draw_one),
    trigger(TriggerCondition::BeginningControllerEndStep, endstone_life),
];
const FANTASTICAR_TRIGGERS: [TriggeredAbilityDef; 2] = [
    trigger(TriggerCondition::CastNoncreatureSpell, fantasticar_animate),
    e5a_trigger(
        E5aTriggerV1::ControllerCastsFourthNoncreatureSpellV1,
        fantasticar_constructs,
    ),
];
const TALON_GATES_TRIGGERS: [TriggeredAbilityDef; 1] =
    [trigger(TriggerCondition::Etb, talon_gates_phase_out)];

/// Triggered abilities of this module's cards.
pub(crate) fn triggers_for(name: &str) -> &'static [TriggeredAbilityDef] {
    match name {
        CHALICE_OF_THE_VOID => &CHALICE_TRIGGERS,
        GLARING_FLESHRAKER => &FLESHRAKER_TRIGGERS,
        UGIN => &UGIN_TRIGGERS,
        COUNCIL_OF_REEDS => &COUNCIL_TRIGGERS,
        INVENTORS_FAIR => &INVENTORS_FAIR_TRIGGERS,
        SCRAWLING_CRAWLER => &CRAWLER_TRIGGERS,
        THE_ENDSTONE => &ENDSTONE_TRIGGERS,
        THE_FANTASTICAR => &FANTASTICAR_TRIGGERS,
        TALON_GATES => &TALON_GATES_TRIGGERS,
        _ => &[],
    }
}

fn own_trigger_effect(name: &str, effect: &EffectOp) -> bool {
    triggers_for(name).iter().any(|trigger| {
        let template = (trigger.effect)();
        template == *effect || crate::eternal_vintage_v1::template_matches(&template, effect)
    }) || delayed_effect_matches(name, effect)
}

/// Target specs of this module's triggered abilities, per ability.
pub(crate) fn trigger_target_spec(name: &str, effect: &EffectOp) -> Option<TargetSpec> {
    if !own_trigger_effect(name, effect) {
        return None;
    }
    Some(if name == UGIN && *effect == ugin_exile() {
        TargetSpec::StandardV1(StandardTargetV1::UpToOneColoredPermanentV1)
    } else if name == TALON_GATES && *effect == talon_gates_phase_out() {
        TargetSpec::StandardV1(StandardTargetV1::UpToOneCreature)
    } else {
        TargetSpec::None
    })
}

/// Queued and delayed triggers owned by this module's cards.
pub(crate) fn delayed_effect_matches(name: &str, effect: &EffectOp) -> bool {
    let EffectOp::StandardV1(StandardOpV1::EternalV1(crate::eternal_cards_v1::EternalOpV1::E5aV1(
        op,
    ))) = effect
    else {
        return false;
    };
    match name {
        OATH_OF_DRUIDS => matches!(op, E5aOpV1::OathV1 { .. }),
        FORBIDDEN_ORCHARD => matches!(op, E5aOpV1::OrchardSpiritV1 { .. }),
        THE_ENDSTONE => matches!(op, E5aOpV1::EndstoneDrawV1),
        _ => false,
    }
}

pub(crate) fn trigger_matches(
    condition: E5aTriggerV1,
    events: &[CommittedEvent],
    index: usize,
    source: ObjectId,
    state: &GameState,
) -> bool {
    let controller = state.objects.get(source).controller;
    match condition {
        E5aTriggerV1::SpellCastManaValueEqualsChargeCountersV1 => {
            let CommittedEvent::SpellCast { spell, .. } = events[index] else {
                return false;
            };
            let Some(item) = state.stack.iter().find(|item| {
                item.kind == crate::state::StackItemKind::Spell && item.source == spell
            }) else {
                return false;
            };
            i32::from(crate::engine::stack_spell_mana_value(state, item))
                == state.objects.get(source).counters.charge
        }
        E5aTriggerV1::ControllerCastsColorlessSpellV1 => {
            matches!(events[index], CommittedEvent::SpellCast { spell, controller: caster }
                if caster == controller && is_colorless(state, spell))
        }
        E5aTriggerV1::OtherColorlessControlledCreatureEntersV1 => {
            let entered = match events[index] {
                CommittedEvent::ZoneChange {
                    object,
                    to: Zone::Battlefield,
                    ..
                }
                | CommittedEvent::CreateToken { object, .. } => object,
                _ => return false,
            };
            entered != source
                && state.objects.try_get(entered).is_some_and(|live| {
                    live.zone == Zone::Battlefield
                        && live.controller == controller
                        && crate::engine::object_has_type(state, entered, CardType::Creature)
                        && is_colorless(state, entered)
                })
        }
        E5aTriggerV1::ControllerCombatIfCastNoncreatureSpellV1 => {
            matches!(events[index], CommittedEvent::BeginningOfCombat { active_player }
                if active_player == controller)
                && noncreature_spells_cast_this_turn(state, controller) > 0
        }
        E5aTriggerV1::ControllerUpkeepIfThreeArtifactsV1 => {
            matches!(events[index], CommittedEvent::UpkeepBegan { player } if player == controller)
                && controls_three_artifacts(state, controller)
        }
        E5aTriggerV1::ControllerCastsFourthNoncreatureSpellV1 => {
            let CommittedEvent::SpellCast {
                spell,
                controller: caster,
            } = events[index]
            else {
                return false;
            };
            caster == controller
                && definition(state, spell).is_some_and(|def| !def.has_type(CardType::Creature))
                && noncreature_spells_cast_this_turn(state, controller) == 4
        }
    }
}

fn queue_trigger(state: &mut GameState, source: ObjectId, controller: PlayerId, op: E5aOpV1) {
    let contract = AbilitySourceContractV4::capture(state, source);
    e5a_state(state).queued_triggers.push(QueuedTriggerV1 {
        source: contract,
        controller,
        op,
    });
}

/// Forbidden Orchard: "Whenever you tap this land for mana, target opponent
/// creates a 1/1 colorless Spirit creature token." Called as a permanent is
/// tapped for mana, by an explicit activation or an automatic payment.
pub(crate) fn note_tapped_for_mana(state: &mut GameState, player: PlayerId, source: ObjectId) {
    if !active_named(state, source, FORBIDDEN_ORCHARD) {
        return;
    }
    queue_trigger(
        state,
        source,
        player,
        E5aOpV1::OrchardSpiritV1 {
            player: player.opponent(),
        },
    );
}

/// The Endstone: "Whenever you play a land or cast a spell, draw a card."
/// Called as `player` plays a land (a special action); casts trigger
/// through the printed table.
pub(crate) fn note_land_played(state: &mut GameState, player: PlayerId) {
    let stones = active_permanents(state, THE_ENDSTONE, Some(player)).collect::<Vec<_>>();
    for stone in stones {
        queue_trigger(state, stone, player, E5aOpV1::EndstoneDrawV1);
    }
}

/// Whether `player`'s opponent controls more creatures than `player`.
pub(crate) fn opponent_controls_more_creatures(state: &GameState, player: PlayerId) -> bool {
    let creatures = |seat: PlayerId| {
        super::controlled(state, seat)
            .filter(|&object| crate::engine::object_has_type(state, object, CardType::Creature))
            .count()
    };
    creatures(player.opponent()) > creatures(player)
}

/// Oath of Druids: "At the beginning of each player's upkeep, that player
/// chooses target player who controls more creatures than they do and is
/// their opponent." With no such player the trigger has no legal target
/// and is removed (603.3d), so it is created only when one exists.
fn oath_triggers(state: &GameState, events: &[CommittedEvent]) -> Vec<PendingTrigger> {
    let mut triggers = Vec::new();
    for event in events {
        let CommittedEvent::UpkeepBegan { player } = *event else {
            continue;
        };
        for oath in active_permanents(state, OATH_OF_DRUIDS, None) {
            let controller = state.objects.get(oath).controller;
            if !opponent_controls_more_creatures(state, player)
                || !crate::eternal_cards_v1::e3::player_targetable(
                    state,
                    player.opponent(),
                    controller,
                    Some(oath),
                )
            {
                continue;
            }
            triggers.push(crate::eternal_cards_v1::delayed_trigger(
                AbilitySourceContractV4::capture(state, oath),
                controller,
                e5a(E5aOpV1::OathV1 { player }),
            ));
        }
    }
    triggers
}

/// The triggers this collection puts on the stack that no printed trigger
/// condition matches.
pub(crate) fn delayed_triggers(
    state: &mut GameState,
    events: &[CommittedEvent],
) -> Vec<PendingTrigger> {
    let mut triggers = oath_triggers(state, events);
    let Some(eternal) = state.eternal_v1.as_mut() else {
        return triggers;
    };
    if eternal.e5a.queued_triggers.is_empty() {
        return triggers;
    }
    triggers.extend(
        std::mem::take(&mut eternal.e5a.queued_triggers)
            .into_iter()
            .map(|queued| {
                crate::eternal_cards_v1::delayed_trigger(
                    queued.source,
                    queued.controller,
                    e5a(queued.op),
                )
            }),
    );
    triggers
}

/// The beginning of `active`'s turn ends this module's turn-bound records.
pub(crate) fn begin_turn(state: &mut GameState, active: PlayerId) {
    let turn = state.turn_key_v1();
    let stack_sources = state
        .stack
        .iter()
        .map(|item| item.source)
        .collect::<Vec<_>>();
    if let Some(eternal) = state.eternal_v1.as_mut() {
        let e5a = &mut eternal.e5a;
        let current = |effect: &super::TurnEffectV1| effect.turn == turn && effect.active == active;
        e5a.wills.retain(current);
        e5a.exile_if_dies.retain(|(_, effect)| current(effect));
        e5a.animated.retain(|(_, effect)| current(effect));
        e5a.library_casts
            .retain(|cast| stack_sources.contains(&cast.card.object));
    }
}
