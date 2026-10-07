# Public-feature continuation result, September 20

Complete panel: 2,352 matched BO3 matches, 5253 natural games. All saved match hashes reverified, unique case sets equal across all three arms. No unresolved jobs or outcome prefixes.

| Arm | BO3 wins / 784 | Game-one wins / 784 |
|---|---:|---:|
| g115 | 645 | 606 |
| control | 633 | 611 |
| structured | 629 | 610 |

**NO-ADVANCE.** Structured versus control: -4 wins (-0.51 percentage points), paired 95% interval [-1.91, +0.89] points. Structured versus g115: -16 wins (-2.04 points), interval [-3.57, -0.51]. The primary improvement gate and g115 match-win floor failed; game-one retention passed. This is no evidence of benefit from this particular feature intervention at this training horizon, not evidence that public features cannot help.

| Candidate deck | g115 | control | structured |
|---|---:|---:|---:|
| Affinity | 96/112 | 94/112 | 94/112 |
| Burn | 85/112 | 87/112 | 88/112 |
| Elves | 94/112 | 92/112 | 89/112 |
| Faeries | 83/112 | 78/112 | 80/112 |
| Rally | 103/112 | 103/112 | 100/112 |
| Terror | 97/112 | 97/112 | 97/112 |
| Wildfire | 87/112 | 82/112 | 81/112 |

Deck/seat/start aggregates are descriptive. Each ordered matchup has only eight paired seeds, two seats and balanced game-one starts. Four observations per seat/start subcell are insufficient for confident individual-matchup conclusions. Bootstrap retains both seats and all arms jointly within the 49 matchup strata. One lineage, seven exact registrations, familiar development V3 opponent, keep-seven openings and static sideboards do not establish human or broad-meta strength. No CP7 outcomes entered selection.

Training: 4,000 primary natural games / 400 updates, plus 20 extra qualification replay games. Control 1299.33 native seconds, structured 1068.43; both complete full optimizer audits. Evaluation summed native worker time 851.12 seconds across 42 jobs, four workers. These are process wall seconds, not CPU-seconds. Separate check/release builds ran concurrently during structured training, so arm elapsed-time differences are not a throughput experiment. No paid compute was allocated.

Independent Fable review remains unavailable: September 19 fresh request failed HTTP429 with zero source reads, reset September22 07:00 EDT. This is an unresolved review gap, not endorsement. The maintainer explicitly assigned bounded execution; preserve g115 and do not promote this candidate. Next engineering work measures deterministic parallel collection before any substantial continuation.
