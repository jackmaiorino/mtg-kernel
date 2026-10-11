//! Future genuine registration is required; no fixture admission or root bypass.
#![cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
use mtg_kernel::card_def::{card_id_by_name, CardType, Subtype, TargetSpec, CARD_DEFS};
use mtg_kernel::engine::{self, Action, Decision};
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::{ManaColor, Pip};
use mtg_kernel::policy_surface_v5::PolicySurfaceV5;
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Target, Zone};

fn put(state: &mut GameState, owner: PlayerId, name: &str, zone: Zone) -> ObjectId {
    let def = card_id_by_name(name).expect("true future printed registration required");
    let object = state.objects.push(GameObject {
        card_def: def,
        name: CARD_DEFS[usize::from(def)].object_name.into(),
        owner,
        controller: owner,
        zone,
        tapped: false,
        summoning_sick: false,
        damage: 0,
        counters: Default::default(),
        attachments: vec![],
        v4: ObjectStateV4::from_card_def(def),
        spell_copy_origin: None,
        plotted_turn: None,
        zone_change_count: 0,
    });
    match zone {
        Zone::Hand => state.players[owner.index()].hand.push(object),
        Zone::Battlefield => state.players[owner.index()].battlefield.push(object),
        _ => panic!("unsupported physical fixture placement"),
    }
    object
}
fn ready(actor: PlayerId) -> GameState {
    let card = card_id_by_name("Giant Growth").unwrap();
    let mut state = GameState::new_from_libraries_with_starting_player_v1(
        &[card; 40],
        &[card; 40],
        |_| "Giant Growth".into(),
        1170,
        actor,
    );
    state.step = Step::Main1;
    for player in [PlayerId::P0, PlayerId::P1] {
        put(&mut state, player, "Forest", Zone::Battlefield);
        put(&mut state, player, "Mountain", Zone::Battlefield);
        put(&mut state, player, "Lightning Bolt", Zone::Hand);
    }
    state
}
fn next(state: &mut GameState) -> Decision {
    let d = engine::advance_until_decision(state);
    assert!(!matches!(d, Decision::Halted { .. }), "{d:?}");
    d
}
fn priority(state: &mut GameState, actor: PlayerId) {
    for _ in 0..6 {
        let d = next(state);
        if matches!(d, Decision::CastSpellOrPass { player, .. } if player == actor) {
            return;
        }
        assert!(matches!(d, Decision::CastSpellOrPass { .. }), "{d:?}");
        engine::step(state, Action::Pass).unwrap();
    }
    panic!("priority unavailable");
}
fn finish(state: &mut GameState) {
    for _ in 0..30 {
        let d = next(state);
        if state.stack.is_empty()
            && state.engine.pending_triggers.is_empty()
            && matches!(d, Decision::CastSpellOrPass { .. })
        {
            return;
        }
        assert!(matches!(d, Decision::CastSpellOrPass { .. }), "{d:?}");
        engine::step(state, Action::Pass).unwrap();
    }
    panic!("resolution did not complete");
}
fn source(state: &mut GameState, actor: PlayerId) -> ObjectId {
    let source = put(state, actor, "Mild-Mannered Librarian", Zone::Hand);
    state.players[actor.index()].mana_pool[ManaColor::G.pool_index()] = 1;
    priority(state, actor);
    engine::step(state, Action::CastSpell(source)).unwrap();
    finish(state);
    assert_eq!(state.objects.get(source).zone, Zone::Battlefield);
    assert!(state.objects.get(source).summoning_sick);
    source
}
fn announce(state: &mut GameState, actor: PlayerId, source: ObjectId) {
    state.players[actor.index()].mana_pool = [0; 6];
    state.players[actor.index()].mana_pool[ManaColor::G.pool_index()] = 4;
    priority(state, actor);
    assert!(
        matches!(next(state), Decision::CastSpellOrPass { activatable_abilities, .. }
        if activatable_abilities.contains(&(source, 0)))
    );
    engine::step(state, Action::ActivateAbility(source, 0)).unwrap();
    // Complete real pending activation/payment before inspecting or mutating stack.
    assert!(matches!(next(state), Decision::CastSpellOrPass { .. }));
    assert!(state
        .stack
        .iter()
        .any(|item| item.source == source && item.v4.activated_ability_index == Some(0)));
    assert_eq!(state.players[actor.index()].mana_pool, [0; 6]);
    assert!(state
        .objects
        .get(source)
        .v4
        .creature_upgrade
        .as_ref()
        .unwrap()
        .once_activated
        .contains(&0));
}
fn restore(state: &GameState) -> GameState {
    let bytes = serde_json::to_vec(state).unwrap();
    let value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(serde_json::to_vec(&value).unwrap(), bytes);
    value
}
fn control_primitive(state: &mut GameState, object: ObjectId, controller: PlayerId) {
    let old = state.objects.get(object).controller;
    state.players[old.index()]
        .battlefield
        .retain(|&id| id != object);
    state.players[controller.index()].battlefield.push(object);
    state.objects.get_mut(object).controller = controller;
}
fn witness(state: &mut GameState, actor: PlayerId, target: ObjectId) -> ObjectId {
    let aura = put(state, actor, "Witness Protection", Zone::Hand);
    state.players[actor.index()].mana_pool[ManaColor::U.pool_index()] = 1;
    priority(state, actor);
    engine::step(state, Action::CastSpell(aura)).unwrap();
    assert!(
        matches!(next(state), Decision::ChooseTargets { legal_targets, .. }
        if legal_targets.contains(&Target::Object(target)))
    );
    engine::step(state, Action::ChooseTarget(Target::Object(target))).unwrap();
    finish(state);
    assert_eq!(state.objects.get(aura).zone, Zone::Battlefield);
    aura
}
fn flip(state: &mut GameState, ending: PlayerId) {
    for _ in 0..45 {
        let d = next(state);
        if state.active_player != ending {
            return;
        }
        assert!(matches!(d, Decision::CastSpellOrPass { .. }), "{d:?}");
        engine::step(state, Action::Pass).unwrap();
    }
    panic!("cleanup did not flip active player");
}
fn forbid_second(state: &mut GameState, actor: PlayerId, source: ObjectId) {
    state.players[actor.index()].mana_pool[ManaColor::G.pool_index()] = 8;
    priority(state, actor);
    assert!(
        matches!(next(state), Decision::CastSpellOrPass { activatable_abilities, .. }
        if !activatable_abilities.contains(&(source, 0)))
    );
    let before = serde_json::to_vec(state).unwrap();
    assert!(engine::step(state, Action::ActivateAbility(source, 0)).is_err());
    assert_eq!(serde_json::to_vec(state).unwrap(), before);
}

#[test]
fn future_printed_metadata_exact_program_mana_quote_and_no_targets() {
    let def = &CARD_DEFS[usize::from(
        card_id_by_name("Mild-Mannered Librarian").expect("parent registration prerequisite"),
    )];
    assert!(def.is_executable());
    assert_eq!(def.cost.generic, 0);
    assert_eq!(def.cost.pips, &[Pip::Colored(ManaColor::G)]);
    assert_eq!((def.power, def.toughness), (Some(1), Some(1)));
    assert_eq!(def.types, &[CardType::Creature]);
    assert_eq!(def.subtypes, &[Subtype::Human]);
    assert_eq!(def.colors, &[ManaColor::G]);
    assert_eq!(def.activated_abilities.len(), 1);
    let ability = &def.activated_abilities[0];
    assert_eq!(ability.target_spec, TargetSpec::None);
    assert!(!ability.sorcery_speed_only);
    assert_eq!(
        (ability.effect)(),
        mtg_kernel::mild_mannered_librarian_v1::program_v1()
    );
    for actor in [PlayerId::P0, PlayerId::P1] {
        for (green, colorless) in [(0, 4), (1, 2), (1, 3)] {
            let mut state = ready(actor);
            let source = source(&mut state, actor);
            put(&mut state, actor, "Blood Token", Zone::Battlefield);
            state.objects.get_mut(source).tapped = true;
            for &id in &state.players[actor.index()].battlefield.clone() {
                if CARD_DEFS[usize::from(state.objects.get(id).card_def)]
                    .types
                    .contains(&CardType::Land)
                {
                    state.objects.get_mut(id).tapped = true;
                }
            }
            state.players[actor.index()].mana_pool = [0; 6];
            state.players[actor.index()].mana_pool[ManaColor::G.pool_index()] = green;
            state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = colorless;
            priority(&mut state, actor);
            assert!(
                matches!(next(&mut state), Decision::CastSpellOrPass { activatable_abilities, .. }
                if activatable_abilities.contains(&(source, 0)) == (green == 1 && colorless == 3))
            );
            if green == 0 || colorless == 2 {
                let before = serde_json::to_vec(&state).unwrap();
                assert!(engine::step(&mut state, Action::ActivateAbility(source, 0)).is_err());
                assert_eq!(serde_json::to_vec(&state).unwrap(), before);
            } else {
                engine::step(&mut state, Action::ActivateAbility(source, 0)).unwrap();
                assert!(matches!(next(&mut state), Decision::CastSpellOrPass { .. }));
                assert_eq!(state.players[actor.index()].mana_pool, [0; 6]);
                let mut replay = restore(&state);
                finish(&mut state);
                finish(&mut replay);
                assert_eq!(state.objects.get(source).counters.plus1_plus1, 2);
                assert_eq!(
                    engine::effective_subtype_ids(&state, source),
                    vec![Subtype::Werewolf.stable_id()]
                );
                assert_eq!(
                    serde_json::to_vec(&state).unwrap(),
                    serde_json::to_vec(&replay).unwrap()
                );
            }
        }
    }
}
#[test]
fn once_is_consumed_on_payment_before_resolution_and_survives_cleanup_and_control_change() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let source = source(&mut state, actor);
        announce(&mut state, actor, source);
        forbid_second(&mut state, actor, source);
        let mut replay = restore(&state);
        finish(&mut state);
        finish(&mut replay);
        for branch in [&mut state, &mut replay] {
            assert_eq!(branch.objects.get(source).counters.plus1_plus1, 2);
            assert_eq!(engine::effective_power(branch, source), 3);
            assert_eq!(engine::effective_toughness(branch, source), 3);
            forbid_second(branch, actor, source);
            branch.step = Step::Main2; // Fixture skips combat; end/cleanup transition is real.
            flip(branch, actor);
            control_primitive(branch, source, actor.opponent());
            forbid_second(branch, actor.opponent(), source);
        }
        assert_eq!(
            serde_json::to_vec(&state).unwrap(),
            serde_json::to_vec(&replay).unwrap()
        );
    }
}
#[test]
fn departed_reentered_and_control_changed_sources_keep_the_original_ability_draw_controller() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for mode in 0..3 {
            let mut state = ready(actor);
            let source = source(&mut state, actor);
            announce(&mut state, actor, source);
            let before_actor = state.players[actor.index()].hand.len();
            let before_other = state.players[actor.opponent().index()].hand.len();
            if mode == 2 {
                control_primitive(&mut state, source, actor.opponent());
            } else {
                event::propose_and_commit(
                    &mut state,
                    ProposedEvent::zone_change(source, Zone::Graveyard),
                );
                if mode == 1 {
                    event::propose_and_commit(
                        &mut state,
                        ProposedEvent::zone_change(source, Zone::Battlefield),
                    );
                }
            }
            let mut replay = restore(&state);
            finish(&mut state);
            finish(&mut replay);
            assert_eq!(state.players[actor.index()].hand.len(), before_actor + 1);
            assert_eq!(
                state.players[actor.opponent().index()].hand.len(),
                before_other
            );
            assert_eq!(
                state.objects.get(source).counters.plus1_plus1,
                if mode == 2 { 2 } else { 0 }
            );
            if mode == 1 {
                assert!(!engine::has_effective_subtype(
                    &state,
                    source,
                    Subtype::Werewolf
                ));
                assert!(state.objects.get(source).v4.creature_upgrade.is_none());
                announce(&mut state, actor, source); // New incarnation has its own allowance.
                finish(&mut state);
                assert_eq!(state.objects.get(source).counters.plus1_plus1, 2);
                // Replay receives the same actual new activation.
                announce(&mut replay, actor, source);
                finish(&mut replay);
            }
            assert_eq!(
                serde_json::to_vec(&state).unwrap(),
                serde_json::to_vec(&replay).unwrap()
            );
        }
    }
}
#[test]
fn actual_witness_timestamps_remove_abilities_without_erasing_counters_or_once_history() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for witness_before_resolution in [false, true] {
            let mut state = ready(actor);
            let source = source(&mut state, actor);
            if witness_before_resolution {
                put(&mut state, actor, "High Fae Trickster", Zone::Battlefield);
            }
            announce(&mut state, actor, source);
            let aura = if witness_before_resolution {
                // A real High Fae Trickster permits this aura response above the activation.
                witness(&mut state, actor, source)
            } else {
                finish(&mut state);
                witness(&mut state, actor, source)
            };
            assert_eq!(state.objects.get(source).counters.plus1_plus1, 2);
            assert_eq!(engine::effective_power(&state, source), 3);
            assert_eq!(
                engine::has_effective_subtype(&state, source, Subtype::Werewolf),
                witness_before_resolution
            );
            assert_eq!(
                engine::has_effective_subtype(&state, source, Subtype::Citizen),
                !witness_before_resolution
            );
            event::propose_and_commit(
                &mut state,
                ProposedEvent::zone_change(aura, Zone::Graveyard),
            );
            forbid_second(&mut state, actor, source);
            let mut replay = restore(&state);
            forbid_second(&mut replay, actor, source);
            assert_eq!(
                serde_json::to_vec(&state).unwrap(),
                serde_json::to_vec(&replay).unwrap()
            );
        }
    }
}
#[test]
fn restored_forged_stack_source_and_controller_halt_before_upgrade_or_draw() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for field in 0..3 {
            let mut state = ready(actor);
            let source = source(&mut state, actor);
            announce(&mut state, actor, source);
            let hand = state.players[actor.index()].hand.clone();
            let item = state
                .stack
                .iter_mut()
                .find(|item| item.source == source)
                .unwrap();
            match field {
                0 => item.controller = actor.opponent(),
                1 => item.v4.ability_source_contract.as_mut().unwrap().owner = actor.opponent(),
                _ => {
                    item.v4
                        .ability_source_contract
                        .as_mut()
                        .unwrap()
                        .zone_change_count += 1
                }
            }
            let mut restored = restore(&state);
            let mut halted = false;
            for _ in 0..8 {
                match engine::advance_until_decision(&mut restored) {
                    Decision::Halted { .. } => {
                        halted = true;
                        break;
                    }
                    Decision::CastSpellOrPass { .. } => {
                        engine::step(&mut restored, Action::Pass).unwrap()
                    }
                    other => panic!("unexpected forged root path: {other:?}"),
                }
            }
            assert!(halted);
            assert_eq!(restored.objects.get(source).counters.plus1_plus1, 0);
            assert_eq!(restored.players[actor.index()].hand, hand);
            assert!(!engine::has_effective_subtype(
                &restored,
                source,
                Subtype::Werewolf
            ));
        }
    }
}
#[test]
fn ordinary_draw_privacy_survives_actual_paid_activation_and_exact_restore() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let source = source(&mut state, actor);
        announce(&mut state, actor, source);
        let drawn = *state.players[actor.index()].library.first().unwrap();
        let mut replay = restore(&state);
        finish(&mut state);
        finish(&mut replay);
        assert!(state.players[actor.index()].hand.contains(&drawn));
        for branch in [&state, &replay] {
            let surface = PolicySurfaceV5::new();
            let own =
                mtg_kernel::rl::observe_policy_v8(branch, &surface, actor, 0, 0, 0, 1).unwrap();
            let other =
                mtg_kernel::rl::observe_policy_v8(branch, &surface, actor.opponent(), 0, 0, 0, 1)
                    .unwrap();
            assert!(own
                .own_hand
                .iter()
                .any(|card| card.card_name == "Giant Growth"));
            assert!(!other
                .own_hand
                .iter()
                .any(|card| card.card_name == "Giant Growth"));
            assert!(other.known_hand_cards[actor.index()].is_empty());
            assert!(!serde_json::to_string(&other)
                .unwrap()
                .contains("Giant Growth"));
        }
        assert_eq!(
            serde_json::to_vec(&state).unwrap(),
            serde_json::to_vec(&replay).unwrap()
        );
    }
}
#[test]
fn genuine_opponent_turn_activation_is_instant_speed_and_keeps_once_through_that_cleanup() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let source = source(&mut state, actor);
        state.objects.get_mut(source).tapped = true;
        state.step = Step::Main2;
        flip(&mut state, actor);
        assert_eq!(state.active_player, actor.opponent());
        announce(&mut state, actor, source);
        let mut replay = restore(&state);
        finish(&mut state);
        finish(&mut replay);
        for branch in [&mut state, &mut replay] {
            assert_eq!(branch.objects.get(source).counters.plus1_plus1, 2);
            assert!(engine::has_effective_subtype(
                branch,
                source,
                Subtype::Werewolf
            ));
            branch.step = Step::Main2;
            flip(branch, actor.opponent());
            forbid_second(branch, actor, source);
        }
        assert_eq!(
            serde_json::to_vec(&state).unwrap(),
            serde_json::to_vec(&replay).unwrap()
        );
    }
}

fn refuse_paid_restore(state: &GameState) {
    let mut restored = restore(state);
    let bytes = serde_json::to_vec(&restored).unwrap();
    assert!(engine::step(&mut restored, Action::Pass).is_err());
    assert_eq!(serde_json::to_vec(&restored).unwrap(), bytes);
    let mut expected = restored.clone();
    expected.engine.halted = Some((
        engine::UnsupportedMechanic::InvalidEffectContinuation,
        ObjectId(0),
    ));
    assert!(matches!(
        engine::advance_until_decision(&mut restored),
        Decision::Halted { .. }
    ));
    assert_eq!(
        serde_json::to_vec(&restored).unwrap(),
        serde_json::to_vec(&expected).unwrap()
    );
}

#[test]
fn actual_paid_once_mark_and_full_root_are_mandatory_before_or_after_resolution() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for completed in [false, true] {
            let mut state = ready(actor);
            let source = source(&mut state, actor);
            announce(&mut state, actor, source);
            if completed {
                finish(&mut state);
            }
            for erase_receipt in [false, true] {
                let mut forged = restore(&state);
                if erase_receipt {
                    forged.engine.event_history.retain(|event| {
                        !matches!(
                            event,
                            mtg_kernel::event::CommittedEvent::LibrarianActivationCommittedV1 { .. }
                        )
                    });
                } else {
                    forged
                        .objects
                        .get_mut(source)
                        .v4
                        .creature_upgrade
                        .as_mut()
                        .unwrap()
                        .once_activated
                        .clear();
                }
                refuse_paid_restore(&forged);
            }
        }
    }
}

#[test]
fn actual_completed_paid_activation_cannot_be_reintroduced_or_lose_completion() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let source = source(&mut state, actor);
        announce(&mut state, actor, source);
        let original = state
            .stack
            .iter()
            .find(|item| item.source == source)
            .unwrap()
            .clone();
        finish(&mut state);
        let mut missing = restore(&state);
        missing.engine.event_history.retain(|event| {
            !matches!(
                event,
                mtg_kernel::event::CommittedEvent::LibrarianActivationFinishedV1 { .. }
            )
        });
        refuse_paid_restore(&missing);
        let mut repeated = restore(&state);
        repeated.stack.push(original);
        refuse_paid_restore(&repeated);
    }
}

#[test]
fn actual_unresolved_paid_activation_cannot_manufacture_completion_without_an_operation() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let source = source(&mut state, actor);
        announce(&mut state, actor, source);
        let original = state
            .stack
            .iter()
            .find(|item| item.source == source)
            .unwrap()
            .clone();
        let hand = state.players[actor.index()].hand.len();
        let mut forged = restore(&state);
        forged
            .stack
            .retain(|item| item.v4.stack_item_id != original.v4.stack_item_id);
        forged.engine.event_history.push(
            mtg_kernel::event::CommittedEvent::LibrarianActivationFinishedV1 {
                stack_item_id: original.v4.stack_item_id,
                operation_history_index: 0,
            },
        );
        assert_eq!(forged.objects.get(source).counters.plus1_plus1, 0);
        assert_eq!(forged.players[actor.index()].hand.len(), hand);
        refuse_paid_restore(&forged);
        // A coherent mutable operation/completion pair still cannot replace
        // the original independently committed exact counter-removal journal.
        let mut forged = restore(&state);
        forged
            .stack
            .retain(|item| item.v4.stack_item_id != original.v4.stack_item_id);
        let operation_history_index = u64::try_from(forged.engine.event_history.len()).unwrap();
        forged.engine.event_history.push(
            mtg_kernel::event::CommittedEvent::LibrarianActivationOperationV1 {
                item: Box::new(original.clone()),
                operation: mtg_kernel::librarian_activation_v1::OperationV1::Countered {
                    removal_history_index: 0,
                },
            },
        );
        forged.engine.event_history.push(
            mtg_kernel::event::CommittedEvent::LibrarianActivationFinishedV1 {
                stack_item_id: original.v4.stack_item_id,
                operation_history_index,
            },
        );
        refuse_paid_restore(&forged);
        finish(&mut state);
        assert_eq!(state.objects.get(source).counters.plus1_plus1, 2);
        assert_eq!(state.players[actor.index()].hand.len(), hand + 1);
        for alteration in 0..3 {
            let mut forged = restore(&state);
            match alteration {
                0 => forged.engine.event_history.retain(|event| {
                    !matches!(
                        event,
                        mtg_kernel::event::CommittedEvent::LibrarianActivationOperationV1 { .. }
                    )
                }),
                1 => {
                    let completion =
                        forged
                            .engine
                            .event_history
                            .iter()
                            .find(|event| {
                                matches!(event,
                        mtg_kernel::event::CommittedEvent::LibrarianActivationFinishedV1 { .. })
                            })
                            .unwrap()
                            .clone();
                    forged.engine.event_history.push(completion);
                }
                _ => {
                    let operation =
                        forged
                            .engine
                            .event_history
                            .iter_mut()
                            .find(|event| {
                                matches!(event,
                        mtg_kernel::event::CommittedEvent::LibrarianActivationOperationV1 { .. })
                            })
                            .unwrap();
                    let mtg_kernel::event::CommittedEvent::LibrarianActivationOperationV1 {
                        operation:
                            mtg_kernel::librarian_activation_v1::OperationV1::Resolved {
                                draw_history_index,
                                ..
                            },
                        ..
                    } = operation
                    else {
                        panic!("genuine resolved operation")
                    };
                    *draw_history_index = 0;
                }
            }
            refuse_paid_restore(&forged);
        }
    }
}
