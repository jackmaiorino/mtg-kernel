//! A two-object mana cost in a priority window of a session
//! (`priority_action_windows_v1`; main only, where the eternal catalog has
//! Birchlore Rangers: "Tap two untapped Elves you control: Add one mana of
//! any color").
//!
//! The session is built by the explicit-deck reset with priority-window
//! rules, in the flat-action V2 form the V3 training reset wraps and the
//! eternal actor's sessions start from, out of two real sixty-card lists.
//! Its game is then set to a labelled scene made of those lists' own cards
//! (the eternal actor's tests arrange their scenes the same way), with the
//! same rules on a fresh surface, and every decision after that is the
//! session's own: its menu, its step, its engine.
//!
//! The flat action contracts still refuse a menu with this action
//! (`UnsupportedActionSemantic`); the session presents and applies it all
//! the same, as it does for the eternal actor.

use super::*;
use crate::engine::Action;
use crate::priority_action_windows_v1::SeatPriorityWindowsV1;
use crate::state::{GameState, Step, Target, Zone};
use crate::surface::{SuppressionReason, SurfaceAction};

fn card(name: &str) -> u16 {
    crate::card_def::card_id_by_name(name).unwrap_or_else(|| panic!("{name} not in CARD_DEFS"))
}

/// Sixty real cards: the Rangers, Elves to pay their cost with, an instant
/// that only their mana pays for in the scene, and Forests.
fn mainboard() -> Vec<u16> {
    let mut cards = Vec::new();
    for (name, count) in [
        ("Birchlore Rangers", 4),
        ("Llanowar Elves", 4),
        ("Lightning Bolt", 4),
        ("Forest", 48),
    ] {
        cards.extend(std::iter::repeat_n(card(name), count));
    }
    assert_eq!(cards.len(), 60);
    cards
}

/// P0's first upkeep with Birchlore Rangers and Llanowar Elves in play, both
/// summoning-sick (neither payment of the Rangers' cost is the tap symbol),
/// and Lightning Bolt in hand. No land: only the Rangers' ability makes mana.
/// Returns the session and the Bolt, the Rangers and the Elf.
fn scene(
    windows: [SeatPriorityWindowsV1; 2],
) -> (FastActorSessionV1, ObjectId, ObjectId, ObjectId) {
    let cards = mainboard();
    let mut session = FastActorSessionV1::reset_with_explicit_decks_and_limits_flat_action_v2_environment_v2_with_starting_player_and_windows_v1(
        7,
        0x5eed_0f00,
        1_000,
        10_000,
        ["elves".into(), "elves".into()],
        [cards.clone(), cards.clone()],
        PlayerId::P0,
        windows,
    )
    .unwrap();
    assert_eq!(
        session.surface.harness_surface().action_window_seats_v1(),
        windows
    );

    let mut state = GameState::new_from_libraries_environment_v2_with_starting_player_v1(
        &cards,
        &cards,
        crate::rl::card_name,
        0x5eed_0f01,
        PlayerId::P0,
    );
    state.step = Step::Upkeep;
    let mut moved = Vec::new();
    for (name, zone) in [
        ("Lightning Bolt", Zone::Hand),
        ("Birchlore Rangers", Zone::Battlefield),
        ("Llanowar Elves", Zone::Battlefield),
    ] {
        let id = card(name);
        let at = state.players[0]
            .library
            .iter()
            .position(|&object| state.objects.get(object).card_def == id)
            .unwrap();
        let object = state.players[0].library.remove(at);
        state.objects.get_mut(object).zone = zone;
        match zone {
            Zone::Hand => state.players[0].hand.push(object),
            Zone::Battlefield => {
                state.players[0].battlefield.push(object);
                state.objects.get_mut(object).summoning_sick = true;
            }
            _ => unreachable!(),
        }
        moved.push(object);
    }

    let mut surface = PolicySurfaceV5::new_for_session();
    surface.set_action_window_seats_v1(windows).unwrap();
    session.state = state;
    session.surface = surface;
    session.environment_revision = 0;
    session.policy_step_count = 0;
    session.physical_decision_count = 0;
    session.current = None;
    session.terminal = None;
    session.flat_action_cache_spare = None;
    session.flat_action_cache_spare_v2 = None;
    session.advance_to_decision_or_terminal();
    (session, moved[0], moved[1], moved[2])
}

fn decision(session: &FastActorSessionV1) -> FastActorDecisionV1 {
    match session.current_response() {
        FastActorResponseV1::Decision(decision) => decision,
        terminal => panic!("expected a decision, got {terminal:?}"),
    }
}

/// The index of the menu row whose engine action `wanted` accepts.
fn row(session: &FastActorSessionV1, wanted: impl Fn(&Action) -> bool) -> u32 {
    let candidates = &session
        .current
        .as_ref()
        .expect("a pending decision")
        .candidates;
    candidates
        .iter()
        .position(|candidate| {
            matches!(
                &candidate.policy_action,
                PolicyActionV5::Surface(SurfaceAction::Action(action)) if wanted(action)
            )
        })
        .unwrap_or_else(|| {
            panic!(
                "the action is not in the menu: {:?}",
                candidates
                    .iter()
                    .map(|candidate| &candidate.semantic)
                    .collect::<Vec<_>>()
            )
        }) as u32
}

#[test]
fn a_two_object_mana_cost_is_offered_and_pays_in_a_window_of_a_session() {
    let (mut session, bolt, ranger, elf) = scene([
        SeatPriorityWindowsV1::ActionWindowsV1,
        SeatPriorityWindowsV1::HarnessV2,
    ]);
    let mut pair = [ranger, elf];
    pair.sort();

    // The action seat is handed its upkeep, which HarnessV2 passes: the
    // window's only line starts with the Rangers' ability.
    let upkeep = decision(&session);
    assert_eq!(upkeep.acting_player, PlayerSeatV1::from(PlayerId::P0));
    assert_eq!(session.game_state().step, Step::Upkeep);
    assert_eq!(
        session.surface.engine_window_v1(),
        Some(SuppressionReason::StepGated)
    );
    let candidates = &session.current.as_ref().unwrap().candidates;
    assert!(candidates.iter().all(|candidate| !matches!(
        candidate.policy_action,
        PolicyActionV5::Surface(SurfaceAction::Action(Action::CastSpell(_)))
    )));
    // The menu offers the pair once per printed color, by its own action.
    let pair_rows = candidates
        .iter()
        .filter(|candidate| {
            matches!(
                &candidate.semantic,
                ActionSemanticV1::ActivateManaAbilityWithTwoCostTargetsV1 { .. }
            )
        })
        .count();
    assert_eq!(pair_rows, 5);
    let pay = row(&session, |action| {
        matches!(
            action,
            Action::ActivateManaAbilityWithTwoCostTargetsV1(source, ManaColor::R, targets)
                if *source == ranger && *targets == pair
        )
    });

    // It applies in the window: both Elves tap for one red mana.
    session.step(upkeep.episode_id, upkeep.step, pay).unwrap();
    let state = session.game_state();
    assert!(state.objects.get(ranger).tapped && state.objects.get(elf).tapped);
    assert_eq!(state.players[0].mana_pool[ManaColor::R.pool_index()], 1);
    assert!(state.stack.is_empty());

    // The seat keeps the window, now with the Bolt to cast.
    let window = decision(&session);
    assert_eq!(window.acting_player, PlayerSeatV1::from(PlayerId::P0));
    assert_eq!(session.game_state().step, Step::Upkeep);
    assert_eq!(
        session.surface.engine_window_v1(),
        Some(SuppressionReason::StepGated)
    );
    let cast = row(
        &session,
        |action| matches!(action, Action::CastSpell(spell) if *spell == bolt),
    );
    session.step(window.episode_id, window.step, cast).unwrap();
    let targets = decision(&session);
    assert_eq!(targets.acting_player, PlayerSeatV1::from(PlayerId::P0));
    let face = row(&session, |action| {
        matches!(action, Action::ChooseTarget(Target::Player(PlayerId::P1)))
    });
    session
        .step(targets.episode_id, targets.step, face)
        .unwrap();
    // Neither seat is asked before the Bolt resolves: its caster has nothing
    // left to do and the other seat is on HarnessV2 in a gated step.
    assert_eq!(session.game_state().players[1].life, 17);
    assert_eq!(session.game_state().objects.get(bolt).zone, Zone::Graveyard);
}

/// With both seats on `harness-v2` the same scene's upkeep is passed for
/// P0, as today: the first decision is in a later step and is not a window a
/// rule added.
#[test]
fn the_same_upkeep_is_passed_without_an_action_window_rule() {
    let (session, _, _, _) = scene([SeatPriorityWindowsV1::HarnessV2; 2]);
    if let FastActorResponseV1::Decision(_) = session.current_response() {
        assert_ne!(session.game_state().step, Step::Upkeep);
        assert_eq!(session.surface.engine_window_v1(), None);
    }
}
