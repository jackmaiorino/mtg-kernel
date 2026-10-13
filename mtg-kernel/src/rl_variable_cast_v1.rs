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
                let old = crate::rl::core_policy_action_candidates_hunter_scoped_v45(
                    decision,
                    state,
                    accept_hunter,
                )?;
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
