# Witness Protection validation status

The implementation extends the original UG fixture with its two unchanged
Witness Protection copies. Both pinned decks now resolve 40/40 at registry
admission. This is not yet a completed gameplay or full-set support claim.

## Observed checks

- `py -3.11 -m unittest python.tests.test_limited_decks_v1 python.tests.test_limited_session_v1`:
  21 passed. Original deck SHA-256s, row/copy order and unsupported-deck refusal
  remain covered. The first check exposed three stale coverage expectations;
  updated them and retained independent unsupported-deck refusal coverage.
- Pinned Rust 1.94.1 formatting and `git diff --check`: passed.
- Catalog-only probe, pinned Rust 1.94.1 and existing build-script dependency
  cache: compilation exit 0 in 1.39 seconds; generator exit 0. This compiles the
  card generator, not the engine. Frozen default v32 remains `64c82a261e078f1a`;
  the new Limited v49 is `3eee8a1cbc874e18`.
- The new native profile retains v48 Armor read compatibility, rejects older
  profiles for publication/resume against the new build, and has independent
  literal/live-identity canaries. Their execution remains pending.

## Pending checks

- 24 focused Witness Rust gameplay cases and required prior regressions/CI.
- Strict XMage reference comparisons, including Armor ordering and death LKI.
- Original UG/WG natural external games in both seats and deterministic replay.
- London mulligans are a subsequent implementation batch within the goal.

At 12:30 EDT October 2 both PCs had actual live reservations. Haley supervisor
37648 held the sequential training-comparison window; Jack qualification and
Spellbench work remained live. No heavy local/remote engine build, training,
GPU work or paid allocation was started by this batch. Hosted CI will execute
Rust validation. Preserve other owners and reservations before subsequent jobs.

Catalog probe scratch is registered at `D:/e-scratch/fdn-witness-codegen-001`.
It is cache/scratch, not a sealed evidence source. Its small logs and manifest
will be sealed with the completed batch report before pruning.

## Hosted attempts retained

- CI37036178524 at5e74eb63 rejected an unintended timestamp-field edit to
  `StackSourceContractV4`. Removed those fields in fca96549; its contract stays
  unchanged. Failed Ubuntu/lint logs are retained locally.
- CI37036495331 atfca96549 compiled and ran the first21 focused cases on Ubuntu:
  11 passed,10 failed. Nine failures came from the absent shorthand name
  Snarespinner; the actual fixture is Treetop Snarespinner, base1/4. The tenth
  exposed a wrong death-trigger assumption: Guarded Heir creates Knights on
  entry. Corrected the fixture name/stats and used Clockwork Percussionist's
  actual dies trigger for loss/restoration tests. No gameplay-suite pass is
  claimed from this attempt.
- Three additional focused cases cover changed-host versus same-host Equipment
  timestamps and lifelink keyword counters on either side of ability removal.
