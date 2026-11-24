//! Card programs for the eternal catalog's Pauper field wave Pf2a
//! (`docs/design/eternal_formats_support_v1.md`): existing-mechanics permanents
//! and spells.
//!
//! `eternal_cards_v1` forwards this module's effect leaves and triggers here,
//! and the recipe tables of `build_eternal_pf2a_v1.rs` name its programs. Every
//! table is keyed by a printed name that occurs only in the eternal registry,
//! so the Pauper, FDN and Standard catalogs never reach it. Behavior follows
//! each card's current Oracle text.
//!
//! Cards:
//! - Radiant Grove: Forest Plains dual land that enters tapped.
//! - Escape Tunnel: fetch basic land tapped, or target creature with power 2 or
//!   less can't be blocked this turn.
//! - Tuktuk Rubblefort: Defender, Reach, "Creatures you control have haste."
//! - Granger Guildmage: `{R}, {T}: 1 damage to any target and 1 damage to you`;
//!   `{W}, {T}: Target creature gains first strike until end of turn.`
//! - Obsidian Acolyte: printed protection from black and `{W}: Target creature
//!   gains protection from black until end of turn.`
//! - Spirit Mascot: "Whenever one or more cards leave your graveyard, put a
//!   +1/+1 counter on this creature."
//! - Keep Watch: "Draw a card for each attacking creature."
//! - Embrace the Paradox: "Draw three cards. You may put a land card from your
//!   hand onto the battlefield tapped."
//! - Thrilling Discovery: "You gain 2 life. Then you may discard two cards. If
//!   you do, draw three cards."
//! - Rites of Initiation: "Discard any number of cards at random. Creatures you
//!   control get +1/+0 until end of turn for each card discarded this way."
#![cfg_attr(not(feature = "eternal-fixtures"), allow(dead_code))]

use crate::card_def::{card_id_by_name, CardDef, CardType, Keywords, TargetSpec, CARD_DEFS};
use crate::effect::{
    EffectContinuation, EffectFrame, EffectObjectBinding, EffectOp, ExecCtx, ObjectRef, PlayerRef,
};
use crate::eternal_cards_v1::e3::{E3ChoiceKindV1, E3ChoiceV1};
use crate::eternal_cards_v1::{EternalOpV1, EternalTriggerV1};
use crate::eternal_legacy_e9a_v1::{active_named, live_link};
use crate::eternal_sideboard_s1d_v1::S1dTargetV1;
use crate::event::{self, CommittedEvent, ProposedEvent};
use crate::ids::{ObjectId, PlayerId};
use crate::mana::ManaColor;
use crate::standard_cards_v1::{StandardOpV1, StandardTargetV1, StandardTriggerV1};
use crate::state::{GameState, Target, Zone};
use crate::trigger::{TriggerCondition, TriggeredAbilityDef};
use serde::{Deserialize, Serialize};

pub(crate) const OBSIDIAN_ACOLYTE: &str = "Obsidian Acolyte";
pub(crate) const SPIRIT_MASCOT: &str = "Spirit Mascot";
pub(crate) const TUKTUK_RUBBLEFORT: &str = "Tuktuk Rubblefort";

const ESCAPE_TUNNEL_TARGET: TargetSpec = TargetSpec::StandardV1(StandardTargetV1::S1dV1(
    S1dTargetV1::CreaturePowerAtMostV1(2),
));

/// Trigger conditions owned by this module, carried as
/// `eternal_cards_v1::EternalTriggerV1::Pf2aV1`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pf2aTriggerV1 {
    /// Spirit Mascot: "Whenever one or more cards leave your graveyard".
    /// One trigger per simultaneous batch (CR 603.2c).
    CardsLeaveYourGraveyardV1,
}

/// Effect leaves owned by this module, carried as
/// `eternal_cards_v1::EternalOpV1::Pf2aV1`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Pf2aOpV1 {
    /// Escape Tunnel: target creature with power 2 or less can't be blocked
    /// this turn.
    EscapeTunnelUnblockableV1,
    /// Granger Guildmage: deal 1 damage to any target and 1 damage to you.
    GrangerPingV1,
    /// Obsidian Acolyte: target creature gains protection from black until end
    /// of turn.
    ObsidianProtectionV1,
    /// Spirit Mascot: put a +1/+1 counter on this creature's exact incarnation.
    SpiritMascotCounterV1,
    /// Keep Watch: draw a card for each attacking creature. Stages a genuine
    /// `DrawCards` frame so draw replacements apply.
    KeepWatchV1,
    /// Embrace the Paradox: you may put a land card from your hand onto the
    /// battlefield tapped.
    EmbraceLandTappedV1,
    /// Thrilling Discovery: you may discard two cards; if you do, draw three.
    ThrillingDiscoveryV1,
    /// Thrilling Discovery accepted branch: discard exactly two cards and
    /// stage three draws.
    ThrillingDiscardTwoV1,
    /// Rites of Initiation: choose how many cards (0..=hand.len()) to discard
    /// at random.
    RitesOfInitiationV1,
    /// Rites of Initiation chosen count: discard `count` cards at random, then
    /// give controlled creatures `+discarded/+0` until end of turn.
    RitesDiscardCountV1 { count: u16 },
}

impl Pf2aOpV1 {
    /// Whether this leaf may suspend for a player choice or stage a draw frame.
    pub(crate) fn contains_player_choice(&self) -> bool {
        matches!(
            self,
            Self::KeepWatchV1
                | Self::EmbraceLandTappedV1
                | Self::ThrillingDiscoveryV1
                | Self::ThrillingDiscardTwoV1
                | Self::RitesOfInitiationV1
        )
    }
}

/// Resolution choices owned by this module, carried as
/// `E3ChoiceKindV1::Pf2aV1`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Pf2aChoiceV1 {
    /// Embrace the Paradox: up to one land card from the controller's hand.
    EmbraceLandTappedV1,
    /// Thrilling Discovery: exactly two cards from the controller's hand to
    /// discard.
    ThrillingDiscardTwoV1,
}

impl Pf2aChoiceV1 {
    pub(crate) fn chooser_private(&self) -> bool {
        match self {
            Self::EmbraceLandTappedV1 | Self::ThrillingDiscardTwoV1 => true,
        }
    }

    pub(crate) fn public_purpose(&self) -> crate::rl::TargetSelectionPurposeV4 {
        match self {
            Self::EmbraceLandTappedV1 | Self::ThrillingDiscardTwoV1 => {
                crate::rl::TargetSelectionPurposeV4::CardSelection
            }
        }
    }
}

fn pf2a(op: Pf2aOpV1) -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::Pf2aV1(op)))
}

fn nothing() -> EffectOp {
    EffectOp::Sequence(Vec::new())
}

fn halt(state: &mut GameState, source: ObjectId) {
    state.engine.halted = Some((
        crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
        source,
    ));
}

fn bind(state: &GameState, object: ObjectId) -> EffectObjectBinding {
    let live = state.objects.get(object);
    EffectObjectBinding {
        object,
        expected_zone: live.zone,
        expected_zone_change_count: live.zone_change_count,
    }
}

fn definition(state: &GameState, object: ObjectId) -> Option<&'static CardDef> {
    let live = state.objects.try_get(object)?;
    CARD_DEFS.get(live.card_def as usize)
}

fn target_is_legal(ctx: &ExecCtx, state: &GameState, spec: TargetSpec, index: usize) -> bool {
    if !ctx.target_incarnation_matches(index, state) {
        return false;
    }
    if let Some(source) = ctx.ability_source_contract {
        source.source == ctx.source
            && source.controller == ctx.controller
            && crate::engine::effect_target_is_legal_from_ability_source(
                state,
                source,
                ctx.controller,
                spec,
                &ctx.targets,
                index,
            )
    } else {
        crate::engine::effect_target_is_legal(
            state,
            ctx.source,
            ctx.controller,
            spec,
            &ctx.targets,
            index,
        )
    }
}

fn offer(
    continuation: &mut EffectContinuation,
    kind: Pf2aChoiceV1,
    player: PlayerId,
    bindings: Vec<EffectObjectBinding>,
    (min, max): (u16, u16),
    path: Vec<u16>,
) {
    let remaining = continuation.frames.clone();
    crate::eternal_cards_v1::e3::offer(
        continuation,
        E3ChoiceV1 {
            kind: E3ChoiceKindV1::Pf2aV1(kind),
            player,
            candidates: bindings
                .iter()
                .map(|binding| Target::Object(binding.object))
                .collect(),
            bindings,
            min,
            max,
            ordered: false,
            path,
            remaining,
        },
    );
}

fn push_choice(
    continuation: &mut EffectContinuation,
    controller: PlayerRef,
    options: Vec<EffectOp>,
    path: Vec<u16>,
) {
    continuation.frames.push(EffectFrame::Program {
        op: EffectOp::Choice {
            controller,
            options,
        },
        path,
    });
}

/// Number of attacking creatures currently on the battlefield.
pub(crate) fn attacking_creature_count(state: &GameState) -> u32 {
    let count = state
        .engine
        .combat
        .attackers
        .iter()
        .filter(|&&id| {
            state.objects.try_get(id).is_some_and(|live| {
                live.zone == Zone::Battlefield
                    && crate::engine::object_has_type(state, id, CardType::Creature)
            })
        })
        .count();
    u32::try_from(count).unwrap_or(u32::MAX)
}

// ---- Programs -----------------------------------------------------------------------

/// Escape Tunnel's second ability: "Target creature with power 2 or less can't
/// be blocked this turn."
pub fn escape_tunnel_unblockable() -> EffectOp {
    pf2a(Pf2aOpV1::EscapeTunnelUnblockableV1)
}

/// Granger Guildmage's red ability: "This creature deals 1 damage to any target
/// and 1 damage to you."
pub fn granger_guildmage_ping() -> EffectOp {
    pf2a(Pf2aOpV1::GrangerPingV1)
}

/// Obsidian Acolyte's ability: "Target creature gains protection from black
/// until end of turn."
pub fn obsidian_acolyte_protection() -> EffectOp {
    pf2a(Pf2aOpV1::ObsidianProtectionV1)
}

/// Spirit Mascot's trigger: "put a +1/+1 counter on this creature."
fn spirit_mascot_counter() -> EffectOp {
    pf2a(Pf2aOpV1::SpiritMascotCounterV1)
}

/// Keep Watch: "Draw a card for each attacking creature."
pub fn keep_watch() -> EffectOp {
    pf2a(Pf2aOpV1::KeepWatchV1)
}

/// Embrace the Paradox: "Draw three cards. You may put a land card from your
/// hand onto the battlefield tapped."
pub fn embrace_the_paradox() -> EffectOp {
    EffectOp::Sequence(vec![
        EffectOp::DrawCards {
            player: PlayerRef::Controller,
            count: 3,
        },
        pf2a(Pf2aOpV1::EmbraceLandTappedV1),
    ])
}

/// Thrilling Discovery: "You gain 2 life. Then you may discard two cards. If
/// you do, draw three cards."
pub fn thrilling_discovery() -> EffectOp {
    EffectOp::Sequence(vec![
        EffectOp::GainLife {
            player: PlayerRef::Controller,
            amount: 2,
        },
        pf2a(Pf2aOpV1::ThrillingDiscoveryV1),
    ])
}

/// Rites of Initiation: "Discard any number of cards at random. Creatures you
/// control get +1/+0 until end of turn for each card discarded this way."
pub fn rites_of_initiation() -> EffectOp {
    pf2a(Pf2aOpV1::RitesOfInitiationV1)
}

// ---- Triggered abilities ------------------------------------------------------------

const SPIRIT_MASCOT_TRIGGERS: [TriggeredAbilityDef; 1] = [TriggeredAbilityDef {
    condition: TriggerCondition::StandardV1(StandardTriggerV1::EternalV1(
        EternalTriggerV1::Pf2aV1(Pf2aTriggerV1::CardsLeaveYourGraveyardV1),
    )),
    home_zone: Zone::Battlefield,
    intervening_if_kicked: false,
    intervening_if_controls_another_source_card: false,
    face_index: 0,
    effect: spirit_mascot_counter,
}];

fn own_triggers(name: &str) -> &'static [TriggeredAbilityDef] {
    match name {
        SPIRIT_MASCOT => &SPIRIT_MASCOT_TRIGGERS,
        _ => &[],
    }
}

/// Triggered abilities of this module's cards, falling through to wave Pf2b.
pub(crate) fn triggers_for(name: &str) -> &'static [TriggeredAbilityDef] {
    match own_triggers(name) {
        [] => crate::eternal_pauper_pf2b_v1::triggers_for(name),
        triggers => triggers,
    }
}

/// Target specs of this module's printed and queued triggered abilities.
pub(crate) fn trigger_target_spec(name: &str, effect: &EffectOp) -> Option<TargetSpec> {
    own_triggers(name)
        .iter()
        .any(|trigger| (trigger.effect)() == *effect)
        .then_some(TargetSpec::None)
}

/// Queued triggers owned by this module's cards: none.
pub(crate) fn delayed_effect_matches(_name: &str, _effect: &EffectOp) -> bool {
    false
}

fn card_left_your_graveyard(
    state: &GameState,
    event: &CommittedEvent,
    controller: PlayerId,
) -> bool {
    let CommittedEvent::ZoneChange {
        object,
        from: Zone::Graveyard,
        ..
    } = *event
    else {
        return false;
    };
    state.objects.try_get(object).is_some_and(|card| {
        let def = &CARD_DEFS[card.card_def as usize];
        card.owner == controller
            && !card.v4.is_token
            && !def.is_token
            && card.spell_copy_origin.is_none()
    })
}

/// Trigger condition matcher for `Pf2aTriggerV1`.
pub(crate) fn trigger_matches(
    condition: Pf2aTriggerV1,
    events: &[CommittedEvent],
    index: usize,
    source: ObjectId,
    state: &GameState,
) -> bool {
    let controller = state.objects.get(source).controller;
    match condition {
        Pf2aTriggerV1::CardsLeaveYourGraveyardV1 => {
            card_left_your_graveyard(state, &events[index], controller)
                && !events[..index]
                    .iter()
                    .any(|event| card_left_your_graveyard(state, event, controller))
        }
    }
}

// ---- Statics: Obsidian Acolyte & Tuktuk Rubblefort ----------------------------------

/// Obsidian Acolyte: "Protection from black", as a color mask. Wave E9a's
/// protection mask chain includes it, so targeting, blocking, damage and
/// attachment SBAs all honor it.
pub(crate) fn protection_mask(state: &GameState, host: ObjectId) -> u8 {
    if active_named(state, host, OBSIDIAN_ACOLYTE) {
        crate::card_def::mana_color_mask(ManaColor::B)
    } else {
        0
    }
}

/// Tuktuk Rubblefort: "Creatures you control have haste." Called from wave
/// E9a's `statics::granted_keywords`.
pub(crate) fn granted_keywords(state: &GameState, object: ObjectId) -> Keywords {
    static RUBBLEFORT_ID: std::sync::OnceLock<Option<u16>> = std::sync::OnceLock::new();
    let Some(rubblefort_id) = *RUBBLEFORT_ID.get_or_init(|| card_id_by_name(TUKTUK_RUBBLEFORT))
    else {
        return Keywords::NONE;
    };
    let Some(live) = state.objects.try_get(object) else {
        return Keywords::NONE;
    };
    if live.zone != Zone::Battlefield
        || !crate::engine::object_has_type(state, object, CardType::Creature)
    {
        return Keywords::NONE;
    }
    let controller = live.controller;
    let granted = [PlayerId::P0, PlayerId::P1].into_iter().any(|seat| {
        state.players[seat.index()]
            .battlefield
            .iter()
            .copied()
            .any(|permanent| {
                let perm = state.objects.get(permanent);
                perm.card_def == rubblefort_id
                    && perm.controller == controller
                    && active_named(state, permanent, TUKTUK_RUBBLEFORT)
                    && crate::continuous_characteristics_v1::granted_ability_survives(
                        state,
                        object,
                        perm.v4.layer_timestamp.unwrap_or(0),
                    )
            })
    });
    if granted {
        Keywords::HASTE
    } else {
        Keywords::NONE
    }
}

// ---- Execution ----------------------------------------------------------------------

pub(crate) fn execute(op: &Pf2aOpV1, ctx: &ExecCtx, state: &mut GameState) {
    match op {
        Pf2aOpV1::EscapeTunnelUnblockableV1 => {
            if !target_is_legal(ctx, state, ESCAPE_TUNNEL_TARGET, 0) {
                return;
            }
            crate::effect::execute(
                &EffectOp::GrantKeywordTargetUntilEndOfTurn {
                    object: ObjectRef::Target(0),
                    keyword: Keywords::CANT_BE_BLOCKED,
                },
                ctx,
                state,
            );
        }
        Pf2aOpV1::GrangerPingV1 => {
            if !target_is_legal(ctx, state, TargetSpec::AnyTarget, 0) {
                return;
            }
            let Some(&target) = ctx.targets.first() else {
                return;
            };
            let self_target = Target::Player(ctx.controller);
            let batch = if target == self_target {
                vec![event::damage_from_exec_ctx_v1(ctx, self_target, 2)]
            } else {
                vec![
                    event::damage_from_exec_ctx_v1(ctx, target, 1),
                    event::damage_from_exec_ctx_v1(ctx, self_target, 1),
                ]
            };
            event::propose_and_commit_batch(state, batch);
        }
        Pf2aOpV1::ObsidianProtectionV1 => {
            if !target_is_legal(ctx, state, TargetSpec::Creature, 0) {
                return;
            }
            let Some(Target::Object(creature)) = ctx.targets.first().copied() else {
                return;
            };
            let host = live_link(state, creature);
            crate::eternal_legacy_e9a_v1::grant_protections(state, &[host], &[ManaColor::B]);
        }
        Pf2aOpV1::SpiritMascotCounterV1 => {
            let live_source = if let Some(source) = ctx.ability_source_contract {
                state.objects.try_get(source.source).is_some_and(|live| {
                    live.zone == Zone::Battlefield
                        && live.zone_change_count == source.zone_change_count
                })
            } else {
                state
                    .objects
                    .try_get(ctx.source)
                    .is_some_and(|live| live.zone == Zone::Battlefield)
            };
            if !live_source {
                return;
            }
            if event::add_plus_one_counters(state, ctx.source, ctx.controller, 1).is_err() {
                halt(state, ctx.source);
            }
        }
        Pf2aOpV1::RitesDiscardCountV1 { count } => {
            let count = usize::from(*count);
            if count == 0 {
                return;
            }
            let cards = match state.random_hand_cards_v1(ctx.controller, count) {
                Ok(cards) => cards,
                Err(_) => {
                    halt(state, ctx.source);
                    return;
                }
            };
            let discarded = i32::try_from(cards.len()).unwrap_or(i32::MAX);
            crate::engine::commit_discarded_cards(state, &cards);
            if discarded > 0 {
                crate::effect::execute(
                    &EffectOp::BoostControlledCreaturesUntilEndOfTurn {
                        power: discarded,
                        toughness: 0,
                        keywords: Keywords::NONE,
                    },
                    ctx,
                    state,
                );
            }
        }
        Pf2aOpV1::KeepWatchV1
        | Pf2aOpV1::EmbraceLandTappedV1
        | Pf2aOpV1::ThrillingDiscoveryV1
        | Pf2aOpV1::ThrillingDiscardTwoV1
        | Pf2aOpV1::RitesOfInitiationV1 => halt(state, ctx.source),
    }
}

pub(crate) fn drive(
    state: &mut GameState,
    continuation: &mut EffectContinuation,
    op: &Pf2aOpV1,
    path: Vec<u16>,
) -> Result<bool, String> {
    let controller = continuation.ctx.controller;
    match op {
        Pf2aOpV1::KeepWatchV1 => {
            let count = attacking_creature_count(state);
            if count > 0 {
                continuation.frames.push(EffectFrame::Program {
                    op: EffectOp::DrawCards {
                        player: PlayerRef::Controller,
                        count,
                    },
                    path,
                });
            }
            Ok(false)
        }
        Pf2aOpV1::EmbraceLandTappedV1 => {
            let lands = state.players[controller.index()]
                .hand
                .iter()
                .copied()
                .filter(|&card| definition(state, card).is_some_and(|def| def.is_land))
                .map(|card| bind(state, card))
                .collect::<Vec<_>>();
            if lands.is_empty() {
                return Ok(false);
            }
            offer(
                continuation,
                Pf2aChoiceV1::EmbraceLandTappedV1,
                controller,
                lands,
                (0, 1),
                path,
            );
            Ok(true)
        }
        Pf2aOpV1::ThrillingDiscoveryV1 => {
            if state.players[controller.index()].hand.len() < 2 {
                return Ok(false);
            }
            push_choice(
                continuation,
                PlayerRef::Controller,
                vec![pf2a(Pf2aOpV1::ThrillingDiscardTwoV1), nothing()],
                path,
            );
            Ok(false)
        }
        Pf2aOpV1::ThrillingDiscardTwoV1 => {
            let hand = state.players[controller.index()].hand.clone();
            if hand.len() < 2 {
                return Ok(false);
            }
            if hand.len() == 2 {
                crate::engine::commit_discarded_cards(state, &hand);
                continuation.frames.push(EffectFrame::Program {
                    op: EffectOp::DrawCards {
                        player: PlayerRef::Controller,
                        count: 3,
                    },
                    path,
                });
                return Ok(false);
            }
            let bindings = hand.into_iter().map(|card| bind(state, card)).collect();
            offer(
                continuation,
                Pf2aChoiceV1::ThrillingDiscardTwoV1,
                controller,
                bindings,
                (2, 2),
                path,
            );
            Ok(true)
        }
        Pf2aOpV1::RitesOfInitiationV1 => {
            let hand_len = state.players[controller.index()].hand.len();
            if hand_len == 0 {
                return Ok(false);
            }
            let max = u16::try_from(hand_len).unwrap_or(u16::MAX - 1);
            let options = (0..=max)
                .map(|count| pf2a(Pf2aOpV1::RitesDiscardCountV1 { count }))
                .collect();
            push_choice(continuation, PlayerRef::Controller, options, path);
            Ok(false)
        }
        other => {
            execute(other, &continuation.ctx, state);
            Ok(false)
        }
    }
}

pub(crate) fn finish_selection(
    state: &mut GameState,
    continuation: &mut EffectContinuation,
    choice: &E3ChoiceV1,
    kind: &Pf2aChoiceV1,
    bindings: Vec<EffectObjectBinding>,
) -> Result<bool, String> {
    match kind {
        Pf2aChoiceV1::EmbraceLandTappedV1 => {
            if let Some(card) = bindings.first() {
                let mut change = ProposedEvent::zone_change_to_battlefield_under_controller(
                    card.object,
                    choice.player,
                );
                if let ProposedEvent::ZoneChange(ref mut zone_change) = change {
                    zone_change.force_battlefield_tapped = true;
                }
                event::propose_and_commit(state, change);
            }
            Ok(false)
        }
        Pf2aChoiceV1::ThrillingDiscardTwoV1 => {
            if bindings.len() != 2 {
                return Err("Thrilling Discovery requires discarding exactly two cards".to_string());
            }
            let cards: Vec<ObjectId> = bindings.iter().map(|binding| binding.object).collect();
            if cards.iter().any(|&card| {
                let live = state.objects.get(card);
                live.zone != Zone::Hand || live.owner != choice.player
            }) {
                return Err("Thrilling Discovery discard left its owner's hand".to_string());
            }
            crate::engine::commit_discarded_cards(state, &cards);
            continuation.frames.push(EffectFrame::Program {
                op: EffectOp::DrawCards {
                    player: PlayerRef::Controller,
                    count: 3,
                },
                path: choice.path.clone(),
            });
            Ok(false)
        }
    }
}
