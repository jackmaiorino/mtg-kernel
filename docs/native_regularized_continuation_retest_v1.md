# Native regularized continuation retest v1

Status: draft for combined design and implementation review. No retest arm has
run.

## Question

Did the original 512-update macro rung collapse because unregularized policy
sharpening against a static Pool3 allowed destructive drift, and can one fixed
KL-to-parent anchor make that same self-play-scale continuation stable?

This is a causal development experiment, not a promotion gate. It repeats the
original envrand-v2 macro recipe and changes only the policy regularizer. A
fixed resulting policy can enter formal V3 evaluation only through a later
candidate sheet and one remaining alpha-ledger slot.

## Fixed training recipe

The control semantics are the exact original macro rung:

- repaired-FULL narrow Net8 policy/value model
- promoted(2) generation 384 continual initialization with fresh Adam moments
- fixed Pool3 SHA-256
  `6c3c8ff09ab519dc9f462b41cbf898da902d230656d14e64d79fc66a19f3bc71`
  and weights `40/20/20/20`
- environment randomization v2 on every training and evaluation trajectory
- learning rate `0.001f32`, value coefficient `0.5f32`, production Adam
- 64 natural episodes per update, 512 updates per full run
- terminal win, draw, or loss as the only reward

The promoted(2) anchor is fixed for the entire run. Its checkpoint, sidecar,
and state file SHA-256 values are respectively
`4bd38cf3a9af3fb03fb04428fbc4286d4635007e848c7b9f0740122e430cbba8`,
`7511c0377edd4e8d918fa5843f89a0270a8264e5466c329f6b4ef18bbf9e76bb`,
and `a6c87366b2da9fc33923abab3c0e22d70c884cd9420477df3a475117be6beb99`.

The only candidate change is
`beta * KL(pi_parent || pi_current)`. Parent probabilities are stop-gradient,
computed from the exact learner-visible observation and legal-action set for
every candidate trajectory decision. Singleton distributions contribute zero.
The KL term uses the same complete physical-group denominator as the policy
term. It is an estimator regularizer, never a reward or strength signal.

The beta-zero branch must bypass all anchor work and reproduce the original
four-update Store tree bit for bit at revealed seed `969999` before any screen.

## Implementation boundary

Implementation starts from the original macro worktree at commit
`308842554b1cbca7ea091b154e8a33addeea995d`, not from the V4 branch. The V4
branch lacks the envrand-v2 macro run-record composition. Port only the
reviewed forward-KL arithmetic and the minimal CUDA training seam.

The frozen parent must run inference on each candidate-visited, learner-visible
observation. The trajectory's retained raw action logits are the current
behavior policy and cannot serve as parent targets. Run records and Store
semantics remain unchanged; beta and parent identities live in the experiment
manifest. Positive zero bypasses the anchor path and calls the original CUDA
train function directly.

Focused tests cover legal masks, singleton zero, exact forward-KL value,
`beta * (pi_current - pi_parent)` logit gradients, complete-group
normalization, CPU/CUDA agreement, coefficient parsing, parent identity, and
beta-zero bit identity through a complete update.

## Bounded coefficient screen

One 32-update matched screen uses development seed `1940001`. All five arms
finish before selection:

1. beta `0`
2. beta `0.01`
3. beta `0.03`
4. beta `0.1`
5. beta `0.3`

A fixed parent-generated validation read at seed `1941001` measures every arm
at updates `0`, `8`, `16`, `24`, and `32`. It reports, overall and by learner
seat, mean forward KL, mean and p90/p99 action TV, entropy, maximum action
probability, selected joint log-ratio tails, parameter L2, and finiteness.
No gameplay outcome is read during coefficient selection.

A positive-beta arm is eligible only if:

- every Store, parameter, moment, and metric is finite and complete;
- mean forward KL is at most `75%` of beta-zero at updates 16, 24, and 32;
- update-32 mean TV is at least `25%` of beta-zero and at least `0.005`;
- update-32 p99 row TV is at most `0.150`;
- maximum absolute selected physical-group log-ratio is at most `1.0` and zero
  groups exceed `1.0`;
- neither seat violates any preceding condition.

The smallest eligible beta is selected. This chooses the least intervention
that demonstrably contracts parent drift without freezing the policy. An exact
tie is impossible because beta is the ordered key. If no arm is eligible, stop
without another coefficient, threshold, optimizer, or schedule retry.

After selection, but before full training, the selected update-32 policy and
the beta-zero update-32 control each play 512 fresh seat-swapped pairs against
promoted(2) at development seed `1942001`. Require selected-minus-control
terminal-order net at least `-26` overall and at least `-18` in each selected
seat. Failure is a gross-safety stop. It cannot select a different beta.

## Throughput screen

Before the coefficient screen, run one bounded campaign-topology comparison on
revealed seed `969999`:

1. one original `2/32/16` worker/session/broker run on GPU 1;
2. two simultaneous original-topology runs, one on GPU 0 and one on GPU 1.

Each point uses eight updates and separate create-new roots. The same-seed
stores must be bit-identical across attempts and devices. Record wall time,
episodes per second, CPU, host memory, per-GPU memory and utilization, and
process count. The two-device topology is selected only if it is resource-safe,
bit-identical, and at least `1.5x` faster in aggregate. Otherwise use GPU 1
only. No third topology is tested. Formal V3 measurement, if later authorized,
remains exclusive to headless GPU 1.

## Full-horizon causal retest

Only the selected beta runs at full horizon. It uses the original training
seeds `970001`, `970002`, and `970003`, each for 512 updates and 32,768 natural
episodes. The immutable original beta-zero Stores are the matched controls.
All three candidate Stores must complete naturally at generation and Adam step
512 with exact source, pool, parent, environment, schedule, and finite-state
bindings.

Generation 512 is the only candidate endpoint. Checkpoints 64, 128, 256, and
384 are diagnostic and cannot rescue or replace a failed endpoint.

On revealed development base seed `982001`, each regularized checkpoint and
its same-training-seed beta-zero control plays 512 seat-swapped pairs against
promoted(2). The report retains per-leg terminal outcomes and computes:

- regularized-minus-control cluster scores using only terminal ordering;
- regularized and control W/L/D against promoted(2), overall and by seat;
- generation-512 versus generation-384 terminal order for each regularized
  seed, overall and by seat;
- one V3 reference confidence sequence for each fixed training seed's 512
  endpoint clusters, labeled development-only, with `ACCUMULATION`,
  `delta_worthwhile=0.003`, `delta_promote=0`, `alpha=0.05`, `c=0.5`,
  `max_N=512`, and exact per-leg Pool3 component retention. Training seeds are
  not pooled into one confidence sequence because their fixed policies may
  have different conditional means.

The retest advances to candidate nomination only if all of the following hold:

1. at least two of three per-seed V3 development reads are `SUCCESS` for
   regularized over beta-zero;
2. the remaining seed is not `HARM` and its endpoint effect is not below
   `-0.01`;
3. every regularized seed has generation-512 minus generation-384 net at least
   `-16` overall and `-12` in each seat;
4. pooled regularized P1 effect over beta-zero is nonnegative;
5. at least one generation-512 regularized policy scores at least `50%`
   against promoted(2) on the fixed development panel.

If the late-stability checks pass but the endpoint strength checks do not, the
result is `STABLE-NO-STRENGTH` and does not nominate. If seat-linked drift
persists after regularization, the previously frozen H4 reopening condition
routes a longer-horizon seat-credit retest. Any other failure stops this lane.

Among advancing generation-512 policies, highest fixed-panel score nominates
the candidate, ties broken by lower training seed. This selection panel is
development data and is excluded from all later formal schedules.

## Formal route and nonclaims

A nomination consumes no alpha. Before any formal game, write and countersign
a separate V3 candidate-02 sheet against promoted(2), assign its initial and
confirmation schedules, size `max_N` at the ledger's actual `0.00875` alpha,
and bind the fixed nominated Store. Initial and confirmation success are both
required. Candidate slots 03 and 04 remain unassigned.

On formal success, open the V3 accumulation chain on that lineage. Do not run
CP7 in this causal retest. CP7 response was the question V4 just closed.

This experiment concerns Rally BO1 under the fixed native Pool3 distribution.
Movement, KL, entropy, value loss, and utilization are diagnostics only.
Terminal win, draw, or loss is the only playing-strength and promotion signal.
No result here alone establishes human, metagame-wide, or professional-level
play.
