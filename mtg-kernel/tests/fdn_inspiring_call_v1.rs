//! Unregistered Inspiring Call fixtures; actual gameplay admission remains pending.
#![cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]

use mtg_kernel::card_def::{card_id_by_name, CardCapability, CardType, Keywords, CARD_DEFS};
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
        Zone::Graveyard => state.players[player.index()].graveyard.push(id),
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

fn cleanup(state: &mut GameState) {
    assert!(state.stack.is_empty());
    let active = state.active_player;
    state.step = Step::End;
    state.priority_player = active;
    state.engine.priority_passes = [false, false];
    for _ in 0..8 {
        assert!(matches!(next(state), Decision::CastSpellOrPass { .. }));
        engine::step(state, Action::Pass).unwrap();
        if state.active_player != active {
            return;
        }
    }
    panic!("cleanup did not finish");
}

#[test]
fn printed_definition_matches_pinned_constructor() {
    let def = &CARD_DEFS[card_id_by_name("Inspiring Call").unwrap() as usize];
    assert_eq!(def.capability, CardCapability::Full);
    assert_eq!(def.types, &[CardType::Instant]);
    assert!(def.subtypes.is_empty());
    assert_eq!(def.mana_value, 3);
    assert_eq!(def.colors, &[ManaColor::G]);
    assert_eq!(def.cost.generic, 2);
    assert_eq!(def.cost.pips, &[Pip::Colored(ManaColor::G)]);
}

fn announce(state: &mut GameState, player: PlayerId) -> ObjectId {
    let spell = put(state, player, "Inspiring Call", Zone::Hand);
    state.players[player.index()].mana_pool[4] = 1;
    state.players[player.index()].mana_pool[5] = 2;
    priority(state, player);
    cast(state, spell, None);
    spell
}

#[test]
fn actual_spell_draws_once_per_countered_creature_including_zero_and_restores() {
    for player in [PlayerId::P0, PlayerId::P1] {
        for count in 0..4 {
            let mut state = ready(player);
            let chosen: Vec<_> = (0..count)
                .map(|i| {
                    let object = put(&mut state, player, "Llanowar Elves", Zone::Battlefield);
                    state.objects.get_mut(object).counters.plus1_plus1 = i + 1;
                    object
                })
                .collect();
            let plain = put(&mut state, player, "Llanowar Elves", Zone::Battlefield);
            let land = put(&mut state, player, "Forest", Zone::Battlefield);
            state.objects.get_mut(land).counters.plus1_plus1 = 7;
            let other = put(
                &mut state,
                player.opponent(),
                "Llanowar Elves",
                Zone::Battlefield,
            );
            state.objects.get_mut(other).counters.plus1_plus1 = 3;
            let spell = announce(&mut state, player);
            let before = state.players[player.index()].hand.len();
            let mut replay = restored(&state);
            for current in [&mut state, &mut replay] {
                settle(current);
                assert_eq!(
                    current.players[player.index()].hand.len(),
                    before + count as usize
                );
                assert_eq!(current.objects.get(spell).zone, Zone::Graveyard);
                for &object in &chosen {
                    assert!(engine::has_effective_keyword(
                        current,
                        object,
                        Keywords::INDESTRUCTIBLE
                    ));
                }
                for object in [plain, land, other] {
                    assert!(!engine::has_effective_keyword(
                        current,
                        object,
                        Keywords::INDESTRUCTIBLE
                    ));
                }
                cleanup(current);
                for &object in &chosen {
                    assert!(!engine::has_effective_keyword(
                        current,
                        object,
                        Keywords::INDESTRUCTIBLE
                    ));
                }
            }
            same(&state, &replay);
        }
    }
}

#[test]
fn resolution_snapshot_and_exact_incarnations_survive_counter_and_control_changes() {
    for player in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(player);
        let first = put(&mut state, player, "Llanowar Elves", Zone::Battlefield);
        let second = put(&mut state, player, "Llanowar Elves", Zone::Battlefield);
        state.objects.get_mut(first).counters.plus1_plus1 = 1;
        announce(&mut state, player);
        state.objects.get_mut(first).counters.plus1_plus1 = 0;
        state.objects.get_mut(second).counters.plus1_plus1 = 1;
        let before = state.players[player.index()].hand.len();
        let mut replay = restored(&state);
        for current in [&mut state, &mut replay] {
            settle(current);
            assert_eq!(current.players[player.index()].hand.len(), before + 1);
            assert!(!engine::has_effective_keyword(
                current,
                first,
                Keywords::INDESTRUCTIBLE
            ));
            current.objects.get_mut(first).counters.plus1_plus1 = 1;
            current.objects.get_mut(second).counters.plus1_plus1 = 0;
            current.players[player.index()]
                .battlefield
                .retain(|&id| id != second);
            current.players[player.opponent().index()]
                .battlefield
                .push(second);
            current.objects.get_mut(second).controller = player.opponent();
            assert!(engine::has_effective_keyword(
                current,
                second,
                Keywords::INDESTRUCTIBLE
            ));
            assert!(!engine::has_effective_keyword(
                current,
                first,
                Keywords::INDESTRUCTIBLE
            ));
            event::propose_and_commit(current, ProposedEvent::zone_change(second, Zone::Graveyard));
            event::propose_and_commit(
                current,
                ProposedEvent::zone_change(second, Zone::Battlefield),
            );
            assert!(!engine::has_effective_keyword(
                current,
                second,
                Keywords::INDESTRUCTIBLE
            ));
        }
        same(&state, &replay);
    }
}

#[test]
fn real_destroy_all_spares_chosen_creature_until_actual_cleanup() {
    for player in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(player);
        let chosen = put(&mut state, player, "Llanowar Elves", Zone::Battlefield);
        state.objects.get_mut(chosen).counters.plus1_plus1 = 1;
        let plain = put(&mut state, player, "Llanowar Elves", Zone::Battlefield);
        announce(&mut state, player);
        settle(&mut state);
        let wipe = put(&mut state, player, "Day of Judgment", Zone::Hand);
        state.players[player.index()].mana_pool[0] = 2;
        state.players[player.index()].mana_pool[5] = 2;
        priority(&mut state, player);
        cast(&mut state, wipe, None);
        let mut replay = restored(&state);
        for current in [&mut state, &mut replay] {
            settle(current);
            assert_eq!(current.objects.get(chosen).zone, Zone::Battlefield);
            assert_eq!(current.objects.get(plain).zone, Zone::Graveyard);
            cleanup(current);
            assert!(!engine::has_effective_keyword(
                current,
                chosen,
                Keywords::INDESTRUCTIBLE
            ));
            mtg_kernel::effect::execute(
                &mtg_kernel::effect::EffectOp::DestroyAllCreatures,
                &mtg_kernel::effect::ExecCtx::no_targets(wipe, player),
                current,
            );
            assert_eq!(current.objects.get(chosen).zone, Zone::Graveyard);
        }
        same(&state, &replay);
    }
}

#[test]
fn later_witness_suppresses_granted_ability_until_removed() {
    for player in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(player);
        let chosen = put(&mut state, player, "Llanowar Elves", Zone::Battlefield);
        state.objects.get_mut(chosen).counters.plus1_plus1 = 1;
        announce(&mut state, player);
        settle(&mut state);
        let witness = put(&mut state, player, "Witness Protection", Zone::Hand);
        state.players[player.index()].mana_pool[1] = 1;
        priority(&mut state, player);
        cast(&mut state, witness, Some(Target::Object(chosen)));
        let mut replay = restored(&state);
        for current in [&mut state, &mut replay] {
            settle(current);
            assert!(!engine::has_effective_keyword(
                current,
                chosen,
                Keywords::INDESTRUCTIBLE
            ));
            event::propose_and_commit(
                current,
                ProposedEvent::zone_change(witness, Zone::Graveyard),
            );
            assert!(engine::has_effective_keyword(
                current,
                chosen,
                Keywords::INDESTRUCTIBLE
            ));
            cleanup(current);
            assert!(!engine::has_effective_keyword(
                current,
                chosen,
                Keywords::INDESTRUCTIBLE
            ));
        }
        same(&state, &replay);
    }
}
