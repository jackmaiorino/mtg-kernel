use super::*;
use crate::{
    card_def::Keywords,
    ids::ObjectId,
    policy_observation_v6::tests::put,
    state::{GameState, Step, Zone},
};

#[test]
fn paid_anthem_current_team_later_entry_departure_counter_and_restore_both_seats() {
    paid_team_static("Anthem of Champions");
}
#[test]
fn paid_eagle_other_effective_flyers_real_grant_cleanup_departure_counter_and_restore_both_seats() {
    paid_team_static("Empyrean Eagle");
}

fn target(own: &mut KaitoSession, native: &mut FastActorSessionV1, object: ObjectId) {
    act(
        own,
        native,
        |a| matches!(a,A::ChooseTarget{target:crate::rl::TargetRefV1::Object{object:o},..} if o.arena_id==object.0),
    );
}
fn pt(state: &GameState, object: ObjectId, expected: (i32, i32)) {
    assert_eq!(
        crate::engine::checked_effective_pt(state, object).unwrap(),
        expected
    );
    assert_eq!(state.objects.get(object).counters.plus1_plus1, 0);
}

fn paid_team_static(name: &str) {
    let eagle = name == "Empyrean Eagle";
    let id = crate::card_def::card_id_by_name(name).unwrap();
    let meaning = super::super::super::roots::baseline(id).unwrap();
    assert_eq!(
        super::super::super::roots::registered_permanent_printing(id).unwrap(),
        Some(meaning)
    );
    let forest = crate::card_def::card_id_by_name("Forest").unwrap();
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut state = GameState::new_from_libraries_with_starting_player_v1(
            &[forest; 40],
            &[forest; 40],
            crate::rl::card_name,
            110_365,
            p,
        );
        state.step = Step::Main1;
        for land in ["Plains", "Island", "Forest"] {
            // Each Eagle can spend two Plains (generic plus W). Keep two
            // additional white sources for Flight and the later Hawk.
            for _ in 0..if eagle && land == "Plains" { 6 } else { 4 } {
                put(&mut state, p, land, Zone::Battlefield);
            }
        }
        for land in ["Island", "Forest"] {
            for _ in 0..3 {
                put(&mut state, p.opponent(), land, Zone::Battlefield);
            }
        }
        let hawk = put(&mut state, p, "Healer's Hawk", Zone::Battlefield);
        let parent = put(&mut state, p, "Prideful Parent", Zone::Battlefield);
        let opposing = put(&mut state, p.opponent(), "Healer's Hawk", Zone::Battlefield);
        let late = put(&mut state, p, "Healer's Hawk", Zone::Hand);
        let flight = put(&mut state, p, "Fleeting Flight", Zone::Hand);
        let counter = put(&mut state, p.opponent(), "Counterspell", Zone::Hand);
        let wings = put(&mut state, p.opponent(), "Broken Wings", Zone::Hand);
        let spell = put(&mut state, p, name, Zone::Hand);
        let second = put(&mut state, p, name, Zone::Hand);
        let mut native = FastActorSessionV1::from_kaito_v75_fixture_state(state.clone());
        let mut own =
            KaitoSession::from_session(FastActorSessionV1::from_kaito_v75_fixture_state(state))
                .unwrap();
        act(
            &mut own,
            &mut native,
            |a| matches!(a,A::CastSpell{source,..} if source.arena_id==spell.0),
        );
        let paid = if eagle { 3 } else { 2 };
        assert_eq!(own.fixture_state().objects.get(spell).zone, Zone::Stack);
        assert_eq!(
            own.fixture_state().players[p.index()]
                .battlefield
                .iter()
                .filter(|&&o| own.fixture_state().objects.get(o).tapped)
                .count(),
            paid
        );
        let saved = own.snapshot();
        let raw = native.snapshot_v1();
        for response in [0, 1, 2] {
            let mut expected = None;
            for _ in 0..2 {
                own.restore(&saved).unwrap();
                native.kaito_restore_v75(&raw).unwrap();
                if response == 1 {
                    act(&mut own, &mut native, |a| matches!(a, A::Pass { .. }));
                    act(
                        &mut own,
                        &mut native,
                        |a| matches!(a,A::CastSpell{source,..}if source.arena_id==counter.0),
                    );
                    act(&mut own, &mut native, |a| {
                        is_original_spell_target(a, spell)
                    });
                }
                settle(&mut own, &mut native);
                let entered = response != 1;
                assert_eq!(
                    own.fixture_state().objects.get(spell).zone,
                    if entered {
                        Zone::Battlefield
                    } else {
                        Zone::Graveyard
                    }
                );
                pt(
                    own.fixture_state(),
                    hawk,
                    if entered { (2, 2) } else { (1, 1) },
                );
                pt(
                    own.fixture_state(),
                    parent,
                    if entered && !eagle { (3, 3) } else { (2, 2) },
                );
                pt(own.fixture_state(), opposing, (1, 1));
                if entered && eagle {
                    pt(own.fixture_state(), spell, (2, 3));
                }
                if response == 2 {
                    act(&mut own, &mut native, |a| matches!(a, A::Pass { .. }));
                    act(
                        &mut own,
                        &mut native,
                        |a| matches!(a,A::CastSpell{source,..}if source.arena_id==wings.0),
                    );
                    target(&mut own, &mut native, spell);
                    settle(&mut own, &mut native);
                    assert_eq!(own.fixture_state().objects.get(spell).zone, Zone::Graveyard);
                }
                let active = response == 0;
                pt(
                    own.fixture_state(),
                    hawk,
                    if active { (2, 2) } else { (1, 1) },
                );
                if active {
                    act(
                        &mut own,
                        &mut native,
                        |a| matches!(a,A::CastSpell{source,..}if source.arena_id==second.0),
                    );
                    settle(&mut own, &mut native);
                    pt(own.fixture_state(), hawk, (3, 3));
                    pt(
                        own.fixture_state(),
                        parent,
                        if eagle { (2, 2) } else { (4, 4) },
                    );
                    if eagle {
                        pt(own.fixture_state(), spell, (3, 4));
                        pt(own.fixture_state(), second, (3, 4));
                    }
                }
                act(
                    &mut own,
                    &mut native,
                    |a| matches!(a,A::CastSpell{source,..}if source.arena_id==flight.0),
                );
                target(&mut own, &mut native, parent);
                settle(&mut own, &mut native);
                assert!(crate::engine::has_effective_keyword(
                    own.fixture_state(),
                    parent,
                    Keywords::FLYING
                ));
                assert_eq!(
                    own.fixture_state().objects.get(parent).counters.plus1_plus1,
                    1
                );
                assert_eq!(
                    crate::engine::checked_effective_pt(own.fixture_state(), parent).unwrap(),
                    if active { (5, 5) } else { (3, 3) }
                );
                act(
                    &mut own,
                    &mut native,
                    |a| matches!(a,A::CastSpell{source,..}if source.arena_id==late.0),
                );
                settle(&mut own, &mut native);
                pt(
                    own.fixture_state(),
                    late,
                    if active { (3, 3) } else { (1, 1) },
                );
                pt(own.fixture_state(), opposing, (1, 1));
                assert_eq!(own.fixture_state().players[p.index()].mana_pool, [0; 6]);
                assert_eq!(
                    own.fixture_state().players[p.index()]
                        .battlefield
                        .iter()
                        .filter(|&&o| own.fixture_state().objects.get(o).tapped)
                        .count(),
                    paid + 2 + if active { paid } else { 0 }
                );
                super::ordinary_equipment::reach_main(&mut own, &mut native, p.opponent());
                assert!(!crate::engine::has_effective_keyword(
                    own.fixture_state(),
                    parent,
                    Keywords::FLYING
                ));
                assert_eq!(
                    crate::engine::checked_effective_pt(own.fixture_state(), parent).unwrap(),
                    if active && !eagle { (5, 5) } else { (3, 3) }
                );
                pt(
                    own.fixture_state(),
                    hawk,
                    if active { (3, 3) } else { (1, 1) },
                );
                pt(
                    own.fixture_state(),
                    late,
                    if active { (3, 3) } else { (1, 1) },
                );
                assert!(crate::engine::has_effective_keyword(
                    own.fixture_state(),
                    parent,
                    Keywords::VIGILANCE
                ));
                if let Some(ref previous) = expected {
                    assert_eq!(own.fixture_state(), previous);
                } else {
                    expected = Some(own.fixture_state().clone());
                }
            }
        }
    }
}
