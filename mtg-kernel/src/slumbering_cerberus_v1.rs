//! Prepared Slumbering Cerberus rules. The definition is not registered here.
use crate::effect::{EffectCond, EffectOp, ExecCtx};
use crate::ids::ObjectId;
use crate::state::{GameState, Zone};

pub(crate) fn program_v1() -> EffectOp {
    EffectOp::Conditional {
        cond: EffectCond::CreatureDiedThisTurn,
        then: Box::new(EffectOp::StandardV1(
            crate::standard_cards_v1::StandardOpV1::UntapCapturedSourceV1,
        )),
        else_: Box::new(EffectOp::Sequence(Vec::new())),
    }
}

/// A step restriction is distinct from a resolving effect's untap attempt.
/// Only the actual current printed definition with active abilities supplies it.
pub(crate) fn prevents_untap_step(state: &GameState, source: ObjectId) -> bool {
    #[cfg(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    ))]
    {
        state.objects.try_get(source).is_some_and(|object| {
            object.zone == Zone::Battlefield
                && crate::card_def::CARD_DEFS
                    .get(usize::from(object.card_def))
                    .is_some_and(|def| def.is_executable() && def.name == "Slumbering Cerberus")
                && crate::continuous_characteristics_v1::printed_abilities_active(state, source)
        })
    }
    #[cfg(any(
        not(feature = "limited-fdn-fixtures"),
        feature = "standard-magezero-fixtures"
    ))]
    {
        let _ = (state, source);
        false
    }
}

/// Production stack root validation supplies authority; this leaf independently
/// checks the captured source before mutation and does not follow a new incarnation.
/// Current controller, definition, or ability removal do not erase a captured trigger.
pub(crate) fn untap_captured_source(ctx: &ExecCtx, state: &mut GameState) {
    let Some(contract) = ctx.ability_source_contract else {
        state.engine.halted = Some((
            crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
            ctx.source,
        ));
        return;
    };
    if contract.source != ctx.source
        || contract.controller != ctx.controller
        || contract.zone != Zone::Battlefield
        || crate::engine::validate_historical_ability_source_contract(state, contract).is_err()
    {
        state.engine.halted = Some((
            crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
            ctx.source,
        ));
        return;
    }
    if state.objects.try_get(ctx.source).is_some_and(|live| {
        live.zone == Zone::Battlefield
            && live.owner == contract.owner
            && live.zone_change_count == contract.zone_change_count
    }) {
        crate::engine::attempt_untap(state, ctx.source);
    }
}

#[cfg(test)]
#[path = "slumbering_cerberus_v1_tests.rs"]
mod tests;
