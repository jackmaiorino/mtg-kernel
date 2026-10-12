//! Genuine ordinary spell operands retain the preexisting path in every catalog.
use super::*;
use crate::engine::{self, Action, Decision};
use crate::policy_observation_v6::tests::put;
use crate::state::{SquadRallierStartedV1, Step};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

#[test]
fn empty_started_binding_has_no_serialized_stack_field_or_hash_input() {
    let mut before = DefaultHasher::new();
    1185u64.hash(&mut before);
    let mut after = before.clone();
    SquadRallierStartedV1::default().hash(&mut after);
    assert_eq!(before.finish(), after.finish());
    let v4 = crate::state::StackStateV4::default();
    let bytes = serde_json::to_vec(&v4).unwrap();
    assert!(!String::from_utf8(bytes.clone())
        .unwrap()
        .contains("squad_rallier_started_v1"));
    let replay: crate::state::StackStateV4 = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(serde_json::to_vec(&replay).unwrap(), bytes);
}

#[test]
fn real_ordinary_bolt_keeps_absent_ownership_bytes_hash_and_pass_replay_both_seats() {
    let forest = crate::card_def::card_id_by_name("Forest").unwrap();
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = GameState::new_from_libraries_with_starting_player_v1(
            &[forest; 20],
            &[forest; 20],
            |_| "Forest".into(),
            1185,
            actor,
        );
        state.step = Step::Main1;
        for player in [PlayerId::P0, PlayerId::P1] {
            put(&mut state, player, "Mountain", Zone::Battlefield);
            put(&mut state, player, "Lightning Bolt", Zone::Hand);
        }
        let bolt = put(&mut state, actor, "Lightning Bolt", Zone::Hand);
        state.players[actor.index()].mana_pool[crate::mana::ManaColor::R.pool_index()] = 1;
        assert!(
            matches!(engine::advance_until_decision(&mut state), Decision::CastSpellOrPass { player, .. } if player == actor)
        );
        engine::step(&mut state, Action::CastSpell(bolt)).unwrap();
        assert!(matches!(
            engine::advance_until_decision(&mut state),
            Decision::ChooseTargets { .. }
        ));
        engine::step(
            &mut state,
            Action::ChooseTarget(Target::Player(actor.opponent())),
        )
        .unwrap();
        assert!(matches!(
            engine::advance_until_decision(&mut state),
            Decision::CastSpellOrPass { .. }
        ));
        let item = state.stack.last().unwrap().clone();
        assert!(item.v4.squad_rallier_started_v1.is_empty());
        let bytes = serde_json::to_vec(&state).unwrap();
        let hash = state.state_hash();
        assert!(acquire_owner(&state, &item, true).unwrap().is_none());
        validate(&state).unwrap();
        assert_eq!(serde_json::to_vec(&state).unwrap(), bytes);
        assert_eq!(state.state_hash(), hash);
        assert!(!String::from_utf8(bytes.clone())
            .unwrap()
            .contains("squad_rallier_started_v1"));
        let mut replay: GameState = serde_json::from_slice(&bytes).unwrap();
        let life = state.players[actor.opponent().index()].life;
        for current in [&mut state, &mut replay] {
            for _ in 0..8 {
                let d = engine::advance_until_decision(current);
                if current.stack.is_empty() {
                    break;
                }
                assert!(matches!(d, Decision::CastSpellOrPass { .. }), "{d:?}");
                engine::step(current, Action::Pass).unwrap();
            }
            assert!(current.stack.is_empty());
            assert_eq!(current.players[actor.opponent().index()].life, life - 3);
            assert!(current.engine.event_history.iter().all(|event| !matches!(
                event,
                CommittedEvent::SquadRallierResolutionBoundaryV1 { .. }
            )));
        }
        assert_eq!(
            serde_json::to_vec(&state).unwrap(),
            serde_json::to_vec(&replay).unwrap()
        );
        assert_eq!(state.state_hash(), replay.state_hash());
    }
}
