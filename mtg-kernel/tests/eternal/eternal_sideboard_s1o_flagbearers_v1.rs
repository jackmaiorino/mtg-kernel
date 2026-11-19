//! Sideboard wave S1o: native rules tests for the Flagbearer obligation of
//! Standard Bearer and Coalition Honor Guard across target shapes: slots with
//! different rosters, slots that depend on an earlier pick, wide optional
//! rosters, per-card cast filters, and the rulings on triggered abilities,
//! copies, changed targets, modes and either player's Flagbearer.
#![cfg(feature = "eternal-fixtures")]

use crate::eternal_catalog_v1::{in_parallel, play_observed, Policy};
use mtg_kernel::card_def::{card_id_by_name, CARD_DEFS};
use mtg_kernel::engine::{self, Action, Decision, OptionalCostChoice};
use mtg_kernel::event::{propose_and_commit, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::limited_session_v1::{CustomCardCountV1, CustomDeckV1};
use mtg_kernel::mana::ManaColor;
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Target, Zone};
use serde_json::Value;

const P0: PlayerId = PlayerId::P0;
const P1: PlayerId = PlayerId::P1;

const BEARER: &str = "Standard Bearer";
const GUARD: &str = "Coalition Honor Guard";

/// Collective Brutality's modes, as wave C1b numbers them.
const BRUTALITY_REVEAL: u8 = 0;
const BRUTALITY_DRAIN: u8 = 3;
const BRUTALITY_SHRINK_DRAIN: u8 = 5;
const BRUTALITY_ALL: u8 = 6;

fn id(name: &str) -> u16 {
    card_id_by_name(name).unwrap_or_else(|| panic!("{name}"))
}

/// A first main phase with `actor` active and holding priority, both
/// libraries twenty `land`s.
fn game(actor: PlayerId, land: &str) -> GameState {
    let ids = vec![id(land); 20];
    let mut state = GameState::new_from_libraries(
        &ids,
        &ids,
        |id| CARD_DEFS[id as usize].object_name.into(),
        0x5331_4f00_0001,
    );
    state.active_player = actor;
    state.priority_player = actor;
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

fn next(state: &mut GameState) -> Decision {
    let decision = engine::advance_until_decision(state);
    assert!(
        !matches!(decision, Decision::Halted { .. }),
        "{decision:?}\nstack: {:#?}",
        state.stack
    );
    decision
}

fn act(state: &mut GameState, action: Action) {
    engine::step(state, action.clone()).unwrap_or_else(|error| panic!("{action:?}: {error}"));
}

/// The first legal answer to any other choice.
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
        Decision::ChooseOptionalCost { .. } => {
            Action::ChooseOptionalCost(OptionalCostChoice::Decline)
        }
        Decision::ChooseSpellCopyRetarget { .. } => Action::ChooseSpellCopyRetarget(true),
        Decision::Discard { count, choices, .. } => {
            Action::Discard(choices.into_iter().take(count as usize).collect())
        }
        other => panic!("unexpected choice: {other:?}"),
    }
}

/// Passes priority and orders triggers until the stack and pending triggers
/// are empty, answering every other choice with `answer`.
fn settle_with(state: &mut GameState, mut answer: impl FnMut(&GameState, Decision) -> Action) {
    for _ in 0..200 {
        let action = match next(state) {
            Decision::OrderTriggers { pending, .. } => {
                Action::OrderTriggers((0..pending.len()).collect())
            }
            Decision::CastSpellOrPass { .. }
                if state.stack.is_empty() && state.engine.pending_triggers.is_empty() =>
            {
                return
            }
            Decision::CastSpellOrPass { .. } => Action::Pass,
            other => answer(state, other),
        };
        act(state, action);
    }
    panic!("did not settle");
}

fn settled(state: &mut GameState) {
    settle_with(state, |_, decision| first_answer(decision));
}

fn castable(state: &mut GameState, spell: ObjectId) -> bool {
    match next(state) {
        Decision::CastSpellOrPass {
            castable_spells, ..
        } => castable_spells.contains(&spell),
        other => panic!("expected priority, got {other:?}"),
    }
}

/// The targets offered now, and whether the selection may finish.
fn offered(state: &mut GameState) -> (Vec<Target>, bool) {
    match next(state) {
        Decision::ChooseTargets {
            legal_targets,
            can_finish,
            ..
        }
        | Decision::ChooseEffectTargets {
            legal_targets,
            can_finish,
            ..
        } => (legal_targets, can_finish),
        other => panic!("expected targets, got {other:?}"),
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

/// Announces `spell`, choosing `mode` if the cast asks for one.
fn announce(state: &mut GameState, spell: ObjectId, mode: Option<u8>) {
    assert!(
        castable(state, spell),
        "{} not castable",
        state.objects.get(spell).name
    );
    act(state, Action::CastSpell(spell));
    if let Some(mode) = mode {
        match next(state) {
            Decision::ChooseSpellMode { legal_modes, .. } => {
                assert!(legal_modes.contains(&mode), "{mode} not in {legal_modes:?}");
                act(state, Action::ChooseSpellMode(mode));
            }
            other => panic!("expected the mode, got {other:?}"),
        }
    }
}

fn same_set(mut left: Vec<Target>, mut right: Vec<Target>) -> bool {
    left.sort_by_key(|target| format!("{target:?}"));
    right.sort_by_key(|target| format!("{target:?}"));
    left == right
}

// ---- Slots with different rosters ------------------------------------------------

#[test]
fn flagbearers_bind_collective_brutalitys_creature_slot_before_a_drain_slot_both_seats() {
    for actor in [P0, P1] {
        let opponent = actor.opponent();
        for (mode, spares) in [(BRUTALITY_SHRINK_DRAIN, 1), (BRUTALITY_ALL, 2)] {
            let mut state = game(actor, "Swamp");
            let bearer = Target::Object(put(&mut state, opponent, BEARER, Zone::Battlefield));
            let elves = put(&mut state, opponent, "Llanowar Elves", Zone::Battlefield);
            for _ in 0..spares {
                put(&mut state, actor, "Island", Zone::Hand);
            }
            let brutality = put(&mut state, actor, "Collective Brutality", Zone::Hand);
            pool(&mut state, actor, &[ManaColor::B], 1);
            announce(&mut state, brutality, Some(mode));
            if mode == BRUTALITY_ALL {
                assert_eq!(offered(&mut state).0, [Target::Player(opponent)]);
                choose_target(&mut state, Target::Player(opponent));
            }
            // The creature slot comes before the last (opponent) slot, which
            // can never name a Flagbearer.
            assert_eq!(offered(&mut state).0, [bearer], "mode {mode}");
            let hash = state.state_hash();
            assert!(engine::step(&mut state, Action::ChooseTarget(Target::Object(elves))).is_err());
            assert_eq!(state.state_hash(), hash);
            choose_target(&mut state, bearer);
            choose_target(&mut state, Target::Player(opponent));
            settled(&mut state);
            let Target::Object(bearer) = bearer else {
                unreachable!()
            };
            assert_eq!(zone(&state, bearer), Zone::Graveyard, "mode {mode}");
            assert_eq!(zone(&state, elves), Zone::Battlefield);
            assert_eq!(life(&state, opponent), 18);
            assert_eq!(zone(&state, brutality), Zone::Graveyard);
        }
    }
}

#[test]
fn flagbearers_bind_searing_blazes_player_to_a_flagbearers_controller_both_seats() {
    for actor in [P0, P1] {
        let opponent = actor.opponent();
        let mut state = game(actor, "Mountain");
        let guard = put(&mut state, opponent, GUARD, Zone::Battlefield);
        put(&mut state, opponent, "Llanowar Elves", Zone::Battlefield);
        put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
        let blaze = put(&mut state, actor, "Searing Blaze", Zone::Hand);
        pool(&mut state, actor, &[ManaColor::R, ManaColor::R], 0);
        announce(&mut state, blaze, None);
        // Choosing the caster (and the caster's creature) would avoid the
        // Honor Guard, which only its controller's creature slot can name.
        assert_eq!(offered(&mut state).0, [Target::Player(opponent)]);
        let hash = state.state_hash();
        assert!(engine::step(&mut state, Action::ChooseTarget(Target::Player(actor))).is_err());
        assert_eq!(state.state_hash(), hash);
        choose_target(&mut state, Target::Player(opponent));
        assert_eq!(offered(&mut state).0, [Target::Object(guard)]);
        choose_target(&mut state, Target::Object(guard));
        settled(&mut state);
        assert_eq!(life(&state, opponent), 19);
        assert_eq!(state.objects.get(guard).damage, 1);
        assert_eq!(zone(&state, blaze), Zone::Graveyard);
    }
}

// ---- Wide optional rosters --------------------------------------------------------

/// `actor`'s first main phase with `actor`'s Liquimetal Coating having made
/// the opponent's Standard Bearer an artifact, the opponent's Sol Ring and
/// Pest Infestation in `actor`'s hand. Returns the state, the spell, the
/// Bearer, the Sol Ring and the Coating.
fn pest_table(actor: PlayerId) -> (GameState, ObjectId, Target, Target, Target) {
    let opponent = actor.opponent();
    let mut state = game(actor, "Forest");
    let bearer = put(&mut state, opponent, BEARER, Zone::Battlefield);
    let ring = put(&mut state, opponent, "Sol Ring", Zone::Battlefield);
    let coating = put(&mut state, actor, "Liquimetal Coating", Zone::Battlefield);
    act(&mut state, Action::ActivateAbility(coating, 0));
    choose_target(&mut state, Target::Object(bearer));
    settled(&mut state);
    assert!(engine::object_has_type(
        &state,
        bearer,
        mtg_kernel::card_def::CardType::Artifact
    ));
    let infestation = put(&mut state, actor, "Pest Infestation", Zone::Hand);
    (
        state,
        infestation,
        Target::Object(bearer),
        Target::Object(ring),
        Target::Object(coating),
    )
}

#[test]
fn flagbearers_bind_pest_infestations_payable_targets_both_seats() {
    for actor in [P0, P1] {
        // {G} and two more: X is at most 1, so a single target must be the
        // artifact Flagbearer, and zero targets stays legal.
        let (mut state, infestation, bearer, ring, _) = pest_table(actor);
        pool(&mut state, actor, &[ManaColor::G], 2);
        announce(&mut state, infestation, None);
        assert_eq!(offered(&mut state), (vec![bearer], true));
        let hash = state.state_hash();
        assert!(engine::step(&mut state, Action::ChooseTarget(ring)).is_err());
        assert_eq!(state.state_hash(), hash);
        let mut zero = state.clone();
        act(&mut zero, Action::FinishEffectSelection);
        settled(&mut zero);
        assert_eq!(zone(&zero, infestation), Zone::Graveyard);
        let Target::Object(bearer_id) = bearer else {
            unreachable!()
        };
        assert_eq!(zone(&zero, bearer_id), Zone::Battlefield);
        choose_target(&mut state, bearer);
        settled(&mut state);
        assert_eq!(zone(&state, bearer_id), Zone::Graveyard);

        // {G} and four more: X may be 2, so the Sol Ring may come first, but
        // then the selection cannot finish without the Flagbearer.
        let (mut state, infestation, bearer, ring, coating) = pest_table(actor);
        pool(&mut state, actor, &[ManaColor::G], 4);
        announce(&mut state, infestation, None);
        let (legal, can_finish) = offered(&mut state);
        assert!(can_finish);
        assert!(same_set(legal, vec![bearer, ring, coating]));
        choose_target(&mut state, ring);
        assert_eq!(offered(&mut state), (vec![bearer], false));
        let hash = state.state_hash();
        assert!(engine::step(&mut state, Action::FinishEffectSelection).is_err());
        assert_eq!(state.state_hash(), hash);
        let mut forged = state.clone();
        forged
            .engine
            .pending_cast
            .as_mut()
            .unwrap()
            .target_selection_finished = true;
        assert!(matches!(
            engine::advance_until_decision(&mut forged),
            Decision::Halted { .. }
        ));
        choose_target(&mut state, bearer);
        settled(&mut state);
        let (Target::Object(bearer), Target::Object(ring)) = (bearer, ring) else {
            unreachable!()
        };
        assert_eq!(zone(&state, bearer), Zone::Graveyard);
        assert_eq!(zone(&state, ring), Zone::Graveyard);
    }
}

// ---- Per-card cast filters --------------------------------------------------------

#[test]
fn flagbearers_are_judged_after_dominates_payable_x_filter_both_seats() {
    for actor in [P0, P1] {
        let opponent = actor.opponent();
        let mut state = game(actor, "Island");
        // Coalition Honor Guard's mana value is 4; Llanowar Elves's is 1.
        let guard = put(&mut state, opponent, GUARD, Zone::Battlefield);
        let elves = put(&mut state, opponent, "Llanowar Elves", Zone::Battlefield);
        let dominate = put(&mut state, actor, "Dominate", Zone::Hand);
        let mut wide = state.clone();

        // {1}{U}{U} and one more: X is at most 1, so the Honor Guard is not
        // a legal target and the Elves are.
        pool(&mut state, actor, &[ManaColor::U, ManaColor::U], 2);
        announce(&mut state, dominate, None);
        assert_eq!(offered(&mut state).0, [Target::Object(elves)]);
        choose_target(&mut state, Target::Object(elves));
        settled(&mut state);
        assert_eq!(state.objects.get(elves).controller, actor);
        assert_eq!(state.objects.get(guard).controller, opponent);

        // With X = 4 payable the Honor Guard is legal and must be chosen.
        pool(&mut wide, actor, &[ManaColor::U, ManaColor::U], 5);
        announce(&mut wide, dominate, None);
        assert_eq!(offered(&mut wide).0, [Target::Object(guard)]);
    }
}

// ---- Rulings ----------------------------------------------------------------------

#[test]
fn flagbearers_do_not_bind_triggered_abilities_both_seats() {
    for actor in [P0, P1] {
        let opponent = actor.opponent();
        let mut state = game(actor, "Plains");
        let bearer = put(&mut state, opponent, BEARER, Zone::Battlefield);
        let elves = put(&mut state, opponent, "Llanowar Elves", Zone::Battlefield);
        let journey = put(&mut state, actor, "Journey to Nowhere", Zone::Hand);
        pool(&mut state, actor, &[ManaColor::W], 1);
        announce(&mut state, journey, None);
        let mut offered_targets = Vec::new();
        settle_with(&mut state, |_, decision| match decision {
            Decision::ChooseTargets { legal_targets, .. } => {
                offered_targets = legal_targets;
                Action::ChooseTarget(Target::Object(elves))
            }
            other => first_answer(other),
        });
        assert!(offered_targets.contains(&Target::Object(bearer)));
        assert!(offered_targets.contains(&Target::Object(elves)));
        assert_eq!(zone(&state, elves), Zone::Exile);
        assert_eq!(zone(&state, bearer), Zone::Battlefield);
    }
}

#[test]
fn flagbearers_do_not_bind_the_new_targets_of_storm_copies_both_seats() {
    for actor in [P0, P1] {
        let opponent = actor.opponent();
        let mut state = game(actor, "Mountain");
        let guard = Target::Object(put(&mut state, opponent, GUARD, Zone::Battlefield));
        let elves = put(&mut state, opponent, "Llanowar Elves", Zone::Battlefield);
        let bolt = put(&mut state, actor, "Lightning Bolt", Zone::Hand);
        pool(&mut state, actor, &[ManaColor::R], 0);
        announce(&mut state, bolt, None);
        assert_eq!(offered(&mut state).0, [guard]);
        choose_target(&mut state, guard);
        settled(&mut state);

        let shot = put(&mut state, actor, "Grapeshot", Zone::Hand);
        pool(&mut state, actor, &[ManaColor::R], 1);
        announce(&mut state, shot, None);
        // The cast Grapeshot is bound.
        assert_eq!(offered(&mut state).0, [guard]);
        choose_target(&mut state, guard);
        // Its storm copy is not cast and may choose the Elves.
        let mut copy_targets = Vec::new();
        settle_with(&mut state, |_, decision| match decision {
            Decision::ChooseTargets { legal_targets, .. }
                if legal_targets.contains(&Target::Object(elves)) =>
            {
                copy_targets = legal_targets;
                Action::ChooseTarget(Target::Object(elves))
            }
            Decision::ChooseEffectTargets { legal_targets, .. }
                if legal_targets.contains(&Target::Object(elves)) =>
            {
                copy_targets = legal_targets;
                Action::ChooseEffectTarget(Target::Object(elves))
            }
            other => first_answer(other),
        });
        assert!(copy_targets.contains(&Target::Player(opponent)));
        assert_eq!(zone(&state, elves), Zone::Graveyard);
        let Target::Object(guard) = guard else {
            unreachable!()
        };
        assert_eq!(zone(&state, guard), Zone::Graveyard, "three and one damage");
    }
}

#[test]
fn flagbearers_do_not_bind_a_changed_target_both_seats() {
    for actor in [P0, P1] {
        let opponent = actor.opponent();
        let mut state = game(actor, "Island");
        let guard = put(&mut state, opponent, GUARD, Zone::Battlefield);
        let elves = put(&mut state, opponent, "Llanowar Elves", Zone::Battlefield);
        let bolt = put(&mut state, actor, "Lightning Bolt", Zone::Hand);
        let misdirection = put(&mut state, actor, "Misdirection", Zone::Hand);
        pool(&mut state, actor, &[ManaColor::R], 0);
        announce(&mut state, bolt, None);
        assert_eq!(offered(&mut state).0, [Target::Object(guard)]);
        choose_target(&mut state, Target::Object(guard));
        // Misdirection's own target is a spell, never a Flagbearer.
        pool(&mut state, actor, &[ManaColor::U, ManaColor::U], 3);
        announce(&mut state, misdirection, None);
        choose_target(&mut state, Target::Object(bolt));
        let mut new_targets = Vec::new();
        settle_with(&mut state, |_, decision| match decision {
            Decision::ChooseEffectTargets { legal_targets, .. }
                if legal_targets.contains(&Target::Object(elves)) =>
            {
                new_targets = legal_targets;
                Action::ChooseEffectTarget(Target::Object(elves))
            }
            Decision::ChooseTargets { legal_targets, .. }
                if legal_targets.contains(&Target::Object(elves)) =>
            {
                new_targets = legal_targets;
                Action::ChooseTarget(Target::Object(elves))
            }
            other => first_answer(other),
        });
        assert!(new_targets.contains(&Target::Object(elves)));
        assert_eq!(zone(&state, elves), Zone::Graveyard);
        assert_eq!(state.objects.get(guard).damage, 0);
        assert_eq!(zone(&state, guard), Zone::Battlefield);
    }
}

#[test]
fn flagbearers_never_force_a_mode_both_seats() {
    for actor in [P0, P1] {
        let opponent = actor.opponent();
        for mode in [BRUTALITY_REVEAL, BRUTALITY_DRAIN] {
            let mut state = game(actor, "Swamp");
            let bearer = put(&mut state, opponent, BEARER, Zone::Battlefield);
            let brutality = put(&mut state, actor, "Collective Brutality", Zone::Hand);
            pool(&mut state, actor, &[ManaColor::B], 1);
            announce(&mut state, brutality, Some(mode));
            assert_eq!(offered(&mut state).0, [Target::Player(opponent)]);
            choose_target(&mut state, Target::Player(opponent));
            settled(&mut state);
            assert_eq!(zone(&state, brutality), Zone::Graveyard);
            assert_eq!(zone(&state, bearer), Zone::Battlefield);
            let drained = if mode == BRUTALITY_DRAIN { 18 } else { 20 };
            assert_eq!(life(&state, opponent), drained);
        }
    }
}

#[test]
fn flagbearers_are_satisfied_by_one_target_among_several_both_seats() {
    for actor in [P0, P1] {
        let opponent = actor.opponent();
        let mut state = game(actor, "Island");
        let bearer = Target::Object(put(&mut state, opponent, BEARER, Zone::Battlefield));
        let elves = Target::Object(put(
            &mut state,
            opponent,
            "Llanowar Elves",
            Zone::Battlefield,
        ));
        let warp = put(&mut state, actor, "Agony Warp", Zone::Hand);
        pool(&mut state, actor, &[ManaColor::U, ManaColor::B], 0);
        announce(&mut state, warp, None);
        // Either slot can name the Bearer, so the first is free.
        assert!(same_set(offered(&mut state).0, vec![bearer, elves]));

        let mut bearer_first = state.clone();
        choose_target(&mut bearer_first, bearer);
        assert!(same_set(offered(&mut bearer_first).0, vec![bearer, elves]));
        choose_target(&mut bearer_first, elves);
        settled(&mut bearer_first);
        let Target::Object(elves_id) = elves else {
            unreachable!()
        };
        assert_eq!(zone(&bearer_first, elves_id), Zone::Graveyard);

        choose_target(&mut state, elves);
        assert_eq!(offered(&mut state).0, [bearer]);
        choose_target(&mut state, bearer);
        settled(&mut state);
        let Target::Object(bearer_id) = bearer else {
            unreachable!()
        };
        assert_eq!(zone(&state, bearer_id), Zone::Graveyard);
    }
}

#[test]
fn flagbearers_of_either_player_satisfy_the_obligation_both_seats() {
    for actor in [P0, P1] {
        let opponent = actor.opponent();
        let mut state = game(actor, "Mountain");
        let own = Target::Object(put(&mut state, actor, BEARER, Zone::Battlefield));
        let theirs = Target::Object(put(&mut state, opponent, GUARD, Zone::Battlefield));
        put(&mut state, opponent, "Llanowar Elves", Zone::Battlefield);
        let bolt = put(&mut state, actor, "Lightning Bolt", Zone::Hand);
        pool(&mut state, actor, &[ManaColor::R], 0);
        announce(&mut state, bolt, None);
        assert!(same_set(offered(&mut state).0, vec![own, theirs]));
        choose_target(&mut state, own);
        settled(&mut state);
        let Target::Object(own) = own else {
            unreachable!()
        };
        assert_eq!(zone(&state, own), Zone::Graveyard);
    }
}

#[test]
fn flagbearers_bind_activated_abilities_both_seats() {
    for actor in [P0, P1] {
        let opponent = actor.opponent();
        let mut state = game(actor, "Mountain");
        let bearer = put(&mut state, opponent, BEARER, Zone::Battlefield);
        let elves = put(&mut state, opponent, "Llanowar Elves", Zone::Battlefield);
        let fanatic = put(&mut state, actor, "Mogg Fanatic", Zone::Battlefield);
        act(&mut state, Action::ActivateAbility(fanatic, 0));
        assert_eq!(offered(&mut state).0, [Target::Object(bearer)]);
        let hash = state.state_hash();
        assert!(engine::step(&mut state, Action::ChooseTarget(Target::Object(elves))).is_err());
        assert_eq!(state.state_hash(), hash);
        choose_target(&mut state, Target::Object(bearer));
        settled(&mut state);
        assert_eq!(zone(&state, bearer), Zone::Graveyard);
        assert_eq!(zone(&state, fanatic), Zone::Graveyard);
    }
}

#[test]
fn flagbearers_without_their_ability_bind_nothing_both_seats() {
    for actor in [P0, P1] {
        let opponent = actor.opponent();
        let mut state = game(actor, "Island");
        put(&mut state, opponent, BEARER, Zone::Battlefield);
        // Each player controls a creature, so either may be Searing
        // Blaze's player.
        put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
        let dress = put(&mut state, actor, "Dress Down", Zone::Hand);
        pool(&mut state, actor, &[ManaColor::U], 1);
        announce(&mut state, dress, None);
        settled(&mut state);
        let blaze = put(&mut state, actor, "Searing Blaze", Zone::Hand);
        pool(&mut state, actor, &[ManaColor::R, ManaColor::R], 0);
        announce(&mut state, blaze, None);
        assert_eq!(
            offered(&mut state).0,
            [Target::Player(P0), Target::Player(P1)]
        );
    }
}

#[test]
fn a_phyrexian_metamorph_copy_of_a_flagbearer_binds_and_is_the_target_both_seats() {
    for actor in [P0, P1] {
        let opponent = actor.opponent();
        let mut state = game(opponent, "Island");
        let bearer = put(&mut state, opponent, BEARER, Zone::Battlefield);
        let elves = put(&mut state, opponent, "Llanowar Elves", Zone::Battlefield);
        let metamorph = put(&mut state, opponent, "Phyrexian Metamorph", Zone::Hand);
        pool(&mut state, opponent, &[ManaColor::U], 3);
        announce(&mut state, metamorph, None);
        settle_with(&mut state, |_, decision| match decision {
            Decision::ChooseEffectTargets { legal_targets, .. }
                if legal_targets.contains(&Target::Object(bearer)) =>
            {
                Action::ChooseEffectTarget(Target::Object(bearer))
            }
            other => first_answer(other),
        });
        assert_eq!(state.objects.get(metamorph).card_def, id(BEARER));
        // Only the copy is left to bind the caster and to be targeted.
        propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(bearer, Zone::Graveyard),
        );
        state.priority_player = actor;
        let bolt = put(&mut state, actor, "Lightning Bolt", Zone::Hand);
        pool(&mut state, actor, &[ManaColor::R], 0);
        announce(&mut state, bolt, None);
        assert_eq!(offered(&mut state).0, [Target::Object(metamorph)]);
        choose_target(&mut state, Target::Object(metamorph));
        settled(&mut state);
        assert_eq!(zone(&state, metamorph), Zone::Graveyard);
        assert_eq!(zone(&state, elves), Zone::Battlefield);
    }
}

#[test]
fn flagbearers_under_dress_down_bind_nothing_and_any_flagbearer_counts_after_both_seats() {
    for actor in [P0, P1] {
        let opponent = actor.opponent();
        let mut state = game(actor, "Island");
        let bearer = put(&mut state, opponent, BEARER, Zone::Battlefield);
        let guard = put(&mut state, actor, GUARD, Zone::Battlefield);
        put(&mut state, opponent, "Llanowar Elves", Zone::Battlefield);
        let dress = put(&mut state, actor, "Dress Down", Zone::Hand);
        pool(&mut state, actor, &[ManaColor::U], 1);
        announce(&mut state, dress, None);
        settled(&mut state);
        // Both Flagbearers lost their abilities: nothing is bound.
        let mut probe = state.clone();
        let blaze = put(&mut probe, actor, "Searing Blaze", Zone::Hand);
        pool(&mut probe, actor, &[ManaColor::R, ManaColor::R], 0);
        announce(&mut probe, blaze, None);
        assert_eq!(
            offered(&mut probe).0,
            [Target::Player(P0), Target::Player(P1)]
        );
        // With Dress Down gone and a second opposing Standard Bearer, either
        // player's Flagbearer satisfies the obligation.
        propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(dress, Zone::Graveyard),
        );
        let second = put(&mut state, opponent, BEARER, Zone::Battlefield);
        let bolt = put(&mut state, actor, "Lightning Bolt", Zone::Hand);
        pool(&mut state, actor, &[ManaColor::R], 0);
        announce(&mut state, bolt, None);
        assert!(same_set(
            offered(&mut state).0,
            [bearer, guard, second].map(Target::Object).to_vec()
        ));
    }
}

#[test]
fn flagbearers_bind_the_last_of_mystic_confluences_three_bounces_both_seats() {
    // Mode 6: three "return target creature to its owner's hand" slots, each
    // of which may name what another named.
    const BOUNCE_BOUNCE_BOUNCE: u8 = 6;
    for actor in [P0, P1] {
        let opponent = actor.opponent();
        let mut state = game(actor, "Island");
        let bearer = Target::Object(put(&mut state, opponent, BEARER, Zone::Battlefield));
        let elves = Target::Object(put(
            &mut state,
            opponent,
            "Llanowar Elves",
            Zone::Battlefield,
        ));
        let mystic = put(&mut state, actor, "Mystic Confluence", Zone::Hand);
        pool(&mut state, actor, &[ManaColor::U, ManaColor::U], 3);
        announce(&mut state, mystic, Some(BOUNCE_BOUNCE_BOUNCE));
        for _ in 0..2 {
            assert!(same_set(offered(&mut state).0, vec![bearer, elves]));
            choose_target(&mut state, elves);
        }
        assert_eq!(offered(&mut state).0, [bearer]);
        choose_target(&mut state, bearer);
        settled(&mut state);
        let (Target::Object(bearer), Target::Object(elves)) = (bearer, elves) else {
            unreachable!()
        };
        assert_eq!(zone(&state, bearer), Zone::Hand);
        assert_eq!(zone(&state, elves), Zone::Hand);
    }
}

#[test]
fn flagbearers_bind_fire_covenants_last_payable_pick_both_seats() {
    for actor in [P0, P1] {
        let opponent = actor.opponent();
        let mut state = game(actor, "Swamp");
        state.players[actor.index()].life = 3;
        let bearer = Target::Object(put(&mut state, opponent, BEARER, Zone::Battlefield));
        let elves = Target::Object(put(
            &mut state,
            opponent,
            "Llanowar Elves",
            Zone::Battlefield,
        ));
        let covenant = put(&mut state, actor, "Fire Covenant", Zone::Hand);
        pool(&mut state, actor, &[ManaColor::B, ManaColor::R], 1);
        announce(&mut state, covenant, None);
        // Zero picks stays legal, and three life allows three picks.
        let (legal, can_finish) = offered(&mut state);
        assert!(can_finish);
        assert!(same_set(legal, vec![bearer, elves]));
        choose_target(&mut state, elves);
        let (legal, can_finish) = offered(&mut state);
        assert!(!can_finish);
        assert!(same_set(legal, vec![bearer, elves]));
        choose_target(&mut state, elves);
        assert_eq!(offered(&mut state), (vec![bearer], false));
        let hash = state.state_hash();
        assert!(engine::step(&mut state, Action::FinishEffectSelection).is_err());
        assert!(engine::step(&mut state, Action::ChooseTarget(elves)).is_err());
        assert_eq!(state.state_hash(), hash);
        choose_target(&mut state, bearer);
        assert_eq!(offered(&mut state), (Vec::new(), true));
    }
}

// ---- The second part's target shapes ---------------------------------------------

#[test]
fn flagbearers_never_bind_unlicensed_hearses_graveyard_targets_both_seats() {
    // "{T}: Exile up to two target cards from a single graveyard": no card
    // in a graveyard is a Flagbearer on the battlefield, so an opposing
    // Standard Bearer changes nothing.
    for actor in [P0, P1] {
        let opponent = actor.opponent();
        let mut state = game(actor, "Island");
        let bearer = put(&mut state, opponent, BEARER, Zone::Battlefield);
        let hearse = put(&mut state, actor, "Unlicensed Hearse", Zone::Battlefield);
        let bolt = Target::Object(put(&mut state, opponent, "Lightning Bolt", Zone::Graveyard));
        let seal = Target::Object(put(&mut state, opponent, "Seal of Fire", Zone::Graveyard));
        let island = Target::Object(put(&mut state, actor, "Island", Zone::Graveyard));
        act(&mut state, Action::ActivateAbility(hearse, 0));
        let (legal, can_finish) = offered(&mut state);
        assert!(can_finish, "zero targets stay legal");
        assert!(same_set(legal, vec![bolt, seal, island]));
        choose_target(&mut state, bolt);
        assert_eq!(offered(&mut state), (vec![seal], true));
        act(&mut state, Action::FinishEffectSelection);
        settled(&mut state);
        let Target::Object(bolt) = bolt else {
            unreachable!()
        };
        assert_eq!(zone(&state, bolt), Zone::Exile);
        assert_eq!(zone(&state, bearer), Zone::Battlefield);
    }
}

#[test]
fn flagbearers_never_bind_eowyns_enters_trigger_both_seats() {
    // "When Eowyn enters, exile target creature an opponent controls with
    // greater power": a triggered ability. Three +1/+1 counters make the
    // Standard Bearer a 4/4, a legal target, so a bound trigger would offer
    // only the Bearer.
    for actor in [P0, P1] {
        let opponent = actor.opponent();
        let mut state = game(actor, "Plains");
        let bearer = put(&mut state, opponent, BEARER, Zone::Battlefield);
        state.objects.get_mut(bearer).counters.plus1_plus1 = 3;
        assert_eq!(engine::effective_power(&state, bearer), 4);
        let big = put(&mut state, opponent, "Griselbrand", Zone::Battlefield);
        let eowyn = put(&mut state, actor, "Eowyn, Fearless Knight", Zone::Hand);
        pool(&mut state, actor, &[ManaColor::R, ManaColor::W], 2);
        announce(&mut state, eowyn, None);
        let mut trigger_targets = Vec::new();
        settle_with(&mut state, |_, decision| match decision {
            Decision::ChooseTargets { legal_targets, .. } => {
                trigger_targets = legal_targets;
                Action::ChooseTarget(Target::Object(big))
            }
            other => first_answer(other),
        });
        assert!(same_set(
            trigger_targets,
            vec![Target::Object(bearer), Target::Object(big)]
        ));
        assert_eq!(zone(&state, big), Zone::Exile);
        assert_eq!(zone(&state, bearer), Zone::Battlefield);
        assert_eq!(zone(&state, eowyn), Zone::Battlefield);
    }
}

// ---- Public-session consumer games -----------------------------------------------

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

/// Whether `action` chooses as a target a Flagbearer that an opponent of
/// the chooser controls.
fn targets_opposing_flagbearer(action: &Value) -> bool {
    let semantic = &action["semantic"];
    let target = &semantic["target"];
    target["target_kind"] == "object"
        && target["object"]["controller"] != semantic["actor"]
        && target["object"]["card_db_id"]
            .as_u64()
            .is_some_and(|id| [BEARER, GUARD].contains(&CARD_DEFS[id as usize].name))
}

#[test]
fn flagbearer_decks_finish_and_replay_through_public_sessions() {
    let white = deck(&[
        ("Plains", 18),
        (BEARER, 8),
        (GUARD, 6),
        ("Thraben Inspector", 4),
        ("Journey to Nowhere", 4),
    ]);
    let red = deck(&[
        ("Mountain", 18),
        ("Lightning Bolt", 10),
        ("Mogg Fanatic", 8),
        ("Searing Blaze", 4),
    ]);
    let mut flagbearer_casts = 0;
    let mut flagbearer_targets = 0;
    let mut games = Vec::new();
    for (pair, first_seed) in [
        ([white.clone(), red.clone()], 0x5331_4f00_u64),
        ([red, white], 0x5331_4f10),
    ] {
        for seed in first_seed..first_seed + 3 {
            for policy in [
                Policy::Ranked,
                Policy::Random(seed + 0x100),
                Policy::Random(seed + 0x200),
            ] {
                games.push((pair.clone(), seed, policy));
            }
        }
    }
    // The games run on worker threads (`in_parallel`); the totals add their
    // counts in this loop's order.
    let counts = in_parallel(&games, |(pair, seed, policy)| {
        let mut targets = 0;
        let game = play_observed(pair.clone(), None, *seed, *policy, &mut |_, action| {
            targets += usize::from(targets_opposing_flagbearer(action));
        });
        let replay = play_observed(pair.clone(), None, *seed, *policy, &mut |_, _| {});
        assert_eq!(
            game.transcript_sha256, replay.transcript_sha256,
            "seed={seed} policy={policy:?}"
        );
        let casts = game
            .cast_cards
            .iter()
            .flat_map(|cards| [id(BEARER), id(GUARD)].map(|card| cards.get(&card)))
            .flatten()
            .sum::<usize>();
        (targets, casts)
    });
    for (targets, casts) in counts {
        flagbearer_targets += targets;
        flagbearer_casts += casts;
    }
    assert!(flagbearer_casts > 0, "no Flagbearer cast");
    assert!(flagbearer_targets > 0, "no opposing Flagbearer targeted");
}
