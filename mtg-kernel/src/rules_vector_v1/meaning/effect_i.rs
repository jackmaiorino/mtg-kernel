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
        _ => unreachable!("dispatched to the wrong slice"),
    }
}
