//! Unregistered Inspiring Paladin fixtures; actual gameplay admission remains pending.
#![cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]

use mtg_kernel::card_def::{
    card_id_by_name, CardCapability, CardType, Keywords, Subtype, CARD_DEFS,
};
use mtg_kernel::engine::{self, Action, Decision};
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::{ManaColor, Pip};
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Target, Zone};

fn ready(active: PlayerId) -> GameState {
    let forest = card_id_by_name("Forest").unwrap();
    let mut state = GameState::new_from_libraries_with_starting_player_v1(
        &[forest; 40],
        &[forest; 40],
        |_| "Forest".into(),
        1109,
        active,
    );
    state.step = Step::Main1;
    state
}

fn put(state: &mut GameState, player: PlayerId, name: &str, zone: Zone) -> ObjectId {
    let card_def = card_id_by_name(name).unwrap();
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
        attachments: vec![],
        v4: ObjectStateV4::from_card_def(card_def),
        spell_copy_origin: None,
        plotted_turn: None,
        zone_change_count: 0,
    });
    match zone {
        Zone::Hand => state.players[player.index()].hand.push(id),
        Zone::Battlefield => state.players[player.index()].battlefield.push(id),
        Zone::Library => state.players[player.index()].library.push(id),
        _ => panic!("fixture zone"),
    }
    id
}

fn next(state: &mut GameState) -> Decision {
    let choice = engine::advance_until_decision(state);
    assert!(!matches!(choice, Decision::Halted { .. }), "{choice:?}");
    choice
}

fn priority(state: &mut GameState, player: PlayerId) {
    let Decision::CastSpellOrPass {
        player: chooser, ..
    } = next(state)
    else {
        panic!("priority decision absent");
    };
    if chooser != player {
        engine::step(state, Action::Pass).unwrap();
        assert!(
            matches!(next(state), Decision::CastSpellOrPass { player: chooser, .. } if chooser == player)
        );
    }
}

fn cast(state: &mut GameState, spell: ObjectId, target: Option<Target>) {
    assert!(
        matches!(next(state), Decision::CastSpellOrPass { castable_spells, .. } if castable_spells.contains(&spell))
    );
    engine::step(state, Action::CastSpell(spell)).unwrap();
    for _ in 0..16 {
        match next(state) {
            Decision::ChooseTargets { legal_targets, .. } => {
                let target = target.expect("fixture target");
                assert!(legal_targets.contains(&target));
                engine::step(state, Action::ChooseTarget(target)).unwrap();
            }
            Decision::OrderTriggers { pending, .. } => {
                engine::step(state, Action::OrderTriggers((0..pending.len()).collect())).unwrap();
            }
            Decision::CastSpellOrPass { .. } if state.engine.pending_cast.is_none() => return,
            choice => panic!("unexpected cast decision {choice:?}"),
        }
    }
    panic!("cast did not finish");
}

fn settle(state: &mut GameState) -> Decision {
    for _ in 0..64 {
        let choice = next(state);
        match choice {
            Decision::CastSpellOrPass { .. }
                if state.stack.is_empty() && state.engine.pending_triggers.is_empty() =>
            {
                return choice
            }
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            Decision::OrderTriggers { ref pending, .. } => {
                engine::step(state, Action::OrderTriggers((0..pending.len()).collect())).unwrap();
            }
            _ => return choice,
        }
    }
    panic!("resolution did not settle");
}

fn restored(state: &GameState) -> GameState {
    serde_json::from_slice(&serde_json::to_vec(state).unwrap()).unwrap()
}

fn same(state: &GameState, replay: &GameState) {
    assert_eq!(
        serde_json::to_value(state).unwrap(),
        serde_json::to_value(replay).unwrap()
    );
}

fn first_strike(state: &GameState, id: ObjectId) -> bool {
    engine::has_effective_keyword(state, id, Keywords::FIRST_STRIKE)
}

fn transfer(state: &mut GameState, id: ObjectId, controller: PlayerId) {
    let prior = state.objects.get(id).controller;
    state.players[prior.index()]
        .battlefield
        .retain(|object| *object != id);
    state.objects.get_mut(id).controller = controller;
    state.players[controller.index()].battlefield.push(id);
}

fn witness(state: &mut GameState, player: PlayerId, target: ObjectId) -> ObjectId {
    let aura = put(state, player, "Witness Protection", Zone::Hand);
    state.players[player.index()].mana_pool[1] = 1;
    priority(state, player);
    cast(state, aura, Some(Target::Object(target)));
    settle(state);
    aura
}

#[test]
fn metadata_cost_refusals_and_actual_casts_restore_in_both_seats() {
    let def = &CARD_DEFS[card_id_by_name("Inspiring Paladin").unwrap() as usize];
    assert_eq!(def.capability, CardCapability::Full);
    assert_eq!(def.types, &[CardType::Creature]);
    assert_eq!(def.subtypes, &[Subtype::Human, Subtype::Knight]);
    assert_eq!(
        (def.power, def.toughness, def.mana_value),
        (Some(3), Some(3), 3)
    );
    assert_eq!(def.cost.generic, 2);
    assert_eq!(def.cost.pips, &[Pip::Colored(ManaColor::W)]);
    assert!(!def.keywords.has(Keywords::FIRST_STRIKE));
    assert_eq!(def.controlled_counter_keyword, Some(Keywords::FIRST_STRIKE));
    for player in [PlayerId::P0, PlayerId::P1] {
        for (white, colorless, legal) in [(0, 3, false), (1, 1, false), (1, 2, true)] {
            let mut state = ready(player);
            let source = put(&mut state, player, "Inspiring Paladin", Zone::Hand);
            state.players[player.index()].mana_pool[0] = white;
            state.players[player.index()].mana_pool[5] = colorless;
            priority(&mut state, player);
            let Decision::CastSpellOrPass {
                castable_spells, ..
            } = next(&mut state)
            else {
                panic!("priority");
            };
            assert_eq!(castable_spells.contains(&source), legal);
            if !legal {
                let bytes = serde_json::to_vec(&state).unwrap();
                assert!(engine::step(&mut state, Action::CastSpell(source)).is_err());
                assert_eq!(serde_json::to_vec(&state).unwrap(), bytes);
                continue;
            }
            cast(&mut state, source, None);
            let mut replay = restored(&state);
            for current in [&mut state, &mut replay] {
                settle(current);
                assert_eq!(current.objects.get(source).zone, Zone::Battlefield);
                assert!(current.objects.get(source).summoning_sick);
                assert!(first_strike(current, source));
            }
            same(&state, &replay);
        }
    }
}

#[test]
fn continuous_counter_grant_tracks_live_creatures_counters_and_controller_turn() {
    for player in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(player);
        let source = put(&mut state, player, "Inspiring Paladin", Zone::Battlefield);
        let own = put(&mut state, player, "Llanowar Elves", Zone::Battlefield);
        let other = put(
            &mut state,
            player.opponent(),
            "Llanowar Elves",
            Zone::Battlefield,
        );
        let land = put(&mut state, player, "Forest", Zone::Battlefield);
        state.objects.get_mut(other).counters.plus1_plus1 = 1;
        state.objects.get_mut(land).counters.plus1_plus1 = 1;
        assert!(first_strike(&state, source));
        assert!(!first_strike(&state, own));
        assert!(!first_strike(&state, other));
        assert!(!first_strike(&state, land));
        state.objects.get_mut(own).counters.plus1_plus1 = 1;
        assert!(first_strike(&state, own));
        state.objects.get_mut(own).counters.plus1_plus1 = 0;
        assert!(!first_strike(&state, own));
        state.objects.get_mut(own).counters.plus1_plus1 = 2;
        put(&mut state, player, "Gnarlid Colony", Zone::Battlefield);
        let mut replay = restored(&state);
        for current in [&mut state, &mut replay] {
            assert!(first_strike(current, own));
            current.active_player = player.opponent();
            assert!(!first_strike(current, source));
            assert!(!first_strike(current, own));
            assert!(engine::has_effective_keyword(
                current,
                own,
                Keywords::TRAMPLE
            ));
            current.active_player = player;
            assert!(first_strike(current, source));
            assert!(first_strike(current, own));
        }
        same(&state, &replay);
    }
}

#[test]
fn source_transfer_recipient_transfer_and_departure_recompute_grants() {
    for player in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(player);
        let source = put(&mut state, player, "Inspiring Paladin", Zone::Battlefield);
        let own = put(&mut state, player, "Llanowar Elves", Zone::Battlefield);
        let other = put(
            &mut state,
            player.opponent(),
            "Llanowar Elves",
            Zone::Battlefield,
        );
        state.objects.get_mut(own).counters.plus1_plus1 = 1;
        state.objects.get_mut(other).counters.plus1_plus1 = 1;
        let mut replay = restored(&state);
        for current in [&mut state, &mut replay] {
            transfer(current, own, player.opponent());
            assert!(!first_strike(current, own));
            transfer(current, source, player.opponent());
            assert!(!first_strike(current, source));
            current.active_player = player.opponent();
            assert!(first_strike(current, source));
            assert!(first_strike(current, own));
            assert!(first_strike(current, other));
            current.objects.get_mut(source).v4.face_index = 1;
            assert!(!first_strike(current, source));
            assert!(!first_strike(current, other));
            current.objects.get_mut(source).v4.face_index = 0;
            event::propose_and_commit(current, ProposedEvent::zone_change(source, Zone::Graveyard));
            assert!(!first_strike(current, source));
            assert!(!first_strike(current, own));
        }
        same(&state, &replay);
    }
}

#[test]
fn actual_witness_casts_remove_source_abilities_and_order_recipient_grants() {
    for player in [PlayerId::P0, PlayerId::P1] {
        for source_is_target in [false, true] {
            let mut state = ready(player);
            let source = put(&mut state, player, "Inspiring Paladin", Zone::Battlefield);
            let own = put(&mut state, player, "Llanowar Elves", Zone::Battlefield);
            state.objects.get_mut(own).counters.plus1_plus1 = 1;
            let target = if source_is_target { source } else { own };
            let aura = witness(&mut state, player, target);
            assert!(!first_strike(&state, own));
            assert_eq!(first_strike(&state, source), !source_is_target);
            let mut replay = restored(&state);
            for current in [&mut state, &mut replay] {
                event::propose_and_commit(
                    current,
                    ProposedEvent::zone_change(aura, Zone::Graveyard),
                );
                assert!(first_strike(current, source));
                assert!(first_strike(current, own));
            }
            same(&state, &replay);
        }
        let mut state = ready(player);
        let own = put(&mut state, player, "Llanowar Elves", Zone::Battlefield);
        state.objects.get_mut(own).counters.plus1_plus1 = 1;
        let aura = witness(&mut state, player, own);
        let source = put(&mut state, player, "Inspiring Paladin", Zone::Hand);
        state.players[player.index()].mana_pool[0] = 1;
        state.players[player.index()].mana_pool[5] = 2;
        priority(&mut state, player);
        cast(&mut state, source, None);
        let mut replay = restored(&state);
        for current in [&mut state, &mut replay] {
            settle(current);
            assert!(first_strike(current, own));
            assert!(
                current.objects.get(source).v4.layer_timestamp
                    > current.objects.get(aura).v4.layer_timestamp
            );
            current.active_player = player.opponent();
            assert!(!first_strike(current, own));
            current.active_player = player;
            assert!(first_strike(current, own));
        }
        same(&state, &replay);
    }
}

#[test]
fn self_and_counter_grants_kill_real_blockers_before_normal_damage_in_both_seats() {
    for player in [PlayerId::P0, PlayerId::P1] {
        for self_attacks in [false, true] {
            let mut state = ready(player);
            mtg_kernel::combat_damage_v1::enable_foundations_combat_v1(&mut state).unwrap();
            let source = put(&mut state, player, "Inspiring Paladin", Zone::Battlefield);
            let attacker = if self_attacks {
                source
            } else {
                let own = put(&mut state, player, "Llanowar Elves", Zone::Battlefield);
                state.objects.get_mut(own).counters.plus1_plus1 = 1;
                own
            };
            let blocker = put(
                &mut state,
                player.opponent(),
                "Llanowar Elves",
                Zone::Battlefield,
            );
            state.objects.get_mut(blocker).counters.plus1_plus1 = if self_attacks { 2 } else { 1 };
            state.step = Step::DeclareAttackers;
            assert!(matches!(
                next(&mut state),
                Decision::DeclareAttackers { .. }
            ));
            engine::step(&mut state, Action::DeclareAttackers(vec![attacker])).unwrap();
            let mut replay = restored(&state);
            for current in [&mut state, &mut replay] {
                let mut finished = false;
                for _ in 0..80 {
                    match next(current) {
                        Decision::CastSpellOrPass { .. }
                            if current.step == Step::CombatDamage
                                && current.objects.get(blocker).zone == Zone::Graveyard =>
                        {
                            assert_eq!(current.objects.get(attacker).zone, Zone::Battlefield);
                            assert_eq!(current.objects.get(attacker).damage, 0);
                            assert_eq!(current.players[player.opponent().index()].life, 20);
                            finished = true;
                            break;
                        }
                        Decision::CastSpellOrPass { .. } => {
                            engine::step(current, Action::Pass).unwrap()
                        }
                        Decision::DeclareBlockers { .. } => engine::step(
                            current,
                            Action::DeclareBlockers(vec![(blocker, attacker)]),
                        )
                        .unwrap(),
                        Decision::ChooseCombatDamageRange { .. } => engine::step(
                            current,
                            Action::ChooseCombatDamageRange { upper_half: false },
                        )
                        .unwrap(),
                        other => panic!("unexpected combat decision {other:?}"),
                    }
                }
                assert!(finished, "first-strike combat did not finish");
            }
            same(&state, &replay);
        }
    }
}

#[test]
fn ordinary_cleanup_and_complete_opponent_turn_switch_both_grants_after_restore() {
    for player in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(player);
        let source = put(&mut state, player, "Inspiring Paladin", Zone::Battlefield);
        let own = put(&mut state, player, "Llanowar Elves", Zone::Battlefield);
        state.objects.get_mut(own).counters.plus1_plus1 = 1;
        state.step = Step::EndStep;
        assert!(first_strike(&state, source));
        assert!(first_strike(&state, own));
        let round = state.turn;
        let mut replay = restored(&state);
        for current in [&mut state, &mut replay] {
            let mut saw_opponent = false;
            let mut returned = false;
            for _ in 0..160 {
                let decision = next(current);
                if current.active_player == player.opponent() {
                    saw_opponent = true;
                    assert!(!first_strike(current, source));
                    assert!(!first_strike(current, own));
                } else if saw_opponent {
                    assert_eq!(current.turn, round + 1);
                    assert!(first_strike(current, source));
                    assert!(first_strike(current, own));
                    returned = true;
                    break;
                }
                match decision {
                    Decision::CastSpellOrPass { .. } => {
                        engine::step(current, Action::Pass).unwrap()
                    }
                    Decision::DeclareAttackers { .. } => {
                        engine::step(current, Action::DeclareAttackers(vec![])).unwrap()
                    }
                    other => panic!("unexpected turn decision {other:?}"),
                }
            }
            assert!(returned, "ordinary full turn did not finish");
        }
        same(&state, &replay);
    }
}
