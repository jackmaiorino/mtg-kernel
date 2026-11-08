//! Static abilities and engine hooks of the Premodern wave P2c cards.

use super::{
    p2c_state, read_state, AKROMA, AVATAR_OF_WOE, BATTLEFIELD_SCROUNGER, KARMIC_GUIDE,
    PHANTOM_NISHOBA, PHYREXIAN_DEVOURER, PUTRID_IMP, REFLECTING_POOL, TRISKELION,
    UNDISCOVERED_PARADISE,
};
use crate::card_def::{mana_color_mask, CardType, CARD_DEFS};
use crate::eternal_legacy_e9a_v1::{active_named, live_link, TurnV1};
use crate::event::{self, DamageProposed, ProposedEvent};
use crate::ids::{ObjectId, PlayerId};
use crate::mana::ManaColor;
use crate::state::{GameState, Target, Zone};

fn name_of(state: &GameState, object: ObjectId) -> Option<&'static str> {
    let live = state.objects.try_get(object)?;
    CARD_DEFS
        .get(usize::from(live.card_def))
        .map(|def| def.name)
}

/// Threshold: seven or more cards in `player`'s graveyard.
fn threshold(state: &GameState, player: PlayerId) -> bool {
    crate::effect::controller_graveyard_card_count(state, player) >= 7
}

/// The definition id of Phyrexian Devourer, if the catalog has it.
pub(crate) fn devourer_id() -> Option<u16> {
    static ID: std::sync::OnceLock<Option<u16>> = std::sync::OnceLock::new();
    *ID.get_or_init(|| crate::card_def::card_id_by_name(PHYREXIAN_DEVOURER))
}

/// Whether `object` is a Phantom Nishoba on the battlefield (or one that
/// just left, for the damage it dealt).
pub(crate) fn is_nishoba(state: &GameState, object: ObjectId) -> bool {
    name_of(state, object) == Some(PHANTOM_NISHOBA)
        && crate::continuous_characteristics_v1::printed_abilities_active(state, object)
}

/// Printed protection from colors, as a color mask: Akroma ("protection
/// from black and from red") and Karmic Guide ("protection from black").
pub(crate) fn protection_mask(state: &GameState, host: ObjectId) -> u8 {
    match name_of(state, host) {
        Some(AKROMA) if active_named(state, host, AKROMA) => {
            mana_color_mask(ManaColor::B) | mana_color_mask(ManaColor::R)
        }
        Some(KARMIC_GUIDE) if active_named(state, host, KARMIC_GUIDE) => {
            mana_color_mask(ManaColor::B)
        }
        _ => 0,
    }
}

/// "Enters with N +1/+1 counters": Triskelion (three) and Phantom Nishoba
/// (seven).
pub(crate) fn apply_entry_counters(
    state: &mut GameState,
    object: ObjectId,
    input: Option<&crate::doubling_season_v48::ReplacementInputV48>,
) {
    let count = match name_of(state, object) {
        Some(TRISKELION) => 3,
        Some(PHANTOM_NISHOBA) => 7,
        _ => return,
    };
    let live = state.objects.get(object);
    // 614.12: the replacement belongs to the permanent as it would be on the
    // battlefield, so a creature that loses its abilities (Dress Down)
    // enters without counters.
    if live.zone != Zone::Battlefield
        || live.v4.face_index != 0
        || !crate::continuous_characteristics_v1::printed_abilities_active(state, object)
    {
        return;
    }
    let controller = live.controller;
    match crate::doubling_season_v48::entry_amount(state, input, controller, count).and_then(
        |amount| {
            state
                .objects
                .get(object)
                .counters
                .plus1_plus1
                .checked_add(amount)
                .ok_or_else(|| "entry counter overflow".to_string())
        },
    ) {
        Ok(total) => state.objects.get_mut(object).counters.plus1_plus1 = total,
        Err(_) => {
            state.engine.halted = Some((
                crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
                object,
            ))
        }
    }
}

// ---- Undiscovered Paradise ----------------------------------------------------------

/// Called after `land` was tapped for mana: an Undiscovered Paradise owes
/// its return to hand in its controller's next untap step.
pub(crate) fn after_tapped_for_mana(state: &mut GameState, land: ObjectId) {
    if name_of(state, land) != Some(UNDISCOVERED_PARADISE)
        || !active_named(state, land, UNDISCOVERED_PARADISE)
    {
        return;
    }
    let link = live_link(state, land);
    let returns = &mut p2c_state(state).paradise_returns;
    if !returns.contains(&link) {
        returns.push(link);
    }
}

/// Untap step: "as you untap your permanents, return Undiscovered Paradise
/// to its owner's hand". Whether `object` was returned instead of untapped.
pub(crate) fn return_at_untap(state: &mut GameState, object: ObjectId) -> bool {
    let Some(p2c) = read_state(state) else {
        return false;
    };
    if p2c.paradise_returns.is_empty() {
        return false;
    }
    // A skipped untap step (Stasis) returns nothing; the next one that
    // happens does. A permanent that merely does not untap still returns.
    if crate::eternal_premodern_p2a_v1::statics::untap_step_skipped(state) {
        return false;
    }
    let link = live_link(state, object);
    if !p2c.paradise_returns.contains(&link) || state.objects.get(object).zone != Zone::Battlefield
    {
        return false;
    }
    p2c_state(state)
        .paradise_returns
        .retain(|entry| *entry != link);
    event::propose_and_commit(state, ProposedEvent::zone_change(object, Zone::Hand));
    true
}

// ---- Reflecting Pool ------------------------------------------------------------------

/// Whether `land`, a land `player` controls, could produce `color` right
/// now: what its current mana abilities (printed, or granted by a land type
/// change, a moon or another effect) would add, ignoring whether it is
/// tapped and ignoring the costs and activation restrictions of those
/// abilities (the Reflecting Pool and Exotic Orchard rulings).
fn land_could_produce(
    state: &GameState,
    player: PlayerId,
    land: ObjectId,
    color: ManaColor,
) -> bool {
    let live = state.objects.get(land);
    if live.zone != Zone::Battlefield
        || live.controller != player
        || !crate::engine::object_has_type(state, land, CardType::Land)
    {
        return false;
    }
    let Some(def) = CARD_DEFS.get(usize::from(live.card_def)) else {
        return false;
    };
    let printed = crate::continuous_characteristics_v1::printed_abilities_active(state, land);
    if printed && def.name == REFLECTING_POOL {
        return false;
    }
    if crate::standard_legends_v1::katilda_mana_colors(state, land).contains(&color) {
        return true;
    }
    if crate::continuous_characteristics_v1::land_grants_colorless_mana(state, land) {
        return color == ManaColor::C;
    }
    if !printed
        || !def.has_mana_ability()
        || !crate::standard_cards_v1::mana_abilities_active(state, land, def)
    {
        return false;
    }
    def.primary_mana_ability_choices(live.v4.chosen_color)
        .contains(&color)
        || def
            .restricted_mana_abilities
            .iter()
            .any(|restricted| restricted.colors.contains(&color))
        || def
            .additional_mana_abilities
            .iter()
            .any(|additional| additional.colors.contains(&color))
}

/// Whether a land `player` controls other than a Reflecting Pool could
/// produce `color` (or colorless) mana right now.
pub(crate) fn other_land_could_produce(
    state: &GameState,
    player: PlayerId,
    color: ManaColor,
) -> bool {
    state.players[player.index()]
        .battlefield
        .iter()
        .copied()
        .any(|land| land_could_produce(state, player, land, color))
}

/// The colors the payment planner may take from a Reflecting Pool: every
/// type a land its controller controls could produce.
pub(crate) fn pool_colors(state: &GameState, land: ObjectId) -> Vec<ManaColor> {
    if name_of(state, land) != Some(REFLECTING_POOL) {
        return Vec::new();
    }
    let controller = state.objects.get(land).controller;
    [
        ManaColor::W,
        ManaColor::U,
        ManaColor::B,
        ManaColor::R,
        ManaColor::G,
        ManaColor::C,
    ]
    .into_iter()
    .filter(|&color| other_land_could_produce(state, controller, color))
    .collect()
}

// ---- Life ---------------------------------------------------------------------------

/// The timestamp of the Life that made `object` a 1/1 creature this turn.
fn life_timestamp(state: &GameState, object: ObjectId) -> Option<u64> {
    let p2c = read_state(state)?;
    if p2c.life_lands.is_empty() {
        return None;
    }
    let live = state.objects.try_get(object)?;
    let now = TurnV1::now(state);
    p2c.life_lands
        .iter()
        .find(|(link, turn, _)| {
            link.object == object
                && link.zone_change_count == live.zone_change_count
                && *turn == now
        })
        .map(|(_, _, timestamp)| *timestamp)
}

/// Life: a land it animated is a creature (and still a land).
pub(crate) fn is_life_land(state: &GameState, object: ObjectId) -> bool {
    state
        .objects
        .try_get(object)
        .is_some_and(|live| live.zone == Zone::Battlefield)
        && life_timestamp(state, object).is_some()
}

/// Life: layer 7b base power and toughness of 1/1.
pub(crate) fn base_power_toughness(state: &GameState, object: ObjectId) -> Option<(i16, i16, u64)> {
    life_timestamp(state, object)
        .filter(|_| is_life_land(state, object))
        .map(|timestamp| (1, 1, timestamp))
}

// ---- Combat and power ---------------------------------------------------------------

/// Avatar of Woe's fear ("can't be blocked except by artifact creatures
/// and/or black creatures") and Putrid Imp's "can't block" with threshold.
pub(crate) fn blocker_allowed(state: &GameState, attacker: ObjectId, blocker: ObjectId) -> bool {
    if active_named(state, attacker, AVATAR_OF_WOE) {
        let black =
            crate::engine::object_color_mask(state, blocker) & mana_color_mask(ManaColor::B) != 0;
        if !black && !crate::engine::object_has_type(state, blocker, CardType::Artifact) {
            return false;
        }
    }
    if active_named(state, blocker, PUTRID_IMP)
        && threshold(state, state.objects.get(blocker).controller)
    {
        return false;
    }
    true
}

/// Putrid Imp: "As long as seven or more cards are in your graveyard, it
/// gets +1/+1."
pub(crate) fn controlled_boost(state: &GameState, object: ObjectId) -> (i128, i128) {
    if active_named(state, object, PUTRID_IMP)
        && threshold(state, state.objects.get(object).controller)
    {
        (1, 1)
    } else {
        (0, 0)
    }
}

/// A batch of simultaneous damage events is about to be replaced: a Phantom
/// Nishoba removes one counter for the whole batch (its prevention effect
/// applies to each event, but the counter comes off once per batch). Returns
/// whether this call opened the batch; pass it to [`end_damage_batch`].
pub(crate) fn begin_damage_batch(state: &mut GameState, events: &[ProposedEvent]) -> bool {
    if read_state(state).is_some_and(|p2c| p2c.nishoba_batch.is_some()) {
        return false;
    }
    let hits_nishoba = events.iter().any(|event| {
        matches!(
            event,
            ProposedEvent::Damage(damage)
                if matches!(damage.target, Target::Object(target)
                    if active_named(state, target, PHANTOM_NISHOBA))
        )
    });
    if hits_nishoba {
        p2c_state(state).nishoba_batch = Some(Vec::new());
    }
    hits_nishoba
}

/// Closes the batch [`begin_damage_batch`] opened.
pub(crate) fn end_damage_batch(state: &mut GameState, began: bool) {
    if began {
        if let Some(eternal) = state.eternal_v1.as_mut() {
            eternal.p2c.nishoba_batch = None;
        }
    }
}

/// Phantom Nishoba: "If damage would be dealt to this creature, prevent that
/// damage. Remove a +1/+1 counter from it." Whether `damage` is prevented.
/// Damage that can't be prevented is dealt, but the effect still applies its
/// additional effect (615.12), and one batch of simultaneous damage removes
/// one counter.
pub(crate) fn prevent_damage(
    state: &mut GameState,
    damage: &DamageProposed,
    unpreventable: bool,
) -> bool {
    let Target::Object(target) = damage.target else {
        return false;
    };
    if damage.amount <= 0 || !active_named(state, target, PHANTOM_NISHOBA) {
        return false;
    }
    let link = live_link(state, target);
    if read_state(state).is_some_and(|p2c| p2c.nishoba_batch.is_some()) {
        let seen = p2c_state(state)
            .nishoba_batch
            .as_mut()
            .expect("an open batch");
        if seen.contains(&link) {
            return !unpreventable;
        }
        seen.push(link);
    }
    let counters = &mut state.objects.get_mut(target).counters.plus1_plus1;
    if *counters > 0 {
        *counters -= 1;
    }
    !unpreventable
}

/// Activation restrictions: Battlefield Scrounger needs threshold.
pub(crate) fn activation_allowed(
    state: &GameState,
    source: ObjectId,
    name: &str,
    ability_index: usize,
) -> bool {
    match (name, ability_index) {
        (BATTLEFIELD_SCROUNGER, 0) => threshold(state, state.objects.get(source).controller),
        _ => true,
    }
}
