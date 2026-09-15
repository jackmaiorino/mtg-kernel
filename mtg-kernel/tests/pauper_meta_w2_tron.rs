//! Pauper meta wave 2, Task 2: the three Urza lands and the conditional
//! per-tap mana yield they teach the automatic payment planner.
//!
//! The bounded rules oracle is the Mage fork at `72a08a3b`:
//! `UrzasTower.java` blob `33660e89813db8b1a2d5938c2d4fddee78c8431e`,
//! `UrzasPowerPlant.java` blob `1ed40cbcc9c0d70dbba0fe7c8242d50247cb35ad`,
//! `UrzasMine.java` blob `f4b4470351a99866a29f4bfb47122caeb798e1e2`, and the
//! shared dynamic value they all read,
//! `Mage/src/main/java/mage/abilities/dynamicvalue/common/UrzaTerrainValue.java`
//! blob `57fa2b3f0ce2f8c8e60d68e2dfe9561e2578d71d`. Together they establish:
//! each piece is a Land with the Urza's supertype-style subtype pair and
//! `{T}: Add {C}`, replaced by `{C}{C}{C}` (Tower) or `{C}{C}` (Mine, Power
//! Plant) when its controller controls at least one permanent of each of the
//! two *other* pieces. `UrzaTerrainValue.calculate` never counts the source
//! for its own piece, so a second copy of the same piece does not assemble
//! Tron.
//!
//! This file also carries the controller-mandated invariance evidence for
//! the planner refactor: `payment_plans_for_the_active_nine_are_unchanged_by_the_yield_refactor`
//! compares every active runtime deck's automatic payment plans against a
//! fixture generated from the pre-refactor build.

use mtg_kernel::card_def::{card_id_by_name, Subtype, CARD_DEFS};
use mtg_kernel::engine::{self, Action, Decision};
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::{self, Cost, ManaColor, PaymentPlan};
use mtg_kernel::rl::build_deck_pair_state;
use mtg_kernel::runtime_decks::RUNTIME_DECKS;
use mtg_kernel::state::{Counters, GameObject, GameState, Step, Zone};
use serde_json::{json, Value};

// ---------------------------------------------------------------------
// Helpers, copied from `tests/pauper_meta_w1_snuff_out_and_duals.rs:21-90`
// ---------------------------------------------------------------------

fn card_id(name: &str) -> u16 {
    card_id_by_name(name).unwrap_or_else(|| panic!("{name} in CARD_DEFS"))
}

fn card_name(card_def: u16) -> String {
    CARD_DEFS[card_def as usize].name.to_string()
}

fn put_object(state: &mut GameState, player: PlayerId, name: &str, zone: Zone) -> ObjectId {
    let card_def = card_id(name);
    let id = state.objects.push(GameObject {
        card_def,
        name: name.to_string(),
        owner: player,
        controller: player,
        zone,
        tapped: false,
        summoning_sick: false,
        damage: 0,
        counters: Counters::default(),
        attachments: Vec::new(),
        v4: mtg_kernel::state::ObjectStateV4::from_card_def(card_def),
        spell_copy_origin: None,
        plotted_turn: None,
        zone_change_count: 0,
    });
    match zone {
        Zone::Hand => state.players[player.index()].hand.push(id),
        Zone::Battlefield => state.players[player.index()].battlefield.push(id),
        Zone::Library => state.players[player.index()].library.push(id),
        Zone::Graveyard => state.players[player.index()].graveyard.push(id),
        Zone::Exile => state.exile.push(id),
        Zone::Command => state.command.push(id),
        Zone::Stack => panic!("test helper does not construct stack objects"),
    }
    id
}

/// Main1, P0 active with priority, both libraries as given.
fn ready_main1(p0_library: &[&str], p1_library: &[&str]) -> GameState {
    let p0_defs = p0_library
        .iter()
        .map(|name| card_id(name))
        .collect::<Vec<_>>();
    let p1_defs = p1_library
        .iter()
        .map(|name| card_id(name))
        .collect::<Vec<_>>();
    let mut state =
        GameState::new_from_libraries(&p0_defs, &p1_defs, card_name, 0x54_52_4f_4e_57_32_5f_32);
    state.step = Step::Main1;
    state.active_player = PlayerId::P0;
    state.priority_player = PlayerId::P0;
    state
}

fn colorless(state: &GameState) -> u8 {
    state.players[PlayerId::P0.index()].mana_pool[ManaColor::C.pool_index()]
}

/// An assembled Tron under P0's control, with the Tower pushed first so the
/// automatic payment planner (which walks `PlayerState::battlefield` in
/// order) reaches the three-yield piece before the two-yield ones.
fn assembled_tron(state: &mut GameState) -> (ObjectId, ObjectId, ObjectId) {
    let tower = put_object(state, PlayerId::P0, "Urza's Tower", Zone::Battlefield);
    let mine = put_object(state, PlayerId::P0, "Urza's Mine", Zone::Battlefield);
    let power_plant = put_object(state, PlayerId::P0, "Urza's Power Plant", Zone::Battlefield);
    (tower, mine, power_plant)
}

fn tapped_lands(state: &GameState) -> usize {
    state.players[PlayerId::P0.index()]
        .battlefield
        .iter()
        .filter(|&&id| state.objects.get(id).tapped)
        .count()
}

// ---------------------------------------------------------------------
// Active-nine payment-plan invariance (brief Step 6)
// ---------------------------------------------------------------------

/// Path of the checked-in fixture, relative to the repository root. Used
/// only by the fixture-writing mode below; the assertion path reads the
/// compiled-in copy so a committed fixture can never drift from the test.
const PAYMENT_FIXTURE_PATH: &str = "data/pauper_meta_w2_active_nine_payment_plans_v1.json";

const PAYMENT_FIXTURE_JSON: &str =
    include_str!("../../data/pauper_meta_w2_active_nine_payment_plans_v1.json");

/// Environment switch that regenerates the fixture instead of asserting
/// against it. Set exactly once, on the pre-refactor build, before
/// `mana.rs` gained `yield_per_tap`/`surplus`; the committed fixture is
/// therefore a genuine before-and-after comparison, not a snapshot of the
/// behavior the refactor happens to produce.
const PAYMENT_FIXTURE_WRITE_ENV: &str = "MTG_KERNEL_W2_WRITE_PAYMENT_FIXTURE";

/// Four deck-pair seeds per deck. Each seed produces a different shuffle,
/// therefore a different opening hand and a different set of battlefield
/// sources, so the fixture covers many distinct solver assignments rather
/// than one.
const FIXTURE_SEEDS: [u64; 4] = [0x5701, 0x5702, 0x5703, 0x5704];

/// Two floating-pool variants per (deck, seed): empty, and one mana of
/// every color. The second exercises `PaymentPlan::pool_used` and the
/// pool-before-tapping preference that the refactor must not disturb.
const FIXTURE_POOLS: [[u8; 6]; 2] = [[0; 6], [1, 1, 1, 1, 1, 1]];

/// How many automatic payment sources are placed on the battlefield.
const FIXTURE_SOURCES_ON_BATTLEFIELD: usize = 8;

fn color_code(color: ManaColor) -> &'static str {
    match color {
        ManaColor::W => "W",
        ManaColor::U => "U",
        ManaColor::B => "B",
        ManaColor::R => "R",
        ManaColor::G => "G",
        ManaColor::C => "C",
    }
}

fn plan_to_value(plan: Option<&PaymentPlan>) -> Value {
    match plan {
        None => Value::Null,
        Some(plan) => json!({
            "taps": plan
                .taps
                .iter()
                .map(|(id, color)| json!([id.0, color_code(*color)]))
                .collect::<Vec<_>>(),
            "pool_used": plan.pool_used.to_vec(),
            "life_paid": plan.life_paid,
        }),
    }
}

/// Deterministic mid-game state for one deck: the environment-seeded
/// opening deal, then the first `FIXTURE_SOURCES_ON_BATTLEFIELD` automatic
/// payment sources in library order moved onto the battlefield untapped and
/// not summoning sick (so mana creatures and tapped-entry lands are real
/// sources), then the requested floating pool.
fn fixture_state(deck_card_ids: &[u16], seed: u64, pool: [u8; 6]) -> GameState {
    let mut state = build_deck_pair_state(seed, deck_card_ids, deck_card_ids)
        .expect("every active runtime deck is fully supported");
    let mut moved = 0usize;
    while moved < FIXTURE_SOURCES_ON_BATTLEFIELD {
        let position = state.players[PlayerId::P0.index()]
            .library
            .iter()
            .position(|&id| {
                CARD_DEFS[state.objects.get(id).card_def as usize].is_automatic_payment_mana_source()
            });
        let Some(position) = position else {
            break;
        };
        let id = state.players[PlayerId::P0.index()].library.remove(position);
        state.objects.get_mut(id).zone = Zone::Hand;
        state.players[PlayerId::P0.index()].hand.push(id);
        assert!(
            state.move_hand_to_battlefield(PlayerId::P0, id),
            "the source was just placed in hand"
        );
        let object = state.objects.get_mut(id);
        object.tapped = false;
        object.summoning_sick = false;
        moved += 1;
    }
    state.players[PlayerId::P0.index()].mana_pool = pool;
    state
}

/// Builds the live payment-plan table for the nine active runtime decks and
/// asserts, per plan, that the refactor left no surplus behind for any
/// single-yield source.
fn active_nine_payment_rows() -> Value {
    let mut rows: Vec<Value> = Vec::new();
    for deck in RUNTIME_DECKS {
        for (seed_index, &seed) in FIXTURE_SEEDS.iter().enumerate() {
            for (pool_index, &pool) in FIXTURE_POOLS.iter().enumerate() {
                let state = fixture_state(deck.card_ids, seed, pool);
                let hand = state.players[PlayerId::P0.index()].hand.clone();
                for (hand_index, &id) in hand.iter().enumerate() {
                    let def = &CARD_DEFS[state.objects.get(id).card_def as usize];
                    if !def.is_castable() {
                        continue;
                    }
                    let plan = mana::can_pay(&def.cost, 0, PlayerId::P0, &state);
                    if let Some(plan) = plan.as_ref() {
                        assert_eq!(
                            plan.surplus,
                            [0u8; 6],
                            "no active-nine source has a yield above one, so {} in {} must float nothing",
                            def.name,
                            deck.id
                        );
                    }
                    rows.push(json!({
                        "deck": deck.id,
                        "seed_index": seed_index,
                        "pool_index": pool_index,
                        "hand_index": hand_index,
                        "card": def.name,
                        "plan": plan_to_value(plan.as_ref()),
                    }));
                }
            }
        }
    }
    json!({
        "schema": "kernel_payment_plan_invariance/v1",
        "note": concat!(
            "Automatic mana payment plans for every castable opening-hand card of the nine ",
            "active runtime decks, recorded from the build immediately before pauper meta ",
            "wave 2 Task 2 taught mana::ManaSource a per-tap yield. Regenerating this file ",
            "from a later build would destroy the before-and-after comparison it exists for."
        ),
        "rows": rows,
    })
}

#[test]
fn payment_plans_for_the_active_nine_are_unchanged_by_the_yield_refactor() {
    let live = active_nine_payment_rows();

    if std::env::var(PAYMENT_FIXTURE_WRITE_ENV).as_deref() == Ok("1") {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("the crate directory has a repository-root parent")
            .join(PAYMENT_FIXTURE_PATH);
        std::fs::write(
            &path,
            serde_json::to_string_pretty(&live).expect("the live table serializes") + "\n",
        )
        .expect("the fixture path is writable");
        return;
    }

    let recorded: Value =
        serde_json::from_str(PAYMENT_FIXTURE_JSON).expect("the fixture is valid JSON");
    assert_eq!(
        recorded["schema"], live["schema"],
        "fixture schema must match the live table's"
    );

    let live_rows = live["rows"].as_array().expect("live rows are an array");
    let recorded_rows = recorded["rows"]
        .as_array()
        .expect("fixture rows are an array");

    // A fixture that recorded nothing (or nothing payable) would assert
    // nothing, so the invariance claim is gated on real coverage first.
    let payable = live_rows
        .iter()
        .filter(|row| !row["plan"].is_null())
        .count();
    let with_taps = live_rows
        .iter()
        .filter(|row| {
            row["plan"]["taps"]
                .as_array()
                .is_some_and(|taps| !taps.is_empty())
        })
        .count();
    assert!(
        live_rows.len() >= 300 && payable >= 100 && with_taps >= 100,
        "the invariance table must be non-vacuous: {} rows, {payable} payable, {with_taps} tapping",
        live_rows.len()
    );

    assert_eq!(
        live_rows.len(),
        recorded_rows.len(),
        "the live table and the pre-refactor fixture must describe the same rows"
    );
    for (live_row, recorded_row) in live_rows.iter().zip(recorded_rows.iter()) {
        assert_eq!(
            live_row, recorded_row,
            "an existing deck's payment plan moved: live {live_row}, pre-refactor {recorded_row}"
        );
    }
}

// ---------------------------------------------------------------------
// The three Urza lands (brief Step 3)
// ---------------------------------------------------------------------

// covers: Urza's Tower: adds_one_colorless_without_the_other_pieces
#[test]
fn urzas_tower_adds_one_colorless_without_the_other_two_pieces() {
    let mut state = ready_main1(&["Forest"; 8], &["Forest"; 8]);
    let tower = put_object(&mut state, PlayerId::P0, "Urza's Tower", Zone::Battlefield);

    let def = &CARD_DEFS[card_id("Urza's Tower") as usize];
    assert!(def.is_land, "Urza's Tower is a land");
    assert_eq!(def.subtypes, &[Subtype::Urzas, Subtype::Tower]);
    assert_eq!(def.produces_mana, &[ManaColor::C]);
    assert!(
        def.is_automatic_payment_mana_source(),
        "a conditional yield must not exile the piece from the automatic payment path"
    );

    engine::step(&mut state, Action::ActivateManaAbility(tower)).unwrap();
    assert!(state.objects.get(tower).tapped);
    assert_eq!(
        state.players[PlayerId::P0.index()].mana_pool,
        [0, 0, 0, 0, 0, 1],
        "a lone Tower adds exactly one colorless"
    );
}

// covers: Urza's Tower: adds_three_colorless_with_assembled_tron
// covers: Urza's Mine: adds_two_colorless_with_assembled_tron
// covers: Urza's Power Plant: adds_two_colorless_with_assembled_tron
#[test]
fn assembled_tron_makes_tower_add_three_and_mine_and_power_plant_add_two() {
    let mut state = ready_main1(&["Forest"; 8], &["Forest"; 8]);
    let (tower, mine, power_plant) = assembled_tron(&mut state);

    engine::step(&mut state, Action::ActivateManaAbility(tower)).unwrap();
    assert_eq!(colorless(&state), 3, "the Tower adds {{C}}{{C}}{{C}}");

    engine::step(&mut state, Action::ActivateManaAbility(mine)).unwrap();
    assert_eq!(colorless(&state), 5, "the Mine adds {{C}}{{C}}");

    engine::step(&mut state, Action::ActivateManaAbility(power_plant)).unwrap();
    assert_eq!(colorless(&state), 7, "the Power Plant adds {{C}}{{C}}");

    assert_eq!(
        state.players[PlayerId::P0.index()].mana_pool,
        [0, 0, 0, 0, 0, 7],
        "assembled Tron makes seven colorless from three lands"
    );
}

// covers: Urza's Tower: a_second_copy_of_the_same_piece_does_not_assemble_tron
// covers: Urza's Mine: adds_one_colorless_without_the_other_pieces
#[test]
fn a_second_copy_of_the_same_piece_does_not_assemble_tron() {
    let mut state = ready_main1(&["Forest"; 8], &["Forest"; 8]);
    let first_tower = put_object(&mut state, PlayerId::P0, "Urza's Tower", Zone::Battlefield);
    let second_tower = put_object(&mut state, PlayerId::P0, "Urza's Tower", Zone::Battlefield);
    let mine = put_object(&mut state, PlayerId::P0, "Urza's Mine", Zone::Battlefield);

    // `UrzaTerrainValue` requires one of each of the two *other* pieces, and
    // no permanent here is an Urza's Power-Plant. So neither Tower may
    // stand in as the other's missing Power-Plant, and the Mine, which
    // needs a Tower and a Power-Plant, is short the Power-Plant too.
    engine::step(&mut state, Action::ActivateManaAbility(first_tower)).unwrap();
    assert_eq!(colorless(&state), 1);
    engine::step(&mut state, Action::ActivateManaAbility(second_tower)).unwrap();
    assert_eq!(colorless(&state), 2);
    engine::step(&mut state, Action::ActivateManaAbility(mine)).unwrap();
    assert_eq!(
        colorless(&state),
        3,
        "three unassembled pieces make three colorless, not seven"
    );
}

// covers: Urza's Tower: one_tap_pays_a_three_generic_cost_automatically
#[test]
fn automatic_payment_taps_one_tower_for_a_three_generic_spell() {
    let mut state = ready_main1(&["Forest"; 8], &["Forest"; 8]);
    let (tower, _mine, _power_plant) = assembled_tron(&mut state);

    // The planner's own view: three generic is one tap of the Tower, with
    // nothing left over.
    let three_generic = Cost {
        pips: &[],
        generic: 3,
        x_count: 0,
    };
    let plan = mana::can_pay(&three_generic, 0, PlayerId::P0, &state)
        .expect("assembled Tron pays a three-generic cost");
    assert_eq!(
        plan.taps,
        vec![(tower, ManaColor::C)],
        "one tap of the Tower covers all three generic"
    );
    assert_eq!(plan.surplus, [0; 6], "nothing is left floating");
    assert_eq!(plan.pool_used, [0; 6]);

    // And the real cast path: Myr Enforcer's {7} (no artifacts on the
    // battlefield, so Affinity reduces nothing) is exactly the three
    // pieces' assembled yield.
    let enforcer = put_object(&mut state, PlayerId::P0, "Myr Enforcer", Zone::Hand);
    match engine::advance_until_decision(&mut state) {
        Decision::CastSpellOrPass {
            castable_spells, ..
        } => assert!(
            castable_spells.contains(&enforcer),
            "three Tron lands alone cast a seven-drop"
        ),
        other => panic!("expected CastSpellOrPass, got {other:?}"),
    }
    engine::step(&mut state, Action::CastSpell(enforcer)).unwrap();
    // The cast finalizes (and therefore pays) inside the engine's own
    // advance, exactly as `tests/pauper_meta_w1_snuff_out_and_duals.rs`
    // observes. Stopping at the very next decision keeps the spell on the
    // stack and the step open, so the mana pool is still observable.
    engine::advance_until_decision(&mut state);
    assert_eq!(state.stack.len(), 1, "the spell is on the stack");
    assert_eq!(tapped_lands(&state), 3, "all three pieces paid");
    assert_eq!(
        state.players[PlayerId::P0.index()].mana_pool,
        [0; 6],
        "seven produced, seven spent"
    );
}

// covers: Urza's Tower: unspent_yield_floats_as_colorless_mana
#[test]
fn surplus_tron_mana_floats_after_a_cheaper_spell() {
    let mut state = ready_main1(&["Forest"; 8], &["Forest"; 8]);
    let (tower, mine, power_plant) = assembled_tron(&mut state);
    let spellbomb = put_object(&mut state, PlayerId::P0, "Nihil Spellbomb", Zone::Hand);

    engine::step(&mut state, Action::CastSpell(spellbomb)).unwrap();
    engine::advance_until_decision(&mut state);
    assert_eq!(state.stack.len(), 1, "the artifact spell is on the stack");

    assert!(state.objects.get(tower).tapped, "the Tower paid");
    assert!(!state.objects.get(mine).tapped, "one tap was enough");
    assert!(!state.objects.get(power_plant).tapped, "one tap was enough");
    assert_eq!(
        state.players[PlayerId::P0.index()].mana_pool,
        [0, 0, 0, 0, 0, 2],
        "the Tower added three, the {{1}} artifact spent one, two float"
    );
}

// covers: Urza's Tower: yield_returns_to_one_after_a_piece_leaves
// covers: Urza's Power Plant: adds_one_colorless_without_the_other_pieces
#[test]
fn losing_a_piece_drops_the_yield_back_to_one() {
    let mut state = ready_main1(&["Forest"; 8], &["Forest"; 8]);
    let (tower, mine, power_plant) = assembled_tron(&mut state);

    // The amount is sampled at activation, never stamped onto the land, so
    // moving the Mine off the battlefield through the ordinary commit
    // pipeline is enough to unassemble Tron.
    event::propose_and_commit(&mut state, ProposedEvent::zone_change(mine, Zone::Graveyard));
    assert_eq!(state.objects.get(mine).zone, Zone::Graveyard);

    engine::step(&mut state, Action::ActivateManaAbility(tower)).unwrap();
    assert_eq!(colorless(&state), 1, "the Tower is back to one colorless");
    engine::step(&mut state, Action::ActivateManaAbility(power_plant)).unwrap();
    assert_eq!(
        colorless(&state),
        2,
        "the Power Plant is back to one colorless"
    );
}
