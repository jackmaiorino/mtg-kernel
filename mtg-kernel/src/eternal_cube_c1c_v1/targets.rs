//! Target filters of the wave C1c cards, carried as
//! `StandardTargetV1::C1cV1`. `standard_cards_v1::legal_targets`,
//! `target_matches`, `target_zone_allows` and `StandardTargetV1::counts` ask
//! them first.
//!
//! Impractical Joke's "up to one target creature or planeswalker" and Fire
//! Covenant's "any number of target creatures" are filters of their own. Fire
//! Covenant's division is announced with its targets (601.2d) as Fire // Ice's
//! is: each pick assigns one point of damage, so a creature picked twice is
//! dealt two, and the damage dealt (and the life paid) is the number of picks.
//!
//! Kolaghan's Command's "choose two" is one printed mode per unordered pair
//! of its four modes. Each pair's filter takes one target per mode in
//! printed order, and each slot is the target of that mode alone, so two
//! slots may name the same player or permanent (115.3: only the same
//! instance of the word "target" may not).

use crate::card_def::{CardType, CARD_DEFS};
use crate::ids::{ObjectId, PlayerId};
use crate::standard_cards_v1::StandardTargetV1;
use crate::state::{GameState, Target, Zone};
use serde::{Deserialize, Serialize};

/// A targeted mode of Kolaghan's Command, in printed order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum KolaghanModeV1 {
    /// "Return target creature card from your graveyard to your hand."
    ReturnCreatureCardV1,
    /// "Target player discards a card."
    DiscardV1,
    /// "Destroy target artifact."
    DestroyArtifactV1,
    /// "Kolaghan's Command deals 2 damage to any target."
    DamageV1,
}

/// The filters of wave C1c.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum C1cTargetV1 {
    /// The two targets of a pair of Kolaghan's Command's modes, in printed
    /// order.
    KolaghanPairV1(KolaghanModeV1, KolaghanModeV1),
    /// "Up to one target creature or planeswalker" (Impractical Joke).
    UpToOneCreatureOrPlaneswalkerV1,
    /// "Any number of target creatures" with the damage divided one point per
    /// pick (Fire Covenant). At most as many picks as the caster has life.
    CreaturePicksPerLifeV1,
}

/// The most picks Fire Covenant may announce, whatever the caster's life.
pub(crate) const MAX_COVENANT_PICKS: u8 = 64;

impl KolaghanModeV1 {
    /// Whether this mode's target is a player (alone or among others).
    fn takes_player(self) -> bool {
        matches!(self, Self::DiscardV1 | Self::DamageV1)
    }

    /// Whether this mode's target is a card in its controller's graveyard.
    fn takes_graveyard_card(self) -> bool {
        self == Self::ReturnCreatureCardV1
    }

    /// Whether this mode's target is a permanent.
    fn takes_permanent(self) -> bool {
        matches!(self, Self::DestroyArtifactV1 | Self::DamageV1)
    }
}

fn pair(target: StandardTargetV1) -> Option<[KolaghanModeV1; 2]> {
    match target {
        StandardTargetV1::C1cV1(C1cTargetV1::KolaghanPairV1(first, second)) => {
            Some([first, second])
        }
        _ => None,
    }
}

fn filter(target: StandardTargetV1) -> Option<C1cTargetV1> {
    match target {
        StandardTargetV1::C1cV1(filter) => Some(filter),
        _ => None,
    }
}

/// How many targets this module's filter takes, and how many at least.
pub(crate) fn counts(target: StandardTargetV1) -> Option<(u8, u8)> {
    Some(match filter(target)? {
        C1cTargetV1::KolaghanPairV1(..) => (2, 2),
        C1cTargetV1::UpToOneCreatureOrPlaneswalkerV1 => (1, 0),
        C1cTargetV1::CreaturePicksPerLifeV1 => (MAX_COVENANT_PICKS, 0),
    })
}

/// Whether this module's filter takes a player as its target number `slot`.
pub(crate) fn slot_takes_player(target: StandardTargetV1, slot: usize) -> bool {
    pair(target).is_some_and(|modes| modes.get(slot).is_some_and(|mode| mode.takes_player()))
}

/// Whether an object target of this module's filter may be in `zone`.
pub(crate) fn zone_allows(target: StandardTargetV1, zone: Zone) -> bool {
    match filter(target) {
        Some(C1cTargetV1::KolaghanPairV1(first, second)) => match zone {
            Zone::Graveyard => [first, second]
                .iter()
                .any(|mode| mode.takes_graveyard_card()),
            Zone::Battlefield => [first, second].iter().any(|mode| mode.takes_permanent()),
            _ => false,
        },
        Some(
            C1cTargetV1::UpToOneCreatureOrPlaneswalkerV1 | C1cTargetV1::CreaturePicksPerLifeV1,
        ) => zone == Zone::Battlefield,
        None => false,
    }
}

fn graveyard_creature_card(state: &GameState, controller: PlayerId, object: ObjectId) -> bool {
    state.objects.try_get(object).is_some_and(|live| {
        live.zone == Zone::Graveyard
            && live.owner == controller
            && !live.v4.is_token
            && live.spell_copy_origin.is_none()
            && CARD_DEFS
                .get(usize::from(live.card_def))
                .is_some_and(|def| def.has_type(CardType::Creature))
    })
}

fn battlefield_matches(state: &GameState, mode: KolaghanModeV1, object: ObjectId) -> bool {
    let live = state.objects.get(object);
    if live.zone != Zone::Battlefield {
        return false;
    }
    let has = |card_type| crate::engine::object_has_type(state, object, card_type);
    match mode {
        KolaghanModeV1::DestroyArtifactV1 => has(CardType::Artifact),
        // 115.4: "any target" is a creature, player, planeswalker or battle.
        KolaghanModeV1::DamageV1 => has(CardType::Creature) || has(CardType::Planeswalker),
        KolaghanModeV1::ReturnCreatureCardV1 | KolaghanModeV1::DiscardV1 => false,
    }
}

/// A creature (or, with `planeswalkers`, a planeswalker) on a battlefield.
fn permanent_matches(state: &GameState, object: ObjectId, planeswalkers: bool) -> bool {
    state
        .objects
        .try_get(object)
        .is_some_and(|live| live.zone == Zone::Battlefield)
        && (crate::engine::object_has_type(state, object, CardType::Creature)
            || planeswalkers
                && crate::engine::object_has_type(state, object, CardType::Planeswalker))
}

/// Whether `object` satisfies the filter of one of this module's slots (the
/// zone settles which), or `None` for every other filter.
pub(crate) fn target_matches(
    target: StandardTargetV1,
    controller: PlayerId,
    object: ObjectId,
    state: &GameState,
) -> Option<bool> {
    Some(match filter(target)? {
        C1cTargetV1::KolaghanPairV1(first, second) => [first, second].iter().any(|&mode| {
            graveyard_creature_card(state, controller, object) && mode.takes_graveyard_card()
                || battlefield_matches(state, mode, object)
        }),
        C1cTargetV1::UpToOneCreatureOrPlaneswalkerV1 => permanent_matches(state, object, true),
        C1cTargetV1::CreaturePicksPerLifeV1 => permanent_matches(state, object, false),
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
    let on_battlefields = |planeswalkers: bool| {
        [PlayerId::P0, PlayerId::P1]
            .into_iter()
            .flat_map(|player| state.players[player.index()].battlefield.iter().copied())
            .filter(|&object| permanent_matches(state, object, planeswalkers))
            .map(Target::Object)
            .collect::<Vec<_>>()
    };
    let modes = match filter(target)? {
        C1cTargetV1::KolaghanPairV1(first, second) => [first, second],
        C1cTargetV1::UpToOneCreatureOrPlaneswalkerV1 => {
            return Some(if prefix.is_empty() {
                on_battlefields(true)
            } else {
                Vec::new()
            })
        }
        C1cTargetV1::CreaturePicksPerLifeV1 => {
            // Each pick is one damage and one life paid: while the spell is
            // being cast, no more picks than the caster has life (the life
            // is paid as the cast completes, so a spell on the stack is
            // checked against no limit), and a creature may be picked again.
            let life = if super::covenant_cast_in_progress(state, controller) {
                usize::try_from(state.players[controller.index()].life.max(0)).unwrap_or(usize::MAX)
            } else {
                usize::MAX
            };
            return Some(
                if prefix.len() >= life.min(usize::from(MAX_COVENANT_PICKS)) {
                    Vec::new()
                } else {
                    on_battlefields(false)
                },
            );
        }
    };
    let Some(&mode) = modes.get(prefix.len()) else {
        return Some(Vec::new());
    };
    let mut legal = Vec::new();
    if mode.takes_player() {
        legal.extend([PlayerId::P0, PlayerId::P1].map(Target::Player));
    }
    if mode.takes_graveyard_card() {
        legal.extend(
            state.players[controller.index()]
                .graveyard
                .iter()
                .copied()
                .filter(|&card| graveyard_creature_card(state, controller, card))
                .map(Target::Object),
        );
    }
    if mode.takes_permanent() {
        legal.extend(
            [PlayerId::P0, PlayerId::P1]
                .into_iter()
                .flat_map(|player| state.players[player.index()].battlefield.iter().copied())
                .filter(|&object| battlefield_matches(state, mode, object))
                .map(Target::Object),
        );
    }
    Some(legal)
}
