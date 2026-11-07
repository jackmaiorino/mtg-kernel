//! The outside-the-game zone of the eternal catalog (stage S3 in
//! `docs/design/eternal_formats_support_v1.md`): each player's cards outside
//! the game, private to their owner, and the wish effects that read them.
//!
//! A best-of-three game's outside-the-game cards are its sideboard without a
//! companion in use (`eternal_bo3_v1`); a custom-deck session receives them
//! at reset. The list lives in module state, never in a zone list, so no
//! public projection or opponent observation sees it. A card leaves it only
//! when an effect reveals it and puts it into its owner's hand; the new hand
//! card is then known to the opponent. Best-of-one games have an empty list,
//! so every wish there finds nothing, as before.
//!
//! Choices among outside-the-game cards are effect options in card id order
//! (the decline option first where the effect is optional). Policy
//! observation v5 describes an effect option only by its index, so a policy
//! reads the option order from its own sideboard.
#![cfg_attr(not(feature = "eternal-fixtures"), allow(dead_code))]

use crate::card_def::{CardType, Subtype, CARD_DEFS};
use crate::effect::{EffectContinuation, EffectFrame, EffectOp, ExecCtx, PlayerRef};
use crate::eternal_cards_v1::EternalOpV1;
use crate::ids::{ObjectId, PlayerId};
use crate::standard_cards_v1::StandardOpV1;
use crate::state::{GameObject, GameState, ObjectStateV4, Zone};
use serde::{Deserialize, Serialize};

/// Each player's cards outside the game, sorted by card id. Absent from the
/// eternal state until a game has any.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OutsideGameStateV1 {
    #[serde(default)]
    pub cards: [Vec<u16>; 2],
}

impl OutsideGameStateV1 {
    pub(crate) fn is_empty(&self) -> bool {
        self.cards.iter().all(Vec::is_empty)
    }
}

/// Gives each player their cards outside the game before the game begins.
/// Empty lists leave the state untouched, so best-of-one games keep their
/// snapshots and hashes.
pub fn set_outside_the_game(state: &mut GameState, cards: [Vec<u16>; 2]) {
    if cards.iter().all(Vec::is_empty) {
        return;
    }
    let outside = &mut state
        .eternal_v1
        .get_or_insert_with(Default::default)
        .outside_game;
    for (player, mut list) in cards.into_iter().enumerate() {
        list.sort_unstable();
        outside.cards[player] = list;
    }
}

/// `player`'s cards outside the game, sorted by card id.
pub fn outside_the_game(state: &GameState, player: PlayerId) -> &[u16] {
    state
        .eternal_v1
        .as_ref()
        .map_or(&[], |eternal| &eternal.outside_game.cards[player.index()])
}

/// This module's effect leaves.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OutsideGameOpV1 {
    /// Burning Wish: "You may reveal a sorcery card you own from outside the
    /// game and put it into your hand." A choice.
    WishSorceryV1,
    /// Karn, the Great Creator's -2: "You may reveal an artifact card you
    /// own from outside the game or choose a face-up artifact card you own
    /// in exile. Put that card into your hand." A choice.
    KarnWishV1,
    /// Learn (701.48a): "You may reveal a Lesson card you own from outside
    /// the game and put it into your hand, or discard a card to draw a
    /// card." A choice.
    LearnV1,
    /// Extrapolate the Impossible: "You may reveal exactly two cards you own
    /// with different names from outside the game. An opponent chooses one
    /// of them. You put that card into your hand." The first card. A choice.
    ExtrapolateV1,
    /// Extrapolate the Impossible's second card, after `first`. A choice.
    ExtrapolateSecondV1 { first: u16 },
    /// Extrapolate the Impossible: `cards` are revealed and an opponent
    /// chooses the one its controller puts into their hand. A choice.
    ExtrapolateRevealedV1 { cards: [u16; 2] },
    /// Reveal one `card` from the controller's cards outside the game and
    /// put it into their hand.
    PutIntoHandV1 { card: u16 },
}

impl OutsideGameOpV1 {
    /// Whether this leaf can yield a player decision, so its program must use
    /// the resumable interpreter.
    pub(crate) fn contains_player_choice(&self) -> bool {
        !matches!(self, Self::PutIntoHandV1 { .. })
    }
}

fn outside(op: OutsideGameOpV1) -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::OutsideGameV1(op)))
}

/// Burning Wish's reveal, before it exiles itself.
pub fn wish_sorcery() -> EffectOp {
    outside(OutsideGameOpV1::WishSorceryV1)
}

/// Karn, the Great Creator's -2.
pub fn karn_wish() -> EffectOp {
    outside(OutsideGameOpV1::KarnWishV1)
}

/// Learn.
pub fn learn() -> EffectOp {
    outside(OutsideGameOpV1::LearnV1)
}

/// Extrapolate the Impossible.
pub fn extrapolate() -> EffectOp {
    outside(OutsideGameOpV1::ExtrapolateV1)
}

/// The distinct cards among `player`'s cards outside the game that `keep`
/// accepts, in card id order.
fn eligible(state: &GameState, player: PlayerId, keep: impl Fn(u16) -> bool) -> Vec<u16> {
    let mut cards: Vec<u16> = outside_the_game(state, player)
        .iter()
        .copied()
        .filter(|&card| keep(card))
        .collect();
    cards.dedup();
    cards
}

fn has_type(card: u16, card_type: CardType) -> bool {
    CARD_DEFS[usize::from(card)].has_type(card_type)
}

fn is_lesson(card: u16) -> bool {
    CARD_DEFS[usize::from(card)]
        .subtypes
        .contains(&Subtype::Lesson)
}

fn decline() -> EffectOp {
    EffectOp::Sequence(Vec::new())
}

fn put_options(cards: &[u16]) -> Vec<EffectOp> {
    cards
        .iter()
        .map(|&card| outside(OutsideGameOpV1::PutIntoHandV1 { card }))
        .collect()
}

fn push(continuation: &mut EffectContinuation, op: EffectOp, path: &[u16]) {
    continuation.frames.push(EffectFrame::Program {
        op,
        path: path.to_vec(),
    });
}

/// Pushes a choice among `options` for `chooser`; a single option runs
/// without a decision.
fn push_options(
    continuation: &mut EffectContinuation,
    chooser: PlayerRef,
    mut options: Vec<EffectOp>,
    path: &[u16],
) {
    match options.len() {
        0 => {}
        1 => push(continuation, options.remove(0), path),
        _ => push(
            continuation,
            EffectOp::Choice {
                controller: chooser,
                options,
            },
            path,
        ),
    }
}

/// Runs a leaf inside the resumable interpreter. Every leaf here only pushes
/// frames (a choice or the effect it settles on), so none suspends itself.
pub(crate) fn drive(
    state: &mut GameState,
    continuation: &mut EffectContinuation,
    op: &OutsideGameOpV1,
    path: Vec<u16>,
) -> Result<bool, String> {
    let controller = continuation.ctx.controller;
    match op {
        OutsideGameOpV1::WishSorceryV1 => {
            let cards = eligible(state, controller, |card| has_type(card, CardType::Sorcery));
            if !cards.is_empty() {
                let mut options = vec![decline()];
                options.extend(put_options(&cards));
                push_options(continuation, PlayerRef::Controller, options, &path);
            }
        }
        OutsideGameOpV1::KarnWishV1 => {
            // The exile choice may itself choose nothing, so it is also the
            // way to decline.
            let mut options = vec![crate::eternal_vintage_v1::karn_exile_selection()];
            let cards = eligible(state, controller, |card| has_type(card, CardType::Artifact));
            options.extend(put_options(&cards));
            push_options(continuation, PlayerRef::Controller, options, &path);
        }
        OutsideGameOpV1::LearnV1 => {
            let mut options = vec![decline()];
            if !state.players[controller.index()].hand.is_empty() {
                options.push(EffectOp::Sequence(vec![
                    EffectOp::DiscardCards {
                        player: PlayerRef::Controller,
                        count: 1,
                    },
                    EffectOp::DrawCards {
                        player: PlayerRef::Controller,
                        count: 1,
                    },
                ]));
            }
            let lessons = eligible(state, controller, is_lesson);
            options.extend(put_options(&lessons));
            push_options(continuation, PlayerRef::Controller, options, &path);
        }
        OutsideGameOpV1::ExtrapolateV1 => {
            let cards = eligible(state, controller, |_| true);
            if cards.len() >= 2 {
                let mut options = vec![decline()];
                options.extend(
                    cards
                        .iter()
                        .map(|&first| outside(OutsideGameOpV1::ExtrapolateSecondV1 { first })),
                );
                push_options(continuation, PlayerRef::Controller, options, &path);
            }
        }
        OutsideGameOpV1::ExtrapolateSecondV1 { first } => {
            let options = eligible(state, controller, |card| card != *first)
                .into_iter()
                .map(|second| {
                    // The opponent sees the two cards in card id order.
                    let mut cards = [*first, second];
                    cards.sort_unstable();
                    outside(OutsideGameOpV1::ExtrapolateRevealedV1 { cards })
                })
                .collect();
            push_options(continuation, PlayerRef::Controller, options, &path);
        }
        OutsideGameOpV1::ExtrapolateRevealedV1 { cards } => {
            push_options(continuation, PlayerRef::Opponent, put_options(cards), &path);
        }
        OutsideGameOpV1::PutIntoHandV1 { card } => {
            put_into_hand(state, controller, *card);
        }
    }
    Ok(false)
}

/// The plain executor: only the put is a plain leaf; the choices run in
/// `drive`.
pub(crate) fn execute(op: &OutsideGameOpV1, ctx: &ExecCtx, state: &mut GameState) {
    match op {
        OutsideGameOpV1::PutIntoHandV1 { card } => {
            put_into_hand(state, ctx.controller, *card);
        }
        _ => {
            state.engine.halted = Some((
                crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
                ctx.source,
            ));
        }
    }
}

/// Reveals one `card` from `owner`'s cards outside the game and puts it into
/// their hand as a new object, known to the opponent. A card no longer
/// outside the game is not put anywhere.
fn put_into_hand(state: &mut GameState, owner: PlayerId, card: u16) -> Option<ObjectId> {
    let list = &mut state.eternal_v1.as_mut()?.outside_game.cards[owner.index()];
    let position = list.iter().position(|&listed| listed == card)?;
    list.remove(position);
    let id = state.objects.push(GameObject {
        card_def: card,
        name: CARD_DEFS[usize::from(card)].object_name.into(),
        owner,
        controller: owner,
        zone: Zone::Hand,
        tapped: false,
        summoning_sick: false,
        damage: 0,
        counters: Default::default(),
        attachments: Vec::new(),
        v4: ObjectStateV4::from_card_def(card),
        spell_copy_origin: None,
        plotted_turn: None,
        zone_change_count: 0,
    });
    state.players[owner.index()].hand.push(id);
    state
        .reveal_hand_card(owner.opponent(), owner, id)
        .expect("the new card is in its owner's hand");
    Some(id)
}
