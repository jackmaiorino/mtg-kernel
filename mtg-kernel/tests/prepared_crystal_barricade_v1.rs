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
