# FDN life-gain creature validation

Local source `7feec2b8b35445ddc77c1e5e8ddf05fd391cc4ef` implements
Exemplar of Light and Sun-Blessed Healer. Matching committed HaleysPC
source is `7fc174bde3c8670ae1afdd919cbac1e68e994e0c`.
See the [rules contract](../design/fdn_lifegain_creatures_v1.md).

| Check | Result |
| --- | --- |
| New rules | All 21 passed: separate life-gain events, source-bound counters, one draw trigger per turn, control changes, new incarnations, ordinary/kicked Healer, eligible graveyard permanents, Aura attachment choice, protection/hexproof, no legal host and restore. |
| Gameplay regressions | All 149 existing checks passed across fixture A/B, targeted spells, combat cards, counter creatures, legend rule, combat, priority and custom sessions. |
| Pending decisions | All six custom-session restore checks passed. A real 40-card custom deck reaches the returned-Aura attachment choice; restored response, legal actions, environment hash and next transition match. |
| Catalog | All 46 definition and 133 record checks passed; three existing record tests remain ignored. Live v39 is `3f6b7e8df71f3195`. Earlier catalog literals remain readable. Historical v34 remains `d2b479e9d5990f07`. |
| Compatibility | All 28 default state checks, exact environment-hash golden and frozen v32 CardDB golden passed. The native flat refusal check passed, including an actual Exemplar trigger-use ledger, with destination buffers unchanged. Frozen flat source bytes are unchanged. |
| Build/lint | Feature all-target Clippy, default workspace Clippy and combined Limited/production release Clippy passed with warnings denied. The external release binary built; all five production-runner phases exited zero. |
| Python | All 34 focused importer, custom client, flat-V2 golden and Pauper-manifest checks passed using the existing Torch environment. |
| Events | All 14 feature event checks passed. Positive gains emit events after prevention; simultaneous lifelink damage is grouped by source and controller. |
| XMage | All six matching scenarios passed at Mage `b273481ec8a`, based on `a5c90fe180021e70e2a644ade00eeab07f857a40`. Surefire reports zero failures/errors/skips and reactor BUILD SUCCESS. [Parity PR #4](https://github.com/jackmaiorino/mage/pull/4) contains the tests and report. |
| Production boundaries | All 15 Windows release checks passed: two v32 refusals, twelve earlier-FDN publisher/resume refusals and live v39 construct/seal/decode/validate round trip. The runner uses clean committed remote source and the strict source guard. |
| External replay | Two seed-123/episode-7 games ended naturally at 379 policy steps and 370 physical decisions, with identical full results and transcripts. Each cast six Exemplars and seven Healers, and paid kicker three times. |
| Hosted CI | [PR #128](https://github.com/jackmaiorino/mtg-kernel/pull/128), head `9d1f5eb1`, run `36966637290`: queued when this report was updated. Keep draft until hosted validation passes. |

The six XMage cases cover two separate gains from Dazzling Angel, one
multi-counter placement from Felling Blow, draws on both players' turns,
ordinary Healer, kicked artifact return with entry draw, and kicked
Bind Monster return onto an opposing hexproof creature. The last case
also has an opposing protected creature; the Aura chooses the hexproof
host, attaches before its entry ability, taps that host and deals its
entry damage to the Healer's controller.

The external transport/replay deck contains 20 Plains, ten Exemplars and
ten Healers. Its natural outcome is `p1_win`. The tested binary has a
hash-verified cold copy at
`E:/pinned-binaries/adf19400bfc000f04238c0881170bba70b247b18359da04425f93419bd092b13/kernel_limited_env.exe`.
This constructed check does not establish completion of either original
fixture deck.

Original fixture coverage is UG 29/40 and WG 34/40; their pinned bytes are
unchanged. Eleven distinct fixture names remain unsupported: Cackling
Prowler, Celestial Armor, Homunculus Horde, Kiora, the Rising Tide, Koma,
World-Eater, Luminous Rebuke, Mischievous Mystic, Strix Lookout, Sylvan
Scavenging, Uncharted Voyage and Witness Protection. The full goal also
requires mulligans, original-deck natural terminal games, deterministic
replay/save/restore, remaining rules comparisons and passing CI. This
batch does not establish full-set coverage or playing strength.

Small manifest: CPU correctness checks on HaleysPC with two Cargo build
jobs, GPU ordinal none; Rust/Cargo 1.94.1, MSVC linker 14.50.35725.0. Rules
setup and external replay use seed 123; other regression seeds are in the
checked-in tests. Jack's PC reservations remain respected. The XMage
check uses Maven 3.9.9, Oracle Java 23.0.2, one reactor worker, two active
processors and bounded heaps. No formal measurement, training or paid
compute was launched.

| Input/output | SHA-256 |
| --- | --- |
| Base registry | `af9d725658d238a6a1b0c99b52c12ff6823bdcaf8147a77556ee2e3b0b67d6bc` |
| FDN extension | `a639764ae9d99d1d8f515088d04597e7b16c77165de44bceaac16cd09c319e57` |
| New integration tests | `2387fa79afa11d3e9cc4e73e929d9f133db2443f3710667b837a8fb82fba6136` |
| Original UG deck | `bb618d6eaddf04b0a9e51e9a88cd512a04635e99ebca91b11ace4c305d634c86` |
| Original WG deck | `be026f1c86e3aabcb294517188d0c5f4f0cdfa3c5e95ee9dedfc51d3cdf814f7` |
| Frozen flat v1 source | `b4de06cab5644196111ab0f59bec454dd51cb1ec785b19386dc11709650d738b` |
| Frozen flat v2 source | `080e417c010c51e04b8dac82574503e2c4a1d38669d15f1019ee82f7a7105e0a` |
| XMage test source | `89a6a8f6c52645f783493d9f780c3fdda31bb2ce3f0912fabc558777835bb739` |
| XMage result XML | `73d474c78750e1c83dd1d27d2516f51b122e081890cbdeed0574c1ab1374a8fa` |
| External input deck | `ff627790ec926253368d242e268cbe122ef4b6042efdb630c8416474580c9609` |
| Tested binary | `adf19400bfc000f04238c0881170bba70b247b18359da04425f93419bd092b13` |
| External transcript | `702442950b19a22fd7de577947c3380f8bb9a7c2d8952afff07f20edf04f04d2` |

Logs stay outside Git under `C:/Users/haley/`:
`fdn-lifegain-pair-checks-001-{1,2,3}.log/.log.exit`,
`fdn-lifegain-pair-checks-002-{4,5,6}.log/.log.exit`,
`fdn-lifegain-pair-checks-003-7.log/.log.exit`,
`fdn-lifegain-pair-default-001-{1,2,3,4}.log/.log.exit`,
and `fdn-mage-lifegain-003.log/.exit`.
The completed production checks are
`fdn-lifegain-pair-production-001-{1,2,3,4,5}.log/.log.exit`;
the runner's terminal file is `fdn-lifegain-pair-production-001.exit` (zero).
External replay evidence is `fdn-lifegain-external-001.py/.json/.log/.exit`
(zero).

Earlier failures remain preserved. Focused iterations corrected test
setup and the returned-Aura continuation guard. Broad checks found the
old definition count and three needless returns; the corrected checks
above passed. The new v39 pin was corrected against generated definitions
without changing the historical v34 literal. XMage iterations corrected
the host choice and the attachment assertion's host-controller argument;
run 003 then passed all six cases. The first Python run used a Python
environment without Torch; the existing Torch environment passed all
34 checks.
