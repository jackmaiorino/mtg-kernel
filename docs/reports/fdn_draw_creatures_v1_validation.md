# FDN draw creature validation

## Integration with current main

Integrated the current-main life-gain prefix 0ce8eb6d and retained six lossless
fixture conversions needed by main's optional CUDA compile gate. The default
registry stays 192 definitions/v34; the draw prefix has 223 definitions/Limited v42,
with generated identity a1376b708b689b01. A catalog-only build-script probe
compiled and emitted the identity; it is not engine/gameplay qualification.

Original FDN profiles and prior composed counter/life-gain profiles remain
readable; publication/resume still require the exact live tuple. Preserve
the grouped filters, all eleven integration targets, fresh pinned Windows
compiler paths, no-fail-fast and the unchanged isolated timing workload.
Formatting, workflow lint, diff and focused Python checks passed. Complete
hosted Rust/gameplay/native/production checks for the composed source remain
pending. Earlier isolated-source evidence below remains historical.

Hosted CI at `2861046736faf13fd0cca1c1f76e9771f9ecd17f` passed all eight checks: Rust on
Ubuntu and Windows, all four Python shards, formatting/lint and path detection.
Run: https://github.com/jackmaiorino/mtg-kernel/actions/runs/36973189527. Earlier pending entries below record the original
verification sequence. The subsequent status/report update changes no tested
code, workflow, fixture or catalog bytes.

The later Windows recheck at01c7334b failed
`snapshot::tests::snapshot_clone_cost_is_bounded`:67.822microseconds per call
against40microseconds, with1743 other library tests passing and49 existing
ignores. Job110986481520's failed log remains the evidence for that attempt.
The CI repair preserves the test source,80objects,200warmups,2000iterations
and40microsecond limit. Its runner executes all other library tests normally,
then the exact timing test in a fresh process with one thread and the same
compiled executable. Rust steps use Bash to propagate each native failure.
The existing Ubuntu job remains live; publish after it finishes. Corrected
end-to-end CI remains required.

The full failed job log and exact snapshot source are sealed at
`E:/mtg-fdn-fixtures/fdn-draw-ci-timing-failure-001`, with a verified independent
mirror at `C:/Users/Jack/fdn-draw-ci-timing-failure-001-sealed`.
Each copy is276505bytes, under its16MiB cap with60GiB reserves checked.
Log SHA-256 is `f4e36ca083540062079d92ac1cd74ccfd634994d9230296e66d83b4d6a2baad6`.
The original failed result is retained; the isolated timing result is pending.

Strix Lookout, Mischievous Mystic and Faerie Token are implemented at
`99171529`, with focused test corrections at `95b4a94a` and `84675f3b`.
Matching committed HaleysPC source is
`f32299b405e3ee80bcc2df87ee4bb7ef0e89e616`.
See the [rules contract](../design/fdn_draw_creatures_v1.md).

| Check | Result |
| --- | --- |
| New rules | All 15 passed: exact costs/stats/keywords, tap and sickness restrictions, draw/discard timing and restore, vigilance followed by second-main looting, second-draw ordinals, both players' turns, multiple Mystics, source departure and token-entry interactions. |
| Gameplay regressions | All 170 existing checks passed across fixture A/B, targeted spells, combat cards, counter and life-gain creatures, legend rule, combat, priority and custom sessions. |
| Pending decisions | All seven custom-session restore checks passed. A real 40-card Lookout deck reaches a discard choice; restoring preserves the complete response, legal actions, environment hash and next transition. |
| Catalog | All 46 definition and 134 record checks passed; three existing record tests remain ignored. Live v40 is `fbef8128c0ddad96`. Historical v39 remains `3f6b7e8df71f3195`; older profiles stay readable. |
| Compatibility | The default v32 CardDB golden passed. Frozen flat v1/v2 source hashes and the pinned deck hashes remain unchanged. |
| Build/lint | Limited-feature all-target Clippy, default workspace Clippy and combined Limited/production release Clippy passed with warnings denied. The external release binary built successfully and its cold copy was hash-verified. |
| Python | All 34 focused importer, custom client, flat-V2 golden and Pauper-manifest checks passed using the existing Torch environment. |
| XMage | All five matching scenarios passed at Mage `3bf7ea717e701b1059e5540ed943f5702d9ef63a`, based on `a5c90fe180021e70e2a644ade00eeab07f857a40`; zero failures/errors/skips and reactor BUILD SUCCESS. [Parity PR #5](https://github.com/jackmaiorino/mage/pull/5) contains tests and report. |
| Production boundaries | All 17 Windows release checks passed: two v32 refusals, fourteen earlier-FDN publisher/resume refusals and a live v40 construct/seal/decode/validate round trip. The source guard used the clean committed remote checkout. |
| External replay | Both games reached the same natural P1 win with identical full result dictionaries and JSONL transcript hashes. Seed 123, episode 7; 20 Islands, ten Lookouts and ten Mystics; 413 policy steps and 397 physical decisions. Across both players: six Lookouts and eight Mystics cast, ten Lookout activations, ten discard choices and twelve decision menus naming Faerie tokens. |
| Hosted CI | The companion PR's checks track hosted validation. This report records completed local/reference checks, without claiming hosted success before those checks finish. |

The XMage cases cover Tidings drawing four cards with one Mystic,
Divination with two Mystics, drawing twice on the opponent's turn,
vigilance followed by looting in the second main phase, and a Lookout
activation that draws the second card and then discards. Strict choice
mode explicitly orders simultaneous Mystic triggers.

Original fixture inspection now reports UG 34/40 and WG 34/40 fully
supported copies. Nine distinct names remain unsupported: Cackling
Prowler, Celestial Armor, Homunculus Horde, Kiora, the Rising Tide, Koma,
World-Eater, Luminous Rebuke, Sylvan Scavenging, Uncharted Voyage and
Witness Protection. Mulligans, original-deck natural terminal gameplay,
remaining rules comparisons and passing CI still belong to the full
goal. This batch does not establish full FDN coverage or playing strength.

Small manifest: CPU correctness checks on HaleysPC with two Cargo build
jobs and incremental compilation disabled; GPU ordinal none. Rust/Cargo
1.94.1, MSVC linker 14.50.35725.0; seed 123 for the new rules/session and
external check. Other regression seeds are in the checked-in
tests. Python 3.11 locally and 3.12.10 on HaleysPC. XMage uses Java
23.0.2, Maven 3.9.9, one reactor worker, two active processors and bounded
heaps. Jack's PC reservations remain respected. No formal measurement,
training or paid compute was launched.

| Input/output | SHA-256 |
| --- | --- |
| Base registry | `af9d725658d238a6a1b0c99b52c12ff6823bdcaf8147a77556ee2e3b0b67d6bc` |
| FDN extension | `dc58145ae96bbb4e05de96cbc910a03903432b7a8bb4d9288540fe3b0f9893cf` |
| New integration tests | `23aaedd65b821f1641a84a72694ed8766c254d0ab10bf4e800dd23b5c0b8aaec` |
| Original UG deck | `bb618d6eaddf04b0a9e51e9a88cd512a04635e99ebca91b11ace4c305d634c86` |
| Original WG deck | `be026f1c86e3aabcb294517188d0c5f4f0cdfa3c5e95ee9dedfc51d3cdf814f7` |
| Frozen flat v1 source | `b4de06cab5644196111ab0f59bec454dd51cb1ec785b19386dc11709650d738b` |
| Frozen flat v2 source | `080e417c010c51e04b8dac82574503e2c4a1d38669d15f1019ee82f7a7105e0a` |
| XMage test source | `447980bc1d5249cfc225245bf57a36cca1f23050988bc981329428a097f15cef` |
| XMage result XML | `f1f69d8accad5d91c51b3ac6d29438ce539fdd94740c6d1320036febe62ecaee` |
| Executed external check | `1d760370acb05513a94b0cd5b88f0175daa1d0a19d52a7576f19d92e7d7d88c4` |
| External input deck | `68b89f426edcecfcb049463d52efd880f43258f508d31ffd0091237eef9fd54a` |
| External binary | `1795dd8d6bcc2826df3c78460f87f7bd3fd5295ef26752a2697c3e97d4136618` |
| External transcript | `643df0672877140a7d5f530e77e04291983b9432669f47512583fc813b809a30` |
| External result JSON | `acbfb25baf37cf23857e3174eae9b50ecf0032c3b25e3a6fcffecc6f3db8340d` |

Logs stay outside Git under `C:/Users/haley/`:
`fdn-draw-creatures-checks-005-{1,2,3}.log/.log.exit`,
`fdn-draw-creatures-checks-006-{4,5,6,7}.log/.log.exit`,
and `fdn-mage-draw-003.log/.exit` with its Surefire XML.
The completed production checks and release build are
`fdn-draw-production-001-{1,2,3,4,5}.log/.log.exit`; the small launch
manifest is `fdn-draw-production-001.manifest.json`. The external check,
result and logs are `fdn-draw-external-001.py/.json/.log/.exit`.
The 2,515,968-byte binary is preserved at
`E:/pinned-binaries/1795dd8d6bcc2826df3c78460f87f7bd3fd5295ef26752a2697c3e97d4136618/kernel_limited_env.exe`.
Earlier failed logs remain preserved. Corrections cover staged cost
commit timing, triggers queued before a discard finishes, forced empty
block declarations, the old native test-fixture identity and XMage test
setup. The final checks above use the corrected source.

Before the batch, uncited incremental cache files in the owned Cargo
target were removed after confirming no compiler used them. The outside-Git
manifest `C:/Users/haley/fdn-owned-incremental-prune-001.json` records
17,593,594,204 regenerable bytes and the cache inventory hash
`60d13497124ca26d6369a4e4bd4179e5242a7c150de66cd1626b0f22f55b7cd4`.
Compiled binaries, cited evidence, source and failed logs were retained.

The first composed card-pair/draw CI found three returning-Aura variants
missing from main's suspended-reference walker. Added exact-incarnation
scanning of both Aura/host bindings, the complete attachment-candidate list,
and the answered guard's remaining frames. A regression covers both bound
objects and unrelated library objects. Formatting/diff checks passed; hosted
Rust and complete feature qualification remain pending.

## Windows compiler environment correction

The completed Windows Kiora job 111079733482 failed 33 default-library cases
with `ForbiddenBuildFlagOverride`: the CI bootstrap exported `RUSTC`, which
the existing compile-time guard correctly rejects. The bootstrap now keeps
the normalized Windows `RUSTUP_HOME` and pinned Rust install without exporting
`RUSTC`. Hosted default and native-build capture checks are pending. The guard
and toolchain pin remain unchanged.
