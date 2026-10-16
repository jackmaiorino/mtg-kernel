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
    candidates_hunter_scoped_v45(origin, state, surface, false)
}
pub(crate) fn candidates_hunter_scoped_v45(
    origin: &PolicyDecisionV5,
    state: &GameState,
    surface: &PolicySurfaceV5,
    accept_hunter: bool,
) -> Result<Option<Vec<CorePolicyActionCandidateV1>>, RlContractError> {
    candidates_panda_scoped_v47(origin, state, surface, accept_hunter, false)
}
pub(crate) fn candidates_panda_scoped_v47(
    origin: &PolicyDecisionV5,
    state: &GameState,
    surface: &PolicySurfaceV5,
    accept_hunter: bool,
    accept_panda: bool,
) -> Result<Option<Vec<CorePolicyActionCandidateV1>>, RlContractError> {
    candidates_conqueror_scoped_v49(origin, state, surface, accept_hunter, accept_panda, false)
}

pub(crate) fn candidates_conqueror_scoped_v49(
    origin: &PolicyDecisionV5,
    state: &GameState,
    surface: &PolicySurfaceV5,
    accept_hunter: bool,
    accept_panda: bool,
    accept_conqueror: bool,
) -> Result<Option<Vec<CorePolicyActionCandidateV1>>, RlContractError> {
    candidates_fake_scoped_v50(
        origin,
        state,
        surface,
        accept_hunter,
        accept_panda,
        accept_conqueror,
        false,
    )
}

pub(crate) fn candidates_fake_scoped_v50(
    origin: &PolicyDecisionV5,
    state: &GameState,
    surface: &PolicySurfaceV5,
    accept_hunter: bool,
    accept_panda: bool,
    accept_conqueror: bool,
    accept_fake: bool,
) -> Result<Option<Vec<CorePolicyActionCandidateV1>>, RlContractError> {
    candidates_current_scope(
        origin,
        state,
        surface,
        (accept_hunter, accept_panda, accept_conqueror, accept_fake),
        false,
        false,
    )
}
pub(crate) fn candidates_next_five_v59(
    origin: &PolicyDecisionV5,
    state: &GameState,
    surface: &PolicySurfaceV5,
) -> Result<Option<Vec<CorePolicyActionCandidateV1>>, RlContractError> {
    candidates_current_scope(
        origin,
        state,
        surface,
        (true, true, true, true),
        true,
        false,
    )
}
pub(crate) fn candidates_graveyard_return_v61(
    origin: &PolicyDecisionV5,
    state: &GameState,
    surface: &PolicySurfaceV5,
) -> Result<Option<Vec<CorePolicyActionCandidateV1>>, RlContractError> {
    crate::graveyard_return_observation_v61::validate_state(state).map_err(RlContractError)?;
    candidates_current_scope(origin, state, surface, (true, true, true, true), true, true)
}
fn candidates_current_scope(
    origin: &PolicyDecisionV5,
    state: &GameState,
    surface: &PolicySurfaceV5,
    capabilities: (bool, bool, bool, bool),
    accept_next_five: bool,
    accept_graveyard_return: bool,
) -> Result<Option<Vec<CorePolicyActionCandidateV1>>, RlContractError> {
    let (accept_hunter, accept_panda, accept_conqueror, accept_fake) = capabilities;

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
    let candidates = if accept_graveyard_return {
        crate::rl_variable_cast_v1::candidates_graveyard_return_v61(state, surface, origin)?
    } else if accept_next_five {
        crate::rl_variable_cast_v1::candidates_next_five_v59(state, surface, origin)?
    } else {
        crate::rl_variable_cast_v1::candidates_fake_scoped_v50(
            state,
            surface,
            origin,
            accept_hunter,
            accept_panda,
            accept_conqueror,
            accept_fake,
        )?
    };
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
                    target: crate::divine_public_v44::target_ref_v44(
                        state,
                        crate::state::StackTargetContractV4::capture(
                            state,
                            crate::state::Target::Object(crate::ids::ObjectId(target.arena_id)),
                        ),
                    )?,
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
