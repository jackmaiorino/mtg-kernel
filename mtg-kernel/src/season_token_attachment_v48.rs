//! Exact one-of-created-token attachment under the genuine enclosing operation.
//! The original Equipment remains a single object when token creation doubles.
use crate::effect::EffectObjectBinding;
use crate::linked_aura_operation_v1::{GeneralAnswerV36, GeneralChoiceV36, OperationV1};
use crate::state::{AbilitySourceContractV4, GameState, ObjectLinkV4, Zone};
use serde::{Deserialize, Serialize};
use std::hash::{Hash, Hasher};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChoiceV48 {
    pub source: AbilitySourceContractV4,
    pub controller: crate::ids::PlayerId,
    pub candidates: Vec<EffectObjectBinding>,
    pub creations: Vec<u64>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CheckpointV48 {
    pub original: Box<GameState>,
    pub operation: OperationV1,
    pub answers: Vec<GeneralAnswerV36>,
    pub choice: GeneralChoiceV36,
    pub input: u64,
    pub used_banner: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InputV48 {
    pub original: Box<GameState>,
    pub operation: OperationV1,
    pub prior_answers: Vec<GeneralAnswerV36>,
    pub choice: ChoiceV48,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum JournalV48 {
    Input {
        input: Box<InputV48>,
    },
    Completed {
        input: u64,
        selected: EffectObjectBinding,
        attachment: u64,
    },
    // Independently emitted only by the actual two-way relation commit helper.
    AttachmentCommitted {
        source: ObjectLinkV4,
        host: ObjectLinkV4,
    },
}
impl Hash for JournalV48 {
    fn hash<H: Hasher>(&self, state: &mut H) {
        serde_json::to_vec(self)
            .expect("private attachment occurrence serializes")
            .hash(state);
    }
}
pub(crate) fn record(state: &mut GameState, journal: JournalV48) -> u64 {
    let at = state.engine.event_history.len() as u64;
    state
        .engine
        .event_history
        .push(crate::event::CommittedEvent::TokenAttachmentJournalV48 {
            journal: Box::new(journal),
        });
    at
}
pub(crate) fn pending(state: &GameState) -> Option<&CheckpointV48> {
    state
        .engine
        .linked_aura_operation_v1
        .pending_token_attachment_v48
        .as_deref()
}
pub(crate) fn validate_choice(state: &GameState, choice: &ChoiceV48) -> Result<(), String> {
    let source = state
        .objects
        .try_get(choice.source.source)
        .ok_or("token Equipment missing")?;
    if source.zone != Zone::Battlefield
        || source.zone_change_count != choice.source.zone_change_count
        || source.v4.face_down_v1.is_some()
        || !crate::engine::object_has_type(
            state,
            choice.source.source,
            crate::card_def::CardType::Artifact,
        )
        || crate::engine::object_has_type(
            state,
            choice.source.source,
            crate::card_def::CardType::Creature,
        )
        || source.owner != choice.source.owner
        || source.card_def != choice.source.card_def
        || crate::card_def::CARD_DEFS[usize::from(source.card_def)]
            .equipment
            .is_none()
        || choice.candidates.len() < 2
        || choice.candidates.len() != choice.creations.len()
    {
        return Err("token Equipment choice lost original source or complete roster".into());
    }
    for (at, candidate) in choice.creations.iter().zip(&choice.candidates) {
        let live = state
            .objects
            .try_get(candidate.object)
            .ok_or("created attachment token absent")?;
        if candidate.expected_zone != Zone::Battlefield
            || live.zone != Zone::Battlefield
            || live.zone_change_count != candidate.expected_zone_change_count
            || live.controller != choice.controller
            || !live.v4.is_token
            || !crate::engine::object_has_type(
                state,
                candidate.object,
                crate::card_def::CardType::Creature,
            )
            || !matches!(state.engine.event_history.get(*at as usize), Some(crate::event::CommittedEvent::CreateToken { object, controller, .. }) if *object == candidate.object && *controller == choice.controller)
        {
            return Err("token Equipment choice changed a real created incarnation".into());
        }
    }
    if choice
        .candidates
        .iter()
        .enumerate()
        .any(|(i, c)| choice.candidates[..i].contains(c))
    {
        return Err("token Equipment choice repeated a candidate".into());
    }
    Ok(())
}
pub(crate) fn validate_journals(state: &GameState) -> Result<(), String> {
    if state.engine.event_log.iter().any(|event| {
        matches!(
            event,
            crate::event::CommittedEvent::TokenAttachmentJournalV48 { .. }
        )
    }) {
        return Err("private token attachment history injected into event log".into());
    }
    for (at, event) in state.engine.event_history.iter().enumerate() {
        let crate::event::CommittedEvent::TokenAttachmentJournalV48 { journal } = event else {
            continue;
        };
        match journal.as_ref() {
            JournalV48::Input { input } => {
                let closed = state
                    .engine
                    .event_history
                    .iter()
                    .enumerate()
                    .filter_map(|(i, e)| match e {
                        crate::event::CommittedEvent::TokenAttachmentJournalV48 { journal } => {
                            match journal.as_ref() {
                                JournalV48::Completed {
                                    input: owner,
                                    selected,
                                    attachment,
                                } if *owner == at as u64 => Some((i, *selected, *attachment)),
                                _ => None,
                            }
                        }
                        _ => None,
                    })
                    .collect::<Vec<_>>();
                if closed.is_empty() {
                    let pending =
                        pending(state).ok_or("mandatory token attachment owner erased")?;
                    if pending.input != at as u64
                        || *pending.original != *input.original
                        || pending.operation != input.operation
                        || pending.answers != input.prior_answers
                        || pending.choice
                            != (GeneralChoiceV36::TokenEquipmentV48 {
                                choice: Box::new(input.choice.clone()),
                                input: at as u64,
                            })
                    {
                        return Err("mandatory token attachment input lost exact checkpoint".into());
                    }
                } else {
                    let [(closed_at, selected, attachment)] = closed.as_slice() else {
                        return Err("token attachment completed more than once".into());
                    };
                    if *attachment <= at as u64
                        || *attachment >= *closed_at as u64
                        || !input.choice.candidates.contains(selected)
                        || !matches!(state.engine.event_history.get(*attachment as usize), Some(crate::event::CommittedEvent::TokenAttachmentJournalV48 { journal }) if matches!(journal.as_ref(),JournalV48::AttachmentCommitted { source, host } if source.object==input.choice.source.source && source.zone_change_count==input.choice.source.zone_change_count && host.object==selected.object && host.zone_change_count==selected.expected_zone_change_count))
                    {
                        return Err(
                            "token attachment closure lacks actual independent relation commit"
                                .into(),
                        );
                    }
                }
            }
            JournalV48::Completed { input, .. } => {
                if !matches!(state.engine.event_history.get(*input as usize), Some(crate::event::CommittedEvent::TokenAttachmentJournalV48 { journal }) if matches!(journal.as_ref(),JournalV48::Input { .. }))
                {
                    return Err("token attachment completion orphaned".into());
                }
            }
            JournalV48::AttachmentCommitted { .. } => {}
        }
    }
    Ok(())
}

#[cfg(all(
    test,
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
pub(crate) mod tests {
    use super::*;
    use crate::{
        engine::{self, Action, Decision},
        event::{self, ProposedEvent},
        ids::PlayerId,
        policy_observation_v6::tests::{put, ready_state_for},
    };
    #[test]
    fn genuine_absent_relation_commit_emits_no_new_private_state() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut state = ready_state_for(actor);
            let equipment = put(&mut state, actor, "Viridian Longbow", Zone::Battlefield);
            let creature = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
            state
                .attach_object_exact(equipment, 0, creature, 0)
                .unwrap();
            assert!(state
                .engine
                .event_history
                .iter()
                .all(|e| !matches!(e, event::CommittedEvent::TokenAttachmentJournalV48 { .. })));
            let value = serde_json::to_value(&state.engine.linked_aura_operation_v1).unwrap();
            assert!(value.get("pending_token_attachment_v48").is_none());
            let restored: GameState =
                serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
            assert_eq!(state, restored);
        }
    }
    pub(crate) fn actual_primitive_checkpoint(
        actor: PlayerId,
    ) -> (GameState, crate::ids::ObjectId) {
        let mut state = ready_state_for(actor);
        for owner in [actor, actor.opponent()] {
            put(&mut state, owner, "Forest", Zone::Battlefield);
            put(&mut state, owner, "Forest", Zone::Library);
        }
        put(&mut state, actor, "Forest", Zone::Hand);
        let season = put(&mut state, actor, "Doubling Season", Zone::Hand);
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(season, Zone::Battlefield),
        );
        let equipment = put(&mut state, actor, "Viridian Longbow", Zone::Battlefield);
        let mut ctx = crate::effect::ExecCtx::no_targets(equipment, actor);
        ctx.ability_source_contract = Some(AbilitySourceContractV4::capture(&state, equipment));
        let token = crate::card_def::card_id_by_name("Goblin Token").unwrap();
        // Existing token-plus-attachment primitive on genuine physical objects.
        // This deliberately does not assign Viridian Longbow a fictional printed trigger.
        crate::effect::execute(
            &crate::effect::EffectOp::CreateTokenAndAttachSource { token_def: token },
            &ctx,
            &mut state,
        );
        assert!(state.engine.halted.is_none());
        assert!(pending(&state).is_some());
        (state, equipment)
    }
    #[test]
    #[cfg(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    ))]
    fn genuine_created_token_attachment_choice_restore_and_original_history_refusal_both_seats() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let (state, equipment) = actual_primitive_checkpoint(actor);
            let Decision::ChooseEffectTargets {
                player,
                source,
                legal_targets,
                min_targets,
                max_targets,
                can_finish,
                ..
            } = engine::advance_until_decision(&mut state.clone())
            else {
                panic!("actual attachment choice absent")
            };
            assert_eq!(player, actor);
            assert_eq!(source, equipment);
            assert_eq!((min_targets, max_targets, can_finish), (1, 1, false));
            assert_eq!(legal_targets.len(), 2);
            assert!(state.objects.get(equipment).v4.attached_to.is_none());
            crate::doubling_season_v48::validate_token_journals(&state).unwrap();
            let original = state.clone();
            for selected in &legal_targets {
                let crate::state::Target::Object(selected) = *selected else {
                    unreachable!()
                };
                let mut branch = original.clone();
                for _ in 0..3 {
                    branch = serde_json::from_slice(&serde_json::to_vec(&branch).unwrap()).unwrap();
                    crate::linked_aura_operation_v1::validate(&branch).unwrap();
                }
                engine::step(
                    &mut branch,
                    Action::ChooseEffectTarget(crate::state::Target::Object(selected)),
                )
                .unwrap();
                assert!(pending(&branch).is_none());
                assert_eq!(
                    branch.objects.get(equipment).v4.attached_to,
                    Some(ObjectLinkV4 {
                        object: selected,
                        zone_change_count: 0
                    })
                );
                assert_eq!(
                    legal_targets
                        .iter()
                        .filter(|target| match target {
                            crate::state::Target::Object(id) =>
                                branch.objects.get(*id).attachments.contains(&equipment),
                            _ => false,
                        })
                        .count(),
                    1
                );
                crate::doubling_season_v48::validate_token_journals(&branch).unwrap();
                let saved = branch.clone();
                // Keep the original Input and completed choice; erase only the
                // independent actual relation occurrence without shifting indices.
                let at=branch.engine.event_history.iter().position(|e|matches!(e,event::CommittedEvent::TokenAttachmentJournalV48 {journal} if matches!(journal.as_ref(),JournalV48::AttachmentCommitted {..}))).unwrap();
                branch.engine.event_history[at] = event::CommittedEvent::Tap { object: equipment };
                let before = branch.clone();
                assert!(engine::step(&mut branch, Action::Pass).is_err());
                assert_eq!(branch, before);
                assert!(crate::doubling_season_public_v48::capture(&branch).is_err());
                assert_eq!(
                    serde_json::from_slice::<GameState>(&serde_json::to_vec(&saved).unwrap())
                        .unwrap(),
                    saved
                );
            }
            let mut erased = original.clone();
            erased
                .engine
                .linked_aura_operation_v1
                .pending_token_attachment_v48 = None;
            let before = erased.clone();
            assert!(engine::step(&mut erased, Action::Pass).is_err());
            assert_eq!(erased, before);
            let mut altered = original.clone();
            let checkpoint = altered
                .engine
                .linked_aura_operation_v1
                .pending_token_attachment_v48
                .as_mut()
                .unwrap();
            let GeneralChoiceV36::TokenEquipmentV48 { choice, .. } = &mut checkpoint.choice else {
                unreachable!()
            };
            choice.candidates.swap(0, 1);
            let before = altered.clone();
            assert!(
                engine::step(&mut altered, Action::ChooseEffectTarget(legal_targets[0])).is_err()
            );
            assert_eq!(altered, before);
        }
    }
}
