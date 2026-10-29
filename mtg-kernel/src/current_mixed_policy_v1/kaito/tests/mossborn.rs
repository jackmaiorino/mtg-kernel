use super::*;
use super::{
    surveil_many::{checkpoint, sessions},
    targeted_entry::{cast, choose, wait_target},
};
use crate::{
    card_def::Keywords,
    policy_observation_v6::tests::put,
    state::{GameState, StackItemKind, Step, Zone},
};

#[test]
fn paid_hydra_entry_counter_current_count_land_play_and_searched_entry_counter_source_removal_restore_both_seats(
) {
    let id = crate::card_def::card_id_by_name("Mossborn Hydra").unwrap();
    assert_eq!(
        super::super::super::roots::registered_permanent_printing(id).unwrap(),
        Some(super::super::super::roots::baseline(id).unwrap())
    );
    let forest = crate::card_def::card_id_by_name("Forest").unwrap();
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = GameState::new_from_libraries_with_starting_player_v1(
            &[forest; 40],
            &[forest; 40],
            crate::rl::card_name,
            110_397,
            p,
        );
        s.step = Step::Main1;
        let lands = (0..4)
            .map(|_| put(&mut s, p, "Forest", Zone::Battlefield))
            .collect::<Vec<_>>();
        for name in ["Island", "Swamp"] {
            for _ in 0..3 {
                put(&mut s, p.opponent(), name, Zone::Battlefield);
            }
        }
        let source = put(&mut s, p, "Mossborn Hydra", Zone::Hand);
        let wilds = put(&mut s, p, "Evolving Wilds", Zone::Hand);
        let veil = put(&mut s, p, "Snakeskin Veil", Zone::Hand);
        let counter = put(&mut s, p.opponent(), "Counterspell", Zone::Hand);
        let removal = put(&mut s, p.opponent(), "Hero's Downfall", Zone::Hand);
        let searched = s.players[p.index()].library[0];
        let (mut own, mut native) = sessions(s);
        cast(&mut own, &mut native, source);
        assert_eq!(own.fixture_state().objects.get(source).zone, Zone::Stack);
        assert_eq!(
            crate::engine::object_mana_value(own.fixture_state(), source),
            3
        );
        assert_eq!(
            lands
                .iter()
                .filter(|&&o| own.fixture_state().objects.get(o).tapped)
                .count(),
            3
        );
        assert_eq!(own.fixture_state().players[p.index()].mana_pool, [0; 6]);
        let paid = own.snapshot();
        let raw = native.snapshot_v1();
        cast(&mut own, &mut native, counter);
        act(&mut own, &mut native, |a| {
            is_original_spell_target(a, source)
        });
        settle(&mut own, &mut native);
        assert_eq!(
            own.fixture_state().objects.get(source).zone,
            Zone::Graveyard
        );
        assert_eq!(
            own.fixture_state().objects.get(source).counters.plus1_plus1,
            0
        );
        own.restore(&paid).unwrap();
        native.kaito_restore_v75(&raw).unwrap();
        settle(&mut own, &mut native);
        assert_eq!(
            own.fixture_state().objects.get(source).zone,
            Zone::Battlefield
        );
        assert_eq!(
            own.fixture_state().objects.get(source).counters.plus1_plus1,
            1
        );
        assert_eq!(
            crate::engine::checked_effective_pt(own.fixture_state(), source).unwrap(),
            (1, 1)
        );
        assert!(crate::engine::has_effective_keyword(
            own.fixture_state(),
            source,
            Keywords::TRAMPLE
        ));
        act(
            &mut own,
            &mut native,
            |a| matches!(a,A::PlayLand{source:s,..}if s.arena_id==wilds.0),
        );
        for _ in 0..64 {
            if own
                .fixture_state()
                .stack
                .last()
                .is_some_and(|i| i.source == source && i.kind == StackItemKind::TriggeredAbility)
            {
                break;
            }
            act(&mut own, &mut native, |a| {
                matches!(
                    a,
                    A::Pass { .. } | A::OrderTriggers { .. } | A::ChooseTriggerOrderNext { .. }
                )
            });
        }
        assert!(own
            .fixture_state()
            .stack
            .last()
            .is_some_and(|i| i.source == source && i.kind == StackItemKind::TriggeredAbility));
        assert_eq!(
            own.fixture_state().objects.get(source).counters.plus1_plus1,
            1
        );
        assert_eq!(
            own.fixture_state().players[p.index()].lands_played_this_turn,
            1
        );
        checkpoint(&mut own, &mut native);
        let captured = own.snapshot();
        let captured_raw = native.snapshot_v1();
        for response in 0..3 {
            let mut expected = None;
            for _ in 0..2 {
                own.restore(&captured).unwrap();
                native.kaito_restore_v75(&captured_raw).unwrap();
                if response == 1 {
                    cast(&mut own, &mut native, veil);
                    choose(&mut own, &mut native, veil, source);
                }
                if response == 2 {
                    cast(&mut own, &mut native, removal);
                    choose(&mut own, &mut native, removal, source);
                }
                settle(&mut own, &mut native);
                if response == 2 {
                    assert_eq!(
                        own.fixture_state().objects.get(source).zone,
                        Zone::Graveyard
                    );
                    assert_eq!(
                        own.fixture_state().objects.get(source).counters.plus1_plus1,
                        0
                    );
                } else {
                    assert_eq!(
                        own.fixture_state().objects.get(source).counters.plus1_plus1,
                        if response == 1 { 4 } else { 2 }
                    );
                    act(
                        &mut own,
                        &mut native,
                        |a| matches!(a,A::ActivateAbility{source:s,ability_index:0,..}if s.arena_id==wilds.0),
                    );
                    assert_eq!(own.fixture_state().objects.get(wilds).zone, Zone::Graveyard);
                    wait_target(&mut own, &mut native, wilds, searched);
                    checkpoint(&mut own, &mut native);
                    choose(&mut own, &mut native, wilds, searched);
                    settle(&mut own, &mut native);
                    assert_eq!(
                        own.fixture_state().objects.get(searched).zone,
                        Zone::Battlefield
                    );
                    assert!(own.fixture_state().objects.get(searched).tapped);
                    assert_eq!(
                        own.fixture_state().objects.get(source).counters.plus1_plus1,
                        if response == 1 { 8 } else { 4 }
                    );
                    assert_eq!(
                        own.fixture_state().players[p.index()].lands_played_this_turn,
                        1
                    );
                }
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
