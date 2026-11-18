//! Card programs for the eternal catalog's Vintage Cube wave C1a
//! (`docs/design/eternal_formats_support_v1.md`): white and blue instants and sorceries.
//!
//! `eternal_cards_v1` forwards this module's effect leaves here, and the
//! recipe tables of `build_eternal_c1a_v1.rs` name its programs. Every
//! table is keyed by a printed name that occurs only in the eternal registry,
//! so the Pauper, FDN and Standard catalogs never reach it. Behavior was read
//! from each card's current Oracle text and, where present, its XMage source.
#![cfg_attr(not(feature = "eternal-fixtures"), allow(dead_code))]

pub(crate) mod targets;

use crate::card_def::{CardType, TargetSpec, CARD_DEFS};
use crate::effect::{
    EffectCond, EffectContinuation, EffectFrame, EffectObjectBinding, EffectOp, ExecCtx,
    LibraryCardFilter, LibrarySearchDestinationV1, ObjectRef, PlayerRef,
};
use crate::eternal_cards_v1::e3::{E3ChoiceKindV1, E3ChoiceV1};
use crate::eternal_cards_v1::EternalOpV1;
use crate::event::{self, LibraryPlacement, ProposedEvent};
use crate::ids::{ObjectId, PlayerId, StackItemId};
use crate::standard_cards_v1::StandardOpV1;
use crate::state::{GameState, StackItem, StackItemKind, Target, Zone};
use crate::trigger::TriggeredAbilityDef;
use serde::{Deserialize, Serialize};

/// Effect leaves owned by this module, carried as
/// `eternal_cards_v1::EternalOpV1::C1aV1`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum C1aOpV1 {
    /// Oust: put target 0 second from the top of its owner's library; its
    /// controller gains 3 life.
    OustV1,
    /// Path to Exile: exile target 0, then its controller may search for a
    /// basic land card. A choice.
    PathToExileV1,
    /// Damn's overload: destroy each creature; they can't be regenerated.
    DamnOverloadV1,
    /// Winds of Abandon: exile target 0, then its controller searches for a
    /// basic land card. A choice.
    WindsOfAbandonV1,
    /// Winds of Abandon's overload: exile each creature you don't control,
    /// then each of their controllers searches for that many basic land
    /// cards. A choice.
    WindsOfAbandonOverloadV1,
    /// Unexpectedly Absent: put target 0 into its owner's library just
    /// beneath the top X cards.
    UnexpectedlyAbsentV1,
    /// Occult Epiphany: draw X cards, then the discard stage.
    OccultEpiphanyV1,
    /// Occult Epiphany's second half: discard `count` cards, then create a
    /// Spirit for each card type among them. A choice.
    OccultDiscardV1 { count: u16 },
    /// Time Spiral: exile it as it finishes resolving, and each player
    /// shuffles their hand and graveyard into their library. The draws and
    /// the untap are the program's next instructions.
    TimeSpiralV1,
    /// Between the draws of "each player draws": if a draw just won the game
    /// (Jace, Wielder of Mysteries), the rest of the program does not happen.
    StopIfWonV1,
    /// One chosen counter (kind 0) or bounce (kind 1) of Mystic Confluence
    /// and the index of its target.
    MysticStepV1 { kind: u8, slot: u8 },
    /// "Counter target spell unless its controller pays {3}": the payment.
    MysticPayV1 { spell: StackItemId },
    /// "Counter target spell unless its controller pays {3}": the counter.
    MysticCounterV1 { spell: StackItemId },
}

impl C1aOpV1 {
    /// Whether the leaf may stop for a player's choice (or a draw that a
    /// replacement can interrupt), so it runs in the resumable interpreter.
    pub(crate) fn contains_player_choice(&self) -> bool {
        matches!(
            self,
            Self::PathToExileV1
                | Self::WindsOfAbandonV1
                | Self::WindsOfAbandonOverloadV1
                | Self::OccultEpiphanyV1
                | Self::OccultDiscardV1 { .. }
                | Self::TimeSpiralV1
                | Self::StopIfWonV1
                | Self::MysticStepV1 { .. }
        )
    }
}

/// A choice of this module, carried as `E3ChoiceKindV1::C1aV1`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum C1aChoiceV1 {
    /// Occult Epiphany: the cards its controller discards.
    OccultDiscardV1,
}

impl C1aChoiceV1 {
    /// Whether the candidates are hidden from the other player.
    pub(crate) fn chooser_private(&self) -> bool {
        true
    }

    pub(crate) fn public_purpose(&self) -> crate::rl::TargetSelectionPurposeV4 {
        crate::rl::TargetSelectionPurposeV4::CardSelection
    }

    /// Object incarnations this choice refers to beyond its candidates.
    pub(crate) fn referenced_bindings(&self) -> Vec<EffectObjectBinding> {
        Vec::new()
    }
}

fn c1a(op: C1aOpV1) -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::C1aV1(op)))
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

// ---- Programs -----------------------------------------------------------------------

/// "Counter target spell. If that spell is countered this way, put it
/// `to_zone` instead of into its owner's graveyard." A spell that can't be
/// countered stays on the stack; a copy ceases to exist; a flashback spell
/// is exiled (the same departure Dissipate and Force of Negation use).
fn counter_instead_to(to_zone: Zone) -> EffectOp {
    EffectOp::Conditional {
        cond: EffectCond::TargetSpellCanBeCountered(0),
        then: Box::new(EffectOp::MoveObject {
            object: ObjectRef::Target(0),
            to_zone,
        }),
        else_: Box::new(nothing()),
    }
}

/// Oust: "Put target creature into its owner's library second from the top.
/// Its controller gains 3 life."
pub fn oust() -> EffectOp {
    c1a(C1aOpV1::OustV1)
}

/// Path to Exile: "Exile target creature. Its controller may search their
/// library for a basic land card, put that card onto the battlefield tapped,
/// then shuffle."
pub fn path_to_exile() -> EffectOp {
    c1a(C1aOpV1::PathToExileV1)
}

/// Reprieve: "Return target spell to its owner's hand. Draw a card." It is
/// not a counter: a spell that can't be countered returns too, and a copy
/// ceases to exist.
pub fn reprieve() -> EffectOp {
    EffectOp::Sequence(vec![
        EffectOp::Conditional {
            cond: EffectCond::TargetInZone(0, Zone::Stack),
            then: Box::new(EffectOp::MoveObject {
                object: ObjectRef::Target(0),
                to_zone: Zone::Hand,
            }),
            else_: Box::new(nothing()),
        },
        draw_one(),
    ])
}

/// Memory Lapse: "Counter target spell. If that spell is countered this way,
/// put it on top of its owner's library instead of into that player's
/// graveyard."
pub fn memory_lapse() -> EffectOp {
    counter_instead_to(Zone::Library)
}

/// Remand: "Counter target spell. If that spell is countered this way, put it
/// into its owner's hand instead of into that player's graveyard. Draw a
/// card." The draw happens even when the spell can't be countered.
pub fn remand() -> EffectOp {
    EffectOp::Sequence(vec![counter_instead_to(Zone::Hand), draw_one()])
}

/// Stern Scolding: "Counter target creature spell with power or toughness 2
/// or less."
pub fn stern_scolding() -> EffectOp {
    counter_instead_to(Zone::Graveyard)
}

/// Damn's overload {2}{W}{W}: "Destroy each creature. A creature destroyed
/// this way can't be regenerated." (The unoverloaded spell is Avatar of
/// Woe's `destroy_target_creature_no_regeneration`.)
pub fn damn_overload() -> EffectOp {
    c1a(C1aOpV1::DamnOverloadV1)
}

/// Winds of Abandon: "Exile target creature you don't control. For each
/// creature exiled this way, its controller searches their library for a
/// basic land card. Those players put those cards onto the battlefield
/// tapped, then shuffle."
pub fn winds_of_abandon() -> EffectOp {
    c1a(C1aOpV1::WindsOfAbandonV1)
}

/// Winds of Abandon's overload {4}{W}{W}: "Exile each creature you don't
/// control. For each creature exiled this way, its controller searches their
/// library for a basic land card. Those players put those cards onto the
/// battlefield tapped, then shuffle."
pub fn winds_of_abandon_overload() -> EffectOp {
    c1a(C1aOpV1::WindsOfAbandonOverloadV1)
}

/// Unexpectedly Absent: "Put target nonland permanent into its owner's
/// library just beneath the top X cards of that library."
pub fn unexpectedly_absent() -> EffectOp {
    c1a(C1aOpV1::UnexpectedlyAbsentV1)
}

/// Occult Epiphany: "Draw X cards, then discard X cards. Create a 1/1 white
/// Spirit creature token with flying for each card type among cards
/// discarded this way."
pub fn occult_epiphany() -> EffectOp {
    c1a(C1aOpV1::OccultEpiphanyV1)
}

/// Time Spiral: "Exile Time Spiral. Each player shuffles their hand and
/// graveyard into their library, then draws seven cards. You untap up to
/// six lands."
pub fn time_spiral() -> EffectOp {
    EffectOp::Sequence(vec![
        c1a(C1aOpV1::TimeSpiralV1),
        EffectOp::DrawCards {
            player: PlayerRef::Controller,
            count: 7,
        },
        stop_if_won(),
        EffectOp::DrawCards {
            player: PlayerRef::Opponent,
            count: 7,
        },
        stop_if_won(),
        EffectOp::UntapUpToLands {
            chooser: PlayerRef::Controller,
            max_targets: 6,
        },
    ])
}

/// An instruction between the draws of two players: a draw that won the game
/// ends it at once, so the instructions after it are dropped (104.1). Wheel
/// of Fortune (wave C1c) uses it too.
pub(crate) fn stop_if_won() -> EffectOp {
    c1a(C1aOpV1::StopIfWonV1)
}

/// Mystic Confluence's three chosen modes (0 counter, 1 bounce, 2 draw), in
/// printed order. Counters take the first targets and bounces the next
/// ones. Each draw is its own instruction of the program, so a draw
/// replacement can interrupt it.
fn mystic(modes: [u8; 3]) -> EffectOp {
    let mut counter_slot = 0_u8;
    let counters = modes.iter().filter(|&&mode| mode == 0).count();
    let mut bounce_slot = u8::try_from(counters).unwrap_or(u8::MAX);
    EffectOp::Sequence(
        modes
            .iter()
            .map(|&kind| match kind {
                0 => {
                    counter_slot += 1;
                    c1a(C1aOpV1::MysticStepV1 {
                        kind,
                        slot: counter_slot - 1,
                    })
                }
                1 => {
                    bounce_slot += 1;
                    c1a(C1aOpV1::MysticStepV1 {
                        kind,
                        slot: bounce_slot - 1,
                    })
                }
                _ => draw_one(),
            })
            .collect(),
    )
}

/// Mystic Confluence's ten modes, one per multiset of its three printed
/// modes: counter (0), bounce (1), draw (2).
macro_rules! mystic_confluence {
    ($($name:ident => [$a:literal, $b:literal, $c:literal]),* $(,)?) => {
        $(
            /// Mystic Confluence's mode for this multiset.
            pub fn $name() -> EffectOp {
                mystic([$a, $b, $c])
            }
        )*
    };
}

mystic_confluence! {
    mystic_confluence_0 => [0, 0, 0],
    mystic_confluence_1 => [0, 0, 1],
    mystic_confluence_2 => [0, 0, 2],
    mystic_confluence_3 => [0, 1, 1],
    mystic_confluence_4 => [0, 1, 2],
    mystic_confluence_5 => [0, 2, 2],
    mystic_confluence_6 => [1, 1, 1],
    mystic_confluence_7 => [1, 1, 2],
    mystic_confluence_8 => [1, 2, 2],
    mystic_confluence_9 => [2, 2, 2],
}

// ---- Target filters -----------------------------------------------------------------

/// Whether the spell `item` on the stack is a creature spell with power or
/// toughness 2 or less (Stern Scolding). The spell's characteristics are
/// its printed ones plus whatever the engine's effective statistics add.
pub(crate) fn creature_spell_power_or_toughness_at_most_two(
    state: &GameState,
    item: &StackItem,
) -> bool {
    crate::engine::stack_spell_has_type(state, item, CardType::Creature)
        && (crate::engine::effective_power(state, item.source) <= 2
            || crate::engine::effective_toughness(state, item.source) <= 2)
}

// ---- Helpers ------------------------------------------------------------------------

/// Target `index` when it is still the object the spell chose, on the
/// battlefield, and still a legal target of `spec` (608.2b).
fn legal_target(
    ctx: &ExecCtx,
    state: &GameState,
    spec: TargetSpec,
    index: usize,
) -> Option<ObjectId> {
    let Target::Object(object) = ctx.targets.get(index).copied()? else {
        return None;
    };
    (ctx.target_incarnation_matches(index, state)
        && state.objects.get(object).zone == Zone::Battlefield
        && crate::engine::effect_target_is_legal(
            state,
            ctx.source,
            ctx.controller,
            spec,
            &ctx.targets[index..=index],
            0,
        ))
    .then_some(object)
}

/// The reference to `player` from the resolving spell's controller.
fn player_ref(ctx: &ExecCtx, player: PlayerId) -> PlayerRef {
    if player == ctx.controller {
        PlayerRef::Controller
    } else {
        PlayerRef::Opponent
    }
}

/// Whether `object`, just moved by an exile, was exiled (a token that
/// ceased to exist was exiled too).
fn was_exiled(state: &GameState, object: ObjectId) -> bool {
    state.objects.try_get(object).is_some_and(|live| {
        live.zone == Zone::Exile || (live.v4.is_token && live.zone != Zone::Battlefield)
    })
}

fn push(continuation: &mut EffectContinuation, op: EffectOp, path: &[u16]) {
    continuation.frames.push(EffectFrame::Program {
        op,
        path: path.to_vec(),
    });
}

/// The frame of `player` searching for up to `count` basic land cards, which
/// enter tapped, then shuffling.
fn basic_land_search(player: PlayerRef, count: u16) -> EffectOp {
    EffectOp::SearchLibraryCardsToDestination {
        player,
        filter: LibraryCardFilter::BasicLand,
        max_targets: count,
        destination: LibrarySearchDestinationV1::Battlefield { tapped: true },
    }
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

/// Simultaneously destroys every non-indestructible creature on the
/// battlefield without regeneration (as wave P3h's Decree of Pain does).
/// Creature tokens and creatures moved to exile by a zone-change replacement
/// effect were still destroyed; indestructible creatures do not die.
fn destroy_all_creatures_no_regeneration(state: &mut GameState) {
    let creatures = [PlayerId::P0, PlayerId::P1]
        .into_iter()
        .flat_map(|player| state.players[player.index()].battlefield.iter().copied())
        .filter(|&object| crate::engine::object_has_type(state, object, CardType::Creature))
        .filter(|&object| {
            !crate::engine::has_effective_keyword(
                state,
                object,
                crate::card_def::Keywords::INDESTRUCTIBLE,
            )
        })
        .collect::<Vec<_>>();
    if creatures.is_empty() {
        return;
    }
    event::propose_and_commit_batch(
        state,
        creatures
            .into_iter()
            .map(|creature| ProposedEvent::zone_change(creature, Zone::Graveyard))
            .collect(),
    );
}

/// Whether the spell `object` can't be countered.
fn cannot_be_countered(state: &GameState, object: ObjectId) -> bool {
    CARD_DEFS[state.objects.get(object).card_def as usize].spell_cannot_be_countered
        || (cfg!(feature = "eternal-fixtures")
            && crate::eternal_cards_v1::e3::spell_cannot_be_countered(state, object))
}

/// The cost of Mystic Confluence's tax.
const THREE: crate::mana::Cost = crate::mana::Cost {
    pips: &[],
    generic: 3,
    x_count: 0,
};

/// The stack item `spell` names, if it is still a spell on the stack.
fn stack_spell(state: &GameState, spell: StackItemId) -> Option<&StackItem> {
    state
        .stack
        .iter()
        .find(|item| item.v4.stack_item_id == spell && item.kind == StackItemKind::Spell)
}

/// Offers `player` the choice of `kind` among `bindings`, `min` to `max`
/// cards.
fn offer(
    continuation: &mut EffectContinuation,
    kind: C1aChoiceV1,
    player: PlayerId,
    bindings: Vec<EffectObjectBinding>,
    min: u16,
    max: u16,
    path: Vec<u16>,
) {
    let remaining = continuation.frames.clone();
    crate::eternal_cards_v1::e3::offer(
        continuation,
        E3ChoiceV1 {
            kind: E3ChoiceKindV1::C1aV1(kind),
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

/// Occult Epiphany: the cards in `cards` are discarded together, and the
/// controller creates a 1/1 white Spirit with flying for each card type among
/// them (an artifact creature has two).
fn discard_and_make_spirits(
    ctx: &ExecCtx,
    state: &mut GameState,
    cards: &[EffectObjectBinding],
) -> Result<(), String> {
    let cards = cards
        .iter()
        .copied()
        .filter(|&card| binding_live(state, card))
        .map(|card| card.object)
        .collect::<Vec<_>>();
    let mut types: Vec<CardType> = Vec::new();
    for &card in &cards {
        for &card_type in CARD_DEFS[state.objects.get(card).card_def as usize].types {
            if !types.contains(&card_type) {
                types.push(card_type);
            }
        }
    }
    crate::engine::commit_discarded_cards(state, &cards);
    if state.engine.halted.is_some() || types.is_empty() {
        return Ok(());
    }
    let token = crate::card_def::card_id_by_name("White Flying Spirit Token")
        .ok_or("the White Flying Spirit Token row is missing")?;
    for _ in 0..types.len() {
        crate::effect::execute(
            &EffectOp::CreateToken {
                token_def: token,
                controller: PlayerRef::Controller,
            },
            ctx,
            state,
        );
    }
    Ok(())
}

// ---- Triggered abilities ------------------------------------------------------------

/// Triggered abilities of this module's cards, falling through to wave
/// C1b.
pub(crate) fn triggers_for(name: &str) -> &'static [TriggeredAbilityDef] {
    crate::eternal_cube_c1b_v1::triggers_for(name)
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
pub(crate) fn execute(op: &C1aOpV1, ctx: &ExecCtx, state: &mut GameState) {
    match op {
        C1aOpV1::OustV1 => {
            let Some(object) = legal_target(ctx, state, TargetSpec::Creature, 0) else {
                return;
            };
            let controller = state.objects.get(object).controller;
            event::propose_and_commit(
                state,
                ProposedEvent::public_library_insert(object, LibraryPlacement::SecondFromTop),
            );
            event::propose_and_commit(state, ProposedEvent::life_gain(controller, 3));
        }
        C1aOpV1::DamnOverloadV1 => destroy_all_creatures_no_regeneration(state),
        C1aOpV1::UnexpectedlyAbsentV1 => {
            let Some(object) = legal_target(ctx, state, TargetSpec::NonlandPermanent, 0) else {
                return;
            };
            event::propose_and_commit(
                state,
                ProposedEvent::public_library_insert(
                    object,
                    LibraryPlacement::BeneathTop(ctx.x_value),
                ),
            );
        }
        // "Counter target spell unless its controller pays {3}": the payment
        // is made, or, when it can't be, the spell is countered.
        C1aOpV1::MysticPayV1 { spell } => {
            let Some(payer) = stack_spell(state, *spell).map(|item| item.controller) else {
                return;
            };
            match crate::mana::can_pay(&THREE, 0, payer, state) {
                Some(plan) => {
                    crate::engine::pay_plan(state, payer, &plan);
                }
                None => {
                    if crate::engine::counter_stack_item_by_id(state, *spell).is_err() {
                        halt(state, ctx.source);
                    }
                }
            }
        }
        C1aOpV1::MysticCounterV1 { spell } => {
            if crate::engine::counter_stack_item_by_id(state, *spell).is_err() {
                halt(state, ctx.source);
            }
        }
        // Choices and frame-pushing leaves run in `drive`.
        C1aOpV1::PathToExileV1
        | C1aOpV1::WindsOfAbandonV1
        | C1aOpV1::WindsOfAbandonOverloadV1
        | C1aOpV1::OccultEpiphanyV1
        | C1aOpV1::OccultDiscardV1 { .. }
        | C1aOpV1::TimeSpiralV1
        | C1aOpV1::StopIfWonV1
        | C1aOpV1::MysticStepV1 { .. } => halt(state, ctx.source),
    }
}

/// Runs one of this module's leaves in the resumable interpreter.
pub(crate) fn drive(
    state: &mut GameState,
    continuation: &mut EffectContinuation,
    op: &C1aOpV1,
    path: Vec<u16>,
) -> Result<bool, String> {
    match op {
        C1aOpV1::PathToExileV1 => {
            let Some(object) = legal_target(&continuation.ctx, state, TargetSpec::Creature, 0)
            else {
                return Ok(false);
            };
            // The creature's controller may search. A zone change resets
            // the controller, so name the searcher before exiling.
            let searcher = player_ref(&continuation.ctx, state.objects.get(object).controller);
            event::propose_and_commit(state, ProposedEvent::zone_change(object, Zone::Exile));
            push(
                continuation,
                crate::eternal_cards_v1::may_search(searcher, basic_land_search(searcher, 1)),
                &path,
            );
            Ok(false)
        }
        C1aOpV1::WindsOfAbandonV1 => {
            let Some(object) = legal_target(
                &continuation.ctx,
                state,
                TargetSpec::OpponentControlledCreature,
                0,
            ) else {
                return Ok(false);
            };
            let searcher = player_ref(&continuation.ctx, state.objects.get(object).controller);
            event::propose_and_commit(state, ProposedEvent::zone_change(object, Zone::Exile));
            // "For each creature exiled this way": nothing is found when it
            // was not exiled.
            if was_exiled(state, object) {
                push(continuation, basic_land_search(searcher, 1), &path);
            }
            Ok(false)
        }
        C1aOpV1::WindsOfAbandonOverloadV1 => {
            let controller = continuation.ctx.controller;
            let creatures = [PlayerId::P0, PlayerId::P1]
                .into_iter()
                .filter(|&player| player != controller)
                .flat_map(|player| state.players[player.index()].battlefield.iter().copied())
                .filter(|&object| crate::engine::object_has_type(state, object, CardType::Creature))
                .map(|object| (object, state.objects.get(object).controller))
                .collect::<Vec<_>>();
            if creatures.is_empty() {
                return Ok(false);
            }
            event::propose_and_commit_batch(
                state,
                creatures
                    .iter()
                    .map(|&(object, _)| ProposedEvent::zone_change(object, Zone::Exile))
                    .collect(),
            );
            if state.engine.halted.is_some() {
                return Ok(false);
            }
            // Each of their controllers searches for as many basic land cards
            // as creatures of theirs were exiled, the active player first.
            let mut counts = [0_u16; 2];
            for &(object, owner) in &creatures {
                if was_exiled(state, object) {
                    counts[owner.index()] += 1;
                }
            }
            let first = state.active_player;
            for player in [first.opponent(), first] {
                let count = counts[player.index()];
                if count > 0 {
                    let searcher = player_ref(&continuation.ctx, player);
                    push(continuation, basic_land_search(searcher, count), &path);
                }
            }
            Ok(false)
        }
        C1aOpV1::OccultEpiphanyV1 => {
            let count = continuation.ctx.x_value;
            if count > 0 {
                push(continuation, c1a(C1aOpV1::OccultDiscardV1 { count }), &path);
                push(
                    continuation,
                    EffectOp::DrawCards {
                        player: PlayerRef::Controller,
                        count: u32::from(count),
                    },
                    &path,
                );
            }
            Ok(false)
        }
        C1aOpV1::OccultDiscardV1 { count } => {
            let controller = continuation.ctx.controller;
            let hand = state.players[controller.index()]
                .hand
                .iter()
                .map(|&card| bind(state, card))
                .collect::<Vec<_>>();
            let need = hand.len().min(usize::from(*count));
            if need == 0 {
                return Ok(false);
            }
            if need == hand.len() {
                // The whole hand goes: nothing to choose.
                let ctx = continuation.ctx.clone();
                discard_and_make_spirits(&ctx, state, &hand)?;
                return Ok(false);
            }
            let need = u16::try_from(need).unwrap_or(u16::MAX);
            offer(
                continuation,
                C1aChoiceV1::OccultDiscardV1,
                controller,
                hand,
                need,
                need,
                path,
            );
            Ok(true)
        }
        C1aOpV1::TimeSpiralV1 => {
            // "Exile Time Spiral": it leaves the stack for exile as it
            // finishes resolving, as Burning Wish does. Then each player
            // shuffles their hand and graveyard into their library; the
            // draws and the untap are the program's next instructions.
            crate::eternal_legacy_e7a_v1::mark_self_exile(&continuation.ctx, state);
            crate::eternal_legacy_e7a_v1::shuffle_echo_libraries(state)?;
            Ok(false)
        }
        C1aOpV1::StopIfWonV1 => {
            if [PlayerId::P0, PlayerId::P1]
                .into_iter()
                .any(|player| crate::eternal_cards_v1::owned_draws_v1::draw_won_game(state, player))
            {
                continuation.frames.clear();
            }
            Ok(false)
        }
        C1aOpV1::MysticStepV1 { kind, slot } => {
            let slot = usize::from(*slot);
            match kind {
                // "Counter target spell unless its controller pays {3}."
                0 => {
                    let ctx = continuation.ctx.clone();
                    let Some(Target::Object(spell)) = ctx.targets.get(slot).copied() else {
                        return Ok(false);
                    };
                    if !ctx.target_incarnation_matches(slot, state) {
                        return Ok(false);
                    }
                    let Some(item) = state
                        .stack
                        .iter()
                        .find(|item| item.source == spell && item.kind == StackItemKind::Spell)
                        .cloned()
                    else {
                        return Ok(false);
                    };
                    if cannot_be_countered(state, spell) {
                        return Ok(false);
                    }
                    let id = item.v4.stack_item_id;
                    if crate::mana::can_pay(&THREE, 0, item.controller, state).is_none() {
                        if crate::engine::counter_stack_item_by_id(state, id).is_err() {
                            return Err("Mystic Confluence could not counter its target".into());
                        }
                        return Ok(false);
                    }
                    push(
                        continuation,
                        EffectOp::Choice {
                            controller: player_ref(&ctx, item.controller),
                            options: vec![
                                c1a(C1aOpV1::MysticPayV1 { spell: id }),
                                c1a(C1aOpV1::MysticCounterV1 { spell: id }),
                            ],
                        },
                        &path,
                    );
                    Ok(false)
                }
                // "Return target creature to its owner's hand."
                1 => {
                    if let Some(creature) =
                        legal_target(&continuation.ctx, state, TargetSpec::Creature, slot)
                    {
                        event::propose_and_commit(
                            state,
                            ProposedEvent::zone_change(creature, Zone::Hand),
                        );
                    }
                    Ok(false)
                }
                // "Draw a card" is an instruction of the program itself.
                _ => Ok(false),
            }
        }
        other => {
            execute(other, &continuation.ctx, state);
            Ok(false)
        }
    }
}

/// Applies an answered choice of this module.
pub(crate) fn finish_selection(
    state: &mut GameState,
    continuation: &mut EffectContinuation,
    _choice: &E3ChoiceV1,
    kind: &C1aChoiceV1,
    bindings: Vec<EffectObjectBinding>,
) -> Result<bool, String> {
    match kind {
        C1aChoiceV1::OccultDiscardV1 => {
            let ctx = continuation.ctx.clone();
            discard_and_make_spirits(&ctx, state, &bindings)?;
            Ok(false)
        }
    }
}
