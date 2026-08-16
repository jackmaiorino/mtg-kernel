# Model-guided-searcher deterministic-CPU-forward audit, v1

Status: COMPLETE. Audit only. This report authorizes no compute, no pool
seat, no pilot, and no pinning change. It proposes pinning changes where a
property is CONDITIONAL or VIOLATED; those changes are not implemented here
and require their own reviewed diff, per the design's own rule for
implementation item 3.

Authority: `CLAUDE-MODEL-GUIDED-SEARCHER-DESIGN-V1.md`, Section 1.5 ("the
model forward must be deterministic on CPU") and Section 5.3 item 3
("Deterministic-CPU-forward audit and any resulting pinning changes ...
Buildable now; reviewed with elevated scrutiny given the determinism stakes,
in the spirit of Amendment 1"). This item is listed under Section 4.2 as
buildable independent of PR-98, the searcher rebase, and v1's calibration
results, and this audit was executed on that basis.

Executed in worktree `mtg-kernel-mgs-audit-fable`, branch
`fable/mgs-forward-determinism-audit-v1`, at `origin/main` `1aab6d4`.
Toolchain: `rustc 1.94.1 (e408947bf 2026-03-25)`, LLVM 21.1.8, host
`x86_64-pc-windows-msvc`, pinned by `rust-toolchain.toml`.

This audit was reviewed adversarially by an independent model (Codex,
GPT-5.6 Sol, xhigh reasoning) before being finalized. Several findings below
are corrections that review produced against the first draft; each is
attributed explicitly where it applies, and the two most checkable claims
Codex made (a sibling worktree's file/line citation, and an existing
bit-exact cross-implementation test's exact location) were independently
re-verified in this environment and confirmed accurate before being
incorporated. Codex's own specific numeric measurements (a UCRT-vs-glibc
comparison corpus, a UCRT `tanhf` disassembly) were not independently
reproduced here (no Linux host and no disassembler-based verification was
performed by this audit itself) and are reported as attributed, not
independently confirmed, evidence.

## 1. Target architecture and checkpoint

The design's target is `kernel-policy-value-net-8` (`Net8`, the narrow
architecture), the live schema the countersigned v1 searcher's own audit
scope binds to. The probe checkpoint is the de-novo screen store, generation
256: `D:\mtg-kernel-denovo-screen-v1\denovo-screen-build\attempt-002\denovo-store\run-0\store`,
`checkpoints/update-00000256.checkpoint.json`. That manifest's
`train_state.parameter_element_count` is `1230994`, which matches
`native_policy_value_net_v1::PARAMETER_COUNT_V1` (the narrow net's frozen
parameter count) exactly, confirming this checkpoint is the narrow
`kernel-policy-value-net-8`, not the wide capacity-experiment sibling
(`kernel-policy-value-net-8w128`, a distinct architecture with a distinct
parameter count). `native_science_loop_v1.rs:189` constructs its model via
`NativePolicyValueNetV1::runner_fixed_v1(...)`, confirming the science loop
also targets the narrow net by default.

## 2. Enumerated forward implementations

Four distinct pieces of Rust code compute forward passes over `Net8`-shaped
parameters. Each is listed with what calls it and whether it is on the
model-guided searcher's leaf-evaluation path (Section 1.3, sites 2 and 3).

### 2.1 `NativePolicyValueNetV1::forward_v1`: the production inference path

`mtg-kernel/src/native_policy_value_net_v1.rs:541-546`, delegating to
`forward_with_action_ingress_capture_v1` (`:548-694`). Single-threaded,
purely scalar `f32` arithmetic; every loop is a fixed `for _ in 0..n` over a
`Vec`/slice index, never a `HashMap`/`HashSet` iteration (grepped the whole
file: no match). The two primitives every sub-network reuses:

- `linear_rows_v1` (`:1034-1050`): for each row, for each output index
  (ascending), initializes the accumulator to the bias and accumulates
  `value += input[i] * weight[i]` for `input_index` in ascending order,
  `0..input_dim`. Fixed, published, sequential operation order.
- `tanh_in_place_v1` (`:1052-1056`): calls `f32::tanh()` (Rust std), applied
  once per hidden unit.

This is the path reached by `NativeCheckpointInferenceV1::score_decision_v1`
(`native_checkpoint_inference_v1.rs:234-241`, via
`score_decision_with_scratch_v1` at `:250-265`, calling
`self.model.forward_v1(...)` at `:261`) and its batch sibling
`NativeCheckpointBatchScorerV1::score_batch_v2`
(`native_checkpoint_inference_v1.rs:404-421`, via `score_batch_checked_v1`
starting `:335`), which is itself a plain sequential loop, one
`forward_v1` call per decision, no worker threads. This checkpoint-inference
boundary is the same one v1's own dispatch pattern already uses for
"checkpoint opponents" (`checkpoint opponents use the scoring view
unchanged`), so it is the implementation the model-guided searcher's three
leaf-evaluation call sites (Section 1.3) will reach once the search core is
wired (implementation item 5, not built by this document).

Other confirmed callers of this same implementation: the checkpoint-runner /
shadow-scorer stdio tool
(`native_checkpoint_shadow_stdio_v1.rs:653`,
`self.state.model_v1().forward_v1(encoded)`), and the trainer's
policy-anchor recompute/consistency check
(`native_trainer_v1.rs:3771`, `anchor.model.forward_v1(encoded)`, whose
result feeds a `RecomputedOutputMismatch` check against a value recorded
earlier in the same run: the trainer already depends on this
implementation's determinism for its own consistency gate to pass, which is
corroborating, not audit, evidence).

### 2.2 `NativePolicyValueNetWideV1::forward_wide_v1` / `forward_into_wide_v1`

Same file, `:1398` / `:1550`. A distinct architecture
(`kernel-policy-value-net-8w128`, hidden dim 128, its own
`W_PARAMETER_COUNT_V1`), used for a separate capacity experiment, NOT the
generation-256 de-novo checkpoint (Section 1 above). Its two-layer and
row-pooling helpers (`apply_two_layer_tanh_rows_wide_v1`,
`add_indexed_rows_wide_v1`, `:1790-1812`) call the exact same
`linear_rows_v1` / `tanh_in_place_v1` primitives as the narrow net: the
"wide" naming is dimensioning only, not a separate arithmetic
implementation. Listed for completeness of the architecture family; every
finding below about `linear_rows_v1` and `tanh_in_place_v1` applies to it
identically, but it is out of scope for the gen-256 checkpoint this audit
targets.

### 2.3 `NativePolicyPackedForwardBuilderV1::forward_v1`: the training-tape path

`native_policy_train_step_v1.rs:2677-2690`
(`NativePolicyPackedForwardBuilderV1::forward_v1`), backed by
`two_layer_forward_into_arena_v1` / `linear_forward_into_arena_v1`
(`:3339-3465`) and `tanh_span_in_place_v1` (`:3335-3337`, which calls the
same `tanh_in_place` / `f32::tanh()`). This is a SEPARATE, independently
written reimplementation of the same math (an arena-allocated, tape-recording
forward needed to run backward/Adam afterward), not a call into 2.1. Reading
`linear_forward_into_arena_v1`'s inner loop (`:3451-3459`: for each row, for
each `(output_index, bias_value)` in ascending order, accumulate
`value += input[...] * weight[...]` for `input_index` in ascending
`0..input_dim`) shows the identical row-major, ascending-index,
bias-then-accumulate structure as `linear_rows_v1` in 2.1. No fast-math, no
threading, no vectorization intrinsics in this function either.

This implementation is used for training-batch gradient computation, called
both directly (`native_trainer_v1.rs:~978`, `self.forward_builder.forward_v1`,
result consumed via `tape.logits_v1()`/`tape.value_v1()`) and through a
four-plus-worker thread pool, `NativePolicyForwardPoolV1`
(`native_trainer_v1.rs:773-880`). The pool's own doc comment states the
determinism-relevant design choice explicitly: "Result publication stays on
the broker thread, reassembled by input ordinal before any caller-visible
slice or association is changed" (`:778-779`): each worker computes one
decision's forward pass to completion on a single thread (no shared
mutable float state between decisions), and results are reassembled by
input index, not completion order. This is a real, load-bearing concurrent-
forward-evaluation pattern already living in this codebase, and it is
structurally the right shape for property (c): concurrency is across
independent decisions, never within one decision's reduction.

**This implementation is not on the model-guided searcher's leaf-evaluation
path.** It exists to compute gradients over already-collected training
batches, not to score search leaves against a frozen checkpoint. It is
enumerated here because the audit task asks for every distinct forward
implementation for the target architecture, and because its concurrent
worker-pool precedent is directly relevant to property (c)'s general
argument.

**Correction from adversarial review:** the first draft of this report
claimed implementations 2.1 and 2.3 were "not empirically cross-checked
bit-for-bit... out of scope." Codex's review caught that this overclaimed
the gap. An existing test,
`native_policy_train_step_v1.rs:5240-5248`, already does exactly this
bit-exact cross-check for two fixture cases named `zero_edges_zero_action_refs`
and, notably, `ordered_edges_and_action_refs` (i.e. including real object/
edge/action-reference topology, not only the empty-object shape): it calls
both `model.forward_v1(encoded(case))` (implementation 2.1) and
`builder.forward_v1(encoded(case))` (implementation 2.3) on the identical
encoded input and asserts `tape.logits_v1() == expected.logits` and
`tape.value_v1().to_bits() == expected.value.to_bits()`: bit-exact, not
tolerance-based. This is real, existing, positive evidence that the two
implementations agree exactly on at least these two fixture shapes; it is a
synthetic-fixture, single-point-in-time equality check (not a repeat-run,
multi-process, or multi-thread determinism test, and not run against the
real gen-256 checkpoint), so it does not fully substitute for what this
audit's own probe measures, but "not empirically cross-checked" was too
strong a claim and is withdrawn.

### 2.4 `experimental_burn_net8_packed_v1`: the burn/CUDA forward backend

`mtg-kernel/src/experimental_burn_net8_packed_v1.rs` (and
`.../training.rs`, `.../bridge.rs`), gated behind the
`experimental-burn-net8-packed-cuda-v1` Cargo feature, off by default
(`Cargo.toml`: `dep:burn`, `dep:burn-cuda`, `dep:cudarc`, all `optional =
true`). Its own module comment: "Experimental production-parameter forward
backend. This is deliberately opt-in and is not a matched-training or
evidence identity." CI only compiles and exercises it under
`--features experimental-burn-net8-packed-cuda-v1` on a hosted runner with
no CUDA device (`.github/workflows/ci.yml:92-94`), i.e. its CPU backend
path. Its own test module additionally records
(`training.rs:3376,3460`) that the stock CUDA backend runs under
"fast-math and conditional TF32," which the module's own authors already
treat as precluding a "strict-fp32 attestation": i.e., this backend is
already self-documented as NOT meeting Section 1.5's properties 2-3 on the
CUDA side, by design, because it is out of scope for any evidence claim.

This backend is not wired to the checkpoint-inference boundary, the
checkpoint runner, or the science loop; nothing in this design proposes
routing search leaves through it. It is enumerated for completeness under
property (e) and excluded from the pass/fail verdicts below because it is
neither on the searcher's call path nor claimed deterministic by its own
authors.

### 2.5 Considered and excluded: `native_flat_cpu_reference_v1::cpu_forward`

`native_flat_cpu_reference_v1.rs:426`. Explicitly a different, synthetic,
fixed-shape architecture (`STATE_DIM=2048`, `ACTION_DIM=128`,
`PARAMETER_COUNT=156_097`) for "the synthetic CUDA capacity diagnostic," per
its own module doc: "not the Flat Policy model ... not a production
trainer API." No external callers (grepped). Not `Net8`, not the checkpoint
architecture, excluded from this audit's scope.

## 3. Determinism risk enumeration, with citations

### 3.1 Reduction order (Section 1.5 property 1): HOLDS

`linear_rows_v1` (`native_policy_value_net_v1.rs:1034-1050`) and its
training-tape twin `linear_forward_into_arena_v1`
(`native_policy_train_step_v1.rs:3419-3465`) both accumulate in one fixed,
published, ascending-index order, entirely on one thread per call, with no
`HashMap`/`HashSet` iteration anywhere in the forward file (grepped). No
parallel or work-stealing reduction exists in either.

### 3.2 No fast-math (Section 1.5 property 2): HOLDS for the audited build

Neither `Cargo.toml` (root or `mtg-kernel/`) nor any `.cargo/config.toml`
(none exists in this repo) sets `RUSTFLAGS`, `-C target-feature`, or any
fast-math-equivalent flag. `rustc` has no stable global fast-math switch
(the unstable `-Z fast-math` requires nightly; this repo is pinned to
stable `1.94.1` by `rust-toolchain.toml`). CI (`.github/workflows/ci.yml`)
runs `cargo test --release --locked --workspace --all-targets` identically
on `ubuntu-24.04` and `windows-2025`, with no per-OS flag divergence.

One qualification raised in adversarial review and accepted: stable `rustc`
does still expose an escape hatch, `-C llvm-args=-fp-contract=fast` (passing
raw arguments straight to LLVM), which is not blocked by "no nightly
`-Z fast-math`." Nothing in this repository's build configuration uses it
today (verified: no `RUSTFLAGS`, `.cargo/config.toml`, or `llvm-args` usage
anywhere in the tree), so the property HOLDS for the actual, current,
audited build; it is not a language-level impossibility the way 3.4 below
turns out to be.

### 3.3 FMA policy (Section 1.5 property 3): HOLDS for this crate's own arithmetic; VIOLATED for the complete forward call graph

**Revised after adversarial review; this corrects the first draft's
reasoning, not just its conclusion.** The first draft argued property 3
held only because `rustc --print cfg` shows no `+fma` target feature on the
default build (`cmpxchg16b, fxsr, sse, sse2, sse3` only, confirmed
empirically on this host/toolchain): i.e., an absence-of-capability
argument. Codex's review corrected this: Rust's language semantics are the
real reason, and they are stronger than target-feature absence. Ordinary
Rust `a * b + c` (exactly `linear_rows_v1`'s inner statement,
`value += input_row[input_index] * weight_row[input_index];`) is specified
as two separate, strict IEEE-754 operations; LLVM does not have blanket
permission to contract them into a fused multiply-add just because the
target supports the FMA instruction, unlike C's default `FP_CONTRACT ON`
behavior. Rust deliberately provides `f32::mul_add` as the *only* way to
request a fused operation, specified separately from `*`/`+` for exactly
this reason. Codex reports having confirmed this by compiling
`linear_rows_v1`'s pattern with AVX2/FMA target features enabled and
observing separate `vmulss`/`vaddss` instructions, not a fused `vfmadd`,
consistent with Rust's own primitive-arithmetic documentation and the
separate `mul_add` specification (not independently re-disassembled in this
audit's own session; cited as Codex's finding). Under this reading, a future
`RUSTFLAGS=-C target-cpu=native`/`-C target-feature=+fma` change would
**not** silently fuse `linear_rows_v1`'s reduction the way the first draft
warned: the earlier draft's specific `target-cpu=native` warning for this
property is withdrawn. **For the crate's own handwritten arithmetic, this
property HOLDS on a language-semantics basis, not merely a target-feature-
absence basis** (my own `rustc --print cfg` evidence stands as
corroborating, not primary, support).

That does not clear the complete forward call graph. `tanh_in_place_v1` and
`tanh_span_in_place_v1` both call `f32::tanh()`, which (Section 3.6) is not
Rust code at all: it is an opaque call into the platform's C math library
(`tanhf`, confirmed by import-table inspection, Section 3.6). Whatever FMA
policy that external, prebuilt library uses internally is entirely outside
Rust's contraction guarantees and outside this repository's control. Codex
reports (attributed, not independently reproduced here) disassembling the
installed Windows UCRT (`ucrtbase.dll`/`api-ms-win-crt-math-l1-1-0.dll`,
version `10.0.26100.8875`) and finding `tanhf` contains multiple
runtime-selected code bodies including distinct SSE and AVX/FMA
implementations, chosen by an undocumented process-local ISA-detection
path, and reports that forcing the alternate selector in an isolated
subprocess produced 38 differing output bits across a 500,000-sample finite
`f32` sweep. If accurate, this means the *actual executed* FMA policy of
`tanh_in_place_v1`'s output is not fixed even on a single Windows host
across different physical CPUs, independent of anything this crate's Cargo
profile or target-feature configuration controls. **Verdict for this
property is therefore split: HOLDS for `linear_rows_v1`/
`linear_forward_into_arena_v1`; VIOLATED for the forward pass as a whole
once `tanh` is included**, and the property is scored VIOLATED at the
whole-call-graph granularity Section 1.5 actually asks about ("the forward
pass must either always use FMA or never use it").

### 3.4 Auto-vectorization (Section 1.5 property 4): HOLDS

**Revised after adversarial review.** The first draft scored this
CONDITIONAL on the same target-feature-absence argument as the original 3.3
(no AVX, so only narrow SSE lanes are even available). Codex's review
correctly points out this framing understates the guarantee: SSE2 already
provides 128-bit vector registers on the default baseline, so "lack of AVX"
was never the operative reason auto-vectorization can't change this
reduction's result. The real reason is the same language-level one as 3.3:
without `reassoc`/fast-math permission on the `fadd` instructions (absent
here per 3.2), LLVM's loop vectorizer will not vectorize a floating-point
reduction in a way that changes summation order: this is not a target-
width question, it is a semantics-permission question, and it holds
regardless of whether the target has SSE2-only or full AVX-512 vector
width. `linear_rows_v1` (`:1034-1050`) and the indexed-pooling accumulation
`add_indexed_rows_v1` (`:1058-1068`) are both simple, loop-carried,
non-reassociation-permitted reductions, so both HOLD on this basis.

### 3.5 Subnormal flush-mode pinning, FTZ/DAZ (Section 1.5 property 5): VIOLATED

**Revised after adversarial review, upgraded from CONDITIONAL to VIOLATED.**
Grepped the entire `mtg-kernel/src` tree for `MXCSR`, `mxcsr`, `FTZ`, `DAZ`,
`flush.zero`, `_mm_setcsr`, `_mm_getcsr`, `set_rounding`: zero matches
outside this audit's own new probe file and the burn/CUDA training module's
comments about PTX-side flush behavior on the GPU path (out of scope,
Section 2.4). No code anywhere in this repository reads, verifies, or pins
the FTZ/DAZ state before the forward pass runs.

The first draft scored this CONDITIONAL, reasoning that the probe measured
a clean state (FTZ=0, DAZ=0) both before and after every forward call, so
the property "holds empirically." Codex's review is correct that this
conflates two different claims: Section 1.5 property 5 requires the state
be *verified* "at the point the forward pass runs": an action the
production code must perform: not merely that the ambient state *happens*
to be clean when an auditor's probe checks it from outside. No production
code path performs that verification anywhere in this repository. A probe
observing clean ambient conditions today provides no defense against a
future in-process CUDA-context initialization or third-party library
leaving FTZ/DAZ dirty on a specific thread (the exact hazard the design's
own text names: "a CUDA context, a different numerical library"), because
nothing would notice. Today's default CPU-only build never links
`cudarc`/`burn-cuda` (both optional, off-by-default features), so this
specific contamination vector is not live in the default configuration, but
that is a mitigating fact about today's feature flags, not a satisfied
property. Verdict: VIOLATED, with the empirical MXCSR results (Section 4)
recorded as evidence of the current ambient state, not as satisfaction of
the property.

### 3.6 Transcendental function (`tanh`) portability: VIOLATED (folded into 3.3's whole-call-graph FMA verdict; also the direct driver of 6's cross-host gap)

`tanh_in_place_v1` (`native_policy_value_net_v1.rs:1052-1056`) and
`tanh_span_in_place_v1` (`native_policy_train_step_v1.rs:3335-3337`) both
call `f32::tanh()`, Rust's standard-library method. This was verified
empirically on the compiled test binary from this exact worktree, not just
asserted from documented behavior:

```
dumpbin /imports target\release\deps\mtg_kernel-<hash>.exe
...
    api-ms-win-crt-math-l1-1-0.dll
                          E6 logf
                          B2 exp
                         11B tanhf
                          FE pow
```

`tanhf` is imported from the Universal CRT math API set on Windows, which
forwards to `ucrtbase.dll`'s implementation at runtime; this directly
confirms `tanh_in_place_v1`'s `f32::tanh()` call resolves to system UCRT
code, not vendored Rust. **Narrowing note from adversarial review:** the
import table also lists `logf`, `exp`, and `pow`, but the first draft
implied these were part of the forward pass's own call graph without
tracing them there; that was not established. This whole-binary import
list only proves the *test binary* links those symbols somewhere (likely
elsewhere in this large crate/its dependencies), not that `forward_v1`
calls them. Only `tanhf`, traced directly to `tanh_in_place_v1`/
`tanh_span_in_place_v1` by source reading, is claimed as part of the
forward pass here.

On the `ubuntu-24.04` CI host, the equivalent binary links against glibc's
`libm` `tanhf` instead: a different, independently implemented library.
IEEE 754 requires correctly-rounded results only for `+ - * / sqrt` (and a
handful of others under IEEE 754-2019), explicitly not for `tanh`; different
libm implementations can legitimately disagree by one or more ULP on the
same input, and are not required by any standard to agree.

Codex's review reports (attributed; not independently reproduced in this
audit's own session: no Linux host was available here) a supplemental
UCRT-vs-glibc-2.35 comparison over 2,000,000 deterministic finite `f32` bit
patterns, finding 54,773 mismatches (54,358 at 1 ULP, 415 at 2 ULP). Codex's
own caveat, preserved here: that synthetic corpus is not representative of
real network activation values, so a ~2.7% mismatch rate on it must not be
read as a "2.7% of model forward calls will diverge" estimate: it is a
concrete, non-hypothetical existence proof that UCRT and glibc `tanhf`
diverge on real inputs, at a nontrivial rate, not a claim about this
specific checkpoint's activation distribution. Codex also notes the glibc
side used was 2.35, not the exact glibc shipped by the `ubuntu-24.04` CI
image, so this is evidence of the general hazard class, not a proof this
exact CI pair has already diverged.

**Correction, adversarial review:** the first draft additionally speculated
that glibc's x86_64 multiarch tree IFUNC-dispatches *scalar* `tanhf` across
multiple CPU-feature-selected implementations, which would add a same-
binary, different-runner-CPU divergence risk on the Ubuntu side alone. Codex
checked glibc 2.39's source tree and reports this is wrong for the scalar
path: `sysdeps/ieee754/flt-32/s_tanhf.c` is an ordinary, non-dispatched
implementation; the `x86_64/fpu/multiarch` tree's IFUNC-dispatched variants
found there are for `double`-precision `tanh` and vector-ABI (SIMD-batch)
entry points, not the scalar `tanhf` this crate calls. **This specific
glibc-IFUNC claim from the first draft is withdrawn** as factually
unsupported for glibc 2.39. The cross-host UCRT-vs-glibc divergence risk
(previous paragraph) stands on its own without it.

Verdict: this is the single largest concrete divergence risk found by this
audit. It is folded into property 3's whole-call-graph verdict (VIOLATED)
and is the direct mechanism behind property 6's verdict (VIOLATED): CI runs
the same test suite independently on both hosts but never compares output
bits across them (`.github/workflows/ci.yml:60-94`), and the one existing
forward-pass golden test that does compare against an external oracle,
`cpu_forward_reproduces_torch_authority_goldens_with_declared_tolerance`
(`native_policy_value_net_v1.rs:2173-2202`), is explicitly tolerance-based
(`assert_close(..., absolute_tolerance, relative_tolerance)`), not a
bit-exact comparison: confirmed by direct reading of that test. Nothing in
this repository's CI or test suite would catch a `tanhf` divergence between
the two build hosts today.

### 3.7 Adjacent, forward-looking finding: the not-yet-built PUCT prior renormalization step will need this same fix

Out of this audit's own file scope (a different implementation item, in a
sibling worktree, `mtg-kernel-mgs-quant-fable`), but worth recording because
it bears directly on whether the fixes proposed in Section 6 need to cover
more than `tanh`. That worktree's `model_guided_search_prior_quantization_v1.rs`
(verified to exist at the cited path, content re-read directly in this
audit) implements Section 1.2's apportionment/quantization arithmetic and
explicitly documents its own scope boundary: "This module does **not**
evaluate the checkpoint's policy head, does not perform the legal-action
masking... Those remain the search-loop wiring's job (design item 5)"
(`:30-34`), and states its own input assumption: "this module's input is
already per-action weight, not a logit requiring softmax" (`:97-98`).
Section 1.2 of the design describes one pipeline: "masked... renormalized...
and quantized": without drawing this exact module boundary, which means
the not-yet-built item-5 search-loop wiring is where the "renormalized"
step (converting the forward pass's raw logits, confirmed as `logits:
Vec<f32>` in `NativePolicyValueOutputV1`/`NativeCheckpointInferenceOutputV1`,
into the probability weight this quantization module expects) will actually
be implemented. If that renormalization is a softmax, it will need its own
`exp()` call, and: per Section 3.6 above: an unpinned `exp()` reintroduces
the identical opaque-system-libm hazard this audit found for `tanh`, in a
module that does not exist yet to audit. This is a heads-up for whoever
implements item 5, not a finding against any code that currently exists.

## 4. Empirical probe results

Probe: `mtg-kernel/src/native_forward_determinism_probe_v1.rs`
(`#[cfg(test)]`, declared in `lib.rs`, five `#[ignore]`d tests). Loads the
real de-novo generation-256 checkpoint via
`native_ladder_pool_resolution_v1::stage_ladder_checkpoint_ref_v1` +
`resolve_ladder_checkpoint_ref_v1` (the same run.json / checkpoint-manifest
/ state-payload chain-of-custody the population-ladder opponent dispatch
uses), then calls `NativeCheckpointInferenceV1::score_decision_v1` (Section
2.1's implementation) over a fixed four-decision battery (all zero-object,
`Pass`-only actions with varied global scalar fields and action counts:
see the battery's own doc comment for why non-`Pass` action kinds and
nonzero object/relation counts were tried and rejected by the tensorizer's
own validation, which is a fact about the tensorizer's contract, not about
forward-pass determinism), hashing every `f32::to_bits()` output (never a
float comparison) into one SHA-256 digest per battery pass. All four real
tests (a probe self-selection test is not counted) ran green on the first
fully-passing battery revision; no divergence was observed in anything this
probe measured.

| Property | Test | Result |
|---|---|---|
| (a) repeated calls, one process, 1000x | `forward_is_byte_identical_across_1000_repeated_calls_in_one_process_v1` | PASS. Battery hash identical across all 1000 repeats: `2ff45fc99c0d496aab11c8fef96aa9861d683ba289fcf4e22bf912565134f66f` |
| (b) across 3 separate process invocations | `forward_is_byte_identical_across_three_process_invocations_v1` | PASS. Parent-process hash `2ff45fc99c0d496aab11c8fef96aa9861d683ba289fcf4e22bf912565134f66f`; all 3 subprocess invocations of the same compiled test binary (spawned via `std::env::current_exe()` with `--exact --ignored`) matched it exactly |
| (c) 4 concurrent threads, shared model, 250 passes each | `forward_is_byte_identical_under_four_concurrent_threads_v1` | PASS. All 4 threads' hashes, and every one of their 250 passes, equal the single-threaded reference hash `2ff45fc99c0d496aab11c8fef96aa9861d683ba289fcf4e22bf912565134f66f`: identical to (a)'s hash, as expected since it is the same battery against the same checkpoint |
| (d) MXCSR FTZ/DAZ/rounding-control before/after every forward call | `mxcsr_ftz_daz_and_rounding_mode_around_forward_calls_v1` | PASS. FTZ=false, DAZ=false, rounding-control=0 (nearest-even), unchanged before/after every call in the battery; raw MXCSR `0x00001fa0` throughout |

**Scope note on (d), corrected after adversarial review:** the first draft
of this table said the clean MXCSR state was confirmed "on every measured
thread and process." That overstated the evidence: MXCSR was read only
inside the dedicated MXCSR test above, which runs single-threaded, in one
process, against the battery sequentially. It was *not* independently
re-read inside the 1000x-repeat, three-process, or four-thread tests. The
four-thread and three-process tests corroborate hash-level (i.e.
arithmetic-output-level) stability under those conditions, which is
consistent with stable FTZ/DAZ/rounding-control across threads/processes,
but this probe did not directly instrument MXCSR inside those specific
test bodies. This is a real, if narrow, residual gap in this audit's own
empirical coverage, not resolved by this report.

This probe's own hash results (a)-(c) demonstrate byte-identical repeat-run
behavior on this one host, for the real gen-256 checkpoint, under every
condition it tested. They cannot and do not demonstrate cross-host
(Windows-vs-Ubuntu) bit identity: no Linux host was available in this
environment: which is exactly the gap Section 3.6/property 6 name as
unresolved by construction, not by this probe's omission.

## 5. Verdict per Section 1.5 property

| # | Property | Verdict |
|---|---|---|
| 1 | Fixed, published operation order | HOLDS |
| 2 | No fast-math | HOLDS for the audited build (a stable-Rust `-C llvm-args` escape hatch exists but is unused anywhere in this repository) |
| 3 | Pinned, explicit FMA policy | HOLDS for this crate's own handwritten arithmetic (`linear_rows_v1` / `linear_forward_into_arena_v1`), on a Rust-language-semantics basis, not merely target-feature absence: **VIOLATED for the complete forward call graph**, because the externally linked `tanhf` (Section 3.6) has an FMA policy this repository does not control and Codex's (attributed, unreproduced) disassembly reports it varies by CPU at runtime |
| 4 | No auto-vectorization-dependent horizontal reduction | HOLDS, on the same Rust/LLVM reassociation-permission basis as property 2/3, independent of target vector width |
| 5 | Subnormal flush-mode pinning (FTZ/DAZ) | VIOLATED: no production code path verifies or pins FTZ/DAZ anywhere in this repository; this audit's probe confirms the *ambient* state is clean today, which is not the same as the property's own required verification existing |
| 6 | Single build target per host, cross-host bit-identity as gate not guarantee | VIOLATED: CI runs the identical unflagged build/test command on both hosts (necessary evidence per the design's own standard), but nothing cross-host-compares actual output bits, the one existing forward-pass golden against an external oracle is explicitly tolerance-based not bit-exact, and property 3.6/3.3's `tanhf` finding is a live, concrete divergence mechanism this gap does not catch |

Two properties are flatly VIOLATED (5, 6), a third is VIOLATED at the
call-graph scope that matters operationally even though the crate's own
code is clean (3). This is a materially sharper conclusion than this
report's first draft reached, produced by adversarial review; it is the
correct one. **Overall verdict: CPU-forward byte-determinism, as Section
1.5 defines it, is not currently established for this codebase.** What
this audit's own probe *did* establish, cleanly, is same-host,
same-checkpoint, same-binary repeat-run determinism (properties (a)-(c) of
the task's six, all green): a necessary but not sufficient condition, and
exactly the "empirical run-twice check that happens to agree today" the
design's own text warns is not a substitute for the structural properties.

## 6. Proposed minimal pinning changes (not implemented; each is its own reviewed diff)

1. **Vendor a pinned `tanh` implementation.** Replace `f32::tanh()` in
   `tanh_in_place_v1` (and the training-tape twin `tanh_span_in_place_v1`)
   with a fixed-source, pure-Rust polynomial/rational approximation checked
   into the repository (or the `libm` crate already present transitively in
   `Cargo.lock`, pinned as a direct dependency and version-locked), with a
   golden bit-exact regression test comparing old-vs-new output so the
   swap's own behavior change is reviewed explicitly rather than silent.
   This is the highest-priority fix: it is the only finding with a
   concrete, reported (if not independently reproduced) demonstration of
   actual divergence between two real implementations. If and when Section
   1.2's PUCT prior renormalization (Section 3.7) is implemented, whatever
   `exp()`/softmax it uses needs the identical treatment before it ships,
   not after a second audit rediscovers the same class of gap.
2. **Pin the FMA/target-feature contract explicitly for the record, even
   though the crate's own arithmetic is language-guaranteed safe.** Add an
   explicit, reviewed `target-feature`/`RUSTFLAGS` pin (matching today's
   default: no `+fma`, no `+avx`) plus a build-time assertion (mirroring
   the existing `bench_fast_sampler.rs` pattern that already rejects
   `RUSTFLAGS`/`CARGO_*_RUSTFLAGS` environment overrides for that one
   benchmark) scoped to the searcher/checkpoint-runner binaries, and
   explicitly forbid `-C llvm-args=-fp-contract=fast` or any other
   contraction-enabling flag by the same mechanism. This closes the
   escape-hatch risk named in 3.2/3.3 even though today's default build
   does not exercise it.
3. **Verify and pin FTZ/DAZ at forward-pass entry.** Add an explicit MXCSR
   read (and, if ever found dirty, a set, or a fail-closed error) at the
   entry to `NativeCheckpointInferenceV1::score_decision_v1` /
   `score_batch_v2`, so a future in-process CUDA-context or third-party-
   library initialization that dirties FTZ/DAZ is caught instead of
   silently changing results. This turns property 5 from VIOLATED into
   actually HOLDS, not merely "ambient-clean today."
4. **Build a real cross-host bit-identity gate.** Neither host comparing
   its own output to itself (today's CI) nor a tolerance-based golden
   against Torch (today's only external-oracle test) satisfies property 6.
   The minimal version: commit a fixed input battery and its expected
   output digest (SHA-256 over `to_bits()`, exactly this audit's probe
   technique) as a checked-in golden, and assert both the Windows and
   Ubuntu CI legs reproduce that exact digest. This is the concrete,
   buildable version of Section 1.5's "cross-host bit-identity as the
   gate."
5. **Wire Section 1.4's "forward-determinism build identity" field**
   (already specified by the design, not yet implemented) so every
   registered authority record commits to a build/target-cpu digest,
   closing the gap items 2-3 above describe operationally at the
   authority-record level, not only at the CI level.

None of these five are implemented by this document. Each requires its own
reviewed diff per the design's Section 5.3 item 3 instruction and the era's
standing adversarial-countersign rule.

## 7. Can implementation item 5 (the core search loop) proceed on the assumption of CPU-forward byte-determinism?

**No, not on an unconditional assumption.** Two of Section 1.5's six
properties are VIOLATED outright (5, 6) and a third is VIOLATED at the
scope that actually matters for the executed forward pass (3, via `tanh`).
This audit's own probe cleanly established same-host, same-binary,
same-checkpoint repeat-run determinism (properties a-c): real, useful
evidence, but explicitly the class of evidence Section 1.5's own text says
is necessary and not sufficient.

What this does and does not block, concretely:

- Structural, non-forward-dependent parts of item 5 (tree bookkeeping, the
  UCB/PUCT integer core, tie-break rules, the array/Vec-indexed node
  storage) do not depend on this audit's findings and are not blocked by
  them.
- Scaffolding or testing item 5 against a deterministic mock/stub
  evaluator (not the real forward pass) is not blocked.
- Wiring item 5 to the *real* `NativeCheckpointInferenceV1::score_decision_v1`
  forward pass, and any determinism-dependent gate downstream of it
  (repeat-run trees, the forward-determinism repeat-run gate Section 1.5
  itself specifies, cross-host CI parity for the searcher), should treat
  Section 1.5's determinism requirement as **not yet satisfied** until at
  minimum pinning change 1 (tanh) and pinning change 3 (FTZ/DAZ
  verification) from Section 6 land and are re-verified, and ideally
  change 4 (an actual cross-host bit-identity CI gate) exists so the claim
  is continuously checked rather than audited once.
- This is consistent with the design's own stop rule: "Stop the
  model-guided algorithm... if repeat runs of the same checkpoint, seed,
  and decision produce different selected actions or different quantized
  priors or values": this audit did not observe that on the one host
  available to it, but has not been able to rule it out cross-host, and
  has found a concrete, named mechanism (`tanh`) by which it could occur.
