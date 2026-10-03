use super::*;

#[test]
fn public_state_only_active_prevention_hidden_invariance_and_replay_both_seats() {
    use crate::event::install_color_damage_prevention;
    use crate::ids::PlayerId;
    use crate::mana::ManaColor;
    use crate::policy_observation_v6::tests::{put, ready_state};
    use crate::state::Zone;

    for actor in [PlayerId::P0, PlayerId::P1] {
        let opponent = if actor == PlayerId::P0 {
            PlayerId::P1
        } else {
            PlayerId::P0
        };
        let mut state = ready_state();
        state.active_player = actor;
        state.priority_player = actor;
        let source = put(&mut state, actor, "Prismatic Strands", Zone::Graveyard);
        put(&mut state, actor, "Sacred Cat", Zone::Battlefield);
        put(&mut state, opponent, "Voldaren Epicure", Zone::Battlefield);
        install_color_damage_prevention(&mut state, source, ManaColor::R).unwrap();
        let mut outputs = Vec::new();
        for variant in 0..2 {
            let mut hidden = state.clone();
            put(
                &mut hidden,
                opponent,
                if variant == 0 { "Island" } else { "Mountain" },
                Zone::Hand,
            );
            for player in [actor, opponent] {
                for name in ["Island", "Mountain"] {
                    put(&mut hidden, player, name, Zone::Library);
                }
                if variant == 1 {
                    hidden.players[player.index()].library.reverse();
                }
            }
            let session = FastActorSessionV1::from_v3_fixture_state(hidden);
            let response = session.current_response();
            let FastActorResponseV1::Decision(decision) = response else {
                panic!("expected live decision")
            };
            assert!(decision.legal_action_count > 1);
            let input = PairedBo1PolicyInputV1::new(&session, decision);
            let mut policy = PublicInputPlayPolicyV1::new(
                FrozenPlayPolicyV1::training_fixture_v4(),
                PublicInputWeightsV1::new(
                    vec![0.0; 2048],
                    (0..384).map(|i| (i % 17) as f32 * 0.01).collect(),
                )
                .unwrap(),
            )
            .unwrap();
            policy.reset_for_game_v1([321, 654]).unwrap();
            let (action, scores) = policy.select_with_scores(&input).unwrap();
            assert!(action < decision.legal_action_count);
            let (tensor, auxiliary) = policy.captured().unwrap();
            assert_eq!(
                auxiliary.state[3], 1.0,
                "active red prevention reaches scorer"
            );
            let replay = policy.replay(tensor, auxiliary).unwrap();
            assert_eq!(scores.logits, replay.logits);
            assert_eq!(scores.value.to_bits(), replay.value.to_bits());
            let mut removed = auxiliary.clone();
            removed.state.fill(0.0);
            let without_state = policy.replay(tensor, &removed).unwrap();
            assert!(
                scores.logits != without_state.logits
                    || scores.value.to_bits() != without_state.value.to_bits(),
                "test must exercise the state projection"
            );
            let mut removed_cost = auxiliary.clone();
            for object in &mut removed_cost.objects {
                object.fill(0.0);
            }
            // Stored cost rows remain a checked public-card description even
            // when their learned projection is zero. Corruption must reject.
            assert!(policy.replay(tensor, &removed_cost).is_err());
            let control = policy
                .fork_for_collection()
                .unwrap()
                .with_inputs_enabled(false);
            let control_scores = control.replay(tensor, auxiliary).unwrap();
            let zero = PublicInputPlayPolicyV1::new(
                FrozenPlayPolicyV1::training_fixture_v4(),
                PublicInputWeightsV1::zero(),
            )
            .unwrap();
            let zero_scores = zero.replay(tensor, auxiliary).unwrap();
            assert_eq!(control_scores.logits, zero_scores.logits);
            assert_eq!(control_scores.value.to_bits(), zero_scores.value.to_bits());
            outputs.push((
                action,
                scores.logits.clone(),
                scores.value.to_bits(),
                tensor.clone(),
                auxiliary.clone(),
            ));
            policy.reset_for_game_v1([321, 654]).unwrap();
            assert_eq!(policy.select_with_scores(&input).unwrap().0, action);
            assert_eq!(session.current_response(), response);
        }
        assert_eq!(
            outputs[0], outputs[1],
            "opponent hand and either library order must be invisible"
        );
    }
}
