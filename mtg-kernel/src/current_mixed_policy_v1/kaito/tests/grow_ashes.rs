use super::*;
use super::{
    library_search::finish_zero,
    surveil_many::{checkpoint, sessions},
    targeted_entry::{actions, cast, wait_target},
};
use crate::{
    ids::ObjectId,
    policy_observation_v6::tests::put,
    state::{GameState, Step, Zone},
};

struct Scene {
    own: KaitoSession,
    native: FastActorSessionV1,
    source: ObjectId,
    counter: ObjectId,
}
fn scene(p: PlayerId, lands: usize, kind: usize) -> Scene {
    let forest = crate::card_def::card_id_by_name("Forest").unwrap();
    let library = match kind {
        0 => [
            "Forest",
            "Island",
            "Swamp",
            "Rogue's Passage",
            "Serra Angel",
        ]
        .iter()
        .map(|n| crate::card_def::card_id_by_name(n).unwrap())
        .collect::<Vec<_>>(),
        1 => ["Rogue's Passage", "Serra Angel"]
            .iter()
            .map(|n| crate::card_def::card_id_by_name(n).unwrap())
            .collect::<Vec<_>>(),
        _ => vec![],
    };
    let other = vec![forest; 20];
    let (left, right) = if p == PlayerId::P0 {
        (library, other)
    } else {
        (other, library)
    };
    let mut s = GameState::new_from_libraries_with_starting_player_v1(
        &left,
        &right,
        crate::rl::card_name,
        110_392,
        p,
    );
    s.step = Step::Main1;
    for _ in 0..lands {
        put(&mut s, p, "Forest", Zone::Battlefield);
    }
    for _ in 0..2 {
        put(&mut s, p.opponent(), "Island", Zone::Battlefield);
    }
    let source = put(&mut s, p, "Grow from the Ashes", Zone::Hand);
    let counter = put(&mut s, p.opponent(), "Counterspell", Zone::Hand);
    let (own, native) = sessions(s);
    Scene {
        own,
        native,
        source,
        counter,
    }
}
fn begin(s: &mut Scene, p: PlayerId, kicked: bool, offered: bool) {
    cast(&mut s.own, &mut s.native, s.source);
    if offered {
        assert_eq!(
            actions(&s.own)
                .iter()
                .filter(|a| matches!(a, A::ChooseKicker { .. }))
                .count(),
            2
        );
        checkpoint(&mut s.own, &mut s.native);
        act(&mut s.own, &mut s.native, |a| {
            matches!(a,
            A::ChooseKicker { source, pay, .. } if source.arena_id == s.source.0 && *pay == kicked)
        });
    } else {
        assert!(!actions(&s.own)
            .iter()
            .any(|a| matches!(a, A::ChooseKicker { .. })));
    }
    let state = s.own.fixture_state();
    assert!(state.engine.pending_cast.is_none());
    assert_eq!(state.objects.get(s.source).zone, Zone::Stack);
    assert_eq!(state.stack.last().unwrap().kicked, kicked);
    assert_eq!(crate::engine::object_mana_value(state, s.source), 3);
    assert_eq!(state.players[p.index()].mana_pool, [0; 6]);
    assert_eq!(
        state.players[p.index()]
            .battlefield
            .iter()
            .filter(|&&o| state.objects.get(o).tapped)
            .count(),
        if kicked { 5 } else { 3 }
    );
}
fn pick(s: &mut Scene, card: ObjectId) {
    act(&mut s.own, &mut s.native, |a| {
        matches!(a,
        A::ChooseEffectTarget { source, target: crate::rl::TargetRefV1::Object { object }, .. }
        if source.arena_id == s.source.0 && object.arena_id == card.0)
    });
}
fn selection(s: &mut Scene, p: PlayerId, original: &[ObjectId], count: usize, maximum: u16) {
    if count == 0 {
        finish_zero(&mut s.own, &mut s.native, s.source);
        return;
    }
    wait_target(&mut s.own, &mut s.native, s.source, original[0]);
    let mut offered = actions(&s.own)
        .into_iter()
        .filter_map(|a| match a {
            A::ChooseEffectTarget {
                actor,
                source,
                target: crate::rl::TargetRefV1::Object { object },
                min_targets,
                max_targets,
                ..
            } if source.arena_id == s.source.0 => {
                assert_eq!(actor, crate::rl::PlayerSeatV1::from(p));
                assert_eq!((min_targets, max_targets), (0, maximum));
                Some(ObjectId(object.arena_id))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    offered.sort_unstable();
    let mut basics = original[..3].to_vec();
    basics.sort_unstable();
    assert_eq!(offered, basics);
    checkpoint(&mut s.own, &mut s.native);
    for (i, &card) in original[..count].iter().enumerate() {
        pick(s, card);
        if i + 1 < count {
            // The first selected land stays in the original library until
            // the entire native batch is selected and committed.
            assert_eq!(s.own.fixture_state().players[p.index()].library, original);
            assert_eq!(s.own.fixture_state().objects.get(card).zone, Zone::Library);
            assert!(!actions(&s.own).iter().any(|a| matches!(a,
                A::ChooseEffectTarget { target: crate::rl::TargetRefV1::Object { object }, .. }
                if object.arena_id == card.0)));
            checkpoint(&mut s.own, &mut s.native);
        }
    }
    if let Some(finish) = actions(&s.own).into_iter().find(|a| {
        matches!(a,
        A::FinishEffectSelection { source, selected_count, .. }
        if source.arena_id == s.source.0 && usize::from(*selected_count) == count)
    }) {
        act(&mut s.own, &mut s.native, |a| a == &finish);
    }
}

#[test]
fn paid_grow_normal_three_mana_private_basic_filter_zero_one_counter_and_restore_both_seats() {
    paid_route(false);
}
#[test]
fn paid_grow_kicked_five_mana_zero_one_two_untapped_land_batch_partial_selection_counter_restore_both_seats(
) {
    paid_route(true);
}
fn paid_route(kicked: bool) {
    let id = crate::card_def::card_id_by_name("Grow from the Ashes").unwrap();
    assert_eq!(
        super::super::super::roots::registered_spell_printing(id).unwrap(),
        Some(super::super::super::roots::baseline(id).unwrap())
    );
    let maximum: usize = if kicked { 2 } else { 1 };
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = scene(p, 5, 0);
        let library = s.own.fixture_state().players[p.index()].library.clone();
        begin(&mut s, p, kicked, true);
        let rng = s.own.fixture_state().legacy_rng().copied();
        let saved = s.own.snapshot();
        let raw = s.native.snapshot_v1();
        for branch in 0..maximum + 2 {
            let count = branch.saturating_sub(1);
            let mut expected = None;
            for _ in 0..2 {
                s.own.restore(&saved).unwrap();
                s.native.kaito_restore_v75(&raw).unwrap();
                if branch == 0 {
                    act(&mut s.own, &mut s.native, |a| matches!(a, A::Pass { .. }));
                    cast(&mut s.own, &mut s.native, s.counter);
                    act(&mut s.own, &mut s.native, |a| {
                        is_original_spell_target(a, s.source)
                    });
                } else {
                    selection(&mut s, p, &library, count, maximum as u16);
                }
                settle(&mut s.own, &mut s.native);
                let state = s.own.fixture_state();
                assert_eq!(state.objects.get(s.source).zone, Zone::Graveyard);
                let mut remaining = state.players[p.index()].library.clone();
                remaining.sort_unstable();
                let mut wanted = library[count..].to_vec();
                wanted.sort_unstable();
                assert_eq!(remaining, wanted);
                for &card in &library[..count] {
                    assert_eq!(state.objects.get(card).zone, Zone::Battlefield);
                    assert_eq!(state.objects.get(card).controller, p);
                    assert!(!state.objects.get(card).tapped);
                }
                assert_eq!(state.players[p.index()].battlefield.len(), 5 + count);
                assert_eq!(
                    state.players[p.index()]
                        .battlefield
                        .iter()
                        .filter(|&&o| state.objects.get(o).tapped)
                        .count(),
                    if kicked { 5 } else { 3 }
                );
                if branch == 0 {
                    assert_eq!(state.players[p.index()].library, library);
                    assert_eq!(state.legacy_rng().copied(), rng);
                }
                assert!(!state.players[p.index()].has_lost);
                if let Some(ref old) = expected {
                    assert_eq!(state, old);
                } else {
                    expected = Some(state.clone());
                }
            }
        }
    }
}

#[test]
fn paid_grow_no_basic_or_empty_library_and_unpayable_kicker_keep_correct_payment_and_restore_both_seats(
) {
    for p in [PlayerId::P0, PlayerId::P1] {
        for kind in [1, 2] {
            for kicked in [false, true] {
                let mut s = scene(p, if kicked { 5 } else { 3 }, kind);
                let mut library = s.own.fixture_state().players[p.index()].library.clone();
                library.sort_unstable();
                begin(&mut s, p, kicked, kicked);
                let saved = s.own.snapshot();
                let raw = s.native.snapshot_v1();
                let mut expected = None;
                for _ in 0..2 {
                    s.own.restore(&saved).unwrap();
                    s.native.kaito_restore_v75(&raw).unwrap();
                    finish_zero(&mut s.own, &mut s.native, s.source);
                    settle(&mut s.own, &mut s.native);
                    let state = s.own.fixture_state();
                    assert_eq!(state.objects.get(s.source).zone, Zone::Graveyard);
                    let mut current = state.players[p.index()].library.clone();
                    current.sort_unstable();
                    assert_eq!(current, library);
                    assert_eq!(
                        state.players[p.index()].battlefield.len(),
                        if kicked { 5 } else { 3 }
                    );
                    assert!(!state.players[p.index()].drew_from_empty);
                    assert!(!state.players[p.index()].has_lost);
                    if let Some(ref old) = expected {
                        assert_eq!(state, old);
                    } else {
                        expected = Some(state.clone());
                    }
                }
            }
        }
    }
}
