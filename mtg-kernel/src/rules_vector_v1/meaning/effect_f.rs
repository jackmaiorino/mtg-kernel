//! Meaning table, EffectOp slice F: trigger-bound counters and temporary
//! boosts, mass boosts, attacking-count life gain, one-sided creature
//! damage, combat damage prevention, returning permanents, owner-library
//! placement, single-card surveil, a board wipe, multi-card searches to a
//! destination and dynamic token creation.
//!
//! Every arm was written against the variant's executor in `effect::execute`
//! or the resumable interpreter in `effect.rs`, and for the trigger markers
//! against `trigger::materialize_trigger_source_program` /
//! `materialize_trigger_event_effect`.
//!
//! Runtime-only variants (the engine builds them at trigger collection or as
//! a resolution-time choice payload; no authored program contains them, and
//! their meaning is emitted by the authored arm that produces them):
//! `PutPlusOnePlusOneCounterOnTriggerEventObject` (from
//! `BindPlusOnePlusOneCounterToTriggerEventObject`),
//! `BoostBoundObjectUntilEndOfTurn` (from
//! `BindTemporaryBoostToTriggerSource`), `DoublePlusOneCountersOnBoundObject`
//! (from `BindDoublePlusOneCountersToTriggerSource`) and
//! `PutBoundAuraOntoBattlefieldAttached` (the answered host choice of
//! `ReturnTargetPermanentToBattlefield`).

use super::*;

/// Bucketed magnitude of a fixed power/toughness change (mirrors slice B).
fn stat_magnitude(power: i32, toughness: i32) -> AmtF {
    AmtF::stat(i64::from(power), i64::from(toughness))
}

/// A `+power/+toughness until end of turn` on one object or class. The
/// engine's `install_temporary_boost` installs nothing for a 0/0 change.
fn temporary_boost(power: i32, toughness: i32, player: RelF, obj: ObjF, out: &mut Collector) {
    if power != 0 || toughness != 0 {
        out.effect(
            EffectAtom::new(EvF::StatChange)
                .player(player)
                .obj(obj)
                .amount(stat_magnitude(power, toughness))
                .duration(DurF::EndOfTurn),
        );
    }
}

/// An atom that looks at or reveals library cards.
fn library_view(ev: EvF, player: RelF, obj: ObjF, amount: AmtF) -> EffectAtom {
    let mut atom = EffectAtom::new(ev).player(player).obj(obj).amount(amount);
    atom.from = Some(ZoneF::Library);
    atom
}

/// The shuffle of a player's library.
fn shuffle(player: RelF) -> EffectAtom {
    let mut atom = EffectAtom::new(EvF::Shuffle).player(player);
    atom.from = Some(ZoneF::Library);
    atom
}

pub(super) fn effect_op(op: &EffectOp, env: &Env, out: &mut Collector) {
    match op {
        EffectOp::BindPlusOnePlusOneCounterToTriggerEventObject => {
            // Authored trigger marker. Collection binds it to the object
            // that entered the battlefield in the triggering event; at
            // resolution that exact incarnation, if still on the
            // battlefield, gets one +1/+1 counter
            // (`PutPlusOnePlusOneCounterOnTriggerEventObject`). Whose
            // creature it is comes from the trigger condition's facts.
            out.effect(
                EffectAtom::new(EvF::PlaceCounter)
                    .player(RelF::ObjectController)
                    .obj(ObjF::EventObject)
                    .amount(AmtF::fixed(1))
                    .duration(DurF::Permanent),
            );
            out.effect(
                EffectAtom::new(EvF::StatChange)
                    .player(RelF::ObjectController)
                    .obj(ObjF::EventObject)
                    .amount(AmtF::fixed(1))
                    .duration(DurF::Permanent),
            );
        }
        EffectOp::PutPlusOnePlusOneCounterOnTriggerEventObject { object } => {
            // runtime-only: materialized from
            // `BindPlusOnePlusOneCounterToTriggerEventObject` when the
            // trigger is collected; its meaning is emitted by that marker.
            let _ = object; // engine-internal exact-incarnation binding
        }
        EffectOp::BindTemporaryBoostToTriggerSource { power, toughness } => {
            // Authored trigger marker. Collection binds it to the source's
            // exact incarnation; at resolution that incarnation, if still on
            // the battlefield, gets +power/+toughness until end of turn
            // (`BoostBoundObjectUntilEndOfTurn`).
            temporary_boost(*power, *toughness, RelF::You, ObjF::ThisObject, out);
        }
        EffectOp::BoostBoundObjectUntilEndOfTurn {
            object,
            power,
            toughness,
        } => {
            // runtime-only: materialized from
            // `BindTemporaryBoostToTriggerSource` when the trigger is
            // collected; its meaning is emitted by that marker.
            let _ = object; // engine-internal exact-incarnation binding
            let _ = (power, toughness); // copied from the marker, emitted there
        }
        EffectOp::BoostPlayerCreaturesUntilEndOfTurn {
            player,
            power,
            toughness,
            keywords,
        } => {
            let player = player_ref(*player);
            let obj = ObjF::Typed(CardTypeF::Creature);
            temporary_boost(*power, *toughness, player, obj, out);
            for bit in keyword_bits(*keywords) {
                out.effect(
                    EffectAtom::new(EvF::GrantKeyword)
                        .player(player)
                        .obj(obj)
                        .amount(AmtF::All)
                        .duration(DurF::EndOfTurn)
                        .keyword(bit),
                );
            }
        }
        EffectOp::BoostControlledCreaturesUntilEndOfTurn {
            power,
            toughness,
            keywords,
        } => {
            // Snapshot of the creatures on the controller's battlefield at
            // resolution; each gets the boost and the keywords until end of
            // turn (P/T layer and ability-adding layer, each skipped when
            // empty).
            let obj = ObjF::Typed(CardTypeF::Creature);
            temporary_boost(*power, *toughness, RelF::You, obj, out);
            for bit in keyword_bits(*keywords) {
                out.effect(
                    EffectAtom::new(EvF::GrantKeyword)
                        .player(RelF::You)
                        .obj(obj)
                        .amount(AmtF::All)
                        .duration(DurF::EndOfTurn)
                        .keyword(bit),
                );
            }
        }
        EffectOp::GainLifeByAttackingSubtypeCount { player, subtype } => {
            // Counts the current attackers on the battlefield controlled by
            // `player` with the subtype, sampled at resolution, and that
            // player gains that much life (nothing for zero).
            // Vocabulary gap: ObjF has no attacking or subtype refinement,
            // so the counted class is Creature (see report).
            let _ = subtype;
            let player = player_ref(*player);
            out.read(
                player,
                Some(ZoneF::Battlefield),
                Some(ObjF::Typed(CardTypeF::Creature)),
                AggF::Count,
            );
            out.effect(
                EffectAtom::new(EvF::LifeGain)
                    .player(player)
                    .obj(ObjF::Player)
                    .amount(AmtF::Dynamic),
            );
        }
        EffectOp::CreatureTargetPowerDamage {
            source_index,
            target_index,
            plus1_plus1,
            target_spec,
        } => {
            // Legality is checked against the variant's own target spec.
            // If the source-creature target is legal: it first gets
            // `plus1_plus1` +1/+1 counters (the executor runs
            // `AddCountersToTarget` on that slot), then, if the second target
            // is still legal, it deals damage equal to its current power
            // (nothing when that is 0 or less) to that target. One-sided:
            // the target deals no damage back.
            let spec_env = Env {
                target_spec: *target_spec,
            };
            if *plus1_plus1 != 0 {
                super::effect_op(
                    &EffectOp::AddCountersToTarget {
                        target_index: *source_index,
                        optional: false,
                        plus1_plus1: *plus1_plus1,
                        lifelink: 0,
                        stun: 0,
                    },
                    &spec_env,
                    out,
                );
            }
            out.read(
                RelF::ObjectController,
                Some(ZoneF::Battlefield),
                Some(spec_env.target_obj(*source_index)),
                AggF::Characteristic,
            );
            let (player, obj) = target_ref(TargetRef::Target(*target_index), &spec_env);
            out.effect(
                EffectAtom::new(EvF::Damage)
                    .maybe_player(player)
                    .obj(obj)
                    .amount(AmtF::Dynamic),
            );
        }
        EffectOp::PreventCombatDamageToTargetThisTurn { target_index } => {
            // Installs a replacement on the targeted battlefield object's
            // current incarnation preventing all combat damage that would be
            // dealt to it this turn. Vocabulary gap: no combat-only
            // qualifier on PreventDamage (see report).
            out.effect(
                EffectAtom::new(EvF::PreventDamage)
                    .player(RelF::ObjectController)
                    .obj(env.target_obj(*target_index))
                    .amount(AmtF::All)
                    .duration(DurF::EndOfTurn),
            );
        }
        EffectOp::FightObjects {
            first,
            second,
            target_spec,
        } => {
            let spec_env = Env {
                target_spec: *target_spec,
            };
            out.control(ControlF::Conditional);
            for (source, recipient) in [(*first, *second), (*second, *first)] {
                out.read(
                    RelF::ObjectController,
                    Some(ZoneF::Battlefield),
                    Some(object_ref(source, &spec_env)),
                    AggF::Characteristic,
                );
                out.effect(
                    EffectAtom::new(EvF::Damage)
                        .obj(object_ref(recipient, &spec_env))
                        .amount(AmtF::Dynamic),
                );
            }
            // Reciprocal source-to-recipient power bindings and simultaneous
            // noncombat timing have no facets in the fixed vocabulary.
            out.atoms.push(Atom::Opaque);
        }
        EffectOp::BindDoublePlusOneCountersToTriggerSource => {
            // Authored trigger marker. Collection binds it to the source's
            // exact incarnation; at resolution, if that incarnation is on the
            // battlefield with N > 0 +1/+1 counters, it gets N more
            // (`DoublePlusOneCountersOnBoundObject`). N is read from the
            // source at resolution.
            out.read(
                RelF::You,
                Some(ZoneF::Battlefield),
                Some(ObjF::ThisObject),
                AggF::Characteristic,
            );
            out.effect(
                EffectAtom::new(EvF::DoubleCounters)
                    .player(RelF::You)
                    .obj(ObjF::ThisObject)
                    .amount(AmtF::Dynamic)
                    .duration(DurF::Permanent),
            );
            out.effect(
                EffectAtom::new(EvF::StatChange)
                    .player(RelF::You)
                    .obj(ObjF::ThisObject)
                    .amount(AmtF::Dynamic)
                    .duration(DurF::Permanent),
            );
        }
        EffectOp::DoublePlusOneCountersOnBoundObject { object } => {
            // runtime-only: materialized from
            // `BindDoublePlusOneCountersToTriggerSource` when the trigger is
            // collected; its meaning is emitted by that marker.
            let _ = object; // engine-internal exact-incarnation binding
        }
        EffectOp::ReturnTargetPermanentToBattlefield { target_index } => {
            // The targeted card, if still the same incarnation in its
            // owner's graveyard, enters the battlefield. A non-Aura moves
            // directly. A creature Aura instead needs a host: the
            // controller chooses one creature on either battlefield (not a
            // target; skipped when exactly one is legal, nothing happens
            // when none is), and the Aura enters attached to it
            // (`PutBoundAuraOntoBattlefieldAttached`).
            let obj = env.target_obj(*target_index);
            out.effect(
                EffectAtom::moving(Some(ZoneF::Graveyard), ZoneF::Battlefield)
                    .player(RelF::ObjectOwner)
                    .obj(obj),
            );
            out.control(ControlF::Conditional);
            out.control(ControlF::ChooseObjects);
            out.effect(
                EffectAtom::new(EvF::Attach)
                    .player(RelF::EachPlayer)
                    .obj(ObjF::Typed(CardTypeF::Creature))
                    .duration(DurF::WhileOnBattlefield),
            );
        }
        EffectOp::PutBoundAuraOntoBattlefieldAttached { aura, host } => {
            // runtime-only: the interpreter builds this from the answered
            // host choice of `ReturnTargetPermanentToBattlefield`, whose arm
            // emits the move and the attachment.
            let _ = aura; // engine-internal graveyard-incarnation binding
            let _ = host; // engine-internal chosen-host binding
        }
        EffectOp::PutObjectInOwnersLibraryTopOrBottom { object } => {
            // The battlefield object's owner chooses top or bottom of their
            // own library (mirrors slice B's second-or-bottom arm).
            out.control(ControlF::ChooseBranch);
            out.effect(
                EffectAtom::moving(Some(ZoneF::Battlefield), ZoneF::Library)
                    .player(RelF::ObjectOwner)
                    .obj(object_ref(*object, env)),
            );
        }
        EffectOp::SurveilOne { player } => {
            // A private look at the top nontoken card of the player's
            // library, then the player chooses zero or one of it to put into
            // the graveyard (otherwise it stays on top).
            let player = player_ref(*player);
            let amount = AmtF::fixed(1);
            out.effect(library_view(EvF::Look, player, ObjF::AnyCard, amount));
            out.control(ControlF::ChooseObjects);
            out.effect(
                EffectAtom::moving(Some(ZoneF::Library), ZoneF::Graveyard)
                    .player(player)
                    .obj(ObjF::AnyCard)
                    .amount(amount),
            );
        }
        EffectOp::BoostAttachedCreatureUntilEndOfTurn { power, toughness } => {
            // The creature the source Equipment is attached to (current
            // attachment, else last-known) gets the boost until end of turn;
            // nothing when the source is unattached.
            temporary_boost(
                *power,
                *toughness,
                RelF::ObjectController,
                ObjF::AttachedObject,
                out,
            );
        }
        EffectOp::DestroyAllCreatures => {
            // One simultaneous batch: every creature on both battlefields
            // without indestructible goes to its owner's graveyard.
            // Vocabulary gap: the indestructible exemption has no facet.
            out.effect(
                EffectAtom::moving(Some(ZoneF::Battlefield), ZoneF::Graveyard)
                    .player(RelF::EachPlayer)
                    .obj(ObjF::Typed(CardTypeF::Creature))
                    .amount(AmtF::All),
            );
        }
        EffectOp::SearchLibraryCardsToDestination {
            player,
            filter,
            max_targets,
            destination,
        } => {
            // Choose zero through `max_targets` matching cards from the
            // player's library (zero is always legal).
            let player = player_ref(*player);
            let obj = reads::library_card_filter(*filter, out);
            let amount = AmtF::fixed(i64::from(*max_targets));
            out.control(ControlF::ChooseObjects);
            match destination {
                LibrarySearchDestinationV1::Battlefield { tapped } => {
                    // The selected cards enter the battlefield as one batch
                    // (tapped when `tapped`), then the library is shuffled.
                    out.effect(
                        EffectAtom::moving(Some(ZoneF::Library), ZoneF::Battlefield)
                            .player(player)
                            .obj(obj)
                            .amount(amount),
                    );
                    if *tapped {
                        out.effect(
                            EffectAtom::new(EvF::Tap)
                                .player(player)
                                .obj(obj)
                                .amount(amount),
                        );
                    }
                    out.effect(shuffle(player));
                }
                LibrarySearchDestinationV1::LibraryTopAfterShuffle => {
                    // At most one card: the library is shuffled, the selected
                    // card is moved to the top and revealed to both players.
                    out.effect(shuffle(player));
                    out.effect(library_view(EvF::Reorder, player, obj, AmtF::fixed(1)));
                    out.effect(library_view(EvF::Reveal, player, obj, AmtF::fixed(1)));
                }
            }
        }
        EffectOp::CreateTokensDynamic {
            token_def,
            controller,
            count,
            tapped,
        } => {
            // `count` is sampled once at resolution (relative to the
            // ability's controller); that many tokens are created for
            // `controller`, each tapped when `tapped`.
            let player = player_ref(*controller);
            let amount = reads::dynamic_value(*count, out);
            out.effect(
                EffectAtom::new(EvF::CreateToken)
                    .player(player)
                    .obj(ObjF::Token)
                    .amount(amount),
            );
            if *tapped {
                out.effect(
                    EffectAtom::new(EvF::Tap)
                        .player(player)
                        .obj(ObjF::Token)
                        .amount(amount),
                );
            }
            out.created_tokens.push(*token_def);
        }
        _ => unreachable!("dispatched to the wrong slice"),
    }
}
