# Native recurrent structured learner v1

## Question

Can a standalone recurrent structured actor-critic learn a stable terminal-outcome
policy direction that the width-48 additive history adapters did not find?

This is a fixed-corpus architecture screen. Natural terminal win, draw, or loss is
the only reward and return target. A pass authorizes a native port and fresh matched
strength test. It is not strength evidence by itself.

## Fixed data and model

- Corpus: exact 2,048-pair terminal-rung cache at
  `D:\mtg-kernel-policy-only-structured-terminal-rung-v1\formal\cache.pt`.
- Split: four whole-pair folds by `pair_index mod 4`.
- Inputs: typed state, objects, zones and groups, graph relations, legal actions,
  action references, and the last 32 completed public physical decisions.
- Digest preservation: centered parent action logits and parent value are explicit
  low-dimensional inputs. They are not added to the outputs. The candidate emits
  direct action logits and a bounded direct value.
- Architecture: width 128; two-layer GRU with state-conditioned temporal attention;
  two graph-message rounds; explicit reference aggregation; four-head action-to-
  object cross-attention; shared encoder with direct policy and `tanh` value heads.
- Initialization: one behavior-distillation epoch against parent probabilities and
  parent value on each fit split.
- Outcome fit: three epochs of physical-decision clipped PPO, clip `0.10`, using
  terminal return minus frozen behavior value, standardized by candidate seat with
  equal episode mass. Joint value MSE targets only the natural terminal result. A
  fixed parent-policy KL coefficient of `0.01` keeps the offline update in support.
- Optimizer: AdamW `3e-4`, weight decay `1e-4`, gradient cap 5, seed `20260804`.

## Throughput selection

Use CUDA PyTorch on exclusive headless GPU 1. On a fixed 128-pair subset, profile
batch sizes 64, 128, and 256 for 100 measured steps after 20 warmup steps. Select
the largest batch within 5 GiB peak allocated memory that reaches at least 95
percent of the best measured physical decisions per second. This selects execution
topology only. Prepacking, cache loading, and metric time are reported separately.

## Gate

All four folds must complete with exact source identity and finite tensors. Advance
only if:

1. Held-out terminal surrogate is positive overall, nonnegative at P0 and P1,
   positive in at least three folds, and its pair-bootstrap 90 percent lower bound
   is above zero.
2. Held-out mean total variation is in `[0.01, 0.05]`, p90 total variation is at
   most `0.15`, and maximum physical-decision joint log ratio is at most `0.50`.
3. Held-out terminal-value MSE improves at least 5 percent over behavior value
   overall, with no candidate-seat regression.
4. Object permutation changes logits by at most `1e-5`; reference removal changes
   at least 20 percent of eligible decisions by more than `1e-4`; and removing the
   parent-logit/value channels changes at least 20 percent of sampled decisions.
5. A repeated fixed profile run produces the same packed-input, metric, and model
   state hashes under the pinned GPU environment.

A failure closes this exact architecture and fit. Do not tune width, history length,
loss weights, epochs, or gates on the revealed folds. A pass authorizes one native
implementation and fresh terminal-outcome strength gate, not promotion or a
professional-level claim.
