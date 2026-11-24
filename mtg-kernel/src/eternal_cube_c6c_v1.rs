//! Card programs for the eternal catalog's Vintage Cube wave C6c
//! (`docs/design/eternal_formats_support_v1.md`): eight planeswalkers.
//!
//! `eternal_cards_v1` forwards this module's effect leaves here, and the
//! recipe tables of `build_eternal_c6c_v1.rs` name its programs. Every
//! table is keyed by a printed name that occurs only in the eternal registry,
//! so the Pauper, FDN and Standard catalogs never reach it. Behavior was read
//! from each card's current Oracle text and, where present, its XMage source.
//!
//! Saheeli remains no_effect: E7a's definition-only copy helper cannot transfer
//! copiable token exceptions/abilities or retrieve those values from a departed
//! second target. Shifting Woodland's graveyard-copy expiry does not supply
//! those battlefield-copy semantics. Chandra remains no_effect because the
//! exile resolution-cast route waives mana costs; its normal-cost cast is not
//! representable by that route. Garruk's runtime lives in the submodule.
#![cfg_attr(not(feature = "eternal-fixtures"), allow(dead_code))]

use crate::card_def::{Keywords, TargetSpec, CARD_DEFS};
use crate::effect::{
    EffectContinuation, EffectFrame, EffectOp, ExecCtx, LibraryLookCount, LibraryPickFilterV1,
    LibraryPickRule, LibraryPickSelectionV1, ObjectRef, PlayerRef,
};
use crate::eternal_cards_v1::EternalOpV1;
use crate::event::{self, CommittedEvent, ProposedEvent};
use crate::ids::{ObjectId, PlayerId};
use crate::standard_cards_v1::{StandardOpV1, StandardTargetV1};
use crate::state::{AbilitySourceContractV4, GameState, ObjectLinkV4, Step, Target, Zone};
use crate::trigger::{PendingTrigger, TriggeredAbilityDef};
use serde::{Deserialize, Serialize};

mod garruk;
pub(crate) use garruk::{begin_turn, capture_attack, capture_batch_entries, capture_single_entry};

const TEFERI: &str = "Teferi, Hero of Dominaria";

/// Player-owned records survive departure or control changes of their maker.
/// The historical source convention is shared with Tezzeret, Cruel Captain.
/// In particular, targeting retains the maker's source characteristics rather
/// than introducing a separate colorless emblem source (the existing convention).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EmblemV1 {
    pub controller: PlayerId,
    pub source: AbilitySourceContractV4,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct C6cStateV1 {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub teferi_emblems: Vec<EmblemV1>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub untaps: Vec<DelayedUntapV1>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub garruk_grants: Vec<EmblemV1>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub garruk_pending: Vec<PendingTrigger>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub garruk_entries: Vec<usize>,
}

impl C6cStateV1 {
    pub(crate) fn is_empty(&self) -> bool {
        self.teferi_emblems.is_empty()
            && self.untaps.is_empty()
            && self.garruk_grants.is_empty()
            && self.garruk_pending.is_empty()
            && self.garruk_entries.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DelayedUntapV1 {
    pub source: AbilitySourceContractV4,
    pub controller: PlayerId,
    pub created_in_end_step: Option<crate::eternal_legacy_e9a_v1::TurnV1>,
}

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
    /// Reveal two cards, then let the opponent choose the card for hand.
    KarnRevealV1,
    KarnPartitionV1 {
        hand: ObjectLinkV4,
        exile: Option<ObjectLinkV4>,
    },
    /// Choose an owned silver-counter card, without targeting it.
    KarnRecoverV1,
    KarnRecoverCardV1 {
        card: ObjectLinkV4,
    },
    TeferiScheduleUntapV1,
    TeferiThirdFromTopV1,
    TeferiEmblemV1,
    TeferiExileV1,
    EmperorCounterFirstStrikeV1,
    GarrukUntapV1,
    GarrukGrantV1,
    GarrukPumpV1 {
        creatures: Vec<ObjectLinkV4>,
    },
}

impl C6cOpV1 {
    /// Whether the leaf may stop for a player's choice (or a draw that a
    /// replacement can interrupt), so it runs in the resumable interpreter.
    pub(crate) fn contains_player_choice(&self) -> bool {
        !matches!(
            self,
            Self::EmperorCounterFirstStrikeV1
                | Self::GarrukUntapV1
                | Self::GarrukGrantV1
                | Self::GarrukPumpV1 { .. }
        )
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

/// Thundertrap Trainer's exact optional selection and random bottom order.
pub fn narset_minus_two() -> EffectOp {
    EffectOp::LookTopSelectV1 {
        player: PlayerRef::Controller,
        count: LibraryLookCount::Fixed(4),
        rule: LibraryPickRule {
            pick: 1,
            choose_rest_order: false,
            selection: Some(LibraryPickSelectionV1 {
                without_mana_cost: crate::engine::FreeCastV1(false),
                filter: LibraryPickFilterV1::NoncreatureNonlandV1,
                optional: true,
                destination: Zone::Hand,
                reveal_selected: true,
            }),
        },
        pick_x: false,
    }
}

/// A prohibition is checked before replacement choices or empty-library draws.
/// The engine resets both players' actual draw counts at every untap step.
pub(crate) fn prevents_draw(state: &GameState, player: PlayerId) -> bool {
    state.players[player.index()].draws_this_turn != 0
        && state.objects.iter().any(|(object, live)| {
            live.zone == Zone::Battlefield
                && live.controller == player.opponent()
                && live.v4.face_index == 0
                && live.v4.face_down_v1.is_none()
                && CARD_DEFS[live.card_def as usize].name == "Narset, Parter of Veils"
                && CARD_DEFS[live.card_def as usize].is_executable()
                && crate::continuous_characteristics_v1::printed_abilities_active(state, object)
        })
}

pub fn karn_plus_one() -> EffectOp {
    leaf(C6cOpV1::KarnRevealV1)
}

pub fn karn_minus_one() -> EffectOp {
    leaf(C6cOpV1::KarnRecoverV1)
}

/// The same token used by Urza's Saga and Simulacrum Synthesizer.
pub fn karn_minus_two() -> EffectOp {
    EffectOp::CreateToken {
        token_def: crate::card_def::card_id_by_name("Karn Construct Token")
            .expect("Karn Construct Token in CARD_DEFS"),
        controller: PlayerRef::Controller,
    }
}

/// The draw is a direct root element so competing replacements can choose.
pub fn teferi_plus_one() -> EffectOp {
    EffectOp::Sequence(vec![
        EffectOp::DrawCards {
            player: PlayerRef::Controller,
            count: 1,
        },
        leaf(C6cOpV1::TeferiScheduleUntapV1),
    ])
}

pub fn teferi_minus_three() -> EffectOp {
    leaf(C6cOpV1::TeferiThirdFromTopV1)
}

pub fn teferi_minus_eight() -> EffectOp {
    leaf(C6cOpV1::TeferiEmblemV1)
}

/// This flag resets at each player's untap and on every zone change.
/// It therefore tracks the current permanent incarnation and actual turn,
/// unlike the round-valued entered_battlefield_turn field.
pub(crate) fn emperor_instant_loyalty(state: &GameState, source: ObjectId) -> bool {
    cfg!(feature = "eternal-fixtures")
        && state.objects.try_get(source).is_some_and(|live| {
            live.zone == Zone::Battlefield
                && live.v4.entered_battlefield_this_turn
                && live.v4.face_index == 0
                && live.v4.face_down_v1.is_none()
                && CARD_DEFS[live.card_def as usize].name == "The Wandering Emperor"
                && CARD_DEFS[live.card_def as usize].is_executable()
                && crate::continuous_characteristics_v1::printed_abilities_active(state, source)
        })
}

pub fn emperor_plus_one() -> EffectOp {
    leaf(C6cOpV1::EmperorCounterFirstStrikeV1)
}

/// Experimental Synthesizer's existing exact Samurai token.
pub fn emperor_minus_one() -> EffectOp {
    EffectOp::CreateToken {
        token_def: crate::card_def::card_id_by_name("Samurai Token")
            .expect("Samurai Token in CARD_DEFS"),
        controller: PlayerRef::Controller,
    }
}

pub fn emperor_minus_two() -> EffectOp {
    EffectOp::Sequence(vec![
        EffectOp::MoveAllTargets {
            to_zone: Zone::Exile,
        },
        EffectOp::GainLife {
            player: PlayerRef::Controller,
            amount: 2,
        },
    ])
}

fn teferi_untap() -> EffectOp {
    // Palinchron's existing selection convention offers tapped lands only.
    EffectOp::UntapUpToLands {
        chooser: PlayerRef::Controller,
        max_targets: 2,
    }
}

pub fn garruk_plus_two() -> EffectOp {
    leaf(C6cOpV1::GarrukUntapV1)
}
pub fn garruk_minus_three() -> EffectOp {
    EffectOp::CreateToken {
        token_def: crate::card_def::card_id_by_name("Garruk Beast Token")
            .expect("Garruk Beast Token in CARD_DEFS"),
        controller: PlayerRef::Controller,
    }
}
pub fn garruk_minus_four() -> EffectOp {
    leaf(C6cOpV1::GarrukGrantV1)
}

fn teferi_exile() -> EffectOp {
    leaf(C6cOpV1::TeferiExileV1)
}

/// E8a's next-end-step scheduling, independent of the source's live zone.
pub(crate) fn delayed_triggers(
    state: &mut GameState,
    events: &[CommittedEvent],
) -> Vec<PendingTrigger> {
    if !cfg!(feature = "eternal-fixtures")
        || state.eternal_v1.as_ref().is_none_or(|e| e.c6c.is_empty())
    {
        return Vec::new();
    }
    let now = crate::eternal_legacy_e9a_v1::TurnV1::now(state);
    let records = &mut state.eternal_v1.as_mut().expect("present above").c6c;
    let mut triggers = std::mem::take(&mut records.garruk_pending);
    records.garruk_entries.clear();
    for event in events {
        match event {
            CommittedEvent::Draw {
                player,
                object: Some(_),
            } => {
                for emblem in &records.teferi_emblems {
                    if emblem.controller == *player {
                        let mut pending = crate::eternal_cards_v1::delayed_trigger(
                            emblem.source,
                            emblem.controller,
                            teferi_exile(),
                        );
                        pending.target_spec =
                            TargetSpec::StandardV1(StandardTargetV1::OpponentPermanentV1);
                        triggers.push(pending);
                    }
                }
            }
            CommittedEvent::BeginningEndStep { .. } => {
                let (ready, waiting): (Vec<_>, Vec<_>) = std::mem::take(&mut records.untaps)
                    .into_iter()
                    .partition(|record| record.created_in_end_step != Some(now));
                records.untaps = waiting;
                for record in ready {
                    triggers.push(crate::eternal_cards_v1::delayed_trigger(
                        record.source,
                        record.controller,
                        teferi_untap(),
                    ));
                }
            }
            _ => {}
        }
    }
    triggers
}

fn live_in(state: &GameState, card: ObjectLinkV4, zone: Zone) -> bool {
    state
        .objects
        .try_get(card.object)
        .is_some_and(|live| live.zone == zone && live.zone_change_count == card.zone_change_count)
}

fn has_silver(state: &GameState, player: PlayerId, card: ObjectLinkV4) -> bool {
    live_in(state, card, Zone::Exile)
        && state.objects.get(card.object).owner == player
        && state
            .eternal_v1
            .as_ref()
            .is_some_and(|eternal| eternal.c6c_silver_cards.contains(&card))
}

fn karn_partition(state: &mut GameState, hand: ObjectLinkV4, exile: Option<ObjectLinkV4>) {
    if live_in(state, hand, Zone::Library) {
        event::propose_and_commit(
            state,
            ProposedEvent::zone_change_preserving_known_identity(hand.object, Zone::Hand),
        );
    }
    if let Some(card) = exile.filter(|&card| live_in(state, card, Zone::Library)) {
        event::propose_and_commit(state, ProposedEvent::zone_change(card.object, Zone::Exile));
        if state.objects.get(card.object).zone == Zone::Exile {
            let exiled = ObjectLinkV4 {
                object: card.object,
                zone_change_count: state.objects.get(card.object).zone_change_count,
            };
            // Like Emperor of Bones, bind the resulting exile incarnation.
            // Unlike Emperor, this record has no dependency on the source.
            state
                .eternal_v1
                .get_or_insert_with(Default::default)
                .c6c_silver_cards
                .push(exiled);
        }
    }
}

// ---- Triggered abilities ------------------------------------------------------------

/// Triggered abilities of this module's cards, falling through to wave
/// Pf1a.
pub(crate) fn triggers_for(name: &str) -> &'static [TriggeredAbilityDef] {
    crate::eternal_pauper_pf1a_v1::triggers_for(name)
}

/// Target specs of this module's printed and queued triggered abilities.
pub(crate) fn trigger_target_spec(name: &str, effect: &EffectOp) -> Option<TargetSpec> {
    if name == TEFERI && *effect == teferi_exile() {
        Some(TargetSpec::StandardV1(
            StandardTargetV1::OpponentPermanentV1,
        ))
    } else {
        delayed_effect_matches(name, effect).then_some(TargetSpec::None)
    }
}

/// Queued triggers owned by this module's cards.
pub(crate) fn delayed_effect_matches(name: &str, effect: &EffectOp) -> bool {
    (name == TEFERI && (*effect == teferi_untap() || *effect == teferi_exile()))
        || garruk::delayed_effect_matches(name, effect)
}

// ---- Execution ----------------------------------------------------------------------

/// Runs one of this module's leaves in synchronous execution.
pub(crate) fn execute(op: &C6cOpV1, ctx: &ExecCtx, state: &mut GameState) {
    if matches!(
        op,
        C6cOpV1::GarrukUntapV1 | C6cOpV1::GarrukGrantV1 | C6cOpV1::GarrukPumpV1 { .. }
    ) {
        garruk::execute(op, ctx, state);
        return;
    }
    if let C6cOpV1::EmperorCounterFirstStrikeV1 = op {
        // Do not resolve an absent optional target as the source, and do not
        // grant the keyword to a new incarnation of a departed target.
        if ctx.targets.is_empty()
            || !ctx.target_incarnation_matches(0, state)
            || !ctx.ability_source_contract.is_some_and(|source| {
                crate::engine::effect_target_is_legal_from_ability_source(
                    state,
                    source,
                    ctx.controller,
                    TargetSpec::StandardV1(StandardTargetV1::UpToOneCreature),
                    &ctx.targets,
                    0,
                )
            })
        {
            return;
        }
        crate::effect::execute(
            &EffectOp::AddCountersToTarget {
                target_index: 0,
                optional: true,
                plus1_plus1: 1,
                lifelink: 0,
                stun: 0,
            },
            ctx,
            state,
        );
        crate::effect::execute(
            &EffectOp::GrantKeywordTargetUntilEndOfTurn {
                object: ObjectRef::Target(0),
                keyword: Keywords::FIRST_STRIKE,
            },
            ctx,
            state,
        );
    }
}

/// Runs one of this module's leaves in the resumable interpreter.
pub(crate) fn drive(
    state: &mut GameState,
    continuation: &mut EffectContinuation,
    op: &C6cOpV1,
    path: Vec<u16>,
) -> Result<bool, String> {
    match *op {
        C6cOpV1::EmperorCounterFirstStrikeV1
        | C6cOpV1::GarrukUntapV1
        | C6cOpV1::GarrukGrantV1
        | C6cOpV1::GarrukPumpV1 { .. } => execute(op, &continuation.ctx, state),
        C6cOpV1::TeferiScheduleUntapV1 => {
            if let Some(source) = continuation.ctx.ability_source_contract {
                let created_in_end_step = (state.step == Step::End)
                    .then(|| crate::eternal_legacy_e9a_v1::TurnV1::now(state));
                state
                    .eternal_v1
                    .get_or_insert_with(Default::default)
                    .c6c
                    .untaps
                    .push(DelayedUntapV1 {
                        source,
                        controller: continuation.ctx.controller,
                        created_in_end_step,
                    });
            }
        }
        C6cOpV1::TeferiEmblemV1 => {
            if let Some(source) = continuation.ctx.ability_source_contract {
                state
                    .eternal_v1
                    .get_or_insert_with(Default::default)
                    .c6c
                    .teferi_emblems
                    .push(EmblemV1 {
                        controller: continuation.ctx.controller,
                        source,
                    });
            }
        }
        C6cOpV1::TeferiThirdFromTopV1 | C6cOpV1::TeferiExileV1 => {
            let ctx = &continuation.ctx;
            let Some(Target::Object(object)) = ctx.targets.first().copied() else {
                return Ok(false);
            };
            let spec = if matches!(op, C6cOpV1::TeferiThirdFromTopV1) {
                TargetSpec::NonlandPermanent
            } else {
                TargetSpec::StandardV1(StandardTargetV1::OpponentPermanentV1)
            };
            if ctx.target_incarnation_matches(0, state)
                && ctx.ability_source_contract.is_some_and(|source| {
                    crate::engine::effect_target_is_legal_from_ability_source(
                        state,
                        source,
                        ctx.controller,
                        spec,
                        &ctx.targets,
                        0,
                    )
                })
            {
                let proposal = if matches!(op, C6cOpV1::TeferiThirdFromTopV1) {
                    ProposedEvent::public_library_insert(
                        object,
                        event::LibraryPlacement::ThirdFromTop,
                    )
                } else {
                    ProposedEvent::zone_change(object, Zone::Exile)
                };
                event::propose_and_commit(state, proposal);
            }
        }
        C6cOpV1::KarnRevealV1 => {
            let player = continuation.ctx.controller;
            let cards: Vec<ObjectLinkV4> = state.players[player.index()]
                .library
                .iter()
                .take(2)
                .map(|&object| ObjectLinkV4 {
                    object,
                    zone_change_count: state.objects.get(object).zone_change_count,
                })
                .collect();
            // spellbench-reviewed-look: both players see the revealed cards.
            for observer in [player, player.opponent()] {
                state.reveal_library_top(observer, player, cards.len());
            }
            match cards.as_slice() {
                [card] => karn_partition(state, *card, None),
                [first, second] => continuation.frames.push(EffectFrame::Program {
                    op: EffectOp::Choice {
                        controller: PlayerRef::Opponent,
                        options: vec![
                            leaf(C6cOpV1::KarnPartitionV1 {
                                hand: *first,
                                exile: Some(*second),
                            }),
                            leaf(C6cOpV1::KarnPartitionV1 {
                                hand: *second,
                                exile: Some(*first),
                            }),
                        ],
                    },
                    path,
                }),
                _ => {}
            }
        }
        C6cOpV1::KarnPartitionV1 { hand, exile } => karn_partition(state, hand, exile),
        C6cOpV1::KarnRecoverV1 => {
            let player = continuation.ctx.controller;
            let cards: Vec<ObjectLinkV4> = state
                .eternal_v1
                .as_ref()
                .map(|eternal| {
                    eternal
                        .c6c_silver_cards
                        .iter()
                        .copied()
                        .filter(|&card| has_silver(state, player, card))
                        .collect()
                })
                .unwrap_or_default();
            if !cards.is_empty() {
                continuation.frames.push(EffectFrame::Program {
                    op: EffectOp::Choice {
                        controller: PlayerRef::Controller,
                        options: cards
                            .into_iter()
                            .map(|card| leaf(C6cOpV1::KarnRecoverCardV1 { card }))
                            .collect(),
                    },
                    path,
                });
            }
        }
        C6cOpV1::KarnRecoverCardV1 { card } => {
            if has_silver(state, continuation.ctx.controller, card) {
                event::propose_and_commit(
                    state,
                    ProposedEvent::zone_change(card.object, Zone::Hand),
                );
            }
        }
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
