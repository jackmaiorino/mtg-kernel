//! Explicit cache-free action authority for typed ObservationV8 / Flat V5.
//! Shared resolver layouts do not assert a predecessor feature identity.

use super::flat_action_v4::{
    flat_visible_action_object_extension_aware_v4, frozen_pending_trigger_semantic_v4,
};
use super::*;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FlatActionDecisionBindingV5(pub(crate) FlatActionDecisionBindingV2, pub(crate) [u8; 32]);

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FlatActionDecisionSliceV5 {
    pub binding: FlatActionDecisionBindingV5,
    pub active_action_count: u32,
    pub active_ref_count: u32,
    pub active_object_count: u16,
}

struct FlatResolvedActionObjectV5 {
    arena_id: u32,
    position: u32,
    reference: CardStableRefV1,
    position_sensitive: bool,
    object: FlatActionObjectV2,
}

pub(super) fn prepare_current(
    session: &FastActorSessionV1,
    current: &mut FastActorCurrentDecisionV1,
) -> Result<(), FlatActionDecisionSliceErrorV1> {
    super::human_v8::prepare_current(session, current)
}

fn validate_current(
    session: &FastActorSessionV1,
    current: &FastActorCurrentDecisionV1,
) -> Result<(), FlatActionDecisionSliceErrorV1> {
    if session.flat_action_contract_mode != FlatActionContractModeV1::V5 {
        return Err(FlatActionDecisionSliceErrorV1::InvalidDecisionRelation);
    }
    if super::human_v8::normalized_origin(session, current)? != current.candidates {
        return Err(FlatActionDecisionSliceErrorV1::CorruptCurrentBinding);
    }
    Ok(())
}

impl FastActorSessionV1 {
    #[cfg(test)]
    pub(crate) fn flat_rich_current_input_v5(
        &self,
        expected: FastActorDecisionV1,
    ) -> Result<
        (
            crate::policy_observation_v8::ObservationV8,
            Vec<ActionSemanticV1>,
        ),
        FlatActionDecisionSliceErrorV1,
    > {
        let observation = self.flat_policy_observation_v5(expected)?;
        let current = self
            .current
            .as_ref()
            .ok_or(FlatActionDecisionSliceErrorV1::NoCurrentDecision)?;
        let actions = current
            .candidates
            .iter()
            .map(|candidate| {
                frozen_pending_trigger_semantic_v4(&self.state, candidate.semantic.clone())
            })
            .collect();
        Ok((observation, actions))
    }

    pub fn into_flat_policy_v5(mut self) -> Result<Self, RlContractError> {
        self.flat_action_contract_mode = FlatActionContractModeV1::V5;
        self.flat_action_cache_spare = None;
        self.flat_action_cache_spare_v2 = None;
        if let Some(mut current) = self.current.take() {
            prepare_current(&self, &mut current)
                .map_err(|error| RlContractError(format!("flat_v5:{error:?}")))?;
            self.current = Some(current);
        }
        Ok(self)
    }

    #[cfg(test)]
    pub(crate) fn from_v5_fixture_state(state: GameState) -> Self {
        let mut session = Self::reset_with_limits(23, 91, 10_000, 10_000);
        session.state = state;
        session.surface = PolicySurfaceV5::new_for_session();
        session.environment_revision = 0;
        session.policy_step_count = 0;
        session.physical_decision_count = 0;
        session.current = None;
        session.terminal = None;
        session.flat_action_contract_mode = FlatActionContractModeV1::V5;
        session.flat_action_cache_spare = None;
        session.flat_action_cache_spare_v2 = None;
        session.advance_to_decision_or_terminal();
        session
    }

    pub(crate) fn flat_policy_observation_v5(
        &self,
        expected: FastActorDecisionV1,
    ) -> Result<crate::policy_observation_v8::ObservationV8, FlatActionDecisionSliceErrorV1> {
        let current = self
            .current
            .as_ref()
            .ok_or(FlatActionDecisionSliceErrorV1::NoCurrentDecision)?;
        flat_validate_expected_decision_v1(self, current, expected)?;
        validate_current(self, current)?;
        crate::rl::observe_policy_v8_unhashed_for_flat_policy(
            &self.state,
            &self.surface,
            current.actor,
            self.policy_step_count,
            current.physical_decision_id,
            current.substep_index,
            current.substep_count,
        )
        .map_err(|_| FlatActionDecisionSliceErrorV1::CorruptCurrentBinding)
    }

    /// Re-derive the bound V5 rows before applying the selected executable
    /// candidate. Failures are read-only and cannot consume another mode.
    pub fn step_flat_action_v5(
        &mut self,
        expected: FastActorDecisionV1,
        binding: FlatActionDecisionBindingV5,
        selected_index: u32,
    ) -> Result<FastActorResponseV1, RlContractError> {
        let count = usize::try_from(expected.legal_action_count)
            .map_err(|_| RlContractError("flat_v5 action count overflow".into()))?;
        let refs_count = count
            .checked_mul(FLAT_ACTION_MAX_TRIGGER_ORDER_REFS_V1)
            .ok_or_else(|| RlContractError("flat_v5 reference count overflow".into()))?;
        let mut actions = vec![FlatActionCoreV1::default(); count];
        let mut refs = vec![FlatActionRefV2::default(); refs_count];
        let mut objects = vec![FlatActionObjectV2::default(); refs_count];
        let current = self
            .encode_current_flat_action_slice_v5(
                expected,
                &mut FlatActionDecisionSliceBuffersV2 {
                    actions: &mut actions,
                    refs: &mut refs,
                    objects: &mut objects,
                },
            )
            .map_err(|error| RlContractError(format!("flat_v5:{error:?}")))?;
        if current.binding != binding {
            return Err(RlContractError("flat_v5 binding changed".into()));
        }
        self.step(expected.episode_id, expected.step, selected_index)
            .map_err(|error| RlContractError(format!("flat_v5:{error:?}")))
    }

    pub(crate) fn encode_current_flat_action_slice_v5(
        &self,
        expected: FastActorDecisionV1,
        buffers: &mut FlatActionDecisionSliceBuffersV2<'_>,
    ) -> Result<FlatActionDecisionSliceV5, FlatActionDecisionSliceErrorV1> {
        let current = self
            .current
            .as_ref()
            .ok_or(FlatActionDecisionSliceErrorV1::NoCurrentDecision)?;
        flat_validate_expected_decision_v1(self, current, expected)?;
        validate_current(self, current)?;
        let extension = crate::rl::policy_observation_extensions_common_for_human_v8(
            &self.state,
            current.actor,
        )
        .map_err(|_| FlatActionDecisionSliceErrorV1::InvalidDecisionRelation)?;
        let actor: PlayerSeatV1 = current.actor.into();
        let mut actions_out: Vec<FlatActionCoreV1> = Vec::with_capacity(current.candidates.len());
        let mut unindexed_refs: Vec<FlatUnindexedActionRefV2> = Vec::new();
        // See `FlatResolvedActionObjectV5`'s doc comment for the two real
        // defects this dedup/consistency key was widened to fix, and why
        // `position` only joins the "already resolved" match when the
        // EXISTING entry's own resolution was position-sensitive.
        let mut resolved_objects: Vec<FlatResolvedActionObjectV5> = Vec::new();
        for (action_index, candidate) in current.candidates.iter().enumerate() {
            flat_validate_semantic_policy_pair_v1(candidate)?;
            let action_index_u32 = u32::try_from(action_index)
                .map_err(|_| FlatActionDecisionSliceErrorV1::CheckedIntegerRange)?;
            let ref_start = u32::try_from(unindexed_refs.len())
                .map_err(|_| FlatActionDecisionSliceErrorV1::CheckedIntegerRange)?;
            let semantic =
                frozen_pending_trigger_semantic_v4(&self.state, candidate.semantic.clone());
            let core = flat_action_core_and_refs_v1(
                &semantic,
                actor,
                ref_start,
                |role, order_index, associated_order, reference| {
                    let position = u32::from(order_index);
                    let (object, position_sensitive) =
                        flat_visible_action_object_extension_aware_v4(
                            &self.state,
                            current.actor,
                            position,
                            role,
                            reference,
                            &extension,
                        )?;
                    if let Some(previous) = resolved_objects.iter().find(|candidate| {
                        candidate.arena_id == reference.arena_id
                            && candidate.reference == *reference
                            && candidate.position_sensitive == position_sensitive
                            && (!position_sensitive || candidate.position == position)
                    }) {
                        if previous.object != object {
                            return Err(FlatActionDecisionSliceErrorV1::InvalidActionReference);
                        }
                    } else {
                        if resolved_objects.iter().any(|candidate| {
                            candidate.object.canonical_key() == object.canonical_key()
                        }) {
                            return Err(FlatActionDecisionSliceErrorV1::DuplicateCanonicalObject);
                        }
                        resolved_objects.push(FlatResolvedActionObjectV5 {
                            arena_id: reference.arena_id,
                            position,
                            reference: reference.clone(),
                            position_sensitive,
                            object,
                        });
                    }
                    unindexed_refs.push(FlatUnindexedActionRefV2 {
                        action_index: action_index_u32,
                        role,
                        order_index,
                        associated_order,
                        object,
                    });
                    Ok(())
                },
            )?;
            actions_out.push(core);
        }
        resolved_objects.sort_unstable_by_key(|candidate| candidate.object.canonical_key());
        if resolved_objects
            .windows(2)
            .any(|pair| pair[0].object.canonical_key() == pair[1].object.canonical_key())
        {
            return Err(FlatActionDecisionSliceErrorV1::DuplicateCanonicalObject);
        }
        let objects_out: Vec<FlatActionObjectV2> = resolved_objects
            .iter()
            .map(|candidate| candidate.object)
            .collect();
        let mut refs_out: Vec<FlatActionRefV2> = Vec::with_capacity(unindexed_refs.len());
        for reference in unindexed_refs {
            let key = reference.object.canonical_key();
            let object_index = objects_out
                .binary_search_by_key(&key, |candidate| candidate.canonical_key())
                .map_err(|_| FlatActionDecisionSliceErrorV1::InvalidActionReference)?;
            refs_out.push(FlatActionRefV2 {
                action_index: reference.action_index,
                role: reference.role,
                order_index: reference.order_index,
                associated_order: reference.associated_order,
                card_token: reference.object.card_token,
                object_index: u16::try_from(object_index)
                    .map_err(|_| FlatActionDecisionSliceErrorV1::CheckedIntegerRange)?,
            });
        }
        let action_count_u32 = u32::try_from(actions_out.len())
            .map_err(|_| FlatActionDecisionSliceErrorV1::CheckedIntegerRange)?;
        let commitment =
            flat_action_commitment_from_rows_v2(actor, &actions_out, &refs_out, &objects_out)?;
        let binding = flat_action_binding_v2(self, current, action_count_u32, commitment);
        // V5 authority also binds the complete typed public input, including
        // life fees and printed traits that do not alter Boolean action rows.
        use sha2::Digest;
        let observation = self.flat_policy_observation_v5(expected)?;
        let bytes = serde_json::to_vec(&observation)
            .map_err(|_| FlatActionDecisionSliceErrorV1::CorruptCurrentBinding)?;
        let observation_commitment: [u8; 32] = sha2::Sha256::digest(bytes).into();

        let (a, r, o) = (actions_out.len(), refs_out.len(), objects_out.len());
        if buffers.actions.len() < a {
            return Err(FlatActionDecisionSliceErrorV1::InsufficientActionCapacity {
                required: a,
                available: buffers.actions.len(),
            });
        }
        if buffers.refs.len() < r {
            return Err(FlatActionDecisionSliceErrorV1::InsufficientRefCapacity {
                required: r,
                available: buffers.refs.len(),
            });
        }
        if buffers.objects.len() < o {
            return Err(FlatActionDecisionSliceErrorV1::InsufficientObjectCapacity {
                required: o,
                available: buffers.objects.len(),
            });
        }
        buffers.actions[..a].copy_from_slice(&actions_out);
        buffers.refs[..r].copy_from_slice(&refs_out);
        buffers.objects[..o].copy_from_slice(&objects_out);
        Ok(FlatActionDecisionSliceV5 {
            binding: FlatActionDecisionBindingV5(binding, observation_commitment),
            active_action_count: a as u32,
            active_ref_count: r as u32,
            active_object_count: o as u16,
        })
    }
}
