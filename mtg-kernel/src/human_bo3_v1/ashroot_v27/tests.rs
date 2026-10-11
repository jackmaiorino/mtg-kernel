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
fn expected(session: &HumanAshrootSessionV27) -> FastActorDecisionV1 {
    let response = session.current_response();
    let FastActorResponseV1::Decision(decision) = response else {
        panic!("real decision, observed {response:?}");
    };
    decision
}
fn action(prompt: &HumanDecisionAshrootV27, prefix: &str) -> HumanActionRequestV1 {
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
        let mut session = HumanAshrootSessionV27::from_session(base).unwrap();
        let current = expected(&session);
        let prior = HumanDecisionProjectorV9::new(actor.into())
            .project_current(&old, current)
            .unwrap();
        let mut projector = HumanAshrootProjectorV27::new(actor.into());
        let before = session.inner.privileged_core_environment_hash();
        let prompt = projector.project_current(&session, current).unwrap();
        safe(&serde_json::to_value(&prompt).unwrap());
        assert_eq!(prompt.actions, prior.actions);
        assert_eq!(prompt.state.public, prior.state.public);
        assert_eq!(prompt.state.extensions.predecessor, prior.state.extensions);
        assert!(prompt.state.extensions.power_contexts.is_empty());
        assert_eq!(
            projector.project_current(&session, current).unwrap(),
            prompt
        );
        assert_eq!(session.inner.privileged_core_environment_hash(), before);
        let hidden = HumanAshrootSessionV27::from_session(
            FastActorSessionV1::from_human_v8_fixture_state(basic(actor, "Island")),
        )
        .unwrap();
        assert_eq!(
            HumanAshrootProjectorV27::new(actor.into())
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
        let mut session = HumanAshrootSessionV27::from_session(
            FastActorSessionV1::from_human_v8_fixture_state(basic(actor, "Lightning Bolt")),
        )
        .unwrap();
        let mut projector = HumanAshrootProjectorV27::new(actor.into());
        let first = projector
            .project_current(&session, expected(&session))
            .unwrap();
        let snapshot = session.snapshot_v27();
        let paid = action(&first, "Play Forest");
        projector.submit(&mut session, paid.clone()).unwrap();
        let future = projector
            .project_current(&session, expected(&session))
            .unwrap();
        let future_request = action(&future, "Pass priority");
        session.restore_v27(&snapshot).unwrap();
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
        let foreign = HumanAshrootSessionV27::from_session(
            FastActorSessionV1::from_human_v8_fixture_state(basic(actor, "Island")),
        )
        .unwrap();
        assert_eq!(
            session.restore_v27(&foreign.snapshot_v27()),
            Err(Error::StaleDecision)
        );
        session.authority.generation = u64::MAX;
        assert_eq!(
            session.restore_v27(&snapshot),
            Err(Error::PromptSequenceExhausted)
        );
        assert_eq!(session.inner.privileged_core_environment_hash(), before);
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
    use crate::ids::ObjectId;
    use crate::mana::ManaColor;
    use crate::state::{GameObject, GameState, ObjectStateV4, Step};
    fn put(state: &mut GameState, owner: PlayerId, name: &str, zone: Zone) -> ObjectId {
        let def = card_id_by_name(name).expect("true future definition registration is required");
        let id = state.objects.push(GameObject {
            card_def: def,
            name: CARD_DEFS[usize::from(def)].object_name.into(),
            owner,
            controller: owner,
            zone,
            tapped: false,
            summoning_sick: false,
            damage: 0,
            counters: Default::default(),
            attachments: vec![],
            v4: ObjectStateV4::from_card_def(def),
            spell_copy_origin: None,
            plotted_turn: None,
            zone_change_count: 0,
        });
        match zone {
            Zone::Hand => state.players[owner.index()].hand.push(id),
            Zone::Battlefield => state.players[owner.index()].battlefield.push(id),
            _ => panic!("unsupported fixture placement"),
        }
        id
    }
    fn ready(actor: PlayerId) -> GameState {
        let forest = card_id_by_name("Forest").unwrap();
        let mut state = GameState::new_from_libraries_with_starting_player_v1(
            &[forest; 40],
            &[forest; 40],
            |_| "Forest".into(),
            1127,
            actor,
        );
        state.step = Step::Main1;
        for player in [PlayerId::P0, PlayerId::P1] {
            put(&mut state, player, "Mountain", Zone::Battlefield);
            put(&mut state, player, "Lightning Bolt", Zone::Hand);
        }
        state
    }
    fn next(state: &mut GameState) -> Decision {
        let d = engine::advance_until_decision(state);
        assert!(!matches!(d, Decision::Halted { .. }), "{d:?}");
        d
    }
    fn priority(state: &mut GameState, actor: PlayerId) {
        for _ in 0..4 {
            let d = next(state);
            if matches!(d, Decision::CastSpellOrPass { player, .. } if player == actor) {
                return;
            }
            assert!(matches!(d, Decision::CastSpellOrPass { .. }), "{d:?}");
            engine::step(state, Action::Pass).unwrap();
        }
        panic!("missing priority");
    }
    fn finish(state: &mut GameState) {
        for _ in 0..30 {
            let d = next(state);
            if state.stack.is_empty()
                && state.engine.pending_triggers.is_empty()
                && matches!(d, Decision::CastSpellOrPass { .. })
            {
                return;
            }
            assert!(matches!(d, Decision::CastSpellOrPass { .. }), "{d:?}");
            engine::step(state, Action::Pass).unwrap();
        }
        panic!("resolution did not complete");
    }

    fn restore(state: &GameState) -> GameState {
        let bytes = serde_json::to_vec(state).unwrap();
        let value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(serde_json::to_vec(&value).unwrap(), bytes);
        value
    }
    fn source(state: &mut GameState, actor: PlayerId) -> ObjectId {
        let id = put(state, actor, "Ashroot Animist", Zone::Hand);
        state.players[actor.index()].mana_pool[ManaColor::R.pool_index()] = 3;
        state.players[actor.index()].mana_pool[ManaColor::G.pool_index()] = 1;
        priority(state, actor);
        engine::step(state, Action::CastSpell(id)).unwrap();
        finish(state);
        assert_eq!(state.objects.get(id).zone, Zone::Battlefield);
        assert!(state.objects.get(id).summoning_sick);
        // Physical maturity setup; this fixture independently verifies actual casting.
        state.objects.get_mut(id).summoning_sick = false;
        id
    }
    fn declare_attack(state: &mut GameState, source: ObjectId) {
        for _ in 0..30 {
            let d = next(state);
            match d {
                Decision::DeclareAttackers { eligible, .. } => {
                    assert!(eligible.contains(&source));
                    break;
                }
                Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
                _ => panic!("{d:?}"),
            }
        }
        engine::step(state, Action::DeclareAttackers(vec![source])).unwrap();
    }

    #[test]
    #[ignore = "requires genuine root-owned Ashroot registration; source-only fixture"]
    fn actual_cast_attack_human_target_submit_retry_and_owning_restore() {
        use crate::human_bo3_v1::{
            HumanActionRequestV1, HumanAshrootProjectorV27, HumanAshrootSessionV27,
            HumanDecisionErrorV1,
        };
        use crate::rl_session::{FastActorResponseV1, FastActorSessionV1};
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut state = ready(actor);
            let source = source(&mut state, actor);
            put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
            declare_attack(&mut state, source);
            let base = FastActorSessionV1::from_ashroot_v27_fixture_state(restore(&state));
            let mut session = HumanAshrootSessionV27::from_session(base).unwrap();
            let FastActorResponseV1::Decision(expected) = session.current_response() else {
                panic!("real target decision")
            };
            let mut projector = HumanAshrootProjectorV27::new(actor.into());
            let prompt = projector.project_current(&session, expected).unwrap();
            assert_eq!(prompt.state.extensions.power_contexts.len(), 1);
            assert_eq!(
                prompt.state.extensions.power_contexts[0].power_if_resolved_now,
                4
            );
            let selected = prompt
                .actions
                .iter()
                .find(|row| row.label.starts_with("Target Llanowar Elves "))
                .unwrap();
            assert!(selected
                .label
                .contains("X sampled at resolution (currently 4)"));
            let request = HumanActionRequestV1 {
                prompt_seq: prompt.prompt_seq,
                action_index: selected.action_index,
            };
            let saved = session.snapshot_v27();
            let receipt = projector.submit(&mut session, request.clone()).unwrap();
            assert_eq!(
                projector.submit(&mut session, request.clone()).unwrap(),
                receipt
            );
            session.restore_v27(&saved).unwrap();
            assert_eq!(
                projector.submit(&mut session, request),
                Err(HumanDecisionErrorV1::StaleDecision)
            );
            let FastActorResponseV1::Decision(expected) = session.current_response() else {
                panic!("restored real target decision")
            };
            let refreshed = projector.project_current(&session, expected).unwrap();
            assert!(refreshed.prompt_seq > prompt.prompt_seq);
            assert_eq!(refreshed.actions, prompt.actions);
        }
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
        let mut session = HumanAshrootSessionV27::from_session(base).unwrap();
        let e = expected(&session);
        let old = HumanDecisionProjectorV9::new(e.acting_player)
            .project_current(&previous, e)
            .unwrap();
        let mut projector = HumanAshrootProjectorV27::new(e.acting_player);
        let prompt = projector.project_current(&session, e).unwrap();
        assert_eq!(prompt.state.public, old.state.public);
        assert_eq!(prompt.state.extensions.predecessor, old.state.extensions);
        assert_eq!(prompt.actions, old.actions);
        safe(&serde_json::to_value(&prompt).unwrap());
        let prefix = if pay { "Pay" } else { "Decline to pay" };
        let input = action(&prompt, prefix);
        let snapshot = session.snapshot_v27();
        let receipt = projector.submit(&mut session, input.clone()).unwrap();
        let after = session.inner.privileged_core_environment_hash();
        assert_eq!(
            projector.submit(&mut session, input.clone()).unwrap(),
            receipt
        );
        assert_eq!(session.inner.privileged_core_environment_hash(), after);
        session.restore_v27(&snapshot).unwrap();
        let mut replay = HumanAshrootProjectorV27::new(e.acting_player);
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
