//! Flagbearer (Standard Bearer, Coalition Honor Guard): "While an opponent is
//! choosing targets as part of casting a spell they control or activating an
//! ability they control, that player must choose at least one Flagbearer on
//! the battlefield if able."
//!
//! The obligation is about the whole set of targets a cast or an activation
//! announces, so the engine applies it where it offers each target of a
//! pending cast or activation (`engine::cast_target_choices` and
//! `engine::activation_target_choices`), to the final legal roster: after the
//! per-card cast filters and the payability checks. "If able" means that a
//! complete legal assignment for the announced mode and target count names a
//! Flagbearer. Any player's Flagbearer counts, and one among all the targets
//! satisfies the obligation. Modes are chosen before targets (601.2b) and are
//! never constrained. For an "up to N" spec the engine interleaves the target
//! count with the targets (601.2c announces it first): finishing at a count
//! is refused only when an assignment of that count could have named a
//! Flagbearer, so a count that allows none, such as zero targets, stays
//! legal. Triggered abilities, copies whose new targets are chosen and
//! changed targets are not cast or activated and never reach this module.
//!
//! `shape` classifies every target spec by how its slots relate. It is an
//! exhaustive match over `TargetSpec`, `StandardTargetV1` and each wave's
//! filter enum with no wildcard arm, so a new target shape does not compile
//! until it is classified here, and the test below checks the classes
//! against the slot counts of every printed spell form and activated ability.
//! That test checks counts only: the shared-roster and independent-slot
//! rules rely on their class descriptions, so a shape whose rosters depend on
//! which objects were picked belongs in `DependentSlots`, which is searched.
//!
//! The rules probe hypothetical prefixes through the engine's candidate
//! function, so every per-card cast filter must judge the prefix it is given:
//! Pest Infestation's payable-X filter takes the length of the prefix being
//! extended (also the free-cast completion search's own prefix in
//! `resolution_cast_v1`), never the targets the pending cast holds.

use super::statics::is_flagbearer;
use crate::card_def::TargetSpec;
use crate::eternal_cube_c1a_v1::targets::C1aTargetV1;
use crate::eternal_cube_c1b_v1::targets::C1bTargetV1;
use crate::eternal_cube_c1c_v1::targets::C1cTargetV1;
use crate::eternal_cube_c2c_v1::targets::C2cTargetV1;
use crate::eternal_cube_c4a_v1::targets::C4aTargetV1;
use crate::eternal_cube_c4c_v1::targets::C4cTargetV1;
use crate::eternal_sideboard_s1d_v1::targets::S1dTargetV1;
use crate::standard_cards_v1::StandardTargetV1;
use crate::state::{GameState, Target};

/// How the target slots of a spec relate, which decides how the Flagbearer
/// obligation is checked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TargetShape {
    /// At most one target.
    Single,
    /// Every slot draws from one roster, which an earlier pick changes at
    /// most by excluding itself. The count may be capped by the cost or the
    /// caster's life, never by which objects were picked.
    SharedRoster,
    /// Picks from one roster, each pick a share of a divided amount, and an
    /// object may be picked again (one point of damage or one counter per
    /// pick).
    DividedAllocation,
    /// A fixed number of slots, each with its own roster that earlier picks
    /// do not change.
    IndependentSlots,
    /// At most three slots whose rosters differ and may depend on earlier
    /// picks.
    DependentSlots,
    /// Only the first slot can name a permanent; later slots take cards in
    /// graveyards.
    LeadingPermanentSlot,
    /// No slot can name a permanent on the battlefield.
    NoPermanentSlot,
}

/// The shape of `spec`.
pub(crate) fn shape(spec: TargetSpec) -> TargetShape {
    use TargetShape::*;
    match spec {
        TargetSpec::StandardV1(filter) => standard_shape(filter),
        TargetSpec::None
        | TargetSpec::AnyTarget
        | TargetSpec::AnySpellOnStack
        | TargetSpec::InstantSpellOnStack
        | TargetSpec::BlueSpellOnStack
        | TargetSpec::AnyPermanent
        | TargetSpec::BluePermanent
        | TargetSpec::AnyPlayer
        | TargetSpec::RedSpellOnStack
        | TargetSpec::RedPermanent
        | TargetSpec::NonlandPermanent
        | TargetSpec::Creature
        | TargetSpec::NonlegendaryCreature
        | TargetSpec::ArtifactOrEnchantmentSpellOnStack
        | TargetSpec::SorcerySpellOnStack
        | TargetSpec::NoncreatureSpellOnStack
        | TargetSpec::ArtifactSpellOnStack
        | TargetSpec::ArtifactPermanent
        | TargetSpec::CreatureOrLandCardInGraveyard
        | TargetSpec::ControlledCreature
        | TargetSpec::EnchantmentPermanent
        | TargetSpec::CreatureCardInOwnGraveyard
        | TargetSpec::TargetOpponent
        | TargetSpec::OpponentControlledCreature
        | TargetSpec::SpellManaValueAtMostControlledSubtypes { .. }
        | TargetSpec::CreatureOtherThanSource
        | TargetSpec::UpToOneTappedCreature
        | TargetSpec::NoncreatureArtifactPermanent
        | TargetSpec::Land
        | TargetSpec::OpponentArtifactOrEnchantmentPermanent
        | TargetSpec::NonblackCreature
        | TargetSpec::ArtifactOrEnchantmentPermanent
        | TargetSpec::AttackingOrBlockingCreature
        | TargetSpec::NonlandPermanentCardInOwnGraveyardManaValueAtMost(_)
        | TargetSpec::CreatureOrPlaneswalker
        | TargetSpec::ArtifactEnchantmentOrFlyingCreature
        | TargetSpec::ArtifactEnchantmentOrCreaturePowerAtLeastFour
        | TargetSpec::OpponentNonlandPermanent
        | TargetSpec::CreatureSpellOnStack
        | TargetSpec::NonOutlawCreature
        | TargetSpec::CreatureToughnessAtLeastFour
        | TargetSpec::CreatureEnchantmentOrPlaneswalker
        | TargetSpec::CreatureCardInOwnGraveyardManaValueAtMost(_)
        | TargetSpec::SpellYouDontControl
        | TargetSpec::AnotherControlledCreature
        | TargetSpec::ControlledCreatureWithSubtype(_)
        | TargetSpec::UpToOneCardInGraveyards
        | TargetSpec::AttackingCreatureWithSubtype(_)
        | TargetSpec::ControlledPermanentWithAnySubtype(_)
        | TargetSpec::PermanentCardInOwnGraveyard
        | TargetSpec::UpToOneOtherControlledPermanent
        | TargetSpec::CreaturePowerPlusToughnessAtMostFive
        | TargetSpec::NonartifactCreature
        | TargetSpec::ArtifactCreatureEnchantmentOrPlaneswalker
        | TargetSpec::AnotherAttackingCreature
        | TargetSpec::UpToOneOtherCreature
        | TargetSpec::ControlledNoncreatureArtifactPermanent
        | TargetSpec::CardInOwnGraveyardWithAnySubtype(_)
        | TargetSpec::OpponentArtifactEnchantmentOrNonbasicLand
        | TargetSpec::LegendaryCreature
        | TargetSpec::CreatureWithStunCounter
        | TargetSpec::StackObject
        | TargetSpec::StackAbility
        | TargetSpec::AnotherCreatureOrPlaneswalker
        | TargetSpec::UpToOneStackAbility
        | TargetSpec::AnotherArtifactOrCreature
        | TargetSpec::AttackingCreature
        | TargetSpec::CardInGraveyards
        | TargetSpec::AnotherNonBearCreatureCardInOwnGraveyardManaValueAtMost(_)
        | TargetSpec::SpellWithManaValueV1(_)
        | TargetSpec::AnotherControlledNonlandPermanent
        | TargetSpec::CreatureCardInOwnGraveyardManaValueAtMostSourcePower => Single,
        TargetSpec::UpToTwoCreatures
        | TargetSpec::ExactlyTwoArtifactPermanents
        | TargetSpec::UpToTwoOtherControlledCreatures
        // Kicker's "any number of target creatures you control".
        | TargetSpec::KickerControlledCreatures => SharedRoster,
        // Only a triggered ability distributes counters this way.
        TargetSpec::CounterDistribution => DividedAllocation,
        TargetSpec::PlayerThenTheirCreature
        | TargetSpec::ArtifactOrEnchantmentThenPlayer
        | TargetSpec::UpToTwoOtherCreaturesDifferentControllers
        | TargetSpec::TwoCreaturesWithDifferentControllers
        | TargetSpec::CreatureThenUpToOneAttachedEquipment
        | TargetSpec::ControlledCreatureThenOpponentCreature
        | TargetSpec::ControlledCreatureThenOpponentCreatureOrPlaneswalker => DependentSlots,
        TargetSpec::UpToTwoCreatureCardsInOwnGraveyard
        | TargetSpec::UpToTwoPlayers
        | TargetSpec::UpToTwoCardsInGraveyards
        | TargetSpec::UpToTwoCardsInSingleGraveyard => NoPermanentSlot,
    }
}

fn standard_shape(filter: StandardTargetV1) -> TargetShape {
    use StandardTargetV1 as S;
    use TargetShape::*;
    match filter {
        S::S1dV1(filter) => match filter {
            S1dTargetV1::CreaturePowerAtMostV1(_)
            | S1dTargetV1::CreatureWithFlyingV1
            | S1dTargetV1::NonFaerieSpellV1
            | S1dTargetV1::ControlledLandV1 => Single,
            S1dTargetV1::GraveyardCardsOfDistinctTypesV1 => NoPermanentSlot,
            // Return to Dust: "target artifact or enchantment" and "up to one
            // other target artifact or enchantment".
            S1dTargetV1::ArtifactOrEnchantmentThenUpToOneOtherV1 => SharedRoster,
        },
        S::C1cV1(filter) => match filter {
            C1cTargetV1::UpToOneCreatureOrPlaneswalkerV1 => Single,
            C1cTargetV1::KolaghanPairV1(..) => DependentSlots,
            C1cTargetV1::CreaturePicksPerLifeV1 => DividedAllocation,
        },
        S::C1aV1(filter) => match filter {
            // Mystic Confluence: spell slots, then creature slots, each of
            // which may name what another named.
            C1aTargetV1::MysticSlotsV1 { .. } => IndependentSlots,
        },
        S::C1bV1(filter) => match filter {
            // Pest Infestation, capped by the payable X.
            C1bTargetV1::ArtifactsAndEnchantmentsV1 => SharedRoster,
            // Collective Brutality: opponent, creature and opponent slots.
            C1bTargetV1::BrutalitySlotsV1(_) => IndependentSlots,
        },
        S::C2cV1(filter) => match filter {
            C2cTargetV1::LandsV1(_) => SharedRoster,
        },
        // The wave C4 filters are the targets of triggered abilities, which
        // the obligation never binds: Displacer Kitten's "up to one target
        // nonland permanent you control", Lutri, the Spellchaser's "target
        // instant or sorcery spell you control" (its copy's new targets are
        // not cast either) and Esika's Chariot's token.
        S::C4aV1(filter) => match filter {
            C4aTargetV1::UpToOneControlledNonlandPermanentV1
            | C4aTargetV1::ControlledInstantOrSorcerySpellV1 => Single,
        },
        S::C4cV1(filter) => match filter {
            C4cTargetV1::ControlledTokenV1 => Single,
        },
        S::UpToTwoAnyTargets
        | S::TwoCreaturesV1
        | S::OneOrTwoAnyTargetsV1
        | S::AnyNumberControlledNonlandNontokenPermanentsV1
        | S::TwoControlledArtifactsCreaturesOrLandsV1
        | S::OneToThreeDistinctAnyTargetsV1
        | S::UpToTwoControlledCreatures
        | S::UpToTwoControlledArtifactsOrCreatures
        | S::UpToEightCreaturesOrPlaneswalkersV74
        | S::TwoDistinctCreaturesV1
        | S::TwoLandsV1
        | S::UpToTwoArtifactsOrEnchantmentsV1
        | S::RepeatableArtifactsV1(_)
        // Kiora's Dismissal: casts use the strive-aware completion in the
        // engine, which follows the same rule.
        | S::AnyNumberEnchantmentsV1 => SharedRoster,
        S::DividedFourDamageCreaturesOrPlaneswalkersV1
        // Shatterskull Smashing: casts use the payable-allocation
        // completion in the engine.
        | S::DividedXDamageUpToTwoV1
        | S::DividedTwoDamageAnyTargetsV1
        | S::DividedFourDamageCreaturesV1
        | S::DividedTwoCountersCreaturesV1 => DividedAllocation,
        S::PlayerThenArtifactV1
        | S::PlayerThenCreatureV1
        | S::ArtifactThenCreatureV1
        | S::CreaturesThenNonlandPermanentsV1 { .. }
        | S::ControlledArtifactOrCreatureThenOpponentSmallCreatureV1 => IndependentSlots,
        S::ArtifactThenArtifactCardInItsControllersGraveyardV1
        | S::OpponentThenUpToOneTheirCreatureV1
        | S::ControlledCreatureThenUpToOneOpponentCreatureV1 => DependentSlots,
        S::CreatureThenGraveyardCardsV1 => LeadingPermanentSlot,
        S::PlayerThenPlayerV1
        | S::PlayerThenGraveyardCardsV1
        | S::UpToThreeLandCardsInOwnGraveyardV1
        | S::AnyNumberOfSpellsV1
        | S::PlayerThenUpToThreeOfTheirGraveyardCardsV1
        | S::PlayerThenUpToTwoOfTheirGraveyardCardsV1 => NoPermanentSlot,
        S::OpponentNonlandPermanentManaValueAtMost(_)
        | S::OpponentNonlandPermanent
        | S::OpponentArtifactOrCreature
        | S::ControlledArtifact
        | S::InstantOrSorceryCardInOwnGraveyard
        | S::AnotherNonlegendaryControlledCreature
        | S::AnotherNonlandPermanent
        | S::UpToOneCreature
        | S::CardInAGraveyard
        | S::TappedOpponentCreature
        | S::CrewedSourceThisTurn
        | S::NonbasicLandV1
        | S::InstantOrSorcerySpellV1
        | S::SpellWithSingleTargetV1
        | S::OpponentSpellOrNonlandPermanentV1
        | S::AnotherArtifactV1
        | S::AttackingClericRogueWarriorOrWizardV1
        | S::UpToOneNoncreatureArtifactV1
        | S::UpToOneOtherNonlandPermanentV1
        | S::TriggeredAbilityOrColorlessSpellV1
        | S::UpToOneColoredPermanentV1
        | S::ForestV1
        | S::UpToOneOpponentCreatureV1
        | S::ArtifactCardInOwnGraveyardV1
        | S::ArtifactCardInOwnGraveyardManaValueAtMostV1(_)
        | S::CreatureCardInAGraveyardV1
        | S::NonlandPermanentManaValueAtMostV1(_)
        | S::OpponentPermanentV1
        | S::UpToOneCreatureOrPlaneswalkerSpellV1
        | S::LandCardInAGraveyardV1
        | S::InstantOrSorceryCardInAGraveyardV1
        | S::UpToOneLandCardInOwnGraveyardV1
        | S::LandCardInOwnGraveyardV1
        | S::UpToOneCreatureWithTrampleOrHasteV1
        | S::CreatureCardPutIntoGraveyardThisTurnV1
        | S::CreatureLandOrPlaneswalkerV1
        | S::ArtifactEnchantmentOrFlyingCreatureV1
        | S::UpToOneControlledCreatureV1
        | S::CreatureOrVehicleV1
        | S::AnotherPermanentV1
        | S::UpToOneArtifactOrEnchantmentV1
        | S::UpToOneArtifactCreatureOrEnchantmentV1
        | S::ArtifactOrCreatureV1
        | S::UpToOneNonbasicLandV1
        | S::UpToOneOpponentNonlandNontokenManaValueAtMostV1(_)
        | S::UpToOneOpponentCreatureOrPlaneswalkerV1
        | S::UpToOneOpponentNoncreatureNonlandPermanentManaValueAtMostV1(_)
        | S::UpToOnePlayerV1
        | S::UpToOnePermanentCardDiedThisTurnV1
        | S::CreaturePowerAtMostTreasureCountersV1
        | S::AssemblyWorkerCreatureV1
        | S::CreatureManaValueAtMostV1(_)
        | S::OpponentArtifactV1
        | S::ArtifactOrLandV1
        | S::ControlledPermanentV1
        | S::CreatureOrPlaneswalkerManaValueAtMostV1(_)
        | S::OpponentCreatureOrEnchantmentV1
        | S::NoncreatureArtifactOrEnchantmentV1
        | S::BlackOrRedPermanentV1
        | S::CreaturePowerOrToughnessAtMostOneV1
        | S::EnchantmentInstantOrSorcerySpellV1
        | S::NonbasicLandCardInAGraveyardV1
        | S::ControlledCreatureOrVehicleV1
        | S::NonblackCreatureManaValueV1(_)
        | S::UpToOneOpponentArtifactCreatureOrPlaneswalkerV1
        | S::NoncreaturePermanentV1
        | S::EnchantmentCardInOwnGraveyardV1
        | S::ControlledArtifactOrCreatureV1
        | S::CreatureSpellPowerOrToughnessAtMostTwoV1 => Single,
    }
}

/// The targets the chooser may pick next for `spec` after `chosen`, and
/// whether the selection may finish now, while an opponent of the chooser
/// controls an active Flagbearer ability (`statics::flagbearer_requirement_applies`).
/// `candidates(prefix)` is the final legal roster for the slot after
/// `prefix`: every target that keeps the assignment completable and payable.
/// The counts are `min` to `max` targets, unbounded when `max` is `None`.
pub(crate) fn target_choices(
    state: &GameState,
    spec: TargetSpec,
    chosen: &[Target],
    min: usize,
    max: Option<usize>,
    candidates: impl Fn(&[Target]) -> Vec<Target>,
) -> (Vec<Target>, bool) {
    let next = if max.is_none_or(|max| chosen.len() < max) {
        candidates(chosen)
    } else {
        Vec::new()
    };
    if chosen.iter().any(|target| is_flagbearer(state, target)) {
        return (next, chosen.len() >= min);
    }
    let shape = shape(spec);
    match (shape, max) {
        (TargetShape::NoPermanentSlot, _) => (next, chosen.len() >= min),
        (TargetShape::Single, _) => shared_roster(state, chosen, min, max, next, &candidates),
        // With at most two slots the complete-assignment search is cheap and
        // needs no assumption about the rosters, so every shape uses it.
        (_, Some(max)) if max <= 2 => {
            complete_assignments(state, chosen, min, max, next, &candidates)
        }
        (TargetShape::SharedRoster | TargetShape::DividedAllocation, _) => {
            shared_roster(state, chosen, min, max, next, &candidates)
        }
        (TargetShape::IndependentSlots, Some(max)) => {
            independent_slots(state, chosen, min, max, next, &candidates)
        }
        (TargetShape::DependentSlots, Some(max)) => {
            complete_assignments(state, chosen, min, max, next, &candidates)
        }
        (TargetShape::LeadingPermanentSlot, _) => {
            leading_permanent_slot(state, chosen, min, next, &candidates)
        }
        // Only `KickerControlledCreatures` is unbounded, and it shares one
        // roster.
        (TargetShape::IndependentSlots | TargetShape::DependentSlots, None) => {
            shared_roster(state, chosen, min, max, next, &candidates)
        }
    }
}

fn any_flagbearer(state: &GameState, targets: &[Target]) -> bool {
    targets.iter().any(|target| is_flagbearer(state, target))
}

/// One roster for every slot. A pick that is not a Flagbearer never removes
/// one, so a Flagbearer available to any slot is available to the last slot
/// the count reaches. Finishing at the current count is refused when its
/// last slot could have taken a Flagbearer; a pick that is not a Flagbearer
/// is refused when no Flagbearer could follow it (a capped count or the
/// last slot).
fn shared_roster(
    state: &GameState,
    chosen: &[Target],
    min: usize,
    max: Option<usize>,
    mut next: Vec<Target>,
    candidates: &impl Fn(&[Target]) -> Vec<Target>,
) -> (Vec<Target>, bool) {
    let can_finish = chosen.len() >= min
        && chosen
            .split_last()
            .is_none_or(|(_, earlier)| !any_flagbearer(state, &candidates(earlier)));
    if any_flagbearer(state, &next) {
        let room_after = max.is_none_or(|max| chosen.len() + 1 < max);
        let others_keep_a_flagbearer = room_after
            && next
                .iter()
                .find(|target| !is_flagbearer(state, target))
                .is_some_and(|&other| {
                    let mut probe = chosen.to_vec();
                    probe.push(other);
                    any_flagbearer(state, &candidates(&probe))
                });
        if !others_keep_a_flagbearer {
            next.retain(|target| is_flagbearer(state, target));
        }
    }
    (next, can_finish)
}

/// A fixed number of slots whose rosters do not depend on earlier picks: a
/// slot must take a Flagbearer when it can and no later slot can, and a
/// complete selection without one may finish only when no slot could take
/// one.
fn independent_slots(
    state: &GameState,
    chosen: &[Target],
    min: usize,
    max: usize,
    mut next: Vec<Target>,
    candidates: &impl Fn(&[Target]) -> Vec<Target>,
) -> (Vec<Target>, bool) {
    debug_assert_eq!(min, max, "independent slots have a fixed count");
    if any_flagbearer(state, &next) {
        let mut later = false;
        let mut probe = chosen.to_vec();
        let mut pick = next.first().copied();
        while let Some(target) = pick {
            probe.push(target);
            if probe.len() >= max {
                break;
            }
            let roster = candidates(&probe);
            if any_flagbearer(state, &roster) {
                later = true;
                break;
            }
            pick = roster.first().copied();
        }
        if !later {
            next.retain(|target| is_flagbearer(state, target));
        }
    }
    let can_finish = chosen.len() >= min
        && (0..chosen.len()).all(|slot| !any_flagbearer(state, &candidates(&chosen[..slot])));
    (next, can_finish)
}

/// Only the first slot can name a permanent.
fn leading_permanent_slot(
    state: &GameState,
    chosen: &[Target],
    min: usize,
    mut next: Vec<Target>,
    candidates: &impl Fn(&[Target]) -> Vec<Target>,
) -> (Vec<Target>, bool) {
    if chosen.is_empty() && any_flagbearer(state, &next) {
        next.retain(|target| is_flagbearer(state, target));
    }
    let can_finish =
        chosen.len() >= min && (chosen.is_empty() || !any_flagbearer(state, &candidates(&[])));
    (next, can_finish)
}

/// The exact rule for few slots: for each count from the current one to
/// `max`, whether some complete assignment of that count names a
/// Flagbearer. A pick stays when some count can still be completed in a way
/// that meets that count's obligation, and the selection may finish at the
/// current count when that count has no obligation.
fn complete_assignments(
    state: &GameState,
    chosen: &[Target],
    min: usize,
    max: usize,
    mut next: Vec<Target>,
    candidates: &impl Fn(&[Target]) -> Vec<Target>,
) -> (Vec<Target>, bool) {
    debug_assert!(max <= 3, "the exact search is for few slots");
    let obligations: Vec<_> = (min.max(chosen.len())..=max)
        .map(|count| {
            (
                count,
                assignment_exists(state, &mut Vec::new(), count, true, candidates),
            )
        })
        .collect();
    let can_finish = obligations
        .iter()
        .any(|&(count, required)| count == chosen.len() && !required);
    let mut prefix = chosen.to_vec();
    next.retain(|&target| {
        prefix.push(target);
        let can_complete = obligations.iter().any(|&(count, required)| {
            count >= prefix.len()
                && assignment_exists(state, &mut prefix, count, required, candidates)
        });
        prefix.pop();
        can_complete
    });
    (next, can_finish)
}

/// Whether `prefix` extends to an assignment of `count` targets, naming a
/// Flagbearer when `requires_flagbearer`.
fn assignment_exists(
    state: &GameState,
    prefix: &mut Vec<Target>,
    count: usize,
    requires_flagbearer: bool,
    candidates: &impl Fn(&[Target]) -> Vec<Target>,
) -> bool {
    if prefix.len() == count {
        return !requires_flagbearer || any_flagbearer(state, prefix);
    }
    candidates(prefix).into_iter().any(|target| {
        prefix.push(target);
        let exists = assignment_exists(state, prefix, count, requires_flagbearer, candidates);
        prefix.pop();
        exists
    })
}

#[cfg(all(test, feature = "eternal-fixtures"))]
mod tests {
    use super::*;
    use crate::card_def::CARD_DEFS;

    /// The slot counts a spec announces, or `None` for the two specs whose
    /// count is not fixed by the spec.
    fn counts(spec: TargetSpec) -> Option<(u8, u8)> {
        (!matches!(
            spec,
            TargetSpec::KickerControlledCreatures | TargetSpec::CounterDistribution
        ))
        .then(|| {
            (
                crate::engine::target_count(spec),
                crate::engine::target_min_count(spec),
            )
        })
    }

    fn check(spec: TargetSpec, origin: &str) {
        let Some((max, min)) = counts(spec) else {
            return;
        };
        let shape = shape(spec);
        // A family whose parameter can give one slot may still be classified
        // for its wider members; a spec classified Single never has two.
        assert!(
            max <= 1 || shape != TargetShape::Single,
            "{origin}: {spec:?} is {shape:?} with {max} slots"
        );
        match shape {
            TargetShape::IndependentSlots => {
                assert_eq!(min, max, "{origin}: {spec:?} needs a fixed count")
            }
            TargetShape::DependentSlots => {
                assert!(max <= 3, "{origin}: {spec:?} is too wide to search")
            }
            TargetShape::LeadingPermanentSlot => {
                assert!(min >= 1, "{origin}: {spec:?} may skip its first slot")
            }
            _ => {}
        }
    }

    #[test]
    fn flagbearer_shapes_match_the_slot_counts_of_every_reachable_spec() {
        let mut seen = 0;
        for def in CARD_DEFS.iter() {
            let forms = 3 + def.additional_modes.len();
            for form in 0..=forms {
                let Ok(form) = u8::try_from(form) else {
                    continue;
                };
                if let Some(spec) = crate::engine::spell_form_target_spec(def, form) {
                    check(spec, def.name);
                    seen += 1;
                }
            }
            if let Some(spec) = def.kicked_target_spec {
                check(spec, def.name);
            }
            for ability in def.activated_abilities {
                check(ability.target_spec, def.name);
            }
        }
        assert!(seen > 0);
        // Specs built from a mode choice at cast time.
        for mask in 1..=7 {
            check(
                TargetSpec::StandardV1(StandardTargetV1::C1bV1(C1bTargetV1::BrutalitySlotsV1(
                    mask,
                ))),
                "Collective Brutality",
            );
        }
        for spells in 0..=3 {
            for creatures in 0..=3 - spells {
                check(
                    TargetSpec::StandardV1(StandardTargetV1::C1aV1(C1aTargetV1::MysticSlotsV1 {
                        spells,
                        creatures,
                    })),
                    "Mystic Confluence",
                );
                check(
                    TargetSpec::StandardV1(StandardTargetV1::CreaturesThenNonlandPermanentsV1 {
                        creatures: spells,
                        permanents: creatures,
                    }),
                    "Eldrazi Confluence",
                );
            }
        }
        for count in 0..=3 {
            check(
                TargetSpec::StandardV1(StandardTargetV1::RepeatableArtifactsV1(count)),
                "Fiery Confluence",
            );
            check(
                TargetSpec::StandardV1(StandardTargetV1::C2cV1(C2cTargetV1::LandsV1(count))),
                "lands",
            );
        }
    }
}
