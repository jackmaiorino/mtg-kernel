use super::*;
use super::{
    surveil_many::{checkpoint, sessions},
    targeted_entry::{actions, cast, choose},
};
use crate::{
    ids::ObjectId,
    policy_observation_v6::tests::put,
    state::{GameState, Step, Target, Zone},
};

struct Scene {
    own: KaitoSession,
    native: FastActorSessionV1,
    source: ObjectId,
    creatures: [ObjectId; 2],
    land: ObjectId,
    counter: ObjectId,
    removal: ObjectId,
    walker: Option<ObjectId>,
}
fn scene(p: PlayerId, mountains: usize, crystal: bool, with_walker: bool) -> Scene {
    let forest = crate::card_def::card_id_by_name("Forest").unwrap();
    let mut s = GameState::new_from_libraries_with_starting_player_v1(
        &[forest; 40],
        &[forest; 40],
        crate::rl::card_name,
        110_268,
        p,
    );
    s.step = Step::Main1;
    for _ in 0..mountains {
        put(&mut s, p, "Mountain", Zone::Battlefield);
    }
    for _ in 0..2 {
        put(&mut s, p.opponent(), "Island", Zone::Battlefield);
    }
    for _ in 0..3 {
        put(&mut s, p.opponent(), "Swamp", Zone::Battlefield);
    }
    let creatures = [p, p.opponent()].map(|q| put(&mut s, q, "Serra Angel", Zone::Battlefield));
    let land = put(&mut s, p.opponent(), "Forest", Zone::Battlefield);
    if crystal {
        put(&mut s, p.opponent(), "Crystal Barricade", Zone::Battlefield);
    }
    let source = put(&mut s, p, "Burst Lightning", Zone::Hand);
    let counter = put(&mut s, p.opponent(), "Counterspell", Zone::Hand);
    let removal = put(&mut s, p.opponent(), "Hero's Downfall", Zone::Hand);
    let walker = with_walker.then(|| {
        for _ in 0..6 {
            put(&mut s, p, "Swamp", Zone::Battlefield);
        }
        put(&mut s, p, "Liliana, Dreadhorde General", Zone::Hand)
    });
    let (own, native) = sessions(s);
    Scene {
        own,
        native,
        source,
        creatures,
        land,
        counter,
        removal,
        walker,
    }
}
fn target(a: &A, source: ObjectId, wanted: Target) -> bool {
    match (a, wanted) {
        (
            A::ChooseTarget {
                source: s,
                target: crate::rl::TargetRefV1::Player { player },
                ..
            },
            Target::Player(p),
        ) => s.arena_id == source.0 && *player == p.into(),
        (
            A::ChooseTarget {
                source: s,
                target: crate::rl::TargetRefV1::Object { object },
                ..
            },
            Target::Object(o),
        ) => s.arena_id == source.0 && object.arena_id == o.0,
        _ => false,
    }
}
fn begin(s: &mut Scene, p: PlayerId, kicked: bool, wanted: Target, offered: bool) {
    let tapped_before = s.own.fixture_state().players[p.index()]
        .battlefield
        .iter()
        .filter(|&&o| s.own.fixture_state().objects.get(o).tapped)
        .count();
    cast(&mut s.own, &mut s.native, s.source);
    let mut selected = false;
    let mut saw_kicker = false;
    for _ in 0..4 {
        if s.own.fixture_state().engine.pending_cast.is_none() {
            break;
        }
        let menu = actions(&s.own);
        if menu.iter().any(|a| matches!(a, A::ChooseKicker { .. })) {
            assert!(offered);
            assert!(!saw_kicker);
            assert_eq!(
                menu.iter()
                    .filter(|a| matches!(a, A::ChooseKicker { .. }))
                    .count(),
                2
            );
            checkpoint(&mut s.own, &mut s.native);
            act(
                &mut s.own,
                &mut s.native,
                |a| matches!(a, A::ChooseKicker { source, pay, .. } if source.arena_id == s.source.0 && *pay == kicked),
            );
            saw_kicker = true;
        } else {
            assert!(!selected);
            for q in [PlayerId::P0, PlayerId::P1] {
                if !s.own.fixture_state().players[q.index()]
                    .battlefield
                    .iter()
                    .any(|&o| {
                        crate::card_def::CARD_DEFS
                            [usize::from(s.own.fixture_state().objects.get(o).card_def)]
                        .name
                            == "Crystal Barricade"
                    })
                {
                    assert!(menu.iter().any(|a| target(a, s.source, Target::Player(q))));
                }
            }
            assert!(s.creatures.iter().all(|&o| menu.iter().any(|a| target(
                a,
                s.source,
                Target::Object(o)
            ))));
            assert!(!menu
                .iter()
                .any(|a| target(a, s.source, Target::Object(s.land))));
            checkpoint(&mut s.own, &mut s.native);
            act(&mut s.own, &mut s.native, |a| target(a, s.source, wanted));
            selected = true;
        }
    }
    assert!(selected);
    assert_eq!(saw_kicker, offered);
    let state = s.own.fixture_state();
    assert!(state.engine.pending_cast.is_none());
    assert_eq!(state.objects.get(s.source).zone, Zone::Stack);
    let item = state.stack.last().unwrap();
    assert_eq!(item.kicked, kicked);
    assert_eq!(crate::engine::stack_spell_mana_value(state, item), 1);
    assert_eq!(state.players[p.index()].mana_pool, [0; 6]);
    assert_eq!(
        state.players[p.index()]
            .battlefield
            .iter()
            .filter(|&&o| state.objects.get(o).tapped)
            .count(),
        tapped_before + if kicked { 5 } else { 1 }
    );
}
fn finish(s: &mut Scene) {
    for _ in 0..32 {
        if s.own.fixture_state().stack.is_empty() {
            return;
        }
        act(&mut s.own, &mut s.native, |a| {
            matches!(
                a,
                A::Pass { .. } | A::OrderTriggers { .. } | A::ChooseTriggerOrderNext { .. }
            )
        });
    }
    panic!("Burst Lightning did not finish");
}
fn damage(s: &Scene, from: usize) -> Vec<(Target, i32)> {
    s.own.fixture_state().engine.event_history[from..]
        .iter()
        .filter_map(|e| match e {
            crate::event::CommittedEvent::Damage {
                source,
                target,
                amount,
            } if *source == s.source => Some((*target, *amount)),
            _ => None,
        })
        .collect()
}
#[test]
fn paid_burst_normal_any_player_or_either_creature_two_damage_single_packet_and_restore_both_seats()
{
    paid_route(false);
}
#[test]
fn paid_burst_kicked_five_mana_any_player_or_either_creature_four_damage_single_packet_and_restore_both_seats(
) {
    paid_route(true);
}
fn paid_route(kicked: bool) {
    let id = crate::card_def::card_id_by_name("Burst Lightning").unwrap();
    assert_eq!(
        super::super::super::roots::registered_spell_printing(id).unwrap(),
        Some(super::super::super::roots::baseline(id).unwrap())
    );
    for p in [PlayerId::P0, PlayerId::P1] {
        for slot in 0..4 {
            let mut s = scene(p, 5, false, false);
            let wanted = if slot < 2 {
                Target::Player([p, p.opponent()][slot])
            } else {
                Target::Object(s.creatures[slot - 2])
            };
            begin(&mut s, p, kicked, wanted, true);
            let saved = s.own.snapshot();
            let raw = s.native.snapshot_v1();
            let from = s.own.fixture_state().engine.event_history.len();
            let mut expected = None;
            for _ in 0..2 {
                s.own.restore(&saved).unwrap();
                s.native.kaito_restore_v75(&raw).unwrap();
                finish(&mut s);
                let amount = if kicked { 4 } else { 2 };
                assert_eq!(damage(&s, from), vec![(wanted, amount)]);
                let state = s.own.fixture_state();
                assert_eq!(state.objects.get(s.source).zone, Zone::Graveyard);
                match wanted {
                    Target::Player(q) => assert_eq!(state.players[q.index()].life, 20 - amount),
                    Target::Object(o) => {
                        assert_eq!(
                            state.objects.get(o).zone,
                            if kicked {
                                Zone::Graveyard
                            } else {
                                Zone::Battlefield
                            }
                        );
                        if !kicked {
                            assert_eq!(state.objects.get(o).damage, 2);
                        }
                    }
                    Target::StackItem(_) => panic!("AnyTarget cannot select a stack item"),
                }
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
fn paid_burst_counter_target_departure_crystal_prevention_and_unpayable_kicker_preserve_cost_and_restore_both_seats(
) {
    for p in [PlayerId::P0, PlayerId::P1] {
        for kicked in [false, true] {
            for branch in 0..3 {
                let mut s = scene(p, 5, branch == 2, false);
                let victim = s.creatures[1];
                begin(&mut s, p, kicked, Target::Object(victim), true);
                let saved = s.own.snapshot();
                let raw = s.native.snapshot_v1();
                let from = s.own.fixture_state().engine.event_history.len();
                let mut expected = None;
                for _ in 0..2 {
                    s.own.restore(&saved).unwrap();
                    s.native.kaito_restore_v75(&raw).unwrap();
                    if branch < 2 {
                        act(&mut s.own, &mut s.native, |a| matches!(a, A::Pass { .. }));
                        let response = if branch == 0 { s.counter } else { s.removal };
                        cast(&mut s.own, &mut s.native, response);
                        if branch == 0 {
                            act(&mut s.own, &mut s.native, |a| {
                                is_original_spell_target(a, s.source)
                            });
                        } else {
                            choose(&mut s.own, &mut s.native, response, victim);
                        }
                    }
                    finish(&mut s);
                    assert!(damage(&s, from).is_empty());
                    let state = s.own.fixture_state();
                    assert_eq!(state.objects.get(s.source).zone, Zone::Graveyard);
                    assert_eq!(
                        state.objects.get(victim).zone,
                        if branch == 1 {
                            Zone::Graveyard
                        } else {
                            Zone::Battlefield
                        }
                    );
                    assert_eq!(state.players[p.index()].mana_pool, [0; 6]);
                    if let Some(ref old) = expected {
                        assert_eq!(state, old);
                    } else {
                        expected = Some(state.clone());
                    }
                }
            }
        }
        let mut s = scene(p, 1, false, false);
        begin(&mut s, p, false, Target::Player(p.opponent()), false);
        checkpoint(&mut s.own, &mut s.native);
        let from = s.own.fixture_state().engine.event_history.len();
        finish(&mut s);
        assert_eq!(damage(&s, from), vec![(Target::Player(p.opponent()), 2)]);
    }
}

#[test]
fn paid_burst_both_kicker_routes_damage_real_paid_liliana_loyalty_and_restore_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = scene(p, 5, false, true);
        // Initial resources only. The planeswalker enters through its actual cast.
        let liliana = s.walker.unwrap();
        cast(&mut s.own, &mut s.native, liliana);
        finish(&mut s);
        assert_eq!(
            crate::planeswalker_v1::loyalty(s.own.fixture_state(), liliana),
            Some(6)
        );
        let paid = s.own.snapshot();
        let paid_raw = s.native.snapshot_v1();
        for kicked in [false, true] {
            s.own.restore(&paid).unwrap();
            s.native.kaito_restore_v75(&paid_raw).unwrap();
            begin(&mut s, p, kicked, Target::Object(liliana), true);
            let saved = s.own.snapshot();
            let raw = s.native.snapshot_v1();
            let from = s.own.fixture_state().engine.event_history.len();
            let mut expected = None;
            for _ in 0..2 {
                s.own.restore(&saved).unwrap();
                s.native.kaito_restore_v75(&raw).unwrap();
                finish(&mut s);
                assert_eq!(
                    damage(&s, from),
                    vec![(Target::Object(liliana), if kicked { 4 } else { 2 })]
                );
                let state = s.own.fixture_state();
                assert_eq!(
                    crate::planeswalker_v1::loyalty(state, liliana),
                    Some(if kicked { 2 } else { 4 })
                );
                assert_eq!(state.objects.get(liliana).zone, Zone::Battlefield);
                assert_eq!(state.objects.get(s.source).zone, Zone::Graveyard);
                if let Some(ref old) = expected {
                    assert_eq!(state, old);
                } else {
                    expected = Some(state.clone());
                }
            }
        }
    }
}
