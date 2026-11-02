//! Effect leaves of the wave E5a cards that need no player decision.
//! Leaves that stage a choice run in `choices::drive` instead.

use super::{
    binding_live, controls_three_artifacts, definition, e5a_state, halt, live_link,
    noncreature_spells_cast_this_turn, token, E5aOpV1, TurnEffectV1, COLORLESS_SPIRIT_TOKEN,
    FANTASTICAR_CONSTRUCT_TOKEN,
};
use crate::card_def::{CardType, Subtype, TargetSpec};
use crate::effect::{EffectOp, ExecCtx, ObjectRef, PlayerRef, TargetRef};
use crate::event::{self, LibraryPlacement, ProposedEvent};
use crate::ids::{ObjectId, PlayerId};
use crate::mana::ManaColor;
use crate::standard_cards_v1::StandardTargetV1;
use crate::state::{GameState, Target, Zone};

/// Target `index`, if it is still the incarnation chosen and still legal
/// for the resolving spell or ability under `spec`.
pub(super) fn legal_target(
    ctx: &ExecCtx,
    state: &GameState,
    index: usize,
    spec: TargetSpec,
) -> Option<Target> {
    let target = *ctx.targets.get(index)?;
    (ctx.target_incarnation_matches(index, state)
        && crate::engine::effect_target_is_legal(
            state,
            ctx.source,
            ctx.controller,
            spec,
            &ctx.targets,
            index,
        ))
    .then_some(target)
}

/// Target `index` as a battlefield object still legal under `spec`.
fn legal_permanent(
    ctx: &ExecCtx,
    state: &GameState,
    index: usize,
    spec: TargetSpec,
) -> Option<ObjectId> {
    match legal_target(ctx, state, index, spec)? {
        Target::Object(object) if state.objects.get(object).zone == Zone::Battlefield => {
            Some(object)
        }
        _ => None,
    }
}

/// The resolving ability's source, if it is still the incarnation that
/// created the ability, on the battlefield.
fn live_ability_source(ctx: &ExecCtx, state: &GameState) -> Option<ObjectId> {
    let contract = ctx.ability_source_contract?;
    let live = state.objects.try_get(contract.source)?;
    (live.zone == Zone::Battlefield && live.zone_change_count == contract.zone_change_count)
        .then_some(contract.source)
}

/// Sacrifices `object` if `player` still controls it on the battlefield.
/// Returns whether it left the battlefield.
pub(super) fn sacrifice(state: &mut GameState, object: ObjectId, player: PlayerId) -> bool {
    let live = state.objects.get(object);
    if live.zone != Zone::Battlefield || live.controller != player {
        return false;
    }
    event::log_sacrifice(state, object);
    event::propose_and_commit(state, ProposedEvent::zone_change(object, Zone::Graveyard));
    state.objects.get(object).zone != Zone::Battlefield
}

fn draw(ctx: &ExecCtx, state: &mut GameState, player: PlayerRef, count: u32) {
    crate::effect::execute(&EffectOp::DrawCards { player, count }, ctx, state);
}

pub(crate) fn execute(op: &E5aOpV1, ctx: &ExecCtx, state: &mut GameState) {
    match op {
        E5aOpV1::DrawThenSourceOnTopV1 => {
            draw(ctx, state, PlayerRef::Controller, 1);
            if let Some(source) = live_ability_source(ctx, state) {
                event::propose_and_commit(
                    state,
                    ProposedEvent::public_library_insert(source, LibraryPlacement::Top),
                );
            }
        }
        E5aOpV1::ExileTopOfLibraryV1 => {
            if let Some(&top) = state.players[ctx.controller.index()].library.first() {
                event::propose_and_commit(state, ProposedEvent::zone_change(top, Zone::Exile));
            }
        }
        E5aOpV1::EnterWithXChargeCountersV1 => {
            if state
                .objects
                .try_get(ctx.source)
                .is_some_and(|live| live.zone == Zone::Stack)
            {
                crate::effect::execute(
                    &EffectOp::MoveObject {
                        object: ObjectRef::ThisSource,
                        to_zone: Zone::Battlefield,
                    },
                    ctx,
                    state,
                );
            }
            if state.objects.get(ctx.source).zone == Zone::Battlefield {
                state.objects.get_mut(ctx.source).counters.charge = i32::from(ctx.x_value);
            }
        }
        E5aOpV1::GainLifeIfThreeArtifactsV1 => {
            if controls_three_artifacts(state, ctx.controller) {
                event::propose_and_commit(state, ProposedEvent::life_gain(ctx.controller, 1));
            }
        }
        E5aOpV1::AnimateSourceUntilEndOfTurnV1 => {
            if let Some(source) = live_ability_source(ctx, state) {
                let link = live_link(state, source);
                let effect = TurnEffectV1::now(state, ctx.controller);
                let animated = &mut e5a_state(state).animated;
                if !animated.iter().any(|(host, _)| *host == link) {
                    animated.push((link, effect));
                }
            }
        }
        E5aOpV1::SacrificeSourceForConstructsV1 => {
            let Some(source) = live_ability_source(ctx, state) else {
                return;
            };
            if sacrifice(state, source, ctx.controller) {
                crate::effect::execute(
                    &EffectOp::CreateTokensDynamic {
                        token_def: token(FANTASTICAR_CONSTRUCT_TOKEN),
                        controller: PlayerRef::Controller,
                        count: crate::card_def::DynamicValueDef::Fixed(4),
                        tapped: false,
                    },
                    ctx,
                    state,
                );
            }
        }
        E5aOpV1::PhaseOutTargetV1 => {
            if let Some(object) = legal_permanent(
                ctx,
                state,
                0,
                TargetSpec::StandardV1(StandardTargetV1::UpToOneCreature),
            ) {
                super::phasing::phase_out(state, object);
            }
        }
        E5aOpV1::PutSourceOntoBattlefieldV1 => {
            // The card revealed from the hand to activate the ability, if it
            // is still that hand incarnation.
            let (source, zone_change_count) =
                match (ctx.ability_source_contract, ctx.hidden_ability_source) {
                    (Some(contract), _) => (contract.source, contract.zone_change_count),
                    (None, Some(hidden)) => (hidden.object, hidden.zone_change_count),
                    (None, None) => (ctx.source, state.objects.get(ctx.source).zone_change_count),
                };
            let in_hand = state.objects.try_get(source).is_some_and(|live| {
                live.zone == Zone::Hand
                    && live.zone_change_count == zone_change_count
                    && live.owner == ctx.controller
            });
            if in_hand {
                event::propose_and_commit(
                    state,
                    ProposedEvent::zone_change_to_battlefield_under_controller(
                        source,
                        ctx.controller,
                    ),
                );
            }
        }
        E5aOpV1::ExileCreatureManaValueAtMostXV1 { slot, spec } => {
            let Some(object) = legal_permanent(ctx, state, usize::from(*slot), *spec) else {
                return;
            };
            if crate::engine::object_has_type(state, object, CardType::Creature)
                && crate::engine::object_mana_value(state, object) <= ctx.x_value
            {
                event::propose_and_commit(state, ProposedEvent::zone_change(object, Zone::Exile));
            }
        }
        E5aOpV1::ExileGraveyardTargetsV1 { first, spec } => {
            let exiled = (usize::from(*first)..ctx.targets.len())
                .filter_map(|index| match legal_target(ctx, state, index, *spec) {
                    Some(Target::Object(card))
                        if state.objects.get(card).zone == Zone::Graveyard =>
                    {
                        Some(card)
                    }
                    _ => None,
                })
                .take(usize::from(ctx.x_value))
                .collect::<Vec<_>>();
            if !exiled.is_empty() {
                event::propose_and_commit_batch(
                    state,
                    exiled
                        .into_iter()
                        .map(|card| ProposedEvent::zone_change(card, Zone::Exile))
                        .collect(),
                );
            }
        }
        E5aOpV1::ExileColoredTargetV1 => {
            if let Some(object) = legal_permanent(
                ctx,
                state,
                0,
                TargetSpec::StandardV1(StandardTargetV1::UpToOneColoredPermanentV1),
            ) {
                event::propose_and_commit(state, ProposedEvent::zone_change(object, Zone::Exile));
            }
        }
        E5aOpV1::OathRevealV1 { player } => {
            if oath_reveal(state, *player).is_err() {
                halt(state, ctx.source);
            }
        }
        E5aOpV1::OrchardSpiritV1 { player } => {
            if !crate::eternal_cards_v1::e3::player_targetable(
                state,
                *player,
                ctx.controller,
                Some(ctx.source),
            ) {
                return;
            }
            let controller = if *player == ctx.controller {
                PlayerRef::Controller
            } else {
                PlayerRef::Opponent
            };
            crate::effect::execute(
                &EffectOp::CreateToken {
                    token_def: token(COLORLESS_SPIRIT_TOKEN),
                    controller,
                },
                ctx,
                state,
            );
        }
        E5aOpV1::FlashPayV1 { creature } => {
            if !binding_live(state, *creature) {
                return;
            }
            let Some(def) = definition(state, creature.object) else {
                return;
            };
            // "its mana cost reduced by {2}": the reduction applies to the
            // generic part (601.2f).
            let cost = crate::mana::Cost {
                pips: def.cost.pips,
                generic: def.cost.generic.saturating_sub(2),
                x_count: 0,
            };
            match crate::mana::can_pay(&cost, 0, ctx.controller, state) {
                Some(plan) => {
                    crate::engine::pay_plan(state, ctx.controller, &plan);
                }
                None => {
                    sacrifice(state, creature.object, ctx.controller);
                }
            }
        }
        E5aOpV1::FlashSacrificeV1 { creature } => {
            if binding_live(state, *creature) {
                sacrifice(state, creature.object, ctx.controller);
            }
        }
        E5aOpV1::OkoElkV1 => {
            let Some(object) =
                legal_permanent(ctx, state, 0, TargetSpec::AnotherArtifactOrCreature)
            else {
                return;
            };
            let link = live_link(state, object);
            let timestamp = crate::engine::next_timestamp(state);
            let live = state.objects.get_mut(object);
            live.v4.effective_subtype_ids = vec![Subtype::Elk.stable_id()];
            live.v4.effective_color_mask = crate::card_def::mana_color_mask(ManaColor::G);
            let elks = &mut e5a_state(state).elks;
            elks.retain(|(host, _)| *host != link);
            elks.push((link, timestamp));
        }
        E5aOpV1::OkoExchangeV1 => {
            let spec = TargetSpec::StandardV1(
                StandardTargetV1::ControlledArtifactOrCreatureThenOpponentSmallCreatureV1,
            );
            let (Some(first), Some(second)) = (
                legal_permanent(ctx, state, 0, spec),
                legal_permanent(ctx, state, 1, spec),
            ) else {
                return;
            };
            let (first_controller, second_controller) = (
                state.objects.get(first).controller,
                state.objects.get(second).controller,
            );
            if first == second || first_controller == second_controller {
                return;
            }
            crate::standard_cards_v1::gain_control(state, first, second_controller);
            crate::standard_cards_v1::gain_control(state, second, first_controller);
        }
        E5aOpV1::SuplexDamageV1 => {
            let Some(object) = legal_permanent(ctx, state, 0, TargetSpec::Creature) else {
                return;
            };
            let link = live_link(state, object);
            let effect = TurnEffectV1::now(state, ctx.controller);
            e5a_state(state).exile_if_dies.push((link, effect));
            crate::effect::execute(
                &EffectOp::DealDamage {
                    target: TargetRef::Target(0),
                    amount: 3,
                },
                ctx,
                state,
            );
        }
        E5aOpV1::LifeBecomesHalfStartingV1 => {
            let player = ctx.controller;
            let target = (crate::state::STARTING_LIFE + 1) / 2;
            let life = state.players[player.index()].life;
            if life > target {
                event::propose_and_commit(state, ProposedEvent::life_loss(player, life - target));
            } else if life < target {
                event::propose_and_commit(state, ProposedEvent::life_gain(player, target - life));
            }
        }
        E5aOpV1::YawgmothsWillV1 => {
            let effect = TurnEffectV1::now(state, ctx.controller);
            let wills = &mut e5a_state(state).wills;
            if !wills.contains(&effect) {
                wills.push(effect);
            }
        }
        E5aOpV1::CouncilCopyV1 => {
            if noncreature_spells_cast_this_turn(state, ctx.controller) == 0 {
                return;
            }
            let source = ctx
                .ability_source_contract
                .map_or(ctx.source, |contract| contract.source);
            crate::standard_cards_v1::create_token_copy(state, source, ctx.controller);
        }
        E5aOpV1::EndstoneDrawV1 => draw(ctx, state, PlayerRef::Controller, 1),
        E5aOpV1::KozilekV1 { .. }
        | E5aOpV1::ScryTargetXV1 { .. }
        | E5aOpV1::UginSearchV1
        | E5aOpV1::OathV1 { .. }
        | E5aOpV1::FlashV1
        | E5aOpV1::BeseechV1
        | E5aOpV1::VampiricV1 => {
            // Choice leaves run only inside the resumable interpreter.
            halt(state, ctx.source);
        }
    }
}

/// Oath of Druids: "The first player may reveal cards from the top of their
/// library until they reveal a creature card. If the first player does,
/// that player puts that card onto the battlefield and all other cards
/// revealed this way into their graveyard."
fn oath_reveal(state: &mut GameState, player: PlayerId) -> Result<(), String> {
    let library = state.players[player.index()].library.clone();
    let found = library.iter().position(|&card| {
        crate::card_def::CARD_DEFS
            .get(state.objects.get(card).card_def as usize)
            .is_some_and(|def| def.has_type(CardType::Creature))
    });
    let revealed = found.map_or(library.len(), |index| index + 1);
    // spellbench-public-reveal: public reveal to both players.
    for observer in [PlayerId::P0, PlayerId::P1] {
        state.reveal_library_top(observer, player, revealed);
    }
    if let Some(index) = found {
        event::propose_and_commit(
            state,
            ProposedEvent::zone_change_to_battlefield_under_controller(library[index], player),
        );
    }
    let rest = library[..found.unwrap_or(library.len())]
        .iter()
        .copied()
        .filter(|&card| state.objects.get(card).zone == Zone::Library)
        .map(|card| ProposedEvent::zone_change(card, Zone::Graveyard))
        .collect::<Vec<_>>();
    if !rest.is_empty() {
        event::propose_and_commit_batch(state, rest);
    }
    Ok(())
}
