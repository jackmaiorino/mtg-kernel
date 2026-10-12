use super::*;
#[test]
fn empty_registered_path_preserves_serde_hash_and_event_history_both_seats() {
    use std::hash::{Hash, Hasher};
    for actor in [crate::ids::PlayerId::P0, crate::ids::PlayerId::P1] {
        let mut state = crate::policy_observation_v6::tests::ready_state();
        let source = crate::policy_observation_v6::tests::put(
            &mut state,
            actor,
            "Llanowar Elves",
            Zone::Battlefield,
        );
        let bytes = serde_json::to_vec(&state).unwrap();
        let hash = state.state_hash();
        before_departure(&mut state, source);
        before_protected_departures(&mut state, &[source], &[]);
        validate_state(&state).unwrap();
        assert!(public_sources(&state).unwrap().is_empty());
        assert_eq!(bytes, serde_json::to_vec(&state).unwrap());
        assert_eq!(hash, state.state_hash());
        let marker = crate::state::EquipmentDamagePublishedV4::default();
        let mut before = std::collections::hash_map::DefaultHasher::new();
        17_u8.hash(&mut before);
        let mut after = std::collections::hash_map::DefaultHasher::new();
        17_u8.hash(&mut after);
        marker.hash(&mut after);
        assert_eq!(before.finish(), after.finish());
        assert!(serde_json::to_value(crate::state::StackStateV4::default())
            .unwrap()
            .get("equipment_damage_published_v4")
            .is_none());
    }
}

#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
#[test]
fn registered_rod_and_elves_actual_cast_equip_and_departed_grant_resolve_without_crystal_both_seats(
) {
    use crate::engine::{Action, Decision};
    use crate::event::{self, ProposedEvent};
    use crate::mana::ManaColor;
    fn finish(state: &mut GameState) {
        for _ in 0..40 {
            let decision = crate::engine::advance_until_decision(state);
            if state.stack.is_empty()
                && state.engine.pending_activation.is_none()
                && state.engine.pending_cast.is_none()
            {
                return;
            }
            assert!(
                matches!(decision, Decision::CastSpellOrPass { .. }),
                "{decision:?}"
            );
            crate::engine::step(state, Action::Pass).unwrap();
        }
        panic!("registered Rod fixture did not finish");
    }
    for actor in [crate::ids::PlayerId::P0, crate::ids::PlayerId::P1] {
        let mut state = crate::policy_observation_v6::tests::ready_state();
        state.active_player = actor;
        state.priority_player = actor;
        state.step = crate::state::Step::Main1;
        let host = crate::policy_observation_v6::tests::put(
            &mut state,
            actor,
            "Llanowar Elves",
            Zone::Battlefield,
        );
        for seat in [actor, actor.opponent()] {
            crate::policy_observation_v6::tests::put(
                &mut state,
                seat,
                "Lightning Bolt",
                Zone::Hand,
            );
            crate::policy_observation_v6::tests::put(
                &mut state,
                seat,
                "Mountain",
                Zone::Battlefield,
            );
        }
        let rod = crate::policy_observation_v6::tests::put(
            &mut state,
            actor,
            "Black Mage's Rod",
            Zone::Hand,
        );
        state.players[actor.index()].mana_pool[ManaColor::B.pool_index()] = 1;
        state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 1;
        crate::engine::step(&mut state, Action::CastSpell(rod)).unwrap();
        finish(&mut state);
        assert_eq!(state.objects.get(rod).zone, Zone::Battlefield);
        state.priority_player = actor;
        state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 3;
        crate::engine::step(&mut state, Action::ActivateAbility(rod, 0)).unwrap();
        assert!(
            matches!(crate::engine::advance_until_decision(&mut state),Decision::ChooseTargets {legal_targets,..}
            if legal_targets.contains(&crate::state::Target::Object(host)))
        );
        crate::engine::step(
            &mut state,
            Action::ChooseTarget(crate::state::Target::Object(host)),
        )
        .unwrap();
        finish(&mut state);
        assert!(state.objects.get(host).attachments.contains(&rod));
        // Registered producer plus typed keyword-counter/physical-departure setup.
        state.objects.get_mut(host).v4.lifelink_keyword_counters = 1;
        let bolt = crate::policy_observation_v6::tests::put(
            &mut state,
            actor,
            "Lightning Bolt",
            Zone::Hand,
        );
        state.priority_player = actor;
        state.players[actor.index()].mana_pool[ManaColor::R.pool_index()] = 1;
        crate::engine::step(&mut state, Action::CastSpell(bolt)).unwrap();
        assert!(matches!(
            crate::engine::advance_until_decision(&mut state),
            Decision::ChooseTargets { .. }
        ));
        crate::engine::step(
            &mut state,
            Action::ChooseTarget(crate::state::Target::Player(actor)),
        )
        .unwrap();
        let _ = crate::engine::advance_until_decision(&mut state);
        let grant = state
            .stack
            .iter()
            .find(|i| i.v4.granted_by.is_some_and(|g| g.source == rod))
            .unwrap()
            .v4
            .stack_item_id;
        assert!(has_publication(&state, grant));
        let mut fake = state.clone();
        let published = publication(&fake, grant).unwrap().clone();
        fake.stack.retain(|item| item.v4.stack_item_id != grant);
        let index = u32::try_from(fake.engine.event_history.len()).unwrap();
        fake.engine
            .event_history
            .push(CommittedEvent::EquipmentDamageClosedV4 {
                closed: Box::new(ClosedV4 {
                    index,
                    publication: published.index,
                    item: published.item,
                    outcome: OutcomeV4::Countered,
                    boundary: None,
                    counter: None,
                }),
            });
        let mut fake: GameState =
            serde_json::from_slice(&serde_json::to_vec(&fake).unwrap()).unwrap();
        let bytes = serde_json::to_vec(&fake).unwrap();
        assert!(crate::engine::step(&mut fake, Action::Pass).is_err());
        assert_eq!(bytes, serde_json::to_vec(&fake).unwrap());
        let mut expected = fake.clone();
        expected.engine.halted = Some((
            crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
            ObjectId(0),
        ));
        assert!(matches!(
            crate::engine::advance_until_decision(&mut fake),
            Decision::Halted { .. }
        ));
        assert_eq!(
            serde_json::to_vec(&fake).unwrap(),
            serde_json::to_vec(&expected).unwrap()
        );
        let mut unowned = state.clone();
        let item = unowned
            .stack
            .iter()
            .find(|item| item.v4.stack_item_id == grant)
            .unwrap()
            .clone();
        let mut expected = unowned.clone();
        expected.engine.halted = Some((
            crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
            host,
        ));
        crate::effect::execute(&printed_root(&item).unwrap(), &context(&item), &mut unowned);
        assert_eq!(
            serde_json::to_vec(&unowned).unwrap(),
            serde_json::to_vec(&expected).unwrap()
        );
        assert!(!crate::crystal_barricade_v1::active(&state));
        // Accepted internal counter primitive against the genuine published row.
        let mut countered = state.clone();
        crate::engine::counter_stack_item_by_id(&mut countered, grant)
            .unwrap()
            .unwrap();
        validate_state(&countered).unwrap();
        assert_eq!(closed_count(&countered, grant), 1);
        let mut counter_replay: GameState =
            serde_json::from_slice(&serde_json::to_vec(&countered).unwrap()).unwrap();
        finish(&mut countered);
        finish(&mut counter_replay);
        assert_eq!(
            serde_json::to_vec(&countered).unwrap(),
            serde_json::to_vec(&counter_replay).unwrap()
        );
        let mut erased = countered;
        erased
            .engine
            .event_history
            .retain(|e| !matches!(e, CommittedEvent::EquipmentDamageClosedV4 { .. }));
        let bytes = serde_json::to_vec(&erased).unwrap();
        assert!(crate::engine::step(&mut erased, Action::Pass).is_err());
        assert_eq!(bytes, serde_json::to_vec(&erased).unwrap());
        event::propose_and_commit(&mut state, ProposedEvent::zone_change(host, Zone::Hand));
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(host, Zone::Battlefield),
        );
        state.objects.get_mut(host).v4.lifelink_keyword_counters = 0;
        let mut erased = state.clone();
        // Keep every original occurrence at its original index while erasing only the private departure proof.
        for event in &mut erased.engine.event_history {
            if matches!(event, CommittedEvent::EquipmentDamageDepartureV4 { .. }) {
                *event = CommittedEvent::Tap { object: host };
            }
        }
        let mut erased: GameState =
            serde_json::from_slice(&serde_json::to_vec(&erased).unwrap()).unwrap();
        let bytes = serde_json::to_vec(&erased).unwrap();
        assert!(crate::engine::step(&mut erased, Action::Pass).is_err());
        assert_eq!(bytes, serde_json::to_vec(&erased).unwrap());
        let mut expected = erased.clone();
        expected.engine.halted = Some((
            crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
            ObjectId(0),
        ));
        assert!(matches!(
            crate::engine::advance_until_decision(&mut erased),
            Decision::Halted { .. }
        ));
        assert_eq!(
            serde_json::to_vec(&erased).unwrap(),
            serde_json::to_vec(&expected).unwrap()
        );
        let life = [state.players[0].life, state.players[1].life];
        let mut replay: GameState =
            serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
        for current in [&mut state, &mut replay] {
            finish(current);
            validate_state(current).unwrap();
            assert_eq!(current.players[actor.index()].life, life[actor.index()] - 2);
            assert_eq!(
                current.players[actor.opponent().index()].life,
                life[actor.opponent().index()] - 1
            );
            assert_eq!(closed_count(current, grant), 1);
        }
        assert_eq!(
            serde_json::to_vec(&state).unwrap(),
            serde_json::to_vec(&replay).unwrap()
        );
    }
}
