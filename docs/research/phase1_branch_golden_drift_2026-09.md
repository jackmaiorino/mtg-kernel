# Phase 1 branch golden drift (2026-09-16)

Branch `lead/phase1-golden-repins-v1`, worktree `E:/mtg-kernel-phase1-w8a`, created from
`codex/learned-sideboarding-integration-v1` at `123311ef` (Phase 1 tip). Task: two lib-test
goldens failed on this branch independently of any work in this session; date the root cause
for each, decide legitimate versus unintended, and re-pin only if legitimate.

## Method

For each test: `git log -S<pinned-hash> -- <file>` found the commit that last set the pin;
`git log --oneline <that commit>..<tip> -- <files the test's call graph touches>` enumerated
every candidate commit; diffs were read for each candidate to classify it as additive/inert,
a pure refactor, or a real behavior change. Where the diff alone was not conclusive, the
worktree checked out the candidate commit detached, built it (`CARGO_TARGET_DIR=E:\cargo-target-phase1-w8a`,
`TEMP`/`TMP=E:\tmp\lead`, `--offline --locked --jobs 4`, `BelowNormal` priority via a PowerShell
`Start-Process` with `-Wait`, logs under `E:\tmp\lead\logs`), and ran the single failing test to
read its live actual/expected values, then returned to the branch. Three probe commits were
built this way: `0bfe680a`, `436c62b2`, `a70f0985` (plus the tip, `123311ef`, for the live
re-pin values). Every value below is read from a live assertion output, never hand-derived.

## Test 1: `async_flat_scored_rollout_v1::tests::first_five_scorer_packets_have_exact_safe_golden`

File: `mtg-kernel/src/async_flat_scored_rollout_v1.rs`.

### When the pin went stale

`git log -S78289df65db7f4464e1107fdcbb7703c9ad2512aa3246fc9b5bb82475d103c81 -- mtg-kernel/src/async_flat_scored_rollout_v1.rs`
finds exactly one commit: `af4f4574` (2026-09-10 17:54:14 -0400, "tests: stale CARD_DEFS.len()
literals, wave-1 golden re-pins, seven-test reconciliation"), the wave-1 catalog re-pin. That
value was still correct there and stayed correct through the next two days of Phase 1 branch
work.

Bisection:

| Commit | Date | Live actual value | Matches pin? |
|---|---|---|---|
| `af4f4574` (pin set) | 2026-09-10 17:54 | `78289df65db7...5103c81` | yes (by construction) |
| `0bfe680a` "Add learned sideboard head and executable frozen-play BO3 workflow" | 2026-09-12 11:57 | `78289df65db7...5103c81` | yes (built and ran; test passes) |
| `436c62b2` "Add explicit V6 observation and V3 frozen-play feature transfer" | 2026-09-12 13:05 | `3d0676137e2bc69ed1fbe63491379b62307398505518cc73c1b478e4d7c87b72` | **no** (built and ran; first drifted value) |
| `2d8d564c`, `7999d377` (Codex, verified by the lead before this task) | 2026-09-15 | `3d0676137e2bc69ed1fbe63491379b62307398505518cc73c1b478e4d7c87b72` | no, same as `436c62b2` |
| `7dc364d7` (pre-merge Phase 1 tip) | 2026-09-15 22:11 | not independently re-probed; no commit between `436c62b2` and `7dc364d7` touches `flat_policy_v1.rs` (`git log --oneline 436c62b2..7dc364d7 -- mtg-kernel/src/flat_policy_v1.rs` is empty), so it is inferred stable and this is consistent with the confirmed value at `2d8d564c`/`7999d377` | no |
| `123311ef` (this branch's tip, after the card-lane catalog merge) | 2026-09-16 00:43 | `f4468293c68ef2b62557aabf03afdd8056a9fae1a9d4258ec4cc708c7f6b84a4` (built and ran at the tip) | **the pin already in source matches**; no re-pin needed |

**Root-cause commit: `436c62b2`** (2026-09-12 13:05:54 -0400, "Add explicit V6 observation and
V3 frozen-play feature transfer").

### Mechanism

`436c62b2` adds two variants to `pub enum FlatActionObjectGroupV1` in `rl_session.rs`
(`DecisionLocalLibrary = 11`, `HistoricalPublicSource = 12`). Making the enum's existing
matches exhaustive requires a two-line addition to `mtg-kernel/src/flat_policy_v1.rs` (a new
match arm returning the same `false` as the pre-existing `Command` arm). `mtg-kernel/build.rs`'s
`flat_policy_contract_codegen` computes `typed_layout_digest = sha256_bytes(fs::read(flat_policy_v1.rs))`,
a byte-for-byte hash of the whole source file, and bakes it into the compiled V1 feature
contract that `decision.contract()` returns and this test hashes. `data/flat_policy_v1/feature_inventory_v1.json`'s
`rust_typed_layout_sha256` was regenerated in the same commit
(`b4de06cab5644...` -> `bcde548e60dcf...`), the standard "edit contract source, regenerate,
`--check` clean" workflow already established on this branch for `KERNEL_CARDDB_HASH` moves.

### Legitimate or not

**Legitimate.** Confirmed by direct source inspection that no pre-existing V1/V2 runtime
encoding behavior changed: every reference to the two new enum variants added in `436c62b2`
(in `flat_policy_v1.rs`'s match, `flat_policy_v2.rs`'s `build_cache`, and
`action_ingress_admission_v2.rs`'s match) is unreachable from the plain V1/V2 pipeline
`run_async_flat_scored_rollout_v1(config(1,1,1,1), ...)` exercises; objects carrying these new
groups are constructed only by the new `register_extensions_v3`/`build_scoring_owned_v3`
functions the same commit adds, which this test's call graph never reaches. `scorer.counts`
(the test's structural-shape assertion) is unaffected, matching the same pure-identity-move
pattern the test's own historical comments already document for every prior
`KERNEL_CARDDB_HASH` re-pin.

### Action

**None.** The pin already in source at the tip, `f4468293c68ef2b62557aabf03afdd8056a9fae1a9d4258ec4cc708c7f6b84a4`,
was set by the card lane's own merge re-pin (`docs/research/phase1_card_lane_merge_repin_record_2026-09.md`,
commit `b6b916f0` on `lead/phase1-card-lane-merge-v1`) against a tree that already carried
`436c62b2`, so it already reflects this root cause plus the catalog identity move. The live
build at `123311ef` confirms this pin is correct: `... ok`. This golden was already green at
the start of this task once the card-lane catalog merge had landed; the task brief's own quoted
"pinned 78289df6..." value predates that merge.

## Test 2: `native_checkpoint_inference_v1::checkpoint_reliance_probe_v1::action_block_gradient_diagnostic_v1::joined_frame_is_preflight_sealed_neutral_and_lineage_complete_v1`

File: `mtg-kernel/src/native_checkpoint_inference_v1/checkpoint_reliance_probe_v1/action_block_gradient_diagnostic_v1.rs`.

### When the pin went stale

`git log -Sae853cabe8cb59c0aa44e93e142d11963d4cc57893a1d9b27ffe7fedc6366a71 -- <same directory>`
finds the same commit, `af4f4574`, as the last one to set that value.

Bisection (same probe commits as test 1, same build):

| Commit | Date | Live actual value | Matches pin? |
|---|---|---|---|
| `af4f4574` (pin set) | 2026-09-10 17:54 | `ae853cabe8cb...6366a71` | yes (by construction) |
| `0bfe680a` | 2026-09-12 11:57 | `ae853cabe8cb...6366a71` | yes (built and ran; test passes) |
| `436c62b2` | 2026-09-12 13:05 | `adfdb3919bdffca4dcdaa134edbc7bbdebb1a826b6e2d50b5c9f7e22b13c8966` | **no** (built and ran; first drift) |
| `a70f0985` "Connect expanded-deck native training, value observations, and grouped sideboard data" | 2026-09-12 16:20 | `f645d487a3efcae3dc201e23ee057463f1def77801906dd0197c4972a67b8d40` | **no, a second, distinct drift** (built and ran) |
| `7dc364d7` (pre-merge Phase 1 tip; value from the card-lane merge agent's own disposable-worktree diagnostic, `docs/research/phase1_card_lane_merge_repin_record_2026-09.md`) | 2026-09-15 22:11 | `cd71f6a0ed8b1f515ffaf4c1ef6fa10a195804f7b8b60602afa11cd71a1d7bb6` | no, a third, distinct value |
| `123311ef` (this branch's tip) | 2026-09-16 00:43 | `ba4b3568f7d93f9485b2a6cb4a00f71372f99f912ae3bb1c41aee31a65ab7c59` (built and ran at the tip; matches the card-lane merge agent's own independently obtained value for the merged tree exactly) | no, a fourth, distinct value |

Unlike test 1, the value keeps moving after the first drift commit: there is no single
root-cause commit, but a continuous run of intentional V2/V3/V4 flat-policy observation-contract
development commits, each of which edits `flat_policy_v2.rs` (or the V3/V4 files layered on
it) and, by the contract's own byte-for-byte design, moves the compiled-in identity this test's
fixture embeds.

### Mechanism

This test's fixture chain constructs a `FlatDecisionBindingV2` (imported directly from
`flat_policy_v2`), whose `contract_digests: FlatPolicyContractDigestsV2` field carries
`base_typed_layout_sha256` (raw SHA-256 of `flat_policy_v1.rs`) and
`overlay_typed_layout_sha256`/`typed_layout_sha256` (raw SHA-256 of `flat_policy_v2.rs`,
domain-separated composite). `436c62b2` moves the base layer (same cause as test 1, first
drift). Every later commit in the branch's active V3/V4 observation-contract work that edits
`flat_policy_v2.rs` moves the overlay layer again, each one regenerating and re-committing
`data/flat_policy_v2/feature_inventory_v2.json` alongside its source change (confirmed directly
in the `a70f0985` diff: `overlay_sha256` moves from `4f5a79784...` to `4d0278465...` while
`base_sha256` stays at `436c62b2`'s value, i.e. only the V2-specific layer moved that time,
consistent with `a70f0985` never touching `flat_policy_v1.rs`). Every such commit within
`af4f4574..123311ef` that touches `flat_policy_v2.rs`:

`436c62b2`, `5ca1c733` ("Preserve initiative trigger controller snapshots in V3
observations"), `cbfa2435` ("Fix V3 goad legality and postboard graveyard and creature-cost
observations"), `a70f0985`, `0f479bad` ("Expose exact Ward payment bindings through V3
observations"), `0f82a02c` ("Keep scoring diagnostics inside V3 to preserve frozen V2 source
identity"), `3071597c` ("Capture opt-in actor-visible V3 scoring failures by stage"),
`a88b2195` ("Resolve pending-trigger targeting sources by their frozen contract in the V3
action slice"), `72c1cd6d` ("Add the V4 registry validator, V4 action-slice encoder, and V4
tensorizer"), `e5902257` ("Fix the shared-physical-source collision in
register_extensions_v4"), plus the card-lane merge commit `123311ef` itself (which brings
636 more lines into `flat_policy_v2.rs` and regenerates `feature_inventory_v2.json` again).

The fixture also directly imports `crate::card_def::KERNEL_CARDDB_HASH`, so the same catalog
identity move (`0xde59_c501_e943_f3fd` -> `0x064a_7c98_9255_ab3c`) that the card-lane merge
already re-pinned once (to `9a19af3bb5da...`, now superseded) contributes an additional,
independent legitimate move at `123311ef`.

### Legitimate or not

**Legitimate.** Every commit-message and diff in the list above was read. Each describes
intentional new V3/V4 observation-contract feature work, or that work's own internal bugfix
(pending-trigger targeting by position, goad legality, `KnownOpponentHand` action ordinals,
combat-participant visibility), scoped to the new V3/V4 registry/projection machinery added
alongside `436c62b2`. None changes this test's own file, `action_block_gradient_diagnostic_v1.rs`
(untouched by any commit in `af4f4574..123311ef` other than the merge commit's own inert
match-arm completion in the neighboring `action_ingress_admission_v2.rs`), and none changes a
pre-existing V1/V2 gameplay or encoding path. The one non-doc, non-flat-policy touch inside this
window, `cbfa2435`'s two-line change to `mtg-kernel/src/engine.rs`, is a bare visibility change
(`fn required_goaded_attackers` -> `pub(crate) fn required_goaded_attackers`) with no logic
change. Several of the V3/V4 commits (`a88b2195`, `9de29f94`, `81cdbab7`, `83dd0107`,
`11181bf8`, `7dc364d7`) are themselves adversarial-review bugfixes for the new V3/V4 layer,
each with a formal-evaluator seed that reproduced the bug and a root-cause writeup in its own
commit message; none of them touches or is reachable from this test's fixture except through
the shared, by-design contract-identity hash.

### Action

**Re-pinned** to `ba4b3568f7d93f9485b2a6cb4a00f71372f99f912ae3bb1c41aee31a65ab7c59`, the live
value read from the failing assertion at tip `123311ef`, with a dated 2026-09-16 comment added
above the assertion in `action_block_gradient_diagnostic_v1.rs` naming `436c62b2` as the first
drift and the V3/V4 development sequence as the reason it kept moving.

## Verification

`cargo test --offline --locked --jobs 4 -p mtg-kernel --lib -- async_flat_scored_rollout_v1::
native_checkpoint_inference_v1::checkpoint_reliance_probe_v1:: --test-threads=4` (official target
dir `E:\cargo-target-phase1-w8a`, `BelowNormal` priority): **91 passed, 0 failed, 3 ignored**
(37 in `async_flat_scored_rollout_v1::`, 54 in
`native_checkpoint_inference_v1::checkpoint_reliance_probe_v1::`; the 3 ignored tests are
pre-existing, unrelated "official no-training diagnostic" / "external Store diagnostic" tests
gated behind `--ignored`). Both named goldens pass by exact name inside this run.

## Commands run (from `E:/mtg-kernel-phase1-w8a`)

Environment for every build: `CARGO_TARGET_DIR=E:\cargo-target-phase1-w8a`,
`TEMP=E:\tmp\lead`, `TMP=E:\tmp\lead`, `--offline --locked --jobs 4`, `BelowNormal` priority via
a PowerShell `Start-Process` (started without `-Wait` so `PriorityClass` could be set on the
returned process object, then `WaitForExit()`), stdout/stderr redirected to files under
`E:\tmp\lead\logs`.

1. `git checkout -b lead/phase1-golden-repins-v1 codex/learned-sideboarding-integration-v1`
   (tip `123311ef`).
2. `git log -S<pin> -- <file>` for each golden's currently-inherited pin value, both resolving
   to `af4f4574`.
3. `git checkout --detach 0bfe680a`; `cargo test --lib -- async_flat_scored_rollout_v1::tests::first_five_scorer_packets_have_exact_safe_golden --exact`
   and the same for the test 2 filter: both pass, confirming no drift yet.
4. `git checkout --detach 436c62b2`; same two filters: both fail, first drift for both goldens.
5. `git checkout --detach a70f0985`; test 2's filter only (test 1 already established stable
   downstream of `436c62b2` by the task brief's own prior verification at `2d8d564c`/`7999d377`):
   fails again, a second distinct value, confirming test 2's identity keeps moving.
6. `git checkout lead/phase1-golden-repins-v1` (back to the tip, `123311ef`); both filters
   together: test 1 passes unedited, test 2 fails with the value re-pinned above.
7. Edited the test 2 assertion and comment in
   `mtg-kernel/src/native_checkpoint_inference_v1/checkpoint_reliance_probe_v1/action_block_gradient_diagnostic_v1.rs`.
8. `cargo test --lib -- async_flat_scored_rollout_v1:: native_checkpoint_inference_v1::checkpoint_reliance_probe_v1:: --test-threads=4`:
   91 passed, 0 failed, 3 ignored (pre-existing ignores).
