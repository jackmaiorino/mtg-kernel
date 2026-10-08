//! MageZero Standard family G: creatures with triggered and static
//! abilities, built for the five mono-color decks first.
#![cfg(feature = "standard-magezero-fixtures")]
use mtg_kernel::card_def::{card_id_by_name, CardCapability, Keywords, Subtype, CARD_DEFS};
use mtg_kernel::engine::{self, Action, Decision};
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::ManaColor;
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Target, Zone};
use mtg_kernel::trigger;

/// Name, subtypes, power/toughness, keywords and trigger count.
#[allow(clippy::type_complexity)]
const CARDS: &[(&str, &[Subtype], (i16, i16), Keywords, usize)] = &[
    (
        "Novice Inspector",
        &[Subtype::Human, Subtype::Detective],
        (1, 2),
        Keywords::NONE,
        1,
    ),
    (
        "Sentinel of the Nameless City",
        &[Subtype::Merfolk, Subtype::Warrior, Subtype::Scout],
        (3, 4),
        Keywords::VIGILANCE,
        2,
    ),
    (
        "Cenote Scout",
        &[Subtype::Merfolk, Subtype::Scout],
        (1, 1),
        Keywords::NONE,
        1,
    ),
];

fn ready_with_library(step: Step, library: &[&str]) -> GameState {
    let ids: Vec<u16> = library
        .iter()
        .map(|name| card_id_by_name(name).unwrap())
        .collect();
    let mut state = GameState::new_from_libraries(
        &ids,
        &ids,
        |id| CARD_DEFS[id as usize].object_name.into(),
        0x4741_4d45,
    );
    state.step = step;
    state
}

fn ready(step: Step) -> GameState {
    ready_with_library(step, &["Forest"; 40])
}

fn put(state: &mut GameState, player: PlayerId, name: &str, zone: Zone) -> ObjectId {
    let card_def = card_id_by_name(name).unwrap();
    let id = state.objects.push(GameObject {
        card_def,
        name: CARD_DEFS[card_def as usize].object_name.into(),
        owner: player,
        controller: player,
        zone,
        tapped: false,
        summoning_sick: zone == Zone::Hand,
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
        Zone::Hand => seat.hand.push(id),
        Zone::Battlefield => seat.battlefield.push(id),
        Zone::Graveyard => seat.graveyard.push(id),
        _ => panic!("helper zone"),
    }
    id
}

#[allow(dead_code)]
fn move_to(state: &mut GameState, id: ObjectId, zone: Zone) {
    event::propose_and_commit(state, ProposedEvent::zone_change(id, zone));
    let triggers = trigger::collect_and_process(state);
    state.engine.pending_triggers.extend(triggers);
}

fn pool(cost: &[(ManaColor, u8)], generic: u8) -> [u8; 6] {
    let mut pool = [0; 6];
    for &(color, count) in cost {
        pool[color.pool_index()] += count;
    }
    pool[5] += generic;
    pool
}

fn next(state: &mut GameState) -> Decision {
    let decision = engine::advance_until_decision(state);
    assert!(!matches!(decision, Decision::Halted { .. }), "{decision:?}");
    decision
}

/// Passes priority and orders triggers until the stack and pending triggers
/// are empty, returning the first other decision if one interrupts.
fn settle(state: &mut GameState) -> Option<Decision> {
    for _ in 0..200 {
        match next(state) {
            Decision::OrderTriggers { pending, .. } => {
                engine::step(state, Action::OrderTriggers((0..pending.len()).collect())).unwrap()
            }
            Decision::CastSpellOrPass { .. }
                if state.stack.is_empty() && state.engine.pending_triggers.is_empty() =>
            {
                return None
            }
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
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

/// Casts a spell from hand with mana already in the pool, answering the
/// target prompts in order.
fn cast(state: &mut GameState, spell: ObjectId, targets: &[Target]) {
    assert!(
        matches!(next(state), Decision::CastSpellOrPass { castable_spells, .. } if castable_spells.contains(&spell)),
        "{} not castable",
        state.objects.get(spell).name
    );
    engine::step(state, Action::CastSpell(spell)).unwrap();
    for &target in targets {
        match next(state) {
            Decision::ChooseTargets { legal_targets, .. } => {
                assert!(legal_targets.contains(&target), "{target:?} not legal");
                engine::step(state, Action::ChooseTarget(target)).unwrap();
            }
            other => panic!("expected a target choice, got {other:?}"),
        }
    }
}

/// Passes priority through the declare-attackers step, resolving attack
/// triggers, until the defender is asked to block.
fn pass_until_blocks(state: &mut GameState) {
    loop {
        match next(state) {
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            Decision::OrderTriggers { pending, .. } => {
                engine::step(state, Action::OrderTriggers((0..pending.len()).collect())).unwrap()
            }
            Decision::DeclareBlockers { .. } => return,
            other => panic!("unexpected {other:?}"),
        }
    }
}

fn battlefield_tokens(state: &GameState, player: PlayerId, name: &str) -> Vec<ObjectId> {
    let def = card_id_by_name(name).unwrap();
    state.players[player.index()]
        .battlefield
        .iter()
        .copied()
        .filter(|&id| state.objects.get(id).card_def == def)
        .collect()
}

#[test]
fn appended_definitions_match_their_printed_characteristics() {
    for &(name, subtypes, (power, toughness), keywords, triggers) in CARDS {
        let id = card_id_by_name(name).unwrap_or_else(|| panic!("{name} missing"));
        let def = &CARD_DEFS[id as usize];
        assert_eq!(def.capability, CardCapability::Full, "{name}");
        assert_eq!(def.subtypes, subtypes, "{name}");
        assert_eq!(
            (def.power, def.toughness),
            (Some(power), Some(toughness)),
            "{name}"
        );
        assert_eq!(def.keywords, keywords, "{name}");
        assert_eq!(trigger::triggers_for(id).len(), triggers, "{name}");
    }
    for subtype in [
        Subtype::Scout,
        Subtype::Bat,
        Subtype::Demon,
        Subtype::Mercenary,
        Subtype::Assassin,
        Subtype::Wolf,
        Subtype::Kraken,
        Subtype::Djinn,
    ] {
        assert!(Subtype::CREATURE_TYPES.contains(&subtype), "{subtype:?}");
        assert!(subtype.is_creature_type(), "{subtype:?}");
    }
}

#[test]
fn novice_inspector_investigates_on_entering() {
    let mut state = ready(Step::Main1);
    let inspector = put(&mut state, PlayerId::P0, "Novice Inspector", Zone::Hand);
    state.players[0].mana_pool = pool(&[(ManaColor::W, 1)], 0);
    cast(&mut state, inspector, &[]);
    settled(&mut state);
    assert_eq!(state.objects.get(inspector).zone, Zone::Battlefield);
    assert_eq!(
        battlefield_tokens(&state, PlayerId::P0, "Clue Token").len(),
        1
    );
}

#[test]
fn sentinel_makes_a_map_on_entering_and_on_attacking() {
    let mut state = ready(Step::Main1);
    let sentinel = put(
        &mut state,
        PlayerId::P0,
        "Sentinel of the Nameless City",
        Zone::Hand,
    );
    state.players[0].mana_pool = pool(&[(ManaColor::G, 1)], 2);
    cast(&mut state, sentinel, &[]);
    settled(&mut state);
    assert_eq!(
        battlefield_tokens(&state, PlayerId::P0, "Map Token").len(),
        1
    );

    state.objects.get_mut(sentinel).summoning_sick = false;
    state.step = Step::DeclareAttackers;
    assert!(matches!(
        next(&mut state),
        Decision::DeclareAttackers { .. }
    ));
    engine::step(&mut state, Action::DeclareAttackers(vec![sentinel])).unwrap();
    // Vigilance: attacking does not tap it.
    assert!(!state.objects.get(sentinel).tapped);
    pass_until_blocks(&mut state);
    assert_eq!(
        battlefield_tokens(&state, PlayerId::P0, "Map Token").len(),
        2
    );
}

#[test]
fn cenote_scout_explores_a_land_into_hand() {
    let mut state = ready_with_library(Step::Main1, &["Forest"; 40]);
    let scout = put(&mut state, PlayerId::P0, "Cenote Scout", Zone::Hand);
    state.players[0].mana_pool = pool(&[(ManaColor::G, 1)], 0);
    let top = state.players[0].library[0];
    cast(&mut state, scout, &[]);
    settled(&mut state);
    assert_eq!(state.objects.get(top).zone, Zone::Hand);
    assert_eq!(state.objects.get(scout).counters.plus1_plus1, 0);
}

#[test]
fn cenote_scout_explores_a_nonland_for_a_counter() {
    let mut state = ready_with_library(Step::Main1, &["Cenote Scout"; 40]);
    let scout = put(&mut state, PlayerId::P0, "Cenote Scout", Zone::Hand);
    state.players[0].mana_pool = pool(&[(ManaColor::G, 1)], 0);
    let top = state.players[0].library[0];
    cast(&mut state, scout, &[]);
    let Some(Decision::ChooseEffectOption {
        player,
        option_count,
        ..
    }) = settle(&mut state)
    else {
        panic!("expected the explore graveyard choice");
    };
    assert_eq!((player, option_count), (PlayerId::P0, 2));
    // Option 1 puts the revealed card into the graveyard.
    engine::step(&mut state, Action::ChooseEffectOption(1)).unwrap();
    settled(&mut state);
    assert_eq!(state.objects.get(top).zone, Zone::Graveyard);
    assert_eq!(state.objects.get(scout).counters.plus1_plus1, 1);
}

#[test]
fn cenote_scout_that_left_still_reveals_but_gets_no_counter() {
    let mut state = ready_with_library(Step::Main1, &["Forest"; 40]);
    let scout = put(&mut state, PlayerId::P0, "Cenote Scout", Zone::Hand);
    state.players[0].mana_pool = pool(&[(ManaColor::G, 1)], 0);
    let top = state.players[0].library[0];
    cast(&mut state, scout, &[]);
    // Resolve the creature spell, leaving its trigger on the stack.
    loop {
        match next(&mut state) {
            Decision::CastSpellOrPass { .. } if !state.stack.is_empty() => {
                if state.objects.get(scout).zone == Zone::Battlefield {
                    break;
                }
                engine::step(&mut state, Action::Pass).unwrap();
            }
            other => panic!("unexpected {other:?}"),
        }
    }
    event::propose_and_commit(&mut state, ProposedEvent::zone_change(scout, Zone::Hand));
    settled(&mut state);
    assert_eq!(state.objects.get(top).zone, Zone::Hand);
    assert_eq!(state.objects.get(scout).counters.plus1_plus1, 0);
}
