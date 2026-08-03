# Native complete-history live mechanism v1

## Question

Does the complete public action history representation that produced large,
seat-balanced held-out CP7 policy agreement and terminal-value improvements
produce a measurable live Rally strength gain over its exact retained parent?

## Fixed candidate

- Parent: retained outcome manifest
  `706b3aa80ec7a3c067d458fef06bb2237320543f202fb2349c5cb885975fdbbb`
  at Adam step 1.
- Corpus: exact complete-history cache SHA-256
  `721aeeb8389464676edf1190b4e90d74ced286104cc0fb30deb46d36ffbc8090`,
  containing 2,048 seat-swapped pairs and 4,096 naturally terminated episodes.
- Representation: typed state, objects, relations, legal actions, action
  references, and a GRU over the last 16 completed public physical decisions.
  Each history row contains the mean selected explicit-action features across
  physical substeps, actor role relative to the current actor, and selected
  public-card histogram. The current physical decision is excluded.
- Objective: CP7 action cross-entropy plus terminal win/loss value MSE. There
  are no heuristic or intermediate rewards.
- Fit: all corpus rows, width 48, 5 epochs, batch 64, AdamW `3e-4`, weight decay
  `1e-4`, gradient norm cap 5, seed `20260802`, and residual scale exactly 1.
- CPU topology: run the fixed 6, 12, 18, and 24-thread bounded throughput
  screen and select the arm with the highest measured training steps per
  second. Game outcomes cannot affect this choice.

The full fit is authorized only if the fixed fold-1 numerical adjudication
passes. Development-corpus fit metrics qualify package construction only and
are not strength evidence.

## Live protocol

After strict package loading, Python-to-Rust inference parity, and one
non-fresh protocol smoke, run the candidate and retained parent sequentially
against XMage CP7 on the same 16 fresh seat-swapped pairs at base seed
`1180001`, episodes `0..31`.

A gain is an episode the candidate wins and the retained parent loses. A loss
is the reverse. A tie has the same terminal win/loss result in both runs.

The candidate passes only if all conditions hold:

1. Paired gains satisfy `G >= L + 2` over all 32 episodes.
2. Candidate-minus-parent paired net is at least `-1` separately when the
   candidate is P0 and P1.
3. Both runs complete all 32 legs with matched seats and environment seeds.
4. There is no scorer fallback, projection, alignment mismatch, protocol
   failure, identity failure, or inference-parity failure.

A pass authorizes 32 additional fresh seat-swapped pairs at base seed
`1190001` with the unchanged package. A fail retires this fixed candidate.

## Non-claims

This test does not establish strength outside Rally mirrors, validate broader
deck generalization, authorize promotion, or establish professional-level
play. The live action selector uses policy logits directly. The learned value
head is packaged for parity and future search work but does not select actions
in this test.
