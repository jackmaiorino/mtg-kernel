//! DIAGNOSTIC BUILD ONLY (never merged): counts what the memoized digest path
//! hashes per decision and prints cumulative totals to stderr whenever a
//! thread that used it exits. Output of the tensorizer is unchanged.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::io::Write;
use std::sync::Mutex;

const RING: usize = 256;

#[derive(Clone, Default)]
pub(super) struct DiagV1 {
    pub threads: u64,
    pub calls: u64,
    pub state_msgs: u64,
    pub state_bytes: u64,
    pub state_hits: u64,
    pub state_misses: u64,
    pub state_miss_blocks_total: u64,
    pub state_miss_blocks_resumed: u64,
    pub state_miss_unresumed: u64,
    pub state_len_hist: [u64; 24],
    /// Over the misses: blocks that could be skipped if the 16-entry cache
    /// had checkpoints every 1, 2, 4, 8 blocks and no key requirement.
    pub ideal_cache_resumed: [u64; 4],
    /// Same, against the last 256 state messages on this thread (every 1, 8).
    pub ideal_ring_resumed: [u64; 2],
    /// Exact state repeats among the last 256 on this thread.
    pub ring_exact: u64,
    /// First differing byte against the previous state on this thread (log2).
    pub prev_firstdiff_hist: [u64; 24],
    pub action_msgs: u64,
    pub action_bytes: u64,
    pub action_hits: u64,
    pub action_miss_blocks: u64,
    pub action_len_hist: [u64; 24],
    pub ns_prepare: u64,
    pub ns_state_memo_hash: u64,
    pub ns_action_memo: u64,
    pub ns_kernel: u64,
    pub ns_finish: u64,
}

fn bucket(value: usize) -> usize {
    (usize::BITS - value.leading_zeros()) as usize
}

impl DiagV1 {
    fn add(&mut self, o: &DiagV1) {
        macro_rules! s {
            ($($f:ident),*) => { $( self.$f += o.$f; )* };
        }
        s!(threads, calls, state_msgs, state_bytes, state_hits, state_misses,
           state_miss_blocks_total, state_miss_blocks_resumed, state_miss_unresumed,
           ring_exact, action_msgs, action_bytes, action_hits, action_miss_blocks,
           ns_prepare, ns_state_memo_hash, ns_action_memo, ns_kernel, ns_finish);
        for i in 0..24 {
            self.state_len_hist[i] += o.state_len_hist[i];
            self.prev_firstdiff_hist[i] += o.prev_firstdiff_hist[i];
            self.action_len_hist[i] += o.action_len_hist[i];
        }
        for i in 0..4 {
            self.ideal_cache_resumed[i] += o.ideal_cache_resumed[i];
        }
        for i in 0..2 {
            self.ideal_ring_resumed[i] += o.ideal_ring_resumed[i];
        }
    }

    fn json(&self) -> String {
        format!(
            concat!(
                "{{\"threads\":{},\"calls\":{},\"state_msgs\":{},\"state_bytes\":{},",
                "\"state_hits\":{},\"state_misses\":{},\"state_miss_blocks_total\":{},",
                "\"state_miss_blocks_resumed\":{},\"state_miss_unresumed\":{},",
                "\"state_len_hist_log2\":{:?},\"ideal_cache_resumed_every_1_2_4_8\":{:?},",
                "\"ideal_ring256_resumed_every_1_8\":{:?},\"ring_exact\":{},",
                "\"prev_firstdiff_hist_log2\":{:?},\"action_msgs\":{},\"action_bytes\":{},",
                "\"action_hits\":{},\"action_miss_blocks\":{},\"action_len_hist_log2\":{:?},",
                "\"ns_prepare\":{},\"ns_state_memo_hash\":{},\"ns_action_memo\":{},",
                "\"ns_kernel\":{},\"ns_finish\":{}}}"
            ),
            self.threads, self.calls, self.state_msgs, self.state_bytes, self.state_hits,
            self.state_misses, self.state_miss_blocks_total, self.state_miss_blocks_resumed,
            self.state_miss_unresumed, self.state_len_hist, self.ideal_cache_resumed,
            self.ideal_ring_resumed, self.ring_exact, self.prev_firstdiff_hist,
            self.action_msgs, self.action_bytes, self.action_hits, self.action_miss_blocks,
            self.action_len_hist, self.ns_prepare, self.ns_state_memo_hash,
            self.ns_action_memo, self.ns_kernel, self.ns_finish
        )
    }
}

static GLOBAL: Mutex<Option<DiagV1>> = Mutex::new(None);

pub(super) struct ThreadDiagV1 {
    pub d: DiagV1,
    ring: VecDeque<Vec<u8>>,
    prev: Vec<u8>,
}

impl Drop for ThreadDiagV1 {
    fn drop(&mut self) {
        if self.d.calls == 0 {
            return;
        }
        self.d.threads = 1;
        let line = {
            let Ok(mut global) = GLOBAL.lock() else { return };
            let total = global.get_or_insert_with(DiagV1::default);
            total.add(&self.d);
            total.json()
        };
        let _ = writeln!(std::io::stderr(), "DIGEST_DIAG_V1 {line}");
    }
}

thread_local! {
    pub(super) static DIAG: RefCell<ThreadDiagV1> = RefCell::new(ThreadDiagV1 {
        d: DiagV1::default(),
        ring: VecDeque::new(),
        prev: Vec::new(),
    });
}

pub(super) fn with<R>(f: impl FnOnce(&mut DiagV1) -> R) -> R {
    DIAG.with(|cell| f(&mut cell.borrow_mut().d))
}

fn usable_blocks(shared: usize, json_len: usize) -> usize {
    let header = super::STATE_HEADER_BYTES_V1;
    ((header + shared) / 128).min((header + json_len) / 128)
}

/// Called for every state message that is not empty (hit or miss), before
/// the message is hashed and before the prefix cache is updated.
pub(super) fn observe_state(json: &[u8], hit: bool, resumed: usize) {
    let header = super::STATE_HEADER_BYTES_V1;
    let total_blocks = (header + json.len() + 1 + 16).div_ceil(128);
    // Cache entries as they are before this message is inserted.
    let best_cache = super::STATE_PREFIX_CACHE_V1.with(|cell| {
        cell.borrow()
            .entries
            .iter()
            .map(|entry| super::common_prefix_len_v1(json, &entry.json))
            .max()
            .unwrap_or(0)
    });
    DIAG.with(|cell| {
        let mut t = cell.borrow_mut();
        let t = &mut *t;
        let (mut best_ring, mut exact) = (0usize, false);
        for old in &t.ring {
            let shared = super::common_prefix_len_v1(json, old);
            exact |= shared == json.len() && old.len() == json.len();
            best_ring = best_ring.max(shared);
        }
        let firstdiff = super::common_prefix_len_v1(json, &t.prev);
        let d = &mut t.d;
        d.state_msgs += 1;
        d.state_bytes += json.len() as u64;
        d.state_len_hist[bucket(json.len()).min(23)] += 1;
        d.prev_firstdiff_hist[bucket(firstdiff).min(23)] += 1;
        if exact {
            d.ring_exact += 1;
        }
        if hit {
            d.state_hits += 1;
        } else {
            d.state_misses += 1;
            d.state_miss_blocks_total += total_blocks as u64;
            d.state_miss_blocks_resumed += resumed as u64;
            if resumed == 0 {
                d.state_miss_unresumed += 1;
            }
            let usable = usable_blocks(best_cache, json.len());
            for (slot, every) in [1usize, 2, 4, 8].into_iter().enumerate() {
                d.ideal_cache_resumed[slot] += (usable - usable % every) as u64;
            }
            let usable = usable_blocks(best_ring, json.len());
            d.ideal_ring_resumed[0] += usable as u64;
            d.ideal_ring_resumed[1] += (usable - usable % 8) as u64;
        }
        let _ = header;
        if t.ring.len() == RING {
            let mut old = t.ring.pop_front().expect("full ring");
            old.clear();
            old.extend_from_slice(json);
            t.ring.push_back(old);
        } else {
            t.ring.push_back(json.to_vec());
        }
        t.prev.clear();
        t.prev.extend_from_slice(json);
    });
}

pub(super) fn observe_action(len: usize, hit: bool) {
    with(|d| {
        d.action_msgs += 1;
        d.action_bytes += len as u64;
        d.action_len_hist[bucket(len).min(23)] += 1;
        if hit {
            d.action_hits += 1;
        } else {
            d.action_miss_blocks += (super::ACTION_HASH_NAMESPACE_V1.len() + 4 + len + 17)
                .div_ceil(128) as u64;
        }
    });
}
