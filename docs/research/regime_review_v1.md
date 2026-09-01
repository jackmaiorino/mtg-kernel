# Training-regime review v1 (literature-grounded)

Status: delivered 2026-09-01 under Jack's approval; input to cycle-5
pre-registration. Produced by four literature lanes (population and
non-transitivity; hidden-information critics; sample efficiency and
optimization; search and scale reference points), a CP7 provenance trace,
and a FLAG-B PPO/GAE forensic re-audit, synthesized and ranked by expected
effect per cost. Citations marked verified were read from the primary source
by the reviewer; the rest are noted as unverified.

## 0. The correction that reframes the program: CP7 is not a trained model

CP7 is XMage's built-in `ComputerPlayer7`: a deterministic alpha-beta search
bot (skill level 7 sets `maxDepth=7`, about 14 s think time, about 5,000
nodes) over the hand-coded heuristic evaluator `GameStateEvaluator2`,
inherited from upstream open-source XMage and predating this project. It has
no network, no learned weights, and zero training games. The program
record's guess that CP7 came from a 96-runner PBT regime is contradicted by
the source and its history; the 96 figure traces to game-simulation worker
processes in a throughput note. Consequences:

- The 46-49% versus 62-63% gap is a regime-quality gap at our own budget,
  not a compute deficit relative to CP7's training.
- The comparison class is "learned policy versus shallow heuristic search."
  A deterministic search opponent excels at tactically forced sequences
  (burn math, lethal counting), a plausible locus of our losses and a natural
  target for value-head accuracy and, potentially, test-time search on our
  side (a decision for Jack; section 6).

## 1. Ranked candidates (expected effect per cost)

1. Run the multiplicative-weights population rebalancing (built, never ran
   in cycle-3): the PSRO meta-strategy solve over the 8-slot pool.
   Verified: Lanctot et al., PSRO, NeurIPS 2017 (arXiv:1711.00832):
   independently trained self-play policies overfit to their training
   partner (34-72% proportional reward loss against novel opponents in
   their experiments); an empirical-payoff meta-strategy cuts that loss by
   roughly half. Cost near zero; already in cycle-4's CONTROL-R and
   TREATMENT-RB arms. Necessary, not sufficient.
2. Non-transitivity diagnostics over existing panel data: Nash support size
   and Nash-averaged rating alongside mean winrate, every refresh.
   Verified: Balduzzi et al., Re-evaluating Evaluation, NeurIPS 2018
   (arXiv:1806.02643); Czarnecki et al., Spinning Tops, NeurIPS 2020
   (arXiv:2004.09468). Under one engineering day; tells us whether diversity
   or per-agent bugs deserve the next cycle.
3. PFSP-style opponent sampling within the pool: weight sampling toward
   opponents the learner loses to or is uncertain against, internal winrates
   only. Verified: Vinyals et al., AlphaStar, Nature 575 (2019). Cheap; the
   weighting function's shape must be pre-registered and held fixed.
4. Training-only privileged (asymmetric) critic: feed the opponent's true
   hand and library order to the value head only, never the policy, never at
   inference. Extends the v4 cell baseline from a categorical id to the
   actual hidden state. Verified: Pinto et al. 2017 (asymmetric
   actor-critic); Suphx (Li et al. 2020) oracle guiding is the closest card
   game precedent. Tens of thousands of parameters, no inference cost, 3-5
   engineering days; follows v4 and needs the informed-POMDP unbiasedness
   check before pairing with TD targets.
5. Narrow, pre-registered GAE/PPO retest: lambda in {0.95, 1.0}, live critic
   only, K in {1, 2}, clip 0.20, FLAG-B-scale screen, gated on pooled
   seat-blind net winrate against a same-budget REINFORCE control with seat
   splits diagnostic only. Rationale in section 3. After v4 lands.
6. Seat-quota-capped, rectified-response exploiter retry: bounded game
   fraction, explicit 50/50 seat quota, and the rectified objective of
   Balduzzi et al., ICML 2019 (arXiv:1901.08106): widen margins where
   already winning, zero gradient from losing cells, instead of "attack weak
   spots," which that paper shows collapses onto a single exploit axis, a
   principled reading of our seat-collapsed exploiters. About one cycle.
7. 2x-width capacity retest, warm-started, full cycle, post-v4: the only
   settling test for capacity. Section 2.
8. Per-role, per-cell entropy logging during the v4 rollout (the
   committal-baseline premature-convergence signature; Chung et al., ICML
   2021, arXiv:2008.13773). Under one day.
9. Value-calibration monitoring for systematic underestimation once a
   stronger critic exists (Mastikhina et al., RLC 2025; abstract-level).
10. Fixed 80/20 self/historical sampling as an interim hedge only.
11. Gradient-noise-scale measurement at 64 games per update (McCandlish et
    al. 2018, arXiv:1812.06162): measurement only; changing batch size from
    a measured statistic is a gray area under the no-outcome-tuning law and
    needs a lead ruling before any change.
12. Search-as-teacher: re-read the T32768 artifacts only (backup-rule
    soundness, range awareness); no new search infrastructure. Precedents
    (ReBeL, DeepStack, Student of Games) used three to six or more orders of
    magnitude more compute; lowest priority for training-time use.

## 2. Capacity verdict

Plausibly not the binding constraint: the measured defects read as
credit-assignment and objective-design bugs, and the 85%-embedding to
15%-reasoning parameter split matches much larger systems' shapes. But the
July test (2x width, fresh-init, 512 updates, one quarter of a cycle, forced
to relearn the card embedding) is not a valid null. Settling test: 2x width
warm-started by width-expanding initialization from the current checkpoint,
one full 2,048-update cycle, after v4, gated on the same pre-registered CP7
metric as every other arm. Queue as a cycle-5 arm.

## 3. PPO/GAE re-audit verdict

FLAG-B is not a clean refutation. Its machine status was INCONCLUSIVE (0 of
48 cells qualified) and its P1-primary gate was unwinnable by any recipe that
reproduced the pre-existing, lineage-heritable seat pathology, whose
training-dynamics root cause was isolated only afterward. Two regimes hid
inside "48 of 48 regressed": low lambda (0.0-0.5), especially with the frozen
critic and K=4 reuse, collapsed both seats (P1 as low as 3.6%), a genuine
value-staleness defect; high lambda (0.95-1.0) reproduced the base recipe's
own P0-up, P1-down shape with a pooled seat-blind effect indistinguishable
from zero in 46 of 48 cells. Recommendation: candidate 5.

## 4. Pre-registered hyperparameter ladder (law-compliant PBT substitute)

Fixed before cycle-5's CP7 numbers exist, with adopt/reject rules per arm:
lambda in {0.95, 1.0} (lower values excluded by FLAG-B's obtained result);
K in {1, 2}, adopt K=2 only on a Holm-corrected pooled seat-blind win with
the P1 stratum not below control minus one SE; clip 0.20 fixed; live critic
only; PFSP weighting as one pre-specified form; 80/20 ratio fixed if used;
batch size held at 64 pending a separate pre-committed rule from the
noise-scale measurement; learning rate stays never-tuned. Each is a fixed
constant or a two-arm comparison with its rule fixed before data, never
outcome-driven evolution.

## 5. Scale gap

Published imperfect-information systems sit three to six or more orders of
magnitude above our ~131k games (~5 GPU-hours) per cycle; the closest
search-free card-game milestone (Hearthstone/LoCM, Xiao et al.) used eight
GPUs for up to 23 days, roughly 880 times one of our cycles; PBT proper
needs on the order of twenty parallel runs. This argues for cheap regime and
bug fixes over search infrastructure or brute compute, and confirms the gap
is regime quality at our budget.

## 6. Lead assessment and the question for Jack

Cycle-4 already carries candidate 1 (real rebalancing) and a coarse form of
candidate 4 (the cell baseline), so it remains the right next measurement.
Cycle-5 should be pre-registered around candidates 4, 5, 6, and 7 with the
ladder in section 4, plus the free diagnostics (2, 8, 11). The open decision
only Jack can make: given that CP7 is itself a search agent, whether a fixed
model wrapped in a bounded test-time search (our value net as the leaf
evaluator, no CP7 contact) is an admissible form for the 60% claim, or a
separate claim. It is the one lever that attacks exactly the tactical
accuracy a depth-7 search opponent punishes, and it changes the panel
protocol rather than the training recipe.

## Disagreements recorded

The provenance correction overrides the program's earlier CP7 framing. The
sample-efficiency lane took the FLAG-B closure at face value; the forensic
lane's finer reading (section 3) is adopted. The committal-baseline
mechanism (candidate 8) is likely secondary, since the seat pathology
predates any EMA baseline. Two exploiter designs were merged into candidate
6. Capacity caution and the underpowered-test finding are both retained.
