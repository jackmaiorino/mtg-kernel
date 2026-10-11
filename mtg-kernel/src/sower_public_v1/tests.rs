use super::*;
use crate::policy_observation_v6::tests::{put, ready_state};

#[test]
fn ordinary_public_parts_and_hidden_identity_parity_keep_real_predecessor_fields() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready_state();
        state.active_player = actor;
        state.priority_player = actor;
        put(&mut state, actor, "Forest", Zone::Hand);
        let hidden = put(&mut state, actor.opponent(), "Lightning Bolt", Zone::Hand);
        let surface = PolicySurfaceV5::new();
        let old = crate::rl::observe_policy_v8(&state, &surface, actor, 0, 0, 0, 1).unwrap();
        let public = observe_v1(&state, &surface, actor, 0, 0, 0, 1).unwrap();
        public.validate_artifact().unwrap();
        assert_eq!(public.projection, old.projection);
        assert_eq!(public.extensions, old.extensions);
        assert_eq!(public.known_face_down_cards, old.known_face_down_cards);
        assert!(public.restrictions.is_empty());
        let definition = crate::card_def::card_id_by_name("Island").unwrap();
        let object = state.objects.get_mut(hidden);
        object.card_def = definition;
        object.name = "Island".into();
        object.v4 = crate::state::ObjectStateV4::from_card_def(definition);
        assert_eq!(
            observe_v1(&state, &surface, actor, 0, 0, 0, 1).unwrap(),
            public
        );
        let mut bad = public.clone();
        bad.schema = "mtg-kernel-policy-observation/v9".into();
        assert!(bad.validate_artifact().is_err());
        let mut bad = public;
        bad.producer_sha256.clear();
        assert!(bad.validate_artifact().is_err());
        let mut bad = observe_v1(&state, &surface, actor, 0, 0, 0, 1).unwrap();
        bad.roots.push(RootV1 {
            card_id: crate::card_def::card_id_by_name("Forest").unwrap(),
            ability_index: 0,
            root_sha256: String::new(),
        });
        bad.projection_sha256 = commitment(&bad).unwrap();
        assert!(bad.validate_artifact().is_err());
    }
}

#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
pub(crate) fn activated(actor: PlayerId) -> (GameState, ObjectId, ObjectId) {
    activated_target(actor, "Llanowar Elves")
}

#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
fn activated_target(actor: PlayerId, name: &str) -> (GameState, ObjectId, ObjectId) {
    use crate::engine::{self, Action, Decision};
    use crate::mana::ManaColor;
    use crate::state::Target;
    let mut state = ready_state();
    state.active_player = actor;
    state.priority_player = actor;
    let source = put(&mut state, actor, "Sower of Chaos", Zone::Battlefield);
    let recipient = put(&mut state, actor.opponent(), name, Zone::Battlefield);
    put(&mut state, actor, "Forest", Zone::Hand);
    for owner in [PlayerId::P0, PlayerId::P1] {
        for _ in 0..4 {
            put(&mut state, owner, "Forest", Zone::Library);
        }
    }
    state.players[actor.index()].mana_pool[ManaColor::R.pool_index()] = 3;
    assert!(
        matches!(engine::advance_until_decision(&mut state), Decision::CastSpellOrPass { player, .. } if player == actor)
    );
    engine::step(&mut state, Action::ActivateAbility(source, 0)).unwrap();
    assert!(matches!(
        engine::advance_until_decision(&mut state),
        Decision::ChooseTargets { .. }
    ));
    engine::step(&mut state, Action::ChooseTarget(Target::Object(recipient))).unwrap();
    assert!(matches!(
        engine::advance_until_decision(&mut state),
        Decision::CastSpellOrPass { .. }
    ));
    assert_eq!(state.stack.len(), 1);
    assert_eq!(state.players[actor.index()].mana_pool, [0; 6]);
    for _ in 0..8 {
        let decision = engine::advance_until_decision(&mut state);
        assert!(
            matches!(decision, Decision::CastSpellOrPass { .. }),
            "{decision:?}"
        );
        if state.stack.is_empty() {
            break;
        }
        engine::step(&mut state, Action::Pass).unwrap();
    }
    assert!(state.stack.is_empty());
    assert_eq!(
        crate::sower_restriction_capture_v1::active(&state)
            .unwrap()
            .len(),
        1
    );
    (state, source, recipient)
}

#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
#[test]
fn actual_registered_installation_current_control_departure_restore_and_cleanup() {
    use crate::engine::{self, Action, Decision};
    use crate::event::{self, ProposedEvent};
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, source, recipient) = activated(actor);
        let surface = PolicySurfaceV5::new();
        for viewer in [actor, actor.opponent()] {
            let public = observe_v1(&state, &surface, viewer, 0, 0, 0, 1).unwrap();
            assert_eq!(public.restrictions.len(), 1);
            assert!(public.restrictions[0].effective);
            assert_eq!(public.restrictions[0].recipient.arena_id, recipient.0);
            assert!(crate::rl::observe_policy_v8(&state, &surface, viewer, 0, 0, 0, 1).is_err());
            assert!(crate::policy_observation_v9::observe_policy_v9(
                &state, &surface, viewer, 0, 0, 0, 1
            )
            .is_err());
            let bytes = serde_json::to_string(&public).unwrap();
            assert!(!bytes.contains("recipient_snapshot"));
            assert!(!bytes.contains("SowerRestrictionInstalled"));
        }
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(source, Zone::Battlefield, Zone::Exile),
        );
        state.players[actor.opponent().index()]
            .battlefield
            .retain(|id| *id != recipient);
        state.players[actor.index()].battlefield.push(recipient);
        state.objects.get_mut(recipient).controller = actor;
        let public = observe_v1(&state, &surface, actor, 0, 0, 0, 1).unwrap();
        assert!(public.restrictions[0].effective);
        assert_eq!(
            public.restrictions[0].recipient.controller,
            actor.opponent().into()
        );
        assert_eq!(
            public.restrictions[0]
                .current_recipient
                .as_ref()
                .unwrap()
                .controller,
            actor.into()
        );
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(recipient, Zone::Battlefield, Zone::Exile),
        );
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(recipient, Zone::Exile, Zone::Battlefield),
        );
        let public = observe_v1(&state, &surface, actor, 0, 0, 0, 1).unwrap();
        assert!(!public.restrictions[0].effective);
        assert!(public.restrictions[0].current_recipient.is_none());
        let restored: GameState =
            serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
        assert_eq!(
            observe_v1(&restored, &surface, actor, 0, 0, 0, 1).unwrap(),
            public
        );
        state.step = crate::state::Step::End;
        state.engine.priority_passes = [false; 2];
        for _ in 0..16 {
            let decision = engine::advance_until_decision(&mut state);
            if state.active_player == actor.opponent() {
                break;
            }
            assert!(
                matches!(decision, Decision::CastSpellOrPass { .. }),
                "{decision:?}"
            );
            engine::step(&mut state, Action::Pass).unwrap();
        }
        assert_eq!(state.active_player, actor.opponent());
        assert!(crate::sower_restriction_capture_v1::active(&state)
            .unwrap()
            .is_empty());
        assert!(observe_v1(&state, &surface, actor, 0, 0, 0, 1)
            .unwrap()
            .restrictions
            .is_empty());
    }
}

#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
#[test]
fn missing_duplicate_coherent_capture_and_expired_reinsertion_refuse_readonly() {
    use crate::engine::UntilEndOfTurnEffect;
    use crate::event::CommittedEvent;
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (state, _, _) = activated(actor);
        let surface = PolicySurfaceV5::new();
        let receipt = state
            .engine
            .event_history
            .iter()
            .find(|event| matches!(event, CommittedEvent::SowerRestrictionInstalledV1 { .. }))
            .unwrap()
            .clone();
        let mut cases = Vec::new();
        let mut bad = state.clone();
        bad.engine.until_end_of_turn.clear();
        cases.push(bad);
        let mut bad = state.clone();
        bad.engine
            .event_history
            .retain(|event| !matches!(event, CommittedEvent::SowerRestrictionInstalledV1 { .. }));
        cases.push(bad);
        let mut bad = state.clone();
        bad.engine.event_history.push(receipt.clone());
        cases.push(bad);
        let mut bad = state.clone();
        bad.engine.event_log.push(receipt.clone());
        cases.push(bad);
        let mut bad = state.clone();
        for event in &mut bad.engine.event_history {
            if let CommittedEvent::SowerRestrictionInstalledV1 { capture } = event {
                capture.recipient.controller = actor.into();
            }
        }
        cases.push(bad);
        let mut bad = state.clone();
        for event in &mut bad.engine.event_history {
            if let CommittedEvent::SowerRestrictionInstalledV1 { capture } = event {
                capture.recipient.controller = actor.into();
                let mut snapshot: crate::state::GameObject =
                    serde_json::from_str(&capture.recipient_snapshot).unwrap();
                snapshot.controller = actor;
                capture.recipient_snapshot = serde_json::to_string(&snapshot).unwrap();
            }
        }
        cases.push(bad);
        let mut bad = state.clone();
        for event in &mut bad.engine.event_history {
            if let CommittedEvent::SowerRestrictionInstalledV1 { capture } = event {
                capture.recipient.card_db_id = crate::card_def::card_id_by_name("Forest").unwrap();
            }
        }
        cases.push(bad);
        let mut bad = state.clone();
        for event in &mut bad.engine.event_history {
            if let CommittedEvent::SowerRestrictionInstalledV1 { capture } = event {
                capture.timestamp += 1;
            }
        }
        for effect in &mut bad.engine.until_end_of_turn {
            if let UntilEndOfTurnEffect::ResolvedObjectCantBlock { timestamp, .. } = effect {
                *timestamp += 1;
            }
        }
        bad.engine.next_effect_timestamp += 1;
        cases.push(bad);
        let mut bad = state.clone();
        bad.engine
            .event_history
            .retain(|event| !matches!(event, CommittedEvent::SowerRestrictionResolutionV1 { .. }));
        cases.push(bad);
        let mut bad = state.clone();
        for effect in &mut bad.engine.until_end_of_turn {
            if let UntilEndOfTurnEffect::ResolvedObjectCantBlock { timestamp, .. } = effect {
                *timestamp += 1;
            }
        }
        cases.push(bad);
        let mut expired = state.clone();
        crate::sower_restriction_capture_v1::expire(&mut expired);
        expired.engine.until_end_of_turn.clear();
        assert!(observe_v1(&expired, &surface, actor, 0, 0, 0, 1)
            .unwrap()
            .restrictions
            .is_empty());
        expired.engine.until_end_of_turn = state.engine.until_end_of_turn.clone();
        cases.push(expired);
        for bad in cases {
            let before = serde_json::to_vec(&bad).unwrap();
            assert!(observe_v1(&bad, &surface, actor, 0, 0, 0, 1).is_err());
            assert_eq!(serde_json::to_vec(&bad).unwrap(), before);
            assert!(matches!(
                crate::rl_session::FastActorSessionV1::from_sower_v14_fixture_state(bad)
                    .current_response(),
                crate::rl_session::FastActorResponseV1::Terminal(_)
            ));
        }
    }
}

#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
#[test]
fn actual_witness_on_source_and_recipient_does_not_remove_resolved_rule() {
    use crate::engine::{self, Action, Decision};
    use crate::mana::ManaColor;
    use crate::state::Target;
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, source, recipient) = activated(actor);
        for target in [source, recipient] {
            let aura = put(&mut state, actor, "Witness Protection", Zone::Hand);
            state.players[actor.index()].mana_pool[ManaColor::U.pool_index()] = 1;
            for _ in 0..4 {
                let decision = engine::advance_until_decision(&mut state);
                if matches!(decision, Decision::CastSpellOrPass { player, .. } if player == actor) {
                    break;
                }
                assert!(
                    matches!(decision, Decision::CastSpellOrPass { .. }),
                    "{decision:?}"
                );
                engine::step(&mut state, Action::Pass).unwrap();
            }
            assert!(
                matches!(engine::advance_until_decision(&mut state), Decision::CastSpellOrPass { player, .. } if player == actor)
            );
            engine::step(&mut state, Action::CastSpell(aura)).unwrap();
            assert!(matches!(
                engine::advance_until_decision(&mut state),
                Decision::ChooseTargets { .. }
            ));
            engine::step(&mut state, Action::ChooseTarget(Target::Object(target))).unwrap();
            for _ in 0..12 {
                let decision = engine::advance_until_decision(&mut state);
                assert!(
                    matches!(decision, Decision::CastSpellOrPass { .. }),
                    "{decision:?}"
                );
                if state.stack.is_empty() {
                    break;
                }
                engine::step(&mut state, Action::Pass).unwrap();
            }
            assert_eq!(state.objects.get(aura).zone, Zone::Battlefield);
            assert!(state.stack.is_empty());
            let restored: GameState =
                serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
            for viewer in [actor, actor.opponent()] {
                let public =
                    observe_v1(&state, &PolicySurfaceV5::new(), viewer, 0, 0, 0, 1).unwrap();
                assert_eq!(public.restrictions.len(), 1);
                assert!(public.restrictions[0].effective);
                assert_eq!(
                    observe_v1(&restored, &PolicySurfaceV5::new(), viewer, 0, 0, 0, 1).unwrap(),
                    public
                );
            }
            assert!(crate::sower_of_chaos_v1::cant_block(&state, recipient));
        }
    }
}
