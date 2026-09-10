//! Focused rules coverage for a conditional alternative cost (Snuff Out)
//! and two typed dual lands (Contaminated Aquifer, Ice Tunnel).
//!
//! The bounded rules oracle is the Mage fork at `72a08a3b`:
//! `SnuffOut.java` blob `c5b936910baa1f461e8f84f0e3df1b695a19aeac`,
//! `ContaminatedAquifer.java` blob `4be81c58b93cce245819c116ee5f612c98328f85`,
//! `IceTunnel.java` blob `c0c4c7bfe83d17930f864bbd472d7e8d7b138af8`. Together
//! they establish: Snuff Out destroys target nonblack creature, and if its
//! controller controls a Swamp they may pay 4 life rather than pay its
//! {3}{B} mana cost; Contaminated Aquifer is a Land - Island Swamp that
//! enters tapped and taps for U or B; Ice Tunnel is the same, plus the Snow
//! supertype.

use mtg_kernel::card_def::{card_id_by_name, Supertype, CARD_DEFS};
use mtg_kernel::engine::{self, Action, CastMode, Decision};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::ManaColor;
use mtg_kernel::state::{Counters, GameObject, GameState, Step, Target, Zone};

fn card_id(name: &str) -> u16 {
    card_id_by_name(name).unwrap_or_else(|| panic!("{name} in CARD_DEFS"))
}

fn card_name(card_def: u16) -> String {
    CARD_DEFS[card_def as usize].name.to_string()
}

// Copied verbatim from `tests/deep_analysis.rs:24-60` / `tests/pauper_meta_w1_spells.rs`.
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

/// Mirrors `pauper_meta_w1_spells.rs`'s `ready_main1`: Main1, P0 active with
/// priority, both libraries as given.
fn ready_main1(p0_library: &[&str], p1_library: &[&str]) -> GameState {
    let p0_defs = p0_library.iter().map(|name| card_id(name)).collect::<Vec<_>>();
    let p1_defs = p1_library.iter().map(|name| card_id(name)).collect::<Vec<_>>();
    let mut state =
        GameState::new_from_libraries(&p0_defs, &p1_defs, card_name, 0x53_4e_55_46_46_4f_55_54);
    state.step = Step::Main1;
    state.active_player = PlayerId::P0;
    state.priority_player = PlayerId::P0;
    state
}

/// `pass_until_stack_len(state, 0)`, copied from `pauper_meta_w1_spells.rs`.
fn pass_until_stack_len(state: &mut GameState, wanted: usize) -> Decision {
    for _ in 0..16 {
        let decision = engine::advance_until_decision(state);
        if state.stack.len() <= wanted {
            return decision;
        }
        match decision {
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            Decision::GameOver { .. } | Decision::Halted { .. } => return decision,
            other => panic!("unexpected decision while resolving the stack: {other:?}"),
        }
    }
    panic!("stack did not reach length {wanted}")
}

fn pass_until_stack_empty(state: &mut GameState) -> Decision {
    pass_until_stack_len(state, 0)
}

/// Advances (declining every attack/block and passing every priority window
/// along the way) until `player`'s *next* Main1. Copied from
/// `tests/pauper_meta_w1_black_red.rs`.
fn advance_to_players_next_main1(state: &mut GameState, player: PlayerId) {
    let starting_turn = state.turn;
    loop {
        match engine::advance_until_decision(state) {
            Decision::CastSpellOrPass { player: p, .. }
                if state.turn > starting_turn
                    && state.step == Step::Main1
                    && state.active_player == player
                    && p == player =>
            {
                return;
            }
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            Decision::DeclareAttackers { .. } => {
                engine::step(state, Action::DeclareAttackers(Vec::new())).unwrap();
            }
            Decision::DeclareBlockers { .. } => {
                engine::step(state, Action::DeclareBlockers(Vec::new())).unwrap();
            }
            other => panic!("unexpected decision while advancing to the next Main1: {other:?}"),
        }
    }
}

#[test]
fn snuff_out_offers_the_life_payment_only_with_a_swamp() {
    let mut state = ready_main1(&["Island"; 8], &["Island"; 8]);
    let snuff_out = put_object(&mut state, PlayerId::P0, "Snuff Out", Zone::Hand);
    put_object(&mut state, PlayerId::P0, "Island", Zone::Battlefield);
    let victim = put_object(&mut state, PlayerId::P1, "Faerie Seer", Zone::Battlefield);
    let black_creature = put_object(&mut state, PlayerId::P1, "Gixian Infiltrator", Zone::Battlefield);
    state.players[0].life = 20;

    // Only a single Island: the printed {3}{B} cost is unpayable (not even
    // enough total mana), and there's no Swamp for the alternative cost --
    // Snuff Out is not castable at all.
    match engine::advance_until_decision(&mut state) {
        Decision::CastSpellOrPass {
            castable_spells, ..
        } => {
            assert!(
                !castable_spells.contains(&snuff_out),
                "no Swamp and not enough mana"
            );
        }
        other => panic!("expected CastSpellOrPass, got {other:?}"),
    }

    // Add a Swamp: the alternative cost's condition is now met, so Snuff
    // Out becomes castable even though there still isn't enough mana for
    // its printed cost.
    put_object(&mut state, PlayerId::P0, "Swamp", Zone::Battlefield);
    match engine::advance_until_decision(&mut state) {
        Decision::CastSpellOrPass {
            castable_spells, ..
        } => {
            assert!(
                castable_spells.contains(&snuff_out),
                "a Swamp unlocks the alternative cost"
            );
        }
        other => panic!("expected CastSpellOrPass, got {other:?}"),
    }

    engine::step(&mut state, Action::CastSpell(snuff_out)).unwrap();
    match engine::advance_until_decision(&mut state) {
        Decision::ChooseTargets { legal_targets, .. } => {
            assert!(legal_targets.contains(&Target::Object(victim)));
            assert!(
                !legal_targets.contains(&Target::Object(black_creature)),
                "Gixian Infiltrator is black; Snuff Out can't target it"
            );
        }
        other => panic!("expected ChooseTargets, got {other:?}"),
    }
    engine::step(&mut state, Action::ChooseTarget(Target::Object(victim))).unwrap();

    // Only the alternative cost is payable (still not enough mana for the
    // printed cost), so `ChooseCastMode` auto-resolves without a decision --
    // same "single payable mode" rule Fireblast's own tests document.
    pass_until_stack_empty(&mut state);

    assert_eq!(
        state.players[0].life, 16,
        "paid 4 life for the alternative cost"
    );
    assert_eq!(state.objects.get(victim).zone, Zone::Graveyard);
    assert_eq!(
        state.players[0].mana_pool,
        [0; 6],
        "no mana spent through the alternative cost"
    );
    assert!(
        state.players[0]
            .battlefield
            .iter()
            .all(|&id| !state.objects.get(id).tapped),
        "no land tapped for the alternative cost"
    );
}

#[test]
fn snuff_out_still_casts_for_mana_without_a_swamp() {
    let mut state = ready_main1(&["Island"; 8], &["Island"; 8]);
    let snuff_out = put_object(&mut state, PlayerId::P0, "Snuff Out", Zone::Hand);
    let victim = put_object(&mut state, PlayerId::P1, "Faerie Seer", Zone::Battlefield);
    let starting_life = state.players[0].life;
    // Four mana floating ({3}{B}) rather than four actual lands: the point
    // under test is the printed-cost path being unaffected by the missing
    // Swamp, not the land-tapping subsystem (covered generically
    // elsewhere). No Swamp is present, so the alternative cost's condition
    // never holds.
    state.players[0].mana_pool[ManaColor::B.pool_index()] = 1;
    state.players[0].mana_pool[ManaColor::C.pool_index()] = 3;

    match engine::advance_until_decision(&mut state) {
        Decision::CastSpellOrPass {
            castable_spells, ..
        } => {
            assert!(castable_spells.contains(&snuff_out));
        }
        other => panic!("expected CastSpellOrPass, got {other:?}"),
    }
    engine::step(&mut state, Action::CastSpell(snuff_out)).unwrap();
    engine::step(&mut state, Action::ChooseTarget(Target::Object(victim))).unwrap();
    // Only the printed cost is payable (no Swamp), so `ChooseCastMode`
    // auto-resolves to `CastMode::Normal`.
    pass_until_stack_empty(&mut state);

    assert_eq!(state.objects.get(victim).zone, Zone::Graveyard);
    assert_eq!(
        state.players[0].life, starting_life,
        "no life paid: cast for its printed mana cost"
    );
    assert_eq!(state.players[0].mana_pool, [0; 6], "all floating mana spent");
    let _ = CastMode::Normal; // documents which mode this test exercises
}

#[test]
fn contaminated_aquifer_enters_tapped_and_taps_for_u_or_b() {
    let mut state = ready_main1(&["Island"; 8], &["Island"; 8]);
    let land = put_object(&mut state, PlayerId::P0, "Contaminated Aquifer", Zone::Hand);
    engine::step(&mut state, Action::PlayLand(land)).unwrap();
    assert_eq!(state.objects.get(land).zone, Zone::Battlefield);
    assert!(state.objects.get(land).tapped, "enters the battlefield tapped");

    advance_to_players_next_main1(&mut state, PlayerId::P0);
    assert!(
        !state.objects.get(land).tapped,
        "untapped by P0's own next untap step"
    );

    engine::step(&mut state, Action::ActivateManaAbilityChoice(land, ManaColor::U)).unwrap();
    assert!(state.objects.get(land).tapped);
    assert_eq!(state.players[0].mana_pool[ManaColor::U.pool_index()], 1);

    state.objects.get_mut(land).tapped = false;
    engine::step(&mut state, Action::ActivateManaAbilityChoice(land, ManaColor::B)).unwrap();
    assert!(state.objects.get(land).tapped);
    assert_eq!(state.players[0].mana_pool[ManaColor::B.pool_index()], 1);
}

#[test]
fn ice_tunnel_is_snow() {
    let id = card_id("Ice Tunnel");
    assert!(
        CARD_DEFS[id as usize].supertypes.contains(&Supertype::Snow),
        "Ice Tunnel is a Snow land"
    );
    assert_eq!(
        CARD_DEFS[id as usize].subtypes,
        &[
            mtg_kernel::card_def::Subtype::Island,
            mtg_kernel::card_def::Subtype::Swamp
        ]
    );
}
