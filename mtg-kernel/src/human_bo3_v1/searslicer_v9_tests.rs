//! Future actual Searslicer root on the unchanged Raid Human transport.
use super::*;
use crate::engine::{self, Action};
use crate::ids::PlayerId;
use crate::policy_observation_v6::tests::{put, ready_state};
use crate::state::{Step, Zone};

#[test]
fn future_searslicer_end_step_uses_existing_safe_pass_actions_and_exact_restore() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready_state();
        state.turn = 3;
        state.active_player = actor;
        state.priority_player = actor;
        state.step = Step::DeclareAttackers;
        put(&mut state, actor, "Searslicer Goblin", Zone::Battlefield);
        let attacker = put(&mut state, actor, "Guttersnipe", Zone::Battlefield);
        engine::step(&mut state, Action::DeclareAttackers(vec![attacker])).unwrap();
        // Typed postcombat setup. The integration fixtures traverse real combat.
        state.step = Step::Main2;
        state.engine.combat = Default::default();
        for player in [actor, actor.opponent()] {
            put(&mut state, player, "Mountain", Zone::Battlefield);
            put(&mut state, player, "Lightning Bolt", Zone::Hand);
            put(&mut state, player, "Island", Zone::Library);
        }
        // The inherited harness suppresses every End priority window. Select
        // the existing explicit engine-priority surface for this consumer test.
        let mut session = HumanRaidSessionV9::from_session(
            FastActorSessionV1::from_human_v8_engine_priority_fixture_state(state.clone()),
        )
        .unwrap();
        let mut replay = HumanRaidSessionV9::from_session(
            FastActorSessionV1::from_human_v8_engine_priority_fixture_state(state),
        )
        .unwrap();
        let mut projectors = [
            HumanDecisionProjectorV9::new(PlayerSeatV1::P0),
            HumanDecisionProjectorV9::new(PlayerSeatV1::P1),
        ];
        let mut replay_projectors = [
            HumanDecisionProjectorV9::new(PlayerSeatV1::P0),
            HumanDecisionProjectorV9::new(PlayerSeatV1::P1),
        ];
        let mut restored_pending = false;
        let mut completed = false;
        for _ in 0..24 {
            let FastActorResponseV1::Decision(expected) = session.current_response() else {
                panic!("actual printed trigger must retain priority");
            };
            let index = usize::from(expected.acting_player == PlayerSeatV1::P1);
            let prompt = projectors[index]
                .project_current(&session, expected)
                .unwrap();
            let FastActorResponseV1::Decision(replay_expected) = replay.current_response() else {
                panic!("restore must retain priority");
            };
            let replay_prompt = replay_projectors[index]
                .project_current(&replay, replay_expected)
                .unwrap();
            assert_eq!(prompt, replay_prompt);
            let raw = serde_json::to_string(&prompt).unwrap();
            for forbidden in [
                "arena_id",
                "card_db_id",
                "source_contract",
                "visible_projection_hash",
            ] {
                assert!(!raw.contains(forbidden), "{forbidden}");
            }
            let own_is_actor = expected.acting_player == PlayerSeatV1::from(actor);
            assert_eq!(prompt.state.extensions.own_attacked_this_turn, own_is_actor);
            assert_eq!(
                prompt.state.extensions.opponent_attacked_this_turn,
                !own_is_actor
            );
            if !restored_pending && !prompt.state.public.stack.is_empty() {
                assert_eq!(prompt.state.public.stack.len(), 1);
                let snapshot = session.snapshot_v9();
                let before = session.inner.privileged_core_environment_hash();
                replay.restore_v9(&snapshot);
                assert_eq!(replay.inner.privileged_core_environment_hash(), before);
                // The frozen adapter's owning rollback behavior is not widened.
                // Fresh projectors replay the exact snapshot.
                projectors = [
                    HumanDecisionProjectorV9::new(PlayerSeatV1::P0),
                    HumanDecisionProjectorV9::new(PlayerSeatV1::P1),
                ];
                replay_projectors = [
                    HumanDecisionProjectorV9::new(PlayerSeatV1::P0),
                    HumanDecisionProjectorV9::new(PlayerSeatV1::P1),
                ];
                restored_pending = true;
                continue;
            }
            let tokens = prompt.state.public.battlefield[actor.index()]
                .iter()
                .filter(|card| card.stable.name == "Goblin Token")
                .collect::<Vec<_>>();
            if !tokens.is_empty() {
                assert_eq!(tokens.len(), 1);
                assert!(tokens[0].is_token && tokens[0].summoning_sick && !tokens[0].tapped);
                completed = true;
                break;
            }
            let pass = prompt
                .actions
                .iter()
                .find(|action| action.label == "Pass priority")
                .unwrap();
            let request = HumanActionRequestV1 {
                prompt_seq: prompt.prompt_seq,
                action_index: pass.action_index,
            };
            let receipt = projectors[index]
                .submit(&mut session, request.clone())
                .unwrap();
            let hash = session.inner.privileged_core_environment_hash();
            assert_eq!(
                projectors[index]
                    .submit(&mut session, request.clone())
                    .unwrap(),
                receipt
            );
            assert_eq!(session.inner.privileged_core_environment_hash(), hash);
            replay_projectors[index]
                .submit(&mut replay, request)
                .unwrap();
            assert_eq!(
                session.inner.privileged_core_environment_hash(),
                replay.inner.privileged_core_environment_hash()
            );
        }
        assert!(restored_pending && completed);
    }
}
