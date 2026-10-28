use super::targeted_entry::{actions, cast, choose, wait_target};
use super::*;
use crate::{
    ids::ObjectId,
    policy_observation_v6::tests::put,
    state::{GameState, Step, Zone},
};

fn quiet(a: &A) -> bool {
    matches!(
        a,
        A::Pass { .. } | A::OrderTriggers { .. } | A::ChooseTriggerOrderNext { .. }
    )
}
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
fn base(p: PlayerId, price: usize) -> GameState {
    let forest = crate::card_def::card_id_by_name("Forest").unwrap();
    let mut state = GameState::new_from_libraries_with_starting_player_v1(
        &[forest; 40],
        &[forest; 40],
        crate::rl::card_name,
        110_380,
        p,
    );
    state.step = Step::Main1;
    for _ in 0..price {
        put(&mut state, p, "Forest", Zone::Battlefield);
    }
    for land in ["Island", "Swamp"] {
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
fn tapped(s: &GameState, p: PlayerId) -> usize {
    s.players[p.index()]
        .battlefield
        .iter()
        .filter(|&&id| s.objects.get(id).tapped)
        .count()
}
fn is_mode(a: &A, source: ObjectId, mode: u8) -> bool {
    matches!(a, A::ChooseSpellMode { source: s, mode_index, .. } if s.arena_id == source.0 && *mode_index == mode)
}
fn mode(
    own: &mut KaitoSession,
    native: &mut FastActorSessionV1,
    source: ObjectId,
    p: PlayerId,
    selected: u8,
) {
    for _ in 0..128 {
        let menu = actions(own);
        if menu.iter().any(|a| is_mode(a, source, selected)) {
            let modes = menu
                .iter()
                .filter_map(|a| match a {
                    A::ChooseSpellMode {
                        actor,
                        source: s,
                        mode_count: 2,
                        mode_index,
                    } if s.arena_id == source.0 => {
                        assert_eq!(*actor, crate::rl::PlayerSeatV1::from(p));
                        Some(*mode_index)
                    }
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(modes, [0, 1]);
            assert!(!menu.iter().any(|a| matches!(a,
                A::ChooseTarget { source: s, .. } | A::ChooseEffectTarget { source: s, .. } if s.arena_id == source.0)));
            let saved = own.snapshot();
            let raw = native.snapshot_v1();
            own.restore(&saved).unwrap();
            native.kaito_restore_v75(&raw).unwrap();
            act(own, native, |a| is_mode(a, source, selected));
            return;
        }
        act(own, native, quiet);
    }
    panic!("original placement-time mode absent");
}
fn tokens(s: &GameState, p: PlayerId) -> Vec<ObjectId> {
    let id = crate::card_def::card_id_by_name("Elf Warrior Token").unwrap();
    s.players[p.index()]
        .battlefield
        .iter()
        .copied()
        .filter(|&object| s.objects.get(object).card_def == id)
        .collect()
}
fn wait_entered_trigger(own: &mut KaitoSession, native: &mut FastActorSessionV1, source: ObjectId) {
    for _ in 0..128 {
        let s = own.fixture_state();
        if s.objects.get(source).zone == Zone::Battlefield {
            assert!(
                !s.stack.is_empty(),
                "genuine intervening-if trigger missing before response"
            );
            return;
        }
        act(own, native, quiet);
    }
    panic!("paid source did not enter before its trigger");
}

#[test]
fn paid_stomper_original_modes_before_targets_counter_source_departure_illegal_target_and_restore_both_seats(
) {
    let id = crate::card_def::card_id_by_name("Apothecary Stomper").unwrap();
    assert_eq!(
        super::super::super::roots::registered_permanent_printing(id).unwrap(),
        Some(super::super::super::roots::baseline(id).unwrap())
    );
    for p in [PlayerId::P0, PlayerId::P1] {
        let q = p.opponent();
        let mut state = base(p, 6);
        let source = put(&mut state, p, "Apothecary Stomper", Zone::Hand);
        let hawk = put(&mut state, p, "Healer's Hawk", Zone::Battlefield);
        let other = put(&mut state, q, "Healer's Hawk", Zone::Battlefield);
        let boots = put(&mut state, p, "Swiftfoot Boots", Zone::Battlefield);
        let land = state.players[p.index()].battlefield[0];
        let counter = put(&mut state, q, "Counterspell", Zone::Hand);
        let downfall = put(&mut state, q, "Hero's Downfall", Zone::Hand);
        let stab = put(&mut state, q, "Stab", Zone::Hand);
        let (mut own, mut native) = owner(state);
        cast(&mut own, &mut native, source);
        assert_eq!(own.fixture_state().objects.get(source).zone, Zone::Stack);
        assert_eq!(tapped(own.fixture_state(), p), 6);
        let paid = own.snapshot();
        let raw = native.snapshot_v1();
        for selected in [0, 1] {
            for response in 0..if selected == 0 { 5 } else { 4 } {
                let target = if response == 4 { source } else { hawk };
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
                        mode(&mut own, &mut native, source, p, selected);
                        if selected == 0 {
                            wait_target(&mut own, &mut native, source, target);
                            let menu = actions(&own);
                            for object in [source, hawk] {
                                assert!(menu.iter().any(|a| object_target(a, source, object)));
                            }
                            for object in [other, boots, land] {
                                assert!(!menu.iter().any(|a| object_target(a, source, object)));
                            }
                            assert!(!menu.iter().any(|a| matches!(a,
                                A::FinishTargetSelection { source: s, selected_count: 0, .. }
                                | A::FinishEffectSelection { source: s, selected_count: 0, .. } if s.arena_id == source.0)));
                            let choosing = own.snapshot();
                            let choosing_raw = native.snapshot_v1();
                            own.restore(&choosing).unwrap();
                            native.kaito_restore_v75(&choosing_raw).unwrap();
                            choose(&mut own, &mut native, source, target);
                        } else {
                            assert!(!actions(&own).iter().any(|a| object_target(a, source, hawk)));
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
                            cast(&mut own, &mut native, stab);
                            choose(&mut own, &mut native, stab, hawk);
                        }
                    }
                    settle(&mut own, &mut native);
                    let s = own.fixture_state();
                    assert_eq!(
                        s.objects.get(source).zone,
                        if response == 1 || response == 2 {
                            Zone::Graveyard
                        } else {
                            Zone::Battlefield
                        }
                    );
                    assert_eq!(
                        s.objects.get(hawk).zone,
                        if response == 3 {
                            Zone::Graveyard
                        } else {
                            Zone::Battlefield
                        }
                    );
                    assert_eq!(
                        s.objects.get(hawk).counters.plus1_plus1,
                        if selected == 0 && response != 1 && response != 3 && response != 4 {
                            2
                        } else {
                            0
                        }
                    );
                    assert_eq!(
                        s.objects.get(source).counters.plus1_plus1,
                        if response == 4 { 2 } else { 0 }
                    );
                    assert_eq!(
                        s.players[p.index()].life,
                        if selected == 1 && response != 1 {
                            24
                        } else {
                            20
                        }
                    );
                    assert_eq!(s.players[q.index()].life, 20);
                    assert_eq!(s.players[p.index()].mana_pool, [0; 6]);
                    assert_eq!(s.players[q.index()].mana_pool, [0; 6]);
                    assert_eq!(tapped(s, p), 6);
                    assert_eq!(
                        tapped(s, q),
                        match response {
                            1 => 2,
                            2 => 3,
                            3 => 1,
                            _ => 0,
                        }
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
}

#[test]
fn paid_stomper_vigilance_keeps_natural_next_turn_attacker_untapped_and_restores_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut state = base(p, 6);
        let source = put(&mut state, p, "Apothecary Stomper", Zone::Hand);
        let (mut own, mut native) = owner(state);
        cast(&mut own, &mut native, source);
        let paid = own.snapshot();
        let raw = native.snapshot_v1();
        let mut expected = None;
        for _ in 0..2 {
            own.restore(&paid).unwrap();
            native.kaito_restore_v75(&raw).unwrap();
            mode(&mut own, &mut native, source, p, 1);
            settle(&mut own, &mut native);
            let mut saw_opponent_turn = false;
            let mut declared = false;
            for _ in 0..512 {
                let s = own.fixture_state();
                if s.active_player == p.opponent() {
                    saw_opponent_turn = true;
                }
                if s.engine.combat.attackers_declared && s.engine.combat.attackers.contains(&source)
                {
                    assert!(saw_opponent_turn);
                    assert_eq!(s.active_player, p);
                    assert!(!s.objects.get(source).tapped);
                    assert_eq!(s.players[p.index()].life, 24);
                    declared = true;
                    break;
                }
                let menu = actions(&own);
                let attacks = |a: &A| match a {
                    A::ChooseAttackerInclusion {
                        attacker,
                        include: true,
                        ..
                    } => attacker.arena_id == source.0,
                    A::DeclareAttackers { attackers, .. } => {
                        attackers.len() == 1 && attackers[0].arena_id == source.0
                    }
                    _ => false,
                };
                if !saw_opponent_turn {
                    assert!(!menu.iter().any(attacks), "fresh Stomper gained haste");
                }
                let chosen = menu
                    .iter()
                    .find(|a| saw_opponent_turn && attacks(a))
                    .or_else(|| {
                        menu.iter().find(|a| match a {
                            A::ChooseAttackerInclusion { include: false, .. } => true,
                            A::DeclareAttackers { attackers, .. } => attackers.is_empty(),
                            _ => quiet(a),
                        })
                    })
                    .expect("natural vigilance attack action absent")
                    .clone();
                act(&mut own, &mut native, |a| a == &chosen);
            }
            assert!(declared, "natural next-turn attacker never declared");
            let s = own.fixture_state();
            let declared_snapshot = own.snapshot();
            let declared_raw = native.snapshot_v1();
            let result = s.clone();
            own.restore(&declared_snapshot).unwrap();
            native.kaito_restore_v75(&declared_raw).unwrap();
            assert_eq!(own.fixture_state(), &result);
            assert_eq!(own.fixture_state(), native.kaito_fixture_state());
            if let Some(ref previous) = expected {
                assert_eq!(&result, previous);
            } else {
                expected = Some(result);
            }
        }
    }
}

#[test]
fn paid_elite_other_elf_rechecked_after_capture_source_departure_counter_and_restore_both_seats() {
    let id = crate::card_def::card_id_by_name("Dwynen's Elite").unwrap();
    assert_eq!(
        super::super::super::roots::registered_permanent_printing(id).unwrap(),
        Some(super::super::super::roots::baseline(id).unwrap())
    );
    for p in [PlayerId::P0, PlayerId::P1] {
        let q = p.opponent();
        let mut state = base(p, 2);
        let elf = put(&mut state, p, "Elvish Regrower", Zone::Battlefield);
        let source = put(&mut state, p, "Dwynen's Elite", Zone::Hand);
        let counter = put(&mut state, q, "Counterspell", Zone::Hand);
        let downfall = put(&mut state, q, "Hero's Downfall", Zone::Hand);
        let stab = put(&mut state, q, "Stab", Zone::Hand);
        let (mut own, mut native) = owner(state);
        cast(&mut own, &mut native, source);
        assert_eq!(own.fixture_state().objects.get(source).zone, Zone::Stack);
        assert_eq!(tapped(own.fixture_state(), p), 2);
        let paid = own.snapshot();
        let raw = native.snapshot_v1();
        for response in 0..4 {
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
                    wait_entered_trigger(&mut own, &mut native, source);
                    assert!(tokens(own.fixture_state(), p).is_empty());
                    let pending = own.snapshot();
                    let pending_raw = native.snapshot_v1();
                    own.restore(&pending).unwrap();
                    native.kaito_restore_v75(&pending_raw).unwrap();
                    if response == 2 {
                        cast(&mut own, &mut native, stab);
                        choose(&mut own, &mut native, stab, source);
                    }
                    if response == 3 {
                        cast(&mut own, &mut native, downfall);
                        choose(&mut own, &mut native, downfall, elf);
                    }
                }
                settle(&mut own, &mut native);
                let s = own.fixture_state();
                let produced = tokens(s, p);
                assert_eq!(produced.len(), usize::from(response == 0 || response == 2));
                assert!(tokens(s, q).is_empty());
                for token in produced {
                    let o = s.objects.get(token);
                    assert_eq!(o.owner, p);
                    assert_eq!(o.controller, p);
                    assert_eq!(crate::engine::effective_power(s, token), 1);
                    assert_eq!(crate::engine::effective_toughness(s, token), 1);
                }
                assert_eq!(
                    s.objects.get(source).zone,
                    if response == 1 || response == 2 {
                        Zone::Graveyard
                    } else {
                        Zone::Battlefield
                    }
                );
                assert_eq!(
                    s.objects.get(elf).zone,
                    if response == 3 {
                        Zone::Graveyard
                    } else {
                        Zone::Battlefield
                    }
                );
                assert_eq!(s.players[p.index()].mana_pool, [0; 6]);
                assert_eq!(s.players[q.index()].mana_pool, [0; 6]);
                assert_eq!(tapped(s, p), 2);
                assert_eq!(
                    tapped(s, q),
                    match response {
                        1 => 2,
                        2 => 1,
                        3 => 3,
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

#[test]
fn paid_elite_self_or_opposing_elf_is_insufficient_and_later_own_elf_does_not_retroactively_trigger_both_seats(
) {
    for p in [PlayerId::P0, PlayerId::P1] {
        for other_elf in [false, true] {
            let mut state = base(p, 3);
            if other_elf {
                put(
                    &mut state,
                    p.opponent(),
                    "Elvish Regrower",
                    Zone::Battlefield,
                );
            }
            let source = put(&mut state, p, "Dwynen's Elite", Zone::Hand);
            let later = put(&mut state, p, "Llanowar Elves", Zone::Hand);
            let (mut own, mut native) = owner(state);
            cast(&mut own, &mut native, source);
            let paid = own.snapshot();
            let raw = native.snapshot_v1();
            let mut expected = None;
            for _ in 0..2 {
                own.restore(&paid).unwrap();
                native.kaito_restore_v75(&raw).unwrap();
                settle(&mut own, &mut native);
                assert!(tokens(own.fixture_state(), p).is_empty());
                cast(&mut own, &mut native, later);
                settle(&mut own, &mut native);
                let s = own.fixture_state();
                assert!(tokens(s, p).is_empty());
                assert!(tokens(s, p.opponent()).is_empty());
                assert_eq!(s.objects.get(source).zone, Zone::Battlefield);
                assert_eq!(s.objects.get(later).zone, Zone::Battlefield);
                assert_eq!(s.players[p.index()].mana_pool, [0; 6]);
                assert_eq!(tapped(s, p), 3);
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
