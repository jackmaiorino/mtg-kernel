# FDN Luminous Rebuke implementation and verification

Luminous Rebuke is implemented as a white instant for `{4}{W}`, mana value
five, that destroys target creature. A tapped creature target reduces only
the generic casting cost by three. Untapping after the cast does not
reprice the paid spell or invalidate its creature target.

The cast offer requires an actual legal, payable target completion. The
target menu and target-action validation share that check, and final
payment derives the reduction from the selected target. Invalid actions
leave state unchanged. Older reducers and modal casts retain their prior
offer path. Existing pending-cast, source and target contracts provide
save/restore and incarnation safety without new serialized state.

| Check | Evidence |
| --- | --- |
| Core and gameplay | `fdn-rebuke-checks-001` exited zero; all fifteen gameplay cases passed. Checks-002 phase 1 repeats those cases successfully. |
| Generated identity | FDN v45 `076524c7f95c3147`, 201 definitions. Checks-002 phase 2 passed all 46 definition checks. |
| Catalog history | Checks-002 phase 3 passed 139 native-record checks, with three existing ignores. Prowler v44 and all earlier frozen profiles remain readable. |
| Limited sessions | Checks-002 phases 4 and 5 passed 66 public session cases and seven private custom-session cases. |
| Prior gameplay | Checks-002 phase 6 passed 234 cases across fifteen integration suites, including Prowler and Pauper counterspells. |
| Python | All twenty deck/client cases passed on Python 3.11. The WG fixture contains two Rebuke copies; its supported-copy count is 38. |
| XMage | Eleven strict-choice cases passed in `fdn-mage-rebuke-001`; [Mage PR #10](https://github.com/jackmaiorino/mage/pull/10) records reference boundaries and hashes. |
| Limited lint | Checks-002 phase 7 passed all-targets Clippy with warnings denied. |
| Default compatibility and engine regression | Checks-002 phases 8/9 passed: 68 default session cases and the exact v32 golden. Checks-003 exited zero: default workspace all-targets Clippy and 125 Limited/124 default engine cases passed. |
| Production mutation boundaries | `fdn-rebuke-production-003` is running the release publication/resume and live-profile round trip checks after correcting the launcher compiler-path spelling. |
| External natural-terminal replay | `fdn-rebuke-external-001` exited zero: two seed-123 games ended naturally, with identical results and transcripts. |
| Hosted CI | Draft [kernel PR #134](https://github.com/jackmaiorino/mtg-kernel/pull/134) is running at `deaa8b29`; no green claim. |

Focused cases cover exact cost/MV/color, the white pip, two-mana tapped-only
and five-mana target menus, protected targets, own and legendary creatures,
land payment, target loss/reentry, an actual Quirion Ranger untap response,
ward as a separate payment, pending-target and ward restore, and the Prowler
death interaction. A synthetic staged-target change also checks an unpaid
cast losing its discount; the actual post-cast response case checks the
distinct paid-spell behavior.

Functional source is local `62587f74`, remote
`9b49d9e0020035468f2016beeee183152c543524` in the owned
`C:/Users/haley/mtg-kernel-fdn-rebuke-codex` worktree. Rust/Cargo 1.94.1,
MSVC linker 14.50.35725.0, two Cargo jobs, no incremental compilation or
debug symbols, seed 123 and no GPU. Small manifests bind source/input and
output hashes. The current launcher uses the direct pinned Cargo executable
and verifies its path and parent before stopping only that owned tree.
It monitors the 60 GiB reserve and declared incremental volume-allocation
cap every three seconds. Checks-002 hit its 2 GiB volume-allocation limit during default Clippy.
Cargo finished between identity inspection and the stop attempt, leaving
that phase without a recorded exit. Checks-003 handles that race, reruns
the unrecorded phase and completes the remaining phases successfully.
A volume-allocation delta includes other writers and is not an attribution
of physical bytes to this build. Logs remain outside Git.

The first Python preparation check used an incorrect WG copy expectation
and a nonexistent client test-module name. Reading the unchanged fixture
and discovering the actual module corrected both; the proper twenty-case
run passed. No formal experiment was started.

Both original deck SHA-256s remain pinned and unchanged. Coverage is UG
37/40 and WG 38/40, with four distinct names missing: Celestial Armor,
Sylvan Scavenging, Uncharted Voyage and Witness Protection. London
mulligans, complete original-deck natural terminals and deterministic
external replays remain required by the active fixture goal. No
playing-strength or full-set Foundations claim.

Release preparations 001 and 002 exited 101 before running tests: strict
build capture rejected relative/forward-slash compiler paths. The third
launcher sets the pinned `RUSTC` using the required drive-and-backslash
Windows spelling; the production source and toolchain pins are unchanged.
Failed preparation logs and manifests remain preserved.

The symbol-free interface binary is 5,788,672 bytes, SHA-256
`eddc2bd7e41ae8c930d8602516d761da5c1783fe8b0d2272d3975f368a2a47fe`.
Verified cold copies live under both PCs' `fdn-pinned-binaries` directories
and `E:/pinned-binaries/`, each keyed by that hash.

The external deck is twelve Plains, eight Forest, eight Prowler, eight
Elvish Mystic and four Rebuke, seed 123 and episode 7. Both games end
naturally with P0 winning, 374 policy steps and 371 physical decisions.
Each casts two Rebuke, five Prowler and five Mystic, selects two spell
targets and exposes Rebuke in four decision menus. Transcript SHA-256
`b30d98ac15a24f60cb576cbff18717d19db83d83076503f6a6a2d35063a314ab`.
Result SHA-256
`33eb5f15ee5728a699a6659fdbd3458410a2ae36274117d40e0c2b5bdda9bc61`;
log SHA-256
`a8dd25ec97a78df9ae769c5dd240ceb705d243d82f41f7681bc132852a8c224e`.
This verifies interface completion and replay. Individual discounted
payment/ward rules are established by the focused kernel/reference cases.
The custom games do not complete either original fixture deck.
