# FDN Kiora implementation and verification

## Integration with current main

Integrated the current-main Koma prefix 9536ef56. Default remains 192 definitions
at v34; Kiora and Scion produce 229 definitions/Limited v45 with generated identity
2d5aebb949eca8a6. The catalog-only generator compiled and emitted this identity;
it is not engine/gameplay qualification. Original and prior composed FDN
profiles remain readable, and publication/resume require the exact live tuple.

All fourteen integration targets and nine library filters remain selected,
including actual live-profile production round trip. Retain the fresh pinned
Windows compiler-path correction and main's explicitly ignored snapshot case,
which runs once through the exact unchanged-workload timing filter. Formatting,
workflow lint and diff checks pass. Composed-source hosted gameplay/native/
production qualification is pending; earlier isolated-source evidence below
remains historical.

Hosted CI at `f8fce216064e6185858383a2bfeac4994432bf9d` passed all eight checks: Rust on
Ubuntu and Windows, all four Python shards, formatting/lint and path detection.
Run: https://github.com/jackmaiorino/mtg-kernel/actions/runs/37000169016. Earlier pending entries below record the original
verification sequence. The subsequent status/report update changes no tested
code, workflow, fixture or catalog bytes.

The report-only recheck at edbc6dc0 failed the unchanged snapshot timing
case in Windows job 110990567893: 76.158 microseconds per clone against the
40-microsecond release budget, with 1746 other library tests passing. Its
Ubuntu job was still live when preparing the repair. The original gameplay
source above remains fully qualified; no engine, fixture or catalog changed.

Reuse PR140's CI helpers: run the original timing case alone with its original
80-object workload and 2000 iterations, propagate every Rust command failure
with Bash, and compile each library feature once before executing all nine
unchanged filters in separate processes. All fourteen integration targets are
preserved. Install the same pinned Rust toolchain in a fresh job directory.
The helpers and bootstrap are identical to PR140; its Ubuntu Limited stage
passed at 22:41:36 UTC and both bootstrap steps passed. Kiora's repaired hosted
execution is pending. No timing threshold or assertion is weakened.

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
| External natural-terminal replay | `fdn-kiora-external-001` exited zero; two seed-123 games ended naturally with identical complete results and transcript hashes. |
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
Hash-verified cold copies on HaleysPC, Jack's PC and E: protect it from subsequent builds.
The small production manifest SHA-256 is
`baab354f914389f9e6aa219ce400988b764a3be1fc54076be960965e5d1f027e`.
The external deck used 12 Forest, 12 Island, eight Kiora, four Strix Lookout
and four Mischievous Mystic. Both games ended naturally with P1 winning,
779 policy steps and 771 physical decisions. Each cast seven Kiora, made
four optional token choices and four legend choices, and exposed Scions
in six decision menus. Transcript SHA-256:
`7a4933f94b293bea96d1e27e6e6a466454fa490279f84303a763a76406a84551`.
Result SHA-256 `04945be7c2bd3397493bb2923fb7f36df28f59387a6450af998bfbdb0ff531a8`;
log SHA-256 `e307559aba8a6febcd5f900560357cd71c0d64ab7e938cc86c1f4815bc188582`.
These custom games exercise the card and existing interactions. They do not
complete the original-fixture milestone or estimate playing strength.

Repaired Windows job111063203013 passed the default suite and isolated timing
case, then failed native compilation because the fresh toolchain supplied
mixed path separators. Applied the same Windows RUSTUP_HOME/RUSTC normalization
as PR140. Preserve the native path validator and pinned toolchain. Workflow
lint and the local pinned compiler/path check passed; the corrected hosted
native and complete Windows checks remain pending. Gameplay source is unchanged.

## Windows compiler environment correction

The completed Windows Kiora job 111079733482 failed 33 default-library cases
with `ForbiddenBuildFlagOverride`: the CI bootstrap exported `RUSTC`, which
the existing compile-time guard correctly rejects. The bootstrap now keeps
the normalized Windows `RUSTUP_HOME` and pinned Rust install without exporting
`RUSTC`. Hosted default and native-build capture checks are pending. The guard
and toolchain pin remain unchanged.

The completed all-feature lint jobs 111090913376 and 111090959717 found
missing Limited fields in main's explicit-deck constructor and missing
incremental-trigger action arms in the human bridge. Both are corrected; the
bridge preserves its explicit unsupported-prompt behavior. Fixture ID tests
now use the generated catalog positions after the 30-definition main expansion.
Formatting, workflow lint and diff checks pass; Rust checks are pending.
