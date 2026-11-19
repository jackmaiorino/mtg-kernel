//! Own-graveyard targets for Fanatic and Night Nurse, following P3h's
//! Auramancer filter and excluding tokens and spell copies.

use crate::card_def::{CardType, CARD_DEFS};
use crate::ids::{ObjectId, PlayerId};
use crate::standard_cards_v1::StandardTargetV1;
use crate::state::{GameState, Target, Zone};

pub(crate) fn owns(target: StandardTargetV1) -> bool {
    matches!(
        target,
        StandardTargetV1::UpToOneCreatureCardInOwnGraveyardV1
            | StandardTargetV1::PermanentCardInOwnGraveyardThisTurnV1
    )
}

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
            && if target == StandardTargetV1::PermanentCardInOwnGraveyardThisTurnV1 {
                super::permanent_card(live.card_def)
                    && super::entered_graveyard_this_turn(state, object)
            } else {
                CARD_DEFS[usize::from(live.card_def)].has_type(CardType::Creature)
            }
    }))
}

pub(crate) fn legal_targets(
    target: StandardTargetV1,
    controller: PlayerId,
    prefix: &[Target],
    state: &GameState,
) -> Option<Vec<Target>> {
    if !owns(target) {
        return None;
    }
    Some(if prefix.is_empty() {
        state.players[controller.index()]
            .graveyard
            .iter()
            .copied()
            .filter(|&object| target_matches(target, controller, object, state) == Some(true))
            .map(Target::Object)
            .collect()
    } else {
        Vec::new()
    })
}
