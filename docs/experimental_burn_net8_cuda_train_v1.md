# Experimental Net8 CUDA training checkpoint

Status: implemented and release-validated as a diagnostic checkpoint. No CUDA
training backend identity is declared. This is not an end-to-end trainer,
games/second, learning-quality, or CPU/CUDA-equivalence claim.

The candidate diagnostic label is
`mtg-kernel-experimental-burn-net8-cuda-train-feasibility-v1`. It remains a
label, not an identity, because the strict-f32 gauge gate described below is
open.

## Implemented path

The feature-gated runner now executes a complete candidate update:

1. Load the Python-authoritative common-model snapshot.
2. Import all 33 tensors / 1,230,994 parameters plus both Adam moment streams,
   checking every visitor ordinal against an explicit production-field
   ParamId/native-name witness.
3. Pack and upload a ragged decision batch, or reuse a resident device batch.
4. Run a separate Burn/CUDA scorer/loss forward and backward.
5. Export gradients in the frozen native 33-tensor order, including the
   inverse transpose for every linear weight.
6. Run device-resident Adam with the native beta, epsilon, step, and scorer
   gauge-hold constants.
7. Export and validate the complete candidate parameter and two-moment streams
   before the only live-state mutation point.

The candidate snapshot exports 14,771,928 bytes per step. A forced failure
immediately before the mutation point proves bit-exact rollback. A separately
injected direct-logit term corrupts the monitor graph upstream of CUDA
backward, returns the exact `GaugeResidualExceeded` variant, and also leaves
live state bit-exact. Unit tests independently accept the exact comparator
boundary and reject both signs one ULP beyond it.

The batch tensors can remain resident across updates. Candidate model and
moment tensors are still allocated per step; this is not yet a persistent
trainer service.

## Release validation checkpoint

Command:

```text
target/release/examples/experimental_burn_net8_cuda_train_v1.exe \
  --validation-only \
  --forward-determinism-invocations 1000 \
  --backward-adam-determinism-invocations 12
```

Environment manifest:

- NVIDIA GeForce RTX 4070 SUPER, compute capability 8.9
- CUDA driver API 13020; runtime 12080; cuBLAS 120803
- cudarc 0.19.8; CubeCL CUDA 0.10.0; CubeK matmul 0.2.0
- Burn / Burn CUDA 0.21.0
- stock numerical mode: CubeCL fast math enabled and conditional TF32
  conversion registered
- manifest SHA-256:
  `62b3b83e7ecfbf766faec6619aabc09540a77241d790cd6c8406ba7adf12ab41`

Results:

- 1,002 genuinely separate forward invocations: 167 for each combination of
  decisions `{1,4,16}` and ragged rotation `{2,7}`; zero output bit flips.
  Readback is verified in chunks of at most 32 distinct returned tensors, so
  this used six synchronization barriers per configuration without reducing
  the invocation count.
- Two fresh processes / CUDA contexts produced the same batch-16 rotation-7
  digest:
  `6e2b319df2919af8dcd296756acebf304213110a8bd6838c52fdc967ce0eb214`.
- 12 complete snapshot-to-candidate backward/ragged-scatter/Adam invocations:
  two per configuration. Each second invocation compared 4,923,981 f32 values
  across loss/gauge evidence, all gradients, parameters, and both moments;
  zero bit flips.
- Baseline step digests by `(decisions, rotation)`:
  - `(1,2)`:
    `e360c7604c8d278a63bbf9fba87ee4e0d0c827af1eed9d081bec5d226c3a7c0e`
  - `(1,7)`:
    `cd214026f93088937470b7db0a11c3e6eecac3ffaf9a905de222780192dc287e`
  - `(4,2)`:
    `890447a85d0e3ebbbfd01f273e0e0a8787f913efcacc22e61e07e328ccbf81c3`
  - `(4,7)`:
    `3b8c52fca7dc9573cd251d97c5566954dea214181c7b51c1fb1ecc9b982b177a`
  - `(16,2)`:
    `242e8b7a7c7ab6ad96ba8050fcc0c7280f70e3eda30e66879ad987dcb0d51ff9`
  - `(16,7)`:
    `e17f3368494ce23f10206ebcb5c18b4522f608de09aa66be49ae6c0122077ede`
- Patterned parameter/moment import-export was bit-exact; one Adam step,
  candidate validation, precommit rollback, scorer-bias hold, and positive-zero
  scorer-bias moments all passed. Explicit ParamId/native-name witnesses also
  matched all 33 production fields in both import and export traversal.

The implementation defaults to a requested 10,000 forward invocations and 96
backward/Adam invocations. The completed checkpoint above is deliberately
smaller: two >=10k attempts exceeded 15 minutes before emitting a record, so
the >=10k validator gate remains **pending**, not inferred from 1,002 passes.
The new `--validation-only` mode prevents a later timing loop from discarding a
completed authority record.

## Numerical proximity oracles

The production CPU oracle is
`NativePolicyValueTrainStateV1::train_step_v1` over four real fixture
decisions. Maximum absolute errors in the release checkpoint were:

| surface | maximum absolute error | tolerance |
|---|---:|---:|
| loss | 6.0737e-5 | atol 2e-3, rtol 2e-3 |
| gradients | 3.7307e-4 | atol 5e-3, rtol 5e-3 |
| parameters after | 5.4389e-7 | atol 2e-3, rtol 2e-3 |
| first moments after | 3.7307e-5 | atol 2e-3, rtol 2e-3 |
| second moments after | 1.9760e-7 | atol 2e-3, rtol 2e-3 |

Relative error uses `max(abs(reference), 1e-6)` as its denominator. These are
proximity envelopes, never equivalence.

The fixed adversarial set covers adjacent f32 logits near one, signed zero and
subnormals, large positive and negative adjacent values, an exact four-way
tie, and q8-scale adjacency. Its canonical input SHA-256 is
`c7cce151eda6b42c7c716f9078bbd12a189a3b3a2ce5498819df2d06db75b783`.
Against an f64 stable-log-softmax/analytic-gradient reference rounded once to
f32, the 27 outputs had maximum absolute error `1.1920929e-7`, maximum relative
error `1.0850922e-7`, and 18 bit-equal values under `2e-5` absolute/relative
tolerances. This oracle does not execute q8 rounding, Hamilton apportionment,
or sampler selection. The `q8-scale-adjacent` case names a logit magnitude; it
is not a sampler-bucket boundary witness, so the accepted expanded
sampler-boundary oracle gate remains open.

## Gauge and strict-f32 blocker

The raw training scorer-bias gradient was `-2.9802322e-8`; it is diagnostic
only. A separate forward/backward monitor derives policy-gradient coefficients
from the same CUDA logits/values plus the actual actions/returns, then applies
per-ragged-row `row - row[0]` centering. The exact cancellation expression uses
each coefficient twice. Its diagnostic bound therefore uses Higham gamma over
twice the coefficient absolute sum, a conservative basic-f32 operation count,
one minimum-normal FTZ allowance per modeled operation, and outward f32
rounding. There is no fitted safety factor. In the checkpoint the modeled count
was 98, the bound was `1.2135632e-5`, and the observed monitor residual was
exactly zero.

This bound is **not** a formal production gate. The monitor is a separately
constructed centered loss: it exercises a real independent CUDA backward, but
it does not bound the raw scorer-bias gradient produced by the actual training
loss. The training residual is currently recorded, then canonicalized to zero;
the monitor cannot make that canonicalization authoritative. This is the
primary gauge blocker.

A separate strict fork:

- disables CubeCL CUDA fast math;
- unregisters TF32 conversion;
- forces CubeK's naive f32 matmul strategy; and
- dumps the actual NVRTC PTX.

The fork is pinned at local commit
`301e1cfb08ab21d9a50658d5e1e6779d55d5e839`, tree
`3e6b4153d95961c51603f380c9b56e6e007862be`. Its 115-file training PTX stream
contains zero `tf32`, `mma`, or `wmma` matches, but still contains 120
`ex2.approx`, `rcp.approx`, or `.ftz` matches from tanh-related kernels and 14
atomic/reduction matches. The complete monitor backward therefore cannot be
attested as strict elementary f32 merely from the fork flags.

Pinned local artifacts under
`E:\mtg-kernel-strict-fp32-fork-019f6e61`:

- forward source log SHA-256:
  `3d011eb3abd02d114aa297902f80f50b674fc1ff9df029b4919f79da2111f383`
- training source log SHA-256:
  `99cff48d7524323c4397b670ed77a7efffeafaf88bcf1afde8a6735f55cb20bc`
- canonical 22-file forward PTX stream SHA-256:
  `9e1d12edaf01d2f4661c2c5e3a00e9091c84b1a87aa27659a91232b2dc4ca019`
- canonical 115-file training PTX stream SHA-256:
  `3761a056f9775bef5a50f55ec01c7c1e10b6bb32872ba65379002416faff5f77`

A production gate must bound the actual training scorer-bias gradient before
canonicalization and attest every operation on that dependency path. A
scorer-bias-only PTX audit of the synthetic monitor is insufficient.

## Diagnostic candidate-step timing

Command:

```text
target/release/examples/experimental_burn_net8_cuda_train_v1.exe \
  --train-decisions 512 \
  --train-warmup 1 \
  --train-iterations 5 \
  --forward-determinism-invocations 12 \
  --backward-adam-determinism-invocations 6
```

This used a 512-decision batch, one warmup, five timed steps, 12 small
forward-determinism calls, and six single-pass backward configurations. The
batch contained 1,717 actions, 8,669 objects, 803 edges, and 1,389 action refs.
The cold resident diagnostic was discarded; both timed arms were then
re-imported from Adam step 7, followed the same trajectory, and ended at step
13.

| path | mean full candidate step | mean candidate decisions/s |
|---|---:|---:|
| persistent resident batch | 671.28 ms | 762.73 |
| fresh batch upload each step | 678.70 ms | 754.39 |

Resident mean phase split:

| phase | mean |
|---|---:|
| forward/loss + sync | 188.99 ms |
| backward/gauge + sync | 451.59 ms |
| device Adam + sync | 4.31 ms |
| 33 parameters + two moments export/validation | 26.36 ms |

Fresh-upload H2D added 4.75 ms/step. A genuinely fresh process/context first
candidate took 9.986 s: 3.132 s forward, 6.011 s backward, 0.812 s Adam, and
31.41 ms export. Kernel compilation dominates that cold result.

These rates are **candidate physical decisions per second**, not games per
second. They include full candidate export/validation but exclude rollout,
the trainer wrapper, checkpoint publication, and persistence. They cannot be
divided into or compared with XMage games/second without the matched end-to-end
trial.

## Remaining gates

1. Resolve and independently review the numerical identity/gauge strategy.
2. Expand the adversarial oracle through the real q8/apportion/select path with
   pinned near-boundary bucket witnesses.
3. Complete the predeclared >=10k forward proof and a stronger backward/Adam
   repetition run on the final numerical backend.
4. Integrate a persistent scorer/trainer service with the even-batch wrapper
   and prove Gate 3(b) against logits transported from rollout time; the
   recompute must be a separate invocation, not a cached tensor.
5. Replace this one-substep-per-physical-group feasibility loss with the final
   matched even-batch training contract and preserve its episode/seed/order
   semantics.
6. Run the matched end-to-end Rust-versus-XMage trial and report games/second,
   update latency, and learning-quality metrics under backend-labeled records.

No production store, CLI, seed derivation, benchmark record, or model identity
is changed by this checkpoint.
