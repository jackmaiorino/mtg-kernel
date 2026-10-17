//! Wide variable-cast semantics. Executable actions remain crate private.
use crate::engine::{self, Action, Decision};
use crate::policy_observation_variable_cast_v1::{variable_cast_context, VariableCastContextV1};
use crate::policy_surface_v5::{PolicyActionV5, PolicyDecisionV5, PolicySurfaceV5};
use crate::rl::{ActionSemanticV1, CardStableRefV1, PlayerSeatV1, RlContractError};
use crate::state::{GameState, Target};
use crate::surface_v2::{SurfaceAction, SurfaceDecision};
use serde::{Deserialize, Serialize};

pub const VARIABLE_CAST_ACTION_SCHEMA_V1: &str = "policy-action-variable-cast/v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ActionSemanticVariableCastV1 {
    Predecessor {
        action: ActionSemanticV1,
    },
    ChooseVariableCastKicker {
        actor: PlayerSeatV1,
        source: CardStableRefV1,
        pay: bool,
    },
    ChooseVariableCastTarget {
        actor: PlayerSeatV1,
        source: CardStableRefV1,
        kicked: bool,
        selected_count: u64,
        target: CardStableRefV1,
    },
    FinishVariableCastTargets {
        actor: PlayerSeatV1,
        source: CardStableRefV1,
        selected_count: u64,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LegalActionVariableCastV1 {
    pub schema: String,
    pub ordinal: u64,
    pub stable_id: String,
    pub semantic: ActionSemanticVariableCastV1,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct CandidateVariableCastV1 {
    pub record: LegalActionVariableCastV1,
    pub action: PolicyActionV5,
}

pub(crate) fn candidates(
    state: &GameState,
    surface: &PolicySurfaceV5,
    decision: &PolicyDecisionV5,
) -> Result<Vec<CandidateVariableCastV1>, RlContractError> {
    candidates_hunter_scoped_v45(state, surface, decision, false)
}
pub(crate) fn candidates_hunter_scoped_v45(
    state: &GameState,
    surface: &PolicySurfaceV5,
    decision: &PolicyDecisionV5,
    accept_hunter: bool,
) -> Result<Vec<CandidateVariableCastV1>, RlContractError> {
    candidates_panda_scoped_v47(state, surface, decision, accept_hunter, false)
}
pub(crate) fn candidates_panda_scoped_v47(
    state: &GameState,
    surface: &PolicySurfaceV5,
    decision: &PolicyDecisionV5,
    accept_hunter: bool,
    accept_panda: bool,
) -> Result<Vec<CandidateVariableCastV1>, RlContractError> {
    candidates_conqueror_scoped_v49(state, surface, decision, accept_hunter, accept_panda, false)
}

pub(crate) fn candidates_conqueror_scoped_v49(
    state: &GameState,
    surface: &PolicySurfaceV5,
    decision: &PolicyDecisionV5,
    accept_hunter: bool,
    accept_panda: bool,
    accept_conqueror: bool,
) -> Result<Vec<CandidateVariableCastV1>, RlContractError> {
    candidates_fake_scoped_v50(
        state,
        surface,
        decision,
        accept_hunter,
        accept_panda,
        accept_conqueror,
        false,
    )
}

pub(crate) fn candidates_fake_scoped_v50(
    state: &GameState,
    surface: &PolicySurfaceV5,
    decision: &PolicyDecisionV5,
    accept_hunter: bool,
    accept_panda: bool,
    accept_conqueror: bool,
    accept_fake: bool,
) -> Result<Vec<CandidateVariableCastV1>, RlContractError> {
    candidates_current_scope(
        state,
        surface,
        decision,
        (accept_hunter, accept_panda, accept_conqueror, accept_fake),
        false,
        false,
        false,
    )
}
pub(crate) fn candidates_next_five_v59(
    state: &GameState,
    surface: &PolicySurfaceV5,
    decision: &PolicyDecisionV5,
) -> Result<Vec<CandidateVariableCastV1>, RlContractError> {
    candidates_current_scope(
        state,
        surface,
        decision,
        (true, true, true, true),
        true,
        false,
        false,
    )
}
pub(crate) fn candidates_graveyard_return_v61(
    state: &GameState,
    surface: &PolicySurfaceV5,
    decision: &PolicyDecisionV5,
) -> Result<Vec<CandidateVariableCastV1>, RlContractError> {
    crate::graveyard_return_observation_v61::validate_state(state).map_err(RlContractError)?;
    candidates_current_scope(
        state,
        surface,
        decision,
        (true, true, true, true),
        true,
        true,
        false,
    )
}
pub(crate) fn candidates_call_v62(
    state: &GameState,
    surface: &PolicySurfaceV5,
    decision: &PolicyDecisionV5,
) -> Result<Vec<CandidateVariableCastV1>, RlContractError> {
    crate::call_observation_v62::validate_state(state).map_err(RlContractError)?;
    candidates_current_scope(
        state,
        surface,
        decision,
        (true, true, true, true),
        true,
        true,
        true,
    )
}
pub(crate) fn candidates_end_turn_v68(
    state: &GameState,
    surface: &PolicySurfaceV5,
    decision: &PolicyDecisionV5,
) -> Result<Vec<CandidateVariableCastV1>, RlContractError> {
    crate::end_turn_observation_v68::validate_state(state).map_err(RlContractError)?;
    candidates_current_scope_end_turn_v68(
        state,
        surface,
        decision,
        (true, true, true, true),
        true,
        true,
        true,
    )
}
pub(crate) fn candidates_genesis_wave_v69(
    state: &GameState,
    surface: &PolicySurfaceV5,
    decision: &PolicyDecisionV5,
) -> Result<Vec<CandidateVariableCastV1>, RlContractError> {
    crate::genesis_wave_observation_v69::validate_state(state).map_err(RlContractError)?;
    candidates_current_scope_genesis_wave_v69(
        state,
        surface,
        decision,
        (true, true, true, true),
        true,
        true,
        true,
    )
}
pub(crate) fn candidates_vivien_v70(
    state: &GameState,
    surface: &PolicySurfaceV5,
    decision: &PolicyDecisionV5,
) -> Result<Vec<CandidateVariableCastV1>, RlContractError> {
    crate::vivien_observation_v70::validate_state(state).map_err(RlContractError)?;
    candidates_current_scope_vivien_v70(
        state,
        surface,
        decision,
        (true, true, true, true),
        true,
        true,
        true,
    )
}
pub(crate) fn candidates_muldrotha_v71(
    state: &GameState,
    surface: &PolicySurfaceV5,
    decision: &PolicyDecisionV5,
) -> Result<Vec<CandidateVariableCastV1>, RlContractError> {
    crate::muldrotha_observation_v71::validate_state(state).map_err(RlContractError)?;
    candidates_current_scope_muldrotha_v71(
        state,
        surface,
        decision,
        (true, true, true, true),
        true,
        true,
        true,
    )
}
pub(crate) fn candidates_tinybones_v72(
    state: &GameState,
    surface: &PolicySurfaceV5,
    decision: &PolicyDecisionV5,
) -> Result<Vec<CandidateVariableCastV1>, RlContractError> {
    crate::tinybones_observation_v72::validate_state(state).map_err(RlContractError)?;
    candidates_current_scope_tinybones_v72(
        state,
        surface,
        decision,
        (true, true, true, true),
        true,
        true,
        true,
    )
}
pub(crate) fn candidates_liliana_v73(
    state: &GameState,
    surface: &PolicySurfaceV5,
    decision: &PolicyDecisionV5,
) -> Result<Vec<CandidateVariableCastV1>, RlContractError> {
    crate::liliana_observation_v73::validate_state(state).map_err(RlContractError)?;
    candidates_current_scope_liliana_v73(
        state,
        surface,
        decision,
        (true, true, true, true),
        true,
        true,
        true,
    )
}

fn candidates_current_scope(
    state: &GameState,
    surface: &PolicySurfaceV5,
    decision: &PolicyDecisionV5,
    capabilities: (bool, bool, bool, bool),
    accept_next_five: bool,
    accept_graveyard_return: bool,
    accept_call: bool,
) -> Result<Vec<CandidateVariableCastV1>, RlContractError> {
    candidates_current_scope_inner_v68(
        state,
        surface,
        decision,
        capabilities,
        accept_next_five,
        accept_graveyard_return,
        accept_call,
        None,
    )
}
fn candidates_current_scope_end_turn_v68(
    state: &GameState,
    surface: &PolicySurfaceV5,
    decision: &PolicyDecisionV5,
    capabilities: (bool, bool, bool, bool),
    accept_next_five: bool,
    accept_graveyard_return: bool,
    accept_call: bool,
) -> Result<Vec<CandidateVariableCastV1>, RlContractError> {
    candidates_current_scope_inner_v68(
        state,
        surface,
        decision,
        capabilities,
        accept_next_five,
        accept_graveyard_return,
        accept_call,
        Some(crate::rl::CommonProjectionScopeV42::EndTurnV68),
    )
}
fn candidates_current_scope_genesis_wave_v69(
    state: &GameState,
    surface: &PolicySurfaceV5,
    decision: &PolicyDecisionV5,
    capabilities: (bool, bool, bool, bool),
    accept_next_five: bool,
    accept_graveyard_return: bool,
    accept_call: bool,
) -> Result<Vec<CandidateVariableCastV1>, RlContractError> {
    candidates_current_scope_inner_v68(
        state,
        surface,
        decision,
        capabilities,
        accept_next_five,
        accept_graveyard_return,
        accept_call,
        Some(crate::rl::CommonProjectionScopeV42::GenesisWaveV69),
    )
}
fn candidates_current_scope_vivien_v70(
    state: &GameState,
    surface: &PolicySurfaceV5,
    decision: &PolicyDecisionV5,
    capabilities: (bool, bool, bool, bool),
    accept_next_five: bool,
    accept_graveyard_return: bool,
    accept_call: bool,
) -> Result<Vec<CandidateVariableCastV1>, RlContractError> {
    candidates_current_scope_inner_v68(
        state,
        surface,
        decision,
        capabilities,
        accept_next_five,
        accept_graveyard_return,
        accept_call,
        Some(crate::rl::CommonProjectionScopeV42::VivienV70),
    )
}
fn candidates_current_scope_muldrotha_v71(
    state: &GameState,
    surface: &PolicySurfaceV5,
    decision: &PolicyDecisionV5,
    capabilities: (bool, bool, bool, bool),
    accept_next_five: bool,
    accept_graveyard_return: bool,
    accept_call: bool,
) -> Result<Vec<CandidateVariableCastV1>, RlContractError> {
    candidates_current_scope_inner_v68(
        state,
        surface,
        decision,
        capabilities,
        accept_next_five,
        accept_graveyard_return,
        accept_call,
        Some(crate::rl::CommonProjectionScopeV42::MuldrothaV71),
    )
}
fn candidates_current_scope_tinybones_v72(
    state: &GameState,
    surface: &PolicySurfaceV5,
    decision: &PolicyDecisionV5,
    capabilities: (bool, bool, bool, bool),
    accept_next_five: bool,
    accept_graveyard_return: bool,
    accept_call: bool,
) -> Result<Vec<CandidateVariableCastV1>, RlContractError> {
    candidates_current_scope_inner_v68(
        state,
        surface,
        decision,
        capabilities,
        accept_next_five,
        accept_graveyard_return,
        accept_call,
        Some(crate::rl::CommonProjectionScopeV42::TinybonesV72),
    )
}
fn candidates_current_scope_liliana_v73(
    state: &GameState,
    surface: &PolicySurfaceV5,
    decision: &PolicyDecisionV5,
    capabilities: (bool, bool, bool, bool),
    accept_next_five: bool,
    accept_graveyard_return: bool,
    accept_call: bool,
) -> Result<Vec<CandidateVariableCastV1>, RlContractError> {
    candidates_current_scope_inner_v68(
        state,
        surface,
        decision,
        capabilities,
        accept_next_five,
        accept_graveyard_return,
        accept_call,
        Some(crate::rl::CommonProjectionScopeV42::LilianaV73),
    )
}

#[allow(clippy::too_many_arguments)]
fn candidates_current_scope_inner_v68(
    state: &GameState,
    surface: &PolicySurfaceV5,
    decision: &PolicyDecisionV5,
    capabilities: (bool, bool, bool, bool),
    accept_next_five: bool,
    accept_graveyard_return: bool,
    accept_call: bool,
    selected_scope: Option<crate::rl::CommonProjectionScopeV42>,
) -> Result<Vec<CandidateVariableCastV1>, RlContractError> {
    let (accept_hunter, accept_panda, accept_conqueror, accept_fake) = capabilities;

    if accept_fake {
        crate::fake_your_own_death_v50::public_v50::capture(state)?;
    } else {
        crate::fake_your_own_death_v50::public_v50::require_legacy_visible(
            state,
            decision.actor(state),
        )?;
    }

    if accept_conqueror {
        crate::bloodthirsty_conqueror_v49::validate(state).map_err(RlContractError)?;
    } else {
        crate::bloodthirsty_conqueror_v49::require_legacy_consumer(state)
            .map_err(RlContractError)?;
    }

    if accept_panda {
        crate::fiendish_panda_v1::custody::validate(state).map_err(RlContractError)?;
    }
    if accept_hunter {
        crate::high_society_hunter_custody_v45::validate(state).map_err(RlContractError)?;
    }
    let mut check_state = state.clone();
    let mut check_surface = surface.clone();
    let actual = check_surface
        .next_decision(&mut check_state)
        .map_err(RlContractError)?;
    if actual != *decision || check_state != *state {
        return Err(RlContractError(
            "candidate origin is not the current authenticated surface".into(),
        ));
    }
    let context = variable_cast_context(state, decision)?;
    let mut raw: Vec<(ActionSemanticVariableCastV1, PolicyActionV5)> = Vec::new();
    match context {
        Some(VariableCastContextV1::Kicker {
            source,
            controller,
            legal_pay_choices,
        }) => {
            for pay in legal_pay_choices {
                raw.push((
                    ActionSemanticVariableCastV1::ChooseVariableCastKicker {
                        actor: controller,
                        source: source.clone(),
                        pay,
                    },
                    PolicyActionV5::Surface(SurfaceAction::Action(Action::ChooseKicker(pay))),
                ));
            }
        }
        Some(VariableCastContextV1::Targets {
            source,
            controller,
            kicked,
            selected_count,
            legal_choices,
            can_finish,
            ..
        }) => {
            for target in legal_choices {
                let id = crate::ids::ObjectId(target.arena_id);
                raw.push((
                    ActionSemanticVariableCastV1::ChooseVariableCastTarget {
                        actor: controller,
                        source: source.clone(),
                        kicked,
                        selected_count,
                        target,
                    },
                    PolicyActionV5::Surface(SurfaceAction::Action(Action::ChooseTarget(
                        Target::Object(id),
                    ))),
                ));
            }
            if can_finish {
                raw.push((
                    ActionSemanticVariableCastV1::FinishVariableCastTargets {
                        actor: controller,
                        source,
                        selected_count,
                    },
                    PolicyActionV5::Surface(SurfaceAction::Action(Action::FinishEffectSelection)),
                ));
            }
        }
        None => {
            if let PolicyDecisionV5::Surface(SurfaceDecision::Decision(
                Decision::CastSpellOrPass {
                    player,
                    castable_spells,
                    mana_abilities,
                    land_drops,
                    activatable_abilities,
                    plot_actions,
                },
            )) = decision
            {
                // Generate from the complete real priority offer. The legacy
                // generator's explicit Divine refusal remains untouched.
                let actor = (*player).into();
                for &id in castable_spells {
                    predecessor(
                        &mut raw,
                        ActionSemanticV1::CastSpell {
                            actor,
                            source: crate::rl::card_ref(state, id)?,
                        },
                        Action::CastSpell(id),
                    );
                }
                for &id in mana_abilities {
                    let choices = engine::available_mana_ability_choices(*player, id, state);
                    for &choice in &choices {
                        let cost_targets = engine::mana_ability_cost_targets(*player, id, state);
                        if cost_targets.is_empty() {
                            predecessor(
                                &mut raw,
                                ActionSemanticV1::ActivateManaAbility {
                                    actor,
                                    source: crate::rl::card_ref(state, id)?,
                                    mana_choice: Some(choice),
                                    cost_target: None,
                                },
                                if choices.len() == 1 {
                                    Action::ActivateManaAbility(id)
                                } else {
                                    Action::ActivateManaAbilityChoice(id, choice)
                                },
                            );
                        } else {
                            for &cost_target in &cost_targets {
                                predecessor(
                                    &mut raw,
                                    ActionSemanticV1::ActivateManaAbility {
                                        actor,
                                        source: crate::rl::card_ref(state, id)?,
                                        mana_choice: Some(choice),
                                        cost_target: Some(crate::rl::card_ref(state, cost_target)?),
                                    },
                                    Action::ActivateManaAbilityWithCostTarget(
                                        id,
                                        choice,
                                        cost_target,
                                    ),
                                );
                            }
                        }
                    }
                }
                for &id in land_drops {
                    predecessor(
                        &mut raw,
                        ActionSemanticV1::PlayLand {
                            actor,
                            source: crate::rl::card_ref(state, id)?,
                        },
                        Action::PlayLand(id),
                    );
                }
                for &(id, ability_index) in activatable_abilities {
                    predecessor(
                        &mut raw,
                        if ability_index == 255 {
                            ActionSemanticV1::TurnFaceUp {
                                actor,
                                source: crate::rl::card_ref(state, id)?,
                            }
                        } else {
                            ActionSemanticV1::ActivateAbility {
                                actor,
                                source: crate::rl::card_ref(state, id)?,
                                ability_index,
                                granted_ability: crate::rl::GrantedAbilityPublicV1(
                                    crate::rl::public_granted_ability(
                                        crate::standard_cards_v1::capture_cauldron_grant(
                                            state,
                                            id,
                                            ability_index,
                                        ),
                                    ),
                                ),
                            }
                        },
                        Action::ActivateAbility(id, ability_index),
                    );
                }
                for &id in plot_actions {
                    predecessor(
                        &mut raw,
                        ActionSemanticV1::PlotSpell {
                            actor,
                            source: crate::rl::card_ref(state, id)?,
                        },
                        Action::PlotSpell(id),
                    );
                }
                predecessor(&mut raw, ActionSemanticV1::Pass { actor }, Action::Pass);
            } else {
                let old = if selected_scope == Some(crate::rl::CommonProjectionScopeV42::LilianaV73)
                {
                    crate::rl::core_policy_action_candidates_liliana_v73(decision, state)?
                } else if selected_scope == Some(crate::rl::CommonProjectionScopeV42::TinybonesV72)
                {
                    crate::rl::core_policy_action_candidates_tinybones_v72(decision, state)?
                } else if selected_scope == Some(crate::rl::CommonProjectionScopeV42::MuldrothaV71)
                {
                    crate::rl::core_policy_action_candidates_muldrotha_v71(decision, state)?
                } else if selected_scope == Some(crate::rl::CommonProjectionScopeV42::VivienV70) {
                    crate::rl::core_policy_action_candidates_vivien_v70(decision, state)?
                } else if selected_scope
                    == Some(crate::rl::CommonProjectionScopeV42::GenesisWaveV69)
                {
                    crate::rl::core_policy_action_candidates_genesis_wave_v69(decision, state)?
                } else if selected_scope == Some(crate::rl::CommonProjectionScopeV42::EndTurnV68) {
                    crate::rl::core_policy_action_candidates_end_turn_v68(decision, state)?
                } else if accept_call {
                    crate::rl::core_policy_action_candidates_call_v62(decision, state)?
                } else if accept_graveyard_return {
                    crate::rl::core_policy_action_candidates_graveyard_return_v61(decision, state)?
                } else if accept_next_five {
                    crate::rl::core_policy_action_candidates_next_five_v59(decision, state)?
                } else {
                    crate::rl::core_policy_action_candidates_fake_scoped_v50(
                        decision,
                        state,
                        accept_hunter,
                        accept_panda,
                        accept_conqueror,
                        accept_fake,
                    )?
                };
                let feasible = if matches!(
                    decision,
                    PolicyDecisionV5::AttackerInclusion { .. }
                        | PolicyDecisionV5::BlockerInclusion { .. }
                ) {
                    Some(
                        surface
                            .feasible_scan_answers(state, decision)
                            .map_err(RlContractError)?,
                    )
                } else {
                    None
                };
                for (i, candidate) in old.into_iter().enumerate() {
                    if feasible.is_some_and(|answers| !answers[i]) {
                        continue;
                    }
                    raw.push((
                        ActionSemanticVariableCastV1::Predecessor {
                            action: candidate.semantic,
                        },
                        candidate.policy_action,
                    ));
                }
            }
        }
    }
    let mut out: Vec<CandidateVariableCastV1> = Vec::with_capacity(raw.len());
    for (index, (semantic, action)) in raw.into_iter().enumerate() {
        if out.iter().any(|prior| prior.record.semantic == semantic) {
            return Err(RlContractError(
                "duplicate variable-cast action semantic".into(),
            ));
        }
        let ordinal = u64::try_from(index)
            .map_err(|_| RlContractError("variable action ordinal exceeds u64".into()))?;
        let stable_id = format!(
            "variable-cast-v1/{:016x}",
            crate::rl::stable_hash_json(&serde_json::to_value(&semantic)?)?
        );
        out.push(CandidateVariableCastV1 {
            record: LegalActionVariableCastV1 {
                schema: VARIABLE_CAST_ACTION_SCHEMA_V1.into(),
                ordinal,
                stable_id,
                semantic,
            },
            action,
        });
    }
    Ok(out)
}

fn predecessor(
    out: &mut Vec<(ActionSemanticVariableCastV1, PolicyActionV5)>,
    semantic: ActionSemanticV1,
    action: Action,
) {
    out.push((
        ActionSemanticVariableCastV1::Predecessor { action: semantic },
        PolicyActionV5::Surface(SurfaceAction::Action(action)),
    ));
}
