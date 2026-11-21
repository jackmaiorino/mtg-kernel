//! Pauper field wave Pf1a: native rules and public-session tests for the
//! permanents with simple text.
#![cfg(feature = "eternal-fixtures")]
#![allow(dead_code)]

use crate::eternal_catalog_v1::{play_observed, Policy};
use mtg_kernel::card_def::{
    card_id_by_name, CardCapability, CardDef, CardType, Keywords, Subtype, CARD_DEFS,
};
use mtg_kernel::engine::{self, Action, Decision, OptionalCostChoice};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::limited_session_v1::{CustomCardCountV1, CustomDeckV1};
use mtg_kernel::mana::ManaColor;
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Target, Zone};
use serde_json::Value;

const P0: PlayerId = PlayerId::P0;
const P1: PlayerId = PlayerId::P1;

const PF1A_CARDS: [&str; 10] = [
    "Wooded Ridgeline",
    "Kabira Crossroads",
    "Jaspera Sentinel",
    "Kor Sanctifiers",
    "Vulshok Sorcerer",
    "Primordial Pachyderm",
    "Lifelink",
    "Mask of Law and Grace",
    "Mycosynth Wellspring",
    "K'un-Lun Warrior",
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
        0x5046_0000_0000 + 0x3161,
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

/// Plays on until the pending activation is on the stack (or finished) and
/// P0 holds priority.
fn on_the_stack(state: &mut GameState) {
    for _ in 0..20 {
        match next(state) {
            Decision::CastSpellOrPass { .. } => return,
            other => {
                let action = first_answer(other);
                act(state, action);
            }
        }
    }
    panic!("the activation did not finish");
}

/// Puts `aura` from P0's hand onto `host` by casting it.
fn enchant(state: &mut GameState, aura: ObjectId, host: ObjectId) {
    cast(state, aura, &[Target::Object(host)]);
    settle_first(state);
    assert_eq!(zone(state, aura), Zone::Battlefield);
    assert_eq!(
        state
            .objects
            .get(aura)
            .v4
            .attached_to
            .map(|link| link.object),
        Some(host)
    );
}

/// Settles, answering effect options with `option` and choosing `object`
/// whenever a card selection (still empty) or target offers it; otherwise
/// the first legal answer.
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

/// Settles a kicker cast: the kicker is paid or not as `kicked` says, and
/// `target` is chosen whenever a target prompt offers it.
fn settle_kicker(state: &mut GameState, kicked: bool, target: Option<ObjectId>) {
    settle_with(state, |_, decision| match decision {
        Decision::ChooseKicker { .. } => Action::ChooseKicker(kicked),
        Decision::ChooseTargets { legal_targets, .. }
            if target.is_some_and(|object| legal_targets.contains(&Target::Object(object))) =>
        {
            Action::ChooseTarget(Target::Object(target.unwrap()))
        }
        other => first_answer(other),
    });
}

// ---- Characteristics -----------------------------------------------------------

#[test]
fn pf1a_cards_are_full() {
    for name in PF1A_CARDS {
        let card = def(name);
        assert_eq!(card.capability, CardCapability::Full, "{name}");
        assert!(card.is_executable(), "{name}");
    }
}

#[test]
fn pf1a_cards_have_their_printed_characteristics() {
    let ridgeline = def("Wooded Ridgeline");
    assert_eq!(ridgeline.types, &[CardType::Land]);
    assert_eq!(ridgeline.subtypes, &[Subtype::Mountain, Subtype::Forest]);
    assert!(ridgeline.enters_battlefield_tapped);
    assert_eq!(ridgeline.mana_value, 0);

    let crossroads = def("Kabira Crossroads");
    assert_eq!(crossroads.types, &[CardType::Land]);
    assert!(crossroads.subtypes.is_empty());
    assert!(crossroads.enters_battlefield_tapped);

    let sentinel = def("Jaspera Sentinel");
    assert_eq!(sentinel.types, &[CardType::Creature]);
    assert_eq!(sentinel.subtypes, &[Subtype::Elf, Subtype::Rogue]);
    assert_eq!(sentinel.colors, &[ManaColor::G]);
    assert_eq!(sentinel.mana_value, 1);
    assert_eq!((sentinel.power, sentinel.toughness), (Some(1), Some(2)));
    assert!(sentinel.keywords.has(Keywords::REACH));

    let sanctifiers = def("Kor Sanctifiers");
    assert_eq!(sanctifiers.types, &[CardType::Creature]);
    assert_eq!(sanctifiers.subtypes, &[Subtype::Kor, Subtype::Cleric]);
    assert_eq!(sanctifiers.colors, &[ManaColor::W]);
    assert_eq!(sanctifiers.mana_value, 3);
    assert_eq!(
        (sanctifiers.power, sanctifiers.toughness),
        (Some(2), Some(3))
    );
    assert!(sanctifiers.kicker_cost.is_some());

    let sorcerer = def("Vulshok Sorcerer");
    assert_eq!(sorcerer.types, &[CardType::Creature]);
    assert_eq!(
        sorcerer.subtypes,
        &[Subtype::Human, Subtype::Shaman, Subtype::Sorcerer]
    );
    assert_eq!(sorcerer.colors, &[ManaColor::R]);
    assert_eq!(sorcerer.mana_value, 3);
    assert_eq!((sorcerer.power, sorcerer.toughness), (Some(1), Some(1)));
    assert!(sorcerer.keywords.has(Keywords::HASTE));
    assert_eq!(sorcerer.activated_abilities.len(), 1);

    let pachyderm = def("Primordial Pachyderm");
    assert_eq!(pachyderm.types, &[CardType::Creature]);
    assert_eq!(pachyderm.subtypes, &[Subtype::Elephant, Subtype::Avatar]);
    assert_eq!(pachyderm.colors, &[ManaColor::G]);
    assert_eq!(pachyderm.mana_value, 4);
    assert_eq!((pachyderm.power, pachyderm.toughness), (Some(4), Some(4)));
    assert!(pachyderm.keywords.has(Keywords::REACH) && pachyderm.keywords.has(Keywords::TRAMPLE));

    for aura in ["Lifelink", "Mask of Law and Grace"] {
        let card = def(aura);
        assert_eq!(card.types, &[CardType::Enchantment], "{aura}");
        assert_eq!(card.subtypes, &[Subtype::Aura], "{aura}");
        assert_eq!(card.colors, &[ManaColor::W], "{aura}");
        assert_eq!(card.mana_value, 1, "{aura}");
        assert!(card.attachment.is_some(), "{aura}");
    }

    let wellspring = def("Mycosynth Wellspring");
    assert_eq!(wellspring.types, &[CardType::Artifact]);
    assert!(wellspring.colors.is_empty());
    assert_eq!(wellspring.mana_value, 2);

    let warrior = def("K'un-Lun Warrior");
    assert_eq!(warrior.types, &[CardType::Creature]);
    assert_eq!(
        warrior.subtypes,
        &[Subtype::Human, Subtype::Warrior, Subtype::Hero]
    );
    assert_eq!(warrior.colors, &[ManaColor::R]);
    assert_eq!(warrior.mana_value, 2);
    assert_eq!((warrior.power, warrior.toughness), (Some(2), Some(2)));
}

// ---- Lands -----------------------------------------------------------------------

#[test]
fn wooded_ridgeline_enters_tapped_and_taps_for_red_or_green() {
    let mut state = plains();
    let land = put(&mut state, P0, "Wooded Ridgeline", Zone::Hand);
    next(&mut state);
    act(&mut state, Action::PlayLand(land));
    assert_eq!(zone(&state, land), Zone::Battlefield);
    assert!(state.objects.get(land).tapped, "enters tapped");

    for color in [ManaColor::R, ManaColor::G] {
        let mut state = plains();
        let land = put(&mut state, P0, "Wooded Ridgeline", Zone::Battlefield);
        next(&mut state);
        act(&mut state, Action::ActivateManaAbilityChoice(land, color));
        assert_eq!(mana(&state, P0, color), 1, "{color:?}");
        assert!(state.objects.get(land).tapped);
    }

    let mut state = plains();
    let land = put(&mut state, P0, "Wooded Ridgeline", Zone::Battlefield);
    next(&mut state);
    for other in [ManaColor::W, ManaColor::U, ManaColor::B] {
        assert!(
            engine::step(
                &mut state.clone(),
                Action::ActivateManaAbilityChoice(land, other)
            )
            .is_err(),
            "{other:?}"
        );
    }
    for subtype in [Subtype::Mountain, Subtype::Forest] {
        assert!(engine::has_effective_subtype(&state, land, subtype));
    }
}

#[test]
fn kabira_crossroads_enters_tapped_gains_two_and_taps_for_white() {
    let mut state = plains();
    let land = put(&mut state, P0, "Kabira Crossroads", Zone::Hand);
    next(&mut state);
    act(&mut state, Action::PlayLand(land));
    settled(&mut state);
    assert_eq!(zone(&state, land), Zone::Battlefield);
    assert!(state.objects.get(land).tapped, "enters tapped");
    assert_eq!(life(&state, P0), 22);
    assert_eq!(life(&state, P1), 20);

    let mut state = plains();
    let land = put(&mut state, P0, "Kabira Crossroads", Zone::Battlefield);
    next(&mut state);
    act(&mut state, Action::ActivateManaAbility(land));
    assert_eq!(mana(&state, P0, ManaColor::W), 1);
    assert!(state.objects.get(land).tapped);
    assert_eq!(life(&state, P0), 20, "no life from tapping");
}

// ---- Creatures -------------------------------------------------------------------

#[test]
fn jaspera_sentinel_taps_itself_and_another_untapped_creature_for_any_color() {
    let mut state = plains();
    let sentinel = put(&mut state, P0, "Jaspera Sentinel", Zone::Battlefield);
    let first = put(&mut state, P0, "Thraben Inspector", Zone::Battlefield);
    let second = put(&mut state, P0, "Elvish Mystic", Zone::Battlefield);
    let tapped = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    state.objects.get_mut(tapped).tapped = true;
    let artifact = put(&mut state, P0, "Sol Ring", Zone::Battlefield);
    let opposing = put(&mut state, P1, "Thraben Inspector", Zone::Battlefield);
    next(&mut state);

    // The other creature must be named, and must be an untapped creature
    // other than the Sentinel that its controller controls.
    assert!(engine::step(
        &mut state.clone(),
        Action::ActivateManaAbilityChoice(sentinel, ManaColor::G)
    )
    .is_err());
    for bad in [sentinel, tapped, artifact, opposing] {
        assert!(
            engine::step(
                &mut state.clone(),
                Action::ActivateManaAbilityWithCostTarget(sentinel, ManaColor::G, bad)
            )
            .is_err(),
            "{}",
            state.objects.get(bad).name
        );
    }
    for color in [
        ManaColor::W,
        ManaColor::U,
        ManaColor::B,
        ManaColor::R,
        ManaColor::G,
    ] {
        let mut branch = state.clone();
        act(
            &mut branch,
            Action::ActivateManaAbilityWithCostTarget(sentinel, color, first),
        );
        assert_eq!(mana(&branch, P0, color), 1, "{color:?}");
        assert!(branch.objects.get(sentinel).tapped);
        assert!(branch.objects.get(first).tapped);
        assert!(!branch.objects.get(second).tapped);
    }

    // A summoning-sick Sentinel can't pay its own {T}; a summoning-sick
    // creature it taps can still be tapped (it is not a {T} cost of that
    // creature).
    state.objects.get_mut(sentinel).summoning_sick = true;
    assert!(engine::step(
        &mut state.clone(),
        Action::ActivateManaAbilityWithCostTarget(sentinel, ManaColor::G, first)
    )
    .is_err());
    state.objects.get_mut(sentinel).summoning_sick = false;
    state.objects.get_mut(second).summoning_sick = true;
    act(
        &mut state,
        Action::ActivateManaAbilityWithCostTarget(sentinel, ManaColor::U, second),
    );
    assert_eq!(mana(&state, P0, ManaColor::U), 1);
}

#[test]
fn jaspera_sentinel_without_another_untapped_creature_makes_no_mana() {
    let mut state = plains();
    let sentinel = put(&mut state, P0, "Jaspera Sentinel", Zone::Battlefield);
    let other = put(&mut state, P0, "Thraben Inspector", Zone::Battlefield);
    state.objects.get_mut(other).tapped = true;
    next(&mut state);
    assert!(engine::step(
        &mut state.clone(),
        Action::ActivateManaAbilityWithCostTarget(sentinel, ManaColor::G, other)
    )
    .is_err());
    assert!(engine::step(
        &mut state.clone(),
        Action::ActivateManaAbilityWithCostTarget(sentinel, ManaColor::G, sentinel)
    )
    .is_err());
    assert!(!state.objects.get(sentinel).tapped);
}

#[test]
fn kor_sanctifiers_kicked_destroys_an_artifact_or_enchantment() {
    // Kicked: an artifact.
    let mut state = plains();
    let sanctifiers = put(&mut state, P0, "Kor Sanctifiers", Zone::Hand);
    let ring = put(&mut state, P1, "Sol Ring", Zone::Battlefield);
    let pestilence = put(&mut state, P1, "Pestilence", Zone::Battlefield);
    pool(&mut state, P0, &[ManaColor::W, ManaColor::W], 2);
    cast(&mut state, sanctifiers, &[]);
    settle_kicker(&mut state, true, Some(ring));
    assert_eq!(zone(&state, sanctifiers), Zone::Battlefield);
    assert_eq!(power_toughness(&state, sanctifiers), (2, 3));
    assert_eq!(zone(&state, ring), Zone::Graveyard);
    assert_eq!(zone(&state, pestilence), Zone::Battlefield);

    // Kicked: an enchantment.
    let mut state = plains();
    let sanctifiers = put(&mut state, P0, "Kor Sanctifiers", Zone::Hand);
    let ring = put(&mut state, P1, "Sol Ring", Zone::Battlefield);
    let pestilence = put(&mut state, P1, "Pestilence", Zone::Battlefield);
    pool(&mut state, P0, &[ManaColor::W, ManaColor::W], 2);
    cast(&mut state, sanctifiers, &[]);
    settle_kicker(&mut state, true, Some(pestilence));
    assert_eq!(zone(&state, pestilence), Zone::Graveyard);
    assert_eq!(zone(&state, ring), Zone::Battlefield);

    // Kicked, with a target of the controller's own (it is not "an opponent's").
    let mut state = plains();
    let sanctifiers = put(&mut state, P0, "Kor Sanctifiers", Zone::Hand);
    let own = put(&mut state, P0, "Mind Stone", Zone::Battlefield);
    pool(&mut state, P0, &[ManaColor::W, ManaColor::W], 2);
    cast(&mut state, sanctifiers, &[]);
    settle_kicker(&mut state, true, Some(own));
    assert_eq!(zone(&state, own), Zone::Graveyard);
}

#[test]
fn kor_sanctifiers_unkicked_destroys_nothing_and_asks_for_no_target() {
    let mut state = plains();
    let sanctifiers = put(&mut state, P0, "Kor Sanctifiers", Zone::Hand);
    let ring = put(&mut state, P1, "Sol Ring", Zone::Battlefield);
    pool(&mut state, P0, &[ManaColor::W], 2);
    cast(&mut state, sanctifiers, &[]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseKicker { .. } => Action::ChooseKicker(false),
        Decision::ChooseTargets { .. } | Decision::ChooseEffectTargets { .. } => {
            panic!("an unkicked Sanctifiers has no target")
        }
        other => first_answer(other),
    });
    assert_eq!(zone(&state, sanctifiers), Zone::Battlefield);
    assert_eq!(zone(&state, ring), Zone::Battlefield);
}

#[test]
fn kor_sanctifiers_kicked_with_nothing_to_destroy_just_enters() {
    let mut state = plains();
    let sanctifiers = put(&mut state, P0, "Kor Sanctifiers", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W, ManaColor::W], 2);
    cast(&mut state, sanctifiers, &[]);
    settle_kicker(&mut state, true, None);
    assert_eq!(zone(&state, sanctifiers), Zone::Battlefield);
}

#[test]
fn kor_sanctifiers_trigger_does_nothing_when_its_target_is_gone() {
    let mut state = plains();
    let sanctifiers = put(&mut state, P0, "Kor Sanctifiers", Zone::Hand);
    let ring = put(&mut state, P1, "Sol Ring", Zone::Battlefield);
    let disenchant = put(&mut state, P0, "Disenchant", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W, ManaColor::W], 2);
    cast(&mut state, sanctifiers, &[]);
    for _ in 0..40 {
        match next(&mut state) {
            Decision::ChooseKicker { .. } => act(&mut state, Action::ChooseKicker(true)),
            Decision::ChooseTargets { legal_targets, .. } => {
                assert!(legal_targets.contains(&Target::Object(ring)));
                act(&mut state, Action::ChooseTarget(Target::Object(ring)));
            }
            Decision::OrderTriggers { pending, .. } => act(
                &mut state,
                Action::OrderTriggers((0..pending.len()).collect()),
            ),
            Decision::CastSpellOrPass { .. }
                if zone(&state, sanctifiers) == Zone::Battlefield && !state.stack.is_empty() =>
            {
                break
            }
            Decision::CastSpellOrPass { .. } => act(&mut state, Action::Pass),
            other => panic!("unexpected choice: {other:?}"),
        }
    }
    assert_eq!(state.stack.len(), 1, "the entry trigger is on the stack");
    // In response the Ring is destroyed, so the trigger's only target is gone.
    pool(&mut state, P0, &[ManaColor::W], 1);
    cast(&mut state, disenchant, &[Target::Object(ring)]);
    settled(&mut state);
    assert_eq!(zone(&state, ring), Zone::Graveyard);
    assert_eq!(zone(&state, sanctifiers), Zone::Battlefield);
}

#[test]
fn vulshok_sorcerer_has_haste_and_pings_any_target() {
    // Haste: it taps the turn it enters.
    let mut state = plains();
    let sorcerer = put(&mut state, P0, "Vulshok Sorcerer", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R, ManaColor::R], 1);
    cast(&mut state, sorcerer, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, sorcerer), Zone::Battlefield);
    assert_eq!(power_toughness(&state, sorcerer), (1, 1));
    assert!(keyword(&state, sorcerer, Keywords::HASTE));
    activate(&mut state, sorcerer, 0, &[Target::Player(P1)]);
    settled(&mut state);
    assert_eq!(life(&state, P1), 19);
    assert!(state.objects.get(sorcerer).tapped);
    assert!(!activatable(&mut state, sorcerer, 0), "tapped");

    // A creature: one damage kills a 1/1 and only marks a 1/2.
    let mut state = plains();
    let sorcerer = put(&mut state, P0, "Vulshok Sorcerer", Zone::Battlefield);
    let elves = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    activate(&mut state, sorcerer, 0, &[Target::Object(elves)]);
    settled(&mut state);
    assert_eq!(zone(&state, elves), Zone::Graveyard);

    let mut state = plains();
    let sorcerer = put(&mut state, P0, "Vulshok Sorcerer", Zone::Battlefield);
    let inspector = put(&mut state, P1, "Thraben Inspector", Zone::Battlefield);
    activate(&mut state, sorcerer, 0, &[Target::Object(inspector)]);
    settled(&mut state);
    assert_eq!(zone(&state, inspector), Zone::Battlefield);
    assert_eq!(state.objects.get(inspector).damage, 1);

    // Itself and its controller are legal targets too.
    let mut state = plains();
    let sorcerer = put(&mut state, P0, "Vulshok Sorcerer", Zone::Battlefield);
    let legal = {
        let mut probe = state.clone();
        act(&mut probe, Action::ActivateAbility(sorcerer, 0));
        match next(&mut probe) {
            Decision::ChooseTargets { legal_targets, .. } => legal_targets,
            other => panic!("expected targets, got {other:?}"),
        }
    };
    assert!(legal.contains(&Target::Player(P0)));
    assert!(legal.contains(&Target::Player(P1)));
    assert!(legal.contains(&Target::Object(sorcerer)));
}

#[test]
fn vulshok_sorcerer_ability_does_nothing_when_its_target_is_gone() {
    let mut state = plains();
    let sorcerer = put(&mut state, P0, "Vulshok Sorcerer", Zone::Battlefield);
    let elves = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    activate(&mut state, sorcerer, 0, &[Target::Object(elves)]);
    on_the_stack(&mut state);
    assert!(state.objects.get(sorcerer).tapped, "its cost is paid");
    pool(&mut state, P0, &[ManaColor::R], 0);
    cast(&mut state, bolt, &[Target::Object(elves)]);
    settled(&mut state);
    assert_eq!(zone(&state, elves), Zone::Graveyard);
    assert_eq!(life(&state, P0), 20);
    assert_eq!(life(&state, P1), 20);
    assert_eq!(zone(&state, sorcerer), Zone::Battlefield);
}

#[test]
fn primordial_pachyderm_gains_two_on_entry_and_can_block_flyers() {
    let mut state = plains();
    let pachyderm = put(&mut state, P0, "Primordial Pachyderm", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 3);
    cast(&mut state, pachyderm, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, pachyderm), Zone::Battlefield);
    assert_eq!(life(&state, P0), 22);
    assert_eq!(life(&state, P1), 20);
    assert_eq!(power_toughness(&state, pachyderm), (4, 4));
    assert!(keyword(&state, pachyderm, Keywords::REACH));
    assert!(keyword(&state, pachyderm, Keywords::TRAMPLE));

    // Reach: it blocks a flyer, a creature without reach cannot.
    let mut state = plains();
    let flyer = put(&mut state, P0, "Mulldrifter", Zone::Battlefield);
    let pachyderm = put(&mut state, P1, "Primordial Pachyderm", Zone::Battlefield);
    let inspector = put(&mut state, P1, "Thraben Inspector", Zone::Battlefield);
    let blockers = legal_blockers(&mut state, flyer);
    assert!(blockers.contains(&pachyderm));
    assert!(!blockers.contains(&inspector));
}

#[test]
fn kun_lun_warrior_discards_a_card_and_draws() {
    let mut state = plains();
    let warrior = put(&mut state, P0, "K'un-Lun Warrior", Zone::Hand);
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    let opposing_artifact = put(&mut state, P1, "Sol Ring", Zone::Battlefield);
    pool(&mut state, P0, &[ManaColor::R], 1);
    cast(&mut state, warrior, &[]);
    // The menu: the controller's hand cards (and own artifacts), never an
    // opponent's artifact, and the controller may choose none.
    let Some(Decision::ChooseEffectTargets {
        legal_targets,
        min_targets,
        max_targets,
        ..
    }) = settle(&mut state)
    else {
        panic!("expected the choice");
    };
    assert!(legal_targets.contains(&Target::Object(bolt)));
    assert!(!legal_targets.contains(&Target::Object(opposing_artifact)));
    assert_eq!((min_targets, max_targets), (0, 1));
    act(&mut state, Action::ChooseEffectTarget(Target::Object(bolt)));
    settle_first(&mut state);
    assert_eq!(zone(&state, warrior), Zone::Battlefield);
    assert_eq!(power_toughness(&state, warrior), (2, 2));
    assert_eq!(zone(&state, bolt), Zone::Graveyard);
    assert_eq!(state.players[0].hand.len(), 1, "one card drawn");
    assert_eq!(zone(&state, opposing_artifact), Zone::Battlefield);
}

#[test]
fn kun_lun_warrior_sacrifices_an_artifact_and_draws() {
    let mut state = plains();
    let warrior = put(&mut state, P0, "K'un-Lun Warrior", Zone::Hand);
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    let ring = put(&mut state, P0, "Sol Ring", Zone::Battlefield);
    pool(&mut state, P0, &[ManaColor::R], 1);
    cast(&mut state, warrior, &[]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseEffectTargets {
            legal_targets,
            selected_count: 0,
            ..
        } => {
            assert!(legal_targets.contains(&Target::Object(ring)));
            assert!(legal_targets.contains(&Target::Object(bolt)));
            Action::ChooseEffectTarget(Target::Object(ring))
        }
        other => first_answer(other),
    });
    assert_eq!(zone(&state, ring), Zone::Graveyard);
    assert_eq!(zone(&state, bolt), Zone::Hand, "nothing discarded");
    assert_eq!(state.players[0].hand.len(), 2, "bolt and the drawn card");
}

#[test]
fn kun_lun_warrior_sacrifices_an_artifact_token() {
    let mut state = plains();
    let warrior = put(&mut state, P0, "K'un-Lun Warrior", Zone::Hand);
    let treasure = token(&mut state, P0, "Treasure Token");
    pool(&mut state, P0, &[ManaColor::R], 1);
    cast(&mut state, warrior, &[]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseEffectTargets {
            legal_targets,
            selected_count: 0,
            ..
        } if legal_targets.contains(&Target::Object(treasure)) => {
            Action::ChooseEffectTarget(Target::Object(treasure))
        }
        other => first_answer(other),
    });
    assert!(!state.players[0].battlefield.contains(&treasure));
    assert_eq!(state.players[0].hand.len(), 1, "one card drawn");
}

#[test]
fn kun_lun_warrior_may_do_neither() {
    let mut state = plains();
    let warrior = put(&mut state, P0, "K'un-Lun Warrior", Zone::Hand);
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    let ring = put(&mut state, P0, "Sol Ring", Zone::Battlefield);
    pool(&mut state, P0, &[ManaColor::R], 1);
    cast(&mut state, warrior, &[]);
    // The first answer declines (the selection may finish with none).
    settle_first(&mut state);
    assert_eq!(zone(&state, warrior), Zone::Battlefield);
    assert_eq!(zone(&state, bolt), Zone::Hand);
    assert_eq!(zone(&state, ring), Zone::Battlefield);
    assert_eq!(state.players[0].hand.len(), 1, "nothing drawn");
    assert_eq!(state.players[0].library.len(), 20);
}

#[test]
fn kun_lun_warrior_with_nothing_to_sacrifice_or_discard_asks_nothing() {
    let mut state = plains();
    let warrior = put(&mut state, P0, "K'un-Lun Warrior", Zone::Hand);
    // An opponent's artifact is not an option.
    put(&mut state, P1, "Sol Ring", Zone::Battlefield);
    pool(&mut state, P0, &[ManaColor::R], 1);
    cast(&mut state, warrior, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, warrior), Zone::Battlefield);
    assert!(state.players[0].hand.is_empty());
    assert_eq!(state.players[0].library.len(), 20, "nothing drawn");
}

// ---- Auras -----------------------------------------------------------------------

#[test]
fn lifelink_aura_gains_its_creatures_controller_life_for_the_damage_dealt() {
    let mut state = plains();
    let inspector = put(&mut state, P0, "Thraben Inspector", Zone::Battlefield);
    let other = put(&mut state, P0, "Thraben Inspector", Zone::Battlefield);
    let aura = put(&mut state, P0, "Lifelink", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W], 0);
    assert!(!keyword(&state, inspector, Keywords::LIFELINK));
    enchant(&mut state, aura, inspector);
    assert!(keyword(&state, inspector, Keywords::LIFELINK));
    assert!(!keyword(&state, other, Keywords::LIFELINK));
    assert_eq!(power_toughness(&state, inspector), (1, 2));
    attack_with(&mut state, vec![inspector], |_, _| None);
    assert_eq!(life(&state, P1), 19);
    assert_eq!(life(&state, P0), 21);

    // The creature without it gains nothing.
    let mut state = plains();
    let inspector = put(&mut state, P0, "Thraben Inspector", Zone::Battlefield);
    let aura = put(&mut state, P0, "Lifelink", Zone::Hand);
    let other = put(&mut state, P0, "Thraben Inspector", Zone::Battlefield);
    pool(&mut state, P0, &[ManaColor::W], 0);
    enchant(&mut state, aura, inspector);
    attack_with(&mut state, vec![other], |_, _| None);
    assert_eq!(life(&state, P1), 19);
    assert_eq!(life(&state, P0), 20);
}

#[test]
fn lifelink_aura_on_an_opposing_creature_gains_that_creatures_controller_life() {
    let mut state = plains();
    let attacker = put(&mut state, P0, "Thraben Inspector", Zone::Battlefield);
    let blocker = put(&mut state, P1, "Thraben Inspector", Zone::Battlefield);
    let aura = put(&mut state, P0, "Lifelink", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W], 0);
    enchant(&mut state, aura, blocker);
    attack_with(&mut state, vec![attacker], |_, decision| match decision {
        Decision::DeclareBlockers { .. } => {
            Some(Action::DeclareBlockers(vec![(blocker, attacker)]))
        }
        _ => None,
    });
    assert_eq!(life(&state, P1), 21, "the blocker's controller gained 1");
    assert_eq!(life(&state, P0), 20);
}

#[test]
fn mask_of_law_and_grace_gives_protection_from_black_and_red_targeting() {
    let mut state = plains();
    let inspector = put(&mut state, P0, "Thraben Inspector", Zone::Battlefield);
    let mystic = put(&mut state, P1, "Elvish Mystic", Zone::Battlefield);
    let mask = put(&mut state, P0, "Mask of Law and Grace", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W], 0);
    enchant(&mut state, mask, inspector);
    assert_eq!(power_toughness(&state, inspector), (1, 2));

    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 0);
    let targets = cast_targets(&state, bolt);
    assert!(!targets.contains(&Target::Object(inspector)), "red");
    assert!(targets.contains(&Target::Object(mystic)));
    assert!(targets.contains(&Target::Player(P1)));

    let cast_down = put(&mut state, P0, "Cast Down", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 1);
    let targets = cast_targets(&state, cast_down);
    assert!(!targets.contains(&Target::Object(inspector)), "black");
    assert!(targets.contains(&Target::Object(mystic)));

    // Other colors still work.
    let scream = put(&mut state, P0, "Unable to Scream", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 0);
    assert!(cast_targets(&state, scream).contains(&Target::Object(inspector)));
}

#[test]
fn mask_of_law_and_grace_prevents_damage_from_black_and_red_sources() {
    let mut state = plains();
    let protected = put(&mut state, P0, "Thraben Inspector", Zone::Battlefield);
    let bare = put(&mut state, P1, "Thraben Inspector", Zone::Battlefield);
    let mask = put(&mut state, P0, "Mask of Law and Grace", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W], 0);
    enchant(&mut state, mask, protected);

    // Red: Pyroclasm deals 2 damage to each creature.
    let pyroclasm = put(&mut state, P0, "Pyroclasm", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 1);
    cast(&mut state, pyroclasm, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, bare), Zone::Graveyard);
    assert_eq!(zone(&state, protected), Zone::Battlefield);
    assert_eq!(state.objects.get(protected).damage, 0);

    // Black: two activations of Pestilence deal 2 damage to each creature.
    let bare = put(&mut state, P1, "Thraben Inspector", Zone::Battlefield);
    let pestilence = put(&mut state, P0, "Pestilence", Zone::Battlefield);
    pool(&mut state, P0, &[ManaColor::B, ManaColor::B], 0);
    for _ in 0..2 {
        activate(&mut state, pestilence, 0, &[]);
        settled(&mut state);
    }
    assert_eq!(zone(&state, bare), Zone::Graveyard);
    assert_eq!(zone(&state, protected), Zone::Battlefield);
    assert_eq!(state.objects.get(protected).damage, 0);
}

#[test]
fn mask_of_law_and_grace_stops_black_and_red_creatures_from_blocking() {
    let mut state = plains();
    let attacker = put(&mut state, P0, "Thraben Inspector", Zone::Battlefield);
    let red = put(&mut state, P1, "Kessig Flamebreather", Zone::Battlefield);
    let black = put(&mut state, P1, "Crypt Rats", Zone::Battlefield);
    let white = put(&mut state, P1, "Thraben Inspector", Zone::Battlefield);
    let green = put(&mut state, P1, "Elvish Mystic", Zone::Battlefield);
    let mask = put(&mut state, P0, "Mask of Law and Grace", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W], 0);
    enchant(&mut state, mask, attacker);
    let blockers = legal_blockers(&mut state, attacker);
    assert!(!blockers.contains(&red));
    assert!(!blockers.contains(&black));
    assert!(blockers.contains(&white));
    assert!(blockers.contains(&green));
}

#[test]
fn mask_of_law_and_grace_protection_ends_when_the_aura_leaves() {
    let mut state = plains();
    let inspector = put(&mut state, P0, "Thraben Inspector", Zone::Battlefield);
    let mask = put(&mut state, P0, "Mask of Law and Grace", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W], 0);
    enchant(&mut state, mask, inspector);
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 0);
    assert!(!cast_targets(&state, bolt).contains(&Target::Object(inspector)));
    let disenchant = put(&mut state, P0, "Disenchant", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W], 1);
    cast(&mut state, disenchant, &[Target::Object(mask)]);
    settled(&mut state);
    assert_eq!(zone(&state, mask), Zone::Graveyard);
    pool(&mut state, P0, &[ManaColor::R], 0);
    assert!(cast_targets(&state, bolt).contains(&Target::Object(inspector)));
}

#[test]
fn an_aura_fizzles_when_its_creature_is_gone() {
    for name in ["Lifelink", "Mask of Law and Grace"] {
        let mut state = plains();
        let inspector = put(&mut state, P0, "Thraben Inspector", Zone::Battlefield);
        let aura = put(&mut state, P0, name, Zone::Hand);
        let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
        pool(&mut state, P0, &[ManaColor::W], 0);
        cast(&mut state, aura, &[Target::Object(inspector)]);
        pool(&mut state, P0, &[ManaColor::R], 0);
        cast(&mut state, bolt, &[Target::Object(inspector)]);
        settled(&mut state);
        assert_eq!(zone(&state, inspector), Zone::Graveyard, "{name}");
        assert_eq!(zone(&state, aura), Zone::Graveyard, "{name}");
    }
}

// ---- Artifact ----------------------------------------------------------------------

#[test]
fn mycosynth_wellspring_may_fetch_a_basic_land_when_it_enters() {
    let mut state = game(&["Island"; 10]);
    let wellspring = put(&mut state, P0, "Mycosynth Wellspring", Zone::Hand);
    let forest = put(&mut state, P0, "Forest", Zone::Library);
    pool(&mut state, P0, &[], 2);
    cast(&mut state, wellspring, &[]);
    settle_picking(&mut state, 1, Some(forest));
    assert_eq!(zone(&state, wellspring), Zone::Battlefield);
    assert_eq!(zone(&state, forest), Zone::Hand);
    assert_eq!(state.players[0].hand.len(), 1);
    assert_eq!(state.players[0].library.len(), 10, "one card left it");
}

#[test]
fn mycosynth_wellspring_declined_search_fetches_nothing() {
    let mut state = game(&["Island"; 10]);
    let wellspring = put(&mut state, P0, "Mycosynth Wellspring", Zone::Hand);
    let forest = put(&mut state, P0, "Forest", Zone::Library);
    pool(&mut state, P0, &[], 2);
    cast(&mut state, wellspring, &[]);
    settle_picking(&mut state, 0, Some(forest));
    assert_eq!(zone(&state, wellspring), Zone::Battlefield);
    assert_eq!(zone(&state, forest), Zone::Library);
    assert!(state.players[0].hand.is_empty());
    assert_eq!(state.players[0].library.len(), 11);
}

#[test]
fn mycosynth_wellspring_fetches_a_basic_land_when_it_is_put_into_a_graveyard() {
    for accept in [true, false] {
        let mut state = game(&["Island"; 10]);
        let wellspring = put(&mut state, P0, "Mycosynth Wellspring", Zone::Battlefield);
        let forest = put(&mut state, P0, "Forest", Zone::Library);
        let disenchant = put(&mut state, P0, "Disenchant", Zone::Hand);
        pool(&mut state, P0, &[ManaColor::W], 1);
        cast(&mut state, disenchant, &[Target::Object(wellspring)]);
        settle_picking(&mut state, u16::from(accept), Some(forest));
        assert_eq!(zone(&state, wellspring), Zone::Graveyard);
        if accept {
            assert_eq!(zone(&state, forest), Zone::Hand);
            assert_eq!(state.players[0].library.len(), 10);
        } else {
            assert_eq!(zone(&state, forest), Zone::Library);
            assert_eq!(state.players[0].library.len(), 11);
        }
    }
}

#[test]
fn mycosynth_wellspring_with_no_basic_land_in_the_library_finds_nothing() {
    let mut state = game(&["Lightning Bolt"; 10]);
    let wellspring = put(&mut state, P0, "Mycosynth Wellspring", Zone::Hand);
    pool(&mut state, P0, &[], 2);
    cast(&mut state, wellspring, &[]);
    settle_picking(&mut state, 1, None);
    assert_eq!(zone(&state, wellspring), Zone::Battlefield);
    assert!(state.players[0].hand.is_empty());
}

// ---- Public sessions ---------------------------------------------------------------

#[derive(Default)]
struct Pf1aEvents {
    plays: [usize; PF1A_CARDS.len()],
}

impl Pf1aEvents {
    fn observe(&mut self, _decision: &Value, action: &Value) {
        let semantic = &action["semantic"];
        if !matches!(
            semantic["action_kind"].as_str(),
            Some("cast_spell" | "play_land")
        ) {
            return;
        }
        let Some(card_id) = semantic["source"]["card_db_id"]
            .as_u64()
            .and_then(|id| usize::try_from(id).ok())
        else {
            return;
        };
        let name = CARD_DEFS[card_id].name;
        if let Some(index) = PF1A_CARDS.iter().position(|&card| card == name) {
            self.plays[index] += 1;
        }
    }
}

#[test]
fn pf1a_decks_finish_and_replay_through_public_sessions_both_seats() {
    let red_green = deck(&[
        ("Wooded Ridgeline", 4),
        ("Mountain", 7),
        ("Forest", 7),
        ("Jaspera Sentinel", 4),
        ("Primordial Pachyderm", 4),
        ("Vulshok Sorcerer", 4),
        ("K'un-Lun Warrior", 4),
        ("Mycosynth Wellspring", 4),
        ("Llanowar Elves", 2),
    ]);
    let white = deck(&[
        ("Kabira Crossroads", 4),
        ("Plains", 16),
        ("Kor Sanctifiers", 4),
        ("Lifelink", 4),
        ("Mask of Law and Grace", 4),
        ("Thraben Inspector", 8),
    ]);
    let mut events = Pf1aEvents::default();
    // Twelve games plus their twelve replays, with both policies and seats.
    for pair in [[red_green.clone(), white.clone()], [white, red_green]] {
        for seed in 0x5F1A_u64..0x5F1D {
            for policy in [Policy::Ranked, Policy::Random(seed + 0x100)] {
                let played = play_observed(pair.clone(), None, seed, policy, &mut |d, a| {
                    events.observe(d, a)
                });
                let replay = play_observed(pair.clone(), None, seed, policy, &mut |_, _| {});
                assert_eq!(played.transcript_sha256, replay.transcript_sha256);
            }
        }
    }
    for (name, plays) in PF1A_CARDS.into_iter().zip(events.plays) {
        assert!(
            plays > 0,
            "{name} was never cast or played through a public session"
        );
    }
}
