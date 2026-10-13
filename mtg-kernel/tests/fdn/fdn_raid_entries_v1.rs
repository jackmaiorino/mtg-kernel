//! Registered Raid entry fixtures; actual gameplay qualification remains pending.
#![cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
use mtg_kernel::card_def::{card_id_by_name, CardCapability, Keywords, Subtype, CARD_DEFS};
use mtg_kernel::engine::{self, Action, Decision};
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::ManaColor;
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Target, Zone};

fn ready(actor: PlayerId) -> GameState {
    let forest = card_id_by_name("Forest").unwrap();
    let mut state = GameState::new_from_libraries_with_starting_player_v1(
        &[forest; 40],
        &[forest; 40],
        |_| "Forest".into(),
        1116,
        actor,
    );
    state.step = Step::Main1;
    put(&mut state, actor, "Mountain", Zone::Hand);
    for player in [PlayerId::P0, PlayerId::P1] {
        put(&mut state, player, "Mountain", Zone::Battlefield);
        put(&mut state, player, "Lightning Bolt", Zone::Hand);
    }
    state
}
fn put(state: &mut GameState, actor: PlayerId, name: &str, zone: Zone) -> ObjectId {
    let card_def = card_id_by_name(name).unwrap();
    let id = state.objects.push(GameObject {
        card_def,
        name: name.into(),
        owner: actor,
        controller: actor,
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
        Zone::Hand => state.players[actor.index()].hand.push(id),
        Zone::Battlefield => state.players[actor.index()].battlefield.push(id),
        _ => panic!("fixture zone"),
    }
    id
}
fn next(state: &mut GameState) -> Decision {
    let d = engine::advance_until_decision(state);
    assert!(!matches!(d, Decision::Halted { .. }), "{d:?}");
    d
}
fn pass(state: &mut GameState) {
    assert!(matches!(next(state), Decision::CastSpellOrPass { .. }));
    engine::step(state, Action::Pass).unwrap();
}
fn priority(state: &mut GameState, actor: PlayerId) {
    for _ in 0..4 {
        if matches!(next(state), Decision::CastSpellOrPass { player, .. } if player == actor) {
            return;
        }
        pass(state);
    }
    panic!("priority was not available to {actor:?}");
}
fn cast(state: &mut GameState, actor: PlayerId, card: ObjectId) {
    state.players[actor.index()].mana_pool[ManaColor::U.pool_index()] = 20;
    state.players[actor.index()].mana_pool[ManaColor::R.pool_index()] = 20;
    priority(state, actor);
    engine::step(state, Action::CastSpell(card)).unwrap();
    pass(state);
    pass(state);
    assert!(matches!(next(state), Decision::CastSpellOrPass { .. }));
    assert_eq!(state.objects.get(card).zone, Zone::Battlefield);
}
fn cast_witness(state: &mut GameState, actor: PlayerId, host: ObjectId) -> ObjectId {
    let aura = put(state, actor, "Witness Protection", Zone::Hand);
    state.players[actor.index()].mana_pool[ManaColor::U.pool_index()] = 20;
    priority(state, actor);
    engine::step(state, Action::CastSpell(aura)).unwrap();
    assert!(
        matches!(next(state), Decision::ChooseTargets { legal_targets, .. } if legal_targets.contains(&Target::Object(host)))
    );
    engine::step(state, Action::ChooseTarget(Target::Object(host))).unwrap();
    for _ in 0..16 {
        let boundary = next(state);
        if state.objects.get(aura).zone == Zone::Battlefield {
            return aura;
        }
        // Guttersnipe's real cast trigger resolves before this Aura.
        assert!(matches!(boundary, Decision::CastSpellOrPass { .. }));
        engine::step(state, Action::Pass).unwrap();
    }
    panic!("Witness did not resolve before the queued Raid ability");
}
fn restored(state: &GameState) -> GameState {
    serde_json::from_slice(&serde_json::to_vec(state).unwrap()).unwrap()
}
fn main2_after_declaration(state: &mut GameState, actor: PlayerId, nonempty: bool) {
    let attacker = put(state, actor, "Guttersnipe", Zone::Battlefield);
    state.step = Step::DeclareAttackers;
    engine::step(
        state,
        Action::DeclareAttackers(if nonempty { vec![attacker] } else { vec![] }),
    )
    .unwrap();
    for _ in 0..40 {
        match next(state) {
            Decision::CastSpellOrPass { player, .. }
                if state.step == Step::Main2 && player == actor =>
            {
                return
            }
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            Decision::DeclareBlockers { .. } => {
                engine::step(state, Action::DeclareBlockers(vec![])).unwrap()
            }
            d => panic!("unexpected combat decision {d:?}"),
        }
    }
    panic!("combat did not reach Main2");
}
#[test]
fn metadata_and_printed_source_rules_match_the_pinned_reference() {
    let sky = &CARD_DEFS[card_id_by_name("Skyship Buccaneer").unwrap() as usize];
    let board = &CARD_DEFS[card_id_by_name("Goblin Boarders").unwrap() as usize];
    assert_eq!(card_id_by_name("Skyship Buccaneer"), Some(390));
    assert_eq!(card_id_by_name("Goblin Boarders"), Some(391));
    assert_eq!(sky.capability, CardCapability::Full);
    assert_eq!(board.capability, CardCapability::Full);
    assert_eq!(
        (sky.mana_value, sky.power, sky.toughness),
        (5, Some(4), Some(3))
    );
    assert_eq!(
        (board.mana_value, board.power, board.toughness),
        (3, Some(3), Some(2))
    );
    assert!(sky.keywords.has(Keywords::FLYING));
    assert!(sky.subtypes.contains(&Subtype::Human) && sky.subtypes.contains(&Subtype::Pirate));
    assert!(board.subtypes.contains(&Subtype::Goblin) && board.subtypes.contains(&Subtype::Pirate));
}
#[test]
fn boarders_counter_is_an_entry_replacement_for_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for mode in 0..4 {
            let history = [None, Some(false), Some(true), Some(true)][mode];
            let mut state = ready(actor);
            if let Some(nonempty) = history {
                main2_after_declaration(&mut state, actor, nonempty);
            }
            if mode == 3 {
                // A real later turn clears Raid and refreshes all occurrence anchors.
                super::fixture_turn_v1::reach(&mut state, actor, Step::Main1);
                assert!(!state.player_attacked_this_turn_v1(actor));
            }
            let enabled = mode == 2;
            let board = put(&mut state, actor, "Goblin Boarders", Zone::Hand);
            cast(&mut state, actor, board);
            assert_eq!(
                state.objects.get(board).counters.plus1_plus1,
                i32::from(enabled)
            );
            assert!(
                state.stack.is_empty(),
                "entry replacement creates no stack ability"
            );
            assert_eq!(
                engine::effective_power(&state, board),
                if enabled { 4 } else { 3 }
            );
        }
    }
}
#[test]
fn skyship_captures_only_after_a_nonempty_declaration_and_draws_on_resolution() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for mode in 0..4 {
            let history = [None, Some(false), Some(true), Some(true)][mode];
            let mut state = ready(actor);
            if let Some(nonempty) = history {
                main2_after_declaration(&mut state, actor, nonempty);
            }
            if mode == 3 {
                // A real later turn clears Raid and refreshes all occurrence anchors.
                super::fixture_turn_v1::reach(&mut state, actor, Step::Main1);
                assert!(!state.player_attacked_this_turn_v1(actor));
            }
            let enabled = mode == 2;
            let sky = put(&mut state, actor, "Skyship Buccaneer", Zone::Hand);
            let library = state.players[actor.index()].library.len();
            cast(&mut state, actor, sky);
            assert_eq!(state.players[actor.index()].library.len(), library);
            let hand = state.players[actor.index()].hand.len();
            if enabled {
                assert_eq!(state.stack.len(), 1);
                pass(&mut state);
                pass(&mut state);
                assert!(matches!(next(&mut state), Decision::CastSpellOrPass { .. }));
                assert_eq!(state.players[actor.index()].library.len(), library - 1);
                assert_eq!(state.players[actor.index()].hand.len(), hand + 1);
            } else {
                assert!(state.stack.is_empty());
            }
        }
    }
}
#[test]
fn another_players_declaration_does_not_enable_flash_raid_entries() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for name in ["Skyship Buccaneer", "Goblin Boarders"] {
            let mut state = ready(actor);
            let other = actor.opponent();
            put(&mut state, other, "High Fae Trickster", Zone::Battlefield);
            main2_after_declaration(&mut state, actor, true);
            let card = put(&mut state, other, name, Zone::Hand);
            let before = state.players[other.index()].library.len();
            cast(&mut state, other, card);
            assert_eq!(state.objects.get(card).counters.plus1_plus1, 0);
            assert_eq!(state.players[other.index()].library.len(), before);
            assert!(state.stack.is_empty());
        }
    }
}
#[test]
fn pending_skyship_draw_restores_captured_controller_and_intervening_condition() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for mode in 0..5 {
            let mut state = ready(actor);
            if mode == 2 {
                put(&mut state, actor, "High Fae Trickster", Zone::Battlefield);
            }
            main2_after_declaration(&mut state, actor, true);
            let sky = put(&mut state, actor, "Skyship Buccaneer", Zone::Hand);
            cast(&mut state, actor, sky);
            assert_eq!(state.stack.len(), 1);
            let library = state.players[actor.index()].library.len();
            let other_library = state.players[actor.opponent().index()].library.len();
            match mode {
                0 => event::propose_and_commit(
                    &mut state,
                    ProposedEvent::zone_change(sky, Zone::Graveyard),
                ),
                1 => {
                    state.players[actor.index()]
                        .battlefield
                        .retain(|&id| id != sky);
                    state.players[actor.opponent().index()]
                        .battlefield
                        .push(sky);
                    state.objects.get_mut(sky).controller = actor.opponent();
                }
                2 => {
                    cast_witness(&mut state, actor, sky);
                    assert!(!engine::has_effective_keyword(
                        &state,
                        sky,
                        Keywords::FLYING
                    ));
                    assert_eq!(state.stack.len(), 1);
                }
                3 => state.attack_declaration_turn_v1 = None,
                _ => state.turn += 1,
            }
            let hand = state.players[actor.index()].hand.len();
            let mut replay = restored(&state);
            for current in [&mut state, &mut replay] {
                if mode == 4 {
                    // A forged round cannot be resumed as a legitimate new turn.
                    super::fixture_turn_v1::stale_turn_refuses_readonly(current);
                    assert_eq!(current.players[actor.index()].hand.len(), hand);
                    assert_eq!(current.players[actor.index()].library.len(), library);
                    assert_eq!(
                        current.players[actor.opponent().index()].library.len(),
                        other_library
                    );
                    continue;
                }
                pass(current);
                pass(current);
                assert!(matches!(next(current), Decision::CastSpellOrPass { .. }));
                let draw = usize::from(mode < 3);
                assert_eq!(current.players[actor.index()].hand.len(), hand + draw);
                assert_eq!(current.players[actor.index()].library.len(), library - draw);
                assert_eq!(
                    current.players[actor.opponent().index()].library.len(),
                    other_library
                );
            }
            assert_eq!(
                state.diagnostic_state_hash(),
                replay.diagnostic_state_hash()
            );
        }
    }
}
#[test]
fn boarders_new_incarnation_gets_one_fresh_entry_counter() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        main2_after_declaration(&mut state, actor, true);
        let board = put(&mut state, actor, "Goblin Boarders", Zone::Hand);
        cast(&mut state, actor, board);
        assert_eq!(state.objects.get(board).counters.plus1_plus1, 1);
        let generation = state.objects.get(board).zone_change_count;
        event::propose_and_commit(&mut state, ProposedEvent::zone_change(board, Zone::Hand));
        cast(&mut state, actor, board);
        assert!(state.objects.get(board).zone_change_count > generation);
        assert_eq!(state.objects.get(board).counters.plus1_plus1, 1);
        let aura = cast_witness(&mut state, actor, board);
        assert_eq!(engine::effective_power(&state, board), 2);
        assert_eq!(state.objects.get(board).counters.plus1_plus1, 1);
        let mut replay = restored(&state);
        for current in [&mut state, &mut replay] {
            event::propose_and_commit(current, ProposedEvent::zone_change(aura, Zone::Graveyard));
            assert_eq!(engine::effective_power(current, board), 4);
            assert_eq!(current.objects.get(board).counters.plus1_plus1, 1);
        }
        assert_eq!(
            state.diagnostic_state_hash(),
            replay.diagnostic_state_hash()
        );
    }
}
