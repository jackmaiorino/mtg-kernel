# S3b: faster exact GPU kernels (design rev 2, 2026-10-31)

Stage S3b of `gpu_batched_inference_v1.md` (section 8.5). The contract is unchanged: every GPU output equals the CPU path's bits, and replies do not depend on batch composition. The S3a tests and qualification carry over; only the kernels, the batching loop and the transfers change. Rev 2 answers the Fable max design review (section 9).

## 0. Targets (the lead, 2026-10-31) and what each can show

1. **net-8.** GPU inference and the GPU learner each beat the CPU path on the D1 qualification segment, on both GPUs, by the frozen rule in section 5.
2. **Large nets.** The exact GEMM primitive reaches the rates the 8.4 projections assumed (0.84 TMAC/s no-FMA, 1.08 TMAC/s FMA on the 3050) at the bignet pilot's S and M shapes, in both the forward regime and the parameter-gradient regime. S3b measures TMAC/s; the S and M games/s rows stay projections (8.4's caveats: 420 forward-equivalents per game, backward at forward rates, attention, softmax and normalization not built).
3. **Fault.** Device-side faults in inference and in the learner each fail the segment closed; a CPU resume reproduces every state hash (section 5).

**The net-8 ceiling.** By the memo's shares (of a 1.41 CPU-s update; the qualification recipe runs about 1.67), the forward is about 8.5% of an update's CPU and the learner step about 5.4%. Moving both at zero cost saves at most about 14%, about 1.16x (approximate). The learner alone is worth about 3%. S3b's job at net-8 is to turn today's inference loss (0.72x) into a gain; the large-net payoff is target 2.

## 1. Where S3a's time goes (receipts, qualification legs on GPU 1)

| Measure | Inference leg | Both leg |
|---|---:|---:|
| Requests | 27,933 | 27,933 |
| Batches (mean batch) | 8,000 (3.5) | 8,153 (3.4) |
| Launches (learner and opponent models: 1.7 per batch) | 13,816 | 14,149 |
| Service busy time (pack, 5 copies, kernels) | 4.62 s | 4.43 s |
| Per batch | 577 us | 543 us |
| Learner busy (18 updates, 14,227 substeps) | | 1.08 s (60 ms per update) |

Four causes:

1. **The kernel is latency-bound.** v1 runs a request on one 256-thread block whose inner loop reads every weight from global memory with 8 loads in flight. Single-row stages (the state encoder's 1,499-term chain, the scorer prefix and the value head) keep 64 threads busy and wait on memory at every step. At batch 1 the kernel takes 355 us for a decision whose math is about 13 us on one SM.
2. **One launch per model**, so about 1.7 launches per batch.
3. **Five pageable copies per batch.** Non-kernel time is about 220 us per batch (S3a's full round trip at 4 KB is 61 us).
4. **The service thread spins** while the GPU works (default context scheduling), burning a core for the service's busy time: about +4.3 CPU-s over the leg, net of the CPU forwards saved.

(The S3a qualification legs ran on different core placements, so their wall-time comparison is indicative only; section 5 fixes that.)

## 2. The kernels

### 2.1 Exact tiled chains

Every linear output is the CPU's single chain: bias (or the scorer's state prefix) first, then `x[k] * w[k][o]` for ascending k across the concatenated segments, each product rounded before its add (`fmaf` for a new net whose CPU reference defines FMA order). S3b changes only where operands come from:

- The block stages K-tiles of the weights and of the input rows in shared memory; each thread fetches the next tile into registers while the current one is used (double buffering).
- Each thread owns a column o and up to `rows / 4` rows of it, one accumulator per row. Each accumulator starts at its init and adds ascending k, tile after tile, segment after segment. No split-K, atomics, tensor cores or cross-thread reductions.
- The tile shape decides which thread computes an output and where its operands wait, never the order of its operations. The tests compile two shapes and require identical bits.

### 2.2 Forward: one block per request, all models in one launch

- **Batch invariance stays structural.** A block reads only its request's inputs and its model's parameters.
- **Block.** 256 threads; 48 KB of dynamic shared memory: two weight tiles of 64 x 64 (16 KB each) and two input tiles of 32 rows x 64 (8 KB each). Inputs are K-tiled like the weights, so any row width fits. Activations stay in the global scratch (the learner's tape layout, unchanged).
- **Single-row chains** (state encoder, scorer prefix, value head) run on 64 threads while all 256 load the weight tiles.
- **Index-adds.** Thread (g, j) owns column j of the destinations d with `d % 4 == g` and walks the sources once in the CPU's order (sources, then targets), accumulating in shared memory. Each destination element therefore still adds its sources in the CPU's order from zero. A stage with more destinations than shared memory holds (more than 192) uses the v1 loop.
- **Output layers** (the scorer's and the value head's single column) stage their rows and weight column in shared memory; thread r runs row r's chain.
- **All models in one launch.** The descriptor names its model slot. The slots' parameter base pointers (at most `max_models` = 10) are a kernel parameter, not a device buffer. Models with equal dims have identical layouts (`gpu_exact_parameters_v1` derives the layout from the dims); the host checks that before launching and passes one layout. There is one launch per group of at most `max_models` distinct models, so one per batch unless a batch mixes more than 10 models.
- **Estimate:** about 25 us per request at batch 1. A D1 batch of 3.5 requests on separate SMs takes about 25 to 30 us.

### 2.3 Transfers, waiting and the async rules

- **One pinned upload, one pinned download per batch.** Descriptors, float bits and ints go up in one write-combined page-locked buffer; logits then values come down into one cached page-locked buffer. Both copies and the launch are on the service stream.
- **The async rules.**
  - Each batch ends with a stream synchronize before its replies are read. Packing the next batch therefore never overwrites a buffer a copy is still reading, and a resident model is dropped (on eviction) only after the last batch that used it has synchronized.
  - cudarc's frees are stream-ordered on the same stream.
  - A device error (launch, copy or synchronize) fails that whole batch and **poisons the service**: every later request fails at once, with no retry. A context error is sticky, and the run must stop and resume. A host-side validation error fails only its request, as in S3a.
- **Eager batching.** The service waits for one request, takes every queued request (those that arrived during the previous batch), launches and replies. A latency window stays available (`--gpu-exact-latency-us`, default 0 now). Batching never changes bits. The receipt records a batch-size histogram.
- **The wait mode** (`--gpu-exact-wait spin|block`) is the primary context's `CU_CTX_SCHED_BLOCKING_SYNC` flag. It is process-wide per device, so it also governs the learner's waits when both use that device. It is chosen once by a short pre-qualification probe (section 5), frozen in this note and in the receipts, and the qualification runs the frozen value only.
- **Estimate per batch:** about 10 us up, 25 to 30 us of kernel, about 10 us down, so about 50 us per batch of 3.5 (S3a: 577 us).

### 2.4 Learner (after PR C's measurement)

PR C changes the learner's wait (the same context flag) but not its kernels. Its phase A/B rewrite is worth at most about 3% at net-8, and S3c retires the tapes it checks. It is therefore built only if the learner leg is NO-PASS after PR C. That would be PR E: phase A on the tiled chains, and phase B as output tiles walking the visiting order, filling the SMs with all layers' tiles in one grid. Otherwise the large-net learner is built on `mtg_exact_gemm` (2.5) in W7.

### 2.5 `mtg_exact_gemm` (target 2)

A grid-wide exact GEMM over rows batched across decisions (M = tokens x decisions), with a fixed output tile, no split-K, and a no-FMA order and an FMA order. Each output's chain depends only on its row and the weights, so it is batch invariant.

**Prototype measurements** (driver-API harness `tools/gemm_tuned.py`; RTX 3050; every sampled output equal to a sequential reference; 64 x 64 tiles, 4 x 4 per thread, register-prefetched K-tiles, 128-bit shared reads):

| Shapes (rows = 39 tokens x 256 decisions) | no-FMA TMAC/s | FMA TMAC/s |
|---|---|---|
| M forward: d 256 to 768, 256, 1,024; 1,024 to 256 | 0.89 to 0.92 | 1.04 to 1.07 |
| S forward: d 96 to 288, 96, 384; 384 to 96 | 0.60 to 0.85 | 0.70 to 0.97 |
| Parameter gradient, reduction over all rows in one chain | 0.10 to 0.92 | 0.11 to 1.24 |
| Parameter gradient, blocked order (chunks of 512 rows, partials added in chunk order) | 0.48 to 1.02 (S and M, except below) | 0.61 to 1.30 |
| Same, shapes with a 1,024-wide side | 0.11 to 0.13 | 0.13 to 0.16 |

What this says:

- **M's forward already meets the assumed rates.** S's narrow layers (d 96) do not yet; PR D tunes for them (a 32-wide tile, more rows per block).
- **A parameter gradient cannot meet them in one chain.** A layer has only as many independent chains as parameters, each as long as the batch's rows. A new net's CPU reference should therefore define its gradient as a blocked sum (fixed chunks, partials in chunk order), which is exact on both sides and parallel. That is a W7 reference decision; net-8's legacy order cannot change.
- **The 1,024-wide shapes** behave like a power-of-two pitch problem (the same 64 x 64 kernel varies 3x between runs at 16,384 x 1,024 x 1,024). PR D pads leading dimensions and re-measures.
- **Attention** is about 7% of S's multiply-adds at 39 tokens. That is not its time share: its reductions have small K, its softmax needs an exp with a tanh-style proof, and softmax and normalization are memory-bound. Those kernels are designed with the W7 CPU reference.

## 3. Interfaces and receipts

No caller changes. `collector_forward_v1`, `gpu_exact_backward_dims_v1` and the receipt schema keep their shape. The RT1 binary gains `--gpu-exact-kernels v1|v2` (default v2) and `--gpu-exact-wait spin|block`. The inference receipt adds:

- `kernels`;
- `wait`;
- `max_latency_us`;
- a batch-size histogram;
- `launches`, now one per group.

The learner receipt adds `wait`. The v1 kernels stay one release as the A/B baseline, and the cleanup PR after requalification deletes them.

## 4. Tests

- Every S3a device test passes on v2: tanh proof, random nets against the CPU forward, batch composition, the service under any timing, more models than slots, faults, non-finite outputs, the policy hook, collection parity and learner parity.
- **Bits on both kernels and two tile shapes** (64 x 32-row and 32 x 16-row tiles): 2,048 random decisions over 4 nets each, plus 48 large decisions per shape (up to 260 objects, 400 edges, 230 actions and 300 refs). The large ones exercise row chunking and the index-add fallback.
- **One launch, ten models:** batches of 2 to 400 requests over 10 models equal each request's batch-of-one reply.
- **Poisoning:** after an injected device fault, every later request fails, and the client sees the error.
- **`mtg_exact_gemm`** (PR D): bits equal to sequential CPU references in both orders and both gradient orders, at net-8, S and M shapes and ragged M from 1 to 4,096, plus row-permutation invariance.
- **Race and memory checks:** `compute-sanitizer --tool racecheck` and `--tool memcheck` over the v2 forward test and the `mtg_exact_gemm` test, once per PR on GPU 1, receipt in the lane. Double-buffered tiles are the one defect class a bit test can pass by luck.

## 5. Qualification (frozen before any run)

- **Placement.** Every leg runs in one named host_slots claim on Jack's PC: logical processors 22 and 23 (E-cores, this lane's share), 10 collection workers, GPU 1. Legs are interleaved, never batched by backend. Each run records the other claims, the GPU 0 load and the binary hash. A HaleysPC 4060 leg compares only to a HaleysPC CPU leg in the same claim and is labeled a stand-in, never GPU 0. GPU 0 legs run only in a window the bignet pilot grants, with their own CPU legs in that window.
- **Wait-mode probe (before qualification, not part of it).** 3 interleaved pairs each of spin and block on the inference leg. The faster mean becomes the frozen default and is written here before qualification starts.
- **Parity.** The 7 S3a legs on the v2 binary: every `after_state_sha256` equal to the CPU leg, and outputs equal apart from timers. Cross-device: the both leg on GPU 0 in a granted window.
- **Throughput rule.** For each comparison, at least 5 pairs (CPU, GPU), with the order alternating between pairs.
  - Inference: the primary metric is paired updates per hour.
  - Learner: the primary metric is paired CPU-s per update, since its expected gain (about 3%) is below wall time's spread. Updates per hour is reported too.
  - Both: paired updates per hour, which must also be at least the better single leg's mean.
  - **Pass:** the mean paired difference is in the gain direction and exceeds 2 standard errors of the paired differences. A miss is NO-PASS, reported as such. No metric switch after the fact.
- **Fault legs** (a separate binary built with feature `gpu-exact-fault-injection-v1`, pinned by hash next to the qualification binary; the feature's trigger is the only difference). The trigger `MTG_KERNEL_GPU_EXACT_FAULT=inference:N` or `learner:U` makes the Nth forward launch, or the Uth learner update's launch, execute `__trap()`: a real sticky device fault.
  - Leg (a): inference fault. Leg (b): learner fault.
  - Each passes if: the exit code is non-zero; there is no completed iteration or checkpoint past the last good update; the fault is in the receipt; a CPU resume reproduces every state hash of the CPU leg; and for (b), the learner receipt shows `cpu_fallbacks: 0`, which proves the fault was not taken as a resource fallback.
  - The resource fallback has its own labeled test (S3a's fallback path, with an injected allocation failure).
- **Large-net microbenchmark.** `mtg_exact_gemm` at the S and M forward and parameter-gradient shapes on GPU 1, and on the 4060 and GPU 0 when available. Pass for target 2: at least 0.84 TMAC/s (no-FMA) and 1.08 TMAC/s (FMA) at every S and M forward shape and every blocked-order gradient shape, on the 3050.

## 6. PR plan

1. **PR C (inference):**
   - the tiled forward and one launch for all models;
   - pinned transfers, eager batching, service poisoning, the wait mode and `--gpu-exact-kernels`;
   - the tests above, the sanitizer receipt, and the fault-injection feature (inference and learner triggers).
2. **PR D:** `mtg_exact_gemm` with its tests, the S-width and pitch tuning, and the benchmark.
3. **PR E (only on a learner NO-PASS):** the phase A/B kernels.
4. **Requalification note** in the lane, then a cleanup PR that removes v1.

## 7. Risks

- **Register pressure** (8 accumulators and 24 prefetch registers per thread at 256 threads). Watched in the compile log; the second tile shape is lighter.
- **The CPU hop** (a channel send and a blocking receive per request). If it eats the inference margin, the fallback is a short spin on the reply before blocking. That is collector-side and does not affect bits.
- **net-8's small ceiling** (section 0). The frozen rule in section 5 may return NO-PASS for the learner even when the GPU path is faster. That outcome is reported, and PR E is then judged on cost.

## 8. Not covered

The W7 rebuilt stack's attention, softmax, normalization and exp kernels, and their CPU reference; S3c's GPU tapes, resident Adam and cross-run batching.

## 9. Review record

| Review | Verdict | Disposition |
|---|---|---|
| Fable max, design rev 1 (`80ed613d7`), standing in for Astra ultra until the Nov 4 reset | CHANGES REQUIRED, items 1 to 8; arithmetic scheme sound | 1: frozen pass rule (section 5). 2: one named claim, interleaved legs, co-tenants recorded, the 4060 labeled a stand-in. 3: separate wait probe, frozen; flag scope stated. 4: device-side faults for inference and the learner, pass criteria, fallback test, separate pinned binary. 5: async and eviction rules, poisoning, pointers as a kernel parameter. 6: target 2 is TMAC/s at forward and gradient shapes, games/s are projections. 7: launch count per group, layout equality checked. 8: sanitizer racecheck and memcheck. Optional items: tile sizes corrected, the round-trip figure corrected, the ceiling marked approximate, a criterion for the both leg, a batch histogram; the learner rewrite moved to a conditional PR E |
