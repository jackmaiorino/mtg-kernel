# Celestial Armor implementation and verification

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
| Release boundaries and hosted CI | Pending. Debug production build correctly refuses with `native_store_build_profile_not_release` at build_support/native_store_build_capture_v1.rs:799. Existing CI explicitly runs the three required combined-feature release checks. No build-profile bypass or new heavy Haley compile. |

Core commit `4f792085`. Rust/Cargo1.94.1, MSVC14.50.35725.0,
two Cargo jobs, no incremental/debug symbols. Guarded checks project2GiB,
cap4GiB and preserve60GiB free. Seed123, no GPU or training.

Original fixture SHA-256s remain:
- WG `be026f1c86e3aabcb294517188d0c5f4f0cdfa3c5e95ee9dedfc51d3cdf814f7`
- UG `bb618d6eaddf04b0a9e51e9a88cd512a04635e99ebca91b11ace4c305d634c86`

External receipt and archived Python/PowerShell checker:
`E:/mtg-fdn-fixtures/fdn-armor-external-001`, hash-verified independent
mirror `C:/Users/Jack/fdn-armor-external-001-sealed`. Two serial CPU
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
