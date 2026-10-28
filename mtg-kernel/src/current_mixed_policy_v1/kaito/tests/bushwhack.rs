use super::*;
use super::{
    library_search::finish_zero,
    surveil_many::{checkpoint, sessions},
    targeted_entry::{actions, cast, choose, wait_target},
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
    removal: ObjectId,
    creatures: [ObjectId; 4],
}
fn scene(p: PlayerId, library_kind: usize) -> Scene {
    let forest = crate::card_def::card_id_by_name("Forest").unwrap();
    let library = match library_kind {
        0 => ["Forest", "Island", "Rogue's Passage", "Serra Angel"]
            .iter()
            .map(|name| crate::card_def::card_id_by_name(name).unwrap())
            .collect::<Vec<_>>(),
        1 => ["Rogue's Passage", "Serra Angel"]
            .iter()
            .map(|name| crate::card_def::card_id_by_name(name).unwrap())
            .collect::<Vec<_>>(),
        _ => vec![],
    };
    let other = vec![forest; 12];
    let (left, right) = if p == PlayerId::P0 {
        (library, other)
    } else {
        (other, library)
    };
    let mut s = GameState::new_from_libraries_with_starting_player_v1(
        &left,
        &right,
        crate::rl::card_name,
        110_387,
        p,
    );
    s.step = Step::Main1;
    put(&mut s, p, "Forest", Zone::Battlefield);
    for _ in 0..2 {
        put(&mut s, p.opponent(), "Island", Zone::Battlefield);
    }
    for _ in 0..3 {
        put(&mut s, p.opponent(), "Swamp", Zone::Battlefield);
    }
    let creatures = [
        put(&mut s, p, "Healer's Hawk", Zone::Battlefield),
        put(&mut s, p, "Aegis Turtle", Zone::Battlefield),
        put(&mut s, p.opponent(), "Serra Angel", Zone::Battlefield),
        put(&mut s, p.opponent(), "Llanowar Elves", Zone::Battlefield),
    ];
    let source = put(&mut s, p, "Bushwhack", Zone::Hand);
    let counter = put(&mut s, p.opponent(), "Counterspell", Zone::Hand);
    let removal = put(&mut s, p.opponent(), "Hero's Downfall", Zone::Hand);
    let (own, native) = sessions(s);
    Scene {
        own,
        native,
        source,
        counter,
        removal,
        creatures,
    }
}
fn mode(s: &mut Scene, index: u8) {
    cast(&mut s.own, &mut s.native, s.source);
    let offered = actions(&s.own)
        .into_iter()
        .filter_map(|a| match a {
            A::ChooseSpellMode { mode_index, .. } => Some(mode_index),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(offered, [0, 1]);
    checkpoint(&mut s.own, &mut s.native);
    act(&mut s.own, &mut s.native, |a| {
        matches!(a,
        A::ChooseSpellMode { mode_index, .. } if *mode_index == index)
    });
}
fn paid(s: &Scene, p: PlayerId) {
    let state = s.own.fixture_state();
    assert!(state.engine.pending_cast.is_none());
    assert_eq!(state.objects.get(s.source).zone, Zone::Stack);
    assert_eq!(crate::engine::object_mana_value(state, s.source), 1);
    assert_eq!(state.players[p.index()].mana_pool, [0; 6]);
    assert_eq!(
        state.players[p.index()]
            .battlefield
            .iter()
            .filter(|&&o| state.objects.get(o).tapped)
            .count(),
        1
    );
}
fn targets(s: &Scene) -> Vec<ObjectId> {
    actions(&s.own)
        .into_iter()
        .filter_map(|a| match a {
            A::ChooseTarget {
                target: crate::rl::TargetRefV1::Object { object },
                ..
            } => Some(ObjectId(object.arena_id)),
            _ => None,
        })
        .collect()
}
fn pick(s: &mut Scene, target: ObjectId) {
    act(&mut s.own, &mut s.native, |a| {
        matches!(a,
        A::ChooseTarget { target: crate::rl::TargetRefV1::Object { object }, .. }
        if object.arena_id == target.0)
    });
}

#[test]
fn paid_bushwhack_search_basic_filter_fail_to_find_empty_shuffle_counter_and_restore_both_seats() {
    let id = crate::card_def::card_id_by_name("Bushwhack").unwrap();
    assert_eq!(
        super::super::super::roots::registered_spell_printing(id).unwrap(),
        Some(super::super::super::roots::baseline(id).unwrap())
    );
    for p in [PlayerId::P0, PlayerId::P1] {
        for kind in 0..3 {
            let mut s = scene(p, kind);
            let library = s.own.fixture_state().players[p.index()].library.clone();
            mode(&mut s, 0);
            paid(&s, p);
            let saved = s.own.snapshot();
            let raw = s.native.snapshot_v1();
            for branch in 0..if kind == 0 { 3 } else { 2 } {
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
                    } else if branch == 1 {
                        finish_zero(&mut s.own, &mut s.native, s.source);
                    } else {
                        wait_target(&mut s.own, &mut s.native, s.source, library[0]);
                        let mut offered = actions(&s.own)
                            .into_iter()
                            .filter_map(|a| match a {
                                A::ChooseEffectTarget {
                                    actor,
                                    target: crate::rl::TargetRefV1::Object { object },
                                    ..
                                } => {
                                    assert_eq!(actor, crate::rl::PlayerSeatV1::from(p));
                                    Some(ObjectId(object.arena_id))
                                }
                                _ => None,
                            })
                            .collect::<Vec<_>>();
                        offered.sort_unstable();
                        let mut basics = library[..2].to_vec();
                        basics.sort_unstable();
                        assert_eq!(offered, basics);
                        checkpoint(&mut s.own, &mut s.native);
                        choose(&mut s.own, &mut s.native, s.source, library[0]);
                    }
                    settle(&mut s.own, &mut s.native);
                    let state = s.own.fixture_state();
                    assert_eq!(state.objects.get(s.source).zone, Zone::Graveyard);
                    let mut remaining = state.players[p.index()].library.clone();
                    remaining.sort_unstable();
                    let mut wanted = library[usize::from(branch == 2)..].to_vec();
                    wanted.sort_unstable();
                    assert_eq!(remaining, wanted);
                    if branch == 0 {
                        assert_eq!(state.players[p.index()].library, library);
                    }
                    if branch == 2 {
                        assert_eq!(state.objects.get(library[0]).zone, Zone::Hand);
                        assert!(state.players[p.index()].hand.contains(&library[0]));
                        assert!(state
                            .known_hand_cards(p.opponent(), p)
                            .iter()
                            .any(|entry| entry.object == library[0]));
                    }
                    assert!(!state.players[p.index()].has_lost);
                    assert!(!state.players[p.index()].drew_from_empty);
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

#[test]
fn paid_bushwhack_two_controller_targets_simultaneous_creature_damage_lifelink_partial_counter_restore_both_seats(
) {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = scene(p, 0);
        let [hawk, turtle, angel, elf] = s.creatures;
        mode(&mut s, 1);
        let mut first = targets(&s);
        first.sort_unstable();
        let mut own = [hawk, turtle];
        own.sort_unstable();
        assert_eq!(first, own);
        pick(&mut s, hawk);
        let mut second = targets(&s);
        second.sort_unstable();
        let mut other = [angel, elf];
        other.sort_unstable();
        assert_eq!(second, other);
        checkpoint(&mut s.own, &mut s.native);
        pick(&mut s, angel);
        paid(&s, p);
        let saved = s.own.snapshot();
        let raw = s.native.snapshot_v1();
        for response in 0..4 {
            let mut expected = None;
            for _ in 0..2 {
                s.own.restore(&saved).unwrap();
                s.native.kaito_restore_v75(&raw).unwrap();
                if response != 0 {
                    act(&mut s.own, &mut s.native, |a| matches!(a, A::Pass { .. }));
                    if response == 1 {
                        cast(&mut s.own, &mut s.native, s.counter);
                        act(&mut s.own, &mut s.native, |a| {
                            is_original_spell_target(a, s.source)
                        });
                    } else {
                        cast(&mut s.own, &mut s.native, s.removal);
                        choose(
                            &mut s.own,
                            &mut s.native,
                            s.removal,
                            if response == 2 { hawk } else { angel },
                        );
                    }
                }
                settle(&mut s.own, &mut s.native);
                let state = s.own.fixture_state();
                assert_eq!(state.objects.get(s.source).zone, Zone::Graveyard);
                assert_eq!(
                    state.objects.get(hawk).zone,
                    if response == 0 || response == 2 {
                        Zone::Graveyard
                    } else {
                        Zone::Battlefield
                    }
                );
                assert_eq!(
                    state.objects.get(angel).zone,
                    if response == 3 {
                        Zone::Graveyard
                    } else {
                        Zone::Battlefield
                    }
                );
                assert_eq!(state.objects.get(angel).damage, u32::from(response == 0));
                assert_eq!(
                    state.players[p.index()].life,
                    if response == 0 { 21 } else { 20 }
                );
                assert_eq!(state.players[p.opponent().index()].life, 20);
                assert_eq!(state.objects.get(turtle).damage, 0);
                assert_eq!(state.objects.get(elf).damage, 0);
                if let Some(ref old) = expected {
                    assert_eq!(state, old);
                } else {
                    expected = Some(state.clone());
                }
            }
        }
    }
}
