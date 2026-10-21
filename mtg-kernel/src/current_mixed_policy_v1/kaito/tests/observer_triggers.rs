use super::*;
use crate::{
    event::CommittedEvent,
    policy_observation_v6::tests::put,
    state::{GameState, StackItemKind, Step, Zone},
};

#[test]
fn paid_pridemate_gain_event_count_original_source_departure_and_restore_both_seats() {
    paid_observer("Ajani's Pridemate");
}
#[test]
fn paid_blight_priest_positive_gain_life_loss_source_departure_and_restore_both_seats() {
    paid_observer("Marauding Blight-Priest");
}
#[test]
fn paid_dazzling_other_creature_gain_source_departure_and_restore_both_seats() {
    paid_observer("Dazzling Angel");
}
#[test]
fn paid_youthful_actual_angel_subtype_filter_original_source_departure_and_restore_both_seats() {
    paid_observer("Youthful Valkyrie");
}
#[test]
fn paid_lagac_actual_landfall_damage_source_departure_and_restore_both_seats() {
    paid_observer("Spitfire Lagac");
}
#[test]
fn paid_tatyova_actual_landfall_ordered_gain_draw_source_departure_and_restore_both_seats() {
    paid_observer("Tatyova, Benthic Druid");
}
#[test]
fn paid_brineborn_flash_offturn_cast_counter_source_departure_and_restore_both_seats() {
    paid_observer("Brineborn Cutthroat");
}
#[test]
fn paid_elementalist_flash_prowess_source_departure_and_restore_both_seats() {
    paid_observer("Elementalist Adept");
}
#[test]
fn paid_archer_noncreature_cast_damage_source_departure_and_restore_both_seats() {
    paid_observer("Firebrand Archer");
}
#[test]
fn paid_cyclops_noncreature_cast_temporary_boost_source_departure_and_restore_both_seats() {
    paid_observer("Crackling Cyclops");
}
#[test]
fn paid_elfsworn_landfall_token_source_departure_and_restore_both_seats() {
    paid_observer("Elfsworn Giant");
}
#[test]
fn paid_skymage_controller_draw_counter_source_departure_and_restore_both_seats() {
    paid_observer("Clinquant Skymage");
}
#[test]
fn paid_erudite_exact_second_draw_counter_source_departure_and_restore_both_seats() {
    paid_observer("Erudite Wizard");
}
#[test]
fn paid_beast_kin_other_creature_temporary_boost_source_departure_and_restore_both_seats() {
    paid_observer("Beast-Kin Ranger");
}

fn paid_observer(name: &str) {
    let forest = crate::card_def::card_id_by_name("Forest").unwrap();
    let life_observer = matches!(name, "Ajani's Pridemate" | "Marauding Blight-Priest");
    let source_counter = matches!(
        name,
        "Ajani's Pridemate"
            | "Youthful Valkyrie"
            | "Brineborn Cutthroat"
            | "Clinquant Skymage"
            | "Erudite Wizard"
    );
    let offturn_cast = matches!(name, "Brineborn Cutthroat" | "Elementalist Adept");
    let spell_cast_observer =
        offturn_cast || matches!(name, "Firebrand Archer" | "Crackling Cyclops");
    let draw_observer = matches!(name, "Clinquant Skymage" | "Erudite Wizard");
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut state = GameState::new_from_libraries_with_starting_player_v1(
            &[forest; 40],
            &[forest; 40],
            crate::rl::card_name,
            110_296,
            if offturn_cast { p.opponent() } else { p },
        );
        state.step = Step::Main1;
        for land in ["Plains", "Island", "Swamp", "Mountain", "Forest"] {
            for _ in 0..2 {
                put(&mut state, p, land, Zone::Battlefield);
            }
        }
        for _ in 0..5 {
            put(&mut state, p.opponent(), "Swamp", Zone::Battlefield);
        }
        if name == "Youthful Valkyrie" {
            // The first1W payment may tap both original Plains. Preserve a
            // real white source for the later2W Angel cast after the Elf.
            put(&mut state, p, "Plains", Zone::Battlefield);
        }
        if draw_observer {
            put(&mut state, p, "Island", Zone::Battlefield);
            for _ in 0..2 {
                put(&mut state, p.opponent(), "Island", Zone::Battlefield);
            }
        }
        let removal = put(&mut state, p.opponent(), "Hero's Downfall", Zone::Hand);
        let source = put(&mut state, p, name, Zone::Hand);
        let bolt = spell_cast_observer.then(|| put(&mut state, p, "Lightning Bolt", Zone::Hand));
        let other_draw =
            draw_observer.then(|| put(&mut state, p.opponent(), "Think Twice", Zone::Hand));
        let own_draw = draw_observer.then(|| put(&mut state, p, "Think Twice", Zone::Hand));
        let first_draw =
            (name == "Erudite Wizard").then(|| put(&mut state, p, "Think Twice", Zone::Hand));
        // Explicit initial Food tokens are support operands. Both life events
        // below use the native paid mana/tap/sacrifice activation and real stack.
        let own_food = life_observer.then(|| put(&mut state, p, "Food Token", Zone::Battlefield));
        let other_food =
            life_observer.then(|| put(&mut state, p.opponent(), "Food Token", Zone::Battlefield));
        let non_angel = matches!(
            name,
            "Youthful Valkyrie" | "Firebrand Archer" | "Crackling Cyclops"
        )
        .then(|| put(&mut state, p, "Llanowar Elves", Zone::Hand));
        let entering_name = match name {
            "Dazzling Angel" | "Beast-Kin Ranger" => Some("Llanowar Elves"),
            "Youthful Valkyrie" => Some("Dazzling Angel"),
            "Spitfire Lagac" | "Tatyova, Benthic Druid" | "Elfsworn Giant" => Some("Forest"),
            _ => None,
        };
        let entering = entering_name.map(|card| put(&mut state, p, card, Zone::Hand));
        let id = crate::card_def::card_id_by_name(name).unwrap();
        let meaning = super::super::super::roots::baseline(id).unwrap();
        let root = &meaning["observer_trigger_printing"];
        assert_eq!(
            root["ordered_triggers"][0]["targets"],
            serde_json::json!([])
        );
        assert_eq!(
            root["ordered_triggers"][0]["captures_source_incarnation_and_controller_at_event"],
            true
        );
        assert_eq!(
            super::super::super::roots::registered_permanent_printing(id).unwrap(),
            Some(meaning)
        );
        let mut native = FastActorSessionV1::from_kaito_v75_fixture_state(state.clone());
        let mut own =
            KaitoSession::from_session(FastActorSessionV1::from_kaito_v75_fixture_state(state))
                .unwrap();
        if offturn_cast {
            assert_eq!(own.fixture_state().priority_player, p.opponent());
            act(&mut own, &mut native, |a| matches!(a, A::Pass { .. }));
        }
        act(
            &mut own,
            &mut native,
            |a| matches!(a,A::CastSpell { source: card, .. } if card.arena_id == source.0),
        );
        let cost = &crate::card_def::CARD_DEFS[usize::from(id)].cost;
        assert_eq!(
            own.fixture_state().players[p.index()]
                .battlefield
                .iter()
                .filter(|&&id| own.fixture_state().objects.get(id).tapped)
                .count(),
            usize::from(cost.generic) + cost.pips.len()
        );
        settle(&mut own, &mut native);
        assert_eq!(
            own.fixture_state().objects.get(source).zone,
            Zone::Battlefield
        );
        assert_eq!(
            own.fixture_state().objects.get(source).counters.plus1_plus1,
            0
        );
        assert_eq!(own.fixture_state().players[p.index()].life, 20);
        assert_eq!(own.fixture_state().players[p.index()].library.len(), 40);
        // Its own entry cannot count as another creature, an Angel entering
        // under another observer, a life-gain event or landfall.
        if let Some(draw) = other_draw {
            act(&mut own, &mut native, |a| matches!(a, A::Pass { .. }));
            act(
                &mut own,
                &mut native,
                |a| matches!(a, A::CastSpell { source: card, .. } if card.arena_id == draw.0),
            );
            settle(&mut own, &mut native);
            assert_eq!(
                own.fixture_state().players[p.opponent().index()]
                    .library
                    .len(),
                39
            );
            assert_eq!(own.fixture_state().players[p.index()].library.len(), 40);
            assert_eq!(
                own.fixture_state().objects.get(source).counters.plus1_plus1,
                0
            );
        }
        if let Some(draw) = first_draw {
            act(
                &mut own,
                &mut native,
                |a| matches!(a, A::CastSpell { source: card, .. } if card.arena_id == draw.0),
            );
            settle(&mut own, &mut native);
            assert_eq!(own.fixture_state().players[p.index()].library.len(), 39);
            assert_eq!(
                own.fixture_state().objects.get(source).counters.plus1_plus1,
                0
            );
        }
        if let Some(food) = other_food {
            act(&mut own, &mut native, |a| matches!(a, A::Pass { .. }));
            act(
                &mut own,
                &mut native,
                |a| matches!(a,A::ActivateAbility { source: card, ability_index: 0, .. } if card.arena_id == food.0),
            );
            settle(&mut own, &mut native);
            assert_eq!(own.fixture_state().players[p.opponent().index()].life, 23);
            assert_eq!(own.fixture_state().players[p.index()].life, 20);
            assert_eq!(
                own.fixture_state().objects.get(source).counters.plus1_plus1,
                0
            );
        }
        if let Some(elf) = non_angel {
            act(
                &mut own,
                &mut native,
                |a| matches!(a,A::CastSpell { source: card, .. } if card.arena_id == elf.0),
            );
            settle(&mut own, &mut native);
            assert_eq!(own.fixture_state().objects.get(elf).zone, Zone::Battlefield);
            assert_eq!(
                own.fixture_state().objects.get(source).counters.plus1_plus1,
                0
            );
        }
        if let Some(draw) = own_draw {
            act(
                &mut own,
                &mut native,
                |a| matches!(a, A::CastSpell { source: card, .. } if card.arena_id == draw.0),
            );
        } else if let Some(bolt) = bolt {
            if offturn_cast {
                assert_eq!(own.fixture_state().priority_player, p.opponent());
                act(&mut own, &mut native, |a| matches!(a, A::Pass { .. }));
            }
            act(
                &mut own,
                &mut native,
                |a| matches!(a,A::CastSpell { source: card, .. } if card.arena_id == bolt.0),
            );
            act(&mut own, &mut native, |a| {
                matches!(a,A::ChooseTarget { target: crate::rl::TargetRefV1::Player { player }, .. }
                if *player == crate::rl::PlayerSeatV1::from(p.opponent()))
            });
        } else if let Some(food) = own_food {
            act(
                &mut own,
                &mut native,
                |a| matches!(a,A::ActivateAbility { source: card, ability_index: 0, .. } if card.arena_id == food.0),
            );
        } else {
            let entering = entering.unwrap();
            if matches!(
                name,
                "Spitfire Lagac" | "Tatyova, Benthic Druid" | "Elfsworn Giant"
            ) {
                act(
                    &mut own,
                    &mut native,
                    |a| matches!(a,A::PlayLand { source: card, .. } if card.arena_id == entering.0),
                );
            } else {
                act(
                    &mut own,
                    &mut native,
                    |a| matches!(a,A::CastSpell { source: card, .. } if card.arena_id == entering.0),
                );
            }
        }
        for _ in 0..32 {
            if own.fixture_state().stack.last().is_some_and(|item| {
                item.source == source && item.kind == StackItemKind::TriggeredAbility
            }) {
                break;
            }
            act(&mut own, &mut native, |a| {
                matches!(
                    a,
                    A::Pass { .. } | A::OrderTriggers { .. } | A::ChooseTriggerOrderNext { .. }
                )
            });
        }
        let trigger = own
            .fixture_state()
            .stack
            .last()
            .expect("actual observer trigger")
            .clone();
        assert_eq!(trigger.source, source);
        assert_eq!(trigger.kind, StackItemKind::TriggeredAbility);
        assert_eq!(trigger.controller, p);
        assert_eq!(
            own.fixture_state().objects.get(source).counters.plus1_plus1,
            0
        );
        let saved = own.snapshot();
        let raw = native.snapshot_v1();
        for removed in [false, true] {
            let mut expected = None;
            for _ in 0..2 {
                own.restore(&saved).unwrap();
                native.kaito_restore_v75(&raw).unwrap();
                if removed {
                    act(&mut own, &mut native, |a| matches!(a, A::Pass { .. }));
                    act(
                        &mut own,
                        &mut native,
                        |a| matches!(a,A::CastSpell { source: card, .. } if card.arena_id == removal.0),
                    );
                    act(
                        &mut own,
                        &mut native,
                        |a| matches!(a,A::ChooseTarget { target: crate::rl::TargetRefV1::Object { object }, .. } if object.arena_id == source.0),
                    );
                }
                settle(&mut own, &mut native);
                let state = own.fixture_state();
                assert_eq!(
                    state.objects.get(source).zone,
                    if removed {
                        Zone::Graveyard
                    } else {
                        Zone::Battlefield
                    }
                );
                assert_eq!(
                    state.objects.get(source).counters.plus1_plus1,
                    i32::from(source_counter && !removed)
                );
                if !removed {
                    let def = &crate::card_def::CARD_DEFS[usize::from(id)];
                    let boost_power = if name == "Crackling Cyclops" {
                        3
                    } else {
                        i32::from(matches!(name, "Elementalist Adept" | "Beast-Kin Ranger"))
                    };
                    let boost_toughness = i32::from(name == "Elementalist Adept");
                    assert_eq!(
                        crate::engine::checked_effective_pt(state, source).unwrap(),
                        (
                            i32::from(def.power.unwrap()) + i32::from(source_counter) + boost_power,
                            i32::from(def.toughness.unwrap())
                                + i32::from(source_counter)
                                + boost_toughness
                        )
                    );
                }
                let own_life = if life_observer {
                    23
                } else if matches!(name, "Dazzling Angel" | "Tatyova, Benthic Druid") {
                    21
                } else {
                    20
                };
                assert_eq!(state.players[p.index()].life, own_life);
                if name == "Elfsworn Giant" {
                    let token = crate::card_def::card_id_by_name("Elf Warrior Token").unwrap();
                    let created = state.players[p.index()]
                        .battlefield
                        .iter()
                        .copied()
                        .filter(|&id| state.objects.get(id).card_def == token)
                        .collect::<Vec<_>>();
                    assert_eq!(created.len(), 1);
                    let object = state.objects.get(created[0]);
                    assert!(object.v4.is_token && object.summoning_sick && !object.tapped);
                    assert_eq!((object.owner, object.controller), (p, p));
                    assert_eq!(
                        crate::engine::checked_effective_pt(state, created[0]).unwrap(),
                        (1, 1)
                    );
                }
                let other_life = if life_observer { 23 } else { 20 }
                    - i32::from(matches!(
                        name,
                        "Marauding Blight-Priest" | "Spitfire Lagac" | "Firebrand Archer"
                    ))
                    - if spell_cast_observer { 3 } else { 0 };
                assert_eq!(state.players[p.opponent().index()].life, other_life);
                assert_eq!(
                    state.players[p.index()].library.len(),
                    if name == "Erudite Wizard" {
                        38
                    } else if matches!(name, "Tatyova, Benthic Druid" | "Clinquant Skymage") {
                        39
                    } else {
                        40
                    }
                );
                assert_eq!(
                    state
                        .engine
                        .event_history
                        .iter()
                        .filter(|event| matches!(event,
                    CommittedEvent::Damage { source: actual, amount: 1, .. } if *actual == source))
                        .count(),
                    usize::from(matches!(name, "Spitfire Lagac" | "Firebrand Archer"))
                );
                if let Some(expected) = &expected {
                    assert_eq!(state, expected);
                } else {
                    expected = Some(state.clone());
                }
            }
        }
    }
}
