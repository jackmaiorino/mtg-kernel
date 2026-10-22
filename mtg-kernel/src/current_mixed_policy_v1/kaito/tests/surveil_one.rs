use super::targeted_entry::{actions, cast, choose};
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
fn activation(a: &A, source: ObjectId) -> bool {
    matches!(a,A::ActivateAbility{source:s,ability_index:0,..}if s.arena_id==source.0)
}
fn scene(p: PlayerId, name: &str, empty: bool) -> (GameState, ObjectId, ObjectId, ObjectId) {
    let forest = crate::card_def::card_id_by_name("Forest").unwrap();
    // Wall's first natural controller turn draws its one remaining card.
    // Thespian never advances the turn in the empty-library branch.
    let library = vec![
        forest;
        if empty {
            usize::from(name == "Rune-Sealed Wall")
        } else {
            40
        }
    ];
    let (left, right) = if p == PlayerId::P0 {
        (library, vec![forest; 40])
    } else {
        (vec![forest; 40], library)
    };
    let mut s = GameState::new_from_libraries_with_starting_player_v1(
        &left,
        &right,
        crate::rl::card_name,
        110_394,
        p,
    );
    s.step = Step::Main1;
    for land in ["Forest", "Island"] {
        for _ in 0..3 {
            put(&mut s, p, land, Zone::Battlefield);
        }
    }
    for land in ["Island", "Swamp"] {
        for _ in 0..3 {
            put(&mut s, p.opponent(), land, Zone::Battlefield);
        }
    }
    let source = put(&mut s, p, name, Zone::Hand);
    let counter = put(&mut s, p.opponent(), "Counterspell", Zone::Hand);
    let removal = put(&mut s, p.opponent(), "Hero's Downfall", Zone::Hand);
    (s, source, counter, removal)
}
fn sessions(s: GameState) -> (KaitoSession, FastActorSessionV1) {
    let raw = FastActorSessionV1::from_kaito_v75_fixture_state(s.clone());
    let own =
        KaitoSession::from_session(FastActorSessionV1::from_kaito_v75_fixture_state(s)).unwrap();
    (own, raw)
}
fn restore(
    own: &mut KaitoSession,
    native: &mut FastActorSessionV1,
    saved: &super::super::KaitoSnapshot,
    raw: &crate::rl_session::FastActorSessionSnapshotV1,
) {
    own.restore(saved).unwrap();
    native.kaito_restore_v75(raw).unwrap();
}
fn surveil(
    own: &mut KaitoSession,
    native: &mut FastActorSessionV1,
    p: PlayerId,
    source: ObjectId,
    mill: bool,
) {
    let top = own.fixture_state().players[p.index()].library[0];
    for _ in 0..128 {
        let menu = actions(own);
        if menu
            .iter()
            .any(|a| matches!(a,A::ChooseEffectOption{source:s,..}if s.arena_id==source.0))
        {
            let choices = menu
                .iter()
                .filter_map(|a| match a {
                    A::ChooseEffectOption {
                        actor,
                        source: s,
                        option_index,
                        option_count,
                        ..
                    } if s.arena_id == source.0 => {
                        assert_eq!(*actor, crate::rl::PlayerSeatV1::from(p));
                        assert_eq!(*option_count, 2);
                        Some(*option_index)
                    }
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(choices, vec![0, 1]);
            let FastActorResponseV1::Decision(h) = own.current_response() else {
                panic!()
            };
            let d = own.encode(h).unwrap();
            assert!(
                d.backend.observation().common.common.known_library_cards[p.index()]
                    .iter()
                    .any(|row| row.position == 0 && row.card.stable.arena_id == top.0)
            );
            let saved = own.snapshot();
            let raw = native.snapshot_v1();
            restore(own, native, &saved, &raw);
            act(
                own,
                native,
                |a| matches!(a,A::ChooseEffectOption{source:s,option_index,..}if s.arena_id==source.0&&*option_index==u16::from(mill)),
            );
            assert_eq!(
                own.fixture_state().objects.get(top).zone,
                if mill { Zone::Graveyard } else { Zone::Library }
            );
            return;
        }
        act(own, native, quiet);
    }
    panic!("actual private surveil choice absent");
}
fn counter_twice(
    own: &mut KaitoSession,
    native: &mut FastActorSessionV1,
    source: ObjectId,
    counter: ObjectId,
) {
    let saved = own.snapshot();
    let raw = native.snapshot_v1();
    let libraries = own
        .fixture_state()
        .players
        .each_ref()
        .map(|player| player.library.clone());
    let mut expected = None;
    for _ in 0..2 {
        restore(own, native, &saved, &raw);
        cast(own, native, counter);
        act(own, native, |a| is_original_spell_target(a, source));
        settle(own, native);
        assert_eq!(
            own.fixture_state().objects.get(source).zone,
            Zone::Graveyard
        );
        assert_eq!(
            own.fixture_state()
                .players
                .each_ref()
                .map(|player| player.library.clone()),
            libraries
        );
        if let Some(ref prior) = expected {
            assert_eq!(own.fixture_state(), prior);
        } else {
            expected = Some(own.fixture_state().clone());
        }
    }
    restore(own, native, &saved, &raw);
}

#[test]
fn paid_thespian_entry_and_death_private_keep_mill_source_departure_empty_counter_and_restore_both_seats(
) {
    let id = crate::card_def::card_id_by_name("Wary Thespian").unwrap();
    assert_eq!(
        super::super::super::roots::registered_permanent_printing(id).unwrap(),
        Some(super::super::super::roots::baseline(id).unwrap())
    );
    for p in [PlayerId::P0, PlayerId::P1] {
        for empty in [false, true] {
            let (s, source, counter, removal) = scene(p, "Wary Thespian", empty);
            let (mut own, mut native) = sessions(s);
            cast(&mut own, &mut native, source);
            let paid = own.snapshot();
            let paid_raw = native.snapshot_v1();
            counter_twice(&mut own, &mut native, source, counter);
            for removed_before_entry in [false, true] {
                for mill in [false, true] {
                    if empty && mill {
                        continue;
                    }
                    let mut expected = None;
                    for _ in 0..2 {
                        restore(&mut own, &mut native, &paid, &paid_raw);
                        let library = own.fixture_state().players[p.index()].library.clone();
                        super::library_search::wait_entered(&mut own, &mut native, source);
                        if !removed_before_entry {
                            if !empty {
                                surveil(&mut own, &mut native, p, source, mill);
                            }
                            settle(&mut own, &mut native);
                        }
                        cast(&mut own, &mut native, removal);
                        choose(&mut own, &mut native, removal, source);
                        if !empty {
                            surveil(&mut own, &mut native, p, source, mill);
                            if removed_before_entry {
                                surveil(&mut own, &mut native, p, source, mill);
                            }
                        }
                        settle(&mut own, &mut native);
                        let s = own.fixture_state();
                        assert_eq!(s.objects.get(source).zone, Zone::Graveyard);
                        assert_eq!(
                            s.players[p.index()].library,
                            &library[if mill { 2 } else { 0 }..]
                        );
                        for &top in &library[..if mill { 2 } else { 0 }] {
                            assert_eq!(s.objects.get(top).zone, Zone::Graveyard);
                        }
                        assert_eq!(s.players[p.index()].life, 20);
                        assert_eq!(s.players[p.opponent().index()].life, 20);
                        assert_eq!(
                            crate::engine::checked_effective_pt(s, source).unwrap(),
                            (3, 1)
                        );
                        assert_eq!(s.players[p.index()].mana_pool, [0; 6]);
                        if let Some(ref prior) = expected {
                            assert_eq!(s, prior);
                        } else {
                            expected = Some(s.clone());
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn paid_wall_natural_readiness_tap_private_keep_mill_source_departure_empty_counter_and_restore_both_seats(
) {
    let id = crate::card_def::card_id_by_name("Rune-Sealed Wall").unwrap();
    assert_eq!(
        super::super::super::roots::registered_permanent_printing(id).unwrap(),
        Some(super::super::super::roots::baseline(id).unwrap())
    );
    for p in [PlayerId::P0, PlayerId::P1] {
        for empty in [false, true] {
            let (s, source, counter, removal) = scene(p, "Rune-Sealed Wall", empty);
            let (mut own, mut native) = sessions(s);
            cast(&mut own, &mut native, source);
            counter_twice(&mut own, &mut native, source, counter);
            settle(&mut own, &mut native);
            assert!(own.fixture_state().objects.get(source).summoning_sick);
            assert!(!actions(&own).iter().any(|a| activation(a, source)));
            super::ordinary_equipment::reach_main(&mut own, &mut native, p.opponent());
            super::ordinary_equipment::reach_main(&mut own, &mut native, p);
            if !actions(&own).iter().any(|a| activation(a, source)) {
                act(&mut own, &mut native, quiet);
            }
            assert!(!own.fixture_state().objects.get(source).summoning_sick);
            assert!(crate::engine::has_effective_keyword(
                own.fixture_state(),
                source,
                crate::card_def::Keywords::DEFENDER
            ));
            assert_eq!(
                crate::engine::checked_effective_pt(own.fixture_state(), source).unwrap(),
                (0, 6)
            );
            let ready = own.snapshot();
            let ready_raw = native.snapshot_v1();
            for removed in [false, true] {
                for mill in [false, true] {
                    if empty && mill {
                        continue;
                    }
                    let mut expected = None;
                    for _ in 0..2 {
                        restore(&mut own, &mut native, &ready, &ready_raw);
                        let library = own.fixture_state().players[p.index()].library.clone();
                        assert_eq!(library.is_empty(), empty);
                        let tapped = own.fixture_state().players[p.index()]
                            .battlefield
                            .iter()
                            .filter(|&&o| own.fixture_state().objects.get(o).tapped)
                            .count();
                        act(&mut own, &mut native, |a| activation(a, source));
                        assert!(own.fixture_state().objects.get(source).tapped);
                        assert_eq!(
                            own.fixture_state().players[p.index()]
                                .battlefield
                                .iter()
                                .filter(|&&o| own.fixture_state().objects.get(o).tapped)
                                .count(),
                            tapped + 1
                        );
                        let pending = own.snapshot();
                        let pending_raw = native.snapshot_v1();
                        restore(&mut own, &mut native, &pending, &pending_raw);
                        if removed {
                            cast(&mut own, &mut native, removal);
                            choose(&mut own, &mut native, removal, source);
                        }
                        if !empty {
                            surveil(&mut own, &mut native, p, source, mill);
                        }
                        settle(&mut own, &mut native);
                        let s = own.fixture_state();
                        assert_eq!(s.players[p.index()].library, &library[usize::from(mill)..]);
                        assert_eq!(
                            s.objects.get(source).zone,
                            if removed {
                                Zone::Graveyard
                            } else {
                                Zone::Battlefield
                            }
                        );
                        assert_eq!(s.players[p.index()].life, 20);
                        assert_eq!(s.players[p.opponent().index()].life, 20);
                        assert_eq!(s.players[p.index()].mana_pool, [0; 6]);
                        assert!(!actions(&own).iter().any(|a| activation(a, source)));
                        if let Some(ref prior) = expected {
                            assert_eq!(own.fixture_state(), prior);
                        } else {
                            expected = Some(own.fixture_state().clone());
                        }
                    }
                }
            }
        }
    }
}
