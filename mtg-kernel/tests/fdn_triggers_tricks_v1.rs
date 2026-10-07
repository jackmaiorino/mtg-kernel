//! FDN batch 3: enter/dies/cast/landfall trigger creatures and three
//! instants, all built from existing engine primitives.
#![cfg(feature = "limited-fdn-fixtures")]
use mtg_kernel::card_def::{
    card_id_by_name, CardCapability, CardType, Keywords, Subtype, CARD_DEFS,
};
use mtg_kernel::engine::{self, Action, Decision};
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::limited_session_v1::CustomDeckV1;
use mtg_kernel::mana::ManaColor;
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Target, Zone};
use mtg_kernel::trigger;

const FIRST_ID: usize = 254;

/// Name, mana cost as (colored pips, generic), colors, types, subtypes,
/// power/toughness, keywords and trigger count, in id order.
#[allow(clippy::type_complexity)]
const CARDS: [(
    &str,
    &[ManaColor],
    u8,
    &[ManaColor],
    CardType,
    &[Subtype],
    Option<(i16, i16)>,
    Keywords,
    usize,
); 14] = [
    (
        "Helpful Hunter",
        &[ManaColor::W],
        1,
        &[ManaColor::W],
        CardType::Creature,
        &[Subtype::Cat],
        Some((1, 1)),
        Keywords::NONE,
        1,
    ),
    (
        "Prideful Parent",
        &[ManaColor::W],
        2,
        &[ManaColor::W],
        CardType::Creature,
        &[Subtype::Cat],
        Some((2, 2)),
        Keywords::VIGILANCE,
        1,
    ),
    (
        "Icewind Elemental",
        &[ManaColor::U],
        4,
        &[ManaColor::U],
        CardType::Creature,
        &[Subtype::Elemental],
        Some((3, 4)),
        Keywords::FLYING,
        1,
    ),
    (
        "Burglar Rat",
        &[ManaColor::B],
        1,
        &[ManaColor::B],
        CardType::Creature,
        &[Subtype::Rat],
        Some((1, 1)),
        Keywords::NONE,
        1,
    ),
    (
        "Diregraf Ghoul",
        &[ManaColor::B],
        0,
        &[ManaColor::B],
        CardType::Creature,
        &[Subtype::Zombie],
        Some((2, 2)),
        Keywords::NONE,
        0,
    ),
    (
        "Infestation Sage",
        &[ManaColor::B],
        0,
        &[ManaColor::B],
        CardType::Creature,
        &[Subtype::Elf, Subtype::Warlock],
        Some((1, 1)),
        Keywords::NONE,
        1,
    ),
    (
        "Wary Thespian",
        &[ManaColor::G],
        1,
        &[ManaColor::G],
        CardType::Creature,
        &[Subtype::Cat, Subtype::Druid],
        Some((3, 1)),
        Keywords::NONE,
        2,
    ),
    (
        "Firebrand Archer",
        &[ManaColor::R],
        1,
        &[ManaColor::R],
        CardType::Creature,
        &[Subtype::Human, Subtype::Archer],
        Some((2, 1)),
        Keywords::NONE,
        1,
    ),
    (
        "Spitfire Lagac",
        &[ManaColor::R],
        3,
        &[ManaColor::R],
        CardType::Creature,
        &[Subtype::Lizard],
        Some((3, 4)),
        Keywords::NONE,
        1,
    ),
    (
        "Giant Growth",
        &[ManaColor::G],
        0,
        &[ManaColor::G],
        CardType::Instant,
        &[],
        None,
        Keywords::NONE,
        0,
    ),
    (
        "Stab",
        &[ManaColor::B],
        0,
        &[ManaColor::B],
        CardType::Instant,
        &[],
        None,
        Keywords::NONE,
        0,
    ),
    (
        "Think Twice",
        &[ManaColor::U],
        1,
        &[ManaColor::U],
        CardType::Instant,
        &[],
        None,
        Keywords::NONE,
        0,
    ),
    (
        "Cat Token",
        &[],
        0,
        &[ManaColor::W],
        CardType::Creature,
        &[Subtype::Cat],
        Some((1, 1)),
        Keywords::NONE,
        0,
    ),
    (
        "Insect Token",
        &[],
        0,
        &[ManaColor::B, ManaColor::G],
        CardType::Creature,
        &[Subtype::Insect],
        Some((1, 1)),
        Keywords::FLYING,
        0,
    ),
];

fn ready(step: Step) -> GameState {
    let forest = card_id_by_name("Forest").unwrap();
    let mut state =
        GameState::new_from_libraries(&[forest; 40], &[forest; 40], |_| "Forest".into(), 123);
    state.step = step;
    state
}

fn put(state: &mut GameState, player: PlayerId, name: &str, zone: Zone) -> ObjectId {
    let card_def = card_id_by_name(name).unwrap();
    let id = state.objects.push(GameObject {
        card_def,
        name: name.into(),
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
    match zone {
        Zone::Hand => state.players[player.index()].hand.push(id),
        Zone::Battlefield => state.players[player.index()].battlefield.push(id),
        _ => panic!("helper zone"),
    }
    id
}

/// Moves an object between zones outside the stack and queues its triggers.
fn move_to(state: &mut GameState, id: ObjectId, zone: Zone) {
    event::propose_and_commit(state, ProposedEvent::zone_change(id, zone));
    let triggers = trigger::collect_and_process(state);
    state.engine.pending_triggers.extend(triggers);
}

fn mana(cost: &[(ManaColor, u8)], generic: u8) -> [u8; 6] {
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
    for _ in 0..100 {
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

fn cast(state: &mut GameState, spell: ObjectId, target: Option<ObjectId>) {
    assert!(
        matches!(next(state), Decision::CastSpellOrPass { castable_spells, .. } if castable_spells.contains(&spell)),
        "{} not castable",
        state.objects.get(spell).name
    );
    engine::step(state, Action::CastSpell(spell)).unwrap();
    if let Some(target) = target {
        assert!(
            matches!(next(state), Decision::ChooseTargets { legal_targets, .. } if legal_targets.contains(&Target::Object(target)))
        );
        engine::step(state, Action::ChooseTarget(Target::Object(target))).unwrap();
    }
    assert!(matches!(next(state), Decision::CastSpellOrPass { .. }));
    assert!(state.engine.pending_cast.is_none());
    assert_eq!(state.players[0].mana_pool, [0; 6]);
}

fn life(state: &GameState) -> (i32, i32) {
    (state.players[0].life, state.players[1].life)
}

#[test]
fn appended_definitions_match_their_printed_characteristics() {
    for (offset, (name, pips, generic, colors, card_type, subtypes, stats, keywords, triggers)) in
        CARDS.into_iter().enumerate()
    {
        let id = card_id_by_name(name).unwrap() as usize;
        assert_eq!(id, FIRST_ID + offset, "{name}");
        let def = &CARD_DEFS[id];
        assert_eq!(def.capability, CardCapability::Full, "{name}");
        assert_eq!(def.cost.generic, generic, "{name}");
        assert_eq!(def.cost.pips.len(), pips.len(), "{name}");
        assert_eq!(
            def.mana_value,
            u16::from(generic) + pips.len() as u16,
            "{name}"
        );
        assert_eq!(def.colors, colors, "{name}");
        assert_eq!(def.types, &[card_type], "{name}");
        assert_eq!(def.subtypes, subtypes, "{name}");
        assert_eq!(
            (def.power, def.toughness),
            (stats.map(|s| s.0), stats.map(|s| s.1)),
            "{name}"
        );
        assert_eq!(def.keywords, keywords, "{name}");
        assert_eq!(trigger::triggers_for(id as u16).len(), triggers, "{name}");
    }
    assert!(CARD_DEFS[card_id_by_name("Think Twice").unwrap() as usize]
        .flashback
        .is_some());
    for subtype in [Subtype::Archer, Subtype::Lizard, Subtype::Insect] {
        assert!(Subtype::CREATURE_TYPES.contains(&subtype));
        assert!(subtype.is_creature_type());
    }
}

#[test]
fn reference_deck_resolves_all_forty_copies() {
    let deck: CustomDeckV1 = serde_json::from_str(
        r#"{"cards":[{"name":"Helpful Hunter","count":1},{"name":"Prideful Parent","count":1},
            {"name":"Icewind Elemental","count":1},{"name":"Burglar Rat","count":1},
            {"name":"Diregraf Ghoul","count":1},{"name":"Infestation Sage","count":1},
            {"name":"Wary Thespian","count":1},{"name":"Firebrand Archer","count":1},
            {"name":"Spitfire Lagac","count":1},{"name":"Giant Growth","count":1},
            {"name":"Stab","count":1},{"name":"Think Twice","count":1},
            {"name":"Swamp","count":28}]}"#,
    )
    .unwrap();
    let ids = deck.resolve().unwrap();
    assert_eq!(ids.len(), 40);
    assert_eq!(&ids[..12], &(254..266).collect::<Vec<u16>>()[..]);
}

#[test]
fn creatures_cast_for_their_exact_costs() {
    for (name, pips, generic, _, card_type, ..) in CARDS {
        if card_type != CardType::Creature || name.ends_with("Token") {
            continue;
        }
        let pool = mana(&[(pips[0], 1)], generic);
        let mut state = ready(Step::Main1);
        let creature = put(&mut state, PlayerId::P0, name, Zone::Hand);
        let mut short = pool;
        short[if generic > 0 { 5 } else { pips[0].pool_index() }] -= 1;
        state.players[0].mana_pool = short;
        assert!(
            engine::step(&mut state, Action::CastSpell(creature)).is_err(),
            "{name}"
        );
        state.players[0].mana_pool = pool;
        cast(&mut state, creature, None);
    }
}

#[test]
fn helpful_hunter_draws_one_on_entering() {
    let mut state = ready(Step::Main1);
    let hunter = put(&mut state, PlayerId::P0, "Helpful Hunter", Zone::Hand);
    state.players[0].mana_pool = mana(&[(ManaColor::W, 1)], 1);
    let library = state.players[0].library.len();
    cast(&mut state, hunter, None);
    settled(&mut state);
    assert_eq!(state.objects.get(hunter).zone, Zone::Battlefield);
    assert_eq!(state.players[0].hand.len(), 1);
    assert_eq!(state.players[0].library.len(), library - 1);
}

#[test]
fn prideful_parent_creates_a_white_one_one_cat() {
    let mut state = ready(Step::Main1);
    let parent = put(&mut state, PlayerId::P0, "Prideful Parent", Zone::Hand);
    state.players[0].mana_pool = mana(&[(ManaColor::W, 1)], 2);
    cast(&mut state, parent, None);
    settled(&mut state);
    let battlefield = &state.players[0].battlefield;
    assert_eq!(battlefield.len(), 2);
    let token = *battlefield.iter().find(|&&id| id != parent).unwrap();
    let token = state.objects.get(token);
    assert_eq!(token.card_def, card_id_by_name("Cat Token").unwrap());
    assert_eq!(token.controller, PlayerId::P0);
}

#[test]
fn icewind_elemental_draws_then_discards() {
    let mut state = ready(Step::Main1);
    let kept = put(&mut state, PlayerId::P0, "Stab", Zone::Hand);
    let elemental = put(&mut state, PlayerId::P0, "Icewind Elemental", Zone::Hand);
    state.players[0].mana_pool = mana(&[(ManaColor::U, 1)], 4);
    cast(&mut state, elemental, None);
    let Some(Decision::Discard {
        player,
        count,
        choices,
    }) = settle(&mut state)
    else {
        panic!("expected discard");
    };
    assert_eq!((player, count), (PlayerId::P0, 1));
    assert_eq!(choices.len(), 2);
    let drawn = *choices.iter().find(|&&id| id != kept).unwrap();
    engine::step(&mut state, Action::Discard(vec![drawn])).unwrap();
    settled(&mut state);
    assert_eq!(state.players[0].hand, vec![kept]);
    assert_eq!(state.players[0].graveyard, vec![drawn]);
}

#[test]
fn burglar_rat_makes_the_opponent_discard_and_tolerates_an_empty_hand() {
    let mut state = ready(Step::Main1);
    let a = put(&mut state, PlayerId::P1, "Stab", Zone::Hand);
    let b = put(&mut state, PlayerId::P1, "Giant Growth", Zone::Hand);
    let rat = put(&mut state, PlayerId::P0, "Burglar Rat", Zone::Hand);
    state.players[0].mana_pool = mana(&[(ManaColor::B, 1)], 1);
    cast(&mut state, rat, None);
    let Some(Decision::Discard {
        player,
        count,
        choices,
    }) = settle(&mut state)
    else {
        panic!("expected opponent discard");
    };
    assert_eq!((player, count), (PlayerId::P1, 1));
    assert_eq!(choices, vec![a, b]);
    engine::step(&mut state, Action::Discard(vec![b])).unwrap();
    settled(&mut state);
    assert_eq!(state.players[1].hand, vec![a]);
    assert_eq!(state.players[1].graveyard, vec![b]);

    let mut state = ready(Step::Main1);
    let rat = put(&mut state, PlayerId::P0, "Burglar Rat", Zone::Hand);
    state.players[0].mana_pool = mana(&[(ManaColor::B, 1)], 1);
    cast(&mut state, rat, None);
    settled(&mut state);
    assert!(state.players[1].graveyard.is_empty());
    assert_eq!(state.players[0].hand.len(), 0);
}

#[test]
fn diregraf_ghoul_enters_tapped() {
    let mut state = ready(Step::Main1);
    let ghoul = put(&mut state, PlayerId::P0, "Diregraf Ghoul", Zone::Hand);
    state.players[0].mana_pool = mana(&[(ManaColor::B, 1)], 0);
    cast(&mut state, ghoul, None);
    settled(&mut state);
    assert_eq!(state.objects.get(ghoul).zone, Zone::Battlefield);
    assert!(state.objects.get(ghoul).tapped);
}

#[test]
fn infestation_sage_leaves_a_flying_insect_when_it_dies() {
    let mut state = ready(Step::Main1);
    let sage = put(
        &mut state,
        PlayerId::P0,
        "Infestation Sage",
        Zone::Battlefield,
    );
    move_to(&mut state, sage, Zone::Graveyard);
    settled(&mut state);
    let [token] = state.players[0].battlefield[..] else {
        panic!("expected one token");
    };
    let token = state.objects.get(token);
    assert_eq!(token.card_def, card_id_by_name("Insect Token").unwrap());
    assert_eq!(token.controller, PlayerId::P0);

    // Leaving for exile is not dying.
    let mut state = ready(Step::Main1);
    let sage = put(
        &mut state,
        PlayerId::P0,
        "Infestation Sage",
        Zone::Battlefield,
    );
    move_to(&mut state, sage, Zone::Exile);
    settled(&mut state);
    assert!(state.players[0].battlefield.is_empty());
}

#[test]
fn wary_thespian_surveils_on_entering_and_on_dying() {
    let mut state = ready(Step::Main1);
    let thespian = put(&mut state, PlayerId::P0, "Wary Thespian", Zone::Hand);
    state.players[0].mana_pool = mana(&[(ManaColor::G, 1)], 1);
    cast(&mut state, thespian, None);
    let top = state.players[0].library[0];
    // Surveil 1: option 0 keeps the card on top, option 1 mills it.
    let Some(Decision::ChooseEffectOption {
        player,
        source,
        option_count,
        ..
    }) = settle(&mut state)
    else {
        panic!("expected enter surveil");
    };
    assert_eq!((player, source, option_count), (PlayerId::P0, thespian, 2));
    engine::step(&mut state, Action::ChooseEffectOption(1)).unwrap();
    settled(&mut state);
    assert_eq!(state.players[0].graveyard, vec![top]);

    move_to(&mut state, thespian, Zone::Graveyard);
    let kept = state.players[0].library[0];
    let Some(Decision::ChooseEffectOption { option_count, .. }) = settle(&mut state) else {
        panic!("expected dies surveil");
    };
    assert_eq!(option_count, 2);
    engine::step(&mut state, Action::ChooseEffectOption(0)).unwrap();
    settled(&mut state);
    assert_eq!(state.players[0].library[0], kept);
    assert_eq!(state.players[0].graveyard, vec![top, thespian]);
}

#[test]
fn firebrand_archer_pings_for_noncreature_spells_only() {
    let mut state = ready(Step::Main1);
    let archer = put(
        &mut state,
        PlayerId::P0,
        "Firebrand Archer",
        Zone::Battlefield,
    );
    let growth = put(&mut state, PlayerId::P0, "Giant Growth", Zone::Hand);
    state.players[0].mana_pool = mana(&[(ManaColor::G, 1)], 0);
    cast(&mut state, growth, Some(archer));
    settled(&mut state);
    assert_eq!(life(&state), (20, 19));
    assert_eq!(engine::effective_power(&state, archer), 5);
    assert_eq!(engine::effective_toughness(&state, archer), 4);

    let hunter = put(&mut state, PlayerId::P0, "Helpful Hunter", Zone::Hand);
    state.players[0].mana_pool = mana(&[(ManaColor::W, 1)], 1);
    cast(&mut state, hunter, None);
    settled(&mut state);
    assert_eq!(life(&state), (20, 19));
}

#[test]
fn spitfire_lagac_pings_when_its_controllers_land_enters() {
    let mut state = ready(Step::Main1);
    put(
        &mut state,
        PlayerId::P0,
        "Spitfire Lagac",
        Zone::Battlefield,
    );
    let land = put(&mut state, PlayerId::P0, "Swamp", Zone::Hand);
    move_to(&mut state, land, Zone::Battlefield);
    settled(&mut state);
    assert_eq!(life(&state), (20, 19));
    let land = put(&mut state, PlayerId::P1, "Swamp", Zone::Hand);
    move_to(&mut state, land, Zone::Battlefield);
    settled(&mut state);
    assert_eq!(life(&state), (20, 19));
}

#[test]
fn stab_kills_a_two_toughness_creature() {
    let mut state = ready(Step::Main1);
    let ghoul = put(
        &mut state,
        PlayerId::P1,
        "Diregraf Ghoul",
        Zone::Battlefield,
    );
    let lagac = put(
        &mut state,
        PlayerId::P1,
        "Spitfire Lagac",
        Zone::Battlefield,
    );
    let stab = put(&mut state, PlayerId::P0, "Stab", Zone::Hand);
    state.players[0].mana_pool = mana(&[(ManaColor::B, 1)], 0);
    cast(&mut state, stab, Some(ghoul));
    settled(&mut state);
    assert_eq!(state.objects.get(ghoul).zone, Zone::Graveyard);
    assert_eq!(state.objects.get(stab).zone, Zone::Graveyard);

    let stab = put(&mut state, PlayerId::P0, "Stab", Zone::Hand);
    state.players[0].mana_pool = mana(&[(ManaColor::B, 1)], 0);
    cast(&mut state, stab, Some(lagac));
    settled(&mut state);
    assert_eq!(state.objects.get(lagac).zone, Zone::Battlefield);
    assert_eq!(engine::effective_power(&state, lagac), 1);
    assert_eq!(engine::effective_toughness(&state, lagac), 2);
}

#[test]
fn think_twice_draws_and_flashes_back_for_two_and_blue() {
    let mut state = ready(Step::Main1);
    let think = put(&mut state, PlayerId::P0, "Think Twice", Zone::Hand);
    state.players[0].mana_pool = mana(&[(ManaColor::U, 1)], 1);
    cast(&mut state, think, None);
    settled(&mut state);
    assert_eq!(state.objects.get(think).zone, Zone::Graveyard);
    assert_eq!(state.players[0].hand.len(), 1);

    state.players[0].mana_pool = mana(&[(ManaColor::U, 1)], 1);
    assert!(
        !matches!(next(&mut state), Decision::CastSpellOrPass { castable_spells, .. } if castable_spells.contains(&think))
    );
    state.players[0].mana_pool = mana(&[(ManaColor::U, 1)], 2);
    cast(&mut state, think, None);
    settled(&mut state);
    assert_eq!(state.players[0].hand.len(), 2);
    assert_eq!(state.objects.get(think).zone, Zone::Exile);
}
