//! Future printed Wildwood acceptance. Genuine root-owned registration required.
#![cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
use mtg_kernel::card_def::{card_id_by_name, CardCapability, Subtype, CARD_DEFS};
use mtg_kernel::effect::{EffectOp, ExecCtx, ObjectRef};
use mtg_kernel::engine::{self, Action, Decision};
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::ManaColor;
use mtg_kernel::state::{GameState, Step, Target, Zone};

fn ready(actor: PlayerId) -> GameState {
    let forest = card_id_by_name("Forest").unwrap();
    let mut state = GameState::new_from_libraries_with_starting_player_v1(
        &[forest; 40],
        &[forest; 40],
        |id| CARD_DEFS[id as usize].object_name.into(),
        110_291,
        actor,
    );
    state.step = Step::Main1;
    state
}
fn add(state: &mut GameState, owner: PlayerId, name: &str, zone: Zone) -> ObjectId {
    let def = card_id_by_name(name).expect("requires genuine FDN admission");
    let id = state.objects.push(mtg_kernel::state::GameObject {
        card_def: def,
        name: CARD_DEFS[def as usize].object_name.into(),
        owner,
        controller: owner,
        zone: Zone::Library,
        tapped: false,
        summoning_sick: false,
        damage: 0,
        counters: Default::default(),
        attachments: Vec::new(),
        v4: mtg_kernel::state::ObjectStateV4::from_card_def(def),
        spell_copy_origin: None,
        plotted_turn: None,
        zone_change_count: 0,
    });
    state.players[owner.index()].library.push(id);
    event::propose_and_commit(state, ProposedEvent::zone_change(id, zone));
    id
}
fn restore(state: &GameState) -> GameState {
    serde_json::from_slice(&serde_json::to_vec(state).unwrap()).unwrap()
}
fn next(state: &mut GameState) -> Decision {
    let d = engine::advance_until_decision(state);
    assert!(!matches!(d, Decision::Halted { .. }), "{d:?}");
    d
}
fn act(state: &mut GameState, d: Decision) {
    let a = match d {
        Decision::CastSpellOrPass { .. } => Action::Pass,
        Decision::ChooseEffectBoolean { .. } => Action::ChooseEffectBoolean(true),
        Decision::OrderTriggers { pending, .. } => {
            Action::OrderTriggers((0..pending.len()).collect())
        }
        other => panic!("unexpected {other:?}"),
    };
    engine::step(state, a).unwrap();
}
fn settle(state: &mut GameState) {
    for _ in 0..80 {
        let d = next(state);
        if state.stack.is_empty()
            && state.engine.pending_triggers.is_empty()
            && matches!(d, Decision::CastSpellOrPass { .. })
        {
            return;
        }
        act(state, d);
    }
    panic!("stack did not settle");
}
fn cast_plain(state: &mut GameState, actor: PlayerId, name: &str) -> ObjectId {
    let id = add(state, actor, name, Zone::Hand);
    state.players[actor.index()].mana_pool = [10; 6];
    next(state);
    engine::step(state, Action::CastSpell(id)).unwrap();
    settle(state);
    id
}
fn announce(state: &mut GameState, actor: PlayerId, x: u16) -> ObjectId {
    let source = add(state, actor, "Wildwood Scourge", Zone::Hand);
    state.players[actor.index()].mana_pool[ManaColor::G.pool_index()] = 1;
    state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] =
        u8::try_from(x + 1).unwrap();
    next(state);
    engine::step(state, Action::CastSpell(source)).unwrap();
    assert!(matches!(next(state),Decision::ChooseEffectOption{option_count,..} if option_count>x));
    engine::step(state, Action::ChooseEffectOption(x)).unwrap();
    next(state); // Actually finalizes payment, captured X and stack root.
    assert_eq!(state.stack.last().unwrap().v4.x_value, x);
    source
}
fn scourge(state: &mut GameState, actor: PlayerId) -> ObjectId {
    let source = announce(state, actor, 2);
    settle(state);
    source
}
fn counters(state: &mut GameState, placing: PlayerId, recipient: ObjectId, count: u8) {
    // Existing typed counter producer, not an invented printed ability.
    let mut ctx = ExecCtx::no_targets(recipient, placing);
    ctx.targets = vec![Target::Object(recipient)];
    ctx.target_contracts = vec![mtg_kernel::state::StackTargetContractV4::capture(
        state,
        Target::Object(recipient),
    )];
    mtg_kernel::effect::execute(
        &EffectOp::AddPlusOnePlusOneCounters {
            object: ObjectRef::Target(0),
            count,
        },
        &ctx,
        state,
    );
}
fn queued(state: &GameState, source: ObjectId) -> usize {
    state
        .engine
        .pending_triggers
        .iter()
        .filter(|p| p.source == source)
        .count()
        + state.stack.iter().filter(|p| p.source == source).count()
        + state
            .wildwood_waiting_v1
            .as_deref()
            .unwrap_or_default()
            .iter()
            .filter(|p| p.source == source)
            .count()
}

#[test]
fn printed_x_entry_zero_death_and_restore_both_seats() {
    let def =
        &CARD_DEFS[card_id_by_name("Wildwood Scourge").expect("future registration") as usize];
    assert_eq!(def.capability, CardCapability::Full);
    assert_eq!((def.power, def.toughness), (Some(0), Some(0)));
    assert_eq!(def.subtypes, &[Subtype::Hydra]);
    assert_eq!(def.cost.x_count, 1);
    for actor in [PlayerId::P0, PlayerId::P1] {
        for x in [0, 1, 3] {
            let mut state = ready(actor);
            let source = announce(&mut state, actor, x);
            let mut replay = restore(&state);
            settle(&mut state);
            settle(&mut replay);
            assert_eq!(
                state.objects.get(source).zone,
                if x == 0 {
                    Zone::Graveyard
                } else {
                    Zone::Battlefield
                }
            );
            assert_eq!(state.objects.get(source).counters.plus1_plus1, i32::from(x));
            assert_eq!(
                serde_json::to_vec(&state).unwrap(),
                serde_json::to_vec(&replay).unwrap()
            );
        }
    }
}
#[test]
fn actual_felidar_savior_counter_placement_triggers_only_for_other_nonhydra_recipient() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let source = scourge(&mut state, actor);
        let elf = add(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
        let savior = add(&mut state, actor, "Felidar Savior", Zone::Hand);
        state.players[actor.index()].mana_pool = [10; 6];
        next(&mut state);
        engine::step(&mut state, Action::CastSpell(savior)).unwrap();
        for _ in 0..40 {
            match next(&mut state) {
                Decision::ChooseTargets {
                    remaining,
                    legal_targets,
                    ..
                } => {
                    let target = if remaining == 2 { source } else { elf };
                    assert!(legal_targets.contains(&Target::Object(target)));
                    engine::step(&mut state, Action::ChooseTarget(Target::Object(target))).unwrap();
                    if remaining == 1 {
                        break;
                    }
                }
                d => act(&mut state, d),
            }
        }
        next(&mut state); // Complete genuine trigger targeting.
        let mut replay = restore(&state);
        settle(&mut state);
        settle(&mut replay);
        assert_eq!(state.objects.get(source).counters.plus1_plus1, 4);
        assert_eq!(state.objects.get(elf).counters.plus1_plus1, 1);
        assert_eq!(
            serde_json::to_vec(&state).unwrap(),
            serde_json::to_vec(&replay).unwrap()
        );
    }
}
#[test]
fn grouped_counter_amount_is_one_trigger_and_placing_player_is_independent() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for placing in [actor, actor.opponent()] {
            for count in [1, 2, 7] {
                let mut state = ready(actor);
                let source = scourge(&mut state, actor);
                let elf = add(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
                counters(&mut state, placing, elf, count);
                assert_eq!(queued(&state, source), 1);
                let mut replay = restore(&state);
                settle(&mut state);
                settle(&mut replay);
                assert_eq!(state.objects.get(source).counters.plus1_plus1, 3);
                assert_eq!(
                    serde_json::to_vec(&state).unwrap(),
                    serde_json::to_vec(&replay).unwrap()
                );
            }
        }
    }
}
#[test]
fn self_hydra_opposing_noncreature_and_zero_are_excluded() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for mode in 0..5 {
            let mut state = ready(actor);
            let source = scourge(&mut state, actor);
            let other = match mode {
                0 => source,
                1 => add(&mut state, actor, "Mossborn Hydra", Zone::Battlefield),
                2 => add(
                    &mut state,
                    actor.opponent(),
                    "Llanowar Elves",
                    Zone::Battlefield,
                ),
                3 => add(&mut state, actor, "Forest", Zone::Battlefield),
                _ => add(&mut state, actor, "Llanowar Elves", Zone::Battlefield),
            };
            counters(&mut state, actor, other, if mode == 4 { 0 } else { 1 });
            assert_eq!(queued(&state, source), 0);
            settle(&mut state);
            assert_eq!(
                state.objects.get(source).counters.plus1_plus1,
                if mode == 0 { 3 } else { 2 }
            );
        }
    }
}
#[test]
fn recipient_departure_and_control_change_after_event_cannot_erase_or_reinterpret_capture() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for depart in [false, true] {
            let mut state = ready(actor);
            let source = scourge(&mut state, actor);
            let elf = add(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
            counters(&mut state, actor, elf, 2);
            assert_eq!(queued(&state, source), 1);
            if depart {
                event::propose_and_commit(
                    &mut state,
                    ProposedEvent::zone_change(elf, Zone::Graveyard),
                );
            } else {
                state.objects.get_mut(elf).controller = actor.opponent();
            }
            let mut replay = restore(&state);
            settle(&mut state);
            settle(&mut replay);
            assert_eq!(state.objects.get(source).counters.plus1_plus1, 3);
            assert_eq!(
                serde_json::to_vec(&state).unwrap(),
                serde_json::to_vec(&replay).unwrap()
            );
        }
    }
}
#[test]
fn captured_source_control_or_blink_does_not_redirect_bound_counter() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for blink in [false, true] {
            let mut state = ready(actor);
            let source = scourge(&mut state, actor);
            let elf = add(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
            counters(&mut state, actor, elf, 1);
            if blink {
                event::propose_and_commit(
                    &mut state,
                    ProposedEvent::zone_change(source, Zone::Graveyard),
                );
                event::propose_and_commit(
                    &mut state,
                    ProposedEvent::zone_change(source, Zone::Battlefield),
                );
                // Existing typed producer keeps the new 0/0 incarnation alive for the old-root check.
                counters(&mut state, actor, source, 2);
            } else {
                state.objects.get_mut(source).controller = actor.opponent();
            }
            let mut replay = restore(&state);
            settle(&mut state);
            settle(&mut replay);
            assert_eq!(
                state.objects.get(source).counters.plus1_plus1,
                if blink { 2 } else { 3 }
            );
            assert_eq!(
                serde_json::to_vec(&state).unwrap(),
                serde_json::to_vec(&replay).unwrap()
            );
        }
    }
}

#[test]
fn real_witness_suppresses_later_capture_without_erasing_captured_counter() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for after in [false, true] {
            let mut state = ready(actor);
            let source = scourge(&mut state, actor);
            let elf = add(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
            cast_plain(&mut state, actor, "High Fae Trickster");
            if after {
                counters(&mut state, actor, elf, 1);
                next(&mut state);
                assert_eq!(queued(&state, source), 1);
            }
            let witness = add(&mut state, actor, "Witness Protection", Zone::Hand);
            state.players[actor.index()].mana_pool[ManaColor::U.pool_index()] = 1;
            next(&mut state);
            engine::step(&mut state, Action::CastSpell(witness)).unwrap();
            assert!(
                matches!(next(&mut state),Decision::ChooseTargets{legal_targets,..}
                if legal_targets.contains(&Target::Object(source)))
            );
            engine::step(&mut state, Action::ChooseTarget(Target::Object(source))).unwrap();
            next(&mut state);
            for _ in 0..20 {
                if state.objects.get(witness).zone == Zone::Battlefield {
                    break;
                }
                let d = next(&mut state);
                act(&mut state, d);
            }
            assert_eq!(state.objects.get(witness).zone, Zone::Battlefield);
            assert!(engine::has_effective_subtype(
                &state,
                source,
                Subtype::Citizen
            ));
            let mut replay = restore(&state);
            settle(&mut state);
            settle(&mut replay);
            assert_eq!(
                state.objects.get(source).counters.plus1_plus1,
                if after { 3 } else { 2 }
            );
            counters(&mut state, actor, elf, 1);
            assert_eq!(queued(&state, source), 0);
        }
    }
}
#[test]
fn malformed_restored_trigger_authority_refuses_before_counter() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for case in 0..3 {
            let mut state = ready(actor);
            let source = scourge(&mut state, actor);
            let elf = add(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
            counters(&mut state, actor, elf, 1);
            next(&mut state);
            assert_eq!(state.stack.len(), 1);
            match case {
                0 => {
                    state.stack[0]
                        .v4
                        .ability_source_contract
                        .as_mut()
                        .unwrap()
                        .owner = actor.opponent()
                }
                1 => {
                    state.stack[0]
                        .v4
                        .ability_source_contract
                        .as_mut()
                        .unwrap()
                        .controller = actor.opponent()
                }
                2 => {
                    let Some(EffectOp::PutPlusOnePlusOneCounterOnBoundObject { object }) =
                        state.stack[0].inline_effect.as_mut()
                    else {
                        panic!("real bound root")
                    };
                    object.expected_zone_change_count += 1;
                }
                _ => unreachable!(),
            }
            state = restore(&state);
            let mut refused = false;
            for _ in 0..20 {
                match engine::advance_until_decision(&mut state) {
                    Decision::Halted { .. } => {
                        refused = true;
                        break;
                    }
                    Decision::CastSpellOrPass { .. } => {
                        let before = serde_json::to_vec(&state).unwrap();
                        if engine::step(&mut state, Action::Pass).is_err() {
                            assert_eq!(serde_json::to_vec(&state).unwrap(), before);
                            refused = true;
                            break;
                        }
                    }
                    d => panic!("unexpected {d:?}"),
                }
            }
            assert!(refused);
            assert_eq!(state.objects.get(source).counters.plus1_plus1, 2);
        }
    }
}
#[test]
fn event_capture_restore_does_not_disclose_owners_private_hand_to_opponent() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let source = scourge(&mut state, actor);
        let elf = add(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
        add(&mut state, actor, "Giant Growth", Zone::Hand);
        counters(&mut state, actor.opponent(), elf, 2);
        next(&mut state);
        assert_eq!(queued(&state, source), 1);
        let obs = mtg_kernel::rl::observe_v1(&state, actor.opponent(), 1).unwrap();
        assert!(obs.own_hand.is_empty());
        assert!(!serde_json::to_string(&obs)
            .unwrap()
            .contains("Giant Growth"));
        let replay = restore(&state);
        assert_eq!(
            serde_json::to_vec(&obs).unwrap(),
            serde_json::to_vec(&mtg_kernel::rl::observe_v1(&replay, actor.opponent(), 1).unwrap())
                .unwrap()
        );
    }
}

#[test]
fn actual_witness_changes_recipient_hydra_classification_at_counter_event_time() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let source = scourge(&mut state, actor);
        let hydra = add(&mut state, actor, "Mossborn Hydra", Zone::Battlefield);
        counters(&mut state, actor, hydra, 1);
        assert_eq!(queued(&state, source), 0);
        let witness = add(&mut state, actor, "Witness Protection", Zone::Hand);
        state.players[actor.index()].mana_pool[ManaColor::U.pool_index()] = 1;
        next(&mut state);
        engine::step(&mut state, Action::CastSpell(witness)).unwrap();
        assert!(matches!(next(&mut state), Decision::ChooseTargets { .. }));
        engine::step(&mut state, Action::ChooseTarget(Target::Object(hydra))).unwrap();
        next(&mut state);
        settle(&mut state);
        assert!(!engine::has_effective_subtype(
            &state,
            hydra,
            Subtype::Hydra
        ));
        assert!(engine::has_effective_subtype(
            &state,
            hydra,
            Subtype::Citizen
        ));
        counters(&mut state, actor.opponent(), hydra, 2);
        assert_eq!(queued(&state, source), 1);
        let mut replay = restore(&state);
        settle(&mut state);
        settle(&mut replay);
        assert_eq!(state.objects.get(source).counters.plus1_plus1, 3);
        assert_eq!(
            serde_json::to_vec(&state).unwrap(),
            serde_json::to_vec(&replay).unwrap()
        );
    }
}

#[test]
fn future_precise_meaning_selects_real_entry_and_counter_roots_and_refuses_forged_semantics() {
    use mtg_kernel::rules_meaning_wildwood_v1 as meaning;
    let selector = meaning::live_selector_v1().unwrap();
    let id = card_id_by_name("Wildwood Scourge").expect("genuine registration required");
    let selected = meaning::meaning_v1(&selector, id).unwrap();
    assert_eq!(selected.entry.printed_cost, "{X}{G}");
    assert_eq!(
        (
            selected.entry.represented_x_minimum,
            selected.entry.represented_x_maximum
        ),
        (0, 255)
    );
    assert_eq!(
        selected.counter_trigger.excluded_effective_subtype,
        Subtype::Hydra
    );
    assert_eq!(
        selected
            .counter_trigger
            .triggers_per_qualifying_recipient_event,
        1
    );
    assert!(
        selected
            .counter_trigger
            .counter_placing_player_is_irrelevant
    );
    assert!(
        !selected
            .counter_trigger
            .recipient_condition_rechecked_at_resolution
    );
    assert!(!selected.historical_event_capture_transport_supplied);
    assert!(selected.complete_apnap_capture_merge_supplied);
    for actor in [PlayerId::P0, PlayerId::P1] {
        for x in [0, 2, 5] {
            let mut state = ready(actor);
            let source = announce(&mut state, actor, x);
            let item = state.stack.last().unwrap();
            assert_eq!(item.source, source);
            assert_eq!(item.v4.x_value, x);
            assert_eq!(item.v4.source_contract.unwrap().card_def, selected.card_id);
            let before = serde_json::to_vec(&state).unwrap();
            meaning::validate_meaning_v1(&selected).unwrap();
            assert_eq!(serde_json::to_vec(&state).unwrap(), before);
            let restored = restore(&state);
            assert_eq!(
                meaning::meaning_v1(&selector, restored.objects.get(source).card_def).unwrap(),
                selected
            );
            let mut replay = restored;
            settle(&mut state);
            settle(&mut replay);
            assert_eq!(
                serde_json::to_vec(&state).unwrap(),
                serde_json::to_vec(&replay).unwrap()
            );
            assert_eq!(
                state.objects.get(source).zone,
                if x == 0 {
                    Zone::Graveyard
                } else {
                    Zone::Battlefield
                }
            );
            if x != 0 {
                assert_eq!(state.objects.get(source).counters.plus1_plus1, i32::from(x));
            }
        }
        let state = ready(actor);
        let before = serde_json::to_vec(&state).unwrap();
        for field in 0..14 {
            let mut forged = selected.clone();
            match field {
                0 => forged.entry.counter_count_equals_captured_x = false,
                1 => forged.entry.represented_x_maximum = u16::MAX,
                2 => forged.entry.entry_is_a_replacement_not_a_trigger = false,
                3 => forged.counter_trigger.excluded_effective_subtype = Subtype::Elf,
                4 => forged.counter_trigger.counter_placing_player_is_irrelevant = false,
                5 => {
                    forged
                        .counter_trigger
                        .recipient_condition_rechecked_at_resolution = true
                }
                6 => {
                    forged
                        .counter_trigger
                        .capture_original_source_controller_and_incarnation = false
                }
                7 => forged.counter_trigger.source_reentry_receives_old_counter = true,
                8 => forged.counter_trigger.placement.clear(),
                9 => forged.historical_event_capture_transport_supplied = true,
                10 => forged.unbounded_x_successor_supplied = true,
                11 => forged.frozen_vector_model_admission_supplied = true,
                12 => forged.complete_apnap_capture_merge_supplied = false,
                _ => forged.counter_trigger.trigger_root_sha256.clear(),
            }
            assert_eq!(
                meaning::validate_meaning_v1(&forged),
                Err(meaning::ErrorV1::WrongMeaning)
            );
            assert_eq!(serde_json::to_vec(&state).unwrap(), before);
        }
    }
    let value = serde_json::to_value(&selected).unwrap();
    assert_eq!(
        serde_json::from_value::<meaning::MeaningV1>(value.clone()).unwrap(),
        selected
    );
    for path in ["entry", "counter_trigger", "selector"] {
        let mut bad = value.clone();
        bad[path]
            .as_object_mut()
            .unwrap()
            .insert("unknown".into(), true.into());
        assert!(serde_json::from_value::<meaning::MeaningV1>(bad).is_err());
    }
    let mut bad = value;
    bad.as_object_mut()
        .unwrap()
        .insert("unknown".into(), true.into());
    assert!(serde_json::from_value::<meaning::MeaningV1>(bad).is_err());
}

#[test]
fn future_committed_counter_captures_keep_original_controller_and_apnap_after_restore() {
    fn transfer(state: &mut GameState, object: ObjectId, controller: PlayerId) {
        let previous = state.objects.get(object).controller;
        state.players[previous.index()]
            .battlefield
            .retain(|id| *id != object);
        state.players[controller.index()].battlefield.push(object);
        state.objects.get_mut(object).controller = controller;
    }
    use mtg_kernel::effect::{EffectObjectBinding, EffectOp};
    use mtg_kernel::rules_meaning_wildwood_v1 as meaning;
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let first = scourge(&mut state, actor);
        let second = scourge(&mut state, actor);
        // Both are actually cast printed sources. This typed control setup
        // supplies two event-time controllers without claiming a printed theft spell.
        transfer(&mut state, second, actor.opponent());
        let a = add(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
        let b = add(
            &mut state,
            actor.opponent(),
            "Llanowar Elves",
            Zone::Battlefield,
        );
        let first_generation = state.objects.get(first).zone_change_count;
        let second_generation = state.objects.get(second).zone_change_count;
        // Reverse event order must still publish AP before NAP at the actual
        // checkpoint. This acceptance case requires the documented capture merge.
        counters(&mut state, actor, b, 5);
        counters(&mut state, actor.opponent(), a, 3);
        assert_eq!(state.wildwood_waiting_v1.as_ref().unwrap().len(), 2);
        assert!(state.engine.pending_triggers.is_empty());
        for (source, controller, generation) in [
            (first, actor, first_generation),
            (second, actor.opponent(), second_generation),
        ] {
            let capture = state
                .wildwood_waiting_v1
                .as_ref()
                .unwrap()
                .iter()
                .find(|trigger| trigger.source == source)
                .unwrap();
            assert_eq!(capture.controller, controller);
            let contract = capture.source_contract.unwrap();
            assert_eq!(
                (contract.controller, contract.zone_change_count),
                (controller, generation)
            );
            assert_eq!(
                capture.effect,
                EffectOp::PutPlusOnePlusOneCounterOnBoundObject {
                    object: EffectObjectBinding {
                        object: source,
                        expected_zone: Zone::Battlefield,
                        expected_zone_change_count: generation,
                    },
                }
            );
            let selected =
                meaning::meaning_v1(&meaning::live_selector_v1().unwrap(), contract.card_def)
                    .unwrap();
            assert!(
                selected
                    .counter_trigger
                    .capture_original_source_controller_and_incarnation
            );
            assert!(
                !selected
                    .counter_trigger
                    .source_current_controller_rechecked_at_resolution
            );
        }
        transfer(&mut state, first, actor.opponent());
        transfer(&mut state, second, actor);
        let mut replay = restore(&state);
        for branch in [&mut state, &mut replay] {
            assert!(matches!(next(branch), Decision::CastSpellOrPass { .. }));
            assert!(branch.engine.pending_triggers.is_empty());
            assert_eq!(
                branch
                    .stack
                    .iter()
                    .map(|item| item.source)
                    .collect::<Vec<_>>(),
                vec![first, second]
            );
            assert_eq!(
                branch
                    .stack
                    .iter()
                    .map(|item| item.controller)
                    .collect::<Vec<_>>(),
                vec![actor, actor.opponent()]
            );
            settle(branch);
            assert_eq!(branch.objects.get(first).counters.plus1_plus1, 3);
            assert_eq!(branch.objects.get(second).counters.plus1_plus1, 3);
        }
        assert_eq!(
            serde_json::to_vec(&state).unwrap(),
            serde_json::to_vec(&replay).unwrap()
        );
    }
}

fn transfer_for_capture_test(state: &mut GameState, id: ObjectId, controller: PlayerId) {
    // Typed control-primitive setup, not a claim of a printed theft spell.
    for player in &mut state.players {
        player.battlefield.retain(|candidate| *candidate != id);
    }
    state.players[controller.index()].battlefield.push(id);
    state.objects.get_mut(id).controller = controller;
}

#[test]
fn reversed_counter_events_publish_captured_ap_before_nap_and_restore_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let ap_source = scourge(&mut state, actor);
        let nap_source = scourge(&mut state, actor);
        transfer_for_capture_test(&mut state, nap_source, actor.opponent());
        let ap_elf = add(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
        let nap_elf = add(
            &mut state,
            actor.opponent(),
            "Llanowar Elves",
            Zone::Battlefield,
        );
        counters(&mut state, actor, nap_elf, 1);
        counters(&mut state, actor.opponent(), ap_elf, 1);
        let waiting = state.wildwood_waiting_v1.as_ref().unwrap();
        assert_eq!(
            waiting.iter().map(|p| p.controller).collect::<Vec<_>>(),
            [actor.opponent(), actor]
        );
        assert!(state.engine.pending_triggers.is_empty());
        transfer_for_capture_test(&mut state, ap_source, actor.opponent());
        transfer_for_capture_test(&mut state, nap_source, actor);
        let mut replay = restore(&state);
        for branch in [&mut state, &mut replay] {
            next(branch); // Genuine checkpoint publishes the unpublished batch.
            assert!(branch.wildwood_waiting_v1.is_none());
            assert_eq!(
                branch
                    .stack
                    .iter()
                    .map(|p| (p.source, p.controller))
                    .collect::<Vec<_>>(),
                [(ap_source, actor), (nap_source, actor.opponent())]
            );
            settle(branch);
            assert_eq!(branch.objects.get(ap_source).counters.plus1_plus1, 3);
            assert_eq!(branch.objects.get(nap_source).counters.plus1_plus1, 3);
        }
        assert_eq!(
            serde_json::to_vec(&state).unwrap(),
            serde_json::to_vec(&replay).unwrap()
        );
    }
}

#[test]
fn mixed_ordinary_etb_and_counter_captures_share_apnap_and_selected_order_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let ap_source = scourge(&mut state, actor);
        let nap_source = scourge(&mut state, actor);
        transfer_for_capture_test(&mut state, nap_source, actor.opponent());
        let ap_elf = add(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
        let nap_elf = add(
            &mut state,
            actor.opponent(),
            "Llanowar Elves",
            Zone::Battlefield,
        );
        // Existing typed zone/counter producers in one unpublished event batch.
        counters(&mut state, actor, nap_elf, 1);
        let ap_etb = add(&mut state, actor, "Dwynen's Elite", Zone::Battlefield);
        counters(&mut state, actor.opponent(), ap_elf, 1);
        let nap_etb = add(
            &mut state,
            actor.opponent(),
            "Dwynen's Elite",
            Zone::Battlefield,
        );
        let mut replay = restore(&state);
        for branch in [&mut state, &mut replay] {
            for controller in [actor, actor.opponent()] {
                let Decision::OrderTriggers { player, pending } = next(branch) else {
                    panic!("mixed controller group must offer real ordering");
                };
                assert_eq!(player, controller);
                assert_eq!(pending.len(), 2);
                assert!(pending.iter().all(|p| p.controller == controller));
                engine::step(branch, Action::OrderTriggers(vec![1, 0])).unwrap();
            }
            next(branch);
            assert_eq!(
                branch
                    .stack
                    .iter()
                    .map(|p| (p.source, p.controller))
                    .collect::<Vec<_>>(),
                [
                    (ap_etb, actor),
                    (ap_source, actor),
                    (nap_etb, actor.opponent()),
                    (nap_source, actor.opponent())
                ]
            );
            settle(branch);
            assert_eq!(branch.objects.get(ap_source).counters.plus1_plus1, 3);
            assert_eq!(branch.objects.get(nap_source).counters.plus1_plus1, 3);
        }
        assert_eq!(
            serde_json::to_vec(&state).unwrap(),
            serde_json::to_vec(&replay).unwrap()
        );
    }
}

#[test]
fn unpublished_collection_does_not_reorder_or_reset_an_existing_target_stage() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let source = scourge(&mut state, actor);
        let elf = add(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
        let savior = add(&mut state, actor, "Felidar Savior", Zone::Battlefield);
        assert!(matches!(next(&mut state), Decision::ChooseTargets { .. }));
        engine::step(&mut state, Action::ChooseTarget(Target::Object(source))).unwrap();
        assert!(matches!(
            next(&mut state),
            Decision::ChooseTargets { remaining: 1, .. }
        ));
        let existing = state.engine.pending_triggers.clone();
        assert_eq!(existing[0].source, savior);
        assert!(existing[0].placement_ordered);
        assert_eq!(existing[0].targets, [Target::Object(source)]);
        // Typed checkpoint transport probe while a genuine placement stage exists.
        // This does not claim players can produce counters during target selection.
        counters(&mut state, actor, elf, 1);
        let mut replay = restore(&state);
        for branch in [&mut state, &mut replay] {
            let fresh = mtg_kernel::trigger::collect_and_process(branch);
            assert_eq!(fresh.len(), 1);
            assert_eq!(fresh[0].source, source);
            assert!(!fresh[0].placement_ordered);
            assert_eq!(branch.engine.pending_triggers, existing);
            assert!(branch.wildwood_waiting_v1.is_none());
        }
        assert_eq!(
            serde_json::to_vec(&state).unwrap(),
            serde_json::to_vec(&replay).unwrap()
        );
    }
}
