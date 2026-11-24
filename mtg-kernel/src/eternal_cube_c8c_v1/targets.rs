//! Target filters of Vintage Cube wave C8c, carried as
//! `StandardTargetV1::C8cV1`. `standard_cards_v1::legal_targets`,
//! `target_matches`, `target_zone_allows`, and `StandardTargetV1::counts` ask
//! these before falling through to their built-in match.

use crate::card_def::{CardType, Subtype};
use crate::ids::{ObjectId, PlayerId};
use crate::standard_cards_v1::StandardTargetV1;
use crate::state::{GameState, ObjectLinkV4, Target, Zone};
use serde::{Deserialize, Serialize};

/// Target filters owned by Vintage Cube wave C8c.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum C8cTargetV1 {
    /// Thieving Skydiver: "target artifact with mana value X or less", where
    /// X is the kicked X paid to cast the Skydiver source incarnation.
    SkydiverArtifactV1,
    /// Absorbing Man: "up to one target artifact, non-Aura enchantment, or
    /// land".
    UpToOneArtifactNonAuraEnchantmentOrLandV1,
}

fn owned(target: StandardTargetV1) -> Option<C8cTargetV1> {
    match target {
        StandardTargetV1::C8cV1(filter) => Some(filter),
        _ => None,
    }
}

/// How many targets this module's filter takes, and how many at least.
pub(crate) fn counts(target: StandardTargetV1) -> Option<(u8, u8)> {
    Some(match owned(target)? {
        C8cTargetV1::SkydiverArtifactV1 => (1, 1),
        C8cTargetV1::UpToOneArtifactNonAuraEnchantmentOrLandV1 => (1, 0),
    })
}

/// Whether an object target of this module's filter may be in `zone`.
pub(crate) fn zone_allows(target: StandardTargetV1, zone: Zone) -> bool {
    owned(target).is_some() && zone == Zone::Battlefield
}

fn is_absorbing_man_permanent(state: &GameState, object: ObjectId) -> bool {
    crate::engine::object_has_type(state, object, CardType::Artifact)
        || (crate::engine::object_has_type(state, object, CardType::Enchantment)
            && !crate::engine::has_effective_subtype(state, object, Subtype::Aura))
        || crate::engine::object_has_type(state, object, CardType::Land)
}

/// Whether `object` satisfies this module's filter `target` for `controller`
/// (independent of source-bound X, which `legal_targets` enforces).
pub(crate) fn target_matches(
    target: StandardTargetV1,
    _controller: PlayerId,
    object: ObjectId,
    state: &GameState,
) -> Option<bool> {
    let filter = owned(target)?;
    let Some(live) = state.objects.try_get(object) else {
        return Some(false);
    };
    if live.zone != Zone::Battlefield {
        return Some(false);
    }
    Some(match filter {
        C8cTargetV1::SkydiverArtifactV1 => {
            crate::engine::object_has_type(state, object, CardType::Artifact)
        }
        C8cTargetV1::UpToOneArtifactNonAuraEnchantmentOrLandV1 => {
            is_absorbing_man_permanent(state, object)
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
    if !prefix.is_empty() {
        return Some(Vec::new());
    }
    match filter {
        C8cTargetV1::SkydiverArtifactV1 => {
            let Some((source_object, source_zcc)) = source else {
                return Some(Vec::new());
            };
            let Some(max_mv) = super::cast_x_for(
                state,
                ObjectLinkV4 {
                    object: source_object,
                    zone_change_count: source_zcc,
                },
            ) else {
                return Some(Vec::new());
            };
            Some(
                [PlayerId::P0, PlayerId::P1]
                    .into_iter()
                    .flat_map(|player| state.players[player.index()].battlefield.iter().copied())
                    .filter(|&object| {
                        target_matches(target, controller, object, state) == Some(true)
                            && crate::engine::object_mana_value(state, object) <= max_mv
                    })
                    .map(Target::Object)
                    .collect(),
            )
        }
        C8cTargetV1::UpToOneArtifactNonAuraEnchantmentOrLandV1 => Some(
            [PlayerId::P0, PlayerId::P1]
                .into_iter()
                .flat_map(|player| state.players[player.index()].battlefield.iter().copied())
                .filter(|&object| target_matches(target, controller, object, state) == Some(true))
                .map(Target::Object)
                .collect(),
        ),
    }
}
