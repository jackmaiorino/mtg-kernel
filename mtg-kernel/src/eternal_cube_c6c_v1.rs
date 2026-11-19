//! Card programs for the eternal catalog's Vintage Cube wave C6c
//! (`docs/design/eternal_formats_support_v1.md`): eight planeswalkers.
//!
//! `eternal_cards_v1` forwards this module's effect leaves here, and the
//! recipe tables of `build_eternal_c6c_v1.rs` name its programs. Every
//! table is keyed by a printed name that occurs only in the eternal registry,
//! so the Pauper, FDN and Standard catalogs never reach it. Behavior was read
//! from each card's current Oracle text and, where present, its XMage source.
#![cfg_attr(not(feature = "eternal-fixtures"), allow(dead_code))]

use crate::card_def::TargetSpec;
use crate::effect::{EffectContinuation, EffectFrame, EffectOp, ExecCtx, ObjectRef, PlayerRef};
use crate::eternal_cards_v1::EternalOpV1;
use crate::event::{self, ProposedEvent};
use crate::ids::PlayerId;
use crate::standard_cards_v1::StandardOpV1;
use crate::state::{GameState, ObjectLinkV4, Target, Zone};
use crate::trigger::TriggeredAbilityDef;
use serde::{Deserialize, Serialize};

/// Effect leaves owned by this module, carried as
/// `eternal_cards_v1::EternalOpV1::C6cV1`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum C6cOpV1 {
    /// Look privately, then offer the controller the optional bottom placement.
    JaceLookV1,
    /// The exact library card bound by the private look.
    BottomLookedCardV1 {
        player: PlayerId,
        card: ObjectLinkV4,
    },
    /// Exile the target player's library, then shuffle their hand into it.
    ExileLibraryShuffleHandV1,
}

impl C6cOpV1 {
    /// Whether the leaf may stop for a player's choice (or a draw that a
    /// replacement can interrupt), so it runs in the resumable interpreter.
    pub(crate) fn contains_player_choice(&self) -> bool {
        true
    }
}

fn leaf(op: C6cOpV1) -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::C6cV1(op)))
}

/// A permanent spell enters through the ordinary zone-change operation.
pub fn jace() -> EffectOp {
    EffectOp::MoveObject {
        object: ObjectRef::ThisSource,
        to_zone: Zone::Battlefield,
    }
}

pub fn jace_plus_two() -> EffectOp {
    leaf(C6cOpV1::JaceLookV1)
}

/// Brainstorm's exact root sequence, including replaceable draws and
/// independent private hand selections (last selected is topmost).
pub fn jace_zero() -> EffectOp {
    EffectOp::Sequence(vec![
        EffectOp::DrawCards {
            player: PlayerRef::Controller,
            count: 3,
        },
        EffectOp::PutCardsFromHandOnLibraryTop {
            player: PlayerRef::Controller,
            count: 2,
        },
    ])
}

/// The same incarnation-checked bounce used by Aether Spellbomb.
pub fn jace_minus_one() -> EffectOp {
    EffectOp::MoveAllTargets {
        to_zone: Zone::Hand,
    }
}

pub fn jace_minus_twelve() -> EffectOp {
    leaf(C6cOpV1::ExileLibraryShuffleHandV1)
}

// ---- Triggered abilities ------------------------------------------------------------

/// Triggered abilities of this module's cards, the tail of the trigger
/// tables.
pub(crate) fn triggers_for(_name: &str) -> &'static [TriggeredAbilityDef] {
    &[]
}

/// Target specs of this module's printed and queued triggered abilities.
pub(crate) fn trigger_target_spec(name: &str, effect: &EffectOp) -> Option<TargetSpec> {
    delayed_effect_matches(name, effect).then_some(TargetSpec::None)
}

/// Queued triggers owned by this module's cards.
pub(crate) fn delayed_effect_matches(_name: &str, _effect: &EffectOp) -> bool {
    false
}

// ---- Execution ----------------------------------------------------------------------

/// Runs one of this module's leaves in synchronous execution.
pub(crate) fn execute(_op: &C6cOpV1, _ctx: &ExecCtx, _state: &mut GameState) {
    // All current leaves use the resumable interpreter.
}

/// Runs one of this module's leaves in the resumable interpreter.
pub(crate) fn drive(
    state: &mut GameState,
    continuation: &mut EffectContinuation,
    op: &C6cOpV1,
    path: Vec<u16>,
) -> Result<bool, String> {
    match *op {
        C6cOpV1::JaceLookV1 => {
            let Some(Target::Player(player)) = continuation.ctx.targets.first().copied() else {
                return Ok(false);
            };
            let Some(&object) = state.players[player.index()].library.first() else {
                return Ok(false);
            };
            let card = ObjectLinkV4 {
                object,
                zone_change_count: state.objects.get(object).zone_change_count,
            };
            // spellbench-reviewed-look: only the ability's controller looks.
            state.reveal_library_top(continuation.ctx.controller, player, 1);
            continuation.frames.push(EffectFrame::Program {
                op: EffectOp::Choice {
                    controller: PlayerRef::Controller,
                    options: vec![
                        EffectOp::Sequence(Vec::new()),
                        leaf(C6cOpV1::BottomLookedCardV1 { player, card }),
                    ],
                },
                path,
            });
        }
        C6cOpV1::BottomLookedCardV1 { player, card } => {
            if state.players[player.index()].library.first() != Some(&card.object)
                || state.objects.get(card.object).zone_change_count != card.zone_change_count
            {
                return Ok(false);
            }
            let owner_knew = state
                .known_library_cards(player, player)
                .iter()
                .any(|entry| {
                    entry.position == 0
                        && entry.object == card.object
                        && entry.zone_change_count == card.zone_change_count
                });
            // Expressive Iteration uses this atomic in-library movement.
            // Unlike scry, Jace grants no new knowledge to the library's owner.
            // Preserve their preexisting knowledge, without revealing the card.
            state.apply_scry_result(player, &[card], &[], &[card.object])?;
            if player != continuation.ctx.controller && !owner_knew {
                state.library_knowledge[player.index()][player.index()]
                    .retain(|entry| entry.object != card.object);
            }
        }
        C6cOpV1::ExileLibraryShuffleHandV1 => {
            let Some(Target::Player(player)) = continuation.ctx.targets.first().copied() else {
                return Ok(false);
            };
            let library = state.players[player.index()].library.clone();
            event::propose_and_commit_batch(
                state,
                library
                    .into_iter()
                    .map(|card| ProposedEvent::zone_change(card, Zone::Exile))
                    .collect(),
            );
            let hand = state.players[player.index()].hand.clone();
            event::propose_and_commit_batch(
                state,
                hand.into_iter()
                    .map(|card| ProposedEvent::zone_change(card, Zone::Library))
                    .collect(),
            );
            // Campfire's shuffle primitive, through the ordinary frame so
            // seeded randomness and library-knowledge invalidation are shared.
            continuation
                .frames
                .push(EffectFrame::ShuffleLibrary { player, path });
        }
    }
    Ok(false)
}
