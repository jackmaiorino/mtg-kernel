//! Sideboard wave S1q: native rules tests for Cityscape Leveler, Invasion
//! Submersible, Summon: Bahamut and the Powerstone token, plus public-session
//! consumer games with replay verification.
#![cfg(feature = "eternal-fixtures")]

use crate::eternal_catalog_v1::{play_observed, Policy};
use mtg_kernel::card_def::{
    card_id_by_name, CardCapability, CardDef, CardType, Keywords, Subtype, CARD_DEFS,
};
use mtg_kernel::engine::{self, Action, Decision};
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::limited_session_v1::{CustomCardCountV1, CustomDeckV1};
use mtg_kernel::mana::ManaColor;
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Target, Zone};
use serde_json::Value;
use std::collections::BTreeSet;

const P0: PlayerId = PlayerId::P0;
const P1: PlayerId = PlayerId::P1;

const LEVELER: &str = "Cityscape Leveler";
const SUBMERSIBLE: &str = "Invasion Submersible";
const BAHAMUT: &str = "Summon: Bahamut";
const POWERSTONE: &str = "Powerstone Token";

fn def(name: &str) -> &'static CardDef {
    &CARD_DEFS[id(name) as usize]
}

fn id(name: &str) -> u16 {
    card_id_by_name(name).unwrap_or_else(|| panic!("{name}"))
}

/// First main phase with `active` active and holding priority, both libraries
/// twenty Islands.
fn game(active: PlayerId) -> GameState {
    let ids = vec![id("Island"); 20];
    let mut state = GameState::new_from_libraries(
        &ids,
        &ids,
        |id| CARD_DEFS[id as usize].object_name.into(),
        0x5331_7100_0001,
    );
    state.active_player = active;
    state.priority_player = active;
    state.step = Step::Main1;
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

fn tapped(state: &GameState, object: ObjectId) -> bool {
    state.objects.get(object).tapped
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

/// The priority holder and the spells and abilities it may cast or activate.
fn priority(state: &mut GameState) -> (PlayerId, Vec<ObjectId>, Vec<(ObjectId, u8)>) {
    match next(state) {
        Decision::CastSpellOrPass {
            player,
            castable_spells,
            activatable_abilities,
            ..
        } => (player, castable_spells, activatable_abilities),
        other => panic!("expected priority, got {other:?}"),
    }
}

fn castable(state: &mut GameState, spell: ObjectId) -> bool {
    priority(state).1.contains(&spell)
}

/// Casts `spell` with the mana already available to its caster.
fn cast(state: &mut GameState, spell: ObjectId) {
    assert!(
        castable(state, spell),
        "{} not castable",
        state.objects.get(spell).name
    );
    act(state, Action::CastSpell(spell));
}

fn activatable(state: &mut GameState, source: ObjectId, index: u8) -> bool {
    priority(state).2.contains(&(source, index))
}

/// Activates `source`'s ability `index` with the mana already available.
fn activate(state: &mut GameState, source: ObjectId, index: u8) {
    assert!(
        activatable(state, source, index),
        "{} ability {index} not activatable",
        state.objects.get(source).name
    );
    act(state, Action::ActivateAbility(source, index));
}

/// Answers the pending trigger's "up to one" target prompt with `target`, or
/// with no target, returning the legal targets it offered.
fn trigger_target(state: &mut GameState, target: Option<ObjectId>) -> Vec<Target> {
    match settle(state) {
        Some(Decision::ChooseTargets {
            legal_targets,
            can_finish,
            ..
        }) => {
            assert!(can_finish, "up to one target");
            match target {
                Some(object) => {
                    assert!(
                        legal_targets.contains(&Target::Object(object)),
                        "{object:?} not in {legal_targets:?}"
                    );
                    act(state, Action::ChooseTarget(Target::Object(object)));
                }
                None => act(state, Action::FinishEffectSelection),
            }
            legal_targets
        }
        other => panic!("expected an up-to-one target prompt, got {other:?}"),
    }
}

fn power(state: &GameState, object: ObjectId) -> i32 {
    engine::effective_power(state, object)
}

fn toughness(state: &GameState, object: ObjectId) -> i32 {
    engine::effective_toughness(state, object)
}

fn creature(state: &GameState, object: ObjectId) -> bool {
    engine::object_has_type(state, object, CardType::Creature)
}

/// `player`'s Powerstone tokens on the battlefield.
fn powerstones(state: &GameState, player: PlayerId) -> Vec<ObjectId> {
    state.players[player.index()]
        .battlefield
        .iter()
        .copied()
        .filter(|&object| CARD_DEFS[state.objects.get(object).card_def as usize].name == POWERSTONE)
        .collect()
}

fn restricted(state: &GameState, player: PlayerId) -> usize {
    state.players[player.index()].restricted_mana_pool.0.len()
}

fn objects(targets: &[Target]) -> BTreeSet<ObjectId> {
    targets
        .iter()
        .filter_map(|target| match target {
            Target::Object(object) => Some(*object),
            _ => None,
        })
        .collect()
}

/// Moves `object` to exile and back: a new object on the battlefield.
fn blink(state: &mut GameState, object: ObjectId) {
    event::propose_and_commit(state, ProposedEvent::zone_change(object, Zone::Exile));
    event::propose_and_commit(state, ProposedEvent::zone_change(object, Zone::Battlefield));
    assert_eq!(zone(state, object), Zone::Battlefield);
}

/// Plays on, passing and declaring no attackers or blockers, until `until`
/// holds at a priority decision. `answer` may take any other decision first.
fn run_until(
    state: &mut GameState,
    until: impl Fn(&GameState) -> bool,
    mut answer: impl FnMut(&mut GameState, &Decision) -> Option<Action>,
) {
    for _ in 0..1000 {
        let decision = next(state);
        if matches!(decision, Decision::CastSpellOrPass { .. }) && until(state) {
            return;
        }
        if let Some(action) = answer(state, &decision) {
            act(state, action);
            continue;
        }
        let action = match decision {
            Decision::CastSpellOrPass { .. } => Action::Pass,
            Decision::DeclareAttackers { .. } => Action::DeclareAttackers(vec![]),
            Decision::DeclareBlockers { .. } => Action::DeclareBlockers(vec![]),
            Decision::OrderTriggers { pending, .. } => {
                Action::OrderTriggers((0..pending.len()).collect())
            }
            Decision::Discard { count, choices, .. } => {
                Action::Discard(choices.into_iter().take(count as usize).collect())
            }
            other => panic!("unexpected decision: {other:?}"),
        };
        act(state, action);
    }
    panic!("condition never reached");
}

// ---- Card Definitions ---------------------------------------------------------------

#[test]
fn s1q_definitions_have_printed_characteristics() {
    let leveler = def(LEVELER);
    assert_eq!(leveler.capability, CardCapability::Full);
    assert_eq!(leveler.types, &[CardType::Artifact, CardType::Creature]);
    assert_eq!(leveler.subtypes, &[Subtype::Construct]);
    assert!(leveler.colors.is_empty());
    assert_eq!(leveler.mana_value, 8);
    assert_eq!((leveler.power, leveler.toughness), (Some(8), Some(8)));
    assert!(leveler.keywords.has(Keywords::TRAMPLE));
    assert_eq!(leveler.activated_abilities.len(), 1, "Unearth {{8}}");
    assert_eq!(leveler.activated_abilities[0].activation_zone, Zone::Graveyard);

    let submersible = def(SUBMERSIBLE);
    assert_eq!(submersible.capability, CardCapability::Full);
    assert_eq!(submersible.types, &[CardType::Artifact]);
    assert_eq!(submersible.subtypes, &[Subtype::Vehicle]);
    assert_eq!(submersible.colors, &[ManaColor::U]);
    assert_eq!(submersible.mana_value, 3);
    assert_eq!(
        (submersible.power, submersible.toughness),
        (Some(0), Some(0))
    );
    assert_eq!(
        submersible.activated_abilities.len(),
        4,
        "one exhaust ability, by number of waterbend taps"
    );

    let bahamut = def(BAHAMUT);
    assert_eq!(bahamut.capability, CardCapability::Full);
    assert_eq!(bahamut.types, &[CardType::Enchantment, CardType::Creature]);
    assert_eq!(bahamut.subtypes, &[Subtype::Saga, Subtype::Dragon]);
    assert!(bahamut.colors.is_empty());
    assert_eq!(bahamut.mana_value, 9);
    assert_eq!((bahamut.power, bahamut.toughness), (Some(9), Some(9)));
    assert!(bahamut.keywords.has(Keywords::FLYING));
    assert_eq!(bahamut.saga.as_ref().expect("a Saga").chapter_effects.len(), 4);

    let powerstone = def(POWERSTONE);
    assert_eq!(powerstone.capability, CardCapability::Full);
    assert!(powerstone.is_token);
    assert_eq!(powerstone.types, &[CardType::Artifact]);
    assert_eq!(powerstone.subtypes, &[Subtype::Powerstone]);
    assert_eq!(powerstone.mana_ability_choices, &[ManaColor::C]);
    assert!(
        powerstone.mana_ability_def.is_some(),
        "not an automatic payment source"
    );
}

// ---- Cityscape Leveler --------------------------------------------------------------

/// "When you cast this spell ..., destroy up to one target nonland permanent.
/// Its controller creates a tapped Powerstone token." The token goes to the
/// target's controller, whoever that is; no target, no token. Lands are not
/// offered.
#[test]
fn leveler_cast_trigger_destroys_and_its_controller_gets_a_tapped_powerstone_in_both_seats() {
    for actor in [P0, P1] {
        let opponent = actor.opponent();
        for target_own in [false, true] {
            let mut state = game(actor);
            let leveler = put(&mut state, actor, LEVELER, Zone::Hand);
            let theirs = put(&mut state, opponent, "Sol Ring", Zone::Battlefield);
            let mine = put(&mut state, actor, "Grim Monolith", Zone::Battlefield);
            let island = put(&mut state, opponent, "Island", Zone::Battlefield);
            pool(&mut state, actor, &[], 8);
            cast(&mut state, leveler);
            let victim = if target_own { mine } else { theirs };
            let offered = trigger_target(&mut state, Some(victim));
            assert!(offered.contains(&Target::Object(theirs)));
            assert!(offered.contains(&Target::Object(mine)));
            assert!(!offered.contains(&Target::Object(island)), "nonland");
            assert!(
                !offered.contains(&Target::Object(leveler)),
                "the spell is not a permanent"
            );
            settled(&mut state);
            assert_eq!(zone(&state, victim), Zone::Graveyard);
            assert_eq!(zone(&state, leveler), Zone::Battlefield);
            let owner = if target_own { actor } else { opponent };
            let tokens = powerstones(&state, owner);
            assert_eq!(tokens.len(), 1);
            assert!(tapped(&state, tokens[0]), "created tapped");
            assert!(powerstones(&state, owner.opponent()).is_empty());
        }

        // No target: nothing is destroyed and no token is created.
        let mut state = game(actor);
        let leveler = put(&mut state, actor, LEVELER, Zone::Hand);
        let ring = put(&mut state, opponent, "Sol Ring", Zone::Battlefield);
        pool(&mut state, actor, &[], 8);
        cast(&mut state, leveler);
        trigger_target(&mut state, None);
        settled(&mut state);
        assert_eq!(zone(&state, ring), Zone::Battlefield);
        assert!(powerstones(&state, P0).is_empty() && powerstones(&state, P1).is_empty());
    }
}

/// An indestructible target is not destroyed, but its controller still
/// creates the token.
#[test]
fn leveler_gives_the_token_for_an_indestructible_target_in_both_seats() {
    for actor in [P0, P1] {
        let opponent = actor.opponent();
        let mut state = game(actor);
        let leveler = put(&mut state, actor, LEVELER, Zone::Hand);
        let angel = put(&mut state, opponent, "Darksteel Angel", Zone::Battlefield);
        pool(&mut state, actor, &[], 8);
        cast(&mut state, leveler);
        trigger_target(&mut state, Some(angel));
        settled(&mut state);
        assert_eq!(zone(&state, angel), Zone::Battlefield);
        assert_eq!(powerstones(&state, opponent).len(), 1);
    }
}

/// Attacks with `attacker` at the next declaration of attackers by its
/// controller.
fn attack_with(state: &mut GameState, attacker: ObjectId) {
    let controller = state.objects.get(attacker).controller;
    for _ in 0..200 {
        match next(state) {
            Decision::DeclareAttackers { player, eligible } if player == controller => {
                assert!(eligible.contains(&attacker), "may attack");
                act(state, Action::DeclareAttackers(vec![attacker]));
                return;
            }
            Decision::CastSpellOrPass { .. } => act(state, Action::Pass),
            Decision::DeclareAttackers { .. } => act(state, Action::DeclareAttackers(vec![])),
            other => panic!("unexpected decision: {other:?}"),
        }
    }
    panic!("no attack declaration");
}

/// "Whenever this creature attacks, destroy up to one target nonland
/// permanent." The attacking Leveler may name itself.
#[test]
fn leveler_attack_trigger_destroys_and_may_name_itself_in_both_seats() {
    for actor in [P0, P1] {
        let opponent = actor.opponent();
        let mut state = game(actor);
        let leveler = put(&mut state, actor, LEVELER, Zone::Battlefield);
        let ring = put(&mut state, opponent, "Sol Ring", Zone::Battlefield);
        attack_with(&mut state, leveler);
        let offered = trigger_target(&mut state, Some(ring));
        assert!(offered.contains(&Target::Object(leveler)), "itself");
        run_until(&mut state, |s| s.step == Step::Main2, |_, _| None);
        assert_eq!(zone(&state, ring), Zone::Graveyard);
        assert_eq!(powerstones(&state, opponent).len(), 1);
        assert_eq!(state.players[opponent.index()].life, 12, "8 trample");

        // Naming itself: the Leveler is destroyed and its controller gets
        // the token.
        let mut state = game(actor);
        let leveler = put(&mut state, actor, LEVELER, Zone::Battlefield);
        attack_with(&mut state, leveler);
        trigger_target(&mut state, Some(leveler));
        run_until(&mut state, |s| s.step == Step::Main2, |_, _| None);
        assert_eq!(zone(&state, leveler), Zone::Graveyard);
        assert_eq!(powerstones(&state, actor).len(), 1);
    }
}

/// "Unearth {8}": from the graveyard at sorcery speed; it returns with haste
/// (and attacks that turn, with its attack trigger) without its cast
/// trigger, and is exiled at the beginning of the next end step.
#[test]
fn leveler_unearths_with_haste_without_a_cast_trigger_and_is_exiled_in_both_seats() {
    for actor in [P0, P1] {
        let opponent = actor.opponent();
        let mut state = game(actor);
        let leveler = put(&mut state, actor, LEVELER, Zone::Graveyard);
        let ring = put(&mut state, opponent, "Sol Ring", Zone::Battlefield);
        pool(&mut state, actor, &[], 8);
        activate(&mut state, leveler, 0);
        settled(&mut state);
        assert_eq!(zone(&state, leveler), Zone::Battlefield);
        assert!(engine::has_effective_keyword(
            &state,
            leveler,
            Keywords::HASTE
        ));
        assert!(powerstones(&state, opponent).is_empty());

        attack_with(&mut state, leveler);
        trigger_target(&mut state, Some(ring));
        run_until(
            &mut state,
            |s| s.active_player == opponent && s.step == Step::Main1,
            |_, _| None,
        );
        assert_eq!(zone(&state, ring), Zone::Graveyard);
        assert_eq!(zone(&state, leveler), Zone::Exile, "unearth");
    }
}

// ---- Powerstone token ---------------------------------------------------------------

/// "{T}: Add {C}. This mana can't be spent to cast a nonartifact spell." The
/// automatic planner never taps it; tapped by hand, its mana pays an artifact
/// spell and an activated ability but not a nonartifact spell.
#[test]
fn powerstone_mana_pays_artifact_spells_and_abilities_only() {
    let mut state = game(P0);
    let first = put(&mut state, P0, POWERSTONE, Zone::Battlefield);
    let second = put(&mut state, P0, POWERSTONE, Zone::Battlefield);
    let stone = put(&mut state, P0, "Mind Stone", Zone::Hand);
    assert!(!castable(&mut state, stone), "not an automatic source");
    act(&mut state, Action::ActivateManaAbility(first));
    act(&mut state, Action::ActivateManaAbility(second));
    assert!(tapped(&state, first) && tapped(&state, second));
    assert_eq!(restricted(&state, P0), 2);
    assert_eq!(state.players[0].mana_pool[ManaColor::C.pool_index()], 0);
    cast(&mut state, stone);
    settled(&mut state);
    assert_eq!(zone(&state, stone), Zone::Battlefield);
    assert_eq!(restricted(&state, P0), 0);

    // A nonartifact spell's generic mana cannot come from it.
    let mut state = game(P0);
    let token = put(&mut state, P0, POWERSTONE, Zone::Battlefield);
    let specter = put(&mut state, P0, "Hypnotic Specter", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B, ManaColor::B], 0);
    next(&mut state);
    act(&mut state, Action::ActivateManaAbility(token));
    assert_eq!(restricted(&state, P0), 1);
    assert!(!castable(&mut state, specter));

    // It pays an activated ability.
    let mut state = game(P0);
    let first = put(&mut state, P0, POWERSTONE, Zone::Battlefield);
    let second = put(&mut state, P0, POWERSTONE, Zone::Battlefield);
    let foundry = put(&mut state, P0, "Retrofitter Foundry", Zone::Battlefield);
    assert!(!activatable(&mut state, foundry, 1), "no mana yet");
    act(&mut state, Action::ActivateManaAbility(first));
    act(&mut state, Action::ActivateManaAbility(second));
    assert!(activatable(&mut state, foundry, 1));
    activate(&mut state, foundry, 1);
    settled(&mut state);
    assert_eq!(restricted(&state, P0), 0);
}

// ---- Invasion Submersible -----------------------------------------------------------

/// "When this Vehicle enters, return up to one other target nonland permanent
/// to its owner's hand." Not itself, not a land; any player's permanent; none
/// is legal.
#[test]
fn submersible_enters_and_bounces_another_nonland_permanent_in_both_seats() {
    for actor in [P0, P1] {
        let opponent = actor.opponent();
        let mut state = game(actor);
        let submersible = put(&mut state, actor, SUBMERSIBLE, Zone::Hand);
        let theirs = put(&mut state, opponent, "Sol Ring", Zone::Battlefield);
        let mine = put(&mut state, actor, "Grim Monolith", Zone::Battlefield);
        let island = put(&mut state, opponent, "Island", Zone::Battlefield);
        pool(&mut state, actor, &[ManaColor::U], 2);
        cast(&mut state, submersible);
        let offered = trigger_target(&mut state, Some(theirs));
        assert_eq!(objects(&offered), [theirs, mine].into_iter().collect());
        assert!(!offered.contains(&Target::Object(island)));
        settled(&mut state);
        assert_eq!(zone(&state, submersible), Zone::Battlefield);
        assert_eq!(zone(&state, theirs), Zone::Hand);
        assert!(state.players[opponent.index()].hand.contains(&theirs));
        assert!(!creature(&state, submersible), "a Vehicle, not a creature");

        let mut state = game(actor);
        let submersible = put(&mut state, actor, SUBMERSIBLE, Zone::Hand);
        let ring = put(&mut state, opponent, "Sol Ring", Zone::Battlefield);
        pool(&mut state, actor, &[ManaColor::U], 2);
        cast(&mut state, submersible);
        trigger_target(&mut state, None);
        settled(&mut state);
        assert_eq!(zone(&state, ring), Zone::Battlefield);
    }
}

/// Answers the waterbend tap prompts with `picks`, in order; a prompt the
/// engine fills by itself (only as many candidates as taps) is skipped.
/// Returns the candidates each prompt offered.
fn waterbend_taps(state: &mut GameState, picks: &[ObjectId]) -> Vec<Vec<ObjectId>> {
    let mut offered = Vec::new();
    let mut picks = picks.iter();
    loop {
        match next(state) {
            Decision::ChooseCostTargets { candidates, .. } => {
                let pick = *picks.next().expect("a waterbend pick");
                assert!(candidates.contains(&pick), "{pick:?} not in {candidates:?}");
                offered.push(candidates);
                act(state, Action::ChooseCostTarget(pick));
            }
            _ => return offered,
        }
    }
}

/// "Exhaust - Waterbend {3}: This Vehicle becomes an artifact creature. Put
/// three +1/+1 counters on it." Each payment taps zero to three untapped
/// artifacts or creatures (summoning-sick ones too, and the Vehicle itself)
/// for that much of the {3}. It is a 3/3 artifact creature from then on, and
/// no exhaust payment is offered again.
#[test]
fn submersible_exhaust_waterbends_zero_to_three_taps_once_in_both_seats() {
    for actor in [P0, P1] {
        for taps in 0..=3u8 {
            let mut state = game(actor);
            let submersible = put(&mut state, actor, SUBMERSIBLE, Zone::Battlefield);
            let walls: Vec<ObjectId> = (0..2)
                .map(|_| {
                    let wall = put(&mut state, actor, "Basking Rootwalla", Zone::Battlefield);
                    state.objects.get_mut(wall).summoning_sick = true;
                    wall
                })
                .collect();
            pool(&mut state, actor, &[], 3 - taps);
            for other in 0..=3u8 {
                assert_eq!(
                    activatable(&mut state, submersible, other),
                    other >= taps,
                    "{taps} mana short: payment {other}"
                );
            }
            activate(&mut state, submersible, taps);
            let picks: Vec<ObjectId> = walls
                .iter()
                .copied()
                .chain([submersible])
                .take(usize::from(taps))
                .collect();
            waterbend_taps(&mut state, &picks);
            settled(&mut state);
            for &pick in &picks {
                assert!(tapped(&state, pick), "tapped for waterbend");
            }
            assert_eq!(
                state.players[actor.index()].mana_pool[ManaColor::C.pool_index()],
                0
            );
            assert!(creature(&state, submersible));
            assert!(engine::object_has_type(
                &state,
                submersible,
                CardType::Artifact
            ));
            assert_eq!(state.objects.get(submersible).counters.plus1_plus1, 3);
            assert_eq!(
                (power(&state, submersible), toughness(&state, submersible)),
                (3, 3)
            );
            // Exhaust: once only, whatever the payment.
            pool(&mut state, actor, &[], 3);
            for other in 0..=3u8 {
                assert!(!activatable(&mut state, submersible, other));
            }
        }
    }
}

/// The animation has no duration: still a creature on later turns, and it
/// can attack once its controller has controlled it since the turn began.
/// A blinked Submersible is a new object: a noncreature Vehicle whose
/// exhaust ability can be activated again.
#[test]
fn submersible_stays_a_creature_and_a_blink_resets_exhaust_in_both_seats() {
    for actor in [P0, P1] {
        let opponent = actor.opponent();
        let mut state = game(actor);
        let submersible = put(&mut state, actor, SUBMERSIBLE, Zone::Battlefield);
        pool(&mut state, actor, &[], 3);
        activate(&mut state, submersible, 0);
        settled(&mut state);
        assert!(creature(&state, submersible));
        run_until(
            &mut state,
            |s| s.active_player == opponent && s.step == Step::Main1,
            |_, _| None,
        );
        run_until(
            &mut state,
            |s| s.active_player == actor && s.step == Step::Main1,
            |_, _| None,
        );
        assert!(creature(&state, submersible), "no duration");
        assert_eq!(power(&state, submersible), 3);
        attack_with(&mut state, submersible);
        run_until(&mut state, |s| s.step == Step::Main2, |_, _| None);
        assert_eq!(state.players[opponent.index()].life, 17);

        blink(&mut state, submersible);
        // The enters trigger of the returned object asks for a target.
        trigger_target(&mut state, None);
        settled(&mut state);
        assert!(!creature(&state, submersible), "a new object");
        assert_eq!(state.objects.get(submersible).counters.plus1_plus1, 0);
        pool(&mut state, actor, &[], 3);
        assert!(activatable(&mut state, submersible, 0));
        activate(&mut state, submersible, 0);
        settled(&mut state);
        assert!(creature(&state, submersible));
    }
}

/// A waterbend tap that would leave the rest of the payment unpayable is not
/// offered: Llanowar Elves are needed for mana when the payment taps one
/// permanent and leaves {2}, so only the Vehicle and the Rootwalla are
/// offered. A payment no choice of taps can complete is not offered at all.
#[test]
fn submersible_waterbend_never_strands_its_mana_part_in_both_seats() {
    for actor in [P0, P1] {
        let mut state = game(actor);
        let submersible = put(&mut state, actor, SUBMERSIBLE, Zone::Battlefield);
        let elves = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
        let wall = put(&mut state, actor, "Basking Rootwalla", Zone::Battlefield);
        put(&mut state, actor, "Forest", Zone::Battlefield);
        // {3} with two mana sources: no. {2} and one tap: yes. {1} and two
        // taps: yes. Three taps: the Vehicle, the Elves and the Rootwalla.
        assert!(!activatable(&mut state, submersible, 0));
        assert!(activatable(&mut state, submersible, 1));
        assert!(activatable(&mut state, submersible, 2));
        assert!(activatable(&mut state, submersible, 3));
        activate(&mut state, submersible, 1);
        let offered = waterbend_taps(&mut state, &[wall]);
        assert_eq!(offered.len(), 1);
        assert_eq!(
            offered[0].iter().copied().collect::<BTreeSet<_>>(),
            [submersible, wall].into_iter().collect()
        );
        assert!(!offered[0].contains(&elves));
        settled(&mut state);
        assert!(creature(&state, submersible));
        assert!(tapped(&state, wall) && tapped(&state, elves));
    }
}

/// Forensic Gadgeteer's reduction applies to the waterbend {3}: it costs
/// {2}, so at most two permanents can be tapped for it, and two taps pay it
/// all.
#[test]
fn submersible_waterbend_follows_a_cost_reduction_in_both_seats() {
    for actor in [P0, P1] {
        let mut state = game(actor);
        let submersible = put(&mut state, actor, SUBMERSIBLE, Zone::Battlefield);
        let gadgeteer = put(&mut state, actor, "Forensic Gadgeteer", Zone::Battlefield);
        let wall = put(&mut state, actor, "Basking Rootwalla", Zone::Battlefield);
        assert!(!activatable(&mut state, submersible, 0), "{{2}} unpaid");
        assert!(!activatable(&mut state, submersible, 1), "{{1}} unpaid");
        assert!(activatable(&mut state, submersible, 2), "two taps pay {{2}}");
        assert!(
            !activatable(&mut state, submersible, 3),
            "no more taps than the reduced cost"
        );
        activate(&mut state, submersible, 2);
        waterbend_taps(&mut state, &[gadgeteer, wall]);
        settled(&mut state);
        assert!(creature(&state, submersible));
        assert!(!tapped(&state, submersible));
        assert!(tapped(&state, gadgeteer) && tapped(&state, wall));

        let mut state = game(actor);
        let submersible = put(&mut state, actor, SUBMERSIBLE, Zone::Battlefield);
        put(&mut state, actor, "Forensic Gadgeteer", Zone::Battlefield);
        pool(&mut state, actor, &[], 2);
        activate(&mut state, submersible, 0);
        settled(&mut state);
        assert!(creature(&state, submersible));
        assert_eq!(
            state.players[actor.index()].mana_pool[ManaColor::C.pool_index()],
            0
        );
    }
}

// ---- Summon: Bahamut ----------------------------------------------------------------

/// Bahamut enters with a lore counter (chapter I) and gets one after its
/// controller's draw step: I and II destroy up to one target nonland
/// permanent (itself included), III draws two cards and IV deals damage equal
/// to the total mana value of the other permanents its controller controls
/// to the opponent; then it is sacrificed.
#[test]
fn bahamut_chapters_destroy_draw_and_mega_flare_then_it_is_sacrificed_in_both_seats() {
    for actor in [P0, P1] {
        let opponent = actor.opponent();
        let mut state = game(actor);
        let bahamut = put(&mut state, actor, BAHAMUT, Zone::Hand);
        let ring = put(&mut state, opponent, "Sol Ring", Zone::Battlefield);
        let pup = put(&mut state, opponent, "Jackal Pup", Zone::Battlefield);
        put(&mut state, actor, "Sol Ring", Zone::Battlefield);
        put(&mut state, actor, "Grim Monolith", Zone::Battlefield);
        put(&mut state, actor, "Island", Zone::Battlefield);
        pool(&mut state, actor, &[], 9);
        cast(&mut state, bahamut);
        let offered = trigger_target(&mut state, Some(ring));
        assert!(offered.contains(&Target::Object(bahamut)), "itself");
        settled(&mut state);
        assert_eq!(state.objects.get(bahamut).counters.lore, 1);
        assert_eq!(zone(&state, ring), Zone::Graveyard);
        assert!(creature(&state, bahamut));
        assert!(engine::has_effective_keyword(
            &state,
            bahamut,
            Keywords::FLYING
        ));

        // Chapter II on the next turn of its controller.
        let mut chapter_two = false;
        run_until(
            &mut state,
            |s| s.active_player == actor && s.step == Step::Main1 && chapter_two_done(s, pup),
            |state, decision| match decision {
                Decision::ChooseTargets { legal_targets, .. }
                    if legal_targets.contains(&Target::Object(pup)) =>
                {
                    assert_eq!(state.objects.get(bahamut).counters.lore, 2);
                    chapter_two = true;
                    Some(Action::ChooseTarget(Target::Object(pup)))
                }
                _ => None,
            },
        );
        assert!(chapter_two);
        assert_eq!(zone(&state, pup), Zone::Graveyard);

        // Chapter III draws two cards.
        let hand = |state: &GameState| state.players[actor.index()].hand.len();
        run_until(
            &mut state,
            |s| s.active_player == opponent && s.step == Step::Main1,
            |_, _| None,
        );
        let before = hand(&state);
        run_until(
            &mut state,
            |s| s.active_player == actor && s.step == Step::Main1 && s.stack.is_empty(),
            |_, _| None,
        );
        assert_eq!(state.objects.get(bahamut).counters.lore, 3);
        assert_eq!(hand(&state), before + 1 + 2, "a draw step and chapter III");
        assert_eq!(zone(&state, bahamut), Zone::Battlefield);

        // Chapter IV: Sol Ring (1) and Grim Monolith (2); the Island and the
        // Saga itself count nothing. Then the Saga is sacrificed.
        run_until(
            &mut state,
            |s| s.active_player == opponent && s.step == Step::Main1,
            |_, _| None,
        );
        let life = state.players[opponent.index()].life;
        run_until(
            &mut state,
            |s| s.active_player == actor && s.step == Step::Main1 && s.stack.is_empty(),
            |_, _| None,
        );
        assert_eq!(state.players[opponent.index()].life, life - 3);
        assert_eq!(zone(&state, bahamut), Zone::Graveyard, "sacrificed after IV");
    }
}

fn chapter_two_done(state: &GameState, pup: ObjectId) -> bool {
    state.objects.get(pup).zone == Zone::Graveyard && state.stack.is_empty()
}

/// Mega Flare reads the mana value of the other permanents as it resolves; a
/// chapter may target nothing. A blinked Bahamut is a new object and starts
/// again at chapter I.
#[test]
fn bahamut_blink_restarts_at_chapter_one_and_chapters_may_target_nothing_in_both_seats() {
    for actor in [P0, P1] {
        let opponent = actor.opponent();
        let mut state = game(actor);
        let bahamut = put(&mut state, actor, BAHAMUT, Zone::Hand);
        let ring = put(&mut state, opponent, "Sol Ring", Zone::Battlefield);
        pool(&mut state, actor, &[], 9);
        cast(&mut state, bahamut);
        trigger_target(&mut state, None);
        settled(&mut state);
        assert_eq!(zone(&state, ring), Zone::Battlefield);
        assert_eq!(state.objects.get(bahamut).counters.lore, 1);

        blink(&mut state, bahamut);
        assert_eq!(state.objects.get(bahamut).counters.lore, 1, "a new Saga");
        trigger_target(&mut state, Some(ring));
        settled(&mut state);
        assert_eq!(zone(&state, ring), Zone::Graveyard);
    }
}

// ---- S1q Public-Session Consumer Games ----------------------------------------------

#[derive(Default)]
struct S1qSessionEvents {
    exhausts: usize,
}

impl S1qSessionEvents {
    fn observe(&mut self, _decision: &Value, action: &Value) {
        let semantic = &action["semantic"];
        let card = semantic["source"]["card_db_id"]
            .as_u64()
            .map(|id| CARD_DEFS[id as usize].name);
        match semantic["action_kind"].as_str() {
            Some("activate_ability") if card == Some(SUBMERSIBLE) => self.exhausts += 1,
            _ => {}
        }
    }
}

#[test]
fn s1q_decks_finish_and_replay_through_public_sessions() {
    let tron = deck(&[
        ("Urza's Tower", 4),
        ("Urza's Mine", 4),
        ("Urza's Power Plant", 4),
        ("Island", 10),
        ("Grim Monolith", 4),
        ("Sol Ring", 4),
        (LEVELER, 6),
        (SUBMERSIBLE, 8),
        (BAHAMUT, 6),
    ]);
    let artifacts = deck(&[
        ("Island", 20),
        ("Sol Ring", 4),
        ("Mind Stone", 4),
        ("Grim Monolith", 4),
        (SUBMERSIBLE, 8),
        (LEVELER, 4),
    ]);

    let mut totals = S1qSessionEvents::default();
    let tracked = [LEVELER, SUBMERSIBLE, BAHAMUT];
    let mut cast_totals = [0usize; 3];

    for (pair, first_seed) in [
        ([tron.clone(), artifacts.clone()], 0x5331_7100_u64),
        ([artifacts, tron], 0x5331_7110),
    ] {
        for seed in first_seed..first_seed + 3 {
            for policy in [
                Policy::Ranked,
                Policy::Random(seed + 0x100),
                Policy::Random(seed + 0x200),
            ] {
                let mut events = S1qSessionEvents::default();
                let game =
                    play_observed(pair.clone(), None, seed, policy, &mut |decision, action| {
                        events.observe(decision, action)
                    });
                let replay = play_observed(pair.clone(), None, seed, policy, &mut |_, _| {});
                assert_eq!(game.transcript_sha256, replay.transcript_sha256);
                for (slot, name) in cast_totals.iter_mut().zip(tracked) {
                    *slot += game
                        .cast_cards
                        .iter()
                        .map(|cards| cards.get(&id(name)).copied().unwrap_or(0))
                        .sum::<usize>();
                }
                totals.exhausts += events.exhausts;
            }
        }
    }

    assert!(cast_totals[0] > 0, "no Cityscape Leveler cast");
    assert!(cast_totals[1] > 0, "no Invasion Submersible cast");
    assert!(cast_totals[2] > 0, "no Summon: Bahamut cast");
    assert!(totals.exhausts > 0, "no exhaust activation");
}

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
