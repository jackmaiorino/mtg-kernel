# Native bilinear policy residual v1

## Question

Can a small explicit state-action interaction head correct the retained policy's nearly static action priors without retraining or changing its value model?

## Fixed design

- Parent: exact retained outcome manifest `706b3aa80ec7a3c067d458fef06bb2237320543f202fb2349c5cb885975fdbbb`.
- Corpus: exact 32-pair on-policy JSONL SHA-256 `b75677397c8461a702bdb5d0f7dfc47fe651e2cd1d4f048cc218001055a828cd`.
- Architecture: `parent_logit + state_hidden^T W action_hidden`, with one row-major `64 x 64` matrix and 4,096 trainable parameters. The parent and value output are frozen.
- Split: pairs whose index modulo 4 is zero are the fixed 8-pair holdout. The other 24 pairs are fit data.
- Objective: one analytic policy-gradient direction at the exact zero residual, using fit-only frozen-value standardized advantages with equal episode mass.
- Scale: selected without outcome data to produce mean fit-policy total variation `0.02`.
- Holdout gate: positive policy surrogate overall and separately for both physical seats, mean parent-to-candidate KL at most `0.01`, p90 total variation at most `0.06`, finite outputs, and exact parent behavior at zero.
- Passing behavior: refit once on all 32 pairs, repeat the outcome-independent movement checks, then emit weights for one fresh 16-pair CP7 gate. Rejection emits no weights and spends no CP7 games.

The leakage regression test changes every held-out return and requires the fit-weight SHA-256 and calibration scale to remain bit-identical.

## Result

Commit `76a5d01` exposed exact frozen state and action latents. Commit `c2bc676` implemented the fixed trainer, validation, report, and CLI. Focused tests passed, including exact zero behavior, malformed-shape rejection, contextual-signal recovery, and held-out leakage protection.

The formal executable SHA-256 was `da394069f975f8d51abe0949a5b1791be39b25c78da2a3c30310093a855b39e8`. It processed 2,995 substeps in 2,664 ms. The 4,766-byte report is `/mnt/d/mtg-kernel-bilinear-policy-residual-v1/report.json` with SHA-256 `3377bde49068c573e85d410dbbf00721cd80d75d92b1c027dc83aaf29d70c62f`.

The fit candidate reached the exact mean-TV target and had positive fit surrogate `+0.00406327`. It did not generalize:

- Held-out overall surrogate: `-0.00752645`.
- Held-out P0 surrogate: `-0.00400071`.
- Held-out P1 surrogate: `-0.01139370`.
- Held-out mean KL: `0.00403783`, within the cap.
- Held-out p90 total variation: `0.07197425`, above the `0.06` cap.

The branch therefore failed all three outcome-signal gates and the tail-movement gate. No full refit ran, no weights were emitted, and no CP7 games were spent.

## Disposition

Retire the unrestricted 4,096-parameter residual on this corpus. Do not tune its scale or matrix against the revealed holdout. The next candidate must materially reduce interaction capacity and use fresh games for its strength decision.
