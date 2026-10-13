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

    #[inline(always)]
    pub(crate) fn span(_: Bucket) -> Span {
        Span
    }

    pub(crate) fn take() -> Option<[(u64, u64); BUCKETS]> {
        None
    }
}

pub(crate) use imp::{span, take};

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
