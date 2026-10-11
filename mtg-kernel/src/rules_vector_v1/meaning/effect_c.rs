//! Meaning table, EffectOp slice C: multi-target moves and damage, explore,
//! surveil, graveyard exile, optional mana payment, subtype/creature-count
//! amounts and reveal-until mills.
//!
//! Every arm was written against the variant's executor in `effect::execute`
//! and, for the interpreter-owned forms, the resumable interpreter frame and
//! its answer consumer. `MoveBoundObject` is the only runtime-only variant in
//! this slice: the interpreter builds it as an option payload of Explore and
//! Surveil, and no generated card program contains a pre-bound object.

use super::*;

/// Where the object behind `Target(slot)` lives when the program refers to
/// it, and whose it is when the spec restricts that. `None` zone: the slot
/// denotes a player (or nothing). `None` relation: either player's object.
///
/// Mirrors slice A's private helper of the same name (not reachable from
/// here); zone changes of announced targets need the origin zone.
fn target_slot_origin(spec: TargetSpec, slot: u8) -> (Option<ZoneF>, Option<RelF>) {
    const BATTLEFIELD: Option<ZoneF> = Some(ZoneF::Battlefield);
    const STACK: Option<ZoneF> = Some(ZoneF::Stack);
    const GRAVEYARD: Option<ZoneF> = Some(ZoneF::Graveyard);
    match spec {
        TargetSpec::None => (None, None),
        TargetSpec::CardInOwnGraveyardWithAnySubtype(_) => (GRAVEYARD, Some(RelF::You)),
        TargetSpec::OpponentArtifactEnchantmentOrNonbasicLand => {
            (BATTLEFIELD, Some(RelF::Opponent))
        }
        TargetSpec::LegendaryCreature => (BATTLEFIELD, None),
        // Slot 0 is a creature or a player; only the creature has a zone.
        TargetSpec::AnyTarget => (BATTLEFIELD, None),
        // Slot 0 is the player, slot 1 a creature that player controls.
        TargetSpec::PlayerThenTheirCreature => match slot {
            0 => (None, None),
            1..=u8::MAX => (BATTLEFIELD, Some(RelF::ChosenPlayer)),
        },
        TargetSpec::AnySpellOnStack
        | TargetSpec::InstantSpellOnStack
        | TargetSpec::BlueSpellOnStack
        | TargetSpec::RedSpellOnStack
        | TargetSpec::ArtifactOrEnchantmentSpellOnStack
        | TargetSpec::SorcerySpellOnStack
        | TargetSpec::NoncreatureSpellOnStack
        | TargetSpec::ArtifactSpellOnStack
        | TargetSpec::CreatureSpellOnStack => (STACK, None),
        TargetSpec::SpellYouDontControl => (STACK, Some(RelF::Opponent)),
        TargetSpec::SpellManaValueAtMostControlledSubtypes { first, second } => {
            // The subtypes bound the legal mana value; that is a target
            // legality fact owned by the targets table, not an origin.
            let _ = (first, second);
            (STACK, None)
        }
        TargetSpec::AnyPermanent
        | TargetSpec::BluePermanent
        | TargetSpec::RedPermanent
        | TargetSpec::NonlandPermanent
        | TargetSpec::Creature
        | TargetSpec::NonlegendaryCreature
        | TargetSpec::ArtifactPermanent
        | TargetSpec::UpToTwoCreatures
        | TargetSpec::ExactlyTwoArtifactPermanents
        | TargetSpec::EnchantmentPermanent
        | TargetSpec::CreatureOtherThanSource
        | TargetSpec::UpToOneTappedCreature
        | TargetSpec::ControlledNoncreatureArtifactPermanent
        | TargetSpec::CreatureWithStunCounter
        | TargetSpec::NoncreatureArtifactPermanent
        | TargetSpec::Land
        | TargetSpec::NonblackCreature
        | TargetSpec::ArtifactOrEnchantmentPermanent
        | TargetSpec::AttackingOrBlockingCreature
        | TargetSpec::CreatureOrPlaneswalker
        | TargetSpec::ArtifactEnchantmentOrFlyingCreature
        | TargetSpec::ArtifactEnchantmentOrCreaturePowerAtLeastFour
        | TargetSpec::CreaturePowerPlusToughnessAtMostFive
        | TargetSpec::NonartifactCreature
        | TargetSpec::UpToOneOtherCreature
        | TargetSpec::AnotherAttackingCreature
        | TargetSpec::NonOutlawCreature
        | TargetSpec::CreatureToughnessAtLeastFour
        | TargetSpec::ArtifactCreatureEnchantmentOrPlaneswalker
        | TargetSpec::CreatureEnchantmentOrPlaneswalker => (BATTLEFIELD, None),
        TargetSpec::ControlledNoncreatureArtifactPermanent
        | TargetSpec::ControlledCreature
        | TargetSpec::CounterDistribution
        | TargetSpec::AnotherControlledCreature
        | TargetSpec::UpToTwoOtherControlledCreatures
        | TargetSpec::UpToOneOtherControlledPermanent => (BATTLEFIELD, Some(RelF::You)),
        TargetSpec::ControlledCreatureWithSubtype(subtype) => {
            // The subtype is a target legality fact (targets table).
            let _ = subtype;
            (BATTLEFIELD, Some(RelF::You))
        }
        TargetSpec::AttackingCreatureWithSubtype(subtype) => {
            // The subtype is a target legality fact (targets table).
            let _ = subtype;
            (BATTLEFIELD, None)
        }
        TargetSpec::ControlledPermanentWithAnySubtype(subtypes) => {
            // The subtypes are a target legality fact (targets table).
            let _ = subtypes;
            (BATTLEFIELD, Some(RelF::You))
        }
        TargetSpec::OpponentControlledCreature
        | TargetSpec::OpponentArtifactOrEnchantmentPermanent
        | TargetSpec::OpponentNonlandPermanent => (BATTLEFIELD, Some(RelF::Opponent)),
        TargetSpec::ControlledCreatureThenOpponentCreature
        | TargetSpec::ControlledCreatureThenOpponentCreatureOrPlaneswalker => match slot {
            0 => (BATTLEFIELD, Some(RelF::You)),
            1..=u8::MAX => (BATTLEFIELD, Some(RelF::Opponent)),
        },
        TargetSpec::AnyPlayer | TargetSpec::UpToTwoPlayers | TargetSpec::TargetOpponent => {
            (None, None)
        }
        TargetSpec::CreatureOrLandCardInGraveyard
        | TargetSpec::UpToTwoCardsInGraveyards
        | TargetSpec::UpToOneCardInGraveyards => (GRAVEYARD, None),
        TargetSpec::UpToTwoCreatureCardsInOwnGraveyard | TargetSpec::CreatureCardInOwnGraveyard => {
            (GRAVEYARD, Some(RelF::You))
        }
        TargetSpec::CreatureCardInOwnGraveyardManaValueAtMost(mana_value) => {
            // The mana-value bound is a target legality fact (targets table).
            let _ = mana_value;
            (GRAVEYARD, Some(RelF::You))
        }
        TargetSpec::PermanentCardInOwnGraveyard => (GRAVEYARD, Some(RelF::You)),
        TargetSpec::NonlandPermanentCardInOwnGraveyardManaValueAtMost(mana_value) => {
            // The mana-value bound is a target legality fact (targets table).
            let _ = mana_value;
            (GRAVEYARD, Some(RelF::You))
        }
    }
}

/// Whose object `object` names: the source is the controller's; a target
/// is restricted by its slot when the spec says so, else `default_rel`.
fn object_whose(object: ObjectRef, env: &Env, default_rel: RelF) -> RelF {
    match object {
        ObjectRef::ThisSource => RelF::You,
        ObjectRef::Target(slot) => {
            let (zone, rel) = target_slot_origin(env.target_spec, slot);
            let _ = zone; // callers here act only on battlefield objects
            rel.unwrap_or(default_rel)
        }
    }
}

/// The distinct object targets the all-targets executors act on: one entry
/// per announced slot (`engine::target_count`) that can denote an object,
/// as (origin zone, restricting relation, object class). Player-only slots
/// have no origin zone and are left out, because those executors skip
/// player targets; slots that denote the same thing are merged so a
/// two-slot spec of one class yields one entry (multiplicity is carried by
/// the target facts).
fn object_target_slots(env: &Env) -> Vec<(ZoneF, Option<RelF>, ObjF)> {
    let mut slots = Vec::new();
    for slot in 0..crate::engine::target_count(env.target_spec) {
        let (zone, rel) = target_slot_origin(env.target_spec, slot);
        let Some(zone) = zone else {
            continue;
        };
        let entry = (zone, rel, env.target_obj(slot));
        if !slots.contains(&entry) {
            slots.push(entry);
        }
    }
    slots
}

/// A look or reveal of library cards (the atom carries the origin zone).
fn library_view(ev: EvF, player: RelF, obj: ObjF, amount: AmtF) -> EffectAtom {
    let mut atom = EffectAtom::new(ev).player(player).obj(obj).amount(amount);
    atom.from = Some(ZoneF::Library);
    atom
}

pub(super) fn effect_op(op: &EffectOp, env: &Env, out: &mut Collector) {
    match op {
        EffectOp::GainLifeEqualToPaidCostManaValue { player } => {
            // Sums the printed mana values of the objects this stack item's
            // cost moved (frozen payment provenance), then gains that much.
            // Vocabulary gap: ObjF has no "object paid as a cost" class;
            // AnyCard with no zone is the nearest (see report).
            out.read(RelF::You, None, Some(ObjF::AnyCard), AggF::Characteristic);
            out.effect(
                EffectAtom::new(EvF::LifeGain)
                    .player(player_ref(*player))
                    .obj(ObjF::Player)
                    .amount(AmtF::Dynamic),
            );
        }
        EffectOp::MoveAllTargets { to_zone } => {
            // Each still-valid announced object target moves independently,
            // in one batch, to its owner's `to_zone` (as `MoveObject`, once
            // per target). Player targets are skipped.
            let to = ZoneF::from(*to_zone);
            for (from, rel, obj) in object_target_slots(env) {
                // Zone changes put the card in its owner's zone.
                let player = rel.unwrap_or(RelF::ObjectOwner);
                if from == ZoneF::Stack && to != ZoneF::Stack {
                    // A live spell leaving the stack without resolving.
                    out.effect(EffectAtom::new(EvF::CounterSpell).player(player).obj(obj));
                }
                out.effect(EffectAtom::moving(Some(from), to).player(player).obj(obj));
            }
        }
        EffectOp::ExploreTarget { object } => {
            // The ability controller reveals their top card to both
            // players. A land goes to hand; otherwise the target creature
            // gets a +1/+1 counter and the controller may put the revealed
            // card into the graveyard.
            let creature = object_ref(*object, env);
            let creature_rel = object_whose(*object, env, RelF::ObjectController);
            out.effect(library_view(
                EvF::Reveal,
                RelF::You,
                ObjF::AnyCard,
                AmtF::fixed(1),
            ));
            out.control(ControlF::Conditional);
            // The branch reads whether the revealed top card is a land.
            out.read(
                RelF::You,
                Some(ZoneF::Library),
                Some(ObjF::Typed(CardTypeF::Land)),
                AggF::Any,
            );
            out.effect(
                EffectAtom::moving(Some(ZoneF::Library), ZoneF::Hand)
                    .player(RelF::You)
                    .obj(ObjF::Typed(CardTypeF::Land))
                    .amount(AmtF::fixed(1)),
            );
            out.effect(
                EffectAtom::new(EvF::PlaceCounter)
                    .player(creature_rel)
                    .obj(creature)
                    .amount(AmtF::fixed(1))
                    .duration(DurF::Permanent),
            );
            // The +1/+1 counter raises power and toughness while it remains.
            out.effect(
                EffectAtom::new(EvF::StatChange)
                    .player(creature_rel)
                    .obj(creature)
                    .amount(AmtF::fixed(1))
                    .duration(DurF::Permanent),
            );
            out.control(ControlF::Optional);
            // Vocabulary gap: no "nonland card" class; AnyCard (see report).
            out.effect(
                EffectAtom::moving(Some(ZoneF::Library), ZoneF::Graveyard)
                    .player(RelF::You)
                    .obj(ObjF::AnyCard)
                    .amount(AmtF::fixed(1)),
            );
        }
        EffectOp::Surveil { player, count } => {
            // A private look and graveyard subset, then ordering the kept
            // complement for counts greater than one.
            let player = player_ref(*player);
            let amount = AmtF::fixed(i64::from(*count));
            out.effect(library_view(EvF::Look, player, ObjF::AnyCard, amount));
            out.control(ControlF::ChooseObjects);
            if *count > 1 {
                let mut reorder = EffectAtom::new(EvF::Reorder)
                    .player(player)
                    .obj(ObjF::AnyCard)
                    .amount(amount);
                reorder.from = Some(ZoneF::Library);
                out.effect(reorder);
            }
            out.effect(
                EffectAtom::moving(Some(ZoneF::Library), ZoneF::Graveyard)
                    .player(player)
                    .obj(ObjF::AnyCard)
                    .amount(amount),
            );
        }
        EffectOp::EachPlayerControllingDefinitionDrawsCard { card_def } => {
            // For each player: if they control a permanent with this exact
            // card definition, they draw one card. No decision is offered.
            out.referenced_cards.push(*card_def);
            out.control(ControlF::Conditional);
            out.read(
                RelF::EachPlayer,
                Some(ZoneF::Battlefield),
                Some(ObjF::SpecificCard),
                AggF::Any,
            );
            out.effect(
                EffectAtom::moving(Some(ZoneF::Library), ZoneF::Hand)
                    .player(RelF::EachPlayer)
                    .obj(ObjF::AnyCard)
                    .amount(AmtF::fixed(1)),
            );
        }
        EffectOp::MoveBoundObject {
            object,
            to_zone,
            preserve_known_identity,
        } => {
            // runtime-only: the interpreter creates this as the graveyard
            // option of Explore and Surveil (whose arms emit that move);
            // generated card programs never contain a pre-bound object.
            let _ = object; // engine-internal exact-incarnation binding
            let _ = to_zone; // emitted by the creating Explore/Surveil arm
            let _ = preserve_known_identity; // hidden-information bookkeeping
        }
        EffectOp::GrantKeywordTargetUntilEndOfTurn { object, keyword } => {
            // Applies only while the object is on the battlefield; a layer-6
            // ability-adding effect for that incarnation until end of turn.
            for bit in keyword_bits(*keyword) {
                out.effect(
                    EffectAtom::new(EvF::GrantKeyword)
                        .player(object_whose(*object, env, RelF::ObjectController))
                        .obj(object_ref(*object, env))
                        .duration(DurF::EndOfTurn)
                        .keyword(bit),
                );
            }
        }
        EffectOp::PumpTargetByControlledSubtypeCount { target, subtype } => {
            // X = permanents the controller controls with the subtype,
            // sampled at resolution; the target creature gets +X/+X until
            // end of turn. Vocabulary gap: ObjF has no subtype refinement,
            // so the counted class is Permanent (see report).
            let _ = subtype;
            out.read(
                RelF::You,
                Some(ZoneF::Battlefield),
                Some(ObjF::Permanent),
                AggF::Count,
            );
            out.effect(
                EffectAtom::new(EvF::StatChange)
                    .player(object_whose(*target, env, RelF::ObjectController))
                    .obj(object_ref(*target, env))
                    .amount(AmtF::Dynamic)
                    .duration(DurF::EndOfTurn),
            );
        }
        EffectOp::SearchLibraryToHandUpTo {
            player,
            filter,
            max_targets,
        } => {
            // Choose zero through `max_targets` matching cards, put them into
            // hand, reveal each (unless the filter is unrestricted), then
            // shuffle.
            let player = player_ref(*player);
            let obj = reads::library_card_filter(*filter, out);
            let amount = AmtF::fixed(i64::from(*max_targets));
            out.control(ControlF::ChooseObjects);
            out.effect(
                EffectAtom::moving(Some(ZoneF::Library), ZoneF::Hand)
                    .player(player)
                    .obj(obj)
                    .amount(amount),
            );
            if filter.reveals_selected_card() {
                let mut reveal = EffectAtom::new(EvF::Reveal)
                    .player(player)
                    .obj(obj)
                    .amount(amount);
                reveal.from = Some(ZoneF::Hand);
                out.effect(reveal);
            }
            let mut shuffle = EffectAtom::new(EvF::Shuffle).player(player);
            shuffle.from = Some(ZoneF::Library);
            out.effect(shuffle);
        }
        EffectOp::DamageAllCreatures { filter, amount } => {
            // One damage batch to every creature on both battlefields that
            // matches the filter predicate. The matcher checks control only
            // for `OpponentControlled` (the filter's relation); every other
            // filter reaches creatures on both sides.
            let (obj, filter_rel) = reads::creature_filter(*filter);
            out.effect(
                EffectAtom::new(EvF::Damage)
                    .player(filter_rel.unwrap_or(RelF::EachPlayer))
                    .obj(obj)
                    .amount(AmtF::fixed(i64::from(*amount))),
            );
        }
        EffectOp::ExilePlayersGraveyard { player } => {
            out.effect(
                EffectAtom::moving(Some(ZoneF::Graveyard), ZoneF::Exile)
                    .player(player_ref(*player))
                    .obj(ObjF::AnyCard)
                    .amount(AmtF::All),
            );
        }
        EffectOp::ExileOneFromPlayersGraveyard { player } => {
            // The selected player chooses one card of their own graveyard.
            out.control(ControlF::ChooseObjects);
            out.effect(
                EffectAtom::moving(Some(ZoneF::Graveyard), ZoneF::Exile)
                    .player(player_ref(*player))
                    .obj(ObjF::AnyCard)
                    .amount(AmtF::fixed(1)),
            );
        }
        EffectOp::MayExileFromPlayersGraveyardMatchingThen {
            player,
            card_type,
            then,
        } => {
            // When a matching card exists, the selected player may exile
            // one of their choice from their own graveyard; only if they
            // do, `then` runs in the same context (targets kept).
            out.control(ControlF::Optional);
            out.control(ControlF::ChooseObjects);
            out.effect(
                EffectAtom::moving(Some(ZoneF::Graveyard), ZoneF::Exile)
                    .player(player_ref(*player))
                    .obj(ObjF::from(*card_type))
                    .amount(AmtF::fixed(1)),
            );
            super::effect_op(then, env, out);
        }
        EffectOp::ExileAllGraveyards => {
            out.effect(
                EffectAtom::moving(Some(ZoneF::Graveyard), ZoneF::Exile)
                    .player(RelF::EachPlayer)
                    .obj(ObjF::AnyCard)
                    .amount(AmtF::All),
            );
        }
        EffectOp::MayPayManaThen {
            player,
            colored,
            generic,
            then,
        } => {
            // Offered only when payable: the player may pay the mana; on
            // payment `then` runs in the same context (targets kept).
            // Vocabulary gap: no effect-level mana payment with a payer and
            // an amount; CostAtom::Mana is the nearest (see report).
            let payer = player_ref(*player);
            let _ = payer; // no payer slot on a cost atom (see report)
            let _ = (colored, generic); // no amount/color slot on CostAtom::Mana
            out.control(ControlF::Optional);
            out.cost(CostAtom::Mana);
            super::effect_op(then, env, out);
        }
        EffectOp::DamageAllTargets { amount } => {
            // One damage batch to each still-valid announced target that is
            // a battlefield creature; player targets and targets elsewhere
            // are skipped.
            for (zone, rel, obj) in object_target_slots(env) {
                // The executor damages only a battlefield creature, whatever
                // the slot's wider class.
                let _ = obj;
                if zone == ZoneF::Battlefield {
                    out.effect(
                        EffectAtom::new(EvF::Damage)
                            .player(rel.unwrap_or(RelF::ObjectController))
                            .obj(ObjF::Typed(CardTypeF::Creature))
                            .amount(AmtF::fixed(i64::from(*amount))),
                    );
                }
            }
        }
        EffectOp::ExileAllArtifactTargets => {
            // Each still-valid announced target that is a battlefield
            // artifact is exiled to its owner's exile, in one batch.
            for (zone, rel, obj) in object_target_slots(env) {
                // The executor exiles only a battlefield artifact, whatever
                // the slot's wider class.
                let _ = obj;
                if zone == ZoneF::Battlefield {
                    out.effect(
                        EffectAtom::moving(Some(ZoneF::Battlefield), ZoneF::Exile)
                            .player(rel.unwrap_or(RelF::ObjectOwner))
                            .obj(ObjF::Typed(CardTypeF::Artifact)),
                    );
                }
            }
        }
        EffectOp::DealDamageByControlledCreatureCount { target, multiplier } => {
            // Damage = multiplier x creatures the controller controls at
            // resolution. Vocabulary gap: the multiplier has no facet; the
            // amount is Dynamic with the count read (see report).
            let _ = multiplier;
            out.read(
                RelF::You,
                Some(ZoneF::Battlefield),
                Some(ObjF::Typed(CardTypeF::Creature)),
                AggF::Count,
            );
            let (player, obj) = target_ref(*target, env);
            out.effect(
                EffectAtom::new(EvF::Damage)
                    .maybe_player(player)
                    .obj(obj)
                    .amount(AmtF::Dynamic),
            );
        }
        EffectOp::ExileTargetPlayersGraveyards => {
            // Every card in each distinctly targeted player's graveyard is
            // exiled in one batch.
            out.effect(
                EffectAtom::moving(Some(ZoneF::Graveyard), ZoneF::Exile)
                    .player(RelF::ChosenPlayer)
                    .obj(ObjF::AnyCard)
                    .amount(AmtF::All),
            );
        }
        EffectOp::RevealUntilCardTypeAndMill { player, card_type } => {
            // Reveal from the top through the first card of the type (or
            // the whole library), then put every revealed card into its
            // owner's graveyard.
            let player = player_ref(*player);
            let amount = AmtF::UntilType(CardTypeF::from(*card_type));
            out.effect(library_view(EvF::Reveal, player, ObjF::AnyCard, amount));
            out.effect(
                EffectAtom::moving(Some(ZoneF::Library), ZoneF::Graveyard)
                    .player(player)
                    .obj(ObjF::AnyCard)
                    .amount(amount),
            );
        }
        _ => unreachable!("dispatched to the wrong slice"),
    }
}
