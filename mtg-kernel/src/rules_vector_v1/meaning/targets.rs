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
        TargetSpec::AnotherControlledNonlandPermanent
        | TargetSpec::AnotherNonBearCreatureCardInOwnGraveyardManaValueAtMost(_)
        | TargetSpec::CreatureCardInOwnGraveyardManaValueAtMostSourcePower => {
            // Frozen vocabulary cannot preserve the signed captured-power
            // bound, Bear/changeling exclusion and historical physical source.
            out.atoms.push(Atom::Opaque);
        }
        TargetSpec::StandardV1(filter) => standard_target_spec(filter, out),
        TargetSpec::KickerControlledCreatures => {
            panic!("variable cast cardinality requires a RulesMeaning successor")
        }
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
        // A spell with an exact mana value. The vocabulary bounds a mana
        // value only from above, so the exact value is opaque.
        TargetSpec::SpellWithManaValueV1(mana_value) => {
            let _ = mana_value;
            object(out, ObjF::Spell, None, ZoneF::Stack);
            out.atoms.push(Atom::Opaque);
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
        TargetSpec::UpToTwoCardsInSingleGraveyard => {
            object(out, ObjF::AnyCard, None, ZoneF::Graveyard);
            out.target(TargetAtom::MultipleTargets);
            out.target(TargetAtom::UpTo);
            // Shared physical graveyard has no frozen relational facet.
            out.atoms.push(Atom::Opaque);
        }
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
        TargetSpec::CreatureThenUpToOneAttachedEquipment => {
            object(out, creature(), None, ZoneF::Battlefield);
            object(out, typed(CardType::Artifact), None, ZoneF::Battlefield);
            out.target(TargetAtom::MultipleTargets);
            out.target(TargetAtom::UpTo);
            // Mandatory first slot, optional attached Equipment subtype and
            // exact attachment dependency have no frozen target vocabulary.
            out.atoms.push(Atom::Opaque);
        }
        TargetSpec::TwoCreaturesWithDifferentControllers => {
            object(out, creature(), None, ZoneF::Battlefield);
            out.target(TargetAtom::MultipleTargets);
            // No frozen relation expresses exactly two distinct objects whose
            // current controllers differ, including symmetric resolution checks.
            out.atoms.push(Atom::Opaque);
        }
        TargetSpec::AttackingCreature => {
            object(out, creature(), None, ZoneF::Battlefield);
            // Frozen vocabulary lacks a predicate for current attacking status.
            out.atoms.push(Atom::Opaque);
        }
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
        TargetSpec::CardInGraveyards => {
            object(out, ObjF::AnyCard, None, ZoneF::Graveyard);
        }
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
        TargetSpec::AnotherControlledNonlandPermanent => ObjF::Permanent,
        TargetSpec::AnotherNonBearCreatureCardInOwnGraveyardManaValueAtMost(_)
        | TargetSpec::CreatureCardInOwnGraveyardManaValueAtMostSourcePower => creature(),
        TargetSpec::CardInOwnGraveyardWithAnySubtype(_) => ObjF::AnyCard,
        TargetSpec::OpponentArtifactEnchantmentOrNonbasicLand => ObjF::Permanent,
        TargetSpec::StackObject
        | TargetSpec::StackAbility
        | TargetSpec::AnotherCreatureOrPlaneswalker => ObjF::AnyCard,
        TargetSpec::AnotherArtifactOrCreature => ObjF::Permanent,
        TargetSpec::UpToTwoOtherCreaturesDifferentControllers => creature(),
        TargetSpec::LegendaryCreature => creature(),
        TargetSpec::StandardV1(filter) => standard_target_obj(filter),
        TargetSpec::KickerControlledCreatures => {
            panic!("variable cast cardinality requires a RulesMeaning successor")
        }
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
        TargetSpec::SpellWithManaValueV1(mana_value) => {
            let _ = (mana_value, slot);
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
        | TargetSpec::AttackingCreature
        | TargetSpec::TwoCreaturesWithDifferentControllers
        | TargetSpec::NonOutlawCreature
        | TargetSpec::CreatureToughnessAtLeastFour => {
            let _ = slot; // Every slot is a creature.
            creature()
        }
        TargetSpec::CreatureThenUpToOneAttachedEquipment => match slot {
            0 => creature(),
            _ => typed(CardType::Artifact),
        },
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
        TargetSpec::UpToTwoCardsInGraveyards
        | TargetSpec::UpToOneCardInGraveyards
        | TargetSpec::UpToTwoCardsInSingleGraveyard
        | TargetSpec::CardInGraveyards => {
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
        S::UpToEightCreaturesOrPlaneswalkersV74
        | S::CreatureLandOrPlaneswalkerV1
        | S::ArtifactEnchantmentOrFlyingCreatureV1 => ObjF::Permanent,
        S::OpponentNonlandPermanentManaValueAtMost(_)
        | S::OpponentNonlandPermanent
        | S::AnotherNonlandPermanent => ObjF::NonlandPermanent,
        S::OpponentArtifactOrCreature | S::UpToTwoControlledArtifactsOrCreatures => ObjF::Permanent,
        S::ControlledArtifact => typed(CardType::Artifact),
        S::InstantOrSorceryCardInOwnGraveyard | S::CardInAGraveyard => ObjF::AnyCard,
        S::CreatureCardPutIntoGraveyardThisTurnV1 => creature(),
        S::AnotherNonlegendaryControlledCreature
        | S::UpToOneCreature
        | S::UpToOneControlledCreatureV1
        | S::TappedOpponentCreature
        | S::CrewedSourceThisTurn
        | S::UpToTwoControlledCreatures
        | S::TwoCreaturesV1 => creature(),
        S::UpToTwoAnyTargets | S::OneOrTwoAnyTargetsV1 | S::OneToThreeDistinctAnyTargetsV1 => {
            ObjF::PlayerOrPermanent
        }
        S::NonbasicLandV1 => typed(CardType::Land),
        S::InstantOrSorcerySpellV1 | S::SpellWithSingleTargetV1 => ObjF::Spell,
        S::OpponentSpellOrNonlandPermanentV1 => ObjF::NonlandPermanent,
        S::AnotherArtifactV1 | S::UpToOneNoncreatureArtifactV1 => typed(CardType::Artifact),
        S::AttackingClericRogueWarriorOrWizardV1 => creature(),
        S::AnyNumberControlledNonlandNontokenPermanentsV1 => ObjF::NonlandPermanent,
        S::PlayerThenArtifactV1 | S::PlayerThenCreatureV1 => ObjF::PlayerOrPermanent,
        S::ArtifactThenCreatureV1 => ObjF::Permanent,
        S::UpToOneOtherNonlandPermanentV1 => ObjF::NonlandPermanent,
        S::TriggeredAbilityOrColorlessSpellV1 => ObjF::Spell,
        S::CreaturesThenNonlandPermanentsV1 { .. } => ObjF::Permanent,
        S::TwoControlledArtifactsCreaturesOrLandsV1 => ObjF::Permanent,
        S::PlayerThenPlayerV1 => ObjF::PlayerOrPermanent,
        S::PlayerThenGraveyardCardsV1 | S::CreatureThenGraveyardCardsV1 => ObjF::AnyCard,
        S::UpToOneColoredPermanentV1 => ObjF::Permanent,
        S::ControlledArtifactOrCreatureThenOpponentSmallCreatureV1 => ObjF::Permanent,
        S::ForestV1 => typed(CardType::Land),
        S::UpToOneOpponentCreatureV1 => creature(),
        S::CreatureOrVehicleV1 => ObjF::Permanent,
        S::ArtifactCardInOwnGraveyardV1 | S::ArtifactCardInOwnGraveyardManaValueAtMostV1(_) => {
            typed(CardType::Artifact)
        }
        S::CreatureCardInAGraveyardV1 => creature(),
        S::UpToThreeLandCardsInOwnGraveyardV1 => typed(CardType::Land),
        S::ArtifactThenArtifactCardInItsControllersGraveyardV1 => typed(CardType::Artifact),
        S::NonlandPermanentManaValueAtMostV1(_) => ObjF::NonlandPermanent,
        S::OpponentPermanentV1 => ObjF::Permanent,
        S::AnyNumberOfSpellsV1 | S::UpToOneCreatureOrPlaneswalkerSpellV1 => ObjF::Spell,
        S::LandCardInAGraveyardV1
        | S::UpToOneLandCardInOwnGraveyardV1
        | S::LandCardInOwnGraveyardV1 => typed(CardType::Land),
        S::InstantOrSorceryCardInAGraveyardV1 => ObjF::AnyCard,
        S::UpToOneCreatureWithTrampleOrHasteV1 => creature(),
        S::AnotherPermanentV1
        | S::UpToOneArtifactOrEnchantmentV1
        | S::UpToOneArtifactCreatureOrEnchantmentV1
        | S::ArtifactOrCreatureV1 => ObjF::Permanent,
        S::UpToOneNonbasicLandV1 => typed(CardType::Land),
        S::UpToOneOpponentNonlandNontokenManaValueAtMostV1(_) => ObjF::NonlandPermanent,
        S::OpponentThenUpToOneTheirCreatureV1 => ObjF::PlayerOrPermanent,
        S::UpToOneOpponentCreatureOrPlaneswalkerV1 => ObjF::Permanent,
        S::ControlledCreatureThenUpToOneOpponentCreatureV1 | S::TwoDistinctCreaturesV1 => {
            creature()
        }
        S::UpToOneOpponentNoncreatureNonlandPermanentManaValueAtMostV1(_) => ObjF::NonlandPermanent,
        S::DividedFourDamageCreaturesOrPlaneswalkersV1 | S::DividedXDamageUpToTwoV1 => {
            ObjF::Permanent
        }
        S::UpToOnePlayerV1 => ObjF::PlayerOrPermanent,
        S::UpToOnePermanentCardDiedThisTurnV1 => ObjF::AnyCard,
        S::DividedTwoDamageAnyTargetsV1 => ObjF::PlayerOrPermanent,
        S::CreatureManaValueAtMostV1(_) => creature(),
        S::OpponentArtifactV1 => typed(CardType::Artifact),
    }
}

pub(super) fn standard_target_origin(
    filter: crate::standard_cards_v1::StandardTargetV1,
) -> (Option<ZoneF>, Option<RelF>) {
    use crate::standard_cards_v1::StandardTargetV1 as S;
    match filter {
        S::InstantOrSorceryCardInOwnGraveyard
        | S::ArtifactCardInOwnGraveyardV1
        | S::ArtifactCardInOwnGraveyardManaValueAtMostV1(_)
        | S::UpToThreeLandCardsInOwnGraveyardV1 => (Some(ZoneF::Graveyard), Some(RelF::You)),
        S::CreatureCardInAGraveyardV1 => (Some(ZoneF::Graveyard), None),
        S::CardInAGraveyard
        | S::CreatureCardPutIntoGraveyardThisTurnV1
        | S::PlayerThenGraveyardCardsV1 => (Some(ZoneF::Graveyard), None),
        S::UpToOnePermanentCardDiedThisTurnV1 => (Some(ZoneF::Graveyard), Some(RelF::You)),
        S::ControlledArtifact
        | S::AnotherNonlegendaryControlledCreature
        | S::UpToOneControlledCreatureV1
        | S::UpToTwoControlledCreatures
        | S::UpToTwoControlledArtifactsOrCreatures
        | S::AttackingClericRogueWarriorOrWizardV1
        | S::AnyNumberControlledNonlandNontokenPermanentsV1
        | S::TwoControlledArtifactsCreaturesOrLandsV1 => {
            (Some(ZoneF::Battlefield), Some(RelF::You))
        }
        S::OpponentNonlandPermanentManaValueAtMost(_)
        | S::OpponentNonlandPermanent
        | S::OpponentArtifactOrCreature
        | S::TappedOpponentCreature
        | S::OpponentSpellOrNonlandPermanentV1
        | S::UpToOneOpponentCreatureV1
        | S::OpponentPermanentV1
        | S::UpToOneOpponentNonlandNontokenManaValueAtMostV1(_)
        | S::OpponentThenUpToOneTheirCreatureV1
        | S::UpToOneOpponentCreatureOrPlaneswalkerV1
        | S::UpToOneOpponentNoncreatureNonlandPermanentManaValueAtMostV1(_)
        | S::OpponentArtifactV1 => (Some(ZoneF::Battlefield), Some(RelF::Opponent)),
        S::LandCardInAGraveyardV1 | S::InstantOrSorceryCardInAGraveyardV1 => {
            (Some(ZoneF::Graveyard), None)
        }
        S::UpToOneLandCardInOwnGraveyardV1 | S::LandCardInOwnGraveyardV1 => {
            (Some(ZoneF::Graveyard), Some(RelF::You))
        }
        S::AnyNumberOfSpellsV1 | S::UpToOneCreatureOrPlaneswalkerSpellV1 => {
            (Some(ZoneF::Stack), None)
        }
        S::InstantOrSorcerySpellV1
        | S::SpellWithSingleTargetV1
        | S::TriggeredAbilityOrColorlessSpellV1 => (Some(ZoneF::Stack), None),
        S::AnotherNonlandPermanent
        | S::UpToEightCreaturesOrPlaneswalkersV74
        | S::DividedFourDamageCreaturesOrPlaneswalkersV1
        | S::DividedXDamageUpToTwoV1
        | S::UpToOnePlayerV1
        | S::DividedTwoDamageAnyTargetsV1
        | S::CreatureManaValueAtMostV1(_)
        | S::CreatureLandOrPlaneswalkerV1
        | S::ArtifactEnchantmentOrFlyingCreatureV1
        | S::UpToTwoAnyTargets
        | S::OneToThreeDistinctAnyTargetsV1
        | S::OneOrTwoAnyTargetsV1
        | S::UpToOneCreature
        | S::TwoCreaturesV1
        | S::NonbasicLandV1
        | S::AnotherArtifactV1
        | S::UpToOneNoncreatureArtifactV1
        | S::PlayerThenArtifactV1
        | S::PlayerThenCreatureV1
        | S::ArtifactThenCreatureV1
        | S::UpToOneOtherNonlandPermanentV1
        | S::CreaturesThenNonlandPermanentsV1 { .. }
        | S::PlayerThenPlayerV1
        | S::CreatureThenGraveyardCardsV1
        | S::UpToOneColoredPermanentV1
        | S::ControlledArtifactOrCreatureThenOpponentSmallCreatureV1
        | S::ForestV1
        | S::CreatureOrVehicleV1
        | S::ArtifactThenArtifactCardInItsControllersGraveyardV1
        | S::NonlandPermanentManaValueAtMostV1(_)
        | S::UpToOneCreatureWithTrampleOrHasteV1
        | S::AnotherPermanentV1
        | S::UpToOneArtifactOrEnchantmentV1
        | S::UpToOneArtifactCreatureOrEnchantmentV1
        | S::ArtifactOrCreatureV1
        | S::UpToOneNonbasicLandV1
        | S::ControlledCreatureThenUpToOneOpponentCreatureV1
        | S::TwoDistinctCreaturesV1
        | S::CrewedSourceThisTurn => {
            // A crew member may have changed controller since it crewed.
            (Some(ZoneF::Battlefield), None)
        }
    }
}

fn standard_target_spec(filter: crate::standard_cards_v1::StandardTargetV1, out: &mut Collector) {
    use crate::standard_cards_v1::StandardTargetV1 as S;
    let (zone, relation) = standard_target_origin(filter);
    let zone = zone.expect("every Standard object target has a zone");
    match filter {
        S::UpToEightCreaturesOrPlaneswalkersV74
        | S::CreatureLandOrPlaneswalkerV1
        | S::ArtifactEnchantmentOrFlyingCreatureV1 => {
            // The old facet vocabulary does not encode this union or land override.
            out.atoms.push(Atom::Opaque);
        }
        S::DividedFourDamageCreaturesOrPlaneswalkersV1
        | S::DividedXDamageUpToTwoV1
        | S::DividedTwoDamageAnyTargetsV1 => {
            // Vocabulary gap: divided damage and repeated picks have no facet.
            out.atoms.push(Atom::Opaque);
        }
        S::CreatureManaValueAtMostV1(maximum) => {
            out.target(TargetAtom::Object {
                obj: creature(),
                controller: relation,
                zone,
                color: None,
                mana_value_at_most: Some(mana_value_bucket(u16::from(maximum))),
                excludes: None,
            });
        }
        S::OpponentArtifactV1 => {
            object(out, standard_target_obj(filter), relation, zone);
        }
        S::UpToOnePlayerV1 => {
            out.target(TargetAtom::UpTo);
            // Vocabulary gap: a lone player target has no facet here.
            out.atoms.push(Atom::Opaque);
        }
        S::UpToOnePermanentCardDiedThisTurnV1 => {
            out.target(TargetAtom::UpTo);
            // Exact current-turn departure needs a successor.
            out.atoms.push(Atom::Opaque);
        }
        S::CreatureCardPutIntoGraveyardThisTurnV1 => {
            object(out, creature(), relation, zone);
            // Exact current-turn entry and physical incarnation need a successor.
            out.atoms.push(Atom::Opaque);
        }
        S::UpToTwoControlledCreatures
        | S::UpToTwoControlledArtifactsOrCreatures
        | S::UpToOneControlledCreatureV1 => {
            // Preserve the native payload without admitting these new target
            // programs through the frozen v1 facet vocabulary.
            out.atoms.push(Atom::Opaque);
        }
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
        S::TappedOpponentCreature | S::CrewedSourceThisTurn => {
            object(out, creature(), relation, zone);
            // Tapped status and exact-incarnation crew history have no facet.
            out.atoms.push(Atom::Opaque);
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
        S::OneToThreeDistinctAnyTargetsV1 => {
            out.target(TargetAtom::PlayerYou);
            out.target(TargetAtom::PlayerOpponent);
            object(out, creature(), None, zone);
            object(out, typed(CardType::Planeswalker), None, zone);
            out.target(TargetAtom::MultipleTargets);
            // The frozen vocabulary cannot express a mandatory first slot
            // followed by optional OTHER slots. Retain exact source payload.
            out.atoms.push(Atom::Opaque);
        }
        S::TwoCreaturesV1 => {
            object(out, creature(), relation, zone);
            out.target(TargetAtom::MultipleTargets);
            // Both target words may name the same creature.
            out.atoms.push(Atom::Opaque);
        }
        S::OneOrTwoAnyTargetsV1 => {
            out.target(TargetAtom::PlayerYou);
            out.target(TargetAtom::PlayerOpponent);
            object(out, creature(), None, zone);
            object(out, typed(CardType::Planeswalker), None, zone);
            out.target(TargetAtom::MultipleTargets);
        }
        S::NonbasicLandV1 => {
            object(out, typed(CardType::Land), relation, zone);
            // The nonbasic refinement has no facet.
            out.atoms.push(Atom::Opaque);
        }
        S::InstantOrSorcerySpellV1 => {
            object(out, ObjF::Spell, relation, zone);
            // The instant-or-sorcery refinement of a spell has no facet.
            out.atoms.push(Atom::Opaque);
        }
        S::SpellWithSingleTargetV1 => {
            object(out, ObjF::Spell, relation, zone);
            // "With a single target" has no facet.
            out.atoms.push(Atom::Opaque);
        }
        S::OpponentSpellOrNonlandPermanentV1 => {
            object(out, ObjF::NonlandPermanent, relation, zone);
            object(out, ObjF::Spell, relation, ZoneF::Stack);
        }
        S::AnotherArtifactV1 | S::AttackingClericRogueWarriorOrWizardV1 => {
            object(out, standard_target_obj(filter), relation, zone);
            // Source exclusion and the attacking-subtype refinement.
            out.atoms.push(Atom::Opaque);
        }
        S::AnyNumberControlledNonlandNontokenPermanentsV1 => {
            object(out, ObjF::NonlandPermanent, relation, zone);
            out.target(TargetAtom::MultipleTargets);
            out.target(TargetAtom::UpTo);
            // The nontoken refinement has no facet.
            out.atoms.push(Atom::Opaque);
        }
        S::UpToOneNoncreatureArtifactV1 => {
            object(out, typed(CardType::Artifact), relation, zone);
            out.target(TargetAtom::UpTo);
            // The noncreature refinement has no facet.
            out.atoms.push(Atom::Opaque);
        }
        S::PlayerThenArtifactV1 | S::PlayerThenCreatureV1 => {
            out.target(TargetAtom::PlayerYou);
            out.target(TargetAtom::PlayerOpponent);
            let second = if filter == S::PlayerThenArtifactV1 {
                typed(CardType::Artifact)
            } else {
                creature()
            };
            object(out, second, relation, zone);
            out.target(TargetAtom::MultipleTargets);
            // Which slot takes which kind has no facet.
            out.atoms.push(Atom::Opaque);
        }
        S::ArtifactThenCreatureV1 => {
            object(out, typed(CardType::Artifact), relation, zone);
            object(out, creature(), relation, zone);
            out.target(TargetAtom::MultipleTargets);
            // Both target words may name one artifact creature.
            out.atoms.push(Atom::Opaque);
        }
        S::UpToOneOtherNonlandPermanentV1 => {
            object(out, ObjF::NonlandPermanent, relation, zone);
            out.target(TargetAtom::UpTo);
            // Source exclusion.
            out.atoms.push(Atom::Opaque);
        }
        S::TriggeredAbilityOrColorlessSpellV1 => {
            object(out, ObjF::Spell, relation, zone);
            // The triggered-ability alternative and the colorless refinement
            // have no facet.
            out.atoms.push(Atom::Opaque);
        }
        S::CreaturesThenNonlandPermanentsV1 { .. } => {
            object(out, creature(), relation, zone);
            object(out, ObjF::NonlandPermanent, relation, zone);
            out.target(TargetAtom::MultipleTargets);
            // The per-mode slot counts have no facet.
            out.atoms.push(Atom::Opaque);
        }
        S::TwoControlledArtifactsCreaturesOrLandsV1 => {
            object(out, typed(CardType::Artifact), relation, zone);
            object(out, creature(), relation, zone);
            object(out, typed(CardType::Land), relation, zone);
            out.target(TargetAtom::MultipleTargets);
            out.atoms.push(Atom::Opaque);
        }
        S::PlayerThenPlayerV1 => {
            out.target(TargetAtom::PlayerYou);
            out.target(TargetAtom::PlayerOpponent);
            out.target(TargetAtom::MultipleTargets);
            // Both target words may name one player.
            out.atoms.push(Atom::Opaque);
        }
        S::PlayerThenGraveyardCardsV1 => {
            out.target(TargetAtom::PlayerYou);
            out.target(TargetAtom::PlayerOpponent);
            object(out, ObjF::AnyCard, None, ZoneF::Graveyard);
            out.target(TargetAtom::MultipleTargets);
            out.target(TargetAtom::UpTo);
            // Which slot takes which kind has no facet.
            out.atoms.push(Atom::Opaque);
        }
        S::CreatureThenGraveyardCardsV1 => {
            object(out, creature(), None, zone);
            object(out, ObjF::AnyCard, None, ZoneF::Graveyard);
            out.target(TargetAtom::MultipleTargets);
            out.target(TargetAtom::UpTo);
            // The X bounds on mana value and card count have no facet.
            out.atoms.push(Atom::Opaque);
        }
        S::UpToOneColoredPermanentV1 => {
            object(out, ObjF::Permanent, relation, zone);
            out.target(TargetAtom::UpTo);
            // "One or more colors" has no facet.
            out.atoms.push(Atom::Opaque);
        }
        S::ControlledArtifactOrCreatureThenOpponentSmallCreatureV1 => {
            object(out, typed(CardType::Artifact), Some(RelF::You), zone);
            object(out, creature(), Some(RelF::You), zone);
            object(out, creature(), Some(RelF::Opponent), zone);
            out.target(TargetAtom::MultipleTargets);
            // The power bound has no facet.
            out.atoms.push(Atom::Opaque);
        }
        S::ForestV1 => {
            object(out, typed(CardType::Land), relation, zone);
            // Vocabulary gap: the Forest subtype has no facet.
            out.atoms.push(Atom::Opaque);
        }
        S::UpToOneOpponentCreatureV1 => {
            object(out, creature(), relation, zone);
            out.target(TargetAtom::UpTo);
        }
        S::CreatureOrVehicleV1 => {
            object(out, creature(), relation, zone);
            object(out, typed(CardType::Artifact), relation, zone);
            // Vocabulary gap: the Vehicle subtype has no facet.
            out.atoms.push(Atom::Opaque);
        }
        S::ArtifactCardInOwnGraveyardV1 | S::CreatureCardInAGraveyardV1 => {
            object(out, standard_target_obj(filter), relation, zone);
        }
        S::ArtifactCardInOwnGraveyardManaValueAtMostV1(maximum) => {
            out.target(TargetAtom::Object {
                obj: typed(CardType::Artifact),
                controller: relation,
                zone,
                color: None,
                mana_value_at_most: Some(mana_value_bucket(u16::from(maximum))),
                excludes: None,
            });
        }
        S::UpToThreeLandCardsInOwnGraveyardV1 => {
            object(out, typed(CardType::Land), relation, zone);
            out.target(TargetAtom::MultipleTargets);
            out.target(TargetAtom::UpTo);
        }
        S::ArtifactThenArtifactCardInItsControllersGraveyardV1 => {
            object(out, typed(CardType::Artifact), None, zone);
            object(out, typed(CardType::Artifact), None, ZoneF::Graveyard);
            out.target(TargetAtom::MultipleTargets);
            // "That player's graveyard" has no facet.
            out.atoms.push(Atom::Opaque);
        }
        S::NonlandPermanentManaValueAtMostV1(maximum) => {
            out.target(TargetAtom::Object {
                obj: ObjF::NonlandPermanent,
                controller: relation,
                zone,
                color: None,
                mana_value_at_most: Some(mana_value_bucket(u16::from(maximum))),
                excludes: None,
            });
        }
        S::OpponentPermanentV1 | S::LandCardInAGraveyardV1 | S::LandCardInOwnGraveyardV1 => {
            object(out, standard_target_obj(filter), relation, zone);
        }
        S::InstantOrSorceryCardInAGraveyardV1 => {
            object(out, typed(CardType::Instant), relation, zone);
            object(out, typed(CardType::Sorcery), relation, zone);
        }
        S::AnyNumberOfSpellsV1 => {
            object(out, ObjF::Spell, relation, zone);
            out.target(TargetAtom::MultipleTargets);
            out.target(TargetAtom::UpTo);
        }
        S::UpToOneCreatureOrPlaneswalkerSpellV1 => {
            object(out, ObjF::Spell, relation, zone);
            out.target(TargetAtom::UpTo);
            // The creature or planeswalker spell refinement has no facet.
            out.atoms.push(Atom::Opaque);
        }
        S::UpToOneLandCardInOwnGraveyardV1 => {
            object(out, typed(CardType::Land), relation, zone);
            out.target(TargetAtom::UpTo);
        }
        S::UpToOneCreatureWithTrampleOrHasteV1 => {
            object(out, creature(), relation, zone);
            out.target(TargetAtom::UpTo);
            // The trample or haste refinement has no facet.
            out.atoms.push(Atom::Opaque);
        }
        S::AnotherPermanentV1 => {
            object(out, ObjF::Permanent, relation, zone);
            // Source exclusion.
            out.atoms.push(Atom::Opaque);
        }
        S::UpToOneArtifactOrEnchantmentV1 => {
            object(out, typed(CardType::Artifact), relation, zone);
            object(out, typed(CardType::Enchantment), relation, zone);
            out.target(TargetAtom::UpTo);
        }
        S::UpToOneArtifactCreatureOrEnchantmentV1 => {
            object(out, typed(CardType::Artifact), relation, zone);
            object(out, creature(), relation, zone);
            object(out, typed(CardType::Enchantment), relation, zone);
            out.target(TargetAtom::UpTo);
        }
        S::ArtifactOrCreatureV1 => {
            object(out, typed(CardType::Artifact), relation, zone);
            object(out, creature(), relation, zone);
        }
        S::UpToOneNonbasicLandV1 => {
            object(out, typed(CardType::Land), relation, zone);
            out.target(TargetAtom::UpTo);
            // The nonbasic refinement has no facet.
            out.atoms.push(Atom::Opaque);
        }
        S::UpToOneOpponentNonlandNontokenManaValueAtMostV1(maximum) => {
            out.target(TargetAtom::Object {
                obj: ObjF::NonlandPermanent,
                controller: relation,
                zone,
                color: None,
                mana_value_at_most: Some(mana_value_bucket(u16::from(maximum))),
                excludes: None,
            });
            out.target(TargetAtom::UpTo);
            // The nontoken refinement has no facet.
            out.atoms.push(Atom::Opaque);
        }
        S::OpponentThenUpToOneTheirCreatureV1 => {
            out.target(TargetAtom::PlayerOpponent);
            object(out, creature(), relation, zone);
            out.target(TargetAtom::MultipleTargets);
            out.target(TargetAtom::UpTo);
            // "They control" ties the creature to the chosen opponent.
            out.atoms.push(Atom::Opaque);
        }
        S::UpToOneOpponentCreatureOrPlaneswalkerV1 => {
            object(out, creature(), relation, zone);
            object(out, typed(CardType::Planeswalker), relation, zone);
            out.target(TargetAtom::UpTo);
        }
        S::ControlledCreatureThenUpToOneOpponentCreatureV1 => {
            object(out, creature(), relation, zone);
            out.target(TargetAtom::MultipleTargets);
            out.target(TargetAtom::UpTo);
            // Which slot takes which controller has no facet.
            out.atoms.push(Atom::Opaque);
        }
        S::TwoDistinctCreaturesV1 => {
            object(out, creature(), relation, zone);
            out.target(TargetAtom::MultipleTargets);
        }
        S::UpToOneOpponentNoncreatureNonlandPermanentManaValueAtMostV1(maximum) => {
            out.target(TargetAtom::Object {
                obj: ObjF::NonlandPermanent,
                controller: relation,
                zone,
                color: None,
                mana_value_at_most: Some(mana_value_bucket(u16::from(maximum))),
                excludes: None,
            });
            out.target(TargetAtom::UpTo);
            // The noncreature refinement has no facet.
            out.atoms.push(Atom::Opaque);
        }
    }
}
