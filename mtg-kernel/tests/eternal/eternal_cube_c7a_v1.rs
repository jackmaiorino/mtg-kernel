//! Vintage Cube wave C7a: native rules and public-session tests for the
//! partial promotions, deferred C6 cards, and Brain Freeze Storm fix.
#![cfg(feature = "eternal-fixtures")]
#![allow(dead_code)]

use crate::eternal_catalog_v1::{play_observed, Policy};
use mtg_kernel::card_def::{
    card_id_by_name, CardCapability, CardDef, CardType, Keywords, Subtype, Supertype, CARD_DEFS,
};
use mtg_kernel::engine::{self, Action, Decision, OptionalCostChoice};
use mtg_kernel::eternal_cube_c7a_v1::chorus_counters;
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::limited_session_v1::{CustomCardCountV1, CustomDeckV1};
use mtg_kernel::mana::{ManaColor, Pip};
use mtg_kernel::planeswalker_v1::{change_loyalty, loyalty};
use mtg_kernel::policy_surface_v5::PolicySurfaceV5;
use mtg_kernel::rl::{observe_policy_v5, observe_v2};
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Target, Zone};
use mtg_kernel::surface_v2::HarnessSurfaceV2;
use serde_json::Value;

const P0: PlayerId = PlayerId::P0;
const P1: PlayerId = PlayerId::P1;

const C7A_CARDS: [&str; 7] = [
    "Hullbreacher",
    "Gandalf, Goblins' Bane",
    "Headliner Scarlett",
    "Metamorphosis Fanatic",
    "Inti, Seneschal of the Sun",
    "Malcolm, Alluring Scoundrel",
    "Chandra, Torch of Defiance",
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
        0x4355_4245_0000 + 0x3761,
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

fn mana(state: &GameState, player: PlayerId, color: ManaColor) -> u8 {
    state.players[player.index()].mana_pool[color.pool_index()]
}

fn life(state: &GameState, player: PlayerId) -> i32 {
    state.players[player.index()].life
}

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

// ---- Characteristics -----------------------------------------------------------

#[test]
fn c7a_cards_are_full() {
    for name in C7A_CARDS {
        let card = def(name);
        assert_eq!(card.capability, CardCapability::Full, "{name}");
        assert!(card.is_executable(), "{name}");
        assert!(card.has_full_support(), "{name}");
    }
    let scarlett = def("Headliner Scarlett");
    assert!(scarlett.keywords.has(Keywords::HASTE));
    assert_eq!((scarlett.power, scarlett.toughness), (Some(3), Some(3)));
    assert_eq!(scarlett.supertypes, &[Supertype::Legendary]);

    let gandalf = def("Gandalf, Goblins' Bane");
    assert_eq!((gandalf.power, gandalf.toughness), (Some(2), Some(3)));
    assert!(gandalf.subtypes.contains(&Subtype::Wizard));
    let adventure = gandalf
        .adventure
        .as_ref()
        .expect("Gandalf has Flameshape adventure");
    assert_eq!(adventure.name, "Flameshape");
    assert_eq!(adventure.cost.generic, 1);
    assert_eq!(adventure.cost.pips, &[Pip::Colored(ManaColor::R)]);
    assert_eq!(adventure.types, &[CardType::Sorcery]);

    let inti = def("Inti, Seneschal of the Sun");
    assert_eq!((inti.power, inti.toughness), (Some(2), Some(2)));
    assert_eq!(inti.cost.generic, 1);
    assert_eq!(inti.cost.pips, &[Pip::Colored(ManaColor::R)]);

    let malcolm = def("Malcolm, Alluring Scoundrel");
    assert!(malcolm.keywords.has(Keywords::FLASH));
    assert!(malcolm.keywords.has(Keywords::FLYING));
    assert_eq!((malcolm.power, malcolm.toughness), (Some(2), Some(1)));

    let chandra = def("Chandra, Torch of Defiance");
    assert_eq!(chandra.types, &[CardType::Planeswalker]);
    assert_eq!(chandra.subtypes, &[Subtype::Chandra]);
    assert_eq!(chandra.starting_loyalty, Some(4));
    assert_eq!(chandra.cost.generic, 2);
    assert_eq!(chandra.cost.pips, &[Pip::Colored(ManaColor::R); 2]);
    assert_eq!(chandra.activated_abilities.len(), 4);
}

// ---- Brain Freeze graveyard Storm fix (#710) -----------------------------------

#[test]
fn brain_freeze_flashback_via_snapcaster_resolves_storm_copies_for_self_and_opponent() {
    for target_player in [P1, P0] {
        let mut state = game(&["Island"; 24]);
        let freeze = put(&mut state, P0, "Brain Freeze", Zone::Graveyard);
        let petal = put(&mut state, P0, "Lotus Petal", Zone::Hand);
        cast(&mut state, petal, &[]);
        settled(&mut state);

        let snap = put(&mut state, P0, "Snapcaster Mage", Zone::Hand);
        pool(&mut state, P0, &[ManaColor::U], 1);
        cast(&mut state, snap, &[]);
        assert!(matches!(
            settle(&mut state),
            Some(Decision::ChooseTargets { .. })
        ));
        act(&mut state, Action::ChooseTarget(Target::Object(freeze)));
        settled(&mut state);
        assert_eq!(state.players[0].spells_cast_this_turn, 2);

        // Flash back Brain Freeze via Snapcaster Mage's granted Flashback.
        // Storm count is 2 (Lotus Petal + Snapcaster Mage) -> 3 total resolutions = 9 milled.
        pool(&mut state, P0, &[ManaColor::U], 1);
        cast(&mut state, freeze, &[Target::Player(target_player)]);
        let saved = serde_json::to_vec(&state).unwrap();
        let mut restored: GameState = serde_json::from_slice(&saved).unwrap();
        let milled_before = state.players[target_player.index()].graveyard.len();
        for branch in [&mut state, &mut restored] {
            settle_with(branch, |_, decision| match decision {
                Decision::ChooseTargets { .. } => {
                    Action::ChooseTarget(Target::Player(target_player))
                }
                other => first_answer(other),
            });
            assert_eq!(zone(branch, freeze), Zone::Exile);
            assert_eq!(
                branch.players[target_player.index()].graveyard.len(),
                milled_before + 9
            );
        }
        assert_eq!(state.state_hash(), restored.state_hash());
    }
}

#[test]
fn brain_freeze_escape_via_underworld_breach_and_yawgmoths_will_resolve_storm_copies() {
    // 1. Escape via Underworld Breach (GraveyardEscape).
    let mut state = game(&["Island"; 20]);
    put(&mut state, P0, "Underworld Breach", Zone::Battlefield);
    let freeze = put(&mut state, P0, "Brain Freeze", Zone::Graveyard);
    for _ in 0..3 {
        put(&mut state, P0, "Plains", Zone::Graveyard);
    }
    let petal = put(&mut state, P0, "Lotus Petal", Zone::Hand);
    cast(&mut state, petal, &[]);
    settled(&mut state);

    pool(&mut state, P0, &[ManaColor::U], 1);
    assert!(castable(&mut state, freeze));
    act(&mut state, Action::CastSpell(freeze));
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseTargets { .. } => Action::ChooseTarget(Target::Player(P1)),
        other => first_answer(other),
    });
    assert_eq!(zone(&state, freeze), Zone::Graveyard);
    assert_eq!(
        state.players[1].graveyard.len(),
        6,
        "original + 1 Storm copy"
    );

    // 2. GraveyardPermissionV1 via Yawgmoth's Will.
    let mut will_state = game(&["Island"; 20]);
    let freeze2 = put(&mut will_state, P0, "Brain Freeze", Zone::Graveyard);
    let will = put(&mut will_state, P0, "Yawgmoth's Will", Zone::Hand);
    pool(&mut will_state, P0, &[ManaColor::B], 2);
    cast(&mut will_state, will, &[]);
    settled(&mut will_state);
    pool(&mut will_state, P0, &[ManaColor::U], 1);
    cast(&mut will_state, freeze2, &[Target::Player(P1)]);
    settle_with(&mut will_state, |_, decision| match decision {
        Decision::ChooseTargets { .. } => Action::ChooseTarget(Target::Player(P1)),
        other => first_answer(other),
    });
    assert_eq!(
        will_state.players[1].graveyard.len(),
        6,
        "original + 1 Storm copy from Yawgmoth's Will"
    );
}

// ---- Hullbreacher audited Eternal draw leaves ----------------------------------

#[test]
fn hullbreacher_competes_with_jace_on_audited_eternal_draw_leaves() {
    // Ice half of Fire // Ice (P1aOpV1::IceV1) on empty library with Jace + Hullbreacher.
    let mut state = game(&[]);
    let jace = put(&mut state, P0, "Jace, Wielder of Mysteries", Zone::Hand);
    mtg_kernel::event::propose_and_commit(
        &mut state,
        mtg_kernel::event::ProposedEvent::zone_change(jace, Zone::Battlefield),
    );
    assert_eq!(loyalty(&state, jace), Some(4));
    put(&mut state, P1, "Hullbreacher", Zone::Battlefield);
    let target = put(&mut state, P1, "Plains", Zone::Battlefield);
    let fire_ice = put(&mut state, P0, "Fire // Ice", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 1);
    cast_mode(&mut state, fire_ice, 1, &[Target::Object(target)]);
    let choice = settle(&mut state).expect("Ice draw must offer Jace vs Hullbreacher");
    assert!(matches!(
        choice,
        Decision::ChooseEffectTargets {
            player: P0,
            can_finish: true,
            ref legal_targets,
            ..
        } if legal_targets == &[Target::Object(jace)]
    ));

    // Choosing Jace wins the game; declining lets Hullbreacher create a Treasure.
    let mut jace_branch = state.clone();
    act(
        &mut jace_branch,
        Action::ChooseEffectTarget(Target::Object(jace)),
    );
    assert!(matches!(
        next(&mut jace_branch),
        Decision::GameOver { winner: Some(P0) }
    ));

    let mut hull_branch = state;
    act(&mut hull_branch, Action::FinishEffectSelection);
    settled(&mut hull_branch);
    assert_eq!(named(&hull_branch, P1, "Treasure Token").len(), 1);
}

// ---- Headliner Scarlett & Gandalf concealed exile privacy ----------------------

#[test]
fn headliner_scarlett_haste_etb_cant_block_and_upkeep_concealed_exile() {
    let mut state = game(&[
        "Mountain",
        "Lightning Bolt",
        "Plains",
        "Plains",
        "Plains",
        "Plains",
        "Plains",
        "Plains",
    ]);
    let blocker = put(&mut state, P1, "Wall of Blossoms", Zone::Battlefield);
    let scarlett = put(&mut state, P0, "Headliner Scarlett", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 3);
    cast(&mut state, scarlett, &[]);
    assert!(matches!(
        settle(&mut state),
        Some(Decision::ChooseTargets { .. })
    ));
    act(&mut state, Action::ChooseTarget(Target::Player(P1)));
    settled(&mut state);
    assert!(keyword(&state, scarlett, Keywords::HASTE));

    // Newly entered creature controlled by P1 also cannot block this turn.
    let late_blocker = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    let blockers = legal_blockers(&mut state, scarlett);
    assert!(!blockers.contains(&blocker));
    assert!(!blockers.contains(&late_blocker));

    // Next turn, P0's upkeep exiles top of library (Mountain) face down; P0 can play it.
    let top_land = state.players[0].library[0];
    assert_eq!(
        CARD_DEFS[state.objects.get(top_land).card_def as usize].name,
        "Mountain"
    );
    pass_until(&mut state, |s| {
        s.active_player == P0 && s.step == Step::Main1
    });
    assert_eq!(zone(&state, top_land), Zone::Exile);
    assert!(state.objects.get(top_land).v4.face_down_v1.is_some());
    assert!(playable_land(&mut state, top_land));
    act(&mut state, Action::PlayLand(top_land));
    settled(&mut state);
    assert_eq!(zone(&state, top_land), Zone::Battlefield);
    assert!(state.objects.get(top_land).v4.face_down_v1.is_none());

    // Blocking restriction ended on the new turn.
    let blockers_next = legal_blockers(&mut state, scarlett);
    assert!(blockers_next.contains(&blocker));

    // Following P0 upkeep exiles Lightning Bolt (after drawing 1 card in Draw step? Check top!).
    let next_upkeep_card = put(&mut state, P0, "Lightning Bolt", Zone::Library);
    pass_until(&mut state, |s| {
        s.active_player == P0 && s.step == Step::Main1
    });
    assert_eq!(zone(&state, next_upkeep_card), Zone::Exile);
    pool(&mut state, P0, &[ManaColor::R], 0);
    assert!(castable(&mut state, next_upkeep_card));
    // Leave it uncast until next turn: permission expires at end of turn.
    pass_until(&mut state, |s| {
        s.active_player == P1 && s.step == Step::Main1
    });
    pass_until(&mut state, |s| {
        s.active_player == P0 && s.step == Step::Main1
    });
    pool(&mut state, P0, &[ManaColor::R], 0);
    assert!(!castable(&mut state, next_upkeep_card));
}

#[test]
fn concealed_exile_play_permissions_hide_land_vs_spell_from_opponent_in_v2_and_v5() {
    // Verify BOTH Headliner Scarlett and Gandalf's Flameshape produce
    // bit-identical opponent ObservationV2 and ObservationV5 when exiling a
    // land (Mountain) versus a spell (Lightning Bolt) face down.
    let run_scarlett = |top_card: &str| {
        let mut state = game(&["Plains"; 10]);
        put(&mut state, P0, "Headliner Scarlett", Zone::Battlefield);
        let card = put(&mut state, P0, top_card, Zone::Library);
        // Advance through P1's turn into P0's next Main1 so Scarlett's upkeep
        // trigger exiles `card` face down and its EndOfTurn permission is live.
        pass_until(&mut state, |s| {
            s.active_player == P1 && s.step == Step::Main1
        });
        pass_until(&mut state, |s| {
            s.active_player == P0 && s.step == Step::Main1 && s.priority_player == P0
        });
        assert_eq!(zone(&state, card), Zone::Exile);
        (state, card)
    };
    let (land_state, land_card) = run_scarlett("Mountain");
    let (spell_state, spell_card) = run_scarlett("Lightning Bolt");

    let surface_v2 = HarnessSurfaceV2::new();
    let surface_v5 = PolicySurfaceV5::new();

    // Holder knows exact identity.
    let owner_land = observe_v2(&land_state, &surface_v2, P0, 0).unwrap();
    let owner_spell = observe_v2(&spell_state, &surface_v2, P0, 0).unwrap();
    assert_eq!(owner_land.known_face_down_cards.len(), 1);
    assert_eq!(
        owner_land.known_face_down_cards[0].card_db_id,
        id("Mountain")
    );
    assert_eq!(owner_spell.known_face_down_cards.len(), 1);
    assert_eq!(
        owner_spell.known_face_down_cards[0].card_db_id,
        id("Lightning Bolt")
    );
    assert_eq!(land_card, spell_card);

    // Opponent cannot distinguish land vs spell in ObservationV2 OR ObservationV5.
    let opp_land_v2 = observe_v2(&land_state, &surface_v2, P1, 0).unwrap();
    let opp_spell_v2 = observe_v2(&spell_state, &surface_v2, P1, 0).unwrap();
    assert!(opp_land_v2.known_face_down_cards.is_empty());
    assert!(opp_spell_v2.known_face_down_cards.is_empty());
    assert_eq!(
        opp_land_v2.visible_projection_hash,
        opp_spell_v2.visible_projection_hash
    );
    assert_eq!(
        serde_json::to_string(&opp_land_v2).unwrap(),
        serde_json::to_string(&opp_spell_v2).unwrap()
    );

    let opp_land_v5 = observe_policy_v5(&land_state, &surface_v5, P1, 0, 0, 0, 1).unwrap();
    let opp_spell_v5 = observe_policy_v5(&spell_state, &surface_v5, P1, 0, 0, 0, 1).unwrap();
    assert_eq!(
        opp_land_v5.visible_projection_hash,
        opp_spell_v5.visible_projection_hash
    );
    assert_eq!(
        serde_json::to_string(&opp_land_v5).unwrap(),
        serde_json::to_string(&opp_spell_v5).unwrap()
    );

    // Repeat the same indistinguishability check for Gandalf's Flameshape (2 cards).
    let run_flameshape = |top_two: [&str; 2]| {
        let mut state = game(&["Plains"; 10]);
        put(&mut state, P0, top_two[1], Zone::Library);
        put(&mut state, P0, top_two[0], Zone::Library);
        let gandalf = put(&mut state, P0, "Gandalf, Goblins' Bane", Zone::Hand);
        pool(&mut state, P0, &[ManaColor::R], 1);
        cast_mode(&mut state, gandalf, 1, &[]);
        settled(&mut state);
        state
    };
    let flame_lands = run_flameshape(["Mountain", "Island"]);
    let flame_spells = run_flameshape(["Lightning Bolt", "Opt"]);

    let f_opp_land_v2 = observe_v2(&flame_lands, &surface_v2, P1, 0).unwrap();
    let f_opp_spell_v2 = observe_v2(&flame_spells, &surface_v2, P1, 0).unwrap();
    assert!(f_opp_land_v2.known_face_down_cards.is_empty());
    assert_eq!(
        f_opp_land_v2.visible_projection_hash,
        f_opp_spell_v2.visible_projection_hash
    );
    assert_eq!(
        serde_json::to_string(&f_opp_land_v2).unwrap(),
        serde_json::to_string(&f_opp_spell_v2).unwrap()
    );

    let f_opp_land_v5 = observe_policy_v5(&flame_lands, &surface_v5, P1, 0, 0, 0, 1).unwrap();
    let f_opp_spell_v5 = observe_policy_v5(&flame_spells, &surface_v5, P1, 0, 0, 0, 1).unwrap();
    assert_eq!(
        f_opp_land_v5.visible_projection_hash,
        f_opp_spell_v5.visible_projection_hash
    );
    assert_eq!(
        serde_json::to_string(&f_opp_land_v5).unwrap(),
        serde_json::to_string(&f_opp_spell_v5).unwrap()
    );
}

#[test]
fn gandalf_flameshape_wizard_gated_unbounded_play_and_front_face_trigger() {
    let mut state = game(&["Plains"; 10]);
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Library);
    let mountain = put(&mut state, P0, "Mountain", Zone::Library);
    let gandalf = put(&mut state, P0, "Gandalf, Goblins' Bane", Zone::Hand);

    // Cast Flameshape ({1}{R} Sorcery Adventure).
    pool(&mut state, P0, &[ManaColor::R], 1);
    cast_mode(&mut state, gandalf, 1, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, gandalf), Zone::Exile);
    assert!(state.objects.get(gandalf).v4.on_adventure);
    assert_eq!(zone(&state, mountain), Zone::Exile);
    assert_eq!(zone(&state, bolt), Zone::Exile);

    // Without a Wizard, neither exiled card may be played.
    pool(&mut state, P0, &[ManaColor::R], 0);
    assert!(!playable_land(&mut state, mountain));
    assert!(!castable(&mut state, bolt));

    // Across turns the permission does NOT expire.
    pass_until(&mut state, |s| {
        s.active_player == P1 && s.step == Step::Main1
    });
    pass_until(&mut state, |s| {
        s.active_player == P0 && s.step == Step::Main1
    });
    pool(&mut state, P0, &[ManaColor::R], 0);
    assert!(!playable_land(&mut state, mountain));
    assert!(!castable(&mut state, bolt));

    // Cast Gandalf (a Wizard!) from adventure exile -> both exiled cards unlock.
    pool(&mut state, P0, &[ManaColor::R], 2);
    cast(&mut state, gandalf, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, gandalf), Zone::Battlefield);
    assert!(playable_land(&mut state, mountain));
    act(&mut state, Action::PlayLand(mountain));
    settled(&mut state);
    assert_eq!(zone(&state, mountain), Zone::Battlefield);

    // Cast the exiled Lightning Bolt -> triggers Gandalf's front-face ability (+1/+1 EOT and 1 damage to P1).
    pool(&mut state, P0, &[ManaColor::R], 0);
    assert!(castable(&mut state, bolt));
    let life_before = life(&state, P1);
    cast(&mut state, bolt, &[Target::Player(P1)]);
    settled(&mut state);
    assert_eq!(power_toughness(&state, gandalf), (3, 4));
    assert_eq!(
        life(&state, P1),
        life_before - 4,
        "3 from Bolt + 1 from Gandalf"
    );

    // Casting a creature spell does NOT trigger Gandalf.
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 0);
    cast(&mut state, elves, &[]);
    settled(&mut state);
    assert_eq!(power_toughness(&state, gandalf), (3, 4));
    assert_eq!(life(&state, P1), life_before - 4);
}

// ---- Metamorphosis Fanatic -----------------------------------------------------

#[test]
fn metamorphosis_fanatic_entry_time_lifelink_counter_and_metamorph_copy_self() {
    // 1. Entry-time lifelink counter is present immediately as the returned
    //    creature enters so its ETB damage (Orcish Bowmasters) gains life.
    let mut state = plains();
    let bowmasters = put(&mut state, P0, "Orcish Bowmasters", Zone::Graveyard);
    let fanatic = put(&mut state, P0, "Metamorphosis Fanatic", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B, ManaColor::B], 4);
    cast(&mut state, fanatic, &[]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseTargets { legal_targets, .. }
            if legal_targets.contains(&Target::Player(P1)) =>
        {
            Action::ChooseTarget(Target::Player(P1))
        }
        Decision::ChooseTargets { legal_targets, .. }
            if legal_targets.contains(&Target::Object(bowmasters)) =>
        {
            Action::ChooseTarget(Target::Object(bowmasters))
        }
        other => first_answer(other),
    });
    assert_eq!(zone(&state, bowmasters), Zone::Battlefield);
    assert_eq!(
        state.objects.get(bowmasters).v4.lifelink_keyword_counters,
        1
    );
    assert!(keyword(&state, bowmasters, Keywords::LIFELINK));
    assert_eq!(life(&state, P0), 21, "Bowmasters ETB damage gained 1 life");
    assert_eq!(life(&state, P1), 19);

    // 2. Returning Phyrexian Metamorph and copying Fanatic itself triggers a
    //    second Fanatic ETB that returns a third creature with a lifelink counter.
    let mut chain = plains();
    let metamorph = put(&mut chain, P0, "Phyrexian Metamorph", Zone::Graveyard);
    let wall = put(&mut chain, P0, "Wall of Blossoms", Zone::Graveyard);
    let fanatic2 = put(&mut chain, P0, "Metamorphosis Fanatic", Zone::Hand);
    pool(&mut chain, P0, &[ManaColor::B, ManaColor::B], 4);
    cast(&mut chain, fanatic2, &[]);
    assert!(matches!(
        settle(&mut chain),
        Some(Decision::ChooseTargets { .. })
    ));
    act(&mut chain, Action::ChooseTarget(Target::Object(metamorph)));
    let copy_choice = settle(&mut chain).expect("Metamorph offers copy choice");
    assert!(matches!(
        copy_choice,
        Decision::ChooseEffectTargets {
            player: P0,
            min_targets: 0,
            max_targets: 1,
            ref legal_targets,
            ..
        } if legal_targets.contains(&Target::Object(fanatic2))
    ));
    act(
        &mut chain,
        Action::ChooseEffectTarget(Target::Object(fanatic2)),
    );
    assert!(matches!(
        settle(&mut chain),
        Some(Decision::ChooseTargets { .. })
    ));
    act(&mut chain, Action::ChooseTarget(Target::Object(wall)));
    settled(&mut chain);
    assert_eq!(zone(&chain, metamorph), Zone::Battlefield);
    assert_eq!(
        chain.objects.get(metamorph).card_def,
        id("Metamorphosis Fanatic")
    );
    assert!(engine::object_has_type(
        &chain,
        metamorph,
        CardType::Artifact
    ));
    assert_eq!(chain.objects.get(metamorph).v4.lifelink_keyword_counters, 1);
    assert_eq!(zone(&chain, wall), Zone::Battlefield);
    assert_eq!(chain.objects.get(wall).v4.lifelink_keyword_counters, 1);
    assert!(keyword(&chain, wall, Keywords::LIFELINK));

    // 3. Returning Phyrexian Metamorph and declining to copy -> enters as 0/0 and dies.
    let mut declined = plains();
    let metamorph2 = put(&mut declined, P0, "Phyrexian Metamorph", Zone::Graveyard);
    let fanatic3 = put(&mut declined, P0, "Metamorphosis Fanatic", Zone::Hand);
    pool(&mut declined, P0, &[ManaColor::B, ManaColor::B], 4);
    cast(&mut declined, fanatic3, &[]);
    assert!(matches!(
        settle(&mut declined),
        Some(Decision::ChooseTargets { .. })
    ));
    act(
        &mut declined,
        Action::ChooseTarget(Target::Object(metamorph2)),
    );
    assert!(matches!(
        settle(&mut declined),
        Some(Decision::ChooseEffectTargets {
            can_finish: true,
            ..
        })
    ));
    act(&mut declined, Action::FinishEffectSelection);
    settled(&mut declined);
    assert_eq!(zone(&declined, metamorph2), Zone::Graveyard);
}

// ---- Inti, Seneschal of the Sun + multi-card discard batching ------------------

#[test]
fn inti_attack_reflexive_trigger_and_batched_discard_impulse_draw() {
    // 1. Attack trigger: optional discard triggers BOTH reflexive +1/+1 & trample
    //    on target attacking creature AND Inti's own impulse draw.
    let mut state = game(&["Mountain"; 12]);
    let inti = put(
        &mut state,
        P0,
        "Inti, Seneschal of the Sun",
        Zone::Battlefield,
    );
    let mate = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    let discard_card = put(&mut state, P0, "Plains", Zone::Hand);

    pass_until(&mut state, |s| s.step == Step::DeclareAttackers);
    act(&mut state, Action::DeclareAttackers(vec![inti, mate]));
    let choice = settle(&mut state).expect("Inti attack offers optional discard");
    assert!(matches!(
        choice,
        Decision::ChooseEffectTargets {
            player: P0,
            min_targets: 0,
            max_targets: 1,
            can_finish: true,
            ..
        }
    ));
    act(
        &mut state,
        Action::ChooseEffectTarget(Target::Object(discard_card)),
    );
    // Order triggers (impulse draw + reflexive pump) and target `mate`.
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseTargets { legal_targets, .. }
            if legal_targets.contains(&Target::Object(mate)) =>
        {
            Action::ChooseTarget(Target::Object(mate))
        }
        other => first_answer(other),
    });
    assert_eq!(zone(&state, discard_card), Zone::Graveyard);
    assert_eq!(state.objects.get(mate).counters.plus1_plus1, 1);
    assert!(keyword(&state, mate, Keywords::TRAMPLE));
    assert_eq!(
        state.exile.len(),
        1,
        "one impulse exile from discarding 1 card"
    );

    // 2. Multi-card discard batching: Faithless Looting (discard 2), Breakthrough
    //    (X=0 discard whole hand, X=1 keep 1 discard 2), Cabal Therapy (2 copies),
    //    Lion's Eye Diamond (discard whole hand), and ordinary cleanup (discard 2)
    //    MUST each trigger Inti EXACTLY ONCE!
    // (a) Faithless Looting (discard 2).
    let mut looting_state = game(&["Mountain"; 12]);
    put(
        &mut looting_state,
        P0,
        "Inti, Seneschal of the Sun",
        Zone::Battlefield,
    );
    let looting = put(&mut looting_state, P0, "Faithless Looting", Zone::Hand);
    pool(&mut looting_state, P0, &[ManaColor::R], 0);
    cast(&mut looting_state, looting, &[]);
    settle_first(&mut looting_state);
    assert_eq!(
        looting_state.exile.len(),
        1,
        "Faithless Looting discarding 2 triggers Inti once"
    );

    // (b) Breakthrough X=0 discarding 3 cards.
    let mut bt0 = game(&["Mountain"; 12]);
    put(
        &mut bt0,
        P0,
        "Inti, Seneschal of the Sun",
        Zone::Battlefield,
    );
    let bt = put(&mut bt0, P0, "Breakthrough", Zone::Hand);
    pool(&mut bt0, P0, &[ManaColor::U], 0);
    cast(&mut bt0, bt, &[]);
    settle_first(&mut bt0);
    assert_eq!(
        bt0.exile.len(),
        1,
        "Breakthrough X=0 discarding 4 drawn cards triggers Inti once"
    );

    // (c) Breakthrough X=1 keeping 1, discarding 3 cards.
    let mut bt1 = game(&["Mountain"; 12]);
    put(
        &mut bt1,
        P0,
        "Inti, Seneschal of the Sun",
        Zone::Battlefield,
    );
    let bt_x1 = put(&mut bt1, P0, "Breakthrough", Zone::Hand);
    pool(&mut bt1, P0, &[ManaColor::U], 1);
    cast(&mut bt1, bt_x1, &[]);
    settle_with(&mut bt1, |_, decision| match decision {
        Decision::ChooseEffectOption { option_count, .. } => {
            Action::ChooseEffectOption(option_count - 1)
        }
        other => first_answer(other),
    });
    assert_eq!(
        bt1.exile.len(),
        1,
        "Breakthrough X=1 discarding 3 cards triggers Inti once"
    );

    // (d) Cabal Therapy hitting 2 copies in P0's hand.
    let mut therapy = game(&["Mountain"; 12]);
    put(
        &mut therapy,
        P0,
        "Inti, Seneschal of the Sun",
        Zone::Battlefield,
    );
    put(&mut therapy, P0, "Opt", Zone::Hand);
    put(&mut therapy, P0, "Opt", Zone::Hand);
    let cabal = put(&mut therapy, P0, "Cabal Therapy", Zone::Hand);
    pool(&mut therapy, P0, &[ManaColor::B], 0);
    cast(&mut therapy, cabal, &[Target::Player(P0)]);
    let (group, within) = therapy_name_options("Opt");
    let mut answers = vec![within, group];
    settle_with(&mut therapy, |_, decision| match decision {
        Decision::ChooseEffectOption { .. } => Action::ChooseEffectOption(answers.pop().unwrap()),
        other => first_answer(other),
    });
    assert_eq!(
        therapy.exile.len(),
        1,
        "Cabal Therapy discarding 2 copies triggers Inti once"
    );

    // (e) Lion's Eye Diamond discarding 3 cards in hand.
    let mut led_state = game(&["Mountain"; 12]);
    put(
        &mut led_state,
        P0,
        "Inti, Seneschal of the Sun",
        Zone::Battlefield,
    );
    put(&mut led_state, P0, "Plains", Zone::Hand);
    put(&mut led_state, P0, "Plains", Zone::Hand);
    put(&mut led_state, P0, "Plains", Zone::Hand);
    let led = put(&mut led_state, P0, "Lion's Eye Diamond", Zone::Battlefield);
    next(&mut led_state);
    act(
        &mut led_state,
        Action::ActivateManaAbilityChoice(led, ManaColor::R),
    );
    settled(&mut led_state);
    assert_eq!(
        led_state.exile.len(),
        1,
        "Lion's Eye Diamond discarding 3 cards triggers Inti once"
    );

    // (f) Ordinary cleanup step discarding 2 cards (9 -> 7).
    let mut cleanup = game(&["Mountain"; 12]);
    put(
        &mut cleanup,
        P0,
        "Inti, Seneschal of the Sun",
        Zone::Battlefield,
    );
    for _ in 0..9 {
        put(&mut cleanup, P0, "Plains", Zone::Hand);
    }
    pass_until(&mut cleanup, |s| {
        s.active_player == P1 && s.step == Step::Main1
    });
    assert_eq!(
        cleanup.exile.len(),
        1,
        "cleanup step discarding 2 cards triggers Inti once"
    );

    // 3. Impulse draw duration: exiled during cleanup (after P0's end step) stays
    //    playable through P1's turn and P0's next Main1, then expires at P0's next end step!
    let exiled_land = cleanup.exile[0];
    pass_until(&mut cleanup, |s| {
        s.active_player == P0 && s.step == Step::Main1
    });
    assert!(playable_land(&mut cleanup, exiled_land));
    pass_until(&mut cleanup, |s| {
        s.active_player == P1 && s.step == Step::Main1
    });
    pass_until(&mut cleanup, |s| {
        s.active_player == P0 && s.step == Step::Main1
    });
    assert!(!playable_land(&mut cleanup, exiled_land));
}

// ---- Malcolm, Alluring Scoundrel -----------------------------------------------

#[test]
fn malcolm_chorus_counters_free_graveyard_cast_and_edge_cases() {
    let mut state = game(&["Plains"; 20]);
    let malcolm = put(
        &mut state,
        P0,
        "Malcolm, Alluring Scoundrel",
        Zone::Battlefield,
    );
    let ground = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    let baloth = put(&mut state, P0, "Ravenous Baloth", Zone::Hand);

    // Hits 1..=3: flying cannot be blocked by ground creature; chorus counters
    // increment to 3; no free cast offered yet.
    for expected in 1..=3 {
        let blockers = legal_blockers(&mut state, malcolm);
        assert!(!blockers.contains(&ground), "Malcolm has flying");
        // Finish combat unblocked and discard a Plains (keeping baloth in hand).
        act(&mut state, Action::DeclareBlockers(Vec::new()));
        run_until(
            &mut state,
            |s| s.step == Step::Main2 && s.stack.is_empty() && s.engine.pending_triggers.is_empty(),
            |_, decision| match decision {
                Decision::ChooseEffectTargets { legal_targets, .. } => {
                    let pick = legal_targets
                        .iter()
                        .copied()
                        .find(|t| *t != Target::Object(baloth))
                        .unwrap_or(legal_targets[0]);
                    Some(Action::ChooseEffectTarget(pick))
                }
                _ => None,
            },
        );
        assert_eq!(chorus_counters(&state, malcolm), expected);
        assert_eq!(zone(&state, baloth), Zone::Hand);
        pass_until(&mut state, |s| {
            s.active_player == P1 && s.step == Step::Main1
        });
        pass_until(&mut state, |s| {
            s.active_player == P0 && s.step == Step::Main1
        });
    }

    // Hit 4: 4th chorus counter -> discard Ravenous Baloth -> cast it FREE during combat!
    pass_until(&mut state, |s| s.step == Step::DeclareAttackers);
    act(&mut state, Action::DeclareAttackers(vec![malcolm]));
    pass_until(&mut state, |s| s.step == Step::DeclareBlockers);
    act(&mut state, Action::DeclareBlockers(Vec::new()));
    pass_until(&mut state, |s| s.step == Step::CombatDamage);
    let discard_choice = settle(&mut state).expect("Malcolm asks which card to discard");
    assert!(matches!(
        discard_choice,
        Decision::ChooseEffectTargets {
            player: P0,
            min_targets: 1,
            max_targets: 1,
            ..
        }
    ));
    act(
        &mut state,
        Action::ChooseEffectTarget(Target::Object(baloth)),
    );
    let free_cast_choice = settle(&mut state).expect("4 chorus counters offers free cast");
    assert!(matches!(
        free_cast_choice,
        Decision::ChooseEffectTargets {
            player: P0,
            min_targets: 0,
            max_targets: 1,
            can_finish: true,
            ref legal_targets,
            ..
        } if legal_targets == &[Target::Object(baloth)]
    ));
    let saved = serde_json::to_vec(&state).unwrap();
    let mut restored: GameState = serde_json::from_slice(&saved).unwrap();
    for branch in [&mut state, &mut restored] {
        act(branch, Action::ChooseEffectTarget(Target::Object(baloth)));
        settled(branch);
        assert_eq!(chorus_counters(branch, malcolm), 4);
        assert_eq!(zone(branch, baloth), Zone::Battlefield);
        assert_eq!(mana(branch, P0, ManaColor::G), 0);
    }
    assert_eq!(state.state_hash(), restored.state_hash());

    // Hit 5: discarding a sorcery (Preordain) casts it free and sends it to the
    // graveyard (NOT exiled like Bilbo).
    let preordain = put(&mut state, P0, "Preordain", Zone::Hand);
    pass_until(&mut state, |s| {
        s.active_player == P1 && s.step == Step::Main1
    });
    pass_until(&mut state, |s| {
        s.active_player == P0 && s.step == Step::DeclareAttackers
    });
    act(&mut state, Action::DeclareAttackers(vec![malcolm]));
    pass_until(&mut state, |s| s.step == Step::DeclareBlockers);
    act(&mut state, Action::DeclareBlockers(Vec::new()));
    pass_until(&mut state, |s| s.step == Step::CombatDamage);
    assert!(matches!(
        settle(&mut state),
        Some(Decision::ChooseEffectTargets { .. })
    ));
    act(
        &mut state,
        Action::ChooseEffectTarget(Target::Object(preordain)),
    );
    assert!(matches!(
        settle(&mut state),
        Some(Decision::ChooseEffectTargets {
            can_finish: true,
            ref legal_targets,
            ..
        }) if legal_targets == &[Target::Object(preordain)]
    ));
    act(
        &mut state,
        Action::ChooseEffectTarget(Target::Object(preordain)),
    );
    settle_first(&mut state);
    assert_eq!(
        zone(&state, preordain),
        Zone::Graveyard,
        "Malcolm's free sorcery goes to graveyard, not exile"
    );

    // Hit 6: with Rest in Peace exiling the discarded card, no free cast is offered.
    put(&mut state, P1, "Rest in Peace", Zone::Battlefield);
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    pass_until(&mut state, |s| {
        s.active_player == P1 && s.step == Step::Main1
    });
    pass_until(&mut state, |s| {
        s.active_player == P0 && s.step == Step::DeclareAttackers
    });
    act(&mut state, Action::DeclareAttackers(vec![malcolm]));
    pass_until(&mut state, |s| s.step == Step::DeclareBlockers);
    act(&mut state, Action::DeclareBlockers(Vec::new()));
    pass_until(&mut state, |s| s.step == Step::CombatDamage);
    assert!(matches!(
        settle(&mut state),
        Some(Decision::ChooseEffectTargets { .. })
    ));
    act(&mut state, Action::ChooseEffectTarget(Target::Object(bolt)));
    settled(&mut state);
    assert_eq!(zone(&state, bolt), Zone::Exile);
}

// ---- Chandra, Torch of Defiance ------------------------------------------------

#[test]
fn chandra_torch_of_defiance_plus_one_exile_cast_decline_land_and_empty() {
    // 1. Castable spell (Lightning Bolt): cast paying {R} -> NO 2 damage; decline -> 2 damage.
    let mut state = game(&["Plains"; 8]);
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Library);
    let chandra = put(&mut state, P0, "Chandra, Torch of Defiance", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R, ManaColor::R], 2);
    cast(&mut state, chandra, &[]);
    settled(&mut state);
    assert_eq!(loyalty(&state, chandra), Some(4));

    pool(&mut state, P0, &[ManaColor::R], 0);
    activate(&mut state, chandra, 0, &[]);
    let choice = settle(&mut state).expect("Chandra +1 offers casting Bolt");
    assert_eq!(loyalty(&state, chandra), Some(5));
    assert_eq!(zone(&state, bolt), Zone::Exile);
    assert!(matches!(
        choice,
        Decision::ChooseEffectTargets {
            player: P0,
            min_targets: 0,
            max_targets: 1,
            can_finish: true,
            ref legal_targets,
            ..
        } if legal_targets == &[Target::Object(bolt)]
    ));

    // Branch A: cast Bolt paying {R} targeting P1 -> P1 takes 3 (NOT 5), mana spent.
    let mut cast_branch = state.clone();
    let saved = serde_json::to_vec(&cast_branch).unwrap();
    let mut restored: GameState = serde_json::from_slice(&saved).unwrap();
    for branch in [&mut cast_branch, &mut restored] {
        act(branch, Action::ChooseEffectTarget(Target::Object(bolt)));
        choose_target(branch, Target::Player(P1));
        settled(branch);
        assert_eq!(mana(branch, P0, ManaColor::R), 0);
        assert_eq!(life(branch, P1), 17, "3 from Bolt, 0 from Chandra");
        assert_eq!(zone(branch, bolt), Zone::Graveyard);
    }
    assert_eq!(cast_branch.state_hash(), restored.state_hash());

    // Branch B: decline casting Bolt -> Bolt stays exiled, P1 takes 2 from Chandra, {R} unspent.
    let mut decline_branch = state;
    act(&mut decline_branch, Action::FinishEffectSelection);
    settled(&mut decline_branch);
    assert_eq!(zone(&decline_branch, bolt), Zone::Exile);
    assert_eq!(mana(&decline_branch, P0, ManaColor::R), 1);
    assert_eq!(life(&decline_branch, P1), 18);

    // 2. Exiled land (Mountain) -> not castable -> immediate 2 damage.
    let mut land_state = game(&["Mountain"]);
    let chandra_l = put(
        &mut land_state,
        P0,
        "Chandra, Torch of Defiance",
        Zone::Hand,
    );
    pool(&mut land_state, P0, &[ManaColor::R, ManaColor::R], 2);
    cast(&mut land_state, chandra_l, &[]);
    settled(&mut land_state);
    activate(&mut land_state, chandra_l, 0, &[]);
    settled(&mut land_state);
    assert_eq!(life(&land_state, P1), 18);
    assert_eq!(land_state.exile.len(), 1);

    // 3. Unaffordable spell (Emrakul) -> immediate 2 damage.
    let mut unaffordable = game(&["Emrakul, the Aeons Torn"]);
    let chandra_u = put(
        &mut unaffordable,
        P0,
        "Chandra, Torch of Defiance",
        Zone::Hand,
    );
    pool(&mut unaffordable, P0, &[ManaColor::R, ManaColor::R], 2);
    cast(&mut unaffordable, chandra_u, &[]);
    settled(&mut unaffordable);
    activate(&mut unaffordable, chandra_u, 0, &[]);
    settled(&mut unaffordable);
    assert_eq!(life(&unaffordable, P1), 18);

    // 4. Empty library -> immediate 2 damage.
    let mut empty = game(&[]);
    let chandra_e = put(&mut empty, P0, "Chandra, Torch of Defiance", Zone::Hand);
    pool(&mut empty, P0, &[ManaColor::R, ManaColor::R], 2);
    cast(&mut empty, chandra_e, &[]);
    settled(&mut empty);
    activate(&mut empty, chandra_e, 0, &[]);
    settled(&mut empty);
    assert_eq!(life(&empty, P1), 18);
}

#[test]
fn chandra_torch_of_defiance_mana_minus_three_and_emblem() {
    let mut state = plains();
    let chandra = put(&mut state, P0, "Chandra, Torch of Defiance", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R, ManaColor::R], 2);
    cast(&mut state, chandra, &[]);
    settled(&mut state);
    assert_eq!(loyalty(&state, chandra), Some(4));

    // +1: Add {R}{R} (uses the stack as a loyalty ability).
    activate(&mut state, chandra, 1, &[]);
    let _ = next(&mut state);
    assert_eq!(state.stack.len(), 1);
    assert_eq!(
        mana(&state, P0, ManaColor::R),
        0,
        "not added until resolution"
    );
    settled(&mut state);
    assert_eq!(loyalty(&state, chandra), Some(5));
    assert_eq!(mana(&state, P0, ManaColor::R), 2);

    // Next turn, -3 deals 4 damage to target creature (kills 0/4 Wall of Blossoms).
    pass_until(&mut state, |s| {
        s.active_player == P1 && s.step == Step::Main1
    });
    pass_until(&mut state, |s| {
        s.active_player == P0 && s.step == Step::Main1
    });
    let wall = put(&mut state, P1, "Wall of Blossoms", Zone::Battlefield);
    activate(&mut state, chandra, 2, &[Target::Object(wall)]);
    settled(&mut state);
    assert_eq!(loyalty(&state, chandra), Some(2));
    assert_eq!(zone(&state, wall), Zone::Graveyard);

    // Bump Chandra to 7 loyalty and activate -7 emblem (Chandra dies to 0 loyalty).
    change_loyalty(&mut state, chandra, 5);
    assert_eq!(loyalty(&state, chandra), Some(7));
    pass_until(&mut state, |s| {
        s.active_player == P1 && s.step == Step::Main1
    });
    pass_until(&mut state, |s| {
        s.active_player == P0 && s.step == Step::Main1
    });
    activate(&mut state, chandra, 3, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, chandra), Zone::Graveyard);

    // Emblem persists after Chandra dies: casting Opt deals 5 damage to P1!
    let opt = put(&mut state, P0, "Opt", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 0);
    cast(&mut state, opt, &[]);
    assert!(matches!(
        settle(&mut state),
        Some(Decision::ChooseTargets { .. })
    ));
    act(&mut state, Action::ChooseTarget(Target::Player(P1)));
    settle_first(&mut state);
    assert_eq!(life(&state, P1), 15);

    // Casting Brain Freeze (Storm count = 1 -> 1 cast + 1 copy) triggers emblem
    // ONLY ONCE for the cast, NOT for the Storm copy!
    let milled_before = state.players[1].graveyard.len();
    let freeze = put(&mut state, P0, "Brain Freeze", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 1);
    cast(&mut state, freeze, &[Target::Player(P1)]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseTargets { legal_targets, .. }
            if legal_targets.contains(&Target::Player(P1)) =>
        {
            Action::ChooseTarget(Target::Player(P1))
        }
        other => first_answer(other),
    });
    assert_eq!(
        state.players[1].graveyard.len(),
        milled_before + 6,
        "Brain Freeze + 1 Storm copy milled 6"
    );
    assert_eq!(
        life(&state, P1),
        10,
        "Storm copy does not trigger Chandra emblem"
    );
}

// ---- Public-session end-to-end replay ------------------------------------------

#[derive(Default)]
struct C7aEvents {
    casts: [u32; C7A_CARDS.len()],
}

impl C7aEvents {
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
        if let Some(index) = C7A_CARDS.iter().position(|&card| card == name) {
            self.casts[index] += 1;
        }
    }
}

#[test]
fn c7a_decks_finish_and_replay_through_public_sessions_both_seats() {
    let grixis = deck(&[
        ("Volcanic Island", 12),
        ("Badlands", 8),
        ("Underground Sea", 4),
        ("Hullbreacher", 8),
        ("Malcolm, Alluring Scoundrel", 8),
        ("Metamorphosis Fanatic", 8),
        ("Chandra, Torch of Defiance", 8),
        ("Opt", 4),
    ]);
    let red = deck(&[
        ("Mountain", 24),
        ("Inti, Seneschal of the Sun", 12),
        ("Gandalf, Goblins' Bane", 12),
        ("Headliner Scarlett", 12),
    ]);
    let mut events = C7aEvents::default();
    // Twelve games plus their twelve replays, with both policies and seats.
    for pair in [[grixis.clone(), red.clone()], [red, grixis]] {
        for seed in 0xC7A0_u64..0xC7A3 {
            for policy in [Policy::Ranked, Policy::Random(seed + 0x100)] {
                let played = play_observed(pair.clone(), None, seed, policy, &mut |d, a| {
                    events.observe(d, a)
                });
                let replay = play_observed(pair.clone(), None, seed, policy, &mut |_, _| {});
                assert_eq!(played.transcript_sha256, replay.transcript_sha256);
            }
        }
    }
    for (name, casts) in C7A_CARDS.into_iter().zip(events.casts) {
        assert!(casts > 0, "{name} never cast through a public session");
    }
}
