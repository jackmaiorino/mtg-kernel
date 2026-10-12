# Allocator v2 synthetic calibration

Stage 1 of collab `LANES/allocator-redesign-20261012/DESIGN.md`. Produced by
`tools/spy_reset/allocator2_calibrate.py --out tools/spy_reset/calibration`; full numbers in
`CALIBRATION.json`. Synthetic rows never enter Magic training.

**Method.** Collection uses the Python mirror of the Rust consultation (dynamic q, clipping,
coalescing, `mu = 0.8 pi + 0.2 q`, cumulative pick). Updates call the real `allocator2.update`
(row checks, frozen advantages, PPO epochs, gradient cap, Adam, KL stop) with 18 games per
parent update; one development cycle is 9 updates. Normalization is frozen from 36 q-arm
synthetic games before initialization, as in development. Five seeds per configuration.

Tasks: three contexts (one-hot in three shared-feature slots, other features noise) prefer
skip, 128k and 512k respectively.
- `perfect`: one consultation per game, deterministic reward (win exactly when the preferred
  amount runs). This bounds reachable movement.
- `contextual`: ten consultations per game, `p(win) = base + effect * fraction correct`.
- `zero`: `p(win) = 0.5` whatever is chosen.

Readouts on a fixed probe set: mean TV(pi, q), and `directed`, the mean over contexts of the
gain in the preferred amount's probability over q.

## Results (one cycle, 9 updates; mean over 5 seeds)

| lr | arm | perfect directed / TV | contextual 0.6 directed / TV | zero directed / TV |
| ---: | :-: | ---: | ---: | ---: |
| 3e-4 (design) | S | +0.011 / 0.019 | +0.001 / 0.019 | +0.000 / 0.020 |
| 3e-4 (design) | H | +0.006 / 0.016 | +0.001 / 0.012 | +0.000 / 0.013 |
| 1e-3 | S | +0.045 / 0.071 | +0.003 / 0.073 | -0.002 / 0.073 |
| 1e-3 | H | +0.020 / 0.052 | +0.005 / 0.059 | +0.003 / 0.062 |
| 3e-3 (chosen) | S | +0.156 / 0.221 | +0.010 / 0.241 | -0.010 / 0.243 |
| 3e-3 (chosen) | H | +0.083 / 0.187 | +0.016 / 0.245 | +0.016 / 0.237 |
| 1e-2 | S | +0.300 / 0.402 | +0.021 / 0.394 | -0.011 / 0.401 |
| 1e-2 | H | +0.103 / 0.275 | +0.034 / 0.322 | +0.007 / 0.314 |

At 3e-3 under perfect feedback, S moves P(skip | c0) from 0.625 to 0.866 and P(128k | c1)
from 0.15 to 0.35. Two cycles at 3e-3: perfect S +0.395 (c1 to 0.86), H +0.191; contextual
0.6 S +0.030, H +0.019; zero S -0.024, H -0.008, with TV 0.32 to 0.36 in every task.

Finite-difference check of the minibatch gradient (actor plus 0.5 critic, ratios away from 1,
coalesced choices, a zero-decision game in the denominator): max abs error 4.0e-10 (S) and
3.5e-10 (H) over 517 entries each.

## Findings

1. **The design learning rate fails.** At 3e-4, one cycle (108 Adam steps) barely moves the
   policy even under perfect feedback (directed +0.011 S, +0.006 H). The clip (0.2) and KL
   stop (0.02) never bind; the step size is the only limit. This repeats the v1 failure mode.
2. **Development constant: lr 3e-3.** It is the smallest grid rate where perfect-feedback
   movement is material in one cycle, while the KL stop rarely binds (S never; H in 15 of 45
   perfect updates). At 1e-2 the KL stop binds in every update. All other PPO constants
   are unchanged. `allocator2.py` keeps `DESIGN_LR = 3e-4` and records `lr` per update.
3. **Signal-to-noise limits the noisy tasks at this dose.** With ten consultations per game
   and Bernoulli outcomes, a 6-point per-decision win effect is not distinguishable from zero
   signal in one or two cycles at any tested rate. TV from q is about as large in `zero` as in
   `contextual`. Undirected movement drifts toward uniform (P(skip | c0) falls in `zero`), as
   expected from logit noise. Expect a TV change after development to measure movement, not
   learning. Read stage-2 results through directed, context-specific diagnostics against q
   and the age/budget baseline. Only strong per-decision effects can be learned from 162
   games per parent per cycle.
4. H moves less than S at equal rates: its 192 extra noise inputs dilute the hidden layer.
   Real H inputs are informative player features rather than noise, so this is a lower bound.
