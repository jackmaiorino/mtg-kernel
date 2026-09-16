//! Opt-in real-engine BO3 data collection. No optimizer or learning objective
//! is changed here. Only naturally complete first-to-two matches have targets.

use crate::bo3_match::{GameOutcomeV1, MatchPhaseV1, PlayDrawChoiceV1};
use crate::bo3_session::BestOfThreeDeckMatchV1;
use crate::game_summary_v1::{
    GameSummaryV1, RemovalCounterspellTagsV1, try_run_fast_episode_with_summary_v1,
};
use crate::human_opening_v1::{HumanOpeningV1, HumanOpeningViewV1};
use crate::ids::PlayerId;
use crate::learned_bo3_v1::{Bo3OpeningProtocolV1, project_sideboard_input_v1};
use crate::learned_sideboard_v1::{
    FrozenSideboardEmbeddingsV1, LearnedSideboardModelV1, SideboardActionV1,
    SideboardDeliberationStateV1,
};
use crate::paired_bo1_harness_v1::{
    PairedBo1PolicyInputV1, PairedBo1PolicyV1, paired_policy_seeds_v1,
};
use crate::phase1_agent_v1::*;
use crate::rl::{PlayerSeatV1, TerminalClassificationV1};
use crate::rl_session::{
    FastActorResponseV1, RlSessionError, RlSessionErrorCode, RlSessionTerminalV1,
};
use crate::sideboard::{DeckConfigurationV1, RegisteredDeckV1};
use crate::sideboard_play_policy_v1::FrozenPlayPolicyV1;
use crate::state::SplitMix64;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

pub const BO3_COLLECTION_CONFIG_SCHEMA_V1: &str = "mtg-kernel-bo3-collection-config/v1";
pub const BO3_COLLECTION_RESULT_SCHEMA_V1: &str = "mtg-kernel-bo3-collection-result/v1";
const MAX_RECORD_BYTES: u64 = 256 * 1024 * 1024;
const MAX_RECORDS: u64 = 100_000;
pub const MAX_BO3_COLLECTION_REQUEST_BYTES_V1: usize = 4 * 1024 * 1024;

/// These exact summary tags influence between-game inputs and are part of the
/// replay configuration. Empty sets are explicit, never an implicit tag loader.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Bo3SummaryTagsV1 {
    pub requires_target: BTreeSet<u16>,
    pub is_counterspell: BTreeSet<u16>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Bo3CollectionConfigV1 {
    pub schema: String,
    pub match_id: String,
    pub seed: u64,
    pub initial_chooser: PlayerSeatV1,
    pub deck_ids: [String; 2],
    pub registrations: [OwnDeckConfigurationV1; 2],
    pub summary_tags: Bo3SummaryTagsV1,
    pub max_physical_games: u8,
    pub max_physical_decisions: u64,
    pub max_policy_steps: u64,
    /// Aggregate across both seats and every phase. Counts committed records.
    pub max_decision_records: u64,
    /// Sum of canonical JSON sizes of committed decisions, not a RAM guarantee.
    pub max_decision_json_bytes: u64,
}

/// Typed external request. Parsing validates the bounded JSON shape and
/// duplicate keys, not runtime compatibility or permission to execute it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Bo3CollectionRequestV1 {
    pub config: Bo3CollectionConfigV1,
    pub packages: [CompleteAgentPackageV1; 2],
}

impl Bo3CollectionRequestV1 {
    pub fn from_json_v1(input: &str) -> Result<Self, String> {
        ensure(
            input.len() <= MAX_BO3_COLLECTION_REQUEST_BYTES_V1,
            "BO3 collection request exceeds 4 MiB",
        )?;
        crate::rl::parse_strict_json_value(input).map_err(|error| error.to_string())?;
        serde_json::from_str(input).map_err(|error| error.to_string())
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct Bo3CollectionGameDiagnosticsV1 {
    pub game_index: u8,
    pub environment_seed: Option<u64>,
    pub observed_terminal: Option<RlSessionTerminalV1>,
    /// An attempted selection is not proof that the engine committed its step.
    pub discarded_pending_selections: u64,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Bo3CollectedMatchV1 {
    pub trajectory: Bo3TrainingTrajectoryV1,
    pub trajectory_sha256: String,
    pub committed_decision_records: u64,
    pub committed_decision_json_bytes: u64,
    pub games: Vec<Bo3CollectionGameDiagnosticsV1>,
}

/// Public collection can only produce this result after both executable
/// packages have passed their current-runtime and actual-model loaders.
#[derive(Clone, Debug, Serialize)]
pub struct Bo3CollectionResultV1 {
    pub schema: String,
    pub config: Bo3CollectionConfigV1,
    pub packages: [CompleteAgentPackageV1; 2],
    pub current_runtimes: [CurrentAgentRuntimeV1; 2],
    pub collected: Bo3CollectedMatchV1,
}

/// Complete, bounded match collection with exact current executable/model pins.
/// Invalid inputs fail before gameplay; runtime errors return a validated
/// incomplete prefix. The result is coordinator data, never a policy input.
pub fn collect_bo3_trajectory_v1(
    config: Bo3CollectionConfigV1,
    packages: [CompleteAgentPackageV1; 2],
) -> Result<Bo3CollectionResultV1, String> {
    collect_public_inner(config, packages, None)
}

pub(crate) fn collect_bo3_with_native_capture_v1(
    config: Bo3CollectionConfigV1,
    packages: [CompleteAgentPackageV1; 2],
    capture: &mut crate::phase1_bo3_learning_v1::CaptureBuffer,
) -> Result<Bo3CollectionResultV1, String> {
    collect_public_inner(config, packages, Some(capture))
}

fn collect_public_inner(
    config: Bo3CollectionConfigV1,
    packages: [CompleteAgentPackageV1; 2],
    capture: Option<&mut crate::phase1_bo3_learning_v1::CaptureBuffer>,
) -> Result<Bo3CollectionResultV1, String> {
    validate_configuration(&config, packages.each_ref())?;
    let [p0, p1] = packages
        .each_ref()
        .map(CompleteAgentPackageV1::load_supported_components_v1);
    let p0 = p0?;
    let p1 = p1?;
    let mut policies = [p0.gameplay, p1.gameplay];
    let heads = [p0.sideboard, p1.sideboard];
    let collected = collect_loaded_inner(
        &config,
        packages.each_ref(),
        &mut policies,
        heads.each_ref().map(Option::as_ref),
        capture,
    )?;
    Ok(Bo3CollectionResultV1 {
        schema: BO3_COLLECTION_RESULT_SCHEMA_V1.into(),
        config,
        packages,
        current_runtimes: [p0.current_runtime, p1.current_runtime],
        collected,
    })
}

fn seat(actor: PlayerSeatV1) -> usize {
    if actor == PlayerSeatV1::P0 { 0 } else { 1 }
}
fn player(actor: PlayerSeatV1) -> PlayerId {
    PlayerId(seat(actor) as u8)
}
fn own(config: &DeckConfigurationV1) -> OwnDeckConfigurationV1 {
    OwnDeckConfigurationV1 {
        mainboard: config.mainboard().to_vec(),
        sideboard: config.sideboard().to_vec(),
    }
}
fn ensure(condition: bool, message: &str) -> Result<(), String> {
    if condition {
        Ok(())
    } else {
        Err(message.into())
    }
}

fn registrations(config: &Bo3CollectionConfigV1) -> Result<[RegisteredDeckV1; 2], String> {
    let [p0, p1] = [0, 1].map(|i| {
        RegisteredDeckV1::new_executable_v1(
            config.deck_ids[i].clone(),
            config.registrations[i].mainboard.clone(),
            config.registrations[i].sideboard.clone(),
        )
        .map_err(|error| error.to_string())
    });
    Ok([p0?, p1?])
}

pub(crate) fn validate_configuration(
    config: &Bo3CollectionConfigV1,
    packages: [&CompleteAgentPackageV1; 2],
) -> Result<(), String> {
    ensure(
        config.schema == BO3_COLLECTION_CONFIG_SCHEMA_V1,
        "BO3 collection config schema differs",
    )?;
    ensure(
        (3..=254).contains(&config.max_physical_games)
            && config.max_physical_decisions > 0
            && config.max_policy_steps > 0,
        "BO3 collection requires positive episode limits and 3..=254 physical games (reserve the match counter's successor)",
    )?;
    ensure(
        (1..=MAX_RECORDS).contains(&config.max_decision_records)
            && (1..=MAX_RECORD_BYTES).contains(&config.max_decision_json_bytes),
        "BO3 recording limits exceed the bounded collector",
    )?;
    for id in &config.summary_tags.requires_target | &config.summary_tags.is_counterspell {
        ensure(
            (id as usize) < crate::card_def::CARD_DEFS.len(),
            "summary tag is outside the compiled card registry",
        )?;
    }
    registrations(config)?;
    for package in packages {
        package.validate_metadata_v1()?;
        ensure(
            matches!(
                package.opening,
                AgentOpeningPolicyV1::Existing {
                    protocol: Bo3OpeningProtocolV1::KeepSevenV2
                }
            ) && matches!(package.play_draw, AgentPlayDrawPolicyV1::Fixed { .. })
                && matches!(package.search, AgentSearchPolicyV1::Disabled),
            "BO3 collector supports only KeepSevenV2, fixed play/draw and disabled search",
        )?;
    }
    let empty = new_trajectory(config, packages)?;
    empty.validate_v1(packages)?;
    Ok(())
}

fn new_trajectory(
    config: &Bo3CollectionConfigV1,
    packages: [&CompleteAgentPackageV1; 2],
) -> Result<Bo3TrainingTrajectoryV1, String> {
    Ok(Bo3TrainingTrajectoryV1 {
        schema: BO3_TRAINING_TRAJECTORY_SCHEMA_V1.into(),
        match_id: config.match_id.clone(),
        initial_chooser: config.initial_chooser,
        behavior_packages_by_seat: [
            packages[0].package_sha256_v1()?,
            packages[1].package_sha256_v1()?,
        ],
        registrations_by_seat: config.registrations.clone(),
        games: Vec::new(),
        ending: Bo3TrajectoryEndingV1::Incomplete {
            reason: IncompleteMatchReasonV1::Interrupted,
        },
    })
}

struct Stop {
    reason: IncompleteMatchReasonV1,
    message: String,
}
impl From<String> for Stop {
    fn from(message: String) -> Self {
        Self {
            reason: IncompleteMatchReasonV1::EngineError,
            message,
        }
    }
}

struct RecordBudget {
    count: u64,
    bytes: u64,
    max_count: u64,
    max_bytes: u64,
}
impl RecordBudget {
    fn check(&self, record: &Bo3DecisionRecordV1) -> Result<u64, Stop> {
        let size = serde_json::to_vec(record)
            .map_err(|e| Stop::from(e.to_string()))?
            .len() as u64;
        if self.count >= self.max_count || self.bytes.saturating_add(size) > self.max_bytes {
            return Err(Stop {
                reason: IncompleteMatchReasonV1::DecisionCap,
                message: "BO3 committed decision recording limit reached".into(),
            });
        }
        Ok(size)
    }
    fn append_checked(
        &mut self,
        game: &mut Bo3TrainingGameV1,
        record: Bo3DecisionRecordV1,
        size: u64,
    ) {
        debug_assert_eq!(record.decision_index, self.count);
        self.count += 1;
        self.bytes += size;
        game.decisions.push(record);
    }
}

fn deterministic(
    budget: &RecordBudget,
    packages: &[String; 2],
    actor: PlayerSeatV1,
    selected_index: u32,
    visible: ActorVisibleDecisionV1,
) -> Bo3DecisionRecordV1 {
    Bo3DecisionRecordV1 {
        decision_index: budget.count,
        actor,
        behavior_package_sha256: packages[seat(actor)].clone(),
        behavior: BehaviorDistributionV1::Deterministic { selected_index },
        visible,
    }
}

/// Private loaded-components boundary. Source tests exercise real engine and
/// actual native models here, without forging a current-runtime certificate.
fn collect_loaded(
    config: &Bo3CollectionConfigV1,
    packages: [&CompleteAgentPackageV1; 2],
    policies: &mut [FrozenPlayPolicyV1; 2],
    heads: [Option<&LearnedSideboardModelV1>; 2],
) -> Result<Bo3CollectedMatchV1, String> {
    collect_loaded_inner(config, packages, policies, heads, None)
}

pub(crate) fn collect_loaded_inner(
    config: &Bo3CollectionConfigV1,
    packages: [&CompleteAgentPackageV1; 2],
    policies: &mut [FrozenPlayPolicyV1; 2],
    heads: [Option<&LearnedSideboardModelV1>; 2],
    mut capture: Option<&mut crate::phase1_bo3_learning_v1::CaptureBuffer>,
) -> Result<Bo3CollectedMatchV1, String> {
    validate_configuration(config, packages)?;
    // Strict whole-generation equality between the two seats, not just the
    // per-seat wide-vs-narrow boolean below: two different wide generations
    // (V3 and V4) both report `true` for `uses_observation_successor_v3`, so
    // comparing only that boolean per seat would silently accept a mixed
    // V3/V4 pairing across seats.
    ensure(
        policies[0].feature_generation_v1() == policies[1].feature_generation_v1(),
        "installed BO3 gameplay differs from the behavior package",
    )?;
    for i in 0..2 {
        ensure(
            policies[i].uses_observation_successor_v3()
                && policies[i].actual_model_identity_v1() == packages[i].gameplay.identity.model
                && policies[i].identity_v1() == &packages[i].gameplay.identity.source_import
                && policies[i].runtime_sampler_identity_v1()
                    == packages[i].gameplay_sampler_identity,
            "installed BO3 gameplay differs from the behavior package",
        )?;
        match (&packages[i].sideboard, heads[i]) {
            (AgentSideboardPolicyV1::Keep, None) => {}
            (
                AgentSideboardPolicyV1::LearnedGreedyV1 {
                    play_identity,
                    embedding_table_sha256,
                    ..
                },
                Some(head),
            ) => {
                let embeddings = FrozenSideboardEmbeddingsV1::new_v1(
                    policies[i].embedding_rows_v1(),
                    play_identity.clone(),
                )
                .map_err(|e| e.to_string())?;
                head.validate_frozen_embeddings_v1(&embeddings)
                    .map_err(|e| e.to_string())?;
                // The public package loader verified the original file bytes.
                // Canonical reserialization is not necessarily the file's hash.
                ensure(
                    head.play_identity_v1() == play_identity
                        && embeddings.table_sha256_v1() == embedding_table_sha256,
                    "installed BO3 sideboard head differs from the behavior package",
                )?;
            }
            _ => return Err("BO3 sideboard descriptor/component mismatch".into()),
        }
    }
    let mut trajectory = new_trajectory(config, packages)?;
    let mut match_session =
        BestOfThreeDeckMatchV1::new_live_v1(registrations(config)?, player(config.initial_chooser))
            .map_err(|e| e.to_string())?;
    let registered = [PlayerId::P0, PlayerId::P1].map(|p| {
        match_session
            .registered_deck(p)
            .unwrap()
            .registered_configuration()
            .clone()
    });
    let mut current = registered.clone();
    let mut summaries: [Vec<GameSummaryV1>; 2] = [Vec::new(), Vec::new()];
    let mut diagnostics = Vec::new();
    let mut seed_stream = SplitMix64::seed(config.seed);
    let mut budget = RecordBudget {
        count: 0,
        bytes: 0,
        max_count: config.max_decision_records,
        max_bytes: config.max_decision_json_bytes,
    };
    let tags = RemovalCounterspellTagsV1 {
        requires_target: config.summary_tags.requires_target.clone(),
        is_counterspell: config.summary_tags.is_counterspell.clone(),
    };
    loop {
        let (game_index, chooser) = match match_session.match_state().phase() {
            MatchPhaseV1::Complete { outcome } => {
                trajectory.ending = Bo3TrajectoryEndingV1::Complete { outcome };
                break;
            }
            MatchPhaseV1::AwaitingPlayDrawChoice {
                game_index,
                chooser,
            } => (game_index, chooser),
            _ => return Err("BO3 coordinator unexpectedly awaits a game result".into()),
        };
        if game_index > config.max_physical_games {
            trajectory.ending = Bo3TrajectoryEndingV1::Incomplete {
                reason: IncompleteMatchReasonV1::PhysicalGameCap,
            };
            break;
        }
        let mut game = Bo3TrainingGameV1 {
            game_index,
            start: None,
            decisions: Vec::new(),
            terminal: None,
        };
        let mut diagnostic = Bo3CollectionGameDiagnosticsV1 {
            game_index,
            environment_seed: None,
            observed_terminal: None,
            discarded_pending_selections: 0,
            error: None,
        };
        let played = play_game(
            config,
            packages,
            &trajectory.behavior_packages_by_seat,
            policies,
            heads,
            &mut match_session,
            &registered,
            &mut current,
            &summaries,
            &tags,
            &mut seed_stream,
            chooser,
            &mut game,
            &mut budget,
            &mut diagnostic,
            capture.as_deref_mut(),
        );
        trajectory.games.push(game);
        match played {
            Ok(summary) => {
                let outcome = summary
                    .winner
                    .map_or(GameOutcomeV1::Draw, |winner| GameOutcomeV1::Win { winner });
                let mut other = summary.clone();
                other.checkpoint_weights_hash =
                    packages[1].gameplay.identity.model.weights_sha256.clone();
                summaries[0].push(summary);
                summaries[1].push(other);
                match_session
                    .record_game_result_v1(outcome)
                    .map_err(|e| e.to_string())?;
                diagnostics.push(diagnostic);
            }
            Err(stop) => {
                diagnostic.error = Some(stop.message);
                diagnostics.push(diagnostic);
                trajectory.ending = Bo3TrajectoryEndingV1::Incomplete {
                    reason: stop.reason,
                };
                break;
            }
        }
    }
    trajectory.validate_v1(packages)?;
    let trajectory_sha256 = format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&trajectory).map_err(|e| e.to_string())?)
    );
    Ok(Bo3CollectedMatchV1 {
        trajectory,
        trajectory_sha256,
        committed_decision_records: budget.count,
        committed_decision_json_bytes: budget.bytes,
        games: diagnostics,
    })
}

#[allow(clippy::too_many_arguments)]
fn play_game(
    config: &Bo3CollectionConfigV1,
    packages: [&CompleteAgentPackageV1; 2],
    hashes: &[String; 2],
    policies: &mut [FrozenPlayPolicyV1; 2],
    heads: [Option<&LearnedSideboardModelV1>; 2],
    match_session: &mut BestOfThreeDeckMatchV1,
    registered: &[DeckConfigurationV1; 2],
    current: &mut [DeckConfigurationV1; 2],
    summaries: &[Vec<GameSummaryV1>; 2],
    tags: &RemovalCounterspellTagsV1,
    seed_stream: &mut SplitMix64,
    chooser: PlayerId,
    game: &mut Bo3TrainingGameV1,
    budget: &mut RecordBudget,
    diagnostic: &mut Bo3CollectionGameDiagnosticsV1,
    capture: Option<&mut crate::phase1_bo3_learning_v1::CaptureBuffer>,
) -> Result<GameSummaryV1, Stop> {
    let wins = [PlayerId::P0, PlayerId::P1].map(|p| match_session.match_state().wins(p).unwrap());
    if game.game_index > 1 {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let i = actor.index();
            let input = project_sideboard_input_v1(
                &registered[i],
                actor,
                &summaries[i],
                game.game_index,
                wins,
            )?;
            let mut deliberation = SideboardDeliberationStateV1::new_v1(&current[i]);
            let embeddings = heads[i]
                .map(|head| {
                    FrozenSideboardEmbeddingsV1::new_v1(
                        policies[i].embedding_rows_v1(),
                        head.play_identity_v1().clone(),
                    )
                })
                .transpose()
                .map_err(|e| e.to_string())?;
            while !deliberation.is_done_v1() {
                let actions = deliberation.legal_actions_v1();
                let selected = if let Some(head) = heads[i] {
                    head.score_v1(&input, &deliberation, embeddings.as_ref().unwrap())
                        .map_err(|e| e.to_string())?
                        .selected_action
                } else {
                    SideboardActionV1::Done
                };
                let selected_index = actions
                    .iter()
                    .position(|a| *a == selected)
                    .ok_or_else(|| "sideboard head selected an unavailable action".to_owned())?
                    as u32;
                let record = deterministic(
                    budget,
                    hashes,
                    actor.into(),
                    selected_index,
                    ActorVisibleDecisionV1::Sideboard {
                        input: input.clone(),
                        ordered_actions: actions,
                    },
                );
                let size = budget.check(&record)?;
                deliberation.apply_v1(selected).map_err(|e| e.to_string())?;
                budget.append_checked(game, record, size);
            }
            current[i] = deliberation.configuration_v1().map_err(|e| e.to_string())?;
        }
    }
    let AgentPlayDrawPolicyV1::Fixed { choice } = packages[chooser.index()].play_draw else {
        unreachable!("validated fixed policy")
    };
    let record = deterministic(
        budget,
        hashes,
        chooser.into(),
        if choice == PlayDrawChoiceV1::Play {
            0
        } else {
            1
        },
        ActorVisibleDecisionV1::PlayDraw {
            own_configuration: own(&current[chooser.index()]),
            own_games_won: wins[chooser.index()],
            opponent_games_won: wins[chooser.opponent().index()],
            ordered_choices: vec![PlayDrawChoiceV1::Play, PlayDrawChoiceV1::Draw],
        },
    );
    let size = budget.check(&record)?;
    let prepared = match_session
        .prepare_game_with_configurations_v1(chooser, choice, current.clone())
        .map_err(|e| e.to_string())?;
    game.start = Some(prepared.start());
    budget.append_checked(game, record, size);
    let environment_seed = seed_stream.next_u64();
    diagnostic.environment_seed = Some(environment_seed);
    let mut opening = HumanOpeningV1::new(
        u64::from(game.game_index),
        environment_seed,
        config.max_physical_decisions,
        config.max_policy_steps,
        config.deck_ids.clone(),
        current.each_ref().map(|c| c.mainboard().to_vec()),
        prepared.start().starting_player,
        PlayerId::P0,
    )?;
    // The existing protocol automatically keeps P1, then explicitly keeps P0.
    let automatic = opening.automatic_keep_seven_view_v1()?;
    let record = opening_record(budget, hashes, &current[1], automatic)?;
    let size = budget.check(&record)?;
    budget.append_checked(game, record, size);
    let record = opening_record(budget, hashes, &current[0], opening.view())?;
    let size = budget.check(&record)?;
    opening.keep()?;
    budget.append_checked(game, record, size);
    let mut episode = opening.into_session()?;
    let mut recorder = RecordingPolicy {
        policies,
        hashes,
        game,
        budget,
        pending: None,
        recording_cap: false,
        rejected_selections: 0,
        capture,
    };
    recorder
        .reset_for_game_v1(paired_policy_seeds_v1(environment_seed))
        .map_err(|e| e.to_string())?;
    let played = try_run_fast_episode_with_summary_v1(
        &mut episode,
        &packages[0].gameplay.identity.model.weights_sha256,
        tags,
        &mut recorder,
    );
    if let FastActorResponseV1::Terminal(terminal) = episode.current_response() {
        diagnostic.observed_terminal = Some(terminal);
    }
    recorder.finish_game(played, diagnostic)
}

fn opening_record(
    budget: &RecordBudget,
    hashes: &[String; 2],
    current: &DeckConfigurationV1,
    view: HumanOpeningViewV1,
) -> Result<Bo3DecisionRecordV1, String> {
    Ok(deterministic(
        budget,
        hashes,
        view.human_seat,
        0,
        ActorVisibleDecisionV1::Mulligan {
            input: ActorOpeningInputV1 {
                own_configuration: own(current),
                own_hand: view.hand.iter().map(|c| c.card_id).collect(),
                mulligans_taken: view.mulligans_taken,
                remaining_bottom: view.required_bottom,
                starting_player: view.starting_player,
                opponent_hand_count: u8::try_from(view.opponent_hand_count)
                    .map_err(|e| e.to_string())?,
                opponent_has_kept: view.opponent_has_kept,
            },
            ordered_choices: vec![MulliganChoiceV1::Keep, MulliganChoiceV1::Mulligan],
        },
    ))
}

struct Pending {
    step: u64,
    record: Bo3DecisionRecordV1,
    json_size: u64,
    native: Option<crate::phase1_bo3_learning_v1::PendingNativeCapture>,
}
struct RecordingPolicy<'a> {
    policies: &'a mut [FrozenPlayPolicyV1; 2],
    hashes: &'a [String; 2],
    game: &'a mut Bo3TrainingGameV1,
    budget: &'a mut RecordBudget,
    pending: Option<Pending>,
    recording_cap: bool,
    rejected_selections: u64,
    capture: Option<&'a mut crate::phase1_bo3_learning_v1::CaptureBuffer>,
}
impl RecordingPolicy<'_> {
    fn finish_game(
        &mut self,
        played: Result<GameSummaryV1, String>,
        diagnostic: &mut Bo3CollectionGameDiagnosticsV1,
    ) -> Result<GameSummaryV1, Stop> {
        match played {
            Ok(summary) => {
                let accepted = (|| -> Result<GameSummaryV1, Stop> {
                    let terminal = diagnostic
                        .observed_terminal
                        .as_ref()
                        .ok_or_else(|| "successful game summary lacks a terminal".to_owned())?;
                    ensure(
                        terminal.terminal_classification == TerminalClassificationV1::Natural,
                        "successful game summary is not natural",
                    )?;
                    ensure(
                        summary.winner.map(PlayerSeatV1::from) == terminal.winner,
                        "natural summary and actual terminal winners differ",
                    )?;
                    self.commit_pending(terminal.policy_step_count)?;
                    self.game.terminal = Some(Bo3GameTerminalV1 {
                        classification: terminal.terminal_classification,
                        outcome: terminal.terminal_outcome,
                        gameplay_decision_count: terminal.policy_step_count,
                    });
                    Ok(summary)
                })();
                if accepted.is_err() {
                    diagnostic.discarded_pending_selections =
                        u64::from(self.pending.take().is_some()) + self.rejected_selections;
                }
                accepted
            }
            Err(message) => {
                // An error gives no successful-step receipt. Even an incremented
                // counter does not promote the last attempted action into a commit.
                diagnostic.discarded_pending_selections =
                    u64::from(self.pending.take().is_some()) + self.rejected_selections;
                let reason = if self.recording_cap
                    || diagnostic.observed_terminal.as_ref().is_some_and(|t| {
                        t.terminal_classification == TerminalClassificationV1::Truncated
                    }) {
                    IncompleteMatchReasonV1::DecisionCap
                } else {
                    IncompleteMatchReasonV1::EngineError
                };
                Err(Stop { reason, message })
            }
        }
    }

    fn commit_pending(&mut self, next_step: u64) -> Result<(), String> {
        if let Some(pending) = &self.pending {
            ensure(
                pending.step.checked_add(1) == Some(next_step),
                "BO3 callback did not confirm the previous gameplay step",
            )?;
        } else {
            ensure(
                next_step == 0,
                "BO3 callback skipped an unrecorded gameplay step",
            )?;
        }
        if let Some(pending) = self.pending.take() {
            self.budget
                .append_checked(self.game, pending.record, pending.json_size);
            if let Some(native) = pending.native {
                self.capture
                    .as_deref_mut()
                    .expect("native pending requires capture sink")
                    .commit(native);
            }
        }
        Ok(())
    }
}
fn recording_error(message: String) -> RlSessionError {
    RlSessionError {
        code: RlSessionErrorCode::StaleEnvironmentBinding,
        message: format!("BO3 trajectory recorder: {message}"),
    }
}
impl PairedBo1PolicyV1 for RecordingPolicy<'_> {
    fn uses_observation_successor_v3(&self) -> bool {
        true
    }
    fn reset_for_game_v1(&mut self, seeds: [u64; 2]) -> Result<(), RlSessionError> {
        for policy in &mut *self.policies {
            policy.reset_for_game_v1(seeds)?;
        }
        Ok(())
    }
    fn select_action_v1(
        &mut self,
        input: PairedBo1PolicyInputV1<'_>,
    ) -> Result<u32, RlSessionError> {
        let decision = input.decision();
        self.commit_pending(decision.step)
            .map_err(recording_error)?;
        let (selected, scores) =
            self.policies[seat(decision.acting_player)].select_paired_with_scores_v1(&input)?;
        self.rejected_selections += 1;
        let behavior = BehaviorDistributionV1::hamilton_from_logits_v1(&scores.logits, selected)
            .map_err(recording_error)?;
        let record = input
            .capture_bo3_gameplay_v1(
                self.budget.count,
                self.hashes[seat(decision.acting_player)].clone(),
                behavior,
            )
            .map_err(recording_error)?;
        let size = match self.budget.check(&record) {
            Ok(size) => size,
            Err(stop) => {
                self.recording_cap = stop.reason == IncompleteMatchReasonV1::DecisionCap;
                return Err(recording_error(stop.message));
            }
        };
        let native = if let Some(capture) = self.capture.as_deref() {
            let tensor = self.policies[seat(decision.acting_player)]
                .last_scored_training_tensor_v3()
                .map_err(recording_error)?;
            match capture.prepare(
                record.decision_index,
                tensor,
                &scores,
                decision.legal_action_count as usize,
            ) {
                Ok(native) => Some(native),
                Err(message) => {
                    self.recording_cap = true;
                    return Err(recording_error(message));
                }
            }
        } else {
            None
        };
        self.pending = Some(Pending {
            step: decision.step,
            record,
            json_size: size,
            native,
        });
        self.rejected_selections -= 1;
        Ok(selected)
    }
}

#[cfg(test)]
pub(crate) mod tests;
