# Terminal teaching with parent-policy retention: no advancement

The frozen comparison completed, but **does not advance**. Both correct-label endpoints select the certified winner on all 32 fresh labelled tactical positions. The retained endpoint reduces mean policy movement against g115 from 0.158515 to 0.069594, a 56.10% reduction. However, the semantic-control endpoint fits only 16/32 of its own training labels, below the predeclared 28/32 gate. Its control comparison is inconclusive. Preserve the measured retention effect and the failed overall gate separately; neither is a whole-match strength result.

The governing frozen design is `docs/public_terminal_retention_comparison_plan_20260921.md`, SHA `e9cc69065a8fe6121d547302cc74eed43bd6b2c50516e63f76487e81de94d45d`. No settings, labels, checkpoints or gates changed after evaluation. Original g115, the three fixed 32-update endpoints, the original 32 training positions, the new 48 tactical cases and the reserved 100 archival games were evaluated. The consumed older 24-position evaluation and CP7 outcomes were excluded.

| Endpoint | Correct training targets, face / creature | Fresh certified choices, face / creature | Mean TV versus g115 on 100 games |
|---|---:|---:|---:|
| g115 | 0/16, 16/16 | 0/16, 16/16 | 0 by definition |
| Unretained, correct labels | 16/16, 16/16 | 16/16, 16/16 | 0.158515 |
| Retained, correct labels | 16/16, 16/16 | 16/16, 16/16 | 0.069594 |
| Retained, semantic control | 0/16, 0/16 | 0/16, 0/16 | 0.165528 |

The semantic row above uses the certified winning labels for comparability. Its **own deliberately incorrect training labels** were fitted on 16/32 cases, not zero. Each tactical family contains eight positions per seat. Each correct-label endpoint scores 8/8 in all four family/seat cells. The further 16 no-win controls have no optimal-action accuracy label, and the evaluator exports null accuracy for them. These are correlated synthetic fixtures, not 32 independent games.

## Retention measurement

All ten reserved jobs and 100 unique games completed: 19,751 rows, 19,551 multi-action rows, and 17,675 physical decisions containing choices. The reader independently reproduced all 9,640 recorded opponent rows exactly: 4,548 from g115 and 5,092 from the other archived opponent. Original behavior was replayed using the actual archived opponent, while all candidate and g115 scores were computed on both actors' inputs. No gameplay outcomes were exported or used.

TV uses the production Hamilton action probabilities. Multi-action rows are averaged within each physical decision, decisions within each game, then games receive equal weight. The retained/unretained ratio is **0.439036**, below the frozen 0.8 gate. The paired retained-minus-unretained mean is **-0.088921**; its 95% percentile interval is **[-0.097773, -0.081033]**. Bootstrap: ten archive update blocks with ten paired games per block, 20,000 resamples, NumPy 2.4.3 PCG64 seed 2026092101. The upper bound is below zero. These intervals concern historical input agreement, not game wins.

| Endpoint | Changed row argmax | Changed physical-decision argmax | Equal-game mean changed-decision fraction | Mean absolute value movement |
|---|---:|---:|---:|---:|
| Unretained | 3,046 / 19,551 | 2,922 / 17,675 | 0.149730 | 0.259719 |
| Retained | 1,375 / 19,551 | 1,331 / 17,675 | 0.068733 | 0.168894 |
| Semantic | 3,236 / 19,551 | 3,056 / 17,675 | 0.167048 | 0.177185 |

The physical numerator counts a decision once if any of its choice rows changes argmax. The equal-game fraction is not the pooled count ratio. Value movement is descriptive and does not measure calibration or improved value prediction.

Every deck/seat cell shows lower mean TV with retention, with only 6 to 16 game exposures per cell. The eight decks include the frozen published deck identifier `published-44ae71e1e126b63d`. There are 45 preboard and 55 postboard games. Per-cell and action-category metrics are in `retained-evaluation-001/analysis.json`. The largest residual category movement is targeting (retained TV 0.224359), followed by blocking (0.168961). Those category summaries average eligible decisions within a game, then games with that category. They are descriptive subdivisions, not additional selection gates or independent trials.

## Failed control and training-only diagnosis

`retained-control-fit-001/result.json` reads only the 32 training inputs and their already-produced final scores. The semantic label is self-targeting in the face family and opponent-targeting in the creature family. The endpoint chooses self on **all 32** cases. Thus it fits 16/16 face-family control targets and 0/16 creature-family control targets, identically across seats. Mean own-target CE is 0.506163 for the face family and 0.773266 for the creature family. All 16 actor-relative target/logit pairs match exactly, and all 32 complete tensors are distinct. This rules out the previous raw-index seat contradiction and exact full-input collisions in these cases.

It does not distinguish insufficient optimization from retention interference or difficulty learning state-conditioned player targeting. The source concatenates state and action embeddings before a nonlinear scorer (`native_policy_value_net_v1.rs:847`), so this evidence does not establish an additive-only architectural impossibility. There were no new learner updates or model inferences for this diagnosis.

**Disposition:** all other frozen gates pass; `semantic_own_fit` fails. No checkpoint is promoted, no human preview is updated, and no whole-match campaign is launched from this result. Retire this validation panel from successor selection. The next bounded question should use training/development inputs to separate the semantic learner's optimization failure from the effect of its retention penalty, with a frozen, matched diagnostic and qualified throughput. That diagnosis cannot retroactively repair this failed comparison. A successor strength claim still requires a fresh design and current-engine matched whole-match and human evidence.

## Implementation, verification and compute

Evaluator source **80002f13** adds `RetainedEvaluate` and `RetainedNatural`. The new reader validates the distinct retained checkpoint schema/loss, all three unique arms, parent/input/label/design identities, selected retention rows, exact final update 32 and Adam age 32,432, full parameter/moment state hash, and immutable optimizer state. It never relabels a retained checkpoint as an older teacher-only checkpoint. Evaluation performs no updates.

`retained-evaluation-001/engineering/completion.json` records byte-identical fresh training replay, exact g115 and unretained logits/value bits against the older independent reader, and rejection of duplicate arms, incorrect age, altered moments and mismatched labels. The fresh tactical panel also replays byte-identically. Natural-scoring qualification reproduced identical normalized outputs on both PCs and both worker counts for the same 40 previously consumed development games; no reserved game was used to choose placement.

The supported launcher is `E:/mtg-meta-recovery-20260921/retained-evaluation.py`. Its substantial `run` mode rejects missing, stale or incompatible qualification, revalidates pinned dependencies and measured results, and selects the fastest measured allocation. This is launcher enforcement, not OS enforcement; the native diagnostic accepts bounded raw engineering requests and is not itself a global launch firewall.

| Allocation | Forecast for 100 games including overhead |
|---|---:|
| Jack, one job | 43.46 s |
| **Jack, four jobs** | **18.88 s** |
| HaleysPC, one job | 90.70 s |
| HaleysPC, four jobs | 48.03 s |
| Both PCs, four jobs each | 25.98 s |

Actual selected execution and recovery took **12.92 seconds** for 100 games, followed by 0.893 seconds for the 48-position panel and 0.899 seconds for its replay. The four-job representative workload took 7.58 seconds versus 17.42 serially. Scoring and recovery use Jack's D SSD with immutable copies on E. The five representative placement runs totalled 88.26 seconds, plus 6.28 seconds of remote setup and separate inventory/orchestration overhead. The evaluator build took 154.37 seconds using four BelowNormal Cargo jobs and E target/temp storage. Both PCs' CPUs, storage, memory and GPUs were inventoried. This reader has no qualified CUDA scorer; GPU acceleration was not assumed. RunPod inventory returned HTTP403; nothing was allocated or charged. No new training occurred in this evaluation.

Both PCs had no active research native jobs after completion. The seven idle human sessions were preserved. The heartbeat remains paused. This run finished well below the two 60-second idle-capacity diagnosis threshold; occupancy is not a strength or throughput claim.

Independent Fable review remains unavailable under the recorded zero-source-read HTTP429 through September 22, 07:00 EDT. It was not repeatedly retried. This is an unresolved review gap, not endorsement. Jack's continuing research/execution authority covers this bounded work; it does not imply paid compute or broader training authority.

## Evidence and limits

Evidence root: `E:/mtg-meta-recovery-20260921/retained-evaluation-001`. Key files: `manifest.json`, `engineering/completion.json`, `compute-choice.json`, `analysis-before-launch.json`, `completion.json`, `analysis.json`, and `owners-after.json`. The analyzer was hashed before reserved results were produced. Completed training remains in `retained-campaign-002`; its binary, inputs and 96 checkpoint files were not altered. The result reader binary SHA is `e1f9d20305c86d2ae9e6c3337df97f1ffde00a135d5ed953d9f153d3536c9fab`.

Archived games predate the trample correction and are not globally unseen model ancestry. Retention means agreement with g115, not correctness. Tactical families are synthetic and share construction templates. Sideboarding, actual human performance and broad current-engine meta competitiveness remain unproven. Kimi's Escape branch remains separate/default-off and the earlier public-stack feature screen remains no-advance.
