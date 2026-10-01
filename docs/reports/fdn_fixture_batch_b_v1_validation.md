# FDN fixture batch B validation

Source commit `353969ae209c71f9d0bb86e4346ae843d438e08f` implements eight
fixture cards and two exact tokens. HaleysPC's corresponding committed source
is `b8e081d514f4e01d0913f92d0e4451139cebcd91`. New IDs are 168 through 177;
earlier IDs and the original Pauper registry remain unchanged.

The cards are Blossoming Sands, Thornwood Falls, Dazzling Angel, Clinquant
Skymage, Dwynen's Elite, Good-Fortune Unicorn, Guarded Heir and Youthful
Valkyrie. Elf Warrior Token is 1/1 green with Elf and Warrior subtypes;
Knight Token is 3/3 white with Knight subtype and no extra abilities.

The entry triggers observe both ordinary zone changes and token creation,
including simultaneous entries. Unicorn counters bind the entering creature
without making it a target. Skymage and Valkyrie bind their own incarnation.
Dwynen checks its intervening condition twice and excludes the historical
source incarnation, so the returned physical card can be another Elf for
the old pending trigger. Existing source-bound stack validation remains
separate from event-bound counters.

References: XMage at `a5c90fe180021e70e2a644ade00eeab07f857a40`, card paths
in `docs/design/fdn_fixture_mechanics_v1.md`, the two token classes under
`Mage/src/main/java/mage/game/permanent/token/`, and
[Wizards' FDN release notes](https://magic.wizards.com/en/news/feature/foundations-release-notes).
Source inspection establishes the intended cases. Executable XMage
comparisons remain part of the final fixture gate.

| Check | Result |
| --- | --- |
| Batch B rules | 21 passed: exact costs, printed characteristics, land mana choices, life-gain timing, draw events, intervening conditions, exact tokens, simultaneous entries, source death, control changes, stale incarnations and pending-order restoration. |
| Existing Limited integrations | Batch A 19, priority 6 and session 9 passed, including deterministic binary games. |
| Registry | 46 card-definition tests passed with the Limited feature; 46 passed without it. The original v32 hash remains `64c82a261e078f1a`. |
| Native decoder | 128 passed, three ignored with the Limited feature. Batch A records remain readable. |
| Mutation boundaries | Four passed: publisher and resume reject v32 and prior batch A records before store mutation under batch B. |
| Python | 17 Limited, 13 Pauper-manifest and eight V2 golden tests passed. |
| Lint | Library, Limited binary and four Limited integration targets passed Clippy with `-D warnings`; formatting and diff checks passed. |
| External gameplay | Two supported-subset games with seed 123 reached natural terminal outcomes and had identical transcripts. Four casts and 52 land plays occurred per replay. This exercises the interface, not all card branches. |
| Hosted CI | Batch A's full matrix passed at `e97f8eae`. Batch B's full matrix is pending its PR run. |

The Limited feature now uses `kernel_carddb/v34`, hash `d2b479e9d5990f07`,
with a separate frozen `FdnFixtureBatchB` native catalog profile. Earlier
literal pairs remain unchanged. Publication and resume require an exact
match with the live build. The runtime nine-deck catalog is unchanged.

Small manifest: CPU only; GPU ordinal none; seed 123. Validation ran in the
owned `C:/Users/haley/mtg-kernel-fdn-batch-b-codex` worktree, with two Cargo
build jobs and the `C:/Users/haley/mtg-kernel-fdn-batch-b-target` cache.
Rust 1.94.1 (`e408947bf`), Cargo 1.94.1 (`29ea6fb6a`), MSVC linker file
version 14.50.35725.0; Python 3.12.10 remotely and 3.11 locally. Jack's PC
remained reserved for the lead's formal work. No GPU run or formal
measurement occurred.

Focused commands used `cargo test --locked -p mtg-kernel` with the
`limited-fdn-fixtures` feature and the integration targets above, then
library filters `card_def::`, `native_training_store_run_v2::`,
`pre_fdn_profile` and `prior_fdn_batch`. Default card tests omitted the
feature. Logs are `C:/Users/haley/fdn-batch-b-regressions-001.log` and
`fdn-batch-b-regressions-002.log`. The first script's production-feature
debug build was refused because that feature requires release; the
four mutation checks subsequently passed on the normal Windows test build.
CI retains the production-feature release checks.

External inputs were constructed 40-card subsets:

- UG: 12 Forest, 12 Island, four each Thornwood Falls, Clinquant Skymage,
  Dwynen's Elite and Llanowar Elves.
- WG: ten Forest, ten Plains, four each Blossoming Sands, Dazzling Angel,
  Good-Fortune Unicorn, Guarded Heir and Youthful Valkyrie.

The deterministic menu policy prefers land, spell, activated ability and
mana actions, then the first offered choice. Each replay reached a natural
terminal after 2,231 policy and physical decisions. The driver and report
are `C:/Users/haley/fdn-batch-b-external-002.py` and
`C:/Users/haley/fdn-batch-b-external-002.json`.

| Input/output | SHA-256 |
| --- | --- |
| Original Pauper registry | `af9d725658d238a6a1b0c99b52c12ff6823bdcaf8147a77556ee2e3b0b67d6bc` |
| Committed Limited extension | `4cfce1d99853bfcdb0df8c0b94be0b93643038c2d35c618ea87a81589aa4d82e` |
| Committed batch B rules test | `9363ddad9b31222500e10e9fb725755755c2cd818481eac2394b0f38097a5a96` |
| Constructed UG input | `4e3937f047108cd80795cd664884ab55e10809ad222646e1ef7733698afb0f45` |
| Constructed WG input | `1ec587e6592a10d076ad1d18b9fa494518d65bf854f4980153fc3833a4479389` |
| Both gameplay transcripts | `7cef84b44941cb7c67786c854c37125711492791e381a45d303f2ab2a86e782e` |
| Tested debug binary | `80a23a358ffef02787f4f7be1866385fd2108096a417265a2293dc08069f6c1f` |

Coverage is 21/286 reference names, with 265 missing. The unchanged original
UG fixture has 23/40 supported copies; WG has 25/40. Their union still needs
22 distinct names, and both original decks continue to be refused by the
resolver. Remaining cards, mulligans, current combat allocation/trample,
relevant planeswalker targeting and executable XMage comparisons remain
outstanding. The full fixture gameplay goal remains active.
