# Public combat opportunity and bounded diagnostic

The broader public-menu census completed all 256 parent BO3 paths, 600 games, 53,714 gameplay rows and 53,300 choice rows. These per-match counts exactly reproduce the preceding targeting census. Across all hands, pure combat menus occur in251 matches and4,171 physical decisions. With the opposing hand publicly empty, they occur in103 matches,146 games and787 physical decisions, covering all eight decks. The 2,065 flat choice rows include multiple substeps of the same physical decisions and are not independent tactical examples.

| Candidate deck | Matches with opponent-empty combat /32 | Games | Physical decisions | Choice rows |
| --- | ---: | ---: | ---: | ---: |
| Affinity | 17 | 24 | 112 | 190 |
| Burn | 10 | 12 | 32 | 55 |
| Elves | 17 | 21 | 100 | 214 |
| Faeries | 12 | 14 | 46 | 88 |
| Gates | 13 | 22 | 279 | 1,039 |
| Rally | 9 | 14 | 42 | 82 |
| Terror | 8 | 12 | 51 | 137 |
| Wildfire | 17 | 27 | 125 | 260 |

Both-hands-empty combat is smaller:45 matches,57 games,295 physical decisions and987 choice rows. All observed combat menus also have empty stacks, but that does not establish absent pending triggers, unknown effects or adversarial replies. Physical seat 0 contributes 450 opponent-empty combat groups in 56 matches; seat 1 contributes 337 in 47 matches. These are opportunity counts on a consumed fixed domain, not certified labels, blunder prevalence or anticipated strength gains.

The frozen census predicates use actor-relative public hand counts, stack presence, phase and every action-kind set. They use neither model confidence nor outcomes to select roots. Pure combat means declare_attackers, declare_blockers_for_attacker, choose_attacker_inclusion or choose_blocker_inclusion only. All other menus remain in the full report. The strata overlap; do not sum them as disjoint datasets.

Evidence: E:/mtg-meta-recovery-20260921/public-opportunity-dispatch-001/{completion.json,analysis.json,formal-result.json}. New scripts public-opportunity-census.py, public-opportunity-dispatch.py and public-opportunity-worker.py preserve the original evidence. Seven reader checks cover actor-relative indexing, empty-hand/stack strata, mixed and forced menus, group deduplication and actor mismatch. Full outputs agree exactly between qualified one- and four-worker runs on 16 preselected timing pairs.

Kimi's identified zz_obs_probe_kimi Cargo compilation occupied the maintainer's native lane during read qualification, so the reader did not benchmark or dispatch there. The compute host's four workers qualified in 7.0319 seconds versus 9.5980 serial, and completed the full read in 20.4567 seconds including dispatch/recovery. Qualification cost 16.6299 seconds; tiny remote staging cost 2.1186 seconds with old immutable input caches reused and rehashed. The 114.6287-second forecast scales small-workload startup and is conservative, not a confidence bound. Summed CPU time 67.125 seconds; process read counters 649,965,269 bytes; maximum start/end per-process RSS sample 79,892,480 bytes, not peak/aggregate memory. Fresh RunPod inventory returned 403, no allocation. All 256 outputs are complete and no matches or training were launched.

## Implemented next engineering slice

terminal_tactics/public_combat.rs adds an offline bounded minimax diagnostic with no gameplay or trainer call site. It admits combat declaration menus with an empty opposing hand and stack. For each legal root action it follows combat declarations and pass actions, using the real V4 legal menus. Own subsequent choices maximize root-player terminal-return bounds; opponent choices minimize them. Unsupported choices, budget exhaustion, leaving combat or crossing the information boundary retain[-1,+1]. A natural terminal is a point bound. Opponent uncertainty therefore cannot be silently treated as a cooperative pass, draw or loss. Equal per-root-action transition budgets prevent one root from consuming another's allowance.

The information boundary compares turn, library vectors, library-knowledge records, draw counts and opposing hand emptiness after every transition. This is a conservative engine diagnostic, not a general proof that every card effect is independent of hidden information. No model logits or values choose branches. The borrowed live session is not advanced.

All five native tests passed in public-combat-tests-002: lethal attacks on both seats; an opponent blocker preventing a cooperative win; defensive roots distinguishing certain death from an unresolved surviving line; depth/node exhaustion and nonempty-hand abstention; and Ninja of the Deep Hours' optional draw. The draw control verifies that the ordinary diagnostic stops before the unsupported choice, then separately traverses that choice in the test and confirms the actual library draw crosses the information boundary. Unknown library identities and order changes preserve the positive attack result, root V4 observations remain unchanged and repeated analysis is exact. These are constructed engine cases, not a natural-game performance sample.

The first build, public-combat-tests-001, compiled but all three initial tests failed in fixture setup because Plains and Ornithopter were absent from the pinned registry. The corrected fixtures use supported cards; the original failure receipt is preserved. The second run compiled and passed in267.7002seconds total, with the test body reported as0.00seconds at the harness's precision. Both exact compiled source files and toolchain details are retained in tests-002. Do not treat build time as search latency. check-public-combat.py limits this run to the named fixed engineering tests, four BelowNormal build jobs and32GiB memory headroom. It permits only the previously identified Kimi source-probe compilation to coexist in a separate checkout/target and rejects unknown native or training/evaluation owners. This narrow correctness-check allocation is not throughput qualification or permission to overlap frozen measurements.

Before natural labels or deployment: reconstruct actual recorded V4 roots with exact replay, audit any hidden-information dependencies beyond the current guards, and qualify end-to-end tree costs. Measure how many roots produce useful terminal bounds and how many abstain. The current prototype has no recorder, gameplay or training integration. Do not train merely because opportunity counts are nonzero. Broader opponent beliefs, learned openings/sideboarding and human calibration remain necessary for the goal.

Known Fable zero-read HTTP429 remains until September 22 at 07:00 EDT. This reversible prototype proceeds under the maintainer's explicit execution authority with that review gap recorded, not independent endorsement. All frozen measurements, terminal rewards, optimizer ancestry and CP7 exclusion remain unchanged.
