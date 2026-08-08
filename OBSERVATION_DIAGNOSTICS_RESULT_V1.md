# Net8 Observation Diagnostics Result v1

Status: RESULT RECORDED; diagnostic only.

## Outcome

The checked Net8 action pathway is highly sensitive to its opaque 96-float
legal-action digest block relative to the matched 99-float direct-action
block. All six trained candidates show larger digest-permutation effects for
action Jensen-Shannon divergence, centered-logit RMS, and top-action flips.

That is an architectural reliance warning, but it is not evidence that
training learned digest memorization:

- the same action-digest dominance is already large at run generation zero;
- candidate-minus-generation-zero change is `MIXED` for Jensen-Shannon and
  centered-logit RMS;
- the relative top-action-flip contrast decreases in five of six pairs even
  though every candidate remains action-digest dominant on that metric.

Those generation-zero checkpoints are two imported, previously trained
ladder models, not fresh initializations. The result therefore excludes the
dominance being newly acquired during the measured 128--512 updates, but it
cannot distinguish intrinsic input-geometry sensitivity from reliance
inherited from ancestor training. Dense whole-record fingerprint geometry is
a leading hypothesis, not a result of this diagnostic.

The state/value result differs. State-direct permutation exceeds
state-digest permutation in all six candidates and in all six
candidate-minus-generation-zero changes.

No global label is licensed. The eight predeclared reads disagree across
metrics and scopes, exactly as the classifier records.

## Authorities and execution history

### Source

- Parent exploiter result:
  `a6259b2d82474a407af98752dbbf802361f0076d`
- Diagnostic implementation:
  `75f8a375a10e66c1fb06eab4d3c0f8a5d59b6f51`
- Valid v2 execution producer:
  `c1cf5f1de05b64a4cae35c61862adc725df46837`
- Classification-retry implementation:
  `ad2689ef2953a7871e5e496a98044ecc00a54938`
- Execution v1 manifest SHA-256:
  `8b8e364cf122397e940c0bf76e6c186fd4ed188a20e65faaa908531cbbc2c575`
- Execution v2 manifest SHA-256:
  `28cdbd8e582c49d5a414c61cf8dfda974467f36093783e246ee9a26d1b79e4d1`
- Classification-retry manifest SHA-256:
  `bc81da0ac15c2d70d654ab8272f1d3b127523ffd3b6aad3448e88a1b751d0a8d`

### Preserved packaging failures

Execution v1 built successfully, but its first probe stdout was rejected
before payload admission because the parser did not account for the exact
Windows libtest prefix. Its artifact root remains immutable:

`D:\mtg-kernel-observation-diagnostics-v1-20260726`

No v1 metric was admitted or interpreted.

Execution v2 then completed all six probes. Its first offline classifier
attempt failed closed because the classifier incorrectly required
`identity_bundle_sha256` to differ between candidate and generation zero.
Rust/Store authority defines that digest at the run level, so equality is
required. Independent review checked 42 Store bindings per pair, 252 total,
with no mismatch; candidate and generation-zero parameter/checkpoint states
are distinct in every pair.

The failed classifier directory remains exactly three files with no
classification output:

`D:\mtg-kernel-observation-diagnostics-v2-20260727\classification`

- failure receipt SHA-256:
  `552edc97b059374f4996e2164ab7546e05dbc91db33af3cb50f27f786d507c56`
- failure receipt payload SHA-256:
  `c1238c21a3ad0a8fac196693ff036e79941b413393db8491e9cebf093dee14f8`
- stderr SHA-256:
  `b2b1a7d72b607bd737011a545d565c19e06897e32bc2d8247ffa2659408195ea`

Neither packaging failure is a scientific result.

### Valid execution and classification

- Artifact root:
  `D:\mtg-kernel-observation-diagnostics-v2-20260727`
- Locked CPU release executable SHA-256:
  `b4a6c5d0713f5ba562212aa6411b4938b9448086d2e26c6a5248ec67ab9ed533`
- Build receipt SHA-256:
  `c15e70719aa52bd31d55389edc57d0ca76665b0f924e96e4e51a90ed35173f49`
- Completion receipt SHA-256:
  `f1be312d65e28e1c803c69fafc65cbe509d4ae4ba2828c0f8b8aa38595c55eb1`
- Completion payload SHA-256:
  `d174a5c55a4dcac2ed397941f2f7626855cd78a9c8924fdd64db6a38553e9a25`
- Bound 30-file inventory aggregate SHA-256:
  `5017527337cd0c29ed781a1aebffa421cc3528cc6f53f7d6f09248f35a57913e`
- Six-pair wall time:
  `461784 ms`
- Classification retry root:
  `D:\mtg-kernel-observation-diagnostics-v2-20260727\classification-retry-v1`
- Classification output SHA-256:
  `eb5d8f944b70dc00be5cd842ee2d5120664ec98b819c77d2221e4fcf5680d541`
- Classification payload SHA-256:
  `77608f948b3811368f0c8c29d520bcf7ba1f444c3de6cf2ba540c7ce3f008acf`
- Classification receipt SHA-256:
  `d354bb6c7ab802c9158ee358bbdebd350e5dbcb60c2a6ea8f6725d9feaa66cef`
- Classification receipt payload SHA-256:
  `06bb90f09dcf875f731fe4598cef138d038afbdeadaf2d7e8d025cd3d4153169`
- Classifier exit/timeout:
  `0` / `false`
- Classifier wall time:
  `261 ms`
- Classification authority:
  `AUTHORITATIVE-DIAGNOSTIC-READ`

The retry wrapper verified the frozen execution manifest, build and
completion receipts and payloads, executable, all build-bound files, all 30
run files, the prior failure bundle, both Git heads, and its sources before
and after classification. Linux and Windows packaging suites each passed
68/68 tests. Two independent reviews accepted the scientific correction and
the final retry packaging.

## Diagnostic A: checked-corpus coverage and collisions

The checked-in feature audit is `COVERAGE-INCOMPLETE`.

- Corpus: 18 observation cases and 115 action cases.
- Observation model-input atoms: 342/564 witnessed.
- Action model-input atoms: 200/202 witnessed.
- Distinct-canonical raw-digest collisions: none.
- Distinct-canonical quantized-tail collisions: none.
- Distinct-canonical complete-representation collisions: none.
- Unexpected canonical equivalences: none.

Two action groups have distinct canonical semantics but identical structured
signatures:

1. `boolean-attacker-false` vs `boolean-attacker-true`;
2. `boolean-blocker-false` vs `boolean-blocker-true`.

Their complete representations remain distinct only because the legal-action
digest differs. Therefore, deleting the digest without first encoding the
attacker/blocker inclusion Boolean would introduce a known representation
collapse.

The two unwitnessed action atoms are the player-target `player` and
`target_kind` fields under `choose_effect_target`. Observation coverage has
222 unwitnessed atoms, so the audit cannot establish whole-schema structured
sufficiency.

`action_ref_card_ids` is validated transport data but is not consumed by the
Net8 forward. It was not credited as a distinguishing model input.

Audit report:

`data\flat_policy_v2\feature_coverage_collision_audit_v1.json`

- report SHA-256:
  `a217144c4506e811109d5fac7d4ec9956c514b006af36355882df3ef1f3dde47`
- payload SHA-256:
  `6a9c23d37c94c522d7fea6ca08f5cd4d4e2b11fb9328db244259cbefb6192a64`

## Diagnostic B: eight separately classified reads

Every contrast is digest-sibling effect minus matched direct-sibling effect.
Positive means the digest effect is larger; negative means the direct effect
is larger. The pooled means are descriptive in each metric's own units and
must not be compared across rows.

| Metric | Generation-zero mean | Candidate read | Candidate-minus-g0 read |
|---|---:|---|---|
| Action Jensen-Shannon mean | `+0.3080507250070590` | `DIGEST-SIBLING-EFFECT-EXCEEDS`; 6 positive, 0 negative; mean `+0.3144896841307701767` | `MIXED`; 4 positive, 2 negative; mean `+0.006438959123711177` |
| Action centered-logit RMS mean | `+5.536804738008470` | `DIGEST-SIBLING-EFFECT-EXCEEDS`; 6 positive, 0 negative; mean `+4.8590725579784975333` | `MIXED`; 2 positive, 4 negative; mean `-0.6777321800299724667` |
| Action top-action-flip fraction | `+0.509765625` | `DIGEST-SIBLING-EFFECT-EXCEEDS`; 6 positive, 0 negative; mean `+0.4609375` | `DIRECT-SIBLING-EFFECT-EXCEEDS`; 1 positive, 5 negative; mean `-0.048828125` |
| State value RMSE | `-0.09656442343371311` | `DIRECT-SIBLING-EFFECT-EXCEEDS`; 0 positive, 6 negative; mean `-0.144095557644057645` | `DIRECT-SIBLING-EFFECT-EXCEEDS`; 0 positive, 6 negative; mean `-0.047531134210344535` |

The authoritative JSON contains every raw six-pair contrast. Its
`metric_label_disagreement_by_scope` values are both `true`;
`any_label_disagreement_across_scopes_or_metrics` is `true`; and
`global_label` is `null`.

The three action metrics are correlated summaries of the same policy logits,
not statistically independent replications. Arm, seed, and candidate
generation also vary together in these six pairs, so the result cannot
attribute differences among them.

## Interpretation

### What the result supports

1. The current action digest has much greater policy leverage than its direct
   sibling on the fixed Rally corpus at trained checkpoints.
2. That leverage is not newly acquired during the measured run segments. It
   is already large at run generation zero and does not consistently grow
   over the measured 128--512 updates.
3. The current state/value path relies more on direct state features than on
   its digest sibling under the matched permutations.
4. A controlled action-encoder experiment is warranted.

### What the result does not support

- No causal percent reliance.
- No proof of memorization, hidden-information leakage, hash collision,
  representation bottleneck, or poor generalization.
- No separation of intrinsic input geometry from reliance learned before
  these runs began.
- No claim that direct structured features are sufficient.
- No attribution to training signal or credit assignment.
- No game-strength, promotion, equilibrium, multi-deck, BO3, human, or
  pro-level-play claim.

The generation-zero comparison is especially important. The three
mirror-start runs share the imported Pool3 primary generation-384 model
(`db58dbe3f1f76b5bdf3bae4de657711dc818393b2bf1eeae88c02d8866b4d01d`),
and the three diverged-start runs share the imported historical-rung-3
generation-512 model
(`9c692503df20669686d4b5706cd5ed53989a60ca9dec3778c10312b3bddc722e`).
Thus the six generation-zero rows contain two distinct previously trained
models, not six independent fresh initializations.

## Routed next experiment

The next experiment should be a controlled action-ingress/structured-encoder
experiment, not an immediate training-signal verdict. It starts with a cheap
no-training admission screen and proceeds to a micro-rung only if that screen
passes.

Minimum implementation order:

1. Version a new encoding/run identity while preserving the 195-float action
   width and Net8 tensor shapes. Recast explicit coordinate 69 as a generic
   action-kind-conditioned Boolean: `value` for `choose_effect_boolean` and
   `include` for attacker/blocker inclusion. The action-kind one-hot already
   distinguishes those meanings. Do not relabel existing Net8 artifacts.
2. Add the two missing player-target witnesses. Require action coverage
   202/202, no checked-corpus structured aliases or collisions, and a runtime
   assertion that every encountered legal-action set remains distinguishable
   when its action digest is zeroed. This does not establish whole-schema
   completeness.
3. On the fixed corpus, score the genuinely fresh frozen common-model
   snapshot and both imported run-generation-zero models. Report direct- and
   digest-block input energy and first-layer contribution RMS. This is the
   smallest screen that addresses the raw-initialization ambiguity.
4. If admission passes, run three 256-update arms, all with the structured
   repair:
   - A: current stable canonical action digest;
   - B: action digest fixed to positive zero;
   - C: one deterministic episode-specific permutation of the 96 digest
     coordinates, applied identically to every action row in that episode.
5. Use six matched training seeds, the same promoted(2) generation-384 start
   with reset optimizer moments, batch 64, and the existing Pool3
   40/20/20/20 curriculum. Keep state/value, object/card/edge/group/action-ref
   paths, optimizer, limits, and seed schedules fixed. Online trajectories
   may diverge and are not bit-paired.
6. Evaluate only the fixed generation-256 heads with common seat-balanced
   seeds against promoted(2), plus promoted(1) and uniform regression
   controls. Predeclare effect-size, uncertainty, cross-seed consistency,
   absolute-strength, regression, and integrity gates before execution; no
   single aggregate win rate can select an arm.

The immediate design question is whether the opaque digest contributes useful
semantic coverage after known structured gaps are closed, or mainly supplies
a dense fingerprint whose scale dominates the direct-action channel. This
result licenses that experiment; it does not answer it.
