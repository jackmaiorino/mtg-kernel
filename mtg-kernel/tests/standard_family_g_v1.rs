//! MageZero Standard family G: creatures with triggered and static
//! abilities, built for the five mono-color decks first.
#![cfg(feature = "standard-magezero-fixtures")]
use mtg_kernel::card_def::{card_id_by_name, CardCapability, Keywords, Subtype, CARD_DEFS};
use mtg_kernel::effect::EffectBooleanChoicePurpose;
use mtg_kernel::engine::{self, Action, Decision};
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
    // Three counters, each placed on one of the two creatures P0 controls.
    for option in [1, 1, 0] {
        match settle(&mut state) {
            Some(Decision::ChooseEffectOption {
                player,
                option_count,
                ..
            }) => {
                assert_eq!((player, option_count), (PlayerId::P0, 2));
                engine::step(&mut state, Action::ChooseEffectOption(option)).unwrap();
            }
            other => panic!("expected a counter placement, got {other:?}"),
        }
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
fn quirion_beastcaller_with_one_other_creature_needs_no_choice() {
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
    settled(&mut state);
    assert_eq!(state.objects.get(other).counters.plus1_plus1, 2);
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
