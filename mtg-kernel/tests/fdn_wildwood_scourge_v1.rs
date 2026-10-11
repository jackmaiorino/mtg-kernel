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
    assert_eq!(state.stack.last().unwrap().v4.x_value, x as u8);
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
