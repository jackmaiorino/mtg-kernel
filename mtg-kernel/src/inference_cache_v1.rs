//! Exact, opt-in prediction cache for frozen-policy forwards (collab
//! LANES/inference-cache-20261014/PLAN.md).
//!
//! A forward is a pure function of the installed parameters, the feature
//! path (V2/V3/V4 transfer), the activation mode and the encoded decision
//! view. The key is every one of those inputs as bytes: a model identity
//! digest, the path and mode tags, the schema, and each consumed slice with
//! its length and exact little-endian element bytes (f32 as bits). A hit
//! compares the stored key with the new one in full before returning its
//! owned logits and value, so a hash collision is a miss, never a wrong
//! answer. Sampling, encoder bookkeeping and forced-move skips happen
//! outside the cache and are unchanged; only time differs.
//!
//! Off unless `MTG_KERNEL_INFERENCE_CACHE_MIB` is a positive integer. The
//! search driver attaches one cache per worker's roles, resets it at the
//! start of every search and reports its counters. The byte budget covers
//! keys, outputs and an estimated per-entry index overhead; eviction is
//! first in, first out, so it is deterministic.

use crate::native_policy_value_net_v1::{ForwardActivationModeV1, NativeEncodedDecisionViewV1};
use serde_json::{json, Value};
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};

/// Estimated bytes per entry beyond its key and logits: the entry record,
/// its FIFO slot and its hash-index slot.
pub(crate) const ENTRY_OVERHEAD_BYTES_V1: usize = 128;

/// Which forward consumed the view (each validates against its own config).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ForwardPathV1 {
    Flat = 1,
    TransferV3 = 3,
    TransferV4 = 4,
}

/// Cache byte budget from `MTG_KERNEL_INFERENCE_CACHE_MIB` (None when unset
/// or 0). Read once per process.
pub(crate) fn budget_bytes_v1() -> Option<usize> {
    static BUDGET: std::sync::OnceLock<Option<usize>> = std::sync::OnceLock::new();
    *BUDGET.get_or_init(
        || match std::env::var("MTG_KERNEL_INFERENCE_CACHE_MIB").as_deref() {
            Err(_) | Ok("0") => None,
            Ok(value) => match value.parse::<usize>() {
                Ok(mib) => Some(mib.saturating_mul(1 << 20)),
                Err(_) => panic!(
                    "MTG_KERNEL_INFERENCE_CACHE_MIB must be a whole number of MiB, got {value:?}"
                ),
            },
        },
    )
}

/// Appends every forward input to `key`: identity, tags, schema, then each
/// slice framed by its element count.
pub(crate) fn write_key_v1(
    key: &mut Vec<u8>,
    model: &[u8; 32],
    path: ForwardPathV1,
    mode: ForwardActivationModeV1,
    view: &NativeEncodedDecisionViewV1<'_>,
) {
    key.clear();
    key.extend_from_slice(b"inference-cache-v1\0");
    key.extend_from_slice(model);
    key.push(path as u8);
    key.push(match mode {
        ForwardActivationModeV1::LibmTanh => 1,
        ForwardActivationModeV1::KernelDeterministicTanh => 2,
        ForwardActivationModeV1::FastSearchTanh => 3,
    });
    let schema = &view.schema;
    for text in [
        schema.version,
        schema.registry_version,
        schema.contract_digest,
        schema.encoding_digest,
    ] {
        key.extend_from_slice(&(text.len() as u64).to_le_bytes());
        key.extend_from_slice(text.as_bytes());
    }
    for dim in [
        schema.state_dim,
        schema.object_feature_dim,
        schema.edge_feature_dim,
        schema.action_feature_dim,
        schema.object_group_count,
        schema.action_ref_feature_dim,
    ] {
        key.extend_from_slice(&(dim as u64).to_le_bytes());
    }
    let floats = |key: &mut Vec<u8>, values: &[f32]| {
        key.extend_from_slice(&(values.len() as u64).to_le_bytes());
        key.reserve(values.len() * 4);
        for value in values {
            key.extend_from_slice(&value.to_bits().to_le_bytes());
        }
    };
    let integers = |key: &mut Vec<u8>, values: &[i64]| {
        key.extend_from_slice(&(values.len() as u64).to_le_bytes());
        key.reserve(values.len() * 8);
        for value in values {
            key.extend_from_slice(&value.to_le_bytes());
        }
    };
    floats(key, view.state);
    floats(key, view.object_features);
    integers(key, view.object_card_ids);
    integers(key, view.object_groups);
    integers(key, view.object_node_ids);
    floats(key, view.edge_features);
    integers(key, view.edge_source_indices);
    integers(key, view.edge_target_indices);
    floats(key, view.action_features);
    floats(key, view.action_ref_features);
    integers(key, view.action_ref_card_ids);
    integers(key, view.action_ref_action_indices);
    integers(key, view.action_ref_node_indices);
}

/// Bucketing hash (not an identity: hits compare keys in full).
pub(crate) fn hash_v1(key: &[u8]) -> u64 {
    let mut hash = 0x9e37_79b9_7f4a_7c15_u64 ^ (key.len() as u64).rotate_left(32);
    let mut chunks = key.chunks_exact(8);
    for chunk in &mut chunks {
        let word = u64::from_le_bytes(chunk.try_into().expect("eight-byte chunk"));
        hash = (hash ^ word).wrapping_mul(0xff51_afd7_ed55_8ccd);
        hash ^= hash >> 32;
    }
    let mut tail = [0u8; 8];
    tail[..chunks.remainder().len()].copy_from_slice(chunks.remainder());
    hash = (hash ^ u64::from_le_bytes(tail)).wrapping_mul(0xc4ce_b9fe_1a85_ec53);
    hash ^ (hash >> 29)
}

struct EntryV1 {
    key: Box<[u8]>,
    logits: Box<[f32]>,
    value: f32,
}

impl EntryV1 {
    fn cost(&self) -> usize {
        self.key.len() + self.logits.len() * 4 + ENTRY_OVERHEAD_BYTES_V1
    }
}

/// Counters since the last reset. `forwards` counts actual forwards made
/// by policies holding this cache (every miss, plus bypasses).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct InferenceCacheStatsV1 {
    pub(crate) requests: u64,
    pub(crate) hits: u64,
    pub(crate) misses: u64,
    /// Same bucket hash, different key bytes (counted within `misses`).
    pub(crate) collisions: u64,
    pub(crate) inserts: u64,
    /// Outputs not stored because one entry alone exceeds the budget.
    pub(crate) oversize: u64,
    pub(crate) evictions: u64,
    pub(crate) forwards: u64,
    pub(crate) key_bytes: u64,
    pub(crate) max_key_bytes: u64,
    pub(crate) peak_bytes: u64,
    pub(crate) peak_entries: u64,
}

/// One worker's cache. Shared by its role policies through [`SharedV1`].
pub(crate) struct InferenceCacheV1 {
    budget: usize,
    bytes: usize,
    by_hash: HashMap<u64, Vec<usize>>,
    entries: Vec<Option<EntryV1>>,
    order: VecDeque<(u64, usize)>,
    free: Vec<usize>,
    stats: InferenceCacheStatsV1,
}

pub(crate) type SharedV1 = Arc<Mutex<InferenceCacheV1>>;

impl InferenceCacheV1 {
    pub(crate) fn new(budget: usize) -> Self {
        Self {
            budget,
            bytes: 0,
            by_hash: HashMap::new(),
            entries: Vec::new(),
            order: VecDeque::new(),
            free: Vec::new(),
            stats: InferenceCacheStatsV1::default(),
        }
    }

    pub(crate) fn shared(budget: usize) -> SharedV1 {
        Arc::new(Mutex::new(Self::new(budget)))
    }

    /// Drops every entry and counter (start of a search).
    pub(crate) fn reset(&mut self) {
        let budget = self.budget;
        *self = Self::new(budget);
    }

    #[cfg(test)]
    pub(crate) fn stats(&self) -> &InferenceCacheStatsV1 {
        &self.stats
    }

    pub(crate) fn bytes(&self) -> usize {
        self.bytes
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.order.len()
    }

    /// A stored output whose key equals `key` in full; counts the request.
    pub(crate) fn get(&mut self, hash: u64, key: &[u8]) -> Option<(Vec<f32>, f32)> {
        self.stats.requests += 1;
        self.stats.key_bytes += key.len() as u64;
        self.stats.max_key_bytes = self.stats.max_key_bytes.max(key.len() as u64);
        let mut same_hash = false;
        if let Some(bucket) = self.by_hash.get(&hash) {
            for &index in bucket {
                let Some(entry) = self.entries[index].as_ref() else {
                    continue;
                };
                same_hash = true;
                if *entry.key == *key {
                    self.stats.hits += 1;
                    return Some((entry.logits.to_vec(), entry.value));
                }
            }
        }
        self.stats.misses += 1;
        if same_hash {
            self.stats.collisions += 1;
        }
        None
    }

    /// Stores a forward's output after a miss; evicts oldest first.
    pub(crate) fn insert(&mut self, hash: u64, key: &[u8], logits: &[f32], value: f32) {
        let entry = EntryV1 {
            key: key.into(),
            logits: logits.into(),
            value,
        };
        let cost = entry.cost();
        if cost > self.budget {
            self.stats.oversize += 1;
            return;
        }
        while self.bytes + cost > self.budget {
            let Some((old_hash, old_index)) = self.order.pop_front() else {
                break;
            };
            if let Some(old) = self.entries[old_index].take() {
                self.bytes -= old.cost();
                self.stats.evictions += 1;
            }
            if let Some(bucket) = self.by_hash.get_mut(&old_hash) {
                bucket.retain(|&index| index != old_index);
                if bucket.is_empty() {
                    self.by_hash.remove(&old_hash);
                }
            }
            self.free.push(old_index);
        }
        let index = match self.free.pop() {
            Some(index) => {
                self.entries[index] = Some(entry);
                index
            }
            None => {
                self.entries.push(Some(entry));
                self.entries.len() - 1
            }
        };
        self.by_hash.entry(hash).or_default().push(index);
        self.order.push_back((hash, index));
        self.bytes += cost;
        self.stats.inserts += 1;
        self.stats.peak_bytes = self.stats.peak_bytes.max(self.bytes as u64);
        self.stats.peak_entries = self.stats.peak_entries.max(self.order.len() as u64);
    }

    pub(crate) fn count_forward(&mut self) {
        self.stats.forwards += 1;
    }

    pub(crate) fn json(&self) -> Value {
        let s = &self.stats;
        json!({"schema":"inference-cache-v1","budget_bytes":self.budget,
            "requests":s.requests,"hits":s.hits,"misses":s.misses,"collisions":s.collisions,
            "inserts":s.inserts,"oversize":s.oversize,"evictions":s.evictions,
            "forwards":s.forwards,"key_bytes":s.key_bytes,"max_key_bytes":s.max_key_bytes,
            "bytes":self.bytes,"entries":self.order.len(),
            "peak_bytes":s.peak_bytes,"peak_entries":s.peak_entries})
    }
}

/// A policy's handle: the worker's cache and this policy's model identity.
#[derive(Clone)]
pub(crate) struct PolicyCacheV1 {
    pub(crate) cache: SharedV1,
    pub(crate) model: [u8; 32],
}

/// What one scoring call did, for passive presentation captures only.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CallOutcomeV1 {
    /// No cache attached: the forward ran.
    Uncached,
    /// Cache attached, no stored identical input: the forward ran.
    Miss,
    /// Cache attached and an identical input's output was returned.
    Hit,
}

/// One scoring call seen by an enabled probe. `input` is the SHA-256 of the
/// exact key bytes; a recorder turns it into a recording-local group ordinal
/// and never publishes it or the key.
#[derive(Clone, Debug)]
#[cfg_attr(
    not(feature = "viewer-recording-v1"),
    allow(dead_code, reason = "read by the viewer recorder only")
)]
pub(crate) struct CallEventV1 {
    pub(crate) outcome: CallOutcomeV1,
    pub(crate) input: [u8; 32],
    pub(crate) key_bytes: usize,
    /// Cache bytes after the call (None without a cache).
    pub(crate) cache_bytes: Option<usize>,
}

/// Passive per-policy call probe. Off unless a recorder enables it; when on,
/// the key is built even without a cache so identical inputs can be grouped.
/// It never changes scores, sampling or cache contents.
#[derive(Clone, Debug)]
pub(crate) struct ProbeV1 {
    pub(crate) model: [u8; 32],
    pub(crate) calls: u32,
    pub(crate) last: Option<CallEventV1>,
}

impl ProbeV1 {
    pub(crate) fn record(
        &mut self,
        outcome: CallOutcomeV1,
        key: &[u8],
        cache_bytes: Option<usize>,
    ) {
        use sha2::{Digest, Sha256};
        self.calls += 1;
        self.last = Some(CallEventV1 {
            outcome,
            input: Sha256::digest(key).into(),
            key_bytes: key.len(),
            cache_bytes,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(byte: u8, len: usize) -> Vec<u8> {
        vec![byte; len]
    }

    #[test]
    fn hit_requires_equal_key_bytes() {
        let mut cache = InferenceCacheV1::new(1 << 20);
        let a = key(1, 64);
        assert_eq!(cache.get(hash_v1(&a), &a), None);
        cache.insert(hash_v1(&a), &a, &[0.5, -1.0], 0.25);
        assert_eq!(cache.get(hash_v1(&a), &a), Some((vec![0.5, -1.0], 0.25)));
        // A forced bucket collision with different bytes is a miss.
        let b = key(2, 64);
        assert_eq!(cache.get(hash_v1(&a), &b), None);
        let s = cache.stats();
        assert_eq!((s.requests, s.hits, s.misses, s.collisions), (3, 1, 2, 1));
    }

    #[test]
    fn colliding_entries_coexist_and_resolve_by_bytes() {
        let mut cache = InferenceCacheV1::new(1 << 20);
        let (a, b) = (key(1, 16), key(2, 16));
        cache.insert(7, &a, &[1.0], 1.0);
        cache.insert(7, &b, &[2.0], 2.0);
        assert_eq!(cache.get(7, &a), Some((vec![1.0], 1.0)));
        assert_eq!(cache.get(7, &b), Some((vec![2.0], 2.0)));
    }

    struct Rows {
        state: Vec<f32>,
        objects: Vec<f32>,
        ids: Vec<i64>,
        actions: Vec<f32>,
    }

    fn view_key(
        rows: &Rows,
        model: u8,
        path: ForwardPathV1,
        mode: ForwardActivationModeV1,
    ) -> Vec<u8> {
        use crate::native_policy_value_net_v1::NativeEncodedDecisionSchemaV1;
        let view = NativeEncodedDecisionViewV1::from_slices_unvalidated(
            NativeEncodedDecisionSchemaV1::contract_v1(),
            &rows.state,
            &rows.objects,
            &rows.ids,
            &[],
            &[],
            &[],
            &[],
            &[],
            &rows.actions,
            &[],
            &[],
            &[],
            &[],
        );
        let mut key = Vec::new();
        write_key_v1(&mut key, &[model; 32], path, mode, &view);
        key
    }

    #[test]
    fn key_covers_identity_mode_path_values_order_and_boundaries() {
        let base = Rows {
            state: vec![0.5, 1.0],
            objects: vec![1.0, 2.0, 3.0],
            ids: vec![7],
            actions: vec![1.0, 2.0, 3.0, 4.0],
        };
        let (flat, libm) = (ForwardPathV1::Flat, ForwardActivationModeV1::LibmTanh);
        let k = view_key(&base, 1, flat, libm);
        assert_eq!(k, view_key(&base, 1, flat, libm));
        assert_ne!(k, view_key(&base, 2, flat, libm));
        assert_ne!(k, view_key(&base, 1, ForwardPathV1::TransferV3, libm));
        assert_ne!(
            k,
            view_key(&base, 1, flat, ForwardActivationModeV1::FastSearchTanh)
        );
        let variants = [
            // Two action rows of width two, reordered.
            Rows {
                actions: vec![3.0, 4.0, 1.0, 2.0],
                ..clone_rows(&base)
            },
            // One ulp in one value.
            Rows {
                state: vec![0.5, 1.0 + f32::EPSILON],
                ..clone_rows(&base)
            },
            Rows {
                ids: vec![8],
                ..clone_rows(&base)
            },
            // Same concatenated values, different slice boundary.
            Rows {
                state: vec![0.5, 1.0, 1.0],
                objects: vec![2.0, 3.0],
                ..clone_rows(&base)
            },
            // Empty and single action rows.
            Rows {
                actions: vec![],
                ..clone_rows(&base)
            },
            Rows {
                actions: vec![1.0, 2.0],
                ..clone_rows(&base)
            },
        ];
        let mut seen = vec![k];
        for v in &variants {
            let key = view_key(v, 1, flat, libm);
            assert!(!seen.contains(&key));
            seen.push(key);
        }
        let zero = |x: f32| Rows {
            state: vec![x, 1.0],
            ..clone_rows(&base)
        };
        assert_ne!(
            view_key(&zero(0.0), 1, flat, libm),
            view_key(&zero(-0.0), 1, flat, libm)
        );
    }

    fn clone_rows(r: &Rows) -> Rows {
        Rows {
            state: r.state.clone(),
            objects: r.objects.clone(),
            ids: r.ids.clone(),
            actions: r.actions.clone(),
        }
    }

    #[test]
    fn eviction_is_fifo_and_bounded() {
        let per = 100 + 4 + ENTRY_OVERHEAD_BYTES_V1;
        let mut cache = InferenceCacheV1::new(per * 2);
        let keys: Vec<_> = (0..3).map(|i| key(i, 100)).collect();
        for (i, k) in keys.iter().enumerate() {
            cache.insert(hash_v1(k), k, &[i as f32], 0.0);
            assert!(cache.bytes() <= per * 2);
        }
        assert_eq!(cache.len(), 2);
        assert_eq!(cache.get(hash_v1(&keys[0]), &keys[0]), None);
        assert!(cache.get(hash_v1(&keys[1]), &keys[1]).is_some());
        assert!(cache.get(hash_v1(&keys[2]), &keys[2]).is_some());
        assert_eq!(cache.stats().evictions, 1);
        // An entry larger than the whole budget is never stored.
        let big = key(9, per * 3);
        cache.insert(hash_v1(&big), &big, &[0.0], 0.0);
        assert_eq!(cache.stats().oversize, 1);
        assert_eq!(cache.len(), 2);
        cache.reset();
        assert_eq!((cache.len(), cache.bytes()), (0, 0));
        assert_eq!(cache.stats(), &InferenceCacheStatsV1::default());
    }
}
