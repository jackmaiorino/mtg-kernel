//! The outside-the-game zone of the eternal catalog (stage S3 in
//! `docs/design/eternal_formats_support_v1.md`): each player's cards outside
//! the game, private to their owner, and the wish effects that read them.
//!
//! A best-of-three game's outside-the-game cards are its sideboard without a
//! companion in use (`eternal_bo3_v1`); a custom-deck session receives them
//! at reset. The list lives in module state, never in a zone list. The public
//! profile is announced without names or counts; unsupported infoset exports
//! refuse before canonicalization. A card leaves the list only
//! when an effect reveals it and puts it into its owner's hand; the new hand
//! card is then known to the opponent. Best-of-one games omit the profile,
//! so every wish there finds nothing, as before.
//!
//! Choices among outside-the-game cards are effect options in card id order
//! (the decline option first where the effect is optional). Limited session
//! replies describe the options to their chooser, including the revealed
//! pair when Extrapolate asks the opponent to choose.
#![cfg_attr(not(feature = "eternal-fixtures"), allow(dead_code))]

use crate::card_def::{CardType, Subtype, CARD_DEFS};
use crate::effect::{
    EffectContinuation, EffectFrame, EffectObjectBinding, EffectOp, ExecCtx, PendingEffectChoice,
    PlayerRef,
};
use crate::eternal_cards_v1::e3::{E3ChoiceKindV1, E3ChoiceV1};
use crate::eternal_cards_v1::EternalOpV1;
use crate::event::{self, ProposedEvent};
use crate::ids::{ObjectId, PlayerId};
use crate::standard_cards_v1::StandardOpV1;
use crate::state::{GameObject, GameState, ObjectStateV4, Target, Zone};
use serde::{Deserialize, Serialize};
use std::hash::{Hash, Hasher};

/// Each player's cards outside the game, sorted by card id. The explicit
/// public profile stays enabled even if both private lists become empty.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OutsideGameStateV1 {
    #[serde(default = "enabled_profile", skip_serializing_if = "is_false")]
    pub enabled: bool,
    #[serde(default)]
    pub cards: [Vec<u16>; 2],
}

fn is_false(value: &bool) -> bool {
    !value
}

// A present historical record denotes an explicit profile. The enclosing
// EternalStateV1 still uses Default (disabled) when the whole field is absent.
fn enabled_profile() -> bool {
    true
}

impl Hash for OutsideGameStateV1 {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // EternalStateV1 derives Hash. An empty extension must therefore write
        // nothing, including no empty-vector lengths, to preserve its old hash.
        if !self.is_empty() {
            b"outside-the-game/v1".hash(state);
            self.enabled.hash(state);
            self.cards.hash(state);
        }
    }
}

impl OutsideGameStateV1 {
    pub(crate) fn is_empty(&self) -> bool {
        !self.enabled && self.cards.iter().all(Vec::is_empty)
    }
}

/// Opts into the public outside-game profile before the game begins. Lists
/// and their sizes stay private. Legacy best-of-one callers do not call this.
pub fn set_outside_the_game(state: &mut GameState, cards: [Vec<u16>; 2]) {
    let outside = &mut state
        .eternal_v1
        .get_or_insert_with(Default::default)
        .outside_game;
    outside.enabled = true;
    for (player, mut list) in cards.into_iter().enumerate() {
        list.sort_unstable();
        outside.cards[player] = list;
    }
}

/// Public capability only, independent of either private list's contents.
/// Nonempty snapshots from the draft before the explicit flag also refuse
/// unsupported exports rather than copying their private cards.
pub fn outside_game_profile(state: &GameState) -> Option<&'static str> {
    state
        .eternal_v1
        .as_ref()
        .filter(|eternal| !eternal.outside_game.is_empty())
        .map(|_| "outside-the-game/v1")
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
    /// Karn's choice of up to one face-up artifact card its controller owns
    /// in exile, put into their hand. A choice.
    KarnExileV1,
    /// Learn's "discard a card to draw a card": the card to discard. A
    /// choice.
    LearnDiscardV1,
}

/// A chooser's option in a wish or learn decision, in effect-option order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum OutsideGameOptionV1 {
    Decline,
    Card { card_def: u16 },
    RevealPair { card_defs: [u16; 2] },
    ChooseExiledArtifact,
    DiscardToDraw,
}

pub(crate) fn is_outside_game_choice(options: &[EffectOp]) -> bool {
    options.iter().any(|op| {
        matches!(
            op,
            EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::OutsideGameV1(_)))
        )
    })
}

/// Describe only the current chooser's options. In particular, the opponent
/// sees Extrapolate's two revealed cards but none of the other sideboard.
pub fn decision_options(
    state: &GameState,
    viewer: crate::rl::PlayerSeatV1,
) -> Option<Vec<OutsideGameOptionV1>> {
    let pending = state.engine.pending_effect.as_ref()?;
    let PendingEffectChoice::ChooseOption {
        player, options, ..
    } = pending.choice.as_ref()?
    else {
        return None;
    };
    if viewer != (*player).into() || !is_outside_game_choice(options) {
        return None;
    }
    options
        .iter()
        .map(|option| match option {
            EffectOp::Sequence(ops) if ops.is_empty() => Some(OutsideGameOptionV1::Decline),
            EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::OutsideGameV1(op))) => {
                Some(match op {
                    OutsideGameOpV1::PutIntoHandV1 { card }
                    | OutsideGameOpV1::ExtrapolateSecondV1 { first: card } => {
                        OutsideGameOptionV1::Card { card_def: *card }
                    }
                    OutsideGameOpV1::ExtrapolateRevealedV1 { cards } => {
                        OutsideGameOptionV1::RevealPair { card_defs: *cards }
                    }
                    OutsideGameOpV1::KarnExileV1 => OutsideGameOptionV1::ChooseExiledArtifact,
                    OutsideGameOpV1::LearnDiscardV1 => OutsideGameOptionV1::DiscardToDraw,
                    _ => return None,
                })
            }
            _ => None,
        })
        .collect()
}

/// Resolution choices owned by this module, carried as
/// `E3ChoiceKindV1::OutsideGameV1`. They are typed choices rather than
/// generic object selections because the wish effects build them as they
/// resolve.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OutsideGameChoiceV1 {
    /// Karn, the Great Creator's -2: up to one face-up artifact card the
    /// chooser owns in exile.
    KarnExileV1,
    /// Learn: exactly one card from the chooser's hand to discard.
    LearnDiscardV1,
}

impl OutsideGameChoiceV1 {
    /// Whether the candidates are hidden from the other player.
    pub(crate) fn chooser_private(&self) -> bool {
        matches!(self, Self::LearnDiscardV1)
    }

    pub(crate) fn public_purpose(&self) -> crate::rl::TargetSelectionPurposeV4 {
        crate::rl::TargetSelectionPurposeV4::CardSelection
    }
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
            let mut options = vec![outside(OutsideGameOpV1::KarnExileV1)];
            let cards = eligible(state, controller, |card| has_type(card, CardType::Artifact));
            options.extend(put_options(&cards));
            push_options(continuation, PlayerRef::Controller, options, &path);
        }
        OutsideGameOpV1::LearnV1 => {
            let mut options = vec![decline()];
            if !state.players[controller.index()].hand.is_empty() {
                options.push(outside(OutsideGameOpV1::LearnDiscardV1));
            }
            let lessons = eligible(state, controller, is_lesson);
            options.extend(put_options(&lessons));
            push_options(continuation, PlayerRef::Controller, options, &path);
        }
        OutsideGameOpV1::KarnExileV1 => {
            let cards = exiled_artifacts(state, controller);
            if !cards.is_empty() {
                offer(
                    continuation,
                    OutsideGameChoiceV1::KarnExileV1,
                    controller,
                    cards,
                    0,
                    path,
                );
                return Ok(true);
            }
        }
        OutsideGameOpV1::LearnDiscardV1 => {
            let cards = crate::standard_cards_v1::hand_candidates(state, controller);
            if !cards.is_empty() {
                offer(
                    continuation,
                    OutsideGameChoiceV1::LearnDiscardV1,
                    controller,
                    cards,
                    1,
                    path,
                );
                return Ok(true);
            }
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

/// The face-up artifact cards `player` owns in exile.
fn exiled_artifacts(state: &GameState, player: PlayerId) -> Vec<EffectObjectBinding> {
    state
        .exile
        .iter()
        .copied()
        .filter(|&object| {
            let live = state.objects.get(object);
            live.owner == player
                && live.zone == Zone::Exile
                && live.v4.face_down_v1.is_none()
                && has_type(live.card_def, CardType::Artifact)
        })
        .map(|object| EffectObjectBinding {
            object,
            expected_zone: Zone::Exile,
            expected_zone_change_count: state.objects.get(object).zone_change_count,
        })
        .collect()
}

/// Stages one of this module's selections of exactly `min` to one card.
fn offer(
    continuation: &mut EffectContinuation,
    kind: OutsideGameChoiceV1,
    player: PlayerId,
    bindings: Vec<EffectObjectBinding>,
    min: u16,
    path: Vec<u16>,
) {
    let remaining = continuation.frames.clone();
    crate::eternal_cards_v1::e3::offer(
        continuation,
        E3ChoiceV1 {
            kind: E3ChoiceKindV1::OutsideGameV1(kind),
            player,
            candidates: bindings
                .iter()
                .map(|binding| Target::Object(binding.object))
                .collect(),
            bindings,
            min,
            max: 1,
            ordered: false,
            path,
            remaining,
        },
    );
}

/// Applies an answered selection. `Ok(true)` would mean it staged a further
/// choice; neither does.
pub(crate) fn finish_selection(
    state: &mut GameState,
    continuation: &mut EffectContinuation,
    choice: &E3ChoiceV1,
    kind: &OutsideGameChoiceV1,
    bindings: Vec<EffectObjectBinding>,
) -> Result<bool, String> {
    match kind {
        OutsideGameChoiceV1::KarnExileV1 => {
            if let Some(card) = bindings.first() {
                let live = state.objects.get(card.object);
                if live.owner != choice.player || live.v4.face_down_v1.is_some() {
                    return Err("Karn's exiled card changed".into());
                }
                event::propose_and_commit(
                    state,
                    ProposedEvent::zone_change(card.object, Zone::Hand),
                );
            }
            Ok(false)
        }
        OutsideGameChoiceV1::LearnDiscardV1 => {
            let [card] = bindings.as_slice() else {
                return Err("learn discards exactly one card".into());
            };
            crate::engine::commit_discarded_card(state, card.object);
            // The draw runs in the interpreter, where dredge may replace it.
            continuation.frames.push(EffectFrame::Program {
                op: EffectOp::DrawCards {
                    player: PlayerRef::Controller,
                    count: 1,
                },
                path: choice.path.clone(),
            });
            Ok(false)
        }
    }
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
    let outside = &mut state.eternal_v1.as_mut()?.outside_game;
    let list = &mut outside.cards[owner.index()];
    let position = list.iter().position(|&listed| listed == card)?;
    list.remove(position);
    outside.enabled = true;
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
    // spellbench-public-reveal: a new card is revealed as it enters hand.
    state
        .reveal_distinct_hand_card(owner.opponent(), owner, id)
        .expect("the new card is in its owner's hand");
    Some(id)
}
