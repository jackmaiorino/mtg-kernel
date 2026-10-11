//! Printed-card acceptance for the actual candidate Bloodwitch definition.
//! Candidate source remains unaccepted until qualification and integration.
#![cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]

use mtg_kernel::card_def::{
    card_id_by_name, CardCapability, CardType, Subtype, TargetSpec, CARD_DEFS,
};
use mtg_kernel::engine::{self, Action, Decision};
use mtg_kernel::event::{self, CommittedEvent, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::{ManaColor, Pip};
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Target, Zone};
use mtg_kernel::trigger;

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
    let card_def = card_id_by_name(name).expect("reviewed Bloodwitch catalog admission required");
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

fn collect(state: &mut GameState) {
    let pending = trigger::collect_and_process(state);
    assert!(state.engine.halted.is_none());
    state.engine.pending_triggers.extend(pending);
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
            Decision::ChooseTargets {
                player,
                legal_targets,
                ..
            } => {
                assert_eq!(legal_targets, vec![Target::Player(player.opponent())]);
                let before = state.state_hash();
                assert!(engine::step(state, Action::ChooseTarget(Target::Player(player))).is_err());
                assert_eq!(state.state_hash(), before);
                Action::ChooseTarget(Target::Player(player.opponent()))
            }
            other => panic!("unexpected Bloodwitch decision: {other:?}"),
        };
        engine::step(state, action).unwrap();
    }
    panic!("Bloodwitch resolution did not settle");
}

fn copy(state: &GameState) -> GameState {
    serde_json::from_slice(&serde_json::to_vec(state).unwrap()).unwrap()
}

#[test]
fn printed_metadata_and_cast_cost_match_pinned_mage_for_both_seats() {
    let id = card_id_by_name("Vengeful Bloodwitch").expect("actual candidate admission required");
    let card = &CARD_DEFS[id as usize];
    assert_eq!(card.capability, CardCapability::Full);
    assert_eq!(card.types, &[CardType::Creature]);
    assert_eq!(card.subtypes, &[Subtype::Vampire, Subtype::Warlock]);
    assert_eq!(
        (card.power, card.toughness, card.mana_value),
        (Some(1), Some(1), 2)
    );
    assert_eq!(card.colors, &[ManaColor::B]);
    assert_eq!(card.cost.generic, 1);
    assert_eq!(card.cost.pips, &[Pip::Colored(ManaColor::B)]);
    assert_eq!(card.keywords.0, 0);
    assert_eq!(trigger::trigger_target_spec(id), TargetSpec::TargetOpponent);
    for player in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(player);
        let source = put(&mut state, player, "Vengeful Bloodwitch", Zone::Hand);
        state.players[player.index()].mana_pool[ManaColor::B.pool_index()] = 2;
        assert!(
            matches!(next(&mut state), Decision::CastSpellOrPass { castable_spells, .. }
            if castable_spells.contains(&source))
        );
        engine::step(&mut state, Action::CastSpell(source)).unwrap();
        settle(&mut state);
        assert_eq!(state.objects.get(source).zone, Zone::Battlefield);
        assert_eq!(state.players[player.index()].mana_pool, [0; 6]);
        assert_eq!(state.players[player.index()].life, 20);
    }
}

#[test]
fn self_and_other_controlled_creature_deaths_target_opponent_and_restore() {
    for player in [PlayerId::P0, PlayerId::P1] {
        for self_death in [false, true] {
            let mut state = ready(player);
            let source = put(&mut state, player, "Vengeful Bloodwitch", Zone::Battlefield);
            let dead = if self_death {
                source
            } else {
                put(&mut state, player, "Llanowar Elves", Zone::Battlefield)
            };
            event::propose_and_commit(
                &mut state,
                ProposedEvent::zone_change(dead, Zone::Graveyard),
            );
            // Restore before collection as well as after captured pending placement.
            let mut restored = copy(&state);
            for current in [&mut state, &mut restored] {
                collect(current);
                assert_eq!(current.engine.pending_triggers.len(), 1);
                assert_eq!(current.engine.pending_triggers[0].controller, player);
                assert_eq!(
                    current.engine.pending_triggers[0].target_spec,
                    TargetSpec::TargetOpponent
                );
                assert!(trigger::collect_and_process(current).is_empty());
                settle(current);
                assert_eq!(current.players[player.index()].life, 21);
                assert_eq!(current.players[player.opponent().index()].life, 19);
            }
            assert_eq!(
                state.diagnostic_state_hash(),
                restored.diagnostic_state_hash()
            );
        }
    }
}

#[test]
fn opponent_creatures_noncreatures_and_non_death_departures_do_not_trigger() {
    for player in [PlayerId::P0, PlayerId::P1] {
        for (owner, name, destination) in [
            (player.opponent(), "Llanowar Elves", Zone::Graveyard),
            (player, "Forest", Zone::Graveyard),
            (player, "Llanowar Elves", Zone::Exile),
            (player, "Llanowar Elves", Zone::Hand),
            (player, "Vengeful Bloodwitch", Zone::Exile),
        ] {
            let mut state = ready(player);
            put(&mut state, player, "Vengeful Bloodwitch", Zone::Battlefield);
            let dead = put(&mut state, owner, name, Zone::Battlefield);
            event::propose_and_commit(&mut state, ProposedEvent::zone_change(dead, destination));
            collect(&mut state);
            assert!(state.engine.pending_triggers.is_empty());
            assert_eq!(state.players[0].life, 20);
            assert_eq!(state.players[1].life, 20);
        }
    }
}

#[test]
fn capture_survives_source_departure_before_collection_and_uses_event_controller() {
    for player in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(player);
        let source = put(&mut state, player, "Vengeful Bloodwitch", Zone::Battlefield);
        let victim = put(&mut state, player, "Llanowar Elves", Zone::Battlefield);
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(victim, Zone::Graveyard),
        );
        event::propose_and_commit(&mut state, ProposedEvent::zone_change(source, Zone::Exile));
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(source, Zone::Battlefield),
        );
        state.players[player.index()]
            .battlefield
            .retain(|id| *id != source);
        state.players[player.opponent().index()]
            .battlefield
            .push(source);
        state.objects.get_mut(source).controller = player.opponent();
        let mut restored = copy(&state);
        for current in [&mut state, &mut restored] {
            collect(current);
            assert_eq!(current.engine.pending_triggers.len(), 1);
            let pending = &current.engine.pending_triggers[0];
            assert_eq!(pending.controller, player);
            assert_eq!(pending.source_contract.unwrap().zone_change_count, 0);
            settle(current);
            assert_eq!(current.players[player.index()].life, 21);
            assert_eq!(current.players[player.opponent().index()].life, 19);
        }
        assert_eq!(
            state.diagnostic_state_hash(),
            restored.diagnostic_state_hash()
        );
    }
}

#[test]
fn finality_rewrites_own_sba_death_but_keeps_other_simultaneous_death_capture() {
    for player in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(player);
        let source = put(&mut state, player, "Vengeful Bloodwitch", Zone::Battlefield);
        let victim = put(&mut state, player, "Llanowar Elves", Zone::Battlefield);
        // Fixture installs the accepted finality-counter state directly. It
        // then exercises actual SBA departure, committed capture and resolution.
        state.objects.get_mut(source).v4.creature_upgrade =
            Some(mtg_kernel::standard_creatures_v1::CreatureUpgradeV1 {
                finality: 1,
                ..Default::default()
            });
        state.objects.get_mut(source).damage = 1;
        state.objects.get_mut(victim).damage = 1;
        let mut restored = copy(&state);
        for current in [&mut state, &mut restored] {
            collect(current);
            assert_eq!(current.objects.get(source).zone, Zone::Exile);
            assert_eq!(current.objects.get(victim).zone, Zone::Graveyard);
            assert_eq!(current.engine.pending_triggers.len(), 1);
            assert_eq!(current.engine.pending_triggers[0].source, source);
            settle(current);
            assert_eq!(current.players[player.index()].life, 21);
            assert_eq!(current.players[player.opponent().index()].life, 19);
        }
        assert_eq!(
            state.diagnostic_state_hash(),
            restored.diagnostic_state_hash()
        );
    }
}

#[test]
fn simultaneous_sources_observe_all_controlled_deaths_in_apnap_order() {
    for active in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(active);
        let first = put(
            &mut state,
            PlayerId::P0,
            "Vengeful Bloodwitch",
            Zone::Battlefield,
        );
        let second = put(
            &mut state,
            PlayerId::P0,
            "Vengeful Bloodwitch",
            Zone::Battlefield,
        );
        let opponent = put(
            &mut state,
            PlayerId::P1,
            "Vengeful Bloodwitch",
            Zone::Battlefield,
        );
        let victim = put(
            &mut state,
            PlayerId::P0,
            "Llanowar Elves",
            Zone::Battlefield,
        );
        event::propose_and_commit_batch(
            &mut state,
            vec![first, opponent, victim, second]
                .into_iter()
                .map(|id| ProposedEvent::zone_change(id, Zone::Graveyard))
                .collect(),
        );
        let mut restored = copy(&state);
        for current in [&mut state, &mut restored] {
            collect(current);
            assert_eq!(current.engine.pending_triggers.len(), 7);
            let controllers = current
                .engine
                .pending_triggers
                .iter()
                .map(|pending| pending.controller)
                .collect::<Vec<_>>();
            let expected = if active == PlayerId::P0 {
                vec![PlayerId::P0; 6]
                    .into_iter()
                    .chain([PlayerId::P1])
                    .collect::<Vec<_>>()
            } else {
                vec![PlayerId::P1]
                    .into_iter()
                    .chain([PlayerId::P0; 6])
                    .collect::<Vec<_>>()
            };
            assert_eq!(controllers, expected);
            settle(current);
            assert_eq!(current.players[0].life, 25);
            assert_eq!(current.players[1].life, 15);
        }
        assert_eq!(
            state.diagnostic_state_hash(),
            restored.diagnostic_state_hash()
        );
    }
}

#[test]
fn one_sba_wave_captures_source_first_and_source_last_deaths_identically() {
    for player in [PlayerId::P0, PlayerId::P1] {
        for source_first in [false, true] {
            let mut state = ready(player);
            let order = if source_first {
                ["Vengeful Bloodwitch", "Llanowar Elves"]
            } else {
                ["Llanowar Elves", "Vengeful Bloodwitch"]
            };
            for name in order {
                let id = put(&mut state, player, name, Zone::Battlefield);
                state.objects.get_mut(id).damage = 1;
            }
            collect(&mut state);
            assert_eq!(state.engine.pending_triggers.len(), 2);
            let mut restored = copy(&state);
            for current in [&mut state, &mut restored] {
                settle(current);
                assert_eq!(current.players[player.index()].life, 22);
                assert_eq!(current.players[player.opponent().index()].life, 18);
            }
            assert_eq!(
                state.diagnostic_state_hash(),
                restored.diagnostic_state_hash()
            );
        }
    }
}

#[test]
fn pending_target_prompt_and_stack_receipt_survive_snapshot_restore() {
    for player in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(player);
        let source = put(&mut state, player, "Vengeful Bloodwitch", Zone::Battlefield);
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(source, Zone::Graveyard),
        );
        collect(&mut state);
        let decision = next(&mut state);
        assert!(matches!(decision, Decision::ChooseTargets { .. }));
        let mut restored = copy(&state);
        assert_eq!(
            format!("{decision:?}"),
            format!("{:?}", next(&mut restored))
        );
        for current in [&mut state, &mut restored] {
            engine::step(
                current,
                Action::ChooseTarget(Target::Player(player.opponent())),
            )
            .unwrap();
            next(current);
            assert_eq!(current.stack.len(), 1);
            assert_eq!(
                current.stack[0]
                    .v4
                    .ability_source_contract
                    .unwrap()
                    .controller,
                player
            );
        }
        let mut stack_restored = copy(&state);
        for current in [&mut state, &mut restored, &mut stack_restored] {
            settle(current);
            assert_eq!(current.players[player.index()].life, 21);
        }
        assert_eq!(
            state.diagnostic_state_hash(),
            restored.diagnostic_state_hash()
        );
        assert_eq!(
            state.diagnostic_state_hash(),
            stack_restored.diagnostic_state_hash()
        );
    }
}

#[test]
fn duplicate_or_stale_capture_is_refused_before_trigger_placement() {
    for player in [PlayerId::P0, PlayerId::P1] {
        for corrupt in 0..3 {
            let mut state = ready(player);
            let source = put(&mut state, player, "Vengeful Bloodwitch", Zone::Battlefield);
            event::propose_and_commit(
                &mut state,
                ProposedEvent::zone_change(source, Zone::Graveyard),
            );
            let capture = state
                .engine
                .event_log
                .iter()
                .position(|event| {
                    matches!(
                        event,
                        CommittedEvent::ControlledCreatureDeathCaptureV1 { .. }
                    )
                })
                .unwrap();
            if corrupt == 0 {
                state
                    .engine
                    .event_log
                    .push(state.engine.event_log[capture].clone());
            } else if let CommittedEvent::ControlledCreatureDeathCaptureV1 {
                source, died, ..
            } = &mut state.engine.event_log[capture]
            {
                if corrupt == 1 {
                    source.zone_change_count += 3;
                } else {
                    died.zone_change_count += 3;
                }
            }
            let mut restored = copy(&state);
            for current in [&mut state, &mut restored] {
                assert!(trigger::collect_and_process(current).is_empty());
                assert!(current.engine.halted.is_some());
                assert!(current.stack.is_empty());
                assert_eq!(current.players[0].life, 20);
                assert_eq!(current.players[1].life, 20);
            }
            assert_eq!(
                state.diagnostic_state_hash(),
                restored.diagnostic_state_hash()
            );
        }
    }
}

#[test]
fn token_death_and_stolen_creature_death_use_controller_not_owner() {
    for player in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(player);
        put(&mut state, player, "Vengeful Bloodwitch", Zone::Battlefield);
        event::propose_and_commit(
            &mut state,
            ProposedEvent::CreateToken(event::CreateTokenProposed {
                token_def: card_id_by_name("Rabbit Token").unwrap(),
                controller: player,
                entry_face_index: None,
                touched_by: Vec::new(),
            }),
        );
        let token = state.players[player.index()]
            .battlefield
            .iter()
            .copied()
            .find(|id| state.objects.get(*id).v4.is_token)
            .unwrap();
        let stolen = put(
            &mut state,
            player.opponent(),
            "Llanowar Elves",
            Zone::Battlefield,
        );
        state.players[player.opponent().index()]
            .battlefield
            .retain(|id| *id != stolen);
        state.players[player.index()].battlefield.push(stolen);
        state.objects.get_mut(stolen).controller = player;
        event::propose_and_commit_batch(
            &mut state,
            [token, stolen]
                .into_iter()
                .map(|id| ProposedEvent::zone_change(id, Zone::Graveyard))
                .collect(),
        );
        collect(&mut state);
        assert_eq!(state.engine.pending_triggers.len(), 2);
        assert!(!state.players[player.index()].graveyard.contains(&token));
        assert!(state.players[player.opponent().index()]
            .graveyard
            .contains(&stolen));
        let mut restored = copy(&state);
        for current in [&mut state, &mut restored] {
            settle(current);
            assert_eq!(current.players[player.index()].life, 22);
            assert_eq!(current.players[player.opponent().index()].life, 18);
        }
        assert_eq!(
            state.diagnostic_state_hash(),
            restored.diagnostic_state_hash()
        );
    }
}

#[test]
fn removed_printed_ability_is_frozen_before_its_aura_leaves_the_death_batch() {
    for player in [PlayerId::P0, PlayerId::P1] {
        for source_dies in [false, true] {
            let mut state = ready(player);
            let source = put(&mut state, player, "Vengeful Bloodwitch", Zone::Battlefield);
            let aura = put(&mut state, player, "Witness Protection", Zone::Hand);
            event::propose_and_commit(
                &mut state,
                ProposedEvent::zone_change(aura, Zone::Battlefield),
            );
            state.objects.get_mut(aura).v4.attached_to = Some(mtg_kernel::state::ObjectLinkV4 {
                object: source,
                zone_change_count: state.objects.get(source).zone_change_count,
            });
            state.objects.get_mut(source).attachments.push(aura);
            let dead = if source_dies {
                source
            } else {
                put(&mut state, player, "Llanowar Elves", Zone::Battlefield)
            };
            event::propose_and_commit_batch(
                &mut state,
                [aura, dead]
                    .into_iter()
                    .map(|id| ProposedEvent::zone_change(id, Zone::Graveyard))
                    .collect(),
            );
            let mut restored = copy(&state);
            for current in [&mut state, &mut restored] {
                collect(current);
                assert!(current.engine.pending_triggers.is_empty());
                assert_eq!(current.players[player.index()].life, 20);
                assert_eq!(current.players[player.opponent().index()].life, 20);
            }
            assert_eq!(
                state.diagnostic_state_hash(),
                restored.diagnostic_state_hash()
            );
        }
    }
}

#[test]
fn coherent_opposing_bloodwitch_capture_forgery_has_no_committed_authority_both_seats() {
    for player in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(player);
        put(&mut state, player, "Vengeful Bloodwitch", Zone::Battlefield);
        let opposing = put(
            &mut state,
            player.opponent(),
            "Vengeful Bloodwitch",
            Zone::Battlefield,
        );
        let deceased = put(&mut state, player, "Llanowar Elves", Zone::Battlefield);
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(deceased, Zone::Graveyard),
        );
        let mut forged = state
            .engine
            .event_log
            .iter()
            .find(|event| {
                matches!(
                    event,
                    CommittedEvent::ControlledCreatureDeathCaptureV1 { .. }
                )
            })
            .unwrap()
            .clone();
        let CommittedEvent::ControlledCreatureDeathCaptureV1 {
            source,
            controller_before,
            ..
        } = &mut forged
        else {
            unreachable!()
        };
        *source = mtg_kernel::state::AbilitySourceContractV4::capture(&state, opposing);
        // Every other source field is the real opposing incarnation. The forged
        // controller is coherent with the real death and formerly passed liveness.
        source.controller = player;
        *controller_before = player;
        state.engine.event_log.push(forged);
        let mut restored = copy(&state);
        for current in [&mut state, &mut restored] {
            let committed = serde_json::to_vec(&current.engine.event_history).unwrap();
            assert!(trigger::collect_and_process(current).is_empty());
            assert!(current.engine.halted.is_some());
            assert!(current.engine.pending_triggers.is_empty());
            assert!(current.stack.is_empty());
            assert_eq!(
                serde_json::to_vec(&current.engine.event_history).unwrap(),
                committed
            );
            assert_eq!((current.players[0].life, current.players[1].life), (20, 20));
        }
        assert_eq!(
            state.diagnostic_state_hash(),
            restored.diagnostic_state_hash()
        );
    }
}

#[test]
fn committed_bloodwitch_death_capture_is_consumed_once_and_replay_preserves_history_both_seats() {
    for player in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(player);
        let source = put(&mut state, player, "Vengeful Bloodwitch", Zone::Battlefield);
        let deceased = put(&mut state, player, "Llanowar Elves", Zone::Battlefield);
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(deceased, Zone::Graveyard),
        );
        let original_log = state.engine.event_log.clone();
        let original_history = state.engine.event_history.clone();
        let pending = trigger::collect_and_process(&mut state);
        assert!(state.engine.halted.is_none());
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].source, source);
        assert_eq!(pending[0].controller, player);
        assert_eq!(
            &state.engine.event_history[..original_history.len()],
            original_history.as_slice()
        );
        assert_eq!(state.engine.event_history.len(), original_history.len() + 1);
        assert!(matches!(
            state.engine.event_history.last(),
            Some(CommittedEvent::ControlledCreatureDeathCaptureCollectedV1 { .. })
        ));
        state.engine.pending_triggers.extend(pending);
        let mut restored = copy(&state);
        for current in [&mut state, &mut restored] {
            current.engine.event_log = original_log.clone();
            let committed = serde_json::to_vec(&current.engine.event_history).unwrap();
            let existing_pending = serde_json::to_vec(&current.engine.pending_triggers).unwrap();
            assert!(trigger::collect_and_process(current).is_empty());
            assert!(current.engine.halted.is_some());
            assert_eq!(
                serde_json::to_vec(&current.engine.event_history).unwrap(),
                committed
            );
            assert_eq!(
                serde_json::to_vec(&current.engine.pending_triggers).unwrap(),
                existing_pending
            );
            assert!(current.stack.is_empty());
            assert_eq!((current.players[0].life, current.players[1].life), (20, 20));
        }
        assert_eq!(
            state.diagnostic_state_hash(),
            restored.diagnostic_state_hash()
        );
    }
}
