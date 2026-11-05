//! Static abilities and engine hooks of the wave E9a cards: Painter's
//! Servant's color, Blood Moon and Magus of the Moon, Anger's haste,
//! Ensnaring Bridge, Orim's Chant, split second, Goblin Rabblemaster's attack
//! requirement, Sundering Eruption's block restriction, activation
//! restrictions (threshold, upkeep, boast), Simian Spirit Guide's mana,
//! Eumidian Hatchery's counters, Knight of the Reliquary's and Elvish
//! Reclaimer's power, Sejiri Steppe's protection, Cephalid Illusionist's
//! prevention, Nomads en-Kor's redirection, Echoing Deeps' entry copy and
//! Ox of Agonas' escape counter.

use super::{
    active_named, active_permanents, battlefield, definition, definition_name, e9a_state,
    link_live_in, live_link, read_state, TurnV1, ANGER, BLOOD_MOON, BROADSIDE_BOMBARDIERS,
    CEPHALID_COLISEUM, ECHOING_DEEPS, ELVISH_RECLAIMER, ENSNARING_BRIDGE, EUMIDIAN_HATCHERY,
    GOBLIN_RABBLEMASTER, KNIGHT_OF_THE_RELIQUARY, LEGOLAS, MAGUS_OF_THE_MOON, PAINTERS_SERVANT,
    SIMIAN_SPIRIT_GUIDE, TOLARIA,
};
use crate::card_def::{CardType, CombatStaticRulesDef, Keywords, Subtype, Supertype, CARD_DEFS};
use crate::event::{self, CommittedEvent, DamageProposed, ProposedEvent};
use crate::ids::{ObjectId, PlayerId};
use crate::mana::ManaColor;
use crate::state::{GameState, ObjectLinkV4, StackItemKind, Step, Target, Zone};

/// The five colors in WUBRG order.
pub(crate) const COLORS: [ManaColor; 5] = [
    ManaColor::W,
    ManaColor::U,
    ManaColor::B,
    ManaColor::R,
    ManaColor::G,
];

/// The color mask of the five colors.
pub(crate) const WUBRG: u8 = 0x1f;

// ---- Painter's Servant ----------------------------------------------------------------

/// Painter's Servant: "All cards that aren't on the battlefield, spells, and
/// permanents are the chosen color in addition to their other colors." The
/// colors every Painter's Servant whose ability applies adds, as a mask.
pub(crate) fn painter_mask(state: &GameState) -> u8 {
    if !cfg!(feature = "eternal-fixtures") {
        return 0;
    }
    let Some(id) = painter_id() else {
        return 0;
    };
    let mut mask = 0;
    for player in [PlayerId::P0, PlayerId::P1] {
        for &object in &state.players[player.index()].battlefield {
            let live = state.objects.get(object);
            if live.card_def != id || !active_named(state, object, PAINTERS_SERVANT) {
                continue;
            }
            if let Some(color) = live.v4.chosen_color {
                mask |= crate::card_def::mana_color_mask(color);
            }
        }
    }
    mask
}

/// Painter's Servant's definition id.
fn painter_id() -> Option<u16> {
    static PAINTER_ID: std::sync::OnceLock<Option<u16>> = std::sync::OnceLock::new();
    *PAINTER_ID.get_or_init(|| crate::card_def::card_id_by_name(PAINTERS_SERVANT))
}

/// The colors `object` has, Painter's Servant included.
pub(crate) fn color_mask(state: &GameState, object: ObjectId) -> u8 {
    crate::engine::object_color_mask(state, object)
}

/// Whether `object` (a card in any zone, a spell or a permanent) is `color`.
pub(crate) fn card_has_color(state: &GameState, object: ObjectId, color: ManaColor) -> bool {
    color_mask(state, object) & crate::card_def::mana_color_mask(color) != 0
}

/// The Painter's Servant that entered without a chosen color.
fn painter_needing_color(state: &GameState) -> Option<ObjectId> {
    let id = painter_id()?;
    battlefield(state).find(|&object| {
        let live = state.objects.get(object);
        live.card_def == id
            && live.v4.face_index == 0
            && live.v4.chosen_color.is_none()
            && crate::continuous_characteristics_v1::printed_abilities_active(state, object)
    })
}

// ---- Blood Moon and Magus of the Moon --------------------------------------------------

/// Whether `object` is a nonbasic land on the battlefield, read from its
/// printed face.
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

/// Blood Moon and Magus of the Moon: "Nonbasic lands are Mountains." The
/// timestamp of the latest moon whose ability applies, if `object` is a
/// nonbasic land. Wave E8a's Harbinger of the Seas makes them Islands; the
/// later of the two effects wins (`eternal_legacy_e8a_v1::statics`).
pub(crate) fn moon_timestamp(state: &GameState, object: ObjectId) -> Option<u64> {
    let e9a = read_state(state)?;
    if e9a.moons.is_empty() || !nonbasic_land(state, object) {
        return None;
    }
    e9a.moons
        .iter()
        .filter(|(link, _)| {
            link_live_in(state, *link, Zone::Battlefield)
                && (active_named(state, link.object, BLOOD_MOON)
                    || active_named(state, link.object, MAGUS_OF_THE_MOON))
        })
        .map(|(_, timestamp)| *timestamp)
        .max()
}

// ---- Keywords -----------------------------------------------------------------------

/// Anger: "As long as this card is in your graveyard and you control a
/// Mountain, creatures you control have haste." Whether `object` gains
/// haste from an Anger.
pub(crate) fn grants_haste(state: &GameState, object: ObjectId) -> bool {
    static ANGER_ID: std::sync::OnceLock<Option<u16>> = std::sync::OnceLock::new();
    let Some(anger) = *ANGER_ID.get_or_init(|| crate::card_def::card_id_by_name(ANGER)) else {
        return false;
    };
    let Some(live) = state.objects.try_get(object) else {
        return false;
    };
    if live.zone != Zone::Battlefield {
        return false;
    }
    let controller = live.controller;
    let in_graveyard = state.players[controller.index()]
        .graveyard
        .iter()
        .any(|&card| {
            let card_live = state.objects.get(card);
            card_live.card_def == anger && card_live.owner == controller && !card_live.v4.is_token
        });
    in_graveyard
        && crate::engine::object_has_type(state, object, CardType::Creature)
        && battlefield(state).any(|permanent| {
            state.objects.get(permanent).controller == controller
                && crate::engine::object_has_type(state, permanent, CardType::Land)
                && crate::engine::has_effective_subtype(state, permanent, Subtype::Mountain)
        })
}

/// The keywords this module grants `object`.
pub(crate) fn granted_keywords(state: &GameState, object: ObjectId) -> Keywords {
    // Goblin Warchief's haste and Mystic Enforcer's flying (wave P1a).
    let premodern = crate::eternal_premodern_p1a_v1::statics::granted_keywords(state, object);
    if grants_haste(state, object) {
        Keywords::HASTE | premodern
    } else {
        premodern
    }
}

// ---- Combat -------------------------------------------------------------------------

/// Ensnaring Bridge: "Creatures with power greater than the number of cards
/// in your hand can't attack." Orim's Chant, kicked: "creatures can't attack
/// this turn."
pub(crate) fn cant_attack(state: &GameState, object: ObjectId) -> bool {
    let now = TurnV1::now(state);
    if read_state(state).is_some_and(|e9a| e9a.attack_bans.contains(&now)) {
        return true;
    }
    let power = crate::engine::effective_power(state, object);
    active_permanents(state, ENSNARING_BRIDGE, None).any(|bridge| {
        let controller = state.objects.get(bridge).controller;
        let hand = i32::try_from(state.players[controller.index()].hand.len()).unwrap_or(i32::MAX);
        power > hand
    })
}

/// Goblin Rabblemaster: "Other Goblin creatures you control attack each
/// combat if able." Whether `object` must attack because of one.
pub(crate) fn must_attack(state: &GameState, object: ObjectId) -> bool {
    let Some(live) = state.objects.try_get(object) else {
        return false;
    };
    live.zone == Zone::Battlefield
        && crate::engine::object_has_type(state, object, CardType::Creature)
        && crate::engine::has_effective_subtype(state, object, Subtype::Goblin)
        && active_permanents(state, GOBLIN_RABBLEMASTER, Some(live.controller))
            .any(|rabblemaster| rabblemaster != object)
}

/// The combat rules of `object` with Goblin Rabblemaster's requirement
/// added to `printed`, or `None` when no Rabblemaster forces it.
pub(crate) fn combat_static_rules(
    state: &GameState,
    object: ObjectId,
    printed: CombatStaticRulesDef,
) -> Option<CombatStaticRulesDef> {
    must_attack(state, object).then_some(CombatStaticRulesDef {
        must_attack_each_combat: true,
        ..printed
    })
}

/// Sundering Eruption: "Creatures without flying can't block this turn."
pub(crate) fn blocker_allowed(state: &GameState, blocker: ObjectId) -> bool {
    let now = TurnV1::now(state);
    !read_state(state).is_some_and(|e9a| e9a.block_bans.contains(&now))
        || crate::engine::has_effective_keyword(state, blocker, Keywords::FLYING)
}

// ---- Casting and activating -------------------------------------------------------------

/// Split second (702.61a): "As long as this spell is on the stack, players
/// can't cast spells or activate abilities that aren't mana abilities."
pub(crate) fn split_second_active(state: &GameState) -> bool {
    static LEGOLAS_ID: std::sync::OnceLock<Option<u16>> = std::sync::OnceLock::new();
    let Some(legolas) = *LEGOLAS_ID.get_or_init(|| crate::card_def::card_id_by_name(LEGOLAS))
    else {
        return false;
    };
    state.stack.iter().any(|item| {
        item.kind == StackItemKind::Spell
            && state
                .objects
                .try_get(item.source)
                .is_some_and(|live| live.zone == Zone::Stack && live.card_def == legolas)
    })
}

/// Orim's Chant: "Target player can't cast spells this turn." and split
/// second. Whether `player` can't cast spells now.
pub(crate) fn cast_prohibited(state: &GameState, player: PlayerId) -> bool {
    let now = TurnV1::now(state);
    read_state(state).is_some_and(|e9a| e9a.cast_bans.contains(&(player, now)))
        || split_second_active(state)
}

/// Whether activating `source`'s ability `index` is a special action or a
/// mana ability the engine performs without the stack, which split second
/// allows.
fn performed_without_stack(state: &GameState, source: ObjectId, index: u8) -> bool {
    index == 255
        || crate::eternal_lurrus_v1::is_companion_source(state, source)
        || crate::eternal_pauper_e6a_v1::is_foretell_action(state, source, index)
        || crate::eternal_legacy_e8a_v1::statics::is_special_activation(state, source, index)
        || crate::eternal_legacy_e7a_v1::is_special_activation(state, source, index)
}

/// Split second: whether it keeps `source`'s ability `index` from being
/// activated now.
pub(crate) fn split_second_blocks(state: &GameState, source: ObjectId, index: u8) -> bool {
    split_second_active(state) && !performed_without_stack(state, source, index)
}

/// Activation restrictions: Cephalid Coliseum's threshold ("Activate only if
/// there are seven or more cards in your graveyard."), Tolaria's "Activate
/// only during any upkeep step." and Broadside Bombardiers' boast
/// ("Activate only if this creature attacked this turn").
pub(crate) fn activation_allowed(
    state: &GameState,
    source: ObjectId,
    name: &str,
    ability_index: usize,
) -> bool {
    match (name, ability_index) {
        (CEPHALID_COLISEUM, 0) => {
            let controller = state.objects.get(source).controller;
            crate::effect::controller_graveyard_card_count(state, controller) >= 7
        }
        (TOLARIA, 0) => state.step == Step::Upkeep,
        (BROADSIDE_BOMBARDIERS, 0) => attacked_this_turn(state, source),
        // Barbarian Ring and Weathered Wayfarer (wave P1a).
        _ => crate::eternal_premodern_p1a_v1::statics::activation_allowed(
            state,
            source,
            name,
            ability_index,
        ),
    }
}

/// Whether `object`'s current incarnation attacked this turn.
fn attacked_this_turn(state: &GameState, object: ObjectId) -> bool {
    let link = live_link(state, object);
    state
        .creatures_attacked_turn_v1
        .as_ref()
        .is_some_and(|(turn, active, objects)| {
            *turn == state.turn_key_v1()
                && *active == state.active_player
                && objects.contains(&link)
        })
}

// ---- Simian Spirit Guide ------------------------------------------------------------

/// Whether activating `source`'s ability `index` is Simian Spirit Guide's
/// mana ability, which the engine performs without the stack.
pub(crate) fn is_special_activation(state: &GameState, source: ObjectId, index: u8) -> bool {
    index == 0
        && state
            .objects
            .try_get(source)
            .is_some_and(|live| live.zone == Zone::Hand)
        && definition_name(state, source) == Some(SIMIAN_SPIRIT_GUIDE)
}

/// Simian Spirit Guide: "Exile this card from your hand: Add {R}." A mana
/// ability (605.1a), so it never uses the stack.
pub(crate) fn special_activation(
    state: &mut GameState,
    player: PlayerId,
    source: ObjectId,
) -> Result<(), String> {
    if !is_special_activation(state, source, 0)
        || !state.players[player.index()].hand.contains(&source)
    {
        return Err("not a special activation".to_string());
    }
    event::propose_and_commit(state, ProposedEvent::zone_change(source, Zone::Exile));
    if state.objects.get(source).zone != Zone::Exile {
        return Err("Simian Spirit Guide stayed in its owner's hand".to_string());
    }
    event::propose_and_commit(state, ProposedEvent::mana_add(player, vec![ManaColor::R]));
    Ok(())
}

// ---- Eumidian Hatchery ----------------------------------------------------------------

/// Eumidian Hatchery: "Put a hatchling counter on this land." Called after
/// `land` was tapped for mana.
pub(crate) fn after_tapped_for_mana(state: &mut GameState, land: ObjectId) {
    // Gemstone Mine's mining counters (wave P1a).
    crate::eternal_premodern_p1a_v1::statics::after_tapped_for_mana(state, land);
    if definition_name(state, land) != Some(EUMIDIAN_HATCHERY)
        || state.objects.get(land).zone != Zone::Battlefield
        || !active_named(state, land, EUMIDIAN_HATCHERY)
    {
        return;
    }
    let link = live_link(state, land);
    let counters = &mut e9a_state(state).hatchlings;
    match counters.iter_mut().find(|(host, _)| *host == link) {
        Some(entry) => entry.1 = entry.1.saturating_add(1),
        None => counters.push((link, 1)),
    }
}

/// The hatchling counters on `object`'s battlefield incarnation.
pub fn hatchling_counters(state: &GameState, object: ObjectId) -> u16 {
    let Some(live) = state.objects.try_get(object) else {
        return 0;
    };
    if live.zone != Zone::Battlefield {
        return 0;
    }
    let link = live_link(state, object);
    read_state(state).map_or(0, |e9a| {
        e9a.hatchlings
            .iter()
            .find(|(host, _)| *host == link)
            .map_or(0, |(_, count)| *count)
    })
}

// ---- Power and toughness ---------------------------------------------------------------

/// The land cards in `player`'s graveyard.
fn graveyard_lands(state: &GameState, player: PlayerId) -> i128 {
    state.players[player.index()]
        .graveyard
        .iter()
        .filter(|&&card| {
            let live = state.objects.get(card);
            live.owner == player
                && !live.v4.is_token
                && definition(state, card).is_some_and(|def| def.has_type(CardType::Land))
        })
        .count() as i128
}

/// Knight of the Reliquary: "This creature gets +1/+1 for each land card in
/// your graveyard." Elvish Reclaimer: "This creature gets +2/+2 as long as
/// there are three or more land cards in your graveyard."
pub(crate) fn controlled_boost(state: &GameState, object: ObjectId) -> (i128, i128) {
    let Some(live) = state.objects.try_get(object) else {
        return (0, 0);
    };
    let controller = live.controller;
    // Mystic Enforcer's threshold (wave P1a).
    let mut boost = crate::eternal_premodern_p1a_v1::statics::controlled_boost(state, object);
    if active_named(state, object, KNIGHT_OF_THE_RELIQUARY) {
        let lands = graveyard_lands(state, controller);
        boost.0 += lands;
        boost.1 += lands;
    }
    if active_named(state, object, ELVISH_RECLAIMER) && graveyard_lands(state, controller) >= 3 {
        boost.0 += 2;
        boost.1 += 2;
    }
    boost
}

// ---- Protection, prevention and redirection ----------------------------------------------

/// Sejiri Steppe: the colors `host` has protection from until end of turn,
/// as a mask.
pub(crate) fn protection_mask(state: &GameState, host: ObjectId) -> u8 {
    // Printed protection from a color (wave P1a).
    let printed = crate::eternal_premodern_p1a_v1::statics::protection_mask(state, host);
    printed | granted_protection_mask(state, host)
}

/// Sejiri Steppe's protection until end of turn, as a mask.
fn granted_protection_mask(state: &GameState, host: ObjectId) -> u8 {
    let Some(e9a) = read_state(state) else {
        return 0;
    };
    if e9a.protections.is_empty() {
        return 0;
    }
    let Some(live) = state.objects.try_get(host) else {
        return 0;
    };
    let now = TurnV1::now(state);
    e9a.protections
        .iter()
        .filter(|(link, _, turn)| {
            link.object == host && link.zone_change_count == live.zone_change_count && *turn == now
        })
        .fold(0, |mask, (_, color, _)| {
            mask | crate::card_def::mana_color_mask(*color)
        })
}

/// Cephalid Illusionist: whether `damage` is combat damage dealt to or by a
/// creature whose combat damage is prevented this turn.
pub(crate) fn prevents_damage(state: &GameState, damage: &DamageProposed) -> bool {
    if !damage.is_combat {
        return false;
    }
    let Some(e9a) = read_state(state) else {
        return false;
    };
    if e9a.prevented.is_empty() {
        return false;
    }
    let now = TurnV1::now(state);
    let prevented = |object: ObjectId| {
        state.objects.try_get(object).is_some_and(|live| {
            e9a.prevented.iter().any(|(link, turn)| {
                link.object == object
                    && link.zone_change_count == live.zone_change_count
                    && *turn == now
            })
        })
    };
    prevented(damage.source) || matches!(damage.target, Target::Object(object) if prevented(object))
}

/// Nomads en-Kor: "The next 1 damage that would be dealt to this creature
/// this turn is dealt to target creature you control instead." Applies the
/// oldest shield of the damaged creature whose target is still on the
/// battlefield: one point goes to that creature, the rest stays.
pub(crate) fn redirect_damage(state: &mut GameState, damage: &mut DamageProposed) {
    if damage.amount <= 0 {
        return;
    }
    let Target::Object(damaged) = damage.target else {
        return;
    };
    let Some(e9a) = read_state(state) else {
        return;
    };
    if e9a.redirects.is_empty() {
        return;
    }
    let Some(live) = state.objects.try_get(damaged) else {
        return;
    };
    if live.zone != Zone::Battlefield {
        return;
    }
    let from = ObjectLinkV4 {
        object: damaged,
        zone_change_count: live.zone_change_count,
    };
    let now = TurnV1::now(state);
    let Some(position) = e9a.redirects.iter().position(|record| {
        record.from == from
            && record.turn == now
            && link_live_in(state, record.to, Zone::Battlefield)
            && crate::engine::object_has_type(state, record.to.object, CardType::Creature)
    }) else {
        return;
    };
    let record = e9a_state(state).redirects.remove(position);
    if record.to.object == damaged {
        return;
    }
    if damage.amount == 1 {
        damage.target = Target::Object(record.to.object);
        return;
    }
    damage.amount -= 1;
    let mut redirected = damage.clone();
    redirected.target = Target::Object(record.to.object);
    redirected.amount = 1;
    event::propose_and_commit(state, ProposedEvent::Damage(redirected));
}

// ---- Echoing Deeps -------------------------------------------------------------------

/// The distinct land cards in graveyards an Echoing Deeps may copy, in
/// graveyard order (the first player's first).
fn deeps_options(state: &GameState) -> Vec<u16> {
    let mut options: Vec<u16> = Vec::new();
    for player in [PlayerId::P0, PlayerId::P1] {
        for &card in &state.players[player.index()].graveyard {
            let live = state.objects.get(card);
            if live.v4.is_token || live.spell_copy_origin.is_some() {
                continue;
            }
            let Some(def) = CARD_DEFS.get(usize::from(live.card_def)) else {
                continue;
            };
            if def.has_type(CardType::Land)
                && def.is_executable()
                && !options.contains(&live.card_def)
            {
                options.push(live.card_def);
            }
        }
    }
    options
}

/// The Echoing Deeps that just entered and still owes its entry choice: it
/// entered since the last trigger collection, and a graveyard holds a land
/// card it may copy.
fn deeps_needing_choice(state: &GameState) -> Option<ObjectId> {
    static DEEPS_ID: std::sync::OnceLock<Option<u16>> = std::sync::OnceLock::new();
    let id = (*DEEPS_ID.get_or_init(|| crate::card_def::card_id_by_name(ECHOING_DEEPS)))?;
    let answered = read_state(state).map(|e9a| e9a.deeps_answered.as_slice());
    let entering = state.engine.event_log.iter().find_map(|event| {
        let CommittedEvent::ZoneChange {
            object,
            to: Zone::Battlefield,
            ..
        } = *event
        else {
            return None;
        };
        let live = state.objects.try_get(object)?;
        (live.zone == Zone::Battlefield
            && live.card_def == id
            && live.v4.face_index == 0
            && !answered.is_some_and(|answered| answered.contains(&live_link(state, object))))
        .then_some(object)
    })?;
    (!deeps_options(state).is_empty()).then_some(entering)
}

/// Whether trigger collection waits for an Echoing Deeps' entry choice, so
/// the triggers of the land it copies see it enter.
pub(crate) fn defer_triggers(state: &GameState) -> bool {
    deeps_needing_choice(state).is_some()
}

// ---- As-enters choices -----------------------------------------------------------------

/// The pending as-enters choice of this module's cards: Echoing Deeps'
/// copy (option 0 declines, option `i` copies the `i`th land card) or
/// Painter's Servant's color (WUBRG order).
pub(crate) fn entry_choice_decision(state: &GameState) -> Option<crate::engine::Decision> {
    if let Some(deeps) = deeps_needing_choice(state) {
        let options = deeps_options(state).len();
        return Some(crate::engine::Decision::ChooseEffectOption {
            player: state.objects.get(deeps).controller,
            source: deeps,
            option_count: u16::try_from(options + 1).unwrap_or(u16::MAX),
        });
    }
    let painter = painter_needing_color(state)?;
    Some(crate::engine::Decision::ChooseEffectOption {
        player: state.objects.get(painter).controller,
        source: painter,
        option_count: 5,
    })
}

/// Answers this module's pending as-enters choice. `Ok(false)` when none
/// is pending.
pub(crate) fn answer_entry_choice(state: &mut GameState, option: u16) -> Result<bool, String> {
    if let Some(deeps) = deeps_needing_choice(state) {
        let options = deeps_options(state);
        let copied = match option {
            0 => None,
            index => Some(
                *options
                    .get(usize::from(index) - 1)
                    .ok_or("Echoing Deeps' copy option is out of range")?,
            ),
        };
        let link = live_link(state, deeps);
        e9a_state(state).deeps_answered.push(link);
        if let Some(copied) = copied {
            enter_as_copy(state, deeps, copied);
        }
        return Ok(true);
    }
    let Some(painter) = painter_needing_color(state) else {
        return Ok(false);
    };
    let color = *COLORS
        .get(usize::from(option))
        .ok_or("Painter's Servant color option is out of range")?;
    state.objects.get_mut(painter).v4.chosen_color = Some(color);
    Ok(true)
}

/// Echoing Deeps: "You may have this land enter tapped as a copy of any land
/// card in a graveyard, except it's a Cave in addition to its other types."
/// Wave E7a's copy swaps the definition until it leaves; the copy enters
/// tapped and with the copied land's "enters with" counters.
fn enter_as_copy(state: &mut GameState, deeps: ObjectId, copied: u16) {
    crate::eternal_legacy_e7a_v1::become_copy(state, deeps, copied, 0);
    let cave = Subtype::Cave.stable_id();
    let live = state.objects.get_mut(deeps);
    if !live.v4.effective_subtype_ids.contains(&cave) {
        live.v4.effective_subtype_ids.push(cave);
        live.v4.effective_subtype_ids.sort_unstable();
    }
    live.tapped = true;
    crate::eternal_cards_v1::apply_entry_counters(state, deeps, None);
}

// ---- Ox of Agonas ---------------------------------------------------------------------

/// Ox of Agonas: "This creature escapes with a +1/+1 counter on it."
pub(crate) fn apply_entry_counters(
    state: &mut GameState,
    object: ObjectId,
    input: Option<&crate::doubling_season_v48::ReplacementInputV48>,
) {
    let live = state.objects.get(object);
    if live.zone != Zone::Battlefield || live.v4.face_index != 0 {
        return;
    }
    let (controller, zone_change_count) = (live.controller, live.zone_change_count);
    let escaped = read_state(state).is_some_and(|e9a| {
        e9a.escape_casts.iter().any(|cast| {
            cast.object == object
                && cast.zone_change_count.checked_add(1) == Some(zone_change_count)
        })
    });
    if !escaped {
        return;
    }
    e9a_state(state)
        .escape_casts
        .retain(|cast| cast.object != object);
    match crate::doubling_season_v48::entry_amount(state, input, controller, 1) {
        Ok(amount) => {
            let counters = &mut state.objects.get_mut(object).counters.plus1_plus1;
            *counters = counters.saturating_add(amount);
        }
        Err(_) => super::halt(state, object),
    }
}

// ---- Modal double-faced cards -----------------------------------------------------------

/// Shatterskull Smashing // Shatterskull, the Hammer Pass and Sundering
/// Eruption // Volcanic Fissure: their back faces are lands played from
/// hand.
pub(crate) fn has_land_back_face(name: &str) -> bool {
    name == super::SHATTERSKULL_SMASHING || name == super::SUNDERING_ERUPTION
}

/// Shatterskull Smashing: X may not be so small that its second target
/// would be dealt no damage.
pub(crate) fn minimum_x(name: &str, targets: &[Target]) -> Option<u8> {
    (name == super::SHATTERSKULL_SMASHING).then(|| super::targets::shatterskull_minimum_x(targets))
}
