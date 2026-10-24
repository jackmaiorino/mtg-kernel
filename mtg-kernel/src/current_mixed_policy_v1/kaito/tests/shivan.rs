use super::*;
use crate::{
    card_def::Keywords,
    policy_observation_v6::tests::put,
    state::{GameState, Step, Zone},
};

#[test]
fn paid_shivan_repeat_red_pumps_sickness_departure_reanimation_counter_cleanup_and_restore_both_seats(
) {
    let id = crate::card_def::card_id_by_name("Shivan Dragon").unwrap();
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
            110_369,
            p,
        );
        state.step = Step::Main1;
        for _ in 0..12 {
            put(&mut state, p, "Mountain", Zone::Battlefield);
        }
        // Preserve actual black payment after any legal generic-mana selection.
        for _ in 0..8 {
            put(&mut state, p, "Swamp", Zone::Battlefield);
        }
        for land in ["Island", "Swamp"] {
            for _ in 0..3 {
                put(&mut state, p.opponent(), land, Zone::Battlefield);
            }
        }
        let source = put(&mut state, p, "Shivan Dragon", Zone::Hand);
        let zombify = put(&mut state, p, "Zombify", Zone::Hand);
        let counter = put(&mut state, p.opponent(), "Counterspell", Zone::Hand);
        let removal = put(&mut state, p.opponent(), "Hero's Downfall", Zone::Hand);
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
            6
        );
        let saved = own.snapshot();
        let raw = native.snapshot_v1();
        for (countered, removed, pumps) in [
            (true, false, 0),
            (false, false, 0),
            (false, false, 1),
            (false, false, 3),
            (false, true, 1),
            (false, true, 3),
        ] {
            let mut expected = None;
            for _ in 0..2 {
                own.restore(&saved).unwrap();
                native.kaito_restore_v75(&raw).unwrap();
                if countered {
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
                    if countered {
                        Zone::Graveyard
                    } else {
                        Zone::Battlefield
                    }
                );
                let original_generation = own.fixture_state().objects.get(source).zone_change_count;
                if !countered {
                    assert!(own.fixture_state().objects.get(source).summoning_sick);
                    assert_eq!(
                        crate::engine::checked_effective_pt(own.fixture_state(), source).unwrap(),
                        (5, 5)
                    );
                    for _ in 0..pumps {
                        act(
                            &mut own,
                            &mut native,
                            |a| matches!(a,A::ActivateAbility{source:s,ability_index:0,..}if s.arena_id==source.0),
                        );
                    }
                    if pumps != 0 {
                        let pending = own.snapshot();
                        let pending_raw = native.snapshot_v1();
                        own.restore(&pending).unwrap();
                        native.kaito_restore_v75(&pending_raw).unwrap();
                    }
                    if removed {
                        act(&mut own, &mut native, |a| matches!(a, A::Pass { .. }));
                        act(
                            &mut own,
                            &mut native,
                            |a| matches!(a,A::CastSpell{source:s,..}if s.arena_id==removal.0),
                        );
                        act(
                            &mut own,
                            &mut native,
                            |a| matches!(a,A::ChooseTarget{target:crate::rl::TargetRefV1::Object{object},..}if object.arena_id==source.0),
                        );
                    }
                    settle(&mut own, &mut native);
                    if removed {
                        assert_eq!(
                            own.fixture_state().objects.get(source).zone,
                            Zone::Graveyard
                        );
                        assert!(own.fixture_state().engine.until_end_of_turn.is_empty());
                    } else {
                        assert_eq!(
                            crate::engine::checked_effective_pt(own.fixture_state(), source)
                                .unwrap(),
                            (5 + pumps, 5)
                        );
                    }
                }
                assert_eq!(
                    own.fixture_state().players[p.index()]
                        .battlefield
                        .iter()
                        .filter(|&&o| own.fixture_state().objects.get(o).tapped)
                        .count(),
                    6 + pumps as usize
                );
                assert_eq!(own.fixture_state().players[p.index()].mana_pool, [0; 6]);
                if removed {
                    act(
                        &mut own,
                        &mut native,
                        |a| matches!(a,A::CastSpell{source:s,..}if s.arena_id==zombify.0),
                    );
                    act(
                        &mut own,
                        &mut native,
                        |a| matches!(a,A::ChooseTarget{target:crate::rl::TargetRefV1::Object{object},..}if object.arena_id==source.0&&object.zone==Zone::Graveyard),
                    );
                    settle(&mut own, &mut native);
                    let live = own.fixture_state().objects.get(source);
                    assert_eq!(live.zone, Zone::Battlefield);
                    assert!(live.zone_change_count > original_generation);
                    assert!(live.summoning_sick);
                    assert_eq!(
                        crate::engine::checked_effective_pt(own.fixture_state(), source).unwrap(),
                        (5, 5)
                    );
                    assert_eq!(
                        own.fixture_state().players[p.index()]
                            .battlefield
                            .iter()
                            .filter(|&&o| own.fixture_state().objects.get(o).tapped)
                            .count(),
                        10 + pumps as usize
                    );
                }
                if !countered {
                    assert!(crate::engine::has_effective_keyword(
                        own.fixture_state(),
                        source,
                        Keywords::FLYING
                    ));
                    assert_eq!(
                        own.fixture_state().objects.get(source).counters.plus1_plus1,
                        0
                    );
                }
                super::ordinary_equipment::reach_main(&mut own, &mut native, p.opponent());
                if !countered {
                    assert_eq!(
                        crate::engine::checked_effective_pt(own.fixture_state(), source).unwrap(),
                        (5, 5)
                    );
                }
                assert_eq!(own.fixture_state().players[p.index()].life, 20);
                assert_eq!(own.fixture_state().players[p.opponent().index()].life, 20);
                if let Some(ref previous) = expected {
                    assert_eq!(own.fixture_state(), previous);
                } else {
                    expected = Some(own.fixture_state().clone());
                }
            }
        }
    }
}
