# Cycle-4 arm launcher (v1)

Status: design contract under the ratified pre-registration
(`OX_CYCLE4_PREREG_SKETCH_V2.md`, SHA `c49bffd6`). This is the integration
deliverable that makes the three arms launchable: a first-class,
contract-validated entry point replacing the env-var `multirun_pilot_v1`
test harness. Pinned from code recon of the harness, the library entry
points, the run-contract validator, and the g896 formal-wrapper family.

## 0. Integration blocker discovered during recon (binding correction)

The v4 loss plumbing declares the baseline-subtracted policy sum in update
evidence (so evidence describes what the device optimized). The Store's v1
evidence validator (`native_training_store_update_group_v1.rs:2210-2246`)
recomputes the v3 loss bit-exactly at publish and resume, so a v3 Store
would reject v4 updates. Therefore the earlier "v3 Store untouched" phrasing
is narrowed: the Store's checkpoint payload, publish, resume, leaf grammar,
and tip proof stay v3, but (a) the run contract gains an honest v4 trainer
identity and (b) update-evidence validation dispatches on that identity to
the v4 recompute (`native_training_store_update_group_v4`) using the
per-update baseline sidecar. A run.json must declare the trainer that ran.

## 1. Run-contract widening (`native_training_store_run_v2.rs`)

- New optional contract section `trainer_v4_candidate` (serde default,
  skip-if-none, mirroring `population_program_v2_cycle2`'s field shape but
  WITH a real validator wired into `validate_contracts_v2`): pins the loss
  identity literal `terminal_reinforce_value/v4-candidate`, the baseline
  schema `mtg-kernel-native-baseline-state/v1`, `BETA` f32 bits (0.05), the
  cell cap 256, the contract doc SHA-256, and the numerical backend
  requirement (`CudaBurnDense`). When present, `FROZEN_LOSS_IDENTITY`
  equality is replaced by equality with the v4 literal; when absent, v3
  behavior is byte-identical.
- New optional section `population_program_v2_cycle4`: the pre-registration
  SHA, the cycle-4 refresh-manifest schema string, the arm kind
  (`control-r` | `static-rb` | `treatment-rb`), the trainee start
  generation 896 and stop generation 2944, and the refresh interval 128,
  with a real validator. `run_native_science_loop_with_population_v1`'s
  `population_program_v1.is_some()` gate gains a sibling that accepts
  `population_program_v2_cycle4` (the v1 gate's ladder-contract
  co-requirement is not carried; cycle-4 arms run the population engine
  only). STATIC-RB declares the same section with refresh interval 128 but
  a `static_pool: true` flag: manifests never advance past genesis.
- Arm-kind consistency: `treatment-rb` and `static-rb` require
  `trainer_v4_candidate`; `control-r` forbids it.

## 2. Evidence validation dispatch

When the run declares `trainer_v4_candidate`, per-update validation runs
the v1 structural checks (grouping, counts, identities, value loss) and
replaces the v3 policy-sum recompute with
`validate_update_baseline_v4(episodes_view, sidecar_record, prior_state,
update_index, update_evidence_sha256)`, adapting v1 evidence into the v4
view (the documented per-episode cursor walk). The sidecar record for update
`t` lives in the arm's chain directory as
`baseline-update-<8-digit index>.record.json`, published atomically right
after the Store commits update `t` and BEFORE update `t+1` begins; the
checkpoint-boundary chain record binds the SHA-256 of every sidecar record
since the previous boundary. Resume validates the pairing rule at the
checkpoint level and replays the sidecar records within the last boundary
to reconstruct `c_t` exactly; a missing or unbound sidecar fails closed.

## 3. Library entry point `run_native_cycle4_arm_v1`

Sibling of `run_native_science_loop_with_population_v1`
(`native_science_loop_v1.rs:467`), one call per refresh interval:

1. Validate the run contract (section 1), the arm kind, and the device
   contract; open or resume the Store.
2. Decode the cycle-4 refresh manifest for this interval with its panel
   bytes (content-resolving decode; genesis takes none), resolve the eight
   slot stores from a machine-local locator file (absolute paths are never
   in hashed contracts), and build `PopulationOpponentEngineV1` from the
   manifest's weight vector via a cycle-4 sibling of
   `resolve_population_opponent_v1`.
3. For v4 arms: resume the baseline chain (pairing rule), replay in-boundary
   sidecars, install `c_t` via `set_baseline_state_v4` before each window;
   after each committed update, apply the returned observations, publish
   the sidecar record, and at each checkpoint boundary publish the chain
   record. CONTROL-R never installs a baseline (v3 path, bit-identical).
4. Stop after exactly `interval` generations (start + 128) and exit cleanly
   for the wrapper to run the panel and refresh; the g896 `+256` sustained
   precedent generalizes only as this per-interval stop, never a
   multi-interval process.
5. Final-store validation on exit (existing `validate_native_training_store_v2`).

## 4. Bin `src/bin/cycle4_arm_v1.rs`

Strict flag parsing into a typed request (following
`checkpoint_shadow_stdio_v1.rs`), no environment variables: `--arm`,
`--store-root`, `--run-record`, `--chain-dir`, `--refresh-manifest`,
`--payoff-panel` (absent only for genesis), `--slot-locator`,
`--stop-generation`, `--device` (sets `CUDA_VISIBLE_DEVICES` for this
process only; there is no library device parameter to inherit). Exit codes:
0 complete, 2 usage, 3 contract rejection, 1 runtime failure. Gated behind
the CUDA feature and the production feature like the existing bins.

## 5. Payoff panel runner (`scripts/experiments/population_v2_cycle4_v1/`)

Ports `scaled_selfplay_population_v1/run_payoff_evaluation.py`'s 28-matchup
round robin to the cycle-4 roster: eight identities from the current
manifest, `G = 256` seeded games per matchup, natural terminals only, no
reused pair seeds, emitting one canonical panel JSON (the bytes the next
manifest binds by hash) plus the BT-rating input document
(`mtg-kernel-bt-rating-input/v1`, reference id = anchor-0) for the derived
metric. The MW weight update then runs through `mw_update_cycle4_v1` via a
thin builder that writes the next manifest.

## 6. Wrapper (`run-cycle4-arm.ps1`)

Reuses `regularized_continuation_retest_v1/common.ps1` (git/toolchain
records, GPU-1 idle assertions, attempt roots, store tree hash) and ports
from the g896 wrappers: the self-contained .NET SHA-256 hashing (detached
PowerShell lacks `Get-FileHash`), the `WaitForExit()` plus `Refresh()`
double call for the GPU monitor's exit code, and the CONTROL preflight
ladder (two independent Store prefixes, two short updates each,
byte-identical relative-file hash comparison plus endpoint fields). Loop
per interval: assert resume position matches the Store, run the bin, run
the panel, build the next manifest, repeat through refresh 16. Terminal
markers follow the g896 family: gate-specific empty markers
(`PREFLIGHT_COMPLETE`, `TRAINING_COMPLETE`) plus a plain-text `RUN_FAILED`.

## 7. Delivery order

A. Section 1 plus 2 (contract widening, validators, evidence dispatch).
B. Sections 3 plus 4 (entry point and bin), depends on A.
C. Section 5 (panel runner and manifest builder), independent of A/B.
D. Section 6 (wrapper and preflight ladder), depends on B and C.
Each lands with focused tests and a Codex review round; A and C proceed in
parallel.
