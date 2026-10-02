# First Foundations combat-card slice validation

Source commits `d57f3d411522bfc915faa47b1f7669c43e83a4c3` and
`41af18f62222f6612e8c85ebb0f44739ed901235` complete Beast-Kin Ranger and
Overrun, with exact-incarnation temporary boosts and real trample damage.
HaleysPC's matching source is committed as
`bf6facd81093df1200128db712b0c22e25b5d2b2` on the owned verification branch.

Dwynen's continuous Elf bonus and attack trigger are tested, but the card
remains `partial` until a real legend-rule choice is implemented. Rust and
Python custom-deck admission reject it. The opt-in catalog has 180 full
definitions and one partial definition, including the two token definitions.
Its v35 identity is `633a324030f31c47`; the default 162-card registry retains
v32 `64c82a261e078f1a`. Earlier catalog identities remain separately readable.

| Check | Result |
| --- | --- |
| New mechanics | 19 passed: entry filters, simultaneous entries, incarnation binding, cleanup, trigger-template authentication, Overrun snapshots and control changes, actual trample combat, continuous Elf bonuses, resolution-time attack counts, source removal and pending JSON restore. |
| Existing Limited | All 75 passed: batch A 19, batch B 21, combat 20, priority six and custom session nine. Two additional library session-restore checks passed. |
| Definitions and records | All 46 card-definition and 129 native run-record tests passed; three existing native tests remain ignored. |
| Default compatibility | All 28 state tests and the exact environment-hash golden passed. |
| Lint | Default workspace and Limited-feature all-target Clippy passed with `-D warnings`; formatting and diff checks passed. |
| Python | 40 passed: 13 deck-import, six client, eight flat-V2 golden and 13 Pauper-manifest tests. |
| External client | Two seed-123 replays reached the same natural terminal with identical transcripts; each cast six Rangers and two Overruns and answered four damage-range choices. |
| Hosted CI | Pending this PR. Parent combat PR #119 has passed lint and all four Python shards; its Ubuntu and Windows Rust jobs remain in progress at this report. |

The external check used a supported constructed mirror: each deck contains
20 Forest, four Llanowar Elves, 12 Beast-Kin Ranger and four Overrun. Episode
7 ended naturally in `p1_win` after 370 policy steps and 365 physical decisions.
The driver prioritizes land plays, Overrun, other casts, attack/block inclusion,
lower-half damage answers, then passes. The identical replay validates these
rules and their external transport. It does not establish original fixture
completion, executable XMage parity or playing strength.

Small manifest: CPU only on HaleysPC, two Cargo build jobs, GPU ordinal none,
seed 123. Rust 1.94.1 (`e408947bf`), Cargo 1.94.1 (`29ea6fb6a`), MSVC linker
file version `14.50.35725.0`, Python 3.12.10 remotely and 3.11 locally.
Jack's PC remained reserved for the lead's formal work. No training,
formal measurement or paid compute was launched.

| Input/output | SHA-256 |
| --- | --- |
| Input deck, UTF-8 LF with trailing LF | `408ea3797f1dc5afffd8d26a558dc26f8faf2e9266d7b914f935de7a4bc0ff9f` |
| Unchanged base registry | `af9d725658d238a6a1b0c99b52c12ff6823bdcaf8147a77556ee2e3b0b67d6bc` |
| FDN extension | `a487cc5a7b19fb6d2f324b6b26d54a606606e0f1639f62be513361041b43bdcd` |
| Final mechanics tests | `a1b2cd999b381303fa734eae226f60647387a2a62378b676a858217f3ceb5051` |
| Binary used by the external check | `fdf6f0ad0798e97153a8e0eccb8e8c1813fda6eb54f04a9695fa1e0160eaca24` |
| External transcript | `49960f4a9802bc0ed709b129ec14ee65fda92ef382cbf74907c417cb55cd8139` |
| External result JSON | `85d222c0230a3ca91246ae1346d6a52811c041dd38aa72bb83ec2e12d2e8a139` |

Logs are `C:/Users/haley/fdn-combat-cards-tests-004.log`, `-005.log` and
`-006.log`. The first contains passing integration/restore checks followed
by an outdated full-definition count assertion. The second verifies the
corrected count, records and default regressions, then reports an unreachable
test match arm. The final log verifies all 19 mechanics tests, Limited Clippy
and formatting after removing that arm; its exit file is zero. Earlier failed
logs are retained. No full local Rust or Python suite completion is claimed.
The external driver and result are `C:/Users/haley/fdn-combat-cards-external-001.py`
and `.json`; the result captures the tested binary hash before later default builds.

The original fixture goal stays active. UG resolves 25/40 copies and WG 26/40;
20 distinct fixture names still need full support, including partial Dwynen.
The 286-name reference has 23 full, one partial and 262 missing definitions.
Mulligans, remaining cards/tokens, the legend rule and executable XMage
comparisons remain required. Rules references were inspected in XMage at
`a5c90fe180021e70e2a644ade00eeab07f857a40` and the
[official FDN release notes](https://magic.wizards.com/en/news/feature/foundations-release-notes).
Source inspection is not executed parity evidence.

## Rebase onto main (2026-10-02)

After merging main (#112, `kernel_carddb/v34`, 192 Pauper definitions), the
three combat cards take IDs 208 through 210, the Limited registry has 211
definitions, and the Limited database is `kernel_carddb/v37`, hash
`41607b95d7a1ec42`. The measurements above describe the original build.
