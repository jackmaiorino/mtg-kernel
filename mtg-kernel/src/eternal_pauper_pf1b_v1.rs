//! Card programs for the eternal catalog's Pauper field wave Pf1b
//! (`docs/design/eternal_formats_support_v1.md`): spells and library effects.
//!
//! `eternal_cards_v1` forwards this module's effect leaves here, and the
//! recipe tables of `build_eternal_pf1b_v1.rs` name its programs. Every
//! table is keyed by a printed name that occurs only in the eternal registry,
//! so the Pauper, FDN and Standard catalogs never reach it. Behavior was read
//! from each card's current Oracle text and, where present, its XMage source.
//!
//! Where each card lives:
//!
//! - Bitter Reunion: the entry trigger is Pursue the Past's may-discard-then-
//!   draw (two cards); the sacrifice ability is a team haste boost.
//! - Kruphix's Insight and Militia Bugler: own leaves with a typed selection
//!   (`Pf1bChoiceV1`), shaped like Malevolent Rumble and You Meet in a Tavern.
//! - Scattershot: one damage to a creature; Storm is wave E3's table
//!   (`eternal_cards_v1::e3`).
//! - Deprive: the additional cost "return a land you control to its owner's
//!   hand" is `PermanentFilterDef::LandV1` in the cast pipeline.
//! - Exclude: Stern Scolding's counter, then a draw.
//! - Fiery Cannonade: one simultaneous damage batch to each non-Pirate
//!   creature (changelings are Pirates).
//! - Mob Lookout: connive, with the counter on the target creature.
//! - Elite Interceptor and Rejoinder: Carnivorous Cultivator's prepared
//!   machinery (`eternal_vintage_e7b_v1::prepare`); Rejoinder is Sewer-
//!   veillance Cam's tap-or-untap choice plus a draw.
#![cfg_attr(not(feature = "eternal-fixtures"), allow(dead_code))]

use crate::card_def::{CardDef, CardType, Subtype, TargetSpec, CARD_DEFS};
use crate::effect::{
    EffectContinuation, EffectFrame, EffectObjectBinding, EffectOp, ExecCtx, PlayerRef, TargetRef,
};
use crate::eternal_cards_v1::e3::{E3ChoiceKindV1, E3ChoiceV1};
use crate::eternal_cards_v1::EternalOpV1;
use crate::event::{self, ProposedEvent};
use crate::ids::{ObjectId, PlayerId};
use crate::standard_cards_v1::StandardOpV1;
use crate::state::{GameState, ObjectLinkV4, Target, Zone};
use crate::trigger::{TriggerCondition, TriggeredAbilityDef};
use serde::{Deserialize, Serialize};

/// Effect leaves owned by this module, carried as
/// `eternal_cards_v1::EternalOpV1::Pf1bV1`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Pf1bOpV1 {
    /// Fiery Cannonade: `amount` damage to each non-Pirate creature, as one
    /// simultaneous batch.
    DamageEachNonPirateV1 { amount: i32 },
    /// Kruphix's Insight: reveal the top six cards, up to three enchantment
    /// cards from among them to the hand, the rest to the graveyard. A
    /// choice.
    InsightRevealV1,
    /// Militia Bugler: look at the top four cards, may reveal a creature
    /// card with power 2 or less from among them into the hand, the rest on
    /// the bottom in a random order. A choice.
    BuglerLookV1,
    /// Mob Lookout's connive discard: discard a card, then a +1/+1 counter
    /// on target 0 if it was a nonland card. A choice.
    ConniveTargetDiscardV1,
}

impl Pf1bOpV1 {
    /// Whether the leaf may stop for a player's choice (or a draw that a
    /// replacement can interrupt), so it runs in the resumable interpreter.
    pub(crate) fn contains_player_choice(&self) -> bool {
        matches!(
            self,
            Self::InsightRevealV1 | Self::BuglerLookV1 | Self::ConniveTargetDiscardV1
        )
    }
}

/// Resolution choices owned by this module, carried as
/// `E3ChoiceKindV1::Pf1bV1`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Pf1bChoiceV1 {
    /// Kruphix's Insight: up to three of the `revealed` enchantment cards.
    InsightPickV1 { revealed: Vec<EffectObjectBinding> },
    /// Militia Bugler: up to one of the `looked` creature cards.
    BuglerPickV1 { looked: Vec<EffectObjectBinding> },
    /// Mob Lookout's connive: the card to discard from the chooser's hand.
    ConniveDiscardV1,
}

impl Pf1bChoiceV1 {
    /// Whether the candidates are hidden from the other player.
    pub(crate) fn chooser_private(&self) -> bool {
        matches!(self, Self::BuglerPickV1 { .. } | Self::ConniveDiscardV1)
    }

    pub(crate) fn public_purpose(&self) -> crate::rl::TargetSelectionPurposeV4 {
        crate::rl::TargetSelectionPurposeV4::CardSelection
    }

    /// Object incarnations this choice refers to beyond its candidates.
    pub(crate) fn referenced_bindings(&self) -> Vec<EffectObjectBinding> {
        match self {
            Self::InsightPickV1 { revealed } => revealed.clone(),
            Self::BuglerPickV1 { looked } => looked.clone(),
            Self::ConniveDiscardV1 => Vec::new(),
        }
    }
}

// ---- Names and helpers --------------------------------------------------------------

pub(crate) const BITTER_REUNION: &str = "Bitter Reunion";
pub(crate) const MILITIA_BUGLER: &str = "Militia Bugler";
pub(crate) const MOB_LOOKOUT: &str = "Mob Lookout";

fn pf1b(op: Pf1bOpV1) -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::Pf1bV1(op)))
}

fn nothing() -> EffectOp {
    EffectOp::Sequence(Vec::new())
}

fn draw_one() -> EffectOp {
    EffectOp::DrawCards {
        player: PlayerRef::Controller,
        count: 1,
    }
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

fn binding_live(state: &GameState, binding: EffectObjectBinding) -> bool {
    state.objects.try_get(binding.object).is_some_and(|live| {
        live.zone == binding.expected_zone
            && live.zone_change_count == binding.expected_zone_change_count
    })
}

fn definition(state: &GameState, object: ObjectId) -> Option<&'static CardDef> {
    let live = state.objects.try_get(object)?;
    CARD_DEFS.get(live.card_def as usize)
}

/// Target `index` if it still names its announced incarnation on the
/// battlefield.
fn bound_battlefield_target(ctx: &ExecCtx, state: &GameState, index: usize) -> Option<ObjectId> {
    let Some(Target::Object(object)) = ctx.targets.get(index).copied() else {
        return None;
    };
    (ctx.target_incarnation_matches(index, state)
        && state.objects.get(object).zone == Zone::Battlefield)
        .then_some(object)
}

/// Offers one of this module's selections; the frames still to run after it
/// are the continuation's current frames.
fn offer(
    continuation: &mut EffectContinuation,
    kind: Pf1bChoiceV1,
    player: PlayerId,
    bindings: Vec<EffectObjectBinding>,
    (min, max): (u16, u16),
    path: Vec<u16>,
) {
    let remaining = continuation.frames.clone();
    crate::eternal_cards_v1::e3::offer(
        continuation,
        E3ChoiceV1 {
            kind: E3ChoiceKindV1::Pf1bV1(kind),
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

// ---- Spells -------------------------------------------------------------------------

/// Exclude: "Counter target creature spell. Draw a card." The counter is
/// Stern Scolding's (a spell that can't be countered stays on the stack), and
/// the draw follows it as a root instruction, so draw replacements are
/// offered. The draw happens even when the spell can't be countered; a
/// spell whose target left the stack does nothing at all (608.2b).
pub fn exclude() -> EffectOp {
    EffectOp::Sequence(vec![
        crate::eternal_cube_c1a_v1::stern_scolding(),
        draw_one(),
    ])
}

/// Fiery Cannonade: "Fiery Cannonade deals 2 damage to each non-Pirate
/// creature."
pub fn fiery_cannonade() -> EffectOp {
    pf1b(Pf1bOpV1::DamageEachNonPirateV1 { amount: 2 })
}

/// Scattershot: "Scattershot deals 1 damage to target creature." Storm is
/// `eternal_cards_v1::e3`'s.
pub fn scattershot() -> EffectOp {
    EffectOp::DealDamage {
        target: TargetRef::Target(0),
        amount: 1,
    }
}

/// Kruphix's Insight: "Reveal the top six cards of your library. Put up to
/// three enchantment cards from among them into your hand and the rest of
/// the revealed cards into your graveyard."
pub fn kruphixs_insight() -> EffectOp {
    pf1b(Pf1bOpV1::InsightRevealV1)
}

/// Rejoinder, Elite Interceptor's prepare spell: "You may tap or untap
/// target creature. Draw a card." (Sewer-veillance Cam's choice.)
pub fn rejoinder() -> EffectOp {
    let e6a = |op| EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::E6aV1(op)));
    EffectOp::Sequence(vec![
        EffectOp::Choice {
            controller: PlayerRef::Controller,
            options: vec![
                e6a(crate::eternal_pauper_e6a_v1::E6aOpV1::TapTargetV1),
                e6a(crate::eternal_pauper_e6a_v1::E6aOpV1::UntapTargetV1),
                nothing(),
            ],
        },
        draw_one(),
    ])
}

// ---- Triggered abilities ------------------------------------------------------------

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

/// Bitter Reunion: "When this enchantment enters, you may discard a card. If
/// you do, draw two cards." (Ragged Short Spear's trigger.)
fn bitter_reunion_entry() -> EffectOp {
    crate::eternal_vintage_e7b_v1::may_discard_then_draw(2)
}

/// Militia Bugler: "When this creature enters, look at the top four cards of
/// your library. You may reveal a creature card with power 2 or less from
/// among them and put it into your hand. Put the rest on the bottom of your
/// library in a random order."
fn bugler_entry() -> EffectOp {
    pf1b(Pf1bOpV1::BuglerLookV1)
}

/// Mob Lookout: "When this creature enters, target creature you control
/// connives." (Raffine's Informant's connive, bound to the target.)
fn lookout_connive() -> EffectOp {
    EffectOp::Sequence(vec![draw_one(), pf1b(Pf1bOpV1::ConniveTargetDiscardV1)])
}

const BITTER_REUNION_TRIGGERS: [TriggeredAbilityDef; 1] =
    [trigger(TriggerCondition::Etb, bitter_reunion_entry)];
const BUGLER_TRIGGERS: [TriggeredAbilityDef; 1] = [trigger(TriggerCondition::Etb, bugler_entry)];
const LOOKOUT_TRIGGERS: [TriggeredAbilityDef; 1] =
    [trigger(TriggerCondition::Etb, lookout_connive)];

/// Triggered abilities of this module's cards, falling through to wave
/// Pf1c.
pub(crate) fn triggers_for(name: &str) -> &'static [TriggeredAbilityDef] {
    match name {
        BITTER_REUNION => &BITTER_REUNION_TRIGGERS,
        MILITIA_BUGLER => &BUGLER_TRIGGERS,
        MOB_LOOKOUT => &LOOKOUT_TRIGGERS,
        _ => crate::eternal_pauper_pf1c_v1::triggers_for(name),
    }
}

/// Target specs of this module's printed and queued triggered abilities.
pub(crate) fn trigger_target_spec(name: &str, effect: &EffectOp) -> Option<TargetSpec> {
    match name {
        BITTER_REUNION if *effect == bitter_reunion_entry() => Some(TargetSpec::None),
        MILITIA_BUGLER if *effect == bugler_entry() => Some(TargetSpec::None),
        MOB_LOOKOUT if *effect == lookout_connive() => Some(TargetSpec::ControlledCreature),
        _ => delayed_effect_matches(name, effect).then_some(TargetSpec::None),
    }
}

/// Queued triggers owned by this module's cards. None.
pub(crate) fn delayed_effect_matches(_name: &str, _effect: &EffectOp) -> bool {
    false
}

// ---- Execution ----------------------------------------------------------------------

/// Runs one of this module's leaves in synchronous execution.
pub(crate) fn execute(op: &Pf1bOpV1, ctx: &ExecCtx, state: &mut GameState) {
    match op {
        Pf1bOpV1::DamageEachNonPirateV1 { amount } => {
            let creatures = [PlayerId::P0, PlayerId::P1]
                .into_iter()
                .flat_map(|player| state.players[player.index()].battlefield.iter().copied())
                .filter(|&object| {
                    crate::engine::object_has_type(state, object, CardType::Creature)
                        && !crate::engine::has_effective_subtype(state, object, Subtype::Pirate)
                })
                .collect::<Vec<_>>();
            let events = creatures
                .into_iter()
                .map(|object| event::damage_from_exec_ctx_v1(ctx, Target::Object(object), *amount))
                .collect::<Vec<_>>();
            if !events.is_empty() {
                event::propose_and_commit_batch(state, events);
            }
        }
        // Choices run in `drive`; the plain executor never reaches them.
        Pf1bOpV1::InsightRevealV1 | Pf1bOpV1::BuglerLookV1 | Pf1bOpV1::ConniveTargetDiscardV1 => {
            halt(state, ctx.source)
        }
    }
}

/// Runs one of this module's leaves in the resumable interpreter.
pub(crate) fn drive(
    state: &mut GameState,
    continuation: &mut EffectContinuation,
    op: &Pf1bOpV1,
    path: Vec<u16>,
) -> Result<bool, String> {
    let controller = continuation.ctx.controller;
    match op {
        Pf1bOpV1::InsightRevealV1 => {
            let revealed = state.players[controller.index()]
                .library
                .iter()
                .take(6)
                .map(|&card| bind(state, card))
                .collect::<Vec<_>>();
            if revealed.is_empty() {
                return Ok(false);
            }
            // spellbench-public-reveal: public reveal to both players.
            for observer in [PlayerId::P0, PlayerId::P1] {
                state.reveal_library_top(observer, controller, revealed.len());
            }
            let enchantments = revealed
                .iter()
                .copied()
                .filter(|binding| {
                    definition(state, binding.object)
                        .is_some_and(|def| def.has_type(CardType::Enchantment))
                })
                .collect::<Vec<_>>();
            if enchantments.is_empty() {
                finish_insight(state, continuation, path, &revealed, &[]);
                return Ok(false);
            }
            let max = u16::try_from(enchantments.len().min(3)).unwrap_or(3);
            offer(
                continuation,
                Pf1bChoiceV1::InsightPickV1 { revealed },
                controller,
                enchantments,
                (0, max),
                path,
            );
            Ok(true)
        }
        Pf1bOpV1::BuglerLookV1 => {
            let looked = state.players[controller.index()]
                .library
                .iter()
                .take(4)
                .map(|&card| bind(state, card))
                .collect::<Vec<_>>();
            if looked.is_empty() {
                return Ok(false);
            }
            // The controller alone looks at the cards.
            state.reveal_library_top(controller, controller, looked.len());
            let candidates = looked
                .iter()
                .copied()
                .filter(|binding| {
                    definition(state, binding.object)
                        .is_some_and(|def| def.has_type(CardType::Creature))
                        && crate::engine::effective_power(state, binding.object) <= 2
                })
                .collect::<Vec<_>>();
            if candidates.is_empty() {
                finish_bugler(state, controller, &looked, &[])?;
                return Ok(false);
            }
            offer(
                continuation,
                Pf1bChoiceV1::BuglerPickV1 { looked },
                controller,
                candidates,
                (0, 1),
                path,
            );
            Ok(true)
        }
        Pf1bOpV1::ConniveTargetDiscardV1 => {
            let hand = state.players[controller.index()]
                .hand
                .iter()
                .map(|&card| bind(state, card))
                .collect::<Vec<_>>();
            if hand.is_empty() {
                return Ok(false);
            }
            offer(
                continuation,
                Pf1bChoiceV1::ConniveDiscardV1,
                controller,
                hand,
                (1, 1),
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

/// Kruphix's Insight's end: `chosen` to the hand, then the rest of
/// `revealed` still in the library to the graveyard as the engine's
/// graveyard batch frame, which asks the owner for their order when the
/// runtime profile exposes graveyard order (CR 404.3) and orders them
/// automatically where the profile says so. The two moves mirror the reveal
/// partition's two separate batches.
fn finish_insight(
    state: &mut GameState,
    continuation: &mut EffectContinuation,
    path: Vec<u16>,
    revealed: &[EffectObjectBinding],
    chosen: &[EffectObjectBinding],
) {
    let live = revealed
        .iter()
        .copied()
        .filter(|binding| binding_live(state, *binding))
        .collect::<Vec<_>>();
    let to_hand = live
        .iter()
        .filter(|binding| chosen.contains(binding))
        .map(|binding| {
            ProposedEvent::zone_change_preserving_known_identity(binding.object, Zone::Hand)
        })
        .collect::<Vec<_>>();
    if !to_hand.is_empty() {
        event::propose_and_commit_batch(state, to_hand);
    }
    let rest = live
        .into_iter()
        .filter(|binding| !chosen.contains(binding) && binding_live(state, *binding))
        .collect::<Vec<_>>();
    if !rest.is_empty() {
        continuation.frames.push(EffectFrame::MoveObjectsBatch {
            objects: rest,
            to_zone: Zone::Graveyard,
            preserve_known_identity: true,
            order_resolved: false,
            path,
        });
    }
}

/// Militia Bugler's end: `chosen` revealed and put into the hand, the rest
/// of `looked` on the bottom in a random order.
fn finish_bugler(
    state: &mut GameState,
    player: PlayerId,
    looked: &[EffectObjectBinding],
    chosen: &[EffectObjectBinding],
) -> Result<(), String> {
    for card in chosen {
        if !binding_live(state, *card) {
            return Err("a Militia Bugler card left the library".into());
        }
        event::propose_and_commit(state, ProposedEvent::zone_change(card.object, Zone::Hand));
        if state.objects.get(card.object).zone == Zone::Hand {
            // spellbench-public-reveal: hand card revealed by the card rules.
            state.reveal_distinct_hand_card(player.opponent(), player, card.object)?;
        }
    }
    let rest = looked
        .iter()
        .filter(|card| !chosen.contains(card) && binding_live(state, **card))
        .map(|card| ObjectLinkV4 {
            object: card.object,
            zone_change_count: card.expected_zone_change_count,
        })
        .collect::<Vec<_>>();
    if rest.is_empty() {
        return Ok(());
    }
    let objects = rest.iter().map(|card| card.object).collect::<Vec<_>>();
    state.apply_scry_result(player, &rest, &[], &objects)?;
    state
        .randomize_library_bottom_v1(player, objects.len())
        .map_err(|error| error.to_string())
}

/// Applies an answered choice of this module.
pub(crate) fn finish_selection(
    state: &mut GameState,
    continuation: &mut EffectContinuation,
    choice: &E3ChoiceV1,
    kind: &Pf1bChoiceV1,
    bindings: Vec<EffectObjectBinding>,
) -> Result<bool, String> {
    let player = choice.player;
    match kind {
        Pf1bChoiceV1::InsightPickV1 { revealed } => {
            if bindings.len() > 3 || bindings.iter().any(|binding| !revealed.contains(binding)) {
                return Err("Kruphix's Insight chose a card it did not reveal".to_string());
            }
            finish_insight(
                state,
                continuation,
                choice.path.clone(),
                revealed,
                &bindings,
            );
            Ok(false)
        }
        Pf1bChoiceV1::BuglerPickV1 { looked } => {
            if bindings.len() > 1 || bindings.iter().any(|binding| !looked.contains(binding)) {
                return Err("Militia Bugler chose a card it did not look at".to_string());
            }
            finish_bugler(state, player, looked, &bindings)?;
            Ok(false)
        }
        Pf1bChoiceV1::ConniveDiscardV1 => {
            let [card] = bindings.as_slice() else {
                return Err("connive discards exactly one card".to_string());
            };
            let live = state.objects.get(card.object);
            if live.zone != Zone::Hand
                || live.zone_change_count != card.expected_zone_change_count
                || !state.players[player.index()].hand.contains(&card.object)
            {
                return Err("connive chose a card no longer in hand".to_string());
            }
            let nonland =
                definition(state, card.object).is_some_and(|def| !def.has_type(CardType::Land));
            crate::engine::commit_discarded_card(state, card.object);
            if nonland {
                if let Some(creature) = bound_battlefield_target(&continuation.ctx, state, 0) {
                    let _ = event::add_plus_one_counters(state, creature, player, 1);
                }
            }
            Ok(false)
        }
    }
}
