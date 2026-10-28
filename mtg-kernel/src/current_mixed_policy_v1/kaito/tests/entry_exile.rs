use super::targeted_entry::{actions, cast, choose, wait_target};
use super::*;
use crate::{
    ids::ObjectId,
    policy_observation_v6::tests::put,
    state::{GameState, Step, Zone},
};

fn object_target(a: &A, source: ObjectId, object: ObjectId) -> bool {
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
fn player_target(a: &A, source: ObjectId, p: PlayerId) -> bool {
    match a {
        A::ChooseEffectTarget {
            source: s,
            target: crate::rl::TargetRefV1::Player { player },
            ..
        }
        | A::ChooseTarget {
            source: s,
            target: crate::rl::TargetRefV1::Player { player },
            ..
        } => s.arena_id == source.0 && *player == crate::rl::PlayerSeatV1::from(p),
        _ => false,
    }
}
fn quiet(a: &A) -> bool {
    matches!(
        a,
        A::Pass { .. } | A::OrderTriggers { .. } | A::ChooseTriggerOrderNext { .. }
    )
}
fn zero_target(a: &A, source: ObjectId) -> bool {
    matches!(a,
        A::FinishEffectSelection { source: s, selected_count: 0, .. }
        | A::FinishTargetSelection { source: s, selected_count: 0, .. } if s.arena_id == source.0)
}
fn finish_zero(own: &mut KaitoSession, native: &mut FastActorSessionV1, source: ObjectId) {
    for _ in 0..128 {
        let state = own.fixture_state();
        if state.stack.is_empty()
            && state.engine.pending_effect.is_none()
            && state.engine.pending_triggers.is_empty()
        {
            return;
        }
        let zero = actions(own).iter().any(|a| zero_target(a, source));
        act(own, native, |a| {
            if zero {
                zero_target(a, source)
            } else {
                quiet(a)
            }
        });
    }
    panic!("zero-target entry did not finish");
}
fn select_player(
    own: &mut KaitoSession,
    native: &mut FastActorSessionV1,
    source: ObjectId,
    selected: PlayerId,
) {
    for _ in 0..128 {
        let menu = actions(own);
        if menu.iter().any(|a| player_target(a, source, selected)) {
            for p in [PlayerId::P0, PlayerId::P1] {
                assert!(menu.iter().any(|a| player_target(a, source, p)));
            }
            assert!(!menu.iter().any(|a| zero_target(a, source)));
            assert!(menu
                .iter()
                .filter(|a| player_target(a, source, selected))
                .all(|a| match a {
                    A::ChooseEffectTarget {
                        min_targets,
                        max_targets,
                        ..
                    } => *min_targets == 1 && *max_targets == 1,
                    A::ChooseTarget { remaining, .. } => *remaining == 1,
                    _ => false,
                }));
            let saved = own.snapshot();
            let raw = native.snapshot_v1();
            own.restore(&saved).unwrap();
            native.kaito_restore_v75(&raw).unwrap();
            act(own, native, |a| player_target(a, source, selected));
            return;
        }
        act(own, native, quiet);
    }
    panic!("entry did not publish the original player target");
}
fn paid_base(p: PlayerId, wolf: bool) -> GameState {
    let forest = crate::card_def::card_id_by_name("Forest").unwrap();
    let mut state = GameState::new_from_libraries_with_starting_player_v1(
        &[forest; 40],
        &[forest; 40],
        crate::rl::card_name,
        110_379,
        if wolf { p.opponent() } else { p },
    );
    state.step = Step::Main1;
    for _ in 0..if wolf { 3 } else { 4 } {
        put(
            &mut state,
            p,
            if wolf { "Forest" } else { "Plains" },
            Zone::Battlefield,
        );
    }
    for land in ["Island", "Swamp", "Forest"] {
        for _ in 0..3 {
            put(&mut state, p.opponent(), land, Zone::Battlefield);
        }
    }
    state
}
fn owner(state: GameState) -> (KaitoSession, FastActorSessionV1) {
    let native = FastActorSessionV1::from_kaito_v75_fixture_state(state.clone());
    let own = KaitoSession::from_session(FastActorSessionV1::from_kaito_v75_fixture_state(state))
        .unwrap();
    (own, native)
}
fn tapped(state: &GameState, p: PlayerId) -> usize {
    state.players[p.index()]
        .battlefield
        .iter()
        .filter(|&&id| state.objects.get(id).tapped)
        .count()
}

#[test]
fn paid_wolf_flash_any_graveyard_card_optional_zero_departure_counter_and_restore_both_seats() {
    let id = crate::card_def::card_id_by_name("Ambush Wolf").unwrap();
    assert_eq!(
        super::super::super::roots::registered_permanent_printing(id).unwrap(),
        Some(super::super::super::roots::baseline(id).unwrap())
    );
    for p in [PlayerId::P0, PlayerId::P1] {
        let q = p.opponent();
        let mut state = paid_base(p, true);
        let source = put(&mut state, p, "Ambush Wolf", Zone::Hand);
        let own_dead = put(&mut state, p, "Healer's Hawk", Zone::Graveyard);
        let other_dead = put(&mut state, q, "Healer's Hawk", Zone::Graveyard);
        let dead_land = put(&mut state, p, "Forest", Zone::Graveyard);
        let dead_instant = put(&mut state, q, "Lightning Bolt", Zone::Graveyard);
        let live = put(&mut state, p, "Healer's Hawk", Zone::Battlefield);
        let counter = put(&mut state, q, "Counterspell", Zone::Hand);
        let downfall = put(&mut state, q, "Hero's Downfall", Zone::Hand);
        let ooze = put(&mut state, q, "Scavenging Ooze", Zone::Battlefield);
        let (mut own, mut native) = owner(state);
        cast(&mut own, &mut native, source);
        assert_eq!(own.fixture_state().active_player, q);
        assert_eq!(own.fixture_state().objects.get(source).zone, Zone::Stack);
        assert_eq!(tapped(own.fixture_state(), p), 3);
        let paid = own.snapshot();
        let raw = native.snapshot_v1();
        for response in 0..7 {
            let target = match response {
                2 => other_dead,
                5 => dead_instant,
                6 => dead_land,
                _ => own_dead,
            };
            let mut expected = None;
            for _ in 0..2 {
                own.restore(&paid).unwrap();
                native.kaito_restore_v75(&raw).unwrap();
                let generation = own.fixture_state().objects.get(target).zone_change_count;
                if response == 1 {
                    cast(&mut own, &mut native, counter);
                    act(&mut own, &mut native, |a| {
                        is_original_spell_target(a, source)
                    });
                } else {
                    wait_target(&mut own, &mut native, source, target);
                    let menu = actions(&own);
                    for card in [own_dead, other_dead, dead_land, dead_instant] {
                        assert!(menu.iter().any(|a| object_target(a, source, card)));
                    }
                    for card in [source, live, counter, ooze] {
                        assert!(!menu.iter().any(|a| object_target(a, source, card)));
                    }
                    assert!(menu.iter().any(|a| zero_target(a, source)));
                    assert!(menu
                        .iter()
                        .filter(|a| object_target(a, source, target))
                        .all(|a| match a {
                            A::ChooseEffectTarget {
                                min_targets,
                                max_targets,
                                ..
                            } => *min_targets == 0 && *max_targets == 1,
                            A::ChooseTarget { remaining, .. } => *remaining == 1,
                            _ => false,
                        }));
                    let choosing = own.snapshot();
                    let choosing_raw = native.snapshot_v1();
                    own.restore(&choosing).unwrap();
                    native.kaito_restore_v75(&choosing_raw).unwrap();
                    if response == 4 {
                        act(&mut own, &mut native, |a| zero_target(a, source));
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
                        if !actions(&own).iter().any(|a| matches!(a, A::ActivateAbility { source: s, ability_index: 0, .. } if s.arena_id == ooze.0)) {
                            act(&mut own, &mut native, |a| matches!(a, A::Pass { .. }));
                        }
                        act(
                            &mut own,
                            &mut native,
                            |a| matches!(a, A::ActivateAbility { source: s, ability_index: 0, .. } if s.arena_id == ooze.0),
                        );
                        choose(&mut own, &mut native, ooze, target);
                    }
                }
                settle(&mut own, &mut native);
                let s = own.fixture_state();
                let exiled = response != 1 && response != 4;
                assert_eq!(
                    s.objects.get(target).zone,
                    if exiled { Zone::Exile } else { Zone::Graveyard }
                );
                assert_eq!(
                    s.objects.get(target).zone_change_count,
                    generation + u32::from(exiled)
                );
                assert_eq!(
                    s.objects.get(target).owner,
                    if matches!(response, 2 | 5) { q } else { p }
                );
                for card in [own_dead, other_dead, dead_land, dead_instant] {
                    if card != target {
                        assert_eq!(s.objects.get(card).zone, Zone::Graveyard);
                    }
                }
                assert_eq!(
                    s.objects.get(source).zone,
                    if response == 1 || response == 2 {
                        Zone::Graveyard
                    } else {
                        Zone::Battlefield
                    }
                );
                assert_eq!(s.players[p.index()].mana_pool, [0; 6]);
                assert_eq!(s.players[q.index()].mana_pool, [0; 6]);
                assert_eq!(tapped(s, p), 3);
                assert_eq!(
                    tapped(s, q),
                    match response {
                        1 => 2,
                        2 => 3,
                        3 => 1,
                        _ => 0,
                    }
                );
                assert_eq!(s.players[p.index()].life, 20);
                assert_eq!(
                    s.players[q.index()].life,
                    if response == 3 { 21 } else { 20 }
                );
                assert_eq!(
                    s.objects.get(ooze).counters.plus1_plus1,
                    i32::from(response == 3)
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

#[test]
fn paid_angel_each_player_current_graveyard_late_arrival_source_departure_counter_and_restore_both_seats(
) {
    let id = crate::card_def::card_id_by_name("Angel of Finality").unwrap();
    assert_eq!(
        super::super::super::roots::registered_permanent_printing(id).unwrap(),
        Some(super::super::super::roots::baseline(id).unwrap())
    );
    for p in [PlayerId::P0, PlayerId::P1] {
        let q = p.opponent();
        let mut state = paid_base(p, false);
        let source = put(&mut state, p, "Angel of Finality", Zone::Hand);
        let mut dead = Vec::new();
        for player in [p, q] {
            for name in ["Healer's Hawk", "Forest", "Lightning Bolt"] {
                dead.push((player, put(&mut state, player, name, Zone::Graveyard)));
            }
        }
        let live = [
            put(&mut state, p, "Healer's Hawk", Zone::Battlefield),
            put(&mut state, q, "Healer's Hawk", Zone::Battlefield),
        ];
        let counter = put(&mut state, q, "Counterspell", Zone::Hand);
        let downfall = put(&mut state, q, "Hero's Downfall", Zone::Hand);
        let stab = put(&mut state, q, "Stab", Zone::Hand);
        let (mut own, mut native) = owner(state);
        cast(&mut own, &mut native, source);
        assert_eq!(own.fixture_state().objects.get(source).zone, Zone::Stack);
        assert_eq!(tapped(own.fixture_state(), p), 4);
        let paid = own.snapshot();
        let raw = native.snapshot_v1();
        for selected in [p, q] {
            for response in 0..4 {
                let arriving = live[usize::from(selected == q)];
                let mut expected = None;
                for _ in 0..2 {
                    own.restore(&paid).unwrap();
                    native.kaito_restore_v75(&raw).unwrap();
                    if response == 1 {
                        cast(&mut own, &mut native, counter);
                        act(&mut own, &mut native, |a| {
                            is_original_spell_target(a, source)
                        });
                    } else {
                        select_player(&mut own, &mut native, source, selected);
                        let pending = own.snapshot();
                        let pending_raw = native.snapshot_v1();
                        own.restore(&pending).unwrap();
                        native.kaito_restore_v75(&pending_raw).unwrap();
                        if response == 2 {
                            cast(&mut own, &mut native, downfall);
                            choose(&mut own, &mut native, downfall, source);
                        }
                        if response == 3 {
                            cast(&mut own, &mut native, stab);
                            choose(&mut own, &mut native, stab, arriving);
                        }
                    }
                    settle(&mut own, &mut native);
                    let s = own.fixture_state();
                    for &(player, card) in &dead {
                        assert_eq!(
                            s.objects.get(card).zone,
                            if response != 1 && player == selected {
                                Zone::Exile
                            } else {
                                Zone::Graveyard
                            }
                        );
                        assert_eq!(s.objects.get(card).owner, player);
                    }
                    assert_eq!(
                        s.objects.get(source).zone,
                        match response {
                            1 => Zone::Graveyard,
                            2 if selected == p => Zone::Exile,
                            2 => Zone::Graveyard,
                            _ => Zone::Battlefield,
                        }
                    );
                    if response == 2 {
                        assert_eq!(
                            s.objects.get(downfall).zone,
                            if selected == q {
                                Zone::Exile
                            } else {
                                Zone::Graveyard
                            }
                        );
                    }
                    if response == 3 {
                        assert_eq!(s.objects.get(arriving).zone, Zone::Exile);
                        assert_eq!(
                            s.objects.get(stab).zone,
                            if selected == q {
                                Zone::Exile
                            } else {
                                Zone::Graveyard
                            }
                        );
                    }
                    assert_eq!(s.players[p.index()].mana_pool, [0; 6]);
                    assert_eq!(s.players[q.index()].mana_pool, [0; 6]);
                    assert_eq!(tapped(s, p), 4);
                    assert_eq!(
                        tapped(s, q),
                        match response {
                            1 => 2,
                            2 => 3,
                            3 => 1,
                            _ => 0,
                        }
                    );
                    assert_eq!(s.players[p.index()].life, 20);
                    assert_eq!(s.players[q.index()].life, 20);
                    if let Some(ref previous) = expected {
                        assert_eq!(s, previous);
                    } else {
                        expected = Some(s.clone());
                    }
                }
            }
        }
    }
}

#[test]
fn paid_entry_exile_empty_graveyards_keep_optional_zero_or_required_player_and_cost_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        for wolf in [true, false] {
            for selected in [p, p.opponent()].into_iter().take(if wolf { 1 } else { 2 }) {
                let mut state = paid_base(p, wolf);
                let source = put(
                    &mut state,
                    p,
                    if wolf {
                        "Ambush Wolf"
                    } else {
                        "Angel of Finality"
                    },
                    Zone::Hand,
                );
                let (mut own, mut native) = owner(state);
                cast(&mut own, &mut native, source);
                let saved = own.snapshot();
                let raw = native.snapshot_v1();
                let mut expected = None;
                for _ in 0..2 {
                    own.restore(&saved).unwrap();
                    native.kaito_restore_v75(&raw).unwrap();
                    if wolf {
                        finish_zero(&mut own, &mut native, source);
                    } else {
                        select_player(&mut own, &mut native, source, selected);
                        settle(&mut own, &mut native);
                    }
                    let s = own.fixture_state();
                    assert_eq!(s.objects.get(source).zone, Zone::Battlefield);
                    assert!(s.players.iter().all(|player| player.graveyard.is_empty()));
                    assert_eq!(s.players[p.index()].mana_pool, [0; 6]);
                    assert_eq!(tapped(s, p), if wolf { 3 } else { 4 });
                    assert_eq!(s.players[p.index()].life, 20);
                    assert_eq!(s.players[p.opponent().index()].life, 20);
                    if let Some(ref previous) = expected {
                        assert_eq!(s, previous);
                    } else {
                        expected = Some(s.clone());
                    }
                }
            }
        }
    }
}
