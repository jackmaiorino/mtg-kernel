# FDN Koma implementation and verification

Koma, World-Eater and Koma's Coil are implemented in the opt-in FDN
catalog. All thirteen focused gameplay checks pass; broader verification remains pending. This batch follows
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
identity `8f1dc68e65c24306`; mutation refusal checks are added and pending.
Default v32 generated counter programs and canonical bytes are preserved
in source; the default golden checks remain pending for this batch.

| Check | Current evidence |
| --- | --- |
| Core compilation | `fdn-koma-preflight-001` exited zero; Limited Cargo check passed in 29.59 seconds on HaleysPC. |
| XMage comparisons | All nine strict-choice cases passed in `fdn-mage-koma-004`; see [Mage PR #7](https://github.com/jackmaiorino/mage/pull/7). |
| Kernel gameplay | All thirteen cases passed in `fdn-koma-gameplay-003`: casting, counters, ward, combat, noncombat exclusion and pending-choice restore. Earlier compile/setup failures remain preserved. |
| Counter helper | A feature-gated unit check covers protected physical spells and copies, counterable triggered abilities, and ordinary departure; execution pending. |
| Catalog, sessions and prior gameplay | Checks prepared; execution pending. |
| Python | The preceding source preparation ran 34 focused Python checks successfully; current hosted CI remains required. |
| Release and external replay | Pending. No Koma natural-terminal or deterministic external replay claim. |
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

Jack's PC remains reserved by Claude #811 through about 08:30 EDT.
HaleysPC free space dropped below its shared 60 GiB reserve after the
initial test build. Further local checks there are held until storage
qualifies. Compression passes `fdn-owned-cache-compress-005` through
`008` preserve every selected file's before/after SHA-256.
They touched only the owned Cargo cache and deleted no files. The
compression receipts, manifests and logs remain under `C:/Users/haley/`.
No unrelated process, cache, checkout or frozen run was modified.

The bounded gameplay-only rerun `fdn-koma-gameplay-003` exited zero after a 66-second build. The preceding rerun was refused by its storage guard before compilation. Remaining library/regression checks are prepared in `fdn-koma-checks-002` and not yet dispatched. Kernel ward
positions use Snap (`{1}{U}`) with no battlefield lands to untap; XMage
positions use Unsummon (`{U}`). Each pays its exact casting cost before
offering the same four-mana ward cost. The reference comparison concerns
ward behavior; it does not assert identical whole-game traces across
these different spells.

The full fixture milestone remains open: seven other distinct card names,
mulligans, original-deck natural terminals and complete regression/CI
evidence are still required. This report makes no playing-strength claim.
