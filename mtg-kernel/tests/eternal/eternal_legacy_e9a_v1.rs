//! Eternal wave E9a: native rules tests for the Legacy long-tail cards that
//! complete Selesnya Depths, Red Stompy, LED Dredge, Painter and Cephalid
//! Breakfast, and the Initiative, Mono-Black and Death and Taxes cards.
#![cfg(feature = "eternal-fixtures")]

use mtg_kernel::card_def::{
    card_id_by_name, AltCostCondition, CardCapability, CardDef, Keywords, Subtype, CARD_DEFS,
};
use mtg_kernel::engine::{self, Action, CastMode, Decision, OptionalCostChoice};
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::ManaColor;
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Target, Zone};

const P0: PlayerId = PlayerId::P0;
const P1: PlayerId = PlayerId::P1;

const E9A_CARDS: [&str; 41] = [
    "Anger",
    "Ashen Rider",
    "Blood Moon",
    "Breakthrough",
    "Bridge from Below",
    "Broadside Bombardiers",
    "Caves of Chaos Adventurer",
    "Cephalid Coliseum",
    "Cephalid Illusionist",
    "Chaos Defiler",
    "Echoing Deeps",
    "Elvish Reclaimer",
    "Endurance",
    "Ensnaring Bridge",
    "Eumidian Hatchery",
    "Fury",
    "Goblin Rabblemaster",
    "Green Sun's Zenith",
    "Grindstone",
    "Horizon Canopy",
    "Inquisition of Kozilek",
    "Knight of the Reliquary",
    "Legolas's Quick Reflexes",
    "Loki, God of Mischief",
    "Magus of the Moon",
    "Mishra's Research Desk",
    "Nomads en-Kor",
    "Orim's Chant",
    "Otherworldly Gaze",
    "Ox of Agonas",
    "Painter's Servant",
    "Poxwalkers",
    "Samwise the Stouthearted",
    "Sejiri Steppe",
    "Shatterskull Smashing",
    "Shuko",
    "Simian Spirit Guide",
    "Sundering Eruption",
    "Super Shredder",
    "Tolaria",
    "Undermountain Adventurer",
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
        0x4539_4121_0001,
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

/// Settles, answering effect options with `option` and choosing `object`
/// whenever a selection or target offers it; otherwise the first answer.
fn settle_picking(state: &mut GameState, option: u16, object: Option<ObjectId>) {
    settle_with(state, |_, decision| match decision {
        Decision::ChooseEffectOption { .. } => Action::ChooseEffectOption(option),
        Decision::ChooseTargets { legal_targets, .. }
            if object.is_some_and(|object| legal_targets.contains(&Target::Object(object))) =>
        {
            Action::ChooseTarget(Target::Object(object.unwrap()))
        }
        Decision::ChooseEffectTargets {
            legal_targets,
            selected_count: 0,
            ..
        } if object.is_some_and(|object| legal_targets.contains(&Target::Object(object))) => {
            Action::ChooseEffectTarget(Target::Object(object.unwrap()))
        }
        other => first_answer(other),
    });
}

/// Settles, taking the first candidate of every selection that has none
/// selected yet (searches find a card) and the first answer otherwise.
fn settle_taking(state: &mut GameState) {
    settle_with(state, |_, decision| match decision {
        Decision::ChooseEffectTargets {
            legal_targets,
            selected_count: 0,
            ..
        } if !legal_targets.is_empty() => Action::ChooseEffectTarget(legal_targets[0]),
        other => first_answer(other),
    });
}

/// Settles, answering only the order of a multi-card mill (each card in
/// turn); any other choice fails.
fn settle_ordering(state: &mut GameState) {
    let mut chosen = Vec::new();
    settle_with(state, |_, decision| match decision {
        Decision::ChooseEffectTargets {
            legal_targets,
            selected_count,
            can_finish: false,
            ..
        } => {
            if selected_count == 0 {
                chosen.clear();
            }
            let target = *legal_targets
                .iter()
                .find(|target| !chosen.contains(*target))
                .expect("an unordered card");
            chosen.push(target);
            Action::ChooseEffectTarget(target)
        }
        Decision::ChooseEffectTargets {
            can_finish: true, ..
        } => Action::FinishEffectSelection,
        other => panic!("unexpected choice: {other:?}"),
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
/// on to P0's second main phase with an empty stack.
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

/// The attackers P0 may declare in this turn's combat.
fn eligible_attackers(state: &mut GameState) -> Vec<ObjectId> {
    pass_until(state, |state| state.step == Step::DeclareAttackers);
    match next(state) {
        Decision::DeclareAttackers { eligible, .. } => eligible,
        _ => Vec::new(),
    }
}

fn hand_contains(state: &GameState, player: PlayerId, object: ObjectId) -> bool {
    state.players[player.index()].hand.contains(&object)
}

fn known_to(state: &GameState, viewer: PlayerId, owner: PlayerId, card: ObjectId) -> bool {
    state
        .known_hand_cards(viewer, owner)
        .iter()
        .any(|entry| entry.object == card)
}

fn mana(state: &GameState, player: PlayerId, color: ManaColor) -> u8 {
    state.players[player.index()].mana_pool[color.pool_index()]
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

/// Deals `amount` damage from `source` to `object`.
fn damage(state: &mut GameState, source: ObjectId, object: ObjectId, amount: i32) {
    event::propose_and_commit(
        state,
        ProposedEvent::damage(source, Target::Object(object), amount),
    );
}

fn has_subtype(state: &GameState, object: ObjectId, subtype: Subtype) -> bool {
    engine::has_effective_subtype(state, object, subtype)
}

// ---- Characteristics -----------------------------------------------------------

#[test]
fn e9a_cards_are_full_with_their_printed_characteristics() {
    for name in E9A_CARDS {
        assert_eq!(def(name).capability, CardCapability::Full, "{name}");
        assert!(!def(name).is_token, "{name}");
    }
    let goblin = def("Goblin Haste Token");
    assert!(goblin.is_token && goblin.capability == CardCapability::Full);
    assert!(goblin.keywords.has(Keywords::HASTE));
    assert_eq!(goblin.subtypes, &[Subtype::Goblin]);
    let insect = def("Black Insect Token");
    assert!(insect.is_token && insect.capability == CardCapability::Full);
    assert!(insect.keywords.has(Keywords::FLYING));
    assert_eq!((insect.power, insect.toughness), (Some(1), Some(1)));

    assert!(def("Anger").keywords.has(Keywords::HASTE));
    assert!(def("Ashen Rider").keywords.has(Keywords::FLYING));
    let bombardiers = def("Broadside Bombardiers");
    assert!(
        bombardiers.keywords.has(Keywords::MENACE) && bombardiers.keywords.has(Keywords::HASTE)
    );
    let endurance = def("Endurance");
    assert!(endurance.keywords.has(Keywords::FLASH) && endurance.keywords.has(Keywords::REACH));
    assert!(def("Fury").keywords.has(Keywords::DOUBLE_STRIKE));
    assert!(def("Poxwalkers").keywords.has(Keywords::DEATHTOUCH));
    assert!(def("Samwise the Stouthearted")
        .keywords
        .has(Keywords::FLASH));
    assert!(def("Super Shredder").keywords.has(Keywords::MENACE));
    assert!(def("Undermountain Adventurer")
        .keywords
        .has(Keywords::VIGILANCE));
    for name in ["Fury", "Endurance"] {
        assert_eq!(
            def(name).alt_cost.map(|alt| alt.condition),
            Some(AltCostCondition::Always),
            "{name}"
        );
    }
    assert!(def("Otherworldly Gaze").flashback.is_some());
    assert!(def("Ox of Agonas").escape.is_some());
    assert!(def("Orim's Chant").kicker_cost.is_some());
    assert!(def("Shuko").equipment.is_some());
    assert!(def("Sejiri Steppe").enters_battlefield_tapped);
    for (name, subtype) in [
        ("Caves of Chaos Adventurer", Subtype::Barbarian),
        ("Loki, God of Mischief", Subtype::Villain),
        ("Nomads en-Kor", Subtype::Nomad),
        ("Painter's Servant", Subtype::Scarecrow),
        ("Samwise the Stouthearted", Subtype::Peasant),
    ] {
        assert!(def(name).subtypes.contains(&subtype), "{name}");
        assert!(subtype.is_creature_type(), "{name}");
    }
}

// ---- Lands -------------------------------------------------------------------------

#[test]
fn cephalid_coliseum_pings_for_blue_and_loots_three_with_threshold() {
    let mut state = islands();
    let coliseum = put(&mut state, P0, "Cephalid Coliseum", Zone::Battlefield);
    next(&mut state);
    act(&mut state, Action::ActivateManaAbility(coliseum));
    assert_eq!(mana(&state, P0, ManaColor::U), 1);
    assert_eq!(state.players[0].life, 19);

    let mut state = islands();
    let coliseum = put(&mut state, P0, "Cephalid Coliseum", Zone::Battlefield);
    pool(&mut state, P0, &[ManaColor::U], 0);
    assert!(!activatable(&mut state, coliseum, 0), "no threshold");
    for _ in 0..7 {
        put(&mut state, P0, "Island", Zone::Graveyard);
    }
    activate(&mut state, coliseum, 0, &[Target::Player(P0)]);
    settle_with(&mut state, |_, decision| first_answer(decision));
    assert_eq!(zone(&state, coliseum), Zone::Graveyard);
    assert_eq!(
        state.players[0].hand.len(),
        0,
        "drew three, discarded three"
    );
    assert_eq!(state.players[0].graveyard.len(), 7 + 1 + 3);
}

#[test]
fn echoing_deeps_enters_tapped_as_a_copy_of_a_graveyard_land() {
    let mut state = islands();
    put(&mut state, P0, "Bojuka Bog", Zone::Graveyard);
    let bolt = put(&mut state, P1, "Lightning Bolt", Zone::Graveyard);
    let deeps = put(&mut state, P0, "Echoing Deeps", Zone::Hand);
    next(&mut state);
    act(&mut state, Action::PlayLand(deeps));
    match next(&mut state) {
        Decision::ChooseEffectOption { option_count, .. } => {
            // Decline, or copy Bojuka Bog.
            assert_eq!(option_count, 2);
            act(&mut state, Action::ChooseEffectOption(1));
        }
        other => panic!("expected the copy choice, got {other:?}"),
    }
    assert_eq!(name_of(&state, deeps), "Bojuka Bog");
    assert!(state.objects.get(deeps).tapped);
    assert!(has_subtype(&state, deeps, Subtype::Cave));
    // The copied land's entry trigger sees it enter.
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseTargets { .. } => Action::ChooseTarget(Target::Player(P1)),
        other => first_answer(other),
    });
    assert_eq!(zone(&state, bolt), Zone::Exile);

    // Declining keeps it an untapped Echoing Deeps.
    let mut state = islands();
    put(&mut state, P0, "Bojuka Bog", Zone::Graveyard);
    let deeps = put(&mut state, P0, "Echoing Deeps", Zone::Hand);
    next(&mut state);
    act(&mut state, Action::PlayLand(deeps));
    settle_picking(&mut state, 0, None);
    assert_eq!(name_of(&state, deeps), "Echoing Deeps");
    assert!(!state.objects.get(deeps).tapped);
    next(&mut state);
    act(&mut state, Action::ActivateManaAbility(deeps));
    assert_eq!(mana(&state, P0, ManaColor::C), 1);
}

#[test]
fn eumidian_hatchery_counts_its_taps_and_leaves_insects() {
    let mut state = islands();
    let hatchery = put(&mut state, P0, "Eumidian Hatchery", Zone::Battlefield);
    next(&mut state);
    act(&mut state, Action::ActivateManaAbility(hatchery));
    assert_eq!(mana(&state, P0, ManaColor::B), 1);
    assert_eq!(state.players[0].life, 19);
    assert_eq!(
        mtg_kernel::eternal_legacy_e9a_v1::statics::hatchling_counters(&state, hatchery),
        1
    );
    to_graveyard(&mut state, hatchery);
    settled(&mut state);
    let insects = controls(&state, P0, "Black Insect Token");
    assert_eq!(insects.len(), 1);
    assert!(keyword(&state, insects[0], Keywords::FLYING));
}

#[test]
fn horizon_canopy_pays_life_for_mana_and_cycles_itself() {
    for color in [ManaColor::G, ManaColor::W] {
        let mut state = islands();
        let canopy = put(&mut state, P0, "Horizon Canopy", Zone::Battlefield);
        next(&mut state);
        act(&mut state, Action::ActivateManaAbilityChoice(canopy, color));
        assert_eq!(mana(&state, P0, color), 1);
        assert_eq!(state.players[0].life, 19);
    }
    let mut state = islands();
    let canopy = put(&mut state, P0, "Horizon Canopy", Zone::Battlefield);
    pool(&mut state, P0, &[], 1);
    activate(&mut state, canopy, 0, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, canopy), Zone::Graveyard);
    assert_eq!(state.players[0].hand.len(), 1);
}

#[test]
fn sejiri_steppe_enters_tapped_and_protects_a_creature_from_a_color() {
    let mut state = islands();
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    let swiftspear = put(&mut state, P1, "Monastery Swiftspear", Zone::Battlefield);
    let steppe = put(&mut state, P0, "Sejiri Steppe", Zone::Hand);
    next(&mut state);
    act(&mut state, Action::PlayLand(steppe));
    assert!(state.objects.get(steppe).tapped);
    // White, blue, black, red, green: red.
    settle_picking(&mut state, 3, Some(elves));
    damage(&mut state, swiftspear, elves, 1);
    assert_eq!(state.objects.get(elves).damage, 0, "protection from red");
}

#[test]
fn shatterskull_smashing_divides_its_damage_and_is_a_land() {
    let mut state = islands();
    let first = put(&mut state, P1, "Monastery Swiftspear", Zone::Battlefield);
    let second = put(&mut state, P1, "Monastery Swiftspear", Zone::Battlefield);
    let smashing = put(&mut state, P0, "Shatterskull Smashing", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R, ManaColor::R], 2);
    assert!(castable(&mut state, smashing));
    act(&mut state, Action::CastSpell(smashing));
    let mut picks = 0;
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseTargets { legal_targets, .. } if picks < 2 => {
            let target = if picks == 0 { first } else { second };
            assert!(legal_targets.contains(&Target::Object(target)));
            picks += 1;
            Action::ChooseTarget(Target::Object(target))
        }
        Decision::ChooseTargets { .. } => Action::FinishEffectSelection,
        Decision::ChooseEffectOption { option_count, .. } => {
            Action::ChooseEffectOption(option_count - 1)
        }
        other => first_answer(other),
    });
    // X is 2: one damage to each.
    assert_eq!(state.objects.get(first).damage, 1);
    assert_eq!(state.objects.get(second).damage, 1);
    assert_eq!(zone(&state, smashing), Zone::Graveyard);

    let mut state = islands();
    let smashing = put(&mut state, P0, "Shatterskull Smashing", Zone::Hand);
    next(&mut state);
    act(&mut state, Action::PlayLand(smashing));
    // Pay 3 life: it enters untapped.
    settle_picking(&mut state, 1, None);
    assert_eq!(zone(&state, smashing), Zone::Battlefield);
    assert!(!state.objects.get(smashing).tapped);
    assert_eq!(state.players[0].life, 17);
    next(&mut state);
    act(&mut state, Action::ActivateManaAbility(smashing));
    assert_eq!(mana(&state, P0, ManaColor::R), 1);
}

#[test]
fn sundering_eruption_destroys_a_land_and_grounds_blockers() {
    let mut state = game(&["Forest", "Island", "Island", "Island", "Island"]);
    let taiga = put(&mut state, P1, "Taiga", Zone::Battlefield);
    let elves = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    let swiftspear = put(&mut state, P0, "Monastery Swiftspear", Zone::Battlefield);
    let eruption = put(&mut state, P0, "Sundering Eruption", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 2);
    cast(&mut state, eruption, &[Target::Object(taiga)]);
    settle_taking(&mut state);
    assert_eq!(zone(&state, taiga), Zone::Graveyard);
    assert_eq!(controls(&state, P1, "Forest").len(), 1, "searched");
    let mut blockers_offered = None;
    attack_with(&mut state, vec![swiftspear], |_, decision| {
        if let Decision::DeclareBlockers { legal_blockers, .. } = decision {
            blockers_offered = Some(
                legal_blockers
                    .iter()
                    .any(|(_, blockers)| blockers.contains(&elves)),
            );
            return Some(Action::DeclareBlockers(Vec::new()));
        }
        None
    });
    assert_eq!(blockers_offered, Some(false), "elves can't block");

    let mut state = islands();
    let eruption = put(&mut state, P0, "Sundering Eruption", Zone::Hand);
    next(&mut state);
    act(&mut state, Action::PlayLand(eruption));
    settle_picking(&mut state, 0, None);
    assert_eq!(zone(&state, eruption), Zone::Battlefield);
    assert!(state.objects.get(eruption).tapped, "didn't pay life");
}

#[test]
fn tolaria_taps_for_blue_and_activates_only_in_upkeep() {
    let mut state = islands();
    let tolaria = put(&mut state, P0, "Tolaria", Zone::Battlefield);
    put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    assert!(!activatable(&mut state, tolaria, 0), "main phase");
    state.step = Step::Upkeep;
    assert!(activatable(&mut state, tolaria, 0));
    state.step = Step::Main1;
    next(&mut state);
    act(&mut state, Action::ActivateManaAbility(tolaria));
    assert_eq!(mana(&state, P0, ManaColor::U), 1);
}

// ---- Static abilities --------------------------------------------------------------

#[test]
fn anger_gives_haste_from_the_graveyard_with_a_mountain() {
    let mut state = islands();
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    state.objects.get_mut(elves).summoning_sick = true;
    put(&mut state, P0, "Anger", Zone::Graveyard);
    assert!(!keyword(&state, elves, Keywords::HASTE), "no Mountain");
    put(&mut state, P0, "Mountain", Zone::Battlefield);
    assert!(keyword(&state, elves, Keywords::HASTE));
    let theirs = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    assert!(!keyword(&state, theirs, Keywords::HASTE));
}

#[test]
fn blood_moon_makes_nonbasic_lands_mountains() {
    let mut state = islands();
    let volcanic = put(&mut state, P0, "Volcanic Island", Zone::Battlefield);
    let moon = put(&mut state, P1, "Blood Moon", Zone::Hand);
    state.active_player = P1;
    state.priority_player = P1;
    pool(&mut state, P1, &[ManaColor::R], 2);
    cast(&mut state, moon, &[]);
    settled(&mut state);
    assert!(has_subtype(&state, volcanic, Subtype::Mountain));
    assert!(!has_subtype(&state, volcanic, Subtype::Island));
    state.active_player = P0;
    state.priority_player = P0;
    next(&mut state);
    act(&mut state, Action::ActivateManaAbility(volcanic));
    assert_eq!(mana(&state, P0, ManaColor::R), 1);
    assert_eq!(
        mana(&state, P0, ManaColor::U),
        0,
        "its own abilities are gone"
    );
}

#[test]
fn magus_of_the_moon_makes_nonbasic_lands_mountains() {
    let mut state = islands();
    let volcanic = put(&mut state, P0, "Volcanic Island", Zone::Battlefield);
    let island = put(&mut state, P0, "Island", Zone::Battlefield);
    let magus = put(&mut state, P1, "Magus of the Moon", Zone::Hand);
    state.active_player = P1;
    state.priority_player = P1;
    pool(&mut state, P1, &[ManaColor::R], 2);
    cast(&mut state, magus, &[]);
    settled(&mut state);
    assert!(has_subtype(&state, volcanic, Subtype::Mountain));
    assert!(!has_subtype(&state, volcanic, Subtype::Island));
    assert!(has_subtype(&state, island, Subtype::Island), "basic");
    to_graveyard(&mut state, magus);
    settled(&mut state);
    assert!(has_subtype(&state, volcanic, Subtype::Island));
}

#[test]
fn ensnaring_bridge_stops_creatures_bigger_than_its_controllers_hand() {
    let mut state = islands();
    put(&mut state, P0, "Ensnaring Bridge", Zone::Battlefield);
    put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    let ragavan = put(
        &mut state,
        P0,
        "Ragavan, Nimble Pilferer",
        Zone::Battlefield,
    );
    let eligible = eligible_attackers(&mut state);
    assert!(eligible.contains(&elves), "power 1, one card in hand");
    assert!(!eligible.contains(&ragavan), "power 2");
}

#[test]
fn painters_servant_colors_every_card() {
    let mut state = islands();
    let bolt = put(&mut state, P1, "Lightning Bolt", Zone::Hand);
    let blue = 1 << ManaColor::U.pool_index();
    assert_eq!(engine::object_color_mask(&state, bolt) & blue, 0);
    let painter = put(&mut state, P0, "Painter's Servant", Zone::Hand);
    pool(&mut state, P0, &[], 2);
    cast(&mut state, painter, &[]);
    // White, blue, black, red, green: blue.
    settle_picking(&mut state, 1, None);
    assert_eq!(
        state.objects.get(painter).v4.chosen_color,
        Some(ManaColor::U)
    );
    assert_ne!(engine::object_color_mask(&state, bolt) & blue, 0);
    assert_ne!(engine::object_color_mask(&state, painter) & blue, 0);
    let red = 1 << ManaColor::R.pool_index();
    assert_ne!(
        engine::object_color_mask(&state, bolt) & red,
        0,
        "still red"
    );
}

// ---- Creatures ---------------------------------------------------------------------

#[test]
fn ashen_rider_exiles_a_permanent_on_entering_and_dying() {
    let mut state = islands();
    let first = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    let second = put(&mut state, P1, "Island", Zone::Battlefield);
    let rider = put(&mut state, P0, "Ashen Rider", Zone::Hand);
    enter(&mut state, rider);
    settle_picking(&mut state, 0, Some(first));
    assert_eq!(zone(&state, first), Zone::Exile);
    to_graveyard(&mut state, rider);
    settle_picking(&mut state, 0, Some(second));
    assert_eq!(zone(&state, second), Zone::Exile);
}

#[test]
fn broadside_bombardiers_boasts_after_attacking() {
    let mut state = islands();
    let bombardiers = put(&mut state, P0, "Broadside Bombardiers", Zone::Battlefield);
    let star = put(&mut state, P0, "Chromatic Star", Zone::Battlefield);
    assert!(
        !activatable(&mut state, bombardiers, 0),
        "it hasn't attacked"
    );
    attack_with(&mut state, vec![bombardiers], |_, _| None);
    assert_eq!(state.players[1].life, 18);
    act(&mut state, Action::ActivateAbility(bombardiers, 0));
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseCostTargets { candidates, .. } => {
            assert_eq!(candidates, vec![star]);
            Action::ChooseCostTarget(star)
        }
        Decision::ChooseTargets { .. } => Action::ChooseTarget(Target::Player(P1)),
        other => first_answer(other),
    });
    assert_eq!(zone(&state, star), Zone::Graveyard);
    // 2 plus Chromatic Star's mana value 1.
    assert_eq!(state.players[1].life, 15);
}

#[test]
fn caves_of_chaos_adventurer_takes_the_initiative_and_plays_its_exile() {
    let mut state = game(&["Lightning Bolt"; 20]);
    let caves = put(&mut state, P0, "Caves of Chaos Adventurer", Zone::Hand);
    enter(&mut state, caves);
    settle_taking(&mut state);
    state.objects.get_mut(caves).summoning_sick = false;
    assert_eq!(state.initiative, Some(P0));
    let top = state.players[0].library[0];
    attack_with(&mut state, vec![caves], |_, _| None);
    assert_eq!(zone(&state, top), Zone::Exile);
    assert!(state
        .engine
        .exile_play_permissions
        .iter()
        .any(|permission| permission.object == top && permission.holder == P0));
    pool(&mut state, P0, &[ManaColor::R], 0);
    assert!(castable(&mut state, top));
}

#[test]
fn cephalid_illusionist_mills_when_targeted_and_prevents_combat_damage() {
    let mut state = islands();
    let illusionist = put(&mut state, P0, "Cephalid Illusionist", Zone::Battlefield);
    let nomads = put(&mut state, P0, "Nomads en-Kor", Zone::Battlefield);
    activate(&mut state, nomads, 0, &[Target::Object(illusionist)]);
    settle_ordering(&mut state);
    assert_eq!(state.players[0].graveyard.len(), 3);
    pool(&mut state, P0, &[ManaColor::U], 2);
    activate(&mut state, illusionist, 0, &[Target::Object(illusionist)]);
    settle_ordering(&mut state);
    assert_eq!(state.players[0].graveyard.len(), 6);
    assert!(state.objects.get(illusionist).tapped);
}

#[test]
fn chaos_defiler_destroys_a_chosen_opposing_permanent() {
    let mut state = islands();
    let elves = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    let island = put(&mut state, P1, "Island", Zone::Battlefield);
    let defiler = put(&mut state, P0, "Chaos Defiler", Zone::Hand);
    enter(&mut state, defiler);
    settle_picking(&mut state, 0, Some(elves));
    assert_eq!(zone(&state, elves), Zone::Graveyard);
    assert_eq!(zone(&state, island), Zone::Battlefield, "nonland only");
    assert!(keyword(&state, defiler, Keywords::TRAMPLE));
}

#[test]
fn elvish_reclaimer_grows_and_trades_a_land_for_any_land() {
    let mut state = game(&["Taiga", "Island", "Island", "Island", "Island"]);
    let reclaimer = put(&mut state, P0, "Elvish Reclaimer", Zone::Battlefield);
    let forest = put(&mut state, P0, "Forest", Zone::Battlefield);
    assert_eq!(power_toughness(&state, reclaimer), (1, 2));
    for _ in 0..2 {
        put(&mut state, P0, "Island", Zone::Graveyard);
    }
    assert_eq!(power_toughness(&state, reclaimer), (1, 2));
    pool(&mut state, P0, &[], 2);
    activate(&mut state, reclaimer, 0, &[]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseCostTargets { .. } => Action::ChooseCostTarget(forest),
        Decision::ChooseEffectTargets {
            legal_targets,
            selected_count: 0,
            ..
        } if !legal_targets.is_empty() => Action::ChooseEffectTarget(legal_targets[0]),
        other => first_answer(other),
    });
    assert_eq!(zone(&state, forest), Zone::Graveyard);
    let fetched = controls(&state, P0, "Taiga");
    assert_eq!(fetched.len(), 1);
    assert!(state.objects.get(fetched[0]).tapped);
    // Three land cards in the graveyard.
    assert_eq!(power_toughness(&state, reclaimer), (3, 4));
}

#[test]
fn endurance_puts_a_graveyard_on_the_bottom_of_its_library() {
    let mut state = islands();
    let first = put(&mut state, P1, "Lightning Bolt", Zone::Graveyard);
    let second = put(&mut state, P1, "Counterspell", Zone::Graveyard);
    let endurance = put(&mut state, P0, "Endurance", Zone::Hand);
    enter(&mut state, endurance);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseTargets { legal_targets, .. }
            if legal_targets.contains(&Target::Player(P1)) =>
        {
            Action::ChooseTarget(Target::Player(P1))
        }
        other => first_answer(other),
    });
    assert!(state.players[1].graveyard.is_empty());
    let library = &state.players[1].library;
    let bottom = &library[library.len() - 2..];
    assert!(bottom.contains(&first) && bottom.contains(&second));
}

#[test]
fn fury_divides_four_damage_and_can_be_evoked() {
    let mut state = islands();
    let first = put(&mut state, P1, "Monastery Swiftspear", Zone::Battlefield);
    let second = put(
        &mut state,
        P1,
        "Ragavan, Nimble Pilferer",
        Zone::Battlefield,
    );
    let fury = put(&mut state, P0, "Fury", Zone::Hand);
    enter(&mut state, fury);
    let mut picks = 0;
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseTargets { .. } => {
            picks += 1;
            // Three to the first, one to the second.
            Action::ChooseTarget(Target::Object(if picks <= 3 { first } else { second }))
        }
        other => first_answer(other),
    });
    assert_eq!(picks, 4);
    assert_eq!(zone(&state, first), Zone::Graveyard);
    assert_eq!(zone(&state, second), Zone::Graveyard);

    // Evoked: exile a red card, and it's sacrificed.
    let mut state = islands();
    let elves = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    let fury = put(&mut state, P0, "Fury", Zone::Hand);
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    cast(&mut state, fury, &[]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseCastMode { .. } => Action::ChooseCastMode(CastMode::Alternative),
        Decision::ChooseCostTargets { .. } => Action::ChooseCostTarget(bolt),
        Decision::ChooseTargets { .. } => Action::ChooseTarget(Target::Object(elves)),
        other => first_answer(other),
    });
    assert_eq!(zone(&state, bolt), Zone::Exile);
    assert_eq!(zone(&state, elves), Zone::Graveyard);
    assert_eq!(zone(&state, fury), Zone::Graveyard, "evoked");
}

#[test]
fn goblin_rabblemaster_makes_goblins_and_drives_them_to_attack() {
    let mut state = islands();
    let rabblemaster = put(&mut state, P0, "Goblin Rabblemaster", Zone::Battlefield);
    let welder = put(&mut state, P0, "Goblin Welder", Zone::Battlefield);
    pass_until(&mut state, |state| state.step == Step::DeclareAttackers);
    let tokens = controls(&state, P0, "Goblin Haste Token");
    assert_eq!(tokens.len(), 1);
    assert!(
        engine::step(&mut state, Action::DeclareAttackers(vec![rabblemaster])).is_err(),
        "the other Goblins must attack"
    );
    act(
        &mut state,
        Action::DeclareAttackers(vec![rabblemaster, welder, tokens[0]]),
    );
    run_until(
        &mut state,
        |state| state.step == Step::Main2 && state.stack.is_empty(),
        |_, _| None,
    );
    // +1/+0 for each of the two other attacking Goblins.
    assert_eq!(power_toughness(&state, rabblemaster), (4, 2));
}

#[test]
fn knight_of_the_reliquary_grows_and_trades_a_forest_for_a_land() {
    let mut state = game(&["Island", "Taiga", "Island", "Island"]);
    let knight = put(&mut state, P0, "Knight of the Reliquary", Zone::Battlefield);
    let forest = put(&mut state, P0, "Forest", Zone::Battlefield);
    put(&mut state, P0, "Island", Zone::Graveyard);
    assert_eq!(power_toughness(&state, knight), (3, 3));
    let taiga = state.players[0].library[1];
    act_activate_with_cost(&mut state, knight, forest, Some(taiga));
    assert_eq!(zone(&state, forest), Zone::Graveyard);
    assert_eq!(zone(&state, taiga), Zone::Battlefield);
    assert!(!state.objects.get(taiga).tapped);
    assert_eq!(power_toughness(&state, knight), (4, 4));
}

/// Activates `source`'s ability 0, paying its object cost with `cost` and
/// picking `found` from a search.
fn act_activate_with_cost(
    state: &mut GameState,
    source: ObjectId,
    cost: ObjectId,
    found: Option<ObjectId>,
) {
    activate(state, source, 0, &[]);
    settle_with(state, |_, decision| match decision {
        Decision::ChooseCostTargets { .. } => Action::ChooseCostTarget(cost),
        Decision::ChooseEffectTargets {
            legal_targets,
            selected_count: 0,
            ..
        } if found.is_some_and(|card| legal_targets.contains(&Target::Object(card))) => {
            Action::ChooseEffectTarget(Target::Object(found.unwrap()))
        }
        other => first_answer(other),
    });
}

#[test]
fn loki_draws_once_each_turn_when_its_controllers_ability_targets() {
    let mut state = islands();
    put(&mut state, P0, "Loki, God of Mischief", Zone::Battlefield);
    let nomads = put(&mut state, P0, "Nomads en-Kor", Zone::Battlefield);
    activate(&mut state, nomads, 0, &[Target::Object(nomads)]);
    settled(&mut state);
    assert_eq!(state.players[0].hand.len(), 1);
    activate(&mut state, nomads, 0, &[Target::Object(nomads)]);
    settled(&mut state);
    assert_eq!(state.players[0].hand.len(), 1, "once each turn");
}

#[test]
fn nomads_en_kor_redirects_the_next_damage() {
    let mut state = islands();
    let nomads = put(&mut state, P0, "Nomads en-Kor", Zone::Battlefield);
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    let swiftspear = put(&mut state, P1, "Monastery Swiftspear", Zone::Battlefield);
    activate(&mut state, nomads, 0, &[Target::Object(elves)]);
    settled(&mut state);
    damage(&mut state, swiftspear, nomads, 1);
    assert_eq!(state.objects.get(elves).damage, 1);
    assert_eq!(state.objects.get(nomads).damage, 0);
    settled(&mut state);
    assert_eq!(zone(&state, elves), Zone::Graveyard);
    damage(&mut state, swiftspear, nomads, 1);
    assert_eq!(state.objects.get(nomads).damage, 1, "the shield is used up");
}

#[test]
fn ox_of_agonas_refills_and_escapes_with_a_counter() {
    let mut state = islands();
    let first = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    let second = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    let ox = put(&mut state, P0, "Ox of Agonas", Zone::Hand);
    enter(&mut state, ox);
    settle_with(&mut state, |_, decision| first_answer(decision));
    assert_eq!(zone(&state, first), Zone::Graveyard);
    assert_eq!(zone(&state, second), Zone::Graveyard);
    assert_eq!(state.players[0].hand.len(), 3);

    let mut state = islands();
    for _ in 0..8 {
        put(&mut state, P0, "Island", Zone::Graveyard);
    }
    let ox = put(&mut state, P0, "Ox of Agonas", Zone::Graveyard);
    pool(&mut state, P0, &[ManaColor::R, ManaColor::R], 0);
    assert!(castable(&mut state, ox), "escape");
    act(&mut state, Action::CastSpell(ox));
    settle_with(&mut state, |_, decision| first_answer(decision));
    assert_eq!(zone(&state, ox), Zone::Battlefield);
    assert_eq!(state.objects.get(ox).counters.plus1_plus1, 1);
    assert_eq!(power_toughness(&state, ox), (5, 3));
}

#[test]
fn poxwalkers_return_when_a_spell_is_cast_from_the_graveyard() {
    let mut state = islands();
    let poxwalkers = put(&mut state, P0, "Poxwalkers", Zone::Graveyard);
    let gaze = put(&mut state, P0, "Otherworldly Gaze", Zone::Graveyard);
    pool(&mut state, P0, &[ManaColor::U], 1);
    assert!(castable(&mut state, gaze), "flashback");
    act(&mut state, Action::CastSpell(gaze));
    settle_with(&mut state, |_, decision| first_answer(decision));
    assert_eq!(zone(&state, poxwalkers), Zone::Battlefield);
    assert!(state.objects.get(poxwalkers).tapped);
    assert_eq!(zone(&state, gaze), Zone::Exile);
}

#[test]
fn samwise_returns_a_permanent_card_that_died_this_turn() {
    let mut state = islands();
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    let older = put(&mut state, P0, "Fyndhorn Elves", Zone::Graveyard);
    to_graveyard(&mut state, elves);
    settled(&mut state);
    let samwise = put(&mut state, P0, "Samwise the Stouthearted", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W], 1);
    cast(&mut state, samwise, &[]);
    let mut offered_older = false;
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseTargets { legal_targets, .. }
            if legal_targets.contains(&Target::Object(elves)) =>
        {
            offered_older = legal_targets.contains(&Target::Object(older));
            Action::ChooseTarget(Target::Object(elves))
        }
        other => first_answer(other),
    });
    assert!(!offered_older, "it was already in the graveyard");
    assert!(hand_contains(&state, P0, elves));
    assert_eq!(zone(&state, samwise), Zone::Battlefield);
}

#[test]
fn simian_spirit_guide_exiles_itself_for_red() {
    let mut state = islands();
    let guide = put(&mut state, P0, "Simian Spirit Guide", Zone::Hand);
    activate(&mut state, guide, 0, &[]);
    assert_eq!(zone(&state, guide), Zone::Exile);
    assert_eq!(mana(&state, P0, ManaColor::R), 1);
}

#[test]
fn super_shredder_grows_when_another_permanent_leaves() {
    let mut state = islands();
    let shredder = put(&mut state, P0, "Super Shredder", Zone::Battlefield);
    let elves = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    let island = put(&mut state, P0, "Island", Zone::Battlefield);
    to_graveyard(&mut state, elves);
    settled(&mut state);
    to_graveyard(&mut state, island);
    settled(&mut state);
    assert_eq!(state.objects.get(shredder).counters.plus1_plus1, 2);
    assert_eq!(power_toughness(&state, shredder), (3, 3));
}

#[test]
fn undermountain_adventurer_takes_the_initiative_and_taps_for_more_after_a_dungeon() {
    let mut state = islands();
    let adventurer = put(&mut state, P0, "Undermountain Adventurer", Zone::Hand);
    enter(&mut state, adventurer);
    settle_taking(&mut state);
    assert_eq!(state.initiative, Some(P0));
    state.objects.get_mut(adventurer).summoning_sick = false;
    next(&mut state);
    act(&mut state, Action::ActivateManaAbility(adventurer));
    assert_eq!(mana(&state, P0, ManaColor::G), 2);
    state.objects.get_mut(adventurer).tapped = false;
    state.players[0].dungeon.completed_dungeons.push(0);
    next(&mut state);
    act(&mut state, Action::ActivateManaAbility(adventurer));
    assert_eq!(mana(&state, P0, ManaColor::G), 8);
}

// ---- Spells ------------------------------------------------------------------------

#[test]
fn breakthrough_draws_four_and_keeps_x() {
    let mut state = islands();
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    let breakthrough = put(&mut state, P0, "Breakthrough", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 1);
    cast(&mut state, breakthrough, &[]);
    settle_with(&mut state, |_, decision| match decision {
        // X is 1.
        Decision::ChooseEffectOption { option_count, .. } => {
            Action::ChooseEffectOption(option_count - 1)
        }
        Decision::ChooseEffectTargets {
            selected_count: 0, ..
        } => Action::ChooseEffectTarget(Target::Object(bolt)),
        other => first_answer(other),
    });
    assert_eq!(state.players[0].hand, vec![bolt]);
    assert_eq!(state.players[0].graveyard.len(), 5);
}

#[test]
fn green_suns_zenith_finds_a_green_creature_and_shuffles_itself_in() {
    let mut state = game(&["Island", "Llanowar Elves", "Island", "Island"]);
    let elves = state.players[0].library[1];
    let zenith = put(&mut state, P0, "Green Sun's Zenith", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 1);
    cast(&mut state, zenith, &[]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseEffectOption { option_count, .. } => {
            Action::ChooseEffectOption(option_count - 1)
        }
        Decision::ChooseEffectTargets {
            legal_targets,
            selected_count: 0,
            ..
        } => {
            assert_eq!(legal_targets, vec![Target::Object(elves)]);
            Action::ChooseEffectTarget(Target::Object(elves))
        }
        other => first_answer(other),
    });
    assert_eq!(zone(&state, elves), Zone::Battlefield);
    assert_eq!(zone(&state, zenith), Zone::Library);
    assert_eq!(state.players[0].library.len(), 4);
}

#[test]
fn inquisition_of_kozilek_takes_a_cheap_nonland_card() {
    let mut state = islands();
    let bolt = put(&mut state, P1, "Lightning Bolt", Zone::Hand);
    let archon = put(&mut state, P1, "Archon of Cruelty", Zone::Hand);
    put(&mut state, P1, "Island", Zone::Hand);
    let inquisition = put(&mut state, P0, "Inquisition of Kozilek", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 0);
    cast(&mut state, inquisition, &[Target::Player(P1)]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseEffectTargets { legal_targets, .. } => {
            assert_eq!(legal_targets, vec![Target::Object(bolt)]);
            Action::ChooseEffectTarget(Target::Object(bolt))
        }
        other => first_answer(other),
    });
    assert_eq!(zone(&state, bolt), Zone::Graveyard);
    assert!(hand_contains(&state, P1, archon));
    assert!(known_to(&state, P0, P1, archon), "revealed");
}

#[test]
fn legolas_quick_reflexes_has_split_second_and_grants_a_tapped_trigger() {
    let mut state = islands();
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    state.objects.get_mut(elves).tapped = true;
    let swiftspear = put(&mut state, P1, "Monastery Swiftspear", Zone::Battlefield);
    let bolt = put(&mut state, P1, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P1, &[ManaColor::R], 0);
    let reflexes = put(&mut state, P0, "Legolas's Quick Reflexes", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 0);
    cast(&mut state, reflexes, &[Target::Object(elves)]);
    act(&mut state, Action::Pass);
    assert_eq!(state.priority_player, P1);
    assert!(!castable(&mut state, bolt), "split second");
    settled(&mut state);
    assert!(!state.objects.get(elves).tapped);
    assert!(keyword(&state, elves, Keywords::REACH));
    assert!(keyword(&state, elves, Keywords::HEXPROOF));
    next(&mut state);
    act(&mut state, Action::ActivateManaAbility(elves));
    settle_picking(&mut state, 0, Some(swiftspear));
    assert_eq!(state.objects.get(swiftspear).damage, 1);
}

#[test]
fn orims_chant_silences_a_player_and_kicked_stops_attacks() {
    let mut state = islands();
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    let bolt = put(&mut state, P1, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P1, &[ManaColor::R], 0);
    let chant = put(&mut state, P0, "Orim's Chant", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W, ManaColor::W], 0);
    assert!(castable(&mut state, chant));
    act(&mut state, Action::CastSpell(chant));
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseKicker { .. } => Action::ChooseKicker(true),
        Decision::ChooseTargets { .. } => Action::ChooseTarget(Target::Player(P1)),
        other => first_answer(other),
    });
    act(&mut state, Action::Pass);
    assert_eq!(state.priority_player, P1);
    assert!(
        !castable(&mut state, bolt),
        "P1 can't cast spells this turn"
    );
    let eligible = eligible_attackers(&mut state);
    assert!(!eligible.contains(&elves), "kicked: creatures can't attack");
}

#[test]
fn otherworldly_gaze_surveils_three_and_flashes_back() {
    let mut state = islands();
    let gaze = put(&mut state, P0, "Otherworldly Gaze", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 0);
    cast(&mut state, gaze, &[]);
    settle_with(&mut state, |_, decision| first_answer(decision));
    assert_eq!(zone(&state, gaze), Zone::Graveyard);
    pool(&mut state, P0, &[ManaColor::U], 1);
    cast(&mut state, gaze, &[]);
    settle_with(&mut state, |_, decision| first_answer(decision));
    assert_eq!(zone(&state, gaze), Zone::Exile);
}

// ---- Artifacts and enchantments ---------------------------------------------------

#[test]
fn bridge_from_below_makes_zombies_and_leaves_when_theirs_die() {
    let mut state = islands();
    let bridge = put(&mut state, P0, "Bridge from Below", Zone::Graveyard);
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    to_graveyard(&mut state, elves);
    settled(&mut state);
    assert_eq!(controls(&state, P0, "Zombie Token").len(), 1);
    let theirs = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    to_graveyard(&mut state, theirs);
    settled(&mut state);
    assert_eq!(zone(&state, bridge), Zone::Exile);
}

#[test]
fn grindstone_mills_again_while_the_cards_share_a_color() {
    let mut state = game(&[
        "Lightning Bolt",
        "Lightning Bolt",
        "Island",
        "Island",
        "Island",
    ]);
    let grindstone = put(&mut state, P0, "Grindstone", Zone::Battlefield);
    pool(&mut state, P0, &[], 3);
    activate(&mut state, grindstone, 0, &[Target::Player(P1)]);
    settle_ordering(&mut state);
    // Two red cards, then two colorless lands that share no color.
    assert_eq!(state.players[1].graveyard.len(), 4);
    assert_eq!(state.players[1].library.len(), 1);
}

#[test]
fn mishras_research_desk_offers_one_of_two_cards_and_unearths() {
    let mut state = game(&["Lightning Bolt", "Island", "Island", "Island"]);
    let bolt = state.players[0].library[0];
    let island = state.players[0].library[1];
    let desk = put(&mut state, P0, "Mishra's Research Desk", Zone::Battlefield);
    pool(&mut state, P0, &[], 1);
    activate(&mut state, desk, 0, &[]);
    settle_picking(&mut state, 0, Some(bolt));
    assert_eq!(zone(&state, bolt), Zone::Exile);
    assert_eq!(zone(&state, island), Zone::Exile);
    assert_eq!(zone(&state, desk), Zone::Graveyard);
    pool(&mut state, P0, &[ManaColor::R], 0);
    assert!(castable(&mut state, bolt));
    pool(&mut state, P0, &[ManaColor::R], 1);
    activate(&mut state, desk, 1, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, desk), Zone::Battlefield);
}

#[test]
fn shuko_equips_for_free() {
    let mut state = islands();
    let shuko = put(&mut state, P0, "Shuko", Zone::Battlefield);
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    activate(&mut state, shuko, 0, &[Target::Object(elves)]);
    settled(&mut state);
    assert_eq!(power_toughness(&state, elves), (2, 1));
}
