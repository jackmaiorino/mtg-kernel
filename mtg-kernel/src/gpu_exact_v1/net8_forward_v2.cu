// Net-8 forward, tiled (S3b). Same contract as `mtg_net8_forward`: every
// output is the CPU's chain (bias or the scorer's state prefix first, then
// in[k] * w[k][o] for ascending k across the concatenated segments, each
// product rounded before its add), and every ordered index-add adds each
// destination's sources in the CPU's order starting from zero.
//
// What changes is where operands come from. A block's threads stage K-tiles
// of the weights and input rows in shared memory (the next tile is copied
// asynchronously while the current one is used), and each thread keeps up to
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
// Shared floats: two weight tiles and two input tiles.
#define MTG_V2_SHARED_FLOATS (2 * MTG_V2_W_TILE + 2 * MTG_V2_X_TILE)
// Model slots one launch serves (`BatchPolicyV1::max_models`).
#define MTG_V2_MAX_MODELS 10

struct MtgModelsV2 {
    const float* params[MTG_V2_MAX_MODELS];
};

// Stage timestamps for the tuning bench only (never defined in the program).
#ifdef MTG_V2_PROFILE
__device__ long long mtg_v2_prof[32];
#define MTG_V2_MARK(i) do { if (blockIdx.x == 0 && threadIdx.x == 0) mtg_v2_prof[i] = clock64(); } while (0)
#else
#define MTG_V2_MARK(i) do { } while (0)
#endif
#ifdef MTG_V2_PROFILE
#define MTG_V2_SPAN(slot, t0) do { if (blockIdx.x == 0 && threadIdx.x == 0 && rows == 1 && tiles > 20) mtg_v2_prof[slot] += clock64() - (t0); } while (0)
#define MTG_V2_NOW() clock64()
#else
#define MTG_V2_SPAN(slot, t0) do { } while (0)
#define MTG_V2_NOW() 0LL
#endif

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

// Asynchronous global-to-shared copies (sm_80 and newer): a tile lands in
// shared memory without passing through registers.
__device__ __forceinline__ void mtg_v2_copy(float* dst, const float* src) {
    const unsigned d = (unsigned)__cvta_generic_to_shared(dst);
    // No memory clobber. Ordering comes from elsewhere: every global value a copy
    // reads (weights, inputs, activations of earlier stages) was written before a
    // barrier the copy is issued after, and its shared writes are read only after
    // the clobbered wait and a barrier.
    asm volatile("cp.async.ca.shared.global [%0], [%1], 4;\n" ::"r"(d), "l"(src));
}
// 16-byte copy for 16-byte aligned source and destination, bypassing L1.
__device__ __forceinline__ void mtg_v2_copy16(float* dst, const float* src) {
    const unsigned d = (unsigned)__cvta_generic_to_shared(dst);
    asm volatile("cp.async.cg.shared.global [%0], [%1], 16;\n" ::"r"(d), "l"(src));
}
__device__ __forceinline__ void mtg_v2_commit() { asm volatile("cp.async.commit_group;\n" ::: "memory"); }
__device__ __forceinline__ void mtg_v2_wait_all_but_one() {
    asm volatile("cp.async.wait_group 1;\n" ::: "memory");
}
__device__ __forceinline__ void mtg_v2_wait_all() { asm volatile("cp.async.wait_group 0;\n" ::: "memory"); }

// Starts copying tile `tile` of rows [r0, r0 + rows) into `ws` and `xs`.
// Elements outside the tile are not written. The compute loop may read them
// (accumulators past the chunk), but those results are never stored.
__device__ __forceinline__ void mtg_v2_issue(const MtgSeg* segs, MtgTileV2 tile, int r0, int rows,
                                             const float* w, float* ws, float* xs) {
    const float* wt = w + (size_t)tile.wrow * MTG_HIDDEN;
    const int wlen = tile.len * MTG_HIDDEN;
    if (((size_t)wt & 15) == 0) {
        // wlen is a multiple of 64 floats, and every weight tile buffer starts on 16 bytes.
        for (int at = 4 * threadIdx.x; at < wlen; at += 4 * MTG_V2_THREADS) mtg_v2_copy16(ws + at, wt + at);
    } else {
        for (int at = threadIdx.x; at < wlen; at += MTG_V2_THREADS) mtg_v2_copy(ws + at, wt + at);
    }
    const MtgSeg seg = segs[tile.seg];
    for (int at = threadIdx.x; at < rows * MTG_V2_KT; at += MTG_V2_THREADS) {
        const int r = at / MTG_V2_KT;
        const int kk = at - r * MTG_V2_KT;
        if (kk < tile.len) {
            const int src = seg.rowmap ? seg.rowmap[r0 + r] : r0 + r;
            mtg_v2_copy(xs + at, seg.base + (size_t)src * seg.stride + tile.k0 + kk);
        }
    }
    mtg_v2_commit();
}

// out[r * H + o] = act(init[o] + sum over segments, k of in(r, k) * w[k * H + o]).
// Thread t owns column o = t % H and rows g, g + G, ... (g = t / H) of each
// chunk of MTG_V2_ROWS rows. Each accumulator adds its terms in ascending k,
// tile after tile. Tile t + 1 is copied while tile t is used; buffer
// (t + 1) & 1 was last read in iteration t - 1, which ended at a barrier.
__device__ __noinline__ void mtg_v2_linear(int rows, const MtgSeg* segs, int nseg, const float* init,
                                           const float* w, float* out, bool apply_tanh, float* shared) {
    // Buffer b: weights at shared + b * W_TILE, inputs at shared + 2 * W_TILE + b * X_TILE.
    const int o = threadIdx.x % MTG_HIDDEN;
    const int g = threadIdx.x / MTG_HIDDEN;
    const int tiles = mtg_v2_tile_count(segs, nseg);
    for (int r0 = 0; r0 < rows; r0 += MTG_V2_ROWS) {
        const int chunk = min(MTG_V2_ROWS, rows - r0);
        float acc[MTG_V2_ACC];
        const float start = init[o];
#pragma unroll
        for (int j = 0; j < MTG_V2_ACC; ++j) acc[j] = start;
        if (tiles > 0)
            mtg_v2_issue(segs, mtg_v2_tile(segs, nseg, 0), r0, chunk, w, shared, shared + 2 * MTG_V2_W_TILE);
        for (int t = 0; t < tiles; ++t) {
            const int cur = t & 1;
            const long long t_issue = MTG_V2_NOW();
            if (t + 1 < tiles) {
                const int next = cur ^ 1;
                mtg_v2_issue(segs, mtg_v2_tile(segs, nseg, t + 1), r0, chunk, w, shared + next * MTG_V2_W_TILE,
                             shared + 2 * MTG_V2_W_TILE + next * MTG_V2_X_TILE);
                mtg_v2_wait_all_but_one();
            } else {
                mtg_v2_wait_all();
            }
            __syncthreads();
            MTG_V2_SPAN(20, t_issue);
            const long long t_compute = MTG_V2_NOW();
            const int len = mtg_v2_tile(segs, nseg, t).len;
            const float* ws = shared + cur * MTG_V2_W_TILE + o;
            const float* xs = shared + 2 * MTG_V2_W_TILE + cur * MTG_V2_X_TILE + g * MTG_V2_KT;
            // Warps whose row group is empty in this chunk only copy. With at most
            // one row per thread, one accumulator; otherwise every accumulator runs
            // (those past the chunk compute unused values and are never stored), so
            // the loop has no predicates and its loads pipeline.
            if (chunk <= MTG_V2_GROUPS) {
                if (g < chunk) {
                    // Operands load eight steps at a time, then the chain adds them
                    // in order, so load latency overlaps instead of joining the chain.
                    int kk = 0;
                    for (; kk + 8 <= len; kk += 8) {
                        float xv[8], wv[8];
#pragma unroll
                        for (int u = 0; u < 8; ++u) {
                            xv[u] = xs[kk + u];
                            wv[u] = ws[(kk + u) * MTG_HIDDEN];
                        }
#pragma unroll
                        for (int u = 0; u < 8; ++u) acc[0] = __fadd_rn(acc[0], __fmul_rn(xv[u], wv[u]));
                    }
                    for (; kk < len; ++kk) acc[0] = __fadd_rn(acc[0], __fmul_rn(xs[kk], ws[kk * MTG_HIDDEN]));
                }
            } else if (len == MTG_V2_KT) {
#pragma unroll 4
                for (int kk = 0; kk < MTG_V2_KT; ++kk) {
                    const float wv = ws[kk * MTG_HIDDEN];
#pragma unroll
                    for (int j = 0; j < MTG_V2_ACC; ++j)
                        acc[j] = __fadd_rn(acc[j], __fmul_rn(xs[j * MTG_V2_GROUPS * MTG_V2_KT + kk], wv));
                }
            } else {
                for (int kk = 0; kk < len; ++kk) {
                    const float wv = ws[kk * MTG_HIDDEN];
#pragma unroll
                    for (int j = 0; j < MTG_V2_ACC; ++j)
                        acc[j] = __fadd_rn(acc[j], __fmul_rn(xs[j * MTG_V2_GROUPS * MTG_V2_KT + kk], wv));
                }
            }
            MTG_V2_SPAN(21, t_compute);
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
    const int passes = idx_b ? 2 : 1;
    const int dest_floats = dest_rows * MTG_HIDDEN;
    if (dest_floats > MTG_V2_SHARED_FLOATS) {
        mtg_index_add_stage(dest, dest_rows, src, src_rows, idx_a, idx_b);
        return;
    }
    // When they fit, the source rows and indices are staged in shared memory
    // too (after the destinations), so the ordered walk below reads no global
    // memory. Scratch rows start on 64 floats, so the rows copy 16 bytes at a time.
    const bool staged = dest_floats + src_rows * (MTG_HIDDEN + passes) <= MTG_V2_SHARED_FLOATS;
    const float* rows = src;
    const int* ia = idx_a;
    const int* ib = idx_b;
    if (staged) {
        float* srows = shared + dest_floats;
        int* sidx = reinterpret_cast<int*>(srows + src_rows * MTG_HIDDEN);
        for (int at = 4 * threadIdx.x; at < src_rows * MTG_HIDDEN; at += 4 * MTG_V2_THREADS)
            mtg_v2_copy16(srows + at, src + at);
        for (int i = threadIdx.x; i < src_rows; i += MTG_V2_THREADS) {
            mtg_v2_copy(reinterpret_cast<float*>(sidx + i), reinterpret_cast<const float*>(idx_a + i));
            if (idx_b) mtg_v2_copy(reinterpret_cast<float*>(sidx + src_rows + i), reinterpret_cast<const float*>(idx_b + i));
        }
        mtg_v2_commit();
        rows = srows;
        ia = sidx;
        ib = idx_b ? sidx + src_rows : nullptr;
    }
    for (int at = threadIdx.x; at < dest_floats; at += blockDim.x) shared[at] = 0.0f;
    if (staged) mtg_v2_wait_all();
    __syncthreads();
    const int j = threadIdx.x % MTG_HIDDEN;
    const int g = threadIdx.x / MTG_HIDDEN;
    for (int pass = 0; pass < passes; ++pass) {
        const int* idx = pass == 0 ? ia : ib;
        for (int i = 0; i < src_rows; ++i) {
            const int d = idx[i];
            if (d % MTG_V2_GROUPS == g) {
                shared[d * MTG_HIDDEN + j] = __fadd_rn(shared[d * MTG_HIDDEN + j], rows[(size_t)i * MTG_HIDDEN + j]);
            }
        }
    }
    __syncthreads();
    for (int at = threadIdx.x; at < dest_floats; at += blockDim.x) dest[at] = shared[at];
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
    extern __shared__ __align__(16) float shared[];
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

    MTG_V2_MARK(0);
    const float* emb = p + layout[L_CARD_EMBEDDING];
    MtgSeg segs[3];

    // Object encoder over [object features, card embedding].
    segs[0] = MtgSeg{obj_feat, nullptr, dims.object_feature_dim, dims.object_feature_dim};
    segs[1] = MtgSeg{emb, card_ids, dims.embedding_dim, dims.embedding_dim};
    mtg_v2_linear(nobj, segs, 2, p + layout[L_OBJ1_B], p + layout[L_OBJ1_W], obj_h1, true, shared);
    MTG_V2_MARK(1);
    segs[0] = MtgSeg{obj_h1, nullptr, H, H};
    mtg_v2_linear(nobj, segs, 1, p + layout[L_OBJ2_B], p + layout[L_OBJ2_W], obj_base, true, shared);
    MTG_V2_MARK(2);

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
    MTG_V2_MARK(3);

    // Node update over [base hidden, pooled messages].
    segs[0] = MtgSeg{obj_base, nullptr, H, H};
    segs[1] = MtgSeg{edge_pooled, nullptr, H, H};
    mtg_v2_linear(nobj, segs, 2, p + layout[L_NODE1_B], p + layout[L_NODE1_W], node_h1, true, shared);
    MTG_V2_MARK(4);
    segs[0] = MtgSeg{node_h1, nullptr, H, H};
    mtg_v2_linear(nobj, segs, 1, p + layout[L_NODE2_B], p + layout[L_NODE2_W], obj_hidden, true, shared);
    MTG_V2_MARK(5);

    // Group pooling in object order, then the state encoder.
    mtg_v2_index_add(group_pooled, dims.group_count, obj_hidden, nobj, groups, nullptr, shared);
    MTG_V2_MARK(6);
    segs[0] = MtgSeg{state, nullptr, dims.state_dim, dims.state_dim};
    segs[1] = MtgSeg{group_pooled, nullptr, dims.group_count * H, dims.group_count * H};
    mtg_v2_linear(1, segs, 2, p + layout[L_STATE1_B], p + layout[L_STATE1_W], state_h1, true, shared);
    MTG_V2_MARK(7);
    segs[0] = MtgSeg{state_h1, nullptr, H, H};
    mtg_v2_linear(1, segs, 1, p + layout[L_STATE2_B], p + layout[L_STATE2_W], state_hidden, true, shared);
    MTG_V2_MARK(8);

    // Action refs, pooled per action in ref order.
    if (nref > 0) {
        segs[0] = MtgSeg{ref_feat, nullptr, dims.action_ref_feature_dim, dims.action_ref_feature_dim};
        segs[1] = MtgSeg{obj_hidden, ref_node, H, H};
        mtg_v2_linear(nref, segs, 2, p + layout[L_REF1_B], p + layout[L_REF1_W], ref_h1, true, shared);
        segs[0] = MtgSeg{ref_h1, nullptr, H, H};
        mtg_v2_linear(nref, segs, 1, p + layout[L_REF2_B], p + layout[L_REF2_W], ref_h, true, shared);
    }
    mtg_v2_index_add(ref_pooled, nact, ref_h, nref, ref_action, nullptr, shared);
    MTG_V2_MARK(9);

    // Action encoder over [action features, ref pooled].
    segs[0] = MtgSeg{act_feat, nullptr, dims.action_feature_dim, dims.action_feature_dim};
    segs[1] = MtgSeg{ref_pooled, nullptr, H, H};
    mtg_v2_linear(nact, segs, 2, p + layout[L_ACT1_B], p + layout[L_ACT1_W], act_h1, true, shared);
    MTG_V2_MARK(10);
    segs[0] = MtgSeg{act_h1, nullptr, H, H};
    mtg_v2_linear(nact, segs, 1, p + layout[L_ACT2_B], p + layout[L_ACT2_W], act_h, true, shared);
    MTG_V2_MARK(11);

    // Scorer: the state prefix once (bias, then the state inputs), then each
    // action row continues from it with its action inputs.
    segs[0] = MtgSeg{state_hidden, nullptr, H, H};
    mtg_v2_linear(1, segs, 1, p + layout[L_SCORER1_B], p + layout[L_SCORER1_W], scorer_prefix, false, shared);
    MTG_V2_MARK(12);
    segs[0] = MtgSeg{act_h, nullptr, H, H};
    mtg_v2_linear(nact, segs, 1, scorer_prefix, p + layout[L_SCORER1_W] + (size_t)H * H, scorer_h, true, shared);
    MTG_V2_MARK(13);
    mtg_v2_linear_column(nact, scorer_h, p + layout[L_SCORER2_B], p + layout[L_SCORER2_W],
                         logits + req[R_LOGITS], shared);
    MTG_V2_MARK(14);

    // Value head.
    segs[0] = MtgSeg{state_hidden, nullptr, H, H};
    mtg_v2_linear(1, segs, 1, p + layout[L_VALUE1_B], p + layout[L_VALUE1_W], value_h, true, shared);
    MTG_V2_MARK(15);
    mtg_v2_linear_column(1, value_h, p + layout[L_VALUE2_B], p + layout[L_VALUE2_W], values + req[R_VALUE],
                         shared);
    MTG_V2_MARK(16);
}
