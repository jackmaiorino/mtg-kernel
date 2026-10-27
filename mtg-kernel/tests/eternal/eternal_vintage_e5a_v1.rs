//! Eternal wave E5a: native rules tests for the cards that complete Vintage
//! Raker Shops, Oath of Druids, Paradoxical Outcome, Blue Tinker and Beseech
//! Storm, and Legacy Mystic Forge.
#![cfg(feature = "eternal-fixtures")]

use mtg_kernel::card_def::{
    card_id_by_name, CardCapability, CardDef, CardType, Keywords, Subtype, Supertype, CARD_DEFS,
};
use mtg_kernel::engine::{self, Action, Decision};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::ManaColor;
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Target, Zone};

const P0: PlayerId = PlayerId::P0;
const P1: PlayerId = PlayerId::P1;

const E5A_CARDS: [&str; 28] = [
    "Beseech the Mirror",
    "Bolas's Citadel",
    "Chalice of the Void",
    "Council of Reeds",
    "Extrapolate the Impossible",
    "Flash",
    "Forbidden Orchard",
    "Glaring Fleshraker",
    "Grim Monolith",
    "Inventors' Fair",
    "Kozilek's Command",
    "Mystic Forge",
    "Oath of Druids",
    "Oko, Thief of Crowns",
    "Planar Nexus",
    "Scrawling Crawler",
    "Sensei's Divining Top",
    "Serra's Emissary",
    "Suplex",
    "Talon Gates of Madara",
    "Tendrils of Agony",
    "The Endstone",
    "The Fantasticar",
    "Ugin, Eye of the Storms",
    "Urza's Workshop",
    "Vampiric Tutor",
    "Voltaic Key",
    "Yawgmoth's Will",
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
        0x4535_4141,
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
        Zone::Library => seat.library.insert(0, object),
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

fn mana(state: &GameState, player: PlayerId, color: ManaColor) -> u8 {
    state.players[player.index()].mana_pool[color.pool_index()]
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
    for _ in 0..300 {
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
            can_finish: true, ..
        } => Action::FinishEffectSelection,
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

fn priority(state: &mut GameState) -> (Vec<ObjectId>, Vec<(ObjectId, u8)>, Vec<ObjectId>) {
    match next(state) {
        Decision::CastSpellOrPass {
            castable_spells,
            activatable_abilities,
            land_drops,
            ..
        } => (castable_spells, activatable_abilities, land_drops),
        other => panic!("expected priority, got {other:?}"),
    }
}

fn castable(state: &mut GameState, spell: ObjectId) -> bool {
    priority(state).0.contains(&spell)
}

fn activatable(state: &mut GameState, source: ObjectId, index: u8) -> bool {
    priority(state).1.contains(&(source, index))
}

fn playable_land(state: &mut GameState, land: ObjectId) -> bool {
    priority(state).2.contains(&land)
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
    for _ in 0..600 {
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

/// Passes to `player`'s next first main phase with an empty stack.
fn to_main(state: &mut GameState, player: PlayerId) {
    pass_until(state, |state| state.active_player != player);
    pass_until(state, |state| {
        state.active_player == player
            && state.step == Step::Main1
            && state.stack.is_empty()
            && state.engine.pending_triggers.is_empty()
    });
}

/// P1 becomes the active player in its first main phase, holding priority.
fn opponents_main_phase(state: &mut GameState) {
    state.active_player = P1;
    state.priority_player = P1;
    state.step = Step::Main1;
}

fn controls(state: &GameState, player: PlayerId, name: &str) -> usize {
    state.players[player.index()]
        .battlefield
        .iter()
        .filter(|&&object| name_of(state, object) == name)
        .count()
}

fn loyalty(state: &GameState, planeswalker: ObjectId) -> Option<u32> {
    mtg_kernel::planeswalker_v1::loyalty(state, planeswalker)
}

// ---- Characteristics -----------------------------------------------------------

#[test]
fn e5a_cards_are_full_with_their_printed_characteristics() {
    for name in E5A_CARDS {
        assert_eq!(def(name).capability, CardCapability::Full, "{name}");
        assert!(!def(name).is_token, "{name}");
    }
    let construct = def("Fantasticar Construct Token");
    assert_eq!(construct.capability, CardCapability::Full);
    assert!(construct.is_token);
    assert_eq!(construct.object_name, "Construct Token");
    assert_eq!((construct.power, construct.toughness), (Some(4), Some(4)));
    assert!(construct.keywords.has(Keywords::FLYING) && construct.keywords.has(Keywords::HASTE));
    assert_eq!(construct.types, &[CardType::Artifact, CardType::Creature]);

    let nexus = def("Planar Nexus");
    for subtype in [
        Subtype::Urzas,
        Subtype::Mine,
        Subtype::PowerPlant,
        Subtype::Tower,
        Subtype::Gate,
        Subtype::Cave,
        Subtype::Locus,
    ] {
        assert!(nexus.subtypes.contains(&subtype), "{subtype:?}");
    }
    assert_eq!(def("Chalice of the Void").cost.x_count, 2);
    assert_eq!(def("Chalice of the Void").mana_value, 0);
    assert_eq!(def("Kozilek's Command").cost.x_count, 1);
    assert_eq!(def("Kozilek's Command").cost.pips.len(), 2);
    assert!(def("Kozilek's Command").mode2.is_some() && def("Kozilek's Command").mode3.is_some());
    assert_eq!(def("Kozilek's Command").additional_modes.len(), 3);
    assert!(def("Suplex").mode2.is_some());
    assert!(def("Serra's Emissary").keywords.has(Keywords::FLYING));
    assert!(def("The Fantasticar").keywords.has(Keywords::FLYING));
    assert_eq!(def("The Fantasticar").subtypes, &[Subtype::Vehicle]);
    assert_eq!(def("Oko, Thief of Crowns").starting_loyalty, Some(4));
    assert_eq!(def("Ugin, Eye of the Storms").starting_loyalty, Some(7));
    assert_eq!(def("Oko, Thief of Crowns").activated_abilities.len(), 3);
    assert_eq!(def("Ugin, Eye of the Storms").activated_abilities.len(), 3);
    assert_eq!(def("Council of Reeds").supertypes, &[Supertype::Legendary]);
    assert!(def("Beseech the Mirror").optional_additional_cost.is_some());
    assert_eq!(
        def("Talon Gates of Madara").activated_abilities[0].activation_zone,
        Zone::Hand
    );
    assert!(def("Glaring Fleshraker").colors.is_empty());
}

// ---- Mana and artifacts --------------------------------------------------------

#[test]
fn grim_monolith_taps_for_three_stays_tapped_and_untaps_for_four() {
    let mut state = islands();
    let monolith = put(&mut state, P0, "Grim Monolith", Zone::Battlefield);
    next(&mut state);
    act(&mut state, Action::ActivateManaAbility(monolith));
    assert_eq!(mana(&state, P0, ManaColor::C), 3);
    assert!(state.objects.get(monolith).tapped);
    to_main(&mut state, P0);
    assert!(state.objects.get(monolith).tapped, "it doesn't untap");
    pool(&mut state, P0, &[], 4);
    activate(&mut state, monolith, 0, &[]);
    settled(&mut state);
    assert!(!state.objects.get(monolith).tapped);
}

#[test]
fn voltaic_key_untaps_target_artifact() {
    let mut state = islands();
    let key = put(&mut state, P0, "Voltaic Key", Zone::Battlefield);
    let monolith = put(&mut state, P0, "Grim Monolith", Zone::Battlefield);
    state.objects.get_mut(monolith).tapped = true;
    pool(&mut state, P0, &[], 1);
    activate(&mut state, key, 0, &[Target::Object(monolith)]);
    settled(&mut state);
    assert!(!state.objects.get(monolith).tapped);
    assert!(state.objects.get(key).tapped);
}

#[test]
fn senseis_divining_top_reorders_the_top_three_and_draws_then_goes_on_top() {
    let mut state = game(&["Island", "Swamp", "Mountain", "Forest", "Plains"]);
    let top = put(&mut state, P0, "Sensei's Divining Top", Zone::Battlefield);
    let before = state.players[0].library[..3].to_vec();
    pool(&mut state, P0, &[], 1);
    activate(&mut state, top, 0, &[]);
    settle_first(&mut state);
    let mut after = state.players[0].library[..3].to_vec();
    let mut expected = before.clone();
    after.sort();
    expected.sort();
    assert_eq!(after, expected, "the same three cards stay on top");

    let hand = state.players[0].hand.len();
    let library = state.players[0].library.len();
    activate(&mut state, top, 1, &[]);
    settled(&mut state);
    assert_eq!(state.players[0].hand.len(), hand + 1);
    assert_eq!(zone(&state, top), Zone::Library);
    assert_eq!(state.players[0].library[0], top);
    assert_eq!(state.players[0].library.len(), library);
}

#[test]
fn inventors_fair_gains_life_with_three_artifacts_and_searches_for_an_artifact() {
    let mut state = islands();
    let fair = put(&mut state, P0, "Inventors' Fair", Zone::Battlefield);
    let monolith = put(&mut state, P0, "Grim Monolith", Zone::Library);
    put(&mut state, P0, "Voltaic Key", Zone::Battlefield);
    put(&mut state, P0, "Sensei's Divining Top", Zone::Battlefield);
    pool(&mut state, P0, &[], 4);
    assert!(!activatable(&mut state, fair, 0), "two artifacts");
    put(&mut state, P0, "Mystic Forge", Zone::Battlefield);
    assert!(activatable(&mut state, fair, 0));
    activate(&mut state, fair, 0, &[]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseEffectTargets { legal_targets, .. }
            if legal_targets.contains(&Target::Object(monolith)) =>
        {
            Action::ChooseEffectTarget(Target::Object(monolith))
        }
        other => first_answer(other),
    });
    assert_eq!(zone(&state, monolith), Zone::Hand);
    assert_eq!(zone(&state, fair), Zone::Graveyard);

    // At the beginning of P0's upkeep with three artifacts: 1 life.
    let mut state = islands();
    put(&mut state, P0, "Inventors' Fair", Zone::Battlefield);
    for name in ["Voltaic Key", "Sensei's Divining Top", "Mystic Forge"] {
        put(&mut state, P0, name, Zone::Battlefield);
    }
    to_main(&mut state, P0);
    assert_eq!(state.players[0].life, 21);
}

#[test]
fn urzas_workshop_taps_for_its_urzas_lands_with_metalcraft() {
    let mut state = islands();
    let workshop = put(&mut state, P0, "Urza's Workshop", Zone::Battlefield);
    put(&mut state, P0, "Planar Nexus", Zone::Battlefield);
    next(&mut state);
    act(&mut state, Action::ActivateManaAbility(workshop));
    assert_eq!(mana(&state, P0, ManaColor::C), 1, "no metalcraft");

    let mut state = islands();
    let workshop = put(&mut state, P0, "Urza's Workshop", Zone::Battlefield);
    put(&mut state, P0, "Planar Nexus", Zone::Battlefield);
    for name in ["Voltaic Key", "Sensei's Divining Top", "Grim Monolith"] {
        put(&mut state, P0, name, Zone::Battlefield);
    }
    next(&mut state);
    act(&mut state, Action::ActivateManaAbility(workshop));
    assert_eq!(mana(&state, P0, ManaColor::C), 2, "two Urza's lands");
}

#[test]
fn planar_nexus_is_every_nonbasic_land_type_and_taps_for_any_color() {
    let mut state = islands();
    let tower = put(&mut state, P0, "Urza's Tower", Zone::Battlefield);
    let nexus = put(&mut state, P0, "Planar Nexus", Zone::Battlefield);
    assert!(engine::has_effective_subtype(&state, nexus, Subtype::Mine));
    assert!(engine::has_effective_subtype(
        &state,
        nexus,
        Subtype::PowerPlant
    ));
    next(&mut state);
    act(&mut state, Action::ActivateManaAbility(tower));
    assert_eq!(
        mana(&state, P0, ManaColor::C),
        3,
        "the Nexus is an Urza's Mine and Power-Plant"
    );
    // "{1}, {T}: Add one mana of any color."
    act(
        &mut state,
        Action::ActivateManaAbilityChoice(nexus, ManaColor::G),
    );
    assert_eq!(mana(&state, P0, ManaColor::G), 1);
    assert_eq!(mana(&state, P0, ManaColor::C), 2);
}

#[test]
fn forbidden_orchard_gives_the_opponent_a_spirit_when_tapped_for_mana() {
    let mut state = islands();
    let orchard = put(&mut state, P0, "Forbidden Orchard", Zone::Battlefield);
    next(&mut state);
    act(
        &mut state,
        Action::ActivateManaAbilityChoice(orchard, ManaColor::G),
    );
    assert_eq!(mana(&state, P0, ManaColor::G), 1);
    settled(&mut state);
    assert_eq!(controls(&state, P1, "Colorless Spirit Token"), 1);
    assert_eq!(controls(&state, P0, "Colorless Spirit Token"), 0);
}

// ---- Library and graveyard permissions ------------------------------------------

#[test]
fn mystic_forge_casts_artifact_and_colorless_spells_from_the_top_and_exiles_it() {
    let mut state = islands();
    put(&mut state, P0, "Mystic Forge", Zone::Battlefield);
    let monolith = put(&mut state, P0, "Grim Monolith", Zone::Library);
    pool(&mut state, P0, &[], 2);
    cast(&mut state, monolith, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, monolith), Zone::Battlefield);

    let raker = put(&mut state, P0, "Glaring Fleshraker", Zone::Library);
    pool(&mut state, P0, &[ManaColor::C], 2);
    assert!(castable(&mut state, raker), "a colorless creature spell");
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Library);
    pool(&mut state, P0, &[ManaColor::R], 0);
    assert!(!castable(&mut state, bolt), "red and not an artifact");
    let under = state.players[0].library[1];
    assert_eq!(under, raker);

    // "{T}, Pay 1 life: Exile the top card of your library."
    let mut state = islands();
    let forge = put(&mut state, P0, "Mystic Forge", Zone::Battlefield);
    let top = state.players[0].library[0];
    activate(&mut state, forge, 0, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, top), Zone::Exile);
    assert_eq!(state.players[0].life, 19);
}

#[test]
fn bolas_citadel_casts_from_the_top_paying_life_and_plays_lands_from_it() {
    let mut state = islands();
    put(&mut state, P0, "Bolas's Citadel", Zone::Battlefield);
    let ritual = put(&mut state, P0, "Dark Ritual", Zone::Library);
    assert!(castable(&mut state, ritual), "no mana needed");
    cast(&mut state, ritual, &[]);
    settled(&mut state);
    assert_eq!(state.players[0].life, 19, "life equal to its mana value");
    assert_eq!(mana(&state, P0, ManaColor::B), 3);
    assert_eq!(zone(&state, ritual), Zone::Graveyard);

    // The top card is a land: it can be played.
    let top = state.players[0].library[0];
    assert_eq!(name_of(&state, top), "Island");
    assert!(playable_land(&mut state, top));
    act(&mut state, Action::PlayLand(top));
    assert_eq!(zone(&state, top), Zone::Battlefield);

    // "{T}, Sacrifice ten nonland permanents: Each opponent loses 10 life."
    let mut state = islands();
    let citadel = put(&mut state, P0, "Bolas's Citadel", Zone::Battlefield);
    for _ in 0..8 {
        put(&mut state, P0, "Voltaic Key", Zone::Battlefield);
    }
    put(&mut state, P0, "Island", Zone::Battlefield);
    assert!(
        !activatable(&mut state, citadel, 0),
        "the Citadel, eight others and a land"
    );
    put(&mut state, P0, "Sensei's Divining Top", Zone::Battlefield);
    activate(&mut state, citadel, 0, &[]);
    settle_first(&mut state);
    assert_eq!(state.players[1].life, 10);
}

#[test]
fn yawgmoths_will_casts_and_plays_from_the_graveyard_and_exiles_instead() {
    let mut state = islands();
    let will = put(&mut state, P0, "Yawgmoth's Will", Zone::Hand);
    let ritual = put(&mut state, P0, "Dark Ritual", Zone::Graveyard);
    let land = put(&mut state, P0, "Swamp", Zone::Graveyard);
    pool(&mut state, P0, &[ManaColor::B], 2);
    cast(&mut state, will, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, will), Zone::Exile, "it exiles itself");
    pool(&mut state, P0, &[ManaColor::B], 0);
    cast(&mut state, ritual, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, ritual), Zone::Exile);
    assert!(playable_land(&mut state, land));
    act(&mut state, Action::PlayLand(land));
    assert_eq!(zone(&state, land), Zone::Battlefield);
    // Another card put into the graveyard is exiled instead.
    let key = put(&mut state, P0, "Voltaic Key", Zone::Battlefield);
    mtg_kernel::event::propose_and_commit(
        &mut state,
        mtg_kernel::event::ProposedEvent::zone_change(key, Zone::Graveyard),
    );
    assert_eq!(zone(&state, key), Zone::Exile);
    // Next turn the permission and the replacement are over.
    to_main(&mut state, P1);
    let other = put(&mut state, P0, "Voltaic Key", Zone::Battlefield);
    mtg_kernel::event::propose_and_commit(
        &mut state,
        mtg_kernel::event::ProposedEvent::zone_change(other, Zone::Graveyard),
    );
    assert_eq!(zone(&state, other), Zone::Graveyard);
}

#[test]
fn vampiric_tutor_puts_the_card_on_top_known_only_to_its_controller() {
    let mut state = islands();
    let tutor = put(&mut state, P0, "Vampiric Tutor", Zone::Hand);
    let wanted = state.players[0].library[10];
    pool(&mut state, P0, &[ManaColor::B], 0);
    cast(&mut state, tutor, &[]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseEffectTargets { legal_targets, .. }
            if legal_targets.contains(&Target::Object(wanted)) =>
        {
            Action::ChooseEffectTarget(Target::Object(wanted))
        }
        other => first_answer(other),
    });
    assert_eq!(state.players[0].library[0], wanted);
    assert_eq!(state.players[0].life, 18);
    assert!(state
        .known_library_cards(P0, P0)
        .iter()
        .any(|entry| entry.object == wanted && entry.position == 0));
    assert!(!state
        .known_library_cards(P1, P0)
        .iter()
        .any(|entry| entry.object == wanted));
}

#[test]
fn beseech_the_mirror_finds_a_card_and_casts_it_free_when_bargained() {
    // Not bargained: the card goes to the hand.
    let mut state = islands();
    let beseech = put(&mut state, P0, "Beseech the Mirror", Zone::Hand);
    let ritual = put(&mut state, P0, "Dark Ritual", Zone::Library);
    pool(
        &mut state,
        P0,
        &[ManaColor::B, ManaColor::B, ManaColor::B],
        1,
    );
    act(&mut state, Action::CastSpell(beseech));
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseEffectTargets { legal_targets, .. }
            if legal_targets.contains(&Target::Object(ritual)) =>
        {
            Action::ChooseEffectTarget(Target::Object(ritual))
        }
        other => first_answer(other),
    });
    assert_eq!(zone(&state, ritual), Zone::Hand);

    // Bargained (a sacrificed artifact): the card may be cast for free.
    let mut state = islands();
    let beseech = put(&mut state, P0, "Beseech the Mirror", Zone::Hand);
    let key = put(&mut state, P0, "Voltaic Key", Zone::Battlefield);
    let ritual = put(&mut state, P0, "Dark Ritual", Zone::Library);
    pool(
        &mut state,
        P0,
        &[ManaColor::B, ManaColor::B, ManaColor::B],
        1,
    );
    act(&mut state, Action::CastSpell(beseech));
    let mut cast_offered = false;
    settle_with(&mut state, |state, decision| match decision {
        // Pay the bargain.
        Decision::ChooseEffectOption { source, .. } if source == beseech => {
            Action::ChooseEffectOption(1)
        }
        Decision::ChooseCostTargets { candidates, .. } => {
            assert!(candidates.contains(&key));
            Action::ChooseCostTarget(key)
        }
        Decision::ChooseEffectTargets { legal_targets, .. }
            if legal_targets.contains(&Target::Object(ritual)) =>
        {
            if zone(state, ritual) == Zone::Hand {
                cast_offered = true;
            }
            Action::ChooseEffectTarget(Target::Object(ritual))
        }
        other => first_answer(other),
    });
    assert!(cast_offered, "the free cast was offered");
    assert_eq!(zone(&state, key), Zone::Graveyard);
    assert_eq!(zone(&state, ritual), Zone::Graveyard, "cast and resolved");
    assert_eq!(mana(&state, P0, ManaColor::B), 3);
}

#[test]
fn extrapolate_the_impossible_finds_nothing_outside_a_best_of_one_game() {
    let mut state = islands();
    let extrapolate = put(&mut state, P0, "Extrapolate the Impossible", Zone::Hand);
    let hand = state.players[0].hand.len();
    pool(&mut state, P0, &[ManaColor::B], 1);
    cast(&mut state, extrapolate, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, extrapolate), Zone::Graveyard);
    assert_eq!(state.players[0].hand.len(), hand - 1);
}

// ---- Spells --------------------------------------------------------------------

#[test]
fn chalice_of_the_void_enters_with_x_counters_and_counters_matching_spells() {
    let mut state = islands();
    let chalice = put(&mut state, P0, "Chalice of the Void", Zone::Hand);
    pool(&mut state, P0, &[], 2);
    act(&mut state, Action::CastSpell(chalice));
    match next(&mut state) {
        Decision::ChooseEffectOption { option_count, .. } => {
            assert_eq!(option_count, 2, "X is 0 or 1");
            act(&mut state, Action::ChooseEffectOption(1));
        }
        other => panic!("expected the X choice, got {other:?}"),
    }
    settled(&mut state);
    assert_eq!(zone(&state, chalice), Zone::Battlefield);
    assert_eq!(state.objects.get(chalice).counters.charge, 1);

    // P1's mana value 1 spell is countered; a mana value 2 spell is not.
    state.priority_player = P1;
    let bolt = put(&mut state, P1, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P1, &[ManaColor::R], 0);
    cast(&mut state, bolt, &[Target::Player(P0)]);
    settled(&mut state);
    assert_eq!(zone(&state, bolt), Zone::Graveyard);
    assert_eq!(state.players[0].life, 20, "countered");
    state.priority_player = P1;
    let monolith = put(&mut state, P1, "Grim Monolith", Zone::Hand);
    pool(&mut state, P1, &[], 2);
    state.active_player = P1;
    cast(&mut state, monolith, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, monolith), Zone::Battlefield);
}

#[test]
fn flash_puts_a_creature_onto_the_battlefield_and_sacrifices_it_unless_paid() {
    for pay in [true, false] {
        let mut state = islands();
        let flash = put(&mut state, P0, "Flash", Zone::Hand);
        let raker = put(&mut state, P0, "Glaring Fleshraker", Zone::Hand);
        pool(&mut state, P0, &[ManaColor::U, ManaColor::C], 1);
        cast(&mut state, flash, &[]);
        settle_with(&mut state, |_, decision| match decision {
            Decision::ChooseEffectTargets { legal_targets, .. }
                if legal_targets.contains(&Target::Object(raker)) =>
            {
                Action::ChooseEffectTarget(Target::Object(raker))
            }
            Decision::ChooseEffectOption { option_count, .. } => {
                assert_eq!(option_count, 2);
                Action::ChooseEffectOption(if pay { 0 } else { 1 })
            }
            other => first_answer(other),
        });
        let expected = if pay {
            Zone::Battlefield
        } else {
            Zone::Graveyard
        };
        assert_eq!(zone(&state, raker), expected, "pay {pay}");
        if pay {
            assert_eq!(mana(&state, P0, ManaColor::C), 0, "{{2}}{{C}} less {{2}}");
        }
    }
}

#[test]
fn suplex_exiles_a_creature_that_would_die_or_exiles_an_artifact() {
    let mut state = islands();
    let suplex = put(&mut state, P0, "Suplex", Zone::Hand);
    let raker = put(&mut state, P1, "Glaring Fleshraker", Zone::Battlefield);
    pool(&mut state, P0, &[ManaColor::R], 1);
    act(&mut state, Action::CastSpell(suplex));
    match next(&mut state) {
        Decision::ChooseSpellMode { .. } => act(&mut state, Action::ChooseSpellMode(0)),
        other => panic!("expected the mode, got {other:?}"),
    }
    choose_target(&mut state, Target::Object(raker));
    settled(&mut state);
    assert_eq!(zone(&state, raker), Zone::Exile, "exiled instead of dying");

    let mut state = islands();
    let suplex = put(&mut state, P0, "Suplex", Zone::Hand);
    let key = put(&mut state, P1, "Voltaic Key", Zone::Battlefield);
    pool(&mut state, P0, &[ManaColor::R], 1);
    act(&mut state, Action::CastSpell(suplex));
    match next(&mut state) {
        Decision::ChooseSpellMode { .. } => act(&mut state, Action::ChooseSpellMode(1)),
        other => panic!("expected the mode, got {other:?}"),
    }
    choose_target(&mut state, Target::Object(key));
    settled(&mut state);
    assert_eq!(zone(&state, key), Zone::Exile);
}

#[test]
fn tendrils_of_agony_drains_two_for_each_copy() {
    let mut state = islands();
    for _ in 0..2 {
        let ritual = put(&mut state, P0, "Dark Ritual", Zone::Hand);
        pool(&mut state, P0, &[ManaColor::B], 0);
        cast(&mut state, ritual, &[]);
        settled(&mut state);
    }
    let tendrils = put(&mut state, P0, "Tendrils of Agony", Zone::Hand);
    cast(&mut state, tendrils, &[Target::Player(P1)]);
    settle_first(&mut state);
    assert_eq!(state.players[1].life, 14, "the spell and two copies");
    assert_eq!(state.players[0].life, 26);
}

#[test]
fn kozilek_command_makes_spawn_and_scries_or_exiles_a_creature_and_cards() {
    // Spawn tokens for X and scry X then draw, both for P0, X = 2.
    let mut state = islands();
    let command = put(&mut state, P0, "Kozilek's Command", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::C, ManaColor::C], 2);
    let hand = state.players[0].hand.len();
    act(&mut state, Action::CastSpell(command));
    match next(&mut state) {
        Decision::ChooseSpellMode { legal_modes, .. } => {
            // No creature is on the battlefield, so only the three pairs
            // without "exile target creature" can be chosen.
            assert_eq!(legal_modes, vec![0, 2, 4]);
            act(&mut state, Action::ChooseSpellMode(0));
        }
        other => panic!("expected the modes, got {other:?}"),
    }
    choose_target(&mut state, Target::Player(P0));
    choose_target(&mut state, Target::Player(P0));
    match next(&mut state) {
        Decision::ChooseEffectOption { option_count, .. } => {
            assert_eq!(option_count, 3, "X is 0, 1 or 2");
            act(&mut state, Action::ChooseEffectOption(2));
        }
        other => panic!("expected the X choice, got {other:?}"),
    }
    settle_first(&mut state);
    assert_eq!(controls(&state, P0, "Eldrazi Spawn Token"), 2);
    assert_eq!(state.players[0].hand.len(), hand, "cast one, drew one");

    // Exile a creature with mana value X or less and X graveyard cards: X
    // must cover both.
    let mut state = islands();
    let command = put(&mut state, P0, "Kozilek's Command", Zone::Hand);
    let raker = put(&mut state, P1, "Glaring Fleshraker", Zone::Battlefield);
    let first = put(&mut state, P1, "Lightning Bolt", Zone::Graveyard);
    let second = put(&mut state, P0, "Dark Ritual", Zone::Graveyard);
    pool(&mut state, P0, &[ManaColor::C, ManaColor::C], 4);
    act(&mut state, Action::CastSpell(command));
    match next(&mut state) {
        Decision::ChooseSpellMode { .. } => act(&mut state, Action::ChooseSpellMode(5)),
        other => panic!("expected the modes, got {other:?}"),
    }
    choose_target(&mut state, Target::Object(raker));
    choose_target(&mut state, Target::Object(first));
    choose_target(&mut state, Target::Object(second));
    match next(&mut state) {
        Decision::ChooseTargets {
            can_finish: true, ..
        } => act(&mut state, Action::FinishEffectSelection),
        other => panic!("expected more optional targets, got {other:?}"),
    }
    match next(&mut state) {
        Decision::ChooseEffectOption { option_count, .. } => {
            assert_eq!(option_count, 2, "X is 3 or 4");
            act(&mut state, Action::ChooseEffectOption(0));
        }
        other => panic!("expected the X choice, got {other:?}"),
    }
    settle_first(&mut state);
    assert_eq!(zone(&state, raker), Zone::Exile);
    assert_eq!(zone(&state, first), Zone::Exile);
    assert_eq!(zone(&state, second), Zone::Exile);
}

// ---- Triggered abilities -------------------------------------------------------

#[test]
fn glaring_fleshraker_makes_spawn_on_colorless_spells_and_pings_on_colorless_creatures() {
    let mut state = islands();
    put(&mut state, P0, "Glaring Fleshraker", Zone::Battlefield);
    let key = put(&mut state, P0, "Voltaic Key", Zone::Hand);
    pool(&mut state, P0, &[], 1);
    cast(&mut state, key, &[]);
    settled(&mut state);
    assert_eq!(controls(&state, P0, "Eldrazi Spawn Token"), 1);
    assert_eq!(state.players[1].life, 19, "the colorless Spawn entered");
    let ritual = put(&mut state, P0, "Dark Ritual", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 0);
    cast(&mut state, ritual, &[]);
    settled(&mut state);
    assert_eq!(
        controls(&state, P0, "Eldrazi Spawn Token"),
        1,
        "a black spell"
    );
}

#[test]
fn scrawling_crawler_makes_each_player_draw_and_punishes_opponent_draws() {
    let mut state = islands();
    put(&mut state, P0, "Scrawling Crawler", Zone::Battlefield);
    let hands = [state.players[0].hand.len(), state.players[1].hand.len()];
    to_main(&mut state, P1);
    assert_eq!(state.players[1].life, 19, "P1's draw step");
    to_main(&mut state, P0);
    assert_eq!(state.players[1].life, 18, "P0's upkeep: each player draws");
    assert_eq!(state.players[0].hand.len(), hands[0] + 2);
    assert_eq!(state.players[1].hand.len(), hands[1] + 2);
    assert_eq!(state.players[0].life, 20);
}

#[test]
fn the_endstone_draws_for_spells_and_lands_and_sets_life_at_the_end_step() {
    let mut state = islands();
    put(&mut state, P0, "The Endstone", Zone::Battlefield);
    let hand = state.players[0].hand.len();
    let key = put(&mut state, P0, "Voltaic Key", Zone::Hand);
    pool(&mut state, P0, &[], 1);
    cast(&mut state, key, &[]);
    settled(&mut state);
    assert_eq!(state.players[0].hand.len(), hand + 1);
    let land = put(&mut state, P0, "Swamp", Zone::Hand);
    next(&mut state);
    act(&mut state, Action::PlayLand(land));
    settled(&mut state);
    assert_eq!(state.players[0].hand.len(), hand + 2);
    pass_until(&mut state, |state| state.active_player == P1);
    assert_eq!(state.players[0].life, 10);
}

#[test]
fn council_of_reeds_copies_itself_after_a_noncreature_spell_and_ignores_the_legend_rule() {
    let mut state = islands();
    put(&mut state, P0, "Council of Reeds", Zone::Battlefield);
    pass_until(&mut state, |state| state.step == Step::Main2);
    assert_eq!(controls(&state, P0, "Council of Reeds"), 1, "no spell yet");

    let mut state = islands();
    put(&mut state, P0, "Council of Reeds", Zone::Battlefield);
    let key = put(&mut state, P0, "Voltaic Key", Zone::Hand);
    pool(&mut state, P0, &[], 1);
    cast(&mut state, key, &[]);
    settled(&mut state);
    pass_until(&mut state, |state| state.step == Step::Main2);
    assert_eq!(controls(&state, P0, "Council of Reeds"), 2);
    let copy = *state.players[0]
        .battlefield
        .iter()
        .find(|&&object| {
            name_of(&state, object) == "Council of Reeds" && state.objects.get(object).v4.is_token
        })
        .expect("a token copy");
    assert!(engine::object_has_type(&state, copy, CardType::Creature));
}

#[test]
fn the_fantasticar_becomes_a_creature_and_turns_into_four_constructs() {
    let mut state = islands();
    let car = put(&mut state, P0, "The Fantasticar", Zone::Battlefield);
    assert!(!engine::object_has_type(&state, car, CardType::Creature));
    let mut casts = 0;
    for _ in 0..4 {
        let key = put(&mut state, P0, "Voltaic Key", Zone::Hand);
        pool(&mut state, P0, &[], 1);
        cast(&mut state, key, &[]);
        casts += 1;
        // Animate it; on the fourth spell, sacrifice it.
        settle_with(&mut state, |_, decision| match decision {
            Decision::ChooseEffectOption { .. } => Action::ChooseEffectOption(0),
            other => first_answer(other),
        });
        if casts < 4 {
            assert!(engine::object_has_type(&state, car, CardType::Creature));
            assert_eq!(engine::effective_power(&state, car), 4);
        }
    }
    assert_ne!(zone(&state, car), Zone::Battlefield);
    assert_eq!(controls(&state, P0, "Fantasticar Construct Token"), 4);

    // Until end of turn only.
    let mut state = islands();
    let car = put(&mut state, P0, "The Fantasticar", Zone::Battlefield);
    let key = put(&mut state, P0, "Voltaic Key", Zone::Hand);
    pool(&mut state, P0, &[], 1);
    cast(&mut state, key, &[]);
    settle_first(&mut state);
    assert!(engine::object_has_type(&state, car, CardType::Creature));
    to_main(&mut state, P1);
    assert!(!engine::object_has_type(&state, car, CardType::Creature));
}

#[test]
fn oath_of_druids_reveals_until_a_creature_for_the_player_with_fewer_creatures() {
    let mut state = game(&[
        "Island",
        "Island",
        "Glaring Fleshraker",
        "Island",
        "Island",
        "Island",
        "Island",
        "Island",
    ]);
    put(&mut state, P0, "Oath of Druids", Zone::Battlefield);
    put(&mut state, P1, "Glaring Fleshraker", Zone::Battlefield);
    let mut answered = false;
    // P1's upkeep: P1 has more creatures, so no trigger. P0's upkeep: P0
    // reveals.
    pass_until(&mut state, |state| state.active_player == P1);
    assert_eq!(controls(&state, P1, "Glaring Fleshraker"), 1);
    for _ in 0..400 {
        match next(&mut state) {
            Decision::ChooseEffectOption {
                player,
                option_count,
                ..
            } => {
                assert_eq!(player, P0);
                assert_eq!(option_count, 2);
                act(&mut state, Action::ChooseEffectOption(0));
                answered = true;
            }
            Decision::CastSpellOrPass { .. } if answered && state.stack.is_empty() => break,
            Decision::CastSpellOrPass { .. } => act(&mut state, Action::Pass),
            Decision::DeclareAttackers { .. } => {
                act(&mut state, Action::DeclareAttackers(Vec::new()))
            }
            Decision::DeclareBlockers { .. } => {
                act(&mut state, Action::DeclareBlockers(Vec::new()))
            }
            Decision::OrderTriggers { pending, .. } => act(
                &mut state,
                Action::OrderTriggers((0..pending.len()).collect()),
            ),
            other => panic!("unexpected decision: {other:?}"),
        }
    }
    assert!(answered);
    assert_eq!(controls(&state, P0, "Glaring Fleshraker"), 1);
    assert!(state.players[0]
        .graveyard
        .iter()
        .any(|&card| name_of(&state, card) == "Island"));
}

#[test]
fn ugin_exiles_colored_permanents_on_casts_and_uses_its_loyalty() {
    let mut state = islands();
    let ugin = put(&mut state, P0, "Ugin, Eye of the Storms", Zone::Hand);
    let oath = put(&mut state, P1, "Oath of Druids", Zone::Battlefield);
    let key = put(&mut state, P1, "Voltaic Key", Zone::Battlefield);
    pool(&mut state, P0, &[], 7);
    act(&mut state, Action::CastSpell(ugin));
    let seen = settle_targeting_seen(&mut state, Target::Object(oath));
    assert!(seen
        .iter()
        .all(|legal| !legal.contains(&Target::Object(key))));
    assert_eq!(zone(&state, oath), Zone::Exile);
    assert_eq!(loyalty(&state, ugin), Some(7));

    // A colorless spell triggers it again.
    let second_oath = put(&mut state, P1, "Oath of Druids", Zone::Battlefield);
    let top = put(&mut state, P0, "Sensei's Divining Top", Zone::Hand);
    pool(&mut state, P0, &[], 1);
    cast(&mut state, top, &[]);
    settle_targeting_seen(&mut state, Target::Object(second_oath));
    assert_eq!(zone(&state, second_oath), Zone::Exile);

    // +2: gain 3 life and draw. 0 (next turn): add {C}{C}{C}.
    let hand = state.players[0].hand.len();
    activate(&mut state, ugin, 0, &[]);
    settled(&mut state);
    assert_eq!(state.players[0].life, 23);
    assert_eq!(state.players[0].hand.len(), hand + 1);
    assert_eq!(loyalty(&state, ugin), Some(9));
    to_main(&mut state, P0);
    activate(&mut state, ugin, 0, &[]);
    settled(&mut state);
    assert_eq!(loyalty(&state, ugin), Some(11));
    to_main(&mut state, P0);
    // -11: exile colorless nonland cards and cast them free this turn.
    let monolith = put(&mut state, P0, "Grim Monolith", Zone::Library);
    activate(&mut state, ugin, 2, &[]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseEffectTargets { legal_targets, .. }
            if legal_targets.contains(&Target::Object(monolith)) =>
        {
            Action::ChooseEffectTarget(Target::Object(monolith))
        }
        other => first_answer(other),
    });
    assert_eq!(zone(&state, monolith), Zone::Exile);
    assert!(
        castable(&mut state, monolith),
        "without paying its mana cost"
    );
    cast(&mut state, monolith, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, monolith), Zone::Battlefield);

    let mut zero = islands();
    let ugin = put(&mut zero, P0, "Ugin, Eye of the Storms", Zone::Hand);
    pool(&mut zero, P0, &[], 7);
    act(&mut zero, Action::CastSpell(ugin));
    settle_first(&mut zero);
    activate(&mut zero, ugin, 1, &[]);
    settled(&mut zero);
    assert_eq!(mana(&zero, P0, ManaColor::C), 3);
}

/// Settles, answering each target prompt with `target` when it is legal
/// and any other choice with `first_answer`. Returns the legal target lists
/// it saw.
fn settle_targeting_seen(state: &mut GameState, target: Target) -> Vec<Vec<Target>> {
    let mut seen = Vec::new();
    while let Some(decision) = settle(state) {
        match decision {
            Decision::ChooseTargets { legal_targets, .. } if legal_targets.contains(&target) => {
                seen.push(legal_targets);
                act(state, Action::ChooseTarget(target));
            }
            other => act(state, first_answer(other)),
        }
    }
    seen
}

// ---- Planeswalker, protection and phasing --------------------------------------

#[test]
fn oko_makes_food_turns_a_permanent_into_an_elk_and_exchanges_control() {
    let mut state = islands();
    let oko = put(&mut state, P0, "Oko, Thief of Crowns", Zone::Hand);
    let monolith = put(&mut state, P1, "Grim Monolith", Zone::Battlefield);
    pool(&mut state, P0, &[ManaColor::G, ManaColor::U], 1);
    cast(&mut state, oko, &[]);
    settled(&mut state);
    activate(&mut state, oko, 0, &[]);
    settled(&mut state);
    assert_eq!(loyalty(&state, oko), Some(6));
    assert_eq!(controls(&state, P0, "Food Token"), 1);

    // +1: P1's Grim Monolith becomes a green 3/3 Elk without abilities.
    to_main(&mut state, P0);
    activate(&mut state, oko, 1, &[Target::Object(monolith)]);
    settled(&mut state);
    assert!(engine::object_has_type(
        &state,
        monolith,
        CardType::Creature
    ));
    assert!(!engine::object_has_type(
        &state,
        monolith,
        CardType::Artifact
    ));
    assert!(engine::has_effective_subtype(
        &state,
        monolith,
        Subtype::Elk
    ));
    assert_eq!(
        (
            engine::effective_power(&state, monolith),
            engine::effective_toughness(&state, monolith)
        ),
        (3, 3)
    );
    assert_ne!(engine::object_color_mask(&state, monolith), 0);
    opponents_main_phase(&mut state);
    next(&mut state);
    assert!(
        engine::step(&mut state, Action::ActivateManaAbility(monolith)).is_err(),
        "no mana ability"
    );

    // -5: exchange P0's Food for P1's creature with power 3 or less.
    let mut state = islands();
    let oko = put(&mut state, P0, "Oko, Thief of Crowns", Zone::Hand);
    let raker = put(&mut state, P1, "Glaring Fleshraker", Zone::Battlefield);
    pool(&mut state, P0, &[ManaColor::G, ManaColor::U], 1);
    cast(&mut state, oko, &[]);
    settled(&mut state);
    activate(&mut state, oko, 0, &[]);
    settled(&mut state);
    let food = state.players[0]
        .battlefield
        .iter()
        .copied()
        .find(|&object| name_of(&state, object) == "Food Token")
        .unwrap();
    to_main(&mut state, P0);
    activate(
        &mut state,
        oko,
        2,
        &[Target::Object(food), Target::Object(raker)],
    );
    settled(&mut state);
    assert_eq!(state.objects.get(raker).controller, P0);
    assert_eq!(state.objects.get(food).controller, P1);
    assert_eq!(loyalty(&state, oko), Some(1));
}

#[test]
fn serras_emissary_gives_its_controller_and_creatures_protection_from_the_chosen_type() {
    let mut state = islands();
    let emissary = put(&mut state, P0, "Serra's Emissary", Zone::Hand);
    let raker = put(&mut state, P0, "Glaring Fleshraker", Zone::Battlefield);
    pool(
        &mut state,
        P0,
        &[ManaColor::W, ManaColor::W, ManaColor::W],
        4,
    );
    cast(&mut state, emissary, &[]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseEffectOption { option_count, .. } => {
            assert_eq!(option_count, 7);
            // Instant.
            Action::ChooseEffectOption(3)
        }
        other => first_answer(other),
    });
    assert_eq!(zone(&state, emissary), Zone::Battlefield);
    state.priority_player = P1;
    let bolt = put(&mut state, P1, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P1, &[ManaColor::R], 0);
    act(&mut state, Action::CastSpell(bolt));
    match next(&mut state) {
        Decision::ChooseTargets { legal_targets, .. } => {
            assert!(!legal_targets.contains(&Target::Player(P0)));
            assert!(!legal_targets.contains(&Target::Object(raker)));
            assert!(!legal_targets.contains(&Target::Object(emissary)));
            assert!(legal_targets.contains(&Target::Player(P1)));
        }
        other => panic!("expected targets, got {other:?}"),
    }
    // Damage from an instant is prevented.
    let damage = mtg_kernel::event::ProposedEvent::damage(bolt, Target::Player(P0), 3);
    mtg_kernel::event::propose_and_commit(&mut state, damage);
    assert_eq!(state.players[0].life, 20);
}

#[test]
fn talon_gates_enters_from_hand_and_phases_a_creature_out_until_its_controllers_untap() {
    let mut state = islands();
    let gates = put(&mut state, P0, "Talon Gates of Madara", Zone::Hand);
    let raker = put(&mut state, P1, "Glaring Fleshraker", Zone::Battlefield);
    pool(&mut state, P0, &[], 4);
    activate(&mut state, gates, 0, &[]);
    let seen = settle_targeting_seen(&mut state, Target::Object(raker));
    assert!(!seen.is_empty(), "the entry trigger targets");
    assert_eq!(zone(&state, gates), Zone::Battlefield);
    assert_eq!(
        state.players[0].lands_played_this_turn, 0,
        "not a land play"
    );
    assert!(!state.players[1].battlefield.contains(&raker), "phased out");
    let incarnation = state.objects.get(raker).zone_change_count;
    to_main(&mut state, P1);
    assert!(state.players[1].battlefield.contains(&raker), "phased in");
    assert_eq!(zone(&state, raker), Zone::Battlefield);
    assert_eq!(state.objects.get(raker).zone_change_count, incarnation);

    // Its mana abilities: {T}: Add {C}, and {1}, {T}: Add any color.
    let mut state = islands();
    let gates = put(&mut state, P0, "Talon Gates of Madara", Zone::Battlefield);
    pool(&mut state, P0, &[], 1);
    next(&mut state);
    act(
        &mut state,
        Action::ActivateManaAbilityChoice(gates, ManaColor::B),
    );
    assert_eq!(mana(&state, P0, ManaColor::B), 1);
    assert_eq!(mana(&state, P0, ManaColor::C), 0);
}
