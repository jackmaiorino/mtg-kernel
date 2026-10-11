//! Meaning table, EffectOp slice E: chosen-color damage prevention, Saga and
//! in-place transformation, look-then-may-reveal, player-directed creature
//! sacrifice, +1/+1 counters (single, counted, and X on entry), damage equal
//! to a cost creature's power, the Initiative and monarch designations,
//! team-wide until-end-of-turn pumps on a chosen board, and damage to a
//! target's controller.
//!
//! Every arm was written against the variant's executor in `effect.rs`
//! (`execute` for the leaves, the resumable interpreter and its frame and
//! answer consumers for the staged choices).
//!
//! Runtime-only (no authored program -- `build.rs` spell/ability programs,
//! `trigger.rs` templates or Saga chapters -- contains them; they are
//! constructed by the engine at trigger creation or resolution):
//! - `InstallDamagePreventionFromColor`: the option branches the color
//!   choice of `PreventDamageFromChosenColorUntilEndOfTurn` offers.
//! - `PutSourceOntoBattlefieldAttachedToTargetWithXPlusOneCounters`: built by
//!   `engine` when a spell cast for its bestow cost resolves (the extractor
//!   records the bestow form separately).
//! - `ResolveInitiativeTrigger`, `EnterUndercityRoom`,
//!   `ResolveUndercityThrone`: the engine-owned Initiative/Undercity
//!   triggers and their interpreter route markers.
//! - `GoadTargetUntilSourcesNextTurn`: only produced as the Undercity
//!   Arena room's program.
//! - `ResolveMonarchTrigger`: the engine-owned monarch end-step draw.

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

/// `amount` +1/+1 counters on a battlefield creature, and the
/// power/toughness change they carry while they remain.
fn plus_one_counters(obj: ObjF, player: RelF, amount: AmtF, out: &mut Collector) {
    out.effect(
        EffectAtom::new(EvF::PlaceCounter)
            .player(player)
            .obj(obj)
            .amount(amount)
            .duration(DurF::Permanent),
    );
    out.effect(
        EffectAtom::new(EvF::StatChange)
            .player(player)
            .obj(obj)
            .amount(amount)
            .duration(DurF::Permanent),
    );
}

/// The top card of the controller's library, looked at or revealed.
fn library_top(ev: EvF, out: &mut Collector) {
    let mut atom = EffectAtom::new(ev)
        .player(RelF::You)
        .obj(ObjF::AnyCard)
        .amount(AmtF::fixed(1));
    atom.from = Some(ZoneF::Library);
    out.effect(atom);
}

/// Magnitude of a power/toughness modifier (the sign has no facet slot; see
/// report).
fn stat_magnitude(power: i16, toughness: i16) -> AmtF {
    AmtF::stat(i64::from(power), i64::from(toughness))
}

pub(super) fn effect_op(op: &EffectOp, env: &Env, out: &mut Collector) {
    match op {
        EffectOp::PreventDamageFromChosenColorUntilEndOfTurn { player } => {
            // The player (always the controller: the installed branch halts
            // otherwise) chooses W, U, B, R or G; a replacement then
            // prevents all damage dealt this turn by any source with that
            // color, to any player or permanent, whoever controls the
            // source. "From a source of the chosen color" has no facet slot
            // (see report).
            let player = player_ref(*player);
            out.effect(EffectAtom::new(EvF::ChooseValue).player(player));
            out.effect(
                EffectAtom::new(EvF::PreventDamage)
                    .player(RelF::EachPlayer)
                    .obj(ObjF::PlayerOrPermanent)
                    .amount(AmtF::All)
                    .duration(DurF::EndOfTurn),
            );
        }
        EffectOp::InstallDamagePreventionFromColor { player, color } => {
            // runtime-only: the interpreter offers these as the options of
            // `PreventDamageFromChosenColorUntilEndOfTurn`'s color choice;
            // that arm carries the meaning.
            let _ = player; // resolved PlayerId bound at choice time
            let _ = color; // the answered choice, not definition data
        }
        EffectOp::TransformSagaSource => {
            // This exact Saga incarnation is exiled, then the same card
            // returns to the battlefield transformed (back face) under the
            // ability's controller.
            out.effect(
                EffectAtom::moving(Some(ZoneF::Battlefield), ZoneF::Exile)
                    .player(RelF::You)
                    .obj(ObjF::ThisObject),
            );
            out.effect(
                EffectAtom::moving(Some(ZoneF::Exile), ZoneF::Battlefield)
                    .player(RelF::You)
                    .obj(ObjF::ThisObject),
            );
            out.effect(
                EffectAtom::new(EvF::Transform)
                    .player(RelF::You)
                    .obj(ObjF::ThisObject),
            );
        }
        EffectOp::TransformSourceInPlace => {
            // The battlefield source flips to its transform face with no
            // zone change (a no-op if that incarnation is gone).
            out.effect(
                EffectAtom::new(EvF::Transform)
                    .player(RelF::You)
                    .obj(ObjF::ThisObject),
            );
        }
        EffectOp::LookAtTopMayRevealThen { predicate, then } => {
            // The controller privately looks at their top card, may reveal
            // it to both players, and if a revealed card's printed types
            // match the predicate, `then` runs.
            library_top(EvF::Look, out);
            out.control(ControlF::Optional);
            library_top(EvF::Reveal, out);
            out.control(ControlF::Conditional);
            // An existence check over the revealed card, encoded as the
            // `AtLeast(1)` threshold per the `reads` convention (never `Any`).
            let obj = reads::card_type_predicate(*predicate);
            out.read(RelF::You, Some(ZoneF::Library), Some(obj), AggF::AtLeast(1));
            super::effect_op(then, env, out);
        }
        EffectOp::SacrificeCreature { player, filter } => {
            // The selected player chooses one creature they control (among
            // those of greatest power, for that restriction) at resolution
            // and sacrifices it; nothing happens if they control none.
            let player = player_ref(*player);
            match filter {
                CreatureSacrificeFilter::Any
                | CreatureSacrificeFilter::Token
                | CreatureSacrificeFilter::Nontoken
                | CreatureSacrificeFilter::PermanentType(_) => {}
                CreatureSacrificeFilter::GreatestPower => {
                    out.read(
                        player,
                        Some(ZoneF::Battlefield),
                        Some(ObjF::Typed(CardTypeF::Creature)),
                        AggF::Characteristic,
                    );
                }
            }
            out.control(ControlF::ChooseObjects);
            out.effect(
                EffectAtom::moving(Some(ZoneF::Battlefield), ZoneF::Graveyard)
                    .player(player)
                    .obj(match filter {
                        CreatureSacrificeFilter::PermanentType(card_type) => {
                            ObjF::Typed((*card_type).into())
                        }
                        _ => ObjF::Typed(CardTypeF::Creature),
                    })
                    .amount(AmtF::fixed(1)),
            );
        }
        EffectOp::PutPlusOnePlusOneCounter { object } => {
            // One +1/+1 counter, only if the object is still a battlefield
            // creature (and, for a target, the same incarnation).
            plus_one_counters(
                object_ref(*object, env),
                whose(*object),
                AmtF::fixed(1),
                out,
            );
        }
        EffectOp::PutSourceOntoBattlefieldWithXPlusOneCounters => {
            // The resolving permanent spell moves from the stack to the
            // battlefield and the new incarnation gets X +1/+1 counters
            // (X as announced at cast).
            out.effect(
                EffectAtom::moving(Some(ZoneF::Stack), ZoneF::Battlefield)
                    .player(RelF::You)
                    .obj(ObjF::ThisObject),
            );
            plus_one_counters(ObjF::ThisObject, RelF::You, AmtF::X, out);
        }
        EffectOp::PutSourceOntoBattlefieldAttachedToTargetWithXPlusOneCounters { target } => {
            // runtime-only: constructed by the engine when a bestowed spell
            // resolves with its target legal; no authored program contains
            // it (see report for the mapping it would carry).
            let _ = target; // always the engine-built `Target(0)`
        }
        EffectOp::DealDamageToTargetEqualToChosenCostCreaturePower { target } => {
            // Reads the power of the creature chosen for the additional cost
            // (live on the battlefield, printed if still in hand, else
            // last-known), then the source deals that much damage.
            out.read(
                RelF::You,
                None,
                Some(ObjF::Typed(CardTypeF::Creature)),
                AggF::Characteristic,
            );
            let (player, obj) = target_ref(*target, env);
            out.effect(
                EffectAtom::new(EvF::Damage)
                    .maybe_player(player)
                    .obj(obj)
                    .amount(AmtF::Dynamic),
            );
        }
        EffectOp::TakeInitiative { player } => {
            // The player takes the Initiative designation, and the executor
            // commits the designation's venture trigger (that player
            // ventures into the Undercity). The rooms' programs belong to
            // the designation, not to this card.
            let player = player_ref(*player);
            out.effect(EffectAtom::new(EvF::TakeInitiative).player(player));
            out.effect(EffectAtom::new(EvF::VentureRoom).player(player));
        }
        EffectOp::ResolveInitiativeTrigger { binding } => {
            // runtime-only: created by `trigger` from a committed Initiative
            // event; the authored `TakeInitiative` carries the meaning.
            let _ = binding; // engine-internal event-history provenance
        }
        EffectOp::EnterUndercityRoom {
            binding,
            from_room,
            room,
        } => {
            // runtime-only: interpreter route marker for the venture choice.
            let _ = binding; // engine-internal event-history provenance
            let _ = from_room; // dungeon progress state, not definition data
            let _ = room; // the answered route choice, not definition data
        }
        EffectOp::AddPlusOnePlusOneCounters { object, count } => {
            // `count` +1/+1 counters on a battlefield creature.
            plus_one_counters(
                object_ref(*object, env),
                whose(*object),
                AmtF::fixed(i64::from(*count)),
                out,
            );
        }
        EffectOp::GoadTargetUntilSourcesNextTurn { object } => {
            // runtime-only: only the Undercity Arena room's program produces
            // it (would be: Restrict on the target creature until the
            // controller's next turn).
            let _ = object; // always the room program's `Target(0)`
        }
        EffectOp::ResolveUndercityThrone { binding } => {
            // runtime-only: the Undercity's final room, staged by the
            // engine-owned Initiative trigger.
            let _ = binding; // engine-internal event-history provenance
        }
        EffectOp::BecomeMonarch => {
            // The controller becomes the monarch; the executor only sets the
            // designation and its provenance source (the end-step draw is
            // the designation's own engine trigger).
            out.effect(EffectAtom::new(EvF::BecomeMonarch).player(RelF::You));
        }
        EffectOp::ResolveMonarchTrigger { binding } => {
            // runtime-only: created by `trigger` from a committed monarch
            // event; the authored `BecomeMonarch` carries the designation.
            let _ = binding; // engine-internal event-history provenance
        }
        EffectOp::PumpAllUntilEndOfTurn {
            filter,
            controller,
            power,
            toughness,
        } => {
            // Every permanent matching `filter` on one player's battlefield
            // (locked in at resolution) gets the modifier until the next
            // cleanup.
            let (obj, filter_rel) = reads::permanent_filter(*filter);
            let _ = filter_rel; // superseded: the scope names whose battlefield is scanned
            let player = match controller {
                PumpControllerScope::Opponents => RelF::Opponent,
                PumpControllerScope::TargetPlayer(slot) => {
                    let _ = slot; // legality of the slot is in the target facts
                    RelF::ChosenPlayer
                }
            };
            if *power != 0 || *toughness != 0 {
                out.effect(
                    EffectAtom::new(EvF::StatChange)
                        .player(player)
                        .obj(obj)
                        .amount(stat_magnitude(*power, *toughness))
                        .duration(DurF::EndOfTurn),
                );
            }
        }
        EffectOp::DealDamageToControllerOfTarget { target, amount } => {
            // The source deals `amount` damage to the player who controlled
            // the announced object target when it was announced.
            let _ = target; // which slot; its legality is in the target facts
            out.effect(
                EffectAtom::new(EvF::Damage)
                    .player(RelF::ObjectController)
                    .obj(ObjF::Player)
                    .amount(AmtF::fixed(i64::from(*amount))),
            );
        }
        _ => unreachable!("dispatched to the wrong slice"),
    }
}
