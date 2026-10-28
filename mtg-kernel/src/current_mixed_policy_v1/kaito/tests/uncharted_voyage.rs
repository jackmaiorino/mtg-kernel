use super::*;
use super::{
    surveil_many::{checkpoint, order, sessions, wait_selection},
    targeted_entry::{actions, cast, choose},
};
use crate::{
    event::{self, ProposedEvent},
    ids::ObjectId,
    policy_observation_v6::tests::put,
    state::{GameState, Step, Zone},
};

fn scene(
    p: PlayerId,
    owner: PlayerId,
    stolen: bool,
    empty: bool,
) -> (GameState, ObjectId, ObjectId, ObjectId, ObjectId) {
    let forest = crate::card_def::card_id_by_name("Forest").unwrap();
    let own = vec![forest; if empty { 0 } else { 40 }];
    let other = vec![forest; 40];
    let (left, right) = if p == PlayerId::P0 {
        (own, other)
    } else {
        (other, own)
    };
    let mut s = GameState::new_from_libraries_with_starting_player_v1(
        &left,
        &right,
        crate::rl::card_name,
        110_386,
        p,
    );
    s.step = Step::Main1;
    for _ in 0..4 {
        put(&mut s, p, "Island", Zone::Battlefield);
    }
    for _ in 0..3 {
        put(&mut s, p.opponent(), "Island", Zone::Battlefield);
        put(&mut s, p.opponent(), "Swamp", Zone::Battlefield);
    }
    let target = put(&mut s, owner, "Aegis Turtle", Zone::Hand);
    event::propose_and_commit(
        &mut s,
        ProposedEvent::zone_change_to_battlefield_under_controller(
            target,
            if stolen { owner.opponent() } else { owner },
        ),
    );
    let source = put(&mut s, p, "Uncharted Voyage", Zone::Hand);
    let counter = put(&mut s, p.opponent(), "Counterspell", Zone::Hand);
    let removal = put(&mut s, p.opponent(), "Hero's Downfall", Zone::Hand);
    (s, source, target, counter, removal)
}

fn owner_choice(
    own: &mut KaitoSession,
    native: &mut FastActorSessionV1,
    source: ObjectId,
    owner: PlayerId,
) {
    for _ in 0..128 {
        let menu = actions(own);
        let choices = menu
            .iter()
            .filter_map(|a| match a {
                A::ChooseEffectOption {
                    source: s,
                    actor,
                    option_index,
                    option_count,
                } if s.arena_id == source.0 => {
                    assert_eq!(*actor, crate::rl::PlayerSeatV1::from(owner));
                    assert_eq!(*option_count, 2);
                    Some(*option_index)
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        if !choices.is_empty() {
            assert_eq!(choices, [0, 1]);
            checkpoint(own, native);
            return;
        }
        act(own, native, |a| {
            matches!(
                a,
                A::Pass { .. } | A::OrderTriggers { .. } | A::ChooseTriggerOrderNext { .. }
            )
        });
    }
    panic!("Uncharted Voyage owner placement choice absent");
}

#[test]
fn paid_voyage_owner_placement_then_private_current_top_keep_mill_and_restore_both_seats() {
    paid_placements(false);
}

#[test]
fn paid_voyage_initially_empty_caster_library_can_surveil_returned_own_creature_without_drawing_both_seats(
) {
    paid_placements(true);
}

fn paid_placements(empty: bool) {
    let id = crate::card_def::card_id_by_name("Uncharted Voyage").unwrap();
    assert_eq!(
        super::super::super::roots::registered_spell_printing(id).unwrap(),
        Some(super::super::super::roots::baseline(id).unwrap())
    );
    for p in [PlayerId::P0, PlayerId::P1] {
        for owner in [p, p.opponent()] {
            for stolen in [false, true] {
                let (s, source, target, _, _) = scene(p, owner, stolen, empty);
                let libraries = s.players.each_ref().map(|player| player.library.clone());
                let (mut own, mut native) = sessions(s);
                cast(&mut own, &mut native, source);
                checkpoint(&mut own, &mut native);
                choose(&mut own, &mut native, source, target);
                assert_eq!(own.fixture_state().objects.get(source).zone, Zone::Stack);
                assert_eq!(
                    crate::engine::object_mana_value(own.fixture_state(), source),
                    4
                );
                assert_eq!(own.fixture_state().players[p.index()].mana_pool, [0; 6]);
                assert_eq!(
                    own.fixture_state().players[p.index()]
                        .battlefield
                        .iter()
                        .filter(|&&o| own.fixture_state().objects.get(o).tapped)
                        .count(),
                    4
                );
                let paid = own.snapshot();
                let raw = native.snapshot_v1();
                for placement in 0..2 {
                    for mill in [false, true] {
                        let mut expected = None;
                        for _ in 0..2 {
                            own.restore(&paid).unwrap();
                            native.kaito_restore_v75(&raw).unwrap();
                            owner_choice(&mut own, &mut native, source, owner);
                            act(&mut own, &mut native, |a| {
                                matches!(a,
                                A::ChooseEffectOption { source: s, option_index, .. }
                                if s.arena_id == source.0 && *option_index == placement)
                            });
                            let mut expected_libraries = libraries.clone();
                            if placement == 0 {
                                expected_libraries[owner.index()].insert(0, target);
                            } else {
                                expected_libraries[owner.index()].push(target);
                            }
                            let mut milled = None;
                            if let Some(&top) = expected_libraries[p.index()].first() {
                                wait_selection(&mut own, &mut native, source, &[top], 0);
                                for a in actions(&own) {
                                    if let A::ChooseEffectTarget { actor, .. }
                                    | A::FinishEffectSelection { actor, .. } = a
                                    {
                                        assert_eq!(actor, crate::rl::PlayerSeatV1::from(p));
                                    }
                                }
                                order(
                                    &mut own,
                                    &mut native,
                                    source,
                                    if mill {
                                        std::slice::from_ref(&top)
                                    } else {
                                        &[]
                                    },
                                );
                                if mill {
                                    expected_libraries[p.index()].remove(0);
                                    milled = Some(top);
                                }
                            }
                            settle(&mut own, &mut native);
                            let state = own.fixture_state();
                            assert_eq!(state.objects.get(source).zone, Zone::Graveyard);
                            assert_eq!(state.objects.get(target).owner, owner);
                            assert_eq!(
                                state.objects.get(target).zone,
                                if milled == Some(target) {
                                    Zone::Graveyard
                                } else {
                                    Zone::Library
                                }
                            );
                            assert_eq!(
                                state
                                    .players
                                    .each_ref()
                                    .map(|player| player.library.clone()),
                                expected_libraries
                            );
                            if let Some(top) = milled {
                                assert_eq!(state.objects.get(top).zone, Zone::Graveyard);
                            }
                            for player in &state.players {
                                assert!(!player.has_lost);
                                assert!(!player.drew_from_empty);
                            }
                            assert_eq!((state.players[0].life, state.players[1].life), (20, 20));
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
    }
}

#[test]
fn paid_voyage_counter_and_illegal_target_skip_placement_and_private_surveil_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let (s, source, target, counter, removal) = scene(p, p.opponent(), false, false);
        let libraries = s.players.each_ref().map(|player| player.library.clone());
        let (mut own, mut native) = sessions(s);
        cast(&mut own, &mut native, source);
        choose(&mut own, &mut native, source, target);
        let paid = own.snapshot();
        let raw = native.snapshot_v1();
        for countered in [false, true] {
            let mut expected = None;
            for _ in 0..2 {
                own.restore(&paid).unwrap();
                native.kaito_restore_v75(&raw).unwrap();
                act(&mut own, &mut native, |a| matches!(a, A::Pass { .. }));
                if countered {
                    cast(&mut own, &mut native, counter);
                    act(&mut own, &mut native, |a| {
                        is_original_spell_target(a, source)
                    });
                } else {
                    cast(&mut own, &mut native, removal);
                    choose(&mut own, &mut native, removal, target);
                }
                settle(&mut own, &mut native);
                let state = own.fixture_state();
                assert_eq!(state.objects.get(source).zone, Zone::Graveyard);
                assert_eq!(
                    state.objects.get(target).zone,
                    if countered {
                        Zone::Battlefield
                    } else {
                        Zone::Graveyard
                    }
                );
                assert_eq!(
                    state
                        .players
                        .each_ref()
                        .map(|player| player.library.clone()),
                    libraries
                );
                if let Some(ref old) = expected {
                    assert_eq!(state, old);
                } else {
                    expected = Some(state.clone());
                }
            }
        }
    }
}
