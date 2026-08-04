# Offline and live evidence map, 2026-08-04

## Purpose

This note connects development signals to fresh natural-terminal results across
the recent structured-policy campaign. It is descriptive reanalysis of existing
artifacts. It does not alter any result, promote a policy, or treat an offline
metric as playing-strength evidence.

For a matched game comparison, `G` means candidate win and control loss, `L`
means the reverse, and `T` means identical terminal outcome. `D = G + L` is the
number of discordant game legs. The reported `D / N` rates below are descriptive
leg-level rates. The two seat-swapped legs generated from one environment seed
form the atomic cluster for future inference and must not be treated as
independent observations.

## Candidates that reached a fresh terminal panel

| Candidate | Upstream signal | Mean policy TV | Fresh matched result | Descriptive terminal reading | Old-gate diagnosis |
| --- | --- | ---: | --- | --- | --- |
| Structured CP7 policy residual | Development CP7 NLL improved 2.98%; 3 of 4,952 top actions changed | 0.020000 | CP7, 16 seat-pair clusters: `0/0/32`; 10 legs changed trajectory length | Active on trajectories, no observed winner change | Unpowered rapid panel |
| Scaled history terminal PPO | Four-fold held-out surrogate `+1.31120e-4`, positive at both seats and all folds | 0.009013 full fit | CP7, 16 clusters: `0/0/32` | Offline terminal surrogate did not produce a winner change on this panel | Unpowered rapid panel |
| Recurrent CP7 imitation | Generalized to fresh CP7 action labels; fixed deployment scale 0.97 | Not reported in the terminal doc | CP7, 16 clusters: `0/0/32`; 15 legs across 12 clusters changed trajectories | Recurrent policy was active, but action-label generalization did not establish terminal gain | Unpowered rapid panel |
| Policy-only structured successor | Distillation parity: TV 0.007952, top-action agreement 0.996353 | 0.007952 | Pool3, 1,024 clusters: `22/18/2008`; net `+4` | `D/N = 1.953%`; descriptive net `+0.195%`; qualified only for noninferiority | Valid noninferiority result, not superiority evidence |
| Structured terminal PPO, head only | Numerical and transport gates passed | 0.001002 | Pool3, 1,024 clusters: `2/1/2045`; net `+1` | `D/N = 0.146%`; too little terminal leverage to resolve direction | `D=3` made the old `+20` gate conditionally impossible |
| Structured terminal PPO, 1/16 projection | Numerical and transport gates passed | 0.001798 | Pool3, 1,024 clusters: `5/1/2042`; net `+4` | `D/N = 0.293%`; favorable direction among only six discordances | `D=6` made the old `+20` gate conditionally impossible |
| Ten-block response oracle | Development search and two selector panels chose a nonzero policy | 0.002013 | Pool3, 1,024 clusters: `6/6/2036`; net `0` | `D/N = 0.586%`; fresh direction was exactly neutral | `D=12` made the old `+20` gate conditionally impossible |
| Bounded full-network terminal PPO | Largest safe clamp retained 5.33 times the projection's TV | 0.009589 | Pool3, 1,024 clusters: `17/16/2015`; net `+1` | `D/N = 1.611%`; 33 discordances give a near-neutral `17/33` direction split | Informative negative for this PPO direction; old `+20` gate was poorly calibrated |
| Bounded history value plus one-step selector | Fresh value MSE improved 34.33% overall, 32.29% P0, 36.38% P1 | Policy unchanged except overrides | CP7 extension, 64 clusters: `1/0/127`; 50 overrides | Strong value prediction transferred to an active selector, but only one winner changed | Too little discordance for strength; closes only this sparse one-step selector |

The mean-TV to leg-discordance ratios for the four 1,024-cluster Pool3 panels
with directly comparable movement reports were approximately 1.46, 1.63, 2.91,
and 1.68 for head-only, 1/16 projection, block oracle, and bounded full PPO.
This is a planning heuristic with substantial variation, not a mathematical
bound. Trajectory length, decision count, and action sensitivity also differ
across candidates.

## Candidates stopped before a fresh terminal panel

| Candidate | Result | Why no live panel was warranted |
| --- | --- | --- |
| On-policy structured population, 512 pairs | Overall held-out surrogate `+2.52972e-5`, P0 `-7.81148e-5`, P1 `+1.28709e-4`; mean TV 0.008422 | Both-seat sign and minimum-activity gates failed |
| Iterative structured population round 1, 2,048 pairs | Overall `-6.66872e-5`, P0 `+7.44416e-5`, P1 `-2.07816e-4`; two of four folds positive | Aggregate, P1, and fold-consistency gates failed |
| Recurrent on-policy terminal correction, 512 pairs | Held-out `-1.611e-5`, P0 `+5.152e-5`, P1 `-8.374e-5`; mean TV 0.000984 | Aggregate, P1, bootstrap, and activity gates failed |

These are useful representation or optimization results, but they do not
estimate fresh terminal strength. The repeated P1-negative pattern is a reason
to run the seat and perspective metamorphic audit before another large
campaign, not evidence by itself that a seat bug exists.

## Conclusions for the next gate

1. Offline label fit, held-out surrogate, and value MSE are mechanism evidence.
   None can substitute for fresh natural-terminal outcomes.
2. The clearest policy-direction negative is bounded full-network PPO: it
   produced enough discordances to observe a nearly even direction split.
   The head-only and 1/16 results are much less informative about direction
   because only three and six legs disagreed.
3. A blinded pilot should estimate cluster-level discordance before freezing
   maximum sample size and stopping boundaries. Mean TV may inform the prior
   range, but cannot validate feasibility alone.
4. Future inference should use each seat-swapped two-leg seed pair as one
   cluster, or model the full joint two-leg outcome. Success, harm, futility,
   maximum sample size, pilot reuse, adaptive-search multiplicity, and disjoint
   confirmation must all be explicit.
5. The strongest unused positive is the confirmed history-value model. It
   should enter an attributable comparison, such as the planned value and GAE
   factor of a 2x2 rung, rather than being bundled with an untested policy
   change.

## Primary records

- `docs/native_structured_policy_residual_live_v1.md`
- `docs/native_scaled_history_outcome_policy_v1.md`
- `docs/native_recurrent_cp7_terminal_screen_v1.md`
- `docs/native_policy_only_structured_successor_v1.md`
- `docs/native_policy_only_structured_terminal_head_only_v1.md`
- `docs/native_policy_only_structured_trust_projection_v1.md`
- `docs/native_policy_block_response_oracle_v1.md`
- `docs/native_bounded_logit_residual_v1.md`
- `docs/experiments/2026-08-04-bounded-history-value-confirmation/README.md`
- `docs/experiments/2026-08-04-bounded-history-value-search/README.md`
- `docs/native_on_policy_structured_population_v1.md`
- `docs/native_iterative_structured_population_v1.md`
- `docs/native_recurrent_terminal_onpolicy_v1.md`
