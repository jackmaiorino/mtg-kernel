//! Exact finite Divine producer. It never substitutes a frozen target count.
use crate::policy_surface_v5::{PolicyDecisionV5, PolicySurfaceV5};
use crate::rl::{ActionSemanticV1 as A, CorePolicyActionCandidateV1, RlContractError};
use crate::rl_variable_cast_v1::ActionSemanticVariableCastV1 as Wide;
use crate::state::GameState;

pub(crate) fn candidates(
    origin: &PolicyDecisionV5,
    state: &GameState,
    surface: &PolicySurfaceV5,
) -> Result<Option<Vec<CorePolicyActionCandidateV1>>, RlContractError> {
    crate::divine_public_v44::validate_state(state).map_err(RlContractError)?;
    let context = crate::policy_observation_variable_cast_v1::variable_cast_context(state, origin)?;
    let priority_offer = matches!(origin,
        PolicyDecisionV5::Surface(crate::surface::SurfaceDecision::Decision(
            crate::engine::Decision::CastSpellOrPass { castable_spells, .. }
        )) if castable_spells.iter().any(|source|
            state.objects.try_get(*source).is_some_and(|object|
                crate::card_def::CARD_DEFS[usize::from(object.card_def)].target_spec
                    == crate::card_def::TargetSpec::KickerControlledCreatures)));
    if context.is_none() && !priority_offer {
        return Ok(None);
    }
    let candidates = crate::rl_variable_cast_v1::candidates(state, surface, origin)?;
    candidates
        .into_iter()
        .map(|candidate| {
            let semantic = match candidate.record.semantic {
                Wide::Predecessor { action } => action,
                Wide::ChooseVariableCastKicker { actor, source, pay } => {
                    A::ChooseDivineKickerV44 { actor, source, pay }
                }
                Wide::ChooseVariableCastTarget {
                    actor,
                    source,
                    kicked,
                    selected_count,
                    target,
                } => A::ChooseDivineTargetV44 {
                    actor,
                    source,
                    kicked,
                    selected_count,
                    target,
                },
                Wide::FinishVariableCastTargets {
                    actor,
                    source,
                    selected_count,
                } => A::FinishDivineTargetsV44 {
                    actor,
                    source,
                    selected_count,
                },
            };
            Ok(CorePolicyActionCandidateV1 {
                semantic,
                policy_action: candidate.action,
            })
        })
        .collect::<Result<Vec<_>, _>>()
        .map(Some)
}
