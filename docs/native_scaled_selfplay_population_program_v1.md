# Native scaled self-play population program v1

Status: paper-only decision package, 2026-08-05. No compute is authorized by
this sheet.

## Decision requested

Jack later decides whether to authorize the program, its seed count, and its
GPU allocation. The regularized continuation retest is complementary and is
the prerequisite causal de-risking lane. A retest success de-risks this
launch by showing that the original 512-update collapse can be controlled. A
retest failure voids this schedule as written and routes an amended schedule
for the observed failure class. Neither outcome authorizes compute by itself.

The original macro rung remains the risk to be addressed: 98,304 episodes
ended at 46.18% against promoted(2), with P1 at 41.18% and no better retained
checkpoint. The proposed program tests population pressure and long-horizon
self-play after that failure is either stabilized or explicitly re-designed.

## Training and population contract

- Three matched training seeds, fixed before launch, use the retest's accepted
  design and, only after retest success, its selected beta and parent contract.
  The initial native environment, architecture, optimizer, episode batch, and
  terminal objective stay fixed.
- The base budget is 1,024 updates at 64 natural episodes per update. Retain
  checkpoints at updates 0, 64, 128, 256, 384, 512, 768, and 1,024. One
  predeclared extension may continue the same lineages to 2,048 updates, with
  checkpoints at 1,280, 1,536, and 2,048.
- The opponent pool has eight policy slots with four protected roles: two
  fixed anchors, two historical policies, two current policies, and two
  response exploiters. Promoted(2) is always an anchor. Historical policies
  are immutable checkpoints retained for at least 256 updates; current slots
  hold recent checkpoints from active lineages; exploiters are separately
  trained responses to the current mixture and are not alternate weights of
  the learner.
- Pool identities are bound in the manifest. Start with equal 25% role
  weights. At each completed 128-update checkpoint, a fresh development
  payoff panel applies a clipped multiplicative-weights update with fixed
  rate `eta=0.10`, role floors of 20%, and a 25% cap on any one policy.
  Refresh occurs only at the boundary, never mid-interval or after inspecting
  a later checkpoint. The archive retains displaced policies, so refresh
  changes exposure rather than erasing history.
- External CP7 policies are measurement anchors, not native training-pool
  members. No checkpoint is promoted merely for diagnostic movement, payoff,
  entropy, value loss, or utilization.

## Measurement cadence and topology

At updates 256, 384, 512, 768, and 1,024, read each lineage against
promoted(2) with fresh seat-swapped native panels and report overall and
physical-seat terminal W/L/D separately. If the extension is authorized,
repeat at 1,280, 1,536, and 2,048. Run CP7 external-anchor panels at 512,
1,024, and 2,048, subject to the separately authorized bridge and panel
contract. These are periodic development measurements only, never automatic
advancement gates.

Before the full run, use the bounded topology screen already specified for
the retest: compare one original `2/32/16` run on exclusive GPU 1 with two
simultaneous original-topology runs on GPU 0 and GPU 1. Use the two-device
topology only if it is resource-safe, produces bit-identical same-seed Stores,
and is at least 1.5x faster in aggregate. Otherwise use GPU 1 only. Formal V3
measurement remains exclusive to headless GPU 1, regardless of screen result.

## Reward, stopping, and escalation

Every nonterminal reward is zero. The terminal result is the only reward:
win `+1`, draw `0`, loss `-1`. Estimators may propagate that terminal result,
but no life, material, board, tempo, damage, engine, or other proxy reward is
allowed. Terminal W/L/D is the only strength, success, and promotion signal.
V3 reporting may use its prescribed terminal-order score, including draw
weight `0.5`, but that reporting convention never becomes a training reward.

Hard-stop on any nonfinite or incomplete Store, identity/hash mismatch,
reward-contract violation, broken seat/pair binding, or failed bit-identical
verification. Stop a lineage at the next checkpoint after two consecutive
native anchor reads cross the predeclared harm boundary overall or in either
physical seat. Do not rescue it by selecting an unplanned intermediate peak.

The 1,024-to-2,048 extension is allowed once, only if all lineages remain
finite and reproducible, no harm boundary is crossed, and the latest refresh
still satisfies the frozen population and parent-drift envelopes. If the
program is stable but has no predeclared strength signal at 1,024, this one
extension is the only escalation. If the extension also fails, close the lane
and amend the population or horizon design before new compute. If the retest
shows late KL loss, collapse not reproduced, or stable-no-strength, record
that classification and amend this sheet rather than silently changing beta,
pool weights, seeds, or checkpoints.

## V3 promotion route and nonclaims

A development endpoint can nominate a lineage but consumes no alpha. A
nomination requires a fresh, countersigned V3 candidate sheet binding the
exact Store, source, pool, seed schedule, and toolchain. Candidate 02 uses its
ledger allocation of `alpha=0.00875` for the initial gate and `alpha=0.00875`
for mandatory independent confirmation, with `max_N` sized by the actual V3
contract. Both gates must pass. Candidate slots 03 and 04 remain unassigned;
only a successful V3 candidate opens the applicable accumulation chain.

This program concerns native Rally BO1 self-play. It does not establish
human, professional, metagame-wide, multi-deck, BO3, sideboarding, or
competitive strength. CP7 anchors do not substitute for V3 formal evidence,
and a native or external development result alone does not establish a
pro-level claim. The schedule also does not establish that population
refresh, regularization, environment randomization, or any other component is
causal unless its matched retest and formal contract support that conclusion.
