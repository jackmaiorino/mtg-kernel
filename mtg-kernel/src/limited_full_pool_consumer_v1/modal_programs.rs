use super::ProgramV1;
use crate::{
    card_def::TargetSpec,
    effect::{EffectCond, EffectOp, ObjectRef, PlayerRef},
};

fn conditional(cond: EffectCond, then: EffectOp) -> EffectOp {
    EffectOp::Conditional {
        cond,
        then: Box::new(then),
        else_: Box::new(EffectOp::Sequence(Vec::new())),
    }
}

fn token(name: &str) -> Option<EffectOp> {
    Some(EffectOp::CreateToken {
        token_def: crate::card_def::card_id_by_name(name)?,
        controller: PlayerRef::Controller,
    })
}

pub(crate) fn branches(root: ProgramV1) -> &'static [(TargetSpec, ProgramV1)] {
    use ProgramV1::*;
    match root {
        SylvanChooseMode => &[(TargetSpec::ControlledCreature, SylvanCounter), (TargetSpec::None, SylvanRaccoon)],
        WardensChooseMode => &[(TargetSpec::None, WardensGainLife), (TargetSpec::None, WardensDrawLoseLife)],
        _ => &[],
    }
}

pub(super) fn template(program: ProgramV1) -> Option<EffectOp> {
    use ProgramV1::*;
    let controller = PlayerRef::Controller;
    Some(match program {
        KioraDrawDiscard => EffectOp::Sequence(vec![
            EffectOp::DrawCards { player: controller, count: 2 },
            EffectOp::DiscardCards { player: controller, count: 2 },
        ]),
        KioraThreshold => conditional(
            EffectCond::ControllerGraveyardCardCountAtLeast(7),
            EffectOp::Choice {
                controller,
                options: vec![EffectOp::Sequence(Vec::new()), token("Scion of the Deep Token")?],
            },
        ),
        SylvanChooseMode | WardensChooseMode => EffectOp::Choice {
            controller,
            options: branches(program).iter()
                .map(|(_, branch)| template(*branch)).collect::<Option<_>>()?,
        },
        SylvanCounter => EffectOp::AddPlusOnePlusOneCounters {
            object: ObjectRef::Target(0), count: 1,
        },
        SylvanRaccoon => conditional(
            EffectCond::ControlsCreaturePowerAtLeast(4),
            token("Raccoon Token")?,
        ),
        WardensGainLife => conditional(
            EffectCond::CreatureDiedThisTurn,
            EffectOp::GainLife { player: controller, amount: 2 },
        ),
        WardensDrawLoseLife => conditional(
            EffectCond::CreatureDiedThisTurn,
            EffectOp::Sequence(vec![
                EffectOp::DrawCards { player: controller, count: 1 },
                EffectOp::LoseLife { player: controller, amount: 1 },
            ]),
        ),
        _ => return None,
    })
}
