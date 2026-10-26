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

const E3_CARDS: [&str; 19] = [
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
    act(&mut state, Action::ActivateManaAbility(cavern));
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
    act(&mut state, Action::Pass);
    // P1 answers with a blue Counterspell; P0 responds with Veil.
    let counterspell = put(&mut state, P1, "Counterspell", Zone::Hand);
    pool(&mut state, P1, &[ManaColor::U, ManaColor::U], 0);
    cast(&mut state, counterspell, &[Target::Object(bolt)]);
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
    // A black Thoughtseize from P1 can't target P0; a red Bolt can.
    state.priority_player = P1;
    let thoughtseize = put(&mut state, P1, "Thoughtseize", Zone::Hand);
    let bolt = put(&mut state, P1, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P1, &[ManaColor::B, ManaColor::R], 0);
    act(&mut state, Action::CastSpell(thoughtseize));
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
