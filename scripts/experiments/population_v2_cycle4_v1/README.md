# Cycle-4 launch stack

Contract: `docs/native_cycle4_arm_launcher_v1.md`. Three arms (`control-r`,
`static-rb`, `treatment-rb`), 2048 updates each from the seeded genesis,
refresh interval 128, refreshes 0..16 (genesis at 0, the update-2048 panel at
16), G = 256 games per matchup.

Files here:

| File | What it is |
| --- | --- |
| `run-cycle4-arm.ps1` | The wrapper. One invocation drives one arm through the interval loop, or runs the CONTROL preflight ladder. |
| `common.ps1` | Shared helpers. Dot-sources `../regularized_continuation_retest_v1/common.ps1` and shadows `Get-FileHash` with a self-contained .NET SHA-256. |
| `run-cycle4-arm-tests.ps1` | Dry-run tests over a synthetic campaign. Launches nothing. |
| `run_payoff_panel_v1.py` | Round C: the 28-matchup payoff panel runner. |
| `bt_rating_v1.py` | Round C: the BT rating derived metric. |

## Operator inputs

Every path below is machine-local and never enters a hashed artifact.

| Input | Wrapper parameter | Notes |
| --- | --- | --- |
| Parent (cycle-3 lineage) Store root | `-GenesisParentStoreRoot` | The arm's genesis weights are copied from it. |
| Parent generation | `-GenesisParentGeneration` | `2048` for the cycle-3 lineage tip. The wrapper hashes `update-<gen>.{checkpoint.json,sidecar.json,state.f32le}` and `run.json` under it into the genesis authority record. |
| Eight slot store roots | `-SlotStoreRoots` | Absolute, in slot order 0..7 (`anchor-0`, `anchor-1`, `historical-0`, `historical-1`, `current-0`, `current-1`, `exploiter-0`, `exploiter-1`). No two slots may name the same root. |
| The arm's `run.json` | `-RunRecord` | Must declare `population_program_v2_cycle4`, and `trainer_v4_candidate` for the two rb arms. |
| The arm's Store root | `-StoreRoot` | Formal mode only. Its PARENT directory is the Store prefix the mode marker claims. |
| The arm's baseline chain directory | `-ChainDir` | Formal mode only. Per-update sidecars, boundary records, and `arm-origin.record.json` land here. |
| The refresh chain directory | `-RefreshChainDir` | Holds `refresh-NN.manifest.json` and `refresh-NN.panel.json`. One per arm. |
| The slot-identities roster directory | `-SlotIdentitiesRosterDir` | `refresh-NN.slot-identities.json` for NN = 1..16, schema `mtg-kernel-cycle4-slot-identities/v1`. See below. |
| `cycle4_arm_v1.exe` | `-ArmExecutable` | |
| `cycle4_refresh_build_v1.exe` | `-RefreshBuilderExecutable` | |
| The `mtg_kernel` release test executable | `-PanelExecutable` | The panel runner drives its ignored `ladder_head_to_head_eval_v1` test. |
| A Python 3.11 interpreter | `-PythonExecutable` | |
| Panel base seed | `-PanelBaseSeed` | One literal per arm. The wrapper strides 32,000,000 per refresh so no pair seed is reused anywhere in the campaign. |
| Device | `-Device` | `0` or `1`; defaults to `1`. Sets `CUDA_VISIBLE_DEVICES` for each child. |

Build the two bins and the panel test executable once:

```
$env:CARGO_TARGET_DIR = 'D:\cargo-target-cycle4'
cargo build -p mtg-kernel --release --features experimental-burn-net8-packed-cuda-v1,native-training-store-v2-production --bin cycle4_arm_v1 --bin cycle4_refresh_build_v1
cargo test  -p mtg-kernel --release --features experimental-burn-net8-packed-cuda-v1 --lib --no-run --message-format=json
```

The panel executable is the `executable` field of the last
`compiler-artifact` line whose `target.name` is `mtg_kernel` and whose
`target.kind` contains `lib`.

### Two inputs the wrapper cannot produce

1. **`refresh-00.manifest.json` (the genesis manifest).** Its `current-1` slot
   binds the arm's own Store at trainee-local 896, but the arm bin will not
   author that Store without a manifest to run against. The genesis manifest
   is therefore built out of band, once per arm, and placed in
   `-RefreshChainDir` before the first launch. The wrapper refuses to start
   without it.
2. **The frozen half of each `refresh-NN.slot-identities.json`.** Six slots
   (both anchors, `historical-1`, `current-0`, both exploiters) and, before
   refresh 4, `historical-0` are compiled Rust constants that the manifest
   validator matches exactly. The wrapper stages the operator's roster and
   overwrites only the slots the ARM itself occupies (`current-1` from refresh
   1, `historical-0` from refresh 4), read from the arm's own Store head.

## Invocations

### CONTROL preflight ladder (run first)

```
powershell -NoProfile -File scripts\experiments\population_v2_cycle4_v1\run-cycle4-arm.ps1 `
  -Mode preflight -Arm control-r `
  -EvidenceRoot E:\mtg-kernel-cycle4\evidence `
  -RunRecord E:\mtg-kernel-cycle4\control-r\run.json `
  -RefreshChainDir E:\mtg-kernel-cycle4\control-r\refresh-chain `
  -SlotIdentitiesRosterDir E:\mtg-kernel-cycle4\control-r\slot-identities `
  -SlotStoreRoots @('E:\...\slot-0','E:\...\slot-1','E:\...\slot-2','E:\...\slot-3','E:\...\slot-4','E:\...\slot-5','E:\...\slot-6','E:\...\slot-7') `
  -GenesisParentStoreRoot E:\mtg-kernel-population-v2-cycle3\lineage\run-0\store `
  -GenesisParentGeneration 2048 `
  -ArmExecutable D:\cargo-target-cycle4\release\cycle4_arm_v1.exe `
  -RefreshBuilderExecutable D:\cargo-target-cycle4\release\cycle4_refresh_build_v1.exe `
  -PanelExecutable D:\cargo-target-cycle4\release\deps\mtg_kernel-<hash>.exe `
  -PythonExecutable D:\mtg-kernel-cycle4\venv\Scripts\python.exe `
  -PanelBaseSeed 4100000000 `
  -Device 1
```

Two throwaway Store prefixes are created under the attempt root
(`ladder\a\store`, `ladder\b\store`), seeded identically from the same parent
and run record, each advanced by the same short window, then compared: every
relative file's size and SHA-256, the whole store tree hash, and the
endpoint's four identity fields. On pass the attempt root gets an empty
`PREFLIGHT_COMPLETE`.

`-PreflightUpdates` defaults to the smallest window that is at least two
updates and a whole number of checkpoint segments (4, for the pre-registered
four-update segment). The arm bin bounds it to 1..8 and refuses it entirely on
any Store prefix a formal run has claimed.

### One arm, formal

Same parameters with `-Mode formal`, the arm's own `-Arm`, plus `-StoreRoot`
and `-ChainDir`:

```
  -Mode formal -Arm treatment-rb `
  -StoreRoot E:\mtg-kernel-cycle4\treatment-rb\store `
  -ChainDir  E:\mtg-kernel-cycle4\treatment-rb\baseline-chain `
```

Substitute `-Arm control-r` or `-Arm static-rb` (with each arm's own store,
chain, refresh chain, roster, run record, and panel base seed) for the other
two.

Per interval the wrapper asserts the Store's resume position, runs the arm bin
at `position + 128`, asserts the Store advanced exactly one interval, runs the
panel over that interval's manifest roster, publishes the panel into the
refresh chain, and builds the next manifest. `static-rb` runs the panel but
never builds: the wrapper asserts before and after every interval that no
manifest past `refresh-00` exists. On completion the attempt root gets an
empty `TRAINING_COMPLETE`; any failure writes a plain-text `RUN_FAILED` naming
the failing phase.

The wrapper resumes: it derives its starting interval from the Store's own
`latest.json`, so a killed run is restarted with the same command line.
`-ThroughRefreshIndex` (default 16) stops it earlier.

### Dry run

Add `-DryRun` to validate every input, write the provenance records and both
locator files, and print the exact command line of every child without
launching one. `-SkipHostAssertions` additionally skips the git, toolchain,
and GPU assertions and is accepted ONLY with `-DryRun`.

```
powershell -NoProfile -File scripts\experiments\population_v2_cycle4_v1\run-cycle4-arm-tests.ps1
```

runs the dry-run test suite against a synthetic campaign under the temp
directory.

## Known issue: slot generation numbering

The refresh manifest pins slots 2 and 5 to TRAINEE-LOCAL generations
(`896 + refresh_index * 128`), while `resolve_population_opponent_cycle4_v1`
and the panel runner pass `source_generation` straight to the Store as a
STORE generation, and the arm's Store numbers its own generations 0..2048.
For the arm-owned slots the two readings differ by 896. The wrapper writes the
value the manifest validator requires (trainee-local), because no other value
is admissible into a manifest, and reads the hashes at the corresponding store
generation. Resolving the disagreement is a round A/B/C change, not a wrapper
change.
