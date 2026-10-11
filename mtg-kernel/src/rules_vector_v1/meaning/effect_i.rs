//! Meaning table, EffectOp slice I: the MageZero Standard lands batch's land
//! animation.
//!
//! Every arm was written against the variant's executor in `effect.rs`.

use super::*;

pub(super) fn effect_op(op: &EffectOp, env: &Env, out: &mut Collector) {
    let _ = env;
    match op {
        EffectOp::AnimateSource => {
            // The source's exact battlefield incarnation becomes its
            // definition's `animation` creature until end of turn (514.2):
            // it gains Creature (and Artifact), base power and toughness,
            // colors, subtypes and keywords. The extractor records those
            // characteristics with the definition's `animation` field.
            out.effect(
                EffectAtom::new(EvF::SetCharacteristic)
                    .player(RelF::You)
                    .obj(ObjF::ThisObject)
                    .duration(DurF::EndOfTurn),
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
        _ => unreachable!("dispatched to the wrong slice"),
    }
}
