//! Eternal wave E4b: native rules tests for companions and the cards that
//! complete the Vintage Lurrus decks.
#![cfg(feature = "eternal-fixtures")]

use mtg_kernel::card_def::{
    card_id_by_name, CardCapability, CardDef, CardType, Keywords, Subtype, Supertype, WardCostDef,
    CARD_DEFS,
};
use mtg_kernel::engine::{self, Action, CastMode, Decision};
use mtg_kernel::eternal_lurrus_v1::{add_companions, companion_condition_met, needle_name_options};
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::limited_session_v1::{CustomCardCountV1, CustomDeckV1, LimitedJsonlServerV1};
use mtg_kernel::mana::ManaColor;
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Target, Zone};
use serde_json::{json, Value};

const P0: PlayerId = PlayerId::P0;
const P1: PlayerId = PlayerId::P1;

const E4B_CARDS: [&str; 29] = [
    "Lurrus of the Dream-Den",
    "Demonic Tutor",
    "Flow State",
    "Force of Negation",
    "Gitaxian Probe",
    "Null Rod",
    "Prismatic Ending",
    "Psychic Frog",
    "Swords to Plowshares",
    "Treasure Cruise",
    "Tundra",
    "Aether Spellbomb",
    "Dig Through Time",
    "Dismember",
    "Gush",
    "Flame of Anor",
    "Forth Eorlingas!",
    "Hexing Squelcher",
    "Meticulous Archive",
    "Mox Opal",
    "Portable Hole",
    "Snapcaster Mage",
    "Soul-Guide Lantern",
    "Thundertrap Trainer",
    "Brain Freeze",
    "Merchant Scroll",
    "Underworld Breach",
    "Mana Drain",
    "Pithing Needle",
];

fn def(name: &str) -> &'static CardDef {
    &CARD_DEFS[id(name) as usize]
}

fn id(name: &str) -> u16 {
    card_id_by_name(name).unwrap_or_else(|| panic!("{name}"))
}

/// A first main phase with P0 active and holding priority. Each library
/// holds `library` in order, top first.
fn game(library: &[&str]) -> GameState {
    let ids: Vec<u16> = library.iter().map(|name| id(name)).collect();
    let mut state = GameState::new_from_libraries(
        &ids,
        &ids,
        |id| CARD_DEFS[id as usize].object_name.into(),
        0x4534_4c55_5252,
    );
    state.active_player = P0;
    state.priority_player = P0;
    state.step = Step::Main1;
    state
}

fn islands() -> GameState {
    game(&["Island"; 20])
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

fn mana(state: &GameState, player: PlayerId, color: ManaColor) -> u8 {
    state.players[player.index()].mana_pool[color.pool_index()]
}

fn zone(state: &GameState, object: ObjectId) -> Zone {
    state.objects.get(object).zone
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

/// The first legal answer to any choice: the first target or option,
/// finishing a selection that may finish, declining a Boolean or kicker.
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
        Decision::ChooseCostTargets { candidates, .. } => Action::ChooseCostTarget(candidates[0]),
        Decision::ChooseCastMode { options, .. } => Action::ChooseCastMode(options[0]),
        Decision::ChooseKicker { .. } => Action::ChooseKicker(false),
        Decision::ChooseSpellMode { legal_modes, .. } => Action::ChooseSpellMode(legal_modes[0]),
        Decision::Discard { count, choices, .. } => {
            Action::Discard(choices.into_iter().take(count as usize).collect())
        }
        other => panic!("unexpected choice: {other:?}"),
    }
}

/// Settles, answering every choice with `answer`.
fn settle_with(state: &mut GameState, mut answer: impl FnMut(&GameState, Decision) -> Action) {
    while let Some(decision) = settle(state) {
        let action = answer(state, decision);
        act(state, action);
    }
}

fn settle_first(state: &mut GameState) {
    settle_with(state, |_, decision| first_answer(decision));
}

fn priority(state: &mut GameState) -> (Vec<ObjectId>, Vec<(ObjectId, u8)>) {
    match next(state) {
        Decision::CastSpellOrPass {
            castable_spells,
            activatable_abilities,
            ..
        } => (castable_spells, activatable_abilities),
        other => panic!("expected priority, got {other:?}"),
    }
}

fn castable(state: &mut GameState, spell: ObjectId) -> bool {
    priority(state).0.contains(&spell)
}

fn activatable(state: &mut GameState, source: ObjectId, index: u8) -> bool {
    priority(state).1.contains(&(source, index))
}

/// Casts `spell`, answering each target prompt from `targets` in order.
fn cast(state: &mut GameState, spell: ObjectId, targets: &[Target]) {
    assert!(
        castable(state, spell),
        "{} not castable",
        state.objects.get(spell).name
    );
    act(state, Action::CastSpell(spell));
    for target in targets {
        choose_target(state, *target);
    }
}

fn choose_target(state: &mut GameState, target: Target) {
    match next(state) {
        Decision::ChooseTargets { legal_targets, .. } => {
            assert!(
                legal_targets.contains(&target),
                "{target:?} not in {legal_targets:?}"
            );
            act(state, Action::ChooseTarget(target));
        }
        Decision::ChooseEffectTargets { legal_targets, .. } => {
            assert!(
                legal_targets.contains(&target),
                "{target:?} not in {legal_targets:?}"
            );
            act(state, Action::ChooseEffectTarget(target));
        }
        other => panic!("expected targets, got {other:?}"),
    }
}

/// Activates `source`'s ability `index`, answering each target prompt.
fn activate(state: &mut GameState, source: ObjectId, index: u8, targets: &[Target]) {
    assert!(
        activatable(state, source, index),
        "{} ability {index} not activatable",
        state.objects.get(source).name
    );
    act(state, Action::ActivateAbility(source, index));
    for target in targets {
        choose_target(state, *target);
    }
}

/// Passes every priority window, declaring no attackers or blockers, until
/// `stop` holds before a decision.
fn pass_until(state: &mut GameState, stop: impl Fn(&GameState) -> bool) {
    for _ in 0..400 {
        let decision = next(state);
        if stop(state) {
            return;
        }
        match decision {
            Decision::CastSpellOrPass { .. } => act(state, Action::Pass),
            Decision::DeclareAttackers { .. } => act(state, Action::DeclareAttackers(Vec::new())),
            Decision::DeclareBlockers { .. } => act(state, Action::DeclareBlockers(Vec::new())),
            Decision::OrderTriggers { pending, .. } => {
                act(state, Action::OrderTriggers((0..pending.len()).collect()))
            }
            Decision::Discard { count, choices, .. } => act(
                state,
                Action::Discard(choices.into_iter().take(count as usize).collect()),
            ),
            other => panic!("unexpected decision while passing: {other:?}"),
        }
    }
    panic!("condition never held");
}

/// P0 attacks with `attackers` this turn, nobody blocks, and the game
/// passes to P0's second main phase.
fn attack_with(state: &mut GameState, attackers: Vec<ObjectId>) {
    let mut declared = false;
    for _ in 0..200 {
        match next(state) {
            Decision::DeclareAttackers { .. } if !declared => {
                declared = true;
                act(state, Action::DeclareAttackers(attackers.clone()));
            }
            Decision::DeclareAttackers { .. } => act(state, Action::DeclareAttackers(Vec::new())),
            Decision::DeclareBlockers { .. } => act(state, Action::DeclareBlockers(Vec::new())),
            Decision::CastSpellOrPass { .. }
                if state.step == Step::Main2
                    && state.stack.is_empty()
                    && state.engine.pending_triggers.is_empty() =>
            {
                return
            }
            Decision::CastSpellOrPass { .. } => act(state, Action::Pass),
            Decision::OrderTriggers { pending, .. } => {
                act(state, Action::OrderTriggers((0..pending.len()).collect()))
            }
            Decision::ChooseCombatDamageRange { .. } => {
                act(state, Action::ChooseCombatDamageRange { upper_half: true })
            }
            Decision::ChooseAttackTarget { .. } => {
                act(state, Action::ChooseAttackTarget(Target::Player(P1)))
            }
            other => panic!("unexpected decision in combat: {other:?}"),
        }
    }
    panic!("combat never ended");
}

/// Settles, answering each target prompt with `target` and any other choice
/// with `first_answer`. Returns the legal target lists it saw.
fn settle_targeting(state: &mut GameState, target: Target) -> Vec<Vec<Target>> {
    let mut seen = Vec::new();
    while let Some(decision) = settle(state) {
        match decision {
            Decision::ChooseTargets { legal_targets, .. } => {
                assert!(
                    legal_targets.contains(&target),
                    "{target:?} not in {legal_targets:?}"
                );
                seen.push(legal_targets);
                act(state, Action::ChooseTarget(target));
            }
            other => act(state, first_answer(other)),
        }
    }
    seen
}

/// P1 becomes the active player in its first main phase, holding priority.
fn opponents_main_phase(state: &mut GameState) {
    state.active_player = P1;
    state.priority_player = P1;
    state.step = Step::Main1;
}

/// P1 casts Lightning Bolt at `target` and passes priority to P0.
fn opponent_bolts(state: &mut GameState, target: Target) -> ObjectId {
    state.priority_player = P1;
    let bolt = put(state, P1, "Lightning Bolt", Zone::Hand);
    pool(state, P1, &[ManaColor::R], 0);
    cast(state, bolt, &[target]);
    assert!(matches!(
        next(state),
        Decision::CastSpellOrPass { player: P1, .. }
    ));
    act(state, Action::Pass);
    bolt
}

// ---- Characteristics -----------------------------------------------------------

#[test]
fn e4b_cards_are_full_with_their_printed_characteristics() {
    for name in E4B_CARDS {
        assert_eq!(def(name).capability, CardCapability::Full, "{name}");
        assert!(!def(name).is_token, "{name}");
    }
    for name in ["Thundertrap Trainer Offspring Token", "Human Knight Token"] {
        assert_eq!(def(name).capability, CardCapability::Full, "{name}");
        assert!(def(name).is_token, "{name}");
    }
    let lurrus = def("Lurrus of the Dream-Den");
    assert_eq!(lurrus.subtypes, &[Subtype::Cat, Subtype::Nightmare]);
    assert_eq!(lurrus.supertypes, &[Supertype::Legendary]);
    assert_eq!((lurrus.power, lurrus.toughness), (Some(3), Some(2)));
    assert_eq!(lurrus.mana_value, 3);
    assert!(lurrus.keywords.has(Keywords::LIFELINK));
    assert_eq!(lurrus.activated_abilities.len(), 1);
    assert_eq!(lurrus.activated_abilities[0].activation_zone, Zone::Exile);
    assert_eq!(def("Psychic Frog").subtypes, &[Subtype::Frog]);
    assert_eq!(
        def("Hexing Squelcher").subtypes,
        &[Subtype::Goblin, Subtype::Sorcerer]
    );
    assert!(Subtype::Frog.is_creature_type());
    assert!(Subtype::Sorcerer.is_creature_type());
    assert!(def("Hexing Squelcher").spell_cannot_be_countered);
    assert_eq!(
        def("Hexing Squelcher").ward_cost,
        Some(WardCostDef::PayLife(2))
    );
    assert!(def("Snapcaster Mage").keywords.has(Keywords::FLASH));
    assert_eq!(
        def("Snapcaster Mage").subtypes,
        &[Subtype::Human, Subtype::Wizard]
    );
    let knight = def("Human Knight Token");
    assert!(knight.keywords.has(Keywords::TRAMPLE) && knight.keywords.has(Keywords::HASTE));
    assert_eq!((knight.power, knight.toughness), (Some(2), Some(2)));
    let offspring = def("Thundertrap Trainer Offspring Token");
    assert_eq!(offspring.object_name, "Thundertrap Trainer");
    assert_eq!((offspring.power, offspring.toughness), (Some(1), Some(1)));
    assert!(def("Thundertrap Trainer").kicker_cost.is_some());
    for name in ["Tundra", "Meticulous Archive"] {
        assert!(def(name).is_land, "{name}");
        assert_eq!(def(name).subtypes, &[Subtype::Plains, Subtype::Island]);
    }
    assert!(def("Meticulous Archive").enters_battlefield_tapped);
    assert!(!def("Tundra").enters_battlefield_tapped);
    assert!(def("Treasure Cruise").delve && def("Dig Through Time").delve);
    assert_eq!(def("Mox Opal").supertypes, &[Supertype::Legendary]);
    assert_eq!(def("Mox Opal").mana_value, 0);
    assert!(def("Force of Negation").alt_cost.is_some());
    assert!(def("Gush").alt_cost.is_some());
    let flame = def("Flame of Anor");
    assert!(flame.mode2.is_some() && flame.mode3.is_some());
    assert_eq!(flame.additional_modes.len(), 3);
    assert_eq!(def("Prismatic Ending").cost.x_count, 1);
    assert_eq!(def("Forth Eorlingas!").cost.x_count, 1);
    assert_eq!(def("Dismember").mana_value, 3);
    assert_eq!(def("Underworld Breach").types, &[CardType::Enchantment]);
}

// ---- Companions -----------------------------------------------------------------

#[test]
fn lurrus_companion_condition_checks_every_permanent_card() {
    let lurrus = id("Lurrus of the Dream-Den");
    let ids = |names: &[&str]| names.iter().map(|name| id(name)).collect::<Vec<_>>();
    assert_eq!(
        companion_condition_met(
            lurrus,
            &ids(&[
                "Island",
                "Mox Pearl",
                "Sol Ring",
                "Snapcaster Mage",
                "Ancestral Recall",
                "Dig Through Time",
            ])
        ),
        Some(true),
        "instants and sorceries of any mana value are fine"
    );
    assert_eq!(
        companion_condition_met(lurrus, &ids(&["Island", "Brazen Borrower"])),
        Some(false)
    );
    assert_eq!(
        companion_condition_met(id("Snapcaster Mage"), &ids(&["Island"])),
        None,
        "not a companion"
    );

    let deck = |companion: Option<&str>, last: &str| CustomDeckV1 {
        cards: vec![
            CustomCardCountV1 {
                name: "Island".into(),
                count: 39,
            },
            CustomCardCountV1 {
                name: last.into(),
                count: 1,
            },
        ],
        companion: companion.map(str::to_owned),
    };
    let (mainboard, companion) = deck(Some("Lurrus of the Dream-Den"), "Snapcaster Mage")
        .resolve_with_companion()
        .unwrap();
    assert_eq!(mainboard.len(), 40);
    assert_eq!(companion, Some(lurrus));
    assert!(deck(Some("Lurrus of the Dream-Den"), "Brazen Borrower")
        .resolve_with_companion()
        .is_err());
    assert!(deck(Some("Island"), "Snapcaster Mage")
        .resolve_with_companion()
        .is_err());
    assert!(
        deck(Some("Lurrus of the Dream-Den"), "Snapcaster Mage")
            .resolve()
            .is_err(),
        "a companion needs the Limited JSONL reset"
    );
    // Without a companion the deck's wire form is unchanged.
    assert_eq!(
        serde_json::to_value(deck(None, "Snapcaster Mage")).unwrap(),
        json!({"cards": [
            {"name": "Island", "count": 39},
            {"name": "Snapcaster Mage", "count": 1},
        ]})
    );
}

#[test]
fn lurrus_companion_waits_in_exile_and_joins_the_hand_for_three() {
    let mut state = islands();
    add_companions(&mut state, [Some(id("Lurrus of the Dream-Den")), None]);
    let lurrus = *state.exile.last().unwrap();
    assert_eq!(zone(&state, lurrus), Zone::Exile);
    assert_eq!(state.objects.get(lurrus).owner, P0);
    pool(&mut state, P0, &[], 2);
    assert!(!activatable(&mut state, lurrus, 0), "{{3}} is not payable");
    pool(&mut state, P0, &[], 1);
    assert!(activatable(&mut state, lurrus, 0));
    act(&mut state, Action::ActivateAbility(lurrus, 0));
    assert_eq!(zone(&state, lurrus), Zone::Hand);
    assert!(state.stack.is_empty(), "a special action");
    assert_eq!(state.players[0].mana_pool, [0; 6]);
    assert!(
        state
            .known_hand_cards(P1, P0)
            .iter()
            .any(|entry| entry.object == lurrus),
        "the opponent knows the companion"
    );
    assert!(matches!(
        next(&mut state),
        Decision::CastSpellOrPass { player: P0, .. }
    ));

    // Only as a sorcery: not while a spell is on the stack.
    let mut state = islands();
    add_companions(&mut state, [Some(id("Lurrus of the Dream-Den")), None]);
    let lurrus = *state.exile.last().unwrap();
    let recall = put(&mut state, P0, "Ancestral Recall", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 3);
    cast(&mut state, recall, &[Target::Player(P0)]);
    assert!(!activatable(&mut state, lurrus, 0));
}

#[test]
fn the_limited_session_puts_a_deck_companion_in_exile_and_rejects_a_failed_condition() {
    let reset = |second: &str| {
        json!({"schema_version": 4, "request_type": "reset", "request_id": "reset",
            "decks": [
                {"cards": [{"name": "Island", "count": 39}, {"name": second, "count": 1}],
                 "companion": "Lurrus of the Dream-Den"},
                {"cards": [{"name": "Island", "count": 40}]},
            ],
            "episode_id": 7, "env_seed": 7,
            "max_physical_decisions": 4096, "max_policy_steps": 8192})
        .to_string()
    };
    let mut server = LimitedJsonlServerV1::new_with_london_mulligans_v1();
    let reply: Value = serde_json::from_str(&server.handle_line(&reset("Mox Pearl"))).unwrap();
    assert_eq!(reply["response_type"], "decision", "{reply}");
    let lurrus = u64::from(id("Lurrus of the Dream-Den"));
    let exile = reply["decision"]["observation"]["projection"]["exile"]
        .as_array()
        .unwrap();
    assert!(
        exile
            .iter()
            .any(|card| card["stable"]["card_db_id"].as_u64() == Some(lurrus)),
        "{exile:?}"
    );

    let mut server = LimitedJsonlServerV1::new_with_london_mulligans_v1();
    let reply: Value =
        serde_json::from_str(&server.handle_line(&reset("Brazen Borrower"))).unwrap();
    assert_eq!(reply["response_type"], "error", "{reply}");
    assert_eq!(reply["error"]["code"], "unsupported_deck");
}

#[test]
fn lurrus_casts_one_permanent_spell_with_mana_value_two_or_less_from_the_graveyard_each_turn() {
    let mut state = islands();
    put(&mut state, P0, "Lurrus of the Dream-Den", Zone::Battlefield);
    let pearl = put(&mut state, P0, "Mox Pearl", Zone::Graveyard);
    let jet = put(&mut state, P0, "Mox Jet", Zone::Graveyard);
    let borrower = put(&mut state, P0, "Brazen Borrower", Zone::Graveyard);
    let recall = put(&mut state, P0, "Ancestral Recall", Zone::Graveyard);
    pool(&mut state, P0, &[ManaColor::U], 3);
    let (castable_now, _) = priority(&mut state);
    assert!(castable_now.contains(&pearl) && castable_now.contains(&jet));
    assert!(!castable_now.contains(&borrower), "mana value 3");
    assert!(!castable_now.contains(&recall), "not a permanent spell");
    cast(&mut state, pearl, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, pearl), Zone::Battlefield);
    assert!(!castable(&mut state, jet), "once during each of your turns");

    // Not during an opponent's turn, even at instant speed.
    let mut state = islands();
    put(&mut state, P0, "Lurrus of the Dream-Den", Zone::Battlefield);
    let snapcaster = put(&mut state, P0, "Snapcaster Mage", Zone::Graveyard);
    pool(&mut state, P0, &[ManaColor::U], 1);
    assert!(castable(&mut state, snapcaster), "flash, on P0's turn");
    state.active_player = P1;
    state.priority_player = P0;
    assert!(!castable(&mut state, snapcaster));
}

// ---- Spells -----------------------------------------------------------------------

#[test]
fn demonic_tutor_finds_any_card_without_revealing_it() {
    let mut state = game(&["Island", "Sol Ring", "Island"]);
    let ring = state.players[0].library[1];
    let tutor = put(&mut state, P0, "Demonic Tutor", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 1);
    cast(&mut state, tutor, &[]);
    match settle(&mut state) {
        Some(Decision::ChooseEffectTargets { legal_targets, .. }) => {
            assert_eq!(legal_targets.len(), 3, "any card");
            act(&mut state, Action::ChooseEffectTarget(Target::Object(ring)));
        }
        other => panic!("expected the search, got {other:?}"),
    }
    settle_first(&mut state);
    assert_eq!(zone(&state, ring), Zone::Hand);
    assert!(
        !state
            .known_hand_cards(P1, P0)
            .iter()
            .any(|entry| entry.object == ring),
        "Demonic Tutor does not reveal"
    );
}

#[test]
fn merchant_scroll_finds_and_reveals_a_blue_instant() {
    let mut state = game(&["Island", "Counterspell", "Lightning Bolt", "Ponder"]);
    let counterspell = state.players[0].library[1];
    let scroll = put(&mut state, P0, "Merchant Scroll", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 1);
    cast(&mut state, scroll, &[]);
    match settle(&mut state) {
        Some(Decision::ChooseEffectTargets { legal_targets, .. }) => {
            assert_eq!(
                legal_targets,
                vec![Target::Object(counterspell)],
                "red instants and blue sorceries do not qualify"
            );
            act(
                &mut state,
                Action::ChooseEffectTarget(Target::Object(counterspell)),
            );
        }
        other => panic!("expected the search, got {other:?}"),
    }
    settle_first(&mut state);
    assert_eq!(zone(&state, counterspell), Zone::Hand);
    assert!(state
        .known_hand_cards(P1, P0)
        .iter()
        .any(|entry| entry.object == counterspell));
}

#[test]
fn flow_state_takes_two_with_an_instant_and_a_sorcery_in_the_graveyard() {
    for (graveyard, taken) in [
        (&["Lightning Bolt"][..], 1),
        (&["Lightning Bolt", "Ponder"][..], 2),
    ] {
        let mut state = game(&["Island"; 10]);
        for name in graveyard {
            put(&mut state, P0, name, Zone::Graveyard);
        }
        let flow = put(&mut state, P0, "Flow State", Zone::Hand);
        pool(&mut state, P0, &[ManaColor::U], 1);
        cast(&mut state, flow, &[]);
        settle_first(&mut state);
        assert_eq!(state.players[0].hand.len(), taken, "{graveyard:?}");
        assert_eq!(state.players[0].library.len(), 10 - taken);
    }
}

#[test]
fn dig_through_time_delves_and_takes_two_of_seven() {
    let mut state = game(&["Island"; 12]);
    for _ in 0..6 {
        put(&mut state, P0, "Lightning Bolt", Zone::Graveyard);
    }
    let top = state.players[0].library[..7].to_vec();
    let dig = put(&mut state, P0, "Dig Through Time", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U, ManaColor::U], 0);
    cast(&mut state, dig, &[]);
    settle_first(&mut state);
    assert_eq!(
        state.players[0].graveyard,
        vec![dig],
        "{{6}}{{U}}{{U}}: delve exiled all six"
    );
    assert_eq!(state.players[0].hand.len(), 2);
    assert!(state.players[0].hand.iter().all(|card| top.contains(card)));
}

#[test]
fn treasure_cruise_delves_seven_and_draws_three() {
    let mut state = islands();
    for _ in 0..7 {
        put(&mut state, P0, "Island", Zone::Graveyard);
    }
    let cruise = put(&mut state, P0, "Treasure Cruise", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 0);
    cast(&mut state, cruise, &[]);
    settled(&mut state);
    assert_eq!(state.players[0].hand.len(), 3);
    assert_eq!(state.players[0].graveyard, vec![cruise]);
}

#[test]
fn gitaxian_probe_reveals_the_target_hand_to_its_caster_and_draws_for_two_life() {
    let mut state = islands();
    let hand = [
        put(&mut state, P1, "Lightning Bolt", Zone::Hand),
        put(&mut state, P1, "Ponder", Zone::Hand),
    ];
    let probe = put(&mut state, P0, "Gitaxian Probe", Zone::Hand);
    cast(&mut state, probe, &[Target::Player(P1)]);
    settled(&mut state);
    assert_eq!(state.players[0].life, 18, "Phyrexian mana paid with life");
    assert_eq!(state.players[0].hand.len(), 1);
    let known = state.known_hand_cards(P0, P1);
    assert!(hand
        .iter()
        .all(|card| known.iter().any(|entry| entry.object == *card)));
}

#[test]
fn swords_to_plowshares_exiles_a_creature_and_its_controller_gains_its_power() {
    let mut state = islands();
    let borrower = put(&mut state, P1, "Brazen Borrower", Zone::Battlefield);
    let swords = put(&mut state, P0, "Swords to Plowshares", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W], 0);
    cast(&mut state, swords, &[Target::Object(borrower)]);
    settled(&mut state);
    assert_eq!(zone(&state, borrower), Zone::Exile);
    assert_eq!(state.players[1].life, 23);
    assert_eq!(state.players[0].life, 20);
}

#[test]
fn dismember_gives_minus_five_for_phyrexian_mana() {
    let mut state = islands();
    let borrower = put(&mut state, P1, "Brazen Borrower", Zone::Battlefield);
    let dismember = put(&mut state, P0, "Dismember", Zone::Hand);
    pool(&mut state, P0, &[], 1);
    cast(&mut state, dismember, &[Target::Object(borrower)]);
    settled(&mut state);
    assert_eq!(zone(&state, borrower), Zone::Graveyard);
    assert_eq!(
        state.players[0].life, 16,
        "both black symbols paid with life"
    );
}

#[test]
fn prismatic_ending_exiles_up_to_the_colors_spent() {
    // One color: a mana value 2 permanent survives.
    let mut state = islands();
    let snapcaster = put(&mut state, P1, "Snapcaster Mage", Zone::Battlefield);
    let ending = put(&mut state, P0, "Prismatic Ending", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W, ManaColor::W], 0);
    cast(&mut state, ending, &[Target::Object(snapcaster)]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseEffectOption { option_count, .. } => {
            Action::ChooseEffectOption(option_count - 1)
        }
        other => first_answer(other),
    });
    assert_eq!(zone(&state, snapcaster), Zone::Battlefield);
    assert_eq!(zone(&state, ending), Zone::Graveyard);

    // Two colors exile it.
    let mut state = islands();
    let snapcaster = put(&mut state, P1, "Snapcaster Mage", Zone::Battlefield);
    let ending = put(&mut state, P0, "Prismatic Ending", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W, ManaColor::U], 0);
    cast(&mut state, ending, &[Target::Object(snapcaster)]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseEffectOption { option_count, .. } => {
            Action::ChooseEffectOption(option_count - 1)
        }
        other => first_answer(other),
    });
    assert_eq!(zone(&state, snapcaster), Zone::Exile);
}

#[test]
fn force_of_negation_exiles_a_noncreature_spell_and_pitches_on_the_opponents_turn() {
    let mut state = islands();
    opponents_main_phase(&mut state);
    let negation = put(&mut state, P0, "Force of Negation", Zone::Hand);
    let pitch = put(&mut state, P0, "Brainstorm", Zone::Hand);
    let recall = put(&mut state, P1, "Ancestral Recall", Zone::Hand);
    pool(&mut state, P1, &[ManaColor::U], 0);
    cast(&mut state, recall, &[Target::Player(P1)]);
    assert!(matches!(
        next(&mut state),
        Decision::CastSpellOrPass { player: P1, .. }
    ));
    act(&mut state, Action::Pass);
    cast(&mut state, negation, &[Target::Object(recall)]);
    settle_first(&mut state);
    assert_eq!(zone(&state, pitch), Zone::Exile, "the pitched blue card");
    assert_eq!(zone(&state, recall), Zone::Exile, "exiled instead");
    assert_eq!(state.players[1].hand.len(), 0);

    // On its controller's own turn the pitch cost is not offered.
    let mut state = islands();
    let negation = put(&mut state, P0, "Force of Negation", Zone::Hand);
    put(&mut state, P0, "Brainstorm", Zone::Hand);
    let walk = put(&mut state, P0, "Time Walk", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 1);
    cast(&mut state, walk, &[]);
    assert!(!castable(&mut state, negation));
}

#[test]
fn gush_returns_two_islands_instead_of_its_mana_cost_and_draws_two() {
    let mut state = islands();
    let lands = [
        put(&mut state, P0, "Island", Zone::Battlefield),
        put(&mut state, P0, "Island", Zone::Battlefield),
        put(&mut state, P0, "Island", Zone::Battlefield),
    ];
    let gush = put(&mut state, P0, "Gush", Zone::Hand);
    assert!(castable(&mut state, gush));
    act(&mut state, Action::CastSpell(gush));
    let mut picked = Vec::new();
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseCastMode { .. } => Action::ChooseCastMode(CastMode::Alternative),
        Decision::ChooseCostTargets {
            candidates,
            remaining,
            ..
        } => {
            assert!(remaining <= 2);
            picked.push(candidates[0]);
            Action::ChooseCostTarget(candidates[0])
        }
        other => first_answer(other),
    });
    assert_eq!(picked.len(), 2, "two Islands");
    assert_eq!(
        lands
            .iter()
            .filter(|&&land| zone(&state, land) == Zone::Hand)
            .count(),
        2
    );
    assert_eq!(state.players[0].hand.len(), 4, "two Islands and two draws");

    // One Island is not enough for the alternative cost.
    let mut state = islands();
    put(&mut state, P0, "Island", Zone::Battlefield);
    let gush = put(&mut state, P0, "Gush", Zone::Hand);
    assert!(!castable(&mut state, gush));
}

#[test]
fn flame_of_anor_chooses_two_modes_only_with_a_wizard() {
    let mut state = islands();
    let elves = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    let flame = put(&mut state, P0, "Flame of Anor", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U, ManaColor::R], 1);
    act(&mut state, Action::CastSpell(flame));
    match next(&mut state) {
        Decision::ChooseSpellMode { legal_modes, .. } => {
            assert!(legal_modes.iter().all(|&mode| mode < 3), "{legal_modes:?}");
            act(&mut state, Action::ChooseSpellMode(2));
        }
        other => panic!("expected the mode, got {other:?}"),
    }
    choose_target(&mut state, Target::Object(elves));
    settled(&mut state);
    assert_eq!(zone(&state, elves), Zone::Graveyard);

    // With a Wizard: destroy an artifact and deal 5 damage to a creature.
    let mut state = islands();
    put(&mut state, P0, "Snapcaster Mage", Zone::Battlefield);
    let ring = put(&mut state, P1, "Sol Ring", Zone::Battlefield);
    let elves = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    let flame = put(&mut state, P0, "Flame of Anor", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U, ManaColor::R], 1);
    act(&mut state, Action::CastSpell(flame));
    match next(&mut state) {
        Decision::ChooseSpellMode { legal_modes, .. } => {
            assert!(legal_modes.contains(&5), "{legal_modes:?}");
            act(&mut state, Action::ChooseSpellMode(5));
        }
        other => panic!("expected the mode, got {other:?}"),
    }
    choose_target(&mut state, Target::Object(ring));
    choose_target(&mut state, Target::Object(elves));
    settled(&mut state);
    assert_eq!(zone(&state, ring), Zone::Graveyard);
    assert_eq!(zone(&state, elves), Zone::Graveyard);

    // Draw two and destroy an artifact.
    let mut state = islands();
    put(&mut state, P0, "Snapcaster Mage", Zone::Battlefield);
    let ring = put(&mut state, P1, "Sol Ring", Zone::Battlefield);
    let flame = put(&mut state, P0, "Flame of Anor", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U, ManaColor::R], 1);
    act(&mut state, Action::CastSpell(flame));
    match next(&mut state) {
        Decision::ChooseSpellMode { .. } => act(&mut state, Action::ChooseSpellMode(3)),
        other => panic!("expected the mode, got {other:?}"),
    }
    choose_target(&mut state, Target::Player(P0));
    choose_target(&mut state, Target::Object(ring));
    settled(&mut state);
    assert_eq!(state.players[0].hand.len(), 2);
    assert_eq!(zone(&state, ring), Zone::Graveyard);
}

#[test]
fn brain_freeze_mills_three_for_itself_and_each_storm_copy() {
    let mut state = islands();
    let mox = put(&mut state, P0, "Mox Pearl", Zone::Hand);
    cast(&mut state, mox, &[]);
    settled(&mut state);
    let freeze = put(&mut state, P0, "Brain Freeze", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 1);
    cast(&mut state, freeze, &[Target::Player(P1)]);
    settle_first(&mut state);
    assert_eq!(
        state.players[1].graveyard.len(),
        6,
        "the spell and one copy"
    );
    assert_eq!(zone(&state, freeze), Zone::Graveyard);
}

#[test]
fn mana_drain_counters_and_adds_colorless_at_the_next_main_phase() {
    let mut state = islands();
    let drain = put(&mut state, P0, "Mana Drain", Zone::Hand);
    let bolt = opponent_bolts(&mut state, Target::Player(P0));
    pool(&mut state, P0, &[ManaColor::U, ManaColor::U], 0);
    cast(&mut state, drain, &[Target::Object(bolt)]);
    settled(&mut state);
    assert_eq!(zone(&state, bolt), Zone::Graveyard);
    assert_eq!(state.players[0].life, 20);
    assert_eq!(mana(&state, P0, ManaColor::C), 0, "not yet");
    // P0's next main phase is this turn's second one.
    pass_until(&mut state, |state| {
        state.players[0].mana_pool[ManaColor::C.pool_index()] == 1
    });
    assert_eq!(state.step, Step::Main2);
    assert_eq!(state.active_player, P0);
}

#[test]
fn forth_eorlingas_makes_hasty_knights_and_their_combat_damage_makes_the_monarch() {
    let mut state = islands();
    let forth = put(&mut state, P0, "Forth Eorlingas!", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R, ManaColor::W], 2);
    cast(&mut state, forth, &[]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseEffectOption { option_count, .. } => {
            Action::ChooseEffectOption(option_count - 1)
        }
        other => first_answer(other),
    });
    let knights = state.players[0]
        .battlefield
        .iter()
        .copied()
        .filter(|&object| {
            CARD_DEFS[state.objects.get(object).card_def as usize].name == "Human Knight Token"
        })
        .collect::<Vec<_>>();
    assert_eq!(knights.len(), 2, "X is 2");
    assert!(knights.iter().all(|&knight| engine::has_effective_keyword(
        &state,
        knight,
        Keywords::HASTE
    )));
    assert!(state.monarch.is_none());
    attack_with(&mut state, knights);
    assert_eq!(state.players[1].life, 16);
    assert_eq!(state.monarch, Some(P0));
}

// ---- Permanents -----------------------------------------------------------------------

#[test]
fn tundra_taps_for_white_or_blue() {
    for color in [ManaColor::W, ManaColor::U] {
        let mut state = islands();
        let tundra = put(&mut state, P0, "Tundra", Zone::Battlefield);
        next(&mut state);
        act(&mut state, Action::ActivateManaAbilityChoice(tundra, color));
        assert_eq!(mana(&state, P0, color), 1);
    }
}

#[test]
fn meticulous_archive_enters_tapped_and_surveils_one() {
    let mut state = game(&["Lightning Bolt", "Island", "Island", "Island"]);
    let top = state.players[0].library[0];
    let archive = put(&mut state, P0, "Meticulous Archive", Zone::Hand);
    next(&mut state);
    act(&mut state, Action::PlayLand(archive));
    assert!(state.objects.get(archive).tapped);
    match settle(&mut state) {
        Some(Decision::ChooseEffectOption { option_count, .. }) => {
            assert_eq!(option_count, 2);
            act(&mut state, Action::ChooseEffectOption(1));
        }
        other => panic!("expected the surveil, got {other:?}"),
    }
    settle_first(&mut state);
    assert_eq!(zone(&state, top), Zone::Graveyard);
}

#[test]
fn mox_opal_taps_for_any_color_with_metalcraft() {
    let mut state = islands();
    let opal = put(&mut state, P0, "Mox Opal", Zone::Battlefield);
    put(&mut state, P0, "Sol Ring", Zone::Battlefield);
    next(&mut state);
    assert!(engine::step(
        &mut state,
        Action::ActivateManaAbilityChoice(opal, ManaColor::G)
    )
    .is_err());
    put(&mut state, P0, "Mox Jet", Zone::Battlefield);
    next(&mut state);
    act(
        &mut state,
        Action::ActivateManaAbilityChoice(opal, ManaColor::G),
    );
    assert_eq!(mana(&state, P0, ManaColor::G), 1);
}

#[test]
fn null_rod_stops_artifact_abilities_including_mana_abilities() {
    let mut state = islands();
    put(&mut state, P1, "Null Rod", Zone::Battlefield);
    let mox = put(&mut state, P0, "Mox Pearl", Zone::Battlefield);
    let bomb = put(&mut state, P0, "Aether Spellbomb", Zone::Battlefield);
    pool(&mut state, P0, &[], 1);
    assert!(!activatable(&mut state, bomb, 1));
    assert!(engine::step(&mut state, Action::ActivateManaAbility(mox)).is_err());
}

#[test]
fn pithing_needle_names_a_visible_card_and_stops_its_non_mana_abilities() {
    let mut state = islands();
    let mine = put(&mut state, P1, "Strip Mine", Zone::Battlefield);
    let needle = put(&mut state, P0, "Pithing Needle", Zone::Hand);
    pool(&mut state, P0, &[], 1);
    cast(&mut state, needle, &[]);
    match settle(&mut state) {
        Some(Decision::ChooseEffectOption {
            player,
            source,
            option_count,
        }) => {
            assert_eq!((player, source), (P0, needle));
            let options = needle_name_options(&state, P0);
            assert_eq!(usize::from(option_count), options.len());
            let index = options
                .iter()
                .position(|&named| named == id("Strip Mine"))
                .expect("Strip Mine is visible");
            act(&mut state, Action::ChooseEffectOption(index as u16));
        }
        other => panic!("expected the name choice, got {other:?}"),
    }
    settled(&mut state);
    assert_eq!(zone(&state, needle), Zone::Battlefield);
    state.priority_player = P1;
    let (_, abilities) = priority(&mut state);
    assert!(!abilities.contains(&(mine, 0)), "Strip Mine's sacrifice");
    act(&mut state, Action::ActivateManaAbility(mine));
    assert_eq!(
        mana(&state, P1, ManaColor::C),
        1,
        "mana abilities still work"
    );
}

#[test]
fn portable_hole_exiles_a_cheap_opposing_permanent_until_it_leaves() {
    let mut state = islands();
    let snapcaster = put(&mut state, P1, "Snapcaster Mage", Zone::Battlefield);
    let borrower = put(&mut state, P1, "Brazen Borrower", Zone::Battlefield);
    let hole = put(&mut state, P0, "Portable Hole", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W], 0);
    cast(&mut state, hole, &[]);
    for legal in settle_targeting(&mut state, Target::Object(snapcaster)) {
        assert!(!legal.contains(&Target::Object(borrower)), "mana value 3");
    }
    assert_eq!(zone(&state, snapcaster), Zone::Exile);
    assert_eq!(zone(&state, borrower), Zone::Battlefield);
    event::propose_and_commit(&mut state, ProposedEvent::zone_change(hole, Zone::Hand));
    settle_first(&mut state);
    assert_eq!(zone(&state, snapcaster), Zone::Battlefield);
}

#[test]
fn soul_guide_lantern_exiles_a_graveyard_card_then_graveyards_or_draws() {
    let mut state = islands();
    let bolt = put(&mut state, P1, "Lightning Bolt", Zone::Graveyard);
    let ponder = put(&mut state, P1, "Ponder", Zone::Graveyard);
    let lantern = put(&mut state, P0, "Soul-Guide Lantern", Zone::Hand);
    pool(&mut state, P0, &[], 1);
    cast(&mut state, lantern, &[]);
    settle_targeting(&mut state, Target::Object(bolt));
    assert_eq!(zone(&state, bolt), Zone::Exile);
    assert_eq!(zone(&state, ponder), Zone::Graveyard);
    activate(&mut state, lantern, 0, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, ponder), Zone::Exile);
    assert_eq!(zone(&state, lantern), Zone::Graveyard);

    let mut state = islands();
    let lantern = put(&mut state, P0, "Soul-Guide Lantern", Zone::Battlefield);
    pool(&mut state, P0, &[], 1);
    activate(&mut state, lantern, 1, &[]);
    settled(&mut state);
    assert_eq!(state.players[0].hand.len(), 1);
}

#[test]
fn aether_spellbomb_bounces_a_creature_or_draws() {
    let mut state = islands();
    let elves = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    let bomb = put(&mut state, P0, "Aether Spellbomb", Zone::Battlefield);
    pool(&mut state, P0, &[ManaColor::U], 0);
    activate(&mut state, bomb, 0, &[Target::Object(elves)]);
    settled(&mut state);
    assert_eq!(zone(&state, elves), Zone::Hand);
    assert_eq!(zone(&state, bomb), Zone::Graveyard);

    let mut state = islands();
    let bomb = put(&mut state, P0, "Aether Spellbomb", Zone::Battlefield);
    pool(&mut state, P0, &[], 1);
    activate(&mut state, bomb, 1, &[]);
    settled(&mut state);
    assert_eq!(state.players[0].hand.len(), 1);
}

#[test]
fn snapcaster_mage_gives_an_instant_flashback_for_its_mana_cost() {
    let mut state = islands();
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Graveyard);
    let snapcaster = put(&mut state, P0, "Snapcaster Mage", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 1);
    assert!(!castable(&mut state, bolt), "no flashback yet");
    cast(&mut state, snapcaster, &[]);
    for legal in settle_targeting(&mut state, Target::Object(bolt)) {
        assert_eq!(legal, vec![Target::Object(bolt)]);
    }
    assert!(!castable(&mut state, bolt), "{{R}} is not available yet");
    pool(&mut state, P0, &[ManaColor::R], 0);
    cast(&mut state, bolt, &[Target::Player(P1)]);
    settled(&mut state);
    assert_eq!(state.players[1].life, 17);
    assert_eq!(zone(&state, bolt), Zone::Exile, "flashback exiles it");
}

#[test]
fn thundertrap_trainer_takes_a_noncreature_nonland_card_and_offspring_copies_it() {
    let mut state = game(&[
        "Llanowar Elves",
        "Lightning Bolt",
        "Island",
        "Island",
        "Island",
        "Island",
    ]);
    let bolt = state.players[0].library[1];
    let trainer = put(&mut state, P0, "Thundertrap Trainer", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 1);
    cast(&mut state, trainer, &[]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseKicker { .. } => Action::ChooseKicker(false),
        Decision::ChooseEffectTargets { legal_targets, .. }
            if legal_targets.contains(&Target::Object(bolt)) =>
        {
            assert_eq!(legal_targets, vec![Target::Object(bolt)]);
            Action::ChooseEffectTarget(Target::Object(bolt))
        }
        other => first_answer(other),
    });
    assert_eq!(zone(&state, bolt), Zone::Hand);
    assert_eq!(state.players[0].library.len(), 5);

    // Offspring {4}: a 1/1 token copy whose entry looks again.
    let mut state = game(&["Island"; 10]);
    let trainer = put(&mut state, P0, "Thundertrap Trainer", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 5);
    cast(&mut state, trainer, &[]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseKicker { .. } => Action::ChooseKicker(true),
        other => first_answer(other),
    });
    let tokens = state.players[0]
        .battlefield
        .iter()
        .filter(|&&object| {
            CARD_DEFS[state.objects.get(object).card_def as usize].name
                == "Thundertrap Trainer Offspring Token"
        })
        .count();
    assert_eq!(tokens, 1);
}

#[test]
fn psychic_frog_grows_by_discarding_flies_by_exiling_and_draws_on_combat_damage() {
    let mut state = islands();
    let frog = put(&mut state, P0, "Psychic Frog", Zone::Battlefield);
    let card = put(&mut state, P0, "Island", Zone::Hand);
    activate(&mut state, frog, 0, &[]);
    settle_first(&mut state);
    assert_eq!(zone(&state, card), Zone::Graveyard);
    assert_eq!(state.objects.get(frog).counters.plus1_plus1, 1);

    for name in ["Island", "Island"] {
        put(&mut state, P0, name, Zone::Graveyard);
    }
    assert!(!engine::has_effective_keyword(
        &state,
        frog,
        Keywords::FLYING
    ));
    activate(&mut state, frog, 1, &[]);
    settle_first(&mut state);
    assert!(state.players[0].graveyard.is_empty(), "three cards exiled");
    assert!(engine::has_effective_keyword(
        &state,
        frog,
        Keywords::FLYING
    ));

    let mut state = islands();
    let frog = put(&mut state, P0, "Psychic Frog", Zone::Battlefield);
    attack_with(&mut state, vec![frog]);
    assert_eq!(state.players[1].life, 19);
    assert_eq!(state.players[0].hand.len(), 1, "it drew a card");
}

#[test]
fn hexing_squelcher_makes_its_controllers_spells_uncounterable() {
    let mut state = islands();
    put(&mut state, P0, "Hexing Squelcher", Zone::Battlefield);
    let recall = put(&mut state, P0, "Ancestral Recall", Zone::Hand);
    let counterspell = put(&mut state, P1, "Counterspell", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 0);
    pool(&mut state, P1, &[ManaColor::U, ManaColor::U], 0);
    cast(&mut state, recall, &[Target::Player(P0)]);
    assert!(matches!(
        next(&mut state),
        Decision::CastSpellOrPass { player: P0, .. }
    ));
    act(&mut state, Action::Pass);
    if castable(&mut state, counterspell) {
        act(&mut state, Action::CastSpell(counterspell));
        choose_target(&mut state, Target::Object(recall));
    }
    settled(&mut state);
    assert_eq!(state.players[0].hand.len(), 3, "Ancestral Recall resolved");
}

#[test]
fn hexing_squelcher_gives_its_controllers_other_creatures_ward_pay_two_life() {
    for pay in [true, false] {
        let mut state = islands();
        put(&mut state, P0, "Hexing Squelcher", Zone::Battlefield);
        let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
        state.priority_player = P1;
        let bolt = put(&mut state, P1, "Lightning Bolt", Zone::Hand);
        pool(&mut state, P1, &[ManaColor::R], 0);
        cast(&mut state, bolt, &[Target::Object(elves)]);
        settle_with(&mut state, |_, decision| match decision {
            Decision::ChooseEffectBoolean { player, .. } => {
                assert_eq!(player, P1, "the targeting player pays");
                Action::ChooseEffectBoolean(pay)
            }
            other => first_answer(other),
        });
        let (life, elves_zone) = if pay {
            (18, Zone::Graveyard)
        } else {
            (20, Zone::Battlefield)
        };
        assert_eq!(state.players[1].life, life, "pay {pay}");
        assert_eq!(zone(&state, elves), elves_zone, "pay {pay}");
    }
}

#[test]
fn underworld_breach_escapes_nonland_cards_and_is_sacrificed_at_the_end_step() {
    let mut state = islands();
    let breach = put(&mut state, P0, "Underworld Breach", Zone::Battlefield);
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Graveyard);
    let island = put(&mut state, P0, "Island", Zone::Graveyard);
    for _ in 0..3 {
        put(&mut state, P0, "Ponder", Zone::Graveyard);
    }
    pool(&mut state, P0, &[ManaColor::R], 0);
    let (castable_now, _) = priority(&mut state);
    assert!(castable_now.contains(&bolt));
    assert!(!castable_now.contains(&island), "lands have no escape");
    act(&mut state, Action::CastSpell(bolt));
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseTargets { .. } => Action::ChooseTarget(Target::Player(P1)),
        other => first_answer(other),
    });
    assert_eq!(state.players[1].life, 17);
    assert_eq!(zone(&state, bolt), Zone::Graveyard, "escape does not exile");
    assert_eq!(state.players[0].graveyard.len(), 2, "three others exiled");
    pass_until(&mut state, |state| zone(state, breach) == Zone::Graveyard);
    assert_eq!(state.step, Step::End);
}
