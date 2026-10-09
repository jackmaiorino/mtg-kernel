//! Meaning table, EffectOp slice B.
//!
//! Each arm follows the variant's executor in `effect.rs` (the immediate
//! `execute` path and, where one exists, the resumable interpreter frame).

use super::*;

pub(super) fn effect_op(op: &EffectOp, env: &Env, out: &mut Collector) {
    /// Whose permanent an object reference denotes, relative to the controller.
    fn whose(object: ObjectRef) -> RelF {
        match object {
            ObjectRef::ThisSource => RelF::You,
            ObjectRef::Target(slot) => {
                // The slot's legality (and so whose object it may be) is carried
                // by the governing target facts.
                let _ = slot;
                RelF::ObjectController
            }
        }
    }

    /// The player who decides for a `TargetRef` recipient: the targeted player
    /// itself, or the controller of the targeted object
    /// (`OfferAffectedPlayerSpellCopy`'s `decider`).
    fn deciding_player(target: TargetRef, env: &Env) -> RelF {
        match target {
            TargetRef::ThisSource => RelF::You,
            TargetRef::Target(slot) => match env.target_obj(slot) {
                ObjF::Player | ObjF::PlayerOrPermanent => RelF::ChosenPlayer,
                ObjF::ThisObject
                | ObjF::AnyCard
                | ObjF::Typed(_)
                | ObjF::Permanent
                | ObjF::NonlandPermanent
                | ObjF::Spell
                | ObjF::Token
                | ObjF::AttachedObject
                | ObjF::EventObject
                | ObjF::SpecificCard
                | ObjF::BasicLand
                | ObjF::Ability => RelF::ObjectController,
            },
            TargetRef::Opponent => RelF::Opponent,
            TargetRef::Controller => RelF::You,
        }
    }

    /// Bucketed magnitude of a fixed power/toughness change.
    fn stat_magnitude(power: i32, toughness: i32) -> AmtF {
        AmtF::stat(i64::from(power), i64::from(toughness))
    }

    match op {
        EffectOp::DamageOpponentAndTheirCreatures { amount } => {
            // One simultaneous batch: the opponent and every creature on the
            // opponent's battlefield.
            let amount = AmtF::fixed(i64::from(*amount));
            out.effect(
                EffectAtom::new(EvF::Damage)
                    .player(RelF::Opponent)
                    .obj(ObjF::Player)
                    .amount(amount),
            );
            out.effect(
                EffectAtom::new(EvF::Damage)
                    .player(RelF::Opponent)
                    .obj(ObjF::Typed(CardTypeF::Creature))
                    .amount(amount),
            );
        }
        EffectOp::PumpControlled {
            filter,
            power,
            toughness,
            grant_haste,
        } => {
            // The executor scans only the controller's battlefield for
            // creatures matching `filter`, so the affected set is always
            // the controller's creatures; the filter narrows the class.
            let (obj, filter_rel) = reads::creature_filter(*filter);
            let _ = filter_rel; // superseded: the executor only scans the controller's battlefield
            if *power != 0 || *toughness != 0 {
                out.effect(
                    EffectAtom::new(EvF::StatChange)
                        .player(RelF::You)
                        .obj(obj)
                        .amount(stat_magnitude(*power, *toughness))
                        .duration(DurF::EndOfTurn),
                );
            }
            if *grant_haste {
                // Every creature in the snapshot gains haste.
                out.effect(
                    EffectAtom::new(EvF::GrantKeyword)
                        .player(RelF::You)
                        .obj(obj)
                        .amount(AmtF::All)
                        .duration(DurF::EndOfTurn)
                        .keyword(keyword_bits(crate::card_def::Keywords::HASTE)[0]),
                );
            }
        }
        EffectOp::ImpulseDraw { count, duration } => {
            // Exiles the controller's top `count` cards one by one and grants
            // the controller permission to play each from exile.
            let amount = AmtF::fixed(i64::from(*count));
            out.effect(
                EffectAtom::moving(Some(ZoneF::Library), ZoneF::Exile)
                    .player(RelF::You)
                    .obj(ObjF::AnyCard)
                    .amount(amount),
            );
            let duration = match duration {
                // Cleared at the very next cleanup step.
                ImpulseDuration::EndOfTurn => DurF::EndOfTurn,
                // The holder is the controller; expires at the cleanup of
                // the controller's next turn.
                ImpulseDuration::UntilOwnersNextTurn => DurF::UntilYourNextTurn,
            };
            let mut permission = EffectAtom::new(EvF::PlayPermission)
                .player(RelF::You)
                .obj(ObjF::AnyCard)
                .amount(amount)
                .duration(duration);
            permission.from = Some(ZoneF::Exile);
            out.effect(permission);
        }
        EffectOp::OfferAffectedPlayerSpellCopy { affected } => {
            // The affected player (a targeted player, or the targeted
            // object's controller) chooses whether to pay {R}{R}; on payment
            // a copy of the resolving spell is put on the stack under that
            // player's control, and they may choose a new target for it.
            // Vocabulary gap: neither the other-player decision nor the
            // {R}{R} payment has its own facet (see report).
            let decider = deciding_player(*affected, env);
            out.control(ControlF::Optional);
            // The copy of the resolving spell is put onto the stack.
            let mut copy = EffectAtom::new(EvF::Copy)
                .player(decider)
                .obj(ObjF::ThisObject);
            copy.from = Some(ZoneF::Stack);
            copy.to = Some(ZoneF::Stack);
            out.effect(copy);
        }
        EffectOp::MillCards { player, count } => {
            out.effect(
                EffectAtom::moving(Some(ZoneF::Library), ZoneF::Graveyard)
                    .player(player_ref(*player))
                    .obj(ObjF::AnyCard)
                    .amount(AmtF::fixed(i64::from(*count))),
            );
        }
        EffectOp::LookAtLibraryTopAndReorder { player, count } => {
            let player = player_ref(*player);
            let amount = AmtF::fixed(i64::from(*count));
            let mut look = EffectAtom::new(EvF::Look)
                .player(player)
                .obj(ObjF::AnyCard)
                .amount(amount);
            look.from = Some(ZoneF::Library);
            out.effect(look);
            let mut reorder = EffectAtom::new(EvF::Reorder)
                .player(player)
                .obj(ObjF::AnyCard)
                .amount(amount);
            reorder.from = Some(ZoneF::Library);
            out.effect(reorder);
        }
        EffectOp::MayShuffleLibrary { player } => {
            // A Boolean choice by the selected player; accepting shuffles.
            out.control(ControlF::Optional);
            let mut shuffle = EffectAtom::new(EvF::Shuffle).player(player_ref(*player));
            shuffle.from = Some(ZoneF::Library);
            out.effect(shuffle);
        }
        EffectOp::PutCardsFromHandOnLibraryTop { player, count } => {
            // The player privately chooses each card from hand in turn.
            out.control(ControlF::ChooseObjects);
            out.effect(
                EffectAtom::moving(Some(ZoneF::Hand), ZoneF::Library)
                    .player(player_ref(*player))
                    .obj(ObjF::AnyCard)
                    .amount(AmtF::fixed(i64::from(*count))),
            );
        }
        EffectOp::Scry { player, count } => {
            // Private look at the top cards, choose a subset for the bottom,
            // then order both groups. Cards stay in the library.
            let player = player_ref(*player);
            let amount = AmtF::fixed(i64::from(*count));
            let mut look = EffectAtom::new(EvF::Look)
                .player(player)
                .obj(ObjF::AnyCard)
                .amount(amount);
            look.from = Some(ZoneF::Library);
            out.effect(look);
            out.control(ControlF::ChooseObjects);
            let mut reorder = EffectAtom::new(EvF::Reorder)
                .player(player)
                .obj(ObjF::AnyCard)
                .amount(amount);
            reorder.from = Some(ZoneF::Library);
            out.effect(reorder);
        }
        EffectOp::SearchLibraryToHand { player, filter } => {
            // Choose zero or one matching card, move it to hand, reveal it
            // (unless the filter is unrestricted), then shuffle.
            let player = player_ref(*player);
            let obj = reads::library_card_filter(*filter, out);
            out.control(ControlF::ChooseObjects);
            out.effect(
                EffectAtom::moving(Some(ZoneF::Library), ZoneF::Hand)
                    .player(player)
                    .obj(obj)
                    .amount(AmtF::fixed(1)),
            );
            if filter.reveals_selected_card() {
                let mut reveal = EffectAtom::new(EvF::Reveal)
                    .player(player)
                    .obj(obj)
                    .amount(AmtF::fixed(1));
                reveal.from = Some(ZoneF::Hand);
                out.effect(reveal);
            }
            let mut shuffle = EffectAtom::new(EvF::Shuffle).player(player);
            shuffle.from = Some(ZoneF::Library);
            out.effect(shuffle);
        }
        EffectOp::PutObjectInOwnersLibrarySecondOrBottom { object } => {
            // The object's owner chooses between second-from-top and bottom
            // of their own library; the object must be on the battlefield.
            out.control(ControlF::ChooseBranch);
            out.effect(
                EffectAtom::moving(Some(ZoneF::Battlefield), ZoneF::Library)
                    .player(RelF::ObjectOwner)
                    .obj(object_ref(*object, env)),
            );
        }
        EffectOp::PutBoundObjectInOwnersLibrary {
            object,
            owner,
            placement,
        } => {
            // runtime-only: the interpreter creates this as the option
            // payload of `PutObjectInOwnersLibrarySecondOrBottom` /
            // `PutObjectInOwnersLibraryTopOrBottom`; generated card programs
            // may not contain it, and its facts are emitted by those arms.
            let _ = object; // engine-internal live-object binding
            let _ = owner; // concrete player id captured at resolution
            let _ = placement; // the branch already represented by the parent's choice
        }
        EffectOp::DestroyObject { object } => {
            // A battlefield permanent without indestructible goes to its
            // owner's graveyard.
            out.effect(
                EffectAtom::moving(Some(ZoneF::Battlefield), ZoneF::Graveyard)
                    .player(whose(*object))
                    .obj(object_ref(*object, env)),
            );
        }
        EffectOp::CounterUnlessPaysGeneric {
            ward_target,
            targeting_stack_item,
            generic,
        } => {
            // runtime-only: built by trigger construction from the card
            // definition's `ward_cost` when an opposing stack item targets
            // the permanent; no authored program contains it.
            let _ = ward_target; // engine-internal stack target contract
            let _ = targeting_stack_item; // engine-internal stack incarnation id
            let _ = generic; // comes from the definition's ward cost, mapped there
        }
        EffectOp::DamageEachCreatureWithoutSubtype {
            amount,
            excluded_subtype,
        } => {
            // One simultaneous batch to every creature on both battlefields
            // lacking the excluded subtype.
            // Vocabulary gap: ObjF has no subtype-exclusion refinement, so
            // the exclusion is not expressible (see report).
            let _ = excluded_subtype;
            out.effect(
                EffectAtom::new(EvF::Damage)
                    .player(RelF::EachPlayer)
                    .obj(ObjF::Typed(CardTypeF::Creature))
                    .amount(AmtF::fixed(i64::from(*amount))),
            );
        }
        EffectOp::CounterTargetUnlessPaysGeneric { target, generic } => {
            // The targeted spell's controller may pay {generic}; if they do
            // not (or cannot), the spell is countered into its owner's
            // graveyard (owner, as for every other stack-to-graveyard
            // counter). Vocabulary gap: the "unless that player pays N"
            // structure and N have no facet; Conditional is the nearest.
            let (target_player, obj) = target_ref(*target, env);
            let _ = target_player; // a spell target is never a player
            out.control(ControlF::Conditional);
            out.effect(
                EffectAtom::new(EvF::PayMana)
                    .player(RelF::ObjectController)
                    .amount(AmtF::fixed(i64::from(*generic))),
            );
            out.effect(
                EffectAtom::new(EvF::CounterSpell)
                    .player(RelF::ObjectOwner)
                    .obj(obj),
            );
            out.effect(
                EffectAtom::moving(Some(ZoneF::Stack), ZoneF::Graveyard)
                    .player(RelF::ObjectOwner)
                    .obj(obj),
            );
        }
        EffectOp::GainLifeDynamic { player, amount } => {
            let amount = reads::dynamic_value(*amount, out);
            out.effect(
                EffectAtom::new(EvF::LifeGain)
                    .player(player_ref(*player))
                    .obj(ObjF::Player)
                    .amount(amount),
            );
        }
        EffectOp::UntapObject { object } => {
            out.effect(
                EffectAtom::new(EvF::Untap)
                    .player(whose(*object))
                    .obj(object_ref(*object, env)),
            );
        }
        EffectOp::PumpTargetUntilEndOfTurnDynamic {
            target,
            power,
            toughness,
        } => {
            // Both values are sampled once at resolution; the executor
            // requires an object target (a battlefield creature).
            let (target_player, obj) = target_ref(*target, env);
            let player = match target {
                TargetRef::ThisSource => RelF::You,
                TargetRef::Target(slot) => {
                    let _ = slot; // legality of the slot is in the target facts
                    target_player.unwrap_or(RelF::ObjectController)
                }
                // Player recipients make the executor panic; kept total.
                TargetRef::Opponent => RelF::Opponent,
                TargetRef::Controller => RelF::You,
            };
            let power_amount = reads::dynamic_value(*power, out);
            // The atom carries the larger change: Dynamic when either value
            // is state-dependent, otherwise the larger fixed bucket.
            let amount = if toughness == power {
                power_amount
            } else {
                power_amount.max(reads::dynamic_value(*toughness, out))
            };
            out.effect(
                EffectAtom::new(EvF::StatChange)
                    .player(player)
                    .obj(obj)
                    .amount(amount)
                    .duration(DurF::EndOfTurn),
            );
        }
        EffectOp::LookTopSelectByTypeToHandBottomRest {
            player,
            count,
            card_type,
        } => {
            // Private look at the top `count`; choose any number of cards of
            // the type and put them into hand, where each is then publicly
            // revealed; the rest go to the bottom in the player's chosen
            // order.
            let player = player_ref(*player);
            let amount = AmtF::fixed(i64::from(*count));
            let selected = ObjF::from(*card_type);
            let mut look = EffectAtom::new(EvF::Look)
                .player(player)
                .obj(ObjF::AnyCard)
                .amount(amount);
            look.from = Some(ZoneF::Library);
            out.effect(look);
            out.control(ControlF::ChooseObjects);
            out.effect(
                EffectAtom::moving(Some(ZoneF::Library), ZoneF::Hand)
                    .player(player)
                    .obj(selected)
                    .amount(amount),
            );
            let mut reveal = EffectAtom::new(EvF::Reveal)
                .player(player)
                .obj(selected)
                .amount(amount);
            reveal.from = Some(ZoneF::Hand);
            out.effect(reveal);
            let mut reorder = EffectAtom::new(EvF::Reorder)
                .player(player)
                .obj(ObjF::AnyCard)
                .amount(amount);
            reorder.from = Some(ZoneF::Library);
            out.effect(reorder);
        }
        _ => unreachable!("dispatched to the wrong slice"),
    }
}
