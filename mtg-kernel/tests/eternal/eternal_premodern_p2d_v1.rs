//! Premodern wave P2d: native rules tests for the cards that complete White
//! Weenie and Mono-Red Control.
#![cfg(feature = "eternal-fixtures")]

use mtg_kernel::card_def::{
    card_id_by_name, CardCapability, CardDef, CardType, Keywords, Subtype, CARD_DEFS,
};
use mtg_kernel::engine::{self, Action, Decision, OptionalCostChoice};
use mtg_kernel::eternal_premodern_p2d_v1::statics;
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::ManaColor;
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Target, Zone};

const P0: PlayerId = PlayerId::P0;
const P1: PlayerId = PlayerId::P1;

const P2D_CARDS: [&str; 15] = [
    "Avalanche Riders",
    "Beloved Chaplain",
    "Devoted Caretaker",
    "Earthquake",
    "Firebolt",
    "Ghitu Encampment",
    "Glowrider",
    "Mother of Runes",
    "Nova Cleric",
    "Pillage",
    "Shared Triumph",
    "Soltari Monk",
    "Soltari Priest",
    "Stone Rain",
    "Veteran Brawlers",
];

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
        0x5032_4400_0001,
    );
    state.active_player = P0;
    state.priority_player = P0;
    state.step = Step::Main1;
    state
}

fn islands() -> GameState {
    game(&["Island"; 20])
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

fn power_toughness(state: &GameState, object: ObjectId) -> (i32, i32) {
    (
        engine::effective_power(state, object),
        engine::effective_toughness(state, object),
    )
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
/// finishing a selection that may finish, declining a Boolean or kicker.
fn first_answer(decision: Decision) -> Action {
    match decision {
        Decision::ChooseTargets {
            legal_targets,
            can_finish: true,
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

/// Settles, answering effect options with `option` and choosing `object`
/// whenever a selection or target offers it; otherwise the first answer.
fn settle_picking(state: &mut GameState, option: u16, object: Option<ObjectId>) {
    settle_with(state, |_, decision| match decision {
        Decision::ChooseEffectOption { .. } => Action::ChooseEffectOption(option),
        Decision::ChooseTargets { legal_targets, .. }
            if object.is_some_and(|object| legal_targets.contains(&Target::Object(object))) =>
        {
            Action::ChooseTarget(Target::Object(object.unwrap()))
        }
        Decision::ChooseEffectTargets {
            legal_targets,
            selected_count: 0,
            ..
        } if object.is_some_and(|object| legal_targets.contains(&Target::Object(object))) => {
            Action::ChooseEffectTarget(Target::Object(object.unwrap()))
        }
        other => first_answer(other),
    });
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

/// The attackers P0 may declare in this turn's combat.
fn eligible_attackers(state: &mut GameState) -> Vec<ObjectId> {
    pass_until(state, |state| state.step == Step::DeclareAttackers);
    match next(state) {
        Decision::DeclareAttackers { eligible, .. } => eligible,
        _ => Vec::new(),
    }
}

/// Puts `object` onto the battlefield under its owner's control, as if an
/// effect did.
fn enter(state: &mut GameState, object: ObjectId) {
    event::propose_and_commit(state, ProposedEvent::zone_change(object, Zone::Battlefield));
}

/// Deals `amount` damage from `source` to `object`.
fn damage(state: &mut GameState, source: ObjectId, object: ObjectId, amount: i32) {
    event::propose_and_commit(
        state,
        ProposedEvent::damage(source, Target::Object(object), amount),
    );
}

/// Answers the X announcement of a pending cast with `x`.
fn announce_x(state: &mut GameState, x: u16) {
    match next(state) {
        Decision::ChooseEffectOption { option_count, .. } => {
            assert!(x < option_count, "X {x} of {option_count}");
            act(state, Action::ChooseEffectOption(x));
        }
        other => panic!("expected the X announcement, got {other:?}"),
    }
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

/// Plays to P0's next turn's first main phase, answering with `answer`.
fn to_next_turn(
    state: &mut GameState,
    answer: impl FnMut(&GameState, &Decision) -> Option<Action>,
) {
    let turn = state.turn;
    run_until(
        state,
        |state| state.turn > turn && state.active_player == P0 && state.step == Step::Main1,
        answer,
    );
}

fn red_option() -> u16 {
    // Wave E9a's colors are in WUBRG order.
    3
}

// ---- Characteristics -----------------------------------------------------------

#[test]
fn p2d_cards_are_full_with_their_printed_characteristics() {
    for name in P2D_CARDS {
        assert_eq!(def(name).capability, CardCapability::Full, "{name}");
        assert!(!def(name).is_token, "{name}");
    }
    assert!(def("Avalanche Riders").keywords.has(Keywords::HASTE));
    assert!(def("Firebolt").flashback.is_some());
    assert!(def("Ghitu Encampment").enters_battlefield_tapped);
    assert_eq!(
        (
            def("Veteran Brawlers").power,
            def("Veteran Brawlers").toughness
        ),
        (Some(4), Some(4))
    );
    for name in ["Soltari Monk", "Soltari Priest"] {
        assert!(def(name).subtypes.contains(&Subtype::Soltari), "{name}");
    }
    assert!(Subtype::Soltari.is_creature_type());
    assert_eq!(Subtype::Soltari.stable_id(), 240);
    assert!(def("Shared Triumph").has_type(CardType::Enchantment));
}

// ---- White Weenie ---------------------------------------------------------------

#[test]
fn beloved_chaplain_has_protection_from_creatures() {
    // Damage from a creature is prevented; from a noncreature it is not.
    let mut state = islands();
    let chaplain = put(&mut state, P0, "Beloved Chaplain", Zone::Battlefield);
    let elves = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    let scroll = put(&mut state, P1, "Cursed Scroll", Zone::Battlefield);
    damage(&mut state, elves, chaplain, 1);
    assert_eq!(state.objects.get(chaplain).damage, 0, "a creature's damage");
    damage(&mut state, scroll, chaplain, 1);
    assert_eq!(
        state.objects.get(chaplain).damage,
        1,
        "an artifact's damage"
    );

    // It can't be blocked by creatures.
    let mut state = islands();
    let chaplain = put(&mut state, P0, "Beloved Chaplain", Zone::Battlefield);
    put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    assert!(legal_blockers(&mut state, chaplain).is_empty());

    // A creature's ability can't target it.
    let mut state = islands();
    let chaplain = put(&mut state, P1, "Beloved Chaplain", Zone::Battlefield);
    let sharpshooter = put(&mut state, P0, "Goblin Sharpshooter", Zone::Battlefield);
    let island = put(&mut state, P1, "Island", Zone::Battlefield);
    act(&mut state, Action::ActivateAbility(sharpshooter, 0));
    match next(&mut state) {
        Decision::ChooseTargets { legal_targets, .. } => {
            assert!(!legal_targets.contains(&Target::Object(chaplain)));
            assert!(legal_targets.contains(&Target::Player(P1)));
        }
        other => panic!("{other:?}"),
    }
    let _ = island;

    // A noncreature spell can.
    let mut state = islands();
    let chaplain = put(&mut state, P1, "Beloved Chaplain", Zone::Battlefield);
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 0);
    cast(&mut state, bolt, &[Target::Object(chaplain)]);
    settled(&mut state);
    assert_eq!(zone(&state, chaplain), Zone::Graveyard);
}

#[test]
fn soltari_have_protection_from_a_color_and_shadow() {
    // Monk: protection from black; Priest: protection from red.
    let mut state = islands();
    let monk = put(&mut state, P0, "Soltari Monk", Zone::Battlefield);
    let priest = put(&mut state, P0, "Soltari Priest", Zone::Battlefield);
    let specter = put(&mut state, P1, "Hypnotic Specter", Zone::Battlefield);
    let fanatic = put(&mut state, P1, "Mogg Fanatic", Zone::Battlefield);
    damage(&mut state, specter, monk, 1);
    assert_eq!(
        state.objects.get(monk).damage,
        0,
        "black damage to the Monk"
    );
    damage(&mut state, fanatic, monk, 1);
    assert_eq!(state.objects.get(monk).damage, 1, "red damage to the Monk");
    damage(&mut state, fanatic, priest, 1);
    assert_eq!(
        state.objects.get(priest).damage,
        0,
        "red damage to the Priest"
    );
    damage(&mut state, specter, priest, 1);
    assert_eq!(
        state.objects.get(priest).damage,
        1,
        "black damage to the Priest"
    );

    // A red spell can't target the Priest.
    let mut state = islands();
    let priest = put(&mut state, P1, "Soltari Priest", Zone::Battlefield);
    let monk = put(&mut state, P1, "Soltari Monk", Zone::Battlefield);
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 0);
    act(&mut state, Action::CastSpell(bolt));
    match next(&mut state) {
        Decision::ChooseTargets { legal_targets, .. } => {
            assert!(!legal_targets.contains(&Target::Object(priest)));
            assert!(legal_targets.contains(&Target::Object(monk)));
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn shadow_blocks_and_is_blocked_only_by_shadow() {
    // A shadow attacker can't be blocked by a creature without shadow.
    let mut state = islands();
    let monk = put(&mut state, P0, "Soltari Monk", Zone::Battlefield);
    put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    assert!(legal_blockers(&mut state, monk).is_empty());

    // A creature without shadow can't be blocked by a shadow creature.
    let mut state = islands();
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    put(&mut state, P1, "Soltari Monk", Zone::Battlefield);
    assert!(legal_blockers(&mut state, elves).is_empty());

    // Shadow blocks shadow, whichever card has it (Dauthi Voidwalker too).
    let mut state = islands();
    let priest = put(&mut state, P0, "Soltari Priest", Zone::Battlefield);
    let monk = put(&mut state, P1, "Soltari Monk", Zone::Battlefield);
    let dauthi = put(&mut state, P1, "Dauthi Voidwalker", Zone::Battlefield);
    let blockers = legal_blockers(&mut state, priest);
    assert!(blockers.contains(&monk) && blockers.contains(&dauthi));
}

#[test]
fn mother_of_runes_gives_protection_from_a_chosen_color() {
    let mut state = islands();
    let mother = put(&mut state, P0, "Mother of Runes", Zone::Battlefield);
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    let fanatic = put(&mut state, P1, "Mogg Fanatic", Zone::Battlefield);
    let scroll = put(&mut state, P1, "Cursed Scroll", Zone::Battlefield);
    activate(&mut state, mother, 0, &[Target::Object(elves)]);
    settle_picking(&mut state, red_option(), None);
    assert!(state.objects.get(mother).tapped);
    damage(&mut state, fanatic, elves, 1);
    assert_eq!(
        state.objects.get(elves).damage,
        0,
        "red damage is prevented"
    );
    damage(&mut state, scroll, elves, 1);
    assert_eq!(
        state.objects.get(elves).damage,
        1,
        "colorless damage is not"
    );

    // A red spell can't target it.
    let bolt = put(&mut state, P1, "Lightning Bolt", Zone::Hand);
    state.priority_player = P1;
    pool(&mut state, P1, &[ManaColor::R], 0);
    act(&mut state, Action::CastSpell(bolt));
    match next(&mut state) {
        Decision::ChooseTargets { legal_targets, .. } => {
            assert!(!legal_targets.contains(&Target::Object(elves)));
            assert!(legal_targets.contains(&Target::Object(mother)));
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn devoted_caretaker_protects_from_instant_and_sorcery_spells() {
    let mut state = islands();
    let caretaker = put(&mut state, P0, "Devoted Caretaker", Zone::Battlefield);
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    let bear = put(&mut state, P1, "Hypnotic Specter", Zone::Battlefield);
    pool(&mut state, P0, &[ManaColor::W], 0);
    activate(&mut state, caretaker, 0, &[Target::Object(elves)]);
    settled(&mut state);

    // An instant can't target the protected permanent.
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 0);
    act(&mut state, Action::CastSpell(bolt));
    match next(&mut state) {
        Decision::ChooseTargets { legal_targets, .. } => {
            assert!(!legal_targets.contains(&Target::Object(elves)));
            assert!(legal_targets.contains(&Target::Object(bear)));
            act(&mut state, Action::ChooseTarget(Target::Object(bear)));
        }
        other => panic!("{other:?}"),
    }
    settled(&mut state);

    // Earthquake's damage to it is prevented; its other damage is not.
    let quake = put(&mut state, P0, "Earthquake", Zone::Hand);
    let goblin = put(&mut state, P1, "Mogg Fanatic", Zone::Battlefield);
    pool(&mut state, P0, &[ManaColor::R], 1);
    act(&mut state, Action::CastSpell(quake));
    announce_x(&mut state, 1);
    settled(&mut state);
    assert_eq!(
        state.objects.get(elves).damage,
        0,
        "protected from a sorcery"
    );
    assert_eq!(zone(&state, goblin), Zone::Graveyard, "an unprotected 1/1");
    assert_eq!(state.players[0].life, 19);

    // A creature's ability still hurts it.
    let sharpshooter = put(&mut state, P0, "Goblin Sharpshooter", Zone::Battlefield);
    activate(&mut state, sharpshooter, 0, &[Target::Object(elves)]);
    settled(&mut state);
    assert_eq!(zone(&state, elves), Zone::Graveyard);
}

#[test]
fn glowrider_taxes_every_noncreature_spell() {
    let mut state = islands();
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R, ManaColor::G], 0);
    assert!(castable(&mut state, bolt));
    assert!(castable(&mut state, elves));

    let mut state = islands();
    put(&mut state, P1, "Glowrider", Zone::Battlefield);
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 0);
    assert!(
        !castable(&mut state, bolt),
        "a noncreature spell costs {{1}} more"
    );
    pool(&mut state, P0, &[ManaColor::G], 0);
    assert!(castable(&mut state, elves), "a creature spell does not");
    pool(&mut state, P0, &[], 1);
    assert!(castable(&mut state, bolt));

    // Two Glowriders cost {2} more.
    let mut state = islands();
    put(&mut state, P1, "Glowrider", Zone::Battlefield);
    put(&mut state, P0, "Glowrider", Zone::Battlefield);
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 1);
    assert!(!castable(&mut state, bolt));
    pool(&mut state, P0, &[], 1);
    assert!(castable(&mut state, bolt));
}

#[test]
fn nova_cleric_destroys_all_enchantments() {
    let mut state = islands();
    let cleric = put(&mut state, P0, "Nova Cleric", Zone::Battlefield);
    let vortex = put(&mut state, P1, "Sulfuric Vortex", Zone::Battlefield);
    let library = put(&mut state, P0, "Sylvan Library", Zone::Battlefield);
    let elves = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    pool(&mut state, P0, &[ManaColor::W], 2);
    activate(&mut state, cleric, 0, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, cleric), Zone::Graveyard, "sacrificed");
    assert_eq!(zone(&state, vortex), Zone::Graveyard);
    assert_eq!(zone(&state, library), Zone::Graveyard);
    assert_eq!(zone(&state, elves), Zone::Battlefield);
}

#[test]
fn shared_triumph_gives_the_chosen_type_plus_one_plus_one() {
    let mut state = islands();
    let fanatic = put(&mut state, P1, "Mogg Fanatic", Zone::Battlefield);
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    let triumph = put(&mut state, P0, "Shared Triumph", Zone::Hand);
    let goblin = statics::triumph_type_options(&state)
        .iter()
        .position(|subtype| *subtype == Subtype::Goblin)
        .expect("a Goblin option");
    pool(&mut state, P0, &[ManaColor::W], 1);
    cast(&mut state, triumph, &[]);
    settle_picking(&mut state, u16::try_from(goblin).unwrap(), None);
    assert_eq!(zone(&state, triumph), Zone::Battlefield);
    assert_eq!(power_toughness(&state, fanatic), (2, 2), "either player's");
    assert_eq!(power_toughness(&state, elves), (1, 1));

    // It stops when the enchantment leaves.
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(triumph, Zone::Graveyard),
    );
    assert_eq!(power_toughness(&state, fanatic), (1, 1));
}

// ---- Mono-Red Control -----------------------------------------------------------

#[test]
fn avalanche_riders_has_haste_destroys_a_land_and_owes_echo() {
    let mut state = islands();
    let island = put(&mut state, P1, "Island", Zone::Battlefield);
    let riders = put(&mut state, P0, "Avalanche Riders", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 3);
    cast(&mut state, riders, &[]);
    settle_picking(&mut state, 0, Some(island));
    assert_eq!(zone(&state, riders), Zone::Battlefield);
    assert_eq!(zone(&state, island), Zone::Graveyard, "destroy target land");
    assert!(eligible_attackers(&mut state).contains(&riders), "haste");

    // Unpaid echo at the next upkeep sacrifices it.
    let mut state = islands();
    let riders = put(&mut state, P0, "Avalanche Riders", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 3);
    cast(&mut state, riders, &[]);
    settle_picking(&mut state, 0, None);
    assert_eq!(zone(&state, riders), Zone::Battlefield);
    to_next_turn(&mut state, |_, decision| match decision {
        Decision::ChooseEffectOption { .. } => Some(Action::ChooseEffectOption(1)),
        _ => None,
    });
    assert_eq!(zone(&state, riders), Zone::Graveyard, "echo not paid");

    // Paid echo keeps it, once.
    let mut state = islands();
    let riders = put(&mut state, P0, "Avalanche Riders", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 3);
    cast(&mut state, riders, &[]);
    settle_picking(&mut state, 0, None);
    for _ in 0..4 {
        put(&mut state, P0, "Mountain", Zone::Battlefield);
    }
    let mut asked = 0;
    to_next_turn(&mut state, |_, decision| match decision {
        Decision::ChooseEffectOption { .. } => {
            asked += 1;
            Some(Action::ChooseEffectOption(0))
        }
        _ => None,
    });
    assert_eq!(asked, 1);
    assert_eq!(zone(&state, riders), Zone::Battlefield, "echo paid");
    let mut asked = 0;
    to_next_turn(&mut state, |_, decision| match decision {
        Decision::ChooseEffectOption { .. } => {
            asked += 1;
            Some(Action::ChooseEffectOption(1))
        }
        _ => None,
    });
    assert_eq!(asked, 0, "echo is owed only once");
    assert_eq!(zone(&state, riders), Zone::Battlefield);

    // Without the mana the creature is sacrificed even if paying is chosen.
    let mut state = islands();
    let riders = put(&mut state, P0, "Avalanche Riders", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 3);
    cast(&mut state, riders, &[]);
    settle_picking(&mut state, 0, None);
    to_next_turn(&mut state, |_, decision| match decision {
        Decision::ChooseEffectOption { .. } => Some(Action::ChooseEffectOption(0)),
        _ => None,
    });
    assert_eq!(zone(&state, riders), Zone::Graveyard);
}

#[test]
fn earthquake_hits_each_creature_without_flying_and_each_player() {
    let mut state = islands();
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    let fanatic = put(&mut state, P1, "Mogg Fanatic", Zone::Battlefield);
    let specter = put(&mut state, P1, "Hypnotic Specter", Zone::Battlefield);
    let quake = put(&mut state, P0, "Earthquake", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 2);
    act(&mut state, Action::CastSpell(quake));
    announce_x(&mut state, 2);
    settled(&mut state);
    assert_eq!(zone(&state, elves), Zone::Graveyard);
    assert_eq!(zone(&state, fanatic), Zone::Graveyard);
    assert_eq!(zone(&state, specter), Zone::Battlefield, "flying");
    assert_eq!(state.objects.get(specter).damage, 0);
    assert_eq!(state.players[0].life, 18);
    assert_eq!(state.players[1].life, 18);
}

#[test]
fn firebolt_deals_two_and_flashes_back() {
    let mut state = islands();
    let firebolt = put(&mut state, P0, "Firebolt", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 0);
    cast(&mut state, firebolt, &[Target::Player(P1)]);
    settled(&mut state);
    assert_eq!(state.players[1].life, 18);
    assert_eq!(zone(&state, firebolt), Zone::Graveyard);
    pool(&mut state, P0, &[ManaColor::R], 4);
    cast(&mut state, firebolt, &[Target::Player(P1)]);
    settled(&mut state);
    assert_eq!(state.players[1].life, 16);
    assert_eq!(zone(&state, firebolt), Zone::Exile, "flashback exiles it");
}

#[test]
fn ghitu_encampment_enters_tapped_and_animates() {
    let mut state = islands();
    let land = put(&mut state, P0, "Ghitu Encampment", Zone::Hand);
    enter(&mut state, land);
    assert!(state.objects.get(land).tapped, "enters tapped");
    assert!(!engine::object_has_type(&state, land, CardType::Creature));
    pool(&mut state, P0, &[ManaColor::R], 1);
    activate(&mut state, land, 0, &[]);
    settled(&mut state);
    assert!(engine::object_has_type(&state, land, CardType::Creature));
    assert!(engine::object_has_type(&state, land, CardType::Land));
    assert_eq!(power_toughness(&state, land), (2, 1));
    assert!(engine::has_effective_keyword(
        &state,
        land,
        Keywords::FIRST_STRIKE
    ));
    assert_eq!(engine::object_color_mask(&state, land), 1 << 3, "red");
    assert!(engine::has_effective_subtype(
        &state,
        land,
        Subtype::Warrior
    ));
}

#[test]
fn pillage_destroys_an_artifact_or_a_land_and_stone_rain_a_land() {
    let mut state = islands();
    let scroll = put(&mut state, P1, "Cursed Scroll", Zone::Battlefield);
    let island = put(&mut state, P1, "Island", Zone::Battlefield);
    let elves = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    let pillage = put(&mut state, P0, "Pillage", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R, ManaColor::R], 1);
    act(&mut state, Action::CastSpell(pillage));
    match next(&mut state) {
        Decision::ChooseTargets { legal_targets, .. } => {
            assert!(legal_targets.contains(&Target::Object(scroll)));
            assert!(legal_targets.contains(&Target::Object(island)));
            assert!(!legal_targets.contains(&Target::Object(elves)));
            act(&mut state, Action::ChooseTarget(Target::Object(scroll)));
        }
        other => panic!("{other:?}"),
    }
    settled(&mut state);
    assert_eq!(zone(&state, scroll), Zone::Graveyard);

    let mut state = islands();
    let island = put(&mut state, P1, "Island", Zone::Battlefield);
    let rain = put(&mut state, P0, "Stone Rain", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 2);
    cast(&mut state, rain, &[Target::Object(island)]);
    settled(&mut state);
    assert_eq!(zone(&state, island), Zone::Graveyard);
}

#[test]
fn pillage_destroys_an_artifact_creature_that_would_regenerate() {
    let mut state = islands();
    let factory = put(&mut state, P0, "Mishra's Factory", Zone::Battlefield);
    pool(&mut state, P0, &[], 1);
    activate(&mut state, factory, 0, &[]);
    settled(&mut state);
    mtg_kernel::eternal_vintage_e7b_v1::statics::add_regeneration_shield(&mut state, factory);
    let pillage = put(&mut state, P0, "Pillage", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R, ManaColor::R], 1);
    cast(&mut state, pillage, &[Target::Object(factory)]);
    settled(&mut state);
    assert_eq!(
        zone(&state, factory),
        Zone::Graveyard,
        "can't be regenerated"
    );
}

#[test]
fn veteran_brawlers_cant_attack_or_block_around_untapped_lands() {
    // It can't attack if defending player controls an untapped land.
    let mut state = islands();
    let brawlers = put(&mut state, P0, "Veteran Brawlers", Zone::Battlefield);
    let island = put(&mut state, P1, "Island", Zone::Battlefield);
    assert!(!eligible_attackers(&mut state).contains(&brawlers));
    let mut state = islands();
    let brawlers = put(&mut state, P0, "Veteran Brawlers", Zone::Battlefield);
    let island2 = put(&mut state, P1, "Island", Zone::Battlefield);
    state.objects.get_mut(island2).tapped = true;
    // Its controller's own lands don't matter.
    put(&mut state, P0, "Island", Zone::Battlefield);
    assert!(eligible_attackers(&mut state).contains(&brawlers));
    let _ = island;

    // It can't block if you control an untapped land.
    let mut state = islands();
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    let brawlers = put(&mut state, P1, "Veteran Brawlers", Zone::Battlefield);
    let land = put(&mut state, P1, "Island", Zone::Battlefield);
    assert!(!legal_blockers(&mut state, elves).contains(&brawlers));
    let mut state = islands();
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    let brawlers = put(&mut state, P1, "Veteran Brawlers", Zone::Battlefield);
    let land = {
        let _ = land;
        put(&mut state, P1, "Island", Zone::Battlefield)
    };
    state.objects.get_mut(land).tapped = true;
    assert!(legal_blockers(&mut state, elves).contains(&brawlers));
}
