//! Bounded optional generic payment. Option N means pay N and draw N.
use super::*;
fn plan(state: &GameState, player: PlayerId, amount: u32) -> Option<crate::mana::PaymentPlan> {
    crate::mana::plan_spell_mana_total_v1(&[], amount, player, state, false, &[], 0)
}
fn maximum(state: &GameState, player: PlayerId) -> Result<u16, String> {
    #[cfg(feature = "standard-magezero-fixtures")]
    let gained = crate::standard_cards_v1::life_gained_this_turn(state, player);
    #[cfg(not(feature = "standard-magezero-fixtures"))]
    let gained = 0u32;
    let mut low = 0u32;
    let mut high = gained;
    while low < high {
        let mid = low + (high - low) / 2 + 1;
        if plan(state, player, mid).is_some() {
            low = mid
        } else {
            high = mid - 1
        }
    }
    if low >= u32::from(u16::MAX) {
        return Err("optional payment exceeds the option-index domain".into());
    }
    Ok(low as u16)
}
fn options(maximum: u16) -> Vec<EffectOp> {
    (0..=maximum)
        .map(|n| EffectOp::DrawCards {
            player: PlayerRef::Controller,
            count: u32::from(n),
        })
        .collect()
}
fn origin(state: &GameState, pending: &EffectContinuation, path: &[u16]) -> Result<(), String> {
    let root = validated_definition_owned_root_effect(state, pending)?;
    if !matches!(
        effect_op_at_structural_path(&root, path),
        Some(EffectOp::StandardLegendV1(
            crate::standard_legends_v1::LegendEffectV1::ShannaPayAndDraw
        ))
    ) {
        return Err("optional payment lost its definition-owned operation".into());
    }
    Ok(())
}
pub(super) fn begin(
    state: &GameState,
    pending: &mut EffectContinuation,
    path: Vec<u16>,
) -> Result<(), String> {
    let player = pending.ctx.controller;
    let maximum = maximum(state, player)?;
    pending.choice = Some(PendingEffectChoice::ChooseOption {
        player,
        path: path.clone(),
        options: options(maximum),
        purpose: EffectOptionChoicePurpose::PayGenericDrawV1 {
            maximum,
            canonical_path: path,
            expected_remaining_frames: pending.frames.clone(),
        },
    });
    Ok(())
}
#[allow(clippy::too_many_arguments)]
pub(super) fn validate_choice(
    state: &GameState,
    pending: &EffectContinuation,
    player: PlayerId,
    path: &[u16],
    offered: &[EffectOp],
    bound: u16,
    canonical: &[u16],
    remaining: &[EffectFrame],
) -> Result<(), String> {
    origin(state, pending, path)?;
    if player != pending.ctx.controller
        || path != canonical
        || pending.frames != remaining
        || bound != maximum(state, player)?
        || offered != options(bound)
    {
        return Err("optional payment choice changed".into());
    }
    Ok(())
}
pub(super) fn pay_and_draw(
    state: &mut GameState,
    pending: &EffectContinuation,
    amount: u16,
    path: &[u16],
    remaining: &[EffectFrame],
) -> Result<(), String> {
    origin(state, pending, path)?;
    if pending.frames != remaining || amount > maximum(state, pending.ctx.controller)? {
        return Err("optional payment answer changed".into());
    }
    let payment = plan(state, pending.ctx.controller, u32::from(amount))
        .ok_or("optional payment became unpayable")?;
    crate::engine::pay_plan(state, pending.ctx.controller, &payment);
    execute(
        &EffectOp::DrawCards {
            player: PlayerRef::Controller,
            count: u32::from(amount),
        },
        &pending.ctx,
        state,
    );
    Ok(())
}
