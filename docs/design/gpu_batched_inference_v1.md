# GPU batched inference and GPU learner for self-play (v1 design)

Status: revised after design review (GPT-6 Astra ultra and Fable max, both PROCEED WITH CHANGES; findings and dispositions in section 10), 2026-10-31. Engineering only. Nothing here changes a study, gate, frozen measurement or production lineage until the qualification in section 7 passes.

## 0. Goal and the decision that shapes everything

Self-play training with a large network needs the network math on the GPU: batched inference for the collectors, and a GPU learner. The binding constraint: training outputs replay byte-identically, or within a ratified, bounded tolerance.

**Decision: exact-order kernels.** Every v1 GPU kernel computes each output element with the same IEEE float operations, in the same order, as the CPU code that defines it. CUDA's correctly rounded intrinsics (`__fadd_rn`, `__fmul_rn`, `__fmaf_rn`, `__fdiv_rn`, `__fsqrt_rn`), NVRTC with `--fmad=false`, no `-use_fast_math`, and no flush-to-zero make every operation's result the IEEE result. The GPU output then equals the CPU output bit for bit. Three properties follow:

1. **CPU parity is exact.** A net-8 segment on the GPU path produces the same logits, values, trajectories and `after_state_sha256` values as the CPU path. There is no new numerical lineage and no tolerance to ratify.
2. **Batch-composition invariance is structural.** A request's outputs read only that request's inputs and the parameters. Each output's reduction order is fixed by the CPU definition, never by batch size, tiling or packing.
3. **Run-to-run and cross-device determinism.** There are no atomics, no split-K, no shape-dependent kernel selection and no TF32.

This is also what the pipeline already demands. The update step replays every collected decision on the CPU forward and requires bit equality (`replay_learner_groups_v1`, `ordered_update_preparation.rs:588`), the CPU learner recomputes each substep and checks the collection bits (`gae_v1.rs:840`), and GAE targets use the collection-time value bits. A tolerance-bounded GPU forward (the existing Burn path: 0 of 61 outputs bit-identical) fails all three. An exact forward passes them unchanged, so the CPU replay stays as an independent check during qualification.

The cost: no tensor cores and no cuBLAS (cuBLAS chooses kernels and K splits by shape). Section 6 measures the price. Section 5 sketches the tier above it for nets where speed matters more than CPU parity.

## 1. What the CPU path computes today (net-8, RT1)

- **Forward** (`native_policy_value_net_v1.rs:924-1147`): object encoder over `[object features 98, card embedding 16]`; edge encoder over `[edge features 41, source hidden, target hidden]`; edge messages added into each object (all source rows in edge order, then all target rows); node update over `[base hidden, pooled messages]`; 20 object groups summed in object order; state encoder over `[state 219, pooled 1280]`; action-ref encoder and per-action sum; action encoder over `[action features, ref pooled]`; scorer over `[state hidden, action hidden]` (state prefix computed once); value head. All two-layer tanh MLPs, hidden 64.
- **Arithmetic.** f32. Every linear output is bias first, then one sequential sum of `input[k] * weight[o][k]` in ascending k, each product rounded before its add (no FMA; `accumulate_linear_row_v1`, `:1555`). AVX2 only widens across outputs. The production activation is platform `tanhf`; on Windows that is UCRT's FMA3 path, which `vector_tanh_v1.rs` already re-implements as IEEE operations and proves equal on all 2^32 inputs. V3, V4 and V7 are the same arithmetic with different input labels. V8 adds a pairing message path and a guarded include column.
- **Collection** (`phase1_parallel_collection.rs:39`): `collection_workers` scoped threads, each playing one game at a time to completion, outputs reordered into schedule order. Per decision, `cached_forward_v1` (`sideboard_play_policy_v1.rs:2581`) returns `{logits, value}`; sampling draws from the seat's `SplitMix64` (`sample_scores`, `:2461`), seeded per episode. The opponent seat may be a different frozen checkpoint (`OpponentCacheV1`, LRU of 8).
- **Update.** Behavior replay (CPU forward on every stored tensor, bit check), GAE targets, then `train_step_gae_core_profiled_dims_v1` (`gae_v1.rs:695`): per-substep forward tapes, loss `(sum(-joint_logp * adv) + c * sum((v - target)^2)) / group_count`, backward (groups in reverse, substeps in reverse, each layer's parameter terms added as it goes), scorer-bias gauge, Adam (`native_policy_train_step_v1.rs:6168`). `exact_parallel_backward_v1.rs` already splits the backward into phase A (per-substep activation gradients, independent) and phase B (each gradient element's additions in visiting order), bit-identical to the sequential pass.
- **Measured split** (net-size memo, D1 recipe, 1.41 CPU-s per update): network forward 8.5%, learner step 5.4%, parameter-sized bookkeeping 25%, everything else (engine, digests, tensorization, I/O) 61%.

## 2. Batched inference service

### 2.1 Shape

```
collector threads (one game each)          inference service thread (owns the CUDA context)
  tensorize decision                            wait for requests
  client.forward(model, view) ----request----> gather until: every live client is waiting,
     (blocks)                                     or max_batch, or the latency window closes
                                                pack -> upload -> one kernel launch -> download
  <------------------------- {logits, value} -- scatter replies by request id
  sample with the seat's own RNG (unchanged)
```

- **Concurrency without rewriting the game loop.** The game loop stays synchronous. RT1 runs more collector threads than cores (one per in-flight game), and a thread waiting on the GPU frees its core. `collection_workers` is part of the run config and is bound on resume (`collection_workers_requested` in each completion receipt), so a GPU-path run declares its worker count like any other run; the CPU and GPU paths of one run use the same count, and only the backend flags differ. `ordered_parallel_jobs_v1` already returns outputs in schedule order, so how many threads exist and which finishes first cannot change the collection's bytes.
- **The hook** (`FrozenPlayPolicyV1::score_owned`). Each collector thread enters the service once (`enter_collector_v1`, one client per thread, so a game's two seats count as one waiting client). The `cached_forward_v1` call gains one branch: on a thread with a client, under the platform tanh, with no prediction cache or call probe attached, the policy sends its view and uses the reply. Tensorization before and the output checks, the no-state-digest cross-check and sampling after are unchanged. The policy caches its net's host copy and digest, cleared at every site that already resets its row cache when parameters change.
- **An execution option, not a run identity.** Replies equal the CPU forward's bits, so the backend is a command-line option of the RT1 binary (`--gpu-exact-inference ORDINAL [--gpu-exact-latency-us N]`), outside the config, its run identity and `completion.json` (which resume compares byte for byte). A run can resume across backends. Each invocation writes `gpu-exact-receipts/invocation-NNNNNN.json` in the run's output directory (outside every iteration directory, so it is no part of the run identity or the byte comparison) and prints it on stderr: device identity, program digest, host `tanhf` identity and tanh proof key, batches, requests, launches, model uploads, busy seconds, and the learner's updates and substeps.
- **Batch policy.** Flush when every live client has a request pending (nothing else can arrive), when the batch reaches 256 requests, or when `max_latency_us` (default 200) passes since the oldest request.
- **Models.** Requests carry their model's digest (parameters, layout and dims). The service keeps up to 10 resident models (LRU) and uploads a model on first use (5 MB at net-8). A batch may mix any number of models: it runs in groups of at most 10 distinct models, never evicting a model its group uses, one launch per model. A device error fails only its group's requests.
- **Liveness.** Each request carries its own reply channel, and the service holds its only sender, so a service that dies with requests pending fails those waits at once.
- **Floating-point environment.** The tanh proof holds in the default MXCSR state with UCRT's FMA3 paths on. The hook checks the calling thread's state before every GPU forward and keeps the CPU forward otherwise; the learner hook checks it too.
- **Randomness stays in the worker.** The service returns logits and value only.
- **Fail closed, fall back exactly.** A non-finite output or a device error fails that request, which fails the episode as a CPU forward error would. A request the exact kernel does not run (V8 pairing, other widths, public/stack/registration inputs, other activation modes) never reaches the service: the policy runs the CPU forward, which gives the same bits by definition. The receipt's request count shows how much the GPU served.

### 2.2 The kernel: one block per request

At net-8 sizes a layer-by-layer kernel sequence costs about 20 launches per batch, which is most of the latency. v1 runs **one fused kernel per batch, one thread block per request**: the block reads its request's descriptor (row counts and offsets into the packed buffers), computes every stage in order in a per-block scratch area with `__syncthreads()` between stages, and writes the request's logits and value. Inside a stage, threads stride over `(row, output)` pairs, and each pair runs the CPU's sequential sum. The ordered index-adds (edge messages, group pooling, action-ref pooling) give each destination element to one thread, which loops over the request's source rows in the CPU's order. No atomics anywhere.

Batch invariance is then immediate: a block's result is a function of its request and the parameters. The test in 7.1 checks it anyway.

### 2.3 The activation

`gpu_exact_v1/tanh.cuh` is `tanh8`'s per-lane operations with correctly rounded intrinsics and bit-pattern constants. Before a device is used, a proof kernel evaluates all 2^32 inputs and the host compares every non-NaN result with the loaded `tanhf` (NaN inputs are excluded: any NaN reaches an output, which both paths refuse) (the same proof `vector_tanh_v1` runs for `tanh8`). The record is keyed by the kernel source digest, the NVRTC version, the device name and compute capability, the driver version, and the host `tanhf` image digest. The portable `KernelDeterministicTanh` math is not ported in v1; under it every forward stays on the CPU.

Lineage note: on Linux the CPU path's `tanhf` is glibc's, so a Linux CPU run differs from a Windows one today. The GPU kernel implements the Windows UCRT-FMA3 lineage, the production one. The GPU path refuses to start on a host whose `tanhf` identity is unavailable (not Windows UCRT with FMA3 in the default MXCSR state), and the proof compares against the loaded `tanhf` itself, so a mismatch anywhere refuses the device.

### 2.4 Startup cost (the "20 s JIT")

The Burn path JIT-compiles hundreds of CubeCL kernels per process. v1 compiles one small CUDA source with NVRTC (measured in section 6) and caches the resulting PTX/cubin on disk keyed by `sha256(source, options, NVRTC version, compute capability)`. A segment process loads from the cache in milliseconds. The tanh proof is a one-time per-device record. The service lives inside the RT1 process (no IPC), so nothing else is paid per segment.

## 3. GPU learner

### 3.1 Split

| Step | v1 (this PR) | v2 (needed above net-8) |
|---|---|---|
| Behavior replay forward (all stored tensors, bit check) | CPU | GPU, batched, same kernel as inference |
| Per-substep forward tapes and outputs | CPU tapes, plus a GPU forward whose scratch is the device tape; the GPU outputs must equal the CPU tape's bits or the update fails | GPU only |
| Loss head: log-softmax, joint log-probability, value error, logits and value gradients (`prepare_reverse_logits_v1`) | CPU, existing code | CPU (about 0.1% of the work; it uses the platform `exp` and `ln`) |
| Scorer-bias gauge, in visiting order | CPU, existing code | CPU |
| Backward phase A: each substep's reverse pass | GPU, one block per substep (`mtg_net8_reverse`) | same |
| Backward phase B: each gradient element's ordered sum | GPU, one thread per element (`mtg_net8_param_grad`, `mtg_net8_embedding_grad`) | same |
| Adam, gauge zeroing, train-state hash, checkpoint | CPU, existing code | Adam on the GPU (elementwise, exact); hash and checkpoint after the throughput lead's binary payload |

The integration point is the one the exact parallel CPU backward already uses: `train_step_gae_core_profiled_dims_v1` takes `(gradients, gauge)` from `gpu_exact_backward_v1` instead of `exact_parallel_backward_v1` when a learner is installed (`--gpu-exact-learner ORDINAL`, again an execution option outside the run identity). Everything after it is unchanged code. v1 keeps the CPU forward tapes so that the loss head runs on existing code and each update cross-checks the GPU forward against the CPU tape; at net-8 that costs the 0.05 s per update the CPU spends today. v2 removes them, which is required for 20M+ nets (section 6).

### 3.2 Parity test and determinism

- **Parity:** on the RT1 net-8 recipe, the GPU learner's per-update `after_state_sha256` equals the CPU learner's on every update. Exact, not bounded.
- **Determinism:** same hashes across two fresh processes and across GPU 0 and GPU 1 (different architectures, sm_89 and sm_86).
- **No cuBLAS workspace question arises** because v1 calls no cuBLAS. If section 5's tier uses cuBLAS later: `CUBLAS_WORKSPACE_CONFIG=:4096:8`, pinned math mode without TF32, and an explicit check that no split-K algorithm is chosen.

### 3.3 Phase B cost at scale

Phase B is "one thread per parameter, sum in a fixed order over substeps and rows". For dense layers that is about 713 x rows products per weight, which parallelizes over weights (64 x 64 to 1,000s x 1,000s). The card embedding is sparse: only rows that occur get threads, each summing its occurrences in the CPU's order. Section 6 measures phase B's shape.

## 4. Model-agnostic interfaces

```rust
/// One decision's request: the encoded view plus the model it runs under.
pub(crate) struct InferenceRequestV1<'a> { model: ModelKeyV1, view: NativeEncodedDecisionViewV1<'a> }

/// A network the GPU can run exactly. Net-8 is the first implementation.
pub(crate) trait ExactDeviceNetV1 {
    /// Identity: architecture, dims, parameter digest, math (tanh) mode.
    fn key(&self) -> ModelKeyV1;
    /// Parameters as one contiguous f32 buffer in a declared order.
    fn parameters(&self) -> &[f32];
    /// Pack one request's rows into the batch buffers; returns its descriptor.
    fn pack(&self, view: &NativeEncodedDecisionViewV1<'_>, batch: &mut PackedBatchV1) -> Result<RequestDescV1, String>;
    /// The CUDA source (kernels and entry names) and its CPU reference forward.
    fn device_program(&self) -> &'static DeviceProgramV1;
}
```

The service, batching policy, model cache, client hook, compile cache, proof records and test harness depend only on this trait. A transformer over object tokens with rich card embeddings plugs in as another `ExactDeviceNetV1`: its tokens are the request's rows, and its CPU reference defines the reduction order the kernel mirrors. The contract that keeps a new architecture exact: its CPU reference must be written with a GPU-friendly order (for example blocked K sums), and its kernels must follow it.

The learner interface is the same idea: `ExactDeviceTrainerV1` provides forward-with-tape, phase A, the phase B element order, and Adam over the same parameter buffer.

## 5. Beyond exact order (not v1)

Exact order forbids tensor cores. At 20M to 50M parameters the dense math dominates, and a tensor-core path is up to several times faster (section 6). The next tier keeps **determinism and batch invariance but not CPU parity**:

- Batch-invariant kernels: fixed K tiling and reduction order per output, independent of M (the batch); no split-K; fixed `mma` instruction shapes. Deterministic run to run on one device architecture.
- CPU replay becomes a **bounded** check (max ULP or absolute error per output, ratified), and the lineage is "numerical backend: gpu-tc-v1 on sm_89". That is a lineage decision for the lead, with its own receipt, and it is out of scope here.

Sections 6 and 8 project both tiers so the lead can see when the second one pays.

## 6. Microbenchmarks on GPU 1 (RTX 3050 6 GB, sm_86)

Receipts: lane `evidence/microbench-gpu1.jsonl` (`examples/gpu_infer_microbench_v1.rs`) and `evidence/gemm-variants-gpu1.jsonl` (`tools/gemm_variants.py`, driver API). 2026-10-31, Jack's PC, one host_slots core.

| Probe | Result |
|---|---|
| NVRTC compile of the exact program, plus module load | 284 ms once; then cached PTX (the Burn path pays about 20 s of JIT per segment process) |
| Round trip: upload, one kernel, download, sync (pageable host memory) | 61 us at 4 KB, 75 us at 64 KB, 319 us at 1 MB (p50) |
| Kernel launch in a dependent chain | 8.6 us each (WDDM) |
| Exact-order linear, net-8 width (K = N = 64), 16,384 rows | 0.75 TMAC/s (cuBLAS SGEMM 0.86) |
| Exact-order linear, K = N = 512 to 1,024, large batches | 0.76 to 0.92 TMAC/s, no FMA (net-8's order); 0.79 to 1.08 TMAC/s with FMA (a new net's order) |
| cuBLAS SGEMM, same shapes (not batch invariant; reference only) | 1.8 to 2.3 TMAC/s |
| Bits | every exact variant equals its sequential CPU reference on every checked output |

What this says:

- **The v1 fused net-8 kernel, measured** (`gpu_forward_timing_receipt`, random decisions of up to 40 objects, 60 edges and 24 actions, so larger than D1's): 355 us at batch 1, 61 us per decision at batch 10, 25 at 64, 19 at 256 and 512. The same random decisions cost 378 us each on one E-core CPU thread (that comparison is for these synthetic shapes only; D1's own CPU forward is about 50 us). One block per request runs every stage in order, and the state encoder's 1,499-term chain on 64 threads sets the latency.
- **What the math alone would allow.** A decision is 0.65M multiply-adds, about 1 us at the exact rates below, so v1 is about 20x off the math. The proposed fix (not built): run the large layers (state, edge and object encoders) as one batched exact GEMM per layer across all requests and models, keep the small per-request stages fused, and launch once per batch for all models. Target about 3 us per decision; every row that depends on it below is marked as a target.
- **Exact order costs about 2.5x against cuBLAS at large shapes**, half of it from separate multiply and add (no FMA). A new net whose CPU reference uses `f32::mul_add` in the same order gets the FMA rate and stays exact. Tuned exact kernels (double buffering, vector loads) should reach 1.5 TMAC/s; that is engineering, not a measured number.
- **The 4070 SUPER** has about 3.9x the 3050's FP32 rate (35 against 9 TFLOPS). Projections below scale by that ratio; GPU 0 belongs to the pilot until it reports, so it is not measured here.

## 7. Tests and qualification

### 7.1 Tests (each PR)

Device tests are `#[ignore]` plus a feature (`gpu-exact-v1`), run locally on GPU 1 with receipts (CI has no GPU). CI compiles the feature (cudarc loads the driver dynamically, so no toolkit is needed) and runs the host-side unit tests.

Status on 2026-10-31 (GPU 1, RTX 3050, driver 13030, NVRTC 12.8, program sha256 5d5211e8; lane `evidence/devtests-20261031.txt`). All passed:

| Test | What it checks | Result |
|---|---|---|
| `gpu_tanh_equals_host_tanhf_on_all_inputs` | the kernel's tanh against the host `tanhf`, all 2^32 inputs | equal (25 s) |
| `gpu_forward_equals_cpu_forward_bit_for_bit` | 4 nets with every parameter random, 500 random decisions each (up to 40 objects, 60 edges, 30 refs, 24 actions; signed zeros mixed in) | every logit and value bit equal |
| `gpu_replies_do_not_depend_on_batch_composition` | 400 requests alone, then in batches of 2, 7, 64, 256 and 400 drawn at random, three models mixed | every reply equal to its batch of one |
| `service_replies_equal_cpu_under_any_timing` | 8 client threads, 2 models, latency windows 0, 200 and 5,000 us (mean batch 1.0, 4.9, 8.0) | every reply equal to the CPU forward |
| `phase1_parallel_episodes_with_gpu_inference_match_serial_cpu` | 6 real Burn games (self-play and a separate opponent policy) collected with 1, 3 and 6 workers through the service | trajectories byte-identical to serial CPU collection; 2,598 forwards served by the GPU |
| `gpu_exact_learner_preserves_every_bit` | two consecutive GAE updates (399 and 240 substeps) through the GPU backward | gradients, gauge, loss terms and train state identical to the CPU; 65 us per substep |
| host tests | random fixtures validate on the CPU forward; packed-batch offsets | pass |

Round 2, after the design review (same device; lane `evidence/devtests-round2-20261031.txt`). All passed:

| Test | What it checks | Result |
|---|---|---|
| all round-1 tests above (tanh proof reused from its record) | as above, with the review fixes | pass |
| `service_serves_more_models_than_resident_slots` | 12 models pending at once against 10 slots, 72 requests | every reply equal to the CPU; 6 batches, 72 launches, 31 uploads (eviction and re-upload) |
| `a_service_fault_fails_the_waiting_client` | a service thread that panics with a request pending | the client fails within 0.4 s, no hang |
| `non_finite_outputs_fail_closed_like_the_cpu` | a net whose outputs overflow | the GPU refuses exactly where the CPU forward does |
| `policy_hook_follows_parameter_replacement_and_falls_back_exactly` | the hook through `replace_training_parameters_v3`, `apply_no_state_digest_v1` and `into_flat_v8_v1` | GPU replies equal the CPU after each; the V8 policy never reaches the service |
| `phase1_parallel_episodes_with_gpu_inference_match_serial_cpu` | now with a distinct opponent model | byte-identical; 2 model uploads, 2,598 forwards served |
| `gpu_exact_learner_preserves_every_bit` | adds a random net on edge-case inputs (signed zeros, subnormals, a self-loop edge, a repeated card), 939 substeps over 3 updates | every gradient, gauge and state bit equal |

Still to add in the PRs: the 14 golden fixture cases and recorded RT1 decisions on the forward test, a fresh-process and cross-device (GPU 0) determinism run, and fault injection (non-finite output, device error) failing closed.

### 7.2 Qualification (done criteria)

On Jack's PC, GPU 1, the D1 recipe (prepared root, chain r2-s1, 18 updates), through the lane's segment runner (`tools/run_segment.py`, a small timing and correctness check under COMPUTE-POLICY):

1. Four segments with the same config (same `collection_workers`): CPU, GPU inference only, GPU learner only, and both. All produce identical per-update `after_state_sha256` and identical output bytes outside the timing receipts and `gpu-exact-receipts/`. The combined segment is rerun in a fresh process and matches. A cross-backend resume (first half on the CPU, second half on the GPU, and the reverse) matches the uninterrupted CPU run.
1b. A device fault (the injected service fault) fails the segment closed, and a CPU rerun resumes from the same checkpoint.
1c. Later, once the pilot releases GPU 0: the combined segment on GPU 0 (sm_89) matches GPU 1 (sm_86). The component profile on recorded D1 decisions and the whole-host probe that 8.4 needs.
2. Updates per hour for both at the same core claim, plus CPU-seconds per update and GPU utilization, recorded in the run manifest. Expected at net-8 with the v1 kernels: roughly a wash (section 8.4); the point is the machinery.
3. A short note (chart and table) in the lane folder.

## 8. The full AlphaZero split (Jack, 2026-10-31): CPU simulates, GPU infers and trains

Scope note from Jack: the binding constraint is hobby compute, not engineering. Options are judged by completed games and updates per unit of compute; rewrites are allowed; determinism and replay receipts may be redesigned but not dropped.

### 8.1 Where an update's CPU goes today, and what the split does with each part

Memo split (D1, net-8, 1.41 CPU-s per update of 10 games):

| Share | Part | In the split | Who |
|---:|---|---|---|
| 8.3% | Engine (game simulation) | stays on the CPU: the floor | engine |
| 10.0% | Tensorization and observation | stays on the CPU (features come from engine state) | this lane, later |
| 11.4% | SHA-512 "state digest" | **not bookkeeping: it is 96 net-8 input features** (SHA-512 of the canonical state JSON, plus per-action digests of each action's canonical JSON, `native_flat_tensorizer_v2.rs`). New nets drop the digest features (a new tensorizer contract, extending the existing no-state-digest one to the action digests), which removes the canonical JSON writes and every SHA-512 from the hot loop. Net-8 keeps them. | model decision (lead, pilot) |
| 25.2% | Parameter-sized bookkeeping (checkpoint JSON, train-state hash, source reloads) | binary formats, resident learner, streamed chunked digests | throughput lane |
| 21.8% | Trajectory and record publication, file I/O | trajectories held in memory in the packed columnar layout the GPU uploads, written once per update as one binary blob hashed while writing | throughput lane (format), this lane (layout) |
| 8.5% + 5.4% | Network forward and learner step | GPU | this lane |
| 9.3% | Waits, JSON, other | mostly gone with the per-segment processes and JSON receipts | both |

Ownership is agreed with the throughput lead (2026-10-31): they own the on-disk checkpoint, train-state, trajectory and receipt formats and their hashing; this lane owns batched inference, the GPU learner, host/device parameter movement and the collector batching protocol, and has asked them for 256-byte aligned per-tensor sections, a chunked (Merkle) state digest that can be computed while downloading or on the GPU, and columnar trajectory tensors.

### 8.2 Receipts that stay exact and stop eating the gain

- **Train state:** a digest per update stays, but over 1 MiB chunks combined in order (computable in parallel, on the host or the GPU), so it costs milliseconds at 50M parameters instead of about 0.4 s for one serial SHA-256 pass over 600 MB. The current `after_state_sha256` stays computable until the equivalence receipt (throughput lane) maps old to new on a pinned replay.
- **Trajectories:** hashed once at write over the compact binary record; no per-decision JSON.
- **Behavior replay stays an independent CPU check.** A GPU replay of GPU-collected bits would be a self-check: a kernel bug would agree with itself. So every stage keeps the full CPU behavior replay until a ratified equivalence receipt exists. After that, a lead decision may reduce it to a deterministic, outcome-blind subset (for example every k-th decision by a digest of its tensor bits, chosen before the outcome is known), never to zero. The learner's GPU tape check (logits and value only, not hidden activations) is an extra check, not a replacement. Cost: the CPU replay is about 4% of an update today and stays on the CPU in S3.
- **Per-state digest:** none is needed for replay exactness; the trajectory digest, the tensor bits replayed on the GPU and the train-state digest cover the chain.

### 8.3 The architecture

```
one long-lived process per host (no per-segment processes)
  actor threads (all free cores): each runs one game at a time
     engine step -> tensorize (no digest features for new nets) -> submit to the GPU service -> sample -> step
  GPU service per GPU: batches across every game of every run on the host; models keyed by digest
  learner per run on a GPU: resident parameters and Adam moments; forward (tape and replay), backward, Adam, chunked digest
  records: columnar packed trajectories in memory, one binary write per update
  R runs interleave: while run A learns on the GPU, runs B..R collect on the CPUs
```

COMPUTE-POLICY item 3 forbids stale-policy batches, so a run never collects with parameters older than its last update. The pipeline stays busy by interleaving independent runs (seeds, arms), as D1's 16 concurrent runs already do, not by overlapping one run's collection with its own learning.

### 8.4 Measured ceiling and projected games per second, Jack's PC

**Status of these numbers: scenario estimates and upper bounds, not qualified throughput.** The component split subtracts workloads that played different games (the no-digest policy has different weights, so its games differ), imports the memo's forward-to-tensorization ratio, and extrapolates a 6-thread run on three SMT P-cores to all 24 threads. Before the lead plans on them, they need (a) a component profile on identical recorded D1 decision surfaces and (b) a whole-host probe with D1's decision mix (both listed in 7.2). The memo's own shares give a cross-check: removing bookkeeping, records and waits (56% of today's CPU) is 2.3x, against the 4.3x the extrapolation gives for the same row, so the host rows below are upper bounds.

**Engine ceiling probe** (`expanded_deck_training_v1::engine_ceiling_probe_v1`, lane `evidence/ceiling.jsonl`): D1's real episodes (decks, seeds, reset cuts), 24 games per thread, a host_slots claim of P-core threads 8-13, 2026-10-31. Per decision, one thread among six sharing three P-cores:

| Part | us per decision | How measured |
|---|---:|---|
| Engine and decision surface | 173 | uniform legal choice |
| State digest features (SHA-512 of the state JSON) | 261 | policy minus policy under the no-state-digest contract (action digests still on) |
| Tensorization | 146 | rest of the no-digest policy, split from the forward by the memo's ratio |
| CPU forward and sampling | 124 | as above |

Scaled to the whole PC (16 P-core threads at that rate, 8 E-cores at the measured E-core ratio 0.57, 139 decisions per D1 game):

| Host CPU-bound throughput | games/s | x today |
|---|---:|---:|
| Today, measured (D1, 16 runs, all overheads) | 49 | 1.0 |
| Engine + tensorization + digest + CPU forward (records and bookkeeping removed) | 210 | 4.3 |
| Engine + tensorization + CPU forward, no state digest features | 334 | 6.8 |
| **Engine + tensorization, forward on the GPU, no state digest features (stage S4 floor)** | **464** | **9.5** |
| Engine alone (the ceiling the GPU path must feed) | 855 | 17.5 |

Caveats: uniform and fixture-policy games run about 270 decisions against D1's 139, so per-decision costs are the transferable numbers; the action digests are still on in the no-digest row (removing them is part of the new-net input contract and lowers the floor further); E-core scaling is from one 6-thread run.

**GPU capacity per device** (`evidence/PROJECTION.json`, `tools/project.py`). Per game: 139 collection forwards, 68 replayed opponent rows and 71 learner substeps (forward, phase A, phase B), 420 forward-equivalents.

| Net | multiply-adds per decision | RTX 3050 | RTX 4070 SUPER | RTX 4060 (HaleysPC) |
|---|---:|---:|---:|---:|
| net-8, v1 kernels as measured, at D1's attainable per-model batch (about 10 games in flight per run: fused forward 61 us per decision, learner 65 us per substep) | 0.65M | about 58 | not measured | not measured |
| net-8, target kernels (not built: layer-batched exact GEMMs, all models in one launch; about 1 us per decision of math) | 0.65M | about 1,350 (target) | about 2,000 (target) | about 1,600 (target) |
| 30x net-8 compute | 20M | 101 to 129 | 393 to 503 | 167 to 214 |
| Dense 20M transformer over 48 tokens | 0.92G | 2.2 to 2.8 | 8.5 to 10.9 | 3.6 to 4.6 |
| Dense 50M transformer over 48 tokens | 2.4G | 0.8 to 1.1 | 3.3 to 4.2 | 1.4 to 1.8 |
| Pilot S (d 96, 3 layers; 0.45M dense, no digest inputs) | 14.8M | 136 to 174 | 531 to 680 | 226 to 289 |
| Pilot M (d 256, 6 layers; 5.4M dense) | 195M | 10 to 13 | 40 to 52 | 17 to 22 |
| Pilot L (d 512, 8 layers; 27.5M dense) | 1.02G | 2.0 to 2.5 | 7.7 to 9.9 | 3.3 to 4.2 |
| net-8 wide (h 256, e 64) | 14.6M | 138 to 177 | 538 to 689 | 229 to 293 |

Pilot rows: the bignet pilot's object transformer (one state token, one per object, one per legal action; edges and refs as attention biases; one forward scores every legal action), multiply-adds at its mean shape of 39 tokens (pilot `macs.py`, collab branch `claude/bignet-pilot-20261031`). Measured on its final store (200k decisions): mean 38.9 tokens, p90 61, p99 94, max 436, so per-decision cost has a long tail.

Ranges are net-8's no-FMA order to a new net's FMA order, both exact. Larger nets scale from the 3050's measured exact GEMM rate (0.84 to 1.08 TMAC/s) by FP32 peak; the 4070 SUPER and 4060 rows are scaled, not measured. Large-net rows use forward GEMM rates for the backward as well; parameter-gradient shapes and Adam and receipt costs at 20M+ are not benchmarked. The 420 forward-equivalents assume D1's mix (learner rows 49%, opponent rows 51% of forwards, memo section 1); a pure self-play run has both seats as learner substeps and about 556. D1's per-model batch is about 10 (one run's games), and independent runs have different weights, so batching across runs needs the multi-model launch above.

**What this means for the big-network track:**

1. **The budget is multiply-adds per decision, not parameters.** At the S4 CPU floor (464 games/s, an upper-bound estimate), one 4070 SUPER with exact kernels keeps up with about **17M multiply-adds per decision**, about 26x net-8 (same caveats). Parameters in lookup tables (rich card embeddings) are nearly free; dense width times tokens is what costs.
2. **A dense 20M to 50M transformer is GPU-bound far below today's rate**: about 15 to 20 games/s (20M) and about 6 to 7 games/s (50M) across all three GPUs, against 49 today and 464 at the CPU floor. That holds even before the learner's extra passes for larger batches.
3. **The deterministic tensor-core tier (section 5)** would raise the GPU rows by an estimated 4 to 5x (not measured), which still leaves a dense 50M net near 30 games/s and needs a lineage decision (exact against the same GPU, bounded against the CPU).
4. **The pilot's candidates against the budget:** S (14.8M multiply-adds) and net-8 wide fit one 4070 SUPER at the CPU floor; M needs about 11x the exact-kernel budget (about 90 games/s across all three GPUs with the FMA order, against 49 today); L needs about 60x (about 17 games/s across all three). M becomes viable with the tensor-core tier or with batches across runs; L needs both and more.
5. **Architectures that fit:** compute per physical decision once and score substeps cheaply (most forwards share a state); keep dense width modest and tokens few; put capacity in embeddings; consider a large net that trains on a slice of games while net-sized policies act only if the lead accepts that learning semantics, which this design does not assume.

### 8.5 Staged path, each stage paying for itself

| Stage | What changes | Owner | Host games/s (net-8) | Receipts |
|---|---|---|---:|---|
| S0 | Today: D1, 16 runs, all CPU | | 49 | unchanged |
| S1 | Parameter-work lever: binary train state, resident learner, cached sources (exact) | throughput lane | about 60 (the memo's 1.23x) | same hashes |
| S2 | Records written once in binary, hashed at write; columnar trajectory tensors | throughput lane (format), this lane (layout) | about 80 | new trajectory digest with a one-time equivalence receipt |
| S3a | PR A and PR B as built: exact GPU inference (execution flag) and exact GPU backward, CPU replay, tapes, gauge and Adam unchanged | this lane | about neutral at net-8 (v1 kernels about 58 games/s per 3050 at D1's batch); the machinery and its exactness are the point | bit-identical; full CPU replay |
| S3b | Kernels: layer-batched exact GEMMs, one launch for all models, separate CUDA streams for learner and inference | this lane | removes the GPU cap (target about 1,350 games/s per 3050 at net-8) | bit-identical |
| S3c | One long-lived process per host serving several runs; GPU learner forward replaces the CPU tapes; resident Adam on the GPU | this lane, with the throughput lane's formats | about 250 (estimate; CPU-bound with digest features on) | bit-identical; full CPU replay kept |
| S4 | New-net input contract without the digest features (state and action); **ruled 2026-10-31: the W7 big-network line drops them, net-8 and V8 keep them** | W7 nets | about 460 or more (estimate) | new contract identity |
| S5 | Tensorization rewrite: incremental features per changed object, or features built on the GPU from a compact state snapshot | engine/tensorizer work | toward the 855 engine ceiling | exact, tested against the current tensorizer |
| S6 | Bigger nets within the multiply-add budget; the tensor-core tier only with a lineage decision | lead, pilot, this lane | depends on multiply-adds per decision (table above) | exact (FMA order) or deterministic-GPU lineage |

S1 to S3c keep net-8's numbers bit-identical. Every S row's games/s is an estimate until qualified (8.4). S4 is the first stage that changes a model input, and it is also the biggest single CPU saving after the GPU move: the digest features cost more per decision than the engine. The lead ruled on 2026-10-31 that new architectures do not use the digest features (they cost 261 us per decision against the engine's 173, and W4 Step A found no learnable meaning in them: include and exclude rows differ only in hash coordinates, AUC 0.53). The big-network serving path and its projections are designed without them; net-8 and the V8 lineage keep them unchanged.

## 9. PR plan

1. **PR A: exact GPU inference.** `gpu_exact_v1` (NVRTC compile cache, tanh proof, the net-8 fused forward, the batching service with per-request replies and model groups, the per-thread client), the hook in `score_owned`, the collector entry in `ordered_parallel_jobs_v1`, the RT1 execution flags `--gpu-exact-inference ORDINAL [--gpu-exact-latency-us N]` (outside the run identity) and the invocation receipt in `gpu-exact-receipts/`. Tests: tanh, forward, batch composition, service timing, more models than slots, service fault, non-finite outputs, the hook across parameter replacement and the V8 fallback, and GPU-served collection with a distinct opponent. The engine ceiling probe and the GPU microbenchmark ride along (measurement only).
2. **PR B: exact GPU backward** (stacked on A). `net8_backward.cu`, `net8_learner.rs`, `gpu_exact_backward_v1` in the GAE train step and `--gpu-exact-learner ORDINAL`. The CPU keeps the forward tapes, behavior replay, loss head, gauge and Adam. Tests: learner parity on the fixture over two updates and on a random net with edge-case inputs.
3. **Deferred, each its own PR:** S3b kernels, S3c (long-lived multi-run process, GPU tapes, resident Adam), the S4 input contract.
4. **Qualification note** (collab lane `LANES/gpu-infer-20261031/`): section 7.2 items 1 and 1b.

## 10. Review record

Reviewed at mtg-kernel `b3277aaa2` by GPT-6 Astra ultra (`codex exec`, read-only) and a Fable max-effort adversarial reviewer; both PROCEED WITH CHANGES and both found no arithmetic-order defect on the supported paths. Full texts: lane `reviews/`. Dispositions:

| Finding | Disposition |
|---|---|
| Collector blocks forever if the service dies (Astra P1) | Fixed: one reply channel per request, held only by the service; test `a_service_fault_fails_the_waiting_client` |
| More than 10 models in a batch fails every request (both) | Fixed: groups of at most 10 models; test `service_serves_more_models_than_resident_slots` |
| Caller's MXCSR and FMA3 state not checked (Astra) | Fixed: checked per forward in the inference hook and per update in the learner hook |
| Tests too narrow: distinct opponent, random backward, edge cases, fault paths (Astra) | Added the distinct opponent, the random-net edge-case learner case, the fault and non-finite tests; golden and recorded-decision cases, fresh process and cross-device are in 7.2 |
| Projection assumes batch 64; section 6 states targets as measured (both) | Fixed: D1's per-model batch of about 10 (about 58 games/s per 3050 with v1 kernels); measured and target rows separated |
| Digest cost and CPU floor not isolated; host scaling not reconciled with the memo (both) | Relabeled as upper-bound scenario estimates with the memo's 2.3x cross-check; isolating measurements listed as prerequisites |
| PR plan contradicted 2.1 and 3.1 (both) | Fixed (section 9) |
| GPU replay in S3 would drop the only independent check (Fable) | Fixed: full CPU replay kept in every stage (8.2) |
| Receipt on stderr only (Fable) | Fixed: `gpu-exact-receipts/` in the output directory, with the tanh identity and proof keys |
| Hook test across parameter replacement sites; `collection_workers` bound on resume (Fable) | Test `policy_hook_follows_parameter_replacement_and_falls_back_exactly`; 2.1 states the worker count is identity |
| `append_v1` checked offsets after the adds; repeatable latency flag (Fable) | Fixed |
| Process-wide host model cache; separate CUDA streams; split S3 (Fable, optional) | Streams and the split are in S3b/S3c; the host cache is noted for S3c |
