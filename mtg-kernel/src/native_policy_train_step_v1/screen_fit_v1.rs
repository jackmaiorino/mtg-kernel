//! The B pre-screen's value-fitting entry point (collab
//! LANES/throughput-architecture-20261020/WIDE-BACKWARD-DESIGN.md v2.1,
//! section 3 PR 4a; B memo, pre-screen). It encodes the win/loss-only law
//! instead of leaving it to the caller:
//!
//! - **Targets.** The only value target is a finished game's terminal
//!   return for the acting seat, derived here from the game record's
//!   terminal outcome ([`TerminalReturnTargetV1`] has no other constructor).
//!   A game that did not end naturally (truncated, halted, decision cap) or
//!   whose outcome, winner and rewards disagree is refused: there is no
//!   bootstrapped, shaped or searched target to fall back on. The config's
//!   `value_target_source` must name `terminal_return_v1`; any other source
//!   is refused, and no entry point takes caller-supplied targets.
//! - **Policy term.** Fixed off: every advantage is +0, so the GAE
//!   objective reduces to `value_coefficient * mean((value - target)^2)`.
//!   Every step checks that its policy sum is exactly zero. Held-out policy
//!   loss (selected-action negative log-likelihood) is reported as a
//!   description only; the narrow parent generated those actions.
//! - **Provenance.** The report records every fit and held-out game id, the
//!   SHA-256 of the outcome source (ids, outcomes and acting seats), of the
//!   decision inputs and of the minibatch order, and the initial and final
//!   state hashes.
//! - **Arithmetic.** Each step is one update of the generic CPU GAE train
//!   step at the state's own width (net-8 or net-8w128). The expected output
//!   bits each update replays come from `forward_with_tape_dims_v1` on the
//!   current parameters, which is the forward the step recomputes. Held-out
//!   evaluation runs through `forward_with_tape_dims_v1` too.
//! - **Parameters.** Step count, minibatch size, learning rate, value
//!   coefficient, minibatch seed, held-out cadence and forward workers come
//!   from the research lead's frozen screen note. They are explicit, every
//!   one is required (no defaults, unknown fields refused) and validated.
//!
//! **Minibatch order.** The unit is one physical decision group of the fit
//! set, in record order (game, then group). A SplitMix64 stream seeded with
//! `minibatch_seed` draws one Fisher-Yates shuffle of the identity order per
//! epoch (for `i` from `n - 1` down to 1, `j = (next_u64 * (i + 1)) >> 64`,
//! swap `i` and `j`). Batches are consecutive runs of `batch_groups` groups
//! of the concatenated epochs, so a batch may straddle an epoch boundary.
//!
//! The screen driver (`expanded_deck_training_v1::b_width_prescreen_v1`)
//! calls this entry point.
#![cfg_attr(not(test), allow(dead_code))]

use super::*;
use crate::expanded_deck_training_v1::encoded_decision_view_generic_v1;
use crate::native_flat_tensorizer_v2::NativeFlatDecisionTensorV2;
use crate::rl::{
    terminal_tuple_is_valid_v1, PlayerSeatV1, TerminalClassificationV1, TerminalOutcomeV1,
    TerminalSafeCodeV2,
};
use crate::rl_session::RlSessionTerminalV1;
use crate::sideboard_play_policy_v1::FreshLineageGenerationV1;
use crate::state::SplitMix64;
use serde::{Deserialize, Serialize};

pub(crate) const SCREEN_VALUE_FIT_ENTRY_VERSION_V1: &str = "native-screen-value-fit-v1";
/// The only accepted value target source.
pub(crate) const TERMINAL_RETURN_TARGET_SOURCE_V1: &str = "terminal_return_v1";
pub(crate) const SCREEN_POLICY_TERM_V1: &str = "off-every-advantage-positive-zero-v1";
const OUTCOME_SOURCE_DOMAIN_V1: &[u8] = b"mtg-kernel-screen-value-fit-outcome-source-v1";
const DECISION_SOURCE_DOMAIN_V1: &[u8] = b"mtg-kernel-screen-value-fit-decision-source-v1";
const MINIBATCH_ORDER_DOMAIN_V1: &[u8] = b"mtg-kernel-screen-value-fit-minibatch-order-v1";

/// A recorded game's terminal outcome, as the session recorded it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct ScreenGameOutcomeV1 {
    pub(crate) terminal_outcome: TerminalOutcomeV1,
    pub(crate) terminal_classification: TerminalClassificationV1,
    pub(crate) terminal_code: TerminalSafeCodeV2,
    pub(crate) winner: Option<PlayerSeatV1>,
    pub(crate) terminal_reward: [i32; 2],
}

impl From<&RlSessionTerminalV1> for ScreenGameOutcomeV1 {
    fn from(terminal: &RlSessionTerminalV1) -> Self {
        Self {
            terminal_outcome: terminal.terminal_outcome,
            terminal_classification: terminal.terminal_classification,
            terminal_code: terminal.terminal_code,
            winner: terminal.winner,
            terminal_reward: terminal.terminal_reward,
        }
    }
}

/// One substep of a recorded decision group: the encoded decision and the
/// action the behavior policy took.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ScreenSubstepV1 {
    pub(crate) tensor: NativeFlatDecisionTensorV2,
    pub(crate) selected_action_index: usize,
}

/// One physical decision of one seat.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ScreenDecisionGroupV1 {
    pub(crate) actor: u8,
    pub(crate) substeps: Vec<ScreenSubstepV1>,
}

/// One recorded game: its id, its terminal outcome and its decision groups.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ScreenGameRecordV1 {
    pub(crate) game_id: String,
    pub(crate) outcome: ScreenGameOutcomeV1,
    pub(crate) groups: Vec<ScreenDecisionGroupV1>,
}

/// A value target that only a naturally finished game's terminal return can
/// produce. It has no other constructor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TerminalReturnTargetV1 {
    terminal_return: i8,
}

impl TerminalReturnTargetV1 {
    /// The terminal return of `actor` in `game`, refused unless the game
    /// ended naturally with a self-consistent outcome.
    pub(crate) fn of_game_v1(
        game: &ScreenGameRecordV1,
        actor: u8,
    ) -> Result<Self, ScreenFitErrorV1> {
        let outcome = &game.outcome;
        let terminal = outcome.terminal_classification == TerminalClassificationV1::Natural
            && outcome.terminal_code == TerminalSafeCodeV2::NaturalGameOver
            && terminal_tuple_is_valid_v1(
                outcome.terminal_outcome,
                outcome.terminal_classification,
                outcome.winner,
                outcome.terminal_reward,
            );
        if !terminal {
            return Err(ScreenFitErrorV1::NonTerminalOutcome {
                game_id: game.game_id.clone(),
            });
        }
        if actor > 1 {
            return Err(ScreenFitErrorV1::InvalidActor {
                game_id: game.game_id.clone(),
                actor,
            });
        }
        let terminal_return = match outcome.terminal_reward[usize::from(actor)] {
            -1 => -1,
            0 => 0,
            1 => 1,
            _ => {
                return Err(ScreenFitErrorV1::NonTerminalOutcome {
                    game_id: game.game_id.clone(),
                })
            }
        };
        Ok(Self { terminal_return })
    }

    pub(crate) fn terminal_return_v1(self) -> i8 {
        self.terminal_return
    }

    pub(crate) fn value_target_v1(self) -> f32 {
        f32::from(self.terminal_return)
    }
}

/// The screen note's fitting constants. Every field is required; unknown
/// fields are refused.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ScreenFitConfigV1 {
    /// Must be [`TERMINAL_RETURN_TARGET_SOURCE_V1`].
    pub(crate) value_target_source: String,
    /// Train-step updates, at least 1.
    pub(crate) steps: u64,
    /// Decision groups per update, at least 1 and at most the fit set.
    pub(crate) batch_groups: usize,
    /// Finite and positive.
    pub(crate) learning_rate: f32,
    /// Finite and positive (the only loss weight in effect).
    pub(crate) value_coefficient: f32,
    pub(crate) minibatch_seed: u64,
    /// Held-out evaluation after every this many updates (and after the
    /// last); at least 1 and at most `steps`.
    pub(crate) heldout_every_steps: u64,
    /// Forward (and exact parallel backward) workers per update, 1 to 64.
    /// Bits do not depend on it.
    pub(crate) forward_workers: usize,
}

impl ScreenFitConfigV1 {
    pub(crate) fn validate_v1(&self) -> Result<(), ScreenFitErrorV1> {
        if self.value_target_source != TERMINAL_RETURN_TARGET_SOURCE_V1 {
            return Err(ScreenFitErrorV1::UnsupportedValueTargetSource {
                requested: self.value_target_source.clone(),
            });
        }
        let invalid = |field: &'static str| Err(ScreenFitErrorV1::InvalidConfig { field });
        if self.steps == 0 {
            return invalid("steps");
        }
        if self.batch_groups == 0 {
            return invalid("batch_groups");
        }
        if !self.learning_rate.is_finite() || self.learning_rate <= 0.0 {
            return invalid("learning_rate");
        }
        if !self.value_coefficient.is_finite() || self.value_coefficient <= 0.0 {
            return invalid("value_coefficient");
        }
        if self.heldout_every_steps == 0 || self.heldout_every_steps > self.steps {
            return invalid("heldout_every_steps");
        }
        if !(1..=64).contains(&self.forward_workers) {
            return invalid("forward_workers");
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ScreenFitErrorV1 {
    UnsupportedValueTargetSource {
        requested: String,
    },
    InvalidConfig {
        field: &'static str,
    },
    EmptyGameSet {
        split: &'static str,
    },
    DuplicateGameId {
        game_id: String,
    },
    NonTerminalOutcome {
        game_id: String,
    },
    InvalidActor {
        game_id: String,
        actor: u8,
    },
    EmptyGame {
        game_id: String,
    },
    EmptyGroup {
        game_id: String,
        group: usize,
    },
    BatchLargerThanFitSet {
        batch_groups: usize,
        fit_groups: usize,
    },
    PolicyTermNotOff {
        step: u64,
    },
    Train(NativePolicyTrainErrorV1),
}

impl Display for ScreenFitErrorV1 {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "native screen value fit v1 error: {self:?}")
    }
}

impl Error for ScreenFitErrorV1 {}

impl From<NativePolicyTrainErrorV1> for ScreenFitErrorV1 {
    fn from(value: NativePolicyTrainErrorV1) -> Self {
        Self::Train(value)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub(crate) struct ScreenFitStepV1 {
    pub(crate) step: u64,
    pub(crate) loss: f32,
    pub(crate) value_sum: f32,
    pub(crate) policy_sum: f32,
    pub(crate) adam_step: u64,
}

/// One held-out game's sums, for whole-game resampling.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub(crate) struct ScreenHeldoutGameV1 {
    pub(crate) game_id: String,
    pub(crate) groups: usize,
    /// Sum over the game's groups of `(value - terminal_return)^2`, in f64.
    pub(crate) value_squared_error_sum: f64,
    /// Sum over the game's groups of `-sum(selected log-probabilities)`, in
    /// f64. Descriptive only.
    pub(crate) policy_nll_sum: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub(crate) struct ScreenHeldoutEvalV1 {
    /// Updates completed before this evaluation (0 is the starting state).
    pub(crate) after_step: u64,
    /// Held-out value loss: terminal-return MSE over every held-out group.
    pub(crate) value_mse: f64,
    /// Held-out policy loss per group (descriptive only).
    pub(crate) policy_nll_mean: f64,
    pub(crate) games: Vec<ScreenHeldoutGameV1>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub(crate) struct ScreenFitReportV1 {
    pub(crate) entry_version: &'static str,
    pub(crate) architecture: &'static str,
    pub(crate) input_generation: &'static str,
    pub(crate) value_target_source: &'static str,
    pub(crate) policy_term: &'static str,
    pub(crate) config: ScreenFitConfigV1,
    pub(crate) learning_rate_bits: u32,
    pub(crate) value_coefficient_bits: u32,
    pub(crate) initial_state_sha256: String,
    pub(crate) final_state_sha256: String,
    pub(crate) initial_adam_step: u64,
    pub(crate) final_adam_step: u64,
    pub(crate) fit_game_ids: Vec<String>,
    pub(crate) heldout_game_ids: Vec<String>,
    pub(crate) fit_group_count: usize,
    pub(crate) heldout_group_count: usize,
    pub(crate) outcome_source_sha256: String,
    pub(crate) decision_source_sha256: String,
    pub(crate) minibatch_order_sha256: String,
    pub(crate) steps: Vec<ScreenFitStepV1>,
    pub(crate) heldout: Vec<ScreenHeldoutEvalV1>,
}

fn generation_name_v1(generation: FreshLineageGenerationV1) -> &'static str {
    match generation {
        FreshLineageGenerationV1::V3 => "v3",
        FreshLineageGenerationV1::V4 => "v4",
    }
}

fn input_config_v1<N: TrainableNetV1>(
    model: &N,
    generation: FreshLineageGenerationV1,
) -> NativePolicyValueModelConfigV1 {
    match generation {
        FreshLineageGenerationV1::V3 => model.feature_transfer_config_v3(),
        FreshLineageGenerationV1::V4 => model.feature_transfer_config_v4(),
    }
}

/// One epoch-wise Fisher-Yates minibatch stream (module documentation).
struct MinibatchStreamV1 {
    rng: SplitMix64,
    order: Vec<usize>,
    cursor: usize,
}

impl MinibatchStreamV1 {
    fn new_v1(seed: u64, groups: usize) -> Self {
        let mut stream = Self {
            rng: SplitMix64::seed(seed),
            order: Vec::new(),
            cursor: 0,
        };
        stream.shuffle_v1(groups);
        stream
    }

    fn shuffle_v1(&mut self, groups: usize) {
        self.order = (0..groups).collect();
        for i in (1..groups).rev() {
            let j = ((u128::from(self.rng.next_u64()) * (i as u128 + 1)) >> 64) as usize;
            self.order.swap(i, j);
        }
        self.cursor = 0;
    }

    fn next_batch_v1(&mut self, size: usize) -> Vec<usize> {
        let mut batch = Vec::with_capacity(size);
        while batch.len() < size {
            if self.cursor == self.order.len() {
                self.shuffle_v1(self.order.len());
            }
            batch.push(self.order[self.cursor]);
            self.cursor += 1;
        }
        batch
    }
}

/// The minibatch groups of the first `steps` updates (module documentation).
pub(crate) fn screen_minibatch_order_v1(
    seed: u64,
    fit_groups: usize,
    batch_groups: usize,
    steps: u64,
) -> Vec<Vec<usize>> {
    let mut stream = MinibatchStreamV1::new_v1(seed, fit_groups);
    (0..steps)
        .map(|_| stream.next_batch_v1(batch_groups))
        .collect()
}

fn hash_bytes_v1(hasher: &mut Sha256, bytes: &[u8]) {
    hasher.update((bytes.len() as u64).to_be_bytes());
    hasher.update(bytes);
}

fn hash_f32s_v1(hasher: &mut Sha256, values: &[f32]) {
    hasher.update((values.len() as u64).to_be_bytes());
    for value in values {
        hasher.update(value.to_bits().to_be_bytes());
    }
}

fn hash_i64s_v1(hasher: &mut Sha256, values: &[i64]) {
    hasher.update((values.len() as u64).to_be_bytes());
    for value in values {
        hasher.update(value.to_be_bytes());
    }
}

/// The outcome source: per split, per game in order, its id, canonical
/// outcome JSON and each group's acting seat.
pub(crate) fn outcome_source_sha256_v1(splits: [(&str, &[ScreenGameRecordV1]); 2]) -> String {
    let mut hasher = Sha256::new();
    hash_bytes_v1(&mut hasher, OUTCOME_SOURCE_DOMAIN_V1);
    for (split, games) in splits {
        hash_bytes_v1(&mut hasher, split.as_bytes());
        hasher.update((games.len() as u64).to_be_bytes());
        for game in games {
            hash_bytes_v1(&mut hasher, game.game_id.as_bytes());
            let outcome = serde_json::to_vec(&game.outcome).expect("outcome serializes");
            hash_bytes_v1(&mut hasher, &outcome);
            let actors: Vec<u8> = game.groups.iter().map(|group| group.actor).collect();
            hash_bytes_v1(&mut hasher, &actors);
        }
    }
    widening_v1::hex_sha256_v1(hasher.finalize().into())
}

/// The decision inputs: per split, per game, per group and substep, every
/// tensor field's bits and the selected action.
pub(crate) fn decision_source_sha256_v1(splits: [(&str, &[ScreenGameRecordV1]); 2]) -> String {
    let mut hasher = Sha256::new();
    hash_bytes_v1(&mut hasher, DECISION_SOURCE_DOMAIN_V1);
    for (split, games) in splits {
        hash_bytes_v1(&mut hasher, split.as_bytes());
        for game in games {
            hash_bytes_v1(&mut hasher, game.game_id.as_bytes());
            hasher.update((game.groups.len() as u64).to_be_bytes());
            for group in &game.groups {
                hasher.update([group.actor]);
                hasher.update((group.substeps.len() as u64).to_be_bytes());
                for substep in &group.substeps {
                    let t = &substep.tensor;
                    hasher.update((substep.selected_action_index as u64).to_be_bytes());
                    hash_f32s_v1(&mut hasher, &t.state);
                    hash_f32s_v1(&mut hasher, &t.object_features);
                    hash_i64s_v1(&mut hasher, &t.object_card_ids);
                    hash_i64s_v1(&mut hasher, &t.object_groups);
                    hash_i64s_v1(&mut hasher, &t.object_node_ids);
                    hash_f32s_v1(&mut hasher, &t.edge_features);
                    hash_i64s_v1(&mut hasher, &t.edge_source_indices);
                    hash_i64s_v1(&mut hasher, &t.edge_target_indices);
                    hash_f32s_v1(&mut hasher, &t.action_features);
                    hash_f32s_v1(&mut hasher, &t.action_ref_features);
                    hash_i64s_v1(&mut hasher, &t.action_ref_card_ids);
                    hash_i64s_v1(&mut hasher, &t.action_ref_action_indices);
                    hash_i64s_v1(&mut hasher, &t.action_ref_node_indices);
                }
            }
        }
    }
    widening_v1::hex_sha256_v1(hasher.finalize().into())
}

/// Checks one split's games and derives each group's terminal-return target,
/// in record order: `(game, group, target)`.
fn split_targets_v1(
    split: &'static str,
    games: &[ScreenGameRecordV1],
) -> Result<Vec<(usize, usize, TerminalReturnTargetV1)>, ScreenFitErrorV1> {
    if games.is_empty() {
        return Err(ScreenFitErrorV1::EmptyGameSet { split });
    }
    let mut targets = Vec::new();
    for (game_index, game) in games.iter().enumerate() {
        if game.groups.is_empty() {
            return Err(ScreenFitErrorV1::EmptyGame {
                game_id: game.game_id.clone(),
            });
        }
        for (group_index, group) in game.groups.iter().enumerate() {
            if group.substeps.is_empty() {
                return Err(ScreenFitErrorV1::EmptyGroup {
                    game_id: game.game_id.clone(),
                    group: group_index,
                });
            }
            let target = TerminalReturnTargetV1::of_game_v1(game, group.actor)?;
            targets.push((game_index, group_index, target));
        }
    }
    Ok(targets)
}

/// Held-out value and policy losses of the current parameters, through
/// `forward_with_tape_dims_v1`. A group's value is its first substep's, as
/// in the train step.
pub(crate) fn evaluate_screen_heldout_dims_v1<D: NetDimsV1>(
    state: &NativePolicyValueTrainStateDimsV1<D>,
    generation: FreshLineageGenerationV1,
    games: &[ScreenGameRecordV1],
    after_step: u64,
) -> Result<ScreenHeldoutEvalV1, ScreenFitErrorV1> {
    let parameters = state.model_dims_v1().parameter_snapshot_v1();
    let config = input_config_v1(state.model_dims_v1(), generation);
    let math = state.training_math_dims_v1();
    let mut total_groups = 0usize;
    let mut value_total = 0.0f64;
    let mut policy_total = 0.0f64;
    let mut per_game = Vec::with_capacity(games.len());
    for game in games {
        let mut value_sum = 0.0f64;
        let mut policy_sum = 0.0f64;
        for (group_index, group) in game.groups.iter().enumerate() {
            let target = TerminalReturnTargetV1::of_game_v1(game, group.actor)?;
            let mut joint_log_probability = 0.0f64;
            let mut value = None;
            for (substep_index, substep) in group.substeps.iter().enumerate() {
                let view = encoded_decision_view_generic_v1(&substep.tensor, generation);
                let tape = forward_with_tape_dims_v1::<D>(&parameters, config, view, math)?;
                if substep.selected_action_index >= tape.logits_v1().len() {
                    return Err(NativePolicyTrainErrorV1::SelectedActionOutOfRange {
                        group_index,
                        substep_index,
                        selected: substep.selected_action_index,
                        action_count: tape.logits_v1().len(),
                    }
                    .into());
                }
                let (log_probability, _) = selected_log_softmax_math_v1(
                    tape.logits_v1(),
                    substep.selected_action_index,
                    math,
                )?;
                joint_log_probability += f64::from(log_probability);
                value.get_or_insert(tape.value_v1());
            }
            let error =
                f64::from(value.expect("nonempty group")) - f64::from(target.value_target_v1());
            value_sum += error * error;
            policy_sum -= joint_log_probability;
        }
        total_groups += game.groups.len();
        value_total += value_sum;
        policy_total += policy_sum;
        per_game.push(ScreenHeldoutGameV1 {
            game_id: game.game_id.clone(),
            groups: game.groups.len(),
            value_squared_error_sum: value_sum,
            policy_nll_sum: policy_sum,
        });
    }
    Ok(ScreenHeldoutEvalV1 {
        after_step,
        value_mse: value_total / total_groups as f64,
        policy_nll_mean: policy_total / total_groups as f64,
        games: per_game,
    })
}

/// The entry's forward (`forward_with_tape_dims_v1`) on one state's current
/// parameters, for the screen driver's step-0 function-preservation check:
/// the same parameters, input configuration and math the held-out
/// evaluation uses.
pub(crate) struct ScreenForwardV1<D: NetDimsV1> {
    parameters: Vec<NativeNamedParameterV1>,
    config: NativePolicyValueModelConfigV1,
    math: TrainingMathV1,
    generation: FreshLineageGenerationV1,
    dims: std::marker::PhantomData<D>,
}

impl<D: NetDimsV1> ScreenForwardV1<D> {
    pub(crate) fn of_state_v1(
        state: &NativePolicyValueTrainStateDimsV1<D>,
        generation: FreshLineageGenerationV1,
    ) -> Self {
        Self {
            parameters: state.model_dims_v1().parameter_snapshot_v1(),
            config: input_config_v1(state.model_dims_v1(), generation),
            math: state.training_math_dims_v1(),
            generation,
            dims: std::marker::PhantomData,
        }
    }

    /// The raw policy logit bits and the value bits of one decision input.
    pub(crate) fn output_bits_v1(
        &self,
        tensor: &NativeFlatDecisionTensorV2,
    ) -> Result<(Vec<u32>, u32), ScreenFitErrorV1> {
        let view = encoded_decision_view_generic_v1(tensor, self.generation);
        let tape = forward_with_tape_dims_v1::<D>(&self.parameters, self.config, view, self.math)?;
        Ok((
            tape.logits_v1()
                .iter()
                .map(|value| value.to_bits())
                .collect(),
            tape.value_v1().to_bits(),
        ))
    }
}

/// The pre-screen fit (module documentation): `config.steps` updates of the
/// generic CPU GAE train step on minibatches of `fit_games`, value targets
/// the terminal returns, every advantage +0, with held-out evaluation on
/// `heldout_games` before the first update, every `heldout_every_steps`
/// updates and after the last. Fit and held-out games are split by whole
/// game: a game id may appear once across both sets.
pub(crate) fn fit_screen_value_dims_v1<D: NetDimsV1>(
    state: &mut NativePolicyValueTrainStateDimsV1<D>,
    generation: FreshLineageGenerationV1,
    fit_games: &[ScreenGameRecordV1],
    heldout_games: &[ScreenGameRecordV1],
    config: &ScreenFitConfigV1,
) -> Result<ScreenFitReportV1, ScreenFitErrorV1> {
    config.validate_v1()?;
    let mut seen = std::collections::BTreeSet::new();
    for game in fit_games.iter().chain(heldout_games) {
        if !seen.insert(game.game_id.as_str()) {
            return Err(ScreenFitErrorV1::DuplicateGameId {
                game_id: game.game_id.clone(),
            });
        }
    }
    let fit_targets = split_targets_v1("fit", fit_games)?;
    let heldout_targets = split_targets_v1("heldout", heldout_games)?;
    if config.batch_groups > fit_targets.len() {
        return Err(ScreenFitErrorV1::BatchLargerThanFitSet {
            batch_groups: config.batch_groups,
            fit_groups: fit_targets.len(),
        });
    }
    let splits = [("fit", fit_games), ("heldout", heldout_games)];
    let outcome_source_sha256 = outcome_source_sha256_v1(splits);
    let decision_source_sha256 = decision_source_sha256_v1(splits);
    let initial_state_sha256 = widening_v1::hex_sha256_v1(state.state_sha256_dims_v1()?);
    let initial_adam_step = state.adam_step_dims_v1();

    let mut heldout = vec![evaluate_screen_heldout_dims_v1(
        state,
        generation,
        heldout_games,
        0,
    )?];
    let mut steps = Vec::with_capacity(config.steps as usize);
    let mut minibatches = MinibatchStreamV1::new_v1(config.minibatch_seed, fit_targets.len());
    let mut order_hasher = Sha256::new();
    hash_bytes_v1(&mut order_hasher, MINIBATCH_ORDER_DOMAIN_V1);
    for step in 1..=config.steps {
        let batch = minibatches.next_batch_v1(config.batch_groups);
        for &flat in &batch {
            order_hasher.update((flat as u64).to_be_bytes());
        }
        let parameters = state.model_dims_v1().parameter_snapshot_v1();
        let input_config = input_config_v1(state.model_dims_v1(), generation);
        let math = state.training_math_dims_v1();
        let mut recorded: Vec<Vec<(Vec<u32>, u32)>> = Vec::with_capacity(batch.len());
        for &flat in &batch {
            let (game, group, _) = fit_targets[flat];
            let mut outputs = Vec::new();
            for substep in &fit_games[game].groups[group].substeps {
                let view = encoded_decision_view_generic_v1(&substep.tensor, generation);
                let tape = forward_with_tape_dims_v1::<D>(&parameters, input_config, view, math)?;
                outputs.push((
                    tape.logits_v1()
                        .iter()
                        .map(|value| value.to_bits())
                        .collect(),
                    tape.value_v1().to_bits(),
                ));
            }
            recorded.push(outputs);
        }
        let substeps: Vec<Vec<NativePolicySubstepV1<'_>>> = batch
            .iter()
            .zip(&recorded)
            .map(|(&flat, outputs)| {
                let (game, group, _) = fit_targets[flat];
                fit_games[game].groups[group]
                    .substeps
                    .iter()
                    .zip(outputs)
                    .map(|(substep, (logits, value))| NativePolicySubstepV1 {
                        forward: NativePolicyForwardInputV1::Encoded(Box::new(
                            encoded_decision_view_generic_v1(&substep.tensor, generation),
                        )),
                        selected_action_index: substep.selected_action_index,
                        expected_raw_action_logit_bits: logits,
                        expected_value_bits: *value,
                    })
                    .collect()
            })
            .collect();
        let groups: Vec<NativePolicyPhysicalDecisionV1<'_>> = batch
            .iter()
            .zip(&substeps)
            .map(|(&flat, substeps)| NativePolicyPhysicalDecisionV1 {
                substeps,
                terminal_return: fit_targets[flat].2.terminal_return_v1(),
                baseline_bits: 0,
            })
            .collect();
        let value_targets: Vec<f32> = batch
            .iter()
            .map(|&flat| fit_targets[flat].2.value_target_v1())
            .collect();
        let advantages = vec![0.0f32; batch.len()];
        let result = state.train_step_gae_feature_transfer_profiled_dims_v1(
            generation,
            &groups,
            &value_targets,
            &advantages,
            config.value_coefficient,
            config.learning_rate,
            None,
            config.forward_workers,
            &mut NativeTrainingPhaseRecorderV1::disabled_v1(),
        )?;
        if result.policy_sum != 0.0 {
            return Err(ScreenFitErrorV1::PolicyTermNotOff { step });
        }
        steps.push(ScreenFitStepV1 {
            step,
            loss: result.loss,
            value_sum: result.value_sum,
            policy_sum: result.policy_sum,
            adam_step: result.adam_step,
        });
        if step % config.heldout_every_steps == 0 || step == config.steps {
            heldout.push(evaluate_screen_heldout_dims_v1(
                state,
                generation,
                heldout_games,
                step,
            )?);
        }
    }
    Ok(ScreenFitReportV1 {
        entry_version: SCREEN_VALUE_FIT_ENTRY_VERSION_V1,
        architecture: D::ARCHITECTURE,
        input_generation: generation_name_v1(generation),
        value_target_source: TERMINAL_RETURN_TARGET_SOURCE_V1,
        policy_term: SCREEN_POLICY_TERM_V1,
        config: config.clone(),
        learning_rate_bits: config.learning_rate.to_bits(),
        value_coefficient_bits: config.value_coefficient.to_bits(),
        initial_state_sha256,
        final_state_sha256: widening_v1::hex_sha256_v1(state.state_sha256_dims_v1()?),
        initial_adam_step,
        final_adam_step: state.adam_step_dims_v1(),
        fit_game_ids: fit_games.iter().map(|game| game.game_id.clone()).collect(),
        heldout_game_ids: heldout_games
            .iter()
            .map(|game| game.game_id.clone())
            .collect(),
        fit_group_count: fit_targets.len(),
        heldout_group_count: heldout_targets.len(),
        outcome_source_sha256,
        decision_source_sha256,
        minibatch_order_sha256: widening_v1::hex_sha256_v1(order_hasher.finalize().into()),
        steps,
        heldout,
    })
}
