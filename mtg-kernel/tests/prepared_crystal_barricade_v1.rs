//! Future real-definition acceptance. Parent registration is required.
//! Physical zone/control setup uses existing operands, not fake printed roots.
#![cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
use mtg_kernel::card_def::{card_id_by_name, CardType, Keywords, Subtype, TargetSpec, CARD_DEFS};
use mtg_kernel::engine::{self, Action, Decision};
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::{ManaColor, Pip};
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Target, Zone};

fn put(state: &mut GameState, owner: PlayerId, name: &str, zone: Zone) -> ObjectId {
    let def = card_id_by_name(name).expect("actual parent registration is required");
    let id = state.objects.push(GameObject {
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
        Zone::Hand => state.players[owner.index()].hand.push(id),
        Zone::Battlefield => state.players[owner.index()].battlefield.push(id),
        _ => panic!("unsupported fixture placement"),
    }
    id
}
fn ready(actor: PlayerId) -> GameState {
    let forest = card_id_by_name("Forest").unwrap();
    let mut state = GameState::new_from_libraries_with_starting_player_v1(
        &[forest; 40],
        &[forest; 40],
        |_| "Forest".into(),
        1184,
        actor,
    );
    state.step = Step::Main1;
    for player in [PlayerId::P0, PlayerId::P1] {
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
    for _ in 0..4 {
        let d = next(state);
        if matches!(d, Decision::CastSpellOrPass { player, .. } if player == actor) {
            return;
        }
        assert!(matches!(d, Decision::CastSpellOrPass { .. }), "{d:?}");
        engine::step(state, Action::Pass).unwrap();
    }
    panic!("no priority");
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
    panic!("resolution did not finish");
}
fn restore(state: &GameState) -> GameState {
    let bytes = serde_json::to_vec(state).unwrap();
    let replay: GameState = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(serde_json::to_vec(&replay).unwrap(), bytes);
    replay
}
fn source(state: &mut GameState, actor: PlayerId) -> ObjectId {
    let id = put(state, actor, "Crystal Barricade", Zone::Hand);
    state.players[actor.index()].mana_pool[ManaColor::W.pool_index()] = 2;
    priority(state, actor);
    engine::step(state, Action::CastSpell(id)).unwrap();
    finish(state);
    assert_eq!(state.objects.get(id).zone, Zone::Battlefield);
    id
}
fn announce_bolt(state: &mut GameState, actor: PlayerId, target: Target) -> ObjectId {
    let bolt = put(state, actor, "Lightning Bolt", Zone::Hand);
    state.players[actor.index()].mana_pool[ManaColor::R.pool_index()] = 1;
    priority(state, actor);
    engine::step(state, Action::CastSpell(bolt)).unwrap();
    assert!(
        matches!(next(state), Decision::ChooseTargets { legal_targets, .. }
        if legal_targets.contains(&target))
    );
    engine::step(state, Action::ChooseTarget(target)).unwrap();
    let _ = next(state); // Real cast finalization before response or snapshot.
    assert!(state
        .stack
        .iter()
        .any(|s| s.source == bolt && s.targets == vec![target]));
    bolt
}
fn control_primitive(state: &mut GameState, object: ObjectId, controller: PlayerId) {
    let old = state.objects.get(object).controller;
    state.players[old.index()]
        .battlefield
        .retain(|&id| id != object);
    state.players[controller.index()].battlefield.push(object);
    state.objects.get_mut(object).controller = controller;
}
fn player_targets(state: &mut GameState, actor: PlayerId) -> Vec<Target> {
    state.priority_player = actor;
    engine::legal_targets_for(TargetSpec::AnyTarget, &[], state)
}
fn damage_primitive(state: &mut GameState, source: ObjectId, target: ObjectId, combat: bool) {
    let ProposedEvent::Damage(mut d) = ProposedEvent::damage(source, Target::Object(target), 1)
    else {
        unreachable!()
    };
    d.is_combat = combat;
    event::propose_and_commit(state, ProposedEvent::Damage(d));
}
fn witness(state: &mut GameState, actor: PlayerId, target: ObjectId) {
    // Existing registered flash enabler permits the real response timing.
    put(state, actor, "High Fae Trickster", Zone::Battlefield);
    let aura = put(state, actor, "Witness Protection", Zone::Hand);
    state.players[actor.index()].mana_pool[ManaColor::U.pool_index()] = 1;
    priority(state, actor);
    engine::step(state, Action::CastSpell(aura)).unwrap();
    assert!(
        matches!(next(state), Decision::ChooseTargets { legal_targets, .. }
        if legal_targets.contains(&Target::Object(target)))
    );
    engine::step(state, Action::ChooseTarget(Target::Object(target))).unwrap();
    let _ = next(state);
    // Resolve only the aura, preserving any older Bolt.
    for _ in 0..8 {
        if state.objects.get(aura).zone == Zone::Battlefield {
            return;
        }
        assert!(matches!(next(state), Decision::CastSpellOrPass { .. }));
        engine::step(state, Action::Pass).unwrap();
    }
    panic!("Witness response did not finish");
}

#[test]
fn actual_metadata_defender_and_player_targeting_restore_refusal() {
    let id = card_id_by_name("Crystal Barricade").expect("actual registration required");
    let def = &CARD_DEFS[usize::from(id)];
    assert!(def.is_executable());
    assert_eq!((def.power, def.toughness), (Some(0), Some(4)));
    assert_eq!(def.cost.generic, 1);
    assert_eq!(def.cost.pips, &[Pip::Colored(ManaColor::W)]);
    assert_eq!(def.types, &[CardType::Artifact, CardType::Creature]);
    assert_eq!(def.subtypes, &[Subtype::Wall]);
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let wall = source(&mut state, actor);
        assert!(engine::has_effective_keyword(
            &state,
            wall,
            Keywords::DEFENDER
        ));
        assert!(!engine::has_effective_keyword(
            &state,
            wall,
            Keywords::HEXPROOF
        ));
        let mut replay = restore(&state);
        for current in [&mut state, &mut replay] {
            assert!(player_targets(current, actor).contains(&Target::Player(actor)));
            assert!(!player_targets(current, actor.opponent()).contains(&Target::Player(actor)));
            assert!(player_targets(current, actor.opponent()).contains(&Target::Object(wall)));
            assert!(mtg_kernel::rl::observe_policy_v8(
                current,
                &mtg_kernel::policy_surface_v5::PolicySurfaceV5::new(),
                actor,
                0,
                0,
                0,
                1
            )
            .is_err());
        }
        assert_eq!(
            serde_json::to_vec(&state).unwrap(),
            serde_json::to_vec(&replay).unwrap()
        );
    }
}

#[test]
fn actual_bolt_is_targeted_normally_but_noncombat_damage_is_prevented_restore() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let wall = source(&mut state, actor);
        let creature = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
        let bolt = announce_bolt(&mut state, actor.opponent(), Target::Object(creature));
        let history = state.engine.event_history.len();
        let mut replay = restore(&state);
        for current in [&mut state, &mut replay] {
            finish(current);
            assert_eq!(current.objects.get(creature).damage, 0);
            assert_eq!(current.objects.get(creature).zone, Zone::Battlefield);
            assert!(!current.engine.event_history[history..].iter().any(|e|
                matches!(e, mtg_kernel::event::CommittedEvent::Damage { source, .. } if *source == bolt)));
            assert_eq!(current.objects.get(wall).damage, 0);
        }
        assert_eq!(
            serde_json::to_vec(&state).unwrap(),
            serde_json::to_vec(&replay).unwrap()
        );
    }
}

#[test]
fn current_controller_other_creature_combat_player_and_two_sources_are_precise() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let wall = source(&mut state, actor);
        let own = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
        let opposing = put(
            &mut state,
            actor.opponent(),
            "Llanowar Elves",
            Zone::Battlefield,
        );
        damage_primitive(&mut state, opposing, own, false);
        assert_eq!(state.objects.get(own).damage, 0);
        damage_primitive(&mut state, opposing, wall, false);
        assert_eq!(state.objects.get(wall).damage, 1);
        damage_primitive(&mut state, opposing, own, true);
        assert_eq!(state.objects.get(own).damage, 1);
        let life = state.players[actor.index()].life;
        event::propose_and_commit(
            &mut state,
            ProposedEvent::damage(opposing, Target::Player(actor), 1),
        );
        assert_eq!(state.players[actor.index()].life, life - 1);
        control_primitive(&mut state, wall, actor.opponent());
        assert!(player_targets(&mut state, actor).contains(&Target::Player(actor)));
        assert!(!player_targets(&mut state, actor).contains(&Target::Player(actor.opponent())));
        damage_primitive(&mut state, own, opposing, false);
        assert_eq!(state.objects.get(opposing).damage, 0);
        damage_primitive(&mut state, opposing, own, false);
        assert_eq!(state.objects.get(own).damage, 2);
        // Two real printed sources protect each other; neither protects itself.
        let second = put(
            &mut state,
            actor.opponent(),
            "Crystal Barricade",
            Zone::Battlefield,
        );
        damage_primitive(&mut state, own, wall, false);
        damage_primitive(&mut state, own, second, false);
        assert_eq!(state.objects.get(wall).damage, 1);
        assert_eq!(state.objects.get(second).damage, 0);
    }
}

#[test]
fn real_bolt_rechecks_player_hexproof_gained_after_announcement() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        announce_bolt(&mut state, actor.opponent(), Target::Player(actor));
        // Typed battlefield-entry setup, not a claimed flash printed cast.
        let wall = put(&mut state, actor, "Crystal Barricade", Zone::Battlefield);
        let life = state.players[actor.index()].life;
        let mut replay = restore(&state);
        for current in [&mut state, &mut replay] {
            finish(current);
            assert_eq!(current.players[actor.index()].life, life);
            assert_eq!(current.objects.get(wall).zone, Zone::Battlefield);
        }
        assert_eq!(
            serde_json::to_vec(&state).unwrap(),
            serde_json::to_vec(&replay).unwrap()
        );
    }
}

#[test]
fn departure_reentry_and_real_witness_response_change_live_prevention() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for mode in 0..3 {
            let mut state = ready(actor);
            let wall = source(&mut state, actor);
            announce_bolt(&mut state, actor.opponent(), Target::Object(wall));
            match mode {
                0 => event::propose_and_commit(
                    &mut state,
                    ProposedEvent::zone_change(wall, Zone::Graveyard),
                ),
                1 => {
                    event::propose_and_commit(
                        &mut state,
                        ProposedEvent::zone_change(wall, Zone::Graveyard),
                    );
                    event::propose_and_commit(
                        &mut state,
                        ProposedEvent::zone_change(wall, Zone::Battlefield),
                    );
                }
                _ => witness(&mut state, actor, wall),
            }
            let mut replay = restore(&state);
            let life = state.players[actor.index()].life;
            for current in [&mut state, &mut replay] {
                finish(current);
                if mode < 2 {
                    assert_eq!(current.objects.get(wall).damage, 0);
                }
                assert_eq!(current.players[actor.index()].life, life);
                let targets = player_targets(current, actor.opponent());
                assert_eq!(targets.contains(&Target::Player(actor)), mode != 1);
            }
            assert_eq!(
                serde_json::to_vec(&state).unwrap(),
                serde_json::to_vec(&replay).unwrap()
            );
        }
    }
}

#[test]
fn actual_flaring_pain_overrides_noncombat_prevention_without_consuming_static_source() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let wall = source(&mut state, actor);
        let own = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
        let pain = put(&mut state, actor, "Flaring Pain", Zone::Hand);
        state.players[actor.index()].mana_pool[ManaColor::R.pool_index()] = 2;
        priority(&mut state, actor);
        engine::step(&mut state, Action::CastSpell(pain)).unwrap();
        finish(&mut state);
        let mut replay = restore(&state);
        for current in [&mut state, &mut replay] {
            damage_primitive(current, wall, own, false);
            assert_eq!(current.objects.get(own).damage, 1);
            assert!(!player_targets(current, actor.opponent()).contains(&Target::Player(actor)));
        }
        assert_eq!(
            serde_json::to_vec(&state).unwrap(),
            serde_json::to_vec(&replay).unwrap()
        );
    }
}

#[test]
fn hidden_opposing_hand_does_not_change_public_static_target_eligibility() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        source(&mut state, actor);
        let a = player_targets(&mut state, actor.opponent());
        put(&mut state, actor, "Giant Growth", Zone::Hand);
        let b = player_targets(&mut state, actor.opponent());
        assert_eq!(a, b);
        let obs = mtg_kernel::rl::observe_v1(&state, actor.opponent(), 0).unwrap();
        assert_eq!(
            obs.own_hand.len(),
            state.players[actor.opponent().index()].hand.len()
        );
        // V1 checks privacy only; the full public successor explicitly remains absent.
        assert!(mtg_kernel::rl::observe_policy_v8(
            &state,
            &mtg_kernel::policy_surface_v5::PolicySurfaceV5::new(),
            actor.opponent(),
            0,
            0,
            0,
            1
        )
        .is_err());
    }
}

fn shield(state: &mut GameState, source: ObjectId, target: ObjectId, remaining: i32) {
    state.engine.next_replacement_id += 1;
    state
        .engine
        .active_replacements
        .push(mtg_kernel::event::ActiveReplacement {
            id: state.engine.next_replacement_id,
            source,
            kind: mtg_kernel::event::ReplacementEffectKind::PreventNextDamage {
                target: Target::Object(target),
                remaining,
            },
        });
}
fn mixed_menu(state: &mut GameState) -> Decision {
    for _ in 0..10 {
        let d = next(state);
        if matches!(d, Decision::ChooseEffectOption { .. }) {
            return d;
        }
        assert!(matches!(d, Decision::CastSpellOrPass { .. }), "{d:?}");
        engine::step(state, Action::Pass).unwrap();
    }
    panic!("no affected-controller prevention menu");
}
fn option(state: &GameState, shield: bool) -> u16 {
    use mtg_kernel::effect::crystal_prevention_v1::PreventionV1;
    use mtg_kernel::effect::{EffectOptionChoicePurpose, PendingEffectChoice};
    let Some(PendingEffectChoice::ChooseOption {
        purpose: EffectOptionChoicePurpose::CrystalPreventionV1 { choice },
        ..
    }) = state
        .engine
        .pending_effect
        .as_ref()
        .and_then(|p| p.choice.as_ref())
    else {
        panic!("real Crystal prevention stage");
    };
    choice
        .options
        .iter()
        .position(|o| {
            if shield {
                matches!(o, PreventionV1::Replacement(_))
            } else {
                matches!(o, PreventionV1::Barricade(_))
            }
        })
        .unwrap() as u16
}
#[test]
fn mixed_consumable_prevention_affected_controller_order_and_restore() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for amount in [1, 5] {
            for first_shield in [false, true] {
                let mut state = ready(actor);
                let wall = source(&mut state, actor);
                let target = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
                // A real typed synthetic shield, no printed shield producer is claimed.
                shield(&mut state, wall, target, amount);
                announce_bolt(&mut state, actor.opponent(), Target::Object(target));
                assert!(matches!(mixed_menu(&mut state),
                    Decision::ChooseEffectOption { player, .. } if player == actor));
                let bytes = serde_json::to_vec(&state).unwrap();
                let mut replay = restore(&state);
                for current in [&mut state, &mut replay] {
                    let chosen = option(current, first_shield);
                    engine::step(current, Action::ChooseEffectOption(chosen)).unwrap();
                    if first_shield && amount == 1 {
                        assert!(
                            matches!(next(current), Decision::ChooseEffectOption { player, .. } if player == actor)
                        );
                        let next_option = option(current, false);
                        engine::step(current, Action::ChooseEffectOption(next_option)).unwrap();
                    }
                    finish(current);
                    assert_eq!(current.objects.get(target).damage, 0);
                    assert_eq!(current.objects.get(target).zone, Zone::Battlefield);
                    let remaining = current
                        .engine
                        .active_replacements
                        .iter()
                        .find_map(|r| match r.kind {
                            mtg_kernel::event::ReplacementEffectKind::PreventNextDamage {
                                remaining,
                                ..
                            } => Some(remaining),
                            _ => None,
                        });
                    assert_eq!(
                        remaining,
                        if first_shield {
                            (amount > 3).then_some(amount - 3)
                        } else {
                            Some(amount)
                        }
                    );
                }
                assert_ne!(serde_json::to_vec(&state).unwrap(), bytes);
                assert_eq!(
                    serde_json::to_vec(&state).unwrap(),
                    serde_json::to_vec(&replay).unwrap()
                );
            }
        }
    }
}
#[test]
fn mixed_prevention_erased_owner_receipt_and_menu_forgeries_refuse_readonly() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let wall = source(&mut state, actor);
        let target = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
        shield(&mut state, wall, target, 1);
        announce_bolt(&mut state, actor.opponent(), Target::Object(target));
        mixed_menu(&mut state);
        for case in 0..5 {
            let mut bad = restore(&state);
            match case {
                0 => bad.engine.pending_effect = None,
                1 => bad.engine.event_history.retain(|e| {
                    !matches!(
                        e,
                        mtg_kernel::event::CommittedEvent::CrystalPreventionBoundaryV1 { .. }
                    )
                }),
                2 => {
                    bad.engine.pending_effect = None;
                    bad.engine.event_history.retain(|e| {
                        !matches!(
                            e,
                            mtg_kernel::event::CommittedEvent::CrystalPreventionBoundaryV1 { .. }
                        )
                    });
                }
                3 => bad.stack.clear(),
                _ => {
                    if let Some(mtg_kernel::effect::PendingEffectChoice::ChooseOption {
                        purpose:
                            mtg_kernel::effect::EffectOptionChoicePurpose::CrystalPreventionV1 {
                                choice,
                            },
                        ..
                    }) = bad
                        .engine
                        .pending_effect
                        .as_mut()
                        .and_then(|p| p.choice.as_mut())
                    {
                        choice.remaining_amount = 2;
                    }
                }
            }
            let before = serde_json::to_vec(&bad).unwrap();
            assert!(engine::step(&mut bad, Action::ChooseEffectOption(0)).is_err());
            assert!(engine::step(&mut bad, Action::Pass).is_err());
            assert_eq!(serde_json::to_vec(&bad).unwrap(), before, "case {case}");
            assert!(matches!(
                engine::advance_until_decision(&mut bad),
                Decision::Halted { .. }
            ));
            bad.engine.halted = None;
            assert_eq!(
                serde_json::to_vec(&bad).unwrap(),
                before,
                "case {case} advance"
            );
        }
        let mut bad = restore(&state);
        let before = serde_json::to_vec(&bad).unwrap();
        assert!(engine::step(&mut bad, Action::ChooseEffectOption(u16::MAX)).is_err());
        assert_eq!(serde_json::to_vec(&bad).unwrap(), before);
    }
}
#[test]
fn mixed_unstaged_direct_and_batch_damage_refuse_before_shield_or_damage_mutation() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let wall = source(&mut state, actor);
        let target = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
        shield(&mut state, wall, target, 1);
        for batch in [false, true] {
            let mut bad = restore(&state);
            let before = serde_json::to_vec(&bad).unwrap();
            let damage = ProposedEvent::damage(wall, Target::Object(target), 3);
            if batch {
                event::propose_and_commit_batch(&mut bad, vec![damage]);
            } else {
                event::propose_and_commit(&mut bad, damage);
            }
            assert!(bad.engine.halted.is_some());
            bad.engine.halted = None;
            assert_eq!(serde_json::to_vec(&bad).unwrap(), before);
        }
    }
}

#[test]
fn selected_shield_operation_and_unconsumed_installation_remain_mandatory_after_restore() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let wall = source(&mut state, actor);
        let target = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
        shield(&mut state, wall, target, 1);
        announce_bolt(&mut state, actor.opponent(), Target::Object(target));
        mixed_menu(&mut state);
        let mut missing_installation = restore(&state);
        missing_installation.engine.active_replacements.clear();
        let before = serde_json::to_vec(&missing_installation).unwrap();
        assert!(engine::step(&mut missing_installation, Action::Pass).is_err());
        assert_eq!(serde_json::to_vec(&missing_installation).unwrap(), before);
        let selected = option(&state, true);
        engine::step(&mut state, Action::ChooseEffectOption(selected)).unwrap();
        assert!(matches!(
            next(&mut state),
            Decision::ChooseEffectOption { .. }
        ));
        for erase in [true, false] {
            let mut bad = restore(&state);
            if erase {
                bad.engine.event_history.retain(|event| {
                    !matches!(
                        event,
                        mtg_kernel::event::CommittedEvent::CrystalShieldOperationV1 { .. }
                    )
                });
            } else {
                let operation = bad
                    .engine
                    .event_history
                    .iter_mut()
                    .find_map(|event| match event {
                        mtg_kernel::event::CommittedEvent::CrystalShieldOperationV1 {
                            operation,
                        } => Some(operation),
                        _ => None,
                    })
                    .expect("actual selected shield operation");
                operation.damage_amount = 2;
            }
            let before = serde_json::to_vec(&bad).unwrap();
            assert!(engine::step(&mut bad, Action::Pass).is_err());
            assert!(engine::step(&mut bad, Action::ChooseEffectOption(0)).is_err());
            assert_eq!(serde_json::to_vec(&bad).unwrap(), before);
            assert!(matches!(
                engine::advance_until_decision(&mut bad),
                Decision::Halted { .. }
            ));
            bad.engine.halted = None;
            assert_eq!(serde_json::to_vec(&bad).unwrap(), before);
        }
    }
}

#[test]
fn future_selected_ordering_meaning_reconstructs_all_admission_and_coverage_flags() {
    let exact = mtg_kernel::rules_meaning_crystal_ordering_v1::selected().unwrap();
    mtg_kernel::rules_meaning_crystal_ordering_v1::validate(&exact).unwrap();
    let mut json = serde_json::to_value(&exact).unwrap();
    json["full_mixed_producer_coverage"] = serde_json::json!(true);
    let forged = serde_json::from_value(json).unwrap();
    assert!(mtg_kernel::rules_meaning_crystal_ordering_v1::validate(&forged).is_err());
    for field in ["producer_sha256", "schema"] {
        let mut json = serde_json::to_value(&exact).unwrap();
        json[field] = serde_json::json!("foreign");
        let forged = serde_json::from_value(json).unwrap();
        assert!(mtg_kernel::rules_meaning_crystal_ordering_v1::validate(&forged).is_err());
    }
    assert!(!exact.full_mixed_producer_coverage);
    assert!(!exact.model_and_sampler_admitted);
}

#[test]
fn unsupported_real_draw_root_refuses_before_pop_or_card_mutation_in_mixed_environment() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let wall = source(&mut state, actor);
        let recipient = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
        shield(&mut state, wall, recipient, 1);
        let draw = put(&mut state, actor, "Think Twice", Zone::Hand);
        state.players[actor.index()].mana_pool[ManaColor::U.pool_index()] = 1;
        state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 1;
        priority(&mut state, actor);
        engine::step(&mut state, Action::CastSpell(draw)).unwrap();
        let _ = next(&mut state); // Genuine announcement and payment complete.
        assert!(state.stack.iter().any(|item| item.source == draw));
        let stack = serde_json::to_vec(&state.stack).unwrap();
        let hand = state.players[actor.index()].hand.clone();
        let library = state.players[actor.index()].library.clone();
        let shield = state.engine.active_replacements.clone();
        let mut halted = false;
        for _ in 0..8 {
            let decision = engine::advance_until_decision(&mut state);
            if matches!(decision, Decision::Halted { .. }) {
                halted = true;
                break;
            }
            assert!(
                matches!(decision, Decision::CastSpellOrPass { .. }),
                "{decision:?}"
            );
            engine::step(&mut state, Action::Pass).unwrap();
        }
        assert!(halted);
        assert_eq!(serde_json::to_vec(&state.stack).unwrap(), stack);
        assert_eq!(state.objects.get(draw).zone, Zone::Stack);
        assert_eq!(state.players[actor.index()].hand, hand);
        assert_eq!(state.players[actor.index()].library, library);
        assert_eq!(state.engine.active_replacements, shield);
        assert!(state.engine.pending_effect.is_none());
        assert!(!state.engine.event_history.iter().any(|event| matches!(
            event,
            mtg_kernel::event::CommittedEvent::CrystalPreventionBoundaryV1 { .. }
        )));
    }
}
