//! Target filters of the wave E9a cards, carried as `StandardTargetV1`
//! variants. `standard_cards_v1::legal_targets` and `target_matches` ask
//! these first.
//!
//! Divided damage (601.2d) is announced with the targets: each pick of a
//! divided-damage filter assigns one point of damage, so a creature picked
//! twice is one target dealt two. Fury picks exactly four times. Shatterskull
//! Smashing, whose X is announced after its targets here, picks its first
//! target k times and may then pick a second target once: the first is dealt
//! k damage and the second the rest, and X may not be so small that the
//! second would be dealt none (`minimum_x`).

use crate::card_def::{CardType, CARD_DEFS};
use crate::event::CommittedEvent;
use crate::ids::{ObjectId, PlayerId};
use crate::standard_cards_v1::StandardTargetV1;
use crate::state::{GameState, Target, Zone};

/// The most picks Shatterskull Smashing's division takes: its first target
/// is dealt at most 15 damage, and the rest goes to the second.
pub(crate) const SHATTERSKULL_PICKS: u8 = 16;

/// Whether `target` is one of this module's filters.
pub(crate) fn owns(target: StandardTargetV1) -> bool {
    use StandardTargetV1 as S;
    matches!(
        target,
        S::DividedFourDamageCreaturesOrPlaneswalkersV1
            | S::DividedXDamageUpToTwoV1
            | S::UpToOnePlayerV1
            | S::UpToOnePermanentCardDiedThisTurnV1
    )
}

/// Whether this module's filter `target` takes graveyard cards.
pub(crate) fn in_graveyard(target: StandardTargetV1) -> bool {
    target == StandardTargetV1::UpToOnePermanentCardDiedThisTurnV1
}

/// How many targets this module's filter takes, and how many at least.
pub(crate) fn counts(target: StandardTargetV1) -> Option<(u8, u8)> {
    use StandardTargetV1 as S;
    Some(match target {
        S::DividedFourDamageCreaturesOrPlaneswalkersV1 => (4, 4),
        S::DividedXDamageUpToTwoV1 => (SHATTERSKULL_PICKS, 0),
        S::UpToOnePlayerV1 | S::UpToOnePermanentCardDiedThisTurnV1 => (1, 0),
        _ => return None,
    })
}

/// A permanent card: one with a permanent card type (110.4a).
fn permanent_card(object: ObjectId, state: &GameState) -> bool {
    let Some(def) = state
        .objects
        .try_get(object)
        .and_then(|live| CARD_DEFS.get(usize::from(live.card_def)))
    else {
        return false;
    };
    [
        CardType::Artifact,
        CardType::Creature,
        CardType::Enchantment,
        CardType::Land,
        CardType::Planeswalker,
    ]
    .into_iter()
    .any(|card_type| def.has_type(card_type))
}

/// Whether `card`'s current graveyard incarnation began when it was put
/// there from the battlefield this turn. The turn begins at its
/// `UpkeepBegan` marker; nothing leaves the battlefield during an untap
/// step.
pub(crate) fn died_this_turn(state: &GameState, card: ObjectId) -> bool {
    for event in state.engine.event_history.iter().rev() {
        match event {
            CommittedEvent::UpkeepBegan { .. } => return false,
            CommittedEvent::ZoneChange {
                object, from, to, ..
            } if *object == card => {
                return *to == Zone::Graveyard && *from == Zone::Battlefield;
            }
            _ => {}
        }
    }
    false
}

/// Whether `object` satisfies this module's filter `target` for
/// `controller`, or `None` for every other filter.
pub(crate) fn target_matches(
    target: StandardTargetV1,
    controller: PlayerId,
    object: ObjectId,
    state: &GameState,
) -> Option<bool> {
    use StandardTargetV1 as S;
    if !owns(target) {
        return None;
    }
    let Some(live) = state.objects.try_get(object) else {
        return Some(false);
    };
    let has = |card_type| crate::engine::object_has_type(state, object, card_type);
    Some(match target {
        S::DividedFourDamageCreaturesOrPlaneswalkersV1 | S::DividedXDamageUpToTwoV1 => {
            live.zone == Zone::Battlefield
                && (has(CardType::Creature) || has(CardType::Planeswalker))
        }
        S::UpToOnePermanentCardDiedThisTurnV1 => {
            live.zone == Zone::Graveyard
                && live.owner == controller
                && !live.v4.is_token
                && live.spell_copy_origin.is_none()
                && permanent_card(object, state)
                && died_this_turn(state, object)
        }
        _ => false,
    })
}

/// The legal choices for this module's filter `target` after the already
/// chosen `prefix`, or `None` for every other filter.
pub(crate) fn legal_targets(
    target: StandardTargetV1,
    controller: PlayerId,
    prefix: &[Target],
    state: &GameState,
) -> Option<Vec<Target>> {
    use StandardTargetV1 as S;
    if !owns(target) {
        return None;
    }
    let (maximum, _) = counts(target)?;
    if prefix.len() >= usize::from(maximum) {
        return Some(Vec::new());
    }
    if target == S::UpToOnePlayerV1 {
        return Some(vec![
            Target::Player(PlayerId::P0),
            Target::Player(PlayerId::P1),
        ]);
    }
    if target == S::UpToOnePermanentCardDiedThisTurnV1 {
        return Some(
            state.players[controller.index()]
                .graveyard
                .iter()
                .copied()
                .filter(|&card| target_matches(target, controller, card, state) == Some(true))
                .map(Target::Object)
                .collect(),
        );
    }
    let candidates = [PlayerId::P0, PlayerId::P1]
        .into_iter()
        .flat_map(|player| state.players[player.index()].battlefield.iter().copied())
        .filter(|&id| target_matches(target, controller, id, state) == Some(true));
    if target == S::DividedXDamageUpToTwoV1 {
        // The first target may be picked again until a second target is
        // picked, which ends the announcement.
        let first = match prefix.first() {
            Some(Target::Object(first)) => Some(*first),
            Some(_) => return Some(Vec::new()),
            None => None,
        };
        if let Some(first) = first {
            if prefix.iter().any(|&pick| pick != Target::Object(first)) {
                return Some(Vec::new());
            }
        }
        return Some(candidates.map(Target::Object).collect());
    }
    Some(candidates.map(Target::Object).collect())
}

/// The division of a divided-damage spell or ability's damage: each target
/// with the damage it is dealt, in first-pick order. Fury's four picks are
/// one point each. Shatterskull Smashing's first target is dealt one point
/// per pick and its second target, if any, the rest of `total`.
pub(crate) fn division(
    target: StandardTargetV1,
    picks: &[Target],
    total: i32,
) -> Vec<(Target, i32)> {
    let mut out: Vec<(Target, i32)> = Vec::new();
    if target == StandardTargetV1::DividedXDamageUpToTwoV1 {
        let Some(&first) = picks.first() else {
            return out;
        };
        let first_count = picks.iter().filter(|&&pick| pick == first).count();
        let first_count = i32::try_from(first_count).unwrap_or(i32::MAX);
        match picks.iter().copied().find(|&pick| pick != first) {
            Some(second) => {
                out.push((first, first_count.min(total)));
                out.push((second, total.saturating_sub(first_count).max(0)));
            }
            None => out.push((first, total)),
        }
        return out;
    }
    for &pick in picks {
        match out.iter_mut().find(|(target, _)| *target == pick) {
            Some((_, amount)) => *amount = amount.saturating_add(1),
            None => out.push((pick, 1)),
        }
    }
    out
}

/// Shatterskull Smashing: the damage it deals for `x`.
pub(crate) fn shatterskull_total(x: u16) -> i32 {
    let x = i32::from(x);
    if x >= 6 {
        x.saturating_mul(2)
    } else {
        x
    }
}

/// The least X for Shatterskull Smashing with these targets: with a second
/// target, enough damage that it is dealt at least 1 after the first
/// target's picks.
pub(crate) fn shatterskull_minimum_x(targets: &[Target]) -> u8 {
    let Some(&first) = targets.first() else {
        return 0;
    };
    if !targets.iter().any(|&pick| pick != first) {
        return 0;
    }
    let first_count = targets.iter().filter(|&&pick| pick == first).count();
    let needed = i32::try_from(first_count)
        .unwrap_or(i32::MAX)
        .saturating_add(1);
    (0..=u8::MAX)
        .find(|&x| shatterskull_total(u16::from(x)) >= needed)
        .unwrap_or(u8::MAX)
}
