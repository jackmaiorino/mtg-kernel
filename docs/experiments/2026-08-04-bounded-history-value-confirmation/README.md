# Parent-preserving bounded history-value confirmation

Status: complete, passed.

## Question

Does the broad value-prediction signal from the rejected unbounded screen survive when terminal values are bounded by construction and evaluated once on entirely fresh on-policy Pool3 games?

Natural terminal win, draw, or loss remains the only target. This is a value-model confirmation, not playing-strength evidence.

## Fixed development fit

- Train only on the now-revealed 2,048-pair cache at SHA-256 `454e4ce1b8f7413839a36c8e2731fc0cb65581ce13e593634bffa70013a6f16d`.
- Initialize from the qualified structured policy state SHA-256 `ff2abf50e8760780a9331e53aa7323cb96e3c64edb6e7d89062dbe38bf6a5cc0`.
- Keep the same width-48 complete-public-history representation and terminal MSE objective. Freeze the policy head.
- The fitted adapter is a separate value model. Its shared representation changes during fitting, so no policy logits from it may control play. Any later search must retain the qualified policy for action generation and use this state only for value queries.
- The retained parent is not itself bounded: 6.65 percent of development rows fall outside `[-1,1]`. Define the fixed legal baseline `b = clamp(p, -0.999, 0.999)` and let `r` be the learned raw residual. Predict `(b + tanh(r)) / (1 + b*tanh(r))`. This equals `b` at zero residual, stays strictly inside `[-1,1]`, and retains nonzero learning room at both boundaries. The `0.001` margin is a numerical-stability constant, not selected for fit.
- Five epochs, batch size 32 physical decisions, AdamW learning rate `3e-4`, weight decay `1e-4`, gradient cap 5, seed `20260810`, and 24 CPU threads.
- No residual scale, clamp threshold, epoch, width, or checkpoint is selected from the prior fold outcomes.

## Fresh confirmation data

- Candidate behavior: exact qualified structured successor.
- Opponent: exact Pool3 `40/20/20/20` mixture.
- Native scorer SHA-256: `c0c9b2004261c5f220f105636c09bdf38a82e43c117e8a67d5ba9d00e0297672`.
- Base seed `1690001`, 1,024 seat-swapped pairs and 2,048 natural games, collected in four parallel 256-pair persistent-scorer shards.
- The fresh corpus is never used for optimization, architecture choice, checkpoint selection, or calibration.
- Require exact complete public history, natural terminals, fixed identities, and no missing, duplicate, or substituted pair.

## Fresh gates

All must pass:

1. Episode-balanced terminal-value MSE improves by at least 10 percent over the fixed projected-parent baseline overall. Raw-parent MSE is reported separately.
2. MSE improves by at least 5 percent separately for candidate P0 and P1.
3. Every prediction is finite and in `[-1,1]`; initialization reproduces the projected parent within `1e-6` on a fixed sample.
4. Object permutation changes value by at most `1e-5`.
5. Exactly 1,024 reference-eligible fresh decisions are sampled. Removing valid action references changes value by more than `1e-4` for at least 20 percent of them. The permutation diagnostic also requires exactly 1,024 fresh decisions.

## Disposition

A pass authorizes one fresh learned-value short-horizon information-set search mechanism screen. Search still must improve paired natural terminal continuations before any policy package or strength gate. A failure closes local learned-value search at width 48 and moves to a larger recurrent end-to-end learner.

## Result

The fit completed on the fixed development cache in 3,312.3 training seconds and 3,461.1 seconds total, with zero stderr. Its state SHA-256 is `cae8e19ef825325508de351b883b2df3863dc66f0288be06ad2ccf868e3d7d7c`.

The untouched confirmation cache contained 1,024 matched pairs, 2,048 natural games, and 80,634 physical value decisions. Its SHA-256 is `44eae5bee2b5556faa6293c80a88cb8f67f90d46066ffb5115ced2daac579800`.

All gates passed. Episode-balanced MSE fell from the projected parent's `0.674345` to `0.442846`, a `34.33%` improvement. Improvement was `32.29%` for candidate P0 and `36.38%` for candidate P1. Improvement over the raw retained parent was `34.51%`, so projection alone did not create the result. Every prediction was finite and bounded from `-0.999956` to `0.999970`.

The exact 1,024-decision permutation diagnostic had maximum delta `7.15e-7`. Removing action references changed the value by more than `1e-4` for `93.95%` of the exact 1,024 eligible decisions.

This is fresh evidence that the width-48 public-history model predicts natural terminal outcomes broadly and on both seats. It is not playing-strength evidence. The pass authorizes the declared learned-value short-horizon information-set search mechanism screen.

Confirmation: `D:\mtg-kernel-bounded-onpolicy-history-value-v1\confirmation.json`, SHA-256 `716189e49c635eebdf5647e17ef4e3b3ab684c68addbc6b3c94fc3bed46f7539`.
