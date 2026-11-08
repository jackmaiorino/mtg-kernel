//! Eternal wave P2c: native rules tests for the Premodern Reanimator and
//! Full English Breakfast cards built so far.
#![cfg(feature = "eternal-fixtures")]

use mtg_kernel::card_def::{card_id_by_name, CardDef, CardType, Keywords, Subtype, CARD_DEFS};
use mtg_kernel::engine::{self, Action, Decision, OptionalCostChoice};
use mtg_kernel::eternal_cards_v1::EternalStateV1;
use mtg_kernel::eternal_premodern_p2c_v1::P2cStateV1;
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::ManaColor;
use mtg_kernel::state::{GameObject, GameState, ObjectLinkV4, ObjectStateV4, Step, Target, Zone};
use std::hash::{Hash, Hasher};

const P0: PlayerId = PlayerId::P0;
const P1: PlayerId = PlayerId::P1;

#[test]
fn empty_p2c_state_preserves_legacy_hash_input_and_json() {
    #[derive(Default)]
    struct HashInput(Vec<u8>);

    impl Hasher for HashInput {
        fn finish(&self) -> u64 {
            0
        }

        fn write(&mut self, bytes: &[u8]) {
            self.0.extend_from_slice(bytes);
        }
    }

    let mut empty_input = HashInput::default();
    P2cStateV1::default().hash(&mut empty_input);
    assert!(empty_input.0.is_empty());

    let mut eternal = EternalStateV1 {
        energy: [3, 0],
        ..Default::default()
    };
    let legacy_json = serde_json::to_value(&eternal).unwrap();
    assert!(legacy_json.get("p2c").is_none());
    assert_eq!(
        serde_json::from_value::<EternalStateV1>(legacy_json).unwrap(),
        eternal
    );

    let mut legacy_input = HashInput::default();
    eternal.hash(&mut legacy_input);
    eternal.p2c.paradise_returns.push(ObjectLinkV4 {
        object: ObjectId(7),
        zone_change_count: 2,
    });
    let mut populated_input = HashInput::default();
    eternal.hash(&mut populated_input);
    assert_ne!(legacy_input.0, populated_input.0);
}

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

// ---- Reanimator ------------------------------------------------------------------------

fn fill_graveyard(state: &mut GameState, player: PlayerId, count: usize) {
    for _ in 0..count {
        put(state, player, "Island", Zone::Graveyard);
    }
}

/// Plays on, attacking with `attackers`, until the blockers are declared;
/// returns the legal blockers of that decision.
fn attack_and_get_blockers(
    state: &mut GameState,
    attackers: &[ObjectId],
) -> Vec<(ObjectId, Vec<ObjectId>)> {
    for _ in 0..200 {
        match next(state) {
            Decision::DeclareAttackers { .. } => {
                act(state, Action::DeclareAttackers(attackers.to_vec()))
            }
            Decision::DeclareBlockers { legal_blockers, .. } => return legal_blockers,
            // No block was possible: the decision never came.
            Decision::CastSpellOrPass { .. }
                if matches!(
                    state.step,
                    Step::DeclareBlockers | Step::CombatDamage | Step::EndCombat | Step::Main2
                ) =>
            {
                return Vec::new()
            }
            Decision::CastSpellOrPass { .. } => act(state, Action::Pass),
            Decision::OrderTriggers { pending, .. } => {
                act(state, Action::OrderTriggers((0..pending.len()).collect()))
            }
            other => act(state, first_answer(other)),
        }
    }
    panic!("blockers were never declared");
}

#[test]
fn undiscovered_paradise_taps_for_any_color_and_returns_next_untap_step() {
    let mut state = islands();
    let paradise = put(&mut state, P0, "Undiscovered Paradise", Zone::Battlefield);
    let other = put(&mut state, P0, "Undiscovered Paradise", Zone::Battlefield);
    next(&mut state);
    for color in [ManaColor::W, ManaColor::R] {
        let mut probe = state.clone();
        act(
            &mut probe,
            Action::ActivateManaAbilityChoice(paradise, color),
        );
        assert_eq!(mana(&probe, P0, color), 1);
    }
    act(
        &mut state,
        Action::ActivateManaAbilityChoice(paradise, ManaColor::R),
    );
    assert_eq!(mana(&state, P0, ManaColor::R), 1);
    // The untapped one is not returned: only the tapped-for-mana land is.
    run_until(&mut state, |s| zone(s, paradise) == Zone::Hand, |_, _| None);
    assert!(hand_contains(&state, P0, paradise));
    assert_eq!(zone(&state, other), Zone::Battlefield);
}

#[test]
fn reflecting_pool_adds_types_other_lands_could_produce() {
    let mut state = islands();
    let pool_land = put(&mut state, P0, "Reflecting Pool", Zone::Battlefield);
    next(&mut state);
    assert!(engine::step(
        &mut state,
        Action::ActivateManaAbilityChoice(pool_land, ManaColor::G)
    )
    .is_err());
    put(&mut state, P0, "Treva's Ruins", Zone::Battlefield);
    put(&mut state, P1, "Swamp", Zone::Battlefield);
    next(&mut state);
    assert!(engine::step(
        &mut state,
        Action::ActivateManaAbilityChoice(pool_land, ManaColor::B)
    )
    .is_err());
    act(
        &mut state,
        Action::ActivateManaAbilityChoice(pool_land, ManaColor::U),
    );
    assert_eq!(mana(&state, P0, ManaColor::U), 1);

    // The planner can pay with it even while Treva's Ruins is tapped.
    let mut state = islands();
    let pool_land = put(&mut state, P0, "Reflecting Pool", Zone::Battlefield);
    let ruins = put(&mut state, P0, "Treva's Ruins", Zone::Battlefield);
    state.objects.get_mut(ruins).tapped = true;
    let study = put(&mut state, P0, "Careful Study", Zone::Hand);
    assert!(castable(&mut state, study));
    cast(&mut state, study, &[]);
    next(&mut state);
    assert!(state.objects.get(pool_land).tapped);
}

#[test]
fn avatar_of_woe_costs_six_less_with_ten_creature_cards_and_has_fear() {
    let mut state = islands();
    let avatar = put(&mut state, P0, "Avatar of Woe", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B, ManaColor::B], 0);
    assert!(!castable(&mut state, avatar), "nine creature cards");
    for _ in 0..5 {
        put(&mut state, P0, "Llanowar Elves", Zone::Graveyard);
    }
    for _ in 0..4 {
        put(&mut state, P1, "Llanowar Elves", Zone::Graveyard);
    }
    put(&mut state, P1, "Island", Zone::Graveyard);
    assert!(!castable(&mut state, avatar), "nine creature cards");
    put(&mut state, P1, "Llanowar Elves", Zone::Graveyard);
    assert!(castable(&mut state, avatar), "ten creature cards in all");
    cast(&mut state, avatar, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, avatar), Zone::Battlefield);
    assert_eq!(power_toughness(&state, avatar), (6, 5));

    // Fear: only black and artifact creatures block it.
    let mut state = islands();
    let avatar = put(&mut state, P0, "Avatar of Woe", Zone::Battlefield);
    let green = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    let black = put(&mut state, P1, "Hypnotic Specter", Zone::Battlefield);
    let artifact = put(&mut state, P1, "Triskelion", Zone::Battlefield);
    let blockers = attack_and_get_blockers(&mut state, &[avatar]);
    let can_block = |creature: ObjectId| {
        blockers
            .iter()
            .any(|(attacker, blockers)| *attacker == avatar && blockers.contains(&creature))
    };
    assert!(!can_block(green));
    assert!(can_block(black));
    assert!(can_block(artifact));
}

#[test]
fn avatar_of_woe_destroys_a_creature_that_cannot_regenerate() {
    let mut state = islands();
    let avatar = put(&mut state, P0, "Avatar of Woe", Zone::Battlefield);
    let victim = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    activate(&mut state, avatar, 0, &[Target::Object(victim)]);
    settled(&mut state);
    assert_eq!(zone(&state, victim), Zone::Graveyard);
    assert!(state.objects.get(avatar).tapped);
}

#[test]
fn exhume_lets_each_player_choose_a_creature_from_their_graveyard() {
    let mut state = islands();
    let exhume = put(&mut state, P0, "Exhume", Zone::Hand);
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Graveyard);
    let piledriver = put(&mut state, P0, "Goblin Piledriver", Zone::Graveyard);
    let specter = put(&mut state, P1, "Hypnotic Specter", Zone::Graveyard);
    let angel = put(&mut state, P1, "Exalted Angel", Zone::Graveyard);
    let spell = put(&mut state, P1, "Careful Study", Zone::Graveyard);
    pool(&mut state, P0, &[ManaColor::B], 1);
    cast(&mut state, exhume, &[]);
    let mut seats = Vec::new();
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseEffectTargets {
            legal_targets,
            player,
            ..
        } => {
            seats.push(player);
            Action::ChooseEffectTarget(legal_targets[1])
        }
        other => first_answer(other),
    });
    assert_eq!(seats, vec![P0, P1], "the active player chooses first");
    assert_eq!(zone(&state, piledriver), Zone::Battlefield);
    assert_eq!(zone(&state, angel), Zone::Battlefield);
    assert_eq!(zone(&state, elves), Zone::Graveyard);
    assert_eq!(zone(&state, specter), Zone::Graveyard);
    assert_eq!(zone(&state, spell), Zone::Graveyard);
    assert_eq!(state.objects.get(angel).controller, P1);

    // A lone creature card needs no choice, and an empty graveyard is fine.
    let mut state = islands();
    let exhume = put(&mut state, P0, "Exhume", Zone::Hand);
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Graveyard);
    pool(&mut state, P0, &[ManaColor::B], 1);
    cast(&mut state, exhume, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, elves), Zone::Battlefield);
}
#[test]
fn life_animates_lands_until_end_of_turn_and_death_reanimates_for_life() {
    let mut state = islands();
    let forest = put(&mut state, P0, "Forest", Zone::Battlefield);
    let island = put(&mut state, P0, "Island", Zone::Battlefield);
    let enemy = put(&mut state, P1, "Island", Zone::Battlefield);
    let card = put(&mut state, P0, "Life // Death", Zone::Hand);
    assert_eq!(def("Life // Death").mana_value, 3);
    assert_eq!(
        def("Life // Death").adventure.as_ref().map(|d| d.name),
        Some("Death")
    );
    pool(&mut state, P0, &[ManaColor::G], 0);
    cast_split_half(&mut state, card, 0, &[]);
    settled(&mut state);
    for land in [forest, island] {
        assert!(engine::object_has_type(&state, land, CardType::Creature));
        assert!(engine::object_has_type(&state, land, CardType::Land));
        assert!(!engine::object_has_type(&state, land, CardType::Artifact));
        assert_eq!(power_toughness(&state, land), (1, 1));
    }
    assert!(!engine::object_has_type(&state, enemy, CardType::Creature));
    assert_eq!(zone(&state, card), Zone::Graveyard);
    // They are lands again after the turn ends.
    run_until(&mut state, |s| s.active_player == P1, |_, _| None);
    assert!(!engine::object_has_type(&state, forest, CardType::Creature));

    let mut state = islands();
    let angel = put(&mut state, P0, "Exalted Angel", Zone::Graveyard);
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Graveyard);
    let card = put(&mut state, P0, "Life // Death", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 1);
    cast_split_half(&mut state, card, 1, &[Target::Object(angel)]);
    settled(&mut state);
    assert_eq!(zone(&state, angel), Zone::Battlefield);
    assert_eq!(zone(&state, elves), Zone::Graveyard);
    assert_eq!(
        state.players[0].life, 14,
        "six life for a mana value of six"
    );
    assert_eq!(zone(&state, card), Zone::Graveyard);
}

/// Casts one half of a split card: `half` 0 is the first, 1 the second.
fn cast_split_half(state: &mut GameState, card: ObjectId, half: usize, targets: &[Target]) {
    assert!(castable(state, card));
    act(state, Action::CastSpell(card));
    let mut targets = targets.iter();
    loop {
        match next(state) {
            Decision::ChooseSpellMode { .. } => act(state, Action::ChooseSpellMode(half as u8)),
            Decision::ChooseCastMode { options, .. } => act(
                state,
                Action::ChooseCastMode(options[half.min(options.len() - 1)]),
            ),
            Decision::ChooseTargets { legal_targets, .. } => {
                let target = targets.next().copied().unwrap_or(legal_targets[0]);
                assert!(legal_targets.contains(&target), "{target:?}");
                act(state, Action::ChooseTarget(target));
            }
            _ => return,
        }
    }
}

#[test]
fn petradon_exiles_two_lands_and_returns_them_when_it_leaves() {
    let mut state = islands();
    let forest = put(&mut state, P0, "Forest", Zone::Battlefield);
    let island = put(&mut state, P1, "Island", Zone::Battlefield);
    let keep = put(&mut state, P1, "Swamp", Zone::Battlefield);
    let petradon = put(&mut state, P0, "Petradon", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R, ManaColor::R], 6);
    cast(&mut state, petradon, &[]);
    let mut picks = vec![Target::Object(forest), Target::Object(island)].into_iter();
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseTargets { legal_targets, .. } => {
            let pick = picks.next().expect("two picks");
            assert!(legal_targets.contains(&pick), "{pick:?}");
            Action::ChooseTarget(pick)
        }
        other => first_answer(other),
    });
    assert_eq!(zone(&state, petradon), Zone::Battlefield);
    assert_eq!(zone(&state, forest), Zone::Exile);
    assert_eq!(zone(&state, island), Zone::Exile);
    assert_eq!(zone(&state, keep), Zone::Battlefield);
    assert_eq!(power_toughness(&state, petradon), (5, 6));
    pool(&mut state, P0, &[ManaColor::R], 0);
    activate(&mut state, petradon, 0, &[]);
    settled(&mut state);
    assert_eq!(power_toughness(&state, petradon), (6, 6));

    mtg_kernel::event::propose_and_commit(
        &mut state,
        mtg_kernel::event::ProposedEvent::zone_change(petradon, Zone::Graveyard),
    );
    settled(&mut state);
    assert_eq!(zone(&state, forest), Zone::Battlefield);
    assert_eq!(zone(&state, island), Zone::Battlefield);
    assert_eq!(state.objects.get(forest).controller, P0);
    assert_eq!(state.objects.get(island).controller, P1, "owner's control");
}

#[test]
fn phantom_nishoba_prevents_damage_for_a_counter_and_gains_life() {
    let mut state = islands();
    let nishoba = put(&mut state, P0, "Phantom Nishoba", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G, ManaColor::W], 5);
    cast(&mut state, nishoba, &[]);
    settled(&mut state);
    assert_eq!(power_toughness(&state, nishoba), (7, 7));
    assert!(keyword(&state, nishoba, Keywords::TRAMPLE));
    let triskelion = put(&mut state, P0, "Triskelion", Zone::Battlefield);
    state.objects.get_mut(triskelion).counters.plus1_plus1 = 3;
    activate(&mut state, triskelion, 0, &[Target::Object(nishoba)]);
    settled(&mut state);
    assert_eq!(state.objects.get(nishoba).damage, 0);
    assert_eq!(
        power_toughness(&state, nishoba),
        (6, 6),
        "one counter fewer"
    );
    assert_eq!(zone(&state, nishoba), Zone::Battlefield);

    // Dealing combat damage gains that much life.
    state.objects.get_mut(nishoba).summoning_sick = false;
    run_until(
        &mut state,
        |s| s.players[1].life < 20,
        |_, decision| match decision {
            Decision::DeclareAttackers { .. } => Some(Action::DeclareAttackers(vec![nishoba])),
            _ => None,
        },
    );
    settled(&mut state);
    assert_eq!(state.players[1].life, 14);
    assert_eq!(state.players[0].life, 26);
}

#[test]
fn putrid_imp_gets_threshold_boost_cannot_block_and_flies_for_a_discard() {
    let mut state = islands();
    let imp = put(&mut state, P0, "Putrid Imp", Zone::Battlefield);
    assert_eq!(power_toughness(&state, imp), (1, 1));
    fill_graveyard(&mut state, P0, 6);
    assert_eq!(power_toughness(&state, imp), (1, 1));
    fill_graveyard(&mut state, P0, 1);
    assert_eq!(power_toughness(&state, imp), (2, 2));

    // With threshold it cannot block; without, it can.
    for (cards, can_block) in [(7, false), (6, true)] {
        let mut state = islands();
        state.active_player = P1;
        state.priority_player = P1;
        let imp = put(&mut state, P0, "Putrid Imp", Zone::Battlefield);
        let attacker = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
        fill_graveyard(&mut state, P0, cards);
        let blockers = attack_and_get_blockers(&mut state, &[attacker]);
        let allowed = blockers.iter().any(|(_, list)| list.contains(&imp));
        assert_eq!(allowed, can_block, "{cards} cards");
    }

    let mut state = islands();
    let imp = put(&mut state, P0, "Putrid Imp", Zone::Battlefield);
    put(&mut state, P0, "Island", Zone::Hand);
    assert!(!keyword(&state, imp, Keywords::FLYING));
    activate(&mut state, imp, 0, &[]);
    settle_with(&mut state, |_, decision| first_answer(decision));
    assert!(keyword(&state, imp, Keywords::FLYING));
    assert!(state.players[0].hand.is_empty());
}
// ---- Full English Breakfast ------------------------------------------------------------

#[test]
fn hermit_druid_reveals_until_a_basic_land() {
    let mut state = game(&["Careful Study"; 20]);
    let forest = put(&mut state, P0, "Forest", Zone::Library);
    let first = put(&mut state, P0, "Careful Study", Zone::Library);
    let second = put(&mut state, P0, "Careful Study", Zone::Library);
    let druid = put(&mut state, P0, "Hermit Druid", Zone::Battlefield);
    pool(&mut state, P0, &[ManaColor::G], 0);
    activate(&mut state, druid, 0, &[]);
    settled(&mut state);
    assert!(hand_contains(&state, P0, forest));
    assert_eq!(zone(&state, first), Zone::Graveyard);
    assert_eq!(zone(&state, second), Zone::Graveyard);
    assert_eq!(state.players[0].graveyard.len(), 2);
    assert!(state.objects.get(druid).tapped);

    // No basic land: the whole library is revealed and put into the graveyard.
    let mut state = game(&["Careful Study"; 20]);
    let druid = put(&mut state, P0, "Hermit Druid", Zone::Battlefield);
    pool(&mut state, P0, &[ManaColor::G], 0);
    activate(&mut state, druid, 0, &[]);
    settled(&mut state);
    assert!(state.players[0].library.is_empty());
    assert_eq!(state.players[0].graveyard.len(), 20);
    assert!(state.players[0].hand.is_empty());
}

#[test]
fn krosan_reclamation_shuffles_up_to_two_cards_and_has_flashback() {
    let mut state = islands();
    let reclamation = put(&mut state, P0, "Krosan Reclamation", Zone::Hand);
    let one = put(&mut state, P0, "Llanowar Elves", Zone::Graveyard);
    let two = put(&mut state, P0, "Goblin Piledriver", Zone::Graveyard);
    let three = put(&mut state, P0, "Exalted Angel", Zone::Graveyard);
    let library = state.players[0].library.len();
    pool(&mut state, P0, &[ManaColor::G], 1);
    cast(
        &mut state,
        reclamation,
        &[Target::Player(P0), Target::Object(one), Target::Object(two)],
    );
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseTargets {
            can_finish: true, ..
        } => Action::FinishEffectSelection,
        other => first_answer(other),
    });
    assert_eq!(zone(&state, one), Zone::Library);
    assert_eq!(zone(&state, two), Zone::Library);
    assert_eq!(zone(&state, three), Zone::Graveyard);
    assert_eq!(state.players[0].library.len(), library + 2);
    assert_eq!(zone(&state, reclamation), Zone::Graveyard);

    // Flashback {1}{G}: with no target cards, only the player.
    pool(&mut state, P0, &[ManaColor::G], 1);
    assert!(castable(&mut state, reclamation));
    act(&mut state, Action::CastSpell(reclamation));
    let mut chose_player = false;
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseTargets { legal_targets, .. } if !chose_player => {
            chose_player = true;
            assert!(legal_targets.contains(&Target::Player(P0)));
            Action::ChooseTarget(Target::Player(P0))
        }
        Decision::ChooseTargets {
            can_finish: true, ..
        } => Action::FinishEffectSelection,
        other => first_answer(other),
    });
    assert_eq!(zone(&state, reclamation), Zone::Exile);
}

#[test]
fn karmic_guide_reanimates_and_has_echo() {
    let mut state = islands();
    let guide = put(&mut state, P0, "Karmic Guide", Zone::Hand);
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Graveyard);
    put(&mut state, P0, "Island", Zone::Graveyard);
    pool(&mut state, P0, &[ManaColor::W, ManaColor::W], 3);
    cast(&mut state, guide, &[]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseTargets { legal_targets, .. } => {
            assert_eq!(legal_targets, vec![Target::Object(elves)]);
            Action::ChooseTarget(Target::Object(elves))
        }
        other => first_answer(other),
    });
    assert_eq!(zone(&state, guide), Zone::Battlefield);
    assert_eq!(zone(&state, elves), Zone::Battlefield);
    assert!(keyword(&state, guide, Keywords::FLYING));
    let specter = put(&mut state, P1, "Hypnotic Specter", Zone::Battlefield);
    assert!(engine::damage_is_prevented_by_protection(
        &state,
        specter,
        Target::Object(guide)
    ));

    // Unpaid echo sacrifices it in its controller's next upkeep.
    let turn = state.turn;
    let mut unpaid = state.clone();
    run_until(
        &mut unpaid,
        |s| s.turn > turn && s.active_player == P0 && s.step == Step::Main1,
        |_, decision| match decision {
            Decision::ChooseEffectOption { .. } => Some(Action::ChooseEffectOption(1)),
            _ => None,
        },
    );
    assert_eq!(zone(&unpaid, guide), Zone::Graveyard);
    // Paid echo ({3}{W}{W}) keeps it.
    for _ in 0..3 {
        put(&mut state, P0, "Island", Zone::Battlefield);
    }
    for _ in 0..2 {
        put(&mut state, P0, "Plains", Zone::Battlefield);
    }
    run_until(
        &mut state,
        |s| s.turn > turn && s.active_player == P0 && s.step == Step::Main1,
        |_, decision| match decision {
            Decision::ChooseEffectOption { .. } => Some(Action::ChooseEffectOption(0)),
            _ => None,
        },
    );
    assert_eq!(zone(&state, guide), Zone::Battlefield);
}

#[test]
fn battlefield_scrounger_needs_threshold_and_moves_three_cards_to_the_library() {
    let mut state = islands();
    let scrounger = put(&mut state, P0, "Battlefield Scrounger", Zone::Battlefield);
    assert_eq!(power_toughness(&state, scrounger), (3, 3));
    assert!(def("Battlefield Scrounger")
        .subtypes
        .contains(&Subtype::Centaur));
    fill_graveyard(&mut state, P0, 6);
    assert!(!activatable(&mut state, scrounger, 0), "six cards");
    fill_graveyard(&mut state, P0, 1);
    let library = state.players[0].library.len();
    activate(&mut state, scrounger, 0, &[]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseCostTargets { candidates, .. } => Action::ChooseCostTarget(candidates[0]),
        other => first_answer(other),
    });
    assert_eq!(power_toughness(&state, scrounger), (6, 6));
    assert_eq!(state.players[0].graveyard.len(), 4);
    assert_eq!(state.players[0].library.len(), library + 3);
    // Once each turn.
    fill_graveyard(&mut state, P0, 3);
    assert!(!activatable(&mut state, scrounger, 0), "once each turn");
}

#[test]
fn phyrexian_devourer_grows_by_the_exiled_card_and_is_sacrificed_at_seven_power() {
    let mut state = islands();
    let devourer = put(&mut state, P0, "Phyrexian Devourer", Zone::Battlefield);
    let cheap = put(&mut state, P0, "Unearth", Zone::Library);
    activate(&mut state, devourer, 0, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, cheap), Zone::Exile);
    assert_eq!(power_toughness(&state, devourer), (2, 2));

    let big = put(&mut state, P0, "Triskelion", Zone::Library);
    activate(&mut state, devourer, 0, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, big), Zone::Exile);
    assert_eq!(zone(&state, devourer), Zone::Graveyard, "power 8");

    // A land exiled adds nothing.
    let mut state = islands();
    let devourer = put(&mut state, P0, "Phyrexian Devourer", Zone::Battlefield);
    activate(&mut state, devourer, 0, &[]);
    settled(&mut state);
    assert_eq!(power_toughness(&state, devourer), (1, 1));
    assert_eq!(state.players[0].library.len(), 19);
}

// ---- Fix round (review findings on PR #565) -------------------------------------------

/// A: two blockers deal combat damage to Phantom Nishoba at once, and only
/// one counter comes off for the batch.
#[test]
fn phantom_nishoba_loses_one_counter_per_batch_of_simultaneous_damage() {
    let mut state = islands();
    let nishoba = put(&mut state, P0, "Phantom Nishoba", Zone::Battlefield);
    state.objects.get_mut(nishoba).counters.plus1_plus1 = 7;
    let first = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    let second = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    run_until(
        &mut state,
        |s| s.step == Step::EndCombat,
        |_, decision| match decision {
            Decision::DeclareAttackers { .. } => Some(Action::DeclareAttackers(vec![nishoba])),
            Decision::DeclareBlockers { .. } => Some(Action::DeclareBlockers(vec![
                (first, nishoba),
                (second, nishoba),
            ])),
            _ => None,
        },
    );
    assert_eq!(state.objects.get(nishoba).damage, 0);
    assert_eq!(
        state.objects.get(nishoba).counters.plus1_plus1,
        6,
        "one counter for the whole batch"
    );
}

/// A: damage that cannot be prevented is dealt, and the prevention effect
/// still removes its counter (615.12).
#[test]
fn phantom_nishoba_takes_unpreventable_damage_and_still_loses_a_counter() {
    let mut state = islands();
    let nishoba = put(&mut state, P0, "Phantom Nishoba", Zone::Battlefield);
    state.objects.get_mut(nishoba).counters.plus1_plus1 = 7;
    state
        .engine
        .until_end_of_turn
        .push(engine::UntilEndOfTurnEffect::DamageCannotBePrevented {
            timestamp: 1_000_000,
            duration: engine::EffectDuration::EndOfTurn,
        });
    let triskelion = put(&mut state, P0, "Triskelion", Zone::Battlefield);
    state.objects.get_mut(triskelion).counters.plus1_plus1 = 3;
    activate(&mut state, triskelion, 0, &[Target::Object(nishoba)]);
    settled(&mut state);
    assert_eq!(state.objects.get(nishoba).damage, 1, "dealt");
    assert_eq!(state.objects.get(nishoba).counters.plus1_plus1, 6);
}

/// A: without its abilities (Dress Down) Phantom Nishoba prevents nothing
/// and gains no life from the damage it deals.
#[test]
fn phantom_nishoba_without_abilities_neither_prevents_nor_gains_life() {
    let mut state = islands();
    let dress_down = put(&mut state, P0, "Dress Down", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 1);
    cast(&mut state, dress_down, &[]);
    settled(&mut state);
    let nishoba = put(&mut state, P0, "Phantom Nishoba", Zone::Battlefield);
    state.objects.get_mut(nishoba).counters.plus1_plus1 = 7;
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 0);
    cast(&mut state, bolt, &[Target::Object(nishoba)]);
    settled(&mut state);
    assert_eq!(state.objects.get(nishoba).damage, 3, "not prevented");
    assert_eq!(state.objects.get(nishoba).counters.plus1_plus1, 7);

    state.objects.get_mut(nishoba).damage = 0;
    run_until(
        &mut state,
        |s| s.players[1].life < 20,
        |_, decision| match decision {
            Decision::DeclareAttackers { .. } => Some(Action::DeclareAttackers(vec![nishoba])),
            _ => None,
        },
    );
    settled(&mut state);
    assert_eq!(state.players[1].life, 13);
    assert_eq!(state.players[0].life, 20, "no life gain without abilities");
}

#[test]
fn nishoba_life_trigger_uses_damage_time_abilities_even_after_lethal_damage() {
    use mtg_kernel::event::{self, ProposedEvent};

    for suppressed in [false, true] {
        let mut state = islands();
        if suppressed {
            let dress_down = put(&mut state, P0, "Dress Down", Zone::Hand);
            pool(&mut state, P0, &[ManaColor::U], 1);
            cast(&mut state, dress_down, &[]);
            settled(&mut state);
        }
        let nishoba = put(&mut state, P0, "Phantom Nishoba", Zone::Battlefield);
        state.objects.get_mut(nishoba).counters.plus1_plus1 = 7;
        let other = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
        state
            .engine
            .until_end_of_turn
            .push(engine::UntilEndOfTurnEffect::DamageCannotBePrevented {
                timestamp: 1_000_000,
                duration: engine::EffectDuration::EndOfTurn,
            });
        event::propose_and_commit_batch(
            &mut state,
            vec![
                ProposedEvent::damage(nishoba, Target::Player(P1), 7),
                ProposedEvent::damage(other, Target::Object(nishoba), 7),
            ],
        );
        // The inter-wave combat SBA path can remove the source before the
        // pending damage triggers are collected.
        mtg_kernel::trigger::sba_fixed_point(&mut state);
        assert_eq!(state.objects.get(nishoba).zone, Zone::Graveyard);
        let triggers = mtg_kernel::trigger::collect_and_process(&mut state);
        state.engine.pending_triggers.extend(triggers);
        settled(&mut state);
        assert_eq!(state.players[P1.index()].life, 13);
        assert_eq!(state.players[P0.index()].life, if suppressed { 20 } else { 27 });
    }
}

/// B: with Dress Down on the battlefield a creature enters without its
/// "enters with counters" replacement (614.12).
#[test]
fn entry_counters_are_not_applied_while_abilities_are_removed() {
    let mut state = islands();
    let dress_down = put(&mut state, P0, "Dress Down", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 1);
    cast(&mut state, dress_down, &[]);
    settled(&mut state);
    let triskelion = put(&mut state, P0, "Triskelion", Zone::Hand);
    pool(&mut state, P0, &[], 6);
    cast(&mut state, triskelion, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, triskelion), Zone::Battlefield);
    assert_eq!(state.objects.get(triskelion).counters.plus1_plus1, 0);
    let nishoba = put(&mut state, P0, "Phantom Nishoba", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G, ManaColor::W], 5);
    cast(&mut state, nishoba, &[]);
    settled(&mut state);
    // Without its seven counters the 0/0 dies.
    assert_eq!(zone(&state, nishoba), Zone::Graveyard);
}

fn top_trigger(state: &GameState) -> Option<Target> {
    state
        .stack
        .last()
        .filter(|item| item.kind == mtg_kernel::state::StackItemKind::TriggeredAbility)
        .map(|item| Target::StackItem(item.v4.stack_item_id))
}

/// C: a state trigger triggers again once its previous instance left the
/// stack (603.8): a Stifled sacrifice trigger is replaced while the power
/// stays at 7 or more.
#[test]
fn phyrexian_devourer_triggers_again_after_its_trigger_was_countered() {
    let mut state = islands();
    let devourer = put(&mut state, P0, "Phyrexian Devourer", Zone::Battlefield);
    let big = put(&mut state, P0, "Triskelion", Zone::Library);
    let stifle = put(&mut state, P0, "Stifle", Zone::Hand);
    activate(&mut state, devourer, 0, &[]);
    run_until(&mut state, |s| top_trigger(s).is_some(), |_, _| None);
    assert_eq!(zone(&state, big), Zone::Exile);
    let trigger = top_trigger(&state).unwrap();
    pool(&mut state, P0, &[ManaColor::U], 0);
    cast(&mut state, stifle, &[trigger]);
    settled(&mut state);
    assert_eq!(zone(&state, stifle), Zone::Graveyard);
    assert_eq!(
        zone(&state, devourer),
        Zone::Graveyard,
        "the second instance sacrificed it"
    );
}

/// D: Palinchron's bounce does nothing if it left the battlefield first.
#[test]
fn palinchron_bounce_does_nothing_after_it_left_the_battlefield() {
    let mut state = islands();
    let palinchron = put(&mut state, P0, "Palinchron", Zone::Battlefield);
    pool(&mut state, P0, &[ManaColor::U, ManaColor::U], 2);
    activate(&mut state, palinchron, 0, &[]);
    assert!(matches!(next(&mut state), Decision::CastSpellOrPass { .. }));
    assert!(!state.stack.is_empty());
    mtg_kernel::event::propose_and_commit(
        &mut state,
        mtg_kernel::event::ProposedEvent::zone_change(palinchron, Zone::Graveyard),
    );
    settled(&mut state);
    assert_eq!(zone(&state, palinchron), Zone::Graveyard);
    assert!(!hand_contains(&state, P0, palinchron));
}

/// E: Reflecting Pool reads what each other land could produce now: a land
/// changed to a Swamp by Vision Charm gives {B} and no longer {U}.
#[test]
fn reflecting_pool_follows_a_land_type_change() {
    let mut state = islands();
    let pool_land = put(&mut state, P0, "Reflecting Pool", Zone::Battlefield);
    put(&mut state, P0, "Island", Zone::Battlefield);
    let charm = put(&mut state, P0, "Vision Charm", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 0);
    act(&mut state, Action::CastSpell(charm));
    let mut option = 0;
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseSpellMode { .. } => Action::ChooseSpellMode(1),
        Decision::ChooseEffectOption { .. } => {
            option += 1;
            // The Island type, then the Swamp type.
            Action::ChooseEffectOption(if option == 1 { 1 } else { 2 })
        }
        other => first_answer(other),
    });
    next(&mut state);
    // The Island is a Swamp now: the Pool has the one color, black.
    act(&mut state, Action::ActivateManaAbility(pool_land));
    assert_eq!(mana(&state, P0, ManaColor::B), 1);
    assert_eq!(mana(&state, P0, ManaColor::U), 0);
}

/// E: "could produce" ignores activation conditions (Tainted Field without a
/// Swamp still counts for {W} and {B}).
#[test]
fn reflecting_pool_ignores_activation_conditions_of_other_lands() {
    let mut state = islands();
    let pool_land = put(&mut state, P0, "Reflecting Pool", Zone::Battlefield);
    put(&mut state, P0, "Tainted Field", Zone::Battlefield);
    next(&mut state);
    assert!(engine::step(
        &mut state,
        Action::ActivateManaAbilityChoice(pool_land, ManaColor::R)
    )
    .is_err());
    for color in [ManaColor::W, ManaColor::B] {
        let mut probe = state.clone();
        act(
            &mut probe,
            Action::ActivateManaAbilityChoice(pool_land, color),
        );
        assert_eq!(mana(&probe, P0, color), 1);
    }
}

/// F: an untap step skipped by Stasis does not return Undiscovered
/// Paradise; the next untap step that happens does.
#[test]
fn undiscovered_paradise_stays_through_a_skipped_untap_step() {
    let mut state = islands();
    let paradise = put(&mut state, P0, "Undiscovered Paradise", Zone::Battlefield);
    let stasis = put(&mut state, P0, "Stasis", Zone::Battlefield);
    next(&mut state);
    act(
        &mut state,
        Action::ActivateManaAbilityChoice(paradise, ManaColor::R),
    );
    let turn = state.turn;
    run_until(
        &mut state,
        |s| s.turn > turn && s.active_player == P0 && s.step == Step::Upkeep,
        |_, decision| match decision {
            Decision::ChooseEffectBoolean { .. } => Some(Action::ChooseEffectBoolean(false)),
            _ => None,
        },
    );
    assert_eq!(zone(&state, paradise), Zone::Battlefield, "untap skipped");
    assert!(state.objects.get(paradise).tapped);
    mtg_kernel::event::propose_and_commit(
        &mut state,
        mtg_kernel::event::ProposedEvent::zone_change(stasis, Zone::Graveyard),
    );
    run_until(&mut state, |s| zone(s, paradise) == Zone::Hand, |_, _| None);
    assert!(hand_contains(&state, P0, paradise));
}
