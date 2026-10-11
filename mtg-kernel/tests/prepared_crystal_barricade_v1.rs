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
        for case in 0..6 {
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
                5 => {
                    let private =
                        bad.engine
                            .event_history
                            .iter()
                            .rev()
                            .find(|event| {
                                matches!(event,
                        mtg_kernel::event::CommittedEvent::CrystalPreventionBoundaryV1 { .. })
                            })
                            .unwrap()
                            .clone();
                    bad.engine.event_log.push(private);
                }
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
fn real_draw_root_completes_and_replays_without_borrowing_crystal_owner() {
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
        let hand = state.players[actor.index()].hand.clone();
        let library = state.players[actor.index()].library.clone();
        let shield = state.engine.active_replacements.clone();
        let mut replay = restore(&state);
        for current in [&mut state, &mut replay] {
            finish(current);
            assert_eq!(current.objects.get(draw).zone, Zone::Graveyard);
            assert_eq!(current.players[actor.index()].hand.len(), hand.len() + 1);
            assert_eq!(
                current.players[actor.index()].library.len(),
                library.len() - 1
            );
            assert_eq!(current.engine.active_replacements, shield);
        }
        assert_eq!(
            serde_json::to_vec(&state).unwrap(),
            serde_json::to_vec(&replay).unwrap()
        );
        assert!(state.engine.pending_effect.is_none());
        assert!(!state.engine.event_history.iter().any(|event| matches!(
            event,
            mtg_kernel::event::CommittedEvent::CrystalPreventionBoundaryV1 { .. }
        )));
    }
}

#[test]
fn mixed_environment_keeps_real_generic_and_life_ward_payment_continuations() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for (name, life_cost) in [("Tolarian Terror", 0), ("Sire of Seven Deaths", 7)] {
            for pay in [false, true] {
                let caster = actor.opponent();
                let mut state = ready(actor);
                let wall = source(&mut state, actor);
                let target = put(&mut state, actor, name, Zone::Battlefield);
                shield(&mut state, wall, target, 5);
                let bolt = announce_bolt(&mut state, caster, Target::Object(target));
                let life = state.players[caster.index()].life;
                state.players[caster.index()].mana_pool[ManaColor::C.pool_index()] = 2;
                for _ in 0..12 {
                    let d = next(&mut state);
                    if matches!(d, Decision::ChooseEffectBoolean { player, .. } if player == caster)
                    {
                        break;
                    }
                    assert!(matches!(d, Decision::CastSpellOrPass { .. }), "{d:?}");
                    engine::step(&mut state, Action::Pass).unwrap();
                }
                assert!(matches!(next(&mut state),
                    Decision::ChooseEffectBoolean { player, .. } if player == caster));
                let saved = serde_json::to_vec(&state).unwrap();
                assert!(engine::step(&mut state, Action::ChooseEffectOption(0)).is_err());
                assert_eq!(serde_json::to_vec(&state).unwrap(), saved);
                let mut replay = restore(&state);
                for current in [&mut state, &mut replay] {
                    engine::step(current, Action::ChooseEffectBoolean(pay)).unwrap();
                    if pay {
                        mixed_menu(current);
                        let selected = option(current, false);
                        engine::step(current, Action::ChooseEffectOption(selected)).unwrap();
                    }
                    finish(current);
                    assert_eq!(current.objects.get(bolt).zone, Zone::Graveyard);
                    assert_eq!(current.objects.get(target).damage, 0);
                    assert_eq!(
                        current.players[caster.index()].life,
                        life - if pay { life_cost } else { 0 }
                    );
                    assert!(matches!(
                        current.engine.active_replacements[0].kind,
                        mtg_kernel::event::ReplacementEffectKind::PreventNextDamage {
                            remaining: 5,
                            ..
                        }
                    ));
                    let crystal_rows =
                        current
                            .engine
                            .event_history
                            .iter()
                            .filter(|e| {
                                matches!(e,
                        mtg_kernel::event::CommittedEvent::CrystalPreventionBoundaryV1 { .. })
                            })
                            .count();
                    assert_eq!(crystal_rows > 0, pay);
                }
                assert_eq!(
                    serde_json::to_vec(&state).unwrap(),
                    serde_json::to_vec(&replay).unwrap()
                );
            }
        }
    }
}

#[test]
fn real_heartfire_sacrifice_uses_captured_power_with_mixed_prevention_and_restore() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for (choose_shield, reenter) in [false, true]
            .into_iter()
            .flat_map(|shield| [false, true].map(|reenter| (shield, reenter)))
        {
            let caster = actor.opponent();
            let mut state = ready(caster);
            let wall = source(&mut state, caster);
            control_primitive(&mut state, wall, actor);
            let fire = put(&mut state, caster, "Heartfire Immolator", Zone::Hand);
            state.players[caster.index()].mana_pool[ManaColor::R.pool_index()] = 1;
            state.players[caster.index()].mana_pool[ManaColor::C.pool_index()] = 1;
            priority(&mut state, caster);
            engine::step(&mut state, Action::CastSpell(fire)).unwrap();
            finish(&mut state);
            let target = put(&mut state, actor, "Shivan Dragon", Zone::Battlefield);
            shield(&mut state, wall, target, 5);
            state.objects.get_mut(fire).v4.lifelink_keyword_counters = 1;
            state.players[caster.index()].mana_pool[ManaColor::R.pool_index()] = 1;
            priority(&mut state, caster);
            engine::step(&mut state, Action::ActivateAbility(fire, 0)).unwrap();
            assert!(matches!(next(&mut state), Decision::ChooseTargets { .. }));
            engine::step(&mut state, Action::ChooseTarget(Target::Object(target))).unwrap();
            let _ = next(&mut state);
            assert_eq!(state.objects.get(fire).zone, Zone::Graveyard);
            if reenter {
                // Genuine zone-change primitive, followed by current-incarnation
                // fixture control/counters. The original capture stays untouched.
                event::propose_and_commit(
                    &mut state,
                    ProposedEvent::zone_change(fire, Zone::Battlefield),
                );
                control_primitive(&mut state, fire, actor);
                state.objects.get_mut(fire).counters.plus1_plus1 = 100;
                assert!(engine::effective_power(&state, fire) > 2);
            }
            let life_before = state.players[caster.index()].life;
            mixed_menu(&mut state);
            let current = state.engine.pending_effect.as_ref().unwrap();
            let mtg_kernel::effect::PendingEffectChoice::ChooseOption {
                purpose:
                    mtg_kernel::effect::EffectOptionChoicePurpose::CrystalPreventionV1 { choice },
                ..
            } = current.choice.as_ref().unwrap()
            else {
                panic!("actual prevention menu");
            };
            assert_eq!(choice.original_amount, 2);
            assert_eq!(
                choice.historical_source,
                Some(current.resolving_item.v4.stack_item_id)
            );
            let public = mtg_kernel::crystal_public_v1::observe_v1(
                &state,
                &mtg_kernel::policy_surface_v5::PolicySurfaceV5::new(),
                actor,
                0,
                0,
                0,
                1,
            )
            .unwrap();
            let menu = public.prevention_menu.unwrap();
            assert_eq!(menu.source.zone, Zone::Battlefield);
            assert_eq!(menu.source.zone_change_count, 0);
            let saved = menu.source_history.unwrap();
            assert_eq!(saved.signed_power, 2);
            assert_eq!(saved.captured_controller, caster.into());
            assert!(saved.lifelink);
            let mut malformed = restore(&state);
            let mtg_kernel::effect::PendingEffectChoice::ChooseOption {
                purpose:
                    mtg_kernel::effect::EffectOptionChoicePurpose::CrystalPreventionV1 { choice },
                ..
            } = malformed
                .engine
                .pending_effect
                .as_mut()
                .unwrap()
                .choice
                .as_mut()
                .unwrap()
            else {
                unreachable!()
            };
            choice.historical_source = None;
            let before = serde_json::to_vec(&malformed).unwrap();
            assert!(engine::step(&mut malformed, Action::ChooseEffectOption(0)).is_err());
            assert_eq!(serde_json::to_vec(&malformed).unwrap(), before);
            let mut replay = restore(&state);
            for current in [&mut state, &mut replay] {
                let selected = option(current, choose_shield);
                engine::step(current, Action::ChooseEffectOption(selected)).unwrap();
                finish(current);
                assert_eq!(current.objects.get(target).damage, 0);
                assert_eq!(current.players[caster.index()].life, life_before);
                assert!(matches!(current.engine.active_replacements[0].kind,
                    mtg_kernel::event::ReplacementEffectKind::PreventNextDamage { remaining, .. }
                        if remaining == if choose_shield { 3 } else { 5 }));
                assert_eq!(current.engine.event_history.iter().filter(|e| matches!(e,
                    mtg_kernel::event::CommittedEvent::SourcePowerActivationFinishedV1 { .. })).count(), 1);
            }
            assert_eq!(
                serde_json::to_vec(&state).unwrap(),
                serde_json::to_vec(&replay).unwrap()
            );
        }
    }
}

#[test]
fn real_joust_prevention_preserves_following_captured_controller_life_gain() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let wall = source(&mut state, actor);
        let target = put(&mut state, actor, "Shivan Dragon", Zone::Battlefield);
        let spell = put(&mut state, actor, "Joust Through", Zone::Hand);
        shield(&mut state, wall, target, 5);
        state.players[actor.index()].mana_pool[ManaColor::W.pool_index()] = 1;
        state.step = Step::DeclareAttackers;
        assert!(matches!(
            next(&mut state),
            Decision::DeclareAttackers { .. }
        ));
        engine::step(&mut state, Action::DeclareAttackers(vec![target])).unwrap();
        assert!(state.engine.combat.attackers.contains(&target));
        priority(&mut state, actor);
        engine::step(&mut state, Action::CastSpell(spell)).unwrap();
        assert!(matches!(next(&mut state), Decision::ChooseTargets { .. }));
        engine::step(&mut state, Action::ChooseTarget(Target::Object(target))).unwrap();
        let _ = next(&mut state);
        mixed_menu(&mut state);
        let life = state.players[actor.index()].life;
        let selected = option(&state, false);
        engine::step(&mut state, Action::ChooseEffectOption(selected)).unwrap();
        assert!(state
            .engine
            .pending_effect
            .as_ref()
            .unwrap()
            .choice
            .is_none());
        assert!(!state
            .engine
            .pending_effect
            .as_ref()
            .unwrap()
            .frames
            .is_empty());
        let mut replay = restore(&state);
        for current in [&mut state, &mut replay] {
            finish(current);
            assert_eq!(current.objects.get(spell).zone, Zone::Graveyard);
            assert_eq!(current.objects.get(target).damage, 0);
            assert_eq!(current.players[actor.index()].life, life + 1);
            assert_eq!(
                current
                    .engine
                    .event_history
                    .iter()
                    .filter(|e| matches!(e,
                mtg_kernel::event::CommittedEvent::LifeGain { player, amount: 1 }
                    if *player == actor))
                    .count(),
                1
            );
        }
        assert_eq!(
            serde_json::to_vec(&state).unwrap(),
            serde_json::to_vec(&replay).unwrap()
        );
        let mut forged = restore(&state);
        for event in &mut forged.engine.event_history {
            if let mtg_kernel::event::CommittedEvent::CrystalExecutionV1 { execution } = event {
                for step in &mut execution.steps {
                    if matches!(
                        step.frame,
                        mtg_kernel::effect::EffectFrame::Program {
                            op: mtg_kernel::effect::EffectOp::GainLife { .. },
                            ..
                        }
                    ) {
                        step.nondamage_history.clear();
                    }
                }
            }
        }
        let before = serde_json::to_vec(&forged).unwrap();
        assert!(engine::step(&mut forged, Action::Pass).is_err());
        assert_eq!(serde_json::to_vec(&forged).unwrap(), before);
    }
}

#[test]
fn answered_real_bolt_cannot_forge_closed_boundary_without_owned_execution() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let wall = source(&mut state, actor);
        let target = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
        shield(&mut state, wall, target, 1);
        let bolt = announce_bolt(&mut state, actor.opponent(), Target::Object(target));
        mixed_menu(&mut state);
        let selected = option(&state, false);
        engine::step(&mut state, Action::ChooseEffectOption(selected)).unwrap();
        assert!(state
            .engine
            .pending_effect
            .as_ref()
            .unwrap()
            .choice
            .is_none());
        assert_eq!(state.objects.get(bolt).zone, Zone::Stack);
        let original = state
            .engine
            .event_history
            .iter()
            .rev()
            .find_map(|event| match event {
                mtg_kernel::event::CommittedEvent::CrystalPreventionBoundaryV1 { boundary } => {
                    Some((**boundary).clone())
                }
                _ => None,
            })
            .unwrap();
        assert!(original.answer.is_some());
        assert!(original.pending.as_ref().unwrap().choice.is_none());
        let mut bad = restore(&state);
        bad.engine.pending_effect = None;
        bad.stack
            .retain(|item| item.v4.stack_item_id != original.item.v4.stack_item_id);
        let mut closed = original.clone();
        closed.previous = Some(original.history_index);
        closed.history_index = u32::try_from(bad.engine.event_history.len()).unwrap();
        closed.answer = None;
        closed.pending = None;
        bad.engine.event_history.push(
            mtg_kernel::event::CommittedEvent::CrystalPreventionBoundaryV1 {
                boundary: Box::new(closed),
            },
        );
        let before = serde_json::to_vec(&bad).unwrap();
        assert!(engine::step(&mut bad, Action::Pass).is_err());
        assert_eq!(serde_json::to_vec(&bad).unwrap(), before);
        assert!(matches!(
            engine::advance_until_decision(&mut bad),
            Decision::Halted { .. }
        ));
        bad.engine.halted = None;
        assert_eq!(serde_json::to_vec(&bad).unwrap(), before);
        assert_eq!(bad.objects.get(bolt).zone, Zone::Stack);
        let mut replay = restore(&state);
        finish(&mut state);
        finish(&mut replay);
        assert_eq!(state.objects.get(bolt).zone, Zone::Graveyard);
        assert_eq!(
            serde_json::to_vec(&state).unwrap(),
            serde_json::to_vec(&replay).unwrap()
        );
        assert!(state
            .engine
            .event_history
            .iter()
            .any(|event| matches!(event,
            mtg_kernel::event::CommittedEvent::CrystalExecutionV1 { execution }
                if execution.item.source == bolt && execution.after.is_none())));
    }
}

fn announce_batch(state: &mut GameState, actor: PlayerId, name: &str) -> ObjectId {
    let spell = put(state, actor, name, Zone::Hand);
    state.players[actor.index()].mana_pool[ManaColor::R.pool_index()] = 2;
    state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 2;
    priority(state, actor);
    engine::step(state, Action::CastSpell(spell)).unwrap();
    if name == "Slagstorm" {
        assert!(
            matches!(next(state), Decision::ChooseSpellMode { legal_modes, .. } if legal_modes.contains(&0))
        );
        engine::step(state, Action::ChooseSpellMode(0)).unwrap();
    }
    let _ = next(state);
    assert!(state.stack.iter().any(|item| item.source == spell));
    spell
}
fn batch_fixture(
    actor: PlayerId,
    name: &str,
) -> (GameState, [ObjectId; 2], [ObjectId; 2], ObjectId) {
    let mut state = ready(actor);
    let wall = source(&mut state, actor);
    let other_wall = put(
        &mut state,
        actor.opponent(),
        "Crystal Barricade",
        Zone::Battlefield,
    );
    let own = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
    let opposing = put(
        &mut state,
        actor.opponent(),
        "Llanowar Elves",
        Zone::Battlefield,
    );
    // Real prior damage makes each single wall lethal in Slagstorm's batch.
    damage_primitive(&mut state, own, wall, false);
    damage_primitive(&mut state, opposing, other_wall, false);
    shield(&mut state, wall, own, 1);
    shield(&mut state, other_wall, opposing, 1);
    let spell = announce_batch(&mut state, actor, name);
    (state, [wall, other_wall], [own, opposing], spell)
}
#[test]
fn real_all_creature_batches_choose_apnap_before_any_damage_and_restore_each_menu() {
    use mtg_kernel::effect::{EffectOptionChoicePurpose, PendingEffectChoice};
    for actor in [PlayerId::P0, PlayerId::P1] {
        for name in ["Slagstorm", "Seismic Rupture"] {
            for shield_first in [false, true] {
                let (mut state, walls, recipients, spell) = batch_fixture(actor, name);
                assert!(
                    matches!(mixed_menu(&mut state), Decision::ChooseEffectOption { player, .. } if player == actor)
                );
                let before_damage = state
                    .engine
                    .event_history
                    .iter()
                    .filter(|e| {
                        matches!(e,
                    event::CommittedEvent::Damage { source, .. } if *source == spell)
                    })
                    .count();
                assert_eq!(before_damage, 0);
                let mut replay = restore(&state);
                for current in [&mut state, &mut replay] {
                    for (ordinal, chooser) in [actor, actor.opponent()].into_iter().enumerate() {
                        assert!(
                            matches!(next(current), Decision::ChooseEffectOption { player, .. } if player == chooser)
                        );
                        let Some(PendingEffectChoice::ChooseOption {
                            purpose: EffectOptionChoicePurpose::CrystalPreventionV1 { choice },
                            ..
                        }) = current
                            .engine
                            .pending_effect
                            .as_ref()
                            .and_then(|p| p.choice.as_ref())
                        else {
                            panic!("real simultaneous menu");
                        };
                        assert_eq!(choice.target, Target::Object(recipients[ordinal]));
                        assert_eq!(choice.batch.as_ref().unwrap().cursor, ordinal);
                        let public = mtg_kernel::crystal_public_v1::observe_v1(
                            current,
                            &mtg_kernel::policy_surface_v5::PolicySurfaceV5::new(),
                            chooser,
                            0,
                            0,
                            0,
                            1,
                        )
                        .unwrap();
                        let public_menu = public.prevention_menu.unwrap();
                        assert_eq!(public_menu.chooser, chooser.into());
                        assert_eq!(
                            public_menu.simultaneous_choices_remaining,
                            Some(2 - ordinal)
                        );
                        assert_eq!(public_menu.recipient.zone, Zone::Battlefield);
                        assert!(public_menu.source_history.is_none());
                        assert_eq!(current.objects.get(walls[0]).damage, 1);
                        assert_eq!(current.objects.get(walls[1]).damage, 1);
                        let snapshot = restore(current);
                        assert_eq!(
                            snapshot.engine.pending_effect,
                            current.engine.pending_effect
                        );
                        if shield_first {
                            let chosen = option(current, true);
                            engine::step(current, Action::ChooseEffectOption(chosen)).unwrap();
                            assert!(
                                matches!(next(current), Decision::ChooseEffectOption { player, .. } if player == chooser)
                            );
                            assert_eq!(current.objects.get(walls[0]).damage, 1);
                            assert_eq!(current.objects.get(walls[1]).damage, 1);
                        }
                        let chosen = option(current, false);
                        engine::step(current, Action::ChooseEffectOption(chosen)).unwrap();
                        if ordinal == 0 {
                            assert!(!current.engine.event_history.iter().any(|e| matches!(e,
                                event::CommittedEvent::Damage { source, .. } if *source == spell)));
                        }
                    }
                    finish(current);
                    for recipient in recipients {
                        assert_eq!(current.objects.get(recipient).zone, Zone::Battlefield);
                        assert_eq!(current.objects.get(recipient).damage, 0);
                    }
                    for wall in walls {
                        if name == "Slagstorm" {
                            assert_eq!(current.objects.get(wall).zone, Zone::Graveyard);
                        } else {
                            assert_eq!(current.objects.get(wall).zone, Zone::Battlefield);
                            assert_eq!(current.objects.get(wall).damage, 3);
                        }
                    }
                    assert_eq!(current.objects.get(spell).zone, Zone::Graveyard);
                    assert_eq!(
                        current.engine.active_replacements.len(),
                        if shield_first { 0 } else { 2 }
                    );
                    assert_eq!(
                        current
                            .engine
                            .event_history
                            .iter()
                            .filter(|e| matches!(e,
                        event::CommittedEvent::Damage { source, .. } if *source == spell))
                            .count(),
                        2
                    );
                }
                assert_eq!(
                    serde_json::to_vec(&state).unwrap(),
                    serde_json::to_vec(&replay).unwrap()
                );
            }
        }
    }
}
#[test]
fn real_batch_progress_capture_and_early_commit_forgeries_refuse_readonly() {
    use mtg_kernel::effect::crystal_prevention_v1::BatchOutcomeV2;
    use mtg_kernel::effect::{EffectOptionChoicePurpose, PendingEffectChoice};
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut original, _, _, _) = batch_fixture(actor, "Slagstorm");
        mixed_menu(&mut original);
        for mode in 0..6 {
            let mut state = restore(&original);
            match mode {
                0 | 1 => {
                    let Some(PendingEffectChoice::ChooseOption {
                        purpose: EffectOptionChoicePurpose::CrystalPreventionV1 { choice },
                        ..
                    }) = state
                        .engine
                        .pending_effect
                        .as_mut()
                        .and_then(|p| p.choice.as_mut())
                    else {
                        unreachable!()
                    };
                    let progress = choice.batch.as_mut().unwrap();
                    if mode == 0 {
                        progress.cursor = 1;
                    } else {
                        progress.outcomes[0] = BatchOutcomeV2::Prevented;
                    }
                }
                2 | 3 => {
                    for event in &mut state.engine.event_history {
                        if let event::CommittedEvent::CrystalExecutionV1 { execution } = event {
                            if let Some(capture) = execution
                                .steps
                                .iter_mut()
                                .find_map(|step| step.batch_capture.as_mut())
                            {
                                if mode == 2 {
                                    capture.choice_order.reverse();
                                } else {
                                    capture.proposals.pop();
                                }
                                break;
                            }
                        }
                    }
                }
                4 => {
                    state.engine.pending_effect = None;
                }
                5 => {
                    // Keep the real captured scene and receipt, but erase the remaining owner.
                    state.stack.clear();
                    state.engine.pending_effect = None;
                }
                _ => unreachable!(),
            }
            let bytes = serde_json::to_vec(&state).unwrap();
            assert!(engine::step(&mut state, Action::Pass).is_err());
            assert_eq!(serde_json::to_vec(&state).unwrap(), bytes);
            let rejected = engine::advance_until_decision(&mut state);
            assert!(matches!(rejected, Decision::Halted { .. }));
            let mut expected: GameState = serde_json::from_slice(&bytes).unwrap();
            expected.engine.halted = state.engine.halted.clone();
            assert_eq!(
                serde_json::to_vec(&state).unwrap(),
                serde_json::to_vec(&expected).unwrap()
            );
        }
    }
}

#[test]
fn real_seismic_batch_uses_effective_flying_at_resolution_before_prevention_choices() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, walls, recipients, spell) = batch_fixture(actor, "Seismic Rupture");
        // Accepted typed temporary keyword setup, not a claimed printed flash spell.
        let timestamp = state.engine.next_effect_timestamp;
        state.engine.next_effect_timestamp += 1;
        state.engine.until_end_of_turn.push(
            engine::UntilEndOfTurnEffect::ResolvedObjectKeywordEffect {
                object_id: walls[0],
                object_zone_change_count: state.objects.get(walls[0]).zone_change_count,
                layer: engine::Layers::ABILITY_ADDING,
                timestamp,
                duration: engine::EffectDuration::EndOfTurn,
                keywords: Keywords::FLYING,
            },
        );
        mixed_menu(&mut state);
        let mut replay = restore(&state);
        for current in [&mut state, &mut replay] {
            for chooser in [actor, actor.opponent()] {
                assert!(
                    matches!(next(current), Decision::ChooseEffectOption { player, .. } if player == chooser)
                );
                let chosen = option(current, false);
                engine::step(current, Action::ChooseEffectOption(chosen)).unwrap();
            }
            finish(current);
            assert_eq!(current.objects.get(walls[0]).damage, 1);
            assert_eq!(current.objects.get(walls[1]).damage, 3);
            for recipient in recipients {
                assert_eq!(current.objects.get(recipient).damage, 0);
            }
            assert_eq!(
                current
                    .engine
                    .event_history
                    .iter()
                    .filter(|e| matches!(e,
                event::CommittedEvent::Damage { source, .. } if *source == spell))
                    .count(),
                1
            );
        }
        assert_eq!(
            serde_json::to_vec(&state).unwrap(),
            serde_json::to_vec(&replay).unwrap()
        );
    }
}

fn announce_power_spell(
    state: &mut GameState,
    actor: PlayerId,
    name: &str,
    source: ObjectId,
    target: ObjectId,
) -> ObjectId {
    let spell = put(state, actor, name, Zone::Hand);
    state.players[actor.index()].mana_pool[ManaColor::G.pool_index()] = 1;
    state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 2;
    priority(state, actor);
    engine::step(state, Action::CastSpell(spell)).unwrap();
    for target in [source, target] {
        assert!(
            matches!(next(state), Decision::ChooseTargets { legal_targets, .. }
            if legal_targets.contains(&Target::Object(target)))
        );
        engine::step(state, Action::ChooseTarget(Target::Object(target))).unwrap();
    }
    let _ = next(state);
    assert!(state.stack.iter().any(|item| item.source == spell
        && item.targets == vec![Target::Object(source), Target::Object(target)]));
    spell
}
#[test]
fn real_bite_and_felling_use_selected_creature_power_after_counters_and_keep_restore() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for name in ["Bite Down", "Felling Blow"] {
            for shield_first in [false, true] {
                let mut state = ready(actor);
                // Genuine printed static definition with physical battlefield setup.
                let wall = put(
                    &mut state,
                    actor.opponent(),
                    "Crystal Barricade",
                    Zone::Battlefield,
                );
                let recipient = put(
                    &mut state,
                    actor.opponent(),
                    "Llanowar Elves",
                    Zone::Battlefield,
                );
                let dealer = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
                shield(&mut state, wall, recipient, 1);
                let spell = announce_power_spell(&mut state, actor, name, dealer, recipient);
                assert!(
                    matches!(mixed_menu(&mut state), Decision::ChooseEffectOption { player, .. } if player == actor.opponent())
                );
                let expected_power = if name == "Felling Blow" { 2 } else { 1 };
                assert_eq!(
                    state.objects.get(dealer).counters.plus1_plus1,
                    expected_power - 1
                );
                let public = mtg_kernel::crystal_public_v1::observe_v1(
                    &state,
                    &mtg_kernel::policy_surface_v5::PolicySurfaceV5::new(),
                    actor.opponent(),
                    0,
                    0,
                    0,
                    1,
                )
                .unwrap();
                let menu = public.prevention_menu.unwrap();
                assert_eq!(menu.source.arena_id, dealer.0);
                assert_eq!(menu.creature_power_at_resolution, Some(expected_power));
                assert_eq!(menu.remaining_damage, expected_power);
                assert!(menu.source_history.is_none());
                let mut replay = restore(&state);
                for current in [&mut state, &mut replay] {
                    let chosen = option(current, shield_first);
                    engine::step(current, Action::ChooseEffectOption(chosen)).unwrap();
                    if shield_first && expected_power > 1 {
                        assert!(matches!(next(current), Decision::ChooseEffectOption { .. }));
                        let chosen = option(current, false);
                        engine::step(current, Action::ChooseEffectOption(chosen)).unwrap();
                    }
                    finish(current);
                    assert_eq!(current.objects.get(spell).zone, Zone::Graveyard);
                    assert_eq!(
                        current.objects.get(dealer).counters.plus1_plus1,
                        expected_power - 1
                    );
                    assert_eq!(current.objects.get(recipient).zone, Zone::Battlefield);
                    assert_eq!(current.objects.get(recipient).damage, 0);
                    assert_eq!(
                        current.engine.active_replacements.len(),
                        usize::from(!shield_first)
                    );
                    assert!(!current.engine.event_history.iter().any(|e| matches!(e, event::CommittedEvent::Damage { source, .. } if *source == spell)));
                }
                assert_eq!(
                    serde_json::to_vec(&state).unwrap(),
                    serde_json::to_vec(&replay).unwrap()
                );
            }
        }
    }
}
#[test]
fn real_felling_partial_targets_preserve_only_genuine_legal_source_counter() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for mode in 0..4 {
            let mut state = ready(actor);
            let wall = put(
                &mut state,
                actor.opponent(),
                "Crystal Barricade",
                Zone::Battlefield,
            );
            let recipient = put(
                &mut state,
                actor.opponent(),
                "Llanowar Elves",
                Zone::Battlefield,
            );
            let dealer = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
            shield(&mut state, wall, recipient, 1);
            let spell = announce_power_spell(&mut state, actor, "Felling Blow", dealer, recipient);
            match mode {
                0 => event::propose_and_commit(
                    &mut state,
                    ProposedEvent::zone_change(recipient, Zone::Graveyard),
                ),
                1 => {
                    event::propose_and_commit(
                        &mut state,
                        ProposedEvent::zone_change(recipient, Zone::Exile),
                    );
                    event::propose_and_commit(
                        &mut state,
                        ProposedEvent::zone_change(recipient, Zone::Battlefield),
                    );
                }
                2 => event::propose_and_commit(
                    &mut state,
                    ProposedEvent::zone_change(dealer, Zone::Graveyard),
                ),
                _ => control_primitive(&mut state, dealer, actor.opponent()),
            }
            let mut replay = restore(&state);
            for current in [&mut state, &mut replay] {
                finish(current);
                assert_eq!(current.objects.get(spell).zone, Zone::Graveyard);
                assert_eq!(
                    current.objects.get(dealer).counters.plus1_plus1,
                    i32::from(mode < 2)
                );
                assert_eq!(current.objects.get(recipient).damage, 0);
                assert_eq!(current.engine.active_replacements.len(), 1);
                assert!(current.engine.pending_effect.is_none());
            }
            assert_eq!(
                serde_json::to_vec(&state).unwrap(),
                serde_json::to_vec(&replay).unwrap()
            );
        }
    }
}
#[test]
fn chosen_power_capture_and_coherent_menu_amount_forgeries_refuse_readonly() {
    use mtg_kernel::effect::{EffectOptionChoicePurpose, PendingEffectChoice};
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut original = ready(actor);
        let wall = put(
            &mut original,
            actor.opponent(),
            "Crystal Barricade",
            Zone::Battlefield,
        );
        let recipient = put(
            &mut original,
            actor.opponent(),
            "Llanowar Elves",
            Zone::Battlefield,
        );
        let dealer = put(&mut original, actor, "Llanowar Elves", Zone::Battlefield);
        shield(&mut original, wall, recipient, 1);
        announce_power_spell(&mut original, actor, "Felling Blow", dealer, recipient);
        mixed_menu(&mut original);
        for mode in 0..4 {
            let mut state = restore(&original);
            match mode {
                0 | 1 => {
                    for event in &mut state.engine.event_history {
                        if let event::CommittedEvent::CrystalExecutionV1 { execution } = event {
                            if let Some(capture) = execution
                                .steps
                                .iter_mut()
                                .find_map(|s| s.creature_power_capture.as_mut())
                            {
                                if mode == 0 {
                                    capture.counter_history.clear();
                                } else {
                                    capture.signed_power += 1;
                                    capture.damage.as_mut().unwrap().amount += 1;
                                }
                            }
                            if mode == 1 {
                                if let Some(PendingEffectChoice::ChooseOption {
                                    purpose:
                                        EffectOptionChoicePurpose::CrystalPreventionV1 { choice },
                                    ..
                                }) = execution.after.as_mut().and_then(|p| p.choice.as_mut())
                                {
                                    choice.original_amount += 1;
                                    choice.remaining_amount += 1;
                                }
                                for step in &mut execution.steps {
                                    if let Some(PendingEffectChoice::ChooseOption {
                                        purpose:
                                            EffectOptionChoicePurpose::CrystalPreventionV1 { choice },
                                        ..
                                    }) = step.offered_menu.as_mut()
                                    {
                                        choice.original_amount += 1;
                                        choice.remaining_amount += 1;
                                    }
                                }
                            }
                        }
                        if mode == 1 {
                            if let event::CommittedEvent::CrystalPreventionBoundaryV1 { boundary } =
                                event
                            {
                                if let Some(PendingEffectChoice::ChooseOption {
                                    purpose:
                                        EffectOptionChoicePurpose::CrystalPreventionV1 { choice },
                                    ..
                                }) = boundary.pending.as_mut().and_then(|p| p.choice.as_mut())
                                {
                                    choice.original_amount += 1;
                                    choice.remaining_amount += 1;
                                }
                            }
                        }
                    }
                    if mode == 1 {
                        let Some(PendingEffectChoice::ChooseOption {
                            purpose: EffectOptionChoicePurpose::CrystalPreventionV1 { choice },
                            ..
                        }) = state
                            .engine
                            .pending_effect
                            .as_mut()
                            .and_then(|p| p.choice.as_mut())
                        else {
                            unreachable!()
                        };
                        choice.original_amount += 1;
                        choice.remaining_amount += 1;
                    }
                }
                2 => {
                    state.objects.get_mut(dealer).counters.plus1_plus1 += 1;
                }
                _ => {
                    let Some(PendingEffectChoice::ChooseOption {
                        purpose: EffectOptionChoicePurpose::CrystalPreventionV1 { choice },
                        ..
                    }) = state
                        .engine
                        .pending_effect
                        .as_mut()
                        .and_then(|p| p.choice.as_mut())
                    else {
                        unreachable!()
                    };
                    choice.creature_power = None;
                }
            }
            let before = serde_json::to_vec(&state).unwrap();
            assert!(engine::step(&mut state, Action::ChooseEffectOption(0)).is_err());
            assert!(engine::step(&mut state, Action::Pass).is_err());
            assert_eq!(serde_json::to_vec(&state).unwrap(), before);
        }
    }
}
