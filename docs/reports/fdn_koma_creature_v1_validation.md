# FDN Koma implementation and verification

## Integration with current main

Integrated the current-main Homunculus prefix e554b397 and retained the
explicit-mainboard helper in both feature selections. Default stays at
192 definitions/v34; Koma and its Coil yield 227 definitions/Limited v44,
with generated identity d196a0b706b69e48. The catalog-only generator compiled
and emitted this identity; it does not qualify engine gameplay. Original
and prior composed FDN profiles remain readable. Mutation still requires
the exact live tuple.

CI retains all thirteen integration targets, its existing Limited and
production filters, fresh pinned Windows compiler paths and the unchanged
isolated snapshot workload. Formatting, workflow lint, diff and catalog
checks passed. Composed-source hosted Rust/gameplay/native/production checks
remain pending. Earlier isolated-source qualification below is historical.

Hosted CI at `c19bd3544f6bcb2f306ce70577881bcd64725b3d` passed all eight checks: Rust on
Ubuntu and Windows, all four Python shards, formatting/lint and path detection.
Run: https://github.com/jackmaiorino/mtg-kernel/actions/runs/36991952282. Earlier pending entries below record the original
verification sequence. The subsequent status/report update changes no tested
code, workflow, fixture or catalog bytes.

Koma, World-Eater and Koma's Coil are implemented in the opt-in FDN
catalog. All thirteen focused gameplay checks, production boundaries and the bounded external replay pass; hosted CI remains pending. This batch follows
[Horde PR #130](https://github.com/jackmaiorino/mtg-kernel/pull/130) and keeps
the original UG and WG fixture decks.

The creature is legendary, blue and green, 8/12 for `{3}{G}{G}{U}{U}`,
with trample, ward four and spell protection against counters. Its combat
damage to a player queues four 3/3 blue Serpent tokens named Koma's Coil.
Counterspell may legally target the creature spell, and payable
counter-unless-pay choices remain available. The protection applies at
the counter attempt and does not protect triggered abilities or prevent
ordinary stack departure.

FDN v42 has hash `f4fba61544ae7963`, verified in generated output after
the core Cargo check. Prior v41 records retain their recognized read-only
identity `8f1dc68e65c24306`; read-only recognition passed; all 21 production boundary checks passed, including prior-v41 mutation refusals.
Default v32 generated counter programs and canonical bytes are preserved
in source; the exact default v32 golden and frozen default session checks passed.

| Check | Current evidence |
| --- | --- |
| Core compilation | `fdn-koma-preflight-001` exited zero; Limited Cargo check passed in 29.59 seconds on HaleysPC. |
| XMage comparisons | All nine strict-choice cases passed in `fdn-mage-koma-004`; see [Mage PR #7](https://github.com/jackmaiorino/mage/pull/7). |
| Kernel gameplay | All thirteen cases passed in `fdn-koma-gameplay-003`: casting, counters, ward, combat, noncombat exclusion and pending-choice restore. Earlier compile/setup failures remain preserved. |
| Counter helper | The focused unit check passed: physical spells and copies resist counters; triggered abilities remain counterable; ordinary departure is unchanged. |
| Catalog, sessions and prior gameplay | 46 definition checks, 136 run-record checks (three existing ignores), 66 FDN session checks, seven custom-session checks and 203 prior gameplay/counterspell checks passed. |
| Default compatibility and lint | The exact default v32 golden and all 68 default session checks passed. Limited all-targets Clippy and default workspace all-targets Clippy passed with warnings denied. |
| Python | The preceding source preparation ran 34 focused Python checks successfully; current hosted CI remains required. |
| Production and external replay | All 21 release production boundary checks and combined-feature release Clippy passed. A symbol-free debug custom-deck binary built and was preserved by hash on both PCs. Two seed-123 external games ended naturally with identical results and transcript hashes. |
| Hosted CI | Draft [PR #131](https://github.com/jackmaiorino/mtg-kernel/pull/131) is running CI at `c19bd354`; no green claim yet. |

Core source commit: `af94973c`; local integration/catalog source commit:
`d644f86a`, followed by test correction `b076f733`. The owned remote
verification branch is `codex/fdn-koma-verify-v1` in
`C:/Users/haley/mtg-kernel-fdn-lifegain-codex`; its corresponding correction
commit is `13034eb`. Rust/Cargo 1.94.1, MSVC linker 14.50.35725.0,
`CARGO_BUILD_JOBS=2`, `CARGO_INCREMENTAL=0`, seed 123, GPU ordinal none.

The updated XMage source commit is `e32e548e4d3`. Nine tests, zero failures,
errors or skips, reactor `BUILD SUCCESS` in 33.028 seconds. Source SHA-256
`797af8c555d7f6473f695b121ee9d51c7605a1dbc5cfd65c28f1e171bb0c6a8a`;
Surefire XML SHA-256
`b8e605476dac54de297b1ea219621d98174bc488564a84aa0408c6e508f3a2b4`.
The blocked position uses a supported Tolarian Terror with four +1/+1
counters, matching the kernel's 9/9 blocker test. Earlier reference
results and failed test-build logs remain outside Git.

Claude #813 released Jack's PC and E: writes at 07:18 EDT on October 2.
Storage interruptions and compression receipts remain preserved outside Git.
Compression touched only owned Cargo and Mage caches, preserved content
hashes and deleted no files. No frozen measurement was interrupted.

The bounded gameplay-only rerun `fdn-koma-gameplay-003` exited zero after a 66-second build. The preceding rerun was refused by its storage guard before compilation. Remaining library/regression checks passed in `fdn-koma-checks-002` phases two through eight. A storage interruption stopped its default build; `fdn-koma-checks-003` resumed phases nine through eleven and exited zero. These checks disabled debug symbols to reduce cache growth; the initial thirteen gameplay checks used ordinary debug settings. Kernel ward
positions use Snap (`{1}{U}`) with no battlefield lands to untap; XMage
positions use Unsummon (`{U}`). Each pays its exact casting cost before
offering the same four-mana ward cost. The reference comparison concerns
ward behavior; it does not assert identical whole-game traces across
these different spells.

The full fixture milestone remains open: seven other distinct card names,
mulligans, original-deck natural terminals and complete regression/CI
evidence are still required. This report makes no playing-strength claim.

The initial production check refused a debug build (`native_store_build_profile_not_release`), preserving the production feature boundary. `fdn-koma-production-002` then exited zero: 21 release boundary checks, combined-feature release Clippy, and the symbol-free debug interface build. The release test build took 11 minutes 39 seconds. The external check `fdn-koma-external-001` subsequently exited zero after the host release.

The debug interface binary is 5,765,632 bytes with SHA-256
`bf8b792fa37a4941f6bc7693870966fe06842fce8457cd354237c46b74abaddf`,
built from remote source `844f9740328027cd3dae8ee3800af4d94cc2e8a7`.
Verified copies are under `C:/Users/haley/fdn-pinned-binaries/` and
`C:/Users/Jack/fdn-pinned-binaries/`, each in that hash's directory.
The same binary is now verified under `E:/pinned-binaries/` as well.

The bounded external check used 14 Forest, 14 Island, eight Koma and four
Strix Lookout, seed 123, episode 7. Both games finished naturally with P0
winning, 582 policy steps and 578 physical decisions. Each cast two Koma
and six Lookout, made 30 Lookout activations and discards, and exposed Coil
tokens in four decision menus. Transcript SHA-256:
`f2e2cf9417f582cdb846ede24a9bfa0462e9dacdecc6fe544fdfb0b44761519e`.
Result SHA-256 `66f911dbec2256dfbf08c8ac34e723cc5ac82b4e9b7082212083ba117a2fd607`;
log SHA-256 `d49dee2c0ea0f2884ad3b37b3209a8ff37f90aa2e57098e57b669d9e2e086fd3`.
These are custom-deck engineering checks, not original-fixture completion
or playing-strength estimates. The release build source and catalog are
unchanged. Hosted CI at the existing PR head continues separately.
