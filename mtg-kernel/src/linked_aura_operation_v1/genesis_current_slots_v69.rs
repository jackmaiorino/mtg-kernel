//! Exact private derivation of a genuine Genesis color-entry continuation.
//! The factual parent world/journal is immutable. Current hidden slots are
//! execution input, never a caller-supplied permission or a public DTO field.
use super::*;
use crate::ids::PlayerId;
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct InputV69 {
    actor: PlayerId,
    seed: u64,
}
fn original_genesis(
    original: &GameState,
    operation: &OperationV1,
    actor: PlayerId,
) -> Result<(), String> {
    let p = original
        .engine
        .pending_effect
        .as_ref()
        .ok_or("Genesis slot derivation has no original continuation")?;
    if !crate::effect::genesis_wave_v1::selected(original, &p.resolving_item)
        || p.ctx.controller != actor
    {
        return Err("Genesis slot derivation changed the native printed source/controller".into());
    }
    crate::effect::genesis_wave_v1::validate(original, p)?;
    let issued = match (&p.choice, operation) {
        (
            Some(crate::effect::PendingEffectChoice::SelectTargets {
                purpose: crate::effect::EffectTargetSelectionPurpose::GenesisWaveCardsV1 { .. },
                ..
            }),
            OperationV1::Step(Action::ChooseEffectTarget(_) | Action::FinishEffectSelection),
        ) => true,
        (None, OperationV1::Advance) => {
            matches!(
                p.frames.as_slice(),
                [crate::effect::EffectFrame::GenesisWaveV1 { .. }]
            ) && p.answered_choice_guard.is_some()
        }
        _ => false,
    };
    if !issued {
        return Err("Genesis slot derivation lost the original issued entry operation".into());
    }
    Ok(())
}
pub(super) fn materialize(
    original: &GameState,
    operation: &OperationV1,
    input: &InputV69,
) -> Result<GameState, String> {
    original_genesis(original, operation, input.actor)?;
    // This is a disposable scene, not a restored Runner or entry capability.
    crate::rl_session::genesis_wave_v69::derive_entry_hidden_input_v69(
        original,
        input.actor,
        input.seed,
    )
}
pub(crate) fn sample_genesis_checkpoint_v69(
    state: &mut GameState,
    actor: PlayerId,
    seed: u64,
) -> Result<(), String> {
    validate(state)?;
    let pending = state
        .engine
        .linked_aura_operation_v1
        .pending_general_v36
        .as_ref()
        .ok_or("Genesis current-slot sampling requires the actual color-entry owner")?;
    if !matches!(pending.choice, GeneralChoiceV36::Color { .. }) {
        return Err("Genesis current-slot sampling cannot borrow a different entry family".into());
    }
    original_genesis(&pending.original, &pending.operation, actor)?;
    if !crate::rl_session::genesis_wave_v69::same_entry_hidden_pool_v69(
        &pending.original,
        state,
        actor,
    ) {
        return Err(
            "Genesis current hidden slots differ from the original unobserved entry pool".into(),
        );
    }
    let input = InputV69 { actor, seed };
    let (next, _) = replay_general_current_slots_v69(
        &pending.original,
        &pending.operation,
        &pending.answers,
        true,
        Some(&input),
    )?;
    if next.engine.event_history != state.engine.event_history
        || next.engine.event_log != state.engine.event_log
    {
        return Err(
            "Genesis slot derivation changed factual history before the real answer".into(),
        );
    }
    validate(&next)?;
    *state = next;
    Ok(())
}
