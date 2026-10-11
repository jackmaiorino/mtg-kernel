//! Unregistered Raid source fixtures; actual card gameplay remains unqualified.
#![cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
use mtg_kernel::card_def::{card_id_by_name, CardCapability, Keywords, Subtype, CARD_DEFS};
use mtg_kernel::engine::{self, Action, Decision};
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::ManaColor;
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Zone};

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
    priority(state, actor);
    state.players[actor.index()].mana_pool[ManaColor::U.pool_index()] = 20;
    state.players[actor.index()].mana_pool[ManaColor::R.pool_index()] = 20;
    engine::step(state, Action::CastSpell(card)).unwrap();
    pass(state);
    pass(state);
    assert_eq!(state.objects.get(card).zone, Zone::Battlefield);
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
        for history in [None, Some(false), Some(true)] {
            let mut state = ready(actor);
            if let Some(nonempty) = history {
                main2_after_declaration(&mut state, actor, nonempty);
            }
            let board = put(&mut state, actor, "Goblin Boarders", Zone::Hand);
            cast(&mut state, actor, board);
            assert_eq!(
                state.objects.get(board).counters.plus1_plus1,
                i32::from(history == Some(true))
            );
            assert!(
                state.stack.is_empty(),
                "entry replacement creates no stack ability"
            );
            assert_eq!(
                engine::effective_power(&state, board),
                if history == Some(true) { 4 } else { 3 }
            );
        }
    }
}
#[test]
fn skyship_captures_only_after_a_nonempty_declaration_and_draws_on_resolution() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for history in [None, Some(false), Some(true)] {
            let mut state = ready(actor);
            if let Some(nonempty) = history {
                main2_after_declaration(&mut state, actor, nonempty);
            }
            let sky = put(&mut state, actor, "Skyship Buccaneer", Zone::Hand);
            let library = state.players[actor.index()].library.len();
            cast(&mut state, actor, sky);
            assert_eq!(state.players[actor.index()].library.len(), library);
            if history == Some(true) {
                assert_eq!(state.stack.len(), 1);
                pass(&mut state);
                pass(&mut state);
                assert_eq!(state.players[actor.index()].library.len(), library - 1);
                assert_eq!(state.players[actor.index()].hand.len(), 1);
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
fn pending_skyship_draw_restores_and_survives_source_departure() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        main2_after_declaration(&mut state, actor, true);
        let sky = put(&mut state, actor, "Skyship Buccaneer", Zone::Hand);
        cast(&mut state, actor, sky);
        assert_eq!(state.stack.len(), 1);
        event::propose_and_commit(&mut state, ProposedEvent::zone_change(sky, Zone::Graveyard));
        let mut replay: GameState =
            serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
        for current in [&mut state, &mut replay] {
            pass(current);
            pass(current);
            assert_eq!(current.players[actor.index()].hand.len(), 1);
        }
        assert_eq!(
            state.diagnostic_state_hash(),
            replay.diagnostic_state_hash()
        );
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
    }
}
