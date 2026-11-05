//! Static abilities and engine hooks of the wave E7b cards: spell taxes by
//! type (Thorn of Amethyst, Lodestone Golem), entering permanents that
//! cause no triggers (Torpor Orb, Karn, Argent Defender), activation
//! prohibitions (Collector Ouphe, Phyrexian Revoker), Crucible of Worlds'
//! land plays, protection from multicolored (Stonecoil Serpent),
//! regeneration (Golgari Grave-Troll), Hollow One's discard count, dash's
//! haste and the loyalty ability Avatar of Burgeoning Echoes grants.

use super::{
    active_named, active_permanents, definition, definition_name, e7b_state, is_creature,
    live_link, read_state, TurnCountV1, AVATAR_OF_BURGEONING_ECHOES, GOLGARI_GRAVE_TROLL,
};
use crate::card_def::{CardType, Keywords, CARD_DEFS};
use crate::ids::{ObjectId, PlayerId};
use crate::state::{AbilitySourceContractV4, GameState, ObjectLinkV4, Zone};

const COLLECTOR_OUPHE: &str = "Collector Ouphe";
const HOLLOW_ONE: &str = "Hollow One";
const CRUCIBLE_OF_WORLDS: &str = "Crucible of Worlds";
const KARN_ARGENT_DEFENDER: &str = "Karn, Argent Defender";
const LODESTONE_GOLEM: &str = "Lodestone Golem";
const PHYREXIAN_REVOKER: &str = "Phyrexian Revoker";
const SPHERE_OF_RESISTANCE: &str = "Sphere of Resistance";
const SERUM_POWDER: &str = "Serum Powder";
const STONECOIL_SERPENT: &str = "Stonecoil Serpent";
const THORN_OF_AMETHYST: &str = "Thorn of Amethyst";
const TORPOR_ORB: &str = "Torpor Orb";

// ---- Serum Powder -------------------------------------------------------------------

fn hand_links(state: &GameState, player: PlayerId) -> Vec<ObjectLinkV4> {
    state.players[player.index()]
        .hand
        .iter()
        .map(|&object| live_link(state, object))
        .collect()
}

/// Serum Powder: "Any time you could mulligan and this card is in your
/// hand, you may exile all the cards from your hand, then draw that many
/// cards." The Serum Powder in `player`'s hand to offer while they decide
/// whether to mulligan, unless they declined it for this exact hand.
pub(crate) fn serum_powder_offer(state: &GameState, player: PlayerId) -> Option<ObjectId> {
    let powder = state.players[player.index()]
        .hand
        .iter()
        .copied()
        .find(|&card| definition_name(state, card) == Some(SERUM_POWDER))?;
    let hand = hand_links(state, player);
    let declined = read_state(state).is_some_and(|e7b| {
        e7b.powder_declined
            .iter()
            .any(|(holder, declined)| *holder == player && *declined == hand)
    });
    (!declined).then_some(powder)
}

/// Records that `player` keeps their current hand past Serum Powder's
/// offer (they still choose whether to mulligan).
pub(crate) fn decline_serum_powder(state: &mut GameState, player: PlayerId) {
    let hand = hand_links(state, player);
    let e7b = e7b_state(state);
    e7b.powder_declined.retain(|(holder, _)| *holder != player);
    e7b.powder_declined.push((player, hand));
}

/// Forgets the declined Serum Powder offers once the mulligans end.
pub(crate) fn end_mulligans(state: &mut GameState) {
    if read_state(state).is_some_and(|e7b| !e7b.powder_declined.is_empty()) {
        e7b_state(state).powder_declined.clear();
    }
}

// ---- Spell taxes --------------------------------------------------------------------

/// Sphere of Resistance (wave E7a): "Spells cost {1} more to cast." Thorn
/// of Amethyst: "Noncreature spells cost {1} more to cast." Lodestone
/// Golem: "Nonartifact spells cost {1} more to cast." The generic increase
/// for a spell with `types`, whoever casts it.
pub(crate) fn spell_tax(state: &GameState, types: &[CardType]) -> u32 {
    let mut increase = u32::try_from(active_permanents(state, SPHERE_OF_RESISTANCE, None).count())
        .unwrap_or(u32::MAX);
    if !types.contains(&CardType::Creature) {
        increase = increase.saturating_add(
            u32::try_from(active_permanents(state, THORN_OF_AMETHYST, None).count())
                .unwrap_or(u32::MAX),
        );
    }
    if !types.contains(&CardType::Artifact) {
        increase = increase.saturating_add(
            u32::try_from(active_permanents(state, LODESTONE_GOLEM, None).count())
                .unwrap_or(u32::MAX),
        );
    }
    increase
}

// ---- Entering permanents that cause no triggers -------------------------------------

/// Torpor Orb: "Creatures entering don't cause abilities to trigger."
/// Karn, Argent Defender: "Artifacts and creatures entering the battlefield
/// don't cause abilities to trigger." Whether `object`'s entry causes no
/// triggers, looking at it as it exists on the battlefield (or as it last
/// existed there). Karn entering with another artifact or creature stops
/// both (its ruling), since it is on the battlefield when triggers are
/// checked.
pub(crate) fn entry_triggers_suppressed(state: &GameState, object: ObjectId) -> bool {
    let Some(live) = state.objects.try_get(object) else {
        return false;
    };
    let types = |card_type| {
        if live.zone == Zone::Battlefield {
            crate::engine::object_has_type(state, object, card_type)
        } else {
            CARD_DEFS
                .get(live.card_def as usize)
                .is_some_and(|def| def.has_type(card_type))
        }
    };
    let creature = types(CardType::Creature);
    let artifact = types(CardType::Artifact);
    (creature && active_permanents(state, TORPOR_ORB, None).next().is_some())
        || ((creature || artifact)
            && active_permanents(state, KARN_ARGENT_DEFENDER, None)
                .next()
                .is_some())
}

/// Whether `event` is an entry whose triggers Torpor Orb or Karn, Argent
/// Defender stop: a permanent moving onto the battlefield or a token being
/// created.
pub(crate) fn entry_event_suppressed(
    state: &GameState,
    event: &crate::event::CommittedEvent,
) -> bool {
    use crate::event::CommittedEvent;
    let object = match *event {
        CommittedEvent::ZoneChange {
            object,
            to: Zone::Battlefield,
            ..
        } => object,
        CommittedEvent::CreateToken { object, .. } => object,
        _ => return false,
    };
    entry_triggers_suppressed(state, object)
}

// ---- Activation prohibitions ----------------------------------------------------------

/// Collector Ouphe: "Activated abilities of artifacts can't be activated."
/// Phyrexian Revoker: "Activated abilities of sources with the chosen name
/// can't be activated." Both include mana abilities.
pub(crate) fn activations_prohibited(state: &GameState, object: ObjectId) -> bool {
    let Some(live) = state.objects.try_get(object) else {
        return false;
    };
    if live.zone == Zone::Battlefield
        && crate::engine::object_has_type(state, object, CardType::Artifact)
        && active_permanents(state, COLLECTOR_OUPHE, None)
            .next()
            .is_some()
    {
        return true;
    }
    let Some(e7b) = read_state(state) else {
        return false;
    };
    if e7b.revoker_names.is_empty() {
        return false;
    }
    let name = crate::engine::effective_name(state, object);
    e7b.revoker_names.iter().any(|(revoker, named)| {
        state
            .objects
            .try_get(revoker.object)
            .is_some_and(|live| live.zone_change_count == revoker.zone_change_count)
            && active_named(state, revoker.object, PHYREXIAN_REVOKER)
            && CARD_DEFS[usize::from(*named)].object_name == name
    })
}

/// The nonland card names `player` may choose for a Phyrexian Revoker: the
/// names of the nonland objects visible to them, as definition ids in
/// ascending order, one per name. As with Pithing Needle
/// (`eternal_lurrus_v1::needle_name_options`), naming a card nobody can see
/// would have no effect this game, so the choice is limited to these.
fn revoker_name_options(state: &GameState, player: PlayerId) -> Vec<u16> {
    crate::eternal_lurrus_v1::needle_name_options(state, player)
        .into_iter()
        .filter(|&id| {
            CARD_DEFS
                .get(usize::from(id))
                .is_some_and(|def| !def.has_type(CardType::Land))
        })
        .collect()
}

/// The Phyrexian Revoker that just entered and has not chosen a name.
fn revoker_needing_name(state: &GameState) -> Option<ObjectId> {
    let id = crate::card_def::card_id_by_name(PHYREXIAN_REVOKER)?;
    [PlayerId::P0, PlayerId::P1]
        .into_iter()
        .flat_map(|player| state.players[player.index()].battlefield.iter().copied())
        .find(|&object| {
            let live = state.objects.get(object);
            live.card_def == id
                && live.v4.face_index == 0
                && !read_state(state).is_some_and(|e7b| {
                    e7b.revoker_names
                        .iter()
                        .any(|(revoker, _)| *revoker == live_link(state, object))
                })
        })
}

/// "As this creature enters, choose a nonland card name": the pending
/// choice, as an option per visible name (the Revoker's own name when no
/// other is visible).
pub(crate) fn entry_choice_decision(state: &GameState) -> Option<crate::engine::Decision> {
    if let Some(decision) = super::dredge::pending_decision(state) {
        return Some(decision);
    }
    let revoker = revoker_needing_name(state)?;
    let player = state.objects.get(revoker).controller;
    Some(crate::engine::Decision::ChooseEffectOption {
        player,
        source: revoker,
        option_count: u16::try_from(revoker_name_options(state, player).len())
            .unwrap_or(u16::MAX)
            .max(1),
    })
}

/// Answers the pending dredge or Phyrexian Revoker choice. `Ok(false)` when
/// neither is pending.
pub(crate) fn answer_entry_choice(state: &mut GameState, option: u16) -> Result<bool, String> {
    if super::dredge::answer_pending_decision(state, option)? {
        return Ok(true);
    }
    let Some(revoker) = revoker_needing_name(state) else {
        return Ok(false);
    };
    let player = state.objects.get(revoker).controller;
    let options = revoker_name_options(state, player);
    let named = options
        .get(usize::from(option))
        .copied()
        .or_else(|| (option == 0).then(|| state.objects.get(revoker).card_def))
        .ok_or("card name option is out of range")?;
    let link = live_link(state, revoker);
    let names = &mut e7b_state(state).revoker_names;
    names.retain(|(host, _)| host.object != revoker);
    names.push((link, named));
    Ok(true)
}

/// The definition whose name a Phyrexian Revoker chose.
pub fn revoker_name(state: &GameState, revoker: ObjectId) -> Option<u16> {
    let link = live_link(state, revoker);
    read_state(state)?
        .revoker_names
        .iter()
        .find(|(host, _)| *host == link)
        .map(|(_, named)| *named)
}

// ---- Crucible of Worlds ---------------------------------------------------------------

/// Crucible of Worlds: "You may play lands from your graveyard." The land
/// cards `player` may play from their graveyard.
pub(crate) fn extra_land_candidates(state: &GameState, player: PlayerId) -> Vec<ObjectId> {
    if active_permanents(state, CRUCIBLE_OF_WORLDS, Some(player))
        .next()
        .is_none()
    {
        return Vec::new();
    }
    state.players[player.index()]
        .graveyard
        .iter()
        .copied()
        .filter(|&card| {
            let live = state.objects.get(card);
            live.owner == player
                && !live.v4.is_token
                && CARD_DEFS[live.card_def as usize].is_playable_land()
        })
        .collect()
}

// ---- Protection from multicolored -----------------------------------------------------

/// Stonecoil Serpent's "protection from multicolored".
pub(crate) fn has_protection_from_multicolored(state: &GameState, object: ObjectId) -> bool {
    active_named(state, object, STONECOIL_SERPENT)
}

/// Whether `source` is multicolored.
pub(crate) fn is_multicolored(state: &GameState, source: ObjectId) -> bool {
    super::color_count(state, source) >= 2
}

/// Whether `object` has protection from `source`'s colors (it can't be
/// targeted, blocked, enchanted or dealt damage by a multicolored source).
pub(crate) fn protected_from_source(state: &GameState, object: ObjectId, source: ObjectId) -> bool {
    has_protection_from_multicolored(state, object) && is_multicolored(state, source)
}

// ---- Regeneration -------------------------------------------------------------------

/// "Regenerate this creature" (701.15a): a shield that lasts this turn.
pub fn add_regeneration_shield(state: &mut GameState, creature: ObjectId) {
    let link = live_link(state, creature);
    let controller = state.objects.get(creature).controller;
    let turn = TurnCountV1::now(state, controller);
    e7b_state(state).shields.push((link, turn));
}

/// Regeneration's replacement (701.15b): "The next time this creature would
/// be destroyed this turn, instead tap it, remove all damage from it, and
/// remove it from combat." Uses one shield and returns `true` when
/// `creature` regenerated instead of being destroyed.
pub(crate) fn regenerate_instead(state: &mut GameState, creature: ObjectId) -> bool {
    let Some(live) = state.objects.try_get(creature) else {
        return false;
    };
    if live.zone != Zone::Battlefield {
        return false;
    }
    // Incinerate and Smother: "It can't be regenerated." (wave P1a)
    if crate::eternal_premodern_p1a_v1::statics::cant_be_regenerated(state, creature) {
        return false;
    }
    let link = live_link(state, creature);
    let Some(position) = read_state(state).and_then(|e7b| {
        e7b.shields
            .iter()
            .position(|(shielded, turn)| *shielded == link && turn.current(state))
    }) else {
        return false;
    };
    e7b_state(state).shields.remove(position);
    if !state.objects.get(creature).tapped {
        crate::event::propose_and_commit(state, crate::event::ProposedEvent::tap(creature));
    }
    let live = state.objects.get_mut(creature);
    live.damage = 0;
    live.v4.deathtouch_damage = false;
    state.engine.combat.remove_from_combat(creature);
    true
}

/// The lethal-damage state-based action (704.5g/h) destroys: a creature
/// with toughness above zero dying to damage regenerates instead when it
/// has a shield. `true` when it regenerated.
pub(crate) fn regenerate_from_damage(state: &mut GameState, creature: ObjectId) -> bool {
    if !state
        .objects
        .try_get(creature)
        .is_some_and(|live| live.zone == Zone::Battlefield)
        || !is_creature(state, creature)
        || crate::engine::effective_toughness(state, creature) <= 0
    {
        return false;
    }
    regenerate_instead(state, creature)
}

/// Golgari Grave-Troll: "This creature enters with a +1/+1 counter on it
/// for each creature card in your graveyard."
pub(crate) fn apply_entry_counters(
    state: &mut GameState,
    object: ObjectId,
    input: Option<&crate::doubling_season_v48::ReplacementInputV48>,
) {
    if definition_name(state, object) != Some(GOLGARI_GRAVE_TROLL) {
        return;
    }
    let live = state.objects.get(object);
    if live.zone != Zone::Battlefield || live.v4.face_index != 0 {
        return;
    }
    let controller = live.controller;
    let creatures = super::graveyard_cards(state, controller)
        .into_iter()
        .filter(|&card| definition(state, card).is_some_and(|def| def.has_type(CardType::Creature)))
        .count();
    let Ok(count) = i32::try_from(creatures) else {
        return;
    };
    if count == 0 {
        return;
    }
    match crate::doubling_season_v48::entry_amount(state, input, controller, count) {
        Ok(amount) => {
            let counters = &mut state.objects.get_mut(object).counters.plus1_plus1;
            *counters = counters.saturating_add(amount);
        }
        Err(_) => super::halt(state, object),
    }
}

// ---- Hollow One -----------------------------------------------------------------------

/// Records a card its owner discarded (or cycled, which discards it), in
/// games with a Hollow One. Called before the card leaves the hand.
pub(crate) fn note_discard(state: &mut GameState, card: ObjectId) {
    let Some(live) = state.objects.try_get(card) else {
        return;
    };
    if live.zone != Zone::Hand || !super::in_game(state, HOLLOW_ONE) {
        return;
    }
    let owner = live.owner;
    let turn = super::state_turn(state);
    super::bump(&mut e7b_state(state).discards, turn, owner);
}

/// Hollow One: "for each card you've cycled or discarded this turn".
pub(crate) fn cards_cycled_or_discarded_this_turn(state: &GameState, player: PlayerId) -> u32 {
    read_state(state).map_or(0, |e7b| {
        super::count_this_turn(state, &e7b.discards, player)
    })
}

// ---- Keywords, alternative costs ----------------------------------------------------

/// Dash: a dashed permanent has haste.
pub(crate) fn conditional_keywords(state: &GameState, object: ObjectId) -> Keywords {
    if super::dashed(state, object) {
        Keywords::HASTE
    } else {
        Keywords::NONE
    }
}

// ---- Avatar of Burgeoning Echoes -----------------------------------------------------

/// 'Planeswalkers you control have "[-10]: Put a +1/+1 counter on target
/// creature for each land you control."' The ability, if `object` is a
/// planeswalker whose controller controls an active Avatar. It is offered
/// one index past the planeswalker's printed abilities.
pub(crate) fn granted_avatar_ability(
    state: &GameState,
    object: ObjectId,
) -> Option<crate::card_def::ActivatedAbilityDef> {
    let live = state.objects.try_get(object)?;
    if live.zone != Zone::Battlefield
        || !crate::engine::object_has_type(state, object, CardType::Planeswalker)
        || active_permanents(state, AVATAR_OF_BURGEONING_ECHOES, Some(live.controller))
            .next()
            .is_none()
    {
        return None;
    }
    avatar_ability()
}

fn avatar_ability() -> Option<crate::card_def::ActivatedAbilityDef> {
    let avatar = crate::card_def::card_id_by_name(AVATAR_OF_BURGEONING_ECHOES)?;
    CARD_DEFS
        .get(usize::from(avatar))?
        .activated_abilities
        .first()
        .copied()
}

/// The granted ability of a stack item activated through Avatar of
/// Burgeoning Echoes: one index past a planeswalker host's printed
/// abilities. The ability exists on the stack whatever happened to the
/// Avatar since.
pub(crate) fn granted_avatar_ability_on_stack(
    host: AbilitySourceContractV4,
    ability_index: u8,
) -> Option<crate::card_def::ActivatedAbilityDef> {
    let def = CARD_DEFS.get(usize::from(host.card_def))?;
    if usize::from(ability_index) != def.activated_abilities.len()
        || !def.has_type(CardType::Planeswalker)
    {
        return None;
    }
    avatar_ability()
}

/// The Avatar's own copy of the ability is never offered: it is the
/// ability planeswalkers gain, not one the creature has.
pub(crate) fn activation_allowed(state: &GameState, source: ObjectId, name: &str) -> bool {
    name != AVATAR_OF_BURGEONING_ECHOES
        || crate::engine::object_has_type(state, source, CardType::Planeswalker)
}
