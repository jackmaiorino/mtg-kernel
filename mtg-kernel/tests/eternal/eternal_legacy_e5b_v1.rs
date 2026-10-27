//! Eternal wave E5b: native rules tests for the cards that complete Legacy
//! Boros Energy, Eldrazi, Tron, Beanstalk Control and the Azorius, Jeskai
//! and Esper tempo and control lists.
#![cfg(feature = "eternal-fixtures")]

use mtg_kernel::card_def::{
    card_id_by_name, CardCapability, CardDef, CardType, Subtype, Supertype, TargetSpec, CARD_DEFS,
};
use mtg_kernel::engine::{self, Action, Decision};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::ManaColor;
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, StackItemKind, Step, Target, Zone};

const P0: PlayerId = PlayerId::P0;
const P1: PlayerId = PlayerId::P1;

const E5B_CARDS: [&str; 9] = [
    "Plateau",
    "Savannah",
    "Scrubland",
    "Snow-Covered Plains",
    "Arid Mesa",
    "Windswept Heath",
    "Goblin Bombardment",
    "Up the Beanstalk",
    "Stifle",
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
        0x4535_4221_0001,
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

/// The newest stack item, if it is a triggered ability.
fn top_trigger(state: &GameState) -> Option<Target> {
    state
        .stack
        .last()
        .filter(|item| item.kind == StackItemKind::TriggeredAbility)
        .map(|item| Target::StackItem(item.v4.stack_item_id))
}

// ---- Characteristics -----------------------------------------------------------

#[test]
fn e5b_cards_are_full_with_their_printed_characteristics() {
    for name in E5B_CARDS {
        assert_eq!(def(name).capability, CardCapability::Full, "{name}");
        assert!(!def(name).is_token, "{name}");
    }
    for (name, subtypes) in [
        ("Plateau", [Subtype::Mountain, Subtype::Plains]),
        ("Savannah", [Subtype::Forest, Subtype::Plains]),
        ("Scrubland", [Subtype::Plains, Subtype::Swamp]),
    ] {
        assert!(def(name).is_land, "{name}");
        assert_eq!(def(name).subtypes, &subtypes, "{name}");
        assert!(!def(name).enters_battlefield_tapped, "{name}");
    }
    let snow = def("Snow-Covered Plains");
    assert_eq!(snow.subtypes, &[Subtype::Plains]);
    assert!(snow.supertypes.contains(&Supertype::Basic));
    assert!(snow.supertypes.contains(&Supertype::Snow));
    assert_eq!(def("Goblin Bombardment").types, &[CardType::Enchantment]);
    assert_eq!(def("Goblin Bombardment").activated_abilities.len(), 1);
    assert_eq!(def("Up the Beanstalk").types, &[CardType::Enchantment]);
    assert_eq!(def("Stifle").target_spec, TargetSpec::StackAbility);
    for fetch in ["Arid Mesa", "Windswept Heath"] {
        assert_eq!(def(fetch).activated_abilities.len(), 1, "{fetch}");
    }
}

// ---- Lands ---------------------------------------------------------------------

#[test]
fn new_duals_and_snow_covered_plains_tap_for_their_colors() {
    for (name, colors) in [
        ("Plateau", vec![ManaColor::R, ManaColor::W]),
        ("Savannah", vec![ManaColor::G, ManaColor::W]),
        ("Scrubland", vec![ManaColor::W, ManaColor::B]),
        ("Snow-Covered Plains", vec![ManaColor::W]),
    ] {
        for color in colors {
            let mut state = islands();
            let land = put(&mut state, P0, name, Zone::Battlefield);
            next(&mut state);
            act(&mut state, Action::ActivateManaAbilityChoice(land, color));
            assert_eq!(
                state.players[0].mana_pool[color.pool_index()],
                1,
                "{name} {color:?}"
            );
            assert!(state.objects.get(land).tapped, "{name}");
        }
    }
    let mut state = islands();
    let plateau = put(&mut state, P0, "Plateau", Zone::Battlefield);
    for subtype in [Subtype::Mountain, Subtype::Plains] {
        assert!(engine::has_effective_subtype(&state, plateau, subtype));
    }
}

#[test]
fn arid_mesa_and_windswept_heath_fetch_either_land_type() {
    for (fetch, finds, misses) in [
        ("Arid Mesa", ["Mountain", "Plateau"], "Island"),
        ("Windswept Heath", ["Savannah", "Plains"], "Swamp"),
    ] {
        let mut state = game(&[finds[0], misses, finds[1]]);
        let library = state.players[0].library.clone();
        let land = put(&mut state, P0, fetch, Zone::Battlefield);
        activate(&mut state, land, 0, &[]);
        next(&mut state);
        assert_eq!(zone(&state, land), Zone::Graveyard, "{fetch}");
        assert_eq!(state.players[0].life, 19, "{fetch}");
        match settle(&mut state) {
            Some(Decision::ChooseEffectTargets { legal_targets, .. }) => {
                assert_eq!(legal_targets.len(), 2, "{fetch}");
                assert!(
                    legal_targets.contains(&Target::Object(library[0])),
                    "{fetch}"
                );
                assert!(
                    legal_targets.contains(&Target::Object(library[2])),
                    "{fetch}"
                );
                act(
                    &mut state,
                    Action::ChooseEffectTarget(Target::Object(library[2])),
                );
            }
            other => panic!("{fetch}: expected the search, got {other:?}"),
        }
        settle_first(&mut state);
        assert_eq!(zone(&state, library[2]), Zone::Battlefield, "{fetch}");
        assert!(!state.objects.get(library[2]).tapped, "{fetch}");
    }
}

// ---- Goblin Bombardment ----------------------------------------------------------

#[test]
fn goblin_bombardment_sacrifices_a_creature_to_deal_one_damage() {
    let mut state = islands();
    let bombardment = put(&mut state, P0, "Goblin Bombardment", Zone::Battlefield);
    assert!(
        !activatable(&mut state, bombardment, 0),
        "no creature to sacrifice"
    );
    let bear = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    let mut sacrificed = false;
    act(&mut state, Action::ActivateAbility(bombardment, 0));
    loop {
        match next(&mut state) {
            Decision::ChooseTargets { legal_targets, .. } => {
                assert!(legal_targets.contains(&Target::Player(P1)));
                act(&mut state, Action::ChooseTarget(Target::Player(P1)));
            }
            Decision::ChooseCostTargets { candidates, .. } => {
                assert_eq!(candidates, vec![bear]);
                act(&mut state, Action::ChooseCostTarget(bear));
                sacrificed = true;
            }
            _ => break,
        }
    }
    assert!(sacrificed);
    assert_eq!(zone(&state, bear), Zone::Graveyard, "sacrificed as a cost");
    settled(&mut state);
    assert_eq!(state.players[1].life, 19);
    assert_eq!(zone(&state, bombardment), Zone::Battlefield);
}

// ---- Up the Beanstalk -------------------------------------------------------------

#[test]
fn up_the_beanstalk_draws_on_entry_and_for_spells_with_mana_value_five_or_more() {
    let mut state = islands();
    let beanstalk = put(&mut state, P0, "Up the Beanstalk", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 1);
    let hand = state.players[0].hand.len();
    cast(&mut state, beanstalk, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, beanstalk), Zone::Battlefield);
    assert_eq!(state.players[0].hand.len(), hand, "cast one, drew one");

    let hand = state.players[0].hand.len();
    let small = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 0);
    cast(&mut state, small, &[Target::Player(P1)]);
    settled(&mut state);
    assert_eq!(
        state.players[0].hand.len(),
        hand,
        "mana value 1 draws nothing"
    );

    // Mana value 5 or more: draw.
    let big = put(&mut state, P0, "Gurmag Angler", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 6);
    let hand = state.players[0].hand.len();
    cast(&mut state, big, &[]);
    settle_first(&mut state);
    assert_eq!(state.players[0].hand.len(), hand, "cast one, drew one");
}

// ---- Stifle ------------------------------------------------------------------------

#[test]
fn stifle_counters_a_triggered_ability() {
    let mut state = islands();
    let beanstalk = put(&mut state, P0, "Up the Beanstalk", Zone::Hand);
    let stifle = put(&mut state, P0, "Stifle", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G, ManaColor::U], 1);
    cast(&mut state, beanstalk, &[]);
    pass_until(&mut state, |state| top_trigger(state).is_some());
    let trigger = top_trigger(&state).unwrap();
    let hand = state.players[0].hand.len();
    cast(&mut state, stifle, &[trigger]);
    settled(&mut state);
    assert_eq!(zone(&state, stifle), Zone::Graveyard);
    assert_eq!(
        state.players[0].hand.len(),
        hand - 1,
        "the entry draw was countered"
    );
    assert!(state.stack.is_empty());
}

#[test]
fn stifle_counters_an_activated_ability_but_not_a_spell() {
    let mut state = islands();
    let fetch = put(&mut state, P0, "Arid Mesa", Zone::Battlefield);
    let mountain = state.objects.push(GameObject {
        card_def: id("Mountain"),
        name: "Mountain".into(),
        owner: P0,
        controller: P0,
        zone: Zone::Library,
        tapped: false,
        summoning_sick: false,
        damage: 0,
        counters: Default::default(),
        attachments: Vec::new(),
        v4: ObjectStateV4::from_card_def(id("Mountain")),
        spell_copy_origin: None,
        plotted_turn: None,
        zone_change_count: 0,
    });
    state.players[0].library.insert(0, mountain);
    let stifle = put(&mut state, P1, "Stifle", Zone::Hand);
    activate(&mut state, fetch, 0, &[]);
    // P0 passes with the fetch ability on the stack; P1 answers it.
    assert!(matches!(
        next(&mut state),
        Decision::CastSpellOrPass { player: P0, .. }
    ));
    act(&mut state, Action::Pass);
    assert!(matches!(
        next(&mut state),
        Decision::CastSpellOrPass { player: P1, .. }
    ));
    let ability = state
        .stack
        .last()
        .map(|item| Target::StackItem(item.v4.stack_item_id))
        .unwrap();
    pool(&mut state, P1, &[ManaColor::U], 0);
    cast(&mut state, stifle, &[ability]);
    settled(&mut state);
    assert_eq!(zone(&state, fetch), Zone::Graveyard, "the cost was paid");
    assert_eq!(
        zone(&state, mountain),
        Zone::Library,
        "the search was countered"
    );

    // A spell is not a legal target.
    let mut state = islands();
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    let stifle = put(&mut state, P0, "Stifle", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R, ManaColor::U], 0);
    cast(&mut state, bolt, &[Target::Player(P1)]);
    assert!(!castable(&mut state, stifle), "no ability to target");
}
