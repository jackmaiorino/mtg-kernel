//! Disposable SIMD lanes8 feasibility probe per SIMD-FORWARD-DESIGN-DRAFT.md
//! revision 7 (SHA f44e7bb49526c330af599110c7946217d342be35bf420a49663a3bda9441986b).
//!
//! OUTSIDE ALL AUTHORITY: mints no Runs, checkpoints, or evidence; timing is
//! indicative only. One baseline-safe executable; no global +avx2. The kernel
//! exists as two separately named never-inlined symbols sharing one source
//! definition: a baseline symbol and a runtime-guarded
//! `#[target_feature(enable = "avx2")]` symbol. Bit agreement between them and
//! an independently written reference of the same frozen lane semantics is
//! required on adversarial and production-shaped vectors; disassembly of the
//! guarded symbol happens outside this binary (asm emission + grep in the
//! probe script).
//!
//! Frozen lane semantics (design section "Kernel semantics"): 8 zero-initialized
//! f32 accumulators; element i goes to lane i mod 8 in increasing i (contiguous
//! 8-chunks, tail element k to lane k); no FMA (separate mul then add per op);
//! reduction ((l0+l1)+(l2+l3)) + ((l4+l5)+(l6+l7)); bias added last.

use std::time::Instant;

const PRODUCTION_DOT_LENGTHS: [usize; 8] = [114, 169, 128, 1499, 89, 259, 128, 64];

#[inline(always)]
fn lanes8_dot_core(input: &[f32], weight: &[f32]) -> f32 {
    debug_assert_eq!(input.len(), weight.len());
    let mut lanes = [0.0f32; 8];
    let chunks = input.len() / 8;
    for chunk in 0..chunks {
        let base = chunk * 8;
        for lane in 0..8 {
            lanes[lane] += input[base + lane] * weight[base + lane];
        }
    }
    for index in (chunks * 8)..input.len() {
        lanes[index % 8] += input[index] * weight[index];
    }
    ((lanes[0] + lanes[1]) + (lanes[2] + lanes[3]))
        + ((lanes[4] + lanes[5]) + (lanes[6] + lanes[7]))
}

/// Baseline symbol: whatever the default target (SSE2 on x86-64-pc-windows-msvc)
/// emits for the frozen source semantics.
#[inline(never)]
#[no_mangle]
pub fn mtg_kernel_probe_lanes8_dot_baseline_v1(input: &[f32], weight: &[f32]) -> f32 {
    lanes8_dot_core(input, weight)
}

/// Guarded symbol: identical source, AVX2 codegen permitted inside this
/// function only. Reached exclusively through the runtime dispatch guard.
///
/// # Safety
///
/// Caller must have verified AVX2 via runtime detection.
#[inline(never)]
#[no_mangle]
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
pub unsafe fn mtg_kernel_probe_lanes8_dot_avx2_v1(input: &[f32], weight: &[f32]) -> f32 {
    lanes8_dot_core(input, weight)
}

/// Independent reference: same frozen semantics written as an explicit
/// strided walk (lane j sums i = j, j+8, ...), structurally different source
/// so shared-codegen artifacts cannot hide an op-sequence change.
fn lanes8_dot_reference_v1(input: &[f32], weight: &[f32]) -> f32 {
    let mut lanes = [0.0f32; 8];
    for (lane, lane_acc) in lanes.iter_mut().enumerate() {
        let mut index = lane;
        while index < input.len() {
            let product = input[index] * weight[index];
            *lane_acc += product;
            index += 8;
        }
    }
    let left = (lanes[0] + lanes[1]) + (lanes[2] + lanes[3]);
    let right = (lanes[4] + lanes[5]) + (lanes[6] + lanes[7]);
    left + right
}

/// Production's current semantics: single sequential accumulator. Timing
/// datum only; its bits legitimately differ from lanes8.
#[inline(never)]
#[no_mangle]
pub fn mtg_kernel_probe_sequential_dot_v1(input: &[f32], weight: &[f32]) -> f32 {
    let mut value = 0.0f32;
    for index in 0..input.len() {
        value += input[index] * weight[index];
    }
    value
}

fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

fn pseudo_uniform_f32(state: &mut u64, scale: f32) -> f32 {
    let raw = (splitmix64(state) >> 40) as u32;
    (raw as f32 / (1u32 << 24) as f32 * 2.0 - 1.0) * scale
}

struct AdversarialCase {
    name: &'static str,
    input: Vec<f32>,
    weight: Vec<f32>,
}

fn adversarial_cases() -> Vec<AdversarialCase> {
    let mut cases = Vec::new();
    // Cancellation: large alternating terms that nearly cancel; any
    // reassociation or fusion changes the bits.
    for &len in &[64usize, 89, 1499] {
        let mut input = Vec::with_capacity(len);
        let mut weight = Vec::with_capacity(len);
        for i in 0..len {
            let sign = if i % 2 == 0 { 1.0f32 } else { -1.0f32 };
            input.push(sign * 1.0e7f32 + i as f32);
            weight.push(1.0f32 + (i as f32) * 1.0e-7f32);
        }
        cases.push(AdversarialCase {
            name: "cancellation",
            input,
            weight,
        });
    }
    // Subnormals: products land in the subnormal range; FTZ/DAZ deviations
    // or fusion show up immediately.
    for &len in &[114usize, 259] {
        let mut input = Vec::with_capacity(len);
        let mut weight = Vec::with_capacity(len);
        for i in 0..len {
            input.push(f32::from_bits(0x0080_0001 + (i as u32 % 64)));
            weight.push(f32::from_bits(0x3F00_0000 - (i as u32 % 16)));
        }
        cases.push(AdversarialCase {
            name: "subnormal",
            input,
            weight,
        });
    }
    // Signed zero boundaries: exact zero products of both signs interleaved
    // with tiny nonzeros; ordering changes flip the accumulated sign of zero
    // before the first nonzero lands.
    for &len in &[128usize, 169] {
        let mut input = Vec::with_capacity(len);
        let mut weight = Vec::with_capacity(len);
        for i in 0..len {
            match i % 4 {
                0 => {
                    input.push(-0.0);
                    weight.push(5.0);
                }
                1 => {
                    input.push(0.0);
                    weight.push(-3.0);
                }
                2 => {
                    input.push(-1.0e-38);
                    weight.push(1.0e-2);
                }
                _ => {
                    input.push(1.0e-38);
                    weight.push(-1.0e-2);
                }
            }
        }
        cases.push(AdversarialCase {
            name: "signed-zero",
            input,
            weight,
        });
    }
    // Production-shaped pseudorandom rows at every real dot length.
    let mut state = 0x5EED_CAFE_F00D_1234u64;
    for &len in &PRODUCTION_DOT_LENGTHS {
        let mut input = Vec::with_capacity(len);
        let mut weight = Vec::with_capacity(len);
        for _ in 0..len {
            input.push(pseudo_uniform_f32(&mut state, 2.0));
            weight.push(pseudo_uniform_f32(&mut state, 0.5));
        }
        cases.push(AdversarialCase {
            name: "production-shaped",
            input,
            weight,
        });
    }
    cases
}

#[cfg(target_arch = "x86_64")]
// This disposable probe reads MXCSR through the plain intrinsic rather than
// the inline-assembly replacement, to keep this frozen record's code
// unchanged from the design revision it was measured against; accepted.
#[allow(deprecated)]
fn capture_environment() -> (u32, bool, bool, bool, bool) {
    let mxcsr = unsafe { std::arch::x86_64::_mm_getcsr() };
    let avx = std::arch::is_x86_feature_detected!("avx");
    let avx2 = std::arch::is_x86_feature_detected!("avx2");
    let ftz = mxcsr & (1 << 15) != 0;
    let daz = mxcsr & (1 << 6) != 0;
    (mxcsr, avx, avx2, ftz, daz)
}

fn main() {
    println!("PROBE simd-lanes8-feasibility-v1 (non-evidence, outside authority)");
    println!("design: revision 7 f44e7bb49526c330af599110c7946217d342be35bf420a49663a3bda9441986b");

    #[cfg(target_arch = "x86_64")]
    {
        let (mxcsr, avx, avx2, ftz, daz) = capture_environment();
        let rounding = (mxcsr >> 13) & 0b11;
        println!(
            "ENVIRONMENT mxcsr={mxcsr:#010x} rounding_mode={rounding} (0=nearest) \
             ftz={ftz} daz={daz} avx={avx} avx2={avx2}"
        );
        if rounding != 0 || ftz || daz {
            println!("PROBE FAIL: numerical environment not round-nearest/no-FTZ/no-DAZ");
            std::process::exit(2);
        }
        if !avx2 {
            println!("PROBE FAIL: runtime AVX2 not eligible on this host");
            std::process::exit(3);
        }

        let cases = adversarial_cases();
        let mut mismatches = 0u32;
        for (ordinal, case) in cases.iter().enumerate() {
            let baseline = mtg_kernel_probe_lanes8_dot_baseline_v1(&case.input, &case.weight);
            let guarded = unsafe { mtg_kernel_probe_lanes8_dot_avx2_v1(&case.input, &case.weight) };
            let reference = lanes8_dot_reference_v1(&case.input, &case.weight);
            let ok = baseline.to_bits() == guarded.to_bits()
                && baseline.to_bits() == reference.to_bits();
            if !ok {
                mismatches += 1;
                println!(
                    "BIT MISMATCH case={} len={} ordinal={} baseline={:#010x} guarded={:#010x} reference={:#010x}",
                    case.name, case.input.len(), ordinal,
                    baseline.to_bits(), guarded.to_bits(), reference.to_bits()
                );
            }
        }
        println!(
            "BIT-EQUIVALENCE cases={} mismatches={} verdict={}",
            cases.len(),
            mismatches,
            if mismatches == 0 { "PASS" } else { "FAIL" }
        );
        if mismatches != 0 {
            std::process::exit(4);
        }

        // Indicative timing only: per the design, a toy-kernel speedup is not
        // production evidence. black_box on arguments and results plus input
        // rotation defeats pure-call hoisting/memoization, which the first
        // harness version did not (its sequential figure was physically
        // impossible for a strict dependent FP-add chain).
        let mut state = 0xBEEF_BEEF_BEEF_0001u64;
        const ROTATION: usize = 8;
        for &len in &PRODUCTION_DOT_LENGTHS {
            let inputs: Vec<Vec<f32>> = (0..ROTATION)
                .map(|_| {
                    (0..len)
                        .map(|_| pseudo_uniform_f32(&mut state, 1.0))
                        .collect()
                })
                .collect();
            let weights: Vec<Vec<f32>> = (0..ROTATION)
                .map(|_| {
                    (0..len)
                        .map(|_| pseudo_uniform_f32(&mut state, 1.0))
                        .collect()
                })
                .collect();
            let iterations = (40_000_000 / len.max(1)).max(10_000);
            let mut sink = 0.0f32;
            let sequential_start = Instant::now();
            for i in 0..iterations {
                let input = std::hint::black_box(&inputs[i % ROTATION]);
                let weight = std::hint::black_box(&weights[i % ROTATION]);
                sink += std::hint::black_box(mtg_kernel_probe_sequential_dot_v1(input, weight));
            }
            let sequential_ns = sequential_start.elapsed().as_nanos() as f64 / iterations as f64;
            let baseline_start = Instant::now();
            for i in 0..iterations {
                let input = std::hint::black_box(&inputs[i % ROTATION]);
                let weight = std::hint::black_box(&weights[i % ROTATION]);
                sink +=
                    std::hint::black_box(mtg_kernel_probe_lanes8_dot_baseline_v1(input, weight));
            }
            let baseline_ns = baseline_start.elapsed().as_nanos() as f64 / iterations as f64;
            let guarded_start = Instant::now();
            for i in 0..iterations {
                let input = std::hint::black_box(&inputs[i % ROTATION]);
                let weight = std::hint::black_box(&weights[i % ROTATION]);
                sink += std::hint::black_box(unsafe {
                    mtg_kernel_probe_lanes8_dot_avx2_v1(input, weight)
                });
            }
            let guarded_ns = guarded_start.elapsed().as_nanos() as f64 / iterations as f64;
            println!(
                "TIMING len={len} sequential_ns={sequential_ns:.1} baseline_lanes8_ns={baseline_ns:.1} \
                 guarded_avx2_ns={guarded_ns:.1} seq_over_guarded={:.2} checksum={sink:e}",
                sequential_ns / guarded_ns
            );
        }
        println!("PROBE COMPLETE verdict=PASS (disassembly witness runs outside this binary)");
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        println!("PROBE FAIL: x86_64 only");
        std::process::exit(1);
    }
}
