use super::*;
use crate::{
    card_def::CARD_DEFS,
    policy_observation_v6::tests::put,
    state::{GameState, Step, Zone},
};

#[test]
fn goblin_bushwhacker_paid_kicker_and_entry_root_both_seats() {
    use super::{
        surveil_many::{checkpoint, sessions},
        targeted_entry::{actions, cast},
    };
    use crate::{
        card_def::{card_id_by_name, Keywords},
        engine::{
            effective_power, effective_toughness, has_effective_keyword, EffectDuration,
            UntilEndOfTurnEffect,
        },
        state::StackItemKind,
    };

    let forest = card_id_by_name("Forest").unwrap();
    let id = card_id_by_name("Goblin Bushwhacker").unwrap();
    assert_eq!(id, 44);
    let meaning = super::super::super::roots::baseline(id).unwrap();
    assert_eq!(
        super::super::super::roots::registered_permanent_printing(id).unwrap(),
        Some(meaning.clone())
    );
    let printed = &meaning["simple_trigger_printing"];
    assert_eq!(
        printed["ordered_triggers"][0]["intervening_if"]["operation"],
        "EnteringSpellWasKicked"
    );
    assert_eq!(
        printed["ordered_triggers"][0]["ordered_resolution"][0]["grant_keyword"],
        "Haste"
    );

    for actor in [PlayerId::P0, PlayerId::P1] {
        for kicked in [false, true] {
            let cast_root = &printed["ordered_cast_roots"][usize::from(kicked)];
            assert_eq!(cast_root["optional_additional_cost"]["selected"], kicked);
            assert_eq!(
                cast_root["ordered_costs"][0]["base_total_before_increases_and_reductions"]
                    ["pips"]
                    .as_array()
                    .unwrap()
                    .len(),
                1 + usize::from(kicked)
            );
            let mut state = GameState::new_from_libraries_with_starting_player_v1(
                &[forest; 40],
                &[forest; 40],
                crate::rl::card_name,
                552_044,
                actor,
            );
            state.step = Step::Main1;
            let mountains = [0; 2].map(|_| put(&mut state, actor, "Mountain", Zone::Battlefield));
            let ally = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
            let opponent = put(&mut state, actor.opponent(), "Llanowar Elves", Zone::Battlefield);
            let source = put(&mut state, actor, "Goblin Bushwhacker", Zone::Hand);
            let (mut owner, mut native) = sessions(state);
            cast(&mut owner, &mut native, source);
            let choices = actions(&owner);
            for pay in [false, true] {
                assert!(choices.iter().any(|a| matches!(a,
                    A::ChooseKicker {source: cast_source, pay: answer, ..}
                        if cast_source.arena_id == source.0 && *answer == pay
                )));
            }
            checkpoint(&mut owner, &mut native);
            act(&mut owner, &mut native, |a| matches!(a,
                A::ChooseKicker {source: cast_source, pay, ..}
                    if cast_source.arena_id == source.0 && *pay == kicked
            ));
            let paid = owner.fixture_state();
            let spell = paid.stack.last().unwrap();
            assert_eq!(spell.kind, StackItemKind::Spell);
            assert_eq!(spell.source, source);
            assert_eq!(spell.kicked, kicked);
            assert_eq!(crate::engine::stack_spell_mana_value(paid, spell), 1);
            assert_eq!(
                mountains.iter().filter(|&&land| paid.objects.get(land).tapped).count(),
                1 + usize::from(kicked)
            );
            assert_eq!(paid.players[actor.index()].mana_pool, [0; 6]);

            let mut saw_trigger = false;
            for _ in 0..16 {
                let state = owner.fixture_state();
                if let Some(item) = state.stack.iter().find(|item| {
                    item.source == source && item.kind == StackItemKind::TriggeredAbility
                }) {
                    assert_eq!(item.controller, actor);
                    assert!(item.kicked);
                    assert!(item.targets.is_empty());
                    saw_trigger = true;
                }
                if state.stack.is_empty() && state.engine.pending_triggers.is_empty() {
                    break;
                }
                act(&mut owner, &mut native, |a| {
                    matches!(
                        a,
                        A::Pass { .. }
                            | A::OrderTriggers { .. }
                            | A::ChooseTriggerOrderNext { .. }
                    )
                });
            }
            let resolved = owner.fixture_state();
            assert!(resolved.stack.is_empty());
            assert_eq!(saw_trigger, kicked);
            assert_eq!(resolved.objects.get(source).zone, Zone::Battlefield);
            for creature in [source, ally] {
                assert_eq!(effective_power(resolved, creature), 1 + i32::from(kicked));
                assert_eq!(effective_toughness(resolved, creature), 1);
                assert_eq!(has_effective_keyword(resolved, creature, Keywords::HASTE), kicked);
            }
            assert_eq!(effective_power(resolved, opponent), 1);
            assert!(!has_effective_keyword(resolved, opponent, Keywords::HASTE));
            assert_eq!(effective_power(resolved, mountains[0]), 0);
            assert_eq!(
                resolved.engine.until_end_of_turn.iter().any(|effect| matches!(
                    effect,
                    UntilEndOfTurnEffect::ResolvedSetEffect {
                        object_ids, duration: EffectDuration::EndOfTurn,
                        power: 1, toughness: 0, grant_haste: true, ..
                    } if object_ids.len() == 2
                        && object_ids.contains(&source) && object_ids.contains(&ally)
                )),
                kicked
            );
        }
    }
}

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
