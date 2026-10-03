# FDN fixture gameplay milestone checks

## Current delivery frontier

The complete implementation stack incorporates main fe479186 and all original
fixture card/token dependencies, combat, pending choices and London mulligans.
The current published CI composition is `92881658`, including the arena storage
change described below. Its hosted qualification is in progress in run `37115415284`.
Full qualification of that source remains pending. Earlier isolated-stack
passes below do not qualify this change.

The cards, London rules, original decks, Voyage walker correction and V6
observation/prompt repair retain the previously tested behavior. London
`f904c95c` and CI `7cbfa1cd` repair the Windows profile import and reservation
regression. The report branch incorporates CI `92881658` and differs only in
Markdown. Its earlier report-only Rust skips provide no gameplay proof.

Windows job `111147114674` at `7cbfa1cd` passed all 24 Witness and 12 London
integration cases, the three original external games and the repaired private
bottom-menu library restore. All three complete game receipts equal the prior
Linux receipts recorded below. The job then failed the unchanged isolated
snapshot check at 62.836 microseconds against 40, despite the timing-child
priority change. Later native/Limited/CUDA steps were skipped, so the overall
Windows job remains a failure. All four Python shards at that source passed.

Linux job `111147114731` at the same `7cbfa1cd` source completed the entire
default/native/Limited/host-safe CUDA matrix successfully. All 21 selected
integration targets passed 347 cases with no integration failures or ignores.
The isolated snapshot measured 7.084 microseconds. All three complete game
receipts equal Windows `7cbfa1cd` and the prior Linux receipts; repeated seed123
is exact and swapped seed701 ends naturally. The private bottom-menu restore
also passed. These results qualify the earlier Linux source, while the Windows
failure and the new arena's hosted qualification remain outstanding.

London source `f904c95c` also completed its full Linux matrix in job
`111162992479`, including all selected integration targets and affected
library filters without failures. The frozen snapshot check measured
7.421 microseconds. All three complete original-game receipts equal the
`7cbfa1cd` baseline, including exact repeated seed123 replay and the natural
swapped seed701 terminal. Private bottom-menu restoration passed. Its Windows
matrix is still running. This qualifies the London source on Linux and does
not qualify the later arena storage change in CI `92881658`.

CI `92881658`, with code introduced by `48b51912`, shares the arena's storage through
`Arc<Vec<T>>`, detaching before any mutation. This avoids copying every object
at snapshot capture; the first mutation of shared storage still clones the
objects. Two arena regressions cover every mutation entry point, stable append
IDs, legacy JSON and value hashing. The snapshot round trip now checks captured
bytes and independent mutation after restore. Serde's `rc` feature changes no
dependency version or lockfile. The entire frozen 80-object workload and timing
function remain unchanged. Formatting and diff checks pass. Hosted lint job
`111181187834` passed formatting and all four release Clippy gates: default,
native Store, Limited and host-safe CUDA compilation, using Rust 1.94.1.
The regressions, original games, frozen timing check and full runtime matrix
must still qualify this change.
Mutating arena APIs now require `T: Clone`, already implemented by `GameObject`.
The changed arena module's five unit tests passed separately on Windows with
Rust 1.94.1 and the workspace's exact dependency versions/checksums, including
both mutation-isolation and legacy-serialization cases. This narrow standalone
check does not qualify full-engine restoration, gameplay or snapshot timing.

At current CI source `92881658`, Windows Python shard1 job `111188356138`
built the real release JSONL environment and passed 373 selected tests with
one skip, no failures, in 930.347 seconds. The repaired WMI descendant case
passed. This qualifies that Python shard and default environment at the new
source; it does not qualify the Limited original games, pending restoration,
frozen snapshot timing gate or complete runtime matrix. The other three
Python shards remain pending.

Report source `49ed45c0` completed all four Python shards in run `37109727043`.
Each shard built the real release JSONL environment. Its Python tests, runner
and workflow match CI `7cbfa1cd`; that tested source differs only in Markdown.
Full terminal logs confirm the following results:

| Python shard at49ed45c0 | Selected tests | Skips | Failures | Seconds |
| --- | --- | --- | --- | --- |
| Ubuntu0, job111181847843 | 374 | 14 | 0 | 502.931 |
| Ubuntu1, job111181847813 | 373 | 13 | 0 | 671.629 |
| Windows0, job111181847797 | 374 | 0 | 0 | 853.563 |
| Windows1, job111181847792 | 373 | 1 | 0 | 1130.100 |

Ubuntu executes 720 tests with 27 skips; Windows executes 746 with 1 skip.
The unchanged original-deck copy-order resolution cases pass on both platforms,
and the repaired WMI descendant-reservation case passes on Windows. These
results qualify the Python tests and procedure at that prior source. They do
not qualify the changed arena's environment or replace its full Rust matrix.

Windows job111116304498 atcd7f451b passed24Witness cases and all12London
integration cases. The original external game test verifies the pinned input
hashes below, runs seed123 twice with complete receipt equality, and runs
swapped seed701. All three receipts report natural/natural_game_over and a
P0 win. Each game performs four explicit bottom choices.

| Earlier Linux/Windows game atcd7f451b/7cbfa1cd | Requests | Physical decisions | Policy steps | Transcript SHA-256 |
| --- | --- | --- | --- | --- |
| Seed123, twice with identical complete receipts | 545 | 538 | 544 | 7738afcf0a9afff429a1e2e699e631897d2d8a9faba13f698a12a1b04d95c6e1 |
| Seed701, seats swapped | 492 | 479 | 491 | 610fda6441c5ccf7d98e3e71fdb8e8c67f9c7dfe658ef0b988ebc6a64eef2f6a |

The pregame restore and V5/V6 public-state/own-hand integration cases also
pass. The next library build fails with the same missing profile-type import,
so the private bottom-menu library case and later default/native/FDN/CUDA
steps did not execute. The overall Windows job remains a failure. CI7cbfa1cd
and the London backport repair that test scope.

Linux job `111116304494` at `cd7f451b` passes the complete Witness/London,
default, native, Limited and host-safe CUDA steps. All 21 selected Limited
integration targets pass 347 cases with zero failures or ignores. Default
library tests pass 2302 cases with 76 existing ignores, and the unchanged
snapshot check measures 5.434 microseconds. The pending private bottom-menu,
combat-damage, priority, discard, kicker, legend, returning-Aura and trigger
ordering restoration cases pass in the selected library filters.

All three complete Linux game receipts equal the Windows receipts, including
the pinned original deck hashes, terminal records and transcript hashes.
The repeated seed123 receipts are identical; swapped seed701 ends naturally.
This proves composed-source game completion and replay on both platforms.
The private bottom-menu library case also passes on Windows at `7cbfa1cd`.
Remaining Windows feature checks and the published arena change still require
their hosted results.

Draw Windows job111114931404 atb296504e passed its default release and native
boundary steps, including the unchanged snapshot case at17.433microseconds.
All11selected Limited integration targets passed185cases with no failures or
ignores. The following library compilation failed with E0433 because three
Windows publication tests used NativeRunCatalogProfileV1 without importing
it. CUDA was skipped and the job remains a failure. The same omission exists
in the other fixture prefixes. The two-line, feature-gated test import is
committed throughout the owned stack; pinned rustfmt and diff checks pass.
Draw's Linux job111114931489 passed its full runtime matrix, with the snapshot
case at5.456microseconds and the corrected live-catalog expectation passing in
default and Limited builds. Current Windows qualification remains pending.

Homunculus Linux job `111118452568` at `6b5697a7` and Koma Linux job `111117937900`
at `51267c7b` pass the complete default, native, Limited and host-safe CUDA
steps. Their selected Limited integration targets number 12 and 13 respectively;
their snapshot measurements are 5.369 and 6.844 microseconds. Full terminal logs
contain no failed test summaries or compilation errors. Each still requires
Windows qualification with the prepared test-module import repair.

Life-gain Windows job `111116101288` at `5a6fa1ca` passes default/native and all 10
Limited integration targets, 170 cases with zero failures or ignores. The
following library compilation fails on two byte-identical duplicate counter
test names and the missing Windows profile import. Published `2b0963ef` retains one
copy of each publication/resume regression and adds the import; its assertions
and filter coverage remain intact. This repair does not change the final
gameplay source, which already has unique test names. Hosted repair execution
is still pending.
CI37088170810 at2397921e failed default Clippy on the omitted V6 London field
and unmatched human-prompt action variants. The repair shares the public
mulligan projection across V5/V6 and preserves the human prompt's existing
custom-game refusal. Its new regression covers public announce/bottom/complete
facts and exact own-hand identities for both observers, plus legacy omission.
That new integration case passes in Windows job111116304498. The private
bottom-menu library case and complete current runtime qualification remain
pending. Configured all-feature lint already passes on cd7f451b.

Counter, lifelink substrate, Kiora and Prowler Linux jobs reported one stale
expectation in `default_fixture_decodes_clean_and_classifies_as_the_live_profile`:
the decoder correctly returned each rebased FDN profile, but the assertion
still expected Targeted Spells. Every owned prefix now pins its concrete
rebased profile in that existing regression; the default Pauper expectation,
production classifier and live-identity checks are unchanged. The repair is
committed through the full stack. Hosted verification remains pending.

Lifegain-mechanics Windows job111116909367 and Voyage Windows job111114906816
failed the already isolated snapshot test at68.294 and66.643microseconds,
respectively, against40. Their native, FDN and CUDA steps were skipped. The
timing-priority change preserves the snapshot code,80objects,200warmups,
2000iterations and40microsecond assertion. A local child-process smoke check
confirms actual normal0x20 and timing0x80 priority. Scheduling contention
remains a hypothesis; the current CI result must establish whether it helps.

Rebuke Windows job `111116024377` at `7e972928` also fails the isolated snapshot
test, at 61.49 microseconds. Its later native/Limited/CUDA steps are skipped.
Its Linux counterpart passes the full matrix. Published Rebuke `5a794540` adds
the same timing-priority procedure as CI `7cbfa1cd`, the Windows profile import
and Rust-relevant classification of native CI helpers. The snapshot workload
and 40-microsecond assertion are unchanged; the hosted performance effect
remains unproven.

CI37092263828 at94d6fc44 completed successfully. All four Python shards built
the real default Rust JSONL environment and passed; its Rust matrix was
skipped because the report-only diff was not Rust-relevant. Those Python
results qualify the unchanged deck/session client and regression source at
that commit. The current four-shard results at5bf586eb above also qualify the
subsequently synchronized reservation test and current Python procedure.
The changed native runner still requires the current CI140 Rust matrix.

| Python shard at94d6fc44 | Selected tests | Skips | Failures | Seconds |
| --- | --- | --- | --- | --- |
| Ubuntu0, job111115367477 | 374 | 14 | 0 | 372.840 |
| Ubuntu1, job111115367474 | 373 | 13 | 0 | 579.632 |
| Windows0, job111115367429 | 374 | 0 | 0 | 613.027 |
| Windows1, job111115367518 | 373 | 1 | 0 | 1032.520 |

Each platform selects747cases: Ubuntu executes720 with27skips and Windows
executes746 with1skip. Ubuntu includes the unchanged original-deck copy-order
resolution and schema-four London client admission cases. This is Python
regression evidence; original Limited games, exact replay and pending restore
require the runtime job logs.

Fresh catalog-only generation verifies both final identities:

| Catalog | Definitions | Version | CardDB identity |
| --- | --- | --- | --- |
| Default main | 192 | v34 | 064a7c989255ab3c |
| Complete FDN fixtures | 236 | v51 | bd1385731e43c4a1 |

The composed prefixes retain these distinct identities and preserve original
and earlier composed records for reading. Publishing/resuming mutations must
match the actual live build.

| Prefix | Definitions | Version | CardDB identity |
| --- | --- | --- | --- |
| Counter / lifelink substrate | 218 | v40 | b3dc8eb6d0a6407d |
| Life-gain cards | 220 | v41 | 958bf2fd746ec314 |
| Draw cards | 223 | v42 | a1376b708b689b01 |
| Homunculus | 225 | v43 | f9e239337d933849 |
| Koma | 227 | v44 | d196a0b706b69e48 |
| Kiora | 229 | v45 | 2d5aebb949eca8a6 |
| Prowler | 230 | v46 | 6630c9c09989878f |
| Rebuke | 231 | v47 | f48d52aff22f0f04 |
| Voyage | 232 | v48 | 3cd3ce13b2c52a14 |
| Scavenging | 234 | v49 | 592f678756e75cf5 |
| Armor | 235 | v50 | f5076bb105d12b32 |
| Witness / London | 236 | v51 | bd1385731e43c4a1 |

Local checks: 22 focused Python deck/session tests, formatting, workflow lint,
Python CI-helper compilation and diff checks pass. Both original decks resolve
in copy order with 40/40 admitted cards. Inventory confirms 39 distinct fixture
names, 80 copies and zero missing or partial fixture cards. Seven dependency
tokens remain in the FDN registry. Fresh reference inventory is 43 full,
1 partial and 242 missing out of 286; this milestone does not claim full-set
support. The original deck byte hashes remain:

- UG: bb618d6eaddf04b0a9e51e9a88cd512a04635e99ebca91b11ace4c305d634c86
- WG: be026f1c86e3aabcb294517188d0c5f4f0cdfa3c5e95ee9dedfc51d3cdf814f7

Integration corrections preserve main's wide counters, equipment abilities,
delve/adventure paths and object hashing while adding the fixture mechanics.
Prowler and monarch share one end-step trigger-ordering window. The typed
reference walk retains counter, returning-Aura, Surveil and removed-ability
bindings. The Windows bootstrap keeps its normalized pinned toolchain directory
without the compiler override rejected by the existing build guard. The timing
case remains exact, isolated and explicitly includes the existing ignored test;
its 40-microsecond limit is unchanged.

| Goal requirement | Current evidence | Remaining check |
| --- | --- | --- |
| Original decks and dependencies | Both unchanged decks resolve 40/40; all 39 fixture names admitted; final catalogs generated | Hosted card/rules cases with the prepared arena change |
| Natural external games | Linux/Windows cd7f451b and Windows7cbfa1cd receipts prove seed123 twice and swapped seed701 end naturally with unchanged original inputs | Qualify the prepared arena source |
| Deterministic replay | All three complete Linux/Windows receipts match across those runs; repeated seed123 receipts are identical; transcript hashes recorded above | Repeat the original games on the prepared arena source |
| Pending save/restore | Windows7cbfa1cd private bottom menu, pregame and V5/V6 cases pass; Linux private bottom menu, combat and effect cases pass | Qualify all restoration cases with the prepared arena storage |
| Rules and regressions | Linux cd7f451b passes all21Limited targets/347cases plus library/default/native/CUDA checks; Windows7cbfa1cd passes24Witness and12London cases before its timing failure; all four Python shards pass at both5bf586eb and7cbfa1cd | Complete the matrix with the unchanged timing gate and new arena isolation/serialization checks |
| XMage comparisons | PR15 d98525a0: 146 tests/16 classes, zero failures/errors/skips | Reuse for unchanged matching card rules; investigate any actual gameplay discrepancy |
| Delivery | Own branches and existing PR stack are maintained | Publish final source/report and verify required checks |

## Historical isolated-stack evidence

The following records identify their tested source and outcomes. They preserve
prior qualification and failed checks; their status text describes those older
runs. The current delivery frontier above governs completion of the goal.

Scope: the two unchanged DraftZero decks for issue #110, all their card/token
dependencies, and their custom-game rules. This is an implementation milestone.
The 286-name reference, drafting, fair Limited search and training integration
have separate milestones in `fdn_limited_implementation_v1.md`.

October 2, 22:57 UTC completion audit: CI37049183922 at d43d59a2 passed
formatting/all-feature lint, all four Python shards and the complete Ubuntu
Rust job 110978361641. Its full log confirms all 21 selected Limited integration
targets, 345 passing cases including 11 London and24 Witness cases, the pending
bottom-menu library restore, and the original fixed-seed games. Seed123's two
complete receipts are identical; swapped seed701 also terminates naturally.
Default library regressions passed 1706 cases with 43 existing ignored tests;
the Limited library, native platform and host-safe CUDA commands completed.
Windows gameplay is also qualified by PR140's Bash step at the compatible
36d54594 source: job 111036127744 completed the London integration and pending
bottom-menu library commands successfully at 22:12:10 UTC. The engine, cards,
original decks and London tests are identical to d43d59a2; the intervening diff
contains only CI helpers, the workflow and reports. Bash propagates either
command's failure. This establishes the original games, exact replay and
pregame restore on both operating systems. The full Windows Rust job is live.

Mage PR15 at d98525a0a7c has green current-head reference and labeler checks.
Kernel PR136 has all eight current-head checks green. The earlier published
report follow-ups retain their original source passes, but their new checks
are still running. Kiora PR132's report-only Windows job 110990567893 also
failed the snapshot timing case: 76.158 microseconds against 40, with 1746
other library tests passing. CI backport 83de53b2 is published in PR132 and
retains all fourteen integration targets and all nine library filters; its
helpers are identical to PR140's. Workflow lint and Python compilation passed.
The old full Ubuntu job passed at 22:53:39 UTC and was preserved through
completion before publication. Its original gameplay source f8fce216 retains
a fully green run; the repaired recheck is pending. Draw PR129's report-only
recheck failed the existing snapshot
timing gate. Repair 990d825d runs that unchanged gate outside parallel unit
tests; both hosted default release steps have now passed, including Windows
job 111032886034 at 22:32:33 UTC. Full feature checks and the completed log's
exact isolated timing result remain pending. Draw PR129 is ready for review
using its unchanged gameplay source's full pass and both affected release
steps. Armor PR137 is ready for review:
its unchanged Rust code retains passing Ubuntu/Windows evidence, and the
affected Windows Python shard at f13fd955 passed all 286 tests, including the
repaired sentinel case. Its other three current Python shards also passed.
PR140 at 36d54594 propagates Rust failures with Bash, groups unchanged library
filters and isolates the unchanged timing gate. Both fresh pinned toolchain
installations, all-feature lint and all four Python shards passed. Ubuntu's
full Rust job 111036127712 completed successfully at 22:50:49 UTC. Its log
confirms all 21 integration targets and 345 passing cases, all nine grouped
library filters, and exactly one isolated snapshot case passing at 4.81
microseconds against the unchanged 40-microsecond limit. The three production
filters correctly select no Linux cases because those tests are Windows-only.
Default regressions passed 1705 cases plus the isolated timing case, with 43
existing ignores; host-safe CUDA passed 15 cases with seven existing ignores.
Windows's default stage is live, and its native publication/resume and grouped
filter checks remain pending. Documentation-only PR141 at 565862f8 has green CI;
its Rust job is correctly skipped. Remaining full Rust execution prevents a
milestone completion claim.

| Requirement | Scenario, action and assertion | Named checks and observed status |
| --- | --- | --- |
| Both original 40-card decks load | Resolve the pinned files in original row/copy order; require 40 IDs and no unsupported names; refuse partial, unknown, no-effect and token mainboards. | `test_original_decks_are_40_cards_and_fully_resolve`; `test_cli_resolves_both_unchanged_original_decks_in_copy_order`; `test_unknown_partial_no_effect_and_token_are_all_rejected`. CLI resolution passed on October 2. |
| Natural external games | Send original decks through the real schema-4 subprocess; player zero mulligans once and player one twice; bottom four cards total before play; require a natural terminal under the safety cap. | `original_fixtures_play_with_london_in_both_seats_and_replay_exactly`: seed 123 twice and swapped seed 701 passed on Ubuntu. Compatible Windows job 111036127744 passed the same integration command under Bash at 22:12:10 UTC. |
| Deterministic replay | Run identical input, seed and actions twice; compare the complete transcript and terminal exactly. | Same original-fixture test; SHA-256/input/terminal receipts are printed by the CI command. Passed on Ubuntu and the compatible Windows Bash step. |
| Pending decision restore | Snapshot every pregame phase and a live private bottom menu; preserve choices, binding, RNG, incarnation and the resulting state; reject stale or foreign choices atomically. | `restore_each_pregame_phase_reproduces_choices_rng_and_zone_incarnations` and the pending-bottom library command passed on both operating systems, using the compatible Windows Bash step. `pending_assignment_rejects_priority_and_restores_exactly` covers combat. |
| Required card/rules behavior | Exercise costs, targets, loss of targets, triggers, tokens, layers, attachments and combat; validate all earlier session/protocol identities. | Current-head Ubuntu passed 24 Witness cases,11 London integration cases, the London session restore, all 19 earlier Limited integration targets, library/RL checks and default/native/CUDA commands. Windows's full job, Windows-only publication/resume canaries and the repaired CI follow-ups remain pending. |
| Relevant XMage comparisons | Execute London and FDN card positions plus arbitrary damage allocation, trample, deathtouch and first/double strike. Require every selected XML report with no failures/errors/skips. | Mage run 37049882555 at 5cc4decd8ffe passed all 146 cases in all 16 selected classes, with zero failures/errors/skips. Includes four strict combat allocation cases, 24 existing combat cases, seven London cases and 111 earlier FDN card cases. Source/output hashes and XML counts independently verified. Mage PR #15 carries the reference suite. |
| Reviewable delivery | Commit changes in owned branches, preserve the stacks and get required checks green. | Kernel PR #139 and Mage PR #15 carry the final batches. Source is committed; review and complete CI remain pending. |

Fresh inventory: 39 unique original fixture names fully resolve. The wider
286-name reference has 43 full, one partial and 242 missing names. Required
token definitions are Elf Warrior, Knight, Faerie, Homunculus Horde, Koma's
Coil, Scion of the Deep and Raccoon. The Limited catalog has 206 definitions
and v49 hash `3eee8a1cbc874e18`; default v32 remains `64c82a261e078f1a`.

Pinned original deck SHA-256s:

- UG: `bb618d6eaddf04b0a9e51e9a88cd512a04635e99ebca91b11ace4c305d634c86`
- WG: `be026f1c86e3aabcb294517188d0c5f4f0cdfa3c5e95ee9dedfc51d3cdf814f7`

CI 37038033614 passed all 19 Limited integration targets on Ubuntu, then
seven session tests and 66 RL session tests. Its next library target found a
stale expected catalog count of 205 rather than 206, with 45 other card-definition
tests passing. Corrected that assertion without changing the catalog or its
identity. Corrected-source Ubuntu job 110977578991 at fdbe23e5 completed
successfully, with all 20 selected integration targets and334 passing cases.
Its Windows job remains live, so complete verification is still required.

Both PCs' other owners retain their resource reservations. All heavy Rust
and Java compilation uses hosted CI. These checks are bounded CPU correctness
checks, with no new training, paid compute or playing-strength claim.
