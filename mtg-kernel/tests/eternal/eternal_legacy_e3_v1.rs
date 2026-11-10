//! Eternal wave E3: native rules tests for the cards that complete Legacy
//! Doomsday, Sneak and Show, Aluren and Omni-Tell.
#![cfg(feature = "eternal-fixtures")]

use mtg_kernel::card_def::{card_id_by_name, CardCapability, CardDef, CardType, CARD_DEFS};
use mtg_kernel::engine::{self, Action, Decision};
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::ManaColor;
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Target, Zone};

const P0: PlayerId = PlayerId::P0;
const P1: PlayerId = PlayerId::P1;

const E3_CARDS: [&str; 27] = [
    "Ancient Tomb",
    "The One Ring",
    "Chrome Mox",
    "Misdirection",
    "Sink into Stupor",
    "Cavern of Souls",
    "Tropical Island",
    "Hedge Maze",
    "Dark Ritual",
    "City of Traitors",
    "Street Wraith",
    "Edge of Autumn",
    "Personal Tutor",
    "Veil of Summer",
    "Flusterstorm",
    "Doomsday",
    "Thassa's Oracle",
    "Jace, Wielder of Mysteries",
    "Lion's Eye Diamond",
    "Show and Tell",
    "Sneak Attack",
    "Omniscience",
    "Aluren",
    "Emrakul, the Aeons Torn",
    "Atraxa, Grand Unifier",
    "Raph & Mikey, Troublemakers",
    "Acererak the Archlich",
];

fn def(name: &str) -> &'static CardDef {
    &CARD_DEFS[card_id_by_name(name).unwrap_or_else(|| panic!("{name}")) as usize]
}

/// A first main phase with P0 active and holding priority. Each library
/// holds `library` in order, top first.
fn game(library: &[&str]) -> GameState {
    let ids: Vec<u16> = library
        .iter()
        .map(|name| card_id_by_name(name).unwrap_or_else(|| panic!("{name}")))
        .collect();
    let mut state = GameState::new_from_libraries(
        &ids,
        &ids,
        |id| CARD_DEFS[id as usize].object_name.into(),
        0x4533_4c45_4741,
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
    match zone {
        Zone::Hand => state.players[player.index()].hand.push(id),
        Zone::Battlefield => state.players[player.index()].battlefield.push(id),
        Zone::Graveyard => state.players[player.index()].graveyard.push(id),
        Zone::Exile => state.exile.push(id),
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

fn mana(state: &GameState, player: PlayerId, color: ManaColor) -> u8 {
    state.players[player.index()].mana_pool[color.pool_index()]
}

#[test]
fn e3_cards_are_full_with_their_printed_characteristics() {
    for name in E3_CARDS {
        assert_eq!(def(name).capability, CardCapability::Full, "{name}");
    }
    assert!(def("Ancient Tomb").is_land);
    assert!(def("Cavern of Souls").is_land);
    assert!(def("The One Ring")
        .keywords
        .has(mtg_kernel::card_def::Keywords::INDESTRUCTIBLE));
    assert_eq!(def("Chrome Mox").mana_value, 0);
    assert_eq!(def("Misdirection").mana_value, 5);
    let sink = def("Sink into Stupor");
    assert!(sink.has_type(CardType::Instant));
    assert!(!sink.is_land);
    let springs = sink.transform_face.as_ref().expect("its land face");
    assert_eq!(springs.name, "Soporific Springs");
    assert_eq!(springs.types, &[CardType::Land]);
}

// ---- Ancient Tomb ----------------------------------------------------------------

#[test]
fn ancient_tomb_adds_two_colorless_and_deals_two_damage_to_its_controller() {
    let mut state = islands();
    let tomb = put(&mut state, P0, "Ancient Tomb", Zone::Battlefield);
    next(&mut state);
    act(&mut state, Action::ActivateManaAbility(tomb));
    assert_eq!(mana(&state, P0, ManaColor::C), 2);
    assert_eq!(state.players[0].life, 18);
    assert!(state.objects.get(tomb).tapped);
}

// ---- The One Ring ------------------------------------------------------------------

#[test]
fn the_one_ring_cast_grants_protection_from_everything_until_the_next_turn() {
    let mut state = islands();
    let ring = put(&mut state, P0, "The One Ring", Zone::Hand);
    pool(&mut state, P0, &[], 4);
    cast(&mut state, ring, &[]);
    settled(&mut state);
    assert_eq!(state.objects.get(ring).zone, Zone::Battlefield);

    // P1 can't target P0, and damage to P0 is prevented.
    state.priority_player = P1;
    let bolt = put(&mut state, P1, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P1, &[ManaColor::R], 0);
    assert!(castable(&mut state, bolt));
    act(&mut state, Action::CastSpell(bolt));
    match next(&mut state) {
        Decision::ChooseTargets { legal_targets, .. } => {
            assert!(!legal_targets.contains(&Target::Player(P0)));
            assert!(legal_targets.contains(&Target::Player(P1)));
        }
        other => panic!("expected targets, got {other:?}"),
    }
    event::propose_and_commit(
        &mut state,
        ProposedEvent::damage(ring, Target::Player(P0), 3),
    );
    assert_eq!(state.players[0].life, 20, "damage to P0 is prevented");

    // The protection ends as P0's next turn begins.
    let mut state = islands();
    let ring = put(&mut state, P0, "The One Ring", Zone::Hand);
    pool(&mut state, P0, &[], 4);
    cast(&mut state, ring, &[]);
    settled(&mut state);
    pass_until(&mut state, |state| state.active_player == P1);
    pass_until(&mut state, |state| {
        state.active_player == P0 && state.step == Step::Main1
    });
    event::propose_and_commit(
        &mut state,
        ProposedEvent::damage(ring, Target::Player(P0), 3),
    );
    assert_eq!(state.players[0].life, 17);
}

#[test]
fn the_one_ring_put_onto_the_battlefield_gives_no_protection() {
    let mut state = islands();
    let ring = put(&mut state, P0, "The One Ring", Zone::Hand);
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(ring, Zone::Battlefield),
    );
    settled(&mut state);
    event::propose_and_commit(
        &mut state,
        ProposedEvent::damage(ring, Target::Player(P0), 3),
    );
    assert_eq!(state.players[0].life, 17, "it was not cast");
}

#[test]
fn the_one_ring_draws_per_burden_counter_and_drains_each_upkeep() {
    let mut state = islands();
    let ring = put(&mut state, P0, "The One Ring", Zone::Battlefield);
    activate(&mut state, ring, 0, &[]);
    settled(&mut state);
    assert_eq!(state.players[0].hand.len(), 1, "one burden counter");
    assert!(state.objects.get(ring).tapped);
    // P0's next upkeep: lose 1 life for the one counter.
    pass_until(&mut state, |state| state.active_player == P1);
    pass_until(&mut state, |state| {
        state.active_player == P0 && state.step == Step::Main1
    });
    assert_eq!(state.players[0].life, 19);
    let hand = state.players[0].hand.len();
    activate(&mut state, ring, 0, &[]);
    settled(&mut state);
    assert_eq!(state.players[0].hand.len(), hand + 2, "two burden counters");
}

// ---- Chrome Mox ----------------------------------------------------------------------

#[test]
fn chrome_mox_imprints_a_nonartifact_nonland_card_and_taps_for_its_colors() {
    let mut state = islands();
    let mox = put(&mut state, P0, "Chrome Mox", Zone::Hand);
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    put(&mut state, P0, "Island", Zone::Hand);
    cast(&mut state, mox, &[]);
    match settle(&mut state) {
        Some(Decision::ChooseEffectTargets {
            legal_targets,
            min_targets,
            max_targets,
            ..
        }) => {
            assert_eq!(legal_targets, vec![Target::Object(bolt)], "no land");
            assert_eq!((min_targets, max_targets), (0, 1));
            act(&mut state, Action::ChooseEffectTarget(Target::Object(bolt)));
        }
        other => panic!("expected the imprint, got {other:?}"),
    }
    settled(&mut state);
    assert_eq!(state.objects.get(bolt).zone, Zone::Exile);
    next(&mut state);
    act(&mut state, Action::ActivateManaAbility(mox));
    assert_eq!(mana(&state, P0, ManaColor::R), 1);
}

#[test]
fn chrome_mox_without_an_imprint_adds_nothing() {
    let mut state = islands();
    let mox = put(&mut state, P0, "Chrome Mox", Zone::Hand);
    put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    cast(&mut state, mox, &[]);
    match settle(&mut state) {
        Some(Decision::ChooseEffectTargets { can_finish, .. }) => {
            assert!(can_finish);
            act(&mut state, Action::FinishEffectSelection);
        }
        other => panic!("expected the imprint, got {other:?}"),
    }
    settled(&mut state);
    next(&mut state);
    assert!(engine::step(&mut state, Action::ActivateManaAbility(mox)).is_err());
}

// ---- Misdirection ----------------------------------------------------------------------

#[test]
fn misdirection_pitches_a_blue_card_and_redirects_a_single_target_spell() {
    let mut state = islands();
    let misdirection = put(&mut state, P0, "Misdirection", Zone::Hand);
    let brainstorm = put(&mut state, P0, "Brainstorm", Zone::Hand);
    let bolt = opponent_bolts(&mut state, Target::Player(P0));
    cast(&mut state, misdirection, &[Target::Object(bolt)]);
    priority(&mut state);
    assert_eq!(state.objects.get(brainstorm).zone, Zone::Exile);
    match settle(&mut state) {
        Some(Decision::ChooseEffectTargets { legal_targets, .. }) => {
            assert!(!legal_targets.contains(&Target::Player(P0)), "a new target");
            assert!(legal_targets.contains(&Target::Player(P1)));
            act(&mut state, Action::ChooseEffectTarget(Target::Player(P1)));
        }
        other => panic!("expected the new target, got {other:?}"),
    }
    settled(&mut state);
    assert_eq!(state.players[0].life, 20);
    assert_eq!(state.players[1].life, 17);
}

// ---- Sink into Stupor // Soporific Springs -------------------------------------------------

#[test]
fn sink_into_stupor_returns_an_opponents_spell_or_nonland_permanent() {
    let mut state = islands();
    let sink = put(&mut state, P0, "Sink into Stupor", Zone::Hand);
    let strix = put(&mut state, P1, "Baleful Strix", Zone::Battlefield);
    put(&mut state, P0, "Baleful Strix", Zone::Battlefield);
    pool(&mut state, P0, &[ManaColor::U, ManaColor::U], 1);
    cast(&mut state, sink, &[Target::Object(strix)]);
    settled(&mut state);
    assert_eq!(state.objects.get(strix).zone, Zone::Hand);
    assert!(state.players[1].hand.contains(&strix));

    let mut state = islands();
    let sink = put(&mut state, P0, "Sink into Stupor", Zone::Hand);
    let bolt = opponent_bolts(&mut state, Target::Player(P0));
    pool(&mut state, P0, &[ManaColor::U, ManaColor::U], 1);
    cast(&mut state, sink, &[Target::Object(bolt)]);
    settled(&mut state);
    assert_eq!(state.objects.get(bolt).zone, Zone::Hand);
    assert!(state.players[1].hand.contains(&bolt));
    assert_eq!(state.players[0].life, 20);
}

#[test]
fn soporific_springs_is_played_paying_three_life_or_enters_tapped() {
    for pay in [true, false] {
        let mut state = islands();
        let sink = put(&mut state, P0, "Sink into Stupor", Zone::Hand);
        next(&mut state);
        act(&mut state, Action::PlayLand(sink));
        match next(&mut state) {
            Decision::ChooseEffectOption {
                source,
                option_count,
                ..
            } => {
                assert_eq!((source, option_count), (sink, 2));
                act(&mut state, Action::ChooseEffectOption(u16::from(pay)));
            }
            other => panic!("expected the life payment, got {other:?}"),
        }
        let land = state.objects.get(sink);
        assert_eq!(land.zone, Zone::Battlefield);
        assert_eq!(land.name, "Soporific Springs");
        assert!(engine::object_has_type(&state, sink, CardType::Land));
        assert_eq!(engine::object_mana_value(&state, sink), 0);
        assert_eq!(land.tapped, !pay);
        assert_eq!(state.players[0].life, if pay { 17 } else { 20 });
        assert_eq!(state.players[0].lands_played_this_turn, 1);
        if pay {
            next(&mut state);
            act(&mut state, Action::ActivateManaAbility(sink));
            assert_eq!(mana(&state, P0, ManaColor::U), 1);
        }
    }
}

#[test]
fn soporific_springs_offers_only_entering_tapped_below_three_life() {
    let mut state = islands();
    state.players[0].life = 2;
    let sink = put(&mut state, P0, "Sink into Stupor", Zone::Hand);
    next(&mut state);
    act(&mut state, Action::PlayLand(sink));
    match next(&mut state) {
        Decision::ChooseEffectOption {
            source,
            option_count,
            ..
        } => {
            assert_eq!((source, option_count), (sink, 1));
            act(&mut state, Action::ChooseEffectOption(0));
        }
        other => panic!("expected the life payment, got {other:?}"),
    }
    assert!(state.objects.get(sink).tapped);
    assert_eq!(state.players[0].life, 2);
}

// ---- Cavern of Souls --------------------------------------------------------------------

#[test]
fn cavern_of_souls_names_a_type_and_its_creature_spells_cant_be_countered() {
    let mut state = islands();
    let cavern = put(&mut state, P0, "Cavern of Souls", Zone::Hand);
    let miscreant = put(&mut state, P0, "Faerie Miscreant", Zone::Hand);
    next(&mut state);
    act(&mut state, Action::PlayLand(cavern));
    let options = mtg_kernel::eternal_cards_v1::e3::cavern_type_options(&state, P0);
    let faerie = options
        .iter()
        .position(|subtype| subtype.same_subtype_as(mtg_kernel::card_def::Subtype::Faerie))
        .expect("Faerie is offered");
    match next(&mut state) {
        Decision::ChooseEffectOption {
            source,
            option_count,
            ..
        } => {
            assert_eq!(source, cavern);
            assert_eq!(usize::from(option_count), options.len());
            act(
                &mut state,
                Action::ChooseEffectOption(u16::try_from(faerie).unwrap()),
            );
        }
        other => panic!("expected the creature type, got {other:?}"),
    }
    // Cavern's restricted blue mana pays for the Faerie.
    cast(&mut state, miscreant, &[]);
    priority(&mut state);
    assert!(state.objects.get(cavern).tapped);
    act(&mut state, Action::Pass);
    let counterspell = put(&mut state, P1, "Counterspell", Zone::Hand);
    pool(&mut state, P1, &[ManaColor::U, ManaColor::U], 0);
    cast(&mut state, counterspell, &[Target::Object(miscreant)]);
    settled(&mut state);
    assert_eq!(
        state.objects.get(miscreant).zone,
        Zone::Battlefield,
        "it can't be countered"
    );
    assert_eq!(state.objects.get(counterspell).zone, Zone::Graveyard);
}

#[test]
fn cavern_of_souls_colored_mana_pays_only_for_its_chosen_type() {
    let mut state = islands();
    let cavern = put(&mut state, P0, "Cavern of Souls", Zone::Hand);
    let strix = put(&mut state, P0, "Harrier Strix", Zone::Hand);
    put(&mut state, P0, "Faerie Miscreant", Zone::Hand);
    next(&mut state);
    act(&mut state, Action::PlayLand(cavern));
    let options = mtg_kernel::eternal_cards_v1::e3::cavern_type_options(&state, P0);
    let faerie = options
        .iter()
        .position(|subtype| subtype.same_subtype_as(mtg_kernel::card_def::Subtype::Faerie))
        .expect("Faerie is offered");
    assert!(matches!(
        next(&mut state),
        Decision::ChooseEffectOption { .. }
    ));
    act(
        &mut state,
        Action::ChooseEffectOption(u16::try_from(faerie).unwrap()),
    );
    assert!(!castable(&mut state, strix), "a Bird needs other blue mana");
    next(&mut state);
    act(
        &mut state,
        Action::ActivateManaAbilityChoice(cavern, ManaColor::C),
    );
    assert_eq!(mana(&state, P0, ManaColor::C), 1, "{{T}}: Add {{C}}");
}

/// Settles, answering every library or surveil selection by finishing with
/// whatever is already selected and every option with the first.
fn settle_declining_selections(state: &mut GameState) {
    while let Some(decision) = settle(state) {
        match decision {
            Decision::ChooseEffectTargets {
                can_finish: true, ..
            } => act(state, Action::FinishEffectSelection),
            Decision::ChooseEffectTargets { legal_targets, .. } => {
                act(state, Action::ChooseEffectTarget(legal_targets[0]))
            }
            Decision::ChooseEffectOption { .. } => act(state, Action::ChooseEffectOption(0)),
            Decision::ChooseEffectBoolean { .. } => act(state, Action::ChooseEffectBoolean(false)),
            other => panic!("unexpected choice: {other:?}"),
        }
    }
}

// ---- Tropical Island, Hedge Maze ------------------------------------------------------

#[test]
fn tropical_island_and_hedge_maze_are_forest_islands() {
    use mtg_kernel::card_def::Subtype;
    for name in ["Tropical Island", "Hedge Maze"] {
        assert_eq!(def(name).subtypes, &[Subtype::Forest, Subtype::Island]);
    }
    assert!(!def("Tropical Island").enters_battlefield_tapped);
    assert!(def("Hedge Maze").enters_battlefield_tapped);
    let mut state = islands();
    let tropical = put(&mut state, P0, "Tropical Island", Zone::Battlefield);
    next(&mut state);
    act(
        &mut state,
        Action::ActivateManaAbilityChoice(tropical, ManaColor::G),
    );
    assert_eq!(mana(&state, P0, ManaColor::G), 1);
}

#[test]
fn hedge_maze_enters_tapped_and_surveils_one() {
    let mut state = game(&["Lightning Bolt", "Island", "Island", "Island"]);
    let top = state.players[0].library[0];
    let maze = put(&mut state, P0, "Hedge Maze", Zone::Hand);
    next(&mut state);
    act(&mut state, Action::PlayLand(maze));
    assert!(state.objects.get(maze).tapped);
    match settle(&mut state) {
        Some(Decision::ChooseEffectOption { option_count, .. }) => {
            assert_eq!(option_count, 2);
            act(&mut state, Action::ChooseEffectOption(1));
        }
        other => panic!("expected the surveil, got {other:?}"),
    }
    settle_declining_selections(&mut state);
    assert_eq!(state.objects.get(top).zone, Zone::Graveyard);
}

// ---- Dark Ritual -----------------------------------------------------------------------

#[test]
fn dark_ritual_adds_three_black_mana() {
    let mut state = islands();
    let ritual = put(&mut state, P0, "Dark Ritual", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 0);
    cast(&mut state, ritual, &[]);
    settled(&mut state);
    assert_eq!(mana(&state, P0, ManaColor::B), 3);
}

// ---- City of Traitors ------------------------------------------------------------------

#[test]
fn city_of_traitors_adds_two_colorless_and_is_sacrificed_when_another_land_is_played() {
    let mut state = islands();
    let city = put(&mut state, P0, "City of Traitors", Zone::Battlefield);
    next(&mut state);
    act(&mut state, Action::ActivateManaAbility(city));
    assert_eq!(mana(&state, P0, ManaColor::C), 2);
    let island = put(&mut state, P0, "Island", Zone::Hand);
    next(&mut state);
    act(&mut state, Action::PlayLand(island));
    settled(&mut state);
    assert_eq!(state.objects.get(city).zone, Zone::Graveyard);
    assert_eq!(state.objects.get(island).zone, Zone::Battlefield);
}

#[test]
fn city_of_traitors_survives_lands_put_onto_the_battlefield_and_its_own_play() {
    let mut state = islands();
    let city = put(&mut state, P0, "City of Traitors", Zone::Hand);
    next(&mut state);
    act(&mut state, Action::PlayLand(city));
    settled(&mut state);
    assert_eq!(
        state.objects.get(city).zone,
        Zone::Battlefield,
        "not another land"
    );
    let island = put(&mut state, P0, "Island", Zone::Hand);
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(island, Zone::Battlefield),
    );
    settled(&mut state);
    assert_eq!(
        state.objects.get(city).zone,
        Zone::Battlefield,
        "not played"
    );
}

// ---- Street Wraith ------------------------------------------------------------------------

#[test]
fn street_wraith_cycles_for_two_life() {
    let mut state = islands();
    let wraith = put(&mut state, P0, "Street Wraith", Zone::Hand);
    activate(&mut state, wraith, 0, &[]);
    settled(&mut state);
    assert_eq!(state.objects.get(wraith).zone, Zone::Graveyard);
    assert_eq!(state.players[0].life, 18);
    assert_eq!(state.players[0].hand.len(), 1);
}

#[test]
fn street_wraith_has_swampwalk() {
    for defender_has_swamp in [true, false] {
        let mut state = islands();
        let wraith = put(&mut state, P0, "Street Wraith", Zone::Battlefield);
        let blocker = put(&mut state, P1, "Baleful Strix", Zone::Battlefield);
        if defender_has_swamp {
            put(&mut state, P1, "Swamp", Zone::Battlefield);
        }
        pass_until(&mut state, |state| state.step == Step::DeclareAttackers);
        match next(&mut state) {
            Decision::DeclareAttackers { .. } => {
                act(&mut state, Action::DeclareAttackers(vec![wraith]))
            }
            other => panic!("expected attacks, got {other:?}"),
        }
        let mut blockable = false;
        for _ in 0..20 {
            match next(&mut state) {
                Decision::DeclareBlockers { legal_blockers, .. } => {
                    blockable = legal_blockers.iter().any(|(attacker, blockers)| {
                        *attacker == wraith && blockers.contains(&blocker)
                    });
                    break;
                }
                Decision::CastSpellOrPass { .. } => act(&mut state, Action::Pass),
                Decision::OrderTriggers { pending, .. } => act(
                    &mut state,
                    Action::OrderTriggers((0..pending.len()).collect()),
                ),
                _ => break,
            }
        }
        assert_eq!(blockable, !defender_has_swamp);
    }
}

// ---- Edge of Autumn ------------------------------------------------------------------------

#[test]
fn edge_of_autumn_fetches_a_tapped_basic_with_four_or_fewer_lands() {
    let mut state = game(&["Island", "Forest", "Island", "Island", "Island"]);
    let forest = state.players[0].library[1];
    for _ in 0..4 {
        put(&mut state, P0, "Island", Zone::Battlefield);
    }
    let edge = put(&mut state, P0, "Edge of Autumn", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 1);
    cast(&mut state, edge, &[]);
    match settle(&mut state) {
        Some(Decision::ChooseEffectTargets { legal_targets, .. }) => {
            assert!(legal_targets.contains(&Target::Object(forest)));
            act(
                &mut state,
                Action::ChooseEffectTarget(Target::Object(forest)),
            );
        }
        other => panic!("expected the search, got {other:?}"),
    }
    settle_declining_selections(&mut state);
    assert_eq!(state.objects.get(forest).zone, Zone::Battlefield);
    assert!(state.objects.get(forest).tapped);

    // Five lands: nothing happens.
    let mut state = game(&["Forest", "Island", "Island"]);
    for _ in 0..5 {
        put(&mut state, P0, "Island", Zone::Battlefield);
    }
    let edge = put(&mut state, P0, "Edge of Autumn", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 1);
    cast(&mut state, edge, &[]);
    settled(&mut state);
    assert_eq!(state.players[0].library.len(), 3);
}

#[test]
fn edge_of_autumn_cycles_by_sacrificing_a_land() {
    let mut state = islands();
    let land = put(&mut state, P0, "Forest", Zone::Battlefield);
    let edge = put(&mut state, P0, "Edge of Autumn", Zone::Hand);
    activate(&mut state, edge, 0, &[]);
    settle_declining_selections(&mut state);
    assert_eq!(state.objects.get(land).zone, Zone::Graveyard);
    assert_eq!(state.objects.get(edge).zone, Zone::Graveyard);
    assert_eq!(state.players[0].hand.len(), 1);
}

// ---- Personal Tutor ---------------------------------------------------------------------------

#[test]
fn personal_tutor_puts_a_revealed_sorcery_on_top() {
    let mut state = game(&["Island", "Island", "Ponder", "Island", "Lightning Bolt"]);
    let ponder = state.players[0].library[2];
    let tutor = put(&mut state, P0, "Personal Tutor", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 0);
    cast(&mut state, tutor, &[]);
    match settle(&mut state) {
        Some(Decision::ChooseEffectTargets { legal_targets, .. }) => {
            assert_eq!(
                legal_targets,
                vec![Target::Object(ponder)],
                "sorceries only"
            );
            act(
                &mut state,
                Action::ChooseEffectTarget(Target::Object(ponder)),
            );
        }
        other => panic!("expected the search, got {other:?}"),
    }
    settle_declining_selections(&mut state);
    assert_eq!(state.players[0].library[0], ponder);
    assert!(state
        .known_library_cards(P1, P0)
        .iter()
        .any(|entry| entry.position == 0 && entry.object == ponder));
}

// ---- Veil of Summer ------------------------------------------------------------------------------

#[test]
fn veil_of_summer_draws_after_a_blue_spell_and_protects_its_controllers_spells() {
    let mut state = islands();
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    let veil = put(&mut state, P0, "Veil of Summer", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R, ManaColor::G], 0);
    cast(&mut state, bolt, &[Target::Player(P1)]);
    priority(&mut state);
    act(&mut state, Action::Pass);
    // P1 answers with a blue Counterspell; P0 responds with Veil.
    let counterspell = put(&mut state, P1, "Counterspell", Zone::Hand);
    pool(&mut state, P1, &[ManaColor::U, ManaColor::U], 0);
    cast(&mut state, counterspell, &[Target::Object(bolt)]);
    priority(&mut state);
    act(&mut state, Action::Pass);
    let hand = state.players[0].hand.len();
    cast(&mut state, veil, &[]);
    settled(&mut state);
    assert_eq!(
        state.players[0].hand.len(),
        hand,
        "Veil left the hand and a card was drawn for the blue spell"
    );
    assert_eq!(state.players[1].life, 17, "the Bolt can't be countered");
    assert_eq!(state.objects.get(counterspell).zone, Zone::Graveyard);
}

#[test]
fn veil_of_summer_gives_hexproof_from_blue_and_black_this_turn() {
    let mut state = islands();
    let veil = put(&mut state, P0, "Veil of Summer", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 0);
    cast(&mut state, veil, &[]);
    settled(&mut state);
    assert_eq!(state.players[0].hand.len(), 0, "no blue or black spell yet");
    // A blue Thought Scour from P1 can't target P0; a red Bolt can.
    state.priority_player = P1;
    let scour = put(&mut state, P1, "Thought Scour", Zone::Hand);
    let bolt = put(&mut state, P1, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P1, &[ManaColor::U, ManaColor::R], 0);
    act(&mut state, Action::CastSpell(scour));
    match next(&mut state) {
        Decision::ChooseTargets { legal_targets, .. } => {
            assert_eq!(legal_targets, vec![Target::Player(P1)]);
            act(&mut state, Action::ChooseTarget(Target::Player(P1)));
        }
        other => panic!("expected targets, got {other:?}"),
    }
    settle_declining_selections(&mut state);
    state.priority_player = P1;
    act(&mut state, Action::CastSpell(bolt));
    match next(&mut state) {
        Decision::ChooseTargets { legal_targets, .. } => {
            assert!(legal_targets.contains(&Target::Player(P0)));
        }
        other => panic!("expected targets, got {other:?}"),
    }
}

// ---- Flusterstorm ---------------------------------------------------------------------------------

#[test]
fn flusterstorm_copies_itself_for_each_earlier_spell() {
    let mut state = islands();
    let flusterstorm = put(&mut state, P0, "Flusterstorm", Zone::Hand);
    let bolt = opponent_bolts(&mut state, Target::Player(P0));
    pool(&mut state, P0, &[ManaColor::U], 0);
    cast(&mut state, flusterstorm, &[Target::Object(bolt)]);
    let mut retargets = 0;
    loop {
        match settle(&mut state) {
            Some(Decision::ChooseEffectTargets {
                legal_targets,
                can_finish,
                ..
            }) => {
                assert!(can_finish, "a copy may keep its target");
                assert!(legal_targets.contains(&Target::Object(bolt)));
                retargets += 1;
                act(&mut state, Action::FinishEffectSelection);
            }
            Some(Decision::ChooseEffectBoolean { .. }) => {
                act(&mut state, Action::ChooseEffectBoolean(false))
            }
            None => break,
            Some(other) => panic!("unexpected choice: {other:?}"),
        }
    }
    assert_eq!(retargets, 1, "one spell was cast before it");
    assert_eq!(state.objects.get(bolt).zone, Zone::Graveyard);
    assert_eq!(state.players[0].life, 20, "the copy countered the Bolt");
}

// ---- Doomsday ------------------------------------------------------------------------------

#[test]
fn doomsday_builds_a_five_card_library_from_library_and_graveyard() {
    let mut state = game(&[
        "Island",
        "Ponder",
        "Island",
        "Brainstorm",
        "Island",
        "Island",
        "Island",
        "Island",
    ]);
    let library = state.players[0].library.clone();
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Graveyard);
    let spare = put(&mut state, P0, "Counterspell", Zone::Graveyard);
    let doomsday = put(&mut state, P0, "Doomsday", Zone::Hand);
    pool(
        &mut state,
        P0,
        &[ManaColor::B, ManaColor::B, ManaColor::B],
        0,
    );
    cast(&mut state, doomsday, &[]);
    // The pile, top first: the Bolt from the graveyard, then four library
    // cards in a new order.
    let pile = [bolt, library[3], library[1], library[0], library[7]];
    let mut picked = 0;
    while picked < pile.len() {
        match settle(&mut state) {
            Some(Decision::ChooseEffectTargets {
                legal_targets,
                max_targets,
                min_targets,
                ..
            }) => {
                assert_eq!((min_targets, max_targets), (5, 5));
                assert!(legal_targets.contains(&Target::Object(pile[picked])));
                act(
                    &mut state,
                    Action::ChooseEffectTarget(Target::Object(pile[picked])),
                );
                picked += 1;
            }
            other => panic!("expected the pile, got {other:?}"),
        }
    }
    settled(&mut state);
    assert_eq!(state.players[0].library, pile.to_vec());
    assert_eq!(state.objects.get(spare).zone, Zone::Exile);
    for card in [library[2], library[4], library[5], library[6]] {
        assert_eq!(state.objects.get(card).zone, Zone::Exile);
    }
    assert_eq!(state.players[0].life, 10, "half of 20");
    let known = state.known_library_cards(P0, P0);
    assert_eq!(known.len(), 5, "its controller knows the order");
    assert!(state.known_library_cards(P1, P0).is_empty());
}

// ---- Thassa's Oracle ---------------------------------------------------------------------------

#[test]
fn thassas_oracle_wins_when_devotion_covers_the_library() {
    let mut state = game(&["Island", "Ponder"]);
    let oracle = put(&mut state, P0, "Thassa's Oracle", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U, ManaColor::U], 0);
    cast(&mut state, oracle, &[]);
    match settle(&mut state) {
        Some(Decision::ChooseEffectTargets { legal_targets, .. }) => {
            assert_eq!(legal_targets.len(), 2, "devotion two looks at two");
            act(&mut state, Action::FinishEffectSelection);
        }
        other => panic!("expected the look, got {other:?}"),
    }
    assert!(matches!(
        next(&mut state),
        Decision::GameOver {
            winner: Some(PlayerId::P0)
        }
    ));
}

#[test]
fn thassas_oracle_keeps_one_on_top_and_bottoms_the_rest() {
    let mut state = game(&["Island", "Ponder", "Brainstorm", "Island", "Island"]);
    let library = state.players[0].library.clone();
    let oracle = put(&mut state, P0, "Thassa's Oracle", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U, ManaColor::U], 0);
    cast(&mut state, oracle, &[]);
    match settle(&mut state) {
        Some(Decision::ChooseEffectTargets { legal_targets, .. }) => {
            assert_eq!(
                legal_targets,
                vec![Target::Object(library[0]), Target::Object(library[1])]
            );
            act(
                &mut state,
                Action::ChooseEffectTarget(Target::Object(library[1])),
            );
        }
        other => panic!("expected the look, got {other:?}"),
    }
    settled(&mut state);
    let after = &state.players[0].library;
    assert_eq!(after[0], library[1]);
    assert_eq!(*after.last().unwrap(), library[0]);
    assert!(
        !state.players[1].has_lost,
        "devotion two is below five cards"
    );
}

// ---- Jace, Wielder of Mysteries ---------------------------------------------------------------

#[test]
fn jace_plus_one_mills_two_and_draws() {
    let mut state = islands();
    let jace = put(&mut state, P0, "Jace, Wielder of Mysteries", Zone::Hand);
    pool(
        &mut state,
        P0,
        &[ManaColor::U, ManaColor::U, ManaColor::U],
        1,
    );
    cast(&mut state, jace, &[]);
    settled(&mut state);
    assert_eq!(mtg_kernel::planeswalker_v1::loyalty(&state, jace), Some(4));
    activate(&mut state, jace, 0, &[Target::Player(P1)]);
    settle_declining_selections(&mut state);
    assert_eq!(state.players[1].graveyard.len(), 2);
    assert_eq!(state.players[0].hand.len(), 1);
    assert_eq!(mtg_kernel::planeswalker_v1::loyalty(&state, jace), Some(5));
}

#[test]
fn jace_wins_instead_of_drawing_from_an_empty_library() {
    let mut state = game(&[]);
    let jace = put(&mut state, P0, "Jace, Wielder of Mysteries", Zone::Hand);
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(jace, Zone::Battlefield),
    );
    event::propose_and_commit(&mut state, ProposedEvent::draw(P0));
    assert!(!state.players[0].drew_from_empty);
    assert!(matches!(
        next(&mut state),
        Decision::GameOver {
            winner: Some(PlayerId::P0)
        }
    ));
}

#[test]
fn jace_minus_eight_draws_seven_and_wins_with_an_empty_library() {
    let mut state = game(&["Island"; 7]);
    let jace = put(&mut state, P0, "Jace, Wielder of Mysteries", Zone::Hand);
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(jace, Zone::Battlefield),
    );
    mtg_kernel::planeswalker_v1::change_loyalty(&mut state, jace, 4);
    activate(&mut state, jace, 1, &[]);
    for _ in 0..20 {
        match next(&mut state) {
            Decision::GameOver { winner } => {
                assert_eq!(winner, Some(PlayerId::P0));
                assert_eq!(state.players[0].hand.len(), 7);
                return;
            }
            Decision::CastSpellOrPass { .. } => act(&mut state, Action::Pass),
            other => panic!("unexpected decision: {other:?}"),
        }
    }
    panic!("the game did not end");
}

// ---- Lion's Eye Diamond -------------------------------------------------------------------------

#[test]
fn lions_eye_diamond_discards_the_hand_for_three_mana_of_one_color() {
    let mut state = islands();
    let diamond = put(&mut state, P0, "Lion's Eye Diamond", Zone::Battlefield);
    let first = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    let second = put(&mut state, P0, "Ponder", Zone::Hand);
    next(&mut state);
    act(
        &mut state,
        Action::ActivateManaAbilityChoice(diamond, ManaColor::B),
    );
    assert_eq!(mana(&state, P0, ManaColor::B), 3);
    assert!(state.players[0].hand.is_empty());
    for card in [diamond, first, second] {
        assert_eq!(state.objects.get(card).zone, Zone::Graveyard);
    }
}

#[test]
fn lions_eye_diamond_is_not_an_automatic_mana_source() {
    let mut state = islands();
    put(&mut state, P0, "Lion's Eye Diamond", Zone::Battlefield);
    let ritual = put(&mut state, P0, "Dark Ritual", Zone::Hand);
    assert!(
        !castable(&mut state, ritual),
        "it is activated only as an instant"
    );
}

// ---- Show and Tell ------------------------------------------------------------------------------

#[test]
fn show_and_tell_puts_each_players_chosen_card_onto_the_battlefield() {
    let mut state = islands();
    let show = put(&mut state, P0, "Show and Tell", Zone::Hand);
    let wraith = put(&mut state, P0, "Street Wraith", Zone::Hand);
    let ritual = put(&mut state, P0, "Dark Ritual", Zone::Hand);
    let diamond = put(&mut state, P1, "Lion's Eye Diamond", Zone::Hand);
    put(&mut state, P1, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 2);
    cast(&mut state, show, &[]);
    match settle(&mut state) {
        Some(Decision::ChooseEffectTargets {
            player,
            legal_targets,
            min_targets,
            max_targets,
            ..
        }) => {
            assert_eq!(player, P0, "the active player chooses first");
            assert_eq!((min_targets, max_targets), (0, 1));
            assert_eq!(legal_targets, vec![Target::Object(wraith)]);
            assert!(!legal_targets.contains(&Target::Object(ritual)));
            act(
                &mut state,
                Action::ChooseEffectTarget(Target::Object(wraith)),
            );
        }
        other => panic!("expected the first choice, got {other:?}"),
    }
    match settle(&mut state) {
        Some(Decision::ChooseEffectTargets {
            player,
            legal_targets,
            ..
        }) => {
            assert_eq!(player, P1);
            assert_eq!(legal_targets, vec![Target::Object(diamond)]);
            assert!(
                state
                    .known_hand_cards(P1, P0)
                    .iter()
                    .any(|entry| entry.object == wraith),
                "the second player sees the first choice"
            );
            act(
                &mut state,
                Action::ChooseEffectTarget(Target::Object(diamond)),
            );
        }
        other => panic!("expected the second choice, got {other:?}"),
    }
    settled(&mut state);
    assert_eq!(state.objects.get(wraith).zone, Zone::Battlefield);
    assert_eq!(state.objects.get(diamond).zone, Zone::Battlefield);
    assert_eq!(state.objects.get(diamond).controller, P1);
    assert_eq!(state.objects.get(ritual).zone, Zone::Hand);
}

#[test]
fn show_and_tell_lets_a_player_decline() {
    let mut state = islands();
    let show = put(&mut state, P0, "Show and Tell", Zone::Hand);
    let wraith = put(&mut state, P0, "Street Wraith", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 2);
    cast(&mut state, show, &[]);
    match settle(&mut state) {
        Some(Decision::ChooseEffectTargets { can_finish, .. }) => {
            assert!(can_finish);
            act(&mut state, Action::FinishEffectSelection);
        }
        other => panic!("expected the choice, got {other:?}"),
    }
    settled(&mut state);
    assert_eq!(state.objects.get(wraith).zone, Zone::Hand);
}

// ---- Sneak Attack -------------------------------------------------------------------------------

#[test]
fn sneak_attack_puts_a_hasty_creature_in_and_sacrifices_it_at_end_step() {
    let mut state = islands();
    let sneak = put(&mut state, P0, "Sneak Attack", Zone::Battlefield);
    let wraith = put(&mut state, P0, "Street Wraith", Zone::Hand);
    put(&mut state, P0, "Dark Ritual", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 0);
    activate(&mut state, sneak, 0, &[]);
    match settle(&mut state) {
        Some(Decision::ChooseEffectTargets { legal_targets, .. }) => {
            assert_eq!(legal_targets, vec![Target::Object(wraith)]);
            act(
                &mut state,
                Action::ChooseEffectTarget(Target::Object(wraith)),
            );
        }
        other => panic!("expected the creature choice, got {other:?}"),
    }
    settled(&mut state);
    assert_eq!(state.objects.get(wraith).zone, Zone::Battlefield);
    assert!(engine::has_effective_keyword(
        &state,
        wraith,
        mtg_kernel::card_def::Keywords::HASTE
    ));
    pass_until(&mut state, |state| {
        state.objects.get(wraith).zone == Zone::Graveyard
    });
    assert_eq!(state.step, Step::End);
}

// ---- Omniscience --------------------------------------------------------------------------------

#[test]
fn omniscience_casts_hand_spells_without_paying_their_mana_costs() {
    let mut state = islands();
    let ritual = put(&mut state, P0, "Dark Ritual", Zone::Hand);
    assert!(!castable(&mut state, ritual), "no mana for it");
    put(&mut state, P0, "Omniscience", Zone::Battlefield);
    cast(&mut state, ritual, &[]);
    settled(&mut state);
    assert_eq!(mana(&state, P0, ManaColor::B), 3);
}

#[test]
fn omniscience_offers_paying_the_mana_cost() {
    let mut state = islands();
    let ritual = put(&mut state, P0, "Dark Ritual", Zone::Hand);
    put(&mut state, P0, "Omniscience", Zone::Battlefield);
    pool(&mut state, P0, &[ManaColor::B], 0);
    cast(&mut state, ritual, &[]);
    match next(&mut state) {
        Decision::ChooseEffectOption { option_count, .. } => {
            assert_eq!(option_count, 2);
            act(&mut state, Action::ChooseEffectOption(0));
        }
        other => panic!("expected the free-cast choice, got {other:?}"),
    }
    settled(&mut state);
    assert_eq!(mana(&state, P0, ManaColor::B), 3, "paid one, added three");
}

// ---- Aluren -------------------------------------------------------------------------------------

#[test]
fn aluren_lets_any_player_cast_small_creatures_free_at_instant_speed() {
    let mut state = islands();
    put(&mut state, P0, "Aluren", Zone::Battlefield);
    let oracle = put(&mut state, P1, "Thassa's Oracle", Zone::Hand);
    let wraith = put(&mut state, P1, "Street Wraith", Zone::Hand);
    state.priority_player = P1;
    let (castable_spells, _) = priority(&mut state);
    assert!(castable_spells.contains(&oracle));
    assert!(!castable_spells.contains(&wraith), "mana value five");
    act(&mut state, Action::CastSpell(oracle));
    for _ in 0..20 {
        match next(&mut state) {
            Decision::ChooseEffectTargets { player, .. } => {
                assert_eq!(player, P1);
                act(&mut state, Action::FinishEffectSelection);
            }
            Decision::CastSpellOrPass { .. }
                if state.stack.is_empty() && state.engine.pending_triggers.is_empty() =>
            {
                break;
            }
            Decision::CastSpellOrPass { .. } => act(&mut state, Action::Pass),
            other => panic!("unexpected decision: {other:?}"),
        }
    }
    assert_eq!(state.objects.get(oracle).zone, Zone::Battlefield);
    assert_eq!(state.objects.get(oracle).controller, P1);
}

// ---- Emrakul, the Aeons Torn --------------------------------------------------------------------

const EMRAKUL: &str = "Emrakul, the Aeons Torn";

#[test]
fn emrakul_cast_takes_an_extra_turn_and_cannot_be_countered() {
    assert!(def(EMRAKUL).spell_cannot_be_countered);
    let mut state = islands();
    let emrakul = put(&mut state, P0, EMRAKUL, Zone::Hand);
    pool(&mut state, P0, &[], 15);
    cast(&mut state, emrakul, &[]);
    settled(&mut state);
    assert_eq!(state.objects.get(emrakul).zone, Zone::Battlefield);
    assert!(
        state.extra_turn_v1.is_some(),
        "the cast trigger scheduled a turn"
    );
    let turn = state.turn;
    pass_until(&mut state, |state| {
        state
            .extra_turn_v1
            .as_ref()
            .is_some_and(|extra| extra.current_extra.is_some())
    });
    assert_eq!(state.active_player, P0, "P0 takes the extra turn");
    assert_eq!(state.turn, turn, "an extra turn keeps the round");
}

#[test]
fn emrakul_has_protection_from_colored_spells() {
    let mut state = islands();
    let emrakul = put(&mut state, P0, EMRAKUL, Zone::Battlefield);
    let other = put(&mut state, P0, "Street Wraith", Zone::Battlefield);
    state.priority_player = P1;
    let bolt = put(&mut state, P1, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P1, &[ManaColor::R], 0);
    act(&mut state, Action::CastSpell(bolt));
    match next(&mut state) {
        Decision::ChooseTargets { legal_targets, .. } => {
            assert!(!legal_targets.contains(&Target::Object(emrakul)));
            assert!(legal_targets.contains(&Target::Object(other)));
        }
        other => panic!("expected targets, got {other:?}"),
    }
    assert!(engine::damage_is_prevented_by_protection(
        &state,
        bolt,
        Target::Object(emrakul)
    ));
    assert!(!engine::damage_is_prevented_by_protection(
        &state,
        bolt,
        Target::Object(other)
    ));
}

#[test]
fn emrakul_annihilator_makes_the_defender_sacrifice_six_permanents() {
    let mut state = islands();
    let emrakul = put(&mut state, P0, EMRAKUL, Zone::Battlefield);
    let lands: Vec<ObjectId> = (0..7)
        .map(|_| put(&mut state, P1, "Island", Zone::Battlefield))
        .collect();
    pass_until(&mut state, |state| state.step == Step::DeclareAttackers);
    act(&mut state, Action::DeclareAttackers(vec![emrakul]));
    match settle(&mut state) {
        Some(Decision::ChooseEffectTargets {
            player,
            min_targets,
            max_targets,
            legal_targets,
            ..
        }) => {
            assert_eq!(player, P1, "the defending player chooses");
            assert_eq!((min_targets, max_targets), (6, 6));
            assert_eq!(legal_targets.len(), 7);
            for land in &lands[..6] {
                act(
                    &mut state,
                    Action::ChooseEffectTarget(Target::Object(*land)),
                );
            }
        }
        other => panic!("expected the sacrifice choice, got {other:?}"),
    }
    settled(&mut state);
    for land in &lands[..6] {
        assert_eq!(state.objects.get(*land).zone, Zone::Graveyard);
    }
    assert_eq!(state.objects.get(lands[6]).zone, Zone::Battlefield);
}

#[test]
fn emrakul_put_into_a_graveyard_from_anywhere_shuffles_the_graveyard_in() {
    let mut state = islands();
    let emrakul = put(&mut state, P0, EMRAKUL, Zone::Hand);
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Graveyard);
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(emrakul, Zone::Graveyard),
    );
    settled(&mut state);
    assert!(state.players[0].graveyard.is_empty());
    assert_eq!(state.objects.get(emrakul).zone, Zone::Library);
    assert_eq!(state.objects.get(bolt).zone, Zone::Library);
    assert_eq!(state.players[0].library.len(), 22);
}

// ---- Atraxa, Grand Unifier ----------------------------------------------------------------------

#[test]
fn atraxa_takes_one_revealed_card_per_card_type() {
    let mut state = game(&[
        "Island",
        "Lightning Bolt",
        "Ponder",
        "Island",
        "Chrome Mox",
        "Street Wraith",
        "Brainstorm",
        "Sneak Attack",
        "Jace, Wielder of Mysteries",
        "Island",
        "Doomsday",
    ]);
    let library = state.players[0].library.clone();
    let atraxa = put(&mut state, P0, "Atraxa, Grand Unifier", Zone::Hand);
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(atraxa, Zone::Battlefield),
    );
    // Island (land), Lightning Bolt (instant), Ponder (sorcery), then
    // finish: the second Island and Brainstorm are no longer offered.
    for pick in [library[0], library[1], library[2]] {
        match settle(&mut state) {
            Some(Decision::ChooseEffectTargets { legal_targets, .. }) => {
                assert!(legal_targets.contains(&Target::Object(pick)));
                act(&mut state, Action::ChooseEffectTarget(Target::Object(pick)));
            }
            other => panic!("expected a pick, got {other:?}"),
        }
    }
    match settle(&mut state) {
        Some(Decision::ChooseEffectTargets { legal_targets, .. }) => {
            for taken_type in [library[3], library[6], library[9]] {
                assert!(!legal_targets.contains(&Target::Object(taken_type)));
            }
            assert!(legal_targets.contains(&Target::Object(library[4])));
            act(&mut state, Action::FinishEffectSelection);
        }
        other => panic!("expected the next pick, got {other:?}"),
    }
    settled(&mut state);
    for card in [library[0], library[1], library[2]] {
        assert_eq!(state.objects.get(card).zone, Zone::Hand);
    }
    let after = &state.players[0].library;
    assert_eq!(after.len(), 8);
    assert_eq!(after[0], library[10], "the unrevealed card is now on top");
}

// ---- Raph & Mikey, Troublemakers ----------------------------------------------------------------

#[test]
fn raph_and_mikey_put_the_first_revealed_creature_in_attacking() {
    let mut state = game(&["Island", "Ponder", "Street Wraith", "Brainstorm"]);
    let library = state.players[0].library.clone();
    let turtles = put(
        &mut state,
        P0,
        "Raph & Mikey, Troublemakers",
        Zone::Battlefield,
    );
    pass_until(&mut state, |state| state.step == Step::DeclareAttackers);
    act(&mut state, Action::DeclareAttackers(vec![turtles]));
    settled(&mut state);
    let wraith = library[2];
    assert_eq!(state.objects.get(wraith).zone, Zone::Battlefield);
    assert!(state.objects.get(wraith).tapped);
    assert!(state.engine.combat.attackers.contains(&wraith));
    let after = &state.players[0].library;
    assert_eq!(after.len(), 3);
    assert_eq!(after[0], library[3]);
    let mut bottom = after[1..].to_vec();
    bottom.sort();
    let mut expected = vec![library[0], library[1]];
    expected.sort();
    assert_eq!(bottom, expected);
}

// ---- Acererak the Archlich and the dungeons -----------------------------------------------------

const ACERERAK: &str = "Acererak the Archlich";
const LOST_MINE: u16 = 2;
const MAD_MAGE: u16 = 3;
const TOMB: u16 = 4;

/// P0 casts Acererak from hand; its entry trigger is next.
fn cast_acererak(state: &mut GameState) -> ObjectId {
    let acererak = put(state, P0, ACERERAK, Zone::Hand);
    pool(state, P0, &[ManaColor::B], 2);
    cast(state, acererak, &[]);
    acererak
}

fn in_room(state: &mut GameState, player: PlayerId, dungeon: u16, room: u16) {
    let marker = &mut state.players[player.index()].dungeon;
    marker.dungeon_id = Some(dungeon);
    marker.room_id = Some(room);
}

#[test]
fn acererak_returns_to_hand_and_ventures_into_a_chosen_dungeon() {
    let mut state = islands();
    let acererak = cast_acererak(&mut state);
    match settle(&mut state) {
        Some(Decision::ChooseEffectOption { option_count, .. }) => {
            assert_eq!(option_count, 3, "the three dungeons");
            act(&mut state, Action::ChooseEffectOption(2));
        }
        other => panic!("expected the dungeon choice, got {other:?}"),
    }
    settled(&mut state);
    assert_eq!(state.objects.get(acererak).zone, Zone::Hand);
    let marker = &state.players[0].dungeon;
    assert_eq!((marker.dungeon_id, marker.room_id), (Some(TOMB), Some(1)));
    // Trapped Entry: each player loses 1 life.
    assert_eq!((state.players[0].life, state.players[1].life), (19, 19));
}

#[test]
fn completing_tomb_of_annihilation_makes_the_atropal_and_stops_acererak() {
    let mut state = islands();
    in_room(&mut state, P0, TOMB, 4);
    let acererak = cast_acererak(&mut state);
    settled(&mut state);
    let marker = &state.players[0].dungeon;
    assert_eq!((marker.dungeon_id, marker.room_id), (None, None));
    assert_eq!(marker.completed_dungeons, vec![TOMB]);
    assert!(state.players[0]
        .battlefield
        .iter()
        .any(|&object| state.objects.get(object).name == "The Atropal"));
    assert_eq!(state.objects.get(acererak).zone, Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 2);
    cast(&mut state, acererak, &[]);
    settled(&mut state);
    assert_eq!(
        state.objects.get(acererak).zone,
        Zone::Battlefield,
        "after Tomb of Annihilation it stays"
    );
}

#[test]
fn lost_mine_completion_waits_for_all_room_triggers_both_seats() {
    let marker = |state: &GameState, player: PlayerId| {
        let dungeon = &state.players[player.index()].dungeon;
        (dungeon.dungeon_id, dungeon.room_id, dungeon.completed_dungeons.clone())
    };
    let bottom = (Some(LOST_MINE), Some(7), Vec::new());
    let completed = (None, None, vec![LOST_MINE]);

    for actor in [P0, P1] {
        let mut state = islands();
        state.active_player = actor;
        state.priority_player = actor;
        let aluren = put(&mut state, actor, "Aluren", Zone::Hand);
        pool(&mut state, actor, &[ManaColor::G; 2], 2);
        cast(&mut state, aluren, &[]);
        settled(&mut state);
        let acererak = put(&mut state, actor, ACERERAK, Zone::Hand);
        cast(&mut state, acererak, &[]);
        assert!(matches!(
            settle(&mut state),
            Some(Decision::ChooseEffectOption { player, option_count: 3, .. }) if player == actor
        ));
        act(&mut state, Action::ChooseEffectOption(0));
        let cave_top = state.players[actor.index()].library[0];
        assert!(matches!(
            settle(&mut state),
            Some(Decision::ChooseEffectTargets { player, ref legal_targets, .. })
                if player == actor && legal_targets.as_slice() == [Target::Object(cave_top)]
        ));
        act(&mut state, Action::ChooseEffectTarget(Target::Object(cave_top)));
        settled(&mut state);

        cast(&mut state, acererak, &[]);
        assert!(matches!(settle(&mut state), Some(Decision::ChooseEffectOption { option_count: 2, .. })));
        act(&mut state, Action::ChooseEffectOption(0)); // Goblin Lair.
        settled(&mut state);
        cast(&mut state, acererak, &[]);
        assert!(matches!(settle(&mut state), Some(Decision::ChooseEffectOption { option_count: 2, .. })));
        act(&mut state, Action::ChooseEffectOption(1)); // Dark Pool.
        pass_until(&mut state, |state| {
            marker(state, actor).1 == Some(5) && state.stack.len() == 1
        });

        // Aluren lets the same Acererak advance again above the older room.
        cast(&mut state, acererak, &[]);
        pass_until(&mut state, |state| marker(state, actor) == bottom && state.stack.len() == 2);
        assert_eq!(state.players[actor.opponent().index()].life, 20);
        let before_draw = state.players[actor.index()].library.len();
        let snapshot = serde_json::to_vec(&state).unwrap();
        let mut resolved: GameState = serde_json::from_slice(&snapshot).unwrap();
        pass_until(&mut resolved, |state| state.stack.len() == 1);
        assert_eq!(marker(&resolved, actor), bottom);
        assert_eq!(resolved.players[actor.index()].library.len(), before_draw - 1);
        settled(&mut resolved);
        assert_eq!(marker(&resolved, actor), completed);
        assert_eq!(resolved.players[actor.opponent().index()].life, 19);

        let mut countered: GameState = serde_json::from_slice(&snapshot).unwrap();
        let temple = Target::StackItem(countered.stack.last().unwrap().v4.stack_item_id);
        let stifle = put(&mut countered, actor, "Stifle", Zone::Hand);
        pool(&mut countered, actor, &[ManaColor::U], 0);
        cast(&mut countered, stifle, &[temple]);
        pass_until(&mut countered, |state| state.stack.len() == 1);
        assert_eq!(marker(&countered, actor), bottom);
        settled(&mut countered);
        assert_eq!(marker(&countered, actor), completed);
        assert_eq!(countered.players[actor.index()].library.len(), before_draw);

        // A new venture completes the old instance before a new room is
        // chosen. The original Temple and Dark Pool remain on the stack.
        cast(&mut state, acererak, &[]);
        assert!(matches!(
            settle(&mut state),
            Some(Decision::ChooseEffectOption { player, option_count: 3, .. }) if player == actor
        ));
        assert_eq!(marker(&state, actor), completed);
        act(&mut state, Action::ChooseEffectOption(0));
        assert!(matches!(settle(&mut state), Some(Decision::ChooseEffectTargets { .. })));
        assert_eq!(marker(&state, actor), (Some(LOST_MINE), Some(1), vec![LOST_MINE]));
        act(&mut state, Action::ChooseEffectTarget(Target::Object(cave_top)));
        settled(&mut state);
        assert_eq!(marker(&state, actor), (Some(LOST_MINE), Some(1), vec![LOST_MINE]));
        assert_eq!(state.players[actor.index()].library.len(), before_draw - 1);
        assert_eq!(state.players[actor.opponent().index()].life, 19);
    }
}

#[test]
fn a_departed_acererak_still_publishes_its_room_before_priority_both_seats() {
    for actor in [P0, P1] {
        let opponent = actor.opponent();
        let mut state = islands();
        state.active_player = actor;
        state.priority_player = actor;
        in_room(&mut state, actor, LOST_MINE, 5);
        let acererak = put(&mut state, actor, ACERERAK, Zone::Hand);
        let bounce = put(&mut state, opponent, "Echoing Truth", Zone::Hand);
        pool(&mut state, actor, &[ManaColor::B], 2);
        cast(&mut state, acererak, &[]);
        pass_until(&mut state, |state| {
            state.objects.get(acererak).zone == Zone::Battlefield && state.stack.len() == 1
        });
        act(&mut state, Action::Pass);
        pool(&mut state, opponent, &[ManaColor::U], 1);
        cast(&mut state, bounce, &[Target::Object(acererak)]);
        pass_until(&mut state, |state| {
            state.objects.get(acererak).zone == Zone::Hand && state.stack.len() == 1
        });
        let enter = state.stack[0].v4.stack_item_id;
        let library = state.players[actor.index()].library.len();
        for seat in [actor, opponent] {
            assert!(matches!(next(&mut state), Decision::CastSpellOrPass { player, .. } if player == seat));
            act(&mut state, Action::Pass);
        }

        assert!(matches!(next(&mut state), Decision::CastSpellOrPass { player, .. } if player == actor));
        let [room] = state.stack.as_slice() else {
            panic!("the room must be published before priority returns");
        };
        assert_eq!(room.source, acererak);
        assert_ne!(room.v4.stack_item_id, enter);
        let marker = &state.players[actor.index()].dungeon;
        assert_eq!((marker.dungeon_id, marker.room_id), (Some(LOST_MINE), Some(7)));
        assert!(marker.completed_dungeons.is_empty());
        settled(&mut state);
        assert_eq!(state.players[actor.index()].library.len(), library - 1);
        assert_eq!(state.players[actor.index()].dungeon.completed_dungeons, vec![LOST_MINE]);
    }
}

#[test]
fn taking_initiative_advances_the_current_forgotten_realms_dungeon_both_seats() {
    for actor in [P0, P1] {
        let mut state = islands();
        state.active_player = actor;
        state.priority_player = actor;
        let acererak = put(&mut state, actor, ACERERAK, Zone::Hand);
        pool(&mut state, actor, &[ManaColor::B], 2);
        cast(&mut state, acererak, &[]);
        assert!(matches!(settle(&mut state), Some(Decision::ChooseEffectOption { option_count: 3, .. })));
        act(&mut state, Action::ChooseEffectOption(0));
        let top = state.players[actor.index()].library[0];
        assert!(matches!(settle(&mut state), Some(Decision::ChooseEffectTargets { .. })));
        act(&mut state, Action::ChooseEffectTarget(Target::Object(top)));
        settled(&mut state);
        pool(&mut state, actor, &[ManaColor::B], 2);
        cast(&mut state, acererak, &[]);
        assert!(matches!(settle(&mut state), Some(Decision::ChooseEffectOption { option_count: 2, .. })));
        act(&mut state, Action::ChooseEffectOption(0)); // Goblin Lair.
        settled(&mut state);

        let hunter = put(&mut state, actor, "Avenging Hunter", Zone::Hand);
        pool(&mut state, actor, &[ManaColor::G], 4);
        cast(&mut state, hunter, &[]);
        let route = settle(&mut state).expect("initiative must advance the existing dungeon");
        assert!(matches!(route, Decision::ChooseEffectOption {
            player, source, option_count: 2, ..
        } if player == actor && source == hunter));
        let marker = &state.players[actor.index()].dungeon;
        assert_eq!((marker.dungeon_id, marker.room_id), (Some(LOST_MINE), Some(2)));

        let saved = serde_json::to_vec(&state).unwrap();
        let mut restored: GameState = serde_json::from_slice(&saved).unwrap();
        assert_eq!(next(&mut restored), route);
        let mut reordered = restored.clone();
        let Some(mtg_kernel::effect::PendingEffectChoice::ChooseOption { options, .. }) =
            reordered.engine.pending_effect.as_mut().and_then(|pending| pending.choice.as_mut())
        else {
            panic!("native initiative route menu");
        };
        options.swap(0, 1);
        assert!(mtg_kernel::effect::validate_pending_effect_choice(&reordered).is_err());

        act(&mut restored, Action::ChooseEffectOption(0)); // Storeroom.
        assert!(matches!(settle(&mut restored), Some(Decision::ChooseTargets {
            player, legal_targets, ..
        }) if player == actor && legal_targets.contains(&Target::Object(hunter))));
        act(&mut restored, Action::ChooseTarget(Target::Object(hunter)));
        settled(&mut restored);
        assert_eq!(restored.objects.get(hunter).counters.plus1_plus1, 1);
        let marker = &restored.players[actor.index()].dungeon;
        assert_eq!((marker.dungeon_id, marker.room_id), (Some(LOST_MINE), Some(4)));
        assert!(marker.completed_dungeons.is_empty());
    }
}

fn paid_black_painter(state: &mut GameState, player: PlayerId) -> ObjectId {
    let painter = put(state, player, "Painter's Servant", Zone::Hand);
    pool(state, player, &[], 2);
    cast(state, painter, &[]);
    assert!(matches!(settle(state), Some(Decision::ChooseEffectOption { option_count: 5, .. })));
    act(state, Action::ChooseEffectOption(2));
    settled(state);
    assert_eq!(state.objects.get(painter).v4.chosen_color, Some(ManaColor::B));
    painter
}

#[test]
fn fungi_cavern_targets_using_the_active_dungeons_characteristics_both_seats() {
    use mtg_kernel::effect::EffectOp;
    use mtg_kernel::eternal_cards_v1::EternalOpV1;
    use mtg_kernel::standard_cards_v1::StandardOpV1;

    let legacy = EffectOp::StandardV1(StandardOpV1::EternalV1(
        EternalOpV1::RoomAbilityV1 { dungeon: LOST_MINE, room: 6 },
    ));
    for actor in [P0, P1] {
        let opponent = actor.opponent();
        for painted in [false, true] {
            let mut state = islands();
            state.active_player = actor;
            state.priority_player = actor;
            in_room(&mut state, actor, LOST_MINE, 3);
            if painted {
                paid_black_painter(&mut state, actor);
            }
            let enforcer = put(&mut state, opponent, "Mystic Enforcer", Zone::Battlefield);
            let chaplain = put(&mut state, opponent, "Beloved Chaplain", Zone::Battlefield);
            let shroud = put(&mut state, opponent, "Nimble Mongoose", Zone::Battlefield);
            let acererak = put(&mut state, actor, ACERERAK, Zone::Hand);
            pool(&mut state, actor, &[ManaColor::B], 2);
            cast(&mut state, acererak, &[]);
            assert!(matches!(settle(&mut state), Some(Decision::ChooseEffectOption { option_count: 2, .. })));
            act(&mut state, Action::ChooseEffectOption(1));
            let Some(Decision::ChooseTargets { player, legal_targets, .. }) = settle(&mut state) else {
                panic!("Fungi Cavern must have legal targets");
            };
            assert_eq!(player, actor);
            assert_eq!(legal_targets.contains(&Target::Object(enforcer)), !painted);
            assert!(legal_targets.contains(&Target::Object(chaplain)));
            assert!(!legal_targets.contains(&Target::Object(shroud)));
            let target = if painted { chaplain } else { enforcer };
            let power = engine::effective_power(&state, target);
            if !painted {
                let mut downgraded = state.clone();
                downgraded.engine.pending_triggers[0].effect = legacy.clone();
                let mut downgraded: GameState =
                    serde_json::from_slice(&serde_json::to_vec(&downgraded).unwrap()).unwrap();
                assert!(matches!(engine::advance_until_decision(&mut downgraded),
                    Decision::Halted { mechanic: engine::UnsupportedMechanic::InvalidEffectContinuation, .. }));
            }
            act(&mut state, Action::ChooseTarget(Target::Object(target)));
            if !painted {
                assert!(matches!(next(&mut state), Decision::CastSpellOrPass { .. }));
                let mut downgraded = state.clone();
                let room = downgraded.stack.last_mut().expect("published Fungi Cavern");
                assert!(matches!(&room.inline_effect,
                    Some(EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::RoomBoundV1 { .. })))));
                room.inline_effect = Some(legacy.clone());
                let mut downgraded: GameState =
                    serde_json::from_slice(&serde_json::to_vec(&downgraded).unwrap()).unwrap();
                for _ in 0..2 {
                    assert!(matches!(next(&mut downgraded), Decision::CastSpellOrPass { .. }));
                    act(&mut downgraded, Action::Pass);
                }
                assert!(matches!(engine::advance_until_decision(&mut downgraded),
                    Decision::Halted { mechanic: engine::UnsupportedMechanic::InvalidEffectContinuation, .. }));
            }
            settled(&mut state);
            assert_eq!(engine::effective_power(&state, target), power - 4);
        }
    }
}

#[test]
fn fungi_cavern_keeps_painted_color_after_its_dungeon_leaves_both_seats() {
    for actor in [P0, P1] {
        let mut state = islands();
        state.active_player = actor;
        state.priority_player = actor;
        in_room(&mut state, actor, LOST_MINE, 3);
        let painter = paid_black_painter(&mut state, actor);
        let aluren = put(&mut state, actor, "Aluren", Zone::Hand);
        pool(&mut state, actor, &[ManaColor::G; 2], 2);
        cast(&mut state, aluren, &[]);
        settled(&mut state);
        let target = put(&mut state, actor.opponent(), "Llanowar Elves", Zone::Battlefield);
        let acererak = put(&mut state, actor, ACERERAK, Zone::Hand);
        cast(&mut state, acererak, &[]);
        assert!(matches!(settle(&mut state), Some(Decision::ChooseEffectOption { option_count: 2, .. })));
        act(&mut state, Action::ChooseEffectOption(1)); // Fungi Cavern.
        choose_target(&mut state, Target::Object(target));
        assert!(matches!(next(&mut state), Decision::CastSpellOrPass { player, .. } if player == actor));
        let fungi = state.stack.last().unwrap().v4.stack_item_id;
        let mut active = state.clone();

        cast(&mut state, acererak, &[]); // Reach Temple with Fungi still waiting.
        pass_until(&mut state, |state| {
            state.players[actor.index()].dungeon.room_id == Some(7) && state.stack.len() == 2
        });
        cast(&mut state, acererak, &[]); // Complete Lost Mine before choosing the new dungeon.
        assert!(matches!(settle(&mut state), Some(Decision::ChooseEffectOption { option_count: 3, .. })));
        assert_eq!(state.players[actor.index()].dungeon.completed_dungeons, vec![LOST_MINE]);
        act(&mut state, Action::ChooseEffectOption(1)); // Mad Mage's Yawning Portal.
        pass_until(&mut state, |state| {
            let marker = &state.players[actor.index()].dungeon;
            marker.dungeon_id == Some(MAD_MAGE) && marker.room_id == Some(1) && state.stack.len() == 2
        });

        for (branch, departed) in [(&mut active, false), (&mut state, true)] {
            let bounce = put(branch, actor, "Echoing Truth", Zone::Hand);
            pool(branch, actor, &[ManaColor::U], 1);
            cast(branch, bounce, &[Target::Object(painter)]);
            pass_until(branch, |state| state.objects.get(painter).zone == Zone::Hand);
            let blessing = put(branch, actor, "Benevolent Blessing", Zone::Hand);
            pool(branch, actor, &[ManaColor::W], 1);
            cast(branch, blessing, &[Target::Object(target)]);
            assert!(matches!(settle(branch), Some(Decision::ChooseEffectOption { option_count: 5, .. })));
            act(branch, Action::ChooseEffectOption(2));
            pass_until(branch, |state| state.objects.get(blessing).v4.chosen_color == Some(ManaColor::B));
            assert!(branch.stack.iter().any(|item| item.v4.stack_item_id == fungi));
            let power = engine::effective_power(branch, target);

            let saved = serde_json::to_value(branch).unwrap();
            if departed {
                let mut forged = saved.clone();
                let removal = forged["engine"]["event_history"].as_array_mut().unwrap().iter_mut()
                    .find_map(|row| row.get_mut("EternalOwnedDrawV1")?.get_mut("journal")?
                        .get_mut("DungeonRoomV1")?.get_mut("Removed"))
                    .expect("original Lost Mine removal");
                assert_eq!(removal["colors"], serde_json::json!(mtg_kernel::card_def::mana_color_mask(ManaColor::B)));
                // The first room of this occurrence was Fungi Cavern, not its bottom room.
                removal["last_room"] = removal["occurrence"].clone();
                let mut forged: GameState = serde_json::from_value(forged).unwrap();
                for _ in 0..2 {
                    assert!(matches!(next(&mut forged), Decision::CastSpellOrPass { .. }));
                    act(&mut forged, Action::Pass);
                }
                assert!(matches!(engine::advance_until_decision(&mut forged), Decision::Halted { .. }));
            }
            let mut restored: GameState = serde_json::from_value(saved).unwrap();
            settled(&mut restored);
            assert_eq!(engine::effective_power(&restored, target), if departed { power } else { power - 4 });
        }
    }
}

fn choose_venture_route(state: &mut GameState, option: u16) {
    assert!(matches!(settle(state), Some(Decision::ChooseEffectOption { .. })));
    act(state, Action::ChooseEffectOption(option));
}

#[test]
fn acererak_advances_native_undercity_with_bound_throne_restore_both_seats() {
    use mtg_kernel::effect::EffectOp;
    use mtg_kernel::state::{InitiativeTriggerKindV1, UndercityRoomV1, UNDERCITY_DUNGEON_ID_V1};

    for actor in [P0, P1] {
        let mut state = game(&[
            "Island", "Forest", "Llanowar Elves", "Llanowar Elves", "Island", "Island", "Island",
        ]);
        state.active_player = actor;
        state.priority_player = actor;
        let hunter = put(&mut state, actor, "Avenging Hunter", Zone::Hand);
        pool(&mut state, actor, &[ManaColor::G], 4);
        cast(&mut state, hunter, &[]);
        assert!(matches!(settle(&mut state), Some(Decision::ChooseEffectTargets { can_finish: true, .. })));
        act(&mut state, Action::FinishEffectSelection);
        settled(&mut state);
        assert_eq!(state.initiative, Some(actor));
        assert_eq!(state.players[actor.index()].dungeon.dungeon_id, Some(UNDERCITY_DUNGEON_ID_V1));
        assert_eq!(state.players[actor.index()].dungeon.room_id, Some(UndercityRoomV1::SecretEntrance.stable_id()));

        let enforcer = put(&mut state, actor.opponent(), "Mystic Enforcer", Zone::Battlefield);
        let chaplain = put(&mut state, actor.opponent(), "Beloved Chaplain", Zone::Battlefield);
        let acererak = put(&mut state, actor, ACERERAK, Zone::Hand);
        let mut painted = state.clone();
        paid_black_painter(&mut painted, actor);
        for (branch, painted) in [(&mut state, false), (&mut painted, true)] {
            pool(branch, actor, &[ManaColor::B], 2);
            cast(branch, acererak, &[]);
            choose_venture_route(branch, 0); // Forge.
            let Some(Decision::ChooseTargets { player, source, legal_targets, .. }) = settle(branch) else {
                panic!("Forge must use the current Undercity room");
            };
            assert_eq!(player, actor);
            assert_eq!(source, acererak);
            assert_eq!(legal_targets.contains(&Target::Object(enforcer)), !painted);
            assert!(legal_targets.contains(&Target::Object(chaplain)));
            let target = if painted { chaplain } else { enforcer };
            act(branch, Action::ChooseTarget(Target::Object(target)));
            settled(branch);
            assert_eq!(branch.objects.get(target).counters.plus1_plus1, 2);
            assert_eq!(branch.players[actor.index()].dungeon.room_id, Some(UndercityRoomV1::Forge.stable_id()));
        }

        pool(&mut state, actor, &[ManaColor::B], 2);
        cast(&mut state, acererak, &[]);
        choose_venture_route(&mut state, 1); // Arena.
        choose_target(&mut state, Target::Object(chaplain));
        settled(&mut state);
        pool(&mut state, actor, &[ManaColor::B], 2);
        cast(&mut state, acererak, &[]);
        choose_venture_route(&mut state, 0); // Archives.
        settled(&mut state);

        pool(&mut state, actor, &[ManaColor::B], 2);
        cast(&mut state, acererak, &[]);
        let throne = settle(&mut state).expect("bound Throne selection");
        let Decision::ChooseEffectTargets { player, ref legal_targets, can_finish: false, .. } = throne else {
            panic!("Throne must reveal and offer its creature choices");
        };
        assert_eq!(player, actor);
        let Target::Object(chosen) = legal_targets[0] else { panic!("Throne creature") };
        let marker = &state.players[actor.index()].dungeon;
        assert_eq!(marker.room_id, Some(UndercityRoomV1::ThroneOfTheDeadThree.stable_id()));
        assert!(marker.completed_dungeons.is_empty());
        assert!(mtg_kernel::rl::observe_v1(&state, actor, 0).unwrap_err().0.contains("Bound Undercity rooms"));

        let pending = state.engine.pending_effect.as_ref().unwrap();
        let Some(EffectOp::ResolveInitiativeTrigger { binding }) = pending.resolving_item.inline_effect.as_ref() else {
            panic!("Throne must retain its native room binding");
        };
        let InitiativeTriggerKindV1::UndercityRoomBoundV1 {
            room: UndercityRoomV1::ThroneOfTheDeadThree, entered_at,
        } = binding.kind else { panic!("bound Throne entry") };
        let saved = serde_json::to_value(&state).unwrap();
        let mut forged = saved.clone();
        forged["engine"]["event_history"][entered_at]["EternalOwnedDrawV1"]["journal"]
            ["DungeonRoomV1"]["Entered"]["room"] =
                serde_json::json!(UndercityRoomV1::Archives.stable_id());
        let forged: GameState = serde_json::from_value(forged).unwrap();
        assert!(mtg_kernel::effect::validate_pending_effect_choice(&forged).is_err());

        let mut restored: GameState = serde_json::from_value(saved).unwrap();
        assert_eq!(next(&mut restored), throne);
        act(&mut restored, Action::ChooseEffectTarget(Target::Object(chosen)));
        settled(&mut restored);
        assert_eq!(restored.objects.get(chosen).zone, Zone::Battlefield);
        assert_eq!(restored.objects.get(chosen).counters.plus1_plus1, 3);
        let marker = &restored.players[actor.index()].dungeon;
        assert_eq!((marker.dungeon_id, marker.room_id), (None, None));
        assert_eq!(marker.completed_dungeons, vec![UNDERCITY_DUNGEON_ID_V1]);
    }
}

#[test]
fn undercity_arena_keeps_painted_color_after_its_dungeon_leaves_both_seats() {
    use mtg_kernel::state::{UndercityRoomV1, UNDERCITY_DUNGEON_ID_V1};

    for actor in [P0, P1] {
        let mut state = islands();
        state.active_player = actor;
        state.priority_player = actor;
        in_room(&mut state, actor, UNDERCITY_DUNGEON_ID_V1, UndercityRoomV1::Forge.stable_id());
        let painter = paid_black_painter(&mut state, actor);
        let aluren = put(&mut state, actor, "Aluren", Zone::Hand);
        pool(&mut state, actor, &[ManaColor::G; 2], 2);
        cast(&mut state, aluren, &[]);
        settled(&mut state);
        let target = put(&mut state, actor.opponent(), "Llanowar Elves", Zone::Battlefield);
        let acererak = put(&mut state, actor, ACERERAK, Zone::Hand);
        cast(&mut state, acererak, &[]);
        choose_venture_route(&mut state, 1); // Arena.
        choose_target(&mut state, Target::Object(target));
        assert!(matches!(next(&mut state), Decision::CastSpellOrPass { player, .. } if player == actor));
        let mut active = state.clone();

        cast(&mut state, acererak, &[]);
        choose_venture_route(&mut state, 0); // Archives.
        pass_until(&mut state, |state| {
            state.players[actor.index()].dungeon.room_id == Some(UndercityRoomV1::Archives.stable_id())
                && state.stack.len() == 1
        });
        cast(&mut state, acererak, &[]);
        pass_until(&mut state, |state| {
            state.players[actor.index()].dungeon.room_id == Some(UndercityRoomV1::ThroneOfTheDeadThree.stable_id())
                && state.stack.len() == 2
        });
        cast(&mut state, acererak, &[]);
        assert!(matches!(settle(&mut state), Some(Decision::ChooseEffectOption { option_count: 3, .. })));
        assert_eq!(state.players[actor.index()].dungeon.completed_dungeons, vec![UNDERCITY_DUNGEON_ID_V1]);
        act(&mut state, Action::ChooseEffectOption(1)); // Start Mad Mage.
        pass_until(&mut state, |state| {
            state.players[actor.index()].dungeon.dungeon_id == Some(MAD_MAGE) && state.stack.len() == 2
        });

        for (branch, departed) in [(&mut active, false), (&mut state, true)] {
            let bounce = put(branch, actor, "Echoing Truth", Zone::Hand);
            pool(branch, actor, &[ManaColor::U], 1);
            cast(branch, bounce, &[Target::Object(painter)]);
            pass_until(branch, |state| state.objects.get(painter).zone == Zone::Hand);
            let blessing = put(branch, actor, "Benevolent Blessing", Zone::Hand);
            pool(branch, actor, &[ManaColor::W], 1);
            cast(branch, blessing, &[Target::Object(target)]);
            assert!(matches!(settle(branch), Some(Decision::ChooseEffectOption { option_count: 5, .. })));
            act(branch, Action::ChooseEffectOption(2));
            let mut restored: GameState =
                serde_json::from_slice(&serde_json::to_vec(branch).unwrap()).unwrap();
            settled(&mut restored);
            assert_eq!(restored.objects.get(target).v4.goaded_by.len(), usize::from(!departed));
        }
    }
}

#[test]
fn veils_of_fear_costs_two_life_unless_a_card_is_discarded() {
    let mut state = islands();
    in_room(&mut state, P0, TOMB, 1);
    let ponder = put(&mut state, P0, "Ponder", Zone::Hand);
    let bolt = put(&mut state, P1, "Lightning Bolt", Zone::Hand);
    cast_acererak(&mut state);
    match settle(&mut state) {
        Some(Decision::ChooseEffectOption { option_count, .. }) => {
            assert_eq!(option_count, 2, "Veils of Fear or Oubliette");
            act(&mut state, Action::ChooseEffectOption(0));
        }
        other => panic!("expected the room choice, got {other:?}"),
    }
    match settle(&mut state) {
        Some(Decision::ChooseEffectTargets {
            player,
            legal_targets,
            ..
        }) => {
            assert_eq!(player, P0);
            assert!(legal_targets.contains(&Target::Object(ponder)));
            act(
                &mut state,
                Action::ChooseEffectTarget(Target::Object(ponder)),
            );
        }
        other => panic!("expected P0's choice, got {other:?}"),
    }
    match settle(&mut state) {
        Some(Decision::ChooseEffectTargets {
            player,
            legal_targets,
            ..
        }) => {
            assert_eq!(player, P1);
            assert_eq!(legal_targets, vec![Target::Object(bolt)]);
            act(&mut state, Action::FinishEffectSelection);
        }
        other => panic!("expected P1's choice, got {other:?}"),
    }
    settled(&mut state);
    assert_eq!(state.objects.get(ponder).zone, Zone::Graveyard);
    assert_eq!(state.objects.get(bolt).zone, Zone::Hand);
    assert_eq!((state.players[0].life, state.players[1].life), (20, 18));
}

#[test]
fn deep_mines_scries_three() {
    let mut state = game(&["Island", "Ponder", "Lightning Bolt", "Island", "Island"]);
    let library = state.players[0].library.clone();
    in_room(&mut state, P0, MAD_MAGE, 7);
    cast_acererak(&mut state);
    match settle(&mut state) {
        Some(Decision::ChooseEffectTargets {
            legal_targets,
            min_targets,
            max_targets,
            ..
        }) => {
            assert_eq!((min_targets, max_targets), (0, 3));
            assert_eq!(legal_targets.len(), 3);
            act(
                &mut state,
                Action::ChooseEffectTarget(Target::Object(library[2])),
            );
            act(&mut state, Action::FinishEffectSelection);
        }
        other => panic!("expected the scry, got {other:?}"),
    }
    match settle(&mut state) {
        Some(Decision::ChooseEffectTargets {
            min_targets,
            max_targets,
            ..
        }) => {
            assert_eq!((min_targets, max_targets), (2, 2));
            // The last card of an ordered, forced selection is placed
            // automatically.
            act(
                &mut state,
                Action::ChooseEffectTarget(Target::Object(library[1])),
            );
        }
        other => panic!("expected the bottom order, got {other:?}"),
    }
    settled(&mut state);
    let after = &state.players[0].library;
    assert_eq!(after[0], library[2]);
    assert_eq!(&after[after.len() - 2..], &[library[1], library[0]]);
    let marker = &state.players[0].dungeon;
    assert_eq!(
        (marker.dungeon_id, marker.room_id),
        (Some(MAD_MAGE), Some(8))
    );
}

#[test]
fn mad_wizards_lair_casts_a_drawn_card_for_free_and_completes_the_dungeon() {
    let mut state = game(&["Dark Ritual", "Island", "Island", "Island", "Island"]);
    let ritual = state.players[0].library[0];
    in_room(&mut state, P0, MAD_MAGE, 8);
    cast_acererak(&mut state);
    let decision = settle(&mut state).expect("Mad Wizard's Lair choice");
    let marker = &state.players[0].dungeon;
    assert_eq!((marker.dungeon_id, marker.room_id), (Some(MAD_MAGE), Some(9)));
    assert!(marker.completed_dungeons.is_empty());
    state = serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
    assert_eq!(next(&mut state), decision);
    match decision {
        Decision::ChooseEffectTargets { legal_targets, .. } => {
            assert_eq!(legal_targets, vec![Target::Object(ritual)]);
            assert!(state
                .known_hand_cards(P1, P0)
                .iter()
                .any(|entry| entry.object == ritual));
            act(
                &mut state,
                Action::ChooseEffectTarget(Target::Object(ritual)),
            );
        }
        other => panic!("expected the free cast, got {other:?}"),
    }
    settled(&mut state);
    assert_eq!(state.objects.get(ritual).zone, Zone::Graveyard);
    assert_eq!(mana(&state, P0, ManaColor::B), 3, "cast without paying");
    assert_eq!(state.players[0].dungeon.completed_dungeons, vec![MAD_MAGE]);
}

#[test]
fn fungi_cavern_shrinks_a_creature_until_its_controllers_next_turn() {
    let mut state = islands();
    in_room(&mut state, P0, LOST_MINE, 3);
    let wraith = put(&mut state, P1, "Street Wraith", Zone::Battlefield);
    cast_acererak(&mut state);
    match settle(&mut state) {
        Some(Decision::ChooseEffectOption { option_count, .. }) => {
            assert_eq!(option_count, 2, "Dark Pool or Fungi Cavern");
            act(&mut state, Action::ChooseEffectOption(1));
        }
        other => panic!("expected the room choice, got {other:?}"),
    }
    for _ in 0..20 {
        match settle(&mut state) {
            Some(Decision::ChooseTargets { legal_targets, .. }) => {
                assert!(legal_targets.contains(&Target::Object(wraith)));
                act(&mut state, Action::ChooseTarget(Target::Object(wraith)));
            }
            None => break,
            other => panic!("unexpected decision: {other:?}"),
        }
    }
    assert_eq!(engine::effective_power(&state, wraith), -1);
    pass_until(&mut state, |state| {
        state.active_player == P1 && state.step == Step::Upkeep
    });
    assert_eq!(
        engine::effective_power(&state, wraith),
        -1,
        "P0's turn has not come"
    );
    pass_until(&mut state, |state| {
        state.active_player == P0 && state.step == Step::Upkeep
    });
    assert_eq!(engine::effective_power(&state, wraith), 3);
}

#[test]
fn acererak_attack_makes_a_zombie_unless_the_opponent_sacrifices_a_creature() {
    let mut state = islands();
    let acererak = put(&mut state, P0, ACERERAK, Zone::Battlefield);
    let wraith = put(&mut state, P1, "Street Wraith", Zone::Battlefield);
    pass_until(&mut state, |state| state.step == Step::DeclareAttackers);
    act(&mut state, Action::DeclareAttackers(vec![acererak]));
    match settle(&mut state) {
        Some(Decision::ChooseEffectTargets {
            player,
            legal_targets,
            ..
        }) => {
            assert_eq!(player, P1);
            assert_eq!(legal_targets, vec![Target::Object(wraith)]);
            act(&mut state, Action::FinishEffectSelection);
        }
        other => panic!("expected the sacrifice choice, got {other:?}"),
    }
    settled(&mut state);
    assert_eq!(state.objects.get(wraith).zone, Zone::Battlefield);
    assert!(state.players[0]
        .battlefield
        .iter()
        .any(|&object| state.objects.get(object).name == "Zombie Token"));
}
