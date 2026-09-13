# Weighted CPU primitive for BO3 learning

The separate crate-internal `train_step_weighted_feature_transfer_v3` entry accepts canonical V6/V3 physical decision groups and one positive finite f32 weight per group. It replays exact forward-output bits, computes weighted gradients, and commits a complete Adam update only after validating the candidate parameters, moments and gauge state. Existing trainer entry points and their arithmetic do not call this path.

For a group with terminal target R, first-substep value v and supplied weight w, its objective is:

`w * [-stop_gradient(R - v) * sum(log p_selected) + lambda * (v - R)^2]`

The value gradient applies only to the first substep. The existing smooth-softmax surrogate is retained; this does not claim differentiation through the quantized Hamilton behavior sampler. Weights are final multipliers with no automatic normalization or extra group-count division. Result `policy_sum` and `value_sum` are weighted sums. Per-group diagnostic terms are unweighted and do not encode the supplied weight bits.

The future BO3 caller must validate complete physical grouping, one learner seat, actual behavior probabilities, fresh model/opponent identities, and complete-match eligibility. It must construct equal-match weights, preserve all attempted-match accounting, and handle an empty eligible batch without advancing Adam. It must persist weight bits and a distinct objective/checkpoint identity. The primitive alone does not ingest match files, change the old game-terminal objective, load a BO3 checkpoint, or implement a training loop.

The new path rejects empty batches, invalid/count-mismatched weights, noncanonical inputs, nonzero baseline-control bits, invalid state and stale replay outputs before mutation. Bounds are65,536physical groups and100,000substeps. These counts do not bound total decoded tensor payload or process memory; caller ingestion limits and execution reserves remain required. There is no GPU or parallel backward implementation in this path.

Six focused native tests passed on September13,2026: unequal-weight gradients without renormalization, central finite differences with frozen advantages and first-value-only credit, exact legacy gradient/state equality for power-of-two uniform weights, invalid/bounded inputs with no mutation, replay/late-error atomicity, and full optimizer snapshot/resumed-next-update equality. Test time was2.74seconds,39.44seconds including compilation. General reciprocal weighting is not claimed bit-identical to legacy division. No playing-strength result follows.

Independent implementation source review found no material defect in this bounded primitive. Fable's separate design consultation was unavailable at its weekly limit before reading sources; its failure and the disposition to continue reversible engineering are recorded in the Phase1 handoff. Campaign training, formal measurements and paid allocations remain paused.
