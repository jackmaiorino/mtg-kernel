use super::*;
use super::{
    ordinary_equipment::reach_main,
    surveil_many::{checkpoint, sessions},
    targeted_entry::{actions, cast, choose},
};
use crate::{
    card_def::Keywords,
    ids::ObjectId,
    policy_observation_v6::tests::put,
    state::{GameState, Step, Zone},
};

fn trample(s: &GameState, object: ObjectId, expected: bool) {
    assert_eq!(
        crate::engine::has_effective_keyword(s, object, Keywords::TRAMPLE),
        expected
    );
}

#[test]
fn paid_colony_base_declined_kicked_entry_counter_dynamic_grant_both_turns_removal_ability_loss_and_restore_both_seats(
) {
    let id = crate::card_def::card_id_by_name("Gnarlid Colony").unwrap();
    assert_eq!(
        super::super::super::roots::registered_permanent_printing(id).unwrap(),
        Some(super::super::super::roots::baseline(id).unwrap())
    );
    let forest = crate::card_def::card_id_by_name("Forest").unwrap();
    for p in [PlayerId::P0, PlayerId::P1] {
        for (resources, kicked) in [(2, false), (7, false), (7, true)] {
            let mut s = GameState::new_from_libraries_with_starting_player_v1(
                &[forest; 40],
                &[forest; 40],
                crate::rl::card_name,
                110_408,
                p,
            );
            s.step = Step::Main1;
            let lands = (0..resources)
                .map(|_| put(&mut s, p, "Forest", Zone::Battlefield))
                .collect::<Vec<_>>();
            for name in ["Island", "Swamp"] {
                for _ in 0..3 {
                    put(&mut s, p.opponent(), name, Zone::Battlefield);
                }
            }
            let source = put(&mut s, p, "Gnarlid Colony", Zone::Hand);
            let hawk = put(&mut s, p, "Healer's Hawk", Zone::Battlefield);
            s.objects.get_mut(hawk).counters.lifelink = 1;
            let plain = put(&mut s, p, "Savannah Lions", Zone::Battlefield);
            let opposing = put(&mut s, p.opponent(), "Healer's Hawk", Zone::Battlefield);
            s.objects.get_mut(opposing).counters.plus1_plus1 = 1;
            let artifact = put(&mut s, p, "Goldvein Pick", Zone::Battlefield);
            s.objects.get_mut(artifact).counters.plus1_plus1 = 1;
            let veil = put(&mut s, p, "Snakeskin Veil", Zone::Hand);
            let counter = put(&mut s, p.opponent(), "Counterspell", Zone::Hand);
            let removal = put(&mut s, p.opponent(), "Hero's Downfall", Zone::Hand);
            let witness = put(&mut s, p.opponent(), "Witness Protection", Zone::Hand);
            let (mut own, mut native) = sessions(s);
            cast(&mut own, &mut native, source);
            if resources == 7 {
                assert_eq!(
                    actions(&own)
                        .iter()
                        .filter(|a| matches!(a, A::ChooseKicker { .. }))
                        .count(),
                    2
                );
                checkpoint(&mut own, &mut native);
                act(
                    &mut own,
                    &mut native,
                    |a| matches!(a,A::ChooseKicker{source:s,pay,..}if s.arena_id==source.0&&*pay==kicked),
                );
            } else {
                assert!(!actions(&own)
                    .iter()
                    .any(|a| matches!(a, A::ChooseKicker { .. })));
            }
            assert_eq!(own.fixture_state().objects.get(source).zone, Zone::Stack);
            assert_eq!(own.fixture_state().stack.last().unwrap().kicked, kicked);
            assert_eq!(
                crate::engine::object_mana_value(own.fixture_state(), source),
                2
            );
            assert_eq!(
                lands
                    .iter()
                    .filter(|&&o| own.fixture_state().objects.get(o).tapped)
                    .count(),
                if kicked { 5 } else { 2 }
            );
            assert_eq!(own.fixture_state().players[p.index()].mana_pool, [0; 6]);
            checkpoint(&mut own, &mut native);
            let saved = own.snapshot();
            let raw = native.snapshot_v1();
            for response in 0..if resources == 7 { 4 } else { 2 } {
                let mut expected = None;
                for _ in 0..2 {
                    own.restore(&saved).unwrap();
                    native.kaito_restore_v75(&raw).unwrap();
                    if response == 0 {
                        cast(&mut own, &mut native, counter);
                        act(&mut own, &mut native, |a| {
                            is_original_spell_target(a, source)
                        });
                    }
                    settle(&mut own, &mut native);
                    assert_eq!(
                        own.fixture_state().objects.get(source).counters.plus1_plus1,
                        if response != 0 && kicked { 2 } else { 0 }
                    );
                    trample(own.fixture_state(), source, response != 0 && kicked);
                    for object in [hawk, plain, opposing, artifact] {
                        trample(own.fixture_state(), object, false);
                    }
                    if resources == 7 {
                        cast(&mut own, &mut native, veil);
                        choose(&mut own, &mut native, veil, hawk);
                        settle(&mut own, &mut native);
                        assert_eq!(
                            own.fixture_state().objects.get(hawk).counters.plus1_plus1,
                            1
                        );
                        trample(own.fixture_state(), hawk, response != 0);
                        if response == 2 {
                            cast(&mut own, &mut native, removal);
                            choose(&mut own, &mut native, removal, source);
                            settle(&mut own, &mut native);
                        } else if response == 3 {
                            // Witness Protection is a sorcery-speed Aura. Use the actual opponent's next main phase.
                            reach_main(&mut own, &mut native, p.opponent());
                            cast(&mut own, &mut native, witness);
                            choose(&mut own, &mut native, witness, source);
                            settle(&mut own, &mut native);
                        }
                    }
                    let active = response == 1;
                    trample(own.fixture_state(), source, active && kicked);
                    trample(own.fixture_state(), hawk, active && resources == 7);
                    for object in [plain, opposing, artifact] {
                        trample(own.fixture_state(), object, false);
                    }
                    assert_eq!(
                        own.fixture_state().objects.get(source).zone,
                        if response == 0 || response == 2 {
                            Zone::Graveyard
                        } else {
                            Zone::Battlefield
                        }
                    );
                    reach_main(&mut own, &mut native, p.opponent());
                    trample(own.fixture_state(), source, active && kicked);
                    trample(own.fixture_state(), hawk, active && resources == 7);
                    checkpoint(&mut own, &mut native);
                    if let Some(ref old) = expected {
                        assert_eq!(own.fixture_state(), old);
                    } else {
                        expected = Some(own.fixture_state().clone());
                    }
                }
            }
        }
    }
}
