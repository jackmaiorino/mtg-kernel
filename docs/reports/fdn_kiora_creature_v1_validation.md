# FDN Kiora implementation and verification

Kiora, the Rising Tide and Scion of the Deep are implemented in the opt-in
FDN catalog. Kiora's entry draws two, then discards two. Its attack trigger
checks seven graveyard cards both when created and when resolved; the
controller may refuse the token. Scion is a legendary blue 8/8 Octopus.

The thirteen gameplay cases cover exact payment, rejected casts, short-hand
and empty-library behavior, second-draw interactions with Mystic and Horde,
threshold boundaries, refusal, acceptance, source departure and the legend
rule. Discard, optional token and legend decisions replay after save/restore.
A separate predicate check excludes tokens and virtual spell copies before
state-based actions remove them. Runtime testing caught and fixed a missing
attack-event emission path for the new threshold trigger.

| Check | Evidence |
| --- | --- |
| Kiora gameplay | `fdn-kiora-checks-005` phase 1: 13 passed. |
| Threshold predicate | Phase 2: 1 passed. |
| Card definitions | Phase 3: 46 passed; live v43 hash `5a8469de3061a1fb`, 199 definitions. |
| Native records | Phase 4: 137 passed, 3 existing ignores; all earlier profiles remain readable. |
| Limited sessions | Phases 5 and 6: 66 public-session and 7 private custom-session checks passed. |
| Prior gameplay | `fdn-kiora-checks-006` phase 7: 216 passed across fourteen integration suites. |
| Lint | Phases 8 and 11: Limited all-target and default workspace all-target Clippy passed with warnings denied. |
| Default compatibility | Phases 9 and 10: 68 session checks and the frozen default v32 hash check passed. |
| Python deck/session tools | `fdn-kiora-python-002`: 14 deck and 6 client cases passed on Python 3.12.10. |
| XMage comparisons | All 9 strict-choice cases passed in `fdn-mage-kiora-003`; [Mage PR #8](https://github.com/jackmaiorino/mage/pull/8) records the source and result hashes. |
| Production mutation boundaries | `fdn-kiora-production-001` exited zero: 23 release checks, including v42 Koma publication/resume refusal and current-profile round trip; combined-feature release Clippy passed. |
| External natural-terminal replay | Pending. No Kiora external-game claim. |
| Hosted CI | Draft [PR #132](https://github.com/jackmaiorino/mtg-kernel/pull/132) is running at `f8fce2160`; no green claim yet. |

The verified kernel functional source is local `7ca57acb`, remote
`0837dfb13135aad0f7ca8f327372387738511471` in
`C:/Users/haley/mtg-kernel-fdn-kiora-codex`. Rust/Cargo 1.94.1,
MSVC linker 14.50.35725.0, two Cargo jobs, no incremental compilation,
no debug symbols, seed 123 and no GPU. Manifests bind source/input hashes,
output log hashes and the 60 GiB free-space reserve. The launcher monitors
only its own Cargo child tree and stops it if free space falls below reserve.
A handle-capture interruption and earlier failed tests are retained outside Git.
The command typo selecting a nonexistent regression target was corrected;
no assertion was weakened to make the regression sequence pass.

The XMage reference uses source pin
`a5c90fe180021e70e2a644ade00eeab07f857a40` and test branch
`codex/fdn-kiora-parity-v1`. The corrected test source is `2146ab93da2`. All nine reference cases pass,
with zero failures/errors/skips, in 23.521 seconds. Surefire XML SHA-256
`337c9d8e608e5f3fc010ec61dd6ceb746fd3f1ba470ef3f445a2d4d63ccd3e88`.
A storage-guard interruption of the first corrected rerun is preserved.
Cold owned Cargo and Mage caches are compressed with before/after content
hash checks; no evidence file is deleted. Claude #813 released Jack's PC and E: writes at 07:18 EDT on October 2.
The old reservation was respected through that release.

Both original deck SHA-256s remain unchanged. Registry coverage is UG 37/40
and WG 34/40 copies, with six distinct required card names still missing.
London mulligans, both complete original-deck games and their deterministic
external replays remain required by the overall goal. These engineering
results do not establish playing strength or full-set FDN support.

The production check's first release build took 11 minutes 37 seconds.
The custom-deck binary built successfully with debug symbols disabled:
5,771,776 bytes, SHA-256
`b3a3ecd5df88200b1eecf6de4025981d36b7c66491a6bf56c645ae34450fa478`.
A hash-verified cold copy on HaleysPC protects it from subsequent builds.
The small production manifest SHA-256 is
`baab354f914389f9e6aa219ce400988b764a3be1fc54076be960965e5d1f027e`.
External replay remains pending until all required cold copies are verified.
