// Net-8 forward on the GPU, one thread block per request, bit-identical to
// `NativePolicyValueNetV1::forward_validated_rows_v1` (V3/V4/V7 inputs,
// no public, stack, registration or pairing rows).
//
// Every linear output is the CPU's sum: bias (or the scorer's state prefix)
// first, then input[k] * weight[k][o] for ascending k across the
// concatenated input segments, each product rounded before its add. Every
// ordered index-add gives each destination element to one thread, which adds
// the request's source rows in the CPU's order. A block reads only its own
// request and its model's parameters, so its outputs do not depend on the
// rest of the batch. Compiled with --fmad=false; every float operation below
// is an explicit correctly rounded intrinsic anyway.
//
// Included after tanh.cuh. MTG_HIDDEN is defined by the host (64 for net-8).

#ifndef MTG_HIDDEN
#error "MTG_HIDDEN must be defined"
#endif

// One input segment of a concatenated row: row r reads
// base[(rowmap ? rowmap[r] : r) * stride + k] for k in [0, len).
struct MtgSeg {
    const float* base;
    const int* rowmap;
    int stride;
    int len;
};

// out[r * n + o] = act(init[o] + sum over segments, k of in(r, k) * w[k * n + o]).
__device__ void mtg_linear_stage(int rows, const MtgSeg* segs, int nseg, const float* init,
                                 const float* w, int n, float* out, bool apply_tanh) {
    const int total = rows * n;
    for (int item = threadIdx.x; item < total; item += blockDim.x) {
        const int r = item / n;
        const int o = item - r * n;
        float acc = init[o];
        int k_base = 0;
        for (int s = 0; s < nseg; ++s) {
            const MtgSeg seg = segs[s];
            const int src_row = seg.rowmap ? seg.rowmap[r] : r;
            const float* in = seg.base + (size_t)src_row * seg.stride;
            const float* wk = w + (size_t)k_base * n + o;
            // Unrolling only lets independent loads issue early; the adds
            // stay one chain in ascending k.
#pragma unroll 8
            for (int k = 0; k < seg.len; ++k) {
                acc = __fadd_rn(acc, __fmul_rn(in[k], wk[(size_t)k * n]));
            }
            k_base += seg.len;
        }
        out[item] = apply_tanh ? mtg_tanh(acc) : acc;
    }
    __syncthreads();
}

// dest[d * H + j] = 0, then for each pass p and source row i in order:
// if idx_p[i] == d, dest[d * H + j] += src[i * H + j].
__device__ void mtg_index_add_stage(float* dest, int dest_rows, const float* src, int src_rows,
                                    const int* idx_a, const int* idx_b) {
    const int total = dest_rows * MTG_HIDDEN;
    for (int item = threadIdx.x; item < total; item += blockDim.x) {
        const int d = item / MTG_HIDDEN;
        const int j = item - d * MTG_HIDDEN;
        float acc = 0.0f;
        for (int i = 0; i < src_rows; ++i) {
            if (idx_a[i] == d) acc = __fadd_rn(acc, src[(size_t)i * MTG_HIDDEN + j]);
        }
        if (idx_b) {
            for (int i = 0; i < src_rows; ++i) {
                if (idx_b[i] == d) acc = __fadd_rn(acc, src[(size_t)i * MTG_HIDDEN + j]);
            }
        }
        dest[item] = acc;
    }
    __syncthreads();
}

// Parameter layout: offsets (in floats) into the model's parameter buffer.
// Weights are input-major ([k][o]), as `LinearV1::weight_t`.
enum MtgLayout {
    L_CARD_EMBEDDING = 0,
    L_OBJ1_W, L_OBJ1_B, L_OBJ2_W, L_OBJ2_B,
    L_EDGE1_W, L_EDGE1_B, L_EDGE2_W, L_EDGE2_B,
    L_NODE1_W, L_NODE1_B, L_NODE2_W, L_NODE2_B,
    L_STATE1_W, L_STATE1_B, L_STATE2_W, L_STATE2_B,
    L_REF1_W, L_REF1_B, L_REF2_W, L_REF2_B,
    L_ACT1_W, L_ACT1_B, L_ACT2_W, L_ACT2_B,
    L_SCORER1_W, L_SCORER1_B, L_SCORER2_W, L_SCORER2_B,
    L_VALUE1_W, L_VALUE1_B, L_VALUE2_W, L_VALUE2_B,
    L_COUNT
};

// Dims shared by every model in a batch (the host refuses mixed dims).
struct MtgDims {
    int state_dim, object_feature_dim, edge_feature_dim, action_feature_dim;
    int action_ref_feature_dim, group_count, embedding_dim;
};

// Per-request descriptor (ints): counts, then offsets into the packed float
// and int buffers, the scratch offset, the logits offset and the value index.
// R_MODEL is the host's model slot; one launch covers one model's requests.
enum MtgReq {
    R_MODEL = 0, R_OBJECTS, R_EDGES, R_REFS, R_ACTIONS,
    R_STATE, R_OBJ_FEAT, R_EDGE_FEAT, R_REF_FEAT, R_ACT_FEAT,      // float offsets
    R_CARD_IDS, R_GROUPS, R_EDGE_SRC, R_EDGE_TGT, R_REF_NODE, R_REF_ACTION,  // int offsets
    R_SCRATCH, R_LOGITS, R_VALUE,
    R_FIELDS
};

extern "C" __global__ void mtg_net8_forward(const int* __restrict__ requests,
                                            const float* __restrict__ floats,
                                            const int* __restrict__ ints,
                                            const float* __restrict__ params,
                                            const long long* __restrict__ layout,
                                            MtgDims dims,
                                            float* __restrict__ scratch,
                                            float* __restrict__ logits,
                                            float* __restrict__ values) {
    const int* req = requests + (size_t)blockIdx.x * R_FIELDS;
    const float* p = params;
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

    // Scratch, carved in the order the host sized it.
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

    __shared__ MtgSeg segs[3];
    const float* emb = p + layout[L_CARD_EMBEDDING];

    // Object encoder over [object features, card embedding].
    if (threadIdx.x == 0) {
        segs[0] = MtgSeg{obj_feat, nullptr, dims.object_feature_dim, dims.object_feature_dim};
        segs[1] = MtgSeg{emb, card_ids, dims.embedding_dim, dims.embedding_dim};
    }
    __syncthreads();
    mtg_linear_stage(nobj, segs, 2, p + layout[L_OBJ1_B], p + layout[L_OBJ1_W], H, obj_h1, true);
    if (threadIdx.x == 0) segs[0] = MtgSeg{obj_h1, nullptr, H, H};
    __syncthreads();
    mtg_linear_stage(nobj, segs, 1, p + layout[L_OBJ2_B], p + layout[L_OBJ2_W], H, obj_base, true);

    // Edge encoder and ordered messages (sources, then targets).
    if (nedge > 0) {
        if (threadIdx.x == 0) {
            segs[0] = MtgSeg{edge_feat, nullptr, dims.edge_feature_dim, dims.edge_feature_dim};
            segs[1] = MtgSeg{obj_base, edge_src, H, H};
            segs[2] = MtgSeg{obj_base, edge_tgt, H, H};
        }
        __syncthreads();
        mtg_linear_stage(nedge, segs, 3, p + layout[L_EDGE1_B], p + layout[L_EDGE1_W], H, edge_h1, true);
        if (threadIdx.x == 0) segs[0] = MtgSeg{edge_h1, nullptr, H, H};
        __syncthreads();
        mtg_linear_stage(nedge, segs, 1, p + layout[L_EDGE2_B], p + layout[L_EDGE2_W], H, edge_h, true);
    }
    mtg_index_add_stage(edge_pooled, nobj, edge_h, nedge, edge_src, edge_tgt);

    // Node update over [base hidden, pooled messages].
    if (threadIdx.x == 0) {
        segs[0] = MtgSeg{obj_base, nullptr, H, H};
        segs[1] = MtgSeg{edge_pooled, nullptr, H, H};
    }
    __syncthreads();
    mtg_linear_stage(nobj, segs, 2, p + layout[L_NODE1_B], p + layout[L_NODE1_W], H, node_h1, true);
    if (threadIdx.x == 0) segs[0] = MtgSeg{node_h1, nullptr, H, H};
    __syncthreads();
    mtg_linear_stage(nobj, segs, 1, p + layout[L_NODE2_B], p + layout[L_NODE2_W], H, obj_hidden, true);

    // Group pooling in object order, then the state encoder.
    mtg_index_add_stage(group_pooled, dims.group_count, obj_hidden, nobj, groups, nullptr);
    if (threadIdx.x == 0) {
        segs[0] = MtgSeg{state, nullptr, dims.state_dim, dims.state_dim};
        segs[1] = MtgSeg{group_pooled, nullptr, dims.group_count * H, dims.group_count * H};
    }
    __syncthreads();
    mtg_linear_stage(1, segs, 2, p + layout[L_STATE1_B], p + layout[L_STATE1_W], H, state_h1, true);
    if (threadIdx.x == 0) segs[0] = MtgSeg{state_h1, nullptr, H, H};
    __syncthreads();
    mtg_linear_stage(1, segs, 1, p + layout[L_STATE2_B], p + layout[L_STATE2_W], H, state_hidden, true);

    // Action refs, pooled per action in ref order.
    if (nref > 0) {
        if (threadIdx.x == 0) {
            segs[0] = MtgSeg{ref_feat, nullptr, dims.action_ref_feature_dim, dims.action_ref_feature_dim};
            segs[1] = MtgSeg{obj_hidden, ref_node, H, H};
        }
        __syncthreads();
        mtg_linear_stage(nref, segs, 2, p + layout[L_REF1_B], p + layout[L_REF1_W], H, ref_h1, true);
        if (threadIdx.x == 0) segs[0] = MtgSeg{ref_h1, nullptr, H, H};
        __syncthreads();
        mtg_linear_stage(nref, segs, 1, p + layout[L_REF2_B], p + layout[L_REF2_W], H, ref_h, true);
    }
    mtg_index_add_stage(ref_pooled, nact, ref_h, nref, ref_action, nullptr);

    // Action encoder over [action features, ref pooled].
    if (threadIdx.x == 0) {
        segs[0] = MtgSeg{act_feat, nullptr, dims.action_feature_dim, dims.action_feature_dim};
        segs[1] = MtgSeg{ref_pooled, nullptr, H, H};
    }
    __syncthreads();
    mtg_linear_stage(nact, segs, 2, p + layout[L_ACT1_B], p + layout[L_ACT1_W], H, act_h1, true);
    if (threadIdx.x == 0) segs[0] = MtgSeg{act_h1, nullptr, H, H};
    __syncthreads();
    mtg_linear_stage(nact, segs, 1, p + layout[L_ACT2_B], p + layout[L_ACT2_W], H, act_h, true);

    // Scorer: the state prefix once (bias, then the state inputs), then each
    // action row continues from it with its action inputs.
    if (threadIdx.x == 0) segs[0] = MtgSeg{state_hidden, nullptr, H, H};
    __syncthreads();
    mtg_linear_stage(1, segs, 1, p + layout[L_SCORER1_B], p + layout[L_SCORER1_W], H, scorer_prefix, false);
    if (threadIdx.x == 0) segs[0] = MtgSeg{act_h, nullptr, H, H};
    __syncthreads();
    mtg_linear_stage(nact, segs, 1, scorer_prefix, p + layout[L_SCORER1_W] + (size_t)H * H, H, scorer_h, true);
    if (threadIdx.x == 0) segs[0] = MtgSeg{scorer_h, nullptr, H, H};
    __syncthreads();
    mtg_linear_stage(nact, segs, 1, p + layout[L_SCORER2_B], p + layout[L_SCORER2_W], 1,
                     logits + req[R_LOGITS], false);

    // Value head.
    if (threadIdx.x == 0) segs[0] = MtgSeg{state_hidden, nullptr, H, H};
    __syncthreads();
    mtg_linear_stage(1, segs, 1, p + layout[L_VALUE1_B], p + layout[L_VALUE1_W], H, value_h, true);
    if (threadIdx.x == 0) segs[0] = MtgSeg{value_h, nullptr, H, H};
    __syncthreads();
    mtg_linear_stage(1, segs, 1, p + layout[L_VALUE2_B], p + layout[L_VALUE2_W], 1, values + req[R_VALUE], false);
}

// All 2^32 inputs in chunks: out[i] = mtg_tanh(bits(first + i)).
extern "C" __global__ void mtg_tanh_proof(unsigned int first, unsigned int count, unsigned int* out) {
    for (unsigned int i = blockIdx.x * blockDim.x + threadIdx.x; i < count; i += gridDim.x * blockDim.x) {
        out[i] = __float_as_uint(mtg_tanh(__uint_as_float(first + i)));
    }
}
