//! Resolution choices of the wave E5a cards. They are staged as
//! `E3ChoiceV1` selections (kind `E3ChoiceKindV1::E5aV1`), so the effect
//! interpreter stages, validates and answers them like the E3 choices.

use super::{bind, binding_live, e5a, execute, token, E5aOpV1, KOZILEKS_COMMAND};
use crate::card_def::{CardType, OptionalAdditionalCostDef, TargetSpec, CARD_DEFS};
use crate::effect::{EffectContinuation, EffectFrame, EffectObjectBinding, EffectOp, PlayerRef};
use crate::eternal_cards_v1::e3::{E3ChoiceKindV1, E3ChoiceV1};
use crate::event::{self, ProposedEvent};
use crate::ids::PlayerId;
use crate::state::{GameState, Target, Zone};
use serde::{Deserialize, Serialize};

/// What a wave E5a resolution choice decides.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum E5aChoiceV1 {
    /// Beseech the Mirror's search: up to one library card. `bargained` is
    /// whether the spell was bargained as it was cast.
    BeseechSearchV1 { bargained: bool },
    /// Beseech the Mirror: whether to cast the found card without paying
    /// its mana cost. The interpreter casts an answered card.
    BeseechCastV1,
    /// Vampiric Tutor's search: up to one library card.
    VampiricSearchV1,
    /// Flash: up to one creature card from the hand.
    FlashPutV1,
    /// Ugin, Eye of the Storms's -11: any number of colorless nonland
    /// library cards.
    UginSearchV1,
}

impl E5aChoiceV1 {
    /// Whether the chooser sees library cards it has not looked at before
    /// (a search).
    pub(crate) fn library_search(&self) -> bool {
        matches!(
            self,
            E5aChoiceV1::BeseechSearchV1 { .. }
                | E5aChoiceV1::VampiricSearchV1
                | E5aChoiceV1::UginSearchV1
        )
    }
}

fn offer(
    continuation: &mut EffectContinuation,
    kind: E5aChoiceV1,
    player: PlayerId,
    cards: Vec<EffectObjectBinding>,
    min: u16,
    max: u16,
    path: Vec<u16>,
) {
    let choice = E3ChoiceV1 {
        kind: E3ChoiceKindV1::E5aV1(kind),
        player,
        candidates: cards
            .iter()
            .map(|card| Target::Object(card.object))
            .collect(),
        bindings: cards,
        min,
        max,
        ordered: false,
        path,
        remaining: continuation.frames.clone(),
    };
    crate::eternal_cards_v1::e3::offer(continuation, choice);
}

fn library_cards(
    state: &GameState,
    player: PlayerId,
    keep: impl Fn(&crate::card_def::CardDef) -> bool,
) -> Vec<EffectObjectBinding> {
    state.players[player.index()]
        .library
        .iter()
        .copied()
        .filter(|&card| {
            CARD_DEFS
                .get(state.objects.get(card).card_def as usize)
                .is_some_and(&keep)
        })
        .map(|card| bind(state, card))
        .collect()
}

fn shuffle(state: &mut GameState, player: PlayerId) -> Result<(), String> {
    state
        .shuffle_library(player)
        .map_err(|error| error.to_string())
}

fn push(continuation: &mut EffectContinuation, op: EffectOp, path: &[u16]) {
    continuation.frames.push(EffectFrame::Program {
        op,
        path: path.to_vec(),
    });
}

/// Runs a wave E5a leaf inside the resumable interpreter. `Ok(true)` when it
/// staged a choice and the continuation must suspend.
pub(crate) fn drive(
    state: &mut GameState,
    continuation: &mut EffectContinuation,
    op: &E5aOpV1,
    path: Vec<u16>,
) -> Result<bool, String> {
    let player = continuation.ctx.controller;
    match op {
        E5aOpV1::KozilekV1 {
            spawn,
            scry,
            creature,
            cards,
            spec,
        } => {
            kozilek(
                state,
                continuation,
                *spawn,
                *scry,
                *creature,
                *cards,
                *spec,
                &path,
            );
            Ok(false)
        }
        E5aOpV1::ScryTargetXV1 { slot } => {
            let Some(&Target::Player(scrier)) = continuation.ctx.targets.get(usize::from(*slot))
            else {
                return Ok(false);
            };
            let count = usize::from(continuation.ctx.x_value);
            Ok(crate::eternal_cards_v1::e3::dungeons::stage_scry(
                state,
                continuation,
                scrier,
                count,
                path,
            ))
        }
        E5aOpV1::UginSearchV1 => {
            // Painter's Servant colors every card (wave E9a).
            let painted = crate::eternal_legacy_e9a_v1::statics::painter_mask(state) != 0;
            let cards = library_cards(state, player, |def| {
                !painted && def.colors.is_empty() && !def.has_type(CardType::Land) && !def.is_token
            });
            if cards.is_empty() {
                shuffle(state, player)?;
                return Ok(false);
            }
            let max = u16::try_from(cards.len()).unwrap_or(u16::MAX);
            offer(
                continuation,
                E5aChoiceV1::UginSearchV1,
                player,
                cards,
                0,
                max,
                path,
            );
            Ok(true)
        }
        E5aOpV1::OathV1 { player: upkeep } => {
            // The target restriction is checked again on resolution.
            let target = upkeep.opponent();
            if !super::triggers::opponent_controls_more_creatures(state, *upkeep)
                || !crate::eternal_cards_v1::e3::player_targetable(
                    state,
                    target,
                    player,
                    Some(continuation.ctx.source),
                )
            {
                return Ok(false);
            }
            let chooser = if *upkeep == player {
                PlayerRef::Controller
            } else {
                PlayerRef::Opponent
            };
            push(
                continuation,
                EffectOp::Choice {
                    controller: chooser,
                    options: vec![
                        e5a(E5aOpV1::OathRevealV1 { player: *upkeep }),
                        EffectOp::Sequence(Vec::new()),
                    ],
                },
                &path,
            );
            Ok(false)
        }
        E5aOpV1::FlashV1 => {
            let cards = state.players[player.index()]
                .hand
                .iter()
                .copied()
                .filter(|&card| {
                    CARD_DEFS
                        .get(state.objects.get(card).card_def as usize)
                        .is_some_and(|def| def.has_type(CardType::Creature))
                })
                .map(|card| bind(state, card))
                .collect::<Vec<_>>();
            if cards.is_empty() {
                return Ok(false);
            }
            offer(
                continuation,
                E5aChoiceV1::FlashPutV1,
                player,
                cards,
                0,
                1,
                path,
            );
            Ok(true)
        }
        E5aOpV1::BeseechV1 => {
            let cards = library_cards(state, player, |_| true);
            if cards.is_empty() {
                shuffle(state, player)?;
                return Ok(false);
            }
            let bargained = continuation.ctx.optional_additional_cost_paid
                == Some(OptionalAdditionalCostDef::Bargain);
            offer(
                continuation,
                E5aChoiceV1::BeseechSearchV1 { bargained },
                player,
                cards,
                0,
                1,
                path,
            );
            Ok(true)
        }
        E5aOpV1::VampiricV1 => {
            let cards = library_cards(state, player, |_| true);
            if cards.is_empty() {
                shuffle(state, player)?;
                return Ok(false);
            }
            offer(
                continuation,
                E5aChoiceV1::VampiricSearchV1,
                player,
                cards,
                0,
                1,
                path,
            );
            Ok(true)
        }
        other => {
            execute(other, &continuation.ctx, state);
            Ok(false)
        }
    }
}

/// Kozilek's Command's two chosen modes, in printed order: the targets
/// are checked once as it resolves (608.2b), and each mode with a legal
/// target becomes a frame.
#[allow(clippy::too_many_arguments)]
fn kozilek(
    state: &GameState,
    continuation: &mut EffectContinuation,
    spawn: Option<u8>,
    scry: Option<u8>,
    creature: Option<u8>,
    cards: Option<u8>,
    spec: TargetSpec,
    path: &[u16],
) {
    let ctx = continuation.ctx.clone();
    let x = ctx.x_value;
    let legal_player = |slot: u8| {
        matches!(
            super::execute::legal_target(&ctx, state, usize::from(slot), spec),
            Some(Target::Player(_))
        )
    };
    let mut ops = Vec::new();
    if let Some(slot) = spawn.filter(|&slot| legal_player(slot)) {
        if x > 0 {
            ops.push(EffectOp::CreateTokensDynamic {
                token_def: token(super::ELDRAZI_SPAWN_TOKEN),
                controller: PlayerRef::Target(slot),
                count: crate::card_def::DynamicValueDef::Fixed(i32::from(x)),
                tapped: false,
            });
        }
    }
    if let Some(slot) = scry.filter(|&slot| legal_player(slot)) {
        if x > 0 {
            ops.push(e5a(E5aOpV1::ScryTargetXV1 { slot }));
        }
        ops.push(EffectOp::DrawCards {
            player: PlayerRef::Target(slot),
            count: 1,
        });
    }
    if let Some(slot) = creature {
        ops.push(e5a(E5aOpV1::ExileCreatureManaValueAtMostXV1 { slot, spec }));
    }
    if let Some(first) = cards {
        ops.push(e5a(E5aOpV1::ExileGraveyardTargetsV1 { first, spec }));
    }
    for op in ops.into_iter().rev() {
        push(continuation, op, path);
    }
}

/// Kozilek's Command: "Exile target creature with mana value X or less"
/// and "Exile up to X target cards from graveyards" restrict X by the
/// chosen targets. The engine announces X after targets, so X may not be
/// less than the creature's mana value or the number of graveyard targets;
/// this keeps exactly the casts 601.2b and 601.2c allow.
pub(crate) fn minimum_x(name: &str, targets: &[Target], state: &GameState) -> Option<u8> {
    if name != KOZILEKS_COMMAND {
        return None;
    }
    let mut creature_value = 0_u16;
    let mut graveyard_cards = 0_u16;
    for target in targets {
        let Target::Object(object) = *target else {
            continue;
        };
        match state.objects.get(object).zone {
            Zone::Battlefield => {
                creature_value =
                    creature_value.max(crate::engine::object_mana_value(state, object));
            }
            Zone::Graveyard => graveyard_cards = graveyard_cards.saturating_add(1),
            _ => {}
        }
    }
    Some(u8::try_from(creature_value.max(graveyard_cards)).unwrap_or(u8::MAX))
}

/// Applies an answered wave E5a choice. `Ok(true)` when it staged a
/// further choice.
pub(crate) fn finish(
    state: &mut GameState,
    continuation: &mut EffectContinuation,
    choice: &E3ChoiceV1,
    kind: &E5aChoiceV1,
    bindings: Vec<EffectObjectBinding>,
) -> Result<bool, String> {
    let player = choice.player;
    match kind {
        E5aChoiceV1::BeseechSearchV1 { bargained } => {
            let Some(card) = bindings.first().copied() else {
                shuffle(state, player)?;
                return Ok(false);
            };
            // Hidden: the opponent learns only that a card left the library.
            event::propose_and_commit(state, ProposedEvent::zone_change(card.object, Zone::Hand));
            shuffle(state, player)?;
            if !bargained || state.objects.get(card.object).zone != Zone::Hand {
                return Ok(false);
            }
            let found = bind(state, card.object);
            let castable = CARD_DEFS[state.objects.get(card.object).card_def as usize].mana_value
                <= 4
                && crate::engine::resolution_cast_v1::can_cast_exiled_without_mana(
                    state, player, found, None,
                );
            if !castable {
                return Ok(false);
            }
            offer(
                continuation,
                E5aChoiceV1::BeseechCastV1,
                player,
                vec![found],
                0,
                1,
                choice.path.clone(),
            );
            Ok(true)
        }
        // An answered card is cast by the interpreter before this runs; a
        // declined card stays in the hand.
        E5aChoiceV1::BeseechCastV1 => Ok(false),
        E5aChoiceV1::VampiricSearchV1 => {
            shuffle(state, player)?;
            let Some(card) = bindings.first() else {
                return Ok(false);
            };
            let library = &mut state.players[player.index()].library;
            let position = library
                .iter()
                .position(|id| *id == card.object)
                .ok_or("Vampiric Tutor's card left the library")?;
            let found = library.remove(position);
            library.insert(0, found);
            // Only the searcher knows the card on top.
            state.reveal_library_position(player, player, 0);
            Ok(false)
        }
        E5aChoiceV1::FlashPutV1 => {
            let Some(card) = bindings.first().copied() else {
                return Ok(false);
            };
            event::propose_and_commit(
                state,
                ProposedEvent::zone_change_to_battlefield_under_controller(card.object, player),
            );
            if state.objects.get(card.object).zone != Zone::Battlefield {
                return Ok(false);
            }
            let creature = bind(state, card.object);
            push(
                continuation,
                EffectOp::Choice {
                    controller: PlayerRef::Controller,
                    options: vec![
                        e5a(E5aOpV1::FlashPayV1 { creature }),
                        e5a(E5aOpV1::FlashSacrificeV1 { creature }),
                    ],
                },
                &choice.path,
            );
            Ok(false)
        }
        E5aChoiceV1::UginSearchV1 => {
            let cards = bindings
                .iter()
                .copied()
                .filter(|card| binding_live(state, *card))
                .collect::<Vec<_>>();
            if !cards.is_empty() {
                event::propose_and_commit_batch(
                    state,
                    cards
                        .iter()
                        .map(|card| ProposedEvent::zone_change(card.object, Zone::Exile))
                        .collect(),
                );
            }
            shuffle(state, player)?;
            for card in cards {
                let live = state.objects.get(card.object);
                if live.zone != Zone::Exile {
                    continue;
                }
                let permission = crate::engine::PlayPermission {
                    without_mana_cost: crate::engine::FreeCastV1(true),
                    object: card.object,
                    holder: player,
                    zone_change_generation: live.zone_change_count,
                    play_or_cast: crate::engine::PlayOrCast::Cast,
                    expiry: crate::engine::PlayPermissionExpiry::EndOfTurn,
                };
                state.engine.exile_play_permissions.push(permission);
            }
            Ok(false)
        }
    }
}

/// The card a Beseech the Mirror answer casts, if any.
pub(crate) fn resolution_cast_card(
    kind: &E5aChoiceV1,
    choice: &E3ChoiceV1,
    selected: &[Target],
) -> Option<EffectObjectBinding> {
    if *kind != E5aChoiceV1::BeseechCastV1 {
        return None;
    }
    let target = *selected.first()?;
    choice
        .bindings
        .iter()
        .copied()
        .find(|binding| Target::Object(binding.object) == target)
}
