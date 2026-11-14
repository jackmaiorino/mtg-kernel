//! Sideboard wave S1n: native rules tests for Apostle's Blessing, Doorkeeper
//! Thrull, Darksteel Angel, From the Ashes, Kiora's Dismissal and Martyr of
//! Sands, plus public-session consumer games with replay verification.
#![cfg(feature = "eternal-fixtures")]

use crate::eternal_catalog_v1::{play_observed, Policy};
use mtg_kernel::card_def::{
    card_id_by_name, CardCapability, CardDef, CardType, Keywords, Subtype, CARD_DEFS,
};
use mtg_kernel::creature_types_v60::{canonical_native_type, CreatureTypeV60};
use mtg_kernel::engine::{self, Action, Decision};
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::limited_session_v1::{CustomCardCountV1, CustomDeckV1};
use mtg_kernel::mana::{ManaColor, Pip};
use mtg_kernel::state::{
    GameObject, GameState, ObjectStateV4, PoisonCountersV1, Step, Target, Zone,
};
use serde_json::Value;

const P0: PlayerId = PlayerId::P0;
const P1: PlayerId = PlayerId::P1;

const BLESSING: &str = "Apostle's Blessing";
const THRULL: &str = "Doorkeeper Thrull";
const ANGEL: &str = "Darksteel Angel";
const ASHES: &str = "From the Ashes";
const DISMISSAL: &str = "Kiora's Dismissal";
const MARTYR: &str = "Martyr of Sands";

fn def(name: &str) -> &'static CardDef {
    &CARD_DEFS[id(name) as usize]
}

fn id(name: &str) -> u16 {
    card_id_by_name(name).unwrap_or_else(|| panic!("{name}"))
}

/// First main phase with `active` active and holding priority, both libraries
/// twenty Islands.
fn game(active: PlayerId) -> GameState {
    let ids = vec![id("Island"); 20];
    let mut state = GameState::new_from_libraries(
        &ids,
        &ids,
        |id| CARD_DEFS[id as usize].object_name.into(),
        0x5331_4e00_0001,
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

// ---- Card Definitions ---------------------------------------------------------------

#[test]
fn s1n_definitions_and_thrull_subtype_have_printed_characteristics() {
    let blessing = def(BLESSING);
    assert_eq!(blessing.capability, CardCapability::Full);
    assert_eq!(blessing.types, &[CardType::Instant]);
    assert_eq!(blessing.colors, &[ManaColor::W]);
    assert_eq!(blessing.mana_value, 2);
    assert_eq!(blessing.cost.generic, 1);
    assert!(matches!(blessing.cost.pips, [Pip::Phyrexian(ManaColor::W)]));
    assert!(blessing.additional_cost.is_none());

    let thrull = def(THRULL);
    assert_eq!(thrull.capability, CardCapability::Full);
    assert_eq!(thrull.types, &[CardType::Creature]);
    assert_eq!(thrull.subtypes, &[Subtype::Thrull]);
    assert_eq!(Subtype::Thrull.stable_id(), 440);
    assert_eq!(
        canonical_native_type(Subtype::Thrull),
        Some(CreatureTypeV60::Thrull)
    );
    assert_eq!(thrull.power, Some(1));
    assert_eq!(thrull.toughness, Some(2));
    assert!(thrull.keywords.has(Keywords::FLASH));
    assert!(thrull.keywords.has(Keywords::FLYING));

    let angel = def(ANGEL);
    assert_eq!(angel.capability, CardCapability::Full);
    assert_eq!(angel.types, &[CardType::Artifact, CardType::Creature]);
    assert_eq!(angel.subtypes, &[Subtype::Angel]);
    assert_eq!(angel.mana_value, 9);
    assert_eq!(angel.power, Some(4));
    assert_eq!(angel.toughness, Some(4));
    assert!(angel.keywords.has(Keywords::FLYING));
    assert!(angel.keywords.has(Keywords::INDESTRUCTIBLE));
    assert!(angel.game_outcome_protection_v1.is_none());

    let ashes = def(ASHES);
    assert_eq!(ashes.capability, CardCapability::Full);
    assert_eq!(ashes.types, &[CardType::Sorcery]);
    assert_eq!(ashes.colors, &[ManaColor::R]);
    assert_eq!(ashes.mana_value, 4);

    let dismissal = def(DISMISSAL);
    assert_eq!(dismissal.capability, CardCapability::Full);
    assert_eq!(dismissal.types, &[CardType::Instant]);
    assert_eq!(dismissal.colors, &[ManaColor::U]);
    assert_eq!(dismissal.mana_value, 1);

    assert_eq!(def(MARTYR).capability, CardCapability::Full);
}

// ---- Apostle's Blessing -------------------------------------------------------------

#[test]
fn apostles_blessing_phyrexian_life_and_artifact_protection_unattach_and_fizzle_in_both_seats() {
    for actor in [P0, P1] {
        let opponent = actor.opponent();
        let mut state = game(actor);
        let inspector = put(&mut state, actor, "Thraben Inspector", Zone::Battlefield);
        let ring = put(&mut state, actor, "Sol Ring", Zone::Battlefield);
        let opp_bear = put(&mut state, opponent, "Thraben Inspector", Zone::Battlefield);
        let spear = put(&mut state, actor, "Shadowspear", Zone::Battlefield);
        let spellbomb = put(&mut state, opponent, "Aether Spellbomb", Zone::Battlefield);
        let blessing = put(&mut state, actor, BLESSING, Zone::Hand);

        // Attach Shadowspear to our Thraben Inspector first.
        pool(&mut state, actor, &[], 2);
        let Decision::CastSpellOrPass { .. } = next(&mut state) else {
            panic!("main phase priority");
        };
        act(&mut state, Action::ActivateAbility(spear, 0));
        let Decision::ChooseTargets { legal_targets, .. } = next(&mut state) else {
            panic!("equip target");
        };
        assert!(legal_targets.contains(&Target::Object(inspector)));
        act(&mut state, Action::ChooseTarget(Target::Object(inspector)));
        settled(&mut state);
        assert!(state.objects.get(inspector).attachments.contains(&spear));

        // At 1 life without {W}, Phyrexian life payment is illegal.
        state.players[actor.index()].life = 1;
        pool(&mut state, actor, &[], 1);
        let Decision::CastSpellOrPass {
            castable_spells, ..
        } = next(&mut state)
        else {
            panic!("priority at 1 life");
        };
        assert!(
            !castable_spells.contains(&blessing),
            "1 life without white mana cannot cast Apostle's Blessing"
        );

        // Opponent activates Aether Spellbomb targeting Inspector (sacrificing
        // Spellbomb so it is already in the graveyard on resolution); in
        // response at 4 life, actor casts Apostle's Blessing paying {1} + 2
        // life and chooses Artifacts (option 0).
        state.players[actor.index()].life = 4;
        pool(&mut state, opponent, &[ManaColor::U], 0);
        act(&mut state, Action::Pass);
        act(&mut state, Action::ActivateAbility(spellbomb, 0));
        act(&mut state, Action::ChooseTarget(Target::Object(inspector)));
        assert_eq!(zone(&state, spellbomb), Zone::Graveyard);

        act(&mut state, Action::Pass);
        act(&mut state, Action::CastSpell(blessing));
        let Decision::ChooseTargets { legal_targets, .. } = next(&mut state) else {
            panic!("blessing target");
        };
        // Controlled artifact and creature are both legal; opponent's creature is not.
        assert!(legal_targets.contains(&Target::Object(inspector)));
        assert!(legal_targets.contains(&Target::Object(ring)));
        assert!(!legal_targets.contains(&Target::Object(opp_bear)));
        act(&mut state, Action::ChooseTarget(Target::Object(inspector)));
        assert_eq!(state.players[actor.index()].life, 2);

        let Some(Decision::ChooseEffectOption { option_count, .. }) = settle(&mut state) else {
            panic!("protection quality choice");
        };
        assert_eq!(option_count, 6);
        act(&mut state, Action::ChooseEffectOption(0)); // Artifacts
        settled(&mut state);

        // Inspector stayed on the battlefield (departed Aether Spellbomb fizzled),
        // and Shadowspear fell off as a state-based action (704.5n).
        assert_eq!(zone(&state, inspector), Zone::Battlefield);
        assert!(state.objects.get(inspector).attachments.is_empty());

        // Artifact creature cannot block Inspector during combat this turn.
        let enforcer = put(&mut state, opponent, "Myr Enforcer", Zone::Battlefield);
        state.step = Step::DeclareAttackers;
        state.priority_player = actor;
        let _ = next(&mut state);
        act(&mut state, Action::DeclareAttackers(vec![inspector]));
        act(&mut state, Action::Pass);
        act(&mut state, Action::Pass);
        let Decision::DeclareBlockers { legal_blockers, .. } = next(&mut state) else {
            panic!("declare blockers");
        };
        assert!(
            !legal_blockers
                .iter()
                .any(|(attacker, blockers)| *attacker == inspector && blockers.contains(&enforcer)),
            "artifact creature cannot block creature with protection from artifacts"
        );
    }
}

#[test]
fn apostles_blessing_color_protection_blocks_red_targeting_and_expires_next_turn() {
    for actor in [P0, P1] {
        let opponent = actor.opponent();
        let mut state = game(actor);
        let ring = put(&mut state, actor, "Sol Ring", Zone::Battlefield);
        let thrull = put(&mut state, actor, THRULL, Zone::Battlefield);
        let blessing = put(&mut state, actor, BLESSING, Zone::Hand);
        let bolt_now = put(&mut state, opponent, "Lightning Bolt", Zone::Hand);
        let bolt_next = put(&mut state, opponent, "Lightning Bolt", Zone::Hand);

        pool(&mut state, actor, &[ManaColor::W], 1);
        let _ = next(&mut state);
        act(&mut state, Action::CastSpell(blessing));
        let mut d = next(&mut state);
        if let Decision::ChooseEffectOption { .. } = d {
            // Choose {W} mana over paying 2 life when both are payable.
            act(&mut state, Action::ChooseEffectOption(0));
            d = next(&mut state);
        }
        let Decision::ChooseTargets { legal_targets, .. } = d else {
            panic!("blessing target");
        };
        assert!(legal_targets.contains(&Target::Object(ring)));
        assert!(legal_targets.contains(&Target::Object(thrull)));
        act(&mut state, Action::ChooseTarget(Target::Object(thrull)));
        assert_eq!(state.players[actor.index()].life, 20);

        let Some(Decision::ChooseEffectOption { .. }) = settle(&mut state) else {
            panic!("blessing option");
        };
        act(&mut state, Action::ChooseEffectOption(4)); // Red
        settled(&mut state);

        // Opponent cannot target Thrull with Lightning Bolt this turn.
        pool(&mut state, opponent, &[ManaColor::R], 0);
        act(&mut state, Action::Pass);
        act(&mut state, Action::CastSpell(bolt_now));
        let Decision::ChooseTargets { legal_targets, .. } = next(&mut state) else {
            panic!("bolt target");
        };
        assert!(!legal_targets.contains(&Target::Object(thrull)));
        act(&mut state, Action::ChooseTarget(Target::Player(actor)));
        settled(&mut state);
        assert_eq!(zone(&state, thrull), Zone::Battlefield);

        // Advance to opponent's turn; protection from red has expired.
        while state.active_player == actor || state.step != Step::Main1 {
            match next(&mut state) {
                Decision::DeclareAttackers { .. } => {
                    act(&mut state, Action::DeclareAttackers(vec![]))
                }
                Decision::Discard { count, choices, .. } => act(
                    &mut state,
                    Action::Discard(choices[..count as usize].to_vec()),
                ),
                _ => act(&mut state, Action::Pass),
            }
        }
        pool(&mut state, opponent, &[ManaColor::R], 0);
        act(&mut state, Action::CastSpell(bolt_next));
        let Decision::ChooseTargets { legal_targets, .. } = next(&mut state) else {
            panic!("bolt target next turn");
        };
        assert!(legal_targets.contains(&Target::Object(thrull)));
        act(&mut state, Action::ChooseTarget(Target::Object(thrull)));
        settled(&mut state);
        assert_eq!(zone(&state, thrull), Zone::Graveyard);
    }
}

// ---- Doorkeeper Thrull --------------------------------------------------------------

#[test]
fn doorkeeper_thrull_suppresses_artifact_and_creature_etbs_but_keeps_enchantment_etb_and_entry_replacements(
) {
    for actor in [P0, P1] {
        let opponent = actor.opponent();
        let mut state = game(actor);
        let land = put(&mut state, opponent, "Mountain", Zone::Battlefield);
        let thrull = put(&mut state, actor, THRULL, Zone::Hand);
        let strix = put(&mut state, opponent, "Baleful Strix", Zone::Hand);
        let jewel = put(&mut state, opponent, "Coveted Jewel", Zone::Hand);
        let seas = put(&mut state, actor, "Spreading Seas", Zone::Hand);
        let moonshadow = put(&mut state, actor, "Moonshadow", Zone::Hand);

        state.active_player = opponent;
        state.priority_player = opponent;
        pool(&mut state, opponent, &[ManaColor::U, ManaColor::B], 6);
        let _ = next(&mut state);
        act(&mut state, Action::CastSpell(strix));
        // Flash Doorkeeper Thrull in response to Baleful Strix.
        pool(&mut state, actor, &[ManaColor::W], 1);
        act(&mut state, Action::Pass);
        act(&mut state, Action::CastSpell(thrull));
        settled(&mut state);

        // Strix entered after Thrull resolved, so Strix's ETB draw did not trigger.
        assert_eq!(zone(&state, thrull), Zone::Battlefield);
        assert_eq!(zone(&state, strix), Zone::Battlefield);
        assert_eq!(state.players[opponent.index()].library.len(), 20);

        // Coveted Jewel (artifact) also enters without drawing 3 cards.
        act(&mut state, Action::CastSpell(jewel));
        settled(&mut state);
        assert_eq!(zone(&state, jewel), Zone::Battlefield);
        assert_eq!(state.players[opponent.index()].library.len(), 20);

        // Simultaneous entry of Doorkeeper Thrull + Baleful Strix also suppresses Strix.
        let thrull2 = put(&mut state, actor, THRULL, Zone::Hand);
        let strix2 = put(&mut state, opponent, "Baleful Strix", Zone::Hand);
        event::propose_and_commit_batch(
            &mut state,
            vec![
                ProposedEvent::zone_change(thrull2, Zone::Battlefield),
                ProposedEvent::zone_change(strix2, Zone::Battlefield),
            ],
        );
        assert!(state.engine.pending_triggers.is_empty());

        // Back on actor's main phase: Spreading Seas (non-artifact enchantment)
        // still triggers its ETB draw, and Moonshadow's as-enters six -1/-1
        // counters replacement still applies (614.16a).
        state.active_player = actor;
        state.priority_player = actor;
        state.step = Step::Main1;
        let actor_lib_before = state.players[actor.index()].library.len();
        pool(&mut state, actor, &[ManaColor::U, ManaColor::B], 1);
        let _ = next(&mut state);
        act(&mut state, Action::CastSpell(seas));
        act(&mut state, Action::ChooseTarget(Target::Object(land)));
        settled(&mut state);
        assert_eq!(
            state.players[actor.index()].library.len(),
            actor_lib_before - 1
        );

        act(&mut state, Action::CastSpell(moonshadow));
        settled(&mut state);
        assert_eq!(zone(&state, moonshadow), Zone::Battlefield);
        assert_eq!(state.objects.get(moonshadow).counters.minus1_minus1, 6);
    }
}

// ---- Darksteel Angel ----------------------------------------------------------------

#[test]
fn darksteel_angel_prevents_loss_opponent_oracle_win_and_minus1_counters_until_disabled() {
    for actor in [P0, P1] {
        let opponent = actor.opponent();
        let mut state = game(actor);
        let angel = put(&mut state, actor, ANGEL, Zone::Battlefield);
        let ally = put(&mut state, actor, "Thraben Inspector", Zone::Battlefield);
        let opp_ally = put(&mut state, opponent, "Thraben Inspector", Zone::Battlefield);
        let spitter_a = put(&mut state, opponent, "Fume Spitter", Zone::Battlefield);
        let spitter_b = put(&mut state, actor, "Fume Spitter", Zone::Battlefield);
        let moonshadow = put(&mut state, actor, "Moonshadow", Zone::Hand);

        // 1. Zero/negative life, 10 poison, and empty-library draw do not make actor lose.
        state.players[actor.index()].life = -3;
        state.players[actor.index()].poison_counters = PoisonCountersV1(10);
        state.players[actor.index()].library.clear();
        event::propose_and_commit(&mut state, ProposedEvent::draw(actor));
        let decision = next(&mut state);
        assert!(
            !matches!(decision, Decision::GameOver { .. }),
            "{decision:?}"
        );
        assert!(state.players.iter().all(|player| !player.has_lost));
        assert!(!state.players[actor.index()].drew_from_empty);

        // 2. Opponent's Thassa's Oracle on an empty library cannot win the game.
        state.players[opponent.index()].library.clear();
        let oracle = put(&mut state, opponent, "Thassa's Oracle", Zone::Hand);
        state.active_player = opponent;
        state.priority_player = opponent;
        pool(&mut state, opponent, &[ManaColor::U, ManaColor::U], 0);
        let _ = next(&mut state);
        act(&mut state, Action::CastSpell(oracle));
        settled(&mut state);
        assert!(state.players.iter().all(|player| !player.has_lost));

        // 3. Controlled creatures cannot get -1/-1 counters (Fume Spitter and
        // Moonshadow entry), while opponent's creatures still get them.
        act(&mut state, Action::ActivateAbility(spitter_a, 0));
        act(&mut state, Action::ChooseTarget(Target::Object(ally)));
        settled(&mut state);
        assert_eq!(state.objects.get(ally).counters.minus1_minus1, 0);

        act(&mut state, Action::Pass);
        act(&mut state, Action::ActivateAbility(spitter_b, 0));
        act(&mut state, Action::ChooseTarget(Target::Object(opp_ally)));
        settled(&mut state);
        assert_eq!(state.objects.get(opp_ally).counters.minus1_minus1, 1);

        state.active_player = actor;
        state.priority_player = actor;
        pool(&mut state, actor, &[ManaColor::B], 0);
        let _ = next(&mut state);
        act(&mut state, Action::CastSpell(moonshadow));
        settled(&mut state);
        assert_eq!(zone(&state, moonshadow), Zone::Battlefield);
        assert_eq!(state.objects.get(moonshadow).counters.minus1_minus1, 0);
        assert_eq!(engine::effective_power(&state, moonshadow), 7);

        // 4. Dress Down removes Darksteel Angel's abilities: SBA immediately
        // sees actor at <= 0 life and 10 poison and actor loses.
        let dress_down = put(&mut state, opponent, "Dress Down", Zone::Hand);
        pool(&mut state, opponent, &[ManaColor::U], 1);
        act(&mut state, Action::Pass);
        act(&mut state, Action::CastSpell(dress_down));
        act(&mut state, Action::Pass);
        act(&mut state, Action::Pass);
        assert_eq!(zone(&state, angel), Zone::Battlefield);
        assert!(state.players[actor.index()].has_lost);
        assert!(!state.players[opponent.index()].has_lost);
    }
}

// ---- From the Ashes -----------------------------------------------------------------

#[test]
fn from_the_ashes_destroys_nonbasics_under_blood_moon_spares_indestructible_and_orders_apnap_searches(
) {
    for actor in [P0, P1] {
        let opponent = actor.opponent();
        let mut state = game(actor);
        state.players[actor.index()].library.clear();
        state.players[opponent.index()].library.clear();

        let actor_bolt = put(&mut state, actor, "Lightning Bolt", Zone::Library);
        let actor_forest = put(&mut state, actor, "Forest", Zone::Library);
        let actor_plains = put(&mut state, actor, "Plains", Zone::Library);
        let _opp_card_a = put(&mut state, opponent, "Lightning Bolt", Zone::Library);
        let _opp_card_b = put(&mut state, opponent, "Seal of Fire", Zone::Library);
        let _opp_island = put(&mut state, opponent, "Island", Zone::Library);
        let opp_order_before = state.players[opponent.index()].library.clone();

        // Actor controls 2 nonbasic lands (still nonbasic under Blood Moon),
        // 1 basic Mountain, and 1 indestructible Rustvale Bridge.
        let tomb = put(&mut state, actor, "Ancient Tomb", Zone::Battlefield);
        let volc = put(&mut state, actor, "Volcanic Island", Zone::Battlefield);
        let basic_mtn = put(&mut state, actor, "Mountain", Zone::Battlefield);
        let bridge = put(&mut state, actor, "Rustvale Bridge", Zone::Battlefield);
        let opp_volc = put(&mut state, opponent, "Volcanic Island", Zone::Battlefield);
        let _moon = put(&mut state, opponent, "Blood Moon", Zone::Battlefield);
        let ashes = put(&mut state, actor, ASHES, Zone::Hand);

        pool(&mut state, actor, &[ManaColor::R], 3);
        let _ = next(&mut state);
        act(&mut state, Action::CastSpell(ashes));

        // Active player (`actor`) is asked first in APNAP order.
        let Some(Decision::ChooseEffectOption {
            player,
            option_count,
            ..
        }) = settle(&mut state)
        else {
            panic!("active player choice");
        };
        assert_eq!(player, actor);
        assert_eq!(option_count, 2);
        assert_eq!(zone(&state, tomb), Zone::Graveyard);
        assert_eq!(zone(&state, volc), Zone::Graveyard);
        assert_eq!(zone(&state, opp_volc), Zone::Graveyard);
        assert_eq!(zone(&state, basic_mtn), Zone::Battlefield);
        assert_eq!(zone(&state, bridge), Zone::Battlefield);

        // Actor chooses to search (option 0): max_targets == 2 (Bridge survived).
        act(&mut state, Action::ChooseEffectOption(0));
        let Decision::ChooseEffectTargets {
            player: search_player,
            legal_targets,
            max_targets,
            ..
        } = next(&mut state)
        else {
            panic!("actor library search pick 1");
        };
        assert_eq!(search_player, actor);
        assert_eq!(max_targets, 2);
        assert!(legal_targets.contains(&Target::Object(actor_plains)));
        assert!(legal_targets.contains(&Target::Object(actor_forest)));
        assert!(!legal_targets.contains(&Target::Object(actor_bolt)));
        act(
            &mut state,
            Action::ChooseEffectTarget(Target::Object(actor_plains)),
        );
        if let Decision::ChooseEffectTargets { legal_targets, .. } = next(&mut state) {
            assert!(legal_targets.contains(&Target::Object(actor_forest)));
            act(
                &mut state,
                Action::ChooseEffectTarget(Target::Object(actor_forest)),
            );
        }

        // Nonactive player (`opponent`) is asked second and declines (option 1).
        let Some(Decision::ChooseEffectOption {
            player: second_player,
            option_count: second_count,
            ..
        }) = settle(&mut state)
        else {
            panic!("nonactive player choice");
        };
        assert_eq!(second_player, opponent);
        assert_eq!(second_count, 2);
        act(&mut state, Action::ChooseEffectOption(1)); // Decline search -> no shuffle.
        settled(&mut state);

        assert_eq!(zone(&state, actor_plains), Zone::Battlefield);
        assert!(!state.objects.get(actor_plains).tapped);
        assert_eq!(zone(&state, actor_forest), Zone::Battlefield);
        assert!(!state.objects.get(actor_forest).tapped);
        assert_eq!(state.players[opponent.index()].library, opp_order_before);
    }
}

// ---- Kiora's Dismissal --------------------------------------------------------------

#[test]
fn kioras_dismissal_strive_payability_omniscience_and_partial_fizzle_in_both_seats() {
    for actor in [P0, P1] {
        let opponent = actor.opponent();
        let mut state = game(actor);
        let seal_a = put(&mut state, opponent, "Seal of Fire", Zone::Battlefield);
        let seal_b = put(&mut state, opponent, "Seal of Fire", Zone::Battlefield);
        let seal_c = put(&mut state, actor, "Seal of Fire", Zone::Battlefield);
        let dismissal = put(&mut state, actor, DISMISSAL, Zone::Hand);

        // 1. With {U}{U}, actor can choose 0, 1, or 2 targets, and once 2 are
        // chosen, the 3rd enchantment is filtered out because {U}{U}{U} is unpayable.
        pool(&mut state, actor, &[ManaColor::U, ManaColor::U], 0);
        let _ = next(&mut state);
        act(&mut state, Action::CastSpell(dismissal));
        let Decision::ChooseTargets {
            legal_targets,
            can_finish,
            ..
        } = next(&mut state)
        else {
            panic!("first dismissal target");
        };
        assert!(can_finish, "0 targets is legal");
        assert_eq!(legal_targets.len(), 3);
        act(&mut state, Action::ChooseTarget(Target::Object(seal_a)));

        let Decision::ChooseTargets {
            legal_targets,
            can_finish,
            ..
        } = next(&mut state)
        else {
            panic!("second dismissal target");
        };
        assert!(can_finish, "1 target costs {{U}}");
        assert_eq!(legal_targets.len(), 2);
        act(&mut state, Action::ChooseTarget(Target::Object(seal_b)));

        if let Decision::ChooseTargets {
            legal_targets,
            can_finish,
            ..
        } = next(&mut state)
        {
            assert!(can_finish);
            assert!(
                !legal_targets.contains(&Target::Object(seal_c)),
                "3rd target requires {{U}}{{U}}{{U}}"
            );
            act(&mut state, Action::FinishEffectSelection);
        }
        settled(&mut state);
        assert_eq!(zone(&state, seal_a), Zone::Hand);
        assert_eq!(zone(&state, seal_b), Zone::Hand);
        assert_eq!(zone(&state, seal_c), Zone::Battlefield);
        assert_eq!(
            state.players[actor.index()].mana_pool[ManaColor::U.pool_index()],
            0
        );

        // 2. Partial fizzle (608.2b): target opponent's `seal_d` and actor's
        // `seal_c`; opponent responds with Veil of Summer, making `seal_d`
        // illegal (hexproof from blue) while `seal_c` still returns to hand.
        let seal_d = put(&mut state, opponent, "Seal of Fire", Zone::Battlefield);
        let dismissal2 = put(&mut state, actor, DISMISSAL, Zone::Hand);
        let veil = put(&mut state, opponent, "Veil of Summer", Zone::Hand);
        pool(&mut state, actor, &[ManaColor::U, ManaColor::U], 0);
        pool(&mut state, opponent, &[ManaColor::G], 0);
        act(&mut state, Action::CastSpell(dismissal2));
        act(&mut state, Action::ChooseTarget(Target::Object(seal_d)));
        act(&mut state, Action::ChooseTarget(Target::Object(seal_c)));
        if matches!(next(&mut state), Decision::ChooseTargets { .. }) {
            act(&mut state, Action::FinishEffectSelection);
        }
        act(&mut state, Action::Pass);
        act(&mut state, Action::CastSpell(veil));
        settled(&mut state);
        assert_eq!(zone(&state, seal_d), Zone::Battlefield);
        assert_eq!(zone(&state, seal_c), Zone::Hand);

        // 3. Under Omniscience with {U} in pool: 0 or 1 target is free, 2
        // targets costs {U} (strive surcharge still paid), and 3 targets is unpayable.
        let _omni = put(&mut state, actor, "Omniscience", Zone::Battlefield);
        let seal_e = put(&mut state, opponent, "Seal of Fire", Zone::Battlefield);
        let seal_f = put(&mut state, actor, "Seal of Fire", Zone::Battlefield);
        let dismissal3 = put(&mut state, actor, DISMISSAL, Zone::Hand);
        pool(&mut state, actor, &[ManaColor::U], 0);
        act(&mut state, Action::CastSpell(dismissal3));
        if let Decision::ChooseEffectOption { .. } = next(&mut state) {
            act(&mut state, Action::ChooseEffectOption(1)); // WithoutManaCost
        }
        act(&mut state, Action::ChooseTarget(Target::Object(seal_d)));
        act(&mut state, Action::ChooseTarget(Target::Object(seal_e)));
        if let Decision::ChooseTargets { legal_targets, .. } = next(&mut state) {
            assert!(!legal_targets.contains(&Target::Object(seal_f)));
            act(&mut state, Action::FinishEffectSelection);
        }
        settled(&mut state);
        assert_eq!(
            state.players[actor.index()].mana_pool[ManaColor::U.pool_index()],
            0
        );
    }
}

// ---- Martyr of Sands + S1n Public-Session Consumer Games ----------------------------

#[derive(Default)]
struct S1nSessionEvents {
    martyr_activations: usize,
    blessing_choices: usize,
    ashes_choices: usize,
}

impl S1nSessionEvents {
    fn observe(&mut self, _decision: &Value, action: &Value) {
        let semantic = &action["semantic"];
        let card = semantic["source"]["card_db_id"]
            .as_u64()
            .map(|id| CARD_DEFS[id as usize].name);
        match semantic["action_kind"].as_str() {
            Some("activate_ability") if card == Some(MARTYR) => {
                self.martyr_activations += 1;
            }
            Some("choose_effect_option") if card == Some(BLESSING) => {
                self.blessing_choices += 1;
            }
            Some("choose_effect_option") if card == Some(ASHES) => {
                self.ashes_choices += 1;
            }
            _ => {}
        }
    }
}

#[test]
fn s1n_and_martyr_of_sands_decks_finish_and_replay_through_public_sessions() {
    let white_artifacts = deck(&[
        ("Plains", 10),
        ("Ancient Tomb", 4),
        ("Sol Ring", 4),
        ("Mana Vault", 4),
        (MARTYR, 6),
        (THRULL, 5),
        (BLESSING, 5),
        (ANGEL, 2),
    ]);
    let izzet_ashes = deck(&[
        ("Island", 10),
        ("Mountain", 8),
        ("Volcanic Island", 4),
        ("Spreading Seas", 6),
        (DISMISSAL, 6),
        (ASHES, 6),
    ]);

    let mut totals = S1nSessionEvents::default();
    let mut cast_totals = [0usize; 6];
    let tracked = [MARTYR, THRULL, BLESSING, ANGEL, DISMISSAL, ASHES];

    for (pair, first_seed) in [
        (
            [white_artifacts.clone(), izzet_ashes.clone()],
            0x5331_4e00_u64,
        ),
        ([izzet_ashes, white_artifacts], 0x5331_4e10),
    ] {
        for seed in first_seed..first_seed + 3 {
            for policy in [
                Policy::Ranked,
                Policy::Random(seed + 0x100),
                Policy::Random(seed + 0x200),
            ] {
                let mut events = S1nSessionEvents::default();
                let game =
                    play_observed(pair.clone(), None, seed, policy, &mut |decision, action| {
                        events.observe(decision, action)
                    });
                let replay = play_observed(pair.clone(), None, seed, policy, &mut |_, _| {});
                assert_eq!(game.transcript_sha256, replay.transcript_sha256);
                for (slot, name) in cast_totals.iter_mut().zip(tracked) {
                    *slot += game
                        .cast_cards
                        .iter()
                        .map(|cards| cards.get(&id(name)).copied().unwrap_or(0))
                        .sum::<usize>();
                }
                totals.martyr_activations += events.martyr_activations;
                totals.blessing_choices += events.blessing_choices;
                totals.ashes_choices += events.ashes_choices;
            }
        }
    }

    assert!(cast_totals[0] > 0, "no Martyr of Sands cast");
    assert!(cast_totals[1] > 0, "no Doorkeeper Thrull cast");
    assert!(cast_totals[2] > 0, "no Apostle's Blessing cast");
    assert!(cast_totals[4] > 0, "no Kiora's Dismissal cast");
    assert!(cast_totals[5] > 0, "no From the Ashes cast");
    assert!(
        totals.martyr_activations > 0,
        "no Martyr of Sands activation"
    );
    assert!(
        totals.blessing_choices > 0,
        "no Apostle's Blessing option choice"
    );
    assert!(totals.ashes_choices > 0, "no From the Ashes option choice");
}
