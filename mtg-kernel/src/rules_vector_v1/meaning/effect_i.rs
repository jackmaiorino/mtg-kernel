//! Meaning table, EffectOp slice I: the MageZero Standard lands batch's land
//! animation.
//!
//! Every arm was written against the variant's executor in `effect.rs`.

use super::*;

pub(super) fn effect_op(op: &EffectOp, env: &Env, out: &mut Collector) {
    let _ = env;
    match op {
        EffectOp::FieryAnnihilation => {
            out.effect(
                EffectAtom::new(EvF::Damage)
                    .obj(env.target_obj(0))
                    .amount(AmtF::Fixed(5)),
            );
            out.effect(
                EffectAtom::moving(Some(ZoneF::Battlefield), ZoneF::Exile).obj(env.target_obj(1)),
            );
            out.effect(
                EffectAtom::moving(Some(ZoneF::Battlefield), ZoneF::Exile)
                    .obj(env.target_obj(0))
                    .duration(DurF::EndOfTurn),
            );
            // No frozen facet correlates independently legal optional attached
            // Equipment with the exact creature's death replacement authority.
            out.atoms.push(Atom::Opaque);
        }
        EffectOp::StandardLegendV1(op) => {
            use crate::standard_legends_v1::LegendEffectV1;
            match op {
                LegendEffectV1::GwennaMana(_, _) => {
                    out.effect(
                        EffectAtom::new(EvF::AddMana)
                            .player(RelF::You)
                            .amount(AmtF::fixed(2)),
                    );
                }
                LegendEffectV1::GwennaCounterAndUntap => {
                    out.effect(EffectAtom::new(EvF::PlaceCounter).obj(ObjF::ThisObject));
                    out.effect(EffectAtom::new(EvF::Untap).obj(ObjF::ThisObject));
                }
                LegendEffectV1::ShannaPayAndDraw => {
                    out.control(ControlF::ChooseObjects);
                    out.effect(
                        EffectAtom::moving(Some(ZoneF::Library), ZoneF::Hand)
                            .player(RelF::You)
                            .obj(ObjF::AnyCard),
                    );
                }
                LegendEffectV1::ProtectTargetFromDeath => out.effect(
                    EffectAtom::moving(Some(ZoneF::Graveyard), ZoneF::Battlefield)
                        .obj(ObjF::Permanent)
                        .duration(DurF::EndOfTurn),
                ),
                LegendEffectV1::ReturnBoundPermanent(_) => out.effect(
                    EffectAtom::moving(Some(ZoneF::Graveyard), ZoneF::Battlefield)
                        .obj(ObjF::Permanent),
                ),
                LegendEffectV1::LagrellaExileTargets => out.effect(
                    EffectAtom::moving(Some(ZoneF::Battlefield), ZoneF::Exile)
                        .obj(ObjF::Typed(CardTypeF::Creature)),
                ),
                LegendEffectV1::CountersOnReturnedPermanent(_) => out.effect(
                    EffectAtom::new(EvF::PlaceCounter)
                        .obj(ObjF::Permanent)
                        .amount(AmtF::fixed(2)),
                ),
                LegendEffectV1::SkrelvGrant(_) => out.effect(
                    EffectAtom::new(EvF::GrantKeyword)
                        .obj(ObjF::Typed(CardTypeF::Creature))
                        .duration(DurF::EndOfTurn),
                ),
                LegendEffectV1::BindJodahCast => {
                    out.effect(
                        EffectAtom::moving(Some(ZoneF::Library), ZoneF::Exile).obj(ObjF::AnyCard),
                    );
                    out.effect(
                        EffectAtom::moving(Some(ZoneF::Exile), ZoneF::Stack).obj(ObjF::AnyCard),
                    );
                    out.effect(
                        EffectAtom::moving(Some(ZoneF::Exile), ZoneF::Library).obj(ObjF::AnyCard),
                    );
                }
                LegendEffectV1::JodahCastSnapshot(_) => {}
                LegendEffectV1::CounterStackTargetAndDraw => {
                    out.effect(
                        EffectAtom::moving(Some(ZoneF::Stack), ZoneF::Graveyard).obj(ObjF::AnyCard),
                    );
                    out.effect(
                        EffectAtom::moving(Some(ZoneF::Library), ZoneF::Hand)
                            .player(RelF::ObjectController)
                            .obj(ObjF::AnyCard),
                    );
                }
                LegendEffectV1::DestroyTargetAndDraw => {
                    out.effect(
                        EffectAtom::moving(Some(ZoneF::Battlefield), ZoneF::Graveyard)
                            .obj(ObjF::Permanent),
                    );
                    out.effect(
                        EffectAtom::moving(Some(ZoneF::Library), ZoneF::Hand)
                            .player(RelF::ObjectController)
                            .obj(ObjF::AnyCard),
                    );
                }
                LegendEffectV1::CountersOnControlledCreatures
                | LegendEffectV1::SourcePowerCountersAndHaste => out.effect(
                    EffectAtom::new(EvF::PlaceCounter).obj(ObjF::Typed(CardTypeF::Creature)),
                ),
                LegendEffectV1::ProtectControlledLegendaryCreatures => out.effect(
                    EffectAtom::new(EvF::SetCharacteristic)
                        .player(RelF::You)
                        .obj(ObjF::Typed(CardTypeF::Creature))
                        .duration(DurF::EndOfTurn),
                ),
            }
        }
        EffectOp::AnimateSource | EffectOp::AnimateSourcePermanentlyV1 => {
            // The source's exact battlefield incarnation becomes its
            // definition's `animation` creature until end of turn (514.2):
            // it gains Creature (and Artifact), base power and toughness,
            // colors, subtypes and keywords. The extractor records those
            // characteristics with the definition's `animation` field.
            out.effect(
                EffectAtom::new(EvF::SetCharacteristic)
                    .player(RelF::You)
                    .obj(ObjF::ThisObject)
                    .duration(if matches!(op, EffectOp::AnimateSourcePermanentlyV1) {
                        DurF::Permanent
                    } else {
                        DurF::EndOfTurn
                    }),
            );
        }
        EffectOp::SetTargetBasePowerToughnessUntilEndOfTurn { index, .. } => {
            out.effect(
                EffectAtom::new(EvF::SetCharacteristic)
                    .obj(env.target_obj(*index))
                    .duration(DurF::EndOfTurn),
            );
        }
        EffectOp::BoostOtherControlledCreaturesUntilEndOfTurn { power, .. } => {
            out.effect(
                EffectAtom::new(EvF::SetCharacteristic)
                    .player(RelF::You)
                    .obj(ObjF::Typed(CardTypeF::Creature))
                    .amount(AmtF::fixed(i64::from(*power)))
                    .duration(DurF::EndOfTurn),
            );
        }
        EffectOp::SelectObjectsV1 { rule } => {
            out.control(ControlF::ChooseObjects);
            match rule.action {
                crate::effect::ObjectSelectionActionV1::SacrificeThenDrawAndMakeCapturedSourceUnblockableV1 => {
                    // Frozen vocabulary does not express this cost-bound consequence.
                    out.atoms.push(Atom::Opaque);
                }
                crate::effect::ObjectSelectionActionV1::TapFiveThenTransformSource => {
                    out.control(ControlF::Conditional);
                    out.effect(
                        EffectAtom::new(EvF::Tap)
                            .player(player_ref(rule.player))
                            .obj(ObjF::Permanent)
                            .amount(AmtF::fixed(5)),
                    );
                    out.effect(EffectAtom::new(EvF::Transform).obj(ObjF::ThisObject));
                }
                crate::effect::ObjectSelectionActionV1::MoveTo(zone) => out.effect(
                    EffectAtom::moving(Some(rule.zone.into()), zone.into())
                        .player(player_ref(rule.player))
                        .obj(ObjF::AnyCard)
                        .amount(AmtF::fixed(i64::from(rule.max))),
                ),
                crate::effect::ObjectSelectionActionV1::CountersAndKeyword {
                    counters,
                    keyword,
                } => {
                    out.effect(
                        EffectAtom::new(EvF::PlaceCounter)
                            .obj(ObjF::Typed(CardTypeF::Creature))
                            .amount(AmtF::fixed(i64::from(counters))),
                    );
                    for keyword in keyword_bits(keyword) {
                        out.effect(
                            EffectAtom::new(EvF::GrantKeyword)
                                .obj(ObjF::Typed(CardTypeF::Creature))
                                .keyword(keyword)
                                .duration(DurF::EndOfTurn),
                        );
                    }
                }
            }
        }
        EffectOp::DestroyCreaturesPowerAtMostV1 { power: _ }
        | EffectOp::DestroyPermanentsSharingTargetNameV1 { index: _ } => {
            out.effect(
                EffectAtom::moving(Some(ZoneF::Battlefield), ZoneF::Graveyard)
                    .obj(ObjF::Permanent)
                    .amount(AmtF::All),
            );
        }
        EffectOp::CreateTokensWithHasteUntilEndOfTurnV1 {
            token_def: _,
            count,
        } => {
            out.effect(
                EffectAtom::new(EvF::CreateToken)
                    .obj(ObjF::Permanent)
                    .amount(AmtF::fixed(i64::from(*count))),
            );
        }
        EffectOp::PutTargetCreatureOnOwnersLibraryBottomThenControllerGainsToughness {
            target_index,
        } => {
            let obj = env.target_obj(*target_index);
            out.effect(
                EffectAtom::moving(Some(ZoneF::Battlefield), ZoneF::Library)
                    .player(RelF::ObjectOwner)
                    .obj(obj),
            );
            out.read(
                RelF::ObjectController,
                Some(ZoneF::Battlefield),
                Some(obj),
                AggF::Characteristic,
            );
            out.effect(
                EffectAtom::new(EvF::LifeGain)
                    .player(RelF::ObjectController)
                    .obj(ObjF::Player)
                    .amount(AmtF::Dynamic),
            );
            // No frozen facet expresses bottom position, toughness specifically,
            // or the correlation to the same captured controller/characteristic.
            out.atoms.push(Atom::Opaque);
        }
        EffectOp::ReturnTwoDifferentControllerCreaturesToOwnersHands => {
            for index in [0, 1] {
                out.effect(
                    EffectAtom::moving(Some(ZoneF::Battlefield), ZoneF::Hand)
                        .player(RelF::ObjectOwner)
                        .obj(env.target_obj(index)),
                );
            }
            // Frozen vocabulary lacks pairwise controller inequality and the
            // symmetric pre-move legality snapshot with partial target success.
            out.atoms.push(Atom::Opaque);
        }
        _ => unreachable!("dispatched to the wrong slice"),
    }
}
