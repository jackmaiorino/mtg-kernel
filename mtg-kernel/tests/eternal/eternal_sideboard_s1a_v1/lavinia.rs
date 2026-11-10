use super::*;
use mtg_kernel::state::CastMethodV4;

/// Pay for Lavinia through real land activations, then give the opponent
/// priority in their main phase. Every regression runs from both seats.
fn paid_lavinia(actor: PlayerId, library: &[&str]) -> GameState {
    let mut state = game(library);
    let controller = actor.opponent();
    set_active(&mut state, controller, controller);
    for name in ["Plains", "Island"] {
        let land = put(&mut state, controller, name, Zone::Battlefield);
        act(&mut state, Action::ActivateManaAbility(land));
    }
    let lavinia = put(
        &mut state,
        controller,
        "Lavinia, Azorius Renegade",
        Zone::Hand,
    );
    cast(&mut state, lavinia, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, lavinia), Zone::Battlefield);
    assert_eq!(state.players[controller.index()].mana_pool, [0; 6]);
    set_active(&mut state, actor, actor);
    state.engine.priority_passes = [false; 2];
    state
}

fn tap(state: &mut GameState, actor: PlayerId, name: &str) -> ObjectId {
    let source = put(state, actor, name, Zone::Battlefield);
    act(state, Action::ActivateManaAbility(source));
    source
}

#[test]
fn lavinia_counter_trigger_follows_its_controller_after_paid_exchange_both_seats() {
    use mtg_kernel::planeswalker_v1::loyalty;
    use mtg_kernel::state::StackItemKind;

    for controller in [P0, P1] {
        let owner = controller.opponent();
        let mut state = paid_lavinia(controller, &["Island"; 20]);
        let lavinia = state.players[owner.index()]
            .battlefield
            .iter()
            .copied()
            .find(|&card| state.objects.get(card).card_def == id("Lavinia, Azorius Renegade"))
            .unwrap();
        let oko = put(&mut state, controller, "Oko, Thief of Crowns", Zone::Hand);
        for name in ["Forest", "Island", "Island"] {
            tap(&mut state, controller, name);
        }
        cast(&mut state, oko, &[]);
        settled(&mut state);
        assert_eq!(state.players[controller.index()].mana_pool, [0; 6]);
        activate(&mut state, oko, 0, &[]);
        settled(&mut state);
        assert_eq!(loyalty(&state, oko), Some(6));
        let food = state.players[controller.index()]
            .battlefield
            .iter()
            .copied()
            .find(|&card| state.objects.get(card).card_def == id("Food Token"))
            .unwrap();
        let turn = state.turn;
        run_until(
            &mut state,
            |s| s.turn > turn && s.active_player == controller && s.step == Step::Main1,
            |_, _| None,
        );
        activate(
            &mut state,
            oko,
            2,
            &[Target::Object(food), Target::Object(lavinia)],
        );
        settled(&mut state);
        assert_eq!(loyalty(&state, oko), Some(1));
        assert_eq!(state.objects.get(food).controller, owner);
        assert_eq!(state.objects.get(lavinia).owner, owner);
        assert_eq!(state.objects.get(lavinia).controller, controller);

        run_until(
            &mut state,
            |s| s.active_player == owner && s.step == Step::Main1 && s.priority_player == owner,
            |_, _| None,
        );
        let mox = put(&mut state, owner, "Mox Pearl", Zone::Hand);
        assert_eq!(state.players[owner.index()].mana_pool, [0; 6]);
        cast(&mut state, mox, &[]);
        let priority = next(&mut state);
        assert!(matches!(priority, Decision::CastSpellOrPass { .. }));
        assert_eq!(zone(&state, mox), Zone::Stack);
        assert_eq!(state.players[owner.index()].mana_pool, [0; 6]);
        let trigger = state
            .stack
            .last()
            .expect("Lavinia's published counter trigger");
        assert_eq!(trigger.kind, StackItemKind::TriggeredAbility);
        assert_eq!(trigger.source, lavinia);
        assert_eq!(trigger.controller, controller);
        let source = trigger.v4.ability_source_contract.unwrap();
        assert_eq!(source.owner, owner);
        assert_eq!(source.controller, controller);

        let mut restored: GameState =
            serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
        assert_eq!(next(&mut restored), priority);
        for branch in [&mut state, &mut restored] {
            settled(branch);
            assert_eq!(zone(branch, mox), Zone::Graveyard);
            assert_eq!(branch.objects.get(lavinia).controller, controller);
        }
        assert_eq!(state.state_hash(), restored.state_hash());
    }
}

#[test]
fn lavinia_caps_announced_x_and_restored_actions_both_seats() {
    for actor in [P0, P1] {
        let mut state = paid_lavinia(actor, &["Island"; 20]);
        for _ in 0..3 {
            tap(&mut state, actor, "Mountain");
        }
        let ring = tap(&mut state, actor, "Sol Ring");
        let meltdown = put(&mut state, actor, "Meltdown", Zone::Hand);
        cast(&mut state, meltdown, &[]);
        let prompt = next(&mut state);
        assert!(
            matches!(
                prompt,
                Decision::ChooseEffectOption {
                    option_count: 3,
                    ..
                }
            ),
            "{prompt:?}"
        );
        let mut restored: GameState =
            serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
        assert_eq!(prompt, next(&mut restored));
        for continued in [&mut state, &mut restored] {
            let before = continued.state_hash();
            assert!(engine::step(continued, Action::ChooseEffectOption(3)).is_err());
            assert_eq!(
                before,
                continued.state_hash(),
                "illegal X cannot mutate the cast"
            );
            act(continued, Action::ChooseEffectOption(2));
            next(continued);
            assert_eq!(engine::object_mana_value(continued, meltdown), 3);
            assert_eq!(continued.stack.last().unwrap().v4.x_value, 2);
            assert_eq!(
                continued.players[actor.index()]
                    .mana_pool
                    .iter()
                    .copied()
                    .sum::<u8>(),
                2
            );
            settled(continued);
            assert_eq!(zone(continued, ring), Zone::Graveyard);
            assert_eq!(zone(continued, meltdown), Zone::Graveyard);
        }
        assert_eq!(state.state_hash(), restored.state_hash());
    }
}

#[test]
fn lavinia_distinguishes_creature_and_bestow_x_both_seats() {
    for actor in [P0, P1] {
        for (form, maximum, method) in [(0, 3, CastMethodV4::Normal), (1, 1, CastMethodV4::Bestow)]
        {
            let mut state = paid_lavinia(actor, &["Island"; 20]);
            tap(&mut state, actor, "Forest");
            tap(&mut state, actor, "Forest");
            tap(&mut state, actor, "Sol Ring");
            let host = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
            state.objects.get_mut(host).summoning_sick = true;
            let hydra = put(&mut state, actor, "Nyxborn Hydra", Zone::Hand);
            cast(&mut state, hydra, &[]);
            assert!(
                matches!(next(&mut state), Decision::ChooseSpellMode { ref legal_modes, .. } if legal_modes == &[0, 1])
            );
            act(&mut state, Action::ChooseSpellMode(form));
            if form == 1 {
                choose_target(&mut state, Target::Object(host));
            }
            let prompt = next(&mut state);
            assert!(
                matches!(prompt, Decision::ChooseEffectOption { option_count, .. } if option_count == maximum + 1),
                "{prompt:?}"
            );
            act(&mut state, Action::ChooseEffectOption(maximum));
            next(&mut state);
            assert_eq!(state.stack.last().unwrap().v4.cast_method, Some(method));
            assert_eq!(engine::object_mana_value(&state, hydra), maximum + 1);
            settled(&mut state);
            assert_eq!(zone(&state, hydra), Zone::Battlefield);
            assert_eq!(
                state.objects.get(hydra).counters.plus1_plus1,
                i32::from(maximum)
            );
            if form == 1 {
                assert_eq!(
                    state.objects.get(hydra).v4.attached_to.unwrap().object,
                    host
                );
            }
        }
    }
}

#[test]
fn lavinia_uses_selected_split_half_and_printed_overload_value_both_seats() {
    for actor in [P0, P1] {
        let mut state = paid_lavinia(actor, &["Island"; 20]);
        tap(&mut state, actor, "Mountain");
        tap(&mut state, actor, "Mountain");
        let split = put(&mut state, actor, "Rough // Tumble", Zone::Hand);
        pool(&mut state, actor, &[], 6);
        cast_modal(&mut state, split, 0, &[]);
        assert_eq!(
            state.stack.last().unwrap().v4.cast_method,
            Some(CastMethodV4::Normal)
        );
        assert_eq!(engine::object_mana_value(&state, split), 2);
        settled(&mut state);

        let mut state = paid_lavinia(actor, &["Island"; 20]);
        tap(&mut state, actor, "Mountain");
        pool(&mut state, actor, &[], 1);
        let victim = put(
            &mut state,
            actor.opponent(),
            "Llanowar Elves",
            Zone::Battlefield,
        );
        let mine = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
        let spell = put(&mut state, actor, "Electrickery", Zone::Hand);
        cast_modal(&mut state, spell, 1, &[]);
        assert_eq!(
            state.stack.last().unwrap().v4.cast_method,
            Some(CastMethodV4::Omen)
        );
        assert_eq!(engine::object_mana_value(&state, spell), 1);
        settled(&mut state);
        assert_eq!(zone(&state, victim), Zone::Graveyard);
        assert_eq!(zone(&state, mine), Zone::Battlefield);
        assert_eq!(state.players[actor.index()].mana_pool, [0; 6]);
    }
}

#[test]
#[cfg(feature = "standard-magezero-fixtures")]
fn lavinia_uses_room_door_value_both_seats() {
    for actor in [P0, P1] {
        let mut state = paid_lavinia(actor, &["Island"; 20]);
        for _ in 0..3 {
            tap(&mut state, actor, "Swamp");
        }
        pool(&mut state, actor, &[], 5);
        let room = put(
            &mut state,
            actor,
            "Unholy Annex // Ritual Chamber",
            Zone::Hand,
        );
        cast(&mut state, room, &[]);
        let prompt = next(&mut state);
        assert!(
            matches!(prompt, Decision::CastSpellOrPass { .. }),
            "{prompt:?}"
        );
        assert_eq!(engine::object_mana_value(&state, room), 3);
        assert_eq!(
            state.stack.last().unwrap().v4.cast_method,
            Some(CastMethodV4::Normal)
        );
        settled(&mut state);
        assert_eq!(zone(&state, room), Zone::Battlefield);
    }
}

#[test]
fn lavinia_restricts_free_resolution_casts_before_the_offer_both_seats() {
    for actor in [P0, P1] {
        for lands in [0, 1] {
            let mut state = paid_lavinia(actor, &["Island", "Lightning Bolt", "Plains", "Plains"]);
            let bolt = state.players[actor.index()].library[1];
            if lands == 1 {
                put(&mut state, actor, "Mountain", Zone::Battlefield);
            }
            let party = put(&mut state, actor, "Boarding Party", Zone::Hand);
            pool(&mut state, actor, &[ManaColor::R], 5);
            cast(&mut state, party, &[]);
            let mut offers = 0;
            settle_with(&mut state, |_, decision| match decision {
                Decision::ChooseEffectOption { option_count, .. } => {
                    assert_eq!(lands, 1, "a prohibited Bolt must not be offered");
                    assert_eq!(option_count, 2);
                    offers += 1;
                    Action::ChooseEffectOption(0)
                }
                Decision::ChooseEffectTargets { legal_targets, .. }
                    if legal_targets == vec![Target::Object(bolt)] =>
                {
                    Action::ChooseEffectTarget(Target::Object(bolt))
                }
                Decision::ChooseTargets { .. } => {
                    Action::ChooseTarget(Target::Player(actor.opponent()))
                }
                other => first_answer(other),
            });
            assert_eq!(offers, lands);
            assert_eq!(
                zone(&state, party),
                Zone::Battlefield,
                "the creature was paid for"
            );
            assert_eq!(state.players[actor.index()].mana_pool, [0; 6]);
            assert_eq!(state.players[actor.opponent().index()].life, 20);
            if lands == 0 {
                assert_eq!(zone(&state, bolt), Zone::Library);
            } else {
                assert_eq!(
                    zone(&state, bolt),
                    Zone::Graveyard,
                    "legal free cast is countered by Lavinia's separate trigger"
                );
            }
        }
    }
}

#[test]
fn lavinia_refuses_madness_before_target_announcement_both_seats() {
    for actor in [P0, P1] {
        let mut state = paid_lavinia(actor, &["Island"; 20]);
        tap(&mut state, actor, "Mountain");
        pool(&mut state, actor, &[ManaColor::R], 0);
        let temper = put(&mut state, actor, "Fiery Temper", Zone::Hand);
        let looting = put(&mut state, actor, "Faithless Looting", Zone::Hand);
        cast(&mut state, looting, &[]);
        let mut offered = false;
        settle_with(&mut state, |state, decision| match decision {
            Decision::Discard { count, choices, .. } => {
                assert!(choices.contains(&temper));
                let mut chosen = vec![temper];
                chosen.extend(
                    choices
                        .into_iter()
                        .filter(|card| *card != temper)
                        .take(count as usize - 1),
                );
                Action::Discard(chosen)
            }
            Decision::ChooseMadnessCast { card, .. } => {
                assert_eq!(card, temper);
                assert_eq!(
                    mana(state, actor, ManaColor::R),
                    1,
                    "Madness cost is affordable"
                );
                offered = true;
                Action::ChooseMadnessCast(true)
            }
            Decision::ChooseTargets { .. } => {
                panic!("a forbidden Madness spell cannot announce targets")
            }
            other => first_answer(other),
        });
        assert!(offered);
        assert_eq!(zone(&state, temper), Zone::Graveyard);
        assert_eq!(mana(&state, actor, ManaColor::R), 1);
        assert_eq!(state.players[actor.opponent().index()].life, 20);
    }
}

#[test]
fn lavinia_restricts_paid_graveyard_resolution_casts_both_seats() {
    for actor in [P0, P1] {
        let mut state = paid_lavinia(actor, &["Island"; 20]);
        let bilbo = put(
            &mut state,
            actor,
            "Bilbo, Thief in the Night",
            Zone::Battlefield,
        );
        let island = put(&mut state, actor, "Island", Zone::Battlefield);
        put(&mut state, actor, "Island", Zone::Battlefield);
        let ring = put(&mut state, actor, "Ichor Wellspring", Zone::Graveyard);
        let forbidden = put(&mut state, actor, "Brotherhood's End", Zone::Graveyard);
        let mut announced = false;
        for _ in 0..30 {
            match next(&mut state) {
                Decision::DeclareAttackers { .. } => {
                    act(&mut state, Action::DeclareAttackers(vec![bilbo]));
                    announced = true;
                    break;
                }
                Decision::CastSpellOrPass { .. } => act(&mut state, Action::Pass),
                other => panic!("unexpected precombat decision: {other:?}"),
            }
        }
        assert!(announced);
        act(&mut state, Action::ActivateManaAbility(island));
        // Nonland red mana makes the forbidden spell affordable after
        // Bilbo's reduction, without raising the permitted mana value.
        pool(&mut state, actor, &[ManaColor::R, ManaColor::R], 0);
        let prompt = settle(&mut state).expect("Bilbo graveyard choice");
        assert!(
            matches!(&prompt, Decision::ChooseEffectTargets { legal_targets, can_finish: true, .. } if legal_targets == &vec![Target::Object(ring)]),
            "{prompt:?}"
        );
        let mut restored: GameState =
            serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
        assert_eq!(prompt, next(&mut restored));
        for continued in [&mut state, &mut restored] {
            assert!(engine::step(
                continued,
                Action::ChooseEffectTarget(Target::Object(forbidden))
            )
            .is_err());
            act(continued, Action::ChooseEffectTarget(Target::Object(ring)));
            settled(continued);
            assert_eq!(zone(continued, forbidden), Zone::Graveyard);
            assert_eq!(zone(continued, ring), Zone::Battlefield);
            assert_eq!(
                continued.players[actor.index()]
                    .mana_pool
                    .iter()
                    .copied()
                    .sum::<u8>(),
                2,
                "one mana paid after Bilbo's reduction"
            );
        }
        assert_eq!(state.state_hash(), restored.state_hash());
    }
}
