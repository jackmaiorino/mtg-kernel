//! Partial source for X damage followed by a batch of excess-damage tokens.
use crate::{
    card_def::{CardType, Keywords, TargetSpec, CARD_DEFS},
    effect::{EffectOp, ExecCtx},
    event::{self, CommittedEvent, ProposedEvent},
    state::{GameState, Target, Zone},
};

/// The existing fixed-damage owner cannot suspend this linked instruction.
/// Ordinary replacement processing is sufficient when its replacements commute.
pub(crate) fn validate_commuting_path(
    state: &GameState,
    ctx: &ExecCtx,
    op: &EffectOp,
) -> Result<(), String> {
    let EffectOp::DealXDamageThenCreateTokensForExcess { target_index, .. } = op else {
        return Err("excess damage route lost its printed operation".into());
    };
    if !ctx.target_incarnation_matches(usize::from(*target_index), state) {
        return Ok(()); // Ordinary all-targets-illegal resolution handles the fizzle.
    }
    let target = *ctx
        .targets
        .get(usize::from(*target_index))
        .ok_or("excess damage target absent")?;
    let ProposedEvent::Damage(damage) =
        ProposedEvent::damage(ctx.source, target, i32::from(ctx.x_value))
    else {
        unreachable!()
    };
    crate::twinflame_tyrant_v1::validate_commuting_damage(state, &damage)?;
    if crate::effect::crystal_prevention_v1::needs_choice(state, &damage) {
        return Err(
            "excess damage requires a successor for noncommuting prevention ordering".into(),
        );
    }
    Ok(())
}

pub(crate) fn resolve(state: &mut GameState, ctx: &ExecCtx, slot: u8, token_def: u16) {
    let index = usize::from(slot);
    if !ctx.target_incarnation_matches(index, state)
        || !crate::engine::effect_target_is_legal(
            state,
            ctx.source,
            ctx.controller,
            TargetSpec::Creature,
            &ctx.targets,
            index,
        )
    {
        return;
    }
    let Some(Target::Object(target)) = ctx.targets.get(index).copied() else {
        return;
    };
    let live = state.objects.get(target);
    if live.zone != Zone::Battlefield
        || !crate::engine::object_has_type(state, target, CardType::Creature)
    {
        return;
    }
    // Lethal damage uses the creature's effective toughness and previously
    // marked damage. Indestructible does not change how much damage is lethal.
    let remaining = (i64::from(crate::engine::effective_toughness(state, target))
        - i64::from(live.damage))
    .max(0);
    let lethal = if crate::engine::has_effective_keyword(state, ctx.source, Keywords::DEATHTOUCH) {
        remaining.min(1)
    } else {
        remaining
    };
    let start = state.engine.event_history.len();
    event::propose_and_commit(
        state,
        ProposedEvent::damage(ctx.source, Target::Object(target), i32::from(ctx.x_value)),
    );
    if state.engine.halted.is_some() {
        return;
    }
    // The replacement/prevention pipeline determines damage actually dealt.
    // No SBA occurs between the damage and this instruction's token creation.
    let dealt: i64 = state.engine.event_history[start..]
        .iter()
        .filter_map(|event| match event {
            CommittedEvent::Damage {
                source,
                target: Target::Object(recipient),
                amount,
            } if *source == ctx.source && *recipient == target => Some(i64::from((*amount).max(0))),
            _ => None,
        })
        .sum();
    let excess = dealt.saturating_sub(lethal).max(0);
    let Ok(excess) = i32::try_from(excess) else {
        state.engine.halted = Some((
            crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
            ctx.source,
        ));
        return;
    };
    if excess > 0 {
        let token = CARD_DEFS
            .get(usize::from(token_def))
            .expect("excess token definition exists");
        assert!(
            token.is_token && token.is_executable() && token.has_full_support(),
            "excess token creation requires a fully supported executable token"
        );
        // One simultaneous creation event, including with no replacement source.
        // The existing batch driver applies Season and captures entry observers.
        let events = (0..excess)
            .map(|_| ProposedEvent::create_token(token_def, ctx.controller))
            .collect();
        event::propose_and_commit_batch(state, events);
    }
}

#[cfg(all(
    test,
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
mod tests;
