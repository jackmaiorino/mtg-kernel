//! Meaning tables for conditions, dynamic amounts and object filters.
//!
//! Conventions shared by every table here, so that encodings that read the
//! same game state collapse onto one `ReadAtom`:
//! - Counting cards of a type in a zone is always
//!   `(player, zone, Typed(type), Count)` for a numeric amount and
//!   `(player, zone, Typed(type), AtLeast(k))` for a threshold. An existence
//!   check is the threshold `AtLeast(1)`, never `Any`, so "has a creature
//!   card in your graveyard" and "at least one creature card in your
//!   graveyard" are the same fact.
//! - A read of something that happened this turn uses `EventThisTurn` with
//!   the zone the event put the object into (a land drop is Battlefield, a
//!   death is Graveyard, a draw is Hand).
//! - A filter's returned relation is the control restriction the filter's
//!   own predicate checks; `None` when the predicate checks no control and
//!   the consumer's scan decides whose objects are considered.

use super::*;
use crate::card_def::{OptionalAdditionalCostDef, SubtypeConjunctionDef};

/// The `AtLeast` aggregate for a threshold, bucketed like `AmtF::fixed`.
fn at_least(n: i64) -> AggF {
    match AmtF::fixed(n) {
        AmtF::Fixed(bucket) => AggF::AtLeast(bucket),
        AmtF::Unit
        | AmtF::X
        | AmtF::Dynamic
        | AmtF::All
        | AmtF::UntilType(_)
        | AmtF::Half
        | AmtF::Minus(_) => unreachable!("AmtF::fixed returns Fixed for a non-negative count"),
    }
}

/// Reads a resolution-time condition performs.
pub(crate) fn effect_cond(cond: &EffectCond, env: &Env, out: &mut Collector) {
    match cond {
        // Constant conditions read nothing.
        EffectCond::Always => {}
        EffectCond::Never => {}
        EffectCond::DiscardedNonLandForCost => {
            // Reads the cards this cast's additional cost discarded
            // (hand -> graveyard) and asks whether any is a nonland card.
            // Vocabulary gap: no nonland-card class and no cast-provenance
            // aggregate; nearest is "a card went to your graveyard".
            out.read(
                RelF::You,
                Some(ZoneF::Graveyard),
                Some(ObjF::AnyCard),
                AggF::EventThisTurn,
            );
        }
        EffectCond::LandfallThisTurn => {
            // Reads the controller's land drops this turn.
            out.read(
                RelF::You,
                Some(ZoneF::Battlefield),
                Some(ObjF::Typed(CardTypeF::Land)),
                AggF::EventThisTurn,
            );
        }
        EffectCond::TargetInZone(slot, zone) => {
            // Whether the targeted object is still in `zone` (the 608.2b
            // legality recheck, incarnation-exact).
            out.read(
                RelF::ObjectController,
                Some(ZoneF::from(*zone)),
                Some(env.target_obj(*slot)),
                at_least(1),
            );
        }
        EffectCond::TargetIsColor(slot, color) => {
            // Reads the targeted object's printed colors.
            // Vocabulary gap: `ReadAtom` carries no color, so the color
            // tested is not represented; the read is of a characteristic.
            let _ = color;
            out.read(
                RelF::ObjectController,
                None,
                Some(env.target_obj(*slot)),
                AggF::Characteristic,
            );
        }
        EffectCond::And(first, second) => {
            effect_cond(first, env, out);
            effect_cond(second, env, out);
        }
        EffectCond::ControlsArtifactCount(minimum) => {
            out.read(
                RelF::You,
                Some(ZoneF::Battlefield),
                Some(ObjF::Typed(CardTypeF::Artifact)),
                at_least(i64::from(*minimum)),
            );
        }
        EffectCond::ControlsOtherSubtypeCount {
            subtype,
            minimum_count,
        } => {
            // Permanents the controller controls with the subtype, other
            // than the source. Vocabulary gap: ObjF has no subtype class.
            let _ = subtype;
            out.read(
                RelF::You,
                Some(ZoneF::Battlefield),
                Some(ObjF::Permanent),
                at_least(i64::from(*minimum_count)),
            );
        }
        EffectCond::ControllerGraveyardCreatureCardsAtLeast(minimum) => {
            out.read(
                RelF::You,
                Some(ZoneF::Graveyard),
                Some(ObjF::Typed(CardTypeF::Creature)),
                at_least(i64::from(*minimum)),
            );
        }
        EffectCond::ControlsAnotherSourceCard => {
            // Another battlefield object of the controller's with the
            // source's own card definition. The definition is the source's
            // own, so no referenced card id is known here.
            out.read(
                RelF::You,
                Some(ZoneF::Battlefield),
                Some(ObjF::SpecificCard),
                at_least(1),
            );
        }
        EffectCond::WasKicked => {
            // Reads cast-scoped metadata: whether this casting paid its
            // optional additional cost. Vocabulary gap: no cost-paid
            // aggregate; nearest is an event about this object.
            optional_cost_paid(out);
        }
        EffectCond::OptionalAdditionalCostPaid(kind) => {
            match kind {
                OptionalAdditionalCostDef::CollectEvidence { minimum_mana_value } => {
                    // The size of the payment is a cost fact, not part of
                    // the read of whether it was paid.
                    let _ = minimum_mana_value;
                }
                OptionalAdditionalCostDef::Bargain => {}
            }
            optional_cost_paid(out);
        }
        EffectCond::ControllerGraveyardHasType(card_type) => {
            out.read(
                RelF::You,
                Some(ZoneF::Graveyard),
                Some(ObjF::from(*card_type)),
                at_least(1),
            );
        }
        EffectCond::OpponentHasCardsInHand => {
            out.read(
                RelF::Opponent,
                Some(ZoneF::Hand),
                Some(ObjF::AnyCard),
                at_least(1),
            );
        }
        EffectCond::ControlsOtherIncarnationSubtypeCount {
            subtype,
            minimum_count,
        } => {
            // Same read as `ControlsOtherSubtypeCount`; only the exclusion
            // of the source is incarnation-exact. Vocabulary gap: ObjF has
            // no subtype class.
            let _ = subtype;
            out.read(
                RelF::You,
                Some(ZoneF::Battlefield),
                Some(ObjF::Permanent),
                at_least(i64::from(*minimum_count)),
            );
        }
        EffectCond::TargetSpellCanBeCountered(slot) => {
            // The targeted spell is still on the stack (exact incarnation)
            // and its definition does not say it can't be countered.
            out.read(
                RelF::ObjectController,
                Some(ZoneF::Stack),
                Some(env.target_obj(*slot)),
                at_least(1),
            );
        }
        EffectCond::ControllerGraveyardCardCountAtLeast(minimum) => {
            // Nontoken, noncopy cards the controller owns in their graveyard.
            out.read(
                RelF::You,
                Some(ZoneF::Graveyard),
                Some(ObjF::AnyCard),
                at_least(i64::from(*minimum)),
            );
        }
        EffectCond::CreatureDiedThisTurn => {
            // Any creature, either player's, went battlefield -> graveyard
            // this turn.
            out.read(
                RelF::EachPlayer,
                Some(ZoneF::Graveyard),
                Some(ObjF::Typed(CardTypeF::Creature)),
                AggF::EventThisTurn,
            );
        }
        EffectCond::ControlsCreaturePowerAtLeast(minimum) => {
            // Whether a creature the controller controls has current power
            // at least `minimum`. Vocabulary gap: `Characteristic` carries
            // no threshold.
            let _ = minimum;
            out.read(
                RelF::You,
                Some(ZoneF::Battlefield),
                Some(ObjF::Typed(CardTypeF::Creature)),
                AggF::Characteristic,
            );
        }
    }
}

/// The read of whether this casting paid an optional additional cost.
fn optional_cost_paid(out: &mut Collector) {
    out.read(RelF::You, None, Some(ObjF::ThisObject), AggF::EventThisTurn);
}

/// Emits the read behind a dynamic value and returns its amount facet.
pub(crate) fn dynamic_value(value: DynamicValueDef, out: &mut Collector) -> AmtF {
    match value {
        DynamicValueDef::BattlefieldPermanentsWithSubtype(subtype) => {
            // Every battlefield permanent, either controller, with the
            // subtype. Vocabulary gap: ObjF has no subtype class.
            let _ = subtype;
            out.read(
                RelF::EachPlayer,
                Some(ZoneF::Battlefield),
                Some(ObjF::Permanent),
                AggF::Count,
            );
            AmtF::Dynamic
        }
        DynamicValueDef::Fixed(n) => AmtF::fixed(i64::from(n)),
        DynamicValueDef::ControllerGraveyardCardsWithType(card_type) => {
            // Nontoken cards of the type the controller owns in their
            // graveyard.
            out.read(
                RelF::You,
                Some(ZoneF::Graveyard),
                Some(ObjF::from(card_type)),
                AggF::Count,
            );
            AmtF::Dynamic
        }
        DynamicValueDef::AmountIfControllerControlsEach {
            required,
            amount_when_met,
            amount_otherwise,
        } => {
            // One existence check per required pair: a permanent the
            // controller controls carrying both subtypes. Vocabulary gap:
            // ObjF has no subtype class.
            for pair in required {
                let SubtypeConjunctionDef { first, second } = pair;
                let _ = (first, second);
                out.read(
                    RelF::You,
                    Some(ZoneF::Battlefield),
                    Some(ObjF::Permanent),
                    at_least(1),
                );
            }
            // The value is one of two fixed numbers chosen by the board, so
            // as an amount it is state-dependent (`Dynamic`). Vocabulary
            // gap: AmtF cannot carry the two candidate magnitudes.
            let _ = (amount_when_met, amount_otherwise);
            AmtF::Dynamic
        }
    }
}

/// Emits the read behind a dynamic count and returns its amount facet.
pub(crate) fn dynamic_count(count: DynamicCountDef, out: &mut Collector) -> AmtF {
    match count {
        DynamicCountDef::ControllerBattlefieldAnyType(types) => {
            // Permanents the caster controls with any listed type.
            for &card_type in types {
                out.read(
                    RelF::You,
                    Some(ZoneF::Battlefield),
                    Some(ObjF::from(card_type)),
                    AggF::Count,
                );
            }
            AmtF::Dynamic
        }
        DynamicCountDef::ControllerGraveyardAnyType(types) => {
            // Cards in the caster's graveyard with any listed type.
            for &card_type in types {
                out.read(
                    RelF::You,
                    Some(ZoneF::Graveyard),
                    Some(ObjF::from(card_type)),
                    AggF::Count,
                );
            }
            AmtF::Dynamic
        }
        DynamicCountDef::ControllerDrawsThisTurn => {
            // The caster's draws this turn. Vocabulary gap: no aggregate
            // for the number of events this turn.
            out.read(
                RelF::You,
                Some(ZoneF::Hand),
                Some(ObjF::AnyCard),
                AggF::EventThisTurn,
            );
            AmtF::Dynamic
        }
        DynamicCountDef::ControllerHasCreatureWithAndWithoutSubtype(subtype) => {
            // One iff the caster controls a creature with the subtype and a
            // creature without it: two existence checks over the caster's
            // creatures, emitted one read per check (as the per-requirement
            // reads of `AmountIfControllerControlsEach`). Vocabulary gap:
            // ObjF has no subtype class, so the two reads coincide.
            let _ = subtype;
            for _half in [true, false] {
                out.read(
                    RelF::You,
                    Some(ZoneF::Battlefield),
                    Some(ObjF::Typed(CardTypeF::Creature)),
                    at_least(1),
                );
            }
            AmtF::Dynamic
        }
        DynamicCountDef::SpellTargetsTappedCreature => {
            // One iff a chosen target is a tapped battlefield creature.
            // Vocabulary gap: no tapped-status aggregate; nearest is a
            // read of the object's characteristics.
            out.read(
                RelF::ObjectController,
                Some(ZoneF::Battlefield),
                Some(ObjF::Typed(CardTypeF::Creature)),
                AggF::Characteristic,
            );
            AmtF::Dynamic
        }
    }
}

/// The creatures a creature filter selects, and whose they are.
pub(crate) fn creature_filter(filter: CreatureFilter) -> (ObjF, Option<RelF>) {
    // The matcher always requires a battlefield creature; it checks control
    // only for `OpponentControlled`.
    let creature = ObjF::Typed(CardTypeF::Creature);
    match filter {
        CreatureFilter::AnyControlled => (creature, None),
        CreatureFilter::ControlledWithSubtype(subtype) => {
            // Vocabulary gap: ObjF has no subtype class.
            let _ = subtype;
            (creature, None)
        }
        CreatureFilter::WithoutKeyword(keyword) => {
            // Vocabulary gap: ObjF has no keyword-exclusion class.
            let _ = keyword;
            (creature, None)
        }
        CreatureFilter::OpponentControlled => (creature, Some(RelF::Opponent)),
        CreatureFilter::All => (creature, None),
    }
}

/// The library cards a search filter accepts.
pub(crate) fn library_card_filter(filter: LibraryCardFilter, out: &mut Collector) -> ObjF {
    match filter {
        LibraryCardFilter::LandWithSubtype(subtype) => {
            // A land card with the subtype. Vocabulary gap: ObjF has no
            // subtype class.
            let _ = subtype;
            ObjF::Typed(CardTypeF::Land)
        }
        LibraryCardFilter::BasicLand => ObjF::BasicLand,
        LibraryCardFilter::BasicLandOrGate => {
            // A basic land card, or any card with the Gate subtype. The
            // matcher does not test the Land type on the Gate side, but Gate
            // is a land subtype (CR 205.3i) and an object only carries
            // subtypes of its own card types (CR 205.3d), so the accepted
            // set is land cards. Vocabulary gap: ObjF has no subtype class.
            ObjF::Typed(CardTypeF::Land)
        }
        LibraryCardFilter::CardDefinition(id) => {
            out.referenced_cards.push(id);
            ObjF::SpecificCard
        }
        LibraryCardFilter::BasicLandWithAnySubtype(subtypes) => {
            // A basic land card with one of the subtypes. Vocabulary gap:
            // ObjF has no subtype class.
            let _ = subtypes;
            ObjF::BasicLand
        }
        LibraryCardFilter::AnyLand => ObjF::Typed(CardTypeF::Land),
        LibraryCardFilter::AnyCard => ObjF::AnyCard,
    }
}

/// The permanents a permanent filter selects, and whose they are.
pub(crate) fn permanent_filter(filter: PermanentFilter) -> (ObjF, Option<RelF>) {
    // The type predicate checks no control; the consumer scans one
    // player's battlefield.
    match filter {
        PermanentFilter::ArtifactOrCreature => {
            // Vocabulary gap: no two-type union class; nearest is any
            // permanent.
            (ObjF::Permanent, None)
        }
        PermanentFilter::Artifact => (ObjF::Typed(CardTypeF::Artifact), None),
        PermanentFilter::Creature => (ObjF::Typed(CardTypeF::Creature), None),
        PermanentFilter::Land => (ObjF::Typed(CardTypeF::Land), None),
    }
}

/// The permanents a definition-level permanent filter selects.
pub(crate) fn permanent_filter_def(filter: PermanentFilterDef) -> (ObjF, Option<RelF>) {
    // Control is checked by the consuming cost component, not the filter.
    match filter {
        PermanentFilterDef::LandWithSubtype(subtype) => {
            // Vocabulary gap: ObjF has no subtype class.
            let _ = subtype;
            (ObjF::Typed(CardTypeF::Land), None)
        }
        PermanentFilterDef::CreatureWithColor(color) => {
            // Vocabulary gap: ObjF has no color refinement.
            let _ = color;
            (ObjF::Typed(CardTypeF::Creature), None)
        }
        PermanentFilterDef::Artifact => (ObjF::Typed(CardTypeF::Artifact), None),
    }
}

/// The cards a card-type predicate accepts.
pub(crate) fn card_type_predicate(predicate: CardTypePredicate) -> ObjF {
    match predicate {
        CardTypePredicate::InstantOrSorcery => {
            // Vocabulary gap: no two-type union class; nearest is the
            // instant card class.
            ObjF::Typed(CardTypeF::Instant)
        }
    }
}
