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
                assert_eq!(menu.resolving_source.arena_id, spell.0);
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

fn announce_incinerating(state: &mut GameState, actor: PlayerId, target: ObjectId) -> ObjectId {
    let spell = put(state, actor, "Incinerating Blast", Zone::Hand);
    state.players[actor.index()].mana_pool[ManaColor::R.pool_index()] = 1;
    state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 4;
    priority(state, actor);
    engine::step(state, Action::CastSpell(spell)).unwrap();
    assert!(
        matches!(next(state), Decision::ChooseTargets { legal_targets, .. }
        if legal_targets.contains(&Target::Object(target)))
    );
    engine::step(state, Action::ChooseTarget(Target::Object(target))).unwrap();
    let _ = next(state);
    assert!(state.stack.iter().any(|i| i.source == spell));
    spell
}
fn discard_after_prevention(state: &mut GameState, actor: PlayerId, spell: ObjectId) {
    mixed_menu(state);
    let chosen = option(state, false);
    engine::step(state, Action::ChooseEffectOption(chosen)).unwrap();
    assert!(matches!(next(state), Decision::ChooseEffectTargets {
        player, source, min_targets: 0, max_targets: 1, selected_count: 0, can_finish: true, ..
    } if player == actor && source == spell));
}
#[test]
fn real_incinerating_optional_discard_and_decline_preserve_controller_privacy_and_restore() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for pay in [false, true] {
            let mut state = ready(actor);
            let wall = source(&mut state, actor.opponent());
            let recipient = put(
                &mut state,
                actor.opponent(),
                "Llanowar Elves",
                Zone::Battlefield,
            );
            let card = put(&mut state, actor, "Forest", Zone::Hand);
            shield(&mut state, wall, recipient, 2);
            let spell = announce_incinerating(&mut state, actor, recipient);
            discard_after_prevention(&mut state, actor, spell);
            assert_eq!(state.objects.get(recipient).damage, 0);
            let surface = mtg_kernel::policy_surface_v5::PolicySurfaceV5::new();
            let public =
                mtg_kernel::crystal_public_v1::observe_v1(&state, &surface, actor, 0, 0, 0, 1)
                    .unwrap();
            let menu = public.optional_discard_menu.unwrap();
            assert_eq!(menu.chooser, actor.into());
            assert_eq!(menu.source.arena_id, spell.0);
            assert_eq!(menu.maximum_cards, 1);
            assert_eq!(menu.draws_if_discarded, 1);
            assert!(public.prevention_menu.is_none());
            let bytes = serde_json::to_vec(&state).unwrap();
            assert!(mtg_kernel::crystal_public_v1::observe_v1(
                &state,
                &surface,
                actor.opponent(),
                0,
                0,
                0,
                1
            )
            .is_err());
            assert_eq!(serde_json::to_vec(&state).unwrap(), bytes);
            let old_hand = state.players[actor.index()].hand.clone();
            let drawn = state.players[actor.index()].library[0];
            let library_len = state.players[actor.index()].library.len();
            let mut replay = restore(&state);
            for current in [&mut state, &mut replay] {
                engine::step(
                    current,
                    if pay {
                        Action::ChooseEffectTarget(Target::Object(card))
                    } else {
                        Action::FinishEffectSelection
                    },
                )
                .unwrap();
                finish(current);
                assert_eq!(current.objects.get(spell).zone, Zone::Graveyard);
                assert_eq!(
                    current.players[actor.index()].library.len(),
                    library_len - usize::from(pay)
                );
                if pay {
                    assert_eq!(current.objects.get(card).zone, Zone::Graveyard);
                    assert!(current.players[actor.index()].hand.contains(&drawn));
                } else {
                    assert_eq!(current.players[actor.index()].hand, old_hand);
                }
                assert_eq!(current.objects.get(recipient).damage, 0);
            }
            assert_eq!(
                serde_json::to_vec(&state).unwrap(),
                serde_json::to_vec(&replay).unwrap()
            );
        }
    }
}
#[test]
fn real_incinerating_empty_hand_skips_optional_cost_and_empty_library_records_actual_draw() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for empty_library in [false, true] {
            let mut state = ready(actor);
            let wall = source(&mut state, actor.opponent());
            let recipient = put(
                &mut state,
                actor.opponent(),
                "Llanowar Elves",
                Zone::Battlefield,
            );
            // Physical departure uses the actual event rather than deleting a hand vector.
            let old = state.players[actor.index()].hand.clone();
            for id in old {
                event::propose_and_commit(
                    &mut state,
                    ProposedEvent::zone_change(id, Zone::Graveyard),
                );
            }
            let card = empty_library.then(|| put(&mut state, actor, "Forest", Zone::Hand));
            if empty_library {
                let cards = state.players[actor.index()].library.clone();
                for id in cards {
                    event::propose_and_commit(
                        &mut state,
                        ProposedEvent::zone_change(id, Zone::Graveyard),
                    );
                }
                assert!(state.players[actor.index()].library.is_empty());
            }
            shield(&mut state, wall, recipient, 1);
            let spell = announce_incinerating(&mut state, actor, recipient);
            mixed_menu(&mut state);
            let chosen = option(&state, false);
            engine::step(&mut state, Action::ChooseEffectOption(chosen)).unwrap();
            if let Some(card) = card {
                assert!(matches!(
                    next(&mut state),
                    Decision::ChooseEffectTargets { max_targets: 1, .. }
                ));
                let mut replay = restore(&state);
                for current in [&mut state, &mut replay] {
                    engine::step(current, Action::ChooseEffectTarget(Target::Object(card)))
                        .unwrap();
                    assert!(current.engine.event_history.iter().any(|e|
                        matches!(e,mtg_kernel::event::CommittedEvent::Draw {player,object:None} if *player==actor)));
                    assert!(matches!(
                        engine::advance_until_decision(current),
                        Decision::GameOver { .. }
                    ));
                    // Preserve the existing engine terminal convention: loss is reported
                    // before the pending resolved spell's final departure is drained.
                    assert_eq!(current.objects.get(spell).zone, Zone::Stack);
                    assert!(current.engine.pending_effect.as_ref().is_some_and(|p| p
                        .resolving_item
                        .source
                        == spell
                        && p.choice.is_none()));
                }
                assert_eq!(
                    serde_json::to_vec(&state).unwrap(),
                    serde_json::to_vec(&replay).unwrap()
                );
            } else {
                finish(&mut state);
                assert_eq!(state.objects.get(spell).zone, Zone::Graveyard);
                assert!(!state.engine.event_history.iter().any(|e|
                    matches!(e,mtg_kernel::event::CommittedEvent::Draw {player,..} if *player==actor)));
            }
        }
    }
}
#[test]
fn real_incinerating_madness_discard_uses_actual_exile_draw_and_later_trigger() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let wall = source(&mut state, actor.opponent());
        let recipient = put(
            &mut state,
            actor.opponent(),
            "Llanowar Elves",
            Zone::Battlefield,
        );
        let temper = put(&mut state, actor, "Fiery Temper", Zone::Hand);
        let drawn = state.players[actor.index()].library[0];
        shield(&mut state, wall, recipient, 1);
        let spell = announce_incinerating(&mut state, actor, recipient);
        discard_after_prevention(&mut state, actor, spell);
        let mut replay = restore(&state);
        for current in [&mut state, &mut replay] {
            engine::step(current, Action::ChooseEffectTarget(Target::Object(temper))).unwrap();
            assert_eq!(current.objects.get(temper).zone, Zone::Exile);
            assert!(current.players[actor.index()].hand.contains(&drawn));
            for _ in 0..30 {
                let d = next(current);
                if matches!(d,Decision::ChooseMadnessCast {player,card} if player==actor && card==temper)
                {
                    engine::step(current, Action::ChooseMadnessCast(false)).unwrap();
                    break;
                }
                assert!(matches!(d, Decision::CastSpellOrPass { .. }), "{d:?}");
                engine::step(current, Action::Pass).unwrap();
            }
            assert_eq!(current.objects.get(temper).zone, Zone::Graveyard);
            finish(current);
            assert_eq!(current.objects.get(spell).zone, Zone::Graveyard);
        }
        assert_eq!(
            serde_json::to_vec(&state).unwrap(),
            serde_json::to_vec(&replay).unwrap()
        );
    }
}
#[test]
fn real_incinerating_private_capture_and_independent_discard_operation_forgeries_refuse_readonly() {
    use mtg_kernel::effect::{EffectTargetSelectionPurpose, PendingEffectChoice};
    use mtg_kernel::event::CommittedEvent;
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let wall = source(&mut state, actor.opponent());
        let recipient = put(
            &mut state,
            actor.opponent(),
            "Llanowar Elves",
            Zone::Battlefield,
        );
        let card = put(&mut state, actor, "Forest", Zone::Hand);
        let foreign = put(&mut state, actor.opponent(), "Forest", Zone::Hand);
        shield(&mut state, wall, recipient, 1);
        let spell = announce_incinerating(&mut state, actor, recipient);
        discard_after_prevention(&mut state, actor, spell);
        let bytes = serde_json::to_vec(&state).unwrap();
        assert!(engine::step(
            &mut state,
            Action::ChooseEffectTarget(Target::Object(foreign))
        )
        .is_err());
        assert_eq!(serde_json::to_vec(&state).unwrap(), bytes);
        for case in 0..4 {
            let mut bad = restore(&state);
            match case {
                0 => {
                    bad.players[actor.index()].hand.swap(0, 1);
                }
                1 => {
                    bad.objects.get_mut(card).zone_change_count += 1;
                }
                2 => {
                    let PendingEffectChoice::SelectTargets { purpose, .. } = bad
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
                    let EffectTargetSelectionPurpose::CrystalDiscardAfterDamageV2 { capture } =
                        purpose
                    else {
                        unreachable!()
                    };
                    *capture += 1;
                }
                _ => {
                    bad.engine.pending_effect.as_mut().unwrap().frames.clear();
                    bad.engine.pending_effect.as_mut().unwrap().choice = None;
                }
            }
            let bytes = serde_json::to_vec(&bad).unwrap();
            assert!(
                engine::step(&mut bad, Action::FinishEffectSelection).is_err(),
                "case {case}"
            );
            assert!(engine::step(&mut bad, Action::Pass).is_err(), "case {case}");
            assert_eq!(serde_json::to_vec(&bad).unwrap(), bytes, "case {case}");
        }
        engine::step(&mut state, Action::ChooseEffectTarget(Target::Object(card))).unwrap();
        let mut fake_closed = restore(&state);
        let mut boundary = fake_closed
            .engine
            .event_history
            .iter()
            .rev()
            .find_map(|e| match e {
                CommittedEvent::CrystalPreventionBoundaryV1 { boundary } => Some(boundary.clone()),
                _ => None,
            })
            .unwrap();
        let id = boundary.item.v4.stack_item_id;
        boundary.previous = Some(boundary.history_index);
        boundary.history_index = u32::try_from(fake_closed.engine.event_history.len()).unwrap();
        boundary.pending = None;
        boundary.answer = None;
        fake_closed.stack.retain(|item| item.v4.stack_item_id != id);
        fake_closed.engine.pending_effect = None;
        fake_closed
            .engine
            .event_history
            .push(CommittedEvent::CrystalPreventionBoundaryV1 { boundary });
        let bytes = serde_json::to_vec(&fake_closed).unwrap();
        assert!(engine::step(&mut fake_closed, Action::Pass).is_err());
        assert_eq!(serde_json::to_vec(&fake_closed).unwrap(), bytes);
        assert!(matches!(
            engine::advance_until_decision(&mut fake_closed),
            Decision::Halted { .. }
        ));
        fake_closed.engine.halted = None;
        assert_eq!(serde_json::to_vec(&fake_closed).unwrap(), bytes);
        for case in 0..3 {
            let mut bad = restore(&state);
            let operation = bad
                .engine
                .event_history
                .iter_mut()
                .rev()
                .find_map(|e| match e {
                    CommittedEvent::CrystalExecutionV1 { execution }
                        if execution.discard_commit.is_some() =>
                    {
                        execution.discard_commit.as_mut()
                    }
                    _ => None,
                })
                .unwrap();
            match case {
                0 => operation.discard_history = None,
                1 => operation.draw_history = None,
                _ => {
                    operation.selected = None;
                    operation.discard_history = None;
                    operation.draw_history = None;
                }
            }
            let bytes = serde_json::to_vec(&bad).unwrap();
            assert!(engine::step(&mut bad, Action::Pass).is_err());
            assert_eq!(serde_json::to_vec(&bad).unwrap(), bytes);
            assert!(matches!(
                engine::advance_until_decision(&mut bad),
                Decision::Halted { .. }
            ));
            bad.engine.halted = None;
            assert_eq!(serde_json::to_vec(&bad).unwrap(), bytes);
        }
    }
}

#[test]
fn real_fixed_and_chosen_power_damage_require_independent_positive_or_prevented_outcome() {
    use mtg_kernel::event::CommittedEvent;
    for actor in [PlayerId::P0, PlayerId::P1] {
        for power_root in [false, true] {
            for entirely_prevented in [false, true] {
                let mut state = ready(actor);
                let wall = source(
                    &mut state,
                    if entirely_prevented {
                        actor.opponent()
                    } else {
                        actor
                    },
                );
                let dealer = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
                let recipient = put(
                    &mut state,
                    actor.opponent(),
                    "Llanowar Elves",
                    Zone::Battlefield,
                );
                // A distinct numeric shield makes this a real mixed environment
                // without adding a prevention choice for this recipient.
                shield(&mut state, wall, wall, 1);
                let spell = if power_root {
                    announce_power_spell(&mut state, actor, "Bite Down", dealer, recipient)
                } else {
                    announce_bolt(&mut state, actor, Target::Object(recipient))
                };
                let before_resolution = restore(&state);
                let mut replay = restore(&state);
                for current in [&mut state, &mut replay] {
                    finish(current);
                }
                assert_eq!(
                    serde_json::to_vec(&state).unwrap(),
                    serde_json::to_vec(&replay).unwrap()
                );
                let operation = state
                    .engine
                    .event_history
                    .iter()
                    .find_map(|e| match e {
                        CommittedEvent::CrystalDamageOperationV3 { operation }
                            if operation.ctx.source == spell =>
                        {
                            Some(operation.as_ref())
                        }
                        _ => None,
                    })
                    .unwrap();
                assert_eq!(operation.output.is_none(), entirely_prevented);
                assert_eq!(operation.damage_history.is_none(), entirely_prevented);
                assert_eq!(
                    operation.input.source,
                    if power_root { dealer } else { spell }
                );
                assert_eq!(state.objects.get(spell).zone, Zone::Graveyard);
                assert_eq!(
                    state.objects.get(recipient).zone,
                    if entirely_prevented {
                        Zone::Battlefield
                    } else {
                        Zone::Graveyard
                    }
                );
                mtg_kernel::effect::validate_pending_effect_choice(&state).unwrap();

                // Preserve genuine publication and all preceding cast/target journals.
                // A forged completion cannot create an actual damage/no-effect operation.
                let initial = state
                    .engine
                    .event_history
                    .iter()
                    .find_map(|e| match e {
                        CommittedEvent::CrystalPreventionBoundaryV1 { boundary }
                            if boundary.item.source == spell && boundary.previous.is_none() =>
                        {
                            Some(boundary.clone())
                        }
                        _ => None,
                    })
                    .unwrap();
                let actual_step = state
                    .engine
                    .event_history
                    .iter()
                    .find_map(|e| match e {
                        CommittedEvent::CrystalExecutionV1 { execution }
                            if execution.item.source == spell =>
                        {
                            execution
                                .steps
                                .iter()
                                .find(|s| {
                                    matches!(&s.frame,
                            mtg_kernel::effect::EffectFrame::Program {op,..}
                                if matches!(op,mtg_kernel::effect::EffectOp::DealDamage {..}
                                    |mtg_kernel::effect::EffectOp::CreatureTargetPowerDamage {..}))
                                })
                                .cloned()
                        }
                        _ => None,
                    })
                    .unwrap();
                let mut bad = before_resolution;
                bad.engine.event_history =
                    state.engine.event_history[..=initial.history_index as usize].to_vec();
                let mut step = actual_step;
                step.damage_history.clear();
                step.damage_operation = None;
                let execution = mtg_kernel::effect::crystal_prevention_v1::ExecutionV1 {
                    history_index: u32::try_from(bad.engine.event_history.len()).unwrap(),
                    previous: initial.history_index,
                    item: initial.item.clone(),
                    before: initial.pending.clone().unwrap(),
                    after: None,
                    steps: vec![step],
                    batch_commit: None,
                    discard_commit: None,
                    program_commit: None,
                };
                let mut closed = initial.clone();
                closed.previous = Some(initial.history_index);
                closed.pending = None;
                closed.answer = None;
                closed.history_index = execution.history_index + 1;
                bad.stack.retain(|item| item.source != spell);
                bad.engine.pending_effect = None;
                bad.engine
                    .event_history
                    .push(CommittedEvent::CrystalExecutionV1 {
                        execution: Box::new(execution),
                    });
                bad.engine
                    .event_history
                    .push(CommittedEvent::CrystalPreventionBoundaryV1 { boundary: closed });
                let bytes = serde_json::to_vec(&bad).unwrap();
                assert!(engine::step(&mut bad, Action::Pass).is_err());
                assert_eq!(serde_json::to_vec(&bad).unwrap(), bytes);
                assert!(matches!(
                    engine::advance_until_decision(&mut bad),
                    Decision::Halted { .. }
                ));
                bad.engine.halted = None;
                assert_eq!(serde_json::to_vec(&bad).unwrap(), bytes);
                for case in 0..3 {
                    let mut bad = restore(&state);
                    match case {
                        0 => {
                            bad.engine.event_history.retain(|e| {
                                !matches!(e, CommittedEvent::CrystalDamageOperationV3 { .. })
                            });
                        }
                        1 => {
                            for event in &mut bad.engine.event_history {
                                if let CommittedEvent::CrystalExecutionV1 { execution } = event {
                                    for step in &mut execution.steps {
                                        step.damage_operation = None;
                                    }
                                }
                            }
                        }
                        _ => {
                            let op = bad
                                .engine
                                .event_history
                                .iter_mut()
                                .find_map(|e| match e {
                                    CommittedEvent::CrystalDamageOperationV3 { operation }
                                        if operation.ctx.source == spell =>
                                    {
                                        Some(operation)
                                    }
                                    _ => None,
                                })
                                .unwrap();
                            op.output = None;
                            op.damage_history = None;
                            op.preventers.clear();
                        }
                    }
                    let bytes = serde_json::to_vec(&bad).unwrap();
                    assert!(engine::step(&mut bad, Action::Pass).is_err(), "case {case}");
                    assert_eq!(serde_json::to_vec(&bad).unwrap(), bytes);
                    assert!(matches!(
                        engine::advance_until_decision(&mut bad),
                        Decision::Halted { .. }
                    ));
                    bad.engine.halted = None;
                    assert_eq!(serde_json::to_vec(&bad).unwrap(), bytes);
                }
            }
        }
    }
}

#[test]
fn mixed_environment_preserves_actual_permanent_pump_and_both_token_modes() {
    use mtg_kernel::event::CommittedEvent;
    for actor in [PlayerId::P0, PlayerId::P1] {
        for case in 0..4 {
            let mut state = ready(actor);
            let wall = source(&mut state, actor);
            let target = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
            shield(&mut state, wall, target, 2);
            let old_shields = state.engine.active_replacements.clone();
            let old_battlefield = state.players[actor.index()].battlefield.len();
            let name = match case {
                0 => "Shivan Dragon",
                1 => "Giant Growth",
                _ => "Goblin Surprise",
            };
            let spell = put(&mut state, actor, name, Zone::Hand);
            state.players[actor.index()].mana_pool[ManaColor::R.pool_index()] = 2;
            state.players[actor.index()].mana_pool[ManaColor::G.pool_index()] = 1;
            state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 4;
            priority(&mut state, actor);
            engine::step(&mut state, Action::CastSpell(spell)).unwrap();
            if case >= 2 {
                assert!(matches!(next(&mut state), Decision::ChooseSpellMode { .. }));
                engine::step(&mut state, Action::ChooseSpellMode((case - 2) as u8)).unwrap();
            }
            if case == 1 {
                assert!(
                    matches!(next(&mut state), Decision::ChooseTargets { legal_targets, .. }
                    if legal_targets.contains(&Target::Object(target)))
                );
                engine::step(&mut state, Action::ChooseTarget(Target::Object(target))).unwrap();
            }
            let _ = next(&mut state);
            assert!(state.stack.iter().any(|item| item.source == spell));
            let mut replay = restore(&state);
            for current in [&mut state, &mut replay] {
                finish(current);
                assert_eq!(current.engine.active_replacements, old_shields);
                assert!(!current.engine.event_history.iter().any(|event| matches!(
                    event,
                    CommittedEvent::CrystalPreventionBoundaryV1 { .. }
                )));
                match case {
                    0 => {
                        assert_eq!(current.objects.get(spell).zone, Zone::Battlefield);
                        assert_eq!(
                            current.players[actor.index()].battlefield.len(),
                            old_battlefield + 1
                        );
                    }
                    1 => {
                        assert_eq!(current.objects.get(spell).zone, Zone::Graveyard);
                        assert_eq!(engine::effective_power(current, target), 4);
                    }
                    2 => {
                        assert_eq!(current.objects.get(spell).zone, Zone::Graveyard);
                        assert_eq!(engine::effective_power(current, target), 3);
                        assert_eq!(
                            current.players[actor.index()].battlefield.len(),
                            old_battlefield
                        );
                    }
                    _ => {
                        assert_eq!(current.objects.get(spell).zone, Zone::Graveyard);
                        assert_eq!(
                            current.players[actor.index()].battlefield.len(),
                            old_battlefield + 2
                        );
                    }
                }
            }
            assert_eq!(
                serde_json::to_vec(&state).unwrap(),
                serde_json::to_vec(&replay).unwrap()
            );
        }
    }
}

fn announce_fight(
    state: &mut GameState,
    actor: PlayerId,
    first: ObjectId,
    second: ObjectId,
) -> ObjectId {
    let spell = put(state, actor, "Bushwhack", Zone::Hand);
    state.players[actor.index()].mana_pool[ManaColor::G.pool_index()] = 1;
    priority(state, actor);
    engine::step(state, Action::CastSpell(spell)).unwrap();
    assert!(matches!(next(state), Decision::ChooseSpellMode { .. }));
    engine::step(state, Action::ChooseSpellMode(1)).unwrap();
    for target in [first, second] {
        assert!(
            matches!(next(state), Decision::ChooseTargets { legal_targets, .. }
            if legal_targets.contains(&Target::Object(target)))
        );
        engine::step(state, Action::ChooseTarget(Target::Object(target))).unwrap();
    }
    let _ = next(state);
    assert!(state.stack.iter().any(|item| item.source == spell
        && item.targets == vec![Target::Object(first), Target::Object(second)]));
    spell
}
fn fight_fixture(
    actor: PlayerId,
    opposing_wall: bool,
) -> (GameState, [ObjectId; 2], [ObjectId; 2], ObjectId) {
    let mut state = ready(actor);
    let wall = source(&mut state, actor);
    // A real registered definition used as a static battlefield operand.
    let other_wall = if opposing_wall {
        put(
            &mut state,
            actor.opponent(),
            "Crystal Barricade",
            Zone::Battlefield,
        )
    } else {
        wall
    };
    let first = put(&mut state, actor, "Shivan Dragon", Zone::Battlefield);
    let second = put(
        &mut state,
        actor.opponent(),
        "Shivan Dragon",
        Zone::Battlefield,
    );
    state.objects.get_mut(first).v4.lifelink_keyword_counters = 1;
    state.objects.get_mut(second).v4.lifelink_keyword_counters = 1;
    shield(&mut state, wall, first, 1);
    shield(&mut state, other_wall, second, 1);
    let spell = announce_fight(&mut state, actor, first, second);
    (state, [wall, other_wall], [first, second], spell)
}
#[test]
fn real_bushwhack_fight_uses_two_sources_and_apnap_before_simultaneous_commit() {
    use mtg_kernel::effect::{EffectOptionChoicePurpose, PendingEffectChoice};
    for actor in [PlayerId::P0, PlayerId::P1] {
        for opposing_wall in [false, true] {
            for choose_shield in [false, true] {
                let (mut state, _, fighters, spell) = fight_fixture(actor, opposing_wall);
                mixed_menu(&mut state);
                let lives = state.players.each_ref().map(|p| p.life);
                let mut replay = restore(&state);
                for current in [&mut state, &mut replay] {
                    let choosers = if opposing_wall {
                        vec![actor, actor.opponent()]
                    } else {
                        vec![actor]
                    };
                    for (ordinal, chooser) in choosers.iter().copied().enumerate() {
                        assert!(
                            matches!(next(current), Decision::ChooseEffectOption { player, .. }
                            if player == chooser)
                        );
                        let choice = match current
                            .engine
                            .pending_effect
                            .as_ref()
                            .unwrap()
                            .choice
                            .as_ref()
                            .unwrap()
                        {
                            PendingEffectChoice::ChooseOption {
                                purpose: EffectOptionChoicePurpose::CrystalPreventionV1 { choice },
                                ..
                            } => choice,
                            _ => panic!("real fight replacement menu"),
                        };
                        let recipient = fighters[ordinal];
                        let source = fighters[1 - ordinal];
                        assert_eq!(choice.source, source);
                        assert_eq!(choice.target, Target::Object(recipient));
                        assert_eq!(choice.original_amount, 5);
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
                        let menu = public.prevention_menu.unwrap();
                        assert_ne!(menu.source, menu.resolving_source);
                        assert!(menu.source_history.is_none());
                        assert_eq!(
                            menu.simultaneous_choices_remaining,
                            Some(choosers.len() - ordinal)
                        );
                        assert_eq!(current.objects.get(fighters[0]).damage, 0);
                        assert_eq!(current.objects.get(fighters[1]).damage, 0);
                        let snapshot = restore(current);
                        assert_eq!(
                            snapshot.engine.pending_effect,
                            current.engine.pending_effect
                        );
                        if choose_shield {
                            let selected = option(current, true);
                            engine::step(current, Action::ChooseEffectOption(selected)).unwrap();
                            assert!(
                                matches!(next(current), Decision::ChooseEffectOption { player, .. }
                                if player == chooser)
                            );
                            assert_eq!(current.objects.get(fighters[0]).damage, 0);
                            assert_eq!(current.objects.get(fighters[1]).damage, 0);
                        }
                        let selected = option(current, false);
                        engine::step(current, Action::ChooseEffectOption(selected)).unwrap();
                        if opposing_wall && ordinal == 0 {
                            assert_eq!(current.objects.get(fighters[0]).damage, 0);
                            assert_eq!(current.objects.get(fighters[1]).damage, 0);
                        }
                    }
                    finish(current);
                    assert_eq!(current.objects.get(spell).zone, Zone::Graveyard);
                    assert_eq!(current.objects.get(fighters[0]).damage, 0);
                    assert_eq!(
                        current.objects.get(fighters[1]).damage,
                        if opposing_wall { 0 } else { 4 }
                    );
                    assert_eq!(
                        current.players[actor.index()].life,
                        lives[actor.index()] + if opposing_wall { 0 } else { 4 }
                    );
                    assert_eq!(
                        current.players[actor.opponent().index()].life,
                        lives[actor.opponent().index()]
                    );
                    assert!(current
                        .engine
                        .event_history
                        .iter()
                        .any(|event| matches!(event,
                        event::CommittedEvent::CrystalBatchPreparedV3 { capture }
                            if capture.ctx.source == spell && capture.fight.is_some())));
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
fn real_bushwhack_rechecks_control_zero_power_and_blinked_target_before_fight() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for case in 0..4 {
            let (mut state, _, fighters, spell) = fight_fixture(actor, false);
            match case {
                0 => {
                    // A typed current-power grant before resolution, not a rewrite of capture.
                    let binding = mtg_kernel::effect::EffectObjectBinding {
                        object: fighters[0],
                        expected_zone: Zone::Battlefield,
                        expected_zone_change_count: state
                            .objects
                            .get(fighters[0])
                            .zone_change_count,
                    };
                    state.engine.until_end_of_turn.push(
                        engine::UntilEndOfTurnEffect::ResolvedObjectEffect {
                            object_id: binding.object,
                            object_zone_change_count: binding.expected_zone_change_count,
                            layer: engine::Layers::POWER_TOUGHNESS,
                            timestamp: 1,
                            duration: engine::EffectDuration::EndOfTurn,
                            power: -5,
                            toughness: 0,
                            grant_haste: false,
                        },
                    );
                }
                1 => control_primitive(&mut state, fighters[0], actor.opponent()),
                _ => {
                    event::propose_and_commit(
                        &mut state,
                        ProposedEvent::zone_change(fighters[case - 2], Zone::Exile),
                    );
                    event::propose_and_commit(
                        &mut state,
                        ProposedEvent::zone_change(fighters[case - 2], Zone::Battlefield),
                    );
                    assert!(state.objects.get(fighters[case - 2]).zone_change_count > 0);
                }
            }
            let mut replay = restore(&state);
            for current in [&mut state, &mut replay] {
                if case == 0 {
                    mixed_menu(current);
                    let chosen = option(current, false);
                    engine::step(current, Action::ChooseEffectOption(chosen)).unwrap();
                }
                finish(current);
                assert_eq!(current.objects.get(spell).zone, Zone::Graveyard);
                assert_eq!(current.objects.get(fighters[0]).damage, 0);
                assert_eq!(current.objects.get(fighters[1]).damage, 0);
                assert!(!current
                    .engine
                    .event_history
                    .iter()
                    .any(|event| matches!(event,
                    event::CommittedEvent::Damage { source, .. } if fighters.contains(source))));
                assert!(matches!(
                    current.engine.active_replacements[0].kind,
                    event::ReplacementEffectKind::PreventNextDamage { remaining: 1, .. }
                ));
            }
            assert_eq!(
                serde_json::to_vec(&state).unwrap(),
                serde_json::to_vec(&replay).unwrap()
            );
        }
    }
}
#[test]
fn waiting_fight_and_independent_preparation_forgeries_refuse_readonly() {
    use event::CommittedEvent;
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, _, fighters, _) = fight_fixture(actor, true);
        mixed_menu(&mut state);
        for case in 0..5 {
            let mut bad = restore(&state);
            match case {
                0 => bad.objects.get_mut(fighters[0]).counters.plus1_plus1 += 1,
                1 => control_primitive(&mut bad, fighters[0], actor.opponent()),
                2 => {
                    for event in &mut bad.engine.event_history {
                        if let CommittedEvent::CrystalBatchPreparedV3 { capture } = event {
                            capture.fight.as_mut().unwrap().signed_powers[0] += 1;
                        }
                    }
                }
                3 => bad.engine.event_history.retain(|event| {
                    !matches!(event, CommittedEvent::CrystalBatchPreparedV3 { .. })
                }),
                _ => {
                    for event in &mut bad.engine.event_history {
                        if let CommittedEvent::CrystalExecutionV1 { execution } = event {
                            for step in &mut execution.steps {
                                if let Some(capture) = step.batch_capture.as_mut() {
                                    capture.preparation = None;
                                }
                            }
                        }
                    }
                }
            }
            let bytes = serde_json::to_vec(&bad).unwrap();
            assert!(engine::step(&mut bad, Action::Pass).is_err(), "case {case}");
            assert_eq!(serde_json::to_vec(&bad).unwrap(), bytes);
            assert!(mtg_kernel::crystal_public_v1::observe_v1(
                &bad,
                &mtg_kernel::policy_surface_v5::PolicySurfaceV5::new(),
                actor,
                0,
                0,
                0,
                1
            )
            .is_err());
            assert_eq!(serde_json::to_vec(&bad).unwrap(), bytes);
            assert!(matches!(
                engine::advance_until_decision(&mut bad),
                Decision::Halted { .. }
            ));
            bad.engine.halted = None;
            assert_eq!(serde_json::to_vec(&bad).unwrap(), bytes);
        }
    }
}

#[test]
fn real_fight_without_menu_commits_both_powers_lifelink_and_requires_preparation() {
    use event::CommittedEvent;
    use mtg_kernel::effect::crystal_prevention_v1::{BatchOutcomeV2, ExecutionV1};
    for actor in [PlayerId::P0, PlayerId::P1] {
        for power in [0, 5] {
            let mut state = ready(actor);
            let wall = source(&mut state, actor);
            let other = put(
                &mut state,
                actor.opponent(),
                "Shivan Dragon",
                Zone::Battlefield,
            );
            // Physical counter/keyword fixture setup before announcement.
            state.objects.get_mut(wall).counters.plus1_plus1 = power;
            for object in [wall, other] {
                state.objects.get_mut(object).v4.lifelink_keyword_counters = 1;
                shield(&mut state, wall, object, 1);
            }
            let spell = announce_fight(&mut state, actor, wall, other);
            let before_resolution = restore(&state);
            let lives = state.players.each_ref().map(|p| p.life);
            let mut replay = restore(&state);
            for current in [&mut state, &mut replay] {
                finish(current);
                assert_eq!(current.objects.get(spell).zone, Zone::Graveyard);
                assert_eq!(
                    current.objects.get(other).damage,
                    if power > 0 { 4 } else { 0 }
                );
                assert_eq!(
                    current.objects.get(wall).zone,
                    if power > 0 {
                        Zone::Battlefield
                    } else {
                        Zone::Graveyard
                    }
                );
                assert_eq!(
                    current.players[actor.index()].life,
                    lives[actor.index()] + if power > 0 { 4 } else { 0 }
                );
                assert_eq!(
                    current.players[actor.opponent().index()].life,
                    lives[actor.opponent().index()] + 4
                );
                assert!(!current
                    .engine
                    .event_history
                    .iter()
                    .any(|event| matches!(event,
                    CommittedEvent::CrystalPreventionBoundaryV1 { boundary }
                        if boundary.pending.as_ref().is_some_and(|p| p.choice.is_some()))));
            }
            assert_eq!(
                serde_json::to_vec(&state).unwrap(),
                serde_json::to_vec(&replay).unwrap()
            );
            let initial = state
                .engine
                .event_history
                .iter()
                .find_map(|event| match event {
                    CommittedEvent::CrystalPreventionBoundaryV1 { boundary }
                        if boundary.item.source == spell && boundary.previous.is_none() =>
                    {
                        Some(boundary.clone())
                    }
                    _ => None,
                })
                .unwrap();
            let actual_step = state
                .engine
                .event_history
                .iter()
                .find_map(|event| match event {
                    CommittedEvent::CrystalExecutionV1 { execution }
                        if execution.item.source == spell =>
                    {
                        execution
                            .steps
                            .iter()
                            .find(|s| s.batch_capture.is_some())
                            .cloned()
                    }
                    _ => None,
                })
                .unwrap();
            let mut bad = before_resolution;
            bad.engine.event_history =
                state.engine.event_history[..=initial.history_index as usize].to_vec();
            let mut step = actual_step;
            step.damage_history.clear();
            let capture = step.batch_capture.as_mut().unwrap();
            capture.preparation = None;
            capture.initial_outcomes.fill(BatchOutcomeV2::Prevented);
            let execution = ExecutionV1 {
                history_index: u32::try_from(bad.engine.event_history.len()).unwrap(),
                previous: initial.history_index,
                item: initial.item.clone(),
                before: initial.pending.clone().unwrap(),
                after: None,
                steps: vec![step],
                batch_commit: None,
                discard_commit: None,
                program_commit: None,
            };
            let mut closed = initial;
            closed.history_index = execution.history_index + 1;
            closed.previous = Some(execution.previous);
            closed.pending = None;
            closed.answer = None;
            bad.stack.retain(|item| item.source != spell);
            bad.engine.pending_effect = None;
            bad.engine
                .event_history
                .push(CommittedEvent::CrystalExecutionV1 {
                    execution: Box::new(execution),
                });
            bad.engine
                .event_history
                .push(CommittedEvent::CrystalPreventionBoundaryV1 { boundary: closed });
            let bytes = serde_json::to_vec(&bad).unwrap();
            assert!(engine::step(&mut bad, Action::Pass).is_err());
            assert_eq!(serde_json::to_vec(&bad).unwrap(), bytes);
            assert!(matches!(
                engine::advance_until_decision(&mut bad),
                Decision::Halted { .. }
            ));
            bad.engine.halted = None;
            assert_eq!(serde_json::to_vec(&bad).unwrap(), bytes);
        }
    }
}

#[test]
fn real_bushwhack_basic_land_mode_keeps_its_existing_private_search_owner() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let wall = source(&mut state, actor);
        let creature = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
        shield(&mut state, wall, creature, 1);
        let original_shields = state.engine.active_replacements.clone();
        let spell = put(&mut state, actor, "Bushwhack", Zone::Hand);
        state.players[actor.index()].mana_pool[ManaColor::G.pool_index()] = 1;
        priority(&mut state, actor);
        engine::step(&mut state, Action::CastSpell(spell)).unwrap();
        assert!(matches!(next(&mut state), Decision::ChooseSpellMode { .. }));
        engine::step(&mut state, Action::ChooseSpellMode(0)).unwrap();
        for _ in 0..12 {
            let decision = next(&mut state);
            if matches!(decision, Decision::ChooseEffectTargets { player, can_finish: true, .. }
                if player == actor)
            {
                break;
            }
            assert!(
                matches!(decision, Decision::CastSpellOrPass { .. }),
                "{decision:?}"
            );
            engine::step(&mut state, Action::Pass).unwrap();
        }
        assert!(
            matches!(next(&mut state), Decision::ChooseEffectTargets { player, can_finish: true, .. }
            if player == actor)
        );
        let mut replay = restore(&state);
        for current in [&mut state, &mut replay] {
            engine::step(current, Action::FinishEffectSelection).unwrap();
            finish(current);
            assert_eq!(current.objects.get(spell).zone, Zone::Graveyard);
            assert_eq!(current.engine.active_replacements, original_shields);
            assert!(!current.engine.event_history.iter().any(|event| matches!(
                event,
                event::CommittedEvent::CrystalPreventionBoundaryV1 { .. }
                    | event::CommittedEvent::CrystalBatchPreparedV3 { .. }
            )));
        }
        assert_eq!(
            serde_json::to_vec(&state).unwrap(),
            serde_json::to_vec(&replay).unwrap()
        );
    }
}

fn indrik_fixture(actor: PlayerId) -> (GameState, ObjectId, ObjectId) {
    let mut state = ready(actor);
    let wall = source(&mut state, actor);
    let other_wall = put(
        &mut state,
        actor.opponent(),
        "Crystal Barricade",
        Zone::Battlefield,
    );
    let target = put(
        &mut state,
        actor.opponent(),
        "Shivan Dragon",
        Zone::Battlefield,
    );
    let indrik = put(&mut state, actor, "Affectionate Indrik", Zone::Hand);
    shield(&mut state, wall, indrik, 1);
    shield(&mut state, other_wall, target, 1);
    state.players[actor.index()].mana_pool[ManaColor::G.pool_index()] = 1;
    state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 5;
    priority(&mut state, actor);
    engine::step(&mut state, Action::CastSpell(indrik)).unwrap();
    for _ in 0..20 {
        let decision = next(&mut state);
        if matches!(decision, Decision::ChooseTargets { .. }) {
            assert_eq!(state.objects.get(indrik).zone, Zone::Battlefield);
            assert!(
                matches!(decision, Decision::ChooseTargets { legal_targets, .. }
                if legal_targets.contains(&Target::Object(target)))
            );
            engine::step(&mut state, Action::ChooseTarget(Target::Object(target))).unwrap();
            let _ = next(&mut state);
            assert!(state
                .stack
                .iter()
                .any(|item| item.source == indrik && item.targets == vec![Target::Object(target)]));
            return (state, indrik, target);
        }
        assert!(
            matches!(decision, Decision::CastSpellOrPass { .. }),
            "{decision:?}"
        );
        engine::step(&mut state, Action::Pass).unwrap();
    }
    panic!("real Indrik trigger was not placed");
}
#[test]
fn real_indrik_optional_choice_then_source_fight_uses_distinct_owning_stages_and_restore() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for accept in [false, true] {
            let (mut state, indrik, target) = indrik_fixture(actor);
            mixed_menu(&mut state);
            assert!(
                matches!(next(&mut state), Decision::ChooseEffectOption { player, option_count: 2, .. }
                if player == actor)
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
            let offer = public.optional_fight_menu.unwrap();
            assert_eq!(offer.chooser, actor.into());
            assert!(offer.requires_original_source_creature);
            assert_eq!((offer.decline_index, offer.fight_index), (0, 1));
            assert!(public.prevention_menu.is_none());
            let mut replay = restore(&state);
            for current in [&mut state, &mut replay] {
                engine::step(current, Action::ChooseEffectOption(u16::from(accept))).unwrap();
                let answered = restore(current);
                assert_eq!(
                    answered.engine.pending_effect,
                    current.engine.pending_effect
                );
                assert!(current.engine.event_history.iter().any(|event| matches!(event,
                    event::CommittedEvent::CrystalProgramChoiceV3 { operation }
                        if operation.item.source == indrik && operation.option == u16::from(accept))));
                if accept {
                    for chooser in [actor, actor.opponent()] {
                        assert!(
                            matches!(next(current), Decision::ChooseEffectOption { player, .. }
                            if player == chooser)
                        );
                        assert_eq!(current.objects.get(indrik).damage, 0);
                        assert_eq!(current.objects.get(target).damage, 0);
                        let chosen = option(current, false);
                        engine::step(current, Action::ChooseEffectOption(chosen)).unwrap();
                    }
                }
                finish(current);
                assert_eq!(current.objects.get(indrik).damage, 0);
                assert_eq!(current.objects.get(target).damage, 0);
                assert_eq!(current.objects.get(indrik).zone, Zone::Battlefield);
                assert_eq!(current.objects.get(target).zone, Zone::Battlefield);
                assert_eq!(current.engine.active_replacements.len(), 2);
                assert_eq!(current.engine.event_history.iter().any(|event| matches!(event,
                    event::CommittedEvent::CrystalBatchPreparedV3 { capture } if capture.ctx.source == indrik)), accept);
            }
            assert_eq!(
                serde_json::to_vec(&state).unwrap(),
                serde_json::to_vec(&replay).unwrap()
            );
        }
    }
}
#[test]
fn real_indrik_choice_retains_original_controller_while_prevention_uses_current_recipient_controller(
) {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, indrik, target) = indrik_fixture(actor);
        // Current-control primitive response at real priority, before the trigger begins.
        control_primitive(&mut state, indrik, actor.opponent());
        mixed_menu(&mut state);
        assert!(
            matches!(next(&mut state), Decision::ChooseEffectOption { player, .. } if player == actor)
        );
        let mut replay = restore(&state);
        for current in [&mut state, &mut replay] {
            engine::step(current, Action::ChooseEffectOption(1)).unwrap();
            for recipient in [target, indrik] {
                assert!(
                    matches!(next(current), Decision::ChooseEffectOption { player, .. }
                    if player == actor.opponent())
                );
                let choice = current
                    .engine
                    .pending_effect
                    .as_ref()
                    .unwrap()
                    .choice
                    .as_ref()
                    .unwrap();
                assert!(
                    matches!(choice, mtg_kernel::effect::PendingEffectChoice::ChooseOption {
                    purpose: mtg_kernel::effect::EffectOptionChoicePurpose::CrystalPreventionV1 { choice }, ..
                } if choice.target == Target::Object(recipient))
                );
                let chosen = option(current, false);
                engine::step(current, Action::ChooseEffectOption(chosen)).unwrap();
            }
            finish(current);
            assert_eq!(current.objects.get(indrik).controller, actor.opponent());
            assert_eq!(current.objects.get(indrik).damage, 0);
            assert_eq!(current.objects.get(target).damage, 0);
        }
        assert_eq!(
            serde_json::to_vec(&state).unwrap(),
            serde_json::to_vec(&replay).unwrap()
        );
    }
}
#[test]
fn real_indrik_original_source_departure_or_reentry_cannot_fight_from_lki() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for reenter in [false, true] {
            let (mut state, indrik, target) = indrik_fixture(actor);
            let original = state
                .stack
                .iter()
                .find(|item| item.source == indrik)
                .unwrap()
                .v4
                .stack_item_id;
            event::propose_and_commit(
                &mut state,
                ProposedEvent::zone_change(indrik, Zone::Graveyard),
            );
            if reenter {
                event::propose_and_commit(
                    &mut state,
                    ProposedEvent::zone_change(indrik, Zone::Battlefield),
                );
            }
            for _ in 0..30 {
                let decision = next(&mut state);
                if matches!(decision, Decision::ChooseEffectOption { .. })
                    && state
                        .engine
                        .pending_effect
                        .as_ref()
                        .is_some_and(|p| p.resolving_item.v4.stack_item_id == original)
                {
                    break;
                }
                match decision {
                    Decision::CastSpellOrPass { .. } => {
                        engine::step(&mut state, Action::Pass).unwrap()
                    }
                    Decision::ChooseTargets { legal_targets, .. } => {
                        assert!(legal_targets.contains(&Target::Object(target)));
                        engine::step(&mut state, Action::ChooseTarget(Target::Object(target)))
                            .unwrap();
                    }
                    Decision::ChooseEffectOption { .. } => {
                        // The genuinely new reentry trigger resolves first; decline it.
                        engine::step(&mut state, Action::ChooseEffectOption(0)).unwrap();
                    }
                    Decision::OrderTriggers { pending, .. } => engine::step(
                        &mut state,
                        Action::OrderTriggers((0..pending.len()).collect()),
                    )
                    .unwrap(),
                    _ => panic!("unexpected reentry trigger decision {decision:?}"),
                }
            }
            assert!(state.engine.pending_effect.as_ref().is_some_and(|p| p
                .resolving_item
                .v4
                .stack_item_id
                == original));
            assert!(
                matches!(next(&mut state), Decision::ChooseEffectOption { player, .. } if player == actor)
            );
            let mut replay = restore(&state);
            for current in [&mut state, &mut replay] {
                engine::step(current, Action::ChooseEffectOption(1)).unwrap();
                // Stop at the original trigger's genuine completion. A reentry
                // creates its own separate trigger, which must not be conflated.
                for _ in 0..12 {
                    let decision = next(current);
                    if !current
                        .stack
                        .iter()
                        .any(|item| item.v4.stack_item_id == original)
                        && current
                            .engine
                            .pending_effect
                            .as_ref()
                            .is_none_or(|p| p.resolving_item.v4.stack_item_id != original)
                    {
                        break;
                    }
                    assert!(
                        matches!(decision, Decision::CastSpellOrPass { .. }),
                        "{decision:?}"
                    );
                    engine::step(current, Action::Pass).unwrap();
                }
                assert!(!current
                    .stack
                    .iter()
                    .any(|item| item.v4.stack_item_id == original));
                assert_eq!(current.objects.get(target).damage, 0);
                assert!(!current
                    .engine
                    .event_history
                    .iter()
                    .any(|event| matches!(event,
                    event::CommittedEvent::Damage { source, .. } if *source == indrik)));
                assert!(current.engine.event_history.iter().any(|event| matches!(event,
                    event::CommittedEvent::CrystalPreventionBoundaryV1 { boundary }
                        if boundary.item.v4.stack_item_id == original && boundary.pending.is_none())));
            }
            assert_eq!(
                serde_json::to_vec(&state).unwrap(),
                serde_json::to_vec(&replay).unwrap()
            );
        }
    }
}
#[test]
fn real_indrik_program_answer_and_owner_erasure_forgeries_refuse_readonly() {
    use event::CommittedEvent;
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut waiting, indrik, _) = indrik_fixture(actor);
        mixed_menu(&mut waiting);
        let before = serde_json::to_vec(&waiting).unwrap();
        assert!(engine::step(&mut waiting, Action::ChooseEffectOption(2)).is_err());
        assert_eq!(serde_json::to_vec(&waiting).unwrap(), before);
        let mut answered = restore(&waiting);
        engine::step(&mut answered, Action::ChooseEffectOption(1)).unwrap();
        for case in 0..5 {
            let mut bad = restore(&answered);
            match case {
                0 => bad.engine.event_history.retain(|event| {
                    !matches!(event, CommittedEvent::CrystalProgramChoiceV3 { .. })
                }),
                1 => {
                    for event in &mut bad.engine.event_history {
                        if let CommittedEvent::CrystalProgramChoiceV3 { operation } = event {
                            operation.option = 0;
                        }
                    }
                }
                2 => {
                    bad.engine.pending_effect = None;
                    bad.engine.event_history.retain(|event| {
                        !matches!(
                            event,
                            CommittedEvent::CrystalPreventionBoundaryV1 { .. }
                                | CommittedEvent::CrystalExecutionV1 { .. }
                                | CommittedEvent::CrystalProgramChoiceV3 { .. }
                        )
                    });
                }
                3 => {
                    for event in &mut bad.engine.event_history {
                        if let CommittedEvent::CrystalExecutionV1 { execution } = event {
                            execution.program_commit = None;
                        }
                    }
                }
                _ => {
                    let operation = bad
                        .engine
                        .event_history
                        .iter()
                        .find_map(|event| match event {
                            CommittedEvent::CrystalProgramChoiceV3 { operation } => {
                                Some(operation.clone())
                            }
                            _ => None,
                        })
                        .unwrap();
                    bad.engine
                        .event_history
                        .push(CommittedEvent::CrystalProgramChoiceV3 { operation });
                }
            }
            assert!(bad.stack.iter().any(|item| item.source == indrik));
            let bytes = serde_json::to_vec(&bad).unwrap();
            assert!(engine::step(&mut bad, Action::Pass).is_err(), "case {case}");
            assert_eq!(serde_json::to_vec(&bad).unwrap(), bytes);
            assert!(mtg_kernel::crystal_public_v1::observe_v1(
                &bad,
                &mtg_kernel::policy_surface_v5::PolicySurfaceV5::new(),
                actor,
                0,
                0,
                0,
                1
            )
            .is_err());
            assert_eq!(serde_json::to_vec(&bad).unwrap(), bytes);
            assert!(matches!(
                engine::advance_until_decision(&mut bad),
                Decision::Halted { .. }
            ));
            bad.engine.halted = None;
            assert_eq!(serde_json::to_vec(&bad).unwrap(), bytes);
        }
    }
}

#[test]
fn real_indrik_can_target_its_transferred_source_and_self_fights_for_twice_power() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for protected in [false, true] {
            for shield_first in [false, true] {
                let mut state = ready(actor);
                let wall = source(&mut state, actor);
                let other_wall = if protected {
                    put(
                        &mut state,
                        actor.opponent(),
                        "Crystal Barricade",
                        Zone::Battlefield,
                    )
                } else {
                    wall
                };
                put(
                    &mut state,
                    actor.opponent(),
                    "Shivan Dragon",
                    Zone::Battlefield,
                );
                let indrik = put(&mut state, actor, "Affectionate Indrik", Zone::Hand);
                state.players[actor.index()].mana_pool[ManaColor::G.pool_index()] = 1;
                state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 5;
                priority(&mut state, actor);
                engine::step(&mut state, Action::CastSpell(indrik)).unwrap();
                for _ in 0..20 {
                    let decision = next(&mut state);
                    if matches!(decision, Decision::ChooseTargets { .. }) {
                        break;
                    }
                    assert!(
                        matches!(decision, Decision::CastSpellOrPass { .. }),
                        "{decision:?}"
                    );
                    engine::step(&mut state, Action::Pass).unwrap();
                }
                assert_eq!(state.objects.get(indrik).zone, Zone::Battlefield);
                // Typed control setup at the committed trigger-target checkpoint;
                // this is not a claimed printed control spell or priority response.
                control_primitive(&mut state, indrik, actor.opponent());
                state.objects.get_mut(indrik).v4.lifelink_keyword_counters = 1;
                shield(&mut state, other_wall, indrik, 1);
                assert!(
                    matches!(next(&mut state), Decision::ChooseTargets { legal_targets, .. }
                    if legal_targets.contains(&Target::Object(indrik)))
                );
                engine::step(&mut state, Action::ChooseTarget(Target::Object(indrik))).unwrap();
                mixed_menu(&mut state);
                assert!(
                    matches!(next(&mut state), Decision::ChooseEffectOption {player,..} if player == actor)
                );
                let life = state.players[actor.opponent().index()].life;
                let mut replay = restore(&state);
                for current in [&mut state, &mut replay] {
                    engine::step(current, Action::ChooseEffectOption(1)).unwrap();
                    if protected {
                        assert!(
                            matches!(next(current), Decision::ChooseEffectOption {player,..}
                            if player == actor.opponent())
                        );
                        let choice = current
                            .engine
                            .pending_effect
                            .as_ref()
                            .unwrap()
                            .choice
                            .as_ref()
                            .unwrap();
                        assert!(
                            matches!(choice, mtg_kernel::effect::PendingEffectChoice::ChooseOption {
                            purpose: mtg_kernel::effect::EffectOptionChoicePurpose::CrystalPreventionV1 {choice},..
                        } if choice.original_amount == 8 && choice.source == indrik && choice.target == Target::Object(indrik))
                        );
                        if shield_first {
                            let chosen = option(current, true);
                            engine::step(current, Action::ChooseEffectOption(chosen)).unwrap();
                            let _ = next(current);
                        }
                        let chosen = option(current, false);
                        engine::step(current, Action::ChooseEffectOption(chosen)).unwrap();
                    }
                    finish(current);
                    let packets = current
                        .engine
                        .event_history
                        .iter()
                        .filter_map(|event| match event {
                            event::CommittedEvent::Damage {
                                source,
                                target,
                                amount,
                            } if *source == indrik => Some((*target, *amount)),
                            _ => None,
                        })
                        .collect::<Vec<_>>();
                    assert_eq!(
                        packets,
                        if protected {
                            vec![]
                        } else {
                            vec![(Target::Object(indrik), 7)]
                        }
                    );
                    assert_eq!(
                        current.players[actor.opponent().index()].life,
                        life + if protected { 0 } else { 7 }
                    );
                    assert_eq!(
                        current.objects.get(indrik).zone,
                        if protected {
                            Zone::Battlefield
                        } else {
                            Zone::Graveyard
                        }
                    );
                    assert!(current
                        .engine
                        .event_history
                        .iter()
                        .any(|event| matches!(event,
                        event::CommittedEvent::CrystalBatchPreparedV3 {capture}
                            if capture.ctx.source == indrik && capture.proposals.len() == 1
                                && capture.proposals[0].amount == 8)));
                }
                assert_eq!(
                    serde_json::to_vec(&state).unwrap(),
                    serde_json::to_vec(&replay).unwrap()
                );
            }
        }
    }
}

fn cast_firebrand(state: &mut GameState, caster: PlayerId) -> ObjectId {
    let fire = put(state, caster, "Fanatical Firebrand", Zone::Hand);
    state.players[caster.index()].mana_pool[ManaColor::R.pool_index()] = 1;
    priority(state, caster);
    engine::step(state, Action::CastSpell(fire)).unwrap();
    finish(state);
    assert_eq!(state.objects.get(fire).zone, Zone::Battlefield);
    fire
}
fn activate_firebrand(
    state: &mut GameState,
    caster: PlayerId,
    fire: ObjectId,
    target: Target,
) -> mtg_kernel::ids::StackItemId {
    priority(state, caster);
    engine::step(state, Action::ActivateAbility(fire, 0)).unwrap();
    assert!(
        matches!(next(state),Decision::ChooseTargets{legal_targets,..} if legal_targets.contains(&target))
    );
    engine::step(state, Action::ChooseTarget(target)).unwrap();
    let _ = next(state); // Genuine tap/sacrifice/payment publication.
    assert_eq!(state.objects.get(fire).zone, Zone::Graveyard);
    let item = state.stack.iter().rev().find(|i| i.source == fire).unwrap();
    assert!(item.v4.fixed_damage_paid_v3.0.is_some());
    item.v4.stack_item_id
}
#[test]
fn real_firebrand_actual_payment_freezes_fixed_damage_and_current_or_reentered_source_does_not_own_it(
) {
    use mtg_kernel::event::CommittedEvent as E;
    for caster in [PlayerId::P0, PlayerId::P1] {
        for (prevent, reenter) in [false, true]
            .into_iter()
            .flat_map(|p| [false, true].map(|r| (p, r)))
        {
            let mut state = ready(caster);
            let wall = source(&mut state, caster);
            control_primitive(&mut state, wall, caster.opponent());
            let fire = cast_firebrand(&mut state, caster);
            let target = if prevent {
                put(
                    &mut state,
                    caster.opponent(),
                    "Shivan Dragon",
                    Zone::Battlefield,
                )
            } else {
                wall
            };
            if prevent {
                shield(&mut state, wall, target, 1);
            }
            state.objects.get_mut(fire).v4.lifelink_keyword_counters = 1;
            // Existing typed keyword-grant primitive, not a fabricated Firebrand root.
            mtg_kernel::effect::execute(
                &mtg_kernel::effect::EffectOp::GrantKeywordTargetUntilEndOfTurn {
                    object: mtg_kernel::effect::ObjectRef::ThisSource,
                    keyword: Keywords::DEATHTOUCH,
                },
                &mtg_kernel::effect::ExecCtx::no_targets(fire, caster),
                &mut state,
            );
            let id = activate_firebrand(&mut state, caster, fire, Target::Object(target));
            let capture = state
                .engine
                .event_history
                .iter()
                .find_map(|e| match e {
                    E::FixedDamageDepartureV3 { departure } => Some(departure.clone()),
                    _ => None,
                })
                .unwrap();
            assert!(capture.snapshot.tapped);
            assert!(capture.facts.lifelink && capture.facts.deathtouch);
            assert_eq!(capture.facts.source.controller, caster);
            if reenter {
                event::propose_and_commit(
                    &mut state,
                    ProposedEvent::zone_change(fire, Zone::Battlefield),
                );
                control_primitive(&mut state, fire, caster.opponent());
                state.objects.get_mut(fire).counters.plus1_plus1 = 100;
                assert!(!engine::has_effective_keyword(
                    &state,
                    fire,
                    Keywords::LIFELINK
                ));
            }
            let life = state.players[caster.index()].life;
            if prevent {
                mixed_menu(&mut state);
                let public = mtg_kernel::crystal_public_v1::observe_v1(
                    &state,
                    &mtg_kernel::policy_surface_v5::PolicySurfaceV5::new(),
                    caster.opponent(),
                    0,
                    0,
                    0,
                    1,
                )
                .unwrap();
                let menu = public.prevention_menu.unwrap();
                assert!(menu.source_history.is_none());
                let fixed = menu.fixed_source_history.unwrap();
                assert_eq!(fixed.printed_damage, 1);
                assert_eq!(fixed.captured_controller, caster.into());
                assert!(fixed.lifelink && fixed.deathtouch);
                assert_eq!(
                    menu.source.zone_change_count,
                    capture.facts.source.zone_change_count
                );
                let mut replay = restore(&state);
                for current in [&mut state, &mut replay] {
                    let choose = option(current, false);
                    engine::step(current, Action::ChooseEffectOption(choose)).unwrap();
                    finish(current);
                    assert_eq!(current.objects.get(target).damage, 0);
                    assert_eq!(current.players[caster.index()].life, life);
                    assert_eq!(current.engine.active_replacements.len(), 1);
                }
                assert_eq!(
                    serde_json::to_vec(&state).unwrap(),
                    serde_json::to_vec(&replay).unwrap()
                );
            } else {
                let mut replay = restore(&state);
                for current in [&mut state, &mut replay] {
                    finish(current);
                    // Actual one-point damage carries original lifelink/deathtouch
                    // even after original departure and another incarnation's return.
                    assert_eq!(current.players[caster.index()].life, life + 1);
                    assert_eq!(current.objects.get(target).zone, Zone::Graveyard);
                    assert_eq!(
                        current
                            .engine
                            .event_history
                            .iter()
                            .filter(|e| matches!(e,E::Damage {source,..} if *source==fire))
                            .count(),
                        1
                    );
                }
                assert_eq!(
                    serde_json::to_vec(&state).unwrap(),
                    serde_json::to_vec(&replay).unwrap()
                );
            }
            assert_eq!(state.engine.event_history.iter().filter(|e|matches!(e,E::FixedDamageRemovedV3{removal}
                if removal.item.v4.stack_item_id==id && removal.outcome==mtg_kernel::event::FixedDamageOutcomeV3::Resolved)).count(),1);
            assert!(!state.stack.iter().any(|i| i.v4.stack_item_id == id));
        }
    }
}
#[test]
fn real_firebrand_source_payment_guard_receipt_and_historical_proposal_forgeries_refuse_readonly() {
    use mtg_kernel::event::CommittedEvent as E;
    for caster in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(caster);
        let wall = source(&mut state, caster);
        control_primitive(&mut state, wall, caster.opponent());
        let fire = cast_firebrand(&mut state, caster);
        let target = put(
            &mut state,
            caster.opponent(),
            "Shivan Dragon",
            Zone::Battlefield,
        );
        shield(&mut state, wall, target, 1);
        let id = activate_firebrand(&mut state, caster, fire, Target::Object(target));
        for case in 0..5 {
            let mut bad = restore(&state);
            match case {
                0 => bad
                    .engine
                    .event_history
                    .retain(|e| !matches!(e, E::FixedDamagePaidV3 { .. })),
                1 => bad.stack.last_mut().unwrap().v4.fixed_damage_paid_v3.0 = None,
                2 => {
                    bad.engine.event_history.retain(|e| {
                        !matches!(
                            e,
                            E::FixedDamagePaidV3 { .. } | E::FixedDamageDepartureV3 { .. }
                        )
                    });
                    bad.stack.last_mut().unwrap().v4.fixed_damage_paid_v3.0 = None;
                }
                3 => bad.stack.clear(),
                _ => {
                    bad.stack.last_mut().unwrap().inline_effect =
                        Some(mtg_kernel::effect::EffectOp::Sequence(vec![]))
                }
            }
            let before = serde_json::to_vec(&bad).unwrap();
            assert!(engine::step(&mut bad, Action::Pass).is_err(), "case {case}");
            assert_eq!(serde_json::to_vec(&bad).unwrap(), before);
            assert!(matches!(
                engine::advance_until_decision(&mut bad),
                Decision::Halted { .. }
            ));
            bad.engine.halted = None;
            assert_eq!(serde_json::to_vec(&bad).unwrap(), before);
        }
        mixed_menu(&mut state);
        let mut bad = restore(&state);
        if let Some(mtg_kernel::effect::PendingEffectChoice::ChooseOption {
            purpose: mtg_kernel::effect::EffectOptionChoicePurpose::CrystalPreventionV1 { choice },
            ..
        }) = bad
            .engine
            .pending_effect
            .as_mut()
            .and_then(|p| p.choice.as_mut())
        {
            choice.historical_source = None;
        }
        let before = serde_json::to_vec(&bad).unwrap();
        assert!(engine::step(&mut bad, Action::ChooseEffectOption(0)).is_err());
        assert_eq!(serde_json::to_vec(&bad).unwrap(), before);
        let chosen = option(&state, false);
        engine::step(&mut state, Action::ChooseEffectOption(chosen)).unwrap();
        finish(&mut state);
        assert!(state.engine.event_history.iter().any(
            |e| matches!(e,E::FixedDamageRemovedV3{removal} if removal.item.v4.stack_item_id==id)
        ));
        for case in 0..3 {
            let mut bad = restore(&state);
            match case {
                0 => bad
                    .engine
                    .event_history
                    .retain(|e| !matches!(e, E::FixedDamageRemovedV3 { .. })),
                1 => {
                    let e = bad
                        .engine
                        .event_history
                        .iter()
                        .find(|e| matches!(e, E::FixedDamageRemovedV3 { .. }))
                        .unwrap()
                        .clone();
                    bad.engine.event_history.push(e);
                }
                _ => {
                    let paid = bad
                        .engine
                        .event_history
                        .iter()
                        .find_map(|e| match e {
                            E::FixedDamagePaidV3 { payment } => Some(payment.item.clone()),
                            _ => None,
                        })
                        .unwrap();
                    let removal = bad
                        .engine
                        .event_history
                        .iter_mut()
                        .find_map(|e| match e {
                            E::FixedDamageRemovedV3 { removal } => Some(removal),
                            _ => None,
                        })
                        .unwrap();
                    removal.outcome = mtg_kernel::event::FixedDamageOutcomeV3::Countered;
                    removal.item = paid;
                }
            }
            let before = serde_json::to_vec(&bad).unwrap();
            assert!(engine::step(&mut bad, Action::Pass).is_err());
            assert_eq!(serde_json::to_vec(&bad).unwrap(), before);
        }
    }
}

#[test]
fn real_firebrand_illegal_departed_reentered_target_closes_paid_owner_without_damage() {
    use mtg_kernel::event::CommittedEvent as E;
    for caster in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(caster);
        let fire = cast_firebrand(&mut state, caster);
        let target = put(
            &mut state,
            caster.opponent(),
            "Shivan Dragon",
            Zone::Battlefield,
        );
        let id = activate_firebrand(&mut state, caster, fire, Target::Object(target));
        event::propose_and_commit(&mut state, ProposedEvent::zone_change(target, Zone::Exile));
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(target, Zone::Battlefield),
        );
        let before_life = [state.players[0].life, state.players[1].life];
        let mut replay = restore(&state);
        for current in [&mut state, &mut replay] {
            finish(current);
            assert_eq!(current.objects.get(target).damage, 0);
            assert_eq!(
                [current.players[0].life, current.players[1].life],
                before_life
            );
            assert!(!current
                .engine
                .event_history
                .iter()
                .any(|e| matches!(e,E::Damage{source,..} if *source==fire)));
            assert_eq!(
                current
                    .engine
                    .event_history
                    .iter()
                    .filter(|e| matches!(e,
                E::FixedDamageRemovedV3{removal} if removal.item.v4.stack_item_id==id
                    && removal.outcome==mtg_kernel::event::FixedDamageOutcomeV3::NoLegalTarget))
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
#[test]
fn real_firebrand_damage_to_player_uses_captured_controller_without_a_crystal_on_battlefield() {
    use mtg_kernel::event::CommittedEvent as E;
    for caster in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(caster);
        let fire = cast_firebrand(&mut state, caster);
        state.objects.get_mut(fire).v4.lifelink_keyword_counters = 1;
        let id = activate_firebrand(&mut state, caster, fire, Target::Player(caster.opponent()));
        let before = [state.players[0].life, state.players[1].life];
        let mut replay = restore(&state);
        for current in [&mut state, &mut replay] {
            finish(current);
            assert_eq!(
                current.players[caster.index()].life,
                before[caster.index()] + 1
            );
            assert_eq!(
                current.players[caster.opponent().index()].life,
                before[caster.opponent().index()] - 1
            );
            assert_eq!(
                current
                    .engine
                    .event_history
                    .iter()
                    .filter(|e| matches!(e,
                E::FixedDamageRemovedV3{removal} if removal.item.v4.stack_item_id==id
                    && removal.outcome==mtg_kernel::event::FixedDamageOutcomeV3::Resolved))
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

#[test]
fn real_firebrand_two_paid_incarnations_on_one_arena_object_keep_distinct_original_facts() {
    use mtg_kernel::event::CommittedEvent as E;
    for caster in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(caster);
        let wall = source(&mut state, caster);
        control_primitive(&mut state, wall, caster.opponent());
        let fire = cast_firebrand(&mut state, caster);
        let target = put(
            &mut state,
            caster.opponent(),
            "Shivan Dragon",
            Zone::Battlefield,
        );
        shield(&mut state, wall, target, 1);
        state.objects.get_mut(fire).v4.lifelink_keyword_counters = 1;
        let original = activate_firebrand(&mut state, caster, fire, Target::Object(target));
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(fire, Zone::Battlefield),
        );
        control_primitive(&mut state, fire, caster.opponent());
        assert!(!engine::has_effective_keyword(
            &state,
            fire,
            Keywords::LIFELINK
        ));
        // The later incarnation genuinely pays its own Tap/Sacrifice cost above
        // the first activation, controlled by the other player.
        let newer = activate_firebrand(&mut state, caster.opponent(), fire, Target::Player(caster));
        assert_ne!(original, newer);
        let payments = state
            .engine
            .event_history
            .iter()
            .filter_map(|e| match e {
                E::FixedDamagePaidV3 { payment } => Some(payment.as_ref()),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(payments.len(), 2);
        assert_ne!(payments[0].departure, payments[1].departure);
        assert_eq!(payments[0].item.controller, caster);
        assert_eq!(payments[1].item.controller, caster.opponent());
        let life = [state.players[0].life, state.players[1].life];
        mixed_menu(&mut state); // Resolves newer ordinary player damage first.
        assert_eq!(state.players[caster.index()].life, life[caster.index()] - 1);
        assert_eq!(
            state.players[caster.opponent().index()].life,
            life[caster.opponent().index()]
        );
        assert!(state
            .engine
            .event_history
            .iter()
            .any(|e| matches!(e,E::FixedDamageRemovedV3{removal}
            if removal.item.v4.stack_item_id==newer)));
        assert_eq!(
            state
                .engine
                .pending_effect
                .as_ref()
                .unwrap()
                .resolving_item
                .v4
                .stack_item_id,
            original
        );
        let public = mtg_kernel::crystal_public_v1::observe_v1(
            &state,
            &mtg_kernel::policy_surface_v5::PolicySurfaceV5::new(),
            caster.opponent(),
            0,
            0,
            0,
            1,
        )
        .unwrap();
        let facts = public
            .prevention_menu
            .unwrap()
            .fixed_source_history
            .unwrap();
        assert_eq!(facts.captured_controller, caster.into());
        assert!(facts.lifelink);
        let mut replay = restore(&state);
        for current in [&mut state, &mut replay] {
            let choice = option(current, false);
            engine::step(current, Action::ChooseEffectOption(choice)).unwrap();
            finish(current);
            assert_eq!(
                current
                    .engine
                    .event_history
                    .iter()
                    .filter(|e| matches!(e, E::FixedDamageRemovedV3 { .. }))
                    .count(),
                2
            );
            assert_eq!(current.objects.get(target).damage, 0);
            assert_eq!(
                current.players[caster.index()].life,
                life[caster.index()] - 1
            );
            assert_eq!(
                current.players[caster.opponent().index()].life,
                life[caster.opponent().index()]
            );
        }
        assert_eq!(
            serde_json::to_vec(&state).unwrap(),
            serde_json::to_vec(&replay).unwrap()
        );
    }
}
#[test]
fn real_heartfire_shield_then_crystal_resumes_twice_and_retains_exact_completion_after_restore() {
    use mtg_kernel::event::CommittedEvent as E;
    for caster in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(caster);
        let wall = source(&mut state, caster);
        control_primitive(&mut state, wall, caster.opponent());
        let fire = put(&mut state, caster, "Heartfire Immolator", Zone::Hand);
        state.players[caster.index()].mana_pool[ManaColor::R.pool_index()] = 1;
        state.players[caster.index()].mana_pool[ManaColor::C.pool_index()] = 1;
        priority(&mut state, caster);
        engine::step(&mut state, Action::CastSpell(fire)).unwrap();
        finish(&mut state);
        let target = put(
            &mut state,
            caster.opponent(),
            "Shivan Dragon",
            Zone::Battlefield,
        );
        shield(&mut state, wall, target, 1);
        state.players[caster.index()].mana_pool[ManaColor::R.pool_index()] = 1;
        priority(&mut state, caster);
        engine::step(&mut state, Action::ActivateAbility(fire, 0)).unwrap();
        assert!(matches!(next(&mut state), Decision::ChooseTargets { .. }));
        engine::step(&mut state, Action::ChooseTarget(Target::Object(target))).unwrap();
        let _ = next(&mut state);
        let id = state
            .stack
            .iter()
            .find(|i| i.source == fire)
            .unwrap()
            .v4
            .stack_item_id;
        mixed_menu(&mut state);
        let mut before_answer = restore(&state);
        for current in [&mut state, &mut before_answer] {
            let selected = option(current, true);
            engine::step(current, Action::ChooseEffectOption(selected)).unwrap();
            assert!(
                matches!(next(current),Decision::ChooseEffectOption{player,..} if player==caster.opponent())
            );
            let mtg_kernel::effect::PendingEffectChoice::ChooseOption {
                purpose:
                    mtg_kernel::effect::EffectOptionChoicePurpose::CrystalPreventionV1 { choice },
                ..
            } = current
                .engine
                .pending_effect
                .as_ref()
                .unwrap()
                .choice
                .as_ref()
                .unwrap()
            else {
                panic!("remaining actual prevention");
            };
            assert_eq!(choice.original_amount, 2);
            assert_eq!(choice.remaining_amount, 1);
            assert_eq!(choice.historical_source, Some(id));
            assert_eq!(current.objects.get(target).damage, 0);
            assert!(!current
                .engine
                .event_history
                .iter()
                .any(|e| matches!(e, E::SourcePowerActivationFinishedV1 { .. })));
        }
        assert_eq!(
            serde_json::to_vec(&state).unwrap(),
            serde_json::to_vec(&before_answer).unwrap()
        );
        let mut answered_restore = restore(&state);
        for current in [&mut state, &mut answered_restore] {
            let selected = option(current, false);
            engine::step(current, Action::ChooseEffectOption(selected)).unwrap();
            finish(current);
            assert_eq!(
                current
                    .engine
                    .event_history
                    .iter()
                    .filter(|e| matches!(e,
                E::SourcePowerActivationFinishedV1{stack_item_id} if *stack_item_id==id))
                    .count(),
                1
            );
            assert_eq!(current.objects.get(target).damage, 0);
            assert!(current.engine.active_replacements.is_empty());
            assert!(!current.stack.iter().any(|i| i.v4.stack_item_id == id));
        }
        assert_eq!(
            serde_json::to_vec(&state).unwrap(),
            serde_json::to_vec(&answered_restore).unwrap()
        );
        for duplicate in [false, true] {
            let mut bad = restore(&state);
            if duplicate {
                let receipt=bad.engine.event_history.iter().find(|e|matches!(e,E::SourcePowerActivationFinishedV1{stack_item_id} if *stack_item_id==id)).unwrap().clone();
                bad.engine.event_history.push(receipt);
            } else {
                bad.engine.event_history.retain(|e|!matches!(e,E::SourcePowerActivationFinishedV1{stack_item_id} if *stack_item_id==id));
            }
            let bytes = serde_json::to_vec(&bad).unwrap();
            assert!(engine::step(&mut bad, Action::Pass).is_err());
            assert_eq!(serde_json::to_vec(&bad).unwrap(), bytes);
            assert!(matches!(
                engine::advance_until_decision(&mut bad),
                Decision::Halted { .. }
            ));
            bad.engine.halted = None;
            assert_eq!(serde_json::to_vec(&bad).unwrap(), bytes);
        }
    }
}

fn raid_crystal_publication(
    actor: PlayerId,
    recipient_player: bool,
) -> (GameState, ObjectId, ObjectId, ObjectId) {
    let mut state = ready(actor);
    let wall = source(&mut state, actor);
    control_primitive(&mut state, wall, actor.opponent());
    let attacker = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
    state.step = Step::DeclareAttackers;
    assert!(
        matches!(next(&mut state), Decision::DeclareAttackers { player, .. } if player == actor)
    );
    engine::step(&mut state, Action::DeclareAttackers(vec![attacker])).unwrap();
    for _ in 0..40 {
        let d = next(&mut state);
        if state.step == Step::Main2
            && matches!(d,Decision::CastSpellOrPass { player, .. } if player == actor)
        {
            break;
        }
        match d {
            Decision::CastSpellOrPass { .. } => engine::step(&mut state, Action::Pass).unwrap(),
            Decision::DeclareBlockers { .. } => {
                engine::step(&mut state, Action::DeclareBlockers(vec![])).unwrap()
            }
            other => panic!("actual Raid combat: {other:?}"),
        }
    }
    assert_eq!(state.step, Step::Main2);
    let recipient = put(
        &mut state,
        actor.opponent(),
        "Shivan Dragon",
        Zone::Battlefield,
    );
    let gorehorn = put(&mut state, actor, "Gorehorn Raider", Zone::Hand);
    state.players[actor.index()].mana_pool[ManaColor::R.pool_index()] = 1;
    state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 4;
    priority(&mut state, actor);
    engine::step(&mut state, Action::CastSpell(gorehorn)).unwrap();
    let _ = next(&mut state);
    for _ in 0..12 {
        let d = next(&mut state);
        if let Decision::ChooseTargets { legal_targets, .. } = d {
            let target = if recipient_player {
                Target::Player(actor)
            } else {
                Target::Object(recipient)
            };
            assert!(legal_targets.contains(&target));
            engine::step(&mut state, Action::ChooseTarget(target)).unwrap();
            let _ = next(&mut state);
            assert!(state.stack.iter().any(|i| i.source == gorehorn
                && i.kind == mtg_kernel::state::StackItemKind::TriggeredAbility));
            return (state, gorehorn, recipient, wall);
        }
        assert!(matches!(d, Decision::CastSpellOrPass { .. }), "{d:?}");
        engine::step(&mut state, Action::Pass).unwrap();
    }
    panic!("actual Gorehorn target publication");
}
fn raid_crystal_control(state: &mut GameState, object: ObjectId, controller: PlayerId) {
    let source = put(state, controller, "Lightning Bolt", Zone::Hand);
    let mut ctx = mtg_kernel::effect::ExecCtx::no_targets(source, controller);
    ctx.targets.push(Target::Object(object));
    ctx.target_contracts
        .push(mtg_kernel::state::StackTargetContractV4::capture(
            state,
            Target::Object(object),
        ));
    mtg_kernel::effect::execute(
        &mtg_kernel::effect::EffectOp::GainControlOfTargetCreatureUntilEndOfTurn {
            target_index: 0,
        },
        &ctx,
        state,
    );
    assert!(state.engine.halted.is_none());
    assert_eq!(state.objects.get(object).controller, controller);
}
fn raid_crystal_bounce(state: &mut GameState, actor: PlayerId, gorehorn: ObjectId) {
    let old_id = state
        .stack
        .iter()
        .find(|i| {
            i.source == gorehorn && i.kind == mtg_kernel::state::StackItemKind::TriggeredAbility
        })
        .unwrap()
        .v4
        .stack_item_id;
    let spell = put(state, actor, "Unsummon", Zone::Hand);
    state.players[actor.index()].mana_pool[ManaColor::U.pool_index()] = 1;
    priority(state, actor);
    engine::step(state, Action::CastSpell(spell)).unwrap();
    assert!(
        matches!(next(state),Decision::ChooseTargets { legal_targets, .. } if legal_targets.contains(&Target::Object(gorehorn)))
    );
    engine::step(state, Action::ChooseTarget(Target::Object(gorehorn))).unwrap();
    let _ = next(state);
    for _ in 0..12 {
        if state.objects.get(spell).zone == Zone::Graveyard {
            break;
        }
        assert!(matches!(next(state), Decision::CastSpellOrPass { .. }));
        engine::step(state, Action::Pass).unwrap();
    }
    assert_eq!(state.objects.get(gorehorn).zone, Zone::Hand);
    event::propose_and_commit(
        state,
        ProposedEvent::zone_change(gorehorn, Zone::Battlefield),
    );
    // Physical reentry after an actual Unsummon. This is not a second printed cast.
    // Its genuine new Raid trigger is targeted and countered by the existing
    // typed counter primitive, leaving only the original instruction in flight.
    assert!(matches!(next(state), Decision::ChooseTargets { .. }));
    engine::step(state, Action::ChooseTarget(Target::Player(actor))).unwrap();
    let _ = next(state);
    let new_id = state
        .stack
        .iter()
        .rev()
        .find(|i| i.source == gorehorn && i.v4.stack_item_id != old_id)
        .unwrap()
        .v4
        .stack_item_id;
    engine::counter_stack_item_by_id(state, new_id)
        .unwrap()
        .unwrap();
}
#[test]
fn real_gorehorn_raid_crystal_prevention_retains_original_source_and_both_owners_after_restore() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for bounce in [false, true] {
            for choose_shield in [false, true] {
                let (mut state, gorehorn, recipient, wall) = raid_crystal_publication(actor, false);
                state.objects.get_mut(gorehorn).v4.lifelink_keyword_counters = 1;
                let old_generation = state.objects.get(gorehorn).zone_change_count;
                if bounce {
                    raid_crystal_bounce(&mut state, actor, gorehorn);
                }

                shield(&mut state, wall, recipient, 5);
                let life = [state.players[0].life, state.players[1].life];
                mixed_menu(&mut state);
                let id = state
                    .engine
                    .pending_effect
                    .as_ref()
                    .unwrap()
                    .resolving_item
                    .v4
                    .stack_item_id;
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
                assert_eq!(menu.source.zone_change_count, old_generation);
                let facts = menu.raid_source_history.unwrap();
                assert_eq!(facts.ability_controller, actor.into());
                assert_eq!(facts.damage_controller, actor.into());
                assert_eq!(facts.historical, bounce);
                assert!(facts.lifelink);
                let mut replay = restore(&state);
                for current in [&mut state, &mut replay] {
                    let selected = option(current, choose_shield);
                    engine::step(current, Action::ChooseEffectOption(selected)).unwrap();
                    finish(current);
                    assert_eq!(current.objects.get(recipient).damage, 0);
                    assert_eq!([current.players[0].life, current.players[1].life], life);
                    assert_eq!(current.engine.event_history.iter().filter(|e|matches!(e,
                        mtg_kernel::event::CommittedEvent::GorehornCrystalCompletionV1 { completion }
                        if completion.item.v4.stack_item_id==id)).count(),1);
                    assert_eq!(
                        current
                            .engine
                            .event_history
                            .iter()
                            .filter(|e| matches!(e,
                        mtg_kernel::event::CommittedEvent::GorehornOperationV1 { operation }
                        if operation.item.v4.stack_item_id==id))
                            .count(),
                        0
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
fn real_gorehorn_crystal_no_menu_damage_uses_current_lifelink_controller_and_requires_completion() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, gorehorn, _, _) = raid_crystal_publication(actor, true);
        state.objects.get_mut(gorehorn).v4.lifelink_keyword_counters = 1;
        raid_crystal_control(&mut state, gorehorn, actor.opponent());
        let life = [state.players[0].life, state.players[1].life];
        let id = state
            .stack
            .iter()
            .find(|i| i.source == gorehorn)
            .unwrap()
            .v4
            .stack_item_id;
        let mut replay = restore(&state);
        for current in [&mut state, &mut replay] {
            finish(current);
            assert_eq!(current.players[actor.index()].life, life[actor.index()] - 2);
            assert_eq!(
                current.players[actor.opponent().index()].life,
                life[actor.opponent().index()] + 2
            );
        }
        assert_eq!(
            serde_json::to_vec(&state).unwrap(),
            serde_json::to_vec(&replay).unwrap()
        );
        let mut bad = restore(&state);
        bad.engine.event_history.retain(|e|!matches!(e,
            mtg_kernel::event::CommittedEvent::GorehornCrystalCompletionV1 { completion } if completion.item.v4.stack_item_id==id));
        let bytes = serde_json::to_vec(&bad).unwrap();
        assert!(engine::step(&mut bad, Action::Pass).is_err());
        assert_eq!(bytes, serde_json::to_vec(&bad).unwrap());
        assert!(matches!(
            engine::advance_until_decision(&mut bad),
            Decision::Halted { .. }
        ));
        bad.engine.halted = None;
        assert_eq!(bytes, serde_json::to_vec(&bad).unwrap());
    }
}
#[test]
fn real_gorehorn_crystal_captured_raid_recheck_and_completion_forgeries_refuse_readonly() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, gorehorn, recipient, wall) = raid_crystal_publication(actor, false);
        shield(&mut state, wall, recipient, 5);
        mixed_menu(&mut state);
        for mode in 0..5 {
            let mut bad = restore(&state);
            match mode {
                0 => bad.engine.event_history.retain(|e| {
                    !matches!(
                        e,
                        mtg_kernel::event::CommittedEvent::GorehornCrystalBeginV1 { .. }
                    )
                }),
                1 => {
                    let item = bad.stack.iter_mut().find(|i| i.source == gorehorn).unwrap();
                    item.v4.crystal_prevention_started_v1 = Default::default();
                }
                2 => {
                    for e in &mut bad.engine.event_history {
                        if let mtg_kernel::event::CommittedEvent::GorehornCrystalBeginV1 { begin } =
                            e
                        {
                            begin.attacked = None;
                        }
                    }
                }
                3 => bad.engine.event_history.retain(|e| {
                    !matches!(
                        e,
                        mtg_kernel::event::CommittedEvent::GorehornPreparedV1 { .. }
                    )
                }),
                _ => {
                    bad.engine.pending_effect = None;
                    bad.stack.retain(|i| i.source != gorehorn);
                }
            }
            let bytes = serde_json::to_vec(&bad).unwrap();
            assert!(engine::step(&mut bad, Action::Pass).is_err());
            assert_eq!(bytes, serde_json::to_vec(&bad).unwrap());
            assert!(matches!(
                engine::advance_until_decision(&mut bad),
                Decision::Halted { .. }
            ));
            bad.engine.halted = None;
            assert_eq!(bytes, serde_json::to_vec(&bad).unwrap());
        }
        let selected = option(&state, false);
        engine::step(&mut state, Action::ChooseEffectOption(selected)).unwrap();
        let mut replay = restore(&state);
        finish(&mut state);
        finish(&mut replay);
        assert_eq!(
            serde_json::to_vec(&state).unwrap(),
            serde_json::to_vec(&replay).unwrap()
        );
    }
}
#[test]
fn real_gorehorn_false_intervening_raid_completes_crystal_without_preparing_damage() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, gorehorn, recipient, _) = raid_crystal_publication(actor, false);
        // Typed stale-turn restore primitive; this is not a normal turn traversal.
        state.turn += 1;
        state.active_player = actor.opponent();
        state.priority_player = actor.opponent();
        let mut replay = restore(&state);
        for current in [&mut state, &mut replay] {
            finish(current);
            assert_eq!(current.objects.get(recipient).damage, 0);
            assert!(!current.engine.event_history.iter().any(|e| matches!(
                e,
                mtg_kernel::event::CommittedEvent::GorehornPreparedV1 { .. }
            )));
            assert!(current.engine.event_history.iter().any(|e|matches!(e,
                mtg_kernel::event::CommittedEvent::GorehornCrystalCompletionV1 { completion } if completion.item.source==gorehorn)));
        }
        assert_eq!(
            serde_json::to_vec(&state).unwrap(),
            serde_json::to_vec(&replay).unwrap()
        );
    }
}

fn crystal_rod_fixture(actor: PlayerId) -> (GameState, ObjectId, ObjectId, ObjectId) {
    let mut state = ready(actor);
    let wall = source(&mut state, actor);
    control_primitive(&mut state, wall, actor.opponent());
    let host = put(&mut state, actor, "Gorehorn Raider", Zone::Hand);
    state.players[actor.index()].mana_pool[ManaColor::R.pool_index()] = 1;
    state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 4;
    priority(&mut state, actor);
    engine::step(&mut state, Action::CastSpell(host)).unwrap();
    finish(&mut state);
    assert!(!state.player_attacked_this_turn_v1(actor));
    let rod = put(&mut state, actor, "Black Mage's Rod", Zone::Battlefield);
    state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 3;
    priority(&mut state, actor);
    engine::step(&mut state, Action::ActivateAbility(rod, 0)).unwrap();
    assert!(
        matches!(next(&mut state),Decision::ChooseTargets { legal_targets, .. } if legal_targets.contains(&Target::Object(host)))
    );
    engine::step(&mut state, Action::ChooseTarget(Target::Object(host))).unwrap();
    let _ = next(&mut state);
    finish(&mut state);
    assert!(state.objects.get(host).attachments.contains(&rod));
    let recipient = put(
        &mut state,
        actor.opponent(),
        "Shivan Dragon",
        Zone::Battlefield,
    );
    shield(&mut state, wall, recipient, 5); // Genuine mixed environment, unrelated to untargeted player damage.
    state.objects.get_mut(host).v4.lifelink_keyword_counters = 1;
    let bolt = announce_bolt(&mut state, actor, Target::Player(actor));
    let grant = state
        .stack
        .iter()
        .find(|i| i.v4.granted_by.is_some_and(|g| g.source == rod))
        .unwrap();
    assert_eq!(grant.source, host);
    assert_eq!(grant.controller, actor);
    assert_eq!(grant.v4.target_spec, None);
    assert!(grant.v4.gorehorn_published_v1.is_empty());
    (state, host, rod, bolt)
}
#[test]
fn actual_pinned_rod_grant_roundtrip_beside_crystal_keeps_host_and_equipment_roots_distinct() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, host, rod, bolt) = crystal_rod_fixture(actor);
        let life = [state.players[0].life, state.players[1].life];
        let grant_id = state
            .stack
            .iter()
            .find(|i| i.v4.granted_by.is_some_and(|g| g.source == rod))
            .unwrap()
            .v4
            .stack_item_id;
        mtg_kernel::crystal_public_v1::observe_v1(
            &state,
            &mtg_kernel::policy_surface_v5::PolicySurfaceV5::new(),
            actor,
            0,
            0,
            0,
            1,
        )
        .unwrap();
        let mut replay = restore(&state);
        for current in [&mut state, &mut replay] {
            finish(current);
            assert_eq!(current.objects.get(host).zone, Zone::Battlefield);
            assert_eq!(current.objects.get(bolt).zone, Zone::Graveyard);
            assert_eq!(current.players[actor.index()].life, life[actor.index()] - 2);
            assert_eq!(
                current.players[actor.opponent().index()].life,
                life[actor.opponent().index()] - 1
            );
            assert!(current.engine.event_history.iter().any(|e|matches!(e,mtg_kernel::event::CommittedEvent::Damage { source,target:Target::Player(p),amount:1,.. } if *source==host && *p==actor.opponent())));
            assert!(!current.engine.event_history.iter().any(|e|matches!(e,
                mtg_kernel::event::CommittedEvent::GorehornPublishedV1 { publication } if publication.item.v4.stack_item_id==grant_id)));
            assert!(!current.engine.event_history.iter().any(|e| matches!(
                e,
                mtg_kernel::event::CommittedEvent::GorehornCrystalBeginV1 { .. }
                    | mtg_kernel::event::CommittedEvent::GorehornCrystalCompletionV1 { .. }
            )));
            assert_eq!(current.engine.event_history.iter().filter(|e|matches!(e,
                mtg_kernel::event::CommittedEvent::CrystalPreventionBoundaryV1 { boundary } if boundary.item.v4.stack_item_id==grant_id && boundary.pending.is_none())).count(),1);
        }
        assert_eq!(
            serde_json::to_vec(&state).unwrap(),
            serde_json::to_vec(&replay).unwrap()
        );
    }
}
#[test]
fn actual_equipment_grant_recipe_and_historical_provenance_forgeries_refuse_readonly_with_crystal()
{
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (state, host, rod, _) = crystal_rod_fixture(actor);
        for mode in 0..4 {
            let mut bad = restore(&state);
            let item = bad
                .stack
                .iter_mut()
                .find(|i| i.v4.granted_by.is_some_and(|g| g.source == rod))
                .unwrap();
            match mode {
                0 => {
                    item.inline_effect = Some(mtg_kernel::effect::EffectOp::DealDamage {
                        target: mtg_kernel::effect::TargetRef::Opponent,
                        amount: 2,
                    })
                }
                1 => item.v4.granted_by.as_mut().unwrap().attached_to = None,
                2 => item.v4.granted_by.as_mut().unwrap().source = host,
                _ => item.controller = actor.opponent(),
            }
            let before = serde_json::to_vec(&bad).unwrap();
            assert!(engine::step(&mut bad, Action::Pass).is_err());
            assert_eq!(before, serde_json::to_vec(&bad).unwrap());
        }
    }
}

#[test]
fn departed_equipment_damage_grant_remains_explicitly_unrepresented_before_mutation() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, host, _, _) = crystal_rod_fixture(actor);
        // Genuine physical departure primitive; no invented printed bounce spell.
        event::propose_and_commit(&mut state, ProposedEvent::zone_change(host, Zone::Hand));
        assert!(state.engine.halted.is_none());
        let mut bad = restore(&state);
        let bytes = serde_json::to_vec(&bad).unwrap();
        assert!(engine::step(&mut bad, Action::Pass).is_err());
        assert_eq!(bytes, serde_json::to_vec(&bad).unwrap());
        assert!(matches!(
            engine::advance_until_decision(&mut bad),
            Decision::Halted { .. }
        ));
        bad.engine.halted = None;
        assert_eq!(bytes, serde_json::to_vec(&bad).unwrap());
    }
}
