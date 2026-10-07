# Celestial Armor implementation and verification

> October 5 integration status: canonical kernel PR #140 preserves this
> report and its source history. Fixture source `92881658` has passed both full
> Rust matrices and all four Python shards. Its current-main composition needs
> fresh CI and exact-head review/merge. Earlier pending/failure statements below
> refer to their named historical sources. See
> [current fixture validation](fdn_fixture_gameplay_v1_validation.md).

## Current Windows timing repair

Windows job `111167180011` at published source `43eab9fe` failed only the
isolated snapshot check: 65.258 microseconds against the unchanged 40 limit.
Every other workspace test summary passed. Native, Limited and host-safe CUDA
steps were skipped, so this job does not qualify the complete Armor matrix.
The full terminal log is retained as `fdn-rust-current-111167180011.log`.

The runner now gives only the short Windows timing child `HIGH_PRIORITY_CLASS`,
using the same procedure as CI `92881658`. It retains normal priority for
correctness tests and the frozen 80 objects, 200 warmups, 2000 iterations and
40-microsecond assertion. Native CI runner changes now select Rust checks.
Python compilation, workflow lint and diff checks qualify the backport's
syntax and selection. Its actual hosted timing effect remains unqualified.
The same published source's Linux job `111167180017` completed its entire
default/native/Limited/host-safe CUDA matrix successfully. All 19 selected
integration targets and affected library filters passed, with no failed test
summaries. The frozen snapshot measured 6.978 microseconds. All four Python
shards and formatting/lint checks also passed at that source. The prior run
is now terminal; its Linux and failed Windows logs are retained before
publication. Complete hosted qualification of the timing repair remains
required.

Hosted CI37031403320 at33d84667 passed Ubuntu Rust and both Ubuntu Python
shards. Windows Python shard0 found a timing-dependent sentinel-coverage test:
fake process startup legitimately ranked serial first, while the test required
the two-device allocation. Apply the already qualified narrow test repair from
319ac081: fix only the ranking input and still execute/compare every prefix and
full-length subprocess output. The repaired focused test passed locally in
11.531 seconds. Windows Rust completed successfully. The complete job110918929067
log confirms all19 selected Limited integration targets executed, with310
passing tests including20 Armor cases and no failed summaries or native
compile-error markers. Both hosts' Rust checks and required release boundaries
pass at33d84667. Publish the test-only repair after this completed audit;
complete corrected-source CI remains required.

The unchanged WG fixture now resolves all40 copies. Armor costs `{2}{W}`,
has flash, and its entry trigger attaches to a controlled creature and
independently grants hexproof and indestructible until end of turn.
Equipment bonuses are +2/+0 and flying; equip `{3}{W}` uses sorcery timing.
Removing or re-equipping Armor moves the static bonus while the original
creature keeps its temporary protection. Returned source/target incarnations
cannot satisfy older contracts.

FDN v48 `8664e1fd25362caa`,205 definitions, Armor ID204. Default v32
and every previous catalog literal remain intact. Old profiles remain
readable and cannot publish/resume against a different live build.

| Check | Observed result |
| --- | --- |
| Armor rules and restore | Checks005 phase1:20 passed, including response hexproof, indestructible destruction/damage, exact costs/timing, source/target incarnations, cleanup and pending targets. |
| Equipment and Scavenging regression | Checks005 phase2:9 Equipment and21 Scavenging cases pass. |
| Definition/catalog history | Checks003:46 definitions and142 records pass,3 existing ignores. |
| Python importer/client |20 pass; unchanged WG resolves40, UG38. |
| XMage |10 strict reference cases pass; Mage PR13. |
| Broader/default compatibility | Checks006 final0:242 prior rules,125 Limited/124 default engine and62 Limited/27 default session checks pass. Limited/default all-target Clippy and frozen v32 golden pass. |
| Original WG external gameplay | Two original WG-versus-WG games reach natural P0 wins at346 policy steps, including Armor casting, with identical transcript hashes. |
| Release boundaries and hosted CI | Both hosted Rust jobs pass at33d84667, including all three combined-feature release filters. The Windows full-log audit verifies all19 Limited integration targets,310 passes and no failures. Hosted CI's only failure is the timing-dependent Python sentinel-coverage expectation; its focused repair passes locally and awaits corrected-source CI. The earlier debug production refusal remains retained. |

Core commit `4f792085`. Rust/Cargo1.94.1, MSVC14.50.35725.0,
two Cargo jobs, no incremental/debug symbols. Guarded checks project2GiB,
cap4GiB and preserve60GiB free. Seed123, no GPU or training.

Original fixture SHA-256s remain:
- WG `be026f1c86e3aabcb294517188d0c5f4f0cdfa3c5e95ee9dedfc51d3cdf814f7`
- UG `bb618d6eaddf04b0a9e51e9a88cd512a04635e99ebca91b11ace4c305d634c86`

External receipt and archived Python/PowerShell checker:
`E:/mtg-fdn-fixtures/fdn-armor-external-001`, hash-verified independent
mirror `C:/Users/user/fdn-armor-external-001-sealed`. Two serial CPU
correctness games, seed123, episode7, maximum16384 steps each,180-second
limit. Guardian checks the exact owned Python process,16MiB output cap,
1MiB projection and60GiB reserve. Observed sealed files15,842 bytes including the sealing receipt.

Binary SHA-256 `c0a0897aeba77f8c59e442928eced949e9fd70b0a83f9893f425f3073faa2d40`,
5,850,112 bytes, preserved by hash on E and on both PCs. Transcript SHA-256
`8ecd0193c0e5688da6fd733a7272fd27a9319f346da42b3d76c10e89deb66efe`.
`fdn_celestial_armor_v1_prune.json` records removal of only the duplicate
scratch files after both copies were hash-verified. All evaluated binaries,
source records and failed logs remain retained.

Retained checks001/002 expose a wrong affordability fixture: Elvish Mystic
provided the missing generic mana. Checks004 exposed a wrong damage
expectation: response hexproof invalidated the opponent's only target, so
no damage occurred. Corrected cases pass. Checks005 retains the debug
production-profile refusal. These failures were preparation, not wins.

The reference inventory has42 full,1 partial and243 missing names.
UG still needs Witness Protection. London mulligans and remaining CI checks
are required before the overall fixture gameplay goal can complete.
No full-set, drafting, search integration or playing-strength claim.

## Current-main integration

The Armor prefix incorporates Scavenging 4d25d550 and main's
192-definition/v34 catalog. Its generated Limited identity is 235
definitions/v50 f5076bb105d12b32; the appended Armor ID is 234. Original and
earlier composed tuples remain readable; mutations require the live identity.
The equipment generator retains main's granted-activated-ability field,
Viridian Longbow program and existing equipment definitions. Armor has no
granted activated ability. All original gameplay and restore targets remain
selected. Catalog generation, formatting, workflow lint and diff checks pass
locally; composed-source Rust qualification is pending.
