# Native iterative structured population v1

## Question

Can genuinely on-policy, additive structured updates produce a fresh native
win-rate gain against the explicit Pool3 mixture after one-batch residual PPO,
compact response oracles, and fixed search rules failed?

Natural terminal win, draw, or loss is the only reward, training signal, and
strength measure. No intermediate or hand-coded reward is used.

## Architecture and round semantics

The fixed retained parent remains the base policy. Each round adds one
immutable structured stage. A stage is the equal-logit average of four
width-48, complete-public-history residual models, each trained with one
pair-mod-4 fold held out. Inference is:

`parent logits + stage 1 average residual + ... + stage N average residual`.

Old stages never change. Round N games are sampled from the complete policy
through stage N-1, and the new zero-initialized stage is fit against those
behavior logits. This makes every round genuinely on-policy and prevents
double-counting earlier residuals.

Each member keeps the last 16 complete public physical decisions, terminal
return minus the frozen parent value, physical-decision joint-ratio PPO,
clip 0.10, five epochs, batch size 64, AdamW learning rate `3e-4`, weight decay
`1e-4`, gradient cap 5, and seed `20260802`. Four folds run concurrently with
six CPU threads each.

## Round 1

- Candidate behavior: retained parent with no structured stage.
- Opponent: exact Pool3 40/20/20/20 mixture.
- Corpus: 2,048 seat-swapped pairs, 4,096 games, base seed `1510001`.
- Collection: one persistent native scorer process.
- Cross-fit gate: aggregate and both-seat held-out terminal surrogates positive;
  at least three folds positive; mean total variation at most 0.05; p90 total
  variation at most 0.15; maximum absolute physical-decision joint log ratio
  at most 0.75; permutation delta at most `1e-5`; reference-removal response at
  least 20 percent. There is no minimum-movement gate.

If the cross-fit gate passes, package the four members as one equal-weight
stage and run a fresh native matched strength gate against the no-stage parent.
Both arms use Pool3 and base seed `1520001` for 1,024 seat-swapped pairs and
2,048 games. Pass only if paired gains are at least paired losses plus 20 and
candidate-minus-parent wins are at least -4 separately at P0 and P1.

A strength pass authorizes round 2 on fresh base seed `1530001`, followed by
the same matched gate against both round 1 and the retained parent at base seed
`1540001`. A cross-fit or strength failure retires this exact iterative stack
without tuning on revealed seeds.

## Throughput expectation and nonclaims

Prior measured throughput predicts about 12 minutes for collection, about 55
minutes for four concurrent folds, and under 15 minutes for both strength
arms. The selected topology uses about 22 of 24 requested CPU threads during
training. GPU 1 remains reserved but unused because this implementation uses
CPU PyTorch.

This is a Rally-only native development ladder. A pass is evidence that the
iterative learner improves against its explicit native population. It is not
an XMage, CP7, human, cross-deck, promotion, or pro-level claim. An independent
exploiter joins the opponent population only after the first fresh native gain.

## Round 1 result

Round 1 rejected before publication or native strength evaluation. Collection
completed 2,048 pairs and 4,096 natural games in 504.88 seconds. The exact
complete-history cache contained 332,265 policy steps and 277,573 physical
decisions. Four concurrent folds completed in 45.7 minutes with about 21
effective CPU cores and zero stderr.

The aggregate held-out terminal surrogate was `-0.0000666872`. P0 was
`+0.0000744416`, P1 was `-0.000207816`, and two of four folds were positive.
Mean total variation was `0.00348273`, p90 total variation was `0.0114342`, and
the maximum absolute physical-decision joint log ratio was `0.277150`. All
movement, permutation, reference-response, and numerical-envelope checks
passed. Aggregate sign, both-seat sign, and three-of-four fold consistency
failed.

No stage was packaged and the base-`1520001` strength gate was not run. This
retires the exact additive four-member first-stage recipe without tuning on
revealed seeds. Evidence root is
`D:\mtg-kernel-iterative-structured-population-v1\round-1`; aggregate SHA-256
is `3c96d93a695af72e83b6de1d818bdefbf4b3c11f49e0545faee8c566fc877da6`;
manifest SHA-256 is
`d9202b4cb1d652ab8ddf92b25881e90fdabe1c5d1fbfd3adf4713f9a9bc68c93`.

The strict additive native stack runtime and publisher were completed and
tested at commit `f42e151`, but remain unused by this rejected run.
