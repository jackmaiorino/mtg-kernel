//! Eternal wave P2c: native rules tests for the Premodern Reanimator and
//! Full English Breakfast cards built so far.
#![cfg(feature = "eternal-fixtures")]

use mtg_kernel::card_def::{card_id_by_name, CardDef, Keywords, Subtype, CARD_DEFS};
use mtg_kernel::engine::{self, Action, Decision, OptionalCostChoice};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::ManaColor;
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Target, Zone};

const P0: PlayerId = PlayerId::P0;
const P1: PlayerId = PlayerId::P1;

fn id(name: &str) -> u16 {
    card_id_by_name(name).unwrap_or_else(|| panic!("{name}"))
}

fn def(name: &str) -> &'static CardDef {
    &CARD_DEFS[id(name) as usize]
}

fn name_of(state: &GameState, object: ObjectId) -> &'static str {
    CARD_DEFS[state.objects.get(object).card_def as usize].name
}

/// A first main phase with P0 active and holding priority. Each library
/// holds `library` in order, top first.
fn game(library: &[&str]) -> GameState {
    let ids: Vec<u16> = library.iter().map(|name| id(name)).collect();
    let mut state = GameState::new_from_libraries(
        &ids,
        &ids,
        |id| CARD_DEFS[id as usize].object_name.into(),
        0x5031_4121_0001,
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

fn controls(state: &GameState, player: PlayerId, name: &str) -> Vec<ObjectId> {
    state.players[player.index()]
        .battlefield
        .iter()
        .copied()
        .filter(|&object| name_of(state, object) == name)
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
            legal_targets,
            can_finish: true,
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
            other => first_answer(other),
        };
        act(state, action);
    }
    panic!("condition never held");
}

fn hand_contains(state: &GameState, player: PlayerId, object: ObjectId) -> bool {
    state.players[player.index()].hand.contains(&object)
}

fn mana(state: &GameState, player: PlayerId, color: ManaColor) -> u8 {
    state.players[player.index()].mana_pool[color.pool_index()]
}

#[test]
fn akroma_has_its_keywords_and_protection_from_black_and_red() {
    let mut state = islands();
    let akroma = put(&mut state, P0, "Akroma, Angel of Wrath", Zone::Battlefield);
    assert_eq!(power_toughness(&state, akroma), (6, 6));
    for kw in [
        Keywords::FLYING,
        Keywords::FIRST_STRIKE,
        Keywords::VIGILANCE,
        Keywords::TRAMPLE,
        Keywords::HASTE,
    ] {
        assert!(keyword(&state, akroma, kw));
    }
    let specter = put(&mut state, P1, "Hypnotic Specter", Zone::Battlefield);
    let piledriver = put(&mut state, P1, "Goblin Piledriver", Zone::Battlefield);
    let angel = put(&mut state, P1, "Exalted Angel", Zone::Battlefield);
    let target = Target::Object(akroma);
    assert!(engine::damage_is_prevented_by_protection(
        &state, specter, target
    ));
    assert!(engine::damage_is_prevented_by_protection(
        &state, piledriver, target
    ));
    assert!(!engine::damage_is_prevented_by_protection(
        &state, angel, target
    ));
    assert_eq!(controls(&state, P0, "Akroma, Angel of Wrath"), vec![akroma]);
    assert_eq!(name_of(&state, akroma), "Akroma, Angel of Wrath");
}

#[test]
fn careful_study_draws_two_then_discards_two() {
    let mut state = islands();
    let study = put(&mut state, P0, "Careful Study", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 0);
    let library = state.players[0].library.len();
    cast(&mut state, study, &[]);
    settle_with(&mut state, |_, decision| first_answer(decision));
    assert_eq!(state.players[0].library.len(), library - 2);
    assert!(state.players[0].hand.is_empty());
    assert_eq!(state.players[0].graveyard.len(), 3);
}

#[test]
fn triskelion_enters_with_three_counters_and_pings() {
    let mut state = islands();
    let triskelion = put(&mut state, P0, "Triskelion", Zone::Hand);
    pool(&mut state, P0, &[], 6);
    cast(&mut state, triskelion, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, triskelion), Zone::Battlefield);
    assert_eq!(power_toughness(&state, triskelion), (4, 4));
    activate(&mut state, triskelion, 0, &[Target::Player(P1)]);
    settled(&mut state);
    assert_eq!(state.players[1].life, 19);
    assert_eq!(power_toughness(&state, triskelion), (3, 3));
}

#[test]
fn unearth_returns_a_cheap_creature_only_and_cycles() {
    let mut state = islands();
    let piledriver = put(&mut state, P0, "Goblin Piledriver", Zone::Graveyard);
    let angel = put(&mut state, P0, "Exalted Angel", Zone::Graveyard);
    let unearth = put(&mut state, P0, "Unearth", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 0);
    assert!(castable(&mut state, unearth));
    act(&mut state, Action::CastSpell(unearth));
    match next(&mut state) {
        Decision::ChooseTargets { legal_targets, .. } => {
            assert!(legal_targets.contains(&Target::Object(piledriver)));
            assert!(!legal_targets.contains(&Target::Object(angel)));
        }
        other => panic!("expected targets, got {other:?}"),
    }
    act(&mut state, Action::ChooseTarget(Target::Object(piledriver)));
    settled(&mut state);
    assert_eq!(zone(&state, piledriver), Zone::Battlefield);
    assert_eq!(zone(&state, angel), Zone::Graveyard);

    let mut state = islands();
    let unearth = put(&mut state, P0, "Unearth", Zone::Hand);
    pool(&mut state, P0, &[], 2);
    activate(&mut state, unearth, 0, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, unearth), Zone::Graveyard);
    assert_eq!(state.players[0].hand.len(), 1);
}

#[test]
fn trevas_ruins_returns_a_non_lair_land_or_is_sacrificed() {
    let mut state = islands();
    let forest = put(&mut state, P0, "Forest", Zone::Battlefield);
    let ruins = put(&mut state, P0, "Treva's Ruins", Zone::Hand);
    assert!(def("Treva's Ruins").subtypes.contains(&Subtype::Lair));
    next(&mut state);
    act(&mut state, Action::PlayLand(ruins));
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseEffectTargets {
            legal_targets,
            selected_count: 0,
            ..
        } => {
            assert_eq!(legal_targets, vec![Target::Object(forest)]);
            Action::ChooseEffectTarget(Target::Object(forest))
        }
        other => first_answer(other),
    });
    assert!(hand_contains(&state, P0, forest));
    assert_eq!(zone(&state, ruins), Zone::Battlefield);
    next(&mut state);
    for color in [ManaColor::G, ManaColor::W, ManaColor::U] {
        state.objects.get_mut(ruins).tapped = false;
        act(&mut state, Action::ActivateManaAbilityChoice(ruins, color));
        assert_eq!(mana(&state, P0, color), 1);
    }

    let mut state = islands();
    let ruins = put(&mut state, P0, "Treva's Ruins", Zone::Hand);
    next(&mut state);
    act(&mut state, Action::PlayLand(ruins));
    settled(&mut state);
    assert_eq!(zone(&state, ruins), Zone::Graveyard);
}

#[test]
fn palinchron_untaps_lands_and_returns_to_hand() {
    let mut state = islands();
    let island = put(&mut state, P0, "Island", Zone::Battlefield);
    state.objects.get_mut(island).tapped = true;
    let palinchron = put(&mut state, P0, "Palinchron", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U, ManaColor::U], 5);
    cast(&mut state, palinchron, &[]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseEffectTargets {
            legal_targets,
            selected_count: 0,
            ..
        } => Action::ChooseEffectTarget(legal_targets[0]),
        other => first_answer(other),
    });
    assert_eq!(zone(&state, palinchron), Zone::Battlefield);
    assert!(!state.objects.get(island).tapped);
    assert!(keyword(&state, palinchron, Keywords::FLYING));
    pool(&mut state, P0, &[ManaColor::U, ManaColor::U], 2);
    activate(&mut state, palinchron, 0, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, palinchron), Zone::Hand);
}

#[test]
fn verdant_force_makes_a_saproling_in_each_upkeep() {
    let mut state = islands();
    put(&mut state, P0, "Verdant Force", Zone::Battlefield);
    assert!(def("Saproling Token")
        .subtypes
        .contains(&Subtype::Saproling));
    // The opponent's upkeep comes first.
    run_until(
        &mut state,
        |state| !controls(state, P0, "Saproling Token").is_empty(),
        |_, _| None,
    );
    let tokens = controls(&state, P0, "Saproling Token");
    assert_eq!(tokens.len(), 1);
    assert_eq!(power_toughness(&state, tokens[0]), (1, 1));
    assert!(controls(&state, P1, "Saproling Token").is_empty());
}
