//! Cache-free owning successor. Rebuild the real surface origin, complete
//! candidate menu and public observation before accepting every submission.
use crate::policy_observation_variable_cast_v1::{
    observe_policy_variable_cast_v1, ObservationVariableCastV1,
};
use crate::policy_surface_v5::{PolicyDecisionV5, PolicySurfaceV5};
use crate::rl_variable_cast_v1::{candidates, CandidateVariableCastV1, LegalActionVariableCastV1};
use crate::state::GameState;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const VARIABLE_CAST_SESSION_SCHEMA_V1: &str = "owning-variable-cast-session/v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VariableCastSessionErrorV1 {
    InvalidAuthority,
    StaleDecision,
    InvalidOrdinal,
    ConflictingRetry,
    CounterExhausted,
    ExecutionFailed,
}
type Result<T> = std::result::Result<T, VariableCastSessionErrorV1>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VariableCastBindingV1 {
    pub schema: String,
    pub episode: u64,
    pub revision: u64,
    pub step: u64,
    pub physical_decision: u64,
    pub commitment: [u8; 32],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VariableCastRequestV1 {
    pub binding: VariableCastBindingV1,
    pub ordinal: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VariableCastReceiptV1 {
    pub request: VariableCastRequestV1,
    pub resulting_revision: u64,
    pub resulting_step: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VariableCastResponseV1 {
    pub binding: VariableCastBindingV1,
    pub observation: ObservationVariableCastV1,
    pub actions: Vec<LegalActionVariableCastV1>,
    pub terminal: bool,
}

/// Opaque backend snapshot owns state and surface. No menu or executable
/// action is serialized or cached. Restore authenticates then rebuilds.
#[derive(Clone)]
pub struct VariableCastSnapshotV1 {
    session: VariableCastSessionV1,
}

#[derive(Clone)]
pub struct VariableCastSessionV1 {
    state: GameState,
    surface: PolicySurfaceV5,
    episode: u64,
    revision: u64,
    step: u64,
    physical_decision: u64,
    authority: [u8; 32],
    last: Option<VariableCastReceiptV1>,
}

fn digest<T: Serialize>(value: &T) -> Result<[u8; 32]> {
    let bytes =
        serde_json::to_vec(value).map_err(|_| VariableCastSessionErrorV1::InvalidAuthority)?;
    Ok(Sha256::digest(bytes).into())
}

impl VariableCastSessionV1 {
    /// Backend-trusted initial state. No registry substitution, model identity
    /// or predecessor catalog pin is introduced by this constructor.
    pub fn from_state(episode: u64, mut state: GameState) -> Result<Self> {
        let mut surface = PolicySurfaceV5::new_with_engine_priority_v1();
        surface
            .next_decision(&mut state)
            .map_err(|_| VariableCastSessionErrorV1::InvalidAuthority)?;
        let mut session = Self {
            state,
            surface,
            episode,
            revision: 0,
            step: 0,
            physical_decision: 0,
            authority: [0; 32],
            last: None,
        };
        session.authority = session.authority_digest()?;
        session.current()?;
        Ok(session)
    }

    fn authority_digest(&self) -> Result<[u8; 32]> {
        let p0 = self
            .surface
            .scan_context_for(crate::ids::PlayerId::P0)
            .map_err(|_| VariableCastSessionErrorV1::InvalidAuthority)?;
        let p1 = self
            .surface
            .scan_context_for(crate::ids::PlayerId::P1)
            .map_err(|_| VariableCastSessionErrorV1::InvalidAuthority)?;
        digest(&(
            &self.state,
            self.surface.harness_public_context(),
            p0,
            p1,
            self.episode,
            self.revision,
            self.step,
            self.physical_decision,
            &self.last,
        ))
    }

    fn validate_authority(&self) -> Result<()> {
        if self.authority_digest()? != self.authority {
            return Err(VariableCastSessionErrorV1::InvalidAuthority);
        }
        Ok(())
    }

    fn rebuild(
        &self,
    ) -> Result<(
        VariableCastResponseV1,
        Vec<CandidateVariableCastV1>,
        PolicyDecisionV5,
    )> {
        self.validate_authority()?;
        let mut state = self.state.clone();
        let mut surface = self.surface.clone();
        let decision = surface
            .next_decision(&mut state)
            .map_err(|_| VariableCastSessionErrorV1::InvalidAuthority)?;
        if state != self.state {
            return Err(VariableCastSessionErrorV1::InvalidAuthority);
        }
        let terminal = decision.is_terminal();
        let actor = decision.actor(&state).unwrap_or(state.active_player);
        let observation = observe_policy_variable_cast_v1(
            &state,
            &surface,
            actor,
            &decision,
            self.step,
            self.physical_decision,
        )
        .map_err(|_| VariableCastSessionErrorV1::InvalidAuthority)?;
        let menu = candidates(&state, &surface, &decision)
            .map_err(|_| VariableCastSessionErrorV1::InvalidAuthority)?;
        let actions = menu
            .iter()
            .map(|candidate| candidate.record.clone())
            .collect::<Vec<_>>();
        let commitment = digest(&(
            VARIABLE_CAST_SESSION_SCHEMA_V1,
            self.episode,
            self.revision,
            self.step,
            self.physical_decision,
            &observation,
            &actions,
            terminal,
        ))?;
        let binding = VariableCastBindingV1 {
            schema: VARIABLE_CAST_SESSION_SCHEMA_V1.into(),
            episode: self.episode,
            revision: self.revision,
            step: self.step,
            physical_decision: self.physical_decision,
            commitment,
        };
        Ok((
            VariableCastResponseV1 {
                binding,
                observation,
                actions,
                terminal,
            },
            menu,
            decision,
        ))
    }

    pub fn current(&self) -> Result<VariableCastResponseV1> {
        Ok(self.rebuild()?.0)
    }

    /// Failures leave the owning state, surface, counters and retry receipt
    /// unchanged. An identical immediate retry applies exactly once.
    pub fn submit(&mut self, request: VariableCastRequestV1) -> Result<VariableCastReceiptV1> {
        self.validate_authority()?;
        if let Some(last) = &self.last {
            if last.request == request {
                return Ok(last.clone());
            }
            if last.request.binding == request.binding {
                return Err(VariableCastSessionErrorV1::ConflictingRetry);
            }
        }
        let (response, menu, _) = self.rebuild()?;
        if request.binding != response.binding {
            return Err(VariableCastSessionErrorV1::StaleDecision);
        }
        let index = usize::try_from(request.ordinal)
            .map_err(|_| VariableCastSessionErrorV1::InvalidOrdinal)?;
        let candidate = menu
            .get(index)
            .ok_or(VariableCastSessionErrorV1::InvalidOrdinal)?;
        if candidate.record.ordinal != request.ordinal || response.terminal {
            return Err(VariableCastSessionErrorV1::InvalidOrdinal);
        }
        let mut next = self.clone();
        next.revision = next
            .revision
            .checked_add(1)
            .ok_or(VariableCastSessionErrorV1::CounterExhausted)?;
        next.step = next
            .step
            .checked_add(1)
            .ok_or(VariableCastSessionErrorV1::CounterExhausted)?;
        next.physical_decision = next
            .physical_decision
            .checked_add(1)
            .ok_or(VariableCastSessionErrorV1::CounterExhausted)?;
        next.surface
            .apply(&mut next.state, candidate.action.clone())
            .map_err(|_| VariableCastSessionErrorV1::ExecutionFailed)?;
        next.surface
            .next_decision(&mut next.state)
            .map_err(|_| VariableCastSessionErrorV1::ExecutionFailed)?;
        next.authority = next.authority_digest()?;
        next.current()?;
        let receipt = VariableCastReceiptV1 {
            request,
            resulting_revision: next.revision,
            resulting_step: next.step,
        };
        next.last = Some(receipt.clone());
        next.authority = next.authority_digest()?;
        *self = next;
        Ok(receipt)
    }

    pub fn save(&self) -> Result<VariableCastSnapshotV1> {
        self.current()?;
        Ok(VariableCastSnapshotV1 {
            session: self.clone(),
        })
    }

    pub fn restore(snapshot: VariableCastSnapshotV1) -> Result<Self> {
        let mut session = snapshot.session;
        session.current()?;
        session.revision = session
            .revision
            .checked_add(1)
            .ok_or(VariableCastSessionErrorV1::CounterExhausted)?;
        session.last = None;
        session.authority = session.authority_digest()?;
        session.current()?;
        Ok(session)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::PlayerId;
    use crate::policy_observation_v6::tests::{put, ready_state};
    use crate::rl_variable_cast_v1::ActionSemanticVariableCastV1 as A;
    use crate::state::{Step, Zone};

    fn ordinary(actor: PlayerId) -> VariableCastSessionV1 {
        let mut state = ready_state();
        state.active_player = actor;
        state.priority_player = actor;
        put(&mut state, actor, "Forest", Zone::Hand);
        VariableCastSessionV1::from_state(11062, state).unwrap()
    }

    fn request(
        session: &VariableCastSessionV1,
        predicate: impl Fn(&A) -> bool,
    ) -> VariableCastRequestV1 {
        let response = session.current().unwrap();
        let ordinal = response
            .actions
            .iter()
            .find(|action| predicate(&action.semantic))
            .unwrap()
            .ordinal;
        VariableCastRequestV1 {
            binding: response.binding,
            ordinal,
        }
    }

    #[test]
    fn registered_predecessor_projection_and_complete_menu_preserve_content() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let session = ordinary(actor);
            let fresh = session.current().unwrap();
            let old = crate::policy_observation_v9::observe_policy_v9(
                &session.state,
                &session.surface,
                actor,
                0,
                0,
                0,
                1,
            )
            .unwrap();
            assert_eq!(fresh.observation.projection, old.projection);
            assert_eq!(fresh.observation.own_hand, old.own_hand);
            assert_eq!(fresh.observation.known_hand_cards, old.known_hand_cards);
            assert_eq!(
                fresh.observation.known_library_cards,
                old.known_library_cards
            );
            assert_eq!(fresh.observation.extensions, old.extensions);
            assert!(fresh.observation.variable_cast.is_none());
            let (_, _, decision) = session.rebuild().unwrap();
            let old_menu =
                crate::rl::core_policy_action_candidates_v5(&decision, &session.state).unwrap();
            assert_eq!(fresh.actions.len(), old_menu.len());
            for (new, old) in fresh.actions.iter().zip(old_menu) {
                assert_eq!(
                    new.semantic,
                    A::Predecessor {
                        action: old.semantic
                    }
                );
            }
            let decoded: VariableCastResponseV1 =
                serde_json::from_slice(&serde_json::to_vec(&fresh).unwrap()).unwrap();
            assert_eq!(decoded, fresh);
        }
    }

    #[test]
    fn readonly_refusal_exact_once_retry_and_snapshot_rebuild() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut session = ordinary(actor);
            let before = session.current().unwrap();
            let authority = session.authority;
            let mut bad = VariableCastRequestV1 {
                binding: before.binding.clone(),
                ordinal: u64::MAX,
            };
            assert_eq!(
                session.submit(bad.clone()),
                Err(VariableCastSessionErrorV1::InvalidOrdinal)
            );
            assert_eq!(session.current().unwrap(), before);
            assert_eq!(session.authority, authority);
            bad.ordinal = 0;
            bad.binding.schema.push_str("/forged");
            assert_eq!(
                session.submit(bad),
                Err(VariableCastSessionErrorV1::StaleDecision)
            );
            assert_eq!(session.current().unwrap(), before);
            let chosen = request(&session, |semantic| {
                matches!(
                    semantic,
                    A::Predecessor {
                        action: crate::rl::ActionSemanticV1::PlayLand { .. }
                    }
                )
            });
            let saved = session.save().unwrap();
            let receipt = session.submit(chosen.clone()).unwrap();
            let after = session.current().unwrap();
            assert_eq!(session.submit(chosen.clone()).unwrap(), receipt);
            assert_eq!(session.current().unwrap(), after);
            let mut conflicting = chosen.clone();
            conflicting.ordinal = conflicting.ordinal.checked_add(1).unwrap();
            assert_eq!(
                session.submit(conflicting),
                Err(VariableCastSessionErrorV1::ConflictingRetry)
            );
            assert_eq!(session.current().unwrap(), after);
            let mut restored = VariableCastSessionV1::restore(saved).unwrap();
            assert_eq!(restored.current().unwrap().observation, before.observation);
            assert_ne!(restored.current().unwrap().binding, before.binding);
            assert_eq!(
                restored.submit(chosen),
                Err(VariableCastSessionErrorV1::StaleDecision)
            );
        }
    }

    #[test]
    fn mutable_owned_state_and_coherent_snapshot_forgery_refuse_before_submission() {
        let mut session = ordinary(PlayerId::P0);
        let offered = request(&session, |_| true);
        session.state.turn += 1;
        let before = serde_json::to_vec(&session.state).unwrap();
        assert_eq!(
            session.submit(offered),
            Err(VariableCastSessionErrorV1::InvalidAuthority)
        );
        assert_eq!(serde_json::to_vec(&session.state).unwrap(), before);
        let mut snapshot = ordinary(PlayerId::P1).save().unwrap();
        snapshot.session.state.step = Step::Main2;
        assert!(matches!(
            VariableCastSessionV1::restore(snapshot),
            Err(VariableCastSessionErrorV1::InvalidAuthority)
        ));
    }

    #[cfg(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    ))]
    fn divine(actor: PlayerId, count: usize) -> VariableCastSessionV1 {
        let mut state = ready_state();
        state.active_player = actor;
        state.priority_player = actor;
        state.players[actor.index()].mana_pool[crate::mana::ManaColor::W.pool_index()] = 4;
        put(&mut state, actor, "Divine Resilience", Zone::Hand);
        for _ in 0..count {
            put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
        }
        let mut session = VariableCastSessionV1::from_state(11062, state).unwrap();
        let (_, _, decision) = session.rebuild().unwrap();
        assert!(crate::rl::core_policy_action_candidates_v5(&decision, &session.state).is_err());
        let cast = request(&session, |semantic| {
            matches!(
                semantic,
                A::Predecessor {
                    action: crate::rl::ActionSemanticV1::CastSpell { .. }
                }
            )
        });
        session.submit(cast).unwrap();
        assert!(
            crate::rl::observe_policy_v8(&session.state, &session.surface, actor, 0, 0, 0, 1)
                .is_err()
        );
        session
    }

    #[cfg(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    ))]
    #[test]
    fn actual_registered_divine_zero_true_only_and_three_hundred_with_restored_finish() {
        use crate::policy_observation_variable_cast_v1::VariableCastContextV1 as C;
        for actor in [PlayerId::P0, PlayerId::P1] {
            for total in [0usize, 300] {
                let mut session = divine(actor, total);
                let Some(C::Kicker {
                    legal_pay_choices, ..
                }) = session.current().unwrap().observation.variable_cast
                else {
                    panic!("actual kicker");
                };
                assert_eq!(
                    legal_pay_choices,
                    if total == 0 {
                        vec![true]
                    } else {
                        vec![false, true]
                    }
                );
                let kick = request(&session, |semantic| {
                    matches!(semantic, A::ChooseVariableCastKicker { pay: true, .. })
                });
                session.submit(kick).unwrap();
                for selected in 0..total {
                    let response = session.current().unwrap();
                    let Some(C::Targets {
                        selected_count,
                        selected: captures,
                        minimum,
                        maximum,
                        can_finish,
                        ..
                    }) = response.observation.variable_cast
                    else {
                        panic!("actual targets");
                    };
                    assert_eq!(selected_count, u64::try_from(selected).unwrap());
                    assert_eq!(captures.len(), selected);
                    assert_eq!((minimum, maximum, can_finish), (0, None, true));
                    if selected == 120 {
                        session = VariableCastSessionV1::restore(session.save().unwrap()).unwrap();
                    }
                    let pick = request(&session, |semantic| {
                        matches!(semantic, A::ChooseVariableCastTarget { .. })
                    });
                    session.submit(pick).unwrap();
                }
                let before = session.current().unwrap();
                let Some(C::Targets {
                    selected_count,
                    selected,
                    maximum,
                    ..
                }) = &before.observation.variable_cast
                else {
                    panic!("final targets");
                };
                assert_eq!(*selected_count, u64::try_from(total).unwrap());
                assert_eq!(selected.len(), total);
                assert_eq!(*maximum, None);
                session = VariableCastSessionV1::restore(session.save().unwrap()).unwrap();
                let finish = request(&session, |semantic| {
                    matches!(semantic, A::FinishVariableCastTargets { .. })
                });
                session.submit(finish).unwrap();
                let item = session.state.stack.last().unwrap();
                assert!(item.kicked);
                assert_eq!(item.targets.len(), total);
                assert_eq!(item.v4.target_contracts.len(), total);
                assert!(session.state.engine.pending_cast.is_none());
            }
        }
    }

    #[cfg(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    ))]
    #[test]
    fn actual_divine_stale_capture_count_kicker_and_contract_forgery_refusals_are_readonly() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut original = divine(actor, 2);
            original
                .submit(request(&original, |semantic| {
                    matches!(semantic, A::ChooseVariableCastKicker { pay: true, .. })
                }))
                .unwrap();
            original
                .submit(request(&original, |semantic| {
                    matches!(semantic, A::ChooseVariableCastTarget { .. })
                }))
                .unwrap();
            let expected = request(&original, |semantic| {
                matches!(semantic, A::FinishVariableCastTargets { .. })
            });
            let (_, _, origin) = original.rebuild().unwrap();
            let mut private_first = original.state.clone();
            let mut private_second = original.state.clone();
            put(&mut private_first, actor, "Snap", Zone::Hand);
            put(&mut private_second, actor, "Lightning Bolt", Zone::Hand);
            let observer_view = |state| {
                observe_policy_variable_cast_v1(
                    state,
                    &original.surface,
                    actor.opponent(),
                    &origin,
                    original.step,
                    original.physical_decision,
                )
                .unwrap()
            };
            let first = observer_view(&private_first);
            assert!(first.variable_cast.is_none());
            assert!(first.known_hand_cards[actor.index()].is_empty());
            assert_eq!(first, observer_view(&private_second));
            for mutation in 0..6 {
                let mut session = original.clone();
                let pending = session.state.engine.pending_cast.as_mut().unwrap();
                let source = pending.spell;
                let target = pending.targets_chosen[0];
                match mutation {
                    0 => {
                        pending.targets_chosen.clear();
                        pending.target_contracts.clear();
                    }
                    1 => {
                        pending.kicked = Some(false);
                        session.state.stack.last_mut().unwrap().kicked = false;
                    }
                    2 => {
                        pending.target_contracts[0] = crate::state::StackTargetContractV4::capture(
                            &original.state,
                            crate::state::Target::Object(source),
                        );
                    }
                    3 => {
                        session.state.objects.get_mut(source).zone_change_count += 1;
                    }
                    4 => {
                        let crate::state::Target::Object(id) = target else {
                            unreachable!()
                        };
                        session.state.objects.get_mut(id).zone_change_count += 2;
                    }
                    5 => {
                        let crate::state::Target::Object(id) = target else {
                            unreachable!()
                        };
                        session.state.objects.get_mut(id).controller = actor.opponent();
                    }
                    _ => unreachable!(),
                }
                let before = serde_json::to_vec(&session.state).unwrap();
                assert_eq!(
                    session.submit(expected.clone()),
                    Err(VariableCastSessionErrorV1::InvalidAuthority)
                );
                assert_eq!(serde_json::to_vec(&session.state).unwrap(), before);
            }
            let (_, _, mut raw) = original.rebuild().unwrap();
            let PolicyDecisionV5::Surface(crate::surface_v2::SurfaceDecision::Decision(
                crate::engine::Decision::ChooseVariableCastTargets { selected_count, .. },
            )) = &mut raw
            else {
                panic!("variable origin");
            };
            *selected_count = 301;
            assert!(observe_policy_variable_cast_v1(
                &original.state,
                &original.surface,
                actor,
                &raw,
                original.step,
                original.physical_decision
            )
            .is_err());
        }
    }
}
