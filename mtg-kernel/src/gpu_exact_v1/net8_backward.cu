// Net-8 learner backward on the GPU, bit-identical to the CPU's exact
// parallel backward (`exact_parallel_backward_v1.rs`).
//
// Phase A (`mtg_net8_reverse`): one block per substep runs
// `reverse_decision_into_dims_v1` on the substep's forward tape (the forward
// kernel's scratch, kept), writing every layer's output gradient (the terms
// phase B adds) into the substep's gradient scratch. Each element is the
// CPU's sum in the CPU's order: an input gradient starts at 0 and adds
// d_out[o] * W[o][i] for ascending o; a tanh gradient is g * (1 - a * a).
//
// Phase B (`mtg_net8_param_grad`, `mtg_net8_embedding_grad`): one thread per
// gradient element adds that element's terms for every substep in visiting
// order and, within a substep, the layer's rows in order (the scorer's output
// bias in reverse), starting from zero. No atomics: no element has two
// writers.
//
// Included after tanh.cuh and net8_forward.cu (descriptors, layout, dims).

enum MtgLayer {
    Y_OBJ1 = 0, Y_OBJ2, Y_EDGE1, Y_EDGE2, Y_NODE1, Y_NODE2, Y_STATE1, Y_STATE2,
    Y_REF1, Y_REF2, Y_ACT1, Y_ACT2, Y_SCORER1, Y_SCORER2, Y_VALUE1, Y_VALUE2, Y_LAYERS
};

// d_in[r * k + i] = sum over o in order of d_out[r * n + o] * w[o * k + i],
// starting from 0 (`input_gradient_rows_v1`).
__device__ void mtg_input_grad_stage(int rows, const float* d_out, const float* w, int n, int k,
                                     float* d_in) {
    const int total = rows * k;
    for (int item = threadIdx.x; item < total; item += blockDim.x) {
        const int r = item / k;
        const int i = item - r * k;
        const float* d = d_out + (size_t)r * n;
        float acc = 0.0f;
#pragma unroll 8
        for (int o = 0; o < n; ++o) acc = __fadd_rn(acc, __fmul_rn(d[o], w[(size_t)o * k + i]));
        d_in[item] = acc;
    }
    __syncthreads();
}

// g[j] *= 1 - a[j] * a[j] (`apply_tanh_gradient`).
__device__ void mtg_tanh_grad_stage(int count, float* g, const float* a) {
    for (int j = threadIdx.x; j < count; j += blockDim.x) {
        g[j] = __fmul_rn(g[j], __fsub_rn(1.0f, __fmul_rn(a[j], a[j])));
    }
    __syncthreads();
}

// `two_layer_backward_into_dims_v1`: d2 = d_out * tanh'(second output), d1 =
// the second linear's input gradient * tanh'(first output), d_in = the first
// linear's input gradient. d2 and d1 are phase B's terms.
__device__ void mtg_two_layer_backward(int rows, const float* d_out, const float* first_out,
                                       const float* second_out, const float* w1, int k1,
                                       const float* w2, float* d2, float* d1, float* d_in) {
    const int H = MTG_HIDDEN;
    for (int j = threadIdx.x; j < rows * H; j += blockDim.x) d2[j] = d_out[j];
    __syncthreads();
    mtg_tanh_grad_stage(rows * H, d2, second_out);
    mtg_input_grad_stage(rows, d2, w2, H, H, d1);
    mtg_tanh_grad_stage(rows * H, d1, first_out);
    mtg_input_grad_stage(rows, d1, w1, H, k1, d_in);
}

// The gradient scratch of one substep, carved in this order. The host sizes
// it with the same arithmetic (`net8_learner.rs`, `GRAD_LAYOUT`).
struct MtgGrad {
    float *d_lg, *d_v;
    float *d_sh, *d_si, *d_state, *d_act, *d_vh, *d_sv;
    float *d2_act, *d1_act, *d_ain;
    float *d_refh, *d2_ref, *d1_ref, *d_rin;
    float *d_objh, *d2_state, *d1_state, *d_sin;
    float *d2_node, *d1_node, *d_nin, *d_base, *d_epool;
    float *d_eh, *d2_edge, *d1_edge, *d_ein;
    float *d2_obj, *d1_obj, *d_oin;
};

__device__ MtgGrad mtg_carve_grad(float* g, int nobj, int nedge, int nref, int nact, MtgDims dims) {
    const int H = MTG_HIDDEN;
    const int k_obj = dims.object_feature_dim + dims.embedding_dim;
    const int k_edge = dims.edge_feature_dim + 2 * H;
    const int k_state = dims.state_dim + dims.group_count * H;
    const int k_ref = dims.action_ref_feature_dim + H;
    const int k_act = dims.action_feature_dim + H;
    MtgGrad t;
    t.d_lg = g;     g += nact;   // scorer2's d_out: the logits' gradient
    t.d_v = g;      g += 1;      // value2's d_out
    t.d_sh = g;     g += (size_t)nact * H;
    t.d_si = g;     g += (size_t)nact * 2 * H;
    t.d_state = g;  g += H;
    t.d_act = g;    g += (size_t)nact * H;
    t.d_vh = g;     g += H;
    t.d_sv = g;     g += H;
    t.d2_act = g;   g += (size_t)nact * H;
    t.d1_act = g;   g += (size_t)nact * H;
    t.d_ain = g;    g += (size_t)nact * k_act;
    t.d_refh = g;   g += (size_t)nref * H;
    t.d2_ref = g;   g += (size_t)nref * H;
    t.d1_ref = g;   g += (size_t)nref * H;
    t.d_rin = g;    g += (size_t)nref * k_ref;
    t.d_objh = g;   g += (size_t)nobj * H;
    t.d2_state = g; g += H;
    t.d1_state = g; g += H;
    t.d_sin = g;    g += k_state;
    t.d2_node = g;  g += (size_t)nobj * H;
    t.d1_node = g;  g += (size_t)nobj * H;
    t.d_nin = g;    g += (size_t)nobj * 2 * H;
    t.d_base = g;   g += (size_t)nobj * H;
    t.d_epool = g;  g += (size_t)nobj * H;
    t.d_eh = g;     g += (size_t)nedge * H;
    t.d2_edge = g;  g += (size_t)nedge * H;
    t.d1_edge = g;  g += (size_t)nedge * H;
    t.d_ein = g;    g += (size_t)nedge * k_edge;
    t.d2_obj = g;   g += (size_t)nobj * H;
    t.d1_obj = g;   g += (size_t)nobj * H;
    t.d_oin = g;    g += (size_t)nobj * k_obj;
    return t;
}

// The forward tape: the forward kernel's scratch for one request.
struct MtgTape {
    const float *obj_h1, *obj_base, *edge_h1, *edge_h, *edge_pooled, *node_h1, *obj_hidden;
    const float *group_pooled, *state_h1, *state_hidden, *ref_h1, *ref_h, *ref_pooled;
    const float *act_h1, *act_h, *scorer_h, *value_h;
};

__device__ MtgTape mtg_carve_tape(const float* s, int nobj, int nedge, int nref, int nact, MtgDims dims) {
    const int H = MTG_HIDDEN;
    MtgTape t;
    t.obj_h1 = s;       s += (size_t)nobj * H;
    t.obj_base = s;     s += (size_t)nobj * H;
    t.edge_h1 = s;      s += (size_t)nedge * H;
    t.edge_h = s;       s += (size_t)nedge * H;
    t.edge_pooled = s;  s += (size_t)nobj * H;
    t.node_h1 = s;      s += (size_t)nobj * H;
    t.obj_hidden = s;   s += (size_t)nobj * H;
    t.group_pooled = s; s += (size_t)dims.group_count * H;
    t.state_h1 = s;     s += H;
    t.state_hidden = s; s += H;
    t.ref_h1 = s;       s += (size_t)nref * H;
    t.ref_h = s;        s += (size_t)nref * H;
    t.ref_pooled = s;   s += (size_t)nact * H;
    t.act_h1 = s;       s += (size_t)nact * H;
    t.act_h = s;        s += (size_t)nact * H;
    s += H;             // scorer state prefix
    t.scorer_h = s;     s += (size_t)nact * H;
    t.value_h = s;
    return t;
}

// Phase A, one block per substep. `d_logits` uses the forward's logits
// offsets and `d_values` its value index; `grad_offsets[b]` is block b's
// gradient scratch.
extern "C" __global__ void mtg_net8_reverse(const int* __restrict__ requests,
                                            const int* __restrict__ ints,
                                            const float* __restrict__ params_torch,
                                            const long long* __restrict__ torch_layout,
                                            MtgDims dims,
                                            const float* __restrict__ scratch,
                                            const float* __restrict__ d_logits,
                                            const float* __restrict__ d_values,
                                            const long long* __restrict__ grad_offsets,
                                            float* __restrict__ grad_scratch) {
    const int* req = requests + (size_t)blockIdx.x * R_FIELDS;
    const int H = MTG_HIDDEN;
    const int nobj = req[R_OBJECTS], nedge = req[R_EDGES], nref = req[R_REFS], nact = req[R_ACTIONS];
    const int* groups = ints + req[R_GROUPS];
    const int* edge_src = ints + req[R_EDGE_SRC];
    const int* edge_tgt = ints + req[R_EDGE_TGT];
    const int* ref_node = ints + req[R_REF_NODE];
    const int* ref_action = ints + req[R_REF_ACTION];
    const float* w[Y_LAYERS];
    for (int l = 0; l < Y_LAYERS; ++l) w[l] = params_torch + torch_layout[l];
    const MtgTape t = mtg_carve_tape(scratch + (size_t)req[R_SCRATCH], nobj, nedge, nref, nact, dims);
    const MtgGrad d = mtg_carve_grad(grad_scratch + grad_offsets[blockIdx.x], nobj, nedge, nref, nact, dims);
    const int k_obj = dims.object_feature_dim + dims.embedding_dim;
    const int k_edge = dims.edge_feature_dim + 2 * H;
    const int k_state = dims.state_dim + dims.group_count * H;
    const int k_ref = dims.action_ref_feature_dim + H;
    const int k_act = dims.action_feature_dim + H;
    const float* dl = d_logits + req[R_LOGITS];
    const float dv = d_values[req[R_VALUE]];
    for (int a = threadIdx.x; a <= nact; a += blockDim.x) {
        if (a < nact) d.d_lg[a] = dl[a]; else d.d_v[0] = dv;
    }
    __syncthreads();

    // Scorer: logits -> scorer hidden -> scorer input [state hidden, action hidden].
    mtg_input_grad_stage(nact, d.d_lg, w[Y_SCORER2], 1, H, d.d_sh);
    mtg_tanh_grad_stage(nact * H, d.d_sh, t.scorer_h);
    mtg_input_grad_stage(nact, d.d_sh, w[Y_SCORER1], H, 2 * H, d.d_si);
    for (int j = threadIdx.x; j < (nact + 1) * H; j += blockDim.x) {
        if (j < H) {
            // 0, then each action's state half in action order.
            float acc = 0.0f;
            for (int a = 0; a < nact; ++a) acc = __fadd_rn(acc, d.d_si[(size_t)a * 2 * H + j]);
            d.d_state[j] = acc;
        } else {
            const int a = j / H - 1, h = j % H;
            d.d_act[(size_t)a * H + h] = d.d_si[(size_t)a * 2 * H + H + h];
        }
    }
    __syncthreads();

    // Value head; d_state_hidden += its state gradient (`add_slices`).
    mtg_input_grad_stage(1, d.d_v, w[Y_VALUE2], 1, H, d.d_vh);
    mtg_tanh_grad_stage(H, d.d_vh, t.value_h);
    mtg_input_grad_stage(1, d.d_vh, w[Y_VALUE1], H, H, d.d_sv);
    for (int h = threadIdx.x; h < H; h += blockDim.x) d.d_state[h] = __fadd_rn(d.d_state[h], d.d_sv[h]);
    __syncthreads();

    // Action encoder.
    mtg_two_layer_backward(nact, d.d_act, t.act_h1, t.act_h, w[Y_ACT1], k_act, w[Y_ACT2],
                           d.d2_act, d.d1_act, d.d_ain);

    // Action refs: each ref's hidden gradient is its action's ref-pooled
    // gradient (a copy), then the ref encoder's input gradient.
    if (nref > 0) {
        for (int j = threadIdx.x; j < nref * H; j += blockDim.x) {
            const int r = j / H, h = j % H;
            d.d_refh[j] = d.d_ain[(size_t)ref_action[r] * k_act + dims.action_feature_dim + h];
        }
        __syncthreads();
        mtg_two_layer_backward(nref, d.d_refh, t.ref_h1, t.ref_h, w[Y_REF1], k_ref, w[Y_REF2],
                               d.d2_ref, d.d1_ref, d.d_rin);
    }

    // State encoder.
    mtg_two_layer_backward(1, d.d_state, t.state_h1, t.state_hidden, w[Y_STATE1], k_state, w[Y_STATE2],
                           d.d2_state, d.d1_state, d.d_sin);

    // d_object_hidden: 0, then refs' node gradients in ref order, then each
    // object's group slot of the state input gradient.
    for (int j = threadIdx.x; j < nobj * H; j += blockDim.x) {
        const int o = j / H, h = j % H;
        float acc = 0.0f;
        for (int r = 0; r < nref; ++r) {
            if (ref_node[r] == o) acc = __fadd_rn(acc, d.d_rin[(size_t)r * k_ref + dims.action_ref_feature_dim + h]);
        }
        acc = __fadd_rn(acc, d.d_sin[dims.state_dim + groups[o] * H + h]);
        d.d_objh[j] = acc;
    }
    __syncthreads();

    // Node update; split its input gradient into base and pooled messages.
    mtg_two_layer_backward(nobj, d.d_objh, t.node_h1, t.obj_hidden, w[Y_NODE1], 2 * H, w[Y_NODE2],
                           d.d2_node, d.d1_node, d.d_nin);
    for (int j = threadIdx.x; j < nobj * H; j += blockDim.x) {
        const int o = j / H, h = j % H;
        d.d_base[j] = d.d_nin[(size_t)o * 2 * H + h];
        d.d_epool[j] = d.d_nin[(size_t)o * 2 * H + H + h];
    }
    __syncthreads();

    // Edges: each edge's hidden gradient is the sum of its endpoints' pooled
    // gradients; its input gradient then adds into both endpoints' base
    // gradients, edge by edge, source before target.
    if (nedge > 0) {
        for (int j = threadIdx.x; j < nedge * H; j += blockDim.x) {
            const int e = j / H, h = j % H;
            d.d_eh[j] = __fadd_rn(d.d_epool[(size_t)edge_src[e] * H + h], d.d_epool[(size_t)edge_tgt[e] * H + h]);
        }
        __syncthreads();
        mtg_two_layer_backward(nedge, d.d_eh, t.edge_h1, t.edge_h, w[Y_EDGE1], k_edge, w[Y_EDGE2],
                               d.d2_edge, d.d1_edge, d.d_ein);
        for (int j = threadIdx.x; j < nobj * H; j += blockDim.x) {
            const int o = j / H, h = j % H;
            float acc = d.d_base[j];
            for (int e = 0; e < nedge; ++e) {
                const float* row = d.d_ein + (size_t)e * k_edge + dims.edge_feature_dim;
                if (edge_src[e] == o) acc = __fadd_rn(acc, row[h]);
                if (edge_tgt[e] == o) acc = __fadd_rn(acc, row[H + h]);
            }
            d.d_base[j] = acc;
        }
        __syncthreads();
    }

    // Object encoder; the card embedding columns of its input gradient are
    // phase B's embedding terms.
    mtg_two_layer_backward(nobj, d.d_base, t.obj_h1, t.obj_base, w[Y_OBJ1], k_obj, w[Y_OBJ2],
                           d.d2_obj, d.d1_obj, d.d_oin);
}

// Phase B for one linear layer. Each substep's term is MTG_TERM_FIELDS
// i64 values: d_out offset (into grad_scratch, row stride n), rows, segment
// count, then per segment (up to three, as the forward reads its input):
// source (0 packed floats, 1 forward scratch, 2 parameters), base offset,
// row map offset into `ints` (-1 for none), stride, length. One thread per
// weight element (o, i) in Torch layout, then one per bias element o.
#define MTG_TERM_FIELDS 18

extern "C" __global__ void mtg_net8_param_grad(const long long* __restrict__ terms, int nterms,
                                               const float* __restrict__ grad_scratch,
                                               const float* __restrict__ floats,
                                               const float* __restrict__ scratch,
                                               const float* __restrict__ params,
                                               const int* __restrict__ ints,
                                               int n, int k, int reverse_bias_rows,
                                               float* __restrict__ weight_grad,
                                               float* __restrict__ bias_grad) {
    const int item = blockIdx.x * blockDim.x + threadIdx.x;
    if (item < n * k) {
        const int o = item / k;
        const int i = item - o * k;
        float acc = 0.0f;
        for (int t = 0; t < nterms; ++t) {
            const long long* term = terms + (size_t)t * MTG_TERM_FIELDS;
            const float* dout = grad_scratch + term[0];
            const int rows = (int)term[1];
            const int nseg = (int)term[2];
            int seg = 0, col = i;
            while (seg + 1 < nseg && col >= (int)term[3 + seg * 5 + 4]) {
                col -= (int)term[3 + seg * 5 + 4];
                ++seg;
            }
            const long long* sg = term + 3 + seg * 5;
            const float* base = (sg[0] == 0 ? floats : sg[0] == 1 ? scratch : params) + sg[1];
            const int* rowmap = sg[2] >= 0 ? ints + sg[2] : nullptr;
            const long long stride = sg[3];
#pragma unroll 4
            for (int r = 0; r < rows; ++r) {
                const long long src = rowmap ? rowmap[r] : r;
                acc = __fadd_rn(acc, __fmul_rn(dout[(size_t)r * n + o], base[src * stride + col]));
            }
        }
        weight_grad[item] = acc;
    } else if (item < n * k + n) {
        const int o = item - n * k;
        float acc = 0.0f;
        for (int t = 0; t < nterms; ++t) {
            const long long* term = terms + (size_t)t * MTG_TERM_FIELDS;
            const float* dout = grad_scratch + term[0];
            const int rows = (int)term[1];
            for (int q = 0; q < rows; ++q) {
                const int r = reverse_bias_rows ? rows - 1 - q : q;
                acc = __fadd_rn(acc, dout[(size_t)r * n + o]);
            }
        }
        bias_grad[o] = acc;
    }
}

// Phase B for the card embedding: `occurrences` lists, per touched token in
// ascending token order, its (gradient scratch offset of the embedding
// columns' row) in visiting order, then object order. One thread per
// (touched token, column).
extern "C" __global__ void mtg_net8_embedding_grad(const int* __restrict__ tokens, int ntokens,
                                                   const int* __restrict__ starts,
                                                   const long long* __restrict__ occurrences,
                                                   const float* __restrict__ grad_scratch,
                                                   int embedding_dim,
                                                   float* __restrict__ embedding_grad) {
    const int item = blockIdx.x * blockDim.x + threadIdx.x;
    if (item >= ntokens * embedding_dim) return;
    const int t = item / embedding_dim;
    const int j = item - t * embedding_dim;
    float acc = 0.0f;
    for (int q = starts[t]; q < starts[t + 1]; ++q) acc = __fadd_rn(acc, grad_scratch[occurrences[q] + j]);
    embedding_grad[(size_t)tokens[t] * embedding_dim + j] = acc;
}
