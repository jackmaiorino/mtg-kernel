//! The outside-the-game zone (stage S3): wish and learn effects reveal cards
//! a player owns from outside the game and put them into their hand, the
//! zone stays private to its owner, and a custom-deck session receives it at
//! reset.
#![cfg(feature = "eternal-fixtures")]

use mtg_kernel::card_def::{card_id_by_name, CARD_DEFS};
use mtg_kernel::engine::{self, Action, Decision};
use mtg_kernel::eternal_outside_game_v1::{
    decision_options, outside_the_game, set_outside_the_game, OutsideGameOptionV1,
};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::limited_session_v1::LimitedJsonlServerV1;
use mtg_kernel::mana::ManaColor;
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Target, Zone};
use serde_json::{json, Value};

const P0: PlayerId = PlayerId::P0;
const P1: PlayerId = PlayerId::P1;

fn id(name: &str) -> u16 {
    card_id_by_name(name).unwrap_or_else(|| panic!("{name}"))
}

/// A first main phase with P0 active and holding priority, both libraries
/// twenty Islands.
fn islands() -> GameState {
    let ids = vec![id("Island"); 20];
    let mut state = GameState::new_from_libraries(
        &ids,
        &ids,
        |id| CARD_DEFS[id as usize].object_name.into(),
        0x5333_0000_0001,
    );
    state.active_player = P0;
    state.priority_player = P0;
    state.step = Step::Main1;
    state
}

fn put(state: &mut GameState, player: PlayerId, name: &str, zone: Zone) -> ObjectId {
    let card_def = id(name);
    let object = state.objects.push(GameObject {
        card_def,
        name: CARD_DEFS[card_def as usize].object_name.into(),
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
    let seat = &mut state.players[player.index()];
    match zone {
        Zone::Hand => seat.hand.push(object),
        Zone::Battlefield => seat.battlefield.push(object),
        Zone::Exile => state.exile.push(object),
        _ => panic!("helper zone"),
    }
    object
}

fn pool(state: &mut GameState, player: PlayerId, colored: &[ManaColor], colorless: u8) {
    let pool = &mut state.players[player.index()].mana_pool;
    for color in colored {
        pool[color.pool_index()] += 1;
    }
    pool[ManaColor::C.pool_index()] += colorless;
}

fn next(state: &mut GameState) -> Decision {
    let decision = engine::advance_until_decision(state);
    assert!(!matches!(decision, Decision::Halted { .. }), "{decision:?}");
    decision
}

fn act(state: &mut GameState, action: Action) {
    engine::step(state, action.clone()).unwrap_or_else(|error| panic!("{action:?}: {error}"));
}

/// Passes priority and orders triggers until the stack and pending triggers
/// are empty, returning the first other decision.
fn settle(state: &mut GameState) -> Option<Decision> {
    for _ in 0..200 {
        match next(state) {
            Decision::OrderTriggers { pending, .. } => {
                act(state, Action::OrderTriggers((0..pending.len()).collect()))
            }
            Decision::CastSpellOrPass { .. }
                if state.stack.is_empty() && state.engine.pending_triggers.is_empty() =>
            {
                return None
            }
            Decision::CastSpellOrPass { .. } => act(state, Action::Pass),
            other => return Some(other),
        }
    }
    panic!("did not settle");
}

fn settled(state: &mut GameState) {
    if let Some(other) = settle(state) {
        panic!("unexpected choice: {other:?}");
    }
}

fn cast(state: &mut GameState, spell: ObjectId) {
    act(state, Action::CastSpell(spell));
}

/// The next decision is an effect option for `player` with `count` options;
/// answers `index`.
fn choose_option(state: &mut GameState, player: PlayerId, count: u16, index: u16) {
    match settle(state) {
        Some(Decision::ChooseEffectOption {
            player: chooser,
            option_count,
            ..
        }) => {
            assert_eq!(chooser, player, "chooser");
            assert_eq!(option_count, count, "option count");
            act(state, Action::ChooseEffectOption(index));
        }
        other => panic!("expected an effect option, got {other:?}"),
    }
}

/// The card ids in `player`'s hand.
fn hand_cards(state: &GameState, player: PlayerId) -> Vec<u16> {
    state.players[player.index()]
        .hand
        .iter()
        .map(|&object| state.objects.get(object).card_def)
        .collect()
}

/// The hand object of `player` with card `name`, which the opponent knows.
fn known_hand_card(state: &GameState, player: PlayerId, name: &str) -> ObjectId {
    let object = *state.players[player.index()]
        .hand
        .iter()
        .find(|&&object| state.objects.get(object).card_def == id(name))
        .unwrap_or_else(|| panic!("{name} in hand"));
    assert!(
        state
            .known_hand_cards(player.opponent(), player)
            .iter()
            .any(|entry| entry.object == object),
        "the opponent knows the revealed {name}"
    );
    object
}

fn sorted(names: &[&str]) -> Vec<u16> {
    let mut ids: Vec<u16> = names.iter().map(|name| id(name)).collect();
    ids.sort_unstable();
    ids
}

/// The option index of `name` among the distinct `eligible` cards in id
/// order, after `skip` leading options.
fn option_of(eligible: &[&str], name: &str, skip: u16) -> u16 {
    let ids = sorted(eligible);
    skip + ids.iter().position(|&card| card == id(name)).unwrap() as u16
}

#[test]
fn burning_wish_puts_a_sorcery_from_outside_the_game_into_its_owners_hand() {
    let mut state = islands();
    set_outside_the_game(
        &mut state,
        [
            vec![id("Gamble"), id("Lightning Bolt"), id("Echo of Eons")],
            vec![id("Gamble")],
        ],
    );
    let wish = put(&mut state, P0, "Burning Wish", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 1);
    cast(&mut state, wish);
    // Decline, then the sorceries in card id order; Lightning Bolt is an
    // instant and P1's Gamble is not P0's.
    let index = option_of(&["Gamble", "Echo of Eons"], "Gamble", 1);
    choose_option(&mut state, P0, 3, index);
    settled(&mut state);
    known_hand_card(&state, P0, "Gamble");
    assert_eq!(
        outside_the_game(&state, P0),
        sorted(&["Lightning Bolt", "Echo of Eons"]).as_slice()
    );
    assert_eq!(outside_the_game(&state, P1), &[id("Gamble")]);
    assert_eq!(state.objects.get(wish).zone, Zone::Exile);

    // Declining keeps the zone and still exiles the wish.
    let mut state = islands();
    set_outside_the_game(&mut state, [vec![id("Gamble")], Vec::new()]);
    let wish = put(&mut state, P0, "Burning Wish", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 1);
    cast(&mut state, wish);
    choose_option(&mut state, P0, 2, 0);
    settled(&mut state);
    assert!(hand_cards(&state, P0).is_empty());
    assert_eq!(outside_the_game(&state, P0), &[id("Gamble")]);
    assert_eq!(state.objects.get(wish).zone, Zone::Exile);

    // No sorcery outside the game: no choice at all, as in best-of-one.
    let mut state = islands();
    set_outside_the_game(&mut state, [vec![id("Lightning Bolt")], Vec::new()]);
    let wish = put(&mut state, P0, "Burning Wish", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 1);
    cast(&mut state, wish);
    settled(&mut state);
    assert_eq!(state.objects.get(wish).zone, Zone::Exile);
}

#[test]
fn karns_minus_two_offers_outside_artifacts_beside_its_exile_choice() {
    let ready = |outside: Vec<u16>| {
        let mut state = islands();
        set_outside_the_game(&mut state, [outside, Vec::new()]);
        let karn = put(&mut state, P0, "Karn, the Great Creator", Zone::Hand);
        let exiled = put(&mut state, P0, "Mana Vault", Zone::Exile);
        pool(&mut state, P0, &[], 4);
        cast(&mut state, karn);
        settled(&mut state);
        act(&mut state, Action::ActivateAbility(karn, 1));
        (state, karn, exiled)
    };
    // Option 0 is the exile choice; then Sol Ring (Lightning Bolt is not an
    // artifact).
    let (mut state, karn, exiled) = ready(vec![id("Sol Ring"), id("Lightning Bolt")]);
    choose_option(&mut state, P0, 2, 1);
    settled(&mut state);
    known_hand_card(&state, P0, "Sol Ring");
    assert_eq!(outside_the_game(&state, P0), &[id("Lightning Bolt")]);
    assert_eq!(state.objects.get(exiled).zone, Zone::Exile);
    assert_eq!(mtg_kernel::planeswalker_v1::loyalty(&state, karn), Some(3));

    let (mut state, _, exiled) = ready(vec![id("Sol Ring")]);
    choose_option(&mut state, P0, 2, 0);
    match settle(&mut state) {
        Some(Decision::ChooseEffectTargets { legal_targets, .. }) => {
            assert_eq!(legal_targets, vec![Target::Object(exiled)]);
            act(
                &mut state,
                Action::ChooseEffectTarget(Target::Object(exiled)),
            );
        }
        other => panic!("expected the exile choice, got {other:?}"),
    }
    while let Some(decision) = settle(&mut state) {
        match decision {
            Decision::ChooseEffectTargets { .. } => act(&mut state, Action::FinishEffectSelection),
            other => panic!("unexpected choice: {other:?}"),
        }
    }
    assert_eq!(state.objects.get(exiled).zone, Zone::Hand);
    assert_eq!(outside_the_game(&state, P0), &[id("Sol Ring")]);
}

#[test]
fn learn_reveals_a_lesson_or_discards_to_draw() {
    // An empty hand: decline or the Lesson.
    let mut state = islands();
    set_outside_the_game(
        &mut state,
        [
            vec![id("Boomerang Basics"), id("Lightning Bolt")],
            Vec::new(),
        ],
    );
    let class = put(&mut state, P0, "First Day of Class", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 1);
    cast(&mut state, class);
    choose_option(&mut state, P0, 2, 1);
    settled(&mut state);
    known_hand_card(&state, P0, "Boomerang Basics");
    assert_eq!(outside_the_game(&state, P0), &[id("Lightning Bolt")]);

    // A card in hand adds the discard option before the Lesson.
    let mut state = islands();
    set_outside_the_game(&mut state, [vec![id("Boomerang Basics")], Vec::new()]);
    let class = put(&mut state, P0, "First Day of Class", Zone::Hand);
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 1);
    cast(&mut state, class);
    choose_option(&mut state, P0, 3, 1);
    match settle(&mut state) {
        Some(Decision::ChooseEffectTargets { legal_targets, .. }) => {
            assert_eq!(legal_targets, vec![Target::Object(bolt)]);
            act(&mut state, Action::ChooseEffectTarget(Target::Object(bolt)));
        }
        other => panic!("expected the card to discard, got {other:?}"),
    }
    settled(&mut state);
    assert_eq!(state.objects.get(bolt).zone, Zone::Graveyard);
    assert_eq!(hand_cards(&state, P0), vec![id("Island")], "drew a card");
    assert_eq!(outside_the_game(&state, P0), &[id("Boomerang Basics")]);
}

#[test]
fn learn_uses_the_madness_discard_path() {
    let mut state = islands();
    let class = put(&mut state, P0, "First Day of Class", Zone::Hand);
    let temper = put(&mut state, P0, "Fiery Temper", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 1);
    cast(&mut state, class);
    choose_option(&mut state, P0, 2, 1);
    assert!(matches!(
        settle(&mut state),
        Some(Decision::ChooseEffectTargets { .. })
    ));
    act(
        &mut state,
        Action::ChooseEffectTarget(Target::Object(temper)),
    );
    assert!(matches!(
        settle(&mut state),
        Some(Decision::ChooseMadnessCast { player: P0, card }) if card == temper
    ));
    assert_eq!(state.objects.get(temper).zone, Zone::Exile);
    assert_eq!(hand_cards(&state, P0), vec![id("Island")]);
    act(&mut state, Action::ChooseMadnessCast(false));
    settled(&mut state);
    assert_eq!(state.objects.get(temper).zone, Zone::Graveyard);
}

#[test]
fn extrapolate_reveals_two_different_cards_and_an_opponent_chooses_one() {
    let cards = ["Lightning Bolt", "Counterspell", "Gamble"];
    let mut state = islands();
    set_outside_the_game(
        &mut state,
        [
            vec![
                id("Lightning Bolt"),
                id("Counterspell"),
                id("Gamble"),
                id("Gamble"),
            ],
            Vec::new(),
        ],
    );
    let extrapolate = put(&mut state, P0, "Extrapolate the Impossible", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 1);
    cast(&mut state, extrapolate);
    assert!(matches!(
        settle(&mut state),
        Some(Decision::ChooseEffectOption { player: P0, .. })
    ));
    assert!(decision_options(&state, P0.into()).is_some());
    assert!(decision_options(&state, P1.into()).is_none());
    // P0 reveals Gamble, then Counterspell (of the other two names); P1
    // chooses between them, in card id order.
    choose_option(&mut state, P0, 4, option_of(&cards, "Gamble", 1));
    let others = ["Lightning Bolt", "Counterspell"];
    choose_option(&mut state, P0, 2, option_of(&others, "Counterspell", 0));
    let revealed = ["Gamble", "Counterspell"];
    assert!(matches!(
        settle(&mut state),
        Some(Decision::ChooseEffectOption { player: P1, .. })
    ));
    assert_eq!(
        decision_options(&state, P1.into()),
        Some(
            sorted(&revealed)
                .into_iter()
                .map(|card_def| OutsideGameOptionV1::Card { card_def })
                .collect()
        )
    );
    choose_option(&mut state, P1, 2, option_of(&revealed, "Counterspell", 0));
    settled(&mut state);
    known_hand_card(&state, P0, "Counterspell");
    assert_eq!(
        outside_the_game(&state, P0),
        sorted(&["Lightning Bolt", "Gamble", "Gamble"]).as_slice()
    );

    // Fewer than two different names: nothing to reveal.
    let mut state = islands();
    set_outside_the_game(&mut state, [vec![id("Gamble"), id("Gamble")], Vec::new()]);
    let extrapolate = put(&mut state, P0, "Extrapolate the Impossible", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 1);
    cast(&mut state, extrapolate);
    settled(&mut state);
    assert!(hand_cards(&state, P0).is_empty());
}

fn reset(server: &mut LimitedJsonlServerV1, schema: u32, outside: Option<Value>) -> Value {
    let mut request = json!({"schema_version":schema,"request_type":"reset","request_id":"reset",
        "decks":[{"cards":[{"name":"Island","count":40}]},{"cards":[{"name":"Mountain","count":40}]}],
        "episode_id":1,"env_seed":7,"max_physical_decisions":4096,"max_policy_steps":65536});
    if let Some(outside) = outside {
        request["outside_the_game"] = outside;
    }
    serde_json::from_str(&server.handle_line(&request.to_string())).unwrap()
}

#[test]
fn session_reset_takes_each_seats_cards_outside_the_game() {
    let mut server = LimitedJsonlServerV1::new_with_london_mulligans_v1();
    let plain = reset(&mut server, 4, None);
    assert_eq!(plain["response_type"], "decision", "{plain}");
    let with = reset(
        &mut LimitedJsonlServerV1::new_with_london_mulligans_v1(),
        4,
        Some(json!([[{"name":"Gamble","count":1},{"name":"Burning Wish","count":2}], []])),
    );
    assert_eq!(with["response_type"], "decision", "{with}");
    // An unopened sideboard changes neither the public deck identity nor the menu.
    assert_eq!(with["decision"]["deck_ids"], plain["decision"]["deck_ids"]);
    assert_eq!(
        with["decision"]["deck_hashes"],
        plain["decision"]["deck_hashes"]
    );
    assert!(with.get("outside_game_options").is_none());
    let empty = reset(
        &mut LimitedJsonlServerV1::new_with_london_mulligans_v1(),
        4,
        Some(json!([[], []])),
    );
    assert_eq!(empty["decision"]["deck_ids"], plain["decision"]["deck_ids"]);

    // Tokens, unknown names and the pre-London schemas are refused.
    for outside in [
        json!([[{"name":"Treasure Token","count":1}], []]),
        json!([[], [{"name":"No Such Card","count":1}]]),
        json!([[{"name":"Gamble","count":0}], []]),
        json!([[{"name":"Gamble","count":u32::MAX}], []]),
    ] {
        let reply = reset(
            &mut LimitedJsonlServerV1::new_with_london_mulligans_v1(),
            4,
            Some(outside),
        );
        assert_eq!(reply["error"]["code"], "unsupported_deck", "{reply}");
    }
    let reply = reset(
        &mut LimitedJsonlServerV1::new_with_engine_priority_v1(),
        2,
        Some(json!([[{"name":"Gamble","count":1}], []])),
    );
    assert_eq!(reply["error"]["code"], "unsupported_deck", "{reply}");
}
