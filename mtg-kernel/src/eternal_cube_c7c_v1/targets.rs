//! Target filters of Vintage Cube wave C7c, carried as a `StandardTargetV1`
//! variant. `standard_cards_v1::legal_targets` and `target_matches` ask these
//! before falling through to their built-in match.

use crate::card_def::{CardType, CARD_DEFS};
use crate::effect::EffectObjectBinding;
use crate::ids::{ObjectId, PlayerId};
use crate::standard_cards_v1::StandardTargetV1;
use crate::state::{GameState, Target, Zone};

/// Whether `target` is owned by wave C7c.
pub(crate) fn owns(target: StandardTargetV1) -> bool {
    target == StandardTargetV1::NonlandPermanentCardInOwnGraveyardManaValueAtMostSourcePowerV1
}

/// Whether `card_def` is a nonland permanent card (creature, artifact,
/// enchantment, or planeswalker, and not a land).
fn nonland_permanent_card(card_def: u16) -> bool {
    CARD_DEFS.get(usize::from(card_def)).is_some_and(|def| {
        !def.has_type(CardType::Land)
            && [
                CardType::Creature,
                CardType::Artifact,
                CardType::Enchantment,
                CardType::Planeswalker,
            ]
            .into_iter()
            .any(|kind| def.has_type(kind))
    })
}

/// The power of the attacking source incarnation `(object, zone_change_count)`:
/// its current power while that battlefield incarnation remains, otherwise its
/// last-known power as it left the battlefield (608.2b, 113.7a).
fn source_power(state: &GameState, source: (ObjectId, u32)) -> Option<i32> {
    crate::creature_stats_lki_v1::current_or_last_creature_stats(
        state,
        EffectObjectBinding {
            object: source.0,
            expected_zone: Zone::Battlefield,
            expected_zone_change_count: source.1,
        },
    )
    .map(|(power, _)| power)
}

/// Whether `object` is a physical nonland permanent card in `controller`'s
/// graveyard (the source-power bound is applied in `legal_targets`, which
/// knows the attacking creature).
pub(crate) fn target_matches(
    target: StandardTargetV1,
    controller: PlayerId,
    object: ObjectId,
    state: &GameState,
) -> Option<bool> {
    if !owns(target) {
        return None;
    }
    Some(state.objects.try_get(object).is_some_and(|live| {
        live.zone == Zone::Graveyard
            && live.owner == controller
            && !live.v4.is_token
            && live.spell_copy_origin.is_none()
            && nonland_permanent_card(live.card_def)
    }))
}

/// Legal choices for Guardian Scalelord's attack trigger (both its own printed
/// trigger and the Backup-granted trigger on another creature): a nonland
/// permanent card in `controller`'s graveyard whose mana value is at most the
/// attacking creature's power. Fails closed when no source is supplied or its
/// power is negative.
pub(crate) fn legal_targets(
    target: StandardTargetV1,
    controller: PlayerId,
    source: Option<(ObjectId, u32)>,
    prefix: &[Target],
    state: &GameState,
) -> Option<Vec<Target>> {
    if !owns(target) {
        return None;
    }
    if !prefix.is_empty() {
        return Some(Vec::new());
    }
    let Some(power) = source.and_then(|src| source_power(state, src)) else {
        return Some(Vec::new());
    };
    if power < 0 {
        return Some(Vec::new());
    }
    Some(
        state.players[controller.index()]
            .graveyard
            .iter()
            .copied()
            .filter(|&object| {
                target_matches(target, controller, object, state) == Some(true)
                    && i32::from(crate::engine::object_mana_value(state, object)) <= power
            })
            .map(Target::Object)
            .collect(),
    )
}
