//! Hidden sideboard identities and counts never enter a canonical payload.
use super::*;
use crate::eternal_outside_game_v1::set_outside_the_game;
use crate::ids::PlayerId;
use crate::state::GameState;

fn fixture() -> (GameState, [Vec<u16>; 2]) {
    let island = super::fixtures::id("Island");
    let forest = super::fixtures::id("Forest");
    let lists = std::array::from_fn(|_| vec![island, forest, island, forest]);
    let state = GameState::new_from_libraries(
        &lists[0],
        &lists[1],
        |id| crate::card_def::CARD_DEFS[id as usize].name.to_string(),
        91,
    );
    (state, lists)
}

fn exported(
    state: &GameState,
    lists: &[Vec<u16>; 2],
    viewer: PlayerId,
    v2: bool,
) -> Result<InfosetV1, ExportErrorV1> {
    let input = ExportInputV1 {
        state,
        journal: state.observation_journal.notes(),
        viewer,
        decklists: [&lists[0], &lists[1]],
        marks: None,
    };
    if v2 {
        export_v2(&input)
    } else {
        export_v1(&input, ExportOptionsV1::default())
    }
}

#[test]
fn public_outside_game_profile_refuses_without_reading_private_cards_or_counts() {
    let (base, lists) = fixture();
    let expected = ExportErrorV1::refuse(
        RefusalCodeV1::R2,
        "outside-game profile is not representable in infoset export",
    );
    for viewer in [PlayerId::P0, PlayerId::P1] {
        for v2 in [false, true] {
            exported(&base, &lists, viewer, v2).expect("legacy baseline exports");
            for hidden in [vec![], vec![lists[0][0]], vec![lists[0][1]; 3]] {
                let mut state = base.clone();
                let mut outside = [vec![], vec![]];
                outside[viewer.opponent().index()] = hidden;
                set_outside_the_game(&mut state, outside);
                assert_eq!(exported(&state, &lists, viewer, v2).unwrap_err(), expected);
                state.eternal_v1.as_mut().unwrap().outside_game.cards = [vec![], vec![]];
                let restored: GameState =
                    serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
                assert_eq!(
                    exported(&restored, &lists, viewer, v2).unwrap_err(),
                    expected
                );
            }
        }
    }
}

#[test]
fn absent_outside_game_extension_keeps_legacy_exports_after_restore() {
    let (mut state, lists) = fixture();
    state.eternal_v1 = Some(Default::default());
    let legacy = serde_json::to_value(&state).unwrap();
    assert!(legacy["eternal_v1"].get("outside_game").is_none());
    let restored: GameState = serde_json::from_value(legacy).unwrap();
    for viewer in [PlayerId::P0, PlayerId::P1] {
        for v2 in [false, true] {
            let before = exported(&state, &lists, viewer, v2).unwrap();
            let after = exported(&restored, &lists, viewer, v2).unwrap();
            assert_eq!(before.bytes, after.bytes);
            assert_eq!(before.sha256, after.sha256);
        }
    }
}

#[test]
fn historical_present_outside_game_record_retains_profile_after_depletion() {
    let (mut state, lists) = fixture();
    set_outside_the_game(&mut state, [vec![], vec![lists[0][0]]]);
    let mut historical = serde_json::to_value(&state).unwrap();
    historical["eternal_v1"]["outside_game"]
        .as_object_mut()
        .unwrap()
        .remove("enabled");
    let mut restored: GameState = serde_json::from_value(historical).unwrap();
    let outside = &mut restored.eternal_v1.as_mut().unwrap().outside_game;
    assert!(outside.enabled);
    outside.cards = [vec![], vec![]];
    for viewer in [PlayerId::P0, PlayerId::P1] {
        for v2 in [false, true] {
            assert_eq!(
                exported(&restored, &lists, viewer, v2).unwrap_err(),
                exported(&state, &lists, viewer, v2).unwrap_err()
            );
        }
    }
}
