//! Rules shared by the Standard legendary creatures. Persistent grants retain
//! exact battlefield incarnations; continuously changing bonuses read the board.
use crate::card_def::{CardType, Keywords, Supertype, CARD_DEFS};
use crate::effect::{EffectObjectBinding, EffectOp, ExecCtx};
use crate::ids::ObjectId;
use crate::state::{GameState, Target, Zone};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LegendEffectV1 {
    CountersOnControlledCreatures,
    ProtectControlledLegendaryCreatures,
    SourcePowerCountersAndHaste,
}

pub(crate) fn has_printed_ability(state: &GameState, object: ObjectId, name: &str) -> bool {
    let Some(live) = state.objects.try_get(object) else {
        return false;
    };
    let def = &CARD_DEFS[live.card_def as usize];
    live.zone == Zone::Battlefield
        && def.is_executable()
        && def.name == name
        && crate::continuous_characteristics_v1::printed_abilities_active(state, object)
}

pub(crate) fn legendary_creature(state: &GameState, object: ObjectId) -> bool {
    crate::engine::object_has_type(state, object, CardType::Creature)
        && CARD_DEFS[state.objects.get(object).card_def as usize]
            .supertypes
            .contains(&Supertype::Legendary)
}

pub(crate) fn jodah_bonus(state: &GameState, recipient: ObjectId) -> i32 {
    let object = state.objects.get(recipient);
    if object.zone != Zone::Battlefield || !legendary_creature(state, recipient) {
        return 0;
    }
    let battlefield = &state.players[object.controller.index()].battlefield;
    let sources = battlefield
        .iter()
        .filter(|&&id| has_printed_ability(state, id, "Jodah, the Unifier"))
        .count();
    let count = battlefield
        .iter()
        .filter(|&&id| legendary_creature(state, id))
        .count();
    i32::try_from(sources.saturating_mul(count)).unwrap_or(i32::MAX)
}

fn binding(state: &GameState, object: ObjectId) -> EffectObjectBinding {
    EffectObjectBinding {
        object,
        expected_zone: Zone::Battlefield,
        expected_zone_change_count: state.objects.get(object).zone_change_count,
    }
}

pub(crate) fn execute(op: LegendEffectV1, ctx: &ExecCtx, state: &mut GameState) {
    match op {
        LegendEffectV1::CountersOnControlledCreatures => {
            let creatures: Vec<_> = state.players[ctx.controller.index()]
                .battlefield
                .iter()
                .copied()
                .filter(|&id| crate::engine::object_has_type(state, id, CardType::Creature))
                .collect();
            for object in creatures {
                crate::event::add_plus_one_counters(state, object, ctx.controller, 1)
                    .expect("live creature");
            }
        }
        LegendEffectV1::ProtectControlledLegendaryCreatures => {
            let creatures: Vec<_> = state.players[ctx.controller.index()]
                .battlefield
                .iter()
                .copied()
                .filter(|&id| legendary_creature(state, id))
                .map(|id| binding(state, id))
                .collect();
            for object in creatures {
                crate::effect::install_temporary_boost(
                    state,
                    object,
                    1,
                    0,
                    Keywords::INDESTRUCTIBLE,
                );
            }
        }
        LegendEffectV1::SourcePowerCountersAndHaste => {
            let Some(Target::Object(target)) = ctx.targets.first().copied() else {
                return;
            };
            if !ctx.target_incarnation_matches(0, state)
                || state.objects.get(target).zone != Zone::Battlefield
            {
                return;
            }
            let power = ctx
                .ability_source_contract
                .and_then(|contract| {
                    let live = state.objects.try_get(contract.source)?;
                    if live.zone == Zone::Battlefield
                        && live.zone_change_count == contract.zone_change_count
                    {
                        Some(crate::engine::effective_power(state, contract.source))
                    } else {
                        state
                            .engine
                            .event_history
                            .iter()
                            .rev()
                            .find_map(|event| match event {
                                crate::event::CommittedEvent::PowerBeforeLeavingBattlefield {
                                    object,
                                    zone_change_count,
                                    power,
                                } if *object == contract.source
                                    && *zone_change_count == contract.zone_change_count =>
                                {
                                    Some(*power)
                                }
                                _ => None,
                            })
                    }
                })
                .unwrap_or(0)
                .max(0);
            crate::event::add_plus_one_counters(state, target, ctx.controller, power)
                .expect("live target");
            crate::effect::execute(
                &EffectOp::GrantKeywordTargetUntilEndOfTurn {
                    object: crate::effect::ObjectRef::Target(0),
                    keyword: Keywords::HASTE,
                },
                ctx,
                state,
            );
        }
    }
}

/// Katilda grants a separate mana ability even to Humans whose own printed
/// abilities have been removed, provided her grant has the later timestamp.
pub(crate) fn katilda_mana_colors(
    state: &GameState,
    object: ObjectId,
) -> Vec<crate::mana::ManaColor> {
    use crate::mana::ManaColor;
    let Some(live) = state.objects.try_get(object) else {
        return Vec::new();
    };
    if live.zone != Zone::Battlefield
        || !crate::engine::object_has_type(state, object, CardType::Creature)
        || !crate::engine::has_effective_subtype(state, object, crate::card_def::Subtype::Human)
    {
        return Vec::new();
    }
    let granted = state.players[live.controller.index()]
        .battlefield
        .iter()
        .any(|&source| {
            has_printed_ability(state, source, "Katilda, Dawnhart Prime")
                && crate::continuous_characteristics_v1::grant_survives(
                    state,
                    object,
                    state.objects.get(source).v4.layer_timestamp.unwrap_or(0),
                )
        });
    if !granted {
        return Vec::new();
    }
    let mask = crate::engine::object_color_mask(state, object);
    [
        ManaColor::W,
        ManaColor::U,
        ManaColor::B,
        ManaColor::R,
        ManaColor::G,
    ]
    .into_iter()
    .filter(|&color| mask & crate::card_def::mana_color_mask(color) != 0)
    .collect()
}

pub(crate) fn katilda_protected_from(
    state: &GameState,
    target: ObjectId,
    source: ObjectId,
) -> bool {
    has_printed_ability(state, target, "Katilda, Dawnhart Prime")
        && crate::engine::has_effective_subtype(state, source, crate::card_def::Subtype::Werewolf)
}
