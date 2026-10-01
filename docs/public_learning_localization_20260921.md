# Learning gains, deck exposure and outcome transitions

Completed read-only audit: `E:/mtg-meta-recovery-20260921/public-learning-localization-002/result.json`, report and `localization.png`. Reproducible tool: `python/tools/public_learning_localization_v1.py`. All 6,144 BO3 reconciled against every frozen matchup/seat/start cell; all 4,000 saved control training trajectories verified against their hashes, episode configurations, optimizer-before identities and update group totals. No new games, training, paid compute or altered analysis gates. Four parsing workers took 49.68 seconds. First attempt `001` stopped on a receipt-layout assumption; its source and partial outcome audit remain preserved.

| Finding | Replica 1 | Replica 2 |
|---|---:|---:|
| Gates control BO3 improvement over g115, /128 | +23 | +29 |
| Gates control game-one improvement, /128 | +13 | +27 |
| Other seven control BO3 change, /896 | -3 | -12 |
| Gates training game share | 20% | 20% |
| Gates mean update decision weight | 26.98% | 27.73% |
| Rally training game share | 11.85% | 11.85% |
| Rally mean update decision weight | 6.35% | 6.34% |

Decision weight means the mean over 200 updates of deck learner groups divided by all learner groups. The counts are 173,138 and 175,967 learner physical decisions, with 2,000 games per control. This is denominator mass before advantage magnitudes and parameter derivatives, not measured gradient magnitude or proof of harmful bias. Wildfire also has above-game-share decision weight, 14.76%/13.79%, but control BO3 changes +1/-5. Exposure alone does not explain performance.

Source evidence: `expanded_deck_training_v1/public_features.rs` collects all learner physical-decision groups, computes per-episode GAE, then normalizes advantages across the batch. `experimental_burn_net8_packed_v1/training/public_inputs/bridge.rs::update_groups` passes the full batch group count to each chunk. `training.rs::dense_group_loss_gae_v1` divides summed policy and value terms by that count. These loss/bridge files are unchanged from the frozen trainer source `5c429a3f`. Do not silently change this to inverse-length weighting: that changes the estimator, and an episodic policy gradient generally sums action contributions.

Control versus g115 changes 45 losses to wins and 25 wins to losses in R1; R2 has 57 gains and 40 losses. Among matched cases with unchanged game-one winner, net BO3 changes are only +1/-2; among cases with a changed game-one winner they are +19/+19. Gates gains therefore already appear before sideboarding. This is a descriptive decomposition, not causal mediation or a new statistical gate.

Entropy versus control is -12/-46 BO3 and -9/-24 game-one wins. Cases with the same game-one winner still contribute -8/-20 net BO3. All compared game-two mainboard hashes are identical, and same-game-one cases have identical game-two starting player. Do not pool postboard game rates: game-three participation and starting conditions depend on prior results. This does not isolate sideboard gameplay as a cause or support another entropy coefficient search.

Research implication: distinguish a change to the deck/opponent sampling schedule from a change to the gradient estimator. A balanced-schedule comparison can preserve terminal rewards, current group normalization, features and optimizer while changing only allocation. A weighting intervention would be a separate question. Neither has been selected for launch here. The next design should specify its target meta distribution, matched controls, complete independent replicas and deck retention criteria before measurement; do not choose them from partial outcomes or CP7. The existing entropy verdict remains NO-ADVANCE.

Literature checked September 21: [Schulman et al., GAE](https://arxiv.org/abs/1506.02438) addresses advantage-estimation bias/variance; [Pan et al., estimation bias](https://arxiv.org/abs/2301.08442) analyzes discounted state-distribution mismatch. These motivate specifying the estimator and objective explicitly; neither validates a correction or causal explanation for this MTG result.

Independent Fable review remains unavailable after known zero-source-read HTTP429 until September 22 07:00 EDT. No repeated retry or endorsement. This descriptive audit proceeds under Jack's research authority, with the missing critique explicit. Human/league competence and model promotion remain unproven. Kimi's interface process PID65424 was verified live at 04:35 EDT; dirty effect/trigger/label/test paths remain untouched.
