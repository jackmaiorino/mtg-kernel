# Fresh grouped sideboard fit and holdout result

September 12, 2026. The read-only evaluator and one fixed fresh fit are complete. The head produces legal sideboards, but this fit does not reproduce any complete held-out exchange. Existing implementation and engineering-001 through003 evidence are preserved.

| Diagnostic | Fresh training | Fresh holdout | Previous head holdout | Keep holdout |
| --- | ---: | ---: | ---: | ---: |
| Exact final 60/15 configuration and exchange | 19/56 (33.9%) | 8/18 (44.4%) | 8/18 (44.4%) | 8/18 (44.4%) |
| Movement examples with exact configuration | 3/40 (7.5%) | 0/10 | 0/10 | 0/10 |
| Done-only examples with exact configuration | 16/16 | 8/8 | 8/8 | 8/8 |
| Teacher-state action agreement | 288/500 (57.6%) | 40/122 (32.8%) | 40/122 (32.8%) | 18/122 (14.8%) |
| Legal completion | 56/56 | 18/18 | 18/18 | 18/18 |

![Holdout configuration comparison](E:/mtg-kernel-learned-sideboarding-evidence/engineering-004/holdout-comparison.png)

The head does make exchanges on movement examples. Its 158 directed card movements include56 matching target movements, against104 required movements. That partial agreement does not complete any of the ten target configurations. The previous head matches15 of104 required movements. Neither the lower cross entropy nor greater partial overlap establishes better BO3 play. Keeping the starting configuration matches all eight no-change targets, which fully explains the44.4% exact-configuration score.

The fresh head also completes only3/40 training exchanges. Thus the observed failure is already present on the training targets; a generalization-only explanation is insufficient. The available evidence does not distinguish optimization budget, representation, conflicting/weak teaching, or accumulated sequential-action errors. No extra epochs, seeds, alternative head, reweighting or holdout selection were tried. Future fitting diagnostics should use training-only criteria and independently reserved evaluation data before a new generalization claim. This result does not establish that learned sideboarding is futile or that the hand-authored teacher is strategically optimal.

## Fixed execution and evaluator

Implementation commit: `0d02f27369a27484373806233ff3045804c4815f`, in `E:/mtg-kernel-learned-sideboarding-codex` on `codex/learned-sideboarding-integration-v1`. This adds `evaluate_imitation` without changing existing training, BO3 or expanded-deck behavior. It verifies pinned dataset sources and grouped assignments, scores only the requested split, separates teacher-state actions from freely generated final configurations, records per-example failures and strata, and verifies unchanged model/checkpoint bytes.

The declared fit uses fresh seed2026091401, the prepared56 training examples/500 actions in stored order,100 epochs of existing clipped SGD, learning rate0.01 and value coefficient0. Exactly50000 policy updates completed in3.084seconds. No value labels exist and the value head remains zero. The frozen ordinary R0 g2304 play weights and card embeddings did not change. The final checkpoint was sealed before holdout evaluation; no checkpoint was selected using holdout predictions.

Training cross entropy fell from2.167939 to1.507015. The fixed evaluator reproduces the final training loss and57.6% action agreement. Holdout cross entropy is2.201479, versus8.232358 for the separately trained previous head. These are descriptive imitation diagnostics from unequal training corpora and budgets, not strength or mechanism attribution.

Fresh checkpoint: `E:/mtg-kernel-learned-sideboarding-evidence/engineering-004/fresh-fit-001/sideboard-checkpoint.json`, SHA256 `f7cd4bb24fd24fb17555a00b6a2e1ac0cd39dd5032a053584ffa8b5b58d8f76f`. It remains bound to play-weight file SHA `154a985811a0c923108f9770c6a3cc67f482c9b1f85976b30b86fed179644a17` and embedding table SHA `063c25fd053a927aba7a8265ca06b446f7bcf6a3369de249b16762cce15aedd9`. The expanded-play CPU successor was not substituted.

Seventeen focused Rust tests passed, including prior tests, source/split mismatch rejection, model immutability and an explicit case where teacher-action agreement overstates complete configuration accuracy. The optimized build passed in4m25s. The actual pinned dataset passed the new evaluator before fitting using training predictions only. All five declared CLI stages exited0, and all original data, old head, fresh head and frozen play bytes stayed unchanged through evaluation. One BelowNormal CPU process was used at a time, with no GPU, paid resource or BO3 game. Rust/Cargo1.94.1, LLVM21.1.8 and installed MSVC linker14.50.35725.0 are recorded. The actual transient linker process was not captured; its installed binary/version/hash were verified.

## Evidence limits and continuation

The holdout contains only three independent connected groups: four, eight and six examples, with fresh exact scores2/4,4/8 and2/6. The apparent sample size is not122 independent actions or18 independent matches. No confidence interval or strength gate is asserted. It covers Affinity/Elves and Terror/Elves in both directions plus Terror/Terror, with no Burn/Rally holdout. The full74-example dataset has24 Done-only records and game3 coverage in15/25 ordered cells. It mixes46 historical revision1 and28 revision2 rollout records, not a fresh revision2 recollection. All teaching is hand-authored warm start; the observed development split is not a secret final test.

The previous head's recorded training corpus has no exact or semantic teacher-target overlap with the holdout. That check does not erase developer exposure or play-model pretraining. The fresh sideboard initialization does not mean the play model is fresh or competent across all five decks.

The [BO3 comparison preparation](learned_sideboard_bo3_comparison_v1.md) keeps no-change, static-table, previous-head and this fixed fresh-head arms. Its first proposed pilot checks coverage, determinism and useful throughput, with400 matched matches; no BO3 comparison has run and no head is promoted. A strength experiment still needs a separately fixed sample size, primary comparison, integer gates and paired analysis. The static table receives a privileged matchup key, whereas the learned head receives actor-visible summaries. That comparison alone cannot establish learned use of opponent evidence.

The [production and brewing plan](learned_sideboard_production_brewing_plan_v1.md) separates the expanded CPU reference from production GPU, search and Store integration. Next concrete integration seams are a strict expanded-successor loader and explicit registered75s in BO3. Preserve the frozen native campaign and evaluate any eventual combined player under a new identity with matched references. Brewing remains primary: proposing and improving new75s, diverse opponent/checkpoint evaluation and fresh confirmation are still required. Sideboard swaps within a fixed75 do not implement brewing.

Fable's fresh design/evaluator consultation `7d54a2b3-e2fa-446b-8ef7-1fc47036386c` and result consultation `910caeba-1696-4138-90cd-f6cd75734857` both failed weekly HTTP429 before source reads or substantive feedback. Independent data and evaluator audits passed, and root proceeded under Jack's explicit bounded assignment. Result-review receipts and the independent audit are recorded in `engineering-004/FIT-REVIEW.md`; an unavailable review is never endorsement. No retry, quota reset or paid alternate review occurred. Engineering checks and imitation results do not establish playing strength.

Artifacts: [visual report](E:/mtg-kernel-learned-sideboarding-evidence/engineering-004/report.html), [full metrics](E:/mtg-kernel-learned-sideboarding-evidence/engineering-004/results-summary.json), [run manifest](E:/mtg-kernel-learned-sideboarding-evidence/engineering-004/run-manifest.json), [per-example holdout traces](E:/mtg-kernel-learned-sideboarding-evidence/engineering-004/fresh-holdout-eval-001/evaluation-examples.json), and [review](C:/Users/Jack/IdeaProjects/sideboarding-integration-20260912/engineering-004/FIT-REVIEW.md).
