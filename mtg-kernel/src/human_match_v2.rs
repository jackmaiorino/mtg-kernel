//! Native human BO3 using one verified complete-agent package.
//!
//! V1 and existing sessions remain separate. Journals are server-private; only
//! fixed-seat views cross JSONL. This interface performs inference, never fit.

mod replay;
pub use replay::{MAX_RECORDED_REPLAY_INPUT_BYTES_V2, prepare_recorded_human_replay_v2};

use crate::bo3_match::{GameOutcomeV1, MatchOutcomeV1, MatchPhaseV1, PlayDrawChoiceV1};
use crate::bo3_session::BestOfThreeDeckMatchV1;
use crate::card_def::CARD_DEFS;
use crate::expanded_deck_training_v1::{ExpandedDeckListV1, PinnedFileV1};
use crate::game_summary_v1::{
    CompletedGameSummaryV2, FastGameSummaryAccumulatorV1, RemovalCounterspellTagsV1,
    finish_opening_concession_v2,
};
use crate::human_bo3_v1::{HumanActionRequestV1, HumanDecisionErrorV1, HumanDecisionProjectorV1};
pub use crate::human_match_v1::HumanMatchCommandV1;
use crate::human_opening_v1::{HumanOpeningPhaseV1, HumanOpeningV1};
use crate::ids::PlayerId;
use crate::learned_bo3_v1::project_sideboard_input_v1;
use crate::learned_sideboard_v1::{FrozenSideboardEmbeddingsV1, LearnedSideboardModelV1};
use crate::paired_bo1_harness_v1::{
    PairedBo1PolicyInputV1, PairedBo1PolicyV1, paired_policy_seeds_v1,
};
use crate::phase1_agent_v1::{
    AgentPlayDrawPolicyV1, AgentSideboardPolicyV1, CompleteAgentPackageV1,
};
use crate::phase1_bo3_collection_v1::Bo3SummaryTagsV1;
use crate::rl::{PlayerSeatV1, TerminalClassificationV1};
use crate::rl_session::{FastActorResponseV1, FastActorSessionV1};
use crate::sideboard::{DeckConfigurationV1, RegisteredDeckV1};
use crate::sideboard_play_policy_v1::FrozenPlayPolicyV1;
use crate::state::SplitMix64;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs::{File, OpenOptions};
use std::io::{BufRead, Read, Write};
use std::path::{Path, PathBuf};

pub const HUMAN_MATCH_CONFIG_SCHEMA_V2: &str = "mtg-kernel-human-match-config/v2";
pub const HUMAN_MATCH_RESPONSE_SCHEMA_V2: &str = "mtg-kernel-human-match-response/v2";
const MAX_CONFIG_BYTES: u64 = 1024 * 1024;
const MAX_REQUEST_BYTES: usize = 128 * 1024;
const MAX_RESPONSE_BYTES: usize = 1024 * 1024;
const MAX_CACHE_BYTES: usize = 64 * 1024 * 1024;
const MAX_CACHE_ENTRIES: usize = 8192;
fn default_games() -> u8 {
    16
}
fn default_physical() -> u64 {
    100_000
}
fn default_steps() -> u64 {
    200_000
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HumanMatchConfigV2 {
    pub schema: String,
    pub package: PinnedFileV1,
    pub registered: [ExpandedDeckListV1; 2],
    pub human_seat: u8,
    pub initial_chooser: u8,
    pub seed: u64,
    pub summary_tags: Bo3SummaryTagsV1,
    pub journal_path: PathBuf,
    #[serde(default = "default_games")]
    pub max_physical_games: u8,
    #[serde(default = "default_physical")]
    pub max_physical_decisions: u64,
    #[serde(default = "default_steps")]
    pub max_policy_steps: u64,
}

fn strict<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> Result<T, String> {
    let text = std::str::from_utf8(bytes).map_err(|_| "Invalid UTF-8 JSON.".to_owned())?;
    let value =
        crate::rl::parse_strict_json_value(text).map_err(|_| "Invalid strict JSON.".to_owned())?;
    serde_json::from_value(value).map_err(|_| "Invalid JSON fields.".to_owned())
}

fn read_bounded(path: &Path, max: u64) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    File::open(path)
        .map_err(|e| e.to_string())?
        .take(max + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > max {
        return Err("Input exceeds its byte limit.".into());
    }
    Ok(bytes)
}

fn request_id(command: &HumanMatchCommandV1) -> &str {
    match command {
        HumanMatchCommandV1::Current { request_id }
        | HumanMatchCommandV1::Action { request_id, .. }
        | HumanMatchCommandV1::Concede { request_id, .. }
        | HumanMatchCommandV1::Sideboard { request_id, .. }
        | HumanMatchCommandV1::PlayDraw { request_id, .. }
        | HumanMatchCommandV1::Mulligan { request_id, .. }
        | HumanMatchCommandV1::Keep { request_id, .. }
        | HumanMatchCommandV1::Bottom { request_id, .. } => request_id,
    }
}

fn configuration_value(deck: &DeckConfigurationV1) -> Value {
    json!({"mainboard":deck.mainboard(),"sideboard":deck.sideboard()})
}

/// No Serialize/Debug implementation: this owns both seats' private state.
pub struct HumanMatchServiceV2 {
    config: HumanMatchConfigV2,
    package: CompleteAgentPackageV1,
    human: PlayerId,
    policy: FrozenPlayPolicyV1,
    sideboard: Option<LearnedSideboardModelV1>,
    match_state: BestOfThreeDeckMatchV1,
    configurations: [DeckConfigurationV1; 2],
    history: Vec<CompletedGameSummaryV2>,
    session: Option<FastActorSessionV1>,
    opening: Option<HumanOpeningV1>,
    opening_revision: u64,
    projector: HumanDecisionProjectorV1,
    seeds: SplitMix64,
    game_index: u8,
    sideboard_ready: bool,
    last_game: Option<Value>,
    stopped: Option<String>,
    journal: File,
    cache: BTreeMap<String, (HumanMatchCommandV1, Value)>,
    cache_bytes: usize,
    summary: Option<FastGameSummaryAccumulatorV1>,
}

impl HumanMatchServiceV2 {
    pub fn new(config: HumanMatchConfigV2) -> Result<Self, String> {
        Self::validate_config(&config)?;
        let bytes = read_bounded(&config.package.path, MAX_CONFIG_BYTES)?;
        if format!("{:x}", Sha256::digest(&bytes)) != config.package.sha256 {
            return Err("Complete-agent package bytes differ from their pin.".into());
        }
        let package = CompleteAgentPackageV1::from_json_v1(
            std::str::from_utf8(&bytes).map_err(|e| e.to_string())?,
        )?;
        let loaded = package.load_supported_components_v1()?;
        let provenance = json!({"package_sha256":loaded.package_sha256,"current_runtime":loaded.current_runtime});
        Self::construct(
            config,
            package,
            loaded.gameplay,
            loaded.sideboard,
            provenance,
        )
    }

    fn validate_config(config: &HumanMatchConfigV2) -> Result<(), String> {
        if config.schema != HUMAN_MATCH_CONFIG_SCHEMA_V2
            || config.human_seat > 1
            || config.initial_chooser > 1
            || !(1..=16).contains(&config.max_physical_games)
            || !(1..=1_000_000).contains(&config.max_physical_decisions)
            || !(1..=1_000_000).contains(&config.max_policy_steps)
            || !config.package.path.is_absolute()
            || !config.journal_path.is_absolute()
            || config.package.sha256.len() != 64
            || !config.package.sha256.bytes().all(|b| b.is_ascii_hexdigit())
        {
            return Err("Invalid bounded human V2 configuration.".into());
        }
        for &id in config
            .summary_tags
            .requires_target
            .union(&config.summary_tags.is_counterspell)
        {
            if usize::from(id) >= CARD_DEFS.len() {
                return Err("Summary card tag is unavailable.".into());
            }
        }
        for deck in &config.registered {
            RegisteredDeckV1::new_executable_v1(
                &deck.label,
                deck.mainboard.clone(),
                deck.sideboard.clone(),
            )
            .map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    // Only new() and this module's explicit test fixture can reach construction.
    fn construct(
        config: HumanMatchConfigV2,
        package: CompleteAgentPackageV1,
        policy: FrozenPlayPolicyV1,
        sideboard: Option<LearnedSideboardModelV1>,
        provenance: Value,
    ) -> Result<Self, String> {
        Self::validate_config(&config)?;
        let registered: Vec<_> = config
            .registered
            .iter()
            .map(|deck| {
                RegisteredDeckV1::new_executable_v1(
                    &deck.label,
                    deck.mainboard.clone(),
                    deck.sideboard.clone(),
                )
                .map_err(|e| e.to_string())
            })
            .collect::<Result<_, _>>()?;
        let registered: [RegisteredDeckV1; 2] = registered
            .try_into()
            .map_err(|_| "Two registrations required.")?;
        let configurations = registered
            .each_ref()
            .map(|deck| deck.registered_configuration().clone());
        let match_state =
            BestOfThreeDeckMatchV1::new_live_v1(registered, PlayerId(config.initial_chooser))
                .map_err(|e| e.to_string())?;
        if let Some(parent) = config.journal_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let journal = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&config.journal_path)
            .map_err(|e| format!("Fresh human V2 journal required: {e}"))?;
        let human = PlayerId(config.human_seat);
        let seeds = SplitMix64::seed(config.seed);
        let mut result = Self {
            config,
            package,
            human,
            policy,
            sideboard,
            match_state,
            configurations,
            history: Vec::new(),
            session: None,
            opening: None,
            opening_revision: 0,
            projector: HumanDecisionProjectorV1::new(human.into()),
            seeds,
            game_index: 1,
            sideboard_ready: true,
            last_game: None,
            stopped: None,
            journal,
            cache: BTreeMap::new(),
            cache_bytes: 0,
            summary: None,
        };
        result.record("session", &json!({"config":result.config,"package":result.package,"verified":provenance,
            "human_opening":"london","model_opening":"keep_seven_v2","sampler":"paired-seat-streams",
            "human_concessions":"explicit_game_result_not_natural_engine_terminal"}))?;
        result.start_if_model_chooser()?;
        Ok(result)
    }

    fn record(&mut self, event: &str, payload: &Value) -> Result<(), String> {
        let saved = (|| {
            serde_json::to_writer(&mut self.journal, &json!({"event":event,"payload":payload}))
                .map_err(|_| "The private session journal could not be written.")?;
            self.journal
                .write_all(b"\n")
                .map_err(|_| "The private session journal could not be written.")?;
            self.journal
                .sync_data()
                .map_err(|_| "The private session journal could not be saved.")
        })()
        .map_err(str::to_owned);
        if let Err(reason) = &saved {
            self.stopped = Some(reason.clone());
        }
        saved
    }

    fn start_if_model_chooser(&mut self) -> Result<(), String> {
        if !self.sideboard_ready {
            return Ok(());
        }
        if let MatchPhaseV1::AwaitingPlayDrawChoice { chooser, .. } =
            self.match_state.match_state().phase()
        {
            if chooser != self.human {
                let AgentPlayDrawPolicyV1::Fixed { choice } = self.package.play_draw else {
                    return Err("Unsupported model play/draw component.".into());
                };
                self.start_game(chooser, choice)?;
            }
        }
        Ok(())
    }

    fn start_game(&mut self, chooser: PlayerId, choice: PlayDrawChoiceV1) -> Result<(), String> {
        let result = self.prepare_game(chooser, choice);
        if result.is_err() {
            self.stopped =
                Some("The next game could not be initialized; this session is stopped.".into());
        }
        result
    }

    fn prepare_game(&mut self, chooser: PlayerId, choice: PlayDrawChoiceV1) -> Result<(), String> {
        let mut next_match = self.match_state.clone();
        let prepared = next_match
            .prepare_game_with_configurations_v1(chooser, choice, self.configurations.clone())
            .map_err(|_| "These configurations cannot start the next game.".to_owned())?;
        let start = prepared.start();
        if start.game_index > self.config.max_physical_games {
            return Err("The physical-game limit has been reached.".into());
        }
        let mut next_seeds = self.seeds;
        let seed = next_seeds.next_u64();
        let opening = HumanOpeningV1::new(
            u64::from(start.game_index),
            seed,
            self.config.max_physical_decisions,
            self.config.max_policy_steps,
            self.config.registered.each_ref().map(|d| d.label.clone()),
            self.configurations
                .each_ref()
                .map(|d| d.mainboard().to_vec()),
            start.starting_player,
            self.human,
        )
        .map_err(|_| "The next game could not be initialized.".to_owned())?;
        let revision = self
            .opening_revision
            .checked_add(1)
            .ok_or("Opening sequence exhausted.")?;
        self.record("game_start", &json!({"start":start,"environment_seed":seed,"configurations":self.configurations.each_ref().map(configuration_value),
            "model_opening_policy":self.package.opening}))?;
        self.policy.reset_sampling_v1(paired_policy_seeds_v1(seed));
        self.summary = None;
        self.seeds = next_seeds;
        self.match_state = next_match;
        self.game_index = start.game_index;
        self.opening = Some(opening);
        self.opening_revision = revision;
        self.session = None;
        Ok(())
    }

    fn sideboard_choice(
        &self,
        history: &[CompletedGameSummaryV2],
        next_game: u8,
        wins: [u8; 2],
    ) -> Result<(DeckConfigurationV1, Value), String> {
        let actor = self.human.opponent();
        let registered = self
            .match_state
            .registered_deck(actor)
            .ok_or("The model registration is unavailable.")?
            .registered_configuration();
        let summaries: Vec<_> = history.iter().map(|game| game.summary.clone()).collect();
        let input = project_sideboard_input_v1(registered, actor, &summaries, next_game, wins)?;
        let current = &self.configurations[actor.index()];
        if let Some(head) = &self.sideboard {
            let embeddings = FrozenSideboardEmbeddingsV1::new_v1(
                self.policy.embedding_rows_v1(),
                head.play_identity_v1().clone(),
            )
            .map_err(|e| e.to_string())?;
            let selected = head
                .deliberate_v1(&input, current, &embeddings)
                .map_err(|e| e.to_string())?;
            Ok((
                selected.configuration.clone(),
                json!({"input":input,"actions":selected.actions,"initial_value":selected.initial_value,"kind":"learned_greedy_v1"}),
            ))
        } else {
            Ok((
                current.clone(),
                json!({"input":input,"actions":["done"],"kind":"keep"}),
            ))
        }
    }

    fn finish_game(&mut self, summary: CompletedGameSummaryV2, reason: &str) -> Result<(), String> {
        let result = self.commit_game(summary, reason);
        if result.is_err() && self.stopped.is_none() {
            self.stopped = Some("Between-game processing failed; this session is stopped. Recorded game results are retained.".into());
        }
        result
    }

    fn commit_game(&mut self, summary: CompletedGameSummaryV2, reason: &str) -> Result<(), String> {
        let winner = summary.summary.winner;
        let mut next_match = self.match_state.clone();
        let outcome = winner.map_or(GameOutcomeV1::Draw, |winner| GameOutcomeV1::Win { winner });
        next_match
            .record_game_result_v1(outcome)
            .map_err(|_| "The game result could not be recorded.".to_owned())?;
        let public = json!({"game_index":self.game_index,"winner":winner.map_or("draw", |p| if p == self.human {"human"} else {"opponent"}),"reason":reason});
        // A later auxiliary failure cannot erase a valid completed game.
        self.record(
            "game_result",
            &json!({"game_index":self.game_index,"outcome":outcome,"reason":reason,
            "summary":summary,"public_summary":public}),
        )?;
        self.match_state = next_match;
        self.history.push(summary);
        self.session = None;
        self.opening = None;
        self.summary = None;
        self.sideboard_ready = false;
        self.last_game = Some(public);
        if let MatchPhaseV1::AwaitingPlayDrawChoice { game_index, .. } =
            self.match_state.match_state().phase()
        {
            self.game_index = game_index;
            if game_index > self.config.max_physical_games {
                self.stopped = Some("The physical-game limit was reached before a completed match. Completed game results are retained.".into());
                return Ok(());
            }
            let wins = [PlayerId::P0, PlayerId::P1]
                .map(|p| self.match_state.match_state().wins(p).unwrap());
            let (selected, record) = self
                .sideboard_choice(&self.history, game_index, wins)
                .map_err(|_| {
                    "The model could not finish sideboarding; this session is stopped.".to_owned()
                })?;
            // This transaction finishes before any subsequent human sideboard input.
            self.record(
                "model_sideboard_committed",
                &json!({"game_index":game_index,"decision":record,
                "configuration":configuration_value(&selected)}),
            )?;
            self.configurations[self.human.opponent().index()] = selected;
        }
        Ok(())
    }

    fn advance_model(&mut self) -> Result<(), String> {
        loop {
            let Some(session) = self.session.as_ref() else {
                return Ok(());
            };
            match session.current_response() {
                FastActorResponseV1::Decision(decision) => {
                    self.summary
                        .as_mut()
                        .ok_or("The game summary is unavailable.")?
                        .observe_current_v1(session)?;
                    if decision.acting_player == PlayerSeatV1::from(self.human) {
                        return Ok(());
                    }
                    let selected = self
                        .policy
                        .select_action_v1(PairedBo1PolicyInputV1::new(session, decision))
                        .map_err(|_| "The model could not choose an action.".to_owned())?;
                    self.record("model_action", &json!({"game_index":self.game_index,"step":decision.step,"selected":selected}))?;
                    self.session
                        .as_mut()
                        .ok_or("The game session is missing.")?
                        .step(decision.episode_id, decision.step, selected)
                        .map_err(|_| "The engine could not apply the model action.".to_owned())?;
                }
                FastActorResponseV1::Terminal(terminal) => {
                    if terminal.terminal_classification != TerminalClassificationV1::Natural {
                        self.record(
                            "incomplete_game",
                            &serde_json::to_value(&terminal).map_err(|e| e.to_string())?,
                        )?;
                        return Err("The engine stopped this game before a valid result. It does not count as a win or loss.".into());
                    }
                    let tags = self.tags();
                    let summary = self
                        .summary
                        .take()
                        .ok_or("The game summary is unavailable.")?
                        .finish_natural_v1(
                            session,
                            &self.package.gameplay.identity.model.weights_sha256,
                            &tags,
                        )?;
                    self.finish_game(summary, "natural")?;
                    return Ok(());
                }
            }
        }
    }

    fn tags(&self) -> RemovalCounterspellTagsV1 {
        RemovalCounterspellTagsV1 {
            requires_target: self.config.summary_tags.requires_target.clone(),
            is_counterspell: self.config.summary_tags.is_counterspell.clone(),
        }
    }

    fn check_game(&self, game: u8) -> Result<(), String> {
        if game != self.game_index {
            return Err("That command belongs to a different game. Refresh the view.".into());
        }
        Ok(())
    }

    fn mutate(&mut self, command: &HumanMatchCommandV1) -> Result<(), String> {
        if self.stopped.is_some() {
            return Err("This session is stopped. Start a new session to play again.".into());
        }
        match command {
            HumanMatchCommandV1::Current { .. } => Ok(()),
            HumanMatchCommandV1::Action {
                prompt_seq,
                action_index,
                ..
            } => {
                let session = self
                    .session
                    .as_mut()
                    .ok_or("There is no active game decision.")?;
                match self.projector.submit(
                    session,
                    HumanActionRequestV1 {
                        prompt_seq: *prompt_seq,
                        action_index: *action_index,
                    },
                ) {
                    Ok(_) => Ok(()),
                    Err(error) => {
                        if matches!(error, HumanDecisionErrorV1::ExecutionFailed) {
                            self.stopped = Some(error.to_string());
                        }
                        Err(error.to_string())
                    }
                }
            }
            HumanMatchCommandV1::Concede { game_index, .. } => {
                self.check_game(*game_index)?;
                let tags = self.tags();
                let result = if let Some(session) = &self.session {
                    if !matches!(session.current_response(), FastActorResponseV1::Decision(_)) {
                        return Err("There is no active game to concede.".into());
                    }
                    self.summary
                        .take()
                        .ok_or("The game summary is unavailable.")?
                        .finish_concession_v1(
                            session,
                            self.human,
                            &self.package.gameplay.identity.model.weights_sha256,
                            &tags,
                        )
                } else if let Some(opening) = &self.opening {
                    finish_opening_concession_v2(
                        opening,
                        self.human,
                        &self.package.gameplay.identity.model.weights_sha256,
                        &tags,
                    )
                } else {
                    return Err("There is no active game to concede.".into());
                };
                let summary = result.map_err(|_| {
                    self.stopped = Some(
                        "The concession could not be recorded; this session is stopped.".into(),
                    );
                    "The concession could not be recorded; this session is stopped.".to_owned()
                })?;
                self.finish_game(summary, "human_concession")
            }
            HumanMatchCommandV1::Sideboard {
                game_index,
                mainboard,
                sideboard,
                ..
            } => {
                self.check_game(*game_index)?;
                if self.sideboard_ready
                    || self.session.is_some()
                    || self.opening.is_some()
                    || *game_index == 1
                    || !matches!(
                        self.match_state.match_state().phase(),
                        MatchPhaseV1::AwaitingPlayDrawChoice { .. }
                    )
                {
                    return Err("Sideboarding is available once between games.".into());
                }
                let selected =
                    DeckConfigurationV1::new_exact_v1(mainboard.clone(), sideboard.clone())
                        .map_err(|_| {
                            "Submit exactly 60 mainboard cards and 15 sideboard cards.".to_owned()
                        })?;
                let registered = self
                    .match_state
                    .registered_deck(self.human)
                    .ok_or("The human registration is unavailable.")?;
                if selected.combined_card_counts_v1()
                    != registered
                        .registered_configuration()
                        .combined_card_counts_v1()
                {
                    return Err("Sideboarding must preserve your registered 75 cards.".into());
                }
                self.record("human_sideboard_committed", &json!({"game_index":game_index,"configuration":configuration_value(&selected)}))?;
                self.configurations[self.human.index()] = selected;
                self.sideboard_ready = true;
                self.start_if_model_chooser()
            }
            HumanMatchCommandV1::PlayDraw {
                game_index, choice, ..
            } => {
                self.check_game(*game_index)?;
                if !self.sideboard_ready {
                    return Err("Finish sideboarding before choosing play or draw.".into());
                }
                let MatchPhaseV1::AwaitingPlayDrawChoice { chooser, .. } =
                    self.match_state.match_state().phase()
                else {
                    return Err("No play or draw choice is required.".into());
                };
                if chooser != self.human {
                    return Err("The opponent chooses play or draw.".into());
                }
                self.start_game(chooser, *choice)
            }
            HumanMatchCommandV1::Mulligan {
                game_index,
                opening_revision,
                ..
            }
            | HumanMatchCommandV1::Keep {
                game_index,
                opening_revision,
                ..
            }
            | HumanMatchCommandV1::Bottom {
                game_index,
                opening_revision,
                ..
            } => {
                self.check_game(*game_index)?;
                if *opening_revision != self.opening_revision {
                    return Err("That opening prompt is no longer current.".into());
                }
                let revision = self
                    .opening_revision
                    .checked_add(1)
                    .ok_or("Opening sequence exhausted.")?;
                let opening = self
                    .opening
                    .as_mut()
                    .ok_or("The opening hand has already been kept.")?;
                match command {
                    HumanMatchCommandV1::Mulligan { .. } => opening.mulligan(),
                    HumanMatchCommandV1::Keep { .. } => opening.keep(),
                    HumanMatchCommandV1::Bottom { hand_indices, .. } => {
                        opening.bottom(hand_indices)
                    }
                    _ => unreachable!(),
                }
                .map_err(|_| {
                    "That opening choice is not available. Follow the current prompt.".to_owned()
                })?;
                self.opening_revision = revision;
                if opening.view().phase == HumanOpeningPhaseV1::Ready {
                    let ready = self.opening.take().ok_or("The kept hand is unavailable.")?;
                    match ready.into_session() {
                        Ok(session) => {
                            let summary = FastGameSummaryAccumulatorV1::new_v1(&session).map_err(|_| {
                                self.stopped = Some("The game summary could not start; this session is stopped.".into());
                                "The game summary could not start; this session is stopped.".to_owned()
                            })?;
                            self.summary = Some(summary);
                            self.session = Some(session);
                        }
                        Err(_) => {
                            self.stopped =
                                Some("Gameplay could not start from the kept hand.".into());
                            return Err("Gameplay could not start from the kept hand.".into());
                        }
                    }
                }
                Ok(())
            }
        }
    }

    fn view(&mut self) -> Result<Value, String> {
        let wins = [PlayerId::P0, PlayerId::P1]
            .map(|p| self.match_state.match_state().wins(p).unwrap_or(0));
        let notice = match self.package.sideboard {
            AgentSideboardPolicyV1::Keep => {
                "The model keeps seven and its registered sideboard. You may use London mulligans and sideboard between games."
            }
            AgentSideboardPolicyV1::LearnedGreedyV1 { .. } => {
                "The model keeps seven and uses its fitted sideboard policy. You may use London mulligans and sideboard between games."
            }
        };
        let mut view = json!({"game_index":self.game_index,"human_seat":self.human.0,"wins":wins,"notice":notice,"last_game":self.last_game});
        if let Some(reason) = &self.stopped {
            view["phase"] = json!("stopped");
            view["message"] = json!(reason);
            return Ok(view);
        }
        match self.match_state.match_state().phase() {
            MatchPhaseV1::Complete { outcome } => {
                view["phase"] = json!("complete");
                view["winner"] = json!(match outcome {
                    MatchOutcomeV1::Winner { winner } =>
                        if winner == self.human {
                            "human"
                        } else {
                            "opponent"
                        },
                    MatchOutcomeV1::Draw => "draw",
                });
            }
            MatchPhaseV1::AwaitingPlayDrawChoice { .. } => {
                if !self.sideboard_ready {
                    view["phase"] = json!("sideboard");
                    view["own_deck"] = self.own_deck();
                } else {
                    view["phase"] = json!("play_draw");
                }
            }
            MatchPhaseV1::AwaitingGameResult { .. } => {
                if let Some(opening) = &self.opening {
                    let opening = opening.view();
                    view["phase"] = serde_json::to_value(opening.phase)
                        .map_err(|_| "Opening display failed.")?;
                    view["opening_revision"] = json!(self.opening_revision);
                    view["opening"] =
                        serde_json::to_value(opening).map_err(|_| "Opening display failed.")?;
                } else {
                    let session = self
                        .session
                        .as_ref()
                        .ok_or("The active game is unavailable.")?;
                    let FastActorResponseV1::Decision(decision) = session.current_response() else {
                        return Err("No current human decision is available.".into());
                    };
                    let decision = self
                        .projector
                        .project_current(session, decision)
                        .map_err(|e| e.to_string())?;
                    view["phase"] = json!("decision");
                    view["decision"] =
                        serde_json::to_value(decision).map_err(|_| "Decision display failed.")?;
                }
            }
        }
        Ok(view)
    }

    fn own_deck(&self) -> Value {
        fn rows(cards: &[u16]) -> Vec<Value> {
            let mut counts = BTreeMap::<u16, u8>::new();
            for &id in cards {
                *counts.entry(id).or_default() += 1;
            }
            counts.into_iter().map(|(card_id, count)| json!({"card_id":card_id,"name":CARD_DEFS[usize::from(card_id)].name,"count":count})).collect()
        }
        let deck = &self.configurations[self.human.index()];
        json!({"label":self.config.registered[self.human.index()].label,"mainboard":rows(deck.mainboard()),"sideboard":rows(deck.sideboard())})
    }

    fn response(&mut self, id: &str, mut error: Option<String>) -> Value {
        let view = self.view().unwrap_or_else(|_| {
            let reason = "The current view is unavailable; the session is stopped.".to_owned();
            self.stopped = Some(reason.clone());
            error = Some(reason.clone());
            json!({"phase":"stopped","message":reason})
        });
        let response = json!({"schema":HUMAN_MATCH_RESPONSE_SCHEMA_V2,"request_id":id,"ok":error.is_none(),"error":error,"view":view});
        if serde_json::to_vec(&response).map_or(true, |bytes| bytes.len() > MAX_RESPONSE_BYTES) {
            self.stopped = Some("The current view exceeds its size limit.".into());
            return json!({"schema":HUMAN_MATCH_RESPONSE_SCHEMA_V2,"request_id":id,"ok":false,"error":"The current view exceeds its size limit; the session is stopped.","view":{"phase":"stopped"}});
        }
        response
    }

    pub fn handle(&mut self, command: HumanMatchCommandV1) -> Value {
        let id = request_id(&command).to_owned();
        if id.is_empty() || id.len() > 128 || !id.bytes().all(|b| b.is_ascii_graphic()) {
            return json!({"schema":HUMAN_MATCH_RESPONSE_SCHEMA_V2,"request_id":"","ok":false,"error":"A short request_id is required."});
        }
        let is_read = matches!(command, HumanMatchCommandV1::Current { .. });
        if !is_read {
            if let Some((previous, response)) = self.cache.get(&id) {
                return if previous == &command {
                    response.clone()
                } else {
                    json!({"schema":HUMAN_MATCH_RESPONSE_SCHEMA_V2,"request_id":id,"ok":false,"error":"That request_id already belongs to a different command."})
                };
            }
            if self.cache.len() >= MAX_CACHE_ENTRIES
                || self.cache_bytes + MAX_RESPONSE_BYTES + MAX_REQUEST_BYTES > MAX_CACHE_BYTES
            {
                self.stopped =
                    Some("The bounded command history is full. Start a new session.".into());
                return self.response(
                    &id,
                    Some("The bounded command history is full. No action was applied.".into()),
                );
            }
        }
        let command_bytes = serde_json::to_vec(&command).unwrap_or_default();
        if command_bytes.len() > MAX_REQUEST_BYTES {
            return self.response(&id, Some("Human command exceeds its size limit.".into()));
        }
        let mutation = if is_read {
            Ok(())
        } else {
            self.record(
                "human_command",
                &serde_json::to_value(&command).unwrap_or(Value::Null),
            )
            .and_then(|_| self.mutate(&command))
        };
        let mut error = mutation.err();
        // Invalid requests never sample, advance an engine state or observe a summary.
        if error.is_none() && self.stopped.is_none() {
            if self.advance_model().is_err() {
                let reason =
                    "The game could not continue safely. This session is stopped.".to_owned();
                self.stopped = Some(reason.clone());
                error = Some(reason);
            }
        }
        let response = self.response(&id, error);
        if !is_read {
            self.cache_bytes += command_bytes.len()
                + serde_json::to_vec(&response).map_or(MAX_RESPONSE_BYTES, |v| v.len());
            self.cache.insert(id, (command, response.clone()));
        }
        response
    }
}

pub fn serve_human_match_v2(
    config_path: &Path,
    mut input: impl BufRead,
    mut output: impl Write,
) -> Result<(), String> {
    let config: HumanMatchConfigV2 = strict(&read_bounded(config_path, MAX_CONFIG_BYTES)?)?;
    let mut service = HumanMatchServiceV2::new(config)?;
    loop {
        let mut line = Vec::new();
        let read = input
            .by_ref()
            .take(MAX_REQUEST_BYTES as u64 + 1)
            .read_until(b'\n', &mut line)
            .map_err(|_| "Could not read human command.")?;
        if read == 0 {
            break;
        }
        let oversized = line.len() > MAX_REQUEST_BYTES;
        let response = if oversized {
            json!({"schema":HUMAN_MATCH_RESPONSE_SCHEMA_V2,"request_id":"","ok":false,"error":"Human command exceeds its size limit."})
        } else {
            match strict::<HumanMatchCommandV1>(&line) {
                Ok(command) => service.handle(command),
                Err(_) => {
                    json!({"schema":HUMAN_MATCH_RESPONSE_SCHEMA_V2,"request_id":"","ok":false,"error":"Invalid human command."})
                }
            }
        };
        serde_json::to_writer(&mut output, &response)
            .map_err(|_| "Could not write human response.")?;
        output
            .write_all(b"\n")
            .map_err(|_| "Could not write human response.")?;
        output
            .flush()
            .map_err(|_| "Could not flush human response.")?;
        if oversized {
            break;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
