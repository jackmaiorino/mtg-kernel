//! Backend-only context derivation; no hidden inventory traversal or cached menu authority.
use super::*;
use crate::effect::EffectOp;
use crate::soul_shackled_zombie_public_v1::{
    SingleGraveyardContextV1, TriggerStageV1, ZombiePublicObservationV1,
};
use crate::state::{AbilitySourceContractV4, StackTargetContractV4};

fn source_ref(c: AbilitySourceContractV4) -> CardStableRefV1 {
    CardStableRefV1 {
        arena_id: c.source.0,
        card_db_id: c.card_def,
        owner: c.owner.into(),
        controller: c.controller.into(),
        zone: c.zone,
        zone_change_count: c.zone_change_count,
    }
}
fn selected_ref(c: &StackTargetContractV4) -> Result<CardStableRefV1, RlContractError> {
    let StackTargetContractV4::Object {
        object,
        card_def,
        owner,
        controller,
        zone,
        zone_change_count,
        ..
    } = *c
    else {
        return Err(RlContractError(
            "Zombie target is not a captured card".into(),
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
impl FastActorSessionV1 {
    #[cfg(test)]
    pub(crate) fn zombie_fixture_state(&self) -> &crate::state::GameState {
        &self.state
    }
    #[cfg(test)]
    pub(crate) fn zombie_fixture_state_mut(&mut self) -> &mut crate::state::GameState {
        &mut self.state
    }
    pub(crate) fn human_zombie_observation_v13(
        &self,
        expected: FastActorDecisionV1,
    ) -> Result<ZombiePublicObservationV1, RlContractError> {
        let base = self
            .human_raid_observation_v9(expected, expected.acting_player)
            .map_err(|_| RlContractError("stale Zombie public decision".into()))?;
        let current = self
            .current
            .as_ref()
            .ok_or_else(|| RlContractError("missing Zombie decision".into()))?;
        let mut contexts = Vec::new();
        for (ordinal, pending) in self.state.engine.pending_triggers.iter().enumerate() {
            if pending.effect != EffectOp::ExileSingleGraveyardTargetsThenDrainTwoV1 {
                continue;
            }
            crate::engine::validate_single_graveyard_public_pending(&self.state, pending)
                .map_err(RlContractError)?;
            let source = pending
                .source_contract
                .ok_or_else(|| RlContractError("missing Zombie source".into()))?;
            let (legal_choices, can_finish) = match &current.origin_decision {
                PolicyDecisionV5::Surface(SurfaceDecision::Decision(Decision::ChooseTargets {
                    spell,
                    legal_targets,
                    can_finish,
                    ..
                })) if ordinal == 0 && *spell == pending.source => {
                    let choices = legal_targets
                        .iter()
                        .map(|target| {
                            selected_ref(&StackTargetContractV4::capture(&self.state, *target))
                        })
                        .collect::<Result<Vec<_>, RlContractError>>()?;
                    (choices, *can_finish)
                }
                _ => (Vec::new(), false),
            };
            contexts.push(
                self.zombie_context(
                    TriggerStageV1::Pending {
                        ordinal: u64::try_from(ordinal)
                            .map_err(|_| RlContractError("pending ordinal overflow".into()))?,
                    },
                    source,
                    &pending.targets,
                    &pending.target_contracts,
                    legal_choices,
                    can_finish,
                )?,
            );
        }
        for (ordinal, item) in self.state.stack.iter().enumerate() {
            if item.inline_effect.as_ref()
                != Some(&EffectOp::ExileSingleGraveyardTargetsThenDrainTwoV1)
            {
                continue;
            }
            crate::engine::validated_stack_item_target_spec(item, &self.state)
                .map_err(RlContractError)?;
            let source = item
                .v4
                .ability_source_contract
                .ok_or_else(|| RlContractError("missing Zombie stack source".into()))?;
            contexts.push(
                self.zombie_context(
                    TriggerStageV1::Stack {
                        ordinal: u64::try_from(ordinal)
                            .map_err(|_| RlContractError("stack ordinal overflow".into()))?,
                    },
                    source,
                    &item.targets,
                    &item.v4.target_contracts,
                    Vec::new(),
                    false,
                )?,
            );
        }
        crate::soul_shackled_zombie_public_v1::from_authenticated_v9(base, contexts)
    }
    fn zombie_context(
        &self,
        stage: TriggerStageV1,
        source: AbilitySourceContractV4,
        targets: &[Target],
        contracts: &[StackTargetContractV4],
        legal_choices: Vec<CardStableRefV1>,
        can_finish: bool,
    ) -> Result<SingleGraveyardContextV1, RlContractError> {
        crate::soul_shackled_zombie_v1::validate_source(&self.state, source)
            .map_err(RlContractError)?;
        crate::soul_shackled_zombie_v1::validate_target_contracts(contracts)
            .map_err(RlContractError)?;
        let selected = contracts
            .iter()
            .map(selected_ref)
            .collect::<Result<Vec<_>, _>>()?;
        let selected_live_legal = targets
            .iter()
            .enumerate()
            .map(|(i, t)| {
                self.state
                    .objects
                    .try_get(match t {
                        Target::Object(id) => *id,
                        _ => return false,
                    })
                    .is_some_and(|o| {
                        o.zone_change_count == selected[i].zone_change_count
                            && o.zone == selected[i].zone
                    })
                    && crate::engine::effect_target_is_legal_from_ability_source(
                        &self.state,
                        source,
                        source.controller,
                        crate::card_def::TargetSpec::UpToTwoCardsInSingleGraveyard,
                        targets,
                        i,
                    )
            })
            .collect();
        Ok(SingleGraveyardContextV1 {
            stage,
            source: source_ref(source),
            original_controller: source.controller.into(),
            captured_definition_colors: crate::card_def::CARD_DEFS[source.card_def as usize]
                .colors
                .to_vec(),
            selected_graveyard_owner: selected.first().map(|r| r.owner),
            selected,
            selected_live_legal,
            minimum: 0,
            maximum: 2,
            legal_choices,
            can_finish,
        })
    }
}
