# Prepared five-deck BO3 comparison

September 12, 2026. The fresh fit and read-only diagnostics are complete at implementation `0d02f27369a27484373806233ff3045804c4815f`. The fixed recipe used seed 2026091401, 100 epochs, learning rate 0.01, value weight 0 and the original 56 grouped training examples. Its final checkpoint is `f7cd4bb24fd24fb17555a00b6a2e1ac0cd39dd5032a053584ffa8b5b58d8f76f`. No BO3 pilot or strength measurement has started.

**Question:** with the same frozen player in both seats, what does the newly fitted sideboard policy change about terminal match wins relative to keeping the deck, the hand-authored teacher, and the previous Rally/Burn head?

| Candidate-seat arm | Actual `run_batch` control | Interpretation |
| --- | --- | --- |
| No change | `{"kind":"keep"}` | Keeps the current configuration at every boundary |
| Static teacher | `static_plan_rows`, explicit table, game-3 carry enabled | Declared matchup-key privileged comparator; teacher expertise is unverified |
| Previous head | `learned`, checkpoint SHA `914eb1d89bbc10b446b06d0f6de82cb1eea82ba37fb57302ae8bf8d417dfaa71` | Historical Rally/Burn imitation baseline, without a clean five-deck holdout claim |
| Fresh grouped head | `learned`, fixed final checkpoint `f7cd4bb2...` | Fresh initialization, prepared training groups only; no selection by BO3 outcomes |

Fix the opponent's sideboarding to the same static teacher across arms; vary only the candidate seat. Learned inputs retain the actor's registered 75, own history and public opponent evidence. The static binder's true matchup key does not enter learned inputs.

## One-root engineering pilot, then a separate strength design

[Bound pilot preparation](E:/mtg-kernel-learned-sideboarding-evidence/engineering-004/bo3-prep-003/pilot-preparation.json) now contains **eight fully bound, unexecuted RunBatch configs**, 50 matches each, plus one exact first-match replay config. Batch policies are fixed, so each arm needs separate candidate-seat-0 and candidate-seat-1 configs. All use the completed head, source and immutable executable `bf9abf4f03525d61ec26c6c7b0c3274011bbd6b5bb1612d57ffbe3e330419152`, with build/toolchain receipts. The earlier prep002 templates remain unchanged.

[run_pilot.py](E:/mtg-kernel-learned-sideboarding-evidence/engineering-004/bo3-prep-003/run_pilot.py) defaults to offline checks and requires explicit `--execute` to dispatch. It runs one BelowNormal CPU process at a time, with a four-hour total wall cap including the extra replay, no GPU or paid resource, and per-batch completion/timing records. Future dispatch requires the native postprocessor to close and matching owned workers to be absent. It neither interrupts native work nor retries failures. Offline file hashes, exact arm/seed preservation and source-derived CLI shapes passed; no RunBatch call occurred and output directories do not exist.

[seed-reservation.json](E:/mtg-kernel-learned-sideboarding-evidence/engineering-004/bo3-prep-002/seed-reservation.json) reserves master root 2026091402, 100 unique match seeds shared across four arms and 600 possible physical-game environments. Exact SHA256 derivation and all inputs are included. Zero collisions were found against 29 historical roots from all six completed teacher datasets, both grouped inventories, or exact registered native exclusion/screen/confirmation/engineering environment values. The check does not certify every unregistered historical training stream. A future strength panel must exclude this complete pilot reservation.

The **400-match pilot measures engineering coverage and completed-match cost only**. One master root cannot support the later root-bootstrap inference. Do not rank or promote heads from pilot wins; preserve the fixed fit and controls. No strength gate is needed for this pilot, and no strength campaign is ready to launch.

Use Rally, Affinity, Elves, Terror and Burn: 25 ordered candidate/opponent pairs, both candidate seats, both initial choosers, four arms. The runner always chooses play. A complete master-root block has **400 matches**. Uniform matchup weights describe this five-deck development mixture, not the external metagame. Freeze the root count after cost calibration and review; split batches at the 1,024-match CLI bound.

For later strength work, derive match seeds from a separate frozen domain, fresh roots, role pair, seat and chooser; reuse each across arms. Pin the list and exclusions. Both seat samplers reset per physical game; later trajectories and choosers can diverge. Pair whole matches, never selectively occurring postboard games.

Proposed engineering limits: six physical games, 4,000 physical decisions and 40,000 policy substeps per game. Limits or failures remain unresolved on the fixed denominator, with worst/best outcome bounds. No fabricated losses, discarded matchups, replacement seeds or outcome-dependent extension. Environmental interruptions may be voided/restarted with one log line.

Report strict match wins, draws, unresolved counts and paired fresh-head differences per cell and overall. Bootstrap whole master-root blocks, sharing draws across contrasts. Before measurement, freeze the primary comparator, integer gates, bootstrap seed/count/quantiles/sidedness and multiplicity handling. These remain unset. Native mirror gates and the old W6 M20/M60 manifest do not transfer; unspecified contrasts remain descriptive.

## Concrete preparation and remaining work

[comparison-preparation.json](C:/Users/Jack/IdeaProjects/sideboarding-integration-20260912/engineering-004/bo3-prep/comparison-preparation.json) preserves the first preparation and **50 unchanged teacher rows**, all source-checked and legal. Observed game-3 training coverage remains 15/25 cells. Prep003 binds the completed head without changing these rows or prep002's reservation.

Freeze ordinary-RL R0 g2304, the existing sampler and explicit V3 revision-2 transfer across arms, with exact identities in that JSON. Search and the one-update expanded-play checkpoint stay outside this comparison; `RunBatch` cannot load the latter. Preserve original and transferred feature identities.

The pilot is prepared for root's reviewed dispatch, not executed. Match JSON has no elapsed-time field; the wrapper records batch and total elapsed time, completed matches/hour, physical games and unresolved counts separately. Batch timing is not decision latency. The first match's complete JSON bytes must reproduce exactly under a fresh replay output path; replay work is reported outside the 400-match denominator. Later strength work separately needs roots, reviewed gates/budget and a paired analyzer.

The completed fresh-head holdout diagnostic matched **8/18 configurations: 8/8 Done-only, 0/10 requiring movement**, with all outputs legal. This does not justify promotion. The holdout has three connected groups and no Burn/Rally coverage. For a later strength experiment, publish a 5 by 5 effect heatmap, paired intervals and completion/cost table. Attributing gains to opponent evidence additionally needs a reviewed permuted-evidence control. No brewing, live-play, promotion or CP7 claim follows.

Fable review remains unavailable: fresh design/evaluator session `7d54a2b3-e2fa-446b-8ef7-1fc47036386c` and result session `910caeba-1696-4138-90cd-f6cd75734857` failed weekly HTTP429 with zero source reads or substantive feedback. Independent source/data/result audits passed; the disposition is recorded in engineering-004/FIT-REVIEW.md. No Fable endorsement or formal launch is implied. This preparation does not authorize additional paid compute or settle the future strength design.
