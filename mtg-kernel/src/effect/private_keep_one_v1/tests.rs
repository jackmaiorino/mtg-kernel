//! Physical primitives use registered cards, without admitting a printed root.
use super::*;
use crate::policy_observation_v6::tests::put;
use crate::rl::observe_v2;
fn ready(actor: PlayerId, size: usize) -> GameState {
    let cards = ["Forest", "Lightning Bolt", "Island", "Mountain", "Swamp"]
        .map(|name| crate::card_def::card_id_by_name(name).unwrap());
    let mut state = GameState::new_from_libraries_with_starting_player_v1(
        &cards[..size],
        &cards[..size],
        |id| crate::card_def::CARD_DEFS[id as usize].name.into(),
        11006,
        actor,
    );
    state.step = crate::state::Step::Main1;
    state
}
fn replay(state: &GameState) -> GameState {
    let bytes = serde_json::to_vec(state).unwrap();
    let restored: GameState = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(bytes, serde_json::to_vec(&restored).unwrap());
    assert_eq!(state.state_hash(), restored.state_hash());
    restored
}
#[test]
fn physical_keep_zero_or_one_short_empty_and_ordered_graveyard_restore_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for size in 0..=5 {
            let length = size.min(3);
            for kept_index in std::iter::once(None).chain((0..length).map(Some)) {
                let mut state = ready(actor, size);
                let source = put(&mut state, actor, "Lightning Bolt", Zone::Hand);
                let binding = capture(&mut state, &ExecCtx::no_targets(source, actor)).unwrap();
                assert_eq!(state.known_library_cards(actor, actor).len(), length);
                assert!(state
                    .known_library_cards(actor.opponent(), actor)
                    .is_empty());
                let graveyard = binding.original_library[..length]
                    .iter()
                    .enumerate()
                    .filter(|(index, _)| Some(*index) != kept_index)
                    .map(|(_, card)| *card)
                    .rev()
                    .collect::<Vec<_>>();
                let mut restored = replay(&state);
                // A departing source does not substitute a new chooser or cancel the look.
                for branch in [&mut state, &mut restored] {
                    event::propose_and_commit(
                        branch,
                        event::ProposedEvent::zone_change(source, Zone::Graveyard),
                    );
                    commit(branch, &binding, &graveyard).unwrap();
                    let expected = kept_index
                        .map(|index| binding.original_library[index].object)
                        .into_iter()
                        .chain(
                            binding.original_library[length..]
                                .iter()
                                .map(|card| card.object),
                        )
                        .collect::<Vec<_>>();
                    assert_eq!(branch.players[actor.index()].library, expected);
                    assert_eq!(
                        branch.players[actor.index()].graveyard[1..],
                        graveyard.iter().map(|card| card.object).collect::<Vec<_>>()
                    );
                    assert!(graveyard.iter().all(|card| branch
                        .objects
                        .get(card.object)
                        .zone_change_count
                        == card.expected_zone_change_count + 1));
                }
                assert_eq!(
                    serde_json::to_vec(&state).unwrap(),
                    serde_json::to_vec(&restored).unwrap()
                );
            }
        }
    }
}
#[test]
fn physical_multi_top_duplicates_unlooked_and_changed_incarnations_refuse_read_only() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor, 5);
        let source = put(&mut state, actor, "Lightning Bolt", Zone::Hand);
        let binding = capture(&mut state, &ExecCtx::no_targets(source, actor)).unwrap();
        for answer in [
            vec![binding.original_library[0]],
            vec![binding.original_library[0], binding.original_library[0]],
            vec![binding.original_library[0], binding.original_library[3]],
        ] {
            let before = serde_json::to_vec(&state).unwrap();
            assert!(commit(&mut state, &binding, &answer).is_err());
            assert_eq!(before, serde_json::to_vec(&state).unwrap());
        }
        state
            .objects
            .get_mut(binding.original_library[0].object)
            .zone_change_count += 1;
        let before = serde_json::to_vec(&state).unwrap();
        assert!(commit(&mut state, &binding, &binding.original_library[..3]).is_err());
        assert_eq!(before, serde_json::to_vec(&state).unwrap());
    }
}
#[test]
fn private_look_reveals_only_to_captured_controller_public_projection_is_invariant() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor, 5);
        let source = put(&mut state, actor, "Lightning Bolt", Zone::Hand);
        let public = observe_v2(
            &state,
            &crate::surface_v2::HarnessSurfaceV2::new(),
            actor.opponent(),
            0,
        )
        .unwrap();
        capture(&mut state, &ExecCtx::no_targets(source, actor)).unwrap();
        assert_eq!(
            public.visible_projection_hash,
            observe_v2(
                &state,
                &crate::surface_v2::HarnessSurfaceV2::new(),
                actor.opponent(),
                0
            )
            .unwrap()
            .visible_projection_hash
        );
        let mut hidden = state.clone();
        let object = hidden.players[actor.index()].library[0];
        let alternate = crate::card_def::card_id_by_name("Counterspell").unwrap();
        hidden.objects.get_mut(object).card_def = alternate;
        hidden.objects.get_mut(object).name = "Counterspell".into();
        hidden.objects.get_mut(object).v4 = crate::state::ObjectStateV4::from_card_def(alternate);
        assert_eq!(
            observe_v2(
                &state,
                &crate::surface_v2::HarnessSurfaceV2::new(),
                actor.opponent(),
                0
            )
            .unwrap()
            .projection,
            observe_v2(
                &hidden,
                &crate::surface_v2::HarnessSurfaceV2::new(),
                actor.opponent(),
                0
            )
            .unwrap()
            .projection
        );
    }
}
