# Natural burn sampling floor and actor-loss distinction

Completed2026-09-22, source/artifact analysis only. Exact integer sampler reconstruction matches every recorded Hamilton mass on both natural witnesses. No sampler, objective, model or gate changed.

| Natural root | Face gap below largest logit | Recorded face sampling probability | Unclamped softmax probability |
| --- | ---: | ---: | ---: |
| cell25 immediate Bolt | 5.44755 | .00428125 | .00428839 |
| cell45 hand combination | 19.37348 | 3.81878e-8 | 1.30878e-9 |

`mtg-kernel/src/fast_sampler.rs` pins the Q8 gap clamp at16 and a4,097-entry Q63 exponential table. `phase1_agent_v1/trajectory.rs::hamilton_from_logits_v1` records the resulting masses. In cell45, indices0,1,6,7 are all beyond the clamp and get the same floor mass. Face is sampled29.178 times more often than unclamped softmax would predict, although its absolute probability is still tiny. Total variation between complete sampled and softmax distributions is .00017386 on cell45 and .00000713 on cell25; these include Q8 quantization, not only the clamp.

The inspected grouped actor loss in `experimental_burn_net8_packed_v1/training.rs::dense_group_loss_coefficients_v1` computes selected logit minus a stabilized, **unclamped** log-sum-exp, then multiplies by the advantage. Its caller is `chunk_backward_coefficients_v1`; this body contains no PPO probability-ratio clipping. This statement is scoped to that body and is not a claim about every historical trainer configuration.

For that mathematical NLL, the face-logit derivative per unit signed advantage is p(face)-1 if face is selected, or p(face) if another action is selected. On cell45 these are approximately-1 and1.30878e-9 respectively. The sampler floor therefore does NOT establish a hard zero-gradient region in this loss. It raises rare-action sampling compared with softmax; removing it would not by itself improve exploration of this face action. Small changes that leave a gap above16 can leave its sampled weight unchanged, despite changing the differentiable loss.

This is a scalar source-level derivative, not a measured CUDA gradient, parameter update, or historical causal diagnosis. Signed advantages, other sampled states, shared parameters and normalization determine actual updates. Kimi retains ownership of further actual credit audits. No proposal to remove the floor, alter sampling identity, or reopen entropy training follows. A benefit estimate and independent review are still missing; M1 unmet.

Evidence `E:/mtg-meta-recovery-20260921/burn-sampler-floor-001.json`; source `python/tools/audit_burn_sampler_floor_v1.py` uses the existing independent integer/bit reference `generate_fast_sampler_candidate_vectors_v1.py`. Both source audit hashes and all masses are checked in the output. No games, GPU, training or paid compute.
