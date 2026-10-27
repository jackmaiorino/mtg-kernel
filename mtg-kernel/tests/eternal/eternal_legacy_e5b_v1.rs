//! Eternal wave E5b: native rules tests for the cards that complete Legacy
//! Boros Energy, Eldrazi, Tron, Beanstalk Control and the Azorius, Jeskai
//! and Esper tempo and control lists.
#![cfg(feature = "eternal-fixtures")]

use mtg_kernel::card_def::{
    card_id_by_name, CardCapability, CardDef, CardType, Keywords, Subtype, Supertype, TargetSpec,
    CARD_DEFS,
};
use mtg_kernel::engine::{self, Action, CastMode, Decision};
use mtg_kernel::eternal_cards_v1::energy;
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::ManaColor;
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, StackItemKind, Step, Target, Zone};

const P0: PlayerId = PlayerId::P0;
const P1: PlayerId = PlayerId::P1;

const E5B_CARDS: [&str; 35] = [
    "Plateau",
    "Savannah",
    "Scrubland",
    "Snow-Covered Plains",
    "Arid Mesa",
    "Windswept Heath",
    "Goblin Bombardment",
    "Up the Beanstalk",
    "Stifle",
    "Consign to Memory",
    "Galvanic Discharge",
    "Wrath of the Skies",
    "Terminus",
    "Leyline Binding",
    "Static Prison",
    "Phelia, Exuberant Shepherd",
    "Quantum Riddler",
    "Guide of Souls",
    "Ocelot Pride",
    "Ajani, Nacatl Pariah",
    "Amped Raptor",
    "Phlage, Titan of Fire's Fury",
    "Sand Scout",
    "Voice of Victory",
    "Lazotep Quarry",
    "Thought-Knot Seer",
    "Reality Smasher",
    "It That Heralds the End",
    "Eldrazi Linebreaker",
    "Wastescape Battlemage",
    "Eldrazi Temple",
    "Eye of Ugin",
    "Abundant Countryside",
    "Petrified Hamlet",
    "Eldrazi Confluence",
];

const E5B_TOKENS: [&str; 6] = [
    "Cat Warrior Token",
    "Cat Token",
    "Warrior Token",
    "Sand Warrior Token",
    "Shapeshifter Token",
    "Eldrazi Scion Token",
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
        Zone::Library => seat.library.insert(0, object),
        _ => panic!("helper zone"),
    }
    object
}

/// A tapped land: on the battlefield but no mana source this turn.
fn put_tapped(state: &mut GameState, player: PlayerId, name: &str) -> ObjectId {
    let land = put(state, player, name, Zone::Battlefield);
    state.objects.get_mut(land).tapped = true;
    land
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

fn named(state: &GameState, player: PlayerId, name: &str) -> Vec<ObjectId> {
    [P0, P1]
        .into_iter()
        .flat_map(|seat| state.players[seat.index()].battlefield.iter().copied())
        .filter(|&object| {
            let live = state.objects.get(object);
            live.controller == player && CARD_DEFS[live.card_def as usize].name == name
        })
        .collect()
}

fn power_toughness(state: &GameState, object: ObjectId) -> (i32, i32) {
    (
        engine::effective_power(state, object),
        engine::effective_toughness(state, object),
    )
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

/// Plays on until `stop` holds before a decision: passing priority, not
/// attacking or blocking, ordering triggers as given and answering any
/// other choice with `answer` (or the first legal answer).
fn run_until(
    state: &mut GameState,
    stop: impl Fn(&GameState) -> bool,
    mut answer: impl FnMut(&GameState, &Decision) -> Option<Action>,
) {
    for _ in 0..2000 {
        let decision = next(state);
        if stop(state) {
            return;
        }
        if let Some(action) = answer(state, &decision) {
            act(state, action);
            continue;
        }
        let action = match decision {
            Decision::CastSpellOrPass { .. } => Action::Pass,
            Decision::DeclareAttackers { .. } => Action::DeclareAttackers(Vec::new()),
            Decision::DeclareBlockers { .. } => Action::DeclareBlockers(Vec::new()),
            Decision::OrderTriggers { pending, .. } => {
                Action::OrderTriggers((0..pending.len()).collect())
            }
            Decision::ChooseCombatDamageRange { .. } => {
                Action::ChooseCombatDamageRange { upper_half: true }
            }
            Decision::ChooseAttackTarget {
                player, candidates, ..
            } => Action::ChooseAttackTarget(
                if candidates.contains(&Target::Player(player.opponent())) {
                    Target::Player(player.opponent())
                } else {
                    candidates[0]
                },
            ),
            other => first_answer(other),
        };
        act(state, action);
    }
    panic!("condition never held");
}

/// Plays on with first answers until `stop` holds.
fn pass_until(state: &mut GameState, stop: impl Fn(&GameState) -> bool) {
    run_until(state, stop, |_, _| None);
}

/// P0 attacks with `attackers` in this turn's combat, then the game plays
/// on to P0's second main phase with an empty stack. `answer` answers other
/// choices.
fn attack_with(
    state: &mut GameState,
    attackers: Vec<ObjectId>,
    answer: impl FnMut(&GameState, &Decision) -> Option<Action>,
) {
    pass_until(state, |state| state.step == Step::DeclareAttackers);
    act(state, Action::DeclareAttackers(attackers));
    run_until(
        state,
        |state| {
            state.step == Step::Main2
                && state.stack.is_empty()
                && state.engine.pending_triggers.is_empty()
                && state.engine.pending_effect.is_none()
        },
        answer,
    );
}

/// The newest stack item, if it is a triggered ability.
fn top_trigger(state: &GameState) -> Option<Target> {
    state
        .stack
        .last()
        .filter(|item| item.kind == StackItemKind::TriggeredAbility)
        .map(|item| Target::StackItem(item.v4.stack_item_id))
}

fn hand_contains(state: &GameState, player: PlayerId, object: ObjectId) -> bool {
    state.players[player.index()].hand.contains(&object)
}

// ---- Characteristics -----------------------------------------------------------

#[test]
fn e5b_cards_are_full_with_their_printed_characteristics() {
    for name in E5B_CARDS {
        assert_eq!(def(name).capability, CardCapability::Full, "{name}");
        assert!(!def(name).is_token, "{name}");
    }
    for name in E5B_TOKENS {
        assert_eq!(def(name).capability, CardCapability::Full, "{name}");
        assert!(def(name).is_token, "{name}");
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
    assert_eq!(def("Stifle").target_spec, TargetSpec::StackAbility);
    for fetch in ["Arid Mesa", "Windswept Heath"] {
        assert_eq!(def(fetch).activated_abilities.len(), 1, "{fetch}");
    }
    let ajani = def("Ajani, Nacatl Pariah");
    assert_eq!(ajani.subtypes, &[Subtype::Cat, Subtype::Warrior]);
    assert_eq!(ajani.supertypes, &[Supertype::Legendary]);
    assert_eq!((ajani.power, ajani.toughness), (Some(1), Some(2)));
    let back = ajani.transform_face.as_ref().expect("back face");
    assert_eq!(back.name, "Ajani, Nacatl Avenger");
    assert_eq!(back.types, &[CardType::Planeswalker]);
    assert_eq!(back.subtypes, &[Subtype::Ajani]);
    assert_eq!(ajani.starting_loyalty, Some(3));
    assert_eq!(ajani.activated_abilities.len(), 3);
    assert!(def("Amped Raptor").keywords.has(Keywords::FIRST_STRIKE));
    let ocelot = def("Ocelot Pride");
    assert!(ocelot.keywords.has(Keywords::FIRST_STRIKE) && ocelot.keywords.has(Keywords::LIFELINK));
    assert!(def("Phelia, Exuberant Shepherd")
        .keywords
        .has(Keywords::FLASH));
    assert!(def("Leyline Binding").keywords.has(Keywords::FLASH));
    let riddler = def("Quantum Riddler");
    assert!(riddler.keywords.has(Keywords::FLYING));
    assert_eq!(riddler.subtypes, &[Subtype::Sphinx]);
    assert!(riddler.alt_cost.is_some(), "warp");
    let smasher = def("Reality Smasher");
    assert!(smasher.keywords.has(Keywords::TRAMPLE) && smasher.keywords.has(Keywords::HASTE));
    assert!(smasher.colors.is_empty());
    assert!(def("Eldrazi Linebreaker").keywords.has(Keywords::TRAMPLE));
    assert!(def("Eldrazi Linebreaker").colors.is_empty(), "devoid");
    assert_eq!(def("Thought-Knot Seer").mana_value, 4);
    assert_eq!(def("Thought-Knot Seer").cost.generic, 3);
    assert_eq!(
        def("Voice of Victory").subtypes,
        &[Subtype::Human, Subtype::Bard]
    );
    assert_eq!(
        def("Sand Warrior Token").subtypes,
        &[Subtype::Sand, Subtype::Warrior]
    );
    assert_eq!(
        def("Eldrazi Scion Token").subtypes,
        &[Subtype::Eldrazi, Subtype::Scion]
    );
    assert!(def("Shapeshifter Token").changeling);
    assert!(def("Eldrazi Scion Token").mana_ability_def.is_some());
    for name in ["Bard", "Sphinx", "Sand", "Scion"] {
        let subtype = match name {
            "Bard" => Subtype::Bard,
            "Sphinx" => Subtype::Sphinx,
            "Sand" => Subtype::Sand,
            _ => Subtype::Scion,
        };
        assert!(subtype.is_creature_type(), "{name}");
    }
    assert_eq!(def("Lazotep Quarry").subtypes, &[Subtype::Desert]);
    assert_eq!(def("Lazotep Quarry").additional_mana_abilities.len(), 1);
    assert_eq!(def("Eye of Ugin").supertypes, &[Supertype::Legendary]);
    assert!(def("Phlage, Titan of Fire's Fury").escape.is_some());
    assert!(def("Wastescape Battlemage").kicker_cost.is_some());
    assert!(def("Wastescape Battlemage").alt_cost.is_some());
    let confluence = def("Eldrazi Confluence");
    assert!(confluence.mode2.is_some() && confluence.mode3.is_some());
    assert_eq!(confluence.additional_modes.len(), 7);
    assert_eq!(def("Wrath of the Skies").cost.x_count, 1);
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
        let single = colors.len() == 1;
        for color in colors {
            let mut state = islands();
            let land = put(&mut state, P0, name, Zone::Battlefield);
            next(&mut state);
            act(
                &mut state,
                if single {
                    Action::ActivateManaAbility(land)
                } else {
                    Action::ActivateManaAbilityChoice(land, color)
                },
            );
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

#[test]
fn abundant_countryside_adds_any_color_only_for_creature_spells_and_makes_shapeshifters() {
    let mut state = islands();
    let countryside = put(&mut state, P0, "Abundant Countryside", Zone::Battlefield);
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Hand);
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    assert!(
        castable(&mut state, elves),
        "creature spell mana of any color"
    );
    assert!(!castable(&mut state, bolt), "not for a noncreature spell");
    next(&mut state);
    act(
        &mut state,
        Action::ActivateManaAbilityChoice(countryside, ManaColor::C),
    );
    assert_eq!(state.players[0].mana_pool[ManaColor::C.pool_index()], 1);

    let mut state = islands();
    let countryside = put(&mut state, P0, "Abundant Countryside", Zone::Battlefield);
    pool(&mut state, P0, &[], 6);
    activate(&mut state, countryside, 0, &[]);
    settled(&mut state);
    let tokens = named(&state, P0, "Shapeshifter Token");
    assert_eq!(tokens.len(), 1);
    assert_eq!(power_toughness(&state, tokens[0]), (1, 1));
    for subtype in [Subtype::Cat, Subtype::Eldrazi, Subtype::Warrior] {
        assert!(engine::has_effective_subtype(&state, tokens[0], subtype));
    }
}

#[test]
fn eldrazi_temple_pays_two_for_colorless_eldrazi_spells_only() {
    let mut state = islands();
    let temple = put(&mut state, P0, "Eldrazi Temple", Zone::Battlefield);
    let seer = put(&mut state, P0, "Thought-Knot Seer", Zone::Hand);
    let binding = put(&mut state, P0, "Leyline Binding", Zone::Hand);
    pool(&mut state, P0, &[], 1);
    assert!(!castable(&mut state, seer), "three of {{3}}{{C}}");
    pool(&mut state, P0, &[], 1);
    assert!(
        castable(&mut state, seer),
        "the Temple pays two of {{3}}{{C}}"
    );
    pool(&mut state, P0, &[ManaColor::W], 1);
    assert!(
        !castable(&mut state, binding),
        "the Temple pays one of {{5}}{{W}}"
    );
    let before = state.players[0].mana_pool[ManaColor::C.pool_index()];
    next(&mut state);
    act(&mut state, Action::ActivateManaAbility(temple));
    assert_eq!(
        state.players[0].mana_pool[ManaColor::C.pool_index()],
        before + 1
    );
}

#[test]
fn eye_of_ugin_discounts_colorless_eldrazi_and_finds_colorless_creatures() {
    let mut state = game(&["Island", "Reality Smasher", "Lightning Bolt"]);
    let smasher = state.players[0].library[1];
    let eye = put(&mut state, P0, "Eye of Ugin", Zone::Battlefield);
    let seer = put(&mut state, P0, "Thought-Knot Seer", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::C], 1);
    assert!(castable(&mut state, seer), "{{3}}{{C}} less {{2}}");
    state.players[0].mana_pool = [0; 6];
    pool(&mut state, P0, &[], 7);
    activate(&mut state, eye, 0, &[]);
    match settle(&mut state) {
        Some(Decision::ChooseEffectTargets { legal_targets, .. }) => {
            assert_eq!(legal_targets, vec![Target::Object(smasher)]);
            act(
                &mut state,
                Action::ChooseEffectTarget(Target::Object(smasher)),
            );
        }
        other => panic!("expected the search, got {other:?}"),
    }
    settle_first(&mut state);
    assert!(hand_contains(&state, P0, smasher));
    assert!(state.objects.get(eye).tapped);
}

#[test]
fn lazotep_quarry_sacrifices_a_creature_for_any_color() {
    let mut state = islands();
    let quarry = put(&mut state, P0, "Lazotep Quarry", Zone::Battlefield);
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    next(&mut state);
    act(
        &mut state,
        Action::ActivateManaAbilityWithCostTarget(quarry, ManaColor::R, elves),
    );
    assert_eq!(zone(&state, elves), Zone::Graveyard);
    assert_eq!(state.players[0].mana_pool[ManaColor::R.pool_index()], 1);
    assert!(state.objects.get(quarry).tapped);
}

#[test]
fn lazotep_quarry_makes_a_four_four_black_zombie_copy() {
    let mut state = islands();
    let quarry = put(&mut state, P0, "Lazotep Quarry", Zone::Battlefield);
    let seer = put(&mut state, P0, "Thought-Knot Seer", Zone::Graveyard);
    pool(&mut state, P0, &[], 6);
    activate(&mut state, quarry, 0, &[Target::Object(seer)]);
    settle_first(&mut state);
    assert_eq!(zone(&state, quarry), Zone::Graveyard, "sacrificed a Desert");
    assert_eq!(zone(&state, seer), Zone::Exile);
    assert_eq!(
        state.players[0].mana_pool[ManaColor::C.pool_index()],
        0,
        "X was its mana value 4, plus 2"
    );
    let copies = named(&state, P0, "Thought-Knot Seer");
    assert_eq!(copies.len(), 1);
    let copy = copies[0];
    assert!(state.objects.get(copy).v4.is_token);
    assert_eq!(power_toughness(&state, copy), (4, 4));
    assert!(engine::has_effective_subtype(&state, copy, Subtype::Zombie));
    assert!(!engine::has_effective_subtype(
        &state,
        copy,
        Subtype::Eldrazi
    ));
}

#[test]
fn petrified_hamlet_names_a_land_whose_abilities_stop_and_which_taps_for_colorless() {
    let mut state = islands();
    let mesa = put(&mut state, P1, "Arid Mesa", Zone::Battlefield);
    let hamlet = put(&mut state, P0, "Petrified Hamlet", Zone::Hand);
    next(&mut state);
    act(&mut state, Action::PlayLand(hamlet));
    let options = mtg_kernel::eternal_legacy_e5b_v1::hamlet_name_options();
    let mesa_option = options
        .iter()
        .position(|&card_def| card_def == id("Arid Mesa"))
        .unwrap() as u16;
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseEffectOption { option_count, .. } => {
            assert_eq!(usize::from(option_count), options.len());
            Action::ChooseEffectOption(mesa_option)
        }
        other => first_answer(other),
    });
    assert_eq!(
        mtg_kernel::eternal_legacy_e5b_v1::hamlet_name(&state, hamlet),
        Some(id("Arid Mesa"))
    );
    state.active_player = P1;
    state.priority_player = P1;
    let (_, abilities) = priority(&mut state);
    assert!(!abilities.contains(&(mesa, 0)), "the fetch is shut off");
    act(&mut state, Action::ActivateManaAbility(mesa));
    assert_eq!(state.players[1].mana_pool[ManaColor::C.pool_index()], 1);
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
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    act(&mut state, Action::ActivateAbility(bombardment, 0));
    loop {
        match next(&mut state) {
            Decision::ChooseTargets { legal_targets, .. } => {
                assert!(legal_targets.contains(&Target::Player(P1)));
                act(&mut state, Action::ChooseTarget(Target::Player(P1)));
            }
            Decision::ChooseCostTargets { candidates, .. } => {
                assert_eq!(candidates, vec![elves]);
                act(&mut state, Action::ChooseCostTarget(elves));
            }
            _ => break,
        }
    }
    assert_eq!(zone(&state, elves), Zone::Graveyard, "sacrificed as a cost");
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

    let big = put(&mut state, P0, "Gurmag Angler", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 6);
    let hand = state.players[0].hand.len();
    cast(&mut state, big, &[]);
    settle_first(&mut state);
    assert_eq!(state.players[0].hand.len(), hand, "cast one, drew one");
}

// ---- Stifle and Consign to Memory -------------------------------------------------

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
    let mountain = put(&mut state, P0, "Mountain", Zone::Library);
    let stifle = put(&mut state, P1, "Stifle", Zone::Hand);
    activate(&mut state, fetch, 0, &[]);
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

    let mut state = islands();
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    let stifle = put(&mut state, P0, "Stifle", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R, ManaColor::U], 0);
    cast(&mut state, bolt, &[Target::Player(P1)]);
    assert!(!castable(&mut state, stifle), "no ability to target");
}

/// Casts Consign to Memory at `target` without replicating it.
fn cast_consign(state: &mut GameState, consign: ObjectId, target: Target) {
    cast(state, consign, &[target]);
    if let Decision::ChooseEffectOption { .. } = next(state) {
        act(state, Action::ChooseEffectOption(0));
    }
}

#[test]
fn consign_to_memory_counters_a_triggered_ability() {
    let mut state = islands();
    let beanstalk = put(&mut state, P0, "Up the Beanstalk", Zone::Hand);
    let consign = put(&mut state, P0, "Consign to Memory", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G, ManaColor::U], 1);
    cast(&mut state, beanstalk, &[]);
    pass_until(&mut state, |state| top_trigger(state).is_some());
    let trigger = top_trigger(&state).unwrap();
    let hand = state.players[0].hand.len();
    cast_consign(&mut state, consign, trigger);
    settled(&mut state);
    assert_eq!(zone(&state, consign), Zone::Graveyard);
    assert_eq!(state.players[0].hand.len(), hand - 1, "no entry draw");
}

#[test]
fn consign_to_memory_counters_a_colorless_spell_but_not_a_colored_one() {
    let mut state = islands();
    state.priority_player = P1;
    let seer = put(&mut state, P1, "Thought-Knot Seer", Zone::Hand);
    let consign = put(&mut state, P0, "Consign to Memory", Zone::Hand);
    pool(&mut state, P1, &[ManaColor::C], 3);
    state.active_player = P1;
    state.step = Step::Main1;
    cast(&mut state, seer, &[]);
    assert!(matches!(
        next(&mut state),
        Decision::CastSpellOrPass { player: P1, .. }
    ));
    act(&mut state, Action::Pass);
    pool(&mut state, P0, &[ManaColor::U], 0);
    cast_consign(&mut state, consign, Target::Object(seer));
    settle_first(&mut state);
    assert_eq!(zone(&state, seer), Zone::Graveyard, "countered");

    let mut state = islands();
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    let consign = put(&mut state, P0, "Consign to Memory", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R, ManaColor::U], 0);
    cast(&mut state, bolt, &[Target::Player(P1)]);
    assert!(
        !castable(&mut state, consign),
        "a red spell is not a target"
    );
}

// ---- Energy spells ------------------------------------------------------------------

#[test]
fn galvanic_discharge_gets_three_energy_and_pays_any_amount_as_damage() {
    for (paid, survives) in [(3_u16, false), (1, true)] {
        let mut state = islands();
        let flamebreather = put(&mut state, P1, "Kessig Flamebreather", Zone::Battlefield);
        let discharge = put(&mut state, P0, "Galvanic Discharge", Zone::Hand);
        pool(&mut state, P0, &[ManaColor::R], 0);
        cast(&mut state, discharge, &[Target::Object(flamebreather)]);
        settle_with(&mut state, |_, decision| match decision {
            Decision::ChooseEffectOption { option_count, .. } => {
                assert_eq!(option_count, 4, "pay 0 to 3");
                Action::ChooseEffectOption(paid)
            }
            other => first_answer(other),
        });
        assert_eq!(
            zone(&state, flamebreather) == Zone::Battlefield,
            survives,
            "paid {paid}"
        );
        assert_eq!(energy(&state, P0), 3 - u32::from(paid));
    }
}

#[test]
fn wrath_of_the_skies_destroys_by_mana_value_up_to_the_energy_paid() {
    let mut state = islands();
    let elves = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    let angler = put(&mut state, P1, "Gurmag Angler", Zone::Battlefield);
    let beanstalk = put(&mut state, P1, "Up the Beanstalk", Zone::Battlefield);
    let wrath = put(&mut state, P0, "Wrath of the Skies", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W, ManaColor::W], 2);
    assert!(castable(&mut state, wrath));
    act(&mut state, Action::CastSpell(wrath));
    // X = 2, then pay both energy.
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseEffectOption { option_count, .. } => {
            Action::ChooseEffectOption(option_count - 1)
        }
        other => first_answer(other),
    });
    assert_eq!(zone(&state, elves), Zone::Graveyard);
    assert_eq!(zone(&state, beanstalk), Zone::Graveyard, "an enchantment");
    assert_eq!(zone(&state, angler), Zone::Battlefield, "mana value 7");
    assert_eq!(energy(&state, P0), 0);
}

// ---- Terminus and miracle -----------------------------------------------------------

#[test]
fn terminus_puts_every_creature_on_the_bottom_in_its_owners_order() {
    let mut state = islands();
    let first = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    let second = put(&mut state, P0, "Kessig Flamebreather", Zone::Battlefield);
    let theirs = put(&mut state, P1, "Gurmag Angler", Zone::Battlefield);
    let terminus = put(&mut state, P0, "Terminus", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W, ManaColor::W], 4);
    cast(&mut state, terminus, &[]);
    let mut order = vec![second, first];
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseEffectTargets { legal_targets, .. } => {
            let pick = order.remove(0);
            assert!(legal_targets.contains(&Target::Object(pick)));
            Action::ChooseEffectTarget(Target::Object(pick))
        }
        other => first_answer(other),
    });
    for creature in [first, second, theirs] {
        assert_eq!(zone(&state, creature), Zone::Library);
    }
    let library = &state.players[0].library;
    assert_eq!(&library[library.len() - 2..], &[second, first]);
    assert_eq!(*state.players[1].library.last().unwrap(), theirs);
}

#[test]
fn terminus_miracle_reveals_the_first_draw_and_casts_for_white() {
    let mut state = islands();
    // P0 draws Terminus as the first card of their next turn.
    let terminus = put(&mut state, P0, "Terminus", Zone::Library);
    let plains = put(&mut state, P0, "Plains", Zone::Battlefield);
    let elves = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    run_until(
        &mut state,
        |state| hand_contains(state, P0, terminus),
        |_, _| None,
    );
    // The reveal is offered to P0 before anything else happens.
    let decision = next(&mut state);
    assert!(
        matches!(
            decision,
            Decision::ChooseEffectOption {
                player: P0,
                source,
                option_count: 2,
            } if source == terminus
        ),
        "{decision:?} offers {:?}",
        state.eternal_v1.as_ref().map(|eternal| &eternal.e5b)
    );
    act(&mut state, Action::ChooseEffectOption(1));
    let after = next(&mut state);
    assert!(
        state.stack.iter().any(|item| item.source == terminus),
        "after the reveal: {after:?}
stack {:?}
pending {:?}
e5b {:?}",
        state.stack,
        state.engine.pending_triggers,
        state.eternal_v1.as_ref().map(|eternal| &eternal.e5b)
    );
    // The miracle trigger offers the cast for {W}.
    let mut offered = false;
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseEffectOption {
            player,
            option_count,
            ..
        } => {
            assert_eq!((player, option_count), (P0, 2), "cast or decline");
            offered = true;
            Action::ChooseEffectOption(0)
        }
        Decision::ChooseEffectTargets { legal_targets, .. }
            if legal_targets == vec![Target::Object(terminus)] =>
        {
            Action::ChooseEffectTarget(Target::Object(terminus))
        }
        other => first_answer(other),
    });
    assert!(
        offered,
        "no cast offered; Terminus in {:?}, stack {:?}",
        zone(&state, terminus),
        state.stack
    );
    assert_eq!(zone(&state, elves), Zone::Library);
    assert_eq!(zone(&state, terminus), Zone::Graveyard);
    assert!(state.objects.get(plains).tapped, "paid {{W}}");
}

#[test]
fn a_miracle_card_that_is_not_the_first_draw_is_not_offered() {
    let mut state = game(&["Island", "Terminus", "Island"]);
    let terminus = state.players[0].library[1];
    state.active_player = P1;
    state.priority_player = P1;
    event::propose_and_commit(&mut state, ProposedEvent::draw(P0));
    event::propose_and_commit(&mut state, ProposedEvent::draw(P0));
    assert!(hand_contains(&state, P0, terminus));
    settled(&mut state);
}

// ---- Leyline Binding and Static Prison ---------------------------------------------

#[test]
fn leyline_binding_costs_one_less_per_basic_land_type_and_exiles_until_it_leaves() {
    let mut state = islands();
    put_tapped(&mut state, P0, "Plateau");
    put_tapped(&mut state, P0, "Tundra");
    let binding = put(&mut state, P0, "Leyline Binding", Zone::Hand);
    let angler = put(&mut state, P1, "Gurmag Angler", Zone::Battlefield);
    pool(&mut state, P0, &[ManaColor::W], 1);
    assert!(!castable(&mut state, binding), "three types: {{2}}{{W}}");
    pool(&mut state, P0, &[], 1);
    cast(&mut state, binding, &[]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseTargets { legal_targets, .. } => {
            assert!(legal_targets.contains(&Target::Object(angler)));
            Action::ChooseTarget(Target::Object(angler))
        }
        other => first_answer(other),
    });
    assert_eq!(zone(&state, angler), Zone::Exile);
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(binding, Zone::Graveyard),
    );
    settled(&mut state);
    assert_eq!(zone(&state, angler), Zone::Battlefield, "returned");
}

#[test]
fn leyline_binding_has_flash() {
    let mut state = islands();
    state.active_player = P1;
    let binding = put(&mut state, P0, "Leyline Binding", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W], 5);
    assert!(castable(&mut state, binding));
}

#[test]
fn static_prison_exiles_gets_energy_and_is_kept_by_paying_energy() {
    let mut state = islands();
    let prison = put(&mut state, P0, "Static Prison", Zone::Hand);
    let angler = put(&mut state, P1, "Gurmag Angler", Zone::Battlefield);
    pool(&mut state, P0, &[ManaColor::W], 0);
    cast(&mut state, prison, &[]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseTargets { .. } => Action::ChooseTarget(Target::Object(angler)),
        other => first_answer(other),
    });
    assert_eq!(zone(&state, angler), Zone::Exile);
    assert_eq!(energy(&state, P0), 2);
    // P0's next first main phase: pay {E} (option 0).
    let turn = state.turn;
    run_until(
        &mut state,
        |state| {
            state.turn > turn
                && state.active_player == P0
                && state.step == Step::Main1
                && state.stack.is_empty()
                && state.engine.pending_triggers.is_empty()
                && state.engine.pending_effect.is_none()
        },
        |_, decision| match decision {
            Decision::ChooseEffectOption { .. } => Some(Action::ChooseEffectOption(0)),
            _ => None,
        },
    );
    assert_eq!(energy(&state, P0), 1);
    assert_eq!(zone(&state, prison), Zone::Battlefield);
    // Next time, decline: the Prison is sacrificed and the creature returns.
    let turn = state.turn;
    run_until(
        &mut state,
        |state| {
            state.turn > turn
                && state.active_player == P0
                && state.step == Step::Main1
                && state.stack.is_empty()
                && state.engine.pending_triggers.is_empty()
                && state.engine.pending_effect.is_none()
        },
        |_, decision| match decision {
            Decision::ChooseEffectOption { .. } => Some(Action::ChooseEffectOption(1)),
            _ => None,
        },
    );
    assert_eq!(zone(&state, prison), Zone::Graveyard);
    assert_eq!(zone(&state, angler), Zone::Battlefield);
    assert_eq!(energy(&state, P0), 1);
}

// ---- Creatures ----------------------------------------------------------------------

#[test]
fn thought_knot_seer_exiles_a_nonland_card_and_its_leaving_draws_the_opponent_a_card() {
    let mut state = islands();
    let bolt = put(&mut state, P1, "Lightning Bolt", Zone::Hand);
    let island = put(&mut state, P1, "Island", Zone::Hand);
    let seer = put(&mut state, P0, "Thought-Knot Seer", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::C], 3);
    cast(&mut state, seer, &[]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseTargets { legal_targets, .. } => {
            assert_eq!(legal_targets, vec![Target::Player(P1)]);
            Action::ChooseTarget(Target::Player(P1))
        }
        other => first_answer(other),
    });
    assert_eq!(zone(&state, bolt), Zone::Exile);
    assert!(hand_contains(&state, P1, island), "lands stay");
    let hand = state.players[1].hand.len();
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(seer, Zone::Graveyard),
    );
    settle_first(&mut state);
    assert_eq!(state.players[1].hand.len(), hand + 1);
}

/// P1 casts Lightning Bolt at `target` and both pass so its triggers
/// resolve.
fn opponent_bolts(state: &mut GameState, target: Target) -> ObjectId {
    state.priority_player = P1;
    let bolt = put(state, P1, "Lightning Bolt", Zone::Hand);
    pool(state, P1, &[ManaColor::R], 0);
    cast(state, bolt, &[target]);
    bolt
}

#[test]
fn reality_smasher_counters_a_targeting_spell_unless_its_controller_discards() {
    // Discard: the spell resolves.
    let mut state = islands();
    let smasher = put(&mut state, P0, "Reality Smasher", Zone::Battlefield);
    let spare = put(&mut state, P1, "Island", Zone::Hand);
    let bolt = opponent_bolts(&mut state, Target::Object(smasher));
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseEffectTargets {
            player,
            legal_targets,
            ..
        } => {
            assert_eq!(player, P1);
            assert!(legal_targets.contains(&Target::Object(spare)));
            Action::ChooseEffectTarget(Target::Object(spare))
        }
        other => first_answer(other),
    });
    assert_eq!(zone(&state, spare), Zone::Graveyard, "discarded");
    assert_eq!(zone(&state, bolt), Zone::Graveyard);
    assert_eq!(state.objects.get(smasher).damage, 3, "the bolt resolved");

    // No discard: countered.
    let mut state = islands();
    let smasher = put(&mut state, P0, "Reality Smasher", Zone::Battlefield);
    put(&mut state, P1, "Island", Zone::Hand);
    let bolt = opponent_bolts(&mut state, Target::Object(smasher));
    settle_first(&mut state);
    assert_eq!(zone(&state, bolt), Zone::Graveyard);
    assert_eq!(state.objects.get(smasher).damage, 0, "countered");

    // Empty hand: countered.
    let mut state = islands();
    let smasher = put(&mut state, P0, "Reality Smasher", Zone::Battlefield);
    opponent_bolts(&mut state, Target::Object(smasher));
    settle_first(&mut state);
    assert_eq!(state.objects.get(smasher).damage, 0);
}

#[test]
fn it_that_heralds_the_end_pumps_other_colorless_creatures_and_discounts_big_colorless_spells() {
    let mut state = islands();
    let herald = put(&mut state, P0, "It That Heralds the End", Zone::Battlefield);
    let seer = put(&mut state, P0, "Thought-Knot Seer", Zone::Battlefield);
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    assert_eq!(power_toughness(&state, seer), (5, 5));
    assert_eq!(power_toughness(&state, elves), (1, 1));
    assert_eq!(power_toughness(&state, herald), (2, 2), "other creatures");
    let emrakul = put(&mut state, P0, "Emrakul, the Aeons Torn", Zone::Hand);
    pool(&mut state, P0, &[], 14);
    assert!(castable(&mut state, emrakul), "{{15}} less {{1}}");
}

#[test]
fn eldrazi_linebreaker_gives_haste_and_plus_x_at_the_beginning_of_combat() {
    let mut state = islands();
    put(&mut state, P0, "Eldrazi Linebreaker", Zone::Battlefield);
    put(&mut state, P0, "Thought-Knot Seer", Zone::Battlefield);
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    run_until(
        &mut state,
        |state| state.step == Step::DeclareAttackers,
        |_, decision| match decision {
            Decision::ChooseTargets { legal_targets, .. } => {
                assert!(legal_targets.contains(&Target::Object(elves)));
                Some(Action::ChooseTarget(Target::Object(elves)))
            }
            _ => None,
        },
    );
    assert_eq!(power_toughness(&state, elves), (3, 1), "two Eldrazi");
    assert!(engine::has_effective_keyword(
        &state,
        elves,
        Keywords::HASTE
    ));
}

#[test]
fn wastescape_battlemage_green_kicker_exiles_an_artifact_or_enchantment() {
    wastescape(true, false);
}

#[test]
fn wastescape_battlemage_blue_kicker_bounces_a_creature() {
    wastescape(false, true);
}

#[test]
fn wastescape_battlemage_takes_both_kickers() {
    wastescape(true, true);
}

/// Casts Wastescape Battlemage with the chosen kickers against an opposing
/// enchantment and creature.
fn wastescape(kick_green: bool, kick_blue: bool) {
    let mut state = islands();
    let beanstalk = put(&mut state, P1, "Up the Beanstalk", Zone::Battlefield);
    let angler = put(&mut state, P1, "Gurmag Angler", Zone::Battlefield);
    let mage = put(&mut state, P0, "Wastescape Battlemage", Zone::Hand);
    pool(
        &mut state,
        P0,
        &[ManaColor::C, ManaColor::G, ManaColor::U],
        2,
    );
    cast(&mut state, mage, &[]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseCastMode { .. } => Action::ChooseCastMode(if kick_blue {
            CastMode::Alternative
        } else {
            CastMode::Normal
        }),
        Decision::ChooseKicker { .. } => Action::ChooseKicker(kick_green),
        Decision::ChooseTargets { legal_targets, .. } => {
            if legal_targets.contains(&Target::Object(beanstalk)) {
                Action::ChooseTarget(Target::Object(beanstalk))
            } else {
                Action::ChooseTarget(Target::Object(angler))
            }
        }
        other => first_answer(other),
    });
    assert_eq!(zone(&state, mage), Zone::Battlefield);
    assert_eq!(
        zone(&state, beanstalk) == Zone::Exile,
        kick_green,
        "{kick_green} {kick_blue}"
    );
    assert_eq!(
        zone(&state, angler) == Zone::Hand,
        kick_blue,
        "{kick_green} {kick_blue}"
    );
}

#[test]
fn ajani_makes_a_cat_warrior_and_transforms_when_another_cat_dies() {
    let mut state = islands();
    let ajani = put(&mut state, P0, "Ajani, Nacatl Pariah", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W], 1);
    cast(&mut state, ajani, &[]);
    settled(&mut state);
    let cats = named(&state, P0, "Cat Warrior Token");
    assert_eq!(cats.len(), 1);
    assert_eq!(power_toughness(&state, cats[0]), (2, 1));
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(cats[0], Zone::Graveyard),
    );
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseEffectOption { option_count, .. } => {
            assert_eq!(option_count, 2, "may");
            Action::ChooseEffectOption(0)
        }
        other => first_answer(other),
    });
    let live = state.objects.get(ajani);
    assert_eq!(live.zone, Zone::Battlefield);
    assert_eq!(live.v4.face_index, 1);
    assert!(engine::object_has_type(
        &state,
        ajani,
        CardType::Planeswalker
    ));
    assert_eq!(mtg_kernel::planeswalker_v1::loyalty(&state, ajani), Some(3));
}

/// Ajani, Nacatl Avenger on P0's battlefield.
fn avenger(state: &mut GameState) -> ObjectId {
    let ajani = put(state, P0, "Ajani, Nacatl Pariah", Zone::Battlefield);
    event::propose_and_commit(state, ProposedEvent::zone_change(ajani, Zone::Exile));
    event::propose_and_commit(
        state,
        ProposedEvent::transformed_battlefield_return(ajani, 1, P0),
    );
    settled(state);
    assert_eq!(state.objects.get(ajani).v4.face_index, 1);
    ajani
}

#[test]
fn ajani_nacatl_avenger_plus_two_and_zero() {
    let mut state = islands();
    let ajani = avenger(&mut state);
    let ocelot = put(&mut state, P0, "Ocelot Pride", Zone::Battlefield);
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    activate(&mut state, ajani, 0, &[]);
    settled(&mut state);
    assert_eq!(state.objects.get(ocelot).counters.plus1_plus1, 1, "a Cat");
    assert_eq!(state.objects.get(elves).counters.plus1_plus1, 0);
    assert_eq!(mtg_kernel::planeswalker_v1::loyalty(&state, ajani), Some(5));

    // 0 without another red permanent: only the token.
    let mut state = islands();
    let ajani = avenger(&mut state);
    activate(&mut state, ajani, 1, &[]);
    settled(&mut state);
    assert_eq!(named(&state, P0, "Cat Warrior Token").len(), 1);
    assert_eq!(state.players[1].life, 20);

    // With a red permanent: damage equal to the number of creatures.
    let mut state = islands();
    let ajani = avenger(&mut state);
    put(&mut state, P0, "Amped Raptor", Zone::Battlefield);
    activate(&mut state, ajani, 1, &[]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseTargets { legal_targets, .. } => {
            assert!(legal_targets.contains(&Target::Player(P1)));
            Action::ChooseTarget(Target::Player(P1))
        }
        other => first_answer(other),
    });
    assert_eq!(state.players[1].life, 18, "Raptor and the new Cat Warrior");
}

#[test]
fn ajani_nacatl_avenger_minus_four_keeps_one_permanent_of_each_type() {
    let mut state = islands();
    let ajani = avenger(&mut state);
    for _ in 0..2 {
        activate(&mut state, ajani, 0, &[]);
        settled(&mut state);
        state
            .objects
            .get_mut(ajani)
            .v4
            .ability_uses_this_turn
            .clear();
    }
    let elves = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    let angler = put(&mut state, P1, "Gurmag Angler", Zone::Battlefield);
    let beanstalk = put(&mut state, P1, "Up the Beanstalk", Zone::Battlefield);
    let island = put(&mut state, P1, "Island", Zone::Battlefield);
    activate(&mut state, ajani, 2, &[]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseEffectTargets {
            player,
            legal_targets,
            ..
        } => {
            assert_eq!(player, P1);
            if legal_targets.contains(&Target::Object(angler)) {
                Action::ChooseEffectTarget(Target::Object(angler))
            } else {
                Action::ChooseEffectTarget(legal_targets[0])
            }
        }
        other => first_answer(other),
    });
    assert_eq!(zone(&state, angler), Zone::Battlefield, "kept creature");
    assert_eq!(
        zone(&state, beanstalk),
        Zone::Battlefield,
        "kept enchantment"
    );
    assert_eq!(zone(&state, elves), Zone::Graveyard);
    assert_eq!(zone(&state, island), Zone::Battlefield, "lands stay");
}

#[test]
fn amped_raptor_gets_energy_and_casts_the_exiled_card_for_energy() {
    let mut state = game(&["Island", "Lightning Bolt", "Island"]);
    let bolt = state.players[0].library[1];
    let raptor = put(&mut state, P0, "Amped Raptor", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 1);
    cast(&mut state, raptor, &[]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseEffectOption { option_count, .. } => {
            assert_eq!(option_count, 2, "cast or decline");
            Action::ChooseEffectOption(0)
        }
        Decision::ChooseEffectTargets { legal_targets, .. }
            if legal_targets == vec![Target::Object(bolt)] =>
        {
            Action::ChooseEffectTarget(Target::Object(bolt))
        }
        Decision::ChooseTargets { .. } => Action::ChooseTarget(Target::Player(P1)),
        other => first_answer(other),
    });
    assert_eq!(state.players[1].life, 17, "the Bolt was cast");
    assert_eq!(zone(&state, bolt), Zone::Graveyard);
    assert_eq!(energy(&state, P0), 1, "two, less its mana value");
    assert_eq!(state.exile.len(), 1, "the Island stays exiled");
}

#[test]
fn amped_raptor_put_onto_the_battlefield_only_gets_energy() {
    let mut state = game(&["Lightning Bolt", "Island"]);
    let bolt = state.players[0].library[0];
    let raptor = put(&mut state, P0, "Amped Raptor", Zone::Hand);
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(raptor, Zone::Battlefield),
    );
    settled(&mut state);
    assert_eq!(energy(&state, P0), 2);
    assert_eq!(zone(&state, bolt), Zone::Library, "not cast from hand");
}

#[test]
fn guide_of_souls_gains_life_and_energy_and_makes_an_angel() {
    let mut state = islands();
    let guide = put(&mut state, P0, "Guide of Souls", Zone::Battlefield);
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 0);
    cast(&mut state, elves, &[]);
    settled(&mut state);
    assert_eq!(state.players[0].life, 21);
    assert_eq!(energy(&state, P0), 1);
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(guide, Zone::Graveyard),
    );
    let guide = put(&mut state, P0, "Guide of Souls", Zone::Battlefield);
    let angler = put(&mut state, P0, "Gurmag Angler", Zone::Battlefield);
    pool(&mut state, P0, &[], 0);
    // Two more energy from a token and a creature entering.
    for name in ["Kessig Flamebreather", "Llanowar Elves"] {
        let creature = put(&mut state, P0, name, Zone::Hand);
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(creature, Zone::Battlefield),
        );
        settled(&mut state);
    }
    assert_eq!(energy(&state, P0), 3);
    attack_with(&mut state, vec![angler], |_, decision| match decision {
        Decision::ChooseEffectOption { .. } => Some(Action::ChooseEffectOption(0)),
        Decision::ChooseTargets { .. } => Some(Action::ChooseTarget(Target::Object(angler))),
        _ => None,
    });
    assert_eq!(energy(&state, P0), 0);
    assert_eq!(state.objects.get(angler).counters.plus1_plus1, 2);
    assert!(engine::has_effective_keyword(
        &state,
        angler,
        Keywords::FLYING
    ));
    assert!(engine::has_effective_subtype(
        &state,
        angler,
        Subtype::Angel
    ));
    assert_eq!(zone(&state, guide), Zone::Battlefield);
}

#[test]
fn ocelot_pride_makes_a_cat_after_life_gain_and_copies_tokens_with_the_citys_blessing() {
    let mut state = islands();
    put(&mut state, P0, "Ocelot Pride", Zone::Battlefield);
    run_until(&mut state, |state| state.step == Step::Main2, |_, _| None);
    run_until(&mut state, |state| state.active_player == P1, |_, _| None);
    assert!(named(&state, P0, "Cat Token").is_empty(), "no life gained");

    let mut state = islands();
    put(&mut state, P0, "Ocelot Pride", Zone::Battlefield);
    event::propose_and_commit(&mut state, ProposedEvent::life_gain(P0, 1));
    run_until(&mut state, |state| state.active_player == P1, |_, _| None);
    assert_eq!(named(&state, P0, "Cat Token").len(), 1);

    // With ten permanents: the city's blessing copies this turn's tokens.
    let mut state = islands();
    put(&mut state, P0, "Ocelot Pride", Zone::Battlefield);
    for _ in 0..9 {
        put(&mut state, P0, "Island", Zone::Battlefield);
    }
    event::propose_and_commit(
        &mut state,
        ProposedEvent::create_token(id("Cat Warrior Token"), P0),
    );
    event::propose_and_commit(&mut state, ProposedEvent::life_gain(P0, 1));
    run_until(&mut state, |state| state.active_player == P1, |_, _| None);
    assert_eq!(named(&state, P0, "Cat Warrior Token").len(), 2);
    assert_eq!(named(&state, P0, "Cat Token").len(), 2);
}

#[test]
fn phelia_exiles_a_permanent_that_returns_at_the_end_step() {
    // An opponent's creature returns under its owner's control.
    let mut state = islands();
    let phelia = put(
        &mut state,
        P0,
        "Phelia, Exuberant Shepherd",
        Zone::Battlefield,
    );
    let angler = put(&mut state, P1, "Gurmag Angler", Zone::Battlefield);
    attack_with(&mut state, vec![phelia], |_, decision| match decision {
        Decision::ChooseTargets { legal_targets, .. } => {
            assert!(!legal_targets.contains(&Target::Object(phelia)), "other");
            Some(Action::ChooseTarget(Target::Object(angler)))
        }
        _ => None,
    });
    assert_eq!(zone(&state, angler), Zone::Exile);
    run_until(&mut state, |state| state.active_player == P1, |_, _| None);
    assert_eq!(zone(&state, angler), Zone::Battlefield);
    assert_eq!(state.objects.get(angler).controller, P1);
    assert_eq!(state.objects.get(phelia).counters.plus1_plus1, 0);

    // One of P0's returns under P0's control: Phelia gets a counter.
    let mut state = islands();
    let phelia = put(
        &mut state,
        P0,
        "Phelia, Exuberant Shepherd",
        Zone::Battlefield,
    );
    let beanstalk = put(&mut state, P0, "Up the Beanstalk", Zone::Battlefield);
    attack_with(&mut state, vec![phelia], |_, decision| match decision {
        Decision::ChooseTargets { .. } => Some(Action::ChooseTarget(Target::Object(beanstalk))),
        _ => None,
    });
    run_until(&mut state, |state| state.active_player == P1, |_, _| None);
    assert_eq!(zone(&state, beanstalk), Zone::Battlefield);
    assert_eq!(state.objects.get(phelia).counters.plus1_plus1, 1);
}

#[test]
fn phlage_is_sacrificed_unless_it_escaped_and_deals_three_on_entry_and_attack() {
    let mut state = islands();
    let phlage = put(&mut state, P0, "Phlage, Titan of Fire's Fury", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R, ManaColor::W], 1);
    cast(&mut state, phlage, &[]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseTargets { .. } => Action::ChooseTarget(Target::Player(P1)),
        other => first_answer(other),
    });
    assert_eq!(zone(&state, phlage), Zone::Graveyard, "sacrificed");
    assert_eq!(state.players[1].life, 17);
    assert_eq!(state.players[0].life, 23);

    // Escaped: it stays, and attacking deals 3 more.
    for _ in 0..5 {
        put(&mut state, P0, "Island", Zone::Graveyard);
    }
    pool(
        &mut state,
        P0,
        &[ManaColor::R, ManaColor::R, ManaColor::W, ManaColor::W],
        0,
    );
    cast(&mut state, phlage, &[]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseTargets { .. } => Action::ChooseTarget(Target::Player(P1)),
        other => first_answer(other),
    });
    assert_eq!(zone(&state, phlage), Zone::Battlefield, "escaped");
    assert_eq!(state.players[1].life, 14);
    state.objects.get_mut(phlage).summoning_sick = false;
    attack_with(&mut state, vec![phlage], |_, decision| match decision {
        Decision::ChooseTargets { .. } => Some(Action::ChooseTarget(Target::Player(P1))),
        _ => None,
    });
    assert_eq!(state.players[1].life, 14 - 3 - 6, "trigger and combat");
}

#[test]
fn quantum_riddler_draws_one_more_while_its_controller_holds_one_card_or_none() {
    let mut state = islands();
    let riddler = put(&mut state, P0, "Quantum Riddler", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U, ManaColor::U], 3);
    cast(&mut state, riddler, &[]);
    settle_first(&mut state);
    assert_eq!(state.players[0].hand.len(), 2, "draw one, plus one");
    // Two cards in hand: no extra card.
    event::propose_and_commit(&mut state, ProposedEvent::draw(P0));
    assert_eq!(state.players[0].hand.len(), 3);
    // The opponent draws normally.
    event::propose_and_commit(&mut state, ProposedEvent::draw(P1));
    assert_eq!(state.players[1].hand.len(), 1);
}

#[test]
fn quantum_riddler_warps_and_is_exiled_at_the_end_step() {
    let mut state = islands();
    let riddler = put(&mut state, P0, "Quantum Riddler", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 1);
    cast(&mut state, riddler, &[]);
    settle_first(&mut state);
    assert_eq!(zone(&state, riddler), Zone::Battlefield, "warped");
    run_until(&mut state, |state| state.active_player == P1, |_, _| None);
    assert_eq!(zone(&state, riddler), Zone::Exile);
}

#[test]
fn sand_scout_fetches_a_desert_and_makes_one_sand_warrior_each_turn() {
    let mut state = game(&["Lazotep Quarry", "Island"]);
    let quarry = state.players[0].library[0];
    put(&mut state, P1, "Island", Zone::Battlefield);
    let scout = put(&mut state, P0, "Sand Scout", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W], 1);
    cast(&mut state, scout, &[]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseEffectTargets { legal_targets, .. } => {
            assert_eq!(legal_targets, vec![Target::Object(quarry)]);
            Action::ChooseEffectTarget(Target::Object(quarry))
        }
        other => first_answer(other),
    });
    assert_eq!(zone(&state, quarry), Zone::Battlefield);
    assert!(state.objects.get(quarry).tapped);

    for _ in 0..2 {
        let fetch = put(&mut state, P0, "Arid Mesa", Zone::Battlefield);
        activate(&mut state, fetch, 0, &[]);
        settle_first(&mut state);
    }
    assert_eq!(
        named(&state, P0, "Sand Warrior Token").len(),
        1,
        "only once each turn"
    );
}

#[test]
fn voice_of_victory_mobilizes_and_stops_opponents_spells_on_its_controllers_turn() {
    let mut state = islands();
    let voice = put(&mut state, P0, "Voice of Victory", Zone::Battlefield);
    let bolt = put(&mut state, P1, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P1, &[ManaColor::R], 0);
    state.priority_player = P1;
    assert!(!castable(&mut state, bolt), "P0's turn");
    state.priority_player = P0;
    attack_with(&mut state, vec![voice], |_, _| None);
    let warriors = named(&state, P0, "Warrior Token");
    assert_eq!(warriors.len(), 2);
    assert_eq!(state.players[1].life, 17, "1 + 1 + 1");
    run_until(&mut state, |state| state.active_player == P1, |_, _| None);
    assert!(named(&state, P0, "Warrior Token").is_empty(), "sacrificed");
}

// ---- Eldrazi Confluence ---------------------------------------------------------------

#[test]
fn eldrazi_confluence_resolves_three_chosen_modes() {
    let mut state = islands();
    let elves = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    let beanstalk = put(&mut state, P1, "Up the Beanstalk", Zone::Battlefield);
    let angler = put(&mut state, P0, "Gurmag Angler", Zone::Battlefield);
    let confluence = put(&mut state, P0, "Eldrazi Confluence", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::C, ManaColor::C], 2);
    act(&mut state, Action::CastSpell(confluence));
    let mut targets = vec![Target::Object(angler), Target::Object(beanstalk)];
    settle_with(&mut state, |_, decision| match decision {
        // [+3/-3, flicker, Scion]
        Decision::ChooseSpellMode { legal_modes, .. } => {
            assert!(legal_modes.contains(&4));
            Action::ChooseSpellMode(4)
        }
        Decision::ChooseTargets { .. } => Action::ChooseTarget(targets.remove(0)),
        other => first_answer(other),
    });
    assert_eq!(power_toughness(&state, angler), (8, 2));
    assert_eq!(zone(&state, beanstalk), Zone::Battlefield);
    assert!(state.objects.get(beanstalk).tapped, "returned tapped");
    assert_eq!(state.objects.get(beanstalk).controller, P1);
    assert_eq!(named(&state, P0, "Eldrazi Scion Token").len(), 1);
    assert_eq!(zone(&state, elves), Zone::Battlefield);
}

// ---- Regressions found by the catalog games -------------------------------------------

#[test]
fn force_of_will_pitched_against_brain_freeze_counters_it() {
    let mut state = islands();
    state.priority_player = P1;
    let freeze = put(&mut state, P1, "Brain Freeze", Zone::Hand);
    pool(&mut state, P1, &[ManaColor::U], 1);
    cast(&mut state, freeze, &[Target::Player(P0)]);
    let force = put(&mut state, P0, "Force of Will", Zone::Hand);
    let pitch = put(&mut state, P0, "Stifle", Zone::Hand);
    run_until(&mut state, |state| state.priority_player == P0, |_, _| None);
    act(&mut state, Action::CastSpell(force));
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseCastMode { .. } => Action::ChooseCastMode(CastMode::Alternative),
        Decision::ChooseTargets { legal_targets, .. } => {
            assert!(legal_targets.contains(&Target::Object(freeze)));
            Action::ChooseTarget(Target::Object(freeze))
        }
        Decision::ChooseCostTargets { candidates, .. } => {
            assert!(candidates.contains(&pitch));
            Action::ChooseCostTarget(pitch)
        }
        other => first_answer(other),
    });
    assert_eq!(zone(&state, freeze), Zone::Graveyard);
    assert_eq!(zone(&state, pitch), Zone::Exile);
    assert_eq!(state.players[0].library.len(), 20, "nothing milled");
}

#[test]
fn vexing_bauble_counters_a_pitched_force_of_will() {
    let mut state = islands();
    put(&mut state, P1, "Vexing Bauble", Zone::Battlefield);
    let bolt = opponent_bolts(&mut state, Target::Player(P0));
    let force = put(&mut state, P0, "Force of Will", Zone::Hand);
    let pitch = put(&mut state, P0, "Stifle", Zone::Hand);
    run_until(&mut state, |state| state.priority_player == P0, |_, _| None);
    act(&mut state, Action::CastSpell(force));
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseCastMode { .. } => Action::ChooseCastMode(CastMode::Alternative),
        Decision::ChooseTargets { legal_targets, .. } => {
            if legal_targets.contains(&Target::Object(bolt)) {
                Action::ChooseTarget(Target::Object(bolt))
            } else {
                Action::ChooseTarget(legal_targets[0])
            }
        }
        Decision::ChooseCostTargets { candidates, .. } => {
            assert!(candidates.contains(&pitch));
            Action::ChooseCostTarget(pitch)
        }
        other => first_answer(other),
    });
    assert_eq!(zone(&state, force), Zone::Graveyard);
    assert_eq!(state.players[0].life, 20 - 1 - 3, "the Bolt resolved");
}

#[test]
fn counterspell_counters_brain_freeze() {
    let mut state = islands();
    state.priority_player = P1;
    let freeze = put(&mut state, P1, "Brain Freeze", Zone::Hand);
    pool(&mut state, P1, &[ManaColor::U], 1);
    cast(&mut state, freeze, &[Target::Player(P0)]);
    let counterspell = put(&mut state, P0, "Counterspell", Zone::Hand);
    run_until(&mut state, |state| state.priority_player == P0, |_, _| None);
    pool(&mut state, P0, &[ManaColor::U, ManaColor::U], 0);
    cast(&mut state, counterspell, &[Target::Object(freeze)]);
    settle_first(&mut state);
    assert_eq!(zone(&state, freeze), Zone::Graveyard);
    assert_eq!(state.players[0].library.len(), 20, "nothing milled");
}
