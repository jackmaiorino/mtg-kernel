use super::*;
use crate::{
    card_def::Keywords,
    ids::ObjectId,
    policy_observation_v6::tests::put,
    state::{GameState, Step, Zone},
};

fn target(own: &mut KaitoSession, native: &mut FastActorSessionV1, id: ObjectId) {
    act(
        own,
        native,
        |a| matches!(a,A::ChooseTarget{target:crate::rl::TargetRefV1::Object{object},..}if object.arena_id==id.0),
    );
}
fn player_target(own: &mut KaitoSession, native: &mut FastActorSessionV1, p: PlayerId) {
    act(
        own,
        native,
        |a| matches!(a,A::ChooseTarget{target:crate::rl::TargetRefV1::Player{player},..}if *player==crate::rl::PlayerSeatV1::from(p)),
    );
}
fn cast(own: &mut KaitoSession, native: &mut FastActorSessionV1, id: ObjectId) {
    act(
        own,
        native,
        |a| matches!(a,A::CastSpell{source,..}if source.arena_id==id.0),
    );
}
fn counter_spell(
    own: &mut KaitoSession,
    native: &mut FastActorSessionV1,
    counter: ObjectId,
    spell: ObjectId,
) {
    act(own, native, |a| matches!(a, A::Pass { .. }));
    cast(own, native, counter);
    act(
        own,
        native,
        |a| matches!(a,A::ChooseTarget{target:crate::rl::TargetRefV1::StackItem{source,..},..}if source.arena_id==spell.0),
    );
}
fn quiet_to_own_upkeep_trigger(
    own: &mut KaitoSession,
    native: &mut FastActorSessionV1,
    p: PlayerId,
) {
    for _ in 0..256 {
        let s = own.fixture_state();
        if s.active_player == p && s.step == Step::Upkeep && !s.stack.is_empty() {
            return;
        }
        act(own, native, |a| {
            matches!(
                a,
                A::Pass { .. }
                    | A::ChooseAttackerInclusion { include: false, .. }
                    | A::ChooseBlockerInclusion { include: false, .. }
                    | A::Discard { .. }
            )
        });
    }
    panic!("natural next own upkeep did not publish Arena trigger");
}
fn meaning(name: &str) {
    let id = crate::card_def::card_id_by_name(name).unwrap();
    assert_eq!(
        super::super::super::roots::registered_permanent_printing(id).unwrap(),
        Some(super::super::super::roots::baseline(id).unwrap())
    );
}

#[test]
fn paid_arena_natural_own_upkeep_draw_loss_source_departure_counter_and_restore_both_seats() {
    meaning("Phyrexian Arena");
    let forest = crate::card_def::card_id_by_name("Forest").unwrap();
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut state = GameState::new_from_libraries_with_starting_player_v1(
            &[forest; 40],
            &[forest; 40],
            crate::rl::card_name,
            110_366,
            p,
        );
        state.step = Step::Main1;
        for _ in 0..5 {
            put(&mut state, p, "Swamp", Zone::Battlefield);
        }
        for _ in 0..3 {
            put(&mut state, p, "Forest", Zone::Battlefield);
            put(&mut state, p.opponent(), "Island", Zone::Battlefield);
        }
        let source = put(&mut state, p, "Phyrexian Arena", Zone::Hand);
        let wings = put(&mut state, p, "Broken Wings", Zone::Hand);
        let counter = put(&mut state, p.opponent(), "Counterspell", Zone::Hand);
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
            3
        );
        let saved = own.snapshot();
        let raw = native.snapshot_v1();
        for response in [0, 1, 2] {
            let mut expected = None;
            for _ in 0..2 {
                own.restore(&saved).unwrap();
                native.kaito_restore_v75(&raw).unwrap();
                if response == 1 {
                    counter_spell(&mut own, &mut native, counter, source);
                }
                settle(&mut own, &mut native);
                assert_eq!(
                    own.fixture_state().objects.get(source).zone,
                    if response == 1 {
                        Zone::Graveyard
                    } else {
                        Zone::Battlefield
                    }
                );
                super::ordinary_equipment::reach_main(&mut own, &mut native, p.opponent());
                assert_eq!(own.fixture_state().players[p.index()].life, 20);
                assert_eq!(own.fixture_state().players[p.index()].library.len(), 40);
                assert_eq!(own.fixture_state().players[p.opponent().index()].life, 20);
                assert_eq!(
                    own.fixture_state().players[p.opponent().index()]
                        .library
                        .len(),
                    39
                );
                if response != 1 {
                    quiet_to_own_upkeep_trigger(&mut own, &mut native, p);
                    assert_eq!(own.fixture_state().players[p.index()].library.len(), 40);
                    assert_eq!(own.fixture_state().players[p.index()].life, 20);
                    // Restore the genuine pending trigger before either response path.
                    let pending = own.snapshot();
                    let pending_raw = native.snapshot_v1();
                    own.restore(&pending).unwrap();
                    native.kaito_restore_v75(&pending_raw).unwrap();
                    if response == 2 {
                        cast(&mut own, &mut native, wings);
                        target(&mut own, &mut native, source);
                    }
                    settle(&mut own, &mut native);
                    assert_eq!(own.fixture_state().step, Step::Upkeep);
                    assert_eq!(own.fixture_state().players[p.index()].library.len(), 39);
                    assert_eq!(own.fixture_state().players[p.index()].life, 19);
                    assert_eq!(
                        own.fixture_state().objects.get(source).zone,
                        if response == 2 {
                            Zone::Graveyard
                        } else {
                            Zone::Battlefield
                        }
                    );
                }
                super::ordinary_equipment::reach_main(&mut own, &mut native, p);
                assert_eq!(
                    own.fixture_state().players[p.index()].library.len(),
                    if response == 1 { 39 } else { 38 }
                );
                assert_eq!(
                    own.fixture_state().players[p.index()].life,
                    if response == 1 { 20 } else { 19 }
                );
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

#[test]
fn paid_rite_actual_cast_not_resolution_opposing_and_creature_filters_departure_counter_and_restore_both_seats(
) {
    meaning("Rite of the Dragoncaller");
    let forest = crate::card_def::card_id_by_name("Forest").unwrap();
    let dragon = crate::card_def::card_id_by_name("Dragon 5/5 Token").unwrap();
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut state = GameState::new_from_libraries_with_starting_player_v1(
            &[forest; 40],
            &[forest; 40],
            crate::rl::card_name,
            110_367,
            p,
        );
        state.step = Step::Main1;
        for _ in 0..8 {
            put(&mut state, p, "Mountain", Zone::Battlefield);
        }
        for _ in 0..3 {
            put(&mut state, p, "Forest", Zone::Battlefield);
        }
        for land in ["Island", "Forest", "Mountain"] {
            for _ in 0..3 {
                put(&mut state, p.opponent(), land, Zone::Battlefield);
            }
        }
        let source = put(&mut state, p, "Rite of the Dragoncaller", Zone::Hand);
        let bolt = put(&mut state, p, "Lightning Bolt", Zone::Hand);
        let elf = put(&mut state, p, "Llanowar Elves", Zone::Hand);
        let counter = put(&mut state, p.opponent(), "Counterspell", Zone::Hand);
        let wings = put(&mut state, p.opponent(), "Broken Wings", Zone::Hand);
        let opposing_bolt = put(&mut state, p.opponent(), "Lightning Bolt", Zone::Hand);
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
            6
        );
        let saved = own.snapshot();
        let raw = native.snapshot_v1();
        for response in [0, 1, 2, 3] {
            let mut expected = None;
            for _ in 0..2 {
                own.restore(&saved).unwrap();
                native.kaito_restore_v75(&raw).unwrap();
                if response == 3 {
                    counter_spell(&mut own, &mut native, counter, source);
                }
                settle(&mut own, &mut native);
                assert_eq!(
                    own.fixture_state().objects.get(source).zone,
                    if response == 3 {
                        Zone::Graveyard
                    } else {
                        Zone::Battlefield
                    }
                );
                cast(&mut own, &mut native, bolt);
                player_target(&mut own, &mut native, p.opponent());
                let pending = own.snapshot();
                let pending_raw = native.snapshot_v1();
                own.restore(&pending).unwrap();
                native.kaito_restore_v75(&pending_raw).unwrap();
                if response == 1 {
                    counter_spell(&mut own, &mut native, counter, bolt);
                }
                if response == 2 {
                    act(&mut own, &mut native, |a| matches!(a, A::Pass { .. }));
                    cast(&mut own, &mut native, wings);
                    target(&mut own, &mut native, source);
                }
                settle(&mut own, &mut native);
                assert_eq!(
                    own.fixture_state().objects.get(source).zone,
                    if response == 2 || response == 3 {
                        Zone::Graveyard
                    } else {
                        Zone::Battlefield
                    }
                );
                assert_eq!(own.fixture_state().objects.get(bolt).zone, Zone::Graveyard);
                assert_eq!(
                    own.fixture_state().players[p.opponent().index()].life,
                    if response == 1 { 20 } else { 17 }
                );
                let dragons = |s: &GameState| {
                    s.players[p.index()]
                        .battlefield
                        .iter()
                        .filter(|&&o| s.objects.get(o).card_def == dragon)
                        .copied()
                        .collect::<Vec<_>>()
                };
                let tokens = dragons(own.fixture_state());
                assert_eq!(tokens.len(), usize::from(response != 3));
                for &o in &tokens {
                    let s = own.fixture_state();
                    let t = s.objects.get(o);
                    assert!(t.v4.is_token);
                    assert_eq!(t.owner, p);
                    assert_eq!(t.controller, p);
                    assert!(!t.tapped);
                    assert!(t.summoning_sick);
                    assert_eq!(crate::engine::checked_effective_pt(s, o).unwrap(), (5, 5));
                    assert!(crate::engine::has_effective_keyword(s, o, Keywords::FLYING));
                }
                cast(&mut own, &mut native, elf);
                settle(&mut own, &mut native);
                assert_eq!(dragons(own.fixture_state()), tokens);
                act(&mut own, &mut native, |a| matches!(a, A::Pass { .. }));
                cast(&mut own, &mut native, opposing_bolt);
                player_target(&mut own, &mut native, p);
                settle(&mut own, &mut native);
                assert_eq!(dragons(own.fixture_state()), tokens);
                assert_eq!(own.fixture_state().players[p.index()].life, 17);
                assert_eq!(own.fixture_state().players[p.index()].mana_pool, [0; 6]);
                assert_eq!(
                    own.fixture_state().players[p.index()]
                        .battlefield
                        .iter()
                        .filter(|&&o| own.fixture_state().objects.get(o).tapped)
                        .count(),
                    8
                );
                if let Some(ref previous) = expected {
                    assert_eq!(own.fixture_state(), previous);
                } else {
                    expected = Some(own.fixture_state().clone());
                }
            }
        }
    }
}
