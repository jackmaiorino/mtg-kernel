use super::*;
use crate::{
    card_def::CARD_DEFS,
    policy_observation_v6::tests::put,
    state::{GameState, Step, Zone},
};

const NAMES: [&str; 8] = [
    "Helpful Hunter",
    "Icewind Elemental",
    "Burglar Rat",
    "Infestation Sage",
    "Dragon Trainer",
    "Resolute Reinforcements",
    "Gleaming Barrier",
    "Prideful Parent",
];

#[test]
fn eight_entry_death_printings_paid_native_numeric_choices_and_restore_both_seats() {
    let forest = crate::card_def::card_id_by_name("Forest").unwrap();
    for name in NAMES {
        let def = &CARD_DEFS[usize::from(crate::card_def::card_id_by_name(name).unwrap())];
        let meaning =
            super::super::super::roots::baseline(crate::card_def::card_id_by_name(name).unwrap())
                .unwrap();
        let printed = &meaning["simple_trigger_printing"];
        assert_eq!(
            super::super::super::roots::registered_permanent_printing(
                crate::card_def::card_id_by_name(name).unwrap()
            )
            .unwrap(),
            Some(meaning.clone())
        );
        assert_eq!(
            printed["ordered_triggers"][0]["targets"],
            serde_json::json!([])
        );
        assert_eq!(
            printed["ordered_triggers"][0]["survives_source_departure_and_later_control_change"],
            true
        );
        assert_eq!(
            printed["ordinary_cast"]["timing"],
            if name == "Resolute Reinforcements" {
                "InstantWithPriority"
            } else {
                "OwnMainPhaseEmptyStackWithPriority"
            }
        );
        for p in [PlayerId::P0, PlayerId::P1] {
            let mut s = GameState::new_from_libraries_with_starting_player_v1(
                &[forest; 40],
                &[forest; 40],
                crate::rl::card_name,
                110_290,
                if name == "Resolute Reinforcements" {
                    p.opponent()
                } else {
                    p
                },
            );
            s.step = Step::Main1;
            for land in ["Plains", "Island", "Swamp", "Mountain"] {
                for _ in 0..5 {
                    put(&mut s, p, land, Zone::Battlefield);
                }
            }
            for _ in 0..3 {
                put(&mut s, p.opponent(), "Swamp", Zone::Battlefield);
            }
            let own_discard = put(&mut s, p, "Llanowar Elves", Zone::Hand);
            let other_discard = put(&mut s, p.opponent(), "Llanowar Elves", Zone::Hand);
            let removal = put(&mut s, p.opponent(), "Hero's Downfall", Zone::Hand);
            let spell = put(&mut s, p, name, Zone::Hand);
            let library_before = s.players[p.index()].library.len();
            if name == "Resolute Reinforcements" {
                assert!(matches!(crate::engine::advance_until_decision(&mut s),
                    crate::engine::Decision::CastSpellOrPass { player, .. } if player == p.opponent()));
                crate::engine::step(&mut s, crate::engine::Action::Pass).unwrap();
            }
            let mut native = FastActorSessionV1::from_kaito_v75_fixture_state(s.clone());
            let mut own =
                KaitoSession::from_session(FastActorSessionV1::from_kaito_v75_fixture_state(s))
                    .unwrap();
            act(
                &mut own,
                &mut native,
                |a| matches!(a,A::CastSpell{source,..}if source.arena_id==spell.0),
            );
            assert_eq!(own.fixture_state().objects.get(spell).zone, Zone::Stack);
            assert_eq!(
                own.fixture_state().players[p.index()]
                    .battlefield
                    .iter()
                    .filter(|&&id| own.fixture_state().objects.get(id).tapped)
                    .count(),
                usize::from(def.cost.generic) + def.cost.pips.len()
            );
            let saved = own.snapshot();
            let raw = native.snapshot_v1();
            let mut expected = None;
            for replay in [false, true] {
                if replay {
                    own.restore(&saved).unwrap();
                    native.kaito_restore_v75(&raw).unwrap();
                }
                let mut discards = 0;
                for _ in 0..64 {
                    let w = own.fixture_state();
                    if w.stack.is_empty()
                        && w.engine.pending_effect.is_none()
                        && w.engine.pending_discard.is_none()
                        && w.engine.pending_triggers.is_empty()
                    {
                        break;
                    }
                    let FastActorResponseV1::Decision(h) = own.current_response() else {
                        panic!("entry ended")
                    };
                    let d = own.encode(h).unwrap();
                    if d.backend
                        .actions()
                        .iter()
                        .any(|a| matches!(a, A::Discard { .. }))
                    {
                        let selected = if name == "Burglar Rat" {
                            other_discard
                        } else {
                            own_discard
                        };
                        let checkpoint = own.snapshot();
                        let native_checkpoint = native.snapshot_v1();
                        let before = own.fixture_state().clone();
                        own.restore(&checkpoint).unwrap();
                        native.kaito_restore_v75(&native_checkpoint).unwrap();
                        assert_eq!(own.fixture_state(), &before);
                        act(
                            &mut own,
                            &mut native,
                            |a| matches!(a,A::Discard{cards,..}if cards.len()==1 && cards[0].arena_id==selected.0),
                        );
                        discards += 1;
                    } else {
                        act(&mut own, &mut native, |a| {
                            matches!(
                                a,
                                A::Pass { .. }
                                    | A::OrderTriggers { .. }
                                    | A::ChooseTriggerOrderNext { .. }
                            )
                        });
                    }
                }
                assert!(own.fixture_state().stack.is_empty());
                assert_eq!(
                    own.fixture_state().objects.get(spell).zone,
                    Zone::Battlefield
                );
                assert_eq!(
                    discards,
                    usize::from(matches!(name, "Burglar Rat" | "Icewind Elemental"))
                );
                assert_eq!(
                    own.fixture_state().players[p.index()].library.len(),
                    library_before
                        - usize::from(matches!(name, "Helpful Hunter" | "Icewind Elemental"))
                );
                if matches!(name, "Burglar Rat" | "Icewind Elemental") {
                    let selected = if name == "Burglar Rat" {
                        other_discard
                    } else {
                        own_discard
                    };
                    assert_eq!(
                        own.fixture_state().objects.get(selected).zone,
                        Zone::Graveyard
                    );
                }
                if matches!(name, "Infestation Sage" | "Gleaming Barrier") {
                    // A paid opposing spell produces the real death event.
                    act(&mut own, &mut native, |a| matches!(a, A::Pass { .. }));
                    act(
                        &mut own,
                        &mut native,
                        |a| matches!(a,A::CastSpell{source,..}if source.arena_id==removal.0),
                    );
                    act(
                        &mut own,
                        &mut native,
                        |a| matches!(a,A::ChooseTarget{target:crate::rl::TargetRefV1::Object{object},..}if object.arena_id==spell.0),
                    );
                    settle(&mut own, &mut native);
                    assert_eq!(own.fixture_state().objects.get(spell).zone, Zone::Graveyard);
                }
                let token = match name {
                    "Infestation Sage" => Some("Insect Token"),
                    "Gleaming Barrier" => Some("Treasure Token"),
                    "Dragon Trainer" => Some("Dragon Token"),
                    "Resolute Reinforcements" => Some("Soldier Token"),
                    "Prideful Parent" => Some("Cat Token"),
                    _ => None,
                };
                if let Some(token) = token {
                    let token_def = crate::card_def::card_id_by_name(token).unwrap();
                    let w = own.fixture_state();
                    let tokens = w.players[p.index()]
                        .battlefield
                        .iter()
                        .filter(|&&id| {
                            let object = w.objects.get(id);
                            object.card_def == token_def
                                && object.v4.is_token
                                && object.controller == p
                        })
                        .count();
                    assert_eq!(tokens, 1, "{name} {p:?}");
                    assert!(!w.players[p.opponent().index()]
                        .battlefield
                        .iter()
                        .any(|&id| w.objects.get(id).card_def == token_def));
                }
                if let Some(ref expected) = expected {
                    assert_eq!(own.fixture_state(), expected);
                } else {
                    expected = Some(own.fixture_state().clone());
                }
            }
        }
    }
}
