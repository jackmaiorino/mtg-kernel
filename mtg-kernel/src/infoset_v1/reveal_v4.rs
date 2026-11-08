//! A revealed name can constrain a sampled hand without binding a native copy.
use super::*;
use crate::event::{propose_and_commit, ProposedEvent};
use crate::ids::PlayerId;
use crate::state::GameState;

fn input<'a>(
    state: &'a GameState,
    viewer: PlayerId,
    decks: &'a [Vec<u16>; 2],
) -> ExportInputV1<'a> {
    ExportInputV1 {
        state,
        journal: state.observation_journal.notes(),
        viewer,
        decklists: [&decks[0], &decks[1]],
        marks: None,
    }
}

#[test]
fn reveal_v4_export_uses_public_name_bounds_and_ignores_native_hand_copies() {
    let id = super::fixtures::id;
    let swamp = id("Swamp");
    let forest = id("Forest");
    let island = id("Island");
    for owner in [PlayerId::P0, PlayerId::P1] {
        let viewer = owner.opponent();
        let mut decks = [vec![island; 4], vec![island; 4]];
        decks[owner.index()] = vec![swamp, forest, island, swamp];
        let run = |permutation: [u16; 4], reveal_slot: usize| {
            let mut libraries = decks.clone();
            libraries[owner.index()] = permutation.to_vec();
            let mut state = GameState::new_from_libraries(
                &libraries[0],
                &libraries[1],
                crate::rl::card_name,
                543,
            );
            state.reveal_meaning_v3.enable();
            for _ in 0..2 {
                propose_and_commit(&mut state, ProposedEvent::draw(owner));
            }
            let shown = state.players[owner.index()].hand[reveal_slot];
            assert_eq!(state.objects.get(shown).card_def, swamp);
            state.reveal_hand_card(viewer, owner, shown).unwrap();
            state
        };
        let left = run([swamp, forest, island, swamp], 0);
        let right = run([island, swamp, swamp, forest], 1);
        assert_ne!(left, right);
        let a = export_reveal_v4(&input(&left, viewer, &decks)).unwrap();
        let b = export_reveal_v4(&input(&right, viewer, &decks)).unwrap();
        assert_eq!(a.schema, INFOSET_SCHEMA_REVEAL_V4);
        assert_eq!(a.bytes, b.bytes);
        assert_eq!(a.sha256, b.sha256);
        let names = a.state.known_hand_cards(viewer, owner);
        assert_eq!(names.len(), 1);
        assert_eq!(a.state.objects.get(names[0].object).card_def, swamp);
        assert!(a.state.hand_knowledge_by_name_v2);

        let previous = export_v3(&input(&left, viewer, &decks)).unwrap();
        assert_eq!(previous.schema, INFOSET_SCHEMA_V3);
        assert!(previous.state.known_hand_cards(viewer, owner).is_empty());
        let mut no_sideband = left.clone();
        no_sideband.reveal_meaning_v3 = Default::default();
        assert!(export_reveal_v4(&input(&no_sideband, viewer, &decks)).is_err());
        assert_eq!(
            previous.bytes,
            export_v3(&input(&no_sideband, viewer, &decks))
                .unwrap()
                .bytes
        );
    }
}
