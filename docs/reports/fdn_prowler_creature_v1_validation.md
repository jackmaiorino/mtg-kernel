# FDN Prowler implementation and verification

Cackling Prowler is implemented in the opt-in FDN catalog: green 4/3 Hyena
Rogue for `{3}{G}`, ward `{2}`, and an intervening morbid trigger at the
beginning of its controller's end step. A qualifying death adds one +1/+1
counter to the source's exact battlefield incarnation.

Creature death history records actual battlefield-to-graveyard moves with
the type before departure. Opposing creatures and tokens count. Discard,
bounce, exile and noncreature deaths do not. The stamp includes the active
player because the engine's round number can span both players' turns.
Untap clears the stamp; absent history preserves prior default bytes/hashes.

The end-step marker freezes the condition at entry, and the conditional
effect rechecks it at resolution. Bound-source operations inside a
conditional program are materialized and authenticated recursively. Late
deaths/entries cannot create a missed trigger; departed/reentered sources
cannot inherit an old counter.

| Check | Evidence |
| --- | --- |
| Core compile | `fdn-prowler-checks-001` phase 1 passed in 29.85 seconds. |
| Gameplay | All 14 cases passed in checks-001 phase 2 and checks-003 phase 1. Casting, death/timing boundaries, ward, source incarnation, death-history restore, queued trigger restore and pending ward restore. |
| Generated identity | FDN v44 `cc304ceaf678738f`, 200 definitions. Default Pauper v32 and all earlier frozen profile literals remain unchanged in source. |
| Python | All 20 deck/client cases passed on Python 3.11. |
| XMage | All 12 strict-choice cases passed in `fdn-mage-prowler-001`; [Mage PR #9](https://github.com/jackmaiorino/mage/pull/9) records reference boundaries and hashes. |
| Catalog | Checks-004 phases 2 and 3 passed: 46 definition checks, 138 native-record checks and 3 existing ignores. Earlier compile failures exposed a missing consumer match arm and overly narrow historical-fixture test configuration; both are corrected. |
| Sessions, regressions and lint | Checks-004 remaining phases are running. |
| Production mutation boundaries | Pending release verification, including prior-v43 publication/resume refusal and current-profile round trip. |
| External natural-terminal replay | Pending Prowler interface build and cold binary preservation. |
| Hosted CI | Draft PR dispatch; no green claim. |

Kernel functional source is local `6abb312d`, remote `bf4f45f` in the owned
`C:/Users/haley/mtg-kernel-fdn-prowler-codex` worktree. Rust/Cargo 1.94.1,
MSVC linker 14.50.35725.0, two Cargo jobs, no incremental compilation,
no debug symbols, seed 123 and no GPU. Small manifests bind source/input
and output hashes. Checks-004 guards a 384 MiB output allowance and the
60 GiB free-space reserve, monitoring only its own identified Cargo tree.
Compiler-failure logs remain preserved outside Git; no measurement run was
started, and no frozen experiment or other agent's tree was changed.

Both original decks retain their pinned SHA-256s. Coverage is now UG 37/40
and WG 36/40 copies, with five distinct card names still missing. London
mulligans, original-deck natural terminals, their deterministic external
replays and complete verification/CI remain required by the active goal.
This slice makes no playing-strength or full-set FDN claim.
