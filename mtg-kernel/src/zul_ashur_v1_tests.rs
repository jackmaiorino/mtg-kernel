use super::*;
use crate::engine::{self, Action, Decision};
use crate::policy_observation_v6::tests::{put, ready_state};
#[test]
fn unrelated_actual_bolt_private_loan_publication_and_restore_preserve_bytes_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready_state();
        state.active_player = actor;
        state.priority_player = actor;
        let bolt = put(&mut state, actor, "Lightning Bolt", Zone::Hand);
        state.players[actor.index()].mana_pool[crate::mana::ManaColor::R.pool_index()] = 1;
        engine::step(&mut state, Action::CastSpell(bolt)).unwrap();
        engine::step(
            &mut state,
            Action::ChooseTarget(Target::Player(actor.opponent())),
        )
        .unwrap();
        assert!(matches!(
            engine::advance_until_decision(&mut state),
            Decision::CastSpellOrPass { .. }
        ));
        let item = state
            .stack
            .iter()
            .find(|i| i.source == bolt)
            .unwrap()
            .clone();
        let before = serde_json::to_vec(&state).unwrap();
        let hash = state.state_hash();
        assert!(acquire(&state, &item).unwrap().is_none());
        publish(&mut state, item.v4.stack_item_id).unwrap();
        note_cleanup(&mut state).unwrap();
        expire(&mut state).unwrap();
        validate_state(&state).unwrap();
        assert!(!permission(&state, bolt, actor, 0));
        assert!(!has_active(&state));
        assert_eq!(before, serde_json::to_vec(&state).unwrap());
        assert_eq!(hash, state.state_hash());
        let mut restored: GameState = serde_json::from_slice(&before).unwrap();
        engine::step(&mut state, Action::Pass).unwrap();
        engine::step(&mut restored, Action::Pass).unwrap();
        assert_eq!(
            serde_json::to_vec(&state).unwrap(),
            serde_json::to_vec(&restored).unwrap()
        );
    }
}
#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
mod future_printed {
    use super::*;
    fn next(state: &mut GameState) -> Decision {
        let d = engine::advance_until_decision(state);
        assert!(!matches!(d, Decision::Halted { .. }), "{d:?}");
        d
    }
    fn setup(actor: PlayerId) -> (GameState, ObjectId, ObjectId) {
        let mut state = ready_state();
        state.active_player = actor;
        state.priority_player = actor;
        state.step = crate::state::Step::Main1;
        let source = put(&mut state, actor, "Zul Ashur, Lich Lord", Zone::Battlefield);
        let target = put(&mut state, actor, "Gutless Plunderer", Zone::Graveyard);
        let foreign = put(
            &mut state,
            actor.opponent(),
            "Gutless Plunderer",
            Zone::Graveyard,
        );
        let nonzombie = put(&mut state, actor, "Llanowar Elves", Zone::Graveyard);
        let land = put(&mut state, actor, "Forest", Zone::Graveyard);
        engine::step(&mut state, Action::ActivateAbility(source, 0)).unwrap();
        let Decision::ChooseTargets { legal_targets, .. } = next(&mut state) else {
            panic!("actual target choice")
        };
        assert!(legal_targets.contains(&Target::Object(target)));
        for id in [foreign, nonzombie, land] {
            assert!(!legal_targets.contains(&Target::Object(id)));
        }
        let bytes = serde_json::to_vec(&state).unwrap();
        assert!(engine::step(&mut state, Action::ChooseTarget(Target::Object(foreign))).is_err());
        assert_eq!(bytes, serde_json::to_vec(&state).unwrap());
        engine::step(&mut state, Action::ChooseTarget(Target::Object(target))).unwrap();
        let _ = next(&mut state);
        assert!(state.objects.get(source).tapped);
        (state, source, target)
    }
    fn settle(state: &mut GameState) {
        for _ in 0..40 {
            let d = next(state);
            if state.stack.is_empty() {
                return;
            }
            assert!(matches!(d, Decision::CastSpellOrPass { .. }), "{d:?}");
            engine::step(state, Action::Pass).unwrap();
        }
        panic!("resolution");
    }
    #[test]
    #[ignore = "Zul Ashur is unregistered; root-owned actual printed qualification"]
    fn actual_targets_payment_original_beneficiary_departed_source_normal_cast_and_restoration_both_seats(
    ) {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let (mut state, source, target) = setup(actor);
            crate::event::propose_and_commit(
                &mut state,
                crate::event::ProposedEvent::zone_change(source, Zone::Hand),
            );
            let mut replay: GameState =
                serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
            for s in [&mut state, &mut replay] {
                settle(s);
                let generation = s.objects.get(target).zone_change_count;
                assert!(permission(s, target, actor, generation));
                assert!(!permission(s, target, actor.opponent(), generation));
                s.priority_player = actor;
                s.players[actor.index()].mana_pool.fill(0);
                let bytes = serde_json::to_vec(s).unwrap();
                assert!(engine::step(s, Action::CastSpell(target)).is_err());
                assert_eq!(bytes, serde_json::to_vec(s).unwrap());
                s.players[actor.index()].mana_pool[crate::mana::ManaColor::B.pool_index()] = 1;
                s.players[actor.index()].mana_pool[crate::mana::ManaColor::C.pool_index()] = 2;
                engine::step(s, Action::CastSpell(target)).unwrap();
                let _ = next(s);
                assert_eq!(s.objects.get(target).zone, Zone::Stack);
                settle(s);
                assert_eq!(s.objects.get(target).zone, Zone::Battlefield);
                crate::event::propose_and_commit(
                    s,
                    crate::event::ProposedEvent::zone_change(target, Zone::Graveyard),
                );
                assert!(!permission(
                    s,
                    target,
                    actor,
                    s.objects.get(target).zone_change_count
                ));
            }
            assert_eq!(
                serde_json::to_vec(&state).unwrap(),
                serde_json::to_vec(&replay).unwrap()
            );
        }
    }
    #[test]
    #[ignore = "Zul Ashur is unregistered; root-owned actual printed qualification"]
    fn actual_owner_erasure_counter_target_blink_cleanup_and_meaning_forgeries_both_seats() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let (state, _, target) = setup(actor);
            let item = state.stack.last().unwrap().clone();
            let mut bad: GameState =
                serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
            bad.engine
                .event_history
                .retain(|e| !matches!(e, CommittedEvent::ZulAshurPublishedV1 { .. }));
            let bytes = serde_json::to_vec(&bad).unwrap();
            assert!(engine::step(&mut bad, Action::Pass).is_err());
            assert_eq!(bytes, serde_json::to_vec(&bad).unwrap());
            let mut countered = state.clone();
            engine::counter_stack_item_by_id(&mut countered, item.v4.stack_item_id)
                .unwrap()
                .unwrap();
            validate_state(&countered).unwrap();
            assert!(!has_active(&countered));
            let mut stale = state.clone();
            for zone in [Zone::Hand, Zone::Graveyard] {
                crate::event::propose_and_commit(
                    &mut stale,
                    crate::event::ProposedEvent::zone_change(target, zone),
                );
            }
            settle(&mut stale);
            assert!(!has_active(&stale));
            let mut resolved = state;
            settle(&mut resolved);
            assert!(has_active(&resolved));
            resolved.step = crate::state::Step::End;
            for _ in 0..40 {
                let d = next(&mut resolved);
                if resolved.active_player != actor {
                    break;
                }
                assert!(matches!(d, Decision::CastSpellOrPass { .. }), "{d:?}");
                engine::step(&mut resolved, Action::Pass).unwrap();
            }
            assert_eq!(resolved.active_player, actor.opponent());
            assert!(!has_active(&resolved));
            validate_state(&resolved).unwrap();
            let selector = crate::rules_meaning_zul_ashur_v1::live_selector().unwrap();
            let id = crate::card_def::card_id_by_name("Zul Ashur, Lich Lord").unwrap();
            for root in [
                crate::rules_meaning_zul_ashur_v1::RootV1::PrintedCreature,
                crate::rules_meaning_zul_ashur_v1::RootV1::WardPayTwoLife,
                crate::rules_meaning_zul_ashur_v1::RootV1::PrintedActivation,
            ] {
                let mut meaning =
                    crate::rules_meaning_zul_ashur_v1::meaning(&selector, id, root).unwrap();
                crate::rules_meaning_zul_ashur_v1::validate(&meaning).unwrap();
                meaning.rule.ordinary_timing_and_all_costs = false;
                assert!(crate::rules_meaning_zul_ashur_v1::validate(&meaning).is_err());
            }
        }
    }
}
