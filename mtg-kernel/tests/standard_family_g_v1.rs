//! MageZero Standard family G: creatures with triggered and static
//! abilities, built for the five mono-color decks first.
#![cfg(feature = "standard-magezero-fixtures")]
use mtg_kernel::card_def::{card_id_by_name, CardCapability, Keywords, Subtype, CARD_DEFS};
use mtg_kernel::effect::EffectBooleanChoicePurpose;
use mtg_kernel::engine::{self, Action, CostKind, Decision};
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::{ManaColor, Pip};
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
    (
        "Gatekeeper of Malakir",
        &[Subtype::Vampire, Subtype::Warrior],
        (2, 2),
        Keywords::NONE,
        1,
    ),
    (
        "Deep-Cavern Bat",
        &[Subtype::Bat],
        (1, 1),
        Keywords(Keywords::FLYING.0 | Keywords::LIFELINK.0),
        1,
    ),
    (
        "Razorkin Needlehead",
        &[Subtype::Human, Subtype::Assassin],
        (2, 2),
        Keywords::NONE,
        1,
    ),
    (
        "Ascendant Packleader",
        &[Subtype::Wolf],
        (2, 1),
        Keywords::NONE,
        1,
    ),
    (
        "Sharp-Eyed Rookie",
        &[Subtype::Human, Subtype::Detective],
        (2, 2),
        Keywords::VIGILANCE,
        1,
    ),
    (
        "Evolving Adaptive",
        &[Subtype::Phyrexian, Subtype::Warrior],
        (0, 0),
        Keywords::NONE,
        1,
    ),
    (
        "Quirion Beastcaller",
        &[Subtype::Dryad, Subtype::Warrior],
        (2, 2),
        Keywords::NONE,
        2,
    ),
    (
        "Coppercoat Vanguard",
        &[Subtype::Human, Subtype::Soldier],
        (2, 2),
        Keywords::NONE,
        0,
    ),
    (
        "Adeline, Resplendent Cathar",
        &[Subtype::Human, Subtype::Knight],
        (0, 4),
        Keywords::VIGILANCE,
        1,
    ),
    (
        "Bloodletter of Aclazotz",
        &[Subtype::Vampire, Subtype::Demon],
        (2, 4),
        Keywords::FLYING,
        0,
    ),
    (
        "Thalia, Guardian of Thraben",
        &[Subtype::Human, Subtype::Soldier],
        (2, 1),
        Keywords::FIRST_STRIKE,
        0,
    ),
    (
        "Haughty Djinn",
        &[Subtype::Djinn],
        (0, 4),
        Keywords::FLYING,
        0,
    ),
    (
        "Hired Claw",
        &[Subtype::Lizard, Subtype::Mercenary],
        (1, 2),
        Keywords::NONE,
        1,
    ),
    (
        "Warden of the Inner Sky",
        &[Subtype::Human, Subtype::Soldier],
        (1, 2),
        Keywords::NONE,
        0,
    ),
    (
        "Extraction Specialist",
        &[Subtype::Human, Subtype::Rogue],
        (3, 2),
        Keywords::LIFELINK,
        1,
    ),
    (
        "Recruitment Officer",
        &[Subtype::Human, Subtype::Soldier],
        (2, 1),
        Keywords::NONE,
        0,
    ),
    (
        "Hullbreaker Horror",
        &[Subtype::Kraken, Subtype::Horror],
        (7, 8),
        Keywords::FLASH,
        1,
    ),
    (
        "Unstoppable Slasher",
        &[Subtype::Zombie, Subtype::Assassin],
        (2, 3),
        Keywords::DEATHTOUCH,
        2,
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
        // Incomplete printed behavior stays available only for development.
        let expected = CardCapability::Full;
        assert_eq!(def.capability, expected, "{name}");
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

/// Casts Gatekeeper of Malakir, answering the kicker prompt.
fn cast_gatekeeper(state: &mut GameState, kicked: bool) -> ObjectId {
    let gatekeeper = put(state, PlayerId::P0, "Gatekeeper of Malakir", Zone::Hand);
    state.players[0].mana_pool = pool(&[(ManaColor::B, 3)], 0);
    assert!(
        matches!(next(state), Decision::CastSpellOrPass { castable_spells, .. } if castable_spells.contains(&gatekeeper))
    );
    engine::step(state, Action::CastSpell(gatekeeper)).unwrap();
    assert!(matches!(next(state), Decision::ChooseKicker { .. }));
    engine::step(state, Action::ChooseKicker(kicked)).unwrap();
    gatekeeper
}

#[test]
fn kicked_gatekeeper_makes_the_target_player_sacrifice() {
    let mut state = ready(Step::Main1);
    let victim = put(&mut state, PlayerId::P1, "Cenote Scout", Zone::Battlefield);
    let gatekeeper = cast_gatekeeper(&mut state, true);
    let target = loop {
        match next(&mut state) {
            Decision::ChooseTargets { legal_targets, .. } => break legal_targets,
            Decision::CastSpellOrPass { .. } => engine::step(&mut state, Action::Pass).unwrap(),
            other => panic!("unexpected {other:?}"),
        }
    };
    assert!(target.contains(&Target::Player(PlayerId::P0)));
    assert!(target.contains(&Target::Player(PlayerId::P1)));
    engine::step(
        &mut state,
        Action::ChooseTarget(Target::Player(PlayerId::P1)),
    )
    .unwrap();
    settled(&mut state);
    assert_eq!(state.objects.get(gatekeeper).zone, Zone::Battlefield);
    assert_eq!(state.objects.get(victim).zone, Zone::Graveyard);
}

#[test]
fn unkicked_gatekeeper_has_no_trigger() {
    let mut state = ready(Step::Main1);
    let bystander = put(&mut state, PlayerId::P1, "Cenote Scout", Zone::Battlefield);
    let gatekeeper = cast_gatekeeper(&mut state, false);
    settled(&mut state);
    assert_eq!(state.objects.get(gatekeeper).zone, Zone::Battlefield);
    assert_eq!(state.objects.get(bystander).zone, Zone::Battlefield);
    assert_eq!(state.players[0].mana_pool[ManaColor::B.pool_index()], 1);
}

/// Casts Deep-Cavern Bat at the opponent and resolves it up to the exile
/// prompt, returning the bat and the prompt's legal cards.
fn cast_bat_to_choice(state: &mut GameState) -> (ObjectId, Option<Vec<Target>>) {
    let bat = put(state, PlayerId::P0, "Deep-Cavern Bat", Zone::Hand);
    state.players[0].mana_pool = pool(&[(ManaColor::B, 1)], 1);
    cast(state, bat, &[]);
    loop {
        match next(state) {
            Decision::ChooseTargets { legal_targets, .. } => {
                assert_eq!(legal_targets, vec![Target::Player(PlayerId::P1)]);
                engine::step(state, Action::ChooseTarget(Target::Player(PlayerId::P1))).unwrap();
            }
            Decision::CastSpellOrPass { .. }
                if state.stack.is_empty() && state.engine.pending_triggers.is_empty() =>
            {
                return (bat, None)
            }
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            Decision::ChooseEffectTargets {
                player,
                min_targets,
                max_targets,
                legal_targets,
                can_finish,
                ..
            } => {
                assert_eq!((player, min_targets, max_targets), (PlayerId::P0, 0, 1));
                assert!(can_finish);
                return (bat, Some(legal_targets));
            }
            other => panic!("unexpected {other:?}"),
        }
    }
}

#[test]
fn deep_cavern_bat_exiles_until_it_leaves() {
    let mut state = ready(Step::Main1);
    let land = put(&mut state, PlayerId::P1, "Forest", Zone::Hand);
    let spell = put(&mut state, PlayerId::P1, "Cenote Scout", Zone::Hand);
    let (bat, legal) = cast_bat_to_choice(&mut state);
    // A single nonland card is still a real "may" choice; lands never qualify.
    assert_eq!(legal, Some(vec![Target::Object(spell)]));
    engine::step(
        &mut state,
        Action::ChooseEffectTarget(Target::Object(spell)),
    )
    .unwrap();
    settled(&mut state);
    assert_eq!(state.objects.get(spell).zone, Zone::Exile);
    assert_eq!(state.players[1].hand, vec![land]);

    // Leaving returns the card at once, with no trigger on the stack.
    event::propose_and_commit(&mut state, ProposedEvent::zone_change(bat, Zone::Graveyard));
    assert_eq!(state.objects.get(spell).zone, Zone::Hand);
    assert!(state.engine.linked_exile_records.is_empty());
}

#[test]
fn deep_cavern_bat_may_decline() {
    let mut state = ready(Step::Main1);
    let spell = put(&mut state, PlayerId::P1, "Cenote Scout", Zone::Hand);
    let (_, legal) = cast_bat_to_choice(&mut state);
    assert!(legal.is_some());
    engine::step(&mut state, Action::FinishEffectSelection).unwrap();
    settled(&mut state);
    assert_eq!(state.objects.get(spell).zone, Zone::Hand);
    assert!(state.engine.linked_exile_records.is_empty());
}

#[test]
fn deep_cavern_bat_sees_only_lands_and_moves_on() {
    let mut state = ready(Step::Main1);
    let land = put(&mut state, PlayerId::P1, "Forest", Zone::Hand);
    let (_, legal) = cast_bat_to_choice(&mut state);
    assert_eq!(legal, None);
    assert_eq!(state.players[1].hand, vec![land]);
}

#[test]
fn deep_cavern_bat_gone_before_its_trigger_resolves_exiles_nothing() {
    let mut state = ready(Step::Main1);
    let spell = put(&mut state, PlayerId::P1, "Cenote Scout", Zone::Hand);
    let bat = put(&mut state, PlayerId::P0, "Deep-Cavern Bat", Zone::Hand);
    state.players[0].mana_pool = pool(&[(ManaColor::B, 1)], 1);
    cast(&mut state, bat, &[]);
    loop {
        match next(&mut state) {
            Decision::ChooseTargets { .. } => {
                engine::step(
                    &mut state,
                    Action::ChooseTarget(Target::Player(PlayerId::P1)),
                )
                .unwrap();
                break;
            }
            Decision::CastSpellOrPass { .. } => engine::step(&mut state, Action::Pass).unwrap(),
            other => panic!("unexpected {other:?}"),
        }
    }
    event::propose_and_commit(&mut state, ProposedEvent::zone_change(bat, Zone::Graveyard));
    match settle(&mut state) {
        None => {}
        Some(Decision::ChooseEffectTargets { .. }) => {
            engine::step(
                &mut state,
                Action::ChooseEffectTarget(Target::Object(spell)),
            )
            .unwrap();
            settled(&mut state);
        }
        Some(other) => panic!("unexpected {other:?}"),
    }
    assert_eq!(state.objects.get(spell).zone, Zone::Hand);
    assert!(state.engine.linked_exile_records.is_empty());
}

#[test]
fn razorkin_needlehead_has_first_strike_only_on_its_controllers_turn() {
    let mut state = ready(Step::Main1);
    let needlehead = put(
        &mut state,
        PlayerId::P0,
        "Razorkin Needlehead",
        Zone::Battlefield,
    );
    assert!(engine::has_effective_keyword(
        &state,
        needlehead,
        Keywords::FIRST_STRIKE
    ));
    state.active_player = PlayerId::P1;
    assert!(!engine::has_effective_keyword(
        &state,
        needlehead,
        Keywords::FIRST_STRIKE
    ));
}

#[test]
fn razorkin_needlehead_pings_the_opponent_for_each_card_they_draw() {
    let mut state = ready(Step::Main1);
    put(
        &mut state,
        PlayerId::P0,
        "Razorkin Needlehead",
        Zone::Battlefield,
    );
    event::propose_and_commit(&mut state, ProposedEvent::draw(PlayerId::P1));
    event::propose_and_commit(&mut state, ProposedEvent::draw(PlayerId::P1));
    event::propose_and_commit(&mut state, ProposedEvent::draw(PlayerId::P0));
    let triggers = trigger::collect_and_process(&mut state);
    state.engine.pending_triggers.extend(triggers);
    settled(&mut state);
    assert_eq!((state.players[0].life, state.players[1].life), (20, 18));
}

/// Puts `name` in P0's hand with exactly its printed mana in the pool and
/// casts it with no targets.
fn cast_creature(state: &mut GameState, name: &str) -> ObjectId {
    let id = put(state, PlayerId::P0, name, Zone::Hand);
    let def = &CARD_DEFS[card_id_by_name(name).unwrap() as usize];
    let mut mana = [0; 6];
    for pip in def.cost.pips {
        let Pip::Colored(color) = pip else {
            panic!("{name}: unexpected pip {pip:?}");
        };
        mana[color.pool_index()] += 1;
    }
    mana[5] += def.cost.generic;
    state.players[0].mana_pool = mana;
    cast(state, id, &[]);
    id
}

#[test]
fn ascendant_packleader_grows_from_big_spells_and_big_permanents() {
    let mut state = ready(Step::Main1);
    let first = cast_creature(&mut state, "Ascendant Packleader");
    settled(&mut state);
    assert_eq!(state.objects.get(first).counters.plus1_plus1, 0);

    // Troll of Khazad-dum has mana value 6: casting it grows the Packleader
    // already on the battlefield.
    let troll = cast_creature(&mut state, "Troll of Khazad-dum");
    settled(&mut state);
    assert_eq!(state.objects.get(troll).zone, Zone::Battlefield);
    assert_eq!(state.objects.get(first).counters.plus1_plus1, 1);
    assert_eq!(engine::effective_power(&state, first), 3);

    // With the Troll in play, a new Packleader enters with its counter, and
    // casting a one-drop does not trigger the first one.
    let second = cast_creature(&mut state, "Ascendant Packleader");
    settled(&mut state);
    assert_eq!(state.objects.get(second).counters.plus1_plus1, 1);
    assert_eq!(state.objects.get(first).counters.plus1_plus1, 1);
}

#[test]
fn sharp_eyed_rookie_grows_and_investigates_when_outclassed() {
    let mut state = ready(Step::Main1);
    let rookie = put(
        &mut state,
        PlayerId::P0,
        "Sharp-Eyed Rookie",
        Zone::Battlefield,
    );
    // A 1/1 does not beat a 2/2.
    cast_creature(&mut state, "Cenote Scout");
    while let Some(decision) = settle(&mut state) {
        // Cenote Scout's explore choice, if a nonland is on top.
        assert!(matches!(decision, Decision::ChooseEffectOption { .. }));
        engine::step(&mut state, Action::ChooseEffectOption(0)).unwrap();
    }
    assert_eq!(state.objects.get(rookie).counters.plus1_plus1, 0);
    assert!(battlefield_tokens(&state, PlayerId::P0, "Clue Token").is_empty());
    // A 3/4 does.
    cast_creature(&mut state, "Sentinel of the Nameless City");
    settled(&mut state);
    assert_eq!(state.objects.get(rookie).counters.plus1_plus1, 1);
    assert_eq!(
        battlefield_tokens(&state, PlayerId::P0, "Clue Token").len(),
        1
    );
    // The opponent's creatures never count.
    let theirs = put(&mut state, PlayerId::P1, "Troll of Khazad-dum", Zone::Hand);
    move_to(&mut state, theirs, Zone::Battlefield);
    settled(&mut state);
    assert_eq!(state.objects.get(rookie).counters.plus1_plus1, 1);
}

#[test]
fn sharp_eyed_rookie_rechecks_its_condition_on_resolution() {
    let mut state = ready(Step::Main1);
    let rookie = put(
        &mut state,
        PlayerId::P0,
        "Sharp-Eyed Rookie",
        Zone::Battlefield,
    );
    let sentinel = put(
        &mut state,
        PlayerId::P0,
        "Sentinel of the Nameless City",
        Zone::Hand,
    );
    move_to(&mut state, sentinel, Zone::Battlefield);
    // Before the trigger resolves, the Rookie becomes a 4/4.
    state.objects.get_mut(rookie).counters.plus1_plus1 = 2;
    settled(&mut state);
    assert_eq!(state.objects.get(rookie).counters.plus1_plus1, 2);
    assert!(battlefield_tokens(&state, PlayerId::P0, "Clue Token").is_empty());
}

#[test]
fn evolving_adaptive_enters_with_oil_and_grows_from_bigger_creatures() {
    let mut state = ready(Step::Main1);
    let adaptive = cast_creature(&mut state, "Evolving Adaptive");
    settled(&mut state);
    assert_eq!(state.objects.get(adaptive).zone, Zone::Battlefield);
    assert_eq!(state.objects.get(adaptive).counters.oil, 1);
    assert_eq!(
        (
            engine::effective_power(&state, adaptive),
            engine::effective_toughness(&state, adaptive)
        ),
        (1, 1)
    );
    // A 2/2 has greater power and toughness than the 1/1.
    cast_creature(&mut state, "Sharp-Eyed Rookie");
    settled(&mut state);
    assert_eq!(state.objects.get(adaptive).counters.oil, 2);
    // Another 2/2 does not beat a 2/2.
    cast_creature(&mut state, "Sharp-Eyed Rookie");
    settled(&mut state);
    assert_eq!(state.objects.get(adaptive).counters.oil, 2);
    assert_eq!(engine::effective_power(&state, adaptive), 2);
}

#[test]
fn quirion_beastcaller_grows_from_creature_spells_only() {
    let mut state = ready(Step::Main1);
    let beastcaller = put(
        &mut state,
        PlayerId::P0,
        "Quirion Beastcaller",
        Zone::Battlefield,
    );
    cast_creature(&mut state, "Cenote Scout");
    settled(&mut state);
    assert_eq!(state.objects.get(beastcaller).counters.plus1_plus1, 1);
    // A noncreature spell does nothing.
    let bolt = put(&mut state, PlayerId::P0, "Lightning Bolt", Zone::Hand);
    state.players[0].mana_pool = pool(&[(ManaColor::R, 1)], 0);
    cast(&mut state, bolt, &[Target::Player(PlayerId::P1)]);
    settled(&mut state);
    assert_eq!(state.players[1].life, 17);
    assert_eq!(state.objects.get(beastcaller).counters.plus1_plus1, 1);
}

#[test]
fn quirion_beastcaller_distributes_its_counters_when_it_dies() {
    let mut state = ready(Step::Main1);
    let beastcaller = put(
        &mut state,
        PlayerId::P0,
        "Quirion Beastcaller",
        Zone::Battlefield,
    );
    let first = put(
        &mut state,
        PlayerId::P0,
        "Gatekeeper of Malakir",
        Zone::Battlefield,
    );
    let second = put(&mut state, PlayerId::P0, "Cenote Scout", Zone::Battlefield);
    let theirs = put(&mut state, PlayerId::P1, "Cenote Scout", Zone::Battlefield);
    state.objects.get_mut(beastcaller).counters.plus1_plus1 = 3;
    move_to(&mut state, beastcaller, Zone::Graveyard);
    // Targets and division are declared with no intervening priority.
    for recipient in [second, second, first] {
        match next(&mut state) {
            Decision::ChooseTargets {
                player,
                legal_targets,
                ..
            } => {
                assert_eq!(player, PlayerId::P0);
                assert!(!legal_targets.contains(&Target::Object(theirs)));
                engine::step(&mut state, Action::ChooseTarget(Target::Object(recipient))).unwrap();
            }
            other => panic!("expected announcement allocation, got {other:?}"),
        }
        assert_eq!(state.objects.get(first).counters.plus1_plus1, 0);
        assert_eq!(state.objects.get(second).counters.plus1_plus1, 0);
    }
    settled(&mut state);
    assert_eq!(state.objects.get(first).counters.plus1_plus1, 1);
    assert_eq!(state.objects.get(second).counters.plus1_plus1, 2);
    assert_eq!(state.objects.get(theirs).counters.plus1_plus1, 0);
}

#[test]
fn quirion_beastcaller_without_counters_distributes_nothing() {
    let mut state = ready(Step::Main1);
    let beastcaller = put(
        &mut state,
        PlayerId::P0,
        "Quirion Beastcaller",
        Zone::Battlefield,
    );
    let other = put(&mut state, PlayerId::P0, "Cenote Scout", Zone::Battlefield);
    move_to(&mut state, beastcaller, Zone::Graveyard);
    settled(&mut state);
    assert_eq!(state.objects.get(other).counters.plus1_plus1, 0);
}

#[test]
fn quirion_beastcaller_can_choose_no_targets_even_with_a_legal_creature() {
    let mut state = ready(Step::Main1);
    let beastcaller = put(
        &mut state,
        PlayerId::P0,
        "Quirion Beastcaller",
        Zone::Battlefield,
    );
    let other = put(&mut state, PlayerId::P0, "Cenote Scout", Zone::Battlefield);
    state.objects.get_mut(beastcaller).counters.plus1_plus1 = 2;
    move_to(&mut state, beastcaller, Zone::Graveyard);
    assert!(matches!(
        next(&mut state),
        Decision::ChooseTargets {
            can_finish: true,
            ..
        }
    ));
    engine::step(&mut state, Action::FinishEffectSelection).unwrap();
    settled(&mut state);
    assert_eq!(state.objects.get(other).counters.plus1_plus1, 0);
}

#[test]
fn unstoppable_slasher_halves_the_damaged_players_life() {
    let mut state = ready(Step::DeclareAttackers);
    let slasher = put(
        &mut state,
        PlayerId::P0,
        "Unstoppable Slasher",
        Zone::Battlefield,
    );
    state.players[1].life = 21;
    assert!(matches!(
        next(&mut state),
        Decision::DeclareAttackers { .. }
    ));
    engine::step(&mut state, Action::DeclareAttackers(vec![slasher])).unwrap();
    pass_until_blocks(&mut state);
    engine::step(&mut state, Action::DeclareBlockers(Vec::new())).unwrap();
    while state.step != Step::Main2 {
        match next(&mut state) {
            Decision::CastSpellOrPass { .. } => engine::step(&mut state, Action::Pass).unwrap(),
            Decision::OrderTriggers { pending, .. } => engine::step(
                &mut state,
                Action::OrderTriggers((0..pending.len()).collect()),
            )
            .unwrap(),
            other => panic!("unexpected {other:?}"),
        }
    }
    // 21 - 2 combat damage = 19, then half of 19 rounded up is 10.
    assert_eq!(state.players[1].life, 9);
}

#[test]
fn unstoppable_slasher_returns_once_then_stays_dead() {
    let mut state = ready(Step::Main1);
    let slasher = put(
        &mut state,
        PlayerId::P0,
        "Unstoppable Slasher",
        Zone::Battlefield,
    );
    move_to(&mut state, slasher, Zone::Graveyard);
    settled(&mut state);
    let returned = state.objects.get(slasher);
    assert_eq!(returned.zone, Zone::Battlefield);
    assert_eq!(returned.controller, PlayerId::P0);
    assert!(returned.tapped);
    assert_eq!(returned.counters.stun, 2);

    // It died with stun counters on it, so it stays in the graveyard.
    move_to(&mut state, slasher, Zone::Graveyard);
    settled(&mut state);
    assert_eq!(state.objects.get(slasher).zone, Zone::Graveyard);
}

#[test]
fn unstoppable_slasher_with_a_counter_or_exiled_first_does_not_return() {
    let mut state = ready(Step::Main1);
    let slasher = put(
        &mut state,
        PlayerId::P0,
        "Unstoppable Slasher",
        Zone::Battlefield,
    );
    state.objects.get_mut(slasher).counters.plus1_plus1 = 1;
    move_to(&mut state, slasher, Zone::Graveyard);
    settled(&mut state);
    assert_eq!(state.objects.get(slasher).zone, Zone::Graveyard);

    // A counterless Slasher whose card leaves the graveyard before its
    // trigger resolves stays where it went.
    let other = put(
        &mut state,
        PlayerId::P0,
        "Unstoppable Slasher",
        Zone::Battlefield,
    );
    move_to(&mut state, other, Zone::Graveyard);
    assert_eq!(state.engine.pending_triggers.len(), 1);
    event::propose_and_commit(&mut state, ProposedEvent::zone_change(other, Zone::Exile));
    settled(&mut state);
    assert_eq!(state.objects.get(other).zone, Zone::Exile);
}

#[test]
fn coppercoat_vanguard_boosts_only_other_humans_you_control() {
    let mut state = ready(Step::Main1);
    let vanguard = put(
        &mut state,
        PlayerId::P0,
        "Coppercoat Vanguard",
        Zone::Battlefield,
    );
    let human = put(
        &mut state,
        PlayerId::P0,
        "Novice Inspector",
        Zone::Battlefield,
    );
    let merfolk = put(&mut state, PlayerId::P0, "Cenote Scout", Zone::Battlefield);
    let theirs = put(
        &mut state,
        PlayerId::P1,
        "Novice Inspector",
        Zone::Battlefield,
    );
    let power = |state: &GameState, id| {
        (
            engine::effective_power(state, id),
            engine::effective_toughness(state, id),
        )
    };
    assert_eq!(power(&state, vanguard), (2, 2));
    assert_eq!(power(&state, human), (2, 2));
    assert_eq!(power(&state, merfolk), (1, 1));
    assert_eq!(power(&state, theirs), (1, 2));
}

/// P1 casts Lightning Bolt at `target` with `extra` colorless mana spare,
/// answering any ward payment with `pay`. Returns the Bolt and how many
/// ward payment choices P1 was offered.
fn opponent_bolts(
    state: &mut GameState,
    target: ObjectId,
    extra: u8,
    pay: bool,
) -> (ObjectId, usize) {
    let bolt = put(state, PlayerId::P1, "Lightning Bolt", Zone::Hand);
    state.players[1].mana_pool = pool(&[(ManaColor::R, 1)], extra);
    if matches!(
        next(state),
        Decision::CastSpellOrPass {
            player: PlayerId::P0,
            ..
        }
    ) {
        engine::step(state, Action::Pass).unwrap();
    }
    cast(state, bolt, &[Target::Object(target)]);
    let mut offers = 0;
    while let Some(decision) = settle(state) {
        match decision {
            Decision::ChooseEffectBoolean {
                player: PlayerId::P1,
                purpose: EffectBooleanChoicePurpose::CounterUnlessPaysGeneric { generic: 1, .. },
                ..
            } => {
                offers += 1;
                engine::step(state, Action::ChooseEffectBoolean(pay)).unwrap();
            }
            other => panic!("unexpected {other:?}"),
        }
    }
    (bolt, offers)
}

#[test]
fn coppercoat_vanguard_gives_other_humans_ward_one() {
    // Declined: the Bolt is countered.
    let mut state = ready(Step::Main1);
    put(
        &mut state,
        PlayerId::P0,
        "Coppercoat Vanguard",
        Zone::Battlefield,
    );
    let human = put(
        &mut state,
        PlayerId::P0,
        "Novice Inspector",
        Zone::Battlefield,
    );
    let (bolt, offers) = opponent_bolts(&mut state, human, 1, false);
    assert_eq!(offers, 1);
    assert_eq!(state.objects.get(bolt).zone, Zone::Graveyard);
    assert_eq!(state.objects.get(human).zone, Zone::Battlefield);
    assert_eq!(state.objects.get(human).damage, 0);

    // Paid: the Bolt resolves.
    let mut state = ready(Step::Main1);
    put(
        &mut state,
        PlayerId::P0,
        "Coppercoat Vanguard",
        Zone::Battlefield,
    );
    let human = put(
        &mut state,
        PlayerId::P0,
        "Novice Inspector",
        Zone::Battlefield,
    );
    let (_, offers) = opponent_bolts(&mut state, human, 1, true);
    assert_eq!(offers, 1);
    assert_eq!(state.objects.get(human).zone, Zone::Graveyard);

    // Unpayable: countered without a choice.
    let mut state = ready(Step::Main1);
    put(
        &mut state,
        PlayerId::P0,
        "Coppercoat Vanguard",
        Zone::Battlefield,
    );
    let human = put(
        &mut state,
        PlayerId::P0,
        "Novice Inspector",
        Zone::Battlefield,
    );
    let (bolt, offers) = opponent_bolts(&mut state, human, 0, true);
    assert_eq!(offers, 0);
    assert_eq!(state.objects.get(bolt).zone, Zone::Graveyard);
    assert_eq!(state.objects.get(human).zone, Zone::Battlefield);

    // The Vanguard itself has no ward.
    let mut state = ready(Step::Main1);
    let vanguard = put(
        &mut state,
        PlayerId::P0,
        "Coppercoat Vanguard",
        Zone::Battlefield,
    );
    let (_, offers) = opponent_bolts(&mut state, vanguard, 1, false);
    assert_eq!(offers, 0);
    assert_eq!(state.objects.get(vanguard).zone, Zone::Graveyard);
}

/// From the declare-attackers decision, attacks with `attackers`, lets the
/// defender block nothing and passes until the second main phase.
fn attack_unblocked(state: &mut GameState, attackers: Vec<ObjectId>) {
    assert!(matches!(next(state), Decision::DeclareAttackers { .. }));
    engine::step(state, Action::DeclareAttackers(attackers)).unwrap();
    pass_until_blocks(state);
    engine::step(state, Action::DeclareBlockers(Vec::new())).unwrap();
    while state.step != Step::Main2 {
        match next(state) {
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            Decision::OrderTriggers { pending, .. } => {
                engine::step(state, Action::OrderTriggers((0..pending.len()).collect())).unwrap()
            }
            Decision::GameOver { .. } => return,
            other => panic!("unexpected {other:?}"),
        }
    }
}

#[test]
fn adeline_power_counts_the_creatures_you_control() {
    let mut state = ready(Step::Main1);
    let adeline = put(
        &mut state,
        PlayerId::P0,
        "Adeline, Resplendent Cathar",
        Zone::Battlefield,
    );
    assert_eq!(engine::effective_power(&state, adeline), 1);
    put(&mut state, PlayerId::P0, "Cenote Scout", Zone::Battlefield);
    put(&mut state, PlayerId::P0, "Forest", Zone::Battlefield);
    put(&mut state, PlayerId::P1, "Cenote Scout", Zone::Battlefield);
    assert_eq!(engine::effective_power(&state, adeline), 2);
    assert_eq!(engine::effective_toughness(&state, adeline), 4);
}

#[test]
fn adeline_makes_an_attacking_human_whenever_you_attack() {
    let mut state = ready(Step::DeclareAttackers);
    let adeline = put(
        &mut state,
        PlayerId::P0,
        "Adeline, Resplendent Cathar",
        Zone::Battlefield,
    );
    attack_unblocked(&mut state, vec![adeline]);
    let humans = battlefield_tokens(&state, PlayerId::P0, "Human Token");
    assert_eq!(humans.len(), 1);
    assert!(state.objects.get(humans[0]).tapped);
    assert!(!state.objects.get(adeline).tapped);
    // Adeline (two creatures) and the Human both connect.
    assert_eq!(state.players[1].life, 17);
}

#[test]
fn adeline_triggers_when_only_another_creature_attacks() {
    let mut state = ready(Step::DeclareAttackers);
    put(
        &mut state,
        PlayerId::P0,
        "Adeline, Resplendent Cathar",
        Zone::Battlefield,
    );
    let scout = put(&mut state, PlayerId::P0, "Cenote Scout", Zone::Battlefield);
    attack_unblocked(&mut state, vec![scout]);
    assert_eq!(
        battlefield_tokens(&state, PlayerId::P0, "Human Token").len(),
        1
    );
    assert_eq!(state.players[1].life, 18);
}

#[test]
fn adeline_does_not_trigger_without_attackers() {
    let mut state = ready(Step::DeclareAttackers);
    put(
        &mut state,
        PlayerId::P0,
        "Adeline, Resplendent Cathar",
        Zone::Battlefield,
    );
    assert!(matches!(
        next(&mut state),
        Decision::DeclareAttackers { .. }
    ));
    engine::step(&mut state, Action::DeclareAttackers(Vec::new())).unwrap();
    settled(&mut state);
    assert!(battlefield_tokens(&state, PlayerId::P0, "Human Token").is_empty());
}

#[test]
fn bloodletter_doubles_opponent_life_loss_during_your_turn() {
    // Your turn: Bolt's damage to the opponent doubles, your own
    // life loss does not.
    let mut state = ready(Step::Main1);
    put(
        &mut state,
        PlayerId::P0,
        "Bloodletter of Aclazotz",
        Zone::Battlefield,
    );
    let bolt = put(&mut state, PlayerId::P0, "Lightning Bolt", Zone::Hand);
    state.players[0].mana_pool = pool(&[(ManaColor::R, 1)], 0);
    cast(&mut state, bolt, &[Target::Player(PlayerId::P1)]);
    settled(&mut state);
    assert_eq!(state.players[1].life, 14);
    let bolt = put(&mut state, PlayerId::P0, "Lightning Bolt", Zone::Hand);
    state.players[0].mana_pool = pool(&[(ManaColor::R, 1)], 0);
    cast(&mut state, bolt, &[Target::Player(PlayerId::P0)]);
    settled(&mut state);
    assert_eq!(state.players[0].life, 17);

    // Unstoppable Slasher's half-life loss doubles too.
    let mut state = ready(Step::DeclareAttackers);
    put(
        &mut state,
        PlayerId::P0,
        "Bloodletter of Aclazotz",
        Zone::Battlefield,
    );
    let slasher = put(
        &mut state,
        PlayerId::P0,
        "Unstoppable Slasher",
        Zone::Battlefield,
    );
    state.players[1].life = 30;
    attack_unblocked(&mut state, vec![slasher]);
    // 2 combat damage costs 4 life (26); half of 26 is 13, doubled is 26.
    assert_eq!(state.players[1].life, 0);
    assert!(matches!(
        next(&mut state),
        Decision::GameOver {
            winner: Some(PlayerId::P0)
        }
    ));
}

#[test]
fn bloodletter_modifies_life_payments_and_records_the_actual_loss() {
    let mut state = ready(Step::Main1);
    put(
        &mut state,
        PlayerId::P0,
        "Bloodletter of Aclazotz",
        Zone::Battlefield,
    );
    event::propose_and_commit(&mut state, ProposedEvent::life_payment(PlayerId::P1, 3));
    assert_eq!(state.players[1].life, 14);
    assert!(state.player_lost_life_this_turn_v1(PlayerId::P1));
    event::propose_and_commit(&mut state, ProposedEvent::life_payment(PlayerId::P0, 3));
    assert_eq!(state.players[0].life, 17);
}

#[test]
fn phyrexian_payments_use_printed_affordability_and_modified_life_loss() {
    for starting_life in [20, 2] {
        let mut state = ready(Step::Main1);
        put(
            &mut state,
            PlayerId::P0,
            "Bloodletter of Aclazotz",
            Zone::Battlefield,
        );
        let elf = put(
            &mut state,
            PlayerId::P0,
            "Llanowar Elves",
            Zone::Battlefield,
        );
        let gut_shot = put(&mut state, PlayerId::P1, "Gut Shot", Zone::Hand);
        state.players[1].life = starting_life;
        state.priority_player = PlayerId::P1;
        // The caster has no mana available. Paying printed {R/P} costs two
        // life even when Bloodletter makes the actual life loss four.
        cast(&mut state, gut_shot, &[Target::Object(elf)]);
        let decision = next(&mut state); // Finalize casting and pay the cost.
        assert_eq!(state.players[1].life, starting_life - 4);
        assert!(state.player_lost_life_this_turn_v1(PlayerId::P1));
        if starting_life == 2 {
            assert!(matches!(
                decision,
                Decision::GameOver {
                    winner: Some(PlayerId::P0)
                }
            ));
        } else {
            assert!(matches!(decision, Decision::CastSpellOrPass { .. }));
        }
    }
}

#[test]
fn bloodletter_does_nothing_on_the_opponents_turn() {
    let mut state = ready(Step::Main1);
    put(
        &mut state,
        PlayerId::P1,
        "Bloodletter of Aclazotz",
        Zone::Battlefield,
    );
    let bolt = put(&mut state, PlayerId::P0, "Lightning Bolt", Zone::Hand);
    state.players[0].mana_pool = pool(&[(ManaColor::R, 1)], 0);
    cast(&mut state, bolt, &[Target::Player(PlayerId::P1)]);
    settled(&mut state);
    assert_eq!(state.players[1].life, 17);
}

fn castable(state: &mut GameState) -> Vec<ObjectId> {
    match next(state) {
        Decision::CastSpellOrPass {
            castable_spells, ..
        } => castable_spells,
        other => panic!("expected priority, got {other:?}"),
    }
}

#[test]
fn thalia_taxes_noncreature_spells_from_either_player() {
    let mut state = ready(Step::Main1);
    put(
        &mut state,
        PlayerId::P1,
        "Thalia, Guardian of Thraben",
        Zone::Battlefield,
    );
    let bolt = put(&mut state, PlayerId::P0, "Lightning Bolt", Zone::Hand);
    let scout = put(&mut state, PlayerId::P0, "Cenote Scout", Zone::Hand);
    state.players[0].mana_pool = pool(&[(ManaColor::R, 1)], 0);
    assert!(!castable(&mut state).contains(&bolt));
    state.players[0].mana_pool = pool(&[(ManaColor::G, 1)], 0);
    assert!(castable(&mut state).contains(&scout));

    state.players[0].mana_pool = pool(&[(ManaColor::R, 1)], 1);
    cast(&mut state, bolt, &[Target::Player(PlayerId::P1)]);
    next(&mut state);
    assert_eq!(state.players[0].mana_pool, [0; 6]);
    settled(&mut state);
    assert_eq!(state.players[1].life, 17);
}

#[test]
fn haughty_djinn_counts_instants_and_sorceries_and_discounts_them() {
    let mut state = ready(Step::Main1);
    let djinn = put(&mut state, PlayerId::P0, "Haughty Djinn", Zone::Battlefield);
    assert_eq!(engine::effective_power(&state, djinn), 0);
    put(&mut state, PlayerId::P0, "Lightning Bolt", Zone::Graveyard);
    put(&mut state, PlayerId::P0, "Snap", Zone::Graveyard);
    put(&mut state, PlayerId::P0, "Forest", Zone::Graveyard);
    put(&mut state, PlayerId::P1, "Lightning Bolt", Zone::Graveyard);
    assert_eq!(engine::effective_power(&state, djinn), 2);

    // Snap costs {U}; Lightning Bolt still needs its {R}.
    let theirs = put(&mut state, PlayerId::P1, "Cenote Scout", Zone::Battlefield);
    let snap = put(&mut state, PlayerId::P0, "Snap", Zone::Hand);
    let bolt = put(&mut state, PlayerId::P0, "Lightning Bolt", Zone::Hand);
    state.players[0].mana_pool = pool(&[(ManaColor::U, 1)], 0);
    let offers = castable(&mut state);
    assert!(offers.contains(&snap));
    assert!(!offers.contains(&bolt));
    cast(&mut state, snap, &[Target::Object(theirs)]);
    next(&mut state);
    assert_eq!(state.players[0].mana_pool, [0; 6]);
    settled(&mut state);
    assert_eq!(state.objects.get(theirs).zone, Zone::Hand);
    assert_eq!(engine::effective_power(&state, djinn), 3);
}

#[test]
fn thalia_and_haughty_djinn_cancel_out() {
    let mut state = ready(Step::Main1);
    put(&mut state, PlayerId::P0, "Haughty Djinn", Zone::Battlefield);
    put(
        &mut state,
        PlayerId::P1,
        "Thalia, Guardian of Thraben",
        Zone::Battlefield,
    );
    let theirs = put(&mut state, PlayerId::P1, "Cenote Scout", Zone::Battlefield);
    let snap = put(&mut state, PlayerId::P0, "Snap", Zone::Hand);
    state.players[0].mana_pool = pool(&[(ManaColor::U, 1)], 0);
    assert!(!castable(&mut state).contains(&snap));
    state.players[0].mana_pool = pool(&[(ManaColor::U, 1)], 1);
    cast(&mut state, snap, &[Target::Object(theirs)]);
    next(&mut state);
    assert_eq!(state.players[0].mana_pool, [0; 6]);
}

fn activatable(state: &mut GameState) -> Vec<(ObjectId, u8)> {
    match next(state) {
        Decision::CastSpellOrPass {
            activatable_abilities,
            ..
        } => activatable_abilities,
        other => panic!("expected priority, got {other:?}"),
    }
}

fn combat_hit(state: &mut GameState, source: ObjectId) {
    let mut damage = ProposedEvent::damage(source, Target::Player(PlayerId::P1), 1);
    if let ProposedEvent::Damage(ref mut damage) = damage {
        damage.is_combat = true;
    }
    event::propose_and_commit(state, damage);
    let incarnation = state.objects.get(source).zone_change_count;
    event::log_combat_damage_to_player(state, source, incarnation, PlayerId::P1, 1);
    let pending = trigger::collect_and_process(state);
    state.engine.pending_triggers.extend(pending);
}

#[test]
fn kellan_upgrades_replace_types_keep_the_impulse_trigger_and_reset_on_zone_change() {
    let mut state = ready(Step::Main1);
    let kellan = put(
        &mut state,
        PlayerId::P0,
        "Kellan, Planar Trailblazer",
        Zone::Battlefield,
    );
    combat_hit(&mut state, kellan);
    assert!(state.engine.pending_triggers.is_empty());
    // The conditional ability can be activated before its prerequisite is met.
    state.players[0].mana_pool = pool(&[(ManaColor::R, 1)], 2);
    engine::step(&mut state, Action::ActivateAbility(kellan, 1)).unwrap();
    settled(&mut state);
    assert!(engine::has_effective_subtype(
        &state,
        kellan,
        Subtype::Scout
    ));
    assert_eq!(engine::effective_power(&state, kellan), 2);

    state.players[0].mana_pool = pool(&[(ManaColor::R, 1)], 1);
    engine::step(&mut state, Action::ActivateAbility(kellan, 0)).unwrap();
    settled(&mut state);
    assert!(!engine::has_effective_subtype(
        &state,
        kellan,
        Subtype::Scout
    ));
    assert!(engine::has_effective_subtype(
        &state,
        kellan,
        Subtype::Detective
    ));
    assert!(engine::has_effective_subtype(
        &state,
        kellan,
        Subtype::Faerie
    ));
    state.players[0].mana_pool = pool(&[(ManaColor::R, 1)], 2);
    engine::step(&mut state, Action::ActivateAbility(kellan, 1)).unwrap();
    settled(&mut state);
    assert!(!engine::has_effective_subtype(
        &state,
        kellan,
        Subtype::Detective
    ));
    assert!(engine::has_effective_subtype(
        &state,
        kellan,
        Subtype::Rogue
    ));
    assert_eq!(
        (
            engine::effective_power(&state, kellan),
            engine::effective_toughness(&state, kellan)
        ),
        (3, 2)
    );
    assert!(engine::has_effective_keyword(
        &state,
        kellan,
        Keywords::DOUBLE_STRIKE
    ));
    let top = state.players[0].library[0];
    combat_hit(&mut state, kellan);
    assert_eq!(state.engine.pending_triggers.len(), 1);
    settled(&mut state);
    assert_eq!(state.objects.get(top).zone, Zone::Exile);
    assert_eq!(state.engine.exile_play_permissions.len(), 1);
    state = serde_json::from_str(&serde_json::to_string(&state).unwrap()).unwrap();
    state.step = Step::End;
    for _ in 0..20 {
        if state.active_player == PlayerId::P1 {
            break;
        }
        match next(&mut state) {
            Decision::CastSpellOrPass { .. } => engine::step(&mut state, Action::Pass).unwrap(),
            other => panic!("unexpected {other:?}"),
        }
    }
    assert!(state.engine.exile_play_permissions.is_empty());
    assert!(engine::has_effective_keyword(
        &state,
        kellan,
        Keywords::DOUBLE_STRIKE
    ));
    move_to(&mut state, kellan, Zone::Hand);
    move_to(&mut state, kellan, Zone::Battlefield);
    assert!(state.objects.get(kellan).v4.creature_upgrade.is_none());
    assert!(engine::has_effective_subtype(
        &state,
        kellan,
        Subtype::Scout
    ));
}

#[test]
fn surge_engine_conditions_and_once_limit_apply_at_activation_even_when_countered() {
    let mut state = ready(Step::Main1);
    let surge = put(&mut state, PlayerId::P0, "Surge Engine", Zone::Battlefield);
    state.players[0].mana_pool = pool(&[(ManaColor::U, 6)], 20);
    let choices = activatable(&mut state);
    assert!(choices.contains(&(surge, 0)));
    assert!(!choices.contains(&(surge, 1)));
    assert!(!choices.contains(&(surge, 2)));
    engine::step(&mut state, Action::ActivateAbility(surge, 0)).unwrap();
    settled(&mut state);
    assert!(!engine::has_effective_keyword(
        &state,
        surge,
        Keywords::DEFENDER
    ));
    assert!(engine::has_effective_keyword(
        &state,
        surge,
        Keywords::CANT_BE_BLOCKED
    ));
    engine::step(&mut state, Action::ActivateAbility(surge, 1)).unwrap();
    settled(&mut state);
    assert_eq!(engine::object_color_mask(&state, surge), 2);
    assert_eq!(
        (
            engine::effective_power(&state, surge),
            engine::effective_toughness(&state, surge)
        ),
        (5, 4)
    );
    assert!(activatable(&mut state).contains(&(surge, 2)));
    engine::step(&mut state, Action::ActivateAbility(surge, 2)).unwrap();
    next(&mut state);
    // Countering the ability does not refund the once-only activation.
    state.stack.clear();
    state.priority_player = PlayerId::P0;
    assert!(!activatable(&mut state).contains(&(surge, 2)));
    assert!(engine::step(&mut state, Action::ActivateAbility(surge, 2)).is_err());
    move_to(&mut state, surge, Zone::Hand);
    move_to(&mut state, surge, Zone::Battlefield);
    assert!(state.objects.get(surge).v4.creature_upgrade.is_none());
    assert!(engine::has_effective_keyword(
        &state,
        surge,
        Keywords::DEFENDER
    ));
}

#[test]
fn kellan_upgrade_resolving_after_blink_does_not_change_the_new_incarnation() {
    let mut state = ready(Step::Main1);
    let kellan = put(
        &mut state,
        PlayerId::P0,
        "Kellan, Planar Trailblazer",
        Zone::Battlefield,
    );
    state.players[0].mana_pool = pool(&[(ManaColor::R, 1)], 1);
    engine::step(&mut state, Action::ActivateAbility(kellan, 0)).unwrap();
    next(&mut state);
    move_to(&mut state, kellan, Zone::Hand);
    move_to(&mut state, kellan, Zone::Battlefield);
    settled(&mut state);
    assert!(state.objects.get(kellan).v4.creature_upgrade.is_none());
    assert!(engine::has_effective_subtype(
        &state,
        kellan,
        Subtype::Scout
    ));
}

#[test]
fn gingerbrute_evasion_allows_only_hasty_blockers_and_food_costs_work() {
    let mut state = ready(Step::Main1);
    let ginger = put(&mut state, PlayerId::P0, "Gingerbrute", Zone::Battlefield);
    let hasty = put(&mut state, PlayerId::P1, "Gingerbrute", Zone::Battlefield);
    let ordinary = put(
        &mut state,
        PlayerId::P1,
        "Novice Inspector",
        Zone::Battlefield,
    );
    state.players[0].mana_pool = pool(&[], 1);
    engine::step(&mut state, Action::ActivateAbility(ginger, 0)).unwrap();
    settled(&mut state);
    state.step = Step::DeclareAttackers;
    assert!(matches!(
        next(&mut state),
        Decision::DeclareAttackers { .. }
    ));
    engine::step(&mut state, Action::DeclareAttackers(vec![ginger])).unwrap();
    pass_until_blocks(&mut state);
    let Decision::DeclareBlockers { legal_blockers, .. } = next(&mut state) else {
        panic!("blockers");
    };
    let choices = &legal_blockers
        .iter()
        .find(|(id, _)| *id == ginger)
        .unwrap()
        .1;
    assert!(choices.contains(&hasty));
    assert!(!choices.contains(&ordinary));

    let mut state = ready(Step::Main1);
    let ginger = put(&mut state, PlayerId::P0, "Gingerbrute", Zone::Battlefield);
    state.objects.get_mut(ginger).summoning_sick = true;
    state.players[0].mana_pool = pool(&[], 2);
    assert!(activatable(&mut state).contains(&(ginger, 1)));
    engine::step(&mut state, Action::ActivateAbility(ginger, 1)).unwrap();
    settled(&mut state);
    assert_eq!(state.objects.get(ginger).zone, Zone::Graveyard);
    assert_eq!(state.players[0].life, 23);
}

#[test]
fn tough_cookie_makes_food_animates_only_own_noncreature_artifacts_and_expires() {
    let mut state = ready(Step::Main1);
    let cookie = put(&mut state, PlayerId::P0, "Tough Cookie", Zone::Hand);
    move_to(&mut state, cookie, Zone::Battlefield);
    settled(&mut state);
    let food = battlefield_tokens(&state, PlayerId::P0, "Food Token")[0];
    let opponent_food = put(&mut state, PlayerId::P1, "Food Token", Zone::Battlefield);
    state.players[0].mana_pool = pool(&[(ManaColor::G, 1)], 2);
    engine::step(&mut state, Action::ActivateAbility(cookie, 0)).unwrap();
    let Decision::ChooseTargets { legal_targets, .. } = next(&mut state) else {
        panic!("targets");
    };
    assert!(legal_targets.contains(&Target::Object(food)));
    assert!(!legal_targets.contains(&Target::Object(cookie)));
    assert!(!legal_targets.contains(&Target::Object(opponent_food)));
    engine::step(&mut state, Action::ChooseTarget(Target::Object(food))).unwrap();
    next(&mut state);
    // The animation does not require its source to survive.
    move_to(&mut state, cookie, Zone::Graveyard);
    settled(&mut state);
    assert!(engine::object_has_type(
        &state,
        food,
        mtg_kernel::card_def::CardType::Creature
    ));
    assert!(engine::object_has_type(
        &state,
        food,
        mtg_kernel::card_def::CardType::Artifact
    ));
    assert_eq!(
        (
            engine::effective_power(&state, food),
            engine::effective_toughness(&state, food)
        ),
        (4, 4)
    );
    assert!(engine::has_effective_subtype(&state, food, Subtype::Food));
    state = serde_json::from_str(&serde_json::to_string(&state).unwrap()).unwrap();
    state.step = Step::End;
    for _ in 0..20 {
        if state.active_player == PlayerId::P1 {
            break;
        }
        match next(&mut state) {
            Decision::CastSpellOrPass { .. } => engine::step(&mut state, Action::Pass).unwrap(),
            other => panic!("unexpected {other:?}"),
        }
    }
    assert!(!engine::object_has_type(
        &state,
        food,
        mtg_kernel::card_def::CardType::Creature
    ));
    assert!(state.objects.get(food).v4.creature_upgrade.is_none());
}

#[test]
fn tough_cookie_does_not_animate_a_target_that_has_changed_controller() {
    let mut state = ready(Step::Main1);
    let cookie = put(&mut state, PlayerId::P0, "Tough Cookie", Zone::Battlefield);
    let food = put(&mut state, PlayerId::P0, "Food Token", Zone::Battlefield);
    state.players[0].mana_pool = pool(&[(ManaColor::G, 1)], 2);
    engine::step(&mut state, Action::ActivateAbility(cookie, 0)).unwrap();
    next(&mut state);
    engine::step(&mut state, Action::ChooseTarget(Target::Object(food))).unwrap();
    next(&mut state);
    state.objects.get_mut(food).controller = PlayerId::P1;
    settled(&mut state);
    assert!(!engine::object_has_type(
        &state,
        food,
        mtg_kernel::card_def::CardType::Creature
    ));
}

#[test]
fn faerie_dreamthief_surveils_privately_then_draws_and_loses_life_from_graveyard() {
    let mut state = ready(Step::Main1);
    let faerie = put(&mut state, PlayerId::P0, "Faerie Dreamthief", Zone::Hand);
    let top = state.players[0].library[0];
    move_to(&mut state, faerie, Zone::Battlefield);
    assert!(matches!(
        settle(&mut state),
        Some(Decision::ChooseEffectOption {
            option_count: 2,
            ..
        })
    ));
    state = serde_json::from_str(&serde_json::to_string(&state).unwrap()).unwrap();
    engine::step(&mut state, Action::ChooseEffectOption(1)).unwrap();
    settled(&mut state);
    assert_eq!(state.objects.get(top).zone, Zone::Graveyard);
    assert!(engine::has_effective_keyword(
        &state,
        faerie,
        Keywords::FLYING
    ));
    move_to(&mut state, faerie, Zone::Graveyard);
    state.players[0].mana_pool = pool(&[(ManaColor::B, 1)], 2);
    let hand = state.players[0].hand.len();
    engine::step(&mut state, Action::ActivateAbility(faerie, 0)).unwrap();
    settled(&mut state);
    assert_eq!(state.objects.get(faerie).zone, Zone::Exile);
    assert_eq!(state.players[0].hand.len(), hand + 1);
    assert_eq!(state.players[0].life, 19);
}

fn make_food(state: &mut GameState) {
    let food = card_id_by_name("Food Token").unwrap();
    event::propose_and_commit(state, ProposedEvent::create_token(food, PlayerId::P0));
    let pending = trigger::collect_and_process(state);
    state.engine.pending_triggers.extend(pending);
}

#[test]
fn wurmlet_counts_first_resolution_not_first_trigger_and_resets_between_player_turns() {
    let mut state = ready(Step::Main1);
    let wurmlet = put(
        &mut state,
        PlayerId::P0,
        "Teething Wurmlet",
        Zone::Battlefield,
    );
    make_food(&mut state);
    make_food(&mut state);
    let Decision::OrderTriggers { pending, .. } = next(&mut state) else {
        panic!("order triggers");
    };
    assert_eq!(pending.len(), 2);
    engine::step(&mut state, Action::OrderTriggers(vec![0, 1])).unwrap();
    next(&mut state);
    assert_eq!(state.stack.len(), 2);
    state.stack.pop();
    settled(&mut state);
    assert_eq!(state.players[0].life, 21);
    assert_eq!(state.objects.get(wurmlet).counters.plus1_plus1, 1);
    assert!(!engine::has_effective_keyword(
        &state,
        wurmlet,
        Keywords::DEATHTOUCH
    ));
    make_food(&mut state);
    settled(&mut state);
    assert_eq!(state.players[0].life, 22);
    assert_eq!(state.objects.get(wurmlet).counters.plus1_plus1, 1);
    assert!(engine::has_effective_keyword(
        &state,
        wurmlet,
        Keywords::DEATHTOUCH
    ));
    // GameState.turn is a round, so the opponent's turn has the same number.
    state.active_player = PlayerId::P1;
    state.priority_player = PlayerId::P1;
    make_food(&mut state);
    settled(&mut state);
    assert_eq!(state.objects.get(wurmlet).counters.plus1_plus1, 2);
    make_food(&mut state);
    move_to(&mut state, wurmlet, Zone::Graveyard);
    settled(&mut state);
    assert_eq!(state.players[0].life, 24);
}

#[test]
fn surrak_draws_for_opponent_targeting_creature_spells_and_battlefield_creatures() {
    let mut state = ready(Step::Main1);
    let surrak = put(
        &mut state,
        PlayerId::P0,
        "Surrak, Elusive Hunter",
        Zone::Battlefield,
    );
    let definition = &CARD_DEFS[state.objects.get(surrak).card_def as usize];
    assert!(definition.spell_cannot_be_countered);
    assert!(engine::has_effective_keyword(
        &state,
        surrak,
        Keywords::TRAMPLE
    ));
    let creature = cast_creature(&mut state, "Novice Inspector");
    next(&mut state);
    engine::step(&mut state, Action::Pass).unwrap();
    let counter = put(&mut state, PlayerId::P1, "Counterspell", Zone::Hand);
    state.players[1].mana_pool = pool(&[(ManaColor::U, 2)], 0);
    let before = state.players[0].hand.len();
    cast(&mut state, counter, &[Target::Object(creature)]);
    settled(&mut state);
    assert_eq!(state.players[0].hand.len(), before + 1);
    assert_eq!(state.objects.get(creature).zone, Zone::Graveyard);
    engine::step(&mut state, Action::Pass).unwrap();
    let shock = put(&mut state, PlayerId::P1, "Shock", Zone::Hand);
    state.players[1].mana_pool = pool(&[(ManaColor::R, 1)], 0);
    cast(&mut state, shock, &[Target::Object(surrak)]);
    settled(&mut state);
    assert_eq!(state.players[0].hand.len(), before + 2);
    assert_eq!(state.objects.get(surrak).damage, 2);
}

#[test]
fn bloodtithe_harvester_counts_controlled_blood_tokens_at_resolution() {
    let mut state = ready(Step::Main1);
    let harvester = put(&mut state, PlayerId::P0, "Bloodtithe Harvester", Zone::Hand);
    move_to(&mut state, harvester, Zone::Battlefield);
    settled(&mut state);
    let blood = battlefield_tokens(&state, PlayerId::P0, "Blood Token")[0];
    let second = put(&mut state, PlayerId::P0, "Blood Token", Zone::Battlefield);
    put(&mut state, PlayerId::P1, "Blood Token", Zone::Battlefield);
    let target = put(
        &mut state,
        PlayerId::P1,
        "Hullbreaker Horror",
        Zone::Battlefield,
    );
    state.objects.get_mut(harvester).summoning_sick = false;
    state.step = Step::BeginCombat;
    assert!(!activatable(&mut state).contains(&(harvester, 0)));
    state.step = Step::Main1;
    engine::step(&mut state, Action::ActivateAbility(harvester, 0)).unwrap();
    next(&mut state);
    engine::step(&mut state, Action::ChooseTarget(Target::Object(target))).unwrap();
    next(&mut state);
    assert_eq!(state.objects.get(harvester).zone, Zone::Graveyard);
    move_to(&mut state, second, Zone::Exile);
    settled(&mut state);
    assert_eq!(engine::effective_power(&state, target), 5);
    assert_eq!(engine::effective_toughness(&state, target), 6);
    assert_eq!(state.objects.get(blood).zone, Zone::Battlefield);
}

#[test]
fn sandstorm_salvager_boosts_only_current_controlled_creature_tokens() {
    let mut state = ready(Step::Main1);
    let salvager = put(&mut state, PlayerId::P0, "Sandstorm Salvager", Zone::Hand);
    move_to(&mut state, salvager, Zone::Battlefield);
    settled(&mut state);
    let golem = battlefield_tokens(&state, PlayerId::P0, "Golem Token")[0];
    let other = put(&mut state, PlayerId::P1, "Golem Token", Zone::Battlefield);
    let food = put(&mut state, PlayerId::P0, "Food Token", Zone::Battlefield);
    state.objects.get_mut(salvager).summoning_sick = false;
    state.players[0].mana_pool = pool(&[], 2);
    engine::step(&mut state, Action::ActivateAbility(salvager, 0)).unwrap();
    next(&mut state);
    move_to(&mut state, salvager, Zone::Graveyard);
    settled(&mut state);
    assert_eq!(state.objects.get(golem).counters.plus1_plus1, 1);
    assert_eq!(engine::effective_power(&state, golem), 4);
    assert!(engine::has_effective_keyword(
        &state,
        golem,
        Keywords::TRAMPLE
    ));
    assert_eq!(state.objects.get(other).counters.plus1_plus1, 0);
    assert_eq!(state.objects.get(food).counters.plus1_plus1, 0);
    let late = put(&mut state, PlayerId::P0, "Golem Token", Zone::Battlefield);
    assert!(!engine::has_effective_keyword(
        &state,
        late,
        Keywords::TRAMPLE
    ));
    state.step = Step::End;
    for _ in 0..20 {
        if state.active_player == PlayerId::P1 {
            break;
        }
        match next(&mut state) {
            Decision::CastSpellOrPass { .. } => engine::step(&mut state, Action::Pass).unwrap(),
            other => panic!("unexpected {other:?}"),
        }
    }
    assert!(!engine::has_effective_keyword(
        &state,
        golem,
        Keywords::TRAMPLE
    ));
    assert_eq!(state.objects.get(golem).counters.plus1_plus1, 1);
}

#[test]
fn preacher_life_comparison_is_evaluated_when_it_attacks_and_ties_trigger_both() {
    for (life, expected_tokens, expected_draws) in [(10, 1, 0), (20, 1, 1), (30, 0, 1)] {
        let mut state = ready(Step::DeclareAttackers);
        state.players[0].life = life;
        let preacher = put(
            &mut state,
            PlayerId::P0,
            "Preacher of the Schism",
            Zone::Battlefield,
        );
        attack_unblocked(&mut state, vec![preacher]);
        assert_eq!(
            battlefield_tokens(&state, PlayerId::P0, "Vampire Token").len(),
            expected_tokens
        );
        assert_eq!(state.players[0].hand.len(), expected_draws);
        assert_eq!(state.players[0].life, life - expected_draws as i32);
        for token in battlefield_tokens(&state, PlayerId::P0, "Vampire Token") {
            assert!(engine::has_effective_keyword(
                &state,
                token,
                Keywords::LIFELINK
            ));
            assert_eq!(engine::object_color_mask(&state, token), 1);
        }
    }
    let mut state = ready(Step::DeclareAttackers);
    state.players[0].life = 30;
    let preacher = put(
        &mut state,
        PlayerId::P0,
        "Preacher of the Schism",
        Zone::Battlefield,
    );
    next(&mut state);
    engine::step(&mut state, Action::DeclareAttackers(vec![preacher])).unwrap();
    // Losing the lead after the trigger is created does not cancel it.
    state.players[0].life = 10;
    pass_until_blocks(&mut state);
    assert_eq!(state.players[0].hand.len(), 1);
    assert_eq!(state.players[0].life, 9);
    assert!(battlefield_tokens(&state, PlayerId::P0, "Vampire Token").is_empty());
}

#[test]
fn hired_claw_pings_when_lizards_attack() {
    let mut state = ready(Step::DeclareAttackers);
    put(&mut state, PlayerId::P0, "Hired Claw", Zone::Battlefield);
    let scout = put(&mut state, PlayerId::P0, "Cenote Scout", Zone::Battlefield);
    // A non-Lizard attacking alone does not trigger it.
    attack_unblocked(&mut state, vec![scout]);
    assert_eq!(state.players[1].life, 19);

    let mut state = ready(Step::DeclareAttackers);
    let claw = put(&mut state, PlayerId::P0, "Hired Claw", Zone::Battlefield);
    assert!(matches!(
        next(&mut state),
        Decision::DeclareAttackers { .. }
    ));
    engine::step(&mut state, Action::DeclareAttackers(vec![claw])).unwrap();
    // The trigger targets the only opponent.
    loop {
        match next(&mut state) {
            Decision::ChooseTargets { legal_targets, .. } => {
                assert_eq!(legal_targets, vec![Target::Player(PlayerId::P1)]);
                engine::step(&mut state, Action::ChooseTarget(legal_targets[0])).unwrap();
            }
            Decision::CastSpellOrPass { .. } => engine::step(&mut state, Action::Pass).unwrap(),
            Decision::DeclareBlockers { .. } => break,
            other => panic!("unexpected {other:?}"),
        }
    }
    assert_eq!(state.players[1].life, 19);
}

#[test]
fn hired_claw_grows_once_a_turn_after_an_opponent_lost_life() {
    let mut state = ready(Step::Main1);
    let claw = put(&mut state, PlayerId::P0, "Hired Claw", Zone::Battlefield);
    state.players[0].mana_pool = pool(&[(ManaColor::R, 2)], 2);
    assert!(!activatable(&mut state).contains(&(claw, 0)));

    // P0 losing life does not count.
    event::propose_and_commit(&mut state, ProposedEvent::life_loss(PlayerId::P0, 1));
    assert!(!activatable(&mut state).contains(&(claw, 0)));

    event::propose_and_commit(&mut state, ProposedEvent::life_loss(PlayerId::P1, 1));
    assert!(activatable(&mut state).contains(&(claw, 0)));
    engine::step(&mut state, Action::ActivateAbility(claw, 0)).unwrap();
    settled(&mut state);
    assert_eq!(state.objects.get(claw).counters.plus1_plus1, 1);
    assert!(!activatable(&mut state).contains(&(claw, 0)));
}

#[test]
fn warden_flies_and_has_vigilance_with_three_counters() {
    let mut state = ready(Step::Main1);
    let warden = put(
        &mut state,
        PlayerId::P0,
        "Warden of the Inner Sky",
        Zone::Battlefield,
    );
    state.objects.get_mut(warden).counters.plus1_plus1 = 2;
    assert!(!engine::has_effective_keyword(
        &state,
        warden,
        Keywords::FLYING
    ));
    state.objects.get_mut(warden).counters.stun = 1;
    assert!(engine::has_effective_keyword(
        &state,
        warden,
        Keywords::FLYING
    ));
    assert!(engine::has_effective_keyword(
        &state,
        warden,
        Keywords::VIGILANCE
    ));
}

#[test]
fn warden_taps_three_artifacts_or_creatures_to_grow_and_scry() {
    let mut state = ready(Step::Main1);
    let warden = put(
        &mut state,
        PlayerId::P0,
        "Warden of the Inner Sky",
        Zone::Battlefield,
    );
    let scout = put(&mut state, PlayerId::P0, "Cenote Scout", Zone::Battlefield);
    // Two creatures are not enough.
    assert!(!activatable(&mut state).contains(&(warden, 0)));

    // A summoning-sick creature can still be tapped for the cost.
    let sick = put(
        &mut state,
        PlayerId::P0,
        "Novice Inspector",
        Zone::Battlefield,
    );
    state.objects.get_mut(sick).summoning_sick = true;
    let spare = put(&mut state, PlayerId::P0, "Hired Claw", Zone::Battlefield);
    assert!(activatable(&mut state).contains(&(warden, 0)));
    engine::step(&mut state, Action::ActivateAbility(warden, 0)).unwrap();
    for pick in [warden, sick, scout] {
        match next(&mut state) {
            Decision::ChooseCostTargets {
                cost_kind: CostKind::TapPermanents,
                candidates,
                ..
            } => {
                assert!(candidates.contains(&pick) && candidates.contains(&spare));
                engine::step(&mut state, Action::ChooseCostTarget(pick)).unwrap();
            }
            other => panic!("expected a tap-cost pick, got {other:?}"),
        }
    }
    let mut scried = false;
    while let Some(decision) = settle(&mut state) {
        scried = true;
        match decision {
            // Scry 1: keep the card on top.
            Decision::ChooseEffectTargets {
                can_finish: true, ..
            } => engine::step(&mut state, Action::FinishEffectSelection).unwrap(),
            other => panic!("unexpected {other:?}"),
        }
    }
    assert!(scried);
    for tapped in [warden, sick, scout] {
        assert!(state.objects.get(tapped).tapped);
    }
    assert!(!state.objects.get(spare).tapped);
    assert_eq!(state.objects.get(warden).counters.plus1_plus1, 1);
}

/// Casts Extraction Specialist, answers its target with `target` if asked,
/// and settles every other trigger.
fn cast_specialist(state: &mut GameState, target: ObjectId) -> ObjectId {
    let specialist = cast_creature(state, "Extraction Specialist");
    loop {
        match settle(state) {
            Some(Decision::ChooseTargets { legal_targets, .. }) => {
                assert_eq!(legal_targets, vec![Target::Object(target)]);
                engine::step(state, Action::ChooseTarget(Target::Object(target))).unwrap();
            }
            None => return specialist,
            Some(other) => panic!("unexpected {other:?}"),
        }
    }
}

#[test]
fn extraction_specialist_returns_a_cheap_creature_that_cannot_fight() {
    let mut state = ready(Step::Main1);
    let inspector = put(
        &mut state,
        PlayerId::P0,
        "Novice Inspector",
        Zone::Graveyard,
    );
    put(
        &mut state,
        PlayerId::P0,
        "Troll of Khazad-dum",
        Zone::Graveyard,
    );
    put(&mut state, PlayerId::P0, "Lightning Bolt", Zone::Graveyard);
    put(&mut state, PlayerId::P1, "Cenote Scout", Zone::Graveyard);
    let specialist = cast_specialist(&mut state, inspector);
    assert_eq!(state.objects.get(inspector).zone, Zone::Battlefield);
    assert_eq!(state.objects.get(inspector).controller, PlayerId::P0);
    // The Inspector's own entry trigger still happened.
    assert_eq!(
        battlefield_tokens(&state, PlayerId::P0, "Clue Token").len(),
        1
    );

    for id in [inspector, specialist] {
        state.objects.get_mut(id).summoning_sick = false;
    }
    state.step = Step::DeclareAttackers;
    match next(&mut state) {
        Decision::DeclareAttackers { eligible, .. } => {
            assert!(eligible.contains(&specialist));
            assert!(!eligible.contains(&inspector));
        }
        other => panic!("unexpected {other:?}"),
    }

    // Once P0 no longer controls the Specialist, the Inspector can attack.
    move_to(&mut state, specialist, Zone::Graveyard);
    match next(&mut state) {
        Decision::DeclareAttackers { eligible, .. } => assert!(eligible.contains(&inspector)),
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn extraction_specialist_creature_cannot_block_either() {
    let mut state = ready(Step::Main1);
    let inspector = put(
        &mut state,
        PlayerId::P0,
        "Novice Inspector",
        Zone::Graveyard,
    );
    let specialist = cast_specialist(&mut state, inspector);

    // P1 attacks: the Specialist may block, the returned Inspector may not.
    state.active_player = PlayerId::P1;
    state.priority_player = PlayerId::P1;
    state.step = Step::DeclareAttackers;
    let attacker = put(&mut state, PlayerId::P1, "Cenote Scout", Zone::Battlefield);
    assert!(matches!(
        next(&mut state),
        Decision::DeclareAttackers { .. }
    ));
    engine::step(&mut state, Action::DeclareAttackers(vec![attacker])).unwrap();
    loop {
        match next(&mut state) {
            Decision::CastSpellOrPass { .. } => engine::step(&mut state, Action::Pass).unwrap(),
            Decision::DeclareBlockers { legal_blockers, .. } => {
                let blockers = &legal_blockers
                    .iter()
                    .find(|(id, _)| *id == attacker)
                    .unwrap()
                    .1;
                assert!(blockers.contains(&specialist));
                assert!(!blockers.contains(&inspector));
                break;
            }
            other => panic!("unexpected {other:?}"),
        }
    }
}

#[test]
fn extraction_specialist_gone_before_resolution_returns_unrestricted() {
    let mut state = ready(Step::Main1);
    let inspector = put(
        &mut state,
        PlayerId::P0,
        "Novice Inspector",
        Zone::Graveyard,
    );
    let specialist = cast_creature(&mut state, "Extraction Specialist");
    // Resolve the creature spell, then answer the trigger's target.
    loop {
        match next(&mut state) {
            Decision::CastSpellOrPass { .. } => engine::step(&mut state, Action::Pass).unwrap(),
            Decision::ChooseTargets { .. } => {
                engine::step(&mut state, Action::ChooseTarget(Target::Object(inspector))).unwrap();
                break;
            }
            other => panic!("unexpected {other:?}"),
        }
    }
    move_to(&mut state, specialist, Zone::Graveyard);
    settled(&mut state);
    assert_eq!(state.objects.get(inspector).zone, Zone::Battlefield);
    assert!(state.attack_block_restrictions_v1.is_none());
}

/// P1 casts Lightning Bolt at P0 while P0 has priority to respond, leaving
/// P0 with priority. Returns the Bolt.
fn opponent_bolts_face(state: &mut GameState) -> ObjectId {
    let bolt = put(state, PlayerId::P1, "Lightning Bolt", Zone::Hand);
    state.players[1].mana_pool = pool(&[(ManaColor::R, 1)], 0);
    engine::step(state, Action::Pass).unwrap();
    cast(state, bolt, &[Target::Player(PlayerId::P0)]);
    assert!(matches!(
        next(state),
        Decision::CastSpellOrPass {
            player: PlayerId::P1,
            ..
        }
    ));
    engine::step(state, Action::Pass).unwrap();
    bolt
}

/// P0 casts Lightning Bolt at P1 with Hullbreaker Horror out, and returns
/// the Bolt and the trigger's legal modes.
fn bolt_with_hullbreaker(state: &mut GameState) -> (ObjectId, Vec<u8>) {
    let bolt = put(state, PlayerId::P0, "Lightning Bolt", Zone::Hand);
    state.players[0].mana_pool = pool(&[(ManaColor::R, 1)], 0);
    cast(state, bolt, &[Target::Player(PlayerId::P1)]);
    match settle(state) {
        Some(Decision::ChooseTriggerMode {
            player: PlayerId::P0,
            mode_count: 3,
            legal_modes,
            ..
        }) => (bolt, legal_modes),
        other => panic!("expected a Hullbreaker mode choice, got {other:?}"),
    }
}

fn choose_mode_and_target(state: &mut GameState, mode: u8, target: Target, legal: &[Target]) {
    engine::step(state, Action::ChooseTriggerMode(mode)).unwrap();
    match next(state) {
        Decision::ChooseTargets { legal_targets, .. } => {
            assert_eq!(legal_targets, legal);
            engine::step(state, Action::ChooseTarget(target)).unwrap();
        }
        other => panic!("expected a target choice, got {other:?}"),
    }
}

#[test]
fn hullbreaker_horror_has_flash_and_cannot_be_countered() {
    let def = &CARD_DEFS[card_id_by_name("Hullbreaker Horror").unwrap() as usize];
    assert!(def.spell_cannot_be_countered);

    let mut state = ready(Step::Main1);
    let their_bolt = opponent_bolts_face(&mut state);
    // Flash: P0 casts it in response, with a spell already on the stack.
    let horror = cast_creature(&mut state, "Hullbreaker Horror");
    let counterspell = put(&mut state, PlayerId::P1, "Counterspell", Zone::Hand);
    state.players[1].mana_pool = pool(&[(ManaColor::U, 2)], 0);
    assert!(matches!(
        next(&mut state),
        Decision::CastSpellOrPass {
            player: PlayerId::P0,
            ..
        }
    ));
    engine::step(&mut state, Action::Pass).unwrap();
    cast(&mut state, counterspell, &[Target::Object(horror)]);
    settled(&mut state);
    assert_eq!(state.objects.get(horror).zone, Zone::Battlefield);
    assert_eq!(state.objects.get(counterspell).zone, Zone::Graveyard);
    assert_eq!(state.objects.get(their_bolt).zone, Zone::Graveyard);
    assert_eq!(state.players[0].life, 17);
}

#[test]
fn hullbreaker_horror_returns_an_opponents_spell_to_hand() {
    let mut state = ready(Step::Main1);
    let horror = put(
        &mut state,
        PlayerId::P0,
        "Hullbreaker Horror",
        Zone::Battlefield,
    );
    let their_bolt = opponent_bolts_face(&mut state);
    let (my_bolt, legal_modes) = bolt_with_hullbreaker(&mut state);
    assert_eq!(legal_modes, vec![0, 1, 2]);
    // Only the opponent's spell is a legal target, not P0's own Bolt.
    choose_mode_and_target(
        &mut state,
        0,
        Target::Object(their_bolt),
        &[Target::Object(their_bolt)],
    );
    settled(&mut state);
    assert_eq!(state.objects.get(their_bolt).zone, Zone::Hand);
    assert!(state.players[1].hand.contains(&their_bolt));
    assert_eq!(state.objects.get(my_bolt).zone, Zone::Graveyard);
    assert_eq!(state.objects.get(horror).zone, Zone::Battlefield);
    assert_eq!(state.players[0].life, 20);
    assert_eq!(state.players[1].life, 17);
}

#[test]
fn hullbreaker_horror_returns_a_nonland_permanent_or_nothing() {
    let mut state = ready(Step::Main1);
    let horror = put(
        &mut state,
        PlayerId::P0,
        "Hullbreaker Horror",
        Zone::Battlefield,
    );
    let scout = put(&mut state, PlayerId::P1, "Cenote Scout", Zone::Battlefield);
    put(&mut state, PlayerId::P1, "Forest", Zone::Battlefield);
    // With no opponent spell on the stack, the spell mode is not legal.
    let (_, legal_modes) = bolt_with_hullbreaker(&mut state);
    assert_eq!(legal_modes, vec![1, 2]);
    choose_mode_and_target(
        &mut state,
        1,
        Target::Object(scout),
        &[Target::Object(horror), Target::Object(scout)],
    );
    settled(&mut state);
    assert_eq!(state.objects.get(scout).zone, Zone::Hand);
    assert!(state.players[1].hand.contains(&scout));
    assert_eq!(state.players[1].life, 17);

    // "Up to one": the last mode does nothing.
    let (_, legal_modes) = bolt_with_hullbreaker(&mut state);
    assert_eq!(legal_modes, vec![1, 2]);
    engine::step(&mut state, Action::ChooseTriggerMode(2)).unwrap();
    settled(&mut state);
    assert_eq!(state.objects.get(horror).zone, Zone::Battlefield);
    assert_eq!(state.players[1].life, 14);
}

/// Puts new cards on top of P0's library, first name on top.
fn stack_library_top(state: &mut GameState, names: &[&str]) -> Vec<ObjectId> {
    let ids: Vec<ObjectId> = names
        .iter()
        .map(|name| {
            let card_def = card_id_by_name(name).unwrap();
            state.objects.push(GameObject {
                card_def,
                name: CARD_DEFS[card_def as usize].object_name.into(),
                owner: PlayerId::P0,
                controller: PlayerId::P0,
                zone: Zone::Library,
                tapped: false,
                summoning_sick: false,
                damage: 0,
                counters: Default::default(),
                attachments: Vec::new(),
                v4: ObjectStateV4::from_card_def(card_def),
                spell_copy_origin: None,
                plotted_turn: None,
                zone_change_count: 0,
            })
        })
        .collect();
    let library = &mut state.players[0].library;
    library.splice(0..0, ids.iter().copied());
    ids
}

/// Activates Recruitment Officer with its mana in the pool and returns the
/// cards it may take, or None if it asks nothing.
fn activate_officer(state: &mut GameState, officer: ObjectId) -> Option<Vec<Target>> {
    state.players[0].mana_pool = pool(&[(ManaColor::W, 1)], 3);
    assert!(activatable(state).contains(&(officer, 0)));
    engine::step(state, Action::ActivateAbility(officer, 0)).unwrap();
    match settle(state) {
        Some(Decision::ChooseEffectTargets {
            player: PlayerId::P0,
            min_targets: 0,
            max_targets: 1,
            legal_targets,
            ..
        }) => Some(legal_targets),
        None => None,
        Some(other) => panic!("unexpected {other:?}"),
    }
}

fn library_bottom(state: &GameState, count: usize) -> Vec<ObjectId> {
    let library = &state.players[0].library;
    library[library.len() - count..].to_vec()
}

#[test]
fn recruitment_officer_takes_a_cheap_creature_and_bottoms_the_rest() {
    let mut state = ready(Step::Main1);
    let officer = put(
        &mut state,
        PlayerId::P0,
        "Recruitment Officer",
        Zone::Battlefield,
    );
    let top = stack_library_top(
        &mut state,
        &[
            "Troll of Khazad-dum",
            "Novice Inspector",
            "Lightning Bolt",
            "Cenote Scout",
            "Hullbreaker Horror",
        ],
    );
    let (troll, inspector, bolt, scout, fifth) = (top[0], top[1], top[2], top[3], top[4]);
    let library_len = state.players[0].library.len();
    // Only creatures with mana value 3 or less among the top four.
    let legal = activate_officer(&mut state, officer).unwrap();
    assert_eq!(
        legal,
        vec![Target::Object(inspector), Target::Object(scout)]
    );
    engine::step(
        &mut state,
        Action::ChooseEffectTarget(Target::Object(scout)),
    )
    .unwrap();
    while let Some(decision) = settle(&mut state) {
        match decision {
            Decision::ChooseEffectTargets {
                can_finish: true, ..
            } => engine::step(&mut state, Action::FinishEffectSelection).unwrap(),
            other => panic!("unexpected {other:?}"),
        }
    }
    assert_eq!(state.objects.get(scout).zone, Zone::Hand);
    assert!(state.players[0].hand.contains(&scout));
    assert_eq!(state.players[0].library.len(), library_len - 1);
    assert_eq!(state.players[0].library[0], fifth);
    assert_same_members(library_bottom(&state, 3), vec![troll, inspector, bolt]);
    assert_eq!(state.players[0].mana_pool, [0; 6]);
}

#[test]
fn recruitment_officer_may_take_nothing() {
    let mut state = ready(Step::Main1);
    let officer = put(
        &mut state,
        PlayerId::P0,
        "Recruitment Officer",
        Zone::Battlefield,
    );
    let top = stack_library_top(
        &mut state,
        &["Novice Inspector", "Forest", "Forest", "Forest"],
    );
    let hand = state.players[0].hand.len();
    assert!(activate_officer(&mut state, officer).is_some());
    engine::step(&mut state, Action::FinishEffectSelection).unwrap();
    settled(&mut state);
    assert_eq!(state.players[0].hand.len(), hand);
    assert_same_members(library_bottom(&state, 4), top);
}

#[test]
fn recruitment_officer_with_no_cheap_creature_asks_nothing() {
    let mut state = ready(Step::Main1);
    let officer = put(
        &mut state,
        PlayerId::P0,
        "Recruitment Officer",
        Zone::Battlefield,
    );
    let top = stack_library_top(
        &mut state,
        &[
            "Troll of Khazad-dum",
            "Lightning Bolt",
            "Forest",
            "Hullbreaker Horror",
        ],
    );
    let hand = state.players[0].hand.len();
    assert_eq!(activate_officer(&mut state, officer), None);
    assert_eq!(state.players[0].hand.len(), hand);
    assert_same_members(library_bottom(&state, 4), top);
}

fn assert_same_members(mut actual: Vec<ObjectId>, mut expected: Vec<ObjectId>) {
    actual.sort_unstable();
    expected.sort_unstable();
    assert_eq!(actual, expected);
}

#[test]
fn outgrowing_entrant_uses_last_known_stats_after_leaving_and_returning() {
    for name in ["Sharp-Eyed Rookie", "Evolving Adaptive"] {
        let mut state = ready(Step::Main1);
        let source = put(&mut state, PlayerId::P0, name, Zone::Hand);
        move_to(&mut state, source, Zone::Battlefield);
        settled(&mut state);
        let entrant = put(&mut state, PlayerId::P0, "Novice Inspector", Zone::Hand);
        state.objects.get_mut(entrant).counters.plus1_plus1 = 5;
        move_to(&mut state, entrant, Zone::Battlefield);
        // Entry resets counters; grow before trigger collection from the
        // next entrant instead, and preserve that incarnation on departure.
        settled(&mut state);
        let entrant = put(&mut state, PlayerId::P0, "Troll of Khazad-dum", Zone::Hand);
        move_to(&mut state, entrant, Zone::Battlefield);
        move_to(&mut state, entrant, Zone::Hand);
        move_to(&mut state, entrant, Zone::Graveyard);
        settled(&mut state);
        if name == "Sharp-Eyed Rookie" {
            assert_eq!(state.objects.get(source).counters.plus1_plus1, 1);
            assert_eq!(
                battlefield_tokens(&state, PlayerId::P0, "Clue Token").len(),
                2
            );
        } else {
            // Inspector's 2 toughness exceeded the initial 1/1 Adaptive.
            assert_eq!(state.objects.get(source).counters.oil, 3);
        }
    }
}

#[test]
fn rookie_still_investigates_if_both_creatures_left_before_resolution() {
    let mut state = ready(Step::Main1);
    let rookie = put(
        &mut state,
        PlayerId::P0,
        "Sharp-Eyed Rookie",
        Zone::Battlefield,
    );
    let entrant = put(&mut state, PlayerId::P0, "Troll of Khazad-dum", Zone::Hand);
    move_to(&mut state, entrant, Zone::Battlefield);
    event::propose_and_commit_batch(
        &mut state,
        vec![
            ProposedEvent::zone_change(rookie, Zone::Graveyard),
            ProposedEvent::zone_change(entrant, Zone::Graveyard),
        ],
    );
    settled(&mut state);
    assert_eq!(
        battlefield_tokens(&state, PlayerId::P0, "Clue Token").len(),
        1
    );
}

#[test]
fn specialist_restriction_does_not_restart_after_regaining_control() {
    let mut state = ready(Step::Main1);
    let inspector = put(
        &mut state,
        PlayerId::P0,
        "Novice Inspector",
        Zone::Graveyard,
    );
    let specialist = cast_specialist(&mut state, inspector);
    assert!(state.attack_block_restrictions_v1.is_some());
    state.objects.get_mut(specialist).controller = PlayerId::P1;
    state.players[0].battlefield.retain(|&id| id != specialist);
    state.players[1].battlefield.push(specialist);
    next(&mut state);
    assert!(state.attack_block_restrictions_v1.is_none());
    state.objects.get_mut(specialist).controller = PlayerId::P0;
    state.players[1].battlefield.retain(|&id| id != specialist);
    state.players[0].battlefield.push(specialist);
    state.objects.get_mut(inspector).summoning_sick = false;
    state.step = Step::DeclareAttackers;
    match next(&mut state) {
        Decision::DeclareAttackers { eligible, .. } => assert!(eligible.contains(&inspector)),
        other => panic!("{other:?}"),
    }
}

#[test]
fn recruitment_randomizes_bottom_without_revealing_its_positions() {
    let mut outcomes = std::collections::BTreeSet::new();
    for seed in 0..16 {
        let mut state = GameState::new_from_libraries(
            &[card_id_by_name("Forest").unwrap(); 40],
            &[card_id_by_name("Forest").unwrap(); 40],
            |id| CARD_DEFS[id as usize].object_name.into(),
            seed,
        );
        state.step = Step::Main1;
        let officer = put(
            &mut state,
            PlayerId::P0,
            "Recruitment Officer",
            Zone::Battlefield,
        );
        let top = stack_library_top(
            &mut state,
            &[
                "Lightning Bolt",
                "Forest",
                "Hullbreaker Horror",
                "Troll of Khazad-dum",
            ],
        );
        let tail = state.players[0].library[4..].to_vec();
        assert_eq!(activate_officer(&mut state, officer), None);
        let bottom = library_bottom(&state, 4);
        assert_same_members(bottom.clone(), top);
        assert_eq!(&state.players[0].library[..tail.len()], &tail);
        for observer in [PlayerId::P0, PlayerId::P1] {
            assert!(state
                .known_library_cards(observer, PlayerId::P0)
                .iter()
                .all(|entry| (entry.position as usize) < tail.len()));
        }
        outcomes.insert(bottom);
    }
    assert!(outcomes.len() > 1);
}

#[test]
fn djinn_reduces_flashback_and_thalia_increases_it() {
    for (djinns, thalias, generic) in [(1, 0, 0), (0, 1, 2), (2, 1, 0)] {
        let mut state = ready(Step::Main1);
        for _ in 0..djinns {
            put(&mut state, PlayerId::P0, "Haughty Djinn", Zone::Battlefield);
        }
        for _ in 0..thalias {
            put(
                &mut state,
                PlayerId::P1,
                "Thalia, Guardian of Thraben",
                Zone::Battlefield,
            );
        }
        let spell = put(&mut state, PlayerId::P0, "Lava Dart", Zone::Graveyard);
        // Lava Dart's flashback has no mana base: sacrifice a Mountain.
        let mountain = put(&mut state, PlayerId::P0, "Mountain", Zone::Battlefield);
        state.objects.get_mut(mountain).tapped = true;
        let expected = (thalias as u8).saturating_sub(djinns as u8);
        state.players[0].mana_pool = pool(&[], expected);
        assert!(
            castable(&mut state).contains(&spell),
            "{djinns}, {thalias}, {generic}"
        );
        engine::step(&mut state, Action::CastSpell(spell)).unwrap();
        loop {
            match next(&mut state) {
                Decision::ChooseTargets { .. } => engine::step(
                    &mut state,
                    Action::ChooseTarget(Target::Player(PlayerId::P1)),
                )
                .unwrap(),
                Decision::ChooseCostTargets { .. } => {
                    engine::step(&mut state, Action::ChooseCostTarget(mountain)).unwrap()
                }
                Decision::CastSpellOrPass { .. } => break,
                other => panic!("{other:?}"),
            }
        }
        assert_eq!(state.players[0].mana_pool, [0; 6]);
        assert_eq!(state.objects.get(mountain).zone, Zone::Graveyard);
        settled(&mut state);
        assert_eq!(state.objects.get(spell).zone, Zone::Exile);
        assert_eq!(state.players[1].life, 19);
    }
}

#[test]
fn djinn_reduction_includes_kicker_and_floors_after_thalia_tax() {
    let mut state = ready(Step::Main1);
    for _ in 0..2 {
        put(&mut state, PlayerId::P0, "Haughty Djinn", Zone::Battlefield);
    }
    put(
        &mut state,
        PlayerId::P1,
        "Thalia, Guardian of Thraben",
        Zone::Battlefield,
    );
    let spell = put(&mut state, PlayerId::P0, "Burst Lightning", Zone::Hand);
    // Printed {R} + kicker {4} + tax {1} - two Djinn = {3}{R}.
    state.players[0].mana_pool = pool(&[(ManaColor::R, 1)], 3);
    next(&mut state);
    engine::step(&mut state, Action::CastSpell(spell)).unwrap();
    assert!(matches!(next(&mut state), Decision::ChooseKicker { .. }));
    engine::step(&mut state, Action::ChooseKicker(true)).unwrap();
    assert!(matches!(next(&mut state), Decision::ChooseTargets { .. }));
    engine::step(
        &mut state,
        Action::ChooseTarget(Target::Player(PlayerId::P1)),
    )
    .unwrap();
    next(&mut state);
    assert_eq!(state.players[0].mana_pool, [0; 6]);
    settled(&mut state);
    assert_eq!(state.players[1].life, 16);
}

#[test]
fn siren_and_reinforcements_create_their_tokens_on_entry() {
    for (name, token) in [
        ("Spyglass Siren", "Map Token"),
        ("Resolute Reinforcements", "Soldier Token"),
    ] {
        let mut state = ready(Step::Main1);
        cast_creature(&mut state, name);
        settled(&mut state);
        assert_eq!(battlefield_tokens(&state, PlayerId::P0, token).len(), 1);
    }
}

#[test]
fn sheoldred_counts_every_actual_draw_and_confidant_reveal_is_not_a_draw() {
    let mut state = ready(Step::Main1);
    put(
        &mut state,
        PlayerId::P0,
        "Sheoldred, the Apocalypse",
        Zone::Battlefield,
    );
    for _ in 0..2 {
        event::propose_and_commit(&mut state, ProposedEvent::draw(PlayerId::P0));
    }
    for _ in 0..3 {
        event::propose_and_commit(&mut state, ProposedEvent::draw(PlayerId::P1));
    }
    settled(&mut state);
    assert_eq!(state.players[0].life, 24);
    assert_eq!(state.players[1].life, 14);
    put(
        &mut state,
        PlayerId::P0,
        "Dark Confidant",
        Zone::Battlefield,
    );
    let top = stack_library_top(&mut state, &["Hullbreaker Horror"])[0];
    event::log_upkeep_began(&mut state, PlayerId::P0);
    settled(&mut state);
    assert_eq!(state.players[0].life, 17);
    assert_eq!(state.objects.get(top).zone, Zone::Hand);
    assert!(state
        .known_hand_cards(PlayerId::P1, PlayerId::P0)
        .iter()
        .any(|fact| fact.object == top));
}

#[test]
fn bunnicorn_counts_nonland_permanents_in_all_zones() {
    let mut state = ready(Step::Main1);
    let bunny = put(&mut state, PlayerId::P0, "Regal Bunnicorn", Zone::Hand);
    let clue = put(&mut state, PlayerId::P0, "Clue Token", Zone::Battlefield);
    put(&mut state, PlayerId::P0, "Forest", Zone::Battlefield);
    put(
        &mut state,
        PlayerId::P1,
        "Novice Inspector",
        Zone::Battlefield,
    );
    assert_eq!(
        (
            engine::effective_power(&state, bunny),
            engine::effective_toughness(&state, bunny)
        ),
        (1, 1)
    );
    move_to(&mut state, bunny, Zone::Battlefield);
    settled(&mut state);
    assert_eq!(
        (
            engine::effective_power(&state, bunny),
            engine::effective_toughness(&state, bunny)
        ),
        (2, 2)
    );
    state.objects.get_mut(bunny).counters.plus1_plus1 = 2;
    move_to(&mut state, clue, Zone::Graveyard);
    assert_eq!(
        (
            engine::effective_power(&state, bunny),
            engine::effective_toughness(&state, bunny)
        ),
        (3, 3)
    );
}

#[test]
fn quirion_allocations_survive_source_moves_and_do_not_redistribute_illegal_shares() {
    let mut state = ready(Step::Main1);
    let source = put(
        &mut state,
        PlayerId::P0,
        "Quirion Beastcaller",
        Zone::Battlefield,
    );
    let first = put(&mut state, PlayerId::P0, "Cenote Scout", Zone::Battlefield);
    let second = put(
        &mut state,
        PlayerId::P0,
        "Novice Inspector",
        Zone::Battlefield,
    );
    state.objects.get_mut(source).counters.plus1_plus1 = 3;
    move_to(&mut state, source, Zone::Graveyard);
    for recipient in [first, first, second] {
        assert!(matches!(next(&mut state), Decision::ChooseTargets { .. }));
        engine::step(&mut state, Action::ChooseTarget(Target::Object(recipient))).unwrap();
    }
    assert!(matches!(next(&mut state), Decision::CastSpellOrPass { .. }));
    assert_eq!(
        state.stack.last().unwrap().targets,
        vec![Target::Object(first), Target::Object(second)]
    );
    let snapshot = serde_json::to_vec(&state).unwrap();
    state = serde_json::from_slice(&snapshot).unwrap();
    move_to(&mut state, source, Zone::Hand);
    move_to(&mut state, first, Zone::Graveyard);
    settled(&mut state);
    assert_eq!(state.objects.get(second).counters.plus1_plus1, 1);
}

#[test]
fn quirion_cannot_finish_a_partly_assigned_distribution() {
    let mut state = ready(Step::Main1);
    let source = put(
        &mut state,
        PlayerId::P0,
        "Quirion Beastcaller",
        Zone::Battlefield,
    );
    let recipient = put(&mut state, PlayerId::P0, "Cenote Scout", Zone::Battlefield);
    state.objects.get_mut(source).counters.plus1_plus1 = 2;
    move_to(&mut state, source, Zone::Graveyard);
    next(&mut state);
    engine::step(&mut state, Action::ChooseTarget(Target::Object(recipient))).unwrap();
    assert!(matches!(
        next(&mut state),
        Decision::ChooseTargets {
            can_finish: false,
            ..
        }
    ));
    let snapshot = serde_json::to_vec(&state).unwrap();
    assert!(engine::step(&mut state, Action::FinishEffectSelection).is_err());
    assert_eq!(serde_json::to_vec(&state).unwrap(), snapshot);
    engine::step(&mut state, Action::ChooseTarget(Target::Object(recipient))).unwrap();
    settled(&mut state);
    assert_eq!(state.objects.get(recipient).counters.plus1_plus1, 2);
}

fn cast_form(state: &mut GameState, spell: ObjectId, form: u8) {
    cast(state, spell, &[]);
    if let Decision::ChooseSpellMode { legal_modes, .. } = next(state) {
        assert!(legal_modes.contains(&form));
        engine::step(state, Action::ChooseSpellMode(form)).unwrap();
    }
    settled(state);
}

#[test]
fn imodane_adventure_makes_vigilant_knights_and_creature_rallies_existing_team() {
    let mut state = ready(Step::Main1);
    let recruiter = put(&mut state, PlayerId::P0, "Imodane's Recruiter", Zone::Hand);
    state.players[0].mana_pool = pool(&[(ManaColor::W, 1)], 4);
    cast_form(&mut state, recruiter, 1);
    assert_eq!(state.objects.get(recruiter).zone, Zone::Exile);
    assert!(state.objects.get(recruiter).v4.on_adventure);
    let knights = battlefield_tokens(&state, PlayerId::P0, "Knight Vigilance Token");
    assert_eq!(knights.len(), 2);
    for &knight in &knights {
        assert_eq!(engine::effective_power(&state, knight), 2);
        assert!(engine::has_effective_keyword(
            &state,
            knight,
            Keywords::VIGILANCE
        ));
    }
    state.players[0].mana_pool = pool(&[(ManaColor::R, 1)], 2);
    cast_form(&mut state, recruiter, 0);
    assert_eq!(state.objects.get(recruiter).zone, Zone::Battlefield);
    for id in [recruiter, knights[0], knights[1]] {
        assert_eq!(engine::effective_power(&state, id), 3);
        assert!(engine::has_effective_keyword(&state, id, Keywords::HASTE));
    }
    let late = put(
        &mut state,
        PlayerId::P0,
        "Knight Vigilance Token",
        Zone::Battlefield,
    );
    assert_eq!(engine::effective_power(&state, late), 2);
    assert!(!engine::has_effective_keyword(
        &state,
        late,
        Keywords::HASTE
    ));
}

#[test]
fn virtue_adventure_and_end_step_counter_then_untap_current_creatures() {
    let mut state = ready(Step::Main1);
    let virtue = put(&mut state, PlayerId::P0, "Virtue of Loyalty", Zone::Hand);
    state.players[0].mana_pool = pool(&[(ManaColor::W, 1)], 1);
    cast_form(&mut state, virtue, 1);
    let knight = battlefield_tokens(&state, PlayerId::P0, "Knight Vigilance Token")[0];
    state.players[0].mana_pool = pool(&[(ManaColor::W, 2)], 3);
    cast_form(&mut state, virtue, 0);
    assert_eq!(state.objects.get(virtue).zone, Zone::Battlefield);
    state.objects.get_mut(knight).tapped = true;
    let stunned = put(&mut state, PlayerId::P0, "Cenote Scout", Zone::Battlefield);
    state.objects.get_mut(stunned).tapped = true;
    state.objects.get_mut(stunned).counters.stun = 2;
    let awake = put(&mut state, PlayerId::P0, "Cenote Scout", Zone::Battlefield);
    state.objects.get_mut(awake).counters.stun = 2;
    let land = put(&mut state, PlayerId::P0, "Forest", Zone::Battlefield);
    state.objects.get_mut(land).tapped = true;
    let theirs = put(&mut state, PlayerId::P1, "Cenote Scout", Zone::Battlefield);
    state.objects.get_mut(theirs).tapped = true;
    state.step = Step::Main2;
    engine::step(&mut state, Action::Pass).unwrap();
    next(&mut state);
    engine::step(&mut state, Action::Pass).unwrap();
    settled(&mut state);
    assert_eq!(state.objects.get(knight).counters.plus1_plus1, 1);
    assert!(!state.objects.get(knight).tapped);
    assert_eq!(state.objects.get(stunned).counters.plus1_plus1, 1);
    assert!(state.objects.get(stunned).tapped);
    assert_eq!(state.objects.get(stunned).counters.stun, 1);
    assert_eq!(state.objects.get(awake).counters.plus1_plus1, 1);
    assert!(!state.objects.get(awake).tapped);
    assert_eq!(state.objects.get(awake).counters.stun, 2);
    assert!(state.objects.get(land).tapped);
    assert!(state.objects.get(theirs).tapped);
    assert_eq!(state.objects.get(virtue).counters.plus1_plus1, 0);
}

fn finish_current_turn(state: &mut GameState) {
    let active = state.active_player;
    state.step = Step::End;
    for _ in 0..30 {
        let decision = next(state);
        if state.active_player != active {
            return;
        }
        match decision {
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            other => panic!("unexpected turn-end decision {other:?}"),
        }
    }
    panic!("turn did not finish");
}

#[test]
fn mosswood_dies_allows_only_adventure_then_ordinary_exile_creature_cast() {
    let mut state = ready(Step::Main1);
    let knight = put(
        &mut state,
        PlayerId::P0,
        "Mosswood Dreadknight",
        Zone::Battlefield,
    );
    move_to(&mut state, knight, Zone::Graveyard);
    settled(&mut state);
    state = serde_json::from_str(&serde_json::to_string(&state).unwrap()).unwrap();
    state.players[0].mana_pool = pool(&[(ManaColor::G, 1)], 1);
    assert!(
        matches!(next(&mut state), Decision::CastSpellOrPass { castable_spells, .. } if !castable_spells.contains(&knight))
    );
    let hand = state.players[0].hand.len();
    let life = state.players[0].life;
    state.players[0].mana_pool = pool(&[(ManaColor::B, 1)], 1);
    cast_form(&mut state, knight, 1);
    assert_eq!(state.objects.get(knight).zone, Zone::Exile);
    assert_eq!(state.players[0].life, life - 1);
    assert_eq!(state.players[0].hand.len(), hand + 1);
    state.players[0].mana_pool = pool(&[(ManaColor::G, 1)], 1);
    cast_form(&mut state, knight, 0);
    assert_eq!(state.objects.get(knight).zone, Zone::Battlefield);
    assert!(engine::has_effective_keyword(
        &state,
        knight,
        Keywords::TRAMPLE
    ));
}

#[test]
fn mosswood_permission_expires_at_end_of_controllers_next_turn_and_never_survives_blink() {
    let mut state = ready(Step::Main1);
    let knight = put(
        &mut state,
        PlayerId::P0,
        "Mosswood Dreadknight",
        Zone::Battlefield,
    );
    move_to(&mut state, knight, Zone::Graveyard);
    settled(&mut state);
    finish_current_turn(&mut state);
    assert!(state
        .objects
        .get(knight)
        .v4
        .creature_upgrade
        .as_ref()
        .unwrap()
        .graveyard_adventure
        .is_some());
    finish_current_turn(&mut state);
    assert_eq!(state.active_player, PlayerId::P0);
    let permission = state
        .objects
        .get(knight)
        .v4
        .creature_upgrade
        .as_ref()
        .unwrap()
        .graveyard_adventure
        .unwrap();
    assert!(permission.holder_turn_started);
    finish_current_turn(&mut state);
    assert!(state.objects.get(knight).v4.creature_upgrade.is_none());

    let mut state = ready(Step::Main1);
    let knight = put(
        &mut state,
        PlayerId::P0,
        "Mosswood Dreadknight",
        Zone::Battlefield,
    );
    move_to(&mut state, knight, Zone::Graveyard);
    next(&mut state);
    move_to(&mut state, knight, Zone::Exile);
    move_to(&mut state, knight, Zone::Graveyard);
    settled(&mut state);
    assert!(state.objects.get(knight).v4.creature_upgrade.is_none());
}

#[test]
fn stolen_mosswood_grants_its_controller_permission_from_owners_graveyard() {
    let mut state = ready(Step::Main1);
    let knight = put(
        &mut state,
        PlayerId::P0,
        "Mosswood Dreadknight",
        Zone::Battlefield,
    );
    state.players[0].battlefield.retain(|id| *id != knight);
    state.players[1].battlefield.push(knight);
    state.objects.get_mut(knight).controller = PlayerId::P1;
    move_to(&mut state, knight, Zone::Graveyard);
    settled(&mut state);
    let permission = state
        .objects
        .get(knight)
        .v4
        .creature_upgrade
        .as_ref()
        .unwrap()
        .graveyard_adventure
        .unwrap();
    assert_eq!(permission.holder, PlayerId::P1);
    state.active_player = PlayerId::P1;
    state.priority_player = PlayerId::P1;
    state.players[1].mana_pool = pool(&[(ManaColor::B, 1)], 1);
    let life = state.players[1].life;
    cast_form(&mut state, knight, 1);
    assert_eq!(state.players[1].life, life - 1);
    assert_eq!(state.objects.get(knight).owner, PlayerId::P0);
    assert!(state.objects.get(knight).v4.on_adventure);
}

#[test]
fn questing_druid_counts_selected_adventure_colors_and_impulse_allows_both_forms() {
    let mut state = ready(Step::Main1);
    let druid = put(
        &mut state,
        PlayerId::P0,
        "Questing Druid",
        Zone::Battlefield,
    );
    let second = put(&mut state, PlayerId::P0, "Questing Druid", Zone::Hand);
    let top = stack_library_top(&mut state, &["Imodane's Recruiter", "Forest"]);
    state.players[0].mana_pool = pool(&[(ManaColor::R, 1)], 1);
    cast_form(&mut state, second, 1);
    assert_eq!(state.objects.get(druid).counters.plus1_plus1, 1);
    assert_eq!(state.objects.get(top[0]).zone, Zone::Exile);
    assert!(state.engine.exile_play_permissions.iter().all(
        |permission| permission.expiry == engine::PlayPermissionExpiry::UntilHoldersNextEndStep
    ));
    // Ordinary impulse exile permits the Adventure too, at its own cost.
    state.players[0].mana_pool = pool(&[(ManaColor::W, 1)], 4);
    cast_form(&mut state, top[0], 1);
    assert_eq!(
        battlefield_tokens(&state, PlayerId::P0, "Knight Vigilance Token").len(),
        2
    );
    assert_eq!(state.objects.get(druid).counters.plus1_plus1, 2);
    state.players[0].mana_pool = pool(&[(ManaColor::G, 1)], 1);
    cast_form(&mut state, second, 0);
    assert_eq!(state.objects.get(druid).counters.plus1_plus1, 2);
}

#[test]
fn seek_the_beast_expires_at_start_of_next_own_end_step_even_same_turn() {
    let mut state = ready(Step::Main1);
    let druid = put(&mut state, PlayerId::P0, "Questing Druid", Zone::Hand);
    state.players[0].mana_pool = pool(&[(ManaColor::R, 1)], 1);
    cast_form(&mut state, druid, 1);
    assert_eq!(state.engine.exile_play_permissions.len(), 2);
    state.step = Step::Main2;
    engine::step(&mut state, Action::Pass).unwrap();
    next(&mut state);
    engine::step(&mut state, Action::Pass).unwrap();
    next(&mut state);
    assert_eq!(state.step, Step::End);
    assert!(state.engine.exile_play_permissions.is_empty());
    // A grant made during the end step itself lasts until the next end step.
    let later = put(&mut state, PlayerId::P0, "Questing Druid", Zone::Hand);
    state.players[0].mana_pool = pool(&[(ManaColor::R, 1)], 1);
    cast_form(&mut state, later, 1);
    finish_current_turn(&mut state);
    assert_eq!(state.engine.exile_play_permissions.len(), 2);
    finish_current_turn(&mut state);
    assert_eq!(state.engine.exile_play_permissions.len(), 2);
    state.step = Step::Main2;
    engine::step(&mut state, Action::Pass).unwrap();
    next(&mut state);
    engine::step(&mut state, Action::Pass).unwrap();
    next(&mut state);
    assert!(state.engine.exile_play_permissions.is_empty());
}

#[test]
fn floodpits_drowner_taps_stuns_then_shuffles_both_owners() {
    let mut state = ready(Step::Main1);
    let target = put(&mut state, PlayerId::P1, "Cenote Scout", Zone::Battlefield);
    let drowner = cast_creature(&mut state, "Floodpits Drowner");
    assert!(matches!(
        settle(&mut state),
        Some(Decision::ChooseTargets { .. })
    ));
    engine::step(&mut state, Action::ChooseTarget(Target::Object(target))).unwrap();
    settled(&mut state);
    assert!(state.objects.get(target).tapped);
    assert_eq!(state.objects.get(target).counters.stun, 1);
    state.objects.get_mut(drowner).summoning_sick = false;
    state.players[0].mana_pool = pool(&[(ManaColor::U, 1)], 1);
    assert!(activatable(&mut state).contains(&(drowner, 0)));
    engine::step(&mut state, Action::ActivateAbility(drowner, 0)).unwrap();
    assert!(
        matches!(next(&mut state), Decision::ChooseTargets { legal_targets, .. } if legal_targets == vec![Target::Object(target)])
    );
    engine::step(&mut state, Action::ChooseTarget(Target::Object(target))).unwrap();
    settled(&mut state);
    assert_eq!(state.objects.get(target).zone, Zone::Library);
    assert_eq!(state.objects.get(drowner).zone, Zone::Library);
    assert!(state.players[0].library.contains(&drowner));
    assert!(state.players[1].library.contains(&target));
}

#[test]
fn floodpits_illegal_target_counters_whole_ability_but_missing_source_does_not() {
    for remove_counter in [false, true] {
        let mut state = ready(Step::Main1);
        let drowner = put(
            &mut state,
            PlayerId::P0,
            "Floodpits Drowner",
            Zone::Battlefield,
        );
        let target = put(&mut state, PlayerId::P1, "Cenote Scout", Zone::Battlefield);
        state.objects.get_mut(target).counters.stun = 1;
        state.players[0].mana_pool = pool(&[(ManaColor::U, 1)], 1);
        activatable(&mut state);
        engine::step(&mut state, Action::ActivateAbility(drowner, 0)).unwrap();
        next(&mut state);
        engine::step(&mut state, Action::ChooseTarget(Target::Object(target))).unwrap();
        next(&mut state);
        if remove_counter {
            state.objects.get_mut(target).counters.stun = 0;
        } else {
            move_to(&mut state, drowner, Zone::Graveyard);
        }
        settled(&mut state);
        assert_eq!(
            state.objects.get(target).zone,
            if remove_counter {
                Zone::Battlefield
            } else {
                Zone::Library
            }
        );
        assert_eq!(
            state.objects.get(drowner).zone,
            if remove_counter {
                Zone::Battlefield
            } else {
                Zone::Graveyard
            }
        );
    }
}

#[test]
fn essence_channeler_life_loss_keywords_reset_and_life_gain_counts_events() {
    let mut state = ready(Step::Main1);
    let essence = put(
        &mut state,
        PlayerId::P0,
        "Essence Channeler",
        Zone::Battlefield,
    );
    assert!(!engine::has_effective_keyword(
        &state,
        essence,
        Keywords::FLYING
    ));
    event::propose_and_commit(&mut state, ProposedEvent::life_loss(PlayerId::P0, 1));
    assert!(engine::has_effective_keyword(
        &state,
        essence,
        Keywords::FLYING
    ));
    assert!(engine::has_effective_keyword(
        &state,
        essence,
        Keywords::VIGILANCE
    ));
    event::propose_and_commit(&mut state, ProposedEvent::life_gain(PlayerId::P0, 5));
    let triggers = trigger::collect_and_process(&mut state);
    state.engine.pending_triggers.extend(triggers);
    settled(&mut state);
    assert_eq!(state.objects.get(essence).counters.plus1_plus1, 1);
    finish_current_turn(&mut state);
    assert!(!engine::has_effective_keyword(
        &state,
        essence,
        Keywords::FLYING
    ));
}

#[test]
fn essence_transfers_all_counter_families_from_departed_incarnation() {
    let mut state = ready(Step::Main1);
    let essence = put(
        &mut state,
        PlayerId::P0,
        "Essence Channeler",
        Zone::Battlefield,
    );
    let recipient = put(
        &mut state,
        PlayerId::P0,
        "Troll of Khazad-dum",
        Zone::Battlefield,
    );
    {
        let source = state.objects.get_mut(essence);
        source.counters.plus1_plus1 = 3;
        source.counters.stun = 2;
        source.counters.oil = 4;
        source.counters.lore = 1;
        source.v4.lifelink_keyword_counters = 1;
        source.v4.time_counters_v1 = 2;
    }
    move_to(&mut state, essence, Zone::Graveyard);
    assert!(matches!(next(&mut state), Decision::ChooseTargets { .. }));
    engine::step(&mut state, Action::ChooseTarget(Target::Object(recipient))).unwrap();
    next(&mut state);
    move_to(&mut state, essence, Zone::Exile);
    state = serde_json::from_str(&serde_json::to_string(&state).unwrap()).unwrap();
    settled(&mut state);
    let recipient = state.objects.get(recipient);
    assert_eq!(recipient.counters.plus1_plus1, 3);
    assert_eq!(recipient.counters.stun, 2);
    assert_eq!(recipient.counters.oil, 4);
    assert_eq!(recipient.counters.lore, 1);
    assert_eq!(recipient.v4.lifelink_keyword_counters, 1);
    assert_eq!(recipient.v4.time_counters_v1, 2);
}

#[test]
fn brightglass_search_is_optional_and_filters_two_revealed_cheap_permanents() {
    let mut state = ready(Step::Main1);
    let top = stack_library_top(
        &mut state,
        &[
            "Novice Inspector",
            "Gingerbrute",
            "Lightning Bolt",
            "Forest",
            "Mosswood Dreadknight",
        ],
    );
    cast_creature(&mut state, "Brightglass Gearhulk");
    assert!(matches!(
        settle(&mut state),
        Some(Decision::ChooseEffectOption { .. })
    ));
    engine::step(&mut state, Action::ChooseEffectOption(1)).unwrap();
    assert!(
        matches!(settle(&mut state), Some(Decision::ChooseEffectTargets { legal_targets, .. }) if legal_targets == vec![Target::Object(top[0]), Target::Object(top[1])])
    );
    engine::step(
        &mut state,
        Action::ChooseEffectTarget(Target::Object(top[0])),
    )
    .unwrap();
    next(&mut state);
    engine::step(
        &mut state,
        Action::ChooseEffectTarget(Target::Object(top[1])),
    )
    .unwrap();
    settled(&mut state);
    assert_eq!(state.objects.get(top[0]).zone, Zone::Hand);
    assert_eq!(state.objects.get(top[1]).zone, Zone::Hand);
    assert_eq!(state.objects.get(top[2]).zone, Zone::Library);
    assert_eq!(state.objects.get(top[3]).zone, Zone::Library);
    assert_eq!(state.objects.get(top[4]).zone, Zone::Library);

    let library = state.players[0].library.clone();
    cast_creature(&mut state, "Brightglass Gearhulk");
    assert!(matches!(
        settle(&mut state),
        Some(Decision::ChooseEffectOption { .. })
    ));
    engine::step(&mut state, Action::ChooseEffectOption(0)).unwrap();
    settled(&mut state);
    assert_eq!(state.players[0].library, library);
}

#[test]
fn tidebinder_counters_exact_ability_and_suppresses_source_only_while_it_remains() {
    for leave_before_resolution in [false, true] {
        let mut state = ready(Step::Main1);
        let ginger = put(&mut state, PlayerId::P0, "Gingerbrute", Zone::Battlefield);
        state.players[0].mana_pool = pool(&[], 1);
        activatable(&mut state);
        engine::step(&mut state, Action::ActivateAbility(ginger, 0)).unwrap();
        next(&mut state);
        let ability = state.stack.last().unwrap().v4.stack_item_id;
        let tidebinder = cast_creature(&mut state, "Tishana's Tidebinder");
        assert!(
            matches!(settle(&mut state), Some(Decision::ChooseTargets { legal_targets, can_finish: true, .. }) if legal_targets.contains(&Target::StackItem(ability)))
        );
        engine::step(&mut state, Action::ChooseTarget(Target::StackItem(ability))).unwrap();
        next(&mut state);
        if leave_before_resolution {
            move_to(&mut state, tidebinder, Zone::Hand);
        }
        state = serde_json::from_str(&serde_json::to_string(&state).unwrap()).unwrap();
        settled(&mut state);
        assert!(!state
            .stack
            .iter()
            .any(|item| item.v4.stack_item_id == ability));
        assert_eq!(
            engine::has_effective_keyword(&state, ginger, Keywords::HASTE),
            leave_before_resolution
        );
        assert!(!state
            .objects
            .get(ginger)
            .v4
            .creature_upgrade
            .as_ref()
            .is_some_and(|upgrade| upgrade.haste_blockers_only));
        if !leave_before_resolution {
            state.players[0].mana_pool = pool(&[], 1);
            assert!(!activatable(&mut state).contains(&(ginger, 0)));
            move_to(&mut state, tidebinder, Zone::Hand);
            assert!(engine::has_effective_keyword(
                &state,
                ginger,
                Keywords::HASTE
            ));
        }
    }
}

#[test]
fn tidebinder_may_target_nothing() {
    let mut state = ready(Step::Main1);
    let tidebinder = cast_creature(&mut state, "Tishana's Tidebinder");
    assert!(
        matches!(settle(&mut state), Some(Decision::ChooseTargets { legal_targets, can_finish: true, .. }) if legal_targets.is_empty())
    );
    engine::step(&mut state, Action::FinishEffectSelection).unwrap();
    settled(&mut state);
    assert_eq!(state.objects.get(tidebinder).zone, Zone::Battlefield);
}

fn choose_creature_option(state: &mut GameState, answer: u8) {
    let pending = state.engine.pending_effect.as_ref().unwrap();
    let Some(mtg_kernel::effect::PendingEffectChoice::ChooseOption { options, .. }) =
        &pending.choice
    else {
        panic!("expected creature choice")
    };
    let index = options.iter().position(|op| matches!(op, mtg_kernel::effect::EffectOp::CreatureChoiceAnswerV1 {answer: candidate, ..} if *candidate == answer)).unwrap();
    engine::step(state, Action::ChooseEffectOption(index as u16)).unwrap();
}

#[test]
fn glissa_announces_modes_and_removes_only_three_chosen_counter_kinds() {
    use mtg_kernel::standard_creature_choices_v1::CounterKindV1;
    let mut state = ready(Step::Main1);
    let glissa = put(
        &mut state,
        PlayerId::P0,
        "Glissa Sunslayer",
        Zone::Battlefield,
    );
    let target = put(&mut state, PlayerId::P1, "Gingerbrute", Zone::Battlefield);
    {
        let object = state.objects.get_mut(target);
        object.counters.plus1_plus1 = 2;
        object.counters.stun = 1;
        object.counters.charge = 2;
        object
            .v4
            .creature_upgrade
            .get_or_insert_with(Default::default)
            .finality = 1;
    }
    combat_hit(&mut state, glissa);
    assert!(matches!(
        settle(&mut state),
        Some(Decision::ChooseTriggerMode { .. })
    ));
    engine::step(&mut state, Action::ChooseTriggerMode(2)).unwrap();
    assert!(matches!(next(&mut state), Decision::ChooseTargets { .. }));
    engine::step(&mut state, Action::ChooseTarget(Target::Object(target))).unwrap();
    for counter in [
        CounterKindV1::Stun,
        CounterKindV1::Finality,
        CounterKindV1::Charge,
    ] {
        assert!(matches!(
            settle(&mut state),
            Some(Decision::ChooseEffectOption { .. })
        ));
        let answer = CounterKindV1::ALL
            .iter()
            .position(|c| *c == counter)
            .unwrap() as u8
            + 1;
        state = serde_json::from_str(&serde_json::to_string(&state).unwrap()).unwrap();
        choose_creature_option(&mut state, answer);
        // The answered guard also survives restoration before execution.
        state = serde_json::from_str(&serde_json::to_string(&state).unwrap()).unwrap();
    }
    settled(&mut state);
    let object = state.objects.get(target);
    assert_eq!(object.counters.plus1_plus1, 2);
    assert_eq!(object.counters.charge, 1);
    assert_eq!(object.counters.stun, 0);
    assert_eq!(object.v4.creature_upgrade.as_ref().unwrap().finality, 0);
    move_to(&mut state, target, Zone::Graveyard);
    assert_eq!(state.objects.get(target).zone, Zone::Graveyard);
}

#[test]
fn glissa_draw_mode_loses_life_and_counter_mode_can_stop_early() {
    let mut state = ready(Step::Main1);
    let glissa = put(
        &mut state,
        PlayerId::P0,
        "Glissa Sunslayer",
        Zone::Battlefield,
    );
    combat_hit(&mut state, glissa);
    assert!(matches!(
        settle(&mut state),
        Some(Decision::ChooseTriggerMode { .. })
    ));
    engine::step(&mut state, Action::ChooseTriggerMode(0)).unwrap();
    settled(&mut state);
    assert_eq!(state.players[0].life, 19);
    assert_eq!(state.players[0].hand.len(), 1);
    state.objects.get_mut(glissa).counters.oil = 5;
    combat_hit(&mut state, glissa);
    assert!(matches!(
        settle(&mut state),
        Some(Decision::ChooseTriggerMode { .. })
    ));
    engine::step(&mut state, Action::ChooseTriggerMode(2)).unwrap();
    next(&mut state);
    engine::step(&mut state, Action::ChooseTarget(Target::Object(glissa))).unwrap();
    assert!(matches!(
        settle(&mut state),
        Some(Decision::ChooseEffectOption { .. })
    ));
    choose_creature_option(&mut state, 0);
    settled(&mut state);
    assert_eq!(state.objects.get(glissa).counters.oil, 5);
}

#[test]
fn frillback_payment_creates_separate_modal_trigger_and_preserves_legal_target() {
    let mut state = ready(Step::Main1);
    let artifact = put(&mut state, PlayerId::P1, "Gingerbrute", Zone::Battlefield);
    let dead = put(
        &mut state,
        PlayerId::P1,
        "Mosswood Dreadknight",
        Zone::Graveyard,
    );
    let source = cast_creature(&mut state, "Tranquil Frillback");
    next(&mut state);
    state.players[0].mana_pool = pool(&[(ManaColor::G, 3)], 0);
    assert!(matches!(
        settle(&mut state),
        Some(Decision::ChooseEffectOption {
            option_count: 4,
            ..
        })
    ));
    assert_eq!(state.stack.len(), 1);
    choose_creature_option(&mut state, 3);
    state = serde_json::from_str(&serde_json::to_string(&state).unwrap()).unwrap();
    assert!(matches!(
        next(&mut state),
        Decision::ChooseTriggerMode { .. }
    ));
    // All three modes are the eighth subset. The payment trigger has finished.
    assert!(state.stack.is_empty());
    engine::step(&mut state, Action::ChooseTriggerMode(7)).unwrap();
    next(&mut state);
    engine::step(&mut state, Action::ChooseTarget(Target::Object(artifact))).unwrap();
    next(&mut state);
    engine::step(
        &mut state,
        Action::ChooseTarget(Target::Player(PlayerId::P1)),
    )
    .unwrap();
    assert!(matches!(next(&mut state), Decision::CastSpellOrPass { .. }));
    assert_eq!(state.stack.len(), 1);
    assert_eq!(state.players[0].mana_pool, [0; 6]);
    // Both the source and one target may leave during the new response window.
    move_to(&mut state, source, Zone::Hand);
    move_to(&mut state, artifact, Zone::Hand);
    settled(&mut state);
    assert_eq!(state.objects.get(artifact).zone, Zone::Hand);
    assert_eq!(state.objects.get(dead).zone, Zone::Exile);
    assert_eq!(state.players[0].life, 24);
}

#[test]
fn frillback_one_payment_limits_modes_and_decline_creates_no_reflexive_trigger() {
    for paid in [0, 1] {
        let mut state = ready(Step::Main1);
        cast_creature(&mut state, "Tranquil Frillback");
        next(&mut state);
        state.players[0].mana_pool = pool(&[(ManaColor::G, 1)], 0);
        assert!(matches!(
            settle(&mut state),
            Some(Decision::ChooseEffectOption {
                option_count: 2,
                ..
            })
        ));
        choose_creature_option(&mut state, paid);
        if paid == 1 {
            let Decision::ChooseTriggerMode { .. } = next(&mut state) else {
                panic!("expected reflexive modes")
            };
            // No artifact exists: mode1 is unavailable, but subset indices stay stable.
            assert!(engine::step(&mut state, Action::ChooseTriggerMode(4)).is_err());
            engine::step(&mut state, Action::ChooseTriggerMode(3)).unwrap();
        }
        settled(&mut state);
        assert_eq!(state.players[0].life, if paid == 1 { 24 } else { 20 });
        assert_eq!(
            state.players[0].mana_pool[ManaColor::G.pool_index()],
            1 - paid
        );
    }
}

#[test]
fn zoraline_payment_targeting_and_finality_survive_source_departure() {
    let mut state = ready(Step::Main1);
    let dead = put(
        &mut state,
        PlayerId::P0,
        "Mosswood Dreadknight",
        Zone::Graveyard,
    );
    let source = cast_creature(&mut state, "Zoraline, Cosmos Caller");
    next(&mut state);
    state.players[0].mana_pool = pool(&[(ManaColor::W, 1), (ManaColor::B, 1)], 0);
    assert!(matches!(
        settle(&mut state),
        Some(Decision::ChooseEffectOption {
            option_count: 2,
            ..
        })
    ));
    choose_creature_option(&mut state, 1);
    assert!(matches!(next(&mut state), Decision::ChooseTargets { .. }));
    assert_eq!(state.players[0].life, 18);
    assert!(state.stack.is_empty());
    engine::step(&mut state, Action::ChooseTarget(Target::Object(dead))).unwrap();
    assert!(matches!(next(&mut state), Decision::CastSpellOrPass { .. }));
    move_to(&mut state, source, Zone::Hand);
    state = serde_json::from_str(&serde_json::to_string(&state).unwrap()).unwrap();
    settled(&mut state);
    assert_eq!(state.objects.get(dead).zone, Zone::Battlefield);
    assert_eq!(
        state
            .objects
            .get(dead)
            .v4
            .creature_upgrade
            .as_ref()
            .unwrap()
            .finality,
        1
    );
    move_to(&mut state, dead, Zone::Graveyard);
    assert_eq!(state.objects.get(dead).zone, Zone::Exile);
    assert!(
        state.engine.pending_triggers.is_empty(),
        "finality prevents the dies event"
    );
}

#[test]
fn zoraline_triggers_once_per_attacking_bat_and_cannot_pay_unavailable_life() {
    let mut state = ready(Step::DeclareAttackers);
    let zoraline = put(
        &mut state,
        PlayerId::P0,
        "Zoraline, Cosmos Caller",
        Zone::Battlefield,
    );
    let bat = put(
        &mut state,
        PlayerId::P0,
        "Deep-Cavern Bat",
        Zone::Battlefield,
    );
    assert!(matches!(
        next(&mut state),
        Decision::DeclareAttackers { .. }
    ));
    engine::step(&mut state, Action::DeclareAttackers(vec![zoraline, bat])).unwrap();
    assert_eq!(
        state.engine.pending_triggers.len(),
        3,
        "two Bat gains and Zoraline's optional attack payment"
    );
    settled(&mut state);
    assert_eq!(state.players[0].life, 22);

    let mut state = ready(Step::Main1);
    state.players[0].life = 1;
    cast_creature(&mut state, "Zoraline, Cosmos Caller");
    next(&mut state);
    state.players[0].mana_pool = pool(&[(ManaColor::W, 1), (ManaColor::B, 1)], 0);
    settled(&mut state);
    assert_eq!(state.players[0].life, 1);
    assert_eq!(
        state.players[0].mana_pool,
        pool(&[(ManaColor::W, 1), (ManaColor::B, 1)], 0)
    );
}

#[test]
fn finality_applies_to_lethal_state_based_actions_without_dies_triggers() {
    let mut state = ready(Step::Main1);
    let creature = put(
        &mut state,
        PlayerId::P0,
        "Mosswood Dreadknight",
        Zone::Battlefield,
    );
    let live = state.objects.get_mut(creature);
    live.v4
        .creature_upgrade
        .get_or_insert_with(Default::default)
        .finality = 1;
    live.damage = 3;
    let triggers = trigger::collect_and_process(&mut state);
    assert_eq!(state.objects.get(creature).zone, Zone::Exile);
    assert!(triggers.is_empty());
}
