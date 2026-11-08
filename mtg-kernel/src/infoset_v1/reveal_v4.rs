//! A revealed name can constrain a sampled hand without binding a native copy.
use super::*;
use crate::event::{propose_and_commit, ProposedEvent};
use crate::ids::PlayerId;
use crate::state::{GameState, Zone};

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
        assert!(a.state.reveal_knowledge_v4.is_some());
        super::payload::validate_reveal_knowledge_v4(&a.state, viewer).unwrap();

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

#[test]
fn reveal_v4_roundtrip_continuation_matches_the_public_reducer() {
    use crate::reveal_meaning_v3::RevealReducerV3;
    use std::collections::BTreeMap;

    let id = super::fixtures::id;
    let swamp = id("Swamp");
    let forest = id("Forest");
    let owner = PlayerId::P1;
    let viewer = owner.opponent();
    let decks = [vec![id("Island"); 4], vec![swamp, swamp, swamp, forest]];
    let mut raw = GameState::new_from_libraries(&decks[0], &decks[1], crate::rl::card_name, 543);
    raw.reveal_meaning_v3.enable();
    for _ in 0..4 {
        propose_and_commit(&mut raw, ProposedEvent::draw(owner));
    }
    let revealed = [
        raw.players[owner.index()].hand[0],
        raw.players[owner.index()].hand[1],
    ];
    for card in revealed {
        propose_and_commit(&mut raw, ProposedEvent::zone_change(card, Zone::Graveyard));
        propose_and_commit(&mut raw, ProposedEvent::zone_change(card, Zone::Hand));
    }

    let exported = export_reveal_v4(&input(&raw, viewer, &decks)).unwrap();
    let mut simulated: GameState =
        serde_json::from_slice(&serde_json::to_vec(&exported.state).unwrap()).unwrap();
    super::payload::validate_reveal_knowledge_v4(&simulated, viewer).unwrap();

    let compare = |raw: &GameState, simulated: &GameState, swamps| {
        let mut reducer = RevealReducerV3::default();
        reducer
            .absorb(&raw.reveal_meaning_v3.transcript(viewer).unwrap())
            .unwrap();
        let mut simulated_rows = BTreeMap::<u16, usize>::new();
        for row in simulated.known_hand_cards(viewer, owner) {
            *simulated_rows
                .entry(simulated.objects.get(row.object).card_def)
                .or_default() += 1;
        }
        assert_eq!(simulated_rows, reducer.hand(owner));
        assert_eq!(simulated_rows.get(&swamp).copied().unwrap_or(0), swamps);
    };
    compare(&raw, &simulated, 2);
    for game in [&mut raw, &mut simulated] {
        let shown: Vec<_> = game.players[owner.index()]
            .hand
            .iter()
            .copied()
            .filter(|&object| game.objects.get(object).card_def == swamp)
            .collect();
        assert_eq!(shown.len(), 3);
        game.reveal_public_hand_group_v3(owner, shown.into_iter())
            .unwrap();
    }
    compare(&raw, &simulated, 2);
    for game in [&mut raw, &mut simulated] {
        let hidden = game.players[owner.index()]
            .hand
            .iter()
            .copied()
            .find(|&object| game.objects.get(object).card_def == forest)
            .unwrap();
        propose_and_commit(game, ProposedEvent::private_bottom_library_insert(hidden));
    }
    compare(&raw, &simulated, 1);
    for game in [&mut raw, &mut simulated] {
        let shown = game.players[owner.index()].hand[0];
        propose_and_commit(game, ProposedEvent::zone_change(shown, Zone::Graveyard));
    }
    compare(&raw, &simulated, 0);
}

#[test]
fn reveal_v4_real_aborted_cast_refills_by_public_name_in_paired_worlds() {
    use crate::engine::{self, Action, Decision};
    use crate::mana::ManaColor;
    use crate::state::{Step, Target};

    let id = super::fixtures::id;
    let spell_def = id("Cast Down");
    let island = id("Island");
    let forest = id("Forest");
    let target_def = id("Aegis Turtle");
    let owner = PlayerId::P0;
    let viewer = owner.opponent();
    let decks = [
        vec![spell_def, island, spell_def, forest],
        vec![target_def, island],
    ];
    let world = |library: [u16; 4]| {
        let mut state =
            GameState::new_from_libraries(&library, &decks[1], crate::rl::card_name, 543);
        state.reveal_meaning_v3.enable();
        state.step = Step::Main1;
        for _ in 0..2 {
            propose_and_commit(&mut state, ProposedEvent::draw(owner));
        }
        let target = state.players[viewer.index()].library[0];
        propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(target, Zone::Battlefield),
        );
        let spell = state.players[owner.index()]
            .hand
            .iter()
            .copied()
            .find(|&object| state.objects.get(object).card_def == spell_def)
            .unwrap();
        state.players[owner.index()].mana_pool[ManaColor::B.pool_index()] = 2;
        assert!(matches!(engine::advance_until_decision(&mut state),
            Decision::CastSpellOrPass { castable_spells, .. } if castable_spells.contains(&spell)));
        engine::step(&mut state, Action::CastSpell(spell)).unwrap();
        assert_eq!(state.objects.get(spell).zone, Zone::Stack);
        assert!(matches!(
            engine::advance_until_decision(&mut state),
            Decision::ChooseTargets { .. }
        ));
        state.players[owner.index()].mana_pool.fill(0);
        engine::step(&mut state, Action::ChooseTarget(Target::Object(target))).unwrap();
        let _ = engine::advance_until_decision(&mut state);
        assert_eq!(state.objects.get(spell).zone, Zone::Hand);
        assert!(state.engine.pending_cast.is_none());
        state
    };
    let a = world([spell_def, island, spell_def, forest]);
    let b = world([forest, spell_def, island, spell_def]);
    assert_ne!(a.players[owner.index()].hand, b.players[owner.index()].hand);
    assert_eq!(
        a.reveal_meaning_v3.transcript(viewer),
        b.reveal_meaning_v3.transcript(viewer)
    );
    let exported = export_reveal_v4(&input(&a, viewer, &decks)).unwrap();
    assert_eq!(
        exported.bytes,
        export_reveal_v4(&input(&b, viewer, &decks)).unwrap().bytes
    );
    let known = exported.state.known_hand_cards(viewer, owner);
    assert_eq!(known.len(), 1);
    assert_eq!(
        exported.state.objects.get(known[0].object).card_def,
        spell_def
    );
    super::payload::validate_reveal_knowledge_v4(&exported.state, viewer).unwrap();
}
