# g115 postboard continuation

September 20, 2026. Jack assigned research execution ownership. This lane is local, bounded development work in `E:/mtg-kernel-postboard-codex`, based on the actual GAE producer source `5239e656`. The human/search worktree and all older measurements remain separate. No paid allocation or broad campaign was launched. Fable review remains unavailable: September 19 session `876765fa-0834-4f03-8db1-6b1f99300885` returned HTTP429 before reading source, with a September 22 07:00 EDT reset. Proceed under Jack's authority; this is a recorded review gap, not endorsement. CP7 outcomes were not read or used for selection.

## Question and fixed inputs

Does adding legal postboard positions improve the g115 player's match performance compared with the same amount of continued preboard training? Both branches start from checkpoint `88c0b997...59e8d1`, including Adam step 32400 and both moment arrays. Retain GAE gamma 1, lambda .9, entropy 0, learning rate .0001, value coefficient .5, terminal game rewards, the seven existing registrations and the 5:3:2 initial/current/fixed-a48 opponent mix. This changes exposure, not the sideboard or mulligan policy. The control and treatment share all matchup, role, opponent-assignment and physical-seed choices. Half of treatment episodes use the fixed opponent-conditioned game-two draft for both seats; all control episodes use registered mainboards. Every actual draft plan changes cards.

The existing `phase1_breadth_v1` compiler still allocates the curriculum. The adapter replaces its uniform choice of postboard configuration with the exact matchup's frozen draft. Static admission verified all 49 ordered plans against the exact registered 75s, native card IDs/names, 60/15 sizes and Full support. The draft is explicitly unratified. All seven registrations were already training inputs. The inherited reserved file is empty; a scan found 282 such files in the evidence tree, all empty. This does not establish an unseen project-wide holdout or current-meta coverage. No new registration was assigned to training.

The fair-search checkout only admits GAE checkpoints for inference. Using its ordinary trainer would silently change the loss. Instead, the qualification reuses the original g115 training executable, SHA-256 `a7cc296178407033d677081cbde47446ece6918e0584c09649cd1c043421742e`, which matches the historical launch receipt. Its embedded build HEAD is `ad1036cb`; the launch records source commit `5239e656` after the harness loss-selection fix. Both identities are preserved. The run uses local GPU 1, BelowNormal priority, 16 requested collection workers and four preparation workers. Strict natural completion replaces the historical .2 resampling tolerance for both branches, preserving matched seeds instead of replacing failures.

## Completed engineering qualification

Evidence: `E:/mtg-postboard-campaign-20260920/qualification-001/qualification-result.json` and its frozen `manifest.json`.

| Branch | Updates | Games | Postboard games | Wall time | Final Adam |
| --- | ---: | ---: | ---: | ---: | ---: |
| Preboard control | 4 | 40 | 0 | 28.65 s | 32404 |
| Mixed treatment | 4 | 40 | 20 | 28.98 s | 32404 |
| Mixed replay, stopped after update 1 then resumed | 4 | 40 | 20 | 37.09 s | 32404 |

All 120 games completed naturally. All seven learner families occur in the 40-pair schedule, though this prefix is uneven and Faeries occurs only once. All four replay parameter/Adam payloads match exactly. Ten first-batch trajectory files match byte for byte. Later 30 trajectories match in every field after rebinding only checkpoint paths and checkpoint-file hashes whose full parameter/Adam equality was established first. Those later files are not raw byte-identical: checkpoints include output-root trajectory pins. The original auditor incorrectly compared unresolved schedule episodes with resolved native opponent sources; its source and correction note are preserved. No training was repeated to repair that audit.

This passes the one-seed replay requirement and the declared 120-second per-arm cost qualification. It establishes actual GAE optimizer continuation and postboard execution, not playing strength. Checkpoints from this four-update qualification are engineering outputs and are not promoted or selected for the next experiment.

## Next bounded experiment

Prepare one matched 200-update block per branch from the original g115 endpoint, with fresh common seeds and the same fixed design above. No intermediate checkpoint selection. Before launch, qualify a frozen BO3 evaluator that actually applies these static plans and a fixed opponent outside this block's training mixture. The existing population CLI supports explicit static plan rows, whereas the newer replay-capture path currently requires Keep. Verify that evaluation uses the sampled policy with search disabled, exact canonical registrations, natural terminals, both model-seat orientations and an actual byte-identical replay. Do not silently substitute keep-sideboard evaluation for postboard evaluation.

The four-update timing suggests roughly 25 to 35 minutes for both 200-update branches together, before evaluation. This is a projection from a small cold-start qualification, not measured production throughput. Keep a 20-minute wall cap per branch initially and report an interruption without interpreting a prefix. Qualification used no paid compute.

Freeze the complete development design and integer/paired analysis gates before its strength measurement. Compare the two trained endpoints and untouched g115 under common match seeds, report the full ordered matchup and seat breakdown, and check both static-sideboard BO3 performance and preboard regression. Treat the seed pair as the paired unit; a single g115 initialization provides no between-lineage replication. No current pilot result exists. A signal here can justify a replication or curriculum revision, not promotion, human-level play or league readiness. Independent opponents, learned openings/sideboards, broader verified field coverage and actual Jack feedback remain separate requirements.

## Matched development pilot, September 20

Pilot root: E:/mtg-postboard-campaign-20260920/pilot-001. Orchestration and analysis frozen at 7f48a557; trainer source 5239e656 and evaluator build d978b486. Exact executable, model, configuration, script and schedule hashes are in manifest.json. This is a bounded local development experiment, not the formal campaign or a promotion.

Question: does adding fixed matchup-specific postboard exposure improve BO3 wins compared with the same amount of further preboard training? Two branches start from untouched g115 Adam32400, each receives 200 updates and 2,000 natural terminal games. Mixed has 1,000 postboard games, control zero. Both retain the same seven registered lists, seeds, learner seats, initial/current/a48 opponent mix, GAE gamma1/lambda.9, terminal rewards, entropy0, LR.0001 and value.5. They end at Adam32600. No intermediate endpoint selection.

Evaluation: each of untouched g115, preboard endpoint and mixed endpoint faces the independent V3 reference in 784 BO3. The 392 seed cases cover 49 ordered matchups and eight replicates, each with both physical seats; G1 play/draw balanced. Both seats receive frozen known-archetype static draft sideboard plans and Keep7. Explicit singleton-only V3 passthrough, sampled policy, no search. All 2,352 matches must complete before comparative analysis. Drawn games count half in G1 score; primary BO3 metric is wins, with draws reported separately.

Frozen development gates: mixed minus control at least 24 net BO3 wins/784 and paired percentile 95% lower bound above zero; G1 score difference lower bound above -5 points versus both control and untouched g115; mixed BO3 wins at least g115. Resample 10,000 times within each ordered matchup, retaining both seats and all arms within each seed case. This permits independent replication only. Per-matchup, seat and play/draw counts remain visible; four BO3 per arm in each such cell cannot establish individual matchup strength.

Cost caps: 20 minutes per training arm, local GPU1, BelowNormal; eight evaluation workers, 180 seconds per job, at most 3,600 summed evaluation worker seconds. Reference runs first; failure prevents training. Full reference completed 784/784 matches and 1,747 natural games, 291.21 worker seconds, 873.62 projected across three arms. No reference win rate was inspected for selection. Training began September 20 at 12:34:59 EDT, first child96032, managed by the staged Python runner. Use live process/receipt state rather than treating this PID as persistent authority.

Fable review remains a recorded gap: fresh September19 consultation failed HTTP429 with zero reads until September22 07:00 EDT; Jack explicitly authorized local continuation and instructed no repeated quota retries. Supplied heuristic sideboards, Keep7, one lineage/opponent, and seven seen lists prevent meta or human-strength claims. No paid compute or CP7 outcome selection. Existing human feedback package unchanged.
