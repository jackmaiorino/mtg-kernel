# Terminal teacher changes the broader policy substantially

The fixed100 archived development games are completely audited:15,953 actor-visible decision rows, including15,791 choice rows. Compared with unchangedg115, the correct-label update32 endpoint changes the top action on2,991/15,791 choice rows (18.94%). The changes extend beyond the synthetic spell-target training domain. This is evidence of broad policy movement, not evidence that every changed choice is worse. The candidate remains unpromoted and its previous reserved feasibility gate remains failed.

The mean Hamilton distribution total variation is0.203731;2,882 choice rows have total variation at least0.5. Across14,368 distinct physical decisions containing choices,2,920 have at least one changed top action. Flattened substeps and repeated game positions are not independent statistical trials. No confidence interval or win-rate claim follows from these counts.

| Choice menu | Top-action changes / choice rows | Mean total variation |
| --- | ---: | ---: |
| Choose target | 199/678 | 0.3221 |
| Blocker inclusion | 242/770 | 0.2646 |
| Attacker inclusion | 64/1,866 | 0.0689 |
| Choose effect color | 106/124 | 0.5490 |
| Pass / mana ability / other ability | 703/1,653 | 0.4555 |
| Pass / cast spell | 86/143 | 0.5832 |

Value predictions also move: mean absolute change0.248392 on choice rows, maximum1.617535. No value-target calibration was evaluated. Zero value-loss coefficient in the teacher experiment did not promise unchanged value predictions: shared learned parameters and continued Adam moments can change them.

## Coverage and limits

| Deck registration label | Choice rows | Top-action changes | Mean total variation |
| --- | ---: | ---: | ---: |
| Affinity | 1,523 | 301 | 0.2215 |
| Burn | 1,598 | 282 | 0.1923 |
| Elves | 2,689 | 480 | 0.1691 |
| Faeries | 2,022 | 229 | 0.1164 |
| Rally | 987 | 77 | 0.0792 |
| Terror | 1,528 | 119 | 0.0986 |
| Wildfire | 1,555 | 286 | 0.2158 |
| published-44ae71e1e126b63d | 3,889 | 1,217 | 0.3389 |

Seat0 has1,313/7,910 changed choice rows; seat1 has1,678/7,881. The corpus contains49 postboard and51 preboard games. Each deck appears in both seats, with21 to31 game exposures per registration. Exposure is uneven, and long games contribute more rows. These are the same previously selected100 natural development archives, fixed by update/episode schedule, not outcome-selected new games.

The historical learner was a structured-feature policy. Half the opponents wereg115 and halfA48. On the actually recordedg115 opponent decisions alone,820/3,537 choices change and mean total variation is0.242978. The overall comparison always scores g115 versus the teacher on identical inputs; A48 is loaded only to verify its recorded behavior. Neither historical learner actions nor A48 actions are treated as optimal labels.

The archives predate the corrected trample engine. This audit scores their saved actor-visible tensors and does not regenerate current-engine gameplay. It establishes policy movement on historical inputs, not current matchup performance, catastrophic forgetting of proven good moves, or human/league competence. No terminal outcome is exported or used for selection. The consumed24-position teacher validation set was not read.

## Verification and execution

Implementation1ea30595 adds `NaturalAudit` to `public_terminal_teacher_v1`. It loads only the fixed correct-label final checkpoint, verifies its source/state, reads pinned V4 archives, decodes the27 action-kind one-hot fields, computes production Hamilton masses, and verifies optimizer state unchanged after scoring. Logged opponent logits and values match exactly on all7,389 opponent rows:3,560g115 and3,829A48. The initial two-archive qualification is byte-identical in fresh processes, and a one-bit change to recorded opponent logits is rejected before output publication.

Attempt001 assumed every recorded opponent wasg115. The first archive passed, but the larger qualification correctly rejected A48 archives before completing. Its root and outputs remain preserved withfailure.json; no formal100-archive result was produced there. Attempt002 keeps exactly the same100 inputs and metrics and verifies each actual opponent identity. A subsequent compiler type error was fixed before native execution; failed build tools002 is preserved. No measurement gate or case was removed to make the corrected attempt pass.

The supported launcher requires a compatible, recent throughput receipt, engineering replay, unchanged helper/dependency/design/input pins and idle-owner checks before the full panel. It does not claim every legacy executable is guarded. It compared four fixed10-archive jobs on JackCPU1/4workers, HaleyCPU1/4workers and a split across both PCs. All normalized complete score outputs match exactly across placements. Existing Haley archive files were verified in place, avoiding a bulk transfer. RunPod's recent authenticated receipt returned403; no paid allocation was made. The scorer uses the nativeCPU inference path for exact archived-score replay; this is not a GPU training benchmark.

| Qualified allocation | Forecast for full100 archives, including remote setup/recovery |
| --- | ---: |
| Jack, one worker | 26.48 s |
| Jack, four workers | 10.94 s |
| Haley, one worker | 51.64 s |
| Haley, four workers | 26.57 s |
| Both PCs, four workers each | 15.45 s |

Selected Jack four workers. Actual full execution and local result publication took6.115seconds, excluding separate owner-inventory/controller checks. Qualification wall costs were10.60/4.39/21.13/11.20/6.27seconds for the five allocations, plus3.11seconds initial remote setup and separate inventories. Repeated qualifications warm input caches; these timings are not cold-storage benchmarks. Builds cost145.76seconds for tools001,27.22seconds for the failed tools002 and143.54seconds for tools003. Preparation and qualification cost more than the final audit. Final inventories show no active owned native jobs on either PC; seven idle human sessions are preserved.

Authoritative evidence: `E:/mtg-meta-recovery-20260921/public-terminal-teacher-natural-audit-002/completion.json`, `analysis.json`, `compute-choice.json`, and `engineering/completion.json`. Analysis SHA256 `6faf6553fa5e1b4eaf0b98f7a8575bef1383f48fc6534a1521f2c09188c43621`. Binary SHA256 `33d3493f46f4b78145b205e83ea7a244121817fe4621226fa037660e15ebb4c2`, build `public-terminal-teacher-natural-tools-003`. Helpers `terminal-teacher-natural-audit-v2.py`, `check-terminal-teacher-natural-audit-v2.py`, and `analyze-terminal-teacher-natural-audit-v2.py` retain their pinned completed-run identities.

## Consequence for the next experiment

The current unrestricted32-update imitation procedure cannot be described as a localized tactical repair. Before another training comparison, implement and verify a retention-aware objective that can combine certified terminal-winner supervision with a separately weighted parent-policy distribution loss on natural development inputs. Keep the two losses identifiable, retain terminal game rewards, exclude known teacher positions from blind parent imitation, and verify the soft-target gradient against a direct reference. This is a proposed engineering direction, not an already tested remedy or authorization for a broad campaign. A semantically consistent negative control and fresh frozen validation remain necessary; the old raw-index control and consumed reserved set cannot support selection.

Parent-policy agreement is a preservation constraint to measure, not a substitute for competitive play. Once a successor passes its newly declared feasibility and preservation checks, any strength claim still requires an independently designed current-engine paired match evaluation and actual human calibration. No model, human preview, reward or frozen result changes here. Fable's recorded zero-readHTTP429 remains untilSeptember22 at07:00EDT; no retry or endorsement is claimed. Independent review of the successor objective remains missing. CP7 excluded; heartbeat paused; goal active.
