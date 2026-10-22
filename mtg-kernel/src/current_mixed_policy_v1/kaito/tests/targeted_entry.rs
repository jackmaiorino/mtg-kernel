use super::*;
use crate::{
    ids::ObjectId,
    policy_observation_v6::tests::put,
    state::{GameState, Step, Zone},
};

fn cast_is(a: &A, object: ObjectId) -> bool {
    matches!(a, A::CastSpell { source, .. } if source.arena_id == object.0)
}
fn target_is(a: &A, source: ObjectId, object: ObjectId) -> bool {
    match a {
        A::ChooseEffectTarget {
            source: s,
            target: crate::rl::TargetRefV1::Object { object: o },
            ..
        }
        | A::ChooseTarget {
            source: s,
            target: crate::rl::TargetRefV1::Object { object: o },
            ..
        } => s.arena_id == source.0 && o.arena_id == object.0,
        _ => false,
    }
}
fn actions(own: &KaitoSession) -> Vec<A> {
    let FastActorResponseV1::Decision(header) = own.current_response() else {
        panic!("entry work ended unexpectedly")
    };
    own.encode(header).unwrap().backend.actions().to_vec()
}
fn cast(own: &mut KaitoSession, native: &mut FastActorSessionV1, object: ObjectId) {
    if !actions(own).iter().any(|a| cast_is(a, object)) {
        act(own, native, |a| matches!(a, A::Pass { .. }));
    }
    act(own, native, |a| cast_is(a, object));
}
fn choose(
    own: &mut KaitoSession,
    native: &mut FastActorSessionV1,
    source: ObjectId,
    target: ObjectId,
) {
    act(own, native, |a| target_is(a, source, target));
    for _ in 0..3 {
        let done = actions(own).into_iter().find(|a| matches!(a,
            A::FinishEffectSelection { source: s, selected_count: 1, .. }
            | A::FinishTargetSelection { source: s, selected_count: 1, .. } if s.arena_id == source.0));
        let Some(done) = done else { return };
        act(own, native, |a| a == &done);
    }
    panic!("one-target selection did not finish");
}
fn wait_target(
    own: &mut KaitoSession,
    native: &mut FastActorSessionV1,
    source: ObjectId,
    target: ObjectId,
) {
    for _ in 0..128 {
        if actions(own).iter().any(|a| target_is(a, source, target)) {
            return;
        }
        act(own, native, |a| {
            matches!(
                a,
                A::Pass { .. } | A::OrderTriggers { .. } | A::ChooseTriggerOrderNext { .. }
            )
        });
    }
    panic!("paid entry did not publish its actual target choice");
}

#[test]
fn paid_pup_flash_optional_other_permanent_return_departure_counter_and_restore_both_seats() {
    paid_entry("Mischievous Pup");
}
#[test]
fn paid_bouncer_opposing_creature_owner_return_departure_counter_and_restore_both_seats() {
    paid_entry("Bigfin Bouncer");
}
#[test]
fn paid_regrower_own_permanent_graveyard_return_exile_departure_counter_and_restore_both_seats() {
    paid_entry("Elvish Regrower");
}
#[test]
fn paid_soulcaller_own_creature_graveyard_return_exile_departure_counter_and_restore_both_seats() {
    paid_entry("Vampire Soulcaller");
}

#[test]
fn paid_mandatory_entry_without_legal_targets_keeps_creature_and_cost_both_seats() {
    let forest = crate::card_def::card_id_by_name("Forest").unwrap();
    for (name, land, price) in [
        ("Bigfin Bouncer", "Island", 4),
        ("Elvish Regrower", "Forest", 4),
        ("Vampire Soulcaller", "Swamp", 5),
    ] {
        for p in [PlayerId::P0, PlayerId::P1] {
            let mut state = GameState::new_from_libraries_with_starting_player_v1(
                &[forest; 40],
                &[forest; 40],
                crate::rl::card_name,
                110_376,
                p,
            );
            state.step = Step::Main1;
            for _ in 0..price {
                put(&mut state, p, land, Zone::Battlefield);
            }
            put(&mut state, p.opponent(), "Forest", Zone::Battlefield);
            let own_creature = put(&mut state, p, "Healer's Hawk", Zone::Battlefield);
            let other_dead = put(&mut state, p.opponent(), "Healer's Hawk", Zone::Graveyard);
            let own_instant = put(&mut state, p, "Lightning Bolt", Zone::Graveyard);
            let source = put(&mut state, p, name, Zone::Hand);
            let mut native = FastActorSessionV1::from_kaito_v75_fixture_state(state.clone());
            let mut own =
                KaitoSession::from_session(FastActorSessionV1::from_kaito_v75_fixture_state(state))
                    .unwrap();
            cast(&mut own, &mut native, source);
            assert_eq!(own.fixture_state().objects.get(source).zone, Zone::Stack);
            let paid = own.snapshot();
            let raw = native.snapshot_v1();
            let mut expected = None;
            for _ in 0..2 {
                own.restore(&paid).unwrap();
                native.kaito_restore_v75(&raw).unwrap();
                settle(&mut own, &mut native);
                let s = own.fixture_state();
                assert_eq!(s.objects.get(source).zone, Zone::Battlefield);
                assert_eq!(s.objects.get(own_creature).zone, Zone::Battlefield);
                assert_eq!(s.objects.get(other_dead).zone, Zone::Graveyard);
                assert_eq!(s.objects.get(own_instant).zone, Zone::Graveyard);
                assert_eq!(s.players[p.index()].mana_pool, [0; 6]);
                assert_eq!(
                    s.players[p.index()]
                        .battlefield
                        .iter()
                        .filter(|&&o| s.objects.get(o).tapped)
                        .count(),
                    price
                );
                if let Some(ref previous) = expected {
                    assert_eq!(s, previous);
                } else {
                    expected = Some(s.clone());
                }
            }
        }
    }
}

fn paid_entry(name: &str) {
    let id = crate::card_def::card_id_by_name(name).unwrap();
    assert_eq!(
        super::super::super::roots::registered_permanent_printing(id).unwrap(),
        Some(super::super::super::roots::baseline(id).unwrap())
    );
    let pup = name == "Mischievous Pup";
    let bouncer = name == "Bigfin Bouncer";
    let regrower = name == "Elvish Regrower";
    let graveyard = !pup && !bouncer;
    let (land, price) = match name {
        "Mischievous Pup" => ("Plains", 3),
        "Bigfin Bouncer" => ("Island", 4),
        "Elvish Regrower" => ("Forest", 4),
        _ => ("Swamp", 5),
    };
    let forest = crate::card_def::card_id_by_name("Forest").unwrap();
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut state = GameState::new_from_libraries_with_starting_player_v1(
            &[forest; 40],
            &[forest; 40],
            crate::rl::card_name,
            110_375,
            if pup { p.opponent() } else { p },
        );
        state.step = Step::Main1;
        let own_land = put(&mut state, p, land, Zone::Battlefield);
        for _ in 1..price {
            put(&mut state, p, land, Zone::Battlefield);
        }
        for land in ["Island", "Swamp", "Forest"] {
            for _ in 0..3 {
                put(&mut state, p.opponent(), land, Zone::Battlefield);
            }
        }
        let other_land = state.players[p.opponent().index()].battlefield[0];
        let own_hawk = put(&mut state, p, "Healer's Hawk", Zone::Battlefield);
        let other_hawk = put(&mut state, p.opponent(), "Healer's Hawk", Zone::Battlefield);
        let own_dead = put(&mut state, p, "Healer's Hawk", Zone::Graveyard);
        let other_dead = put(&mut state, p.opponent(), "Healer's Hawk", Zone::Graveyard);
        let dead_land = put(&mut state, p, "Forest", Zone::Graveyard);
        let dead_instant = put(&mut state, p, "Lightning Bolt", Zone::Graveyard);
        let source = put(&mut state, p, name, Zone::Hand);
        let counter = put(&mut state, p.opponent(), "Counterspell", Zone::Hand);
        let downfall = put(&mut state, p.opponent(), "Hero's Downfall", Zone::Hand);
        let stab = put(&mut state, p.opponent(), "Stab", Zone::Hand);
        let ooze = put(
            &mut state,
            p.opponent(),
            "Scavenging Ooze",
            Zone::Battlefield,
        );
        let selected = if pup {
            own_hawk
        } else if bouncer {
            other_hawk
        } else {
            own_dead
        };
        let target_owner = if bouncer { p.opponent() } else { p };
        let target_zone = if graveyard {
            Zone::Graveyard
        } else {
            Zone::Battlefield
        };
        let mut native = FastActorSessionV1::from_kaito_v75_fixture_state(state.clone());
        let mut own =
            KaitoSession::from_session(FastActorSessionV1::from_kaito_v75_fixture_state(state))
                .unwrap();
        cast(&mut own, &mut native, source);
        assert_eq!(own.fixture_state().objects.get(source).zone, Zone::Stack);
        assert_eq!(
            own.fixture_state().players[p.index()]
                .battlefield
                .iter()
                .filter(|&&o| own.fixture_state().objects.get(o).tapped)
                .count(),
            price
        );
        assert_eq!(own.fixture_state().players[p.index()].mana_pool, [0; 6]);
        if pup {
            assert_eq!(own.fixture_state().active_player, p.opponent());
        }
        let saved = own.snapshot();
        let raw = native.snapshot_v1();
        // Original resolution; countered source; source removed after target
        // choice; target removed by real Stab/Ooze; optional zero-target Pup;
        // Regrower also returns a genuine land card, unlike Soulcaller.
        let responses = if pup || regrower { 5 } else { 4 };
        for response in 0..responses {
            let target = if regrower && response == 4 {
                dead_land
            } else {
                selected
            };
            let mut expected = None;
            for _ in 0..2 {
                own.restore(&saved).unwrap();
                native.kaito_restore_v75(&raw).unwrap();
                let generation = own.fixture_state().objects.get(target).zone_change_count;
                if response == 1 {
                    cast(&mut own, &mut native, counter);
                    act(&mut own, &mut native, |a| {
                        is_original_spell_target(a, source)
                    });
                    settle(&mut own, &mut native);
                } else {
                    wait_target(&mut own, &mut native, source, target);
                    let menu = actions(&own);
                    let forbidden = if pup {
                        vec![source, other_hawk, own_dead]
                    } else if bouncer {
                        vec![source, own_hawk, own_dead, other_land]
                    } else if regrower {
                        vec![source, own_hawk, other_dead, dead_instant]
                    } else {
                        vec![source, own_hawk, other_dead, dead_land, dead_instant]
                    };
                    for object in forbidden {
                        assert!(
                            !menu.iter().any(|a| target_is(a, source, object)),
                            "foreign target {object:?}"
                        );
                    }
                    if pup {
                        assert!(menu.iter().any(|a| target_is(a, source, own_land)));
                    }
                    if regrower {
                        assert!(menu.iter().any(|a| target_is(a, source, dead_land)));
                    }
                    assert!(menu.iter().filter(|a| target_is(a, source, target)).all(
                        |a| match a {
                            A::ChooseEffectTarget {
                                min_targets,
                                max_targets,
                                ..
                            } => *min_targets == u16::from(!pup) && *max_targets == 1,
                            A::ChooseTarget { remaining, .. } => *remaining == 1,
                            _ => false,
                        }
                    ));
                    let choosing = own.snapshot();
                    let choosing_raw = native.snapshot_v1();
                    own.restore(&choosing).unwrap();
                    native.kaito_restore_v75(&choosing_raw).unwrap();
                    if pup && response == 4 {
                        act(&mut own, &mut native, |a| {
                            matches!(a,
                            A::FinishEffectSelection { source: s, selected_count: 0, .. }
                            | A::FinishTargetSelection { source: s, selected_count: 0, .. } if s.arena_id == source.0)
                        });
                    } else {
                        choose(&mut own, &mut native, source, target);
                    }
                    let pending = own.snapshot();
                    let pending_raw = native.snapshot_v1();
                    own.restore(&pending).unwrap();
                    native.kaito_restore_v75(&pending_raw).unwrap();
                    if response == 2 {
                        cast(&mut own, &mut native, downfall);
                        choose(&mut own, &mut native, downfall, source);
                    }
                    if response == 3 {
                        if graveyard {
                            if !actions(&own).iter().any(|a| matches!(a, A::ActivateAbility { source: s, ability_index: 0, .. } if s.arena_id == ooze.0)) {
                                act(&mut own, &mut native, |a| matches!(a, A::Pass { .. }));
                            }
                            act(
                                &mut own,
                                &mut native,
                                |a| matches!(a, A::ActivateAbility { source: s, ability_index: 0, .. } if s.arena_id == ooze.0),
                            );
                            choose(&mut own, &mut native, ooze, target);
                        } else {
                            cast(&mut own, &mut native, stab);
                            choose(&mut own, &mut native, stab, target);
                        }
                    }
                    settle(&mut own, &mut native);
                }
                let s = own.fixture_state();
                assert_eq!(
                    s.objects.get(source).zone,
                    if response == 1 || response == 2 {
                        Zone::Graveyard
                    } else {
                        Zone::Battlefield
                    }
                );
                let returned = response != 1 && response != 3 && !(pup && response == 4);
                let target_after = s.objects.get(target);
                assert_eq!(
                    target_after.zone,
                    if returned {
                        Zone::Hand
                    } else if response == 3 && graveyard {
                        Zone::Exile
                    } else if response == 3 {
                        Zone::Graveyard
                    } else {
                        target_zone
                    }
                );
                assert_eq!(target_after.owner, target_owner);
                assert_eq!(target_after.controller, target_owner);
                assert_eq!(
                    target_after.zone_change_count,
                    generation + u32::from(returned || response == 3)
                );
                assert_eq!(s.players[p.index()].mana_pool, [0; 6]);
                assert_eq!(s.players[p.opponent().index()].mana_pool, [0; 6]);
                assert_eq!(
                    s.players[p.opponent().index()]
                        .battlefield
                        .iter()
                        .filter(|&&o| s.objects.get(o).tapped)
                        .count(),
                    match response {
                        1 => 2,
                        2 => 3,
                        3 => 1,
                        _ => 0,
                    }
                );
                assert_eq!(s.players[p.index()].life, 20);
                assert_eq!(
                    s.players[p.opponent().index()].life,
                    if response == 3 && graveyard { 21 } else { 20 }
                );
                if response == 3 && graveyard {
                    assert_eq!(s.objects.get(ooze).counters.plus1_plus1, 1);
                }
                if let Some(ref previous) = expected {
                    assert_eq!(s, previous);
                } else {
                    expected = Some(s.clone());
                }
            }
        }
    }
}
