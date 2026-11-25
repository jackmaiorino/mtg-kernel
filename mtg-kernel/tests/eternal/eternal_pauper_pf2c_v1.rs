//! Pauper field wave Pf2c: native rules and public-session tests for the three
//! new-piece cards:
//! - Benevolent Unicorn (CR 614.1a spell damage replacement effect, minus 1)
//! - Boulderbranch Golem (CR 718 Prototype {3}{G} -- 3/3 + ETB LKI life gain)
//! - Inside Out (CR 613.4d Layer 7d P/T switch until end of turn + draw 1)
#![cfg(feature = "eternal-fixtures")]
#![allow(dead_code)]

use crate::eternal_catalog_v1::{play_observed, Policy};
use mtg_kernel::card_def::{
    card_id_by_name, mana_color_mask, CardCapability, CardDef, CardType, Subtype, CARD_DEFS,
};
use mtg_kernel::engine::{self, Action, CastMode, Decision, OptionalCostChoice};
use mtg_kernel::eternal_cards_v1::EternalStateV1;
use mtg_kernel::eternal_pauper_pf2c_v1::Pf2cStateV1;
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::limited_session_v1::{CustomCardCountV1, CustomDeckV1};
use mtg_kernel::mana::ManaColor;
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Target, Zone};
use serde_json::Value;
use std::hash::{Hash, Hasher};

const P0: PlayerId = PlayerId::P0;
const P1: PlayerId = PlayerId::P1;

const PF2C_CARDS: [&str; 3] = ["Benevolent Unicorn", "Boulderbranch Golem", "Inside Out"];

fn def(name: &str) -> &'static CardDef {
    &CARD_DEFS[id(name) as usize]
}

fn id(name: &str) -> u16 {
    card_id_by_name(name).unwrap_or_else(|| panic!("{name}"))
}

fn game(library: &[&str]) -> GameState {
    let ids: Vec<u16> = library.iter().map(|name| id(name)).collect();
    let mut state = GameState::new_from_libraries(
        &ids,
        &ids,
        |id| CARD_DEFS[id as usize].object_name.into(),
        0x5046_0000_0000 + 0x3263,
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

fn cast_with_mode(state: &mut GameState, spell: ObjectId, mode: CastMode) {
    assert!(castable(state, spell));
    act(state, Action::CastSpell(spell));
    if let Decision::ChooseCastMode { options, .. } = next(state) {
        assert!(options.contains(&mode), "{mode:?} not in {options:?}");
        act(state, Action::ChooseCastMode(mode));
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
    panic!("did not reach target state");
}

fn to_next_turn(state: &mut GameState) {
    let turn = state.turn;
    let active = state.active_player;
    run_until(
        state,
        |s| (s.turn != turn || s.active_player != active) && s.step == Step::Main1,
        |_, _| None,
    );
}

fn deck(rows: &[(&str, u32)]) -> CustomDeckV1 {
    CustomDeckV1 {
        cards: rows
            .iter()
            .map(|(name, count)| CustomCardCountV1 {
                name: (*name).to_owned(),
                count: *count,
            })
            .collect(),
        companion: None,
    }
}

// ---- Catalog & state invariants -----------------------------------------------------

#[test]
fn pf2c_cards_are_full_and_empty_state_hashes_zero_bytes() {
    for name in PF2C_CARDS {
        assert_eq!(
            def(name).capability,
            CardCapability::Full,
            "{name} must be Full"
        );
    }

    let unicorn = def("Benevolent Unicorn");
    assert_eq!(unicorn.mana_value, 2);
    assert_eq!((unicorn.power, unicorn.toughness), (Some(1), Some(2)));
    assert_eq!(unicorn.subtypes, &[Subtype::Unicorn]);

    let golem = def("Boulderbranch Golem");
    assert_eq!(golem.mana_value, 7);
    assert_eq!((golem.power, golem.toughness), (Some(6), Some(5)));
    assert_eq!(golem.types, &[CardType::Artifact, CardType::Creature]);
    assert_eq!(golem.subtypes, &[Subtype::Golem]);
    assert!(golem.alt_cost.is_some());

    let inside = def("Inside Out");
    assert_eq!(inside.mana_value, 2);
    assert_eq!(inside.types, &[CardType::Instant]);
    assert_eq!(inside.colors, &[ManaColor::U, ManaColor::R]);

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
    let empty = Pf2cStateV1::default();
    let mut counter = ByteCounter::default();
    empty.hash(&mut counter);
    assert_eq!(counter.0, 0);
    assert_eq!(serde_json::to_string(&empty).unwrap(), "{}");
    let eternal_json = serde_json::to_string(&EternalStateV1::default()).unwrap();
    assert!(!eternal_json.contains("\"pf2c\""));
}

// ---- Benevolent Unicorn (CR 614.1a spell damage replacement) ------------------------

#[test]
fn benevolent_unicorn_reduces_spell_damage_stacks_and_ignores_damage_cant_be_prevented() {
    let mut state = plains();
    let u1 = put(&mut state, P0, "Benevolent Unicorn", Zone::Battlefield);
    let creature = put(&mut state, P0, "Kor Skyfisher", Zone::Battlefield); // 2/3

    // Advance to P1's Main1 so P1 can cast both Instants and Sorceries (End the Festivities).
    to_next_turn(&mut state);
    assert_eq!(state.active_player, P1);

    // 1. Lightning Bolt (3) to player -> 2; to creature -> 2 (survives!).
    let bolt1 = put(&mut state, P1, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P1, &[ManaColor::R], 0);
    cast(&mut state, bolt1, &[Target::Player(P0)]);
    settled(&mut state);
    assert_eq!(life(&state, P0), 18, "Bolt 3 -> 2 with one Unicorn");

    let bolt2 = put(&mut state, P1, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P1, &[ManaColor::R], 0);
    cast(&mut state, bolt2, &[Target::Object(creature)]);
    settled(&mut state);
    assert_eq!(zone(&state, creature), Zone::Battlefield);
    assert_eq!(state.objects.get(creature).damage, 2);

    // 2. End the Festivities (1 to opponent and each opponent creature) -> 0 damage everywhere!
    let fest = put(&mut state, P1, "End the Festivities", Zone::Hand);
    pool(&mut state, P1, &[ManaColor::R], 0);
    cast(&mut state, fest, &[]);
    settled(&mut state);
    assert_eq!(life(&state, P0), 18);
    assert_eq!(state.objects.get(creature).damage, 2);
    assert_eq!(state.objects.get(u1).damage, 0);

    // 3. Applies to controller's own spells too, and two Unicorns stack (3 -> 1).
    let _u2 = put(&mut state, P0, "Benevolent Unicorn", Zone::Battlefield);
    state.priority_player = P0;
    let own_bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 0);
    cast(&mut state, own_bolt, &[Target::Player(P1)]);
    settled(&mut state);
    assert_eq!(life(&state, P1), 19, "two Unicorns reduce Bolt from 3 to 1");

    // 4. CR 614.1a replacement, NOT prevention: Stomp ("damage can't be prevented")
    //    deals 2 - 2 = 0 with two Unicorns, and subsequent Bolt still deals 3 - 2 = 1!
    let giant = put(&mut state, P1, "Bonecrusher Giant", Zone::Hand);
    pool(&mut state, P1, &[ManaColor::R], 1);
    cast_mode(&mut state, giant, 1, &[Target::Player(P0)]);
    settled(&mut state);
    assert_eq!(
        life(&state, P0),
        18,
        "Stomp's own 2 spell damage is replaced to 0 by two Unicorns"
    );
    let bolt3 = put(&mut state, P1, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P1, &[ManaColor::R], 0);
    cast(&mut state, bolt3, &[Target::Player(P0)]);
    settled(&mut state);
    assert_eq!(
        life(&state, P0),
        17,
        "damage can't be prevented must not suppress Benevolent Unicorn"
    );
}

#[test]
fn benevolent_unicorn_does_not_reduce_combat_or_ability_damage_and_respects_dress_down() {
    // 1. Activated ability damage (Vulshok Sorcerer ping) and combat damage are NOT reduced.
    let mut state = plains();
    let _u = put(&mut state, P1, "Benevolent Unicorn", Zone::Battlefield);
    let pinger = put(&mut state, P0, "Vulshok Sorcerer", Zone::Battlefield);
    let attacker = put(&mut state, P0, "K'un-Lun Warrior", Zone::Battlefield);
    activate(&mut state, pinger, 0, &[Target::Player(P1)]);
    settled(&mut state);
    assert_eq!(life(&state, P1), 19, "ability damage must not be reduced");

    run_until(
        &mut state,
        |s| s.step == Step::Main2,
        |_, decision| match decision {
            Decision::DeclareAttackers { .. } => Some(Action::DeclareAttackers(vec![attacker])),
            _ => None,
        },
    );
    assert_eq!(life(&state, P1), 17, "combat damage must not be reduced");

    // 2. Dress Down removes Benevolent Unicorn's static replacement ability.
    let dress = put(&mut state, P0, "Dress Down", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 1);
    cast(&mut state, dress, &[]);
    settled(&mut state);
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 0);
    cast(&mut state, bolt, &[Target::Player(P1)]);
    settled(&mut state);
    assert_eq!(life(&state, P1), 14, "Dress Down suppresses Unicorn");
}

#[test]
fn benevolent_unicorn_orders_with_ojer_axonil_and_aurora_shield() {
    // 1. Unicorn + Ojer Axonil (power 4) on Lightning Bolt (3), EVEN under Stomp:
    //    affected player chooses Unicorn first (3->2->4) or Ojer first (3->4->3).
    let mut outcomes = Vec::new();
    for choice in [0_u16, 1] {
        let mut state = plains();
        put(&mut state, P1, "Benevolent Unicorn", Zone::Battlefield);
        put(
            &mut state,
            P0,
            "Ojer Axonil, Deepest Might",
            Zone::Battlefield,
        );
        // Disable damage prevention via Stomp first (targets Ojer so P1 stays at 20).
        let giant = put(&mut state, P0, "Bonecrusher Giant", Zone::Hand);
        let ojer = state.players[P0.index()].battlefield[0];
        pool(&mut state, P0, &[ManaColor::R], 1);
        cast_mode(&mut state, giant, 1, &[Target::Object(ojer)]);
        settled(&mut state);

        let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
        pool(&mut state, P0, &[ManaColor::R], 0);
        cast(&mut state, bolt, &[Target::Player(P1)]);
        settle_with(&mut state, |_, decision| match decision {
            Decision::ChooseEffectOption {
                player,
                option_count,
                ..
            } => {
                assert_eq!(player, P1, "affected player chooses replacement order");
                assert_eq!(option_count, 2);
                Action::ChooseEffectOption(choice)
            }
            other => first_answer(other),
        });
        outcomes.push(20 - life(&state, P1));
    }
    outcomes.sort_unstable();
    assert_eq!(
        outcomes,
        vec![3, 4],
        "affected player can choose Ojer->Unicorn (3) or Unicorn->Ojer (4)"
    );

    // 2. Unicorn + Aurora Eidolon shield (3) on Lightning Bolt (3) followed by Shock (2):
    //    - Unicorn first saves 1 charge on Aurora shield -> Shock deals 0 (2-1=1, prevented by leftover 1)!
    //    - Aurora first spends all 3 charge -> Shock deals 1 (2-1=1, 0 shield left)!
    let mut after_shock = Vec::new();
    for choice in [0_u16, 1] {
        let mut state = plains();
        put(&mut state, P1, "Benevolent Unicorn", Zone::Battlefield);
        let aurora = put(&mut state, P1, "Aurora Eidolon", Zone::Battlefield);
        state.priority_player = P1;
        pool(&mut state, P1, &[ManaColor::W], 0);
        activate(&mut state, aurora, 0, &[Target::Player(P1)]);
        settled(&mut state);

        state.priority_player = P0;
        let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
        pool(&mut state, P0, &[ManaColor::R], 0);
        cast(&mut state, bolt, &[Target::Player(P1)]);
        settle_with(&mut state, |_, decision| match decision {
            Decision::ChooseEffectOption {
                player,
                option_count,
                ..
            } => {
                assert_eq!(player, P1);
                assert_eq!(option_count, 2);
                Action::ChooseEffectOption(choice)
            }
            other => first_answer(other),
        });
        assert_eq!(life(&state, P1), 20);

        let shock = put(&mut state, P0, "Shock", Zone::Hand);
        pool(&mut state, P0, &[ManaColor::R], 0);
        cast(&mut state, shock, &[Target::Player(P1)]);
        settle_with(&mut state, |_, d| first_answer(d));
        after_shock.push(life(&state, P1));
    }
    after_shock.sort_unstable();
    assert_eq!(
        after_shock,
        vec![19, 20],
        "ordering Unicorn before Aurora preserves 1 prevention charge for Shock"
    );
}

// ---- Boulderbranch Golem (CR 718 Prototype + LKI ETB) -------------------------------

#[test]
fn boulderbranch_golem_normal_and_prototyped_profiles_cost_reduction_and_flicker() {
    let green = mana_color_mask(ManaColor::G);

    // 1. Cast prototyped ({3}{G}) -> green MV 4 on stack & battlefield, 3/3, gains 3 life.
    //    Goblin Anarchomancer reduces prototyped cost from {3}{G} to {2}{G}!
    let mut state = plains();
    put(&mut state, P0, "Goblin Anarchomancer", Zone::Battlefield);
    let beanstalk = put(&mut state, P0, "Up the Beanstalk", Zone::Battlefield);
    let golem = put(&mut state, P0, "Boulderbranch Golem", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 2); // Only {2}{G}: payable via Anarchomancer + Prototype!
    cast_with_mode(&mut state, golem, CastMode::Alternative);
    assert_eq!(
        state.stack.len(),
        1,
        "prototyped MV 4 must NOT trigger Beanstalk"
    );
    assert_eq!(engine::object_mana_value(&state, golem), 4);
    assert_eq!(engine::object_color_mask(&state, golem), green);
    settled(&mut state);
    assert_eq!(zone(&state, golem), Zone::Battlefield);
    assert_eq!(power_toughness(&state, golem), (3, 3));
    assert_eq!(engine::object_mana_value(&state, golem), 4);
    assert_eq!(engine::object_color_mask(&state, golem), green);
    assert_eq!(life(&state, P0), 23, "prototyped 3/3 ETB gains 3 life");
    assert!(state.players[0].hand.is_empty(), "Beanstalk did not draw");
    let _ = beanstalk;

    // 2. Flickering a prototyped Boulderbranch Golem with Rescue the Foal returns it as full
    //    6/5 colorless MV 7, gaining 6 life!
    let guardian = put(&mut state, P0, "Pegasus Guardian", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W], 1);
    cast_mode(&mut state, guardian, 1, &[Target::Object(golem)]);
    settled(&mut state);
    assert_eq!(zone(&state, golem), Zone::Battlefield);
    assert_eq!(power_toughness(&state, golem), (6, 5));
    assert_eq!(engine::object_mana_value(&state, golem), 7);
    assert_eq!(engine::object_color_mask(&state, golem), 0, "colorless");
    assert_eq!(life(&state, P0), 29, "flickered 6/5 ETB gains 6 life");

    // 3. Normal cast ({7}): colorless MV 7 triggers Up the Beanstalk, enters 6/5, gains 6 life.
    let mut state = plains();
    put(&mut state, P0, "Up the Beanstalk", Zone::Battlefield);
    let golem = put(&mut state, P0, "Boulderbranch Golem", Zone::Hand);
    pool(&mut state, P0, &[], 7);
    cast_with_mode(&mut state, golem, CastMode::Normal);
    assert_eq!(engine::object_mana_value(&state, golem), 7);
    assert_eq!(engine::object_color_mask(&state, golem), 0);
    settled(&mut state);
    assert_eq!(power_toughness(&state, golem), (6, 5));
    assert_eq!(life(&state, P0), 26);
    assert_eq!(
        state.players[0].hand.len(),
        1,
        "normal MV 7 cast triggers Up the Beanstalk"
    );
}

#[test]
fn boulderbranch_golem_prototype_copy_lki_etb_and_fatal_push_mv() {
    let green = mana_color_mask(ManaColor::G);

    // 1. Phyrexian Metamorph copying a prototyped 3/3 green MV 4 Golem copies its prototype
    //    values (CR 718.3c / 707.2) and gains 3 life!
    let mut state = plains();
    let golem = put(&mut state, P0, "Boulderbranch Golem", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 3);
    cast_with_mode(&mut state, golem, CastMode::Alternative);
    settled(&mut state);
    assert_eq!(life(&state, P0), 23);

    let morph = put(&mut state, P0, "Phyrexian Metamorph", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 3);
    cast(&mut state, morph, &[]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseEffectTargets { legal_targets, .. }
            if legal_targets.contains(&Target::Object(golem)) =>
        {
            Action::ChooseEffectTarget(Target::Object(golem))
        }
        other => first_answer(other),
    });
    assert_eq!(zone(&state, morph), Zone::Battlefield);
    assert_eq!(power_toughness(&state, morph), (3, 3));
    assert_eq!(engine::object_mana_value(&state, morph), 4);
    assert_eq!(engine::object_color_mask(&state, morph), green);
    assert_eq!(life(&state, P0), 26, "prototyped copy ETB gains 3 life");

    // 2. Fatal Push with revolt destroys prototyped Golem (MV 4 <= 4), NOT normal Golem (MV 7).
    let tunnel = put(&mut state, P1, "Escape Tunnel", Zone::Battlefield);
    state.priority_player = P1;
    activate(&mut state, tunnel, 0, &[]);
    settle_with(&mut state, |_, d| first_answer(d)); // Revolt enabled for P1!

    let push = put(&mut state, P1, "Fatal Push", Zone::Hand);
    state.priority_player = P1;
    pool(&mut state, P1, &[ManaColor::B], 0);
    cast(&mut state, push, &[Target::Object(golem)]);
    settled(&mut state);
    assert_eq!(
        zone(&state, golem),
        Zone::Graveyard,
        "revolt Fatal Push destroys prototyped MV 4 Golem"
    );
    // In graveyard, Boulderbranch Golem reverts to normal MV 7 colorless 6/5.
    assert_eq!(engine::object_mana_value(&state, golem), 7);
    assert_eq!(engine::object_color_mask(&state, golem), 0);

    // 3. ETB last-known power when killed before trigger resolves (pumped prototyped 3/3 +2/+2 = 5).
    let mut state = plains();
    let golem = put(&mut state, P0, "Boulderbranch Golem", Zone::Hand);
    let pump = put(&mut state, P0, "Mutagenic Growth", Zone::Hand);
    let bolt = put(&mut state, P1, "Cast Down", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 3);
    cast_with_mode(&mut state, golem, CastMode::Alternative);
    // Resolve Golem onto the battlefield -> ETB trigger goes on stack, P0 gets priority.
    run_until(
        &mut state,
        |s| zone(s, golem) == Zone::Battlefield && s.stack.len() == 1 && s.priority_player == P0,
        |_, _| None,
    );
    // P0 pumps Golem to 5/5 with Mutagenic Growth ({G}), then P1 kills it with Cast Down before ETB resolves.
    pool(&mut state, P0, &[ManaColor::G], 0);
    cast(&mut state, pump, &[Target::Object(golem)]);
    // Resolve Mutagenic Growth while ETB stays on stack.
    run_until(
        &mut state,
        |s| s.stack.len() == 1 && power_toughness(s, golem) == (5, 5) && s.priority_player == P0,
        |_, _| None,
    );
    act(&mut state, Action::Pass); // pass to P1
    pool(&mut state, P1, &[ManaColor::B], 1);
    cast(&mut state, bolt, &[Target::Object(golem)]);
    settled(&mut state);
    assert_eq!(zone(&state, golem), Zone::Graveyard);
    assert_eq!(
        life(&state, P0),
        25,
        "ETB must read last-known power (5) after Golem departed"
    );
}

// ---- Inside Out (CR 613.4d Layer 7d P/T switch) -------------------------------------

#[test]
fn inside_out_switches_pt_in_layer_7d_kills_zero_power_cancels_on_second_and_draws() {
    // 1. Tireless Tribe (1/1) + Inside Out, THEN discard 5 cards -> Layer 7c (+0/+20) applies
    //    BEFORE Layer 7d switch -> 21/1! Also draws 1 card and expires at cleanup.
    let mut state = plains();
    let tribe = put(&mut state, P0, "Tireless Tribe", Zone::Battlefield);
    let inside = put(&mut state, P0, "Inside Out", Zone::Hand);
    let mut fodder = Vec::new();
    for _ in 0..4 {
        fodder.push(put(&mut state, P0, "Plains", Zone::Hand));
    }
    pool(&mut state, P0, &[ManaColor::U], 1);
    cast(&mut state, inside, &[Target::Object(tribe)]);
    settled(&mut state);
    assert_eq!(
        state.players[0].hand.len(),
        5,
        "Inside Out drew 1 card (4 fodder + 1 drawn)"
    );
    assert_eq!(power_toughness(&state, tribe), (1, 1));

    // Discard all 5 cards to Tireless Tribe AFTER Inside Out resolved!
    for _ in 0..5 {
        activate(&mut state, tribe, 0, &[]);
        settle_with(&mut state, |_, decision| first_answer(decision));
    }
    assert!(state.players[0].hand.is_empty(), "all 5 cards discarded");
    assert_eq!(
        power_toughness(&state, tribe),
        (21, 1),
        "CR 613.4d: Layer 7c +0/+20 applies before Layer 7d switch -> 21/1"
    );

    // Second Inside Out on the same creature switches back to 1/21 (even count cancels).
    let inside2 = put(&mut state, P0, "Inside Out", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 1);
    cast(&mut state, inside2, &[Target::Object(tribe)]);
    settled(&mut state);
    assert_eq!(power_toughness(&state, tribe), (1, 21));

    // Expires at cleanup back to 1/1.
    to_next_turn(&mut state);
    assert_eq!(power_toughness(&state, tribe), (1, 1));

    // 2. Switching a 0/3 creature (Tuktuk Rubblefort) makes it 3/0 -> dies immediately to SBA 704.5f!
    let mut state = plains();
    let wall = put(&mut state, P1, "Tuktuk Rubblefort", Zone::Battlefield);
    let inside = put(&mut state, P0, "Inside Out", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 1);
    cast(&mut state, inside, &[Target::Object(wall)]);
    settled(&mut state);
    assert_eq!(
        zone(&state, wall),
        Zone::Graveyard,
        "0-power creature switched to 0 toughness dies to SBA"
    );
    assert_eq!(state.players[0].hand.len(), 1, "still drew 1 card");

    // 3. Fizzled Inside Out draws NO card.
    let mut state = plains();
    let target = put(&mut state, P1, "K'un-Lun Warrior", Zone::Battlefield);
    let inside = put(&mut state, P0, "Inside Out", Zone::Hand);
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U, ManaColor::R], 1);
    cast(&mut state, inside, &[Target::Object(target)]);
    cast(&mut state, bolt, &[Target::Object(target)]);
    settled(&mut state);
    assert_eq!(zone(&state, target), Zone::Graveyard);
    assert_eq!(
        state.players[0].hand.len(),
        0,
        "fizzled Inside Out must not draw a card"
    );
}

// ---- Public sessions ----------------------------------------------------------------

#[derive(Default)]
struct Pf2cEvents {
    plays: [usize; PF2C_CARDS.len()],
}

impl Pf2cEvents {
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
        if let Some(index) = PF2C_CARDS.iter().position(|&card| card == name) {
            self.plays[index] += 1;
        }
    }
}

#[test]
fn pf2c_decks_finish_and_replay_through_public_sessions_both_seats() {
    let bant_golem = deck(&[
        ("Plains", 8),
        ("Forest", 10),
        ("Island", 4),
        ("Benevolent Unicorn", 4),
        ("Boulderbranch Golem", 4),
        ("Inside Out", 4),
        ("Tireless Tribe", 4),
        ("Llanowar Elves", 2),
    ]);
    let izzet_burn = deck(&[
        ("Island", 8),
        ("Mountain", 12),
        ("Inside Out", 4),
        ("Lightning Bolt", 4),
        ("Shock", 4),
        ("K'un-Lun Warrior", 4),
        ("Augur of Bolas", 4),
    ]);
    let mut events = Pf2cEvents::default();
    for pair in [
        [bant_golem.clone(), izzet_burn.clone()],
        [izzet_burn, bant_golem],
    ] {
        for seed in 0x5F2C_u64..0x5F2F {
            for policy in [Policy::Ranked, Policy::Random(seed + 0x100)] {
                let played = play_observed(pair.clone(), None, seed, policy, &mut |d, a| {
                    events.observe(d, a)
                });
                let replay = play_observed(pair.clone(), None, seed, policy, &mut |_, _| {});
                assert_eq!(played.transcript_sha256, replay.transcript_sha256);
            }
        }
    }
    for (name, plays) in PF2C_CARDS.into_iter().zip(events.plays) {
        assert!(plays > 0, "{name} was never cast through a public session");
    }
}
