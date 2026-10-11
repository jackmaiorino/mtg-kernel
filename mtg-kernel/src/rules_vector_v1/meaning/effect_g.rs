//! Meaning table, EffectOp slice G: the MageZero Standard family G
//! creatures' resolution effects: the outgrowing-entrant recheck, oil
//! counters, a token that enters attacking, a counter on an ability's own
//! source, a restricted reanimation, losing half one's life, a stunned
//! return from the graveyard and an optional look-top creature pick.
//!
//! Every arm was written against the variant's executor in `effect.rs`, and
//! for the trigger markers against `trigger::materialize_trigger_source_program`
//! / `materialize_trigger_event_effect`.
//!
//! Runtime-only variants (built at trigger collection; no authored program
//! contains them, and their meaning is emitted by the authored marker that
//! produces them): `IfEntrantOutgrowsSourceThen` (from
//! `BindEntrantOutgrowsSourceThen`) and `PutOilCounterOnBoundObject` (from
//! `BindOilCounterToTriggerSource`).

use super::*;

/// One counter that changes no power or toughness (oil, stun) on `obj`.
fn plain_counter(player: RelF, obj: ObjF, amount: AmtF, out: &mut Collector) {
    out.effect(
        EffectAtom::new(EvF::PlaceCounter)
            .player(player)
            .obj(obj)
            .amount(amount)
            .duration(DurF::Permanent),
    );
}

pub(super) fn effect_op(op: &EffectOp, env: &Env, out: &mut Collector) {
    match op {
        EffectOp::CreatureUpgrade(effect) => {
            use crate::standard_creatures_v1::CreatureEffectV1;
            if matches!(effect, CreatureEffectV1::SalvagerBoostTokens) {
                plain_counter(
                    RelF::You,
                    ObjF::Typed(CardTypeF::Creature),
                    AmtF::fixed(1),
                    out,
                );
                out.effect(
                    EffectAtom::new(EvF::GrantKeyword)
                        .player(RelF::You)
                        .obj(ObjF::Typed(CardTypeF::Creature))
                        .duration(DurF::EndOfTurn)
                        .keyword(keyword_bits(crate::card_def::Keywords::TRAMPLE)[0]),
                );
            }
            if matches!(effect, CreatureEffectV1::HarvesterWeakening) {
                out.effect(
                    EffectAtom::new(EvF::StatChange)
                        .obj(ObjF::Typed(CardTypeF::Creature))
                        .amount(AmtF::Dynamic)
                        .duration(DurF::EndOfTurn),
                );
                out.read(
                    RelF::You,
                    Some(ZoneF::Battlefield),
                    Some(ObjF::Token),
                    AggF::Count,
                );
            }
            out.control(ControlF::Conditional);
            if matches!(effect, CreatureEffectV1::WurmletCounterIfFirstResolution) {
                plain_counter(RelF::You, ObjF::ThisObject, AmtF::fixed(1), out);
                out.read(RelF::You, None, Some(ObjF::ThisObject), AggF::EventThisTurn);
            }
            out.read(
                RelF::You,
                Some(ZoneF::Battlefield),
                Some(ObjF::ThisObject),
                AggF::Characteristic,
            );
            if matches!(effect, CreatureEffectV1::ToughCookieAnimate) {
                out.effect(
                    EffectAtom::new(EvF::StatChange)
                        .player(RelF::You)
                        .obj(ObjF::Typed(CardTypeF::Artifact))
                        .duration(DurF::EndOfTurn),
                );
            }
            if matches!(
                effect,
                CreatureEffectV1::KellanRogue | CreatureEffectV1::SurgeBlue
            ) {
                out.effect(
                    EffectAtom::new(EvF::StatChange)
                        .player(RelF::You)
                        .obj(ObjF::ThisObject)
                        .duration(DurF::Permanent),
                );
            }
            let keyword = match effect {
                CreatureEffectV1::KellanRogue => Some(crate::card_def::Keywords::DOUBLE_STRIKE),
                CreatureEffectV1::SurgeUnblockable => {
                    Some(crate::card_def::Keywords::CANT_BE_BLOCKED)
                }
                CreatureEffectV1::GingerEvasion => Some(crate::card_def::Keywords::CANT_BE_BLOCKED),
                _ => None,
            };
            if let Some(keyword) = keyword {
                let duration = if matches!(effect, CreatureEffectV1::GingerEvasion) {
                    DurF::EndOfTurn
                } else {
                    DurF::Permanent
                };
                out.effect(
                    EffectAtom::new(EvF::GrantKeyword)
                        .player(RelF::You)
                        .obj(ObjF::ThisObject)
                        .duration(duration)
                        .keyword(keyword_bits(keyword)[0]),
                );
            }
            if matches!(effect, CreatureEffectV1::KellanDetective) {
                super::effect_op(
                    &EffectOp::ImpulseDraw {
                        count: 1,
                        duration: crate::effect::ImpulseDuration::EndOfTurn,
                    },
                    env,
                    out,
                );
            }
        }
        EffectOp::DistributePlusOneCounters { .. } => {
            plain_counter(
                RelF::You,
                ObjF::Typed(CardTypeF::Creature),
                AmtF::Dynamic,
                out,
            );
            out.control(ControlF::ChooseBranch);
        }
        EffectOp::BindEntrantOutgrowsSourceThen { then } => {
            // Authored trigger marker. Collection binds the entering
            // creature and the source; at resolution `then` runs only if the
            // entrant's power or toughness is greater than the source's,
            // each read live while it remains the bound incarnation.
            out.control(ControlF::Conditional);
            out.read(
                RelF::You,
                Some(ZoneF::Battlefield),
                Some(ObjF::EventObject),
                AggF::Characteristic,
            );
            out.read(
                RelF::You,
                Some(ZoneF::Battlefield),
                Some(ObjF::ThisObject),
                AggF::Characteristic,
            );
            super::effect_op(then, env, out);
        }
        EffectOp::IfEntrantOutgrowsSourceThen {
            entrant,
            source,
            then,
        } => {
            // runtime-only: materialized from
            // `BindEntrantOutgrowsSourceThen` when the trigger is collected;
            // its meaning is emitted by that marker.
            let _ = (entrant, source); // engine-internal exact-incarnation bindings
            let _ = then; // copied from the marker, emitted there
        }
        EffectOp::BindOilCounterToTriggerSource => {
            // Authored trigger marker. Collection binds the source's exact
            // incarnation; at resolution it gets one oil counter if still on
            // the battlefield (`PutOilCounterOnBoundObject`).
            plain_counter(RelF::You, ObjF::ThisObject, AmtF::fixed(1), out);
        }
        EffectOp::PutOilCounterOnBoundObject { object } => {
            // runtime-only: materialized from
            // `BindOilCounterToTriggerSource`; its meaning is emitted there.
            let _ = object; // engine-internal exact-incarnation binding
        }
        EffectOp::CreateTokenTappedAndAttacking { token_def } => {
            // One token under the controller's control, tapped and, during
            // combat, attacking without being declared. Being put into
            // combat has no facet (as for ninjutsu in slice D).
            out.effect(
                EffectAtom::new(EvF::CreateToken)
                    .player(RelF::You)
                    .obj(ObjF::Token)
                    .amount(AmtF::fixed(1)),
            );
            out.effect(EffectAtom::new(EvF::Tap).player(RelF::You).obj(ObjF::Token));
            out.created_tokens.push(*token_def);
        }
        EffectOp::AddPlusOneCounterToAbilitySource => {
            // One +1/+1 counter on the ability's source while it is still
            // the battlefield incarnation the ability came from.
            out.effect(
                EffectAtom::new(EvF::PlaceCounter)
                    .player(RelF::You)
                    .obj(ObjF::ThisObject)
                    .amount(AmtF::fixed(1))
                    .duration(DurF::Permanent),
            );
            out.effect(
                EffectAtom::new(EvF::StatChange)
                    .player(RelF::You)
                    .obj(ObjF::ThisObject)
                    .amount(AmtF::fixed(1))
                    .duration(DurF::Permanent),
            );
        }
        EffectOp::ReturnTargetCreatureCardRestrictedWhileSourceControlled { target_index } => {
            // The targeted card, if still the same incarnation in its
            // owner's graveyard, enters the battlefield under that player's
            // control. While that player controls the ability's source it
            // can't attack or block; a source that already left imposes
            // nothing. Vocabulary gap: no "while you control" duration;
            // nearest is the source's battlefield lifetime.
            let obj = env.target_obj(*target_index);
            out.effect(
                EffectAtom::moving(Some(ZoneF::Graveyard), ZoneF::Battlefield)
                    .player(RelF::ObjectOwner)
                    .obj(obj),
            );
            out.effect(
                EffectAtom::new(EvF::Restrict)
                    .player(RelF::ObjectController)
                    .obj(obj)
                    .duration(DurF::WhileOnBattlefield),
            );
        }
        EffectOp::LoseHalfLifeRoundedUp { player } => {
            // The player loses half their life, rounded up; nothing at 0 or
            // less life.
            out.effect(
                EffectAtom::new(EvF::LifeLoss)
                    .player(player_ref(*player))
                    .obj(ObjF::Player)
                    .amount(AmtF::Half),
            );
        }
        EffectOp::ReturnSourceFromGraveyardTappedWithStunCounters { stun } => {
            // The dies trigger's source card, if it is still the graveyard
            // incarnation the death created, returns to the battlefield
            // tapped under its owner's control with `stun` stun counters.
            out.effect(
                EffectAtom::moving(Some(ZoneF::Graveyard), ZoneF::Battlefield)
                    .player(RelF::You)
                    .obj(ObjF::ThisObject),
            );
            out.effect(
                EffectAtom::new(EvF::Tap)
                    .player(RelF::You)
                    .obj(ObjF::ThisObject),
            );
            plain_counter(
                RelF::You,
                ObjF::ThisObject,
                AmtF::fixed(i64::from(*stun)),
                out,
            );
        }
        EffectOp::LookTopMayTakeCreatureManaValueAtMostToHandBottomRest {
            player,
            count,
            max_mana_value,
        } => {
            // Private look at the top `count`; the player may choose one
            // creature card with mana value at most `max_mana_value`, which
            // is put into hand and publicly revealed; the rest go to the
            // bottom in looked-at order. Vocabulary gap: the moved class
            // carries no mana-value bound.
            let _ = max_mana_value;
            let player = player_ref(*player);
            let creature = ObjF::Typed(CardTypeF::Creature);
            let mut look = EffectAtom::new(EvF::Look)
                .player(player)
                .obj(ObjF::AnyCard)
                .amount(AmtF::fixed(i64::from(*count)));
            look.from = Some(ZoneF::Library);
            out.effect(look);
            out.control(ControlF::Optional);
            out.control(ControlF::ChooseObjects);
            out.effect(
                EffectAtom::moving(Some(ZoneF::Library), ZoneF::Hand)
                    .player(player)
                    .obj(creature)
                    .amount(AmtF::fixed(1)),
            );
            let mut reveal = EffectAtom::new(EvF::Reveal)
                .player(player)
                .obj(creature)
                .amount(AmtF::fixed(1));
            reveal.from = Some(ZoneF::Hand);
            out.effect(reveal);
        }
        _ => unreachable!("dispatched to the wrong slice"),
    }
}
