# Population training and per-seat BO3

The Multi-Deck BO3 Campaign remains active. This continuation supplies distinct learner/opponent routing and resumable CPU iteration execution, preserving the measured engineering-006 player/head and all earlier native/sideboard evidence. Brewing remains a primary end goal through explicit registered and selected lists.

## Implementation and review

`ExpandedEpisodeV1.opponent` optionally names a separately pinned model source. Explicit opponents produce trajectory schema v2 with ordered per-seat sources and actual model/state identities. Each physical actor consumes its own RNG stream. Update replays both players against their actual models, but only learner decisions enter the unchanged terminal-return policy/value update. Omitting the opponent retains the historical self-play trajectory shape.

`native_expanded_training_run_v1` binds a complete iteration schedule and named opponent roster. Assignments may select the current learner, initial learner, fixed archive member, or an already completed iteration. Every update requires fresh trajectories for the exact learner state. The CPU coordinator publishes immutable collection, update, checkpoint and iteration records, validates a contiguous completed prefix on resume, and uses an OS file lock for single-writer ownership. A bounded invocation can stop after a chosen number of new completed iterations without changing the schedule.

A checkpoint without its completed update receipt is never adopted as the next learner. Recovery reuses a completed collection and retries from the same predecessor in a fresh attempt. Completed updates require exact source, trajectory, full-state and one-step Adam continuity. Orphan staging files are retained while receipt publication uses fresh staging names. Independent Codex review identified the staging restart failure and a public BO3 V3-identity gap; both were corrected. The [source review](E:/mtg-kernel-learned-sideboarding-evidence/engineering-007/SOURCE-REVIEW.md) records dispositions.

`run_population_batch` loads two strict V3 model sources and binds each sideboard head to its own physical seat's actual weights and embedding table. The router passes only the acting seat's public input. Population results record both models explicitly; no synthetic pair digest is labeled as a single model's weight hash. Existing CLI modes and legacy match serialization remain unchanged.

## Verification

Forty focused tests passed: 19 library tests, 14 CLI/evaluator tests, and seven live-registration configuration tests. These cover actor/RNG routing, wrong model/head bindings, replayed opponent outputs, learner-only gradients, stale learner rejection, schedule references, exact legacy serialization, OS lock lifetime and stale staging recovery. Some tensor fixtures use synthetic terminal containers to test grouping and gradients; these are not played-game results.

The release build at clean source `1e58c55e98b39f88aabe161d1351b52ce5eae346` completed in 171.751 seconds. [CPU runtime verification](E:/mtg-kernel-learned-sideboarding-evidence/engineering-007/resume-verification-001/verification.json) completed two logical updates from four natural game episodes across Affinity/Elves and Terror/Rally. Adam progressed from step 1 to 2 to 3. Distinct actual opponent parameters were exercised with both learner seats. Iteration zero used the original warm-start player as a fixed opponent; iteration one used the initial successor and completed iteration zero. In this two-iteration check, the latter was also the current learner, so that assignment exercised history resolution rather than an older distinct opponent.

The run paused after its first completed iteration. A separately retained reference update then supplied the bytes for a simulated checkpoint-publication interruption: a complete checkpoint existed but its update receipt did not. Resume retained the orphan, reused the completed collection, and reran from the same predecessor in a new attempt. The recovered checkpoint was byte-identical to the reference, with Adam step 3 and full-state SHA `55ddd533de0e03d7da55e3f5c21aff825b44604ea22123b5cdfc414b12c74258`. Reinvoking the completed run was idempotent; changing its learning rate rejected without changing completion. This tests the constructed artifact state, not an actual killed process or general filesystem durability. The extra reference update is outside the two logical updates. The [independent result audit](E:/mtg-kernel-learned-sideboarding-evidence/engineering-007/RESUME-RESULT-REVIEW.md) recomputed checkpoint and trajectory identities and found no evidence blocker.

| Live BO3 check | Matches | Games | Result |
| --- | ---: | ---: | --- |
| Distinct original/successor players, Keep, swapped seats | 2 | 4 | Ordered actual identities and registered 75s preserved |
| Distinct players with their own learned heads, swapped seats | 2 | 4 | Both weight/embedding bindings and legal exchanges verified |
| Final coordinator checkpoint versus archive, Keep | 1 | 2 | Adam-3 checkpoint entered actual BO3 |
| Exact population replay | 1 | 2 | Full match bytes identical |
| Existing engineering-006 expanded match replay | 1 | 2 | Full match bytes identical under the new binary |
| Wrong head on each physical seat | 0 | 0 | Both rejected before output creation or games |

The [BO3 verification](E:/mtg-kernel-learned-sideboarding-evidence/engineering-007/population-bo3-verification-001/verification.json) completed in 7.955 seconds, with 14 total games including the four replay games. The five positive population matches are engineering cases, not independent strength observations or a head comparison. Both final registrations remain the changed Affinity 75 and Elves. The original and engineering-006 heads were retained unchanged; the final coordinator player used Keep because its updated weights/embeddings require a separately compatible head. All observed matches ended naturally in two games, so this check adds no game-three coverage.

Final checkpoint SHA is `a74d73e68782a9f841dabda9cba56f02d21076887d870106ae8daf30937b38fb`; actual weights are `6ef35c01075c2ce7bac87c259b05c0917ddc88ba1f920a83794b26bbf75ae5e0` and embeddings `bed09f1af5063439432e1b2e7f0863adb3b95495f50db8b16dafc845737e1934`. These identify the measured engineering successor, not a promoted model. See [result](E:/mtg-kernel-learned-sideboarding-evidence/engineering-007/RESULT.md) for the separate evidence roots and remaining scope.

No large training campaign, new GPU or paid allocation, win-rate gate, model promotion or strength claim follows from these checks. Future breadth/learning comparisons need a separate fixed design with matched roots and independent confirmation. Human meta rates remain an external diagnostic, not a training target; CP7 outcomes remain excluded from selection.

Fresh Fable review `9583d514-4f80-4ddd-9619-815c934b3831` failed weekly HTTP429 with zero source reads or substantive tokens. Root continued already assigned CPU engineering work with independent Codex review and focused tests, recording the unavailable Fable consultation rather than claiming endorsement. No quota reset or purchase occurred.
