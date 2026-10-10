//! Meaning table, EffectOp slice D: dynamic damage, bound and backup
//! counters, Aura and ninjutsu entry, land untap/destroy/search, hand
//! reveal-and-take, graveyard-to-library shuffle, Storm copies, the
//! unpreventable-damage rule, -1/-1 counters and source-linked exile.
//!
//! Every arm was written against the variant's executor in `effect.rs`
//! (`execute` for the leaves, the resumable interpreter and its frame
//! consumers for the staged choices) and, for the trigger markers, against
//! `trigger::materialize_trigger_source_program`.
//!
//! Runtime-only (no authored program contains them; their authored marker
//! carries the meaning instead): `PutPlusOnePlusOneCounterOnBoundObject`
//! (materialized from `BindPlusOnePlusOneCounterToTriggerSource`) and
//! `CreateStormCopies` (materialized from `MaterializeStormCopies`).

use super::*;

/// Whose permanent an object reference denotes, relative to the controller.
/// A target's legality (and so whose object it may be) is carried by the
/// governing target facts.
fn whose(object: ObjectRef) -> RelF {
    match object {
        ObjectRef::ThisSource => RelF::You,
        ObjectRef::Target(slot) => {
            let _ = slot; // legality of the slot is in the target facts
            RelF::ObjectController
        }
    }
}

/// One +1/+1 counter on a battlefield creature, and the power/toughness
/// change it carries while it remains.
fn one_plus_one_counter(obj: ObjF, player: RelF, out: &mut Collector) {
    out.effect(
        EffectAtom::new(EvF::PlaceCounter)
            .player(player)
            .obj(obj)
            .amount(AmtF::fixed(1))
            .duration(DurF::Permanent),
    );
    out.effect(
        EffectAtom::new(EvF::StatChange)
            .player(player)
            .obj(obj)
            .amount(AmtF::fixed(1))
            .duration(DurF::Permanent),
    );
}

/// Choose up to one matching library card, put it onto the battlefield
/// tapped, then shuffle that library (`EffectFrame::
/// SearchLibraryToBattlefieldTapped`: the shuffle happens whether or not a
/// card was selected; no reveal).
fn search_to_battlefield_tapped(player: RelF, obj: ObjF, out: &mut Collector) {
    out.control(ControlF::ChooseObjects);
    out.effect(
        EffectAtom::moving(Some(ZoneF::Library), ZoneF::Battlefield)
            .player(player)
            .obj(obj)
            .amount(AmtF::fixed(1)),
    );
    // `zone_change_to_battlefield_tapped`: the card enters tapped.
    out.effect(
        EffectAtom::new(EvF::Tap)
            .player(player)
            .obj(obj)
            .amount(AmtF::fixed(1)),
    );
    let mut shuffle = EffectAtom::new(EvF::Shuffle).player(player);
    shuffle.from = Some(ZoneF::Library);
    out.effect(shuffle);
}

/// Publicly reveal a player's whole hand (both observers).
fn reveal_hand(player: RelF, out: &mut Collector) {
    let mut reveal = EffectAtom::new(EvF::Reveal)
        .player(player)
        .obj(ObjF::AnyCard)
        .amount(AmtF::All);
    reveal.from = Some(ZoneF::Hand);
    out.effect(reveal);
}

pub(super) fn effect_op(op: &EffectOp, env: &Env, out: &mut Collector) {
    match op {
        EffectOp::DealDamageDynamic { target, amount } => {
            // The amount is sampled once from the game state at resolution,
            // then one damage event from the source goes to the recipient.
            let (player, obj) = target_ref(*target, env);
            let amount = reads::dynamic_value(*amount, out);
            out.effect(
                EffectAtom::new(EvF::Damage)
                    .maybe_player(player)
                    .obj(obj)
                    .amount(amount),
            );
        }
        EffectOp::BindPlusOnePlusOneCounterToTriggerSource => {
            // Authored trigger marker. Trigger creation binds it to the
            // source's exact incarnation; at resolution one +1/+1 counter is
            // put on the source if that incarnation is still on the
            // battlefield (`PutPlusOnePlusOneCounterOnBoundObject`).
            one_plus_one_counter(ObjF::ThisObject, RelF::You, out);
        }
        EffectOp::PutPlusOnePlusOneCounterOnBoundObject { object } => {
            // runtime-only: materialized from
            // `BindPlusOnePlusOneCounterToTriggerSource` when the trigger is
            // created; its meaning is emitted by that marker's arm.
            let _ = object; // engine-internal exact-incarnation binding
        }
        EffectOp::PutSourceOntoBattlefieldAttachedToTarget { target } => {
            // The resolving Aura spell moves from the stack to the
            // battlefield under its controller, attached to the targeted
            // battlefield creature.
            out.effect(
                EffectAtom::moving(Some(ZoneF::Stack), ZoneF::Battlefield)
                    .player(RelF::You)
                    .obj(ObjF::ThisObject),
            );
            out.effect(
                EffectAtom::new(EvF::Attach)
                    .player(whose(*target))
                    .obj(object_ref(*target, env))
                    .duration(DurF::WhileOnBattlefield),
            );
        }
        EffectOp::TapAttachedCreatureAndDamageControllerByPower => {
            // Tap the creature the Aura is attached to (live link, or the
            // last-known link if the Aura left), then that creature deals
            // damage equal to its power to the ability's controller.
            out.effect(
                EffectAtom::new(EvF::Tap)
                    .player(RelF::ObjectController)
                    .obj(ObjF::AttachedObject),
            );
            out.read(
                RelF::ObjectController,
                Some(ZoneF::Battlefield),
                Some(ObjF::AttachedObject),
                AggF::Characteristic,
            );
            out.effect(
                EffectAtom::new(EvF::Damage)
                    .player(RelF::You)
                    .obj(ObjF::Player)
                    .amount(AmtF::Dynamic),
            );
        }
        EffectOp::BackupTarget { target, keyword } => {
            // One +1/+1 counter on the battlefield object; the keyword is
            // granted until end of turn only when the object is not the
            // ability's own source (`target != ctx.source`).
            let obj = object_ref(*target, env);
            let player = whose(*target);
            one_plus_one_counter(obj, player, out);
            match target {
                ObjectRef::ThisSource => {
                    // The object is the source itself, so the engine never
                    // grants the keyword.
                    let _ = keyword;
                }
                ObjectRef::Target(slot) => {
                    let _ = slot; // legality of the slot is in the target facts
                    if *keyword != crate::card_def::Keywords::NONE {
                        // Granted only if the target is not the source at
                        // resolution: an identity check, not a state read,
                        // so no read accompanies the Conditional.
                        out.control(ControlF::Conditional);
                        for bit in keyword_bits(*keyword) {
                            out.effect(
                                EffectAtom::new(EvF::GrantKeyword)
                                    .player(player)
                                    .obj(obj)
                                    .duration(DurF::EndOfTurn)
                                    .keyword(bit),
                            );
                        }
                    }
                }
            }
        }
        EffectOp::PutSourceOntoBattlefieldTappedAndAttacking => {
            // Ninjutsu resolution: the hand source enters the battlefield
            // under its owner-controller, tapped and attacking (unblocked).
            // Being put into combat has no facet (see report).
            out.effect(
                EffectAtom::moving(Some(ZoneF::Hand), ZoneF::Battlefield)
                    .player(RelF::You)
                    .obj(ObjF::ThisObject),
            );
            out.effect(
                EffectAtom::new(EvF::Tap)
                    .player(RelF::You)
                    .obj(ObjF::ThisObject),
            );
        }
        EffectOp::UntapUpToLands {
            chooser,
            max_targets,
        } => {
            // The chooser selects zero through `max_targets` tapped lands of
            // any controller (not targets), which are untapped.
            // Every authored use has the controller choose; the control
            // vocabulary has no chooser slot (see report).
            let _ = chooser;
            out.control(ControlF::ChooseObjects);
            // The candidates are every player's tapped lands
            // (`tapped_land_bindings`); siblings use EachPlayer for an
            // any-controller object pool.
            out.effect(
                EffectAtom::new(EvF::Untap)
                    .player(RelF::EachPlayer)
                    .obj(ObjF::Typed(CardTypeF::Land))
                    .amount(AmtF::fixed(i64::from(*max_targets))),
            );
        }
        EffectOp::DestroyTargetLandThenMaySearchBasicTapped { object } => {
            // Destroy the target land (the `DestroyObject` executor: a
            // battlefield permanent without indestructible goes to its
            // owner's graveyard), then the controller it had at resolution
            // may search their library for a basic land that enters tapped.
            let obj = object_ref(*object, env);
            let player = whose(*object);
            out.effect(
                EffectAtom::moving(Some(ZoneF::Battlefield), ZoneF::Graveyard)
                    .player(player)
                    .obj(obj),
            );
            out.control(ControlF::Optional);
            search_to_battlefield_tapped(RelF::ObjectController, ObjF::BasicLand, out);
        }
        EffectOp::SearchLibraryToBattlefieldTapped { player, filter } => {
            let player = player_ref(*player);
            let obj = reads::library_card_filter(*filter, out);
            search_to_battlefield_tapped(player, obj, out);
        }
        EffectOp::RevealTargetHandChooseNoncreatureNonlandDiscard { player } => {
            // Reveal the (opposing) player's whole hand; when it holds a
            // noncreature, nonland card the effect controller chooses one
            // and that player discards it.
            let player = player_ref(*player);
            reveal_hand(player, out);
            out.control(ControlF::ChooseObjects);
            // No noncreature-nonland card class exists (see report).
            out.effect(
                EffectAtom::moving(Some(ZoneF::Hand), ZoneF::Graveyard)
                    .player(player)
                    .obj(ObjF::AnyCard)
                    .amount(AmtF::fixed(1)),
            );
        }
        EffectOp::ShuffleTriggerSourceIntoOwnersLibrary => {
            // The leave-trigger source, if it is the exact card now in its
            // owner's graveyard, moves into that library, which is shuffled.
            out.effect(
                EffectAtom::moving(Some(ZoneF::Graveyard), ZoneF::Library)
                    .player(RelF::ObjectOwner)
                    .obj(ObjF::ThisObject),
            );
            let mut shuffle = EffectAtom::new(EvF::Shuffle).player(RelF::ObjectOwner);
            shuffle.from = Some(ZoneF::Library);
            out.effect(shuffle);
        }
        EffectOp::MaterializeStormCopies => {
            // Authored cast-trigger marker. At the SpellCast checkpoint it is
            // bound to the spells cast before the source this turn (by both
            // players); resolution then puts that many copies of the source
            // spell onto the stack (`create_storm_spell_copies`).
            out.read(RelF::EachPlayer, None, Some(ObjF::Spell), AggF::Count);
            let mut copy = EffectAtom::new(EvF::Copy)
                .player(RelF::You)
                .obj(ObjF::ThisObject)
                .amount(AmtF::Dynamic);
            copy.from = Some(ZoneF::Stack);
            copy.to = Some(ZoneF::Stack);
            out.effect(copy);
        }
        EffectOp::CreateStormCopies { binding } => {
            // runtime-only: materialized from `MaterializeStormCopies` when
            // the trigger is put on the stack; its meaning is emitted by that
            // marker's arm.
            let _ = binding; // engine-internal cast-history binding
        }
        EffectOp::DamageCannotBePreventedThisTurn => {
            // Installs a global until-end-of-turn rule under which no damage
            // can be prevented. No facet names a disabled-prevention rule;
            // nearest is a game-wide restriction (see report).
            out.effect(
                EffectAtom::new(EvF::Restrict)
                    .player(RelF::EachPlayer)
                    .duration(DurF::EndOfTurn),
            );
        }
        EffectOp::AddMinusOneMinusOneCounter { object } => {
            // One -1/-1 counter on the battlefield creature; it lowers power
            // and toughness while it remains. The counter's sign has no
            // facet slot (`AmtF::fixed` is unsigned; see report).
            let obj = object_ref(*object, env);
            let player = whose(*object);
            out.effect(
                EffectAtom::new(EvF::PlaceCounter)
                    .player(player)
                    .obj(obj)
                    .amount(AmtF::fixed(-1))
                    .duration(DurF::Permanent),
            );
            out.effect(
                EffectAtom::new(EvF::StatChange)
                    .player(player)
                    .obj(obj)
                    .amount(AmtF::fixed(-1))
                    .duration(DurF::Permanent),
            );
        }
        EffectOp::RevealHandChooseNonlandToLinkedExile { player } => {
            // Reveal the player's whole hand; the effect controller chooses
            // one nonland card from it, which is exiled linked to the
            // source (the linked return is a separate trigger).
            let player = player_ref(*player);
            reveal_hand(player, out);
            out.control(ControlF::ChooseObjects);
            // No nonland-card class exists (see report).
            out.effect(
                EffectAtom::moving(Some(ZoneF::Hand), ZoneF::Exile)
                    .player(player)
                    .obj(ObjF::AnyCard)
                    .amount(AmtF::fixed(1)),
            );
        }
        EffectOp::ReturnLinkedExiledCardToOwnersHand => {
            // The card this source incarnation exiled, if still in exile,
            // returns to its owner's hand.
            out.effect(
                EffectAtom::moving(Some(ZoneF::Exile), ZoneF::Hand)
                    .player(RelF::ObjectOwner)
                    .obj(ObjF::AnyCard)
                    .amount(AmtF::fixed(1)),
            );
        }
        EffectOp::ExileTargetLinkedToSource { object } => {
            // The target battlefield object is exiled and linked to the live
            // source incarnation (the return is a separate trigger).
            out.effect(
                EffectAtom::moving(Some(ZoneF::Battlefield), ZoneF::Exile)
                    .player(whose(*object))
                    .obj(object_ref(*object, env)),
            );
        }
        EffectOp::ReturnObjectsExiledBySource => {
            // Every object still exiled by this source incarnation returns
            // to the battlefield under its owner's control in one batch.
            // Only battlefield permanents are ever linked
            // (`ExileTargetLinkedToSource` requires one).
            out.effect(
                EffectAtom::moving(Some(ZoneF::Exile), ZoneF::Battlefield)
                    .player(RelF::ObjectOwner)
                    .obj(ObjF::Permanent)
                    .amount(AmtF::All),
            );
        }
        _ => unreachable!("dispatched to the wrong slice"),
    }
}
