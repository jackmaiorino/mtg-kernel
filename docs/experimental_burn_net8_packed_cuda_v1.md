# Experimental production Net8 packed CUDA diagnostic

Status: diagnostic only. This is not an end-to-end training, games/second, or
CUDA bit-identity claim.

Backend identity:
`mtg-kernel-experimental-burn-net8-packed-cuda-forward-v1`.

## What this closes

- Loads the committed Python-authoritative common-model snapshot through the
  production Rust snapshot loader before timing.
- Imports the exact 33 tensors / 1,230,994 f32 parameters into Burn 0.21's
  Net8 graph. Native/Python linear weights are `[output,input]`; Burn stores
  them `[input,output]`, so the adapter performs an explicit transpose.
- Exports all Burn parameters back into the native ordering and requires raw
  f32-bit equality plus equality of the framed parameter-stream SHA-256.
- Parses the 14 non-synthetic Burn/Rally replay cases in
  `data/flat_policy_v2/python_full_features_v2.json`, evaluates the production
  `NativePolicyValueNetV1` CPU reference, packs repeated real decisions into
  one ragged CUDA graph, and checks every logit/value against that reference.
- Uses a reusable, worst-case-reserved host workspace. Timed packing performs
  no workspace reallocation and creates no per-index scratch vectors.

The CPU/CUDA comparison is tolerance based: `atol=1e-3`, `rtol=1e-3`.
It is explicitly not bit identity. On the run below, maximum absolute error was
`7.3191337e-4`; maximum error divided by its allowed tolerance was `0.7192`.
Zero of 61 unique-case outputs were bit-identical. The raw maximum relative
error (`2.0943`) is dominated by a near-zero reference denominator.

Parameter proof from the run:

- snapshot SHA-256: `33455d0fedc5aea8abd4deeaf37c5480f1832dbea34b9391c9a942d95f040771`
- snapshot payload SHA-256: `79f715b11ccce80ac66cc832bfdc0c963a8a20f27f7b492fdfbb433c008a90a5`
- native/exported parameter-stream SHA-256:
  `36157c71b9fd736d4913e6c5722dcb9c1e4f119b7b28b108bde9d74f18862d54`
- encoded-decision fixture SHA-256:
  `5dbece4f903a09260a499295d866c7e6ff4283f9de83f842224511f977ae8a97`

## Warmed diagnostic result

Command:

```text
target/release/examples/experimental_burn_net8_packed_cuda_v1.exe --warmup 30 --iterations 100
```

Environment: Windows 11 build 26200; rustc 1.94.1; i7-13700K; NVIDIA RTX
4070 SUPER (device 0, driver 596.36, 12,282 MiB); Burn/CUDA 0.21 baseline
without fusion or autotune. Both GPUs were idle immediately before the run.

Rates are decisions/second. `p95 rate` is decisions divided by p95 latency, so
lower is worse. Full lane includes reusable host packing, fresh Burn tensor
creation/H2D, forward/sync, readback, and validation.

| decisions | resident forward mean | forward p50 rate | forward p95 rate | full-lane mean | full p50 rate | full p95 rate |
|---:|---:|---:|---:|---:|---:|---:|
| 16  | 8,856 | 8,512 | 8,031 | 7,660 | 7,570 | 6,869 |
| 64  | 35,800 | 36,824 | 31,403 | 21,285 | 20,204 | 17,367 |
| 128 | 64,639 | 66,893 | 55,332 | 33,895 | 33,856 | 29,615 |
| 256 | 111,295 | 111,698 | 100,585 | 47,116 | 46,080 | 39,402 |
| 512 | 175,911 | 175,776 | 158,279 | 61,116 | 61,928 | 50,212 |

Mean phase split, microseconds:

| decisions | host pack | H2D + sync | forward + sync | readback + validate | full lane |
|---:|---:|---:|---:|---:|---:|
| 16  | 9.5 | 242.9 | 1,512.6 | 323.3 | 2,088.8 |
| 64  | 44.4 | 732.9 | 1,853.0 | 375.9 | 3,006.8 |
| 128 | 125.5 | 1,161.1 | 2,188.7 | 300.2 | 3,776.4 |
| 256 | 231.0 | 2,226.2 | 2,546.4 | 428.6 | 5,433.4 |
| 512 | 523.1 | 4,574.1 | 2,959.8 | 319.1 | 8,377.5 |

In an earlier paired warmed run, removing the temporary index vectors from the
reusable packer reduced the 512-decision host-pack mean from 678.8 us to
423.3 us and the full-lane mean from 8,423.8 us to 7,717.0 us. That is a
separate local diagnostic before/after, not a portable benchmark claim; the
fresh exact-head table above also exposes normal host/GPU scheduling variance.

## Exact next integration seam

Keep a service object alive for the trainer update:

1. Import one `NativePolicyValueTrainStateV1` parameter snapshot before the
   timed update and keep the Burn model device resident.
2. Append each scorer request's validated `NativeEncodedDecisionViewV1` into
   this module's host workspace, preserving canonical episode/group/substep
   order and action offsets.
3. Upload once when the configured inference batch is full, execute one packed
   forward, split logits/values by the saved offsets, and return them to the
   existing association gate.
4. Reuse device allocations at their high-water capacity. The current Burn
   `Tensor::from_data` path clones all host vectors and allocates fresh device
   tensors for each changing batch; at 512 decisions that H2D path costs about
   4.05 ms versus 2.86 ms for resident forward. A persistent staging/service
   layer is therefore the next performance unit.

The module and root runner exist only with the
`experimental-burn-net8-packed-cuda-v1` feature. No store, CLI, seed, schema,
or cross-language benchmark contract is changed.

## Backward / Adam follow-up

The experimental backward, frozen-order gradient export, device Adam,
moment-stream import/export, rollback, resident-batch timing, and their current
numerical-identity blockers are now tracked in
`docs/experimental_burn_net8_cuda_train_v1.md`. This forward-only document is
retained as the original diagnostic baseline; its blocker list is superseded
by that checkpoint.
