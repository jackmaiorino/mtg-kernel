use super::*;
use super::{
    surveil_many::{checkpoint, scene, sessions},
    targeted_entry::{actions, cast},
};
use crate::{card_def::Keywords, policy_observation_v6::tests::put, state::Zone};

fn setup(
    p: PlayerId,
    empty: bool,
) -> (
    KaitoSession,
    FastActorSessionV1,
    crate::ids::ObjectId,
    crate::ids::ObjectId,
    Vec<crate::ids::ObjectId>,
    crate::ids::ObjectId,
) {
    let (mut s, source, counter, _) = scene(p, "Goblin Surprise", 40);
    for name in ["Mountain", "Forest"] {
        for _ in 0..3 {
            put(&mut s, p, name, Zone::Battlefield);
        }
    }
    let own = if empty {
        vec![]
    } else {
        vec![
            put(&mut s, p, "Savannah Lions", Zone::Battlefield),
            put(&mut s, p, "Serra Angel", Zone::Battlefield),
        ]
    };
    let other = put(&mut s, p.opponent(), "Savannah Lions", Zone::Battlefield);
    put(&mut s, p, "Llanowar Elves", Zone::Hand);
    let (a, b) = sessions(s);
    (a, b, source, counter, own, other)
}
fn select_mode(
    own: &mut KaitoSession,
    native: &mut FastActorSessionV1,
    source: crate::ids::ObjectId,
    index: u8,
) {
    cast(own, native, source);
    assert_eq!(
        actions(own)
            .iter()
            .filter_map(|a| if let A::ChooseSpellMode { mode_index, .. } = a {
                Some(*mode_index)
            } else {
                None
            })
            .collect::<Vec<_>>(),
        [0, 1]
    );
    checkpoint(own, native);
    act(
        own,
        native,
        |a| matches!(a,A::ChooseSpellMode{mode_index,..}if *mode_index==index),
    );
    assert!(own.fixture_state().engine.pending_cast.is_none());
    assert_eq!(own.fixture_state().objects.get(source).zone, Zone::Stack);
    assert_eq!(
        own.fixture_state().players[own.fixture_state().objects.get(source).owner.index()]
            .battlefield
            .iter()
            .filter(|&&o| own.fixture_state().objects.get(o).tapped)
            .count(),
        3
    );
}
fn goblins(s: &crate::state::GameState, p: PlayerId) -> Vec<crate::ids::ObjectId> {
    let id = crate::card_def::card_id_by_name("Goblin Token").unwrap();
    s.players[p.index()]
        .battlefield
        .iter()
        .copied()
        .filter(|&o| s.objects.get(o).card_def == id)
        .collect()
}
#[test]
fn paid_goblin_surprise_both_modes_current_set_later_entry_token_characteristics_expiry_and_restore_both_seats(
) {
    let id = crate::card_def::card_id_by_name("Goblin Surprise").unwrap();
    assert_eq!(
        super::super::super::roots::registered_spell_printing(id).unwrap(),
        Some(super::super::super::roots::baseline(id).unwrap())
    );
    for p in [PlayerId::P0, PlayerId::P1] {
        for mode in [0, 1] {
            let (mut own, mut native, source, _, creatures, other) = setup(p, false);
            select_mode(&mut own, &mut native, source, mode);
            let saved = own.snapshot();
            let raw = native.snapshot_v1();
            let mut expected = None;
            for _ in 0..2 {
                own.restore(&saved).unwrap();
                native.kaito_restore_v75(&raw).unwrap();
                settle(&mut own, &mut native);
                let s = own.fixture_state();
                assert_eq!(s.objects.get(source).zone, Zone::Graveyard);
                assert_eq!(s.players[p.index()].mana_pool, [0; 6]);
                assert_eq!(
                    crate::engine::checked_effective_pt(s, creatures[0]).unwrap(),
                    (2 + if mode == 0 { 2 } else { 0 }, 1)
                );
                assert_eq!(
                    crate::engine::checked_effective_pt(s, creatures[1]).unwrap(),
                    (4 + if mode == 0 { 2 } else { 0 }, 4)
                );
                assert_eq!(
                    crate::engine::checked_effective_pt(s, other).unwrap(),
                    (2, 1)
                );
                let tokens = goblins(s, p);
                assert_eq!(tokens.len(), if mode == 1 { 2 } else { 0 });
                let history = s
                    .engine
                    .event_history
                    .iter()
                    .enumerate()
                    .collect::<Vec<_>>();
                let creations = history
                    .iter()
                    .filter_map(|(i, event)| match event {
                        crate::event::CommittedEvent::CreateToken { object, .. }
                            if tokens.contains(object) =>
                        {
                            Some(*i)
                        }
                        _ => None,
                    })
                    .collect::<Vec<_>>();
                for (index, event) in history {
                    if let crate::event::CommittedEvent::GarrukEntryObservationV1 {
                        entry, ..
                    } = event
                    {
                        if tokens.contains(&entry.object) {
                            assert!(creations.iter().all(|&i| i < index));
                        }
                    }
                }
                for &o in &tokens {
                    let object = s.objects.get(o);
                    assert!(object.v4.is_token && !object.tapped && object.summoning_sick);
                    assert_eq!((object.owner, object.controller), (p, p));
                    assert_eq!(crate::engine::checked_effective_pt(s, o).unwrap(), (1, 1));
                    assert!(!crate::engine::has_effective_keyword(s, o, Keywords::HASTE));
                    assert_eq!(object.counters.plus1_plus1, 0);
                }
                let elf = own.fixture_state().players[p.index()]
                    .hand
                    .iter()
                    .copied()
                    .find(|&o| {
                        own.fixture_state().objects.get(o).card_def
                            == crate::card_def::card_id_by_name("Llanowar Elves").unwrap()
                    })
                    .unwrap();
                cast(&mut own, &mut native, elf);
                settle(&mut own, &mut native);
                assert_eq!(
                    crate::engine::checked_effective_pt(own.fixture_state(), elf).unwrap(),
                    (1, 1)
                );
                super::ordinary_equipment::reach_main(&mut own, &mut native, p.opponent());
                assert_eq!(
                    crate::engine::checked_effective_pt(own.fixture_state(), creatures[0]).unwrap(),
                    (2, 1)
                );
                assert_eq!(
                    crate::engine::checked_effective_pt(own.fixture_state(), creatures[1]).unwrap(),
                    (4, 4)
                );
                assert_eq!(goblins(own.fixture_state(), p), tokens);
                if let Some(ref s) = expected {
                    assert_eq!(own.fixture_state(), s);
                } else {
                    expected = Some(own.fixture_state().clone());
                }
            }
        }
    }
}
#[test]
fn paid_goblin_surprise_empty_set_keeps_both_modes_and_counter_skips_entire_selected_mode_both_seats(
) {
    for p in [PlayerId::P0, PlayerId::P1] {
        for mode in [0, 1] {
            let (mut own, mut native, source, counter, _, other) = setup(p, true);
            select_mode(&mut own, &mut native, source, mode);
            let saved = own.snapshot();
            let raw = native.snapshot_v1();
            for countered in [false, true] {
                let mut expected = None;
                for _ in 0..2 {
                    own.restore(&saved).unwrap();
                    native.kaito_restore_v75(&raw).unwrap();
                    if countered {
                        act(&mut own, &mut native, |a| matches!(a, A::Pass { .. }));
                        cast(&mut own, &mut native, counter);
                        act(&mut own, &mut native, |a| {
                            is_original_spell_target(a, source)
                        });
                    }
                    settle(&mut own, &mut native);
                    let s = own.fixture_state();
                    assert_eq!(s.objects.get(source).zone, Zone::Graveyard);
                    assert_eq!(
                        goblins(s, p).len(),
                        if mode == 1 && !countered { 2 } else { 0 }
                    );
                    assert_eq!(
                        crate::engine::checked_effective_pt(s, other).unwrap(),
                        (2, 1)
                    );
                    assert_eq!(
                        s.players[p.index()]
                            .battlefield
                            .iter()
                            .filter(|&&o| s.objects.get(o).tapped)
                            .count(),
                        3
                    );
                    assert_eq!(s.players[p.index()].mana_pool, [0; 6]);
                    if let Some(ref state) = expected {
                        assert_eq!(s, state);
                    } else {
                        expected = Some(s.clone());
                    }
                }
            }
        }
    }
}
