# Native recurrent CP7 candidate-state screen v1

## Question

Can a width-128 recurrent structured residual learn CP7's choices on states
actually visited by the promoted candidate, where the width-48 additive residual
did not? This is a representation screen. CP7 choices are supervision, not
reward, and a pass is not playing-strength evidence.

## Fixed data and model

- Corpus: 14,525 exact CP7 labels from 256 matched candidate-state pairs in
  `D:\mtg-kernel-cp7-dagger-shadow-v1-synchronized`, joined to the exact
  complete-public-history cache with SHA-256
  `98babc28617a57d3053bf178ba1d1084f943339f69d75b918402f2e4dd10d1df`.
- Whole-pair split: residues 1 and 2 modulo 4 train the model, residue 3 selects
  one of the 12 fixed epoch checkpoints by the lowest worst-seat candidate-to-
  parent NLL ratio with overall NLL as the tiebreak, and residue 0 is evaluated
  once after selection.
- Inputs: typed state, public action history, objects and graph relations, legal
  actions and references, frozen parent logits, and frozen parent value.
- Model: the existing width-128 two-layer-GRU structured actor-critic. Its policy
  head is zero-initialized and interpreted as a residual added to parent logits.
  The value head is frozen and ignored.
- Trust region: every forward pass is projected so the maximum absolute legal
  action log-probability change is at most `0.49`. All labels are one-substep
  priority decisions.
- Objective: CP7 selected-index cross entropy with equal episode mass. AdamW
  `3e-4`, weight decay `1e-4`, gradient cap `5`, batch size selected by the
  bounded GPU profile, 12 epochs, seed `20260810`.

## Throughput selection

On exclusive GPU 1, profile batches 64, 128, and 256 on 64 corpus pairs. Select
the largest batch under 5 GiB peak allocation whose end-to-end training rate is
at least 95 percent of the best arm. Repeat the selected arm and require exact
loss-trace and model-state hashes before the full run.

## Gate

Advance to larger candidate-state CP7 collection only if the untouched held-out
split has:

1. at least 5 percent relative CP7 NLL improvement overall;
2. at least 3 percentage points top-1 improvement overall;
3. nonnegative NLL improvement at both candidate seats;
4. mean total variation at most `0.03` and p90 at most `0.10`, overall and at
   both seats; and
5. maximum possible one-step absolute log-probability change at most `0.50`.

The report also records the hard-projection scale distribution. This is diagnostic
only, but distinguishes representation failure from a residual that repeatedly
saturates the fixed trust region.

A failure closes this exact recurrent residual and existing-corpus recipe. A pass
authorizes larger CP7 label collection before any native port or strength gate.
Terminal win, draw, or loss remains the only reward and promotion measure.

## Result

Pending.
