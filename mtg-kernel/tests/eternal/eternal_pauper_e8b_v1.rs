//! Eternal wave E8b: native rules tests for the cards that complete Pauper
//! Ruby Storm, Golgari Gardens, Familiars, Jeskai Ephemerate, Glintblade,
//! Izzet Terror, Burn and Boros Bully.
#![cfg(feature = "eternal-fixtures")]

use mtg_kernel::card_def::{
    card_id_by_name, AltCostCondition, CardCapability, CardDef, CardType, Keywords, Subtype,
    Supertype, TargetSpec, CARD_DEFS,
};
use mtg_kernel::engine::{self, Action, CastMode, Decision, OptionalCostChoice};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::ManaColor;
use mtg_kernel::state::{GameObject, GameState, ObjectLinkV4, ObjectStateV4, Step, Target, Zone};

const P0: PlayerId = PlayerId::P0;
const P1: PlayerId = PlayerId::P1;

const E8B_CARDS: [&str; 46] = [
    "Accursed Marauder",
    "Azorius Chancery",
    "Contaminated Landscape",
    "Crimson Fleet Commodore",
    "Crossover Collaboration",
    "Crypt Rats",
    "Defile",
    "Desert",
    "Desperate Ritual",
    "First Day of Class",
    "Geothermal Crevice",
    "Glimpse the Impossible",
    "Glorious Gale",
    "God-Pharaoh's Faithful",
    "Goblin Anarchomancer",
    "Goldmire Bridge",
    "Goliath Paladin",
    "Golgari Rot Farm",
    "Haunted Mire",
    "Heaped Harvest",
    "Hickory Woodlot",
    "Khalni Garden",
    "Lonely Arroyo",
    "Manamorphose",
    "Meeting of Minds",
    "Mirran Banesplitter",
    "Mortuary Mire",
    "Oroku Saki, Shredder Rising",
    "Orzhov Basilica",
    "Perilous Landscape",
    "Rally the Peasants",
    "Rancid Earth",
    "Reckless Lackey",
    "Ride's End",
    "Sandstone Needle",
    "Sea Gate Oracle",
    "Seething Song",
    "Seize the Storm",
    "Skred",
    "Snow-Covered Mountain",
    "Sunscape Familiar",
    "Thornscape Familiar",
    "Twinned Vision",
    "Volatile Fjord",
    "Witch's Cottage",
    "Wrenn's Resolve",
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
        0x4538_4221_0001,
    );
    state.active_player = P0;
    state.priority_player = P0;
    state.step = Step::Main1;
    state
}

fn plains() -> GameState {
    game(&["Plains"; 20])
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

fn keyword(state: &GameState, object: ObjectId, keyword: Keywords) -> bool {
    engine::has_effective_keyword(state, object, keyword)
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
/// finishing a selection that may finish, declining a Boolean or cost.
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
        Decision::ChooseTriggerMode { legal_modes, .. } => {
            Action::ChooseTriggerMode(legal_modes[0])
        }
        Decision::ChooseOptionalCost { .. } => {
            Action::ChooseOptionalCost(OptionalCostChoice::Decline)
        }
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

/// Settles, choosing `object` whenever a selection or target offers it.
fn settle_choosing(state: &mut GameState, object: ObjectId) {
    settle_with(state, |_, decision| match decision {
        Decision::ChooseTargets { legal_targets, .. }
            if legal_targets.contains(&Target::Object(object)) =>
        {
            Action::ChooseTarget(Target::Object(object))
        }
        Decision::ChooseEffectTargets { legal_targets, .. }
            if legal_targets.contains(&Target::Object(object)) =>
        {
            Action::ChooseEffectTarget(Target::Object(object))
        }
        other => first_answer(other),
    });
}

/// Settles, answering effect options with `option` and otherwise with the
/// first legal answer.
fn settle_option(state: &mut GameState, option: u16) {
    settle_with(state, |_, decision| match decision {
        Decision::ChooseEffectOption { .. } => Action::ChooseEffectOption(option),
        other => first_answer(other),
    });
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

/// P0 attacks with `attacker`; the creatures P1 may block it with.
fn legal_blockers(state: &mut GameState, attacker: ObjectId) -> Vec<ObjectId> {
    pass_until(state, |state| state.step == Step::DeclareAttackers);
    act(state, Action::DeclareAttackers(vec![attacker]));
    for _ in 0..50 {
        match next(state) {
            Decision::DeclareBlockers { legal_blockers, .. } => {
                return legal_blockers
                    .into_iter()
                    .find(|(candidate, _)| *candidate == attacker)
                    .map(|(_, blockers)| blockers)
                    .unwrap_or_default();
            }
            Decision::CastSpellOrPass { .. } => act(state, Action::Pass),
            Decision::OrderTriggers { pending, .. } => {
                act(state, Action::OrderTriggers((0..pending.len()).collect()))
            }
            other => panic!("expected blockers, got {other:?}"),
        }
    }
    panic!("no declare blockers decision");
}

fn hand_contains(state: &GameState, player: PlayerId, object: ObjectId) -> bool {
    state.players[player.index()].hand.contains(&object)
}

fn mana(state: &GameState, player: PlayerId, color: ManaColor) -> u8 {
    state.players[player.index()].mana_pool[color.pool_index()]
}

/// Plays `land` from P0's hand and settles its triggers with first answers.
fn play_land(state: &mut GameState, land: ObjectId) {
    next(state);
    act(state, Action::PlayLand(land));
    settle_first(state);
}

/// The battlefield incarnation of `object` as a link.
fn link(state: &GameState, object: ObjectId) -> ObjectLinkV4 {
    ObjectLinkV4 {
        object,
        zone_change_count: state.objects.get(object).zone_change_count,
    }
}

/// Makes `bearer` P0's Ring-bearer with the Ring's first `level` abilities.
fn ring(state: &mut GameState, bearer: ObjectId, level: u8) {
    let bearer = link(state, bearer);
    let e8b = &mut state.eternal_v1.get_or_insert_with(Default::default).e8b;
    e8b.ring_levels[0] = level;
    e8b.ring_bearers[0] = Some(bearer);
}

// ---- Characteristics -----------------------------------------------------------

#[test]
fn e8b_cards_are_full_with_their_printed_characteristics() {
    for name in E8B_CARDS {
        assert_eq!(def(name).capability, CardCapability::Full, "{name}");
        assert!(!def(name).is_token, "{name}");
    }
    let plant = def("Plant Token");
    assert!(plant.is_token);
    assert_eq!(plant.capability, CardCapability::Full);
    assert_eq!(plant.subtypes, &[Subtype::Plant]);
    assert_eq!(plant.colors, &[ManaColor::G]);
    assert_eq!((plant.power, plant.toughness), (Some(0), Some(1)));
    let elemental = def("Red Elemental Token");
    assert!(elemental.is_token);
    assert_eq!(elemental.capability, CardCapability::Full);
    assert_eq!(elemental.subtypes, &[Subtype::Elemental]);
    assert!(elemental.keywords.has(Keywords::TRAMPLE));

    assert!(Subtype::Ogre.is_creature_type());
    assert_eq!(
        def("Crimson Fleet Commodore").subtypes,
        &[Subtype::Ogre, Subtype::Pirate]
    );
    assert!(def("Crimson Fleet Commodore")
        .keywords
        .has(Keywords::TRAMPLE));
    assert!(def("Goliath Paladin").keywords.has(Keywords::VIGILANCE));
    assert!(def("Mirran Banesplitter").keywords.has(Keywords::FLASH));
    assert!(def("Mirran Banesplitter").equipment.is_some());
    let lackey = def("Reckless Lackey");
    assert!(lackey.keywords.has(Keywords::FIRST_STRIKE) && lackey.keywords.has(Keywords::HASTE));
    assert!(def("Sunscape Familiar").keywords.has(Keywords::DEFENDER));
    assert_eq!(def("Desperate Ritual").subtypes, &[Subtype::Arcane]);
    assert_eq!(
        def("Oroku Saki, Shredder Rising").supertypes,
        &[Supertype::Legendary]
    );
    for (name, condition) in [
        ("Crossover Collaboration", AltCostCondition::TeamworkV1(2)),
        ("Oroku Saki, Shredder Rising", AltCostCondition::SneakV1),
        ("Meeting of Minds", AltCostCondition::Always),
    ] {
        assert_eq!(
            def(name).alt_cost.map(|alt| alt.condition),
            Some(condition),
            "{name}"
        );
    }
    for name in ["Rally the Peasants", "Seize the Storm", "Twinned Vision"] {
        assert!(def(name).flashback.is_some(), "{name}");
    }
    assert!(def("Twinned Vision").additional_cost.is_some());
    assert!(def("Ride's End").generic_cost_reduction.is_some());
    assert_eq!(
        def("Ride's End").target_spec,
        TargetSpec::StandardV1(
            mtg_kernel::standard_cards_v1::StandardTargetV1::CreatureOrVehicleV1
        )
    );
    assert_eq!(def("Crypt Rats").activated_abilities.len(), 10);
    for name in [
        "Azorius Chancery",
        "Golgari Rot Farm",
        "Orzhov Basilica",
        "Geothermal Crevice",
        "Goldmire Bridge",
        "Haunted Mire",
        "Hickory Woodlot",
        "Sandstone Needle",
        "Khalni Garden",
        "Lonely Arroyo",
        "Mortuary Mire",
        "Volatile Fjord",
    ] {
        assert!(def(name).enters_battlefield_tapped, "{name}");
    }
    assert!(!def("Witch's Cottage").enters_battlefield_tapped);
    assert!(def("Witch's Cottage")
        .enters_battlefield_tapped_unless
        .is_some());
    assert!(def("Goldmire Bridge")
        .keywords
        .has(Keywords::INDESTRUCTIBLE));
    assert!(def("Goldmire Bridge").has_type(CardType::Artifact));
    assert_eq!(
        def("Snow-Covered Mountain").supertypes,
        &[Supertype::Basic, Supertype::Snow]
    );
    assert_eq!(def("Volatile Fjord").supertypes, &[Supertype::Snow]);
    assert_eq!(
        def("Volatile Fjord").subtypes,
        &[Subtype::Island, Subtype::Mountain]
    );
    assert_eq!(
        def("Haunted Mire").subtypes,
        &[Subtype::Swamp, Subtype::Forest]
    );
    assert_eq!(def("Lonely Arroyo").subtypes, &[Subtype::Desert]);
    assert_eq!(def("Desert").subtypes, &[Subtype::Desert]);
    assert_eq!(def("Heaped Harvest").subtypes, &[Subtype::Food]);
}

// ---- Lands ---------------------------------------------------------------------

#[test]
fn karoos_return_a_land_and_tap_for_both_colors() {
    for (name, colors) in [
        ("Azorius Chancery", [ManaColor::W, ManaColor::U]),
        ("Golgari Rot Farm", [ManaColor::B, ManaColor::G]),
        ("Orzhov Basilica", [ManaColor::W, ManaColor::B]),
    ] {
        let mut state = plains();
        let plain = put(&mut state, P0, "Plains", Zone::Battlefield);
        let karoo = put(&mut state, P0, name, Zone::Hand);
        next(&mut state);
        act(&mut state, Action::PlayLand(karoo));
        settle_choosing(&mut state, plain);
        assert!(state.objects.get(karoo).tapped, "{name}");
        assert!(hand_contains(&state, P0, plain), "{name}");

        let mut state = plains();
        let karoo = put(&mut state, P0, name, Zone::Battlefield);
        next(&mut state);
        act(&mut state, Action::ActivateManaAbility(karoo));
        for color in colors {
            assert_eq!(mana(&state, P0, color), 1, "{name} {color:?}");
        }
    }
}

#[test]
fn contaminated_and_perilous_landscape_fetch_basics_and_cycle() {
    for (name, basic, cycling) in [
        (
            "Contaminated Landscape",
            "Swamp",
            [ManaColor::W, ManaColor::U, ManaColor::B],
        ),
        (
            "Perilous Landscape",
            "Mountain",
            [ManaColor::U, ManaColor::R, ManaColor::W],
        ),
    ] {
        let mut state = plains();
        let landscape = put(&mut state, P0, name, Zone::Battlefield);
        next(&mut state);
        act(&mut state, Action::ActivateManaAbility(landscape));
        assert_eq!(mana(&state, P0, ManaColor::C), 1, "{name}");

        let mut state = plains();
        let landscape = put(&mut state, P0, name, Zone::Battlefield);
        let land = put(&mut state, P0, basic, Zone::Library);
        activate(&mut state, landscape, 0, &[]);
        settle_choosing(&mut state, land);
        assert_eq!(zone(&state, landscape), Zone::Graveyard, "{name}");
        assert_eq!(zone(&state, land), Zone::Battlefield, "{name}");
        assert!(state.objects.get(land).tapped, "{name}");

        let mut state = plains();
        let landscape = put(&mut state, P0, name, Zone::Hand);
        pool(&mut state, P0, &cycling, 0);
        let hand = state.players[0].hand.len();
        activate(&mut state, landscape, 1, &[]);
        settled(&mut state);
        assert_eq!(zone(&state, landscape), Zone::Graveyard, "{name}");
        assert_eq!(
            state.players[0].hand.len(),
            hand,
            "{name} cycled for a card"
        );
    }
}

#[test]
fn desert_pings_an_attacking_creature_only_during_the_end_of_combat_step() {
    let mut state = plains();
    let desert = put(&mut state, P0, "Desert", Zone::Battlefield);
    let attacker = put(&mut state, P1, "Thraben Inspector", Zone::Battlefield);
    let mut checked = false;
    run_until(
        &mut state,
        |state| {
            state.active_player == P1
                && state.step == Step::EndCombat
                && state.priority_player == P0
        },
        |state, decision| match decision {
            Decision::DeclareAttackers { player: P1, .. } => {
                Some(Action::DeclareAttackers(vec![attacker]))
            }
            Decision::CastSpellOrPass {
                player: P0,
                activatable_abilities,
                ..
            } if state.active_player == P1 && state.step == Step::DeclareBlockers => {
                assert!(!activatable_abilities.contains(&(desert, 0)), "too early");
                checked = true;
                None
            }
            _ => None,
        },
    );
    assert!(checked);
    activate(&mut state, desert, 0, &[Target::Object(attacker)]);
    settled(&mut state);
    assert_eq!(state.objects.get(attacker).damage, 1);
    assert!(state.objects.get(desert).tapped);

    // Its mana ability adds {C}.
    let mut state = plains();
    let desert = put(&mut state, P0, "Desert", Zone::Battlefield);
    next(&mut state);
    act(&mut state, Action::ActivateManaAbility(desert));
    assert_eq!(mana(&state, P0, ManaColor::C), 1);
}

#[test]
fn geothermal_crevice_taps_for_red_or_sacrifices_for_black_and_green() {
    let mut state = plains();
    let crevice = put(&mut state, P0, "Geothermal Crevice", Zone::Hand);
    play_land(&mut state, crevice);
    assert!(state.objects.get(crevice).tapped);

    let mut state = plains();
    let crevice = put(&mut state, P0, "Geothermal Crevice", Zone::Battlefield);
    next(&mut state);
    act(
        &mut state,
        Action::ActivateManaAbilityChoice(crevice, ManaColor::R),
    );
    assert_eq!(mana(&state, P0, ManaColor::R), 1);
    assert_eq!(zone(&state, crevice), Zone::Battlefield);

    let mut state = plains();
    let crevice = put(&mut state, P0, "Geothermal Crevice", Zone::Battlefield);
    next(&mut state);
    act(
        &mut state,
        Action::ActivateManaAbilityChoice(crevice, ManaColor::B),
    );
    assert_eq!(mana(&state, P0, ManaColor::B), 1);
    assert_eq!(mana(&state, P0, ManaColor::G), 1);
    assert_eq!(zone(&state, crevice), Zone::Graveyard);
}

#[test]
fn goldmire_bridge_enters_tapped_and_taps_for_white_or_black() {
    let mut state = plains();
    let bridge = put(&mut state, P0, "Goldmire Bridge", Zone::Hand);
    play_land(&mut state, bridge);
    assert!(state.objects.get(bridge).tapped);
    assert!(keyword(&state, bridge, Keywords::INDESTRUCTIBLE));
    for color in [ManaColor::W, ManaColor::B] {
        let mut state = plains();
        let bridge = put(&mut state, P0, "Goldmire Bridge", Zone::Battlefield);
        next(&mut state);
        act(&mut state, Action::ActivateManaAbilityChoice(bridge, color));
        assert_eq!(mana(&state, P0, color), 1, "{color:?}");
    }
}

/// A tapped dual: enters tapped and taps for either color.
fn tapped_dual(name: &str, colors: [ManaColor; 2]) {
    let mut state = plains();
    let land = put(&mut state, P0, name, Zone::Hand);
    play_land(&mut state, land);
    assert!(state.objects.get(land).tapped, "{name}");
    for color in colors {
        let mut state = plains();
        let land = put(&mut state, P0, name, Zone::Battlefield);
        next(&mut state);
        act(&mut state, Action::ActivateManaAbilityChoice(land, color));
        assert_eq!(mana(&state, P0, color), 1, "{name} {color:?}");
    }
}

#[test]
fn haunted_mire_is_a_tapped_swamp_forest() {
    tapped_dual("Haunted Mire", [ManaColor::B, ManaColor::G]);
    let mut state = plains();
    let mire = put(&mut state, P0, "Haunted Mire", Zone::Battlefield);
    assert!(engine::has_effective_subtype(&state, mire, Subtype::Swamp));
    assert!(engine::has_effective_subtype(&state, mire, Subtype::Forest));
}

#[test]
fn volatile_fjord_is_a_tapped_snow_island_mountain() {
    tapped_dual("Volatile Fjord", [ManaColor::U, ManaColor::R]);
    let mut state = plains();
    let fjord = put(&mut state, P0, "Volatile Fjord", Zone::Battlefield);
    assert!(engine::effective_supertypes(&state, fjord).contains(&Supertype::Snow));
    assert!(engine::has_effective_subtype(
        &state,
        fjord,
        Subtype::Island
    ));
}

#[test]
fn snow_covered_mountain_is_a_basic_snow_mountain() {
    let mut state = plains();
    let mountain = put(&mut state, P0, "Snow-Covered Mountain", Zone::Hand);
    play_land(&mut state, mountain);
    assert!(!state.objects.get(mountain).tapped);
    assert!(engine::has_effective_subtype(
        &state,
        mountain,
        Subtype::Mountain
    ));
    next(&mut state);
    act(&mut state, Action::ActivateManaAbility(mountain));
    assert_eq!(mana(&state, P0, ManaColor::R), 1);
}

#[test]
fn depletion_lands_add_two_mana_twice_then_are_sacrificed() {
    for (name, color) in [
        ("Hickory Woodlot", ManaColor::G),
        ("Sandstone Needle", ManaColor::R),
    ] {
        let mut state = plains();
        let land = put(&mut state, P0, name, Zone::Hand);
        play_land(&mut state, land);
        assert!(state.objects.get(land).tapped, "{name}");
        assert_eq!(
            mtg_kernel::eternal_pauper_e8b_v1::depletion_counters(&state, land),
            2
        );
        state.objects.get_mut(land).tapped = false;
        next(&mut state);
        act(&mut state, Action::ActivateManaAbility(land));
        assert_eq!(mana(&state, P0, color), 2, "{name}");
        assert_eq!(zone(&state, land), Zone::Battlefield, "{name}");
        assert_eq!(
            mtg_kernel::eternal_pauper_e8b_v1::depletion_counters(&state, land),
            1
        );
        state.objects.get_mut(land).tapped = false;
        next(&mut state);
        act(&mut state, Action::ActivateManaAbility(land));
        assert_eq!(mana(&state, P0, color), 4, "{name}");
        assert_eq!(zone(&state, land), Zone::Graveyard, "{name} sacrificed");

        // Without a depletion counter the ability can't be activated.
        let mut state = plains();
        let land = put(&mut state, P0, name, Zone::Battlefield);
        next(&mut state);
        assert!(engine::step(&mut state, Action::ActivateManaAbility(land)).is_err());
    }
}

#[test]
fn khalni_garden_makes_a_plant() {
    let mut state = plains();
    let garden = put(&mut state, P0, "Khalni Garden", Zone::Hand);
    play_land(&mut state, garden);
    assert!(state.objects.get(garden).tapped);
    let plants = named(&state, P0, "Plant Token");
    assert_eq!(plants.len(), 1);
    assert_eq!(power_toughness(&state, plants[0]), (0, 1));
    state.objects.get_mut(garden).tapped = false;
    next(&mut state);
    act(&mut state, Action::ActivateManaAbility(garden));
    assert_eq!(mana(&state, P0, ManaColor::G), 1);
}

#[test]
fn lonely_arroyo_pings_target_opponent() {
    let mut state = plains();
    let arroyo = put(&mut state, P0, "Lonely Arroyo", Zone::Hand);
    play_land(&mut state, arroyo);
    assert!(state.objects.get(arroyo).tapped);
    assert_eq!(state.players[1].life, 19);
    for color in [ManaColor::W, ManaColor::U] {
        let mut state = plains();
        let arroyo = put(&mut state, P0, "Lonely Arroyo", Zone::Battlefield);
        next(&mut state);
        act(&mut state, Action::ActivateManaAbilityChoice(arroyo, color));
        assert_eq!(mana(&state, P0, color), 1, "{color:?}");
    }
}

#[test]
fn mortuary_mire_may_put_a_creature_card_on_top_of_the_library() {
    let mut state = plains();
    let inspector = put(&mut state, P0, "Thraben Inspector", Zone::Graveyard);
    let mire = put(&mut state, P0, "Mortuary Mire", Zone::Hand);
    next(&mut state);
    act(&mut state, Action::PlayLand(mire));
    settle_choosing(&mut state, inspector);
    assert!(state.objects.get(mire).tapped);
    assert_eq!(state.players[0].library[0], inspector, "put on top");

    // Declining leaves it in the graveyard.
    let mut state = plains();
    let inspector = put(&mut state, P0, "Thraben Inspector", Zone::Graveyard);
    let mire = put(&mut state, P0, "Mortuary Mire", Zone::Hand);
    next(&mut state);
    act(&mut state, Action::PlayLand(mire));
    settle_option(&mut state, 1);
    assert_eq!(zone(&state, inspector), Zone::Graveyard);
}

#[test]
fn witchs_cottage_enters_untapped_with_three_other_swamps_and_returns_a_creature() {
    let mut state = plains();
    let inspector = put(&mut state, P0, "Thraben Inspector", Zone::Graveyard);
    let cottage = put(&mut state, P0, "Witch's Cottage", Zone::Hand);
    play_land(&mut state, cottage);
    assert!(state.objects.get(cottage).tapped, "too few Swamps");
    assert_eq!(zone(&state, inspector), Zone::Graveyard, "no trigger");

    let mut state = plains();
    for _ in 0..3 {
        put(&mut state, P0, "Swamp", Zone::Battlefield);
    }
    let inspector = put(&mut state, P0, "Thraben Inspector", Zone::Graveyard);
    let cottage = put(&mut state, P0, "Witch's Cottage", Zone::Hand);
    next(&mut state);
    act(&mut state, Action::PlayLand(cottage));
    settle_choosing(&mut state, inspector);
    assert!(!state.objects.get(cottage).tapped);
    assert_eq!(state.players[0].library[0], inspector);
    next(&mut state);
    act(&mut state, Action::ActivateManaAbility(cottage));
    assert_eq!(mana(&state, P0, ManaColor::B), 1);
}

// ---- Creatures ------------------------------------------------------------------

#[test]
fn accursed_marauder_makes_each_player_sacrifice_a_nontoken_creature() {
    let mut state = plains();
    let mine = put(&mut state, P0, "Thraben Inspector", Zone::Battlefield);
    let token = put(&mut state, P0, "Bird Token", Zone::Battlefield);
    state.objects.get_mut(token).v4.is_token = true;
    let theirs = put(&mut state, P1, "Thraben Inspector", Zone::Battlefield);
    let marauder = put(&mut state, P0, "Accursed Marauder", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 1);
    cast(&mut state, marauder, &[]);
    settle_choosing(&mut state, mine);
    assert_eq!(zone(&state, mine), Zone::Graveyard);
    assert_eq!(zone(&state, theirs), Zone::Graveyard);
    assert_eq!(zone(&state, marauder), Zone::Battlefield);
    assert_eq!(zone(&state, token), Zone::Battlefield, "a token is spared");
}

#[test]
fn crimson_fleet_commodore_makes_its_controller_the_monarch() {
    let mut state = plains();
    state.monarch = Some(P1);
    let commodore = put(&mut state, P0, "Crimson Fleet Commodore", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 3);
    cast(&mut state, commodore, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, commodore), Zone::Battlefield);
    assert_eq!(state.monarch, Some(P0));
    assert!(keyword(&state, commodore, Keywords::TRAMPLE));
    assert_eq!(power_toughness(&state, commodore), (5, 2));
}

#[test]
fn crypt_rats_deals_x_to_each_creature_and_player_paid_with_black_mana_only() {
    let mut state = plains();
    let rats = put(&mut state, P0, "Crypt Rats", Zone::Battlefield);
    let mine = put(&mut state, P0, "Thraben Inspector", Zone::Battlefield);
    let theirs = put(&mut state, P1, "Thraben Inspector", Zone::Battlefield);
    pool(&mut state, P0, &[ManaColor::R, ManaColor::R], 0);
    assert!(!activatable(&mut state, rats, 0), "only black mana pays X");
    pool(&mut state, P0, &[ManaColor::B, ManaColor::B], 0);
    assert!(activatable(&mut state, rats, 1));
    assert!(!activatable(&mut state, rats, 2), "X = 3 needs three black");
    activate(&mut state, rats, 1, &[]);
    settled(&mut state);
    assert_eq!(state.players[0].life, 18);
    assert_eq!(state.players[1].life, 18);
    for creature in [rats, mine, theirs] {
        assert_eq!(zone(&state, creature), Zone::Graveyard);
    }
}

#[test]
fn god_pharaohs_faithful_gains_life_for_blue_black_or_red_spells() {
    let mut state = plains();
    put(&mut state, P0, "God-Pharaoh's Faithful", Zone::Battlefield);
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 0);
    cast(&mut state, bolt, &[Target::Player(P1)]);
    settled(&mut state);
    assert_eq!(state.players[0].life, 21);
    assert_eq!(state.players[1].life, 17);
    let inspector = put(&mut state, P0, "Thraben Inspector", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W], 0);
    cast(&mut state, inspector, &[]);
    settled(&mut state);
    assert_eq!(state.players[0].life, 21, "a white spell gains nothing");
}

#[test]
fn goblin_anarchomancer_makes_red_and_green_spells_cost_one_less() {
    let mut state = plains();
    let song = put(&mut state, P0, "Seething Song", Zone::Hand);
    let familiar = put(&mut state, P0, "Thornscape Familiar", Zone::Hand);
    let rally = put(&mut state, P0, "Rally the Peasants", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 1);
    assert!(!castable(&mut state, song));
    put(&mut state, P0, "Goblin Anarchomancer", Zone::Battlefield);
    assert!(castable(&mut state, song), "{{2}}{{R}} costs {{1}}{{R}}");
    let mut state_green = state.clone();
    state_green.players[0].mana_pool = [0; 6];
    pool(&mut state_green, P0, &[ManaColor::G], 0);
    assert!(
        castable(&mut state_green, familiar),
        "{{1}}{{G}} costs {{G}}"
    );
    state_green.players[0].mana_pool = [0; 6];
    pool(&mut state_green, P0, &[ManaColor::W], 1);
    assert!(!castable(&mut state_green, rally), "white is not reduced");
    cast(&mut state, song, &[]);
    settled(&mut state);
    assert_eq!(mana(&state, P0, ManaColor::R), 5);
}

#[test]
fn sunscape_familiar_makes_green_and_blue_spells_cost_one_less() {
    let mut state = plains();
    let familiar = put(&mut state, P0, "Sunscape Familiar", Zone::Battlefield);
    assert!(keyword(&state, familiar, Keywords::DEFENDER));
    let oracle = put(&mut state, P0, "Sea Gate Oracle", Zone::Hand);
    let song = put(&mut state, P0, "Seething Song", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 1);
    assert!(castable(&mut state, oracle), "{{2}}{{U}} costs {{1}}{{U}}");
    state.players[0].mana_pool = [0; 6];
    pool(&mut state, P0, &[ManaColor::R], 1);
    assert!(!castable(&mut state, song), "red is not reduced");
}

#[test]
fn thornscape_familiar_makes_red_and_white_spells_cost_one_less() {
    let mut state = plains();
    put(&mut state, P0, "Thornscape Familiar", Zone::Battlefield);
    let song = put(&mut state, P0, "Seething Song", Zone::Hand);
    let oracle = put(&mut state, P0, "Sea Gate Oracle", Zone::Hand);
    let rally = put(&mut state, P0, "Rally the Peasants", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 1);
    assert!(castable(&mut state, song));
    state.players[0].mana_pool = [0; 6];
    pool(&mut state, P0, &[ManaColor::W], 1);
    assert!(castable(&mut state, rally));
    state.players[0].mana_pool = [0; 6];
    pool(&mut state, P0, &[ManaColor::U], 1);
    assert!(!castable(&mut state, oracle), "blue is not reduced");
}

#[test]
fn goliath_paladin_takes_the_initiative() {
    let mut state = plains();
    let paladin = put(&mut state, P0, "Goliath Paladin", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W], 4);
    cast(&mut state, paladin, &[]);
    settle_first(&mut state);
    assert_eq!(zone(&state, paladin), Zone::Battlefield);
    assert_eq!(state.initiative, Some(P0));
    assert!(keyword(&state, paladin, Keywords::VIGILANCE));
}

#[test]
fn oroku_saki_draws_and_loses_life_when_it_deals_combat_damage() {
    let mut state = plains();
    let saki = put(
        &mut state,
        P0,
        "Oroku Saki, Shredder Rising",
        Zone::Battlefield,
    );
    let hand = state.players[0].hand.len();
    attack_with(&mut state, vec![saki], |_, _| None);
    assert_eq!(state.players[1].life, 17);
    assert_eq!(state.players[0].life, 19);
    assert_eq!(state.players[0].hand.len(), hand + 1);
}

#[test]
fn oroku_saki_is_sneaked_in_during_the_declare_blockers_step() {
    let mut state = plains();
    let inspector = put(&mut state, P0, "Thraben Inspector", Zone::Battlefield);
    let saki = put(&mut state, P0, "Oroku Saki, Shredder Rising", Zone::Hand);
    // The sneak cost {1}{B} is paid during the declare blockers step.
    put(&mut state, P0, "Swamp", Zone::Battlefield);
    put(&mut state, P0, "Plains", Zone::Battlefield);
    let mut sneaked = false;
    attack_with(
        &mut state,
        vec![inspector],
        |state, decision| match decision {
            Decision::CastSpellOrPass {
                player: P0,
                castable_spells,
                ..
            } if state.step == Step::DeclareBlockers && !sneaked => {
                assert!(castable_spells.contains(&saki));
                sneaked = true;
                Some(Action::CastSpell(saki))
            }
            Decision::ChooseCastMode { options, .. } => Some(Action::ChooseCastMode(
                if options.contains(&CastMode::Alternative) {
                    CastMode::Alternative
                } else {
                    options[0]
                },
            )),
            Decision::ChooseCostTargets { candidates, .. } => {
                assert!(candidates.contains(&inspector));
                Some(Action::ChooseCostTarget(inspector))
            }
            _ => None,
        },
    );
    assert!(sneaked);
    assert!(hand_contains(&state, P0, inspector), "returned to hand");
    assert_eq!(zone(&state, saki), Zone::Battlefield);
    assert!(state.objects.get(saki).tapped);
    // Oroku Saki dealt 3 combat damage, then drew and lost 1 life.
    assert_eq!(state.players[1].life, 17);
    assert_eq!(state.players[0].life, 19);
}

#[test]
fn reckless_lackey_draws_and_makes_a_treasure() {
    let mut state = plains();
    let lackey = put(&mut state, P0, "Reckless Lackey", Zone::Battlefield);
    assert!(keyword(&state, lackey, Keywords::FIRST_STRIKE));
    assert!(keyword(&state, lackey, Keywords::HASTE));
    let hand = state.players[0].hand.len();
    pool(&mut state, P0, &[ManaColor::R], 2);
    activate(&mut state, lackey, 0, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, lackey), Zone::Graveyard);
    assert_eq!(state.players[0].hand.len(), hand + 1);
    assert_eq!(named(&state, P0, "Treasure Token").len(), 1);
}

#[test]
fn sea_gate_oracle_puts_one_of_the_top_two_into_hand_and_the_other_on_the_bottom() {
    let mut state = plains();
    let second = put(&mut state, P0, "Lightning Bolt", Zone::Library);
    let first = put(&mut state, P0, "Thraben Inspector", Zone::Library);
    let oracle = put(&mut state, P0, "Sea Gate Oracle", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 2);
    cast(&mut state, oracle, &[]);
    settle_choosing(&mut state, second);
    assert!(hand_contains(&state, P0, second));
    assert_eq!(state.players[0].library.last(), Some(&first));
}

// ---- Artifacts -------------------------------------------------------------------

#[test]
fn heaped_harvest_finds_a_basic_land_on_entry_and_when_sacrificed() {
    let mut state = plains();
    let first = put(&mut state, P0, "Forest", Zone::Library);
    let harvest = put(&mut state, P0, "Heaped Harvest", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 2);
    cast(&mut state, harvest, &[]);
    settle_choosing(&mut state, first);
    assert_eq!(zone(&state, first), Zone::Battlefield);
    assert!(state.objects.get(first).tapped);

    let second = put(&mut state, P0, "Forest", Zone::Library);
    pool(&mut state, P0, &[], 2);
    activate(&mut state, harvest, 0, &[]);
    settle_choosing(&mut state, second);
    assert_eq!(zone(&state, harvest), Zone::Graveyard);
    assert_eq!(state.players[0].life, 23);
    assert_eq!(zone(&state, second), Zone::Battlefield, "sacrificed");
}

#[test]
fn mirran_banesplitter_attaches_as_it_enters_and_equips_for_three() {
    let mut state = plains();
    let inspector = put(&mut state, P0, "Thraben Inspector", Zone::Battlefield);
    let other = put(&mut state, P0, "Thraben Inspector", Zone::Battlefield);
    let splitter = put(&mut state, P0, "Mirran Banesplitter", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 0);
    cast(&mut state, splitter, &[]);
    settle_choosing(&mut state, inspector);
    assert_eq!(power_toughness(&state, inspector), (3, 2));
    pool(&mut state, P0, &[], 3);
    activate(&mut state, splitter, 0, &[Target::Object(other)]);
    settled(&mut state);
    assert_eq!(power_toughness(&state, other), (3, 2));
    assert_eq!(power_toughness(&state, inspector), (1, 2));
}

// ---- Spells ---------------------------------------------------------------------

#[test]
fn crossover_collaboration_impulses_two_and_teamwork_makes_a_treasure() {
    let mut state = plains();
    let crossover = put(&mut state, P0, "Crossover Collaboration", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 2);
    let top = state.players[0].library[..2].to_vec();
    cast(&mut state, crossover, &[]);
    settled(&mut state);
    assert!(top.iter().all(|&card| zone(&state, card) == Zone::Exile));
    assert!(
        named(&state, P0, "Treasure Token").is_empty(),
        "no teamwork"
    );

    // Teamwork: two creatures with total power 2.
    let mut state = plains();
    let first = put(&mut state, P0, "Thraben Inspector", Zone::Battlefield);
    let second = put(&mut state, P0, "Thraben Inspector", Zone::Battlefield);
    let crossover = put(&mut state, P0, "Crossover Collaboration", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 2);
    assert!(castable(&mut state, crossover));
    act(&mut state, Action::CastSpell(crossover));
    let mut chose_teamwork = false;
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseCastMode { options, .. } => {
            assert!(options.contains(&CastMode::Alternative));
            chose_teamwork = true;
            Action::ChooseCastMode(CastMode::Alternative)
        }
        other => first_answer(other),
    });
    assert!(chose_teamwork);
    assert!(state.objects.get(first).tapped && state.objects.get(second).tapped);
    assert_eq!(named(&state, P0, "Treasure Token").len(), 1);

    // One creature with power 1 can't pay teamwork.
    let mut state = plains();
    put(&mut state, P0, "Thraben Inspector", Zone::Battlefield);
    let crossover = put(&mut state, P0, "Crossover Collaboration", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 2);
    act(&mut state, Action::CastSpell(crossover));
    assert!(!matches!(next(&mut state), Decision::ChooseCastMode { .. }));
}

#[test]
fn defile_shrinks_a_creature_for_each_swamp() {
    let mut state = plains();
    put(&mut state, P0, "Swamp", Zone::Battlefield);
    let target = put(&mut state, P1, "Thraben Inspector", Zone::Battlefield);
    let defile = put(&mut state, P0, "Defile", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 0);
    cast(&mut state, defile, &[Target::Object(target)]);
    settled(&mut state);
    assert_eq!(power_toughness(&state, target), (0, 1));

    put(&mut state, P0, "Swamp", Zone::Battlefield);
    let defile = put(&mut state, P0, "Defile", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 0);
    cast(&mut state, defile, &[Target::Object(target)]);
    settled(&mut state);
    assert_eq!(zone(&state, target), Zone::Graveyard);
}

#[test]
fn desperate_ritual_adds_three_red_and_splices_onto_arcane() {
    let mut state = plains();
    let ritual = put(&mut state, P0, "Desperate Ritual", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 1);
    cast(&mut state, ritual, &[]);
    settled(&mut state);
    assert_eq!(mana(&state, P0, ManaColor::R), 3);

    // Splicing two more Desperate Rituals: {1}{R} each, {R}{R}{R} each.
    let mut state = plains();
    let ritual = put(&mut state, P0, "Desperate Ritual", Zone::Hand);
    let spliced = [
        put(&mut state, P0, "Desperate Ritual", Zone::Hand),
        put(&mut state, P0, "Desperate Ritual", Zone::Hand),
    ];
    pool(&mut state, P0, &[ManaColor::R; 3], 3);
    assert!(castable(&mut state, ritual));
    act(&mut state, Action::CastSpell(ritual));
    match next(&mut state) {
        Decision::ChooseEffectOption { option_count, .. } => assert_eq!(option_count, 3),
        other => panic!("expected the splice count, got {other:?}"),
    }
    act(&mut state, Action::ChooseEffectOption(2));
    assert!(matches!(
        next(&mut state),
        Decision::CastSpellOrPass { player: P0, .. }
    ));
    for card in spliced {
        assert!(
            hand_contains(&state, P0, card),
            "spliced cards stay in hand"
        );
        assert!(
            state
                .known_hand_cards(P1, P0)
                .iter()
                .any(|entry| entry.object == card),
            "revealed"
        );
    }
    settled(&mut state);
    assert_eq!(mana(&state, P0, ManaColor::R), 9);

    // Onto Ethereal Haze, another Arcane spell.
    let mut state = plains();
    let haze = put(&mut state, P0, "Ethereal Haze", Zone::Hand);
    put(&mut state, P0, "Desperate Ritual", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W, ManaColor::R], 1);
    act(&mut state, Action::CastSpell(haze));
    match next(&mut state) {
        Decision::ChooseEffectOption { option_count, .. } => assert_eq!(option_count, 2),
        other => panic!("expected the splice count, got {other:?}"),
    }
    act(&mut state, Action::ChooseEffectOption(1));
    settled(&mut state);
    assert_eq!(mana(&state, P0, ManaColor::R), 3);
}

#[test]
fn first_day_of_class_counters_and_hastes_creatures_entering_this_turn() {
    let mut state = plains();
    let class = put(&mut state, P0, "First Day of Class", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 1);
    cast(&mut state, class, &[]);
    settle_first(&mut state);
    let inspector = put(&mut state, P0, "Thraben Inspector", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W], 0);
    cast(&mut state, inspector, &[]);
    settled(&mut state);
    assert_eq!(power_toughness(&state, inspector), (2, 3));
    assert!(keyword(&state, inspector, Keywords::HASTE));

    // Learn: discard a card to draw a card.
    let mut state = plains();
    let class = put(&mut state, P0, "First Day of Class", Zone::Hand);
    let discarded = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 1);
    cast(&mut state, class, &[]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseOptionalCost { .. } => {
            Action::ChooseOptionalCost(OptionalCostChoice::Discard)
        }
        other => first_answer(other),
    });
    assert_eq!(zone(&state, discarded), Zone::Graveyard);
    assert_eq!(state.players[0].hand.len(), 1, "drew a card");
}

#[test]
fn glimpse_the_impossible_exiles_three_and_spawns_for_each_left_at_the_end_step() {
    let mut state = plains();
    let glimpse = put(&mut state, P0, "Glimpse the Impossible", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 2);
    let top = state.players[0].library[..3].to_vec();
    cast(&mut state, glimpse, &[]);
    settled(&mut state);
    assert!(top.iter().all(|&card| zone(&state, card) == Zone::Exile));
    // Play one of them this turn.
    act(&mut state, Action::PlayLand(top[0]));
    settled(&mut state);
    assert_eq!(zone(&state, top[0]), Zone::Battlefield);
    pass_until(&mut state, |state| {
        state.active_player == P1 && state.step == Step::Main1
    });
    assert_eq!(zone(&state, top[1]), Zone::Graveyard);
    assert_eq!(zone(&state, top[2]), Zone::Graveyard);
    assert_eq!(named(&state, P0, "Eldrazi Spawn Token").len(), 2);
}

/// P1, active, casts `creature` and passes; P0 answers with Glorious Gale.
fn gale_a_creature_spell(creature: &str) -> (GameState, ObjectId, ObjectId) {
    let mut state = plains();
    state.active_player = P1;
    state.priority_player = P1;
    let bearer = put(&mut state, P0, "Thraben Inspector", Zone::Battlefield);
    let spell = put(&mut state, P1, creature, Zone::Hand);
    pool(&mut state, P1, &[ManaColor::B, ManaColor::W], 3);
    cast(&mut state, spell, &[]);
    assert!(matches!(
        next(&mut state),
        Decision::CastSpellOrPass { player: P1, .. }
    ));
    act(&mut state, Action::Pass);
    let gale = put(&mut state, P0, "Glorious Gale", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 1);
    cast(&mut state, gale, &[Target::Object(spell)]);
    settle_choosing(&mut state, bearer);
    (state, spell, bearer)
}

#[test]
fn glorious_gale_counters_a_creature_spell_and_a_legend_tempts_with_the_ring() {
    let (state, spell, bearer) = gale_a_creature_spell("Thraben Inspector");
    assert_eq!(zone(&state, spell), Zone::Graveyard, "countered");
    assert_eq!(mtg_kernel::eternal_pauper_e8b_v1::ring_level(&state, P0), 0);
    assert!(!engine::effective_supertypes(&state, bearer).contains(&Supertype::Legendary));

    let (state, spell, bearer) = gale_a_creature_spell("Oroku Saki, Shredder Rising");
    assert_eq!(zone(&state, spell), Zone::Graveyard, "countered");
    assert_eq!(mtg_kernel::eternal_pauper_e8b_v1::ring_level(&state, P0), 1);
    assert_eq!(
        mtg_kernel::eternal_pauper_e8b_v1::ring_bearer(&state, P0),
        Some(bearer)
    );
    assert!(engine::effective_supertypes(&state, bearer).contains(&Supertype::Legendary));
}

#[test]
fn the_ring_bearer_is_unblockable_by_greater_power_and_loots_and_drains() {
    let mut state = plains();
    let bearer = put(&mut state, P0, "Thraben Inspector", Zone::Battlefield);
    let big = put(&mut state, P1, "Crimson Fleet Commodore", Zone::Battlefield);
    let small = put(&mut state, P1, "Sunscape Familiar", Zone::Battlefield);
    ring(&mut state, bearer, 1);
    let blockers = legal_blockers(&mut state, bearer);
    assert!(blockers.contains(&small));
    assert!(!blockers.contains(&big), "greater power can't block");

    // The second and fourth abilities: loot on attack, drain on damage.
    let mut state = plains();
    let bearer = put(&mut state, P0, "Thraben Inspector", Zone::Battlefield);
    let discard = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    ring(&mut state, bearer, 4);
    let hand = state.players[0].hand.len();
    attack_with(&mut state, vec![bearer], |_, decision| match decision {
        Decision::Discard { choices, .. } => {
            assert!(choices.contains(&discard));
            Some(Action::Discard(vec![discard]))
        }
        _ => None,
    });
    assert_eq!(zone(&state, discard), Zone::Graveyard);
    assert_eq!(state.players[0].hand.len(), hand, "drew, then discarded");
    assert_eq!(state.players[1].life, 20 - 1 - 3);
}

#[test]
fn the_ring_bearers_blockers_are_sacrificed_at_end_of_combat() {
    let mut state = plains();
    let bearer = put(&mut state, P0, "Thornscape Familiar", Zone::Battlefield);
    let blocker = put(&mut state, P1, "Sunscape Familiar", Zone::Battlefield);
    ring(&mut state, bearer, 3);
    let hand = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    attack_with(&mut state, vec![bearer], |_, decision| match decision {
        Decision::DeclareBlockers { player: P1, .. } => {
            Some(Action::DeclareBlockers(vec![(blocker, bearer)]))
        }
        Decision::Discard { .. } => Some(Action::Discard(vec![hand])),
        _ => None,
    });
    assert_eq!(zone(&state, blocker), Zone::Graveyard, "sacrificed");
    assert_eq!(zone(&state, bearer), Zone::Battlefield);
}

#[test]
fn manamorphose_adds_two_mana_of_any_colors_and_draws() {
    let mut state = plains();
    let morph = put(&mut state, P0, "Manamorphose", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 1);
    cast(&mut state, morph, &[]);
    // Options pair the colors in W, U, B, R, G order; 8 is {U}{G}.
    settle_option(&mut state, 8);
    assert_eq!(mana(&state, P0, ManaColor::U), 1);
    assert_eq!(mana(&state, P0, ManaColor::G), 1);
    assert_eq!(state.players[0].hand.len(), 1);
}

#[test]
fn meeting_of_minds_convokes_and_draws_two() {
    let mut state = plains();
    let creatures = [
        put(&mut state, P0, "Thraben Inspector", Zone::Battlefield),
        put(&mut state, P0, "Thraben Inspector", Zone::Battlefield),
        put(&mut state, P0, "Thraben Inspector", Zone::Battlefield),
    ];
    let meeting = put(&mut state, P0, "Meeting of Minds", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 0);
    cast(&mut state, meeting, &[]);
    settle_first(&mut state);
    assert_eq!(zone(&state, meeting), Zone::Graveyard);
    assert_eq!(state.players[0].hand.len(), 2);
    assert!(creatures
        .iter()
        .all(|&creature| state.objects.get(creature).tapped));
}

#[test]
fn rally_the_peasants_pumps_and_flashes_back() {
    let mut state = plains();
    let inspector = put(&mut state, P0, "Thraben Inspector", Zone::Battlefield);
    let rally = put(&mut state, P0, "Rally the Peasants", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W], 2);
    cast(&mut state, rally, &[]);
    settled(&mut state);
    assert_eq!(power_toughness(&state, inspector), (3, 2));
    assert_eq!(zone(&state, rally), Zone::Graveyard);
    pool(&mut state, P0, &[ManaColor::R], 2);
    cast(&mut state, rally, &[]);
    settled(&mut state);
    assert_eq!(power_toughness(&state, inspector), (5, 2));
    assert_eq!(zone(&state, rally), Zone::Exile);
}

#[test]
fn rancid_earth_destroys_a_land_and_with_threshold_damages_everything() {
    let mut state = plains();
    let land = put(&mut state, P1, "Island", Zone::Battlefield);
    let creature = put(&mut state, P1, "Thraben Inspector", Zone::Battlefield);
    let earth = put(&mut state, P0, "Rancid Earth", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B, ManaColor::B], 1);
    cast(&mut state, earth, &[Target::Object(land)]);
    settled(&mut state);
    assert_eq!(zone(&state, land), Zone::Graveyard);
    assert_eq!(state.objects.get(creature).damage, 0);

    let mut state = plains();
    for _ in 0..7 {
        put(&mut state, P0, "Plains", Zone::Graveyard);
    }
    let land = put(&mut state, P1, "Island", Zone::Battlefield);
    let creature = put(&mut state, P1, "Thraben Inspector", Zone::Battlefield);
    let earth = put(&mut state, P0, "Rancid Earth", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B, ManaColor::B], 1);
    cast(&mut state, earth, &[Target::Object(land)]);
    settled(&mut state);
    assert_eq!(zone(&state, land), Zone::Graveyard);
    assert_eq!(state.objects.get(creature).damage, 1);
    assert_eq!(state.players[0].life, 19);
    assert_eq!(state.players[1].life, 19);
}

#[test]
fn rides_end_costs_three_less_against_a_tapped_permanent() {
    let mut state = plains();
    let untapped = put(&mut state, P1, "Thraben Inspector", Zone::Battlefield);
    let rides_end = put(&mut state, P0, "Ride's End", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W], 1);
    assert!(
        !castable(&mut state, rides_end),
        "full cost against untapped"
    );
    state.objects.get_mut(untapped).tapped = true;
    cast(&mut state, rides_end, &[Target::Object(untapped)]);
    settled(&mut state);
    assert_eq!(zone(&state, untapped), Zone::Exile);
}

#[test]
fn seething_song_adds_five_red() {
    let mut state = plains();
    let song = put(&mut state, P0, "Seething Song", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 2);
    cast(&mut state, song, &[]);
    settled(&mut state);
    assert_eq!(mana(&state, P0, ManaColor::R), 5);
}

#[test]
fn seize_the_storm_makes_an_elemental_sized_by_spells_and_flashback_cards() {
    let mut state = plains();
    put(&mut state, P0, "Lightning Bolt", Zone::Graveyard);
    put(&mut state, P0, "Lightning Bolt", Zone::Graveyard);
    put(&mut state, P0, "Rally the Peasants", Zone::Exile);
    let storm = put(&mut state, P0, "Seize the Storm", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 4);
    cast(&mut state, storm, &[]);
    settled(&mut state);
    let elementals = named(&state, P0, "Red Elemental Token");
    assert_eq!(elementals.len(), 1);
    // Two Bolts and Seize the Storm itself in the graveyard, plus the
    // exiled Rally the Peasants.
    assert_eq!(power_toughness(&state, elementals[0]), (4, 4));
    assert!(keyword(&state, elementals[0], Keywords::TRAMPLE));

    // Flashback {6}{R}: then it is exiled with flashback.
    pool(&mut state, P0, &[ManaColor::R], 6);
    cast(&mut state, storm, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, storm), Zone::Exile);
    let elementals = named(&state, P0, "Red Elemental Token");
    assert_eq!(elementals.len(), 2);
    assert_eq!(power_toughness(&state, elementals[1]), (4, 4));
}

#[test]
fn skred_deals_damage_equal_to_snow_permanents() {
    let mut state = plains();
    put(&mut state, P0, "Snow-Covered Mountain", Zone::Battlefield);
    let target = put(&mut state, P1, "Sunscape Familiar", Zone::Battlefield);
    let skred = put(&mut state, P0, "Skred", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 0);
    cast(&mut state, skred, &[Target::Object(target)]);
    settled(&mut state);
    assert_eq!(state.objects.get(target).damage, 1);

    put(&mut state, P0, "Volatile Fjord", Zone::Battlefield);
    put(&mut state, P0, "Snow-Covered Island", Zone::Battlefield);
    let skred = put(&mut state, P0, "Skred", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 0);
    cast(&mut state, skred, &[Target::Object(target)]);
    settled(&mut state);
    assert_eq!(zone(&state, target), Zone::Graveyard);
}

#[test]
fn twinned_vision_draws_two_when_not_cast_from_hand() {
    let mut state = plains();
    let vision = put(&mut state, P0, "Twinned Vision", Zone::Hand);
    let discard = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 1);
    cast(&mut state, vision, &[]);
    settled(&mut state);
    assert_eq!(state.players[0].hand.len(), 2, "the Bolt and one card");
    assert_eq!(zone(&state, vision), Zone::Graveyard);

    // Flashback {1}{U/R}{U/R}, discarding a card: draws two.
    pool(&mut state, P0, &[ManaColor::U, ManaColor::R], 1);
    assert!(castable(&mut state, vision));
    act(&mut state, Action::CastSpell(vision));
    settle_with(&mut state, |_, decision| match decision {
        Decision::Discard { .. } => Action::Discard(vec![discard]),
        other => first_answer(other),
    });
    assert_eq!(zone(&state, discard), Zone::Graveyard);
    assert_eq!(zone(&state, vision), Zone::Exile);
    assert_eq!(state.players[0].hand.len(), 3);
}

#[test]
fn wrenns_resolve_exiles_two_playable_until_the_end_of_next_turn() {
    let mut state = plains();
    let resolve = put(&mut state, P0, "Wrenn's Resolve", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 1);
    let top = state.players[0].library[..2].to_vec();
    cast(&mut state, resolve, &[]);
    settled(&mut state);
    assert!(top.iter().all(|&card| zone(&state, card) == Zone::Exile));
    act(&mut state, Action::PlayLand(top[0]));
    settled(&mut state);
    assert_eq!(zone(&state, top[0]), Zone::Battlefield);
}
