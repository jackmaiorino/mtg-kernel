// Exact-order CUDA forward and learner backward kernels for S (`s_transformer_v1`).
//
// Every floating-point operation uses IEEE-754 round-to-nearest-even intrinsics
// (`__fadd_rn`, `__fsub_rn`, `__fmul_rn`, `__fdiv_rn`, `__fsqrt_rn`) compiled
// with `--fmad=false --ftz=false --prec-div=true --prec-sqrt=true`. Every
// reduction visits terms in the CPU's ascending index order starting from the
// CPU's initial value (`bias[j]`, `+0.0f`, or existing value), with no atomic
// reductions and no platform libm calls. The RL value's `tanh(0.5 * z)` runs
// on the host via `deterministic_math_v1::tanh_f32_v1`.

#define MTG_S_THREADS 256
#define MTG_S_MAX_MODELS 10
#define MTG_S_R_FIELDS 20

#define MTG_S_STATE_KEEP 123
#define MTG_S_OBJ_FEAT 98
#define MTG_S_EDGE_FEAT 41
#define MTG_S_ACT_FEAT 195
#define MTG_S_REF_FEAT 25
#define MTG_S_GROUP_COUNT 20

struct MtgSDims {
    int d_model;
    int layers;
    int heads;
    int ffn;
    int card_dim;
    int card_vocab;
};

struct MtgSModels {
    const float* params[MTG_S_MAX_MODELS];
};

struct MtgSLin {
    long long w;
    long long b;
};

struct MtgSBlockOff {
    long long ln1_g;
    long long ln1_b;
    MtgSLin qkv;
    MtgSLin proj;
    long long ln2_g;
    long long ln2_b;
    MtgSLin ff0;
    MtgSLin ff2;
};

struct MtgSLayout {
    long long ref_bias;
    long long card;
    MtgSLin obj0;
    MtgSLin obj2;
    long long group;
    MtgSLin state0;
    MtgSLin state2;
    MtgSLin act0;
    MtgSLin act2;
    MtgSLin ref0;
    MtgSLin ref2;
    MtgSLin edge;
    long long block_base;
    long long block_stride;
    long long ln_g;
    long long ln_b;
    MtgSLin pol0;
    MtgSLin pol2;
    MtgSLin val0;
    MtgSLin val2;
};

__device__ __forceinline__ MtgSLin mtg_s_take_lin(
    long long* at,
    long long in_dim,
    long long out_dim
) {
    MtgSLin lin;
    lin.w = *at;
    *at += out_dim * in_dim;
    lin.b = *at;
    *at += out_dim;
    return lin;
}

__device__ __forceinline__ MtgSLayout mtg_s_layout(MtgSDims dims) {
    long long d = dims.d_model;
    long long h = dims.heads;
    long long ffn = dims.ffn;
    long long cd = dims.card_dim;
    long long vocab = dims.card_vocab;
    long long at = 0;

    MtgSLayout lay;
    lay.ref_bias = at;
    at += 2 * h;
    lay.card = at;
    at += vocab * cd;
    lay.obj0 = mtg_s_take_lin(&at, MTG_S_OBJ_FEAT + cd, d);
    lay.obj2 = mtg_s_take_lin(&at, d, d);
    lay.group = at;
    at += MTG_S_GROUP_COUNT * d;
    lay.state0 = mtg_s_take_lin(&at, MTG_S_STATE_KEEP, d);
    lay.state2 = mtg_s_take_lin(&at, d, d);
    lay.act0 = mtg_s_take_lin(&at, MTG_S_ACT_FEAT, d);
    lay.act2 = mtg_s_take_lin(&at, d, d);
    lay.ref0 = mtg_s_take_lin(&at, MTG_S_REF_FEAT + cd, d);
    lay.ref2 = mtg_s_take_lin(&at, d, d);
    lay.edge = mtg_s_take_lin(&at, MTG_S_EDGE_FEAT, 2 * h);

    lay.block_base = at;
    lay.block_stride = 2 * d
        + (3 * d * d + 3 * d)
        + (d * d + d)
        + 2 * d
        + (ffn * d + ffn)
        + (d * ffn + d);
    at += (long long)dims.layers * lay.block_stride;

    lay.ln_g = at;
    at += d;
    lay.ln_b = at;
    at += d;
    lay.pol0 = mtg_s_take_lin(&at, 2 * d, d);
    lay.pol2 = mtg_s_take_lin(&at, d, 1);
    lay.val0 = mtg_s_take_lin(&at, d, d);
    lay.val2 = mtg_s_take_lin(&at, d, 1);
    return lay;
}

__device__ __forceinline__ MtgSBlockOff mtg_s_block_off(
    const MtgSLayout& lay,
    MtgSDims dims,
    int layer
) {
    long long d = dims.d_model;
    long long ffn = dims.ffn;
    long long at = lay.block_base + (long long)layer * lay.block_stride;
    MtgSBlockOff b;
    b.ln1_g = at;
    at += d;
    b.ln1_b = at;
    at += d;
    b.qkv = mtg_s_take_lin(&at, d, 3 * d);
    b.proj = mtg_s_take_lin(&at, d, d);
    b.ln2_g = at;
    at += d;
    b.ln2_b = at;
    at += d;
    b.ff0 = mtg_s_take_lin(&at, d, ffn);
    b.ff2 = mtg_s_take_lin(&at, ffn, d);
    return b;
}

// Tape offsets relative to a substep's `scratch_off` (`for_backward != 0`).
struct MtgSTapeOff {
    long long obj_inp;
    long long obj_pre;
    long long obj_act;
    long long state_pre;
    long long state_act;
    long long act_pre;
    long long act_act;
    long long ref_inp;
    long long ref_pre;
    long long ref_act;
    long long ref_out;
    long long edge_out;
    long long bias;
    long long layers_base;
    long long layer_stride;
    long long x_out;
    long long fin;
    long long fin_stats;
    long long pol_inp;
    long long pol_pre;
    long long pol_act;
    long long val_pre;
    long long val_act;
};

struct MtgSLayerTapeOff {
    long long x_in;
    long long ln1;
    long long ln1_stats;
    long long qkv;
    long long probs;
    long long attn;
    long long x_mid;
    long long ln2;
    long long ln2_stats;
    long long ff_pre;
    long long ff_act;
};

__device__ __forceinline__ MtgSTapeOff mtg_s_carve_tape(
    MtgSDims dims,
    int n,
    int e,
    int a,
    int r
) {
    long long d = dims.d_model;
    long long h = dims.heads;
    long long ffn = dims.ffn;
    long long cd = dims.card_dim;
    long long l = 1 + (long long)n + (long long)a;
    long long at = 0;

    MtgSTapeOff t;
    t.obj_inp = at;
    at += (long long)n * (MTG_S_OBJ_FEAT + cd);
    t.obj_pre = at;
    at += (long long)n * d;
    t.obj_act = at;
    at += (long long)n * d;
    t.state_pre = at;
    at += d;
    t.state_act = at;
    at += d;
    t.act_pre = at;
    at += (long long)a * d;
    t.act_act = at;
    at += (long long)a * d;
    t.ref_inp = at;
    at += (long long)r * (MTG_S_REF_FEAT + cd);
    t.ref_pre = at;
    at += (long long)r * d;
    t.ref_act = at;
    at += (long long)r * d;
    t.ref_out = at;
    at += (long long)r * d;
    t.edge_out = at;
    at += (long long)e * 2 * h;
    t.bias = at;
    at += h * l * l;

    t.layers_base = at;
    t.layer_stride = 8 * l * d + 4 * l + h * l * l + 2 * l * ffn;
    at += (long long)dims.layers * t.layer_stride;

    t.x_out = at;
    at += l * d;
    t.fin = at;
    at += (1 + (long long)a) * d;
    t.fin_stats = at;
    at += 2 * (1 + (long long)a);
    t.pol_inp = at;
    at += (long long)a * 2 * d;
    t.pol_pre = at;
    at += (long long)a * d;
    t.pol_act = at;
    at += (long long)a * d;
    t.val_pre = at;
    at += d;
    t.val_act = at;
    return t;
}

__device__ __forceinline__ MtgSLayerTapeOff mtg_s_layer_tape_off(
    const MtgSTapeOff& t,
    MtgSDims dims,
    int l_int,
    int layer
) {
    long long d = dims.d_model;
    long long h = dims.heads;
    long long ffn = dims.ffn;
    long long l = l_int;
    long long at = t.layers_base + (long long)layer * t.layer_stride;
    MtgSLayerTapeOff lt;
    lt.x_in = at;
    at += l * d;
    lt.ln1 = at;
    at += l * d;
    lt.ln1_stats = at;
    at += 2 * l;
    lt.qkv = at;
    at += l * 3 * d;
    lt.probs = at;
    at += h * l * l;
    lt.attn = at;
    at += l * d;
    lt.x_mid = at;
    at += l * d;
    lt.ln2 = at;
    at += l * d;
    lt.ln2_stats = at;
    at += 2 * l;
    lt.ff_pre = at;
    at += l * ffn;
    lt.ff_act = at;
    return lt;
}

// Gradient scratch offsets relative to `grad_offsets[s]`.
struct MtgSGradOff {
    long long dz;
    long long dval_pre;
    long long dpol_pre;
    long long ln_dg;
    long long ln_db;
    long long blocks_base;
    long long block_stride;
    long long dx_final;
    long long dstate_pre;
    long long dobj_pre;
    long long dobj_inp;
    long long dact_pre;
    long long dref_out;
    long long dref_pre;
    long long dref_inp;
    long long dbias;
    long long deb;
    long long ws_dx;
    long long ws_dx2;
    long long ws_dattn;
    long long ws_dscore;
    long long ws_dfin;
    long long ws_dpol_inp;
};

struct MtgSBlockGradOff {
    long long dff2_dy;
    long long dff0_dy;
    long long ln2_dg;
    long long ln2_db;
    long long dproj_dy;
    long long dqkv;
    long long ln1_dg;
    long long ln1_db;
};

__device__ __forceinline__ MtgSGradOff mtg_s_carve_grad(
    MtgSDims dims,
    int n,
    int e,
    int a,
    int r
) {
    long long d = dims.d_model;
    long long h = dims.heads;
    long long ffn = dims.ffn;
    long long cd = dims.card_dim;
    long long l = 1 + (long long)n + (long long)a;
    long long at = 0;

    MtgSGradOff g;
    g.dz = at;
    at += 1;
    g.dval_pre = at;
    at += d;
    g.dpol_pre = at;
    at += (long long)a * d;
    g.ln_dg = at;
    at += (1 + (long long)a) * d;
    g.ln_db = at;
    at += (1 + (long long)a) * d;

    g.blocks_base = at;
    g.block_stride = 9 * l * d + l * ffn;
    at += (long long)dims.layers * g.block_stride;

    g.dx_final = at;
    at += l * d;
    g.dstate_pre = at;
    at += d;
    g.dobj_pre = at;
    at += (long long)n * d;
    g.dobj_inp = at;
    at += (long long)n * (MTG_S_OBJ_FEAT + cd);
    g.dact_pre = at;
    at += (long long)a * d;
    g.dref_out = at;
    at += (long long)r * d;
    g.dref_pre = at;
    at += (long long)r * d;
    g.dref_inp = at;
    at += (long long)r * (MTG_S_REF_FEAT + cd);
    g.dbias = at;
    at += h * l * l;
    g.deb = at;
    at += (long long)e * 2 * h;

    g.ws_dx = at;
    at += l * d;
    g.ws_dx2 = at;
    at += l * d;
    g.ws_dattn = at;
    at += l * d;
    g.ws_dscore = at;
    at += h * l * l;
    g.ws_dfin = at;
    at += (1 + (long long)a) * d;
    g.ws_dpol_inp = at;
    return g;
}

__device__ __forceinline__ MtgSBlockGradOff mtg_s_block_grad_off(
    const MtgSGradOff& g,
    MtgSDims dims,
    int l_int,
    int layer
) {
    long long d = dims.d_model;
    long long ffn = dims.ffn;
    long long l = l_int;
    long long at = g.blocks_base + (long long)layer * g.block_stride;
    MtgSBlockGradOff b;
    b.dff2_dy = at;
    at += l * d;
    b.dff0_dy = at;
    at += l * ffn;
    b.ln2_dg = at;
    at += l * d;
    b.ln2_db = at;
    at += l * d;
    b.dproj_dy = at;
    at += l * d;
    b.dqkv = at;
    at += l * 3 * d;
    b.ln1_dg = at;
    at += l * d;
    b.ln1_db = at;
    return b;
}

// Portable IEEE-754 transcendentals (`s_transformer_v1/kernels.rs`).
__device__ __forceinline__ float mtg_s_clamp_exp(float x) {
    if (isnan(x)) return x;
    if (x < -87.0f) return -87.0f;
    if (x > 88.0f) return 88.0f;
    return x;
}

__device__ __forceinline__ float mtg_s_exp(float x) {
    const float LOG2E = __uint_as_float(0x3fb8aa3bu);
    const float LN2_HI = __uint_as_float(0x3f317200u);
    const float LN2_LO = __uint_as_float(0x35bfbe8eu);
    const float ROUND_MAGIC = 12582912.0f;
    const float C7 = __uint_as_float(0x39500d01u); // 1.0f / 5040.0f
    const float C6 = __uint_as_float(0x3ab60b61u); // 1.0f / 720.0f
    const float C5 = __uint_as_float(0x3c088889u); // 1.0f / 120.0f
    const float C4 = __uint_as_float(0x3d2aaaabu); // 1.0f / 24.0f
    const float C3 = __uint_as_float(0x3e2aaaabu); // 1.0f / 6.0f

    x = mtg_s_clamp_exp(x);
    float kf = __fsub_rn(__fadd_rn(__fmul_rn(x, LOG2E), ROUND_MAGIC), ROUND_MAGIC);
    float r = __fsub_rn(__fsub_rn(x, __fmul_rn(kf, LN2_HI)), __fmul_rn(kf, LN2_LO));
    float p = __fadd_rn(C6, __fmul_rn(r, C7));
    p = __fadd_rn(C5, __fmul_rn(r, p));
    p = __fadd_rn(C4, __fmul_rn(r, p));
    p = __fadd_rn(C3, __fmul_rn(r, p));
    p = __fadd_rn(0.5f, __fmul_rn(r, p));
    p = __fadd_rn(1.0f, __fmul_rn(r, p));
    p = __fadd_rn(1.0f, __fmul_rn(r, p));
    int k = isnan(kf) ? 0 : (int)kf;
    float scale = __uint_as_float(((unsigned int)(k + 127)) << 23);
    return __fmul_rn(p, scale);
}

__device__ __forceinline__ float mtg_s_normal_cdf(float x) {
    const float FRAC_1_SQRT_2 = __uint_as_float(0x3f3504f3u);
    const float C0 = __uint_as_float(0xbfa1fc4eu);
    const float C1 = __uint_as_float(0x3f8000c7u);
    const float C2 = __uint_as_float(0x3ebf88fbu);
    const float C3 = __uint_as_float(0x3dc636c9u);
    const float C4 = __uint_as_float(0xbe3ec24cu);
    const float C5 = __uint_as_float(0x3e8ec7ccu);
    const float C6 = __uint_as_float(0xbf914e5du);
    const float C7 = __uint_as_float(0x3fbe87b0u);
    const float C8 = __uint_as_float(0xbf527892u);
    const float C9 = __uint_as_float(0x3e2ef945u);

    float z = fabsf(__fmul_rn(x, FRAC_1_SQRT_2));
    float t = __fdiv_rn(1.0f, __fadd_rn(1.0f, __fmul_rn(0.5f, z)));
    float h = __fadd_rn(C8, __fmul_rn(t, C9));
    h = __fadd_rn(C7, __fmul_rn(t, h));
    h = __fadd_rn(C6, __fmul_rn(t, h));
    h = __fadd_rn(C5, __fmul_rn(t, h));
    h = __fadd_rn(C4, __fmul_rn(t, h));
    h = __fadd_rn(C3, __fmul_rn(t, h));
    h = __fadd_rn(C2, __fmul_rn(t, h));
    h = __fadd_rn(C1, __fmul_rn(t, h));
    float neg_z = __uint_as_float(__float_as_uint(z) ^ 0x80000000u);
    float poly = __fadd_rn(__fadd_rn(__fmul_rn(neg_z, z), C0), __fmul_rn(t, h));
    float half_erfc = __fmul_rn(0.5f, __fmul_rn(t, mtg_s_exp(poly)));
    return (x >= 0.0f) ? __fsub_rn(1.0f, half_erfc) : half_erfc;
}

__device__ __forceinline__ float mtg_s_gelu(float x) {
    return __fmul_rn(x, mtg_s_normal_cdf(x));
}

__device__ __forceinline__ float mtg_s_gelu_grad(float x) {
    const float INV_SQRT_2PI = __uint_as_float(0x3ecc422au);
    float arg = __fmul_rn(__fmul_rn(-0.5f, x), x);
    float pdf = __fmul_rn(mtg_s_exp(arg), INV_SQRT_2PI);
    return __fadd_rn(mtg_s_normal_cdf(x), __fmul_rn(x, pdf));
}

// Forward linear helper: `y[r, o] = bias[o] + sum_{k=0..in_dim-1} x[r, k] * wt[k, o]`.
__device__ void mtg_s_linear_fwd_strided(
    int tid,
    int rows,
    int in_dim,
    int out_dim,
    const float* x,
    int x_rs,
    const float* wt,
    int wt_rs,
    const float* bias,
    float* y,
    int y_rs
) {
    int total = rows * out_dim;
    for (int idx = tid; idx < total; idx += MTG_S_THREADS) {
        int r = idx / out_dim;
        int o = idx % out_dim;
        float acc = (bias != nullptr) ? bias[o] : 0.0f;
        const float* x_row = x + (long long)r * x_rs;
        for (int k = 0; k < in_dim; ++k) {
            acc = __fadd_rn(acc, __fmul_rn(x_row[k], wt[(long long)k * wt_rs + o]));
        }
        y[(long long)r * y_rs + o] = acc;
    }
    __syncthreads();
}

// Forward linear over live rows `[0, 1+n .. 1+n+a)` when `live_only != 0`.
__device__ void mtg_s_linear_fwd_rows(
    int tid,
    int m,
    int n_obj,
    int live_only,
    int in_dim,
    int out_dim,
    const float* x,
    const float* wt,
    const float* bias,
    float* y
) {
    int total = m * out_dim;
    for (int idx = tid; idx < total; idx += MTG_S_THREADS) {
        int r = idx / out_dim;
        int o = idx % out_dim;
        int t = (live_only && r > 0) ? (n_obj + r) : r;
        float acc = (bias != nullptr) ? bias[o] : 0.0f;
        const float* x_row = x + (long long)t * in_dim;
        for (int k = 0; k < in_dim; ++k) {
            acc = __fadd_rn(acc, __fmul_rn(x_row[k], wt[(long long)k * out_dim + o]));
        }
        y[(long long)t * out_dim + o] = acc;
    }
    __syncthreads();
}

__device__ void mtg_s_ln_fwd_rows(
    int tid,
    int m,
    int n_obj,
    int live_only,
    int d,
    const float* x,
    const float* gain,
    const float* bias,
    float* y,
    int y_compact,
    float* stats,
    int stats_compact
) {
    const float EPS = __uint_as_float(0x3727c5acu); // 1.0e-5f
    float width_f = (float)d;
    for (int r = tid; r < m; r += MTG_S_THREADS) {
        int t = (live_only && r > 0) ? (n_obj + r) : r;
        const float* xr = x + (long long)t * d;
        float sum = 0.0f;
        for (int c = 0; c < d; ++c) {
            sum = __fadd_rn(sum, xr[c]);
        }
        float mean = __fdiv_rn(sum, width_f);
        float squares = 0.0f;
        for (int c = 0; c < d; ++c) {
            float centered = __fsub_rn(xr[c], mean);
            squares = __fadd_rn(squares, __fmul_rn(centered, centered));
        }
        float var = __fdiv_rn(squares, width_f);
        float rstd = __fdiv_rn(1.0f, __fsqrt_rn(__fadd_rn(var, EPS)));
        int sr = stats_compact ? r : t;
        stats[2 * sr] = mean;
        stats[2 * sr + 1] = rstd;
    }
    __syncthreads();

    int total = m * d;
    for (int idx = tid; idx < total; idx += MTG_S_THREADS) {
        int r = idx / d;
        int c = idx % d;
        int t = (live_only && r > 0) ? (n_obj + r) : r;
        int sr = stats_compact ? r : t;
        int yr = y_compact ? r : t;
        float mean = stats[2 * sr];
        float rstd = stats[2 * sr + 1];
        float norm = __fmul_rn(__fsub_rn(x[(long long)t * d + c], mean), rstd);
        y[(long long)yr * d + c] = __fadd_rn(__fmul_rn(norm, gain[c]), bias[c]);
    }
    __syncthreads();
}

// Backward linear `dx` helper: `dx[r, col] = (add ? dx : 0) + sum_{o=0..out_dim-1} dy[r, o] * w[o, col]`.
__device__ void mtg_s_linear_dx(
    int tid,
    int rows,
    int in_dim,
    int out_dim,
    const float* dy,
    int dy_rs,
    const float* w,
    float* dx,
    int dx_rs,
    int add_to_dx
) {
    int total = rows * in_dim;
    for (int idx = tid; idx < total; idx += MTG_S_THREADS) {
        int r = idx / in_dim;
        int col = idx % in_dim;
        float acc = add_to_dx ? dx[(long long)r * dx_rs + col] : 0.0f;
        const float* dy_row = dy + (long long)r * dy_rs;
        for (int o = 0; o < out_dim; ++o) {
            acc = __fadd_rn(acc, __fmul_rn(dy_row[o], w[(long long)o * in_dim + col]));
        }
        dx[(long long)r * dx_rs + col] = acc;
    }
    __syncthreads();
}

__device__ __forceinline__ void mtg_s_ln_bwd_one_row(
    int d,
    const float* x,
    float mean,
    float rstd,
    const float* gain,
    const float* dy,
    float* dg_out,
    float* db_out,
    float* dx_out,
    int add_to_dx
) {
    float sum_dxhat = 0.0f;
    float sum_dxhat_xhat = 0.0f;
    for (int c = 0; c < d; ++c) {
        float xhat = __fmul_rn(__fsub_rn(x[c], mean), rstd);
        dg_out[c] = __fmul_rn(dy[c], xhat);
        db_out[c] = dy[c];
        float dxhat = __fmul_rn(dy[c], gain[c]);
        sum_dxhat = __fadd_rn(sum_dxhat, dxhat);
        sum_dxhat_xhat = __fadd_rn(sum_dxhat_xhat, __fmul_rn(dxhat, xhat));
    }
    float width_f = (float)d;
    float mean_dxhat = __fdiv_rn(sum_dxhat, width_f);
    float mean_dxhat_xhat = __fdiv_rn(sum_dxhat_xhat, width_f);
    for (int c = 0; c < d; ++c) {
        float xhat = __fmul_rn(__fsub_rn(x[c], mean), rstd);
        float dxhat = __fmul_rn(dy[c], gain[c]);
        float term = __fmul_rn(
            rstd,
            __fsub_rn(__fsub_rn(dxhat, mean_dxhat), __fmul_rn(xhat, mean_dxhat_xhat))
        );
        if (add_to_dx) {
            dx_out[c] = __fadd_rn(dx_out[c], term);
        } else {
            dx_out[c] = term;
        }
    }
}

extern "C" __global__ void mtg_s_forward(
    const long long* __restrict__ reqs,
    const float* __restrict__ floats,
    const int* __restrict__ ints,
    MtgSModels models,
    MtgSDims dims,
    int for_backward,
    float* __restrict__ scratch,
    float* __restrict__ out,
    int* __restrict__ live_flags
) {
    int tid = threadIdx.x;
    const long long* req = reqs + (long long)blockIdx.x * MTG_S_R_FIELDS;
    int slot = (int)req[0];
    int n = (int)req[1];
    int e = (int)req[2];
    int a = (int)req[3];
    int r = (int)req[4];

    const float* state_in = floats + req[5];
    const float* obj_feat = floats + req[6];
    const float* edge_feat = floats + req[7];
    const float* act_feat = floats + req[8];
    const float* ref_feat = floats + req[9];

    const int* obj_cards = ints + req[10];
    const int* obj_groups = ints + req[11];
    const int* edge_src = ints + req[12];
    const int* edge_tgt = ints + req[13];
    const int* ref_cards = ints + req[14];
    const int* ref_acts = ints + req[15];
    const int* ref_nodes = ints + req[16];

    float* base = scratch + req[17];
    float* out_logits = out + req[18];
    float* out_value_logit = out + req[19];

    const float* wt = models.params[slot];
    MtgSLayout lay = mtg_s_layout(dims);

    int d = dims.d_model;
    int h = dims.heads;
    int dh = d / h;
    int cd = dims.card_dim;
    int ffn = dims.ffn;
    int l = 1 + n + a;

    // Compact scratch pointers when `for_backward == 0`, or full tape pointers.
    MtgSTapeOff tape_off;
    float* buf_x0 = nullptr;
    float* buf_x1 = nullptr;
    float* inf_bias = nullptr;
    float* inf_ln = nullptr;
    float* inf_stats = nullptr;
    float* inf_qkv = nullptr;
    float* inf_probs = nullptr;
    float* inf_attn = nullptr;
    float* inf_x_mid = nullptr;
    float* inf_work = nullptr;
    float* inf_work2 = nullptr;

    if (for_backward) {
        tape_off = mtg_s_carve_tape(dims, n, e, a, r);
        MtgSLayerTapeOff l0 = mtg_s_layer_tape_off(tape_off, dims, l, 0);
        buf_x0 = base + l0.x_in;
        inf_bias = base + tape_off.bias;
    } else {
        long long at = 0;
        buf_x0 = base + at;
        at += (long long)l * d;
        buf_x1 = base + at;
        at += (long long)l * d;
        inf_bias = base + at;
        at += (long long)h * l * l;
        inf_ln = base + at;
        at += (long long)l * d;
        inf_stats = base + at;
        at += 2LL * l;
        inf_qkv = base + at;
        at += (long long)l * 3 * d;
        inf_probs = base + at;
        at += (long long)h * l * l;
        inf_attn = base + at;
        at += (long long)l * d;
        inf_x_mid = base + at;
        at += (long long)l * d;

        long long w1 = (long long)n * (MTG_S_OBJ_FEAT + cd);
        long long w1_r = (long long)r * (MTG_S_REF_FEAT + cd);
        long long w1_rd = (long long)r * d;
        long long w1_ff = (long long)l * ffn;
        long long w1_pol = (long long)a * 2 * d;
        if (w1_r > w1) w1 = w1_r;
        if (w1_rd > w1) w1 = w1_rd;
        if (w1_ff > w1) w1 = w1_ff;
        if (w1_pol > w1) w1 = w1_pol;
        inf_work = base + at;
        at += w1;

        long long max_nar = n;
        if (a > max_nar) max_nar = a;
        if (r > max_nar) max_nar = r;
        if (1 > max_nar) max_nar = 1;
        long long w2 = max_nar * d;
        long long w2_e = (long long)e * 2 * h;
        if (w2_e > w2) w2 = w2_e;
        inf_work2 = base + at;
    }

    // 1. Object tokens.
    int obj_in = MTG_S_OBJ_FEAT + cd;
    float* obj_inp = for_backward ? (base + tape_off.obj_inp) : inf_work;
    float* obj_pre = for_backward ? (base + tape_off.obj_pre) : inf_work2;
    float* obj_act = for_backward ? (base + tape_off.obj_act) : inf_work2;
    for (int idx = tid; idx < n * obj_in; idx += MTG_S_THREADS) {
        int i = idx / obj_in;
        int col = idx % obj_in;
        if (col < MTG_S_OBJ_FEAT) {
            obj_inp[idx] = obj_feat[i * MTG_S_OBJ_FEAT + col];
        } else {
            int card = obj_cards[i];
            obj_inp[idx] = wt[lay.card + (long long)card * cd + (col - MTG_S_OBJ_FEAT)];
        }
    }
    __syncthreads();
    mtg_s_linear_fwd_strided(
        tid, n, obj_in, d, obj_inp, obj_in, wt + lay.obj0.w, d, wt + lay.obj0.b, obj_pre, d
    );
    for (int idx = tid; idx < n * d; idx += MTG_S_THREADS) {
        obj_act[idx] = mtg_s_gelu(obj_pre[idx]);
    }
    __syncthreads();
    mtg_s_linear_fwd_strided(
        tid, n, d, d, obj_act, d, wt + lay.obj2.w, d, wt + lay.obj2.b, buf_x0 + d, d
    );
    for (int idx = tid; idx < n * d; idx += MTG_S_THREADS) {
        int i = idx / d;
        int c = idx % d;
        int g = obj_groups[i];
        float slot_val = buf_x0[(1 + i) * d + c];
        buf_x0[(1 + i) * d + c] = __fadd_rn(slot_val, wt[lay.group + (long long)g * d + c]);
    }
    __syncthreads();

    // 2. State token.
    float* state_pre = for_backward ? (base + tape_off.state_pre) : inf_work2;
    float* state_act = for_backward ? (base + tape_off.state_act) : inf_work2;
    mtg_s_linear_fwd_strided(
        tid,
        1,
        MTG_S_STATE_KEEP,
        d,
        state_in,
        MTG_S_STATE_KEEP,
        wt + lay.state0.w,
        d,
        wt + lay.state0.b,
        state_pre,
        d
    );
    for (int c = tid; c < d; c += MTG_S_THREADS) {
        state_act[c] = mtg_s_gelu(state_pre[c]);
    }
    __syncthreads();
    mtg_s_linear_fwd_strided(
        tid, 1, d, d, state_act, d, wt + lay.state2.w, d, wt + lay.state2.b, buf_x0, d
    );

    // 3. Action tokens and reference encodings.
    float* act_pre = for_backward ? (base + tape_off.act_pre) : inf_work2;
    float* act_act = for_backward ? (base + tape_off.act_act) : inf_work2;
    mtg_s_linear_fwd_strided(
        tid,
        a,
        MTG_S_ACT_FEAT,
        d,
        act_feat,
        MTG_S_ACT_FEAT,
        wt + lay.act0.w,
        d,
        wt + lay.act0.b,
        act_pre,
        d
    );
    for (int idx = tid; idx < a * d; idx += MTG_S_THREADS) {
        act_act[idx] = mtg_s_gelu(act_pre[idx]);
    }
    __syncthreads();
    mtg_s_linear_fwd_strided(
        tid,
        a,
        d,
        d,
        act_act,
        d,
        wt + lay.act2.w,
        d,
        wt + lay.act2.b,
        buf_x0 + (long long)(1 + n) * d,
        d
    );

    if (r > 0) {
        int ref_in = MTG_S_REF_FEAT + cd;
        float* ref_inp = for_backward ? (base + tape_off.ref_inp) : inf_work;
        float* ref_pre = for_backward ? (base + tape_off.ref_pre) : inf_work2;
        float* ref_act = for_backward ? (base + tape_off.ref_act) : inf_work2;
        float* ref_out = for_backward ? (base + tape_off.ref_out) : inf_work;
        for (int idx = tid; idx < r * ref_in; idx += MTG_S_THREADS) {
            int k = idx / ref_in;
            int col = idx % ref_in;
            if (col < MTG_S_REF_FEAT) {
                ref_inp[idx] = ref_feat[k * MTG_S_REF_FEAT + col];
            } else {
                int card = ref_cards[k];
                ref_inp[idx] = wt[lay.card + (long long)card * cd + (col - MTG_S_REF_FEAT)];
            }
        }
        __syncthreads();
        mtg_s_linear_fwd_strided(
            tid, r, ref_in, d, ref_inp, ref_in, wt + lay.ref0.w, d, wt + lay.ref0.b, ref_pre, d
        );
        for (int idx = tid; idx < r * d; idx += MTG_S_THREADS) {
            ref_act[idx] = mtg_s_gelu(ref_pre[idx]);
        }
        __syncthreads();
        mtg_s_linear_fwd_strided(
            tid, r, d, d, ref_act, d, wt + lay.ref2.w, d, wt + lay.ref2.b, ref_out, d
        );
        for (int idx = tid; idx < a * d; idx += MTG_S_THREADS) {
            int act_idx = idx / d;
            int c = idx % d;
            float acc = buf_x0[(long long)(1 + n + act_idx) * d + c];
            for (int k = 0; k < r; ++k) {
                if (ref_acts[k] == act_idx) {
                    acc = __fadd_rn(acc, ref_out[(long long)k * d + c]);
                }
            }
            buf_x0[(long long)(1 + n + act_idx) * d + c] = acc;
        }
        __syncthreads();
    }

    // 4. Attention bias `[h, l, l]`.
    long long bias_len = (long long)h * l * l;
    for (long long idx = tid; idx < bias_len; idx += MTG_S_THREADS) {
        inf_bias[idx] = 0.0f;
    }
    __syncthreads();

    if (e > 0) {
        float* edge_out = for_backward ? (base + tape_off.edge_out) : inf_work2;
        mtg_s_linear_fwd_strided(
            tid,
            e,
            MTG_S_EDGE_FEAT,
            2 * h,
            edge_feat,
            MTG_S_EDGE_FEAT,
            wt + lay.edge.w,
            2 * h,
            wt + lay.edge.b,
            edge_out,
            2 * h
        );
        for (int head = tid; head < h; head += MTG_S_THREADS) {
            float* hb = inf_bias + (long long)head * l * l;
            for (int k = 0; k < e; ++k) {
                int s = 1 + edge_src[k];
                int t = 1 + edge_tgt[k];
                long long off = (long long)s * l + t;
                hb[off] = __fadd_rn(hb[off], edge_out[(long long)k * 2 * h + head]);
            }
            for (int k = 0; k < e; ++k) {
                int s = 1 + edge_src[k];
                int t = 1 + edge_tgt[k];
                long long off = (long long)t * l + s;
                hb[off] = __fadd_rn(hb[off], edge_out[(long long)k * 2 * h + h + head]);
            }
        }
        __syncthreads();
    }
    if (r > 0) {
        for (int head = tid; head < h; head += MTG_S_THREADS) {
            float* hb = inf_bias + (long long)head * l * l;
            float rb0 = wt[lay.ref_bias + head];
            float rb1 = wt[lay.ref_bias + h + head];
            for (int k = 0; k < r; ++k) {
                int ai = 1 + n + ref_acts[k];
                int oi = 1 + ref_nodes[k];
                long long off = (long long)ai * l + oi;
                hb[off] = __fadd_rn(hb[off], rb0);
            }
            for (int k = 0; k < r; ++k) {
                int ai = 1 + n + ref_acts[k];
                int oi = 1 + ref_nodes[k];
                long long off = (long long)oi * l + ai;
                hb[off] = __fadd_rn(hb[off], rb1);
            }
        }
        __syncthreads();
    }

    // 5. Transformer blocks.
    float scale = __fdiv_rn(1.0f, __fsqrt_rn((float)dh));
    for (int layer = 0; layer < dims.layers; ++layer) {
        MtgSBlockOff blk = mtg_s_block_off(lay, dims, layer);
        int is_last = (layer + 1 == dims.layers) ? 1 : 0;

        float* x_in = nullptr;
        float* ln1 = nullptr;
        float* ln1_stats = nullptr;
        float* qkv = nullptr;
        float* probs = nullptr;
        float* attn = nullptr;
        float* x_mid = nullptr;
        float* ln2 = nullptr;
        float* ln2_stats = nullptr;
        float* ff_pre = nullptr;
        float* ff_act = nullptr;
        float* x_next = nullptr;

        if (for_backward) {
            MtgSLayerTapeOff lt = mtg_s_layer_tape_off(tape_off, dims, l, layer);
            x_in = base + lt.x_in;
            ln1 = base + lt.ln1;
            ln1_stats = base + lt.ln1_stats;
            qkv = base + lt.qkv;
            probs = base + lt.probs;
            attn = base + lt.attn;
            x_mid = base + lt.x_mid;
            ln2 = base + lt.ln2;
            ln2_stats = base + lt.ln2_stats;
            ff_pre = base + lt.ff_pre;
            ff_act = base + lt.ff_act;
            if (is_last) {
                x_next = base + tape_off.x_out;
                // Zero the skipped object rows' tape buffers and `x_out` (`size_final_tape_v1`).
                long long zero_span = ((long long)h * l * l)
                    + 3LL * l * d
                    + 2LL * l
                    + 2LL * l * ffn;
                for (long long z = tid; z < zero_span; z += MTG_S_THREADS) {
                    probs[z] = 0.0f;
                }
                for (int z = tid; z < l * d; z += MTG_S_THREADS) {
                    x_next[z] = 0.0f;
                }
                __syncthreads();
            } else {
                MtgSLayerTapeOff lt_next = mtg_s_layer_tape_off(tape_off, dims, l, layer + 1);
                x_next = base + lt_next.x_in;
            }
        } else {
            x_in = (layer % 2 == 0) ? buf_x0 : buf_x1;
            x_next = (layer % 2 == 0) ? buf_x1 : buf_x0;
            ln1 = inf_ln;
            ln1_stats = inf_stats;
            qkv = inf_qkv;
            probs = inf_probs;
            attn = inf_attn;
            x_mid = inf_x_mid;
            ln2 = inf_ln;
            ln2_stats = inf_stats;
            ff_pre = inf_work;
            ff_act = inf_work;
        }

        // LN1 over all `l` rows.
        mtg_s_ln_fwd_rows(
            tid, l, n, 0, d, x_in, wt + blk.ln1_g, wt + blk.ln1_b, ln1, 0, ln1_stats, 0
        );

        int m = is_last ? (1 + a) : l;
        if (is_last && !for_backward && n > 0) {
            mtg_s_linear_fwd_rows(
                tid, m, n, 1, d, 3 * d, ln1, wt + blk.qkv.w, wt + blk.qkv.b, qkv
            );
            mtg_s_linear_fwd_strided(
                tid,
                n,
                d,
                2 * d,
                ln1 + d,
                d,
                wt + blk.qkv.w + d,
                3 * d,
                wt + blk.qkv.b + d,
                qkv + 3 * d + d,
                3 * d
            );
        } else {
            mtg_s_linear_fwd_strided(
                tid, l, d, 3 * d, ln1, d, wt + blk.qkv.w, 3 * d, wt + blk.qkv.b, qkv, 3 * d
            );
            if (is_last && for_backward) {
                int local_finite = 1;
                for (int idx = tid; idx < l * 3 * d; idx += MTG_S_THREADS) {
                    if (!isfinite(qkv[idx])) {
                        local_finite = 0;
                    }
                }
                int all_finite = __syncthreads_and(local_finite);
                if (tid == 0) {
                    live_flags[blockIdx.x] = all_finite ? 1 : 0;
                }
                __syncthreads();
            }
        }

        // Multi-head attention: scores + bias.
        int score_items = h * m * l;
        for (int idx = tid; idx < score_items; idx += MTG_S_THREADS) {
            int j = idx % l;
            int tmp = idx / l;
            int r_idx = tmp % m;
            int head = tmp / m;
            int i = (is_last && r_idx > 0) ? (n + r_idx) : r_idx;
            const float* q_row = qkv + (long long)i * 3 * d + head * dh;
            const float* k_row = qkv + (long long)j * 3 * d + d + head * dh;
            float sum = 0.0f;
            for (int c = 0; c < dh; ++c) {
                sum = __fadd_rn(sum, __fmul_rn(q_row[c], k_row[c]));
            }
            long long p_off = ((long long)head * l + i) * l + j;
            probs[p_off] = __fadd_rn(__fmul_rn(sum, scale), inf_bias[p_off]);
        }
        __syncthreads();

        // Softmax per `(head, r_idx)` row.
        int prob_rows = h * m;
        for (int row_id = tid; row_id < prob_rows; row_id += MTG_S_THREADS) {
            int head = row_id / m;
            int r_idx = row_id % m;
            int i = (is_last && r_idx > 0) ? (n + r_idx) : r_idx;
            float* row = probs + ((long long)head * l + i) * l;
            float maximum = __uint_as_float(0xff800000u); // -inf
            for (int j = 0; j < l; ++j) {
                if (row[j] > maximum) {
                    maximum = row[j];
                }
            }
            float sum = 0.0f;
            for (int j = 0; j < l; ++j) {
                float e_val = mtg_s_exp(__fsub_rn(row[j], maximum));
                row[j] = e_val;
                sum = __fadd_rn(sum, e_val);
            }
            for (int j = 0; j < l; ++j) {
                row[j] = __fdiv_rn(row[j], sum);
            }
        }
        __syncthreads();

        // Context `attn[i, head * dh + c] = sum_{j=0..l-1} probs[head, i, j] * V[j, head * dh + c]`.
        int attn_items = m * d;
        for (int idx = tid; idx < attn_items; idx += MTG_S_THREADS) {
            int r_idx = idx / d;
            int col = idx % d;
            int head = col / dh;
            int i = (is_last && r_idx > 0) ? (n + r_idx) : r_idx;
            const float* p_row = probs + ((long long)head * l + i) * l;
            float sum = 0.0f;
            for (int j = 0; j < l; ++j) {
                sum = __fadd_rn(sum, __fmul_rn(p_row[j], qkv[(long long)j * 3 * d + 2 * d + col]));
            }
            attn[(long long)i * d + col] = sum;
        }
        __syncthreads();

        // Output projection + residual `x_mid = proj(attn) + x_in`.
        mtg_s_linear_fwd_rows(
            tid, m, n, is_last, d, d, attn, wt + blk.proj.w, wt + blk.proj.b, x_mid
        );
        for (int idx = tid; idx < m * d; idx += MTG_S_THREADS) {
            int r_idx = idx / d;
            int c = idx % d;
            int t = (is_last && r_idx > 0) ? (n + r_idx) : r_idx;
            long long off = (long long)t * d + c;
            x_mid[off] = __fadd_rn(x_mid[off], x_in[off]);
        }
        __syncthreads();

        // LN2 + FFN + residual.
        mtg_s_ln_fwd_rows(
            tid, m, n, is_last, d, x_mid, wt + blk.ln2_g, wt + blk.ln2_b, ln2, 0, ln2_stats, 0
        );
        mtg_s_linear_fwd_rows(
            tid, m, n, is_last, d, ffn, ln2, wt + blk.ff0.w, wt + blk.ff0.b, ff_pre
        );
        for (int idx = tid; idx < m * ffn; idx += MTG_S_THREADS) {
            int r_idx = idx / ffn;
            int c = idx % ffn;
            int t = (is_last && r_idx > 0) ? (n + r_idx) : r_idx;
            long long off = (long long)t * ffn + c;
            ff_act[off] = mtg_s_gelu(ff_pre[off]);
        }
        __syncthreads();
        mtg_s_linear_fwd_rows(
            tid, m, n, is_last, ffn, d, ff_act, wt + blk.ff2.w, wt + blk.ff2.b, x_next
        );
        for (int idx = tid; idx < m * d; idx += MTG_S_THREADS) {
            int r_idx = idx / d;
            int c = idx % d;
            int t = (is_last && r_idx > 0) ? (n + r_idx) : r_idx;
            long long off = (long long)t * d + c;
            x_next[off] = __fadd_rn(x_next[off], x_mid[off]);
        }
        __syncthreads();
    }

    // 6. Final LayerNorm and policy / value heads.
    float* x_out = for_backward
        ? (base + tape_off.x_out)
        : ((dims.layers % 2 == 0) ? buf_x0 : buf_x1);
    float* fin = for_backward ? (base + tape_off.fin) : inf_ln;
    float* fin_stats = for_backward ? (base + tape_off.fin_stats) : inf_stats;
    mtg_s_ln_fwd_rows(
        tid, 1 + a, n, 1, d, x_out, wt + lay.ln_g, wt + lay.ln_b, fin, 1, fin_stats, 1
    );

    float* pol_inp = for_backward ? (base + tape_off.pol_inp) : inf_work;
    float* pol_pre = for_backward ? (base + tape_off.pol_pre) : inf_work2;
    float* pol_act = for_backward ? (base + tape_off.pol_act) : inf_work2;
    for (int idx = tid; idx < a * 2 * d; idx += MTG_S_THREADS) {
        int j = idx / (2 * d);
        int col = idx % (2 * d);
        if (col < d) {
            pol_inp[idx] = fin[col];
        } else {
            pol_inp[idx] = fin[(long long)(1 + j) * d + (col - d)];
        }
    }
    __syncthreads();
    mtg_s_linear_fwd_strided(
        tid, a, 2 * d, d, pol_inp, 2 * d, wt + lay.pol0.w, d, wt + lay.pol0.b, pol_pre, d
    );
    for (int idx = tid; idx < a * d; idx += MTG_S_THREADS) {
        pol_act[idx] = mtg_s_gelu(pol_pre[idx]);
    }
    __syncthreads();
    mtg_s_linear_fwd_strided(
        tid, a, d, 1, pol_act, d, wt + lay.pol2.w, 1, wt + lay.pol2.b, out_logits, 1
    );

    float* val_pre = for_backward ? (base + tape_off.val_pre) : inf_work2;
    float* val_act = for_backward ? (base + tape_off.val_act) : inf_work2;
    mtg_s_linear_fwd_strided(
        tid, 1, d, d, fin, d, wt + lay.val0.w, d, wt + lay.val0.b, val_pre, d
    );
    for (int c = tid; c < d; c += MTG_S_THREADS) {
        val_act[c] = mtg_s_gelu(val_pre[c]);
    }
    __syncthreads();
    mtg_s_linear_fwd_strided(
        tid, 1, d, 1, val_act, d, wt + lay.val2.w, 1, wt + lay.val2.b, out_value_logit, 1
    );
}

// Phase A: one block per substep, computing all activation adjoints into `grad_scratch`.
extern "C" __global__ void mtg_s_reverse(
    const long long* __restrict__ reqs,
    const int* __restrict__ ints,
    const float* __restrict__ values,
    MtgSDims dims,
    const float* __restrict__ tape_scratch,
    const float* __restrict__ d_logits,
    const float* __restrict__ d_dz,
    const int* __restrict__ live_flags,
    const long long* __restrict__ grad_offsets,
    float* __restrict__ grad_scratch
) {
    int tid = threadIdx.x;
    int sub = blockIdx.x;
    const long long* req = reqs + (long long)sub * MTG_S_R_FIELDS;
    int n = (int)req[1];
    int e = (int)req[2];
    int a = (int)req[3];
    int r = (int)req[4];

    const int* edge_src = ints + req[12];
    const int* edge_tgt = ints + req[13];
    const int* ref_acts = ints + req[15];

    const float* tbase = tape_scratch + req[17];
    const float* dlogits_s = d_logits + req[18];
    float dz = d_dz[sub];
    int live = live_flags[sub];
    float* gbase = grad_scratch + grad_offsets[sub];

    MtgSLayout lay = mtg_s_layout(dims);
    MtgSTapeOff t = mtg_s_carve_tape(dims, n, e, a, r);
    MtgSGradOff g = mtg_s_carve_grad(dims, n, e, a, r);

    int d = dims.d_model;
    int h = dims.heads;
    int dh = d / h;
    int cd = dims.card_dim;
    int ffn = dims.ffn;
    int l = 1 + n + a;

    float* g_dz = gbase + g.dz;
    float* g_dval_pre = gbase + g.dval_pre;
    float* g_dpol_pre = gbase + g.dpol_pre;
    float* g_ln_dg = gbase + g.ln_dg;
    float* g_ln_db = gbase + g.ln_db;
    float* g_dx_final = gbase + g.dx_final;
    float* g_dstate_pre = gbase + g.dstate_pre;
    float* g_dobj_pre = gbase + g.dobj_pre;
    float* g_dobj_inp = gbase + g.dobj_inp;
    float* g_dact_pre = gbase + g.dact_pre;
    float* g_dref_out = gbase + g.dref_out;
    float* g_dref_pre = gbase + g.dref_pre;
    float* g_dref_inp = gbase + g.dref_inp;
    float* g_dbias = gbase + g.dbias;
    float* g_deb = gbase + g.deb;

    float* ws_dx = gbase + g.ws_dx;
    float* ws_dx2 = gbase + g.ws_dx2;
    float* ws_dattn = gbase + g.ws_dattn;
    float* ws_dscore = gbase + g.ws_dscore;
    float* ws_dfin = gbase + g.ws_dfin;
    float* ws_dpol_inp = gbase + g.ws_dpol_inp;

    // 1. Value head.
    if (tid == 0) {
        g_dz[0] = dz;
    }
    __syncthreads();
    mtg_s_linear_dx(tid, 1, d, 1, g_dz, 1, values + lay.val2.w, g_dval_pre, d, 0);
    for (int c = tid; c < d; c += MTG_S_THREADS) {
        g_dval_pre[c] = __fmul_rn(g_dval_pre[c], mtg_s_gelu_grad(tbase[t.val_pre + c]));
    }
    __syncthreads();
    mtg_s_linear_dx(tid, 1, d, d, g_dval_pre, d, values + lay.val0.w, ws_dfin, d, 0);

    // 2. Policy head.
    mtg_s_linear_dx(tid, a, d, 1, dlogits_s, 1, values + lay.pol2.w, g_dpol_pre, d, 0);
    for (int idx = tid; idx < a * d; idx += MTG_S_THREADS) {
        g_dpol_pre[idx] = __fmul_rn(g_dpol_pre[idx], mtg_s_gelu_grad(tbase[t.pol_pre + idx]));
    }
    __syncthreads();
    mtg_s_linear_dx(tid, a, 2 * d, d, g_dpol_pre, d, values + lay.pol0.w, ws_dpol_inp, 2 * d, 0);
    for (int c = tid; c < d; c += MTG_S_THREADS) {
        float acc = ws_dfin[c];
        for (int j = 0; j < a; ++j) {
            acc = __fadd_rn(acc, ws_dpol_inp[(long long)j * 2 * d + c]);
        }
        ws_dfin[c] = acc;
    }
    for (int idx = tid; idx < a * d; idx += MTG_S_THREADS) {
        int j = idx / d;
        int c = idx % d;
        ws_dfin[(long long)(1 + j) * d + c] = ws_dpol_inp[(long long)j * 2 * d + d + c];
    }
    __syncthreads();

    // 3. Final LayerNorm.
    for (int idx = tid; idx < l * d; idx += MTG_S_THREADS) {
        ws_dx[idx] = 0.0f;
    }
    long long dbias_len = (long long)h * l * l;
    for (long long idx = tid; idx < dbias_len; idx += MTG_S_THREADS) {
        g_dbias[idx] = 0.0f;
    }
    __syncthreads();

    for (int slot = tid; slot <= a; slot += MTG_S_THREADS) {
        int tok = (slot == 0) ? 0 : (n + slot);
        mtg_s_ln_bwd_one_row(
            d,
            tbase + t.x_out + (long long)tok * d,
            tbase[t.fin_stats + 2 * slot],
            tbase[t.fin_stats + 2 * slot + 1],
            values + lay.ln_g,
            ws_dfin + (long long)slot * d,
            g_ln_dg + (long long)slot * d,
            g_ln_db + (long long)slot * d,
            ws_dx + (long long)tok * d,
            0
        );
    }
    __syncthreads();

    // 4. Transformer blocks in reverse.
    float scale = __fdiv_rn(1.0f, __fsqrt_rn((float)dh));
    for (int layer = dims.layers - 1; layer >= 0; --layer) {
        MtgSBlockOff blk = mtg_s_block_off(lay, dims, layer);
        MtgSLayerTapeOff lt = mtg_s_layer_tape_off(t, dims, l, layer);
        MtgSBlockGradOff bg = mtg_s_block_grad_off(g, dims, l, layer);

        int is_last_live = (layer + 1 == dims.layers && live != 0) ? 1 : 0;
        int m = is_last_live ? (1 + a) : l;

        float* g_dff2_dy = gbase + bg.dff2_dy;
        float* g_dff0_dy = gbase + bg.dff0_dy;
        float* g_ln2_dg = gbase + bg.ln2_dg;
        float* g_ln2_db = gbase + bg.ln2_db;
        float* g_dproj_dy = gbase + bg.dproj_dy;
        float* g_dqkv = gbase + bg.dqkv;
        float* g_ln1_dg = gbase + bg.ln1_dg;
        float* g_ln1_db = gbase + bg.ln1_db;

        // Save `dff2_dy` (gathered live rows if `is_last_live`, else all `l` rows).
        for (int idx = tid; idx < m * d; idx += MTG_S_THREADS) {
            int r_idx = idx / d;
            int c = idx % d;
            int tok = (is_last_live && r_idx > 0) ? (n + r_idx) : r_idx;
            g_dff2_dy[idx] = ws_dx[(long long)tok * d + c];
        }
        __syncthreads();

        // FFN backward: `ff2` -> `gelu'` -> `ff0`.
        mtg_s_linear_dx(tid, m, ffn, d, g_dff2_dy, d, values + blk.ff2.w, g_dff0_dy, ffn, 0);
        for (int idx = tid; idx < m * ffn; idx += MTG_S_THREADS) {
            int r_idx = idx / ffn;
            int c = idx % ffn;
            int tok = (is_last_live && r_idx > 0) ? (n + r_idx) : r_idx;
            float pre = tbase[lt.ff_pre + (long long)tok * ffn + c];
            g_dff0_dy[idx] = __fmul_rn(g_dff0_dy[idx], mtg_s_gelu_grad(pre));
        }
        __syncthreads();
        mtg_s_linear_dx(tid, m, d, ffn, g_dff0_dy, ffn, values + blk.ff0.w, ws_dx2, d, 0);

        // LN2 backward: adds into `ws_dx[tok, :]`.
        for (int r_idx = tid; r_idx < m; r_idx += MTG_S_THREADS) {
            int tok = (is_last_live && r_idx > 0) ? (n + r_idx) : r_idx;
            mtg_s_ln_bwd_one_row(
                d,
                tbase + lt.x_mid + (long long)tok * d,
                tbase[lt.ln2_stats + 2 * tok],
                tbase[lt.ln2_stats + 2 * tok + 1],
                values + blk.ln2_g,
                ws_dx2 + (long long)r_idx * d,
                g_ln2_dg + (long long)r_idx * d,
                g_ln2_db + (long long)r_idx * d,
                ws_dx + (long long)tok * d,
                1
            );
        }
        __syncthreads();

        // Save `dproj_dy` and compute `dattn` (`m` compact rows).
        for (int idx = tid; idx < m * d; idx += MTG_S_THREADS) {
            int r_idx = idx / d;
            int c = idx % d;
            int tok = (is_last_live && r_idx > 0) ? (n + r_idx) : r_idx;
            g_dproj_dy[idx] = ws_dx[(long long)tok * d + c];
        }
        __syncthreads();
        mtg_s_linear_dx(tid, m, d, d, g_dproj_dy, d, values + blk.proj.w, ws_dattn, d, 0);

        // Attention backward across all `h` heads.
        for (int idx = tid; idx < l * 3 * d; idx += MTG_S_THREADS) {
            g_dqkv[idx] = 0.0f;
        }
        __syncthreads();

        const float* lt_qkv = tbase + lt.qkv;
        const float* lt_probs = tbase + lt.probs;

        // `dP = dO * V^T` and `dV = P^T * dO`.
        int score_items = h * m * l;
        for (int idx = tid; idx < score_items; idx += MTG_S_THREADS) {
            int j = idx % l;
            int tmp = idx / l;
            int r_idx = tmp % m;
            int head = tmp / m;
            const float* do_row = ws_dattn + (long long)r_idx * d + head * dh;
            const float* v_row = lt_qkv + (long long)j * 3 * d + 2 * d + head * dh;
            float sum = 0.0f;
            for (int c = 0; c < dh; ++c) {
                sum = __fadd_rn(sum, __fmul_rn(do_row[c], v_row[c]));
            }
            ws_dscore[((long long)head * m + r_idx) * l + j] = sum;
        }
        for (int idx = tid; idx < l * d; idx += MTG_S_THREADS) {
            int j = idx / d;
            int col = idx % d;
            int head = col / dh;
            float sum = 0.0f;
            for (int r_idx = 0; r_idx < m; ++r_idx) {
                int i = (is_last_live && r_idx > 0) ? (n + r_idx) : r_idx;
                float p_val = lt_probs[((long long)head * l + i) * l + j];
                float do_val = ws_dattn[(long long)r_idx * d + col];
                sum = __fadd_rn(sum, __fmul_rn(p_val, do_val));
            }
            g_dqkv[(long long)j * 3 * d + 2 * d + col] = sum;
        }
        __syncthreads();

        // Softmax backward `dS = P * (dP - rowsum(P * dP))` and `dbias += dS`.
        int prob_rows = h * m;
        for (int row_id = tid; row_id < prob_rows; row_id += MTG_S_THREADS) {
            int head = row_id / m;
            int r_idx = row_id % m;
            int i = (is_last_live && r_idx > 0) ? (n + r_idx) : r_idx;
            const float* p_row = lt_probs + ((long long)head * l + i) * l;
            float* ds_row = ws_dscore + ((long long)head * m + r_idx) * l;
            float* b_row = g_dbias + ((long long)head * l + i) * l;
            float row_dot = 0.0f;
            for (int j = 0; j < l; ++j) {
                row_dot = __fadd_rn(row_dot, __fmul_rn(p_row[j], ds_row[j]));
            }
            for (int j = 0; j < l; ++j) {
                float ds = __fmul_rn(p_row[j], __fsub_rn(ds_row[j], row_dot));
                ds_row[j] = ds;
                b_row[j] = __fadd_rn(b_row[j], ds);
            }
        }
        __syncthreads();

        // `dQ = (dS * K) * scale` and `dK = (dS^T * Q) * scale`.
        for (int idx = tid; idx < m * d; idx += MTG_S_THREADS) {
            int r_idx = idx / d;
            int col = idx % d;
            int head = col / dh;
            int i = (is_last_live && r_idx > 0) ? (n + r_idx) : r_idx;
            const float* ds_row = ws_dscore + ((long long)head * m + r_idx) * l;
            float sum = 0.0f;
            for (int j = 0; j < l; ++j) {
                sum = __fadd_rn(sum, __fmul_rn(ds_row[j], lt_qkv[(long long)j * 3 * d + d + col]));
            }
            g_dqkv[(long long)i * 3 * d + col] = __fmul_rn(sum, scale);
        }
        for (int idx = tid; idx < l * d; idx += MTG_S_THREADS) {
            int j = idx / d;
            int col = idx % d;
            int head = col / dh;
            float sum = 0.0f;
            for (int r_idx = 0; r_idx < m; ++r_idx) {
                int i = (is_last_live && r_idx > 0) ? (n + r_idx) : r_idx;
                float ds = ws_dscore[((long long)head * m + r_idx) * l + j];
                float q_val = lt_qkv[(long long)i * 3 * d + col];
                sum = __fadd_rn(sum, __fmul_rn(ds, q_val));
            }
            g_dqkv[(long long)j * 3 * d + d + col] = __fmul_rn(sum, scale);
        }
        __syncthreads();

        // QKV `dx2`: when `is_last_live`, object rows `1..=n` skip the query rows `0..d`.
        const float* w_qkv = values + blk.qkv.w;
        for (int idx = tid; idx < l * d; idx += MTG_S_THREADS) {
            int tok = idx / d;
            int col = idx % d;
            int first_o = (is_last_live && tok >= 1 && tok <= n) ? d : 0;
            const float* dy_row = g_dqkv + (long long)tok * 3 * d;
            float acc = 0.0f;
            for (int o = first_o; o < 3 * d; ++o) {
                acc = __fadd_rn(acc, __fmul_rn(dy_row[o], w_qkv[(long long)o * d + col]));
            }
            ws_dx2[idx] = acc;
        }
        __syncthreads();

        // LN1 backward over all `l` rows: `ws_dx += LN1'(ws_dx2)`.
        for (int tok = tid; tok < l; tok += MTG_S_THREADS) {
            mtg_s_ln_bwd_one_row(
                d,
                tbase + lt.x_in + (long long)tok * d,
                tbase[lt.ln1_stats + 2 * tok],
                tbase[lt.ln1_stats + 2 * tok + 1],
                values + blk.ln1_g,
                ws_dx2 + (long long)tok * d,
                g_ln1_dg + (long long)tok * d,
                g_ln1_db + (long long)tok * d,
                ws_dx + (long long)tok * d,
                1
            );
        }
        __syncthreads();
    }

    // 5. Save `g_dx_final` (`ws_dx` after layer 0) and run token + edge input adjoints.
    for (int idx = tid; idx < l * d; idx += MTG_S_THREADS) {
        g_dx_final[idx] = ws_dx[idx];
    }
    __syncthreads();

    // State MLP.
    mtg_s_linear_dx(tid, 1, d, d, g_dx_final, d, values + lay.state2.w, g_dstate_pre, d, 0);
    for (int c = tid; c < d; c += MTG_S_THREADS) {
        g_dstate_pre[c] = __fmul_rn(g_dstate_pre[c], mtg_s_gelu_grad(tbase[t.state_pre + c]));
    }
    __syncthreads();

    // Object MLP.
    int obj_in = MTG_S_OBJ_FEAT + cd;
    mtg_s_linear_dx(tid, n, d, d, g_dx_final + d, d, values + lay.obj2.w, g_dobj_pre, d, 0);
    for (int idx = tid; idx < n * d; idx += MTG_S_THREADS) {
        g_dobj_pre[idx] = __fmul_rn(g_dobj_pre[idx], mtg_s_gelu_grad(tbase[t.obj_pre + idx]));
    }
    __syncthreads();
    mtg_s_linear_dx(tid, n, obj_in, d, g_dobj_pre, d, values + lay.obj0.w, g_dobj_inp, obj_in, 0);

    // Action MLP.
    float* dtok_act = g_dx_final + (long long)(1 + n) * d;
    mtg_s_linear_dx(tid, a, d, d, dtok_act, d, values + lay.act2.w, g_dact_pre, d, 0);
    for (int idx = tid; idx < a * d; idx += MTG_S_THREADS) {
        g_dact_pre[idx] = __fmul_rn(g_dact_pre[idx], mtg_s_gelu_grad(tbase[t.act_pre + idx]));
    }
    __syncthreads();

    // Reference MLP (`if r > 0`).
    if (r > 0) {
        int ref_in = MTG_S_REF_FEAT + cd;
        for (int idx = tid; idx < r * d; idx += MTG_S_THREADS) {
            int k = idx / d;
            int c = idx % d;
            int j = ref_acts[k];
            g_dref_out[idx] = dtok_act[(long long)j * d + c];
        }
        __syncthreads();
        mtg_s_linear_dx(tid, r, d, d, g_dref_out, d, values + lay.ref2.w, g_dref_pre, d, 0);
        for (int idx = tid; idx < r * d; idx += MTG_S_THREADS) {
            g_dref_pre[idx] = __fmul_rn(g_dref_pre[idx], mtg_s_gelu_grad(tbase[t.ref_pre + idx]));
        }
        __syncthreads();
        mtg_s_linear_dx(tid, r, ref_in, d, g_dref_pre, d, values + lay.ref0.w, g_dref_inp, ref_in, 0);
    }

    // Edge bias gather (`if e > 0`).
    if (e > 0) {
        for (int idx = tid; idx < e * h; idx += MTG_S_THREADS) {
            int k = idx / h;
            int head = idx % h;
            int s = 1 + edge_src[k];
            int tg = 1 + edge_tgt[k];
            g_deb[(long long)k * 2 * h + head] = g_dbias[((long long)head * l + s) * l + tg];
            g_deb[(long long)k * 2 * h + h + head] = g_dbias[((long long)head * l + tg) * l + s];
        }
        __syncthreads();
    }
}

__device__ __forceinline__ const float* mtg_s_source_ptr(
    long long source,
    const float* floats,
    const float* tape_scratch,
    const float* grad_scratch,
    const float* d_logits
) {
    if (source == 0) return floats;
    if (source == 1) return tape_scratch;
    if (source == 2) return grad_scratch;
    return d_logits;
}

// Phase B1: fused linear weight + bias gradients across all substeps in ascending order.
extern "C" __global__ void mtg_s_linear_grad(
    const long long* __restrict__ specs,
    int num_specs,
    const long long* __restrict__ terms,
    int num_substeps,
    const float* __restrict__ floats,
    const float* __restrict__ tape_scratch,
    const float* __restrict__ grad_scratch,
    const float* __restrict__ d_logits,
    float* __restrict__ gradients
) {
    long long blk = blockIdx.x;
    int lo = 0;
    int hi = num_specs - 1;
    while (lo < hi) {
        int mid = (lo + hi + 1) >> 1;
        if (specs[(long long)mid * 6] <= blk) {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    const long long* sp = specs + (long long)lo * 6;
    long long first_block = sp[0];
    int out_dim = (int)sp[1];
    int in_dim = (int)sp[2];
    long long w_offset = sp[3];
    long long b_offset = sp[4];
    long long terms_offset = sp[5];

    long long local_idx = (blk - first_block) * MTG_S_THREADS + threadIdx.x;
    long long weights_len = (long long)out_dim * in_dim;
    if (local_idx < weights_len) {
        int o = (int)(local_idx / in_dim);
        int i = (int)(local_idx % in_dim);
        float acc = 0.0f;
        for (int s = 0; s < num_substeps; ++s) {
            const long long* tm = terms + (terms_offset + s) * 9;
            int rows = (int)tm[0];
            if (rows == 0) continue;
            const float* x_buf =
                mtg_s_source_ptr(tm[1], floats, tape_scratch, grad_scratch, d_logits) + tm[2];
            long long x_stride = tm[3];
            int x_live_n = (int)tm[4];
            const float* dy_buf =
                mtg_s_source_ptr(tm[5], floats, tape_scratch, grad_scratch, d_logits) + tm[6];
            long long dy_stride = tm[7];
            int dy_live_n = (int)tm[8];
            for (int r = 0; r < rows; ++r) {
                int xr = (x_live_n >= 0 && r > 0) ? (x_live_n + r) : r;
                int dyr = (dy_live_n >= 0 && r > 0) ? (dy_live_n + r) : r;
                float xv = x_buf[(long long)xr * x_stride + i];
                float dyv = dy_buf[(long long)dyr * dy_stride + o];
                acc = __fadd_rn(acc, __fmul_rn(dyv, xv));
            }
        }
        gradients[w_offset + local_idx] = acc;
    } else if (local_idx < weights_len + out_dim) {
        int o = (int)(local_idx - weights_len);
        float acc = 0.0f;
        for (int s = 0; s < num_substeps; ++s) {
            const long long* tm = terms + (terms_offset + s) * 9;
            int rows = (int)tm[0];
            if (rows == 0) continue;
            const float* dy_buf =
                mtg_s_source_ptr(tm[5], floats, tape_scratch, grad_scratch, d_logits) + tm[6];
            long long dy_stride = tm[7];
            int dy_live_n = (int)tm[8];
            for (int r = 0; r < rows; ++r) {
                int dyr = (dy_live_n >= 0 && r > 0) ? (dy_live_n + r) : r;
                acc = __fadd_rn(acc, dy_buf[(long long)dyr * dy_stride + o]);
            }
        }
        gradients[b_offset + o] = acc;
    }
}

// Phase B2: LayerNorm gain and bias gradients across all substeps in ascending order.
extern "C" __global__ void mtg_s_ln_grad(
    const long long* __restrict__ ln_grad_offsets,
    const long long* __restrict__ ln_terms,
    int num_ln_vecs,
    int num_substeps,
    int d,
    const float* __restrict__ grad_scratch,
    float* __restrict__ gradients
) {
    int idx = blockIdx.x * blockDim.x + threadIdx.x;
    if (idx >= num_ln_vecs * d) return;
    int vec_idx = idx / d;
    int c = idx % d;
    const long long* terms = ln_terms + (long long)vec_idx * num_substeps * 2;
    float acc = 0.0f;
    for (int s = 0; s < num_substeps; ++s) {
        long long base = terms[2 * s];
        int rows = (int)terms[2 * s + 1];
        for (int r = 0; r < rows; ++r) {
            acc = __fadd_rn(acc, grad_scratch[base + (long long)r * d + c]);
        }
    }
    gradients[ln_grad_offsets[vec_idx] + c] = acc;
}

// Phase B3: `group.weight` and `ref_bias` gradients across all substeps in ascending order.
extern "C" __global__ void mtg_s_group_refbias_grad(
    const long long* __restrict__ reqs,
    const int* __restrict__ ints,
    MtgSDims dims,
    int num_substeps,
    const long long* __restrict__ grad_offsets,
    const float* __restrict__ grad_scratch,
    long long group_grad_offset,
    long long ref_bias_grad_offset,
    float* __restrict__ gradients
) {
    int d = dims.d_model;
    int h = dims.heads;
    int idx = blockIdx.x * blockDim.x + threadIdx.x;
    int group_items = MTG_S_GROUP_COUNT * d;
    if (idx < group_items) {
        int grp = idx / d;
        int c = idx % d;
        float acc = 0.0f;
        for (int s = 0; s < num_substeps; ++s) {
            const long long* req = reqs + (long long)s * MTG_S_R_FIELDS;
            int n = (int)req[1];
            int e = (int)req[2];
            int a = (int)req[3];
            int r = (int)req[4];
            const int* obj_groups = ints + req[11];
            MtgSGradOff g = mtg_s_carve_grad(dims, n, e, a, r);
            const float* dobj = grad_scratch + grad_offsets[s] + g.dx_final + d;
            for (int i = 0; i < n; ++i) {
                if (obj_groups[i] == grp) {
                    acc = __fadd_rn(acc, dobj[(long long)i * d + c]);
                }
            }
        }
        gradients[group_grad_offset + idx] = acc;
    } else if (idx < group_items + 2 * h) {
        int rb = idx - group_items;
        int dir = rb / h;
        int head = rb % h;
        float acc = 0.0f;
        for (int s = 0; s < num_substeps; ++s) {
            const long long* req = reqs + (long long)s * MTG_S_R_FIELDS;
            int n = (int)req[1];
            int e = (int)req[2];
            int a = (int)req[3];
            int r = (int)req[4];
            int l = 1 + n + a;
            const int* ref_acts = ints + req[15];
            const int* ref_nodes = ints + req[16];
            MtgSGradOff g = mtg_s_carve_grad(dims, n, e, a, r);
            const float* dbias = grad_scratch + grad_offsets[s] + g.dbias;
            for (int k = 0; k < r; ++k) {
                int ai = 1 + n + ref_acts[k];
                int oi = 1 + ref_nodes[k];
                long long b_idx = (dir == 0)
                    ? (((long long)head * l + ai) * l + oi)
                    : (((long long)head * l + oi) * l + ai);
                acc = __fadd_rn(acc, dbias[b_idx]);
            }
        }
        gradients[ref_bias_grad_offset + rb] = acc;
    }
}

// Phase B4: `card.weight` gradients for touched non-padding tokens in visiting order.
extern "C" __global__ void mtg_s_card_grad(
    const int* __restrict__ tokens,
    int num_tokens,
    const int* __restrict__ starts,
    const long long* __restrict__ offsets,
    const float* __restrict__ grad_scratch,
    int cd,
    long long card_grad_offset,
    float* __restrict__ gradients
) {
    int idx = blockIdx.x * blockDim.x + threadIdx.x;
    if (idx >= num_tokens * cd) return;
    int t_idx = idx / cd;
    int c = idx % cd;
    int token = tokens[t_idx];
    int q0 = starts[t_idx];
    int q1 = starts[t_idx + 1];
    float acc = 0.0f;
    for (int q = q0; q < q1; ++q) {
        acc = __fadd_rn(acc, grad_scratch[offsets[q] + c]);
    }
    gradients[card_grad_offset + (long long)token * cd + c] = acc;
}
