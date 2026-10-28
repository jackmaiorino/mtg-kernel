use super::*;
use super::{
    ordinary_equipment::reach_main,
    surveil_many::{checkpoint, sessions},
    targeted_entry::{cast, choose},
};
use crate::{
    card_def::{Keywords, Subtype},
    ids::ObjectId,
    policy_observation_v6::tests::put,
    state::{GameState, Step, Zone},
};
fn characteristics(s: &GameState, source: ObjectId, counters: i32, flying_knight: bool) {
    assert_eq!(s.objects.get(source).counters.plus1_plus1, counters);
    assert_eq!(
        crate::engine::checked_effective_pt(s, source).unwrap(),
        (1 + counters, 1 + counters)
    );
    assert_eq!(
        crate::engine::has_effective_keyword(s, source, Keywords::FLYING),
        flying_knight
    );
    assert_eq!(
        crate::engine::has_effective_subtype(s, source, Subtype::Knight),
        flying_knight
    );
    assert!(crate::engine::has_effective_subtype(s, source, Subtype::Cat) || !flying_knight);
}
#[test]
fn paid_squire_own_token_and_nontoken_entries_opponent_artifact_exclusions_threshold_both_turns_ability_loss_removal_restore_both_seats(
) {
    let id = crate::card_def::card_id_by_name("Skyknight Squire").unwrap();
    assert_eq!(
        super::super::super::roots::registered_permanent_printing(id).unwrap(),
        Some(super::super::super::roots::baseline(id).unwrap())
    );
    let forest = crate::card_def::card_id_by_name("Forest").unwrap();
    let soldier = crate::card_def::card_id_by_name("Soldier Token").unwrap();
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = GameState::new_from_libraries_with_starting_player_v1(
            &[forest; 40],
            &[forest; 40],
            crate::rl::card_name,
            110_411,
            p,
        );
        s.step = Step::Main1;
        let lands = (0..7)
            .map(|_| put(&mut s, p, "Plains", Zone::Battlefield))
            .collect::<Vec<_>>();
        for name in ["Plains", "Island", "Swamp"] {
            for _ in 0..3 {
                put(&mut s, p.opponent(), name, Zone::Battlefield);
            }
        }
        let source = put(&mut s, p, "Skyknight Squire", Zone::Hand);
        let hawk = put(&mut s, p, "Healer's Hawk", Zone::Hand);
        let own_reinforce = put(&mut s, p, "Resolute Reinforcements", Zone::Hand);
        let other_reinforce = put(&mut s, p.opponent(), "Resolute Reinforcements", Zone::Hand);
        let artifact = put(&mut s, p, "Goldvein Pick", Zone::Hand);
        let counter = put(&mut s, p.opponent(), "Counterspell", Zone::Hand);
        let removal = put(&mut s, p.opponent(), "Hero's Downfall", Zone::Hand);
        let witness = put(&mut s, p.opponent(), "Witness Protection", Zone::Hand);
        let (mut own, mut native) = sessions(s);
        cast(&mut own, &mut native, source);
        assert_eq!(
            lands
                .iter()
                .filter(|&&o| own.fixture_state().objects.get(o).tapped)
                .count(),
            2
        );
        assert_eq!(own.fixture_state().players[p.index()].mana_pool, [0; 6]);
        checkpoint(&mut own, &mut native);
        let saved = own.snapshot();
        let raw = native.snapshot_v1();
        for response in 0..4 {
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
                if response != 0 {
                    characteristics(own.fixture_state(), source, 0, false);
                }
                cast(&mut own, &mut native, other_reinforce);
                settle(&mut own, &mut native);
                if response != 0 {
                    characteristics(own.fixture_state(), source, 0, false);
                }
                cast(&mut own, &mut native, artifact);
                settle(&mut own, &mut native);
                if response != 0 {
                    characteristics(own.fixture_state(), source, 0, false);
                }
                cast(&mut own, &mut native, hawk);
                settle(&mut own, &mut native);
                if response != 0 {
                    characteristics(own.fixture_state(), source, 1, false);
                }
                cast(&mut own, &mut native, own_reinforce);
                settle(&mut own, &mut native);
                if response != 0 {
                    characteristics(own.fixture_state(), source, 3, true);
                }
                assert_eq!(
                    own.fixture_state().players[p.index()]
                        .battlefield
                        .iter()
                        .filter(|&&o| own.fixture_state().objects.get(o).card_def == soldier)
                        .count(),
                    1
                );
                assert_eq!(
                    own.fixture_state().players[p.opponent().index()]
                        .battlefield
                        .iter()
                        .filter(|&&o| own.fixture_state().objects.get(o).card_def == soldier)
                        .count(),
                    1
                );
                for object in [hawk, own_reinforce, other_reinforce, artifact] {
                    assert_eq!(
                        own.fixture_state().objects.get(object).counters.plus1_plus1,
                        0
                    );
                }
                assert!(lands
                    .iter()
                    .all(|&o| own.fixture_state().objects.get(o).tapped));
                checkpoint(&mut own, &mut native);
                if response == 2 {
                    cast(&mut own, &mut native, removal);
                    choose(&mut own, &mut native, removal, source);
                    settle(&mut own, &mut native);
                }
                reach_main(&mut own, &mut native, p.opponent());
                if response == 3 {
                    cast(&mut own, &mut native, witness);
                    choose(&mut own, &mut native, witness, source);
                    settle(&mut own, &mut native);
                    characteristics(own.fixture_state(), source, 3, false);
                }
                if response == 1 {
                    characteristics(own.fixture_state(), source, 3, true);
                }
                assert_eq!(
                    own.fixture_state().objects.get(source).zone,
                    if response == 0 || response == 2 {
                        Zone::Graveyard
                    } else {
                        Zone::Battlefield
                    }
                );
                if let Some(ref old) = expected {
                    assert_eq!(own.fixture_state(), old);
                } else {
                    expected = Some(own.fixture_state().clone());
                }
                checkpoint(&mut own, &mut native);
            }
        }
    }
}
