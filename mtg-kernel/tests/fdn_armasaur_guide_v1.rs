//! Source preparation. Registration and card gameplay qualification are pending.
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
use mtg_kernel::trigger;

fn ready(player: PlayerId) -> GameState {
    let forest = card_id_by_name("Forest").unwrap();
    let mut state = GameState::new_from_libraries_with_starting_player_v1(
        &[forest; 40],
        &[forest; 40],
        |_| "Forest".into(),
        839,
        player,
    );
    state.step = Step::Main1;
    state
}

fn put(state: &mut GameState, player: PlayerId, name: &str, zone: Zone) -> ObjectId {
    let card_def =
        card_id_by_name(name).unwrap_or_else(|| panic!("unregistered fixture card: {name}"));
    let id = state.objects.push(GameObject {
        card_def,
        name: CARD_DEFS[card_def as usize].object_name.into(),
        owner: player,
        controller: player,
        zone,
        tapped: false,
        summoning_sick: false,
        damage: 0,
        counters: Default::default(),
        attachments: vec![],
        v4: ObjectStateV4::from_card_def(card_def),
        spell_copy_origin: None,
        plotted_turn: None,
        zone_change_count: 0,
    });
    match zone {
        Zone::Hand => state.players[player.index()].hand.push(id),
        Zone::Battlefield => state.players[player.index()].battlefield.push(id),
        _ => panic!("unsupported fixture zone"),
    }
    id
}

fn next(state: &mut GameState) -> Decision {
    let decision = engine::advance_until_decision(state);
    assert!(!matches!(decision, Decision::Halted { .. }), "{decision:?}");
    decision
}

fn targets(state: &mut GameState) -> Decision {
    for _ in 0..32 {
        match next(state) {
            choice @ Decision::ChooseTargets { .. } => return choice,
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            Decision::OrderTriggers { pending, .. } => {
                engine::step(state, Action::OrderTriggers((0..pending.len()).collect())).unwrap()
            }
            other => panic!("unexpected target decision {other:?}"),
        }
    }
    panic!("Armasaur target choice absent");
}

fn settle(state: &mut GameState) {
    for _ in 0..64 {
        match next(state) {
            Decision::CastSpellOrPass { .. }
                if state.stack.is_empty() && state.engine.pending_triggers.is_empty() =>
            {
                return
            }
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            other => panic!("unexpected resolution decision {other:?}"),
        }
    }
    panic!("Armasaur resolution did not settle");
}

fn restored(state: &GameState) -> GameState {
    serde_json::from_slice(&serde_json::to_vec(state).unwrap()).unwrap()
}

fn refuse(state: &mut GameState, action: Action) {
    let before = serde_json::to_vec(state).unwrap();
    assert!(engine::step(state, action).is_err());
    assert_eq!(serde_json::to_vec(state).unwrap(), before);
}

fn declare(state: &mut GameState, attackers: Vec<ObjectId>) {
    state.step = Step::DeclareAttackers;
    engine::step(state, Action::DeclareAttackers(attackers)).unwrap();
}

#[test]
fn exact_metadata_payment_and_pending_cast_restore_for_both_seats() {
    let id = card_id_by_name("Armasaur Guide").unwrap();
    assert_eq!(id, 367);
    let def = &CARD_DEFS[id as usize];
    assert_eq!(def.types, &[CardType::Creature]);
    assert_eq!(def.subtypes, &[Subtype::Dinosaur]);
    assert_eq!(
        (def.power, def.toughness, def.mana_value),
        (Some(4), Some(4), 5)
    );
    assert_eq!(def.cost.generic, 4);
    assert_eq!(def.cost.pips, &[Pip::Colored(ManaColor::W)]);
    assert!(def.keywords.has(Keywords::VIGILANCE));
    assert_eq!(
        trigger::trigger_target_spec(id),
        TargetSpec::ControlledCreature
    );
    for player in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(player);
        let source = put(&mut state, player, "Armasaur Guide", Zone::Hand);
        state.players[player.index()].mana_pool[5] = 5;
        next(&mut state);
        refuse(&mut state, Action::CastSpell(source));
        state.players[player.index()].mana_pool[5] = 4;
        state.players[player.index()].mana_pool[ManaColor::W.pool_index()] = 1;
        engine::step(&mut state, Action::CastSpell(source)).unwrap();
        next(&mut state);
        let mut replay = restored(&state);
        for current in [&mut state, &mut replay] {
            settle(current);
            assert_eq!(current.objects.get(source).zone, Zone::Battlefield);
            assert!(current.engine.pending_triggers.is_empty());
            assert_eq!(current.players[player.index()].mana_pool, [0; 6]);
        }
        assert_eq!(
            state.diagnostic_state_hash(),
            replay.diagnostic_state_hash()
        );
    }
}

#[test]
fn threshold_is_one_trigger_per_declaration_and_source_need_not_attack() {
    for player in [PlayerId::P0, PlayerId::P1] {
        for source_attacks in [false, true] {
            for count in 0..=4 {
                let mut state = ready(player);
                let source = put(&mut state, player, "Armasaur Guide", Zone::Battlefield);
                let mut attackers = vec![];
                if source_attacks && count > 0 {
                    attackers.push(source);
                }
                while attackers.len() < count {
                    attackers.push(put(&mut state, player, "Rat Token", Zone::Battlefield));
                }
                declare(&mut state, attackers.clone());
                assert_eq!(state.engine.pending_triggers.len(), usize::from(count >= 3));
                assert!(!state.objects.get(source).tapped);
                if count < 3 {
                    continue;
                }
                let Decision::ChooseTargets {
                    legal_targets,
                    can_finish,
                    remaining,
                    ..
                } = targets(&mut state)
                else {
                    unreachable!()
                };
                assert!(!can_finish);
                assert_eq!(remaining, 1);
                assert!(legal_targets.contains(&Target::Object(source)));
                engine::step(&mut state, Action::ChooseTarget(Target::Object(source))).unwrap();
                settle(&mut state);
                assert_eq!(state.objects.get(source).counters.plus1_plus1, 1);
                for attacker in attackers.into_iter().filter(|&id| id != source) {
                    assert_eq!(state.objects.get(attacker).counters.plus1_plus1, 0);
                }
            }
        }
    }
}

#[test]
fn opponent_declaration_and_direct_combat_insertion_do_not_trigger() {
    for player in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(player.opponent());
        put(&mut state, player, "Armasaur Guide", Zone::Battlefield);
        let attackers: Vec<_> = (0..3)
            .map(|_| {
                put(
                    &mut state,
                    player.opponent(),
                    "Rat Token",
                    Zone::Battlefield,
                )
            })
            .collect();
        declare(&mut state, attackers);
        assert!(state.engine.pending_triggers.is_empty());
        let mut state = ready(player);
        put(&mut state, player, "Armasaur Guide", Zone::Battlefield);
        state.engine.combat.attackers = (0..3)
            .map(|_| put(&mut state, player, "Rat Token", Zone::Battlefield))
            .collect();
        assert!(trigger::collect_and_process(&mut state).is_empty());
    }
}

#[test]
fn target_legality_source_departure_and_restored_resolution() {
    for player in [PlayerId::P0, PlayerId::P1] {
        for change in 0..4 {
            let mut state = ready(player);
            let source = put(&mut state, player, "Armasaur Guide", Zone::Battlefield);
            let recipient = put(&mut state, player, "Llanowar Elves", Zone::Battlefield);
            let land = put(&mut state, player, "Forest", Zone::Battlefield);
            let foreign = put(
                &mut state,
                player.opponent(),
                "Llanowar Elves",
                Zone::Battlefield,
            );
            let attackers = (0..3)
                .map(|_| put(&mut state, player, "Rat Token", Zone::Battlefield))
                .collect();
            declare(&mut state, attackers);
            targets(&mut state);
            refuse(&mut state, Action::ChooseTarget(Target::Object(land)));
            refuse(&mut state, Action::ChooseTarget(Target::Object(foreign)));
            refuse(&mut state, Action::FinishEffectSelection);
            engine::step(&mut state, Action::ChooseTarget(Target::Object(recipient))).unwrap();
            assert!(matches!(next(&mut state), Decision::CastSpellOrPass { .. }));
            assert_eq!(state.stack.len(), 1);
            event::propose_and_commit(&mut state, ProposedEvent::zone_change(source, Zone::Exile));
            match change {
                0 => {}
                1 => {
                    state.players[player.index()]
                        .battlefield
                        .retain(|&id| id != recipient);
                    state.players[player.opponent().index()]
                        .battlefield
                        .push(recipient);
                    state.objects.get_mut(recipient).controller = player.opponent();
                }
                2 => {
                    event::propose_and_commit(
                        &mut state,
                        ProposedEvent::zone_change(recipient, Zone::Exile),
                    );
                }
                3 => {
                    event::propose_and_commit(
                        &mut state,
                        ProposedEvent::zone_change(recipient, Zone::Exile),
                    );
                    event::propose_and_commit(
                        &mut state,
                        ProposedEvent::zone_change(recipient, Zone::Battlefield),
                    );
                }
                _ => unreachable!(),
            }
            // The threshold applies at declaration, not at resolution.
            state.engine.combat.attackers.clear();
            let mut replay = restored(&state);
            for current in [&mut state, &mut replay] {
                settle(current);
                assert_eq!(
                    current.objects.get(recipient).counters.plus1_plus1,
                    i32::from(change == 0)
                );
                assert_eq!(current.objects.get(source).zone, Zone::Exile);
            }
            assert_eq!(
                state.diagnostic_state_hash(),
                replay.diagnostic_state_hash()
            );
        }
    }
}
