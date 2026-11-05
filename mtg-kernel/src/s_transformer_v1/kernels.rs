//! Deterministic f32 kernels for the S transformer (`s_transformer_v1`).
//!
//! Every kernel fixes the order of its floating-point operations: each output
//! element is accumulated in ascending order of its reduction index, starting
//! from its declared initial value, with one rounded multiply and one rounded
//! add per term (Rust never contracts these into a fused multiply-add). The
//! AVX2 entry points compile the same bodies with wider vectors: lanes are
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
    assert!(b.len() >= kdim * n, "gemm_v1: b too short");
    assert!(c.len() >= m * n, "gemm_v1: c too short");
    if m > 0 && kdim > 0 {
        assert!(
            (m - 1) * a_rs + (kdim - 1) * a_cs < a.len(),
            "gemm_v1: a too short"
        );
    }
    if let InitV1::Bias(bias) = init {
        assert!(bias.len() >= n, "gemm_v1: bias too short");
    }
    #[cfg(target_arch = "x86_64")]
    {
        if std::arch::is_x86_feature_detected!("avx2") {
            // SAFETY: AVX2 support was detected at run time just above.
            unsafe { gemm_avx2_v1(m, n, kdim, a, a_rs, a_cs, b, init, c) };
            return;
        }
    }
    gemm_body_v1(m, n, kdim, a, a_rs, a_cs, b, init, c);
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
    init: InitV1<'_>,
    c: &mut [f32],
) {
    gemm_body_v1(m, n, kdim, a, a_rs, a_cs, b, init, c);
}

const ROWS_V1: usize = 4;
const COLS_V1: usize = 16;

#[inline(always)]
fn init_value_v1(init: InitV1<'_>, c: &[f32], index: usize, column: usize) -> f32 {
    match init {
        InitV1::Bias(bias) => bias[column],
        InitV1::Zero => 0.0,
        InitV1::Existing => c[index],
    }
}

#[inline(always)]
#[allow(clippy::too_many_arguments)]
fn gemm_body_v1(
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
    let mut i0 = 0;
    while i0 < m {
        let rows = (m - i0).min(ROWS_V1);
        let mut j0 = 0;
        while j0 + COLS_V1 <= n {
            let mut acc = [[0.0f32; COLS_V1]; ROWS_V1];
            for (t, row) in acc.iter_mut().enumerate().take(rows) {
                for (q, slot) in row.iter_mut().enumerate() {
                    *slot = init_value_v1(init, c, (i0 + t) * n + j0 + q, j0 + q);
                }
            }
            if rows == ROWS_V1 {
                for k in 0..kdim {
                    let w: &[f32; COLS_V1] = b[k * n + j0..k * n + j0 + COLS_V1]
                        .try_into()
                        .expect("16 columns");
                    let x0 = a[i0 * a_rs + k * a_cs];
                    let x1 = a[(i0 + 1) * a_rs + k * a_cs];
                    let x2 = a[(i0 + 2) * a_rs + k * a_cs];
                    let x3 = a[(i0 + 3) * a_rs + k * a_cs];
                    for q in 0..COLS_V1 {
                        acc[0][q] += x0 * w[q];
                        acc[1][q] += x1 * w[q];
                        acc[2][q] += x2 * w[q];
                        acc[3][q] += x3 * w[q];
                    }
                }
            } else {
                for k in 0..kdim {
                    let w: &[f32; COLS_V1] = b[k * n + j0..k * n + j0 + COLS_V1]
                        .try_into()
                        .expect("16 columns");
                    for (t, row) in acc.iter_mut().enumerate().take(rows) {
                        let x = a[(i0 + t) * a_rs + k * a_cs];
                        for q in 0..COLS_V1 {
                            row[q] += x * w[q];
                        }
                    }
                }
            }
            for (t, row) in acc.iter().enumerate().take(rows) {
                c[(i0 + t) * n + j0..(i0 + t) * n + j0 + COLS_V1].copy_from_slice(row);
            }
            j0 += COLS_V1;
        }
        for t in 0..rows {
            for j in j0..n {
                let index = (i0 + t) * n + j;
                let mut sum = init_value_v1(init, c, index, j);
                for k in 0..kdim {
                    sum += a[(i0 + t) * a_rs + k * a_cs] * b[k * n + j];
                }
                c[index] = sum;
            }
        }
        i0 += ROWS_V1;
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
    gelu_slice_body_v1(pre, post);
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
    gelu_backward_slice_body_v1(pre, grad);
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
    for (index, out) in y.iter_mut().enumerate() {
        *out = ((x[index] - mean) * rstd) * gain[index] + bias[index];
    }
    (mean, rstd)
}

/// LayerNorm backward for one row: accumulates `dgain`, `dbias` and writes
/// `dx` (overwritten). `scratch` holds at least `x.len()` floats.
#[allow(clippy::too_many_arguments)]
pub(super) fn layer_norm_row_backward_v1(
    x: &[f32],
    mean: f32,
    rstd: f32,
    gain: &[f32],
    dy: &[f32],
    dgain: &mut [f32],
    dbias: &mut [f32],
    dx: &mut [f32],
    scratch: &mut [f32],
) {
    let width = x.len();
    let mut sum_dxhat = 0.0f32;
    let mut sum_dxhat_xhat = 0.0f32;
    for index in 0..width {
        let xhat = (x[index] - mean) * rstd;
        dgain[index] += dy[index] * xhat;
        dbias[index] += dy[index];
        let dxhat = dy[index] * gain[index];
        scratch[index] = dxhat;
        sum_dxhat += dxhat;
        sum_dxhat_xhat += dxhat * xhat;
    }
    let mean_dxhat = sum_dxhat / width as f32;
    let mean_dxhat_xhat = sum_dxhat_xhat / width as f32;
    for index in 0..width {
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
    let mut sum = 0.0f32;
    for value in row.iter_mut() {
        *value = exp_v1(*value - maximum);
        sum += *value;
    }
    for value in row.iter_mut() {
        *value /= sum;
    }
}
