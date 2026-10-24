use super::*;
use crate::{
    event::CommittedEvent,
    ids::ObjectId,
    policy_observation_v6::tests::put,
    state::{GameState, Step, Zone},
};

fn is_player_target(a: &A, source: ObjectId, p: PlayerId) -> bool {
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
fn select_opponent_after_restore(
    own: &mut KaitoSession,
    native: &mut FastActorSessionV1,
    source: ObjectId,
    p: PlayerId,
) {
    let FastActorResponseV1::Decision(header) = own.current_response() else {
        panic!("missing Bloodwitch target decision");
    };
    let actions = own.encode(header).unwrap().backend.actions().to_vec();
    assert!(
        !actions.iter().any(|a| is_player_target(a, source, p)),
        "Bloodwitch cannot target its captured controller"
    );
    assert!(actions
        .iter()
        .any(|a| is_player_target(a, source, p.opponent())));
    assert!(actions
        .iter()
        .filter(|a| is_player_target(a, source, p.opponent()))
        .all(|a| match a {
            A::ChooseEffectTarget {
                min_targets,
                max_targets,
                ..
            } => *min_targets == 1 && *max_targets == 1,
            A::ChooseTarget { remaining, .. } => *remaining == 1,
            _ => false,
        }));
    let pending = own.snapshot();
    let raw = native.snapshot_v1();
    own.restore(&pending).unwrap();
    native.kaito_restore_v75(&raw).unwrap();
    act(own, native, |a| is_player_target(a, source, p.opponent()));
}
fn wait_for_target(
    own: &mut KaitoSession,
    native: &mut FastActorSessionV1,
    source: ObjectId,
    p: PlayerId,
) {
    for _ in 0..128 {
        let FastActorResponseV1::Decision(header) = own.current_response() else {
            panic!("death work ended before required target");
        };
        if own
            .encode(header)
            .unwrap()
            .backend
            .actions()
            .iter()
            .any(|a| is_player_target(a, source, p.opponent()))
        {
            return;
        }
        act(own, native, quiet);
    }
    panic!("actual death did not publish Bloodwitch target choice");
}
fn finish_death_work(
    own: &mut KaitoSession,
    native: &mut FastActorSessionV1,
    source: ObjectId,
    p: PlayerId,
) -> usize {
    let mut targets = 0;
    for _ in 0..256 {
        let FastActorResponseV1::Decision(header) = own.current_response() else {
            panic!("death work ended unexpectedly");
        };
        let target = own
            .encode(header)
            .unwrap()
            .backend
            .actions()
            .iter()
            .any(|a| is_player_target(a, source, p.opponent()));
        if target {
            select_opponent_after_restore(own, native, source, p);
            targets += 1;
        } else {
            let s = own.fixture_state();
            if s.stack.is_empty()
                && s.engine.pending_effect.is_none()
                && s.engine.pending_triggers.is_empty()
            {
                let actions = own.encode(header).unwrap().backend.actions().to_vec();
                if !actions.iter().any(|a| {
                    matches!(
                        a,
                        A::OrderTriggers { .. } | A::ChooseTriggerOrderNext { .. }
                    )
                }) {
                    return targets;
                }
            }
            act(own, native, quiet);
        }
    }
    panic!("Bloodwitch death work did not finish");
}
fn stab(
    own: &mut KaitoSession,
    native: &mut FastActorSessionV1,
    spell: ObjectId,
    victim: ObjectId,
) {
    act(
        own,
        native,
        |a| matches!(a,A::CastSpell{source,..}if source.arena_id==spell.0),
    );
    act(
        own,
        native,
        |a| matches!(a,A::ChooseTarget{target:crate::rl::TargetRefV1::Object{object},..}if object.arena_id==victim.0),
    );
}

#[test]
fn paid_bloodwitch_controlled_and_self_death_targeted_life_simultaneous_batch_departure_counter_and_restore_both_seats(
) {
    let id = crate::card_def::card_id_by_name("Vengeful Bloodwitch").unwrap();
    assert_eq!(
        super::super::super::roots::registered_permanent_printing(id).unwrap(),
        Some(super::super::super::roots::baseline(id).unwrap())
    );
    let forest = crate::card_def::card_id_by_name("Forest").unwrap();
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut state = GameState::new_from_libraries_with_starting_player_v1(
            &[forest; 40],
            &[forest; 40],
            crate::rl::card_name,
            110_371,
            p,
        );
        state.step = Step::Main1;
        for _ in 0..5 {
            put(&mut state, p, "Swamp", Zone::Battlefield);
        }
        for _ in 0..4 {
            put(&mut state, p, "Plains", Zone::Battlefield);
        }
        for land in ["Island", "Swamp"] {
            for _ in 0..3 {
                put(&mut state, p.opponent(), land, Zone::Battlefield);
            }
        }
        let own_hawk = put(&mut state, p, "Healer's Hawk", Zone::Battlefield);
        let opposing_hawk = put(&mut state, p.opponent(), "Healer's Hawk", Zone::Battlefield);
        let source = put(&mut state, p, "Vengeful Bloodwitch", Zone::Hand);
        let first_stab = put(&mut state, p, "Stab", Zone::Hand);
        let second_stab = put(&mut state, p, "Stab", Zone::Hand);
        let day = put(&mut state, p, "Day of Judgment", Zone::Hand);
        let counter = put(&mut state, p.opponent(), "Counterspell", Zone::Hand);
        let removal = put(&mut state, p.opponent(), "Stab", Zone::Hand);
        let mut native = FastActorSessionV1::from_kaito_v75_fixture_state(state.clone());
        let mut own =
            KaitoSession::from_session(FastActorSessionV1::from_kaito_v75_fixture_state(state))
                .unwrap();
        act(
            &mut own,
            &mut native,
            |a| matches!(a,A::CastSpell{source:s,..}if s.arena_id==source.0),
        );
        assert_eq!(own.fixture_state().objects.get(source).zone, Zone::Stack);
        assert_eq!(
            own.fixture_state().players[p.index()]
                .battlefield
                .iter()
                .filter(|&&o| own.fixture_state().objects.get(o).tapped)
                .count(),
            2
        );
        let saved = own.snapshot();
        let raw = native.snapshot_v1();
        for scene in [0, 1, 2, 3, 4] {
            let mut expected = None;
            for _ in 0..2 {
                own.restore(&saved).unwrap();
                native.kaito_restore_v75(&raw).unwrap();
                if scene == 0 {
                    act(&mut own, &mut native, |a| matches!(a, A::Pass { .. }));
                    act(
                        &mut own,
                        &mut native,
                        |a| matches!(a,A::CastSpell{source:s,..}if s.arena_id==counter.0),
                    );
                    act(&mut own, &mut native, |a| {
                        is_original_spell_target(a, source)
                    });
                }
                settle(&mut own, &mut native);
                assert_eq!(
                    own.fixture_state().objects.get(source).zone,
                    if scene == 0 {
                        Zone::Graveyard
                    } else {
                        Zone::Battlefield
                    }
                );
                let targets = match scene {
                    0 => {
                        stab(&mut own, &mut native, first_stab, own_hawk);
                        finish_death_work(&mut own, &mut native, source, p)
                    }
                    1 => {
                        stab(&mut own, &mut native, first_stab, opposing_hawk);
                        assert_eq!(finish_death_work(&mut own, &mut native, source, p), 0);
                        assert_eq!(own.fixture_state().players[p.index()].life, 20);
                        assert_eq!(own.fixture_state().players[p.opponent().index()].life, 20);
                        stab(&mut own, &mut native, second_stab, own_hawk);
                        finish_death_work(&mut own, &mut native, source, p)
                    }
                    2 => {
                        stab(&mut own, &mut native, first_stab, own_hawk);
                        wait_for_target(&mut own, &mut native, source, p);
                        assert_eq!(
                            own.fixture_state().objects.get(own_hawk).zone,
                            Zone::Graveyard
                        );
                        select_opponent_after_restore(&mut own, &mut native, source, p);
                        act(&mut own, &mut native, |a| matches!(a, A::Pass { .. }));
                        stab(&mut own, &mut native, removal, source);
                        1 + finish_death_work(&mut own, &mut native, source, p)
                    }
                    3 => {
                        act(
                            &mut own,
                            &mut native,
                            |a| matches!(a,A::CastSpell{source:s,..}if s.arena_id==day.0),
                        );
                        finish_death_work(&mut own, &mut native, source, p)
                    }
                    _ => {
                        stab(&mut own, &mut native, first_stab, source);
                        finish_death_work(&mut own, &mut native, source, p)
                    }
                };
                let count = match scene {
                    0 => 0,
                    1 | 4 => 1,
                    _ => 2,
                };
                assert_eq!(targets, count);
                let s = own.fixture_state();
                assert_eq!(s.players[p.index()].life, 20 + count as i32);
                assert_eq!(s.players[p.opponent().index()].life, 20 - count as i32);
                assert_eq!(
                    s.objects.get(source).zone,
                    if scene == 1 {
                        Zone::Battlefield
                    } else {
                        Zone::Graveyard
                    }
                );
                assert_eq!(
                    s.objects.get(own_hawk).zone,
                    if scene == 4 {
                        Zone::Battlefield
                    } else {
                        Zone::Graveyard
                    }
                );
                assert_eq!(
                    s.objects.get(opposing_hawk).zone,
                    if scene == 1 || scene == 3 {
                        Zone::Graveyard
                    } else {
                        Zone::Battlefield
                    }
                );
                assert_eq!(
                    s.players[p.index()]
                        .battlefield
                        .iter()
                        .filter(|&&o| s.objects.get(o).tapped)
                        .count(),
                    match scene {
                        1 => 4,
                        3 => 6,
                        _ => 3,
                    }
                );
                assert_eq!(s.players[p.index()].mana_pool, [0; 6]);
                assert!(
                    !s.engine.event_history.iter().any(
                        |e| matches!(e,CommittedEvent::Damage{source:actual,..}if *actual==source)
                    ),
                    "life loss must not become damage"
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
