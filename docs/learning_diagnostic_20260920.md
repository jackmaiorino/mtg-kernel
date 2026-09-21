# Learning diagnosis after the public-feature pilot

The completed public-feature pilot did not improve BO3 results: structured629/784, matched control633/784, untouchedg115645/784. Game-one retention passed. The added representation alone is not supported at this horizon. This does not identify the cause of the regression or justify another longer continuation.

## Verified implementation facts

`expanded_deck_training_v1/public_features.rs` computes terminal-reward GAE with gamma1/lambda0.9, normalizes advantages and updates once per ten-game batch. `experimental_burn_net8_packed_v1/training.rs::dense_group_loss_gae_v1` uses joint selected log-probability times advantage plus value squared error. This path has no entropy bonus, reference-policy penalty or PPO ratio clipping. `training/public_inputs.rs::apply` applies Adam to the accumulated gradients. A fixed learning rate is not an observed bound on policy movement. Absence of these terms is a property, not proof of a bug.

There is relevant local precedent: `docs/native_regularized_continuation_retest_v1_result.md` reports that beta0.1 prevented a reproduced late collapse on the prior native Rally BO1 development task. Three seeds and58,368 terminal evaluation games supported that narrow result. It does not transfer automatically to the expandedg115 network, multi-deck mixture, postboard states or this CUDA path. The grouped CUDA policy-anchor sibling in training.rs is test-only and is not active in the present public trainer.

## Literature and implications

[Rudolph et al., revised March2026](https://arxiv.org/html/2502.08938v3) compare policy-gradient and game-theoretic deep-RL approaches on five imperfect-information benchmarks. Their results support investigating regularization and update control before assuming a more complex algorithm is necessary. Their entropy study finds useful settings beyond common library defaults. These games, metrics and loss scales differ from MTG, so importing a coefficient or claiming MTG exploitability would be unjustified.

[Sokota et al.](https://arxiv.org/abs/2206.05825) motivate magnetic mirror descent as regularized learning for two-player zero-sum games. Its theory does not make a fixed forward-KL anchor in this function approximator equivalent to their algorithm. [AlphaStar](https://www.nature.com/articles/s41586-019-1724-z) provides precedent for diverse opponents and counter-strategies; its large league and human data are not evidence that a new MTG population campaign is the cheapest next step.

The working inference is to measure drift and confidence before choosing between retaining competence, encouraging exploration, or changing opponents. These are competing explanations. No new learning loss or campaign has been selected here.

## Immediate completed-data diagnostic

`python/tools/audit_public_pilot_behavior_v1.py` reads all4,000 original primary trajectories, validates their saved hashes, and aggregates fixed50-update windows by arm, opponent and pre/postboard status. It measures terminal wins, initial value error against terminal return, genuine-choice softmax entropy, top-action concentration and actual public-prevention feature exposure. Forced actions are excluded from confidence metrics. Macro substeps remain separately weighted and softmax is a proxy for the quantized execution sampler. These are training-data diagnostics, not independent win-rate estimates or causal evidence. No intermediate checkpoint is selected.

The decisive follow-up would score unchanged parent and final policies on the same fixed development decisions, measure actual KL/action disagreement, and inspect representative changed legal choices using public information. If meaningful drift is confirmed, a matched retention intervention becomes better motivated; if not, do not force the anchor hypothesis. Preserve terminal rewards and independent BO3 controls for any eventual learning experiment. Counterbalanced parallel replay timing and machine qualification precede substantial compute.

Fable's fresh September19 review failed HTTP429 with zero source reads untilSeptember22 07:00EDT. Do not retry the known quota failure or imply endorsement. This read-only diagnostic proceeds under Jack's execution assignment. Any subsequent scientific decision retains this review gap and residual uncertainty. No CP7 outcomes are used.

## Additional literature check during independent replication

[Patwa, May 2026](https://arxiv.org/html/2605.28863v1) reports a controlled Big 2 comparison using shared observation and legal-action representations. Moderate entropy regularization improved its PPO baseline; current-policy self-play beat checkpoint-pool and fixed-opponent curricula within the tested budget. The paper explicitly cautions that its tables generally use one training/evaluation seed without uncertainty across independent seeds. Evaluation uses heuristic opponents, not humans. Four-player Big 2 and its reward scale differ from this MTG task.

Our inference is limited: observation sufficiency, optimization and opponent coverage remain separable hypotheses. This supports a controlled entropy/update experiment if completed MTG evidence motivates one, not importing its coefficient or assuming a larger historical league helps. The current state-prevention replication remains frozen with no loss, reward, curriculum or gate change. Any subsequent decision must account for the complete replication, canonical retention, real human-interface coverage and the independent-review gap.
