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
- Launcher cross-binding (round-E review round 3): `validate_source_v2`
  admits the two production launcher names as a shape gate, and
  `validate_cross_bindings_v2` decides which one a given record may carry.
  A record declaring `population_program_v2_cycle4` must name
  `cycle4_arm_v1.exe` exactly; a record without that section must name
  `mtg-kernel-native.exe` exactly. Admitting either name for every record
  would let a cycle-4 record claim the legacy publisher, which is the
  wrong-attribution case the widening exists to avoid.

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

Round-E review round 3 adds two things. `--print-build-identity` is a
whole-command-line mode that writes this binary's embedded build tuple
(package, toolchain, source tree, features) as canonical JSON and exits 0,
having read nothing and touched no device; section 4a's builder uses it. And
at EVERY launch, bootstrap and interval alike, the arm captures its own
embedded build metadata plus a no-follow double read of its own executable
(`std::env::current_exe`) and requires the run record's `package`,
`toolchain`, `source` and `runtime` to equal it exactly, failing closed with
`cycle4_arm_v1_build_provenance_mismatch`. The gate sits immediately before
the Store prefix is claimed: strictly before any side effect, and after the
pure input-state rejections so those stay diagnosable. Without that a record built by
one build and an arm binary from another produced a Store whose record
attributed it to the wrong source tree, with every validator passing.

Round F adds a third whole-command-line mode, `--check-slot-locator PATH`,
accepted ALONE like `--print-build-identity`. It decodes the eight slot
Stores' `run.json` named by that locator, plus the genesis parent's when the
locator carries one, through the same `decode_train_run_v2` entry point
`resolve_population_opponent_cycle4_v1` and
`resolve_ladder_checkpoint_authority_v1` use, and exits 0 or 3. It is
strictly read-only and device-free: no Store root is opened, no checkpoint
read, no Store-prefix mode marker claimed, no CUDA context allocated, nothing
written. It exists because the first CONTROL preflight ladder attempt spent
two five-minute genesis bootstraps before either rung reached slot resolution
and refused there, on a roster record that had been on disk and undecodable
from the attempt's first second. The wrapper now runs this before any
bootstrap. It proves decodability only; identity binding needs a refresh
manifest and stays where it is proven, at the slot resolver.

## 4a. Bin `src/bin/cycle4_run_record_v1.rs` (round E)

Section 1 says what a cycle-4 `run.json` must declare but nothing produced
one, so every arm was blocked on an input no operator could write. This bin
derives it: `--arm`, `--parent-store-root`, `--parent-generation`,
`--arm-executable`, `--output`, plus the value-less `--force`. Same strict
parsing, same exit codes (0/2/3), same feature gating as section 4.
`--parent-generation` admits only the pre-registered 896 and is checked
before anything is staged from the parent, so the lineage tip can never be
written into `opponent_ladder_initialization`.

The record is assembled from three sources and nothing else: the arm kind
(which decides `arm_kind`, `static_pool`, the presence of
`trainer_v4_candidate`, the loss identity, and the arm's own base seed), the
pinned parent Store (train step, model architecture, schedule
shape, environment, plus the six digests of
`opponent_ladder_initialization`, resolved through the same
`stage_ladder_checkpoint_initialization_v1` the genesis bootstrap
re-derives), and the compiled cycle-4 literals. No clock, no environment
variable, no operator-chosen field, so two invocations against one parent
produce byte-identical output. Every predecessor program section is dropped.

The three formal training base seeds live on `Cycle4ArmKindV1` and nowhere
else, with the disjoint-domain policy the pre-registration's section 8
requires: one reserved band per arm, and the whole training band disjoint
from the payoff-panel band. The mapping is enforced in the launcher's own
record-level validator, not only by this builder, so an operator-supplied
record carrying another arm's seed is refused on every invocation.

Provenance is CAPTURED, not inherited. `package`, `toolchain` and `source`
come from this build's embedded build-capture tuple plus a real no-follow
double read of `--arm-executable`, and `runtime` is the CUDA runtime pair
all three arms train under; `contracts.train_step.numerical_backend_identity`
is set to match. Inheriting the parent's provenance would make a cycle-4
record describe an older executable built from an older tree, possibly
without the CUDA feature. `source.binary_name` therefore names
`cycle4_arm_v1.exe`, which section 1's validator now admits alongside the
legacy launcher.

The consequence is deliberate and worth stating: a record binds the exact
arm executable, file identity included, so a rebuilt or recopied launcher
produces a different record. The wrapper re-derives on every launch and the
builder refuses to replace a differing record, so that shows up as a refused
launch rather than a campaign that silently changed executables mid-run.
Determinism is unchanged for a fixed executable: two invocations against one
parent and one launcher still produce byte-identical output.

Capturing the arm launcher's executable hash is not on its own evidence
that the two binaries belong to one build: a hash is just a hash. So before
capturing, the builder runs `--arm-executable --print-build-identity` and
requires the reported tuple to equal its own byte for byte, refusing
otherwise. The arm then re-proves the same relationship from its own side at
every launch (section 4), so the record, the builder and the publisher are
pinned to one build from both ends.

Output passes `validate_train_run_record_v2` and the arm launcher's own
record-level check before any bytes are written. `--output` is published
through `durable_move_publication_v2`: create-new when absent, and
`replace_file_by_move_v2` for a forced replacement, because a plain rename
cannot replace an existing destination on Windows. An existing DIFFERENT
record is refused without `--force`, since a run record is a campaign
identity.

Reading the cycle-3 parent at all required widening the run contract with a
struct-only `population_program_v2_cycle3` section, on the terms
`population_program_v2_cycle2` was widened on: the real cycle-3 record
carries it, `deny_unknown_fields` otherwise makes that record undecodable,
and the genesis bootstrap decodes it too.

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

Round E corrections: the run record is derived by section 4a's bin on every
launch rather than supplied (`-UseExistingRunRecord` is the explicit
override); the slot table is rebuilt per boundary because `historical-1`
rotates by `refresh_index mod 3` (`-HistoricalOneStoreRoots`), and the
chosen root plus, before refresh 4, `historical-0`'s root are verified
against that boundary's manifest identity; two slots may name one Store at
different pinned generations (anchor-1 and the middle rotation phase share
970002), since identity rather than path is what must be distinct;
`-GenesisParentGeneration` is cross-checked against the run record's own
pinned origin, and its value is the cycle-3 focal run's store generation
896; and a DRY RUN publishes neither terminal marker, writing `result.json`
with status `DRY_RUN_PLANNED` instead, because a run that trained and
compared nothing may not leave behind the file an operator reads as
"finished".

Round F corrections, all five from the first CONTROL preflight ladder
attempt:

- **Exit codes are captured, or the launch stops.** `Start-Process -PassThru`
  under PowerShell 5.1 can return a `Process` holding no cached native
  handle; once the child is reaped `.ExitCode` answers `$null` forever, and
  `[int]$null` is `0`. The ladder therefore recorded exit_code 0 for two arm
  rungs that had in fact exited 3. `Invoke-Cycle4Process` now reads
  `.Handle` immediately after the start, which caches the handle for the
  object's lifetime, and treats a `$null` exit code as a hard failure rather
  than casting it. The `WaitForExit()` plus `Refresh()` pair is kept.
- **Inputs are proven before any bootstrap.** Section 4's
  `--check-slot-locator` runs in the inputs phase, over a locator the wrapper
  writes from the operator's roster plus the genesis parent, so an
  undecodable roster record stops the launch in a second instead of after two
  five-minute genesis bootstraps.
- **A launch form that works.** `-ParameterFile` takes one deny-unknown-keys
  JSON document naming every parameter, because `powershell -File` cannot
  pass an array at all; splatting from inside a session is the documented
  alternative.
- **A failure publishes a result document.** `RUN_FAILED` is still written,
  and beside it a `result.json` with status `RUN_FAILED`, the failing phase,
  the error text, and the commands run so far.
- **The panel executable's build identity is proven for a formal interval.**
  `-PanelExecutable` is a cargo test binary whose name is a content hash, and
  a preflight only hashed it; the first attempt used one that predated the
  launch commit. A formal launch now requires its embedded build identity, or
  a build receipt written beside it by the documented build step, to name the
  launch commit.

## 7. Delivery order

A. Section 1 plus 2 (contract widening, validators, evidence dispatch).
B. Sections 3 plus 4 (entry point and bin), depends on A.
C. Section 5 (panel runner and manifest builder), independent of A/B.
D. Section 6 (wrapper and preflight ladder), depends on B and C.
Each lands with focused tests and a Codex review round; A and C proceed in
parallel.
