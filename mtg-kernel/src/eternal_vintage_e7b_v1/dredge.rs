//! Dredge (702.52): "If you would draw a card, you may mill N cards
//! instead. If you do, return this card from your graveyard to your hand."
//!
//! Dredge replaces a draw while the player's library holds at least N
//! cards (702.52b). The choice is offered for every draw an effect makes
//! inside the resumable interpreter (each card of a multi-card draw is its
//! own draw) and for the draw step's draw, which waits as an as-enters
//! style decision (`defer_draw_step_draw`). Effects that resolve through the
//! synchronous interpreter are moved to the resumable one while a dredge
//! choice exists (`forces_resumable`).

use super::{
    binding_link, definition, definition_name, e7b_state, graveyard_cards, link_live_in,
    read_state, E7bChoiceV1, GOLGARI_GRAVE_TROLL, GOLGARI_THUG, SHAMBLING_SHELL, STINKWEED_IMP,
};
use crate::effect::{EffectContinuation, EffectFrame, EffectObjectBinding, EffectOp, PlayerRef};
use crate::eternal_cards_v1::e3::{E3ChoiceKindV1, E3ChoiceV1};
use crate::event::{self, ProposedEvent};
use crate::ids::{ObjectId, PlayerId};
use crate::state::{GameState, Target, Zone};

/// Life from the Loam (wave E7a): "Dredge 3".
const LIFE_FROM_THE_LOAM: &str = "Life from the Loam";

/// A card's dredge number.
pub(crate) fn dredge_count(name: &str) -> Option<u8> {
    match name {
        GOLGARI_GRAVE_TROLL => Some(6),
        STINKWEED_IMP => Some(5),
        GOLGARI_THUG => Some(4),
        SHAMBLING_SHELL | LIFE_FROM_THE_LOAM => Some(3),
        _ => None,
    }
}

/// The dredge cards in `player`'s graveyard they could dredge now.
pub(crate) fn dredge_cards(state: &GameState, player: PlayerId) -> Vec<ObjectId> {
    let library = state.players[player.index()].library.len();
    graveyard_cards(state, player)
        .into_iter()
        .filter(|&card| {
            definition_name(state, card)
                .and_then(dredge_count)
                .is_some_and(|count| library >= usize::from(count))
        })
        .collect()
}

/// Whether some player has a dredge card they could dredge now.
fn any_dredge(state: &GameState) -> bool {
    [PlayerId::P0, PlayerId::P1]
        .into_iter()
        .any(|player| !dredge_cards(state, player).is_empty())
}

/// Whether `op` draws cards anywhere in its tree.
fn draws(op: &EffectOp) -> bool {
    match op {
        EffectOp::DrawCards { .. } | EffectOp::DrawCardsDynamic { .. } => true,
        EffectOp::Sequence(ops) => ops.iter().any(draws),
        EffectOp::Conditional { then, else_, .. } => draws(then) || draws(else_),
        EffectOp::Choice { options, .. } => options.iter().any(draws),
        _ => false,
    }
}

/// Whether a synchronous program should run in the resumable interpreter
/// so its draws can be dredged.
pub(crate) fn forces_resumable(state: &GameState, program: &EffectOp) -> bool {
    cfg!(feature = "eternal-fixtures")
        && draws(program)
        && any_dredge(state)
        && crate::effect::resumable_program_supported(program)
}

/// Whether a draw by `player` (resolved against the continuation) may be
/// dredged.
pub(crate) fn draw_may_dredge(
    state: &GameState,
    continuation: &EffectContinuation,
    player: PlayerRef,
) -> bool {
    let player = continuation.ctx.resolve_player(player, state);
    !dredge_cards(state, player).is_empty()
}

/// Draws `count` cards for `player`, offering dredge for the first. The
/// rest wait as a frame and are offered in turn. `Ok(true)` when a choice
/// was staged.
pub(crate) fn drive_draws(
    state: &mut GameState,
    continuation: &mut EffectContinuation,
    player: PlayerRef,
    count: u32,
    path: Vec<u16>,
) -> Result<bool, String> {
    if count == 0 {
        return Ok(false);
    }
    let drawer = continuation.ctx.resolve_player(player, state);
    let cards = dredge_cards(state, drawer);
    if cards.is_empty() {
        for _ in 0..count {
            event::propose_and_commit(state, ProposedEvent::draw(drawer));
        }
        return Ok(false);
    }
    if count > 1 {
        continuation.frames.push(EffectFrame::Program {
            op: EffectOp::DrawCards {
                player,
                count: count - 1,
            },
            path: path.clone(),
        });
    }
    let bindings = cards
        .iter()
        .map(|&card| super::bind(state, card))
        .collect::<Vec<_>>();
    let remaining = continuation.frames.clone();
    crate::eternal_cards_v1::e3::offer(
        continuation,
        E3ChoiceV1 {
            kind: E3ChoiceKindV1::E7bV1(E7bChoiceV1::DredgeV1),
            player: drawer,
            candidates: cards.iter().map(|&card| Target::Object(card)).collect(),
            bindings,
            min: 0,
            max: 1,
            ordered: false,
            path,
            remaining,
        },
    );
    Ok(true)
}

/// Applies a dredge choice: no card draws, a card dredges.
pub(crate) fn finish(state: &mut GameState, player: PlayerId, chosen: Option<EffectObjectBinding>) {
    let Some(card) = chosen else {
        event::propose_and_commit(state, ProposedEvent::draw(player));
        return;
    };
    if !dredge(state, player, card.object, binding_link(&card)) {
        event::propose_and_commit(state, ProposedEvent::draw(player));
    }
}

/// Mills the card's dredge number, then returns it from the graveyard to
/// its owner's hand. `false` (and nothing happens) if it can no longer
/// dredge.
fn dredge(
    state: &mut GameState,
    player: PlayerId,
    card: ObjectId,
    link: crate::state::ObjectLinkV4,
) -> bool {
    let Some(count) = definition(state, card).and_then(|def| dredge_count(def.name)) else {
        return false;
    };
    let library = state.players[player.index()].library.clone();
    if !link_live_in(state, link, Zone::Graveyard)
        || state.objects.get(card).owner != player
        || library.len() < usize::from(count)
    {
        return false;
    }
    let milled = library
        .into_iter()
        .take(usize::from(count))
        .map(|object| ProposedEvent::zone_change(object, Zone::Graveyard))
        .collect::<Vec<_>>();
    event::propose_and_commit_batch(state, milled);
    if link_live_in(state, link, Zone::Graveyard) {
        event::propose_and_commit(
            state,
            ProposedEvent::zone_change_preserving_known_identity(card, Zone::Hand),
        );
    }
    true
}

// ---- The draw step ------------------------------------------------------------------

/// The draw step's draw: whether `player` may dredge instead, so the draw
/// waits for their choice (option 0 draws, option N dredges their Nth
/// dredge card).
pub(crate) fn defer_draw_step_draw(state: &mut GameState, player: PlayerId) -> bool {
    if dredge_cards(state, player).is_empty() {
        return false;
    }
    e7b_state(state).dredge_draw = Some(player);
    true
}

/// The pending draw-step dredge choice, if any.
pub(crate) fn pending_decision(state: &GameState) -> Option<crate::engine::Decision> {
    let player = read_state(state)?.dredge_draw?;
    let cards = dredge_cards(state, player);
    Some(crate::engine::Decision::ChooseEffectOption {
        player,
        source: cards.first().copied()?,
        option_count: u16::try_from(cards.len() + 1).ok()?,
    })
}

/// Answers the pending draw-step dredge choice. `Ok(false)` when none is
/// pending.
pub(crate) fn answer_pending_decision(state: &mut GameState, option: u16) -> Result<bool, String> {
    let Some(player) = read_state(state).and_then(|e7b| e7b.dredge_draw) else {
        return Ok(false);
    };
    let cards = dredge_cards(state, player);
    e7b_state(state).dredge_draw = None;
    let dredged = match usize::from(option) {
        0 => false,
        index => {
            let card = *cards
                .get(index - 1)
                .ok_or("the dredge option is out of range")?;
            let link = super::live_link(state, card);
            dredge(state, player, card, link)
        }
    };
    if !dredged {
        event::propose_and_commit(state, ProposedEvent::draw(player));
        crate::eternal_cards_v1::note_draw_step_draw(state, player);
    }
    Ok(true)
}
