use super::*;
use crate::ids::PlayerId;
use crate::policy_observation_v6::tests::{put, ready_state};
use crate::state::Zone;

fn basic(actor: PlayerId, hidden: &str) -> crate::state::GameState {
    let mut state = ready_state();
    state.active_player = actor;
    state.priority_player = actor;
    put(&mut state, actor, "Forest", Zone::Hand);
    put(&mut state, actor.opponent(), hidden, Zone::Hand);
    state
}
#[track_caller]
fn expected(session: &HumanStromkirkSessionV28) -> FastActorDecisionV1 {
    let response = session.current_response();
    let FastActorResponseV1::Decision(decision) = response else {
        panic!("real decision, observed {response:?}");
    };
    decision
}
fn action(prompt: &HumanDecisionStromkirkV28, prefix: &str) -> HumanActionRequestV1 {
    let row = prompt
        .actions
        .iter()
        .find(|row| row.label.starts_with(prefix))
        .unwrap();
    HumanActionRequestV1 {
        prompt_seq: prompt.prompt_seq,
        action_index: row.action_index,
    }
}
fn safe(value: &serde_json::Value) {
    match value {
        serde_json::Value::Object(fields) => {
            for (key, child) in fields {
                assert!(!matches!(
                    key.as_str(),
                    "arena_id"
                        | "zone_change_count"
                        | "event_history"
                        | "event_index"
                        | "card_db_id"
                        | "binding"
                        | "engine_indexes"
                        | "projection_sha256"
                        | "producer_sha256"
                ));
                safe(child);
            }
        }
        serde_json::Value::Array(rows) => {
            for row in rows {
                safe(row);
            }
        }
        _ => {}
    }
}
#[test]
fn registered_ordinary_predecessor_menu_privacy_submit_and_retry_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let base = FastActorSessionV1::from_human_v8_fixture_state(basic(actor, "Lightning Bolt"));
        let old = HumanRaidSessionV9::from_session(base.clone()).unwrap();
        let mut session = HumanStromkirkSessionV28::from_session(base).unwrap();
        let current = expected(&session);
        let prior = HumanDecisionProjectorV9::new(actor.into())
            .project_current(&old, current)
            .unwrap();
        let mut projector = HumanStromkirkProjectorV28::new(actor.into());
        let before = session.inner.privileged_core_environment_hash();
        let prompt = projector.project_current(&session, current).unwrap();
        safe(&serde_json::to_value(&prompt).unwrap());
        assert_eq!(prompt.actions, prior.actions);
        assert_eq!(prompt.state.public, prior.state.public);
        assert_eq!(prompt.state.extensions.predecessor, prior.state.extensions);
        assert!(prompt.state.extensions.loss_contexts.is_empty());
        assert_eq!(
            projector.project_current(&session, current).unwrap(),
            prompt
        );
        assert_eq!(session.inner.privileged_core_environment_hash(), before);
        let hidden = HumanStromkirkSessionV28::from_session(
            FastActorSessionV1::from_human_v8_fixture_state(basic(actor, "Island")),
        )
        .unwrap();
        assert_eq!(
            HumanStromkirkProjectorV28::new(actor.into())
                .project_current(&hidden, expected(&hidden))
                .unwrap(),
            prompt
        );
        let request = action(&prompt, "Play Forest");
        let receipt = projector.submit(&mut session, request.clone()).unwrap();
        let after = session.inner.privileged_core_environment_hash();
        assert_eq!(projector.submit(&mut session, request).unwrap(), receipt);
        assert_eq!(session.inner.privileged_core_environment_hash(), after);
    }
}
#[test]
fn registered_owning_restore_foreign_owner_generation_and_old_future_requests_refuse_readonly() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut session = HumanStromkirkSessionV28::from_session(
            FastActorSessionV1::from_human_v8_fixture_state(basic(actor, "Lightning Bolt")),
        )
        .unwrap();
        let mut projector = HumanStromkirkProjectorV28::new(actor.into());
        let first = projector
            .project_current(&session, expected(&session))
            .unwrap();
        let snapshot = session.snapshot_v28();
        let paid = action(&first, "Play Forest");
        projector.submit(&mut session, paid.clone()).unwrap();
        let future = projector
            .project_current(&session, expected(&session))
            .unwrap();
        let future_request = action(&future, "Pass priority");
        session.restore_v28(&snapshot).unwrap();
        let before = session.inner.privileged_core_environment_hash();
        assert_eq!(
            projector.submit(&mut session, paid),
            Err(Error::StaleDecision)
        );
        assert_eq!(
            projector.submit(&mut session, future_request),
            Err(Error::StaleDecision)
        );
        assert_eq!(session.inner.privileged_core_environment_hash(), before);
        let refreshed = projector
            .project_current(&session, expected(&session))
            .unwrap();
        assert!(refreshed.prompt_seq > future.prompt_seq);
        let foreign = HumanStromkirkSessionV28::from_session(
            FastActorSessionV1::from_human_v8_fixture_state(basic(actor, "Island")),
        )
        .unwrap();
        assert_eq!(
            session.restore_v28(&foreign.snapshot_v28()),
            Err(Error::StaleDecision)
        );
        session.authority.generation = u64::MAX;
        assert_eq!(
            session.restore_v28(&snapshot),
            Err(Error::PromptSequenceExhausted)
        );
        assert_eq!(session.inner.privileged_core_environment_hash(), before);
    }
}

fn ward_state(actor: PlayerId) -> crate::state::GameState {
    use crate::engine::{self, Action, Decision};
    use crate::mana::ManaColor;
    use crate::state::Target;
    let mut state = ready_state();
    state.active_player = actor;
    state.priority_player = actor;
    let terror = put(
        &mut state,
        actor.opponent(),
        "Tolarian Terror",
        Zone::Battlefield,
    );
    let spells =
        ["Lightning Bolt", "Lightning Bolt"].map(|name| put(&mut state, actor, name, Zone::Hand));
    state.players[actor.index()].mana_pool[ManaColor::R.pool_index()] = 8;
    for spell in spells {
        engine::step(&mut state, Action::CastSpell(spell)).unwrap();
        engine::step(&mut state, Action::ChooseTarget(Target::Object(terror))).unwrap();
        assert!(matches!(
            engine::advance_until_decision(&mut state),
            Decision::CastSpellOrPass { .. }
        ));
    }
    state
}
#[test]
fn inherited_registered_generic_ward_menu_is_preserved_and_submission_replays() {
    for (actor, pay) in [PlayerId::P0, PlayerId::P1]
        .into_iter()
        .flat_map(|actor| [false, true].map(|pay| (actor, pay)))
    {
        let mut state = ward_state(actor);
        crate::policy_observation_v6::tests::reach_ward_payment(&mut state);
        let base = FastActorSessionV1::from_human_v8_fixture_state(state);
        let previous = HumanRaidSessionV9::from_session(base.clone()).unwrap();
        let mut session = HumanStromkirkSessionV28::from_session(base).unwrap();
        let e = expected(&session);
        let old = HumanDecisionProjectorV9::new(e.acting_player)
            .project_current(&previous, e)
            .unwrap();
        let mut projector = HumanStromkirkProjectorV28::new(e.acting_player);
        let prompt = projector.project_current(&session, e).unwrap();
        assert_eq!(prompt.state.public, old.state.public);
        assert_eq!(prompt.state.extensions.predecessor, old.state.extensions);
        assert_eq!(prompt.actions, old.actions);
        safe(&serde_json::to_value(&prompt).unwrap());
        let prefix = if pay { "Pay" } else { "Decline to pay" };
        let input = action(&prompt, prefix);
        let snapshot = session.snapshot_v28();
        let receipt = projector.submit(&mut session, input.clone()).unwrap();
        let after = session.inner.privileged_core_environment_hash();
        assert_eq!(
            projector.submit(&mut session, input.clone()).unwrap(),
            receipt
        );
        assert_eq!(session.inner.privileged_core_environment_hash(), after);
        session.restore_v28(&snapshot).unwrap();
        let mut replay = HumanStromkirkProjectorV28::new(e.acting_player);
        assert_eq!(
            replay
                .project_current(&session, expected(&session))
                .unwrap(),
            prompt
        );
        replay.submit(&mut session, input).unwrap();
        assert_eq!(session.inner.privileged_core_environment_hash(), after);
    }
}

#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
mod future_printed {
    use super::*;
    use crate::card_def::{card_id_by_name, CARD_DEFS};
    use crate::engine::{self, Action, Decision};
    use crate::event::{self, ProposedEvent};
    use crate::mana::ManaColor;
    use crate::state::{GameState, Step};
    #[test]
    fn actual_cast_endstep_public_human_target_binding_restore_and_hidden_privacy() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut cards = vec![card_id_by_name("Forest").unwrap(); 40];
            cards[0] = card_id_by_name("Stromkirk Bloodthief").expect("true printed registration");
            let mut state = GameState::new_from_libraries_with_starting_player_v1(
                &cards,
                &cards,
                |id| CARD_DEFS[id as usize].name.into(),
                28,
                actor,
            );
            state.step = Step::Main2;
            state.priority_player = actor;
            put(&mut state, actor, "Mountain", Zone::Battlefield);
            put(&mut state, actor, "Lightning Bolt", Zone::Hand);
            put(&mut state, actor.opponent(), "Mountain", Zone::Battlefield);
            let hidden = put(&mut state, actor.opponent(), "Island", Zone::Hand);
            let source = put(&mut state, actor, "Stromkirk Bloodthief", Zone::Hand);
            state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 2;
            state.players[actor.index()].mana_pool[ManaColor::B.pool_index()] = 1;
            assert!(
                matches!(engine::advance_until_decision(&mut state), Decision::CastSpellOrPass { player, castable_spells, .. }
                if player == actor && castable_spells.contains(&source))
            );
            engine::step(&mut state, Action::CastSpell(source)).unwrap();
            assert!(matches!(
                engine::advance_until_decision(&mut state),
                Decision::CastSpellOrPass { .. }
            ));
            assert!(state.stack.iter().any(|i| i.source == source));
            for attempt in 0..40 {
                let decision = engine::advance_until_decision(&mut state);
                if state.stack.is_empty() {
                    break;
                }
                assert!(
                    matches!(decision, Decision::CastSpellOrPass { .. }),
                    "{decision:?}"
                );
                engine::step(&mut state, Action::Pass).unwrap();
                assert!(attempt < 39);
            }
            assert_eq!(state.objects.get(source).zone, Zone::Battlefield);
            event::propose_and_commit(&mut state, ProposedEvent::life_payment(actor.opponent(), 1));
            for attempt in 0..40 {
                let decision = engine::advance_until_decision(&mut state);
                if matches!(decision, Decision::ChooseTargets { .. }) {
                    break;
                }
                assert!(
                    matches!(decision, Decision::CastSpellOrPass { .. }),
                    "{decision:?}"
                );
                engine::step(&mut state, Action::Pass).unwrap();
                assert!(attempt < 39);
            }
            assert_eq!(state.engine.pending_triggers.len(), 1);
            let mut session = HumanStromkirkSessionV28::from_session(
                FastActorSessionV1::from_stromkirk_v28_fixture_state(state.clone()),
            )
            .unwrap();
            let current = expected(&session);
            let public = session.public_observation(current).unwrap();
            assert_eq!(public.roots.len(), 1);
            assert_eq!(public.loss_contexts.len(), 1);
            assert!(public.loss_contexts[0].opponent_lost_life_if_resolved_now);
            let mut projector = HumanStromkirkProjectorV28::new(actor.into());
            let prompt = projector.project_current(&session, current).unwrap();
            safe(&serde_json::to_value(&prompt).unwrap());
            assert_eq!(
                prompt.state.extensions.opponent_lost_life_this_turn,
                Some(true)
            );
            assert_eq!(prompt.state.extensions.own_lost_life_this_turn, Some(false));
            assert!(prompt
                .actions
                .iter()
                .all(|row| row.label.contains("Vampire permanent you control")));
            // Substitute only an unreferenced hidden hand identity, preserving counts.
            let hidden_def = card_id_by_name("Lightning Bolt").unwrap();
            let hidden_object = state.objects.get_mut(hidden);
            hidden_object.card_def = hidden_def;
            hidden_object.name = "Lightning Bolt".into();
            hidden_object.v4 = crate::state::ObjectStateV4::from_card_def(hidden_def);
            let hidden_session = HumanStromkirkSessionV28::from_session(
                FastActorSessionV1::from_stromkirk_v28_fixture_state(state),
            )
            .unwrap();
            let hidden_prompt = HumanStromkirkProjectorV28::new(actor.into())
                .project_current(&hidden_session, expected(&hidden_session))
                .unwrap();
            assert_eq!(
                prompt, hidden_prompt,
                "unrevealed additional opposing card {hidden:?}"
            );
            let snapshot = session.snapshot_v28();
            let request = HumanActionRequestV1 {
                prompt_seq: prompt.prompt_seq,
                action_index: prompt.actions[0].action_index,
            };
            let receipt = projector.submit(&mut session, request.clone()).unwrap();
            assert_eq!(
                projector.submit(&mut session, request.clone()).unwrap(),
                receipt
            );
            session.restore_v28(&snapshot).unwrap();
            let hash = session.inner.privileged_core_environment_hash();
            assert_eq!(
                projector.submit(&mut session, request),
                Err(Error::StaleDecision)
            );
            assert_eq!(session.inner.privileged_core_environment_hash(), hash);
            let refreshed = projector
                .project_current(&session, expected(&session))
                .unwrap();
            projector
                .submit(
                    &mut session,
                    HumanActionRequestV1 {
                        prompt_seq: refreshed.prompt_seq,
                        action_index: refreshed.actions[0].action_index,
                    },
                )
                .unwrap();
            for attempt in 0..40 {
                let response = session.current_response();
                let FastActorResponseV1::Decision(now) = response else {
                    panic!("{response:?}");
                };
                if session.inner.stromkirk_fixture_state_mut().stack.is_empty() {
                    break;
                }
                let seat = now.acting_player;
                let mut acting = HumanStromkirkProjectorV28::new(seat);
                let window = acting.project_current(&session, now).unwrap();
                let pass = action(&window, "Pass priority");
                acting.submit(&mut session, pass).unwrap();
                assert!(attempt < 39);
            }
            assert_eq!(
                session
                    .inner
                    .stromkirk_fixture_state_mut()
                    .objects
                    .get(source)
                    .counters
                    .plus1_plus1,
                1
            );
        }
    }
}
