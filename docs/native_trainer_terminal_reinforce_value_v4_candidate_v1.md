# Native trainer successor: terminal_reinforce_value/v4-candidate (cycle-4)

Status: design draft under the ratified cycle-4 pre-registration
(`OX_CYCLE4_PREREG_SKETCH_V2.md`, SHA `c49bffd6`). This is a versioned trainer
successor used only by the STATIC-RB and TREATMENT-RB arms; it reinterprets no
v3 artifact, and CONTROL-R keeps v3 semantics untouched. Implementation
surfaces are pinned in section 5 after code recon.

## 1. What changes and what does not

The single change is the policy-term advantage. v3 computes, per learner
physical term:

    advantage = target - value
    policy_term = (-q) * advantage

v4-candidate computes:

    advantage_v4 = (target - value) - c_t(cell)
    policy_term = (-q) * advantage_v4

Everything else is unchanged: the value prediction, the value loss
`(value - target)^2`, the value coefficient, the grouping of substeps into
physical terms, the 1/group_count normalization, seat parity scheduling,
seed derivation, natural-terminal admission, and the observation/action/model
contracts (net-8). Terminal W/L remains the only reward; `c` is
action-independent and outcome-derived, a control variate.

## 2. The cell key

    cell = (opponent_behavior_identity, learner_role)

- `opponent_behavior_identity` is the opponent's exact checkpoint manifest
  SHA-256 (the identity recorded today in update-evidence episodes), never
  the mutable slot index. A returning identity resumes its cell; a genuinely
  new identity initializes at zero; a slot's value is never carried to a
  different occupant.
- `learner_role` is the learner's physical seat (p0/p1), which under the
  unchanged P0-first production start equals on-the-play/on-the-draw.

## 3. The strict-lag transaction (no leave-in bias)

Per update `t`, in order, matching the audit consult's required transaction:

1. Load model, optimizer, and the committed baseline state `c_t`.
2. Collect the update's 64 episodes.
3. Train using `c_t` only. Terms whose cell has no entry use `c = 0`.
4. After the optimizer step, derive `c_{t+1}` per observed cell:

       mean_cell = decision-weighted mean over the update's learner physical
                   terms in that cell of (target - value_pretrain)
       c_{t+1}(cell) = (1 - BETA) * c_t(cell) + BETA * mean_cell

   with `BETA = 0.05` (ratified), f32 arithmetic, terms accumulated in batch
   order into an f64 sum then rounded once to f32 at the division; cells not
   observed in update `t` carry `c_{t+1} = c_t` unchanged.

   `value_pretrain` is pinned to the CPU-CANONICAL transported value bits
   (the pre-update rollout value recorded per physical term in evidence),
   NOT a post-update re-evaluation and NOT the CUDA device forward output.
   Rationale (amendment after implementation review): the store validator
   must recompute the exact EMA trajectory from persisted evidence alone,
   which requires `c_{t+1}` to derive from evidence bits; and the CUDA lane
   already accepts a bounded tolerance between the transported bits and the
   device forward for every v3 evidence quantity, so the same bounded
   discrepancy between the optimized objective's value and the
   baseline-derivation value is the established acceptance, not a new one.
5. Atomically commit model, optimizer, `c_{t+1}`, per-cell decision and
   episode counts, old and new f32 bit patterns, and the source batch digest.
   A crash exposes the complete old state or the complete new state, never a
   mix and never a double-applied EMA.

## 4. Evidence and audit surface

Update evidence gains a `baseline_v4` record listing, per observed cell:
opponent identity, role, decision count, episode count, `c_t` and `c_{t+1}`
f32 bits, and the weighted-sum f64 bits before rounding. The store validator
recomputes the EMA from the evidence terms and rejects any mismatch, exactly
as the v3 validator recomputes the loss. The M3 diagnostic tests the centered
residual `(target - value) - c_t` (the lagged value actually used), not raw
advantage; near-zero per-cell means are expected only in steady state, not
after identity changes.

## 5. Implementation surfaces (pinned from code recon)

Backend scope: v4 admits only the CudaBurnDense backend (how the arms
actually run on GPU 1); the Sequential and FixedFourPartitions CPU backends
fail closed under a v4 run contract, mirroring the wide-path fail-closed
precedent (`native_trainer_v1.rs:3937-3941`). The wide model path is out of
scope.

Loss change sites, updated in lockstep (the loss exists in three places):

1. The differentiated CUDA kernel: `dense_group_loss_v1`
   (`experimental_burn_net8_packed_v1/training.rs:1868`) gains a per-group
   baseline tensor of shape `[group_count]`, built in
   `build_dense_group_loss_plan_v1` exactly as `targets` is
   (`training.rs:1764-1805`): `advantage = targets - values.detach() -
   baseline`. The value branch (`value_error = group_values - targets`,
   lines 1873-1874) is untouched, so the value head's regression target is
   unchanged.
2. The CPU-canonical evidence reconstruction: `bridge.rs:734-736` and `:953`
   subtract the same committed `c_t` f32 bits from the transported-bits
   advantage, so persisted evidence describes exactly the objective the
   device optimized.
3. The store validator: a v4 update-group validation path reconstructs
   `policy_term = (-q) * ((target - value) - c)` bit-exactly from persisted
   evidence (per-term cell reference plus the `baseline_v4` record of
   section 4), the same way the v3 validator recomputes today's loss at
   `native_training_store_update_group_v1.rs:2216-2246`. v3 evidence is
   untouched; cross-schema resume and validation fail closed.

Baseline-state persistence (crash-consistent by construction):

- The baseline map joins the train-state snapshot as a v4 sibling of
  `NativePolicyValueTrainSnapshotV1` (which already carries the non-tensor
  scalars `adam_step` and `scorer_bias_anchor_bits`): cells as a canonically
  ordered map from (opponent checkpoint manifest SHA-256, role) to f32 bits,
  with a fail-closed cap of 256 cells. `train_state_sha256` (v4) therefore
  covers the baseline, and `logical_state_sha256`'s atom list is unchanged
  because it already folds `train_state_sha256`.
- Persisting the composed snapshot uses the checkpoint-manifest v4 sibling
  schema (`mtg_kernel_native_train_checkpoint/v4`); the payload's frozen
  three-section `f32le` layout is unchanged. There is no "v4 store": v4
  manifests live only in the launcher-level chain described next, and every
  StoreV2 wire struct, publish schedule, resume read set, and leaf-grammar
  form stays exactly v3.
- Persistence architecture (pinned after the v4 core landed): the frozen
  StoreV2 publish, resume, leaf-grammar, and tip-proof surfaces are NOT
  extended. The arm's model and optimizer live in an unchanged v3 Store; the
  baseline persists as a launcher-level hash-chained artifact stream of
  checkpoint-manifest v4 records (one per checkpoint boundary), each binding
  the Store generation, the payload-derived core snapshot hash, the baseline
  cells, and the previous record's SHA-256, published with the same
  create-new atomic move primitives the Store uses. Crash consistency comes
  from the generation-pairing rule: resume validates that the newest chain
  record's generation and composed hash agree with the Store's checkpoint at
  that generation, and on any mismatch falls back to the newest agreeing
  pair; a mixed state (new parameters with stale `c`, or the reverse) is
  never resumable. This is the same launcher-chain pattern the g896
  structured and policy-only formal lanes used for their checkpoint chains.

Cell labeling at gradient time (pinned from the opponent-identity trace):

- The batch already carries episode boundaries and learner seat at every
  level (`FlatGroupedEpisodeCore.episode_id/learner_seat`, redundantly on
  each `FlatPhysicalDecisionSampleCore`), and the flatten loop in
  `train_grouped_candidate_v1` (`native_trainer_v1.rs:3700-3752`) already
  builds a per-group parallel vector (`terminal_returns`). The cell id rides
  the same pattern: computed once per episode in that loop and zipped into
  `NativePolicyPhysicalDecisionV1` beside `terminal_return`.
- Opponent identity is a pure function of `(base_seed, episode_index)`
  (`slot_for_episode_v1`, seed domain
  `kernel-native-ladder-trainer-sha256-v1` namespace
  `train-opponent-pool-choice`); today it is recomputed and attached to
  evidence only AFTER the gradient step
  (`attach_population_opponent_identity_v1`, `native_trainer_v1.rs:3624`).
  v4 moves that same side-effect-free recomputation before the step to
  label cells; evidence attachment is unchanged.
- v4 requires the population opponent engine (8-slot manifest mode); a v4
  run with only the ladder engine fails closed. The cycle-4 arms all run
  the manifest engine (STATIC-RB with a frozen genesis manifest).
- Launcher note for the later deliverable: engine construction currently
  lives in the `multirun_pilot_v1` ignored-test harness driven by
  `MULTIRUN_*` environment variables (`native_science_loop_v1.rs:923+`),
  which appears to be the de facto production launcher; the cycle-4
  launcher work must give the arms a first-class, contract-validated entry
  point rather than extending that harness.

## 6. Validation gates before the freeze

- Deterministic vectors: a fixed synthetic batch with known cells must
  reproduce exact `c` trajectories and loss bytes across two independent
  runs, plus a crash-recovery test proving old-or-new atomicity.
- A v3-equivalence test: with `c` forced to zero for all cells, the v4 loss
  must be bit-identical to v3 on the same batch.
- The existing preflight ladder (two-update repeat identity, checkpoint
  reload identity) applies unchanged to both arms using this trainer.
