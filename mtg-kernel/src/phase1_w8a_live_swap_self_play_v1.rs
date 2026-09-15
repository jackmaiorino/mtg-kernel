//! W8a self-play driver skeleton (deck-model spec section 8, ruling 9).
//!
//! Plays N BO3 matches between a learner seat, whose sideboard swaps come
//! from the deck model's sampled entry (`learned_sideboard_v1::sample_v1`
//! and its `deliberate_sampled_v1` sibling), and a fixed opponent whose
//! swaps stay on the match's own installed static/Keep policy, connecting
//! them through `bo3_session::prepare_game_with_live_policies_v1` (W8a's
//! live-swap match path). Each match's terminal `MatchOutcomeV1` is recorded
//! as the sole reward (ruling 11: terminal match win/loss, never a batched
//! delta). Per-decision records reuse `phase1_agent_v1`'s exact
//! `Bo3DecisionRecordV1`/`ActorVisibleDecisionV1`/`BehaviorDistributionV1`
//! vocabulary so a later `phase1_bo3_learning_v1` capture extension can fold
//! this receipt in without a translation layer; this module does not itself
//! wire into that capture path.
//!
//! This is deliberately a *skeleton*: playing one physical game is a
//! caller-supplied [`W8aPhysicalGamePlayerV1`], not a fixed production
//! engine/checkpoint binding, so this module owns no weights, dispatches no
//! training, and is reachable from no CLI or binary. Ruling 10 (whether and
//! when to spend a training cycle on RL refinement) stays open; nothing here
//! trains a model or writes a checkpoint. A real caller would implement
//! [`W8aPhysicalGamePlayerV1`] over `learned_bo3_v1`'s existing
//! `FastActorSessionV1` construction and
//! `game_summary_v1::try_run_fast_episode_with_summary_v1`, exactly as
//! `learned_bo3_v1::run_learned_bo3_session_v1` already does for evaluation.

use crate::bo3_match::{GameOutcomeV1, GameStartV1, MatchOutcomeV1, MatchPhaseV1, PlayDrawChoiceV1};
use crate::bo3_session::{
    BestOfThreeDeckMatchV1, Bo3SessionErrorV1, LiveSideboardConsultationV1,
    LiveSideboardSwapPolicyV1,
};
use crate::game_summary_v1::GameSummaryV1;
use crate::ids::PlayerId;
use crate::learned_bo3_v1::project_sideboard_input_v1;
use crate::learned_sideboard_v1::{
    FrozenSideboardEmbeddingsV1, LearnedSideboardInputV1, LearnedSideboardModelV1,
    SampledSideboardDecisionV1, SideboardActionV1, SIDEBOARD_LIVE_SAMPLER_VERSION_V1,
};
use crate::phase1_agent_v1::{ActorVisibleDecisionV1, BehaviorDistributionV1, Bo3DecisionRecordV1};
use crate::rl::PlayerSeatV1;
use crate::sideboard::{DeckConfigurationV1, DeterministicSideboardPolicyV1, RegisteredDeckV1};
use serde::Serialize;
use sha2::{Digest, Sha256};

pub const W8A_SELF_PLAY_RECEIPT_SCHEMA_V1: &str = "kernel-w8a-live-sideboard-self-play-receipt/v1";

/// A caller-supplied stand-in for "play this one physical game to
/// completion." Receives exactly what a real engine binding needs (the
/// prepared game's `start` and each seat's mainboard) and nothing about the
/// learner's sideboard sampler; a fake implementation in tests can return a
/// scripted `GameSummaryV1` with no engine simulation at all.
pub trait W8aPhysicalGamePlayerV1 {
    fn play_physical_game_v1(
        &mut self,
        start: GameStartV1,
        mainboards: [Vec<u16>; 2],
        environment_seed: u64,
    ) -> Result<GameSummaryV1, String>;
}

/// Deterministic per-match seed, domain-separated from every other seed
/// family in this codebase (`sideboard_search_campaign_v1::candidate_seed_v1`
/// is the sibling precedent this follows). Reproducible from `base_seed` and
/// `match_ordinal` alone.
pub fn w8a_match_seed_v1(base_seed: u64, match_ordinal: u32) -> u64 {
    let mut hasher = Sha256::new();
    hasher.update(b"kernel-w8a-self-play-match-seed/v1");
    hasher.update(base_seed.to_be_bytes());
    hasher.update(match_ordinal.to_be_bytes());
    let digest = hasher.finalize();
    u64::from_be_bytes(digest[0..8].try_into().expect("sha256 digest is at least 8 bytes"))
}

/// Deterministic per-decision sampler seed: domain-separated from the match
/// seed, the physical game index, the seat sampled, and the decision ordinal
/// within that one sideboarding turn (a deliberation is a sequence of
/// several swap decisions, each needing its own seed). Reproducible from
/// these four values alone, matching `w8a_match_seed_v1`'s style.
pub fn w8a_sideboard_sample_seed_v1(
    match_seed: u64,
    game_index: u8,
    seat: PlayerId,
    decision_ordinal: u32,
) -> u64 {
    let mut hasher = Sha256::new();
    hasher.update(b"kernel-w8a-live-sideboard-sample-seed/v1");
    hasher.update(match_seed.to_be_bytes());
    hasher.update([game_index]);
    hasher.update([seat.0]);
    hasher.update(decision_ordinal.to_be_bytes());
    let digest = hasher.finalize();
    u64::from_be_bytes(digest[0..8].try_into().expect("sha256 digest is at least 8 bytes"))
}

/// Adapts the deck model's sampler to the W8a live-policy boundary
/// (`LiveSideboardSwapPolicyV1`). `select_live_v1`'s own signature is the
/// evidence-type enforcement: this adapter, like any implementor, can only
/// ever be called with `LearnedSideboardInputV1`/`DeckConfigurationV1`.
/// Stashes every step's `SampledSideboardDecisionV1` in `decisions` so the
/// driver can read them back after `prepare_game_with_live_policies_v1`
/// returns and the borrow this adapter is passed under ends.
struct SampledSeatPolicyV1<'model, 'table> {
    model: &'model LearnedSideboardModelV1,
    embeddings: &'model FrozenSideboardEmbeddingsV1<'table>,
    match_seed: u64,
    game_index: u8,
    seat: PlayerId,
    decisions: Vec<SampledSideboardDecisionV1>,
}

impl LiveSideboardSwapPolicyV1 for SampledSeatPolicyV1<'_, '_> {
    fn select_live_v1(
        &mut self,
        evidence: &LearnedSideboardInputV1,
        current: &DeckConfigurationV1,
    ) -> Result<(DeckConfigurationV1, Vec<SideboardActionV1>), String> {
        let (match_seed, game_index, seat) = (self.match_seed, self.game_index, self.seat);
        let result = self
            .model
            .deliberate_sampled_v1(evidence, current, self.embeddings, |ordinal| {
                w8a_sideboard_sample_seed_v1(match_seed, game_index, seat, ordinal)
            })
            .map_err(|error| error.to_string())?;
        self.decisions = result.decisions;
        Ok((result.configuration, result.actions))
    }
}

#[derive(Clone, Debug)]
pub struct W8aSelfPlayConfigV1 {
    pub base_seed: u64,
    pub match_count: u32,
    pub learner_seat: PlayerId,
    pub game_one_chooser: PlayerId,
    pub max_physical_games: u8,
}

/// One completed self-play match's sideboard-only decision trace, plus the
/// match's terminal outcome as the RL reward this receipt exists to carry.
/// Gameplay decisions (mulligan, play/draw, in-game actions) are out of this
/// receipt's scope: `phase1_bo3_collection_v1`'s existing "gameplay updates"
/// block already captures those separately (BO3-LEARNING-DESIGN-001 block
/// 1); `decision_index` here is therefore contiguous only across this
/// receipt's own sideboard records, not across the whole match.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct W8aSelfPlayMatchReceiptV1 {
    pub schema: String,
    pub match_ordinal: u32,
    pub match_seed: u64,
    pub learner_seat: PlayerSeatV1,
    pub deck_ids: [String; 2],
    pub sampler_version: &'static str,
    pub game_starts: Vec<GameStartV1>,
    pub sideboard_decisions: Vec<Bo3DecisionRecordV1>,
    pub outcome: MatchOutcomeV1,
}

/// Plays `config.match_count` matches and returns one receipt per completed
/// match. Never trains, never persists a checkpoint, never touches a CLI:
/// the caller owns `learner_model`/`embeddings` (already loaded) and
/// `game_player` (already bound to whatever engine it wraps). A caller that
/// wants to fold this into a real self-play campaign still needs its own,
/// separately ruled dispatch; this function alone authorizes nothing.
pub fn run_w8a_self_play_matches_v1(
    config: &W8aSelfPlayConfigV1,
    registered_decks: [RegisteredDeckV1; 2],
    fixed_sideboard_policy: &DeterministicSideboardPolicyV1,
    learner_model: &LearnedSideboardModelV1,
    embeddings: &FrozenSideboardEmbeddingsV1<'_>,
    game_player: &mut dyn W8aPhysicalGamePlayerV1,
) -> Result<Vec<W8aSelfPlayMatchReceiptV1>, String> {
    if config.max_physical_games < 3 {
        return Err("W8a self-play driver requires at least three physical games".to_owned());
    }
    let mut receipts = Vec::with_capacity(config.match_count as usize);
    for match_ordinal in 0..config.match_count {
        let match_seed = w8a_match_seed_v1(config.base_seed, match_ordinal);
        receipts.push(run_one_self_play_match_v1(
            config,
            match_ordinal,
            match_seed,
            [registered_decks[0].clone(), registered_decks[1].clone()],
            fixed_sideboard_policy,
            learner_model,
            embeddings,
            game_player,
        )?);
    }
    Ok(receipts)
}

#[allow(clippy::too_many_arguments)]
fn run_one_self_play_match_v1(
    config: &W8aSelfPlayConfigV1,
    match_ordinal: u32,
    match_seed: u64,
    registered_decks: [RegisteredDeckV1; 2],
    fixed_sideboard_policy: &DeterministicSideboardPolicyV1,
    learner_model: &LearnedSideboardModelV1,
    embeddings: &FrozenSideboardEmbeddingsV1<'_>,
    game_player: &mut dyn W8aPhysicalGamePlayerV1,
) -> Result<W8aSelfPlayMatchReceiptV1, String> {
    let deck_ids = [
        registered_decks[0].deck_id().to_owned(),
        registered_decks[1].deck_id().to_owned(),
    ];
    let registered_configurations = [
        registered_decks[0].registered_configuration().clone(),
        registered_decks[1].registered_configuration().clone(),
    ];
    let learner_index = config.learner_seat.index();
    let behavior_package_sha256 = learner_model
        .checkpoint_sha256_v1()
        .map_err(|error| error.to_string())?;

    let mut match_session = BestOfThreeDeckMatchV1::new_v1(
        registered_decks[0].clone(),
        registered_decks[1].clone(),
        fixed_sideboard_policy.clone(),
        config.game_one_chooser,
    )
    .map_err(bo3_session_error_to_string)?;
    let mut current = registered_configurations.clone();
    let mut summaries: [Vec<GameSummaryV1>; 2] = [Vec::new(), Vec::new()];
    let mut game_starts = Vec::new();
    let mut sideboard_decisions = Vec::new();
    let mut decision_index = 0u64;

    loop {
        let (game_index, chooser) = match match_session.match_state().phase() {
            MatchPhaseV1::Complete { outcome } => {
                return Ok(W8aSelfPlayMatchReceiptV1 {
                    schema: W8A_SELF_PLAY_RECEIPT_SCHEMA_V1.to_owned(),
                    match_ordinal,
                    match_seed,
                    learner_seat: config.learner_seat.into(),
                    deck_ids,
                    sampler_version: SIDEBOARD_LIVE_SAMPLER_VERSION_V1,
                    game_starts,
                    sideboard_decisions,
                    outcome,
                })
            }
            MatchPhaseV1::AwaitingPlayDrawChoice { game_index, chooser } => (game_index, chooser),
            _ => {
                return Err(
                    "W8a self-play driver: match unexpectedly awaits an unrecorded game result"
                        .to_owned(),
                )
            }
        };
        if game_index > config.max_physical_games {
            return Err(
                "W8a self-play driver: match incomplete at the physical-game limit".to_owned(),
            );
        }
        let prepared = if game_index >= 2 {
            let wins = [
                match_session.match_state().wins(PlayerId::P0).unwrap(),
                match_session.match_state().wins(PlayerId::P1).unwrap(),
            ];
            let evidence = project_sideboard_input_v1(
                &registered_configurations[learner_index],
                config.learner_seat,
                &summaries[learner_index],
                game_index,
                wins,
            )?;
            let mut adapter = SampledSeatPolicyV1 {
                model: learner_model,
                embeddings,
                match_seed,
                game_index,
                seat: config.learner_seat,
                decisions: Vec::new(),
            };
            let mut live: [Option<LiveSideboardConsultationV1<'_>>; 2] = [None, None];
            live[learner_index] = Some(LiveSideboardConsultationV1 {
                policy: &mut adapter,
                evidence: &evidence,
                current: &current[learner_index],
            });
            let result = match_session
                .prepare_game_with_live_policies_v1(chooser, PlayDrawChoiceV1::Play, live)
                .map_err(bo3_session_error_to_string)?;
            for decision in &adapter.decisions {
                let logits_f32: Vec<f32> = decision.logits.iter().map(|&value| value as f32).collect();
                let behavior =
                    BehaviorDistributionV1::hamilton_from_logits_v1(&logits_f32, decision.sampled_index)?;
                sideboard_decisions.push(Bo3DecisionRecordV1 {
                    decision_index,
                    actor: config.learner_seat.into(),
                    behavior_package_sha256: behavior_package_sha256.clone(),
                    behavior,
                    visible: ActorVisibleDecisionV1::Sideboard {
                        input: evidence.clone(),
                        ordered_actions: decision.ordered_actions.clone(),
                    },
                });
                decision_index += 1;
            }
            result.prepared
        } else {
            match_session
                .prepare_game_with_live_policies_v1(chooser, PlayDrawChoiceV1::Play, [None, None])
                .map_err(bo3_session_error_to_string)?
                .prepared
        };
        for seat in [PlayerId::P0, PlayerId::P1] {
            current[seat.index()] = prepared.configuration(seat).unwrap().clone();
        }
        game_starts.push(prepared.start());
        // Deterministic, domain-separated from the sideboard sampler stream
        // above; a real binding would derive this the same way
        // `learned_bo3_v1::run_learned_bo3_session_v1` already does.
        let environment_seed = w8a_environment_seed_v1(match_seed, game_index);
        let mainboards = current.each_ref().map(|configuration| configuration.mainboard().to_vec());
        let summary =
            game_player.play_physical_game_v1(prepared.start(), mainboards, environment_seed)?;
        let outcome = summary
            .winner
            .map_or(GameOutcomeV1::Draw, |winner| GameOutcomeV1::Win { winner });
        summaries[0].push(summary.clone());
        summaries[1].push(summary);
        match_session
            .record_game_result_v1(outcome)
            .map_err(bo3_session_error_to_string)?;
    }
}

fn w8a_environment_seed_v1(match_seed: u64, game_index: u8) -> u64 {
    let mut hasher = Sha256::new();
    hasher.update(b"kernel-w8a-self-play-environment-seed/v1");
    hasher.update(match_seed.to_be_bytes());
    hasher.update([game_index]);
    let digest = hasher.finalize();
    u64::from_be_bytes(digest[0..8].try_into().expect("sha256 digest is at least 8 bytes"))
}

fn bo3_session_error_to_string(error: Bo3SessionErrorV1) -> String {
    error.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game_summary_v1::ResourceCurveV1;
    use crate::sideboard::SideboardDefaultPlanV1;
    use std::collections::{BTreeMap, VecDeque};

    fn alpha_deck() -> RegisteredDeckV1 {
        RegisteredDeckV1::new_exact_v1(
            "W8aAlpha",
            [vec![1; 58], vec![2; 2]].concat(),
            vec![3; 15],
        )
        .unwrap()
    }

    fn beta_deck() -> RegisteredDeckV1 {
        RegisteredDeckV1::new_exact_v1("W8aBeta", vec![9; 60], vec![10; 15]).unwrap()
    }

    fn fixed_policy() -> DeterministicSideboardPolicyV1 {
        DeterministicSideboardPolicyV1::new_v1(
            "w8a-self-play-test-policy",
            vec!["W8aAlpha".to_owned(), "W8aBeta".to_owned()],
            SideboardDefaultPlanV1::KeepRegisteredConfiguration,
            Vec::new(),
        )
        .unwrap()
    }

    fn embeddings_table() -> Vec<f32> {
        // 65,537 rows by 16 columns; row zero (Unknown) must stay all zero.
        let mut table = vec![0.0_f32; 65_537 * 16];
        for card in 0..12u16 {
            table[(usize::from(card) + 1) * 16 + usize::from(card % 16)] = 1.5;
        }
        table
    }

    fn identity() -> crate::learned_sideboard_v1::SideboardPlayIdentityV1 {
        crate::learned_sideboard_v1::SideboardPlayIdentityV1 {
            weights_sha256: "c".repeat(64),
            git_head: "d".repeat(40),
        }
    }

    /// Ignores every input and returns a scripted winner (or draw) per call,
    /// in order. No engine simulation: proves the driver's own state-machine
    /// and receipt-building logic without needing a real checkpoint.
    struct ScriptedGamePlayer {
        outcomes: VecDeque<Option<PlayerId>>,
        calls: Vec<GameStartV1>,
    }

    impl ScriptedGamePlayer {
        fn new(outcomes: &[Option<PlayerId>]) -> Self {
            Self {
                outcomes: outcomes.iter().copied().collect(),
                calls: Vec::new(),
            }
        }
    }

    impl W8aPhysicalGamePlayerV1 for ScriptedGamePlayer {
        fn play_physical_game_v1(
            &mut self,
            start: GameStartV1,
            _mainboards: [Vec<u16>; 2],
            _environment_seed: u64,
        ) -> Result<GameSummaryV1, String> {
            self.calls.push(start);
            let winner = self
                .outcomes
                .pop_front()
                .ok_or_else(|| "scripted outcomes exhausted".to_owned())?;
            Ok(GameSummaryV1 {
                schema_version: 1,
                checkpoint_weights_hash: "a".repeat(64),
                checkpoint_git_head: "b".repeat(40),
                winner,
                opponent_evidence: [Vec::new(), Vec::new()],
                own_card_outcomes: [BTreeMap::new(), BTreeMap::new()],
                resource_curve: ResourceCurveV1::default(),
            })
        }
    }

    #[test]
    fn seed_derivations_are_pure_and_domain_separated() {
        assert_eq!(w8a_match_seed_v1(7, 3), w8a_match_seed_v1(7, 3));
        assert_ne!(w8a_match_seed_v1(7, 3), w8a_match_seed_v1(7, 4));
        assert_ne!(w8a_match_seed_v1(7, 3), w8a_match_seed_v1(8, 3));
        let a = w8a_sideboard_sample_seed_v1(11, 2, PlayerId::P0, 0);
        assert_eq!(a, w8a_sideboard_sample_seed_v1(11, 2, PlayerId::P0, 0));
        assert_ne!(a, w8a_sideboard_sample_seed_v1(11, 2, PlayerId::P0, 1));
        assert_ne!(a, w8a_sideboard_sample_seed_v1(11, 3, PlayerId::P0, 0));
        assert_ne!(a, w8a_sideboard_sample_seed_v1(11, 2, PlayerId::P1, 0));
        assert_ne!(a, w8a_sideboard_sample_seed_v1(12, 2, PlayerId::P0, 0));
    }

    #[test]
    fn self_play_receipts_record_sampled_probabilities_and_terminal_outcome() {
        let table = embeddings_table();
        let embeddings =
            FrozenSideboardEmbeddingsV1::new_v1(&table, identity()).unwrap();
        let model = LearnedSideboardModelV1::new_v1(5, &embeddings);
        let config = W8aSelfPlayConfigV1 {
            base_seed: 4242,
            match_count: 2,
            learner_seat: PlayerId::P0,
            game_one_chooser: PlayerId::P0,
            max_physical_games: 5,
        };
        // Game one: P0 wins (1-0). Game two (one sideboard round): a draw.
        // Game three (a second sideboard round): P0 wins again, completing
        // the match 2-0. `game_player` is reused across both matches
        // (`match_count: 2`), so the script repeats once per match.
        let script = [
            Some(PlayerId::P0),
            None,
            Some(PlayerId::P0),
            Some(PlayerId::P0),
            None,
            Some(PlayerId::P0),
        ];
        let mut game_player = ScriptedGamePlayer::new(&script);
        let receipts = run_w8a_self_play_matches_v1(
            &config,
            [alpha_deck(), beta_deck()],
            &fixed_policy(),
            &model,
            &embeddings,
            &mut game_player,
        )
        .unwrap();

        assert_eq!(receipts.len(), 2);
        for (ordinal, receipt) in receipts.iter().enumerate() {
            assert_eq!(receipt.schema, W8A_SELF_PLAY_RECEIPT_SCHEMA_V1);
            assert_eq!(receipt.match_ordinal, ordinal as u32);
            assert_eq!(receipt.sampler_version, SIDEBOARD_LIVE_SAMPLER_VERSION_V1);
            assert_eq!(receipt.deck_ids, ["W8aAlpha".to_owned(), "W8aBeta".to_owned()]);
            assert_eq!(
                receipt.outcome,
                MatchOutcomeV1::Winner { winner: PlayerId::P0 }
            );
            assert_eq!(receipt.game_starts.len(), 3);
            assert_eq!(receipt.game_starts[0].game_index, 1);
            assert_eq!(receipt.game_starts[0].starting_player, PlayerId::P0);
            assert!(!receipt.sideboard_decisions.is_empty());
            for (index, decision) in receipt.sideboard_decisions.iter().enumerate() {
                assert_eq!(decision.decision_index, index as u64);
                assert_eq!(decision.actor, PlayerSeatV1::from(PlayerId::P0));
                let ActorVisibleDecisionV1::Sideboard { ordered_actions, .. } = &decision.visible
                else {
                    panic!("expected a sideboard decision");
                };
                let probability = decision
                    .behavior
                    .selected_probability_v1(ordered_actions.len())
                    .unwrap();
                assert!(probability > 0.0 && probability <= 1.0);
            }
        }
        // Same base seed, same script: fully deterministic end to end.
        let mut repeat_player = ScriptedGamePlayer::new(&script);
        let repeat = run_w8a_self_play_matches_v1(
            &config,
            [alpha_deck(), beta_deck()],
            &fixed_policy(),
            &model,
            &embeddings,
            &mut repeat_player,
        )
        .unwrap();
        assert_eq!(receipts, repeat);
    }

    #[test]
    fn different_base_seeds_can_change_the_sampled_trace() {
        let table = embeddings_table();
        let embeddings = FrozenSideboardEmbeddingsV1::new_v1(&table, identity()).unwrap();
        let model = LearnedSideboardModelV1::new_v1(5, &embeddings);
        let script = [Some(PlayerId::P0), None, Some(PlayerId::P0)];
        let run = |base_seed: u64| {
            let config = W8aSelfPlayConfigV1 {
                base_seed,
                match_count: 1,
                learner_seat: PlayerId::P0,
                game_one_chooser: PlayerId::P0,
                max_physical_games: 5,
            };
            let mut game_player = ScriptedGamePlayer::new(&script);
            run_w8a_self_play_matches_v1(
                &config,
                [alpha_deck(), beta_deck()],
                &fixed_policy(),
                &model,
                &embeddings,
                &mut game_player,
            )
            .unwrap()
        };
        // Different match seeds are not guaranteed to select a different
        // action every time (a heavily peaked distribution can coincide),
        // but the recorded seeds themselves must differ, proving each run
        // drew from its own domain-separated stream rather than a shared one.
        let a = run(1);
        let b = run(2);
        assert_ne!(a[0].match_seed, b[0].match_seed);
    }
}
