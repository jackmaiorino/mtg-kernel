# D6 working result

## Local branch

- Branch `claude/cube-m4-d6-20261125`, head `cc6fa6e1423e3e9756a16b83dc8c578391547eb3`.
- D4 `1a6f9427e92871467336aec3819fb68b50cafd17`, with D1 `040a1f5e81e819e0838901bee6f3e48ca7649afe`, D2 `1b389358c29576399406fd3a4c96c5a6b7aae903`, and D2a `59b7c5a884dcbc5e1dad49bd19437ffc0b78c5af`.
- Previously merged D3 `b309912d5a5233040c1f8a16787de72c8b123da6` and D5 `c76694ab58b378240333054da19202ae675c1fce`.

D6 calls the real D4 replay and optimizer with chairs in pod and seat order, reconstructed pre-pick states, matched sibling scores, and the pinned snapshots used to collect the rows. Checkpoints carry per-set parameters, both moments, Adam steps and D4's six counters. Fixture part 3 uses D4's enumerated derivatives and D5's game retry, cap classification and stop ledger path.

## Verification

- The final-head optimized Cargo D6 suite passed all 11 tests in 12.56 seconds. The recovery fixture compares every file after simulating the interrupted publication boundary. It checks the 2,240-game window and the two coupled games sharing the injected halt seed.
- Python runner tests passed, 39 tests and 19 subtests.
- Scoped Rust formatting and git diff checks passed. Workspace formatting reported inherited differences in D5's example, pod runner, read-plan and tests.
- D4's focused optimized test harness passed 11 tests. D3's focused sampler and seat harness passed 9 tests. The isolated final commit parser passed its complete, absent, torn, incomplete and corrupt record checks.
- Default workspace Clippy passed. The feature-enabled Clippy recheck reports four inherited D5 diagnostics and none in D6.
- Verification logs are under `target`, including `d6-optimized.log`.
- Optimized D1 integration ran 7 tests. Six failed at the pinned table's card database check, which expected `4836b459b81e9350` and received `7fd442a712f0cc4b`. One fetch test was ignored as declared. The known D1 empty index pin is independently owned. The focused D2 forward test confirmed the missing golden file.

## Remaining integration

D5's per-decision liveness and cap record field changes remain needed for live D0 play. Full-mode D6 rules take explicit count and rate thresholds. The supplied inputs do not specify the exact signed A2.5 rate formula or who turns the authorized census baseline into the run's threshold rules. D8 still owns the live host receipts and signed-input binding.

## Loader and A2.5 follow-up

Local head `956fb9381879e4f14a9535be55501c7ccb16e623`.

- D16 can call `load_checkpoint_read_only_v1(path, &CubeTableV1)` for the verified checkpoint, model and bound rows. The loader checks schema, layout, state digest, index pin, table file pin and compiled card database identity.
- Full mode pins an authorized census-b summary by its file SHA-256 and binds its build and fence to the run's binary and fence pins. Resume loads the same pinned summary. This replaces caller-entered rules. Ordinary stop and card rates use eligible slots and signed A2.5 thresholds. Q9 and one-event stop handling have integration coverage.
- The census summarizer emits a `tripwire` rollup only when events must be excluded. Summaries containing excluded slots require this rollup and a pin for the regenerated summary. Old summaries with entirely eligible rows retain their shape.
- Optimized library built with LTO disabled and 16 codegen units. The focused driver harness passed 14 tests with warnings denied, and the focused rule harness passed 7 tests. Python passed 53 tests and 19 subtests. Scoped formatting and the diff check passed.


## D6 review round 2

- D3 model changes kept: `assert_eq!(hidden.trunk(), TrunkV1::Pick)` in
  `PickForwardV1::from_cached_hidden` and `assert_eq!(hidden.trunk(), TrunkV1::Build)`
  in `BuildForwardV1::from_cached_hidden`. These enforce trunk identity in release builds.
- The launcher must drop `TrainingDriver` before a clean exit, including before
  `std::process::exit`. Skipping destructors leaves the active marker and the next
  open requires host-loss authorization. A consumed entry remains recorded after
  a clean drop. Those receipts are separate from deterministic training artifacts.
- Stop records name their zero-based `batch_index`. Checkpoint `update` remains
  the checkpoint number. Full-mode admission takes the summary, manifest and
  authorization files and verifies all three pins and their manifest binding.
