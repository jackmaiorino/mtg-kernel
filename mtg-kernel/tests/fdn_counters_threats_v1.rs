//! FDN batch 4: counterspells, a graveyard sorcery and simple trigger
//! threats, all built from existing engine primitives plus a creature-spell
//! target filter.
#![cfg(feature = "limited-fdn-fixtures")]
use mtg_kernel::card_def::{
    card_id_by_name, CardCapability, CardType, Keywords, Subtype, TargetSpec, CARD_DEFS,
};
use mtg_kernel::engine::{self, Action, Decision};
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::limited_session_v1::CustomDeckV1;
use mtg_kernel::mana::ManaColor;
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Target, Zone};
use mtg_kernel::trigger;

const FIRST_ID: usize = 268;

/// Name, mana cost as (colored pips, generic), colors, types, subtypes,
/// power/toughness, keywords and trigger count, in id order.
#[allow(clippy::type_complexity)]
const CARDS: [(
    &str,
    &[ManaColor],
    u8,
    &[ManaColor],
    &[CardType],
    &[Subtype],
    Option<(i16, i16)>,
    Keywords,
    usize,
); 10] = [
    ("Elementalist Adept", &[ManaColor::U], 1, &[ManaColor::U], &[CardType::Creature], &[Subtype::Human, Subtype::Wizard], Some((2, 1)), Keywords::FLASH, 1),
    ("Crypt Feaster", &[ManaColor::B], 3, &[ManaColor::B], &[CardType::Creature], &[Subtype::Zombie], Some((3, 4)), Keywords::MENACE, 1),
    ("Erudite Wizard", &[ManaColor::U], 2, &[ManaColor::U], &[CardType::Creature], &[Subtype::Human, Subtype::Wizard], Some((2, 3)), Keywords::NONE, 1),
    ("Phyrexian Arena", &[ManaColor::B, ManaColor::B], 1, &[ManaColor::B], &[CardType::Enchantment], &[], None, Keywords::NONE, 1),
    ("Gleaming Barrier", &[], 2, &[], &[CardType::Artifact, CardType::Creature], &[Subtype::Wall], Some((0, 4)), Keywords::DEFENDER, 1),
    ("Angel of Finality", &[ManaColor::W], 3, &[ManaColor::W], &[CardType::Creature], &[Subtype::Angel], Some((3, 4)), Keywords::FLYING, 1),
    ("Bigfin Bouncer", &[ManaColor::U], 3, &[ManaColor::U], &[CardType::Creature], &[Subtype::Shark, Subtype::Pirate], Some((3, 2)), Keywords::NONE, 1),
    ("Essence Scatter", &[ManaColor::U], 1, &[ManaColor::U], &[CardType::Instant], &[], None, Keywords::NONE, 0),
    ("Refute", &[ManaColor::U, ManaColor::U], 1, &[ManaColor::U], &[CardType::Instant], &[], None, Keywords::NONE, 0),
    ("Macabre Waltz", &[ManaColor::B], 1, &[ManaColor::B], &[CardType::Sorcery], &[], None, Keywords::NONE, 0),
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
        Zone::Graveyard => state.players[player.index()].graveyard.push(id),
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
    let after = next(state);
    assert!(matches!(after, Decision::CastSpellOrPass { .. }), "{after:?}");
    assert!(state.engine.pending_cast.is_none());
    assert_eq!(state.players[0].mana_pool, [0; 6]);
}

fn life(state: &GameState) -> (i32, i32) {
    (state.players[0].life, state.players[1].life)
}

#[test]
fn appended_definitions_match_their_printed_characteristics() {
    for (offset, (name, pips, generic, colors, types, subtypes, stats, keywords, triggers)) in
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
        assert_eq!(def.types, types, "{name}");
        assert_eq!(def.subtypes, subtypes, "{name}");
        assert_eq!(
            (def.power, def.toughness),
            (stats.map(|s| s.0), stats.map(|s| s.1)),
            "{name}"
        );
        assert_eq!(def.keywords, keywords, "{name}");
        assert_eq!(trigger::triggers_for(id as u16).len(), triggers, "{name}");
    }
    let target = |name| CARD_DEFS[card_id_by_name(name).unwrap() as usize].target_spec;
    assert_eq!(target("Essence Scatter"), TargetSpec::CreatureSpellOnStack);
    assert_eq!(target("Refute"), TargetSpec::AnySpellOnStack);
    assert_eq!(
        target("Macabre Waltz"),
        TargetSpec::UpToTwoCreatureCardsInOwnGraveyard
    );
    for subtype in [Subtype::Shark] {
        assert!(Subtype::CREATURE_TYPES.contains(&subtype));
        assert!(subtype.is_creature_type());
    }
}

#[test]
fn reference_deck_resolves_all_forty_copies() {
    let deck: CustomDeckV1 = serde_json::from_str(
        r#"{"cards":[{"name":"Elementalist Adept","count":1},
            {"name":"Crypt Feaster","count":1},{"name":"Erudite Wizard","count":1},
            {"name":"Phyrexian Arena","count":1},
            {"name":"Gleaming Barrier","count":1},{"name":"Angel of Finality","count":1},
            {"name":"Bigfin Bouncer","count":1},{"name":"Essence Scatter","count":1},
            {"name":"Refute","count":1},{"name":"Macabre Waltz","count":1},
            {"name":"Island","count":30}]}"#,
    )
    .unwrap();
    let ids = deck.resolve().unwrap();
    assert_eq!(ids.len(), 40);
    let first = FIRST_ID as u16;
    assert_eq!(&ids[..10], &(first..first + 10).collect::<Vec<u16>>()[..]);
}

#[test]
fn permanents_cast_for_their_exact_costs() {
    for (name, pips, generic, _, types, ..) in CARDS {
        if types.contains(&CardType::Instant) || types.contains(&CardType::Sorcery) {
            continue;
        }
        let mut pool = [0u8; 6];
        for pip in pips {
            pool[pip.pool_index()] += 1;
        }
        pool[5] += generic;
        let mut state = ready(Step::Main1);
        let permanent = put(&mut state, PlayerId::P0, name, Zone::Hand);
        let mut short = pool;
        short[5] -= 1;
        state.players[0].mana_pool = short;
        assert!(
            engine::step(&mut state, Action::CastSpell(permanent)).is_err(),
            "{name}"
        );
        state.players[0].mana_pool = pool;
        cast(&mut state, permanent, None);
        if let Some(Decision::ChooseTargets { legal_targets, .. }) = settle(&mut state) {
            engine::step(&mut state, Action::ChooseTarget(legal_targets[0])).unwrap();
            settled(&mut state);
        }
        assert_eq!(state.objects.get(permanent).zone, Zone::Battlefield, "{name}");
    }
}

#[test]
fn elementalist_adept_has_flash_and_prowess() {
    let mut state = ready(Step::Main1);
    // Flash: castable while a spell is on the stack.
    let think = put(&mut state, PlayerId::P0, "Think Twice", Zone::Hand);
    let adept = put(&mut state, PlayerId::P0, "Elementalist Adept", Zone::Hand);
    state.players[0].mana_pool = mana(&[(ManaColor::U, 1)], 1);
    cast(&mut state, think, None);
    state.players[0].mana_pool = mana(&[(ManaColor::U, 1)], 1);
    cast(&mut state, adept, None);
    settled(&mut state);
    assert_eq!(state.objects.get(adept).zone, Zone::Battlefield);
    // The adept entered after Think Twice was cast, so it did not see it.
    assert_eq!(engine::effective_power(&state, adept), 2);

    let think = put(&mut state, PlayerId::P0, "Think Twice", Zone::Hand);
    state.players[0].mana_pool = mana(&[(ManaColor::U, 1)], 1);
    cast(&mut state, think, None);
    settled(&mut state);
    assert_eq!(engine::effective_power(&state, adept), 3);
    assert_eq!(engine::effective_toughness(&state, adept), 2);
}

fn fill_graveyard(state: &mut GameState, count: usize) {
    for _ in 0..count {
        put(state, PlayerId::P0, "Stab", Zone::Graveyard);
    }
}

#[test]
fn crypt_feaster_gets_two_power_attacking_only_with_threshold() {
    for (cards, power) in [(6, 3), (7, 5)] {
        let mut state = ready(Step::DeclareAttackers);
        let feaster = put(&mut state, PlayerId::P0, "Crypt Feaster", Zone::Battlefield);
        fill_graveyard(&mut state, cards);
        engine::step(&mut state, Action::DeclareAttackers(vec![feaster])).unwrap();
        settled(&mut state);
        assert_eq!(engine::effective_power(&state, feaster), power, "{cards}");
        assert_eq!(engine::effective_toughness(&state, feaster), 4);
    }
}

#[test]
fn erudite_wizard_grows_on_the_second_draw_each_turn() {
    let mut state = ready(Step::Main1);
    let wizard = put(&mut state, PlayerId::P0, "Erudite Wizard", Zone::Battlefield);
    for expected in [2, 3, 3] {
        let think = put(&mut state, PlayerId::P0, "Think Twice", Zone::Hand);
        state.players[0].mana_pool = mana(&[(ManaColor::U, 1)], 1);
        cast(&mut state, think, None);
        settled(&mut state);
        assert_eq!(engine::effective_power(&state, wizard), expected);
    }
    assert_eq!(engine::effective_toughness(&state, wizard), 4);
}

#[test]
fn gleaming_barrier_cannot_attack_and_leaves_a_treasure_when_it_dies() {
    let mut state = ready(Step::DeclareAttackers);
    let barrier = put(&mut state, PlayerId::P0, "Gleaming Barrier", Zone::Battlefield);
    assert!(engine::step(&mut state, Action::DeclareAttackers(vec![barrier])).is_err());

    let mut state = ready(Step::Main1);
    let barrier = put(&mut state, PlayerId::P0, "Gleaming Barrier", Zone::Battlefield);
    move_to(&mut state, barrier, Zone::Graveyard);
    settled(&mut state);
    let [token] = state.players[0].battlefield[..] else {
        panic!("expected one Treasure");
    };
    let token = state.objects.get(token);
    assert_eq!(token.card_def, card_id_by_name("Treasure Token").unwrap());
    assert_eq!(token.controller, PlayerId::P0);
}

#[test]
fn phyrexian_arena_draws_and_drains_its_controller_each_upkeep() {
    let mut state = ready(Step::Untap);
    state.active_player = PlayerId::P0;
    state.priority_player = PlayerId::P0;
    put(&mut state, PlayerId::P0, "Phyrexian Arena", Zone::Battlefield);
    let hand = state.players[0].hand.len();
    for _ in 0..20 {
        if state.step == Step::Draw || state.step == Step::Main1 {
            break;
        }
        match next(&mut state) {
            Decision::CastSpellOrPass { .. } => engine::step(&mut state, Action::Pass).unwrap(),
            other => panic!("unexpected decision: {other:?}"),
        }
    }
    assert_eq!(life(&state), (19, 20));
    assert!(state.players[0].hand.len() >= hand + 1);
}

#[test]
fn angel_of_finality_exiles_the_target_players_graveyard() {
    let mut state = ready(Step::Main1);
    put(&mut state, PlayerId::P1, "Stab", Zone::Graveyard);
    put(&mut state, PlayerId::P1, "Refute", Zone::Graveyard);
    let own = put(&mut state, PlayerId::P0, "Stab", Zone::Graveyard);
    let angel = put(&mut state, PlayerId::P0, "Angel of Finality", Zone::Hand);
    state.players[0].mana_pool = mana(&[(ManaColor::W, 1)], 3);
    cast(&mut state, angel, None);
    let Some(Decision::ChooseTargets { legal_targets, .. }) = settle(&mut state) else {
        panic!("expected player target");
    };
    assert!(legal_targets.contains(&Target::Player(PlayerId::P0)));
    assert!(legal_targets.contains(&Target::Player(PlayerId::P1)));
    engine::step(&mut state, Action::ChooseTarget(Target::Player(PlayerId::P1))).unwrap();
    settled(&mut state);
    assert!(state.players[1].graveyard.is_empty());
    assert_eq!(state.players[0].graveyard, vec![own]);
}

#[test]
fn bigfin_bouncer_returns_an_opponents_creature_to_hand() {
    let mut state = ready(Step::Main1);
    let own = put(&mut state, PlayerId::P0, "Crypt Feaster", Zone::Battlefield);
    let enemy = put(&mut state, PlayerId::P1, "Crypt Feaster", Zone::Battlefield);
    let bouncer = put(&mut state, PlayerId::P0, "Bigfin Bouncer", Zone::Hand);
    state.players[0].mana_pool = mana(&[(ManaColor::U, 1)], 3);
    cast(&mut state, bouncer, None);
    let Some(Decision::ChooseTargets { legal_targets, .. }) = settle(&mut state) else {
        panic!("expected creature target");
    };
    assert_eq!(legal_targets, vec![Target::Object(enemy)]);
    assert!(!legal_targets.contains(&Target::Object(own)));
    engine::step(&mut state, Action::ChooseTarget(Target::Object(enemy))).unwrap();
    settled(&mut state);
    let enemy_card = card_id_by_name("Crypt Feaster").unwrap();
    assert!(state.players[1]
        .hand
        .iter()
        .any(|&id| state.objects.get(id).card_def == enemy_card));
    assert!(state.players[1].battlefield.is_empty());

    // With no opposing creature the trigger has no target and does nothing.
    let mut state = ready(Step::Main1);
    let bouncer = put(&mut state, PlayerId::P0, "Bigfin Bouncer", Zone::Hand);
    state.players[0].mana_pool = mana(&[(ManaColor::U, 1)], 3);
    cast(&mut state, bouncer, None);
    settled(&mut state);
    assert_eq!(state.objects.get(bouncer).zone, Zone::Battlefield);
}

#[test]
fn essence_scatter_counters_creature_spells_only() {
    let mut state = ready(Step::Main1);
    let wizard = put(&mut state, PlayerId::P0, "Erudite Wizard", Zone::Hand);
    let scatter = put(&mut state, PlayerId::P0, "Essence Scatter", Zone::Hand);
    state.players[0].mana_pool = mana(&[(ManaColor::U, 1)], 2);
    cast(&mut state, wizard, None);
    state.players[0].mana_pool = mana(&[(ManaColor::U, 1)], 1);
    cast(&mut state, scatter, Some(wizard));
    settled(&mut state);
    assert_eq!(state.objects.get(wizard).zone, Zone::Graveyard);
    assert_eq!(state.objects.get(scatter).zone, Zone::Graveyard);

    let mut state = ready(Step::Main1);
    let think = put(&mut state, PlayerId::P0, "Think Twice", Zone::Hand);
    let scatter = put(&mut state, PlayerId::P0, "Essence Scatter", Zone::Hand);
    state.players[0].mana_pool = mana(&[(ManaColor::U, 1)], 1);
    cast(&mut state, think, None);
    assert!(engine::legal_targets_for(TargetSpec::CreatureSpellOnStack, &[], &state).is_empty());
    state.players[0].mana_pool = mana(&[(ManaColor::U, 1)], 1);
    assert!(
        !matches!(next(&mut state), Decision::CastSpellOrPass { castable_spells, .. } if castable_spells.contains(&scatter))
    );
}

#[test]
fn refute_counters_any_spell_then_loots() {
    let mut state = ready(Step::Main1);
    let kept = put(&mut state, PlayerId::P0, "Stab", Zone::Hand);
    let think = put(&mut state, PlayerId::P0, "Think Twice", Zone::Hand);
    let refute = put(&mut state, PlayerId::P0, "Refute", Zone::Hand);
    state.players[0].mana_pool = mana(&[(ManaColor::U, 1)], 1);
    cast(&mut state, think, None);
    state.players[0].mana_pool = mana(&[(ManaColor::U, 2)], 1);
    cast(&mut state, refute, Some(think));
    let Some(Decision::Discard {
        player,
        count,
        choices,
    }) = settle(&mut state)
    else {
        panic!("expected loot discard");
    };
    assert_eq!((player, count, choices.len()), (PlayerId::P0, 1, 2));
    let drawn = *choices.iter().find(|&&id| id != kept).unwrap();
    engine::step(&mut state, Action::Discard(vec![drawn])).unwrap();
    settled(&mut state);
    assert_eq!(state.objects.get(think).zone, Zone::Graveyard);
    assert_eq!(state.objects.get(refute).zone, Zone::Graveyard);
    assert_eq!(state.players[0].hand, vec![kept]);
    assert_eq!(state.players[0].graveyard.len(), 3);
}

#[test]
fn macabre_waltz_returns_up_to_two_creature_cards_then_discards() {
    let mut state = ready(Step::Main1);
    let first = put(&mut state, PlayerId::P0, "Crypt Feaster", Zone::Graveyard);
    let second = put(&mut state, PlayerId::P0, "Bigfin Bouncer", Zone::Graveyard);
    let left = put(&mut state, PlayerId::P0, "Erudite Wizard", Zone::Graveyard);
    let noncreature = put(&mut state, PlayerId::P0, "Stab", Zone::Graveyard);
    let theirs = put(&mut state, PlayerId::P1, "Crypt Feaster", Zone::Graveyard);
    let waltz = put(&mut state, PlayerId::P0, "Macabre Waltz", Zone::Hand);
    state.players[0].mana_pool = mana(&[(ManaColor::B, 1)], 1);
    engine::step(&mut state, Action::CastSpell(waltz)).unwrap();
    let decision = next(&mut state);
    let Decision::ChooseTargets {
        legal_targets,
        remaining,
        can_finish,
        ..
    } = &decision
    else {
        panic!("expected graveyard targets, got {decision:?}");
    };
    assert_eq!((*remaining, *can_finish), (2, true));
    assert_eq!(
        legal_targets,
        &vec![
            Target::Object(first),
            Target::Object(second),
            Target::Object(left)
        ]
    );
    assert!(!legal_targets.contains(&Target::Object(noncreature)));
    assert!(!legal_targets.contains(&Target::Object(theirs)));
    engine::step(&mut state, Action::ChooseTarget(Target::Object(first))).unwrap();
    engine::step(&mut state, Action::ChooseTarget(Target::Object(second))).unwrap();
    let Some(Decision::Discard {
        player,
        count,
        choices,
    }) = settle(&mut state)
    else {
        panic!("expected discard");
    };
    assert_eq!((player, count), (PlayerId::P0, 1));
    assert_eq!(choices, vec![first, second]);
    engine::step(&mut state, Action::Discard(vec![second])).unwrap();
    settled(&mut state);
    assert_eq!(state.players[0].hand, vec![first]);
    assert_eq!(state.objects.get(left).zone, Zone::Graveyard);
    assert_eq!(state.objects.get(waltz).zone, Zone::Graveyard);
}
