//! Vintage sideboard wave S1j: native rules tests for Archon of Absolution,
//! which completes Mono-White Initiative.
#![cfg(feature = "eternal-fixtures")]

use mtg_kernel::card_def::{
    card_id_by_name, CardCapability, CardDef, CardType, Keywords, Subtype, CARD_DEFS,
};
use mtg_kernel::engine::{self, Action, Decision};
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::ManaColor;
use mtg_kernel::policy_surface_v5::{PolicyActionV5, PolicyDecisionV5, PolicySurfaceV5};
use mtg_kernel::rl::policy_legal_action_candidates_v5;
use mtg_kernel::state::{GameObject, GameState, GoadStateV4, ObjectStateV4, Step, Target, Zone};

const P0: PlayerId = PlayerId::P0;
const P1: PlayerId = PlayerId::P1;

const ARCHON: &str = "Archon of Absolution";
const ANGLER: &str = "Gurmag Angler";
const ELVES: &str = "Llanowar Elves";
/// A vigilant creature with "{T}: Add {G}{G}".
const ADVENTURER: &str = "Undermountain Adventurer";

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
        0x5331_4a00_0001,
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

/// A permanent that entered from `player`'s hand, so it has a timestamp.
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
    panic!("the stack never settled");
}

fn settled(state: &mut GameState) {
    if let Some(other) = settle(state) {
        panic!("unexpected choice: {other:?}");
    }
}

fn restored(state: &GameState) -> GameState {
    serde_json::from_slice(&serde_json::to_vec(state).unwrap()).unwrap()
}

/// Passes priority until `player`'s declaration of attackers, declaring no
/// attackers or blockers on the way, and returns the creatures they may
/// attack with.
fn attackers_of(state: &mut GameState, player: PlayerId) -> Vec<ObjectId> {
    for _ in 0..400 {
        let decision = next(state);
        match decision {
            Decision::DeclareAttackers {
                player: declaring,
                eligible,
            } if declaring == player => return eligible,
            Decision::CastSpellOrPass { .. } => act(state, Action::Pass),
            Decision::DeclareAttackers { .. } => act(state, Action::DeclareAttackers(Vec::new())),
            Decision::DeclareBlockers { .. } => act(state, Action::DeclareBlockers(Vec::new())),
            Decision::OrderTriggers { pending, .. } => {
                act(state, Action::OrderTriggers((0..pending.len()).collect()))
            }
            other => panic!("unexpected choice: {other:?}"),
        }
    }
    panic!("{player:?} never declared attackers");
}

/// P0's first declaration of attackers. P0 has `forests` Forests and the
/// named creatures, which can attack, and P1 has `archons` Archons of
/// Absolution.
fn declaration(forests: usize, creatures: &[&str], archons: usize) -> (GameState, Vec<ObjectId>) {
    let mut state = islands();
    for _ in 0..forests {
        put(&mut state, P0, "Forest", Zone::Battlefield);
    }
    let creatures: Vec<ObjectId> = creatures
        .iter()
        .map(|name| put(&mut state, P0, name, Zone::Battlefield))
        .collect();
    for _ in 0..archons {
        put(&mut state, P1, ARCHON, Zone::Battlefield);
    }
    assert_eq!(attackers_of(&mut state, P0), creatures);
    (state, creatures)
}

/// `attackers` declared on a copy of `state`, or the engine's refusal.
fn declare(state: &GameState, attackers: &[ObjectId]) -> Result<GameState, String> {
    let mut declared = state.clone();
    engine::step(&mut declared, Action::DeclareAttackers(attackers.to_vec()))?;
    Ok(declared)
}

/// The permanents named `name` that `player` controls, tapped.
fn tapped(state: &GameState, player: PlayerId, name: &str) -> usize {
    state.players[player.index()]
        .battlefield
        .iter()
        .filter(|&&object| {
            let live = state.objects.get(object);
            live.tapped && CARD_DEFS[live.card_def as usize].name == name
        })
        .count()
}

// ---- Characteristics -----------------------------------------------------------------

#[test]
fn archon_of_absolution_is_full_with_its_printed_characteristics() {
    let archon = def(ARCHON);
    assert_eq!(archon.capability, CardCapability::Full);
    assert!(!archon.is_token);
    assert_eq!(archon.types, &[CardType::Creature][..]);
    assert_eq!((archon.mana_value, archon.colors), (4, &[ManaColor::W][..]));
    assert_eq!((archon.power, archon.toughness), (Some(3), Some(2)));
    assert_eq!(archon.subtypes, &[Subtype::Archon][..]);
    assert!(archon.keywords.has(Keywords::FLYING));
}

#[test]
fn archon_of_absolution_has_protection_from_white() {
    let mut state = islands();
    let archon = put(&mut state, P1, ARCHON, Zone::Battlefield);
    let inspector = put(&mut state, P1, "Thraben Inspector", Zone::Battlefield);
    // Swords to Plowshares is white: the Archon isn't a legal target.
    let swords = put(&mut state, P0, "Swords to Plowshares", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W], 0);
    act(&mut state, Action::CastSpell(swords));
    match next(&mut state) {
        Decision::ChooseTargets { legal_targets, .. } => {
            assert!(legal_targets.contains(&Target::Object(inspector)));
            assert!(!legal_targets.contains(&Target::Object(archon)));
        }
        other => panic!("expected targets, got {other:?}"),
    }
    // Lightning Bolt is red: it can target the Archon.
    let mut state = islands();
    let archon = put(&mut state, P1, ARCHON, Zone::Battlefield);
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 0);
    act(&mut state, Action::CastSpell(bolt));
    match next(&mut state) {
        Decision::ChooseTargets { legal_targets, .. } => {
            assert!(legal_targets.contains(&Target::Object(archon)));
        }
        other => panic!("expected targets, got {other:?}"),
    }
}

// ---- The cost to attack --------------------------------------------------------------

#[test]
fn each_attacker_costs_one_for_each_archon_its_defender_controls() {
    // {1} for each attacker: two Forests pay for two of three.
    let (state, angler) = declaration(2, &[ANGLER, ANGLER, ANGLER], 1);
    assert!(declare(&state, &angler).is_err());
    let paid = declare(&state, &angler[..2]).unwrap();
    assert_eq!(paid.engine.combat.attackers, angler[..2].to_vec());
    assert_eq!(tapped(&paid, P0, "Forest"), 2);
    let copy = restored(&paid);
    assert_eq!(paid.state_hash(), copy.state_hash());
    let paid = declare(&state, &angler[2..]).unwrap();
    assert_eq!(tapped(&paid, P0, "Forest"), 1);
    // Two Archons: {2} for each attacker.
    let (state, angler) = declaration(3, &[ANGLER, ANGLER], 2);
    assert!(declare(&state, &angler).is_err());
    let paid = declare(&state, &angler[..1]).unwrap();
    assert_eq!(tapped(&paid, P0, "Forest"), 2);
    // No attackers cost nothing.
    let none = declare(&state, &[]).unwrap();
    assert!(none.engine.combat.attackers.is_empty());
    assert_eq!(tapped(&none, P0, "Forest"), 0);
}

#[test]
fn attacking_taps_a_creature_before_the_cost_is_paid() {
    // An attacking Llanowar Elves is tapped by its attack, so it can't pay
    // its own {1}.
    let (state, elves) = declaration(0, &[ELVES], 1);
    assert!(declare(&state, &elves).is_err());
    // A Forest can, and the Elves stay tapped from attacking.
    let (state, elves) = declaration(1, &[ELVES], 1);
    let paid = declare(&state, &elves).unwrap();
    assert_eq!(tapped(&paid, P0, "Forest"), 1);
    assert!(paid.objects.get(elves[0]).tapped);
    // Elves that stay home pay for another attacker.
    let (state, creatures) = declaration(0, &[ELVES, ANGLER], 1);
    assert!(declare(&state, &creatures).is_err());
    let paid = declare(&state, &creatures[1..]).unwrap();
    assert!(paid.objects.get(creatures[0]).tapped, "the Elves paid");
    // A vigilant attacker isn't tapped by attacking, so it can pay.
    let (state, adventurer) = declaration(0, &[ADVENTURER], 1);
    let paid = declare(&state, &adventurer).unwrap();
    assert_eq!(paid.engine.combat.attackers, adventurer);
    assert!(paid.objects.get(adventurer[0]).tapped, "tapped for mana");
}

#[test]
fn only_an_archon_the_defender_controls_with_its_abilities_taxes_attacks() {
    // The active player's own Archon costs them nothing.
    let mut state = islands();
    let angler = put(&mut state, P0, ANGLER, Zone::Battlefield);
    let own = put(&mut state, P0, ARCHON, Zone::Battlefield);
    assert_eq!(attackers_of(&mut state, P0), vec![angler, own]);
    assert!(declare(&state, &[angler, own]).is_ok());

    // An Archon that left the battlefield costs nothing.
    let (mut state, angler) = declaration(0, &[ANGLER], 1);
    assert!(declare(&state, &angler).is_err());
    let archon = state.players[1].battlefield[0];
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(archon, Zone::Graveyard),
    );
    assert!(declare(&state, &angler).is_ok());

    // Dress Down: creatures lose all abilities, the Archon's cost too.
    let mut state = islands();
    let angler = put(&mut state, P0, ANGLER, Zone::Battlefield);
    put(&mut state, P1, ARCHON, Zone::Battlefield);
    entered(&mut state, P0, "Dress Down");
    assert_eq!(attackers_of(&mut state, P0), vec![angler]);
    assert!(declare(&state, &[angler]).is_ok());
}

#[test]
fn a_phyrexian_metamorph_copying_an_archon_taxes_attacks_too() {
    let mut state = islands();
    let archon = put(&mut state, P0, ARCHON, Zone::Battlefield);
    // P1 casts the Metamorph on its own turn, copying P0's Archon.
    state.active_player = P1;
    state.priority_player = P1;
    let metamorph = put(&mut state, P1, "Phyrexian Metamorph", Zone::Hand);
    pool(&mut state, P1, &[ManaColor::U], 3);
    act(&mut state, Action::CastSpell(metamorph));
    match settle(&mut state) {
        Some(Decision::ChooseEffectTargets { legal_targets, .. })
            if legal_targets.contains(&Target::Object(archon)) => {}
        other => panic!("expected the copy choice, got {other:?}"),
    }
    act(
        &mut state,
        Action::ChooseEffectTarget(Target::Object(archon)),
    );
    settled(&mut state);
    assert_eq!(state.objects.get(metamorph).card_def, id(ARCHON));
    // P0's attack next turn costs {1}.
    put(&mut state, P0, "Forest", Zone::Battlefield);
    let attackers = attackers_of(&mut state, P0);
    assert_eq!(attackers, vec![archon]);
    let paid = declare(&state, &attackers).unwrap();
    assert_eq!(tapped(&paid, P0, "Forest"), 1);
}

#[test]
fn no_creature_is_required_to_attack_while_attacks_have_a_cost() {
    // A goaded creature must attack if able...
    let goaded = |state: &mut GameState, creature: ObjectId| {
        let expires_at_turn = state.turn + 2;
        state.objects.get_mut(creature).v4.goaded_by = vec![GoadStateV4 {
            player: P1,
            expires_at_turn,
        }];
    };
    let mut state = islands();
    let angler = put(&mut state, P0, ANGLER, Zone::Battlefield);
    goaded(&mut state, angler);
    assert_eq!(attackers_of(&mut state, P0), vec![angler]);
    assert!(declare(&state, &[]).is_err(), "goad requires the attack");
    // ...unless attacking has a cost: then it may stay home, and with no
    // mana to pay it does.
    let mut state = islands();
    let angler = put(&mut state, P0, ANGLER, Zone::Battlefield);
    put(&mut state, P1, ARCHON, Zone::Battlefield);
    goaded(&mut state, angler);
    assert_eq!(attackers_of(&mut state, P0), vec![angler]);
    assert!(declare(&state, &[angler]).is_err());
    assert!(declare(&state, &[]).is_ok());
    put(&mut state, P0, "Forest", Zone::Battlefield);
    assert!(declare(&state, &[]).is_ok());
    assert!(declare(&state, &[angler]).is_ok());
}

// ---- Consumers -------------------------------------------------------------------------

/// Every subset of `creatures`, by bit mask.
fn subsets(creatures: &[ObjectId]) -> impl Iterator<Item = Vec<ObjectId>> + '_ {
    (0..1usize << creatures.len()).map(move |mask| {
        creatures
            .iter()
            .enumerate()
            .filter(|(index, _)| mask & (1 << index) != 0)
            .map(|(_, &creature)| creature)
            .collect()
    })
}

/// Walks every path of the policy surface's attacker scan from `state`,
/// checking that each step offers exactly the answers that keep a
/// declaration the engine accepts, and that each path's last answer commits
/// the declaration it chose. Collects the declarations committed.
fn walk_scan(
    state: &GameState,
    surface: &PolicySurfaceV5,
    legal: &[Vec<ObjectId>],
    chosen: Vec<ObjectId>,
    candidates: &[ObjectId],
    committed: &mut Vec<Vec<ObjectId>>,
) {
    if state.engine.combat.attackers_declared {
        assert_eq!(state.engine.combat.attackers, chosen);
        committed.push(chosen);
        return;
    }
    let mut state = state.clone();
    let mut surface = surface.clone();
    let decision = surface.next_decision(&mut state).unwrap();
    let PolicyDecisionV5::AttackerInclusion { attacker, .. } = decision else {
        panic!("the scan ended without a declaration: {decision:?}");
    };
    let cursor = candidates
        .iter()
        .position(|&candidate| candidate == attacker)
        .unwrap();
    let offered: Vec<bool> = policy_legal_action_candidates_v5(&decision, &surface, &state)
        .unwrap()
        .into_iter()
        .map(|candidate| match candidate.policy_action {
            PolicyActionV5::ChooseAttackerInclusion { include, .. } => include,
            other => panic!("unexpected scan action {other:?}"),
        })
        .collect();
    let decided = &candidates[..cursor];
    let expected: Vec<bool> = [false, true]
        .into_iter()
        .filter(|&include| {
            legal.iter().any(|declaration| {
                decided
                    .iter()
                    .all(|creature| declaration.contains(creature) == chosen.contains(creature))
                    && declaration.contains(&attacker) == include
            })
        })
        .collect();
    assert_eq!(offered, expected, "chosen {chosen:?}, at {attacker:?}");
    for include in offered {
        let mut next_state = state.clone();
        let mut next_surface = surface.clone();
        next_surface
            .apply(
                &mut next_state,
                PolicyActionV5::ChooseAttackerInclusion {
                    actor: P0,
                    attacker,
                    include,
                },
            )
            .unwrap();
        let mut next_chosen = chosen.clone();
        if include {
            next_chosen.push(attacker);
        }
        walk_scan(
            &next_state,
            &next_surface,
            legal,
            next_chosen,
            candidates,
            committed,
        );
    }
}

#[test]
fn the_attacker_scan_offers_exactly_the_answers_that_keep_a_payable_declaration() {
    // One Forest, the Elves and two Anglers against one Archon: the
    // payable declarations are the empty one, the Elves alone (the Forest
    // pays), either Angler alone, and both Anglers (the Forest and the
    // Elves, which stay home, pay). A declaration with the Elves and an
    // Angler costs {2} with only the Forest untapped.
    let (state, creatures) = declaration(1, &[ELVES, ANGLER, ANGLER], 1);
    let legal: Vec<Vec<ObjectId>> = subsets(&creatures)
        .filter(|declaration| declare(&state, declaration).is_ok())
        .collect();
    let [elves, first, second] = creatures[..] else {
        unreachable!()
    };
    let mut expected = vec![
        vec![],
        vec![elves],
        vec![first],
        vec![second],
        vec![first, second],
    ];
    expected.sort();
    let mut sorted = legal.clone();
    sorted.sort();
    assert_eq!(sorted, expected);
    let mut committed = Vec::new();
    walk_scan(
        &state,
        &PolicySurfaceV5::new(),
        &legal,
        Vec::new(),
        &creatures,
        &mut committed,
    );
    committed.sort();
    assert_eq!(committed, expected, "every payable declaration, once");
}
