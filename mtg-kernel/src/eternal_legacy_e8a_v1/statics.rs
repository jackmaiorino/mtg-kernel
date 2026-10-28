//! Static abilities and engine hooks of the wave E8a cards: Harbinger of
//! the Seas' land type change, Back to Basics' untap prohibition, Teferi,
//! Time Raveler's timing rules, Dark Matter Manipulator's, Wight of the
//! Reliquary's and Lion Sash's power, Lion Sash's reconfigure, Wishclaw
//! Talisman's wish counters, Valgavoth's Lair's chosen color, Rain of
//! Filth's granted mana ability, Hogaak's graveyard cast and Skyclave
//! Apparition's Illusion tokens.

use super::{
    active_named, active_permanents, definition, definition_name, e8a_state, link_live_in,
    live_link, read_state, TurnV1, AETHER_VIAL, BACK_TO_BASICS, DARK_MATTER_MANIPULATOR,
    HARBINGER_OF_THE_SEAS, HOGAAK, ILLUSION_TOKEN, LION_SASH, RAIN_OF_FILTH, TEFERI,
    VALGAVOTHS_LAIR, WIGHT_OF_THE_RELIQUARY, WISHCLAW_TALISMAN,
};
use crate::card_def::{CardType, Subtype, Supertype, CARD_DEFS};
use crate::event::{self, ProposedEvent};
use crate::ids::{ObjectId, PlayerId};
use crate::mana::ManaColor;
use crate::state::{GameState, ObjectLinkV4, Zone};

/// The land types a nonbasic land loses under Harbinger of the Seas.
const LAND_TYPES: [Subtype; 18] = [
    Subtype::Plains,
    Subtype::Island,
    Subtype::Swamp,
    Subtype::Mountain,
    Subtype::Forest,
    Subtype::Desert,
    Subtype::Gate,
    Subtype::Cave,
    Subtype::Urzas,
    Subtype::Tower,
    Subtype::PowerPlant,
    Subtype::Mine,
    Subtype::Lair,
    Subtype::Locus,
    Subtype::Sphere,
    Subtype::Town,
    Subtype::Cloud,
    Subtype::Planet,
];

const COLORS: [ManaColor; 5] = [
    ManaColor::W,
    ManaColor::U,
    ManaColor::B,
    ManaColor::R,
    ManaColor::G,
];

/// Whether `object` is a nonbasic land on the battlefield, read from its
/// printed face (no effect in these decks changes a land's card types).
fn nonbasic_land(state: &GameState, object: ObjectId) -> bool {
    state.objects.try_get(object).is_some_and(|live| {
        live.zone == Zone::Battlefield
            && live.v4.face_down_v1.is_none()
            && CARD_DEFS
                .get(usize::from(live.card_def))
                .is_some_and(|def| {
                    def.types_for_face(live.v4.face_index)
                        .contains(&CardType::Land)
                        && !def.supertypes.contains(&Supertype::Basic)
                })
    })
}

// ---- Back to Basics -------------------------------------------------------------------

/// Back to Basics: "Nonbasic lands don't untap during their controllers'
/// untap steps."
pub(crate) fn prevents_untap_step(state: &GameState, object: ObjectId) -> bool {
    nonbasic_land(state, object)
        && active_permanents(state, BACK_TO_BASICS, None)
            .next()
            .is_some()
}

// ---- Harbinger of the Seas ------------------------------------------------------------

/// The timestamp of the latest Harbinger of the Seas whose ability applies,
/// if `object` is a nonbasic land it makes an Island.
fn harbinger_timestamp(state: &GameState, object: ObjectId) -> Option<u64> {
    let e8a = read_state(state)?;
    if e8a.harbingers.is_empty() || !nonbasic_land(state, object) {
        return None;
    }
    e8a.harbingers
        .iter()
        .filter(|(link, _)| {
            link_live_in(state, *link, Zone::Battlefield)
                && active_named(state, link.object, HARBINGER_OF_THE_SEAS)
        })
        .map(|(_, timestamp)| *timestamp)
        .max()
}

/// Harbinger of the Seas: "Nonbasic lands are Islands." (305.7: such a
/// land loses its land types and all abilities from its rules text and has
/// "{T}: Add {U}.") The removal's timestamp for a nonbasic land.
pub(crate) fn removal_timestamp(state: &GameState, object: ObjectId) -> Option<u64> {
    harbinger_timestamp(state, object)
}

/// Harbinger of the Seas: a nonbasic land's land types become Island.
/// Other subtypes (Dryad Arbor's Dryad) stay.
pub(crate) fn adjust_subtypes(state: &GameState, object: ObjectId, subtype_ids: &mut Vec<u16>) {
    if harbinger_timestamp(state, object).is_none() {
        return;
    }
    subtype_ids.retain(|&id| LAND_TYPES.iter().all(|land| land.stable_id() != id));
    subtype_ids.push(Subtype::Island.stable_id());
    subtype_ids.sort_unstable();
    subtype_ids.dedup();
}

/// Mana abilities this module grants: a nonbasic land under Harbinger of
/// the Seas has "{T}: Add {U}."; Valgavoth's Lair has "{T}: Add one mana of
/// the chosen color."
pub(crate) fn granted_mana_colors(state: &GameState, object: ObjectId) -> Vec<ManaColor> {
    if harbinger_timestamp(state, object).is_some() {
        return vec![ManaColor::U];
    }
    if active_named(state, object, VALGAVOTHS_LAIR) {
        if let Some(color) = state.objects.get(object).v4.chosen_color {
            return vec![color];
        }
    }
    Vec::new()
}

// ---- Valgavoth's Lair ---------------------------------------------------------------

/// The Valgavoth's Lair that entered without a chosen color.
fn lair_needing_color(state: &GameState) -> Option<ObjectId> {
    let id = crate::card_def::card_id_by_name(VALGAVOTHS_LAIR)?;
    [PlayerId::P0, PlayerId::P1]
        .into_iter()
        .flat_map(|player| state.players[player.index()].battlefield.iter().copied())
        .find(|&object| {
            let live = state.objects.get(object);
            live.card_def == id
                && live.zone == Zone::Battlefield
                && live.v4.face_index == 0
                && live.v4.chosen_color.is_none()
                && crate::continuous_characteristics_v1::printed_abilities_active(state, object)
        })
}

/// "As it enters, choose a color": the pending choice, one option per
/// color in WUBRG order.
pub(crate) fn entry_choice_decision(state: &GameState) -> Option<crate::engine::Decision> {
    let lair = lair_needing_color(state)?;
    Some(crate::engine::Decision::ChooseEffectOption {
        player: state.objects.get(lair).controller,
        source: lair,
        option_count: 5,
    })
}

/// Answers the pending Valgavoth's Lair color. `Ok(false)` when none is
/// pending.
pub(crate) fn answer_entry_choice(state: &mut GameState, option: u16) -> Result<bool, String> {
    let Some(lair) = lair_needing_color(state) else {
        return Ok(false);
    };
    let color = *COLORS
        .get(usize::from(option))
        .ok_or("Valgavoth's Lair color option is out of range")?;
    state.objects.get_mut(lair).v4.chosen_color = Some(color);
    Ok(true)
}

// ---- Power and toughness ---------------------------------------------------------------

/// Dark Matter Manipulator: "This creature gets +2/+0 for every seven cards
/// in your graveyard." Wight of the Reliquary: "This creature gets +1/+1
/// for each creature card in your graveyard." Lion Sash: "Equipped creature
/// gets +1/+1 for each +1/+1 counter on this Equipment."
pub(crate) fn controlled_boost(state: &GameState, object: ObjectId) -> (i128, i128) {
    let Some(live) = state.objects.try_get(object) else {
        return (0, 0);
    };
    let controller = live.controller;
    let mut boost = (0_i128, 0_i128);
    if active_named(state, object, DARK_MATTER_MANIPULATOR) {
        let cards = crate::effect::controller_graveyard_card_count(state, controller) as i128;
        boost.0 += 2 * (cards / 7);
    }
    if active_named(state, object, WIGHT_OF_THE_RELIQUARY) {
        let creatures = state.players[controller.index()]
            .graveyard
            .iter()
            .filter(|&&card| {
                let card_live = state.objects.get(card);
                card_live.owner == controller
                    && !card_live.v4.is_token
                    && definition(state, card).is_some_and(|def| def.has_type(CardType::Creature))
            })
            .count() as i128;
        boost.0 += creatures;
        boost.1 += creatures;
    }
    let host = live_link(state, object);
    for &attached in &live.attachments {
        let Some(sash) = state.objects.try_get(attached) else {
            continue;
        };
        if sash.v4.attached_to == Some(host) && active_named(state, attached, LION_SASH) {
            let counters = i128::from(sash.counters.plus1_plus1.max(0));
            boost.0 += counters;
            boost.1 += counters;
        }
    }
    boost
}

/// Lion Sash's reconfigure: "While attached, this isn't a creature."
pub(crate) fn has_type_override(
    state: &GameState,
    object: ObjectId,
    card_type: CardType,
) -> Option<bool> {
    if card_type != CardType::Creature {
        return None;
    }
    let live = state.objects.try_get(object)?;
    if live.zone != Zone::Battlefield || live.v4.attached_to.is_none() {
        return None;
    }
    (CARD_DEFS.get(usize::from(live.card_def))?.name == LION_SASH).then_some(false)
}

// ---- Activations ----------------------------------------------------------------------

/// The wish counters on `object`'s battlefield incarnation.
pub(crate) fn wish_counters(state: &GameState, object: ObjectId) -> u8 {
    let Some(live) = state.objects.try_get(object) else {
        return 0;
    };
    if live.zone != Zone::Battlefield {
        return 0;
    }
    let link = live_link(state, object);
    read_state(state).map_or(0, |e8a| {
        e8a.wish_counters
            .iter()
            .find(|(host, _)| *host == link)
            .map_or(0, |(_, count)| *count)
    })
}

/// Activation restrictions: Wishclaw Talisman's needs a wish counter to
/// remove and "Activate only during your turn"; Lion Sash unattaches only
/// while attached.
pub(crate) fn activation_allowed(
    state: &GameState,
    source: ObjectId,
    name: &str,
    ability_index: usize,
) -> bool {
    match (name, ability_index) {
        (WISHCLAW_TALISMAN, 0) => {
            wish_counters(state, source) > 0
                && state.active_player == state.objects.get(source).controller
        }
        (LION_SASH, 2) => state.objects.get(source).v4.attached_to.is_some(),
        // Rain of Filth's granted ability is offered only on lands.
        (RAIN_OF_FILTH, _) => false,
        _ => true,
    }
}

/// Costs paid as an ability is put on the stack: Wishclaw Talisman's
/// "Remove a wish counter from this artifact".
pub(crate) fn activation_paid(state: &mut GameState, source: ObjectId, ability_index: u8) {
    if ability_index != 0 || definition_name(state, source) != Some(WISHCLAW_TALISMAN) {
        return;
    }
    if state.objects.get(source).zone != Zone::Battlefield {
        return;
    }
    let link = live_link(state, source);
    if let Some(entry) = e8a_state(state)
        .wish_counters
        .iter_mut()
        .find(|(host, _)| *host == link)
    {
        entry.1 = entry.1.saturating_sub(1);
    }
}

/// "Enters with" counters: Wishclaw Talisman's three wish counters.
pub(crate) fn apply_entry_counters(state: &mut GameState, object: ObjectId) {
    if definition_name(state, object) != Some(WISHCLAW_TALISMAN) {
        return;
    }
    let live = state.objects.get(object);
    if live.zone != Zone::Battlefield || live.v4.face_index != 0 {
        return;
    }
    let link = live_link(state, object);
    let counters = &mut e8a_state(state).wish_counters;
    counters.retain(|(host, _)| host.object != object);
    counters.push((link, 3));
}

// ---- Teferi, Time Raveler -------------------------------------------------------------

/// Teferi, Time Raveler: "Each opponent can cast spells only any time they
/// could cast a sorcery." Whether `player` is so restricted.
pub(crate) fn sorcery_timing_only(state: &GameState, player: PlayerId) -> bool {
    active_permanents(state, TEFERI, Some(player.opponent()))
        .next()
        .is_some()
}

/// Teferi's +1: "Until your next turn, you may cast sorcery spells as
/// though they had flash."
pub(crate) fn sorceries_have_flash(state: &GameState, player: PlayerId) -> bool {
    read_state(state).is_some_and(|e8a| e8a.sorcery_flash.contains(&player))
}

// ---- Rain of Filth ----------------------------------------------------------------------

/// Whether `land` has the ability Rain of Filth granted this turn.
fn rain_granted(state: &GameState, land: ObjectId) -> bool {
    let Some(e8a) = read_state(state) else {
        return false;
    };
    if e8a.rain_lands.is_empty() {
        return false;
    }
    let Some(live) = state.objects.try_get(land) else {
        return false;
    };
    if live.zone != Zone::Battlefield {
        return false;
    }
    let link = live_link(state, land);
    let now = TurnV1::now(state);
    e8a.rain_lands
        .iter()
        .any(|(granted, turn)| *granted == link && *turn == now)
}

/// The ability Rain of Filth granted `land`: 'Sacrifice this land: Add
/// {B}.' It is Rain of Filth's own ability 0.
pub(crate) fn granted_rain_ability(
    state: &GameState,
    land: ObjectId,
) -> Option<crate::card_def::ActivatedAbilityDef> {
    if !rain_granted(state, land) {
        return None;
    }
    let rain = crate::card_def::card_id_by_name(RAIN_OF_FILTH)?;
    CARD_DEFS
        .get(usize::from(rain))?
        .activated_abilities
        .first()
        .copied()
}

/// The index the granted ability takes on `land`: one past its printed
/// abilities. A land that keeps another granted ability there (a Thespian's
/// Stage copy) is not offered this one.
pub(crate) fn rain_ability_index(state: &GameState, land: ObjectId) -> Option<u8> {
    granted_rain_ability(state, land)?;
    if crate::eternal_legacy_e7a_v1::retained_stage_ability(state, land).is_some() {
        return None;
    }
    let def = definition(state, land)?;
    u8::try_from(def.activated_abilities.len()).ok()
}

/// Whether activating `source`'s ability `index` is Rain of Filth's granted
/// mana ability, which the engine performs without the stack.
pub(crate) fn is_special_activation(state: &GameState, source: ObjectId, index: u8) -> bool {
    rain_ability_index(state, source) == Some(index)
}

/// Performs Rain of Filth's granted ability: `player` sacrifices `land` and
/// adds {B}.
pub(crate) fn special_activation(
    state: &mut GameState,
    player: PlayerId,
    land: ObjectId,
) -> Result<(), String> {
    let live = state.objects.get(land);
    if live.zone != Zone::Battlefield || live.controller != player || !rain_granted(state, land) {
        return Err("Rain of Filth's ability is not available on that land".to_string());
    }
    event::log_sacrifice(state, land);
    event::propose_and_commit(state, ProposedEvent::zone_change(land, Zone::Graveyard));
    event::propose_and_commit(state, ProposedEvent::mana_add(player, vec![ManaColor::B]));
    Ok(())
}

// ---- Hogaak, Arisen Necropolis -----------------------------------------------------------

/// Hogaak: "You may cast this card from your graveyard." `zone_change_count`
/// is the graveyard incarnation; the permission still covers that card once
/// it moved to the stack to be cast.
pub(crate) fn hogaak_permission(
    state: &GameState,
    object: ObjectId,
    holder: PlayerId,
    zone_change_count: u32,
) -> bool {
    state.objects.try_get(object).is_some_and(|live| {
        ((live.zone == Zone::Graveyard && live.zone_change_count == zone_change_count)
            || (live.zone == Zone::Stack
                && zone_change_count.checked_add(1) == Some(live.zone_change_count)))
            && live.owner == holder
            && !live.v4.is_token
            && live.spell_copy_origin.is_none()
            && CARD_DEFS
                .get(usize::from(live.card_def))
                .is_some_and(|def| def.name == HOGAAK && def.is_executable())
    })
}

/// Hogaak: "You can't spend mana to cast this spell." Whether the card
/// named `name` is cast only by convoke and delve.
pub(crate) fn no_mana_spell(name: &str) -> bool {
    name == HOGAAK
}

// ---- Skyclave Apparition's Illusions ------------------------------------------------------

/// An Illusion token's power and toughness: the mana value of the card
/// Skyclave Apparition exiled.
pub(crate) fn characteristic_defining_power(state: &GameState, object: ObjectId) -> Option<i32> {
    let live = state.objects.try_get(object)?;
    if live.zone != Zone::Battlefield || definition_name(state, object) != Some(ILLUSION_TOKEN) {
        return None;
    }
    let link = ObjectLinkV4 {
        object,
        zone_change_count: live.zone_change_count,
    };
    Some(read_state(state).map_or(0, |e8a| {
        e8a.illusions
            .iter()
            .find(|(token, _)| *token == link)
            .map_or(0, |(_, size)| i32::from(*size))
    }))
}

// ---- Aether Vial --------------------------------------------------------------------------

/// The charge counters on Aether Vial's ability source: the live count, or
/// the count it had as it left the battlefield.
pub(crate) fn vial_charge(state: &GameState, source: crate::state::AbilitySourceContractV4) -> i32 {
    match state.objects.try_get(source.source) {
        Some(live)
            if live.zone == Zone::Battlefield
                && live.zone_change_count == source.zone_change_count =>
        {
            live.counters.charge
        }
        _ => state
            .counter_lki_for(source.source, source.zone_change_count)
            .map_or(0, |counters| counters.charge),
    }
}

/// Whether `name` is Aether Vial, whose ability reads the charge counters it
/// had as it left the battlefield.
pub(crate) fn reads_counter_lki(name: &str) -> bool {
    name == AETHER_VIAL
}

// ---- Hogaak's payment ---------------------------------------------------------------------

/// The colors a mana symbol accepts, or `None` for a symbol that only mana
/// or life can pay.
fn pip_colors(pip: crate::mana::Pip) -> Option<Vec<ManaColor>> {
    match pip {
        crate::mana::Pip::Colored(color) => Some(vec![color]),
        crate::mana::Pip::Hybrid(first, second) => Some(vec![first, second]),
        crate::mana::Pip::Phyrexian(_) | crate::mana::Pip::PhyrexianAnyColor(_) => None,
    }
}

/// Assigns each of `pips` to a distinct creature of `creatures` with one of
/// its colors, and returns the creatures used, in pip order.
fn assign_pips(
    state: &GameState,
    pips: &[Vec<ManaColor>],
    creatures: &[ObjectId],
    used: &mut [bool],
) -> Option<Vec<ObjectId>> {
    let Some((colors, rest)) = pips.split_first() else {
        return Some(Vec::new());
    };
    for (index, &creature) in creatures.iter().enumerate() {
        if used[index] {
            continue;
        }
        let mask = crate::engine::object_color_mask(state, creature);
        if !colors
            .iter()
            .any(|&color| mask & crate::card_def::mana_color_mask(color) != 0)
        {
            continue;
        }
        used[index] = true;
        if let Some(mut assigned) = assign_pips(state, rest, creatures, used) {
            assigned.insert(0, creature);
            return Some(assigned);
        }
        used[index] = false;
    }
    None
}

/// Hogaak, Arisen Necropolis: "You can't spend mana to cast this spell.
/// Convoke, delve." Pays `pips` and `generic` with untapped creatures
/// `player` controls (each pays one colored symbol of its color or one
/// generic) and, for the generic rest, cards exiled from their graveyard,
/// oldest first. `chosen` are the creatures the player chose to tap; with
/// none chosen yet, any creatures may be used (an affordability check).
/// `excluded` objects neither convoke nor delve (the spell itself). Returns
/// the creatures to tap and the graveyard cards to exile.
pub(crate) fn hogaak_payment(
    pips: &[crate::mana::Pip],
    generic: u32,
    player: PlayerId,
    state: &GameState,
    excluded: &[ObjectId],
    chosen: Option<&[ObjectId]>,
) -> Option<(Vec<ObjectId>, Vec<ObjectId>)> {
    let pips = pips
        .iter()
        .map(|&pip| pip_colors(pip))
        .collect::<Option<Vec<_>>>()?;
    let eligible = |object: ObjectId| {
        let live = state.objects.get(object);
        live.zone == Zone::Battlefield
            && live.controller == player
            && !live.tapped
            && !excluded.contains(&object)
            && crate::engine::object_has_type(state, object, CardType::Creature)
    };
    let creatures = match chosen {
        Some(chosen) => {
            if chosen
                .iter()
                .enumerate()
                .any(|(index, &object)| !eligible(object) || chosen[..index].contains(&object))
            {
                return None;
            }
            chosen.to_vec()
        }
        None => state.players[player.index()]
            .battlefield
            .iter()
            .copied()
            .filter(|&object| eligible(object))
            .collect(),
    };
    let mut used = vec![false; creatures.len()];
    let colored = assign_pips(state, &pips, &creatures, &mut used)?;
    let spare = creatures
        .iter()
        .zip(&used)
        .filter_map(|(&creature, &taken)| (!taken).then_some(creature))
        .collect::<Vec<_>>();
    let graveyard = state.players[player.index()]
        .graveyard
        .iter()
        .copied()
        .filter(|&card| {
            let live = state.objects.get(card);
            live.owner == player && !live.v4.is_token && !excluded.contains(&card)
        })
        .collect::<Vec<_>>();
    let generic = usize::try_from(generic).ok()?;
    let (convokers, delved) = if chosen.is_some() {
        // Every chosen creature pays: what they leave is delved.
        if spare.len() > generic {
            return None;
        }
        (spare.len(), generic - spare.len())
    } else {
        // Delve first, then tap creatures for what remains.
        let delved = generic.min(graveyard.len());
        (generic - delved, delved)
    };
    if convokers > spare.len() || delved > graveyard.len() {
        return None;
    }
    let mut tapped = colored;
    tapped.extend(spare.into_iter().take(convokers));
    Some((tapped, graveyard[..delved].to_vec()))
}
