//! Vintage Cube wave C9c: native rules and public-session tests for Elspeth,
//! Storm Slayer; Grist, the Hunger Tide (+ Black Green Insect Token); Nissa,
//! Who Shakes the World; Tamiyo, Collector of Tales; and Through the Breach
//! (including generalized Splice onto Arcane).
#![cfg(feature = "eternal-fixtures")]
#![allow(dead_code, unused_imports)]

use crate::eternal_catalog_v1::{play_observed, Policy};
use mtg_kernel::card_def::{
    card_id_by_name, CardCapability, CardDef, CardType, Keywords, Subtype, Supertype, TargetSpec,
    CARD_DEFS,
};
use mtg_kernel::engine::{self, Action, Decision, OptionalCostChoice};
use mtg_kernel::eternal_cards_v1::{card_type_count, EternalStateV1};
use mtg_kernel::eternal_cube_c9c_v1::C9cStateV1;
use mtg_kernel::eternal_sideboard_s1b_v1::nameable_nonland;
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::limited_session_v1::{CustomCardCountV1, CustomDeckV1};
use mtg_kernel::mana::ManaColor;
use mtg_kernel::planeswalker_v1::{change_loyalty, loyalty};
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Target, Zone};
use serde_json::Value;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

const P0: PlayerId = PlayerId::P0;
const P1: PlayerId = PlayerId::P1;

const C9C_CARDS: [&str; 5] = [
    "Elspeth, Storm Slayer",
    "Grist, the Hunger Tide",
    "Nissa, Who Shakes the World",
    "Tamiyo, Collector of Tales",
    "Through the Breach",
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
        0x4355_4245_0000 + 0x3963,
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

fn settle_with(state: &mut GameState, mut answer: impl FnMut(&GameState, Decision) -> Action) {
    while let Some(decision) = settle(state) {
        let action = answer(state, decision);
        act(state, action);
    }
}

fn settle_first(state: &mut GameState) {
    settle_with(state, |_, decision| first_answer(decision));
}

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

fn cast_targets(state: &GameState, spell: ObjectId) -> Vec<Target> {
    let mut probe = state.clone();
    assert!(castable(&mut probe, spell));
    act(&mut probe, Action::CastSpell(spell));
    match next(&mut probe) {
        Decision::ChooseTargets { legal_targets, .. }
        | Decision::ChooseEffectTargets { legal_targets, .. } => legal_targets,
        other => panic!("expected targets, got {other:?}"),
    }
}

fn activate_targets(state: &GameState, source: ObjectId, index: u8) -> Vec<Target> {
    let mut probe = state.clone();
    assert!(activatable(&mut probe, source, index));
    act(&mut probe, Action::ActivateAbility(source, index));
    match next(&mut probe) {
        Decision::ChooseTargets { legal_targets, .. }
        | Decision::ChooseEffectTargets { legal_targets, .. } => legal_targets,
        other => panic!("expected targets, got {other:?}"),
    }
}

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

fn pass_until(state: &mut GameState, stop: impl Fn(&GameState) -> bool) {
    run_until(state, stop, |_, _| None);
}

fn hand_contains(state: &GameState, player: PlayerId, object: ObjectId) -> bool {
    state.players[player.index()].hand.contains(&object)
}

fn mana(state: &GameState, player: PlayerId, color: ManaColor) -> u8 {
    state.players[player.index()].mana_pool[color.pool_index()]
}

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

fn life(state: &GameState, player: PlayerId) -> i32 {
    state.players[player.index()].life
}

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

fn cast_elspeth(state: &mut GameState, player: PlayerId) -> ObjectId {
    state.active_player = player;
    state.priority_player = player;
    let elspeth = put(state, player, "Elspeth, Storm Slayer", Zone::Hand);
    pool(state, player, &[ManaColor::W, ManaColor::W], 3);
    cast(state, elspeth, &[]);
    settled(state);
    elspeth
}

fn cast_grist(state: &mut GameState, player: PlayerId) -> ObjectId {
    state.active_player = player;
    state.priority_player = player;
    let grist = put(state, player, "Grist, the Hunger Tide", Zone::Hand);
    pool(state, player, &[ManaColor::B, ManaColor::G], 1);
    cast(state, grist, &[]);
    settled(state);
    grist
}

fn cast_nissa(state: &mut GameState, player: PlayerId) -> ObjectId {
    state.active_player = player;
    state.priority_player = player;
    let nissa = put(state, player, "Nissa, Who Shakes the World", Zone::Hand);
    pool(state, player, &[ManaColor::G, ManaColor::G], 3);
    cast(state, nissa, &[]);
    settled(state);
    nissa
}

fn cast_tamiyo(state: &mut GameState, player: PlayerId) -> ObjectId {
    state.active_player = player;
    state.priority_player = player;
    let tamiyo = put(state, player, "Tamiyo, Collector of Tales", Zone::Hand);
    pool(state, player, &[ManaColor::G, ManaColor::U], 2);
    cast(state, tamiyo, &[]);
    settled(state);
    tamiyo
}

fn nonland_name_choice(name: &str) -> (u16, u16) {
    let target = CARD_DEFS[id(name) as usize].object_name;
    for (group_index, group) in nameable_nonland().iter().enumerate() {
        if let Some(card_index) = group
            .iter()
            .position(|&card_def| CARD_DEFS[usize::from(card_def)].object_name == target)
        {
            return (
                u16::try_from(group_index).unwrap(),
                u16::try_from(card_index).unwrap(),
            );
        }
    }
    panic!("{name} not in nameable_nonland()");
}

// ---- Characteristics & State Isolation -----------------------------------------

#[test]
fn c9c_cards_are_full_with_exact_characteristics() {
    for name in C9C_CARDS {
        let card = def(name);
        assert_eq!(card.capability, CardCapability::Full, "{name}");
        assert!(card.is_executable(), "{name}");
    }

    let elspeth = def("Elspeth, Storm Slayer");
    assert_eq!(elspeth.mana_value, 5);
    assert_eq!(elspeth.starting_loyalty, Some(5));
    assert!(elspeth.supertypes.contains(&Supertype::Legendary));
    assert_eq!(elspeth.types, &[CardType::Planeswalker]);
    assert_eq!(elspeth.subtypes, &[Subtype::Elspeth]);
    assert_eq!(elspeth.colors, &[ManaColor::W]);

    let grist = def("Grist, the Hunger Tide");
    assert_eq!(grist.mana_value, 3);
    assert_eq!(grist.starting_loyalty, Some(3));
    assert!(grist.supertypes.contains(&Supertype::Legendary));
    assert_eq!(grist.types, &[CardType::Planeswalker]);
    assert_eq!(grist.subtypes, &[Subtype::Grist]);
    assert_eq!(grist.colors, &[ManaColor::B, ManaColor::G]);

    let nissa = def("Nissa, Who Shakes the World");
    assert_eq!(nissa.mana_value, 5);
    assert_eq!(nissa.starting_loyalty, Some(5));
    assert!(nissa.supertypes.contains(&Supertype::Legendary));
    assert_eq!(nissa.types, &[CardType::Planeswalker]);
    assert_eq!(nissa.subtypes, &[Subtype::Nissa]);
    assert_eq!(nissa.colors, &[ManaColor::G]);

    let tamiyo = def("Tamiyo, Collector of Tales");
    assert_eq!(tamiyo.mana_value, 4);
    assert_eq!(tamiyo.starting_loyalty, Some(5));
    assert!(tamiyo.supertypes.contains(&Supertype::Legendary));
    assert_eq!(tamiyo.types, &[CardType::Planeswalker]);
    assert_eq!(tamiyo.subtypes, &[Subtype::Tamiyo]);
    assert_eq!(tamiyo.colors, &[ManaColor::U, ManaColor::G]);

    let breach = def("Through the Breach");
    assert_eq!(breach.mana_value, 5);
    assert_eq!(breach.starting_loyalty, None);
    assert_eq!(breach.types, &[CardType::Instant]);
    assert_eq!(breach.subtypes, &[Subtype::Arcane]);
    assert_eq!(breach.colors, &[ManaColor::R]);

    let insect = def("Black Green Insect Token");
    assert_eq!(insect.capability, CardCapability::Full);
    assert!(insect.is_token);
    assert_eq!((insect.power, insect.toughness), (Some(1), Some(1)));
    assert_eq!(insect.types, &[CardType::Creature]);
    assert_eq!(insect.subtypes, &[Subtype::Insect]);
    assert_eq!(insect.colors, &[ManaColor::B, ManaColor::G]);
}

#[test]
fn c9c_empty_state_skips_serde_and_hashes_zero_bytes() {
    let empty = C9cStateV1::default();
    assert!(empty.is_empty());

    let eternal = EternalStateV1::default();
    let json = serde_json::to_value(&eternal).expect("serialize EternalStateV1");
    assert!(
        json.get("c9c").is_none(),
        "empty C9cStateV1 must be omitted from EternalStateV1 serde output"
    );

    let mut before = DefaultHasher::new();
    let mut after = DefaultHasher::new();
    0x1234_u64.hash(&mut before);
    0x1234_u64.hash(&mut after);
    empty.hash(&mut after);
    assert_eq!(
        before.finish(),
        after.finish(),
        "empty C9cStateV1 must hash zero bytes"
    );
}

// ---- Elspeth, Storm Slayer -----------------------------------------------------

#[test]
fn elspeth_doubles_own_plus_one_noncreature_tokens_batches_and_token_copies() {
    let mut state = plains();
    let elspeth = cast_elspeth(&mut state, P0);
    assert_eq!(zone(&state, elspeth), Zone::Battlefield);
    assert_eq!(loyalty(&state, elspeth), Some(5));

    // +1 creates TWO 1/1 white Soldier tokens instead of one.
    activate(&mut state, elspeth, 0, &[]);
    settled(&mut state);
    assert_eq!(loyalty(&state, elspeth), Some(6));
    let soldiers = named(&state, P0, "Soldier Token");
    assert_eq!(
        soldiers.len(),
        2,
        "Elspeth doubles her own +1 Soldier token"
    );
    for soldier in soldiers {
        assert_eq!(power_toughness(&state, soldier), (1, 1));
    }

    // Noncreature artifact tokens (Thraben Inspector -> 2 Clue tokens) are doubled.
    let inspector = put(&mut state, P0, "Thraben Inspector", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W], 0);
    cast(&mut state, inspector, &[]);
    settled(&mut state);
    assert_eq!(
        named(&state, P0, "Clue Token").len(),
        2,
        "Elspeth doubles controlled noncreature artifact tokens"
    );

    // Multi-token batch (Lingering Souls -> 4 White Flying Spirit Tokens).
    let souls = put(&mut state, P0, "Lingering Souls", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W], 2);
    cast(&mut state, souls, &[]);
    settled(&mut state);
    assert_eq!(
        named(&state, P0, "White Flying Spirit Token").len(),
        4,
        "Elspeth doubles multi-token batches"
    );

    // Opponent token creation is NOT doubled by P0's Elspeth.
    pass_until(&mut state, |s| {
        s.active_player == P1 && s.step == Step::Main1
    });
    let opp_inspector = put(&mut state, P1, "Thraben Inspector", Zone::Hand);
    pool(&mut state, P1, &[ManaColor::W], 0);
    cast(&mut state, opp_inspector, &[]);
    settled(&mut state);
    assert_eq!(
        named(&state, P1, "Clue Token").len(),
        1,
        "Elspeth only doubles tokens created under her controller's control"
    );

    // Token copies (Vaultborn Tyrant dying -> TWO artifact-creature token copies).
    let mut state = plains();
    let _elspeth = cast_elspeth(&mut state, P0);
    let tyrant = put(&mut state, P0, "Vaultborn Tyrant", Zone::Battlefield);
    let edict = put(&mut state, P0, "Diabolic Edict", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 1);
    cast(&mut state, edict, &[Target::Player(P0)]);
    settled(&mut state);
    assert_eq!(zone(&state, tyrant), Zone::Graveyard);
    let copies = named(&state, P0, "Vaultborn Tyrant");
    assert_eq!(
        copies.len(),
        2,
        "Elspeth doubles token copies created under her controller's control"
    );
    for copy in copies {
        assert!(state.objects.get(copy).v4.is_token);
        assert!(engine::object_has_type(&state, copy, CardType::Artifact));
        assert!(engine::object_has_type(&state, copy, CardType::Creature));
        assert_eq!(power_toughness(&state, copy), (6, 6));
    }

    // Two Elspeths multiply by 2^2 = 4 tokens per creation event.
    let mut state = plains();
    put(&mut state, P0, "Elspeth, Storm Slayer", Zone::Battlefield);
    put(&mut state, P0, "Elspeth, Storm Slayer", Zone::Battlefield);
    event::propose_and_commit(
        &mut state,
        ProposedEvent::create_token(id("Soldier Token"), P0),
    );
    assert_eq!(
        named(&state, P0, "Soldier Token").len(),
        4,
        "two Elspeths produce 2^2 = 4 tokens"
    );
}

#[test]
fn elspeth_zero_puts_counters_and_grants_flying_until_next_turn() {
    let mut state = plains();
    let elspeth = cast_elspeth(&mut state, P0);
    let mystic = put(&mut state, P0, "Elvish Mystic", Zone::Battlefield);
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    let opp_mystic = put(&mut state, P1, "Elvish Mystic", Zone::Battlefield);

    // Activate 0: +1/+1 counter and flying until P0's next turn.
    activate(&mut state, elspeth, 1, &[]);
    settled(&mut state);
    assert_eq!(loyalty(&state, elspeth), Some(5));
    assert_eq!(power_toughness(&state, mystic), (2, 2));
    assert_eq!(power_toughness(&state, elves), (2, 2));
    assert!(keyword(&state, mystic, Keywords::FLYING));
    assert!(keyword(&state, elves, Keywords::FLYING));
    assert_eq!(power_toughness(&state, opp_mystic), (1, 1));
    assert!(!keyword(&state, opp_mystic, Keywords::FLYING));

    // A creature entering after 0 resolved does NOT get a counter or flying.
    let late = put(&mut state, P0, "Elvish Mystic", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 0);
    cast(&mut state, late, &[]);
    settled(&mut state);
    assert_eq!(power_toughness(&state, late), (1, 1));
    assert!(!keyword(&state, late, Keywords::FLYING));

    // On P1's turn, flying is STILL active (not "until end of turn").
    pass_until(&mut state, |s| {
        s.active_player == P1 && s.step == Step::Main1
    });
    assert!(
        keyword(&state, mystic, Keywords::FLYING),
        "Elspeth 0 flying must persist through opponent's turn"
    );
    assert!(keyword(&state, elves, Keywords::FLYING));

    // On P0's next turn, flying expires while the +1/+1 counters stay.
    pass_until(&mut state, |s| {
        s.active_player == P0 && s.step == Step::Main1
    });
    assert!(
        !keyword(&state, mystic, Keywords::FLYING),
        "Elspeth 0 flying must expire on controller's next turn"
    );
    assert!(!keyword(&state, elves, Keywords::FLYING));
    assert_eq!(power_toughness(&state, mystic), (2, 2));
    assert_eq!(power_toughness(&state, elves), (2, 2));
}

#[test]
fn elspeth_minus_three_targets_opponent_mv_three_plus_and_rechecks_on_resolution() {
    let mut state = plains();
    let elspeth = cast_elspeth(&mut state, P0);
    let own_sheoldred = put(
        &mut state,
        P0,
        "Sheoldred, the Apocalypse",
        Zone::Battlefield,
    );
    let opp_mv1 = put(&mut state, P1, "Elvish Mystic", Zone::Battlefield);
    let opp_mv2 = put(
        &mut state,
        P1,
        "Thalia, Guardian of Thraben",
        Zone::Battlefield,
    );
    let opp_mv4 = put(
        &mut state,
        P1,
        "Sheoldred, the Apocalypse",
        Zone::Battlefield,
    );

    let targets = activate_targets(&state, elspeth, 2);
    assert_eq!(
        targets,
        vec![Target::Object(opp_mv4)],
        "only opponent creatures with MV >= 3 are legal targets"
    );
    assert!(!targets.contains(&Target::Object(own_sheoldred)));
    assert!(!targets.contains(&Target::Object(opp_mv1)));
    assert!(!targets.contains(&Target::Object(opp_mv2)));

    // Resolve -3 destroying P1's Sheoldred.
    activate(&mut state, elspeth, 2, &[Target::Object(opp_mv4)]);
    settled(&mut state);
    assert_eq!(loyalty(&state, elspeth), Some(2));
    assert_eq!(zone(&state, opp_mv4), Zone::Graveyard);

    // Recheck legality on resolution: Tamiyo's Safekeeping grants hexproof + indestructible.
    let mut state = plains();
    let elspeth = cast_elspeth(&mut state, P0);
    let opp_mv4 = put(
        &mut state,
        P1,
        "Sheoldred, the Apocalypse",
        Zone::Battlefield,
    );
    activate(&mut state, elspeth, 2, &[Target::Object(opp_mv4)]);
    assert!(matches!(
        next(&mut state),
        Decision::CastSpellOrPass { player: P0, .. }
    ));
    act(&mut state, Action::Pass);
    let safekeeping = put(&mut state, P1, "Tamiyo's Safekeeping", Zone::Hand);
    pool(&mut state, P1, &[ManaColor::G], 0);
    cast(&mut state, safekeeping, &[Target::Object(opp_mv4)]);
    settled(&mut state);
    assert_eq!(
        zone(&state, opp_mv4),
        Zone::Battlefield,
        "target becoming illegal / indestructible in response survives -3"
    );
}

// ---- Grist, the Hunger Tide ----------------------------------------------------

#[test]
fn grist_cda_works_in_all_nonbattlefield_zones_and_not_on_battlefield() {
    // 1. Hand, Library, Graveyard, Exile characteristics.
    let mut state = plains();
    let grist_hand = put(&mut state, P0, "Grist, the Hunger Tide", Zone::Hand);
    let grist_lib = put(&mut state, P0, "Grist, the Hunger Tide", Zone::Library);
    let grist_gy = put(&mut state, P0, "Grist, the Hunger Tide", Zone::Graveyard);
    let grist_exile = put(&mut state, P0, "Grist, the Hunger Tide", Zone::Exile);

    for obj in [grist_hand, grist_lib, grist_gy, grist_exile] {
        assert!(engine::object_has_type(&state, obj, CardType::Creature));
        assert!(engine::object_has_type(&state, obj, CardType::Planeswalker));
        assert!(engine::has_effective_subtype(&state, obj, Subtype::Insect));
        assert!(engine::has_effective_subtype(&state, obj, Subtype::Grist));
        assert_eq!(power_toughness(&state, obj), (1, 1));
    }

    // Graveyard card_type_count counts BOTH Creature and Planeswalker (2 types!).
    assert_eq!(
        card_type_count(&state, &[grist_gy]),
        2,
        "Grist in graveyard counts as both Creature and Planeswalker"
    );
    let pyrogoyf = put(&mut state, P0, "Pyrogoyf", Zone::Battlefield);
    assert_eq!(
        power_toughness(&state, pyrogoyf),
        (2, 3),
        "Pyrogoyf sees Creature + Planeswalker from Grist alone"
    );

    // 2. Stack CDA: creature-cast trigger fires, NOT taxed by Thalia, legal for
    //    Essence Scatter, illegal for Negate and Force of Negation.
    let mut state = plains();
    let beastcaller = put(&mut state, P0, "Quirion Beastcaller", Zone::Battlefield);
    let _thalia = put(
        &mut state,
        P1,
        "Thalia, Guardian of Thraben",
        Zone::Battlefield,
    );
    let grist = put(&mut state, P0, "Grist, the Hunger Tide", Zone::Hand);
    // Exact {1}{B}{G}: would be uncastable if Thalia taxed Grist as noncreature.
    pool(&mut state, P0, &[ManaColor::B, ManaColor::G], 1);
    assert!(
        castable(&mut state, grist),
        "Thalia must not tax Grist (it is a creature spell)"
    );
    cast(&mut state, grist, &[]);
    // Quirion Beastcaller triggers on casting creature spell Grist.
    assert!(matches!(
        next(&mut state),
        Decision::CastSpellOrPass { player: P0, .. }
    ));
    act(&mut state, Action::Pass);
    let scatter = put(&mut state, P1, "Essence Scatter", Zone::Hand);
    let negate = put(&mut state, P1, "Negate", Zone::Hand);
    let fon = put(&mut state, P1, "Force of Negation", Zone::Hand);
    pool(
        &mut state,
        P1,
        &[ManaColor::U, ManaColor::U, ManaColor::U],
        4,
    );
    assert!(
        castable(&mut state, scatter),
        "Essence Scatter can counter Grist on the stack"
    );
    assert!(
        !castable(&mut state, negate),
        "Negate cannot target creature spell Grist"
    );
    assert!(
        !castable(&mut state, fon),
        "Force of Negation cannot target creature spell Grist"
    );
    // Let Grist and Quirion Beastcaller trigger resolve.
    settled(&mut state);
    assert_eq!(
        state.objects.get(beastcaller).counters.plus1_plus1,
        1,
        "Quirion Beastcaller triggered on casting Grist"
    );

    // 3. On the battlefield: ONLY Legendary Planeswalker - Grist with 3 loyalty!
    assert_eq!(zone(&state, grist), Zone::Battlefield);
    assert_eq!(loyalty(&state, grist), Some(3));
    assert!(engine::object_has_type(
        &state,
        grist,
        CardType::Planeswalker
    ));
    assert!(
        !engine::object_has_type(&state, grist, CardType::Creature),
        "Grist is NOT a creature on the battlefield"
    );
    assert!(engine::has_effective_subtype(&state, grist, Subtype::Grist));
    assert!(
        !engine::has_effective_subtype(&state, grist, Subtype::Insect),
        "Grist is NOT an Insect on the battlefield"
    );

    // 4. Graveyard reanimation: Reanimate, Unearth (MV 3), Metamorphosis Fanatic.
    let mut state = plains();
    let grist = put(&mut state, P0, "Grist, the Hunger Tide", Zone::Graveyard);
    let unearth = put(&mut state, P0, "Unearth", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 0);
    assert_eq!(cast_targets(&state, unearth), vec![Target::Object(grist)]);
    cast(&mut state, unearth, &[Target::Object(grist)]);
    settled(&mut state);
    assert_eq!(zone(&state, grist), Zone::Battlefield);
    assert_eq!(loyalty(&state, grist), Some(3));

    let mut state = plains();
    let grist = put(&mut state, P0, "Grist, the Hunger Tide", Zone::Graveyard);
    let reanimate = put(&mut state, P0, "Reanimate", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 0);
    assert_eq!(cast_targets(&state, reanimate), vec![Target::Object(grist)]);
    cast(&mut state, reanimate, &[Target::Object(grist)]);
    settled(&mut state);
    assert_eq!(zone(&state, grist), Zone::Battlefield);
    assert_eq!(loyalty(&state, grist), Some(3));
    assert_eq!(life(&state, P0), 17);

    // 5. Library tutors: Green Sun's Zenith (X=3) and Recruiter of the Guard.
    let mut state = game(&["Grist, the Hunger Tide", "Plains", "Plains"]);
    let grist_in_lib = state.players[0].library[0];
    let gsz = put(&mut state, P0, "Green Sun's Zenith", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 3);
    cast(&mut state, gsz, &[]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseEffectOption { .. } => Action::ChooseEffectOption(3),
        Decision::ChooseEffectTargets { legal_targets, .. } => {
            assert!(
                legal_targets.contains(&Target::Object(grist_in_lib)),
                "Green Sun's Zenith (X=3) must find Grist in library"
            );
            Action::ChooseEffectTarget(Target::Object(grist_in_lib))
        }
        other => first_answer(other),
    });
    assert_eq!(zone(&state, grist_in_lib), Zone::Battlefield);
    assert_eq!(loyalty(&state, grist_in_lib), Some(3));

    let mut state = game(&["Grist, the Hunger Tide", "Plains", "Plains"]);
    let grist_in_lib = state.players[0].library[0];
    let recruiter = put(&mut state, P0, "Recruiter of the Guard", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W], 2);
    cast(&mut state, recruiter, &[]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseEffectBoolean { .. } => Action::ChooseEffectBoolean(true),
        Decision::ChooseEffectTargets { legal_targets, .. } => {
            assert!(
                legal_targets.contains(&Target::Object(grist_in_lib)),
                "Recruiter of the Guard must find 1/1 Grist in library"
            );
            Action::ChooseEffectTarget(Target::Object(grist_in_lib))
        }
        other => first_answer(other),
    });
    assert!(hand_contains(&state, P0, grist_in_lib));

    // 6. Hand checks: Duress skips Grist (it is a creature card!), Sneak Attack offers Grist.
    let mut state = plains();
    let grist = put(&mut state, P1, "Grist, the Hunger Tide", Zone::Hand);
    let duress = put(&mut state, P0, "Duress", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 0);
    cast(&mut state, duress, &[Target::Player(P1)]);
    settled(&mut state);
    assert!(
        hand_contains(&state, P1, grist),
        "Duress cannot take creature card Grist from hand"
    );

    let mut state = plains();
    let grist = put(&mut state, P0, "Grist, the Hunger Tide", Zone::Hand);
    let sneak = put(&mut state, P0, "Sneak Attack", Zone::Battlefield);
    pool(&mut state, P0, &[ManaColor::R], 0);
    activate(&mut state, sneak, 0, &[]);
    settle_choosing(&mut state, grist);
    assert_eq!(zone(&state, grist), Zone::Battlefield);
    assert_eq!(loyalty(&state, grist), Some(3));
}

#[test]
fn grist_plus_one_creates_insect_mills_and_repeats_on_insect_changeling_and_grist() {
    let mut state = game(&[
        "Wirewood Symbiote",
        "Masked Vandal",
        "Grist, the Hunger Tide",
        "Plains",
        "Plains",
    ]);
    let grist = cast_grist(&mut state, P0);
    assert_eq!(loyalty(&state, grist), Some(3));

    // +1 (loyalty 3 -> 4) -> Insect #1, mill Wirewood Symbiote (Insect, +1 => 5) ->
    // Insect #2, mill Masked Vandal (Changeling/Insect, +1 => 6) ->
    // Insect #3, mill Grist (CDA Insect, +1 => 7) ->
    // Insect #4, mill Plains (non-Insect) -> stop!
    activate(&mut state, grist, 0, &[]);
    settled(&mut state);
    let insects = named(&state, P0, "Black Green Insect Token");
    assert_eq!(insects.len(), 4);
    for insect in insects {
        assert_eq!(power_toughness(&state, insect), (1, 1));
        assert!(engine::has_effective_subtype(
            &state,
            insect,
            Subtype::Insect
        ));
    }
    assert_eq!(state.players[0].graveyard.len(), 4);
    assert_eq!(loyalty(&state, grist), Some(7));

    // Empty library edge case: creates 1 Insect token and stops cleanly.
    let mut state = plains();
    let grist = cast_grist(&mut state, P0);
    state.players[0].library.clear();
    activate(&mut state, grist, 0, &[]);
    settled(&mut state);
    assert_eq!(named(&state, P0, "Black Green Insect Token").len(), 1);
    assert_eq!(loyalty(&state, grist), Some(4));

    // Grist dies in response to +1: still creates tokens and repeats on milled Insect.
    let mut state = game(&["Wirewood Symbiote", "Plains"]);
    let grist = cast_grist(&mut state, P0);
    activate(&mut state, grist, 0, &[]);
    assert!(matches!(
        next(&mut state),
        Decision::CastSpellOrPass { player: P0, .. }
    ));
    act(&mut state, Action::Pass);
    // Two Lightning Bolts kill 4-loyalty Grist in response to +1.
    let bolt1 = put(&mut state, P1, "Lightning Bolt", Zone::Hand);
    let bolt2 = put(&mut state, P1, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P1, &[ManaColor::R, ManaColor::R], 0);
    cast(&mut state, bolt1, &[Target::Object(grist)]);
    cast(&mut state, bolt2, &[Target::Object(grist)]);
    settled(&mut state);
    assert_eq!(zone(&state, grist), Zone::Graveyard);
    assert_eq!(
        named(&state, P0, "Black Green Insect Token").len(),
        2,
        "+1 still repeats when an Insect is milled even after Grist left"
    );
}

#[test]
fn grist_minus_two_reflexive_trigger_and_minus_five_drain() {
    // 1. -2 declining sacrifice queues no reflexive trigger.
    let mut state = plains();
    let grist = cast_grist(&mut state, P0);
    let fodder = token(&mut state, P0, "Black Green Insect Token");
    let opp_oko = cast_tamiyo(&mut state, P1);
    state.active_player = P0;
    state.priority_player = P0;
    activate(&mut state, grist, 1, &[]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseEffectTargets {
            can_finish: true, ..
        } => Action::FinishEffectSelection,
        other => panic!("unexpected {other:?}"),
    });
    assert_eq!(loyalty(&state, grist), Some(1));
    assert_eq!(zone(&state, fodder), Zone::Battlefield);
    assert_eq!(zone(&state, opp_oko), Zone::Battlefield);

    // 2. -2 sacrificing a creature queues reflexive trigger destroying a planeswalker.
    let mut state = plains();
    let grist = cast_grist(&mut state, P0);
    let fodder = token(&mut state, P0, "Black Green Insect Token");
    let opp_pw = cast_tamiyo(&mut state, P1);
    state.active_player = P0;
    state.priority_player = P0;
    activate(&mut state, grist, 1, &[]);
    let mut saw_reflexive_target = false;
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseEffectTargets { legal_targets, .. }
            if legal_targets.contains(&Target::Object(fodder)) =>
        {
            Action::ChooseEffectTarget(Target::Object(fodder))
        }
        Decision::ChooseTargets { legal_targets, .. } => {
            assert!(legal_targets.contains(&Target::Object(opp_pw)));
            saw_reflexive_target = true;
            Action::ChooseTarget(Target::Object(opp_pw))
        }
        other => first_answer(other),
    });
    assert!(saw_reflexive_target);
    assert_eq!(zone(&state, fodder), Zone::Graveyard);
    assert_eq!(zone(&state, opp_pw), Zone::Graveyard);

    // 3. -5 counts creature cards in controller's graveyard, INCLUDING Grist
    //    itself after dying to 0-loyalty SBA before -5 resolves!
    let mut state = plains();
    let grist = cast_grist(&mut state, P0);
    change_loyalty(&mut state, grist, 2);
    assert_eq!(loyalty(&state, grist), Some(5));
    put(&mut state, P0, "Elvish Mystic", Zone::Graveyard);
    put(&mut state, P0, "Llanowar Elves", Zone::Graveyard);
    put(&mut state, P0, "Plains", Zone::Graveyard);
    activate(&mut state, grist, 2, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, grist), Zone::Graveyard);
    assert_eq!(
        life(&state, P1),
        17,
        "-5 drains 3 (Elvish Mystic + Llanowar Elves + Grist via CDA)"
    );
}

// ---- Nissa, Who Shakes the World -----------------------------------------------

#[test]
fn nissa_forest_mana_trigger_works_manually_and_in_automatic_payment() {
    let mut state = plains();
    let _nissa = cast_nissa(&mut state, P0);
    let forest = put(&mut state, P0, "Forest", Zone::Battlefield);
    let taiga = put(&mut state, P0, "Taiga", Zone::Battlefield);
    let arbor = put(&mut state, P0, "Dryad Arbor", Zone::Battlefield);
    let plain = put(&mut state, P0, "Plains", Zone::Battlefield);
    let opp_forest = put(&mut state, P1, "Forest", Zone::Battlefield);

    // Basic Forest -> {G}{G}.
    next(&mut state);
    act(&mut state, Action::ActivateManaAbility(forest));
    assert_eq!(mana(&state, P0, ManaColor::G), 2);
    assert!(state.stack.is_empty(), "triggered mana ability skips stack");

    // Typed dual Taiga tapped for {R} -> adds {R} + {G}.
    act(
        &mut state,
        Action::ActivateManaAbilityChoice(taiga, ManaColor::R),
    );
    assert_eq!(mana(&state, P0, ManaColor::R), 1);
    assert_eq!(mana(&state, P0, ManaColor::G), 3);

    // Dryad Arbor (Forest Creature) -> adds {G}{G}.
    act(&mut state, Action::ActivateManaAbility(arbor));
    assert_eq!(mana(&state, P0, ManaColor::G), 5);

    // Non-Forest land Plains -> adds only {W}.
    act(&mut state, Action::ActivateManaAbility(plain));
    assert_eq!(mana(&state, P0, ManaColor::W), 1);
    assert_eq!(mana(&state, P0, ManaColor::G), 5);

    // Yavimaya, Cradle of Growth makes Plains a Forest too -> now adds {W} + {G}!
    let _yavimaya = put(
        &mut state,
        P0,
        "Yavimaya, Cradle of Growth",
        Zone::Battlefield,
    );
    let island = put(&mut state, P0, "Island", Zone::Battlefield);
    act(
        &mut state,
        Action::ActivateManaAbilityChoice(island, ManaColor::U),
    );
    assert_eq!(mana(&state, P0, ManaColor::U), 1);
    assert_eq!(
        mana(&state, P0, ManaColor::G),
        6,
        "Yavimaya-granted Forest subtype triggers Nissa"
    );

    // Opponent tapping their Forest does NOT trigger P0's Nissa.
    state.priority_player = P1;
    next(&mut state);
    act(&mut state, Action::ActivateManaAbility(opp_forest));
    assert_eq!(mana(&state, P1, ManaColor::G), 1);
    assert_eq!(mana(&state, P0, ManaColor::G), 6);

    // Automatic mana payment tapping a Forest leaves extra {G} floating!
    let mut state = plains();
    let _nissa = cast_nissa(&mut state, P0);
    let forest = put(&mut state, P0, "Forest", Zone::Battlefield);
    let mystic = put(&mut state, P0, "Elvish Mystic", Zone::Hand);
    cast(&mut state, mystic, &[]);
    assert!(state.objects.get(forest).tapped);
    assert_eq!(
        mana(&state, P0, ManaColor::G),
        1,
        "automatic payment tapping Forest triggers Nissa's extra {{G}}"
    );
    settled(&mut state);
    assert_eq!(zone(&state, mystic), Zone::Battlefield);
}

#[test]
fn nissa_plus_one_animates_noncreature_land_permanently_and_allows_zero_targets() {
    assert!(
        !def("Nissa, Who Shakes the World")
            .keywords
            .has(Keywords::INDESTRUCTIBLE),
        "Nissa herself must NOT have printed Indestructible"
    );
    let mut state = plains();
    let nissa = cast_nissa(&mut state, P0);
    let forest = put(&mut state, P0, "Forest", Zone::Battlefield);
    let arbor = put(&mut state, P0, "Dryad Arbor", Zone::Battlefield);
    let opp_forest = put(&mut state, P1, "Forest", Zone::Battlefield);
    state.objects.get_mut(forest).tapped = true;

    let targets = activate_targets(&state, nissa, 0);
    assert_eq!(
        targets,
        vec![Target::Object(forest)],
        "only controlled noncreature lands are legal targets for Nissa +1"
    );
    assert!(!targets.contains(&Target::Object(arbor)));
    assert!(!targets.contains(&Target::Object(opp_forest)));

    // Option 1: choose 0 targets (up to one).
    let mut zero_probe = state.clone();
    act(&mut zero_probe, Action::ActivateAbility(nissa, 0));
    match next(&mut zero_probe) {
        Decision::ChooseTargets {
            can_finish: true, ..
        }
        | Decision::ChooseEffectTargets {
            can_finish: true, ..
        } => act(&mut zero_probe, Action::FinishEffectSelection),
        other => panic!("expected optional target prompt, got {other:?}"),
    }
    settled(&mut zero_probe);
    assert_eq!(loyalty(&zero_probe, nissa), Some(6));
    assert!(zero_probe.objects.get(forest).tapped);
    assert!(!engine::object_has_type(
        &zero_probe,
        forest,
        CardType::Creature
    ));

    // Option 2: target the tapped Forest -> 3 +1/+1 counters, untaps, becomes
    // 0/0 (3/3) Elemental Land Creature with Vigilance + Haste.
    activate(&mut state, nissa, 0, &[Target::Object(forest)]);
    settled(&mut state);
    assert_eq!(loyalty(&state, nissa), Some(6));
    assert!(
        !state.objects.get(forest).tapped,
        "Nissa +1 untaps the land"
    );
    assert_eq!(state.objects.get(forest).counters.plus1_plus1, 3);
    assert_eq!(power_toughness(&state, forest), (3, 3));
    assert!(engine::object_has_type(&state, forest, CardType::Creature));
    assert!(engine::object_has_type(&state, forest, CardType::Land));
    assert!(engine::has_effective_subtype(
        &state,
        forest,
        Subtype::Elemental
    ));
    assert!(engine::has_effective_subtype(
        &state,
        forest,
        Subtype::Forest
    ));
    assert!(keyword(&state, forest, Keywords::VIGILANCE));
    assert!(keyword(&state, forest, Keywords::HASTE));

    // Can attack immediately (haste) and doesn't tap (vigilance), then taps for {G}{G}!
    pass_until(&mut state, |s| s.step == Step::DeclareAttackers);
    act(&mut state, Action::DeclareAttackers(vec![forest]));
    pass_until(&mut state, |s| s.step == Step::Main2);
    assert_eq!(life(&state, P1), 17);
    assert!(
        !state.objects.get(forest).tapped,
        "vigilance kept it untapped"
    );
    act(&mut state, Action::ActivateManaAbility(forest));
    assert_eq!(mana(&state, P0, ManaColor::G), 2);

    // Animation is permanent on later turns, and the land is no longer a noncreature land.
    pass_until(&mut state, |s| {
        s.active_player == P0 && s.step == Step::Main1
    });
    assert!(engine::object_has_type(&state, forest, CardType::Creature));
    assert_eq!(power_toughness(&state, forest), (3, 3));
}

#[test]
fn nissa_minus_eight_emblem_indestructible_persists_and_searches_forests_tapped() {
    let mut state = game(&["Forest", "Taiga", "Island", "Plains"]);
    let lib_forest = state.players[0].library[0];
    let lib_taiga = state.players[0].library[1];
    let lib_island = state.players[0].library[2];

    let nissa = cast_nissa(&mut state, P0);
    change_loyalty(&mut state, nissa, 3);
    assert_eq!(loyalty(&state, nissa), Some(8));
    let opp_librarian = put(&mut state, P1, "Wan Shi Tong, Librarian", Zone::Battlefield);
    let own_plain = put(&mut state, P0, "Plains", Zone::Battlefield);
    let opp_plain = put(&mut state, P1, "Plains", Zone::Battlefield);

    // Activate -8: Nissa dies to 0-loyalty SBA before -8 resolves, yet emblem persists!
    activate(&mut state, nissa, 1, &[]);
    let mut picks = vec![lib_forest, lib_taiga];
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseEffectTargets {
            legal_targets,
            can_finish,
            ..
        } => {
            assert!(can_finish);
            assert!(
                !legal_targets.contains(&Target::Object(lib_island)),
                "non-Forest Island must not be offered"
            );
            if let Some(next_pick) = picks.first().copied() {
                if legal_targets.contains(&Target::Object(next_pick)) {
                    picks.remove(0);
                    return Action::ChooseEffectTarget(Target::Object(next_pick));
                }
            }
            Action::FinishEffectSelection
        }
        other => first_answer(other),
    });

    assert_eq!(zone(&state, nissa), Zone::Graveyard);
    assert_eq!(zone(&state, lib_forest), Zone::Battlefield);
    assert_eq!(zone(&state, lib_taiga), Zone::Battlefield);
    assert!(state.objects.get(lib_forest).tapped);
    assert!(state.objects.get(lib_taiga).tapped);
    assert_eq!(
        state.objects.get(opp_librarian).counters.plus1_plus1,
        1,
        "Nissa -8 library search notifies Wan Shi Tong, Librarian"
    );

    // Emblem grants indestructible to ALL lands P0 controls (not P1's lands).
    assert!(keyword(&state, own_plain, Keywords::INDESTRUCTIBLE));
    assert!(keyword(&state, lib_forest, Keywords::INDESTRUCTIBLE));
    assert!(keyword(&state, lib_taiga, Keywords::INDESTRUCTIBLE));
    assert!(!keyword(&state, opp_plain, Keywords::INDESTRUCTIBLE));

    // Wasteland cannot destroy P0's indestructible Taiga!
    let wasteland = put(&mut state, P1, "Wasteland", Zone::Battlefield);
    state.priority_player = P1;
    activate(&mut state, wasteland, 0, &[Target::Object(lib_taiga)]);
    settled(&mut state);
    assert_eq!(
        zone(&state, lib_taiga),
        Zone::Battlefield,
        "Nissa emblem indestructible saves Taiga from Wasteland"
    );
}

// ---- Tamiyo, Collector of Tales ------------------------------------------------

#[test]
fn tamiyo_static_stops_opponent_discard_and_sacrifice_but_not_own_costs_or_cleanup() {
    let mut state = plains();
    let tamiyo = cast_tamiyo(&mut state, P0);
    let mystic = put(&mut state, P0, "Elvish Mystic", Zone::Battlefield);
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    let extra = put(&mut state, P0, "Ponder", Zone::Hand);

    // Pass to P1's main phase to cast opponent spells/abilities.
    pass_until(&mut state, |s| {
        s.active_player == P1 && s.step == Step::Main1
    });

    // 1. Opponent's Thoughtseize: reveals hand, P1 picks card, P0 keeps it, P1 loses 2 life.
    let seize = put(&mut state, P1, "Thoughtseize", Zone::Hand);
    pool(&mut state, P1, &[ManaColor::B], 0);
    cast(&mut state, seize, &[Target::Player(P0)]);
    settle_choosing(&mut state, bolt);
    assert!(
        hand_contains(&state, P0, bolt),
        "Tamiyo stops Thoughtseize discard"
    );
    assert_eq!(
        life(&state, P1),
        18,
        "Thoughtseize caster still loses 2 life"
    );

    // 2. Opponent's Hymn to Tourach: P0 discards nothing.
    let hymn = put(&mut state, P1, "Hymn to Tourach", Zone::Hand);
    pool(&mut state, P1, &[ManaColor::B, ManaColor::B], 0);
    cast(&mut state, hymn, &[Target::Player(P0)]);
    settled(&mut state);
    assert!(hand_contains(&state, P0, bolt));
    assert!(hand_contains(&state, P0, extra));

    // 3. Opponent's Diabolic Edict & Sheoldred's Edict (creature & planeswalker modes).
    let diabolic = put(&mut state, P1, "Diabolic Edict", Zone::Hand);
    pool(&mut state, P1, &[ManaColor::B], 1);
    cast(&mut state, diabolic, &[Target::Player(P0)]);
    settled(&mut state);
    assert_eq!(zone(&state, mystic), Zone::Battlefield);

    let sheoldred_edict_c = put(&mut state, P1, "Sheoldred's Edict", Zone::Hand);
    pool(&mut state, P1, &[ManaColor::B], 1);
    cast_mode(&mut state, sheoldred_edict_c, 0, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, mystic), Zone::Battlefield);

    let sheoldred_edict_pw = put(&mut state, P1, "Sheoldred's Edict", Zone::Hand);
    pool(&mut state, P1, &[ManaColor::B], 1);
    cast_mode(&mut state, sheoldred_edict_pw, 2, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, tamiyo), Zone::Battlefield);

    // 4. Opponent's Liliana of the Veil +1 (P1 discards, P0 does not) and -2 (no sac).
    let liliana = put(&mut state, P1, "Liliana of the Veil", Zone::Hand);
    pool(&mut state, P1, &[ManaColor::B, ManaColor::B], 1);
    cast(&mut state, liliana, &[]);
    settled(&mut state);
    let p1_card = put(&mut state, P1, "Plains", Zone::Hand);
    let p0_hand_before = state.players[0].hand.len();
    activate(&mut state, liliana, 0, &[]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::Discard { player, .. } => {
            assert_eq!(player, P1, "only Liliana's controller discards");
            Action::Discard(vec![p1_card])
        }
        other => panic!("unexpected {other:?}"),
    });
    assert_eq!(state.players[0].hand.len(), p0_hand_before);

    // Opponent's Liliana of the Veil -2 targeting P0: P0 sacrifices nothing.
    let mut lili_sac_state = plains();
    let _tamiyo_sac = cast_tamiyo(&mut lili_sac_state, P0);
    let mystic_sac = put(&mut lili_sac_state, P0, "Elvish Mystic", Zone::Battlefield);
    pass_until(&mut lili_sac_state, |s| {
        s.active_player == P1 && s.step == Step::Main1
    });
    let lili_sac = put(&mut lili_sac_state, P1, "Liliana of the Veil", Zone::Hand);
    pool(&mut lili_sac_state, P1, &[ManaColor::B, ManaColor::B], 1);
    cast(&mut lili_sac_state, lili_sac, &[]);
    settled(&mut lili_sac_state);
    activate(&mut lili_sac_state, lili_sac, 1, &[Target::Player(P0)]);
    settled(&mut lili_sac_state);
    assert_eq!(
        zone(&lili_sac_state, mystic_sac),
        Zone::Battlefield,
        "Tamiyo stops opponent's Liliana -2 sacrifice"
    );

    // 5. Opponent's Wheel of Fortune: P0 skips discard AND still draws 7!
    let p0_hand_before_wheel = state.players[0].hand.len();
    let wheel = put(&mut state, P1, "Wheel of Fortune", Zone::Hand);
    pool(&mut state, P1, &[ManaColor::R], 2);
    cast(&mut state, wheel, &[]);
    settled(&mut state);
    assert!(hand_contains(&state, P0, bolt));
    assert_eq!(
        state.players[0].hand.len(),
        p0_hand_before_wheel + 7,
        "Tamiyo controller skips opponent's Wheel discard and still draws 7"
    );

    // 6. Opponent's Emrakul (Annihilator 6) attack trigger: P0 sacrifices nothing!
    let emrakul = put(&mut state, P1, "Emrakul, the Aeons Torn", Zone::Battlefield);
    pass_until(&mut state, |s| {
        s.active_player == P1 && s.step == Step::DeclareAttackers
    });
    act(&mut state, Action::DeclareAttackers(vec![emrakul]));
    pass_until(&mut state, |s| {
        s.active_player == P1 && s.step == Step::Main2
    });
    assert_eq!(zone(&state, mystic), Zone::Battlefield);
    assert_eq!(zone(&state, tamiyo), Zone::Battlefield);

    // 7. Opponent's Balance: P0 does not sacrifice creatures or discard cards.
    let mut state = plains();
    let _tamiyo = cast_tamiyo(&mut state, P0);
    let c1 = put(&mut state, P0, "Elvish Mystic", Zone::Battlefield);
    let c2 = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    let h1 = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    let h2 = put(&mut state, P0, "Ponder", Zone::Hand);
    pass_until(&mut state, |s| {
        s.active_player == P1 && s.step == Step::Main1
    });
    state.players[1].hand.clear();
    let balance = put(&mut state, P1, "Balance", Zone::Hand);
    pool(&mut state, P1, &[ManaColor::W], 1);
    cast(&mut state, balance, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, c1), Zone::Battlefield);
    assert_eq!(zone(&state, c2), Zone::Battlefield);
    assert!(hand_contains(&state, P0, h1));
    assert!(hand_contains(&state, P0, h2));

    // 8. Controller's OWN spells (Faithless Looting), COSTS (Polluted Delta,
    //    Wild Mongrel), and CLEANUP discard still happen normally!
    let mut state = plains();
    let _tamiyo = cast_tamiyo(&mut state, P0);
    let looting = put(&mut state, P0, "Faithless Looting", Zone::Hand);
    let delta = put(&mut state, P0, "Polluted Delta", Zone::Battlefield);
    let mongrel = put(&mut state, P0, "Wild Mongrel", Zone::Battlefield);
    pool(&mut state, P0, &[ManaColor::R], 0);
    cast(&mut state, looting, &[]);
    let mut own_discarded = false;
    settle_with(&mut state, |_, decision| match decision {
        Decision::Discard {
            player: P0,
            count,
            choices,
            ..
        } => {
            own_discarded = true;
            Action::Discard(choices.into_iter().take(count as usize).collect())
        }
        other => first_answer(other),
    });
    assert!(
        own_discarded,
        "controller's own Faithless Looting still discards"
    );

    // Fetchland sacrifice cost still works.
    activate(&mut state, delta, 0, &[]);
    settle_first(&mut state);
    assert_eq!(zone(&state, delta), Zone::Graveyard);

    // Wild Mongrel discard cost still works.
    state.players[0].hand.clear();
    let fodder_card = put(&mut state, P0, "Plains", Zone::Hand);
    activate(&mut state, mongrel, 0, &[]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseCostTargets { candidates, .. } if candidates.contains(&fodder_card) => {
            Action::ChooseCostTarget(fodder_card)
        }
        Decision::Discard { choices, .. } if choices.contains(&fodder_card) => {
            Action::Discard(vec![fodder_card])
        }
        other => first_answer(other),
    });
    assert_eq!(zone(&state, fodder_card), Zone::Graveyard);

    // Cleanup step hand-size discard still forces P0 to discard down to 7!
    for _ in 0..8 {
        put(&mut state, P0, "Plains", Zone::Hand);
    }
    let mut saw_cleanup_discard = false;
    run_until(
        &mut state,
        |s| s.active_player == P1 && s.step == Step::Main1,
        |_, decision| match decision {
            Decision::Discard {
                player: P0,
                count,
                choices,
                ..
            } => {
                saw_cleanup_discard = true;
                Some(Action::Discard(
                    choices.iter().copied().take(*count as usize).collect(),
                ))
            }
            _ => None,
        },
    );
    assert!(
        saw_cleanup_discard,
        "cleanup hand-size discard is a turn-based action, not an opponent effect"
    );
    assert_eq!(state.players[0].hand.len(), 7);
}

#[test]
fn tamiyo_plus_one_names_nonland_reveals_four_and_minus_three_regrowths() {
    // Nonland restriction: Plains and Forest are excluded from nameable_nonland().
    let plains_name = def("Plains").object_name;
    let bolt_name = def("Lightning Bolt").object_name;
    assert!(
        !nameable_nonland()
            .iter()
            .flatten()
            .any(|&card_def| { CARD_DEFS[usize::from(card_def)].object_name == plains_name }),
        "lands must not be nameable by Tamiyo +1"
    );
    assert!(nameable_nonland()
        .iter()
        .flatten()
        .any(|&card_def| { CARD_DEFS[usize::from(card_def)].object_name == bolt_name }));

    let mut state = game(&[
        "Lightning Bolt",
        "Plains",
        "Lightning Bolt",
        "Forest",
        "Island",
    ]);
    let bolt1 = state.players[0].library[0];
    let plain = state.players[0].library[1];
    let bolt2 = state.players[0].library[2];
    let forest = state.players[0].library[3];
    let island = state.players[0].library[4];

    let tamiyo = cast_tamiyo(&mut state, P0);
    assert_eq!(loyalty(&state, tamiyo), Some(5));

    // +1 naming Lightning Bolt: both Bolts go to hand (publicly known), Plains & Forest to GY.
    let (group_idx, card_idx) = nonland_name_choice("Lightning Bolt");
    activate(&mut state, tamiyo, 0, &[]);
    let mut step_idx = 0usize;
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseEffectOption { .. } => {
            let pick = if step_idx == 0 { group_idx } else { card_idx };
            step_idx += 1;
            Action::ChooseEffectOption(pick)
        }
        other => panic!("unexpected {other:?}"),
    });
    assert_eq!(loyalty(&state, tamiyo), Some(6));
    assert!(hand_contains(&state, P0, bolt1));
    assert!(hand_contains(&state, P0, bolt2));
    assert_eq!(zone(&state, plain), Zone::Graveyard);
    assert_eq!(zone(&state, forest), Zone::Graveyard);
    assert_eq!(zone(&state, island), Zone::Library);
    // Opponent knows the revealed Bolts put into P0's hand.
    assert!(state
        .known_hand_cards(P1, P0)
        .iter()
        .any(|entry| entry.object == bolt1));
    assert!(state
        .known_hand_cards(P1, P0)
        .iter()
        .any(|entry| entry.object == bolt2));

    // Next turn, -3 returns Plains (any card type!) from P0's graveyard to P0's hand.
    pass_until(&mut state, |s| {
        s.active_player == P0 && s.step == Step::Main1
    });
    activate(&mut state, tamiyo, 1, &[Target::Object(plain)]);
    settled(&mut state);
    assert_eq!(loyalty(&state, tamiyo), Some(3));
    assert!(hand_contains(&state, P0, plain));
}

// ---- Through the Breach & Generalized Splice onto Arcane -----------------------

#[test]
fn through_the_breach_puts_creature_with_haste_sacrifices_at_end_step_and_works_with_grist() {
    // 1. Putting Emrakul onto battlefield with haste, attacking, sacrificing at end step.
    let mut state = plains();
    let breach = put(&mut state, P0, "Through the Breach", Zone::Hand);
    let emrakul = put(&mut state, P0, "Emrakul, the Aeons Torn", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 4);
    cast(&mut state, breach, &[]);
    settle_choosing(&mut state, emrakul);
    assert_eq!(zone(&state, emrakul), Zone::Battlefield);
    assert!(keyword(&state, emrakul, Keywords::HASTE));

    // Can attack immediately on the turn it entered!
    pass_until(&mut state, |s| s.step == Step::DeclareAttackers);
    act(&mut state, Action::DeclareAttackers(vec![emrakul]));
    pass_until(&mut state, |s| s.step == Step::Main2);
    assert_eq!(life(&state, P1), 5);

    // At the beginning of the next end step, Emrakul is sacrificed!
    pass_until(&mut state, |s| {
        s.active_player == P1 && s.step == Step::Main1
    });
    assert_ne!(
        zone(&state, emrakul),
        Zone::Battlefield,
        "Through the Breach sacrifices the creature at beginning of next end step"
    );

    // 2. Declining the optional hand choice leaves creature in hand.
    let mut state = plains();
    let breach = put(&mut state, P0, "Through the Breach", Zone::Hand);
    let sheoldred = put(&mut state, P0, "Sheoldred, the Apocalypse", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 4);
    cast(&mut state, breach, &[]);
    settle_first(&mut state);
    assert!(hand_contains(&state, P0, sheoldred));

    // 3. Through the Breach with Grist in hand (CDA creature card!): enters as
    //    3-loyalty Planeswalker, activates +1, still sacrificed at end step!
    let mut state = plains();
    let breach = put(&mut state, P0, "Through the Breach", Zone::Hand);
    let grist = put(&mut state, P0, "Grist, the Hunger Tide", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 4);
    cast(&mut state, breach, &[]);
    settle_choosing(&mut state, grist);
    assert_eq!(zone(&state, grist), Zone::Battlefield);
    assert_eq!(loyalty(&state, grist), Some(3));
    assert!(!engine::object_has_type(&state, grist, CardType::Creature));
    activate(&mut state, grist, 0, &[]);
    settled(&mut state);
    assert_eq!(named(&state, P0, "Black Green Insect Token").len(), 1);
    pass_until(&mut state, |s| {
        s.active_player == P1 && s.step == Step::Main1
    });
    assert_eq!(
        zone(&state, grist),
        Zone::Graveyard,
        "Grist put in via Through the Breach is sacrificed at next end step"
    );
}

#[test]
fn generalized_splice_onto_arcane_supports_ritual_and_breach_in_all_combinations() {
    // 1. Casting Through the Breach with no other splice card NEVER offers splice.
    let mut state = plains();
    let breach = put(&mut state, P0, "Through the Breach", Zone::Hand);
    let creature = put(&mut state, P0, "Sheoldred, the Apocalypse", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R; 4], 6);
    act(&mut state, Action::CastSpell(breach));
    assert!(
        matches!(
            next(&mut state),
            Decision::CastSpellOrPass { player: P0, .. }
        ),
        "Through the Breach cannot splice onto itself"
    );
    settle_choosing(&mut state, creature);
    assert_eq!(zone(&state, creature), Zone::Battlefield);

    // 2. Splicing Through the Breach ({2}{R}{R}) onto Desperate Ritual ({1}{R}).
    let mut state = plains();
    let ritual = put(&mut state, P0, "Desperate Ritual", Zone::Hand);
    let breach = put(&mut state, P0, "Through the Breach", Zone::Hand);
    let sheoldred = put(&mut state, P0, "Sheoldred, the Apocalypse", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R; 3], 3);
    act(&mut state, Action::CastSpell(ritual));
    match next(&mut state) {
        Decision::ChooseEffectOption { option_count, .. } => assert_eq!(option_count, 2),
        other => panic!("expected 2 splice options (none or Breach), got {other:?}"),
    }
    act(&mut state, Action::ChooseEffectOption(1));
    assert!(matches!(
        next(&mut state),
        Decision::CastSpellOrPass { player: P0, .. }
    ));
    assert!(
        hand_contains(&state, P0, breach),
        "spliced Breach stays in hand"
    );
    assert!(
        state
            .known_hand_cards(P1, P0)
            .iter()
            .any(|entry| entry.object == breach),
        "spliced Breach is revealed to opponent"
    );
    settle_choosing(&mut state, sheoldred);
    assert_eq!(
        mana(&state, P0, ManaColor::R),
        3,
        "Desperate Ritual added RRR"
    );
    assert_eq!(zone(&state, sheoldred), Zone::Battlefield);
    assert!(keyword(&state, sheoldred, Keywords::HASTE));
    pass_until(&mut state, |s| {
        s.active_player == P1 && s.step == Step::Main1
    });
    assert_eq!(
        zone(&state, sheoldred),
        Zone::Graveyard,
        "spliced Breach sacrifices the creature at end step"
    );

    // 3. Splicing Desperate Ritual ({1}{R}) onto Through the Breach ({4}{R}).
    let mut state = plains();
    let breach = put(&mut state, P0, "Through the Breach", Zone::Hand);
    let ritual = put(&mut state, P0, "Desperate Ritual", Zone::Hand);
    let sheoldred = put(&mut state, P0, "Sheoldred, the Apocalypse", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R; 2], 5);
    act(&mut state, Action::CastSpell(breach));
    match next(&mut state) {
        Decision::ChooseEffectOption { option_count, .. } => assert_eq!(option_count, 2),
        other => panic!("expected 2 splice options (none or Ritual), got {other:?}"),
    }
    act(&mut state, Action::ChooseEffectOption(1));
    assert!(hand_contains(&state, P0, ritual));
    settle_choosing(&mut state, sheoldred);
    assert_eq!(zone(&state, sheoldred), Zone::Battlefield);
    assert_eq!(
        mana(&state, P0, ManaColor::R),
        3,
        "spliced Ritual added RRR"
    );

    // 4. Splicing a second Through the Breach onto Through the Breach -> TWO creatures!
    let mut state = plains();
    let breach1 = put(&mut state, P0, "Through the Breach", Zone::Hand);
    let breach2 = put(&mut state, P0, "Through the Breach", Zone::Hand);
    let c1 = put(&mut state, P0, "Sheoldred, the Apocalypse", Zone::Hand);
    let c2 = put(&mut state, P0, "Elvish Mystic", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R; 3], 6);
    act(&mut state, Action::CastSpell(breach1));
    match next(&mut state) {
        Decision::ChooseEffectOption { option_count, .. } => assert_eq!(option_count, 2),
        other => panic!("expected 2 splice options, got {other:?}"),
    }
    act(&mut state, Action::ChooseEffectOption(1));
    assert!(hand_contains(&state, P0, breach2));
    let mut to_put = vec![c1, c2];
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseEffectTargets { legal_targets, .. } => {
            let pick = to_put.remove(0);
            assert!(legal_targets.contains(&Target::Object(pick)));
            Action::ChooseEffectTarget(Target::Object(pick))
        }
        other => first_answer(other),
    });
    assert_eq!(zone(&state, c1), Zone::Battlefield);
    assert_eq!(zone(&state, c2), Zone::Battlefield);
    assert!(keyword(&state, c1, Keywords::HASTE));
    assert!(keyword(&state, c2, Keywords::HASTE));
    pass_until(&mut state, |s| {
        s.active_player == P1 && s.step == Step::Main1
    });
    assert_eq!(zone(&state, c1), Zone::Graveyard);
    assert_eq!(zone(&state, c2), Zone::Graveyard);

    // 5. Splicing BOTH Desperate Ritual AND Through the Breach onto Ethereal Haze!
    let mut state = plains();
    let haze = put(&mut state, P0, "Ethereal Haze", Zone::Hand);
    let ritual = put(&mut state, P0, "Desperate Ritual", Zone::Hand);
    let breach = put(&mut state, P0, "Through the Breach", Zone::Hand);
    let sheoldred = put(&mut state, P0, "Sheoldred, the Apocalypse", Zone::Hand);
    pool(
        &mut state,
        P0,
        &[ManaColor::W, ManaColor::R, ManaColor::R, ManaColor::R],
        3,
    );
    act(&mut state, Action::CastSpell(haze));
    match next(&mut state) {
        Decision::ChooseEffectOption { option_count, .. } => assert_eq!(option_count, 4),
        other => panic!("expected 4 splice combos (0, R, B, R+B), got {other:?}"),
    }
    act(&mut state, Action::ChooseEffectOption(3));
    assert!(hand_contains(&state, P0, ritual));
    assert!(hand_contains(&state, P0, breach));
    settle_choosing(&mut state, sheoldred);
    assert_eq!(mana(&state, P0, ManaColor::R), 3);
    assert_eq!(zone(&state, sheoldred), Zone::Battlefield);
    assert!(keyword(&state, sheoldred, Keywords::HASTE));
}

// ---- Public Sessions -----------------------------------------------------------

#[derive(Default)]
struct C9cEvents {
    casts: [usize; C9C_CARDS.len()],
}

impl C9cEvents {
    fn observe(&mut self, _decision: &Value, action: &Value) {
        let semantic = &action["semantic"];
        if semantic["action_kind"].as_str() != Some("cast_spell") {
            return;
        }
        let Some(card) = semantic["source"]["card_db_id"]
            .as_u64()
            .and_then(|id| usize::try_from(id).ok())
            .and_then(|id| CARD_DEFS.get(id))
        else {
            return;
        };
        if let Some(index) = C9C_CARDS.iter().position(|name| *name == card.name) {
            self.casts[index] += 1;
        }
    }
}

#[test]
fn c9c_decks_finish_and_replay_through_public_sessions_both_seats() {
    let bant_walkers = deck(&[
        ("Savannah", 8),
        ("Tropical Island", 8),
        ("Tundra", 4),
        ("Sol Ring", 4),
        ("Mox Emerald", 4),
        ("Mox Pearl", 4),
        ("Mox Sapphire", 4),
        ("Elspeth, Storm Slayer", 6),
        ("Nissa, Who Shakes the World", 6),
        ("Tamiyo, Collector of Tales", 6),
        ("Elvish Mystic", 4),
        ("Lightning Bolt", 2),
    ]);
    let jund_breach = deck(&[
        ("Bayou", 8),
        ("Taiga", 8),
        ("Mountain", 4),
        ("Sol Ring", 4),
        ("Mox Ruby", 4),
        ("Mox Jet", 4),
        ("Mox Emerald", 4),
        ("Grist, the Hunger Tide", 6),
        ("Through the Breach", 6),
        ("Desperate Ritual", 4),
        ("Wirewood Symbiote", 4),
        ("Sheoldred, the Apocalypse", 4),
    ]);
    let mut events = C9cEvents::default();
    // 2 seats x 3 seeds x 2 policies x (original + replay) = 24 games total.
    for pair in [
        [bant_walkers.clone(), jund_breach.clone()],
        [jund_breach, bant_walkers],
    ] {
        for seed in 0x4339_6300_u64..0x4339_6303 {
            for policy in [Policy::Ranked, Policy::Random(seed + 0x100)] {
                let played =
                    play_observed(pair.clone(), None, seed, policy, &mut |decision, action| {
                        events.observe(decision, action);
                    });
                let replay = play_observed(pair.clone(), None, seed, policy, &mut |_, _| {});
                assert_eq!(played.transcript_sha256, replay.transcript_sha256);
            }
        }
    }
    for (name, casts) in C9C_CARDS.into_iter().zip(events.casts) {
        assert!(casts > 0, "{name} was never cast in a public session");
    }
}
