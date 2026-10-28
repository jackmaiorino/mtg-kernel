use super::*;
use super::{
    surveil_many::{checkpoint, sessions},
    targeted_entry::{actions, cast, choose},
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
    targets: [ObjectId; 2],
    counter: ObjectId,
    removal: ObjectId,
    stab: ObjectId,
}
fn scene(p: PlayerId, flashback: bool, turtle: bool) -> Scene {
    let forest = crate::card_def::card_id_by_name("Forest").unwrap();
    let mut s = GameState::new_from_libraries_with_starting_player_v1(
        &[forest; 40],
        &[forest; 40],
        crate::rl::card_name,
        110_380,
        p,
    );
    s.step = Step::Main1;
    for _ in 0..6 {
        put(&mut s, p, "Mountain", Zone::Battlefield);
    }
    for _ in 0..3 {
        put(&mut s, p.opponent(), "Swamp", Zone::Battlefield);
    }
    for _ in 0..2 {
        put(&mut s, p.opponent(), "Island", Zone::Battlefield);
    }
    let name = if turtle {
        "Aegis Turtle"
    } else {
        "Serra Angel"
    };
    let targets = [
        put(&mut s, p, name, Zone::Battlefield),
        put(&mut s, p.opponent(), name, Zone::Battlefield),
    ];
    let source = put(
        &mut s,
        p,
        "Bulk Up",
        if flashback {
            Zone::Graveyard
        } else {
            Zone::Hand
        },
    );
    let counter = put(&mut s, p.opponent(), "Counterspell", Zone::Hand);
    let removal = put(&mut s, p.opponent(), "Hero's Downfall", Zone::Hand);
    let stab = put(&mut s, p.opponent(), "Stab", Zone::Hand);
    let (own, native) = sessions(s);
    Scene {
        own,
        native,
        source,
        targets,
        counter,
        removal,
        stab,
    }
}
fn begin(s: &mut Scene, p: PlayerId, flashback: bool, target: ObjectId) {
    cast(&mut s.own, &mut s.native, s.source);
    let menu = actions(&s.own);
    for &object in &s.targets {
        assert!(menu.iter().any(|a|matches!(a,A::ChooseTarget{target:crate::rl::TargetRefV1::Object{object:o},..}if o.arena_id==object.0)));
    }
    assert_eq!(
        menu.iter()
            .filter(|a| matches!(a, A::ChooseTarget { .. }))
            .count(),
        2
    );
    checkpoint(&mut s.own, &mut s.native);
    choose(&mut s.own, &mut s.native, s.source, target);
    let state = s.own.fixture_state();
    assert!(state.engine.pending_cast.is_none());
    assert_eq!(state.objects.get(s.source).zone, Zone::Stack);
    assert_eq!(state.stack.last().unwrap().is_flashback, flashback);
    assert_eq!(crate::engine::object_mana_value(state, s.source), 2);
    assert_eq!(
        state.players[p.index()]
            .battlefield
            .iter()
            .filter(|&&o| state.objects.get(o).tapped)
            .count(),
        if flashback { 6 } else { 2 }
    );
}
fn response(s: &mut Scene, spell: ObjectId, target: ObjectId) {
    cast(&mut s.own, &mut s.native, spell);
    choose(&mut s.own, &mut s.native, spell, target);
}
#[test]
fn paid_bulk_normal_current_power_later_modifier_either_controller_counter_illegal_target_cleanup_and_restore_both_seats(
) {
    paid_route(false);
}
#[test]
fn paid_bulk_flashback_six_mana_stack_value_exile_current_power_counter_illegal_target_cleanup_and_restore_both_seats(
) {
    paid_route(true);
}
fn paid_route(flashback: bool) {
    let id = crate::card_def::card_id_by_name("Bulk Up").unwrap();
    assert_eq!(
        super::super::super::roots::registered_spell_printing(id).unwrap(),
        Some(super::super::super::roots::baseline(id).unwrap())
    );
    for p in [PlayerId::P0, PlayerId::P1] {
        for target_index in [0, 1] {
            let mut s = scene(p, flashback, false);
            let target = s.targets[target_index];
            let other = s.targets[1 - target_index];
            begin(&mut s, p, flashback, target);
            let saved = s.own.snapshot();
            let raw = s.native.snapshot_v1();
            for branch in 0..4 {
                let mut expected = None;
                for _ in 0..2 {
                    s.own.restore(&saved).unwrap();
                    s.native.kaito_restore_v75(&raw).unwrap();
                    if branch != 0 {
                        act(&mut s.own, &mut s.native, |a| matches!(a, A::Pass { .. }));
                        if branch == 1 {
                            cast(&mut s.own, &mut s.native, s.counter);
                            act(&mut s.own, &mut s.native, |a| {
                                is_original_spell_target(a, s.source)
                            });
                        } else {
                            let spell = if branch == 2 { s.removal } else { s.stab };
                            response(&mut s, spell, target);
                        }
                    }
                    settle(&mut s.own, &mut s.native);
                    let state = s.own.fixture_state();
                    assert_eq!(
                        state.objects.get(s.source).zone,
                        if flashback {
                            Zone::Exile
                        } else {
                            Zone::Graveyard
                        }
                    );
                    assert_eq!(
                        crate::engine::checked_effective_pt(state, other).unwrap(),
                        (4, 4)
                    );
                    if branch == 2 {
                        assert_eq!(state.objects.get(target).zone, Zone::Graveyard);
                    } else {
                        assert_eq!(
                            crate::engine::checked_effective_pt(state, target).unwrap(),
                            match branch {
                                0 => (8, 4),
                                1 => (4, 4),
                                _ => (4, 2),
                            }
                        );
                        assert_eq!(state.objects.get(target).counters.plus1_plus1, 0);
                    }
                    assert_eq!(state.players[p.index()].mana_pool, [0; 6]);
                    if branch == 0 {
                        let stab = s.stab;
                        response(&mut s, stab, target);
                        settle(&mut s.own, &mut s.native);
                        assert_eq!(
                            crate::engine::checked_effective_pt(s.own.fixture_state(), target)
                                .unwrap(),
                            (6, 2),
                            "later Stab must not resample an earlier double"
                        );
                        super::ordinary_equipment::reach_main(
                            &mut s.own,
                            &mut s.native,
                            p.opponent(),
                        );
                        assert_eq!(
                            crate::engine::checked_effective_pt(s.own.fixture_state(), target)
                                .unwrap(),
                            (4, 4)
                        );
                    }
                    if let Some(ref state) = expected {
                        assert_eq!(s.own.fixture_state(), state);
                    } else {
                        expected = Some(s.own.fixture_state().clone());
                    }
                }
            }
        }
    }
}
#[test]
fn paid_bulk_zero_and_response_created_negative_power_preserve_signed_resolution_sample_both_routes_and_seats(
) {
    for p in [PlayerId::P0, PlayerId::P1] {
        for flashback in [false, true] {
            let mut s = scene(p, flashback, true);
            let target = s.targets[0];
            begin(&mut s, p, flashback, target);
            let saved = s.own.snapshot();
            let raw = s.native.snapshot_v1();
            for negative in [false, true] {
                let mut expected = None;
                for _ in 0..2 {
                    s.own.restore(&saved).unwrap();
                    s.native.kaito_restore_v75(&raw).unwrap();
                    if negative {
                        let stab = s.stab;
                        response(&mut s, stab, target);
                    }
                    settle(&mut s.own, &mut s.native);
                    let state = s.own.fixture_state();
                    assert_eq!(
                        crate::engine::checked_effective_pt(state, target).unwrap(),
                        if negative { (-4, 3) } else { (0, 5) }
                    );
                    assert_eq!(
                        state.objects.get(s.source).zone,
                        if flashback {
                            Zone::Exile
                        } else {
                            Zone::Graveyard
                        }
                    );
                    assert_eq!(state.objects.get(target).counters.plus1_plus1, 0);
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
