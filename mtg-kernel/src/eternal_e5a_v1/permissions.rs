//! Play permissions and zone replacements of the wave E5a cards: casting
//! from the top of the library (Mystic Forge, Bolas's Citadel), playing
//! lands from the top of the library (Citadel) and from the graveyard
//! (Yawgmoth's Will), and the exile replacements of Yawgmoth's Will and
//! Suplex.

use super::{controls_active, e5a_state, read_state, LibraryCastV1, BOLAS_CITADEL, MYSTIC_FORGE};
use crate::card_def::{CardType, CARD_DEFS};
use crate::event::{self, ProposedEvent};
use crate::ids::{ObjectId, PlayerId};
use crate::state::{GameState, ObjectLinkV4, Zone};

/// Whether `card` is the top card of `player`'s own library.
fn is_library_top(state: &GameState, player: PlayerId, card: ObjectId) -> bool {
    state.players[player.index()].library.first() == Some(&card)
        && state
            .objects
            .try_get(card)
            .is_some_and(|live| live.zone == Zone::Library && live.owner == player)
}

/// How `player` may cast the top card of their library right now:
/// `Some(true)` through Bolas's Citadel ("pay life equal to its mana value
/// rather than pay its mana cost"), `Some(false)` through Mystic Forge
/// ("You may cast artifact spells and colorless spells from the top of
/// your library"), `None` if neither applies. A player controlling both
/// casts through the Citadel while they can pay the life.
pub(crate) fn library_top_cast(
    state: &GameState,
    player: PlayerId,
    card: ObjectId,
) -> Option<bool> {
    if !is_library_top(state, player, card) {
        return None;
    }
    let def = CARD_DEFS.get(state.objects.get(card).card_def as usize)?;
    if def.has_type(CardType::Land) || !def.is_castable() {
        return None;
    }
    if controls_active(state, player, BOLAS_CITADEL)
        && state.players[player.index()].life >= i32::from(def.mana_value)
    {
        return Some(true);
    }
    (controls_active(state, player, MYSTIC_FORGE)
        && (def.has_type(CardType::Artifact) || def.colors.is_empty()))
    .then_some(false)
}

/// Records a spell announced from the top of its owner's library, so the
/// cast keeps its permission and payment form while it is on the stack.
pub(crate) fn note_library_cast(state: &mut GameState, player: PlayerId, card: ObjectId) {
    let Some(pay_life) = library_top_cast(state, player, card) else {
        return;
    };
    let cast = LibraryCastV1 {
        card: super::live_link(state, card),
        holder: player,
        pay_life,
    };
    let casts = &mut e5a_state(state).library_casts;
    casts.retain(|recorded| recorded.card.object != card);
    casts.push(cast);
}

fn recorded_cast(state: &GameState, card: ObjectLinkV4) -> Option<LibraryCastV1> {
    read_state(state)?
        .library_casts
        .iter()
        .copied()
        .find(|cast| cast.card == card)
}

/// `SpellCastRouteV4::LibraryPermissionV1`'s authority: the library
/// incarnation (`object`, `zone_change_count`) was announced by `holder`
/// through Mystic Forge or Bolas's Citadel.
pub(crate) fn library_permission(
    state: &GameState,
    object: ObjectId,
    holder: PlayerId,
    zone_change_count: u32,
) -> bool {
    let card = ObjectLinkV4 {
        object,
        zone_change_count,
    };
    recorded_cast(state, card).is_some_and(|cast| cast.holder == holder)
        || (state
            .objects
            .try_get(object)
            .is_some_and(|live| live.zone_change_count == zone_change_count)
            && library_top_cast(state, holder, object).is_some())
}

/// Whether casting `source` pays life instead of its mana cost: a library
/// card offered through Bolas's Citadel, or a spell announced that way.
pub(crate) fn citadel_cast(state: &GameState, source: ObjectId, player: PlayerId) -> bool {
    let Some(live) = state.objects.try_get(source) else {
        return false;
    };
    match live.zone {
        Zone::Library => library_top_cast(state, player, source) == Some(true),
        Zone::Stack => live.v4.spell_cast_origin.is_some_and(|origin| {
            origin.origin_zone == Zone::Library
                && recorded_cast(
                    state,
                    ObjectLinkV4 {
                        object: source,
                        zone_change_count: origin.origin_zone_change_count,
                    },
                )
                .is_some_and(|cast| cast.pay_life && cast.holder == player)
        }),
        _ => false,
    }
}

/// Bolas's Citadel's life payment, made as the cast's other costs are
/// paid.
pub(crate) fn pay_citadel_life(state: &mut GameState, spell: ObjectId, player: PlayerId) {
    if !citadel_cast(state, spell, player) {
        return;
    }
    let life = i32::from(crate::engine::object_mana_value(state, spell));
    if life > 0 {
        event::propose_and_commit(state, ProposedEvent::life_payment(player, life));
    }
}

// ---- Yawgmoth's Will ----------------------------------------------------------------

/// Whether a Yawgmoth's Will resolved for `player` this turn.
pub(crate) fn will_active(state: &GameState, player: PlayerId) -> bool {
    read_state(state).is_some_and(|e5a| {
        e5a.wills
            .iter()
            .any(|will| will.player == player && will.current(state))
    })
}

/// Yawgmoth's Will: "you may ... cast spells from your graveyard." Whether
/// `holder` may cast the card `object` from their graveyard this turn.
pub(crate) fn will_permission(state: &GameState, object: ObjectId, holder: PlayerId) -> bool {
    will_active(state, holder)
        && state.objects.try_get(object).is_some_and(|live| {
            live.owner == holder
                && !live.v4.is_token
                && live.spell_copy_origin.is_none()
                && CARD_DEFS
                    .get(live.card_def as usize)
                    .is_some_and(|def| !def.is_token)
        })
}

/// The lands `player` may play from outside their hand: their graveyard's
/// lands under Yawgmoth's Will, and the top card of their library under
/// Bolas's Citadel ("You may play lands ... from the top of your library").
pub(crate) fn extra_land_candidates(state: &GameState, player: PlayerId) -> Vec<ObjectId> {
    let mut out = Vec::new();
    if will_active(state, player) {
        out.extend(
            state.players[player.index()]
                .graveyard
                .iter()
                .copied()
                .filter(|&card| {
                    let live = state.objects.get(card);
                    live.owner == player
                        && !live.v4.is_token
                        && CARD_DEFS[live.card_def as usize].is_playable_land()
                }),
        );
    }
    if controls_active(state, player, BOLAS_CITADEL) {
        if let Some(&top) = state.players[player.index()].library.first() {
            if CARD_DEFS[state.objects.get(top).card_def as usize].is_playable_land() {
                out.push(top);
            }
        }
    }
    out
}

// ---- Zone replacements --------------------------------------------------------------

/// Yawgmoth's Will: "If a card would be put into your graveyard from
/// anywhere this turn, exile that card instead." Suplex: "If that creature
/// would die this turn, exile it instead." Applied as every zone change
/// commits.
pub(crate) fn replace_zone_change(state: &GameState, change: &mut event::ZoneChangeProposed) {
    if change.to_zone != Zone::Graveyard {
        return;
    }
    let Some(e5a) = read_state(state) else {
        return;
    };
    if e5a.wills.is_empty() && e5a.exile_if_dies.is_empty() {
        return;
    }
    let Some(live) = state.objects.try_get(change.object) else {
        return;
    };
    let dies = live.zone == Zone::Battlefield
        && crate::engine::object_has_type(state, change.object, CardType::Creature)
        && e5a.exile_if_dies.iter().any(|(link, effect)| {
            link.object == change.object
                && link.zone_change_count == live.zone_change_count
                && effect.current(state)
        });
    let card = !live.v4.is_token
        && live.spell_copy_origin.is_none()
        && CARD_DEFS
            .get(live.card_def as usize)
            .is_some_and(|def| !def.is_token);
    let willed = card
        && e5a
            .wills
            .iter()
            .any(|will| will.player == live.owner && will.current(state));
    if dies || willed {
        change.to_zone = Zone::Exile;
    }
}
