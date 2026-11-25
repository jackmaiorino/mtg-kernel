//! Tests of the scoring-call extensions memo (`extensions_memo_v1`).
//!
//! The acceptance test plays whole games and, at every decision, compares the
//! V4 scorer input built with the memo against the input built without it,
//! field by field, and the training tensor and logits bit by bit. The
//! safeguard tests each fail when their safeguard is removed.

use super::extensions_memo_test_controls_v1 as controls;
use super::extensions_memo_v1::{memoized_v1, ExtensionsKeyV1};
use super::{policy_observation_extensions_v6, ObservationTextModeV2};
use crate::flat_policy_v2::*;
use crate::flat_policy_v4::{FlatDecisionEncoderV4, FlatDecisionV4};
use crate::ids::PlayerId;
use crate::rl_session::{FastActorDecisionV1, FastActorResponseV1, FastActorSessionV1};
use crate::sideboard_play_policy_v1::FrozenPlayPolicyV1;

const DECKS: [&str; 8] = [
    "Wildfire", "Rally", "Affinity", "Elves", "Spy", "Burn", "CawGates", "Faeries",
];

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct OwnedV4 {
    objects: Vec<FlatObjectCoreV2>,
    relations: Vec<FlatRelationV2>,
    object_subtypes: Vec<FlatObjectSubtypeV2>,
    ability_uses: Vec<FlatObjectAbilityUseV2>,
    goads: Vec<FlatObjectGoadV2>,
    completed_dungeons: Vec<FlatCompletedDungeonV2>,
    effect_subtype_changes: Vec<FlatEffectSubtypeChangeV2>,
    context_path_elements: Vec<FlatContextPathElementV2>,
    actions: Vec<FlatScorerActionCoreV2>,
    action_refs: Vec<FlatScorerActionRefV2>,
}

fn encode(
    session: &FastActorSessionV1,
    decision: FastActorDecisionV1,
) -> (Result<FlatDecisionV4, FlatDecisionErrorV2>, OwnedV4) {
    let mut owned = OwnedV4::default();
    let result = session.encode_current_flat_scoring_decision_owned_v4(
        decision,
        &mut FlatDecisionEncoderV4::default(),
        &mut FlatScoringOwnedBuffersV2 {
            objects: &mut owned.objects,
            relations: &mut owned.relations,
            object_subtypes: &mut owned.object_subtypes,
            ability_uses: &mut owned.ability_uses,
            goads: &mut owned.goads,
            completed_dungeons: &mut owned.completed_dungeons,
            effect_subtype_changes: &mut owned.effect_subtype_changes,
            context_path_elements: &mut owned.context_path_elements,
            actions: &mut owned.actions,
            action_refs: &mut owned.action_refs,
        },
    );
    (result, owned)
}

fn tensor_and_logits(
    policy: &mut FrozenPlayPolicyV1,
    session: &FastActorSessionV1,
) -> Result<(serde_json::Value, Vec<u32>), String> {
    let scores = policy.score_fast_session_v1(session)?;
    let tensor = policy.last_scored_training_tensor_v4()?;
    Ok((
        serde_json::to_value(crate::agent_bridge_flat_v4_encode::tensor_bits_v1(tensor)).unwrap(),
        scores.logits.iter().map(|x| x.to_bits()).collect(),
    ))
}

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
}

fn games_v1() -> usize {
    std::env::var("MTG_KERNEL_OBS_MEMO_TEST_GAMES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(16)
}

/// Acceptance test: whole games, both seats, eight decks in sixteen
/// pairings by default. Every decision's V4 scorer input, training tensor and
/// logits are equal with the memo, without it, and with the rebuild check on.
#[test]
fn extensions_memo_v1_scorer_inputs_equal_a_rebuild_on_played_games() {
    let mut reference_policy = FrozenPlayPolicyV1::training_fixture_v4();
    let mut memo_policy = FrozenPlayPolicyV1::training_fixture_v4();
    let mut decisions = 0_u64;
    let mut per_seat = [0_u64; 2];
    let mut errors = 0_u64;
    let mut total_hits = 0_u64;
    // Decisions whose extensions are not empty: historical sources (stack
    // abilities, a resolving effect), a pending effect, a library search, a
    // cast in progress, Ward payments; and decisions with a nonempty stack.
    let (mut stack, mut pending, mut library, mut cast, mut ward) = (0, 0, 0, 0, 0);
    let mut stacked = 0_u64;
    let mut terminals = 0_u64;
    for game in 0..games_v1() {
        let first = DECKS[game % DECKS.len()];
        let second = DECKS[(game * 3 + 1 + game / DECKS.len()) % DECKS.len()];
        let mut session = FastActorSessionV1::reset_with_decks_and_limits_flat_action_v3(
            4_100 + game as u64,
            77 + game as u64,
            600,
            60_000,
            [first.to_string(), second.to_string()],
        )
        .unwrap();
        let mut rng = Rng(0x5EED_0000 + game as u64);
        for _ in 0..1_500 {
            let FastActorResponseV1::Decision(decision) = session.current_response() else {
                terminals += 1;
                break;
            };
            let actor =
                crate::kernel_native_search_opponent_v1::player_id_v1(decision.acting_player);
            stacked += u64::from(!session.game_state().stack.is_empty());
            if let Ok(x) = policy_observation_extensions_v6(session.game_state(), actor) {
                stack += u64::from(!x.historical_public_sources.is_empty());
                pending += u64::from(session.game_state().engine.pending_effect.is_some());
                library += u64::from(x.decision_local_library.is_some());
                cast += u64::from(
                    x.pending_cast_object_cost.is_some()
                        || x.pending_chosen_creature_cost.is_some()
                        || !x.finalized_chosen_creature_costs.is_empty(),
                );
                ward += u64::from(
                    x.pending_ward_payment.is_some() || !x.queued_ward_payments.is_empty(),
                );
            }
            controls::set_disabled(true);
            let reference = encode(&session, decision);
            let reference_scored = tensor_and_logits(&mut reference_policy, &session);
            controls::set_disabled(false);
            controls::take_counts();
            let memo = encode(&session, decision);
            let (hits, misses) = controls::take_counts();
            let memo_scored = tensor_and_logits(&mut memo_policy, &session);
            controls::set_check(true);
            let checked = encode(&session, decision);
            controls::set_check(false);
            let at = format!("{first} vs {second}, game {game}, step {}", decision.step);
            assert_eq!(memo, reference, "scorer input with the memo differs: {at}");
            assert_eq!(
                checked, reference,
                "scorer input in check mode differs: {at}"
            );
            assert_eq!(
                memo_scored, reference_scored,
                "tensor or logits differ: {at}"
            );
            if reference.0.is_ok() {
                assert!(
                    hits >= 2 && misses == 1,
                    "one compute must serve the other calls (hits {hits}, misses {misses}): {at}"
                );
                total_hits += hits;
            } else {
                errors += 1;
            }
            decisions += 1;
            per_seat[usize::from(decision.acting_player == super::PlayerSeatV1::P1)] += 1;
            let row = (rng.next() % u64::from(decision.legal_action_count)) as u32;
            session
                .step(decision.episode_id, decision.step, row)
                .unwrap();
        }
    }
    eprintln!(
        "extensions memo: {decisions} decisions (seats {per_seat:?}), {terminals} terminals, \
         {errors} encode errors, {total_hits} memo hits; stack nonempty {stacked}; non-empty \
         extensions: historical sources {stack}, pending effect {pending}, library {library}, \
         cast cost {cast}, ward {ward}"
    );
    if games_v1() >= 16 {
        assert!(decisions >= 3_000, "only {decisions} decisions");
        assert!(per_seat.iter().all(|&n| n >= 1_000), "seats {per_seat:?}");
        assert!(
            stacked > 0 && stack > 0 && pending > 0,
            "games must reach stack and pending-effect states"
        );
    }
}

fn key(actor: PlayerId) -> ExtensionsKeyV1 {
    ExtensionsKeyV1 {
        acting_player: actor,
        text_mode: ObservationTextModeV2::FlatForbiddenElided,
        accept_life_ward: false,
        accept_fishing: false,
        accepted_sources: (false, false, false, false),
        accept_conqueror: false,
        accept_fake: false,
    }
}

fn library_search_state() -> (crate::state::GameState, PlayerId, PlayerId) {
    let state = crate::policy_observation_v6::tests::forest_search_state(false, "Mountain");
    let chooser = [PlayerId::P0, PlayerId::P1]
        .into_iter()
        .find(|&p| {
            policy_observation_extensions_v6(&state, p)
                .unwrap()
                .decision_local_library
                .is_some()
        })
        .expect("fixture has a library search");
    let other = if chooser == PlayerId::P0 {
        PlayerId::P1
    } else {
        PlayerId::P0
    };
    (state, chooser, other)
}

/// Safeguard: the acting player is part of the key. The library search is
/// visible to its chooser only, so a kept chooser value served to the other
/// player would differ from a rebuild.
#[test]
fn extensions_memo_v1_key_separates_actors() {
    let (state, chooser, other) = library_search_state();
    let fresh_other = policy_observation_extensions_v6(&state, other);
    assert!(fresh_other
        .as_ref()
        .unwrap()
        .decision_local_library
        .is_none());
    super::with_extensions_memo_v1(&state, || {
        let kept = policy_observation_extensions_v6(&state, chooser);
        assert!(kept.unwrap().decision_local_library.is_some());
        assert_eq!(policy_observation_extensions_v6(&state, other), fresh_other);
    });
}

/// Safeguard: only the memo's own state object is served. A different state
/// passed inside the memo is computed fresh.
#[test]
fn extensions_memo_v1_serves_only_the_memo_state() {
    let (state, chooser, _) = library_search_state();
    let mut changed = state.clone();
    changed.engine.pending_effect = None;
    let fresh_changed = policy_observation_extensions_v6(&changed, chooser);
    assert_ne!(
        fresh_changed,
        policy_observation_extensions_v6(&state, chooser)
    );
    super::with_extensions_memo_v1(&state, || {
        let _ = policy_observation_extensions_v6(&state, chooser);
        assert_eq!(
            policy_observation_extensions_v6(&changed, chooser),
            fresh_changed
        );
    });
}

/// Safeguard: nothing survives the memo. After it ends the state may change,
/// and the next call sees the change.
#[test]
fn extensions_memo_v1_clears_when_the_memo_ends() {
    let (mut state, chooser, _) = library_search_state();
    super::with_extensions_memo_v1(&state, || {
        let kept = policy_observation_extensions_v6(&state, chooser).unwrap();
        assert!(kept.decision_local_library.is_some());
    });
    state.engine.pending_effect = None;
    let after = policy_observation_extensions_v6(&state, chooser);
    controls::set_disabled(true);
    let rebuilt = policy_observation_extensions_v6(&state, chooser);
    controls::set_disabled(false);
    assert_eq!(after, rebuilt);
}

/// Safeguard: a memo opened on another state inside a memo restores the
/// outer memo when it ends; one opened on the same state joins it.
#[test]
fn extensions_memo_v1_nested_memo_restores_or_joins_the_outer() {
    let (state, chooser, _) = library_search_state();
    let inner_state = state.clone();
    super::with_extensions_memo_v1(&state, || {
        let _ = policy_observation_extensions_v6(&state, chooser);
        super::with_extensions_memo_v1(&inner_state, || {
            let _ = policy_observation_extensions_v6(&inner_state, chooser);
        });
        controls::take_counts();
        let _ = policy_observation_extensions_v6(&state, chooser);
        assert_eq!(
            controls::take_counts(),
            (1, 0),
            "outer entry is served again"
        );
        super::with_extensions_memo_v1(&state, || {
            let _ = policy_observation_extensions_v6(&state, chooser);
        });
        assert_eq!(controls::take_counts(), (1, 0), "same-state memo joins");
    });
}

/// Safeguard: check mode recomputes a hit and refuses a kept value that
/// differs from the rebuild.
#[test]
#[should_panic(expected = "memo differs from a rebuild")]
fn extensions_memo_v1_check_mode_refuses_a_wrong_kept_value() {
    let (state, chooser, _) = library_search_state();
    super::with_extensions_memo_v1(&state, || {
        // Plant a wrong value under the real key, as a missed change would.
        let _ = memoized_v1(&state, key(chooser), || {
            Ok(crate::policy_observation_v6::PolicyObservationExtensionsV6::default())
        });
        controls::set_check(true);
        let _ = policy_observation_extensions_v6(&state, chooser);
    });
}
