# Current Net8 CP7 terminal response v2

Status: draft for implementation review. No v2 arm has run.

## Question

Can reducing the auxiliary terminal-value loss preserve useful terminal-policy
movement while keeping every selected physical-decision log-ratio inside the
existing safety envelope?

The v1 update cleared the mean and p90 movement gates but stopped at maximum
absolute joint log ratio `1.640879`. Value MSE improved while policy NLL and
top-1 did not. V2 is a bounded development screen of that specific diagnosis,
not a threshold relaxation and not playing-strength evidence.

## Fixed arms

Both arms start independently from the exact retained GAE8 native state,
payload, Adam step `520`, and the exact 128-game, 4,769-decision corpus SHA-256
`fe95949e852227259efda060889c2ea707033f77b919f6100f42f5feeef754b4`.

Both use terminal return minus the frozen step-520 value, standardized
separately by candidate seat with equal episode mass; fixed step-520 `pi_old`;
physical-decision joint-ratio PPO clip `0.10`; learning rate `0.001`; four
full-batch updates; unchanged Adam continuation; and full-network gradients.
Natural terminal win, draw, or loss is the only reward.

- Arm `policy-only`: value coefficient `0.0`. Terminal result affects only the
  policy advantage. The frozen source value remains a baseline but no value
  target gradient is applied.
- Arm `low-value`: value coefficient `0.1`. The value target is the same
  natural terminal result.

A value-head-only scoped arm is deferred as a later value-quality optimization
and will be reconsidered only if a selected arm's downstream results show that
retaining value accuracy matters.

The arms run in fixed order, `policy-only` then `low-value`. Both run to
completion before selection. No fit metric, epoch, coefficient, or checkpoint
is selected from either arm.

## Movement-only selection

An arm is eligible only if every tensor is finite, Adam ends at step `524`,
parameter L2 from GAE8 is at most `0.75`, mean action TV is in
`[0.010, 0.050]`, p90 TV is at most `0.150`, and maximum absolute selected
physical-decision joint log ratio is at most `1.0`.

If neither arm is eligible, stop v2. If exactly one is eligible, select it. If
both are eligible, select the arm with higher mean TV to maximize the chance
of observable terminal discordance while remaining inside every cap. Exact
ties select `policy-only`. Selection uses movement only, never training
outcomes, NLL, top-1, value error, or fresh gameplay.

The v1 candidate is not eligible under v2 and its `1.0` cap is not changed.
Each arm writes its report before any package. Arm packages have distinct
authorities; only the selected authority may enter downstream gates.

## Ordered downstream gates

Only a selected v2 arm continues through the unchanged v1 gates:

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

## Compute

V1 took `30.786` seconds. Two serial CPU arms project to about 62 seconds plus
startup and report writing. GPU 1 remains exclusively reserved and is expected
to remain idle. This is already short enough that topology optimization would
not materially reduce wall time.

## Nonclaims

- Movement-only selection is not playing-strength selection.
- Reusing the revealed training corpus does not create fresh evidence.
- A downstream CP7 skill-7 pass would be versus-training-opponent evidence,
  not broad, human, or professional-level strength.
