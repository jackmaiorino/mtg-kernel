//! A small CPU deck policy trained by imitation of externally supplied plans.
//!
//! Inputs contain our registered cards and completed, player-visible evidence,
//! never an opponent deck identifier. The shared per-card policy consumes pooled
//! sets rather than a catalog-specific deck classifier. Play-model embeddings
//! are borrowed, frozen inputs. This is an imitation implementation, not an RL
//! trainer or a claim that a particular checkpoint improves match win rate.

use crate::sideboard::{CardCountV1, DeckConfigurationV1, SideboardPlanV1};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

pub const SIDEBOARD_EMBEDDING_DIM_V1: usize = 16;
pub const SIDEBOARD_MAX_DECISIONS_V1: usize = 64;
const VOCAB_SIZE: usize = 65_537;
const HIDDEN: usize = 24;
// Registered, current main, current side, opponent, game-index weighted opponent,
// pooled own outcomes, positional own outcomes, resources with missingness, score.
const CONTEXT: usize = 16 * 5 + 6 * 2 + 12 * 2 + 5 + 9;
// Context, candidate embedding, candidate outcomes, kind and counts.
const FEATURES: usize = CONTEXT + 16 + 12 + 3 + 3;
const SCHEMA: &str = "kernel_learned_sideboard/v1";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LearnedSideboardErrorV1(pub String);

impl fmt::Display for LearnedSideboardErrorV1 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}
impl std::error::Error for LearnedSideboardErrorV1 {}
type ResultV1<T> = Result<T, LearnedSideboardErrorV1>;
fn error(message: impl Into<String>) -> LearnedSideboardErrorV1 {
    LearnedSideboardErrorV1(message.into())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SideboardPlayIdentityV1 {
    pub weights_sha256: String,
    pub git_head: String,
}

/// Native policy token zero is Unknown; a known card uses card_id + 1.
/// The immutable borrow prevents this policy's optimizer from changing the table.
pub struct FrozenSideboardEmbeddingsV1<'a> {
    values: &'a [f32],
    identity: SideboardPlayIdentityV1,
    table_sha256: String,
}

impl<'a> FrozenSideboardEmbeddingsV1<'a> {
    pub fn new_v1(values: &'a [f32], identity: SideboardPlayIdentityV1) -> ResultV1<Self> {
        validate_identity(&identity)?;
        if values.len() != VOCAB_SIZE * SIDEBOARD_EMBEDDING_DIM_V1
            || values.iter().any(|v| !v.is_finite())
            || values[..SIDEBOARD_EMBEDDING_DIM_V1]
                .iter()
                .any(|v| *v != 0.0)
        {
            return Err(error(
                "expected finite native 65537 by 16 embeddings with zero Unknown row",
            ));
        }
        let mut digest = Sha256::new();
        for value in values {
            digest.update(value.to_bits().to_le_bytes());
        }
        Ok(Self {
            values,
            identity,
            table_sha256: format!("{:x}", digest.finalize()),
        })
    }

    pub fn identity_v1(&self) -> &SideboardPlayIdentityV1 {
        &self.identity
    }
    pub fn table_sha256_v1(&self) -> &str {
        &self.table_sha256
    }
    fn row(&self, card: Option<u16>) -> &[f32] {
        let begin = card.map_or(0, |id| usize::from(id) + 1) * 16;
        &self.values[begin..begin + 16]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VisibleEvidenceZoneV1 {
    Battlefield,
    Stack,
    Graveyard,
    Exile,
    Hand,
    OtherVisible,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SideboardOpponentEvidenceV1 {
    pub card_id: Option<u16>,
    pub game_index: u8,
    pub first_seen_turn: u32,
    /// Only a zone visible to the acting player may be supplied here.
    pub zone: VisibleEvidenceZoneV1,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SideboardOwnCardOutcomeV1 {
    pub card_id: u16,
    pub game_index: u8,
    pub times_drawn: u8,
    pub cast: bool,
    pub stuck_in_hand: bool,
    pub died_without_dealing_damage: bool,
    pub removal_no_target: bool,
    pub counterspell_held: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SideboardGameResourceV1 {
    pub game_index: u8,
    pub own_lands_mean: Option<f32>,
    pub own_hand_mean: Option<f32>,
    pub own_life_final: Option<i32>,
    pub opponent_life_final: Option<i32>,
    pub own_damage_dealt: Option<i64>,
    pub own_damage_taken: Option<i64>,
}

/// A visible-only boundary type. Do not serialize raw game state into this type.
/// Missing observations stay absent; neither a true opponent deck ID nor hidden
/// hand/library contents have a field in this contract.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LearnedSideboardInputV1 {
    pub registered_cards: Vec<CardCountV1>,
    pub own_card_outcomes: Vec<SideboardOwnCardOutcomeV1>,
    pub opponent_evidence: Vec<SideboardOpponentEvidenceV1>,
    pub resource_summaries: Vec<SideboardGameResourceV1>,
    pub next_game_number: u8,
    pub acting_player_games_won: u8,
    pub opponent_games_won: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum SideboardActionV1 {
    MoveOneToSideboard { card_id: u16 },
    MoveOneToMainboard { card_id: u16 },
    Done,
}

/// Canonical out-then-in deliberation covers every nonredundant exchange of up
/// to 15 cards. Its phases prevent cycling and guarantee a legal submission in
/// at most 31 decisions, within the adapter's 64-decision ceiling. Intermediate
/// configurations may have fewer than 60 mainboard cards; Done never does.
#[derive(Debug, Clone)]
pub struct SideboardDeliberationStateV1 {
    main: BTreeMap<u16, u8>,
    side: BTreeMap<u16, u8>,
    registered: BTreeMap<u16, u8>,
    moved_out: BTreeSet<u16>,
    out_count: usize,
    adding: bool,
    done: bool,
}

impl SideboardDeliberationStateV1 {
    pub fn new_v1(configuration: &DeckConfigurationV1) -> Self {
        Self {
            main: counts(configuration.mainboard()),
            side: counts(configuration.sideboard()),
            registered: configuration
                .combined_card_counts_v1()
                .into_iter()
                .map(|row| (row.card_id, row.count))
                .collect(),
            moved_out: BTreeSet::new(),
            out_count: 0,
            adding: false,
            done: false,
        }
    }

    pub fn legal_actions_v1(&self) -> Vec<SideboardActionV1> {
        if self.done {
            return Vec::new();
        }
        let main_size = total(&self.main);
        let vacancy = 60 - main_size;
        let mut result = Vec::new();
        if !self.adding && self.out_count < 15 {
            for (&card_id, &count) in &self.main {
                // Do not remove a card if doing so would make restoring 60
                // impossible without a redundant reversal of an earlier move.
                let incoming_capacity: usize = self
                    .side
                    .iter()
                    .filter(|(id, _)| **id != card_id && !self.moved_out.contains(id))
                    .map(|(_, n)| usize::from(*n))
                    .sum();
                if count > 0 && incoming_capacity > vacancy {
                    result.push(SideboardActionV1::MoveOneToSideboard { card_id });
                }
            }
        }
        if vacancy > 0 {
            for (&card_id, &count) in &self.side {
                if count > 0 && !self.moved_out.contains(&card_id) {
                    result.push(SideboardActionV1::MoveOneToMainboard { card_id });
                }
            }
        }
        if main_size == 60 {
            result.push(SideboardActionV1::Done);
        }
        result
    }

    pub fn apply_v1(&mut self, action: SideboardActionV1) -> ResultV1<()> {
        if !self.legal_actions_v1().contains(&action) {
            return Err(error(format!("illegal sideboard action {action:?}")));
        }
        match action {
            SideboardActionV1::MoveOneToSideboard { card_id } => {
                move_one(&mut self.main, &mut self.side, card_id);
                self.moved_out.insert(card_id);
                self.out_count += 1;
            }
            SideboardActionV1::MoveOneToMainboard { card_id } => {
                move_one(&mut self.side, &mut self.main, card_id);
                self.adding = true;
            }
            SideboardActionV1::Done => self.done = true,
        }
        Ok(())
    }

    pub fn is_done_v1(&self) -> bool {
        self.done
    }
    pub fn configuration_v1(&self) -> ResultV1<DeckConfigurationV1> {
        let configuration =
            DeckConfigurationV1::new_exact_v1(expand(&self.main), expand(&self.side))
                .map_err(|e| error(e.to_string()))?;
        let registered: BTreeMap<_, _> = configuration
            .combined_card_counts_v1()
            .into_iter()
            .map(|row| (row.card_id, row.count))
            .collect();
        if registered != self.registered {
            return Err(error("registered 75 changed"));
        }
        Ok(configuration)
    }
}

/// Converts an accepted table plan into the design's canonical teaching target.
/// The caller supplies matching visible evidence and must separately establish
/// the plan's provenance and evaluation quality.
pub fn canonical_sideboard_actions_v1(plan: &SideboardPlanV1) -> Vec<SideboardActionV1> {
    let mut actions = Vec::new();
    for row in plan.cards_out() {
        for _ in 0..row.count {
            actions.push(SideboardActionV1::MoveOneToSideboard {
                card_id: row.card_id,
            });
        }
    }
    for row in plan.cards_in() {
        for _ in 0..row.count {
            actions.push(SideboardActionV1::MoveOneToMainboard {
                card_id: row.card_id,
            });
        }
    }
    actions.push(SideboardActionV1::Done);
    actions
}

/// Produces canonical teaching actions from the actual current configuration.
/// In game three this can differ from expanding a registered-relative table
/// plan, because game two may already have moved some of the same cards.
pub fn actions_between_configurations_v1(
    initial: &DeckConfigurationV1,
    target: &DeckConfigurationV1,
) -> ResultV1<Vec<SideboardActionV1>> {
    if initial.combined_card_counts_v1() != target.combined_card_counts_v1() {
        return Err(error("teacher target changes the registered 75"));
    }
    let initial_counts = counts(initial.mainboard());
    let target_counts = counts(target.mainboard());
    let mut actions = Vec::new();
    for (&card_id, &count) in &initial_counts {
        let removed = count.saturating_sub(*target_counts.get(&card_id).unwrap_or(&0));
        for _ in 0..removed {
            actions.push(SideboardActionV1::MoveOneToSideboard { card_id });
        }
    }
    for (&card_id, &count) in &target_counts {
        let added = count.saturating_sub(*initial_counts.get(&card_id).unwrap_or(&0));
        for _ in 0..added {
            actions.push(SideboardActionV1::MoveOneToMainboard { card_id });
        }
    }
    actions.push(SideboardActionV1::Done);
    let mut state = SideboardDeliberationStateV1::new_v1(initial);
    for &action in &actions {
        state.apply_v1(action)?;
    }
    if state.configuration_v1()? != *target {
        return Err(error(
            "teacher delta does not reproduce target configuration",
        ));
    }
    Ok(actions)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SideboardImitationExampleV1 {
    pub input: LearnedSideboardInputV1,
    pub initial_mainboard: Vec<u16>,
    pub initial_sideboard: Vec<u16>,
    pub target_actions: Vec<SideboardActionV1>,
    /// A measured BO3 win-rate delta in [-1,1], or absent. No synthetic value
    /// label is substituted when a teacher plan lacks a measured target.
    pub target_value: Option<f32>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SideboardTrainingConfigV1 {
    pub epochs: usize,
    pub learning_rate: f64,
    pub value_loss_weight: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SideboardTrainingMetricsV1 {
    pub examples: usize,
    pub decisions: usize,
    pub initial_cross_entropy: f64,
    pub final_cross_entropy: f64,
    pub initial_action_accuracy: f64,
    pub final_action_accuracy: f64,
    pub value_targets: usize,
    pub final_value_mse: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScoredSideboardDecisionV1 {
    pub ordered_actions: Vec<SideboardActionV1>,
    pub logits: Vec<f64>,
    pub probabilities: Vec<f64>,
    pub selected_action: SideboardActionV1,
    pub value: f64,
}

#[derive(Debug, Clone)]
pub struct LearnedSideboardResultV1 {
    pub configuration: DeckConfigurationV1,
    pub actions: Vec<SideboardActionV1>,
    pub initial_value: f64,
}

/// Shared action MLP and value head. The nonlinear shared head lets visible
/// opponent evidence change the preference among candidate card moves. There
/// are no per-archetype output rows. Only these weights are optimized.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LearnedSideboardModelV1 {
    schema: String,
    play_identity: SideboardPlayIdentityV1,
    embedding_sha256: String,
    initialization_seed: u64,
    training_steps: u64,
    hidden_weights: Vec<f64>,
    hidden_bias: Vec<f64>,
    policy_weights: Vec<f64>,
    value_weights: Vec<f64>,
    value_bias: f64,
}

impl LearnedSideboardModelV1 {
    pub fn new_v1(seed: u64, embeddings: &FrozenSideboardEmbeddingsV1<'_>) -> Self {
        let mut rng = SeededRng(seed);
        Self {
            schema: SCHEMA.to_owned(),
            play_identity: embeddings.identity.clone(),
            embedding_sha256: embeddings.table_sha256.clone(),
            initialization_seed: seed,
            training_steps: 0,
            hidden_weights: (0..HIDDEN * FEATURES).map(|_| rng.weight(0.15)).collect(),
            hidden_bias: vec![0.0; HIDDEN],
            policy_weights: (0..HIDDEN).map(|_| rng.weight(0.2)).collect(),
            value_weights: vec![0.0; CONTEXT],
            value_bias: 0.0,
        }
    }

    pub fn play_identity_v1(&self) -> &SideboardPlayIdentityV1 {
        &self.play_identity
    }
    pub fn training_steps_v1(&self) -> u64 {
        self.training_steps
    }
    pub fn to_json_v1(&self) -> ResultV1<String> {
        self.validate()?;
        serde_json::to_string_pretty(self).map_err(|e| error(e.to_string()))
    }
    pub fn from_json_v1(json: &str) -> ResultV1<Self> {
        let model: Self = serde_json::from_str(json).map_err(|e| error(e.to_string()))?;
        model.validate()?;
        Ok(model)
    }
    pub fn checkpoint_sha256_v1(&self) -> ResultV1<String> {
        Ok(format!(
            "{:x}",
            Sha256::digest(self.to_json_v1()?.as_bytes())
        ))
    }

    /// Check a frozen-play binding before running games or collecting inputs.
    pub fn validate_frozen_embeddings_v1(
        &self,
        embeddings: &FrozenSideboardEmbeddingsV1<'_>,
    ) -> ResultV1<()> {
        self.check_embeddings(embeddings)
    }

    pub fn score_v1(
        &self,
        input: &LearnedSideboardInputV1,
        state: &SideboardDeliberationStateV1,
        embeddings: &FrozenSideboardEmbeddingsV1<'_>,
    ) -> ResultV1<ScoredSideboardDecisionV1> {
        self.check_embeddings(embeddings)?;
        let context = encode_context(input, state, embeddings)?;
        let actions = state.legal_actions_v1();
        if actions.is_empty() {
            return Err(error("cannot score a completed deliberation"));
        }
        let logits: Vec<f64> = actions
            .iter()
            .map(|action| {
                let features = action_features(input, state, embeddings, &context, *action);
                self.forward(&features).1
            })
            .collect();
        let probabilities = softmax(&logits)?;
        let selected_action = actions[argmax(&logits)];
        let value = self.value(&context);
        Ok(ScoredSideboardDecisionV1 {
            ordered_actions: actions,
            logits,
            probabilities,
            selected_action,
            value,
        })
    }

    pub fn deliberate_v1(
        &self,
        input: &LearnedSideboardInputV1,
        configuration: &DeckConfigurationV1,
        embeddings: &FrozenSideboardEmbeddingsV1<'_>,
    ) -> ResultV1<LearnedSideboardResultV1> {
        let mut state = SideboardDeliberationStateV1::new_v1(configuration);
        let mut actions = Vec::new();
        let mut initial_value = 0.0;
        for step in 0..SIDEBOARD_MAX_DECISIONS_V1 {
            let decision = self.score_v1(input, &state, embeddings)?;
            if step == 0 {
                initial_value = decision.value;
            }
            state.apply_v1(decision.selected_action)?;
            actions.push(decision.selected_action);
            if state.is_done_v1() {
                return Ok(LearnedSideboardResultV1 {
                    configuration: state.configuration_v1()?,
                    actions,
                    initial_value,
                });
            }
        }
        Err(error("sideboard deliberation exceeded decision limit"))
    }

    pub fn train_imitation_v1(
        &mut self,
        examples: &[SideboardImitationExampleV1],
        embeddings: &FrozenSideboardEmbeddingsV1<'_>,
        config: SideboardTrainingConfigV1,
    ) -> ResultV1<SideboardTrainingMetricsV1> {
        self.check_embeddings(embeddings)?;
        if examples.is_empty()
            || config.epochs == 0
            || config.epochs > 100_000
            || !config.learning_rate.is_finite()
            || !(0.0..=1.0).contains(&config.learning_rate)
            || config.learning_rate == 0.0
            || !config.value_loss_weight.is_finite()
            || !(0.0..=100.0).contains(&config.value_loss_weight)
        {
            return Err(error("invalid imitation training configuration"));
        }
        // Validate the entire teacher set before changing any weights.
        let prepared = examples
            .iter()
            .map(|example| prepare_example(example, embeddings))
            .collect::<ResultV1<Vec<_>>>()?;
        let initial = self.measure(&prepared)?;
        let mut candidate = self.clone();
        for _ in 0..config.epochs {
            for example in &prepared {
                for decision in &example.decisions {
                    candidate.policy_step(decision, config.learning_rate)?;
                    candidate.training_steps = candidate
                        .training_steps
                        .checked_add(1)
                        .ok_or_else(|| error("training step overflow"))?;
                }
                // A plan's measured value belongs to its initial configuration,
                // not to every teacher-forced intermediate card movement.
                if let Some(target) = example.value {
                    let prediction = candidate.value(&example.context);
                    let gradient = (2.0
                        * (prediction - target)
                        * (1.0 - prediction * prediction)
                        * config.value_loss_weight)
                        .clamp(-10.0, 10.0);
                    for (weight, feature) in
                        candidate.value_weights.iter_mut().zip(&example.context)
                    {
                        *weight -= config.learning_rate * gradient * feature;
                    }
                    candidate.value_bias -= config.learning_rate * gradient;
                }
            }
        }
        candidate.validate()?;
        let final_measure = candidate.measure(&prepared)?;
        let metrics = SideboardTrainingMetricsV1 {
            examples: examples.len(),
            decisions: initial.decisions,
            initial_cross_entropy: initial.cross_entropy,
            final_cross_entropy: final_measure.cross_entropy,
            initial_action_accuracy: initial.accuracy,
            final_action_accuracy: final_measure.accuracy,
            value_targets: final_measure.value_targets,
            final_value_mse: final_measure.value_mse,
        };
        *self = candidate;
        Ok(metrics)
    }

    fn validate(&self) -> ResultV1<()> {
        validate_identity(&self.play_identity)?;
        if self.schema != SCHEMA
            || !is_hex(&self.embedding_sha256, 64)
            || self.hidden_weights.len() != HIDDEN * FEATURES
            || self.hidden_bias.len() != HIDDEN
            || self.policy_weights.len() != HIDDEN
            || self.value_weights.len() != CONTEXT
            || self
                .hidden_weights
                .iter()
                .chain(&self.hidden_bias)
                .chain(&self.policy_weights)
                .chain(&self.value_weights)
                .chain(std::iter::once(&self.value_bias))
                .any(|v| !v.is_finite() || v.abs() > 1e6)
        {
            return Err(error(
                "invalid learned sideboard checkpoint shape or numerical values",
            ));
        }
        Ok(())
    }
    fn check_embeddings(&self, embeddings: &FrozenSideboardEmbeddingsV1<'_>) -> ResultV1<()> {
        self.validate()?;
        if self.play_identity != embeddings.identity
            || self.embedding_sha256 != embeddings.table_sha256
        {
            return Err(error(
                "learned sideboard checkpoint and frozen play embeddings disagree",
            ));
        }
        Ok(())
    }
    fn forward(&self, features: &[f64]) -> (Vec<f64>, f64) {
        let hidden: Vec<f64> = self
            .hidden_weights
            .chunks_exact(FEATURES)
            .zip(&self.hidden_bias)
            .map(|(weights, bias)| (dot(weights, features) + bias).tanh())
            .collect();
        let logit = dot(&hidden, &self.policy_weights);
        (hidden, logit)
    }
    fn value(&self, context: &[f64]) -> f64 {
        (dot(&self.value_weights, context) + self.value_bias).tanh()
    }

    fn policy_step(&mut self, decision: &PreparedDecision, learning_rate: f64) -> ResultV1<()> {
        let forward: Vec<_> = decision.features.iter().map(|f| self.forward(f)).collect();
        let logits: Vec<_> = forward.iter().map(|(_, logit)| *logit).collect();
        let probabilities = softmax(&logits)?;
        let mut weight_gradient = vec![0.0; HIDDEN * FEATURES];
        let mut bias_gradient = [0.0; HIDDEN];
        let mut policy_gradient = [0.0; HIDDEN];
        for (action, ((hidden, _), features)) in forward.iter().zip(&decision.features).enumerate()
        {
            let residual = probabilities[action] - indicator(action == decision.target);
            for h in 0..HIDDEN {
                policy_gradient[h] += residual * hidden[h];
                let gradient = residual * self.policy_weights[h] * (1.0 - hidden[h] * hidden[h]);
                bias_gradient[h] += gradient;
                for feature in 0..FEATURES {
                    weight_gradient[h * FEATURES + feature] += gradient * features[feature];
                }
            }
        }
        let norm = weight_gradient
            .iter()
            .chain(&bias_gradient)
            .chain(&policy_gradient)
            .map(|g| g * g)
            .sum::<f64>()
            .sqrt();
        if !norm.is_finite() {
            return Err(error("non-finite imitation gradient"));
        }
        let step = learning_rate * if norm > 10.0 { 10.0 / norm } else { 1.0 };
        for (weight, gradient) in self.hidden_weights.iter_mut().zip(weight_gradient) {
            *weight -= step * gradient;
        }
        for h in 0..HIDDEN {
            self.hidden_bias[h] -= step * bias_gradient[h];
            self.policy_weights[h] -= step * policy_gradient[h];
        }
        Ok(())
    }

    fn measure(&self, examples: &[PreparedExample]) -> ResultV1<Measured> {
        let mut loss = 0.0;
        let mut correct = 0;
        let mut decisions = 0;
        let mut value_error = 0.0;
        let mut value_targets = 0;
        for example in examples {
            for decision in &example.decisions {
                let logits: Vec<_> = decision
                    .features
                    .iter()
                    .map(|f| self.forward(f).1)
                    .collect();
                let maximum = logits[argmax(&logits)];
                loss += maximum
                    + logits
                        .iter()
                        .map(|logit| (logit - maximum).exp())
                        .sum::<f64>()
                        .ln()
                    - logits[decision.target];
                correct += usize::from(argmax(&logits) == decision.target);
                decisions += 1;
            }
            if let Some(target) = example.value {
                value_error += (self.value(&example.context) - target).powi(2);
                value_targets += 1;
            }
        }
        if !loss.is_finite() || !value_error.is_finite() {
            return Err(error("non-finite imitation metrics"));
        }
        Ok(Measured {
            decisions,
            cross_entropy: loss / decisions as f64,
            accuracy: correct as f64 / decisions as f64,
            value_targets,
            value_mse: (value_targets > 0).then(|| value_error / value_targets as f64),
        })
    }
}

struct PreparedDecision {
    features: Vec<Vec<f64>>,
    target: usize,
}
struct PreparedExample {
    decisions: Vec<PreparedDecision>,
    context: Vec<f64>,
    value: Option<f64>,
}
struct Measured {
    decisions: usize,
    cross_entropy: f64,
    accuracy: f64,
    value_targets: usize,
    value_mse: Option<f64>,
}

fn prepare_example(
    example: &SideboardImitationExampleV1,
    embeddings: &FrozenSideboardEmbeddingsV1<'_>,
) -> ResultV1<PreparedExample> {
    if example.target_actions.is_empty()
        || example.target_actions.len() > SIDEBOARD_MAX_DECISIONS_V1
        || example
            .target_actions
            .windows(2)
            .any(|pair| pair[0] > pair[1])
        || example
            .target_value
            .is_some_and(|v| !v.is_finite() || !(-1.0..=1.0).contains(&v))
    {
        return Err(error(
            "invalid canonical teacher sequence or measured value target",
        ));
    }
    let configuration = DeckConfigurationV1::new_exact_v1(
        example.initial_mainboard.clone(),
        example.initial_sideboard.clone(),
    )
    .map_err(|e| error(e.to_string()))?;
    let mut state = SideboardDeliberationStateV1::new_v1(&configuration);
    let initial_context = encode_context(&example.input, &state, embeddings)?;
    let mut decisions = Vec::new();
    for &target in &example.target_actions {
        let context = encode_context(&example.input, &state, embeddings)?;
        let actions = state.legal_actions_v1();
        let target_index = actions
            .iter()
            .position(|a| *a == target)
            .ok_or_else(|| error(format!("teacher supplied illegal action {target:?}")))?;
        let features = actions
            .iter()
            .map(|&action| action_features(&example.input, &state, embeddings, &context, action))
            .collect();
        decisions.push(PreparedDecision {
            features,
            target: target_index,
        });
        state.apply_v1(target)?;
    }
    if !state.is_done_v1() {
        return Err(error("teacher sequence must terminate with Done"));
    }
    state.configuration_v1()?;
    Ok(PreparedExample {
        decisions,
        context: initial_context,
        value: example.target_value.map(f64::from),
    })
}

fn validate_input(
    input: &LearnedSideboardInputV1,
    state: &SideboardDeliberationStateV1,
) -> ResultV1<()> {
    if input.next_game_number < 2
        || input.acting_player_games_won > 2
        || input.opponent_games_won > 2
    {
        return Err(error("invalid post-board match context"));
    }
    let mut registered = BTreeMap::new();
    for row in &input.registered_cards {
        if row.count == 0 || registered.insert(row.card_id, row.count).is_some() {
            return Err(error("registered card counts must be positive and unique"));
        }
    }
    if registered != state.registered {
        return Err(error("input registered 75 differs from configuration"));
    }
    let prior_game = |game| game > 0 && game < input.next_game_number;
    let mut own_keys = BTreeSet::new();
    for outcome in &input.own_card_outcomes {
        if !registered.contains_key(&outcome.card_id)
            || !prior_game(outcome.game_index)
            || !own_keys.insert((outcome.game_index, outcome.card_id))
        {
            return Err(error(
                "own outcomes must refer to unique registered card and completed game pairs",
            ));
        }
    }
    if input
        .opponent_evidence
        .iter()
        .any(|row| !prior_game(row.game_index))
    {
        return Err(error("opponent evidence must come from completed games"));
    }
    let mut resource_games = BTreeSet::new();
    for resource in &input.resource_summaries {
        if !prior_game(resource.game_index)
            || !resource_games.insert(resource.game_index)
            || resource
                .own_lands_mean
                .into_iter()
                .chain(resource.own_hand_mean)
                .any(|v| !v.is_finite() || v < 0.0)
        {
            return Err(error(
                "invalid or duplicated completed-game resource summary",
            ));
        }
    }
    Ok(())
}

fn encode_context(
    input: &LearnedSideboardInputV1,
    state: &SideboardDeliberationStateV1,
    embeddings: &FrozenSideboardEmbeddingsV1<'_>,
) -> ResultV1<Vec<f64>> {
    validate_input(input, state)?;
    let mut context = Vec::with_capacity(CONTEXT);
    context.extend(pool_cards(&state.registered, embeddings));
    context.extend(pool_cards(&state.main, embeddings));
    context.extend(pool_cards(&state.side, embeddings));
    // Sort before accumulating floats so row order cannot affect inference or
    // checkpoint training. Repeated visible copies remain repeated evidence.
    let mut evidence: Vec<_> = input.opponent_evidence.iter().collect();
    evidence.sort_by_key(|row| (row.game_index, row.card_id, row.first_seen_turn, row.zone));
    let mut opponent = [0.0; 16];
    let mut positional_opponent = [0.0; 16];
    let mut evidence_meta = [0.0; 9];
    for row in &evidence {
        let positional = position(row.game_index);
        for (i, value) in embeddings.row(row.card_id).iter().enumerate() {
            let feature = f64::from(*value).tanh();
            opponent[i] += feature;
            positional_opponent[i] += feature * positional;
        }
        evidence_meta[0] += (f64::from(row.first_seen_turn) / 20.0).tanh();
        evidence_meta[1] += indicator(row.card_id.is_none());
        let zone = match row.zone {
            VisibleEvidenceZoneV1::Battlefield => 0,
            VisibleEvidenceZoneV1::Stack => 1,
            VisibleEvidenceZoneV1::Graveyard => 2,
            VisibleEvidenceZoneV1::Exile => 3,
            VisibleEvidenceZoneV1::Hand => 4,
            VisibleEvidenceZoneV1::OtherVisible => 5,
        };
        evidence_meta[3 + zone] += 1.0;
    }
    let evidence_n = evidence.len().max(1) as f64;
    context.extend(opponent.map(|x| x / evidence_n));
    context.extend(positional_opponent.map(|x| x / evidence_n));
    let mut outcomes: Vec<_> = input.own_card_outcomes.iter().collect();
    outcomes.sort_by_key(|row| (row.game_index, row.card_id));
    let mut pooled_outcomes = [0.0; 6];
    let mut positional_outcomes = [0.0; 6];
    for row in &outcomes {
        let features = outcome_features(row);
        for i in 0..6 {
            pooled_outcomes[i] += features[i];
            positional_outcomes[i] += features[i] * position(row.game_index);
        }
    }
    let outcome_n = outcomes.len().max(1) as f64;
    context.extend(pooled_outcomes.map(|x| x / outcome_n));
    context.extend(positional_outcomes.map(|x| x / outcome_n));
    let mut resources: Vec<_> = input.resource_summaries.iter().collect();
    resources.sort_by_key(|row| row.game_index);
    let mut pooled_resources = [0.0; 12];
    let mut positional_resources = [0.0; 12];
    for row in &resources {
        let raw = [
            row.own_lands_mean.map(|x| f64::from(x) / 8.0),
            row.own_hand_mean.map(|x| f64::from(x) / 8.0),
            row.own_life_final.map(|x| f64::from(x) / 20.0),
            row.opponent_life_final.map(|x| f64::from(x) / 20.0),
            row.own_damage_dealt.map(|x| x as f64 / 20.0),
            row.own_damage_taken.map(|x| x as f64 / 20.0),
        ];
        for (i, value) in raw.iter().enumerate() {
            let feature = value.map_or(0.0, f64::tanh);
            pooled_resources[2 * i] += feature;
            pooled_resources[2 * i + 1] += indicator(value.is_some());
            positional_resources[2 * i] += feature * position(row.game_index);
            positional_resources[2 * i + 1] +=
                indicator(value.is_some()) * position(row.game_index);
        }
    }
    let resource_n = resources.len().max(1) as f64;
    context.extend(pooled_resources.map(|x| x / resource_n));
    context.extend(positional_resources.map(|x| x / resource_n));
    context.extend([
        f64::from(input.next_game_number) / 255.0,
        f64::from(input.acting_player_games_won) / 2.0,
        f64::from(input.opponent_games_won) / 2.0,
        (60 - total(&state.main)) as f64 / 15.0,
        state.out_count as f64 / 15.0,
    ]);
    for feature in &mut evidence_meta {
        *feature /= evidence_n;
    }
    evidence_meta[2] = (evidence.len() as f64 / 20.0).tanh();
    context.extend(evidence_meta);
    debug_assert_eq!(context.len(), CONTEXT);
    Ok(context)
}

fn action_features(
    input: &LearnedSideboardInputV1,
    state: &SideboardDeliberationStateV1,
    embeddings: &FrozenSideboardEmbeddingsV1<'_>,
    context: &[f64],
    action: SideboardActionV1,
) -> Vec<f64> {
    let (card, kind) = match action {
        SideboardActionV1::MoveOneToSideboard { card_id } => (Some(card_id), 0),
        SideboardActionV1::MoveOneToMainboard { card_id } => (Some(card_id), 1),
        SideboardActionV1::Done => (None, 2),
    };
    let mut features = Vec::with_capacity(FEATURES);
    features.extend_from_slice(context);
    features.extend(embeddings.row(card).iter().map(|v| f64::from(*v).tanh()));
    let mut outcomes: Vec<_> = input
        .own_card_outcomes
        .iter()
        .filter(|row| Some(row.card_id) == card)
        .collect();
    outcomes.sort_by_key(|row| row.game_index);
    let mut pooled = [0.0; 12];
    for row in &outcomes {
        let values = outcome_features(row);
        for i in 0..6 {
            pooled[i] += values[i];
            pooled[6 + i] += values[i] * position(row.game_index);
        }
    }
    features.extend(pooled.map(|x| x / outcomes.len().max(1) as f64));
    for i in 0..3 {
        features.push(indicator(kind == i));
    }
    features.extend([
        card.map_or(0.0, |id| {
            f64::from(*state.main.get(&id).unwrap_or(&0)) / 60.0
        }),
        card.map_or(0.0, |id| {
            f64::from(*state.side.get(&id).unwrap_or(&0)) / 30.0
        }),
        card.map_or(0.0, |id| {
            f64::from(*state.registered.get(&id).unwrap_or(&0)) / 75.0
        }),
    ]);
    debug_assert_eq!(features.len(), FEATURES);
    features
}

fn outcome_features(row: &SideboardOwnCardOutcomeV1) -> [f64; 6] {
    [
        (f64::from(row.times_drawn) / 8.0).tanh(),
        indicator(row.cast),
        indicator(row.stuck_in_hand),
        indicator(row.died_without_dealing_damage),
        indicator(row.removal_no_target),
        indicator(row.counterspell_held),
    ]
}

fn pool_cards(
    cards: &BTreeMap<u16, u8>,
    embeddings: &FrozenSideboardEmbeddingsV1<'_>,
) -> [f64; 16] {
    let mut pooled = [0.0; 16];
    for (&card, &count) in cards {
        for (i, value) in embeddings.row(Some(card)).iter().enumerate() {
            pooled[i] += f64::from(count) * f64::from(*value).tanh();
        }
    }
    let n = total(cards).max(1) as f64;
    pooled.map(|x| x / n)
}
fn position(game_index: u8) -> f64 {
    f64::from(game_index) / (f64::from(game_index) + 1.0)
}
fn indicator(value: bool) -> f64 {
    if value {
        1.0
    } else {
        0.0
    }
}
fn counts(cards: &[u16]) -> BTreeMap<u16, u8> {
    let mut counts = BTreeMap::new();
    for &card in cards {
        *counts.entry(card).or_insert(0) += 1;
    }
    counts
}
fn total(cards: &BTreeMap<u16, u8>) -> usize {
    cards.values().map(|&n| usize::from(n)).sum()
}
fn expand(cards: &BTreeMap<u16, u8>) -> Vec<u16> {
    cards
        .iter()
        .flat_map(|(&id, &n)| std::iter::repeat_n(id, usize::from(n)))
        .collect()
}
fn move_one(from: &mut BTreeMap<u16, u8>, to: &mut BTreeMap<u16, u8>, card: u16) {
    let count = from.get_mut(&card).expect("legal action source");
    *count -= 1;
    if *count == 0 {
        from.remove(&card);
    }
    *to.entry(card).or_insert(0) += 1;
}
fn is_hex(value: &str, length: usize) -> bool {
    value.len() == length && value.bytes().all(|b| b.is_ascii_hexdigit())
}
fn validate_identity(identity: &SideboardPlayIdentityV1) -> ResultV1<()> {
    if !is_hex(&identity.weights_sha256, 64) || !is_hex(&identity.git_head, 40) {
        return Err(error(
            "expected full SHA256 weights hash and full git commit identity",
        ));
    }
    Ok(())
}
fn dot(left: &[f64], right: &[f64]) -> f64 {
    left.iter().zip(right).map(|(a, b)| a * b).sum()
}
fn argmax(values: &[f64]) -> usize {
    let mut best = 0;
    for i in 1..values.len() {
        if values[i] > values[best] {
            best = i;
        }
    }
    best
}
fn softmax(logits: &[f64]) -> ResultV1<Vec<f64>> {
    if logits.is_empty() || logits.iter().any(|x| !x.is_finite()) {
        return Err(error("non-finite or empty policy logits"));
    }
    let maximum = logits[argmax(logits)];
    let mut probabilities: Vec<_> = logits.iter().map(|x| (x - maximum).exp()).collect();
    let denominator: f64 = probabilities.iter().sum();
    for probability in &mut probabilities {
        *probability /= denominator;
    }
    Ok(probabilities)
}
struct SeededRng(u64);
impl SeededRng {
    fn weight(&mut self, scale: f64) -> f64 {
        self.0 = self.0.wrapping_add(0x9e3779b97f4a7c15);
        let mut bits = self.0;
        bits = (bits ^ (bits >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
        bits = (bits ^ (bits >> 27)).wrapping_mul(0x94d049bb133111eb);
        bits ^= bits >> 31;
        (((bits >> 11) as f64 / (1_u64 << 53) as f64) * 2.0 - 1.0) * scale
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_table() -> Vec<f32> {
        let mut table = vec![0.0; VOCAB_SIZE * 16];
        // Synthetic embeddings are confined to these implementation tests.
        for card in 0..10 {
            table[(card + 1) * 16 + card] = 2.0;
            table[(card + 1) * 16 + 15] = card as f32 / 10.0;
        }
        table
    }
    fn identity() -> SideboardPlayIdentityV1 {
        SideboardPlayIdentityV1 {
            weights_sha256: "a".repeat(64),
            git_head: "b".repeat(40),
        }
    }
    fn configuration() -> DeckConfigurationV1 {
        DeckConfigurationV1::new_exact_v1(vec![0; 60], [vec![1; 7], vec![2; 8]].concat()).unwrap()
    }
    fn input(configuration: &DeckConfigurationV1, opponent: u16) -> LearnedSideboardInputV1 {
        LearnedSideboardInputV1 {
            registered_cards: configuration.combined_card_counts_v1(),
            own_card_outcomes: Vec::new(),
            resource_summaries: Vec::new(),
            opponent_evidence: vec![SideboardOpponentEvidenceV1 {
                card_id: Some(opponent),
                game_index: 1,
                first_seen_turn: 2,
                zone: VisibleEvidenceZoneV1::Battlefield,
            }],
            next_game_number: 2,
            acting_player_games_won: 1,
            opponent_games_won: 0,
        }
    }

    #[test]
    fn legality_preserves_75_and_cannot_submit_or_cycle_mid_exchange() {
        let registered = configuration();
        let mut state = SideboardDeliberationStateV1::new_v1(&registered);
        assert_eq!(
            state.legal_actions_v1(),
            vec![
                SideboardActionV1::MoveOneToSideboard { card_id: 0 },
                SideboardActionV1::Done
            ]
        );
        state
            .apply_v1(SideboardActionV1::MoveOneToSideboard { card_id: 0 })
            .unwrap();
        assert!(state.configuration_v1().is_err());
        assert!(state.apply_v1(SideboardActionV1::Done).is_err());
        assert!(state
            .apply_v1(SideboardActionV1::MoveOneToMainboard { card_id: 0 })
            .is_err());
        state
            .apply_v1(SideboardActionV1::MoveOneToMainboard { card_id: 1 })
            .unwrap();
        assert_eq!(state.legal_actions_v1(), vec![SideboardActionV1::Done]);
        state.apply_v1(SideboardActionV1::Done).unwrap();
        let result = state.configuration_v1().unwrap();
        assert_eq!(
            result.combined_card_counts_v1(),
            registered.combined_card_counts_v1()
        );
        assert_eq!(result.mainboard().iter().filter(|&&id| id == 1).count(), 1);
        assert!(state.apply_v1(SideboardActionV1::Done).is_err());
        // When both zones contain only the same card, no nonredundant exchange
        // exists. Removing it must not strand the policy at an invalid size.
        let same = DeckConfigurationV1::new_exact_v1(vec![0; 60], vec![0; 15]).unwrap();
        assert_eq!(
            SideboardDeliberationStateV1::new_v1(&same).legal_actions_v1(),
            vec![SideboardActionV1::Done]
        );
    }

    #[test]
    fn untrained_policies_finish_within_31_decisions() {
        let table = fixture_table();
        let embeddings = FrozenSideboardEmbeddingsV1::new_v1(&table, identity()).unwrap();
        let configuration = DeckConfigurationV1::new_exact_v1(
            [vec![0; 30], vec![1; 30]].concat(),
            [vec![1; 5], vec![2; 5], vec![3; 5]].concat(),
        )
        .unwrap();
        let input = input(&configuration, 4);
        for seed in 0..12 {
            let model = LearnedSideboardModelV1::new_v1(seed, &embeddings);
            let result = model
                .deliberate_v1(&input, &configuration, &embeddings)
                .unwrap();
            assert!(result.actions.len() <= 31);
            assert_eq!(result.actions.last(), Some(&SideboardActionV1::Done));
            assert_eq!(
                result.configuration.combined_card_counts_v1(),
                configuration.combined_card_counts_v1()
            );
        }
    }

    #[test]
    fn game_three_targets_are_deltas_from_the_current_configuration() {
        let registered = configuration();
        let mut game_two = SideboardDeliberationStateV1::new_v1(&registered);
        game_two
            .apply_v1(SideboardActionV1::MoveOneToSideboard { card_id: 0 })
            .unwrap();
        game_two
            .apply_v1(SideboardActionV1::MoveOneToMainboard { card_id: 1 })
            .unwrap();
        let current = game_two.configuration_v1().unwrap();
        let mut game_three = SideboardDeliberationStateV1::new_v1(&registered);
        game_three
            .apply_v1(SideboardActionV1::MoveOneToSideboard { card_id: 0 })
            .unwrap();
        game_three
            .apply_v1(SideboardActionV1::MoveOneToMainboard { card_id: 2 })
            .unwrap();
        let target = game_three.configuration_v1().unwrap();
        assert_eq!(
            actions_between_configurations_v1(&current, &target).unwrap(),
            vec![
                SideboardActionV1::MoveOneToSideboard { card_id: 1 },
                SideboardActionV1::MoveOneToMainboard { card_id: 2 },
                SideboardActionV1::Done,
            ]
        );
        assert_eq!(
            actions_between_configurations_v1(&current, &current).unwrap(),
            vec![SideboardActionV1::Done]
        );
    }

    #[test]
    fn exact_set_permutation_invariance_and_completed_game_position() {
        let table = fixture_table();
        let embeddings = FrozenSideboardEmbeddingsV1::new_v1(&table, identity()).unwrap();
        let configuration = configuration();
        let state = SideboardDeliberationStateV1::new_v1(&configuration);
        let mut original = input(&configuration, 3);
        original.next_game_number = 3;
        original
            .opponent_evidence
            .push(SideboardOpponentEvidenceV1 {
                card_id: Some(4),
                game_index: 2,
                first_seen_turn: 5,
                zone: VisibleEvidenceZoneV1::Graveyard,
            });
        original.own_card_outcomes = vec![
            SideboardOwnCardOutcomeV1 {
                card_id: 0,
                game_index: 1,
                times_drawn: 4,
                cast: true,
                stuck_in_hand: false,
                died_without_dealing_damage: false,
                removal_no_target: false,
                counterspell_held: false,
            },
            SideboardOwnCardOutcomeV1 {
                card_id: 1,
                game_index: 2,
                times_drawn: 2,
                cast: false,
                stuck_in_hand: true,
                died_without_dealing_damage: false,
                removal_no_target: true,
                counterspell_held: false,
            },
        ];
        original.resource_summaries = vec![
            SideboardGameResourceV1 {
                game_index: 1,
                own_lands_mean: None,
                own_hand_mean: Some(3.0),
                own_life_final: Some(5),
                opponent_life_final: Some(0),
                own_damage_dealt: Some(20),
                own_damage_taken: Some(15),
            },
            SideboardGameResourceV1 {
                game_index: 2,
                own_lands_mean: None,
                own_hand_mean: None,
                own_life_final: Some(0),
                opponent_life_final: Some(20),
                own_damage_dealt: Some(0),
                own_damage_taken: Some(20),
            },
        ];
        let mut permuted = original.clone();
        permuted.registered_cards.reverse();
        permuted.opponent_evidence.reverse();
        permuted.own_card_outcomes.reverse();
        permuted.resource_summaries.reverse();
        let model = LearnedSideboardModelV1::new_v1(17, &embeddings);
        assert_eq!(
            model.score_v1(&original, &state, &embeddings).unwrap(),
            model.score_v1(&permuted, &state, &embeddings).unwrap()
        );
        permuted.opponent_evidence[0].game_index = 1;
        permuted.opponent_evidence[1].game_index = 2;
        assert_ne!(
            encode_context(&original, &state, &embeddings).unwrap(),
            encode_context(&permuted, &state, &embeddings).unwrap()
        );
        permuted.registered_cards[0].count += 1;
        assert!(model.score_v1(&permuted, &state, &embeddings).is_err());
    }

    #[test]
    fn imitation_learns_distinct_visible_opponent_conditioned_exchanges() {
        let table = fixture_table();
        let embeddings = FrozenSideboardEmbeddingsV1::new_v1(&table, identity()).unwrap();
        let configuration = configuration();
        let examples: Vec<_> = [(3, 1), (4, 2)]
            .iter()
            .map(|&(opponent, incoming)| SideboardImitationExampleV1 {
                input: input(&configuration, opponent),
                initial_mainboard: configuration.mainboard().to_vec(),
                initial_sideboard: configuration.sideboard().to_vec(),
                target_actions: vec![
                    SideboardActionV1::MoveOneToSideboard { card_id: 0 },
                    SideboardActionV1::MoveOneToMainboard { card_id: incoming },
                    SideboardActionV1::Done,
                ],
                target_value: None,
            })
            .collect();
        let mut model = LearnedSideboardModelV1::new_v1(7, &embeddings);
        // Unlabeled examples cannot silently train a fabricated zero-value
        // target or decay an already learned value head.
        model.value_weights[0] = 0.42;
        model.value_bias = 0.1;
        let original_value_weights = model.value_weights.clone();
        let mut replay = model.clone();
        let config = SideboardTrainingConfigV1 {
            epochs: 400,
            learning_rate: 0.12,
            value_loss_weight: 0.0,
        };
        let metrics = model
            .train_imitation_v1(&examples, &embeddings, config)
            .unwrap();
        assert!(
            metrics.final_cross_entropy < metrics.initial_cross_entropy * 0.25,
            "{metrics:?}"
        );
        assert_eq!(metrics.final_action_accuracy, 1.0, "{metrics:?}");
        assert_eq!(metrics.final_value_mse, None);
        assert_eq!(model.value_weights, original_value_weights);
        assert_eq!(model.value_bias, 0.1);
        for example in &examples {
            let result = model
                .deliberate_v1(&example.input, &configuration, &embeddings)
                .unwrap();
            assert_eq!(result.actions, example.target_actions);
        }
        replay
            .train_imitation_v1(&examples, &embeddings, config)
            .unwrap();
        assert_eq!(model, replay);
        assert_eq!(
            LearnedSideboardModelV1::from_json_v1(&model.to_json_v1().unwrap()).unwrap(),
            model
        );
        assert_eq!(
            model.checkpoint_sha256_v1().unwrap(),
            replay.checkpoint_sha256_v1().unwrap()
        );
        assert_eq!(
            table,
            fixture_table(),
            "imitation must never alter play embeddings"
        );
    }

    #[test]
    fn malformed_teacher_and_checkpoint_are_rejected_without_partial_training() {
        let table = fixture_table();
        let embeddings = FrozenSideboardEmbeddingsV1::new_v1(&table, identity()).unwrap();
        let configuration = configuration();
        let mut model = LearnedSideboardModelV1::new_v1(8, &embeddings);
        let initial = model.clone();
        let invalid = SideboardImitationExampleV1 {
            input: input(&configuration, 3),
            initial_mainboard: configuration.mainboard().to_vec(),
            initial_sideboard: configuration.sideboard().to_vec(),
            target_actions: vec![
                SideboardActionV1::MoveOneToSideboard { card_id: 0 },
                SideboardActionV1::Done,
            ],
            target_value: None,
        };
        let config = SideboardTrainingConfigV1 {
            epochs: 1,
            learning_rate: 0.1,
            value_loss_weight: 0.0,
        };
        assert!(model
            .train_imitation_v1(&[invalid], &embeddings, config)
            .is_err());
        assert_eq!(initial, model);
        let mut encoded = serde_json::to_value(&model).unwrap();
        encoded["hidden_weights"] = serde_json::json!([0.0]);
        assert!(LearnedSideboardModelV1::from_json_v1(&encoded.to_string()).is_err());
        let mut changed = table.clone();
        changed[16] = 1.0;
        let other = FrozenSideboardEmbeddingsV1::new_v1(&changed, identity()).unwrap();
        assert!(model
            .score_v1(
                &input(&configuration, 3),
                &SideboardDeliberationStateV1::new_v1(&configuration),
                &other
            )
            .is_err());
        assert_eq!(embeddings.row(Some(0)), &table[16..32]);
        assert_eq!(embeddings.row(None), &[0.0; 16]);
    }

    #[test]
    fn supplied_nonzero_value_targets_fit_without_changing_a_forced_policy() {
        let table = fixture_table();
        let embeddings = FrozenSideboardEmbeddingsV1::new_v1(&table, identity()).unwrap();
        // With the same card in both zones, Done is the only legal action. The
        // regression isolates value learning from policy imitation gradients.
        let configuration = DeckConfigurationV1::new_exact_v1(vec![0; 60], vec![0; 15]).unwrap();
        let state = SideboardDeliberationStateV1::new_v1(&configuration);
        let examples: Vec<_> = [(3, 0.6_f32), (4, -0.4_f32)]
            .iter()
            .map(|&(opponent, target_value)| SideboardImitationExampleV1 {
                input: input(&configuration, opponent),
                initial_mainboard: configuration.mainboard().to_vec(),
                initial_sideboard: configuration.sideboard().to_vec(),
                target_actions: vec![SideboardActionV1::Done],
                target_value: Some(target_value),
            })
            .collect();
        let mut model = LearnedSideboardModelV1::new_v1(19, &embeddings);
        let initial_policy = (
            model.hidden_weights.clone(),
            model.hidden_bias.clone(),
            model.policy_weights.clone(),
        );
        let initial_mse = examples
            .iter()
            .map(|example| {
                let prediction = model
                    .score_v1(&example.input, &state, &embeddings)
                    .unwrap()
                    .value;
                (prediction - f64::from(example.target_value.unwrap())).powi(2)
            })
            .sum::<f64>()
            / examples.len() as f64;
        let metrics = model
            .train_imitation_v1(
                &examples,
                &embeddings,
                SideboardTrainingConfigV1 {
                    epochs: 120,
                    learning_rate: 0.05,
                    value_loss_weight: 1.0,
                },
            )
            .unwrap();
        assert_eq!(metrics.value_targets, 2);
        assert!(
            metrics.final_value_mse.unwrap() < initial_mse * 0.05,
            "{metrics:?}"
        );
        assert!(
            model
                .score_v1(&examples[0].input, &state, &embeddings)
                .unwrap()
                .value
                > 0.4
        );
        assert!(
            model
                .score_v1(&examples[1].input, &state, &embeddings)
                .unwrap()
                .value
                < -0.2
        );
        assert_eq!(
            initial_policy,
            (
                model.hidden_weights,
                model.hidden_bias,
                model.policy_weights
            )
        );
    }
}
