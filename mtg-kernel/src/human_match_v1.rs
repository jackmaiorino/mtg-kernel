//! Local human-versus-checkpoint BO3 service.
//!
//! Only fixed-seat human views cross the JSONL boundary. Source pins, model
//! outputs, native bindings and random seeds remain in the server journal.

use crate::bo3_match::{GameOutcomeV1, MatchOutcomeV1, MatchPhaseV1, PlayDrawChoiceV1};
use crate::bo3_session::BestOfThreeDeckMatchV1;
use crate::card_def::CARD_DEFS;
use crate::expanded_deck_training_v1::{
    load_expanded_inference_v1, ExpandedDeckListV1, ExpandedModelSourceV1,
};
use crate::human_bo3_v1::{HumanActionRequestV1, HumanDecisionProjectorV1};
use crate::human_opening_v1::{HumanOpeningPhaseV1, HumanOpeningV1};
use crate::ids::PlayerId;
use crate::paired_bo1_harness_v1::paired_policy_seeds_v1;
use crate::rl::{PlayerSeatV1, TerminalClassificationV1};
use crate::rl_session::{FastActorResponseV1, FastActorSessionV1};
use crate::sideboard::{DeckConfigurationV1, RegisteredDeckV1};
use crate::sideboard_play_policy_v1::FrozenPlayPolicyV1;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::fs::{File, OpenOptions};
use std::io::{BufRead, Read, Write};
use std::path::{Path, PathBuf};

pub const HUMAN_MATCH_CONFIG_SCHEMA_V1: &str = "mtg-kernel-human-match-config/v1";
pub const HUMAN_MATCH_RESPONSE_SCHEMA_V1: &str = "mtg-kernel-human-match-response/v1";
const MAX_CONFIG_BYTES: u64 = 1024 * 1024;
const MAX_REQUEST_BYTES: u64 = 128 * 1024;
const SETUP_NOTICE: &str = "The model keeps its opening seven and its registered sideboard. Learned mulligans and learned sideboarding are not enabled for this human session.";
fn default_physical_limit() -> u64 { 100_000 }
fn default_step_limit() -> u64 { 200_000 }

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HumanMatchConfigV1 {
    pub schema: String,
    pub source: ExpandedModelSourceV1,
    pub registered: [ExpandedDeckListV1; 2],
    pub human_seat: u8,
    pub seed: u64,
    pub starting_player: u8,
    pub journal_path: PathBuf,
    #[serde(default = "default_physical_limit")]
    pub max_physical_decisions: u64,
    #[serde(default = "default_step_limit")]
    pub max_policy_steps: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case", deny_unknown_fields)]
pub enum HumanMatchCommandV1 {
    Current { request_id: String },
    Action { request_id: String, prompt_seq: u64, action_index: u32 },
    Concede { request_id: String, game_index: u8 },
    Sideboard { request_id: String, game_index: u8, mainboard: Vec<u16>, sideboard: Vec<u16> },
    PlayDraw { request_id: String, game_index: u8, choice: PlayDrawChoiceV1 },
    Mulligan { request_id: String, game_index: u8, opening_revision: u64 },
    Keep { request_id: String, game_index: u8, opening_revision: u64 },
    Bottom { request_id: String, game_index: u8, opening_revision: u64, hand_indices: Vec<u32> },
}
impl HumanMatchCommandV1 {
    fn request_id(&self) -> &str {
        match self {
            Self::Current { request_id } | Self::Action { request_id, .. }
            | Self::Concede { request_id, .. } | Self::Sideboard { request_id, .. }
            | Self::PlayDraw { request_id, .. } | Self::Mulligan { request_id, .. }
            | Self::Keep { request_id, .. } | Self::Bottom { request_id, .. } => request_id,
        }
    }
    fn is_read(&self) -> bool { matches!(self, Self::Current { .. }) }
}

// This struct deliberately has no serialization or Debug implementation.
pub struct HumanMatchServiceV1 {
    config: HumanMatchConfigV1,
    human: PlayerId,
    policy: FrozenPlayPolicyV1,
    match_state: BestOfThreeDeckMatchV1,
    configurations: [DeckConfigurationV1; 2],
    session: Option<FastActorSessionV1>,
    opening: Option<HumanOpeningV1>,
    opening_revision: u64,
    projector: HumanDecisionProjectorV1,
    game_index: u8,
    sideboard_ready: bool,
    last_game: Option<Value>,
    stopped: Option<String>,
    journal: File,
    last_mutation: Option<(HumanMatchCommandV1, Value)>,
}

impl HumanMatchServiceV1 {
    pub fn new(config: HumanMatchConfigV1) -> Result<Self, String> {
        if config.schema != HUMAN_MATCH_CONFIG_SCHEMA_V1 || config.human_seat > 1 || config.starting_player > 1 {
            return Err("Invalid human match configuration or player seat.".into());
        }
        if !(1..=1_000_000).contains(&config.max_physical_decisions)
            || !(1..=1_000_000).contains(&config.max_policy_steps) {
            return Err("Human match limits are outside the supported range.".into());
        }
        let registered: Vec<_> = config.registered.iter().map(|deck|
            RegisteredDeckV1::new_executable_v1(&deck.label, deck.mainboard.clone(), deck.sideboard.clone())
                .map_err(|error| error.to_string())).collect::<Result<_, _>>()?;
        let registered: [RegisteredDeckV1; 2] = registered.try_into().map_err(|_| "Expected two registered decks.")?;
        let configurations = registered.each_ref().map(|deck| deck.registered_configuration().clone());
        let match_state = BestOfThreeDeckMatchV1::new_live_v1(registered, PlayerId(config.starting_player))
            .map_err(|error| error.to_string())?;
        let (policy, identity) = load_expanded_inference_v1(&config.source)?;
        if let Some(parent) = config.journal_path.parent() { std::fs::create_dir_all(parent).map_err(|error| error.to_string())?; }
        let journal = OpenOptions::new().create_new(true).write(true).open(&config.journal_path)
            .map_err(|error| format!("Create a fresh human session journal: {error}"))?;
        let human = PlayerId(config.human_seat);
        let mut service = Self {
            config, human, policy, match_state, configurations, session: None, opening: None, opening_revision: 0,
            projector: HumanDecisionProjectorV1::new(human.into()), game_index: 1,
            sideboard_ready: true, last_game: None, stopped: None, journal, last_mutation: None,
        };
        service.record("session", &json!({"config":service.config,"model_identity":identity,"model_play_policy":"sampled-v3-with-paired-seat-streams","model_sideboard":"keep","model_mulligan":"keep_seven","human_mulligan":"london"}))?;
        service.start_game(PlayerId(service.config.starting_player), PlayDrawChoiceV1::Play)?;
        Ok(service)
    }

    fn record(&mut self, event: &str, payload: &Value) -> Result<(), String> {
        let row = json!({"event":event,"payload":payload});
        let saved = (|| {
            serde_json::to_writer(&mut self.journal, &row).map_err(|_| "The session journal could not be written.".to_owned())?;
            self.journal.write_all(b"\n").map_err(|_| "The session journal could not be written.".to_owned())?;
            self.journal.sync_data().map_err(|_| "The session journal could not be saved.".to_owned())
        })();
        if let Err(reason) = &saved { self.stopped = Some(reason.clone()); }
        saved
    }

    fn start_game(&mut self, chooser: PlayerId, choice: PlayDrawChoiceV1) -> Result<(), String> {
        let result = self.prepare_game(chooser, choice);
        if let Err(reason) = &result { self.stopped = Some(reason.clone()); }
        result
    }

    fn prepare_game(&mut self, chooser: PlayerId, choice: PlayDrawChoiceV1) -> Result<(), String> {
        let mut next_match = self.match_state.clone();
        let prepared = next_match.prepare_game_with_configurations_v1(chooser, choice, self.configurations.clone())
            .map_err(|_| "These deck configurations cannot start the next game.")?;
        let start = prepared.start();
        let seed = self.config.seed.wrapping_add(u64::from(start.game_index).wrapping_sub(1).wrapping_mul(0x9E3779B97F4A7C15));
        let labels = self.config.registered.each_ref().map(|deck| deck.label.clone());
        let mainboards = self.configurations.each_ref().map(|deck| deck.mainboard().to_vec());
        let opening = HumanOpeningV1::new(
            u64::from(start.game_index), seed, self.config.max_physical_decisions, self.config.max_policy_steps,
            labels, mainboards, start.starting_player, self.human,
        ).map_err(|_| "The next game could not be initialized.")?;
        let next_revision = self.opening_revision.checked_add(1).ok_or("The opening request sequence is exhausted.")?;
        self.policy.reset_sampling_v1(paired_policy_seeds_v1(seed));
        let configurations = self.configurations.each_ref().map(|deck| json!({"mainboard":deck.mainboard(),"sideboard":deck.sideboard()}));
        self.record("game_start", &json!({"start":start,"environment_seed":seed,"configurations":configurations}))?;
        self.game_index = start.game_index; self.match_state = next_match; self.session = None;
        self.opening = Some(opening); self.opening_revision = next_revision;
        Ok(())
    }

    fn finish_game(&mut self, winner: Option<PlayerId>, reason: &str) -> Result<(), String> {
        let outcome = winner.map_or(GameOutcomeV1::Draw, |winner| GameOutcomeV1::Win { winner });
        let mut next_match = self.match_state.clone();
        if next_match.record_game_result_v1(outcome).is_err() {
            self.stopped = Some("The game result could not be recorded.".into());
            return Err("The game result could not be recorded.".into());
        }
        let winner_label = winner.map_or("draw", |winner| if winner == self.human { "human" } else { "opponent" });
        let mut summary = json!({"game_index":self.game_index,"winner":winner_label,"reason":reason});
        if let Some(session) = &self.session {
            let state = session.kernel_search_state_v1();
            summary["turn"] = json!(state.turn);
            summary["life_totals"] = json!([state.players[0].life, state.players[1].life]);
        }
        self.record("game_result", &json!({"game_index":self.game_index,"outcome":outcome,"reason":reason,"public_summary":summary}))?;
        self.match_state = next_match;
        self.session = None; self.opening = None; self.sideboard_ready = false;
        self.last_game = Some(summary);
        if let MatchPhaseV1::AwaitingPlayDrawChoice { game_index, .. } = self.match_state.match_state().phase() { self.game_index = game_index; }
        Ok(())
    }

    fn advance_model(&mut self) -> Result<(), String> {
        loop {
            let Some(session) = self.session.as_ref() else { return Ok(()); };
            match session.current_response() {
                FastActorResponseV1::Decision(decision) => {
                    if decision.acting_player == PlayerSeatV1::from(self.human) { return Ok(()); }
                    let selected = self.policy.select_fast_session_v1(session)
                        .map_err(|_| "The model could not choose an action. This game is stopped.")?;
                    self.record("model_action", &json!({"game_index":self.game_index,"step":decision.step,"selected":selected}))?;
                    self.session.as_mut().ok_or("The game session is missing.")?
                        .step(decision.episode_id, decision.step, selected)
                        .map_err(|_| "The engine could not apply the model action. This game is stopped.")?;
                }
                FastActorResponseV1::Terminal(terminal) => {
                    self.record("engine_terminal", &serde_json::to_value(&terminal).map_err(|_| "The game terminal could not be recorded.")?)?;
                    if terminal.terminal_classification != TerminalClassificationV1::Natural {
                        return Err("The engine stopped this game before a natural result. It does not count as a win or loss.".into());
                    }
                    let winner = terminal.winner.map(|seat| match seat { PlayerSeatV1::P0 => PlayerId::P0, PlayerSeatV1::P1 => PlayerId::P1 });
                    self.finish_game(winner, "natural")?;
                    return Ok(());
                }
            }
        }
    }

    fn check_game_index(&self, requested: u8) -> Result<(), String> {
        if requested == self.game_index { Ok(()) } else { Err("That command belongs to a different game. Refresh the current view.".into()) }
    }

    fn mutate(&mut self, command: &HumanMatchCommandV1) -> Result<(), String> {
        if self.stopped.is_some() { return Err("This session is stopped. Start a new session to play again.".into()); }
        match command {
            HumanMatchCommandV1::Current { .. } => Ok(()),
            HumanMatchCommandV1::Action { prompt_seq, action_index, .. } => {
                let session = self.session.as_mut().ok_or("There is no active game decision.")?;
                self.projector.submit(session, HumanActionRequestV1 { prompt_seq: *prompt_seq, action_index: *action_index })
                    .map_err(|error| error.to_string())?;
                Ok(())
            }
            HumanMatchCommandV1::Concede { game_index, .. } => {
                self.check_game_index(*game_index)?;
                if self.session.is_none() && self.opening.is_none() { return Err("There is no active game to concede.".into()); }
                self.finish_game(Some(self.human.opponent()), "concession")
            }
            HumanMatchCommandV1::Sideboard { game_index, mainboard, sideboard, .. } => {
                self.check_game_index(*game_index)?;
                if self.session.is_some() || self.opening.is_some() || *game_index == 1 || !matches!(self.match_state.match_state().phase(), MatchPhaseV1::AwaitingPlayDrawChoice { .. }) {
                    return Err("Sideboarding is available only between games.".into());
                }
                if self.sideboard_ready { return Err("Sideboarding has already been submitted for this game.".into()); }
                let selected = DeckConfigurationV1::new_exact_v1(mainboard.clone(), sideboard.clone())
                    .map_err(|_| "Submit exactly 60 mainboard cards and 15 sideboard cards.")?;
                let registered = self.match_state.registered_deck(self.human).ok_or("The registered human deck is missing.")?;
                if selected.combined_card_counts_v1() != registered.registered_configuration().combined_card_counts_v1() {
                    return Err("Sideboarding must preserve your registered 75 cards.".into());
                }
                self.configurations[self.human.index()] = selected;
                self.sideboard_ready = true;
                if let MatchPhaseV1::AwaitingPlayDrawChoice { chooser, .. } = self.match_state.match_state().phase() {
                    if chooser != self.human { self.start_game(chooser, PlayDrawChoiceV1::Play)?; }
                }
                Ok(())
            }
            HumanMatchCommandV1::PlayDraw { game_index, choice, .. } => {
                self.check_game_index(*game_index)?;
                if !self.sideboard_ready { return Err("Finish sideboarding before choosing play or draw.".into()); }
                let MatchPhaseV1::AwaitingPlayDrawChoice { chooser, .. } = self.match_state.match_state().phase() else {
                    return Err("A play or draw choice is not currently required.".into());
                };
                if chooser != self.human { return Err("The opponent chooses play or draw for this game.".into()); }
                self.start_game(chooser, *choice)
            }
            HumanMatchCommandV1::Mulligan { game_index, opening_revision, .. }
            | HumanMatchCommandV1::Keep { game_index, opening_revision, .. }
            | HumanMatchCommandV1::Bottom { game_index, opening_revision, .. } => {
                self.check_game_index(*game_index)?;
                if *opening_revision != self.opening_revision { return Err("That opening hand is no longer current. Refresh the view.".into()); }
                let next_revision = self.opening_revision.checked_add(1).ok_or("The opening request sequence is exhausted.")?;
                let opening = self.opening.as_mut().ok_or("The opening hand has already been kept.")?;
                match command {
                    HumanMatchCommandV1::Mulligan { .. } => opening.mulligan(),
                    HumanMatchCommandV1::Keep { .. } => opening.keep(),
                    HumanMatchCommandV1::Bottom { hand_indices, .. } => opening.bottom(hand_indices),
                    _ => unreachable!(),
                }.map_err(|_| "That mulligan or bottom selection is not available. Follow the current opening prompt.")?;
                self.opening_revision = next_revision;
                if opening.view().phase == HumanOpeningPhaseV1::Ready {
                    let ready = self.opening.take().ok_or("The kept opening hand is unavailable.")?;
                    match ready.into_session() {
                        Ok(session) => self.session = Some(session),
                        Err(_) => {
                            self.stopped = Some("The engine could not start gameplay from the kept hand.".into());
                            return Err("The engine could not start gameplay from the kept hand.".into());
                        }
                    }
                }
                Ok(())
            }
        }
    }

    fn view(&mut self) -> Result<Value, String> {
        let wins = [self.match_state.match_state().wins(PlayerId::P0).unwrap_or(0), self.match_state.match_state().wins(PlayerId::P1).unwrap_or(0)];
        let mut view = json!({"game_index":self.game_index,"human_seat":self.human.0,"wins":wins,"notice":SETUP_NOTICE,"last_game":self.last_game});
        if let Some(reason) = &self.stopped { view["phase"] = json!("stopped"); view["message"] = json!(reason); return Ok(view); }
        match self.match_state.match_state().phase() {
            MatchPhaseV1::Complete { outcome } => {
                view["phase"] = json!("complete");
                view["winner"] = json!(match outcome { MatchOutcomeV1::Winner { winner } => if winner == self.human { "human" } else { "opponent" }, MatchOutcomeV1::Draw => "draw" });
            }
            MatchPhaseV1::AwaitingPlayDrawChoice { chooser, .. } => {
                if !self.sideboard_ready {
                    view["phase"] = json!("sideboard"); view["own_deck"] = self.own_deck();
                    view["next_chooser"] = json!(if chooser == self.human { "human" } else { "opponent" });
                } else { view["phase"] = json!("play_draw"); }
            }
            MatchPhaseV1::AwaitingGameResult { .. } => {
                if let Some(opening) = &self.opening {
                    let opening = opening.view();
                    view["phase"] = serde_json::to_value(opening.phase).map_err(|_| "The opening phase could not be displayed.")?;
                    view["opening_revision"] = json!(self.opening_revision);
                    view["opening"] = serde_json::to_value(opening).map_err(|_| "The opening hand could not be displayed.")?;
                    return Ok(view);
                }
                let session = self.session.as_ref().ok_or("The active game session is unavailable.")?;
                let FastActorResponseV1::Decision(decision) = session.current_response() else { return Err("The active game has no current human decision.".into()); };
                let human = self.projector.project_current(session, decision).map_err(|error| error.to_string())?;
                view["phase"] = json!("decision"); view["decision"] = serde_json::to_value(human).map_err(|_| "The human decision could not be displayed.")?;
            }
        }
        Ok(view)
    }

    fn own_deck(&self) -> Value {
        fn rows(cards: &[u16]) -> Vec<Value> {
            let mut counts = BTreeMap::<u16, u8>::new();
            for &id in cards { *counts.entry(id).or_default() += 1; }
            counts.into_iter().map(|(card_id, count)| json!({"card_id":card_id,"name":CARD_DEFS[usize::from(card_id)].name,"count":count})).collect()
        }
        let deck = &self.configurations[self.human.index()];
        json!({"label":self.config.registered[self.human.index()].label,"mainboard":rows(deck.mainboard()),"sideboard":rows(deck.sideboard())})
    }

    pub fn handle(&mut self, command: HumanMatchCommandV1) -> Value {
        let request_id = command.request_id().to_owned();
        if request_id.is_empty() || request_id.len() > 128 || !request_id.bytes().all(|b| b.is_ascii_graphic()) {
            return json!({"schema":HUMAN_MATCH_RESPONSE_SCHEMA_V1,"request_id":"","ok":false,"error":"A short request_id is required."});
        }
        if !command.is_read() {
            if let Some((previous, response)) = &self.last_mutation {
                if previous.request_id() == request_id {
                    return if previous == &command { response.clone() } else { json!({"schema":HUMAN_MATCH_RESPONSE_SCHEMA_V1,"request_id":request_id,"ok":false,"error":"That request_id already belongs to a different command."}) };
                }
            }
        }
        let mutation = if command.is_read() { Ok(()) } else {
            self.record("human_command", &serde_json::to_value(&command).unwrap_or(Value::Null)).and_then(|_| self.mutate(&command))
        };
        let mut error = mutation.err();
        if self.stopped.is_none() {
            if let Err(reason) = self.advance_model() { self.stopped = Some(reason.clone()); error = Some(reason); }
        }
        let view = match self.view() {
            Ok(view) => view,
            Err(reason) => {
                self.stopped = Some(reason.clone()); error.get_or_insert(reason.clone());
                self.view().unwrap_or_else(|_| json!({"phase":"stopped","game_index":self.game_index,"human_seat":self.human.0,"message":reason}))
            }
        };
        let response = json!({"schema":HUMAN_MATCH_RESPONSE_SCHEMA_V1,"request_id":request_id,"ok":error.is_none(),"error":error,"view":view});
        if !command.is_read() { self.last_mutation = Some((command, response.clone())); }
        response
    }
}

pub fn serve_human_match_v1(config_path: &Path, mut input: impl BufRead, mut output: impl Write) -> Result<(), String> {
    let mut config_bytes = Vec::new();
    File::open(config_path).map_err(|error| error.to_string())?.take(MAX_CONFIG_BYTES + 1).read_to_end(&mut config_bytes).map_err(|error| error.to_string())?;
    if config_bytes.len() as u64 > MAX_CONFIG_BYTES { return Err("Human match config exceeds 1 MiB.".into()); }
    let config = serde_json::from_slice(&config_bytes).map_err(|error| format!("Invalid human match config: {error}"))?;
    let mut service = HumanMatchServiceV1::new(config)?;
    loop {
        let mut line = Vec::new();
        let read = input.by_ref().take(MAX_REQUEST_BYTES + 1).read_until(b'\n', &mut line).map_err(|_| "Could not read human command.")?;
        if read == 0 { break; }
        let oversized = line.len() as u64 > MAX_REQUEST_BYTES;
        let response = if oversized {
            json!({"schema":HUMAN_MATCH_RESPONSE_SCHEMA_V1,"request_id":"","ok":false,"error":"Human command exceeds the supported size."})
        } else {
            match serde_json::from_slice::<HumanMatchCommandV1>(&line) {
                Ok(command) => service.handle(command),
                Err(_) => json!({"schema":HUMAN_MATCH_RESPONSE_SCHEMA_V1,"request_id":"","ok":false,"error":"Invalid human command."}),
            }
        };
        serde_json::to_writer(&mut output, &response).map_err(|_| "Could not write human response.")?;
        output.write_all(b"\n").map_err(|_| "Could not write human response.")?;
        output.flush().map_err(|_| "Could not flush human response.")?;
        if oversized { break; }
    }
    Ok(())
}
