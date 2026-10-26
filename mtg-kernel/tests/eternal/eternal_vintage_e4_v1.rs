//! Eternal wave E4: native rules tests for the cards that complete Vintage
//! Jewel Shops and Mono-White Initiative.
#![cfg(feature = "eternal-fixtures")]

use mtg_kernel::card_def::{
    card_id_by_name, CardCapability, CardDef, CardType, Subtype, Supertype, CARD_DEFS,
};
use mtg_kernel::engine::{self, Action, Decision};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::ManaColor;
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Target, Zone};

const P0: PlayerId = PlayerId::P0;
const P1: PlayerId = PlayerId::P1;

const E4_CARDS: [&str; 14] = [
    "Mox Emerald",
    "Mox Jet",
    "Mox Pearl",
    "Mox Ruby",
    "Mox Sapphire",
    "Black Lotus",
    "Sol Ring",
    "Mana Crypt",
    "Snow-Covered Island",
    "Strip Mine",
    "Karakas",
    "Tolarian Academy",
    "Ancestral Recall",
    "Time Walk",
];

fn def(name: &str) -> &'static CardDef {
    &CARD_DEFS[card_id_by_name(name).unwrap_or_else(|| panic!("{name}")) as usize]
}

/// A first main phase with P0 active and holding priority. Each library
/// holds `library` in order, top first.
fn seeded_game(library: &[&str], seed: u64) -> GameState {
    let ids: Vec<u16> = library
        .iter()
        .map(|name| card_id_by_name(name).unwrap_or_else(|| panic!("{name}")))
        .collect();
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
    seeded_game(library, 0x4534_5649_4e54)
}

fn islands() -> GameState {
    game(&["Island"; 20])
}

fn put(state: &mut GameState, player: PlayerId, name: &str, zone: Zone) -> ObjectId {
    let card_def = card_id_by_name(name).unwrap_or_else(|| panic!("{name}"));
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
        attachments: Vec::new(),
        v4: ObjectStateV4::from_card_def(card_def),
        spell_copy_origin: None,
        plotted_turn: None,
        zone_change_count: 0,
    });
    let seat = &mut state.players[player.index()];
    match zone {
        Zone::Hand => seat.hand.push(id),
        Zone::Battlefield => seat.battlefield.push(id),
        Zone::Graveyard => seat.graveyard.push(id),
        _ => panic!("helper zone"),
    }
    id
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

#[test]
fn e4_cards_are_full_with_their_printed_characteristics() {
    for name in E4_CARDS {
        assert_eq!(def(name).capability, CardCapability::Full, "{name}");
    }
    for name in [
        "Mox Emerald",
        "Mox Jet",
        "Mox Pearl",
        "Mox Ruby",
        "Mox Sapphire",
        "Black Lotus",
        "Mana Crypt",
    ] {
        let card = def(name);
        assert_eq!(card.types, &[CardType::Artifact], "{name}");
        assert_eq!(card.mana_value, 0, "{name}");
        assert!(card.colors.is_empty(), "{name}");
    }
    assert_eq!(def("Sol Ring").mana_value, 1);
    let island = def("Snow-Covered Island");
    assert!(island.is_land);
    assert_eq!(island.subtypes, &[Subtype::Island]);
    assert_eq!(island.supertypes, &[Supertype::Basic, Supertype::Snow]);
    for name in ["Karakas", "Tolarian Academy"] {
        assert!(def(name).is_land, "{name}");
        assert_eq!(def(name).supertypes, &[Supertype::Legendary], "{name}");
    }
    assert!(def("Strip Mine").is_land);
    assert_eq!(def("Ancestral Recall").types, &[CardType::Instant]);
    assert_eq!(def("Time Walk").types, &[CardType::Sorcery]);
    assert_eq!(def("Time Walk").mana_value, 2);
}

// ---- Fast mana ------------------------------------------------------------

#[test]
fn moxen_cost_nothing_and_tap_for_their_color() {
    for (name, color) in [
        ("Mox Emerald", ManaColor::G),
        ("Mox Jet", ManaColor::B),
        ("Mox Pearl", ManaColor::W),
        ("Mox Ruby", ManaColor::R),
        ("Mox Sapphire", ManaColor::U),
    ] {
        let mut state = islands();
        let mox = put(&mut state, P0, name, Zone::Hand);
        cast(&mut state, mox, &[]);
        settled(&mut state);
        assert_eq!(state.objects.get(mox).zone, Zone::Battlefield, "{name}");
        act(&mut state, Action::ActivateManaAbility(mox));
        assert_eq!(mana(&state, P0, color), 1, "{name}");
        assert!(state.objects.get(mox).tapped, "{name}");
    }
}

#[test]
fn black_lotus_sacrifices_for_three_mana_of_one_color() {
    let mut state = islands();
    let lotus = put(&mut state, P0, "Black Lotus", Zone::Battlefield);
    next(&mut state);
    act(
        &mut state,
        Action::ActivateManaAbilityChoice(lotus, ManaColor::G),
    );
    assert_eq!(mana(&state, P0, ManaColor::G), 3);
    assert_eq!(state.objects.get(lotus).zone, Zone::Graveyard);
}

#[test]
fn sol_ring_and_mana_crypt_tap_for_two_colorless() {
    for name in ["Sol Ring", "Mana Crypt"] {
        let mut state = islands();
        let rock = put(&mut state, P0, name, Zone::Battlefield);
        next(&mut state);
        act(&mut state, Action::ActivateManaAbility(rock));
        assert_eq!(mana(&state, P0, ManaColor::C), 2, "{name}");
        assert!(state.objects.get(rock).tapped, "{name}");

        // The payment planner counts both mana: one rock pays a {2} spell.
        let mut state = islands();
        let rock = put(&mut state, P0, name, Zone::Battlefield);
        let wellspring = put(&mut state, P0, "Ichor Wellspring", Zone::Hand);
        cast(&mut state, wellspring, &[]);
        settled(&mut state);
        assert_eq!(
            state.objects.get(wellspring).zone,
            Zone::Battlefield,
            "{name}"
        );
        assert!(state.objects.get(rock).tapped, "{name}");
        assert_eq!(state.players[0].mana_pool, [0; 6], "{name}");
    }
}

#[test]
fn mana_crypt_flips_a_coin_each_upkeep_and_deals_three_on_a_loss() {
    let mut outcomes = Vec::new();
    for seed in 1..=24_u64 {
        let mut state = seeded_game(&["Island"; 20], seed);
        put(&mut state, P0, "Mana Crypt", Zone::Battlefield);
        pass_until(&mut state, |state| {
            state.active_player == P0 && state.step == Step::Main1 && state.turn >= 2
        });
        let life = state.players[0].life;
        assert!(life == 20 || life == 17, "seed {seed}: life {life}");
        assert_eq!(state.players[1].life, 20, "seed {seed}");
        outcomes.push(life);
    }
    assert!(outcomes.contains(&20), "some flips are won");
    assert!(outcomes.contains(&17), "some flips are lost");
}

#[test]
fn tolarian_academy_adds_blue_for_each_artifact() {
    let mut state = islands();
    let academy = put(&mut state, P0, "Tolarian Academy", Zone::Battlefield);
    for _ in 0..3 {
        put(&mut state, P0, "Mishra's Bauble", Zone::Battlefield);
    }
    next(&mut state);
    act(&mut state, Action::ActivateManaAbility(academy));
    assert_eq!(mana(&state, P0, ManaColor::U), 3);

    // No artifact: the planner offers nothing from it.
    let mut state = islands();
    put(&mut state, P0, "Tolarian Academy", Zone::Battlefield);
    let recall = put(&mut state, P0, "Ancestral Recall", Zone::Hand);
    assert!(!castable(&mut state, recall));
    // One artifact pays {U}.
    let mut state = islands();
    let academy = put(&mut state, P0, "Tolarian Academy", Zone::Battlefield);
    put(&mut state, P0, "Mishra's Bauble", Zone::Battlefield);
    let recall = put(&mut state, P0, "Ancestral Recall", Zone::Hand);
    cast(&mut state, recall, &[Target::Player(P0)]);
    settled(&mut state);
    assert!(state.objects.get(academy).tapped);
    assert_eq!(state.players[0].hand.len(), 3);
}

// ---- Lands ----------------------------------------------------------------

#[test]
fn snow_covered_island_is_a_basic_snow_island() {
    let mut state = islands();
    let island = put(&mut state, P0, "Snow-Covered Island", Zone::Hand);
    next(&mut state);
    act(&mut state, Action::PlayLand(island));
    assert!(engine::has_effective_subtype(
        &state,
        island,
        Subtype::Island
    ));
    next(&mut state);
    act(&mut state, Action::ActivateManaAbility(island));
    assert_eq!(mana(&state, P0, ManaColor::U), 1);
}

#[test]
fn strip_mine_taps_for_colorless_and_destroys_any_land() {
    let mut state = islands();
    let mine = put(&mut state, P0, "Strip Mine", Zone::Battlefield);
    next(&mut state);
    act(&mut state, Action::ActivateManaAbility(mine));
    assert_eq!(mana(&state, P0, ManaColor::C), 1);

    let mut state = islands();
    let mine = put(&mut state, P0, "Strip Mine", Zone::Battlefield);
    let island = put(&mut state, P1, "Island", Zone::Battlefield);
    activate(&mut state, mine, 0, &[Target::Object(island)]);
    settled(&mut state);
    assert_eq!(state.objects.get(mine).zone, Zone::Graveyard);
    assert_eq!(state.objects.get(island).zone, Zone::Graveyard);
}

#[test]
fn karakas_taps_for_white_and_returns_a_legendary_creature() {
    let mut state = islands();
    let karakas = put(&mut state, P0, "Karakas", Zone::Battlefield);
    next(&mut state);
    act(&mut state, Action::ActivateManaAbility(karakas));
    assert_eq!(mana(&state, P0, ManaColor::W), 1);

    let mut state = islands();
    let karakas = put(&mut state, P0, "Karakas", Zone::Battlefield);
    let thalia = put(
        &mut state,
        P1,
        "Thalia, Guardian of Thraben",
        Zone::Battlefield,
    );
    let bears = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    assert!(activatable(&mut state, karakas, 0));
    act(&mut state, Action::ActivateAbility(karakas, 0));
    match next(&mut state) {
        Decision::ChooseTargets { legal_targets, .. } => {
            assert!(legal_targets.contains(&Target::Object(thalia)));
            assert!(!legal_targets.contains(&Target::Object(bears)));
            act(&mut state, Action::ChooseTarget(Target::Object(thalia)));
        }
        other => panic!("expected the target, got {other:?}"),
    }
    settled(&mut state);
    assert_eq!(state.objects.get(thalia).zone, Zone::Hand);
    assert!(state.players[1].hand.contains(&thalia));
    assert!(state.objects.get(karakas).tapped);
}

// ---- Power ----------------------------------------------------------------

#[test]
fn ancestral_recall_has_target_player_draw_three() {
    let mut state = islands();
    let recall = put(&mut state, P0, "Ancestral Recall", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 0);
    cast(&mut state, recall, &[Target::Player(P1)]);
    settled(&mut state);
    assert_eq!(state.players[1].hand.len(), 3);
    assert_eq!(state.players[0].hand.len(), 0);
    assert_eq!(state.objects.get(recall).zone, Zone::Graveyard);
}

#[test]
fn time_walk_takes_an_extra_turn() {
    let mut state = islands();
    let walk = put(&mut state, P0, "Time Walk", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 1);
    cast(&mut state, walk, &[]);
    settled(&mut state);
    assert_eq!(state.objects.get(walk).zone, Zone::Graveyard);
    assert!(state.extra_turn_v1.is_some());
    // P0's extra turn: its draw step draws the first card of P0's hand.
    pass_until(&mut state, |state| {
        state.step == Step::Main1 && state.players[0].hand.len() == 1
    });
    assert_eq!(state.active_player, P0, "the extra turn is P0's");
    assert_eq!(state.turn, 1, "an extra turn keeps the round");
    pass_until(&mut state, |state| {
        state.active_player == P1 && state.step == Step::Main1
    });
    assert_eq!(state.players[1].hand.len(), 1, "then P1's regular turn");
}
