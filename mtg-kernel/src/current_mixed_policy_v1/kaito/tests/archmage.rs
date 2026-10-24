use super::surveil_many::{checkpoint, sessions};
use super::targeted_entry::{cast, choose};
use super::*;
use crate::{
    policy_observation_v6::tests::put,
    state::{GameState, Step, Zone},
};

fn tapped(s: &GameState, p: PlayerId) -> usize {
    s.players[p.index()]
        .battlefield
        .iter()
        .filter(|&&o| s.objects.get(o).tapped)
        .count()
}

#[test]
fn paid_archmage_live_stacking_discount_cast_draw_artifact_and_opponent_exclusions_counter_departure_restore_both_seats(
) {
    let id = crate::card_def::card_id_by_name("Archmage of Runes").unwrap();
    assert_eq!(
        super::super::super::roots::registered_permanent_printing(id).unwrap(),
        Some(super::super::super::roots::baseline(id).unwrap())
    );
    let island = crate::card_def::card_id_by_name("Island").unwrap();
    for p in [PlayerId::P0, PlayerId::P1] {
        let q = p.opponent();
        for copies in [1usize, 2] {
            let mut s = GameState::new_from_libraries_with_starting_player_v1(
                &[island; 40],
                &[island; 40],
                crate::rl::card_name,
                110_403,
                p,
            );
            s.step = Step::Main1;
            for _ in 0..14 {
                put(&mut s, p, "Island", Zone::Battlefield);
            }
            for _ in 0..6 {
                put(&mut s, q, "Island", Zone::Battlefield);
            }
            let sources = (0..copies)
                .map(|_| put(&mut s, p, "Archmage of Runes", Zone::Hand))
                .collect::<Vec<_>>();
            let spell = put(&mut s, p, "Divination", Zone::Hand);
            let artifact = put(&mut s, p, "Goldvein Pick", Zone::Hand);
            let other = put(&mut s, q, "Think Twice", Zone::Hand);
            let counter = put(&mut s, q, "Counterspell", Zone::Hand);
            let removal = put(&mut s, q, "Unsummon", Zone::Hand);
            let (mut own, mut native) = sessions(s);
            for &source in &sources {
                let before = tapped(own.fixture_state(), p);
                cast(&mut own, &mut native, source);
                assert_eq!(tapped(own.fixture_state(), p), before + 5);
                checkpoint(&mut own, &mut native);
                settle(&mut own, &mut native);
                assert_eq!(
                    crate::engine::checked_effective_pt(own.fixture_state(), source).unwrap(),
                    (3, 6)
                );
            }
            assert_eq!(own.fixture_state().players[p.index()].library.len(), 40);
            cast(&mut own, &mut native, other);
            assert_eq!(tapped(own.fixture_state(), q), 2);
            settle(&mut own, &mut native);
            assert_eq!(own.fixture_state().players[p.index()].library.len(), 40);
            assert_eq!(own.fixture_state().players[q.index()].library.len(), 39);
            let before = tapped(own.fixture_state(), p);
            cast(&mut own, &mut native, artifact);
            assert_eq!(tapped(own.fixture_state(), p), before + 2);
            settle(&mut own, &mut native);
            assert_eq!(own.fixture_state().players[p.index()].library.len(), 40);
            let ready = own.snapshot();
            let ready_raw = native.snapshot_v1();
            for branch in 0..3 {
                let mut expected = None;
                for _ in 0..2 {
                    own.restore(&ready).unwrap();
                    native.kaito_restore_v75(&ready_raw).unwrap();
                    let before = tapped(own.fixture_state(), p);
                    cast(&mut own, &mut native, spell);
                    assert_eq!(tapped(own.fixture_state(), p), before + 3 - copies);
                    assert_eq!(own.fixture_state().players[p.index()].library.len(), 40);
                    checkpoint(&mut own, &mut native);
                    if branch == 1 {
                        cast(&mut own, &mut native, counter);
                        act(&mut own, &mut native, |a| {
                            is_original_spell_target(a, spell)
                        });
                    }
                    if branch == 2 {
                        cast(&mut own, &mut native, removal);
                        choose(&mut own, &mut native, removal, sources[0]);
                    }
                    settle(&mut own, &mut native);
                    let s = own.fixture_state();
                    let drawn = copies + if branch == 1 { 0 } else { 2 };
                    assert_eq!(s.players[p.index()].library.len(), 40 - drawn);
                    assert_eq!(
                        s.players[p.index()].hand.len(),
                        drawn + usize::from(branch == 2)
                    );
                    assert_eq!(s.objects.get(spell).zone, Zone::Graveyard);
                    assert_eq!(
                        s.objects.get(sources[0]).zone,
                        if branch == 2 {
                            Zone::Hand
                        } else {
                            Zone::Battlefield
                        }
                    );
                    assert_eq!(tapped(s, p), before + 3 - copies);
                    assert_eq!(s.players[p.index()].mana_pool, [0; 6]);
                    assert_eq!(s.players[q.index()].library.len(), 39);
                    assert_eq!(s.players[p.index()].life, 20);
                    assert_eq!(s.players[q.index()].life, 20);
                    if let Some(ref old) = expected {
                        assert_eq!(s, old);
                    } else {
                        expected = Some(s.clone());
                    }
                }
            }
        }
    }
}
