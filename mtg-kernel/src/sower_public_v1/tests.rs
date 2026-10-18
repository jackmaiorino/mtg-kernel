use super::*;
use crate::policy_observation_v6::tests::put;

#[test]
fn ordinary_public_parts_and_hidden_identity_parity_keep_real_predecessor_fields() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = crate::policy_observation_v6::tests::ready_state_with_turn_authority(actor);
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
    let mut state = crate::policy_observation_v6::tests::ready_state_with_turn_authority(actor);
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
        event::propose_and_commit(&mut state, ProposedEvent::zone_change(source, Zone::Exile));
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
            ProposedEvent::zone_change(recipient, Zone::Exile),
        );
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(recipient, Zone::Battlefield),
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
        let (mut state, _, recipient) = activated(actor);
        let foreign = put(
            &mut state,
            actor.opponent(),
            "Llanowar Elves",
            Zone::Battlefield,
        );
        let surface = PolicySurfaceV5::new();
        let receipt = state
            .engine
            .event_history
            .iter()
            .find(|event| matches!(event, CommittedEvent::SowerRestrictionInstalledV1 { .. }))
            .unwrap()
            .clone();
        let mut cases = Vec::new();
        // Rewriting every successor record and the live marker together must
        // not redirect the original independently committed targeting event.
        let targeted: Vec<_> = state
            .engine
            .event_history
            .iter()
            .filter(|event| matches!(event, CommittedEvent::Targeted { .. }))
            .cloned()
            .collect();
        assert!(targeted.iter().any(|event| matches!(event,
            CommittedEvent::Targeted { target, .. } if *target == recipient)));
        let mut redirected = state.clone();
        let replacement = crate::state::Target::Object(foreign);
        let contract = crate::state::StackTargetContractV4::capture(&redirected, replacement);
        let public_ref = crate::rl::card_ref(&redirected, foreign).unwrap();
        let snapshot = serde_json::to_string(redirected.objects.get(foreign)).unwrap();
        for event in &mut redirected.engine.event_history {
            match event {
                CommittedEvent::SowerRestrictionResolutionV1 {
                    item,
                    recipient_snapshot,
                    ..
                } => {
                    item.targets = vec![replacement];
                    item.v4.target_contracts = vec![contract];
                    *recipient_snapshot = snapshot.clone();
                }
                CommittedEvent::SowerRestrictionInstalledV1 { capture } => {
                    capture.item.targets = vec![replacement];
                    capture.item.v4.target_contracts = vec![contract];
                    capture.recipient = public_ref.clone();
                    capture.recipient_snapshot = snapshot.clone();
                }
                _ => {}
            }
        }
        for effect in &mut redirected.engine.until_end_of_turn {
            if let UntilEndOfTurnEffect::ResolvedObjectCantBlock {
                object_id,
                object_zone_change_count,
                ..
            } = effect
            {
                *object_id = foreign;
                *object_zone_change_count = public_ref.zone_change_count;
            }
        }
        assert_eq!(
            redirected
                .engine
                .event_history
                .iter()
                .filter(|event| matches!(event, CommittedEvent::Targeted { .. }))
                .cloned()
                .collect::<Vec<_>>(),
            targeted
        );
        cases.push(redirected);
        let mut redirected_controller = state.clone();
        for event in &mut redirected_controller.engine.event_history {
            let item = match event {
                CommittedEvent::SowerRestrictionResolutionV1 { item, .. } => item,
                CommittedEvent::SowerRestrictionInstalledV1 { capture } => &mut capture.item,
                _ => continue,
            };
            item.controller = actor.opponent();
            item.v4.ability_source_contract.as_mut().unwrap().controller = actor.opponent();
        }
        cases.push(redirected_controller);
        let mut missing_targeted = state.clone();
        missing_targeted
            .engine
            .event_history
            .retain(|event| !matches!(event, CommittedEvent::Targeted { .. }));
        cases.push(missing_targeted);
        let mut duplicate_targeted = state.clone();
        // Duplicates must precede resolution to test the unique announcement
        // authority, rather than merely appending unrelated later history.
        duplicate_targeted
            .engine
            .event_history
            .insert(0, targeted[0].clone());
        cases.push(duplicate_targeted);
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
            for viewer in [actor, actor.opponent()] {
                assert!(observe_v1(&bad, &surface, viewer, 0, 0, 0, 1).is_err());
                let restored: GameState = serde_json::from_slice(&before).unwrap();
                assert!(observe_v1(&restored, &surface, viewer, 0, 0, 0, 1).is_err());
            }
            assert_eq!(serde_json::to_vec(&bad).unwrap(), before);
            let mut rejected =
                crate::rl_session::FastActorSessionV1::from_sower_v14_fixture_state(bad);
            assert!(matches!(
                rejected.current_response(),
                crate::rl_session::FastActorResponseV1::Terminal(_)
            ));
            // Constructor refusal must precede Cleanup, not repair the corruption
            // by publishing an expiry while automatically advancing priority.
            assert_eq!(
                serde_json::to_vec(rejected.sower_fixture_state_mut()).unwrap(),
                before
            );
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

#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
#[test]
fn genuine_paid_sower_and_frenzied_restrictions_partition_restore_and_refuse_forgeries_both_seats()
{
    use crate::akroma_family_public_v40::FamilyFactsV40;
    use crate::engine::{self, Action, Decision, UntilEndOfTurnEffect};
    use crate::event::CommittedEvent;
    use crate::rl_session::{ElendaSessionV52, FastActorSessionV1};
    use crate::state::Target;
    for actor in [PlayerId::P0, PlayerId::P1] {
        // Existing real attack/target/payment fixture. Printed permanents and
        // its initial floating red are support; no paid Goblin cast is claimed.
        let (mut state, _, recipient) = crate::frenzied_public_v1::tests::actual_payment(actor);
        engine::step(&mut state, Action::ChooseEffectBoolean(true)).unwrap();
        for _ in 0..16 {
            let decision = engine::advance_until_decision(&mut state);
            if state.engine.pending_effect.is_none()
                && state.stack.is_empty()
                && matches!(decision, Decision::CastSpellOrPass { player, .. } if player == actor)
            {
                break;
            }
            assert!(
                matches!(decision, Decision::CastSpellOrPass { .. }),
                "{decision:?}"
            );
            engine::step(&mut state, Action::Pass).unwrap();
        }
        assert_eq!(
            crate::effect::frenzied_goblin_resolution_v1::active(&state)
                .unwrap()
                .len(),
            1
        );
        assert_eq!(state.players[actor.index()].mana_pool, [0; 6]);
        let sower = put(&mut state, actor, "Sower of Chaos", Zone::Battlefield);
        // Existing genuine printing as support; its activation is paid with
        // actual native land abilities, independently of the earlier Goblin.
        let mountain = state.players[actor.index()]
            .battlefield
            .iter()
            .copied()
            .find(|id| {
                crate::card_def::CARD_DEFS[usize::from(state.objects.get(*id).card_def)].name
                    == "Mountain"
            })
            .unwrap();
        let forests = [
            put(&mut state, actor, "Forest", Zone::Battlefield),
            put(&mut state, actor, "Forest", Zone::Battlefield),
        ];
        for land in [mountain, forests[0], forests[1]] {
            engine::step(&mut state, Action::ActivateManaAbility(land)).unwrap();
            assert!(state.objects.get(land).tapped);
            assert!(
                matches!(engine::advance_until_decision(&mut state), Decision::CastSpellOrPass { player, .. } if player == actor)
            );
        }
        engine::step(&mut state, Action::ActivateAbility(sower, 0)).unwrap();
        assert!(matches!(
            engine::advance_until_decision(&mut state),
            Decision::ChooseTargets { .. }
        ));
        engine::step(&mut state, Action::ChooseTarget(Target::Object(recipient))).unwrap();
        // Target selection precedes native activation finalization and payment.
        assert!(matches!(
            engine::advance_until_decision(&mut state),
            Decision::CastSpellOrPass { .. }
        ));
        assert!(state.engine.pending_activation.is_none());
        let [paid] = state.stack.as_slice() else {
            panic!("one genuinely paid Sower activation must be on the stack")
        };
        assert_eq!(paid.source, sower);
        assert_eq!(paid.controller, actor);
        assert_eq!(paid.kind, crate::state::StackItemKind::ActivatedAbility);
        assert_eq!(paid.targets.as_slice(), &[Target::Object(recipient)]);
        assert_eq!(state.players[actor.index()].mana_pool, [0; 6]);
        for _ in 0..16 {
            let decision = engine::advance_until_decision(&mut state);
            if state.stack.is_empty() && state.engine.pending_effect.is_none() {
                break;
            }
            assert!(
                matches!(decision, Decision::CastSpellOrPass { .. }),
                "{decision:?}"
            );
            engine::step(&mut state, Action::Pass).unwrap();
        }
        let complete = crate::sower_restriction_capture_v1::complete_family(&state).unwrap();
        assert_eq!((complete.sower.len(), complete.frenzied.len()), (1, 1));
        assert_eq!(complete.sower[0].recipient.arena_id, recipient.0);
        assert_eq!(complete.frenzied[0].recipient.arena_id, recipient.0);
        let sower_timestamp = complete.sower[0].timestamp;
        let frenzied_timestamp = complete.frenzied[0].timestamp;
        let sower_stack = complete.sower[0].item.v4.stack_item_id;
        assert_ne!(sower_timestamp, frenzied_timestamp);
        assert!(crate::sower_of_chaos_v1::cant_block(&state, recipient));
        let original = serde_json::to_vec(&state).unwrap();
        assert!(crate::sower_restriction_capture_v1::active(&state).is_err());
        assert!(crate::frenzied_public_v1::validate_projection_authority(&state).is_err());
        assert_eq!(serde_json::to_vec(&state).unwrap(), original);
        let restored: GameState = serde_json::from_slice(&original).unwrap();
        let mut prior = None;
        for native in [state.clone(), restored] {
            let mut session = ElendaSessionV52::from_session(
                FastActorSessionV1::from_elenda_v52_fixture_state(native),
            )
            .unwrap();
            let decision = session.decision_v52().unwrap();
            let public = decision.observation();
            let sower_rows = public
                .common
                .families
                .iter()
                .find_map(|f| {
                    if let FamilyFactsV40::Sower(v) = f {
                        Some(&v.restrictions)
                    } else {
                        None
                    }
                })
                .unwrap();
            let frenzied_rows = public
                .common
                .families
                .iter()
                .find_map(|f| {
                    if let FamilyFactsV40::Frenzied(v) = f {
                        Some(&v.restrictions)
                    } else {
                        None
                    }
                })
                .unwrap();
            assert_eq!((sower_rows.len(), frenzied_rows.len()), (1, 1));
            assert_eq!(sower_rows[0].timestamp, sower_timestamp);
            assert_eq!(frenzied_rows[0].timestamp, frenzied_timestamp);
            assert!(sower_rows[0].effective && frenzied_rows[0].effective);
            let before = serde_json::to_vec(session.fixture_state_v52()).unwrap();
            let counts = session.fixture_counts_v52();
            let saved = session.snapshot_v52();
            session.restore_v52(&saved).unwrap();
            let replayed = session.decision_v52().unwrap();
            assert_eq!(replayed.observation(), public);
            assert_eq!(replayed.actions(), decision.actions());
            assert_eq!(
                serde_json::to_vec(session.fixture_state_v52()).unwrap(),
                before
            );
            assert_eq!(session.fixture_counts_v52(), counts);
            if let Some(previous) = &prior {
                assert_eq!(public, previous);
            }
            prior = Some(public.clone());
        }
        for case in 0..6 {
            let mut corrupt = state.clone();
            match case {
                0 => corrupt.engine.until_end_of_turn.retain(|effect| !matches!(effect,
                    UntilEndOfTurnEffect::ResolvedObjectCantBlock { timestamp, .. } if *timestamp == sower_timestamp)),
                1 => {
                    let marker = corrupt.engine.until_end_of_turn.iter().find(|effect| matches!(effect,
                        UntilEndOfTurnEffect::ResolvedObjectCantBlock { timestamp, .. } if *timestamp == frenzied_timestamp)).unwrap().clone();
                    corrupt.engine.until_end_of_turn.push(marker);
                }
                2 => corrupt.engine.until_end_of_turn.push(UntilEndOfTurnEffect::ResolvedObjectCantBlock {
                    object_id: recipient,
                    object_zone_change_count: corrupt.objects.get(recipient).zone_change_count,
                    timestamp: corrupt.engine.next_effect_timestamp,
                    duration: engine::EffectDuration::EndOfTurn,
                }),
                3 => corrupt.engine.event_history.retain(|event| !matches!(event,
                    CommittedEvent::Targeted { targeting_stack_item, .. } if *targeting_stack_item == sower_stack)),
                4 => {
                    let receipt = corrupt.engine.event_history.iter().find(|event| matches!(event,
                        CommittedEvent::SowerRestrictionInstalledV1 { .. })).unwrap().clone();
                    corrupt.engine.event_history.push(receipt);
                }
                _ => {
                    // Both independent original targets remain genuine. Rewrite
                    // only Sower's journal timestamp to alias the Goblin owner.
                    corrupt.engine.until_end_of_turn.retain(|effect| !matches!(effect,
                        UntilEndOfTurnEffect::ResolvedObjectCantBlock { timestamp, .. } if *timestamp == sower_timestamp));
                    for event in &mut corrupt.engine.event_history {
                        match event {
                            CommittedEvent::SowerRestrictionResolutionV1 { timestamp, .. } => *timestamp = frenzied_timestamp,
                            CommittedEvent::SowerRestrictionInstalledV1 { capture } => capture.timestamp = frenzied_timestamp,
                            _ => {}
                        }
                    }
                }
            }
            let before = serde_json::to_vec(&corrupt).unwrap();
            assert!(
                crate::sower_restriction_capture_v1::complete_family(&corrupt).is_err(),
                "forged case {case}"
            );
            assert_eq!(serde_json::to_vec(&corrupt).unwrap(), before);
        }
    }
}
