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
| Parent generation | `-GenesisParentGeneration` | `896`: the cycle-3 focal run's store generation 896, which is trainee-local 896 and the pre-registered start. NOT the lineage tip 2048. The wrapper hashes `update-<gen>.{checkpoint.json,sidecar.json,state.f32le}` and `run.json` under it into the genesis authority record, and cross-checks all four against the run record's own `contracts.opponent_ladder_initialization`, so a wrong generation fails at phase=inputs rather than binding the wrong parent. |
| Eight slot store roots | `-SlotStoreRoots` | Absolute, in slot order 0..7 (`anchor-0`, `anchor-1`, `historical-0`, `historical-1`, `current-0`, `current-1`, `exploiter-0`, `exploiter-1`). Two slots MAY name the same root when their pinned generations differ (anchor-1 is 970002 at 1536 and historical-1's middle rotation phase is 970002 at 1024, one Store as two occupants); the same root at the same generation in two slots is still rejected. Whichever slots the manifest binds to the arm's own run (`current-1` always, `historical-0` from refresh 4) are overridden with `-StoreRoot`, so their table entries are placeholders. |
| The three `historical-1` rotation roots | `-HistoricalOneStoreRoots` | Absolute, in rotation order: the Stores for program-v1 seeds 970001, 970002 and 970003, each pinned at generation 1024. Slot 3 takes `roots[refresh_index mod 3]` at every boundary. Omit it only if you intend the campaign to stop at refresh 1: the wrapper verifies the chosen root's four content hashes against that boundary's slot-3 identity and fails closed. |
| The arm's `run.json` | `-RunRecord` | Where the wrapper WRITES the derived run record (and re-derives it on every later launch). Not an operator input unless `-UseExistingRunRecord` is given. |
| `cycle4_run_record_v1.exe` | `-RunRecordExecutable` | The run-record builder. Required unless `-UseExistingRunRecord`. |
| The arm's Store root | `-StoreRoot` | Formal mode only. Its PARENT directory is the Store prefix the mode marker claims. |
| The arm's baseline chain directory | `-ChainDir` | Formal mode only. Per-update sidecars, boundary records, and `arm-origin.record.json` land here. |
| The refresh chain directory | `-RefreshChainDir` | Holds `refresh-NN.manifest.json` and `refresh-NN.panel.json`. One per arm. The wrapper builds every manifest in it, genesis included. |
| The slot-identities roster directory | `-SlotIdentitiesRosterDir` | `refresh-NN.slot-identities.json` for NN = 0..16, schema `mtg-kernel-cycle4-slot-identities/v1`. See below. |
| `cycle4_arm_v1.exe` | `-ArmExecutable` | |
| `cycle4_refresh_build_v1.exe` | `-RefreshBuilderExecutable` | |
| The `mtg_kernel` release test executable | `-PanelExecutable` | The panel runner drives its ignored `ladder_head_to_head_eval_v1` test. |
| A Python 3.11 interpreter | `-PythonExecutable` | |
| Panel base seed | `-PanelBaseSeed` | One literal per arm. The wrapper strides 32,000,000 per refresh so no pair seed is reused anywhere in the campaign. |
| Device | `-Device` | `0` or `1`; defaults to `1`. Sets `CUDA_VISIBLE_DEVICES` for each child. |

Build the three bins and the panel test executable once:

```
$env:CARGO_TARGET_DIR = 'D:\cargo-target-cycle4'
cargo build -p mtg-kernel --release --features experimental-burn-net8-packed-cuda-v1,native-training-store-v2-production --bin cycle4_arm_v1 --bin cycle4_refresh_build_v1 --bin cycle4_run_record_v1
cargo test  -p mtg-kernel --release --features experimental-burn-net8-packed-cuda-v1 --lib --no-run --message-format=json
```

The panel executable is the `executable` field of the last
`compiler-artifact` line whose `target.name` is `mtg_kernel` and whose
`target.kind` contains `lib`.

### The one input the wrapper cannot produce

**The frozen half of each `refresh-NN.slot-identities.json`, for NN = 0..16.**
Five slots (both anchors, `current-0`, both exploiters) plus `historical-1`'s
three rotation phases are compiled Rust constants that the manifest validator
matches on all seven fields. `historical-0` before refresh 4 is NOT a compiled
constant: the validator pins only its `source_run_sha256`, `source_base_seed`
and lagged `source_generation` (the cycle-3 lineage at trainee-local minus
512), and reads its four content hashes from this roster. Those four are
therefore proven the only way they can be, against the cycle-3 Store itself:
the wrapper recomputes them at that generation before writing the interval's
locators, and does the same for `historical-1`'s rotation root, so a roster
entry that does not match the Store on disk stops the interval.

The wrapper stages the operator's roster and overwrites only the slots the ARM
itself occupies (`current-1` at every refresh including genesis, `historical-0`
from refresh 4), read from the arm's own Store head.

Nothing else is an operator input. In particular `refresh-00.manifest.json` is
NOT (see the genesis sequence below) and neither is the arm's `run.json`
(`cycle4_run_record_v1` derives it, and re-derives it on every later launch).

### The genesis sequence

The genesis manifest's `current-1` slot binds the arm's own Store at
trainee-local 896 (store generation 0), which does not exist until the Store
does, and an interval invocation cannot open a Store without a manifest. The
wrapper breaks that circularity in two steps, both automatic:

1. `cycle4_arm_v1 --bootstrap-genesis` seeds the Store from the pinned parent
   and exits without training. It validates the run and device contracts
   exactly as an interval would, claims the Store prefix, runs the final-store
   validation, and publishes `arm-origin.record.json` carrying the four hashes
   of the genesis checkpoint it just wrote. On a Store that already holds a
   genesis it is exit 3, so the wrapper only calls it when `latest.json` is
   absent.
2. `cycle4_refresh_build_v1 --genesis` authors `refresh-00.manifest.json` from
   the operator's pinned roster with the own-run slot filled from that Store.

The wrapper then asserts the built manifest's own-run slot against the same
four hashes read three independent ways: from the manifest, from the origin
record, and from its own read of `checkpoints/update-00000000.checkpoint.json`.
That agreement is written to `genesis-binding.json` in the attempt root. Both
steps are skipped when the Store and the manifest already exist, so a
restarted campaign runs neither.

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
  -HistoricalOneStoreRoots @('D:\...\wave-00-seed-970001-gpu0\run-0\store','D:\...\wave-00-seed-970002-gpu1\run-0\store','D:\...\wave-01-seed-970003-gpu1\run-0\store') `
  -GenesisParentStoreRoot E:\mtg-kernel-population-v2-cycle3\lineage\real-attempt-003\run-0\store `
  -GenesisParentGeneration 896 `
  -ArmExecutable D:\cargo-target-cycle4\release\cycle4_arm_v1.exe `
  -RunRecordExecutable D:\cargo-target-cycle4\release\cycle4_run_record_v1.exe `
  -RefreshBuilderExecutable D:\cargo-target-cycle4\release\cycle4_refresh_build_v1.exe `
  -PanelExecutable D:\cargo-target-cycle4\release\deps\mtg_kernel-<hash>.exe `
  -PythonExecutable D:\mtg-kernel-cycle4\venv\Scripts\python.exe `
  -PanelBaseSeed 4100000000 `
  -Device 1
```

Two throwaway Store prefixes are created under the attempt root
(`ladder\a\store`, `ladder\b\store`). Each is bootstrapped from the same
parent and run record and gets its OWN genesis manifest built from its own
Store, so neither rung can read the other's artifacts as an opponent. Each is
then advanced by the same short window and the two are compared: every
relative file's size and SHA-256, the whole store tree hash, the endpoint's
four identity fields, and the two genesis manifests, which must be
byte-identical because the two genesis checkpoints must be. On pass the
attempt root gets an empty `PREFLIGHT_COMPLETE`.

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

The wrapper bootstraps and builds the genesis manifest first if either is
missing (see above). Then, per interval, it asserts the Store's resume
position, runs the arm bin at `position + 128`, asserts the Store advanced
exactly one interval, runs the panel over that interval's manifest roster,
publishes the panel into the refresh chain, and builds the next manifest. `static-rb` runs the panel but
never builds: the wrapper asserts before and after every interval that no
manifest past `refresh-00` exists. On completion the attempt root gets an
empty `TRAINING_COMPLETE`; any failure writes a plain-text `RUN_FAILED` naming
the failing phase.

### Resuming an interrupted attempt

Rerun the same command line. The wrapper works out what is left from three
things: the Store's `latest.json`, the refresh chain's contents, and a
hash-chained journal it keeps per interval (`interval-NN.phase.json` in the
attempt root, written atomically at each of `training-started`,
`training-complete`, `panel-complete`, `manifest-complete`). On start it reads
the journals of every previous non-dry-run attempt under the gate root and
verifies the chain; an edited, truncated or reordered journal stops the launch.

- Interrupted mid-training, so `latest.json` sits on a checkpoint segment
  inside an interval: that interval is resumed toward its ORIGINAL stop
  generation, not the Store position plus 128.
- Interrupted after training, before the panel or the manifest: those are
  finished before anything advances. This is the case that used to skip them
  silently, and at the program end used to publish `TRAINING_COMPLETE` with
  the last panel and manifest missing.
- A panel counts as complete only once its bytes are in the refresh chain, so
  the journal and the chain cannot disagree. A journal that claims a panel the
  chain no longer holds stops the launch rather than silently re-running a
  28-matchup panel.

`TRAINING_COMPLETE` is published only after the Store is at the program end
and every panel and manifest through `-ThroughRefreshIndex` exists and binds
this arm's identity (for `static-rb`, whose panels never enter the chain,
after every interval is journalled panel-complete and no manifest past
genesis exists).

`-ThroughRefreshIndex` (default 16) stops the campaign earlier.

### Dry run

Add `-DryRun` to validate every input, write the provenance records and both
locator files, and print the exact command line of every child without
launching one. A dry run publishes NO terminal marker: its `result.json`
carries `status: "DRY_RUN_PLANNED"`, so a planned campaign can never be
mistaken for a finished one. A dry run over a campaign whose run record does
not exist yet prints the `cycle4_run_record_v1` command and stops there
(`dry_run_stopped_after: "run-record"`). `-SkipHostAssertions` additionally skips the git, toolchain,
and GPU assertions and is accepted ONLY with `-DryRun`.

On a campaign that has not been bootstrapped yet, a dry run prints the two
genesis commands and stops there (`dry_run_stopped_after: "genesis-manifest"`
in `result.json`): every interval's roster, locators and stop generation are
read from manifests those commands would have produced. The printed
`--trainee-run-sha256` is the literal
`<from-arm-origin.record.json-after-bootstrap>` for the same reason.

```
powershell -NoProfile -File scripts\experiments\population_v2_cycle4_v1\run-cycle4-arm-tests.ps1
```

runs the dry-run test suite against a synthetic campaign under the temp
directory.

## The panel locator's slot entries

The wrapper writes two machine-local locators per interval from one slot
table. The arm bin's is identity-keyed
(`mtg-kernel-cycle4-arm-slot-locator/v1`) and unchanged. The payoff panel
runner's is index-keyed (`mtg-kernel-cycle4-slot-locator/v1`) and each slot
entry is one of two shapes:

```json
{
  "schema": "mtg-kernel-cycle4-slot-locator/v1",
  "stores": {
    "0": "E:\cycle4\slot-0",
    "5": {
      "store_root": "E:\cycle4\treatment-rb\store",
      "baseline_chain_dir": "E:\cycle4\treatment-rb\baseline-chain"
    }
  }
}
```

A bare string is a store root and nothing more. The object form adds the
optional `baseline_chain_dir`, and the wrapper writes it on exactly the slots
the interval's manifest binds to the ARM's own run (`current-1` always,
`historical-0` from refresh 4), and only when the arm is `treatment-rb` or
`static-rb`. Those arms' trained own-run checkpoints load only through the
baseline-aware loader, which needs the chain the arm was launched with, so the
value is that arm's own `-ChainDir`, absolute.

`control-r` has no baseline chain, so every slot in its locator is a bare
string and the file is shaped exactly as it was before the field existed.
Other-run slots never carry the field on any arm.

## Known issue: slot generation numbering

The refresh manifest pins slots 2 and 5 to TRAINEE-LOCAL generations
(`896 + refresh_index * 128`), while `resolve_population_opponent_cycle4_v1`
and the panel runner pass `source_generation` straight to the Store as a
STORE generation, and the arm's Store numbers its own generations 0..2048.
For the arm-owned slots the two readings differ by 896. The wrapper writes the
value the manifest validator requires (trainee-local), because no other value
is admissible into a manifest, and reads the hashes at the corresponding store
generation. The 896-offset translation on the reading side is being added
separately in `resolve_population_opponent_cycle4_v1` and the panel runner.
