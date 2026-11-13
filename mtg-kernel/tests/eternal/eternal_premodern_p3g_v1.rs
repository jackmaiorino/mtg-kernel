//! Premodern sideboard wave P3g: native rules tests for the cards that
//! complete the sideboards of Landstill and BW Control (Akroma's Vengeance,
//! Arcane Laboratory, Circle of Protection: Red and Infest), and for the
//! departed sources both Circles of Protection now follow.
#![cfg(feature = "eternal-fixtures")]
#![allow(dead_code)]

use mtg_kernel::card_def::{
    card_id_by_name, mana_color_mask, CardCapability, CardDef, CardType, CARD_DEFS,
};
use mtg_kernel::effect::{EffectOp, PendingEffectChoice};
use mtg_kernel::engine::{self, Action, Decision, OptionalCostChoice};
use mtg_kernel::eternal_cards_v1::EternalOpV1;
use mtg_kernel::eternal_sideboard_s1d_v1::{CircleColorV1, S1dOpV1};
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::ManaColor;
use mtg_kernel::standard_cards_v1::StandardOpV1;
use mtg_kernel::state::{
    GameObject, GameState, ObjectLinkV4, ObjectStateV4, StackItemKind, Step, Target, Zone,
};

const P0: PlayerId = PlayerId::P0;
const P1: PlayerId = PlayerId::P1;

const P3G_CARDS: [&str; 4] = [
    "Akroma's Vengeance",
    "Arcane Laboratory",
    "Circle of Protection: Red",
    "Infest",
];

const RED_CIRCLE: &str = "Circle of Protection: Red";
const BLUE_CIRCLE: &str = "Circle of Protection: Blue";

fn def(name: &str) -> &'static CardDef {
    &CARD_DEFS[id(name) as usize]
}

fn id(name: &str) -> u16 {
    card_id_by_name(name).unwrap_or_else(|| panic!("{name}"))
}

/// A first main phase with P0 active and holding priority. Each library
/// holds twenty Plains, and both hands are empty.
fn plains() -> GameState {
    let ids = vec![id("Plains"); 20];
    let mut state = GameState::new_from_libraries(
        &ids,
        &ids,
        |id| CARD_DEFS[id as usize].object_name.into(),
        0x5033_4700_0001,
    );
    state.active_player = P0;
    state.priority_player = P0;
    state.step = Step::Main1;
    assert!(state.players.iter().all(|seat| seat.hand.is_empty()));
    state
}

/// `plains` with `active` holding priority on their own turn.
fn turn_of(active: PlayerId) -> GameState {
    let mut state = plains();
    state.active_player = active;
    state.priority_player = active;
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

fn link(state: &GameState, object: ObjectId) -> ObjectLinkV4 {
    ObjectLinkV4 {
        object,
        zone_change_count: state.objects.get(object).zone_change_count,
    }
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

/// The first legal answer to any choice.
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
        Decision::ChooseOptionalCost { .. } => {
            Action::ChooseOptionalCost(OptionalCostChoice::Decline)
        }
        other => panic!("unexpected choice: {other:?}"),
    }
}

/// Settles, answering every choice with its first legal answer.
fn settle_first(state: &mut GameState) {
    while let Some(decision) = settle(state) {
        act(state, first_answer(decision));
    }
}

/// Settles, answering effect options with `option` and otherwise with the
/// first legal answer.
fn settle_option(state: &mut GameState, option: u16) {
    while let Some(decision) = settle(state) {
        let action = match decision {
            Decision::ChooseEffectOption { .. } => Action::ChooseEffectOption(option),
            other => first_answer(other),
        };
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

/// Activates `source`'s ability `index` from priority, answering each target
/// prompt from `targets` in order and each cost choice with `cost` when it
/// is offered (or the first candidate).
fn activate_paying(
    state: &mut GameState,
    source: ObjectId,
    index: u8,
    targets: &[Target],
    cost: Option<ObjectId>,
) {
    assert!(
        activatable(state, source, index),
        "{} ability {index} not activatable",
        state.objects.get(source).name
    );
    act(state, Action::ActivateAbility(source, index));
    let mut targets = targets.iter().copied();
    loop {
        match next(state) {
            Decision::ChooseTargets { legal_targets, .. } => {
                let target = targets.next().expect("a target");
                assert!(legal_targets.contains(&target), "{target:?}");
                act(state, Action::ChooseTarget(target));
            }
            Decision::ChooseCostTargets { candidates, .. } => {
                let pick = cost
                    .filter(|object| candidates.contains(object))
                    .unwrap_or(candidates[0]);
                act(state, Action::ChooseCostTarget(pick));
            }
            _ => break,
        }
    }
    assert!(targets.next().is_none(), "unused targets");
}

fn activate(state: &mut GameState, source: ObjectId, index: u8, targets: &[Target]) {
    activate_paying(state, source, index, targets, None);
}

/// Passes priority from the player holding it.
fn pass(state: &mut GameState) {
    priority(state);
    act(state, Action::Pass);
}

/// Gives priority to `player` by passing while the other player holds it.
fn give_priority(state: &mut GameState, player: PlayerId) {
    priority(state);
    if state.priority_player != player {
        pass(state);
        priority(state);
    }
    assert_eq!(state.priority_player, player);
}

/// A serialized round trip of `state`.
fn restored(state: &GameState) -> GameState {
    serde_json::from_slice(&serde_json::to_vec(state).unwrap()).unwrap()
}

/// The sources the pending Circle choice offers, in order, with the color
/// of the Circle whose shield each option creates.
fn circle_menu(state: &GameState) -> Vec<(ObjectLinkV4, CircleColorV1)> {
    let Some(PendingEffectChoice::ChooseOption { options, .. }) = state
        .engine
        .pending_effect
        .as_ref()
        .and_then(|pending| pending.choice.as_ref())
    else {
        panic!("Circle source options missing");
    };
    options
        .iter()
        .map(|op| match op {
            EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::S1dV1(
                S1dOpV1::CopShieldV1 { source },
            ))) => (*source, CircleColorV1::Blue),
            EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::S1dV1(
                S1dOpV1::CircleShieldV1 { source, color },
            ))) => (*source, *color),
            other => panic!("unexpected Circle option {other:?}"),
        })
        .collect()
}

/// Activates `circle` for `player` with priority, who pays {1}, and lets the
/// ability resolve to its source choice, returning the menu.
fn open_circle(
    state: &mut GameState,
    player: PlayerId,
    circle: ObjectId,
) -> Vec<(ObjectLinkV4, CircleColorV1)> {
    give_priority(state, player);
    pool(state, player, &[], 1);
    activate(state, circle, 0, &[]);
    let decision = settle(state).expect("the Circle's source choice");
    assert!(
        matches!(decision, Decision::ChooseEffectOption { player: chooser, source, .. }
            if chooser == player && source == circle),
        "{decision:?}"
    );
    circle_menu(state)
}

/// A second source of `color` on `player`'s battlefield. The interpreter
/// applies a choice with one option without asking, so tests that check a
/// Circle's menu give it at least two sources.
fn decoy(state: &mut GameState, player: PlayerId, color: CircleColorV1) -> ObjectLinkV4 {
    let name = match color {
        CircleColorV1::Blue => "Mulldrifter",
        CircleColorV1::Red => "Jackal Pup",
    };
    let object = put(state, player, name, Zone::Battlefield);
    link(state, object)
}

/// Answers the open Circle choice with `source`.
fn choose_source(state: &mut GameState, source: ObjectLinkV4) {
    let menu = circle_menu(state);
    let option = menu
        .iter()
        .position(|(offered, _)| *offered == source)
        .unwrap_or_else(|| panic!("{source:?} not offered: {menu:?}"));
    act(
        state,
        Action::ChooseEffectOption(u16::try_from(option).unwrap()),
    );
}

/// Activates `circle` and chooses `source`, which must be offered.
fn shield(state: &mut GameState, player: PlayerId, circle: ObjectId, source: ObjectLinkV4) {
    open_circle(state, player, circle);
    choose_source(state, source);
}

fn shields(state: &GameState) -> usize {
    state
        .eternal_v1
        .as_ref()
        .map_or(0, |eternal| eternal.s1d.cop_shields.len())
}

fn departed_records(state: &GameState) -> usize {
    state
        .eternal_v1
        .as_ref()
        .map_or(0, |eternal| eternal.s1d.departed_sources.len())
}

fn deal(state: &mut GameState, source: ObjectId, player: PlayerId, amount: i32) {
    event::propose_and_commit(
        state,
        ProposedEvent::damage(source, Target::Player(player), amount),
    );
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
            Decision::Discard { count, choices, .. } => {
                Action::Discard(choices.into_iter().take(count as usize).collect())
            }
            other => panic!("unexpected choice: {other:?}"),
        };
        act(state, action);
    }
    panic!("condition never held");
}

// ---- Characteristics -----------------------------------------------------------

#[test]
fn p3g_cards_are_full_with_their_printed_characteristics() {
    for name in P3G_CARDS.into_iter().chain([BLUE_CIRCLE]) {
        assert_eq!(def(name).capability, CardCapability::Full, "{name}");
        assert!(!def(name).is_token, "{name}");
    }
    let red = def(RED_CIRCLE);
    assert!(red.has_type(CardType::Enchantment));
    assert_eq!((red.mana_value, red.colors), (2, &[ManaColor::W][..]));
    assert_eq!(red.activated_abilities.len(), 1);
    let laboratory = def("Arcane Laboratory");
    assert!(laboratory.has_type(CardType::Enchantment));
    assert_eq!(
        (laboratory.mana_value, laboratory.colors),
        (3, &[ManaColor::U][..])
    );
    assert!(laboratory.activated_abilities.is_empty());
    let infest = def("Infest");
    assert!(infest.has_type(CardType::Sorcery));
    assert_eq!((infest.mana_value, infest.colors), (3, &[ManaColor::B][..]));
    let vengeance = def("Akroma's Vengeance");
    assert!(vengeance.has_type(CardType::Sorcery));
    assert_eq!(
        (vengeance.mana_value, vengeance.colors),
        (6, &[ManaColor::W][..])
    );
    assert_eq!(vengeance.activated_abilities.len(), 1, "cycling");
}

// ---- Circle of Protection: Red ---------------------------------------------------

#[test]
fn circle_of_protection_red_prevents_the_next_damage_from_a_chosen_red_source() {
    let mut state = plains();
    let circle = put(&mut state, P0, RED_CIRCLE, Zone::Battlefield);
    let pup = put(&mut state, P1, "Jackal Pup", Zone::Battlefield);
    let other_pup = put(&mut state, P1, "Jackal Pup", Zone::Battlefield);
    let elves = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    let drifter = put(&mut state, P1, "Mulldrifter", Zone::Battlefield);
    let other_drifter = put(&mut state, P1, "Mulldrifter", Zone::Battlefield);
    let blue_circle = put(&mut state, P0, BLUE_CIRCLE, Zone::Battlefield);
    let menu = open_circle(&mut state, P0, circle);
    // Only red sources, each by its incarnation, with red shields.
    assert_eq!(
        menu,
        vec![
            (link(&state, pup), CircleColorV1::Red),
            (link(&state, other_pup), CircleColorV1::Red),
        ]
    );
    let pup_link = link(&state, pup);
    choose_source(&mut state, pup_link);
    settled(&mut state);
    let recorded = state.eternal_v1.as_ref().unwrap().s1d.cop_shields[0];
    assert_eq!(recorded.color, CircleColorV1::Red);
    // The shield's color is serialized: a round trip keeps the hash.
    let copy = restored(&state);
    assert_eq!(copy.state_hash(), state.state_hash());
    assert_eq!(
        copy.eternal_v1.as_ref().unwrap().s1d.cop_shields[0],
        recorded
    );
    // An unchosen red source and a nonred source are not prevented.
    deal(&mut state, other_pup, P0, 2);
    assert_eq!(life(&state, P0), 18);
    deal(&mut state, elves, P0, 2);
    assert_eq!(life(&state, P0), 16);
    // The chosen source is prevented once, then deals damage.
    deal(&mut state, pup, P0, 2);
    assert_eq!(life(&state, P0), 16);
    deal(&mut state, pup, P0, 2);
    assert_eq!(life(&state, P0), 14);
    // Circle of Protection: Blue still offers only blue sources, with its
    // serialized shields.
    let menu = open_circle(&mut state, P0, blue_circle);
    assert_eq!(
        menu,
        vec![
            (link(&state, drifter), CircleColorV1::Blue),
            (link(&state, other_drifter), CircleColorV1::Blue),
        ]
    );
    let drifter_link = link(&state, drifter);
    choose_source(&mut state, drifter_link);
    settled(&mut state);
    let blue = state.eternal_v1.as_ref().unwrap().s1d.cop_shields[0];
    assert_eq!(blue.color, CircleColorV1::Blue);
    let json = serde_json::to_value(blue).unwrap();
    assert!(json.get("color").is_none(), "{json}");
    deal(&mut state, drifter, P0, 2);
    assert_eq!(life(&state, P0), 14);
}

#[test]
fn circle_of_protection_red_prevents_a_chosen_red_spell_both_seats() {
    for actor in [P0, P1] {
        let opponent = actor.opponent();
        let mut state = turn_of(opponent);
        let circle = put(&mut state, actor, RED_CIRCLE, Zone::Battlefield);
        let decoy = decoy(&mut state, opponent, CircleColorV1::Red);
        let bolt = put(&mut state, opponent, "Lightning Bolt", Zone::Hand);
        pool(&mut state, opponent, &[ManaColor::R], 0);
        cast(&mut state, bolt, &[Target::Player(actor)]);
        let menu = open_circle(&mut state, actor, circle);
        assert_eq!(
            menu,
            vec![
                (decoy, CircleColorV1::Red),
                (link(&state, bolt), CircleColorV1::Red),
            ]
        );
        let bolt_link = link(&state, bolt);
        choose_source(&mut state, bolt_link);
        let mut copy = restored(&state);
        for branch in [&mut state, &mut copy] {
            settled(branch);
            assert_eq!(life(branch, actor), 20);
            assert_eq!(shields(branch), 0, "used up");
        }
        assert_eq!(state.state_hash(), copy.state_hash());
    }
}

#[test]
fn circle_of_protection_red_follows_a_chosen_permanent_spell_onto_the_battlefield() {
    for actor in [P0, P1] {
        let opponent = actor.opponent();
        let mut state = turn_of(opponent);
        let circle = put(&mut state, actor, RED_CIRCLE, Zone::Battlefield);
        decoy(&mut state, opponent, CircleColorV1::Red);
        let pup = put(&mut state, opponent, "Jackal Pup", Zone::Hand);
        pool(&mut state, opponent, &[ManaColor::R], 0);
        cast(&mut state, pup, &[]);
        assert_eq!(zone(&state, pup), Zone::Stack);
        let spell = link(&state, pup);
        shield(&mut state, actor, circle, spell);
        settled(&mut state);
        assert_eq!(zone(&state, pup), Zone::Battlefield);
        let bound = state.eternal_v1.as_ref().unwrap().s1d.cop_shields[0];
        assert_eq!(bound.source, spell);
        assert_eq!(bound.spell.unwrap().permanent, Some(link(&state, pup)));
        deal(&mut state, pup, actor, 2);
        assert_eq!(life(&state, actor), 20);
        deal(&mut state, pup, actor, 2);
        assert_eq!(life(&state, actor), 18);
    }
}

#[test]
fn a_circle_shield_survives_its_source_losing_the_color() {
    // 609.7b: the color is checked as the damage would be dealt, and the
    // shield isn't used up by damage from a source that has lost it.
    let mut state = plains();
    let circle = put(&mut state, P0, RED_CIRCLE, Zone::Battlefield);
    let drifter = put(&mut state, P1, "Mulldrifter", Zone::Battlefield);
    let painter = put(&mut state, P0, "Painter's Servant", Zone::Hand);
    pool(&mut state, P0, &[], 2);
    cast(&mut state, painter, &[]);
    // Painter's Servant names a color as it enters, in the order W U B R G:
    // red.
    settle_option(&mut state, 3);
    assert_eq!(
        state.objects.get(painter).v4.chosen_color,
        Some(ManaColor::R)
    );
    let red = mana_color_mask(ManaColor::R);
    assert_ne!(engine::object_color_mask(&state, drifter) & red, 0);
    let drifter_link = link(&state, drifter);
    shield(&mut state, P0, circle, drifter_link);
    settled(&mut state);
    assert_eq!(shields(&state), 1);
    let removal = put(&mut state, P0, "Cast Down", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 1);
    cast(&mut state, removal, &[Target::Object(painter)]);
    settled(&mut state);
    assert_eq!(zone(&state, painter), Zone::Graveyard);
    assert_eq!(engine::object_color_mask(&state, drifter) & red, 0);
    deal(&mut state, drifter, P0, 2);
    assert_eq!(life(&state, P0), 18, "a blue Mulldrifter isn't prevented");
    assert_eq!(shields(&state), 1, "and the shield remains");
}

// ---- Departed sources --------------------------------------------------------------

/// Seal of Fire and Siege-Gang Commander sacrifice themselves to deal their
/// damage. A shield chosen while the source was on the battlefield, or
/// chosen for the departed source while its ability waits, prevents it.
#[test]
fn circle_of_protection_red_prevents_sources_sacrificed_to_their_own_abilities_both_seats() {
    for actor in [P0, P1] {
        for name in ["Seal of Fire", "Siege-Gang Commander"] {
            for chosen_before in [true, false] {
                let opponent = actor.opponent();
                let mut state = turn_of(opponent);
                let circle = put(&mut state, actor, RED_CIRCLE, Zone::Battlefield);
                let decoy = decoy(&mut state, opponent, CircleColorV1::Red);
                let source = put(&mut state, opponent, name, Zone::Battlefield);
                let original = link(&state, source);
                if chosen_before {
                    shield(&mut state, actor, circle, original);
                    settled(&mut state);
                    give_priority(&mut state, opponent);
                }
                if name == "Siege-Gang Commander" {
                    pool(&mut state, opponent, &[ManaColor::R], 1);
                }
                activate_paying(
                    &mut state,
                    source,
                    0,
                    &[Target::Player(actor)],
                    Some(source),
                );
                assert_eq!(zone(&state, source), Zone::Graveyard, "{name}");
                assert!(state.objects.get(source).zone_change_count > original.zone_change_count);
                assert!(state
                    .stack
                    .iter()
                    .any(|item| item.source == source
                        && item.kind == StackItemKind::ActivatedAbility));
                assert_eq!(departed_records(&state), 1, "{name}");
                if !chosen_before {
                    // The departed source is offered by the incarnation its
                    // ability names, never by its graveyard card.
                    let menu = open_circle(&mut state, actor, circle);
                    assert_eq!(
                        menu,
                        vec![(decoy, CircleColorV1::Red), (original, CircleColorV1::Red)],
                        "{name}"
                    );
                    let copy = restored(&state);
                    assert_eq!(copy.state_hash(), state.state_hash());
                    let mut forged = copy.clone();
                    let Some(PendingEffectChoice::ChooseOption { options, .. }) = forged
                        .engine
                        .pending_effect
                        .as_mut()
                        .and_then(|pending| pending.choice.as_mut())
                    else {
                        panic!("restored Circle choice missing");
                    };
                    let card = ObjectLinkV4 {
                        object: source,
                        zone_change_count: copy.objects.get(source).zone_change_count,
                    };
                    options[1] = EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::S1dV1(
                        S1dOpV1::CircleShieldV1 {
                            source: card,
                            color: CircleColorV1::Red,
                        },
                    )));
                    let hash = forged.state_hash();
                    assert!(engine::step(&mut forged, Action::ChooseEffectOption(1)).is_err());
                    assert_eq!(forged.state_hash(), hash);
                    choose_source(&mut state, original);
                }
                let mut copy = restored(&state);
                for branch in [&mut state, &mut copy] {
                    settle_first(branch);
                    assert_eq!(life(branch, actor), 20, "{name} before={chosen_before}");
                    assert_eq!(shields(branch), 0);
                }
                assert_eq!(state.state_hash(), copy.state_hash());
                // The records are dropped as the next turn begins.
                pass_until(&mut state, |state| state.active_player == actor);
                assert_eq!(departed_records(&state), 0);
            }
        }
    }
}

#[test]
fn circle_of_protection_red_prevents_a_lavamancer_killed_in_response_both_seats() {
    for actor in [P0, P1] {
        for chosen_before in [true, false] {
            let opponent = actor.opponent();
            let mut state = turn_of(opponent);
            let circle = put(&mut state, actor, RED_CIRCLE, Zone::Battlefield);
            let decoy = decoy(&mut state, opponent, CircleColorV1::Red);
            let lavamancer = put(&mut state, opponent, "Grim Lavamancer", Zone::Battlefield);
            put(&mut state, opponent, "Plains", Zone::Graveyard);
            put(&mut state, opponent, "Plains", Zone::Graveyard);
            let original = link(&state, lavamancer);
            if chosen_before {
                shield(&mut state, actor, circle, original);
                settled(&mut state);
                give_priority(&mut state, opponent);
            }
            pool(&mut state, opponent, &[ManaColor::R], 0);
            activate(&mut state, lavamancer, 0, &[Target::Player(actor)]);
            assert_eq!(zone(&state, lavamancer), Zone::Battlefield);
            // The actor kills it in response.
            give_priority(&mut state, actor);
            let plow = put(&mut state, actor, "Swords to Plowshares", Zone::Hand);
            pool(&mut state, actor, &[ManaColor::W], 0);
            cast(&mut state, plow, &[Target::Object(lavamancer)]);
            // Both players pass once: only the Plowshares resolves.
            while zone(&state, lavamancer) == Zone::Battlefield {
                pass(&mut state);
            }
            assert_eq!(zone(&state, lavamancer), Zone::Exile);
            assert_eq!(state.stack.len(), 1, "the Lavamancer's ability waits");
            if !chosen_before {
                let menu = open_circle(&mut state, actor, circle);
                assert_eq!(
                    menu,
                    vec![(decoy, CircleColorV1::Red), (original, CircleColorV1::Red)]
                );
                choose_source(&mut state, original);
            }
            settled(&mut state);
            // Swords to Plowshares gave the Lavamancer's controller 1 life.
            assert_eq!(life(&state, opponent), 21);
            assert_eq!(life(&state, actor), 20, "before={chosen_before}");
        }
    }
}

#[test]
fn circle_of_protection_red_offers_a_channeled_twinshot_sniper_both_seats() {
    for actor in [P0, P1] {
        let opponent = actor.opponent();
        let mut state = turn_of(opponent);
        let circle = put(&mut state, actor, RED_CIRCLE, Zone::Battlefield);
        let decoy = decoy(&mut state, opponent, CircleColorV1::Red);
        let sniper = put(&mut state, opponent, "Twinshot Sniper", Zone::Hand);
        let held = put(&mut state, opponent, "Twinshot Sniper", Zone::Hand);
        let in_hand = link(&state, sniper);
        pool(&mut state, opponent, &[ManaColor::R], 1);
        activate(&mut state, sniper, 0, &[Target::Player(actor)]);
        assert_eq!(zone(&state, sniper), Zone::Graveyard);
        // The channeled card is offered by its hand incarnation; a card
        // still in a hand never is.
        let menu = open_circle(&mut state, actor, circle);
        assert_eq!(
            menu,
            vec![(decoy, CircleColorV1::Red), (in_hand, CircleColorV1::Red)]
        );
        assert!(menu.iter().all(|(source, _)| source.object != held));
        choose_source(&mut state, in_hand);
        settled(&mut state);
        assert_eq!(life(&state, actor), 20);
    }
}

#[test]
fn circle_of_protection_red_keeps_mogg_fanatics_capture_without_a_duplicate() {
    for actor in [P0, P1] {
        let opponent = actor.opponent();
        let mut state = turn_of(opponent);
        let circle = put(&mut state, actor, RED_CIRCLE, Zone::Battlefield);
        let decoy = decoy(&mut state, opponent, CircleColorV1::Red);
        let fanatic = put(&mut state, opponent, "Mogg Fanatic", Zone::Battlefield);
        let original = link(&state, fanatic);
        activate(&mut state, fanatic, 0, &[Target::Player(actor)]);
        assert_eq!(zone(&state, fanatic), Zone::Graveyard);
        let menu = open_circle(&mut state, actor, circle);
        assert_eq!(
            menu,
            vec![(decoy, CircleColorV1::Red), (original, CircleColorV1::Red)]
        );
        choose_source(&mut state, original);
        settled(&mut state);
        assert_eq!(life(&state, actor), 20);
    }
}

#[test]
fn circle_of_protection_blue_prevents_a_departed_seal_by_its_last_known_color_both_seats() {
    // Painter's Servant makes the Seal of Fire blue too. It leaves before
    // the Seal's ability resolves, but the Seal's damage is dealt by the
    // Seal as it last existed, which was blue (113.7a).
    for actor in [P0, P1] {
        let opponent = actor.opponent();
        let mut state = turn_of(opponent);
        let circle = put(&mut state, actor, BLUE_CIRCLE, Zone::Battlefield);
        let decoy = decoy(&mut state, opponent, CircleColorV1::Blue);
        let seal = put(&mut state, opponent, "Seal of Fire", Zone::Battlefield);
        let painter = put(&mut state, opponent, "Painter's Servant", Zone::Hand);
        pool(&mut state, opponent, &[], 2);
        cast(&mut state, painter, &[]);
        // Painter's Servant names blue (W U B R G).
        settle_option(&mut state, 1);
        assert_eq!(
            state.objects.get(painter).v4.chosen_color,
            Some(ManaColor::U)
        );
        let original = link(&state, seal);
        give_priority(&mut state, opponent);
        activate(&mut state, seal, 0, &[Target::Player(actor)]);
        let blue = mana_color_mask(ManaColor::U);
        let record = state.eternal_v1.as_ref().unwrap().s1d.departed_sources[0];
        assert_eq!(record.source, original);
        assert_eq!(record.zone, Zone::Battlefield);
        assert_ne!(record.color_mask & blue, 0);
        // The Painter leaves while the Seal's ability waits.
        give_priority(&mut state, actor);
        let removal = put(&mut state, actor, "Cast Down", Zone::Hand);
        pool(&mut state, actor, &[ManaColor::B], 1);
        cast(&mut state, removal, &[Target::Object(painter)]);
        while zone(&state, painter) != Zone::Graveyard {
            pass(&mut state);
        }
        assert_eq!(engine::object_color_mask(&state, seal) & blue, 0);
        assert_eq!(state.stack.len(), 1);
        let menu = open_circle(&mut state, actor, circle);
        assert_eq!(
            menu,
            vec![
                (decoy, CircleColorV1::Blue),
                (original, CircleColorV1::Blue)
            ]
        );
        choose_source(&mut state, original);
        let mut copy = restored(&state);
        for branch in [&mut state, &mut copy] {
            settled(branch);
            assert_eq!(life(branch, actor), 20);
        }
        assert_eq!(state.state_hash(), copy.state_hash());
    }
}

#[test]
fn departures_record_nothing_without_a_circle() {
    let mut state = turn_of(P1);
    let seal = put(&mut state, P1, "Seal of Fire", Zone::Battlefield);
    activate(&mut state, seal, 0, &[Target::Player(P0)]);
    settled(&mut state);
    assert_eq!(life(&state, P0), 18);
    assert_eq!(departed_records(&state), 0);

    // A Circle that arrives while the departed source's ability waits can't
    // choose it: nothing recorded the source's color (a known gap).
    let mut state = turn_of(P1);
    let seal = put(&mut state, P1, "Seal of Fire", Zone::Battlefield);
    activate(&mut state, seal, 0, &[Target::Player(P0)]);
    let circle = put(&mut state, P0, RED_CIRCLE, Zone::Battlefield);
    give_priority(&mut state, P0);
    pool(&mut state, P0, &[], 1);
    activate(&mut state, circle, 0, &[]);
    settled(&mut state);
    assert_eq!(life(&state, P0), 18);
}

// ---- Arcane Laboratory --------------------------------------------------------------

#[test]
fn arcane_laboratory_limits_each_player_to_one_spell_each_turn() {
    for controller in [P0, P1] {
        let mut state = plains();
        put(
            &mut state,
            controller,
            "Arcane Laboratory",
            Zone::Battlefield,
        );
        let first = put(&mut state, P0, "Jackal Pup", Zone::Hand);
        let second = put(&mut state, P0, "Jackal Pup", Zone::Hand);
        let vengeance = put(&mut state, P0, "Akroma's Vengeance", Zone::Hand);
        let firebolt = put(&mut state, P0, "Firebolt", Zone::Graveyard);
        // Enough mana for both Pups, the flashback and the cycling.
        pool(
            &mut state,
            P0,
            &[ManaColor::R, ManaColor::R, ManaColor::R],
            8,
        );
        assert!(castable(&mut state, firebolt), "flashback before a spell");
        cast(&mut state, first, &[]);
        settled(&mut state);
        assert!(!castable(&mut state, second), "a second spell");
        assert!(!castable(&mut state, firebolt), "flashback is a cast too");
        assert!(
            activatable(&mut state, vengeance, 0),
            "cycling isn't a spell"
        );
        activate(&mut state, vengeance, 0, &[]);
        settled(&mut state);
        assert_eq!(zone(&state, vengeance), Zone::Graveyard);

        // The opponent may still cast one spell this turn.
        let reply = put(&mut state, P1, "Lightning Bolt", Zone::Hand);
        let another = put(&mut state, P1, "Lightning Bolt", Zone::Hand);
        pool(&mut state, P1, &[ManaColor::R, ManaColor::R], 0);
        give_priority(&mut state, P1);
        cast(&mut state, reply, &[Target::Player(P0)]);
        assert_eq!(state.priority_player, P1);
        assert!(!castable(&mut state, another));
        settled(&mut state);
        assert_eq!(life(&state, P0), 17);

        // The limit resets as the next turn begins.
        pass_until(&mut state, |state| {
            state.active_player == P1 && state.step == Step::Main1
        });
        give_priority(&mut state, P1);
        pool(&mut state, P1, &[ManaColor::R], 0);
        assert!(castable(&mut state, another));
    }
}

#[test]
fn arcane_laboratory_counts_spells_cast_before_it_entered() {
    let mut state = plains();
    let laboratory = put(&mut state, P0, "Arcane Laboratory", Zone::Hand);
    let pup = put(&mut state, P0, "Jackal Pup", Zone::Hand);
    // Enough mana for the Pup after the Laboratory, however its generic
    // cost is paid.
    pool(
        &mut state,
        P0,
        &[ManaColor::U, ManaColor::R, ManaColor::R, ManaColor::R],
        2,
    );
    assert!(castable(&mut state, pup));
    cast(&mut state, laboratory, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, laboratory), Zone::Battlefield);
    assert!(
        !castable(&mut state, pup),
        "the Laboratory itself was a spell"
    );
}

// ---- Infest and Akroma's Vengeance ------------------------------------------------

#[test]
fn infest_gives_all_creatures_minus_two_minus_two_until_end_of_turn() {
    let mut state = plains();
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    let angler = put(&mut state, P1, "Gurmag Angler", Zone::Battlefield);
    let infest = put(&mut state, P0, "Infest", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B, ManaColor::B], 1);
    cast(&mut state, infest, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, elves), Zone::Graveyard);
    assert_eq!(power_toughness(&state, angler), (3, 3));
    pass_until(&mut state, |state| state.active_player == P1);
    assert_eq!(power_toughness(&state, angler), (5, 5));
}

#[test]
fn akromas_vengeance_destroys_all_artifacts_creatures_and_enchantments() {
    let mut state = plains();
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    let hollow = put(&mut state, P0, "Yavimaya Hollow", Zone::Battlefield);
    let regenerated = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    let angler = put(&mut state, P1, "Gurmag Angler", Zone::Battlefield);
    let ring = put(&mut state, P1, "Sol Ring", Zone::Battlefield);
    let block = put(&mut state, P1, "Nutrient Block", Zone::Battlefield);
    let seal = put(&mut state, P1, "Seal of Fire", Zone::Battlefield);
    let island = put(&mut state, P1, "Island", Zone::Battlefield);
    pool(&mut state, P0, &[ManaColor::G], 0);
    activate(&mut state, hollow, 0, &[Target::Object(regenerated)]);
    settled(&mut state);
    let vengeance = put(&mut state, P0, "Akroma's Vengeance", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W, ManaColor::W], 4);
    cast(&mut state, vengeance, &[]);
    settled(&mut state);
    for card in [elves, angler, ring, seal] {
        assert_eq!(
            zone(&state, card),
            Zone::Graveyard,
            "{}",
            state.objects.get(card).name
        );
    }
    assert_eq!(zone(&state, block), Zone::Battlefield, "indestructible");
    assert_eq!(zone(&state, regenerated), Zone::Battlefield, "regenerated");
    assert!(state.objects.get(regenerated).tapped);
    for land in [hollow, island] {
        assert_eq!(zone(&state, land), Zone::Battlefield);
    }
    assert_eq!(zone(&state, vengeance), Zone::Graveyard);
}

#[test]
fn akromas_vengeance_cycles_for_three() {
    let mut state = plains();
    let vengeance = put(&mut state, P0, "Akroma's Vengeance", Zone::Hand);
    let hand = state.players[0].hand.len();
    pool(&mut state, P0, &[], 2);
    assert!(
        !activatable(&mut state, vengeance, 0),
        "cycling costs {{3}}"
    );
    pool(&mut state, P0, &[], 1);
    activate(&mut state, vengeance, 0, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, vengeance), Zone::Graveyard);
    assert_eq!(
        state.players[0].hand.len(),
        hand,
        "one card for the cycled one"
    );
}
