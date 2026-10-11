//! Meaning table, EffectOp slice I: the MageZero Standard lands batch's land
//! animation.
//!
//! Every arm was written against the variant's executor in `effect.rs`.

use super::*;

pub(super) fn effect_op(op: &EffectOp, env: &Env, out: &mut Collector) {
    let _ = env;
    match op {
        EffectOp::StandardLegendV1(op) => {
            use crate::standard_legends_v1::LegendEffectV1;
            match op {
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
                    .amount(AmtF::fixed(*power))
                    .duration(DurF::EndOfTurn),
            );
        }
        EffectOp::SelectObjectsV1 { rule } => {
            out.control(ControlF::ChooseObjects);
            match rule.action {
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
        _ => unreachable!("dispatched to the wrong slice"),
    }
}
