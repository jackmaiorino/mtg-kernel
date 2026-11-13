//! Vintage sideboard wave S1e: native rules tests for the cards that
//! complete Jewel Shops, Oath of Druids, Dredge, Doomsday and Underworld
//! Breach.
#![cfg(feature = "eternal-fixtures")]

use mtg_kernel::card_def::{
    card_id_by_name, CardCapability, CardDef, CardType, Keywords, Subtype, CARD_DEFS,
};
use mtg_kernel::engine::{self, Action, CastMode, Decision};
use mtg_kernel::eternal_sideboard_s1e_v1::read_state;
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::ManaColor;
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Target, Zone};

const P0: PlayerId = PlayerId::P0;
const P1: PlayerId = PlayerId::P1;

const BEASTBINDER: &str = "Azure Beastbinder";
const SUSPEND: &str = "Suspend";
const SHOAL: &str = "Sickening Shoal";
const SCREAM: &str = "Unable to Scream";
const PORTAL: &str = "Portal to Phyrexia";
const S1E_CARDS: [&str; 5] = [BEASTBINDER, SUSPEND, SHOAL, SCREAM, PORTAL];

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
        0x5331_4500_0001,
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

/// A permanent that entered from `player`'s hand, so it has what entering
/// gives it (a planeswalker's loyalty).
fn entered(state: &mut GameState, player: PlayerId, name: &str) -> ObjectId {
    let object = put(state, player, name, Zone::Hand);
    event::propose_and_commit(state, ProposedEvent::zone_change(object, Zone::Battlefield));
    settled(state);
    assert_eq!(zone(state, object), Zone::Battlefield);
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

fn power_toughness(state: &GameState, object: ObjectId) -> (i32, i32) {
    (
        engine::effective_power(state, object),
        engine::effective_toughness(state, object),
    )
}

fn has_type(state: &GameState, object: ObjectId, card_type: CardType) -> bool {
    engine::object_has_type(state, object, card_type)
}

fn keyword(state: &GameState, object: ObjectId, keyword: Keywords) -> bool {
    engine::has_effective_keyword(state, object, keyword)
}

fn subtype(state: &GameState, object: ObjectId, subtype: Subtype) -> bool {
    engine::has_effective_subtype(state, object, subtype)
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

/// The first legal answer to a choice.
fn first_answer(decision: Decision) -> Action {
    match decision {
        Decision::ChooseTargets {
            can_finish: true,
            ref legal_targets,
            ..
        } if legal_targets.is_empty() => Action::FinishEffectSelection,
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
        Decision::Discard { count, choices, .. } => {
            Action::Discard(choices.into_iter().take(count as usize).collect())
        }
        other => panic!("unexpected choice: {other:?}"),
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

/// Whether the player holding priority may activate `source`'s ability
/// `index` (255 turns a face-down permanent face up).
fn activatable(state: &mut GameState, source: ObjectId, index: u8) -> bool {
    priority(state).1.contains(&(source, index))
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

/// Plays on until `stop` holds before a decision, answering it with
/// `answer` when that gives an action and otherwise passing priority, not
/// attacking or blocking, keeping triggers in order and taking first
/// answers.
fn run_until(
    state: &mut GameState,
    stop: impl Fn(&GameState) -> bool,
    mut answer: impl FnMut(&GameState, &Decision) -> Option<Action>,
) {
    for _ in 0..3000 {
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
            other => first_answer(other),
        };
        act(state, action);
    }
    panic!("condition never held");
}

fn pass_until(state: &mut GameState, stop: impl Fn(&GameState) -> bool) {
    run_until(state, stop, |_, _| None);
}

/// Plays on to P0's next first main phase.
fn to_next_own_main(state: &mut GameState) {
    pass_until(state, |state| state.active_player == P1);
    pass_until(state, |state| {
        state.active_player == P0 && state.step == Step::Main1
    });
}

/// P0 declares `attackers` against P1 in this turn's combat.
fn attack(state: &mut GameState, attackers: Vec<ObjectId>) {
    pass_until(state, |state| state.step == Step::DeclareAttackers);
    act(state, Action::DeclareAttackers(attackers));
    if let Decision::ChooseAttackTarget { .. } = next(state) {
        act(state, Action::ChooseAttackTarget(Target::Player(P1)));
    }
}

/// Answers Azure Beastbinder's attack trigger with `target`, or with none.
fn bind_target(state: &mut GameState, binder: ObjectId, target: Option<ObjectId>) -> Vec<Target> {
    match next(state) {
        Decision::ChooseTargets {
            player,
            spell,
            legal_targets,
            can_finish,
            ..
        } => {
            assert_eq!((player, spell), (P0, binder));
            assert!(can_finish, "up to one target");
            act(
                state,
                target.map_or(Action::FinishEffectSelection, |target| {
                    Action::ChooseTarget(Target::Object(target))
                }),
            );
            legal_targets
        }
        other => panic!("expected the attack trigger's target, got {other:?}"),
    }
}

// ---- Characteristics -----------------------------------------------------------

#[test]
fn s1e_cards_are_full_with_their_printed_characteristics() {
    for name in S1E_CARDS {
        assert_eq!(def(name).capability, CardCapability::Full, "{name}");
        assert!(!def(name).is_token, "{name}");
    }
    let binder = def(BEASTBINDER);
    assert!(binder.has_type(CardType::Creature));
    assert_eq!((binder.mana_value, binder.colors), (2, &[ManaColor::U][..]));
    assert_eq!(binder.subtypes, &[Subtype::Rat, Subtype::Rogue][..]);
    assert_eq!((binder.power, binder.toughness), (Some(1), Some(3)));
    assert!(binder.keywords.has(Keywords::VIGILANCE));
    let suspend = def(SUSPEND);
    assert!(suspend.has_type(CardType::Instant));
    assert_eq!(
        (suspend.mana_value, suspend.colors),
        (1, &[ManaColor::U][..])
    );
    let shoal = def(SHOAL);
    assert!(shoal.has_type(CardType::Instant));
    assert_eq!((shoal.mana_value, shoal.colors), (2, &[ManaColor::B][..]));
    assert_eq!(shoal.subtypes, &[Subtype::Arcane][..]);
    let scream = def(SCREAM);
    assert!(scream.has_type(CardType::Enchantment));
    assert_eq!((scream.mana_value, scream.colors), (1, &[ManaColor::U][..]));
    assert_eq!(scream.subtypes, &[Subtype::Aura][..]);
    let portal = def(PORTAL);
    assert!(portal.has_type(CardType::Artifact));
    assert_eq!((portal.mana_value, portal.colors), (9, &[][..]));
}

// ---- Azure Beastbinder ----------------------------------------------------------

#[test]
fn azure_beastbinder_has_vigilance_and_evades_power_two_or_greater() {
    let mut state = islands();
    let binder = put(&mut state, P0, BEASTBINDER, Zone::Battlefield);
    let elves = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    let angler = put(&mut state, P1, "Gurmag Angler", Zone::Battlefield);
    attack(&mut state, vec![binder]);
    // "Up to one target": none is chosen.
    bind_target(&mut state, binder, None);
    assert!(!state.objects.get(binder).tapped, "vigilance");
    let legal_blockers = loop {
        match next(&mut state) {
            Decision::DeclareBlockers {
                player,
                legal_blockers,
                ..
            } => {
                assert_eq!(player, P1);
                break legal_blockers;
            }
            Decision::CastSpellOrPass { .. } => act(&mut state, Action::Pass),
            other => panic!("unexpected choice: {other:?}"),
        }
    };
    assert_eq!(
        legal_blockers,
        vec![(binder, vec![elves])],
        "the Angler's power is 5"
    );
    act(&mut state, Action::DeclareBlockers(vec![(elves, binder)]));
    pass_until(&mut state, |state| state.step == Step::Main2);
    assert_eq!(zone(&state, elves), Zone::Graveyard);
    assert_eq!(zone(&state, angler), Zone::Battlefield);
    assert_eq!(state.objects.get(binder).damage, 1);
    assert!(read_state(&state).is_none_or(|s1e| s1e.bound.is_empty()));
}

#[test]
fn azure_beastbinder_strips_its_target_until_its_controllers_next_turn() {
    let mut state = islands();
    let binder = put(&mut state, P0, BEASTBINDER, Zone::Battlefield);
    let mine = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    let griselbrand = put(&mut state, P1, "Griselbrand", Zone::Battlefield);
    state.objects.get_mut(griselbrand).counters.plus1_plus1 = 1;
    let ring = put(&mut state, P1, "Sol Ring", Zone::Battlefield);
    let island = put(&mut state, P1, "Island", Zone::Battlefield);
    assert_eq!(power_toughness(&state, griselbrand), (8, 8));

    attack(&mut state, vec![binder]);
    let legal = bind_target(&mut state, binder, Some(griselbrand));
    for object in [griselbrand, ring] {
        assert!(legal.contains(&Target::Object(object)), "{object:?}");
    }
    for object in [island, mine, binder] {
        assert!(!legal.contains(&Target::Object(object)), "{object:?}");
    }
    assert!(!legal.contains(&Target::Player(P1)));
    settled(&mut state);

    // Base 2/2 under its +1/+1 counter, without flying or lifelink.
    let mut copy = restored(&state);
    assert_eq!(state.state_hash(), copy.state_hash());
    for continued in [&mut state, &mut copy] {
        assert_eq!(power_toughness(continued, griselbrand), (3, 3));
        assert!(!keyword(continued, griselbrand, Keywords::FLYING));
        assert!(!keyword(continued, griselbrand, Keywords::LIFELINK));
        let bound = &read_state(continued).unwrap().bound;
        assert_eq!(bound.len(), 1);
        assert!(bound[0].creature);
    }
    pass_until(&mut state, |state| state.step == Step::Main2);
    assert_eq!(state.players[1].life, 19, "unblocked");

    // Through the opponent's turn: its activated ability is gone too.
    pass_until(&mut state, |state| {
        state.active_player == P1 && state.step == Step::Main1
    });
    assert_eq!(power_toughness(&state, griselbrand), (3, 3));
    assert!(!activatable(&mut state, griselbrand, 0));

    // It ends as P0's next turn begins.
    pass_until(&mut state, |state| {
        state.active_player == P0 && state.step == Step::Main1
    });
    assert_eq!(power_toughness(&state, griselbrand), (8, 8));
    assert!(keyword(&state, griselbrand, Keywords::FLYING));
    assert!(read_state(&state).unwrap().bound.is_empty());
    pass_until(&mut state, |state| {
        state.active_player == P1 && state.step == Step::Main1
    });
    assert!(activatable(&mut state, griselbrand, 0));
}

#[test]
fn azure_beastbinder_turns_off_a_planeswalkers_loyalty_abilities() {
    let mut state = islands();
    let oko = entered(&mut state, P1, "Oko, Thief of Crowns");
    let binder = put(&mut state, P0, BEASTBINDER, Zone::Battlefield);
    attack(&mut state, vec![binder]);
    let legal = bind_target(&mut state, binder, Some(oko));
    assert!(legal.contains(&Target::Object(oko)));
    settled(&mut state);
    let bound = &read_state(&state).unwrap().bound;
    assert_eq!(bound.len(), 1);
    assert!(!bound[0].creature, "a planeswalker gets no base 2/2");
    assert!(!has_type(&state, oko, CardType::Creature));
    pass_until(&mut state, |state| {
        state.active_player == P1 && state.step == Step::Main1
    });
    assert_eq!(zone(&state, oko), Zone::Battlefield);
    let (_, abilities) = priority(&mut state);
    assert!(
        abilities.iter().all(|&(source, _)| source != oko),
        "{abilities:?}"
    );
}

// ---- Suspend --------------------------------------------------------------------

#[test]
fn suspend_exiles_a_creature_its_owner_casts_with_haste_two_upkeeps_later() {
    let mut state = islands();
    let elves = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    let suspend = put(&mut state, P0, SUSPEND, Zone::Hand);
    let dominate = put(&mut state, P0, "Dominate", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 0);
    cast(&mut state, suspend, &[Target::Object(elves)]);
    settled(&mut state);
    assert_eq!(zone(&state, elves), Zone::Exile);
    assert_eq!(state.objects.get(elves).v4.time_counters_v1, 2);

    // Its owner's upkeep removes one; its opponent's doesn't.
    pass_until(&mut state, |state| {
        state.active_player == P1 && state.step == Step::Main1
    });
    assert_eq!(state.objects.get(elves).v4.time_counters_v1, 1);
    let mut copy = restored(&state);
    assert_eq!(state.state_hash(), copy.state_hash());
    to_next_own_main(&mut copy);
    assert_eq!(copy.objects.get(elves).v4.time_counters_v1, 1);

    // The last one leaves at the next: its owner casts it without paying
    // its mana cost, and it has haste.
    let mut offered = false;
    run_until(
        &mut copy,
        |state| state.active_player == P1 && state.step == Step::Main1,
        |state, decision| match decision {
            Decision::ChooseEffectOption {
                player,
                option_count,
                ..
            } if state.active_player == P1 && state.step == Step::Upkeep => {
                assert_eq!((*player, *option_count), (P1, 2));
                offered = true;
                Some(Action::ChooseEffectOption(0))
            }
            _ => None,
        },
    );
    assert!(offered);
    let mut state = copy;
    assert_eq!(zone(&state, elves), Zone::Battlefield);
    assert_eq!(state.objects.get(elves).controller, P1);
    assert!(keyword(&state, elves, Keywords::HASTE));
    assert_eq!(read_state(&state).unwrap().hasty_permanents.len(), 1);
    let mut eligible_attackers = Vec::new();
    run_until(
        &mut state,
        |state| state.step == Step::Main2,
        |_, decision| match decision {
            Decision::DeclareAttackers { player, eligible } => {
                assert_eq!(*player, P1);
                eligible_attackers = eligible.clone();
                Some(Action::DeclareAttackers(Vec::new()))
            }
            _ => None,
        },
    );
    assert!(eligible_attackers.contains(&elves), "haste");

    // "Until you lose control of ... the permanent it becomes."
    pass_until(&mut state, |state| {
        state.step == Step::Main2 && state.priority_player == P0 && state.stack.is_empty()
    });
    pool(&mut state, P0, &[ManaColor::U, ManaColor::U], 2);
    assert!(castable(&mut state, dominate));
    act(&mut state, Action::CastSpell(dominate));
    settle_with(&mut state, |_, decision| match decision {
        // X = 1, the most the pool pays.
        Decision::ChooseEffectOption { option_count, .. } => {
            Action::ChooseEffectOption(option_count - 1)
        }
        Decision::ChooseTargets { .. } => Action::ChooseTarget(Target::Object(elves)),
        other => panic!("unexpected choice: {other:?}"),
    });
    assert_eq!(state.objects.get(elves).controller, P0);
    assert!(!keyword(&state, elves, Keywords::HASTE));
    assert!(read_state(&state).unwrap().hasty_permanents.is_empty());
}

// ---- Sickening Shoal -------------------------------------------------------------

#[test]
fn sickening_shoal_exiles_a_black_card_whose_mana_value_is_x() {
    let mut state = islands();
    let angler = put(&mut state, P1, "Gurmag Angler", Zone::Battlefield);
    let first = put(&mut state, P0, SHOAL, Zone::Hand);
    let second = put(&mut state, P0, SHOAL, Zone::Hand);
    let ritual = put(&mut state, P0, "Dark Ritual", Zone::Hand);
    let griselbrand = put(&mut state, P0, "Griselbrand", Zone::Hand);
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);

    // Without mana: the alternative cost only, so no mode is asked. The
    // exiled Dark Ritual makes X 1, and no X is asked.
    let cast_shoal = |state: &mut GameState, shoal: ObjectId, exiled: ObjectId| {
        assert!(castable(state, shoal));
        act(state, Action::CastSpell(shoal));
        settle_with(state, |_, decision| match decision {
            Decision::ChooseCastMode { options, .. } => {
                assert!(options.contains(&CastMode::Alternative));
                Action::ChooseCastMode(CastMode::Alternative)
            }
            Decision::ChooseCostTargets { candidates, .. } => {
                assert!(candidates.contains(&exiled));
                assert!(!candidates.contains(&bolt), "a red card");
                assert!(!candidates.contains(&shoal), "itself");
                Action::ChooseCostTarget(exiled)
            }
            Decision::ChooseTargets { .. } => Action::ChooseTarget(Target::Object(angler)),
            other => panic!("unexpected choice: {other:?}"),
        });
    };
    cast_shoal(&mut state, first, ritual);
    assert_eq!(zone(&state, ritual), Zone::Exile);
    assert_eq!(zone(&state, first), Zone::Graveyard);
    assert_eq!(power_toughness(&state, angler), (4, 4));
    let mut copy = restored(&state);
    assert_eq!(state.state_hash(), copy.state_hash());
    pass_until(&mut copy, |state| state.active_player == P1);
    assert_eq!(power_toughness(&copy, angler), (5, 5), "until end of turn");

    // Griselbrand's mana value is 8.
    cast_shoal(&mut state, second, griselbrand);
    assert_eq!(zone(&state, griselbrand), Zone::Exile);
    assert_eq!(zone(&state, angler), Zone::Graveyard);
}

#[test]
fn sickening_shoal_can_be_cast_for_x_and_two_black_mana() {
    let mut state = islands();
    let angler = put(&mut state, P1, "Gurmag Angler", Zone::Battlefield);
    let shoal = put(&mut state, P0, SHOAL, Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B, ManaColor::B], 3);
    assert!(castable(&mut state, shoal));
    act(&mut state, Action::CastSpell(shoal));
    let mut x_asked = false;
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseEffectOption { option_count, .. } => {
            assert_eq!(option_count, 4, "X from 0 to 3");
            x_asked = true;
            Action::ChooseEffectOption(3)
        }
        Decision::ChooseTargets { .. } => Action::ChooseTarget(Target::Object(angler)),
        other => panic!("unexpected choice: {other:?}"),
    });
    assert!(x_asked);
    assert_eq!(power_toughness(&state, angler), (2, 2));
    assert_eq!(state.players[0].mana_pool.iter().sum::<u8>(), 0);
}

// ---- Unable to Scream ------------------------------------------------------------

#[test]
fn unable_to_scream_makes_its_host_a_0_2_toy_artifact_creature_without_abilities() {
    let mut state = islands();
    let griselbrand = put(&mut state, P1, "Griselbrand", Zone::Battlefield);
    let scream = put(&mut state, P0, SCREAM, Zone::Hand);
    let binder = put(&mut state, P0, BEASTBINDER, Zone::Battlefield);
    pool(&mut state, P0, &[ManaColor::U], 0);
    cast(&mut state, scream, &[Target::Object(griselbrand)]);
    settled(&mut state);
    assert_eq!(zone(&state, scream), Zone::Battlefield);
    let mut copy = restored(&state);
    assert_eq!(state.state_hash(), copy.state_hash());
    for continued in [&mut state, &mut copy] {
        assert!(has_type(continued, griselbrand, CardType::Artifact));
        assert!(has_type(continued, griselbrand, CardType::Creature));
        assert!(subtype(continued, griselbrand, Subtype::Toy));
        assert!(
            subtype(continued, griselbrand, Subtype::Demon),
            "in addition"
        );
        assert_eq!(power_toughness(continued, griselbrand), (0, 2));
        assert!(!keyword(continued, griselbrand, Keywords::FLYING));
        assert!(!keyword(continued, griselbrand, Keywords::LIFELINK));
    }

    // A later base 2/2 wins while it lasts (613.7): Azure Beastbinder's
    // trigger, until P0's next turn.
    attack(&mut state, vec![binder]);
    bind_target(&mut state, binder, Some(griselbrand));
    settled(&mut state);
    assert_eq!(power_toughness(&state, griselbrand), (2, 2));
    pass_until(&mut state, |state| {
        state.active_player == P1 && state.step == Step::Main1
    });
    assert_eq!(power_toughness(&state, griselbrand), (2, 2));
    assert!(!activatable(&mut state, griselbrand, 0));
    pass_until(&mut state, |state| {
        state.active_player == P0 && state.step == Step::Main1
    });
    assert_eq!(power_toughness(&state, griselbrand), (0, 2));
    assert!(has_type(&state, griselbrand, CardType::Artifact));
}

#[test]
fn unable_to_scream_keeps_a_face_down_creature_face_down() {
    let mut state = islands();
    // Abhorrent Oculus manifests dread from the top two, both creature
    // cards, at P1's upkeep.
    for _ in 0..2 {
        put(&mut state, P0, "Llanowar Elves", Zone::Library);
    }
    for _ in 0..6 {
        put(&mut state, P0, "Island", Zone::Graveyard);
    }
    let oculus = put(&mut state, P0, "Abhorrent Oculus", Zone::Hand);
    let scream = put(&mut state, P0, SCREAM, Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 2);
    cast(&mut state, oculus, &[]);
    settle_with(&mut state, |_, decision| first_answer(decision));
    assert_eq!(zone(&state, oculus), Zone::Battlefield);
    pass_until(&mut state, |state| {
        state.active_player == P1 && state.step == Step::Main1
    });
    let face_down: Vec<ObjectId> = state.players[0]
        .battlefield
        .iter()
        .copied()
        .filter(|&object| state.objects.get(object).v4.face_down_v1.is_some())
        .collect();
    assert_eq!(face_down.len(), 1, "manifested one");
    let face_down = face_down[0];
    assert_eq!(state.objects.get(face_down).card_def, id("Llanowar Elves"));

    // A manifested creature card turns face up for its mana cost...
    pass_until(&mut state, |state| {
        state.active_player == P0 && state.step == Step::Main1
    });
    pool(&mut state, P0, &[ManaColor::G, ManaColor::U], 0);
    assert!(activatable(&mut state, face_down, 255));

    // ...but not while Unable to Scream enchants it.
    cast(&mut state, scream, &[Target::Object(face_down)]);
    settled(&mut state);
    assert_eq!(zone(&state, scream), Zone::Battlefield);
    assert!(!activatable(&mut state, face_down, 255));
    assert!(state.objects.get(face_down).v4.face_down_v1.is_some());
    assert!(has_type(&state, face_down, CardType::Artifact));
    assert!(has_type(&state, face_down, CardType::Creature));
    assert!(subtype(&state, face_down, Subtype::Toy));
    assert!(!subtype(&state, face_down, Subtype::Elf), "face down");
    assert_eq!(power_toughness(&state, face_down), (0, 2));
}

// ---- Portal to Phyrexia ----------------------------------------------------------

#[test]
fn portal_to_phyrexia_makes_the_opponent_sacrifice_three_and_reanimates_each_upkeep() {
    let mut state = islands();
    let portal = put(&mut state, P0, PORTAL, Zone::Hand);
    let mine = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    let griselbrand = put(&mut state, P1, "Griselbrand", Zone::Battlefield);
    let angler = put(&mut state, P1, "Gurmag Angler", Zone::Battlefield);
    let elves = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    let goblin = token(&mut state, P1, "Goblin Token");
    let island = put(&mut state, P1, "Island", Zone::Battlefield);
    pool(&mut state, P0, &[], 9);
    cast(&mut state, portal, &[]);

    // The opponent chooses three of their four creatures, sacrificed at
    // once.
    match settle(&mut state) {
        Some(Decision::ChooseEffectTargets {
            player,
            min_targets,
            max_targets,
            legal_targets,
            ..
        }) => {
            assert_eq!(player, P1, "the opponent chooses");
            assert_eq!((min_targets, max_targets), (3, 3));
            assert_eq!(legal_targets.len(), 4);
            assert!(!legal_targets.contains(&Target::Object(island)));
            assert!(!legal_targets.contains(&Target::Object(mine)));
        }
        other => panic!("expected the sacrifice choice, got {other:?}"),
    }
    let mut copy = restored(&state);
    assert_eq!(state.state_hash(), copy.state_hash());
    for continued in [&mut state, &mut copy] {
        for object in [angler, elves, goblin] {
            act(
                continued,
                Action::ChooseEffectTarget(Target::Object(object)),
            );
        }
        settled(continued);
    }
    assert_eq!(state.state_hash(), copy.state_hash());
    assert_eq!(zone(&state, griselbrand), Zone::Battlefield);
    assert_eq!(zone(&state, angler), Zone::Graveyard);
    assert_eq!(zone(&state, elves), Zone::Graveyard);
    assert_ne!(zone(&state, goblin), Zone::Battlefield);
    assert_eq!(zone(&state, mine), Zone::Battlefield);

    // Its controller's upkeep: a creature card from any graveyard, under
    // their control, a Phyrexian in addition to its other types.
    let griselbrand_card = put(&mut state, P0, "Griselbrand", Zone::Graveyard);
    let mut legal = Vec::new();
    pass_until(&mut state, |state| state.active_player == P1);
    run_until(
        &mut state,
        |state| state.active_player == P0 && state.step == Step::Main1,
        |_, decision| match decision {
            Decision::ChooseTargets {
                player,
                spell,
                legal_targets,
                ..
            } if *spell == portal => {
                assert_eq!(*player, P0);
                legal = legal_targets.clone();
                Some(Action::ChooseTarget(Target::Object(angler)))
            }
            _ => None,
        },
    );
    for card in [angler, elves, griselbrand_card] {
        assert!(legal.contains(&Target::Object(card)), "{card:?}");
    }
    assert!(
        !legal.contains(&Target::Object(griselbrand)),
        "on the battlefield"
    );
    assert_eq!(zone(&state, angler), Zone::Battlefield);
    assert_eq!(state.objects.get(angler).controller, P0);
    assert!(subtype(&state, angler, Subtype::Phyrexian));
    assert!(subtype(&state, angler, Subtype::Zombie));
    assert!(subtype(&state, angler, Subtype::Fish));
    assert_eq!(power_toughness(&state, angler), (5, 5));
    let copy = restored(&state);
    assert_eq!(state.state_hash(), copy.state_hash());
    assert!(subtype(&copy, angler, Subtype::Phyrexian));
}

#[test]
fn portal_to_phyrexia_takes_every_creature_from_an_opponent_with_three_or_fewer() {
    let mut state = islands();
    let portal = put(&mut state, P0, PORTAL, Zone::Hand);
    let mine = put(&mut state, P0, "Gurmag Angler", Zone::Battlefield);
    let first = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    let second = put(&mut state, P1, "Gurmag Angler", Zone::Battlefield);
    pool(&mut state, P0, &[], 9);
    cast(&mut state, portal, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, first), Zone::Graveyard);
    assert_eq!(zone(&state, second), Zone::Graveyard);
    assert_eq!(zone(&state, mine), Zone::Battlefield);
    assert_eq!(zone(&state, portal), Zone::Battlefield);
}
