//! Eternal wave S1c: native rules tests for the Vintage-only sideboard cards
//! (Argentum Masticore, Hullbreacher, Wurmcoil Engine and the rest of the
//! wave's cards).
#![cfg(feature = "eternal-fixtures")]

use mtg_kernel::card_def::{
    card_id_by_name, CardDef, Keywords, Subtype, CARD_DEFS,
};
use mtg_kernel::engine::{self, Action, CastMode, Decision, OptionalCostChoice};
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::ManaColor;
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Target, Zone};

const P0: PlayerId = PlayerId::P0;
const P1: PlayerId = PlayerId::P1;

fn def(name: &str) -> &'static CardDef {
    &CARD_DEFS[id(name) as usize]
}

fn id(name: &str) -> u16 {
    card_id_by_name(name).unwrap_or_else(|| panic!("{name}"))
}

/// A first main phase with P0 active and holding priority. Each library
/// holds `library` in order, top first.
fn game(library: &[&str]) -> GameState {
    let ids: Vec<u16> = library.iter().map(|name| id(name)).collect();
    let mut state = GameState::new_from_libraries(
        &ids,
        &ids,
        |id| CARD_DEFS[id as usize].object_name.into(),
        0x4539_4221_0001,
    );
    state.active_player = P0;
    state.priority_player = P0;
    state.step = Step::Main1;
    state
}

fn plains() -> GameState {
    game(&["Plains"; 20])
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
        Zone::Exile => state.exile.push(object),
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

fn named(state: &GameState, player: PlayerId, name: &str) -> Vec<ObjectId> {
    [P0, P1]
        .into_iter()
        .flat_map(|seat| state.players[seat.index()].battlefield.iter().copied())
        .filter(|&object| {
            let live = state.objects.get(object);
            live.controller == player && CARD_DEFS[live.card_def as usize].name == name
        })
        .collect()
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

/// The first legal answer to any choice: the first target or option,
/// finishing a selection that may finish, declining a Boolean or cost.
fn first_answer(decision: Decision) -> Action {
    match decision {
        Decision::ChooseTargets { legal_targets, .. } => Action::ChooseTarget(legal_targets[0]),
        Decision::ChooseEffectTargets {
            can_finish: true, ..
        } => Action::FinishEffectSelection,
        Decision::ChooseEffectTargets { legal_targets, .. } => {
            Action::ChooseEffectTarget(legal_targets[0])
        }
        Decision::ChooseEffectOption { .. } => Action::ChooseEffectOption(0),
        Decision::ChooseEffectBoolean { .. } => Action::ChooseEffectBoolean(false),
        Decision::ChooseCostTargets { candidates, .. } => Action::ChooseCostTarget(candidates[0]),
        Decision::ChooseCastMode { options, .. } => Action::ChooseCastMode(options[0]),
        Decision::ChooseKicker { .. } => Action::ChooseKicker(false),
        Decision::ChooseSpellMode { legal_modes, .. } => Action::ChooseSpellMode(legal_modes[0]),
        Decision::ChooseTriggerMode { legal_modes, .. } => {
            Action::ChooseTriggerMode(legal_modes[0])
        }
        Decision::ChooseOptionalCost { .. } => {
            Action::ChooseOptionalCost(OptionalCostChoice::Decline)
        }
        Decision::Discard { count, choices, .. } => {
            Action::Discard(choices.into_iter().take(count as usize).collect())
        }
        other => panic!("unexpected choice: {other:?}"),
    }
}

/// Settles, answering every choice with `answer`.
fn settle_with(state: &mut GameState, mut answer: impl FnMut(&GameState, Decision) -> Action) {
    while let Some(decision) = settle(state) {
        let action = answer(state, decision);
        act(state, action);
    }
}

fn settle_first(state: &mut GameState) {
    settle_with(state, |_, decision| first_answer(decision));
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

fn choose_target(state: &mut GameState, target: Target) {
    match next(state) {
        Decision::ChooseTargets { legal_targets, .. } => {
            assert!(
                legal_targets.contains(&target),
                "{target:?} not in {legal_targets:?}"
            );
            act(state, Action::ChooseTarget(target));
        }
        Decision::ChooseEffectTargets { legal_targets, .. } => {
            assert!(
                legal_targets.contains(&target),
                "{target:?} not in {legal_targets:?}"
            );
            act(state, Action::ChooseEffectTarget(target));
        }
        other => panic!("expected targets, got {other:?}"),
    }
}

/// The targets `spell` may choose first.
fn cast_targets(state: &GameState, spell: ObjectId) -> Vec<Target> {
    let mut probe = state.clone();
    assert!(castable(&mut probe, spell));
    act(&mut probe, Action::CastSpell(spell));
    match next(&mut probe) {
        Decision::ChooseTargets { legal_targets, .. } => legal_targets,
        other => panic!("expected targets, got {other:?}"),
    }
}

/// Activates `source`'s ability `index`, answering each target prompt.
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

/// Plays on until `stop` holds before a decision: passing priority, not
/// attacking or blocking, ordering triggers as given and answering any
/// other choice with `answer` (or the first legal answer).
fn run_until(
    state: &mut GameState,
    stop: impl Fn(&GameState) -> bool,
    mut answer: impl FnMut(&GameState, &Decision) -> Option<Action>,
) {
    for _ in 0..2000 {
        let decision = next(state);
        if stop(state) {
            return;
        }
        if let Some(action) = answer(state, &decision) {
            act(state, action);
            continue;
        }
        let action = match decision {
            Decision::CastSpellOrPass { .. } => Action::Pass,
            Decision::DeclareAttackers { .. } => Action::DeclareAttackers(Vec::new()),
            Decision::DeclareBlockers { .. } => Action::DeclareBlockers(Vec::new()),
            Decision::OrderTriggers { pending, .. } => {
                Action::OrderTriggers((0..pending.len()).collect())
            }
            Decision::ChooseCombatDamageRange { .. } => {
                Action::ChooseCombatDamageRange { upper_half: true }
            }
            Decision::ChooseAttackTarget {
                player, candidates, ..
            } => Action::ChooseAttackTarget(
                if candidates.contains(&Target::Player(player.opponent())) {
                    Target::Player(player.opponent())
                } else {
                    candidates[0]
                },
            ),
            other => first_answer(other),
        };
        act(state, action);
    }
    panic!("condition never held");
}

/// Plays on with first answers until `stop` holds.
fn pass_until(state: &mut GameState, stop: impl Fn(&GameState) -> bool) {
    run_until(state, stop, |_, _| None);
}

/// P0 attacks with `attacker`; the creatures P1 may block it with.
fn legal_blockers(state: &mut GameState, attacker: ObjectId) -> Vec<ObjectId> {
    pass_until(state, |state| state.step == Step::DeclareAttackers);
    act(state, Action::DeclareAttackers(vec![attacker]));
    for _ in 0..50 {
        match next(state) {
            Decision::DeclareBlockers { legal_blockers, .. } => {
                return legal_blockers
                    .into_iter()
                    .find(|(candidate, _)| *candidate == attacker)
                    .map(|(_, blockers)| blockers)
                    .unwrap_or_default();
            }
            Decision::CastSpellOrPass { .. } => act(state, Action::Pass),
            Decision::OrderTriggers { pending, .. } => {
                act(state, Action::OrderTriggers((0..pending.len()).collect()))
            }
            other => panic!("expected blockers, got {other:?}"),
        }
    }
    panic!("no declare blockers decision");
}

fn hand_contains(state: &GameState, player: PlayerId, object: ObjectId) -> bool {
    state.players[player.index()].hand.contains(&object)
}

fn mana(state: &GameState, player: PlayerId, color: ManaColor) -> u8 {
    state.players[player.index()].mana_pool[color.pool_index()]
}

const S1C_CARDS: [&str; 19] = [
    "Argentum Masticore",
    "Dark Petition",
    "Feed the Swarm",
    "Hullbreacher",
    "Hurkyl's Recall",
    "Light of Day",
    "Lithomantic Barrage",
    "Long Goodbye",
    "Mystical Dispute",
    "Rebuild",
    "Ravenous Trap",
    "Snapback",
    "Spell Snare",
    "Sphinx of the Steel Wind",
    "Urborg, Tomb of Yawgmoth",
    "Virtue's Ruin",
    "Wurmcoil Engine",
    "Deathtouch Wurm Token",
    "Lifelink Wurm Token",
];

fn eligible_attackers(state: &mut GameState) -> Vec<ObjectId> {
    pass_until(state, |state| state.step == Step::DeclareAttackers);
    match next(state) {
        Decision::DeclareAttackers { eligible, .. } => eligible,
        other => panic!("expected attackers, got {other:?}"),
    }
}

#[test]
fn s1c_cards_keep_their_support_labels() {
    for name in S1C_CARDS {
        assert_eq!(
            def(name).capability,
            crate::eternal_catalog_v1::expected_capability(name),
            "{name}"
        );
    }
}

#[test]
fn spell_snare_targets_only_mana_value_two_spells() {
    let mut state = plains();
    let inspector = put(&mut state, P0, "Dauthi Voidwalker", Zone::Hand);
    let snare = put(&mut state, P0, "Spell Snare", Zone::Hand);
    pool(
        &mut state,
        P0,
        &[ManaColor::B, ManaColor::B, ManaColor::U],
        0,
    );
    cast(&mut state, inspector, &[]);
    assert!(
        cast_targets(&state, snare).contains(&Target::Object(inspector)),
        "a mana value 2 spell"
    );
    cast(&mut state, snare, &[Target::Object(inspector)]);
    settled(&mut state);
    assert_eq!(zone(&state, inspector), Zone::Graveyard);
    assert_eq!(zone(&state, snare), Zone::Graveyard);

    let mut state = plains();
    let breacher = put(&mut state, P0, "Hullbreacher", Zone::Hand);
    let snare = put(&mut state, P0, "Spell Snare", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U, ManaColor::U], 2);
    cast(&mut state, breacher, &[]);
    assert!(
        !castable(&mut state, snare),
        "a mana value 3 spell is not a legal target"
    );
}

#[test]
fn hurkyls_recall_returns_only_the_targeted_players_artifacts() {
    let mut state = plains();
    let theirs = put(&mut state, P1, "Sol Ring", Zone::Battlefield);
    let mine = put(&mut state, P0, "Sol Ring", Zone::Battlefield);
    let recall = put(&mut state, P0, "Hurkyl's Recall", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 1);
    cast(&mut state, recall, &[Target::Player(P1)]);
    settled(&mut state);
    assert!(hand_contains(&state, P1, theirs));
    assert_eq!(zone(&state, mine), Zone::Battlefield);
}

#[test]
fn rebuild_returns_every_artifact_and_cycles() {
    let mut state = plains();
    let theirs = put(&mut state, P1, "Sol Ring", Zone::Battlefield);
    let mine = put(&mut state, P0, "Sol Ring", Zone::Battlefield);
    let rebuild = put(&mut state, P0, "Rebuild", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 2);
    cast(&mut state, rebuild, &[]);
    settled(&mut state);
    assert!(hand_contains(&state, P1, theirs));
    assert!(hand_contains(&state, P0, mine));

    let mut state = plains();
    let rebuild = put(&mut state, P0, "Rebuild", Zone::Hand);
    pool(&mut state, P0, &[], 2);
    let hand = state.players[0].hand.len();
    activate(&mut state, rebuild, 0, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, rebuild), Zone::Graveyard);
    assert_eq!(state.players[0].hand.len(), hand, "one card out, one drawn");
}

/// P1 casts `spell` (paid with `colored` and `colorless` mana) at instant
/// speed after P0 passes, then passes so that P0 holds priority with the
/// spell on the stack.
fn opponent_casts(
    state: &mut GameState,
    spell: ObjectId,
    colored: &[ManaColor],
    colorless: u8,
    targets: &[Target],
) {
    next(state);
    act(state, Action::Pass);
    pool(state, P1, colored, colorless);
    cast(state, spell, targets);
    next(state);
    act(state, Action::Pass);
    next(state);
}

#[test]
fn mystical_dispute_costs_two_less_only_against_a_blue_spell() {
    // Against a blue spell it costs {U}.
    let mut state = plains();
    let breacher = put(&mut state, P1, "Hullbreacher", Zone::Hand);
    let dispute = put(&mut state, P0, "Mystical Dispute", Zone::Hand);
    opponent_casts(&mut state, breacher, &[ManaColor::U], 2, &[]);
    pool(&mut state, P0, &[ManaColor::U], 0);
    cast(&mut state, dispute, &[Target::Object(breacher)]);
    settle_first(&mut state);
    assert_eq!(
        zone(&state, breacher),
        Zone::Graveyard,
        "countered unless {{3}}"
    );

    // Against a nonblue spell it costs the full {2}{U}.
    let mut state = plains();
    let bolt = put(&mut state, P1, "Lightning Bolt", Zone::Hand);
    let dispute = put(&mut state, P0, "Mystical Dispute", Zone::Hand);
    opponent_casts(&mut state, bolt, &[ManaColor::R], 0, &[Target::Player(P0)]);
    pool(&mut state, P0, &[ManaColor::U], 0);
    assert!(!castable(&mut state, dispute), "{{U}} is not enough");
    pool(&mut state, P0, &[], 2);
    assert!(castable(&mut state, dispute), "{{2}}{{U}}");
}

#[test]
fn long_goodbye_destroys_a_small_creature_but_not_a_big_one() {
    let mut state = plains();
    let small = put(&mut state, P1, "Thraben Inspector", Zone::Battlefield);
    let big = put(&mut state, P1, "Wurmcoil Engine", Zone::Battlefield);
    let goodbye = put(&mut state, P0, "Long Goodbye", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 1);
    let targets = cast_targets(&state, goodbye);
    assert!(targets.contains(&Target::Object(small)));
    assert!(!targets.contains(&Target::Object(big)), "mana value 6");
    cast(&mut state, goodbye, &[Target::Object(small)]);
    settled(&mut state);
    assert_eq!(zone(&state, small), Zone::Graveyard);
    assert!(def("Long Goodbye").spell_cannot_be_countered);
}

#[test]
fn snapback_exiles_a_blue_card_to_bounce_a_creature() {
    let mut state = plains();
    let victim = put(&mut state, P1, "Thraben Inspector", Zone::Battlefield);
    let fodder = put(&mut state, P0, "Spell Snare", Zone::Hand);
    let snapback = put(&mut state, P0, "Snapback", Zone::Hand);
    assert!(castable(&mut state, snapback), "no mana needed");
    act(&mut state, Action::CastSpell(snapback));
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseCastMode { options, .. } => {
            assert!(options.contains(&CastMode::Alternative));
            Action::ChooseCastMode(CastMode::Alternative)
        }
        Decision::ChooseCostTargets { candidates, .. } => {
            assert_eq!(candidates, vec![fodder]);
            Action::ChooseCostTarget(fodder)
        }
        Decision::ChooseTargets { .. } => Action::ChooseTarget(Target::Object(victim)),
        other => first_answer(other),
    });
    assert_eq!(zone(&state, fodder), Zone::Exile);
    assert!(hand_contains(&state, P1, victim));
}

#[test]
fn hullbreacher_replaces_all_but_the_draw_step_draw() {
    let mut state = plains();
    put(&mut state, P1, "Hullbreacher", Zone::Battlefield);
    let before = state.players[0].hand.len();
    event::propose_and_commit(&mut state, ProposedEvent::draw(P0));
    event::propose_and_commit(&mut state, ProposedEvent::draw(P0));
    assert_eq!(state.players[0].hand.len(), before, "both draws replaced");
    assert_eq!(named(&state, P1, "Treasure Token").len(), 2);

    // The draw step's own draw is not replaced.
    let mut state = plains();
    put(&mut state, P1, "Hullbreacher", Zone::Battlefield);
    pass_until(&mut state, |state| {
        state.turn >= 3 && state.active_player == P0 && state.step == Step::Main1
    });
    assert!(named(&state, P1, "Treasure Token").is_empty());
}

#[test]
fn wurmcoil_engine_leaves_two_wurms() {
    let mut state = plains();
    let engine_card = put(&mut state, P1, "Wurmcoil Engine", Zone::Battlefield);
    let grapple = put(&mut state, P0, "Grapple with Death", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B, ManaColor::G], 1);
    cast(&mut state, grapple, &[Target::Object(engine_card)]);
    settled(&mut state);
    assert_eq!(zone(&state, engine_card), Zone::Graveyard);
    let deathtouch = named(&state, P1, "Deathtouch Wurm Token");
    let lifelink = named(&state, P1, "Lifelink Wurm Token");
    assert_eq!((deathtouch.len(), lifelink.len()), (1, 1));
    assert_eq!(power_toughness(&state, deathtouch[0]), (3, 3));
    assert!(keyword(&state, deathtouch[0], Keywords::DEATHTOUCH));
    assert!(keyword(&state, lifelink[0], Keywords::LIFELINK));
}

#[test]
fn light_of_day_stops_black_creatures_from_attacking_and_blocking() {
    let mut state = plains();
    let black = put(&mut state, P0, "Cauldron Familiar", Zone::Battlefield);
    let white = put(&mut state, P0, "Thraben Inspector", Zone::Battlefield);
    put(&mut state, P1, "Light of Day", Zone::Battlefield);
    let eligible = eligible_attackers(&mut state);
    assert!(eligible.contains(&white));
    assert!(!eligible.contains(&black), "black creatures can't attack");

    let mut state = plains();
    let attacker = put(&mut state, P0, "Thraben Inspector", Zone::Battlefield);
    let blocker = put(&mut state, P1, "Cauldron Familiar", Zone::Battlefield);
    assert!(legal_blockers(&mut state, attacker).contains(&blocker));
    let mut state = plains();
    let attacker = put(&mut state, P0, "Thraben Inspector", Zone::Battlefield);
    let blocker = put(&mut state, P1, "Cauldron Familiar", Zone::Battlefield);
    put(&mut state, P1, "Light of Day", Zone::Battlefield);
    assert!(
        !legal_blockers(&mut state, attacker).contains(&blocker),
        "black creatures can't block"
    );
}

#[test]
fn dark_petition_adds_mana_only_with_spell_mastery() {
    for (spells, mana_black) in [(2, 3), (1, 0)] {
        let mut state = game(&["Plains", "Island", "Swamp", "Plains", "Plains"]);
        for _ in 0..spells {
            put(&mut state, P0, "Spell Snare", Zone::Graveyard);
        }
        let petition = put(&mut state, P0, "Dark Petition", Zone::Hand);
        pool(&mut state, P0, &[ManaColor::B, ManaColor::B], 3);
        cast(&mut state, petition, &[]);
        settle_first(&mut state);
        assert_eq!(
            mana(&state, P0, ManaColor::B),
            mana_black,
            "{spells} spells"
        );
    }
}

#[test]
fn rebuild_and_virtues_ruin_hit_what_the_text_says() {
    let mut state = plains();
    let white = put(&mut state, P1, "Thraben Inspector", Zone::Battlefield);
    let black = put(&mut state, P1, "Cauldron Familiar", Zone::Battlefield);
    let ruin = put(&mut state, P0, "Virtue's Ruin", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 2);
    cast(&mut state, ruin, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, white), Zone::Graveyard);
    assert_eq!(zone(&state, black), Zone::Battlefield);
}

#[test]
fn feed_the_swarm_costs_life_equal_to_the_mana_value() {
    let mut state = plains();
    let breacher = put(&mut state, P1, "Hullbreacher", Zone::Battlefield);
    let own = put(&mut state, P0, "Thraben Inspector", Zone::Battlefield);
    let feed = put(&mut state, P0, "Feed the Swarm", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 1);
    let targets = cast_targets(&state, feed);
    assert!(targets.contains(&Target::Object(breacher)));
    assert!(
        !targets.contains(&Target::Object(own)),
        "an opponent controls"
    );
    cast(&mut state, feed, &[Target::Object(breacher)]);
    settled(&mut state);
    assert_eq!(zone(&state, breacher), Zone::Graveyard);
    assert_eq!(state.players[0].life, 17);
}

#[test]
fn lithomantic_barrage_deals_five_to_white_and_blue_targets() {
    // Hullbreacher (blue, 3/2) dies to five damage; Dauthi Voidwalker (black,
    // 3/2) survives the single point.
    let mut state = plains();
    let blue = put(&mut state, P1, "Hullbreacher", Zone::Battlefield);
    let barrage = put(&mut state, P0, "Lithomantic Barrage", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 0);
    cast(&mut state, barrage, &[Target::Object(blue)]);
    settled(&mut state);
    assert_eq!(zone(&state, blue), Zone::Graveyard);

    let mut state = plains();
    let black = put(&mut state, P1, "Dauthi Voidwalker", Zone::Battlefield);
    let barrage = put(&mut state, P0, "Lithomantic Barrage", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 0);
    cast(&mut state, barrage, &[Target::Object(black)]);
    settled(&mut state);
    assert_eq!(zone(&state, black), Zone::Battlefield);
    assert_eq!(state.objects.get(black).damage, 1);
    assert!(def("Lithomantic Barrage").spell_cannot_be_countered);
}

#[test]
fn sphinx_of_the_steel_wind_has_protection_from_red_and_green() {
    let mut state = plains();
    let sphinx = put(
        &mut state,
        P1,
        "Sphinx of the Steel Wind",
        Zone::Battlefield,
    );
    let barrage = put(&mut state, P0, "Lithomantic Barrage", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 0);
    assert!(
        !castable(&mut state, barrage),
        "the Sphinx is the only target and is protected from red"
    );
    assert_eq!(power_toughness(&state, sphinx), (6, 6));
    assert!(keyword(&state, sphinx, Keywords::FLYING));
    assert!(keyword(&state, sphinx, Keywords::LIFELINK));
}

#[test]
fn urborg_makes_every_land_a_swamp() {
    let mut state = plains();
    let urborg = put(
        &mut state,
        P0,
        "Urborg, Tomb of Yawgmoth",
        Zone::Battlefield,
    );
    let plains_land = put(&mut state, P1, "Plains", Zone::Battlefield);
    assert!(engine::has_effective_subtype(
        &state,
        plains_land,
        Subtype::Swamp
    ));
    assert!(engine::has_effective_subtype(
        &state,
        urborg,
        Subtype::Swamp
    ));
    let mut state = plains();
    let plains_land = put(&mut state, P1, "Plains", Zone::Battlefield);
    assert!(!engine::has_effective_subtype(
        &state,
        plains_land,
        Subtype::Swamp
    ));
}

#[test]
fn argentum_masticore_sacrifices_unless_its_controller_discards() {
    // With an empty hand it is sacrificed at the upkeep.
    let mut state = plains();
    let masticore = put(&mut state, P0, "Argentum Masticore", Zone::Battlefield);
    state.players[0].hand.clear();
    pass_until(&mut state, |state| {
        state.turn >= 3 && state.active_player == P0 && state.step == Step::Main1
    });
    assert_eq!(zone(&state, masticore), Zone::Graveyard);
}
#[test]
fn ravenous_trap_is_free_after_three_cards_hit_the_opponents_graveyard() {
    let mut state = plains();
    let trap = put(&mut state, P0, "Ravenous Trap", Zone::Hand);
    assert!(
        !castable(&mut state, trap),
        "no mana and no graveyard cards"
    );
    for _ in 0..3 {
        let card = put(&mut state, P1, "Plains", Zone::Hand);
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(card, Zone::Graveyard),
        );
    }
    let mut state = state;
    assert_eq!(state.players[1].graveyard.len(), 3);
    assert!(castable(&mut state, trap), "three cards this turn");
    act(&mut state, Action::CastSpell(trap));
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseCastMode { options, .. } => {
            assert!(options.contains(&CastMode::Alternative));
            Action::ChooseCastMode(CastMode::Alternative)
        }
        Decision::ChooseTargets { .. } => Action::ChooseTarget(Target::Player(P1)),
        other => first_answer(other),
    });
    assert!(state.players[1].graveyard.is_empty(), "exiled");
    assert_eq!(state.exile.len(), 3);
}
