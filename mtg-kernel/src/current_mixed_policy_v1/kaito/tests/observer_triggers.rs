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

fn paid_observer(name: &str) {
    let forest = crate::card_def::card_id_by_name("Forest").unwrap();
    let life_observer = matches!(name, "Ajani's Pridemate" | "Marauding Blight-Priest");
    let source_counter = matches!(name, "Ajani's Pridemate" | "Youthful Valkyrie");
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut state = GameState::new_from_libraries_with_starting_player_v1(
            &[forest; 40],
            &[forest; 40],
            crate::rl::card_name,
            110_296,
            p,
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
        let removal = put(&mut state, p.opponent(), "Hero's Downfall", Zone::Hand);
        let source = put(&mut state, p, name, Zone::Hand);
        // Explicit initial Food tokens are support operands. Both life events
        // below use the native paid mana/tap/sacrifice activation and real stack.
        let own_food = life_observer.then(|| put(&mut state, p, "Food Token", Zone::Battlefield));
        let other_food =
            life_observer.then(|| put(&mut state, p.opponent(), "Food Token", Zone::Battlefield));
        let non_angel =
            (name == "Youthful Valkyrie").then(|| put(&mut state, p, "Llanowar Elves", Zone::Hand));
        let entering_name = match name {
            "Dazzling Angel" => Some("Llanowar Elves"),
            "Youthful Valkyrie" => Some("Dazzling Angel"),
            "Spitfire Lagac" | "Tatyova, Benthic Druid" => Some("Forest"),
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
        if let Some(food) = own_food {
            act(
                &mut own,
                &mut native,
                |a| matches!(a,A::ActivateAbility { source: card, ability_index: 0, .. } if card.arena_id == food.0),
            );
        } else {
            let entering = entering.unwrap();
            if matches!(name, "Spitfire Lagac" | "Tatyova, Benthic Druid") {
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
                let own_life = if life_observer {
                    23
                } else if matches!(name, "Dazzling Angel" | "Tatyova, Benthic Druid") {
                    21
                } else {
                    20
                };
                assert_eq!(state.players[p.index()].life, own_life);
                let other_life = if life_observer { 23 } else { 20 }
                    - i32::from(matches!(name, "Marauding Blight-Priest" | "Spitfire Lagac"));
                assert_eq!(state.players[p.opponent().index()].life, other_life);
                assert_eq!(
                    state.players[p.index()].library.len(),
                    if name == "Tatyova, Benthic Druid" {
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
                    usize::from(name == "Spitfire Lagac")
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
