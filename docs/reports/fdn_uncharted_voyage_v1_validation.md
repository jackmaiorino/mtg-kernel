# FDN Uncharted Voyage implementation and verification

Hosted CI at `2ac7c6ee3f33f0d1c548802e375d42c914cfe38f` passed all eight checks: Rust on
Ubuntu and Windows, all four Python shards, formatting/lint and path detection.
Run: https://github.com/jackmaiorino/mtg-kernel/actions/runs/37017088413. Earlier pending entries below record the original
verification sequence. The subsequent status/report update changes no tested
code, workflow, fixture or catalog bytes.

Uncharted Voyage targets a creature for `{3}{U}`. Its owner chooses top
or bottom of their library, then the caster privately surveils one. The
two effects resolve in order without an intervening priority or
state-based action window. Losing the only target skips both effects.

The owner-placement interpreter shares Deem Inferior's bound choice and
answered-frame machinery. It authenticates each card's printed recipe;
Deem Inferior still uses second-from-top or bottom. Surveil binds the
whole library, privately reveals its top card and suspends for keep or
graveyard. It excludes tokens from card selection while retaining them
for the normal post-resolution state-based cleanup. New enum variants
are appended; older variants and default card identities are preserved.

Functional source: `16fec7c06ce2f7dba3148e760dc18c77320f2011`.
The owned local checkout is `mtg-kernel-fdn-voyage-codex`; the matching
HaleysPC checkout uses the same commit. Rust/Cargo 1.94.1, MSVC linker
14.50.35725.0, two Cargo jobs, no incremental compilation, no debug
symbols, seed 123 and no GPU. These are small CPU correctness checks.

| Check | Observed result |
| --- | --- |
| Voyage rules | Checks-001 phase 1: 19 passed, including exact cost, owner/controller, both placements, target incarnation changes, ward, token targets, privacy, invalid actions and pending/answered choice restoration. |
| Shared effects | The same phase: all 16 Deem Inferior and 11 Preordain cases passed. |
| Generated identity | Checks-001 phase 2: 46 definition checks passed. FDN v46 is `f3989f46ebcb5034`, with 202 definitions and Voyage at ID 201. |
| Catalog history | Checks-002 phase 3: 140 native-record checks passed, with three existing ignores. Rebuke v45 and earlier profiles remain readable but are rejected as stale at live mutation boundaries. |
| Limited sessions | Checks-002 phase 4: 62 passed across public sessions, custom decks, priority and combat. |
| Prior gameplay | Checks-004 phase 5: 223 passed across fourteen prior FDN and Pauper counterspell suites. |
| Engine and default compatibility | Checks-004 phases 6/8/9: 125 Limited engine, 124 default engine and 27 default public-session cases passed. Build-001 phase 2 separately passed the exact v32 golden. |
| Lint | Checks-004 exited zero; Limited all-targets and default workspace all-targets Clippy passed with warnings denied. |
| Python | Twenty deck/client checks passed on Python 3.11. Original deck hashes and fail-closed admission remain checked. |
| XMage | Eleven strict reference cases passed; [Mage PR #11](https://github.com/jackmaiorino/mage/pull/11) records the command, hashes and setup corrections. |
| External interface | External-001 exited zero. Two fixed-seed games reached natural outcomes with identical results and transcripts; each cast Voyage eight times. |
| Production mutation boundaries | Production-003 exited zero: two prior-profile checks, 26 publication/resume boundaries and one profile roundtrip passed. Combined-feature release Clippy passed with warnings denied. |
| Hosted CI | Pending; no green claim. The new rules suite is included in the Limited CI command. |

Local retained job prefixes are `C:/Users/Jack/fdn-voyage-checks-001`,
`-002`, `-004`, `fdn-voyage-build-001` and `fdn-voyage-external-001`.
Each has a manifest binding source, toolchains, input/output hashes and
the guarded launcher's reserve and allocation allowance. Logs stay
outside Git. The verified release prefix is
`C:/Users/haley/fdn-voyage-production-003`.

The pinned external binary is 5,821,952 bytes, SHA-256
`ec0bda5397d0d8c620ff3b44517446315fea0383bb7bdc06937d970a738797d3`.
Copies under both PCs' `fdn-pinned-binaries` directories and
`E:/pinned-binaries/` were verified by hash.

The external smoke deck is twelve Island, eight Forest, eight Llanowar
Elves, eight Elvish Mystic and four Voyage, seed 123 and episode 7. Both
games end naturally with P0 winning at 1,351 policy steps and 1,339
physical decisions. Each answers eight owner-placement and eight
surveil selections. Transcript SHA-256:
`ce3c6c93dda673b87a0f6705e0f5e960db85fd2263dbdabdc57d44b4e02e7cb7`.
Result SHA-256:
`a7eadd3d7b1ca8cdcfc781794a51f61d4d255fd3096c9fe6b0cd26acde3f713f`.
Log SHA-256:
`cd15863161cafab7a433ced3d5d1c5a7b0ea05f1599bb85a04a05392f470631a`.
The retained checker SHA-256 is
`20f8e5848a6a947f16ef9e94b36c7767f3c330b7c534d355027b6993e90199c9`.
This establishes interface completion and replay for the smoke deck.

Preparation failures remain retained: the initial rules suite used a
nonexistent finish-action name, then an incorrect token definition name;
both were corrected. Checks-001 found the live-record fixture still
using v45; Checks-002 corrected it and passed all record cases. Two
temporary launchers used incorrect counterspell test-target spellings;
Checks-004 used the discovered existing target and passed. No formal
measurement was started.

Release preparation 001 was stopped for the demonstrated v45 fixture
error. Preparation 002 stopped at its 2 GiB volume-allocation allowance.
Its measured volume delta includes concurrent writers and is not a
physical-byte attribution to this build. Preparation 003 declares a
4 GiB allowance and keeps the 60 GiB reserve. The toolchain is unchanged.

Both original deck files retain their pinned hashes. Registry coverage
is UG 38/40 and WG 38/40. Celestial Armor, Sylvan Scavenging and Witness
Protection remain, along with London mulligans and complete original-deck
external games. This slice does not complete the active fixture goal or
establish full-set Foundations support or playing strength.
