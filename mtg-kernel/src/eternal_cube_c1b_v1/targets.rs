//! Target filters of the wave C1b cards, carried as
//! `StandardTargetV1::C1bV1`. `standard_cards_v1::legal_targets`,
//! `target_matches`, `target_zone_allows` and `StandardTargetV1::counts` ask
//! them first.
//!
//! Pest Infestation takes any number of artifacts and/or enchantments (the
//! kernel announces X after targets, so "up to X" is bounded by
//! `eternal_cube_c1b_v1::minimum_x`). Collective Brutality's seven modes are
//! the nonempty subsets of its three modes; a mode's filter takes one target
//! per chosen mode that targets, in printed order: "target opponent" (reveal
//! their hand), "target creature" (-2/-2) and "target opponent" (drain).
//! Each target word is its own slot (115.3), so the two opponent slots may
//! both name the one opponent.

use crate::card_def::CardType;
use crate::ids::{ObjectId, PlayerId};
use crate::standard_cards_v1::StandardTargetV1;
use crate::state::{GameState, Target, Zone};
use serde::{Deserialize, Serialize};

/// The most targets a Pest Infestation may name.
const MOST_TARGETS: u8 = 64;

/// The filters of wave C1b.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum C1bTargetV1 {
    /// Pest Infestation: any number of distinct artifacts and/or
    /// enchantments.
    ArtifactsAndEnchantmentsV1,
    /// Collective Brutality: the slots of the chosen modes, as a mask of
    /// bit 0 (reveal: an opponent), bit 1 (shrink: a creature) and bit 2
    /// (drain: an opponent), never empty.
    BrutalitySlotsV1(u8),
}

/// The kind of target a slot takes.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Slot {
    Opponent,
    Creature,
}

/// The slots of Collective Brutality's mode mask, in printed order.
fn brutality_slots(mask: u8) -> Vec<Slot> {
    let mut slots = Vec::new();
    if mask & 1 != 0 {
        slots.push(Slot::Opponent);
    }
    if mask & 2 != 0 {
        slots.push(Slot::Creature);
    }
    if mask & 4 != 0 {
        slots.push(Slot::Opponent);
    }
    slots
}

fn owned(target: StandardTargetV1) -> Option<C1bTargetV1> {
    match target {
        StandardTargetV1::C1bV1(filter) => Some(filter),
        _ => None,
    }
}

/// How many targets this module's filter takes, and how many at least.
pub(crate) fn counts(target: StandardTargetV1) -> Option<(u8, u8)> {
    Some(match owned(target)? {
        C1bTargetV1::ArtifactsAndEnchantmentsV1 => (MOST_TARGETS, 0),
        C1bTargetV1::BrutalitySlotsV1(mask) => {
            let count = u8::try_from(brutality_slots(mask).len()).unwrap_or(u8::MAX);
            (count, count)
        }
    })
}

/// Whether this module's filter takes a player as its target number `slot`:
/// Collective Brutality's "target opponent" slots.
pub(crate) fn slot_takes_player(target: StandardTargetV1, slot: usize) -> bool {
    matches!(
        owned(target),
        Some(C1bTargetV1::BrutalitySlotsV1(mask))
            if brutality_slots(mask).get(slot) == Some(&Slot::Opponent)
    )
}

/// Whether an object target of this module's filter may be in `zone`.
pub(crate) fn zone_allows(target: StandardTargetV1, zone: Zone) -> bool {
    match owned(target) {
        Some(C1bTargetV1::ArtifactsAndEnchantmentsV1) => zone == Zone::Battlefield,
        Some(C1bTargetV1::BrutalitySlotsV1(mask)) => {
            zone == Zone::Battlefield && brutality_slots(mask).contains(&Slot::Creature)
        }
        None => false,
    }
}

fn on_battlefield(state: &GameState, object: ObjectId) -> bool {
    state
        .objects
        .try_get(object)
        .is_some_and(|live| live.zone == Zone::Battlefield)
}

/// Whether `object` satisfies the filter of one of this module's object
/// slots, or `None` for every other filter.
pub(crate) fn target_matches(
    target: StandardTargetV1,
    object: ObjectId,
    state: &GameState,
) -> Option<bool> {
    let filter = owned(target)?;
    if !on_battlefield(state, object) {
        return Some(false);
    }
    let has = |card_type| crate::engine::object_has_type(state, object, card_type);
    Some(match filter {
        C1bTargetV1::ArtifactsAndEnchantmentsV1 => {
            has(CardType::Artifact) || has(CardType::Enchantment)
        }
        C1bTargetV1::BrutalitySlotsV1(_) => has(CardType::Creature),
    })
}

/// The legal choices for slot `prefix.len()` of this module's filter, or
/// `None` for every other filter.
pub(crate) fn legal_targets(
    target: StandardTargetV1,
    controller: PlayerId,
    prefix: &[Target],
    state: &GameState,
) -> Option<Vec<Target>> {
    let filter = owned(target)?;
    let battlefield = || {
        [PlayerId::P0, PlayerId::P1]
            .into_iter()
            .flat_map(|player| state.players[player.index()].battlefield.iter().copied())
    };
    Some(match filter {
        C1bTargetV1::ArtifactsAndEnchantmentsV1 => {
            if prefix.len() >= usize::from(MOST_TARGETS) {
                return Some(Vec::new());
            }
            battlefield()
                .filter(|&object| target_matches(target, object, state) == Some(true))
                .map(Target::Object)
                .filter(|candidate| !prefix.contains(candidate))
                .collect()
        }
        C1bTargetV1::BrutalitySlotsV1(mask) => match brutality_slots(mask).get(prefix.len()) {
            Some(Slot::Opponent) => vec![Target::Player(controller.opponent())],
            Some(Slot::Creature) => battlefield()
                .filter(|&object| target_matches(target, object, state) == Some(true))
                .map(Target::Object)
                .collect(),
            None => Vec::new(),
        },
    })
}
