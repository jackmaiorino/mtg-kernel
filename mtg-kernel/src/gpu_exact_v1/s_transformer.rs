//! S (`s_transformer_v1`) exact forward on a CUDA device: resident transposed
//! parameter buffers, request validation and packing, and multi-model batched
//! launches (`s_transformer.cu`).
//!
//! Every floating-point operation inside `mtg_s_forward` uses IEEE-754
//! round-to-nearest-even intrinsics in the CPU forward's exact reduction order,
//! and the host finishes `value = tanh_f32_v1(0.5 * value_logit)` via the
//! portable rational/polynomial implementation in `deterministic_math_v1`, so
//! GPU outputs equal `SModelV1::forward_v1` / `SModelV1::score_tensor_v1` bit
//! for bit on every machine.

use super::net8::{ensure, ensure_pinned, PinnedV1, DEVICE_ERROR_PREFIX_V1};
use super::ExactDeviceV1;
use crate::s_transformer_v1::{
    ensure_mxcsr_v1, CountsV1, SConfigV1, SInputV1, SModelV1, SOutputV1, OBJECT_FEATURE_DIM_V1,
    REF_FEATURE_DIM_V1, STATE_KEEP_V1,
};
use cudarc::driver::{CudaSlice, DevicePtr, DevicePtrMut, DeviceRepr, LaunchConfig, PushKernelArg};
use sha2::{Digest, Sha256};

/// `MTG_S_R_FIELDS` in `s_transformer.cu` (`i64` elements per descriptor).
pub(super) const S_R_FIELDS: usize = 20;
pub(super) const S_THREADS_PER_BLOCK: u32 = 256;
/// `MTG_S_MAX_MODELS` in `s_transformer.cu`.
pub(crate) const S_MAX_MODELS: usize = 10;

/// `MtgSDims` in `s_transformer.cu`.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct SDimsV1 {
    pub(super) d_model: i32,
    pub(super) layers: i32,
    pub(super) heads: i32,
    pub(super) ffn: i32,
    pub(super) card_dim: i32,
    pub(super) card_vocab: i32,
}

// SAFETY: plain `repr(C)` 32-bit integers matching `struct MtgSDims`.
unsafe impl DeviceRepr for SDimsV1 {}

impl SDimsV1 {
    pub(super) fn of_config_v1(config: SConfigV1) -> Self {
        Self {
            d_model: config.d_model as i32,
            layers: config.layers as i32,
            heads: config.heads as i32,
            ffn: config.ffn as i32,
            card_dim: config.card_dim as i32,
            card_vocab: config.card_vocab as i32,
        }
    }
}

/// `MtgSModels` in `s_transformer.cu`: per-slot `wt` base pointer.
#[repr(C)]
#[derive(Clone, Copy)]
pub(super) struct ModelsS {
    pub(super) params: [u64; S_MAX_MODELS],
}

// SAFETY: `repr(C)` array of 64-bit device pointers matching `struct MtgSModels`.
unsafe impl DeviceRepr for ModelsS {}

/// Host-side S parameter copy (`wt`, containing transposed linear weights and
/// untransposed embeddings/norms/biases) keyed by SHA-256 digest.
pub(crate) struct SHostModelV1 {
    pub(super) wt: Vec<f32>,
    pub(super) dims: SDimsV1,
    pub(crate) config: SConfigV1,
    pub(crate) digest: [u8; 32],
}

impl SHostModelV1 {
    pub(crate) fn from_model_v1(model: &SModelV1) -> Self {
        let config = model.config_v1();
        let dims = SDimsV1::of_config_v1(config);
        let wt = model.wt_v1().to_vec();
        let digest = s_parameter_digest_v1(dims, &wt);
        Self {
            wt,
            dims,
            config,
            digest,
        }
    }
}

fn s_parameter_digest_v1(dims: SDimsV1, wt: &[f32]) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(b"gpu-exact-s-v1");
    for d in [
        dims.d_model,
        dims.layers,
        dims.heads,
        dims.ffn,
        dims.card_dim,
        dims.card_vocab,
    ] {
        h.update(d.to_le_bytes());
    }
    h.update((wt.len() as u64).to_le_bytes());
    #[cfg(target_endian = "little")]
    {
        // SAFETY: `f32` has size 4, alignment >= 1, and no padding bytes.
        let bytes = unsafe {
            std::slice::from_raw_parts(wt.as_ptr().cast::<u8>(), std::mem::size_of_val(wt))
        };
        h.update(bytes);
    }
    #[cfg(not(target_endian = "little"))]
    {
        for v in wt {
            h.update(v.to_bits().to_le_bytes());
        }
    }
    h.finalize().into()
}

/// An S model resident in device memory.
pub(crate) struct SDeviceModelV1 {
    pub(super) params: CudaSlice<f32>,
    pub(super) dims: SDimsV1,
}

impl SDeviceModelV1 {
    pub(crate) fn upload_v1(device: &ExactDeviceV1, host: &SHostModelV1) -> Result<Self, String> {
        Ok(Self {
            params: device
                .stream
                .clone_htod(&host.wt)
                .map_err(|e| format!("{DEVICE_ERROR_PREFIX_V1}S model upload: {e:?}"))?,
            dims: host.dims,
        })
    }
}

/// Compact scratch floats for one inference decision (`for_backward == 0` in
/// `s_transformer.cu`).
pub(super) fn infer_scratch_floats_v1(dims: SDimsV1, counts: CountsV1) -> usize {
    let d = dims.d_model as usize;
    let h = dims.heads as usize;
    let ffn = dims.ffn as usize;
    let cd = dims.card_dim as usize;
    let (n, e, a, r) = (counts.objects, counts.edges, counts.actions, counts.refs);
    let l = 1 + n + a;

    let w1 = (n * (OBJECT_FEATURE_DIM_V1 + cd))
        .max(r * (REF_FEATURE_DIM_V1 + cd))
        .max(r * d)
        .max(l * ffn)
        .max(a * 2 * d);
    let w2 = (n.max(a).max(r).max(1) * d).max(e * 2 * h);

    2 * l * d + h * l * l + l * d + 2 * l + l * 3 * d + h * l * l + l * d + l * d + w1 + w2
}

/// Full-tape scratch floats for one learner substep (`mtg_s_carve_tape` in
/// `s_transformer.cu`).
pub(super) fn tape_scratch_floats_v1(dims: SDimsV1, counts: CountsV1) -> usize {
    let d = dims.d_model as usize;
    let h = dims.heads as usize;
    let ffn = dims.ffn as usize;
    let cd = dims.card_dim as usize;
    let layers = dims.layers as usize;
    let (n, e, a, r) = (counts.objects, counts.edges, counts.actions, counts.refs);
    let l = 1 + n + a;
    let per_layer = 8 * l * d + 4 * l + h * l * l + 2 * l * ffn;

    n * (OBJECT_FEATURE_DIM_V1 + cd)
        + 2 * n * d
        + 2 * d
        + 2 * a * d
        + r * (REF_FEATURE_DIM_V1 + cd)
        + 3 * r * d
        + e * 2 * h
        + h * l * l
        + layers * per_layer
        + l * d
        + (1 + a) * d
        + 2 * (1 + a)
        + a * 2 * d
        + 2 * a * d
        + 2 * d
}

/// Validated S requests packed for one launch.
#[derive(Default)]
pub(crate) struct SPackedBatchV1 {
    pub(super) requests: Vec<i64>,
    pub(super) floats: Vec<f32>,
    pub(super) ints: Vec<i32>,
    /// Per request: `(model_slot, logits_offset, actions)`.
    pub(super) replies: Vec<(usize, usize, usize)>,
    pub(super) scratch_len: usize,
    pub(super) logits_len: usize,
}

impl SPackedBatchV1 {
    pub(crate) fn clear(&mut self) {
        self.requests.clear();
        self.floats.clear();
        self.ints.clear();
        self.replies.clear();
        self.scratch_len = 0;
        self.logits_len = 0;
    }

    pub(crate) fn len(&self) -> usize {
        self.replies.len()
    }

    /// Validates `input` identically to `SModelV1::score_tensor_v1` and appends
    /// it with compact inference scratch.
    pub(crate) fn push_infer_v1(
        &mut self,
        slot: usize,
        config: &SConfigV1,
        input: SInputV1<'_>,
    ) -> Result<CountsV1, String> {
        ensure_mxcsr_v1()?;
        let counts = input.validate_v1(config)?;
        let dims = SDimsV1::of_config_v1(*config);
        let scratch = infer_scratch_floats_v1(dims, counts);
        self.push_validated_v1(slot, counts, scratch, input)?;
        Ok(counts)
    }

    /// Validates `input` identically to `SModelV1::forward_into_v1` and appends
    /// it with full backward-tape scratch.
    pub(crate) fn push_tape_v1(
        &mut self,
        config: &SConfigV1,
        input: SInputV1<'_>,
    ) -> Result<CountsV1, String> {
        ensure_mxcsr_v1()?;
        let counts = input.validate_v1(config)?;
        let dims = SDimsV1::of_config_v1(*config);
        let scratch = tape_scratch_floats_v1(dims, counts);
        self.push_validated_v1(0, counts, scratch, input)?;
        Ok(counts)
    }

    fn push_validated_v1(
        &mut self,
        slot: usize,
        counts: CountsV1,
        scratch_floats: usize,
        input: SInputV1<'_>,
    ) -> Result<(), String> {
        let state_off = self.floats.len() as i64;
        self.floats.extend_from_slice(&input.state[..STATE_KEEP_V1]);
        let obj_feat_off = self.floats.len() as i64;
        self.floats.extend_from_slice(input.object_features);
        let edge_feat_off = self.floats.len() as i64;
        self.floats.extend_from_slice(input.edge_features);
        let act_feat_off = self.floats.len() as i64;
        self.floats.extend_from_slice(input.action_features);
        let ref_feat_off = self.floats.len() as i64;
        self.floats.extend_from_slice(input.ref_features);

        let obj_card_off = self.ints.len() as i64;
        self.ints
            .extend(input.object_card_ids.iter().map(|&v| v as i32));
        let obj_group_off = self.ints.len() as i64;
        self.ints
            .extend(input.object_groups.iter().map(|&v| v as i32));
        let edge_src_off = self.ints.len() as i64;
        self.ints
            .extend(input.edge_source.iter().map(|&v| v as i32));
        let edge_tgt_off = self.ints.len() as i64;
        self.ints
            .extend(input.edge_target.iter().map(|&v| v as i32));
        let ref_card_off = self.ints.len() as i64;
        self.ints
            .extend(input.ref_card_ids.iter().map(|&v| v as i32));
        let ref_act_off = self.ints.len() as i64;
        self.ints.extend(input.ref_action.iter().map(|&v| v as i32));
        let ref_node_off = self.ints.len() as i64;
        self.ints.extend(input.ref_node.iter().map(|&v| v as i32));

        let scratch_off = self.scratch_len as i64;
        self.scratch_len = self
            .scratch_len
            .checked_add(scratch_floats)
            .ok_or_else(|| "batch too large: S scratch overflow".to_owned())?;
        let logits_off = self.logits_len as i64;
        self.logits_len = self
            .logits_len
            .checked_add(counts.actions)
            .ok_or_else(|| "batch too large: S logits overflow".to_owned())?;
        let value_idx = self.replies.len() as i64;

        self.requests.extend_from_slice(&[
            slot as i64,
            counts.objects as i64,
            counts.edges as i64,
            counts.actions as i64,
            counts.refs as i64,
            state_off,
            obj_feat_off,
            edge_feat_off,
            act_feat_off,
            ref_feat_off,
            obj_card_off,
            obj_group_off,
            edge_src_off,
            edge_tgt_off,
            ref_card_off,
            ref_act_off,
            ref_node_off,
            scratch_off,
            logits_off,
            value_idx,
        ]);
        self.replies
            .push((slot, logits_off as usize, counts.actions));
        Ok(())
    }

    /// Appends `other`'s requests under `slot`, rebasing every buffer offset.
    pub(crate) fn append_v1(&mut self, other: &SPackedBatchV1, slot: usize) -> Result<(), String> {
        let floats_base = self.floats.len() as i64;
        let ints_base = self.ints.len() as i64;
        let scratch_base = self.scratch_len as i64;
        let logits_base = self.logits_len as i64;
        let values_base = self.replies.len() as i64;
        let new_scratch = self
            .scratch_len
            .checked_add(other.scratch_len)
            .ok_or_else(|| "batch too large: S scratch overflow".to_owned())?;
        let new_logits = self
            .logits_len
            .checked_add(other.logits_len)
            .ok_or_else(|| "batch too large: S logits overflow".to_owned())?;

        for descriptor in other.requests.chunks_exact(S_R_FIELDS) {
            let mut d: [i64; S_R_FIELDS] = descriptor.try_into().expect("descriptor width");
            d[0] = slot as i64;
            for field in &mut d[5..=9] {
                *field += floats_base;
            }
            for field in &mut d[10..=16] {
                *field += ints_base;
            }
            d[17] += scratch_base;
            d[18] += logits_base;
            d[19] += values_base;
            self.requests.extend_from_slice(&d);
        }
        self.floats.extend_from_slice(&other.floats);
        self.ints.extend_from_slice(&other.ints);
        self.replies.extend(
            other
                .replies
                .iter()
                .map(|&(_, at, actions)| (slot, at + logits_base as usize, actions)),
        );
        self.scratch_len = new_scratch;
        self.logits_len = new_logits;
        Ok(())
    }
}

/// Reusable device and pinned host buffers for S batched inference.
#[derive(Default)]
pub(crate) struct SDeviceBuffersV1 {
    packed: Option<CudaSlice<u32>>,
    scratch: Option<CudaSlice<f32>>,
    out: Option<CudaSlice<f32>>,
    dummy_live: Option<CudaSlice<i32>>,
    upload: Option<PinnedV1<u32>>,
    download: Option<PinnedV1<f32>>,
}

/// Runs one batched S forward across up to [`S_MAX_MODELS`] resident S models
/// sharing input/architecture dimensions. Replies come back in packing order.
pub(crate) fn forward_s_batch_v1(
    device: &ExactDeviceV1,
    models: &[&SDeviceModelV1],
    batch: &SPackedBatchV1,
    buffers: &mut SDeviceBuffersV1,
) -> Result<Vec<SOutputV1>, String> {
    let count = batch.len();
    if count == 0 {
        return Ok(Vec::new());
    }
    if models.len() > S_MAX_MODELS {
        return Err(format!("at most {S_MAX_MODELS} S models per launch"));
    }
    let dims = models.first().ok_or("no S models")?.dims;
    if models.iter().any(|m| m.dims != dims) {
        return Err("an S batch's models must share SConfigV1 dimensions".into());
    }
    if batch
        .replies
        .iter()
        .any(|&(slot, _, _)| slot >= models.len())
    {
        return Err("S request names a model slot that is not resident".into());
    }
    let result = forward_s_enqueued(device, models, dims, batch, buffers);
    if result.is_err() {
        // Never leave an async copy to or from pinned memory in flight.
        let _ = device.stream.synchronize();
    }
    result
}

fn forward_s_enqueued(
    device: &ExactDeviceV1,
    models: &[&SDeviceModelV1],
    dims: SDimsV1,
    batch: &SPackedBatchV1,
    buffers: &mut SDeviceBuffersV1,
) -> Result<Vec<SOutputV1>, String> {
    let kernels = device
        .s_kernels_v1()
        .map_err(|e| format!("{DEVICE_ERROR_PREFIX_V1}{e}"))?;
    let count = batch.len();
    let req_words = batch.requests.len() * 2;
    let float_words = batch.floats.len();
    let int_words = batch.ints.len();
    let total_words = req_words + float_words + int_words;
    let outputs = batch.logits_len + count;

    ensure(device, &mut buffers.packed, total_words)?;
    ensure(device, &mut buffers.scratch, batch.scratch_len)?;
    ensure(device, &mut buffers.out, outputs)?;
    ensure(device, &mut buffers.dummy_live, 1)?;
    ensure_pinned(device, &mut buffers.upload, total_words, true)?;
    ensure_pinned(device, &mut buffers.download, outputs, false)?;

    {
        let write = buffers
            .upload
            .as_mut()
            .expect("ensured")
            .slice_mut(total_words);
        for (req_idx, desc) in batch.requests.chunks_exact(S_R_FIELDS).enumerate() {
            for (field_idx, &val) in desc.iter().enumerate() {
                let rebased = if field_idx == 19 {
                    (batch.logits_len + req_idx) as i64
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
        for (k, &x) in batch.floats.iter().enumerate() {
            write[floats_at + k] = x.to_bits();
        }
        let ints_at = floats_at + float_words;
        for (k, &x) in batch.ints.iter().enumerate() {
            write[ints_at + k] = x as u32;
        }
        let stream = &device.stream;
        let packed = buffers.packed.as_mut().expect("ensured");
        let mut dst = packed.slice_mut(..total_words);
        let (pointer, _record) = dst.device_ptr_mut(stream);
        // SAFETY: `write` is page-locked and holds `total_words` u32 elements;
        // `forward_s_batch_v1` synchronizes `stream` before returning.
        unsafe { cudarc::driver::result::memcpy_htod_async(pointer, write, stream.cu_stream()) }
            .map_err(|e| format!("{DEVICE_ERROR_PREFIX_V1}S upload: {e:?}"))?;
    }

    let stream = &device.stream;
    let mut table = ModelsS {
        params: [0; S_MAX_MODELS],
    };
    let mut records = Vec::with_capacity(models.len());
    for (slot, model) in models.iter().enumerate() {
        let (ptr, record) = model.params.device_ptr(stream);
        table.params[slot] = ptr;
        records.push(record);
    }

    let packed = buffers.packed.as_ref().expect("ensured");
    let reqs_view = packed.slice(..req_words);
    let floats_view = packed.slice(req_words..req_words + float_words);
    let ints_view = packed.slice(req_words + float_words..total_words);
    let scratch = buffers.scratch.as_mut().expect("ensured");
    let out = buffers.out.as_mut().expect("ensured");
    let dummy_live = buffers.dummy_live.as_mut().expect("ensured");
    let for_backward = 0i32;

    {
        let mut launch = stream.launch_builder(&kernels.forward);
        launch
            .arg(&reqs_view)
            .arg(&floats_view)
            .arg(&ints_view)
            .arg(&table)
            .arg(&dims)
            .arg(&for_backward)
            .arg(&mut *scratch)
            .arg(&mut *out)
            .arg(&mut *dummy_live);
        let config = LaunchConfig {
            grid_dim: (count as u32, 1, 1),
            block_dim: (S_THREADS_PER_BLOCK, 1, 1),
            shared_mem_bytes: 0,
        };
        // SAFETY: every descriptor offset lies inside `packed`, `scratch`, and
        // `out` sized above; every model slot is resident in `table`.
        unsafe { launch.launch(config) }
            .map_err(|e| format!("{DEVICE_ERROR_PREFIX_V1}S forward launch: {e:?}"))?;
    }
    drop(records);

    let read = buffers
        .download
        .as_mut()
        .expect("ensured")
        .slice_mut(outputs);
    {
        let (pointer, _record) = out.device_ptr(stream);
        // SAFETY: `read` is page-locked and holds `outputs` elements; `stream`
        // is synchronized immediately below.
        unsafe {
            cudarc::driver::result::memcpy_dtoh_async(&mut *read, pointer, stream.cu_stream())
        }
        .map_err(|e| format!("{DEVICE_ERROR_PREFIX_V1}S download: {e:?}"))?;
    }
    stream
        .synchronize()
        .map_err(|e| format!("{DEVICE_ERROR_PREFIX_V1}S synchronize: {e:?}"))?;

    let value_logits = &read[batch.logits_len..batch.logits_len + count];
    Ok(batch
        .replies
        .iter()
        .zip(value_logits)
        .map(|(&(_, at, actions), &value_logit)| {
            let value = crate::deterministic_math_v1::tanh_f32_v1(0.5 * value_logit);
            SOutputV1 {
                logits: read[at..at + actions].to_vec(),
                value,
                value_logit,
            }
        })
        .collect())
}
