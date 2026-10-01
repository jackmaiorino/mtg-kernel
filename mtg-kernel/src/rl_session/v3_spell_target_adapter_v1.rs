//! Explicit evaluation-only extension of V3's previously rejected spell targets.
//! No rollout is performed. Only actor-visible encoder rows leave this module.
use super::*;
#[cfg(any(test, feature = "experimental-burn-net8-packed-cuda-v1"))]
use crate::policy_observation_v6::ObservationV6;

pub(super) fn visible_spell_target(
    state: &crate::state::GameState,
    actor: PlayerId,
    reference: &CardStableRefV1,
) -> Result<FlatActionObjectV2, FlatActionDecisionSliceErrorV1> {
    let error = match flat_visible_action_object_v2(state, actor, reference) {
        Ok(row) => return Ok(row),
        Err(error) => error,
    };
    if error != FlatActionDecisionSliceErrorV1::InvalidActionReference
        || reference.zone != Zone::Stack
    {
        return Err(error);
    }
    let id = ObjectId(reference.arena_id);
    let live = state.objects.try_get(id).ok_or(error)?;
    if live.card_def != reference.card_db_id
        || PlayerSeatV1::from(live.owner) != reference.owner
        || PlayerSeatV1::from(live.controller) != reference.controller
        || live.zone != Zone::Stack
        || live.zone_change_count != reference.zone_change_count
    {
        return Err(error);
    }
    let mut spells =
        state.stack.iter().enumerate().filter(|(_, item)| {
            item.source == id && item.kind == crate::state::StackItemKind::Spell
        });
    let (ordinal, spell) = spells.next().ok_or(error)?;
    if spells.next().is_some()
        || spell.controller != live.controller
        || crate::engine::validated_stack_item_target_spec(spell, state).is_err()
    {
        return Err(error);
    }
    Ok(FlatActionObjectV2 {
        card_token: flat_card_token_v2(live.card_def),
        group: FlatActionObjectGroupV1::Stack,
        actor_visible_ordinal: u16::try_from(ordinal)
            .map_err(|_| FlatActionDecisionSliceErrorV1::CheckedIntegerRange)?,
        owner_relative: flat_relative_seat_v1(reference.owner, actor.into())?,
        controller_relative: flat_relative_seat_v1(reference.controller, actor.into())?,
        zone: flat_zone_v1(Zone::Stack),
        zone_change_count: reference.zone_change_count,
    })
}

#[cfg(any(test, feature = "experimental-burn-net8-packed-cuda-v1"))]
impl FastActorSessionV1 {
    /// The original scorer remains the first path. Only a rejected action
    /// reference can enter the explicitly identified adapter. The copy never
    /// steps the game, consumes RNG, or exposes hidden state to the policy.
    pub(crate) fn encode_v3_spell_target_adapter_v1(
        &self,
        expected: FastActorDecisionV1,
        encoder: &mut crate::flat_policy_v3::FlatDecisionEncoderV3,
        buffers: &mut crate::flat_policy_v2::FlatScoringOwnedBuffersV2<'_>,
    ) -> Result<
        (crate::flat_policy_v3::FlatDecisionV3, bool),
        crate::flat_policy_v2::FlatDecisionErrorV2,
    > {
        use crate::flat_policy_v2::FlatDecisionErrorV2;
        match self.encode_current_flat_scoring_decision_owned_v3(expected, encoder, buffers) {
            Ok(value) => return Ok((value, false)),
            Err(FlatDecisionErrorV2::Action(
                FlatActionDecisionSliceErrorV1::InvalidActionReference,
            )) => {}
            Err(error) => return Err(error),
        }
        let view = self.v3_spell_target_adapter_view(expected)?;
        let value =
            view.encode_current_flat_scoring_decision_owned_v3(expected, encoder, buffers)?;
        Ok((value, true))
    }

    /// Diagnostic recording must validate the same public references as the
    /// explicitly adapted scorer. The original human transport stays unchanged.
    pub(crate) fn diagnostic_visible_spell_adapter_v1(
        &self,
        expected: FastActorDecisionV1,
    ) -> Result<(ObservationV6, Vec<ActionSemanticV1>, bool), FlatActionDecisionSliceErrorV1> {
        match self.human_current_decision_input_v1(expected, expected.acting_player) {
            Ok((observation, actions, _)) => return Ok((observation, actions, false)),
            Err(FlatActionDecisionSliceErrorV1::InvalidActionReference) => {}
            Err(error) => return Err(error),
        }
        let view = self.v3_spell_target_adapter_view(expected)?;
        let (observation, actions, _) =
            view.human_current_decision_input_v1(expected, expected.acting_player)?;
        Ok((observation, actions, true))
    }

    fn v3_spell_target_adapter_view(
        &self,
        expected: FastActorDecisionV1,
    ) -> Result<Self, FlatActionDecisionSliceErrorV1> {
        let mut view = self.clone();
        view.v3_spell_target_reference_adapter = true;
        let mut current = view
            .current
            .take()
            .ok_or(FlatActionDecisionSliceErrorV1::NoCurrentDecision)?;
        flat_validate_expected_decision_v1(&view, &current, expected)?;
        let original = current.candidates.clone();
        let cache = super::flat_action_v3::prepare_and_build_v3(&view, &mut current)?;
        // Never remap a returned action onto another executable decision.
        if current.candidates != original {
            return Err(FlatActionDecisionSliceErrorV1::CorruptCurrentBinding);
        }
        flat_install_action_cache_build_result_v2(&mut current, Ok(cache));
        view.current = Some(current);
        Ok(view)
    }
}

#[cfg(test)]
pub(crate) fn pyroblast_target_fixture_v1() -> (crate::state::GameState, ObjectId, ObjectId) {
    use crate::engine::{self, Action, Decision};
    use crate::policy_observation_v6::tests::{put, ready_state};
    use crate::state::Target;
    let mut state = ready_state();
    let chrysalis = put(&mut state, PlayerId::P0, "Writhing Chrysalis", Zone::Hand);
    let counterspell = put(&mut state, PlayerId::P1, "Counterspell", Zone::Hand);
    let pyroblast = put(&mut state, PlayerId::P0, "Pyroblast", Zone::Hand);
    // Keep a genuine priority choice after target selection, so the fast
    // session does not auto-pass through the entire stack before inspection.
    put(&mut state, PlayerId::P0, "Lightning Bolt", Zone::Hand);
    state.players[0].mana_pool[ManaColor::R.pool_index()] = 3;
    state.players[0].mana_pool[ManaColor::G.pool_index()] = 1;
    state.players[0].mana_pool[ManaColor::C.pool_index()] = 2;
    state.players[1].mana_pool[ManaColor::U.pool_index()] = 2;
    engine::step(&mut state, Action::CastSpell(chrysalis)).unwrap();
    engine::advance_until_decision(&mut state);
    assert_eq!(state.stack.len(), 2);
    engine::step(&mut state, Action::Pass).unwrap();
    engine::advance_until_decision(&mut state);
    engine::step(&mut state, Action::CastSpell(counterspell)).unwrap();
    engine::step(&mut state, Action::ChooseTarget(Target::Object(chrysalis))).unwrap();
    engine::advance_until_decision(&mut state);
    engine::step(&mut state, Action::Pass).unwrap();
    engine::advance_until_decision(&mut state);
    engine::step(&mut state, Action::CastSpell(pyroblast)).unwrap();
    if matches!(
        engine::advance_until_decision(&mut state),
        Decision::ChooseSpellMode { .. }
    ) {
        engine::step(&mut state, Action::ChooseSpellMode(0)).unwrap();
    }
    let Decision::ChooseTargets { legal_targets, .. } = engine::advance_until_decision(&mut state)
    else {
        panic!("expected Pyroblast target selection");
    };
    assert_eq!(
        legal_targets,
        vec![Target::Object(chrysalis), Target::Object(counterspell)]
    );
    (state, chrysalis, counterspell)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reference(state: &crate::state::GameState, object: ObjectId) -> CardStableRefV1 {
        let live = state.objects.get(object);
        CardStableRefV1 {
            arena_id: object.0,
            card_db_id: live.card_def,
            owner: live.owner.into(),
            controller: live.controller.into(),
            zone: live.zone,
            zone_change_count: live.zone_change_count,
        }
    }

    #[test]
    fn v3_spell_target_adapter_resolves_real_targets_and_rejects_ambiguous_or_stale_identity() {
        let (state, chrysalis, counterspell) = pyroblast_target_fixture_v1();
        let target = reference(&state, chrysalis);
        assert_eq!(
            flat_visible_action_object_v2(&state, PlayerId::P0, &target).unwrap_err(),
            FlatActionDecisionSliceErrorV1::InvalidActionReference
        );
        let repaired = visible_spell_target(&state, PlayerId::P0, &target).unwrap();
        assert_eq!(repaired.group, FlatActionObjectGroupV1::Stack);
        assert_eq!(repaired.actor_visible_ordinal, 0);
        let other = reference(&state, counterspell);
        assert_eq!(
            visible_spell_target(&state, PlayerId::P0, &other).unwrap(),
            flat_visible_action_object_v2(&state, PlayerId::P0, &other).unwrap()
        );
        let mut stale = target.clone();
        stale.zone_change_count += 1;
        assert!(visible_spell_target(&state, PlayerId::P0, &stale).is_err());
        let mut wrong_card = target.clone();
        wrong_card.card_db_id = other.card_db_id;
        assert!(visible_spell_target(&state, PlayerId::P0, &wrong_card).is_err());
        let mut duplicate = state.clone();
        duplicate.stack.push(duplicate.stack[0].clone());
        assert!(visible_spell_target(&duplicate, PlayerId::P0, &target).is_err());
        let mut no_spell = state.clone();
        no_spell.stack[0].kind = crate::state::StackItemKind::TriggeredAbility;
        assert!(visible_spell_target(&no_spell, PlayerId::P0, &target).is_err());
    }

    #[test]
    fn v3_spell_target_adapter_preserves_each_executable_pyroblast_choice() {
        use crate::engine::{self, Action, Decision};
        use crate::state::Target;
        let (state, chrysalis, counterspell) = pyroblast_target_fixture_v1();
        let session = FastActorSessionV1::from_v3_fixture_state(state);
        let FastActorResponseV1::Decision(expected) = session.current_response() else {
            panic!()
        };
        assert_eq!(expected.legal_action_count, 2);
        let mut view = session.clone();
        view.v3_spell_target_reference_adapter = true;
        let mut current = view.current.take().unwrap();
        let original = current.candidates.clone();
        let cache =
            super::super::flat_action_v3::prepare_and_build_v3(&view, &mut current).unwrap();
        assert_eq!(current.candidates, original);
        let ordinals: Vec<_> = cache
            .refs
            .iter()
            .filter(|r| r.role == FlatActionRefRoleV1::TargetObject)
            .map(|r| cache.objects[usize::from(r.object_index)].actor_visible_ordinal)
            .collect();
        assert_eq!(ordinals, vec![0, 2]);
        for (index, target) in [chrysalis, counterspell].into_iter().enumerate() {
            let mut applied = session.clone();
            applied
                .step(expected.episode_id, expected.step, index as u32)
                .unwrap();
            assert_eq!(
                applied.state.stack.last().unwrap().targets,
                vec![Target::Object(target)]
            );
            let pyroblast = applied.state.stack.last().unwrap().source;
            for _ in 0..8 {
                if applied.state.objects.get(pyroblast).zone != Zone::Stack {
                    break;
                }
                assert!(matches!(
                    engine::advance_until_decision(&mut applied.state),
                    Decision::CastSpellOrPass { .. }
                ));
                engine::step(&mut applied.state, Action::Pass).unwrap();
            }
            assert_eq!(applied.state.objects.get(pyroblast).zone, Zone::Graveyard);
            assert_eq!(
                applied.state.objects.get(counterspell).zone == Zone::Graveyard,
                target == counterspell
            );
        }
    }
}
