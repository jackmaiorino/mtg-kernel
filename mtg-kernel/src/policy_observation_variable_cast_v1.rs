//! Explicit variable-cast transport. Frozen observation and model identities
//! retain their refusal. Exact references are backend transport authority.
use crate::engine::{self, Decision};
use crate::ids::PlayerId;
use crate::policy_observation_v9::PolicyObservationExtensionsV9;
use crate::policy_surface_v5::{PolicyDecisionV5, PolicySurfaceV5};
use crate::rl::{
    CardPrivateV1, CardStableRefV1, KnownLibraryCardV4, PlayerSeatV1,
    PublicObservationProjectionV5, RlContractError,
};
use crate::state::{GameState, StackTargetContractV4, Target};
use crate::surface_v2::SurfaceDecision;
use serde::{Deserialize, Serialize};

pub const VARIABLE_CAST_OBSERVATION_SCHEMA_V1: &str = "policy-observation-variable-cast/v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "stage", rename_all = "snake_case", deny_unknown_fields)]
pub enum VariableCastContextV1 {
    Kicker {
        source: CardStableRefV1,
        controller: PlayerSeatV1,
        legal_pay_choices: Vec<bool>,
    },
    Targets {
        source: CardStableRefV1,
        controller: PlayerSeatV1,
        kicked: bool,
        selected: Vec<CardStableRefV1>,
        selected_count: u64,
        minimum: u64,
        maximum: Option<u64>,
        legal_choices: Vec<CardStableRefV1>,
        can_finish: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObservationVariableCastV1 {
    pub schema: String,
    pub kernel_version: String,
    pub surface_version: u32,
    pub policy_surface_version: u32,
    pub card_db_hash: u64,
    pub acting_player: PlayerSeatV1,
    pub step_index: u64,
    pub physical_decision_id: u64,
    pub substep_index: u32,
    pub substep_count: u32,
    pub projection: PublicObservationProjectionV5,
    pub own_hand: Vec<CardPrivateV1>,
    pub known_library_cards: [Vec<KnownLibraryCardV4>; 2],
    pub known_hand_cards: [Vec<CardPrivateV1>; 2],
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub known_face_down_cards: Vec<crate::rl::FaceDownCardKnowledgeV1>,
    pub extensions: PolicyObservationExtensionsV9,
    pub variable_cast: Option<VariableCastContextV1>,
    pub visible_projection_hash: u64,
}

impl ObservationVariableCastV1 {
    /// Structural wire validation is separate from engine authority. The
    /// owning session additionally rederives the exact engine origin/menu.
    pub fn validate(&self) -> Result<(), RlContractError> {
        if self.schema != VARIABLE_CAST_OBSERVATION_SCHEMA_V1
            || self.substep_count == 0
            || self.substep_index >= self.substep_count
        {
            return Err(RlContractError(
                "variable observation header is invalid".into(),
            ));
        }
        if let Some(context) = &self.variable_cast {
            match context {
                VariableCastContextV1::Kicker {
                    controller,
                    legal_pay_choices,
                    ..
                } => {
                    if *controller != self.acting_player
                        || (legal_pay_choices.as_slice() != [true]
                            && legal_pay_choices.as_slice() != [false, true])
                    {
                        return Err(RlContractError(
                            "variable kicker wire choices are invalid".into(),
                        ));
                    }
                }
                VariableCastContextV1::Targets {
                    controller,
                    kicked,
                    selected,
                    selected_count,
                    minimum,
                    maximum,
                    legal_choices,
                    can_finish,
                    ..
                } => {
                    let count = usize::try_from(*selected_count).map_err(|_| {
                        RlContractError("variable selected count exceeds usize".into())
                    })?;
                    if *controller != self.acting_player
                        || count != selected.len()
                        || (*minimum, *maximum) != if *kicked { (0, None) } else { (1, Some(1)) }
                        || maximum.is_some_and(|max| *selected_count > max)
                        || *can_finish != (*kicked && *selected_count >= *minimum)
                    {
                        return Err(RlContractError(
                            "variable target wire cardinality is invalid".into(),
                        ));
                    }
                    for (i, target) in selected.iter().enumerate() {
                        if selected[..i]
                            .iter()
                            .any(|prior| prior.arena_id == target.arena_id)
                        {
                            return Err(RlContractError(
                                "variable selected wire references are not distinct".into(),
                            ));
                        }
                    }
                    for (i, target) in legal_choices.iter().enumerate() {
                        if target.controller != *controller
                            || legal_choices[..i]
                                .iter()
                                .any(|prior| prior.arena_id == target.arena_id)
                            || selected
                                .iter()
                                .any(|prior| prior.arena_id == target.arena_id)
                        {
                            return Err(RlContractError(
                                "variable legal wire references are invalid".into(),
                            ));
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

pub(crate) fn captured_target_ref(
    contract: StackTargetContractV4,
) -> Result<CardStableRefV1, RlContractError> {
    let StackTargetContractV4::Object {
        object,
        card_def,
        owner,
        controller,
        zone,
        zone_change_count,
        ..
    } = contract
    else {
        return Err(RlContractError(
            "variable cast captured a noncreature target".into(),
        ));
    };
    Ok(CardStableRefV1 {
        arena_id: object.0,
        card_db_id: card_def,
        owner: owner.into(),
        controller: controller.into(),
        zone,
        zone_change_count,
    })
}

pub(crate) fn variable_cast_context(
    state: &GameState,
    decision: &PolicyDecisionV5,
) -> Result<Option<VariableCastContextV1>, RlContractError> {
    validate_variable_cast_authority(state)?;
    let PolicyDecisionV5::Surface(SurfaceDecision::Decision(raw)) = decision else {
        return Ok(None);
    };
    let Some(pending) = &state.engine.pending_cast else {
        if matches!(
            raw,
            Decision::ChooseVariableCastKicker { .. } | Decision::ChooseVariableCastTargets { .. }
        ) {
            return Err(RlContractError(
                "variable cast lost its pending authority".into(),
            ));
        }
        return Ok(None);
    };
    if pending.target_spec != crate::card_def::TargetSpec::KickerControlledCreatures {
        return Ok(None);
    }
    engine::validate_pending_cast(state, pending).map_err(RlContractError)?;
    // Recreate the actual engine origin. A coherent forged menu/count cannot
    // authenticate itself merely by repeating the pending mutable fields.
    let mut rederived = state.clone();
    if engine::advance_until_decision(&mut rederived) != *raw || rederived != *state {
        return Err(RlContractError(
            "variable cast decision differs from current engine authority".into(),
        ));
    }
    let capture = pending.source_contract;
    let source = CardStableRefV1 {
        arena_id: capture.source.0,
        card_db_id: capture.card_def,
        owner: capture.owner.into(),
        controller: capture.controller.into(),
        zone: capture.zone,
        zone_change_count: capture.zone_change_count,
    };
    match raw {
        Decision::ChooseVariableCastKicker {
            player,
            spell,
            can_decline,
        } => {
            if pending.kicked.is_some() || *player != pending.controller || *spell != pending.spell
            {
                return Err(RlContractError(
                    "variable kicker stage disagrees with source".into(),
                ));
            }
            let mut legal_pay_choices = Vec::new();
            if *can_decline {
                legal_pay_choices.push(false);
            }
            legal_pay_choices.push(true);
            Ok(Some(VariableCastContextV1::Kicker {
                source,
                controller: pending.controller.into(),
                legal_pay_choices,
            }))
        }
        Decision::ChooseVariableCastTargets {
            player,
            spell,
            selected_count,
            legal_targets,
            can_finish,
        } => {
            if pending.kicked != Some(true)
                || *selected_count != pending.targets_chosen.len()
                || *player != pending.controller
                || *spell != pending.spell
            {
                return Err(RlContractError(
                    "variable target stage disagrees with source or count".into(),
                ));
            }
            targets_context(state, source, pending, legal_targets, *can_finish)
        }
        Decision::ChooseTargets {
            player,
            spell,
            legal_targets,
            can_finish,
            ..
        } => {
            if pending.kicked != Some(false)
                || *player != pending.controller
                || *spell != pending.spell
            {
                return Err(RlContractError(
                    "unkicked variable target stage disagrees with source".into(),
                ));
            }
            targets_context(state, source, pending, legal_targets, *can_finish)
        }
        _ => Ok(None),
    }
}

fn validate_variable_cast_authority(state: &GameState) -> Result<(), RlContractError> {
    if let Some(pending) = &state.engine.pending_cast {
        engine::validate_pending_cast(state, pending).map_err(RlContractError)?;
    }
    for item in state.stack.iter().chain(
        state
            .engine
            .pending_effect
            .as_ref()
            .map(|pending| &pending.resolving_item),
    ) {
        let definition_owned = |card_def: u16| {
            crate::card_def::CARD_DEFS
                .get(usize::from(card_def))
                .is_some_and(|definition| {
                    definition.target_spec == crate::card_def::TargetSpec::KickerControlledCreatures
                })
        };
        if item.v4.target_spec == Some(crate::card_def::TargetSpec::KickerControlledCreatures)
            || item
                .v4
                .source_contract
                .is_some_and(|source| definition_owned(source.card_def))
            || state
                .objects
                .try_get(item.source)
                .is_some_and(|source| definition_owned(source.card_def))
        {
            engine::validated_stack_item_target_spec(item, state).map_err(RlContractError)?;
        }
    }
    Ok(())
}

fn targets_context(
    state: &GameState,
    source: CardStableRefV1,
    pending: &engine::PendingCast,
    legal: &[Target],
    can_finish: bool,
) -> Result<Option<VariableCastContextV1>, RlContractError> {
    let selected = pending
        .target_contracts
        .iter()
        .copied()
        .map(captured_target_ref)
        .collect::<Result<Vec<_>, _>>()?;
    if selected.len() != pending.targets_chosen.len()
        || selected
            .iter()
            .enumerate()
            .any(|(i, item)| selected[..i].contains(item))
    {
        return Err(RlContractError(
            "variable selection contracts are not a distinct exact sequence".into(),
        ));
    }
    let legal_choices = legal
        .iter()
        .map(|target| captured_target_ref(StackTargetContractV4::capture(state, *target)))
        .collect::<Result<Vec<_>, _>>()?;
    let kicked = pending
        .kicked
        .ok_or_else(|| RlContractError("variable target stage has no kicker choice".into()))?;
    Ok(Some(VariableCastContextV1::Targets {
        source,
        controller: pending.controller.into(),
        kicked,
        selected_count: u64::try_from(selected.len())
            .map_err(|_| RlContractError("variable count exceeds u64".into()))?,
        selected,
        minimum: if kicked { 0 } else { 1 },
        maximum: if kicked { None } else { Some(1) },
        legal_choices,
        can_finish,
    }))
}

/// The decision must be the real current surface origin. Nonacting observers
/// receive their own authorized rosters and no actor-only cast menu context.
pub fn observe_policy_variable_cast_v1(
    state: &GameState,
    surface: &PolicySurfaceV5,
    observer: PlayerId,
    decision: &PolicyDecisionV5,
    step_index: u64,
    physical_decision_id: u64,
) -> Result<ObservationVariableCastV1, RlContractError> {
    let mut current_state = state.clone();
    let mut current_surface = surface.clone();
    let current = current_surface
        .next_decision(&mut current_state)
        .map_err(RlContractError)?;
    if current != *decision || current_state != *state {
        return Err(RlContractError(
            "variable observation origin is stale".into(),
        ));
    }
    let context = variable_cast_context(state, decision)?;
    let substep = decision.substep();
    let storage = crate::rl::variable_cast_observation_storage_v1(
        state,
        surface,
        observer,
        step_index,
        physical_decision_id,
        substep,
    )?;
    let parts = crate::policy_observation_v8::observation_storage_v8(state, observer, storage)?;
    let base = parts.base;
    let mut observation = ObservationVariableCastV1 {
        schema: VARIABLE_CAST_OBSERVATION_SCHEMA_V1.into(),
        kernel_version: base.kernel_version,
        surface_version: base.surface_version,
        policy_surface_version: base.policy_surface_version,
        card_db_hash: base.card_db_hash,
        acting_player: base.acting_player,
        step_index: base.step_index,
        physical_decision_id: base.physical_decision_id,
        substep_index: base.substep_index,
        substep_count: base.substep_count,
        projection: base.projection,
        own_hand: base.own_hand,
        known_library_cards: base.known_library_cards,
        known_hand_cards: base.known_hand_cards,
        known_face_down_cards: base.known_face_down_cards,
        extensions: PolicyObservationExtensionsV9 {
            predecessor: parts.extensions,
            attacked_this_turn: [
                state.player_attacked_this_turn_v1(PlayerId::P0),
                state.player_attacked_this_turn_v1(PlayerId::P1),
            ],
        },
        variable_cast: if decision.actor(state) == Some(observer) {
            context
        } else {
            None
        },
        visible_projection_hash: 0,
    };
    let mut value = serde_json::to_value(&observation)?;
    value
        .as_object_mut()
        .ok_or_else(|| RlContractError("variable observation is not an object".into()))?
        .remove("visible_projection_hash");
    observation.visible_projection_hash = crate::rl::stable_hash_json(&value)?;
    observation.validate()?;
    Ok(observation)
}
