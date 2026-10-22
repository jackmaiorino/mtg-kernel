use super::targeted_entry::{actions, cast};
use super::*;
use crate::{
    card_def::Keywords,
    ids::ObjectId,
    policy_observation_v6::tests::put,
    state::{GameState, Step, Zone},
};

fn activation(a: &A, source: ObjectId) -> bool {
    matches!(a, A::ActivateAbility { source: s, ability_index: 0, .. } if s.arena_id == source.0)
}
fn tapped(state: &GameState, p: PlayerId) -> usize {
    state.players[p.index()]
        .battlefield
        .iter()
        .filter(|&&o| state.objects.get(o).tapped)
        .count()
}

#[test]
fn paid_sailor_opposing_turn_flash_repeat_draw_source_departure_counter_and_restore_both_seats() {
    let id = crate::card_def::card_id_by_name("Spectral Sailor").unwrap();
    assert_eq!(
        super::super::super::roots::registered_permanent_printing(id).unwrap(),
        Some(super::super::super::roots::baseline(id).unwrap())
    );
    let island = crate::card_def::card_id_by_name("Island").unwrap();
    for p in [PlayerId::P0, PlayerId::P1] {
        let q = p.opponent();
        let mut state = GameState::new_from_libraries_with_starting_player_v1(
            &[island; 40],
            &[island; 40],
            crate::rl::card_name,
            110_381,
            q,
        );
        state.step = Step::Main1;
        for _ in 0..9 {
            put(&mut state, p, "Island", Zone::Battlefield);
        }
        for land in ["Island", "Swamp"] {
            for _ in 0..3 {
                put(&mut state, q, land, Zone::Battlefield);
            }
        }
        let source = put(&mut state, p, "Spectral Sailor", Zone::Hand);
        let counter = put(&mut state, q, "Counterspell", Zone::Hand);
        let removal = put(&mut state, q, "Hero's Downfall", Zone::Hand);
        let mut native = FastActorSessionV1::from_kaito_v75_fixture_state(state.clone());
        let mut own =
            KaitoSession::from_session(FastActorSessionV1::from_kaito_v75_fixture_state(state))
                .unwrap();
        cast(&mut own, &mut native, source);
        assert_eq!(own.fixture_state().active_player, q);
        assert_eq!(own.fixture_state().objects.get(source).zone, Zone::Stack);
        assert_eq!(tapped(own.fixture_state(), p), 1);
        let paid = own.snapshot();
        let raw = native.snapshot_v1();
        for (countered, removed, draws) in [
            (true, false, 0usize),
            (false, false, 0),
            (false, false, 1),
            (false, false, 2),
            (false, true, 1),
            (false, true, 2),
        ] {
            let mut expected = None;
            for _ in 0..2 {
                own.restore(&paid).unwrap();
                native.kaito_restore_v75(&raw).unwrap();
                let before = own.fixture_state().players[p.index()].library.clone();
                let hand = own.fixture_state().players[p.index()].hand.len();
                if countered {
                    cast(&mut own, &mut native, counter);
                    act(&mut own, &mut native, |a| {
                        is_original_spell_target(a, source)
                    });
                }
                settle(&mut own, &mut native);
                if !countered {
                    let s = own.fixture_state();
                    assert!(s.objects.get(source).summoning_sick);
                    assert!(crate::engine::has_effective_keyword(
                        s,
                        source,
                        Keywords::FLYING
                    ));
                    assert_eq!(
                        crate::engine::checked_effective_pt(s, source).unwrap(),
                        (1, 1)
                    );
                    for _ in 0..draws {
                        if !actions(&own).iter().any(|a| activation(a, source)) {
                            act(&mut own, &mut native, |a| matches!(a, A::Pass { .. }));
                        }
                        act(&mut own, &mut native, |a| activation(a, source));
                    }
                    if draws != 0 {
                        let pending = own.snapshot();
                        let pending_raw = native.snapshot_v1();
                        own.restore(&pending).unwrap();
                        native.kaito_restore_v75(&pending_raw).unwrap();
                        // Activations consume mana immediately, with no tap or draw yet.
                        assert_eq!(tapped(own.fixture_state(), p), 1 + 4 * draws);
                        assert_eq!(own.fixture_state().players[p.index()].library, before);
                        assert!(!own.fixture_state().objects.get(source).tapped);
                    }
                    if removed {
                        cast(&mut own, &mut native, removal);
                        act(
                            &mut own,
                            &mut native,
                            |a| matches!(a,A::ChooseTarget{target:crate::rl::TargetRefV1::Object{object},..} if object.arena_id==source.0),
                        );
                    }
                    settle(&mut own, &mut native);
                    assert_eq!(
                        own.fixture_state().players[p.index()].library.len(),
                        before.len() - draws
                    );
                    assert_eq!(
                        own.fixture_state().players[p.index()].hand.len(),
                        hand + draws
                    );
                    for &drawn in before.iter().rev().take(draws) {
                        assert!(own.fixture_state().players[p.index()].hand.contains(&drawn));
                    }
                }
                let s = own.fixture_state();
                assert_eq!(s.active_player, q);
                assert_eq!(
                    s.objects.get(source).zone,
                    if countered || removed {
                        Zone::Graveyard
                    } else {
                        Zone::Battlefield
                    }
                );
                assert_eq!(tapped(s, p), 1 + 4 * draws);
                assert_eq!(s.players[p.index()].mana_pool, [0; 6]);
                assert_eq!(s.players[p.index()].life, 20);
                assert_eq!(s.players[q.index()].life, 20);
                if countered {
                    assert_eq!(s.players[p.index()].library, before);
                    assert_eq!(s.players[p.index()].hand.len(), hand);
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
