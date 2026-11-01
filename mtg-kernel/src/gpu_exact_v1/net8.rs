//! Net-8's exact forward on a device: resident models, request packing, and
//! the batch launch. v1 (`net8_forward.cu`) is one fused launch per model per
//! batch; v2 (`net8_forward_v2.cu`) is one tiled launch for every model, with
//! one pinned upload and one pinned download.

use super::device::KernelsV1;
use super::ExactDeviceV1;
use crate::inference_cache_v1::ForwardPathV1;
use crate::native_policy_value_net_v1::{NativeEncodedDecisionViewV1, NativePolicyValueNetV1};
use cudarc::driver::{CudaSlice, DevicePtr, DevicePtrMut, DeviceRepr, LaunchConfig, PushKernelArg};

/// `MtgReq` in `net8_forward.cu`.
pub(super) const R_FIELDS: usize = 19;
pub(super) const THREADS_PER_BLOCK: u32 = 256;
const HIDDEN: usize = 64;

/// `MtgDims` in `net8_forward.cu`.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct DimsV1 {
    pub(super) state_dim: i32,
    pub(super) object_feature_dim: i32,
    pub(super) edge_feature_dim: i32,
    pub(super) action_feature_dim: i32,
    pub(super) action_ref_feature_dim: i32,
    pub(super) group_count: i32,
    pub(super) embedding_dim: i32,
}

// SAFETY: plain `repr(C)` integers, the layout of the CUDA struct.
unsafe impl DeviceRepr for DimsV1 {}

/// Prefix of an error from the device itself (allocation, copy, launch or
/// synchronize). After one the context may be poisoned, so the service stops
/// serving; other errors (validation, slot bookkeeping) fail only their request.
pub(crate) const DEVICE_ERROR_PREFIX_V1: &str = "gpu-exact device: ";

/// `MTG_V2_MAX_MODELS` in `net8_forward_v2.cu`: the model slots one v2 launch
/// serves (`BatchPolicyV1::max_models` must not exceed it).
pub(crate) const V2_MAX_MODELS: usize = 10;

/// `MtgModelsV2`: each slot's parameter base pointer, passed by value.
#[repr(C)]
#[derive(Clone, Copy)]
struct ModelsV2 {
    params: [u64; V2_MAX_MODELS],
}

// SAFETY: `repr(C)` device pointers, the layout of the CUDA struct.
unsafe impl DeviceRepr for ModelsV2 {}

/// A model's host-side parameters for upload, identified by its digest.
pub(crate) struct Net8HostModelV1 {
    values: Vec<f32>,
    layout: Vec<i64>,
    dims: DimsV1,
    /// SHA-256 of the dims, layout and parameter bits, so a cache keyed by it
    /// never serves another model's parameters.
    pub(crate) digest: [u8; 32],
}

impl Net8HostModelV1 {
    pub(crate) fn from_net_v1(net: &NativePolicyValueNetV1) -> Result<Self, String> {
        let parameters = net.gpu_exact_parameters_v1().ok_or(
            "the exact GPU forward runs net-8 only (no V8 pairing, hidden 64, embedding 16)",
        )?;
        let dims = DimsV1::of_config_v1(&net.config_v1());
        let digest = parameter_digest_v1(dims, &parameters.values, &parameters.layout);
        Ok(Self {
            values: parameters.values,
            layout: parameters.layout,
            dims,
            digest,
        })
    }
}

impl DimsV1 {
    pub(super) fn of_config_v1(
        config: &crate::native_policy_value_net_v1::NativePolicyValueModelConfigV1,
    ) -> Self {
        Self {
            state_dim: config.state_dim as i32,
            object_feature_dim: config.object_feature_dim as i32,
            edge_feature_dim: config.edge_feature_dim as i32,
            action_feature_dim: config.action_feature_dim as i32,
            action_ref_feature_dim: config.action_ref_feature_dim as i32,
            group_count: config.object_group_count as i32,
            embedding_dim: config.card_embedding_dim as i32,
        }
    }
}

/// A model resident on the device.
pub(crate) struct Net8DeviceModelV1 {
    pub(super) params: CudaSlice<f32>,
    pub(super) layout: CudaSlice<i64>,
    /// The host copy of `layout`, so a v2 launch can check its models share one.
    pub(super) layout_host: Vec<i64>,
    pub(super) dims: DimsV1,
}

impl Net8DeviceModelV1 {
    pub(crate) fn upload_v1(
        device: &ExactDeviceV1,
        host: &Net8HostModelV1,
    ) -> Result<Self, String> {
        Ok(Self {
            params: device
                .stream
                .clone_htod(&host.values)
                .map_err(|e| format!("{DEVICE_ERROR_PREFIX_V1}upload: {e:?}"))?,
            layout: device
                .stream
                .clone_htod(&host.layout)
                .map_err(|e| format!("{DEVICE_ERROR_PREFIX_V1}upload: {e:?}"))?,
            layout_host: host.layout.clone(),
            dims: host.dims,
        })
    }
}

fn parameter_digest_v1(dims: DimsV1, values: &[f32], layout: &[i64]) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(b"gpu-exact-net8-parameters-v1");
    h.update(format!("{dims:?}"));
    for offset in layout {
        h.update(offset.to_le_bytes());
    }
    let mut bytes = Vec::with_capacity(values.len() * 4);
    for value in values {
        bytes.extend_from_slice(&value.to_bits().to_le_bytes());
    }
    h.update(&bytes);
    h.finalize().into()
}

/// The config a forward path validates its views against, or an error for a
/// path the exact GPU forward does not run.
fn path_config_v1(
    net: &NativePolicyValueNetV1,
    path: ForwardPathV1,
) -> Result<crate::native_policy_value_net_v1::NativePolicyValueModelConfigV1, String> {
    match path {
        ForwardPathV1::Flat => Ok(net.config_v1()),
        ForwardPathV1::TransferV3 => Ok(net.feature_transfer_config_v3()),
        ForwardPathV1::TransferV4 => Ok(net.feature_transfer_config_v4()),
        ForwardPathV1::TransferV7 => Ok(net.feature_transfer_config_v7()),
        ForwardPathV1::TransferV8 => {
            Err("the exact GPU forward does not run the V8 inclusion net".into())
        }
    }
}

/// Requests packed for one launch sequence. Host buffers are reused.
#[derive(Default)]
pub(crate) struct PackedBatchV1 {
    pub(super) requests: Vec<i32>,
    pub(super) floats: Vec<f32>,
    pub(super) ints: Vec<i32>,
    /// Per request: model slot, logits offset and action count.
    pub(super) replies: Vec<(usize, usize, usize)>,
    pub(super) scratch_len: usize,
    pub(super) logits_len: usize,
}

fn offset_i32(len: usize, what: &str) -> Result<i32, String> {
    i32::try_from(len).map_err(|_| format!("batch too large: {what} offset {len}"))
}

impl PackedBatchV1 {
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

    /// Appends `other`'s requests under model slot `slot`, rebasing their
    /// offsets. A request's bits never depend on where it lands.
    pub(crate) fn append_v1(&mut self, other: &PackedBatchV1, slot: usize) -> Result<(), String> {
        let floats = offset_i32(self.floats.len(), "float")?;
        let ints = offset_i32(self.ints.len(), "int")?;
        let scratch = offset_i32(self.scratch_len, "scratch")?;
        let logits = offset_i32(self.logits_len, "logits")?;
        let values = offset_i32(self.replies.len(), "value")?;
        // Every rebased offset must fit before anything is appended.
        offset_i32(self.floats.len() + other.floats.len(), "float")?;
        offset_i32(self.ints.len() + other.ints.len(), "int")?;
        offset_i32(self.scratch_len + other.scratch_len, "scratch")?;
        offset_i32(self.logits_len + other.logits_len, "logits")?;
        offset_i32(self.replies.len() + other.replies.len(), "value")?;
        for descriptor in other.requests.chunks_exact(R_FIELDS) {
            let mut d: [i32; R_FIELDS] = descriptor.try_into().expect("descriptor width");
            d[0] = slot as i32;
            for field in &mut d[5..=9] {
                *field += floats;
            }
            for field in &mut d[10..=15] {
                *field += ints;
            }
            d[16] += scratch;
            d[17] += logits;
            d[18] += values;
            self.requests.extend_from_slice(&d);
        }
        self.floats.extend_from_slice(&other.floats);
        self.ints.extend_from_slice(&other.ints);
        self.replies.extend(
            other
                .replies
                .iter()
                .map(|&(_, at, actions)| (slot, at + logits as usize, actions)),
        );
        self.scratch_len += other.scratch_len;
        self.logits_len += other.logits_len;
        Ok(())
    }

    /// Validates `view` exactly as the CPU forward does and appends it.
    pub(crate) fn push_v1(
        &mut self,
        slot: usize,
        net: &NativePolicyValueNetV1,
        path: ForwardPathV1,
        view: NativeEncodedDecisionViewV1<'_>,
    ) -> Result<(), String> {
        self.push_with_config_v1(slot, path_config_v1(net, path)?, view)
    }

    /// [`Self::push_v1`] validating against an explicit config (the
    /// learner's input config).
    pub(crate) fn push_with_config_v1(
        &mut self,
        slot: usize,
        config: crate::native_policy_value_net_v1::NativePolicyValueModelConfigV1,
        view: NativeEncodedDecisionViewV1<'_>,
    ) -> Result<(), String> {
        if !view.pairing_node_indices.is_empty() {
            return Err("pairing rows need the V8 net".into());
        }
        let counts = view.validate(config).map_err(|e| format!("{e:?}"))?;
        let (objects, edges, refs, actions) = (
            counts.object_count,
            counts.edge_count,
            counts.action_ref_count,
            counts.action_count,
        );
        let floats = |values: &[f32], floats: &mut Vec<f32>| -> Result<i32, String> {
            let at = offset_i32(floats.len(), "float")?;
            floats.extend_from_slice(values);
            Ok(at)
        };
        let state = floats(view.state, &mut self.floats)?;
        let object_features = floats(view.object_features, &mut self.floats)?;
        let edge_features = floats(view.edge_features, &mut self.floats)?;
        let ref_features = floats(view.action_ref_features, &mut self.floats)?;
        let action_features = floats(view.action_features, &mut self.floats)?;
        // Validated indices are in range, so they fit in i32.
        let ints = |values: &[i64], ints: &mut Vec<i32>| -> Result<i32, String> {
            let at = offset_i32(ints.len(), "int")?;
            ints.extend(values.iter().map(|&v| v as i32));
            Ok(at)
        };
        let card_ids = ints(view.object_card_ids, &mut self.ints)?;
        let groups = ints(view.object_groups, &mut self.ints)?;
        let edge_sources = ints(view.edge_source_indices, &mut self.ints)?;
        let edge_targets = ints(view.edge_target_indices, &mut self.ints)?;
        let ref_nodes = ints(view.action_ref_node_indices, &mut self.ints)?;
        let ref_actions = ints(view.action_ref_action_indices, &mut self.ints)?;
        let scratch = offset_i32(self.scratch_len, "scratch")?;
        self.scratch_len += HIDDEN
            * (5 * objects + 2 * edges + config.object_group_count + 4 + 2 * refs + 4 * actions);
        let logits = offset_i32(self.logits_len, "logits")?;
        self.logits_len += actions;
        let value = offset_i32(self.replies.len(), "value")?;
        self.requests.extend_from_slice(&[
            slot as i32,
            objects as i32,
            edges as i32,
            refs as i32,
            actions as i32,
            state,
            object_features,
            edge_features,
            ref_features,
            action_features,
            card_ids,
            groups,
            edge_sources,
            edge_targets,
            ref_nodes,
            ref_actions,
            scratch,
            logits,
            value,
        ]);
        self.replies.push((slot, logits as usize, actions));
        Ok(())
    }
}

/// Device buffers reused across batches (grown, never shrunk).
#[derive(Default)]
pub(crate) struct DeviceBuffersV1 {
    pub(super) requests: Option<CudaSlice<i32>>,
    pub(super) floats: Option<CudaSlice<f32>>,
    pub(super) ints: Option<CudaSlice<i32>>,
    pub(super) scratch: Option<CudaSlice<f32>>,
    logits: Option<CudaSlice<f32>>,
    values: Option<CudaSlice<f32>>,
    /// v2: the descriptors, floats and ints in one buffer.
    packed: Option<CudaSlice<u32>>,
    /// v2: the logits, then the values.
    out: Option<CudaSlice<f32>>,
    upload: Option<PinnedV1<u32>>,
    download: Option<PinnedV1<f32>>,
}

/// Page-locked host memory. Uploads use write-combined memory (fast for the
/// device to read, slow for the host to read); downloads use cached memory.
/// Every batch synchronizes its stream before returning, so a buffer is never
/// rewritten while a copy of it is in flight.
pub(super) struct PinnedV1<T> {
    ptr: *mut T,
    len: usize,
}

// SAFETY: an owned allocation, used by one thread at a time (the buffers'
// owner holds `&mut`).
unsafe impl<T> Send for PinnedV1<T> {}

impl<T: Copy + Default> PinnedV1<T> {
    fn new(device: &ExactDeviceV1, len: usize, write_combined: bool) -> Result<Self, String> {
        let flags = if write_combined {
            cudarc::driver::sys::CU_MEMHOSTALLOC_WRITECOMBINED
        } else {
            0
        };
        device
            .ctx
            .bind_to_thread()
            .map_err(|e| format!("{DEVICE_ERROR_PREFIX_V1}bind: {e:?}"))?;
        // SAFETY: a fresh allocation of `len` elements, initialized below.
        let ptr =
            unsafe { cudarc::driver::result::malloc_host(len * std::mem::size_of::<T>(), flags) }
                .map_err(|e| format!("{DEVICE_ERROR_PREFIX_V1}pinned alloc: {e:?}"))?
                as *mut T;
        // SAFETY: `ptr` holds `len` elements; every one is written here.
        unsafe { std::slice::from_raw_parts_mut(ptr, len).fill(T::default()) };
        Ok(Self { ptr, len })
    }

    fn slice_mut(&mut self, len: usize) -> &mut [T] {
        assert!(len <= self.len);
        // SAFETY: in bounds, and `&mut self` excludes other access.
        unsafe { std::slice::from_raw_parts_mut(self.ptr, len) }
    }
}

impl<T> Drop for PinnedV1<T> {
    fn drop(&mut self) {
        // SAFETY: allocated by `malloc_host`; no copy is in flight (above).
        let _ = unsafe { cudarc::driver::result::free_host(self.ptr as *mut std::ffi::c_void) };
    }
}

fn ensure_pinned<T: Copy + Default>(
    device: &ExactDeviceV1,
    slot: &mut Option<PinnedV1<T>>,
    len: usize,
    write_combined: bool,
) -> Result<(), String> {
    let len = len.max(1);
    if slot.as_ref().is_none_or(|s| s.len < len) {
        *slot = None;
        *slot = Some(PinnedV1::new(
            device,
            len.next_power_of_two(),
            write_combined,
        )?);
    }
    Ok(())
}

pub(super) fn ensure<T: DeviceRepr + cudarc::driver::ValidAsZeroBits>(
    device: &ExactDeviceV1,
    slot: &mut Option<CudaSlice<T>>,
    len: usize,
) -> Result<(), String> {
    let len = len.max(1);
    if slot.as_ref().is_none_or(|s| s.len() < len) {
        *slot = Some(
            device
                .stream
                .alloc_zeros::<T>(len.next_power_of_two())
                .map_err(|e| format!("{DEVICE_ERROR_PREFIX_V1}alloc: {e:?}"))?,
        );
    }
    Ok(())
}

/// One request's reply.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ExactOutputV1 {
    pub(crate) logits: Vec<f32>,
    pub(crate) value: f32,
}

/// Runs every packed request on `kernels`, each block one request. Replies
/// come back in packing order and do not depend on the kernel version.
pub(crate) fn forward_batch_v1(
    device: &ExactDeviceV1,
    kernels: KernelsV1,
    models: &[&Net8DeviceModelV1],
    batch: &PackedBatchV1,
    buffers: &mut DeviceBuffersV1,
) -> Result<Vec<ExactOutputV1>, String> {
    let count = batch.len();
    if count == 0 {
        return Ok(Vec::new());
    }
    let dims = models.first().ok_or("no models")?.dims;
    if models.iter().any(|m| m.dims != dims) {
        return Err("a batch's models must share input dims".into());
    }
    if batch
        .replies
        .iter()
        .any(|&(slot, _, _)| slot >= models.len())
    {
        return Err("request names a model slot that is not resident".into());
    }
    match kernels {
        KernelsV1::V1 => forward_v1(device, models, batch, buffers),
        KernelsV1::V2 => forward_v2(device, models, batch, buffers),
    }
}

/// v2: one tiled launch for every model (at most `V2_MAX_MODELS`; the
/// slots' parameter pointers are a kernel parameter). The upload is one
/// buffer of 32-bit words: the descriptors, the floats' bits and the ints. The
/// download is the logits followed by the values. The stream is synchronized
/// before returning, so the next call may rewrite every buffer and a caller
/// may drop a model once this returns.
fn forward_v2(
    device: &ExactDeviceV1,
    models: &[&Net8DeviceModelV1],
    batch: &PackedBatchV1,
    buffers: &mut DeviceBuffersV1,
) -> Result<Vec<ExactOutputV1>, String> {
    let count = batch.len();
    let stream = &device.stream;
    if models.len() > V2_MAX_MODELS {
        return Err(format!(
            "one v2 launch serves at most {V2_MAX_MODELS} models, not {}",
            models.len()
        ));
    }
    // Models with equal dims have equal layouts (`gpu_exact_parameters_v1`
    // derives the layout from the dims), so one layout serves all; checked.
    if models
        .iter()
        .any(|m| m.layout_host != models[0].layout_host)
    {
        return Err("a v2 launch's models must share one parameter layout".into());
    }
    let layout = &models[0].layout;
    let mut table = ModelsV2 {
        params: [0; V2_MAX_MODELS],
    };
    let mut records = Vec::with_capacity(models.len());
    for (slot, model) in models.iter().enumerate() {
        let (pointer, record) = model.params.device_ptr(stream);
        table.params[slot] = pointer;
        records.push(record);
    }
    let requests_at = 0;
    let floats_at = requests_at + batch.requests.len();
    let ints_at = floats_at + batch.floats.len();
    let words = ints_at + batch.ints.len();
    let outputs = batch.logits_len + count;
    ensure(device, &mut buffers.packed, words)?;
    ensure(device, &mut buffers.out, outputs)?;
    ensure(device, &mut buffers.scratch, batch.scratch_len)?;
    ensure_pinned(device, &mut buffers.upload, words, true)?;
    ensure_pinned(device, &mut buffers.download, outputs, false)?;
    let host = buffers.upload.as_mut().expect("ensured").slice_mut(words);
    for (word, &value) in host[requests_at..floats_at].iter_mut().zip(&batch.requests) {
        *word = value as u32;
    }
    for (word, &value) in host[floats_at..ints_at].iter_mut().zip(&batch.floats) {
        *word = value.to_bits();
    }
    for (word, &value) in host[ints_at..].iter_mut().zip(&batch.ints) {
        *word = value as u32;
    }
    let packed = buffers.packed.as_mut().expect("ensured");
    {
        let (pointer, _record) = packed.device_ptr_mut(stream);
        // SAFETY: `host` is pinned and outlives the copy (the stream is
        // synchronized below); the device buffer holds at least `words`.
        unsafe { cudarc::driver::result::memcpy_htod_async(pointer, &*host, stream.cu_stream()) }
            .map_err(|e| format!("{DEVICE_ERROR_PREFIX_V1}upload: {e:?}"))?;
    }
    let scratch = buffers.scratch.as_mut().expect("ensured");
    let out = buffers.out.as_mut().expect("ensured");
    {
        let descriptors = packed.slice(requests_at..floats_at);
        let floats = packed.slice(floats_at..ints_at);
        let ints = packed.slice(ints_at..words);
        let (mut logits, mut values) = out.split_at_mut(batch.logits_len);
        let mut launch = stream.launch_builder(&device.net8_forward_v2);
        launch
            .arg(&descriptors)
            .arg(&floats)
            .arg(&ints)
            .arg(&table)
            .arg(layout)
            .arg(&models[0].dims)
            .arg(&mut *scratch)
            .arg(&mut logits)
            .arg(&mut values);
        let config = LaunchConfig {
            grid_dim: (count as u32, 1, 1),
            block_dim: (THREADS_PER_BLOCK, 1, 1),
            shared_mem_bytes: device.v2_shared_bytes,
        };
        // SAFETY: every descriptor's offsets lie inside the packed buffer and
        // the scratch and outputs sized by `push_v1`; every slot it names is
        // in the table (checked above).
        unsafe { launch.launch(config) }
            .map_err(|e| format!("{DEVICE_ERROR_PREFIX_V1}forward launch: {e:?}"))?;
    }
    // The parameter reads are scheduled; the records order later writes after them.
    drop(records);
    let read = buffers
        .download
        .as_mut()
        .expect("ensured")
        .slice_mut(outputs);
    {
        let (pointer, _record) = out.device_ptr(stream);
        // SAFETY: `read` is pinned and holds `outputs` elements; the stream is
        // synchronized before it is read.
        unsafe {
            cudarc::driver::result::memcpy_dtoh_async(&mut *read, pointer, stream.cu_stream())
        }
        .map_err(|e| format!("{DEVICE_ERROR_PREFIX_V1}download: {e:?}"))?;
    }
    stream
        .synchronize()
        .map_err(|e| format!("{DEVICE_ERROR_PREFIX_V1}synchronize: {e:?}"))?;
    let values = &read[batch.logits_len..];
    Ok(batch
        .replies
        .iter()
        .zip(values)
        .map(|(&(_, at, actions), &value)| ExactOutputV1 {
            logits: read[at..at + actions].to_vec(),
            value,
        })
        .collect())
}

/// v1: one launch per model slot present.
fn forward_v1(
    device: &ExactDeviceV1,
    models: &[&Net8DeviceModelV1],
    batch: &PackedBatchV1,
    buffers: &mut DeviceBuffersV1,
) -> Result<Vec<ExactOutputV1>, String> {
    let count = batch.len();
    ensure(device, &mut buffers.requests, batch.requests.len())?;
    ensure(device, &mut buffers.floats, batch.floats.len())?;
    ensure(device, &mut buffers.ints, batch.ints.len())?;
    ensure(device, &mut buffers.scratch, batch.scratch_len)?;
    ensure(device, &mut buffers.logits, batch.logits_len)?;
    ensure(device, &mut buffers.values, count)?;
    let stream = &device.stream;
    let requests = buffers.requests.as_mut().expect("ensured");
    let floats = buffers.floats.as_mut().expect("ensured");
    let ints = buffers.ints.as_mut().expect("ensured");
    let upload = |e| format!("{DEVICE_ERROR_PREFIX_V1}upload: {e:?}");
    if !batch.floats.is_empty() {
        stream
            .memcpy_htod(&batch.floats, &mut floats.slice_mut(..batch.floats.len()))
            .map_err(upload)?;
    }
    if !batch.ints.is_empty() {
        stream
            .memcpy_htod(&batch.ints, &mut ints.slice_mut(..batch.ints.len()))
            .map_err(upload)?;
    }
    // One launch per model: descriptors are uploaded grouped by slot (a
    // stable sort), and each descriptor carries its own output offsets, so
    // replies still come back in packing order.
    let mut order: Vec<usize> = (0..count).collect();
    order.sort_by_key(|&i| batch.replies[i].0);
    let mut sorted = Vec::with_capacity(batch.requests.len());
    for &i in &order {
        sorted.extend_from_slice(&batch.requests[i * R_FIELDS..(i + 1) * R_FIELDS]);
    }
    stream
        .memcpy_htod(&sorted, &mut requests.slice_mut(..sorted.len()))
        .map_err(upload)?;
    let scratch = buffers.scratch.as_mut().expect("ensured");
    let logits = buffers.logits.as_mut().expect("ensured");
    let values = buffers.values.as_mut().expect("ensured");
    let mut start = 0;
    while start < count {
        let slot = batch.replies[order[start]].0;
        let mut end = start;
        while end < count && batch.replies[order[end]].0 == slot {
            end += 1;
        }
        let model = models
            .get(slot)
            .ok_or("request names a model slot that is not resident")?;
        let descriptors = requests.slice(start * R_FIELDS..end * R_FIELDS);
        let mut launch = stream.launch_builder(&device.net8_forward);
        launch
            .arg(&descriptors)
            .arg(&*floats)
            .arg(&*ints)
            .arg(&model.params)
            .arg(&model.layout)
            .arg(&model.dims)
            .arg(&mut *scratch)
            .arg(&mut *logits)
            .arg(&mut *values);
        let config = LaunchConfig {
            grid_dim: ((end - start) as u32, 1, 1),
            block_dim: (THREADS_PER_BLOCK, 1, 1),
            shared_mem_bytes: 0,
        };
        // SAFETY: every descriptor's offsets lie inside the uploaded buffers
        // and the scratch, logits and values sized by `push_v1`.
        unsafe { launch.launch(config) }
            .map_err(|e| format!("{DEVICE_ERROR_PREFIX_V1}forward launch: {e:?}"))?;
        start = end;
    }
    let mut host_logits = vec![0f32; batch.logits_len];
    let mut host_values = vec![0f32; count];
    let download = |e| format!("{DEVICE_ERROR_PREFIX_V1}download: {e:?}");
    if batch.logits_len > 0 {
        stream
            .memcpy_dtoh(&logits.slice(..batch.logits_len), &mut host_logits)
            .map_err(download)?;
    }
    stream
        .memcpy_dtoh(&values.slice(..count), &mut host_values)
        .map_err(download)?;
    Ok(batch
        .replies
        .iter()
        .zip(host_values)
        .map(|(&(_, at, actions), value)| ExactOutputV1 {
            logits: host_logits[at..at + actions].to_vec(),
            value,
        })
        .collect())
}
