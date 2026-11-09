//! Sideboard wave S1a: native rules tests for the Legacy sideboard cards
//! that complete the most decks per card (graveyard hate, mass removal,
//! artifact and enchantment hate, taxing statics).
#![cfg(feature = "eternal-fixtures")]

use mtg_kernel::card_def::{
    card_id_by_name, CardCapability, CardDef, CardType, Keywords, Subtype, CARD_DEFS,
};
use mtg_kernel::engine::{self, Action, Decision, OptionalCostChoice};
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::ManaColor;
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Target, Zone};

const P0: PlayerId = PlayerId::P0;
const P1: PlayerId = PlayerId::P1;

const S1A_CARDS: [&str; 24] = [
    "Brotherhood's End",
    "Carpet of Flowers",
    "Celestial Purge",
    "Choke",
    "Containment Priest",
    "Defense Grid",
    "Disruptor Flute",
    "Dread of Night",
    "Force of Vigor",
    "Haywire Mite",
    "Lavinia, Azorius Renegade",
    "Leyline of the Void",
    "Massacre",
    "Meltdown",
    "Pyroclasm",
    "Rest in Peace",
    "Rip Apart",
    "Rough // Tumble",
    "Serenity",
    "Surgical Extraction",
    "Swan Song",
    "Tormod's Crypt",
    "Twinshot Sniper",
    "Warping Wail",
];

fn def(name: &str) -> &'static CardDef {
    &CARD_DEFS[id(name) as usize]
}

fn id(name: &str) -> u16 {
    card_id_by_name(name).unwrap_or_else(|| panic!("{name}"))
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
        0x5031_4200_0001,
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

fn mana(state: &GameState, player: PlayerId, color: ManaColor) -> u8 {
    state.players[player.index()].mana_pool[color.pool_index()]
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

/// Answers the X announcement of a pending cast with `x`.
fn announce_x(state: &mut GameState, x: u16) {
    match next(state) {
        Decision::ChooseEffectOption { option_count, .. } => {
            assert!(x < option_count, "X {x} of {option_count}");
            act(state, Action::ChooseEffectOption(x));
        }
        other => panic!("expected the X announcement, got {other:?}"),
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

/// Plays to P0's next turn's first main phase, answering upkeep choices with
/// `answer`.
fn to_next_turn(
    state: &mut GameState,
    answer: impl FnMut(&GameState, &Decision) -> Option<Action>,
) {
    let turn = state.turn;
    run_until(
        state,
        |state| state.turn > turn && state.active_player == P0 && state.step == Step::Main1,
        answer,
    );
}

/// Moves `object` to its owner's graveyard, as if destroyed.
fn to_graveyard(state: &mut GameState, object: ObjectId) {
    event::propose_and_commit(state, ProposedEvent::zone_change(object, Zone::Graveyard));
}

/// Puts `object` onto the battlefield under its owner's control, as if an
/// effect did.
fn enter(state: &mut GameState, object: ObjectId) {
    event::propose_and_commit(state, ProposedEvent::zone_change(object, Zone::Battlefield));
}

/// A game at its very beginning: turn 1's untap step, nothing in hand.
fn fresh_game() -> GameState {
    let ids: Vec<u16> = ["Island"; 20].iter().map(|name| id(name)).collect();
    GameState::new_from_libraries(
        &ids,
        &ids,
        |id| CARD_DEFS[id as usize].object_name.into(),
        0x5331_4100_0001,
    )
}

/// `active` holds priority in a first main phase on their own turn.
fn set_active(state: &mut GameState, active: PlayerId, priority: PlayerId) {
    state.active_player = active;
    state.priority_player = priority;
    state.step = Step::Main1;
}

/// The (letter group, name) option indices of `name` in a card-name choice
/// made by first letter, then from the names with that letter.
fn name_choice(name: &str) -> (u16, u16) {
    let mut seen: Vec<&str> = Vec::new();
    let mut groups: Vec<(char, Vec<&str>)> = Vec::new();
    for def in CARD_DEFS.iter() {
        if def.is_token || !def.is_executable() || seen.contains(&def.object_name) {
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
    for (group, (_, names)) in groups.iter().enumerate() {
        if let Some(index) = names.iter().position(|candidate| *candidate == name) {
            return (group as u16, index as u16);
        }
    }
    panic!("{name} is not nameable");
}

/// Casts the modal `spell`, choosing `mode` and answering each target
/// prompt from `targets` in order.
fn cast_modal(state: &mut GameState, spell: ObjectId, mode: u8, targets: &[Target]) {
    assert!(
        castable(state, spell),
        "{} not castable",
        state.objects.get(spell).name
    );
    act(state, Action::CastSpell(spell));
    let mut targets = targets.iter().copied();
    loop {
        match next(state) {
            Decision::ChooseSpellMode { legal_modes, .. } => {
                assert!(legal_modes.contains(&mode), "{mode} not in {legal_modes:?}");
                act(state, Action::ChooseSpellMode(mode));
            }
            Decision::ChooseTargets { legal_targets, .. } => {
                let target = targets.next().expect("a target to choose");
                assert!(
                    legal_targets.contains(&target),
                    "{target:?} not in {legal_targets:?}"
                );
                act(state, Action::ChooseTarget(target));
            }
            _ => break,
        }
    }
}

/// The active player passes priority with a spell on the stack.
fn pass_priority(state: &mut GameState) {
    next(state);
    act(state, Action::Pass);
}

fn graveyard_count(state: &GameState, player: PlayerId) -> usize {
    state.players[player.index()].graveyard.len()
}

// ---- Characteristics -----------------------------------------------------------

#[test]
fn s1a_cards_are_full_with_their_printed_characteristics() {
    for name in S1A_CARDS {
        assert_eq!(def(name).capability, CardCapability::Full, "{name}");
        assert!(!def(name).is_token, "{name}");
    }
    let bird = def("Swan Song Bird Token");
    assert!(bird.is_token && bird.capability == CardCapability::Full);
    assert!(bird.keywords.has(Keywords::FLYING));
    assert_eq!((bird.power, bird.toughness), (Some(2), Some(2)));
    assert_eq!(bird.subtypes, &[Subtype::Bird]);
    let sniper = def("Twinshot Sniper");
    assert_eq!(sniper.types, &[CardType::Artifact, CardType::Creature]);
    assert_eq!(sniper.subtypes, &[Subtype::Goblin, Subtype::Archer]);
    assert!(sniper.keywords.has(Keywords::REACH));
    assert_eq!((sniper.power, sniper.toughness), (Some(2), Some(3)));
    assert_eq!(sniper.activated_abilities.len(), 1, "channel");
    let mite = def("Haywire Mite");
    assert_eq!(mite.types, &[CardType::Artifact, CardType::Creature]);
    assert_eq!(mite.subtypes, &[Subtype::Insect]);
    assert_eq!((mite.power, mite.toughness), (Some(1), Some(1)));
    let priest = def("Containment Priest");
    assert!(priest.keywords.has(Keywords::FLASH));
    assert_eq!(priest.subtypes, &[Subtype::Human, Subtype::Cleric]);
    assert!(def("Disruptor Flute").keywords.has(Keywords::FLASH));
    assert_eq!(def("Meltdown").cost.x_count, 1);
    assert_eq!(
        def("Rough // Tumble")
            .adventure
            .as_ref()
            .map(|half| half.name),
        Some("Tumble")
    );
    for name in ["Rip Apart", "Brotherhood's End"] {
        assert!(def(name).mode2.is_some(), "{name}");
    }
    let wail = def("Warping Wail");
    assert!(wail.mode2.is_some() && wail.mode3.is_some());
    assert!(def("Massacre").alt_cost.is_some() && def("Force of Vigor").alt_cost.is_some());
}

// ---- Spells --------------------------------------------------------------------

#[test]
fn pyroclasm_deals_two_damage_to_each_creature() {
    let mut state = islands();
    let mine = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    let theirs = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    let sovereign = put(&mut state, P1, "Vampire Sovereign", Zone::Battlefield);
    let pyroclasm = put(&mut state, P0, "Pyroclasm", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 1);
    cast(&mut state, pyroclasm, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, mine), Zone::Graveyard);
    assert_eq!(zone(&state, theirs), Zone::Graveyard);
    assert_eq!(zone(&state, sovereign), Zone::Battlefield);
    assert_eq!(state.objects.get(sovereign).damage, 2);
    assert_eq!(zone(&state, pyroclasm), Zone::Graveyard);
}

#[test]
fn rip_apart_damages_a_creature_or_planeswalker_or_destroys_an_artifact_or_enchantment() {
    let mut state = islands();
    let elves = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    let ring = put(&mut state, P1, "Sol Ring", Zone::Battlefield);
    let rip = put(&mut state, P0, "Rip Apart", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R, ManaColor::W], 0);
    cast_modal(&mut state, rip, 0, &[Target::Object(elves)]);
    settled(&mut state);
    assert_eq!(zone(&state, elves), Zone::Graveyard);
    assert_eq!(
        zone(&state, ring),
        Zone::Battlefield,
        "the artifact is no target"
    );

    for (name, kind) in [("Sol Ring", "artifact"), ("Choke", "enchantment")] {
        let mut state = islands();
        let elves = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
        let permanent = put(&mut state, P1, name, Zone::Battlefield);
        let rip = put(&mut state, P0, "Rip Apart", Zone::Hand);
        pool(&mut state, P0, &[ManaColor::R, ManaColor::W], 0);
        cast_modal(&mut state, rip, 1, &[Target::Object(permanent)]);
        settled(&mut state);
        assert_eq!(zone(&state, permanent), Zone::Graveyard, "{kind}");
        assert_eq!(zone(&state, elves), Zone::Battlefield, "{kind}");
    }

    // A planeswalker is a legal target of the damage mode.
    let mut state = islands();
    let dack = put(&mut state, P1, "Dack Fayden", Zone::Hand);
    enter(&mut state, dack);
    let rip = put(&mut state, P0, "Rip Apart", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R, ManaColor::W], 0);
    cast_modal(&mut state, rip, 0, &[Target::Object(dack)]);
    settled(&mut state);
    assert_eq!(zone(&state, dack), Zone::Graveyard, "3 damage to 3 loyalty");
}

#[test]
fn tormods_crypt_exiles_a_players_graveyard() {
    let mut state = islands();
    let crypt = put(&mut state, P0, "Tormod's Crypt", Zone::Battlefield);
    let theirs = put(&mut state, P1, "Island", Zone::Graveyard);
    put(&mut state, P1, "Llanowar Elves", Zone::Graveyard);
    let mine = put(&mut state, P0, "Island", Zone::Graveyard);
    activate(&mut state, crypt, 0, &[Target::Player(P1)]);
    settled(&mut state);
    assert_eq!(zone(&state, crypt), Zone::Graveyard, "sacrificed");
    assert_eq!(zone(&state, theirs), Zone::Exile);
    assert_eq!(graveyard_count(&state, P1), 0);
    assert_eq!(
        zone(&state, mine),
        Zone::Graveyard,
        "only the target player's"
    );
}

#[test]
fn twinshot_sniper_deals_two_when_it_enters_or_when_channeled() {
    let mut state = islands();
    let sniper = put(&mut state, P0, "Twinshot Sniper", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 3);
    cast(&mut state, sniper, &[]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseTargets { .. } => Action::ChooseTarget(Target::Player(P1)),
        other => first_answer(other),
    });
    assert_eq!(state.players[1].life, 18);
    assert_eq!(zone(&state, sniper), Zone::Battlefield);
    assert!(keyword(&state, sniper, Keywords::REACH));

    let mut state = islands();
    let elves = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    let sniper = put(&mut state, P0, "Twinshot Sniper", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 1);
    activate(&mut state, sniper, 0, &[Target::Object(elves)]);
    settled(&mut state);
    assert_eq!(zone(&state, elves), Zone::Graveyard);
    assert_eq!(zone(&state, sniper), Zone::Graveyard, "discarded, no entry");
    assert_eq!(state.players[1].life, 20);
}

#[test]
fn haywire_mite_gains_two_on_dying_and_exiles_a_noncreature_artifact_or_enchantment() {
    let mut state = islands();
    let mite = put(&mut state, P0, "Haywire Mite", Zone::Battlefield);
    to_graveyard(&mut state, mite);
    settled(&mut state);
    assert_eq!(state.players[0].life, 22);

    let mut state = islands();
    let mite = put(&mut state, P0, "Haywire Mite", Zone::Battlefield);
    let ring = put(&mut state, P1, "Sol Ring", Zone::Battlefield);
    let choke = put(&mut state, P1, "Choke", Zone::Battlefield);
    pool(&mut state, P0, &[ManaColor::G], 0);
    activate(&mut state, mite, 0, &[Target::Object(ring)]);
    settled(&mut state);
    assert_eq!(zone(&state, ring), Zone::Exile);
    assert_eq!(zone(&state, choke), Zone::Battlefield);
    assert_eq!(zone(&state, mite), Zone::Graveyard, "sacrificed");
    assert_eq!(state.players[0].life, 22, "and it died");

    // An artifact creature (another Mite) is no target.
    let mut state = islands();
    let mite = put(&mut state, P0, "Haywire Mite", Zone::Battlefield);
    put(&mut state, P1, "Haywire Mite", Zone::Battlefield);
    pool(&mut state, P0, &[ManaColor::G], 0);
    assert!(!activatable(&mut state, mite, 0));
    let choke = put(&mut state, P1, "Choke", Zone::Battlefield);
    assert!(activatable(&mut state, mite, 0));
    activate(&mut state, mite, 0, &[Target::Object(choke)]);
    settled(&mut state);
    assert_eq!(zone(&state, choke), Zone::Exile);
}

#[test]
fn serenity_destroys_every_artifact_and_enchantment_at_its_controllers_upkeep() {
    let mut state = islands();
    let serenity = put(&mut state, P0, "Serenity", Zone::Battlefield);
    let ring = put(&mut state, P1, "Sol Ring", Zone::Battlefield);
    let choke = put(&mut state, P1, "Choke", Zone::Battlefield);
    let mite = put(&mut state, P0, "Haywire Mite", Zone::Battlefield);
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    to_next_turn(&mut state, |_, _| None);
    assert_eq!(zone(&state, serenity), Zone::Graveyard, "itself included");
    assert_eq!(zone(&state, ring), Zone::Graveyard);
    assert_eq!(zone(&state, choke), Zone::Graveyard);
    assert_eq!(zone(&state, mite), Zone::Graveyard, "an artifact creature");
    assert_eq!(zone(&state, elves), Zone::Battlefield);
}

#[test]
fn celestial_purge_exiles_only_a_black_or_red_permanent() {
    let mut state = islands();
    let voidwalker = put(&mut state, P1, "Dauthi Voidwalker", Zone::Battlefield);
    let swiftspear = put(&mut state, P1, "Monastery Swiftspear", Zone::Battlefield);
    let inspector = put(&mut state, P1, "Thraben Inspector", Zone::Battlefield);
    let ring = put(&mut state, P1, "Sol Ring", Zone::Battlefield);
    let purge = put(&mut state, P0, "Celestial Purge", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W], 1);
    act(&mut state, Action::CastSpell(purge));
    match next(&mut state) {
        Decision::ChooseTargets { legal_targets, .. } => {
            assert!(legal_targets.contains(&Target::Object(voidwalker)));
            assert!(legal_targets.contains(&Target::Object(swiftspear)));
            assert!(!legal_targets.contains(&Target::Object(inspector)));
            assert!(!legal_targets.contains(&Target::Object(ring)));
        }
        other => panic!("{other:?}"),
    }
    act(&mut state, Action::ChooseTarget(Target::Object(swiftspear)));
    settled(&mut state);
    assert_eq!(zone(&state, swiftspear), Zone::Exile);
    assert_eq!(zone(&state, voidwalker), Zone::Battlefield);
}

#[test]
fn brotherhoods_end_damages_creatures_and_planeswalkers_or_destroys_cheap_artifacts() {
    let mut state = islands();
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    let sovereign = put(&mut state, P1, "Vampire Sovereign", Zone::Battlefield);
    let dack = put(&mut state, P1, "Dack Fayden", Zone::Hand);
    enter(&mut state, dack);
    let ring = put(&mut state, P0, "Sol Ring", Zone::Battlefield);
    let end = put(&mut state, P0, "Brotherhood's End", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R, ManaColor::R], 1);
    cast_modal(&mut state, end, 0, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, elves), Zone::Graveyard);
    assert_eq!(zone(&state, sovereign), Zone::Battlefield);
    assert_eq!(state.objects.get(sovereign).damage, 3);
    assert_eq!(zone(&state, dack), Zone::Graveyard);
    assert_eq!(zone(&state, ring), Zone::Battlefield);

    let mut state = islands();
    let ring = put(&mut state, P0, "Sol Ring", Zone::Battlefield);
    let trinisphere = put(&mut state, P1, "Trinisphere", Zone::Battlefield);
    let jewel = put(&mut state, P1, "Coveted Jewel", Zone::Battlefield);
    let mite = put(&mut state, P1, "Haywire Mite", Zone::Battlefield);
    let elves = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    let end = put(&mut state, P0, "Brotherhood's End", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R, ManaColor::R], 1);
    cast_modal(&mut state, end, 1, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, ring), Zone::Graveyard);
    assert_eq!(zone(&state, trinisphere), Zone::Graveyard, "mana value 3");
    assert_eq!(zone(&state, mite), Zone::Graveyard, "an artifact creature");
    assert_eq!(zone(&state, jewel), Zone::Battlefield, "mana value 6");
    assert_eq!(zone(&state, elves), Zone::Battlefield);
}

#[test]
fn meltdown_destroys_each_artifact_with_mana_value_x_or_less() {
    let mut state = islands();
    let mox = put(&mut state, P0, "Mox Pearl", Zone::Battlefield);
    let ring = put(&mut state, P1, "Sol Ring", Zone::Battlefield);
    let trinisphere = put(&mut state, P1, "Trinisphere", Zone::Battlefield);
    let jewel = put(&mut state, P0, "Coveted Jewel", Zone::Battlefield);
    let meltdown = put(&mut state, P0, "Meltdown", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 3);
    act(&mut state, Action::CastSpell(meltdown));
    announce_x(&mut state, 3);
    settled(&mut state);
    assert_eq!(zone(&state, mox), Zone::Graveyard);
    assert_eq!(zone(&state, ring), Zone::Graveyard);
    assert_eq!(zone(&state, trinisphere), Zone::Graveyard);
    assert_eq!(zone(&state, jewel), Zone::Battlefield);

    // X = 0 destroys only the artifacts worth nothing.
    let mut state = islands();
    let mox = put(&mut state, P0, "Mox Pearl", Zone::Battlefield);
    let ring = put(&mut state, P1, "Sol Ring", Zone::Battlefield);
    let meltdown = put(&mut state, P0, "Meltdown", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 0);
    act(&mut state, Action::CastSpell(meltdown));
    announce_x(&mut state, 0);
    settled(&mut state);
    assert_eq!(zone(&state, mox), Zone::Graveyard);
    assert_eq!(zone(&state, ring), Zone::Battlefield);
}

#[test]
fn rough_deals_two_to_creatures_without_flying_and_tumble_six_to_those_with_flying() {
    let mut state = islands();
    let elves = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    let sovereign = put(&mut state, P1, "Vampire Sovereign", Zone::Battlefield);
    let altisaur = put(&mut state, P0, "Annoyed Altisaur", Zone::Battlefield);
    let split = put(&mut state, P0, "Rough // Tumble", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 6);
    cast_modal(&mut state, split, 0, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, elves), Zone::Graveyard);
    assert_eq!(state.objects.get(sovereign).damage, 0, "it has flying");
    assert_eq!(state.objects.get(altisaur).damage, 2);
    assert_eq!(zone(&state, split), Zone::Graveyard);

    let mut state = islands();
    let elves = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    let sovereign = put(&mut state, P1, "Vampire Sovereign", Zone::Battlefield);
    let split = put(&mut state, P0, "Rough // Tumble", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 5);
    cast_modal(&mut state, split, 1, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, sovereign), Zone::Graveyard, "6 damage");
    assert_eq!(zone(&state, elves), Zone::Battlefield);
    assert_eq!(
        zone(&state, split),
        Zone::Graveyard,
        "a split card is not exiled"
    );
}

#[test]
fn swan_song_counters_an_enchantment_instant_or_sorcery_and_gives_its_controller_a_bird() {
    let mut state = islands();
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    let pyroclasm = put(&mut state, P0, "Pyroclasm", Zone::Hand);
    let song = put(&mut state, P1, "Swan Song", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 1);
    cast(&mut state, pyroclasm, &[]);
    pass_priority(&mut state);
    pool(&mut state, P1, &[ManaColor::U], 0);
    cast(&mut state, song, &[Target::Object(pyroclasm)]);
    settled(&mut state);
    assert_eq!(zone(&state, pyroclasm), Zone::Graveyard, "countered");
    assert_eq!(zone(&state, elves), Zone::Battlefield, "it never resolved");
    assert_eq!(controls(&state, P0, "Swan Song Bird Token").len(), 1);
    assert!(controls(&state, P1, "Swan Song Bird Token").is_empty());
    let bird = controls(&state, P0, "Swan Song Bird Token")[0];
    assert_eq!(power_toughness(&state, bird), (2, 2));
    assert!(keyword(&state, bird, Keywords::FLYING));

    // An enchantment spell.
    let mut state = islands();
    let choke = put(&mut state, P0, "Choke", Zone::Hand);
    let song = put(&mut state, P1, "Swan Song", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 2);
    cast(&mut state, choke, &[]);
    pass_priority(&mut state);
    pool(&mut state, P1, &[ManaColor::U], 0);
    cast(&mut state, song, &[Target::Object(choke)]);
    settled(&mut state);
    assert_eq!(zone(&state, choke), Zone::Graveyard);

    // A creature spell is no target.
    let mut state = islands();
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Hand);
    let song = put(&mut state, P1, "Swan Song", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 0);
    cast(&mut state, elves, &[]);
    pass_priority(&mut state);
    pool(&mut state, P1, &[ManaColor::U], 0);
    assert!(!castable(&mut state, song));
}

// ---- Statics -------------------------------------------------------------------

#[test]
fn choke_keeps_islands_tapped_through_their_controllers_untap_steps() {
    let mut state = islands();
    put(&mut state, P1, "Choke", Zone::Battlefield);
    let island = put(&mut state, P0, "Island", Zone::Battlefield);
    let theirs = put(&mut state, P1, "Volcanic Island", Zone::Battlefield);
    let forest = put(&mut state, P0, "Forest", Zone::Battlefield);
    for object in [island, theirs, forest] {
        state.objects.get_mut(object).tapped = true;
    }
    to_next_turn(&mut state, |_, _| None);
    assert!(state.objects.get(island).tapped);
    assert!(state.objects.get(theirs).tapped, "a nonbasic Island too");
    assert!(!state.objects.get(forest).tapped);
}

#[test]
fn defense_grid_taxes_spells_cast_outside_their_controllers_turn() {
    let mut state = islands();
    put(&mut state, P1, "Defense Grid", Zone::Battlefield);
    let pyroclasm = put(&mut state, P0, "Pyroclasm", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 1);
    assert!(castable(&mut state, pyroclasm), "on its controller's turn");
    cast(&mut state, pyroclasm, &[]);
    pass_priority(&mut state);
    // P1 casts Swan Song on P0's turn: {U} plus {3}.
    let song = put(&mut state, P1, "Swan Song", Zone::Hand);
    pool(&mut state, P1, &[ManaColor::U], 2);
    assert!(!castable(&mut state, song));
    pool(&mut state, P1, &[], 1);
    assert!(castable(&mut state, song));
    // The tax applies to the Grid's controller on the other turn too.
    let mut state = islands();
    put(&mut state, P1, "Defense Grid", Zone::Battlefield);
    set_active(&mut state, P1, P0);
    let swan = put(&mut state, P0, "Swan Song", Zone::Hand);
    let pyroclasm = put(&mut state, P1, "Pyroclasm", Zone::Hand);
    set_active(&mut state, P1, P1);
    pool(&mut state, P1, &[ManaColor::R], 1);
    cast(&mut state, pyroclasm, &[]);
    pass_priority(&mut state);
    pool(&mut state, P0, &[ManaColor::U], 2);
    assert!(!castable(&mut state, swan));
}

#[test]
fn dread_of_night_gives_white_creatures_minus_one_minus_one() {
    let mut state = islands();
    let priest = put(&mut state, P0, "Containment Priest", Zone::Battlefield);
    let elves = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    let inspector = put(&mut state, P1, "Thraben Inspector", Zone::Battlefield);
    assert_eq!(power_toughness(&state, priest), (2, 2));
    let dread = put(&mut state, P0, "Dread of Night", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 0);
    cast(&mut state, dread, &[]);
    settled(&mut state);
    assert_eq!(power_toughness(&state, priest), (1, 1));
    assert_eq!(
        power_toughness(&state, inspector),
        (0, 1),
        "an opponent's too"
    );
    assert_eq!(power_toughness(&state, elves), (1, 1), "a green creature");
    // A second Dread kills the 1/2 white creature and shrinks the Priest to 0/0.
    let dread2 = put(&mut state, P0, "Dread of Night", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 0);
    cast(&mut state, dread2, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, priest), Zone::Graveyard);
    assert_eq!(zone(&state, inspector), Zone::Graveyard);
    to_graveyard(&mut state, dread2);
    to_graveyard(&mut state, dread);
    assert_eq!(power_toughness(&state, elves), (1, 1));
}

#[test]
fn massacre_is_free_with_an_opposing_plains_and_a_swamp_and_shrinks_every_creature() {
    let mut state = islands();
    let elves = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    let sovereign = put(&mut state, P0, "Vampire Sovereign", Zone::Battlefield);
    let massacre = put(&mut state, P0, "Massacre", Zone::Hand);
    put(&mut state, P0, "Swamp", Zone::Battlefield);
    assert!(!castable(&mut state, massacre), "no opposing Plains");
    put(&mut state, P1, "Plains", Zone::Battlefield);
    assert!(castable(&mut state, massacre));
    cast(&mut state, massacre, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, massacre), Zone::Graveyard);
    assert_eq!(zone(&state, elves), Zone::Graveyard);
    assert_eq!(power_toughness(&state, sovereign), (1, 2));
    // Creatures entering afterwards are unaffected.
    let later = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    assert_eq!(power_toughness(&state, later), (1, 1));

    // The Swamp is needed too.
    let mut state = islands();
    let massacre = put(&mut state, P0, "Massacre", Zone::Hand);
    put(&mut state, P1, "Plains", Zone::Battlefield);
    assert!(!castable(&mut state, massacre));
    // Or its mana cost.
    pool(&mut state, P0, &[ManaColor::B, ManaColor::B], 2);
    assert!(castable(&mut state, massacre));
}

#[test]
fn warping_wail_exiles_a_small_creature_counters_a_sorcery_or_makes_a_scion() {
    let mut state = islands();
    let elves = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    let sovereign = put(&mut state, P1, "Vampire Sovereign", Zone::Battlefield);
    let wail = put(&mut state, P0, "Warping Wail", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W, ManaColor::W], 0);
    assert!(!castable(&mut state, wail), "{{C}} needs colorless mana");
    pool(&mut state, P0, &[], 2);
    act(&mut state, Action::CastSpell(wail));
    loop {
        match next(&mut state) {
            Decision::ChooseSpellMode { legal_modes, .. } => {
                assert!(legal_modes.contains(&0) && legal_modes.contains(&2));
                act(&mut state, Action::ChooseSpellMode(0));
            }
            Decision::ChooseTargets { legal_targets, .. } => {
                assert!(legal_targets.contains(&Target::Object(elves)));
                assert!(!legal_targets.contains(&Target::Object(sovereign)));
                act(&mut state, Action::ChooseTarget(Target::Object(elves)));
            }
            _ => break,
        }
    }
    settled(&mut state);
    assert_eq!(zone(&state, elves), Zone::Exile);

    // Counter target sorcery spell.
    let mut state = islands();
    let pyroclasm = put(&mut state, P0, "Pyroclasm", Zone::Hand);
    let wail = put(&mut state, P1, "Warping Wail", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 1);
    cast(&mut state, pyroclasm, &[]);
    pass_priority(&mut state);
    pool(&mut state, P1, &[], 2);
    cast_modal(&mut state, wail, 1, &[Target::Object(pyroclasm)]);
    settled(&mut state);
    assert_eq!(zone(&state, pyroclasm), Zone::Graveyard);

    // An Eldrazi Scion token.
    let mut state = islands();
    let wail = put(&mut state, P0, "Warping Wail", Zone::Hand);
    pool(&mut state, P0, &[], 2);
    cast_modal(&mut state, wail, 2, &[]);
    settled(&mut state);
    let scions = controls(&state, P0, "Eldrazi Scion Token");
    assert_eq!(scions.len(), 1);
    assert_eq!(power_toughness(&state, scions[0]), (1, 1));
}

#[test]
fn force_of_vigor_destroys_up_to_two_and_is_free_on_another_turn_for_a_green_card() {
    let mut state = islands();
    let ring = put(&mut state, P1, "Sol Ring", Zone::Battlefield);
    let choke = put(&mut state, P1, "Choke", Zone::Battlefield);
    let elves = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    let force = put(&mut state, P0, "Force of Vigor", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G, ManaColor::G], 2);
    cast(
        &mut state,
        force,
        &[Target::Object(ring), Target::Object(choke)],
    );
    settled(&mut state);
    assert_eq!(zone(&state, ring), Zone::Graveyard);
    assert_eq!(zone(&state, choke), Zone::Graveyard);
    assert_eq!(zone(&state, elves), Zone::Battlefield);

    // One target is enough.
    let mut state = islands();
    let ring = put(&mut state, P1, "Sol Ring", Zone::Battlefield);
    let choke = put(&mut state, P1, "Choke", Zone::Battlefield);
    let force = put(&mut state, P0, "Force of Vigor", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G, ManaColor::G], 2);
    cast(&mut state, force, &[Target::Object(ring)]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseTargets { .. } => Action::FinishEffectSelection,
        other => first_answer(other),
    });
    assert_eq!(zone(&state, ring), Zone::Graveyard);
    assert_eq!(zone(&state, choke), Zone::Battlefield);

    // On another player's turn, a green card from hand pays for it.
    let mut state = islands();
    set_active(&mut state, P1, P0);
    let ring = put(&mut state, P1, "Sol Ring", Zone::Battlefield);
    let force = put(&mut state, P0, "Force of Vigor", Zone::Hand);
    assert!(!castable(&mut state, force), "no green card to exile");
    let green = put(&mut state, P0, "Llanowar Elves", Zone::Hand);
    assert!(castable(&mut state, force));
    act(&mut state, Action::CastSpell(force));
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseCastMode { .. } => Action::ChooseCastMode(engine::CastMode::Alternative),
        Decision::ChooseCostTargets { candidates, .. } => {
            assert!(candidates.contains(&green));
            Action::ChooseCostTarget(green)
        }
        Decision::ChooseTargets { legal_targets, .. }
            if legal_targets.contains(&Target::Object(ring)) =>
        {
            Action::ChooseTarget(Target::Object(ring))
        }
        other => first_answer(other),
    });
    assert_eq!(zone(&state, green), Zone::Exile);
    assert_eq!(zone(&state, ring), Zone::Graveyard);

    // Not on your own turn.
    let mut state = islands();
    let force = put(&mut state, P0, "Force of Vigor", Zone::Hand);
    put(&mut state, P1, "Sol Ring", Zone::Battlefield);
    put(&mut state, P0, "Llanowar Elves", Zone::Hand);
    assert!(!castable(&mut state, force));
}

// ---- Zone-change replacements --------------------------------------------------

#[test]
fn rest_in_peace_exiles_graveyards_and_everything_headed_for_them() {
    let mut state = islands();
    let mine = put(&mut state, P0, "Island", Zone::Graveyard);
    let theirs = put(&mut state, P1, "Llanowar Elves", Zone::Graveyard);
    let rest = put(&mut state, P0, "Rest in Peace", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W], 1);
    cast(&mut state, rest, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, rest), Zone::Battlefield);
    assert_eq!(zone(&state, mine), Zone::Exile);
    assert_eq!(zone(&state, theirs), Zone::Exile);

    // A creature dying, a discard and a resolving spell are exiled instead.
    let mite = put(&mut state, P0, "Haywire Mite", Zone::Battlefield);
    to_graveyard(&mut state, mite);
    settled(&mut state);
    assert_eq!(zone(&state, mite), Zone::Exile);
    assert_eq!(state.players[0].life, 20, "it did not die");
    let discard = put(&mut state, P1, "Island", Zone::Hand);
    to_graveyard(&mut state, discard);
    assert_eq!(zone(&state, discard), Zone::Exile);
    let pyroclasm = put(&mut state, P0, "Pyroclasm", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 1);
    cast(&mut state, pyroclasm, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, pyroclasm), Zone::Exile);
    assert_eq!(graveyard_count(&state, P0) + graveyard_count(&state, P1), 0);

    // A token never reaches a graveyard, so nothing sees it die.
    let token = put(&mut state, P0, "Goblin Token", Zone::Battlefield);
    to_graveyard(&mut state, token);
    assert!(state
        .objects
        .try_get(token)
        .is_none_or(|live| live.zone != Zone::Graveyard));
}

#[test]
fn leyline_of_the_void_exiles_cards_headed_for_opponents_graveyards() {
    let mut state = islands();
    put(&mut state, P0, "Leyline of the Void", Zone::Battlefield);
    let theirs = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    let mine = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    let discard = put(&mut state, P1, "Island", Zone::Hand);
    to_graveyard(&mut state, theirs);
    to_graveyard(&mut state, mine);
    to_graveyard(&mut state, discard);
    assert_eq!(zone(&state, theirs), Zone::Exile);
    assert_eq!(zone(&state, discard), Zone::Exile);
    assert_eq!(zone(&state, mine), Zone::Graveyard, "its controller's own");
}

#[test]
fn leyline_of_the_void_may_begin_the_game_on_the_battlefield() {
    for begin in [true, false] {
        let mut state = fresh_game();
        let leyline = put(&mut state, P1, "Leyline of the Void", Zone::Hand);
        let mut offered = false;
        run_until(
            &mut state,
            |state| state.step == Step::Main1 && state.stack.is_empty(),
            |state, decision| match decision {
                Decision::ChooseEffectOption { player, .. } => {
                    assert_eq!(*player, P1);
                    assert_eq!(state.step, Step::Upkeep);
                    offered = true;
                    Some(Action::ChooseEffectOption(u16::from(!begin)))
                }
                _ => None,
            },
        );
        assert!(offered);
        let expected = if begin { Zone::Battlefield } else { Zone::Hand };
        assert_eq!(zone(&state, leyline), expected, "begin {begin}");
    }
}

#[test]
fn containment_priest_exiles_a_nontoken_creature_that_enters_without_being_cast() {
    let mut state = islands();
    let priest = put(&mut state, P0, "Containment Priest", Zone::Battlefield);
    // Cast creatures enter.
    let cast_elves = put(&mut state, P0, "Llanowar Elves", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 0);
    cast(&mut state, cast_elves, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, cast_elves), Zone::Battlefield);
    // Put onto the battlefield from a hand, a graveyard or exile: exiled.
    for from in [Zone::Hand, Zone::Graveyard, Zone::Exile] {
        let elves = put(&mut state, P1, "Llanowar Elves", from);
        enter(&mut state, elves);
        assert_eq!(zone(&state, elves), Zone::Exile, "from {from:?}");
    }
    // Noncreature permanents and tokens enter.
    let ring = put(&mut state, P0, "Sol Ring", Zone::Hand);
    enter(&mut state, ring);
    assert_eq!(zone(&state, ring), Zone::Battlefield);
    event::propose_and_commit(
        &mut state,
        ProposedEvent::create_token(id("Goblin Token"), P0),
    );
    assert_eq!(controls(&state, P0, "Goblin Token").len(), 1);
    // The Priest does not exile itself, but a second Priest exiles the first.
    to_graveyard(&mut state, priest);
    let entering = put(&mut state, P0, "Containment Priest", Zone::Hand);
    enter(&mut state, entering);
    assert_eq!(zone(&state, entering), Zone::Battlefield);
    let second = put(&mut state, P0, "Containment Priest", Zone::Hand);
    enter(&mut state, second);
    assert_eq!(zone(&state, second), Zone::Exile);
}

#[test]
fn disruptor_flute_taxes_the_chosen_name_and_stops_its_activated_abilities() {
    let mut state = islands();
    let flute = put(&mut state, P0, "Disruptor Flute", Zone::Hand);
    pool(&mut state, P0, &[], 2);
    cast(&mut state, flute, &[]);
    let (group, index) = name_choice("Strip Mine");
    match settle(&mut state) {
        Some(Decision::ChooseEffectOption { player, .. }) => {
            assert_eq!(player, P0);
            act(&mut state, Action::ChooseEffectOption(group));
        }
        other => panic!("expected the letter, got {other:?}"),
    }
    if let Some(Decision::ChooseEffectOption { .. }) = settle(&mut state) {
        act(&mut state, Action::ChooseEffectOption(index));
    }
    settled(&mut state);
    assert_eq!(zone(&state, flute), Zone::Battlefield);
    // Strip Mine's sacrifice ability cannot be activated, though it still
    // taps for mana.
    let mine = put(&mut state, P0, "Strip Mine", Zone::Battlefield);
    put(&mut state, P1, "Island", Zone::Battlefield);
    assert!(!activatable(&mut state, mine, 0));
    act(&mut state, Action::ActivateManaAbility(mine));
    assert_eq!(mana(&state, P0, ManaColor::C), 1);
    // Another land is unaffected.
    let tower = put(&mut state, P0, "Wasteland", Zone::Battlefield);
    assert!(activatable(&mut state, tower, 0));
    // Spells with the chosen name cost {3} more.
    let second = put(&mut state, P0, "Strip Mine", Zone::Hand);
    let pyroclasm = put(&mut state, P0, "Pyroclasm", Zone::Hand);
    let (_, _) = (second, pyroclasm);
}

#[test]
fn disruptor_flute_taxes_spells_with_the_chosen_name() {
    let mut state = islands();
    let flute = put(&mut state, P0, "Disruptor Flute", Zone::Hand);
    pool(&mut state, P0, &[], 2);
    cast(&mut state, flute, &[]);
    let (group, index) = name_choice("Sol Ring");
    match settle(&mut state) {
        Some(Decision::ChooseEffectOption { .. }) => {
            act(&mut state, Action::ChooseEffectOption(group));
        }
        other => panic!("expected the letter, got {other:?}"),
    }
    if let Some(Decision::ChooseEffectOption { .. }) = settle(&mut state) {
        act(&mut state, Action::ChooseEffectOption(index));
    }
    settled(&mut state);
    let ring = put(&mut state, P1, "Sol Ring", Zone::Hand);
    let mine = put(&mut state, P0, "Sol Ring", Zone::Hand);
    pool(&mut state, P0, &[], 3);
    assert!(!castable(&mut state, mine), "{{1}} plus {{3}}");
    pool(&mut state, P0, &[], 1);
    assert!(castable(&mut state, mine));
    // It taxes every player's copy.
    let (_, _) = (ring, mine);
    set_active(&mut state, P1, P1);
    pool(&mut state, P1, &[], 3);
    assert!(!castable(&mut state, ring));
    pool(&mut state, P1, &[], 1);
    assert!(castable(&mut state, ring));
}

// ---- Surgical Extraction, Lavinia and Carpet of Flowers -----------------------

#[test]
fn surgical_extraction_exiles_every_copy_from_the_graveyard_hand_and_library() {
    let mut state = game(&[
        "Island",
        "Llanowar Elves",
        "Island",
        "Llanowar Elves",
        "Island",
    ]);
    let target = put(&mut state, P1, "Llanowar Elves", Zone::Graveyard);
    let in_hand = put(&mut state, P1, "Llanowar Elves", Zone::Hand);
    let other = put(&mut state, P1, "Island", Zone::Hand);
    let mine = put(&mut state, P0, "Llanowar Elves", Zone::Hand);
    let extraction = put(&mut state, P0, "Surgical Extraction", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 0);
    let library = |state: &GameState, player: PlayerId, name: &str| {
        state.players[player.index()]
            .library
            .iter()
            .filter(|&&card| name_of(state, card) == name)
            .count()
    };
    assert_eq!(library(&state, P1, "Llanowar Elves"), 2);
    cast(&mut state, extraction, &[Target::Object(target)]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseEffectTargets { legal_targets, .. } => {
            Action::ChooseEffectTarget(legal_targets[0])
        }
        other => first_answer(other),
    });
    assert_eq!(zone(&state, target), Zone::Exile);
    assert_eq!(zone(&state, in_hand), Zone::Exile);
    assert_eq!(zone(&state, other), Zone::Hand, "another name");
    assert_eq!(zone(&state, mine), Zone::Hand, "another owner");
    assert_eq!(library(&state, P1, "Llanowar Elves"), 0);
    assert_eq!(library(&state, P0, "Llanowar Elves"), 2);
    assert_eq!(state.players[0].life, 20, "paid with {{B}}");
    assert_eq!(zone(&state, extraction), Zone::Graveyard);
}

#[test]
fn surgical_extraction_can_be_paid_with_life_and_never_targets_a_basic_land() {
    let mut state = islands();
    let basic = put(&mut state, P1, "Island", Zone::Graveyard);
    let mine = put(&mut state, P0, "Strip Mine", Zone::Graveyard);
    let theirs = put(&mut state, P1, "Llanowar Elves", Zone::Graveyard);
    let extraction = put(&mut state, P0, "Surgical Extraction", Zone::Hand);
    assert!(castable(&mut state, extraction), "two life for {{B/P}}");
    act(&mut state, Action::CastSpell(extraction));
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseTargets { legal_targets, .. } => {
            assert!(!legal_targets.contains(&Target::Object(basic)));
            assert!(legal_targets.contains(&Target::Object(mine)));
            assert!(legal_targets.contains(&Target::Object(theirs)));
            Action::ChooseTarget(Target::Object(mine))
        }
        Decision::ChooseEffectTargets { legal_targets, .. } => {
            assert_eq!(legal_targets, vec![Target::Object(mine)]);
            Action::ChooseEffectTarget(Target::Object(mine))
        }
        other => first_answer(other),
    });
    assert_eq!(state.players[0].life, 18);
    assert_eq!(zone(&state, mine), Zone::Exile);
    assert_eq!(zone(&state, basic), Zone::Graveyard);
}

#[test]
fn surgical_extraction_can_finish_at_zero_or_exile_a_private_subset_in_either_seat() {
    use mtg_kernel::eternal_sideboard_s1a_v1::surgical_search_look_v1;
    use mtg_kernel::rl::{legal_action_candidates_v1, ActionSemanticV1};
    use mtg_kernel::state::ObservationNoteKindV1;
    use mtg_kernel::surface::{SurfaceAction, SurfaceDecision};

    for actor in [P0, P1] {
        let owner = actor.opponent();
        let mut state = game(&["Llanowar Elves", "Island", "Llanowar Elves"]);
        state.active_player = actor;
        state.priority_player = actor;
        let target = put(&mut state, owner, "Llanowar Elves", Zone::Graveyard);
        let in_hand = put(&mut state, owner, "Llanowar Elves", Zone::Hand);
        let other = put(&mut state, owner, "Snap", Zone::Hand);
        let extraction = put(&mut state, actor, "Surgical Extraction", Zone::Hand);
        pool(&mut state, actor, &[ManaColor::B], 0);
        cast(&mut state, extraction, &[Target::Object(target)]);
        let prompt = settle(&mut state).expect("native Surgical search");
        let Decision::ChooseEffectTargets { player, legal_targets, min_targets, max_targets, can_finish, .. } = &prompt else {
            panic!("expected private search, got {prompt:?}");
        };
        assert_eq!((*player, *min_targets, *max_targets, *can_finish), (actor, 0, 4, true));
        let library = state.players[owner.index()].library.clone();
        assert_eq!(legal_targets, &vec![Target::Object(target), Target::Object(in_hand), Target::Object(library[0]), Target::Object(library[2])]);
        let look = surgical_search_look_v1(&state, actor).unwrap().unwrap();
        assert_eq!(look.owner, owner);
        assert_eq!(look.hand_printings.len(), 2);
        assert!(look.hand_printings.contains(&id("Snap")));
        assert!(look.library_printings.contains(&id("Island")));
        assert!(surgical_search_look_v1(&state, owner).unwrap().is_none());

        let rows = legal_action_candidates_v1(&SurfaceDecision::Decision(prompt), &state).unwrap();
        assert_eq!(rows.len(), 5);
        assert!(matches!(&rows[1].record.semantic, ActionSemanticV1::ChooseEffectTarget {
            target: mtg_kernel::rl::TargetRefV1::Object { object }, ..
        } if object.arena_id == in_hand.0 && object.card_db_id == id("Llanowar Elves")));
        assert_eq!(rows[1].surface_action, SurfaceAction::Action(Action::ChooseEffectTarget(Target::Object(in_hand))));
        assert_eq!(rows[4].surface_action, SurfaceAction::Action(Action::FinishEffectSelection));

        let mut zero = state.clone();
        act(&mut zero, Action::FinishEffectSelection);
        settled(&mut zero);
        assert_eq!(zone(&zero, target), Zone::Graveyard);
        assert_eq!(zone(&zero, in_hand), Zone::Hand);
        assert_eq!(zone(&zero, library[0]), Zone::Library);
        assert!(zero.observation_journal.notes().iter().any(|note| matches!(note.kind, ObservationNoteKindV1::LibraryRandomized { owner: shuffled } if shuffled == owner)));

        act(&mut state, Action::ChooseEffectTarget(Target::Object(in_hand)));
        assert!(matches!(next(&mut state), Decision::ChooseEffectTargets { selected_count: 1, .. }));
        let snapshot = serde_json::to_vec(&state).unwrap();
        let mut restored: GameState = serde_json::from_slice(&snapshot).unwrap();
        assert_eq!(next(&mut state), next(&mut restored));
        let mut reordered = restored.clone();
        let Some(mtg_kernel::effect::PendingEffectChoice::SelectTargets { legal, .. }) = reordered
            .engine.pending_effect.as_mut().and_then(|pending| pending.choice.as_mut())
        else {
            panic!("missing restored search choice");
        };
        legal.swap(0, 1);
        assert!(mtg_kernel::effect::validate_pending_effect_choice(&reordered).is_err());
        for continued in [&mut state, &mut restored] {
            act(continued, Action::ChooseEffectTarget(Target::Object(library[2])));
            act(continued, Action::FinishEffectSelection);
            settled(continued);
            assert_eq!(zone(continued, in_hand), Zone::Exile);
            assert_eq!(zone(continued, library[2]), Zone::Exile);
            assert_eq!(zone(continued, target), Zone::Graveyard);
            assert_eq!(zone(continued, other), Zone::Hand);
            assert_eq!(zone(continued, library[0]), Zone::Library);
            assert!(surgical_search_look_v1(continued, actor).unwrap().is_none());
        }
        assert_eq!(state.state_hash(), restored.state_hash());
    }
}

#[test]
fn surgical_extraction_hides_the_search_roster_and_width_from_the_nonchooser() {
    use mtg_kernel::eternal_sideboard_s1a_v1::surgical_search_look_v1;
    use mtg_kernel::policy_surface_v5::PolicySurfaceV5;
    use mtg_kernel::rl::{observe_policy_v5, PendingEffectChoiceSemanticV4};

    for actor in [P0, P1] {
        let viewer = actor.opponent();
        let run = |hidden| {
            let mut state = game(&["Island"; 4]);
            state.active_player = actor;
            state.priority_player = actor;
            let target = put(&mut state, actor, "Llanowar Elves", Zone::Graveyard);
            put(&mut state, actor, hidden, Zone::Library);
            let extraction = put(&mut state, actor, "Surgical Extraction", Zone::Hand);
            pool(&mut state, actor, &[ManaColor::B], 0);
            cast(&mut state, extraction, &[Target::Object(target)]);
            let Some(Decision::ChooseEffectTargets { max_targets, .. }) = settle(&mut state) else {
                panic!("expected Surgical search");
            };
            assert_eq!(max_targets, if hidden == "Llanowar Elves" { 2 } else { 1 });
            assert!(surgical_search_look_v1(&state, viewer).unwrap().is_none());
            let observation = observe_policy_v5(&state, &PolicySurfaceV5::new(), viewer, 0, 0, 0, 1).unwrap();
            assert!(matches!(
                observation.projection.surface.engine_context.pending_effect.as_ref()
                    .and_then(|pending| pending.choice.as_ref()),
                Some(PendingEffectChoiceSemanticV4::Targets {
                    selected_targets, legal_targets, min_targets: 0, max_targets: 0, ..
                }) if selected_targets.is_empty() && legal_targets.is_empty()
            ));
            observation
        };
        assert_eq!(run("Llanowar Elves"), run("Forest"));
    }
}

#[test]
fn lavinia_stops_big_noncreature_spells_for_opponents() {
    let mut state = islands();
    put(
        &mut state,
        P0,
        "Lavinia, Azorius Renegade",
        Zone::Battlefield,
    );
    set_active(&mut state, P1, P1);
    put(&mut state, P1, "Island", Zone::Battlefield);
    put(&mut state, P1, "Island", Zone::Battlefield);
    let pyroclasm = put(&mut state, P1, "Pyroclasm", Zone::Hand);
    let end = put(&mut state, P1, "Brotherhood's End", Zone::Hand);
    let sovereign = put(&mut state, P1, "Vampire Sovereign", Zone::Hand);
    pool(
        &mut state,
        P1,
        &[ManaColor::R, ManaColor::R, ManaColor::B, ManaColor::B],
        6,
    );
    assert!(castable(&mut state, pyroclasm), "mana value 2, two lands");
    assert!(!castable(&mut state, end), "mana value 3, two lands");
    assert!(
        castable(&mut state, sovereign),
        "creature spells are free to cast"
    );
    put(&mut state, P1, "Island", Zone::Battlefield);
    assert!(castable(&mut state, end), "three lands");

    // Her controller is not restricted.
    let mut state = islands();
    put(
        &mut state,
        P0,
        "Lavinia, Azorius Renegade",
        Zone::Battlefield,
    );
    let end = put(&mut state, P0, "Brotherhood's End", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R, ManaColor::R], 1);
    assert!(castable(&mut state, end));
}

#[test]
fn lavinia_counters_a_spell_an_opponent_casts_without_spending_mana() {
    let mut state = islands();
    put(
        &mut state,
        P0,
        "Lavinia, Azorius Renegade",
        Zone::Battlefield,
    );
    set_active(&mut state, P1, P1);
    let mox = put(&mut state, P1, "Mox Pearl", Zone::Hand);
    cast(&mut state, mox, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, mox), Zone::Graveyard, "countered");

    // A spell paid for with mana resolves.
    let mut state = islands();
    put(
        &mut state,
        P0,
        "Lavinia, Azorius Renegade",
        Zone::Battlefield,
    );
    set_active(&mut state, P1, P1);
    put(&mut state, P1, "Island", Zone::Battlefield);
    let ring = put(&mut state, P1, "Sol Ring", Zone::Hand);
    pool(&mut state, P1, &[], 1);
    cast(&mut state, ring, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, ring), Zone::Battlefield);

    // Her controller's own free spells resolve.
    let mut state = islands();
    put(
        &mut state,
        P0,
        "Lavinia, Azorius Renegade",
        Zone::Battlefield,
    );
    let mox = put(&mut state, P0, "Mox Pearl", Zone::Hand);
    cast(&mut state, mox, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, mox), Zone::Battlefield);
}

#[test]
fn carpet_of_flowers_adds_mana_for_each_island_the_opponent_controls() {
    let mut state = islands();
    put(&mut state, P0, "Carpet of Flowers", Zone::Battlefield);
    for _ in 0..3 {
        put(&mut state, P1, "Island", Zone::Battlefield);
    }
    put(&mut state, P1, "Volcanic Island", Zone::Battlefield);
    put(&mut state, P1, "Forest", Zone::Battlefield);
    let turn = state.turn;
    run_until(
        &mut state,
        |state| state.turn > turn && state.active_player == P0 && state.step == Step::Main1,
        |_, _| None,
    );
    // Options: nothing, then {W}, {U}, {B}, {R}, {G} for one to four mana.
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseEffectOption { option_count, .. } => {
            assert_eq!(option_count, 1 + 5 * 4);
            Action::ChooseEffectOption(1 + 3 * 4 + 2)
        }
        other => first_answer(other),
    });
    assert_eq!(mana(&state, P0, ManaColor::R), 3, "three red mana");

    // Declining adds nothing, and with no Islands there is no ability.
    let mut state = islands();
    put(&mut state, P0, "Carpet of Flowers", Zone::Battlefield);
    put(&mut state, P1, "Forest", Zone::Battlefield);
    let turn = state.turn;
    run_until(
        &mut state,
        |state| state.turn > turn && state.active_player == P0 && state.step == Step::Main1,
        |_, _| None,
    );
    settled(&mut state);
    for color in [
        ManaColor::W,
        ManaColor::U,
        ManaColor::B,
        ManaColor::R,
        ManaColor::G,
    ] {
        assert_eq!(mana(&state, P0, color), 0);
    }
}
