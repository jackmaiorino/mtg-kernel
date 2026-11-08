//! Separately owned prospective Eternal numeric actor. Native executable pairs
//! remain private. Capture is fail-closed on any unrepresented printed program
//! or dynamic operand; this engineering owner is not a catalog qualification.
use super::*;
use crate::current_mixed_policy_v1::{
    eternal_packet_v1, Error, EternalPublicV1, Packet, Result as ActorResult,
};
use std::sync::Arc;

pub struct EternalActorSessionV1 {
    inner: FastActorSessionV1,
    owner: Arc<()>,
    generation: u64,
    own_deck_types: Arc<[Vec<crate::card_def::Subtype>; 2]>,
}

#[cfg(all(test, feature = "eternal-fixtures"))]
mod tests {
    use super::*;
    use crate::current_tree_learner_v1::{AdamConfigV1, LearnerV1, ModelV1, TrainingExampleV1};

    fn mainboard(name: &str) -> Vec<u16> {
        let snapshot: serde_json::Value = serde_json::from_str(include_str!(
            "../../../data/eternal_v1/meta_snapshot_mtggoldfish_20261026.json"
        ))
        .unwrap();
        let list = snapshot["formats"]
            .as_object()
            .unwrap()
            .values()
            .flat_map(|format| format.as_array().unwrap())
            .find(|deck| deck["archetype"] == name)
            .unwrap();
        let cards = list["mainboard"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|row| {
                let id = crate::card_def::card_id_by_name(row[1].as_str().unwrap()).unwrap();
                std::iter::repeat_n(id, row[0].as_u64().unwrap() as usize)
            })
            .collect::<Vec<_>>();
        assert_eq!(cards.len(), 60);
        cards
    }

    #[test]
    fn issue454_eternal_reported_mainboards_use_sealed_scorer_and_learner() {
        for name in ["legacy-reanimator", "legacy-doomsday", "vintage-red-prison"] {
            for starting_player in [PlayerId::P0, PlayerId::P1] {
                let cards = mainboard(name);
                let mut session = EternalActorSessionV1::reset_explicit_mainboards(
                    454,
                    73,
                    (100, 200),
                    [name.into(), name.into()],
                    [cards.clone(), cards],
                    starting_player,
                )
                .unwrap_or_else(|error| panic!("{name}/{starting_player:?}: {error}"));
                let mut learner = LearnerV1::new(
                    ModelV1::deterministic_fixture(),
                    AdamConfigV1 {
                        learning_rate: 0.001,
                        beta1: 0.9,
                        beta2: 0.999,
                        epsilon: 1e-8,
                    },
                )
                .unwrap();
                for step in 0..8 {
                    let FastActorResponseV1::Decision(header) = session.current_response().unwrap()
                    else {
                        panic!("unexpected early terminal {name}/{starting_player:?}/{step}");
                    };
                    let decision = session.encode(header).unwrap_or_else(|error| {
                        panic!("{name}/{starting_player:?}/{step}: {error}")
                    });
                    assert_eq!(
                        decision.packet().source(),
                        crate::current_mixed_policy_v1::Source::EternalActorV1
                    );
                    if step == 0 {
                        let input = session.training_input_v1(&decision).unwrap();
                        let mut targets = vec![0.; input.action_flags().len()];
                        targets[0] = 1.;
                        assert!(learner
                            .update(&[TrainingExampleV1 {
                                input: &input,
                                targets: &targets
                            }])
                            .unwrap()
                            .is_finite());
                        let foreign = EternalActorSessionV1 {
                            inner: session.inner.clone(),
                            owner: Arc::new(()),
                            generation: 0,
                            own_deck_types: session.own_deck_types.clone(),
                        };
                        assert!(matches!(
                            foreign.training_input_v1(&decision),
                            Err(Error::StaleBinding)
                        ));
                        let snapshot = session.snapshot();
                        session.restore(&snapshot).unwrap();
                        assert!(matches!(
                            session.training_input_v1(&decision),
                            Err(Error::StaleBinding)
                        ));
                    }
                    let decision = session.encode(header).unwrap();
                    let (logits, selected, _) = session
                        .score_and_apply_tree_v1(&decision, learner.model())
                        .unwrap();
                    assert_eq!(logits.len(), header.legal_action_count as usize);
                    assert!(logits.iter().all(|value| value.is_finite()));
                    assert!((selected as usize) < logits.len());
                    assert!(matches!(
                        session.training_input_v1(&decision),
                        Err(Error::StaleBinding)
                    ));
                }
            }
        }
    }

    #[test]
    fn issue454_eternal_owner_hidden_hand_variants_leave_input_identical() {
        for player in [PlayerId::P0, PlayerId::P1] {
            let cards = mainboard("legacy-reanimator");
            let session = EternalActorSessionV1::reset_explicit_mainboards(
                454,
                73,
                (100, 200),
                ["legacy-reanimator".into(), "legacy-reanimator".into()],
                [cards.clone(), cards],
                player,
            )
            .unwrap();
            let FastActorResponseV1::Decision(header) = session.current_response().unwrap() else {
                panic!("decision");
            };
            let actor = session.inner.current.as_ref().unwrap().actor;
            let hidden = session.inner.state.players[actor.opponent().index()].hand[0];
            let baseline = session.encode(header).unwrap().packet;
            for name in ["Island", "Once Upon a Time", "Atraxa, Grand Unifier"] {
                let mut variant = EternalActorSessionV1 {
                    inner: session.inner.clone(),
                    owner: Arc::new(()),
                    generation: 0,
                    own_deck_types: session.own_deck_types.clone(),
                };
                let card = variant.inner.state.objects.get_mut(hidden);
                card.card_def = crate::card_def::card_id_by_name(name).unwrap();
                card.name = crate::card_def::CARD_DEFS[usize::from(card.card_def)]
                    .object_name
                    .into();
                card.v4 = crate::state::ObjectStateV4::from_card_def(card.card_def);
                assert_eq!(
                    variant.encode(header).unwrap().packet,
                    baseline,
                    "{player:?}/{name}"
                );
            }
        }
    }
}
#[derive(Clone)]
pub struct EternalActorDecisionV1 {
    expected: FastActorDecisionV1,
    original: Vec<CorePolicyActionCandidateV1>,
    packet: Packet,
    owner: Arc<()>,
    generation: u64,
}
#[derive(Clone)]
pub struct EternalActorSnapshotV1 {
    inner: FastActorSessionV1,
    owner: Arc<()>,
}
impl EternalActorDecisionV1 {
    pub fn expected(&self) -> FastActorDecisionV1 {
        self.expected
    }
    pub fn packet(&self) -> &Packet {
        &self.packet
    }
}
fn projection(error: impl std::fmt::Display) -> Error {
    Error::Projection(error.to_string())
}

impl EternalActorSessionV1 {
    /// Exact explicit mainboards use native Full admission and environment-v2
    /// construction. The finite printed contract adds its own independent
    /// guard; neither Partial status nor unsupported native resets are waived.
    pub fn reset_explicit_mainboards(
        episode_id: u64,
        env_seed: u64,
        limits: (u64, u64),
        deck_ids: SessionDeckIdsV1,
        mainboards: [Vec<u16>; 2],
        starting_player: PlayerId,
    ) -> ActorResult<Self> {
        if !cfg!(feature = "eternal-fixtures")
            || limits.0 == 0
            || limits.1 == 0
            || mainboards.iter().any(|deck| deck.len() != 60)
        {
            return Err(Error::UnsupportedOperand(
                "Eternal actor requires exact 60-card mainboards".into(),
            ));
        }
        for card in mainboards.iter().flatten() {
            crate::eternal_actor_printed_v1::printing_v1(*card).map_err(projection)?;
        }
        let own_deck_types = Arc::new(mainboards.each_ref().map(|cards| {
            let mut types: Vec<crate::card_def::Subtype> = Vec::new();
            for id in cards {
                let def = &crate::card_def::CARD_DEFS[usize::from(*id)];
                if !def.has_type(crate::card_def::CardType::Creature) {
                    continue;
                }
                for subtype in def.subtypes {
                    if subtype.is_creature_type()
                        && !types.iter().any(|known| known.same_subtype_as(subtype))
                    {
                        types.push(*subtype);
                    }
                }
            }
            types.sort_by_key(|subtype| subtype.stable_id());
            if types.is_empty() {
                types.push(crate::card_def::Subtype::CREATURE_TYPES[0]);
            }
            types
        }));
        let inner = FastActorSessionV1::reset_with_explicit_decks_and_limits_flat_action_v2_environment_v2_with_starting_player_v1(
            episode_id, env_seed, limits.0, limits.1, deck_ids, mainboards, starting_player,
        ).map_err(|error| Error::Projection(format!("{error:?}")))?;
        Ok(Self {
            inner,
            owner: Arc::new(()),
            generation: 0,
            own_deck_types,
        })
    }
    fn require_entry_visibility(&self) -> ActorResult<()> {
        if self.inner.current.as_ref().is_some_and(|current| {
            current.candidates.iter().any(|candidate| {
                matches!(&candidate.semantic, ActionSemanticV1::ChooseEffectOption { source, .. }
                if crate::card_def::CARD_DEFS.get(usize::from(source.card_db_id))
                    .is_some_and(|def| def.name == "Pithing Needle"))
            })
        }) {
            crate::eternal_actor_public_v1::require_continuous_needle_sources(&self.inner.state)
                .map_err(projection)?;
        }
        Ok(())
    }
    pub fn current_response(&self) -> ActorResult<FastActorResponseV1> {
        // A legacy native name-menu count is private until the successor
        // proves its operands public, including when publishing only a header.
        self.require_entry_visibility()?;
        Ok(self.inner.current_response())
    }
    pub fn snapshot(&self) -> EternalActorSnapshotV1 {
        EternalActorSnapshotV1 {
            inner: self.inner.clone(),
            owner: self.owner.clone(),
        }
    }
    pub fn restore(&mut self, snapshot: &EternalActorSnapshotV1) -> ActorResult<()> {
        if !Arc::ptr_eq(&self.owner, &snapshot.owner) {
            return Err(Error::ForeignSnapshot);
        }
        let generation = self.generation.checked_add(1).ok_or(Error::Range)?;
        let restored = Self {
            inner: snapshot.inner.clone(),
            owner: self.owner.clone(),
            generation,
            own_deck_types: self.own_deck_types.clone(),
        };
        if let FastActorResponseV1::Decision(expected) = restored.current_response()? {
            restored.encode(expected)?;
        }
        *self = restored;
        Ok(())
    }
    pub fn encode(&self, expected: FastActorDecisionV1) -> ActorResult<EternalActorDecisionV1> {
        if self.current_response()? != FastActorResponseV1::Decision(expected)
            || self.inner.flat_action_contract_mode != FlatActionContractModeV1::V2
        {
            return Err(Error::StaleBinding);
        }
        let current = self.inner.current.as_ref().ok_or(Error::NoDecision)?;
        if current.environment_revision != self.inner.environment_revision
            || current.bound_policy_step_count != self.inner.policy_step_count
            || current.bound_physical_decision_count != self.inner.physical_decision_count
            || current.physical_decision_id != self.inner.physical_decision_count
            || current.origin_decision.actor(&self.inner.state) != Some(current.actor)
        {
            return Err(Error::StaleBinding);
        }
        // Native pairs and filtered combat answers are rederived from the
        // genuine private origin. No caller-supplied observation grants authority.
        let raw = core_policy_action_candidates_v5(&current.origin_decision, &self.inner.state)
            .map_err(projection)?;
        let actual = flat_scan_menu_v2(
            &self.inner.surface,
            &self.inner.state,
            &current.origin_decision,
            raw,
        )
        .map_err(projection)?;
        if actual != current.candidates || actual.is_empty() {
            return Err(Error::StaleBinding);
        }
        self.inner
            .surface
            .scan_context_for_owned_revision_v1(
                &self.inner.state,
                current.actor,
                self.inner.environment_revision,
            )
            .map_err(projection)?;
        let state = &self.inner.state;
        let observation = observe_policy_v5_unhashed_for_flat_policy(
            state,
            &self.inner.surface,
            current.actor,
            self.inner.policy_step_count,
            current.physical_decision_id,
            current.substep_index,
            current.substep_count,
        )
        .map_err(projection)?;
        let stack =
            crate::eternal_actor_stack_v1::capture_v1(state, &observation.projection.surface.stack)
                .map_err(projection)?;
        let pending = crate::eternal_actor_stack_v1::capture_pending_v1(
            state,
            current.actor,
            &observation
                .projection
                .surface
                .engine_context
                .pending_triggers,
        )
        .map_err(projection)?;
        let resolving = crate::eternal_actor_stack_v1::capture_resolving_v1(
            state,
            current.actor,
            observation
                .projection
                .surface
                .engine_context
                .pending_effect
                .as_ref(),
        )
        .map_err(projection)?;
        let public = EternalPublicV1 {
            observation,
            dynamic: crate::eternal_actor_public_v1::capture_v1(state).map_err(projection)?,
            selection: crate::eternal_actor_choice_v1::capture_v1(state, current.actor)
                .map_err(projection)?,
            stack,
            pending,
            resolving,
        };
        let actions = actual
            .iter()
            .map(|candidate| {
                Ok((
                    candidate.semantic.clone(),
                    crate::eternal_actor_public_v1::entry_option_v1(
                        state,
                        &candidate.semantic,
                        Some(&self.own_deck_types[current.actor.index()]),
                    )
                    .map_err(projection)?,
                ))
            })
            .collect::<ActorResult<Vec<_>>>()?;
        let packet = eternal_packet_v1(&public, &actions)?;
        Ok(EternalActorDecisionV1 {
            expected,
            original: actual,
            packet,
            owner: self.owner.clone(),
            generation: self.generation,
        })
    }
    fn validate(&self, decision: &EternalActorDecisionV1) -> ActorResult<()> {
        if !Arc::ptr_eq(&self.owner, &decision.owner) || self.generation != decision.generation {
            return Err(Error::StaleBinding);
        }
        let fresh = self.encode(decision.expected)?;
        if fresh.original != decision.original || fresh.packet != decision.packet {
            return Err(Error::StaleBinding);
        }
        Ok(())
    }
    pub fn training_input_v1(
        &self,
        decision: &EternalActorDecisionV1,
    ) -> ActorResult<crate::current_tree_learner_v1::AuthenticatedInputV1> {
        self.validate(decision)?;
        crate::current_tree_learner_v1::AuthenticatedInputV1::from_current(
            decision.packet.clone(),
            &decision
                .original
                .iter()
                .map(|candidate| candidate.semantic.clone())
                .collect::<Vec<_>>(),
        )
    }
    pub fn apply(
        &mut self,
        decision: &EternalActorDecisionV1,
        selected_index: u32,
    ) -> ActorResult<FastActorResponseV1> {
        self.validate(decision)?;
        if selected_index as usize >= decision.original.len() {
            return Err(Error::ActionRange);
        }
        let generation = self.generation.checked_add(1).ok_or(Error::Range)?;
        self.inner
            .step(
                decision.expected.episode_id,
                decision.expected.step,
                selected_index,
            )
            .map_err(|error| Error::Projection(format!("{error:?}")))?;
        self.generation = generation;
        self.current_response()
    }
    pub fn score_and_apply_tree_v1(
        &mut self,
        decision: &EternalActorDecisionV1,
        model: &crate::current_tree_learner_v1::ModelV1,
    ) -> ActorResult<(Vec<f64>, u32, FastActorResponseV1)> {
        let input = self.training_input_v1(decision)?;
        let logits = model.forward(&input)?;
        let mut selected = 0;
        for index in 1..logits.len() {
            if logits[index] > logits[selected] {
                selected = index;
            }
        }
        let selected = u32::try_from(selected).map_err(|_| Error::Range)?;
        let response = self.apply(decision, selected)?;
        Ok((logits, selected, response))
    }
}
