# FDN targeted-spell validation

Local source `8da4554e` (merged with parent CI correction at `80f716f5`)
adds Bite Down, Felling Blow, Fleeting Flight and Joust Through. Matching
HaleysPC source is `dc5c4b8fb1dfdaefcc1f4f6db3188827cd4b8d74`.
Windows-only live-profile expectations are corrected in local `08db5e0c`
and remote `94abfc3c0501f619b17ac530a7de843e62876f77`.
See [rules contract](../design/fdn_targeted_spells_v1.md).

| Check | Result |
| --- | --- |
| Targeted spells | 24 passed: casting costs/timing, sequential target restrictions, individual legality, control/hexproof changes, counters before power, stale incarnations, lifelink/deathtouch, combat-only prevention, cleanup, unpreventable damage, trample and loyalty SBAs. |
| Existing Limited gameplay | All 109 passed: fixture A 19, B 21, combat cards 19, legend 15, combat 20, priority six, custom session nine. |
| Session restore | Four feature-enabled checks passed, including the second target of both power spells with identical action identity, response and environment binding. Serialized GameState tests cover pending targets, loyalty and prevention. |
| Definitions and records | All 46 definition and 131 catalog-record checks passed. Three existing catalog tests remain ignored. v36 is readable but no longer live. |
| Native buffers | The new loyalty/prevention refusal check passed and preserved destination buffers. |
| Default compatibility | All 28 state tests, the exact environment-hash golden, the v32 CardDB hash tripwire and two default session restore checks passed. |
| Build and lint | Default workspace and feature-enabled all-target Clippy passed with warnings denied. External binary built. |
| Production boundaries | All 11 Windows release checks passed: ten v32/v33/v34/v35/v36 publisher/resume refusals and the live v37 construct/seal/decode/validate round trip. Combined Limited/production Clippy passed with warnings denied. Rerun 010 exited zero from clean committed remote source `94abfc3c`. |
| Python | All 41 passed: deck import 14, client six, flat-V2 goldens eight and Pauper manifest 13. |
| External replay | Two identical seed-123/episode-7 games ended naturally. Each cast Bite Down twice, Felling Blow once, Fleeting Flight twice and Joust Through once. |
| Hosted CI | Head `80f716f5` passed lint and both Linux Python shards before the Windows assertion correction. Updated source `08db5e0c` has a fresh run; remaining hosted gates are pending. |

External deck: eight Forest, eight Plains, eight Llanowar Elves, four
Treetop Snarespinner and three copies of each new spell, totaling 40.
The terminal is `p0_win`, 404 policy steps and 401 physical decisions.
The driver chooses lands/spells, includes attackers/blockers and uses lower
half damage decisions. This verifies constructed transport and replay,
not original-deck completion, executed XMage parity or playing strength.

Limited v37 is `fc090e5b2a7b3e4f`. It contains 185 full definitions,
including tokens, and one partial reference planeswalker. Ajani's initial
four loyalty, damage and zero-loyalty SBAs are exercised in focused tests;
its abilities and combat defender support are unfinished, and admission
refuses the reference deck. Optional new state is publicly projected and
saved/restored; historical absent-field bytes/hashes and frozen flat format
layouts remain unchanged. The latter refuse unrepresentable live contexts.

Small manifest: CPU correctness checks on HaleysPC, two Cargo build jobs,
GPU ordinal none; seed 123, episode 7. Rust 1.94.1 (`e408947bf`), Cargo
1.94.1 (`29ea6fb6a`), MSVC linker `14.50.35725.0`, Python 3.12.10 remotely
and 3.11 locally. Jack's PC remains reserved for the lead. No training,
formal measurement or paid compute was launched. Source was committed
before production checks; the strict source guard stays enabled.

| Input/output | SHA-256 |
| --- | --- |
| Base registry | `af9d725658d238a6a1b0c99b52c12ff6823bdcaf8147a77556ee2e3b0b67d6bc` |
| FDN extension | `724279b725420b2a0afad09e8a512143a616cb91e6502cf37bc4534356c0825e` |
| Spell integration tests | `48e4dc8f82beac8a52686bc1271440f8fe8ff4f3f7b7ae93fdaae0e700572126` |
| Loyalty implementation | `ecfdec7353d0aac7297c215849fa1848c3daa8fceecb6875cc35c21357bc83aa` |
| Input deck, UTF-8 LF with trailing LF | `11c57f761fc100813e933e6474a98412d171424f00706ad4e9a962b94bf5496b` |
| Tested binary | `02fe68f593b8ba68c08b1200911d3b67761130fc5644ab1031788a7ab11a7bcd` |
| External transcript | `5751cfc1174569cd90c0819e7c83b45702e553340b3e5cc813f2ae7d9fdbd2ef` |
| External driver | `3a99b4f0da5d63845cb7f71e08394e992811e4b7f176ca0d11ef3686c7084e24` |
| External result JSON | `ed5a8c9d7f54865163a6a4d1b2624075d8db6643b8e13c99916a032181c01aa1` |

Logs and replay files remain outside Git under `C:/Users/haley/`:
`fdn-targeted-tests-006.log`, `-007.log`, `-008.log`, `-009.log`,
`-010.log` and
`fdn-targeted-external-001.py/.json/.log/.exit`. The earlier failed logs
remain available: 001 found an incomplete codegen match, 002 found the
loyalty struct/module wiring errors, 003 a duplicated helper, 004 the
missing target-contract shapes and a payment timing assertion, and 005
an incorrect survival assertion. 008 passed the ten refusal checks but
found a stale live-profile assertion; 009 was refused at build preflight
because its copied assertion fix had not yet been committed remotely. The
remote fix was committed before 010, which passed with the guard enabled.
Passing checks above retain their tested source identities.
Full local Rust-suite success and executed XMage parity are not claimed.

Parent #121's default Linux CI failure was a legend restore test using
feature-only Dwynen without a feature gate. Parent correction `e23752e9`
adds that gate. The unchanged feature test and both default restore
checks pass in this branch. Its hosted CI is pending.

The full fixture goal remains active. Original UG now has 28/40 full copies,
WG 29/40, and 15 distinct fixture names remain unsupported. The 286-name
reference has 28 full, one partial and 257 missing names. Original fixture
files retain their pinned SHA-256s. Mulligans, remaining cards/tokens,
executed XMage comparisons and passing CI remain required.
