//! Garruk's event-time observations and source-independent temporary trigger.
use super::*;
use crate::card_def::CardType;

const GARRUK: &str = "Garruk, Curse Breaker";

fn sources(state: &GameState) -> Vec<ObjectId> {
    if !cfg!(feature = "eternal-fixtures") {
        return Vec::new();
    }
    crate::eternal_e5a_v1::active_permanents(state, GARRUK, None).collect()
}

fn draw() -> EffectOp {
    EffectOp::Sequence(vec![EffectOp::DrawCards {
        player: PlayerRef::Controller,
        count: 1,
    }])
}

/// Single commits sample after entry counters and attachments; batch commits
/// defer this observation until the entire simultaneous board is installed.
pub(crate) fn capture_single_entry(state: &mut GameState, index: usize, deferred: bool) {
    if !deferred && cfg!(feature = "eternal-fixtures") {
        capture_entry(state, index);
    }
}

pub(crate) fn capture_batch_entries(state: &mut GameState, start: usize) {
    if sources(state).is_empty() {
        return;
    }
    let end = state.engine.event_history.len();
    for index in start..end {
        capture_entry(state, index);
    }
}

fn capture_entry(state: &mut GameState, index: usize) {
    let Some(entrant) = state
        .engine
        .event_history
        .get(index)
        .and_then(crate::trigger::battlefield_entry_object)
    else {
        return;
    };
    let observers = sources(state);
    if observers.is_empty()
        || state
            .eternal_v1
            .as_ref()
            .is_some_and(|e| e.c6c.garruk_entries.contains(&index))
    {
        return;
    }
    // Mark even a below-threshold observation so an enclosing batch cannot
    // resample a separately completed nested entry after a later power change.
    state
        .eternal_v1
        .get_or_insert_with(Default::default)
        .c6c
        .garruk_entries
        .push(index);
    let live = state.objects.get(entrant);
    if live.zone != Zone::Battlefield
        || !crate::engine::object_has_type(state, entrant, CardType::Creature)
        || crate::engine::effective_power(state, entrant) < 4
    {
        return;
    }
    let controller = live.controller;
    let pending: Vec<_> = observers
        .into_iter()
        .filter(|&source| state.objects.get(source).controller == controller)
        .map(|source| {
            crate::eternal_cards_v1::delayed_trigger(
                AbilitySourceContractV4::capture(state, source),
                controller,
                draw(),
            )
        })
        .collect();
    state
        .eternal_v1
        .get_or_insert_with(Default::default)
        .c6c
        .garruk_pending
        .extend(pending);
}

/// Called once after all attack targets are chosen, before the SBA checkpoint.
/// In two seats only the owner's attackers can attack that owner's opponent.
pub(crate) fn capture_attack(state: &mut GameState) {
    let Some(records) = state.eternal_v1.as_ref().map(|e| &e.c6c) else {
        return;
    };
    if records.garruk_grants.is_empty() {
        return;
    }
    let creatures: Vec<_> = state
        .engine
        .combat
        .attackers
        .iter()
        .copied()
        .filter(|&object| crate::attack_target_v1::attacked_planeswalker(state, object).is_none())
        .map(|object| ObjectLinkV4 {
            object,
            zone_change_count: state.objects.get(object).zone_change_count,
        })
        .collect();
    if creatures.is_empty() {
        return;
    }
    let pending: Vec<_> = records
        .garruk_grants
        .iter()
        .filter(|grant| grant.controller == state.active_player)
        .map(|grant| {
            crate::eternal_cards_v1::delayed_trigger(
                grant.source,
                grant.controller,
                leaf(C6cOpV1::GarrukPumpV1 {
                    creatures: creatures.clone(),
                }),
            )
        })
        .collect();
    state
        .eternal_v1
        .as_mut()
        .expect("records above")
        .c6c
        .garruk_pending
        .extend(pending);
}

/// Actual turn entry, including an extra turn, expires the installed ability.
/// This is Teferi, Time Raveler's lifecycle, without the round-valued turn key.
pub(crate) fn begin_turn(state: &mut GameState, player: PlayerId) {
    if let Some(eternal) = state.eternal_v1.as_mut() {
        eternal
            .c6c
            .garruk_grants
            .retain(|grant| grant.controller != player);
    }
}

pub(super) fn delayed_effect_matches(name: &str, effect: &EffectOp) -> bool {
    name == GARRUK
        && (*effect == draw()
            || matches!(
                effect,
                EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::C6cV1(
                    C6cOpV1::GarrukPumpV1 { .. }
                )))
            ))
}

pub(super) fn execute(op: &C6cOpV1, ctx: &ExecCtx, state: &mut GameState) {
    match op {
        C6cOpV1::GarrukUntapV1 => {
            for (index, target) in ctx.targets.iter().enumerate() {
                let Target::Object(object) = *target else {
                    continue;
                };
                if ctx.target_incarnation_matches(index, state)
                    && ctx.ability_source_contract.is_some_and(|source| {
                        crate::engine::effect_target_is_legal_from_ability_source(
                            state,
                            source,
                            ctx.controller,
                            TargetSpec::StandardV1(StandardTargetV1::UpToTwoLandsV1),
                            &ctx.targets,
                            index,
                        )
                    })
                {
                    crate::engine::attempt_untap(state, object);
                }
            }
        }
        C6cOpV1::GarrukGrantV1 => {
            if let Some(source) = ctx.ability_source_contract {
                state
                    .eternal_v1
                    .get_or_insert_with(Default::default)
                    .c6c
                    .garruk_grants
                    .push(EmblemV1 {
                        controller: ctx.controller,
                        source,
                    });
            }
        }
        C6cOpV1::GarrukPumpV1 { creatures } => {
            for &creature in creatures {
                if !live_in(state, creature, Zone::Battlefield) {
                    continue;
                }
                let timestamp = crate::engine::next_timestamp(state);
                state.engine.until_end_of_turn.push(
                    crate::engine::UntilEndOfTurnEffect::ResolvedObjectEffect {
                        object_id: creature.object,
                        object_zone_change_count: creature.zone_change_count,
                        layer: crate::engine::Layers::POWER_TOUGHNESS,
                        timestamp,
                        duration: crate::engine::EffectDuration::EndOfTurn,
                        power: 2,
                        toughness: 2,
                        grant_haste: false,
                    },
                );
                state.engine.until_end_of_turn.push(
                    crate::engine::UntilEndOfTurnEffect::ResolvedObjectKeywordEffect {
                        object_id: creature.object,
                        object_zone_change_count: creature.zone_change_count,
                        layer: crate::engine::Layers::ABILITY_ADDING,
                        timestamp,
                        duration: crate::engine::EffectDuration::EndOfTurn,
                        keywords: Keywords::TRAMPLE,
                    },
                );
            }
        }
        _ => {}
    }
}
