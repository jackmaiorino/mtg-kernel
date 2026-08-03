# Native policy-only structured successor v1

## Question

Can a freshly fitted complete-public-history structured policy replace the
retained parent's policy at practical parity while the exact retained parent
value remains frozen, then serve as the initialization for one terminal-only
on-policy learning rung?

This is a new branch after the joint successor failed its frozen value gate.
No revealed fold checkpoint is reused. The structured policy becomes the
acting policy, so this is not an additive residual. The retained parent is
loaded only for its exact value output and identity binding.

## Fixed fit

- Source cache SHA-256:
  `280e34cd7f685beaf52c1cab3b41c53613a5029c063871942f48c063b6f5996f`.
- Data: all candidate-policy rows from 2,048 Rally pairs against the frozen
  Pool3 `40/20/20/20` mixture, with history reconstructed from both candidate
  and population public-action lanes.
- Architecture: width 48, last 16 complete public physical decisions,
  structured state, objects, relations, actions, references, and digest inputs.
- Initialization: fresh deterministic seed `20260804`. No joint-screen model
  state is loaded.
- Objective: teacher-to-student policy KL only. Every episode has equal total
  mass, every physical decision has equal mass within its episode, and
  autoregressive substeps share their physical decision's mass equally.
- Fit: five epochs, batch size 64 physical decisions, AdamW learning rate
  `3e-4`, weight decay `1e-4`, gradient norm cap 5, and 12 CPU threads.
- Value: exact retained parent manifest
  `706b3aa80ec7a3c067d458fef06bb2237320543f202fb2349c5cb885975fdbbb`,
  Adam step 1, unchanged and frozen.

The package publishes absolute structured logits, never parent plus residual.
It must bind the complete parameter layout, parent value identity, report,
weights, and composite model hash. Strict Python/Rust fixtures must cover empty,
short, and full history windows and both acting seats.

## Fit and transport gates

The full-data fit is a qualification check, not strength evidence. Publish only
if mean policy TV is at most `0.015`, p90 TV is at most `0.040`, and top-action
agreement is at least `0.990` overall and for both candidate seats. Python/Rust
maximum absolute logit error must be at most `3e-5`; the frozen parent value
must match bit-exactly.

## Fresh native noninferiority gate

After one protocol smoke, run the candidate and retained parent separately on
the same 1,024 fresh seat-swapped Pool3 pairs at base seed `1650001`. Join all
2,048 games by pair, episode, environment seed, and candidate seat.

The initialization qualifies only if candidate losses relative to parent are
at most candidate gains plus 20, candidate total wins are at least parent wins
minus 20, and candidate-minus-parent wins are at least `-12` separately at P0
and P1. Every terminal must be natural and all transport checks must pass.

A pass authorizes one separately frozen terminal-only on-policy rung from this
policy. It does not promote the initialization. A failure retires this exact
policy-only package without tuning against the fresh panel.

## Nonclaims

Distillation targets are initialization supervision, not reward. Natural
terminal win, draw, or loss remains the only reinforcement-learning reward and
the only promotion measure. This branch does not establish CP7 superiority,
cross-deck strength, human strength, or pro-level play.
