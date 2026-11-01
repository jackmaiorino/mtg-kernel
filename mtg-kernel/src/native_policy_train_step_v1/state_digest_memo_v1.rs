//! Process memo of train-state digests.
//!
//! A native training update hashes the same train state several times: the
//! update hashes its new snapshot, the checkpoint readback hashes the restored
//! snapshot and then the restored state, and update validation, the next
//! collection and the next update each hash a duplicate of that state again.
//! Every one of those digests is SHA-256 over the same byte stream: the Adam
//! step, the gauge anchor and, for each of the three sections (parameters,
//! first moments, second moments), the tensor count written for it and each
//! tensor's name, shape and value bits, in order.
//!
//! The digest is a pure function of that stream. This memo keeps the stream of
//! the last few states it hashed, with their digests, and returns a held
//! digest only when the stream about to be hashed equals a held one exactly:
//! the same step and anchor, the same tensor counts, and every tensor with the
//! same name, the same shape and bit-for-bit the same values (`to_bits`, so a
//! signed zero or a NaN payload counts). Anything else is hashed as before
//! and then held. Validation is the caller's and always runs first, so an
//! invalid state still never receives a digest.
//!
//! `MTG_KERNEL_TRAIN_STATE_DIGEST_MEMO_ENTRIES` sets the number of held
//! streams (default 2; 0 turns the memo off). An entry holds a copy of every
//! value it hashed (about 15 MB for net-8).

use std::collections::VecDeque;
use std::sync::{Arc, Mutex, OnceLock, PoisonError};

#[cfg(test)]
mod tests;

const DEFAULT_ENTRIES_V1: usize = 2;

/// One element of a digest's stream, in hashing order.
pub(super) enum StreamItemV1<'a> {
    /// A section begins; `count` is the tensor count the digest writes for it.
    Section { count: usize },
    /// A tensor of the current section.
    Tensor {
        name: &'static str,
        shape: &'a [usize],
        values: &'a [f32],
    },
}

/// A train state's digest stream: its Adam step, its gauge anchor and its
/// sections, visited in the order the digest hashes them.
pub(super) trait DigestStreamV1 {
    fn adam_step_v1(&self) -> u64;
    fn scorer_bias_anchor_bits_v1(&self) -> u32;
    fn visit_stream_v1(&self, visitor: &mut dyn FnMut(StreamItemV1<'_>));
}

enum HeldItemV1 {
    Section {
        count: usize,
    },
    Tensor {
        name: &'static str,
        shape: Vec<usize>,
        values: Vec<f32>,
    },
}

struct EntryV1 {
    adam_step: u64,
    scorer_bias_anchor_bits: u32,
    items: Vec<HeldItemV1>,
    digest: [u8; 32],
}

struct MemoV1 {
    capacity: usize,
    /// Least recently used first.
    entries: Mutex<VecDeque<Arc<EntryV1>>>,
    #[cfg(test)]
    computed: std::sync::atomic::AtomicUsize,
}

fn global_v1() -> &'static MemoV1 {
    static MEMO: OnceLock<MemoV1> = OnceLock::new();
    MEMO.get_or_init(|| {
        let capacity = std::env::var("MTG_KERNEL_TRAIN_STATE_DIGEST_MEMO_ENTRIES")
            .ok()
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or(DEFAULT_ENTRIES_V1);
        MemoV1::new_v1(capacity)
    })
}

/// Whether two value slices hold the same bits. Branch-free within a block
/// so the comparison runs at memory speed.
fn same_bits_v1(left: &[f32], right: &[f32]) -> bool {
    const BLOCK: usize = 1024;
    left.len() == right.len()
        && left
            .chunks(BLOCK)
            .zip(right.chunks(BLOCK))
            .all(|(left, right)| {
                left.iter()
                    .zip(right)
                    .fold(true, |same, (l, r)| same & (l.to_bits() == r.to_bits()))
            })
}

/// Whether `stream` is exactly the stream `entry` holds.
fn matches_v1(entry: &EntryV1, stream: &dyn DigestStreamV1) -> bool {
    if entry.adam_step != stream.adam_step_v1()
        || entry.scorer_bias_anchor_bits != stream.scorer_bias_anchor_bits_v1()
    {
        return false;
    }
    let mut next = 0usize;
    let mut same = true;
    stream.visit_stream_v1(&mut |item| {
        if !same {
            return;
        }
        same = match (entry.items.get(next), item) {
            (Some(HeldItemV1::Section { count }), StreamItemV1::Section { count: actual }) => {
                *count == actual
            }
            (
                Some(HeldItemV1::Tensor {
                    name,
                    shape,
                    values,
                }),
                StreamItemV1::Tensor {
                    name: actual_name,
                    shape: actual_shape,
                    values: actual_values,
                },
            ) => {
                *name == actual_name
                    && shape.as_slice() == actual_shape
                    && same_bits_v1(values, actual_values)
            }
            _ => false,
        };
        next += 1;
    });
    same && next == entry.items.len()
}

fn hold_v1(stream: &dyn DigestStreamV1, digest: [u8; 32]) -> EntryV1 {
    let mut items = Vec::new();
    stream.visit_stream_v1(&mut |item| {
        items.push(match item {
            StreamItemV1::Section { count } => HeldItemV1::Section { count },
            StreamItemV1::Tensor {
                name,
                shape,
                values,
            } => HeldItemV1::Tensor {
                name,
                shape: shape.to_vec(),
                values: values.to_vec(),
            },
        });
    });
    EntryV1 {
        adam_step: stream.adam_step_v1(),
        scorer_bias_anchor_bits: stream.scorer_bias_anchor_bits_v1(),
        items,
        digest,
    }
}

impl MemoV1 {
    fn new_v1(capacity: usize) -> Self {
        Self {
            capacity,
            entries: Mutex::new(VecDeque::new()),
            #[cfg(test)]
            computed: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    /// The digest of `stream`: a held one when `stream` equals its held
    /// stream exactly, else `compute()`, which must hash exactly `stream`.
    fn digest_v1(
        &self,
        stream: &dyn DigestStreamV1,
        compute: impl FnOnce() -> [u8; 32],
    ) -> [u8; 32] {
        if self.capacity == 0 {
            return compute();
        }
        let held: Vec<Arc<EntryV1>> = self
            .entries
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .rev()
            .cloned()
            .collect();
        // Compared outside the lock; the newest entry first.
        if let Some(entry) = held.iter().find(|entry| matches_v1(entry, stream)) {
            let digest = entry.digest;
            let mut entries = self.entries.lock().unwrap_or_else(PoisonError::into_inner);
            if let Some(index) = entries.iter().position(|held| Arc::ptr_eq(held, entry)) {
                if let Some(found) = entries.remove(index) {
                    entries.push_back(found);
                }
            }
            return digest;
        }
        #[cfg(test)]
        self.computed
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let digest = compute();
        let entry = Arc::new(hold_v1(stream, digest));
        let mut entries = self.entries.lock().unwrap_or_else(PoisonError::into_inner);
        while entries.len() >= self.capacity {
            entries.pop_front();
        }
        entries.push_back(entry);
        digest
    }
}

/// The process memo's digest of `stream` (see the module documentation).
pub(super) fn digest_v1(
    stream: &dyn DigestStreamV1,
    compute: impl FnOnce() -> [u8; 32],
) -> [u8; 32] {
    global_v1().digest_v1(stream, compute)
}
