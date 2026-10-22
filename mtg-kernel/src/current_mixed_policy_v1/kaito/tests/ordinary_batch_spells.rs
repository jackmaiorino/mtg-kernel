use super::*;
use crate::{
    card_def::Keywords,
    policy_observation_v6::tests::put,
    state::{GameState, Step, Zone},
};

#[test]
fn paid_seismic_effective_flying_filter_both_sides_counter_response_and_restore_both_seats() {
    paid_batch_spell("Seismic Rupture");
}
#[test]
fn paid_heroic_new_soldiers_existing_set_later_entry_cleanup_counter_and_restore_both_seats() {
    paid_batch_spell("Heroic Reinforcements");
}
#[test]
fn paid_bounty_resolution_land_count_treasure_counter_response_and_restore_both_seats() {
    paid_batch_spell("Brass's Bounty");
}

fn paid_batch_spell(name: &str) {
    let seismic = name == "Seismic Rupture";
    let heroic = name == "Heroic Reinforcements";
    let bounty = name == "Brass's Bounty";
    let id = crate::card_def::card_id_by_name(name).unwrap();
    let meaning = super::super::super::roots::baseline(id).unwrap();
    assert_eq!(
        meaning["ordinary_batch_spell_printing"]["ordered_cast_roots"][0]["targets"],
        serde_json::json!([])
    );
    assert_eq!(
        super::super::super::roots::registered_spell_printing(id).unwrap(),
        Some(meaning)
    );
    let forest = crate::card_def::card_id_by_name("Forest").unwrap();
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut state = GameState::new_from_libraries_with_starting_player_v1(
            &[forest; 40],
            &[forest; 40],
            crate::rl::card_name,
            110_303,
            p,
        );
        state.step = Step::Main1;
        for land in ["Plains", "Island", "Swamp", "Mountain", "Forest"] {
            for _ in 0..2 {
                put(&mut state, p, land, Zone::Battlefield);
            }
        }
        for _ in 0..3 {
            put(&mut state, p.opponent(), "Island", Zone::Battlefield);
        }
        for _ in 0..2 {
            put(&mut state, p.opponent(), "Forest", Zone::Battlefield);
        }
        if bounty {
            for _ in 0..3 {
                put(&mut state, p.opponent(), "Swamp", Zone::Battlefield);
            }
        }
        let creature_land =
            bounty.then(|| put(&mut state, p, "Soulstone Sanctuary", Zone::Battlefield));
        let land_removal =
            bounty.then(|| put(&mut state, p.opponent(), "Hero's Downfall", Zone::Hand));
        let own_ground = put(&mut state, p, "Prideful Parent", Zone::Battlefield);
        let other_ground = put(
            &mut state,
            p.opponent(),
            "Prideful Parent",
            Zone::Battlefield,
        );
        let own_flyer = put(&mut state, p, "Healer's Hawk", Zone::Battlefield);
        let other_flyer = put(&mut state, p.opponent(), "Healer's Hawk", Zone::Battlefield);
        let late_elf = heroic.then(|| put(&mut state, p, "Llanowar Elves", Zone::Hand));
        let counter = put(&mut state, p.opponent(), "Counterspell", Zone::Hand);
        let spell = put(&mut state, p, name, Zone::Hand);
        let mut native = FastActorSessionV1::from_kaito_v75_fixture_state(state.clone());
        let mut own =
            KaitoSession::from_session(FastActorSessionV1::from_kaito_v75_fixture_state(state))
                .unwrap();
        if let Some(land) = creature_land {
            act(
                &mut own,
                &mut native,
                |a| matches!(a, A::ActivateAbility { source, ability_index: 0, .. } if source.arena_id == land.0),
            );
            settle(&mut own, &mut native);
            assert!(crate::engine::object_has_type(
                own.fixture_state(),
                land,
                crate::card_def::CardType::Land
            ));
            assert!(crate::engine::object_has_type(
                own.fixture_state(),
                land,
                crate::card_def::CardType::Creature
            ));
        }
        let tapped_before = own.fixture_state().players[p.index()]
            .battlefield
            .iter()
            .filter(|&&card| own.fixture_state().objects.get(card).tapped)
            .count();
        act(
            &mut own,
            &mut native,
            |a| matches!(a,A::CastSpell { source, .. } if source.arena_id == spell.0),
        );
        assert_eq!(own.fixture_state().objects.get(spell).zone, Zone::Stack);
        let cost = crate::card_def::CARD_DEFS[usize::from(id)].cost;
        let paid = usize::from(cost.generic) + cost.pips.len();
        assert_eq!(
            own.fixture_state().players[p.index()]
                .battlefield
                .iter()
                .filter(|&&card| own.fixture_state().objects.get(card).tapped)
                .count(),
            paid + tapped_before
        );
        assert_eq!(own.fixture_state().players[p.index()].mana_pool, [0; 6]);
        let saved = own.snapshot();
        let raw = native.snapshot_v1();
        let creature_land_tapped =
            creature_land.is_some_and(|land| own.fixture_state().objects.get(land).tapped);
        for response in [0, 1, 2] {
            if response == 2 && !bounty {
                continue;
            }
            let countered = response == 1;
            let land_destroyed = response == 2;
            let mut expected = None;
            for _ in 0..2 {
                own.restore(&saved).unwrap();
                native.kaito_restore_v75(&raw).unwrap();
                if countered {
                    act(&mut own, &mut native, |a| matches!(a, A::Pass { .. }));
                    act(
                        &mut own,
                        &mut native,
                        |a| matches!(a,A::CastSpell { source, .. } if source.arena_id == counter.0),
                    );
                    act(
                        &mut own,
                        &mut native,
                        |a| matches!(a,A::ChooseTarget { target: crate::rl::TargetRefV1::StackItem { source, .. }, .. } if source.arena_id == spell.0),
                    );
                } else if land_destroyed {
                    let removal = land_removal.unwrap();
                    let land = creature_land.unwrap();
                    act(&mut own, &mut native, |a| matches!(a, A::Pass { .. }));
                    act(
                        &mut own,
                        &mut native,
                        |a| matches!(a,A::CastSpell { source, .. } if source.arena_id == removal.0),
                    );
                    act(
                        &mut own,
                        &mut native,
                        |a| matches!(a,A::ChooseTarget { target: crate::rl::TargetRefV1::Object { object }, .. } if object.arena_id == land.0),
                    );
                }
                settle(&mut own, &mut native);
                let state = own.fixture_state();
                assert_eq!(state.objects.get(spell).zone, Zone::Graveyard);
                assert_eq!(state.players[p.index()].mana_pool, [0; 6]);
                assert_eq!(
                    state.players[p.index()]
                        .battlefield
                        .iter()
                        .filter(|&&card| state.objects.get(card).tapped)
                        .count(),
                    paid + tapped_before - usize::from(land_destroyed && creature_land_tapped)
                );
                assert_eq!(state.players[p.index()].life, 20);
                assert_eq!(state.players[p.opponent().index()].life, 20);
                for ground in [own_ground, other_ground] {
                    assert_eq!(
                        state.objects.get(ground).zone,
                        if seismic && !countered {
                            Zone::Graveyard
                        } else {
                            Zone::Battlefield
                        }
                    );
                }
                for flyer in [own_flyer, other_flyer] {
                    assert_eq!(state.objects.get(flyer).zone, Zone::Battlefield);
                    assert_eq!(state.objects.get(flyer).damage, 0);
                }
                assert_eq!(
                    crate::engine::checked_effective_pt(state, other_flyer).unwrap(),
                    (1, 1)
                );
                if !seismic || countered {
                    assert_eq!(
                        crate::engine::checked_effective_pt(state, other_ground).unwrap(),
                        (2, 2)
                    );
                }
                let soldier_id = crate::card_def::card_id_by_name("Soldier Token").unwrap();
                let treasure_id = crate::card_def::card_id_by_name("Treasure Token").unwrap();
                let tokens = |id| {
                    state.players[p.index()]
                        .battlefield
                        .iter()
                        .copied()
                        .filter(|&card| state.objects.get(card).card_def == id)
                        .collect::<Vec<_>>()
                };
                let soldiers = tokens(soldier_id);
                let treasures = tokens(treasure_id);
                assert_eq!(soldiers.len(), if heroic && !countered { 2 } else { 0 });
                assert_eq!(
                    treasures.len(),
                    if bounty && !countered {
                        if land_destroyed {
                            10
                        } else {
                            11
                        }
                    } else {
                        0
                    }
                );
                if let Some(land) = creature_land {
                    assert_eq!(
                        state.objects.get(land).zone,
                        if land_destroyed {
                            Zone::Graveyard
                        } else {
                            Zone::Battlefield
                        }
                    );
                }
                for &token in soldiers.iter().chain(treasures.iter()) {
                    let object = state.objects.get(token);
                    assert!(object.v4.is_token && !object.tapped);
                    assert_eq!((object.owner, object.controller), (p, p));
                }
                if heroic && !countered {
                    assert_eq!(
                        crate::engine::checked_effective_pt(state, own_ground).unwrap(),
                        (3, 3)
                    );
                    assert_eq!(
                        crate::engine::checked_effective_pt(state, own_flyer).unwrap(),
                        (2, 2)
                    );
                    for &token in &soldiers {
                        assert_eq!(
                            crate::engine::checked_effective_pt(state, token).unwrap(),
                            (2, 2)
                        );
                        assert!(crate::engine::has_effective_keyword(
                            state,
                            token,
                            Keywords::HASTE
                        ));
                        assert_eq!(state.objects.get(token).counters.plus1_plus1, 0);
                    }
                    let elf = late_elf.unwrap();
                    act(
                        &mut own,
                        &mut native,
                        |a| matches!(a,A::CastSpell{source,..}if source.arena_id==elf.0),
                    );
                    settle(&mut own, &mut native);
                    assert_eq!(
                        crate::engine::checked_effective_pt(own.fixture_state(), elf).unwrap(),
                        (1, 1)
                    );
                    assert!(!crate::engine::has_effective_keyword(
                        own.fixture_state(),
                        elf,
                        Keywords::HASTE
                    ));
                    super::ordinary_equipment::reach_main(&mut own, &mut native, p.opponent());
                    let state = own.fixture_state();
                    assert_eq!(
                        crate::engine::checked_effective_pt(state, own_ground).unwrap(),
                        (2, 2)
                    );
                    assert_eq!(
                        crate::engine::checked_effective_pt(state, own_flyer).unwrap(),
                        (1, 1)
                    );
                    for &token in &soldiers {
                        assert_eq!(
                            crate::engine::checked_effective_pt(state, token).unwrap(),
                            (1, 1)
                        );
                        assert!(!crate::engine::has_effective_keyword(
                            state,
                            token,
                            Keywords::HASTE
                        ));
                    }
                }
                if let Some(ref state) = expected {
                    assert_eq!(own.fixture_state(), state);
                } else {
                    expected = Some(own.fixture_state().clone());
                }
            }
        }
    }
}
