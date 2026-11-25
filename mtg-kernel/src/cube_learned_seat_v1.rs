//! Sampling and replay of the M4 pick and build paths.

use std::sync::Arc;

use crate::card_def::CARD_DEFS;
use crate::cube_draft_build_v1::model::{
    BuildForwardV1, CardHiddenTableV1, ModelCardRowsV1, ModelErrorV1, ModelV1, PickForwardV1,
    PickStateV1, TrunkV1, TYPE_SET_COUNT_V1,
};
use crate::cube_draft_v1::{basic_land_ids, Deck, DECK_CARDS, PACK_SIZE, POOL_CARDS, ROUNDS};
use crate::cube_seat_v1::{
    canonical_deck, BuildOutcome, BuildRows, DecisionKind, DecisionRow, PassDirection, PickOutcome,
    SeatBuilder, SeatError, SeatPickView, SeatPicker,
};
use crate::m4_sampler_v1::{sample_v1, GRID_SIZE, SAMPLER_ID_V1};
use crate::state::SplitMix64;

/// D2's five pick inputs. All three sets are in ascending registry order.
pub struct PickInputs<'a> {
    pub pack: &'a [u16],
    pub pool: &'a [u16],
    pub history: &'a [u16],
    pub step: [f32; POOL_CARDS],
    pub direction: f32,
}

/// Context before a pointer, type-set or split decision. The pointer step is
/// `chosen.len()`. At B = 40 the chosen set is empty for the type and split heads.
pub struct BuildInputs<'a> {
    pub pool: &'a [u16],
    pub chosen: &'a [u16],
    pub basics: u16,
}

/// Implement on a frozen D2 parameter snapshot. The forward uses portable
/// math, summing card rows in the supplied order and using zero for an empty
/// mean. Each head scores exactly the supplied options in their given order.
pub trait SeatForward: Send + Sync {
    fn pick_and_value(&self, input: &PickInputs<'_>) -> Result<(Vec<f32>, f32), SeatError>;
    fn count(&self, pool: &[u16]) -> Result<[f32; 41], SeatError>;
    fn pointer(&self, input: &BuildInputs<'_>, remaining: &[u16]) -> Result<Vec<f32>, SeatError>;
    fn type_set(&self, input: &BuildInputs<'_>, masks: &[u16]) -> Result<Vec<f32>, SeatError>;
    fn split(&self, input: &BuildInputs<'_>, bits: &[u16]) -> Result<Vec<f32>, SeatError>;
}

/// An immutable D2 model and its cached card representations for both trunks.
pub struct ModelSeatForward {
    model: Arc<ModelV1>,
    rows: Arc<ModelCardRowsV1>,
    pick_hidden: CardHiddenTableV1,
    build_hidden: CardHiddenTableV1,
}

impl ModelSeatForward {
    pub fn new(
        model: Arc<ModelV1>,
        rows: Arc<ModelCardRowsV1>,
        threads: usize,
    ) -> Result<Self, SeatError> {
        let pick_hidden = model
            .card_hidden_table(&rows, TrunkV1::Pick, threads)
            .map_err(model_error)?;
        let build_hidden = model
            .card_hidden_table(&rows, TrunkV1::Build, threads)
            .map_err(model_error)?;
        Ok(Self {
            model,
            rows,
            pick_hidden,
            build_hidden,
        })
    }

    fn build_forward(&self) -> Result<BuildForwardV1<'_>, SeatError> {
        BuildForwardV1::from_cached_hidden(&self.model, &self.rows, &self.build_hidden)
            .map_err(model_error)
    }
}

fn model_error(error: ModelErrorV1) -> SeatError {
    SeatError::Seat(error.to_string())
}

impl SeatForward for ModelSeatForward {
    fn pick_and_value(&self, input: &PickInputs<'_>) -> Result<(Vec<f32>, f32), SeatError> {
        let step = input
            .step
            .iter()
            .position(|&entry| entry == 1.0)
            .ok_or_else(|| invalid("pick step must be one-hot"))?;
        if input
            .step
            .iter()
            .enumerate()
            .any(|(i, &value)| i != step && value != 0.0)
        {
            return Err(invalid("pick step must be one-hot"));
        }
        let direction = match input.direction {
            1.0 => 1,
            -1.0 => -1,
            _ => return Err(invalid("pick direction must be +1 or -1")),
        };
        let state = PickStateV1 {
            pack: input.pack.to_vec(),
            pool: input.pool.to_vec(),
            history: input.history.to_vec(),
            step: step as u8,
            direction,
        };
        let forward = PickForwardV1::from_cached_hidden(&self.model, &self.rows, &self.pick_hidden)
            .map_err(model_error)?;
        let logits = forward.pick_logits_v1(&state).map_err(model_error)?;
        if logits.ids != input.pack {
            return Err(invalid("pick logits do not match the pack order"));
        }
        let value = forward.value_v1(&state).map_err(model_error)?;
        Ok((logits.logits, value))
    }

    fn count(&self, pool: &[u16]) -> Result<[f32; 41], SeatError> {
        self.build_forward()?
            .build_context_v1(pool)
            .map_err(model_error)?
            .count_logits_v1()
            .try_into()
            .map_err(|_| invalid("expected 41 basics-count logits"))
    }

    fn pointer(&self, input: &BuildInputs<'_>, remaining: &[u16]) -> Result<Vec<f32>, SeatError> {
        let logits = self
            .build_forward()?
            .build_context_v1(input.pool)
            .map_err(model_error)?
            .pointer_logits_v1(usize::from(input.basics), input.chosen, remaining)
            .map_err(model_error)?;
        if logits.ids != remaining {
            return Err(invalid(
                "pointer logits do not match the remaining card order",
            ));
        }
        Ok(logits.logits)
    }

    fn type_set(&self, input: &BuildInputs<'_>, masks: &[u16]) -> Result<Vec<f32>, SeatError> {
        if masks.iter().any(|&mask| {
            !(1..=TYPE_SET_COUNT_V1 as u16).contains(&mask)
                || mask.count_ones() > u32::from(input.basics)
        }) {
            return Err(invalid("infeasible basic type mask"));
        }
        let logits = self
            .build_forward()?
            .build_context_v1(input.pool)
            .map_err(model_error)?
            .type_context_v1(usize::from(input.basics), input.chosen)
            .map_err(model_error)?
            .typeset_logits_v1();
        Ok(masks
            .iter()
            .map(|&mask| logits[usize::from(mask) - 1])
            .collect())
    }

    fn split(&self, input: &BuildInputs<'_>, bits: &[u16]) -> Result<Vec<f32>, SeatError> {
        if bits.iter().any(|&bit| !matches!(bit, 1 | 2 | 4 | 8 | 16))
            || bits.windows(2).any(|pair| pair[0] >= pair[1])
        {
            return Err(invalid(
                "split types must be single basic bits in ascending order",
            ));
        }
        let mask = bits.iter().fold(0u8, |mask, &bit| mask | bit as u8);
        let logits = self
            .build_forward()?
            .build_context_v1(input.pool)
            .map_err(model_error)?
            .type_context_v1(usize::from(input.basics), input.chosen)
            .map_err(model_error)?
            .split_logits_v1(mask)
            .map_err(model_error)?;
        Ok(logits.logits)
    }
}

pub struct LearnedSeatPicker {
    params: Arc<dyn SeatForward>,
}

impl LearnedSeatPicker {
    pub fn new(params: Arc<dyn SeatForward>) -> Self {
        Self { params }
    }
}

pub struct LearnedSeatBuilder {
    params: Arc<dyn SeatForward>,
}

impl LearnedSeatBuilder {
    pub fn new(params: Arc<dyn SeatForward>) -> Self {
        Self { params }
    }
}

fn invalid(message: &str) -> SeatError {
    SeatError::Seat(message.to_owned())
}

fn sorted_cards(cards: &[u16]) -> Vec<u16> {
    let mut sorted = cards.to_vec();
    sorted.sort_unstable();
    sorted
}

struct Decision {
    index: usize,
    row: DecisionRow,
    log_prob: f64,
}

/// A specified choice scores an observed path under new parameters. Replay
/// leaves it unset so that the recorded word must reproduce the choice.
fn decide(
    kind: DecisionKind,
    options: &[u16],
    logits: &[f32],
    value: Option<f32>,
    word: u64,
    specified: Option<u16>,
) -> Result<Decision, SeatError> {
    if logits.len() != options.len() {
        return Err(invalid("forward must supply one finite logit per option"));
    }
    if value.is_some_and(|v| !v.is_finite()) {
        return Err(invalid("nonfinite pick value"));
    }
    // Every vector is compacted before this call, including type-set masks.
    let enabled = vec![true; options.len()];
    let sample = sample_v1(logits, &enabled, word)
        .map_err(|error| SeatError::Seat(format!("learned sampler: {error}")))?;
    let index = match specified {
        Some(chosen) => options
            .binary_search(&chosen)
            .map_err(|_| invalid("infeasible choice"))?,
        None => sample.selected_index,
    };
    let log_prob = sample.log_probabilities[index];
    let probability = sample.counts[index] as f64 / GRID_SIZE as f64;
    Ok(Decision {
        index,
        log_prob,
        row: DecisionRow {
            kind,
            sampler: SAMPLER_ID_V1.into(),
            options: options.to_vec(),
            chosen: options[index],
            stream_values: vec![word],
            logits_bits: Some(logits.iter().map(|x| x.to_bits()).collect()),
            p_chosen_bits: Some(probability.to_bits()),
            value_bits: value.map(f32::to_bits),
        },
    })
}

fn recorded_word(row: &DecisionRow) -> Result<u64, SeatError> {
    match row.stream_values.as_slice() {
        [word] => Ok(*word),
        _ => Err(invalid("expected one recorded stream value")),
    }
}

fn pick_with(
    params: &dyn SeatForward,
    view: &SeatPickView<'_>,
    word: impl FnOnce() -> Result<u64, SeatError>,
) -> Result<PickOutcome, SeatError> {
    let round = usize::from(view.round);
    let pick = usize::from(view.pick);
    if round >= ROUNDS || pick >= PACK_SIZE || view.pack.len() != PACK_SIZE - pick {
        return Err(invalid("invalid draft step or pack size"));
    }
    if view.pack.len() == 1 {
        return Ok(PickOutcome {
            index: 0,
            row: None,
        });
    }
    let pack = sorted_cards(view.pack);
    let mut step = [0.0; POOL_CARDS];
    step[round * PACK_SIZE + pick] = 1.0;
    let input = PickInputs {
        pack: &pack,
        pool: &sorted_cards(view.pool),
        history: &sorted_cards(view.seen_not_taken),
        step,
        direction: match view.direction {
            PassDirection::Left => 1.0,
            PassDirection::Right => -1.0,
        },
    };
    let (logits, value) = params.pick_and_value(&input)?;
    let decision = decide(
        DecisionKind::Pick,
        &pack,
        &logits,
        Some(value),
        word()?,
        None,
    )?;
    let index = view
        .pack
        .iter()
        .position(|&id| id == decision.row.chosen)
        .ok_or_else(|| invalid("chosen card missing from pack"))?;
    Ok(PickOutcome {
        index,
        row: Some(decision.row),
    })
}

impl SeatPicker for LearnedSeatPicker {
    fn pick(
        &mut self,
        view: &SeatPickView<'_>,
        stream: &mut SplitMix64,
    ) -> Result<PickOutcome, SeatError> {
        pick_with(self.params.as_ref(), view, || Ok(stream.next_u64()))
    }
}

pub fn replay_pick(
    row: &DecisionRow,
    params: &dyn SeatForward,
    view: &SeatPickView<'_>,
) -> Result<(), SeatError> {
    let replay = pick_with(params, view, || recorded_word(row))?;
    if replay.row.as_ref() != Some(row) {
        return Err(invalid("pick row differs on replay"));
    }
    Ok(())
}

#[derive(Clone, Copy)]
enum Step {
    Count,
    Pointer(usize),
    TypeSet,
    Split(usize),
}

impl Step {
    fn kind(self) -> DecisionKind {
        match self {
            Self::Count => DecisionKind::Count,
            Self::Pointer(_) => DecisionKind::Pointer,
            Self::TypeSet => DecisionKind::TypeSet,
            Self::Split(_) => DecisionKind::Split,
        }
    }

    fn recorded(self, rows: &BuildRows) -> Result<&DecisionRow, SeatError> {
        match self {
            Self::Count => Some(&rows.count),
            Self::Pointer(i) => rows.pointer.get(i),
            Self::TypeSet => rows.type_set.as_ref(),
            Self::Split(i) => rows.split.get(i),
        }
        .ok_or_else(|| invalid("missing build decision"))
    }
}

struct BuildPath {
    deck: Deck,
    rows: BuildRows,
    log_prob: f64,
}

/// All three consumers use this walk so every forward sees the same prefix.
fn walk_build(
    params: &dyn SeatForward,
    pool: &[u16],
    mut choose: impl FnMut(Step, &[u16], &[f32]) -> Result<Decision, SeatError>,
) -> Result<BuildPath, SeatError> {
    let pool = sorted_cards(pool);
    if pool.len() != POOL_CARDS
        || pool.windows(2).any(|w| w[0] == w[1])
        || pool.iter().any(|&id| usize::from(id) >= CARD_DEFS.len())
    {
        return Err(invalid("expected 45 distinct registry cards"));
    }
    let counts: Vec<_> = (0..=DECK_CARDS as u16).collect();
    let count = choose(Step::Count, &counts, &params.count(&pool)?)?;
    let basics = count.row.chosen;
    let mut log_prob = count.log_prob;
    let mut rows = BuildRows {
        count: count.row,
        pointer: Vec::new(),
        type_set: None,
        split: Vec::new(),
    };
    let mut chosen = Vec::with_capacity(DECK_CARDS as usize);
    let mut remaining = pool.clone();
    for j in 0..DECK_CARDS as usize - usize::from(basics) {
        let state = BuildInputs {
            pool: &pool,
            chosen: &chosen,
            basics,
        };
        let logits = params.pointer(&state, &remaining)?;
        let decision = choose(Step::Pointer(j), &remaining, &logits)?;
        let card = remaining.remove(decision.index);
        chosen.insert(chosen.partition_point(|&id| id < card), card);
        log_prob += decision.log_prob;
        rows.pointer.push(decision.row);
    }

    if basics > 0 {
        let state = BuildInputs {
            pool: &pool,
            chosen: &chosen,
            basics,
        };
        let masks: Vec<_> = (1u16..32)
            .filter(|m| m.count_ones() <= u32::from(basics))
            .collect();
        let decision = choose(Step::TypeSet, &masks, &params.type_set(&state, &masks)?)?;
        let set = decision.row.chosen;
        log_prob += decision.log_prob;
        rows.type_set = Some(decision.row);
        let bits: Vec<u16> = [1, 2, 4, 8, 16]
            .into_iter()
            .filter(|&bit| set & bit != 0)
            .collect();
        let extra = usize::from(basics) - bits.len();
        let mut copies = vec![1usize; bits.len()];
        if bits.len() == 1 {
            copies[0] += extra;
        } else if extra > 0 {
            let logits = params.split(&state, &bits)?;
            let mut log_q = vec![0.0; bits.len()];
            for j in 0..extra {
                let decision = choose(Step::Split(j), &bits, &logits)?;
                copies[decision.index] += 1;
                log_q[decision.index] = decision.log_prob;
                rows.split.push(decision.row);
            }
            // Sum the split by type in canonical order, independent of draw order.
            let split_log_prob = copies
                .iter()
                .zip(log_q)
                .fold(0.0, |sum, (&n, q)| sum + (n - 1) as f64 * q);
            log_prob += split_log_prob;
        }
        let ids = basic_land_ids()?;
        for (bit, n) in bits.into_iter().zip(copies) {
            chosen.extend(std::iter::repeat_n(ids[bit.trailing_zeros() as usize], n));
        }
    }
    let deck = canonical_deck(&chosen);
    deck.validate(&pool)?;
    Ok(BuildPath {
        deck,
        rows,
        log_prob,
    })
}

impl SeatBuilder for LearnedSeatBuilder {
    fn build(&mut self, pool: &[u16], stream: &mut SplitMix64) -> Result<BuildOutcome, SeatError> {
        let path = walk_build(self.params.as_ref(), pool, |step, options, logits| {
            decide(step.kind(), options, logits, None, stream.next_u64(), None)
        })?;
        Ok(BuildOutcome {
            deck: path.deck,
            rows: Some(path.rows),
        })
    }
}

/// Recompute the forward and redraw from every word, comparing the entire row
/// tree (including optional fields and any missing or trailing decisions).
pub fn replay_build(
    rows: &BuildRows,
    params: &dyn SeatForward,
    pool: &[u16],
) -> Result<(), SeatError> {
    let path = walk_build(params, pool, |step, options, logits| {
        decide(
            step.kind(),
            options,
            logits,
            None,
            recorded_word(step.recorded(rows)?)?,
            None,
        )
    })?;
    if &path.rows != rows {
        return Err(invalid("build rows differ on replay"));
    }
    Ok(())
}

/// Score the recorded choices at the supplied parameters, which can differ
/// from collection. Uses D2a's f64 log probabilities directly. No
/// multinomial coefficient or permutation sum enters the score.
pub fn try_build_path_log_prob(
    rows: &BuildRows,
    params: &dyn SeatForward,
    pool: &[u16],
) -> Result<f64, SeatError> {
    let path = walk_build(params, pool, |step, options, logits| {
        let row = step.recorded(rows)?;
        if row.kind != step.kind() || row.sampler != SAMPLER_ID_V1 || row.options != options {
            return Err(invalid("inconsistent recorded build options"));
        }
        decide(
            step.kind(),
            options,
            logits,
            None,
            recorded_word(row)?,
            Some(row.chosen),
        )
    })?;
    if rows.pointer.len() != path.rows.pointer.len()
        || rows.split.len() != path.rows.split.len()
        || rows.type_set.is_some() != path.rows.type_set.is_some()
    {
        return Err(invalid("extra build decisions"));
    }
    Ok(path.log_prob)
}

/// Scalar loss input for a validated path. Invalid input yields NaN.
/// Use `try_build_path_log_prob` when ingesting fallible data.
pub fn build_path_log_prob(rows: &BuildRows, params: &dyn SeatForward, pool: &[u16]) -> f64 {
    try_build_path_log_prob(rows, params, pool).unwrap_or(f64::NAN)
}

#[cfg(test)]
mod tests;
