// Net-8 forward, tiled (S3b). Same contract as `mtg_net8_forward`: every
// output is the CPU's chain (bias or the scorer's state prefix first, then
// in[k] * w[k][o] for ascending k across the concatenated segments, each
// product rounded before its add), and every ordered index-add adds each
// destination's sources in the CPU's order starting from zero.
//
// What changes is where operands come from. A block's threads stage K-tiles
// of the weights and input rows in shared memory (the next tile is fetched
// into registers while the current one is used), and each thread keeps up to
// MTG_V2_ROWS / 4 accumulators, one per row of its output column. A tile
// shape decides which thread computes an output, never the order of that
// output's operations, so every shape gives the same bits (tested with two).
//
// One launch serves every model: a request's descriptor names its model slot,
// and the `models` parameter holds each slot's parameter base pointer (models
// with equal dims share one layout). A block still reads only its own request
// and its model, so replies do not depend on the batch.
//
// Included after net8_forward.cu (segments, descriptors, layout, dims).

#ifndef MTG_V2_KT
#define MTG_V2_KT 64
#endif
#ifndef MTG_V2_ROWS
#define MTG_V2_ROWS 32
#endif
#define MTG_V2_THREADS 256
#define MTG_V2_GROUPS (MTG_V2_THREADS / MTG_HIDDEN)
#define MTG_V2_ACC (MTG_V2_ROWS / MTG_V2_GROUPS)
#define MTG_V2_W_TILE (MTG_V2_KT * MTG_HIDDEN)
#define MTG_V2_X_TILE (MTG_V2_ROWS * MTG_V2_KT)
#define MTG_V2_W_PER_THREAD (MTG_V2_W_TILE / MTG_V2_THREADS)
#define MTG_V2_X_PER_THREAD ((MTG_V2_X_TILE + MTG_V2_THREADS - 1) / MTG_V2_THREADS)
// Shared floats: two weight tiles and two input tiles.
#define MTG_V2_SHARED_FLOATS (2 * MTG_V2_W_TILE + 2 * MTG_V2_X_TILE)
// Model slots one launch serves (`BatchPolicyV1::max_models`).
#define MTG_V2_MAX_MODELS 10

struct MtgModelsV2 {
    const float* params[MTG_V2_MAX_MODELS];
};

// Tile t of a stage's concatenated input: its segment, the first k inside the
// segment, its length and the weight row of its first k.
struct MtgTileV2 {
    int seg, k0, len, wrow;
};

__device__ __forceinline__ int mtg_v2_tile_count(const MtgSeg* segs, int nseg) {
    int count = 0;
    for (int s = 0; s < nseg; ++s) count += (segs[s].len + MTG_V2_KT - 1) / MTG_V2_KT;
    return count;
}

__device__ __forceinline__ MtgTileV2 mtg_v2_tile(const MtgSeg* segs, int nseg, int t) {
    int wrow = 0;
    for (int s = 0; s < nseg; ++s) {
        const int tiles = (segs[s].len + MTG_V2_KT - 1) / MTG_V2_KT;
        if (t < tiles) {
            const int k0 = t * MTG_V2_KT;
            const int len = min(MTG_V2_KT, segs[s].len - k0);
            return MtgTileV2{s, k0, len, wrow + k0};
        }
        t -= tiles;
        wrow += segs[s].len;
    }
    return MtgTileV2{0, 0, 0, 0};
}

// Registers holding one tile on its way to shared memory.
struct MtgFetchV2 {
    float w[MTG_V2_W_PER_THREAD];
    float x[MTG_V2_X_PER_THREAD];
};

// Reads tile `tile` of rows [r0, r0 + rows) into registers. Elements outside
// the tile read as zero; the compute loop never uses them.
__device__ __forceinline__ void mtg_v2_fetch(const MtgSeg* segs, MtgTileV2 tile, int r0, int rows,
                                             const float* w, MtgFetchV2& f) {
    const MtgSeg seg = segs[tile.seg];
    const float* wt = w + (size_t)tile.wrow * MTG_HIDDEN;
    const int wlen = tile.len * MTG_HIDDEN;
#pragma unroll
    for (int m = 0; m < MTG_V2_W_PER_THREAD; ++m) {
        const int at = threadIdx.x + m * MTG_V2_THREADS;
        f.w[m] = at < wlen ? wt[at] : 0.0f;
    }
#pragma unroll
    for (int m = 0; m < MTG_V2_X_PER_THREAD; ++m) {
        const int at = threadIdx.x + m * MTG_V2_THREADS;
        const int r = at / MTG_V2_KT;
        const int kk = at - r * MTG_V2_KT;
        float v = 0.0f;
        if (r < rows && kk < tile.len) {
            const int src = seg.rowmap ? seg.rowmap[r0 + r] : r0 + r;
            v = seg.base[(size_t)src * seg.stride + tile.k0 + kk];
        }
        if (at < MTG_V2_X_TILE) f.x[m] = v;
    }
}

__device__ __forceinline__ void mtg_v2_store(float* ws, float* xs, const MtgFetchV2& f) {
#pragma unroll
    for (int m = 0; m < MTG_V2_W_PER_THREAD; ++m) ws[threadIdx.x + m * MTG_V2_THREADS] = f.w[m];
#pragma unroll
    for (int m = 0; m < MTG_V2_X_PER_THREAD; ++m) {
        const int at = threadIdx.x + m * MTG_V2_THREADS;
        if (at < MTG_V2_X_TILE) xs[at] = f.x[m];
    }
}

// out[r * H + o] = act(init[o] + sum over segments, k of in(r, k) * w[k * H + o]).
// Thread t owns column o = t % H and rows g, g + G, ... (g = t / H) of each
// chunk of MTG_V2_ROWS rows. Each accumulator adds its terms in ascending k,
// tile after tile.
__device__ void mtg_v2_linear(int rows, const MtgSeg* segs, int nseg, const float* init,
                              const float* w, float* out, bool apply_tanh, float* shared) {
    float* ws_buf[2] = {shared, shared + MTG_V2_W_TILE};
    float* xs_buf[2] = {shared + 2 * MTG_V2_W_TILE, shared + 2 * MTG_V2_W_TILE + MTG_V2_X_TILE};
    const int o = threadIdx.x % MTG_HIDDEN;
    const int g = threadIdx.x / MTG_HIDDEN;
    const int tiles = mtg_v2_tile_count(segs, nseg);
    for (int r0 = 0; r0 < rows; r0 += MTG_V2_ROWS) {
        const int chunk = min(MTG_V2_ROWS, rows - r0);
        float acc[MTG_V2_ACC];
#pragma unroll
        for (int j = 0; j < MTG_V2_ACC; ++j) acc[j] = init[o];
        MtgFetchV2 f;
        if (tiles > 0) {
            mtg_v2_fetch(segs, mtg_v2_tile(segs, nseg, 0), r0, chunk, w, f);
            mtg_v2_store(ws_buf[0], xs_buf[0], f);
        }
        __syncthreads();
        for (int t = 0; t < tiles; ++t) {
            const int cur = t & 1;
            const bool more = t + 1 < tiles;
            if (more) mtg_v2_fetch(segs, mtg_v2_tile(segs, nseg, t + 1), r0, chunk, w, f);
            const int len = mtg_v2_tile(segs, nseg, t).len;
            const float* ws = ws_buf[cur];
            const float* xs = xs_buf[cur];
            for (int kk = 0; kk < len; ++kk) {
                const float wv = ws[kk * MTG_HIDDEN + o];
#pragma unroll
                for (int j = 0; j < MTG_V2_ACC; ++j) {
                    const int r = g + j * MTG_V2_GROUPS;
                    if (r < chunk) acc[j] = __fadd_rn(acc[j], __fmul_rn(xs[r * MTG_V2_KT + kk], wv));
                }
            }
            if (more) mtg_v2_store(ws_buf[cur ^ 1], xs_buf[cur ^ 1], f);
            __syncthreads();
        }
#pragma unroll
        for (int j = 0; j < MTG_V2_ACC; ++j) {
            const int r = g + j * MTG_V2_GROUPS;
            if (r < chunk) out[(size_t)(r0 + r) * MTG_HIDDEN + o] = apply_tanh ? mtg_tanh(acc[j]) : acc[j];
        }
    }
    __syncthreads();
}

// out[r] = init[0] + sum over k of in[r * H + k] * w[k] (one output column,
// the scorer's and value head's last layers). Rows and the weight column are
// staged in shared memory; thread r runs row r's chain.
__device__ void mtg_v2_linear_column(int rows, const float* in, const float* init, const float* w,
                                     float* out, float* shared) {
    float* ws = shared;
    float* xs = shared + MTG_HIDDEN;
    const int per_chunk = (MTG_V2_SHARED_FLOATS - MTG_HIDDEN) / MTG_HIDDEN;
    const int chunk_rows = min(per_chunk, MTG_V2_THREADS);
    for (int k = threadIdx.x; k < MTG_HIDDEN; k += blockDim.x) ws[k] = w[k];
    for (int r0 = 0; r0 < rows; r0 += chunk_rows) {
        const int chunk = min(chunk_rows, rows - r0);
        for (int at = threadIdx.x; at < chunk * MTG_HIDDEN; at += blockDim.x) {
            xs[at] = in[(size_t)r0 * MTG_HIDDEN + at];
        }
        __syncthreads();
        if (threadIdx.x < chunk) {
            const float* x = xs + threadIdx.x * MTG_HIDDEN;
            float acc = init[0];
            for (int k = 0; k < MTG_HIDDEN; ++k) acc = __fadd_rn(acc, __fmul_rn(x[k], ws[k]));
            out[r0 + threadIdx.x] = acc;
        }
        __syncthreads();
    }
}

// dest[d * H + j] = 0, then for each pass p and source row i in order:
// if idx_p[i] == d, dest[d * H + j] += src[i * H + j]. Thread (g, j) owns
// column j of the destinations d with d % G == g and walks the sources once,
// so each destination element still adds its sources in the CPU's order. The
// destinations accumulate in shared memory when they fit.
__device__ void mtg_v2_index_add(float* dest, int dest_rows, const float* src, int src_rows,
                                 const int* idx_a, const int* idx_b, float* shared) {
    const int capacity = MTG_V2_SHARED_FLOATS / MTG_HIDDEN;
    if (dest_rows > capacity) {
        mtg_index_add_stage(dest, dest_rows, src, src_rows, idx_a, idx_b);
        return;
    }
    for (int at = threadIdx.x; at < dest_rows * MTG_HIDDEN; at += blockDim.x) shared[at] = 0.0f;
    __syncthreads();
    const int j = threadIdx.x % MTG_HIDDEN;
    const int g = threadIdx.x / MTG_HIDDEN;
    for (int pass = 0; pass < (idx_b ? 2 : 1); ++pass) {
        const int* idx = pass == 0 ? idx_a : idx_b;
        for (int i = 0; i < src_rows; ++i) {
            const int d = idx[i];
            if (d % MTG_V2_GROUPS == g) {
                shared[d * MTG_HIDDEN + j] = __fadd_rn(shared[d * MTG_HIDDEN + j], src[(size_t)i * MTG_HIDDEN + j]);
            }
        }
    }
    __syncthreads();
    for (int at = threadIdx.x; at < dest_rows * MTG_HIDDEN; at += blockDim.x) dest[at] = shared[at];
    __syncthreads();
}

// Fault injection (feature gpu-exact-fault-injection-v1): a real device fault,
// which poisons the context as a hardware or driver fault would.
extern "C" __global__ void mtg_fault_trap() { __trap(); }

extern "C" __global__ void __launch_bounds__(MTG_V2_THREADS)
mtg_net8_forward_v2(const int* __restrict__ requests,
                    const float* __restrict__ floats,
                    const int* __restrict__ ints,
                    MtgModelsV2 models,
                    const long long* __restrict__ layout,
                    MtgDims dims,
                    float* __restrict__ scratch,
                    float* __restrict__ logits,
                    float* __restrict__ values) {
    extern __shared__ float shared[];
    const int* req = requests + (size_t)blockIdx.x * R_FIELDS;
    const float* p = models.params[req[R_MODEL]];
    const int H = MTG_HIDDEN;
    const int nobj = req[R_OBJECTS], nedge = req[R_EDGES], nref = req[R_REFS], nact = req[R_ACTIONS];
    const float* state = floats + req[R_STATE];
    const float* obj_feat = floats + req[R_OBJ_FEAT];
    const float* edge_feat = floats + req[R_EDGE_FEAT];
    const float* ref_feat = floats + req[R_REF_FEAT];
    const float* act_feat = floats + req[R_ACT_FEAT];
    const int* card_ids = ints + req[R_CARD_IDS];
    const int* groups = ints + req[R_GROUPS];
    const int* edge_src = ints + req[R_EDGE_SRC];
    const int* edge_tgt = ints + req[R_EDGE_TGT];
    const int* ref_node = ints + req[R_REF_NODE];
    const int* ref_action = ints + req[R_REF_ACTION];

    // Scratch, carved exactly as `mtg_net8_forward` (the learner's tape).
    float* s = scratch + (size_t)req[R_SCRATCH];
    float* obj_h1 = s;            s += (size_t)nobj * H;
    float* obj_base = s;          s += (size_t)nobj * H;
    float* edge_h1 = s;           s += (size_t)nedge * H;
    float* edge_h = s;            s += (size_t)nedge * H;
    float* edge_pooled = s;       s += (size_t)nobj * H;
    float* node_h1 = s;           s += (size_t)nobj * H;
    float* obj_hidden = s;        s += (size_t)nobj * H;
    float* group_pooled = s;      s += (size_t)dims.group_count * H;
    float* state_h1 = s;          s += H;
    float* state_hidden = s;      s += H;
    float* ref_h1 = s;            s += (size_t)nref * H;
    float* ref_h = s;             s += (size_t)nref * H;
    float* ref_pooled = s;        s += (size_t)nact * H;
    float* act_h1 = s;            s += (size_t)nact * H;
    float* act_h = s;             s += (size_t)nact * H;
    float* scorer_prefix = s;     s += H;
    float* scorer_h = s;          s += (size_t)nact * H;
    float* value_h = s;           s += H;

    const float* emb = p + layout[L_CARD_EMBEDDING];
    MtgSeg segs[3];

    // Object encoder over [object features, card embedding].
    segs[0] = MtgSeg{obj_feat, nullptr, dims.object_feature_dim, dims.object_feature_dim};
    segs[1] = MtgSeg{emb, card_ids, dims.embedding_dim, dims.embedding_dim};
    mtg_v2_linear(nobj, segs, 2, p + layout[L_OBJ1_B], p + layout[L_OBJ1_W], obj_h1, true, shared);
    segs[0] = MtgSeg{obj_h1, nullptr, H, H};
    mtg_v2_linear(nobj, segs, 1, p + layout[L_OBJ2_B], p + layout[L_OBJ2_W], obj_base, true, shared);

    // Edge encoder and ordered messages (sources, then targets).
    if (nedge > 0) {
        segs[0] = MtgSeg{edge_feat, nullptr, dims.edge_feature_dim, dims.edge_feature_dim};
        segs[1] = MtgSeg{obj_base, edge_src, H, H};
        segs[2] = MtgSeg{obj_base, edge_tgt, H, H};
        mtg_v2_linear(nedge, segs, 3, p + layout[L_EDGE1_B], p + layout[L_EDGE1_W], edge_h1, true, shared);
        segs[0] = MtgSeg{edge_h1, nullptr, H, H};
        mtg_v2_linear(nedge, segs, 1, p + layout[L_EDGE2_B], p + layout[L_EDGE2_W], edge_h, true, shared);
    }
    mtg_v2_index_add(edge_pooled, nobj, edge_h, nedge, edge_src, edge_tgt, shared);

    // Node update over [base hidden, pooled messages].
    segs[0] = MtgSeg{obj_base, nullptr, H, H};
    segs[1] = MtgSeg{edge_pooled, nullptr, H, H};
    mtg_v2_linear(nobj, segs, 2, p + layout[L_NODE1_B], p + layout[L_NODE1_W], node_h1, true, shared);
    segs[0] = MtgSeg{node_h1, nullptr, H, H};
    mtg_v2_linear(nobj, segs, 1, p + layout[L_NODE2_B], p + layout[L_NODE2_W], obj_hidden, true, shared);

    // Group pooling in object order, then the state encoder.
    mtg_v2_index_add(group_pooled, dims.group_count, obj_hidden, nobj, groups, nullptr, shared);
    segs[0] = MtgSeg{state, nullptr, dims.state_dim, dims.state_dim};
    segs[1] = MtgSeg{group_pooled, nullptr, dims.group_count * H, dims.group_count * H};
    mtg_v2_linear(1, segs, 2, p + layout[L_STATE1_B], p + layout[L_STATE1_W], state_h1, true, shared);
    segs[0] = MtgSeg{state_h1, nullptr, H, H};
    mtg_v2_linear(1, segs, 1, p + layout[L_STATE2_B], p + layout[L_STATE2_W], state_hidden, true, shared);

    // Action refs, pooled per action in ref order.
    if (nref > 0) {
        segs[0] = MtgSeg{ref_feat, nullptr, dims.action_ref_feature_dim, dims.action_ref_feature_dim};
        segs[1] = MtgSeg{obj_hidden, ref_node, H, H};
        mtg_v2_linear(nref, segs, 2, p + layout[L_REF1_B], p + layout[L_REF1_W], ref_h1, true, shared);
        segs[0] = MtgSeg{ref_h1, nullptr, H, H};
        mtg_v2_linear(nref, segs, 1, p + layout[L_REF2_B], p + layout[L_REF2_W], ref_h, true, shared);
    }
    mtg_v2_index_add(ref_pooled, nact, ref_h, nref, ref_action, nullptr, shared);

    // Action encoder over [action features, ref pooled].
    segs[0] = MtgSeg{act_feat, nullptr, dims.action_feature_dim, dims.action_feature_dim};
    segs[1] = MtgSeg{ref_pooled, nullptr, H, H};
    mtg_v2_linear(nact, segs, 2, p + layout[L_ACT1_B], p + layout[L_ACT1_W], act_h1, true, shared);
    segs[0] = MtgSeg{act_h1, nullptr, H, H};
    mtg_v2_linear(nact, segs, 1, p + layout[L_ACT2_B], p + layout[L_ACT2_W], act_h, true, shared);

    // Scorer: the state prefix once (bias, then the state inputs), then each
    // action row continues from it with its action inputs.
    segs[0] = MtgSeg{state_hidden, nullptr, H, H};
    mtg_v2_linear(1, segs, 1, p + layout[L_SCORER1_B], p + layout[L_SCORER1_W], scorer_prefix, false, shared);
    segs[0] = MtgSeg{act_h, nullptr, H, H};
    mtg_v2_linear(nact, segs, 1, scorer_prefix, p + layout[L_SCORER1_W] + (size_t)H * H, scorer_h, true, shared);
    mtg_v2_linear_column(nact, scorer_h, p + layout[L_SCORER2_B], p + layout[L_SCORER2_W],
                         logits + req[R_LOGITS], shared);

    // Value head.
    segs[0] = MtgSeg{state_hidden, nullptr, H, H};
    mtg_v2_linear(1, segs, 1, p + layout[L_VALUE1_B], p + layout[L_VALUE1_W], value_h, true, shared);
    mtg_v2_linear_column(1, value_h, p + layout[L_VALUE2_B], p + layout[L_VALUE2_W], values + req[R_VALUE],
                         shared);
}
