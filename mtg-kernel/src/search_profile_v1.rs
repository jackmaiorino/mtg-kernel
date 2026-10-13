//! Measurement-only cost split of Stage 4a persistent search (collab
//! LANES/search-efficiency-plan-20261012). Compiled in only with the
//! `s4a-profile-v1` feature; default builds get zero-sized no-op spans, so
//! uninstrumented timings come from a build without the feature.
//!
//! Buckets are exclusive leaves: a span never encloses another span, so
//! bucket times add up without double counting. Accumulators are per thread.

/// Exclusive cost buckets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code, reason = "variants are only constructed with the feature")]
pub(crate) enum Bucket {
    /// World sampling, including the root clone it makes.
    Sample = 0,
    /// Policy role reseeding per simulation.
    Reseed,
    /// `current_response` reads (decision or terminal).
    Response,
    /// Node key construction (`Keyer::step` and `storage`).
    KeyStep,
    /// Tree lookup.
    TreeFind,
    /// Edge choice at a stored node.
    NodeSelect,
    /// New node: permutation and node construction.
    Expand,
    /// Keyer advance along the chosen edge.
    KeyAdvance,
    /// Actor-visible encoding of the policy input.
    PolicyEncode,
    /// Tensorization of the encoded view (V2 policies; V3/V4 use the
    /// `Tensor*` buckets).
    PolicyTensorize,
    /// Network forward pass.
    PolicyForward,
    /// Sampler draw from the logits.
    PolicySample,
    /// Offered/chosen menu labels (action semantics and classification).
    LabelObserve,
    /// Resolution-label snapshots before and after a transition.
    LabelTransition,
    /// Engine transition (`step`).
    EngineStep,
    /// Backup of a natural simulation (diagnostics and tree update).
    Backup,
    /// V3/V4 tensorizer: auxiliary-table validation and object projection.
    TensorProject,
    /// V3/V4 tensorizer: object, edge and extension-edge features.
    TensorGraph,
    /// V3/V4 tensorizer: canonical observation JSON.
    TensorStateJson,
    /// V3/V4 tensorizer: state head features.
    TensorStateHead,
    /// V3/V4 tensorizer: action features and canonical action JSON.
    TensorActions,
    /// V3/V4 tensorizer: SHA-512 digests of the state and action JSON.
    TensorHash,
    /// V3/V4 tensorizer: digest features and output validation.
    TensorFinish,
}

pub(crate) const BUCKETS: usize = 23;

pub(crate) const NAMES: [&str; BUCKETS] = [
    "sample",
    "reseed",
    "response",
    "key_step",
    "tree_find",
    "node_select",
    "expand",
    "key_advance",
    "policy_encode",
    "policy_tensorize",
    "policy_forward",
    "policy_sample",
    "label_observe",
    "label_transition",
    "engine_step",
    "backup",
    "tensor_project",
    "tensor_graph",
    "tensor_state_json",
    "tensor_state_head",
    "tensor_actions",
    "tensor_hash",
    "tensor_finish",
];

#[cfg(feature = "s4a-profile-v1")]
mod imp {
    use super::{Bucket, BUCKETS};
    use std::cell::RefCell;
    use std::time::Instant;

    thread_local! {
        static ACC: RefCell<[(u64, u64); BUCKETS]> = const { RefCell::new([(0, 0); BUCKETS]) };
    }

    pub(crate) struct Span {
        bucket: Bucket,
        start: Instant,
    }

    impl Drop for Span {
        fn drop(&mut self) {
            let ns = self.start.elapsed().as_nanos() as u64;
            ACC.with(|a| {
                let mut a = a.borrow_mut();
                let x = &mut a[self.bucket as usize];
                x.0 += ns;
                x.1 += 1;
            });
        }
    }

    #[inline]
    pub(crate) fn span(bucket: Bucket) -> Span {
        Span {
            bucket,
            start: Instant::now(),
        }
    }

    /// Takes and clears this thread's accumulators: (nanoseconds, spans).
    pub(crate) fn take() -> Option<[(u64, u64); BUCKETS]> {
        Some(ACC.with(|a| std::mem::replace(&mut *a.borrow_mut(), [(0, 0); BUCKETS])))
    }
}

#[cfg(not(feature = "s4a-profile-v1"))]
mod imp {
    use super::{Bucket, BUCKETS};

    pub(crate) struct Span;

    // Ends a span explicitly at `drop(span)` call sites in both builds.
    impl Drop for Span {
        #[inline(always)]
        fn drop(&mut self) {}
    }

    #[inline(always)]
    pub(crate) fn span(_: Bucket) -> Span {
        Span
    }

    pub(crate) fn take() -> Option<[(u64, u64); BUCKETS]> {
        None
    }
}

pub(crate) use imp::{span, take};

/// Digest workload of the tensorizer's SHA-512 step (measurement only):
/// how many blocks are hashed, how many the prefix cache skips, and how
/// often a whole message repeats within one search. Feature-gated like the
/// spans; default builds record nothing.
#[cfg(feature = "s4a-profile-v1")]
pub(crate) mod digest {
    use std::cell::RefCell;
    use std::collections::{HashSet, VecDeque};

    /// Window of recent state messages compared for a wider prefix cache.
    const WINDOW: usize = 1024;

    #[derive(Default)]
    struct Acc {
        calls: u64,
        state_bytes: u64,
        state_blocks: u64,
        state_resumed_blocks: u64,
        state_repeat_calls: u64,
        state_repeat_blocks: u64,
        state_window_resumable_blocks: u64,
        action_messages: u64,
        action_blocks: u64,
        action_repeat_messages: u64,
        action_repeat_blocks: u64,
        seen_states: HashSet<(u64, usize)>,
        seen_actions: HashSet<(u64, usize)>,
        recent: VecDeque<(u64, Vec<u8>)>,
    }

    thread_local! {
        static ACC: RefCell<Acc> = RefCell::new(Acc::default());
    }

    fn fast(bytes: &[u8]) -> u64 {
        let mut hash = 0xcbf2_9ce4_8422_2325_u64 ^ bytes.len() as u64;
        for chunk in bytes.chunks(8) {
            let mut word = [0u8; 8];
            word[..chunk.len()].copy_from_slice(chunk);
            hash = (hash ^ u64::from_le_bytes(word)).wrapping_mul(0x9e37_79b9_7f4a_7c15);
            hash ^= hash >> 29;
        }
        hash
    }

    /// One decision's digest request: `resumed` is the blocks the prefix
    /// cache let each state message skip, the headers are the
    /// namespace-plus-counter lengths, `checkpoint` the cache's checkpoint
    /// spacing in blocks and `head` the leading bytes the cache buckets on.
    pub(crate) fn observe(
        state: &[u8],
        actions: &[&[u8]],
        resumed: usize,
        state_header: usize,
        action_header: usize,
        checkpoint: usize,
        head: usize,
    ) {
        let blocks = |header: usize, len: usize| (header + len + 17).div_ceil(128) as u64;
        ACC.with(|acc| {
            let mut a = acc.borrow_mut();
            a.calls += 1;
            a.state_bytes += state.len() as u64;
            let state_blocks = blocks(state_header, state.len());
            a.state_blocks += 6 * state_blocks;
            a.state_resumed_blocks += 6 * resumed as u64;
            if !a.seen_states.insert((fast(state), state.len())) {
                a.state_repeat_calls += 1;
                a.state_repeat_blocks += 6 * state_blocks;
            }
            // Best checkpoint-aligned resume against the last WINDOW states.
            let data_blocks = (state_header + state.len()) / 128;
            let mut best = 0usize;
            let key = (state.len() >= head).then(|| fast(&state[..head]));
            for (other_key, other) in a.recent.iter() {
                if key != Some(*other_key) {
                    continue;
                }
                let limit = state.len().min(other.len());
                let mut shared = 0;
                while shared + 64 <= limit
                    && state[shared..shared + 64] == other[shared..shared + 64]
                {
                    shared += 64;
                }
                while shared < limit && state[shared] == other[shared] {
                    shared += 1;
                }
                let usable = ((state_header + shared) / 128)
                    .min(data_blocks)
                    .min((state_header + other.len()) / 128);
                best = best.max(usable / checkpoint * checkpoint);
            }
            a.state_window_resumable_blocks += 6 * best as u64;
            if let Some(key) = key {
                if a.recent.len() == WINDOW {
                    a.recent.pop_front();
                }
                a.recent.push_back((key, state.to_vec()));
            }
            for action in actions {
                let action_blocks = 6 * blocks(action_header, action.len());
                a.action_messages += 1;
                a.action_blocks += action_blocks;
                if !a.seen_actions.insert((fast(action), action.len())) {
                    a.action_repeat_messages += 1;
                    a.action_repeat_blocks += action_blocks;
                }
            }
        });
    }

    /// Takes and clears this thread's digest counters.
    pub(crate) fn take_json() -> serde_json::Value {
        ACC.with(|acc| {
            let a = std::mem::take(&mut *acc.borrow_mut());
            serde_json::json!({
                "calls": a.calls,
                "state_bytes": a.state_bytes,
                "state_blocks": a.state_blocks,
                "state_resumed_blocks": a.state_resumed_blocks,
                "state_repeat_calls": a.state_repeat_calls,
                "state_repeat_blocks": a.state_repeat_blocks,
                "state_window1024_resumable_blocks": a.state_window_resumable_blocks,
                "action_messages": a.action_messages,
                "action_blocks": a.action_blocks,
                "action_repeat_messages": a.action_repeat_messages,
                "action_repeat_blocks": a.action_repeat_blocks,
            })
        })
    }
}

/// The accumulators as JSON (`null` without the feature).
pub(crate) fn take_json() -> serde_json::Value {
    match take() {
        None => serde_json::Value::Null,
        Some(acc) => {
            let mut m = serde_json::Map::new();
            for (i, (ns, n)) in acc.iter().enumerate() {
                m.insert(
                    NAMES[i].to_owned(),
                    serde_json::json!({"seconds": *ns as f64 * 1e-9, "spans": n}),
                );
            }
            serde_json::Value::Object(m)
        }
    }
}
