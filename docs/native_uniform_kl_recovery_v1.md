# Native uniform-KL recovery API

Opt in through `loss_selection` with `kind: gae_uniform_kl_recovery_v1`,
`gamma: 1.0`, `lambda: 0.9`, `initial_adam_step: 32400`, `recovery: true`
and `diagnostics: true`. The ordinary matched arm sets only `recovery: false`.
Only CPU sequential backward, learning rate 1e-4 and value coefficient .5
are admitted. Collection, whole-update GAE and normalization are unchanged.

Update t is `(Adam step before update + 1) - initial_adam_step`, in 1..64.
The binary32 schedule is .01 for t1..32 and `(.01 * (64-t) as f32) / 32.0`
for t33..64. The ordinary arm and t64 produce positive zero. Every substep
receives full weight and the denominator remains physical group count N.

The added scalar is beta/N times the sum of max-centered
`log(sum(exp(z-max))) - mean(z-max) - log(K)` evaluated in binary64.
The analytic logit derivative uses the existing binary32 trainer
`exp(log_softmax(z))`, giving `(beta/N) * (p-1/K)`. This is a finite-batch
surrogate on the existing quantized collection distribution. It does not
claim an unbiased on-policy gradient. Singleton terms are exactly zero.
Beta zero skips loss and canonical gradient additions. Old configurations
retain their existing serialized shape. New checkpoints add `exploration`
with the selection and use loss identity `gae_uniform_kl_recovery/v1`.
State hashes still cover the same full parameters and Adam state.
Continuation after initial Adam32400 requires the identical exploration
selection, including arm and diagnostics flag. Checkpoint scalar bits and
Adam age must agree with its exploration metadata.

`update.json.exploration` records update index, beta bits, raw regularizer
sum, N, objective and optional diagnostics. Updates1,32,64 perform exactly
two extra backward passes, value-only and beta-weighted regularizer-only,
using the retained canonical forward tape. The actor component is the
binary64 residual combined minus value minus regularizer. It includes
binary32 backward accumulation differences and is explicitly not an exact
independent actor backward. Receipts report block norms/dots, cancellation
ratio, a bound on the final binary64 subtraction only, and a null bound for
the unmeasured binary32 component-separation error. Canonical gradients and
Adam updates are unaffected by enabling diagnostics. Shared encoder, actor
head and value head blocks include realized Adam-step and current-weight
norms. Tanh receipts count each retained activation once and report abs>=.99
plus bounded-bin distributions of binary32 `1-h*h`.

The gauge extension compares the declared auxiliary binary32 arithmetic
with a max-centered binary64 reference, records observed rounding and a
count-derived accumulation bound, and checks that supplied combined
gradients equal the declared addition. It never sets a blanket tolerance
from the final residual. The canonical scalar scorer bias remains anchored.
The recovery receipt includes the raw and high-precision gauge residuals,
derived bound, before/after anchor bits and pass indicators.

Forward-only probes use expanded command `mode: score_saved_states` with
`source`, `probes: [{probe_id, trajectory: {path,sha256}, row_index}]`,
`membership_sha256`, and a fresh `output_directory`. At most3072 probes are
admitted. One pinned trajectory is decoded at a time. No game or sampler is
run. The result and `probes.json` have schema `native-saved-state-probe/v1`,
complete/source/membership/state SHA/Adam step/forward_calls and ordered
rows containing the input identifiers, logits_bits,
trainer_log_probabilities_bits, trainer_probabilities_bits and value_bits.
The caller validates frozen membership and runtime pins; the command
validates source, trajectory hashes, unique IDs, row bounds and feature
generation. Saved outcomes and selected actions do not influence scoring.

Verification scope: scalar derivative and menu cases, schedule boundaries,
declared-rounding gauge, parameter finite differences, physical-group
weighting, complete beta-zero state identity, and diagnostics on/off state
identity. Release checks use the pinned guarded build helper. Runtime
qualification and fixed-probe corpus execution belong to the lead.

Observed first check: commit1035f550 passed five exploration and three
existing GAE release tests under Rust1.94.1 and pinned MSVC linker
ee9b29be652eee20affa6963a7ce54d01271b0f1b2443e315ea86469cbb95694.
Receipt: D:/e-scratch/exploration-execution-20261006/loss-checks-001/completion.json.
Total1218.961s,227050351 logical bytes across cache/output, zero formal games
and zero paid compute. The following checkpoint/gauge receipt repair adds
one affected metadata-admission test and requires checks at its own commit.
