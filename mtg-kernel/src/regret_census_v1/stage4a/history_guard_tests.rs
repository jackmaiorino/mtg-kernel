//! The event-history index changes no game. A history guard skips a scan
//! only when no event of the kinds its scan acts on occurred, so every
//! computation must give the same result with its guards and with every
//! guard forced open. These tests compare the two on fixture games, and on
//! mid-game states with one event of each kind added: a kind no fixture game
//! produces is exactly where a guard that left it out would hide.

use super::tests::test_cfg;
use super::{game_setup, new_session};
use crate::event::CommittedEvent;
use crate::event_history_v1::guard_tests_v1::{
    every_kind_v1, recording_sites, with_scans_forced, SitesV1,
};
use crate::rl::{ActionSemanticV1, TerminalClassificationV1};
use crate::rl_session::{FastActorDecisionV1, FastActorResponseV1, FastActorSessionV1};
use crate::state::GameState;
use sha2::{Digest, Sha256};

fn state_digest(state: &GameState) -> String {
    let bytes = serde_json::to_vec(state).expect("state serializes");
    format!("{:x}", Sha256::digest(&bytes))
}

/// Exercise the same native session on the pinned fixture deal. The frozen
/// scorer has a narrower observation contract than this engine guard test.
fn drive_native(
    game: u64,
    mut visit: impl FnMut(&FastActorSessionV1, FastActorDecisionV1, &[ActionSemanticV1], u32) -> bool,
) -> FastActorSessionV1 {
    let setup = game_setup(&test_cfg(), 1, game);
    let mut session = new_session(&setup).expect("fixture game resets");
    while let FastActorResponseV1::Decision(decision) = session.current_response() {
        let actions = session
            .diagnostic_current_action_semantics()
            .expect("native fixture decision has a menu");
        assert_eq!(actions.len(), decision.legal_action_count as usize);
        // Play lands and spells, commit attackers and blockers, then take
        // the first native ordinal within each priority. Avoid repeatedly
        // activating optional abilities while a pass is available.
        let (index, _) = actions
            .iter()
            .enumerate()
            .min_by_key(|&(index, action)| {
                let rank = match action {
                    ActionSemanticV1::PlayLand { .. }
                    | ActionSemanticV1::ChooseAttackerInclusion { include: true, .. }
                    | ActionSemanticV1::ChooseBlockerInclusion { include: true, .. } => 0,
                    ActionSemanticV1::CastSpell { .. } => 1,
                    ActionSemanticV1::Pass { .. } => 3,
                    ActionSemanticV1::ActivateManaAbility { .. }
                    | ActionSemanticV1::ActivateAbility { .. } => 4,
                    _ => 2,
                };
                (rank, index)
            })
            .expect("native fixture decision has a legal action");
        let action = index as u32;
        if visit(&session, decision, &actions, action) {
            return session;
        }
        session
            .step(decision.episode_id, decision.step, action)
            .unwrap_or_else(|error| {
                panic!(
                    "fixture game {game} step {} action {action}: {error:?}",
                    decision.step
                )
            });
    }
    session
}

/// Each decision, its full native menu, selected ordinal and state digest,
/// followed by the terminal result.
fn game_record(game: u64) -> Vec<String> {
    let mut record = Vec::new();
    let session = drive_native(game, |s, decision, actions, action| {
        record.push(format!(
            "{decision:?} {actions:?} {action} {}",
            state_digest(s.game_state()),
        ));
        false
    });
    let response = session.current_response();
    assert!(
        matches!(&response, FastActorResponseV1::Terminal(terminal)
            if terminal.terminal_classification == TerminalClassificationV1::Natural),
        "game {game} did not finish naturally: {response:?}",
    );
    // The original scorer refusal was after combat in game 0.
    if game == 0 {
        assert!(
            session
                .game_state()
                .engine
                .event_history
                .iter()
                .any(|event| matches!(event, CommittedEvent::CombatDamageToPlayer { .. })),
            "game {game} produced no combat damage history",
        );
    }
    record.push(format!(
        "{response:?} {}",
        state_digest(session.game_state())
    ));
    record
}

fn report(label: &str, sites: &SitesV1) {
    let skipping = sites.values().filter(|[_, skipped]| *skipped > 0).count();
    let skips: u64 = sites.values().map(|[_, skipped]| skipped).sum();
    eprintln!(
        "{label}: {} guards reached, {skipping} of them skipped their scan {skips} times",
        sites.len()
    );
}

#[test]
fn fixture_games_are_identical_with_guards_and_with_every_guard_open() {
    let games = [0, 13, 40, 71];
    let (guarded, sites) = recording_sites(|| games.map(game_record));
    let forced = with_scans_forced(|| games.map(game_record));
    for ((game, guarded), forced) in games.iter().zip(&guarded).zip(&forced) {
        assert_eq!(guarded.len(), forced.len(), "game {game} length");
        for (step, (guarded, forced)) in guarded.iter().zip(forced).enumerate() {
            assert_eq!(guarded, forced, "game {game} step {step}");
        }
    }
    assert!(
        sites.values().any(|[_, skipped]| *skipped > 0),
        "the guarded games skipped no scan"
    );
    report("fixture games", &sites);
}

/// A named check of a state, rendered as text.
type StateCheck = (&'static str, fn(&GameState) -> String);

/// Every check that takes only a state and has its own history guard; each
/// is compared directly, so an early failure of one cannot hide another.
fn state_checks() -> Vec<StateCheck> {
    macro_rules! checks {
        ($($check:path),* $(,)?) => {
            vec![$((stringify!($check), (|state: &GameState| format!("{:?}", $check(state))) as fn(&GameState) -> String)),*]
        };
    }
    checks![
        crate::akroma_public_v1::facts,
        crate::arahbo_public_v1::facts,
        crate::arahbo_v1::validate_state,
        crate::ashroot_animist_v1::validate_state,
        crate::ashroot_dispatch_v2::validate_state,
        crate::authority_of_the_consuls_v1::validate_state,
        crate::authority_public_capture_v1::validate_facts,
        crate::authority_public_capture_v1::validate_state,
        crate::banishing_light_v1::validate_state,
        crate::banishing_public_v1::facts,
        crate::bloodthirsty_conqueror_v49::has_history,
        crate::bloodthirsty_conqueror_v49::require_legacy_consumer,
        crate::bloodthirsty_conqueror_v49::custody::validate,
        crate::cerberus_capture_v1::requires_successor,
        crate::cerberus_capture_v1::validate_state,
        crate::conqueror_public_v49::capture,
        crate::controlled_creature_grants_v1::require_legacy_consumer_supported,
        crate::controlled_creature_grants_v1::validate_color_history,
        crate::doubling_season_public_v48::capture,
        crate::doubling_season_v48::requires_successor,
        crate::doubling_season_v48::validate_token_journals,
        crate::eaten_public_v46::validate_state,
        crate::effect::crystal_prevention_v1::requires_successor,
        crate::effect::exile_top_choose_v1::require_legacy_consumer_supported,
        crate::effect::frenzied_goblin_resolution_v1::active,
        crate::effect::inspiration_resolution_v1::validate,
        crate::effect::strongbox_permission_v1::validate,
        crate::engine::mandatory_cast_cost_v1::linked_payment_input,
        crate::engine::mandatory_cast_cost_v1::validate_state,
        crate::equipment_damage_source_v4::validate_state,
        crate::equipment_damage_source_v4::requires_successor,
        crate::equipment_damage_source_v4::public_sources,
        crate::event::validate_exile_if_dies_replacements,
        crate::extra_turn_v1::validate,
        crate::fiendish_panda_v1::validate_state,
        crate::fiendish_panda_v1::custody::validate,
        crate::fiery_annihilation_v1::requires_successor,
        crate::fiery_public_v1::facts_v1,
        crate::fishing_pole_public_v39::facts_v39,
        crate::fixed_source_damage_v3::validate_state,
        crate::garruk_public_v1::facts,
        crate::garruks_uprising_v1::validate_state,
        crate::garruks_uprising_v1::requires_successor,
        crate::gorehorn_damage_v1::crystal::validate,
        crate::gorehorn_damage_v1::public_sources,
        crate::gorehorn_damage_v1::validate_state,
        crate::heraldic_banner_v1::requires_successor,
        crate::heraldic_banner_v1::validate_state,
        crate::high_society_hunter_custody_v45::captures,
        crate::high_society_hunter_public_v45::capture,
        crate::inspiration_observation_v1::validate_projection_authority,
        crate::krenko_public_v1::facts_v1,
        crate::krenko_v1::validate_state,
        crate::krenko_v1::requires_successor,
        crate::librarian_activation_v1::validate_state,
        crate::panda_public_v47::capture,
        crate::raid_entry_custody_v1::validate,
        crate::ravenous_public_v51::capture,
        crate::season_token_attachment_v48::validate_journals,
        crate::soul_counters_v1::has_history,
        crate::source_power_activation_v1::validate_state,
        crate::sower_restriction_capture_v1::active,
        crate::stromkirk_bloodthief_v1::validate_state,
        crate::stromkirk_dispatch_v1::validate_state,
        crate::twinflame_tyrant_v1::requires_successor,
        crate::twinflame_tyrant_v1::combat::validate,
        crate::twinflame_tyrant_v1::fiery::validate,
        crate::twinflame_tyrant_v1::ordering::validate,
        crate::twinflame_tyrant_v1::sampling::validate,
        crate::vampire_gourmand_dispatch_v2::requires_successor,
        crate::wildwood_capture_v1::active,
        crate::zul_ashur_public_v1::facts_v1,
        crate::zul_ashur_v1::validate_state,
    ]
}

/// What `f` returns, or the message it panics with: an added event the state
/// does not account for may make a check panic, and that too must be the
/// same with and without the guards.
fn caught(f: impl FnOnce() -> String) -> String {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)) {
        Ok(outcome) => outcome,
        Err(panic) => {
            let message = panic
                .downcast_ref::<String>()
                .map(String::as_str)
                .or_else(|| panic.downcast_ref::<&str>().copied())
                .unwrap_or("?");
            format!("panicked: {message}")
        }
    }
}

/// What a state's checks return, and where one step from it leads.
fn outcomes(session: &FastActorSessionV1, action: u32, checks: &[StateCheck]) -> Vec<String> {
    let state = session.game_state();
    let mut outcomes: Vec<String> = checks
        .iter()
        .map(|(name, check)| format!("{name}: {}", caught(|| check(state))))
        .collect();
    if let FastActorResponseV1::Decision(d) = session.current_response() {
        outcomes.push(caught(|| {
            let mut next = session.clone();
            let response = next.step(d.episode_id, d.step, action);
            let menu = next.diagnostic_current_action_semantics();
            format!(
                "step: {response:?} {menu:?} {}",
                state_digest(next.game_state())
            )
        }));
    }
    outcomes
}

/// Independently sample early, combat and later decisions from each deal.
fn base_sessions() -> Vec<(FastActorSessionV1, u32)> {
    let mut bases = Vec::new();
    for game in [0, 40] {
        let mut game_bases = Vec::new();
        let mut last_step = 0;
        drive_native(game, |session, decision, _, action| {
            let state = session.game_state();
            let take = match game_bases.len() {
                0 => decision.step >= 10,
                // Only Standard-catalog builds record which creatures
                // attacked this turn; declared attackers stay in combat
                // until the end of combat in every build.
                1 => {
                    state.creatures_attacked_this_turn_v1() > 0
                        || !state.engine.combat.attackers.is_empty()
                }
                _ => decision.step >= last_step + 25,
            };
            if take {
                game_bases.push((session.clone(), action));
                last_step = decision.step;
            }
            game_bases.len() == 3
        });
        assert!(
            game_bases.len() >= 2,
            "game {game} did not supply a combat base"
        );
        bases.extend(game_bases);
    }
    bases
}

#[test]
fn one_event_of_each_kind_changes_no_guarded_result() {
    let samples = every_kind_v1();
    let checks = state_checks();
    let bases = base_sessions();
    let ((), sites) = recording_sites(|| {
        for (base_index, (base, action)) in bases.iter().enumerate() {
            for sample in &samples {
                let mut session = base.clone();
                session
                    .kernel_search_state_mut_for_test_v1()
                    .engine
                    .event_history
                    .push(sample.clone());
                let guarded = outcomes(&session, *action, &checks);
                let forced = with_scans_forced(|| outcomes(&session, *action, &checks));
                for (guarded, forced) in guarded.iter().zip(&forced) {
                    assert_eq!(
                        guarded,
                        forced,
                        "base {base_index}, one {:?} event added",
                        sample.kind_v1()
                    );
                }
                assert_eq!(guarded.len(), forced.len());
            }
        }
    });
    report("one event of each kind", &sites);
}
