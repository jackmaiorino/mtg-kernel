# Net8 Action-Ingress Admission Manifest v1

Status: PREDECLARED; no-training diagnostic only.

This manifest defines the smallest no-training screen licensed by
`OBSERVATION_DIAGNOSTICS_RESULT_V1.md`. It does not authorize a model
promotion, a production encoding change, or the three-arm training
micro-rung.

## 1. Parent authority and question

- Parent result commit:
  `ebd13031ea14526547849272ffbd7526fa2087fd`
- Parent result SHA-256:
  `a728aafcba53f42b9d78f7f5db468c5fe0dc87c325168cacec94926aa9ff63f3`
- Prior valid execution producer:
  `c1cf5f1de05b64a4cae35c61862adc725df46837`
- Prior classification producer:
  `ad2689ef2953a7871e5e496a98044ecc00a54938`
- Prior fixed Rally corpus identity:
  `rally-mirror-splitmix64-modulo-fixed-256-post-tensorization-v1`
- Prior fixed Rally corpus SHA-256:
  `72103ea367a662f76675a044ad4efcf4c52bf86d32630df88e5247cf79f5e5e0`

The parent result establishes that both imported run-generation-zero models
and all six measured candidates are much more sensitive to action-digest
reassignment than to matched direct-action reassignment. The imported
generation-zero models were already trained before those runs began.

This screen asks:

1. Can the known combat-Boolean structured aliases be repaired without
   widening the 195-float action tensor or changing any parameter shape?
2. Does that repaired structured representation distinguish every action in
   the checked action corpus and the fixed runtime corpus with the action
   digest zeroed?
3. Is action-digest dominance already present in the genuinely fresh frozen
   common-model snapshot, or only in the two imported trained starts?

## 2. Frozen authorities that must not change

The screen is an additive counterfactual adapter. It must not edit or relabel
the following frozen authorities:

- `python/mtg_kernel_rl/features.py`
  - SHA-256:
    `fce419176dbd15e2b911e5c5f688bb390e731e3817da142571f38b1a7cc778eb`
- `python/mtg_kernel_rl/model.py`
  - SHA-256:
    `2e3e830d4212b8c8f8085861b2508c49a6d7192b9621cef087dd396e22d12c59`
- `data/common_model_snapshot_v1/manifest.json`
  - SHA-256:
    `d5d296f5d4ee1f7e40a6005f1e1dd328b2885f6b95f0c6968c6bf1b87351c7cc`
- `data/common_model_snapshot_v1/parameters.f32le`
  - SHA-256:
    `79f715b11ccce80ac66cc832bfdc0c963a8a20f27f7b492fdfbb433c008a90a5`
- common-snapshot identity:
  `mtg-kernel-python-authoritative-common-model-snapshot-v1`
- common-snapshot named-parameter stream SHA-256:
  `36157c71b9fd736d4913e6c5722dcb9c1e4f119b7b28b108bde9d74f18862d54`
- common-snapshot model-initialization seed:
  `6443515232517447393`
- `data/flat_policy_v2/python_action_features_v2.json`
  - SHA-256:
    `6fab4b246b052e6b8404520d945b630e24ce60323a8fa4c6e78fdc17d3f9a3b8`

The common snapshot binds the exact `features.py` and `model.py` source
hashes. Editing either would invalidate the raw-initialization authority.

The dimensions remain:

- direct action: `[0,99)`;
- action digest: `[99,195)`;
- action-reference pooled input: appended after action column 194;
- `action_encoder.0.weight`: `[64,259]`;
- parameter count: `1,230,994`.

State features, state digest, objects, card embeddings, edges, groups, action
references, the value pathway, and all model parameters are out of scope for
mutation.

## 3. Counterfactual structured repair

The adapter starts from the frozen encoded decision and clones only
`action_features`.

For action-feature coordinate 69:

- `choose_effect_boolean`: retain the frozen encoded `value`;
- `choose_attacker_inclusion`: write `1.0f32` when `include=true`, otherwise
  exact positive zero;
- `choose_blocker_inclusion`: write `1.0f32` when `include=true`, otherwise
  exact positive zero;
- every other action kind: retain the frozen bit pattern.

The action-kind one-hot makes this a kind-conditioned generic Boolean
coordinate. The transform identity is:

`net8-action-kind-conditioned-boolean-slot69-counterfactual-v1`

No other direct coordinate, digest coordinate, canonical JSON value,
action-reference tensor, or model parameter may change.

This is a diagnostic candidate for a future versioned encoding. It is not a
silent correction to existing Net8 artifacts.

## 4. Fixed digest gates

After the optional structured repair, the adapter applies exactly one fixed
mode to each action row:

- `FULL`: copy columns `[99,195)` bit-exactly; do not multiply by one;
- `ZERO`: fill columns `[99,195)` with exact positive-zero `f32` bits;
- `SCALED(bits)`: decode one explicitly supplied finite, nonnegative `f32`
  bit pattern and perform exactly one binary32 multiplication per digest
  coordinate.

Negative, NaN, infinity, malformed, missing, or unbound scale bits fail
closed. A trainable gate is forbidden.

The gate identity is:

`net8-fixed-action-digest-gate-f32-v1`

Only `FULL` and `ZERO` are scientific reads in this screen. `SCALED` exists
for fail-closed and seam tests and cannot be selected post hoc.

## 5. Static checked-corpus admission

The static authority is the unchanged 115-case action golden plus one
supplemental `choose_effect_target` player-target case constructed through
the frozen classifier and encoder. That case must witness both currently
unwitnessed action atoms:

- `legal_action.semantic.<action_kind=choose_effect_target>.target.<target_kind=player>.target_kind`;
- `legal_action.semantic.<action_kind=choose_effect_target>.target.<target_kind=player>.player`.

The supplemental case, its canonical JSON, all tensors, and its digest must
be emitted into the diagnostic report; it does not replace the frozen v2
golden.

Static admission requires all of the following:

1. action model-input coverage is exactly `202/202`;
2. action Boolean-polarity and optional-presence gaps are empty;
3. no distinct-canonical raw-digest, quantized-tail, or complete-
   representation collision exists;
4. after the slot-69 repair, no distinct-canonical structured alias exists;
5. attacker false/true and blocker false/true pairs differ in their direct
   prefixes at coordinate 69 and nowhere else;
6. those four cases retain their original canonical JSON, raw digest,
   quantized digest tail, action references, and all non-69 direct bits;
7. frozen encoding with no repair remains byte-identical to the checked
   authority.

Observation coverage is not an admission criterion and remains
`COVERAGE-INCOMPLETE`.

## 6. Runtime-corpus admission

The runtime screen rebuilds the unchanged fixed 256-decision Rally/Rally
corpus and requires its tensor digest to equal
`72103ea367a662f76675a044ad4efcf4c52bf86d32630df88e5247cf79f5e5e0`.

For every decision:

1. canonical legal-action semantics are pairwise distinct;
2. after the structured repair and `ZERO` gate, every pair remains
   distinguishable by the complete non-digest inputs consumed for its action
   path;
3. every inclusion pair differs at direct coordinate 69;
4. no state, object, card, edge, group, or action-reference bit changes;
5. applying arbitrary digest-tail replacements after `ZERO` cannot change
   logits;
6. a direct-feature perturbation and an action-reference perturbation each
   retain a live policy effect under `ZERO`;
7. every action-only intervention preserves value bits exactly.

If a runtime action pair is canonically distinct but indistinguishable at the
repaired digest-zero action ingress, admission fails. No training is then
licensed.

## 7. Three fixed model reads

The screen scores exactly three parameter authorities:

1. `raw-common-snapshot`
   - fresh trainer-seeded model-init seed:
     `6443515232517447393`;
   - named-parameter stream SHA-256:
     `36157c71b9fd736d4913e6c5722dcb9c1e4f119b7b28b108bde9d74f18862d54`;
2. `imported-mirror-g0`
   - model-parameter SHA-256:
     `db58dbe3f1f76b5bdf3bae4de657711dc818393b2bf1eeae88c02d8866b4d01d`;
   - canonical Store:
     `D:\mtg-kernel-exploiter-v3b-20260726\runs-arm1\dev0\run-0\store`;
3. `imported-diverged-g0`
   - model-parameter SHA-256:
     `9c692503df20669686d4b5706cd5ed53989a60ca9dec3778c10312b3bddc722e`;
   - canonical Store:
     `D:\mtg-kernel-exploiter-v3b-20260726\runs-arm2\dev0\run-0\store`.

The two Store reads must revalidate the same run, boundary, generation-zero
checkpoint, sidecar, payload, and identity-bundle bindings already admitted
by the prior v2 completion receipt. A copied parameter payload without its
Store authority is forbidden.

For each model, the screen records:

- exact parameter-manifest digest;
- baseline frozen/full output digest;
- repaired/full output digest;
- repaired/zero output digest;
- repeated-forward bit identity;
- direct-block and digest-block input value RMS;
- per-action-row direct and digest squared norm;
- first action-encoder-layer direct-only and digest-only contribution RMS,
  excluding bias and action-reference input;
- repaired direct-sibling and digest-sibling one-row-rotation effects:
  - mean Jensen-Shannon divergence in nats;
  - mean centered-logit RMS delta;
  - top-action-flip fraction;
- digest-minus-direct contrast for each of those three metrics;
- repaired/full versus repaired/zero effects;
- exact value-bit invariance for every action-only intervention.

The direct and digest contribution vectors are computed with the corresponding
columns of the same fixed `action_encoder.0.weight`; neither includes bias,
the other block, or the appended action-reference input.

## 8. Interpretation

The raw common snapshot receives one descriptive label:

- `RAW-INIT-DIGEST-DOMINANT` if all three digest-minus-direct functional
  contrasts are strictly positive;
- `RAW-INIT-DIRECT-DOMINANT` if all three are strictly negative;
- `RAW-INIT-MIXED` otherwise.

Exact zero is neither positive nor negative. The three metrics are correlated
views of the same logits and are not independent replications.

The two imported models receive the same labels separately. No vote, pooled
global label, causal percentage, or strength conclusion is permitted.

Interpretation:

- raw digest dominance supports initialization-time input geometry as a live
  explanation, but does not prove it is useful or harmful;
- imported-only digest dominance leaves inherited learned reliance as the
  leading unresolved explanation;
- mixed results retain both explanations;
- static/runtime admission failure blocks a digest-zero training arm.

Passing this screen licenses drafting, but not executing, the separately
predeclared three-arm micro-rung.

## 9. Integrity and execution

- Branch:
  `codex/observation-diagnostics-v1`
- Worktree:
  `C:\Users\Jack\IdeaProjects\mtg-kernel-observation-diagnostics-codex`
- Artifact root:
  `D:\mtg-kernel-action-ingress-admission-v1-20260726`
- Dedicated Cargo target:
  `E:\cargo-target-action-ingress-admission-v1`
- GPU use:
  forbidden
- Training, optimizer steps, checkpoint publication, Store mutation, and
  model-parameter mutation:
  forbidden

Before execution:

1. commit this manifest and the exact implementation;
2. require a clean worktree at that commit;
3. require the artifact root and Cargo target to be absent;
4. record the manifest SHA-256, implementation commit, Rust/Python source
   hashes, input hashes, exact commands, runtime tuple, and executable hash;
5. pass Linux and Windows static tests plus artifact-free Rust controls;
6. build one locked CPU release test executable;
7. run the raw snapshot and two Store reads sequentially with a fixed timeout;
8. preserve stdout, stderr, receipts, canonical reports, and an exact file
   inventory;
9. classify only a complete valid receipt.

Any source/input drift, dirty worktree, stale output, malformed record,
non-finite value, timeout, panic, nonzero exit, model digest mismatch,
corpus mismatch, cross-model parameter mutation, invariant failure, or
partial result is `INVALID`. No partial metric may be interpreted.

## 10. Non-claims

This screen makes no claim about:

- digest usefulness or harm;
- learned memorization, leakage, collision absence outside the checked
  corpora, or structured sufficiency outside the checked corpora;
- training signal, credit assignment, generalization, or robustness;
- game strength, promotion, equilibrium, multi-deck play, BO3, sideboarding,
  human play, or pro-level play.
