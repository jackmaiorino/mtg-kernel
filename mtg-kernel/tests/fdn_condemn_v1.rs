//! Prepared printed-card acceptance. This card remains unregistered; run only
//! after a reviewed candidate catalog admits its real definition, without hooks.
#![cfg(feature = "limited-fdn-fixtures")]

use mtg_kernel::card_def::{card_id_by_name, CardType, Keywords, TargetSpec, CARD_DEFS};
use mtg_kernel::engine::{self, Action, Decision};
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::ManaColor;
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Target, Zone};

fn ready(player: PlayerId) -> GameState {
    let forest = card_id_by_name("Forest").unwrap();
    let mut state = GameState::new_from_libraries_with_starting_player_v1(
        &[forest; 40],
        &[forest; 40],
        |_| "Forest".into(),
        11031,
        player,
    );
    state.step = Step::Main1;
    state
}

fn put(state: &mut GameState, player: PlayerId, name: &str, zone: Zone) -> ObjectId {
    let card_def = card_id_by_name(name).expect("reviewed Condemn catalog admission required");
    let id = state.objects.push(GameObject {
        card_def,
        name: name.into(),
        owner: player,
        controller: player,
        zone,
        tapped: false,
        summoning_sick: false,
        damage: 0,
        counters: Default::default(),
        attachments: Vec::new(),
        v4: ObjectStateV4::from_card_def(card_def),
        spell_copy_origin: None,
        plotted_turn: None,
        zone_change_count: 0,
    });
    match zone {
        Zone::Battlefield => state.players[player.index()].battlefield.push(id),
        Zone::Hand => state.players[player.index()].hand.push(id),
        _ => panic!("fixture zone"),
    }
    id
}

fn next(state: &mut GameState) -> Decision {
    let decision = engine::advance_until_decision(state);
    assert!(!matches!(decision, Decision::Halted { .. }), "{decision:?}");
    decision
}

fn settle(state: &mut GameState) {
    for _ in 0..100 {
        let decision = next(state);
        if matches!(decision, Decision::CastSpellOrPass { .. })
            && state.stack.is_empty()
            && state.engine.pending_triggers.is_empty()
        {
            return;
        }
        let action = match decision {
            Decision::CastSpellOrPass { .. } => Action::Pass,
            Decision::OrderTriggers { pending, .. } => {
                Action::OrderTriggers((0..pending.len()).collect())
            }
            other => panic!("unexpected Condemn decision: {other:?}"),
        };
        engine::step(state, action).unwrap();
    }
    panic!("Condemn resolution did not settle");
}

fn copy(state: &GameState) -> GameState {
    serde_json::from_slice(&serde_json::to_vec(state).unwrap()).unwrap()
}

fn combat(caster: PlayerId, owner: PlayerId, name: &str) -> (GameState, ObjectId) {
    let controller = caster.opponent();
    let mut state = ready(controller);
    let target = put(&mut state, owner, name, Zone::Hand);
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change_to_battlefield_under_controller(target, controller),
    );
    state.objects.get_mut(target).summoning_sick = false;
    state.step = Step::DeclareAttackers;
    assert!(
        matches!(next(&mut state), Decision::DeclareAttackers { eligible, .. } if eligible.contains(&target))
    );
    engine::step(&mut state, Action::DeclareAttackers(vec![target])).unwrap();
    assert!(
        matches!(next(&mut state), Decision::CastSpellOrPass { player, .. } if player == controller)
    );
    engine::step(&mut state, Action::Pass).unwrap();
    assert!(
        matches!(next(&mut state), Decision::CastSpellOrPass { player, .. } if player == caster)
    );
    (state, target)
}

fn announce(state: &mut GameState, caster: PlayerId, target: ObjectId) -> ObjectId {
    let spell = put(state, caster, "Condemn", Zone::Hand);
    state.players[caster.index()].mana_pool[ManaColor::W.pool_index()] = 1;
    assert!(
        matches!(next(state), Decision::CastSpellOrPass { castable_spells, .. } if castable_spells.contains(&spell))
    );
    engine::step(state, Action::CastSpell(spell)).unwrap();
    let legal = match next(state) {
        Decision::ChooseTargets { legal_targets, .. } => legal_targets,
        other => panic!("{other:?}"),
    };
    assert!(legal.contains(&Target::Object(target)));
    let before = state.state_hash();
    assert!(engine::step(state, Action::ChooseTarget(Target::Player(caster))).is_err());
    assert_eq!(state.state_hash(), before);
    engine::step(state, Action::ChooseTarget(Target::Object(target))).unwrap();
    assert!(matches!(next(state), Decision::CastSpellOrPass { .. }));
    assert!(state.stack.iter().any(|item| item.source == spell));
    spell
}

fn gain_events(state: &GameState) -> Vec<(PlayerId, i32)> {
    state
        .engine
        .event_history
        .iter()
        .filter_map(|event| match event {
            mtg_kernel::event::CommittedEvent::LifeGain { player, amount } => {
                Some((*player, *amount))
            }
            _ => None,
        })
        .collect()
}

#[test]
fn condemn_printed_metadata_owner_library_and_controller_life_survive_restore_both_seats() {
    let definition = &CARD_DEFS
        [usize::from(card_id_by_name("Condemn").expect("actual reviewed admission required"))];
    assert!(definition.is_executable());
    assert_eq!(definition.types, &[CardType::Instant]);
    assert!(definition.subtypes.is_empty());
    assert_eq!(definition.colors, &[ManaColor::W]);
    assert_eq!(definition.target_spec, TargetSpec::AttackingCreature);
    assert_eq!(definition.mana_value, 1);
    assert_eq!(definition.cost.generic, 0);
    assert_eq!(
        definition.cost.pips,
        &[mtg_kernel::mana::Pip::Colored(ManaColor::W)]
    );
    for caster in [PlayerId::P0, PlayerId::P1] {
        for owner in [caster, caster.opponent()] {
            let (mut state, target) = combat(caster, owner, "Guttersnipe");
            let original_library = state.players[owner.index()].library.clone();
            let spell = announce(&mut state, caster, target);
            let mut restored = copy(&state);
            for current in [&mut state, &mut restored] {
                settle(current);
                assert_eq!(current.objects.get(target).zone, Zone::Library);
                assert_eq!(current.objects.get(target).controller, owner);
                assert_eq!(
                    current.players[owner.index()].library,
                    [original_library.clone(), vec![target]].concat()
                );
                assert_eq!(current.players[caster.opponent().index()].life, 22);
                assert_eq!(current.players[caster.index()].life, 20);
                assert_eq!(gain_events(current), vec![(caster.opponent(), 2)]);
                assert_eq!(current.objects.get(spell).zone, Zone::Graveyard);
                assert!(!current.engine.combat.attackers.contains(&target));
                assert_eq!(
                    current.players[caster.index()].mana_pool[ManaColor::W.pool_index()],
                    0
                );
            }
            assert_eq!(
                state.diagnostic_state_hash(),
                restored.diagnostic_state_hash()
            );
        }
    }
}

#[test]
fn condemn_target_menu_excludes_idle_and_blocking_only_creatures_and_refuses_them_readonly() {
    for caster in [PlayerId::P0, PlayerId::P1] {
        let (mut state, target) = combat(caster, caster.opponent(), "Guttersnipe");
        let blocker = put(&mut state, caster, "Llanowar Elves", Zone::Battlefield);
        let idle = put(
            &mut state,
            caster.opponent(),
            "Llanowar Elves",
            Zone::Battlefield,
        );
        let land = put(&mut state, caster, "Forest", Zone::Battlefield);
        // Give Condemn its real post-blockers priority window.
        engine::step(&mut state, Action::Pass).unwrap();
        assert!(matches!(next(&mut state), Decision::DeclareBlockers { .. }));
        engine::step(&mut state, Action::DeclareBlockers(vec![(blocker, target)])).unwrap();
        assert!(
            matches!(next(&mut state), Decision::CastSpellOrPass { player, .. } if player == caster.opponent())
        );
        engine::step(&mut state, Action::Pass).unwrap();
        let spell = put(&mut state, caster, "Condemn", Zone::Hand);
        state.players[caster.index()].mana_pool[ManaColor::W.pool_index()] = 1;
        engine::step(&mut state, Action::CastSpell(spell)).unwrap();
        let legal = match next(&mut state) {
            Decision::ChooseTargets { legal_targets, .. } => legal_targets,
            other => panic!("{other:?}"),
        };
        assert_eq!(legal, vec![Target::Object(target)]);
        let before = serde_json::to_vec(&state).unwrap();
        for invalid in [blocker, idle, land] {
            assert!(
                engine::step(&mut state, Action::ChooseTarget(Target::Object(invalid))).is_err()
            );
            assert_eq!(serde_json::to_vec(&state).unwrap(), before);
        }
        engine::step(&mut state, Action::ChooseTarget(Target::Object(target))).unwrap();
        state = copy(&state);
        settle(&mut state);
        assert_eq!(state.objects.get(target).zone, Zone::Library);
        assert_eq!(state.objects.get(blocker).zone, Zone::Battlefield);
        assert_eq!(gain_events(&state), vec![(caster.opponent(), 2)]);
    }
}

#[test]
fn condemn_samples_response_growth_and_plus_one_counters_at_resolution_before_move() {
    for caster in [PlayerId::P0, PlayerId::P1] {
        let (mut state, target) = combat(caster, caster, "Guttersnipe");
        state.objects.get_mut(target).counters.plus1_plus1 = 2;
        announce(&mut state, caster, target);
        let growth = put(&mut state, caster, "Giant Growth", Zone::Hand);
        state.players[caster.index()].mana_pool[ManaColor::G.pool_index()] = 1;
        engine::step(&mut state, Action::CastSpell(growth)).unwrap();
        assert!(
            matches!(next(&mut state), Decision::ChooseTargets { legal_targets, .. } if legal_targets.contains(&Target::Object(target)))
        );
        engine::step(&mut state, Action::ChooseTarget(Target::Object(target))).unwrap();
        assert!(matches!(next(&mut state), Decision::CastSpellOrPass { .. }));
        state = copy(&state);
        settle(&mut state);
        assert_eq!(state.objects.get(target).zone, Zone::Library);
        assert_eq!(engine::effective_toughness(&state, target), 2);
        assert_eq!(gain_events(&state), vec![(caster.opponent(), 7)]);
        assert_eq!(state.players[caster.opponent().index()].life, 27);
        assert_eq!(state.players[caster.index()].life, 20);
    }
}

#[test]
fn condemn_fizzles_without_move_or_life_after_attack_departure_zone_departure_and_reentry() {
    for caster in [PlayerId::P0, PlayerId::P1] {
        for change in 0..3 {
            let (mut state, target) = combat(caster, caster.opponent(), "Guttersnipe");
            let spell = announce(&mut state, caster, target);
            match change {
                0 => state.engine.combat.attackers.clear(),
                1 => event::propose_and_commit(
                    &mut state,
                    ProposedEvent::zone_change(target, Zone::Hand),
                ),
                _ => {
                    event::propose_and_commit(
                        &mut state,
                        ProposedEvent::zone_change(target, Zone::Hand),
                    );
                    event::propose_and_commit(
                        &mut state,
                        ProposedEvent::zone_change(target, Zone::Battlefield),
                    );
                    // Reinstalling attack status cannot defeat the old target incarnation contract.
                    state.engine.combat.attackers = vec![target];
                }
            }
            let mut restored = copy(&state);
            for current in [&mut state, &mut restored] {
                let expected_zone = current.objects.get(target).zone;
                let generation = current.objects.get(target).zone_change_count;
                let owners_library = current.players[caster.opponent().index()].library.clone();
                settle(current);
                assert_eq!(current.objects.get(target).zone, expected_zone);
                assert_eq!(current.objects.get(target).zone_change_count, generation);
                assert_eq!(
                    current.players[caster.opponent().index()].library,
                    owners_library
                );
                assert!(gain_events(current).is_empty());
                assert_eq!((current.players[0].life, current.players[1].life), (20, 20));
                assert_eq!(current.objects.get(spell).zone, Zone::Graveyard);
            }
            assert_eq!(
                state.diagnostic_state_hash(),
                restored.diagnostic_state_hash()
            );
        }
    }
}

#[test]
fn condemn_protection_is_respected_at_offer_and_rechecked_after_targeting() {
    for caster in [PlayerId::P0, PlayerId::P1] {
        let (mut protected, target) =
            combat(caster, caster.opponent(), "Guardian of the Guildpact");
        let spell = put(&mut protected, caster, "Condemn", Zone::Hand);
        protected.players[caster.index()].mana_pool[ManaColor::W.pool_index()] = 1;
        assert!(
            !matches!(next(&mut protected), Decision::CastSpellOrPass { castable_spells, .. } if castable_spells.contains(&spell))
        );
        let before = serde_json::to_vec(&protected).unwrap();
        assert!(engine::step(&mut protected, Action::CastSpell(spell)).is_err());
        assert_eq!(serde_json::to_vec(&protected).unwrap(), before);
        assert_eq!(protected.objects.get(target).zone, Zone::Battlefield);
        let (mut state, target) = combat(caster, caster.opponent(), "Guttersnipe");
        announce(&mut state, caster, target);
        state.engine.until_end_of_turn.push(
            engine::UntilEndOfTurnEffect::ResolvedObjectKeywordEffect {
                object_id: target,
                object_zone_change_count: state.objects.get(target).zone_change_count,
                layer: engine::Layers::ABILITY_ADDING,
                timestamp: 999,
                duration: engine::EffectDuration::EndOfTurn,
                keywords: Keywords::PROTECTION_FROM_MONOCOLORED,
            },
        );
        state = copy(&state);
        settle(&mut state);
        assert_eq!(state.objects.get(target).zone, Zone::Battlefield);
        assert!(gain_events(&state).is_empty());
        assert_eq!((state.players[0].life, state.players[1].life), (20, 20));
    }
}

#[test]
fn condemn_can_target_own_attacker_and_uses_witness_override_lki_before_aura_departure() {
    for caster in [PlayerId::P0, PlayerId::P1] {
        for owner in [caster, caster.opponent()] {
            for with_witness in [false, true] {
                let mut state = ready(caster);
                let target = put(&mut state, owner, "Aegis Turtle", Zone::Hand);
                event::propose_and_commit(
                    &mut state,
                    ProposedEvent::zone_change_to_battlefield_under_controller(target, caster),
                );
                state.objects.get_mut(target).summoning_sick = false;
                let witness = if with_witness {
                    let aura = put(&mut state, caster, "Witness Protection", Zone::Hand);
                    state.players[caster.index()].mana_pool[ManaColor::U.pool_index()] = 1;
                    assert!(
                        matches!(next(&mut state), Decision::CastSpellOrPass { castable_spells, .. } if castable_spells.contains(&aura))
                    );
                    engine::step(&mut state, Action::CastSpell(aura)).unwrap();
                    assert!(
                        matches!(next(&mut state), Decision::ChooseTargets { legal_targets, .. } if legal_targets.contains(&Target::Object(target)))
                    );
                    engine::step(&mut state, Action::ChooseTarget(Target::Object(target))).unwrap();
                    settle(&mut state);
                    Some(aura)
                } else {
                    None
                };
                let toughness = if with_witness { 1 } else { 5 };
                assert_eq!(engine::effective_toughness(&state, target), toughness);
                state.step = Step::DeclareAttackers;
                assert!(
                    matches!(next(&mut state), Decision::DeclareAttackers { eligible, .. } if eligible.contains(&target))
                );
                engine::step(&mut state, Action::DeclareAttackers(vec![target])).unwrap();
                assert!(
                    matches!(next(&mut state), Decision::CastSpellOrPass { player, .. } if player == caster)
                );
                announce(&mut state, caster, target);
                let mut restored = copy(&state);
                for current in [&mut state, &mut restored] {
                    settle(current);
                    assert_eq!(current.objects.get(target).zone, Zone::Library);
                    assert_eq!(current.players[owner.index()].library.last(), Some(&target));
                    assert_eq!(engine::effective_toughness(current, target), 5);
                    assert_eq!(current.players[caster.index()].life, 20 + toughness);
                    assert_eq!(current.players[caster.opponent().index()].life, 20);
                    assert_eq!(gain_events(current), vec![(caster, toughness)]);
                    if let Some(aura) = witness {
                        assert_eq!(current.objects.get(aura).zone, Zone::Graveyard);
                    }
                }
                assert_eq!(
                    state.diagnostic_state_hash(),
                    restored.diagnostic_state_hash()
                );
            }
        }
    }
}
