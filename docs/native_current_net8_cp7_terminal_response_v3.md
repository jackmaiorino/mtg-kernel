# Current Net8 CP7 terminal response v3

Status: draft for combined design and implementation review. No v3 arm has run.

## Question

Can an explicit old-policy trust region preserve observable terminal-policy
movement while removing the broad action-distribution tail that stopped v1 and
both v2 arms?

The exact v2 policy-only replay had p99 row action TV `0.287794`, maximum row
TV `0.622635`, and 8 of 4,173 physical groups above absolute selected joint log
ratio `1.0`. V3 tests a fixed forward-KL anchor. It does not relax any v2 cap.

## Fixed four-arm screen

All arms start independently from the exact retained GAE8 native state,
payload, and Adam step `520`, using exact revealed corpus SHA-256
`fe95949e852227259efda060889c2ea707033f77b919f6100f42f5feeef754b4`.

Every arm keeps the v2 policy-only recipe: terminal return minus the frozen
step-520 value, standardized separately by candidate seat with equal episode
mass; fixed step-520 `pi_old`; physical-decision joint-ratio PPO clip `0.10`;
learning rate `0.001`; four full-batch updates; value coefficient `0.0`;
unchanged Adam continuation; and full-network gradients. Natural terminal win,
draw, or loss is the only reward.

The only changed term is beta times `KL(pi_old || pi_current)`, evaluated over
every legal-action distribution from the corpus old-policy logits. The fixed
order is beta `0.3`, `1.0`, `3.0`, then `10.0`. All four arms finish before
selection. No coefficient, epoch, fit metric, checkpoint, or gameplay outcome
is adaptively selected.

## Movement-only eligibility and selection

An arm is eligible only if all quantities are finite, Adam ends at step `524`,
parameter L2 from GAE8 is at most `0.75`, mean row action TV is in
`[0.010, 0.050]`, p90 row TV is at most `0.150`, p99 row TV is at most `0.150`,
p99 physical-group absolute selected joint log ratio is at most `0.75`, maximum
absolute selected joint log ratio is at most `1.0`, and zero physical groups
are above absolute selected joint log ratio `1.0`.

Among eligible arms, highest mean TV wins. An exact tie selects the larger
beta. Every report and any eligible package must match the run manifest, exact
recipe, source, corpus, Adam step, and byte hashes. Failed arms must not publish
a package. If no arm is eligible, stop this exact trust-region lane and return
the next design decision to Jack.

## Ordered downstream gates

Only the selected arm may continue through the unchanged v1 gates:

1. One already-revealed-pair bridge repeat.
2. Candidate versus retained GAE8 on 1,024 common-receipt native Pool3
   episodes at untouched base seed `1830001`; require overall terminal-order
   net at least `-16` and each seat at least `-12`.
3. Candidate and fresh GAE8 arms versus XMage CP7 skill 7 on 128 fresh
   seat-swapped pairs, 256 natural games per arm, at untouched base seed
   `1840001`; require paired terminal-order net at least `+4`, candidate wins
   at least GAE8 wins plus `4`, and each candidate-seat net at least `-2`.

No downstream outcome is inspected before its complete panel finishes. CP7
skill 8 at base seed `1850001` remains untouched and reserved.

## Compute and nonclaims

The exact v2 replay took `30.736` seconds. Four serial CPU arms project to about
123 seconds plus anchor overhead, startup, selection, and report writing. The
expected wall time is under three minutes. GPU 1 remains exclusively reserved
and is expected to remain idle because this native trainer is CPU-bound.

Movement-only selection on a revealed corpus is mechanism evidence, not
playing-strength evidence. Terminal win, draw, or loss remains the only
playing-strength and promotion measure. A downstream CP7 skill-7 pass would be
versus-training-opponent evidence, not broad, human, or professional-level
strength.
