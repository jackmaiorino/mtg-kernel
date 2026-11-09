//! The S learner's independent forward and adjoint pass records the terms
//! of each parameter gradient. Replay keeps each parameter's original
//! substep and row order, including additions to shared embeddings.

use super::profile::{self, DrainV1, StageV1};
use super::*;
use std::ops::Range;
use std::sync::Mutex;
use std::thread;

struct LinearTermV1 {
    index: LinearIdxV1,
    rows: usize,
    x: Range<usize>,
    dy: Range<usize>,
}

#[derive(Default)]
pub(super) struct SGradientTermsV1 {
    data: Vec<f32>,
    linears: Vec<LinearTermV1>,
    additions: Vec<(usize, Range<usize>)>,
}

impl SGradientTermsV1 {
    fn reset_v1(&mut self) {
        self.data.clear();
        self.linears.clear();
        self.additions.clear();
    }

    fn store_v1(&mut self, values: &[f32]) -> Range<usize> {
        let start = self.data.len();
        self.data.extend_from_slice(values);
        start..self.data.len()
    }

    /// Live value bytes, live term-record bytes and reserved bytes of these
    /// records (measurement only).
    fn bytes_v1(&self) -> (usize, usize, usize) {
        use std::mem::size_of;
        let addition = size_of::<(usize, Range<usize>)>();
        (
            self.data.len() * size_of::<f32>(),
            self.linears.len() * size_of::<LinearTermV1>() + self.additions.len() * addition,
            self.data.capacity() * size_of::<f32>()
                + self.linears.capacity() * size_of::<LinearTermV1>()
                + self.additions.capacity() * addition,
        )
    }
}

pub(super) enum SGradientSinkV1<'a> {
    Direct(&'a mut [f32]),
    Record(&'a mut SGradientTermsV1),
}

fn add_values_v1(gradient: &mut [f32], offset: usize, values: &[f32]) {
    for (acc, value) in gradient[offset..offset + values.len()]
        .iter_mut()
        .zip(values)
    {
        *acc += *value;
    }
}

impl SGradientSinkV1<'_> {
    pub(super) fn add_v1(&mut self, offset: usize, values: &[f32]) {
        match self {
            Self::Direct(gradient) => add_values_v1(gradient, offset, values),
            Self::Record(terms) => {
                profile::addition_term_v1(values.len());
                let range = terms.store_v1(values);
                terms.additions.push((offset, range));
            }
        }
    }

    pub(super) fn linear_v1(&mut self, index: LinearIdxV1, rows: usize, x: &[f32], dy: &[f32]) {
        let _span = profile::span_v1(StageV1::SinkLinear);
        match self {
            Self::Direct(gradient) => {
                // dW[o, i] += sum_r dy[r, o] x[r, i]
                gemm_v1(
                    index.out_dim,
                    index.in_dim,
                    rows,
                    dy,
                    1,
                    index.out_dim,
                    x,
                    InitV1::Existing,
                    &mut gradient[index.w..index.w + index.out_dim * index.in_dim],
                );
                kernels::bias_grad_v1(
                    rows,
                    index.out_dim,
                    dy,
                    &mut gradient[index.b..index.b + index.out_dim],
                );
            }
            Self::Record(terms) => {
                profile::linear_term_v1(rows, index.in_dim, index.out_dim);
                let x = terms.store_v1(&x[..rows * index.in_dim]);
                let dy = terms.store_v1(&dy[..rows * index.out_dim]);
                terms.linears.push(LinearTermV1 { index, rows, x, dy });
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn layer_norm_v1(
        &mut self,
        gain_offset: usize,
        bias_offset: usize,
        x: &[f32],
        mean: f32,
        rstd: f32,
        gain: &[f32],
        dy: &[f32],
        dx: &mut [f32],
        scratch: &mut [f32],
    ) {
        let width = x.len();
        match self {
            Self::Direct(gradient) => {
                let (gains, biases) = gradient.split_at_mut(bias_offset);
                let dgain = &mut gains[gain_offset..gain_offset + width];
                let dbias = &mut biases[..width];
                kernels::layer_norm_row_backward_v1(
                    x,
                    mean,
                    rstd,
                    gain,
                    dy,
                    |i, g, b| {
                        dgain[i] += g;
                        dbias[i] += b;
                    },
                    dx,
                    scratch,
                );
            }
            Self::Record(terms) => {
                profile::layer_norm_term_v1(width);
                let bias = terms.store_v1(dy);
                let gain_start = terms.data.len();
                terms.data.resize(gain_start + width, 0.0);
                kernels::layer_norm_row_backward_v1(
                    x,
                    mean,
                    rstd,
                    gain,
                    dy,
                    |i, g, _| {
                        terms.data[gain_start + i] = g;
                    },
                    dx,
                    scratch,
                );
                terms
                    .additions
                    .push((gain_offset, gain_start..gain_start + width));
                terms.additions.push((bias_offset, bias));
            }
        }
    }
}

struct WeightShardV1<'a> {
    offset: usize,
    start_row: usize,
    rows: usize,
    values: &'a mut [f32],
}

fn replay_v1(
    layout: &LayoutV1,
    gradient: &mut [f32],
    block: &[PreparedGaeV1<'_>],
    workers: usize,
) -> Result<(), String> {
    // The non-weight terms do not overlap a Linear weight. The shared
    // card and ref tables still receive each individual addition in order.
    let additions = profile::span_v1(StageV1::ReplayAdditions);
    for substep in block {
        let terms = &substep.terms;
        for &(offset, ref range) in &terms.additions {
            add_values_v1(gradient, offset, &terms.data[range.clone()]);
        }
        for term in &terms.linears {
            let index = term.index;
            kernels::bias_grad_v1(
                term.rows,
                index.out_dim,
                &terms.data[term.dy.clone()],
                &mut gradient[index.b..index.b + index.out_dim],
            );
        }
    }
    drop(additions);
    let build = profile::span_v1(StageV1::ReplayShardBuild);
    let mut shards = Vec::new();
    let mut remaining = gradient;
    let target_size = layout.total.div_ceil(workers * 4);
    for tensor in &layout.tensors {
        let (values, tail) = remaining.split_at_mut(tensor.len);
        remaining = tail;
        if !tensor.linear {
            continue;
        }
        let width = tensor.shape[1];
        let rows_per_shard = target_size.div_ceil(width).max(4).next_multiple_of(4);
        for (shard, values) in values.chunks_mut(rows_per_shard * width).enumerate() {
            shards.push(WeightShardV1 {
                offset: tensor.offset,
                start_row: shard * rows_per_shard,
                rows: values.len() / width,
                values,
            });
        }
    }
    shards.sort_by_key(|shard| std::cmp::Reverse(shard.values.len()));
    drop(build);
    let _span = profile::span_v1(StageV1::ReplayDrain);
    let replay = |_: &mut (), shard: WeightShardV1<'_>| {
        let mut census = profile::shard_v1(
            layout,
            shard.offset,
            shard.start_row,
            shard.rows,
            shard.values.len(),
        );
        for substep in block {
            let terms = &substep.terms;
            for term in &terms.linears {
                census.scanned_v1();
                if term.index.w == shard.offset {
                    census.matched_v1(term.rows, term.index.in_dim);
                    gemm_v1(
                        shard.rows,
                        term.index.in_dim,
                        term.rows,
                        &terms.data[term.dy.start + shard.start_row..term.dy.end],
                        1,
                        term.index.out_dim,
                        &terms.data[term.x.clone()],
                        InitV1::Existing,
                        shard.values,
                    );
                }
            }
        }
    };
    drain_v1(
        DrainV1::Replay,
        &mut vec![(); workers],
        shards.into_iter(),
        replay,
    )
}

/// Fixed-size, scoped workers. The caller also drains jobs, including any
/// work left when the OS cannot start all requested helpers. `kind` only
/// labels the measurement-only census (`profile`).
fn drain_v1<T: Send, W: Send>(
    kind: DrainV1,
    workspaces: &mut [W],
    items: impl Iterator<Item = T> + Send,
    work: impl Fn(&mut W, T) + Sync,
) -> Result<(), String> {
    let census = profile::drain_v1(kind, workspaces.len());
    let queue = Mutex::new(items);
    let drain = |workspace: &mut W, worker: usize| -> Result<(), String> {
        ensure_mxcsr_v1()?;
        let mut busy = census.worker_v1(worker);
        loop {
            let item = queue.lock().expect("S learner job queue").next();
            let Some(item) = item else { return Ok(()) };
            let started = busy.item_started_v1();
            work(workspace, item);
            busy.item_done_v1(started);
        }
    };
    let (caller, helpers) = workspaces
        .split_first_mut()
        .expect("S learner needs a worker");
    thread::scope(|scope| {
        let drain = &drain;
        let spawning = census.mark_v1();
        let helpers: Vec<_> = helpers
            .iter_mut()
            .enumerate()
            .filter_map(|(index, workspace)| {
                let helper = thread::Builder::new()
                    .spawn_scoped(scope, move || drain(workspace, 1 + index))
                    .ok();
                census.spawned_v1(helper.is_some());
                helper
            })
            .collect();
        census.spawn_done_v1(spawning);
        let result = drain(caller, 0);
        let joining = census.mark_v1();
        let joins = helpers.len();
        for helper in helpers {
            helper
                .join()
                .unwrap_or_else(|payload| std::panic::resume_unwind(payload))?;
        }
        census.join_done_v1(joining, joins);
        result
    })
}

#[derive(Clone, Copy)]
struct GaeJobV1<'a> {
    substep: SSubstepV1<'a>,
    index: usize,
    sub: usize,
    last: bool,
    advantage: f32,
    target: f32,
}

#[derive(Default)]
struct PreparedGaeV1<'a> {
    job: Option<Result<GaeJobV1<'a>, String>>,
    terms: SGradientTermsV1,
    score: Option<Result<(f32, f32), String>>,
}

fn term_block_bytes_v1(
    active: &[PreparedGaeV1<'_>],
    inactive: &[PreparedGaeV1<'_>],
) -> profile::BlockBytesV1 {
    let mut bytes = profile::BlockBytesV1::default();
    for slot in active {
        let (values, records, capacity) = slot.terms.bytes_v1();
        bytes.values += values;
        bytes.records += records;
        bytes.active_capacity += capacity;
    }
    bytes.retained_capacity = bytes.active_capacity
        + inactive
            .iter()
            .map(|slot| slot.terms.bytes_v1().2)
            .sum::<usize>();
    bytes
}

#[cfg(all(test, feature = "s-learner-profile-v1"))]
#[test]
fn partial_block_counts_inactive_slot_capacity_separately() {
    let mut slots = [PreparedGaeV1::default(), PreparedGaeV1::default()];
    let active_range = slots[0].terms.store_v1(&[1.0, 2.0]);
    slots[0].terms.additions.push((0, active_range));
    let old_range = slots[1].terms.store_v1(&[0.0; 32]);
    slots[1].terms.additions.push((0, old_range));

    let (active, inactive) = slots.split_at(1);
    let bytes = term_block_bytes_v1(active, inactive);
    let (values, records, active_capacity) = slots[0].terms.bytes_v1();
    assert_eq!(bytes.values, values);
    assert_eq!(bytes.records, records);
    assert_eq!(bytes.active_capacity, active_capacity);
    assert_eq!(
        bytes.retained_capacity,
        active_capacity + slots[1].terms.bytes_v1().2
    );
    assert!(bytes.retained_capacity > bytes.active_capacity);
}

fn jobs_v1<'a>(
    groups: &'a [SGroupV1<'a>],
) -> impl Iterator<Item = Result<GaeJobV1<'a>, String>> + 'a {
    groups.iter().enumerate().flat_map(|(index, group)| {
        let invalid = if group.substeps.is_empty() {
            Some(format!("S group {index} has no substeps"))
        } else if !group.advantage.is_finite() || !group.value_target.is_finite() {
            Some(format!("S group {index} has a non-finite target"))
        } else {
            None
        };
        let substeps = if invalid.is_some() {
            &[][..]
        } else {
            &group.substeps
        };
        invalid
            .into_iter()
            .map(Err)
            .chain(substeps.iter().enumerate().map(move |(sub, &substep)| {
                Ok(GaeJobV1 {
                    substep,
                    index,
                    sub,
                    last: sub + 1 == group.substeps.len(),
                    advantage: group.advantage,
                    target: group.value_target,
                })
            }))
    })
}

impl SModelV1 {
    pub(super) fn gae_loss_gradient_parallel_v1(
        &self,
        groups: &[SGroupV1<'_>],
        value_coefficient: f32,
        gradient: &mut [f32],
        workers: usize,
        caller_work: &mut SWorkV1,
    ) -> Result<SLossV1, String> {
        ensure_mxcsr_v1()?;
        if groups.is_empty() {
            return Err("S update has no groups".into());
        }
        if !value_coefficient.is_finite() || value_coefficient <= 0.0 {
            return Err("S value coefficient must be positive".into());
        }
        let group_count = groups.len() as f32;
        let mut policy_sum = 0.0f32;
        let mut value_sum = 0.0f32;
        let mut joint = 0.0f32;
        let mut value_error = 0.0f32;
        let mut jobs = jobs_v1(groups);
        // One tape per worker, at most two contribution records per worker.
        let mut helper_work = vec![SWorkV1::default(); workers - 1];
        let mut workspaces: Vec<_> = std::iter::once(caller_work)
            .chain(helper_work.iter_mut())
            .collect();
        let mut block: Vec<_> = (0..workers * 2).map(|_| PreparedGaeV1::default()).collect();
        loop {
            let mut len = 0;
            for (slot, job) in block.iter_mut().zip(&mut jobs) {
                slot.job = Some(job);
                slot.terms.reset_v1();
                len += 1;
            }
            if len == 0 {
                break;
            }
            let active_workers = workers.min(len);
            let (block, inactive) = block.split_at_mut(len);
            let record = profile::span_v1(StageV1::RecordDrain);
            drain_v1(
                DrainV1::Record,
                &mut workspaces[..active_workers],
                block.iter_mut(),
                |work, slot| {
                    let Some(Ok(job)) = slot.job else { return };
                    slot.score = Some(self.prepare_gae_v1(
                        job,
                        group_count,
                        value_coefficient,
                        work,
                        &mut slot.terms,
                    ));
                },
            )?;
            drop(record);
            profile::block_v1(len, active_workers, || term_block_bytes_v1(block, inactive));
            let merge = profile::span_v1(StageV1::ScoreMerge);
            for slot in block.iter_mut() {
                let job = slot.job.take().expect("S learner scheduled job")?;
                let (selected, error) = slot.score.take().expect("S learner prepared job")?;
                if job.sub == 0 {
                    joint = selected;
                    value_error = error;
                } else {
                    joint += selected;
                }
                if job.last {
                    policy_sum += -joint * job.advantage;
                    value_sum += value_error * value_error;
                }
            }
            drop(merge);
            replay_v1(&self.layout, gradient, block, active_workers)?;
        }
        for range in self.layout.gauge_ranges_v1() {
            gradient[range].fill(0.0);
        }
        let loss = (policy_sum + value_coefficient * value_sum) / group_count;
        if !loss.is_finite() || gradient.iter().any(|g| !g.is_finite()) {
            return Err("S loss or gradient is not finite".into());
        }
        Ok(SLossV1 {
            group_count: groups.len(),
            policy_sum,
            value_sum,
            policy_term: policy_sum / group_count,
            value_term: value_sum / group_count,
            loss,
        })
    }

    fn prepare_gae_v1(
        &self,
        job: GaeJobV1<'_>,
        group_count: f32,
        value_coefficient: f32,
        work: &mut SWorkV1,
        terms: &mut SGradientTermsV1,
    ) -> Result<(f32, f32), String> {
        let forward = profile::span_v1(StageV1::Forward);
        self.forward_into_v1(job.substep.input, &mut work.tape)?;
        drop(forward);
        let policy_row = profile::span_v1(StageV1::PolicyRow);
        let (selected, dlogits) = gae_policy_row_v1(
            &work.tape.logits,
            job.substep.selected,
            -job.advantage / group_count,
            job.index,
            job.sub,
        )?;
        drop(policy_row);
        let (value_error, dvalue) = if job.sub == 0 {
            let error = work.tape.value - job.target;
            (error, (value_coefficient / group_count) * (2.0 * error))
        } else {
            (0.0, 0.0)
        };
        let backward = profile::span_v1(StageV1::Backward);
        self.backward_into_v1(
            job.substep.input,
            &work.tape,
            &dlogits,
            dvalue,
            &mut SGradientSinkV1::Record(terms),
            &mut work.back,
        )?;
        drop(backward);
        Ok((selected, value_error))
    }
}
