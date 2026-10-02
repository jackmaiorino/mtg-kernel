# FDN fixture gameplay milestone checks

Scope: the two unchanged DraftZero decks for issue #110, all their card/token
dependencies, and their custom-game rules. This is an implementation milestone.
The 286-name reference, drafting, fair Limited search and training integration
have separate milestones in `fdn_limited_implementation_v1.md`.

October 2, 22:39 UTC completion audit: CI37049183922 at d43d59a2 passed
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
are still running. Draw PR129's report-only recheck failed the existing snapshot
timing gate. Repair 990d825d runs that unchanged gate outside parallel unit
tests; both hosted default release steps have now passed, including Windows
job 111032886034 at 22:32:33 UTC. Full feature checks and the completed log's
exact isolated timing result remain pending. Armor PR137 is ready for review:
its unchanged Rust code retains passing Ubuntu/Windows evidence, and the
affected Windows Python shard at f13fd955 passed all 286 tests, including the
repaired sentinel case. Its other three current Python shards also passed.
PR140 at 36d54594 propagates Rust failures with Bash, groups unchanged library
filters and isolates the unchanged timing gate. Both fresh pinned toolchain
installations, all-feature lint and all four Python shards passed. Ubuntu's
default/native stages passed; its Limited stage and Windows's default stage
are live. The grouped filters and exact isolated timing results still need
completed-log verification. Documentation-only PR141 at 565862f8 has green CI;
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
