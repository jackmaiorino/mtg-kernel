# Net8 Observation Diagnostics Execution Manifest v1

Status: PREDECLARED, implementation pending, diagnostic only.

This manifest governs the smallest follow-up licensed by the completed
concentrated-opponent exploiter probe. That probe classified both arms
`PARITY-ROBUST` but returned `MIXED-OFF-TARGET-RISE`, so it did not license
either a training-signal or observation-hash hypothesis verdict.

These diagnostics answer two narrower questions:

1. Does the frozen checked-in feature corpus expose a model-input distinction
   that the structured tensors collapse and only the digest tail preserves, or
   any actual representation collision?
2. How much do trained checkpoints' outputs change when the state/action
   digest blocks are reassigned while their marginal values are preserved,
   relative to matched reassignment of the sibling direct-feature blocks?

The diagnostics are CPU-only and record-only. They do not train, alter,
promote, or qualify a model.

## 1. Source and artifact isolation

- Parent result commit:
  `a6259b2d82474a407af98752dbbf802361f0076d`
- Branch:
  `codex/observation-diagnostics-v1`
- Worktree:
  `C:\Users\Jack\IdeaProjects\mtg-kernel-observation-diagnostics-codex`
- Artifact root:
  `D:\mtg-kernel-observation-diagnostics-v1-20260726`
- Dedicated Cargo target:
  `E:\cargo-target-observation-diagnostics-v1`
- GPU use: forbidden.

The final implementation commit, this manifest's SHA-256, executable SHA-256,
input hashes, exact commands, exit codes, wall times, and output hashes must be
captured before interpretation. A dirty worktree, changed input after
preflight, missing positive marker, nonzero exit, panic, non-finite metric, or
timeout is an execution failure rather than a scientific result.

## 2. Representation facts being tested

The frozen Net8 tensor ingress is:

- state: 219 floats, direct features `0..123`, observation digest `123..219`;
- action: 195 floats per legal action, direct features `0..99`, legal-action
  digest `99..195`;
- objects: 98 direct floats plus a card-token embedding lookup;
- edges: 41 direct floats plus source/target node indices;
- action references: 25 direct floats plus action/node indices.

`action_ref_card_ids` is validated transport data but is not consumed by the
Net8 forward. The object/card/edge/group and action-reference pathways are a
third structured bucket and must never be folded into the sibling
state/action direct-feature controls.

Each 96-float digest tail is six SHA-512 blocks over
`namespace || counter_u32le || canonical_json`, with 16 little-endian `u32`
chunks per block mapped to `[-1, 1]` as `f32`. This is a whole-record
fingerprint, not per-field bucket hashing. The relevant collision classes are
distinct canonical records sharing raw digest bytes, sharing the quantized
96-float tail, or sharing the complete model representation.

## 3. Diagnostic A: feature coverage and collision audit

The audit consumes only:

- `python/mtg_kernel_rl/features.py`, unchanged frozen authority;
- `data/flat_policy_v2/python_full_features_v2.json`;
- `data/flat_policy_v2/python_action_features_v2.json`.

It emits canonical JSON binding all three input hashes and its own source hash.
Action atom identities retain their action-variant context. Optional fields
retain separate present/absent witnesses. Canonical equivalences caused by
declared normalization or removal of operational/forbidden data are recorded
separately and are not called collisions.

Required report sections:

- dimensions and digest construction;
- input/source SHA-256 bindings;
- corpus case/action counts;
- per-scope field/atom witness counts and categorical coverage;
- structured signatures and canonical-record identities;
- declared intentional equivalence groups;
- raw-digest, quantized-tail, and complete-representation collision groups;
- an exact categorical status and non-claims.

Predeclared precedence:

1. Source/input mismatch, malformed input, or an unexpected canonical
   equivalence: `INVALID`.
2. Any collision between distinct canonical records at the raw-digest,
   quantized-tail, or complete-representation level:
   `COLLISION-DETECTED`.
3. Any required in-scope model-input atom/category without a witness:
   `COVERAGE-INCOMPLETE`.
4. With no blocker, a pre-existing tactically varying Burn/Rally pair whose
   structured channels are identical and whose digest tail differs:
   `HASH-DEPENDENCE-CANDIDATE`.
5. With no blocker and every audited tactical distinction changing a
   structured channel: `STRUCTURED-DISTINGUISHABLE`.

The checked-in corpus may honestly end at `COVERAGE-INCOMPLETE`. Absence of a
collision proves identity preservation only over this corpus; it does not
prove that a cryptographic fingerprint is learnable or harmless.

## 4. Diagnostic B: trained-checkpoint reliance

### 4.1 Fixed checkpoint pairs

Each row uses generation 0 and the already-selected decisive generation from
the same validated Store. No new checkpoint selection occurs.

| Arm | Seed | Store | Candidate generation |
|---|---:|---|---:|
| mirror-start | 920013 | `D:\mtg-kernel-exploiter-v3b-20260726\runs-arm1\dev0\run-0\store` | 256 |
| mirror-start | 920014 | `D:\mtg-kernel-exploiter-v3b-20260726\runs-arm1\dev0\run-1\store` | 384 |
| mirror-start | 920015 | `D:\mtg-kernel-exploiter-v3b-20260726\runs-arm1\dev0\run-2\store` | 256 |
| diverged-start | 920016 | `D:\mtg-kernel-exploiter-v3b-20260726\runs-arm2\dev0\run-0\store` | 256 |
| diverged-start | 920017 | `D:\mtg-kernel-exploiter-v3b-20260726\runs-arm2\dev0\run-1\store` | 512 |
| diverged-start | 920018 | `D:\mtg-kernel-exploiter-v3b-20260726\runs-arm2\dev0\run-2\store` | 128 |

The loader must validate each complete Store, run record, boundary,
checkpoint, payload, model-parameter hash, feature-contract digest, and
feature-encoding digest before scoring. The report records all identities.

### 4.2 Fixed corpus

The implementation hard-codes deterministic Rally/Rally episode and
environment seeds before any external checkpoint is scored. Uniform legal
action selection is derived deterministically from those seeds. Collection
continues in fixed episode order until exactly 256 post-tensorization
decisions exist.

The corpus must contain at least 128 decisions with more than one legal
action. A canonical SHA-256 over every tensor's name, shape, and raw bits
binds the corpus. The same materialized corpus and perturbation maps are used
for all twelve checkpoint reads.

### 4.3 Interventions

Score every checkpoint under:

1. baseline;
2. state-digest permutation;
3. action-digest permutation;
4. both digest permutations;
5. state-direct permutation;
6. action-direct permutation;
7. both direct permutations;
8. positive-zero both digest blocks;
9. positive-zero both direct blocks.

State donor mapping is the derangement
`donor(i) = (i + 129) mod 256`; copy the entire source block. Action mapping
rotates complete source rows by one position within every multi-action
decision. One-action decisions are unchanged and excluded from policy
metrics. The direct controls use exactly the same donor/rotation maps over
`state[0..123]` and `action[0..99]`.

No intervention may change legal-action count/order, object/card/edge/group
features, action-reference features or indices, or any block not named above.

### 4.4 Metrics

For each checkpoint and condition, over multi-action decisions:

- stable-softmax Jensen-Shannon divergence from baseline, using natural logs;
- centered-logit RMS delta after subtracting each decision's logit mean;
- top-action flip rate, with lowest-index tie breaking;
- change in the baseline top action's probability.

For state-changing conditions, over all decisions:

- value MAE and RMSE;
- value absolute-error p50, p95, and maximum;
- value sign-flip count, with exact zero reported separately.

Report per-checkpoint raw summaries, candidate-minus-generation-zero
contrasts, six-pair distributions, and paired digest-minus-direct sibling
contrasts. Also report, as corroborative rather than functional evidence,
per-parameter-normalized weight RMS, candidate-minus-generation-zero weight
delta RMS, and Adam first/second-moment summaries for the four exclusive
first-layer column groups.

No metric is converted into a causal "percent reliance." Object/card/edge and
action-reference signal remains unperturbed and is named as the third bucket.

### 4.5 Integrity controls

The executable must assert:

- an identity transform scores bit-exactly;
- every permutation followed by its inverse restores all tensor bits;
- sorted source-block bit multisets are identical before and after each
  permutation, every state donor differs, and only declared slices change;
- a complete legal-action-row rotation, including consistently remapped
  action references, rotates logits bit-exactly and leaves the value bit-exact;
- softmax/Jensen-Shannon helpers cover gauge shifts, width one, ties, extreme
  finite logits, and reject non-finite input;
- all outputs and metrics are finite;
- repeated scoring produces the same raw-output digest.

The probe asserts integrity and coverage only, never a desired scientific
outcome. The predeclared wall-clock cap is 120 seconds per checkpoint pair on
the designated host.

## 5. Interpretation

Diagnostic A has the categorical statuses in Section 3. Diagnostic B is
descriptive because the digest and direct sibling blocks are not the whole
representation and their activation distributions differ.

The primary cross-check for B is whether action-digest permutation effects,
and their candidate-minus-generation-zero changes, consistently exceed the
matched action-direct effects across the six fixed pairs. "Consistently"
means the digest-minus-direct contrast has the same strict sign in at least
five of six pairs and its pooled mean has that sign. State/value reads use the
same five-of-six rule and are reported separately. Exact ties or any other
pattern are `MIXED`.

- positive consistency:
  `DIGEST-SIBLING-EFFECT-EXCEEDS`;
- negative consistency:
  `DIRECT-SIBLING-EFFECT-EXCEEDS`;
- otherwise:
  `MIXED`.

This comparison is made independently for Jensen-Shannon divergence,
centered-logit RMS, top-action flip rate, and value RMSE. Disagreement among
metrics is reported as disagreement, not majority-voted into a global label.

A large digest effect or positive consistency is an opaque-code
reliance/memorization warning and can justify a controlled structured-encoder
experiment. It is not proof of hidden-information leakage, a representation
bottleneck, or poor generalization. A small effect does not prove the
structured encoding is sufficient; it routes next to the matched
Pool3/Pool4 gradient-direction and credit-assignment audit.

## 6. Non-claims

No training or game-strength claim; no numerical qualification; no
promotion; no equilibrium claim; no assertion about model capacity; no
observation-leakage verdict; no multi-deck result beyond the static checked-in
Burn/Rally fixtures; no BO3, sideboarding, human, or pro-level-play authority.
