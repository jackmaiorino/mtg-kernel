//! Deterministic f32 kernels for the S transformer (`s_transformer_v1`).
//!
//! Every kernel fixes the order of its floating-point operations: each output
//! element is accumulated in ascending order of its reduction index, starting
//! from its declared initial value, with one rounded multiply and one rounded
//! add per term (Rust never contracts these into a fused multiply-add). The
//! AVX2 and AVX-512 entry points execute the same per-lane IEEE operations on
//! independent output elements, so the bits are the same as the scalar path
//! on every x86-64 CPU and system. Transcendentals use only IEEE add, multiply,
//! divide and exact power-of-two scaling, so they too give the same bits
//! everywhere (no platform libm).

#![allow(clippy::needless_range_loop)]

/// Where a [`gemm_v1`] output starts before its terms are added.
#[derive(Clone, Copy)]
pub(super) enum InitV1<'a> {
    /// Row-broadcast bias: `c[i, j]` starts at `bias[j]`.
    Bias(&'a [f32]),
    /// `c[i, j]` starts at `+0.0`.
    Zero,
    /// `c[i, j]` starts at its current value (accumulation).
    Existing,
}

/// `c[i, j] = init(i, j) + sum over k ascending of a(i, k) * b[k, j]`, with
/// `a(i, k) = a[i * a_rs + k * a_cs]`, `b` row-major `[kdim, n]` and `c`
/// row-major `[m, n]`.
#[allow(clippy::too_many_arguments)]
pub(super) fn gemm_v1(
    m: usize,
    n: usize,
    kdim: usize,
    a: &[f32],
    a_rs: usize,
    a_cs: usize,
    b: &[f32],
    init: InitV1<'_>,
    c: &mut [f32],
) {
    gemm_strided_v1(m, n, kdim, a, a_rs, a_cs, b, n, init, c, n);
}

/// [`gemm_v1`] with explicit row strides `b_rs >= n` and `c_rs >= n` so a
/// column sub-panel of `b` and `c` can be computed in place without copying.
#[allow(clippy::too_many_arguments)]
pub(super) fn gemm_strided_v1(
    m: usize,
    n: usize,
    kdim: usize,
    a: &[f32],
    a_rs: usize,
    a_cs: usize,
    b: &[f32],
    b_rs: usize,
    init: InitV1<'_>,
    c: &mut [f32],
    c_rs: usize,
) {
    assert!(b_rs >= n, "gemm_strided_v1: b_rs < n");
    assert!(c_rs >= n, "gemm_strided_v1: c_rs < n");
    if kdim > 0 && n > 0 {
        assert!((kdim - 1) * b_rs + n <= b.len(), "gemm_v1: b too short");
    }
    if m > 0 && n > 0 {
        assert!((m - 1) * c_rs + n <= c.len(), "gemm_v1: c too short");
    }
    if m > 0 && kdim > 0 {
        assert!(
            (m - 1) * a_rs + (kdim - 1) * a_cs < a.len(),
            "gemm_v1: a too short"
        );
    }
    if let InitV1::Bias(bias) = init {
        assert!(bias.len() >= n, "gemm_v1: bias too short");
    }
    if m == 0 || n == 0 {
        return;
    }
    #[cfg(target_arch = "x86_64")]
    {
        if std::arch::is_x86_feature_detected!("avx512f") {
            // SAFETY: AVX-512F support was detected at run time just above.
            unsafe { gemm_avx512_v1(m, n, kdim, a, a_rs, a_cs, b, b_rs, init, c, c_rs) };
            return;
        }
        if std::arch::is_x86_feature_detected!("avx2") {
            // SAFETY: AVX2 support was detected at run time just above.
            unsafe { gemm_avx2_v1(m, n, kdim, a, a_rs, a_cs, b, b_rs, init, c, c_rs) };
            return;
        }
    }
    gemm_body_v1::<false>(m, n, kdim, a, a_rs, a_cs, b, b_rs, init, c, c_rs);
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx512f")]
#[allow(clippy::too_many_arguments)]
unsafe fn gemm_avx512_v1(
    m: usize,
    n: usize,
    kdim: usize,
    a: &[f32],
    a_rs: usize,
    a_cs: usize,
    b: &[f32],
    b_rs: usize,
    init: InitV1<'_>,
    c: &mut [f32],
    c_rs: usize,
) {
    gemm_body_v1::<true>(m, n, kdim, a, a_rs, a_cs, b, b_rs, init, c, c_rs);
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
#[allow(clippy::too_many_arguments)]
unsafe fn gemm_avx2_v1(
    m: usize,
    n: usize,
    kdim: usize,
    a: &[f32],
    a_rs: usize,
    a_cs: usize,
    b: &[f32],
    b_rs: usize,
    init: InitV1<'_>,
    c: &mut [f32],
    c_rs: usize,
) {
    gemm_body_v1::<false>(m, n, kdim, a, a_rs, a_cs, b, b_rs, init, c, c_rs);
}

const ROWS_V1: usize = 4;

#[inline(always)]
fn init_panel_v1<const COLS: usize>(
    init: InitV1<'_>,
    c: &[f32],
    row_base: usize,
    j0: usize,
) -> [f32; COLS] {
    let mut out = [0.0f32; COLS];
    match init {
        InitV1::Bias(bias) => {
            out.copy_from_slice(&bias[j0..j0 + COLS]);
        }
        InitV1::Zero => {}
        InitV1::Existing => {
            out.copy_from_slice(&c[row_base + j0..row_base + j0 + COLS]);
        }
    }
    out
}

#[inline(always)]
#[allow(clippy::too_many_arguments)]
fn gemm_panel_v1<const COLS: usize>(
    m: usize,
    kdim: usize,
    a: &[f32],
    a_rs: usize,
    a_cs: usize,
    b: &[f32],
    b_rs: usize,
    init: InitV1<'_>,
    c: &mut [f32],
    c_rs: usize,
    j0: usize,
) {
    let mut i0 = 0;
    while i0 + ROWS_V1 <= m {
        let mut acc0 = init_panel_v1::<COLS>(init, c, i0 * c_rs, j0);
        let mut acc1 = init_panel_v1::<COLS>(init, c, (i0 + 1) * c_rs, j0);
        let mut acc2 = init_panel_v1::<COLS>(init, c, (i0 + 2) * c_rs, j0);
        let mut acc3 = init_panel_v1::<COLS>(init, c, (i0 + 3) * c_rs, j0);
        for k in 0..kdim {
            let w: &[f32; COLS] = b[k * b_rs + j0..k * b_rs + j0 + COLS]
                .try_into()
                .expect("panel columns");
            let x0 = a[i0 * a_rs + k * a_cs];
            let x1 = a[(i0 + 1) * a_rs + k * a_cs];
            let x2 = a[(i0 + 2) * a_rs + k * a_cs];
            let x3 = a[(i0 + 3) * a_rs + k * a_cs];
            for q in 0..COLS {
                acc0[q] += x0 * w[q];
                acc1[q] += x1 * w[q];
                acc2[q] += x2 * w[q];
                acc3[q] += x3 * w[q];
            }
        }
        c[i0 * c_rs + j0..i0 * c_rs + j0 + COLS].copy_from_slice(&acc0);
        c[(i0 + 1) * c_rs + j0..(i0 + 1) * c_rs + j0 + COLS].copy_from_slice(&acc1);
        c[(i0 + 2) * c_rs + j0..(i0 + 2) * c_rs + j0 + COLS].copy_from_slice(&acc2);
        c[(i0 + 3) * c_rs + j0..(i0 + 3) * c_rs + j0 + COLS].copy_from_slice(&acc3);
        i0 += ROWS_V1;
    }
    match m - i0 {
        3 => {
            let mut acc0 = init_panel_v1::<COLS>(init, c, i0 * c_rs, j0);
            let mut acc1 = init_panel_v1::<COLS>(init, c, (i0 + 1) * c_rs, j0);
            let mut acc2 = init_panel_v1::<COLS>(init, c, (i0 + 2) * c_rs, j0);
            for k in 0..kdim {
                let w: &[f32; COLS] = b[k * b_rs + j0..k * b_rs + j0 + COLS]
                    .try_into()
                    .expect("panel columns");
                let x0 = a[i0 * a_rs + k * a_cs];
                let x1 = a[(i0 + 1) * a_rs + k * a_cs];
                let x2 = a[(i0 + 2) * a_rs + k * a_cs];
                for q in 0..COLS {
                    acc0[q] += x0 * w[q];
                    acc1[q] += x1 * w[q];
                    acc2[q] += x2 * w[q];
                }
            }
            c[i0 * c_rs + j0..i0 * c_rs + j0 + COLS].copy_from_slice(&acc0);
            c[(i0 + 1) * c_rs + j0..(i0 + 1) * c_rs + j0 + COLS].copy_from_slice(&acc1);
            c[(i0 + 2) * c_rs + j0..(i0 + 2) * c_rs + j0 + COLS].copy_from_slice(&acc2);
        }
        2 => {
            let mut acc0 = init_panel_v1::<COLS>(init, c, i0 * c_rs, j0);
            let mut acc1 = init_panel_v1::<COLS>(init, c, (i0 + 1) * c_rs, j0);
            for k in 0..kdim {
                let w: &[f32; COLS] = b[k * b_rs + j0..k * b_rs + j0 + COLS]
                    .try_into()
                    .expect("panel columns");
                let x0 = a[i0 * a_rs + k * a_cs];
                let x1 = a[(i0 + 1) * a_rs + k * a_cs];
                for q in 0..COLS {
                    acc0[q] += x0 * w[q];
                    acc1[q] += x1 * w[q];
                }
            }
            c[i0 * c_rs + j0..i0 * c_rs + j0 + COLS].copy_from_slice(&acc0);
            c[(i0 + 1) * c_rs + j0..(i0 + 1) * c_rs + j0 + COLS].copy_from_slice(&acc1);
        }
        1 => {
            let mut acc0 = init_panel_v1::<COLS>(init, c, i0 * c_rs, j0);
            for k in 0..kdim {
                let w: &[f32; COLS] = b[k * b_rs + j0..k * b_rs + j0 + COLS]
                    .try_into()
                    .expect("panel columns");
                let x0 = a[i0 * a_rs + k * a_cs];
                for q in 0..COLS {
                    acc0[q] += x0 * w[q];
                }
            }
            c[i0 * c_rs + j0..i0 * c_rs + j0 + COLS].copy_from_slice(&acc0);
        }
        _ => {}
    }
}

#[inline(always)]
#[allow(clippy::too_many_arguments)]
fn gemm_body_v1<const WIDE32: bool>(
    m: usize,
    n: usize,
    kdim: usize,
    a: &[f32],
    a_rs: usize,
    a_cs: usize,
    b: &[f32],
    b_rs: usize,
    init: InitV1<'_>,
    c: &mut [f32],
    c_rs: usize,
) {
    let mut j0 = 0;
    if WIDE32 {
        while j0 + 32 <= n {
            gemm_panel_v1::<32>(m, kdim, a, a_rs, a_cs, b, b_rs, init, c, c_rs, j0);
            j0 += 32;
        }
    }
    while j0 + 16 <= n {
        gemm_panel_v1::<16>(m, kdim, a, a_rs, a_cs, b, b_rs, init, c, c_rs, j0);
        j0 += 16;
    }
    if j0 + 8 <= n {
        gemm_panel_v1::<8>(m, kdim, a, a_rs, a_cs, b, b_rs, init, c, c_rs, j0);
        j0 += 8;
    }
    if j0 + 4 <= n {
        gemm_panel_v1::<4>(m, kdim, a, a_rs, a_cs, b, b_rs, init, c, c_rs, j0);
        j0 += 4;
    }
    while j0 < n {
        gemm_panel_v1::<1>(m, kdim, a, a_rs, a_cs, b, b_rs, init, c, c_rs, j0);
        j0 += 1;
    }
}

#[cfg(test)]
#[inline(always)]
fn init_value_v1(init: InitV1<'_>, c: &[f32], index: usize, column: usize) -> f32 {
    match init {
        InitV1::Bias(bias) => bias[column],
        InitV1::Zero => 0.0,
        InitV1::Existing => c[index],
    }
}

/// The plain triple loop with the same per-element order as [`gemm_v1`];
/// tests compare the two bit for bit.
#[cfg(test)]
#[allow(clippy::too_many_arguments)]
pub(super) fn gemm_reference_v1(
    m: usize,
    n: usize,
    kdim: usize,
    a: &[f32],
    a_rs: usize,
    a_cs: usize,
    b: &[f32],
    init: InitV1<'_>,
    c: &mut [f32],
) {
    for i in 0..m {
        for j in 0..n {
            let mut sum = init_value_v1(init, c, i * n + j, j);
            for k in 0..kdim {
                sum += a[i * a_rs + k * a_cs] * b[k * n + j];
            }
            c[i * n + j] = sum;
        }
    }
}

/// `db[j] += sum over rows ascending of dy[r, j]`.
pub(super) fn bias_grad_v1(rows: usize, n: usize, dy: &[f32], db: &mut [f32]) {
    #[cfg(target_arch = "x86_64")]
    {
        if std::arch::is_x86_feature_detected!("avx2") {
            // SAFETY: AVX2 support was detected at run time just above.
            unsafe { bias_grad_avx2_v1(rows, n, dy, db) };
            return;
        }
    }
    bias_grad_body_v1(rows, n, dy, db);
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn bias_grad_avx2_v1(rows: usize, n: usize, dy: &[f32], db: &mut [f32]) {
    bias_grad_body_v1(rows, n, dy, db);
}

#[inline(always)]
fn bias_grad_body_v1(rows: usize, n: usize, dy: &[f32], db: &mut [f32]) {
    for r in 0..rows {
        let row = &dy[r * n..(r + 1) * n];
        for (slot, value) in db[..n].iter_mut().zip(row) {
            *slot += *value;
        }
    }
}

const LOG2E_V1: f32 = std::f32::consts::LOG2_E;
// ln(2) split as in FreeBSD's expf: k * LN2_HI_V1 is exact for |k| < 2^9.
const LN2_HI_V1: f32 = f32::from_bits(0x3f31_7200);
const LN2_LO_V1: f32 = f32::from_bits(0x35bf_be8e);
// 1.5 * 2^23: adding and subtracting it rounds an f32 of magnitude below
// 2^22 to the nearest integer, ties to even (the pinned rounding mode).
const ROUND_MAGIC_V1: f32 = 12_582_912.0;

/// `exp(x)` for f32: Cody-Waite reduction, a degree-7 Taylor polynomial on
/// `|r| <= ln(2)/2` (truncation below 1e-8 relative) and exact power-of-two
/// scaling. Inputs are clamped to `[-87, 88]`, so the result is a normal
/// number. Only IEEE operations, so the bits are the same everywhere.
#[inline(always)]
pub(super) fn exp_v1(x: f32) -> f32 {
    let x = x.clamp(-87.0, 88.0);
    let kf = (x * LOG2E_V1 + ROUND_MAGIC_V1) - ROUND_MAGIC_V1;
    let r = (x - kf * LN2_HI_V1) - kf * LN2_LO_V1;
    let p = 1.0
        + r * (1.0
            + r * (0.5
                + r * (1.0 / 6.0
                    + r * (1.0 / 24.0
                        + r * (1.0 / 120.0 + r * (1.0 / 720.0 + r * (1.0 / 5040.0)))))));
    let k = kf as i32;
    let scale = f32::from_bits(((k + 127) as u32) << 23);
    p * scale
}

const FRAC_1_SQRT_2_V1: f32 = std::f32::consts::FRAC_1_SQRT_2;
// 1 / sqrt(2 pi)
const INV_SQRT_2PI_V1: f32 = 0.398_942_280_401_432_7_f64 as f32;
// Numerical Recipes (6.2) erfc coefficients.
const ERFC_C_V1: [f32; 10] = [
    -1.265_512_23_f64 as f32,
    1.000_023_68_f64 as f32,
    0.374_091_96_f64 as f32,
    0.096_784_18_f64 as f32,
    -0.186_288_06_f64 as f32,
    0.278_868_07_f64 as f32,
    -1.135_203_98_f64 as f32,
    1.488_515_87_f64 as f32,
    -0.822_152_23_f64 as f32,
    0.170_872_77_f64 as f32,
];

/// The standard normal CDF, `0.5 * erfc(-x / sqrt(2))`, through the
/// Chebyshev-fitted `erfc` of Numerical Recipes (6.2), fractional error below
/// 1.2e-7 in exact arithmetic. Branch-free apart from a final select.
#[inline(always)]
pub(super) fn normal_cdf_v1(x: f32) -> f32 {
    let z = (x * FRAC_1_SQRT_2_V1).abs();
    let t = 1.0 / (1.0 + 0.5 * z);
    let c = ERFC_C_V1;
    let poly = -z * z
        + c[0]
        + t * (c[1]
            + t * (c[2]
                + t * (c[3]
                    + t * (c[4] + t * (c[5] + t * (c[6] + t * (c[7] + t * (c[8] + t * c[9]))))))));
    let half_erfc = 0.5 * (t * exp_v1(poly));
    if x >= 0.0 {
        1.0 - half_erfc
    } else {
        half_erfc
    }
}

/// Exact (erf) GELU, `x * Phi(x)`, PyTorch's `nn.GELU()` default.
#[inline(always)]
pub(super) fn gelu_v1(x: f32) -> f32 {
    x * normal_cdf_v1(x)
}

/// `d gelu / dx = Phi(x) + x * phi(x)`.
#[inline(always)]
pub(super) fn gelu_grad_v1(x: f32) -> f32 {
    normal_cdf_v1(x) + x * (exp_v1(-0.5 * x * x) * INV_SQRT_2PI_V1)
}

#[cfg(target_arch = "x86_64")]
mod avx2_math {
    use super::*;
    use std::arch::x86_64::*;

    // Below 1e18 every square (`z * z`, `-0.5 * x * x`) stays finite and well
    // below f32::MAX, so `vmaxps`/`vminps` match `f32::clamp(-87.0, 88.0)` and
    // `vcvttps2dq` matches `kf as i32` bit for bit across all 8 lanes. Any
    // chunk containing NaN, +/-Inf or a huge magnitude falls back to scalar.
    const SAFE_ABS_LIMIT_V1: f32 = 1.0e18;

    #[target_feature(enable = "avx2")]
    #[inline]
    pub(super) unsafe fn all_safe8_v1(x: __m256) -> bool {
        let abs_mask = _mm256_castsi256_ps(_mm256_set1_epi32(0x7fff_ffff));
        let abs_x = _mm256_and_ps(x, abs_mask);
        let ok = _mm256_cmp_ps::<_CMP_LT_OQ>(abs_x, _mm256_set1_ps(SAFE_ABS_LIMIT_V1));
        _mm256_movemask_ps(ok) == 0xff
    }

    #[target_feature(enable = "avx2")]
    #[inline]
    pub(super) unsafe fn exp8_finite_v1(x: __m256) -> __m256 {
        let x = _mm256_min_ps(
            _mm256_max_ps(x, _mm256_set1_ps(-87.0)),
            _mm256_set1_ps(88.0),
        );
        let magic = _mm256_set1_ps(ROUND_MAGIC_V1);
        let kf = _mm256_sub_ps(
            _mm256_add_ps(_mm256_mul_ps(x, _mm256_set1_ps(LOG2E_V1)), magic),
            magic,
        );
        let r = _mm256_sub_ps(
            _mm256_sub_ps(x, _mm256_mul_ps(kf, _mm256_set1_ps(LN2_HI_V1))),
            _mm256_mul_ps(kf, _mm256_set1_ps(LN2_LO_V1)),
        );
        let mut p = _mm256_add_ps(
            _mm256_set1_ps(1.0 / 720.0),
            _mm256_mul_ps(r, _mm256_set1_ps(1.0 / 5040.0)),
        );
        p = _mm256_add_ps(_mm256_set1_ps(1.0 / 120.0), _mm256_mul_ps(r, p));
        p = _mm256_add_ps(_mm256_set1_ps(1.0 / 24.0), _mm256_mul_ps(r, p));
        p = _mm256_add_ps(_mm256_set1_ps(1.0 / 6.0), _mm256_mul_ps(r, p));
        p = _mm256_add_ps(_mm256_set1_ps(0.5), _mm256_mul_ps(r, p));
        p = _mm256_add_ps(_mm256_set1_ps(1.0), _mm256_mul_ps(r, p));
        p = _mm256_add_ps(_mm256_set1_ps(1.0), _mm256_mul_ps(r, p));
        let k = _mm256_cvttps_epi32(kf);
        let biased = _mm256_add_epi32(k, _mm256_set1_epi32(127));
        let scale = _mm256_castsi256_ps(_mm256_slli_epi32::<23>(biased));
        _mm256_mul_ps(p, scale)
    }

    #[target_feature(enable = "avx2")]
    #[inline]
    pub(super) unsafe fn normal_cdf8_safe_v1(x: __m256) -> __m256 {
        let abs_mask = _mm256_castsi256_ps(_mm256_set1_epi32(0x7fff_ffff));
        let sign_bit = _mm256_set1_ps(-0.0);
        let one = _mm256_set1_ps(1.0);
        let half = _mm256_set1_ps(0.5);
        let z = _mm256_and_ps(_mm256_mul_ps(x, _mm256_set1_ps(FRAC_1_SQRT_2_V1)), abs_mask);
        let t = _mm256_div_ps(one, _mm256_add_ps(one, _mm256_mul_ps(half, z)));
        let c = ERFC_C_V1;
        let mut horner =
            _mm256_add_ps(_mm256_set1_ps(c[8]), _mm256_mul_ps(t, _mm256_set1_ps(c[9])));
        horner = _mm256_add_ps(_mm256_set1_ps(c[7]), _mm256_mul_ps(t, horner));
        horner = _mm256_add_ps(_mm256_set1_ps(c[6]), _mm256_mul_ps(t, horner));
        horner = _mm256_add_ps(_mm256_set1_ps(c[5]), _mm256_mul_ps(t, horner));
        horner = _mm256_add_ps(_mm256_set1_ps(c[4]), _mm256_mul_ps(t, horner));
        horner = _mm256_add_ps(_mm256_set1_ps(c[3]), _mm256_mul_ps(t, horner));
        horner = _mm256_add_ps(_mm256_set1_ps(c[2]), _mm256_mul_ps(t, horner));
        horner = _mm256_add_ps(_mm256_set1_ps(c[1]), _mm256_mul_ps(t, horner));
        let neg_z = _mm256_xor_ps(z, sign_bit);
        let poly = _mm256_add_ps(
            _mm256_add_ps(_mm256_mul_ps(neg_z, z), _mm256_set1_ps(c[0])),
            _mm256_mul_ps(t, horner),
        );
        let half_erfc = _mm256_mul_ps(half, _mm256_mul_ps(t, exp8_finite_v1(poly)));
        let ge_zero = _mm256_cmp_ps::<_CMP_GE_OQ>(x, _mm256_setzero_ps());
        _mm256_blendv_ps(half_erfc, _mm256_sub_ps(one, half_erfc), ge_zero)
    }
}

/// `post[i] = gelu(pre[i])`.
pub(super) fn gelu_slice_v1(pre: &[f32], post: &mut [f32]) {
    #[cfg(target_arch = "x86_64")]
    {
        if std::arch::is_x86_feature_detected!("avx2") {
            // SAFETY: AVX2 support was detected at run time just above.
            unsafe { gelu_slice_avx2_v1(pre, post) };
            return;
        }
    }
    gelu_slice_body_v1(pre, post);
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn gelu_slice_avx2_v1(pre: &[f32], post: &mut [f32]) {
    use std::arch::x86_64::*;
    let len = pre.len().min(post.len());
    let mut i = 0;
    while i + 8 <= len {
        let x = unsafe { _mm256_loadu_ps(pre.as_ptr().add(i)) };
        if unsafe { avx2_math::all_safe8_v1(x) } {
            let y = _mm256_mul_ps(x, unsafe { avx2_math::normal_cdf8_safe_v1(x) });
            unsafe { _mm256_storeu_ps(post.as_mut_ptr().add(i), y) };
        } else {
            for k in 0..8 {
                post[i + k] = gelu_v1(pre[i + k]);
            }
        }
        i += 8;
    }
    while i < len {
        post[i] = gelu_v1(pre[i]);
        i += 1;
    }
}

#[inline(always)]
fn gelu_slice_body_v1(pre: &[f32], post: &mut [f32]) {
    for (out, x) in post.iter_mut().zip(pre) {
        *out = gelu_v1(*x);
    }
}

/// `grad[i] *= gelu'(pre[i])`.
pub(super) fn gelu_backward_slice_v1(pre: &[f32], grad: &mut [f32]) {
    #[cfg(target_arch = "x86_64")]
    {
        if std::arch::is_x86_feature_detected!("avx2") {
            // SAFETY: AVX2 support was detected at run time just above.
            unsafe { gelu_backward_slice_avx2_v1(pre, grad) };
            return;
        }
    }
    gelu_backward_slice_body_v1(pre, grad);
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn gelu_backward_slice_avx2_v1(pre: &[f32], grad: &mut [f32]) {
    use std::arch::x86_64::*;
    let len = pre.len().min(grad.len());
    let neg_half = _mm256_set1_ps(-0.5);
    let inv_sqrt_2pi = _mm256_set1_ps(INV_SQRT_2PI_V1);
    let mut i = 0;
    while i + 8 <= len {
        let x = unsafe { _mm256_loadu_ps(pre.as_ptr().add(i)) };
        if unsafe { avx2_math::all_safe8_v1(x) } {
            let cdf = unsafe { avx2_math::normal_cdf8_safe_v1(x) };
            let arg = _mm256_mul_ps(_mm256_mul_ps(neg_half, x), x);
            let pdf = _mm256_mul_ps(unsafe { avx2_math::exp8_finite_v1(arg) }, inv_sqrt_2pi);
            let dg = _mm256_add_ps(cdf, _mm256_mul_ps(x, pdf));
            let g = unsafe { _mm256_loadu_ps(grad.as_ptr().add(i)) };
            unsafe { _mm256_storeu_ps(grad.as_mut_ptr().add(i), _mm256_mul_ps(g, dg)) };
        } else {
            for k in 0..8 {
                grad[i + k] *= gelu_grad_v1(pre[i + k]);
            }
        }
        i += 8;
    }
    while i < len {
        grad[i] *= gelu_grad_v1(pre[i]);
        i += 1;
    }
}

#[inline(always)]
fn gelu_backward_slice_body_v1(pre: &[f32], grad: &mut [f32]) {
    for (g, x) in grad.iter_mut().zip(pre) {
        *g *= gelu_grad_v1(*x);
    }
}

/// LayerNorm epsilon (PyTorch's default).
pub(super) const LAYER_NORM_EPS_V1: f32 = 1.0e-5;

/// One row of LayerNorm (biased variance). Returns `(mean, rstd)`.
pub(super) fn layer_norm_row_v1(
    x: &[f32],
    gain: &[f32],
    bias: &[f32],
    y: &mut [f32],
) -> (f32, f32) {
    let width = x.len() as f32;
    let mut sum = 0.0f32;
    for value in x {
        sum += *value;
    }
    let mean = sum / width;
    let mut squares = 0.0f32;
    for value in x {
        let centered = *value - mean;
        squares += centered * centered;
    }
    let variance = squares / width;
    let rstd = 1.0 / (variance + LAYER_NORM_EPS_V1).sqrt();
    #[cfg(target_arch = "x86_64")]
    {
        if std::arch::is_x86_feature_detected!("avx2") {
            // SAFETY: AVX2 support was detected at run time just above.
            unsafe { layer_norm_scale_avx2_v1(x, mean, rstd, gain, bias, y) };
            return (mean, rstd);
        }
    }
    layer_norm_scale_body_v1(x, mean, rstd, gain, bias, y);
    (mean, rstd)
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn layer_norm_scale_avx2_v1(
    x: &[f32],
    mean: f32,
    rstd: f32,
    gain: &[f32],
    bias: &[f32],
    y: &mut [f32],
) {
    layer_norm_scale_body_v1(x, mean, rstd, gain, bias, y);
}

#[inline(always)]
fn layer_norm_scale_body_v1(
    x: &[f32],
    mean: f32,
    rstd: f32,
    gain: &[f32],
    bias: &[f32],
    y: &mut [f32],
) {
    let n = y.len();
    assert!(x.len() >= n && gain.len() >= n && bias.len() >= n);
    for index in 0..n {
        y[index] = ((x[index] - mean) * rstd) * gain[index] + bias[index];
    }
}

/// LayerNorm backward for one row: emits the gain and bias additions and
/// writes `dx` (overwritten). `scratch` holds at least `x.len()` floats.
#[allow(clippy::too_many_arguments)]
pub(super) fn layer_norm_row_backward_v1(
    x: &[f32],
    mean: f32,
    rstd: f32,
    gain: &[f32],
    dy: &[f32],
    mut add_parameter_gradient: impl FnMut(usize, f32, f32),
    dx: &mut [f32],
    scratch: &mut [f32],
) {
    let width = x.len();
    let mut sum_dxhat = 0.0f32;
    let mut sum_dxhat_xhat = 0.0f32;
    for index in 0..width {
        let xhat = (x[index] - mean) * rstd;
        add_parameter_gradient(index, dy[index] * xhat, dy[index]);
        let dxhat = dy[index] * gain[index];
        scratch[index] = dxhat;
        sum_dxhat += dxhat;
        sum_dxhat_xhat += dxhat * xhat;
    }
    let mean_dxhat = sum_dxhat / width as f32;
    let mean_dxhat_xhat = sum_dxhat_xhat / width as f32;
    #[cfg(target_arch = "x86_64")]
    {
        if std::arch::is_x86_feature_detected!("avx2") {
            // SAFETY: AVX2 support was detected at run time just above.
            unsafe {
                layer_norm_dx_avx2_v1(
                    x,
                    mean,
                    rstd,
                    mean_dxhat,
                    mean_dxhat_xhat,
                    scratch,
                    &mut dx[..width],
                );
            }
            return;
        }
    }
    layer_norm_dx_body_v1(
        x,
        mean,
        rstd,
        mean_dxhat,
        mean_dxhat_xhat,
        scratch,
        &mut dx[..width],
    );
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn layer_norm_dx_avx2_v1(
    x: &[f32],
    mean: f32,
    rstd: f32,
    mean_dxhat: f32,
    mean_dxhat_xhat: f32,
    scratch: &[f32],
    dx: &mut [f32],
) {
    layer_norm_dx_body_v1(x, mean, rstd, mean_dxhat, mean_dxhat_xhat, scratch, dx);
}

#[inline(always)]
fn layer_norm_dx_body_v1(
    x: &[f32],
    mean: f32,
    rstd: f32,
    mean_dxhat: f32,
    mean_dxhat_xhat: f32,
    scratch: &[f32],
    dx: &mut [f32],
) {
    let n = dx.len();
    assert!(x.len() >= n && scratch.len() >= n);
    for index in 0..n {
        let xhat = (x[index] - mean) * rstd;
        dx[index] = rstd * ((scratch[index] - mean_dxhat) - xhat * mean_dxhat_xhat);
    }
}

/// In-place softmax of one row: max subtraction, [`exp_v1`], sequential sum.
pub(super) fn softmax_row_v1(row: &mut [f32]) {
    let mut maximum = f32::NEG_INFINITY;
    for value in row.iter() {
        if *value > maximum {
            maximum = *value;
        }
    }
    #[cfg(target_arch = "x86_64")]
    {
        if std::arch::is_x86_feature_detected!("avx2") && maximum.is_finite() {
            // SAFETY: AVX2 support was detected at run time just above.
            unsafe { softmax_row_avx2_v1(row, maximum) };
            return;
        }
    }
    let mut sum = 0.0f32;
    for value in row.iter_mut() {
        *value = exp_v1(*value - maximum);
        sum += *value;
    }
    for value in row.iter_mut() {
        *value /= sum;
    }
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn softmax_row_avx2_v1(row: &mut [f32], maximum: f32) {
    use std::arch::x86_64::*;
    let len = row.len();
    let max_v = _mm256_set1_ps(maximum);
    let mut i = 0;
    while i + 8 <= len {
        let shifted = _mm256_sub_ps(unsafe { _mm256_loadu_ps(row.as_ptr().add(i)) }, max_v);
        if unsafe { avx2_math::all_safe8_v1(shifted) } {
            let e = unsafe { avx2_math::exp8_finite_v1(shifted) };
            unsafe { _mm256_storeu_ps(row.as_mut_ptr().add(i), e) };
        } else {
            for k in 0..8 {
                row[i + k] = exp_v1(row[i + k] - maximum);
            }
        }
        i += 8;
    }
    while i < len {
        row[i] = exp_v1(row[i] - maximum);
        i += 1;
    }
    let mut sum = 0.0f32;
    for value in row.iter() {
        sum += *value;
    }
    let sum_v = _mm256_set1_ps(sum);
    let mut i = 0;
    while i + 8 <= len {
        let v = unsafe { _mm256_loadu_ps(row.as_ptr().add(i)) };
        unsafe { _mm256_storeu_ps(row.as_mut_ptr().add(i), _mm256_div_ps(v, sum_v)) };
        i += 8;
    }
    while i < len {
        row[i] /= sum;
        i += 1;
    }
}
