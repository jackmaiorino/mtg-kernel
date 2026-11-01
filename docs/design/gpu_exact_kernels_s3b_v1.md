# S3b: faster exact GPU kernels (design, 2026-10-31)

Stage S3b of `gpu_batched_inference_v1.md` (section 8.5). The bit-exactness contract stays as it is: every GPU output equals the CPU path's bits, and replies do not depend on batch composition. The S3a tests and the qualification all carry over unchanged. Only the kernels, the batching loop and the transfers change.

## 0. Targets (the lead, 2026-10-31)

1. At net-8, GPU inference and the GPU learner each beat the CPU path in updates per hour on the D1 qualification segment, on both GPUs.
2. At the bignet pilot's S and M sizes, the exact kernels reach the projected rates (design 8.4: 0.84 to 1.08 TMAC/s on the 3050, scaled by FP32 peak elsewhere). Those rates give S 136 to 174 and M 10 to 13 games/s per 3050.
3. A segment-level device-fault run is part of the requalification: the segment fails closed, resumes on the CPU and reaches the CPU leg's state hashes.

**What target 1 can buy.** At net-8 the forward is 8.5% of an update's CPU and the learner step 5.4% (net-size memo). Moving both to the GPU at zero cost would therefore save at most about 14%, so the ceiling at net-8 is about 1.16x. S3b's job at net-8 is to turn today's loss (0.72x for inference) into that gain. The large-net payoff is target 2.

## 1. Where S3a's time goes (receipts, qualification legs on GPU 1)

| Measure | Inference leg | Both leg |
|---|---:|---:|
| Requests | 27,933 | 27,933 |
| Batches (mean batch) | 8,000 (3.5) | 8,153 (3.4) |
| Launches (1.7 per batch: learner and opponent models) | 13,816 | 14,149 |
| GPU service busy time (pack, 5 copies, kernels) | 4.62 s | 4.43 s |
| Per batch | 577 us | 543 us |
| Learner busy (18 updates, 14,227 substeps) | | 1.08 s (60 ms per update) |
| Wall / CPU-s against the CPU leg's 18.7 / 30.4 | 23.3 / 34.7 | 25.7 / 35.0 |

Four causes, each visible in the code:

1. **The kernel is latency-bound.** v1 runs a request on one 256-thread block whose inner loop reads each weight from global memory, with only 8 loads in flight. Single-row stages (the state encoder's 1,499-term chain, the value head and the scorer prefix) keep 64 threads busy and wait on memory at every step. At batch 1 the measured kernel time is 355 us for a decision whose math is about 13 us on one SM.
2. **One launch per model.** Each batch mixes the learner's and the opponent's models, so it pays about 1.7 launches.
3. **Five pageable copies per batch** (descriptors, floats and ints up; logits and values down). Each pays a WDDM round trip, 61 us at 4 KB in the S3a microbenchmark.
4. **The service thread spin-waits.** The CUDA context uses the default scheduling, which spins while it waits for the GPU. That burns one CPU core for all of the service's busy time. It is the CPU-s difference above: about +4.3 CPU-s against 4.6 s busy, net of the CPU forwards saved.

## 2. The kernels

### 2.1 Exact tiled chains (the one primitive)

Every linear output is the CPU's single chain: bias first, then `x[k] * w[k][o]` for ascending k, each product rounded before its add (or `fmaf` for a new net that defines FMA order). S3b keeps that chain per output and changes only where operands come from:

- The block's threads cooperatively stage a K-tile (32 steps of k) of the weights and the input rows into shared memory, with the next tile loading while the current one is consumed (double buffering).
- Each thread owns a register tile of outputs, R rows of one column o, with R independent accumulators. Each accumulator still adds its terms in ascending k, tile after tile, so no output's operation order changes. The tile shape decides only which thread computes an output, never its order. That is why the shapes can be tuned freely, and the tests run two shapes to prove it.
- No split-K, no atomics, no tensor cores, no reductions across threads.

The same device function serves every forward layer, the learner's input-gradient stages (phase A: the chain over o for each input i), and phase B's parameter gradients. Phase B's chain is the visiting order of rows across substeps, and the tiles step along that order.

### 2.2 Forward: one block per request, all models in one launch

- **Batch invariance stays structural.** A request still runs on its own block, reading only its own inputs and its model's parameters.
- **Block shape.** 256 threads, a shared-memory input-row cache and two weight tiles of 32 x 64 (16 KB each). A request's activations stay in a global scratch area that is L1-resident. At D1's shapes, every stage with at least 4 rows keeps all 256 threads busy.
- **Single-row chains** (the state encoder, the scorer prefix, the value head) get 64 threads. Their weights stream through the double-buffered tiles that all 256 threads load, so each step reads shared memory instead of waiting on global memory. Estimated latency for the 1,499-term state chain: about 5 us, against about 40 us in v1.
- **Index-add stages** are unchanged: one thread per destination element, sources in the CPU's order.
- **All models in one launch.** Each descriptor carries its model's slot, and a device table of parameter base pointers (one per resident slot) replaces the per-launch `params` argument. The batch policy's grouping by `max_models` remains only as the residency limit.
- **Estimate.** About 25 us per request at batch 1 (13 us of math on one SM plus about 20 stage barriers). Requests in a batch run in parallel on separate SMs (20 on the 3050, 24 on the 4060, 56 on the 4070 SUPER), so a D1 batch of 3.5 costs about 25 to 30 us of kernel time.

### 2.3 Transfers and waiting

- **One pinned upload and one pinned download per batch.** The descriptors, floats and ints are packed into one page-locked staging buffer with fixed sub-offsets, and the logits and values come back in one contiguous output region. Both copies are asynchronous on the service stream, and one event marks completion.
- **Eager batching.** The service waits for one request, takes every request already queued without waiting further, launches, and on completion replies and loops. While the GPU runs, new requests queue for the next batch. This replaces the 200 us latency window: at low load a request waits for at most one batch, and at high load batches fill naturally up to `max_requests`. Batching never affects bits, so the policy is free to change.
- **The wait.** `CU_CTX_SCHED_BLOCKING_SYNC` makes the service thread sleep on the event instead of spinning, at the cost of a wake-up of a few tens of microseconds on Windows. Spinning costs a core for each batch's 30 to 60 us. The qualification runs both (`--gpu-exact-wait spin|block`, an execution flag like the others) and makes the faster one the default. Neither affects bits.
- **Estimate per batch:** about 10 us up, 25 to 30 us kernel, about 10 us down, so about 50 us per batch of 3.5 requests (14 us per request), against 577 us today and the CPU forward's roughly 50 us.

### 2.4 Learner

- **Forward tape.** The learner reuses the 2.2 kernel over its substeps, still bit-checked against the CPU tapes, which S3c retires.
- **Phase A** (one block per substep) uses the tiled chains for each input-gradient stage. Each gradient element's chain is over o in ascending order, the CPU's order.
- **Phase B.** One output tile per block: a 32 x 32 tile of a weight gradient. The block walks the layer's terms in visiting order, substep by substep and row by row, staging each run of rows' `d_out` and inputs into shared memory, and each thread adds its elements' products in that order. Bias gradients use the same walk, with the scorer's output bias rows reversed as the CPU does. The embedding gradient keeps its occurrence lists. Each step reads shared memory instead of global memory, and every block's threads are busy. S3a's receipt does not split the learner's 60 ms per update by phase; PR D's receipt does.
- **The learner's own stream.** Inference and the learner already run at different times within an update (collection, then the update). A second stream is kept for S3c, where the two overlap across runs.
- **Estimate per update:** about 60 ms today to about 8 ms.

### 2.5 Large nets (target 2)

The bignet S and M dense layers are the same exact chain at larger widths (d 96 and 256, MLP 4d, about 39 tokens per decision with a long tail). The primitive in 2.1, used grid-wide over rows batched across decisions (M = tokens x decisions, a fixed tile per output, no split-K), is the kernel those nets need. Its rows are independent, so it is batch invariant for the same reason as 2.2.

S3b ships it as `mtg_exact_gemm` with a no-FMA order and an FMA order, plus a benchmark at the pilot's S and M layer shapes and its token distribution.

Attention, softmax, normalization and the activation of the rebuilt stack need a CPU reference before their exact order exists. They are designed with that reference (the W7 rebuilt stack), not here. The S and M games/s numbers therefore come from the measured GEMM rate applied to the dense multiply-adds, with attention estimated from its share. At 39 tokens and d 96, attention is about 7% of the multiply-adds.

## 3. Interfaces

No caller changes. `collector_forward_v1`, `gpu_exact_backward_dims_v1`, the RT1 flags and the receipts keep their shape. The receipts gain these fields:

- `kernel_version` (`net8-tiled-v2`);
- the wait mode;
- per-batch upload, kernel and download microseconds (sums);
- `launches` now at most one per batch;
- for the learner, phase A and phase B microseconds.

The v1 kernels stay in the program for one release, behind `--gpu-exact-kernels v1`. That gives the A/B comparison and the fallback a bisect needs, and the next PR after requalification deletes them.

## 4. Tests

- Every S3a device test runs on the v2 kernels unchanged: tanh proof, random nets against the CPU forward, batch composition, the service under any timing, more models than slots, faults, non-finite outputs, the policy hook, collection parity, and learner parity over 3 updates and 939 substeps.
- **Tile-shape invariance:** the program compiled with two register-tile shapes (R = 4 and R = 2) and two K-tile sizes gives identical bits on the random nets and the edge-case fixture.
- **All models in one launch:** a batch of 12 requests over 10 models, with results equal to per-model CPU forwards. An eviction between batches does not change bits.
- **Eager batching:** the timing-perturbation service test also runs with a delayed device (an injected sleep in the test hook), so batches of 1 to `max_requests` all occur.
- **`mtg_exact_gemm`:** bits equal to a sequential CPU reference (no-FMA and `f32::mul_add` orders) at net-8, S and M shapes, ragged M from 1 to 4,096, plus row permutation invariance.
- **Fault injection for the segment run:** a feature `gpu-exact-fault-injection-v1`, not in release builds, makes the Nth request fail.

## 5. Qualification (done criteria)

- The 7 S3a legs plus GPU 0 on the v2 binary, through `tools/qualify.py`: every `after_state_sha256` equal to the CPU leg, and outputs equal apart from timers.
- Updates/hour: CPU, inference, learner and both, at least 3 runs each, inside the same host_slots claim shape on each GPU. GPU 1 runs on Jack's PC; GPU 0 runs when the pilot frees it, otherwise on HaleysPC's 4060 (the same architecture) as a stand-in. **Pass:** inference and learner each beat the CPU mean beyond the run-to-run spread.
- **Fault leg:** fault at request N; the invocation exits with an error and leaves no completed iteration past the last good one. A CPU resume finishes the segment, and every state hash equals the CPU leg's.
- **Large-net microbenchmark:** `mtg_exact_gemm` at the S and M shapes, TMAC/s on the 3050 and the 4060 (and the 4070 SUPER if free), compared with design 8.4's rates. The projected games/s rows are updated from measured numbers.

## 6. PR plan

1. **PR C: inference.** The tiled forward kernel, one launch for all models, packed pinned transfers, eager batching, the wait mode, `--gpu-exact-kernels`, and the tests above for the forward and service.
2. **PR D: learner and `mtg_exact_gemm`.** Phase A and B on the tiled chains, the GEMM kernel with its tests and benchmark, and the fault-injection feature.
3. **Requalification note** in the lane, then a cleanup PR that removes the v1 kernels.

## 7. Risks

- **Register pressure** from R accumulators per thread at 256 threads. Bounded by R = 4 and checked by the compile log (`-Xptxas -v` equivalent through NVRTC).
- **The CPU hop.** A request still costs a channel send and a blocking receive on the collector side, a few microseconds of OS wake-up each way. If the qualification shows that this eats the net-8 margin, the fallback is to spin briefly on the reply before blocking. That is a collector-side choice and does not affect bits.
- **net-8's ceiling is small** (section 0). A pass at net-8 is a gain of a few percent to about 15%; the measured margin may sit near the run-to-run spread. In that case the receipt reports the CPU-s per update saved, which has a smaller spread than wall time, alongside updates/hour.
