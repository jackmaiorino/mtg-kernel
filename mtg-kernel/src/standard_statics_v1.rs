//! Static abilities of the MageZero Standard family G creatures. Every query
//! here is recomputed from the live board, like the existing self and lord
//! boosts in `engine`, and only exists in `standard-magezero-fixtures`
//! builds.

use crate::card_def::{CardType, Keywords, Subtype, CARD_DEFS};
use crate::ids::ObjectId;
use crate::state::{GameState, Zone};

fn battlefield_definition_name(state: &GameState, id: ObjectId) -> Option<&'static str> {
    let object = state.objects.try_get(id)?;
    let definition = CARD_DEFS.get(object.card_def as usize)?;
    (object.zone == Zone::Battlefield
        && definition.is_executable()
        && crate::continuous_characteristics_v1::printed_abilities_active(state, id))
    .then_some(definition.name)
}

fn total_counters(state: &GameState, id: ObjectId) -> i64 {
    let counters = state.objects.get(id).counters;
    [
        i64::from(counters.plus1_plus1),
        i64::from(counters.minus1_minus1),
        i64::from(counters.minus0_minus1),
        i64::from(counters.stun),
        i64::from(counters.lore),
        i64::from(counters.oil),
    ]
    .into_iter()
    .map(|count| count.max(0))
    .sum()
}

/// Keywords a permanent grants itself while a printed condition holds.
pub(crate) fn conditional_self_keywords(state: &GameState, id: ObjectId) -> Keywords {
    let Some(name) = battlefield_definition_name(state, id) else {
        return Keywords::NONE;
    };
    let controller = state.objects.get(id).controller;
    match name {
        // "This creature has first strike during your turn."
        "Razorkin Needlehead" if state.active_player == controller => Keywords::FIRST_STRIKE,
        // "As long as this creature has three or more counters on it, it has
        // flying and vigilance."
        "Warden of the Inner Sky" if total_counters(state, id) >= 3 => {
            Keywords(Keywords::FLYING.0 | Keywords::VIGILANCE.0)
        }
        _ => Keywords::NONE,
    }
}

/// Counters a permanent enters with beyond `CardDef::enters_with_plus_one_counters`,
/// applied in the same entry step before triggers and state-based actions.
pub(crate) fn apply_conditional_entry_counters(state: &mut GameState, id: ObjectId) {
    let Some(name) = battlefield_definition_name(state, id) else {
        return;
    };
    let controller = state.objects.get(id).controller;
    match name {
        // "This creature enters with a +1/+1 counter on it if you control a
        // permanent with mana value 4 or greater."
        "Ascendant Packleader" => {
            let controls_big_permanent =
                state.players[controller.index()]
                    .battlefield
                    .iter()
                    .any(|&other| {
                        other != id
                            && CARD_DEFS[state.objects.get(other).card_def as usize].mana_value >= 4
                    });
            if controls_big_permanent {
                state.objects.get_mut(id).counters.plus1_plus1 += 1;
            }
        }
        // "This creature enters with an oil counter on it."
        "Evolving Adaptive" => state.objects.get_mut(id).counters.oil += 1,
        _ => {}
    }
}

/// Power/toughness a permanent's own static gives itself from its counters.
pub(crate) fn self_counter_boost(state: &GameState, id: ObjectId) -> (i32, i32) {
    match battlefield_definition_name(state, id) {
        // "This creature gets +1/+1 for each oil counter on it."
        Some("Evolving Adaptive") => {
            let oil = i32::from(state.objects.get(id).counters.oil);
            (oil, oil)
        }
        _ => (0, 0),
    }
}

/// Definitions whose leave-the-battlefield abilities read the counters the
/// permanent had (`GameState::counter_lki_v1`).
pub(crate) fn reads_counter_lki(card_def: u16) -> bool {
    matches!(
        CARD_DEFS.get(card_def as usize).map(|def| def.name),
        Some("Quirion Beastcaller" | "Unstoppable Slasher")
    )
}

/// Generic ward costs other permanents grant `id`, one entry per granting
/// permanent: "Each other Human you control ... has ward {1}" (Coppercoat
/// Vanguard).
pub(crate) fn granted_ward_generics(state: &GameState, id: ObjectId) -> Vec<u8> {
    let Some(object) = state.objects.try_get(id) else {
        return Vec::new();
    };
    if object.zone != Zone::Battlefield
        || !crate::engine::object_has_type(state, id, CardType::Creature)
        || !crate::engine::has_effective_subtype(state, id, Subtype::Human)
    {
        return Vec::new();
    }
    state.players[object.controller.index()]
        .battlefield
        .iter()
        .copied()
        .filter(|&source| {
            source != id
                && battlefield_definition_name(state, source) == Some("Coppercoat Vanguard")
        })
        .map(|_| 1)
        .collect()
}

/// Whether a ward trigger costing `generic` on a permanent of `card_def` can
/// come from a grant above. The grant may have ended by the time the trigger
/// is validated on the stack, so this checks only the printed shape.
pub(crate) fn may_have_granted_ward(card_def: u16, generic: u8) -> bool {
    generic == 1
        && CARD_DEFS.get(card_def as usize).is_some_and(|def| {
            def.has_type(CardType::Creature) && def.subtypes.contains(&Subtype::Human)
        })
}

/// A characteristic-defining ability setting the card's own power (604.3),
/// applied in every zone. Effects that set power and toughness outright
/// (`continuous_characteristics_v1::creature_override`) still win.
pub(crate) fn characteristic_defining_power(state: &GameState, id: ObjectId) -> Option<i32> {
    let object = state.objects.try_get(id)?;
    let definition = CARD_DEFS.get(object.card_def as usize)?;
    if !definition.is_executable()
        || (object.zone == Zone::Battlefield
            && !crate::continuous_characteristics_v1::printed_abilities_active(state, id))
    {
        return None;
    }
    let controller = object.controller;
    let count = match definition.name {
        // "Adeline's power is equal to the number of creatures you control."
        "Adeline, Resplendent Cathar" => state.players[controller.index()]
            .battlefield
            .iter()
            .filter(|&&creature| {
                crate::engine::object_has_type(state, creature, CardType::Creature)
            })
            .count(),
        // "Haughty Djinn's power is equal to the number of instant and
        // sorcery cards in your graveyard."
        "Haughty Djinn" => state.players[controller.index()]
            .graveyard
            .iter()
            .filter(|&&card| {
                let card = state.objects.get(card);
                let def = &CARD_DEFS[card.card_def as usize];
                card.spell_copy_origin.is_none()
                    && (def.has_type(CardType::Instant) || def.has_type(CardType::Sorcery))
            })
            .count(),
        _ => return None,
    };
    Some(i32::try_from(count).unwrap_or(i32::MAX))
}

/// Life `player` actually loses when they would lose `amount` (from damage,
/// an effect, or a life payment): "If an opponent would lose life during
/// your turn, they lose twice that much life instead" (Bloodletter of
/// Aclazotz), once per Bloodletter the active player controls.
pub(crate) fn modified_life_loss(
    state: &GameState,
    player: crate::ids::PlayerId,
    amount: i32,
) -> i32 {
    let active = state.active_player;
    if amount <= 0 || player == active {
        return amount;
    }
    state.players[active.index()]
        .battlefield
        .iter()
        .filter(|&&source| {
            battlefield_definition_name(state, source) == Some("Bloodletter of Aclazotz")
                && state.objects.get(source).controller == active
        })
        .fold(amount, |lost, _| lost.saturating_mul(2))
}

/// Generic mana a spell with `types` cast by `caster` costs more and less
/// because of permanents' statics: "Noncreature spells cost {1} more to
/// cast" (Thalia, Guardian of Thraben, any controller) and "Instant and
/// sorcery spells you cast cost {1} less to cast" (Haughty Djinn).
pub(crate) fn spell_cost_generic_modifiers(
    state: &GameState,
    types: &[CardType],
    caster: crate::ids::PlayerId,
) -> (u8, u8) {
    let mut increase = 0u8;
    let mut reduction = 0u8;
    for seat in &state.players {
        for &source in &seat.battlefield {
            match battlefield_definition_name(state, source) {
                Some("Thalia, Guardian of Thraben") if !types.contains(&CardType::Creature) => {
                    increase = increase.saturating_add(1)
                }
                Some("Haughty Djinn")
                    if state.objects.get(source).controller == caster
                        && (types.contains(&CardType::Instant)
                            || types.contains(&CardType::Sorcery)) =>
                {
                    reduction = reduction.saturating_add(1)
                }
                _ => {}
            }
        }
    }
    (increase, reduction)
}

/// Printed "activate only if" conditions beyond timing and per-turn limits.
pub(crate) fn activation_condition_met(
    state: &GameState,
    source: ObjectId,
    ability_index: usize,
) -> bool {
    match (battlefield_definition_name(state, source), ability_index) {
        // "Activate only if an opponent has lost life this turn and only
        // once each turn."
        (Some("Hired Claw"), 0) => {
            let controller = state.objects.get(source).controller;
            state.player_lost_life_this_turn_v1(controller.opponent())
        }
        _ => true,
    }
}

fn link_is_live_on_battlefield(state: &GameState, link: crate::state::ObjectLinkV4) -> bool {
    state.objects.try_get(link.object).is_some_and(|object| {
        object.zone == Zone::Battlefield && object.zone_change_count == link.zone_change_count
    })
}

fn restriction_active(
    state: &GameState,
    restriction: &crate::state::AttackBlockRestrictionV1,
) -> bool {
    link_is_live_on_battlefield(state, restriction.creature)
        && link_is_live_on_battlefield(state, restriction.source)
        && state.objects.get(restriction.source.object).controller == restriction.controller
}

/// Whether a recorded restriction stops `id` from attacking or blocking.
pub(crate) fn cant_attack_or_block(state: &GameState, id: ObjectId) -> bool {
    state
        .attack_block_restrictions_v1
        .as_ref()
        .is_some_and(|restrictions| {
            restrictions.iter().any(|restriction| {
                restriction.creature.object == id && restriction_active(state, restriction)
            })
        })
}

/// Records a restriction, dropping any whose duration has already ended.
pub(crate) fn record_attack_block_restriction(
    state: &mut GameState,
    restriction: crate::state::AttackBlockRestrictionV1,
) {
    let mut restrictions = state
        .attack_block_restrictions_v1
        .take()
        .unwrap_or_default();
    restrictions.retain(|existing| restriction_active(state, existing));
    if restriction_active(state, &restriction) {
        restrictions.push(restriction);
    }
    state.attack_block_restrictions_v1 = (!restrictions.is_empty()).then_some(restrictions);
}

/// One static ability above, as the rules-vector extractor describes it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StandardStaticV1 {
    /// The permanent has these keywords while a printed condition holds.
    ConditionalSelfKeywords(Keywords),
    /// Enters with a +1/+1 counter if its controller controls another
    /// permanent with mana value 4 or greater.
    EntersWithPlusOneCounterIfControlsManaValueFour,
    /// Enters with an oil counter.
    EntersWithOilCounter,
    /// Gets +1/+1 for each oil counter on it.
    PlusOnePerOilCounter,
    /// Each other Human its controller controls has ward {1}.
    GrantsWardToOtherHumans,
    /// Power equals the number of creatures its controller controls.
    PowerEqualsControlledCreatures,
    /// Power equals the number of instant and sorcery cards in its
    /// controller's graveyard.
    PowerEqualsGraveyardInstantsAndSorceries,
    /// An opponent's life loss during its controller's turn doubles.
    DoublesOpponentLifeLossOnYourTurn,
    /// Noncreature spells cost {1} more.
    NoncreatureSpellsCostOneMore,
    /// Its controller's instant and sorcery spells cost {1} less.
    YourInstantsAndSorceriesCostOneLess,
    /// Its first activated ability needs an opponent to have lost life this
    /// turn, once per turn.
    FirstAbilityNeedsOpponentLifeLossThisTurn,
}

/// The statics this module applies for the definition named `name`. Every
/// name keyed above except the counter last-known-information bookkeeping
/// (`reads_counter_lki`) has an entry.
pub(crate) fn rules_vector_statics(name: &str) -> &'static [StandardStaticV1] {
    match name {
        "Razorkin Needlehead" => &[StandardStaticV1::ConditionalSelfKeywords(
            Keywords::FIRST_STRIKE,
        )],
        "Warden of the Inner Sky" => &[StandardStaticV1::ConditionalSelfKeywords(Keywords(
            Keywords::FLYING.0 | Keywords::VIGILANCE.0,
        ))],
        "Ascendant Packleader" => {
            &[StandardStaticV1::EntersWithPlusOneCounterIfControlsManaValueFour]
        }
        "Evolving Adaptive" => &[
            StandardStaticV1::EntersWithOilCounter,
            StandardStaticV1::PlusOnePerOilCounter,
        ],
        "Coppercoat Vanguard" => &[StandardStaticV1::GrantsWardToOtherHumans],
        "Adeline, Resplendent Cathar" => &[StandardStaticV1::PowerEqualsControlledCreatures],
        "Haughty Djinn" => &[
            StandardStaticV1::PowerEqualsGraveyardInstantsAndSorceries,
            StandardStaticV1::YourInstantsAndSorceriesCostOneLess,
        ],
        "Bloodletter of Aclazotz" => &[StandardStaticV1::DoublesOpponentLifeLossOnYourTurn],
        "Thalia, Guardian of Thraben" => &[StandardStaticV1::NoncreatureSpellsCostOneMore],
        "Hired Claw" => &[StandardStaticV1::FirstAbilityNeedsOpponentLifeLossThisTurn],
        _ => &[],
    }
}
