//! Meaning table, EffectOp slice H: the MageZero Standard family D keyword
//! effects: non-mana Ward costs, convoke-sized library looks, battle cry,
//! incubate, warp and unearth exile, enduring returns, impending time
//! counters and a graveyard-exile drain.
//!
//! Every arm was written against the variant's executor in `effect.rs`, and
//! for the trigger markers against `trigger::materialize_trigger_source_program`
//! / `materialize_trigger_event_effect`.
//!
//! Runtime-only variants (built at trigger collection; no authored program
//! contains them, and their meaning is emitted by the authored marker that
//! produces them or by the definition's Ward cost):
//! `CounterUnlessCollectsEvidence`, `CounterUnlessPaysLife`,
//! `CounterUnlessDiscardsCard` (from `ward_cost`),
//! `LookTopTakeCreaturesManaValueAtMostThenShuffle` (from
//! `BindConvokedCreatureCountToLookTop`), `Incubate` (from
//! `BindIncubateToTriggerSpell`), `WarpExileBoundObject` (from
//! `BindWarpExileToTriggerSource`) and
//! `PutPlusOnePlusOneCounterOnTargetOtherThan` (from
//! `BindPlusOneCounterOnAnotherTargetToTriggerTarget`).

use super::*;

/// The source's exact incarnation exiled from the battlefield.
fn exile_self(out: &mut Collector) {
    out.effect(
        EffectAtom::moving(Some(ZoneF::Battlefield), ZoneF::Exile)
            .player(RelF::You)
            .obj(ObjF::ThisObject),
    );
}

pub(super) fn effect_op(op: &EffectOp, env: &Env, out: &mut Collector) {
    let _ = env;
    match op {
        EffectOp::ExileUntilThenCastV1 {
            players,
            return_rest_to_bottom,
            ..
        } => {
            for player in players {
                out.effect(
                    EffectAtom::moving(Some(ZoneF::Library), ZoneF::Exile)
                        .player(player_ref(*player))
                        .obj(ObjF::AnyCard),
                );
                if *return_rest_to_bottom {
                    out.effect(
                        EffectAtom::moving(Some(ZoneF::Exile), ZoneF::Library)
                            .player(player_ref(*player))
                            .obj(ObjF::AnyCard),
                    );
                }
            }
        }
        EffectOp::Discover { limit: _ } => {
            out.effect(
                EffectAtom::moving(Some(ZoneF::Library), ZoneF::Exile)
                    .player(RelF::You)
                    .obj(ObjF::AnyCard),
            );
            out.effect(
                EffectAtom::moving(Some(ZoneF::Exile), ZoneF::Hand)
                    .player(RelF::You)
                    .obj(ObjF::AnyCard)
                    .amount(AmtF::fixed(1)),
            );
            out.effect(
                EffectAtom::moving(Some(ZoneF::Exile), ZoneF::Library)
                    .player(RelF::You)
                    .obj(ObjF::AnyCard),
            );
        }
        EffectOp::Hideaway { count } => {
            let mut look = EffectAtom::new(EvF::Look)
                .player(RelF::You)
                .obj(ObjF::AnyCard)
                .amount(AmtF::fixed(i64::from(*count)));
            look.from = Some(ZoneF::Library);
            out.effect(look);
            out.effect(
                EffectAtom::moving(Some(ZoneF::Library), ZoneF::Exile)
                    .player(RelF::You)
                    .obj(ObjF::AnyCard)
                    .amount(AmtF::fixed(1)),
            );
        }
        EffectOp::PlayHideawayIfThreeDistinctPowers => {
            // The fixed vocabulary has no free-play or distinct-power atom.
        }
        EffectOp::CopySpellSnapshot { .. } | EffectOp::IncreaseSpeed { .. } => { /* Runtime player ability, described by StartYourEnginesMaxSpeedDoubleStrike. */
        }
        EffectOp::CounterUnlessCollectsEvidence {
            ward_target,
            targeting_stack_item,
            minimum_mana_value,
        } => {
            // runtime-only: built by trigger construction from the card
            // definition's `ward_cost`; its meaning is the printed Ward fact.
            let _ = ward_target; // engine-internal stack target contract
            let _ = targeting_stack_item; // engine-internal stack incarnation id
            let _ = minimum_mana_value; // comes from the definition's ward cost
        }
        EffectOp::CounterUnlessPaysLife {
            ward_target,
            targeting_stack_item,
            life,
        } => {
            // runtime-only: as `CounterUnlessCollectsEvidence`.
            let _ = ward_target; // engine-internal stack target contract
            let _ = targeting_stack_item; // engine-internal stack incarnation id
            let _ = life; // comes from the definition's ward cost
        }
        EffectOp::CounterUnlessDiscardsCard {
            ward_target,
            targeting_stack_item,
        } => {
            // runtime-only: as `CounterUnlessCollectsEvidence`.
            let _ = ward_target; // engine-internal stack target contract
            let _ = targeting_stack_item; // engine-internal stack incarnation id
        }
        EffectOp::BindConvokedCreatureCountToLookTop { count, max_taken } => {
            // Authored trigger marker. Collection binds the number of
            // creatures that convoked the triggering spell as the mana-value
            // bound; at resolution the controller looks at the top `count`,
            // takes up to `max_taken` creature cards under that bound to
            // hand (revealed, chosen deterministically), then shuffles.
            // Vocabulary gap: the moved class carries no mana-value bound.
            let creature = ObjF::Typed(CardTypeF::Creature);
            let mut look = EffectAtom::new(EvF::Look)
                .player(RelF::You)
                .obj(ObjF::AnyCard)
                .amount(AmtF::fixed(i64::from(*count)));
            look.from = Some(ZoneF::Library);
            out.effect(look);
            out.effect(
                EffectAtom::moving(Some(ZoneF::Library), ZoneF::Hand)
                    .player(RelF::You)
                    .obj(creature)
                    .amount(AmtF::fixed(i64::from(*max_taken))),
            );
            let mut reveal = EffectAtom::new(EvF::Reveal)
                .player(RelF::You)
                .obj(creature)
                .amount(AmtF::fixed(i64::from(*max_taken)));
            reveal.from = Some(ZoneF::Library);
            out.effect(reveal);
            let mut shuffle = EffectAtom::new(EvF::Shuffle).player(RelF::You);
            shuffle.from = Some(ZoneF::Library);
            out.effect(shuffle);
        }
        EffectOp::LookTopTakeCreaturesManaValueAtMostThenShuffle {
            count,
            max_taken,
            max_mana_value,
        } => {
            // runtime-only: materialized from
            // `BindConvokedCreatureCountToLookTop`; its meaning is emitted
            // there.
            let _ = (count, max_taken, max_mana_value);
        }
        EffectOp::BindDamageOpponentEqualToSourceLastPower => {
            // Authored trigger marker. Collection binds the source's
            // last-known power; at resolution that much damage is dealt to
            // the opponent.
            out.effect(
                EffectAtom::new(EvF::Damage)
                    .player(RelF::Opponent)
                    .obj(ObjF::Player)
                    .amount(AmtF::Dynamic),
            );
        }
        EffectOp::PumpOtherAttackingCreaturesUntilEndOfTurn { power, toughness } => {
            // Battle cry: each other attacking creature (sampled at
            // resolution) gets the modifier until the next cleanup.
            // Vocabulary gap: ObjF has no attacking or "other" refinement.
            out.effect(
                EffectAtom::new(EvF::StatChange)
                    .player(RelF::You)
                    .obj(ObjF::Typed(CardTypeF::Creature))
                    .amount(AmtF::stat(i64::from(*power), i64::from(*toughness)))
                    .duration(DurF::EndOfTurn),
            );
        }
        EffectOp::RevealTopCardToHandLoseLifeEqualToManaValue => {
            // The controller reveals their top card, puts it into hand, and
            // loses life equal to its mana value.
            let mut reveal = EffectAtom::new(EvF::Reveal)
                .player(RelF::You)
                .obj(ObjF::AnyCard)
                .amount(AmtF::fixed(1));
            reveal.from = Some(ZoneF::Library);
            out.effect(reveal);
            out.effect(
                EffectAtom::moving(Some(ZoneF::Library), ZoneF::Hand)
                    .player(RelF::You)
                    .obj(ObjF::AnyCard)
                    .amount(AmtF::fixed(1)),
            );
            out.effect(
                EffectAtom::new(EvF::LifeLoss)
                    .player(RelF::You)
                    .obj(ObjF::Player)
                    .amount(AmtF::Dynamic),
            );
        }
        EffectOp::BindIncubateToTriggerSpell => {
            // Authored trigger marker. Collection binds the triggering
            // spell's mana value N; at resolution the controller creates an
            // Incubator token with N +1/+1 counters.
            out.effect(
                EffectAtom::new(EvF::CreateToken)
                    .player(RelF::You)
                    .obj(ObjF::Token)
                    .amount(AmtF::fixed(1)),
            );
            out.effect(
                EffectAtom::new(EvF::PlaceCounter)
                    .player(RelF::You)
                    .obj(ObjF::Token)
                    .amount(AmtF::Dynamic)
                    .duration(DurF::Permanent),
            );
            if let Some(incubator) = crate::card_def::card_id_by_name("Incubator Token") {
                out.created_tokens.push(incubator);
            }
        }
        EffectOp::Incubate { amount } => {
            // runtime-only: materialized from `BindIncubateToTriggerSpell`;
            // its meaning is emitted there.
            let _ = amount;
        }
        EffectOp::BindWarpExileToTriggerSource => {
            // Authored trigger marker. Collection binds the warped (or
            // unearthed) source incarnation; at resolution it is exiled if
            // still on the battlefield. A warped card may later be cast
            // from exile; that permission lives in the warp cast mode.
            out.control(ControlF::Conditional);
            exile_self(out);
        }
        EffectOp::WarpExileBoundObject { object } => {
            // runtime-only: materialized from
            // `BindWarpExileToTriggerSource`; its meaning is emitted there.
            let _ = object; // engine-internal exact-incarnation binding
        }
        EffectOp::BindPlusOneCounterOnAnotherTargetToTriggerTarget => {
            // Authored trigger marker. Collection binds the creature whose
            // targeting triggered the ability; at resolution target 0,
            // another creature the controller controls, gets a +1/+1
            // counter.
            let creature = ObjF::Typed(CardTypeF::Creature);
            out.effect(
                EffectAtom::new(EvF::PlaceCounter)
                    .player(RelF::You)
                    .obj(creature)
                    .amount(AmtF::fixed(1))
                    .duration(DurF::Permanent),
            );
            out.effect(
                EffectAtom::new(EvF::StatChange)
                    .player(RelF::You)
                    .obj(creature)
                    .amount(AmtF::fixed(1))
                    .duration(DurF::Permanent),
            );
        }
        EffectOp::PutPlusOnePlusOneCounterOnTargetOtherThan { other_than } => {
            // runtime-only: materialized from
            // `BindPlusOneCounterOnAnotherTargetToTriggerTarget`; its
            // meaning is emitted there.
            let _ = other_than; // engine-internal object id
        }
        EffectOp::ReturnAbilitySourceFromGraveyard { tapped } => {
            out.effect(
                EffectAtom::moving(Some(ZoneF::Graveyard), ZoneF::Battlefield)
                    .player(RelF::You)
                    .obj(ObjF::ThisObject),
            );
            if *tapped {
                out.effect(EffectAtom::new(EvF::Tap).obj(ObjF::ThisObject));
            }
        }
        EffectOp::ReturnSourceFromGraveyardUnearthed => {
            // Unearth: the source returns from the graveyard to the
            // battlefield, marked to be exiled at the next end step or if
            // it would leave.
            out.effect(
                EffectAtom::moving(Some(ZoneF::Graveyard), ZoneF::Battlefield)
                    .player(RelF::You)
                    .obj(ObjF::ThisObject),
            );
            out.effect(
                EffectAtom::new(EvF::Restrict)
                    .player(RelF::You)
                    .obj(ObjF::ThisObject)
                    .duration(DurF::WhileOnBattlefield),
            );
        }
        EffectOp::CounterTargetSpellThenCreateTokens {
            target_index,
            token_def,
            count,
        } => {
            let _ = target_index; // the target filter owns slot legality
            out.control(ControlF::Conditional);
            out.effect(
                EffectAtom::new(EvF::CounterSpell)
                    .player(RelF::ObjectOwner)
                    .obj(ObjF::Spell),
            );
            // Nominal counter departure; the executor preserves the shared
            // flashback exile and virtual-copy cease exceptions.
            out.effect(
                EffectAtom::moving(Some(ZoneF::Stack), ZoneF::Graveyard)
                    .player(RelF::ObjectOwner)
                    .obj(ObjF::Spell),
            );
            out.effect(
                EffectAtom::new(EvF::CreateToken)
                    .player(RelF::ObjectController)
                    .obj(ObjF::Token)
                    .amount(AmtF::fixed(i64::from(*count))),
            );
            out.created_tokens.push(*token_def);
        }
        EffectOp::ReturnAllGraveyardCreaturesUnderController => {
            let creature = ObjF::Typed(CardTypeF::Creature);
            out.read(
                RelF::EachPlayer,
                Some(ZoneF::Graveyard),
                Some(creature),
                AggF::Characteristic,
            );
            out.effect(
                EffectAtom::moving(Some(ZoneF::Graveyard), ZoneF::Battlefield)
                    .player(RelF::EachPlayer)
                    .obj(creature)
                    .amount(AmtF::All),
            );
            out.effect(
                EffectAtom::new(EvF::GainControl)
                    .player(RelF::You)
                    .obj(creature)
                    .amount(AmtF::All),
            );
            // Card/token filtering and simultaneous entry have no fixed predicate facets.
            out.atoms.push(Atom::Opaque);
        }
        EffectOp::ReturnOwnGraveyardCreaturesManaValueAtMost { max_mana_value } => {
            let creature = ObjF::Typed(CardTypeF::Creature);
            out.read(
                RelF::You,
                Some(ZoneF::Graveyard),
                Some(creature),
                AggF::Characteristic,
            );
            out.effect(
                EffectAtom::moving(Some(ZoneF::Graveyard), ZoneF::Battlefield)
                    .player(RelF::You)
                    .obj(creature)
                    .amount(AmtF::All),
            );
            // The fixed vocabulary lacks the mana-value bound and card/token distinction.
            let _ = max_mana_value;
            out.atoms.push(Atom::Opaque);
        }
        EffectOp::ReturnAttackingCreaturesToOwnersHands => {
            out.effect(
                EffectAtom::moving(Some(ZoneF::Battlefield), ZoneF::Hand)
                    .player(RelF::ObjectOwner)
                    .obj(ObjF::Typed(CardTypeF::Creature))
                    .amount(AmtF::All),
            );
            // Attacking is absent from the fixed object facet vocabulary.
            out.atoms.push(Atom::Opaque);
        }
        EffectOp::LoseOpponentsLifeXThenGainLifeLost => {
            out.effect(
                EffectAtom::new(EvF::LifeLoss)
                    .player(RelF::EachOpponent)
                    .obj(ObjF::Player)
                    .amount(AmtF::Dynamic),
            );
            out.effect(
                EffectAtom::new(EvF::LifeGain)
                    .player(RelF::You)
                    .obj(ObjF::Player)
                    .amount(AmtF::Dynamic),
            );
            // The exact program records X and the replaced life-loss dependency.
            // The fixed facet vocabulary has no term for that binding.
            out.atoms.push(Atom::Opaque);
        }
        EffectOp::ExileGraveyardTargetsDrainPerCreature { max_targets } => {
            // Each still-legal graveyard card target is exiled; per creature
            // card exiled, each opponent loses 1 life and the controller
            // gains 1.
            out.effect(
                EffectAtom::moving(Some(ZoneF::Graveyard), ZoneF::Exile)
                    .player(RelF::EachPlayer)
                    .obj(ObjF::AnyCard)
                    .amount(AmtF::fixed(i64::from(*max_targets))),
            );
            out.effect(
                EffectAtom::new(EvF::LifeLoss)
                    .player(RelF::EachOpponent)
                    .obj(ObjF::Player)
                    .amount(AmtF::Dynamic),
            );
            out.effect(
                EffectAtom::new(EvF::LifeGain)
                    .player(RelF::You)
                    .obj(ObjF::Player)
                    .amount(AmtF::Dynamic),
            );
        }
        EffectOp::RemoveTimeCounterFromSource => {
            // Impending: one time counter off the source, if it has one.
            out.effect(
                EffectAtom::new(EvF::RemoveCounter)
                    .player(RelF::You)
                    .obj(ObjF::ThisObject)
                    .amount(AmtF::fixed(1)),
            );
        }
        EffectOp::ReturnSourceAsEnduringEnchantment => {
            // Enduring: the source returns from the graveyard to the
            // battlefield under its owner's control as an enchantment that
            // is not a creature.
            out.effect(
                EffectAtom::moving(Some(ZoneF::Graveyard), ZoneF::Battlefield)
                    .player(RelF::ObjectOwner)
                    .obj(ObjF::ThisObject),
            );
            out.effect(
                EffectAtom::new(EvF::SetCharacteristic)
                    .player(RelF::ObjectOwner)
                    .obj(ObjF::ThisObject)
                    .duration(DurF::WhileOnBattlefield),
            );
        }
        _ => unreachable!("dispatched to the wrong slice"),
    }
}
