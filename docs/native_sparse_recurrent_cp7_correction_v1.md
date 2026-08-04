# Native sparse recurrent CP7 correction v1

## Question

Can the recurrent CP7 residual spend movement only where the frozen parent and
CP7 disagree, instead of saturating the trust projection on almost every state?
This is a rapid mechanism screen motivated by the dense residual result. It uses
no new reward and produces no playing-strength evidence.

## Fixed screen

- Reuse the exact 14,525-label candidate-state corpus and width-128 recurrent
  residual architecture from the dense screen.
- Fit only whole-pair residues 1 and 2 modulo 4. Residue 3 is the mechanism
  selection split. The previously revealed residue-0 split is not evaluated or
  used for any decision in this screen.
- Parent logits and value remain frozen inputs. The residual policy head is zero
  initialized. The value head is frozen and ignored.
- On rows where the parent top action differs from CP7, minimize CP7 selected-
  index cross entropy. On all rows, minimize `KL(parent || candidate)` to preserve
  the parent distribution. Both terms use equal label-bearing-episode mass.
- Screen KL coefficients `0.3`, `1`, `3`, `10`, and `30`, with eight epochs per
  arm, AdamW `3e-4`, weight decay `1e-4`, gradient cap `5`, batch 256, seed
  `20260810`, and exclusive GPU 1.
- Use a hard legal-action log-probability budget of `0.20`, selected before this
  screen from the dense result's roughly twofold movement-envelope miss. This is
  a new recipe, so no previously revealed split can qualify it.

## Mechanism gate

An arm advances only if the residue-3 selection split has at least 5 percent CP7
NLL improvement, at least 3 percentage points top-1 improvement, nonnegative NLL
improvement at both seats, mean TV at most `0.03`, p90 TV at most `0.10`, and
maximum legal-action log-probability change at most `0.50`, overall and by seat
where applicable. Among passing checkpoints, select the lowest mean-TV result.

A pass authorizes collection of a fresh disjoint candidate-state CP7 panel and
one fixed out-of-sample label gate. A failure closes this exact sparse objective.
Only a later fresh natural terminal win/loss gate could establish strength.

## Result

Pending.
