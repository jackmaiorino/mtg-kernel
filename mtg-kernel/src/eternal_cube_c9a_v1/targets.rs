//! Target filters of Vintage Cube wave C9a, carried as
//! `StandardTargetV1::C9aV1`. `standard_cards_v1::legal_targets`,
//! `target_matches`, `target_zone_allows`, `StandardTargetV1::counts`, and
//! `state::validate_stack_target_contract` ask these before falling through
//! to their built-in match.

use crate::card_def::CardType;
use crate::ids::{ObjectId, PlayerId};
use crate::standard_cards_v1::StandardTargetV1;
use crate::state::{GameState, StackItemKind, Target, Zone};
use serde::{Deserialize, Serialize};

/// Target filters owned by Vintage Cube wave C9a.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum C9aTargetV1 {
    /// Aang, Swift Savior: "up to one other target creature or spell".
    UpToOneOtherCreatureOrSpellV1,
    /// Badgermole Cub: "target land you control" (earthbend 1).
    ControlledLandV1,
    /// Batroc the Leaper: "each of up to X targets", where X is the number of
    /// times he was kicked. Distinct any-targets (CR 115.3).
    BatrocUpToAnyTargetsV1(u8),
}

fn owned(target: StandardTargetV1) -> Option<C9aTargetV1> {
    match target {
        StandardTargetV1::C9aV1(filter) => Some(filter),
        _ => None,
    }
}

/// How many targets this module's filter takes, and how many at least.
pub(crate) fn counts(target: StandardTargetV1) -> Option<(u8, u8)> {
    Some(match owned(target)? {
        C9aTargetV1::UpToOneOtherCreatureOrSpellV1 => (1, 0),
        C9aTargetV1::ControlledLandV1 => (1, 1),
        C9aTargetV1::BatrocUpToAnyTargetsV1(max) => (max, 0),
    })
}

/// Whether an object target of this module's filter may be in `zone`.
pub(crate) fn zone_allows(target: StandardTargetV1, zone: Zone) -> bool {
    match owned(target) {
        Some(C9aTargetV1::UpToOneOtherCreatureOrSpellV1) => {
            matches!(zone, Zone::Battlefield | Zone::Stack)
        }
        Some(C9aTargetV1::ControlledLandV1 | C9aTargetV1::BatrocUpToAnyTargetsV1(_)) => {
            zone == Zone::Battlefield
        }
        None => false,
    }
}

/// Whether slot `slot` of this module's filter may name a player.
pub(crate) fn slot_takes_player(target: StandardTargetV1, slot: usize) -> bool {
    match owned(target) {
        Some(C9aTargetV1::BatrocUpToAnyTargetsV1(max)) => slot < usize::from(max),
        _ => false,
    }
}

fn is_live_stack_spell(state: &GameState, object: ObjectId) -> bool {
    if state
        .engine
        .pending_cast
        .as_ref()
        .is_some_and(|pending| pending.spell == object)
    {
        return false;
    }
    let Some(live) = state.objects.try_get(object) else {
        return false;
    };
    if live.zone != Zone::Stack {
        return false;
    }
    state.stack.iter().any(|item| {
        item.kind == StackItemKind::Spell
            && item.source == object
            && item
                .v4
                .source_contract
                .is_none_or(|contract| contract.zone_change_count == live.zone_change_count)
    })
}

/// Whether `object` satisfies this module's filter `target` for `controller`
/// (independent of source exclusion, which `legal_targets` enforces).
pub(crate) fn target_matches(
    target: StandardTargetV1,
    controller: PlayerId,
    object: ObjectId,
    state: &GameState,
) -> Option<bool> {
    let filter = owned(target)?;
    let Some(live) = state.objects.try_get(object) else {
        return Some(false);
    };
    Some(match filter {
        C9aTargetV1::UpToOneOtherCreatureOrSpellV1 => match live.zone {
            Zone::Battlefield => crate::engine::object_has_type(state, object, CardType::Creature),
            Zone::Stack => is_live_stack_spell(state, object),
            _ => false,
        },
        C9aTargetV1::ControlledLandV1 => {
            live.zone == Zone::Battlefield
                && live.controller == controller
                && crate::engine::object_has_type(state, object, CardType::Land)
        }
        C9aTargetV1::BatrocUpToAnyTargetsV1(_) => {
            live.zone == Zone::Battlefield
                && (crate::engine::object_has_type(state, object, CardType::Creature)
                    || crate::engine::object_has_type(state, object, CardType::Planeswalker))
        }
    })
}

/// Legal choices for slot `prefix.len()` of this module's filter, or `None`
/// for every other filter.
pub(crate) fn legal_targets(
    target: StandardTargetV1,
    controller: PlayerId,
    source: Option<(ObjectId, u32)>,
    prefix: &[Target],
    state: &GameState,
) -> Option<Vec<Target>> {
    let filter = owned(target)?;
    match filter {
        C9aTargetV1::UpToOneOtherCreatureOrSpellV1 => {
            if !prefix.is_empty() {
                return Some(Vec::new());
            }
            let same_source = |object: ObjectId| {
                source.is_some_and(|(source_id, source_zcc)| {
                    source_id == object
                        && state
                            .objects
                            .try_get(object)
                            .is_some_and(|live| live.zone_change_count == source_zcc)
                })
            };
            let mut out = Vec::new();
            for item in &state.stack {
                if item.kind == StackItemKind::Spell
                    && is_live_stack_spell(state, item.source)
                    && !same_source(item.source)
                {
                    let candidate = Target::Object(item.source);
                    if !out.contains(&candidate) {
                        out.push(candidate);
                    }
                }
            }
            for player in [PlayerId::P0, PlayerId::P1] {
                for &object in &state.players[player.index()].battlefield {
                    if !same_source(object)
                        && target_matches(target, controller, object, state) == Some(true)
                    {
                        out.push(Target::Object(object));
                    }
                }
            }
            Some(out)
        }
        C9aTargetV1::ControlledLandV1 => {
            if !prefix.is_empty() {
                return Some(Vec::new());
            }
            Some(
                state.players[controller.index()]
                    .battlefield
                    .iter()
                    .copied()
                    .filter(|&object| {
                        target_matches(target, controller, object, state) == Some(true)
                    })
                    .map(Target::Object)
                    .collect(),
            )
        }
        C9aTargetV1::BatrocUpToAnyTargetsV1(max) => {
            if prefix.len() >= usize::from(max) {
                return Some(Vec::new());
            }
            let players = [PlayerId::P0, PlayerId::P1].map(Target::Player);
            let permanents = [PlayerId::P0, PlayerId::P1]
                .into_iter()
                .flat_map(|player| state.players[player.index()].battlefield.iter().copied())
                .filter(|&object| target_matches(target, controller, object, state) == Some(true))
                .map(Target::Object);
            Some(
                players
                    .into_iter()
                    .chain(permanents)
                    .filter(|candidate| !prefix.contains(candidate))
                    .collect(),
            )
        }
    }
}
