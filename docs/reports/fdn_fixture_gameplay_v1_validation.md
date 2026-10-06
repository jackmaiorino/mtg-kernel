# FDN fixture gameplay milestone checks

## Current delivery frontier, October 5

PR #140 is the canonical integration PR for the two original FDN fixture
decks. It incorporates current main `85da0e95`, all current kernel prefix heads
from #124 through #139, the committed Witness repair `53b9eb4d`, and report
history from #141 through `5ccb37f1`. Every listed local and published head is
an ancestor of the integration composition; no unique source, fix or receipt
was discarded. Main's tracing, saved-input diagnostic and guarded launcher
changes remain intact. This integration needs its own complete CI and current
diff review before exact-head merge. It is not delivered on main yet.

Qualified fixture source `928816583af511220cb36960034591bfab2e3aa0` passed all
eight hosted checks in run `37115415284`: complete Linux and Windows Rust
matrices, four Python shards, formatting/lint and change detection. Both Rust
logs cover all 21 selected Limited integration targets, affected session and
catalog filters, native production boundaries and host-safe CUDA checks with
no failed summaries. All five arena unit cases, strengthened GameState restore
and the private London bottom-menu snapshot restore pass. The frozen snapshot
check retains 80 objects, 200 warmups, 2,000 iterations and its 40-microsecond
limit; Linux measured 223 nanoseconds and Windows 583 nanoseconds. Mutation of
shared arena storage still clones its objects. These are snapshot measurements,
not complete-turn, search or playing-strength claims.

All three complete game receipts from current Linux and Windows are identical
to each other and both London `f904c95c` and earlier `7cbfa1cd` platform lists.
The repeated seed123 receipts are identical in full. All games use the original
40-card inputs and the external custom-deck subprocess with London mulligans;
none ends at the safety cap. Pending decision restoration covers mulligan
phases, private bottom choices, combat allocation and effect/trigger choices.

| Original game | Seat assignment | Physical decisions | Policy steps | Outcome | Transcript SHA-256 |
| --- | --- | --- | --- | --- | --- |
| Seed 123, twice | UG / WG | 538 | 544 | Natural P0 win | 7738afcf0a9afff429a1e2e699e631897d2d8a9faba13f698a12a1b04d95c6e1 |
| Seed 701 | WG / UG | 479 | 491 | Natural P0 win | 610fda6441c5ccf7d98e3e71fdb8e8c67f9c7dfe658ef0b988ebc6a64eef2f6a |

Both unchanged decks resolve 40/40 copies: 39 distinct fixture names, 80 total
copies, zero missing or partial fixture cards. Seven required tokens are Elf
Warrior, Knight, Faerie, Homunculus Horde, Koma's Coil, Scion of the Deep and
Raccoon. Default catalog is 192 definitions/v34 `064a7c989255ab3c`; complete
fixture Limited catalog is 236/v51 `bd1385731e43c4a1`. Historical profiles
remain readable; publication/resume mutation requires the actual live build.
The pinned original file SHA-256s are:

- UG: `bb618d6eaddf04b0a9e51e9a88cd512a04635e99ebca91b11ace4c305d634c86`
- WG: `be026f1c86e3aabcb294517188d0c5f4f0cdfa3c5e95ee9dedfc51d3cdf814f7`

Current qualified Python shards at `92881658` built the actual release JSONL
environment. Ubuntu0 selected 374 tests with 14 skips; Ubuntu1 selected 373
with 13 skips. Windows0 selected 374 with zero skips; Windows1 selected 373
with one skip. Every shard has zero failures. Original input/copy-order tests
and the repaired Windows descendant-lifetime regression pass. Exact jobs:
`111188356190`, `111188356188`, `111188356176`, `111188356138`.

The matching XMage reference at Mage PR #15, source
`d98525a0a7c8cd1dc526e608c36d5cdbabe39acd`, passed all 146 tests in 16 classes
with zero failures, errors or skips in run `37050614051`, job `110982798031`.
This includes four strict damage-allocation cases, 24 combat cases, seven
London cases and 111 card cases. No Java production behavior changed. These
reference results remain compatible with the unchanged fixture rules; the
reference PR and its parent stack remain separate reviewable deliverables.

| Goal requirement | Authoritative evidence | Integration status |
| --- | --- | --- |
| Both original 40-card decks fully supported | Pinned file hashes, row/copy-order resolution, all 39 names and seven tokens; focused rules tests | Proved at 92881658; composed-main checks pending |
| Natural external games | Original-fixture subprocess test, two seed123 runs and swapped seed701, full natural terminal receipts | Passed on Linux and Windows at 92881658 |
| Deterministic replay | Complete repeated receipts and cross-platform/baseline equality, transcript hashes above | Passed at 92881658 |
| Pending save/restore | London pregame and private-bottom binding/transition tests, combat and effect regressions, arena isolation and exact GameState round trip | Passed in both full matrices at 92881658 |
| Focused rules, XMage and regressions | 21 kernel integration targets plus library/native/CUDA matrices; 146 matching XMage cases; all four Python shards | Matching source passes; new composition requires full CI |
| Commit, review and integration | All local/public sibling heads retained in canonical #140; current main retained | Complete diff review, new CI and exact-head main merge pending |

## Prefix reconciliation

All current prefix heads are retained as merge ancestors. Their later Windows
profile import, reservation regression and timing-child procedure are already
present in the qualified canonical source. Rebuke and Witness arena backports
are byte-identical to its three source files. Prowler, Rebuke, Armor and Witness
report additions and #141's seven report/design updates are preserved.

The #128 conflict concerned removal of a duplicate counter publication/resume
test in an older catalog prefix. The complete source already has exactly one
copy of each counter regression; its later life-gain/draw/Homunculus/Koma/Kiora/
Prowler/Rebuke/Voyage/Scavenging/Armor cases are unique. Retain that complete
source and the duplicate-removal invariant, rather than dropping later tests.
No production source changed in this conflict resolution.

- PR #124: `f673b10acfc80a535bbdb77690a348984275c988`
- PR #125: `1371000ed86069809dab2755f29ad02d19732af3`
- PR #128: `2ac0b22accecf70b2c69cb3fa7cc47661c4d5883`
- PR #129: `9c4a3826afaa2d917e55dda31c7e998da7768f98`
- PR #130: `5f9fa36914866c6f0c9bccb1848b254e7e955301`
- PR #131: `c5fcce105590f4ba18dcb416cb32e44bf07f80a2`
- PR #132: `35f27dae036074973914db98b0d5eafc8dcc4735`
- PR #133: `057740e8aee23326782941b34257edc3f71ce0fc`
- PR #134: `add3d48561f6845c5cf708974e2cfacd300edefc`
- PR #135: `330ce16f891350fa0babb891c7953b19c61d0c92`
- PR #136: `9e7583a83128de57156742600d81292b4948102d`
- PR #137: `a72f4d1085e805a94caf24e939ff2da3f0ffddcd`
- PR #138: `53b9eb4d9351a5e37c2be3a6299f276100c219a3`; published predecessor `7b02fc92613ed1f3aa13fc63a82f9b090fa4829d` also retained
- PR #139: `f904c95c55f299c5fca0744a1e2afa4b29331c9c`
- PR #141: `5ccb37f13e9cb1cdad2c1563a775c9342f53caf2`; published predecessor `49ed45c066bc301568597ca2399fdbb22365392e` also retained

Broader Foundations coverage remains a separate issue #110 milestone: the
pinned 286-name observed-gameplay reference has 43 full, one partial and 242
missing names. It is not a complete booster manifest. Drafting, deck building,
full-set support, fair search and training are outside this fixture milestone.
No new training, paid compute, formal measurement or strength claim was made.

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
