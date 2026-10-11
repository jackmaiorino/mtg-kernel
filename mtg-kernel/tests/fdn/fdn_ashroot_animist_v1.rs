//! Future actual printed acceptance, requiring parent-owned real registration.
#![cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
use mtg_kernel::card_def::{card_id_by_name, Keywords, Subtype, CARD_DEFS};
use mtg_kernel::engine::{self, Action, Decision};
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::{ManaColor, Pip};

use mtg_kernel::rules_meaning_ashroot_v1 as meaning;
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Target, Zone};

fn put(state: &mut GameState, owner: PlayerId, name: &str, zone: Zone) -> ObjectId {
    let def = card_id_by_name(name).expect("true future definition registration is required");
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
        1127,
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
    panic!("missing priority");
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

fn restore(state: &GameState) -> GameState {
    let bytes = serde_json::to_vec(state).unwrap();
    let value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(serde_json::to_vec(&value).unwrap(), bytes);
    value
}
fn source(state: &mut GameState, actor: PlayerId) -> ObjectId {
    let id = put(state, actor, "Ashroot Animist", Zone::Hand);
    state.players[actor.index()].mana_pool[ManaColor::R.pool_index()] = 3;
    state.players[actor.index()].mana_pool[ManaColor::G.pool_index()] = 1;
    priority(state, actor);
    engine::step(state, Action::CastSpell(id)).unwrap();
    finish(state);
    assert_eq!(state.objects.get(id).zone, Zone::Battlefield);
    assert!(state.objects.get(id).summoning_sick);
    // Physical maturity setup; this fixture independently verifies actual casting.
    state.objects.get_mut(id).summoning_sick = false;
    id
}
fn attack(state: &mut GameState, source: ObjectId, target: ObjectId) {
    for _ in 0..30 {
        let d = next(state);
        match d {
            Decision::DeclareAttackers { eligible, .. } => {
                assert!(eligible.contains(&source));
                break;
            }
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            _ => panic!("{d:?}"),
        }
    }
    engine::step(state, Action::DeclareAttackers(vec![source])).unwrap();
    let d = next(state);
    let Decision::ChooseTargets {
        legal_targets,
        remaining: 1,
        ..
    } = d
    else {
        panic!("{d:?}")
    };
    assert!(legal_targets.contains(&Target::Object(target)));
    assert!(!legal_targets.contains(&Target::Object(source)));
    engine::step(state, Action::ChooseTarget(Target::Object(target))).unwrap();
    assert!(matches!(next(state), Decision::CastSpellOrPass { .. }));
    assert!(state.stack.iter().any(|row| row.source == source));
}
fn resolve_attack(state: &mut GameState) {
    finish(state);
}
fn cast_target(state: &mut GameState, actor: PlayerId, name: &str, target: ObjectId) {
    let spell = put(state, actor, name, Zone::Hand);
    state.players[actor.index()].mana_pool = [12; 6];
    priority(state, actor);
    engine::step(state, Action::CastSpell(spell)).unwrap();
    assert!(
        matches!(next(state),Decision::ChooseTargets {legal_targets,..} if legal_targets.contains(&Target::Object(target)))
    );
    engine::step(state, Action::ChooseTarget(Target::Object(target))).unwrap();
    assert!(matches!(next(state), Decision::CastSpellOrPass { .. }));
}
#[test]
fn printed_metadata_cast_payment_and_precise_root() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let id = source(&mut state, actor);
        let d = &CARD_DEFS[usize::from(state.objects.get(id).card_def)];
        assert_eq!(d.cost.generic, 2);
        assert_eq!(
            d.cost.pips,
            &[Pip::Colored(ManaColor::R), Pip::Colored(ManaColor::G)]
        );
        assert_eq!(d.subtypes, &[Subtype::Lizard, Subtype::Druid]);
        assert_eq!(engine::effective_power(&state, id), 4);
        assert!(engine::has_effective_keyword(&state, id, Keywords::TRAMPLE));
        let selected = meaning::meaning(
            &meaning::live_selector().unwrap(),
            state.objects.get(id).card_def,
            0,
        )
        .unwrap();
        meaning::validate(&selected).unwrap();
        let json = serde_json::to_value(&selected).unwrap();
        for field in json["rule"].as_object().unwrap().keys() {
            let mut bad = json.clone();
            bad["rule"][field] = false.into();
            assert!(meaning::validate(&serde_json::from_value(bad).unwrap()).is_err());
        }
    }
}
#[test]
fn actual_attack_targets_another_controlled_creature_and_samples_power_at_resolution() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let source = source(&mut state, actor);
        let target = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
        let opponent = put(
            &mut state,
            actor.opponent(),
            "Llanowar Elves",
            Zone::Battlefield,
        );
        attack(&mut state, source, target);
        assert!(!state
            .stack
            .last()
            .unwrap()
            .targets
            .contains(&Target::Object(opponent)));
        cast_target(&mut state, actor, "Bulk Up", source);
        let mut replay = restore(&state);
        for s in [&mut state, &mut replay] {
            resolve_attack(s);
            assert_eq!(engine::effective_power(s, source), 8);
            assert_eq!(engine::effective_power(s, target), 9);
            assert_eq!(engine::effective_toughness(s, target), 9);
            assert!(engine::has_effective_keyword(s, target, Keywords::TRAMPLE));
        }
        assert_eq!(
            state.diagnostic_state_hash(),
            replay.diagnostic_state_hash()
        );
    }
}
#[test]
fn original_departure_power_survives_source_return_and_is_not_rebound() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let source = source(&mut state, actor);
        let target = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
        attack(&mut state, source, target);
        event::propose_and_commit(&mut state, ProposedEvent::zone_change(source, Zone::Hand));
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(source, Zone::Battlefield),
        );
        let mut replay = restore(&state);
        for s in [&mut state, &mut replay] {
            resolve_attack(s);
            assert_eq!(engine::effective_power(s, target), 5);
            assert_eq!(engine::effective_power(s, source), 4);
            assert!(engine::has_effective_keyword(s, target, Keywords::TRAMPLE));
        }
        assert_eq!(
            state.diagnostic_state_hash(),
            replay.diagnostic_state_hash()
        );
    }
}
#[test]
fn departed_or_reentered_target_fizzles_without_boost_or_trample() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let source = source(&mut state, actor);
        let target = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
        attack(&mut state, source, target);
        event::propose_and_commit(&mut state, ProposedEvent::zone_change(target, Zone::Hand));
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(target, Zone::Battlefield),
        );
        let mut replay = restore(&state);
        for s in [&mut state, &mut replay] {
            resolve_attack(s);
            assert_eq!(engine::effective_power(s, target), 1);
            assert!(!engine::has_effective_keyword(s, target, Keywords::TRAMPLE));
        }
        assert_eq!(
            state.diagnostic_state_hash(),
            replay.diagnostic_state_hash()
        );
    }
}
#[test]
fn source_ability_removal_after_attack_changes_sample_not_captured_trigger() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        put(
            &mut state,
            actor.opponent(),
            "High Fae Trickster",
            Zone::Battlefield,
        );
        let source = source(&mut state, actor);
        let target = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
        attack(&mut state, source, target);
        cast_target(&mut state, actor.opponent(), "Witness Protection", source);
        let mut replay = restore(&state);
        for s in [&mut state, &mut replay] {
            resolve_attack(s);
            assert_eq!(engine::effective_power(s, source), 1);
            assert_eq!(engine::effective_power(s, target), 2);
            assert!(engine::has_effective_keyword(s, target, Keywords::TRAMPLE));
        }
        assert_eq!(
            state.diagnostic_state_hash(),
            replay.diagnostic_state_hash()
        );
    }
}
#[test]
fn actual_turn_cleanup_removes_both_effects_and_restores_identically() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let source = source(&mut state, actor);
        let target = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
        attack(&mut state, source, target);
        resolve_attack(&mut state);
        let mut replay = restore(&state);
        for s in [&mut state, &mut replay] {
            for _ in 0..120 {
                if s.active_player != actor {
                    break;
                }
                let d = next(s);
                match d {
                    Decision::CastSpellOrPass { .. } => engine::step(s, Action::Pass).unwrap(),
                    Decision::DeclareBlockers { .. } => {
                        engine::step(s, Action::DeclareBlockers(vec![])).unwrap()
                    }
                    _ => panic!("{d:?}"),
                }
            }
            assert_eq!(s.active_player, actor.opponent());
            assert_eq!(engine::effective_power(s, target), 1);
            assert!(!engine::has_effective_keyword(s, target, Keywords::TRAMPLE));
        }
        assert_eq!(
            state.diagnostic_state_hash(),
            replay.diagnostic_state_hash()
        );
    }
}
#[test]
fn forged_original_source_binding_or_controller_refuses_before_priority_mutation() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let source = source(&mut state, actor);
        let target = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
        attack(&mut state, source, target);
        for mode in 0..4 {
            let mut bad = restore(&state);
            let item = bad.stack.iter_mut().find(|i| i.source == source).unwrap();
            match mode {
                0 => {
                    let Some(mtg_kernel::effect::EffectOp::AshrootAttackBoostV1 {
                        source: binding,
                    }) = &mut item.inline_effect
                    else {
                        panic!()
                    };
                    binding.object = target;
                }
                1 => {
                    item.controller = actor.opponent();
                    item.v4.ability_source_contract.as_mut().unwrap().controller = actor.opponent();
                }
                2 => item.v4.ability_source_contract = None,
                _ => {
                    let Some(mtg_kernel::effect::EffectOp::AshrootAttackBoostV1 {
                        source: binding,
                    }) = &mut item.inline_effect
                    else {
                        panic!()
                    };
                    binding.expected_zone_change_count += 1;
                }
            }
            let before = serde_json::to_vec(&bad).unwrap();
            assert!(engine::step(&mut bad, Action::Pass).is_err());
            assert_eq!(serde_json::to_vec(&bad).unwrap(), before);
        }
    }
}

#[test]
fn typed_pending_placement_excludes_original_source_but_accepts_returned_incarnation() {
    // Physical event boundary: no claim that a player receives priority before
    // mandatory trigger placement. Actual declaration creates the captured root.
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let source = source(&mut state, actor);
        put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
        for _ in 0..30 {
            match next(&mut state) {
                Decision::DeclareAttackers { .. } => break,
                Decision::CastSpellOrPass { .. } => engine::step(&mut state, Action::Pass).unwrap(),
                d => panic!("{d:?}"),
            }
        }
        engine::step(&mut state, Action::DeclareAttackers(vec![source])).unwrap();
        event::propose_and_commit(&mut state, ProposedEvent::zone_change(source, Zone::Hand));
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(source, Zone::Battlefield),
        );
        let mut replay = restore(&state);
        for s in [&mut state, &mut replay] {
            assert!(
                matches!(next(s), Decision::ChooseTargets { legal_targets, .. }
                if legal_targets.contains(&Target::Object(source)))
            );
            engine::step(s, Action::ChooseTarget(Target::Object(source))).unwrap();
            resolve_attack(s);
            assert_eq!(engine::effective_power(s, source), 8);
            assert_eq!(engine::effective_toughness(s, source), 8);
        }
        assert_eq!(
            state.diagnostic_state_hash(),
            replay.diagnostic_state_hash()
        );
    }
}
#[test]
fn actual_target_control_change_fizzles_and_original_controller_remains_bound() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let source = source(&mut state, actor);
        let target = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
        attack(&mut state, source, target);
        put(
            &mut state,
            actor.opponent(),
            "High Fae Trickster",
            Zone::Battlefield,
        );
        cast_target(
            &mut state,
            actor.opponent(),
            "Involuntary Employment",
            target,
        );
        let mut replay = restore(&state);
        for s in [&mut state, &mut replay] {
            resolve_attack(s);
            assert_eq!(engine::effective_power(s, target), 1);
            assert!(!engine::has_effective_keyword(s, target, Keywords::TRAMPLE));
        }
        assert_eq!(
            state.diagnostic_state_hash(),
            replay.diagnostic_state_hash()
        );
    }
}
