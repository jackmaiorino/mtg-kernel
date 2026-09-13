use super::{
    hex_digest, require, AgentOpeningPolicyV1, AgentPlayDrawPolicyV1, AgentSideboardPolicyV1,
    CompleteAgentPackageV1,
};
use crate::bo3_match::{
    BestOfThreeMatchStateV1, GameOutcomeV1, GameStartV1, MatchOutcomeV1, MatchPhaseV1,
    PlayDrawChoiceV1,
};
use crate::ids::PlayerId;
use crate::learned_sideboard_v1::{
    LearnedSideboardInputV1, SideboardActionV1, SideboardDeliberationStateV1,
};
use crate::policy_observation_v6::ObservationV6;
use crate::rl::{ActionSemanticV1, PlayerSeatV1, TerminalClassificationV1, TerminalOutcomeV1};
use crate::rl_session::{FastActorResponseV1, FastActorSessionV1};
use crate::sideboard::{CardCountV1, DeckConfigurationV1, RegisteredDeckV1};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const BO3_TRAINING_TRAJECTORY_SCHEMA_V1: &str = "mtg-kernel-bo3-training-trajectory/v1";
const PROBABILITY_SUM_TOLERANCE: f64 = 1e-9;
const MAX_ACTIONS: usize = 65_536;

/// The actual behavior distribution, not pre-search logits or an unused
/// softmax. Greedy selection has probability one. Q64 numerators use decimal
/// strings so JSON cannot round a 2^64 mass through binary64.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum BehaviorDistributionV1 {
    Deterministic {
        selected_index: u32,
    },
    Categorical {
        selected_index: u32,
        probabilities: Vec<f64>,
    },
    HamiltonQ64 {
        selected_index: u32,
        mass_numerators: Vec<String>,
    },
}

impl BehaviorDistributionV1 {
    /// Reuse the actual wide sampler's Hamilton apportionment. The selected
    /// index must still come from the behavior sampler used for this decision.
    pub fn hamilton_from_logits_v1(logits: &[f32], selected_index: u32) -> Result<Self, String> {
        let mut scratch = crate::fast_sampler::WideCategoricalScratchV1::default();
        let masses = scratch
            .apportion(logits)
            .map_err(|error| format!("invalid behavior logits: {error:?}"))?;
        let result = Self::HamiltonQ64 {
            selected_index,
            mass_numerators: masses.iter().map(u128::to_string).collect(),
        };
        result.selected_probability_v1(logits.len())?;
        Ok(result)
    }

    pub fn selected_index_v1(&self) -> usize {
        match self {
            Self::Deterministic { selected_index }
            | Self::Categorical { selected_index, .. }
            | Self::HamiltonQ64 { selected_index, .. } => *selected_index as usize,
        }
    }

    pub fn selected_probability_v1(&self, action_count: usize) -> Result<f64, String> {
        let selected = self.selected_index_v1();
        require(
            action_count > 0 && action_count <= MAX_ACTIONS && selected < action_count,
            "invalid behavior action count/selection",
        )?;
        match self {
            Self::Deterministic { .. } => Ok(1.0),
            Self::Categorical { probabilities, .. } => {
                require(
                    probabilities.len() == action_count
                        && probabilities
                            .iter()
                            .all(|value| value.is_finite() && *value >= 0.0 && *value <= 1.0)
                        && (probabilities.iter().sum::<f64>() - 1.0).abs()
                            <= PROBABILITY_SUM_TOLERANCE
                        && probabilities[selected] > 0.0,
                    "behavior probabilities are nonfinite, unnormalized or select zero mass",
                )?;
                Ok(probabilities[selected])
            }
            Self::HamiltonQ64 {
                mass_numerators, ..
            } => {
                require(
                    mass_numerators.len() == action_count,
                    "Q64 action/mass count differs",
                )?;
                let mut sum = 0_u128;
                let mut selected_mass = 0_u128;
                for (index, text) in mass_numerators.iter().enumerate() {
                    require(
                        !text.is_empty()
                            && text.len() <= 20
                            && text.bytes().all(|byte| byte.is_ascii_digit())
                            && (text == "0" || !text.starts_with('0')),
                        "Q64 mass must be a canonical decimal integer",
                    )?;
                    let mass: u128 = text.parse().map_err(|_| "invalid Q64 mass".to_owned())?;
                    require(mass <= 1_u128 << 64, "Q64 mass exceeds total")?;
                    sum = sum.checked_add(mass).ok_or("Q64 mass overflow")?;
                    if index == selected {
                        selected_mass = mass;
                    }
                }
                require(
                    sum == 1_u128 << 64 && selected_mass > 0,
                    "Q64 masses must total 2^64 and select positive mass",
                )?;
                Ok(selected_mass as f64 / (1_u128 << 64) as f64)
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnDeckConfigurationV1 {
    pub mainboard: Vec<u16>,
    pub sideboard: Vec<u16>,
}

impl OwnDeckConfigurationV1 {
    fn checked(&self) -> Result<DeckConfigurationV1, String> {
        DeckConfigurationV1::new_exact_v1(self.mainboard.clone(), self.sideboard.clone())
            .map_err(|error| error.to_string())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActorOpeningInputV1 {
    pub own_configuration: OwnDeckConfigurationV1,
    pub own_hand: Vec<u16>,
    pub mulligans_taken: u8,
    pub remaining_bottom: u8,
    pub starting_player: PlayerSeatV1,
    pub opponent_hand_count: u8,
    pub opponent_has_kept: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MulliganChoiceV1 {
    Keep,
    Mulligan,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LearningComponentV1 {
    Gameplay,
    PlayDraw,
    Mulligan,
    Bottom,
    Sideboard,
}

/// Only this actor-specific value is exposed to the policy. Match metadata,
/// both registrations, outcome targets, and opponent package identities remain
/// outside it. Gameplay references still require the existing V3 tensorizer;
/// raw arena identities must never become model features.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ActorVisibleDecisionV1 {
    PlayDraw {
        own_configuration: OwnDeckConfigurationV1,
        own_games_won: u8,
        opponent_games_won: u8,
        ordered_choices: Vec<PlayDrawChoiceV1>,
    },
    Mulligan {
        input: ActorOpeningInputV1,
        ordered_choices: Vec<MulliganChoiceV1>,
    },
    /// One conditional choice in an ordered bottoming sequence. Repeated cards
    /// retain their current hand indices; no library position is exposed.
    Bottom {
        input: ActorOpeningInputV1,
        ordered_hand_indices: Vec<u32>,
    },
    Sideboard {
        input: LearnedSideboardInputV1,
        ordered_actions: Vec<SideboardActionV1>,
    },
    Gameplay {
        observation: Box<ObservationV6>,
        ordered_actions: Vec<ActionSemanticV1>,
    },
}

impl ActorVisibleDecisionV1 {
    pub fn component_v1(&self) -> LearningComponentV1 {
        match self {
            Self::PlayDraw { .. } => LearningComponentV1::PlayDraw,
            Self::Mulligan { .. } => LearningComponentV1::Mulligan,
            Self::Bottom { .. } => LearningComponentV1::Bottom,
            Self::Sideboard { .. } => LearningComponentV1::Sideboard,
            Self::Gameplay { .. } => LearningComponentV1::Gameplay,
        }
    }

    fn action_count(&self) -> usize {
        match self {
            Self::PlayDraw {
                ordered_choices, ..
            } => ordered_choices.len(),
            Self::Mulligan {
                ordered_choices, ..
            } => ordered_choices.len(),
            Self::Bottom {
                ordered_hand_indices,
                ..
            } => ordered_hand_indices.len(),
            Self::Sideboard {
                ordered_actions, ..
            } => ordered_actions.len(),
            Self::Gameplay {
                ordered_actions, ..
            } => ordered_actions.len(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Bo3DecisionRecordV1 {
    /// Contiguous across the whole match, including both seats and all phases.
    pub decision_index: u64,
    pub actor: PlayerSeatV1,
    pub behavior_package_sha256: String,
    pub behavior: BehaviorDistributionV1,
    pub visible: ActorVisibleDecisionV1,
}

impl Bo3DecisionRecordV1 {
    /// Captures the current trusted session's actor-visible V6 input/actions
    /// from one binding. No GameState, environment seed or diagnostic hash is
    /// accepted from an external caller.
    pub fn gameplay_from_session_v1(
        decision_index: u64,
        behavior_package_sha256: String,
        behavior: BehaviorDistributionV1,
        session: &FastActorSessionV1,
    ) -> Result<Self, String> {
        let expected = match session.current_response() {
            FastActorResponseV1::Decision(value) => value,
            FastActorResponseV1::Terminal(_) => {
                return Err("cannot capture a decision after terminal".into())
            }
        };
        let (observation, ordered_actions, _) = session
            .human_current_decision_input_v1(expected, expected.acting_player)
            .map_err(|error| format!("actor-visible binding unavailable: {error:?}"))?;
        behavior.selected_probability_v1(ordered_actions.len())?;
        require(
            hex_digest(&behavior_package_sha256, 64),
            "invalid behavior package digest",
        )?;
        Ok(Self {
            decision_index,
            actor: expected.acting_player,
            behavior_package_sha256,
            behavior,
            visible: ActorVisibleDecisionV1::Gameplay {
                observation: Box::new(observation),
                ordered_actions,
            },
        })
    }

    pub fn model_input_v1(&self) -> &ActorVisibleDecisionV1 {
        &self.visible
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Bo3GameTerminalV1 {
    pub classification: TerminalClassificationV1,
    pub outcome: TerminalOutcomeV1,
    pub gameplay_decision_count: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Bo3TrainingGameV1 {
    pub game_index: u8,
    /// None is allowed only when an incomplete match stops before play/draw.
    pub start: Option<GameStartV1>,
    pub decisions: Vec<Bo3DecisionRecordV1>,
    pub terminal: Option<Bo3GameTerminalV1>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IncompleteMatchReasonV1 {
    Interrupted,
    DecisionCap,
    EngineError,
    PhysicalGameCap,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Bo3TrajectoryEndingV1 {
    Complete { outcome: MatchOutcomeV1 },
    Incomplete { reason: IncompleteMatchReasonV1 },
}

/// Training/evaluation bookkeeping. Never supply this whole object to a policy.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Bo3TrainingTrajectoryV1 {
    pub schema: String,
    pub match_id: String,
    pub initial_chooser: PlayerSeatV1,
    pub behavior_packages_by_seat: [String; 2],
    pub registrations_by_seat: [OwnDeckConfigurationV1; 2],
    pub games: Vec<Bo3TrainingGameV1>,
    pub ending: Bo3TrajectoryEndingV1,
}

/// Constructed only after the structural validator succeeds. This is not a
/// replay certificate or independent verification of an external producer.
pub struct ValidatedBo3TrajectoryV1<'a> {
    trajectory: &'a Bo3TrainingTrajectoryV1,
    winner: Option<PlayerId>,
}

impl ValidatedBo3TrajectoryV1<'_> {
    pub fn is_complete_v1(&self) -> bool {
        self.winner.is_some()
    }
    pub fn trajectory_v1(&self) -> &Bo3TrainingTrajectoryV1 {
        self.trajectory
    }
    pub fn match_return_v1(&self, actor: PlayerSeatV1) -> Option<f64> {
        self.winner.map(|winner| {
            if winner == player_id(actor) {
                1.0
            } else {
                -1.0
            }
        })
    }
}

fn seat_index(seat: PlayerSeatV1) -> usize {
    match seat {
        PlayerSeatV1::P0 => 0,
        PlayerSeatV1::P1 => 1,
    }
}
fn player_id(seat: PlayerSeatV1) -> PlayerId {
    match seat {
        PlayerSeatV1::P0 => PlayerId::P0,
        PlayerSeatV1::P1 => PlayerId::P1,
    }
}

fn card_counts(cards: &[u16]) -> BTreeMap<u16, usize> {
    let mut result = BTreeMap::new();
    for card in cards {
        *result.entry(*card).or_insert(0) += 1;
    }
    result
}

fn validate_opening_input(
    input: &ActorOpeningInputV1,
    current: &DeckConfigurationV1,
    start: GameStartV1,
) -> Result<(), String> {
    require(
        input.own_configuration.checked()? == *current
            && player_id(input.starting_player) == start.starting_player
            && input.mulligans_taken <= 7
            && input.remaining_bottom <= input.mulligans_taken
            && input.opponent_hand_count <= 7,
        "opening input configuration, role or counts differ",
    )?;
    let main = card_counts(current.mainboard());
    require(
        card_counts(&input.own_hand)
            .iter()
            .all(|(card, count)| main.get(card).is_some_and(|available| count <= available)),
        "opening hand contains cards outside the actor's current mainboard",
    )
}

fn validate_sideboard_input(
    input: &LearnedSideboardInputV1,
    registered: &[CardCountV1],
    game_index: u8,
    wins: [u8; 2],
    actor: usize,
) -> Result<(), String> {
    require(
        input.registered_cards == registered
            && input.next_game_number == game_index
            && input.acting_player_games_won == wins[actor]
            && input.opponent_games_won == wins[1 - actor],
        "sideboard input registration or completed score differs",
    )?;
    let prior = |game: u8| game > 0 && game < game_index;
    let mut own_keys = BTreeSet::new();
    require(
        input.own_card_outcomes.iter().all(|row| {
            prior(row.game_index)
                && registered.iter().any(|card| card.card_id == row.card_id)
                && own_keys.insert((row.game_index, row.card_id))
        }) && input
            .opponent_evidence
            .iter()
            .all(|row| prior(row.game_index)),
        "sideboard evidence exceeds the completed visible prefix",
    )?;
    let mut resource_games = BTreeSet::new();
    require(
        input.resource_summaries.iter().all(|row| {
            prior(row.game_index)
                && resource_games.insert(row.game_index)
                && row
                    .own_lands_mean
                    .into_iter()
                    .chain(row.own_hand_mean)
                    .all(|value| value.is_finite() && value >= 0.0)
        }),
        "sideboard resource summaries are nonfinite, duplicate or outside the completed prefix",
    )
}

impl Bo3TrainingTrajectoryV1 {
    pub fn from_json_v1(json: &str) -> Result<Self, String> {
        let value = crate::rl::parse_strict_json_value(json).map_err(|error| error.to_string())?;
        serde_json::from_value(value).map_err(|error| error.to_string())
    }

    pub fn validate_v1<'a>(
        &'a self,
        packages: [&CompleteAgentPackageV1; 2],
    ) -> Result<ValidatedBo3TrajectoryV1<'a>, String> {
        require(
            self.schema == BO3_TRAINING_TRAJECTORY_SCHEMA_V1
                && !self.match_id.is_empty()
                && self.match_id.len() <= 128
                && self.match_id.is_ascii(),
            "invalid BO3 trajectory identity",
        )?;
        let hashes = [
            packages[0].package_sha256_v1()?,
            packages[1].package_sha256_v1()?,
        ];
        require(
            packages[0].runtime == packages[1].runtime,
            "physical seats use different runtime/feature contracts",
        )?;
        require(
            self.behavior_packages_by_seat == hashes,
            "trajectory packages differ from the fixed behavior packages",
        )?;
        for (seat, configuration) in self.registrations_by_seat.iter().enumerate() {
            RegisteredDeckV1::new_executable_v1(
                format!("phase1-seat-{seat}"),
                configuration.mainboard.clone(),
                configuration.sideboard.clone(),
            )
            .map_err(|error| error.to_string())?;
        }
        let registered = [
            self.registrations_by_seat[0].checked()?,
            self.registrations_by_seat[1].checked()?,
        ];
        let mut current = registered.clone();
        let mut state = BestOfThreeMatchStateV1::new_v1(player_id(self.initial_chooser))
            .map_err(|error| error.to_string())?;
        let mut decision_index = 0_u64;
        for (ordinal, game) in self.games.iter().enumerate() {
            let (expected_game, chooser) = match state.phase() {
                MatchPhaseV1::AwaitingPlayDrawChoice {
                    game_index,
                    chooser,
                } => (game_index, chooser),
                _ => return Err(
                    "trajectory contains a game after match completion or before a prior result"
                        .into(),
                ),
            };
            require(
                game.game_index == expected_game,
                "missing, duplicate or reordered physical game",
            )?;
            let wins = [
                state.wins(PlayerId::P0).unwrap(),
                state.wins(PlayerId::P1).unwrap(),
            ];
            let mut sideboard = current.each_ref().map(SideboardDeliberationStateV1::new_v1);
            let mut board_done = [expected_game == 1; 2];
            let mut start = None;
            let mut mulligans = [0_u8; 2];
            let mut kept_hand: [Option<Vec<u16>>; 2] = [None, None];
            let mut bottom_remaining = [0_u8; 2];
            let mut opening_ready = [false; 2];
            let mut gameplay_count = 0_u64;
            for decision in &game.decisions {
                let actor = seat_index(decision.actor);
                require(
                    decision.decision_index == decision_index
                        && decision.behavior_package_sha256 == hashes[actor],
                    "missing/reordered decision or stale behavior package",
                )?;
                decision_index = decision_index
                    .checked_add(1)
                    .ok_or("decision index overflow")?;
                decision
                    .behavior
                    .selected_probability_v1(decision.visible.action_count())?;
                let selected = decision.behavior.selected_index_v1();
                match &decision.visible {
                    ActorVisibleDecisionV1::Sideboard {
                        input,
                        ordered_actions,
                    } => {
                        require(
                            expected_game > 1 && start.is_none() && !board_done[actor],
                            "sideboarding is outside its between-game phase",
                        )?;
                        validate_sideboard_input(
                            input,
                            &registered[actor].combined_card_counts_v1(),
                            expected_game,
                            wins,
                            actor,
                        )?;
                        require(
                            *ordered_actions == sideboard[actor].legal_actions_v1(),
                            "sideboard legal action order differs from replay",
                        )?;
                        if matches!(packages[actor].sideboard, AgentSideboardPolicyV1::Keep) {
                            require(
                                ordered_actions[selected] == SideboardActionV1::Done,
                                "keep sideboard policy changed the deck",
                            )?;
                        }
                        require(matches!(decision.behavior, BehaviorDistributionV1::Deterministic { .. }),
                            "current keep/greedy sideboard descriptors require deterministic behavior")?;
                        sideboard[actor]
                            .apply_v1(ordered_actions[selected])
                            .map_err(|error| error.to_string())?;
                        if sideboard[actor].is_done_v1() {
                            current[actor] = sideboard[actor]
                                .configuration_v1()
                                .map_err(|error| error.to_string())?;
                            board_done[actor] = true;
                        }
                    }
                    ActorVisibleDecisionV1::PlayDraw {
                        own_configuration,
                        own_games_won,
                        opponent_games_won,
                        ordered_choices,
                    } => {
                        require(
                            board_done == [true; 2]
                                && start.is_none()
                                && player_id(decision.actor) == chooser,
                            "play/draw choice has wrong actor or phase",
                        )?;
                        require(
                            own_configuration.checked()? == current[actor]
                                && *own_games_won == wins[actor]
                                && *opponent_games_won == wins[1 - actor]
                                && *ordered_choices
                                    == [PlayDrawChoiceV1::Play, PlayDrawChoiceV1::Draw],
                            "play/draw visible context or legal order differs",
                        )?;
                        if let AgentPlayDrawPolicyV1::Fixed { choice } = &packages[actor].play_draw
                        {
                            require(
                                ordered_choices[selected] == *choice
                                    && matches!(
                                        decision.behavior,
                                        BehaviorDistributionV1::Deterministic { .. }
                                    ),
                                "fixed play/draw policy has different behavior",
                            )?;
                        }
                        start = Some(
                            state
                                .choose_play_draw_v1(chooser, ordered_choices[selected])
                                .map_err(|error| error.to_string())?,
                        );
                    }
                    ActorVisibleDecisionV1::Mulligan {
                        input,
                        ordered_choices,
                    } => {
                        let started = start.ok_or("mulligan before play/draw")?;
                        require(
                            !opening_ready[actor]
                                && kept_hand[actor].is_none()
                                && gameplay_count == 0,
                            "mulligan occurs after keeping or gameplay",
                        )?;
                        validate_opening_input(input, &current[actor], started)?;
                        let expected = if mulligans[actor] < 7 {
                            vec![MulliganChoiceV1::Keep, MulliganChoiceV1::Mulligan]
                        } else {
                            vec![MulliganChoiceV1::Keep]
                        };
                        require(
                            input.mulligans_taken == mulligans[actor]
                                && input.remaining_bottom == 0
                                && input.own_hand.len() == 7
                                && *ordered_choices == expected,
                            "mulligan count, hand or legal choices differ",
                        )?;
                        if matches!(
                            packages[actor].opening,
                            AgentOpeningPolicyV1::Existing { .. }
                        ) {
                            require(
                                ordered_choices[selected] == MulliganChoiceV1::Keep
                                    && matches!(
                                        decision.behavior,
                                        BehaviorDistributionV1::Deterministic { .. }
                                    ),
                                "keep-seven package cannot supply learned mulligan behavior",
                            )?;
                        }
                        match ordered_choices[selected] {
                            MulliganChoiceV1::Keep => {
                                kept_hand[actor] = Some(input.own_hand.clone());
                                bottom_remaining[actor] = mulligans[actor];
                                opening_ready[actor] = mulligans[actor] == 0;
                            }
                            MulliganChoiceV1::Mulligan => mulligans[actor] += 1,
                        }
                    }
                    ActorVisibleDecisionV1::Bottom {
                        input,
                        ordered_hand_indices,
                    } => {
                        let started = start.ok_or("bottom choice before play/draw")?;
                        validate_opening_input(input, &current[actor], started)?;
                        require(
                            !opening_ready[actor]
                                && bottom_remaining[actor] > 0
                                && gameplay_count == 0
                                && input.mulligans_taken == mulligans[actor]
                                && input.remaining_bottom == bottom_remaining[actor]
                                && kept_hand[actor].as_ref() == Some(&input.own_hand)
                                && *ordered_hand_indices
                                    == (0..input.own_hand.len() as u32).collect::<Vec<_>>(),
                            "bottoming is out of phase or does not continue the kept hand",
                        )?;
                        kept_hand[actor]
                            .as_mut()
                            .unwrap()
                            .remove(ordered_hand_indices[selected] as usize);
                        bottom_remaining[actor] -= 1;
                        opening_ready[actor] = bottom_remaining[actor] == 0;
                    }
                    ActorVisibleDecisionV1::Gameplay {
                        observation,
                        ordered_actions,
                    } => {
                        require(
                            start.is_some() && opening_ready == [true; 2],
                            "gameplay begins before both openings finish",
                        )?;
                        require(
                            observation.schema_version == 6
                                && observation.acting_player == decision.actor
                                && observation.policy_surface_version
                                    == crate::policy_surface_v5::POLICY_SURFACE_VERSION
                                && observation.step_index == gameplay_count
                                && format!("{:016x}", observation.card_db_hash)
                                    == packages[actor].runtime.card_db_hash
                                && observation.substep_count > 0
                                && observation.substep_index < observation.substep_count,
                            "gameplay observation actor, feature contract or step differs",
                        )?;
                        for action in ordered_actions {
                            let value =
                                serde_json::to_value(action).map_err(|error| error.to_string())?;
                            require(
                                value.get("actor")
                                    == Some(
                                        &serde_json::to_value(decision.actor)
                                            .map_err(|error| error.to_string())?,
                                    ),
                                "gameplay legal action belongs to another actor",
                            )?;
                        }
                        gameplay_count += 1;
                    }
                }
            }
            require(
                start == game.start,
                "declared game start differs from play/draw replay",
            )?;
            match &game.terminal {
                Some(terminal) => {
                    require(
                        terminal.gameplay_decision_count == gameplay_count,
                        "terminal gameplay count differs",
                    )?;
                    let outcome = match (terminal.classification, terminal.outcome) {
                        (TerminalClassificationV1::Natural, TerminalOutcomeV1::P0Win) => {
                            Some(GameOutcomeV1::Win {
                                winner: PlayerId::P0,
                            })
                        }
                        (TerminalClassificationV1::Natural, TerminalOutcomeV1::P1Win) => {
                            Some(GameOutcomeV1::Win {
                                winner: PlayerId::P1,
                            })
                        }
                        (TerminalClassificationV1::Natural, TerminalOutcomeV1::Draw) => {
                            Some(GameOutcomeV1::Draw)
                        }
                        (TerminalClassificationV1::Truncated, TerminalOutcomeV1::Truncated)
                        | (TerminalClassificationV1::Halted, TerminalOutcomeV1::Halted) => None,
                        _ => return Err("terminal classification and outcome differ".into()),
                    };
                    if let Some(outcome) = outcome {
                        require(
                            start.is_some() && opening_ready == [true; 2] && gameplay_count > 0,
                            "natural terminal lacks complete opening/gameplay records",
                        )?;
                        state
                            .record_game_result_v1(outcome)
                            .map_err(|error| error.to_string())?;
                    } else {
                        require(
                            ordinal + 1 == self.games.len()
                                && matches!(self.ending, Bo3TrajectoryEndingV1::Incomplete { .. }),
                            "capped/halted games cannot continue or receive a match target",
                        )?;
                    }
                }
                None => require(
                    ordinal + 1 == self.games.len()
                        && matches!(self.ending, Bo3TrajectoryEndingV1::Incomplete { .. }),
                    "missing terminal before match continuation/completion",
                )?,
            }
        }
        let winner = match self.ending {
            Bo3TrajectoryEndingV1::Complete { outcome } => {
                require(
                    state.outcome() == Some(outcome),
                    "declared match outcome differs from natural first-to-two results",
                )?;
                match outcome {
                    MatchOutcomeV1::Winner { winner } => Some(winner),
                    MatchOutcomeV1::Draw => {
                        return Err("match draws are not complete training targets".into())
                    }
                }
            }
            Bo3TrajectoryEndingV1::Incomplete { .. } => {
                require(
                    state.outcome().is_none(),
                    "a naturally completed match is mislabeled incomplete",
                )?;
                None
            }
        };
        Ok(ValidatedBo3TrajectoryV1 {
            trajectory: self,
            winner,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct MatchNormalizedDecisionWeightV1 {
    pub match_id: String,
    pub decision_index: u64,
    pub actor: PlayerSeatV1,
    pub component: LearningComponentV1,
    pub match_return: f64,
    pub weight: f64,
}

/// Only the requested learner/component receives weights. Incomplete matches
/// and matches with no such decision contribute zero. Each remaining match has
/// total weight 1/N, regardless of game count or trajectory length. This utility
/// derives weights; it does not implement a loss, gradient, optimizer or update.
pub fn equal_match_weights_v1(
    trajectories: &[ValidatedBo3TrajectoryV1<'_>],
    actor: PlayerSeatV1,
    component: LearningComponentV1,
    expected_behavior_package_sha256: &str,
) -> Result<Vec<MatchNormalizedDecisionWeightV1>, String> {
    require(
        hex_digest(expected_behavior_package_sha256, 64),
        "invalid expected learner package",
    )?;
    let mut ids = BTreeSet::new();
    let mut eligible = Vec::new();
    for validated in trajectories {
        let trajectory = validated.trajectory;
        require(
            ids.insert(&trajectory.match_id),
            "duplicate match in learning batch",
        )?;
        let Some(value) = validated.match_return_v1(actor) else {
            continue;
        };
        require(
            trajectory.behavior_packages_by_seat[seat_index(actor)]
                == expected_behavior_package_sha256,
            "learning batch mixes stale learner behavior packages",
        )?;
        let decisions: Vec<_> = trajectory
            .games
            .iter()
            .flat_map(|game| &game.decisions)
            .filter(|decision| {
                decision.actor == actor && decision.visible.component_v1() == component
            })
            .collect();
        if !decisions.is_empty() {
            eligible.push((trajectory, value, decisions));
        }
    }
    let matches = eligible.len();
    let mut result = Vec::new();
    for (trajectory, value, decisions) in eligible {
        let weight = 1.0 / matches as f64 / decisions.len() as f64;
        for decision in decisions {
            result.push(MatchNormalizedDecisionWeightV1 {
                match_id: trajectory.match_id.clone(),
                decision_index: decision.decision_index,
                actor,
                component,
                match_return: value,
                weight,
            });
        }
    }
    Ok(result)
}
