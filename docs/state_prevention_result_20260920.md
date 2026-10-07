# Prevention pilot: REPLICATE, no promotion

The state-only treatment passed all predeclared BO3 gates after the complete2,616-match panel. This is a positive single-training-seed result against familiar development V3 opposition, not evidence of human or league competence. The next scientific step is an independent fresh-training-seed and fresh-evaluation-seed replication of the same intervention and gates.

## Complete results

| Endpoint | Gates-involving BO3 wins /480 | Canonical BO3 wins /392 |
|---|---:|---:|
| Untouched g115 |320|322|
| Matched control |366|322|
| State-only treatment |384|318|

No draws. All90 jobs,2,616 BO3 and5,740 natural games completed. Raw match hashes were reverified after recovery. All36 timing-prefix matches also exactly matched their corresponding full-panel outputs. Only final update199 was evaluated; no prefix outcome selected an endpoint or changed a gate.

- **Primary passed:** treatment minus control +18 wins/480, +3.75 percentage points, paired95% score interval[+1.25,+6.25]. Required at least15 net wins and a strictly positive lower bound.
- **Parent floor passed:** treatment minus g115 +64 wins/480, +13.33 points,[+10.42,+16.25].
- **Canonical BO3 retention passed:** treatment minus control -4/392, -1.02 points,[-2.55,+0.51]; versus g115 -1.02 points,[-3.32,+1.28]. Both lower bounds exceed the declared -5-point floor. Passing retention does not prove zero regression.
- **Material descriptive weakness:** canonical game-one score declined by2.81 points versus control,95%[-4.85,-0.77], and3.32 versus g115,[-5.36,-1.02]. Game one was descriptive in this design, so it does not retroactively change the gate. It remains a concern for replication and later evaluation.

Intervals use10,000 matched-seed resamples within each ordered matchup, keeping all arms and both physical seats together. They describe evaluation-seed uncertainty for these fixed endpoints and lists. They do not measure training-seed variability or meta-weighted transfer.

## What improved and what remains weak

The focal aggregate includes very easy matches against the V3 opponent playing Gates: treatment224/224, control223/224 and g115224/224. It must not be presented as an80% general Gates-deck strength estimate.

When **playing Gates against the canonical seven**, treatment won128/224, control112/224 and g11587/224. In the Gates mirror they won32/32,31/32 and9/32 respectively. Of the treatment's18 additional focal wins over control,16 came from playing Gates against other lists, one from facing Gates and one from the mirror.

The largest descriptive cell gain was Gates versus Elves,30/32 treatment versus20/32 control. Gates versus Rally remains0/32 for every endpoint, and treatment Gates versus Terror is only5/32. Cells have just32 matches each; these are descriptive development findings, not independent subgroup success claims. Broad competitive play remains unproven.

## Learning and compute

Both arms completed200 updates and2,000 natural training games from untouched g115, preserving terminal rewards and the full legacy optimizer. Final ages are legacy Adam32600/public Adam200. The control's public parameters/moments stayed zero. Treatment learned all64 coefficients and both moment arrays in each of the five color-state columns; cannot-prevent stayed zero. Every cost parameter and moment stayed zero. This proves the intended learning path operated, not the precise mechanism of the win-rate gain.

Actual training native time was872.09sec control and856.97sec treatment, running concurrently on the desktop's GPUs with10 collectors each. Dispatch plus complete audit took914.34sec. Full-batch serial/parallel/cross-host qualification cost439.23sec and reproduced504 saved learning files plus72 restart comparisons. The initial group projection739sec underestimated actual training cost; preserve this difference.

The live run exposed an output-storage bottleneck on the E: hard drive. The complete panel subsequently used D: SSD and16 CPU workers after18 measured worker/host/storage alternatives: C/D/E on the maintainer's and C on the compute host at1/4/8/16 workers, plus two simultaneous-host allocations. All648 engineering BO3 executions reproduced36 repeated arm-cases exactly across configurations. The launch guard rejected slower selection, missing serial evidence, unmeasured storage and a changed binary. No paid compute was allocated; the saved RunPod inventory returnedHTTP403.

Full-panel staging3.86sec, native wall271.60sec, recovery14.00sec, total289.46sec. Summed native process time1,793.62sec. The402.52sec projection was conservative: qualification used single-match jobs with more model-loading overhead per match than the full jobs. The tested equal two-PC splits were slower under that qualification, which does not establish that every weighted or dynamic cross-host allocation is slower.

A remaining scheduling issue is visible: six128-match Gates jobs were the longest, with the longest224.16sec. Average active-process equivalent was6.60 despite a16-worker pool. Future evaluation should qualify smaller deterministic chunks or prioritize long jobs to reduce the tail. Future training must qualify native SSD execution plus E archive/recovery before another substantial launch. Do not mutate or rerun these completed measurements to improve utilization figures.

## Evidence and next step

- Scientific manifest and full analysis: `E:/mtg-postboard-campaign-20260920/state-prevention-pilot-001/{manifest,training-audit,analysis,secondary-audit}.json`.
- Figure: `E:/mtg-postboard-campaign-20260920/state-prevention-pilot-001/analysis-summary.png` and `.svg`.
- Evaluation choices, raw receipts, guard checks and complete results: `E:/mtg-meta-recovery-20260920/state-prevention-bo3-001`.
- Native trainer SHA94a80b79979773f9e975c61e0efe232b37485bba2481e07b1c5214f91f22d318; evaluator SHAb4dfcf1eb5a609fac116e99bfb2c93b93eb211c07946f69296a0bbe4d5593d30. Python execution source73582704; original analysis was pinned before training.

Preserve this result and both endpoints. Replication should restart both arms from the same untouched g115 parent with disjoint training and evaluation seeds, retain the200x10 exposure schedule and exact gates, and report game-one changes and weak cells without changing the primary test. An independent replication is not yet launched or qualified. Later stronger-opponent and actual human tests remain necessary. Public-checkpoint human delivery, learned openings/sideboards and broader adapter coverage are not established by this panel.

Fable review remains unavailable under the documented zero-source-read HTTP429 until September22 07:00EDT. No retry or endorsement is implied. Bounded continuation proceeds under the maintainer's assignment with that independent-review gap explicit. No CP7 outcomes were used for model or experiment selection. No model was promoted.
