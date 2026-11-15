//! Premodern sideboard wave P3h: native rules and public-session tests for
//! Overgrown Estate and Decree of Pain (completing Premodern Other Control's
//! 75), and for Full English Breakfast's four sideboard cards (Enlightened
//! Tutor, Auramancer, Monk Realist and Ghitu Slinger).
#![cfg(feature = "eternal-fixtures")]
#![allow(dead_code)]

use crate::eternal_catalog_v1::{play_observed, Policy};
use mtg_kernel::card_def::{
    card_id_by_name, CardCapability, CardDef, CardType, Subtype, CARD_DEFS,
};
use mtg_kernel::effect::{EffectOp, PendingEffectChoice};
use mtg_kernel::engine::{self, Action, Decision, OptionalCostChoice};
use mtg_kernel::eternal_cards_v1::EternalOpV1;
use mtg_kernel::eternal_sideboard_s1d_v1::{CircleColorV1, S1dOpV1};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::limited_session_v1::{CustomCardCountV1, CustomDeckV1};
use mtg_kernel::mana::ManaColor;
use mtg_kernel::standard_cards_v1::StandardOpV1;
use mtg_kernel::state::{GameObject, GameState, ObjectLinkV4, ObjectStateV4, Step, Target, Zone};
use serde_json::Value;

const P0: PlayerId = PlayerId::P0;
const P1: PlayerId = PlayerId::P1;

const P3H_CARDS: [&str; 6] = [
    "Overgrown Estate",
    "Decree of Pain",
    "Enlightened Tutor",
    "Auramancer",
    "Monk Realist",
    "Ghitu Slinger",
];

fn def(name: &str) -> &'static CardDef {
    &CARD_DEFS[id(name) as usize]
}

fn id(name: &str) -> u16 {
    card_id_by_name(name).unwrap_or_else(|| panic!("{name}"))
}

fn turn_of(active: PlayerId) -> GameState {
    let ids = vec![id("Plains"); 20];
    let mut state = GameState::new_from_libraries(
        &ids,
        &ids,
        |id| CARD_DEFS[id as usize].object_name.into(),
        0x5033_4800_0001,
    );
    state.active_player = active;
    state.priority_player = active;
    state.step = Step::Main1;
    assert!(state.players.iter().all(|seat| seat.hand.is_empty()));
    state
}

fn ordered_turn_of(active: PlayerId) -> GameState {
    let ids = vec![id("Plains"); 20];
    let mut state = GameState::new_from_libraries_with_graveyard_order_v1(
        &ids,
        &ids,
        |id| CARD_DEFS[id as usize].object_name.into(),
        0x5033_4800_0002,
        active,
    );
    state.active_player = active;
    state.priority_player = active;
    state.step = Step::Main1;
    state
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

fn life(state: &GameState, player: PlayerId) -> i32 {
    state.players[player.index()].life
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

fn settle(state: &mut GameState) -> Option<Decision> {
    for _ in 0..200 {
        match next(state) {
            Decision::OrderTriggers { pending, .. } => {
                act(state, Action::OrderTriggers((0..pending.len()).collect()))
            }
            Decision::CastSpellOrPass { .. }
                if state.stack.is_empty() && state.engine.pending_triggers.is_empty() =>
            {
                return None;
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
        Decision::ChooseOptionalCost { .. } => {
            Action::ChooseOptionalCost(OptionalCostChoice::Decline)
        }
        other => panic!("unexpected choice: {other:?}"),
    }
}

fn settle_option(state: &mut GameState, option: u16) {
    while let Some(decision) = settle(state) {
        let action = match decision {
            Decision::ChooseEffectOption { .. } => Action::ChooseEffectOption(option),
            other => first_answer(other),
        };
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

fn choose_target(state: &mut GameState, target: Target) {
    match next(state) {
        Decision::ChooseTargets { legal_targets, .. } => {
            assert!(
                legal_targets.contains(&target),
                "{target:?} not in {legal_targets:?}"
            );
            act(state, Action::ChooseTarget(target));
        }
        other => panic!("expected targets, got {other:?}"),
    }
}

fn choose_etb_target(state: &mut GameState, target: Target) {
    match settle(state) {
        Some(Decision::ChooseTargets { legal_targets, .. }) => {
            assert!(
                legal_targets.contains(&target),
                "{target:?} not in {legal_targets:?}"
            );
            act(state, Action::ChooseTarget(target));
        }
        other => panic!("expected ETB trigger targets, got {other:?}"),
    }
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

fn activate_paying(
    state: &mut GameState,
    source: ObjectId,
    index: u8,
    targets: &[Target],
    cost: Option<ObjectId>,
) {
    assert!(
        activatable(state, source, index),
        "{} ability {index} not activatable",
        state.objects.get(source).name
    );
    act(state, Action::ActivateAbility(source, index));
    let mut targets = targets.iter().copied();
    loop {
        match next(state) {
            Decision::ChooseTargets { legal_targets, .. } => {
                let target = targets.next().expect("a target");
                assert!(legal_targets.contains(&target), "{target:?}");
                act(state, Action::ChooseTarget(target));
            }
            Decision::ChooseCostTargets { candidates, .. } => {
                let pick = cost
                    .filter(|object| candidates.contains(object))
                    .unwrap_or(candidates[0]);
                act(state, Action::ChooseCostTarget(pick));
            }
            _ => break,
        }
    }
    assert!(targets.next().is_none(), "unused targets");
}

fn activate(state: &mut GameState, source: ObjectId, index: u8, targets: &[Target]) {
    activate_paying(state, source, index, targets, None);
}

fn give_priority(state: &mut GameState, player: PlayerId) {
    priority(state);
    if state.priority_player != player {
        act(state, Action::Pass);
        priority(state);
    }
    assert_eq!(state.priority_player, player);
}

fn pass_until(state: &mut GameState, mut done: impl FnMut(&GameState) -> bool) {
    for _ in 0..3000 {
        let decision = next(state);
        if done(state) {
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
            other => panic!("unexpected decision while advancing turn: {other:?}"),
        }
    }
    panic!("condition not reached");
}

fn settle_finishing(state: &mut GameState) {
    while let Some(decision) = settle(state) {
        match decision {
            Decision::ChooseEffectTargets {
                can_finish: true, ..
            } => act(state, Action::FinishEffectSelection),
            other => panic!("unexpected choice: {other:?}"),
        }
    }
}

fn circle_menu(state: &GameState) -> Vec<(ObjectLinkV4, CircleColorV1)> {
    let Some(PendingEffectChoice::ChooseOption { options, .. }) = state
        .engine
        .pending_effect
        .as_ref()
        .and_then(|pending| pending.choice.as_ref())
    else {
        panic!("Circle source options missing");
    };
    options
        .iter()
        .map(|op| match op {
            EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::S1dV1(
                S1dOpV1::CopShieldV1 { source },
            ))) => (*source, CircleColorV1::Blue),
            EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::S1dV1(
                S1dOpV1::CircleShieldV1 { source, color },
            ))) => (*source, *color),
            other => panic!("unexpected Circle option {other:?}"),
        })
        .collect()
}

// ---- Registry metadata and castability ----------------------------------------------

#[test]
fn p3h_cards_are_registered_full_with_expected_characteristics() {
    for name in P3H_CARDS {
        let card = def(name);
        assert_eq!(card.capability, CardCapability::Full, "{name}");
        assert!(card.is_executable(), "{name}");
    }

    let estate = def("Overgrown Estate");
    assert_eq!(estate.mana_value, 3);
    assert_eq!(estate.types, &[CardType::Enchantment]);
    assert_eq!(estate.activated_abilities.len(), 1);

    let decree = def("Decree of Pain");
    assert_eq!(decree.mana_value, 8);
    assert_eq!(decree.types, &[CardType::Sorcery]);
    assert_eq!(decree.activated_abilities.len(), 1);

    let tutor = def("Enlightened Tutor");
    assert_eq!(tutor.mana_value, 1);
    assert_eq!(tutor.types, &[CardType::Instant]);

    let auramancer = def("Auramancer");
    assert_eq!(auramancer.mana_value, 3);
    assert_eq!((auramancer.power, auramancer.toughness), (Some(2), Some(2)));
    assert_eq!(auramancer.subtypes, &[Subtype::Human, Subtype::Wizard]);

    let monk = def("Monk Realist");
    assert_eq!(monk.mana_value, 2);
    assert_eq!((monk.power, monk.toughness), (Some(1), Some(1)));
    assert_eq!(
        monk.subtypes,
        &[Subtype::Human, Subtype::Monk, Subtype::Cleric]
    );

    let slinger = def("Ghitu Slinger");
    assert_eq!(slinger.mana_value, 3);
    assert_eq!((slinger.power, slinger.toughness), (Some(2), Some(2)));
    assert_eq!(slinger.subtypes, &[Subtype::Human, Subtype::Nomad]);
}

#[test]
fn p3h_cards_cast_from_tapped_lands_both_seats() {
    for seat in [P0, P1] {
        let mut state = turn_of(seat);
        for land in ["Plains", "Plains", "Swamp", "Swamp", "Forest", "Mountain"] {
            put(&mut state, seat, land, Zone::Battlefield);
        }
        for _ in 0..4 {
            put(&mut state, seat, "Swamp", Zone::Battlefield);
        }
        for name in P3H_CARDS {
            let spell = put(&mut state, seat, name, Zone::Hand);
            assert!(castable(&mut state, spell), "{name} in {seat:?}");
        }
    }
}

// ---- Overgrown Estate ---------------------------------------------------------------

#[test]
fn overgrown_estate_sacrifices_only_controlled_lands_to_gain_three_life_both_seats() {
    for seat in [P0, P1] {
        let opponent = seat.opponent();
        let mut state = turn_of(opponent);
        let estate = put(&mut state, seat, "Overgrown Estate", Zone::Battlefield);
        let _opp_land = put(&mut state, opponent, "Forest", Zone::Battlefield);
        let _own_elves = put(&mut state, seat, "Llanowar Elves", Zone::Battlefield);

        give_priority(&mut state, seat);
        assert!(
            !activatable(&mut state, estate, 0),
            "opponent lands and nonlands cannot pay Overgrown Estate"
        );

        let first = put(&mut state, seat, "Plains", Zone::Battlefield);
        let second = put(&mut state, seat, "Swamp", Zone::Battlefield);
        give_priority(&mut state, seat);
        assert!(activatable(&mut state, estate, 0));

        activate_paying(&mut state, estate, 0, &[], Some(second));
        assert_eq!(
            zone(&state, second),
            Zone::Graveyard,
            "sacrifice paid on announcement"
        );
        assert_eq!(zone(&state, first), Zone::Battlefield);
        assert_eq!(life(&state, seat), 20);

        settled(&mut state);
        assert_eq!(life(&state, seat), 23);
    }
}

// ---- Decree of Pain -----------------------------------------------------------------

#[test]
fn decree_of_pain_destroys_without_regeneration_ignores_indestructible_and_counts_tokens_and_exile_both_seats(
) {
    for seat in [P0, P1] {
        let opponent = seat.opponent();
        let mut state = turn_of(seat);
        let hollow = put(&mut state, seat, "Yavimaya Hollow", Zone::Battlefield);
        let shielded = put(&mut state, seat, "Llanowar Elves", Zone::Battlefield);
        let angler = put(&mut state, opponent, "Gurmag Angler", Zone::Battlefield);
        let indestructible = put(&mut state, opponent, "Nutrient Block", Zone::Battlefield);
        let ring = put(&mut state, opponent, "Sol Ring", Zone::Battlefield);
        // Rest in Peace replaces graveyard zone changes with exile; destroyed
        // creatures exiled instead still count as destroyed (701.8).
        let rip = put(&mut state, opponent, "Rest in Peace", Zone::Battlefield);

        // Put a regeneration shield on Llanowar Elves.
        pool(&mut state, seat, &[ManaColor::G], 0);
        activate(&mut state, hollow, 0, &[Target::Object(shielded)]);
        settled(&mut state);

        // Create a 1/1 Soldier creature token via Decree of Justice cycling.
        let justice = put(&mut state, seat, "Decree of Justice", Zone::Hand);
        pool(&mut state, seat, &[ManaColor::W], 3);
        activate(&mut state, justice, 0, &[]);
        // Choose to pay {1} for 1 Soldier token, then let cycling draw resolve.
        match settle(&mut state) {
            Some(Decision::ChooseEffectOption { .. }) => {
                act(&mut state, Action::ChooseEffectOption(1))
            }
            other => panic!("expected Decree of Justice X choice, got {other:?}"),
        }
        settled(&mut state);
        let hand_before = state.players[seat.index()].hand.len();
        let token = state.players[seat.index()]
            .battlefield
            .iter()
            .copied()
            .find(|&obj| state.objects.get(obj).v4.is_token)
            .expect("Soldier token on battlefield");

        let decree = put(&mut state, seat, "Decree of Pain", Zone::Hand);
        pool(&mut state, seat, &[ManaColor::B, ManaColor::B], 6);
        cast(&mut state, decree, &[]);
        settled(&mut state);

        // Shielded creature, opposing creature, and Soldier token were all
        // destroyed (3 total); Nutrient Block (indestructible), Sol Ring, Rest
        // in Peace, and Yavimaya Hollow remain on the battlefield.
        assert_eq!(zone(&state, shielded), Zone::Exile);
        assert_eq!(zone(&state, angler), Zone::Exile);
        assert_ne!(zone(&state, token), Zone::Battlefield);
        assert_eq!(zone(&state, indestructible), Zone::Battlefield);
        assert_eq!(zone(&state, ring), Zone::Battlefield);
        assert_eq!(zone(&state, rip), Zone::Battlefield);
        assert_eq!(zone(&state, hollow), Zone::Battlefield);
        assert_eq!(
            state.players[seat.index()].hand.len(),
            hand_before + 3,
            "drew 3 cards (shielded creature + opposing creature + token, even under Rest in Peace)"
        );
    }
}

#[test]
fn decree_of_pain_offers_dredge_for_a_dredge_creature_it_destroyed_itself() {
    for seat in [P0, P1] {
        let opponent = seat.opponent();
        let mut state = turn_of(seat);
        let imp = put(&mut state, seat, "Stinkweed Imp", Zone::Battlefield);
        let elves = put(&mut state, opponent, "Llanowar Elves", Zone::Battlefield);
        let decree = put(&mut state, seat, "Decree of Pain", Zone::Hand);
        assert!(state.players[seat.index()].graveyard.is_empty());

        pool(&mut state, seat, &[ManaColor::B, ManaColor::B], 6);
        cast(&mut state, decree, &[]);

        // First draw after destruction offers Stinkweed Imp's Dredge 5.
        match settle(&mut state) {
            Some(Decision::ChooseEffectTargets {
                player,
                legal_targets,
                can_finish: true,
                ..
            }) => {
                assert_eq!(player, seat);
                assert_eq!(legal_targets, vec![Target::Object(imp)]);
                act(&mut state, Action::ChooseEffectTarget(Target::Object(imp)));
            }
            other => panic!("expected Dredge prompt for destroyed Stinkweed Imp, got {other:?}"),
        }
        settle_finishing(&mut state);

        // Imp was dredged to hand on the first draw (milling 5 Plains) and
        // the second draw drew a Plains normally.
        assert_eq!(zone(&state, imp), Zone::Hand);
        assert_eq!(zone(&state, elves), Zone::Graveyard);
        assert_eq!(state.players[seat.index()].hand.len(), 2);
    }
}

#[test]
fn decree_of_pain_waits_for_ordered_graveyard_batch_before_drawing_both_seats() {
    for seat in [P0, P1] {
        let mut state = ordered_turn_of(seat);
        let pup = put(&mut state, seat, "Jackal Pup", Zone::Battlefield);
        let elves = put(&mut state, seat, "Llanowar Elves", Zone::Battlefield);
        let decree = put(&mut state, seat, "Decree of Pain", Zone::Hand);
        pool(&mut state, seat, &[ManaColor::B, ManaColor::B], 6);
        cast(&mut state, decree, &[]);

        // Decree of Pain resolves after priority passes, pausing to order the 2
        // simultaneous graveyard arrivals before drawing.
        match settle(&mut state) {
            Some(Decision::ChooseEffectTargets {
                player,
                legal_targets,
                can_finish: false,
                ..
            }) => {
                assert_eq!(player, seat);
                assert_eq!(
                    legal_targets,
                    vec![Target::Object(pup), Target::Object(elves)]
                );
                assert!(
                    state.players[seat.index()].hand.is_empty(),
                    "draws wait until the simultaneous graveyard batch is ordered"
                );
                // Place elves first (bottom), leaving pup on top of the graveyard.
                act(
                    &mut state,
                    Action::ChooseEffectTarget(Target::Object(elves)),
                );
            }
            other => panic!("expected graveyard order prompt, got {other:?}"),
        }
        settled(&mut state);

        assert_eq!(
            state.players[seat.index()].graveyard,
            vec![elves, pup, decree]
        );
        assert_eq!(state.players[seat.index()].hand.len(), 2);
    }
}

#[test]
fn decree_of_pain_cycling_shrinks_creatures_before_draw_and_triggers_under_voidwalker_both_seats() {
    for seat in [P0, P1] {
        let opponent = seat.opponent();
        let mut state = turn_of(seat);
        // Dauthi Voidwalker exiles opponent's discarded cards; 702.29c still
        // triggers Decree of Pain's "When you cycle this card" ability, which
        // kills the 3/2 Voidwalker before the cycling draw resolves.
        let voidwalker = put(&mut state, opponent, "Dauthi Voidwalker", Zone::Battlefield);
        let angler = put(&mut state, opponent, "Gurmag Angler", Zone::Battlefield);
        let decree = put(&mut state, seat, "Decree of Pain", Zone::Hand);

        pool(&mut state, seat, &[ManaColor::B, ManaColor::B], 2);
        assert!(
            !activatable(&mut state, decree, 0),
            "cycling Decree of Pain costs {{3}}{{B}}{{B}}"
        );
        pool(&mut state, seat, &[], 1);
        activate(&mut state, decree, 0, &[]);

        // Decree was exiled with a void counter by Dauthi Voidwalker, yet the
        // cycling trigger was queued above the cycling draw.
        assert_eq!(zone(&state, decree), Zone::Exile);
        priority(&mut state);
        assert_eq!(
            state.stack.len(),
            2,
            "cycling ability on bottom, -2/-2 trigger on top"
        );

        // Resolve only the top stack item (the -2/-2 trigger) before the draw.
        act(&mut state, Action::Pass);
        act(&mut state, Action::Pass);
        priority(&mut state);
        assert_eq!(state.stack.len(), 1, "cycling draw still on stack");
        assert_eq!(zone(&state, voidwalker), Zone::Graveyard);
        assert_eq!(power_toughness(&state, angler), (3, 3));
        assert!(state.players[seat.index()].hand.is_empty());

        // Resolve the cycling draw, then pass to next turn so -2/-2 wears off.
        settled(&mut state);
        assert_eq!(state.players[seat.index()].hand.len(), 1);
        pass_until(&mut state, |s| s.active_player == opponent);
        assert_eq!(power_toughness(&state, angler), (5, 5));
    }
}

// ---- Enlightened Tutor --------------------------------------------------------------

#[test]
fn enlightened_tutor_finds_artifact_or_enchantment_reveals_and_allows_fail_to_find_both_seats() {
    for seat in [P0, P1] {
        let mut state = turn_of(seat);
        let ring = put(&mut state, seat, "Sol Ring", Zone::Library);
        let seal = put(&mut state, seat, "Seal of Fire", Zone::Library);
        let bolt = put(&mut state, seat, "Lightning Bolt", Zone::Library);
        let elves = put(&mut state, seat, "Llanowar Elves", Zone::Library);
        let tutor = put(&mut state, seat, "Enlightened Tutor", Zone::Hand);

        pool(&mut state, seat, &[ManaColor::W], 0);
        cast(&mut state, tutor, &[]);

        match settle(&mut state) {
            Some(Decision::ChooseEffectTargets {
                player,
                legal_targets,
                can_finish: true,
                ..
            }) => {
                assert_eq!(player, seat);
                assert!(legal_targets.contains(&Target::Object(ring)));
                assert!(legal_targets.contains(&Target::Object(seal)));
                assert!(!legal_targets.contains(&Target::Object(bolt)));
                assert!(!legal_targets.contains(&Target::Object(elves)));
                act(&mut state, Action::ChooseEffectTarget(Target::Object(seal)));
            }
            other => panic!("expected Enlightened Tutor search prompt, got {other:?}"),
        }
        settle_finishing(&mut state);

        assert_eq!(
            state.players[seat.index()].library.first().copied(),
            Some(seal),
            "chosen enchantment placed on top of library after shuffle"
        );
        for viewer in [P0, P1] {
            assert!(
                state
                    .known_library_cards(viewer, seat)
                    .iter()
                    .any(|entry| entry.position == 0 && entry.object == seal),
                "found card revealed to {viewer:?}"
            );
        }

        // Second Enlightened Tutor declines to find a card (701.19b).
        let second_tutor = put(&mut state, seat, "Enlightened Tutor", Zone::Hand);
        pool(&mut state, seat, &[ManaColor::W], 0);
        cast(&mut state, second_tutor, &[]);
        match settle(&mut state) {
            Some(Decision::ChooseEffectTargets {
                can_finish: true, ..
            }) => act(&mut state, Action::FinishEffectSelection),
            other => panic!("expected optional search completion, got {other:?}"),
        }
        settled(&mut state);
        assert_eq!(zone(&state, second_tutor), Zone::Graveyard);
    }
}

// ---- Auramancer ---------------------------------------------------------------------

#[test]
fn auramancer_targets_only_own_graveyard_enchantment_supports_decline_and_fizzles_if_exiled_both_seats(
) {
    for seat in [P0, P1] {
        let opponent = seat.opponent();
        let mut state = turn_of(seat);
        let own_seal = put(&mut state, seat, "Seal of Fire", Zone::Graveyard);
        let own_ring = put(&mut state, seat, "Sol Ring", Zone::Graveyard);
        let opp_seal = put(&mut state, opponent, "Seal of Fire", Zone::Graveyard);

        // First Auramancer: accept "you may" (option 0) -> returns Seal of Fire.
        let first = put(&mut state, seat, "Auramancer", Zone::Hand);
        pool(&mut state, seat, &[ManaColor::W], 2);
        cast(&mut state, first, &[]);
        match settle(&mut state) {
            Some(Decision::ChooseTargets { legal_targets, .. }) => {
                assert_eq!(
                    legal_targets,
                    vec![Target::Object(own_seal)],
                    "only enchantment cards in controller's graveyard are legal"
                );
                assert!(!legal_targets.contains(&Target::Object(own_ring)));
                assert!(!legal_targets.contains(&Target::Object(opp_seal)));
                act(&mut state, Action::ChooseTarget(Target::Object(own_seal)));
            }
            other => panic!("expected Auramancer target choice, got {other:?}"),
        }
        settle_option(&mut state, 0);
        assert_eq!(zone(&state, own_seal), Zone::Hand);

        // Second Auramancer: decline "you may" (option 1) -> stays in graveyard.
        let grove = put(&mut state, seat, "Sterling Grove", Zone::Graveyard);
        let second = put(&mut state, seat, "Auramancer", Zone::Hand);
        pool(&mut state, seat, &[ManaColor::W], 2);
        cast(&mut state, second, &[]);
        choose_etb_target(&mut state, Target::Object(grove));
        settle_option(&mut state, 1);
        assert_eq!(zone(&state, grove), Zone::Graveyard);

        // Third Auramancer: opponent exiles target in response via Ghost Vacuum ->
        // trigger fails to resolve (608.2b) without prompting ChooseEffectOption.
        let vacuum = put(&mut state, opponent, "Ghost Vacuum", Zone::Battlefield);
        let third = put(&mut state, seat, "Auramancer", Zone::Hand);
        pool(&mut state, seat, &[ManaColor::W], 2);
        cast(&mut state, third, &[]);
        choose_etb_target(&mut state, Target::Object(grove));
        give_priority(&mut state, opponent);
        pool(&mut state, opponent, &[], 1);
        activate(&mut state, vacuum, 0, &[Target::Object(grove)]);
        settled(&mut state);
        assert_eq!(zone(&state, grove), Zone::Exile);
    }
}

// ---- Monk Realist -------------------------------------------------------------------

#[test]
fn monk_realist_destroys_mandatory_target_enchantment_and_skips_when_none_both_seats() {
    for seat in [P0, P1] {
        let opponent = seat.opponent();
        let mut state = turn_of(seat);

        // No enchantment on the battlefield: Monk Realist enters with no target prompt.
        let _ring = put(&mut state, opponent, "Sol Ring", Zone::Battlefield);
        let first = put(&mut state, seat, "Monk Realist", Zone::Hand);
        pool(&mut state, seat, &[ManaColor::W], 1);
        cast(&mut state, first, &[]);
        settled(&mut state);
        assert_eq!(zone(&state, first), Zone::Battlefield);

        // With an enchantment on the battlefield: mandatory target destroys it.
        let seal = put(&mut state, opponent, "Seal of Fire", Zone::Battlefield);
        let second = put(&mut state, seat, "Monk Realist", Zone::Hand);
        pool(&mut state, seat, &[ManaColor::W], 1);
        cast(&mut state, second, &[]);
        choose_etb_target(&mut state, Target::Object(seal));
        settled(&mut state);
        assert_eq!(zone(&state, seal), Zone::Graveyard);
    }
}

// ---- Ghitu Slinger ------------------------------------------------------------------

#[test]
fn ghitu_slinger_deals_two_damage_to_any_target_supports_cop_red_departed_source_and_owes_echo_both_seats(
) {
    for seat in [P0, P1] {
        let opponent = seat.opponent();
        let mut state = turn_of(seat);
        let cop_red = put(
            &mut state,
            opponent,
            "Circle of Protection: Red",
            Zone::Battlefield,
        );
        let jace = put(
            &mut state,
            opponent,
            "Jace, Wielder of Mysteries",
            Zone::Hand,
        );
        mtg_kernel::event::propose_and_commit(
            &mut state,
            mtg_kernel::event::ProposedEvent::zone_change(jace, Zone::Battlefield),
        );
        assert_eq!(mtg_kernel::planeswalker_v1::loyalty(&state, jace), Some(4));
        let elves = put(&mut state, opponent, "Llanowar Elves", Zone::Battlefield);

        // First Slinger targets Jace (planeswalker), verifying AnyTarget includes
        // creatures, players, and planeswalkers; then pays Echo {2}{R} on next upkeep.
        let first = put(&mut state, seat, "Ghitu Slinger", Zone::Hand);
        pool(&mut state, seat, &[ManaColor::R], 2);
        cast(&mut state, first, &[]);
        match settle(&mut state) {
            Some(Decision::ChooseTargets { legal_targets, .. }) => {
                assert!(legal_targets.contains(&Target::Player(seat)));
                assert!(legal_targets.contains(&Target::Player(opponent)));
                assert!(legal_targets.contains(&Target::Object(elves)));
                assert!(legal_targets.contains(&Target::Object(jace)));
                assert!(!legal_targets.contains(&Target::Object(cop_red)));
                act(&mut state, Action::ChooseTarget(Target::Object(jace)));
            }
            other => panic!("expected Ghitu Slinger target prompt, got {other:?}"),
        }
        settled(&mut state);
        assert_eq!(mtg_kernel::planeswalker_v1::loyalty(&state, jace), Some(2));

        // Second Slinger targets opponent, is killed in response by Swords to
        // Plowshares, and opponent activates Circle of Protection: Red choosing
        // the departed Slinger to prevent the 2 damage (113.7a / 609.7a).
        let second = put(&mut state, seat, "Ghitu Slinger", Zone::Hand);
        let swords = put(&mut state, opponent, "Swords to Plowshares", Zone::Hand);
        pool(&mut state, seat, &[ManaColor::R], 2);
        cast(&mut state, second, &[]);
        choose_etb_target(&mut state, Target::Player(opponent));
        let departed_slinger = ObjectLinkV4 {
            object: second,
            zone_change_count: state.objects.get(second).zone_change_count,
        };
        give_priority(&mut state, opponent);
        pool(&mut state, opponent, &[ManaColor::W], 0);
        cast(&mut state, swords, &[Target::Object(second)]);
        // Resolve Swords to Plowshares so second Slinger is exiled while its
        // ETB damage trigger is still on the stack.
        priority(&mut state);
        act(&mut state, Action::Pass);
        priority(&mut state);
        act(&mut state, Action::Pass);
        priority(&mut state);
        assert_eq!(zone(&state, second), Zone::Exile);
        assert_eq!(state.stack.len(), 1);

        give_priority(&mut state, opponent);
        pool(&mut state, opponent, &[], 1);
        activate(&mut state, cop_red, 0, &[]);
        match settle(&mut state) {
            Some(Decision::ChooseEffectOption { player, .. }) => {
                assert_eq!(player, opponent);
                let menu = circle_menu(&state);
                let idx = menu
                    .iter()
                    .position(|&(offered, color)| {
                        offered == departed_slinger && color == CircleColorV1::Red
                    })
                    .unwrap_or_else(|| panic!("departed Ghitu Slinger not offered in {menu:?}"));
                act(&mut state, Action::ChooseEffectOption(idx as u16));
            }
            other => panic!("expected CoP: Red source choice, got {other:?}"),
        }
        settled(&mut state);
        assert_eq!(life(&state, opponent), 20, "2 damage prevented by CoP: Red");

        // Advance to seat's next upkeep: first Slinger's Echo {2}{R} triggers.
        pass_until(&mut state, |s| s.active_player == opponent);
        pass_until(&mut state, |s| {
            s.active_player == seat && s.step == Step::Upkeep
        });
        // Pay {2}{R} (option 0) to keep first Slinger.
        pool(&mut state, seat, &[ManaColor::R], 2);
        settle_option(&mut state, 0);
        assert_eq!(zone(&state, first), Zone::Battlefield);

        // Cast a third Slinger targeting opponent (no CoP activation -> 18 life),
        // then decline Echo on the following upkeep -> sacrificed to graveyard.
        pass_until(&mut state, |s| {
            s.active_player == seat && s.step == Step::Main1
        });
        let third = put(&mut state, seat, "Ghitu Slinger", Zone::Hand);
        pool(&mut state, seat, &[ManaColor::R], 2);
        cast(&mut state, third, &[]);
        choose_etb_target(&mut state, Target::Player(opponent));
        settled(&mut state);
        assert_eq!(life(&state, opponent), 18);

        pass_until(&mut state, |s| s.active_player == opponent);
        pass_until(&mut state, |s| {
            s.active_player == seat && s.step == Step::Upkeep
        });
        // First Slinger no longer owes echo; only third Slinger triggers echo.
        pool(&mut state, seat, &[ManaColor::R], 2);
        settle_option(&mut state, 1);
        assert_eq!(zone(&state, first), Zone::Battlefield);
        assert_eq!(zone(&state, third), Zone::Graveyard);
    }
}

// ---- Public-session consumer games --------------------------------------------------

#[derive(Default)]
struct P3hEvents {
    tutor_casts: usize,
    auramancer_casts: usize,
    monk_casts: usize,
    slinger_casts: usize,
    decree_casts_or_cycles: usize,
    estate_casts: usize,
}

impl P3hEvents {
    fn observe(&mut self, _decision: &Value, action: &Value) {
        let semantic = &action["semantic"];
        let Some(card_id) = semantic["source"]["card_db_id"].as_u64() else {
            return;
        };
        let name = CARD_DEFS[card_id as usize].name;
        match (semantic["action_kind"].as_str(), name) {
            (Some("cast_spell"), "Enlightened Tutor") => self.tutor_casts += 1,
            (Some("cast_spell"), "Auramancer") => self.auramancer_casts += 1,
            (Some("cast_spell"), "Monk Realist") => self.monk_casts += 1,
            (Some("cast_spell"), "Ghitu Slinger") => self.slinger_casts += 1,
            (Some("cast_spell" | "activate_ability"), "Decree of Pain") => {
                self.decree_casts_or_cycles += 1
            }
            (Some("cast_spell"), "Overgrown Estate") => self.estate_casts += 1,
            _ => {}
        }
    }
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

#[test]
fn p3h_decks_finish_and_replay_through_public_sessions_both_seats() {
    let feb_sb_shell = deck(&[
        ("Plateau", 8),
        ("Ancient Tomb", 4),
        ("Plains", 4),
        ("Mountain", 4),
        ("Sol Ring", 4),
        ("Seal of Fire", 4),
        ("Lightning Bolt", 8),
        ("Jackal Pup", 8),
        ("Enlightened Tutor", 4),
        ("Auramancer", 4),
        ("Monk Realist", 4),
        ("Ghitu Slinger", 4),
    ]);
    let control_sb_shell = deck(&[
        ("Scrubland", 6),
        ("Bayou", 6),
        ("Savannah", 4),
        ("Ancient Tomb", 4),
        ("Dark Ritual", 4),
        ("Sol Ring", 4),
        ("Mana Vault", 4),
        ("Hypnotic Specter", 8),
        ("Llanowar Elves", 8),
        ("Overgrown Estate", 6),
        ("Decree of Pain", 6),
    ]);

    let mut totals = P3hEvents::default();
    for (pair, first_seed) in [
        (
            [feb_sb_shell.clone(), control_sb_shell.clone()],
            0x5033_4800_u64,
        ),
        ([control_sb_shell, feb_sb_shell], 0x5033_4810),
    ] {
        for seed in first_seed..first_seed + 3 {
            for policy in [Policy::Ranked, Policy::Random(seed + 0x100)] {
                let mut events = P3hEvents::default();
                let game =
                    play_observed(pair.clone(), None, seed, policy, &mut |decision, action| {
                        events.observe(decision, action)
                    });
                let replay = play_observed(pair.clone(), None, seed, policy, &mut |_, _| {});
                assert_eq!(game.transcript_sha256, replay.transcript_sha256);

                totals.tutor_casts += events.tutor_casts;
                totals.auramancer_casts += events.auramancer_casts;
                totals.monk_casts += events.monk_casts;
                totals.slinger_casts += events.slinger_casts;
                totals.decree_casts_or_cycles += events.decree_casts_or_cycles;
                totals.estate_casts += events.estate_casts;
            }
        }
    }

    assert!(totals.tutor_casts > 0, "Enlightened Tutor never cast");
    assert!(totals.auramancer_casts > 0, "Auramancer never cast");
    assert!(totals.monk_casts > 0, "Monk Realist never cast");
    assert!(totals.slinger_casts > 0, "Ghitu Slinger never cast");
    assert!(
        totals.decree_casts_or_cycles > 0,
        "Decree of Pain never cast or cycled"
    );
    assert!(totals.estate_casts > 0, "Overgrown Estate never cast");
}
