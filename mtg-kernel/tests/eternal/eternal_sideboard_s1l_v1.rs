//! Legacy sideboard wave S1l: native rules tests for Triumph of Saint
//! Katherine and for the miracle costs Lavinia, Azorius Renegade counts as
//! mana spent, and the deck games that play both cards through public
//! sessions.
#![cfg(feature = "eternal-fixtures")]

use crate::eternal_catalog_v1::{play_observed, stress_deck, Policy};
use mtg_kernel::card_def::{
    card_id_by_name, CardCapability, CardDef, CardType, Keywords, Subtype, CARD_DEFS,
};
use mtg_kernel::engine::{self, Action, Decision};
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::ManaColor;
use mtg_kernel::state::{
    GameObject, GameState, LibraryKnowledgeEntry, ObjectStateV4, StackItemKind, Step, Target, Zone,
};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

const P0: PlayerId = PlayerId::P0;
const P1: PlayerId = PlayerId::P1;

const TRIUMPH: &str = "Triumph of Saint Katherine";
const LAVINIA: &str = "Lavinia, Azorius Renegade";

fn def(name: &str) -> &'static CardDef {
    &CARD_DEFS[id(name) as usize]
}

fn id(name: &str) -> u16 {
    card_id_by_name(name).unwrap_or_else(|| panic!("{name}"))
}

/// A first main phase with P0 active and holding priority. Each library
/// holds `library` in order, top first.
fn game_with_seed(library: &[&str], seed: u64) -> GameState {
    let ids: Vec<u16> = library.iter().map(|name| id(name)).collect();
    let mut state = GameState::new_from_libraries(
        &ids,
        &ids,
        |id| CARD_DEFS[id as usize].object_name.into(),
        seed,
    );
    state.active_player = P0;
    state.priority_player = P0;
    state.step = Step::Main1;
    state
}

fn game(library: &[&str]) -> GameState {
    game_with_seed(library, 0x5331_4c00_0001)
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
        Zone::Graveyard => seat.graveyard.push(object),
        Zone::Exile => state.exile.push(object),
        Zone::Library => seat.library.insert(0, object),
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

fn set_active(state: &mut GameState, active: PlayerId) {
    state.active_player = active;
    state.priority_player = active;
    state.step = Step::Main1;
}

fn zone(state: &GameState, object: ObjectId) -> Zone {
    state.objects.get(object).zone
}

fn next(state: &mut GameState) -> Decision {
    let decision = engine::advance_until_decision(state);
    assert!(
        !matches!(decision, Decision::Halted { .. }),
        "{decision:?}\nstack: {:#?}\npending triggers: {:#?}",
        state.stack,
        state.engine.pending_triggers
    );
    decision
}

fn act(state: &mut GameState, action: Action) {
    engine::step(state, action.clone()).unwrap_or_else(|error| panic!("{action:?}: {error}"));
}

/// Passes priority and orders triggers until the stack and pending triggers
/// are empty, returning the first other decision if one interrupts.
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

/// The first legal answer to any choice.
fn first_answer(decision: Decision) -> Action {
    match decision {
        Decision::ChooseTargets { legal_targets, .. } => Action::ChooseTarget(legal_targets[0]),
        Decision::ChooseEffectTargets {
            can_finish: true, ..
        } => Action::FinishEffectSelection,
        Decision::ChooseEffectTargets { legal_targets, .. } => {
            Action::ChooseEffectTarget(legal_targets[0])
        }
        Decision::ChooseEffectOption { .. } => Action::ChooseEffectOption(0),
        Decision::ChooseEffectBoolean { .. } => Action::ChooseEffectBoolean(false),
        other => panic!("unexpected choice: {other:?}"),
    }
}

fn settle_with(state: &mut GameState, mut answer: impl FnMut(&GameState, Decision) -> Action) {
    while let Some(decision) = settle(state) {
        let action = answer(state, decision);
        act(state, action);
    }
}

fn castable(state: &mut GameState, spell: ObjectId) -> bool {
    match next(state) {
        Decision::CastSpellOrPass {
            castable_spells, ..
        } => castable_spells.contains(&spell),
        other => panic!("expected priority, got {other:?}"),
    }
}

/// Casts `spell`, answering each target prompt from `targets` in order.
fn cast(state: &mut GameState, spell: ObjectId, targets: &[Target]) {
    assert!(
        castable(state, spell),
        "{} not castable",
        state.objects.get(spell).name
    );
    act(state, Action::CastSpell(spell));
    for &target in targets {
        match next(state) {
            Decision::ChooseTargets { legal_targets, .. } => {
                assert!(legal_targets.contains(&target), "{target:?}");
                act(state, Action::ChooseTarget(target));
            }
            other => panic!("expected targets, got {other:?}"),
        }
    }
}

fn hand_contains(state: &GameState, player: PlayerId, object: ObjectId) -> bool {
    state.players[player.index()].hand.contains(&object)
}

/// Moves `object` to its owner's graveyard, as if destroyed.
fn destroy(state: &mut GameState, object: ObjectId) {
    event::propose_and_commit(state, ProposedEvent::zone_change(object, Zone::Graveyard));
}

/// Whether a triggered ability of `source` is on the stack.
fn triggered(state: &GameState, source: ObjectId) -> bool {
    state
        .stack
        .iter()
        .any(|item| item.source == source && item.kind == StackItemKind::TriggeredAbility)
}

/// Puts `name` on top of `owner`'s library and draws it, as `owner`'s first
/// card this turn. The draw is committed outside the engine's loop, so one
/// advance collects it, which notes a miracle card's reveal for the next.
fn draw_first(state: &mut GameState, owner: PlayerId, name: &str) -> ObjectId {
    let card = put(state, owner, name, Zone::Library);
    event::propose_and_commit(state, ProposedEvent::draw(owner));
    assert!(hand_contains(state, owner, card));
    next(state);
    card
}

/// Reveals the miracle card `card` its owner just drew, then answers its
/// trigger: takes the cast when offered and casts `card`. Returns whether
/// the cast was offered; any other choice takes its first answer.
fn miracle(state: &mut GameState, owner: PlayerId, card: ObjectId) -> bool {
    let decision = next(state);
    assert!(
        matches!(
            decision,
            Decision::ChooseEffectOption {
                player,
                source,
                option_count: 2,
            } if player == owner && source == card
        ),
        "the reveal: {decision:?}"
    );
    act(state, Action::ChooseEffectOption(1));
    assert!(triggered(state, card) || !state.engine.pending_triggers.is_empty());
    let mut offered = false;
    settle_with(state, |_, decision| match decision {
        Decision::ChooseEffectOption {
            player,
            option_count: 2,
            ..
        } if player == owner && !offered => {
            offered = true;
            Action::ChooseEffectOption(0)
        }
        Decision::ChooseEffectTargets { legal_targets, .. }
            if legal_targets == vec![Target::Object(card)] =>
        {
            Action::ChooseEffectTarget(Target::Object(card))
        }
        other => first_answer(other),
    });
    offered
}

// ---- Characteristics -----------------------------------------------------------

#[test]
fn triumph_and_lavinia_are_full_with_their_printed_characteristics() {
    let triumph = def(TRIUMPH);
    assert_eq!(triumph.capability, CardCapability::Full);
    assert!(triumph.is_executable() && !triumph.is_token);
    assert_eq!(
        (
            triumph.cost.generic,
            triumph.cost.pips.len(),
            triumph.cost.x_count
        ),
        (4, 1, 0)
    );
    assert_eq!(triumph.mana_value, 5);
    assert_eq!(triumph.types, &[CardType::Creature]);
    assert_eq!(triumph.subtypes, &[Subtype::Human, Subtype::Warrior]);
    assert_eq!((triumph.power, triumph.toughness), (Some(5), Some(5)));
    assert!(triumph.keywords.has(Keywords::LIFELINK));
    assert!(triumph.activated_abilities.is_empty());
    assert_eq!(def(LAVINIA).capability, CardCapability::Full);
}

// ---- Praesidium Protectiva ----------------------------------------------------------

/// Triumph dies with ten cards in its owner's library: it and the top six
/// cards become the top seven, in a random order, and the rest keep their
/// order. The owner's knowledge of its top card is dropped, and of a deeper
/// card shifts one position.
#[test]
fn triumph_dying_puts_itself_and_the_top_six_cards_back_in_random_order_both_seats() {
    for owner in [P0, P1] {
        let mut orders = BTreeSet::new();
        for seed in 0..8 {
            let mut state = game_with_seed(&["Island"; 10], 0x5331_4c00_0100 + seed);
            let triumph = put(&mut state, owner, TRIUMPH, Zone::Battlefield);
            let before = state.players[owner.index()].library.clone();
            for position in [0, 8] {
                let card = before[position];
                state.library_knowledge[owner.index()][owner.index()].push(LibraryKnowledgeEntry {
                    position: position as u32,
                    object: card,
                    zone_change_count: state.objects.get(card).zone_change_count,
                });
            }
            let changes = before
                .iter()
                .map(|&card| state.objects.get(card).zone_change_count)
                .collect::<Vec<_>>();
            destroy(&mut state, triumph);
            next(&mut state);
            assert!(triggered(&state, triumph), "Praesidium Protectiva");
            settled(&mut state);
            assert_eq!(zone(&state, triumph), Zone::Library);
            let after = state.players[owner.index()].library.clone();
            assert_eq!(after.len(), before.len() + 1);
            let pile = after[..7].iter().copied().collect::<BTreeSet<_>>();
            let expected = before[..6]
                .iter()
                .copied()
                .chain([triumph])
                .collect::<BTreeSet<_>>();
            assert_eq!(pile, expected);
            assert_eq!(&after[7..], &before[6..]);
            // The library cards keep their objects.
            for (&card, &count) in before.iter().zip(&changes) {
                assert_eq!(state.objects.get(card).zone_change_count, count);
            }
            let known = state.library_knowledge[owner.index()][owner.index()]
                .iter()
                .map(|entry| (entry.position, entry.object))
                .collect::<Vec<_>>();
            assert_eq!(known, vec![(9, before[8])]);
            assert!(state.stack.is_empty());
            orders.insert(after[..7].to_vec());
        }
        assert!(orders.len() > 1, "the pile is put back in a random order");
    }
}

#[test]
fn triumph_does_nothing_with_fewer_than_six_library_cards_both_seats() {
    for owner in [P0, P1] {
        let mut state = game(&["Island"; 5]);
        let triumph = put(&mut state, owner, TRIUMPH, Zone::Battlefield);
        let before = state.players[owner.index()].library.clone();
        destroy(&mut state, triumph);
        next(&mut state);
        assert!(triggered(&state, triumph));
        settled(&mut state);
        assert_eq!(zone(&state, triumph), Zone::Graveyard);
        assert_eq!(state.players[owner.index()].library, before);
    }
}

#[test]
fn triumph_does_nothing_once_it_left_the_graveyard_both_seats() {
    for owner in [P0, P1] {
        let mut state = game(&["Island"; 10]);
        let triumph = put(&mut state, owner, TRIUMPH, Zone::Battlefield);
        let before = state.players[owner.index()].library.clone();
        destroy(&mut state, triumph);
        next(&mut state);
        assert!(triggered(&state, triumph));
        event::propose_and_commit(&mut state, ProposedEvent::zone_change(triumph, Zone::Exile));
        settled(&mut state);
        assert_eq!(zone(&state, triumph), Zone::Exile);
        assert_eq!(state.players[owner.index()].library, before);
    }
}

/// Dominate steals Triumph, which then dies: it goes to its owner's
/// graveyard, not its controller's, so its ability doesn't trigger.
#[test]
fn a_stolen_triumph_that_dies_does_not_trigger_both_seats() {
    for owner in [P0, P1] {
        let thief = owner.opponent();
        let mut state = game(&["Island"; 10]);
        set_active(&mut state, thief);
        let triumph = put(&mut state, owner, TRIUMPH, Zone::Battlefield);
        let dominate = put(&mut state, thief, "Dominate", Zone::Hand);
        // {X}{1}{U}{U} with X = 5, Triumph's mana value.
        pool(&mut state, thief, &[ManaColor::U, ManaColor::U], 6);
        cast(&mut state, dominate, &[Target::Object(triumph)]);
        settled(&mut state);
        assert_eq!(state.objects.get(triumph).controller, thief);
        let before = state.players[owner.index()].library.clone();
        destroy(&mut state, triumph);
        next(&mut state);
        assert!(!triggered(&state, triumph));
        settled(&mut state);
        assert!(state.players[owner.index()].graveyard.contains(&triumph));
        assert_eq!(state.players[owner.index()].library, before);
    }
}

/// Rest in Peace exiles a dying Triumph instead, so it is never put into a
/// graveyard and nothing triggers.
#[test]
fn rest_in_peace_exiles_a_dying_triumph_without_its_trigger_both_seats() {
    for owner in [P0, P1] {
        let mut state = game(&["Island"; 10]);
        put(
            &mut state,
            owner.opponent(),
            "Rest in Peace",
            Zone::Battlefield,
        );
        let triumph = put(&mut state, owner, TRIUMPH, Zone::Battlefield);
        let before = state.players[owner.index()].library.clone();
        destroy(&mut state, triumph);
        next(&mut state);
        assert!(!triggered(&state, triumph));
        settled(&mut state);
        assert_eq!(zone(&state, triumph), Zone::Exile);
        assert_eq!(state.players[owner.index()].library, before);
    }
}

// ---- Miracle ------------------------------------------------------------------------

/// Triumph drawn as its owner's first card of a turn, theirs or their
/// opponent's, is revealed and cast for {1}{W} while its trigger resolves.
#[test]
fn triumph_miracle_casts_it_for_one_and_white_on_either_turn_both_seats() {
    for owner in [P0, P1] {
        for active in [owner, owner.opponent()] {
            let mut state = game(&["Island"; 10]);
            set_active(&mut state, active);
            let plains = put(&mut state, owner, "Plains", Zone::Battlefield);
            let island = put(&mut state, owner, "Island", Zone::Battlefield);
            let triumph = draw_first(&mut state, owner, TRIUMPH);
            assert!(miracle(&mut state, owner, triumph), "the cast is offered");
            assert_eq!(zone(&state, triumph), Zone::Battlefield);
            assert_eq!(state.objects.get(triumph).controller, owner);
            assert!(
                state.objects.get(plains).tapped && state.objects.get(island).tapped,
                "paid {{1}}{{W}}"
            );
        }
    }
}

#[test]
fn triumph_miracle_is_not_offered_without_one_and_white_both_seats() {
    for owner in [P0, P1] {
        let mut state = game(&["Island"; 10]);
        put(&mut state, owner, "Plains", Zone::Battlefield);
        let triumph = draw_first(&mut state, owner, TRIUMPH);
        assert!(!miracle(&mut state, owner, triumph));
        assert!(hand_contains(&state, owner, triumph));
    }
}

// ---- Lavinia and miracle --------------------------------------------------------

/// `actor`'s opponent casts Lavinia, paying with real land activations,
/// then `actor` becomes active with priority in their main phase.
fn lavinia_against(actor: PlayerId) -> (GameState, ObjectId) {
    let mut state = game(&["Island"; 20]);
    let controller = actor.opponent();
    set_active(&mut state, controller);
    for name in ["Plains", "Island"] {
        let land = put(&mut state, controller, name, Zone::Battlefield);
        act(&mut state, Action::ActivateManaAbility(land));
    }
    let lavinia = put(&mut state, controller, LAVINIA, Zone::Hand);
    cast(&mut state, lavinia, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, lavinia), Zone::Battlefield);
    set_active(&mut state, actor);
    state.engine.priority_passes = [false; 2];
    (state, lavinia)
}

/// Whether S1a's state records no paid miracle and no unpaid cast.
fn no_lavinia_records(state: &GameState) -> bool {
    mtg_kernel::eternal_sideboard_s1a_v1::read_state(state)
        .is_none_or(|s1a| s1a.miracle_payments.is_empty() && s1a.unpaid_casts.is_empty())
}

/// A spell cast for its miracle cost spent mana (601.2f, 702.94a), so an
/// opposing Lavinia doesn't counter it, though the resolution cast commits
/// none. The paid miracle is recorded through the cast's selection, which
/// a snapshot keeps.
#[test]
fn lavinia_does_not_counter_a_triumph_cast_for_its_miracle_cost_both_seats() {
    for actor in [P0, P1] {
        let (mut state, lavinia) = lavinia_against(actor);
        put(&mut state, actor, "Plains", Zone::Battlefield);
        put(&mut state, actor, "Island", Zone::Battlefield);
        let triumph = draw_first(&mut state, actor, TRIUMPH);
        assert!(matches!(
            next(&mut state),
            Decision::ChooseEffectOption { source, .. } if source == triumph
        ));
        act(&mut state, Action::ChooseEffectOption(1));
        match settle(&mut state) {
            Some(Decision::ChooseEffectOption {
                player,
                option_count: 2,
                ..
            }) if player == actor => act(&mut state, Action::ChooseEffectOption(0)),
            other => panic!("expected the cast offer, got {other:?}"),
        }
        assert!(matches!(
            next(&mut state),
            Decision::ChooseEffectTargets { ref legal_targets, .. }
                if legal_targets == &[Target::Object(triumph)]
        ));
        let payments = &mtg_kernel::eternal_sideboard_s1a_v1::read_state(&state)
            .unwrap()
            .miracle_payments;
        assert_eq!(payments.len(), 1);
        assert_eq!(payments[0].object, triumph);
        let mut restored: GameState =
            serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
        assert_eq!(restored.state_hash(), state.state_hash());
        for branch in [&mut state, &mut restored] {
            act(branch, Action::ChooseEffectTarget(Target::Object(triumph)));
            next(branch);
            assert_eq!(zone(branch, triumph), Zone::Stack);
            assert!(!triggered(branch, lavinia), "no counter trigger");
            settled(branch);
            assert_eq!(zone(branch, triumph), Zone::Battlefield);
            assert!(no_lavinia_records(branch));
        }
        assert_eq!(state.state_hash(), restored.state_hash());
    }
}

/// Terminus's miracle cast is a noncreature spell with mana value 6: under
/// an opposing Lavinia it is offered with six lands and resolves, and isn't
/// offered with five.
#[test]
fn lavinia_lets_a_paid_terminus_miracle_resolve_only_with_six_lands_both_seats() {
    for actor in [P0, P1] {
        for lands in [6, 5] {
            let (mut state, lavinia) = lavinia_against(actor);
            put(&mut state, actor, "Plains", Zone::Battlefield);
            for _ in 1..lands {
                put(&mut state, actor, "Island", Zone::Battlefield);
            }
            let terminus = draw_first(&mut state, actor, "Terminus");
            let offered = miracle(&mut state, actor, terminus);
            assert_eq!(offered, lands == 6, "{lands} lands");
            if offered {
                assert_eq!(zone(&state, terminus), Zone::Graveyard);
                assert_eq!(zone(&state, lavinia), Zone::Library, "Terminus resolved");
            } else {
                assert!(hand_contains(&state, actor, terminus));
                assert_eq!(zone(&state, lavinia), Zone::Battlefield);
            }
            assert!(no_lavinia_records(&state));
        }
    }
}

// ---- Deck games -----------------------------------------------------------------

/// Counts, over one session game, Lavinia's counter triggers on the public
/// stack, Triumph casts for its miracle cost answered from the action list,
/// and Praesidium Protectiva triggers that resolved with Triumph leaving
/// its owner's graveyard.
#[derive(Default)]
struct DeckEvents {
    lavinia_counters: usize,
    miracle_casts: usize,
    piles: usize,
    /// Lavinia triggers on the stack at the last decision.
    lavinia_on_stack: usize,
    /// Triumph's pile triggers on the stack at the last decision, by their
    /// source's arena id.
    piles_on_stack: BTreeSet<u64>,
    /// Action kinds chosen at decisions about a Triumph, for the report.
    triumph_actions: BTreeMap<String, usize>,
    /// The source zones of Triumph's triggers seen on the public stack, by
    /// decision, for the report.
    trigger_zones: BTreeMap<String, usize>,
}

fn named_card(card: &Value, name: &str) -> bool {
    card["card_db_id"]
        .as_u64()
        .is_some_and(|id| CARD_DEFS[id as usize].name == name)
}

impl DeckEvents {
    fn observe(&mut self, decision: &Value, action: &Value) {
        let projection = &decision["observation"]["projection"];
        let stack = projection["stack"].as_array().cloned().unwrap_or_default();
        let lavinias = stack
            .iter()
            .filter(|item| {
                item["stack_item_kind"] == "triggered_ability"
                    && named_card(&item["source"], LAVINIA)
            })
            .count();
        self.lavinia_counters += lavinias.saturating_sub(self.lavinia_on_stack);
        self.lavinia_on_stack = lavinias;
        let triumph_triggers = stack.iter().filter(|item| {
            item["stack_item_kind"] == "triggered_ability" && named_card(&item["source"], TRIUMPH)
        });
        for item in triumph_triggers.clone() {
            *self
                .trigger_zones
                .entry(item["source"]["zone"].to_string())
                .or_default() += 1;
        }
        // The miracle trigger's source is in its hand; the pile trigger's
        // isn't.
        let piles = triumph_triggers
            .filter(|item| item["source"]["zone"] != "Hand")
            .filter_map(|item| item["source"]["arena_id"].as_u64())
            .collect::<BTreeSet<_>>();
        let graveyards = projection["graveyards"]
            .as_array()
            .into_iter()
            .flatten()
            .flat_map(|cards| cards.as_array().into_iter().flatten())
            .filter_map(|card| card["stable"]["arena_id"].as_u64())
            .collect::<BTreeSet<_>>();
        self.piles += self
            .piles_on_stack
            .iter()
            .filter(|arena| !piles.contains(arena) && !graveyards.contains(arena))
            .count();
        self.piles_on_stack = piles;
        let semantic = &action["semantic"];
        if semantic
            .to_string()
            .contains(&format!("\"card_db_id\":{}", id(TRIUMPH)))
        {
            *self
                .triumph_actions
                .entry(semantic["action_kind"].as_str().unwrap_or("?").to_owned())
                .or_default() += 1;
        }
        if semantic["action_kind"] == "choose_effect_target"
            && named_card(&semantic["source"], TRIUMPH)
            && semantic["target"]["object"]["card_db_id"] == semantic["source"]["card_db_id"]
        {
            self.miracle_casts += 1;
        }
    }
}

/// Lavinia and Triumph are sideboard cards in their lists, which the
/// best-of-three harness rarely draws, so a white-blue deck with four of
/// each in its mainboard and a blue-black deck of free and expensive
/// noncreature spells, with Gurmag Anglers to block and trade with Triumph,
/// play each other through public sessions, both seats with ranked and with
/// seeded random actions. A ranked seat reveals a drawn Triumph and casts it
/// for its miracle cost. The games put a Lavinia counter on the public
/// stack, cast a Triumph for its miracle cost from the action list and
/// resolve Triumph's pile. Every session game checks that no noncreature
/// spell over the caster's lands is offered while an opponent controls
/// Lavinia.
#[test]
fn s1l_lavinia_and_triumph_decks_finish_and_replay_through_public_sessions() {
    let white_blue = stress_deck(
        &[("Plains", 12), ("Island", 12)],
        [
            LAVINIA,
            TRIUMPH,
            "Brainstorm",
            "Opt",
            "Ponder",
            "Swords to Plowshares",
            "Counterspell",
            "Thraben Inspector",
            "Kor Skyfisher",
        ],
    );
    let blue_black = stress_deck(
        &[("Island", 12), ("Swamp", 12)],
        [
            "Force of Will",
            "Daze",
            "Gitaxian Probe",
            "Mental Misstep",
            "Gush",
            "Diabolic Edict",
            "Dismember",
            "Toxic Deluge",
            "Gurmag Angler",
        ],
    );
    let mut totals = DeckEvents::default();
    for (pair, seed) in [
        ([white_blue.clone(), blue_black.clone()], 0x5331_4c00_u64),
        ([blue_black, white_blue], 0x5331_4c01),
    ] {
        for policy in [
            Policy::Ranked,
            Policy::Random(seed),
            Policy::Random(seed + 16),
            Policy::Random(seed + 32),
            Policy::Random(seed + 48),
            Policy::Random(seed + 64),
        ] {
            let mut events = DeckEvents::default();
            let game = play_observed(pair.clone(), None, seed, policy, &mut |decision, action| {
                events.observe(decision, action)
            });
            let replay = play_observed(pair.clone(), None, seed, policy, &mut |_, _| {});
            assert_eq!(game.transcript_sha256, replay.transcript_sha256);
            println!(
                "seed={seed:#x} random={}: counters {} miracle casts {} piles {} triumph actions {:?} trigger zones {:?}",
                matches!(policy, Policy::Random(_)),
                events.lavinia_counters,
                events.miracle_casts,
                events.piles,
                events.triumph_actions,
                events.trigger_zones
            );
            totals.lavinia_counters += events.lavinia_counters;
            totals.miracle_casts += events.miracle_casts;
            totals.piles += events.piles;
        }
    }
    println!(
        "Lavinia counters: {}, miracle casts: {}, piles: {}",
        totals.lavinia_counters, totals.miracle_casts, totals.piles
    );
    assert!(
        totals.lavinia_counters > 0,
        "no Lavinia counter reached the public stack"
    );
    assert!(
        totals.miracle_casts > 0,
        "no Triumph was cast for its miracle cost"
    );
    assert!(totals.piles > 0, "no Triumph pile resolved");
}
