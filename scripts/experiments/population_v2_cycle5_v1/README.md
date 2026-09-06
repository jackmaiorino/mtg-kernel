# Cycle-5 arm launcher (uncompiled draft, branch `lead/cycle5-launcher-v1`)

Mirror of `population_v2_cycle4_v1` for the cycle-5 program whose parent the
immutable cycle-4 routing record names (`routing-record.json`, SHA-256
`f9ad13e99d3040f50bff66f1d49c4a7288d226d42186c767b58e7902870f5b2f`: NO_CARRY,
parent run `f25a63d0...` at its own generation 2048, recipe control-r v3).

Status on 2026-09-06: written, committed, NOT compiled and NOT run (the
machine is reserved for a Codex lane until a build window is granted). The
first build will surface renaming and import slips; the design points below
are the ones a reviewer should check, not the compiler.

## What is here

- `mtg-kernel/src/native_training_store_run_v2.rs`: additive contract
  sections `population_program_v2_cycle5` and `trainer_v5_candidate`, the
  cycle-5 literals, and validators that fail closed while any literal is an
  `UNRATIFIED:` sentinel. `cfg(test)` twins carry fictitious values so the
  unit tests exercise the whole path.
- `native_cycle5_run_record_v1.rs` + `src/bin/cycle5_run_record_v1.rs`:
  builds one arm's `run.json` from the arm kind, the routing record (bytes
  must hash to the pinned record; parent run, checkpoint manifest and
  generation cross-checked against the staged parent), the parent Store and
  the arm launcher's own build identity.
- `native_cycle5_arm_v1.rs` + `src/bin/cycle5_arm_v1.rs`: the cycle-4
  launcher's frozen v3 path continued from the routed parent. Arm kinds:
  `control-v3` (supported end to end) and `centered-v5` (declared; every
  launch path refuses it until the v5 trainer contract is ratified and
  implemented). The v4 baseline-chain machinery is removed; the chain
  directory still holds the origin record.
- `native_population_refresh_manifest_cycle5_v1.rs`,
  `native_population_refresh_builder_cycle5_v1.rs`,
  `src/bin/cycle5_refresh_build_v1.rs`: the refresh manifest and builder with
  the roster gated as unratified.
- `run-cycle5-arm.ps1`, `common.ps1`, `run_payoff_panel_v1.py` and its tests:
  the cycle-4 wrapper stack with the cycle-5 arm names, the routing record as
  a required input, `PanelMatchupWorkers` defaulting to 12, the wrapper
  pinning itself (and so every child) to the P-cores, the existing arm
  build-identity versus launch-commit assertion, and a required mailbox read
  receipt for every formal non-dry launch.

## Gates that fail closed until the owner rules

| Literal or gate | Where | Production value | Effect while unratified |
|---|---|---|---|
| `CYCLE5_PREREG_SHA256_V1` | run_v2 | `UNRATIFIED:cycle5-prereg-sketch` | no cycle-5 record validates |
| `CYCLE5_TRAINER_V5_CONTRACT_DOCUMENT_SHA256_V1` | run_v2 | `UNRATIFIED:trainer-v5-contract-draft` | any record carrying `trainer_v5_candidate` is refused |
| `Cycle5ArmKindV1::formal_base_seed_v1` | arm module | placeholder 0 | the arm validator refuses every record (`cycle5_arm_v1_base_seed_unratified`) |
| `CYCLE5_ROSTER_RATIFIED_V1` | refresh manifest | `false` | no manifest builds or decodes (`RosterUnratified`) |
| `centered-v5` arm | run record and arm module | declared only | refused before any I/O |

## Assumptions fixed without a ruling (numbered for the owner)

1. Trainee start generation 2048 and the routing record's parent are the
   genesis (fact from the routing record); the trainee stop generation is
   4096, i.e. the cycle-4 per-arm scope of 2,048 successful updates carried
   forward (sketch open decision 5).
2. Refresh interval 128 and maximum refresh index 16 carried from cycle 4.
3. Arm kinds named `control-v3` and `centered-v5`; both refresh (no
   static-pool arm in cycle 5), so `static_pool` is always false.
4. The roster VALUES are the cycle-4 frozen eight-identity genesis pool
   (sketch open decision 3's default option), gated as unratified.
5. The v5 trainer loss identity string `terminal_reinforce_value/v5-candidate`
   and the `trainer_v5_candidate` field set (`loss_identity`,
   `baseline_schema`, `estimator_kind`, `window_updates`,
   `contract_document_sha256`, `numerical_backend`) follow the contract draft.
6. Formal base seeds are not chosen; the `cfg(test)` twins 990000 and 991000
   are fictitious and must never be promoted.
7. The routing record is bound by its whole-file SHA-256 and by the parent
   fields it names; its `recipe` must be the v3 control recipe with
   `centered_baseline` false.
8. The mailbox read receipt is a JSON file with `read_at_utc`, `last_entry`
   and `hold_open`, at most 30 minutes old, written by the operator after
   reading the mailbox; the wrapper cannot read the mailbox itself.
9. The wrapper's launch-commit assertion remains the cycle-4 one (the arm
   executable's `--print-build-identity` commit must equal the launch
   commit); the run-record and refresh-builder executables are not asserted
   the same way because they do not print a build identity.
10. Chain filename scheme (`refresh-NN.manifest.json`, `refresh-NN.panel.json`)
    unchanged; the panel runner's schema strings carry `cycle5`.

## Not done

- No compilation, clippy, fmt or test run yet (build window pending).
- No `docs/native_cycle5_arm_launcher_v1.md`; the cycle-4 document plus this
  README are the description of record until one is written.
- No preflight ladder run, no scorer preflight against a cycle-5 Store.
