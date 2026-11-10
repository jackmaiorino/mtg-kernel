//! Play permissions and zone replacements of the wave E5a cards: casting
//! from the top of the library (Mystic Forge, Bolas's Citadel), playing
//! lands from the top of the library (Citadel) and from the graveyard
//! (Yawgmoth's Will), and the exile replacements of Yawgmoth's Will and
//! Suplex.

use super::{
    controls_active, e5a_state, read_state, FutureSightOverloadV1, LibraryCastV1, BOLAS_CITADEL,
    MYSTIC_FORGE,
};
use crate::card_def::{CardType, CARD_DEFS};
use crate::event::{self, CommittedEvent, ProposedEvent};
use crate::event_history_v1::EventKindV1;
use crate::ids::{ObjectId, PlayerId};
use crate::state::{
    AbilitySourceContractV4, CastMethodV4, GameState, ObjectLinkV4, SpellCastRouteV4, StackItemKind,
    Zone,
};

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
    // Future Sight: "You may play the top card of your library" (Premodern
    // wave P2a), paying its mana cost.
    if crate::eternal_premodern_p2a_v1::statics::future_sight(state, player) {
        return Some(false);
    }
    (controls_active(state, player, MYSTIC_FORGE)
        && (def.has_type(CardType::Artifact)
            || (def.colors.is_empty()
                && crate::eternal_legacy_e9a_v1::statics::painter_mask(state) == 0)))
        .then_some(false)
}

/// Mystic Forge and Bolas's Citadel: "You may look at the top card of your
/// library any time." Before a priority decision, each player controlling
/// one knows the top card of their own library; the opponent learns
/// nothing.
pub(crate) fn look_at_library_tops(state: &mut GameState) {
    for player in [PlayerId::P0, PlayerId::P1] {
        let Some(&top) = state.players[player.index()].library.first() else {
            continue;
        };
        // Future Sight: "Play with the top card of your library revealed",
        // to both players (Premodern wave P2a).
        if crate::eternal_premodern_p2a_v1::statics::future_sight(state, player) {
            let incarnation = state.objects.get(top).zone_change_count;
            for observer in [PlayerId::P0, PlayerId::P1] {
                let known = state
                    .known_library_cards(observer, player)
                    .iter()
                    .any(|entry| {
                        entry.position == 0
                            && entry.object == top
                            && entry.zone_change_count == incarnation
                    });
                if !known {
                    // spellbench-public-reveal: Future Sight reveals the top card to both players.
                    state.reveal_library_top(observer, player, 1);
                }
            }
            continue;
        }
        if !(controls_active(state, player, MYSTIC_FORGE)
            || controls_active(state, player, BOLAS_CITADEL))
        {
            continue;
        }
        let incarnation = state.objects.get(top).zone_change_count;
        let known = state
            .known_library_cards(player, player)
            .iter()
            .any(|entry| {
                entry.position == 0 && entry.object == top && entry.zone_change_count == incarnation
            });
        if !known {
            state.reveal_library_top(player, player, 1);
        }
    }
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
        future_sight_overload: library_overload_source(state, player, card).map(|source| {
            FutureSightOverloadV1 {
                source: AbilitySourceContractV4::capture(state, source),
                announced_at: state.engine.event_history.len(),
            }
        }),
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

fn library_overload_source(
    state: &GameState,
    player: PlayerId,
    card: ObjectId,
) -> Option<ObjectId> {
    if !is_library_top(state, player, card)
        || !crate::eternal_sideboard_s1d_v1::statics::is_overload(
            CARD_DEFS
                .get(usize::from(state.objects.get(card).card_def))?
                .name,
        )
    {
        return None;
    }
    crate::eternal_premodern_p2a_v1::statics::future_sight_source(state, player)
}

/// Future Sight grants a normally paid cast. Overload replaces that mana
/// cost even when the ordinary form can instead use Bolas's Citadel.
pub(crate) fn library_overload_allowed(
    state: &GameState,
    player: PlayerId,
    card: ObjectId,
) -> bool {
    let Some(live) = state.objects.try_get(card) else {
        return false;
    };
    if live.zone == Zone::Library {
        return library_overload_source(state, player, card).is_some();
    }
    let Some(origin) = live.v4.spell_cast_origin else {
        return false;
    };
    if live.zone != Zone::Stack
        || origin.origin_zone != Zone::Library
        || live.owner != player
        || (origin.finalized_method.is_none() && live.controller != player)
        || origin.origin_zone_change_count.checked_add(1) != Some(live.zone_change_count)
        || !matches!(origin.route, SpellCastRouteV4::LibraryPermissionV1 {
            holder, permission_zone_change_count
        } if holder == player && permission_zone_change_count == origin.origin_zone_change_count)
    {
        return false;
    }
    recorded_cast(
        state,
        ObjectLinkV4 {
            object: card,
            zone_change_count: origin.origin_zone_change_count,
        },
    )
    .filter(|cast| cast.holder == player)
    .and_then(|cast| cast.future_sight_overload)
    .is_some_and(|permit| {
        let source = permit.source;
        source.zone == Zone::Battlefield
            && source.controller == player
            && permit.announced_at <= state.engine.event_history.len()
            && crate::card_def::card_id_by_name(crate::eternal_premodern_p2a_v1::FUTURE_SIGHT)
                == Some(source.card_def)
            && state.objects.try_get(source.source).is_some_and(|live| {
                live.owner == source.owner
                    && if origin.finalized_method.is_none() {
                        AbilitySourceContractV4::capture(state, source.source) == source
                            && state.players[player.index()]
                                .battlefield
                                .contains(&source.source)
                            && crate::eternal_premodern_p2a_v1::active_named(
                                state,
                                source.source,
                                crate::eternal_premodern_p2a_v1::FUTURE_SIGHT,
                            )
                    } else if live.zone_change_count == source.zone_change_count {
                        live.zone == Zone::Battlefield && live.card_def == source.card_def
                    } else {
                        origin.finalized_method == Some(CastMethodV4::Omen)
                            && live.zone_change_count > source.zone_change_count
                            && state
                                .engine
                                .event_history
                                .iter()
                                .skip(permit.announced_at)
                                .any(|event| {
                                    matches!(event, CommittedEvent::ZoneChange {
                                    object, from: Zone::Battlefield, to, ..
                                } if *object == source.source && *to != Zone::Battlefield)
                                })
                    }
            })
    })
}

/// Freeze the exact original paid permission before committing costs. Other
/// library spells keep the original cast-identity receipt shape.
pub(crate) fn prepare_library_overload_publication(
    state: &GameState,
    card: ObjectId,
    player: PlayerId,
    method: CastMethodV4,
) -> Result<Option<FutureSightOverloadV1>, String> {
    let live = state.objects.get(card);
    let Some(origin) = live.v4.spell_cast_origin else {
        return Ok(None);
    };
    if method != CastMethodV4::Omen
        || origin.origin_zone != Zone::Library
        || !crate::eternal_sideboard_s1d_v1::statics::is_overload(
            CARD_DEFS[usize::from(live.card_def)].name,
        )
    {
        return Ok(None);
    }
    if !library_overload_allowed(state, player, card) {
        return Err("missing original paid Future Sight permission".into());
    }
    recorded_cast(state, ObjectLinkV4 {
        object: card,
        zone_change_count: origin.origin_zone_change_count,
    })
    .and_then(|cast| cast.future_sight_overload)
    .map(Some)
    .ok_or_else(|| "missing Future Sight announcement binding".into())
}

/// The serialized announcement may still be read by ordinary target checks
/// before publication. At native entry points every completed overload on
/// the stack must also match its independently published paid cast.
pub(crate) fn validate_published_library_overloads(state: &GameState) -> Result<(), String> {
    if !cfg!(feature = "eternal-fixtures") {
        return Ok(());
    }
    let history = &state.engine.event_history;
    for item in &state.stack {
        if item.kind != StackItemKind::Spell
            || item.is_copy
            || item.v4.cast_method != Some(CastMethodV4::Omen)
        {
            continue;
        }
        let live = state.objects.try_get(item.source).ok_or("library spell source is missing")?;
        let Some(origin) = live.v4.spell_cast_origin else { continue };
        let definition = CARD_DEFS.get(usize::from(live.card_def))
            .ok_or("library spell printing is missing")?;
        if origin.origin_zone != Zone::Library
            || !crate::eternal_sideboard_s1d_v1::statics::is_overload(definition.name)
        {
            continue;
        }
        let caster = crate::eternal_vintage_e7b_v1::spell_caster(state, item)
            .unwrap_or(item.controller);
        let Some(permit) = prepare_library_overload_publication(
            state, item.source, caster, CastMethodV4::Omen,
        )? else {
            return Err("library overload lost its paid permission".into());
        };
        let mut publications = history.scan_for_v1(&[EventKindV1::PublishedCastIdentityV1])
            .iter()
            .enumerate()
            .filter_map(|(at, event)| match event {
                CommittedEvent::PublishedCastIdentityV1 { receipt }
                    if receipt.stack_item_id == item.v4.stack_item_id => Some((at, receipt)),
                _ => None,
            });
        let Some((at, receipt)) = publications.next() else {
            return Err("library overload lost its finalized cast publication".into());
        };
        let identity = crate::cast_identity_v1::PublicIdentityV1::Known { card_def: live.card_def };
        if publications.next().is_some()
            || origin.finalized_method != Some(CastMethodV4::Omen)
            || item.v4.stack_item_id.0 == 0
            || receipt.future_sight_overload != Some(permit)
            || permit.announced_at > at
            || receipt.spell != item.source
            || receipt.owner != live.owner
            || receipt.controller != caster
            || receipt.origin_zone != Zone::Library
            || receipt.origin_zone_change_count != origin.origin_zone_change_count
            || receipt.stack_zone_change_count != live.zone_change_count
            || receipt.public_identity != identity
            || receipt.spell_cast_history_index != (at + 1) as u64
            || history.get(at + 1) != Some(&CommittedEvent::SpellCast {
                spell: item.source,
                controller: caster,
            })
        {
            return Err("library overload publication disagrees with its paid permission".into());
        }
    }
    Ok(())
}

/// `SpellCastRouteV4::LibraryPermissionV1`'s authority: the library
/// incarnation (`object`, `zone_change_count`) was announced by `holder`
/// through Mystic Forge, Bolas's Citadel or Future Sight.
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
    if controls_active(state, player, BOLAS_CITADEL)
        || crate::eternal_premodern_p2a_v1::statics::future_sight(state, player)
    {
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
