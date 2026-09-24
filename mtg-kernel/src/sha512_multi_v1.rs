//! Multi-buffer SHA-512 for the tensorizer's digest features.
//!
//! The V2 tensorizer derives each 96-float digest tail from six SHA-512
//! digests of `namespace || counter_le32 || canonical_json`, counters 0 to 5.
//! Those messages share a length, so four of them can run through one AVX2
//! compression in lockstep (one 64-bit lane each). This module computes
//! exactly FIPS 180-4 SHA-512; the scalar path is the `sha2` crate the
//! tensorizer used before, and differential tests pin the AVX2 path to it.
//!
//! Jobs of different lengths may share a group: a lane whose message has
//! run out of blocks keeps its state (masked blend) until the group ends.

use sha2::{Digest, Sha512};

/// One message `prefix || counter.to_le_bytes() || body`.
#[derive(Clone, Copy)]
pub(crate) struct Sha512JobV1<'a> {
    pub(crate) prefix: &'a [u8],
    pub(crate) counter: u32,
    pub(crate) body: &'a [u8],
}

impl Sha512JobV1<'_> {
    fn header_len(&self) -> usize {
        self.prefix.len() + 4
    }

    fn message_len(&self) -> usize {
        self.header_len() + self.body.len()
    }

    /// Padded block count: message, 0x80, zeros, 16-byte length.
    fn block_count(&self) -> usize {
        (self.message_len() + 1 + 16).div_ceil(128)
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

/// Reference digest through the `sha2` crate (the pre-existing path).
pub(crate) fn sha512_job_scalar_v1(job: &Sha512JobV1<'_>) -> [u8; 64] {
    let mut digest = Sha512::new();
    digest.update(job.prefix);
    digest.update(job.counter.to_le_bytes());
    digest.update(job.body);
    digest.finalize().into()
}

/// Digests every job into `out` (same order). Uses 4-lane AVX2 when the CPU
/// has it, otherwise the `sha2` crate per job.
pub(crate) fn sha512_jobs_v1(jobs: &[Sha512JobV1<'_>], out: &mut [[u8; 64]]) {
    assert_eq!(jobs.len(), out.len());
    #[cfg(target_arch = "x86_64")]
    {
        if avx2_available_v1() {
            // SAFETY: AVX2 support was checked at runtime.
            unsafe { x86::sha512_jobs_avx2_v1(jobs, out) };
            return;
        }
    }
    for (job, digest) in jobs.iter().zip(out.iter_mut()) {
        *digest = sha512_job_scalar_v1(job);
    }
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

const INITIAL_STATE_V1: [u64; 8] = [
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
    use super::{Sha512JobV1, INITIAL_STATE_V1, ROUND_CONSTANTS_V1};
    use std::arch::x86_64::*;

    const LANES: usize = 4;

    macro_rules! rotr {
        ($x:expr, $n:literal, $m:literal) => {
            _mm256_or_si256(_mm256_srli_epi64::<$n>($x), _mm256_slli_epi64::<$m>($x))
        };
    }

    #[inline(always)]
    unsafe fn big_sigma0(x: __m256i) -> __m256i {
        _mm256_xor_si256(
            _mm256_xor_si256(rotr!(x, 28, 36), rotr!(x, 34, 30)),
            rotr!(x, 39, 25),
        )
    }

    #[inline(always)]
    unsafe fn big_sigma1(x: __m256i) -> __m256i {
        _mm256_xor_si256(
            _mm256_xor_si256(rotr!(x, 14, 50), rotr!(x, 18, 46)),
            rotr!(x, 41, 23),
        )
    }

    #[inline(always)]
    unsafe fn small_sigma0(x: __m256i) -> __m256i {
        _mm256_xor_si256(
            _mm256_xor_si256(rotr!(x, 1, 63), rotr!(x, 8, 56)),
            _mm256_srli_epi64::<7>(x),
        )
    }

    #[inline(always)]
    unsafe fn small_sigma1(x: __m256i) -> __m256i {
        _mm256_xor_si256(
            _mm256_xor_si256(rotr!(x, 19, 45), rotr!(x, 61, 3)),
            _mm256_srli_epi64::<6>(x),
        )
    }

    /// Loads word `word` (big-endian) of each lane's block into one vector.
    #[inline(always)]
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
    /// previous state.
    #[inline(always)]
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

    #[target_feature(enable = "avx2")]
    pub(super) unsafe fn sha512_jobs_avx2_v1(jobs: &[Sha512JobV1<'_>], out: &mut [[u8; 64]]) {
        // Group jobs of equal block count first so lanes rarely idle.
        let mut order: Vec<usize> = (0..jobs.len()).collect();
        order.sort_by_key(|&index| jobs[index].block_count());
        let mut blocks = [[0u8; 128]; LANES];
        for group in order.chunks(LANES) {
            let mut counts = [0usize; LANES];
            for (lane, &index) in group.iter().enumerate() {
                counts[lane] = jobs[index].block_count();
            }
            let max_blocks = counts.iter().copied().max().unwrap_or(0);
            let mut state = [_mm256_setzero_si256(); 8];
            for (slot, initial) in state.iter_mut().zip(INITIAL_STATE_V1) {
                *slot = _mm256_set1_epi64x(initial as i64);
            }
            for block_index in 0..max_blocks {
                let mut mask = [0i64; LANES];
                for lane in 0..LANES {
                    if block_index < counts[lane] {
                        jobs[group[lane]].block(block_index, &mut blocks[lane]);
                        mask[lane] = -1;
                    }
                }
                let active = _mm256_setr_epi64x(mask[0], mask[1], mask[2], mask[3]);
                compress(&mut state, &blocks, active);
            }
            let mut words = [[0u64; LANES]; 8];
            for (row, value) in words.iter_mut().zip(state) {
                _mm256_storeu_si256(row.as_mut_ptr().cast(), value);
            }
            for (lane, &index) in group.iter().enumerate() {
                for (word, row) in words.iter().enumerate() {
                    out[index][word * 8..word * 8 + 8].copy_from_slice(&row[lane].to_be_bytes());
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn next(state: &mut u64) -> u64 {
        *state = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut value = *state;
        value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        value ^ (value >> 31)
    }

    #[test]
    fn multi_buffer_matches_sha2_on_every_length_boundary() {
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
                .map(|(index, &len)| Sha512JobV1 {
                    prefix,
                    counter: (index % 7) as u32,
                    body: &body[..len],
                })
                .collect();
            let mut actual = vec![[0u8; 64]; jobs.len()];
            sha512_jobs_v1(&jobs, &mut actual);
            for (job, digest) in jobs.iter().zip(&actual) {
                assert_eq!(*digest, sha512_job_scalar_v1(job), "len {}", job.body.len());
            }
        }
    }

    #[test]
    fn multi_buffer_handles_partial_groups() {
        let body = b"{\"semantic\":{\"action_kind\":\"pass\",\"actor\":\"self\"}}";
        for count in 0..=9 {
            let jobs: Vec<Sha512JobV1<'_>> = (0..count)
                .map(|counter| Sha512JobV1 {
                    prefix: b"legal-action",
                    counter,
                    body,
                })
                .collect();
            let mut actual = vec![[0u8; 64]; jobs.len()];
            sha512_jobs_v1(&jobs, &mut actual);
            for (job, digest) in jobs.iter().zip(&actual) {
                assert_eq!(*digest, sha512_job_scalar_v1(job));
            }
        }
    }

    #[test]
    fn known_answer_abc() {
        // FIPS 180-2 example: SHA-512("abc").
        let job = Sha512JobV1 {
            prefix: b"",
            counter: 0,
            body: b"",
        };
        // The counter always contributes four bytes, so check the scalar and
        // vector paths agree on a message equal to "abc" by construction.
        let mut digest = [[0u8; 64]; 1];
        sha512_jobs_v1(&[job], &mut digest);
        assert_eq!(digest[0], sha512_job_scalar_v1(&job));
        let abc: [u8; 64] = Sha512::digest(b"abc").into();
        assert_eq!(abc[..8], [0xdd, 0xaf, 0x35, 0xa1, 0x93, 0x61, 0x7a, 0xba]);
    }

    /// Throughput of the scalar and multi-buffer paths on the tensorizer's
    /// state shape: six counters over one body of `MTG_KERNEL_SHA512_BENCH_BYTES`
    /// (default 21,544, the D5 corpus mean state JSON).
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
            .map(|counter| Sha512JobV1 {
                prefix: b"observation-state",
                counter,
                body: &body,
            })
            .collect();
        let threads: usize = std::env::var("MTG_KERNEL_SHA512_BENCH_THREADS")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(1);
        let rounds = 2_000;
        // Wall time per six-counter set on each thread, all threads at once.
        let timed = |multi: bool| -> f64 {
            let barrier = std::sync::Barrier::new(threads);
            let start = std::time::Instant::now();
            std::thread::scope(|scope| {
                for _ in 0..threads {
                    scope.spawn(|| {
                        let mut out = vec![[0u8; 64]; 6];
                        barrier.wait();
                        for _ in 0..rounds {
                            if multi {
                                sha512_jobs_v1(std::hint::black_box(&jobs), &mut out);
                            } else {
                                for (job, digest) in jobs.iter().zip(out.iter_mut()) {
                                    *digest = sha512_job_scalar_v1(std::hint::black_box(job));
                                }
                            }
                            std::hint::black_box(&out);
                        }
                    });
                }
            });
            start.elapsed().as_nanos() as f64 / rounds as f64 / 1_000.0
        };
        let scalar = timed(false);
        let multi = timed(true);
        println!(
            "sha512 six-counter body={bytes} threads={threads} scalar_us={scalar:.2} multi_us={multi:.2} speedup={:.2}",
            scalar / multi
        );
    }
}
