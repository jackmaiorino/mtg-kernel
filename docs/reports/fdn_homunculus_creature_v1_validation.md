# FDN Homunculus Horde validation

## Integration with current main

Integrated the current-main draw prefix 8ec65480, preserving the expanded
192-definition default/v34 catalog. Homunculus and its token produce
225 definitions/Limited v43 with generated identity f9e239337d933849.
The catalog-only generator compiled and emitted this literal; it does not
qualify engine gameplay. Original and prior composed FDN profiles remain
readable, while native publication/resume still require the exact live tuple.

Retain main's explicit-mainboard regression cases and keep only its fixed
default transcript goldens gated off in Limited builds. The grouped filters
preserve all twelve integration targets, the four original Limited filters
(including the full RL session group), three production filters and the
unchanged isolated timing case. Returning-Aura/counter reference handling
and lossless terminal-tactic fixture conversions remain included. Formatting,
workflow lint, diff checks and 20 focused Python deck/session cases passed.
Composed-source Rust/gameplay/native/production hosted checks remain pending.
Earlier isolated-source qualification below remains historical.

Hosted CI at `e35f4754aff84724b69db2a73a98dab81f6d6ffe` passed all eight checks: Rust on
Ubuntu and Windows, all four Python shards, formatting/lint and path detection.
Run: https://github.com/jackmaiorino/mtg-kernel/actions/runs/36985482516. Earlier pending entries below record the original
verification sequence. The subsequent status/report update changes no tested
code, workflow, fixture or catalog bytes.

Homunculus Horde and its token copy are implemented at `d3e09847`, with
test corrections at `90a8f1cd` and `9b75805b` and v41 compatibility at
`288fce15`. The card batch was checked on matching HaleysPC commit
`e0f5ee681eafe98b6d70c3b9f54aaa669568b922`. The later custom-session
trigger-order extension is implemented through local `6d6f26dd`, matching
HaleysPC `f3efeef927eb28468a445a21b4418ffa49f76c87`.
See the [rules contract](../design/fdn_homunculus_creature_v1.md).

| Check | Result |
| --- | --- |
| New rules | All ten passed: printed and token characteristics, exact casting cost, second-draw timing, batch draws, multiple sources, copies triggering on later turns, counters/damage/tapped state, departed sources, token departure and pending-trigger restore. |
| Gameplay regressions | All 185 existing checks passed across fixture cards, combat, priority, legends, counters, life gain, draw creatures and custom sessions. |
| Pending decisions | All six custom-session restore checks passed. The separate Horde/Mystic integration check restores simultaneously pending second-draw triggers and produces identical final state. |
| Large trigger ordering | Four new tests passed: nine triggers with one final engine commit, separate trigger instances from one source, stale-answer refusal and mid-prefix restore, and whole-group step budgeting. All 66 FDN session tests passed. |
| Catalog | All 46 definition and 135 record checks passed; three existing record tests remain ignored. Live v41 is `8f1dc68e65c24306`. Historical v40 remains `fbef8128c0ddad96`; older records remain readable. |
| Compatibility | All 68 default session tests passed, including independent flat commitments, V5 transcript bytes and environment/reset hashes. The exact default v32 CardDB golden also passed on the final source. Frozen flat v1/v2 source hashes and both original deck hashes remain unchanged. |
| Build/lint | The card batch passed Limited/default all-target Clippy and combined Limited/production release Clippy with warnings denied. The final session extension passed Limited all-target Clippy and default workspace all-target Clippy with warnings denied; the new release binary built successfully in 3m44s. |
| Python | All 34 focused importer, custom client, full-feature-V2 golden and Pauper-manifest checks passed using the existing Torch environment after the session extension. |
| XMage | All five matching scenarios passed at Mage `4a5b56e0121`, based on `a5c90fe180021e70e2a644ade00eeab07f857a40`; zero failures/errors/skips and reactor BUILD SUCCESS. [Parity PR #6](https://github.com/jackmaiorino/mage/pull/6) contains tests and report. |
| Production boundaries | All 19 Windows release checks passed: two v32 refusals, sixteen older-FDN publisher/resume refusals and a live v41 construct/seal/decode/validate round trip. |
| External replay | Both runs of the same Horde/Lookout/Mystic deck reached a natural P0 win, with identical full transcript hashes. Each used 621 policy steps, 598 physical decisions and nine incremental choices for a nine-trigger group. |
| Hosted CI | Pending. The batch remains draft until the hosted regression and feature checks pass. The parent lifegain and draw PRs each have seven jobs green, with Windows Rust tests still running. |

The first external check used the supported 40-card deck `20 Island`,
`8 Homunculus Horde`, `8 Strix Lookout`, `4 Mischievous Mystic`, seed 123
and episode 7. It halted at policy step 568 and physical decision 561
because the old adapter enumerated every permutation and refused nine
pending triggers. The deck and seed are retained. The custom Foundations
session now offers one remaining trigger at a time, includes its selected
prefix in bindings/snapshots and commits the complete order once. Groups
of seven or fewer and catalog V5/V6 sessions retain their existing menus.
The new check cast two Hordes, seven Lookouts and five Mystics, activated
Lookout 29 times, discarded 19 cards and observed copied Hordes in 29
decision menus. Both complete replay results, including the transcript
SHA-256, were equal. The binary is hash-verified in an immutable HaleysPC
pin and the local cold pin at
`E:/pinned-binaries/10cd104cdd856d51fcf057cd7ce93c28cab552258440687c1424879003d1dac6/kernel_limited_env.exe`.

The first broad feature-enabled session check passed 66 tests and failed
seven default-catalog goldens. Those independently fixed values include
the default CardDB hash, while the optional FDN build uses v41. The seven
checks and their private helpers now compile only in the default build;
their expected values are unchanged and all passed there. The feature
build then passed its complete 66-test session suite. All-target lint
also exposed one missing semantic-category arm in the benchmark example;
the new feature-gated arm and default-only helper gates fix compilation.
Earlier failure logs remain preserved.

The XMage cases cover a four-card draw with one original, two originals
drawing twice, an original and its copy triggering on the opponent's next
turn, a counter on the original being excluded from copied values, and
opponent draws not triggering the controller's Hordes. Both original and
copy have the printed name, blue color, Homunculus subtype and mana value
four. Strict choice mode explicitly orders simultaneous triggers.

Original fixture coverage is UG 35/40 and WG 34/40 fully supported copies.
Eight distinct names remain unsupported: Cackling Prowler, Celestial Armor,
Kiora, the Rising Tide, Koma, World-Eater, Luminous Rebuke, Sylvan
Scavenging, Uncharted Voyage and Witness Protection. Mulligans,
original-deck natural terminals, remaining rules comparisons and passing
CI still belong to the full goal. This batch does not establish full FDN
coverage or playing strength.

Small manifest: CPU correctness checks on HaleysPC with two Cargo build
jobs and incremental compilation disabled; GPU ordinal none. Rust/Cargo
1.94.1, MSVC linker 14.50.35725.0; seed 123 for the new rules and external
check. Other regression seeds are in the checked-in tests. XMage uses Java
23.0.2, Maven 3.9.9, one reactor worker, two active processors and bounded
heaps. Jack's PC reservations remain respected. No formal measurement,
training or paid compute was launched.

| Input/output | SHA-256 |
| --- | --- |
| Base registry | `af9d725658d238a6a1b0c99b52c12ff6823bdcaf8147a77556ee2e3b0b67d6bc` |
| FDN extension | `d0624fbcbe934b4987bbc25c8196581a6c838a47d3d4840e2d78abaa5e9213d8` |
| New integration tests | `555df2b6c78fb791183967dda08d188d8ed429d5a48550ffbbc07ddc552f40cf` |
| Original UG deck | `bb618d6eaddf04b0a9e51e9a88cd512a04635e99ebca91b11ace4c305d634c86` |
| Original WG deck | `be026f1c86e3aabcb294517188d0c5f4f0cdfa3c5e95ee9dedfc51d3cdf814f7` |
| Frozen flat v1 source | `b4de06cab5644196111ab0f59bec454dd51cb1ec785b19386dc11709650d738b` |
| Frozen flat v2 source | `080e417c010c51e04b8dac82574503e2c4a1d38669d15f1019ee82f7a7105e0a` |
| XMage test source | `bb40a4fe4134732d8ad16bf36d454fe566245c7fb21bdc42d9e2f2a23e7af096` |
| XMage result XML | `5e4a722cb1566369111d07bfbce7d6ce1e2ad48be817479829f0f2f6a40e35c6` |
| Session action source | `597d0e0622cb5cbd6effce1dc194507679016dba8a7dec43fc53a39e495dda96` |
| Session adapter source | `f99744c16467a8dbd07a128b959225ba53995f5b7e52427c5991dd500ba3d42a` |
| Final release binary | `10cd104cdd856d51fcf057cd7ce93c28cab552258440687c1424879003d1dac6` |
| External input deck | `c54178ee9cd6b03a72ecd4b15d705a11a4f78fc8ebeac7e4fbbfaad0efeb765b` |
| External checker | `1c9d6fcb337b65574828e3feb070958bdc1e9341056a64897daf709c5fa289d2` |
| Repeated external transcript | `d2646ecbc9ceba0bd81883c9b3072bed0494ee2171bfad9c05e4dd74eb184c01` |
| External result | `6c66f5bd9154a4f3249b3b3b7673f71bc9bd413129d89fb17634e5f8fabe50a2` |
| Final lint/build manifest | `f4fac86adadc320a4a05a49619cd31296fc9d26b7222c82b0247592623af519c` |
| Owned symbol compression manifest | `a043bb0acf759190bfc6c926944cb8b0648f345a745ebe710f1083053b336458` |
| Second symbol compression manifest | `52a18a89647f905374ab5e41795d97dbbd9c752bd1be69451020770856bd4377` |
| Remaining symbol compression manifest | `8799c3d7989adf0bc8dc6301a6f1d640778cc2a12f5ea4d32a9e289c09633262` |

Logs remain outside Git under `C:/Users/haley/`:
`fdn-homunculus-checks-003-{1,2,3,4,5,6,7}.log/.log.exit` and
`fdn-mage-homunculus-004.log/.exit` with its Surefire XML. Earlier failed
test-setup records remain preserved. The production tests are
`fdn-homunculus-production-001-{1,2,3}.log/.log.exit`; release lint is
`fdn-homunculus-production-003-{4,5}.log/.log.exit`. Session-extension
checks are in `fdn-homunculus-order-checks-002-{1,2,3}.log`,
`fdn-homunculus-order-checks-003-{3,4}.log` and
`fdn-homunculus-order-checks-004-{4,5,6,7,8}.log`, with exit files and small
`.manifest.json` files. The final default golden is
`fdn-homunculus-default-golden-001.log/.exit/.manifest.json`. The phase-7
filter in order-checks-004 matched zero tests and is not counted as a
passing golden; the exact named test then executed and passed separately.
The failed external log is
`fdn-homunculus-external-001.log/.exit`. The successful two-game replay
is `fdn-homunculus-external-002.py/.ps1/.log/.exit/.json`.

Before the release check, the five largest PDB files in the owned debug
cache were compressed through NTFS. All five before/after SHA-256s matched;
no files were deleted. The outside-Git manifest
`fdn-owned-symbol-compress-001.json` records paths, hashes and physical
compression results. Free C: space increased from 64,477,990,912 to
65,233,133,568 bytes, preserving the 60 GiB reserve and the runner's
512 MiB initial build allowance. After the production test build, the
retry's storage guard refused dispatch. Two further compression passes
preserved all hashes in the other 37 owned debug PDB files and brought free
space to 66,102,472,704 bytes. Their manifests are
`fdn-owned-symbol-compress-002.json` and
`fdn-owned-symbol-compress-003.json`. The resumed lint/build runner uses a
1 GiB build allowance above the 60 GiB reserve.

The first combined-feature release Clippy attempt reused a stale cached
build script and reported an unknown Homunculus subtype. The local and
remote build-script source hashes both remained
`659f035a7ea26bc23f805a04999cef3f68bd5e24f12bdb124f4432b8f82eac2e`.
Refreshing only the owned source file's modification timestamp forced a
fresh build script; release Clippy then passed. No source bytes changed,
and the failed attempt's log remains preserved.

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
