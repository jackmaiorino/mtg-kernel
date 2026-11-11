//! Premodern sideboard wave P3f: native rules tests for the cards that
//! complete Mono-Black Control's 75.
#![cfg(feature = "eternal-fixtures")]

use mtg_kernel::card_def::{
    card_id_by_name, mana_color_mask, CardCapability, CardDef, CardType, Keywords, Subtype,
    CARD_DEFS,
};
use mtg_kernel::engine::{self, Action, CastMode, Decision};
use mtg_kernel::eternal_premodern_p3f_v1::{
    minus_two_minus_one_counters, processor_payment, read_state,
};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::ManaColor;
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, StackItemKind, Step, Target, Zone};

const P0: PlayerId = PlayerId::P0;
const P1: PlayerId = PlayerId::P1;

const P3F_CARDS: [&str; 3] = ["Contagion", "Lurking Skirge", "Phyrexian Processor"];

fn def(name: &str) -> &'static CardDef {
    &CARD_DEFS[id(name) as usize]
}

fn id(name: &str) -> u16 {
    card_id_by_name(name).unwrap_or_else(|| panic!("{name}"))
}

/// A first main phase with P0 active and holding priority. Each library
/// holds twenty Islands, and both hands are empty.
fn islands() -> GameState {
    let ids = vec![id("Island"); 20];
    let mut state = GameState::new_from_libraries(
        &ids,
        &ids,
        |id| CARD_DEFS[id as usize].object_name.into(),
        0x5033_4600_0001,
    );
    state.active_player = P0;
    state.priority_player = P0;
    state.step = Step::Main1;
    assert!(state.players.iter().all(|seat| seat.hand.is_empty()));
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

fn token(state: &mut GameState, player: PlayerId, name: &str) -> ObjectId {
    let object = put(state, player, name, Zone::Battlefield);
    state.objects.get_mut(object).v4.is_token = true;
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

fn has_type(state: &GameState, object: ObjectId, card_type: CardType) -> bool {
    engine::object_has_type(state, object, card_type)
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

/// Passes priority and orders triggers until the stack and pending triggers
/// are empty, returning the first other decision if one interrupts.
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

fn choose_target(state: &mut GameState, target: Target) {
    match next(state) {
        Decision::ChooseTargets { legal_targets, .. } => {
            assert!(
                legal_targets.contains(&target),
                "{target:?} not in {legal_targets:?}"
            );
            act(state, Action::ChooseTarget(target));
        }
        other => panic!("expected targets, got {other:?}"),
    }
}

/// Casts `spell` for its mana cost, answering each target prompt from
/// `targets` in order.
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

/// A serialized round trip of `state`.
fn restored(state: &GameState) -> GameState {
    serde_json::from_slice(&serde_json::to_vec(state).unwrap()).unwrap()
}

/// Plays on until `stop` holds before a decision, passing priority, not
/// attacking or blocking and keeping triggers in order.
fn pass_until(state: &mut GameState, stop: impl Fn(&GameState) -> bool) {
    for _ in 0..3000 {
        let decision = next(state);
        if stop(state) {
            return;
        }
        let action = match decision {
            Decision::CastSpellOrPass { .. } => Action::Pass,
            Decision::DeclareAttackers { .. } => Action::DeclareAttackers(Vec::new()),
            Decision::DeclareBlockers { .. } => Action::DeclareBlockers(Vec::new()),
            Decision::OrderTriggers { pending, .. } => {
                Action::OrderTriggers((0..pending.len()).collect())
            }
            other => panic!("unexpected choice: {other:?}"),
        };
        act(state, action);
    }
    panic!("condition never held");
}

// ---- Characteristics -----------------------------------------------------------

#[test]
fn p3f_cards_are_full_with_their_printed_characteristics() {
    for name in P3F_CARDS {
        assert_eq!(def(name).capability, CardCapability::Full, "{name}");
        assert!(!def(name).is_token, "{name}");
    }
    let contagion = def("Contagion");
    assert!(contagion.has_type(CardType::Instant));
    assert_eq!(
        (contagion.mana_value, contagion.colors),
        (5, &[ManaColor::B][..])
    );
    let skirge = def("Lurking Skirge");
    assert!(skirge.has_type(CardType::Enchantment) && !skirge.has_type(CardType::Creature));
    assert_eq!((skirge.mana_value, skirge.colors), (2, &[ManaColor::B][..]));
    let processor = def("Phyrexian Processor");
    assert!(processor.has_type(CardType::Artifact));
    assert_eq!((processor.mana_value, processor.colors), (4, &[][..]));
    assert_eq!(processor.activated_abilities.len(), 1);
    let minion = def("Phyrexian Minion Token");
    assert!(minion.is_token && minion.has_type(CardType::Creature));
    assert_eq!(minion.colors, &[ManaColor::B][..]);
    assert_eq!(minion.subtypes, &[Subtype::Phyrexian, Subtype::Minion][..]);
}

// ---- Contagion ------------------------------------------------------------------

#[test]
fn contagion_divides_two_minus_two_minus_one_counters_among_its_targets() {
    let mut state = islands();
    let angler = put(&mut state, P1, "Gurmag Angler", Zone::Battlefield);
    state.objects.get_mut(angler).counters.plus1_plus1 = 1;
    let elves = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    let island = put(&mut state, P1, "Island", Zone::Battlefield);
    let first = put(&mut state, P0, "Contagion", Zone::Hand);
    let second = put(&mut state, P0, "Contagion", Zone::Hand);

    // The other Contagion in hand could pay the alternative cost, so the
    // mode is asked. Two picks among creatures only: two targets get one
    // counter each.
    pool(&mut state, P0, &[ManaColor::B, ManaColor::B], 3);
    assert!(castable(&mut state, first));
    act(&mut state, Action::CastSpell(first));
    let mut picks = vec![Target::Object(elves), Target::Object(angler)];
    let mut mode_asked = false;
    while let Some(decision) = settle(&mut state) {
        let action = match decision {
            Decision::ChooseCastMode { options, .. } => {
                assert!(options.contains(&CastMode::Alternative));
                mode_asked = true;
                Action::ChooseCastMode(CastMode::Normal)
            }
            Decision::ChooseTargets { legal_targets, .. } => {
                assert!(legal_targets.contains(&Target::Object(angler)));
                assert!(legal_targets.contains(&Target::Object(elves)));
                assert!(!legal_targets.contains(&Target::Object(island)), "a land");
                assert!(!legal_targets.contains(&Target::Player(P1)), "a player");
                Action::ChooseTarget(picks.pop().expect("two picks"))
            }
            other => panic!("unexpected choice: {other:?}"),
        };
        act(&mut state, action);
    }
    assert!(mode_asked && picks.is_empty());
    assert_eq!(zone(&state, second), Zone::Hand, "paid with mana");
    assert_eq!(minus_two_minus_one_counters(&state, angler), 1);
    assert_eq!(power_toughness(&state, angler), (4, 5), "a 6/6 with one");
    assert_ne!(zone(&state, elves), Zone::Battlefield, "a 1/1 dies at -1/0");

    // One creature picked twice is one target with both counters. -2/-1
    // counters don't annihilate with +1/+1 counters (704.5q).
    pool(&mut state, P0, &[ManaColor::B, ManaColor::B], 3);
    cast(
        &mut state,
        second,
        &[Target::Object(angler), Target::Object(angler)],
    );
    let mut copy = restored(&state);
    assert_eq!(state.state_hash(), copy.state_hash());
    for continued in [&mut state, &mut copy] {
        settled(continued);
        assert_eq!(minus_two_minus_one_counters(continued, angler), 3);
        assert_eq!(continued.objects.get(angler).counters.plus1_plus1, 1);
        assert_eq!(power_toughness(continued, angler), (0, 3));
    }
    assert_eq!(state.state_hash(), copy.state_hash());

    // The Elves' counter is dropped as the next turn begins.
    assert_eq!(read_state(&state).unwrap().minus_two_minus_one.len(), 2);
    pass_until(&mut state, |state| state.active_player == P1);
    assert_eq!(read_state(&state).unwrap().minus_two_minus_one.len(), 1);
    assert_eq!(minus_two_minus_one_counters(&state, angler), 3);
}

#[test]
fn contagion_can_be_cast_for_one_life_and_another_black_card() {
    let mut state = islands();
    let elves = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    let contagion = put(&mut state, P0, "Contagion", Zone::Hand);

    // Without mana and without another black card it can't be cast.
    assert!(!castable(&mut state, contagion), "it can't exile itself");

    let ritual = put(&mut state, P0, "Dark Ritual", Zone::Hand);
    assert!(castable(&mut state, contagion));
    act(&mut state, Action::CastSpell(contagion));
    let mut picks = 0;
    while let Some(decision) = settle(&mut state) {
        let action = match decision {
            Decision::ChooseCastMode { options, .. } => {
                assert!(options.contains(&CastMode::Alternative));
                Action::ChooseCastMode(CastMode::Alternative)
            }
            Decision::ChooseCostTargets { candidates, .. } => {
                assert_eq!(candidates, vec![ritual], "only another black card");
                Action::ChooseCostTarget(ritual)
            }
            Decision::ChooseTargets { .. } => {
                picks += 1;
                Action::ChooseTarget(Target::Object(elves))
            }
            other => panic!("unexpected choice: {other:?}"),
        };
        act(&mut state, action);
    }
    assert_eq!(picks, 2);
    assert_eq!(life(&state, P0), 19);
    assert_eq!(zone(&state, ritual), Zone::Exile);
    assert_eq!(zone(&state, contagion), Zone::Graveyard);
    assert_ne!(zone(&state, elves), Zone::Battlefield);
}

// ---- Lurking Skirge -------------------------------------------------------------

/// Settles like `settled`, returning the most triggers of `source` that
/// waited at once, pending or on the stack.
fn settle_counting_triggers(state: &mut GameState, source: ObjectId) -> usize {
    let mut most = 0;
    for _ in 0..200 {
        let decision = next(state);
        let waiting = state
            .stack
            .iter()
            .filter(|item| item.kind == StackItemKind::TriggeredAbility && item.source == source)
            .count()
            + state
                .engine
                .pending_triggers
                .iter()
                .filter(|trigger| trigger.source == source)
                .count();
        most = most.max(waiting);
        match decision {
            Decision::OrderTriggers { pending, .. } => {
                act(state, Action::OrderTriggers((0..pending.len()).collect()))
            }
            Decision::CastSpellOrPass { .. }
                if state.stack.is_empty() && state.engine.pending_triggers.is_empty() =>
            {
                return most
            }
            Decision::CastSpellOrPass { .. } => act(state, Action::Pass),
            other => panic!("unexpected choice: {other:?}"),
        }
    }
    panic!("did not settle");
}

#[test]
fn lurking_skirge_becomes_a_flying_imp_when_an_opponents_creature_dies() {
    let mut state = islands();
    let skirge = put(&mut state, P0, "Lurking Skirge", Zone::Battlefield);
    let mine = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    let elves = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    let goblin = token(&mut state, P1, "Goblin Token");
    let fanatic = put(&mut state, P1, "Mogg Fanatic", Zone::Battlefield);
    let first_bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    let second_bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    let pyroclasm = put(&mut state, P0, "Pyroclasm", Zone::Hand);

    // A creature put into its controller's own graveyard doesn't trigger it.
    pool(&mut state, P0, &[ManaColor::R], 0);
    cast(&mut state, first_bolt, &[Target::Object(mine)]);
    assert_eq!(settle_counting_triggers(&mut state, skirge), 0);
    assert_eq!(zone(&state, mine), Zone::Graveyard);
    assert!(has_type(&state, skirge, CardType::Enchantment));
    assert!(!has_type(&state, skirge, CardType::Creature));

    // Three of the opponent's creatures die at once, a token among them:
    // three triggers. The first to resolve changes the Skirge; the others
    // find it no longer an enchantment (603.4).
    pool(&mut state, P0, &[ManaColor::R], 1);
    cast(&mut state, pyroclasm, &[]);
    let mut copy = restored(&state);
    assert_eq!(state.state_hash(), copy.state_hash());
    for continued in [&mut state, &mut copy] {
        assert_eq!(settle_counting_triggers(continued, skirge), 3);
        assert_eq!(read_state(continued).unwrap().skirges.len(), 1);
    }
    assert_eq!(state.state_hash(), copy.state_hash());
    for object in [elves, goblin, fanatic] {
        assert_ne!(zone(&state, object), Zone::Battlefield);
    }

    // A 3/2 black Phyrexian Imp creature with flying, and no longer an
    // enchantment.
    assert!(has_type(&state, skirge, CardType::Creature));
    assert!(!has_type(&state, skirge, CardType::Enchantment));
    assert!(engine::has_effective_subtype(
        &state,
        skirge,
        Subtype::Phyrexian
    ));
    assert!(engine::has_effective_subtype(&state, skirge, Subtype::Imp));
    assert_eq!(power_toughness(&state, skirge), (3, 2));
    assert!(engine::has_effective_keyword(
        &state,
        skirge,
        Keywords::FLYING
    ));
    assert_eq!(
        engine::object_color_mask(&state, skirge),
        mana_color_mask(ManaColor::B)
    );

    // The change has no duration, and a later death doesn't trigger it.
    let turn = state.turn;
    pass_until(&mut state, |state| {
        state.active_player == P0 && state.step == Step::Main1 && state.turn > turn
    });
    assert!(has_type(&state, skirge, CardType::Creature));
    assert_eq!(power_toughness(&state, skirge), (3, 2));
    let elves = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    pool(&mut state, P0, &[ManaColor::R], 0);
    cast(&mut state, second_bolt, &[Target::Object(elves)]);
    assert_eq!(settle_counting_triggers(&mut state, skirge), 0);
    assert_eq!(zone(&state, elves), Zone::Graveyard);
    assert_eq!(read_state(&state).unwrap().skirges.len(), 1);
}

// ---- Phyrexian Processor --------------------------------------------------------

#[test]
fn phyrexian_processor_pays_life_as_it_enters_for_its_minions() {
    let mut state = islands();
    let processor = put(&mut state, P0, "Phyrexian Processor", Zone::Hand);
    let crumble = put(&mut state, P1, "Crumble", Zone::Hand);

    pool(&mut state, P0, &[], 4);
    cast(&mut state, processor, &[]);
    match settle(&mut state) {
        Some(Decision::ChooseEffectOption {
            player,
            source,
            option_count,
        }) => {
            // Any amount from 0 to its controller's life (119.4).
            assert_eq!((player, source, option_count), (P0, processor, 21));
        }
        other => panic!("expected the life payment, got {other:?}"),
    }
    let mut probe = state.clone();
    assert!(engine::step(&mut probe, Action::ChooseEffectOption(21)).is_err());
    let mut copy = restored(&state);
    assert_eq!(state.state_hash(), copy.state_hash());
    for continued in [&mut state, &mut copy] {
        act(continued, Action::ChooseEffectOption(7));
        settled(continued);
        assert_eq!(life(continued, P0), 13);
        assert_eq!(processor_payment(continued, processor), Some(7));
    }

    // "{4}, {T}: Create an X/X black Phyrexian Minion creature token": X is
    // the life paid, still known when the Processor is destroyed in
    // response.
    pool(&mut state, P0, &[], 4);
    assert!(priority(&mut state).1.contains(&(processor, 0)));
    act(&mut state, Action::ActivateAbility(processor, 0));
    assert!(matches!(
        next(&mut state),
        Decision::CastSpellOrPass { player, .. } if player == P0
    ));
    act(&mut state, Action::Pass);
    pool(&mut state, P1, &[ManaColor::G], 0);
    assert!(castable(&mut state, crumble), "P1 holds priority");
    act(&mut state, Action::CastSpell(crumble));
    choose_target(&mut state, Target::Object(processor));
    settled(&mut state);
    assert_eq!(zone(&state, processor), Zone::Graveyard);
    assert_eq!(life(&state, P0), 17, "Crumble's 4 life");
    let minions = state.players[0]
        .battlefield
        .iter()
        .copied()
        .filter(|&object| {
            CARD_DEFS[state.objects.get(object).card_def as usize].name == "Phyrexian Minion Token"
        })
        .collect::<Vec<_>>();
    assert_eq!(minions.len(), 1);
    let minion = minions[0];
    assert!(state.objects.get(minion).v4.is_token);
    assert_eq!(power_toughness(&state, minion), (7, 7));
    assert!(has_type(&state, minion, CardType::Creature));
    assert!(engine::has_effective_subtype(
        &state,
        minion,
        Subtype::Minion
    ));
    assert!(engine::has_effective_subtype(
        &state,
        minion,
        Subtype::Phyrexian
    ));
    let mut copy = restored(&state);
    assert_eq!(state.state_hash(), copy.state_hash());
    assert_eq!(power_toughness(&copy, minion), (7, 7));

    // The payment of the incarnation that left is dropped as the next turn
    // begins; the Minion keeps its size.
    pass_until(&mut copy, |state| state.active_player == P1);
    let records = read_state(&copy).unwrap();
    assert!(records.payments.is_empty());
    assert_eq!(records.minions.len(), 1);
    assert_eq!(power_toughness(&copy, minion), (7, 7));
}

#[test]
fn phyrexian_processor_that_paid_nothing_makes_minions_that_die() {
    let mut state = islands();
    let processor = put(&mut state, P0, "Phyrexian Processor", Zone::Hand);
    pool(&mut state, P0, &[], 4);
    cast(&mut state, processor, &[]);
    match settle(&mut state) {
        Some(Decision::ChooseEffectOption { option_count, .. }) => assert_eq!(option_count, 21),
        other => panic!("expected the life payment, got {other:?}"),
    }
    act(&mut state, Action::ChooseEffectOption(0));
    settled(&mut state);
    assert_eq!(life(&state, P0), 20, "paying 0 life is allowed");
    assert_eq!(processor_payment(&state, processor), Some(0));
    pool(&mut state, P0, &[], 4);
    act(&mut state, Action::ActivateAbility(processor, 0));
    settled(&mut state);
    assert!(
        state.players[0].battlefield.iter().all(|&object| {
            CARD_DEFS[state.objects.get(object).card_def as usize].name != "Phyrexian Minion Token"
        }),
        "a 0/0 Minion dies"
    );
}
