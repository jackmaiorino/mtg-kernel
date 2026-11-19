//! Vintage Cube wave C6a: native rules and public-session tests for the
//! discard, exile and impulse-draw trigger creatures.
#![cfg(feature = "eternal-fixtures")]
#![allow(dead_code)]

use crate::eternal_catalog_v1::{play_observed, Policy};
use mtg_kernel::card_def::{
    card_id_by_name, CardCapability, CardDef, CardType, Keywords, Subtype, Supertype, CARD_DEFS,
};
use mtg_kernel::engine::{self, Action, Decision, OptionalCostChoice};
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::limited_session_v1::{CustomCardCountV1, CustomDeckV1};
use mtg_kernel::mana::{ManaColor, Pip};
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Target, Zone};
use serde_json::Value;

const P0: PlayerId = PlayerId::P0;
const P1: PlayerId = PlayerId::P1;

const C6A_CARDS: [&str; 6] = [
    "Ivora, Insatiable Heir",
    "Gau, Feral Youth",
    "Laelia, the Blade Reforged",
    "Fallen Shinobi",
    "Generous Plunderer",
    "Gut, True Soul Zealot",
];

// These are admission checks for unfinished cards, not printed-text rules
// tests. Concealed play permissions currently reveal land versus spell in
// the opponent's observation. Keep both cards out of Full session decks.
fn assert_private_impulse_card_is_partial(name: &str) {
    assert_eq!(def(name).capability, CardCapability::Partial);
    assert!(!C6A_CARDS.contains(&name));
    let deck = CustomDeckV1 {
        cards: vec![
            CustomCardCountV1 {
                name: name.into(),
                count: 1,
            },
            CustomCardCountV1 {
                name: "Mountain".into(),
                count: 39,
            },
        ],
        companion: None,
    };
    assert_eq!(
        deck.resolve().unwrap_err(),
        format!("card {name:?} is a token or lacks full engine support")
    );
}

#[test]
fn headliner_scarlett_stays_partial_until_impulse_permissions_preserve_hidden_information() {
    assert_private_impulse_card_is_partial("Headliner Scarlett");
}

#[test]
fn gandalf_stays_partial_until_hidden_wizard_conditional_permissions_are_supported() {
    assert_private_impulse_card_is_partial("Gandalf, Goblins' Bane");
}

#[test]
fn black_widow_stays_no_effect_until_spell_scoped_any_type_spending_is_supported() {
    let name = "Black Widow, Super Spy";
    assert_eq!(def(name).capability, CardCapability::NoEffect);
    assert!(!C6A_CARDS.contains(&name));
    assert_eq!(
        deck(&[(name, 1), ("Swamp", 39)]).resolve().unwrap_err(),
        format!("card {name:?} is a token or lacks full engine support")
    );
}

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
        0x4355_4245_0000 + 0x3661,
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

/// Settles, choosing `object` whenever a selection or target offers it.
fn settle_choosing(state: &mut GameState, object: ObjectId) {
    settle_with(state, |_, decision| match decision {
        Decision::ChooseTargets { legal_targets, .. }
            if legal_targets.contains(&Target::Object(object)) =>
        {
            Action::ChooseTarget(Target::Object(object))
        }
        Decision::ChooseEffectTargets { legal_targets, .. }
            if legal_targets.contains(&Target::Object(object)) =>
        {
            Action::ChooseEffectTarget(Target::Object(object))
        }
        Decision::ChooseCostTargets { candidates, .. } if candidates.contains(&object) => {
            Action::ChooseCostTarget(object)
        }
        other => first_answer(other),
    });
}

/// Settles, answering effect options with `option` and otherwise with the
/// first legal answer.
fn settle_option(state: &mut GameState, option: u16) {
    settle_with(state, |_, decision| match decision {
        Decision::ChooseEffectOption { .. } => Action::ChooseEffectOption(option),
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

/// Casts the modal `spell` choosing `mode` (chosen silently when it is the
/// only legal one), then its targets.
fn cast_mode(state: &mut GameState, spell: ObjectId, mode: u8, targets: &[Target]) {
    assert!(castable(state, spell));
    act(state, Action::CastSpell(spell));
    if let Decision::ChooseSpellMode { legal_modes, .. } = next(state) {
        assert!(legal_modes.contains(&mode), "{mode} not in {legal_modes:?}");
        act(state, Action::ChooseSpellMode(mode));
    }
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

/// P0 attacks with `attackers` in this turn's combat, then the game plays
/// on to P0's second main phase with an empty stack. `answer` answers other
/// choices.
fn attack_with(
    state: &mut GameState,
    attackers: Vec<ObjectId>,
    answer: impl FnMut(&GameState, &Decision) -> Option<Action>,
) {
    pass_until(state, |state| state.step == Step::DeclareAttackers);
    act(state, Action::DeclareAttackers(attackers));
    run_until(
        state,
        |state| {
            state.step == Step::Main2
                && state.stack.is_empty()
                && state.engine.pending_triggers.is_empty()
                && state.engine.pending_effect.is_none()
        },
        answer,
    );
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

/// Plays `land` from P0's hand and settles its triggers with first answers.
fn play_land(state: &mut GameState, land: ObjectId) {
    next(state);
    act(state, Action::PlayLand(land));
    settle_first(state);
}

/// P1 casts a Lightning Bolt at `target` and passes, leaving it on the
/// stack with P0 holding priority.
fn opponent_bolts(state: &mut GameState, target: Target) -> ObjectId {
    state.priority_player = P1;
    let bolt = put(state, P1, "Lightning Bolt", Zone::Hand);
    pool(state, P1, &[ManaColor::R], 0);
    cast(state, bolt, &[target]);
    assert!(matches!(
        next(state),
        Decision::CastSpellOrPass { player: P1, .. }
    ));
    act(state, Action::Pass);
    bolt
}

fn life(state: &GameState, player: PlayerId) -> i32 {
    state.players[player.index()].life
}

/// A custom deck for a public session.
fn deck(rows: &[(&str, u32)]) -> CustomDeckV1 {
    CustomDeckV1 {
        cards: rows
            .iter()
            .map(|&(name, count)| CustomCardCountV1 {
                name: name.to_owned(),
                count,
            })
            .collect(),
        companion: None,
    }
}

// ---- Ivora ---------------------------------------------------------------------

#[test]
fn c6a_cards_are_full_with_printed_characteristics() {
    let card = def(C6A_CARDS[0]);
    assert_eq!(card.capability, CardCapability::Full);
    assert!(card.is_executable());
    assert_eq!(card.cost.generic, 1);
    assert_eq!(card.cost.pips, &[Pip::Colored(ManaColor::R)]);
    assert_eq!(card.cost.x_count, 0);
    assert_eq!(card.types, &[CardType::Creature]);
    assert_eq!(card.supertypes, &[Supertype::Legendary]);
    assert_eq!(card.subtypes, &[Subtype::Vampire, Subtype::Warrior]);
    assert_eq!((card.power, card.toughness), (Some(1), Some(1)));
    assert!(card.keywords.has(Keywords::TRAMPLE));
    for (name, generic, subtypes, haste) in [
        (C6A_CARDS[1], 1, [Subtype::Human, Subtype::Berserker], false),
        (C6A_CARDS[2], 2, [Subtype::Spirit, Subtype::Warrior], true),
    ] {
        let card = def(name);
        assert_eq!(card.capability, CardCapability::Full);
        assert!(card.is_executable());
        assert_eq!(card.cost.generic, generic);
        assert_eq!(card.cost.pips, &[Pip::Colored(ManaColor::R)]);
        assert_eq!(card.cost.x_count, 0);
        assert_eq!(card.types, &[CardType::Creature]);
        assert_eq!(card.supertypes, &[Supertype::Legendary]);
        assert_eq!(card.subtypes, &subtypes);
        assert_eq!((card.power, card.toughness), (Some(2), Some(2)));
        assert_eq!(card.keywords.has(Keywords::HASTE), haste);
    }
    let shinobi = def("Fallen Shinobi");
    assert_eq!(shinobi.capability, CardCapability::Full);
    assert!(shinobi.is_executable());
    assert_eq!(shinobi.cost.generic, 3);
    assert_eq!(
        shinobi.cost.pips,
        &[Pip::Colored(ManaColor::U), Pip::Colored(ManaColor::B)]
    );
    assert_eq!(shinobi.cost.x_count, 0);
    assert_eq!(shinobi.types, &[CardType::Creature]);
    assert!(shinobi.supertypes.is_empty());
    assert_eq!(shinobi.subtypes, &[Subtype::Zombie, Subtype::Ninja]);
    assert_eq!((shinobi.power, shinobi.toughness), (Some(5), Some(4)));
    assert_eq!(shinobi.keywords, Keywords::NONE);
    assert_eq!(shinobi.activated_abilities.len(), 1);
    let ninjutsu = &shinobi.activated_abilities[0];
    assert_eq!(ninjutsu.activation_zone, Zone::Hand);
    assert!(!ninjutsu.sorcery_speed_only);
    assert_eq!(ninjutsu.max_activations_per_turn, None);
    for (name, generic, subtypes, legendary, menace) in [
        (
            C6A_CARDS[4],
            1,
            [Subtype::Human, Subtype::Rogue],
            false,
            true,
        ),
        (
            C6A_CARDS[5],
            2,
            [Subtype::Goblin, Subtype::Shaman],
            true,
            false,
        ),
    ] {
        let card = def(name);
        assert_eq!(card.capability, CardCapability::Full);
        assert!(card.is_executable());
        assert_eq!(card.cost.generic, generic);
        assert_eq!(card.cost.pips, &[Pip::Colored(ManaColor::R)]);
        assert_eq!(card.cost.x_count, 0);
        assert_eq!(card.colors, &[ManaColor::R]);
        assert_eq!(card.types, &[CardType::Creature]);
        assert_eq!(card.subtypes, &subtypes);
        assert_eq!(card.supertypes.contains(&Supertype::Legendary), legendary);
        assert_eq!((card.power, card.toughness), (Some(2), Some(2)));
        assert_eq!(
            card.keywords,
            if menace {
                Keywords::MENACE
            } else {
                Keywords::NONE
            }
        );
    }
    let skeleton = def("Skeleton Token");
    assert!(skeleton.is_token);
    assert_eq!(skeleton.colors, &[ManaColor::B]);
    assert_eq!(skeleton.types, &[CardType::Creature]);
    assert_eq!(skeleton.subtypes, &[Subtype::Skeleton]);
    assert_eq!((skeleton.power, skeleton.toughness), (Some(4), Some(1)));
    assert_eq!(skeleton.keywords, Keywords::MENACE);
}

// ---- Fallen Shinobi -----------------------------------------------------------

fn cast_shinobi(state: &mut GameState) -> ObjectId {
    let shinobi = put(state, P0, "Fallen Shinobi", Zone::Hand);
    assert!(!castable(state, shinobi));
    pool(state, P0, &[ManaColor::U, ManaColor::B], 3);
    cast(state, shinobi, &[]);
    settled(state);
    assert_eq!(zone(state, shinobi), Zone::Battlefield);
    assert_eq!(power_toughness(state, shinobi), (5, 4));
    assert!(state.objects.get(shinobi).summoning_sick);
    assert!(!keyword(state, shinobi, Keywords::HASTE));
    shinobi
}

fn ready_shinobi(state: &mut GameState) -> ObjectId {
    let shinobi = cast_shinobi(state);
    next_turn_main(state);
    next_turn_main(state);
    assert!(!state.objects.get(shinobi).summoning_sick);
    shinobi
}

#[test]
fn fallen_shinobi_exiles_two_opposing_cards_and_casts_them_free_under_its_controller() {
    let mut state = plains();
    let shinobi = ready_shinobi(&mut state);
    let own_top = state.players[0].library[0];
    let bear = put(&mut state, P1, "Wild Mongrel", Zone::Library);
    let bolt = put(&mut state, P1, "Lightning Bolt", Zone::Library);
    let third = state.players[1].library[2];
    attack_with(&mut state, vec![shinobi], |_, _| None);
    assert_eq!(life(&state, P1), 15);
    assert_eq!(zone(&state, bolt), Zone::Exile);
    assert_eq!(zone(&state, bear), Zone::Exile);
    assert_eq!(zone(&state, third), Zone::Library);
    assert_eq!(zone(&state, own_top), Zone::Library);
    assert_eq!(state.engine.exile_play_permissions.len(), 2);
    assert!(state
        .engine
        .exile_play_permissions
        .iter()
        .all(|permission| { permission.holder == P0 && permission.without_mana_cost.0 }));
    state.priority_player = P1;
    pool(&mut state, P1, &[ManaColor::R, ManaColor::G], 1);
    assert!(!castable(&mut state, bolt), "owner has no permission");
    state.priority_player = P0;
    assert!(state.players[0].mana_pool.iter().all(|amount| *amount == 0));
    cast(&mut state, bolt, &[Target::Player(P1)]);
    settled(&mut state);
    assert_eq!(life(&state, P1), 12);
    assert!(state.players[1].graveyard.contains(&bolt));
    cast(&mut state, bear, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, bear), Zone::Battlefield);
    assert_eq!(state.objects.get(bear).controller, P0);
    assert_eq!(state.objects.get(bear).owner, P1);
    assert!(state.players[0].mana_pool.iter().all(|amount| *amount == 0));
}

#[test]
fn fallen_shinobi_ninjutsu_pays_four_mana_returns_an_unblocked_attacker_and_hits() {
    let mut state = plains();
    let attacker = put(&mut state, P0, "Wild Mongrel", Zone::Battlefield);
    let shinobi = put(&mut state, P0, "Fallen Shinobi", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U, ManaColor::B], 2);
    assert!(!activatable(&mut state, shinobi, 0), "not in combat");
    pass_until(&mut state, |s| s.step == Step::DeclareAttackers);
    act(&mut state, Action::DeclareAttackers(vec![attacker]));
    on_the_stack(&mut state);
    pool(&mut state, P0, &[ManaColor::U, ManaColor::B], 2);
    assert!(
        !activatable(&mut state, shinobi, 0),
        "blockers not declared"
    );
    pass_until(&mut state, |s| s.step == Step::DeclareBlockers);
    act(&mut state, Action::DeclareBlockers(vec![]));
    // The main-phase pool emptied when combat began.
    pool(&mut state, P0, &[ManaColor::U, ManaColor::B], 2);
    activate(&mut state, shinobi, 0, &[]);
    on_the_stack(&mut state);
    assert_eq!(zone(&state, attacker), Zone::Hand);
    assert_eq!(zone(&state, shinobi), Zone::Hand);
    assert!(state.players[0].mana_pool.iter().all(|amount| *amount == 0));
    assert!(!state.stack.is_empty());
    settled(&mut state);
    assert_eq!(zone(&state, shinobi), Zone::Battlefield);
    assert!(state.objects.get(shinobi).tapped);
    assert!(state.engine.combat.attackers.contains(&shinobi));
    pass_until(&mut state, |s| s.step == Step::Main2 && s.stack.is_empty());
    assert_eq!(life(&state, P1), 15);
    assert_eq!(state.engine.exile_play_permissions.len(), 2);
}

#[test]
fn fallen_shinobi_ninjutsu_requires_mana_and_an_unblocked_attacker() {
    for blocked in [false, true] {
        let mut state = plains();
        let attacker = put(&mut state, P0, "Wild Mongrel", Zone::Battlefield);
        let blocker = put(&mut state, P1, "Wild Mongrel", Zone::Battlefield);
        let shinobi = put(&mut state, P0, "Fallen Shinobi", Zone::Hand);
        pass_until(&mut state, |s| s.step == Step::DeclareAttackers);
        act(&mut state, Action::DeclareAttackers(vec![attacker]));
        pass_until(&mut state, |s| s.step == Step::DeclareBlockers);
        act(
            &mut state,
            Action::DeclareBlockers(if blocked {
                vec![(blocker, attacker)]
            } else {
                vec![]
            }),
        );
        assert!(!activatable(&mut state, shinobi, 0), "no mana");
        pool(&mut state, P0, &[ManaColor::U, ManaColor::B], 1);
        assert!(!activatable(&mut state, shinobi, 0), "one mana short");
        pool(&mut state, P0, &[], 1);
        assert_eq!(activatable(&mut state, shinobi, 0), !blocked);
        assert_eq!(
            zone(&state, attacker),
            Zone::Battlefield,
            "declining pays nothing"
        );
        assert_eq!(zone(&state, shinobi), Zone::Hand);
        assert!(state.engine.exile_play_permissions.is_empty());
    }
}

#[test]
fn fallen_shinobi_uses_the_correct_opponent_and_exiles_in_one_batch_in_both_seats() {
    for player in [P0, P1] {
        let mut state = plains();
        state.active_player = player;
        state.priority_player = player;
        let shinobi = put(&mut state, player, "Fallen Shinobi", Zone::Hand);
        pool(&mut state, player, &[ManaColor::U, ManaColor::B], 3);
        cast(&mut state, shinobi, &[]);
        settled(&mut state);
        next_turn_main(&mut state);
        next_turn_main(&mut state);
        let opponent = player.opponent();
        let laelia = put(
            &mut state,
            opponent,
            "Laelia, the Blade Reforged",
            Zone::Battlefield,
        );
        let bolt = put(&mut state, opponent, "Lightning Bolt", Zone::Library);
        let land = put(&mut state, opponent, "Swamp", Zone::Library);
        let own_top = state.players[player.index()].library[0];
        attack_with(&mut state, vec![shinobi], |_, _| None);
        assert_eq!(zone(&state, own_top), Zone::Library);
        assert_eq!(zone(&state, land), Zone::Exile);
        assert_eq!(zone(&state, bolt), Zone::Exile);
        assert_eq!(state.objects.get(laelia).counters.plus1_plus1, 1);
        assert!(state
            .engine
            .exile_play_permissions
            .iter()
            .all(|permission| permission.holder == player));
        cast(&mut state, bolt, &[Target::Player(opponent)]);
        settled(&mut state);
        assert_eq!(life(&state, opponent), 12);
    }
}

#[test]
fn fallen_shinobi_ninjutsu_does_not_recover_a_source_that_left_hand() {
    let mut state = plains();
    let attacker = put(&mut state, P0, "Wild Mongrel", Zone::Battlefield);
    let shinobi = put(&mut state, P0, "Fallen Shinobi", Zone::Hand);
    pass_until(&mut state, |s| s.step == Step::DeclareAttackers);
    act(&mut state, Action::DeclareAttackers(vec![attacker]));
    pass_until(&mut state, |s| s.step == Step::DeclareBlockers);
    act(&mut state, Action::DeclareBlockers(vec![]));
    pool(&mut state, P0, &[ManaColor::U, ManaColor::B], 2);
    activate(&mut state, shinobi, 0, &[]);
    on_the_stack(&mut state);
    event::propose_and_commit(&mut state, ProposedEvent::zone_change(shinobi, Zone::Exile));
    event::propose_and_commit(&mut state, ProposedEvent::zone_change(shinobi, Zone::Hand));
    settled(&mut state);
    assert_eq!(zone(&state, shinobi), Zone::Hand);
    assert_eq!(zone(&state, attacker), Zone::Hand);
    assert!(!state.engine.combat.attackers.contains(&shinobi));
}

#[test]
fn fallen_shinobi_ignores_noncombat_damage_other_attackers_and_blocked_damage() {
    for blocked in [false, true] {
        let mut state = plains();
        let shinobi = ready_shinobi(&mut state);
        event::propose_and_commit(
            &mut state,
            ProposedEvent::damage(shinobi, Target::Player(P1), 1),
        );
        settled(&mut state);
        assert!(state.engine.exile_play_permissions.is_empty());
        let bear = put(&mut state, P0, "Wild Mongrel", Zone::Battlefield);
        let blocker = put(&mut state, P1, "Wild Mongrel", Zone::Battlefield);
        let top = state.players[1].library[0];
        attack_with(
            &mut state,
            vec![if blocked { shinobi } else { bear }],
            |_, decision| {
                if blocked && matches!(decision, Decision::DeclareBlockers { .. }) {
                    Some(Action::DeclareBlockers(vec![(blocker, shinobi)]))
                } else {
                    None
                }
            },
        );
        assert_eq!(zone(&state, top), Zone::Library);
        assert!(state.engine.exile_play_permissions.is_empty());
        assert_eq!(life(&state, P1), if blocked { 19 } else { 17 });
    }
}

#[test]
fn fallen_shinobi_permissions_survive_source_departure_and_obey_timing_and_land_limit() {
    for used_land in [false, true] {
        let mut state = plains();
        let shinobi = ready_shinobi(&mut state);
        if used_land {
            let land = put(&mut state, P0, "Island", Zone::Hand);
            play_land(&mut state, land);
        }
        let bear = put(&mut state, P1, "Wild Mongrel", Zone::Library);
        let land = put(&mut state, P1, "Swamp", Zone::Library);
        pass_until(&mut state, |s| s.step == Step::DeclareAttackers);
        act(&mut state, Action::DeclareAttackers(vec![shinobi]));
        pass_until(&mut state, |s| {
            s.step == Step::CombatDamage && !s.stack.is_empty()
        });
        // Removal in response cannot erase the already-triggered ability.
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(shinobi, Zone::Graveyard),
        );
        settled(&mut state);
        assert_eq!(zone(&state, land), Zone::Exile);
        assert_eq!(zone(&state, bear), Zone::Exile);
        assert!(!castable(&mut state, bear), "creature timing still applies");
        let Decision::CastSpellOrPass { land_drops, .. } = next(&mut state) else {
            panic!("combat priority");
        };
        assert!(!land_drops.contains(&land));
        pass_until(&mut state, |s| s.step == Step::Main2);
        assert!(castable(&mut state, bear));
        let Decision::CastSpellOrPass { land_drops, .. } = next(&mut state) else {
            panic!("main-phase priority");
        };
        assert_eq!(land_drops.contains(&land), !used_land);
        if !used_land {
            play_land(&mut state, land);
            assert_eq!(state.objects.get(land).controller, P0);
            assert_eq!(state.objects.get(land).owner, P1);
            assert!(state.players[P0.index()].battlefield.contains(&land));
            assert!(!state.players[P1.index()].battlefield.contains(&land));
            let other = put(&mut state, P0, "Island", Zone::Hand);
            let Decision::CastSpellOrPass { land_drops, .. } = next(&mut state) else {
                panic!("main-phase priority");
            };
            assert!(
                !land_drops.contains(&other),
                "foreign land used the land drop"
            );
        }
    }
}

#[test]
fn fallen_shinobi_declined_permissions_expire_at_cleanup_and_never_follow_reexile() {
    let mut state = plains();
    let shinobi = ready_shinobi(&mut state);
    let bolt = put(&mut state, P1, "Lightning Bolt", Zone::Library);
    let other = put(&mut state, P1, "Lightning Bolt", Zone::Library);
    attack_with(&mut state, vec![shinobi], |_, _| None);
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(other, Zone::Graveyard),
    );
    event::propose_and_commit(&mut state, ProposedEvent::zone_change(other, Zone::Exile));
    settled(&mut state);
    assert!(
        !castable(&mut state, other),
        "permission is incarnation scoped"
    );
    end_step_settled(&mut state);
    assert!(castable(&mut state, bolt), "end step is still this turn");
    let round = state.turn;
    next_turn_main(&mut state);
    assert_eq!(state.turn, round, "both players share the round key");
    state.priority_player = P0;
    assert!(
        !castable(&mut state, bolt),
        "cleanup expired the permission"
    );
    assert_eq!(zone(&state, bolt), Zone::Exile);
    assert_eq!(zone(&state, other), Zone::Exile);
    next_turn_main(&mut state);
    assert!(!castable(&mut state, bolt));
}

#[test]
fn fallen_shinobi_free_cast_forces_x_zero_and_still_requires_additional_costs() {
    let mut state = plains();
    let shinobi = ready_shinobi(&mut state);
    let ballista = put(&mut state, P1, "Walking Ballista", Zone::Library);
    let raze = put(&mut state, P1, "Raze", Zone::Library);
    let target = put(&mut state, P1, "Island", Zone::Battlefield);
    attack_with(&mut state, vec![shinobi], |_, _| None);
    pool(&mut state, P0, &[], 10);
    cast(&mut state, ballista, &[]);
    // No X choice is allowed, even when mana is available for a positive X.
    settled(&mut state);
    assert_eq!(zone(&state, ballista), Zone::Graveyard);
    assert_eq!(state.players[0].mana_pool[ManaColor::C.pool_index()], 10);
    assert!(!castable(&mut state, raze), "no land to sacrifice");
    let sacrifice = put(&mut state, P0, "Swamp", Zone::Hand);
    play_land(&mut state, sacrifice);
    cast(&mut state, raze, &[Target::Object(target)]);
    settle_choosing(&mut state, sacrifice);
    assert_eq!(zone(&state, sacrifice), Zone::Graveyard);
    assert_eq!(zone(&state, target), Zone::Graveyard);
    assert!(state.players[1].graveyard.contains(&raze));
}

#[test]
fn fallen_shinobi_empty_and_one_card_libraries_exile_only_available_cards() {
    for count in 0..=1 {
        let mut state = plains();
        let shinobi = ready_shinobi(&mut state);
        let library = state.players[1].library.clone();
        event::propose_and_commit_batch(
            &mut state,
            library
                .iter()
                .map(|&card| ProposedEvent::zone_change(card, Zone::Graveyard))
                .collect(),
        );
        settled(&mut state);
        let last = (count == 1).then(|| put(&mut state, P1, "Lightning Bolt", Zone::Library));
        attack_with(&mut state, vec![shinobi], |_, _| None);
        assert!(state.players[1].library.is_empty());
        assert_eq!(state.engine.exile_play_permissions.len(), count);
        assert_eq!(life(&state, P1), 15);
        if let Some(card) = last {
            assert_eq!(zone(&state, card), Zone::Exile);
            assert!(castable(&mut state, card));
        }
    }
}

fn cast_ivora(state: &mut GameState, player: PlayerId) -> ObjectId {
    state.priority_player = player;
    let creature = put(state, player, C6A_CARDS[0], Zone::Hand);
    pool(state, player, &[ManaColor::R], 1);
    cast(state, creature, &[]);
    settled(state);
    assert_eq!(zone(state, creature), Zone::Battlefield);
    creature
}

fn on_the_stack(state: &mut GameState) {
    loop {
        match next(state) {
            Decision::CastSpellOrPass { .. } => return,
            Decision::OrderTriggers { pending, .. } => {
                act(state, Action::OrderTriggers((0..pending.len()).collect()));
            }
            other => act(state, first_answer(other)),
        }
    }
}

fn cycle(state: &mut GameState, player: PlayerId) {
    state.priority_player = player;
    let hollow = put(state, player, "Hollow One", Zone::Hand);
    pool(state, player, &[], 2);
    activate(state, hollow, 0, &[]);
    settled(state);
    assert_eq!(zone(state, hollow), Zone::Graveyard);
}

fn hymn(state: &mut GameState, target: PlayerId) {
    state.priority_player = state.active_player;
    let caster = state.active_player;
    let spell = put(state, caster, "Hymn to Tourach", Zone::Hand);
    pool(state, caster, &[ManaColor::B, ManaColor::B], 0);
    cast(state, spell, &[Target::Player(target)]);
    settled(state);
}

#[test]
fn ivora_creates_blood_on_entry_and_combat_damage_and_blood_discards_before_drawing() {
    let mut state = plains();
    let ivora = cast_ivora(&mut state, P0);
    let blood = named(&state, P0, "Blood Token")[0];
    assert_eq!(named(&state, P0, "Blood Token").len(), 1);
    let discard = put(&mut state, P0, "Mountain", Zone::Hand);
    assert!(!activatable(&mut state, blood, 0), "requires one mana");
    pool(&mut state, P0, &[], 1);
    let drawn = state.players[0].library[0];
    activate(&mut state, blood, 0, &[]);
    on_the_stack(&mut state);
    assert_ne!(zone(&state, blood), Zone::Battlefield);
    assert_eq!(zone(&state, discard), Zone::Graveyard);
    assert_eq!(zone(&state, drawn), Zone::Library, "draw has not resolved");
    assert_eq!(mana(&state, P0, ManaColor::C), 0);
    settled(&mut state);
    assert_eq!(zone(&state, drawn), Zone::Hand);
    assert_eq!(power_toughness(&state, ivora), (2, 2));
    state.objects.get_mut(ivora).summoning_sick = false;
    attack_with(&mut state, vec![ivora], |_, _| None);
    assert_eq!(named(&state, P0, "Blood Token").len(), 1);
    assert_eq!(life(&state, P1), 18);
    let turn = state.turn;
    pass_until(&mut state, |s| {
        s.active_player == P0 && s.turn != turn && s.step == Step::Main1
    });
    attack_with(&mut state, vec![ivora], |_, _| None);
    assert_eq!(named(&state, P0, "Blood Token").len(), 2);
    assert_eq!(life(&state, P1), 16);
}

#[test]
fn ivora_blood_cannot_activate_without_a_discardable_card_or_when_tapped() {
    let mut state = plains();
    cast_ivora(&mut state, P0);
    let blood = named(&state, P0, "Blood Token")[0];
    pool(&mut state, P0, &[], 1);
    assert!(!activatable(&mut state, blood, 0), "empty hand");
    put(&mut state, P0, "Mountain", Zone::Hand);
    event::propose_and_commit(&mut state, ProposedEvent::tap(blood));
    settled(&mut state);
    assert!(!activatable(&mut state, blood, 0), "tapped Blood");
}

#[test]
fn ivora_tramples_over_a_blocker_and_does_not_make_blood_for_only_creature_damage() {
    for growing in [false, true] {
        let mut state = plains();
        let ivora = cast_ivora(&mut state, P0);
        let blocker = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
        if growing {
            put(&mut state, P0, "Mountain", Zone::Hand);
            put(&mut state, P0, "Plains", Zone::Hand);
            hymn(&mut state, P0);
        }
        state.objects.get_mut(ivora).summoning_sick = false;
        attack_with(&mut state, vec![ivora], |_, decision| match decision {
            Decision::DeclareBlockers { .. } => {
                Some(Action::DeclareBlockers(vec![(blocker, ivora)]))
            }
            Decision::ChooseCombatDamageRange { .. } => {
                Some(Action::ChooseCombatDamageRange { upper_half: false })
            }
            _ => None,
        });
        assert_eq!(
            named(&state, P0, "Blood Token").len(),
            if growing { 2 } else { 1 }
        );
        assert_eq!(life(&state, P1), if growing { 18 } else { 20 });
        assert_eq!(zone(&state, blocker), Zone::Graveyard);
    }
}

#[test]
fn ivora_ignores_noncombat_player_damage_and_another_creatures_combat_damage() {
    let mut state = plains();
    let ivora = cast_ivora(&mut state, P0);
    event::propose_and_commit(
        &mut state,
        ProposedEvent::damage(ivora, Target::Player(P1), 1),
    );
    settled(&mut state);
    assert_eq!(named(&state, P0, "Blood Token").len(), 1);
    assert_eq!(life(&state, P1), 19);
    let bear = put(&mut state, P0, "Wild Mongrel", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 1);
    cast(&mut state, bear, &[]);
    settled(&mut state);
    assert_eq!(
        named(&state, P0, "Blood Token").len(),
        1,
        "another creature entering"
    );
    state.objects.get_mut(bear).summoning_sick = false;
    attack_with(&mut state, vec![bear], |_, _| None);
    assert_eq!(named(&state, P0, "Blood Token").len(), 1);
    assert_eq!(life(&state, P1), 17);
}

#[test]
fn ivora_counts_each_discarded_card_but_not_an_empty_hand_or_opponents_discard() {
    for player in [P0, P1] {
        let mut state = plains();
        state.active_player = player;
        state.priority_player = player;
        let ivora = cast_ivora(&mut state, player);
        let cards = [
            put(&mut state, player, "Mountain", Zone::Hand),
            put(&mut state, player, "Plains", Zone::Hand),
        ];
        hymn(&mut state, player);
        assert!(cards
            .into_iter()
            .all(|card| zone(&state, card) == Zone::Graveyard));
        assert_eq!(power_toughness(&state, ivora), (3, 3));
        hymn(&mut state, player);
        assert_eq!(
            power_toughness(&state, ivora),
            (3, 3),
            "empty hand is no discard"
        );
        put(&mut state, player.opponent(), "Mountain", Zone::Hand);
        put(&mut state, player.opponent(), "Plains", Zone::Hand);
        hymn(&mut state, player.opponent());
        assert_eq!(
            power_toughness(&state, ivora),
            (3, 3),
            "opponent's discard is ignored"
        );
    }
}

#[test]
fn ivora_counts_discard_self_costs_for_its_current_controller_only() {
    let mut state = plains();
    let ivora = cast_ivora(&mut state, P0);
    cycle(&mut state, P0);
    assert_eq!(power_toughness(&state, ivora), (2, 2));
    cycle(&mut state, P1);
    assert_eq!(power_toughness(&state, ivora), (2, 2));
}

#[test]
fn ivora_counter_does_not_follow_a_source_that_left_and_optionally_returned() {
    for returns in [false, true] {
        let mut state = plains();
        let ivora = cast_ivora(&mut state, P0);
        let hollow = put(&mut state, P0, "Hollow One", Zone::Hand);
        pool(&mut state, P0, &[], 2);
        activate(&mut state, hollow, 0, &[]);
        on_the_stack(&mut state);
        assert_eq!(zone(&state, hollow), Zone::Graveyard);
        event::propose_and_commit(&mut state, ProposedEvent::zone_change(ivora, Zone::Hand));
        if returns {
            event::propose_and_commit(
                &mut state,
                ProposedEvent::zone_change(ivora, Zone::Battlefield),
            );
        }
        settled(&mut state);
        assert_eq!(state.objects.get(ivora).counters.plus1_plus1, 0);
        assert_eq!(
            named(&state, P0, "Blood Token").len(),
            if returns { 2 } else { 1 }
        );
    }
}

#[test]
fn ivora_triggers_once_for_a_madness_discard_even_though_the_card_goes_to_exile() {
    let mut state = plains();
    let ivora = cast_ivora(&mut state, P0);
    let blood = named(&state, P0, "Blood Token")[0];
    let temper = put(&mut state, P0, "Fiery Temper", Zone::Hand);
    pool(&mut state, P0, &[], 1);
    activate(&mut state, blood, 0, &[]);
    let mut madness = 0;
    settle_with(&mut state, |state, decision| match decision {
        Decision::ChooseMadnessCast { card, .. } => {
            assert_eq!(card, temper);
            assert_eq!(zone(state, temper), Zone::Exile);
            madness += 1;
            Action::ChooseMadnessCast(false)
        }
        other => first_answer(other),
    });
    assert_eq!(madness, 1);
    assert_eq!(zone(&state, temper), Zone::Graveyard);
    assert_eq!(power_toughness(&state, ivora), (2, 2));
}

#[test]
fn ivora_counts_each_cleanup_discard_and_ignores_other_hand_zone_changes() {
    let mut state = plains();
    let ivora = cast_ivora(&mut state, P0);
    let moved = put(&mut state, P0, "Mountain", Zone::Hand);
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(moved, Zone::Graveyard),
    );
    settled(&mut state);
    assert_eq!(power_toughness(&state, ivora), (1, 1));
    for _ in 0..9 {
        put(&mut state, P0, "Mountain", Zone::Hand);
    }
    pass_until(&mut state, |s| {
        s.active_player == P1 && s.step == Step::Main1
    });
    assert_eq!(state.players[0].hand.len(), 7);
    assert_eq!(power_toughness(&state, ivora), (3, 3));
}

// ---- Gau and Laelia -------------------------------------------------------------

fn cast_c6a_creature(state: &mut GameState, player: PlayerId, name: &str) -> ObjectId {
    state.priority_player = player;
    let creature = put(state, player, name, Zone::Hand);
    pool(state, player, &[ManaColor::R], def(name).cost.generic);
    cast(state, creature, &[]);
    settled(state);
    assert_eq!(zone(state, creature), Zone::Battlefield);
    creature
}

fn end_step_settled(state: &mut GameState) {
    pass_until(state, |s| s.step == Step::End);
    settled(state);
}

fn next_turn_main(state: &mut GameState) {
    let active = state.active_player;
    pass_until(state, |s| {
        s.active_player != active && s.step == Step::Main1
    });
}

#[test]
fn gau_has_no_haste_and_grows_only_when_it_attacks() {
    let mut state = plains();
    let gau = cast_c6a_creature(&mut state, P0, C6A_CARDS[1]);
    assert!(state.objects.get(gau).summoning_sick);
    assert!(!keyword(&state, gau, Keywords::HASTE));
    pass_until(&mut state, |s| s.step == Step::DeclareAttackers);
    let Decision::DeclareAttackers { eligible, .. } = next(&mut state) else {
        panic!("expected attackers");
    };
    assert!(!eligible.contains(&gau));
    attack_with(&mut state, Vec::new(), |_, _| None);
    assert_eq!(power_toughness(&state, gau), (2, 2));
    next_turn_main(&mut state);
    next_turn_main(&mut state);
    assert!(!state.objects.get(gau).summoning_sick);
    attack_with(&mut state, vec![gau], |_, _| None);
    assert_eq!(power_toughness(&state, gau), (3, 3));
    assert_eq!(life(&state, P1), 17);
    end_step_settled(&mut state);
    assert_eq!(life(&state, P1), 17, "no graveyard departure, no burn");
}

#[test]
fn gau_observes_graveyard_departures_before_entry_to_every_destination_both_seats() {
    for player in [P0, P1] {
        for destination in [Zone::Hand, Zone::Battlefield, Zone::Library, Zone::Exile] {
            let mut state = plains();
            state.active_player = player;
            state.priority_player = player;
            let gau = put(&mut state, player, C6A_CARDS[1], Zone::Hand);
            let card = put(&mut state, player, "Mountain", Zone::Graveyard);
            // Ownership of the graveyard matters, not a stale controller field.
            state.objects.get_mut(card).controller = player.opponent();
            event::propose_and_commit(&mut state, ProposedEvent::zone_change(card, destination));
            settled(&mut state);
            pool(&mut state, player, &[ManaColor::R], 1);
            cast(&mut state, gau, &[]);
            settled(&mut state);
            assert_eq!(zone(&state, card), destination);
            end_step_settled(&mut state);
            assert_eq!(life(&state, player), 20);
            assert_eq!(life(&state, player.opponent()), 18);
        }
    }
}

#[test]
fn gau_end_steps_track_each_players_turn_and_ignore_opponents_graveyard() {
    let mut state = plains();
    let gau = cast_c6a_creature(&mut state, P0, C6A_CARDS[1]);
    let other = put(&mut state, P1, "Mountain", Zone::Graveyard);
    event::propose_and_commit(&mut state, ProposedEvent::zone_change(other, Zone::Exile));
    let hand = put(&mut state, P0, "Mountain", Zone::Hand);
    event::propose_and_commit(&mut state, ProposedEvent::zone_change(hand, Zone::Exile));
    end_step_settled(&mut state);
    assert_eq!(life(&state, P1), 20);
    // A departure after this end step began cannot retroactively trigger Gau.
    let late = put(&mut state, P0, "Mountain", Zone::Graveyard);
    event::propose_and_commit(&mut state, ProposedEvent::zone_change(late, Zone::Exile));
    settled(&mut state);
    assert_eq!(life(&state, P1), 20);
    next_turn_main(&mut state);
    end_step_settled(&mut state);
    assert_eq!(life(&state, P1), 20, "previous player's turn is expired");
    next_turn_main(&mut state);
    let own = put(&mut state, P0, "Plains", Zone::Graveyard);
    event::propose_and_commit(&mut state, ProposedEvent::zone_change(own, Zone::Hand));
    end_step_settled(&mut state);
    assert_eq!(life(&state, P1), 18);
    next_turn_main(&mut state);
    let on_their_turn = put(&mut state, P0, "Plains", Zone::Graveyard);
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(on_their_turn, Zone::Exile),
    );
    end_step_settled(&mut state);
    assert_eq!(
        life(&state, P1),
        16,
        "also triggers at the opponent's end step"
    );
    assert_eq!(power_toughness(&state, gau), (2, 2));
    next_turn_main(&mut state);
    end_step_settled(&mut state);
    assert_eq!(life(&state, P1), 16, "a new turn needs a new departure");
}

#[test]
fn gau_counts_a_card_cast_from_the_graveyard() {
    let mut state = plains();
    cast_c6a_creature(&mut state, P0, C6A_CARDS[1]);
    let firebolt = put(&mut state, P0, "Firebolt", Zone::Graveyard);
    pool(&mut state, P0, &[ManaColor::R], 4);
    cast(&mut state, firebolt, &[Target::Player(P1)]);
    settled(&mut state);
    assert_eq!(zone(&state, firebolt), Zone::Exile);
    assert_eq!(life(&state, P1), 18);
    end_step_settled(&mut state);
    assert_eq!(life(&state, P1), 16);
}

#[test]
fn gau_samples_current_power_and_uses_departure_power_even_after_a_blink() {
    for departure in [None, Some(Zone::Hand), Some(Zone::Battlefield)] {
        let mut state = plains();
        let gau = cast_c6a_creature(&mut state, P0, C6A_CARDS[1]);
        let card = put(&mut state, P0, "Plains", Zone::Graveyard);
        event::propose_and_commit(&mut state, ProposedEvent::zone_change(card, Zone::Exile));
        pass_until(&mut state, |s| s.step == Step::End && !s.stack.is_empty());
        let growth = put(&mut state, P0, "Mutagenic Growth", Zone::Hand);
        state.priority_player = P0;
        pool(&mut state, P0, &[ManaColor::G], 0);
        cast(&mut state, growth, &[Target::Object(gau)]);
        on_the_stack(&mut state);
        // Resolve the pump, stopping with the original end-step trigger waiting.
        pass_until(&mut state, |s| zone(s, growth) == Zone::Graveyard);
        assert_eq!(power_toughness(&state, gau), (4, 4));
        if let Some(destination) = departure {
            event::propose_and_commit(&mut state, ProposedEvent::zone_change(gau, Zone::Hand));
            if destination == Zone::Battlefield {
                event::propose_and_commit(
                    &mut state,
                    ProposedEvent::zone_change(gau, Zone::Battlefield),
                );
                assert_eq!(power_toughness(&state, gau), (2, 2));
            }
        }
        settled(&mut state);
        assert_eq!(life(&state, P1), 16);
        assert_eq!(life(&state, P0), 20);
    }
}

#[test]
fn gau_zero_or_negative_power_deals_no_damage() {
    for counters in [2, 3] {
        let mut state = plains();
        let gau = cast_c6a_creature(&mut state, P0, C6A_CARDS[1]);
        let card = put(&mut state, P0, "Plains", Zone::Graveyard);
        event::propose_and_commit(&mut state, ProposedEvent::zone_change(card, Zone::Exile));
        pass_until(&mut state, |s| s.step == Step::End && !s.stack.is_empty());
        // A toughness boost keeps a nonpositive-power creature alive.
        state
            .engine
            .until_end_of_turn
            .push(engine::UntilEndOfTurnEffect::ResolvedObjectEffect {
                object_id: gau,
                object_zone_change_count: state.objects.get(gau).zone_change_count,
                layer: engine::Layers::POWER_TOUGHNESS,
                timestamp: 1,
                duration: engine::EffectDuration::EndOfTurn,
                power: -counters,
                toughness: 1,
                grant_haste: false,
            });
        settled(&mut state);
        assert_eq!(life(&state, P1), 20);
    }
}

#[test]
fn laelia_haste_attack_exiles_and_grows_then_casts_the_exiled_creature_for_mana() {
    let mut state = plains();
    let laelia = cast_c6a_creature(&mut state, P0, C6A_CARDS[2]);
    let bear = put(&mut state, P0, "Wild Mongrel", Zone::Library);
    let opponents_top = state.players[1].library[0];
    assert!(keyword(&state, laelia, Keywords::HASTE));
    assert!(state.objects.get(laelia).summoning_sick);
    attack_with(&mut state, vec![laelia], |_, _| None);
    assert_eq!(zone(&state, bear), Zone::Exile);
    assert_eq!(zone(&state, opponents_top), Zone::Library);
    assert_eq!(power_toughness(&state, laelia), (3, 3));
    assert_eq!(life(&state, P1), 17);
    assert!(
        !castable(&mut state, bear),
        "permission does not waive mana"
    );
    pool(&mut state, P0, &[ManaColor::G], 1);
    cast(&mut state, bear, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, bear), Zone::Battlefield);
    assert_eq!(
        power_toughness(&state, laelia),
        (3, 3),
        "leaving exile is not another trigger"
    );
}

#[test]
fn laelia_impulse_allows_land_play_but_obeys_the_land_limit() {
    for land_already_played in [false, true] {
        let mut state = plains();
        let laelia = cast_c6a_creature(&mut state, P0, C6A_CARDS[2]);
        if land_already_played {
            let land = put(&mut state, P0, "Mountain", Zone::Hand);
            play_land(&mut state, land);
        }
        let top = state.players[0].library[0];
        attack_with(&mut state, vec![laelia], |_, _| None);
        let Decision::CastSpellOrPass { land_drops, .. } = next(&mut state) else {
            panic!("expected main-phase priority");
        };
        assert_eq!(land_drops.contains(&top), !land_already_played);
        if !land_already_played {
            act(&mut state, Action::PlayLand(top));
            settled(&mut state);
            assert_eq!(zone(&state, top), Zone::Battlefield);
        } else {
            assert_eq!(zone(&state, top), Zone::Exile);
        }
    }
}

#[test]
fn laelia_declined_impulse_expires_on_the_actual_next_players_turn() {
    let mut state = plains();
    let laelia = cast_c6a_creature(&mut state, P0, C6A_CARDS[2]);
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Library);
    attack_with(&mut state, vec![laelia], |_, _| None);
    pool(&mut state, P0, &[ManaColor::R], 0);
    assert!(castable(&mut state, bolt));
    end_step_settled(&mut state);
    pool(&mut state, P0, &[ManaColor::R], 0);
    assert!(
        castable(&mut state, bolt),
        "still playable during the end step"
    );
    next_turn_main(&mut state);
    assert_eq!(zone(&state, bolt), Zone::Exile);
    state.priority_player = P0;
    pool(&mut state, P0, &[ManaColor::R], 0);
    assert!(
        !castable(&mut state, bolt),
        "expired despite same round number"
    );
    next_turn_main(&mut state);
    pool(&mut state, P0, &[ManaColor::R], 0);
    assert!(
        !castable(&mut state, bolt),
        "does not return on the next own turn"
    );
}

#[test]
fn laelia_counts_separate_exile_instructions_in_one_resolution_both_seats() {
    use mtg_kernel::effect::{self, EffectOp, ExecCtx, ObjectRef};

    for player in [P0, P1] {
        let mut state = plains();
        state.active_player = player;
        state.priority_player = player;
        let laelia = cast_c6a_creature(&mut state, player, C6A_CARDS[2]);
        let library = put(&mut state, player, C6A_CARDS[0], Zone::Library);
        let graveyard = put(&mut state, player, C6A_CARDS[3], Zone::Graveyard);
        let mut ctx = ExecCtx::no_targets(laelia, player);
        ctx.targets = vec![Target::Object(library), Target::Object(graveyard)];
        // One resolution, two instructions, with no trigger collection between them.
        effect::execute(
            &EffectOp::Sequence(vec![
                EffectOp::MoveObject {
                    object: ObjectRef::Target(0),
                    to_zone: Zone::Exile,
                },
                EffectOp::MoveObject {
                    object: ObjectRef::Target(1),
                    to_zone: Zone::Exile,
                },
            ]),
            &ctx,
            &mut state,
        );
        assert_eq!(zone(&state, library), Zone::Exile);
        assert_eq!(zone(&state, graveyard), Zone::Exile);
        assert_eq!(state.objects.get(laelia).counters.plus1_plus1, 0);
        settled(&mut state);
        assert_eq!(state.objects.get(laelia).counters.plus1_plus1, 2);
    }
}

#[test]
fn laelia_counts_each_batch_and_a_separate_exile_before_one_collection_both_seats() {
    for player in [P0, P1] {
        let mut state = plains();
        state.active_player = player;
        state.priority_player = player;
        let laelia = cast_c6a_creature(&mut state, player, C6A_CARDS[2]);
        for _ in 0..2 {
            let library = put(&mut state, player, C6A_CARDS[0], Zone::Library);
            let graveyard = put(&mut state, player, C6A_CARDS[3], Zone::Graveyard);
            event::propose_and_commit_batch(
                &mut state,
                vec![
                    ProposedEvent::zone_change(library, Zone::Exile),
                    ProposedEvent::zone_change(graveyard, Zone::Exile),
                ],
            );
        }
        let last = put(&mut state, player, C6A_CARDS[1], Zone::Library);
        event::propose_and_commit(&mut state, ProposedEvent::zone_change(last, Zone::Exile));
        settled(&mut state);
        assert_eq!(state.objects.get(laelia).counters.plus1_plus1, 3);
    }
}

#[test]
fn laelia_counts_one_exile_batch_across_both_own_zones_and_ignores_other_moves_both_seats() {
    for player in [P0, P1] {
        let mut state = plains();
        state.active_player = player;
        state.priority_player = player;
        let laelia = cast_c6a_creature(&mut state, player, C6A_CARDS[2]);
        let library = put(&mut state, player, "Mountain", Zone::Library);
        let graveyard = put(&mut state, player, "Lightning Bolt", Zone::Graveyard);
        // An effect controlled by the opponent can exile a card you own.
        state.objects.get_mut(graveyard).controller = player.opponent();
        event::propose_and_commit_batch(
            &mut state,
            vec![
                ProposedEvent::zone_change(library, Zone::Exile),
                ProposedEvent::zone_change(graveyard, Zone::Exile),
            ],
        );
        settled(&mut state);
        assert_eq!(power_toughness(&state, laelia), (3, 3));
        let other_library = put(&mut state, player.opponent(), "Mountain", Zone::Library);
        let other_graveyard = put(&mut state, player.opponent(), "Plains", Zone::Graveyard);
        let hand = put(&mut state, player, "Mountain", Zone::Hand);
        let permanent = put(&mut state, player, "Plains", Zone::Battlefield);
        event::propose_and_commit_batch(
            &mut state,
            vec![
                ProposedEvent::zone_change(other_library, Zone::Exile),
                ProposedEvent::zone_change(other_graveyard, Zone::Exile),
                ProposedEvent::zone_change(hand, Zone::Exile),
                ProposedEvent::zone_change(permanent, Zone::Exile),
                ProposedEvent::zone_change(graveyard, Zone::Hand),
            ],
        );
        settled(&mut state);
        assert_eq!(power_toughness(&state, laelia), (3, 3));
        let own_graveyard = put(&mut state, player, "Plains", Zone::Graveyard);
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(own_graveyard, Zone::Exile),
        );
        settled(&mut state);
        assert_eq!(power_toughness(&state, laelia), (4, 4));
    }
}

#[test]
fn laelia_empty_library_attack_does_not_grow_or_create_permission() {
    let mut state = game(&[]);
    let laelia = cast_c6a_creature(&mut state, P0, C6A_CARDS[2]);
    attack_with(&mut state, vec![laelia], |_, _| None);
    assert_eq!(power_toughness(&state, laelia), (2, 2));
    assert_eq!(life(&state, P1), 18);
    assert!(state.engine.exile_play_permissions.is_empty());
}

#[test]
fn c6a_attack_counters_do_not_follow_a_departed_source_and_laelia_impulse_survives() {
    for name in [C6A_CARDS[1], C6A_CARDS[2]] {
        for returns in [false, true] {
            let mut state = plains();
            let source = cast_c6a_creature(&mut state, P0, name);
            if name == C6A_CARDS[1] {
                next_turn_main(&mut state);
                next_turn_main(&mut state);
            }
            let top = state.players[0].library[0];
            pass_until(&mut state, |s| s.step == Step::DeclareAttackers);
            act(&mut state, Action::DeclareAttackers(vec![source]));
            on_the_stack(&mut state);
            assert!(!state.stack.is_empty());
            event::propose_and_commit(&mut state, ProposedEvent::zone_change(source, Zone::Hand));
            if returns {
                event::propose_and_commit(
                    &mut state,
                    ProposedEvent::zone_change(source, Zone::Battlefield),
                );
            }
            settled(&mut state);
            // A returned Laelia sees the later exile as a new permanent.
            assert_eq!(
                state.objects.get(source).counters.plus1_plus1,
                i32::from(name == C6A_CARDS[2] && returns)
            );
            assert_eq!(
                zone(&state, top),
                if name == C6A_CARDS[2] {
                    Zone::Exile
                } else {
                    Zone::Library
                }
            );
            if name == C6A_CARDS[2] {
                pass_until(&mut state, |s| s.step == Step::Main2);
                play_land(&mut state, top);
                assert_eq!(zone(&state, top), Zone::Battlefield);
            }
        }
    }
}

#[test]
fn laelia_exile_counter_does_not_follow_a_blinked_source() {
    let mut state = plains();
    let laelia = cast_c6a_creature(&mut state, P0, C6A_CARDS[2]);
    let grave = put(&mut state, P0, "Mountain", Zone::Graveyard);
    event::propose_and_commit(&mut state, ProposedEvent::zone_change(grave, Zone::Exile));
    on_the_stack(&mut state);
    assert!(!state.stack.is_empty());
    event::propose_and_commit(&mut state, ProposedEvent::zone_change(laelia, Zone::Hand));
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(laelia, Zone::Battlefield),
    );
    settled(&mut state);
    assert_eq!(power_toughness(&state, laelia), (2, 2));
}

#[test]
fn gau_freezes_power_before_simultaneous_departure_of_its_aura() {
    let mut state = plains();
    let gau = cast_c6a_creature(&mut state, P0, C6A_CARDS[1]);
    let rancor = put(&mut state, P0, "Rancor", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 0);
    cast(&mut state, rancor, &[Target::Object(gau)]);
    settled(&mut state);
    assert_eq!(power_toughness(&state, gau), (4, 2));
    let grave = put(&mut state, P0, "Mountain", Zone::Graveyard);
    event::propose_and_commit(&mut state, ProposedEvent::zone_change(grave, Zone::Hand));
    pass_until(&mut state, |s| s.step == Step::End && !s.stack.is_empty());
    event::propose_and_commit_batch(
        &mut state,
        vec![
            ProposedEvent::zone_change(rancor, Zone::Hand),
            ProposedEvent::zone_change(gau, Zone::Hand),
        ],
    );
    settled(&mut state);
    assert_eq!(life(&state, P1), 16);
}

#[test]
fn opponents_crypt_exiles_multiple_cards_once_for_laelia_and_enables_gau() {
    let mut state = plains();
    cast_c6a_creature(&mut state, P0, C6A_CARDS[1]);
    let laelia = cast_c6a_creature(&mut state, P0, C6A_CARDS[2]);
    let cards = [
        put(&mut state, P0, "Mountain", Zone::Graveyard),
        put(&mut state, P0, "Lightning Bolt", Zone::Graveyard),
    ];
    let crypt = put(&mut state, P1, "Tormod's Crypt", Zone::Hand);
    next_turn_main(&mut state);
    cast(&mut state, crypt, &[]);
    settled(&mut state);
    activate(&mut state, crypt, 0, &[Target::Player(P0)]);
    settled(&mut state);
    assert_eq!(zone(&state, crypt), Zone::Graveyard);
    assert!(cards
        .into_iter()
        .all(|card| zone(&state, card) == Zone::Exile));
    assert_eq!(power_toughness(&state, laelia), (3, 3));
    end_step_settled(&mut state);
    assert_eq!(life(&state, P1), 18);
}

#[test]
fn laelia_does_not_impulse_when_only_another_creature_attacks() {
    let mut state = plains();
    let laelia = cast_c6a_creature(&mut state, P0, C6A_CARDS[2]);
    let other = put(&mut state, P0, "Wild Mongrel", Zone::Battlefield);
    let top = state.players[0].library[0];
    attack_with(&mut state, vec![other], |_, _| None);
    assert_eq!(zone(&state, top), Zone::Library);
    assert_eq!(power_toughness(&state, laelia), (2, 2));
    assert!(state.engine.exile_play_permissions.is_empty());
}

#[test]
fn laelia_impulse_permission_does_not_follow_a_card_that_leaves_and_returns_to_exile() {
    let mut state = plains();
    let laelia = cast_c6a_creature(&mut state, P0, C6A_CARDS[2]);
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Library);
    attack_with(&mut state, vec![laelia], |_, _| None);
    pool(&mut state, P0, &[ManaColor::R], 0);
    assert!(castable(&mut state, bolt));
    state.priority_player = P1;
    pool(&mut state, P1, &[ManaColor::R], 0);
    assert!(
        !castable(&mut state, bolt),
        "only the trigger's controller may play it"
    );
    state.priority_player = P0;
    event::propose_and_commit(&mut state, ProposedEvent::zone_change(bolt, Zone::Hand));
    event::propose_and_commit(&mut state, ProposedEvent::zone_change(bolt, Zone::Exile));
    settled(&mut state);
    assert!(!castable(&mut state, bolt));
}

// ---- Generous Plunderer and Gut ------------------------------------------------

fn plunderer_next_upkeep(state: &mut GameState, player: PlayerId) {
    pass_until(state, |s| s.active_player != player);
    pass_until(state, |s| {
        s.active_player == player && s.step == Step::Upkeep
    });
}

#[test]
fn plunderer_upkeep_is_optional_controller_only_and_gives_a_separate_tapped_treasure() {
    for player in [P0, P1] {
        for accept in [false, true] {
            let mut state = plains();
            state.active_player = player;
            state.priority_player = player;
            let plunderer = cast_c6a_creature(&mut state, player, C6A_CARDS[4]);
            assert!(keyword(&state, plunderer, Keywords::MENACE));
            assert!(state.objects.get(plunderer).summoning_sick);
            next_turn_main(&mut state);
            assert!(named(&state, player, "Treasure Token").is_empty());
            assert!(named(&state, player.opponent(), "Treasure Token").is_empty());
            pass_until(&mut state, |s| {
                s.active_player == player && s.step == Step::Upkeep
            });
            let choice = settle(&mut state);
            assert!(matches!(choice,
                Some(Decision::ChooseEffectOption { player: chooser, .. }) if chooser == player
            ));
            act(
                &mut state,
                Action::ChooseEffectOption(if accept { 0 } else { 1 }),
            );
            if accept {
                match settle(&mut state) {
                    Some(Decision::ChooseTargets {
                        player: chooser,
                        legal_targets,
                        ..
                    }) => {
                        assert_eq!(chooser, player);
                        assert_eq!(legal_targets, vec![Target::Player(player.opponent())]);
                    }
                    other => panic!("expected reflexive target, got {other:?}"),
                }
                let ours = named(&state, player, "Treasure Token");
                assert_eq!(ours.len(), 1);
                assert!(!state.objects.get(ours[0]).tapped);
                assert!(named(&state, player.opponent(), "Treasure Token").is_empty());
                act(
                    &mut state,
                    Action::ChooseTarget(Target::Player(player.opponent())),
                );
                on_the_stack(&mut state);
                assert_eq!(state.stack.len(), 1, "the gift can be responded to");
                settled(&mut state);
                let theirs = named(&state, player.opponent(), "Treasure Token");
                assert_eq!(theirs.len(), 1);
                assert!(state.objects.get(theirs[0]).tapped);
                assert_eq!(state.objects.get(theirs[0]).owner, player.opponent());
            } else {
                settled(&mut state);
                assert!(named(&state, player, "Treasure Token").is_empty());
                assert!(named(&state, player.opponent(), "Treasure Token").is_empty());
            }
        }
    }
}

#[test]
fn plunderer_gift_survives_its_source_but_rechecks_opponent_hexproof() {
    for hexproof in [false, true] {
        let mut state = plains();
        let plunderer = cast_c6a_creature(&mut state, P0, C6A_CARDS[4]);
        plunderer_next_upkeep(&mut state, P0);
        assert!(matches!(
            settle(&mut state),
            Some(Decision::ChooseEffectOption { .. })
        ));
        act(&mut state, Action::ChooseEffectOption(0));
        assert!(matches!(
            settle(&mut state),
            Some(Decision::ChooseTargets { .. })
        ));
        act(&mut state, Action::ChooseTarget(Target::Player(P1)));
        on_the_stack(&mut state);
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(plunderer, Zone::Graveyard),
        );
        if hexproof {
            put(&mut state, P1, "Leyline of Sanctity", Zone::Battlefield);
        }
        settled(&mut state);
        assert_eq!(named(&state, P0, "Treasure Token").len(), 1);
        assert_eq!(
            named(&state, P1, "Treasure Token").len(),
            usize::from(!hexproof)
        );
    }
}

#[test]
fn plunderer_can_create_its_treasure_even_when_no_opponent_can_be_targeted() {
    let mut state = plains();
    cast_c6a_creature(&mut state, P0, C6A_CARDS[4]);
    put(&mut state, P1, "Leyline of Sanctity", Zone::Battlefield);
    plunderer_next_upkeep(&mut state, P0);
    assert!(matches!(
        settle(&mut state),
        Some(Decision::ChooseEffectOption { .. })
    ));
    act(&mut state, Action::ChooseEffectOption(0));
    settled(&mut state);
    assert_eq!(named(&state, P0, "Treasure Token").len(), 1);
    assert!(named(&state, P1, "Treasure Token").is_empty());
}

fn ready_plunderer(state: &mut GameState) -> ObjectId {
    let source = cast_c6a_creature(state, P0, C6A_CARDS[4]);
    next_turn_main(state);
    run_until(
        state,
        |s| s.active_player == P0 && s.step == Step::Main1,
        |_, choice| {
            matches!(choice, Decision::ChooseEffectOption { .. })
                .then_some(Action::ChooseEffectOption(1))
        },
    );
    source
}

#[test]
fn plunderer_attack_counts_opponents_current_artifacts_even_tapped_and_after_source_leaves() {
    for artifact_count in [0, 2] {
        let mut state = plains();
        let source = ready_plunderer(&mut state);
        token(&mut state, P0, "Treasure Token");
        let gone = token(&mut state, P1, "Treasure Token");
        pass_until(&mut state, |s| s.step == Step::DeclareAttackers);
        act(&mut state, Action::DeclareAttackers(vec![source]));
        on_the_stack(&mut state);
        assert_eq!(state.stack.len(), 1);
        event::propose_and_commit(&mut state, ProposedEvent::zone_change(gone, Zone::Exile));
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(source, Zone::Graveyard),
        );
        for _ in 0..artifact_count {
            let treasure = token(&mut state, P1, "Treasure Token");
            state.objects.get_mut(treasure).tapped = true;
        }
        settled(&mut state);
        assert_eq!(life(&state, P1), 20 - artifact_count);
    }
    let mut state = plains();
    cast_c6a_creature(&mut state, P0, C6A_CARDS[4]);
    let bear = put(&mut state, P0, "Wild Mongrel", Zone::Battlefield);
    token(&mut state, P1, "Treasure Token");
    attack_with(&mut state, vec![bear], |_, _| None);
    assert_eq!(
        life(&state, P1),
        18,
        "another attack does not trigger Plunderer"
    );
}

#[test]
fn plunderer_menace_requires_two_blockers() {
    let mut state = plains();
    let source = ready_plunderer(&mut state);
    let first = put(&mut state, P1, "Wild Mongrel", Zone::Battlefield);
    let second = put(&mut state, P1, "Wild Mongrel", Zone::Battlefield);
    pass_until(&mut state, |s| s.step == Step::DeclareAttackers);
    act(&mut state, Action::DeclareAttackers(vec![source]));
    pass_until(&mut state, |s| s.step == Step::DeclareBlockers);
    assert!(engine::step(&mut state, Action::DeclareBlockers(vec![(first, source)])).is_err());
    act(
        &mut state,
        Action::DeclareBlockers(vec![(first, source), (second, source)]),
    );
}

#[test]
fn gut_sacrifices_another_creature_or_artifact_once_per_attack_and_makes_an_attacking_skeleton() {
    for sacrifice_name in ["Wild Mongrel", "Treasure Token", "Hollow One"] {
        let mut state = plains();
        let gut = cast_c6a_creature(&mut state, P0, C6A_CARDS[5]);
        let first = put(&mut state, P0, "Wild Mongrel", Zone::Battlefield);
        let second = put(&mut state, P0, "Wild Mongrel", Zone::Battlefield);
        let sacrifice = if sacrifice_name == "Treasure Token" {
            token(&mut state, P0, sacrifice_name)
        } else {
            put(&mut state, P0, sacrifice_name, Zone::Battlefield)
        };
        let land = put(&mut state, P0, "Mountain", Zone::Battlefield);
        let enemy = put(&mut state, P1, "Hollow One", Zone::Battlefield);
        let mut selections = 0;
        pass_until(&mut state, |s| s.step == Step::DeclareAttackers);
        act(&mut state, Action::DeclareAttackers(vec![first, second]));
        run_until(
            &mut state,
            |s| s.step == Step::DeclareBlockers,
            |_, choice| {
                if let Decision::ChooseEffectTargets {
                    player,
                    legal_targets,
                    ..
                } = choice
                {
                    selections += 1;
                    assert_eq!(*player, P0);
                    assert!(legal_targets.contains(&Target::Object(sacrifice)));
                    for excluded in [gut, land, enemy] {
                        assert!(!legal_targets.contains(&Target::Object(excluded)));
                    }
                    return Some(Action::ChooseEffectTarget(Target::Object(sacrifice)));
                }
                None
            },
        );
        assert_eq!(selections, 1, "several attackers still give one trigger");
        assert_ne!(zone(&state, sacrifice), Zone::Battlefield);
        assert_eq!(zone(&state, gut), Zone::Battlefield);
        let skeletons = named(&state, P0, "Skeleton Token");
        assert_eq!(
            skeletons.len(),
            1,
            "the entering attacker does not retrigger Gut"
        );
        let skeleton = skeletons[0];
        assert!(state.objects.get(skeleton).tapped);
        assert!(state.engine.combat.attackers.contains(&skeleton));
        assert_eq!(power_toughness(&state, skeleton), (4, 1));
        assert!(keyword(&state, skeleton, Keywords::MENACE));
        assert!(
            engine::step(&mut state, Action::DeclareBlockers(vec![(enemy, skeleton)])).is_err()
        );
        act(&mut state, Action::DeclareBlockers(Vec::new()));
        pass_until(&mut state, |s| s.step == Step::Main2);
        assert_eq!(life(&state, P1), 12);
    }
}

#[test]
fn gut_allows_decline_and_has_no_offer_without_another_creature_or_artifact() {
    for have_candidate in [false, true] {
        let mut state = plains();
        let gut = cast_c6a_creature(&mut state, P0, C6A_CARDS[5]);
        next_turn_main(&mut state);
        next_turn_main(&mut state);
        let candidate =
            have_candidate.then(|| put(&mut state, P0, "Wild Mongrel", Zone::Battlefield));
        let mut offers = 0;
        attack_with(&mut state, vec![gut], |_, choice| {
            if let Decision::ChooseEffectTargets { can_finish, .. } = choice {
                offers += 1;
                assert!(*can_finish);
                return Some(Action::FinishEffectSelection);
            }
            None
        });
        assert_eq!(offers, usize::from(have_candidate));
        assert!(named(&state, P0, "Skeleton Token").is_empty());
        assert_eq!(life(&state, P1), 18);
        if let Some(candidate) = candidate {
            assert_eq!(zone(&state, candidate), Zone::Battlefield);
        }
    }
}

#[test]
fn gut_does_not_trigger_for_empty_attacks_or_opponents_attacks() {
    let mut state = plains();
    cast_c6a_creature(&mut state, P0, C6A_CARDS[5]);
    token(&mut state, P0, "Treasure Token");
    attack_with(&mut state, Vec::new(), |_, choice| {
        assert!(!matches!(choice, Decision::ChooseEffectTargets { .. }));
        None
    });
    assert!(named(&state, P0, "Skeleton Token").is_empty());
    next_turn_main(&mut state);
    let attacker = put(&mut state, P1, "Wild Mongrel", Zone::Battlefield);
    attack_with(&mut state, vec![attacker], |_, choice| {
        assert!(!matches!(choice, Decision::ChooseEffectTargets { .. }));
        None
    });
    assert_eq!(life(&state, P0), 18);
    assert!(named(&state, P0, "Skeleton Token").is_empty());
}

#[test]
fn gut_can_sacrifice_itself_as_an_artifact_and_still_creates_the_skeleton() {
    let mut state = plains();
    let gut = cast_c6a_creature(&mut state, P0, C6A_CARDS[5]);
    let coating = put(&mut state, P0, "Liquimetal Coating", Zone::Battlefield);
    activate(&mut state, coating, 0, &[Target::Object(gut)]);
    settled(&mut state);
    let attacker = put(&mut state, P0, "Wild Mongrel", Zone::Battlefield);
    attack_with(&mut state, vec![attacker], |_, choice| match choice {
        Decision::ChooseEffectTargets { legal_targets, .. } => {
            assert!(legal_targets.contains(&Target::Object(gut)));
            Some(Action::ChooseEffectTarget(Target::Object(gut)))
        }
        _ => None,
    });
    assert_eq!(zone(&state, gut), Zone::Graveyard);
    assert_eq!(named(&state, P0, "Skeleton Token").len(), 1);
    assert_eq!(life(&state, P1), 14);
}

#[test]
fn gut_selects_at_resolution_and_a_returned_source_is_another_creature() {
    for return_source in [false, true] {
        let mut state = plains();
        let gut = cast_c6a_creature(&mut state, P0, C6A_CARDS[5]);
        let attacker = put(&mut state, P0, "Wild Mongrel", Zone::Battlefield);
        pass_until(&mut state, |s| s.step == Step::DeclareAttackers);
        act(&mut state, Action::DeclareAttackers(vec![attacker]));
        on_the_stack(&mut state);
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(attacker, Zone::Graveyard),
        );
        if return_source {
            event::propose_and_commit(&mut state, ProposedEvent::zone_change(gut, Zone::Exile));
            event::propose_and_commit(
                &mut state,
                ProposedEvent::zone_change(gut, Zone::Battlefield),
            );
            settle_choosing(&mut state, gut);
            assert_eq!(zone(&state, gut), Zone::Graveyard);
        } else {
            settled(&mut state);
        }
        assert_eq!(
            named(&state, P0, "Skeleton Token").len(),
            usize::from(return_source)
        );
    }
}

// ---- Public sessions: all six completed cards, 12 games and 12 replays ---------

#[derive(Default)]
struct C6aEvents {
    casts: [usize; C6A_CARDS.len()],
}

impl C6aEvents {
    fn observe(&mut self, _decision: &Value, action: &Value) {
        let semantic = &action["semantic"];
        if semantic["action_kind"].as_str() != Some("cast_spell") {
            return;
        }
        let Some(card_id) = semantic["source"]["card_db_id"]
            .as_u64()
            .and_then(|id| usize::try_from(id).ok())
        else {
            return;
        };
        if let Some(index) = C6A_CARDS
            .iter()
            .position(|&name| name == CARD_DEFS[card_id].name)
        {
            self.casts[index] += 1;
        }
    }
}

#[test]
fn c6a_decks_finish_and_replay_through_public_sessions_both_seats() {
    let red = deck(&[
        ("Mountain", 24),
        ("Jackal Pup", 6),
        ("Ivora, Insatiable Heir", 6),
        ("Gau, Feral Youth", 6),
        ("Laelia, the Blade Reforged", 6),
        ("Generous Plunderer", 6),
        ("Gut, True Soul Zealot", 6),
    ]);
    let ninjas = deck(&[
        ("Underground Sea", 12),
        ("Island", 4),
        ("Swamp", 4),
        ("Faerie Seer", 12),
        ("Fallen Shinobi", 12),
    ]);
    let mut totals = C6aEvents::default();
    for (pair, first_seed) in [
        ([red.clone(), ninjas.clone()], 0x4336_4100_u64),
        ([ninjas, red], 0x4336_4110_u64),
    ] {
        for seed in first_seed..first_seed + 3 {
            for policy in [Policy::Ranked, Policy::Random(seed + 0x100)] {
                let game =
                    play_observed(pair.clone(), None, seed, policy, &mut |decision, action| {
                        totals.observe(decision, action)
                    });
                let replay = play_observed(pair.clone(), None, seed, policy, &mut |_, _| {});
                assert_eq!(game.transcript_sha256, replay.transcript_sha256);
            }
        }
    }
    for (name, casts) in C6A_CARDS.into_iter().zip(totals.casts) {
        assert!(casts > 0, "{name} never cast through the public session");
    }
}
