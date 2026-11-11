//! Premodern sideboard wave P3c: native rules tests for the cards that
//! complete the 75s of Psychatog, Elves and Reanimator.
#![cfg(feature = "eternal-fixtures")]

use mtg_kernel::card_def::{
    card_id_by_name, CardCapability, CardDef, CardType, Keywords, Subtype, Supertype, CARD_DEFS,
};
use mtg_kernel::engine::{self, Action, Decision};
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::ManaColor;
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Target, Zone};

const P0: PlayerId = PlayerId::P0;
const P1: PlayerId = PlayerId::P1;

const P3C_CARDS: [&str; 8] = [
    "Crumble",
    "Dominate",
    "Druid Lyrist",
    "Ihsan's Shade",
    "Iridescent Angel",
    "Multani, Maro-Sorcerer",
    "Spike Feeder",
    "Uktabi Orangutan",
];

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
        0x5033_4300_0001,
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

fn keyword(state: &GameState, object: ObjectId, keyword: Keywords) -> bool {
    engine::has_effective_keyword(state, object, keyword)
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

/// Settles, answering every choice with `answer`.
fn settle_with(state: &mut GameState, mut answer: impl FnMut(&GameState, Decision) -> Action) {
    while let Some(decision) = settle(state) {
        let action = answer(state, decision);
        act(state, action);
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

fn activatable(state: &mut GameState, source: ObjectId, index: u8) -> bool {
    priority(state).1.contains(&(source, index))
}

/// Casts `spell`, answering each target prompt from `targets` in order.
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

/// Activates `source`'s ability `index`, answering each target prompt from
/// `targets` in order.
fn activate(state: &mut GameState, source: ObjectId, index: u8, targets: &[Target]) {
    assert!(
        activatable(state, source, index),
        "{} ability {index} not activatable",
        state.objects.get(source).name
    );
    act(state, Action::ActivateAbility(source, index));
    for target in targets {
        choose_target(state, *target);
    }
}

/// Finishes an activation whose targets are chosen: its costs are paid and
/// the ability waits on the stack.
fn on_the_stack(state: &mut GameState) {
    let before = state.stack.len();
    assert!(matches!(next(state), Decision::CastSpellOrPass { .. }));
    assert_eq!(state.stack.len(), before + 1);
}

/// Finishes a cast whose targets are chosen: its costs are paid and the
/// spell, put on the stack as the cast began, waits there.
fn cast_finished(state: &mut GameState, spell: ObjectId) {
    assert!(matches!(next(state), Decision::CastSpellOrPass { .. }));
    assert_eq!(zone(state, spell), Zone::Stack);
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

/// The targets `spell` may choose first, cast by its owner now.
fn cast_targets(state: &GameState, spell: ObjectId) -> Vec<Target> {
    let mut probe = state.clone();
    assert!(castable(&mut probe, spell));
    act(&mut probe, Action::CastSpell(spell));
    match next(&mut probe) {
        Decision::ChooseTargets { legal_targets, .. } => legal_targets,
        other => panic!("expected targets, got {other:?}"),
    }
}

/// The targets `source`'s ability `index` may choose, activated now.
fn ability_targets(state: &GameState, source: ObjectId, index: u8) -> Vec<Target> {
    let mut probe = state.clone();
    assert!(activatable(&mut probe, source, index));
    act(&mut probe, Action::ActivateAbility(source, index));
    match next(&mut probe) {
        Decision::ChooseTargets { legal_targets, .. } => legal_targets,
        other => panic!("expected targets, got {other:?}"),
    }
}

/// Plays on until `stop` holds before a decision: passing priority and not
/// attacking or blocking.
fn pass_until(state: &mut GameState, stop: impl Fn(&GameState) -> bool) {
    for _ in 0..2000 {
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

/// Moves `object` to its owner's graveyard, as if it were destroyed.
fn bury(state: &mut GameState, object: ObjectId) {
    event::propose_and_commit(state, ProposedEvent::zone_change(object, Zone::Graveyard));
}

// ---- Characteristics -----------------------------------------------------------

#[test]
fn p3c_cards_are_full_with_their_printed_characteristics() {
    for name in P3C_CARDS {
        assert_eq!(def(name).capability, CardCapability::Full, "{name}");
        assert!(!def(name).is_token, "{name}");
    }
    for (name, power, toughness, subtypes) in [
        ("Druid Lyrist", 1, 1, &[Subtype::Human, Subtype::Druid][..]),
        (
            "Ihsan's Shade",
            5,
            5,
            &[Subtype::Shade, Subtype::Knight][..],
        ),
        ("Iridescent Angel", 4, 4, &[Subtype::Angel][..]),
        ("Multani, Maro-Sorcerer", 0, 0, &[Subtype::Elemental][..]),
        ("Spike Feeder", 0, 0, &[Subtype::Spike][..]),
        ("Uktabi Orangutan", 2, 2, &[Subtype::Ape][..]),
    ] {
        let card = def(name);
        assert!(card.has_type(CardType::Creature), "{name}");
        assert_eq!(
            (card.power, card.toughness),
            (Some(power), Some(toughness)),
            "{name}"
        );
        for subtype in subtypes {
            assert!(card.subtypes.contains(subtype), "{name}: {subtype:?}");
        }
    }
    assert!(Subtype::Spike.is_creature_type());
    for name in ["Ihsan's Shade", "Multani, Maro-Sorcerer"] {
        assert!(
            def(name).supertypes.contains(&Supertype::Legendary),
            "{name}"
        );
    }
    assert!(def("Iridescent Angel").keywords.has(Keywords::FLYING));
    assert!(def("Multani, Maro-Sorcerer").keywords.has(Keywords::SHROUD));
    assert_eq!(def("Druid Lyrist").activated_abilities.len(), 1);
    assert_eq!(def("Spike Feeder").activated_abilities.len(), 2);
    for name in ["Crumble", "Dominate"] {
        assert!(def(name).has_type(CardType::Instant), "{name}");
    }
    assert_eq!(def("Dominate").cost.x_count, 1);
    assert_eq!(def("Dominate").mana_value, 3);
}

// ---- Spells ---------------------------------------------------------------------

/// Casts Dominate at `target`, announcing `x`, and leaves it on the stack.
/// X starts at the target's mana value; the kernel announces it without a
/// choice when only that X is payable.
fn cast_dominate(state: &mut GameState, dominate: ObjectId, target: ObjectId, x: u16) {
    cast(state, dominate, &[Target::Object(target)]);
    let minimum = engine::object_mana_value(state, target);
    match next(state) {
        Decision::ChooseEffectOption { option_count, .. } => {
            let option = x - minimum;
            assert!(option < option_count, "X = {x} is not payable");
            act(state, Action::ChooseEffectOption(option));
            assert!(matches!(next(state), Decision::CastSpellOrPass { .. }));
        }
        Decision::CastSpellOrPass { .. } => assert_eq!(x, minimum, "the only payable X"),
        other => panic!("expected the X announcement, got {other:?}"),
    }
    assert_eq!(state.stack.len(), 1);
    assert_eq!(zone(state, dominate), Zone::Stack);
}

#[test]
fn dominate_steals_a_creature_with_mana_value_up_to_a_payable_x() {
    let mut state = islands();
    let mother = put(&mut state, P1, "Mother of Runes", Zone::Battlefield);
    let mongrel = put(&mut state, P1, "Wild Mongrel", Zone::Battlefield);
    let specter = put(&mut state, P1, "Hypnotic Specter", Zone::Battlefield);
    let dominate = put(&mut state, P0, "Dominate", Zone::Hand);
    // {1}{U}{U} and nothing for X: every creature costs at least 1.
    pool(&mut state, P0, &[ManaColor::U, ManaColor::U], 1);
    assert!(!castable(&mut state, dominate));
    // Two more pay X = 2: not the Specter.
    pool(&mut state, P0, &[], 2);
    let targets = cast_targets(&state, dominate);
    assert!(targets.contains(&Target::Object(mother)));
    assert!(targets.contains(&Target::Object(mongrel)));
    assert!(!targets.contains(&Target::Object(specter)));
    cast_dominate(&mut state, dominate, mongrel, 2);
    settled(&mut state);
    let stolen = state.objects.get(mongrel);
    assert_eq!(stolen.controller, P0);
    assert!(stolen.summoning_sick);
    assert!(state.players[0].battlefield.contains(&mongrel));
    assert!(!state.players[1].battlefield.contains(&mongrel));
    assert_eq!(state.players[0].mana_pool, [0; 6]);
    assert_eq!(zone(&state, dominate), Zone::Graveyard);
    // The control change has no duration.
    let turn = (state.turn, state.active_player);
    pass_until(&mut state, |state| {
        (state.turn, state.active_player) != turn && state.step == Step::Main1
    });
    assert_eq!(state.active_player, P1);
    assert_eq!(state.objects.get(mongrel).controller, P0);
}

#[test]
fn dominate_may_overpay_x_and_does_nothing_if_its_target_is_gone() {
    // A larger X is fine.
    let mut state = islands();
    let mother = put(&mut state, P1, "Mother of Runes", Zone::Battlefield);
    let dominate = put(&mut state, P0, "Dominate", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U, ManaColor::U], 4);
    cast_dominate(&mut state, dominate, mother, 3);
    settled(&mut state);
    assert_eq!(state.objects.get(mother).controller, P0);
    assert_eq!(state.players[0].mana_pool, [0; 6]);

    // A target that leaves first is illegal on resolution.
    let mut state = islands();
    let mongrel = put(&mut state, P1, "Wild Mongrel", Zone::Battlefield);
    let mother = put(&mut state, P1, "Mother of Runes", Zone::Battlefield);
    let dominate = put(&mut state, P0, "Dominate", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U, ManaColor::U], 3);
    cast_dominate(&mut state, dominate, mongrel, 2);
    bury(&mut state, mongrel);
    settled(&mut state);
    assert_eq!(zone(&state, mongrel), Zone::Graveyard);
    assert_eq!(state.objects.get(mother).controller, P1);
    assert_eq!(zone(&state, dominate), Zone::Graveyard);
}

#[test]
fn crumble_destroys_an_artifact_that_cant_regenerate_and_its_controller_gains_life() {
    let mut state = islands();
    let masticore = put(&mut state, P1, "Masticore", Zone::Battlefield);
    let ring = put(&mut state, P0, "Sol Ring", Zone::Battlefield);
    let mongrel = put(&mut state, P1, "Wild Mongrel", Zone::Battlefield);
    mtg_kernel::eternal_vintage_e7b_v1::statics::add_regeneration_shield(&mut state, masticore);
    let crumble = put(&mut state, P0, "Crumble", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 0);
    let targets = cast_targets(&state, crumble);
    assert!(targets.contains(&Target::Object(masticore)));
    assert!(targets.contains(&Target::Object(ring)), "yours too");
    assert!(!targets.contains(&Target::Object(mongrel)));
    cast(&mut state, crumble, &[Target::Object(masticore)]);
    settled(&mut state);
    assert_eq!(
        zone(&state, masticore),
        Zone::Graveyard,
        "can't be regenerated"
    );
    assert_eq!(life(&state, P1), 24, "its controller gains its mana value");
    assert_eq!(life(&state, P0), 20);

    // Your own artifact gives you the life.
    let crumble = put(&mut state, P0, "Crumble", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 0);
    cast(&mut state, crumble, &[Target::Object(ring)]);
    settled(&mut state);
    assert_eq!(zone(&state, ring), Zone::Graveyard);
    assert_eq!(life(&state, P0), 21);
}

#[test]
fn crumble_gives_life_for_an_indestructible_artifact_but_not_for_an_illegal_target() {
    // The One Ring survives, and its controller still gains 4.
    let mut state = islands();
    let one_ring = put(&mut state, P1, "The One Ring", Zone::Battlefield);
    let crypt = put(&mut state, P1, "Tormod's Crypt", Zone::Battlefield);
    let crumble = put(&mut state, P0, "Crumble", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 0);
    cast(&mut state, crumble, &[Target::Object(one_ring)]);
    settled(&mut state);
    assert_eq!(zone(&state, one_ring), Zone::Battlefield);
    assert_eq!(life(&state, P1), 24);

    // A mana value of 0 gives no life.
    let crumble = put(&mut state, P0, "Crumble", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 0);
    cast(&mut state, crumble, &[Target::Object(crypt)]);
    settled(&mut state);
    assert_eq!(zone(&state, crypt), Zone::Graveyard);
    assert_eq!(life(&state, P1), 24);

    // An artifact that left first gives none either.
    let mut state = islands();
    let ring = put(&mut state, P1, "Sol Ring", Zone::Battlefield);
    let crumble = put(&mut state, P0, "Crumble", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 0);
    cast(&mut state, crumble, &[Target::Object(ring)]);
    cast_finished(&mut state, crumble);
    bury(&mut state, ring);
    settled(&mut state);
    assert_eq!(life(&state, P1), 20);
    assert_eq!(zone(&state, crumble), Zone::Graveyard);
}

// ---- Creatures ------------------------------------------------------------------

#[test]
fn ihsans_shade_has_protection_from_white() {
    let mut state = islands();
    let shade = put(&mut state, P1, "Ihsan's Shade", Zone::Battlefield);
    let mongrel = put(&mut state, P1, "Wild Mongrel", Zone::Battlefield);
    let mother = put(&mut state, P0, "Mother of Runes", Zone::Battlefield);
    let specter = put(&mut state, P0, "Hypnotic Specter", Zone::Battlefield);
    assert_eq!(power_toughness(&state, shade), (5, 5));
    let target = Target::Object(shade);
    assert!(engine::damage_is_prevented_by_protection(
        &state, mother, target
    ));
    assert!(!engine::damage_is_prevented_by_protection(
        &state, specter, target
    ));
    let swords = put(&mut state, P0, "Swords to Plowshares", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W], 0);
    let targets = cast_targets(&state, swords);
    assert!(!targets.contains(&target), "white spells can't target it");
    assert!(targets.contains(&Target::Object(mongrel)));
}

#[test]
fn iridescent_angel_flies_with_protection_from_every_color() {
    let mut state = islands();
    let angel = put(&mut state, P1, "Iridescent Angel", Zone::Battlefield);
    assert_eq!(power_toughness(&state, angel), (4, 4));
    assert!(keyword(&state, angel, Keywords::FLYING));
    let target = Target::Object(angel);
    for name in [
        "Mother of Runes",
        "Palinchron",
        "Hypnotic Specter",
        "Goblin Piledriver",
        "Wild Mongrel",
    ] {
        let source = put(&mut state, P0, name, Zone::Battlefield);
        assert!(
            engine::damage_is_prevented_by_protection(&state, source, target),
            "{name}"
        );
    }
    let masticore = put(&mut state, P0, "Masticore", Zone::Battlefield);
    assert!(
        !engine::damage_is_prevented_by_protection(&state, masticore, target),
        "a colorless source"
    );
    let swords = put(&mut state, P0, "Swords to Plowshares", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W], 0);
    assert!(!cast_targets(&state, swords).contains(&target));
}

#[test]
fn multani_counts_the_cards_in_all_hands_and_has_shroud() {
    let mut state = islands();
    let multani = put(&mut state, P0, "Multani, Maro-Sorcerer", Zone::Battlefield);
    let mongrel = put(&mut state, P1, "Wild Mongrel", Zone::Battlefield);
    let swords = put(&mut state, P0, "Swords to Plowshares", Zone::Hand);
    assert_eq!(power_toughness(&state, multani), (1, 1), "the Swords");
    for _ in 0..3 {
        put(&mut state, P1, "Island", Zone::Hand);
    }
    put(&mut state, P0, "Island", Zone::Hand);
    assert_eq!(power_toughness(&state, multani), (5, 5));
    assert!(keyword(&state, multani, Keywords::SHROUD));
    pool(&mut state, P0, &[ManaColor::W], 0);
    let targets = cast_targets(&state, swords);
    assert!(
        !targets.contains(&Target::Object(multani)),
        "not even by its controller"
    );
    assert!(targets.contains(&Target::Object(mongrel)));
    // Casting the Swords leaves four cards in hands.
    cast(&mut state, swords, &[Target::Object(mongrel)]);
    cast_finished(&mut state, swords);
    assert_eq!(power_toughness(&state, multani), (4, 4));
    settled(&mut state);
    assert_eq!(zone(&state, mongrel), Zone::Exile);
}

#[test]
fn multani_with_no_cards_in_hands_dies_as_it_enters() {
    let mut state = islands();
    let multani = put(&mut state, P0, "Multani, Maro-Sorcerer", Zone::Hand);
    assert_eq!(
        power_toughness(&state, multani),
        (1, 1),
        "it counts itself in a hand"
    );
    pool(&mut state, P0, &[ManaColor::G, ManaColor::G], 4);
    cast(&mut state, multani, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, multani), Zone::Graveyard, "a 0/0");
}

#[test]
fn druid_lyrist_sacrifices_itself_to_destroy_an_enchantment() {
    let mut state = islands();
    let lyrist = put(&mut state, P0, "Druid Lyrist", Zone::Battlefield);
    let seal = put(&mut state, P1, "Seal of Fire", Zone::Battlefield);
    let ring = put(&mut state, P1, "Sol Ring", Zone::Battlefield);
    let mongrel = put(&mut state, P1, "Wild Mongrel", Zone::Battlefield);
    assert!(!activatable(&mut state, lyrist, 0), "needs {{G}}");
    pool(&mut state, P0, &[ManaColor::G], 0);
    state.objects.get_mut(lyrist).summoning_sick = true;
    assert!(!activatable(&mut state, lyrist, 0), "it taps");
    state.objects.get_mut(lyrist).summoning_sick = false;
    let targets = ability_targets(&state, lyrist, 0);
    assert_eq!(targets, vec![Target::Object(seal)]);
    activate(&mut state, lyrist, 0, &[Target::Object(seal)]);
    on_the_stack(&mut state);
    assert_eq!(
        zone(&state, lyrist),
        Zone::Graveyard,
        "sacrificed as a cost"
    );
    settled(&mut state);
    assert_eq!(zone(&state, seal), Zone::Graveyard);
    for kept in [ring, mongrel] {
        assert_eq!(zone(&state, kept), Zone::Battlefield);
    }
}

#[test]
fn spike_feeder_enters_with_two_counters_and_spends_them() {
    let mut state = islands();
    let mongrel = put(&mut state, P0, "Wild Mongrel", Zone::Battlefield);
    let feeder = put(&mut state, P0, "Spike Feeder", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G, ManaColor::G], 1);
    cast(&mut state, feeder, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, feeder), Zone::Battlefield);
    assert_eq!(state.objects.get(feeder).counters.plus1_plus1, 2);
    assert_eq!(power_toughness(&state, feeder), (2, 2));
    // "Remove a +1/+1 counter: You gain 2 life." needs no mana.
    activate(&mut state, feeder, 1, &[]);
    settled(&mut state);
    assert_eq!(life(&state, P0), 22);
    assert_eq!(power_toughness(&state, feeder), (1, 1));
    // "{2}, Remove a +1/+1 counter: Put a +1/+1 counter on target
    // creature." The last counter leaves a 0/0.
    assert!(!activatable(&mut state, feeder, 0), "needs {{2}}");
    pool(&mut state, P0, &[], 2);
    let targets = ability_targets(&state, feeder, 0);
    assert!(targets.contains(&Target::Object(mongrel)));
    activate(&mut state, feeder, 0, &[Target::Object(mongrel)]);
    settled(&mut state);
    assert_eq!(zone(&state, feeder), Zone::Graveyard);
    assert_eq!(power_toughness(&state, mongrel), (3, 3));
}

#[test]
fn uktabi_orangutan_destroys_an_artifact_as_it_enters() {
    let mut state = islands();
    let ring = put(&mut state, P1, "Sol Ring", Zone::Battlefield);
    let mongrel = put(&mut state, P1, "Wild Mongrel", Zone::Battlefield);
    let orangutan = put(&mut state, P0, "Uktabi Orangutan", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 2);
    cast(&mut state, orangutan, &[]);
    let mut asked = 0;
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseTargets { legal_targets, .. } => {
            asked += 1;
            assert_eq!(legal_targets, vec![Target::Object(ring)]);
            Action::ChooseTarget(Target::Object(ring))
        }
        other => panic!("unexpected choice: {other:?}"),
    });
    assert_eq!(asked, 1);
    assert_eq!(zone(&state, orangutan), Zone::Battlefield);
    assert_eq!(zone(&state, ring), Zone::Graveyard);
    assert_eq!(zone(&state, mongrel), Zone::Battlefield);

    // The target is not optional: with only your own artifact, it goes.
    let mut state = islands();
    let ring = put(&mut state, P0, "Sol Ring", Zone::Battlefield);
    let orangutan = put(&mut state, P0, "Uktabi Orangutan", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 2);
    cast(&mut state, orangutan, &[]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseTargets { legal_targets, .. } => {
            assert_eq!(legal_targets, vec![Target::Object(ring)]);
            Action::ChooseTarget(Target::Object(ring))
        }
        other => panic!("unexpected choice: {other:?}"),
    });
    assert_eq!(zone(&state, ring), Zone::Graveyard);

    // With no artifact, the trigger does nothing.
    let mut state = islands();
    let orangutan = put(&mut state, P0, "Uktabi Orangutan", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 2);
    cast(&mut state, orangutan, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, orangutan), Zone::Battlefield);
}
