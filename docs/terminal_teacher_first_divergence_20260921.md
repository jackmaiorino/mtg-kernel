# Retained128 first-divergence diagnosis and optimizer carryover audit

All 256 paired game-one traces align on identical visible inputs and prior actions until the first changed choice or game end. There are zero alignment failures and zero opponent probability/action mismatches before candidate divergence. The candidate changes an action in 228 pairs; 28 game-one histories and terminals remain identical. Changes occur across all eight decks and many decision families. This supports investigating broad policy movement; it does not identify the cause of the BO3 regression.

The completed whole-match result remains retained103/256 versus g115123/256, net -20 (-7.8125 percentage points), paired95% interval [-12.50,-3.515625] points. Both frozen advancement gates failed. The original493/512 interrupted panel remains incomplete and separate. No checkpoint promotion, new match, training campaign or human-strength claim follows from this diagnostic.

| Quantity | Completed result |
| --- | ---: |
| Matched pairs / source BO3 matches | 256 / 512 |
| First changed game-one choice | 228 / 256 |
| Identical game-one histories and terminal | 28 / 256 |
| Aligned records, including both actors and opening choices | 12,380 |
| Candidate multi-action gameplay records | 6,147 |
| Pooled record-weighted prefix TV | 0.03190823 |
| Equal-pair mean prefix TV | 0.05787316 |
| Median first-divergence record index | 30, zero-based |

TV compares the actual production Q64 action masses on equal ordered menus. Both means include the first divergent decision and stop there; they are conditional on common-prefix length, not full-trajectory retention or action quality. Record index30 is not turn30. A different sampled action need not be an argmax change or a mistake. This is consumed development data with no new significance or model-selection gate.

| Candidate deck | Pairs | First divergence | Identical game one | Pooled prefix TV |
| --- | ---: | ---: | ---: | ---: |
| Affinity | 32 | 26 | 6 | 0.028933 |
| Burn | 32 | 29 | 3 | 0.026612 |
| Elves | 32 | 30 | 2 | 0.042491 |
| Faeries | 32 | 30 | 2 | 0.023422 |
| Gates | 32 | 32 | 0 | 0.044602 |
| Rally | 32 | 21 | 11 | 0.023432 |
| Terror | 32 | 30 | 2 | 0.041519 |
| Wildfire | 32 | 30 | 2 | 0.037483 |

| Physical candidate seat | Pairs | First divergence | Candidate choice records | Pooled prefix TV |
| --- | ---: | ---: | ---: | ---: |
| 0 | 128 | 116 | 3219 | 0.032646 |
| 1 | 128 | 112 | 2928 | 0.031097 |

The largest action-kind transitions are cast-to-cast41, effect-target-to-effect-target30, cast-to-pass22, blocker-inclusion-to-blocker-inclusion21, and target-to-target15. Same-kind entries can select different objects or spells. Cast-to-pass22 versus pass-to-cast1 is descriptive and conditional on being the first divergence; it is not a general estimate of pass propensity. Full selected semantic objects are preserved in pair-details.csv. Card-name interpretation was not attempted without verifying dense engine card-ID mapping.

Two of the28 control-only BO3 wins have identical game-one histories. A game-one divergence explanation cannot account for every lost match. Outcome-group summaries are associations with full BO3 results, not counterfactual action-quality labels. Physical seats are balanced, but the candidate always chooses play in game one; this panel does not balance initial play/draw. The eight-deck uniform mixture, fixed opponent, keep-seven policy and absence of learned sideboarding/search remain limitations.

Sequential actions affect later input distributions, so successful isolated teaching examples need not translate into match strength. Ross, Gordon and Bagnell's [2011 imitation-learning analysis](https://proceedings.mlr.press/v15/ross11a.html) motivates checking induced trajectories. Its additional reduction assumptions are not established for these terminal fixtures; it neither diagnoses our loss mechanism nor guarantees that applying DAgger would fix it.

The supported retained-prefix-dispatch.py launcher required a complete compatible throughput choice, exact timing-output parity, manifest/script/input pins, assigned pair IDs and fresh ownership before the full read. Twelve placements were measured: The maintainer E1/4/8/16, D4/8/16, the compute host C1/2/4 and two mixed allocations. All use the same16 timing pairs, selected without new trace metrics. E16 was added before full execution to separate storage from concurrency, preserving the earlier choice and extension receipt.

The maintainer E16 was fastest among tested eligible placements: 3.9564s for the timing workload, conservative forecast63.3026s for256 pairs. The actual full diagnostic took8.5166s. The maintainer's serial forecast113.5181s, best mixed86.2868s, best ComputeHost164.8825s. Forecasts include applicable full staging and conservatively scale repeated startup overhead; they are not confidence intervals or exhaustive allocation optimality. D staging cost8.3879s, remote staging13.3875s; these copies were used for qualification, not needed by selected E execution. Qualification and staging costs are separate from8.5166s.

Summed worker CPU99.015625s, process read counters1,332,073,618 bytes, write counters0. Maximum single-process start/end RSS sample56,270,848 bytes is not peak or aggregate memory. Process reads are not physical disk traffic. Both PCs were available at qualification. The compute host readers were bounded to4 for memory headroom. RunPod inventory returned HTTP403; availability was not verified, no paid authority was present and nothing was rented. The full read ended before two60-second idle-capacity windows. The launcher guard protects this diagnostic path, not arbitrary raw commands.

The subsequent six-second checkpoint arithmetic audit isolates a real optimizer property. Starting from g115's full Adam age32400 and moments, one hypothetical zero-gradient update changes153,492 of1,230,994 stored parameter values at the bit level. The actual first teaching update changes153,524. Similar changed-element counts do not mean similar update vectors or action probabilities, and do not establish that momentum dominates teaching.

All four value-head tensors at saved updates1,2 and128 exactly match the source's zero-gradient recurrence, including parameter bits and both moment arrays:12 tensor/end-point checks passed. At128,4,224 of4,225 value-head elements differ from the parent. The implementation has zero direct value loss in teacher CE and parent KL, but still advances warm Adam moments. This is expected optimizer behavior, consistent with [Kingma and Ba's Adam formulation](https://arxiv.org/abs/1412.6980), not a newly established trainer bug. The probe follows the repository's exact operation order rather than substituting a generic optimizer formula.

Value-head-only movement does not directly alter this search-free agent's action logits; shared representation changes can alter both logits and value outputs. The audit neither evaluates the hypothetical momentum-only policy nor attributes the20 net losses to momentum. No optimizer was reset, checkpoint modified or runnable counterfactual model exported. The next bounded engineering question is action-probability movement from a native, in-memory one-step momentum-only control on the same fixed inputs, compared with the saved actual first update. First-step results cannot explain all128 updates or serve as a promotion gate.

Evidence roots: E:/mtg-meta-recovery-20260921/retained-prefix-dispatch-001 (completion.json, analysis.json, pair-details.csv, seat-supplement.json, all timing receipts), and optimizer-carryover-audit-001.json under the same parent. The latter pins its plan, Python/NumPy implementation, four input checkpoints and three Rust source files. The original parser's six mutation checks cover seed mismatch, visible-state mismatch, stopping at the first changed action, a one-Q64-unit opponent drift, invalid masses and an identical trace. The arithmetic audit checks actual saved native value-head outputs; it does not yet prove whole-model native counterfactual parity.

Fable's independent review remains missing under the recorded zero-read HTTP429 until September22 at07:00EDT. No retry or endorsement is claimed. The read-only diagnosis proceeds under the maintainer's execution assignment, with this uncertainty explicit. Frozen outcomes, g115's full optimizer ancestry, terminal rewards and CP7 exclusion are preserved. The heartbeat remains paused. Human/league competence remains unproven.
