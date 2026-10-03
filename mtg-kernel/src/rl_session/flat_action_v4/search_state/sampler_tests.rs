use super::*;
use crate::engine::{self, Action, Decision};
use crate::mana::ManaColor;
use crate::policy_observation_v6::tests::{put, ready_state};

fn library_choice(card: &str) -> GameState {
    let mut state = ready_state();
    let source = put(&mut state, PlayerId::P0, card, Zone::Hand);
    for owner in [PlayerId::P0, PlayerId::P1] {
        for name in ["Forest", "Island", "Mountain", "Swamp", "Counterspell"] {
            put(&mut state, owner, name, Zone::Library);
        }
    }
    state.players[0].mana_pool[ManaColor::U.pool_index()] = 3;
    engine::step(&mut state, Action::CastSpell(source)).unwrap();
    for _ in 0..20 {
        match engine::advance_until_decision(&mut state) {
            Decision::CastSpellOrPass { .. } => engine::step(&mut state, Action::Pass).unwrap(),
            Decision::ChooseTargets { .. } => engine::step(
                &mut state,
                Action::ChooseTarget(crate::state::Target::Player(PlayerId::P0)),
            )
            .unwrap(),
            Decision::ChooseEffectTargets { .. } => return state,
            other => panic!("unexpected {card} decision {other:?}"),
        }
    }
    panic!("no library choice");
}

#[test]
fn v4_search_sampler_library_locks_admit_and_unlocked_binding_rejects() {
    let state = library_choice("Preordain");
    assert_eq!(
        state.known_library_cards(PlayerId::P0, PlayerId::P0).len(),
        2
    );
    let scry = FastActorSessionV1::from_v3_fixture_state(state.clone());
    let mill = FastActorSessionV1::from_v3_fixture_state(library_choice("Thought Scour"));
    for seed in [1, 2, 8172, 9381] {
        assert!(
            scry.kernel_search_redeterminized_clone_v4(seed).is_ok(),
            "normal scry retains its owner locks"
        );
        assert!(
            mill.kernel_search_redeterminized_clone_v4(seed).is_ok(),
            "normal mill retains its owner locks"
        );
        // Deliberately corrupt the knowledge table to exercise the reference
        // scan independently of the later root boundary comparison.
        let mut unlocked = state.clone();
        unlocked.library_knowledge[0][0].clear();
        assert_eq!(
            sampler::redeterminize(&mut unlocked, PlayerId::P0, seed),
            Err(Error::HiddenStateContract)
        );
    }
}

fn historical_opponent_source(owner: PlayerId) -> (GameState, ObjectId) {
    let mut state = ready_state();
    state.active_player = owner;
    state.priority_player = owner;
    let other = owner.opponent();
    let source = put(&mut state, owner, "Writhing Chrysalis", Zone::Hand);
    let counter = put(&mut state, other, "Counterspell", Zone::Hand);
    for (color, n) in [(ManaColor::R, 1), (ManaColor::G, 1), (ManaColor::C, 2)] {
        state.players[owner.index()].mana_pool[color.pool_index()] = n;
    }
    state.players[other.index()].mana_pool[ManaColor::U.pool_index()] = 2;
    engine::step(&mut state, Action::CastSpell(source)).unwrap();
    assert!(matches!(
        engine::advance_until_decision(&mut state),
        Decision::CastSpellOrPass { .. }
    ));
    engine::step(&mut state, Action::Pass).unwrap();
    engine::advance_until_decision(&mut state);
    engine::step(&mut state, Action::CastSpell(counter)).unwrap();
    engine::advance_until_decision(&mut state);
    engine::step(
        &mut state,
        Action::ChooseTarget(crate::state::Target::Object(source)),
    )
    .unwrap();
    for _ in 0..16 {
        if state.stack.len() == 1 {
            break;
        }
        assert!(matches!(
            engine::advance_until_decision(&mut state),
            Decision::CastSpellOrPass { .. }
        ));
        engine::step(&mut state, Action::Pass).unwrap();
    }
    assert_eq!(state.stack.len(), 1);
    assert_eq!(state.objects.get(source).zone, Zone::Graveyard);
    crate::event::propose_and_commit(
        &mut state,
        crate::event::ProposedEvent::zone_change(source, Zone::Library),
    );
    let triggers = crate::trigger::collect_and_process(&mut state);
    assert!(triggers.is_empty());
    for p in [owner, other] {
        for name in ["Forest", "Island", "Mountain", "Swamp"] {
            put(&mut state, p, name, Zone::Library);
        }
    }
    for name in ["Lightning Bolt", "Gut Shot", "Lotus Petal"] {
        put(&mut state, owner, name, Zone::Hand);
    }
    put(&mut state, other, "Lightning Bolt", Zone::Hand);
    state.players[other.index()].mana_pool[ManaColor::R.pool_index()] = 1;
    // Deliberate fixed-state fixture, not a claim of a naturally played shuffle.
    state.priority_player = other;
    state.engine.priority_passes = [false, false];
    (state, source)
}

#[test]
fn v4_search_sampler_historical_source_crosses_opponent_zones_both_seats() {
    for owner in [PlayerId::P0, PlayerId::P1] {
        let (state, source) = historical_opponent_source(owner);
        let session = FastActorSessionV1::from_v3_fixture_state(state);
        let FastActorResponseV1::Decision(root) = session.current_response() else {
            panic!("root")
        };
        assert_eq!(
            crate::kernel_native_search_opponent_v1::player_id_v1(root.acting_player),
            owner.opponent()
        );
        let mut in_hand = false;
        let mut in_library = false;
        for seed in 1..=32 {
            let mut sample = session.kernel_search_redeterminized_clone_v4(seed).unwrap();
            match sample.state.objects.get(source).zone {
                Zone::Hand => in_hand = true,
                Zone::Library => in_library = true,
                _ => panic!("pool"),
            }
            assert_eq!(
                session.kernel_search_visible_key_v4(4),
                sample.kernel_search_visible_key_v4(4)
            );
            let mut direct = sample.clone();
            for _ in 0..4 {
                let FastActorResponseV1::Decision(d) = sample.current_response() else {
                    break;
                };
                let token = sample.kernel_search_action_token_v4(d).unwrap();
                assert_eq!(
                    sample.kernel_search_consume_v4(d, token, 0).unwrap(),
                    direct.step(d.episode_id, d.step, 0).unwrap()
                );
                assert_eq!(
                    sample.diagnostic_state_hash(),
                    direct.diagnostic_state_hash()
                );
            }
        }
        assert!(
            in_hand && in_library,
            "must cover both zones, not just shuffle library positions"
        );
    }
}
