//! Eternal wave S1d: native rules tests for the Pauper sideboard cards
//! (Vandalblast, Crypt Incursion, Circle of Protection: Blue and the rest).
#![cfg(feature = "eternal-fixtures")]
#![allow(dead_code)]

use mtg_kernel::card_def::{card_id_by_name, CardDef, Keywords, Subtype, CARD_DEFS};
use mtg_kernel::engine::{self, Action, Decision, OptionalCostChoice};
use mtg_kernel::eternal_cards_v1::EternalStateV1;
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::ManaColor;
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, StackItemKind, Step, Target, Zone};
use std::hash::{Hash, Hasher};

const P0: PlayerId = PlayerId::P0;
const P1: PlayerId = PlayerId::P1;

#[test]
fn empty_sideboard_state_preserves_legacy_hash_input_and_json() {
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

    let mut eternal = EternalStateV1 {
        energy: [3, 0],
        ..Default::default()
    };
    let mut empty = HashInput::default();
    eternal.s1a.hash(&mut empty);
    eternal.s1b.hash(&mut empty);
    eternal.s1d.hash(&mut empty);
    assert!(empty.0.is_empty());
    let legacy_json = serde_json::to_value(&eternal).unwrap();
    for key in ["s1a", "s1b", "s1d"] {
        assert!(legacy_json.get(key).is_none());
    }

    eternal.s1a.leylines_offered = true;
    eternal.s1b.noncreature_casts[0] = 1;
    eternal.s1d.combat_skips.push(P0);
    let mut a = HashInput::default();
    let mut b = HashInput::default();
    let mut d = HashInput::default();
    eternal.s1a.hash(&mut a);
    eternal.s1b.hash(&mut b);
    eternal.s1d.hash(&mut d);
    for (input, domain) in [
        (a, b"eternal-sideboard-s1a/v1"),
        (b, b"eternal-sideboard-s1b/v1"),
        (d, b"eternal-sideboard-s1d/v1"),
    ] {
        let mut prefix = HashInput::default();
        domain.hash(&mut prefix);
        assert!(input.0.starts_with(&prefix.0));
    }
}

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
        0x4539_4221_0001,
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

fn token(state: &mut GameState, player: PlayerId, name: &str) -> ObjectId {
    let object = put(state, player, name, Zone::Battlefield);
    state.objects.get_mut(object).v4.is_token = true;
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
        Decision::ChooseCostTargets { candidates, .. } if candidates.contains(&object) => {
            Action::ChooseCostTarget(object)
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

/// Casts the modal `spell` choosing `mode` (chosen silently when it is the
/// only legal one), then its targets.
fn cast_mode(state: &mut GameState, spell: ObjectId, mode: u8, targets: &[Target]) {
    assert!(castable(state, spell));
    act(state, Action::CastSpell(spell));
    if let Decision::ChooseSpellMode { legal_modes, .. } = next(state) {
        assert!(legal_modes.contains(&mode), "{mode} not in {legal_modes:?}");
        act(state, Action::ChooseSpellMode(mode));
    }
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

/// The targets `spell` may choose first.
fn cast_targets(state: &GameState, spell: ObjectId) -> Vec<Target> {
    let mut probe = state.clone();
    assert!(castable(&mut probe, spell));
    act(&mut probe, Action::CastSpell(spell));
    match next(&mut probe) {
        Decision::ChooseTargets { legal_targets, .. } => legal_targets,
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

/// P1 casts a Lightning Bolt at `target` and passes, leaving it on the
/// stack with P0 holding priority.
fn opponent_bolts(state: &mut GameState, target: Target) -> ObjectId {
    state.priority_player = P1;
    let bolt = put(state, P1, "Lightning Bolt", Zone::Hand);
    pool(state, P1, &[ManaColor::R], 0);
    cast(state, bolt, &[target]);
    assert!(matches!(
        next(state),
        Decision::CastSpellOrPass { player: P1, .. }
    ));
    act(state, Action::Pass);
    bolt
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

const S1D_CARDS: [&str; 31] = [
    "Vandalblast",
    "Crypt Incursion",
    "Circle of Protection: Blue",
    "Call Damage Control",
    "Nylea's Disciple",
    "Scattershot Archer",
    "Sentinel's Eyes",
    "Stonehorn Dignitary",
    "Standard Bearer",
    "Firebending Lesson",
    "Gruul Turf",
    "It'll Quench Ya!",
    "Origin of Metalbending",
    "Waterbending Lesson",
    "Drown in Sorrow",
    "Palace Sentinels",
    "Deglamer",
    "Martyr of Sands",
    "Navigator's Compass",
    "Last Breath",
    "Electrickery",
    "Faerie Trickery",
    "Sagittars' Volley",
    "Spreading Seas",
    "Holy Light",
    "Ultimate Alliance",
    "Coalition Honor Guard",
    "Molten Tributary",
    "Return to Dust",
    "Arashin Cleric",
    "Foot Ninjas",
];

fn life(state: &GameState, player: PlayerId) -> i32 {
    state.players[player.index()].life
}

// ---- Characteristics -----------------------------------------------------------

#[test]
fn s1d_cards_keep_their_support_labels_and_printed_characteristics() {
    for name in S1D_CARDS {
        assert_eq!(
            def(name).capability,
            crate::eternal_catalog_v1::expected_capability(name),
            "{name}"
        );
        assert!(!def(name).is_token, "{name}");
    }
    assert_eq!(def("Vandalblast").mana_value, 1);
    assert_eq!(def("Foot Ninjas").mana_value, 6);
    assert!(def("Standard Bearer")
        .subtypes
        .contains(&Subtype::Flagbearer));
    assert!(def("Coalition Honor Guard")
        .subtypes
        .contains(&Subtype::Flagbearer));
    for lesson in [
        "Firebending Lesson",
        "It'll Quench Ya!",
        "Origin of Metalbending",
        "Waterbending Lesson",
    ] {
        assert!(def(lesson).subtypes.contains(&Subtype::Lesson), "{lesson}");
    }
    assert!(def("Faerie Trickery").subtypes.contains(&Subtype::Faerie));
    assert!(def("Molten Tributary").is_land);
}

// ---- Vandalblast and Electrickery ---------------------------------------------

#[test]
fn vandalblast_destroys_one_artifact_you_dont_control() {
    let mut state = plains();
    let mine = put(&mut state, P0, "Sol Ring", Zone::Battlefield);
    let theirs = put(&mut state, P1, "Sol Ring", Zone::Battlefield);
    let other = put(&mut state, P1, "Nutrient Block", Zone::Battlefield);
    let blast = put(&mut state, P0, "Vandalblast", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 0);
    act(&mut state, Action::CastSpell(blast));
    if let Decision::ChooseSpellMode { .. } = next(&mut state) {
        act(&mut state, Action::ChooseSpellMode(0));
    }
    match next(&mut state) {
        Decision::ChooseTargets { legal_targets, .. } => {
            assert!(legal_targets.contains(&Target::Object(theirs)));
            assert!(!legal_targets.contains(&Target::Object(mine)));
        }
        other => panic!("expected targets, got {other:?}"),
    }
    act(&mut state, Action::ChooseTarget(Target::Object(theirs)));
    settled(&mut state);
    assert_eq!(zone(&state, theirs), Zone::Graveyard);
    assert_eq!(zone(&state, other), Zone::Battlefield);
    assert_eq!(zone(&state, mine), Zone::Battlefield);
    assert_eq!(zone(&state, blast), Zone::Graveyard);
}

#[test]
fn vandalblast_overload_destroys_each_opposing_artifact() {
    let mut state = plains();
    let mine = put(&mut state, P0, "Sol Ring", Zone::Battlefield);
    let first = put(&mut state, P1, "Sol Ring", Zone::Battlefield);
    let second = put(&mut state, P1, "Prophetic Prism", Zone::Battlefield);
    // Nutrient Block is indestructible.
    let sturdy = put(&mut state, P1, "Nutrient Block", Zone::Battlefield);
    let elves = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    let blast = put(&mut state, P0, "Vandalblast", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 4);
    act(&mut state, Action::CastSpell(blast));
    match next(&mut state) {
        Decision::ChooseSpellMode { legal_modes, .. } => {
            assert!(legal_modes.contains(&1), "{legal_modes:?}");
            act(&mut state, Action::ChooseSpellMode(1));
        }
        other => panic!("expected the form choice, got {other:?}"),
    }
    // The overloaded spell has no targets.
    let item = state.stack.last().expect("the spell").clone();
    assert!(item.targets.is_empty());
    settled(&mut state);
    assert_eq!(zone(&state, blast), Zone::Graveyard, "spell resolved");
    assert_eq!(zone(&state, first), Zone::Graveyard);
    assert_eq!(zone(&state, second), Zone::Graveyard);
    assert_eq!(zone(&state, sturdy), Zone::Battlefield);
    assert_eq!(zone(&state, mine), Zone::Battlefield);
    assert_eq!(zone(&state, elves), Zone::Battlefield);
    assert_eq!(zone(&state, blast), Zone::Graveyard);
}

#[test]
fn electrickery_deals_one_damage_to_a_creature_you_dont_control() {
    let mut state = plains();
    let mine = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    let theirs = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    let big = put(&mut state, P1, "Kor Skyfisher", Zone::Battlefield);
    let spell = put(&mut state, P0, "Electrickery", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 0);
    act(&mut state, Action::CastSpell(spell));
    if let Decision::ChooseSpellMode { .. } = next(&mut state) {
        act(&mut state, Action::ChooseSpellMode(0));
    }
    match next(&mut state) {
        Decision::ChooseTargets { legal_targets, .. } => {
            assert!(legal_targets.contains(&Target::Object(theirs)));
            assert!(!legal_targets.contains(&Target::Object(mine)));
        }
        other => panic!("expected targets, got {other:?}"),
    }
    act(&mut state, Action::ChooseTarget(Target::Object(theirs)));
    settled(&mut state);
    assert_eq!(zone(&state, theirs), Zone::Graveyard);
    assert_eq!(zone(&state, mine), Zone::Battlefield);
    assert_eq!(state.objects.get(big).damage, 0);
}

#[test]
fn electrickery_overload_hits_each_creature_you_dont_control() {
    let mut state = plains();
    let mine = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    let first = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    let second = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    let big = put(&mut state, P1, "Kor Skyfisher", Zone::Battlefield);
    let spell = put(&mut state, P0, "Electrickery", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 1);
    act(&mut state, Action::CastSpell(spell));
    act(&mut state, Action::ChooseSpellMode(1));
    settled(&mut state);
    assert_eq!(zone(&state, first), Zone::Graveyard);
    assert_eq!(zone(&state, second), Zone::Graveyard);
    assert_eq!(zone(&state, mine), Zone::Battlefield);
    assert_eq!(state.objects.get(big).damage, 1);
}

#[test]
fn overload_uses_paid_graveyard_permission_and_restores_both_seats() {
    use mtg_kernel::state::{CastMethodV4, SpellCastRouteV4};
    for actor in [P0, P1] {
        for (name, victim, mine, generic) in [
            ("Vandalblast", "Sol Ring", "Prophetic Prism", 4),
            ("Electrickery", "Llanowar Elves", "Kor Skyfisher", 1),
        ] {
            let mut state = plains();
            state.active_player = actor;
            state.priority_player = actor;
            let other = actor.opponent();
            let spell = put(&mut state, actor, name, Zone::Graveyard);
            let will = put(&mut state, actor, "Yawgmoth's Will", Zone::Hand);
            let mine = put(&mut state, actor, mine, Zone::Battlefield);
            let first = put(&mut state, other, victim, Zone::Battlefield);
            let second = put(&mut state, other, victim, Zone::Battlefield);
            pool(&mut state, actor, &[ManaColor::B], 2);
            cast(&mut state, will, &[]);
            settled(&mut state);
            assert_eq!(zone(&state, will), Zone::Exile);
            pool(&mut state, actor, &[ManaColor::R], generic);
            assert!(castable(&mut state, spell));
            act(&mut state, Action::CastSpell(spell));
            let prompt = next(&mut state);
            assert!(
                matches!(&prompt, Decision::ChooseSpellMode { legal_modes, .. } if legal_modes == &[0, 1])
            );
            let mut restored: GameState =
                serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
            assert_eq!(prompt, next(&mut restored));
            for continued in [&mut state, &mut restored] {
                act(continued, Action::ChooseSpellMode(1));
                assert!(matches!(next(continued), Decision::CastSpellOrPass { .. }));
                let item = continued.stack.last().unwrap();
                assert_eq!(item.v4.cast_method, Some(CastMethodV4::Omen));
                assert!(item.targets.is_empty());
                assert_eq!(engine::object_mana_value(continued, spell), 1);
                assert!(matches!(
                    continued.objects.get(spell).v4.spell_cast_origin.unwrap().route,
                    SpellCastRouteV4::GraveyardPermissionV1 { holder, .. } if holder == actor
                ));
                assert_eq!(continued.players[actor.index()].mana_pool, [0; 6]);
                settled(continued);
                assert_eq!(zone(continued, spell), Zone::Exile);
                assert_eq!(zone(continued, mine), Zone::Battlefield);
                assert_eq!(zone(continued, first), Zone::Graveyard);
                assert_eq!(zone(continued, second), Zone::Graveyard);
            }
        }
    }
}

#[test]
fn overload_with_retrace_pays_the_land_discard_and_mana_both_seats() {
    for actor in [P0, P1] {
        let mut state = plains();
        state.active_player = actor;
        state.priority_player = actor;
        let wrenn = put(&mut state, actor, "Wrenn and Six", Zone::Hand);
        pool(&mut state, actor, &[ManaColor::R, ManaColor::G], 0);
        cast(&mut state, wrenn, &[]);
        settled(&mut state);
        mtg_kernel::planeswalker_v1::change_loyalty(&mut state, wrenn, 4);
        activate(&mut state, wrenn, 2, &[]);
        settled(&mut state);

        let spell = put(&mut state, actor, "Electrickery", Zone::Graveyard);
        let land = put(&mut state, actor, "Plains", Zone::Hand);
        put(&mut state, actor, "Lightning Bolt", Zone::Hand);
        let first = put(
            &mut state,
            actor.opponent(),
            "Llanowar Elves",
            Zone::Battlefield,
        );
        let second = put(
            &mut state,
            actor.opponent(),
            "Llanowar Elves",
            Zone::Battlefield,
        );
        pool(&mut state, actor, &[ManaColor::R], 1);
        act(&mut state, Action::CastSpell(spell));
        assert!(
            matches!(next(&mut state), Decision::ChooseSpellMode { legal_modes, .. } if legal_modes == [0, 1])
        );
        act(&mut state, Action::ChooseSpellMode(1));
        assert!(matches!(
            next(&mut state),
            Decision::ChooseCostTargets { candidates, .. } if candidates == [land]
        ));
        act(&mut state, Action::ChooseCostTarget(land));
        settled(&mut state);
        assert_eq!(zone(&state, land), Zone::Graveyard);
        assert_eq!(zone(&state, first), Zone::Graveyard);
        assert_eq!(zone(&state, second), Zone::Graveyard);
        assert_eq!(state.players[actor.index()].mana_pool, [0; 6]);
    }
}

#[test]
fn free_cascade_cannot_offer_overload_both_seats() {
    use mtg_kernel::state::CastMethodV4;
    for actor in [P0, P1] {
        for (name, victim, generic) in [
            ("Vandalblast", "Sol Ring", 4),
            ("Electrickery", "Llanowar Elves", 1),
        ] {
            for ordinary_targets in [true, false] {
                let mut state = game(&["Island", name, "Plains", "Plains"]);
                state.active_player = actor;
                state.priority_player = actor;
                let other = actor.opponent();
                let spell = state.players[actor.index()].library[1];
                let victims = ordinary_targets.then(|| {
                    (
                        put(&mut state, other, victim, Zone::Battlefield),
                        put(&mut state, other, victim, Zone::Battlefield),
                    )
                });
                let party = put(&mut state, actor, "Boarding Party", Zone::Hand);
                pool(
                    &mut state,
                    actor,
                    &[ManaColor::R, ManaColor::R],
                    5 + generic,
                );
                cast(&mut state, party, &[]);
                if let Some((first, second)) = victims {
                    assert!(matches!(
                        settle(&mut state),
                        Some(Decision::ChooseEffectOption {
                            option_count: 2,
                            ..
                        })
                    ));
                    let mut restored: GameState =
                        serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
                    act(&mut restored, Action::ChooseEffectOption(0));
                    assert!(matches!(
                        next(&mut restored),
                        Decision::ChooseTargets { legal_targets, can_finish: false, .. }
                            if legal_targets.contains(&Target::Object(first))
                                && legal_targets.contains(&Target::Object(second))
                    ));
                    let mut forged = restored.clone();
                    forged.engine.pending_cast.as_mut().unwrap().mode_chosen = Some(1);
                    assert!(matches!(
                        engine::advance_until_decision(&mut forged),
                        Decision::Halted { .. }
                    ));
                    assert!(engine::step(&mut restored, Action::ChooseSpellMode(1)).is_err());
                    act(&mut restored, Action::ChooseTarget(Target::Object(first)));
                    assert!(matches!(
                        next(&mut restored),
                        Decision::CastSpellOrPass { .. }
                    ));
                    assert_eq!(
                        restored.stack.last().unwrap().v4.cast_method,
                        Some(CastMethodV4::Normal)
                    );
                    settled(&mut restored);
                    assert_eq!(zone(&restored, spell), Zone::Graveyard);
                    assert_eq!(zone(&restored, first), Zone::Graveyard);
                    assert_eq!(zone(&restored, second), Zone::Battlefield);
                    state = restored;
                } else {
                    settled(&mut state);
                    assert_eq!(zone(&state, spell), Zone::Library);
                }
                assert_eq!(zone(&state, party), Zone::Battlefield);
                assert_eq!(
                    state.players[actor.index()].mana_pool.iter().sum::<u8>(),
                    generic + 1
                );
            }
        }
    }
}

// ---- Graveyard and library cards --------------------------------------------------

#[test]
fn crypt_incursion_exiles_creature_cards_and_gains_three_each() {
    let mut state = plains();
    let first = put(&mut state, P1, "Llanowar Elves", Zone::Graveyard);
    let second = put(&mut state, P1, "Kor Skyfisher", Zone::Graveyard);
    let bolt = put(&mut state, P1, "Lightning Bolt", Zone::Graveyard);
    let mine = put(&mut state, P0, "Llanowar Elves", Zone::Graveyard);
    let spell = put(&mut state, P0, "Crypt Incursion", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 2);
    cast(&mut state, spell, &[Target::Player(P1)]);
    settled(&mut state);
    assert_eq!(zone(&state, first), Zone::Exile);
    assert_eq!(zone(&state, second), Zone::Exile);
    assert_eq!(zone(&state, bolt), Zone::Graveyard);
    assert_eq!(zone(&state, mine), Zone::Graveyard);
    assert_eq!(life(&state, P0), 26);
}

#[test]
fn call_damage_control_returns_up_to_two_cards_of_different_types() {
    let mut state = plains();
    let creature = put(&mut state, P0, "Llanowar Elves", Zone::Graveyard);
    let other_creature = put(&mut state, P0, "Kor Skyfisher", Zone::Graveyard);
    let land = put(&mut state, P0, "Forest", Zone::Graveyard);
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Graveyard);
    let theirs = put(&mut state, P1, "Sol Ring", Zone::Graveyard);
    let spell = put(&mut state, P0, "Call Damage Control", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 1);
    let first = cast_targets(&state, spell);
    assert!(first.contains(&Target::Object(creature)));
    assert!(first.contains(&Target::Object(land)));
    assert!(!first.contains(&Target::Object(bolt)));
    assert!(!first.contains(&Target::Object(theirs)));
    act(&mut state, Action::CastSpell(spell));
    choose_target(&mut state, Target::Object(creature));
    // A second creature card fills no other mode.
    match next(&mut state) {
        Decision::ChooseTargets { legal_targets, .. } => {
            assert!(!legal_targets.contains(&Target::Object(other_creature)));
            assert!(legal_targets.contains(&Target::Object(land)));
        }
        other => panic!("expected targets, got {other:?}"),
    }
    act(&mut state, Action::ChooseTarget(Target::Object(land)));
    settle_first(&mut state);
    assert_eq!(zone(&state, creature), Zone::Hand);
    assert_eq!(zone(&state, land), Zone::Hand);
    assert_eq!(zone(&state, other_creature), Zone::Graveyard);
}

#[test]
fn deglamer_shuffles_an_artifact_or_enchantment_into_its_owners_library() {
    let mut state = plains();
    let artifact = put(&mut state, P1, "Sol Ring", Zone::Battlefield);
    let spell = put(&mut state, P0, "Deglamer", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 1);
    let library = state.players[1].library.len();
    cast(&mut state, spell, &[Target::Object(artifact)]);
    settled(&mut state);
    assert_eq!(zone(&state, artifact), Zone::Library);
    assert_eq!(state.players[1].library.len(), library + 1);
}

// ---- Circle of Protection: Blue ---------------------------------------------------

#[test]
fn circle_of_protection_blue_prevents_the_next_damage_from_a_chosen_blue_source() {
    let mut state = plains();
    let circle = put(
        &mut state,
        P0,
        "Circle of Protection: Blue",
        Zone::Battlefield,
    );
    let blue = put(&mut state, P1, "Mulldrifter", Zone::Battlefield);
    let other_blue = put(&mut state, P1, "Mulldrifter", Zone::Battlefield);
    let red = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    pool(&mut state, P0, &[], 1);
    activate(&mut state, circle, 0, &[]);
    // The blue sources are offered in object order; the first is `blue`.
    settle_option(&mut state, 0);
    let deal = |state: &mut GameState, source: ObjectId| {
        mtg_kernel::event::propose_and_commit(
            state,
            mtg_kernel::event::ProposedEvent::damage(source, Target::Player(P0), 2),
        );
    };
    // An unchosen source and a nonblue source are not prevented.
    deal(&mut state, other_blue);
    assert_eq!(life(&state, P0), 18);
    deal(&mut state, red);
    assert_eq!(life(&state, P0), 16);
    // The chosen source is prevented once, then deals damage.
    deal(&mut state, blue);
    assert_eq!(life(&state, P0), 16);
    deal(&mut state, blue);
    assert_eq!(life(&state, P0), 14);
}

// ---- Creatures with triggers -----------------------------------------------------

#[test]
fn arashin_cleric_and_navigators_compass_gain_three_life_as_they_enter() {
    for (name, colored, generic) in [
        ("Arashin Cleric", vec![ManaColor::W], 1),
        ("Navigator's Compass", vec![], 1),
    ] {
        let mut state = plains();
        let card = put(&mut state, P0, name, Zone::Hand);
        pool(&mut state, P0, &colored, generic);
        cast(&mut state, card, &[]);
        settled(&mut state);
        assert_eq!(zone(&state, card), Zone::Battlefield, "{name}");
        assert_eq!(life(&state, P0), 23, "{name}");
    }
}

#[test]
fn nyleas_disciple_gains_life_equal_to_devotion_to_green() {
    let mut state = plains();
    put(&mut state, P0, "Scattershot Archer", Zone::Battlefield);
    put(&mut state, P1, "Scattershot Archer", Zone::Battlefield);
    let disciple = put(&mut state, P0, "Nylea's Disciple", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G, ManaColor::G], 2);
    cast(&mut state, disciple, &[]);
    settled(&mut state);
    // {2}{G}{G} and {G}: three green symbols among permanents you control.
    assert_eq!(life(&state, P0), 23);
}

#[test]
fn palace_sentinels_makes_its_controller_the_monarch() {
    let mut state = plains();
    state.monarch = Some(P1);
    let card = put(&mut state, P0, "Palace Sentinels", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W], 3);
    cast(&mut state, card, &[]);
    settled(&mut state);
    assert_eq!(state.monarch, Some(P0));
}

#[test]
fn stonehorn_dignitary_makes_the_opponent_skip_their_next_combat() {
    let mut state = plains();
    let card = put(&mut state, P0, "Stonehorn Dignitary", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W], 3);
    cast(&mut state, card, &[]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseTargets { .. } => Action::ChooseTarget(Target::Player(P1)),
        other => first_answer(other),
    });
    assert_eq!(zone(&state, card), Zone::Battlefield);
    // Play on to P1's postcombat main phase without P1 ever beginning combat.
    let saw_combat = std::cell::Cell::new(false);
    run_until(
        &mut state,
        |state| {
            if state.active_player == P1
                && matches!(
                    state.step,
                    Step::BeginCombat | Step::DeclareAttackers | Step::EndCombat
                )
            {
                saw_combat.set(true);
            }
            state.active_player == P1 && state.step == Step::Main2
        },
        |_, _| None,
    );
    assert!(!saw_combat.get());
}

// ---- Scattershot Archer, Sagittars' Volley, Holy Light, Drown in Sorrow ----------

#[test]
fn scattershot_archer_damages_each_creature_with_flying() {
    let mut state = plains();
    let archer = put(&mut state, P0, "Scattershot Archer", Zone::Battlefield);
    let mine = put(&mut state, P0, "Kor Skyfisher", Zone::Battlefield);
    let theirs = put(&mut state, P1, "Kor Skyfisher", Zone::Battlefield);
    let ground = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    activate(&mut state, archer, 0, &[]);
    settled(&mut state);
    assert_eq!(state.objects.get(mine).damage, 1);
    assert_eq!(state.objects.get(theirs).damage, 1);
    assert_eq!(state.objects.get(ground).damage, 0);
    assert!(state.objects.get(archer).tapped);
}

#[test]
fn sagittars_volley_destroys_a_flier_and_pings_opposing_fliers() {
    let mut state = plains();
    let mine = put(&mut state, P0, "Kor Skyfisher", Zone::Battlefield);
    let first = put(&mut state, P1, "Kor Skyfisher", Zone::Battlefield);
    let second = put(&mut state, P1, "Kor Skyfisher", Zone::Battlefield);
    let ground = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    let spell = put(&mut state, P0, "Sagittars' Volley", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 2);
    let legal = cast_targets(&state, spell);
    assert!(legal.contains(&Target::Object(first)));
    assert!(!legal.contains(&Target::Object(ground)));
    cast(&mut state, spell, &[Target::Object(first)]);
    settled(&mut state);
    assert_eq!(zone(&state, first), Zone::Graveyard);
    assert_eq!(state.objects.get(second).damage, 1);
    assert_eq!(state.objects.get(mine).damage, 0);
    assert_eq!(state.objects.get(ground).damage, 0);
}

#[test]
fn holy_light_shrinks_only_nonwhite_creatures() {
    let mut state = plains();
    let white = put(&mut state, P0, "Kor Skyfisher", Zone::Battlefield);
    let green = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    let small = put(&mut state, P1, "Scattershot Archer", Zone::Battlefield);
    let spell = put(&mut state, P0, "Holy Light", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W], 2);
    cast(&mut state, spell, &[]);
    settled(&mut state);
    assert_eq!(power_toughness(&state, white), (2, 3));
    assert_eq!(zone(&state, green), Zone::Graveyard);
    assert_eq!(power_toughness(&state, small), (0, 1));
}

#[test]
fn drown_in_sorrow_gives_all_creatures_minus_two_minus_two() {
    let mut state = plains();
    let mine = put(&mut state, P0, "Kor Skyfisher", Zone::Battlefield);
    let elves = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    let spell = put(&mut state, P0, "Drown in Sorrow", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B, ManaColor::B], 1);
    cast(&mut state, spell, &[]);
    settle_first(&mut state);
    assert_eq!(power_toughness(&state, mine), (0, 1));
    assert_eq!(zone(&state, elves), Zone::Graveyard);
}

// ---- Instants ------------------------------------------------------------------

#[test]
fn last_breath_exiles_a_small_creature_and_its_controller_gains_four() {
    let mut state = plains();
    let small = put(&mut state, P1, "Kor Skyfisher", Zone::Battlefield);
    let big = put(&mut state, P1, "Annoyed Altisaur", Zone::Battlefield);
    let spell = put(&mut state, P0, "Last Breath", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W], 1);
    let legal = cast_targets(&state, spell);
    assert!(legal.contains(&Target::Object(small)));
    assert!(!legal.contains(&Target::Object(big)));
    cast(&mut state, spell, &[Target::Object(small)]);
    settled(&mut state);
    assert_eq!(zone(&state, small), Zone::Exile);
    assert_eq!(life(&state, P1), 24);
    assert_eq!(life(&state, P0), 20);
}

#[test]
fn firebending_lesson_deals_two_damage_or_five_when_kicked() {
    let mut state = plains();
    let target = put(&mut state, P1, "Annoyed Altisaur", Zone::Battlefield);
    let spell = put(&mut state, P0, "Firebending Lesson", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 0);
    act(&mut state, Action::CastSpell(spell));
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseTargets { .. } => Action::ChooseTarget(Target::Object(target)),
        other => first_answer(other),
    });
    assert_eq!(state.objects.get(target).damage, 2);

    let mut state = plains();
    let target = put(&mut state, P1, "Annoyed Altisaur", Zone::Battlefield);
    let spell = put(&mut state, P0, "Firebending Lesson", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 4);
    act(&mut state, Action::CastSpell(spell));
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseKicker { .. } => Action::ChooseKicker(true),
        Decision::ChooseTargets { .. } => Action::ChooseTarget(Target::Object(target)),
        other => first_answer(other),
    });
    assert_eq!(zone(&state, target), Zone::Graveyard);
}

#[test]
fn it_ll_quench_ya_counters_unless_its_controller_pays_two() {
    let mut state = plains();
    let bolt = opponent_bolts(&mut state, Target::Player(P0));
    state.priority_player = P0;
    let spell = put(&mut state, P0, "It'll Quench Ya!", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 1);
    cast(&mut state, spell, &[Target::Object(bolt)]);
    settled(&mut state);
    assert_eq!(zone(&state, bolt), Zone::Graveyard);
    assert_eq!(life(&state, P0), 20);
}

#[test]
fn origin_of_metalbending_destroys_or_grows_and_protects() {
    let mut state = plains();
    let artifact = put(&mut state, P1, "Sol Ring", Zone::Battlefield);
    let spell = put(&mut state, P0, "Origin of Metalbending", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 1);
    cast_mode(&mut state, spell, 0, &[Target::Object(artifact)]);
    settled(&mut state);
    assert_eq!(zone(&state, artifact), Zone::Graveyard);

    let mut state = plains();
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    let spell = put(&mut state, P0, "Origin of Metalbending", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 1);
    cast_mode(&mut state, spell, 1, &[Target::Object(elves)]);
    settled(&mut state);
    assert_eq!(power_toughness(&state, elves), (2, 2));
    assert!(keyword(&state, elves, Keywords::INDESTRUCTIBLE));
}

#[test]
fn faerie_trickery_exiles_the_countered_non_faerie_spell() {
    let mut state = plains();
    let bolt = opponent_bolts(&mut state, Target::Player(P0));
    state.priority_player = P0;
    let spell = put(&mut state, P0, "Faerie Trickery", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U, ManaColor::U], 1);
    cast(&mut state, spell, &[Target::Object(bolt)]);
    settled(&mut state);
    assert_eq!(zone(&state, bolt), Zone::Exile);
    assert_eq!(life(&state, P0), 20);

    // A Faerie spell is not a legal target.
    let mut state = plains();
    state.priority_player = P1;
    let borrower = put(&mut state, P1, "Brazen Borrower", Zone::Hand);
    pool(&mut state, P1, &[ManaColor::U, ManaColor::U], 1);
    cast(&mut state, borrower, &[]);
    assert!(matches!(
        next(&mut state),
        Decision::CastSpellOrPass { player: P1, .. }
    ));
    act(&mut state, Action::Pass);
    state.priority_player = P0;
    let spell = put(&mut state, P0, "Faerie Trickery", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U, ManaColor::U], 1);
    assert!(!castable(&mut state, spell));
}

#[test]
fn ultimate_alliance_deals_damage_equal_to_your_creature_count() {
    let mut state = plains();
    put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    put(&mut state, P0, "Kor Skyfisher", Zone::Battlefield);
    let target = put(&mut state, P1, "Annoyed Altisaur", Zone::Battlefield);
    let spell = put(&mut state, P0, "Ultimate Alliance", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W], 0);
    cast(&mut state, spell, &[Target::Object(target)]);
    settled(&mut state);
    assert_eq!(state.objects.get(target).damage, 3);
}

fn finish_targets_if_offered(state: &mut GameState) {
    if let Decision::ChooseTargets {
        can_finish: true, ..
    } = next(state)
    {
        act(state, Action::FinishEffectSelection);
    }
}

#[test]
fn return_to_dust_exiles_two_in_your_main_phase() {
    let mut state = plains();
    let first = put(&mut state, P1, "Sol Ring", Zone::Battlefield);
    let second = put(&mut state, P1, "Nutrient Block", Zone::Battlefield);
    let spell = put(&mut state, P0, "Return to Dust", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W, ManaColor::W], 2);
    act(&mut state, Action::CastSpell(spell));
    choose_target(&mut state, Target::Object(first));
    choose_target(&mut state, Target::Object(second));
    finish_targets_if_offered(&mut state);
    settle_option(&mut state, 1);
    assert_eq!(zone(&state, first), Zone::Exile);
    assert_eq!(zone(&state, second), Zone::Exile);
}

#[test]
fn return_to_dust_exiles_only_one_outside_your_main_phase() {
    let mut state = plains();
    state.active_player = P1;
    state.priority_player = P0;
    let first = put(&mut state, P1, "Sol Ring", Zone::Battlefield);
    let second = put(&mut state, P1, "Nutrient Block", Zone::Battlefield);
    let spell = put(&mut state, P0, "Return to Dust", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W, ManaColor::W], 2);
    act(&mut state, Action::CastSpell(spell));
    choose_target(&mut state, Target::Object(first));
    choose_target(&mut state, Target::Object(second));
    finish_targets_if_offered(&mut state);
    settle_first(&mut state);
    assert_eq!(zone(&state, first), Zone::Exile);
    assert_eq!(zone(&state, second), Zone::Battlefield);
}

// ---- Waterbending Lesson -------------------------------------------------------

#[test]
fn waterbending_lesson_discards_without_a_way_to_waterbend() {
    let mut state = plains();
    let spell = put(&mut state, P0, "Waterbending Lesson", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 3);
    cast(&mut state, spell, &[]);
    settle_first(&mut state);
    assert_eq!(state.players[0].hand.len(), 2);
}

#[test]
fn waterbending_lesson_keeps_the_cards_when_you_tap_creatures() {
    let mut state = plains();
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    let skyfisher = put(&mut state, P0, "Kor Skyfisher", Zone::Battlefield);
    let spell = put(&mut state, P0, "Waterbending Lesson", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 3);
    cast(&mut state, spell, &[]);
    // Option 1 waterbends; then the first option taps a creature (the mana
    // option, when offered, is the first).
    let mut taps = 0;
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseEffectOption { .. } if taps == 0 => {
            taps += 1;
            Action::ChooseEffectOption(1)
        }
        other => first_answer(other),
    });
    assert_eq!(state.players[0].hand.len(), 3);
    let _ = (elves, skyfisher);
}

// ---- Spreading Seas and Navigator's Compass ---------------------------------------

#[test]
fn spreading_seas_makes_the_enchanted_land_an_island_and_draws() {
    let mut state = plains();
    let forest = put(&mut state, P1, "Forest", Zone::Battlefield);
    let seas = put(&mut state, P0, "Spreading Seas", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 1);
    let hand = state.players[0].hand.len();
    cast(&mut state, seas, &[Target::Object(forest)]);
    settled(&mut state);
    assert_eq!(zone(&state, seas), Zone::Battlefield);
    // The Aura left the hand and a card was drawn.
    assert_eq!(state.players[0].hand.len(), hand);
    assert!(engine::has_effective_subtype(
        &state,
        forest,
        Subtype::Island
    ));
    assert!(!engine::has_effective_subtype(
        &state,
        forest,
        Subtype::Forest
    ));
}

#[test]
fn navigators_compass_adds_a_basic_land_type_until_end_of_turn() {
    let mut state = plains();
    let compass = put(&mut state, P0, "Navigator's Compass", Zone::Battlefield);
    let mountain = put(&mut state, P0, "Mountain", Zone::Battlefield);
    activate(&mut state, compass, 0, &[Target::Object(mountain)]);
    // Options: Plains, Island, Swamp, Mountain, Forest.
    settle_option(&mut state, 1);
    assert!(engine::has_effective_subtype(
        &state,
        mountain,
        Subtype::Island
    ));
    assert!(engine::has_effective_subtype(
        &state,
        mountain,
        Subtype::Mountain
    ));
    next(&mut state);
    act(
        &mut state,
        Action::ActivateManaAbilityChoice(mountain, ManaColor::U),
    );
    assert_eq!(mana(&state, P0, ManaColor::U), 1);
}

// ---- Lands -----------------------------------------------------------------

#[test]
fn molten_tributary_enters_tapped_and_taps_for_blue_or_red() {
    tapped_dual("Molten Tributary", [ManaColor::U, ManaColor::R]);
    let mut state = plains();
    let land = put(&mut state, P0, "Molten Tributary", Zone::Battlefield);
    assert!(engine::has_effective_subtype(&state, land, Subtype::Island));
    assert!(engine::has_effective_subtype(
        &state,
        land,
        Subtype::Mountain
    ));
}

#[test]
fn gruul_turf_returns_a_land_and_taps_for_red_and_green() {
    let mut state = plains();
    let forest = put(&mut state, P0, "Forest", Zone::Battlefield);
    let turf = put(&mut state, P0, "Gruul Turf", Zone::Hand);
    play_land(&mut state, turf);
    assert!(state.objects.get(turf).tapped);
    assert!(zone(&state, forest) == Zone::Hand || zone(&state, turf) == Zone::Hand);

    let mut state = plains();
    let turf = put(&mut state, P0, "Gruul Turf", Zone::Battlefield);
    next(&mut state);
    act(&mut state, Action::ActivateManaAbility(turf));
    assert_eq!(mana(&state, P0, ManaColor::R), 1);
    assert_eq!(mana(&state, P0, ManaColor::G), 1);
}

// ---- Auras and Flagbearers -------------------------------------------------------

#[test]
fn sentinels_eyes_gives_plus_one_plus_one_and_vigilance() {
    let mut state = plains();
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    let eyes = put(&mut state, P0, "Sentinel's Eyes", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W], 0);
    cast(&mut state, eyes, &[Target::Object(elves)]);
    settled(&mut state);
    assert_eq!(power_toughness(&state, elves), (2, 2));
    assert!(keyword(&state, elves, Keywords::VIGILANCE));
}

#[test]
fn sentinels_eyes_escapes_from_the_graveyard() {
    let mut state = plains();
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    let eyes = put(&mut state, P0, "Sentinel's Eyes", Zone::Graveyard);
    put(&mut state, P0, "Lightning Bolt", Zone::Graveyard);
    put(&mut state, P0, "Lightning Bolt", Zone::Graveyard);
    pool(&mut state, P0, &[ManaColor::W], 0);
    act(&mut state, Action::CastSpell(eyes));
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseTargets { .. } => Action::ChooseTarget(Target::Object(elves)),
        other => first_answer(other),
    });
    assert_eq!(zone(&state, eyes), Zone::Battlefield);
    assert_eq!(power_toughness(&state, elves), (2, 2));
}

#[test]
fn flagbearers_force_the_opponent_to_target_them() {
    for name in ["Standard Bearer", "Coalition Honor Guard"] {
        let mut state = plains();
        let bearer = put(&mut state, P0, name, Zone::Battlefield);
        let other = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
        state.priority_player = P1;
        let bolt = put(&mut state, P1, "Lightning Bolt", Zone::Hand);
        pool(&mut state, P1, &[ManaColor::R], 0);
        let legal = cast_targets(&state, bolt);
        assert!(legal.contains(&Target::Object(bearer)), "{name}");
        assert!(!legal.contains(&Target::Object(other)), "{name}");
        assert!(!legal.contains(&Target::Player(P0)), "{name}");
        // The Flagbearer's controller is not restricted.
        let mut state = plains();
        put(&mut state, P0, name, Zone::Battlefield);
        let theirs = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
        let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
        pool(&mut state, P0, &[ManaColor::R], 0);
        let legal = cast_targets(&state, bolt);
        assert!(legal.contains(&Target::Object(theirs)), "{name}");
        assert!(legal.contains(&Target::Player(P1)), "{name}");
    }
}

// ---- Martyr of Sands -------------------------------------------------------------

#[test]
fn martyr_of_sands_pays_zero_exact_subset_and_more_than_ten_both_seats() {
    use mtg_kernel::{event::CommittedEvent, rl, surface_v2::HarnessSurfaceV2};
    use std::collections::BTreeSet;

    let library = [id("Plains"); 20];
    for actor in [P0, P1] {
        let other = actor.opponent();
        for (white_count, picks) in [
            (4, vec![1, 3]),
            (0, vec![]),
            (3, vec![]),
            (12, (1..12).collect()),
        ] {
            let mut state = GameState::new_from_libraries_with_starting_player_v1(
                &library,
                &library,
                |id| CARD_DEFS[usize::from(id)].object_name.into(),
                566,
                actor,
            );
            state.step = Step::Main1;
            state.engine.spellbench_neutral_v1.choices = white_count == 4;
            state.enable_hand_knowledge_by_name_v2();
            let martyr = put(&mut state, other, "Martyr of Sands", Zone::Battlefield);
            state.players[other.index()]
                .battlefield
                .retain(|&object| object != martyr);
            state.players[actor.index()].battlefield.push(martyr);
            state.objects.get_mut(martyr).controller = actor;
            let land = put(&mut state, actor, "Plains", Zone::Battlefield);
            let white: Vec<_> = (0..white_count)
                .map(|i| {
                    put(
                        &mut state,
                        actor,
                        if i == 0 {
                            "Kor Skyfisher"
                        } else {
                            "Thraben Inspector"
                        },
                        Zone::Hand,
                    )
                })
                .collect();
            let nonwhite = put(&mut state, actor, "Llanowar Elves", Zone::Hand);
            let opponent_card = put(&mut state, other, "Kor Skyfisher", Zone::Hand);
            assert_eq!(def("Martyr of Sands").activated_abilities.len(), 1);
            assert!(!activatable(&mut state, martyr, 1));
            activate(&mut state, martyr, 0, &[]);
            let initial = next(&mut state);
            assert!(matches!(initial, Decision::ChooseEffectTargets {
                player, source, selected_count: 0, min_targets: 0, max_targets, legal_targets, can_finish: true,
            } if player == actor && source == martyr && usize::from(max_targets) == white_count
                && legal_targets == white.iter().copied().map(Target::Object).collect::<Vec<_>>()));
            for illegal in [nonwhite, opponent_card] {
                let before = state.state_hash();
                assert!(engine::step(
                    &mut state,
                    Action::ChooseEffectTarget(Target::Object(illegal))
                )
                .is_err());
                assert_eq!(state.state_hash(), before);
            }
            for &pick in &picks {
                let selected = white[pick];
                act(
                    &mut state,
                    Action::ChooseEffectTarget(Target::Object(selected)),
                );
                let before = state.state_hash();
                assert!(engine::step(
                    &mut state,
                    Action::ChooseEffectTarget(Target::Object(selected))
                )
                .is_err());
                assert_eq!(state.state_hash(), before);
            }
            assert!(state.known_hand_cards(other, actor).is_empty());
            assert!(!state.objects.get(land).tapped);
            assert_eq!(zone(&state, martyr), Zone::Battlefield);
            assert!(state.stack.is_empty());
            let surface = HarnessSurfaceV2::default();
            let chosen = |state: &GameState, viewer| {
                rl::observe_v2(state, &surface, viewer, 1)
                    .unwrap()
                    .projection
                    .engine_context
                    .pending_activation
                    .unwrap()
                    .object_cost_chosen
            };
            assert_eq!(
                chosen(&state, actor)
                    .iter()
                    .map(|row| row.arena_id)
                    .collect::<Vec<_>>(),
                picks.iter().map(|&i| white[i].0).collect::<Vec<_>>()
            );
            assert!(chosen(&state, other).is_empty());

            let mut restored: GameState =
                serde_json::from_value(serde_json::to_value(&state).unwrap()).unwrap();
            assert_eq!(next(&mut restored), next(&mut state));
            if !picks.is_empty() {
                let mut stale = restored.clone();
                stale
                    .engine
                    .pending_activation
                    .as_mut()
                    .unwrap()
                    .object_cost_chosen[0]
                    .expected_zone_change_count += 1;
                let before = stale.state_hash();
                assert!(engine::step(&mut stale, Action::FinishEffectSelection).is_err());
                assert_eq!(stale.state_hash(), before);
            }
            if state.engine.spellbench_neutral_v1.choices {
                let mut unpublishable = restored.clone();
                assert!(unpublishable
                    .engine
                    .spellbench_neutral_v1
                    .pending_activation
                    .take()
                    .is_some());
                let before = unpublishable.state_hash();
                assert!(engine::step(&mut unpublishable, Action::FinishEffectSelection).is_err());
                assert_eq!(unpublishable.state_hash(), before);
                assert!(unpublishable.known_hand_cards(other, actor).is_empty());
                assert!(!unpublishable.objects.get(land).tapped);
                assert_eq!(zone(&unpublishable, martyr), Zone::Battlefield);
            }
            act(&mut restored, Action::FinishEffectSelection);
            let ability = restored.stack.last().expect("actual paid Martyr ability");
            assert_eq!(ability.kind, StackItemKind::ActivatedAbility);
            assert_eq!(ability.source, martyr);
            assert_eq!(ability.controller, actor);
            assert_eq!(ability.v4.activated_ability_index, Some(0));
            assert_eq!(usize::from(ability.v4.x_value), picks.len());
            assert!(restored.objects.get(land).tapped);
            assert!(restored.players[other.index()].graveyard.contains(&martyr));
            assert_eq!(life(&restored, actor), 20);
            assert_eq!(restored.players[actor.index()].hand.len(), white_count + 1);
            let actual: BTreeSet<_> = restored
                .known_hand_cards(other, actor)
                .iter()
                .map(|entry| entry.object)
                .collect();
            assert_eq!(actual, picks.iter().map(|&i| white[i]).collect());
            assert!(restored.known_hand_cards(actor, other).is_empty());
            assert!(restored.engine.event_history.iter().any(|event| matches!(event,
                CommittedEvent::MartyrRevealPaidV1 { item, revealed, .. }
                    if *item == ability.v4.stack_item_id && revealed == &vec![id("Thraben Inspector"); picks.len()])));

            let mut paid: GameState =
                serde_json::from_value(serde_json::to_value(&restored).unwrap()).unwrap();
            let mut forged = paid.clone();
            forged.stack.last_mut().unwrap().v4.x_value += 1;
            assert!(rl::observe_v2(&forged, &surface, other, 2).is_err());
            settled(&mut paid);
            assert_eq!(
                life(&paid, actor),
                20 + 3 * i32::try_from(picks.len()).unwrap()
            );
        }
    }
}

// ---- Foot Ninjas -------------------------------------------------------------------

#[test]
fn foot_ninjas_cast_with_hybrid_mana_gains_three_life() {
    let mut state = plains();
    let ninjas = put(&mut state, P0, "Foot Ninjas", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W, ManaColor::B], 4);
    cast(&mut state, ninjas, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, ninjas), Zone::Battlefield);
    assert_eq!(power_toughness(&state, ninjas), (5, 5));
    assert_eq!(life(&state, P0), 23);
    assert!(def("Foot Ninjas").alt_cost.is_some());
}
