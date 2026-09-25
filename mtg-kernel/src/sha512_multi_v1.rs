//! Multi-buffer SHA-512 for the tensorizer's digest features.
//!
//! The V2 tensorizer derives each 96-float digest tail from six SHA-512
//! digests of `namespace || counter_le32 || canonical_json`, counters 0 to 5.
//! Those messages share a length, so four of them can be compressed in
//! lockstep in the 64-bit lanes of one AVX2 compression. This module computes
//! exactly FIPS 180-4 SHA-512; the scalar path is the `sha2` crate the
//! tensorizer used before (its `compress512` for resumed or checkpointed
//! jobs), and differential tests pin the AVX2 path to it.
//!
//! Measured on the six-counter state shape (i7-13700K): four AVX2 lanes plus
//! two `sha2` jobs are about 1.4x faster than six `sha2` jobs, at one thread
//! and at 24. Interleaving two scalar lanes into the AVX2 round loop was
//! slower (1.2x) and was removed.
//!
//! A job may resume from a chaining state taken after its first
//! `start_block` blocks, and the caller may ask for the chaining state at
//! every `every`-th data-only block (a block holding no padding). SHA-512 is
//! Merkle-Damgard, so the state after block `k` depends only on the first
//! `k + 1` blocks: a resume is exact whenever those blocks are byte-identical,
//! which the caller must establish by comparing bytes.
//!
//! Scheduling: longest job first, and a lane takes the next job as soon as its
//! own ends. Once the queue is empty, a lane with no job left is masked (its
//! state kept by a blend) while at least three jobs still run; the last one or
//! two jobs finish through `sha2::compress512` from their chaining states.

use sha2::{Digest, Sha512};

/// One message `prefix || counter.to_le_bytes() || body`, optionally resumed
/// from `start_state` after its first `start_block` blocks.
#[derive(Clone, Copy)]
pub(crate) struct Sha512JobV1<'a> {
    pub(crate) prefix: &'a [u8],
    pub(crate) counter: u32,
    pub(crate) body: &'a [u8],
    pub(crate) start_block: usize,
    pub(crate) start_state: [u64; 8],
}

impl<'a> Sha512JobV1<'a> {
    pub(crate) fn new(prefix: &'a [u8], counter: u32, body: &'a [u8]) -> Self {
        Self {
            prefix,
            counter,
            body,
            start_block: 0,
            start_state: INITIAL_STATE_V1,
        }
    }

    /// Resumes after `start_block` blocks whose chaining state is `state`.
    /// Only data-only blocks may be skipped.
    pub(crate) fn resumed(mut self, start_block: usize, state: [u64; 8]) -> Self {
        assert!(start_block <= self.data_block_count());
        self.start_block = start_block;
        self.start_state = state;
        self
    }

    fn header_len(&self) -> usize {
        self.prefix.len() + 4
    }

    fn message_len(&self) -> usize {
        self.header_len() + self.body.len()
    }

    /// Padded block count: message, 0x80, zeros, 16-byte length.
    pub(crate) fn block_count(&self) -> usize {
        (self.message_len() + 1 + 16).div_ceil(128)
    }

    /// Leading blocks that hold only message bytes (no padding).
    pub(crate) fn data_block_count(&self) -> usize {
        self.message_len() / 128
    }

    fn remaining_blocks(&self) -> usize {
        self.block_count() - self.start_block
    }

    /// Writes padded block `index` into `out`.
    fn block(&self, index: usize, out: &mut [u8; 128]) {
        let start = index * 128;
        let header_len = self.header_len();
        let message_len = self.message_len();
        let blocks = self.block_count();
        // Fast path: the block lies entirely inside the body.
        if start >= header_len && start + 128 <= message_len {
            out.copy_from_slice(&self.body[start - header_len..start - header_len + 128]);
            return;
        }
        for (offset, byte) in out.iter_mut().enumerate() {
            let position = start + offset;
            *byte = if position < self.prefix.len() {
                self.prefix[position]
            } else if position < header_len {
                self.counter.to_le_bytes()[position - self.prefix.len()]
            } else if position < message_len {
                self.body[position - header_len]
            } else if position == message_len {
                0x80
            } else {
                0
            };
        }
        if index + 1 == blocks {
            let bits = (message_len as u128) * 8;
            out[112..128].copy_from_slice(&bits.to_be_bytes());
        }
    }
}

/// Chaining states a batch reports: for job `j`, `(blocks, state)` pairs
/// where `state` follows the first `blocks` blocks, for every multiple of
/// `every` above the job's start and at most its data-only block count.
pub(crate) struct Sha512CheckpointsV1 {
    pub(crate) every: usize,
    pub(crate) states: Vec<Vec<(usize, [u64; 8])>>,
}

impl Sha512CheckpointsV1 {
    fn record(
        &mut self,
        job_index: usize,
        job: &Sha512JobV1<'_>,
        blocks_done: usize,
        state: [u64; 8],
    ) {
        if blocks_done > job.start_block
            && blocks_done.is_multiple_of(self.every)
            && blocks_done <= job.data_block_count()
        {
            self.states[job_index].push((blocks_done, state));
        }
    }
}

/// Reference digest through the `sha2` crate (the pre-existing path) for a
/// job that starts at block zero.
pub(crate) fn sha512_job_scalar_v1(job: &Sha512JobV1<'_>) -> [u8; 64] {
    assert_eq!(job.start_block, 0);
    let mut digest = Sha512::new();
    digest.update(job.prefix);
    digest.update(job.counter.to_le_bytes());
    digest.update(job.body);
    digest.finalize().into()
}

/// How a batch of jobs is scheduled. Every mode computes the same digests.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Sha512ModeV1 {
    /// The `sha2` crate one job at a time.
    #[cfg_attr(not(test), allow(dead_code))]
    Scalar,
    /// Groups of four jobs in the lanes of one AVX2 compression; groups of
    /// one or two go to the `sha2` crate.
    Avx2,
}

/// Digests every job into `out` (same order) with the fastest mode the CPU
/// supports.
pub(crate) fn sha512_jobs_v1(
    jobs: &[Sha512JobV1<'_>],
    out: &mut [[u8; 64]],
    checkpoints: Option<&mut Sha512CheckpointsV1>,
) {
    sha512_jobs_with_mode_v1(default_mode_v1(), jobs, out, checkpoints);
}

/// Digests with an explicit mode (AVX2 modes fall back to scalar without
/// AVX2).
pub(crate) fn sha512_jobs_with_mode_v1(
    mode: Sha512ModeV1,
    jobs: &[Sha512JobV1<'_>],
    out: &mut [[u8; 64]],
    mut checkpoints: Option<&mut Sha512CheckpointsV1>,
) {
    assert_eq!(jobs.len(), out.len());
    if let Some(checkpoints) = checkpoints.as_deref_mut() {
        assert!(checkpoints.every > 0);
        checkpoints.states.clear();
        checkpoints.states.resize(jobs.len(), Vec::new());
    }
    #[cfg(target_arch = "x86_64")]
    {
        if mode != Sha512ModeV1::Scalar && avx2_available_v1() {
            // SAFETY: AVX2 support was checked at runtime.
            unsafe { x86::sha512_jobs_avx2_v1(jobs, out, checkpoints) };
            return;
        }
    }
    for (index, (job, digest)) in jobs.iter().zip(out.iter_mut()).enumerate() {
        *digest = scalar_job_v1(index, job, checkpoints.as_deref_mut());
    }
}

fn default_mode_v1() -> Sha512ModeV1 {
    Sha512ModeV1::Avx2
}

#[cfg(target_arch = "x86_64")]
fn avx2_available_v1() -> bool {
    use std::sync::OnceLock;
    static AVAILABLE: OnceLock<bool> = OnceLock::new();
    *AVAILABLE.get_or_init(|| {
        std::env::var_os("MTG_KERNEL_SHA512_FORCE_SCALAR").is_none()
            && std::arch::is_x86_feature_detected!("avx2")
    })
}

fn digest_bytes_v1(state: &[u64; 8]) -> [u8; 64] {
    let mut out = [0u8; 64];
    for (bytes, word) in out.chunks_exact_mut(8).zip(state) {
        bytes.copy_from_slice(&word.to_be_bytes());
    }
    out
}

/// One job through the `sha2` crate: `Sha512` itself for a fresh job without
/// checkpoints, otherwise `sha2::compress512` (the compression `Sha512` uses)
/// over runs of blocks between checkpoints.
fn scalar_job_v1(
    index: usize,
    job: &Sha512JobV1<'_>,
    checkpoints: Option<&mut Sha512CheckpointsV1>,
) -> [u8; 64] {
    if job.start_block == 0 && checkpoints.is_none() {
        return sha512_job_scalar_v1(job);
    }
    scalar_job_v1_from(index, job, job.start_block, checkpoints)
}

/// Continues `job` from `job.start_block` (any block, padding included) and
/// records checkpoints as for a job that started at `original_start`.
fn scalar_job_v1_from(
    index: usize,
    job: &Sha512JobV1<'_>,
    original_start: usize,
    mut checkpoints: Option<&mut Sha512CheckpointsV1>,
) -> [u8; 64] {
    let recorded = Sha512JobV1 {
        start_block: original_start,
        ..*job
    };
    let total = job.block_count();
    let mut state = job.start_state;
    let mut next = job.start_block;
    while next < total {
        let boundary = match checkpoints.as_deref() {
            Some(checkpoints) => (next / checkpoints.every + 1) * checkpoints.every,
            None => total,
        }
        .min(total);
        compress_range_v1(job, &mut state, next, boundary);
        next = boundary;
        if let Some(checkpoints) = checkpoints.as_deref_mut() {
            checkpoints.record(index, &recorded, next, state);
        }
    }
    digest_bytes_v1(&state)
}

/// Compresses blocks `from..to` of `job` into `state` with `sha2::compress512`.
/// Blocks lying wholly inside the body are one contiguous body slice and are
/// passed without copying; the header block and padded blocks are assembled.
fn compress_range_v1(job: &Sha512JobV1<'_>, state: &mut [u64; 8], from: usize, to: usize) {
    use sha2::digest::generic_array::{typenum::U128, GenericArray};
    let header_len = job.header_len();
    let message_len = job.message_len();
    let mut block = [0u8; 128];
    let mut index = from;
    while index < to {
        let start = index * 128;
        if start >= header_len && start + 128 <= message_len {
            // Longest run of body-only blocks from here (the header block is
            // excluded by the check above, so every later block starts in
            // the body).
            let mut end = index;
            while end < to && end * 128 + 128 <= message_len {
                end += 1;
            }
            let bytes = &job.body[start - header_len..end * 128 - header_len];
            // SAFETY: GenericArray<u8, U128> is a [u8; 128] (align 1), and
            // `bytes` holds exactly `end - index` whole blocks; sha2 itself
            // casts the same way inside compress512.
            let blocks = unsafe {
                std::slice::from_raw_parts(
                    bytes.as_ptr().cast::<GenericArray<u8, U128>>(),
                    end - index,
                )
            };
            sha2::compress512(state, blocks);
            index = end;
        } else {
            job.block(index, &mut block);
            sha2::compress512(
                state,
                std::slice::from_ref(GenericArray::from_slice(&block)),
            );
            index += 1;
        }
    }
}

pub(crate) const INITIAL_STATE_V1: [u64; 8] = [
    0x6a09_e667_f3bc_c908,
    0xbb67_ae85_84ca_a73b,
    0x3c6e_f372_fe94_f82b,
    0xa54f_f53a_5f1d_36f1,
    0x510e_527f_ade6_82d1,
    0x9b05_688c_2b3e_6c1f,
    0x1f83_d9ab_fb41_bd6b,
    0x5be0_cd19_137e_2179,
];

const ROUND_CONSTANTS_V1: [u64; 80] = [
    0x428a_2f98_d728_ae22,
    0x7137_4491_23ef_65cd,
    0xb5c0_fbcf_ec4d_3b2f,
    0xe9b5_dba5_8189_dbbc,
    0x3956_c25b_f348_b538,
    0x59f1_11f1_b605_d019,
    0x923f_82a4_af19_4f9b,
    0xab1c_5ed5_da6d_8118,
    0xd807_aa98_a303_0242,
    0x1283_5b01_4570_6fbe,
    0x2431_85be_4ee4_b28c,
    0x550c_7dc3_d5ff_b4e2,
    0x72be_5d74_f27b_896f,
    0x80de_b1fe_3b16_96b1,
    0x9bdc_06a7_25c7_1235,
    0xc19b_f174_cf69_2694,
    0xe49b_69c1_9ef1_4ad2,
    0xefbe_4786_384f_25e3,
    0x0fc1_9dc6_8b8c_d5b5,
    0x240c_a1cc_77ac_9c65,
    0x2de9_2c6f_592b_0275,
    0x4a74_84aa_6ea6_e483,
    0x5cb0_a9dc_bd41_fbd4,
    0x76f9_88da_8311_53b5,
    0x983e_5152_ee66_dfab,
    0xa831_c66d_2db4_3210,
    0xb003_27c8_98fb_213f,
    0xbf59_7fc7_beef_0ee4,
    0xc6e0_0bf3_3da8_8fc2,
    0xd5a7_9147_930a_a725,
    0x06ca_6351_e003_826f,
    0x1429_2967_0a0e_6e70,
    0x27b7_0a85_46d2_2ffc,
    0x2e1b_2138_5c26_c926,
    0x4d2c_6dfc_5ac4_2aed,
    0x5338_0d13_9d95_b3df,
    0x650a_7354_8baf_63de,
    0x766a_0abb_3c77_b2a8,
    0x81c2_c92e_47ed_aee6,
    0x9272_2c85_1482_353b,
    0xa2bf_e8a1_4cf1_0364,
    0xa81a_664b_bc42_3001,
    0xc24b_8b70_d0f8_9791,
    0xc76c_51a3_0654_be30,
    0xd192_e819_d6ef_5218,
    0xd699_0624_5565_a910,
    0xf40e_3585_5771_202a,
    0x106a_a070_32bb_d1b8,
    0x19a4_c116_b8d2_d0c8,
    0x1e37_6c08_5141_ab53,
    0x2748_774c_df8e_eb99,
    0x34b0_bcb5_e19b_48a8,
    0x391c_0cb3_c5c9_5a63,
    0x4ed8_aa4a_e341_8acb,
    0x5b9c_ca4f_7763_e373,
    0x682e_6ff3_d6b2_b8a3,
    0x748f_82ee_5def_b2fc,
    0x78a5_636f_4317_2f60,
    0x84c8_7814_a1f0_ab72,
    0x8cc7_0208_1a64_39ec,
    0x90be_fffa_2363_1e28,
    0xa450_6ceb_de82_bde9,
    0xbef9_a3f7_b2c6_7915,
    0xc671_78f2_e372_532b,
    0xca27_3ece_ea26_619c,
    0xd186_b8c7_21c0_c207,
    0xeada_7dd6_cde0_eb1e,
    0xf57d_4f7f_ee6e_d178,
    0x06f0_67aa_7217_6fba,
    0x0a63_7dc5_a2c8_98a6,
    0x113f_9804_bef9_0dae,
    0x1b71_0b35_131c_471b,
    0x28db_77f5_2304_7d84,
    0x32ca_ab7b_40c7_2493,
    0x3c9e_be0a_15c9_bebc,
    0x431d_67c4_9c10_0d4c,
    0x4cc5_d4be_cb3e_42b6,
    0x597f_299c_fc65_7e2a,
    0x5fcb_6fab_3ad6_faec,
    0x6c44_198c_4a47_5817,
];

#[cfg(target_arch = "x86_64")]
mod x86 {
    use super::{digest_bytes_v1, Sha512CheckpointsV1, Sha512JobV1, ROUND_CONSTANTS_V1};
    use std::arch::x86_64::*;

    // Every helper carries the `avx2` target feature itself, so the AVX2
    // intrinsics can always be inlined into it.

    const LANES: usize = 4;

    macro_rules! rotr {
        ($x:expr, $n:literal, $m:literal) => {
            _mm256_or_si256(_mm256_srli_epi64::<$n>($x), _mm256_slli_epi64::<$m>($x))
        };
    }

    #[inline]
    #[target_feature(enable = "avx2")]
    unsafe fn big_sigma0(x: __m256i) -> __m256i {
        _mm256_xor_si256(
            _mm256_xor_si256(rotr!(x, 28, 36), rotr!(x, 34, 30)),
            rotr!(x, 39, 25),
        )
    }

    #[inline]
    #[target_feature(enable = "avx2")]
    unsafe fn big_sigma1(x: __m256i) -> __m256i {
        _mm256_xor_si256(
            _mm256_xor_si256(rotr!(x, 14, 50), rotr!(x, 18, 46)),
            rotr!(x, 41, 23),
        )
    }

    #[inline]
    #[target_feature(enable = "avx2")]
    unsafe fn small_sigma0(x: __m256i) -> __m256i {
        _mm256_xor_si256(
            _mm256_xor_si256(rotr!(x, 1, 63), rotr!(x, 8, 56)),
            _mm256_srli_epi64::<7>(x),
        )
    }

    #[inline]
    #[target_feature(enable = "avx2")]
    unsafe fn small_sigma1(x: __m256i) -> __m256i {
        _mm256_xor_si256(
            _mm256_xor_si256(rotr!(x, 19, 45), rotr!(x, 61, 3)),
            _mm256_srli_epi64::<6>(x),
        )
    }

    /// Loads the sixteen big-endian words of each lane's block, transposed so
    /// vector `i` holds word `i` of every lane.
    #[inline]
    #[target_feature(enable = "avx2")]
    unsafe fn load_words(blocks: &[[u8; 128]; LANES], w: &mut [__m256i; 16]) {
        // Byte swap within each 64-bit element.
        let swap = _mm256_setr_epi8(
            7, 6, 5, 4, 3, 2, 1, 0, 15, 14, 13, 12, 11, 10, 9, 8, 7, 6, 5, 4, 3, 2, 1, 0, 15, 14,
            13, 12, 11, 10, 9, 8,
        );
        for quad in 0..4 {
            let offset = quad * 32;
            let r0 = _mm256_shuffle_epi8(
                _mm256_loadu_si256(blocks[0].as_ptr().add(offset).cast()),
                swap,
            );
            let r1 = _mm256_shuffle_epi8(
                _mm256_loadu_si256(blocks[1].as_ptr().add(offset).cast()),
                swap,
            );
            let r2 = _mm256_shuffle_epi8(
                _mm256_loadu_si256(blocks[2].as_ptr().add(offset).cast()),
                swap,
            );
            let r3 = _mm256_shuffle_epi8(
                _mm256_loadu_si256(blocks[3].as_ptr().add(offset).cast()),
                swap,
            );
            // Transpose 4x4 of 64-bit: row k = lane k's words [4q..4q+4).
            let t0 = _mm256_unpacklo_epi64(r0, r1); // l0w0 l1w0 | l0w2 l1w2
            let t1 = _mm256_unpackhi_epi64(r0, r1); // l0w1 l1w1 | l0w3 l1w3
            let t2 = _mm256_unpacklo_epi64(r2, r3); // l2w0 l3w0 | l2w2 l3w2
            let t3 = _mm256_unpackhi_epi64(r2, r3); // l2w1 l3w1 | l2w3 l3w3
            w[quad * 4] = _mm256_permute2x128_si256::<0x20>(t0, t2);
            w[quad * 4 + 1] = _mm256_permute2x128_si256::<0x20>(t1, t3);
            w[quad * 4 + 2] = _mm256_permute2x128_si256::<0x31>(t0, t2);
            w[quad * 4 + 3] = _mm256_permute2x128_si256::<0x31>(t1, t3);
        }
    }

    /// One compression of four lanes; lanes with `active` all-zero keep their
    /// previous state. (Computing the whole schedule first and unrolling the
    /// rounds measured slower on this kernel's target, an i7-13700K.)
    #[inline]
    #[target_feature(enable = "avx2")]
    unsafe fn compress(state: &mut [__m256i; 8], blocks: &[[u8; 128]; LANES], active: __m256i) {
        let mut w = [_mm256_setzero_si256(); 16];
        load_words(blocks, &mut w);
        let mut a = state[0];
        let mut b = state[1];
        let mut c = state[2];
        let mut d = state[3];
        let mut e = state[4];
        let mut f = state[5];
        let mut g = state[6];
        let mut h = state[7];
        for t in 0..80 {
            let wt = if t < 16 {
                w[t]
            } else {
                let next = _mm256_add_epi64(
                    _mm256_add_epi64(small_sigma1(w[(t - 2) & 15]), w[(t - 7) & 15]),
                    _mm256_add_epi64(small_sigma0(w[(t - 15) & 15]), w[(t - 16) & 15]),
                );
                w[t & 15] = next;
                next
            };
            let k = _mm256_set1_epi64x(ROUND_CONSTANTS_V1[t] as i64);
            let ch = _mm256_xor_si256(_mm256_and_si256(e, f), _mm256_andnot_si256(e, g));
            let t1 = _mm256_add_epi64(
                _mm256_add_epi64(_mm256_add_epi64(h, big_sigma1(e)), _mm256_add_epi64(ch, k)),
                wt,
            );
            let maj = _mm256_xor_si256(
                _mm256_and_si256(a, b),
                _mm256_and_si256(c, _mm256_xor_si256(a, b)),
            );
            let t2 = _mm256_add_epi64(big_sigma0(a), maj);
            h = g;
            g = f;
            f = e;
            e = _mm256_add_epi64(d, t1);
            d = c;
            c = b;
            b = a;
            a = _mm256_add_epi64(t1, t2);
        }
        let next = [a, b, c, d, e, f, g, h];
        for (slot, value) in state.iter_mut().zip(next) {
            let updated = _mm256_add_epi64(*slot, value);
            *slot = _mm256_blendv_epi8(*slot, updated, active);
        }
    }

    #[inline]
    #[target_feature(enable = "avx2")]
    unsafe fn vector_lane_states(state: &[__m256i; 8]) -> [[u64; 8]; LANES] {
        let mut words = [[0u64; LANES]; 8];
        for (row, value) in words.iter_mut().zip(state) {
            _mm256_storeu_si256(row.as_mut_ptr().cast(), *value);
        }
        let mut lanes = [[0u64; 8]; LANES];
        for (lane, lane_state) in lanes.iter_mut().enumerate() {
            for (word, row) in lane_state.iter_mut().zip(&words) {
                *word = row[lane];
            }
        }
        lanes
    }

    #[inline]
    #[target_feature(enable = "avx2")]
    unsafe fn load_lane_states(lanes: &[[u64; 8]; LANES]) -> [__m256i; 8] {
        let mut state = [_mm256_setzero_si256(); 8];
        for (word, slot) in state.iter_mut().enumerate() {
            *slot = _mm256_setr_epi64x(
                lanes[0][word] as i64,
                lanes[1][word] as i64,
                lanes[2][word] as i64,
                lanes[3][word] as i64,
            );
        }
        state
    }

    /// Longest job first; each lane takes the next job the moment its own
    /// finishes, so lanes stay busy whatever the mix of lengths. Once the
    /// queue is empty and at most two jobs are still running, they finish
    /// through the `sha2` crate from their current chaining states (as fast
    /// as a half-empty vector group).
    #[target_feature(enable = "avx2")]
    pub(super) unsafe fn sha512_jobs_avx2_v1(
        jobs: &[Sha512JobV1<'_>],
        out: &mut [[u8; 64]],
        mut checkpoints: Option<&mut Sha512CheckpointsV1>,
    ) {
        let mut order: Vec<usize> = (0..jobs.len()).collect();
        order.sort_by_key(|&index| std::cmp::Reverse(jobs[index].remaining_blocks()));
        let mut queue = order.into_iter().peekable();
        // Per lane: the job and the index of its next block.
        let mut lanes: [Option<(usize, usize)>; LANES] = [None; LANES];
        let mut lane_states = [[0u64; 8]; LANES];
        let mut state = load_lane_states(&lane_states);
        let mut blocks = [[0u8; 128]; LANES];
        loop {
            let mut refilled = false;
            for lane in 0..LANES {
                if lanes[lane].is_none() {
                    if let Some(index) = queue.next() {
                        lanes[lane] = Some((index, jobs[index].start_block));
                        lane_states[lane] = jobs[index].start_state;
                        refilled = true;
                    }
                }
            }
            let running = lanes.iter().filter(|lane| lane.is_some()).count();
            if running == 0 {
                break;
            }
            if running <= 2 && queue.peek().is_none() {
                // Scalar tail: continue each job from its current state.
                for lane in 0..LANES {
                    if let Some((index, next_block)) = lanes[lane] {
                        let tail = Sha512JobV1 {
                            start_block: next_block,
                            start_state: lane_states[lane],
                            ..jobs[index]
                        };
                        out[index] = super::scalar_job_v1_from(
                            index,
                            &tail,
                            jobs[index].start_block,
                            checkpoints.as_deref_mut(),
                        );
                    }
                }
                break;
            }
            if refilled {
                state = load_lane_states(&lane_states);
            }
            let mut mask = [0i64; LANES];
            for lane in 0..LANES {
                if let Some((index, next_block)) = lanes[lane] {
                    jobs[index].block(next_block, &mut blocks[lane]);
                    mask[lane] = -1;
                }
            }
            let active = _mm256_setr_epi64x(mask[0], mask[1], mask[2], mask[3]);
            compress(&mut state, &blocks, active);
            // Advance every lane; extract lane states only when a job ends or
            // reaches a checkpoint.
            let mut extract = false;
            for lane in lanes.iter_mut().flatten() {
                lane.1 += 1;
                let job = &jobs[lane.0];
                if lane.1 == job.block_count() {
                    extract = true;
                }
                if let Some(checkpoints) = checkpoints.as_deref() {
                    if lane.1.is_multiple_of(checkpoints.every) && lane.1 <= job.data_block_count()
                    {
                        extract = true;
                    }
                }
            }
            if extract {
                lane_states = vector_lane_states(&state);
                for lane in 0..LANES {
                    if let Some((index, done)) = lanes[lane] {
                        let job = &jobs[index];
                        if let Some(checkpoints) = checkpoints.as_deref_mut() {
                            checkpoints.record(index, job, done, lane_states[lane]);
                        }
                        if done == job.block_count() {
                            out[index] = digest_bytes_v1(&lane_states[lane]);
                            lanes[lane] = None;
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MODES: [Sha512ModeV1; 2] = [Sha512ModeV1::Scalar, Sha512ModeV1::Avx2];

    fn next(state: &mut u64) -> u64 {
        *state = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut value = *state;
        value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        value ^ (value >> 31)
    }

    #[test]
    fn every_mode_matches_sha2_on_every_length_boundary() {
        let mut random = 0x5eed_u64;
        let body: Vec<u8> = (0..4_096).map(|_| next(&mut random) as u8).collect();
        let prefixes: [&[u8]; 3] = [b"observation-state", b"legal-action", b""];
        // Every body length 0..=600 (all padding boundaries for each prefix)
        // plus longer ones, mixed into groups of unequal lengths.
        let mut lengths: Vec<usize> = (0..=600).collect();
        lengths.extend([1_000, 2_047, 2_048, 2_049, 4_096]);
        for prefix in prefixes {
            let jobs: Vec<Sha512JobV1<'_>> = lengths
                .iter()
                .enumerate()
                .map(|(index, &len)| Sha512JobV1::new(prefix, (index % 7) as u32, &body[..len]))
                .collect();
            for mode in MODES {
                let mut actual = vec![[0u8; 64]; jobs.len()];
                sha512_jobs_with_mode_v1(mode, &jobs, &mut actual, None);
                for (job, digest) in jobs.iter().zip(&actual) {
                    assert_eq!(
                        *digest,
                        sha512_job_scalar_v1(job),
                        "{mode:?} len {}",
                        job.body.len()
                    );
                }
            }
        }
    }

    #[test]
    fn every_mode_handles_partial_groups() {
        let body = b"{\"semantic\":{\"action_kind\":\"pass\",\"actor\":\"self\"}}";
        for count in 0..=13 {
            let jobs: Vec<Sha512JobV1<'_>> = (0..count)
                .map(|counter| Sha512JobV1::new(b"legal-action", counter, body))
                .collect();
            for mode in MODES {
                let mut actual = vec![[0u8; 64]; jobs.len()];
                sha512_jobs_with_mode_v1(mode, &jobs, &mut actual, None);
                for (job, digest) in jobs.iter().zip(&actual) {
                    assert_eq!(*digest, sha512_job_scalar_v1(job), "{mode:?} count {count}");
                }
            }
        }
    }

    /// Checkpoints taken in one mode resume exactly in every mode, from every
    /// checkpoint, including jobs whose messages differ after the resume
    /// point.
    #[test]
    fn checkpoints_resume_exactly_in_every_mode() {
        let mut random = 0x0c0f_fee0_u64;
        let base: Vec<u8> = (0..3_000).map(|_| next(&mut random) as u8).collect();
        let mut variant = base.clone();
        variant[2_000] ^= 0x5a;
        variant.truncate(2_700);
        let prefix = b"observation-state";
        for record_mode in MODES {
            let jobs: Vec<Sha512JobV1<'_>> = (0..6)
                .map(|counter| Sha512JobV1::new(prefix, counter, &base))
                .collect();
            let mut digests = vec![[0u8; 64]; 6];
            let mut checkpoints = Sha512CheckpointsV1 {
                every: 2,
                states: Vec::new(),
            };
            sha512_jobs_with_mode_v1(record_mode, &jobs, &mut digests, Some(&mut checkpoints));
            let data_blocks = jobs[0].data_block_count();
            for (job, digest) in jobs.iter().zip(&digests) {
                assert_eq!(*digest, sha512_job_scalar_v1(job));
            }
            for states in &checkpoints.states {
                let expected: Vec<usize> = (1..=data_blocks / 2).map(|k| k * 2).collect();
                assert_eq!(
                    states.iter().map(|(blocks, _)| *blocks).collect::<Vec<_>>(),
                    expected
                );
            }
            for resume_mode in MODES {
                for checkpoint in 0..checkpoints.states[0].len() {
                    for body in [&base[..], &variant[..]] {
                        // The variant differs at body byte 2,000 (message
                        // byte 2,021, block 15), so only resumes after at
                        // most 15 blocks apply to it.
                        let blocks = checkpoints.states[0][checkpoint].0;
                        if body.len() != base.len() && blocks * 128 > 21 + 2_000 {
                            continue;
                        }
                        let resumed: Vec<Sha512JobV1<'_>> = (0..6)
                            .map(|counter| {
                                let (blocks, state) =
                                    checkpoints.states[counter as usize][checkpoint];
                                Sha512JobV1::new(prefix, counter, body).resumed(blocks, state)
                            })
                            .collect();
                        let mut actual = vec![[0u8; 64]; 6];
                        sha512_jobs_with_mode_v1(resume_mode, &resumed, &mut actual, None);
                        for (counter, digest) in actual.iter().enumerate() {
                            let fresh = Sha512JobV1::new(prefix, counter as u32, body);
                            assert_eq!(
                                *digest,
                                sha512_job_scalar_v1(&fresh),
                                "record {record_mode:?} resume {resume_mode:?} checkpoint {checkpoint}"
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn known_answer_matches_fips_vector() {
        // Every mode against sha2 on a message made only of the four counter
        // bytes ("abcd"), and sha2 itself against the FIPS 180-2 "abc" vector.
        let expected: [u8; 64] = Sha512::digest(b"abcd").into();
        let job = Sha512JobV1::new(b"", u32::from_le_bytes(*b"abcd"), b"");
        for mode in MODES {
            let jobs = [job; 6];
            let mut out = [[0u8; 64]; 6];
            sha512_jobs_with_mode_v1(mode, &jobs, &mut out, None);
            assert!(out.iter().all(|digest| *digest == expected), "{mode:?}");
        }
        let abc: [u8; 64] = Sha512::digest(b"abc").into();
        assert_eq!(abc[..4], [0xdd, 0xaf, 0x35, 0xa1]);
    }

    /// Throughput of each mode on the tensorizer's state shape: six counters
    /// over one body of `MTG_KERNEL_SHA512_BENCH_BYTES` (default 21,544, the
    /// D5 corpus mean state JSON), on `MTG_KERNEL_SHA512_BENCH_THREADS`
    /// threads at once.
    #[test]
    #[ignore = "timing probe: run explicitly"]
    fn multi_buffer_throughput_probe_v1() {
        let bytes: usize = std::env::var("MTG_KERNEL_SHA512_BENCH_BYTES")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(21_544);
        let mut random = 7_u64;
        let body: Vec<u8> = (0..bytes).map(|_| next(&mut random) as u8).collect();
        let jobs: Vec<Sha512JobV1<'_>> = (0..6)
            .map(|counter| Sha512JobV1::new(b"observation-state", counter, &body))
            .collect();
        let threads: usize = std::env::var("MTG_KERNEL_SHA512_BENCH_THREADS")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(1);
        let rounds = 2_000;
        let timed = |mode: Sha512ModeV1| -> f64 {
            let barrier = std::sync::Barrier::new(threads);
            let start = std::time::Instant::now();
            std::thread::scope(|scope| {
                for _ in 0..threads {
                    scope.spawn(|| {
                        let mut out = vec![[0u8; 64]; 6];
                        barrier.wait();
                        for _ in 0..rounds {
                            sha512_jobs_with_mode_v1(
                                mode,
                                std::hint::black_box(&jobs),
                                &mut out,
                                None,
                            );
                            std::hint::black_box(&out);
                        }
                    });
                }
            });
            start.elapsed().as_nanos() as f64 / rounds as f64 / 1_000.0
        };
        let scalar = timed(Sha512ModeV1::Scalar);
        let avx2 = timed(Sha512ModeV1::Avx2);
        println!(
            "sha512 six-counter body={bytes} threads={threads} scalar_us={scalar:.2} \
             avx2_us={avx2:.2} avx2_speedup={:.2}",
            scalar / avx2
        );
    }
}
