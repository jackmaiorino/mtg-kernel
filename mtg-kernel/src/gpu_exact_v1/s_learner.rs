//! S (`s_transformer_v1`) exact learner forward + backward on a CUDA device
//! (`s_transformer.cu`): bit-identical to the CPU GAE and EX1 auxiliary
//! learners across all parameters.
//!
//! Each update runs:
//! 1. `mtg_s_forward` with `for_backward = 1`, recording the exact CPU forward
//!    tape and per-substep `live_flags` (`final_block_compacts_v1`).
//! 2. The caller's exact CPU loss head (`gae_policy_row_into_v1` / auxiliary KL
//!    + value head `dz = dvalue * 0.5 * (1.0 - value * value)`).
//! 3. `mtg_s_reverse` (Phase A): one block per substep computing all activation
//!    adjoints into `grad_scratch`.
//! 4. Phase B1-B4 (`mtg_s_linear_grad`, `mtg_s_ln_grad`,
//!    `mtg_s_group_refbias_grad`, `mtg_s_card_grad`): deterministic reduction
//!    over substeps `0..S` in ascending visiting order, without atomics or FMA.

use super::net8::{ensure_pinned, PinnedV1, DEVICE_ERROR_PREFIX_V1};
use super::net8_learner::RESOURCE_ERROR_PREFIX_V1;
use super::s_transformer::{
    tape_scratch_floats_v1, ModelsS, SDimsV1, SPackedBatchV1, S_MAX_MODELS, S_R_FIELDS,
    S_THREADS_PER_BLOCK,
};
use super::ExactDeviceV1;
use crate::s_transformer_v1::{
    CountsV1, LinearIdxV1, SInputV1, SModelV1, SOutputV1, ACTION_FEATURE_DIM_V1,
    EDGE_FEATURE_DIM_V1, OBJECT_FEATURE_DIM_V1, OBJECT_GROUP_COUNT_V1, REF_FEATURE_DIM_V1,
    STATE_KEEP_V1,
};
use cudarc::driver::{
    CudaSlice, DevicePtr, DevicePtrMut, DeviceRepr, LaunchConfig, PushKernelArg, ValidAsZeroBits,
};

const SRC_FLOATS: i64 = 0;
const SRC_TAPE: i64 = 1;
const SRC_GRAD: i64 = 2;
const SRC_DLOGITS: i64 = 3;

const LINEAR_SPEC_FIELDS: usize = 6;
const LINEAR_TERM_FIELDS: usize = 9;

fn resource(error: impl std::fmt::Display) -> String {
    format!("{RESOURCE_ERROR_PREFIX_V1}{error}")
}

fn grown_learner_cap_v1(len: usize) -> usize {
    let len = len.max(1);
    let padded = len.saturating_add(len / 4).max(4096);
    padded.div_ceil(4096).saturating_mul(4096)
}

fn extra_learner_slice_bytes_v1<T>(slot: &Option<CudaSlice<T>>, len: usize) -> usize {
    let len = len.max(1);
    let elem = std::mem::size_of::<T>();
    match slot {
        Some(s) if s.len() >= len => 0,
        Some(s) => grown_learner_cap_v1(len)
            .saturating_mul(elem)
            .saturating_sub(s.len().saturating_mul(elem)),
        None => grown_learner_cap_v1(len).saturating_mul(elem),
    }
}

fn ensure_learner_slice_v1<T: DeviceRepr + ValidAsZeroBits>(
    device: &ExactDeviceV1,
    slot: &mut Option<CudaSlice<T>>,
    len: usize,
) -> Result<(), String> {
    let len = len.max(1);
    if slot.as_ref().is_none_or(|s| s.len() < len) {
        // Free the previous allocation before growing so large tape/grad
        // buffers never hold old + new allocations simultaneously.
        *slot = None;
        let cap = grown_learner_cap_v1(len);
        *slot = Some(
            device
                .stream
                .alloc_zeros::<T>(cap)
                .map_err(|e| format!("{DEVICE_ERROR_PREFIX_V1}S learner alloc: {e:?}"))?,
        );
    }
    Ok(())
}

/// Persistent host and device buffers reused across S learner updates.
#[derive(Default)]
pub(crate) struct SLearnerBuffersV1 {
    batch: SPackedBatchV1,
    counts: Vec<CountsV1>,
    packed: Option<CudaSlice<u32>>,
    upload: Option<PinnedV1<u32>>,
    wt_device: Option<CudaSlice<f32>>,
    values_device: Option<CudaSlice<f32>>,
    tape_scratch: Option<CudaSlice<f32>>,
    out: Option<CudaSlice<f32>>,
    download: Option<PinnedV1<f32>>,
    live_flags_host: Vec<i32>,
    live_flags_device: Option<CudaSlice<i32>>,
    outputs_host: Vec<SOutputV1>,
    d_logits_host: Vec<f32>,
    d_dz_host: Vec<f32>,
    d_logits_device: Option<CudaSlice<f32>>,
    d_dz_device: Option<CudaSlice<f32>>,
    grad_offsets_host: Vec<i64>,
    grad_offsets_device: Option<CudaSlice<i64>>,
    grad_scratch: Option<CudaSlice<f32>>,
    linear_specs_host: Vec<i64>,
    linear_terms_host: Vec<i64>,
    linear_specs_device: Option<CudaSlice<i64>>,
    linear_terms_device: Option<CudaSlice<i64>>,
    ln_offsets_host: Vec<i64>,
    ln_terms_host: Vec<i64>,
    ln_offsets_device: Option<CudaSlice<i64>>,
    ln_terms_device: Option<CudaSlice<i64>>,
    occurrences: Vec<(i32, i64)>,
    tokens_host: Vec<i32>,
    starts_host: Vec<i32>,
    offsets_host: Vec<i64>,
    tokens_device: Option<CudaSlice<i32>>,
    starts_device: Option<CudaSlice<i32>>,
    offsets_device: Option<CudaSlice<i64>>,
    gradients_device: Option<CudaSlice<f32>>,
    gradients_host: Vec<f32>,
}

/// `MtgSTapeOff` in `s_transformer.cu`.
#[derive(Clone, Copy)]
struct STapeOff {
    obj_inp: i64,
    obj_act: i64,
    state_act: i64,
    act_act: i64,
    ref_inp: i64,
    ref_act: i64,
    layers_base: i64,
    layer_stride: i64,
    fin: i64,
    pol_inp: i64,
    pol_act: i64,
    val_act: i64,
}

#[derive(Clone, Copy)]
struct SLayerTapeOff {
    ln1: i64,
    attn: i64,
    ln2: i64,
    ff_act: i64,
}

fn carve_tape_v1(dims: SDimsV1, counts: CountsV1) -> STapeOff {
    let d = dims.d_model as i64;
    let h = dims.heads as i64;
    let ffn = dims.ffn as i64;
    let cd = dims.card_dim as i64;
    let (n, e, a, r) = (
        counts.objects as i64,
        counts.edges as i64,
        counts.actions as i64,
        counts.refs as i64,
    );
    let l = 1 + n + a;
    let mut at = 0i64;
    let mut take = |len: i64| {
        let start = at;
        at += len;
        start
    };
    let obj_inp = take(n * (OBJECT_FEATURE_DIM_V1 as i64 + cd));
    let _obj_pre = take(n * d);
    let obj_act = take(n * d);
    let _state_pre = take(d);
    let state_act = take(d);
    let _act_pre = take(a * d);
    let act_act = take(a * d);
    let ref_inp = take(r * (REF_FEATURE_DIM_V1 as i64 + cd));
    let _ref_pre = take(r * d);
    let ref_act = take(r * d);
    let _ref_out = take(r * d);
    let _edge_out = take(e * 2 * h);
    let _bias = take(h * l * l);
    let layer_stride = 8 * l * d + 4 * l + h * l * l + 2 * l * ffn;
    let layers_base = take((dims.layers as i64) * layer_stride);
    let _x_out = take(l * d);
    let fin = take((1 + a) * d);
    let _fin_stats = take(2 * (1 + a));
    let pol_inp = take(a * 2 * d);
    let _pol_pre = take(a * d);
    let pol_act = take(a * d);
    let _val_pre = take(d);
    let val_act = take(d);
    debug_assert_eq!(at as usize, tape_scratch_floats_v1(dims, counts));
    STapeOff {
        obj_inp,
        obj_act,
        state_act,
        act_act,
        ref_inp,
        ref_act,
        layers_base,
        layer_stride,
        fin,
        pol_inp,
        pol_act,
        val_act,
    }
}

fn layer_tape_off_v1(t: &STapeOff, dims: SDimsV1, l: i64, layer: usize) -> SLayerTapeOff {
    let d = dims.d_model as i64;
    let h = dims.heads as i64;
    let ffn = dims.ffn as i64;
    let mut at = t.layers_base + (layer as i64) * t.layer_stride;
    let mut take = |len: i64| {
        let start = at;
        at += len;
        start
    };
    let _x_in = take(l * d);
    let ln1 = take(l * d);
    let _ln1_stats = take(2 * l);
    let _qkv = take(l * 3 * d);
    let _probs = take(h * l * l);
    let attn = take(l * d);
    let _x_mid = take(l * d);
    let ln2 = take(l * d);
    let _ln2_stats = take(2 * l);
    let _ff_pre = take(l * ffn);
    let ff_act = take(l * ffn);
    debug_assert_eq!(
        at - (t.layers_base + (layer as i64) * t.layer_stride),
        t.layer_stride
    );
    SLayerTapeOff {
        ln1,
        attn,
        ln2,
        ff_act,
    }
}

/// `MtgSGradOff` in `s_transformer.cu`.
#[derive(Clone, Copy)]
struct SGradOff {
    dz: i64,
    dval_pre: i64,
    dpol_pre: i64,
    ln_dg: i64,
    ln_db: i64,
    blocks_base: i64,
    block_stride: i64,
    dx_final: i64,
    dstate_pre: i64,
    dobj_pre: i64,
    dobj_inp: i64,
    dact_pre: i64,
    dref_out: i64,
    dref_pre: i64,
    dref_inp: i64,
    deb: i64,
    len: usize,
}

#[derive(Clone, Copy)]
struct SBlockGradOff {
    dff2_dy: i64,
    dff0_dy: i64,
    ln2_dg: i64,
    ln2_db: i64,
    dproj_dy: i64,
    dqkv: i64,
    ln1_dg: i64,
    ln1_db: i64,
}

fn carve_grad_v1(dims: SDimsV1, counts: CountsV1) -> SGradOff {
    let d = dims.d_model as i64;
    let h = dims.heads as i64;
    let ffn = dims.ffn as i64;
    let cd = dims.card_dim as i64;
    let (n, e, a, r) = (
        counts.objects as i64,
        counts.edges as i64,
        counts.actions as i64,
        counts.refs as i64,
    );
    let l = 1 + n + a;
    let mut at = 0i64;
    let mut take = |len: i64| {
        let start = at;
        at += len;
        start
    };
    let dz = take(1);
    let dval_pre = take(d);
    let dpol_pre = take(a * d);
    let ln_dg = take((1 + a) * d);
    let ln_db = take((1 + a) * d);
    let block_stride = 9 * l * d + l * ffn;
    let blocks_base = take((dims.layers as i64) * block_stride);
    let dx_final = take(l * d);
    let dstate_pre = take(d);
    let dobj_pre = take(n * d);
    let dobj_inp = take(n * (OBJECT_FEATURE_DIM_V1 as i64 + cd));
    let dact_pre = take(a * d);
    let dref_out = take(r * d);
    let dref_pre = take(r * d);
    let dref_inp = take(r * (REF_FEATURE_DIM_V1 as i64 + cd));
    let _dbias = take(h * l * l);
    let deb = take(e * 2 * h);
    let _ws_dx = take(l * d);
    let _ws_dx2 = take(l * d);
    let _ws_dattn = take(l * d);
    let _ws_dscore = take(h * l * l);
    let _ws_dfin = take((1 + a) * d);
    let _ws_dpol_inp = take(a * 2 * d);
    SGradOff {
        dz,
        dval_pre,
        dpol_pre,
        ln_dg,
        ln_db,
        blocks_base,
        block_stride,
        dx_final,
        dstate_pre,
        dobj_pre,
        dobj_inp,
        dact_pre,
        dref_out,
        dref_pre,
        dref_inp,
        deb,
        len: at as usize,
    }
}

fn block_grad_off_v1(g: &SGradOff, dims: SDimsV1, l: i64, layer: usize) -> SBlockGradOff {
    let d = dims.d_model as i64;
    let ffn = dims.ffn as i64;
    let mut at = g.blocks_base + (layer as i64) * g.block_stride;
    let mut take = |len: i64| {
        let start = at;
        at += len;
        start
    };
    let dff2_dy = take(l * d);
    let dff0_dy = take(l * ffn);
    let ln2_dg = take(l * d);
    let ln2_db = take(l * d);
    let dproj_dy = take(l * d);
    let dqkv = take(l * 3 * d);
    let ln1_dg = take(l * d);
    let ln1_db = take(l * d);
    debug_assert_eq!(
        at - (g.blocks_base + (layer as i64) * g.block_stride),
        g.block_stride
    );
    SBlockGradOff {
        dff2_dy,
        dff0_dy,
        ln2_dg,
        ln2_db,
        dproj_dy,
        dqkv,
        ln1_dg,
        ln1_db,
    }
}

/// Runs one batched S learner forward + loss head + backward on `device`,
/// returning `(loss_meta, ungauged_gradients)` (`len == model.layout_v1().total`).
///
/// The caller zeroes `model.layout_v1().gauge_ranges_v1()` (after reading
/// `pol2.b` when computing EX1's `auxiliary_bias_residual`).
pub(crate) fn s_learner_gradient_v1<T, F>(
    device: &ExactDeviceV1,
    buffers: &mut SLearnerBuffersV1,
    model: &SModelV1,
    inputs: &[SInputV1<'_>],
    loss_head: F,
) -> Result<(T, Vec<f32>), String>
where
    F: FnOnce(&[SOutputV1], &mut Vec<f32>, &mut Vec<f32>) -> Result<T, String>,
{
    let lay = model.layout_v1();
    let total_params = lay.total;
    if inputs.is_empty() {
        buffers.outputs_host.clear();
        buffers.d_logits_host.clear();
        buffers.d_dz_host.clear();
        let meta = loss_head(
            &buffers.outputs_host,
            &mut buffers.d_logits_host,
            &mut buffers.d_dz_host,
        )?;
        return Ok((meta, vec![0.0; total_params]));
    }
    if inputs.len() > u32::MAX as usize {
        return Err(resource("S learner batch exceeds CUDA grid limit"));
    }

    let result = s_learner_enqueued_v1(device, buffers, model, inputs, loss_head);
    if result.is_err() {
        let _ = device.stream.synchronize();
    }
    result
}

fn s_learner_enqueued_v1<T, F>(
    device: &ExactDeviceV1,
    buffers: &mut SLearnerBuffersV1,
    model: &SModelV1,
    inputs: &[SInputV1<'_>],
    loss_head: F,
) -> Result<(T, Vec<f32>), String>
where
    F: FnOnce(&[SOutputV1], &mut Vec<f32>, &mut Vec<f32>) -> Result<T, String>,
{
    let kernels = device
        .s_kernels_v1()
        .map_err(|e| format!("{DEVICE_ERROR_PREFIX_V1}{e}"))?;
    let lay = model.layout_v1();
    let config = lay.config;
    let dims = SDimsV1::of_config_v1(config);
    let total_params = lay.total;
    let count = inputs.len();

    buffers.batch.clear();
    buffers.counts.clear();
    buffers.counts.reserve(count);
    buffers.grad_offsets_host.clear();
    buffers.grad_offsets_host.reserve(count);

    let mut grad_scratch_len = 0usize;
    let mut card_occ_upper = 0usize;
    for &input in inputs {
        let counts = buffers.batch.push_tape_v1(&config, input).map_err(|e| {
            if e.starts_with("batch too large") {
                resource(e)
            } else {
                e
            }
        })?;
        let g = carve_grad_v1(dims, counts);
        buffers.grad_offsets_host.push(grad_scratch_len as i64);
        grad_scratch_len = grad_scratch_len
            .checked_add(g.len)
            .ok_or_else(|| resource("S gradient scratch overflow"))?;
        card_occ_upper = card_occ_upper.saturating_add(counts.objects + counts.refs);
        buffers.counts.push(counts);
    }

    let req_words = buffers.batch.requests.len() * 2;
    let float_words = buffers.batch.floats.len();
    let int_words = buffers.batch.ints.len();
    let total_words = req_words
        .checked_add(float_words)
        .and_then(|v| v.checked_add(int_words))
        .ok_or_else(|| resource("S packed upload overflow"))?;
    let outputs_len = buffers
        .batch
        .logits_len
        .checked_add(count)
        .ok_or_else(|| resource("S outputs overflow"))?;

    let layers = config.layers;
    // 4 head linears + 4*(layers - 1) non-final block linears + 5 final block
    // linears (qkv split into query + kv) + 9 input/edge linears = 14 + 4*layers.
    let num_linear_specs = 14 + 4 * layers;
    let linear_specs_len = num_linear_specs * LINEAR_SPEC_FIELDS;
    let linear_terms_len = num_linear_specs
        .checked_mul(count)
        .and_then(|v| v.checked_mul(LINEAR_TERM_FIELDS))
        .ok_or_else(|| resource("S linear terms overflow"))?;
    let num_ln_vecs = 2 * (1 + 2 * layers);
    let ln_terms_len = num_ln_vecs
        .checked_mul(count)
        .and_then(|v| v.checked_mul(2))
        .ok_or_else(|| resource("S LayerNorm terms overflow"))?;

    let extra_needed_bytes = extra_learner_slice_bytes_v1(&buffers.packed, total_words)
        .saturating_add(extra_learner_slice_bytes_v1(
            &buffers.wt_device,
            total_params,
        ))
        .saturating_add(extra_learner_slice_bytes_v1(
            &buffers.values_device,
            total_params,
        ))
        .saturating_add(extra_learner_slice_bytes_v1(
            &buffers.tape_scratch,
            buffers.batch.scratch_len,
        ))
        .saturating_add(extra_learner_slice_bytes_v1(&buffers.out, outputs_len))
        .saturating_add(extra_learner_slice_bytes_v1(
            &buffers.live_flags_device,
            count,
        ))
        .saturating_add(extra_learner_slice_bytes_v1(
            &buffers.d_logits_device,
            buffers.batch.logits_len,
        ))
        .saturating_add(extra_learner_slice_bytes_v1(&buffers.d_dz_device, count))
        .saturating_add(extra_learner_slice_bytes_v1(
            &buffers.grad_offsets_device,
            count,
        ))
        .saturating_add(extra_learner_slice_bytes_v1(
            &buffers.grad_scratch,
            grad_scratch_len,
        ))
        .saturating_add(extra_learner_slice_bytes_v1(
            &buffers.linear_specs_device,
            linear_specs_len,
        ))
        .saturating_add(extra_learner_slice_bytes_v1(
            &buffers.linear_terms_device,
            linear_terms_len,
        ))
        .saturating_add(extra_learner_slice_bytes_v1(
            &buffers.ln_offsets_device,
            num_ln_vecs,
        ))
        .saturating_add(extra_learner_slice_bytes_v1(
            &buffers.ln_terms_device,
            ln_terms_len,
        ))
        .saturating_add(extra_learner_slice_bytes_v1(
            &buffers.tokens_device,
            config.card_vocab.max(1),
        ))
        .saturating_add(extra_learner_slice_bytes_v1(
            &buffers.starts_device,
            config.card_vocab + 1,
        ))
        .saturating_add(extra_learner_slice_bytes_v1(
            &buffers.offsets_device,
            card_occ_upper.max(1),
        ))
        .saturating_add(extra_learner_slice_bytes_v1(
            &buffers.gradients_device,
            total_params,
        ));
    let (free, _) = device
        .memory_v1()
        .map_err(|e| format!("{DEVICE_ERROR_PREFIX_V1}{e}"))?;
    let budget = (free / 10) * 9;
    if extra_needed_bytes > budget {
        return Err(resource(format!(
            "S learner update needs {extra_needed_bytes} additional bytes, device budget is {budget}"
        )));
    }

    ensure_learner_slice_v1(device, &mut buffers.packed, total_words)?;
    ensure_learner_slice_v1(device, &mut buffers.wt_device, total_params)?;
    ensure_learner_slice_v1(device, &mut buffers.values_device, total_params)?;
    ensure_learner_slice_v1(device, &mut buffers.tape_scratch, buffers.batch.scratch_len)?;
    ensure_learner_slice_v1(device, &mut buffers.out, outputs_len)?;
    ensure_learner_slice_v1(device, &mut buffers.live_flags_device, count)?;
    ensure_pinned(device, &mut buffers.upload, total_words, true)?;
    ensure_pinned(device, &mut buffers.download, outputs_len, false)?;

    let upload_err = |e| format!("{DEVICE_ERROR_PREFIX_V1}S learner upload: {e:?}");
    let download_err = |e| format!("{DEVICE_ERROR_PREFIX_V1}S learner download: {e:?}");

    // Upload `wt` (for forward) and `values` (for backward `dx`).
    {
        let stream = &device.stream;
        let wt_dev = buffers.wt_device.as_mut().expect("ensured");
        stream
            .memcpy_htod(model.wt_v1(), &mut wt_dev.slice_mut(..total_params))
            .map_err(upload_err)?;
        let val_dev = buffers.values_device.as_mut().expect("ensured");
        stream
            .memcpy_htod(model.values_v1(), &mut val_dev.slice_mut(..total_params))
            .map_err(upload_err)?;
    }

    // Pack and upload descriptors + floats + ints.
    {
        let write = buffers
            .upload
            .as_mut()
            .expect("ensured")
            .slice_mut(total_words);
        for (req_idx, desc) in buffers.batch.requests.chunks_exact(S_R_FIELDS).enumerate() {
            for (field_idx, &val) in desc.iter().enumerate() {
                let rebased = if field_idx == 19 {
                    (buffers.batch.logits_len + req_idx) as i64
                } else {
                    val
                };
                let bits = rebased as u64;
                let w_at = (req_idx * S_R_FIELDS + field_idx) * 2;
                write[w_at] = bits as u32;
                write[w_at + 1] = (bits >> 32) as u32;
            }
        }
        let floats_at = req_words;
        for (k, &x) in buffers.batch.floats.iter().enumerate() {
            write[floats_at + k] = x.to_bits();
        }
        let ints_at = floats_at + float_words;
        for (k, &x) in buffers.batch.ints.iter().enumerate() {
            write[ints_at + k] = x as u32;
        }
        let stream = &device.stream;
        let packed = buffers.packed.as_mut().expect("ensured");
        let mut dst = packed.slice_mut(..total_words);
        let (pointer, _record) = dst.device_ptr_mut(stream);
        // SAFETY: `write` is page-locked and holds `total_words` u32 elements;
        // `stream` is synchronized before `buffers.upload` is reused.
        unsafe { cudarc::driver::result::memcpy_htod_async(pointer, write, stream.cu_stream()) }
            .map_err(upload_err)?;
    }

    // Phase 1: Forward with full backward tape (`for_backward = 1`).
    {
        let stream = &device.stream;
        let wt_dev = buffers.wt_device.as_ref().expect("ensured");
        let (wt_ptr, wt_record) = wt_dev.device_ptr(stream);
        let mut table = ModelsS {
            params: [0; S_MAX_MODELS],
        };
        table.params[0] = wt_ptr;

        let packed = buffers.packed.as_ref().expect("ensured");
        let reqs_view = packed.slice(..req_words);
        let floats_view = packed.slice(req_words..req_words + float_words);
        let ints_view = packed.slice(req_words + float_words..total_words);
        let tape_scratch = buffers.tape_scratch.as_mut().expect("ensured");
        let out = buffers.out.as_mut().expect("ensured");
        let live_flags_dev = buffers.live_flags_device.as_mut().expect("ensured");
        let for_backward = 1i32;

        let mut launch = stream.launch_builder(&kernels.forward);
        launch
            .arg(&reqs_view)
            .arg(&floats_view)
            .arg(&ints_view)
            .arg(&table)
            .arg(&dims)
            .arg(&for_backward)
            .arg(&mut *tape_scratch)
            .arg(&mut *out)
            .arg(&mut *live_flags_dev);
        let cfg = LaunchConfig {
            grid_dim: (count as u32, 1, 1),
            block_dim: (S_THREADS_PER_BLOCK, 1, 1),
            shared_mem_bytes: 0,
        };
        // SAFETY: all offsets in `packed`, `tape_scratch`, `out`, and
        // `live_flags_dev` were sized and validated above.
        unsafe { launch.launch(cfg) }
            .map_err(|e| format!("{DEVICE_ERROR_PREFIX_V1}S learner forward launch: {e:?}"))?;
        drop(wt_record);
    }

    // Download forward outputs + `live_flags`.
    buffers.live_flags_host.resize(count, 0);
    {
        let stream = &device.stream;
        let out = buffers.out.as_ref().expect("ensured");
        let read = buffers
            .download
            .as_mut()
            .expect("ensured")
            .slice_mut(outputs_len);
        {
            let (pointer, _record) = out.device_ptr(stream);
            // SAFETY: `read` is page-locked with `outputs_len` elements;
            // `memcpy_dtoh` below synchronizes `stream` before `read` is read.
            unsafe {
                cudarc::driver::result::memcpy_dtoh_async(&mut *read, pointer, stream.cu_stream())
            }
            .map_err(download_err)?;
        }
        let live_dev = buffers.live_flags_device.as_ref().expect("ensured");
        stream
            .memcpy_dtoh(&live_dev.slice(..count), &mut buffers.live_flags_host)
            .map_err(download_err)?;

        buffers.outputs_host.clear();
        buffers.outputs_host.reserve(count);
        let value_logits = &read[buffers.batch.logits_len..outputs_len];
        for (&(_, at, actions), &value_logit) in
            buffers.batch.replies.iter().zip(value_logits.iter())
        {
            let value = crate::deterministic_math_v1::tanh_f32_v1(0.5 * value_logit);
            let logits_slice = &read[at..at + actions];
            if !value.is_finite() || logits_slice.iter().any(|x| !x.is_finite()) {
                return Err("S forward produced a non-finite output".into());
            }
            buffers.outputs_host.push(SOutputV1 {
                logits: logits_slice.to_vec(),
                value,
                value_logit,
            });
        }
    }

    // Phase 2: Exact CPU loss head (`d_logits_host`, `d_dz_host`).
    buffers.d_logits_host.clear();
    buffers.d_logits_host.reserve(buffers.batch.logits_len);
    buffers.d_dz_host.clear();
    buffers.d_dz_host.reserve(count);
    let loss_meta = loss_head(
        &buffers.outputs_host,
        &mut buffers.d_logits_host,
        &mut buffers.d_dz_host,
    )?;
    if buffers.d_logits_host.len() != buffers.batch.logits_len || buffers.d_dz_host.len() != count {
        return Err("S learner loss head produced mismatched adjoint lengths".into());
    }

    // Ensure backward device buffers and upload loss adjoints + grad offsets.
    ensure_learner_slice_v1(
        device,
        &mut buffers.d_logits_device,
        buffers.batch.logits_len,
    )?;
    ensure_learner_slice_v1(device, &mut buffers.d_dz_device, count)?;
    ensure_learner_slice_v1(device, &mut buffers.grad_offsets_device, count)?;
    ensure_learner_slice_v1(device, &mut buffers.grad_scratch, grad_scratch_len)?;
    ensure_learner_slice_v1(device, &mut buffers.linear_specs_device, linear_specs_len)?;
    ensure_learner_slice_v1(device, &mut buffers.linear_terms_device, linear_terms_len)?;
    ensure_learner_slice_v1(device, &mut buffers.ln_offsets_device, num_ln_vecs)?;
    ensure_learner_slice_v1(device, &mut buffers.ln_terms_device, ln_terms_len)?;
    ensure_learner_slice_v1(device, &mut buffers.gradients_device, total_params)?;

    {
        let stream = &device.stream;
        if !buffers.d_logits_host.is_empty() {
            let d_logits_dev = buffers.d_logits_device.as_mut().expect("ensured");
            stream
                .memcpy_htod(
                    &buffers.d_logits_host,
                    &mut d_logits_dev.slice_mut(..buffers.d_logits_host.len()),
                )
                .map_err(upload_err)?;
        }
        let d_dz_dev = buffers.d_dz_device.as_mut().expect("ensured");
        stream
            .memcpy_htod(&buffers.d_dz_host, &mut d_dz_dev.slice_mut(..count))
            .map_err(upload_err)?;
        let goff_dev = buffers.grad_offsets_device.as_mut().expect("ensured");
        stream
            .memcpy_htod(&buffers.grad_offsets_host, &mut goff_dev.slice_mut(..count))
            .map_err(upload_err)?;
    }

    // Phase 3 (Phase A): `mtg_s_reverse`, one block per substep.
    {
        let stream = &device.stream;
        let packed = buffers.packed.as_ref().expect("ensured");
        let reqs_view = packed.slice(..req_words);
        let ints_view = packed.slice(req_words + float_words..total_words);
        let values_dev = buffers.values_device.as_ref().expect("ensured");
        let tape_scratch = buffers.tape_scratch.as_ref().expect("ensured");
        let d_logits_dev = buffers.d_logits_device.as_ref().expect("ensured");
        let d_dz_dev = buffers.d_dz_device.as_ref().expect("ensured");
        let live_flags_dev = buffers.live_flags_device.as_ref().expect("ensured");
        let goff_dev = buffers.grad_offsets_device.as_ref().expect("ensured");
        let grad_scratch = buffers.grad_scratch.as_mut().expect("ensured");

        let mut launch = stream.launch_builder(&kernels.reverse);
        launch
            .arg(&reqs_view)
            .arg(&ints_view)
            .arg(values_dev)
            .arg(&dims)
            .arg(tape_scratch)
            .arg(d_logits_dev)
            .arg(d_dz_dev)
            .arg(live_flags_dev)
            .arg(goff_dev)
            .arg(&mut *grad_scratch);
        let cfg = LaunchConfig {
            grid_dim: (count as u32, 1, 1),
            block_dim: (S_THREADS_PER_BLOCK, 1, 1),
            shared_mem_bytes: 0,
        };
        // SAFETY: every offset in `reqs_view`, `tape_scratch`, `d_logits_dev`,
        // `d_dz_dev`, `live_flags_dev`, `goff_dev`, and `grad_scratch` is sized above.
        unsafe { launch.launch(cfg) }
            .map_err(|e| format!("{DEVICE_ERROR_PREFIX_V1}S reverse launch: {e:?}"))?;
    }

    // Build Phase B tables on the host while `mtg_s_reverse` runs on the GPU!
    let linear_grid_blocks = build_linear_specs_and_terms_v1(
        lay,
        dims,
        &buffers.batch,
        &buffers.counts,
        &buffers.grad_offsets_host,
        &buffers.live_flags_host,
        &mut buffers.linear_specs_host,
        &mut buffers.linear_terms_host,
    )?;
    build_ln_specs_and_terms_v1(
        lay,
        dims,
        &buffers.counts,
        &buffers.grad_offsets_host,
        &buffers.live_flags_host,
        &mut buffers.ln_offsets_host,
        &mut buffers.ln_terms_host,
    );
    build_card_occurrences_v1(
        dims,
        inputs,
        &buffers.counts,
        &buffers.grad_offsets_host,
        &mut buffers.occurrences,
        &mut buffers.tokens_host,
        &mut buffers.starts_host,
        &mut buffers.offsets_host,
    );

    // Zero `gradients_device[..total_params]` so untouched card rows and gauge
    // slots start at `+0.0f`.
    {
        let stream = &device.stream;
        let grads = buffers.gradients_device.as_mut().expect("ensured");
        stream
            .memset_zeros(&mut grads.slice_mut(..total_params))
            .map_err(|e| format!("{DEVICE_ERROR_PREFIX_V1}S gradients zero: {e:?}"))?;
    }

    // Upload Phase B1 & B2 tables.
    {
        let stream = &device.stream;
        let lspecs_dev = buffers.linear_specs_device.as_mut().expect("ensured");
        stream
            .memcpy_htod(
                &buffers.linear_specs_host,
                &mut lspecs_dev.slice_mut(..linear_specs_len),
            )
            .map_err(upload_err)?;
        let lterms_dev = buffers.linear_terms_device.as_mut().expect("ensured");
        stream
            .memcpy_htod(
                &buffers.linear_terms_host,
                &mut lterms_dev.slice_mut(..linear_terms_len),
            )
            .map_err(upload_err)?;
        let lnoff_dev = buffers.ln_offsets_device.as_mut().expect("ensured");
        stream
            .memcpy_htod(
                &buffers.ln_offsets_host,
                &mut lnoff_dev.slice_mut(..num_ln_vecs),
            )
            .map_err(upload_err)?;
        let lnterms_dev = buffers.ln_terms_device.as_mut().expect("ensured");
        stream
            .memcpy_htod(
                &buffers.ln_terms_host,
                &mut lnterms_dev.slice_mut(..ln_terms_len),
            )
            .map_err(upload_err)?;
    }

    // Phase B1: `mtg_s_linear_grad`.
    {
        let stream = &device.stream;
        let lspecs_dev = buffers.linear_specs_device.as_ref().expect("ensured");
        let lterms_dev = buffers.linear_terms_device.as_ref().expect("ensured");
        let packed = buffers.packed.as_ref().expect("ensured");
        let floats_view = packed.slice(req_words..req_words + float_words);
        let tape_scratch = buffers.tape_scratch.as_ref().expect("ensured");
        let grad_scratch = buffers.grad_scratch.as_ref().expect("ensured");
        let d_logits_dev = buffers.d_logits_device.as_ref().expect("ensured");
        let grads = buffers.gradients_device.as_mut().expect("ensured");
        let num_specs_i32 = num_linear_specs as i32;
        let num_substeps_i32 = count as i32;

        let mut launch = stream.launch_builder(&kernels.linear_grad);
        launch
            .arg(lspecs_dev)
            .arg(&num_specs_i32)
            .arg(lterms_dev)
            .arg(&num_substeps_i32)
            .arg(&floats_view)
            .arg(tape_scratch)
            .arg(grad_scratch)
            .arg(d_logits_dev)
            .arg(&mut *grads);
        let cfg = LaunchConfig {
            grid_dim: (linear_grid_blocks, 1, 1),
            block_dim: (S_THREADS_PER_BLOCK, 1, 1),
            shared_mem_bytes: 0,
        };
        // SAFETY: `lspecs_dev` and `lterms_dev` reference verified slices inside
        // `floats_view`, `tape_scratch`, `grad_scratch`, `d_logits_dev`, and `grads`.
        unsafe { launch.launch(cfg) }
            .map_err(|e| format!("{DEVICE_ERROR_PREFIX_V1}S linear_grad launch: {e:?}"))?;
    }

    // Phase B2: `mtg_s_ln_grad`.
    {
        let stream = &device.stream;
        let lnoff_dev = buffers.ln_offsets_device.as_ref().expect("ensured");
        let lnterms_dev = buffers.ln_terms_device.as_ref().expect("ensured");
        let grad_scratch = buffers.grad_scratch.as_ref().expect("ensured");
        let grads = buffers.gradients_device.as_mut().expect("ensured");
        let num_vecs_i32 = num_ln_vecs as i32;
        let num_substeps_i32 = count as i32;
        let d_i32 = dims.d_model;
        let items = (num_ln_vecs * config.d_model) as u32;

        let mut launch = stream.launch_builder(&kernels.ln_grad);
        launch
            .arg(lnoff_dev)
            .arg(lnterms_dev)
            .arg(&num_vecs_i32)
            .arg(&num_substeps_i32)
            .arg(&d_i32)
            .arg(grad_scratch)
            .arg(&mut *grads);
        let cfg = LaunchConfig {
            grid_dim: (items.div_ceil(S_THREADS_PER_BLOCK), 1, 1),
            block_dim: (S_THREADS_PER_BLOCK, 1, 1),
            shared_mem_bytes: 0,
        };
        // SAFETY: all `ln_offsets` and `ln_terms` lie inside `grads` and `grad_scratch`.
        unsafe { launch.launch(cfg) }
            .map_err(|e| format!("{DEVICE_ERROR_PREFIX_V1}S ln_grad launch: {e:?}"))?;
    }

    // Phase B3: `mtg_s_group_refbias_grad`.
    {
        let stream = &device.stream;
        let packed = buffers.packed.as_ref().expect("ensured");
        let reqs_view = packed.slice(..req_words);
        let ints_view = packed.slice(req_words + float_words..total_words);
        let goff_dev = buffers.grad_offsets_device.as_ref().expect("ensured");
        let grad_scratch = buffers.grad_scratch.as_ref().expect("ensured");
        let grads = buffers.gradients_device.as_mut().expect("ensured");
        let num_substeps_i32 = count as i32;
        let group_off = lay.group as i64;
        let ref_bias_off = lay.ref_bias as i64;
        let items = (OBJECT_GROUP_COUNT_V1 * config.d_model + 2 * config.heads) as u32;

        let mut launch = stream.launch_builder(&kernels.group_refbias_grad);
        launch
            .arg(&reqs_view)
            .arg(&ints_view)
            .arg(&dims)
            .arg(&num_substeps_i32)
            .arg(goff_dev)
            .arg(grad_scratch)
            .arg(&group_off)
            .arg(&ref_bias_off)
            .arg(&mut *grads);
        let cfg = LaunchConfig {
            grid_dim: (items.div_ceil(S_THREADS_PER_BLOCK), 1, 1),
            block_dim: (S_THREADS_PER_BLOCK, 1, 1),
            shared_mem_bytes: 0,
        };
        // SAFETY: `group_off` (`20 * d`) and `ref_bias_off` (`2 * h`) lie inside `grads`.
        unsafe { launch.launch(cfg) }
            .map_err(|e| format!("{DEVICE_ERROR_PREFIX_V1}S group_refbias_grad launch: {e:?}"))?;
    }

    // Phase B4: `mtg_s_card_grad` across touched non-padding card tokens.
    if !buffers.tokens_host.is_empty() {
        let num_tokens = buffers.tokens_host.len();
        ensure_learner_slice_v1(device, &mut buffers.tokens_device, num_tokens)?;
        ensure_learner_slice_v1(
            device,
            &mut buffers.starts_device,
            buffers.starts_host.len(),
        )?;
        ensure_learner_slice_v1(
            device,
            &mut buffers.offsets_device,
            buffers.offsets_host.len(),
        )?;
        let stream = &device.stream;
        {
            let tok_dev = buffers.tokens_device.as_mut().expect("ensured");
            stream
                .memcpy_htod(&buffers.tokens_host, &mut tok_dev.slice_mut(..num_tokens))
                .map_err(upload_err)?;
            let st_dev = buffers.starts_device.as_mut().expect("ensured");
            stream
                .memcpy_htod(
                    &buffers.starts_host,
                    &mut st_dev.slice_mut(..buffers.starts_host.len()),
                )
                .map_err(upload_err)?;
            let off_dev = buffers.offsets_device.as_mut().expect("ensured");
            stream
                .memcpy_htod(
                    &buffers.offsets_host,
                    &mut off_dev.slice_mut(..buffers.offsets_host.len()),
                )
                .map_err(upload_err)?;
        }
        let tok_dev = buffers.tokens_device.as_ref().expect("ensured");
        let st_dev = buffers.starts_device.as_ref().expect("ensured");
        let off_dev = buffers.offsets_device.as_ref().expect("ensured");
        let grad_scratch = buffers.grad_scratch.as_ref().expect("ensured");
        let grads = buffers.gradients_device.as_mut().expect("ensured");
        let num_tokens_i32 = num_tokens as i32;
        let cd_i32 = dims.card_dim;
        let card_off = lay.card as i64;
        let items = (num_tokens * config.card_dim) as u32;

        let mut launch = stream.launch_builder(&kernels.card_grad);
        launch
            .arg(tok_dev)
            .arg(&num_tokens_i32)
            .arg(st_dev)
            .arg(off_dev)
            .arg(grad_scratch)
            .arg(&cd_i32)
            .arg(&card_off)
            .arg(&mut *grads);
        let cfg = LaunchConfig {
            grid_dim: (items.div_ceil(S_THREADS_PER_BLOCK), 1, 1),
            block_dim: (S_THREADS_PER_BLOCK, 1, 1),
            shared_mem_bytes: 0,
        };
        // SAFETY: every `token` was validated `< card_vocab` and `> 0`, and every
        // `offsets[q]` points to `cd` floats inside `grad_scratch`.
        unsafe { launch.launch(cfg) }
            .map_err(|e| format!("{DEVICE_ERROR_PREFIX_V1}S card_grad launch: {e:?}"))?;
    }

    // Download gradients.
    buffers.gradients_host.resize(total_params, 0.0);
    {
        let grads = buffers.gradients_device.as_ref().expect("ensured");
        device
            .stream
            .memcpy_dtoh(
                &grads.slice(..total_params),
                &mut buffers.gradients_host[..total_params],
            )
            .map_err(download_err)?;
    }

    Ok((loss_meta, buffers.gradients_host.clone()))
}

#[allow(clippy::too_many_arguments)]
fn build_linear_specs_and_terms_v1(
    lay: &crate::s_transformer_v1::LayoutV1,
    dims: SDimsV1,
    batch: &SPackedBatchV1,
    counts_per_sub: &[CountsV1],
    grad_offsets: &[i64],
    live_flags: &[i32],
    specs_out: &mut Vec<i64>,
    terms_out: &mut Vec<i64>,
) -> Result<u32, String> {
    let count = counts_per_sub.len();
    let layers = dims.layers as usize;
    let d = dims.d_model as i64;
    let h = dims.heads as i64;
    let ffn = dims.ffn as i64;
    let cd = dims.card_dim as i64;
    let num_specs = 14 + 4 * layers;

    specs_out.clear();
    specs_out.reserve(num_specs * LINEAR_SPEC_FIELDS);
    terms_out.clear();
    terms_out.resize(num_specs * count * LINEAR_TERM_FIELDS, 0);

    let mut first_block = 0i64;
    let mut spec_idx = 0usize;
    let mut push_spec = |lin: LinearIdxV1| -> usize {
        let idx = spec_idx;
        let items = lin.out_dim * lin.in_dim + lin.out_dim;
        let blocks = items.div_ceil(S_THREADS_PER_BLOCK as usize) as i64;
        let terms_offset = (idx * count) as i64;
        specs_out.extend_from_slice(&[
            first_block,
            lin.out_dim as i64,
            lin.in_dim as i64,
            lin.w as i64,
            lin.b as i64,
            terms_offset,
        ]);
        first_block += blocks;
        spec_idx += 1;
        idx
    };

    let idx_val2 = push_spec(lay.val2);
    let idx_val0 = push_spec(lay.val0);
    let idx_pol2 = push_spec(lay.pol2);
    let idx_pol0 = push_spec(lay.pol0);

    let mut block_spec_base = Vec::with_capacity(layers);
    for (layer, blk) in lay.blocks.iter().enumerate() {
        let base = push_spec(blk.ff2);
        let _ = push_spec(blk.ff0);
        let _ = push_spec(blk.proj);
        if layer + 1 < layers {
            let _ = push_spec(blk.qkv);
        } else {
            let ud = config_d_usize(dims);
            let _ = push_spec(blk.qkv.rows_v1(0, ud));
            let _ = push_spec(blk.qkv.rows_v1(ud, 2 * ud));
        }
        block_spec_base.push(base);
    }

    let idx_state2 = push_spec(lay.state2);
    let idx_state0 = push_spec(lay.state0);
    let idx_obj2 = push_spec(lay.obj2);
    let idx_obj0 = push_spec(lay.obj0);
    let idx_act2 = push_spec(lay.act2);
    let idx_act0 = push_spec(lay.act0);
    let idx_ref2 = push_spec(lay.ref2);
    let idx_ref0 = push_spec(lay.ref0);
    let idx_edge = push_spec(lay.edge);
    debug_assert_eq!(spec_idx, num_specs);

    let grid_blocks =
        u32::try_from(first_block).map_err(|_| resource("S linear_grad grid overflow"))?;

    let mut set_term = |sp: usize, sub: usize, tm: [i64; LINEAR_TERM_FIELDS]| {
        let at = (sp * count + sub) * LINEAR_TERM_FIELDS;
        terms_out[at..at + LINEAR_TERM_FIELDS].copy_from_slice(&tm);
    };

    for (sub, &counts) in counts_per_sub.iter().enumerate() {
        let req = &batch.requests[sub * S_R_FIELDS..(sub + 1) * S_R_FIELDS];
        let n = counts.objects as i64;
        let e = counts.edges as i64;
        let a = counts.actions as i64;
        let r = counts.refs as i64;
        let l = 1 + n + a;
        let state_off = req[5];
        let edge_feat_off = req[7];
        let act_feat_off = req[8];
        let sc = req[17];
        let dlog_off = req[18];
        let gs = grad_offsets[sub];
        let live = live_flags[sub] != 0;

        let t = carve_tape_v1(dims, counts);
        let g = carve_grad_v1(dims, counts);

        set_term(
            idx_val2,
            sub,
            [
                1,
                SRC_TAPE,
                sc + t.val_act,
                d,
                -1,
                SRC_GRAD,
                gs + g.dz,
                1,
                -1,
            ],
        );
        set_term(
            idx_val0,
            sub,
            [
                1,
                SRC_TAPE,
                sc + t.fin,
                d,
                -1,
                SRC_GRAD,
                gs + g.dval_pre,
                d,
                -1,
            ],
        );
        set_term(
            idx_pol2,
            sub,
            [
                a,
                SRC_TAPE,
                sc + t.pol_act,
                d,
                -1,
                SRC_DLOGITS,
                dlog_off,
                1,
                -1,
            ],
        );
        set_term(
            idx_pol0,
            sub,
            [
                a,
                SRC_TAPE,
                sc + t.pol_inp,
                2 * d,
                -1,
                SRC_GRAD,
                gs + g.dpol_pre,
                d,
                -1,
            ],
        );

        for (layer, &base) in block_spec_base.iter().enumerate() {
            let lt = layer_tape_off_v1(&t, dims, l, layer);
            let bg = block_grad_off_v1(&g, dims, l, layer);
            let is_last = layer + 1 == layers;
            let is_last_live = is_last && live;
            let m = if is_last_live { 1 + a } else { l };
            let x_live = if is_last_live { n } else { -1 };

            set_term(
                base,
                sub,
                [
                    m,
                    SRC_TAPE,
                    sc + lt.ff_act,
                    ffn,
                    x_live,
                    SRC_GRAD,
                    gs + bg.dff2_dy,
                    d,
                    -1,
                ],
            );
            set_term(
                base + 1,
                sub,
                [
                    m,
                    SRC_TAPE,
                    sc + lt.ln2,
                    d,
                    x_live,
                    SRC_GRAD,
                    gs + bg.dff0_dy,
                    ffn,
                    -1,
                ],
            );
            set_term(
                base + 2,
                sub,
                [
                    m,
                    SRC_TAPE,
                    sc + lt.attn,
                    d,
                    x_live,
                    SRC_GRAD,
                    gs + bg.dproj_dy,
                    d,
                    -1,
                ],
            );
            if !is_last {
                set_term(
                    base + 3,
                    sub,
                    [
                        l,
                        SRC_TAPE,
                        sc + lt.ln1,
                        d,
                        -1,
                        SRC_GRAD,
                        gs + bg.dqkv,
                        3 * d,
                        -1,
                    ],
                );
            } else {
                set_term(
                    base + 3,
                    sub,
                    [
                        m,
                        SRC_TAPE,
                        sc + lt.ln1,
                        d,
                        x_live,
                        SRC_GRAD,
                        gs + bg.dqkv,
                        3 * d,
                        x_live,
                    ],
                );
                set_term(
                    base + 4,
                    sub,
                    [
                        l,
                        SRC_TAPE,
                        sc + lt.ln1,
                        d,
                        -1,
                        SRC_GRAD,
                        gs + bg.dqkv + d,
                        3 * d,
                        -1,
                    ],
                );
            }
        }

        set_term(
            idx_state2,
            sub,
            [
                1,
                SRC_TAPE,
                sc + t.state_act,
                d,
                -1,
                SRC_GRAD,
                gs + g.dx_final,
                d,
                -1,
            ],
        );
        set_term(
            idx_state0,
            sub,
            [
                1,
                SRC_FLOATS,
                state_off,
                STATE_KEEP_V1 as i64,
                -1,
                SRC_GRAD,
                gs + g.dstate_pre,
                d,
                -1,
            ],
        );
        set_term(
            idx_obj2,
            sub,
            [
                n,
                SRC_TAPE,
                sc + t.obj_act,
                d,
                -1,
                SRC_GRAD,
                gs + g.dx_final + d,
                d,
                -1,
            ],
        );
        set_term(
            idx_obj0,
            sub,
            [
                n,
                SRC_TAPE,
                sc + t.obj_inp,
                OBJECT_FEATURE_DIM_V1 as i64 + cd,
                -1,
                SRC_GRAD,
                gs + g.dobj_pre,
                d,
                -1,
            ],
        );
        set_term(
            idx_act2,
            sub,
            [
                a,
                SRC_TAPE,
                sc + t.act_act,
                d,
                -1,
                SRC_GRAD,
                gs + g.dx_final + (1 + n) * d,
                d,
                -1,
            ],
        );
        set_term(
            idx_act0,
            sub,
            [
                a,
                SRC_FLOATS,
                act_feat_off,
                ACTION_FEATURE_DIM_V1 as i64,
                -1,
                SRC_GRAD,
                gs + g.dact_pre,
                d,
                -1,
            ],
        );
        set_term(
            idx_ref2,
            sub,
            [
                r,
                SRC_TAPE,
                sc + t.ref_act,
                d,
                -1,
                SRC_GRAD,
                gs + g.dref_out,
                d,
                -1,
            ],
        );
        set_term(
            idx_ref0,
            sub,
            [
                r,
                SRC_TAPE,
                sc + t.ref_inp,
                REF_FEATURE_DIM_V1 as i64 + cd,
                -1,
                SRC_GRAD,
                gs + g.dref_pre,
                d,
                -1,
            ],
        );
        set_term(
            idx_edge,
            sub,
            [
                e,
                SRC_FLOATS,
                edge_feat_off,
                EDGE_FEATURE_DIM_V1 as i64,
                -1,
                SRC_GRAD,
                gs + g.deb,
                2 * h,
                -1,
            ],
        );
    }

    Ok(grid_blocks)
}

fn config_d_usize(dims: SDimsV1) -> usize {
    dims.d_model as usize
}

fn build_ln_specs_and_terms_v1(
    lay: &crate::s_transformer_v1::LayoutV1,
    dims: SDimsV1,
    counts_per_sub: &[CountsV1],
    grad_offsets: &[i64],
    live_flags: &[i32],
    ln_offsets: &mut Vec<i64>,
    ln_terms: &mut Vec<i64>,
) {
    let count = counts_per_sub.len();
    let layers = dims.layers as usize;
    let num_vecs = 2 * (1 + 2 * layers);

    ln_offsets.clear();
    ln_offsets.reserve(num_vecs);
    ln_offsets.push(lay.ln_g as i64);
    ln_offsets.push(lay.ln_b as i64);
    for blk in &lay.blocks {
        ln_offsets.push(blk.ln2_g as i64);
        ln_offsets.push(blk.ln2_b as i64);
        ln_offsets.push(blk.ln1_g as i64);
        ln_offsets.push(blk.ln1_b as i64);
    }
    debug_assert_eq!(ln_offsets.len(), num_vecs);

    ln_terms.clear();
    ln_terms.resize(num_vecs * count * 2, 0);
    let mut set_ln = |vec_idx: usize, sub: usize, base: i64, rows: i64| {
        let at = (vec_idx * count + sub) * 2;
        ln_terms[at] = base;
        ln_terms[at + 1] = rows;
    };

    for (sub, &counts) in counts_per_sub.iter().enumerate() {
        let n = counts.objects as i64;
        let a = counts.actions as i64;
        let l = 1 + n + a;
        let gs = grad_offsets[sub];
        let live = live_flags[sub] != 0;
        let g = carve_grad_v1(dims, counts);

        set_ln(0, sub, gs + g.ln_dg, 1 + a);
        set_ln(1, sub, gs + g.ln_db, 1 + a);
        for layer in 0..layers {
            let bg = block_grad_off_v1(&g, dims, l, layer);
            let is_last_live = (layer + 1 == layers) && live;
            let m = if is_last_live { 1 + a } else { l };
            let v_base = 2 + 4 * layer;
            set_ln(v_base, sub, gs + bg.ln2_dg, m);
            set_ln(v_base + 1, sub, gs + bg.ln2_db, m);
            set_ln(v_base + 2, sub, gs + bg.ln1_dg, l);
            set_ln(v_base + 3, sub, gs + bg.ln1_db, l);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn build_card_occurrences_v1(
    dims: SDimsV1,
    inputs: &[SInputV1<'_>],
    counts_per_sub: &[CountsV1],
    grad_offsets: &[i64],
    occurrences: &mut Vec<(i32, i64)>,
    tokens_out: &mut Vec<i32>,
    starts_out: &mut Vec<i32>,
    offsets_out: &mut Vec<i64>,
) {
    let cd = dims.card_dim as i64;
    let obj_in = OBJECT_FEATURE_DIM_V1 as i64 + cd;
    let ref_in = REF_FEATURE_DIM_V1 as i64 + cd;

    occurrences.clear();
    for (sub, (&input, &counts)) in inputs.iter().zip(counts_per_sub.iter()).enumerate() {
        let gs = grad_offsets[sub];
        let g = carve_grad_v1(dims, counts);
        for (i, &card_u32) in input.object_card_ids.iter().enumerate() {
            let card = card_u32 as i32;
            if card != 0 {
                occurrences.push((
                    card,
                    gs + g.dobj_inp + (i as i64) * obj_in + OBJECT_FEATURE_DIM_V1 as i64,
                ));
            }
        }
        for (k, &card_u32) in input.ref_card_ids.iter().enumerate() {
            let card = card_u32 as i32;
            if card != 0 {
                occurrences.push((
                    card,
                    gs + g.dref_inp + (k as i64) * ref_in + REF_FEATURE_DIM_V1 as i64,
                ));
            }
        }
    }

    // Stable sort keeps each token's occurrences in exact CPU visiting order.
    occurrences.sort_by_key(|&(token, _)| token);

    tokens_out.clear();
    starts_out.clear();
    offsets_out.clear();
    if occurrences.is_empty() {
        return;
    }
    offsets_out.reserve(occurrences.len());
    let mut i = 0usize;
    while i < occurrences.len() {
        let token = occurrences[i].0;
        tokens_out.push(token);
        starts_out.push(offsets_out.len() as i32);
        while i < occurrences.len() && occurrences[i].0 == token {
            offsets_out.push(occurrences[i].1);
            i += 1;
        }
    }
    starts_out.push(offsets_out.len() as i32);
}
