//! Eternal wave P1a: native rules tests for the Premodern cards that
//! complete Stiflenought, Sligh, RG Aggro, Goblins, Terrageddon and
//! Mono-Black Aggro, with the cards they share with wave P1b.
#![cfg(feature = "eternal-fixtures")]

use mtg_kernel::card_def::{
    card_id_by_name, AltCostCondition, CardCapability, CardDef, Keywords, Subtype, CARD_DEFS,
};
use mtg_kernel::engine::{self, Action, CastMode, Decision, OptionalCostChoice};
use mtg_kernel::eternal_premodern_p1a_v1::statics;
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::ManaColor;
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Target, Zone};

const P0: PlayerId = PlayerId::P0;
const P1: PlayerId = PlayerId::P1;

const P1A_CARDS: [&str; 53] = [
    "Anurid Brushhopper",
    "Armageddon",
    "Ball Lightning",
    "Barbarian Ring",
    "Call of the Herd",
    "Cursed Scroll",
    "Diabolic Edict",
    "Disenchant",
    "Exalted Angel",
    "Fire // Ice",
    "Flash of Insight",
    "Foil",
    "Funeral Charm",
    "Gempalm Incinerator",
    "Gemstone Mine",
    "Goblin Goon",
    "Goblin Lackey",
    "Goblin Matron",
    "Goblin Piledriver",
    "Goblin Ringleader",
    "Goblin Sharpshooter",
    "Goblin Vandal",
    "Goblin Warchief",
    "Graveborn Muse",
    "Grim Lavamancer",
    "Hypnotic Specter",
    "Incinerate",
    "Jackal Pup",
    "Knight of Stromgald",
    "Mogg Fanatic",
    "Mystic Enforcer",
    "Nantuko Shade",
    "Naturalize",
    "Portent",
    "Powder Keg",
    "Price of Progress",
    "Rishadan Port",
    "Rith's Grove",
    "Seal of Fire",
    "Secluded Steppe",
    "Siege-Gang Commander",
    "Skeletal Scrying",
    "Skirk Prospector",
    "Smother",
    "Sulfuric Vortex",
    "Sylvan Library",
    "Terravore",
    "Thwart",
    "Tranquil Thicket",
    "Vision Charm",
    "Weathered Wayfarer",
    "Wild Mongrel",
    "Withered Wretch",
];

fn def(name: &str) -> &'static CardDef {
    &CARD_DEFS[id(name) as usize]
}

fn id(name: &str) -> u16 {
    card_id_by_name(name).unwrap_or_else(|| panic!("{name}"))
}

fn name_of(state: &GameState, object: ObjectId) -> &'static str {
    CARD_DEFS[state.objects.get(object).card_def as usize].name
}

/// A first main phase with P0 active and holding priority. Each library
/// holds `library` in order, top first.
fn game(library: &[&str]) -> GameState {
    let ids: Vec<u16> = library.iter().map(|name| id(name)).collect();
    let mut state = GameState::new_from_libraries(
        &ids,
        &ids,
        |id| CARD_DEFS[id as usize].object_name.into(),
        0x5031_4121_0001,
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

fn controls(state: &GameState, player: PlayerId, name: &str) -> Vec<ObjectId> {
    state.players[player.index()]
        .battlefield
        .iter()
        .copied()
        .filter(|&object| name_of(state, object) == name)
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

/// Settles, taking the first candidate of every selection that has none
/// selected yet (searches find a card) and the first answer otherwise.
fn settle_taking(state: &mut GameState) {
    settle_with(state, |_, decision| match decision {
        Decision::ChooseEffectTargets {
            legal_targets,
            selected_count: 0,
            ..
        } if !legal_targets.is_empty() => Action::ChooseEffectTarget(legal_targets[0]),
        other => first_answer(other),
    });
}

/// Settles, answering only the order of a multi-card mill (each card in
/// turn); any other choice fails.
fn settle_ordering(state: &mut GameState) {
    let mut chosen = Vec::new();
    settle_with(state, |_, decision| match decision {
        Decision::ChooseEffectTargets {
            legal_targets,
            selected_count,
            can_finish: false,
            ..
        } => {
            if selected_count == 0 {
                chosen.clear();
            }
            let target = *legal_targets
                .iter()
                .find(|target| !chosen.contains(*target))
                .expect("an unordered card");
            chosen.push(target);
            Action::ChooseEffectTarget(target)
        }
        Decision::ChooseEffectTargets {
            can_finish: true, ..
        } => Action::FinishEffectSelection,
        other => panic!("unexpected choice: {other:?}"),
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

/// P0 attacks with `attackers` in this turn's combat, then the game plays
/// on to P0's second main phase with an empty stack.
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

/// The attackers P0 may declare in this turn's combat.
fn eligible_attackers(state: &mut GameState) -> Vec<ObjectId> {
    pass_until(state, |state| state.step == Step::DeclareAttackers);
    match next(state) {
        Decision::DeclareAttackers { eligible, .. } => eligible,
        _ => Vec::new(),
    }
}

fn hand_contains(state: &GameState, player: PlayerId, object: ObjectId) -> bool {
    state.players[player.index()].hand.contains(&object)
}

fn mana(state: &GameState, player: PlayerId, color: ManaColor) -> u8 {
    state.players[player.index()].mana_pool[color.pool_index()]
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

fn has_subtype(state: &GameState, object: ObjectId, subtype: Subtype) -> bool {
    engine::has_effective_subtype(state, object, subtype)
}

/// Answers the rest of a cast in progress with first answers until
/// priority returns.
fn finish_cast(state: &mut GameState) {
    for _ in 0..16 {
        match next(state) {
            Decision::CastSpellOrPass { .. } => return,
            other => act(state, first_answer(other)),
        }
    }
    panic!("the cast did not finish");
}

/// Plays the rest of this turn's combat with first answers, to the second
/// main phase with an empty stack.
fn attack_finish(state: &mut GameState) {
    run_until(
        state,
        |state| {
            state.step == Step::Main2
                && state.stack.is_empty()
                && state.engine.pending_triggers.is_empty()
                && state.engine.pending_effect.is_none()
        },
        |_, _| None,
    );
}

/// Passes priority until `player` holds it with something on the stack.
fn pass_to(state: &mut GameState, player: PlayerId) {
    for _ in 0..16 {
        match next(state) {
            Decision::CastSpellOrPass { .. } if state.priority_player == player => return,
            Decision::CastSpellOrPass { .. } => act(state, Action::Pass),
            other => act(state, first_answer(other)),
        }
    }
    panic!("{player:?} never got priority");
}

// ---- Characteristics -----------------------------------------------------------

#[test]
fn p1a_cards_are_full_with_their_printed_characteristics() {
    for name in P1A_CARDS {
        assert_eq!(def(name).capability, CardCapability::Full, "{name}");
        assert!(!def(name).is_token, "{name}");
    }
    let elephant = def("Elephant Token");
    assert!(elephant.is_token && elephant.capability == CardCapability::Full);
    assert_eq!((elephant.power, elephant.toughness), (Some(3), Some(3)));
    assert_eq!(elephant.subtypes, &[Subtype::Elephant]);

    let ball = def("Ball Lightning");
    assert!(ball.keywords.has(Keywords::TRAMPLE) && ball.keywords.has(Keywords::HASTE));
    let angel = def("Exalted Angel");
    assert!(angel.keywords.has(Keywords::FLYING) && angel.keywords.has(Keywords::LIFELINK));
    assert_eq!(
        angel.alt_cost.map(|alt| alt.condition),
        Some(AltCostCondition::DisguiseFromHand)
    );
    assert!(def("Goblin Ringleader").keywords.has(Keywords::HASTE));
    assert!(def("Hypnotic Specter").keywords.has(Keywords::FLYING));
    assert!(def("Terravore").keywords.has(Keywords::TRAMPLE));
    for name in ["Call of the Herd", "Flash of Insight"] {
        assert!(def(name).flashback.is_some(), "{name}");
    }
    for name in ["Foil", "Thwart"] {
        assert_eq!(
            def(name).alt_cost.map(|alt| alt.condition),
            Some(AltCostCondition::Always),
            "{name}"
        );
    }
    assert!(def("Skeletal Scrying").additional_cost.is_some());
    for name in ["Secluded Steppe", "Tranquil Thicket"] {
        assert!(def(name).enters_battlefield_tapped, "{name}");
    }
    assert!(def("Funeral Charm").mode2.is_some() && def("Funeral Charm").mode3.is_some());
    assert!(def("Vision Charm").mode2.is_some() && def("Vision Charm").mode3.is_some());
    let fire = def("Fire // Ice");
    assert_eq!(fire.mana_value, 4);
    assert_eq!(fire.adventure.as_ref().map(|ice| ice.name), Some("Ice"));
    assert!(def("Rith's Grove").subtypes.contains(&Subtype::Lair));
    for (name, subtype) in [
        ("Jackal Pup", Subtype::Jackal),
        ("Hypnotic Specter", Subtype::Specter),
        ("Nantuko Shade", Subtype::Shade),
        ("Mystic Enforcer", Subtype::Mystic),
    ] {
        assert!(def(name).subtypes.contains(&subtype), "{name}");
        assert!(subtype.is_creature_type(), "{name}");
    }
}

// ---- Lands -------------------------------------------------------------------------

#[test]
fn barbarian_ring_pings_for_red_and_shoots_with_threshold() {
    let mut state = islands();
    let ring = put(&mut state, P0, "Barbarian Ring", Zone::Battlefield);
    next(&mut state);
    act(&mut state, Action::ActivateManaAbility(ring));
    assert_eq!(mana(&state, P0, ManaColor::R), 1);
    assert_eq!(state.players[0].life, 19);

    let mut state = islands();
    let ring = put(&mut state, P0, "Barbarian Ring", Zone::Battlefield);
    pool(&mut state, P0, &[ManaColor::R], 0);
    assert!(!activatable(&mut state, ring, 0), "no threshold");
    for _ in 0..7 {
        put(&mut state, P0, "Island", Zone::Graveyard);
    }
    activate(&mut state, ring, 0, &[Target::Player(P1)]);
    settled(&mut state);
    assert_eq!(zone(&state, ring), Zone::Graveyard);
    assert_eq!(state.players[1].life, 18);
}

#[test]
fn gemstone_mine_taps_three_times_for_any_color_then_is_sacrificed() {
    let mut state = islands();
    let mine = put(&mut state, P0, "Gemstone Mine", Zone::Hand);
    next(&mut state);
    act(&mut state, Action::PlayLand(mine));
    assert_eq!(statics::mining_counters(&state, mine), 3);
    for (turn, color) in [ManaColor::B, ManaColor::G, ManaColor::W]
        .into_iter()
        .enumerate()
    {
        state.objects.get_mut(mine).tapped = false;
        next(&mut state);
        act(&mut state, Action::ActivateManaAbilityChoice(mine, color));
        assert_eq!(mana(&state, P0, color), 1);
        assert_eq!(
            statics::mining_counters(&state, mine),
            2 - u8::try_from(turn).unwrap()
        );
    }
    assert_eq!(
        zone(&state, mine),
        Zone::Graveyard,
        "no mining counters left"
    );
}

#[test]
fn rishadan_port_taps_a_land_and_adds_colorless() {
    let mut state = islands();
    let port = put(&mut state, P0, "Rishadan Port", Zone::Battlefield);
    let island = put(&mut state, P1, "Island", Zone::Battlefield);
    next(&mut state);
    act(&mut state, Action::ActivateManaAbility(port));
    assert_eq!(mana(&state, P0, ManaColor::C), 1);
    state.objects.get_mut(port).tapped = false;
    activate(&mut state, port, 0, &[Target::Object(island)]);
    settled(&mut state);
    assert!(state.objects.get(island).tapped);
    assert!(state.objects.get(port).tapped);
}

#[test]
fn riths_grove_returns_a_non_lair_land_or_is_sacrificed() {
    let mut state = islands();
    let forest = put(&mut state, P0, "Forest", Zone::Battlefield);
    let grove = put(&mut state, P0, "Rith's Grove", Zone::Hand);
    next(&mut state);
    act(&mut state, Action::PlayLand(grove));
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseEffectTargets {
            legal_targets,
            selected_count: 0,
            ..
        } => {
            assert_eq!(legal_targets, vec![Target::Object(forest)]);
            Action::ChooseEffectTarget(Target::Object(forest))
        }
        other => first_answer(other),
    });
    assert!(hand_contains(&state, P0, forest));
    assert_eq!(zone(&state, grove), Zone::Battlefield);
    next(&mut state);
    for color in [ManaColor::R, ManaColor::G, ManaColor::W] {
        state.objects.get_mut(grove).tapped = false;
        act(&mut state, Action::ActivateManaAbilityChoice(grove, color));
        assert_eq!(mana(&state, P0, color), 1);
    }

    // No other land: it is sacrificed.
    let mut state = islands();
    let grove = put(&mut state, P0, "Rith's Grove", Zone::Hand);
    next(&mut state);
    act(&mut state, Action::PlayLand(grove));
    settled(&mut state);
    assert_eq!(zone(&state, grove), Zone::Graveyard);
}

#[test]
fn secluded_steppe_and_tranquil_thicket_enter_tapped_and_cycle() {
    for (name, color, cost) in [
        ("Secluded Steppe", ManaColor::W, ManaColor::W),
        ("Tranquil Thicket", ManaColor::G, ManaColor::G),
    ] {
        let mut state = islands();
        let land = put(&mut state, P0, name, Zone::Hand);
        next(&mut state);
        act(&mut state, Action::PlayLand(land));
        assert!(state.objects.get(land).tapped, "{name}");
        state.objects.get_mut(land).tapped = false;
        next(&mut state);
        act(&mut state, Action::ActivateManaAbility(land));
        assert_eq!(mana(&state, P0, color), 1, "{name}");

        let mut state = islands();
        let land = put(&mut state, P0, name, Zone::Hand);
        pool(&mut state, P0, &[cost], 0);
        activate(&mut state, land, 0, &[]);
        settled(&mut state);
        assert_eq!(zone(&state, land), Zone::Graveyard, "{name}");
        assert_eq!(state.players[0].hand.len(), 1, "{name} drew");
    }
}

// ---- Spells ------------------------------------------------------------------------

#[test]
fn armageddon_destroys_every_land() {
    let mut state = islands();
    let mine = put(&mut state, P0, "Plains", Zone::Battlefield);
    let theirs = put(&mut state, P1, "Wasteland", Zone::Battlefield);
    let elves = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    let armageddon = put(&mut state, P0, "Armageddon", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W], 3);
    cast(&mut state, armageddon, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, mine), Zone::Graveyard);
    assert_eq!(zone(&state, theirs), Zone::Graveyard);
    assert_eq!(zone(&state, elves), Zone::Battlefield);
}

#[test]
fn call_of_the_herd_makes_an_elephant_and_flashes_back() {
    let mut state = islands();
    let call = put(&mut state, P0, "Call of the Herd", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 2);
    cast(&mut state, call, &[]);
    settled(&mut state);
    let elephants = controls(&state, P0, "Elephant Token");
    assert_eq!(elephants.len(), 1);
    assert_eq!(power_toughness(&state, elephants[0]), (3, 3));
    assert_eq!(zone(&state, call), Zone::Graveyard);
    pool(&mut state, P0, &[ManaColor::G], 3);
    cast(&mut state, call, &[]);
    settled(&mut state);
    assert_eq!(controls(&state, P0, "Elephant Token").len(), 2);
    assert_eq!(zone(&state, call), Zone::Exile, "flashback exiles it");
}

#[test]
fn diabolic_edict_makes_target_player_sacrifice_a_creature() {
    let mut state = islands();
    let elves = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    let edict = put(&mut state, P0, "Diabolic Edict", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 1);
    cast(&mut state, edict, &[Target::Player(P1)]);
    settle_taking(&mut state);
    assert_eq!(zone(&state, elves), Zone::Graveyard);
}

#[test]
fn disenchant_and_naturalize_destroy_an_artifact_or_enchantment() {
    for (name, color) in [("Disenchant", ManaColor::W), ("Naturalize", ManaColor::G)] {
        let mut state = islands();
        let ring = put(&mut state, P1, "Sol Ring", Zone::Battlefield);
        let elves = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
        let spell = put(&mut state, P0, name, Zone::Hand);
        pool(&mut state, P0, &[color], 1);
        assert!(castable(&mut state, spell));
        act(&mut state, Action::CastSpell(spell));
        match next(&mut state) {
            Decision::ChooseTargets { legal_targets, .. } => {
                assert!(legal_targets.contains(&Target::Object(ring)));
                assert!(!legal_targets.contains(&Target::Object(elves)), "{name}");
                act(&mut state, Action::ChooseTarget(Target::Object(ring)));
            }
            other => panic!("{other:?}"),
        }
        settled(&mut state);
        assert_eq!(zone(&state, ring), Zone::Graveyard, "{name}");
    }
}

#[test]
fn fire_divides_two_damage_and_ice_taps_and_draws() {
    let mut state = islands();
    let first = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    let fire = put(&mut state, P0, "Fire // Ice", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 1);
    assert!(castable(&mut state, fire));
    act(&mut state, Action::CastSpell(fire));
    let mut picks = 0;
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseSpellMode { .. } => Action::ChooseSpellMode(0),
        Decision::ChooseTargets { legal_targets, .. } => {
            picks += 1;
            assert!(legal_targets.contains(&Target::Player(P1)));
            Action::ChooseTarget(if picks == 1 {
                Target::Object(first)
            } else {
                Target::Player(P1)
            })
        }
        other => first_answer(other),
    });
    assert_eq!(picks, 2);
    assert_eq!(zone(&state, first), Zone::Graveyard);
    assert_eq!(state.players[1].life, 19);
    assert_eq!(zone(&state, fire), Zone::Graveyard);

    // Both picks on one target deal it 2.
    let mut state = islands();
    let fire = put(&mut state, P0, "Fire // Ice", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 1);
    act(&mut state, Action::CastSpell(fire));
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseSpellMode { .. } => Action::ChooseSpellMode(0),
        Decision::ChooseTargets { .. } => Action::ChooseTarget(Target::Player(P1)),
        other => first_answer(other),
    });
    assert_eq!(state.players[1].life, 18);

    // Ice: tap target permanent, draw a card; it goes to the graveyard.
    let mut state = islands();
    let elves = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    let ice = put(&mut state, P0, "Fire // Ice", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 1);
    act(&mut state, Action::CastSpell(ice));
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseSpellMode { legal_modes, .. } => {
            assert!(legal_modes.contains(&1));
            Action::ChooseSpellMode(1)
        }
        Decision::ChooseTargets { .. } => Action::ChooseTarget(Target::Object(elves)),
        other => first_answer(other),
    });
    assert!(state.objects.get(elves).tapped);
    assert_eq!(state.players[0].hand.len(), 1);
    assert_eq!(
        zone(&state, ice),
        Zone::Graveyard,
        "a split card is not exiled"
    );
}

#[test]
fn a_split_card_half_on_the_stack_has_its_own_color_and_mana_value() {
    let mut state = islands();
    let ice = put(&mut state, P0, "Fire // Ice", Zone::Hand);
    let elves = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    assert_eq!(engine::object_mana_value(&state, ice), 4);
    pool(&mut state, P0, &[ManaColor::U], 1);
    act(&mut state, Action::CastSpell(ice));
    // Only Ice is affordable, so its half is chosen without asking.
    choose_target(&mut state, Target::Object(elves));
    finish_cast(&mut state);
    assert_eq!(zone(&state, ice), Zone::Stack);
    assert_eq!(engine::object_mana_value(&state, ice), 2);
    let blue = mtg_kernel::card_def::mana_color_mask(ManaColor::U);
    assert_eq!(engine::object_color_mask(&state, ice), blue);
}

#[test]
fn flash_of_insight_looks_at_x_and_flashes_back_exiling_blue_cards() {
    let mut state = game(&["Island", "Opt", "Brainstorm", "Island", "Island"]);
    let flash = put(&mut state, P0, "Flash of Insight", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 4);
    act(&mut state, Action::CastSpell(flash));
    // X = 3.
    let mut looked = None;
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseEffectOption { option_count, .. } if option_count > 3 => {
            Action::ChooseEffectOption(3)
        }
        Decision::ChooseEffectTargets { legal_targets, .. } if !legal_targets.is_empty() => {
            looked.get_or_insert(legal_targets.len());
            Action::ChooseEffectTarget(legal_targets[0])
        }
        Decision::ChooseEffectTargets { .. } => Action::FinishEffectSelection,
        other => first_answer(other),
    });
    assert_eq!(looked, Some(3), "X is 3");
    assert_eq!(state.players[0].hand.len(), 1);
    assert_eq!(zone(&state, flash), Zone::Graveyard);

    // Flashback: exile two blue cards from the graveyard, X is 2.
    let mut state = game(&["Island", "Island", "Island", "Island", "Island"]);
    let flash = put(&mut state, P0, "Flash of Insight", Zone::Graveyard);
    let opt = put(&mut state, P0, "Opt", Zone::Graveyard);
    let brainstorm = put(&mut state, P0, "Brainstorm", Zone::Graveyard);
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Graveyard);
    pool(&mut state, P0, &[ManaColor::U], 1);
    assert!(castable(&mut state, flash));
    act(&mut state, Action::CastSpell(flash));
    let mut offered_bolt = false;
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseEffectTargets { legal_targets, .. } if !legal_targets.is_empty() => {
            offered_bolt |= legal_targets.contains(&Target::Object(bolt));
            Action::ChooseEffectTarget(legal_targets[0])
        }
        Decision::ChooseEffectTargets { .. } => Action::FinishEffectSelection,
        other => first_answer(other),
    });
    assert!(!offered_bolt, "only blue cards");
    assert_eq!(zone(&state, opt), Zone::Exile);
    assert_eq!(zone(&state, brainstorm), Zone::Exile);
    assert_eq!(zone(&state, bolt), Zone::Graveyard);
    assert_eq!(zone(&state, flash), Zone::Exile);
    assert_eq!(state.players[0].hand.len(), 1, "looked at two, took one");
}

#[test]
fn foil_and_thwart_counter_for_their_alternative_costs() {
    let mut state = islands();
    let bolt = put(&mut state, P1, "Lightning Bolt", Zone::Hand);
    let foil = put(&mut state, P0, "Foil", Zone::Hand);
    let island = put(&mut state, P0, "Island", Zone::Hand);
    let other = put(&mut state, P0, "Opt", Zone::Hand);
    state.active_player = P1;
    state.priority_player = P1;
    pool(&mut state, P1, &[ManaColor::R], 0);
    cast(&mut state, bolt, &[Target::Player(P0)]);
    pass_to(&mut state, P0);
    assert!(castable(&mut state, foil));
    act(&mut state, Action::CastSpell(foil));
    let mut costs = Vec::new();
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseCastMode { .. } => Action::ChooseCastMode(CastMode::Alternative),
        Decision::ChooseCostTargets { candidates, .. } => {
            let pick = if costs.is_empty() {
                assert_eq!(candidates, vec![island], "the Island card first");
                island
            } else {
                assert!(candidates.contains(&other));
                other
            };
            costs.push(pick);
            Action::ChooseCostTarget(pick)
        }
        other => first_answer(other),
    });
    assert_eq!(zone(&state, island), Zone::Graveyard);
    assert_eq!(zone(&state, other), Zone::Graveyard);
    assert_eq!(zone(&state, bolt), Zone::Graveyard);
    assert_eq!(state.players[0].life, 20, "countered");

    let mut state = islands();
    let bolt = put(&mut state, P1, "Lightning Bolt", Zone::Hand);
    let thwart = put(&mut state, P0, "Thwart", Zone::Hand);
    let islands = (0..3)
        .map(|_| put(&mut state, P0, "Island", Zone::Battlefield))
        .collect::<Vec<_>>();
    state.active_player = P1;
    state.priority_player = P1;
    pool(&mut state, P1, &[ManaColor::R], 0);
    cast(&mut state, bolt, &[Target::Player(P0)]);
    pass_to(&mut state, P0);
    act(&mut state, Action::CastSpell(thwart));
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseCastMode { .. } => Action::ChooseCastMode(CastMode::Alternative),
        other => first_answer(other),
    });
    for island in islands {
        assert!(hand_contains(&state, P0, island));
    }
    assert_eq!(state.players[0].life, 20, "countered");
}

#[test]
fn funeral_charm_discards_pumps_or_grants_swampwalk() {
    let mut state = islands();
    let card = put(&mut state, P1, "Island", Zone::Hand);
    let charm = put(&mut state, P0, "Funeral Charm", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 0);
    act(&mut state, Action::CastSpell(charm));
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseSpellMode { .. } => Action::ChooseSpellMode(0),
        Decision::ChooseTargets { .. } => Action::ChooseTarget(Target::Player(P1)),
        other => first_answer(other),
    });
    assert_eq!(zone(&state, card), Zone::Graveyard);

    let mut state = islands();
    let swiftspear = put(&mut state, P0, "Monastery Swiftspear", Zone::Battlefield);
    let charm = put(&mut state, P0, "Funeral Charm", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 0);
    act(&mut state, Action::CastSpell(charm));
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseSpellMode { .. } => Action::ChooseSpellMode(1),
        Decision::ChooseTargets { .. } => Action::ChooseTarget(Target::Object(swiftspear)),
        other => first_answer(other),
    });
    // Monastery Swiftspear's prowess adds +1/+1 for the cast.
    assert_eq!(power_toughness(&state, swiftspear), (4, 2));

    let mut state = islands();
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    let blocker = put(&mut state, P1, "Monastery Swiftspear", Zone::Battlefield);
    put(&mut state, P1, "Swamp", Zone::Battlefield);
    let charm = put(&mut state, P0, "Funeral Charm", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 0);
    act(&mut state, Action::CastSpell(charm));
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseSpellMode { .. } => Action::ChooseSpellMode(2),
        Decision::ChooseTargets { .. } => Action::ChooseTarget(Target::Object(elves)),
        other => first_answer(other),
    });
    assert!(statics::has_swampwalk(&state, elves));
    pass_until(&mut state, |state| state.step == Step::DeclareAttackers);
    act(&mut state, Action::DeclareAttackers(vec![elves]));
    pass_until(&mut state, |state| state.step == Step::DeclareBlockers);
    assert!(
        engine::step(&mut state, Action::DeclareBlockers(vec![(blocker, elves)])).is_err(),
        "swampwalk"
    );
}

#[test]
fn incinerate_deals_three_and_stops_regeneration() {
    let mut state = islands();
    let troll = put(&mut state, P1, "Golgari Grave-Troll", Zone::Battlefield);
    state.objects.get_mut(troll).counters.plus1_plus1 = 2;
    mtg_kernel::eternal_vintage_e7b_v1::statics::add_regeneration_shield(&mut state, troll);
    let incinerate = put(&mut state, P0, "Incinerate", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 1);
    cast(&mut state, incinerate, &[Target::Object(troll)]);
    settled(&mut state);
    assert_eq!(zone(&state, troll), Zone::Graveyard, "can't be regenerated");

    // A shield saves it from Lightning Bolt.
    let mut state = islands();
    let troll = put(&mut state, P1, "Golgari Grave-Troll", Zone::Battlefield);
    state.objects.get_mut(troll).counters.plus1_plus1 = 2;
    mtg_kernel::eternal_vintage_e7b_v1::statics::add_regeneration_shield(&mut state, troll);
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 0);
    cast(&mut state, bolt, &[Target::Object(troll)]);
    settled(&mut state);
    assert_eq!(zone(&state, troll), Zone::Battlefield, "regenerated");

    let mut state = islands();
    let incinerate = put(&mut state, P0, "Incinerate", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 1);
    cast(&mut state, incinerate, &[Target::Player(P1)]);
    settled(&mut state);
    assert_eq!(state.players[1].life, 17);
}

#[test]
fn smother_destroys_a_small_creature_that_cant_regenerate() {
    let mut state = islands();
    let elves = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    let angel = put(&mut state, P1, "Exalted Angel", Zone::Battlefield);
    let smother = put(&mut state, P0, "Smother", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 1);
    act(&mut state, Action::CastSpell(smother));
    match next(&mut state) {
        Decision::ChooseTargets { legal_targets, .. } => {
            assert!(legal_targets.contains(&Target::Object(elves)));
            assert!(
                !legal_targets.contains(&Target::Object(angel)),
                "mana value 6"
            );
            act(&mut state, Action::ChooseTarget(Target::Object(elves)));
        }
        other => panic!("{other:?}"),
    }
    settled(&mut state);
    assert_eq!(zone(&state, elves), Zone::Graveyard);
}

#[test]
fn portent_reorders_target_library_may_shuffle_and_draws_next_upkeep() {
    let mut state = game(&["Island", "Opt", "Brainstorm", "Swamp", "Forest"]);
    let library = state.players[1].library.clone();
    let portent = put(&mut state, P0, "Portent", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 0);
    cast(&mut state, portent, &[Target::Player(P1)]);
    // Order the top three in reverse, then decline the shuffle.
    let reversed = [library[2], library[1], library[0]];
    let mut picks = 0;
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseEffectTargets { .. } => {
            let card = reversed[picks];
            picks += 1;
            Action::ChooseEffectTarget(Target::Object(card))
        }
        Decision::ChooseEffectOption { .. } => Action::ChooseEffectOption(1),
        other => first_answer(other),
    });
    assert_eq!(&state.players[1].library[..3], &reversed);
    assert!(state.players[0].hand.is_empty());
    pass_until(&mut state, |state| state.players[0].hand.len() == 1);
    assert_eq!(state.active_player, P1, "drew at the next turn's upkeep");
    assert_eq!(state.step, Step::Upkeep);
}

#[test]
fn price_of_progress_punishes_nonbasic_lands() {
    let mut state = islands();
    put(&mut state, P0, "Wasteland", Zone::Battlefield);
    put(&mut state, P0, "Mountain", Zone::Battlefield);
    put(&mut state, P1, "Volcanic Island", Zone::Battlefield);
    put(&mut state, P1, "Rishadan Port", Zone::Battlefield);
    let price = put(&mut state, P0, "Price of Progress", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 1);
    cast(&mut state, price, &[]);
    settled(&mut state);
    assert_eq!(state.players[0].life, 18);
    assert_eq!(state.players[1].life, 16);
}

#[test]
fn skeletal_scrying_exiles_x_cards_to_draw_x_and_lose_x() {
    let mut state = islands();
    let scrying = put(&mut state, P0, "Skeletal Scrying", Zone::Hand);
    let graveyard = (0..3)
        .map(|_| put(&mut state, P0, "Swamp", Zone::Graveyard))
        .collect::<Vec<_>>();
    pool(&mut state, P0, &[ManaColor::B], 2);
    act(&mut state, Action::CastSpell(scrying));
    let mut exiled = 0;
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseEffectTargets { legal_targets, .. } if exiled < 2 => {
            exiled += 1;
            Action::ChooseEffectTarget(legal_targets[0])
        }
        Decision::ChooseEffectTargets { can_finish, .. } => {
            assert!(can_finish, "three mana pays at most X = 2");
            Action::FinishEffectSelection
        }
        other => first_answer(other),
    });
    assert_eq!(
        graveyard
            .iter()
            .filter(|&&card| zone(&state, card) == Zone::Exile)
            .count(),
        2
    );
    assert_eq!(state.players[0].hand.len(), 2);
    assert_eq!(state.players[0].life, 18);
}

#[test]
fn vision_charm_mills_changes_land_types_or_phases_out_an_artifact() {
    let mut state = islands();
    let charm = put(&mut state, P0, "Vision Charm", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 0);
    act(&mut state, Action::CastSpell(charm));
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseSpellMode { .. } => Action::ChooseSpellMode(0),
        Decision::ChooseTargets { .. } => Action::ChooseTarget(Target::Player(P1)),
        other => first_answer(other),
    });
    assert_eq!(state.players[1].graveyard.len(), 4);

    // Islands become Swamps until end of turn.
    let mut state = islands();
    let island = put(&mut state, P1, "Island", Zone::Battlefield);
    let volcanic = put(&mut state, P1, "Volcanic Island", Zone::Battlefield);
    let charm = put(&mut state, P0, "Vision Charm", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 0);
    act(&mut state, Action::CastSpell(charm));
    let mut option = 0;
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseSpellMode { .. } => Action::ChooseSpellMode(1),
        Decision::ChooseEffectOption { .. } => {
            option += 1;
            // First Island (stable land type order: Plains, Island, ...),
            // then Swamp.
            Action::ChooseEffectOption(if option == 1 { 1 } else { 2 })
        }
        other => first_answer(other),
    });
    for land in [island, volcanic] {
        assert!(has_subtype(&state, land, Subtype::Swamp));
        assert!(!has_subtype(&state, land, Subtype::Island));
    }
    assert!(!has_subtype(&state, volcanic, Subtype::Mountain));

    let mut state = islands();
    let ring = put(&mut state, P1, "Sol Ring", Zone::Battlefield);
    let charm = put(&mut state, P0, "Vision Charm", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 0);
    act(&mut state, Action::CastSpell(charm));
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseSpellMode { .. } => Action::ChooseSpellMode(2),
        Decision::ChooseTargets { .. } => Action::ChooseTarget(Target::Object(ring)),
        other => first_answer(other),
    });
    assert!(!state.players[1].battlefield.contains(&ring), "phased out");
}

// ---- Artifacts and enchantments ------------------------------------------------------

#[test]
fn cursed_scroll_damages_when_the_random_card_has_the_named_name() {
    let mut state = islands();
    let scroll = put(&mut state, P0, "Cursed Scroll", Zone::Battlefield);
    put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P0, &[], 3);
    activate(&mut state, scroll, 0, &[Target::Player(P1)]);
    // Name "Lightning Bolt": its letter, then the name.
    let (letter, name) = naming_options("Lightning Bolt");
    let mut answers = vec![name, letter];
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseEffectOption { .. } => {
            Action::ChooseEffectOption(answers.pop().expect("two naming options"))
        }
        other => first_answer(other),
    });
    assert_eq!(state.players[1].life, 18);

    // Naming another card deals nothing.
    let mut state = islands();
    let scroll = put(&mut state, P0, "Cursed Scroll", Zone::Battlefield);
    put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P0, &[], 3);
    activate(&mut state, scroll, 0, &[Target::Player(P1)]);
    let (letter, name) = naming_options("Island");
    let mut answers = vec![name, letter];
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseEffectOption { .. } => {
            Action::ChooseEffectOption(answers.pop().expect("two naming options"))
        }
        other => first_answer(other),
    });
    assert_eq!(state.players[1].life, 20);
}

/// Cursed Scroll's naming options for `named`: its first letter's option,
/// then its option among the names with that letter (every executable
/// nontoken card name once, letters sorted, names in registry order).
fn naming_options(named: &str) -> (u16, u16) {
    let mut seen: Vec<&str> = Vec::new();
    let mut groups: Vec<(char, Vec<&str>)> = Vec::new();
    for def in CARD_DEFS.iter() {
        if def.is_token
            || !def.is_executable()
            || def.object_name.is_empty()
            || seen.contains(&def.object_name)
        {
            continue;
        }
        seen.push(def.object_name);
        let letter = def.object_name.chars().next().unwrap().to_ascii_uppercase();
        match groups.iter_mut().find(|(group, _)| *group == letter) {
            Some((_, names)) => names.push(def.object_name),
            None => groups.push((letter, vec![def.object_name])),
        }
    }
    groups.sort_by_key(|(letter, _)| *letter);
    let letter = groups
        .iter()
        .position(|(_, names)| names.contains(&named))
        .unwrap();
    let name = groups[letter]
        .1
        .iter()
        .position(|name| *name == named)
        .unwrap();
    (letter as u16, name as u16)
}

#[test]
fn powder_keg_counts_fuses_and_destroys_that_mana_value() {
    let mut state = islands();
    let keg = put(&mut state, P0, "Powder Keg", Zone::Battlefield);
    let ring = put(&mut state, P1, "Sol Ring", Zone::Battlefield);
    let elves = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    let bauble = put(&mut state, P1, "Mishra's Bauble", Zone::Battlefield);
    // P0's next upkeep: add a fuse counter.
    pass_until(&mut state, |state| {
        state.active_player == P0 && state.step == Step::Upkeep && !state.stack.is_empty()
    });
    settle_picking(&mut state, 0, None);
    assert_eq!(statics::fuse_counters(&state, keg), 1);
    pass_until(&mut state, |state| {
        state.step == Step::Main1 && state.active_player == P0
    });
    activate(&mut state, keg, 0, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, keg), Zone::Graveyard);
    assert_eq!(zone(&state, ring), Zone::Graveyard);
    assert_eq!(zone(&state, elves), Zone::Graveyard);
    assert_eq!(zone(&state, bauble), Zone::Battlefield, "mana value 0");
}

#[test]
fn seal_of_fire_sacrifices_for_two_damage() {
    let mut state = islands();
    let seal = put(&mut state, P0, "Seal of Fire", Zone::Battlefield);
    activate(&mut state, seal, 0, &[Target::Player(P1)]);
    settled(&mut state);
    assert_eq!(zone(&state, seal), Zone::Graveyard);
    assert_eq!(state.players[1].life, 18);
}

#[test]
fn sulfuric_vortex_burns_each_upkeep_and_stops_life_gain() {
    let mut state = islands();
    put(&mut state, P0, "Sulfuric Vortex", Zone::Battlefield);
    let angel = put(&mut state, P1, "Exalted Angel", Zone::Battlefield);
    pass_until(&mut state, |state| {
        state.active_player == P1 && state.step == Step::Upkeep && !state.stack.is_empty()
    });
    settled(&mut state);
    assert_eq!(state.players[1].life, 18);
    // Lifelink gains nothing.
    let bolt_target = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    damage(&mut state, angel, bolt_target, 4);
    settled(&mut state);
    assert_eq!(state.players[1].life, 18, "no life gained");
}

#[test]
fn sylvan_library_draws_two_and_charges_four_life_or_puts_back() {
    let mut state = game(&["Island"; 30]);
    put(&mut state, P0, "Sylvan Library", Zone::Battlefield);
    pass_until(&mut state, |state| {
        state.active_player == P0 && state.step == Step::Draw && !state.stack.is_empty()
    });
    // Draw two more, choose two of the three drawn, pay 4 life for one.
    let mut picked = 0;
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseEffectOption { .. } => Action::ChooseEffectOption(0),
        Decision::ChooseEffectTargets {
            legal_targets,
            selected_count,
            ..
        } => {
            if picked == 0 {
                assert_eq!(legal_targets.len(), 3, "the three cards drawn this turn");
            }
            if picked < 2 || (picked == 2 && selected_count == 0) {
                picked += 1;
                Action::ChooseEffectTarget(legal_targets[0])
            } else {
                Action::FinishEffectSelection
            }
        }
        other => first_answer(other),
    });
    assert_eq!(state.players[0].hand.len(), 2, "drew three, put one back");
    assert_eq!(state.players[0].life, 16);
}

// ---- Creatures ---------------------------------------------------------------------

#[test]
fn anurid_brushhopper_blinks_until_the_next_end_step() {
    let mut state = islands();
    let hopper = put(&mut state, P0, "Anurid Brushhopper", Zone::Battlefield);
    put(&mut state, P0, "Island", Zone::Hand);
    put(&mut state, P0, "Island", Zone::Hand);
    activate(&mut state, hopper, 0, &[]);
    settle_taking(&mut state);
    assert_eq!(zone(&state, hopper), Zone::Exile);
    pass_until(&mut state, |state| {
        state.step == Step::Cleanup || zone(state, hopper) == Zone::Battlefield
    });
    assert_eq!(zone(&state, hopper), Zone::Battlefield);
    assert_eq!(state.objects.get(hopper).owner, P0);
}

#[test]
fn ball_lightning_is_sacrificed_at_the_end_step() {
    let mut state = islands();
    let ball = put(&mut state, P0, "Ball Lightning", Zone::Battlefield);
    assert!(keyword(&state, ball, Keywords::HASTE));
    pass_until(&mut state, |state| state.step == Step::Cleanup);
    assert_eq!(zone(&state, ball), Zone::Graveyard);
}

#[test]
fn exalted_angel_is_cast_face_down_and_turned_up_for_its_morph_cost() {
    let mut state = islands();
    let angel = put(&mut state, P0, "Exalted Angel", Zone::Hand);
    pool(&mut state, P0, &[], 3);
    act(&mut state, Action::CastSpell(angel));
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseCastMode { .. } => Action::ChooseCastMode(CastMode::Alternative),
        other => first_answer(other),
    });
    assert_eq!(zone(&state, angel), Zone::Battlefield);
    assert!(state.objects.get(angel).v4.face_down_v1.is_some());
    assert_eq!(power_toughness(&state, angel), (2, 2));
    assert_eq!(
        state.objects.get(angel).v4.ward_generic,
        0,
        "morph has no ward"
    );
    pool(&mut state, P0, &[ManaColor::W, ManaColor::W], 2);
    assert!(activatable(&mut state, angel, 255));
    act(&mut state, Action::ActivateAbility(angel, 255));
    assert!(state.objects.get(angel).v4.face_down_v1.is_none());
    assert_eq!(power_toughness(&state, angel), (4, 5));
    assert!(keyword(&state, angel, Keywords::LIFELINK));
}

#[test]
fn gempalm_incinerator_cycles_for_damage_per_goblin() {
    let mut state = islands();
    put(&mut state, P0, "Goblin Lackey", Zone::Battlefield);
    put(&mut state, P1, "Goblin Token", Zone::Battlefield);
    let target = put(&mut state, P1, "Exalted Angel", Zone::Battlefield);
    let gempalm = put(&mut state, P0, "Gempalm Incinerator", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 1);
    activate(&mut state, gempalm, 0, &[]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseTargets { .. } => Action::ChooseTarget(Target::Object(target)),
        Decision::ChooseEffectOption { .. } => Action::ChooseEffectOption(0),
        other => first_answer(other),
    });
    assert_eq!(state.objects.get(target).damage, 2, "two Goblins");
    assert_eq!(state.players[0].hand.len(), 1, "cycling drew");
}

#[test]
fn goblin_goon_needs_more_creatures_to_attack_or_block() {
    let mut state = islands();
    let goon = put(&mut state, P0, "Goblin Goon", Zone::Battlefield);
    put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    assert!(!eligible_attackers(&mut state).contains(&goon));
    let mut state = islands();
    let goon = put(&mut state, P0, "Goblin Goon", Zone::Battlefield);
    put(&mut state, P0, "Goblin Token", Zone::Battlefield);
    put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    assert!(eligible_attackers(&mut state).contains(&goon));
}

#[test]
fn goblin_lackey_puts_a_goblin_from_hand_when_it_hits() {
    let mut state = islands();
    let lackey = put(&mut state, P0, "Goblin Lackey", Zone::Battlefield);
    let siege = put(&mut state, P0, "Siege-Gang Commander", Zone::Hand);
    attack_with(&mut state, vec![lackey], |_, decision| match decision {
        Decision::ChooseEffectTargets {
            legal_targets,
            selected_count: 0,
            ..
        } if legal_targets.contains(&Target::Object(siege)) => {
            Some(Action::ChooseEffectTarget(Target::Object(siege)))
        }
        _ => None,
    });
    assert_eq!(zone(&state, siege), Zone::Battlefield);
    assert_eq!(
        controls(&state, P0, "Goblin Token").len(),
        3,
        "its entry trigger"
    );
}

#[test]
fn goblin_matron_finds_a_goblin() {
    let mut state = game(&["Island", "Mogg Fanatic", "Island", "Island"]);
    let matron = put(&mut state, P0, "Goblin Matron", Zone::Hand);
    enter(&mut state, matron);
    settle_taking(&mut state);
    let hand = &state.players[0].hand;
    assert_eq!(hand.len(), 1);
    assert_eq!(name_of(&state, hand[0]), "Mogg Fanatic");
}

#[test]
fn goblin_piledriver_grows_per_attacking_goblin_and_resists_blue() {
    let mut state = islands();
    let piledriver = put(&mut state, P0, "Goblin Piledriver", Zone::Battlefield);
    let lackey = put(&mut state, P0, "Goblin Lackey", Zone::Battlefield);
    pass_until(&mut state, |state| state.step == Step::DeclareAttackers);
    act(
        &mut state,
        Action::DeclareAttackers(vec![piledriver, lackey]),
    );
    run_until(
        &mut state,
        |state| state.step == Step::DeclareBlockers || state.step == Step::CombatDamage,
        |_, _| None,
    );
    assert_eq!(power_toughness(&state, piledriver).0, 3);

    // Protection from blue: Ice can't target it.
    let mut state = islands();
    let piledriver = put(&mut state, P1, "Goblin Piledriver", Zone::Battlefield);
    let island = put(&mut state, P1, "Island", Zone::Battlefield);
    let ice = put(&mut state, P0, "Fire // Ice", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 1);
    act(&mut state, Action::CastSpell(ice));
    // Only Ice is affordable, so its half is chosen without asking.
    match next(&mut state) {
        Decision::ChooseTargets { legal_targets, .. } => {
            assert!(!legal_targets.contains(&Target::Object(piledriver)));
            assert!(legal_targets.contains(&Target::Object(island)));
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn goblin_ringleader_takes_the_revealed_goblins() {
    let mut state = game(&["Mogg Fanatic", "Island", "Goblin Lackey", "Island", "Swamp"]);
    let ringleader = put(&mut state, P0, "Goblin Ringleader", Zone::Hand);
    enter(&mut state, ringleader);
    settle_ordering(&mut state);
    let names = state.players[0]
        .hand
        .iter()
        .map(|&card| name_of(&state, card))
        .collect::<Vec<_>>();
    assert_eq!(names.len(), 2);
    assert!(names.contains(&"Mogg Fanatic") && names.contains(&"Goblin Lackey"));
    let library = &state.players[0].library;
    assert_eq!(
        name_of(&state, library[0]),
        "Swamp",
        "the rest went to the bottom"
    );
}

#[test]
fn goblin_sharpshooter_untaps_when_a_creature_dies() {
    let mut state = islands();
    let sharpshooter = put(&mut state, P0, "Goblin Sharpshooter", Zone::Battlefield);
    let elves = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    activate(&mut state, sharpshooter, 0, &[Target::Object(elves)]);
    settled(&mut state);
    assert_eq!(zone(&state, elves), Zone::Graveyard);
    assert!(
        !state.objects.get(sharpshooter).tapped,
        "untapped by the death"
    );
    state.objects.get_mut(sharpshooter).tapped = true;
    pass_until(&mut state, |state| {
        state.active_player == P0 && state.step == Step::Upkeep
    });
    assert!(state.objects.get(sharpshooter).tapped, "doesn't untap");
}

#[test]
fn goblin_vandal_trades_its_damage_for_an_artifact() {
    let mut state = islands();
    let vandal = put(&mut state, P0, "Goblin Vandal", Zone::Battlefield);
    let ring = put(&mut state, P1, "Sol Ring", Zone::Battlefield);
    put(&mut state, P0, "Mountain", Zone::Battlefield);
    // An instant-speed action keeps P0's priority windows open, so the
    // check below runs in the declare blockers step.
    put(&mut state, P0, "Seal of Fire", Zone::Battlefield);
    pass_until(&mut state, |state| state.step == Step::DeclareAttackers);
    act(&mut state, Action::DeclareAttackers(vec![vandal]));
    run_until(
        &mut state,
        |state| zone(state, ring) == Zone::Graveyard,
        |_, decision| match decision {
            Decision::ChooseTargets { .. } => Some(Action::ChooseTarget(Target::Object(ring))),
            _ => None,
        },
    );
    assert_eq!(state.step, Step::DeclareBlockers);
    assert!(statics::assigns_no_combat_damage(&state, vandal));
    attack_finish(&mut state);
    assert_eq!(zone(&state, ring), Zone::Graveyard);
    assert_eq!(state.players[1].life, 20, "it assigned no combat damage");
}

#[test]
fn goblin_warchief_discounts_and_hastes_goblins() {
    let mut state = islands();
    put(&mut state, P0, "Goblin Warchief", Zone::Battlefield);
    let lackey = put(&mut state, P0, "Goblin Lackey", Zone::Hand);
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    // Goblin Lackey costs {R}: still {R}. Goblin Matron's {2}{R} costs {1}{R}.
    let matron = put(&mut state, P0, "Goblin Matron", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 1);
    assert!(castable(&mut state, matron));
    cast(&mut state, matron, &[]);
    settle_taking(&mut state);
    assert!(keyword(&state, matron, Keywords::HASTE));
    assert!(!keyword(&state, elves, Keywords::HASTE));
    assert_eq!(zone(&state, lackey), Zone::Hand);
}

#[test]
fn graveborn_muse_draws_and_drains_per_zombie() {
    let mut state = game(&["Island"; 10]);
    put(&mut state, P0, "Graveborn Muse", Zone::Battlefield);
    put(&mut state, P0, "Withered Wretch", Zone::Battlefield);
    pass_until(&mut state, |state| {
        state.active_player == P0 && state.step == Step::Upkeep && !state.stack.is_empty()
    });
    settled(&mut state);
    assert_eq!(state.players[0].hand.len(), 2);
    assert_eq!(state.players[0].life, 18);
}

#[test]
fn grim_lavamancer_exiles_two_graveyard_cards_to_shoot() {
    let mut state = islands();
    let lavamancer = put(&mut state, P0, "Grim Lavamancer", Zone::Battlefield);
    put(&mut state, P0, "Island", Zone::Graveyard);
    put(&mut state, P0, "Island", Zone::Graveyard);
    pool(&mut state, P0, &[ManaColor::R], 0);
    activate(&mut state, lavamancer, 0, &[Target::Player(P1)]);
    settle_with(&mut state, |_, decision| first_answer(decision));
    assert_eq!(state.players[1].life, 18);
    assert!(state.players[0].graveyard.is_empty());
}

#[test]
fn hypnotic_specter_discards_at_random_when_it_hits() {
    let mut state = islands();
    let specter = put(&mut state, P0, "Hypnotic Specter", Zone::Battlefield);
    let card = put(&mut state, P1, "Island", Zone::Hand);
    attack_with(&mut state, vec![specter], |_, _| None);
    assert_eq!(zone(&state, card), Zone::Graveyard);
    assert_eq!(state.players[1].life, 18);
}

#[test]
fn jackal_pup_hurts_its_controller_when_dealt_damage() {
    let mut state = islands();
    let pup = put(&mut state, P0, "Jackal Pup", Zone::Battlefield);
    let source = put(&mut state, P1, "Monastery Swiftspear", Zone::Battlefield);
    damage(&mut state, source, pup, 3);
    settled(&mut state);
    assert_eq!(state.players[0].life, 17);
}

#[test]
fn knight_of_stromgald_strikes_first_pumps_and_resists_white() {
    let mut state = islands();
    let knight = put(&mut state, P0, "Knight of Stromgald", Zone::Battlefield);
    pool(
        &mut state,
        P0,
        &[ManaColor::B, ManaColor::B, ManaColor::B],
        0,
    );
    activate(&mut state, knight, 0, &[]);
    settled(&mut state);
    assert!(keyword(&state, knight, Keywords::FIRST_STRIKE));
    activate(&mut state, knight, 1, &[]);
    settled(&mut state);
    assert_eq!(power_toughness(&state, knight), (3, 1));

    let mut state = islands();
    let knight = put(&mut state, P1, "Knight of Stromgald", Zone::Battlefield);
    let plow = put(&mut state, P0, "Swords to Plowshares", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W], 0);
    assert!(!castable(&mut state, plow), "no white spell can target it");
    assert_eq!(zone(&state, knight), Zone::Battlefield);
}

#[test]
fn mogg_fanatic_sacrifices_to_ping() {
    let mut state = islands();
    let fanatic = put(&mut state, P0, "Mogg Fanatic", Zone::Battlefield);
    activate(&mut state, fanatic, 0, &[Target::Player(P1)]);
    settled(&mut state);
    assert_eq!(zone(&state, fanatic), Zone::Graveyard);
    assert_eq!(state.players[1].life, 19);
}

#[test]
fn mystic_enforcer_grows_and_flies_with_threshold() {
    let mut state = islands();
    let enforcer = put(&mut state, P0, "Mystic Enforcer", Zone::Battlefield);
    assert_eq!(power_toughness(&state, enforcer), (3, 3));
    assert!(!keyword(&state, enforcer, Keywords::FLYING));
    for _ in 0..7 {
        put(&mut state, P0, "Island", Zone::Graveyard);
    }
    assert_eq!(power_toughness(&state, enforcer), (6, 6));
    assert!(keyword(&state, enforcer, Keywords::FLYING));
    let specter = put(&mut state, P1, "Hypnotic Specter", Zone::Battlefield);
    damage(&mut state, specter, enforcer, 3);
    assert_eq!(
        state.objects.get(enforcer).damage,
        0,
        "protection from black"
    );
}

#[test]
fn nantuko_shade_pumps_for_black() {
    let mut state = islands();
    let shade = put(&mut state, P0, "Nantuko Shade", Zone::Battlefield);
    pool(&mut state, P0, &[ManaColor::B, ManaColor::B], 0);
    activate(&mut state, shade, 0, &[]);
    settled(&mut state);
    activate(&mut state, shade, 0, &[]);
    settled(&mut state);
    assert_eq!(power_toughness(&state, shade), (4, 3));
}

#[test]
fn siege_gang_commander_makes_goblins_and_sacrifices_them() {
    let mut state = islands();
    let siege = put(&mut state, P0, "Siege-Gang Commander", Zone::Hand);
    enter(&mut state, siege);
    settled(&mut state);
    let tokens = controls(&state, P0, "Goblin Token");
    assert_eq!(tokens.len(), 3);
    pool(&mut state, P0, &[ManaColor::R], 1);
    activate(&mut state, siege, 0, &[Target::Player(P1)]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseCostTargets { candidates, .. } => {
            assert!(candidates.contains(&siege), "it is a Goblin itself");
            Action::ChooseCostTarget(tokens[0])
        }
        other => first_answer(other),
    });
    assert_eq!(state.players[1].life, 18);
    assert_eq!(controls(&state, P0, "Goblin Token").len(), 2);
}

#[test]
fn skirk_prospector_sacrifices_a_goblin_for_red() {
    let mut state = islands();
    let prospector = put(&mut state, P0, "Skirk Prospector", Zone::Battlefield);
    state.objects.get_mut(prospector).summoning_sick = true;
    let token = put(&mut state, P0, "Goblin Token", Zone::Battlefield);
    next(&mut state);
    act(
        &mut state,
        Action::ActivateManaAbilityWithCostTarget(prospector, ManaColor::R, token),
    );
    assert_eq!(mana(&state, P0, ManaColor::R), 1);
    assert_eq!(zone(&state, token), Zone::Graveyard);
    next(&mut state);
    act(
        &mut state,
        Action::ActivateManaAbilityWithCostTarget(prospector, ManaColor::R, prospector),
    );
    assert_eq!(mana(&state, P0, ManaColor::R), 2);
    assert_eq!(zone(&state, prospector), Zone::Graveyard);
}

#[test]
fn terravore_counts_land_cards_in_all_graveyards() {
    let mut state = islands();
    let terravore = put(&mut state, P0, "Terravore", Zone::Battlefield);
    put(&mut state, P0, "Forest", Zone::Graveyard);
    put(&mut state, P1, "Wasteland", Zone::Graveyard);
    put(&mut state, P1, "Lightning Bolt", Zone::Graveyard);
    assert_eq!(power_toughness(&state, terravore), (2, 2));
    assert!(keyword(&state, terravore, Keywords::TRAMPLE));
}

#[test]
fn weathered_wayfarer_fetches_a_land_when_behind_on_lands() {
    let mut state = game(&["Island", "Wasteland", "Island"]);
    let wayfarer = put(&mut state, P0, "Weathered Wayfarer", Zone::Battlefield);
    pool(&mut state, P0, &[ManaColor::W], 0);
    assert!(!activatable(&mut state, wayfarer, 0), "not behind on lands");
    put(&mut state, P1, "Island", Zone::Battlefield);
    activate(&mut state, wayfarer, 0, &[]);
    settle_taking(&mut state);
    assert_eq!(state.players[0].hand.len(), 1);
    assert!(def(name_of(&state, state.players[0].hand[0])).is_land);
}

#[test]
fn wild_mongrel_discards_to_grow_and_change_color() {
    let mut state = islands();
    let mongrel = put(&mut state, P0, "Wild Mongrel", Zone::Battlefield);
    put(&mut state, P0, "Island", Zone::Hand);
    activate(&mut state, mongrel, 0, &[]);
    // WUBRG order: blue.
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseEffectOption { .. } => Action::ChooseEffectOption(1),
        other => first_answer(other),
    });
    assert_eq!(power_toughness(&state, mongrel), (3, 3));
    let blue = mtg_kernel::card_def::mana_color_mask(ManaColor::U);
    assert_eq!(engine::object_color_mask(&state, mongrel), blue);
}

#[test]
fn withered_wretch_exiles_a_graveyard_card() {
    let mut state = islands();
    let wretch = put(&mut state, P0, "Withered Wretch", Zone::Battlefield);
    let card = put(&mut state, P1, "Lightning Bolt", Zone::Graveyard);
    pool(&mut state, P0, &[], 1);
    activate(&mut state, wretch, 0, &[Target::Object(card)]);
    settled(&mut state);
    assert_eq!(zone(&state, card), Zone::Exile);
}
