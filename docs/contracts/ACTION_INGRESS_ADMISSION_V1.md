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
- Prior v2 completion receipt SHA-256:
  `f1be312d65e28e1c803c69fafc65cbe509d4ae4ba2828c0f8b8aa38595c55eb1`
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

The adapter starts from the frozen encoded decision and its authoritative,
row-aligned legal-action semantics, then clones only `action_features`.
Python retains the exact legal-action list passed to frozen
`encode_decision`; Rust retains the exact `FlatScorerActionCoreV2` rows and
references passed to the frozen tensorizer. Before patching, the adapter must
revalidate action count, row order, action kind, references, canonical model
JSON, and each row's frozen digest tail against those semantics.

The inclusion bit must come only from the bound `include` semantic or
`FLAT_ACTION_FLAG_INCLUDE_V1`. Inferring it from row position, neighboring
actions, display/stable metadata, or digest values is forbidden.

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

Any scale with a set sign bit, including negative zero, plus NaN, infinity,
malformed, missing, or unbound scale bits fails closed. A trainable gate is
forbidden.

The gate identity is:

`net8-fixed-action-digest-gate-f32-v1`

Only `FULL` and `ZERO` are scientific reads in this screen. `SCALED` exists
for fail-closed and seam tests and cannot be selected post hoc.

## 5. Static checked-corpus admission

The static authority is the unchanged 115-case action golden plus exactly
one supplemental case named
`supplemental-primary-choose-effect-target-player-self-v1`. It is derived
deterministically from the unchanged golden case
`primary-choose_effect_target`: retain its actor and source, retain
`selected=1`, `min=1`, and `max=3`, and replace only its target with the
actor-relative semantic
`{"target_kind":"player","player":"self"}`. Its flat core must have
`target_kind=1`, `target_player=1`, and `ref_len=1`, retaining only the
source reference and source object.

The supplemental case is constructed through the frozen classifier and
encoder and must witness both currently unwitnessed action atoms:

- `legal_action.semantic.<action_kind=choose_effect_target>.target.<target_kind=player>.target_kind`;
- `legal_action.semantic.<action_kind=choose_effect_target>.target.<target_kind=player>.player`.

The supplemental case, its canonical JSON, all tensors, and its digest must
be emitted into the diagnostic report; it does not replace the frozen v2
golden. The combined corpus identity is
`net8-action-ingress-static-checked-116-v1`. Its digest contract is
`sha256(canonical-json(case_identity_rows sorted lexicographically by name))`
and its expected SHA-256 is
`7a8cc702393253fdb2dfe61bcca648cbc8684cd1527e0d30a37b242baeb20a6e`.

Static admission requires all of the following:

1. action model-input coverage is exactly `202/202`;
2. action Boolean-polarity and optional-presence gaps are empty;
3. no distinct-canonical raw-digest, quantized-tail, or complete-
   representation collision exists;
4. after the slot-69 repair, no distinct-canonical structured alias exists;
5. no new or unexpected canonical-equivalence group exists;
6. attacker false/true and blocker false/true pairs differ in their direct
   prefixes at coordinate 69 and nowhere else;
7. those four cases retain their original canonical JSON, raw digest,
   quantized digest tail, action references, and all non-69 direct bits;
8. frozen encoding with no repair remains byte-identical to the checked
   authority.

Observation coverage is not an admission criterion and remains
`COVERAGE-INCOMPLETE`.

## 6. Runtime-corpus admission

The runtime screen rebuilds the unchanged fixed 256-decision Rally/Rally
corpus and requires its tensor digest to equal
`72103ea367a662f76675a044ad4efcf4c52bf86d32630df88e5247cf79f5e5e0`.

For every decision and each of the three fixed models:

1. after the structured repair and `ZERO` gate, every legal-action row has a
   pairwise numerically distinct 163-float pre-action-encoder non-digest
   vector: its repaired direct columns `[0,99)` followed by its exact
   64-float pooled action-reference hidden row computed by the frozen object,
   edge, node-update, and action-reference encoders. Every coordinate must be
   finite, and every pair must have at least one coordinate for which
   binary32 numerical comparison gives `a != b`; positive-zero versus
   negative-zero bits do not qualify as distinct. The report also records
   each vector's bit digest;
2. every encountered attacker/blocker false/true inclusion pair differs at
   direct coordinate 69, and the report records nonzero attacker-inclusion
   and blocker-inclusion pair counts;
3. no state, object, card, edge, group, or action-reference source bit
   changes;
4. rotating each complete frozen digest tail upstream within its decision so
   destination row `j` receives source row `(j + 1) mod n`, then applying
   `ZERO`, produces tensors and model outputs bit-identical to ordinary
   repaired/`ZERO`; decisions with `n=1` are unchanged. Artifact-free
   property tests cover additional finite replacement tails;
5. every action-only intervention preserves value bits exactly.

The fixed gate also receives artifact-free seam tests proving that direct and
action-reference inputs pass through unchanged under `ZERO`; no unbound
post-hoc perturbation is a runtime admission read.

If two legal-action rows have an identical repaired digest-zero
pre-action-encoder vector for any fixed model, admission fails. No training is
then licensed. This assertion is limited to the fixed runtime corpus and the
three fixed parameter authorities.

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
   - prior v2 probe payload:
     `runs/01-mirror-start-seed920013-g256/probe-payload.json`;
   - prior v2 probe-payload SHA-256:
     `9f3ed14d69dcf1019ab890060c1c4872e65a12b9d7222e83eeac1bccbcc7ec2b`;
3. `imported-diverged-g0`
   - model-parameter SHA-256:
     `9c692503df20669686d4b5706cd5ed53989a60ca9dec3778c10312b3bddc722e`;
   - canonical Store:
     `D:\mtg-kernel-exploiter-v3b-20260726\runs-arm2\dev0\run-0\store`;
   - prior v2 probe payload:
     `runs/04-diverged-start-seed920016-g256/probe-payload.json`;
   - prior v2 probe-payload SHA-256:
     `b95d91c0c79020fd37ea27285af31165977da9d0a6b582f77a5a4e12c8cfcfd5`.

The imported Store authorities must reproduce these exact expected
generation-zero bindings field by field:

- `imported-mirror-g0`
  - generation index: `0`;
  - run SHA-256:
    `0b46f9507caede181e745da51dabbb6c9f73d72d3eb2315f089ef248c60e2f80`;
  - identity-bundle SHA-256:
    `3b3e4e2270d307e7984314b91be69f1ccad0ec171d3210e3048a7ba2eb747024`;
  - segment ordinal: `0`;
  - segment-manifest SHA-256:
    `54c1d3cc527bc339f55734a47c660b2f5078b291a9d2d4b0cdfd36eeeaa8ec5e`;
  - parent-boundary-head SHA-256: `null`;
  - boundary-head SHA-256:
    `659a9e4cd250cf1f38a678d3632d1ce6ae1fd6aa7d7bc02918bb6e0d4762cfd2`;
  - boundary-head-record SHA-256:
    `0b45da9663aed2f56460c85693122dae267b9a7f782152023dcbe02f2fa3d64e`;
  - checkpoint-manifest SHA-256:
    `fb780bfb8c5de8f88a9a1254108c7f45f7a90dba75f8ef614c8103681c7127a1`;
  - checkpoint-payload SHA-256:
    `2a0840425ccfd09df56747d016d8fcd6b5bc19bba09b6f8cbcdc4507b7315095`;
  - checkpoint-sidecar SHA-256:
    `a6a6c1934f388ff0e212bb15a5f43f7fd6a03dc9ec1dff91acfe762a4a72b62f`;
  - logical-state SHA-256:
    `f46efcc86d9cc6ad2aec8bcc13e02560d1cd3bc3da166bb9a9e7054430dba18a`;
  - train-state SHA-256:
    `0b35c448201efe92375f48a22201c432d3272a3286fae1440f6e7aa2277b9de5`;
  - last-update-evidence SHA-256: `null`;
  - Adam step: `0`.
- `imported-diverged-g0`
  - generation index: `0`;
  - run SHA-256:
    `fee86543272b4f709be46bb7f9eec820d979d264a93b606408e07c9a6871e51f`;
  - identity-bundle SHA-256:
    `27c1c4798f8eb4a396e1952d055cb04122ce44d24fc8ff98118787ae0cb0985c`;
  - segment ordinal: `0`;
  - segment-manifest SHA-256:
    `9957484508c494032526b91a3226c8b30e3e82d5a50e4070479b74e5fda4a5b5`;
  - parent-boundary-head SHA-256: `null`;
  - boundary-head SHA-256:
    `142fe85ace4c0b8e4d006b2d424c5f65604375eb0f64856395e87b783d648a13`;
  - boundary-head-record SHA-256:
    `42360bb84f74a995be98f473181601baedd22ff238012fed4222d1790d11c456`;
  - checkpoint-manifest SHA-256:
    `2503dc79396fd9cf22e2771324e13b246f686de89503e497680d50091a4fbd99`;
  - checkpoint-payload SHA-256:
    `0d818f5803a96c7ae15c0a550cc9cec99bc50bf72a996697e2d0a1f09fd41145`;
  - checkpoint-sidecar SHA-256:
    `3c6ef5aa5fb4358014a95870060cc1cb0d80b2f38ee5fde8660167968f666ad0`;
  - logical-state SHA-256:
    `c05d303a31e300398ea40d3eca4b37b75a7cc832648fa0dda22920586a93e09b`;
  - train-state SHA-256:
    `207f2b99499ec67fcca99b332b28614771be84088696bbd4983c2053b482bd2c`;
  - last-update-evidence SHA-256: `null`;
  - Adam step: `0`.

A copied parameter payload, a replacement Store that is merely internally
valid, or any field mismatch is forbidden.

Before repaired reads, each imported model's frozen/full output must also
reproduce its prior v2 baseline stream bit for bit using digest identity
`sha256-framed-role-condition-decision-logit-value-f32le-v1`, role `g0`,
and condition `baseline`:

- `imported-mirror-g0`:
  `92d40cc1bd5ad4d54cb65cabb66b2788e4de16306727e6efc8c92f1b37e631da`;
- `imported-diverged-g0`:
  `39d5466625461fe9eb364436255a0dec0ba75d10c1d0fcdc27b6cc582a436dfc`.

Failure to reproduce either prior stream invalidates that read and therefore
the complete screen.

For each model, the screen records:

- exact parameter-manifest digest;
- baseline frozen/full output digest;
- repaired/full output digest;
- repaired/zero output digest;
- repeated-forward bit identity;
- repaired/`FULL` direct-block and digest-block input value RMS;
- repaired/`FULL` per-action-row direct and digest squared norm;
- first action-encoder-layer direct-only and digest-only contribution RMS,
  computed from repaired/`FULL` inputs and excluding bias and
  action-reference input;
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

For a decision with `n > 1` actions, sibling rotation writes destination row
`j` from source row `(j + 1) mod n` for only the selected block. Decisions
with one action are unchanged. The direct sibling rotates `[0,99)` and the
digest sibling rotates `[99,195)`; all other tensor bits remain fixed.

All scalar summaries use finite `f64` accumulation in corpus decision order.
Input value RMS is
`sqrt(sum(x^2) / element_count)` over every action row and every coordinate
in the named block. Per-row squared norms are the ordered `f64` sums of
squared binary32 values; their report includes every row and also the
decision-order arithmetic mean. First-layer contribution RMS is
`sqrt(sum(c^2) / (total_action_count * 64))`, where each 64-vector `c` is the
row-major binary32 weight/block matrix-vector product accumulated in
increasing coordinate order. Each output accumulator begins as exact
positive-zero `f32`; every multiplication and addition rounds to binary32 in
the same order as the frozen forward. Bias is excluded.

Functional effects use repaired/full output as baseline and weight each of
the 256 multi-action decisions equally:

- softmax converts each finite binary32 logit to `f64`, subtracts the row
  maximum, exponentiates, sums in legal-action order, and normalizes;
- Jensen-Shannon is
  `0.5 * KL(p || (p+q)/2) + 0.5 * KL(q || (p+q)/2)` in natural-log units,
  clamped upward to zero after a finite check;
- centered-logit RMS subtracts each row's own `f64` logit mean, takes the
  intervened-minus-baseline difference per legal action, and computes the
  root mean square within that decision;
- top action is the greatest finite logit with lowest-index tie break, and a
  flip is an unequal baseline/intervened top index;
- each reported mean/fraction is the arithmetic mean over the 256 decision
  values.

Repaired/full-versus-repaired/zero uses the same repaired/full baseline and
the same three formulas. Digest-minus-direct means the digest-sibling effect
minus the direct-sibling effect in that metric.

## 8. Interpretation

The raw common snapshot receives one descriptive label:

- `RAW-INIT-DIGEST-DOMINANT` if all three digest-minus-direct functional
  contrasts are strictly positive;
- `RAW-INIT-DIRECT-DOMINANT` if all three are strictly negative;
- `RAW-INIT-MIXED` otherwise.

Exact zero is neither positive nor negative. The three metrics are correlated
views of the same logits and are not independent replications.

Each imported model receives one separate authority-qualified label under the
same three sign predicates:

- `IMPORTED-DIGEST-DOMINANT`;
- `IMPORTED-DIRECT-DOMINANT`;
- `IMPORTED-MIXED`.

No vote, pooled global label, causal percentage, or strength conclusion is
permitted.

Interpretation:

- raw digest dominance supports initialization-time input geometry as a live
  explanation, but does not prove it is useful or harmful;
- imported-only digest dominance is consistent with reliance acquired before
  import, but one raw seed cannot distinguish that from
  initialization-to-initialization variation; both remain unresolved;
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
7. run the raw snapshot and two Store reads sequentially with a timeout of
   exactly 120 seconds per model invocation;
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
- raw-initialization behavior beyond seed `6443515232517447393` and the fixed
  Rally corpus;
- learned memorization, leakage, collision absence outside the checked
  corpora, or structured sufficiency outside the checked corpora;
- training signal, credit assignment, generalization, or robustness;
- game strength, promotion, equilibrium, multi-deck play, BO3, sideboarding,
  human play, or pro-level play.
