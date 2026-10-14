//! CR 205.3m: a creature type is a word, not a spelling. The card registry
//! carries some creature types in two case spellings ("HUMAN" and "Human",
//! "FAERIE" and "Faerie", ...), and mtg-kernel compared them as different
//! types (found by shadowing mtg-kernel with the gorge engine in SpellBench
//! Faeries mirrors: Of One Mind was offered for {U} with only Humans).

use mtg_kernel::card_def::{card_id_by_name, Keywords, Subtype, CARD_DEFS};
use mtg_kernel::engine::{self, Action, Decision};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::ManaColor;
use mtg_kernel::state::{Counters, GameObject, GameState, ObjectStateV4, Step, Zone};

fn card_name(card_def: u16) -> String {
    CARD_DEFS[card_def as usize].name.to_string()
}

fn ready_main1(seed: u64) -> GameState {
    let mut state = GameState::new_from_libraries(&[], &[], card_name, seed);
    state.step = Step::Main1;
    state.active_player = PlayerId::P0;
    state.priority_player = PlayerId::P0;
    state
}

fn put_object(state: &mut GameState, name: &str, zone: Zone) -> ObjectId {
    let card_def = card_id_by_name(name).unwrap_or_else(|| panic!("{name} in CARD_DEFS"));
    let id = state.objects.push(GameObject {
        card_def,
        name: CARD_DEFS[card_def as usize].object_name.to_string(),
        owner: PlayerId::P0,
        controller: PlayerId::P0,
        zone,
        tapped: false,
        summoning_sick: false,
        damage: 0,
        counters: Counters::default(),
        attachments: Vec::new(),
        v4: ObjectStateV4::from_card_def(card_def),
        spell_copy_origin: None,
        plotted_turn: None,
        zone_change_count: 0,
    });
    match zone {
        Zone::Hand => state.players[0].hand.push(id),
        Zone::Battlefield => state.players[0].battlefield.push(id),
        Zone::Graveyard => state.players[0].graveyard.push(id),
        Zone::Exile => state.exile.push(id),
        Zone::Library => state.players[0].library.push(id),
        Zone::Command => state.command.push(id),
        Zone::Stack => panic!("test helper does not fabricate stack items"),
    }
    id
}

fn castable(state: &mut GameState, spell: ObjectId) -> bool {
    match engine::advance_until_decision(state) {
        Decision::CastSpellOrPass {
            castable_spells, ..
        } => castable_spells.contains(&spell),
        other => panic!("expected CastSpellOrPass, got {other:?}"),
    }
}

/// Faeries-800 step 131: Humbling Elder ("HUMAN MONK") and Moon-Circuit
/// Hacker ("Human Ninja") are both Humans, so Of One Mind's {2} reduction
/// ("if you control a Human creature and a non-Human creature") does not
/// apply and it cannot be cast with one Island.
#[test]
fn of_one_mind_sees_an_all_caps_human_as_a_human() {
    let mut state = ready_main1(0x4b53_4841_444f_5706);
    state.players[0].mana_pool[ManaColor::U.pool_index()] = 1;
    let elder = put_object(&mut state, "Humbling Elder", Zone::Battlefield);
    let hacker = put_object(&mut state, "Moon-Circuit Hacker", Zone::Battlefield);
    let spell = put_object(&mut state, "Of One Mind", Zone::Hand);
    assert!(engine::has_effective_subtype(&state, elder, Subtype::Human));
    assert!(engine::has_effective_subtype(
        &state,
        hacker,
        Subtype::HumanAllCaps
    ));
    assert!(!castable(&mut state, spell), "only Humans: no reduction");

    put_object(&mut state, "Faerie Seer", Zone::Battlefield);
    assert!(
        castable(&mut state, spell),
        "a Human and a non-Human: reduced"
    );
}

/// Rally at the Hornburg: "Humans you control gain haste until end of
/// turn." Humbling Elder ("HUMAN MONK") is a Human, so it gains haste.
#[test]
fn rally_at_the_hornburg_hastes_an_all_caps_human() {
    let mut state = ready_main1(0x4b53_4841_444f_570a);
    state.players[0].mana_pool[ManaColor::R.pool_index()] = 2;
    let elder = put_object(&mut state, "Humbling Elder", Zone::Battlefield);
    let seer = put_object(&mut state, "Faerie Seer", Zone::Battlefield);
    let rally = put_object(&mut state, "Rally at the Hornburg", Zone::Hand);
    assert!(castable(&mut state, rally));
    engine::step(&mut state, Action::CastSpell(rally)).unwrap();
    for _ in 0..8 {
        if state.objects.get(rally).zone == Zone::Graveyard {
            break;
        }
        match engine::advance_until_decision(&mut state) {
            Decision::CastSpellOrPass { .. } => engine::step(&mut state, Action::Pass).unwrap(),
            other => panic!("unexpected decision while resolving Rally: {other:?}"),
        }
    }
    assert_eq!(state.objects.get(rally).zone, Zone::Graveyard);
    assert!(engine::has_effective_keyword(
        &state,
        elder,
        Keywords::HASTE
    ));
    assert!(!engine::has_effective_keyword(
        &state,
        seer,
        Keywords::HASTE
    ));
}

/// Every case-duplicated registry spelling names the same creature type as
/// its mixed-case twin, in both directions, for every card that carries one.
#[test]
fn every_case_duplicated_creature_type_matches_its_twin() {
    let twins = [
        (Subtype::BirdAllCaps, Subtype::Bird),
        (Subtype::FaerieAllCaps, Subtype::Faerie),
        (Subtype::HumanAllCaps, Subtype::Human),
        (Subtype::NinjaAllCaps, Subtype::Ninja),
        (Subtype::RogueAllCaps, Subtype::Rogue),
        (Subtype::WizardAllCaps, Subtype::Wizard),
    ];
    let mut checked = 0;
    for (index, def) in CARD_DEFS.iter().enumerate() {
        if !def.is_executable() {
            continue;
        }
        for &(caps, mixed) in &twins {
            if !def.subtypes.contains(&caps) && !def.subtypes.contains(&mixed) {
                continue;
            }
            let mut state = ready_main1(0x4b53_4841_444f_5707);
            let object = put_object(&mut state, &card_name(index as u16), Zone::Battlefield);
            assert!(
                engine::has_effective_subtype(&state, object, caps)
                    && engine::has_effective_subtype(&state, object, mixed),
                "{} is a {mixed:?} under either spelling",
                def.name
            );
            checked += 1;
        }
    }
    assert!(checked >= 7, "the census covers the all-caps cards");
}
