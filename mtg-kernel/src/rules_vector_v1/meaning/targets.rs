//! Meaning table for `TargetSpec`.
//!
//! Every arm restates the legal pool `engine::legal_targets_for_controller_
//! from_source` builds for the spec, and the cardinality `engine::target_count`
//! / `target_min_count` give it. The pool filters that apply to every spec
//! alike (hexproof against the opponent's targeting, protection from
//! monocolored sources) are not spec facts and are not emitted here.
//!
//! An `Object` atom names one class of legal object. A spec whose pool is a
//! union ("artifact or enchantment") emits one atom per class. Object atoms on
//! the battlefield use `controller` for the controlling player; atoms in a
//! graveyard use it for the owning player, which is what the engine checks
//! there (`object.owner == controller`).

use super::*;

/// Legality facts of a target specification.
pub(crate) fn target_spec(spec: TargetSpec, out: &mut Collector) {
    match spec {
        TargetSpec::StandardV1(filter) => standard_target_spec(filter, out),
        // Target count 0: nothing is announced.
        TargetSpec::None => {}
        TargetSpec::ArtifactOrEnchantmentThenPlayer => {
            target_spec(TargetSpec::ArtifactOrEnchantmentPermanent, out);
            target_spec(TargetSpec::AnyPlayer, out);
            out.target(TargetAtom::MultipleTargets);
        }
        TargetSpec::UpToOneOtherControlledPermanent => {
            object(out, ObjF::Permanent, Some(RelF::You), ZoneF::Battlefield);
            out.target(TargetAtom::UpTo);
            // Vocabulary has no exact-source-incarnation exclusion facet.
            out.atoms.push(Atom::Opaque);
        }
        TargetSpec::CounterDistribution => {
            object(out, creature(), Some(RelF::You), ZoneF::Battlefield);
            out.target(TargetAtom::MultipleTargets);
            out.target(TargetAtom::UpTo);
        }
        TargetSpec::UpToTwoOtherControlledCreatures => {
            object(out, creature(), Some(RelF::You), ZoneF::Battlefield);
            out.target(TargetAtom::MultipleTargets);
            out.target(TargetAtom::UpTo);
            out.atoms.push(Atom::Opaque);
        }
        // Both players and creatures, plus planeswalkers in Standard.
        TargetSpec::AnyTarget => {
            out.target(TargetAtom::PlayerYou);
            out.target(TargetAtom::PlayerOpponent);
            object(out, creature(), None, ZoneF::Battlefield);
            #[cfg(feature = "standard-magezero-fixtures")]
            object(out, typed(CardType::Planeswalker), None, ZoneF::Battlefield);
        }
        // Either player; never an object.
        TargetSpec::AnyPlayer => {
            out.target(TargetAtom::PlayerYou);
            out.target(TargetAtom::PlayerOpponent);
        }
        // First pick: either player. Second pick: a creature that player
        // controls (the pool is computed from the first pick).
        TargetSpec::PlayerThenTheirCreature => {
            out.target(TargetAtom::PlayerYou);
            out.target(TargetAtom::PlayerOpponent);
            object(
                out,
                creature(),
                Some(RelF::ChosenPlayer),
                ZoneF::Battlefield,
            );
            out.target(TargetAtom::MultipleTargets);
        }
        // Any spell stack item (abilities excluded by `StackItemKind::Spell`).
        TargetSpec::StackObject | TargetSpec::StackAbility => {
            object(out, ObjF::AnyCard, None, ZoneF::Stack)
        }
        TargetSpec::AnotherCreatureOrPlaneswalker => {
            object(out, ObjF::Permanent, None, ZoneF::Battlefield);
        }
        TargetSpec::AnySpellOnStack => object(out, ObjF::Spell, None, ZoneF::Stack),
        TargetSpec::InstantSpellOnStack => {
            object(out, typed(CardType::Instant), None, ZoneF::Stack)
        }
        TargetSpec::BlueSpellOnStack => {
            colored(out, ObjF::Spell, ZoneF::Stack, crate::mana::ManaColor::U)
        }
        TargetSpec::RedSpellOnStack => {
            colored(out, ObjF::Spell, ZoneF::Stack, crate::mana::ManaColor::R)
        }
        TargetSpec::ArtifactOrEnchantmentSpellOnStack => {
            object(out, typed(CardType::Artifact), None, ZoneF::Stack);
            object(out, typed(CardType::Enchantment), None, ZoneF::Stack);
        }
        TargetSpec::SorcerySpellOnStack => {
            object(out, typed(CardType::Sorcery), None, ZoneF::Stack)
        }
        // Any spell lacking the creature type. No negated-type facet exists;
        // the nearest class is "a spell".
        TargetSpec::NoncreatureSpellOnStack => out.target(TargetAtom::Object {
            obj: ObjF::Spell,
            controller: None,
            zone: ZoneF::Stack,
            color: None,
            mana_value_at_most: None,
            excludes: Some(CardTypeF::Creature),
        }),
        TargetSpec::CreatureSpellOnStack => object(out, creature(), None, ZoneF::Stack),
        TargetSpec::ArtifactSpellOnStack => {
            object(out, typed(CardType::Artifact), None, ZoneF::Stack)
        }
        TargetSpec::AnyPermanent => object(out, ObjF::Permanent, None, ZoneF::Battlefield),
        TargetSpec::BluePermanent => colored(
            out,
            ObjF::Permanent,
            ZoneF::Battlefield,
            crate::mana::ManaColor::U,
        ),
        TargetSpec::RedPermanent => colored(
            out,
            ObjF::Permanent,
            ZoneF::Battlefield,
            crate::mana::ManaColor::R,
        ),
        TargetSpec::NonlandPermanent => {
            object(out, ObjF::NonlandPermanent, None, ZoneF::Battlefield)
        }
        TargetSpec::Creature => object(out, creature(), None, ZoneF::Battlefield),
        // A creature without the Legendary supertype. No supertype facet
        // exists; the nearest class is "a creature".
        TargetSpec::NonlegendaryCreature => object(out, creature(), None, ZoneF::Battlefield),
        TargetSpec::ArtifactPermanent => {
            object(out, typed(CardType::Artifact), None, ZoneF::Battlefield)
        }
        TargetSpec::CreatureOrLandCardInGraveyard => {
            object(out, creature(), None, ZoneF::Graveyard);
            object(out, typed(CardType::Land), None, ZoneF::Graveyard);
        }
        TargetSpec::ControlledCreature => {
            object(out, creature(), Some(RelF::You), ZoneF::Battlefield)
        }
        // Up to two nontoken creature cards the targeting player owns, in
        // that player's graveyard.
        TargetSpec::UpToTwoCreatureCardsInOwnGraveyard => {
            object(out, creature(), Some(RelF::You), ZoneF::Graveyard);
            out.target(TargetAtom::MultipleTargets);
            out.target(TargetAtom::UpTo);
        }
        // Up to two distinct creatures on either battlefield.
        TargetSpec::UpToTwoCreatures => {
            object(out, creature(), None, ZoneF::Battlefield);
            out.target(TargetAtom::MultipleTargets);
            out.target(TargetAtom::UpTo);
        }
        // Exactly two distinct artifacts (minimum equals maximum).
        TargetSpec::ExactlyTwoArtifactPermanents => {
            object(out, typed(CardType::Artifact), None, ZoneF::Battlefield);
            out.target(TargetAtom::MultipleTargets);
        }
        TargetSpec::EnchantmentPermanent => {
            object(out, typed(CardType::Enchantment), None, ZoneF::Battlefield)
        }
        // Up to two distinct players from {P0, P1}.
        TargetSpec::UpToTwoPlayers => {
            out.target(TargetAtom::PlayerYou);
            out.target(TargetAtom::PlayerOpponent);
            out.target(TargetAtom::MultipleTargets);
            out.target(TargetAtom::UpTo);
        }
        TargetSpec::CreatureCardInOwnGraveyard => {
            object(out, creature(), Some(RelF::You), ZoneF::Graveyard)
        }
        // The pool is exactly `controller.opponent()`.
        TargetSpec::TargetOpponent => out.target(TargetAtom::PlayerOpponent),
        TargetSpec::OpponentControlledCreature => {
            object(out, creature(), Some(RelF::Opponent), ZoneF::Battlefield)
        }
        // A spell whose mana value is at most the number of battlefield
        // permanents the targeting player controls with either subtype. The
        // bound is read from the game state at targeting time, so the fixed
        // `mana_value_at_most` bound stays empty and the count is a read.
        TargetSpec::SpellManaValueAtMostControlledSubtypes { first, second } => {
            // The subtypes select which permanents are counted. The facet
            // vocabulary has no subtype facet, so they are not representable
            // here and the read uses the nearest class, "a permanent".
            let _ = (first, second);
            out.read(
                RelF::You,
                Some(ZoneF::Battlefield),
                Some(ObjF::Permanent),
                AggF::Count,
            );
            object(out, ObjF::Spell, None, ZoneF::Stack);
        }
        // Up to two distinct nontoken cards in either graveyard.
        TargetSpec::UpToTwoCardsInGraveyards => {
            object(out, ObjF::AnyCard, None, ZoneF::Graveyard);
            out.target(TargetAtom::MultipleTargets);
            out.target(TargetAtom::UpTo);
        }
        // A creature other than the targeting source. The exclusion of the
        // source itself has no facet; the nearest class is "a creature".
        TargetSpec::CreatureOtherThanSource => object(out, creature(), None, ZoneF::Battlefield),
        // Zero or one tapped creature. The tapped requirement has no facet;
        // the nearest class is "a creature".
        TargetSpec::UpToOneTappedCreature => {
            object(out, creature(), None, ZoneF::Battlefield);
            out.target(TargetAtom::UpTo);
        }
        // An artifact without the creature type. No negated-type facet
        // exists; the nearest class is "an artifact".
        TargetSpec::UpToOneStackAbility => {
            object(out, ObjF::AnyCard, None, ZoneF::Stack);
            out.target(TargetAtom::UpTo);
        }
        TargetSpec::CreatureWithStunCounter => object(out, creature(), None, ZoneF::Battlefield),
        TargetSpec::ControlledNoncreatureArtifactPermanent => object(
            out,
            typed(CardType::Artifact),
            Some(RelF::You),
            ZoneF::Battlefield,
        ),
        TargetSpec::NoncreatureArtifactPermanent => {
            object(out, typed(CardType::Artifact), None, ZoneF::Battlefield)
        }
        TargetSpec::Land => object(out, typed(CardType::Land), None, ZoneF::Battlefield),
        TargetSpec::OpponentArtifactOrEnchantmentPermanent => {
            object(
                out,
                typed(CardType::Artifact),
                Some(RelF::Opponent),
                ZoneF::Battlefield,
            );
            object(
                out,
                typed(CardType::Enchantment),
                Some(RelF::Opponent),
                ZoneF::Battlefield,
            );
        }
        // A creature that is not black. No negated-color facet exists; the
        // nearest class is "a creature" with no color filter.
        TargetSpec::NonblackCreature => object(out, creature(), None, ZoneF::Battlefield),
        TargetSpec::ArtifactOrEnchantmentPermanent => {
            object(out, typed(CardType::Artifact), None, ZoneF::Battlefield);
            object(out, typed(CardType::Enchantment), None, ZoneF::Battlefield);
        }
        // First pick: a creature the targeting player controls. Second pick:
        // a creature a different player (the opponent) controls.
        TargetSpec::ControlledCreatureThenOpponentCreature => {
            object(out, creature(), Some(RelF::You), ZoneF::Battlefield);
            object(out, creature(), Some(RelF::Opponent), ZoneF::Battlefield);
            out.target(TargetAtom::MultipleTargets);
        }
        TargetSpec::ControlledCreatureThenOpponentCreatureOrPlaneswalker => {
            object(out, creature(), Some(RelF::You), ZoneF::Battlefield);
            object(out, creature(), Some(RelF::Opponent), ZoneF::Battlefield);
            object(
                out,
                typed(CardType::Planeswalker),
                Some(RelF::Opponent),
                ZoneF::Battlefield,
            );
            out.target(TargetAtom::MultipleTargets);
        }
        // A creature among the attackers or blockers of the current combat.
        // No combat-status facet exists; the nearest class is "a creature".
        TargetSpec::AttackingOrBlockingCreature => {
            object(out, creature(), None, ZoneF::Battlefield)
        }
        // A nontoken, nonland permanent card (creature, artifact,
        // enchantment or planeswalker) the targeting player owns, in that
        // player's graveyard, with printed mana value at most `maximum`.
        TargetSpec::PermanentCardInOwnGraveyard => {
            object(out, ObjF::Permanent, Some(RelF::You), ZoneF::Graveyard);
        }
        TargetSpec::NonlandPermanentCardInOwnGraveyardManaValueAtMost(maximum) => {
            out.target(TargetAtom::Object {
                obj: ObjF::NonlandPermanent,
                controller: Some(RelF::You),
                zone: ZoneF::Graveyard,
                color: None,
                mana_value_at_most: Some(mana_value_bucket(maximum)),
                excludes: None,
            });
        }
        TargetSpec::CreatureOrPlaneswalker => {
            object(out, creature(), None, ZoneF::Battlefield);
            object(out, typed(CardType::Planeswalker), None, ZoneF::Battlefield);
        }
        // Artifact, enchantment, or creature with (effective) flying. The
        // flying requirement has no facet; the nearest class is "a creature".
        TargetSpec::ArtifactEnchantmentOrFlyingCreature => {
            object(out, typed(CardType::Artifact), None, ZoneF::Battlefield);
            object(out, typed(CardType::Enchantment), None, ZoneF::Battlefield);
            object(out, creature(), None, ZoneF::Battlefield);
        }
        // Artifact, enchantment, or creature with effective power >= 4. The
        // power requirement has no facet; the nearest class is "a creature".
        TargetSpec::ArtifactEnchantmentOrCreaturePowerAtLeastFour => {
            object(out, typed(CardType::Artifact), None, ZoneF::Battlefield);
            object(out, typed(CardType::Enchantment), None, ZoneF::Battlefield);
            object(out, creature(), None, ZoneF::Battlefield);
        }
        // A permanent a different player (the opponent) controls, without
        // the land type.
        TargetSpec::OpponentNonlandPermanent => object(
            out,
            ObjF::NonlandPermanent,
            Some(RelF::Opponent),
            ZoneF::Battlefield,
        ),
        // A creature without an outlaw type. No creature-type facet exists;
        // the nearest class is "a creature".
        TargetSpec::CreaturePowerPlusToughnessAtMostFive
        | TargetSpec::NonartifactCreature
        | TargetSpec::UpToOneOtherCreature
        | TargetSpec::AnotherAttackingCreature
        | TargetSpec::NonOutlawCreature => object(out, creature(), None, ZoneF::Battlefield),
        // A creature with effective toughness >= 4. The toughness
        // requirement has no facet; the nearest class is "a creature".
        TargetSpec::CreatureToughnessAtLeastFour => {
            object(out, creature(), None, ZoneF::Battlefield)
        }
        TargetSpec::ArtifactCreatureEnchantmentOrPlaneswalker => {
            object(out, creature(), None, ZoneF::Battlefield);
            object(out, typed(CardType::Artifact), None, ZoneF::Battlefield);
            object(out, typed(CardType::Enchantment), None, ZoneF::Battlefield);
            object(out, typed(CardType::Planeswalker), None, ZoneF::Battlefield);
        }
        TargetSpec::CreatureEnchantmentOrPlaneswalker => {
            object(out, creature(), None, ZoneF::Battlefield);
            object(out, typed(CardType::Enchantment), None, ZoneF::Battlefield);
            object(out, typed(CardType::Planeswalker), None, ZoneF::Battlefield);
        }
        // A creature card the targeting player owns, in that player's
        // graveyard, with printed mana value at most `maximum`.
        TargetSpec::CreatureCardInOwnGraveyardManaValueAtMost(maximum) => {
            out.target(TargetAtom::Object {
                obj: creature(),
                controller: Some(RelF::You),
                zone: ZoneF::Graveyard,
                color: None,
                mana_value_at_most: Some(mana_value_bucket(maximum)),
                excludes: None,
            });
        }
        // A spell stack item controlled by the other player.
        TargetSpec::SpellYouDontControl => {
            object(out, ObjF::Spell, Some(RelF::Opponent), ZoneF::Stack)
        }
        // A creature the targeting player controls other than the source
        // (or than the creature its trigger names). The exclusion has no
        // facet; the nearest class is "a creature you control".
        TargetSpec::AnotherControlledCreature => {
            object(out, creature(), Some(RelF::You), ZoneF::Battlefield)
        }
        // A creature the targeting player controls with one subtype. No
        // subtype facet exists; the nearest class is "a creature you
        // control".
        TargetSpec::ControlledCreatureWithSubtype(subtype) => {
            let _ = subtype;
            object(out, creature(), Some(RelF::You), ZoneF::Battlefield)
        }
        // Zero or one nontoken card in either graveyard.
        TargetSpec::UpToOneCardInGraveyards => {
            object(out, ObjF::AnyCard, None, ZoneF::Graveyard);
            out.target(TargetAtom::UpTo);
        }
        // One attacking creature with this effective subtype, either
        // controller. Vocabulary gap: no attacking or subtype filter.
        TargetSpec::AttackingCreatureWithSubtype(subtype) => {
            let _ = subtype;
            object(out, creature(), None, ZoneF::Battlefield)
        }
        // One permanent the targeting player controls with any of these
        // effective subtypes. Vocabulary gap: no subtype filter.
        TargetSpec::CardInOwnGraveyardWithAnySubtype(_) => {
            object(out, ObjF::AnyCard, Some(RelF::You), ZoneF::Graveyard)
        }
        TargetSpec::OpponentArtifactEnchantmentOrNonbasicLand => object(
            out,
            ObjF::Permanent,
            Some(RelF::Opponent),
            ZoneF::Battlefield,
        ),
        TargetSpec::AnotherArtifactOrCreature => {
            object(out, ObjF::Permanent, None, ZoneF::Battlefield)
        }
        TargetSpec::UpToTwoOtherCreaturesDifferentControllers => {
            object(out, creature(), None, ZoneF::Battlefield)
        }
        TargetSpec::LegendaryCreature => object(out, creature(), None, ZoneF::Battlefield),
        TargetSpec::ControlledPermanentWithAnySubtype(subtypes) => {
            let _ = subtypes;
            object(out, ObjF::Permanent, Some(RelF::You), ZoneF::Battlefield)
        }
    }
}

/// The object class that `Target(slot)` can denote under `spec`.
///
/// For a union pool this is the narrowest single class covering every member
/// of the pool. For a spec with one target slot every slot maps to that class
/// (the engine never resolves a slot past `target_count`).
pub(crate) fn target_slot_obj(spec: TargetSpec, slot: u8) -> ObjF {
    match spec {
        TargetSpec::CardInOwnGraveyardWithAnySubtype(_) => ObjF::AnyCard,
        TargetSpec::OpponentArtifactEnchantmentOrNonbasicLand => ObjF::Permanent,
        TargetSpec::StackObject
        | TargetSpec::StackAbility
        | TargetSpec::AnotherCreatureOrPlaneswalker => ObjF::AnyCard,
        TargetSpec::AnotherArtifactOrCreature => ObjF::Permanent,
        TargetSpec::UpToTwoOtherCreaturesDifferentControllers => creature(),
        TargetSpec::LegendaryCreature => creature(),
        TargetSpec::StandardV1(filter) => standard_target_obj(filter),
        // No targets are chosen, so the engine's `resolve_object` would
        // panic on any `Target(slot)`; no well-formed program refers to one.
        // The unfiltered class keeps the extractor total.
        TargetSpec::None => {
            let _ = slot; // No slot exists to distinguish.
            ObjF::AnyCard
        }
        TargetSpec::AnyTarget => {
            let _ = slot; // One target slot.
            ObjF::PlayerOrPermanent
        }
        TargetSpec::PlayerThenTheirCreature => match slot {
            0 => ObjF::Player,
            1..=u8::MAX => creature(),
        },
        TargetSpec::AnyPlayer | TargetSpec::UpToTwoPlayers | TargetSpec::TargetOpponent => {
            let _ = slot; // Every slot is a player.
            ObjF::Player
        }
        TargetSpec::UpToOneStackAbility => ObjF::AnyCard,
        TargetSpec::ArtifactOrEnchantmentThenPlayer => {
            if slot == 0 {
                ObjF::Permanent
            } else {
                ObjF::Player
            }
        }
        TargetSpec::AnySpellOnStack
        | TargetSpec::BlueSpellOnStack
        | TargetSpec::RedSpellOnStack
        | TargetSpec::ArtifactOrEnchantmentSpellOnStack
        | TargetSpec::NoncreatureSpellOnStack => {
            let _ = slot; // One target slot.
            ObjF::Spell
        }
        TargetSpec::SpellManaValueAtMostControlledSubtypes { first, second } => {
            // The subtypes bound the spell's mana value; they do not change
            // the class of the targeted object.
            let _ = (first, second, slot);
            ObjF::Spell
        }
        TargetSpec::InstantSpellOnStack => {
            let _ = slot; // One target slot.
            typed(CardType::Instant)
        }
        TargetSpec::SorcerySpellOnStack => {
            let _ = slot; // One target slot.
            typed(CardType::Sorcery)
        }
        TargetSpec::CreatureSpellOnStack => {
            let _ = slot; // One target slot.
            creature()
        }
        TargetSpec::SpellYouDontControl => {
            let _ = slot; // One target slot.
            ObjF::Spell
        }
        TargetSpec::CreatureCardInOwnGraveyardManaValueAtMost(maximum) => {
            // The bound restricts which cards qualify, not their class.
            let _ = (maximum, slot);
            creature()
        }
        TargetSpec::ArtifactSpellOnStack
        | TargetSpec::ArtifactPermanent
        | TargetSpec::ExactlyTwoArtifactPermanents
        | TargetSpec::ControlledNoncreatureArtifactPermanent
        | TargetSpec::NoncreatureArtifactPermanent => {
            let _ = slot; // Every slot is an artifact.
            typed(CardType::Artifact)
        }
        TargetSpec::AnyPermanent
        | TargetSpec::BluePermanent
        | TargetSpec::RedPermanent
        | TargetSpec::OpponentArtifactOrEnchantmentPermanent
        | TargetSpec::ArtifactOrEnchantmentPermanent
        | TargetSpec::CreatureOrPlaneswalker
        | TargetSpec::ArtifactEnchantmentOrFlyingCreature
        | TargetSpec::ArtifactEnchantmentOrCreaturePowerAtLeastFour
        | TargetSpec::ArtifactCreatureEnchantmentOrPlaneswalker
        | TargetSpec::CreatureEnchantmentOrPlaneswalker => {
            let _ = slot; // One target slot.
            ObjF::Permanent
        }
        TargetSpec::NonlandPermanent | TargetSpec::OpponentNonlandPermanent => {
            let _ = slot; // One target slot.
            ObjF::NonlandPermanent
        }
        TargetSpec::PermanentCardInOwnGraveyard | TargetSpec::UpToOneOtherControlledPermanent => {
            let _ = slot;
            ObjF::Permanent
        }
        TargetSpec::NonlandPermanentCardInOwnGraveyardManaValueAtMost(maximum) => {
            // The bound restricts which cards qualify, not their class.
            let _ = (maximum, slot);
            ObjF::NonlandPermanent
        }
        TargetSpec::UpToTwoOtherControlledCreatures
        | TargetSpec::CounterDistribution
        | TargetSpec::Creature
        | TargetSpec::NonlegendaryCreature
        | TargetSpec::CreatureWithStunCounter
        | TargetSpec::ControlledCreature
        | TargetSpec::UpToTwoCreatureCardsInOwnGraveyard
        | TargetSpec::UpToTwoCreatures
        | TargetSpec::CreatureCardInOwnGraveyard
        | TargetSpec::OpponentControlledCreature
        | TargetSpec::CreatureOtherThanSource
        | TargetSpec::UpToOneTappedCreature
        | TargetSpec::NonblackCreature
        | TargetSpec::ControlledCreatureThenOpponentCreature
        | TargetSpec::AttackingOrBlockingCreature
        | TargetSpec::CreaturePowerPlusToughnessAtMostFive
        | TargetSpec::NonartifactCreature
        | TargetSpec::UpToOneOtherCreature
        | TargetSpec::AnotherAttackingCreature
        | TargetSpec::NonOutlawCreature
        | TargetSpec::CreatureToughnessAtLeastFour => {
            let _ = slot; // Every slot is a creature.
            creature()
        }
        TargetSpec::ControlledCreatureThenOpponentCreatureOrPlaneswalker => match slot {
            0 => creature(),
            1..=u8::MAX => ObjF::Permanent,
        },
        TargetSpec::EnchantmentPermanent => {
            let _ = slot; // One target slot.
            typed(CardType::Enchantment)
        }
        TargetSpec::Land => {
            let _ = slot; // One target slot.
            typed(CardType::Land)
        }
        // Creature or land cards: both are permanent cards, the narrowest
        // single class covering the pool.
        TargetSpec::CreatureOrLandCardInGraveyard => {
            let _ = slot; // One target slot.
            ObjF::Permanent
        }
        TargetSpec::UpToTwoCardsInGraveyards | TargetSpec::UpToOneCardInGraveyards => {
            let _ = slot; // Every slot is an unfiltered graveyard card.
            ObjF::AnyCard
        }
        TargetSpec::AnotherControlledCreature => {
            let _ = slot; // One target slot.
            creature()
        }
        TargetSpec::ControlledCreatureWithSubtype(subtype) => {
            // The subtype restricts which creatures qualify, not their class.
            let _ = (subtype, slot);
            creature()
        }
        TargetSpec::AttackingCreatureWithSubtype(subtype) => {
            // The subtype restricts which creatures qualify, not their class.
            let _ = (subtype, slot);
            creature()
        }
        TargetSpec::ControlledPermanentWithAnySubtype(subtypes) => {
            // Any permanent with a listed subtype; one target slot.
            let _ = (subtypes, slot);
            ObjF::Permanent
        }
    }
}

fn typed(card_type: CardType) -> ObjF {
    ObjF::from(card_type)
}

fn creature() -> ObjF {
    typed(CardType::Creature)
}

/// One legal object class with no color or mana-value filter.
fn object(out: &mut Collector, obj: ObjF, controller: Option<RelF>, zone: ZoneF) {
    out.target(TargetAtom::Object {
        obj,
        controller,
        zone,
        color: None,
        mana_value_at_most: None,
        excludes: None,
    });
}

/// One legal object class on either side, filtered to a color.
fn colored(out: &mut Collector, obj: ObjF, zone: ZoneF, color: crate::mana::ManaColor) {
    out.target(TargetAtom::Object {
        obj,
        controller: None,
        zone,
        color: Some(color.into()),
        mana_value_at_most: None,
        excludes: None,
    });
}

/// A mana-value bound in the same buckets as `AmtF::fixed`.
fn mana_value_bucket(maximum: u16) -> u8 {
    match AmtF::fixed(i64::from(maximum)) {
        AmtF::Fixed(bucket) => bucket,
        AmtF::Unit
        | AmtF::X
        | AmtF::Dynamic
        | AmtF::All
        | AmtF::UntilType(_)
        | AmtF::Half
        | AmtF::Minus(_) => {
            unreachable!("AmtF::fixed returns Fixed for a non-negative count")
        }
    }
}

/// Exact target domains; keep new Standard payloads in the source record and
/// explicitly mark refinements unavailable in the frozen facet vocabulary.
fn standard_target_obj(filter: crate::standard_cards_v1::StandardTargetV1) -> ObjF {
    use crate::standard_cards_v1::StandardTargetV1 as S;
    match filter {
        S::OpponentNonlandPermanentManaValueAtMost(_)
        | S::OpponentNonlandPermanent
        | S::AnotherNonlandPermanent => ObjF::NonlandPermanent,
        S::OpponentArtifactOrCreature => ObjF::Permanent,
        S::ControlledArtifact => typed(CardType::Artifact),
        S::InstantOrSorceryCardInOwnGraveyard | S::CardInAGraveyard => ObjF::AnyCard,
        S::AnotherNonlegendaryControlledCreature | S::UpToOneCreature => creature(),
        S::UpToTwoAnyTargets => ObjF::PlayerOrPermanent,
    }
}

pub(super) fn standard_target_origin(
    filter: crate::standard_cards_v1::StandardTargetV1,
) -> (Option<ZoneF>, Option<RelF>) {
    use crate::standard_cards_v1::StandardTargetV1 as S;
    match filter {
        S::InstantOrSorceryCardInOwnGraveyard => (Some(ZoneF::Graveyard), Some(RelF::You)),
        S::CardInAGraveyard => (Some(ZoneF::Graveyard), None),
        S::ControlledArtifact | S::AnotherNonlegendaryControlledCreature => {
            (Some(ZoneF::Battlefield), Some(RelF::You))
        }
        S::OpponentNonlandPermanentManaValueAtMost(_)
        | S::OpponentNonlandPermanent
        | S::OpponentArtifactOrCreature => (Some(ZoneF::Battlefield), Some(RelF::Opponent)),
        S::AnotherNonlandPermanent | S::UpToTwoAnyTargets | S::UpToOneCreature => {
            (Some(ZoneF::Battlefield), None)
        }
    }
}

fn standard_target_spec(filter: crate::standard_cards_v1::StandardTargetV1, out: &mut Collector) {
    use crate::standard_cards_v1::StandardTargetV1 as S;
    let (zone, relation) = standard_target_origin(filter);
    let zone = zone.expect("every Standard object target has a zone");
    match filter {
        S::OpponentNonlandPermanentManaValueAtMost(maximum) => {
            out.target(TargetAtom::Object {
                obj: ObjF::NonlandPermanent,
                controller: relation,
                zone,
                color: None,
                mana_value_at_most: Some(mana_value_bucket(u16::from(maximum))),
                excludes: None,
            });
        }
        S::OpponentNonlandPermanent | S::ControlledArtifact | S::CardInAGraveyard => {
            object(out, standard_target_obj(filter), relation, zone);
        }
        S::OpponentArtifactOrCreature => {
            object(out, typed(CardType::Artifact), relation, zone);
            object(out, creature(), relation, zone);
        }
        S::InstantOrSorceryCardInOwnGraveyard => {
            object(out, typed(CardType::Instant), relation, zone);
            object(out, typed(CardType::Sorcery), relation, zone);
        }
        S::AnotherNonlandPermanent | S::AnotherNonlegendaryControlledCreature => {
            object(out, standard_target_obj(filter), relation, zone);
            out.atoms.push(Atom::Opaque); // Source exclusion and Legendary refinement.
        }
        S::UpToOneCreature => {
            object(out, creature(), relation, zone);
            out.target(TargetAtom::UpTo);
        }
        S::UpToTwoAnyTargets => {
            out.target(TargetAtom::PlayerYou);
            out.target(TargetAtom::PlayerOpponent);
            object(out, creature(), None, zone);
            object(out, typed(CardType::Planeswalker), None, zone);
            out.target(TargetAtom::MultipleTargets);
            out.target(TargetAtom::UpTo);
        }
    }
}
