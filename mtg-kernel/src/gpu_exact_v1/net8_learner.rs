//! Net-8's exact learner backward on a device (`net8_backward.cu`): the
//! gradients the CPU's exact parallel backward computes, bit for bit.
//!
//! The caller (`native_policy_train_step_v1::gpu_exact_backward_v1`) keeps
//! the loss head on the CPU (log-softmax, the logits' and value's gradients,
//! the scorer-bias gauge) and passes the substeps in the backward's visiting
//! order. Here: one forward with its scratch kept as the tape (its outputs
//! must equal the CPU tape's bits), phase A per substep, phase B per
//! gradient element, and the gradients downloaded in parameter order.

use super::net8::{
    ensure, forward_batch_v1, DeviceBuffersV1, DimsV1, Net8DeviceModelV1, PackedBatchV1, R_FIELDS,
    THREADS_PER_BLOCK,
};
use super::ExactDeviceV1;
use crate::native_policy_value_net_v1::{NativeEncodedDecisionViewV1, NativePolicyValueModelConfigV1};
use cudarc::driver::{CudaSlice, LaunchConfig, PushKernelArg};

const H: usize = 64;
/// Torch-layout parameter tensors: the embedding, then weight and bias of
/// 16 linears (`native_policy_train_step_v1`'s ordinals).
const TENSORS: usize = 33;
const LAYERS: usize = 16;
const TERM_FIELDS: usize = 18;

/// Prefix of an error that only means this update does not fit the device
/// (memory, or a batch past the i32 offsets): the caller then runs the CPU
/// backward, which gives the same bits. Every other error is fatal.
pub(crate) const RESOURCE_ERROR_PREFIX_V1: &str = "gpu-exact resource: ";

fn resource(error: impl std::fmt::Display) -> String {
    format!("{RESOURCE_ERROR_PREFIX_V1}{error}")
}

/// Whether `error` is a resource error (see [`RESOURCE_ERROR_PREFIX_V1`]).
pub(crate) fn is_resource_error_v1(error: &str) -> bool {
    error.starts_with(RESOURCE_ERROR_PREFIX_V1) || error.contains("CUDA_ERROR_OUT_OF_MEMORY")
}

/// One substep of the backward, in visiting order.
pub(crate) struct LearnerSubstepV1<'a> {
    pub(crate) view: NativeEncodedDecisionViewV1<'a>,
    /// The CPU tape's outputs: the GPU forward must reproduce their bits.
    pub(crate) logits: &'a [f32],
    pub(crate) value: f32,
    pub(crate) d_logits: &'a [f32],
    pub(crate) d_value: f32,
}

/// Device state reused across updates.
#[derive(Default)]
pub(crate) struct LearnerBuffersV1 {
    forward: DeviceBuffersV1,
    grad_scratch: Option<CudaSlice<f32>>,
}

/// A request's shape, read back from its packed descriptor.
#[derive(Clone, Copy)]
struct Shape {
    objects: usize,
    edges: usize,
    refs: usize,
    actions: usize,
    floats: [i64; 5],
    ints: [i64; 6],
    scratch: i64,
}

impl Shape {
    fn of(descriptor: &[i32]) -> Self {
        let f = |i: usize| i64::from(descriptor[i]);
        Self {
            objects: descriptor[1] as usize,
            edges: descriptor[2] as usize,
            refs: descriptor[3] as usize,
            actions: descriptor[4] as usize,
            floats: [f(5), f(6), f(7), f(8), f(9)],
            ints: [f(10), f(11), f(12), f(13), f(14), f(15)],
            scratch: f(16),
        }
    }
}

/// Offsets of the forward tape, relative to the request's scratch
/// (`mtg_carve_tape`).
struct Tape {
    obj_h1: i64,
    obj_base: i64,
    edge_h1: i64,
    edge_pooled: i64,
    node_h1: i64,
    obj_hidden: i64,
    group_pooled: i64,
    state_h1: i64,
    state_hidden: i64,
    ref_h1: i64,
    ref_pooled: i64,
    act_h1: i64,
    act_h: i64,
    scorer_h: i64,
    value_h: i64,
}

fn tape(s: &Shape, dims: &DimsV1) -> Tape {
    let (o, e, r, a) = (s.objects as i64, s.edges as i64, s.refs as i64, s.actions as i64);
    let h = H as i64;
    let g = i64::from(dims.group_count);
    let edge_h1 = 2 * o * h;
    let edge_pooled = edge_h1 + 2 * e * h;
    let node_h1 = edge_pooled + o * h;
    let obj_hidden = node_h1 + o * h;
    let group_pooled = obj_hidden + o * h;
    let state_h1 = group_pooled + g * h;
    let state_hidden = state_h1 + h;
    let ref_h1 = state_hidden + h;
    let ref_pooled = ref_h1 + 2 * r * h;
    let act_h1 = ref_pooled + a * h;
    let act_h = act_h1 + a * h;
    let scorer_h = act_h + a * h + h;
    let value_h = scorer_h + a * h;
    Tape {
        obj_h1: 0,
        obj_base: o * h,
        edge_h1,
        edge_pooled,
        node_h1,
        obj_hidden,
        group_pooled,
        state_h1,
        state_hidden,
        ref_h1,
        ref_pooled,
        act_h1,
        act_h,
        scorer_h,
        value_h,
    }
}

/// Offsets of the gradient scratch's terms (`mtg_carve_grad`), relative to
/// the substep's gradient scratch, and its total length.
struct Grad {
    d_lg: i64,
    d_v: i64,
    d_sh: i64,
    d_vh: i64,
    d2_act: i64,
    d1_act: i64,
    d2_ref: i64,
    d1_ref: i64,
    d2_state: i64,
    d1_state: i64,
    d2_node: i64,
    d1_node: i64,
    d2_edge: i64,
    d1_edge: i64,
    d2_obj: i64,
    d1_obj: i64,
    d_oin: i64,
    len: i64,
}

fn grad(s: &Shape, dims: &DimsV1) -> Grad {
    let (o, e, r, a) = (s.objects as i64, s.edges as i64, s.refs as i64, s.actions as i64);
    let h = H as i64;
    let k_obj = i64::from(dims.object_feature_dim + dims.embedding_dim);
    let k_edge = i64::from(dims.edge_feature_dim) + 2 * h;
    let k_state = i64::from(dims.state_dim) + i64::from(dims.group_count) * h;
    let k_ref = i64::from(dims.action_ref_feature_dim) + h;
    let k_act = i64::from(dims.action_feature_dim) + h;
    let mut at = 0i64;
    let mut take = |len: i64| {
        let start = at;
        at += len;
        start
    };
    let d_lg = take(a);
    let d_v = take(1);
    let d_sh = take(a * h);
    take(2 * a * h); // d_si
    take(h); // d_state
    take(a * h); // d_act
    let d_vh = take(h);
    take(h); // d_sv
    let d2_act = take(a * h);
    let d1_act = take(a * h);
    take(a * k_act); // d_ain
    take(r * h); // d_refh
    let d2_ref = take(r * h);
    let d1_ref = take(r * h);
    take(r * k_ref); // d_rin
    take(o * h); // d_objh
    let d2_state = take(h);
    let d1_state = take(h);
    take(k_state); // d_sin
    let d2_node = take(o * h);
    let d1_node = take(o * h);
    take(2 * o * h); // d_nin
    take(o * h); // d_base
    take(o * h); // d_epool
    take(e * h); // d_eh
    let d2_edge = take(e * h);
    let d1_edge = take(e * h);
    take(e * k_edge); // d_ein
    let d2_obj = take(o * h);
    let d1_obj = take(o * h);
    let d_oin = take(o * k_obj);
    Grad {
        d_lg,
        d_v,
        d_sh,
        d_vh,
        d2_act,
        d1_act,
        d2_ref,
        d1_ref,
        d2_state,
        d1_state,
        d2_node,
        d1_node,
        d2_edge,
        d1_edge,
        d2_obj,
        d1_obj,
        d_oin,
        len: at,
    }
}

/// Segment sources in `mtg_net8_param_grad`.
const FLOATS: i64 = 0;
const SCRATCH: i64 = 1;
const PARAMS: i64 = 2;

/// One term: d_out offset, rows, then up to three (source, base, rowmap,
/// stride, len) segments.
fn term(dout: i64, rows: usize, segments: &[(i64, i64, i64, i64, i64)]) -> [i64; TERM_FIELDS] {
    let mut t = [0i64; TERM_FIELDS];
    t[0] = dout;
    t[1] = rows as i64;
    t[2] = segments.len() as i64;
    for (s, seg) in segments.iter().enumerate() {
        t[3 + s * 5..3 + s * 5 + 5].copy_from_slice(&[seg.0, seg.1, seg.2, seg.3, seg.4]);
    }
    t
}

/// The forward's input-major parameter buffer (`gpu_exact_parameters_v1`'s
/// layout) from the Torch-layout tensors, and the Torch buffer with each
/// tensor's offset.
fn layouts(tensors: &[&[f32]]) -> (Vec<f32>, Vec<i64>, Vec<f32>, Vec<i64>) {
    let mut forward = Vec::new();
    let mut forward_layout = Vec::new();
    forward_layout.push(0);
    forward.extend_from_slice(tensors[0]);
    for layer in 0..LAYERS {
        let weight = tensors[1 + 2 * layer];
        let bias = tensors[2 + 2 * layer];
        let n = bias.len();
        let k = weight.len() / n;
        forward_layout.push(forward.len() as i64);
        for i in 0..k {
            for o in 0..n {
                forward.push(weight[o * k + i]);
            }
        }
        forward_layout.push(forward.len() as i64);
        forward.extend_from_slice(bias);
    }
    let mut torch = Vec::new();
    let mut torch_offsets = Vec::new();
    for tensor in tensors {
        torch_offsets.push(torch.len() as i64);
        torch.extend_from_slice(tensor);
    }
    (forward, forward_layout, torch, torch_offsets)
}

/// The gradients of every parameter tensor (Torch layout, parameter order)
/// for `substeps` in visiting order.
pub(crate) fn net8_gradients_v1(
    device: &ExactDeviceV1,
    buffers: &mut LearnerBuffersV1,
    config: NativePolicyValueModelConfigV1,
    tensors: &[&[f32]],
    substeps: &[LearnerSubstepV1<'_>],
) -> Result<Vec<Vec<f32>>, String> {
    if tensors.len() != TENSORS {
        return Err(format!("net-8 has {TENSORS} parameter tensors, not {}", tensors.len()));
    }
    let dims = DimsV1::of_config_v1(&config);
    let stream = &device.stream;
    let e = |what: &'static str| move |error: cudarc::driver::DriverError| format!("{what}: {error:?}");

    // Forward with the tape kept; its outputs must be the CPU tape's bits.
    let mut batch = PackedBatchV1::default();
    for substep in substeps {
        batch
            .push_with_config_v1(0, config, substep.view)
            .map_err(|e| if e.starts_with("batch too large") { resource(e) } else { e })?;
    }
    let (forward, forward_layout, torch, torch_offsets) = layouts(tensors);
    // Everything this update allocates, against what the device has free
    // (another process, such as the inference service, may hold the rest).
    let grad_floats: i64 = batch
        .requests
        .chunks_exact(R_FIELDS)
        .map(|d| grad(&Shape::of(d), &dims).len)
        .sum();
    let needed_floats = batch.floats.len()
        + batch.ints.len()
        + batch.requests.len()
        + 2 * batch.scratch_len.next_power_of_two()
        + 3 * batch.logits_len
        + 2 * substeps.len()
        + (grad_floats as usize).next_power_of_two()
        + 2 * torch.len()
        + forward.len();
    let (free, _) = device.memory_v1()?;
    if needed_floats.saturating_mul(4) > free / 10 * 9 {
        return Err(resource(format!(
            "update needs about {} MiB, device has {} MiB free",
            needed_floats * 4 >> 20,
            free >> 20
        )));
    }
    let model = Net8DeviceModelV1 {
        params: stream.clone_htod(&forward).map_err(e("upload"))?,
        layout: stream.clone_htod(&forward_layout).map_err(e("upload"))?,
        dims,
    };
    let outputs = forward_batch_v1(device, &[&model], &batch, &mut buffers.forward)?;
    for (index, (output, substep)) in outputs.iter().zip(substeps).enumerate() {
        let same = output.value.to_bits() == substep.value.to_bits()
            && output.logits.len() == substep.logits.len()
            && output.logits.iter().zip(substep.logits).all(|(a, b)| a.to_bits() == b.to_bits());
        if !same {
            return Err(format!("GPU forward differs from the CPU tape at substep {index}"));
        }
    }

    // The logits' and value's gradients at the forward's offsets.
    let mut d_logits = vec![0f32; batch.logits_len];
    let mut d_values = vec![0f32; substeps.len()];
    for (index, substep) in substeps.iter().enumerate() {
        let (_, at, actions) = batch.replies[index];
        if substep.d_logits.len() != actions {
            return Err("d_logits length differs from the action count".into());
        }
        d_logits[at..at + actions].copy_from_slice(substep.d_logits);
        d_values[index] = substep.d_value;
    }
    let shapes: Vec<Shape> = batch.requests.chunks_exact(R_FIELDS).map(Shape::of).collect();
    let mut grad_offsets = Vec::with_capacity(shapes.len());
    let mut grad_len = 0i64;
    for shape in &shapes {
        grad_offsets.push(grad_len);
        grad_len += grad(shape, &dims).len;
    }
    ensure(device, &mut buffers.grad_scratch, grad_len as usize)?;
    let torch_device = stream.clone_htod(&torch).map_err(e("upload"))?;
    let torch_layout: Vec<i64> = (0..LAYERS).map(|l| torch_offsets[1 + 2 * l]).collect();
    let torch_layout_device = stream.clone_htod(&torch_layout).map_err(e("upload"))?;
    let d_logits_device = stream.clone_htod(&d_logits).map_err(e("upload"))?;
    let d_values_device = stream.clone_htod(&d_values).map_err(e("upload"))?;
    let grad_offsets_device = stream.clone_htod(&grad_offsets).map_err(e("upload"))?;
    let forward_buffers = &buffers.forward;
    let requests = forward_buffers.requests.as_ref().expect("forward ran");
    let floats = forward_buffers.floats.as_ref().expect("forward ran");
    let ints = forward_buffers.ints.as_ref().expect("forward ran");
    let scratch = forward_buffers.scratch.as_ref().expect("forward ran");
    let grad_scratch = buffers.grad_scratch.as_mut().expect("ensured");

    // Phase A: one block per substep.
    {
        let mut launch = stream.launch_builder(&device.net8_reverse);
        launch
            .arg(requests)
            .arg(ints)
            .arg(&torch_device)
            .arg(&torch_layout_device)
            .arg(&dims)
            .arg(scratch)
            .arg(&d_logits_device)
            .arg(&d_values_device)
            .arg(&grad_offsets_device)
            .arg(&mut *grad_scratch);
        let config = LaunchConfig {
            grid_dim: (substeps.len() as u32, 1, 1),
            block_dim: (THREADS_PER_BLOCK, 1, 1),
            shared_mem_bytes: 0,
        };
        // SAFETY: descriptors, tape and gradient scratch were sized by the
        // same arithmetic as the kernel's carving.
        unsafe { launch.launch(config) }.map_err(e("reverse launch"))?;
    }

    // Phase B terms per layer, substeps in visiting order.
    let fo = i64::from(dims.object_feature_dim);
    let fe = i64::from(dims.edge_feature_dim);
    let fs = i64::from(dims.state_dim);
    let fr = i64::from(dims.action_ref_feature_dim);
    let fa = i64::from(dims.action_feature_dim);
    let emb = i64::from(dims.embedding_dim);
    let gh = i64::from(dims.group_count) * H as i64;
    let h = H as i64;
    let mut terms: Vec<Vec<[i64; TERM_FIELDS]>> = vec![Vec::new(); LAYERS];
    let mut occurrences: Vec<(i32, i64)> = Vec::new();
    for (index, shape) in shapes.iter().enumerate() {
        let t = tape(shape, &dims);
        let g = grad(shape, &dims);
        let gs = grad_offsets[index];
        let sc = shape.scratch;
        let [state, obj_feat, edge_feat, ref_feat, act_feat] = shape.floats;
        let [card_ids, _groups, edge_src, edge_tgt, ref_node, _ref_action] = shape.ints;
        let none = -1;
        let (o, ed, r, a) = (shape.objects, shape.edges, shape.refs, shape.actions);
        let mut push = |layer: usize, dout: i64, rows: usize, segs: &[(i64, i64, i64, i64, i64)]| {
            terms[layer].push(term(gs + dout, rows, segs));
        };
        push(0, g.d1_obj, o, &[(FLOATS, obj_feat, none, fo, fo), (PARAMS, torch_offsets[0], card_ids, emb, emb)]);
        push(1, g.d2_obj, o, &[(SCRATCH, sc + t.obj_h1, none, h, h)]);
        push(2, g.d1_edge, ed, &[
            (FLOATS, edge_feat, none, fe, fe),
            (SCRATCH, sc + t.obj_base, edge_src, h, h),
            (SCRATCH, sc + t.obj_base, edge_tgt, h, h),
        ]);
        push(3, g.d2_edge, ed, &[(SCRATCH, sc + t.edge_h1, none, h, h)]);
        push(4, g.d1_node, o, &[(SCRATCH, sc + t.obj_base, none, h, h), (SCRATCH, sc + t.edge_pooled, none, h, h)]);
        push(5, g.d2_node, o, &[(SCRATCH, sc + t.node_h1, none, h, h)]);
        push(6, g.d1_state, 1, &[(FLOATS, state, none, fs, fs), (SCRATCH, sc + t.group_pooled, none, gh, gh)]);
        push(7, g.d2_state, 1, &[(SCRATCH, sc + t.state_h1, none, h, h)]);
        push(8, g.d1_ref, r, &[(FLOATS, ref_feat, none, fr, fr), (SCRATCH, sc + t.obj_hidden, ref_node, h, h)]);
        push(9, g.d2_ref, r, &[(SCRATCH, sc + t.ref_h1, none, h, h)]);
        push(10, g.d1_act, a, &[(FLOATS, act_feat, none, fa, fa), (SCRATCH, sc + t.ref_pooled, none, h, h)]);
        push(11, g.d2_act, a, &[(SCRATCH, sc + t.act_h1, none, h, h)]);
        // Every action row reads the same state hidden row: stride 0.
        push(12, g.d_sh, a, &[(SCRATCH, sc + t.state_hidden, none, 0, h), (SCRATCH, sc + t.act_h, none, h, h)]);
        push(13, g.d_lg, a, &[(SCRATCH, sc + t.scorer_h, none, h, h)]);
        push(14, g.d_vh, 1, &[(SCRATCH, sc + t.state_hidden, none, h, h)]);
        push(15, g.d_v, 1, &[(SCRATCH, sc + t.value_h, none, h, h)]);
        // Card embedding rows: non-padding tokens in object order.
        let k_obj = fo + emb;
        for (object, &token) in substeps[index].view.object_card_ids.iter().enumerate() {
            if token != 0 {
                occurrences.push((token as i32, gs + g.d_oin + object as i64 * k_obj + fo));
            }
        }
    }

    let mut gradients = stream.alloc_zeros::<f32>(torch.len()).map_err(e("alloc"))?;
    for (layer, layer_terms) in terms.iter().enumerate() {
        let weight_offset = torch_offsets[1 + 2 * layer] as usize;
        let bias_offset = torch_offsets[2 + 2 * layer] as usize;
        let n = tensors[2 + 2 * layer].len();
        let k = tensors[1 + 2 * layer].len() / n;
        let flat: Vec<i64> = layer_terms.iter().flatten().copied().collect();
        let terms_device = stream.clone_htod(&flat).map_err(e("upload"))?;
        let (nterms, n_i, k_i) = (layer_terms.len() as i32, n as i32, k as i32);
        // The scorer's output bias adds its rows in reverse (`linear_backward_into`).
        let reverse = i32::from(layer == 13);
        let (mut left, mut bias_view) = gradients.split_at_mut(bias_offset);
        let mut weight_view = left.slice_mut(weight_offset..);
        let mut launch = stream.launch_builder(&device.net8_param_grad);
        launch
            .arg(&terms_device)
            .arg(&nterms)
            .arg(&*grad_scratch)
            .arg(floats)
            .arg(scratch)
            .arg(&torch_device)
            .arg(ints)
            .arg(&n_i)
            .arg(&k_i)
            .arg(&reverse)
            .arg(&mut weight_view)
            .arg(&mut bias_view);
        let items = (n * k + n) as u32;
        let config = LaunchConfig {
            grid_dim: (items.div_ceil(256), 1, 1),
            block_dim: (256, 1, 1),
            shared_mem_bytes: 0,
        };
        // SAFETY: every term addresses rows inside the uploaded buffers.
        unsafe { launch.launch(config) }.map_err(e("phase B launch"))?;
    }

    // Card embedding: touched tokens ascending, occurrences in visiting order.
    occurrences.sort_by_key(|&(token, _)| token); // stable: keeps visiting order per token
    if !occurrences.is_empty() {
        let mut tokens = Vec::new();
        let mut starts = Vec::new();
        for (q, &(token, _)) in occurrences.iter().enumerate() {
            if tokens.last() != Some(&token) {
                tokens.push(token);
                starts.push(q as i32);
            }
        }
        starts.push(occurrences.len() as i32);
        let offsets: Vec<i64> = occurrences.iter().map(|&(_, at)| at).collect();
        let tokens_device = stream.clone_htod(&tokens).map_err(e("upload"))?;
        let starts_device = stream.clone_htod(&starts).map_err(e("upload"))?;
        let offsets_device = stream.clone_htod(&offsets).map_err(e("upload"))?;
        let (ntokens, emb_i) = (tokens.len() as i32, dims.embedding_dim);
        let mut launch = stream.launch_builder(&device.net8_embedding_grad);
        launch
            .arg(&tokens_device)
            .arg(&ntokens)
            .arg(&starts_device)
            .arg(&offsets_device)
            .arg(&*grad_scratch)
            .arg(&emb_i)
            .arg(&mut gradients);
        let items = (tokens.len() * dims.embedding_dim as usize) as u32;
        let config = LaunchConfig {
            grid_dim: (items.div_ceil(256), 1, 1),
            block_dim: (256, 1, 1),
            shared_mem_bytes: 0,
        };
        // SAFETY: tokens are validated card ids; offsets lie in the scratch.
        unsafe { launch.launch(config) }.map_err(e("embedding launch"))?;
    }
    let host = stream.clone_dtoh(&gradients).map_err(e("download"))?;
    Ok(torch_offsets
        .iter()
        .zip(tensors)
        .map(|(&at, tensor)| host[at as usize..at as usize + tensor.len()].to_vec())
        .collect())
}
