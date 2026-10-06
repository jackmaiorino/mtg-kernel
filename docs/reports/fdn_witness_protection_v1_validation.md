# Witness Protection validation status

## Current snapshot-storage repair

Windows job `111200190062` at published source `7b02fc92` failed only the
isolated snapshot timing check: 64.622 microseconds against the frozen 40
limit, despite already using the high-priority timing child. Focused Witness
rules and every other workspace test summary passed. Later native, Limited
and CUDA steps were skipped, so the overall job is a failure. Its complete
terminal log is retained as `fdn-rust-current-111200190062.log`.

The prepared backport copies the three arena-storage source files from
`92881658`. `Arena<T>` shares `Arc<Vec<T>>` storage and detaches before every
mutation. IDs, value hashes and legacy JSON remain value based. Mutating APIs
require `T: Clone`, which `GameObject` implements. Arena tests cover every
mutation entry point and legacy serialization; the strengthened GameState
round trip checks captured bytes and independent mutation after restore.
The first mutation of shared storage still clones the objects.

The entire frozen 80-object workload and timing function, dependency versions,
lockfile, compiler/linker pins and card behavior are unchanged. The canonical
source has passed its complete Linux runtime matrix in job `111188356118`,
including the arena tests, strengthened restore and original-deck replay;
the frozen snapshot measured 223 nanoseconds. All four canonical Python shards
and formatting/lint passed. Canonical Windows runtime qualification remains
active. These results support the backport but do not qualify this prefix's
new source or Windows timing. It requires its own full hosted matrix.

The Witness Linux job `111200190079` at `7b02fc92` remains active. Keep that
job running and hold publication of the prepared repair until no current-run
job is executing. Earlier entries below record previous source qualification.

The implementation extends the original UG fixture with its two unchanged
Witness Protection copies. Both pinned decks now resolve 40/40 at registry
admission. This is not yet a completed gameplay or full-set support claim.

## Observed checks

- `py -3.11 -m unittest python.tests.test_limited_decks_v1 python.tests.test_limited_session_v1`:
  21 passed. Original deck SHA-256s, row/copy order and unsupported-deck refusal
  remain covered. The first check exposed three stale coverage expectations;
  updated them and retained independent unsupported-deck refusal coverage.
- Pinned Rust 1.94.1 formatting and `git diff --check`: passed.
- CI37038033614 at a6b639d0: focused Witness Rust steps passed on Ubuntu and
  Windows; formatting/lint and all four Python shards passed. The full Rust
  release and native-store stages passed on both hosts. Ubuntu then failed
  `card_def::tests::card_defs_len_matches_pool`: the extended catalog contains
  206 definitions, while the assertion retained the Armor count of 205.
  Correct the Limited expectation to 206; the default expectation remains 162.
  All 19 Limited integration targets, seven session restore tests and 66 RL
  session tests had passed before that assertion. Subsequent checks did not
  execute in that failed attempt; corrected-source results are recorded below.
- CI37048888583 at fdbe23e5: Ubuntu job 110977578991 completed successfully at
  20:29:36UTC. Its full log confirms all 20 selected Limited integration targets,
  334 passing cases including 24 Witness cases, 1705 default library tests,
  seven session restore, 66 RL session and 46 card-definition tests. The Koma,
  threshold, 143 record tests and host-safe CUDA commands passed. Existing
  default/record/CUDA exclusions remain 43/3/7 ignored tests. Corrected-source
  Windows verification is still running.
- Mage run37049882555 at 5cc4decd8ffe passed all 146 comparisons in 16 selected
  classes, including 16 Witness, 7 London and 28 combat cases, with no failures,
  errors or skips. Source/output hashes and XML counts were independently
  verified. Mage PR15 has green current-head checks at d98525a0a7c. Retained
  evidence and mirror are listed in the gameplay validation matrix.
- Catalog-only probe, pinned Rust 1.94.1 and existing build-script dependency
  cache: compilation exit 0 in 1.39 seconds; generator exit 0. This compiles the
  card generator, not the engine. Frozen default v32 remains `64c82a261e078f1a`;
  the new Limited v49 is `3eee8a1cbc874e18`.
- The new native profile retains v48 Armor read compatibility, rejects older
  profiles for publication/resume against the new build, and has independent
  literal/live-identity canaries. Their Windows-only execution remains pending.

## Pending checks

- Complete corrected-source Windows Rust regressions and publication/resume
  canaries, followed by current-head CI readiness.

Original UG/WG external games, exact replay and pending London restore passed
on Ubuntu in London PR139's full Rust job at d43d59a2 and on Windows in PR140's
compatible Bash step, job 111036127744 at 36d54594, completed 22:12:10 UTC.

At 12:30 EDT October 2 both PCs had actual live reservations. Haley supervisor
37648 held the sequential training-comparison window; Jack qualification and
Spellbench work remained live. No heavy local/remote engine build, training,
GPU work or paid allocation was started by this batch. Hosted CI will execute
Rust validation. Preserve other owners and reservations before subsequent jobs.

Catalog probe and focused launcher logs are sealed in
`E:/mtg-fdn-fixtures/fdn-witness-preflight-001`, with a hash-verified independent
mirror at `C:/Users/Jack/fdn-witness-preflight-001-sealed`. The pinned probe
executable is recorded by SHA-256 in `seal.json`. Two raw executions reproduced
identical output bytes; the earlier PowerShell log has the same two identities
with CRLF line endings. All failed hosted attempt logs remain sealed too.
Only owned scratch copies and uncited debug symbols were pruned after mirror
verification: 2,973,168 bytes. The exact receipt is
`fdn_witness_preflight_001_prune.json`.

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
- CI37037404413 at e51c66bd rejected a test that counted exile through a
  nonexistent player field. Fixed the assertion to inspect the object arena
  in a6b639d0. Failed lint output remains retained.
- CI37038033614 at a6b639d0: the focused Witness Protection step completed
  successfully on Ubuntu and Windows, and formatting/lint passed. Full Rust
  and Python jobs are still active; final logs and regression results remain
  pending.

## CI prerequisite repair

Armor CI37031403320 exposed a Windows-only timing assumption in
`VerdictTests.test_m1_sentinel_covers_every_placement_and_arm_at_full_length`:
the fake trainer can legitimately rank serial faster under process-startup
contention. This test checks sentinel coverage for a selected two-device
allocation, not host speed. Give that test deterministic projected ranking
inputs while still running every prefix/full-length process, output comparison,
and rejection mutation. Production ranking and the separate fastest-allocation
test stay unchanged. The repaired focused test passed on Jack's PC in 11.616s.
Hosted Windows shard 111036193230 at Armor source f13fd955 passed the repaired
case and all 286 tests; all four current Python shards passed. Armor is ready
for review using its unchanged Rust code's passing Ubuntu/Windows evidence.
The active Witness workflow is preserved for the remaining full Windows checks.

## Current-main integration

The Witness prefix incorporates Armor d4f02b41 and main's
192-definition/v34 catalog. Its generated Limited identity is 236
definitions/v51 bd1385731e43c4a1; the appended Witness ID is 235. Original and
prior composed records remain readable; mutations require the live identity.
Layer timestamp fields coexist with Adventure eligibility. The custom object
hash retains main's Adventure bit before appending optional layer stamps.
The typed reference walk retains the removed-ability event's physical object;
a regression distinguishes the referenced object from an unrelated one.
Catalog generation, formatting, workflow lint and diff checks pass locally;
composed-source rules, replay and complete Rust qualification are pending.
