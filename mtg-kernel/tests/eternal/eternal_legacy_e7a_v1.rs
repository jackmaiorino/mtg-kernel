//! Eternal wave E7a: native rules tests for the cards that complete Legacy
//! Sewer-Cam combo, Rakdos Reanimator, The Epic Storm, Stiflenought, Mardu
//! Energy and Affinity Stompy, and most of Legacy Lands.
#![cfg(feature = "eternal-fixtures")]

use mtg_kernel::card_def::{
    card_id_by_name, CardCapability, CardDef, CardType, Keywords, Subtype, CARD_DEFS,
};
use mtg_kernel::engine::{self, Action, CastMode, Decision};
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::ManaColor;
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Target, Zone};

const P0: PlayerId = PlayerId::P0;
const P1: PlayerId = PlayerId::P1;

const E7A_CARDS: [&str; 50] = [
    "Abhorrent Oculus",
    "Animate Dead",
    "Arena of Glory",
    "Archon of Cruelty",
    "Badlands",
    "Blast Zone",
    "Burning Wish",
    "Cabal Therapy",
    "Chromatic Star",
    "Curie, Emergent Intelligence",
    "Dark Depths",
    "Dress Down",
    "Echo of Eons",
    "Emry, Lurker of the Loch",
    "Exploration",
    "Gaea's Will",
    "Galvanic Relay",
    "Gamble",
    "Ghost Quarter",
    "Goblin Engineer",
    "Goblin Welder",
    "Grafdigger's Cage",
    "Griselbrand",
    "Icetill Explorer",
    "Kappa Cannoneer",
    "Legion Extruder",
    "Life from the Loam",
    "Maze of Ith",
    "Memory Guardian",
    "Mox Diamond",
    "Patchwork Automaton",
    "Phyrexian Dragon Engine",
    "Phyrexian Dreadnought",
    "Pinnacle Emissary",
    "Raucous Theater",
    "Reanimate",
    "Shadowspear",
    "Shadowy Backstreet",
    "Shallow Grave",
    "Silent Clearing",
    "Skateboard",
    "Song of Creation",
    "Sphere of Resistance",
    "Taiga",
    "The Tabernacle at Pendrell Vale",
    "Thespian's Stage",
    "Unmask",
    "Verdant Catacombs",
    "Wooded Foothills",
    "Yavimaya, Cradle of Growth",
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
        0x4537_4121_0001,
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
            live.zone == Zone::Battlefield
                && live.controller == player
                && CARD_DEFS[live.card_def as usize].name == name
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
/// finishing a selection that may finish, declining a Boolean or kicker.
fn first_answer(decision: Decision) -> Action {
    match decision {
        Decision::ChooseTargets {
            can_finish: true,
            ref legal_targets,
            ..
        } if legal_targets.is_empty() => Action::FinishEffectSelection,
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
        Decision::ChooseLegendPermanent { candidates, .. } => {
            Action::ChooseLegendPermanent(candidates[0])
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
        Decision::ChooseCostTargets { candidates, .. } if candidates.contains(&object) => {
            Action::ChooseCostTarget(object)
        }
        other => first_answer(other),
    });
}

fn priority(state: &mut GameState) -> Decision {
    match next(state) {
        decision @ Decision::CastSpellOrPass { .. } => decision,
        other => panic!("expected priority, got {other:?}"),
    }
}

fn castable(state: &mut GameState, spell: ObjectId) -> bool {
    matches!(priority(state), Decision::CastSpellOrPass { castable_spells, .. } if castable_spells.contains(&spell))
}

fn activatable(state: &mut GameState, source: ObjectId, index: u8) -> bool {
    matches!(priority(state), Decision::CastSpellOrPass { activatable_abilities, .. } if activatable_abilities.contains(&(source, index)))
}

fn land_drop(state: &mut GameState, land: ObjectId) -> bool {
    matches!(priority(state), Decision::CastSpellOrPass { land_drops, .. } if land_drops.contains(&land))
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
    for _ in 0..3000 {
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

/// Plays on to P0's next first main phase.
fn to_next_own_main(state: &mut GameState) {
    pass_until(state, |state| state.active_player == P1);
    pass_until(state, |state| {
        state.active_player == P0 && state.step == Step::Main1
    });
}

/// P0 attacks with `attackers` in this turn's combat, then the game plays
/// on to P0's second main phase with an empty stack.
fn attack_with(state: &mut GameState, attackers: Vec<ObjectId>) {
    pass_until(state, |state| state.step == Step::DeclareAttackers);
    act(state, Action::DeclareAttackers(attackers));
    pass_until(state, |state| {
        state.step == Step::Main2
            && state.stack.is_empty()
            && state.engine.pending_triggers.is_empty()
            && state.engine.pending_effect.is_none()
    });
}

fn hand_contains(state: &GameState, player: PlayerId, object: ObjectId) -> bool {
    state.players[player.index()].hand.contains(&object)
}

fn mana(state: &GameState, player: PlayerId, color: ManaColor) -> u8 {
    state.players[player.index()].mana_pool[color.pool_index()]
}

fn hand_len(state: &GameState, player: PlayerId) -> usize {
    state.players[player.index()].hand.len()
}

fn library_len(state: &GameState, player: PlayerId) -> usize {
    state.players[player.index()].library.len()
}

// ---- Characteristics -----------------------------------------------------------

#[test]
fn e7a_cards_are_full_with_their_printed_characteristics() {
    for name in E7A_CARDS {
        assert_eq!(def(name).capability, CardCapability::Full, "{name}");
        assert!(!def(name).is_token, "{name}");
    }
    let lage = def("Marit Lage Token");
    assert!(lage.is_token);
    assert_eq!(lage.capability, CardCapability::Full);
    assert_eq!(lage.object_name, "Marit Lage");
    assert_eq!((lage.power, lage.toughness), (Some(20), Some(20)));
    assert!(lage.keywords.has(Keywords::FLYING) && lage.keywords.has(Keywords::INDESTRUCTIBLE));
    let drone = def("Drone Token");
    assert!(drone.is_token && drone.keywords.has(Keywords::FLYING));
    assert!(drone.has_type(CardType::Artifact) && drone.has_type(CardType::Creature));
    assert_eq!(def("Abhorrent Oculus").subtypes, &[Subtype::Eye]);
    assert!(def("Phyrexian Dreadnought")
        .subtypes
        .contains(&Subtype::Dreadnought));
    for subtype in [Subtype::Eye, Subtype::Dreadnought] {
        assert!(subtype.is_creature_type(), "{subtype:?}");
    }
    let griselbrand = def("Griselbrand");
    assert!(
        griselbrand.keywords.has(Keywords::FLYING) && griselbrand.keywords.has(Keywords::LIFELINK)
    );
    assert!(def("Phyrexian Dragon Engine")
        .keywords
        .has(Keywords::DOUBLE_STRIKE));
    assert!(def("Dress Down").keywords.has(Keywords::FLASH));
    assert!(def("Cabal Therapy").flashback.is_some());
    assert!(def("Echo of Eons").flashback.is_some());
    assert!(def("Unmask").alt_cost.is_some());
    assert!(def("Abhorrent Oculus").additional_cost.is_some());
    assert!(def("Kappa Cannoneer").ward_cost.is_some());
    assert!(def("Patchwork Automaton").ward_cost.is_some());
    assert_eq!(
        def("Badlands").subtypes,
        &[Subtype::Swamp, Subtype::Mountain]
    );
    assert_eq!(def("Taiga").subtypes, &[Subtype::Mountain, Subtype::Forest]);
}

// ---- Lands ---------------------------------------------------------------------

#[test]
fn badlands_and_taiga_tap_for_either_color() {
    for (name, colors) in [
        ("Badlands", [ManaColor::B, ManaColor::R]),
        ("Taiga", [ManaColor::R, ManaColor::G]),
    ] {
        for color in colors {
            let mut state = islands();
            let land = put(&mut state, P0, name, Zone::Battlefield);
            next(&mut state);
            act(&mut state, Action::ActivateManaAbilityChoice(land, color));
            assert_eq!(mana(&state, P0, color), 1, "{name} {color:?}");
            assert!(state.objects.get(land).tapped);
        }
    }
}

#[test]
fn raucous_theater_and_shadowy_backstreet_enter_tapped_and_surveil() {
    for (name, colors) in [
        ("Raucous Theater", [ManaColor::B, ManaColor::R]),
        ("Shadowy Backstreet", [ManaColor::W, ManaColor::B]),
    ] {
        let mut state = islands();
        let land = put(&mut state, P0, name, Zone::Hand);
        assert!(land_drop(&mut state, land));
        act(&mut state, Action::PlayLand(land));
        let mut surveilled = false;
        settle_with(&mut state, |_, decision| {
            surveilled = true;
            first_answer(decision)
        });
        assert!(surveilled, "{name} surveils");
        assert!(state.objects.get(land).tapped, "{name}");
        for color in colors {
            let mut state = islands();
            let land = put(&mut state, P0, name, Zone::Battlefield);
            next(&mut state);
            act(&mut state, Action::ActivateManaAbilityChoice(land, color));
            assert_eq!(mana(&state, P0, color), 1, "{name} {color:?}");
        }
    }
}

#[test]
fn verdant_catacombs_and_wooded_foothills_fetch_their_land_types() {
    for (fetch, wanted, other) in [
        ("Verdant Catacombs", "Badlands", "Mountain"),
        ("Wooded Foothills", "Taiga", "Swamp"),
    ] {
        let mut state = islands();
        put(&mut state, P0, other, Zone::Library);
        let dual = put(&mut state, P0, wanted, Zone::Library);
        let land = put(&mut state, P0, fetch, Zone::Battlefield);
        activate(&mut state, land, 0, &[]);
        settle_with(&mut state, |_, decision| match decision {
            Decision::ChooseEffectTargets { legal_targets, .. }
                if legal_targets.contains(&Target::Object(dual)) =>
            {
                assert_eq!(legal_targets.len(), 1, "{fetch}: only {wanted}");
                Action::ChooseEffectTarget(Target::Object(dual))
            }
            other => first_answer(other),
        });
        assert_eq!(zone(&state, land), Zone::Graveyard, "{fetch}");
        assert_eq!(zone(&state, dual), Zone::Battlefield, "{fetch}");
        assert_eq!(state.players[0].life, 19, "{fetch}");
    }
}

#[test]
fn silent_clearing_pays_life_for_mana_and_cycles_itself() {
    for color in [ManaColor::W, ManaColor::B] {
        let mut state = islands();
        let clearing = put(&mut state, P0, "Silent Clearing", Zone::Battlefield);
        next(&mut state);
        act(
            &mut state,
            Action::ActivateManaAbilityChoice(clearing, color),
        );
        assert_eq!(mana(&state, P0, color), 1);
        assert_eq!(state.players[0].life, 19);
    }
    let mut state = islands();
    let clearing = put(&mut state, P0, "Silent Clearing", Zone::Battlefield);
    pool(&mut state, P0, &[], 1);
    activate(&mut state, clearing, 0, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, clearing), Zone::Graveyard);
    assert_eq!(hand_len(&state, P0), 1);
}

#[test]
fn ghost_quarter_destroys_a_land_and_its_controller_may_fetch_a_basic() {
    let mut state = islands();
    let quarter = put(&mut state, P0, "Ghost Quarter", Zone::Battlefield);
    let taiga = put(&mut state, P1, "Taiga", Zone::Battlefield);
    activate(&mut state, quarter, 0, &[Target::Object(taiga)]);
    let mut searcher = None;
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseEffectTargets {
            player,
            legal_targets,
            ..
        } if !legal_targets.is_empty() => {
            searcher = Some(player);
            Action::ChooseEffectTarget(legal_targets[0])
        }
        other => first_answer(other),
    });
    assert_eq!(searcher, Some(P1), "the land's controller searches");
    assert_eq!(zone(&state, quarter), Zone::Graveyard);
    assert_eq!(zone(&state, taiga), Zone::Graveyard);
    assert_eq!(named(&state, P1, "Island").len(), 1);
    assert_eq!(library_len(&state, P1), 19);
}

#[test]
fn maze_of_ith_untaps_an_attacker_and_prevents_its_combat_damage() {
    let mut state = islands();
    let maze = put(&mut state, P0, "Maze of Ith", Zone::Battlefield);
    let angler = put(&mut state, P1, "Gurmag Angler", Zone::Battlefield);
    run_until(
        &mut state,
        |state| {
            state.active_player == P1
                && state.step == Step::DeclareAttackers
                && state.priority_player == P0
                && state.objects.get(angler).tapped
        },
        |_, decision| match decision {
            Decision::DeclareAttackers { player, .. } if *player == P1 => {
                Some(Action::DeclareAttackers(vec![angler]))
            }
            _ => None,
        },
    );
    assert!(state.objects.get(angler).tapped);
    activate(&mut state, maze, 0, &[Target::Object(angler)]);
    settled(&mut state);
    assert!(!state.objects.get(angler).tapped, "untapped");
    pass_until(&mut state, |state| {
        state.active_player == P1 && state.step == Step::Main2
    });
    assert_eq!(state.players[0].life, 20, "its combat damage is prevented");
}

#[test]
fn thespians_stage_copies_dark_depths_into_marit_lage() {
    let mut state = islands();
    let depths = put(&mut state, P0, "Dark Depths", Zone::Hand);
    act(&mut state, Action::PlayLand(depths));
    settled(&mut state);
    assert_eq!(zone(&state, depths), Zone::Battlefield);
    assert!(named(&state, P0, "Marit Lage Token").is_empty(), "ten ice");
    // {3}: remove an ice counter.
    pool(&mut state, P0, &[], 3);
    activate(&mut state, depths, 0, &[]);
    settled(&mut state);
    assert_eq!(
        state
            .eternal_v1
            .as_ref()
            .unwrap()
            .e7a
            .ice_counters
            .iter()
            .find(|(link, _)| link.object == depths)
            .map(|(_, ice)| *ice),
        Some(9)
    );
    // The Stage becomes a Dark Depths with no ice counters, keeps itself
    // over the legend rule, and is sacrificed for Marit Lage.
    let stage = put(&mut state, P0, "Thespian's Stage", Zone::Battlefield);
    pool(&mut state, P0, &[], 2);
    activate(&mut state, stage, 0, &[Target::Object(depths)]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseLegendPermanent { candidates, .. } => {
            assert!(candidates.contains(&stage));
            Action::ChooseLegendPermanent(stage)
        }
        other => first_answer(other),
    });
    assert_eq!(zone(&state, depths), Zone::Graveyard, "legend rule");
    assert_eq!(zone(&state, stage), Zone::Graveyard, "sacrificed");
    assert_eq!(
        CARD_DEFS[state.objects.get(stage).card_def as usize].name,
        "Thespian's Stage",
        "a copy stops being one as it leaves"
    );
    let lage = named(&state, P0, "Marit Lage Token");
    assert_eq!(lage.len(), 1);
    assert_eq!(power_toughness(&state, lage[0]), (20, 20));
    assert!(keyword(&state, lage[0], Keywords::INDESTRUCTIBLE));
}

#[test]
fn blast_zone_enters_charged_adds_counters_and_destroys_by_mana_value() {
    let mut state = islands();
    let zone_land = put(&mut state, P0, "Blast Zone", Zone::Hand);
    act(&mut state, Action::PlayLand(zone_land));
    settled(&mut state);
    assert_eq!(state.objects.get(zone_land).counters.charge, 1);
    let elves = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    let angler = put(&mut state, P1, "Gurmag Angler", Zone::Battlefield);
    pool(&mut state, P0, &[], 3);
    activate(&mut state, zone_land, 8, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, zone_land), Zone::Graveyard);
    assert_eq!(zone(&state, elves), Zone::Graveyard, "mana value 1");
    assert_eq!(zone(&state, angler), Zone::Battlefield);

    // {X}{X}, {T} with X = 1.
    let mut state = islands();
    let zone_land = put(&mut state, P0, "Blast Zone", Zone::Battlefield);
    pool(&mut state, P0, &[], 2);
    activate(&mut state, zone_land, 1, &[]);
    settled(&mut state);
    assert_eq!(state.objects.get(zone_land).counters.charge, 1);
    assert!(state.objects.get(zone_land).tapped);
}

#[test]
fn arena_of_glory_enters_tapped_without_a_mountain_and_exerts_for_hasty_red() {
    let mut state = islands();
    let arena = put(&mut state, P0, "Arena of Glory", Zone::Hand);
    act(&mut state, Action::PlayLand(arena));
    settled(&mut state);
    assert!(state.objects.get(arena).tapped, "no Mountain");

    let mut state = islands();
    let mountain = put(&mut state, P0, "Mountain", Zone::Battlefield);
    let arena = put(&mut state, P0, "Arena of Glory", Zone::Hand);
    let welder = put(&mut state, P0, "Goblin Welder", Zone::Hand);
    act(&mut state, Action::PlayLand(arena));
    settled(&mut state);
    assert!(!state.objects.get(arena).tapped);
    activate(&mut state, arena, 0, &[]);
    assert!(state.objects.get(arena).tapped);
    assert!(state.objects.get(mountain).tapped, "{{R}} paid");
    assert_eq!(state.players[0].restricted_mana_pool.0.len(), 2);
    cast(&mut state, welder, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, welder), Zone::Battlefield);
    assert!(keyword(&state, welder, Keywords::HASTE));
    assert_eq!(state.players[0].restricted_mana_pool.0.len(), 1);
}

#[test]
fn yavimaya_makes_each_land_a_forest_that_taps_for_green() {
    let mut state = islands();
    let island = put(&mut state, P0, "Island", Zone::Battlefield);
    put(
        &mut state,
        P1,
        "Yavimaya, Cradle of Growth",
        Zone::Battlefield,
    );
    assert!(engine::has_effective_subtype(
        &state,
        island,
        Subtype::Forest
    ));
    assert!(engine::has_effective_subtype(
        &state,
        island,
        Subtype::Island
    ));
    next(&mut state);
    act(
        &mut state,
        Action::ActivateManaAbilityChoice(island, ManaColor::G),
    );
    assert_eq!(mana(&state, P0, ManaColor::G), 1);
}

#[test]
fn the_tabernacle_destroys_each_creature_whose_upkeep_payment_is_missing() {
    let mut state = islands();
    put(
        &mut state,
        P1,
        "The Tabernacle at Pendrell Vale",
        Zone::Battlefield,
    );
    let island = put(&mut state, P0, "Island", Zone::Battlefield);
    let welder = put(&mut state, P0, "Goblin Welder", Zone::Battlefield);
    let angler = put(&mut state, P0, "Gurmag Angler", Zone::Battlefield);
    to_next_own_main(&mut state);
    let survivors = [welder, angler]
        .into_iter()
        .filter(|&creature| zone(&state, creature) == Zone::Battlefield)
        .count();
    assert_eq!(survivors, 1, "one paid with the Island, one destroyed");
    assert!(state.objects.get(island).tapped);
}

#[test]
fn exploration_allows_a_second_land_each_turn() {
    let mut state = islands();
    let first = put(&mut state, P0, "Island", Zone::Hand);
    let second = put(&mut state, P0, "Island", Zone::Hand);
    act(&mut state, Action::PlayLand(first));
    assert!(!land_drop(&mut state, second), "one land a turn");

    let mut state = islands();
    put(&mut state, P0, "Exploration", Zone::Battlefield);
    let first = put(&mut state, P0, "Island", Zone::Hand);
    let second = put(&mut state, P0, "Island", Zone::Hand);
    let third = put(&mut state, P0, "Island", Zone::Hand);
    act(&mut state, Action::PlayLand(first));
    assert!(land_drop(&mut state, second));
    act(&mut state, Action::PlayLand(second));
    assert!(!land_drop(&mut state, third), "one additional");
}

#[test]
fn icetill_explorer_plays_lands_from_the_graveyard_and_mills_on_landfall() {
    let mut state = islands();
    put(&mut state, P0, "Icetill Explorer", Zone::Battlefield);
    let buried = put(&mut state, P0, "Taiga", Zone::Graveyard);
    let held = put(&mut state, P0, "Island", Zone::Hand);
    assert!(land_drop(&mut state, buried));
    act(&mut state, Action::PlayLand(buried));
    settled(&mut state);
    assert_eq!(zone(&state, buried), Zone::Battlefield);
    assert_eq!(library_len(&state, P0), 19, "milled one");
    assert!(land_drop(&mut state, held), "an additional land");
    act(&mut state, Action::PlayLand(held));
    settled(&mut state);
    assert_eq!(library_len(&state, P0), 18);
}

// ---- Artifacts ------------------------------------------------------------------

#[test]
fn mox_diamond_stays_only_if_a_land_card_is_discarded() {
    let mut state = islands();
    let mox = put(&mut state, P0, "Mox Diamond", Zone::Hand);
    let land = put(&mut state, P0, "Taiga", Zone::Hand);
    cast(&mut state, mox, &[]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseEffectOption { option_count, .. } => {
            assert_eq!(option_count, 2, "keep nothing, or discard the Taiga");
            Action::ChooseEffectOption(1)
        }
        other => first_answer(other),
    });
    assert_eq!(zone(&state, mox), Zone::Battlefield);
    assert_eq!(zone(&state, land), Zone::Graveyard);
    next(&mut state);
    act(
        &mut state,
        Action::ActivateManaAbilityChoice(mox, ManaColor::B),
    );
    assert_eq!(mana(&state, P0, ManaColor::B), 1);

    let mut state = islands();
    let mox = put(&mut state, P0, "Mox Diamond", Zone::Hand);
    cast(&mut state, mox, &[]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseEffectOption { option_count, .. } => {
            assert_eq!(option_count, 1);
            Action::ChooseEffectOption(0)
        }
        other => first_answer(other),
    });
    assert_eq!(zone(&state, mox), Zone::Graveyard);
}

#[test]
fn chromatic_star_makes_any_color_and_draws_when_it_dies() {
    let mut state = islands();
    let star = put(&mut state, P0, "Chromatic Star", Zone::Battlefield);
    pool(&mut state, P0, &[], 1);
    next(&mut state);
    act(
        &mut state,
        Action::ActivateManaAbilityChoice(star, ManaColor::G),
    );
    assert_eq!(mana(&state, P0, ManaColor::G), 1);
    assert_eq!(mana(&state, P0, ManaColor::C), 0);
    assert_eq!(zone(&state, star), Zone::Graveyard);
    settled(&mut state);
    assert_eq!(hand_len(&state, P0), 1, "drew a card");
}

#[test]
fn sphere_of_resistance_taxes_every_spell() {
    let mut state = islands();
    put(&mut state, P1, "Sphere of Resistance", Zone::Battlefield);
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 0);
    assert!(!castable(&mut state, bolt));
    pool(&mut state, P0, &[], 1);
    cast(&mut state, bolt, &[Target::Player(P1)]);
    settled(&mut state);
    assert_eq!(state.players[1].life, 17);
}

#[test]
fn grafdiggers_cage_stops_reanimation_and_graveyard_casting() {
    let mut state = islands();
    put(&mut state, P1, "Grafdigger's Cage", Zone::Battlefield);
    let angler = put(&mut state, P0, "Gurmag Angler", Zone::Graveyard);
    let echo = put(&mut state, P0, "Echo of Eons", Zone::Graveyard);
    let reanimate = put(&mut state, P0, "Reanimate", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B, ManaColor::U], 2);
    assert!(!castable(&mut state, echo), "no flashback from a graveyard");
    cast(&mut state, reanimate, &[Target::Object(angler)]);
    settled(&mut state);
    assert_eq!(zone(&state, angler), Zone::Graveyard);
}

#[test]
fn shadowspear_equips_and_strips_indestructible() {
    let mut state = islands();
    let spear = put(&mut state, P0, "Shadowspear", Zone::Battlefield);
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    let bridge = put(&mut state, P1, "Razortide Bridge", Zone::Battlefield);
    pool(&mut state, P0, &[], 3);
    activate(&mut state, spear, 1, &[Target::Object(elves)]);
    settled(&mut state);
    assert_eq!(power_toughness(&state, elves), (2, 2));
    assert!(keyword(&state, elves, Keywords::TRAMPLE));
    assert!(keyword(&state, elves, Keywords::LIFELINK));
    assert!(keyword(&state, bridge, Keywords::INDESTRUCTIBLE));
    activate(&mut state, spear, 0, &[]);
    settled(&mut state);
    assert!(!keyword(&state, bridge, Keywords::INDESTRUCTIBLE));
}

#[test]
fn skateboard_taps_a_permanent_on_entry_and_grants_haste() {
    let mut state = islands();
    let angler = put(&mut state, P1, "Gurmag Angler", Zone::Battlefield);
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    state.objects.get_mut(elves).summoning_sick = true;
    let board = put(&mut state, P0, "Skateboard", Zone::Hand);
    pool(&mut state, P0, &[], 2);
    cast(&mut state, board, &[]);
    settle_choosing(&mut state, angler);
    assert!(state.objects.get(angler).tapped);
    activate(&mut state, board, 0, &[Target::Object(elves)]);
    settled(&mut state);
    assert_eq!(power_toughness(&state, elves), (2, 1));
    assert!(keyword(&state, elves, Keywords::HASTE));
}

#[test]
fn legion_extruder_deals_two_on_entry_and_makes_golems() {
    let mut state = islands();
    let star = put(&mut state, P0, "Chromatic Star", Zone::Battlefield);
    let extruder = put(&mut state, P0, "Legion Extruder", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 1);
    cast(&mut state, extruder, &[]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseTargets { .. } => Action::ChooseTarget(Target::Player(P1)),
        other => first_answer(other),
    });
    assert_eq!(state.players[1].life, 18);
    pool(&mut state, P0, &[], 2);
    activate(&mut state, extruder, 0, &[]);
    settle_choosing(&mut state, star);
    assert_eq!(zone(&state, star), Zone::Graveyard);
    assert_eq!(named(&state, P0, "Golem Token").len(), 1);
}

#[test]
fn goblin_engineer_entombs_an_artifact_and_returns_a_cheap_one() {
    let mut state = islands();
    let cage = put(&mut state, P0, "Grafdigger's Cage", Zone::Library);
    let engineer = put(&mut state, P0, "Goblin Engineer", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 1);
    cast(&mut state, engineer, &[]);
    settle_choosing(&mut state, cage);
    assert_eq!(zone(&state, cage), Zone::Graveyard);

    let mut state = islands();
    let engineer = put(&mut state, P0, "Goblin Engineer", Zone::Battlefield);
    let star = put(&mut state, P0, "Chromatic Star", Zone::Battlefield);
    let sphere = put(&mut state, P0, "Sphere of Resistance", Zone::Graveyard);
    let dreadnought = put(&mut state, P0, "Phyrexian Dreadnought", Zone::Graveyard);
    let guardian = put(&mut state, P0, "Memory Guardian", Zone::Graveyard);
    pool(&mut state, P0, &[ManaColor::R], 0);
    act(&mut state, Action::ActivateAbility(engineer, 0));
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseTargets { legal_targets, .. } => {
            assert!(legal_targets.contains(&Target::Object(dreadnought)));
            assert!(
                !legal_targets.contains(&Target::Object(guardian)),
                "mana value 5"
            );
            Action::ChooseTarget(Target::Object(sphere))
        }
        Decision::ChooseCostTargets { candidates, .. } => {
            assert!(candidates.contains(&star));
            Action::ChooseCostTarget(star)
        }
        other => first_answer(other),
    });
    assert_eq!(zone(&state, star), Zone::Graveyard);
    assert_eq!(zone(&state, sphere), Zone::Battlefield);
}

#[test]
fn goblin_welder_swaps_an_artifact_with_one_in_its_graveyard() {
    let mut state = islands();
    let welder = put(&mut state, P0, "Goblin Welder", Zone::Battlefield);
    let sphere = put(&mut state, P1, "Sphere of Resistance", Zone::Battlefield);
    let cage = put(&mut state, P1, "Grafdigger's Cage", Zone::Graveyard);
    let mine = put(&mut state, P0, "Chromatic Star", Zone::Graveyard);
    act(&mut state, Action::ActivateAbility(welder, 0));
    choose_target(&mut state, Target::Object(sphere));
    match next(&mut state) {
        Decision::ChooseTargets { legal_targets, .. } => {
            assert!(
                !legal_targets.contains(&Target::Object(mine)),
                "that player's graveyard"
            );
            act(&mut state, Action::ChooseTarget(Target::Object(cage)));
        }
        other => panic!("{other:?}"),
    }
    settled(&mut state);
    assert_eq!(zone(&state, sphere), Zone::Graveyard);
    assert_eq!(zone(&state, cage), Zone::Battlefield);
    assert_eq!(state.objects.get(cage).controller, P1);
}

// ---- Creatures ------------------------------------------------------------------

#[test]
fn patchwork_automaton_grows_when_its_controller_casts_an_artifact() {
    let mut state = islands();
    let automaton = put(&mut state, P0, "Patchwork Automaton", Zone::Battlefield);
    let star = put(&mut state, P0, "Chromatic Star", Zone::Hand);
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 1);
    cast(&mut state, bolt, &[Target::Player(P1)]);
    settled(&mut state);
    assert_eq!(
        power_toughness(&state, automaton),
        (1, 1),
        "not an artifact"
    );
    cast(&mut state, star, &[]);
    settled(&mut state);
    assert_eq!(power_toughness(&state, automaton), (2, 2));
}

#[test]
fn pinnacle_emissary_makes_a_drone_for_each_artifact_spell() {
    let mut state = islands();
    put(&mut state, P0, "Pinnacle Emissary", Zone::Battlefield);
    let star = put(&mut state, P0, "Chromatic Star", Zone::Hand);
    pool(&mut state, P0, &[], 1);
    cast(&mut state, star, &[]);
    settled(&mut state);
    let drones = named(&state, P0, "Drone Token");
    assert_eq!(drones.len(), 1);
    assert!(keyword(&state, drones[0], Keywords::FLYING));
    assert_eq!(power_toughness(&state, drones[0]), (1, 1));

    // Warp {U/R}: exiled at the beginning of the next end step.
    let mut state = islands();
    let emissary = put(&mut state, P0, "Pinnacle Emissary", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 0);
    cast(&mut state, emissary, &[]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseCastMode { options, .. } => {
            assert!(options.contains(&CastMode::Alternative));
            Action::ChooseCastMode(CastMode::Alternative)
        }
        other => first_answer(other),
    });
    assert_eq!(zone(&state, emissary), Zone::Battlefield);
    pass_until(&mut state, |state| state.active_player == P1);
    assert_eq!(zone(&state, emissary), Zone::Exile);
}

#[test]
fn kappa_cannoneer_improvises_and_grows_unblockable_as_artifacts_enter() {
    let mut state = islands();
    let stars: Vec<ObjectId> = (0..5)
        .map(|_| put(&mut state, P0, "Chromatic Star", Zone::Battlefield))
        .collect();
    let kappa = put(&mut state, P0, "Kappa Cannoneer", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 0);
    cast(&mut state, kappa, &[]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseCastMode { options, .. } => {
            assert!(options.contains(&CastMode::Alternative));
            Action::ChooseCastMode(CastMode::Alternative)
        }
        Decision::ChooseEffectTargets {
            legal_targets,
            can_finish: false,
            ..
        } => Action::ChooseEffectTarget(legal_targets[0]),
        other => first_answer(other),
    });
    assert_eq!(zone(&state, kappa), Zone::Battlefield);
    assert!(stars.iter().all(|&star| state.objects.get(star).tapped));
    assert_eq!(power_toughness(&state, kappa), (5, 5), "its own entry");
    assert!(keyword(&state, kappa, Keywords::CANT_BE_BLOCKED));
    let star = put(&mut state, P0, "Chromatic Star", Zone::Hand);
    pool(&mut state, P0, &[], 1);
    cast(&mut state, star, &[]);
    settled(&mut state);
    assert_eq!(power_toughness(&state, kappa), (6, 6));
}

#[test]
fn memory_guardian_costs_one_less_per_artifact() {
    let mut state = islands();
    for _ in 0..4 {
        put(&mut state, P0, "Chromatic Star", Zone::Battlefield);
    }
    let guardian = put(&mut state, P0, "Memory Guardian", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 0);
    cast(&mut state, guardian, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, guardian), Zone::Battlefield);
    assert!(keyword(&state, guardian, Keywords::FLYING));
}

#[test]
fn emry_mills_four_and_lets_an_artifact_be_cast_from_the_graveyard() {
    let mut state = islands();
    put(&mut state, P0, "Chromatic Star", Zone::Battlefield);
    put(&mut state, P0, "Chromatic Star", Zone::Battlefield);
    let emry = put(&mut state, P0, "Emry, Lurker of the Loch", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 0);
    cast(&mut state, emry, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, emry), Zone::Battlefield);
    assert_eq!(library_len(&state, P0), 16, "milled four");

    let mut state = islands();
    let emry = put(
        &mut state,
        P0,
        "Emry, Lurker of the Loch",
        Zone::Battlefield,
    );
    let star = put(&mut state, P0, "Chromatic Star", Zone::Graveyard);
    pool(&mut state, P0, &[], 1);
    assert!(!castable(&mut state, star));
    activate(&mut state, emry, 0, &[Target::Object(star)]);
    settled(&mut state);
    cast(&mut state, star, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, star), Zone::Battlefield);
}

#[test]
fn curie_becomes_a_copy_and_draws_its_base_power_on_combat_damage() {
    let mut state = islands();
    let curie = put(
        &mut state,
        P0,
        "Curie, Emergent Intelligence",
        Zone::Battlefield,
    );
    let dreadnought = put(&mut state, P0, "Phyrexian Dreadnought", Zone::Battlefield);
    pool(&mut state, P0, &[ManaColor::U], 1);
    activate(&mut state, curie, 0, &[]);
    settle_choosing(&mut state, dreadnought);
    assert_eq!(zone(&state, dreadnought), Zone::Exile);
    assert_eq!(power_toughness(&state, curie), (12, 12));
    assert!(keyword(&state, curie, Keywords::TRAMPLE));
    attack_with(&mut state, vec![curie]);
    assert_eq!(state.players[1].life, 8);
    assert_eq!(hand_len(&state, P0), 12, "drew its base power");
}

#[test]
fn phyrexian_dreadnought_is_sacrificed_unless_twelve_power_is() {
    let mut state = islands();
    let dreadnought = put(&mut state, P0, "Phyrexian Dreadnought", Zone::Hand);
    pool(&mut state, P0, &[], 1);
    cast(&mut state, dreadnought, &[]);
    settle_first(&mut state);
    assert_eq!(zone(&state, dreadnought), Zone::Graveyard);

    let mut state = islands();
    let anglers: Vec<ObjectId> = (0..3)
        .map(|_| put(&mut state, P0, "Gurmag Angler", Zone::Battlefield))
        .collect();
    let dreadnought = put(&mut state, P0, "Phyrexian Dreadnought", Zone::Hand);
    pool(&mut state, P0, &[], 1);
    cast(&mut state, dreadnought, &[]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseEffectTargets { legal_targets, .. } => match anglers
            .iter()
            .find(|&&angler| legal_targets.contains(&Target::Object(angler)))
        {
            Some(&angler) => Action::ChooseEffectTarget(Target::Object(angler)),
            None => Action::FinishEffectSelection,
        },
        other => first_answer(other),
    });
    assert_eq!(zone(&state, dreadnought), Zone::Battlefield);
    assert!(anglers
        .iter()
        .all(|&angler| zone(&state, angler) == Zone::Graveyard));
}

#[test]
fn phyrexian_dragon_engine_rummages_when_unearthed() {
    let mut state = islands();
    let engine_card = put(&mut state, P0, "Phyrexian Dragon Engine", Zone::Graveyard);
    let held = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R, ManaColor::R], 3);
    activate(&mut state, engine_card, 0, &[]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseEffectOption { option_count, .. } => {
            assert_eq!(option_count, 2, "discard and draw, or not");
            Action::ChooseEffectOption(0)
        }
        other => first_answer(other),
    });
    assert_eq!(zone(&state, engine_card), Zone::Battlefield);
    assert_eq!(zone(&state, held), Zone::Graveyard);
    assert_eq!(hand_len(&state, P0), 3);
    assert!(keyword(&state, engine_card, Keywords::DOUBLE_STRIKE));
    pass_until(&mut state, |state| state.active_player == P1);
    assert_eq!(zone(&state, engine_card), Zone::Exile, "unearth");
}

#[test]
fn abhorrent_oculus_exiles_six_and_manifests_dread_each_opponent_upkeep() {
    let mut state = islands();
    let fodder: Vec<ObjectId> = (0..6)
        .map(|_| put(&mut state, P0, "Island", Zone::Graveyard))
        .collect();
    let oculus = put(&mut state, P0, "Abhorrent Oculus", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 2);
    cast(&mut state, oculus, &[]);
    settle_first(&mut state);
    assert_eq!(zone(&state, oculus), Zone::Battlefield);
    assert!(fodder.iter().all(|&card| zone(&state, card) == Zone::Exile));
    assert!(keyword(&state, oculus, Keywords::FLYING));
    pass_until(&mut state, |state| {
        state.active_player == P1 && state.step == Step::Main1
    });
    let face_down: Vec<ObjectId> = state.players[0]
        .battlefield
        .iter()
        .copied()
        .filter(|&object| state.objects.get(object).v4.face_down_v1.is_some())
        .collect();
    assert_eq!(face_down.len(), 1, "manifested one");
    assert_eq!(power_toughness(&state, face_down[0]), (2, 2));
    assert_eq!(state.players[0].graveyard.len(), 1, "the other card");
    assert_eq!(library_len(&state, P0), 18);
}

#[test]
fn archon_of_cruelty_punishes_the_opponent_as_it_enters() {
    let mut state = islands();
    let elves = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    let held = put(&mut state, P1, "Lightning Bolt", Zone::Hand);
    let archon = put(&mut state, P0, "Archon of Cruelty", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B, ManaColor::B], 6);
    cast(&mut state, archon, &[]);
    settle_first(&mut state);
    assert_eq!(zone(&state, archon), Zone::Battlefield);
    assert_eq!(zone(&state, elves), Zone::Graveyard);
    assert_eq!(zone(&state, held), Zone::Graveyard);
    assert_eq!(state.players[1].life, 17);
    assert_eq!(state.players[0].life, 23);
    assert_eq!(hand_len(&state, P0), 1);
}

#[test]
fn griselbrand_pays_seven_life_to_draw_seven() {
    let mut state = islands();
    let griselbrand = put(&mut state, P0, "Griselbrand", Zone::Battlefield);
    activate(&mut state, griselbrand, 0, &[]);
    settled(&mut state);
    assert_eq!(state.players[0].life, 13);
    assert_eq!(hand_len(&state, P0), 7);
}

// ---- Reanimation ----------------------------------------------------------------

#[test]
fn reanimate_takes_a_creature_from_any_graveyard_for_its_mana_value_in_life() {
    let mut state = islands();
    let angler = put(&mut state, P1, "Gurmag Angler", Zone::Graveyard);
    let reanimate = put(&mut state, P0, "Reanimate", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 0);
    cast(&mut state, reanimate, &[Target::Object(angler)]);
    settled(&mut state);
    assert_eq!(zone(&state, angler), Zone::Battlefield);
    assert_eq!(state.objects.get(angler).controller, P0);
    assert_eq!(state.players[0].life, 13);
}

#[test]
fn animate_dead_returns_a_creature_weakened_and_takes_it_when_it_leaves() {
    let mut state = islands();
    let angler = put(&mut state, P1, "Gurmag Angler", Zone::Graveyard);
    let animate = put(&mut state, P0, "Animate Dead", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 1);
    cast(&mut state, animate, &[Target::Object(angler)]);
    settled(&mut state);
    assert_eq!(zone(&state, animate), Zone::Battlefield);
    assert_eq!(zone(&state, angler), Zone::Battlefield);
    assert_eq!(state.objects.get(angler).controller, P0);
    assert_eq!(power_toughness(&state, angler), (4, 5));
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(animate, Zone::Graveyard),
    );
    settled(&mut state);
    assert_eq!(zone(&state, angler), Zone::Graveyard);
    assert!(state.players[1].graveyard.contains(&angler));
}

#[test]
fn shallow_grave_returns_the_top_creature_with_haste_until_the_end_step() {
    let mut state = islands();
    let angler = put(&mut state, P0, "Gurmag Angler", Zone::Graveyard);
    let griselbrand = put(&mut state, P0, "Griselbrand", Zone::Graveyard);
    put(&mut state, P0, "Island", Zone::Graveyard);
    let grave = put(&mut state, P0, "Shallow Grave", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 1);
    cast(&mut state, grave, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, griselbrand), Zone::Battlefield, "the top one");
    assert_eq!(zone(&state, angler), Zone::Graveyard);
    assert!(keyword(&state, griselbrand, Keywords::HASTE));
    pass_until(&mut state, |state| state.active_player == P1);
    assert_eq!(zone(&state, griselbrand), Zone::Exile);
}

// ---- Spells ---------------------------------------------------------------------

/// Cabal Therapy's name choice: the option index of the letter group and of
/// the name within it, mirroring the program's table.
fn therapy_name_options(name: &str) -> (u16, u16) {
    let mut seen: Vec<&str> = Vec::new();
    let mut groups: Vec<(char, Vec<&str>)> = Vec::new();
    for def in CARD_DEFS.iter() {
        if def.is_token
            || def.is_land
            || !def.is_executable()
            || def.object_name.is_empty()
            || seen.contains(&def.object_name)
        {
            continue;
        }
        seen.push(def.object_name);
        let letter = def
            .object_name
            .chars()
            .next()
            .map_or(' ', |letter| letter.to_ascii_uppercase());
        match groups.iter_mut().find(|(group, _)| *group == letter) {
            Some((_, names)) => names.push(def.object_name),
            None => groups.push((letter, vec![def.object_name])),
        }
    }
    groups.sort_by_key(|(letter, _)| *letter);
    let group = groups
        .iter()
        .position(|(_, names)| names.contains(&name))
        .unwrap();
    let within = groups[group]
        .1
        .iter()
        .position(|&candidate| candidate == name)
        .unwrap();
    (group as u16, within as u16)
}

#[test]
fn cabal_therapy_discards_every_copy_of_the_named_card_and_flashes_back() {
    let mut state = islands();
    let bolts = [
        put(&mut state, P1, "Lightning Bolt", Zone::Hand),
        put(&mut state, P1, "Lightning Bolt", Zone::Hand),
    ];
    let island = put(&mut state, P1, "Island", Zone::Hand);
    let therapy = put(&mut state, P0, "Cabal Therapy", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 0);
    cast(&mut state, therapy, &[Target::Player(P1)]);
    let (group, within) = therapy_name_options("Lightning Bolt");
    let mut answers = vec![within, group];
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseEffectOption { .. } => Action::ChooseEffectOption(answers.pop().unwrap()),
        other => first_answer(other),
    });
    assert!(bolts
        .iter()
        .all(|&bolt| zone(&state, bolt) == Zone::Graveyard));
    assert_eq!(zone(&state, island), Zone::Hand);
    assert_eq!(zone(&state, therapy), Zone::Graveyard);

    // Flashback: sacrifice a creature.
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    assert!(castable(&mut state, therapy));
    act(&mut state, Action::CastSpell(therapy));
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseTargets { .. } => Action::ChooseTarget(Target::Player(P1)),
        Decision::ChooseCostTargets { candidates, .. } => {
            assert!(candidates.contains(&elves));
            Action::ChooseCostTarget(elves)
        }
        other => first_answer(other),
    });
    assert_eq!(zone(&state, elves), Zone::Graveyard);
    assert_eq!(zone(&state, therapy), Zone::Exile);
}

#[test]
fn unmask_exiles_a_black_card_instead_of_mana_and_takes_a_nonland_card() {
    let mut state = islands();
    let bolt = put(&mut state, P1, "Lightning Bolt", Zone::Hand);
    let island = put(&mut state, P1, "Island", Zone::Hand);
    let unmask = put(&mut state, P0, "Unmask", Zone::Hand);
    let pitched = put(&mut state, P0, "Reanimate", Zone::Hand);
    assert!(castable(&mut state, unmask));
    act(&mut state, Action::CastSpell(unmask));
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseTargets { .. } => Action::ChooseTarget(Target::Player(P1)),
        Decision::ChooseCastMode { .. } => Action::ChooseCastMode(CastMode::Alternative),
        Decision::ChooseCostTargets { candidates, .. } => {
            assert_eq!(candidates, vec![pitched], "only the black card");
            Action::ChooseCostTarget(pitched)
        }
        other => first_answer(other),
    });
    assert_eq!(zone(&state, pitched), Zone::Exile);
    assert_eq!(zone(&state, bolt), Zone::Graveyard);
    assert_eq!(zone(&state, island), Zone::Hand);
}

#[test]
fn echo_of_eons_refills_both_hands_and_flashes_back() {
    let mut state = islands();
    put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    put(&mut state, P0, "Gurmag Angler", Zone::Graveyard);
    put(&mut state, P1, "Lightning Bolt", Zone::Graveyard);
    let echo = put(&mut state, P0, "Echo of Eons", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U, ManaColor::U], 4);
    cast(&mut state, echo, &[]);
    settled(&mut state);
    assert_eq!(hand_len(&state, P0), 7);
    assert_eq!(hand_len(&state, P1), 7);
    assert_eq!(state.players[0].graveyard, vec![echo]);
    assert!(state.players[1].graveyard.is_empty());
    assert_eq!(library_len(&state, P0), 15);
    assert_eq!(library_len(&state, P1), 14);
    pool(&mut state, P0, &[ManaColor::U], 2);
    cast(&mut state, echo, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, echo), Zone::Exile);
}

#[test]
fn burning_wish_exiles_itself() {
    let mut state = islands();
    let wish = put(&mut state, P0, "Burning Wish", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 1);
    cast(&mut state, wish, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, wish), Zone::Exile);
}

#[test]
fn gamble_tutors_a_card_then_discards_one_at_random() {
    let mut state = islands();
    let wanted = put(&mut state, P0, "Griselbrand", Zone::Library);
    put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    let gamble = put(&mut state, P0, "Gamble", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 0);
    cast(&mut state, gamble, &[]);
    settle_choosing(&mut state, wanted);
    assert_eq!(hand_len(&state, P0), 1, "found one, discarded one");
    assert_eq!(state.players[0].graveyard.len(), 2);
    assert_eq!(library_len(&state, P0), 20);
}

#[test]
fn galvanic_relay_storms_and_its_cards_are_playable_next_turn() {
    let mut state = islands();
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    let relay = put(&mut state, P0, "Galvanic Relay", Zone::Hand);
    let top = put(&mut state, P0, "Lightning Bolt", Zone::Library);
    pool(&mut state, P0, &[ManaColor::R, ManaColor::R], 2);
    cast(&mut state, bolt, &[Target::Player(P1)]);
    settled(&mut state);
    cast(&mut state, relay, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, top), Zone::Exile);
    assert_eq!(library_len(&state, P0), 19, "storm copied it once");
    assert_eq!(state.exile.len(), 2);
    pool(&mut state, P0, &[ManaColor::R], 0);
    assert!(!castable(&mut state, top), "not this turn");
    to_next_own_main(&mut state);
    pool(&mut state, P0, &[ManaColor::R], 0);
    cast(&mut state, top, &[Target::Player(P1)]);
    settled(&mut state);
    assert_eq!(state.players[1].life, 14);
}

#[test]
fn gaeas_will_is_suspended_and_lets_lands_be_played_from_the_graveyard() {
    let mut state = islands();
    let will = put(&mut state, P0, "Gaea's Will", Zone::Hand);
    let taiga = put(&mut state, P0, "Taiga", Zone::Graveyard);
    assert!(!castable(&mut state, will), "no mana cost");
    pool(&mut state, P0, &[ManaColor::G], 0);
    activate(&mut state, will, 0, &[]);
    assert_eq!(zone(&state, will), Zone::Exile);
    assert_eq!(state.objects.get(will).v4.time_counters_v1, 4);
    assert_eq!(mana(&state, P0, ManaColor::G), 0);
    assert!(!land_drop(&mut state, taiga));
    for counters in [3, 2, 1] {
        to_next_own_main(&mut state);
        assert_eq!(
            state.objects.get(will).v4.time_counters_v1,
            counters,
            "one removed each upkeep"
        );
    }
    to_next_own_main(&mut state);
    assert_eq!(state.objects.get(will).v4.time_counters_v1, 0);
    assert!(
        land_drop(&mut state, taiga),
        "cast as the last counter left"
    );
    act(&mut state, Action::PlayLand(taiga));
    assert_eq!(zone(&state, taiga), Zone::Battlefield);
}

#[test]
fn life_from_the_loam_returns_lands_and_dredges_on_the_draw() {
    let mut state = islands();
    let lands = [
        put(&mut state, P0, "Taiga", Zone::Graveyard),
        put(&mut state, P0, "Badlands", Zone::Graveyard),
    ];
    let loam = put(&mut state, P0, "Life from the Loam", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 1);
    act(&mut state, Action::CastSpell(loam));
    for land in lands {
        choose_target(&mut state, Target::Object(land));
    }
    settle_first(&mut state);
    assert!(lands.iter().all(|&land| hand_contains(&state, P0, land)));
    assert_eq!(zone(&state, loam), Zone::Graveyard);

    // Dredge 3 instead of the draw-step draw.
    let hand = hand_len(&state, P0);
    pass_until(&mut state, |state| state.active_player == P1);
    run_until(
        &mut state,
        |state| state.active_player == P0 && state.step == Step::Main1,
        |_, decision| match decision {
            Decision::ChooseEffectOption {
                source,
                option_count,
                ..
            } if *source == loam => {
                assert_eq!(*option_count, 2);
                Some(Action::ChooseEffectOption(1))
            }
            _ => None,
        },
    );
    assert!(hand_contains(&state, P0, loam));
    assert_eq!(hand_len(&state, P0), hand + 1, "no card drawn");
    assert_eq!(library_len(&state, P0), 17);
    assert_eq!(state.players[0].graveyard.len(), 3);
}

// ---- Enchantments ---------------------------------------------------------------

#[test]
fn dress_down_draws_removes_creature_abilities_and_leaves_at_the_end_step() {
    let mut state = islands();
    let griselbrand = put(&mut state, P1, "Griselbrand", Zone::Battlefield);
    let dress = put(&mut state, P0, "Dress Down", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 1);
    cast(&mut state, dress, &[]);
    settled(&mut state);
    assert_eq!(hand_len(&state, P0), 1, "drew a card");
    assert!(!keyword(&state, griselbrand, Keywords::FLYING));
    pass_until(&mut state, |state| state.active_player == P1);
    assert_eq!(zone(&state, dress), Zone::Graveyard);
    assert!(keyword(&state, griselbrand, Keywords::FLYING));
}

#[test]
fn song_of_creation_draws_two_per_spell_and_discards_the_hand_at_end_step() {
    let mut state = islands();
    put(&mut state, P0, "Song of Creation", Zone::Battlefield);
    let first = put(&mut state, P0, "Island", Zone::Hand);
    let second = put(&mut state, P0, "Island", Zone::Hand);
    act(&mut state, Action::PlayLand(first));
    assert!(land_drop(&mut state, second), "an additional land");
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 0);
    cast(&mut state, bolt, &[Target::Player(P1)]);
    settled(&mut state);
    assert_eq!(hand_len(&state, P0), 3, "the Island and two draws");
    pass_until(&mut state, |state| state.active_player == P1);
    assert_eq!(hand_len(&state, P0), 0, "discarded at the end step");
}
