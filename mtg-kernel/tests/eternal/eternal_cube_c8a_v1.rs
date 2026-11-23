//! Vintage Cube wave C8a: native rules and public-session tests for the
//! Wave C8A Vintage Cube cards.
#![cfg(feature = "eternal-fixtures")]
#![allow(dead_code, unused_imports)]

use crate::eternal_catalog_v1::{play_observed, Policy};
use mtg_kernel::card_def::{
    card_id_by_name, card_id_by_visible_name, mana_colors_mask, CardCapability, CardDef, CardType,
    Keywords, Subtype, Supertype, TargetSpec, WardCostDef, CARD_DEFS,
};
use mtg_kernel::engine::{self, Action, CastMode, Decision, OptionalCostChoice};
use mtg_kernel::eternal_cards_v1::EternalStateV1;
use mtg_kernel::eternal_cube_c8a_v1::C8aStateV1;
use mtg_kernel::event::{propose_and_commit, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::limited_session_v1::{CustomCardCountV1, CustomDeckV1};
use mtg_kernel::mana::{ManaColor, Pip};
use mtg_kernel::planeswalker_v1::{change_loyalty, loyalty};
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Target, Zone};
use serde_json::Value;
use std::hash::{Hash, Hasher};

const P0: PlayerId = PlayerId::P0;
const P1: PlayerId = PlayerId::P1;

const C8A_CARDS: [&str; 6] = [
    "Bonecrusher Giant",
    "Concealing Curtains",
    "Jace, Vryn's Prodigy",
    "Jennifer Walters",
    "Emeritus of Abundance",
    "Emeritus of Ideation",
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
        0x4355_4245_0000 + 0x3861,
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

/// Puts a double-faced card onto the battlefield already transformed on face 1
/// (for Jace, Telepath Unbound this also initializes its 5 starting loyalty).
fn put_transformed(state: &mut GameState, player: PlayerId, name: &str) -> ObjectId {
    let object = put(state, player, name, Zone::Exile);
    propose_and_commit(
        state,
        ProposedEvent::transformed_battlefield_return(object, 1, player),
    );
    state.objects.get_mut(object).summoning_sick = false;
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

fn exile_copies(state: &GameState, name: &str) -> Vec<ObjectId> {
    state
        .exile
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

/// Casts the modal or Adventure `spell` choosing `mode` (chosen silently when
/// it is the only legal one), then its targets.
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

/// The targets `source`'s ability `index` may choose first.
fn ability_targets(state: &GameState, source: ObjectId, index: u8) -> Vec<Target> {
    let mut probe = state.clone();
    assert!(activatable(&mut probe, source, index));
    act(&mut probe, Action::ActivateAbility(source, index));
    match next(&mut probe) {
        Decision::ChooseTargets { legal_targets, .. }
        | Decision::ChooseEffectTargets { legal_targets, .. } => legal_targets,
        other => panic!("expected targets, got {other:?}"),
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

fn life(state: &GameState, player: PlayerId) -> i32 {
    state.players[player.index()].life
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

/// A custom deck for a public session.
fn deck(rows: &[(&str, u32)]) -> CustomDeckV1 {
    CustomDeckV1 {
        cards: rows
            .iter()
            .map(|&(name, count)| CustomCardCountV1 {
                name: name.to_owned(),
                count,
            })
            .collect(),
        companion: None,
    }
}

// ---- Characteristics and isolation ---------------------------------------------

#[test]
fn c8a_cards_are_full() {
    for name in C8A_CARDS.into_iter().chain(["Regrowth"]) {
        let card = def(name);
        assert_eq!(card.capability, CardCapability::Full, "{name}");
        assert!(card.is_executable(), "{name}");
        assert!(card.has_full_support(), "{name}");
    }
}

#[test]
fn c8a_printed_characteristics_and_faces() {
    let giant = def("Bonecrusher Giant");
    assert_eq!(giant.mana_value, 3);
    assert_eq!((giant.power, giant.toughness), (Some(4), Some(3)));
    assert_eq!(giant.subtypes, &[Subtype::Giant]);
    let stomp = giant.adventure.as_ref().expect("Stomp adventure");
    assert_eq!(stomp.name, "Stomp");
    assert_eq!(stomp.cost.generic, 1);
    assert_eq!(stomp.cost.pips, &[Pip::Colored(ManaColor::R)]);
    assert_eq!(stomp.types, &[CardType::Instant]);
    assert_eq!(stomp.target_spec, TargetSpec::AnyTarget);
    assert_eq!(
        card_id_by_visible_name("Stomp"),
        Some((id("Bonecrusher Giant"), 2))
    );

    let curtains = def("Concealing Curtains");
    assert_eq!(curtains.mana_value, 1);
    assert_eq!((curtains.power, curtains.toughness), (Some(0), Some(4)));
    assert!(curtains.keywords.has(Keywords::DEFENDER));
    assert_eq!(curtains.subtypes, &[Subtype::Wall]);
    let eye = curtains.transform_face.as_ref().expect("Revealing Eye");
    assert_eq!(eye.name, "Revealing Eye");
    assert_eq!((eye.power, eye.toughness), (Some(3), Some(4)));
    assert_eq!(eye.colors, &[ManaColor::B]);
    assert_eq!(eye.subtypes, &[Subtype::Eye, Subtype::Horror]);
    assert!(eye.keywords.has(Keywords::MENACE));
    assert_eq!(
        card_id_by_visible_name("Revealing Eye"),
        Some((id("Concealing Curtains"), 1))
    );

    let jace = def("Jace, Vryn's Prodigy");
    assert_eq!(jace.mana_value, 2);
    assert_eq!((jace.power, jace.toughness), (Some(0), Some(2)));
    assert_eq!(jace.supertypes, &[Supertype::Legendary]);
    assert_eq!(jace.starting_loyalty, Some(5));
    let telepath = jace
        .transform_face
        .as_ref()
        .expect("Jace, Telepath Unbound");
    assert_eq!(telepath.name, "Jace, Telepath Unbound");
    assert_eq!(telepath.types, &[CardType::Planeswalker]);
    assert_eq!(telepath.subtypes, &[Subtype::Jace]);
    assert_eq!(telepath.colors, &[ManaColor::U]);
    assert_eq!(
        card_id_by_visible_name("Jace, Telepath Unbound"),
        Some((id("Jace, Vryn's Prodigy"), 1))
    );

    let jennifer = def("Jennifer Walters");
    assert_eq!(jennifer.mana_value, 2);
    assert_eq!((jennifer.power, jennifer.toughness), (Some(2), Some(3)));
    assert_eq!(jennifer.supertypes, &[Supertype::Legendary]);
    assert_eq!(
        jennifer.subtypes,
        &[Subtype::Human, Subtype::Advisor, Subtype::Hero]
    );
    assert!(jennifer.alt_cost.is_some());
    let she_hulk = jennifer
        .transform_face
        .as_ref()
        .expect("The Sensational She-Hulk");
    assert_eq!(she_hulk.name, "The Sensational She-Hulk");
    assert_eq!((she_hulk.power, she_hulk.toughness), (Some(6), Some(6)));
    assert_eq!(she_hulk.colors, &[ManaColor::G, ManaColor::W]);
    assert_eq!(she_hulk.subtypes, &[Subtype::Gamma, Subtype::Hero]);
    assert!(she_hulk.keywords.has(Keywords::REACH));
    assert!(she_hulk.keywords.has(Keywords::TRAMPLE));
    assert_eq!(
        card_id_by_visible_name("The Sensational She-Hulk"),
        Some((id("Jennifer Walters"), 1))
    );

    let abundance = def("Emeritus of Abundance");
    assert_eq!(abundance.mana_value, 3);
    assert_eq!((abundance.power, abundance.toughness), (Some(3), Some(4)));
    assert!(abundance.keywords.has(Keywords::VIGILANCE));
    assert_eq!(abundance.subtypes, &[Subtype::Elf, Subtype::Druid]);
    let regrowth = def("Regrowth");
    assert_eq!(regrowth.mana_value, 2);
    assert_eq!(regrowth.types, &[CardType::Sorcery]);

    let ideation = def("Emeritus of Ideation");
    assert_eq!(ideation.mana_value, 5);
    assert_eq!((ideation.power, ideation.toughness), (Some(5), Some(5)));
    assert!(ideation.keywords.has(Keywords::FLYING));
    assert_eq!(ideation.ward_cost, Some(WardCostDef::Generic(2)));
    assert_eq!(ideation.subtypes, &[Subtype::Human, Subtype::Wizard]);
}

#[test]
fn empty_c8a_state_hashes_zero_bytes_and_skips_serde() {
    #[derive(Default)]
    struct ByteCounter(usize);
    impl Hasher for ByteCounter {
        fn finish(&self) -> u64 {
            self.0 as u64
        }
        fn write(&mut self, bytes: &[u8]) {
            self.0 += bytes.len();
        }
    }

    let empty = C8aStateV1::default();
    let mut counter = ByteCounter::default();
    empty.hash(&mut counter);
    assert_eq!(counter.0, 0, "empty C8aStateV1 must hash zero bytes");
    assert_eq!(serde_json::to_string(&empty).unwrap(), "{}");
    let eternal_json = serde_json::to_string(&EternalStateV1::default()).unwrap();
    assert!(
        !eternal_json.contains("\"c8a\""),
        "empty c8a field must be skipped in EternalStateV1 serde: {eternal_json}"
    );
}

// ---- Bonecrusher Giant // Stomp ------------------------------------------------

#[test]
fn stomp_deals_two_disables_fog_exiles_on_adventure_and_casts_giant_from_exile() {
    let mut state = plains();
    let swiftspear = put(&mut state, P0, "Monastery Swiftspear", Zone::Battlefield);
    let giant = put(&mut state, P0, "Bonecrusher Giant", Zone::Hand);
    let fog = put(&mut state, P1, "Fog", Zone::Hand);
    put(&mut state, P1, "Forest", Zone::Battlefield);

    // Cast Stomp ({1}{R}) targeting P1.
    pool(&mut state, P0, &[ManaColor::R], 1);
    cast_mode(&mut state, giant, 1, &[Target::Player(P1)]);
    settled(&mut state);
    assert_eq!(life(&state, P1), 18, "Stomp deals 2 damage to P1");
    assert_eq!(zone(&state, giant), Zone::Exile);
    assert!(state.objects.get(giant).v4.on_adventure);

    // Combat: P1 casts Fog, but Stomp disabled damage prevention this turn.
    // Swiftspear got +1/+1 from prowess (2/3) so it deals 2 combat damage.
    let mut fogged = false;
    attack_with(&mut state, vec![swiftspear], |_, decision| match decision {
        Decision::CastSpellOrPass {
            player: P1,
            castable_spells,
            ..
        } if !fogged && castable_spells.contains(&fog) => {
            fogged = true;
            Some(Action::CastSpell(fog))
        }
        _ => None,
    });
    assert!(fogged, "P1 cast Fog during combat");
    assert_eq!(
        life(&state, P1),
        16,
        "Fog did not prevent Swiftspear's 2 combat damage after Stomp"
    );

    // Cast Bonecrusher Giant from exile on an adventure in Main2.
    pool(&mut state, P0, &[ManaColor::R], 2);
    cast(&mut state, giant, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, giant), Zone::Battlefield);
    assert!(!state.objects.get(giant).v4.on_adventure);
    assert_eq!(power_toughness(&state, giant), (4, 3));
}

#[test]
fn stomp_fizzle_goes_to_graveyard_and_does_not_disable_prevention() {
    let mut state = plains();
    let giant = put(&mut state, P0, "Bonecrusher Giant", Zone::Hand);
    let attacker = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    let victim = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    let swords = put(&mut state, P1, "Swords to Plowshares", Zone::Hand);
    let fog = put(&mut state, P1, "Fog", Zone::Hand);
    put(&mut state, P1, "Forest", Zone::Battlefield);

    pool(&mut state, P0, &[ManaColor::R], 1);
    cast_mode(&mut state, giant, 1, &[Target::Object(victim)]);
    // P0 passes priority; P1 exiles Stomp's target with Swords.
    assert!(matches!(
        next(&mut state),
        Decision::CastSpellOrPass { player: P0, .. }
    ));
    act(&mut state, Action::Pass);
    pool(&mut state, P1, &[ManaColor::W], 0);
    cast(&mut state, swords, &[Target::Object(victim)]);
    settled(&mut state);

    assert_eq!(zone(&state, victim), Zone::Exile);
    assert_eq!(
        zone(&state, giant),
        Zone::Graveyard,
        "fizzled Stomp goes to graveyard, not exile on an adventure"
    );
    assert!(!state.objects.get(giant).v4.on_adventure);

    // Fog still prevents combat damage because Stomp fizzled.
    let life_before = life(&state, P1);
    let mut fogged = false;
    attack_with(&mut state, vec![attacker], |_, decision| match decision {
        Decision::CastSpellOrPass {
            player: P1,
            castable_spells,
            ..
        } if !fogged && castable_spells.contains(&fog) => {
            fogged = true;
            Some(Action::CastSpell(fog))
        }
        _ => None,
    });
    assert!(fogged);
    assert_eq!(life(&state, P1), life_before);
}

#[test]
fn bonecrusher_giant_trigger_fires_for_any_spell_not_abilities_and_survives_departures() {
    // 1. Trigger resolves BEFORE targeting spell, killing a 2-life opponent before Bolt resolves.
    let mut state = plains();
    let giant = put(&mut state, P0, "Bonecrusher Giant", Zone::Battlefield);
    state.players[P1.index()].life = 2;
    opponent_bolts(&mut state, Target::Object(giant));
    act(&mut state, Action::Pass);
    assert_eq!(next(&mut state), Decision::GameOver { winner: Some(P0) });
    assert_eq!(life(&state, P1), 0);
    assert_eq!(zone(&state, giant), Zone::Battlefield);

    // 2. Fires on own spells (damages own controller) and multiple times per turn.
    let mut state = plains();
    let giant = put(&mut state, P0, "Bonecrusher Giant", Zone::Battlefield);
    let safe = put(&mut state, P0, "Tamiyo's Safekeeping", Zone::Hand);
    let shock = put(&mut state, P0, "Shock", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G, ManaColor::R], 0);
    cast(&mut state, safe, &[Target::Object(giant)]);
    settled(&mut state);
    assert_eq!(
        life(&state, P0),
        20,
        "-2 from Giant trigger, +2 from Safekeeping"
    );
    cast(&mut state, shock, &[Target::Object(giant)]);
    settled(&mut state);
    assert_eq!(
        life(&state, P0),
        18,
        "triggers again on second own spell in the same turn (indestructible Giant survives)"
    );
    assert_eq!(zone(&state, giant), Zone::Battlefield);

    // 3. Does NOT fire on activated abilities (Goblin Bombardment).
    let mut state = plains();
    let giant = put(&mut state, P0, "Bonecrusher Giant", Zone::Battlefield);
    let bombardment = put(&mut state, P1, "Goblin Bombardment", Zone::Battlefield);
    put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    state.priority_player = P1;
    activate(&mut state, bombardment, 0, &[Target::Object(giant)]);
    settle_first(&mut state);
    assert_eq!(life(&state, P1), 20, "abilities do not trigger Bonecrusher");
    assert_eq!(state.objects.get(giant).damage, 1);

    // 4. Survives targeting spell being countered AND Bonecrusher leaving battlefield,
    //    and bypasses player shroud (True Believer) because it does not target.
    let mut state = plains();
    let giant = put(&mut state, P0, "Bonecrusher Giant", Zone::Battlefield);
    put(&mut state, P1, "True Believer", Zone::Battlefield);
    let bolt = opponent_bolts(&mut state, Target::Object(giant));
    let counter = put(&mut state, P0, "Counterspell", Zone::Hand);
    let swords = put(&mut state, P0, "Swords to Plowshares", Zone::Hand);
    pool(
        &mut state,
        P0,
        &[ManaColor::U, ManaColor::U, ManaColor::W],
        0,
    );
    cast(&mut state, counter, &[Target::Object(bolt)]);
    assert!(matches!(
        next(&mut state),
        Decision::CastSpellOrPass { player: P0, .. }
    ));
    cast(&mut state, swords, &[Target::Object(giant)]);
    settled(&mut state);
    assert_eq!(zone(&state, giant), Zone::Exile);
    assert_eq!(zone(&state, bolt), Zone::Graveyard);
    assert_eq!(
        life(&state, P1),
        18,
        "P1 took 2 despite Bolt countered, Giant exiled, and True Believer shroud"
    );
    assert_eq!(
        life(&state, P0),
        22,
        "P0 took 2 from own Swords targeting Giant, then gained 4 from Swords"
    );

    // 5. Does NOT trigger while Bonecrusher Giant is a spell on the stack.
    let mut state = plains();
    let giant = put(&mut state, P0, "Bonecrusher Giant", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 2);
    cast_mode(&mut state, giant, 0, &[]);
    assert!(matches!(
        next(&mut state),
        Decision::CastSpellOrPass { player: P0, .. }
    ));
    act(&mut state, Action::Pass);
    let counter = put(&mut state, P1, "Counterspell", Zone::Hand);
    pool(&mut state, P1, &[ManaColor::U, ManaColor::U], 0);
    cast(&mut state, counter, &[Target::Object(giant)]);
    settled(&mut state);
    assert_eq!(zone(&state, giant), Zone::Graveyard);
    assert_eq!(life(&state, P1), 20);
}

// ---- Concealing Curtains // Revealing Eye --------------------------------------

#[test]
fn concealing_curtains_defender_sorcery_transform_and_revealing_eye_discard_draw() {
    let mut state = plains();
    let curtains = put(&mut state, P0, "Concealing Curtains", Zone::Battlefield);
    let land = put(&mut state, P1, "Plains", Zone::Hand);
    let bolt = put(&mut state, P1, "Lightning Bolt", Zone::Hand);
    let counter = put(&mut state, P1, "Counterspell", Zone::Hand);

    assert_eq!(power_toughness(&state, curtains), (0, 4));
    assert!(keyword(&state, curtains, Keywords::DEFENDER));

    // Sorcery-speed only: not activatable on opponent's turn.
    let mut opp_turn = state.clone();
    opp_turn.active_player = P1;
    opp_turn.priority_player = P0;
    pool(&mut opp_turn, P0, &[ManaColor::B], 2);
    assert!(!activatable(&mut opp_turn, curtains, 0));

    // Activate {2}{B} on P0's Main1.
    pool(&mut state, P0, &[ManaColor::B], 2);
    activate(&mut state, curtains, 0, &[]);
    // Transform resolves, queuing Revealing Eye's trigger targeting P1.
    let p1_lib_before = state.players[P1.index()].library.len();
    let mut saw_choice = false;
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseTargets { legal_targets, .. } => {
            assert_eq!(legal_targets, vec![Target::Player(P1)]);
            Action::ChooseTarget(Target::Player(P1))
        }
        Decision::ChooseEffectTargets {
            player,
            min_targets,
            max_targets,
            can_finish,
            legal_targets,
            ..
        } => {
            saw_choice = true;
            assert_eq!(player, P0);
            assert_eq!((min_targets, max_targets, can_finish), (0, 1, true));
            assert!(legal_targets.contains(&Target::Object(bolt)));
            assert!(legal_targets.contains(&Target::Object(counter)));
            assert!(
                !legal_targets.contains(&Target::Object(land)),
                "lands are excluded from Revealing Eye's nonland choice"
            );
            Action::ChooseEffectTarget(Target::Object(bolt))
        }
        other => first_answer(other),
    });

    assert!(saw_choice);
    assert_eq!(state.objects.get(curtains).v4.face_index, 1);
    assert_eq!(power_toughness(&state, curtains), (3, 4));
    assert!(keyword(&state, curtains, Keywords::MENACE));
    assert!(!keyword(&state, curtains, Keywords::DEFENDER));
    pool(&mut state, P0, &[ManaColor::B], 2);
    assert!(
        !activatable(&mut state, curtains, 0),
        "front-face transform ability is not on Revealing Eye"
    );
    assert_eq!(zone(&state, bolt), Zone::Graveyard, "chosen card discarded");
    assert_eq!(zone(&state, counter), Zone::Hand);
    assert_eq!(zone(&state, land), Zone::Hand);
    assert_eq!(
        state.players[P1.index()].library.len(),
        p1_lib_before - 1,
        "opponent drew 1 card after discarding"
    );

    // Back face has Menace: 1 blocker is rejected, 2 blockers are accepted.
    let b1 = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    let b2 = put(&mut state, P1, "Elvish Mystic", Zone::Battlefield);
    pass_until(&mut state, |s| s.step == Step::DeclareAttackers);
    act(&mut state, Action::DeclareAttackers(vec![curtains]));
    pass_until(&mut state, |s| s.step == Step::DeclareBlockers);
    assert!(engine::step(&mut state, Action::DeclareBlockers(vec![(b1, curtains)])).is_err());
    act(
        &mut state,
        Action::DeclareBlockers(vec![(b1, curtains), (b2, curtains)]),
    );
}

#[test]
fn revealing_eye_decline_all_lands_shroud_fizzle_and_hullbreacher_hook() {
    // 1. Declining the optional nonland choice causes neither discard nor draw.
    let mut state = plains();
    let curtains = put(&mut state, P0, "Concealing Curtains", Zone::Battlefield);
    let bolt = put(&mut state, P1, "Lightning Bolt", Zone::Hand);
    let lib = state.players[P1.index()].library.len();
    pool(&mut state, P0, &[ManaColor::B], 2);
    activate(&mut state, curtains, 0, &[]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseTargets { .. } => Action::ChooseTarget(Target::Player(P1)),
        Decision::ChooseEffectTargets {
            can_finish: true, ..
        } => Action::FinishEffectSelection,
        other => first_answer(other),
    });
    assert_eq!(zone(&state, bolt), Zone::Hand);
    assert_eq!(state.players[P1.index()].library.len(), lib);

    // 2. Opponent hand with only lands: no choice prompt, no discard, no draw.
    let mut state = plains();
    let curtains = put(&mut state, P0, "Concealing Curtains", Zone::Battlefield);
    put(&mut state, P1, "Plains", Zone::Hand);
    put(&mut state, P1, "Island", Zone::Hand);
    let lib = state.players[P1.index()].library.len();
    pool(&mut state, P0, &[ManaColor::B], 2);
    activate(&mut state, curtains, 0, &[]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseTargets { .. } => Action::ChooseTarget(Target::Player(P1)),
        Decision::ChooseEffectTargets { .. } => panic!("all-land hand must not prompt"),
        other => first_answer(other),
    });
    assert_eq!(state.players[P1.index()].library.len(), lib);
    assert_eq!(state.players[P1.index()].hand.len(), 2);

    // 3. Target opponent gains shroud before trigger resolves: fizzles cleanly.
    let mut state = plains();
    let curtains = put(&mut state, P0, "Concealing Curtains", Zone::Battlefield);
    let bolt = put(&mut state, P1, "Lightning Bolt", Zone::Hand);
    let lib = state.players[P1.index()].library.len();
    pool(&mut state, P0, &[ManaColor::B], 2);
    activate(&mut state, curtains, 0, &[]);
    // Resolve transform ability and target P1 with the Revealing Eye trigger.
    assert!(matches!(
        next(&mut state),
        Decision::CastSpellOrPass { player: P0, .. }
    ));
    act(&mut state, Action::Pass);
    assert!(matches!(
        next(&mut state),
        Decision::CastSpellOrPass { player: P1, .. }
    ));
    act(&mut state, Action::Pass);
    choose_target(&mut state, Target::Player(P1));
    assert!(matches!(next(&mut state), Decision::CastSpellOrPass { .. }));
    // Now P1 gains shroud while Revealing Eye's trigger is on the stack.
    put(&mut state, P1, "True Believer", Zone::Battlefield);
    settled(&mut state);
    assert_eq!(zone(&state, bolt), Zone::Hand);
    assert_eq!(state.players[P1.index()].library.len(), lib);

    // 4. Resumable draw integrates with Hullbreacher: P1 discards Bolt, P0 gets a Treasure,
    //    and P1 does not draw.
    let mut state = plains();
    let curtains = put(&mut state, P0, "Concealing Curtains", Zone::Battlefield);
    put(&mut state, P0, "Hullbreacher", Zone::Battlefield);
    let bolt = put(&mut state, P1, "Lightning Bolt", Zone::Hand);
    let lib = state.players[P1.index()].library.len();
    pool(&mut state, P0, &[ManaColor::B], 2);
    activate(&mut state, curtains, 0, &[]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseTargets { .. } => Action::ChooseTarget(Target::Player(P1)),
        Decision::ChooseEffectTargets { legal_targets, .. }
            if legal_targets.contains(&Target::Object(bolt)) =>
        {
            Action::ChooseEffectTarget(Target::Object(bolt))
        }
        other => first_answer(other),
    });
    assert_eq!(zone(&state, bolt), Zone::Graveyard);
    assert_eq!(
        state.players[P1.index()].library.len(),
        lib,
        "Hullbreacher replaced P1's draw"
    );
    assert_eq!(named(&state, P0, "Treasure Token").len(), 1);
}

// ---- Jace, Vryn's Prodigy // Jace, Telepath Unbound ----------------------------

#[test]
fn jace_vryns_prodigy_loot_transform_threshold_owner_control_departure_and_dredge() {
    // 1. Below 5 cards in graveyard: stays face 0. At 5+ cards: exiles and returns transformed
    //    as Jace, Telepath Unbound with 5 loyalty, immediately activatable that turn.
    let mut state = plains();
    let jace = put(&mut state, P0, "Jace, Vryn's Prodigy", Zone::Battlefield);
    for _ in 0..3 {
        put(&mut state, P0, "Island", Zone::Graveyard);
    }
    activate(&mut state, jace, 0, &[]);
    settle_first(&mut state);
    assert_eq!(state.objects.get(jace).v4.face_index, 0);
    assert_eq!(state.players[P0.index()].graveyard.len(), 4);
    assert_eq!(loyalty(&state, jace), None);

    // Next turn untap Jace and activate again -> 5th card in graveyard -> transforms!
    pass_until(&mut state, |s| {
        s.active_player == P1 && s.step == Step::Main1
    });
    pass_until(&mut state, |s| {
        s.active_player == P0 && s.step == Step::Main1
    });
    activate(&mut state, jace, 0, &[]);
    settle_first(&mut state);
    assert_eq!(zone(&state, jace), Zone::Battlefield);
    assert_eq!(state.objects.get(jace).v4.face_index, 1);
    assert_eq!(loyalty(&state, jace), Some(5));
    assert!(
        !activatable(&mut state, jace, 0),
        "creature tap ability is unavailable on planeswalker face"
    );
    assert!(
        activatable(&mut state, jace, 1),
        "new planeswalker object can activate +1 on the same turn"
    );

    // 2. Owner's control: P1 owns Jace, P0 controls him -> returns under P1's control.
    let mut state = plains();
    let jace = put(&mut state, P1, "Jace, Vryn's Prodigy", Zone::Battlefield);
    state.players[P1.index()]
        .battlefield
        .retain(|&id| id != jace);
    state.players[P0.index()].battlefield.push(jace);
    state.objects.get_mut(jace).controller = P0;
    for _ in 0..4 {
        put(&mut state, P0, "Island", Zone::Graveyard);
    }
    activate(&mut state, jace, 0, &[]);
    settle_first(&mut state);
    assert_eq!(state.objects.get(jace).v4.face_index, 1);
    assert_eq!(
        state.objects.get(jace).controller,
        P1,
        "returns transformed under his owner's control"
    );
    assert_eq!(loyalty(&state, jace), Some(5));

    // 3. Jace leaves battlefield before ability resolves: still loots, does NOT return.
    let mut state = plains();
    let jace = put(&mut state, P0, "Jace, Vryn's Prodigy", Zone::Battlefield);
    for _ in 0..4 {
        put(&mut state, P0, "Island", Zone::Graveyard);
    }
    let lib = state.players[P0.index()].library.len();
    activate(&mut state, jace, 0, &[]);
    assert!(matches!(
        next(&mut state),
        Decision::CastSpellOrPass { player: P0, .. }
    ));
    act(&mut state, Action::Pass);
    let bolt = put(&mut state, P1, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P1, &[ManaColor::R], 0);
    cast(&mut state, bolt, &[Target::Object(jace)]);
    settle_first(&mut state);
    assert_eq!(zone(&state, jace), Zone::Graveyard);
    assert_eq!(
        state.players[P0.index()].library.len(),
        lib - 1,
        "P0 still drew and discarded even though Jace died"
    );

    // 4. Dredge replaces Jace's draw, filling the graveyard to 5+ and transforming Jace.
    let mut state = plains();
    let jace = put(&mut state, P0, "Jace, Vryn's Prodigy", Zone::Battlefield);
    let imp = put(&mut state, P0, "Stinkweed Imp", Zone::Graveyard);
    let lib = state.players[P0.index()].library.len();
    activate(&mut state, jace, 0, &[]);
    settle_choosing(&mut state, imp);
    assert_eq!(state.players[P0.index()].library.len(), lib - 5);
    assert_eq!(state.objects.get(jace).v4.face_index, 1);
    assert_eq!(loyalty(&state, jace), Some(5));
}

#[test]
fn jace_telepath_unbound_plus_one_lasts_until_next_turn_and_supports_zero_targets() {
    // 1. Choosing 0 targets is legal and still adds +1 loyalty.
    let mut state = plains();
    let jace = put_transformed(&mut state, P0, "Jace, Vryn's Prodigy");
    put(&mut state, P1, "Bonecrusher Giant", Zone::Battlefield);
    act(&mut state, Action::ActivateAbility(jace, 1));
    assert!(matches!(
        next(&mut state),
        Decision::ChooseEffectTargets {
            can_finish: true,
            min_targets: 0,
            max_targets: 1,
            ..
        }
    ));
    act(&mut state, Action::FinishEffectSelection);
    settled(&mut state);
    assert_eq!(loyalty(&state, jace), Some(6));

    // 2. Weakens target creature by -2/-0 through opponent's turn, expiring on P0's next turn.
    let mut state = plains();
    let jace = put_transformed(&mut state, P0, "Jace, Vryn's Prodigy");
    let giant = put(&mut state, P1, "Bonecrusher Giant", Zone::Battlefield);
    activate(&mut state, jace, 1, &[Target::Object(giant)]);
    settled(&mut state);
    assert_eq!(loyalty(&state, jace), Some(6));
    assert_eq!(power_toughness(&state, giant), (2, 3));

    pass_until(&mut state, |s| {
        s.active_player == P1 && s.step == Step::Main1
    });
    assert_eq!(
        power_toughness(&state, giant),
        (2, 3),
        "-2/-0 persists through opponent's turn"
    );

    pass_until(&mut state, |s| {
        s.active_player == P0 && s.step == Step::Main1
    });
    assert_eq!(
        power_toughness(&state, giant),
        (4, 3),
        "-2/-0 expires at the start of controller's next turn"
    );

    // 3. Target legality rechecked on resolution (hexproof fizzles the debuff).
    let mut state = plains();
    let jace = put_transformed(&mut state, P0, "Jace, Vryn's Prodigy");
    let giant = put(&mut state, P1, "Bonecrusher Giant", Zone::Battlefield);
    let safe = put(&mut state, P1, "Tamiyo's Safekeeping", Zone::Hand);
    activate(&mut state, jace, 1, &[Target::Object(giant)]);
    assert!(matches!(
        next(&mut state),
        Decision::CastSpellOrPass { player: P0, .. }
    ));
    act(&mut state, Action::Pass);
    pool(&mut state, P1, &[ManaColor::G], 0);
    cast(&mut state, safe, &[Target::Object(giant)]);
    settled(&mut state);
    assert_eq!(
        power_toughness(&state, giant),
        (4, 3),
        "hexproof made +1 target illegal on resolution"
    );
}

#[test]
fn jace_telepath_unbound_minus_three_casts_normal_or_alt_cost_and_exiles_instead_of_graveyard() {
    // 1. Targets only instant or sorcery cards in controller's graveyard, NOT creatures,
    //    adventurers, lands, or opponent's graveyard.
    let mut state = plains();
    let jace = put_transformed(&mut state, P0, "Jace, Vryn's Prodigy");
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Graveyard);
    let ponder = put(&mut state, P0, "Ponder", Zone::Graveyard);
    let giant = put(&mut state, P0, "Bonecrusher Giant", Zone::Graveyard);
    let land = put(&mut state, P0, "Island", Zone::Graveyard);
    let opp_opt = put(&mut state, P1, "Opt", Zone::Graveyard);
    let legal = ability_targets(&state, jace, 2);
    assert!(legal.contains(&Target::Object(bolt)));
    assert!(legal.contains(&Target::Object(ponder)));
    assert!(!legal.contains(&Target::Object(giant)));
    assert!(!legal.contains(&Target::Object(land)));
    assert!(!legal.contains(&Target::Object(opp_opt)));

    // Cast Lightning Bolt for normal cost -> resolves -> exiled instead of graveyard.
    activate(&mut state, jace, 2, &[Target::Object(bolt)]);
    settled(&mut state);
    assert_eq!(loyalty(&state, jace), Some(2));
    pool(&mut state, P0, &[ManaColor::R], 0);
    cast(&mut state, bolt, &[Target::Player(P1)]);
    settled(&mut state);
    assert_eq!(life(&state, P1), 17);
    assert_eq!(zone(&state, bolt), Zone::Exile);

    // 2. Countered spell cast via Jace -3 is ALSO exiled instead of going to graveyard.
    let mut state = plains();
    let jace = put_transformed(&mut state, P0, "Jace, Vryn's Prodigy");
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Graveyard);
    activate(&mut state, jace, 2, &[Target::Object(bolt)]);
    settled(&mut state);
    pool(&mut state, P0, &[ManaColor::R], 0);
    cast(&mut state, bolt, &[Target::Player(P1)]);
    assert!(matches!(
        next(&mut state),
        Decision::CastSpellOrPass { player: P0, .. }
    ));
    act(&mut state, Action::Pass);
    let counter = put(&mut state, P1, "Counterspell", Zone::Hand);
    pool(&mut state, P1, &[ManaColor::U, ManaColor::U], 0);
    cast(&mut state, counter, &[Target::Object(bolt)]);
    settled(&mut state);
    assert_eq!(life(&state, P1), 20);
    assert_eq!(
        zone(&state, bolt),
        Zone::Exile,
        "countered Jace -3 spell is exiled instead of put into graveyard"
    );

    // 3. Remand returns the spell to hand (NOT exiled); recasting from hand puts it in graveyard.
    let mut state = plains();
    let jace = put_transformed(&mut state, P0, "Jace, Vryn's Prodigy");
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Graveyard);
    activate(&mut state, jace, 2, &[Target::Object(bolt)]);
    settled(&mut state);
    pool(&mut state, P0, &[ManaColor::R], 0);
    cast(&mut state, bolt, &[Target::Player(P1)]);
    assert!(matches!(
        next(&mut state),
        Decision::CastSpellOrPass { player: P0, .. }
    ));
    act(&mut state, Action::Pass);
    let remand = put(&mut state, P1, "Remand", Zone::Hand);
    pool(&mut state, P1, &[ManaColor::U], 1);
    cast(&mut state, remand, &[Target::Object(bolt)]);
    settled(&mut state);
    assert_eq!(zone(&state, bolt), Zone::Hand, "Remand returns to hand");
    pool(&mut state, P0, &[ManaColor::R], 0);
    cast(&mut state, bolt, &[Target::Player(P1)]);
    settled(&mut state);
    assert_eq!(
        zone(&state, bolt),
        Zone::Graveyard,
        "recasting from hand goes to graveyard normally"
    );

    // 4. Alternative cost (Force of Will): playable without mana by pitching a blue card
    //    (proving Jace -3 does NOT force Flashback), and still exiles on departure.
    let mut state = plains();
    let jace = put_transformed(&mut state, P0, "Jace, Vryn's Prodigy");
    let force = put(&mut state, P0, "Force of Will", Zone::Graveyard);
    let pitch = put(&mut state, P0, "Brainstorm", Zone::Hand);
    activate(&mut state, jace, 2, &[Target::Object(force)]);
    settled(&mut state);
    let opp_bolt = opponent_bolts(&mut state, Target::Player(P0));
    cast(&mut state, force, &[Target::Object(opp_bolt)]);
    settled(&mut state);
    assert_eq!(zone(&state, pitch), Zone::Exile);
    assert_eq!(zone(&state, opp_bolt), Zone::Graveyard);
    assert_eq!(
        life(&state, P0),
        19,
        "1 life for Force of Will, Bolt countered"
    );
    assert_eq!(zone(&state, force), Zone::Exile);

    // 5. Permission expires after this turn.
    let mut state = plains();
    let jace = put_transformed(&mut state, P0, "Jace, Vryn's Prodigy");
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Graveyard);
    activate(&mut state, jace, 2, &[Target::Object(bolt)]);
    settled(&mut state);
    pass_until(&mut state, |s| {
        s.active_player == P1 && s.step == Step::Main1
    });
    state.priority_player = P0;
    pool(&mut state, P0, &[ManaColor::R], 0);
    assert!(!castable(&mut state, bolt));
}

#[test]
fn jace_telepath_unbound_minus_nine_emblem_persists_after_death_and_mills_five() {
    let mut state = plains();
    let jace = put_transformed(&mut state, P0, "Jace, Vryn's Prodigy");
    change_loyalty(&mut state, jace, 4);
    assert_eq!(loyalty(&state, jace), Some(9));

    activate(&mut state, jace, 3, &[]);
    settled(&mut state);
    assert_eq!(
        zone(&state, jace),
        Zone::Graveyard,
        "Jace dies to 0 loyalty SBA"
    );

    // Emblem triggers on P0's spells and mills P1 for 5.
    let p1_lib = state.players[P1.index()].library.len();
    let opt = put(&mut state, P0, "Opt", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 0);
    cast(&mut state, opt, &[]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseTargets { legal_targets, .. } => {
            assert_eq!(legal_targets, vec![Target::Player(P1)]);
            Action::ChooseTarget(Target::Player(P1))
        }
        other => first_answer(other),
    });
    assert_eq!(state.players[P1.index()].library.len(), p1_lib - 5);

    // Opponent's spells do NOT trigger P0's emblem.
    let p0_lib = state.players[P0.index()].library.len();
    let p1_lib = state.players[P1.index()].library.len();
    let opp_bolt = opponent_bolts(&mut state, Target::Player(P0));
    settled(&mut state);
    assert_eq!(zone(&state, opp_bolt), Zone::Graveyard);
    assert_eq!(state.players[P0.index()].library.len(), p0_lib);
    assert_eq!(state.players[P1.index()].library.len(), p1_lib);

    // Opponent with shroud (True Believer): emblem trigger cannot target P1 and does not mill.
    put(&mut state, P1, "True Believer", Zone::Battlefield);
    let opt2 = put(&mut state, P0, "Opt", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 0);
    cast(&mut state, opt2, &[]);
    settle_first(&mut state);
    assert_eq!(state.players[P1.index()].library.len(), p1_lib);
}

#[test]
fn non_dfc_clones_and_tokens_cannot_transform_jace_or_curtains() {
    // 1. CR 712.14a: Phantasmal Image copying Jace, Vryn's Prodigy with 5+
    //    cards in graveyard exiles itself, unwinds to Phantasmal Image, and
    //    STAYS in exile (does not return with face_index = 1).
    let mut state = plains();
    let _opp_jace = put(&mut state, P1, "Jace, Vryn's Prodigy", Zone::Battlefield);
    for _ in 0..4 {
        put(&mut state, P0, "Plains", Zone::Graveyard);
    }
    let img = put(&mut state, P0, "Phantasmal Image", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 1);
    cast(&mut state, img, &[]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseEffectOption { .. } => Action::ChooseEffectOption(1),
        other => first_answer(other),
    });
    assert_eq!(zone(&state, img), Zone::Battlefield);
    assert_eq!(name_of(&state, img), "Jace, Vryn's Prodigy");
    state.objects.get_mut(img).summoning_sick = false;
    activate(&mut state, img, 0, &[]);
    settle_first(&mut state);
    assert_eq!(
        zone(&state, img),
        Zone::Exile,
        "single-faced clone of Jace must stay exiled per CR 712.14a"
    );
    assert_eq!(name_of(&state, img), "Phantasmal Image");
    assert_eq!(state.objects.get(img).v4.face_index, 0);

    // 2. CR 701.28a / 712.9: Phantasmal Image copying Concealing Curtains
    //    resolves the {2}{B} transform ability as a no-op (face_index stays 0).
    let mut state = plains();
    let _opp_curtains = put(&mut state, P1, "Concealing Curtains", Zone::Battlefield);
    let img = put(&mut state, P0, "Phantasmal Image", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 1);
    cast(&mut state, img, &[]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseEffectOption { .. } => Action::ChooseEffectOption(1),
        other => first_answer(other),
    });
    assert_eq!(name_of(&state, img), "Concealing Curtains");
    pool(&mut state, P0, &[ManaColor::B], 2);
    activate(&mut state, img, 0, &[]);
    settled(&mut state);
    assert_eq!(
        state.objects.get(img).v4.face_index,
        0,
        "non-DFC clone cannot transform in place"
    );

    // 3. Token copy of Concealing Curtains cannot transform in place.
    let mut state = plains();
    let tok = put(&mut state, P0, "Concealing Curtains", Zone::Battlefield);
    state.objects.get_mut(tok).v4.is_token = true;
    pool(&mut state, P0, &[ManaColor::B], 2);
    activate(&mut state, tok, 0, &[]);
    settled(&mut state);
    assert_eq!(
        state.objects.get(tok).v4.face_index,
        0,
        "token copy cannot transform in place"
    );
}

// ---- Jennifer Walters // The Sensational She-Hulk ------------------------------

#[test]
fn jennifer_walters_blocks_opponent_spells_on_both_faces_and_transforms_sorcery_speed() {
    let mut state = plains();
    let jw = put(&mut state, P0, "Jennifer Walters", Zone::Battlefield);
    let bolt = put(&mut state, P1, "Lightning Bolt", Zone::Hand);

    // Front face blocks P1 from casting spells on P0's turn.
    state.priority_player = P1;
    pool(&mut state, P1, &[ManaColor::R], 0);
    assert!(!castable(&mut state, bolt));

    // Dress Down suppresses the static ability.
    let mut suppressed = state.clone();
    let dress = put(&mut suppressed, P1, "Dress Down", Zone::Hand);
    propose_and_commit(
        &mut suppressed,
        ProposedEvent::zone_change(dress, Zone::Battlefield),
    );
    settled(&mut suppressed);
    suppressed.priority_player = P1;
    assert!(castable(&mut suppressed, bolt));

    // P1 CAN cast spells on P1's own turn.
    let mut opp_turn = state.clone();
    opp_turn.active_player = P1;
    opp_turn.priority_player = P1;
    assert!(castable(&mut opp_turn, bolt));

    // Sorcery-speed transform is not activatable on P1's turn.
    opp_turn.priority_player = P0;
    pool(
        &mut opp_turn,
        P0,
        &[ManaColor::G, ManaColor::W, ManaColor::W],
        3,
    );
    assert!(!activatable(&mut opp_turn, jw, 0));

    // Transform on P0's Main1 into The Sensational She-Hulk.
    state.priority_player = P0;
    pool(
        &mut state,
        P0,
        &[ManaColor::G, ManaColor::W, ManaColor::W],
        3,
    );
    activate(&mut state, jw, 0, &[]);
    settled(&mut state);
    assert_eq!(state.objects.get(jw).v4.face_index, 1);
    assert_eq!(power_toughness(&state, jw), (6, 6));
    assert!(keyword(&state, jw, Keywords::REACH));
    assert!(keyword(&state, jw, Keywords::TRAMPLE));
    assert_eq!(engine::object_mana_value(&state, jw), 6);
    assert_eq!(
        engine::object_color_mask(&state, jw),
        mana_colors_mask(&[ManaColor::G, ManaColor::W])
    );
    pool(
        &mut state,
        P0,
        &[ManaColor::G, ManaColor::W, ManaColor::W],
        3,
    );
    assert!(!activatable(&mut state, jw, 0));

    // Back face STILL blocks P1 from casting spells on P0's turn.
    state.priority_player = P1;
    assert!(!castable(&mut state, bolt));

    // Back face has Reach and can block a Flying attacker.
    let mut combat = plains();
    let she_hulk = put_transformed(&mut combat, P1, "Jennifer Walters");
    let flyer = put(&mut combat, P0, "Emeritus of Ideation", Zone::Battlefield);
    pass_until(&mut combat, |s| s.step == Step::DeclareAttackers);
    act(&mut combat, Action::DeclareAttackers(vec![flyer]));
    pass_until(&mut combat, |s| s.step == Step::DeclareBlockers);
    act(
        &mut combat,
        Action::DeclareBlockers(vec![(she_hulk, flyer)]),
    );
}

#[test]
fn jennifer_walters_modal_back_face_cast_and_lurrus_restriction() {
    // Direct modal DFC back-face cast via CastMode::Alternative ({3}{G}{W}{W}).
    let mut state = plains();
    let jw = put(&mut state, P0, "Jennifer Walters", Zone::Hand);
    pool(
        &mut state,
        P0,
        &[ManaColor::G, ManaColor::W, ManaColor::W],
        3,
    );
    act(&mut state, Action::CastSpell(jw));
    match next(&mut state) {
        Decision::ChooseCastMode { options, .. } => {
            assert!(options.contains(&CastMode::Normal));
            assert!(options.contains(&CastMode::Alternative));
            act(&mut state, Action::ChooseCastMode(CastMode::Alternative));
        }
        other => panic!("expected ChooseCastMode, got {other:?}"),
    }
    assert!(matches!(
        next(&mut state),
        Decision::CastSpellOrPass { player: P0, .. }
    ));
    assert_eq!(zone(&state, jw), Zone::Stack);
    assert_eq!(
        engine::object_mana_value(&state, jw),
        6,
        "stack mana value of modal back face is 6"
    );
    assert_eq!(
        engine::object_color_mask(&state, jw),
        mana_colors_mask(&[ManaColor::G, ManaColor::W]),
        "stack colors of modal back face are G and W"
    );
    settled(&mut state);
    assert_eq!(zone(&state, jw), Zone::Battlefield);
    assert_eq!(state.objects.get(jw).v4.face_index, 1);
    assert_eq!(power_toughness(&state, jw), (6, 6));
    assert!(keyword(&state, jw, Keywords::REACH));
    assert!(keyword(&state, jw, Keywords::TRAMPLE));

    // Lurrus (MV <= 2) allows casting the 2-mana front face from graveyard, NOT the 6-mana back face.
    let mut state = plains();
    put(&mut state, P0, "Lurrus of the Dream-Den", Zone::Battlefield);
    let jw = put(&mut state, P0, "Jennifer Walters", Zone::Graveyard);
    pool(
        &mut state,
        P0,
        &[ManaColor::G, ManaColor::W, ManaColor::W],
        3,
    );
    assert!(castable(&mut state, jw));
    act(&mut state, Action::CastSpell(jw));
    // Must not offer ChooseCastMode::Alternative; enters directly on front face.
    settled(&mut state);
    assert_eq!(zone(&state, jw), Zone::Battlefield);
    assert_eq!(state.objects.get(jw).v4.face_index, 0);
    assert_eq!(power_toughness(&state, jw), (2, 3));

    // Yawgmoth's Will (unrestricted MV graveyard permission) allows casting the
    // 6-mana modal back face (The Sensational She-Hulk) from the graveyard end to end.
    let mut state = plains();
    let will = put(&mut state, P0, "Yawgmoth's Will", Zone::Hand);
    let jw = put(&mut state, P0, "Jennifer Walters", Zone::Graveyard);
    pool(&mut state, P0, &[ManaColor::B], 2);
    cast(&mut state, will, &[]);
    settled(&mut state);
    pool(
        &mut state,
        P0,
        &[ManaColor::G, ManaColor::W, ManaColor::W],
        3,
    );
    assert!(castable(&mut state, jw));
    act(&mut state, Action::CastSpell(jw));
    let mut chose_mode = false;
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseCastMode { options, .. } => {
            assert!(
                options.contains(&CastMode::Alternative),
                "Yawgmoth's Will must allow Alternative cast mode for She-Hulk on stack"
            );
            chose_mode = true;
            Action::ChooseCastMode(CastMode::Alternative)
        }
        other => first_answer(other),
    });
    assert!(chose_mode);
    assert_eq!(zone(&state, jw), Zone::Battlefield);
    assert_eq!(state.objects.get(jw).v4.face_index, 1);
    assert_eq!(power_toughness(&state, jw), (6, 6));
    assert!(keyword(&state, jw, Keywords::REACH));
    assert!(keyword(&state, jw, Keywords::TRAMPLE));
}

#[test]
fn she_hulk_trigger_decline_does_not_consume_cap_accept_caps_turn_and_resets_next_turn() {
    let mut state = plains();
    let she_hulk = put_transformed(&mut state, P0, "Jennifer Walters");
    let beast = put(&mut state, P0, "Questing Beast", Zone::Battlefield);

    // Move to P1's turn so P1 can cast spells freely.
    pass_until(&mut state, |s| {
        s.active_player == P1 && s.step == Step::Main1
    });

    // 1. First damage (Shock = 2): P0 declines (option 0) -> no damage, cap NOT consumed.
    let shock1 = put(&mut state, P1, "Shock", Zone::Hand);
    pool(&mut state, P1, &[ManaColor::R], 0);
    cast(&mut state, shock1, &[Target::Object(she_hulk)]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseTargets { .. } => Action::ChooseTarget(Target::Player(P1)),
        Decision::ChooseEffectOption { option_count, .. } => {
            assert_eq!(option_count, 2);
            Action::ChooseEffectOption(0)
        }
        other => first_answer(other),
    });
    assert_eq!(life(&state, P1), 20, "declining deals no damage");

    // 2. Second damage (Lightning Bolt = 3): P0 accepts (option 1) -> deals 3, consumes cap.
    let bolt = put(&mut state, P1, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P1, &[ManaColor::R], 0);
    cast(&mut state, bolt, &[Target::Object(she_hulk)]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseTargets { .. } => Action::ChooseTarget(Target::Player(P1)),
        Decision::ChooseEffectOption { .. } => Action::ChooseEffectOption(1),
        other => first_answer(other),
    });
    assert_eq!(life(&state, P1), 17, "accepted trigger dealt 3 damage");

    // 3. Third damage this same turn (Shock on Questing Beast): cap is already used.
    let shock2 = put(&mut state, P1, "Shock", Zone::Hand);
    pool(&mut state, P1, &[ManaColor::R], 0);
    cast(&mut state, shock2, &[Target::Object(beast)]);
    settled(&mut state);
    assert_eq!(life(&state, P1), 17, "already used once this turn");

    // 4. Next turn resets the cap.
    pass_until(&mut state, |s| {
        s.active_player == P0 && s.step == Step::Main1
    });
    let own_bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 0);
    cast(&mut state, own_bolt, &[Target::Object(beast)]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseTargets { .. } => Action::ChooseTarget(Target::Player(P1)),
        Decision::ChooseEffectOption { .. } => Action::ChooseEffectOption(1),
        other => first_answer(other),
    });
    assert_eq!(life(&state, P1), 14, "cap reset on the next turn");
}

#[test]
fn she_hulk_simultaneous_combat_triggers_lethal_lki_and_fizzle_preserves_cap() {
    // 1. Simultaneous combat damage to two creatures P0 controls (P1 attacks with two 1/2s,
    //    P0 blocks with She-Hulk 6/6 and Emeritus of Abundance 3/4):
    //    (a) Accepting the first trigger stops the second from prompting or dealing damage.
    //    (b) Declining the first trigger lets the second trigger still accept and deal damage.
    for accept_first in [true, false] {
        let mut state = plains();
        let she_hulk = put_transformed(&mut state, P0, "Jennifer Walters");
        let emeritus = put(&mut state, P0, "Emeritus of Abundance", Zone::Battlefield);
        let a1 = put(&mut state, P1, "Monastery Swiftspear", Zone::Battlefield);
        let a2 = put(&mut state, P1, "Monastery Swiftspear", Zone::Battlefield);
        pass_until(&mut state, |s| {
            s.active_player == P1 && s.step == Step::DeclareAttackers
        });
        act(&mut state, Action::DeclareAttackers(vec![a1, a2]));
        let mut prompts = 0usize;
        run_until(
            &mut state,
            |s| {
                s.active_player == P1
                    && s.step == Step::Main2
                    && s.stack.is_empty()
                    && s.engine.pending_triggers.is_empty()
                    && s.engine.pending_effect.is_none()
            },
            |_, decision| match decision {
                Decision::DeclareBlockers { .. } => Some(Action::DeclareBlockers(vec![
                    (she_hulk, a1),
                    (emeritus, a2),
                ])),
                Decision::ChooseTargets { .. } => Some(Action::ChooseTarget(Target::Player(P1))),
                Decision::ChooseEffectOption { .. } => {
                    prompts += 1;
                    let choice = if accept_first {
                        1
                    } else if prompts == 1 {
                        0
                    } else {
                        1
                    };
                    Some(Action::ChooseEffectOption(choice))
                }
                _ => None,
            },
        );
        assert_eq!(
            prompts,
            if accept_first { 1 } else { 2 },
            "accept_first={accept_first}"
        );
        assert_eq!(
            life(&state, P1),
            19,
            "exactly one She-Hulk trigger dealt 1 damage (accept_first={accept_first})"
        );
    }

    // 2. Lethal combat damage to She-Hulk herself: dies to SBAs, LKI trigger still deals 6.
    let mut state = plains();
    let p0_hulk = put_transformed(&mut state, P0, "Jennifer Walters");
    let p1_hulk = put_transformed(&mut state, P1, "Jennifer Walters");
    attack_with(&mut state, vec![p0_hulk], |_, decision| match decision {
        Decision::DeclareBlockers { .. } => Some(Action::DeclareBlockers(vec![(p1_hulk, p0_hulk)])),
        Decision::ChooseTargets { player, .. } => {
            Some(Action::ChooseTarget(Target::Player(player.opponent())))
        }
        Decision::ChooseEffectOption { player, .. } => {
            Some(Action::ChooseEffectOption(if *player == P0 {
                1
            } else {
                0
            }))
        }
        _ => None,
    });
    assert_eq!(zone(&state, p0_hulk), Zone::Graveyard);
    assert_eq!(
        life(&state, P1),
        14,
        "dead She-Hulk's trigger dealt 6 via LKI"
    );

    // 3. Fizzle on illegal target does NOT consume the once-per-turn cap.
    let mut state = plains();
    let she_hulk = put_transformed(&mut state, P0, "Jennifer Walters");
    let opp_elf = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    let safe = put(&mut state, P1, "Tamiyo's Safekeeping", Zone::Hand);
    pass_until(&mut state, |s| {
        s.active_player == P1 && s.step == Step::Main1
    });
    let shock1 = put(&mut state, P1, "Shock", Zone::Hand);
    pool(&mut state, P1, &[ManaColor::R], 0);
    cast(&mut state, shock1, &[Target::Object(she_hulk)]);
    // Resolve Shock -> She-Hulk triggers targeting opp_elf.
    assert!(matches!(
        next(&mut state),
        Decision::CastSpellOrPass { player: P1, .. }
    ));
    act(&mut state, Action::Pass);
    assert!(matches!(
        next(&mut state),
        Decision::CastSpellOrPass { player: P0, .. }
    ));
    act(&mut state, Action::Pass);
    choose_target(&mut state, Target::Object(opp_elf));
    // P1 casts Tamiyo's Safekeeping on opp_elf so She-Hulk's trigger fizzles.
    assert!(matches!(
        next(&mut state),
        Decision::CastSpellOrPass { player: P1, .. }
    ));
    pool(&mut state, P1, &[ManaColor::G], 0);
    cast(&mut state, safe, &[Target::Object(opp_elf)]);
    settled(&mut state);
    assert_eq!(zone(&state, opp_elf), Zone::Battlefield);

    // A second Shock this same turn STILL offers and resolves She-Hulk's trigger!
    let shock2 = put(&mut state, P1, "Shock", Zone::Hand);
    pool(&mut state, P1, &[ManaColor::R], 0);
    cast(&mut state, shock2, &[Target::Object(she_hulk)]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseTargets { .. } => Action::ChooseTarget(Target::Player(P1)),
        Decision::ChooseEffectOption { .. } => Action::ChooseEffectOption(1),
        other => first_answer(other),
    });
    assert_eq!(
        life(&state, P1),
        20,
        "20 + 2 from Safekeeping - 2 from accepted second trigger = 20"
    );
}

// ---- Emeritus of Abundance // Regrowth -----------------------------------------

#[test]
fn emeritus_of_abundance_enters_prepared_regrowth_targets_own_gy_and_respects_dress_down() {
    let mut state = plains();
    let emeritus = put(&mut state, P0, "Emeritus of Abundance", Zone::Hand);
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Graveyard);
    let land = put(&mut state, P0, "Forest", Zone::Graveyard);
    let opp_opt = put(&mut state, P1, "Opt", Zone::Graveyard);

    pool(&mut state, P0, &[ManaColor::G], 2);
    cast(&mut state, emeritus, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, emeritus), Zone::Battlefield);
    let copies = exile_copies(&state, "Regrowth");
    assert_eq!(copies.len(), 1, "Emeritus of Abundance enters prepared");
    let regrowth = copies[0];

    pool(&mut state, P0, &[ManaColor::G], 1);
    let targets = cast_targets(&state, regrowth);
    assert!(targets.contains(&Target::Object(bolt)));
    assert!(targets.contains(&Target::Object(land)));
    assert!(
        !targets.contains(&Target::Object(opp_opt)),
        "Regrowth targets only controller's graveyard"
    );

    cast(&mut state, regrowth, &[Target::Object(bolt)]);
    settled(&mut state);
    assert!(hand_contains(&state, P0, bolt));
    assert!(
        exile_copies(&state, "Regrowth").is_empty(),
        "prepared copy ceases after resolution"
    );
    assert!(
        !state.players[P0.index()].graveyard.contains(&regrowth),
        "prepare copy is never put into graveyard"
    );

    // Target legality rechecked on resolution: if target leaves graveyard in response, fizzles.
    let mut fizzle = plains();
    let em_f = put(&mut fizzle, P0, "Emeritus of Abundance", Zone::Hand);
    let bolt_f = put(&mut fizzle, P0, "Lightning Bolt", Zone::Graveyard);
    pool(&mut fizzle, P0, &[ManaColor::G], 2);
    cast(&mut fizzle, em_f, &[]);
    settled(&mut fizzle);
    let regrowth_f = exile_copies(&fizzle, "Regrowth")[0];
    pool(&mut fizzle, P0, &[ManaColor::G], 1);
    cast(&mut fizzle, regrowth_f, &[Target::Object(bolt_f)]);
    assert!(matches!(
        next(&mut fizzle),
        Decision::CastSpellOrPass { player: P0, .. }
    ));
    propose_and_commit(&mut fizzle, ProposedEvent::zone_change(bolt_f, Zone::Exile));
    settled(&mut fizzle);
    assert_eq!(zone(&fizzle, bolt_f), Zone::Exile);
    assert!(!hand_contains(&fizzle, P0, bolt_f));

    // Dress Down suppresses "enters prepared".
    let mut suppressed = plains();
    let dress = put(&mut suppressed, P1, "Dress Down", Zone::Hand);
    propose_and_commit(
        &mut suppressed,
        ProposedEvent::zone_change(dress, Zone::Battlefield),
    );
    settled(&mut suppressed);
    let em2 = put(&mut suppressed, P0, "Emeritus of Abundance", Zone::Hand);
    pool(&mut suppressed, P0, &[ManaColor::G], 2);
    cast(&mut suppressed, em2, &[]);
    settled(&mut suppressed);
    assert!(exile_copies(&suppressed, "Regrowth").is_empty());
}

#[test]
fn emeritus_of_abundance_attack_intervening_if_checks_eight_lands_on_trigger_and_resolution() {
    let mut state = plains();
    let emeritus = put(&mut state, P0, "Emeritus of Abundance", Zone::Hand);
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Graveyard);
    pool(&mut state, P0, &[ManaColor::G], 2);
    cast(&mut state, emeritus, &[]);
    settled(&mut state);
    let initial = exile_copies(&state, "Regrowth")[0];
    pool(&mut state, P0, &[ManaColor::G], 1);
    cast(&mut state, initial, &[Target::Object(bolt)]);
    settled(&mut state);
    assert!(exile_copies(&state, "Regrowth").is_empty());

    // Wait a turn so Emeritus can attack.
    pass_until(&mut state, |s| {
        s.active_player == P1 && s.step == Step::Main1
    });
    pass_until(&mut state, |s| {
        s.active_player == P0 && s.step == Step::Main1
    });

    // 1. With 7 lands: intervening-if fails on trigger, stays unprepared (Vigilance keeps untapped).
    let mut forests = Vec::new();
    for _ in 0..7 {
        forests.push(put(&mut state, P0, "Forest", Zone::Battlefield));
    }
    attack_with(&mut state, vec![emeritus], |_, _| None);
    assert!(exile_copies(&state, "Regrowth").is_empty());
    assert!(
        !state.objects.get(emeritus).tapped,
        "Vigilance keeps Emeritus untapped"
    );

    // 2. With 8 lands on attack, but P1 Strip Mines the 8th land in response -> fails on resolution!
    pass_until(&mut state, |s| {
        s.active_player == P1 && s.step == Step::Main1
    });
    pass_until(&mut state, |s| {
        s.active_player == P0 && s.step == Step::Main1
    });
    let eighth = put(&mut state, P0, "Forest", Zone::Battlefield);
    let mine = put(&mut state, P1, "Strip Mine", Zone::Battlefield);
    pass_until(&mut state, |s| s.step == Step::DeclareAttackers);
    act(&mut state, Action::DeclareAttackers(vec![emeritus]));
    // Trigger goes on the stack; P0 passes, P1 destroys the 8th Forest.
    assert!(matches!(
        next(&mut state),
        Decision::CastSpellOrPass { player: P0, .. }
    ));
    assert_eq!(state.stack.len(), 1);
    act(&mut state, Action::Pass);
    activate(&mut state, mine, 0, &[Target::Object(eighth)]);
    settled(&mut state);
    assert_eq!(zone(&state, eighth), Zone::Graveyard);
    assert!(
        exile_copies(&state, "Regrowth").is_empty(),
        "intervening-if rechecked on resolution with only 7 lands"
    );

    // 3. With 8 lands remaining through resolution -> becomes prepared again!
    pass_until(&mut state, |s| {
        s.active_player == P1 && s.step == Step::Main1
    });
    pass_until(&mut state, |s| {
        s.active_player == P0 && s.step == Step::Main1
    });
    put(&mut state, P0, "Forest", Zone::Battlefield);
    attack_with(&mut state, vec![emeritus], |_, _| None);
    assert_eq!(
        exile_copies(&state, "Regrowth").len(),
        1,
        "becomes prepared with 8 lands"
    );
}

// ---- Emeritus of Ideation // Ancestral Recall ----------------------------------

#[test]
fn emeritus_of_ideation_ward_enters_prepared_ancestral_recall_and_standalone_intact() {
    let mut state = plains();
    let emeritus = put(&mut state, P0, "Emeritus of Ideation", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U, ManaColor::U], 3);
    cast(&mut state, emeritus, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, emeritus), Zone::Battlefield);
    assert!(keyword(&state, emeritus, Keywords::FLYING));
    assert_eq!(exile_copies(&state, "Ancestral Recall").len(), 1);

    // Ward {2} counters an unpaid Lightning Bolt targeting Emeritus of Ideation.
    let opp_bolt = opponent_bolts(&mut state, Target::Object(emeritus));
    settle_first(&mut state);
    assert_eq!(zone(&state, opp_bolt), Zone::Graveyard);
    assert_eq!(state.objects.get(emeritus).damage, 0);

    // Casting the prepared Ancestral Recall copy draws 3 and ceases (never enters graveyard).
    let copy = exile_copies(&state, "Ancestral Recall")[0];
    let hand_before = state.players[P0.index()].hand.len();
    pool(&mut state, P0, &[ManaColor::U], 0);
    cast(&mut state, copy, &[Target::Player(P0)]);
    settled(&mut state);
    assert_eq!(state.players[P0.index()].hand.len(), hand_before + 3);
    assert!(exile_copies(&state, "Ancestral Recall").is_empty());
    assert!(!state.players[P0.index()].graveyard.contains(&copy));

    // Standalone Ancestral Recall card in hand still behaves normally and goes to graveyard.
    let real_recall = put(&mut state, P0, "Ancestral Recall", Zone::Hand);
    let hand_before = state.players[P0.index()].hand.len();
    pool(&mut state, P0, &[ManaColor::U], 0);
    cast(&mut state, real_recall, &[Target::Player(P0)]);
    settled(&mut state);
    assert_eq!(state.players[P0.index()].hand.len(), hand_before - 1 + 3);
    assert_eq!(zone(&state, real_recall), Zone::Graveyard);
}

#[test]
fn emeritus_of_ideation_attack_exile_eight_below_decline_choose_of_nine_and_exact_eight() {
    let mut state = plains();
    let emeritus = put(&mut state, P0, "Emeritus of Ideation", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U, ManaColor::U], 3);
    cast(&mut state, emeritus, &[]);
    settled(&mut state);
    let initial = exile_copies(&state, "Ancestral Recall")[0];
    pool(&mut state, P0, &[ManaColor::U], 0);
    cast(&mut state, initial, &[Target::Player(P0)]);
    settled(&mut state);
    assert!(exile_copies(&state, "Ancestral Recall").is_empty());

    // Wait a turn so Emeritus can attack.
    pass_until(&mut state, |s| {
        s.active_player == P1 && s.step == Step::Main1
    });
    pass_until(&mut state, |s| {
        s.active_player == P0 && s.step == Step::Main1
    });

    // 1. 7 cards in graveyard: no choice offered, stays unprepared.
    let mut gy = Vec::new();
    for _ in 0..7 {
        gy.push(put(&mut state, P0, "Island", Zone::Graveyard));
    }
    let mut prompted = false;
    attack_with(&mut state, vec![emeritus], |_, decision| match decision {
        Decision::ChooseEffectOption { .. } | Decision::ChooseEffectTargets { .. } => {
            prompted = true;
            None
        }
        _ => None,
    });
    assert!(!prompted, "fewer than 8 cards in graveyard must not prompt");
    assert!(exile_copies(&state, "Ancestral Recall").is_empty());
    assert_eq!(state.players[P0.index()].graveyard.len(), 7);

    // 2. 9 cards in graveyard, decline (option 0): all 9 remain, stays unprepared.
    pass_until(&mut state, |s| {
        s.active_player == P1 && s.step == Step::Main1
    });
    pass_until(&mut state, |s| {
        s.active_player == P0 && s.step == Step::Main1
    });
    gy.push(put(&mut state, P0, "Island", Zone::Graveyard));
    let keep = put(&mut state, P0, "Lightning Bolt", Zone::Graveyard);
    let mut saw_decline = false;
    attack_with(&mut state, vec![emeritus], |_, decision| match decision {
        Decision::ChooseEffectOption { option_count, .. } => {
            saw_decline = true;
            assert_eq!(*option_count, 2);
            Some(Action::ChooseEffectOption(0))
        }
        _ => None,
    });
    assert!(saw_decline);
    assert_eq!(state.players[P0.index()].graveyard.len(), 9);
    assert!(exile_copies(&state, "Ancestral Recall").is_empty());

    // 3. 9 cards in graveyard, accept (option 1): prompts exact 8 of 9; exile the 8 Islands,
    //    keep Lightning Bolt in graveyard, become prepared again!
    pass_until(&mut state, |s| {
        s.active_player == P1 && s.step == Step::Main1
    });
    pass_until(&mut state, |s| {
        s.active_player == P0 && s.step == Step::Main1
    });
    let mut picks = 0usize;
    attack_with(&mut state, vec![emeritus], |_, decision| match decision {
        Decision::ChooseEffectOption { .. } => Some(Action::ChooseEffectOption(1)),
        Decision::ChooseEffectTargets {
            min_targets,
            max_targets,
            can_finish,
            legal_targets,
            ..
        } => {
            assert_eq!((*min_targets, *max_targets, *can_finish), (8, 8, false));
            picks += 1;
            let pick = legal_targets
                .iter()
                .copied()
                .find(|&t| t != Target::Object(keep))
                .expect("non-Bolt Island candidate");
            Some(Action::ChooseEffectTarget(pick))
        }
        _ => None,
    });
    assert_eq!(picks, 8);
    assert_eq!(zone(&state, keep), Zone::Graveyard);
    assert_eq!(state.players[P0.index()].graveyard, vec![keep]);
    assert_eq!(exile_copies(&state, "Ancestral Recall").len(), 1);

    // 4. Exact 8 cards in graveyard, accept (option 1): exiles all 8 directly and prepares!
    let copy = exile_copies(&state, "Ancestral Recall")[0];
    pool(&mut state, P0, &[ManaColor::U], 0);
    cast(&mut state, copy, &[Target::Player(P1)]);
    settled(&mut state);
    pass_until(&mut state, |s| {
        s.active_player == P1 && s.step == Step::Main1
    });
    pass_until(&mut state, |s| {
        s.active_player == P0 && s.step == Step::Main1
    });
    // Ensure exactly 8 cards in P0's graveyard right before combat (regardless of any cleanup).
    while state.players[P0.index()].graveyard.len() < 8 {
        put(&mut state, P0, "Island", Zone::Graveyard);
    }
    assert_eq!(state.players[P0.index()].graveyard.len(), 8);
    // Ensure P1 stays alive long enough for this attack step.
    state.players[P1.index()].life = 20;
    let mut target_prompts = 0usize;
    attack_with(&mut state, vec![emeritus], |_, decision| match decision {
        Decision::ChooseEffectOption { .. } => Some(Action::ChooseEffectOption(1)),
        Decision::ChooseEffectTargets { .. } => {
            target_prompts += 1;
            None
        }
        _ => None,
    });
    assert_eq!(
        target_prompts, 0,
        "exact 8 cards auto-exiles without clicks"
    );
    assert!(state.players[P0.index()].graveyard.is_empty());
    assert_eq!(exile_copies(&state, "Ancestral Recall").len(), 1);
}

// ---- Public-session end-to-end replay ------------------------------------------

#[derive(Default)]
struct C8aEvents {
    casts: [u32; C8A_CARDS.len()],
}

impl C8aEvents {
    fn observe(&mut self, _decision: &Value, action: &Value) {
        let semantic = &action["semantic"];
        if semantic["action_kind"].as_str() != Some("cast_spell") {
            return;
        }
        let Some(card_id) = semantic["source"]["card_db_id"]
            .as_u64()
            .and_then(|id| usize::try_from(id).ok())
        else {
            return;
        };
        let name = CARD_DEFS[card_id].name;
        if let Some(index) = C8A_CARDS.iter().position(|&card| card == name) {
            self.casts[index] += 1;
        }
    }
}

#[test]
fn c8a_decks_finish_and_replay_through_public_sessions_both_seats() {
    let grixis = deck(&[
        ("Volcanic Island", 10),
        ("Underground Sea", 8),
        ("Badlands", 6),
        ("Bonecrusher Giant", 8),
        ("Concealing Curtains", 8),
        ("Jace, Vryn's Prodigy", 8),
        ("Emeritus of Ideation", 8),
        ("Opt", 4),
    ]);
    let bant = deck(&[
        ("Savannah", 10),
        ("Tropical Island", 8),
        ("Tundra", 6),
        ("Jennifer Walters", 10),
        ("Emeritus of Abundance", 10),
        ("Llanowar Elves", 8),
        ("Opt", 4),
        ("Ponder", 4),
    ]);
    let mut events = C8aEvents::default();
    for pair in [[grixis.clone(), bant.clone()], [bant, grixis]] {
        for seed in 0xC8A0_u64..0xC8A4 {
            for policy in [Policy::Ranked, Policy::Random(seed + 0x100)] {
                let played = play_observed(pair.clone(), None, seed, policy, &mut |d, a| {
                    events.observe(d, a)
                });
                let replay = play_observed(pair.clone(), None, seed, policy, &mut |_, _| {});
                assert_eq!(played.transcript_sha256, replay.transcript_sha256);
            }
        }
    }
    for (index, name) in C8A_CARDS.iter().enumerate() {
        assert!(
            events.casts[index] > 0,
            "{name} was never cast across the public C8a sessions"
        );
    }
}
