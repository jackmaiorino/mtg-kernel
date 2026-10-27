//! Eternal wave E6a: native rules tests for the cards that complete Pauper
//! Tron, Esper, Dimir and 4c Affinity, Esper Ephemerate, Mono-Blue Faeries,
//! Mardu Synthesizer and Azorius Gates.
#![cfg(feature = "eternal-fixtures")]

use mtg_kernel::card_def::{
    card_id_by_name, CardCapability, CardDef, CardType, Keywords, Subtype, Supertype, TargetSpec,
    CARD_DEFS,
};
use mtg_kernel::engine::{self, Action, CastMode, Decision};
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::ManaColor;
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, StackItemKind, Step, Target, Zone};

const P0: PlayerId = PlayerId::P0;
const P1: PlayerId = PlayerId::P1;

const E6A_CARDS: [&str; 35] = [
    "Abjure",
    "Ancient Den",
    "Archaeomancer",
    "Behold the Multiverse",
    "Bender's Waterskin",
    "Bramble Wurm",
    "Brinebarrow Intruder",
    "Crop Rotation",
    "Cryoshatter",
    "Dispatch",
    "Energy Refractor",
    "Ephemerate",
    "Ethereal Haze",
    "Gearseeker Serpent",
    "Ghostly Flicker",
    "Glacial Floodplain",
    "Grim Bauble",
    "Hard Evidence",
    "Kor Skyfisher",
    "Moment's Peace",
    "Mulldrifter",
    "Mystical Teachings",
    "Omen of the Dead",
    "Prohibit",
    "Prophetic Prism",
    "Razortide Bridge",
    "Remote Isle",
    "Rustvale Bridge",
    "Sewer-veillance Cam",
    "Shattered Landscape",
    "Stream of Thought",
    "Tangle",
    "Tangled Islet",
    "Tithing Blade",
    "Union of the Third Path",
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
        0x4536_4121_0001,
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

fn hand_contains(state: &GameState, player: PlayerId, object: ObjectId) -> bool {
    state.players[player.index()].hand.contains(&object)
}

fn mana(state: &GameState, player: PlayerId, color: ManaColor) -> u8 {
    state.players[player.index()].mana_pool[color.pool_index()]
}

// ---- Characteristics -----------------------------------------------------------

#[test]
fn e6a_cards_are_full_with_their_printed_characteristics() {
    for name in E6A_CARDS {
        assert_eq!(def(name).capability, CardCapability::Full, "{name}");
        assert!(!def(name).is_token, "{name}");
    }
    let crab = def("Crab Token");
    assert_eq!(crab.capability, CardCapability::Full);
    assert!(crab.is_token);
    assert_eq!(crab.subtypes, &[Subtype::Crab]);
    assert_eq!((crab.power, crab.toughness), (Some(0), Some(3)));
    assert_eq!(crab.colors, &[ManaColor::U]);
    for subtype in [Subtype::Crab, Subtype::Kor] {
        assert!(subtype.is_creature_type(), "{subtype:?}");
    }
    assert!(!Subtype::Arcane.is_creature_type());
    assert_eq!(def("Ethereal Haze").subtypes, &[Subtype::Arcane]);
    assert_eq!(
        def("Kor Skyfisher").subtypes,
        &[Subtype::Kor, Subtype::Soldier]
    );
    assert!(def("Kor Skyfisher").keywords.has(Keywords::FLYING));
    assert!(def("Mulldrifter").keywords.has(Keywords::FLYING));
    assert!(def("Mulldrifter").alt_cost.is_some(), "evoke");
    let wurm = def("Bramble Wurm");
    assert!(wurm.keywords.has(Keywords::REACH) && wurm.keywords.has(Keywords::TRAMPLE));
    for flash in [
        "Brinebarrow Intruder",
        "Omen of the Dead",
        "Sewer-veillance Cam",
    ] {
        assert!(def(flash).keywords.has(Keywords::FLASH), "{flash}");
    }
    let floodplain = def("Glacial Floodplain");
    assert_eq!(floodplain.subtypes, &[Subtype::Plains, Subtype::Island]);
    assert_eq!(floodplain.supertypes, &[Supertype::Snow]);
    assert_eq!(
        def("Tangled Islet").subtypes,
        &[Subtype::Forest, Subtype::Island]
    );
    for bridge in ["Razortide Bridge", "Rustvale Bridge"] {
        assert!(
            def(bridge).keywords.has(Keywords::INDESTRUCTIBLE),
            "{bridge}"
        );
        assert!(def(bridge).enters_battlefield_tapped, "{bridge}");
        assert!(def(bridge).has_type(CardType::Artifact), "{bridge}");
    }
    assert!(def("Ancient Den").has_type(CardType::Artifact));
    assert!(!def("Ancient Den").enters_battlefield_tapped);
    for tapped in ["Glacial Floodplain", "Tangled Islet", "Remote Isle"] {
        assert!(def(tapped).enters_battlefield_tapped, "{tapped}");
    }
    assert!(def("Prohibit").kicker_cost.is_some());
    assert!(def("Moment's Peace").flashback.is_some());
    assert!(def("Mystical Teachings").flashback.is_some());
    assert!(def("Behold the Multiverse").alt_cost.is_some(), "foretell");
    assert_eq!(def("Behold the Multiverse").activated_abilities.len(), 1);
    assert!(def("Abjure").additional_cost.is_some());
    assert!(def("Crop Rotation").additional_cost.is_some());
    assert!(def("Cryoshatter").attachment.is_some());
    let blade = def("Tithing Blade");
    let back = blade.transform_face.as_ref().expect("back face");
    assert_eq!(back.name, "Consuming Sepulcher");
    assert_eq!(back.types, &[CardType::Artifact]);
    assert_eq!(blade.activated_abilities.len(), 1);
    assert_eq!(def("Gearseeker Serpent").subtypes, &[Subtype::Serpent]);
    assert!(def("Gearseeker Serpent").generic_cost_reduction.is_some());
    assert_eq!(
        def("Ghostly Flicker").target_spec,
        TargetSpec::StandardV1(
            mtg_kernel::standard_cards_v1::StandardTargetV1::TwoControlledArtifactsCreaturesOrLandsV1
        )
    );
    assert_eq!(
        mtg_kernel::eternal_cards_v1::replicate_cost("Stream of Thought").map(|cost| cost.generic),
        Some(2)
    );
    assert_eq!(def("Prophetic Prism").additional_mana_abilities.len(), 1);
    assert_eq!(def("Energy Refractor").additional_mana_abilities.len(), 1);
}

// ---- Lands ---------------------------------------------------------------------

#[test]
fn new_lands_tap_for_their_colors_and_the_tapped_ones_enter_tapped() {
    for (name, colors, enters_tapped) in [
        ("Ancient Den", vec![ManaColor::W], false),
        ("Razortide Bridge", vec![ManaColor::W, ManaColor::U], true),
        ("Rustvale Bridge", vec![ManaColor::R, ManaColor::W], true),
        ("Glacial Floodplain", vec![ManaColor::W, ManaColor::U], true),
        ("Tangled Islet", vec![ManaColor::G, ManaColor::U], true),
        ("Remote Isle", vec![ManaColor::U], true),
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
            assert_eq!(mana(&state, P0, color), 1, "{name} {color:?}");
            assert!(state.objects.get(land).tapped, "{name}");
        }
        let mut state = islands();
        let land = put(&mut state, P0, name, Zone::Hand);
        next(&mut state);
        act(&mut state, Action::PlayLand(land));
        assert_eq!(zone(&state, land), Zone::Battlefield, "{name}");
        assert_eq!(state.objects.get(land).tapped, enters_tapped, "{name}");
    }
    let mut state = islands();
    let islet = put(&mut state, P0, "Tangled Islet", Zone::Battlefield);
    for subtype in [Subtype::Forest, Subtype::Island] {
        assert!(engine::has_effective_subtype(&state, islet, subtype));
    }
}

#[test]
fn bridges_are_indestructible() {
    let mut state = islands();
    let bridge = put(&mut state, P1, "Razortide Bridge", Zone::Battlefield);
    let boulder = put(&mut state, P0, "Giant's Boulder", Zone::Battlefield);
    pool(&mut state, P0, &[], 7);
    activate(&mut state, boulder, 0, &[Target::Object(bridge)]);
    settled(&mut state);
    assert_eq!(zone(&state, boulder), Zone::Graveyard);
    assert_eq!(zone(&state, bridge), Zone::Battlefield, "indestructible");
}

#[test]
fn remote_isle_cycles_for_two() {
    let mut state = islands();
    let isle = put(&mut state, P0, "Remote Isle", Zone::Hand);
    pool(&mut state, P0, &[], 1);
    assert!(!activatable(&mut state, isle, 0));
    pool(&mut state, P0, &[], 1);
    let hand = state.players[0].hand.len();
    activate(&mut state, isle, 0, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, isle), Zone::Graveyard);
    assert_eq!(state.players[0].hand.len(), hand, "cycled for a card");
}

#[test]
fn shattered_landscape_fetches_a_basic_mountain_plains_or_swamp_tapped_and_cycles() {
    let mut state = islands();
    let landscape = put(&mut state, P0, "Shattered Landscape", Zone::Battlefield);
    let plains = put(&mut state, P0, "Plains", Zone::Library);
    activate(&mut state, landscape, 0, &[]);
    settle_choosing(&mut state, plains);
    assert_eq!(zone(&state, landscape), Zone::Graveyard);
    assert_eq!(zone(&state, plains), Zone::Battlefield);
    assert!(state.objects.get(plains).tapped);

    let mut state = islands();
    let landscape = put(&mut state, P0, "Shattered Landscape", Zone::Hand);
    pool(
        &mut state,
        P0,
        &[ManaColor::R, ManaColor::W, ManaColor::B],
        0,
    );
    let hand = state.players[0].hand.len();
    activate(&mut state, landscape, 1, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, landscape), Zone::Graveyard);
    assert_eq!(state.players[0].hand.len(), hand);
}

// ---- Mana rocks -----------------------------------------------------------------

#[test]
fn benders_waterskin_taps_for_any_color_and_untaps_on_the_opponents_untap_step() {
    let mut state = islands();
    let waterskin = put(&mut state, P0, "Bender's Waterskin", Zone::Battlefield);
    let prism = put(&mut state, P0, "Prophetic Prism", Zone::Battlefield);
    next(&mut state);
    act(
        &mut state,
        Action::ActivateManaAbilityChoice(waterskin, ManaColor::B),
    );
    assert_eq!(mana(&state, P0, ManaColor::B), 1);
    assert!(state.objects.get(waterskin).tapped);
    state.objects.get_mut(prism).tapped = true;
    pass_until(&mut state, |state| {
        state.active_player == P1 && state.step == Step::Main1
    });
    assert!(
        !state.objects.get(waterskin).tapped,
        "untapped during P1's untap step"
    );
    assert!(
        state.objects.get(prism).tapped,
        "an ordinary artifact stays tapped"
    );
}

#[test]
fn prophetic_prism_draws_on_entry_and_filters_one_into_any_color() {
    let mut state = islands();
    let prism = put(&mut state, P0, "Prophetic Prism", Zone::Hand);
    pool(&mut state, P0, &[], 2);
    cast(&mut state, prism, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, prism), Zone::Battlefield);
    assert_eq!(state.players[0].hand.len(), 1, "drew a card");
    next(&mut state);
    assert!(engine::step(
        &mut state,
        Action::ActivateManaAbilityChoice(prism, ManaColor::G)
    )
    .is_err());
    pool(&mut state, P0, &[], 1);
    act(
        &mut state,
        Action::ActivateManaAbilityChoice(prism, ManaColor::G),
    );
    assert_eq!(mana(&state, P0, ManaColor::G), 1);
    assert_eq!(mana(&state, P0, ManaColor::C), 0);
    assert!(state.objects.get(prism).tapped);
}

#[test]
fn energy_refractor_draws_on_entry_and_turns_two_into_any_color_without_tapping() {
    let mut state = islands();
    let refractor = put(&mut state, P0, "Energy Refractor", Zone::Hand);
    pool(&mut state, P0, &[], 2);
    cast(&mut state, refractor, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, refractor), Zone::Battlefield);
    assert_eq!(state.players[0].hand.len(), 1, "drew a card");
    pool(&mut state, P0, &[], 2);
    next(&mut state);
    act(
        &mut state,
        Action::ActivateManaAbilityChoice(refractor, ManaColor::B),
    );
    assert_eq!(mana(&state, P0, ManaColor::B), 1);
    assert_eq!(mana(&state, P0, ManaColor::C), 0);
    assert!(!state.objects.get(refractor).tapped);
}

// ---- Creatures ------------------------------------------------------------------

#[test]
fn kor_skyfisher_returns_a_permanent_its_controller_controls() {
    let mut state = islands();
    let island = put(&mut state, P0, "Island", Zone::Battlefield);
    let skyfisher = put(&mut state, P0, "Kor Skyfisher", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W], 1);
    cast(&mut state, skyfisher, &[]);
    settle_choosing(&mut state, island);
    assert_eq!(zone(&state, skyfisher), Zone::Battlefield);
    assert!(hand_contains(&state, P0, island));
}

#[test]
fn archaeomancer_returns_an_instant_or_sorcery_card() {
    let mut state = islands();
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Graveyard);
    let angler = put(&mut state, P0, "Gurmag Angler", Zone::Graveyard);
    let archaeomancer = put(&mut state, P0, "Archaeomancer", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U, ManaColor::U], 2);
    cast(&mut state, archaeomancer, &[]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseTargets { legal_targets, .. } => {
            assert!(!legal_targets.contains(&Target::Object(angler)));
            Action::ChooseTarget(Target::Object(bolt))
        }
        other => first_answer(other),
    });
    assert!(hand_contains(&state, P0, bolt));
    assert_eq!(zone(&state, angler), Zone::Graveyard);
}

#[test]
fn omen_of_the_dead_returns_a_creature_card_and_sacrifices_to_scry() {
    let mut state = islands();
    let angler = put(&mut state, P0, "Gurmag Angler", Zone::Graveyard);
    let omen = put(&mut state, P0, "Omen of the Dead", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 0);
    cast(&mut state, omen, &[]);
    settle_choosing(&mut state, angler);
    assert!(hand_contains(&state, P0, angler));
    assert_eq!(zone(&state, omen), Zone::Battlefield);
    pool(&mut state, P0, &[ManaColor::B], 2);
    activate(&mut state, omen, 0, &[]);
    settle_first(&mut state);
    assert_eq!(zone(&state, omen), Zone::Graveyard);
    assert_eq!(state.players[0].library.len(), 20);
}

#[test]
fn mulldrifter_draws_two_and_is_sacrificed_only_when_evoked() {
    // Evoked: {2}{U} pays only the evoke cost.
    let mut state = islands();
    let mulldrifter = put(&mut state, P0, "Mulldrifter", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 2);
    cast(&mut state, mulldrifter, &[]);
    settle_first(&mut state);
    assert_eq!(zone(&state, mulldrifter), Zone::Graveyard, "evoked");
    assert_eq!(state.players[0].hand.len(), 2);

    // Cast for its mana cost, it stays.
    let mut state = islands();
    let mulldrifter = put(&mut state, P0, "Mulldrifter", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 4);
    cast(&mut state, mulldrifter, &[]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseCastMode { options, .. } => {
            assert!(options.contains(&CastMode::Alternative));
            Action::ChooseCastMode(CastMode::Normal)
        }
        other => first_answer(other),
    });
    assert_eq!(zone(&state, mulldrifter), Zone::Battlefield);
    assert_eq!(state.players[0].hand.len(), 2);
}

#[test]
fn brinebarrow_intruder_shrinks_an_opponents_creature() {
    let mut state = islands();
    let angler = put(&mut state, P1, "Gurmag Angler", Zone::Battlefield);
    let intruder = put(&mut state, P0, "Brinebarrow Intruder", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 0);
    cast(&mut state, intruder, &[]);
    settle_choosing(&mut state, angler);
    assert_eq!(power_toughness(&state, angler), (3, 5));
}

#[test]
fn grim_bauble_shrinks_an_opponents_creature_and_sacrifices_to_surveil() {
    let mut state = islands();
    let elves = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    let bauble = put(&mut state, P0, "Grim Bauble", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 0);
    cast(&mut state, bauble, &[]);
    settle_choosing(&mut state, elves);
    assert_eq!(zone(&state, elves), Zone::Graveyard, "-2/-2");
    pool(&mut state, P0, &[ManaColor::B], 2);
    activate(&mut state, bauble, 0, &[]);
    settle_first(&mut state);
    assert_eq!(zone(&state, bauble), Zone::Graveyard);
}

#[test]
fn sewer_veillance_cam_taps_on_entry_and_untaps_as_it_leaves() {
    let mut state = islands();
    let angler = put(&mut state, P1, "Gurmag Angler", Zone::Battlefield);
    let cam = put(&mut state, P0, "Sewer-veillance Cam", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 0);
    cast(&mut state, cam, &[]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseTargets { .. } => Action::ChooseTarget(Target::Object(angler)),
        Decision::ChooseEffectOption { option_count, .. } => {
            assert_eq!(option_count, 3, "tap, untap or neither");
            Action::ChooseEffectOption(0)
        }
        other => first_answer(other),
    });
    assert!(state.objects.get(angler).tapped);

    // Sacrificed to draw two: its leave trigger untaps the creature.
    pool(&mut state, P0, &[ManaColor::U], 3);
    let hand = state.players[0].hand.len();
    activate(&mut state, cam, 0, &[]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseTargets { .. } => Action::ChooseTarget(Target::Object(angler)),
        Decision::ChooseEffectOption { .. } => Action::ChooseEffectOption(1),
        other => first_answer(other),
    });
    assert_eq!(zone(&state, cam), Zone::Graveyard);
    assert!(!state.objects.get(angler).tapped);
    assert_eq!(state.players[0].hand.len(), hand + 2);
}

#[test]
fn bramble_wurm_gains_five_on_entry_and_from_the_graveyard() {
    let mut state = islands();
    let wurm = put(&mut state, P0, "Bramble Wurm", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 6);
    cast(&mut state, wurm, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, wurm), Zone::Battlefield);
    assert_eq!(state.players[0].life, 25);

    let mut state = islands();
    let wurm = put(&mut state, P0, "Bramble Wurm", Zone::Graveyard);
    pool(&mut state, P0, &[ManaColor::G], 2);
    activate(&mut state, wurm, 0, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, wurm), Zone::Exile);
    assert_eq!(state.players[0].life, 25);
}

#[test]
fn gearseeker_serpent_costs_less_per_artifact_and_becomes_unblockable() {
    let mut state = islands();
    for _ in 0..5 {
        put(&mut state, P0, "Ancient Den", Zone::Battlefield);
    }
    let serpent = put(&mut state, P0, "Gearseeker Serpent", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U, ManaColor::U], 0);
    cast(&mut state, serpent, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, serpent), Zone::Battlefield);
    assert!(!engine::has_effective_keyword(
        &state,
        serpent,
        Keywords::CANT_BE_BLOCKED
    ));
    pool(&mut state, P0, &[ManaColor::U], 5);
    activate(&mut state, serpent, 0, &[]);
    settled(&mut state);
    assert!(engine::has_effective_keyword(
        &state,
        serpent,
        Keywords::CANT_BE_BLOCKED
    ));
}

// ---- Tithing Blade and Cryoshatter -----------------------------------------------

#[test]
fn tithing_blade_makes_an_opponent_sacrifice_and_crafts_into_a_draining_sepulcher() {
    let mut state = islands();
    let elves = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    let blade = put(&mut state, P0, "Tithing Blade", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 1);
    cast(&mut state, blade, &[]);
    settle_first(&mut state);
    assert_eq!(zone(&state, elves), Zone::Graveyard, "sacrificed");
    assert_eq!(zone(&state, blade), Zone::Battlefield);

    // Craft with creature: a creature card from P0's graveyard.
    let artifact = put(&mut state, P0, "Prophetic Prism", Zone::Graveyard);
    let material = put(&mut state, P0, "Gurmag Angler", Zone::Graveyard);
    pool(&mut state, P0, &[ManaColor::B], 4);
    activate(&mut state, blade, 0, &[]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseCostTargets { candidates, .. } => {
            assert!(
                !candidates.contains(&artifact),
                "an artifact is no material"
            );
            assert!(candidates.contains(&material));
            Action::ChooseCostTarget(material)
        }
        other => first_answer(other),
    });
    assert_eq!(zone(&state, material), Zone::Exile);
    assert_eq!(zone(&state, blade), Zone::Battlefield);
    assert_eq!(
        state.objects.get(blade).v4.face_index,
        1,
        "Consuming Sepulcher"
    );

    // At the beginning of P0's next upkeep, P1 loses 1 and P0 gains 1.
    run_until(&mut state, |state| state.players[1].life == 19, |_, _| None);
    assert_eq!(state.active_player, P0);
    assert_eq!(state.players[0].life, 21);
}

#[test]
fn cryoshatter_shrinks_and_destroys_when_tapped_or_dealt_damage() {
    let mut state = islands();
    let angler = put(&mut state, P1, "Gurmag Angler", Zone::Battlefield);
    let shatter = put(&mut state, P0, "Cryoshatter", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 0);
    cast(&mut state, shatter, &[Target::Object(angler)]);
    settled(&mut state);
    assert_eq!(zone(&state, shatter), Zone::Battlefield);
    assert_eq!(power_toughness(&state, angler), (0, 5));
    event::propose_and_commit(&mut state, ProposedEvent::tap(angler));
    settled(&mut state);
    assert_eq!(
        zone(&state, angler),
        Zone::Graveyard,
        "destroyed when tapped"
    );

    let mut state = islands();
    let angler = put(&mut state, P1, "Gurmag Angler", Zone::Battlefield);
    let shatter = put(&mut state, P0, "Cryoshatter", Zone::Hand);
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U, ManaColor::R], 0);
    cast(&mut state, shatter, &[Target::Object(angler)]);
    settled(&mut state);
    cast(&mut state, bolt, &[Target::Object(angler)]);
    settled(&mut state);
    assert_eq!(
        zone(&state, angler),
        Zone::Graveyard,
        "destroyed when dealt damage"
    );
}

// ---- Counterspells --------------------------------------------------------------

#[test]
fn abjure_sacrifices_a_blue_permanent_to_counter_a_spell() {
    let mut state = islands();
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    let abjure = put(&mut state, P0, "Abjure", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R, ManaColor::U], 0);
    cast(&mut state, bolt, &[Target::Player(P1)]);
    assert!(
        !castable(&mut state, abjure),
        "no blue permanent to sacrifice"
    );

    let mut state = islands();
    let delver = put(&mut state, P0, "Delver of Secrets", Zone::Battlefield);
    let island = put(&mut state, P0, "Island", Zone::Battlefield);
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    let abjure = put(&mut state, P0, "Abjure", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R, ManaColor::U], 0);
    cast(&mut state, bolt, &[Target::Player(P1)]);
    act(&mut state, Action::CastSpell(abjure));
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseTargets { .. } => Action::ChooseTarget(Target::Object(bolt)),
        Decision::ChooseCostTargets { candidates, .. } => {
            assert_eq!(candidates, vec![delver], "only the blue permanent");
            Action::ChooseCostTarget(delver)
        }
        other => first_answer(other),
    });
    assert_eq!(zone(&state, delver), Zone::Graveyard);
    assert_eq!(zone(&state, island), Zone::Battlefield);
    assert_eq!(zone(&state, bolt), Zone::Graveyard);
    assert_eq!(state.players[1].life, 20, "countered");
}

/// P0 casts `spell` (paying `cost` into the pool first), then Prohibit at
/// it, kicked or not.
fn prohibit_at(spell_name: &str, cost: &[ManaColor], generic: u8, kicked: bool) -> GameState {
    let mut state = islands();
    let spell = put(&mut state, P0, spell_name, Zone::Hand);
    let prohibit = put(&mut state, P0, "Prohibit", Zone::Hand);
    pool(&mut state, P0, cost, generic);
    act(&mut state, Action::CastSpell(spell));
    while let Decision::ChooseTargets { legal_targets, .. } = next(&mut state) {
        let target = if legal_targets.contains(&Target::Player(P1)) {
            Target::Player(P1)
        } else {
            legal_targets[0]
        };
        act(&mut state, Action::ChooseTarget(target));
    }
    pool(&mut state, P0, &[ManaColor::U], if kicked { 3 } else { 1 });
    act(&mut state, Action::CastSpell(prohibit));
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseKicker { .. } => Action::ChooseKicker(kicked),
        Decision::ChooseTargets { legal_targets, .. }
            if legal_targets.contains(&Target::Object(spell)) =>
        {
            Action::ChooseTarget(Target::Object(spell))
        }
        other => first_answer(other),
    });
    state
}

#[test]
fn prohibit_counters_mana_value_two_or_less_or_four_or_less_kicked() {
    let state = prohibit_at("Lightning Bolt", &[ManaColor::R], 0, false);
    assert_eq!(
        state.players[1].life, 20,
        "a mana value 1 spell is countered"
    );
    let state = prohibit_at("Archaeomancer", &[ManaColor::U, ManaColor::U], 2, false);
    assert_eq!(
        named(&state, P0, "Archaeomancer").len(),
        1,
        "mana value 4 resolves unkicked"
    );
    let state = prohibit_at("Archaeomancer", &[ManaColor::U, ManaColor::U], 2, true);
    assert!(
        named(&state, P0, "Archaeomancer").is_empty(),
        "kicked, mana value 4 is countered"
    );
}

// ---- Instants and sorceries ------------------------------------------------------

#[test]
fn crop_rotation_sacrifices_a_land_to_put_any_land_onto_the_battlefield() {
    let mut state = islands();
    let island = put(&mut state, P0, "Island", Zone::Battlefield);
    let tower = put(&mut state, P0, "Urza's Tower", Zone::Library);
    let rotation = put(&mut state, P0, "Crop Rotation", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 0);
    act(&mut state, Action::CastSpell(rotation));
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseCostTargets { candidates, .. } => {
            assert_eq!(candidates, vec![island]);
            Action::ChooseCostTarget(island)
        }
        Decision::ChooseEffectTargets { legal_targets, .. }
            if legal_targets.contains(&Target::Object(tower)) =>
        {
            Action::ChooseEffectTarget(Target::Object(tower))
        }
        other => first_answer(other),
    });
    assert_eq!(zone(&state, island), Zone::Graveyard);
    assert_eq!(zone(&state, tower), Zone::Battlefield);
    assert!(!state.objects.get(tower).tapped);
}

#[test]
fn dispatch_taps_and_exiles_with_metalcraft() {
    for dens in [2, 3] {
        let mut state = islands();
        for _ in 0..dens {
            put(&mut state, P0, "Ancient Den", Zone::Battlefield);
        }
        let angler = put(&mut state, P1, "Gurmag Angler", Zone::Battlefield);
        let dispatch = put(&mut state, P0, "Dispatch", Zone::Hand);
        pool(&mut state, P0, &[ManaColor::W], 0);
        cast(&mut state, dispatch, &[Target::Object(angler)]);
        settled(&mut state);
        if dens == 3 {
            assert_eq!(zone(&state, angler), Zone::Exile, "metalcraft");
        } else {
            assert_eq!(zone(&state, angler), Zone::Battlefield);
            assert!(state.objects.get(angler).tapped);
        }
    }
}

#[test]
fn ephemerate_flickers_and_rebounds_at_the_next_upkeep() {
    let mut state = islands();
    let mulldrifter = put(&mut state, P0, "Mulldrifter", Zone::Battlefield);
    let ephemerate = put(&mut state, P0, "Ephemerate", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W], 0);
    cast(&mut state, ephemerate, &[Target::Object(mulldrifter)]);
    settle_first(&mut state);
    assert_eq!(zone(&state, mulldrifter), Zone::Battlefield);
    assert_eq!(state.objects.get(mulldrifter).zone_change_count, 2);
    assert_eq!(state.players[0].hand.len(), 2, "the entry draw");
    assert_eq!(zone(&state, ephemerate), Zone::Exile, "rebound");

    // P0's next upkeep: cast it from exile without paying its mana cost.
    let mut offered = false;
    run_until(
        &mut state,
        |state| zone(state, ephemerate) == Zone::Graveyard,
        |_, decision| match decision {
            Decision::ChooseEffectOption { option_count, .. } => {
                assert_eq!(*option_count, 2, "cast or decline");
                offered = true;
                Some(Action::ChooseEffectOption(0))
            }
            Decision::ChooseEffectTargets { legal_targets, .. }
                if legal_targets.contains(&Target::Object(ephemerate)) =>
            {
                Some(Action::ChooseEffectTarget(Target::Object(ephemerate)))
            }
            Decision::ChooseTargets { legal_targets, .. }
                if legal_targets.contains(&Target::Object(mulldrifter)) =>
            {
                Some(Action::ChooseTarget(Target::Object(mulldrifter)))
            }
            _ => None,
        },
    );
    assert!(offered);
    assert_eq!(state.active_player, P0);
    assert_eq!(state.objects.get(mulldrifter).zone_change_count, 4);
}

#[test]
fn ghostly_flicker_returns_two_permanents_under_its_controllers_control() {
    let mut state = islands();
    let island = put(&mut state, P0, "Island", Zone::Battlefield);
    state.objects.get_mut(island).tapped = true;
    let mulldrifter = put(&mut state, P0, "Mulldrifter", Zone::Battlefield);
    let flicker = put(&mut state, P0, "Ghostly Flicker", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 2);
    cast(
        &mut state,
        flicker,
        &[Target::Object(island), Target::Object(mulldrifter)],
    );
    settle_first(&mut state);
    assert_eq!(zone(&state, island), Zone::Battlefield);
    assert!(!state.objects.get(island).tapped, "a new untapped land");
    assert_eq!(state.objects.get(mulldrifter).zone_change_count, 2);
    assert_eq!(state.players[0].hand.len(), 2);
}

#[test]
fn ghostly_flicker_needs_two_different_targets() {
    let mut state = islands();
    put(&mut state, P0, "Mulldrifter", Zone::Battlefield);
    let flicker = put(&mut state, P0, "Ghostly Flicker", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 2);
    assert!(
        !castable(&mut state, flicker),
        "one permanent is not two targets"
    );
}

#[test]
fn moments_peace_prevents_combat_damage_and_has_flashback() {
    let mut state = islands();
    let angler = put(&mut state, P0, "Gurmag Angler", Zone::Battlefield);
    let peace = put(&mut state, P0, "Moment's Peace", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 1);
    cast(&mut state, peace, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, peace), Zone::Graveyard);
    attack_with(&mut state, vec![angler], |_, _| None);
    assert_eq!(state.players[1].life, 20, "no combat damage");
    pool(&mut state, P0, &[ManaColor::G], 2);
    cast(&mut state, peace, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, peace), Zone::Exile, "flashback");
}

#[test]
fn ethereal_haze_prevents_damage_creatures_would_deal_but_not_a_bolt() {
    let mut state = islands();
    let angler = put(&mut state, P0, "Gurmag Angler", Zone::Battlefield);
    let haze = put(&mut state, P0, "Ethereal Haze", Zone::Hand);
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W], 0);
    cast(&mut state, haze, &[]);
    settled(&mut state);
    attack_with(&mut state, vec![angler], |_, _| None);
    assert_eq!(state.players[1].life, 20, "no damage from creatures");
    pool(&mut state, P0, &[ManaColor::R], 0);
    cast(&mut state, bolt, &[Target::Player(P1)]);
    settled(&mut state);
    assert_eq!(state.players[1].life, 17);
}

#[test]
fn tangle_prevents_combat_damage_and_attackers_skip_their_next_untap() {
    let mut state = islands();
    let angler = put(&mut state, P0, "Gurmag Angler", Zone::Battlefield);
    let tangle = put(&mut state, P1, "Tangle", Zone::Hand);
    // Mana pools empty between steps; P1 pays from its lands.
    put(&mut state, P1, "Forest", Zone::Battlefield);
    put(&mut state, P1, "Forest", Zone::Battlefield);
    let mut cast_it = false;
    attack_with(&mut state, vec![angler], |state, decision| match decision {
        Decision::CastSpellOrPass { player: P1, .. }
            if !cast_it && state.step == Step::DeclareAttackers =>
        {
            cast_it = true;
            Some(Action::CastSpell(tangle))
        }
        _ => None,
    });
    assert!(cast_it);
    assert_eq!(zone(&state, tangle), Zone::Graveyard);
    assert_eq!(state.players[1].life, 20, "no combat damage");
    assert!(state.objects.get(angler).v4.skip_next_untap);
}

#[test]
fn hard_evidence_makes_a_crab_and_a_clue() {
    let mut state = islands();
    let evidence = put(&mut state, P0, "Hard Evidence", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 0);
    cast(&mut state, evidence, &[]);
    settled(&mut state);
    let crabs = named(&state, P0, "Crab Token");
    assert_eq!(crabs.len(), 1);
    assert_eq!(power_toughness(&state, crabs[0]), (0, 3));
    assert_eq!(named(&state, P0, "Clue Token").len(), 1);
}

#[test]
fn mystical_teachings_finds_an_instant_or_a_card_with_flash_and_has_flashback() {
    let mut state = islands();
    let intruder = put(&mut state, P0, "Brinebarrow Intruder", Zone::Library);
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Library);
    let angler = put(&mut state, P0, "Gurmag Angler", Zone::Library);
    let teachings = put(&mut state, P0, "Mystical Teachings", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 3);
    cast(&mut state, teachings, &[]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseEffectTargets { legal_targets, .. }
            if legal_targets.contains(&Target::Object(intruder)) =>
        {
            assert!(legal_targets.contains(&Target::Object(bolt)));
            assert!(!legal_targets.contains(&Target::Object(angler)));
            Action::ChooseEffectTarget(Target::Object(intruder))
        }
        other => first_answer(other),
    });
    assert!(hand_contains(&state, P0, intruder));
    assert_eq!(zone(&state, teachings), Zone::Graveyard);
    pool(&mut state, P0, &[ManaColor::B], 5);
    assert!(castable(&mut state, teachings), "flashback {{5}}{{B}}");
}

#[test]
fn behold_the_multiverse_scries_and_draws_two() {
    let mut state = islands();
    let behold = put(&mut state, P0, "Behold the Multiverse", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 3);
    cast(&mut state, behold, &[]);
    settle_first(&mut state);
    assert_eq!(zone(&state, behold), Zone::Graveyard);
    assert_eq!(state.players[0].hand.len(), 2);
}

#[test]
fn behold_the_multiverse_is_foretold_face_down_and_cast_later_for_its_foretell_cost() {
    let mut state = islands();
    let behold = put(&mut state, P0, "Behold the Multiverse", Zone::Hand);
    pool(&mut state, P0, &[], 1);
    assert!(
        !activatable(&mut state, behold, 0),
        "foretelling costs {{2}}"
    );
    pool(&mut state, P0, &[], 1);
    act(&mut state, Action::ActivateAbility(behold, 0));
    assert!(state.stack.is_empty(), "a special action");
    assert_eq!(zone(&state, behold), Zone::Exile);
    let face = state
        .objects
        .get(behold)
        .v4
        .face_down_v1
        .expect("face down");
    assert_eq!(face.lookers, 1 << P0.index(), "only its owner may look");
    assert_eq!(mana(&state, P0, ManaColor::C), 0);
    pool(&mut state, P0, &[ManaColor::U], 3);
    assert!(
        !castable(&mut state, behold),
        "not the turn it was foretold"
    );

    // P0's next turn: castable for {1}{U}, never for its mana cost.
    pass_until(&mut state, |state| state.active_player == P1);
    pass_until(&mut state, |state| {
        state.active_player == P0 && state.step == Step::Main1
    });
    pool(&mut state, P0, &[ManaColor::U], 0);
    assert!(!castable(&mut state, behold), "{{1}}{{U}} is not yet paid");
    pool(&mut state, P0, &[], 1);
    let hand = state.players[0].hand.len();
    cast(&mut state, behold, &[]);
    settle_first(&mut state);
    assert_eq!(zone(&state, behold), Zone::Graveyard);
    assert_eq!(state.players[0].hand.len(), hand + 2);
}

#[test]
fn foretelling_is_offered_only_during_its_owners_turn() {
    let mut state = islands();
    let behold = put(&mut state, P1, "Behold the Multiverse", Zone::Hand);
    pool(&mut state, P1, &[], 2);
    pass_until(&mut state, |state| state.priority_player == P1);
    let (_, abilities) = priority(&mut state);
    assert!(!abilities.contains(&(behold, 0)));
}

#[test]
fn stream_of_thought_mills_four_and_shuffles_up_to_four_back() {
    let mut state = islands();
    let first = put(&mut state, P0, "Lightning Bolt", Zone::Graveyard);
    let second = put(&mut state, P0, "Lightning Bolt", Zone::Graveyard);
    let stream = put(&mut state, P0, "Stream of Thought", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 0);
    cast(&mut state, stream, &[Target::Player(P1)]);
    settle_with(&mut state, |state, decision| match decision {
        Decision::ChooseEffectTargets { legal_targets, .. }
            if legal_targets.contains(&Target::Object(first))
                && state.objects.get(first).zone == Zone::Graveyard =>
        {
            Action::ChooseEffectTarget(Target::Object(first))
        }
        Decision::ChooseEffectTargets { legal_targets, .. }
            if legal_targets.contains(&Target::Object(second)) =>
        {
            Action::ChooseEffectTarget(Target::Object(second))
        }
        other => first_answer(other),
    });
    assert_eq!(state.players[1].library.len(), 16);
    assert_eq!(state.players[1].graveyard.len(), 4);
    assert_eq!(zone(&state, first), Zone::Library);
    assert_eq!(zone(&state, second), Zone::Library);
    assert_eq!(state.players[0].library.len(), 22);
    assert_eq!(zone(&state, stream), Zone::Graveyard);
}

#[test]
fn union_of_the_third_path_draws_then_gains_life_per_card_in_hand() {
    let mut state = islands();
    put(&mut state, P0, "Island", Zone::Hand);
    put(&mut state, P0, "Island", Zone::Hand);
    let union = put(&mut state, P0, "Union of the Third Path", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W], 2);
    cast(&mut state, union, &[]);
    settled(&mut state);
    assert_eq!(state.players[0].hand.len(), 3);
    assert_eq!(state.players[0].life, 23);
}

#[test]
fn ephemerate_that_does_not_resolve_does_not_rebound() {
    let mut state = islands();
    let mulldrifter = put(&mut state, P0, "Mulldrifter", Zone::Battlefield);
    let ephemerate = put(&mut state, P0, "Ephemerate", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W], 0);
    cast(&mut state, ephemerate, &[Target::Object(mulldrifter)]);
    // Countered on resolution for an illegal target: no rebound.
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(mulldrifter, Zone::Graveyard),
    );
    settle_first(&mut state);
    assert_eq!(zone(&state, ephemerate), Zone::Graveyard);
    assert!(state
        .stack
        .iter()
        .all(|item| item.kind != StackItemKind::Spell));
}
