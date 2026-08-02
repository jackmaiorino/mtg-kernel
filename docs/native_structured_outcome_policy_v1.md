# Native structured outcome policy v1

## Question

Can the structured object/action representation learn a useful on-policy
terminal-outcome direction when trained on eight times more games than the
rejected 32-pair outcome experiments?

This is a rapid mechanism-to-strength test. It changes both representation and
sample size, so it does not isolate either cause. The terminal game result
remains the only reward.

## Fixed data collection

- Behavior policy: exact retained parent manifest
  `706b3aa80ec7a3c067d458fef06bb2237320543f202fb2349c5cb885975fdbbb`
  at Adam step 1.
- Opponent: deterministic XMage CP7 skill 7 in the Mono Red Rally mirror.
- Training block: 256 seat-swapped pairs at base seed `1200001`. Pair `0`
  deterministically hits an XMage CP7 mapper coverage error in its second leg,
  so the complete block is pairs `1..256`, episodes `2..513`. This replacement
  was selected by pair index before inspecting any replacement outcome.
- Throughput: seven independent 32-pair shards with first episodes `64`,
  `128`, `192`, `256`, `320`, `384`, and `448`, plus concurrent replacement
  shards for first episode `2` with 31 pairs and first episode `512` with one
  pair.
- Every row must bind the exact parent identity, end in a natural terminal, and
  pass the existing strict typed-tensor and physical-substep loader.

These 512 games are training data. Their raw win rate is not a strength result.

## Fixed development screen

- Representation: the same 48-wide state, object, relation, group-pooling,
  action, reference, and object-attention path used by the integrated
  policy-only candidate. The parent value remains unchanged.
- Split: four folds by whole pair, with `pair_index mod 4` held out.
- Advantage: terminal reward minus the frozen parent value. Center and scale
  advantages within each fit split and candidate seat using equal episode
  mass. For a multi-substep physical decision, use the parent value at its
  first substep and one joint-ratio advantage for the complete decision. No
  intermediate reward or hand-coded evaluation enters the target.
- Objective: physical-decision joint-ratio PPO with clip `0.10`, equal episode
  mass, 10 epochs, batch size 32 physical decisions, AdamW learning rate
  `3e-4`, weight decay `1e-4`, gradient norm cap 5, seed `20260802`.
- Initialization: zero policy residual. The first update moves the policy head;
  later clipped updates may train the structured path.
- Calibration: if necessary, scale only the final policy head down so fit mean
  policy total variation is at most `0.03`. Never amplify the trained head.

For each held-out fold, report the episode-balanced unclipped parent-data
policy surrogate overall and by candidate seat, movement, joint log ratios,
permutation invariance, and action-reference sensitivity.

Advance only if all conditions hold:

1. Aggregate held-out policy surrogate is positive.
2. Aggregate held-out policy surrogate is positive for both candidate seats.
3. At least three of four individual folds have positive held-out surrogate.
4. Mean total variation is at most `0.03`, p90 total variation is at most
   `0.10`, and maximum absolute physical-decision joint log ratio is at most
   `0.50`.
5. Object permutation changes logits by at most `1e-5`, and removing valid
   references changes at least 20 percent of eligible decisions by more than
   `1e-4`.

A failure closes this exact objective and spends no fresh strength games. Do
not tune epochs, clip, scale, or thresholds against the held-out folds.

## Strength gate after a development pass

Refit once on all 256 pairs with the fixed configuration and publish through
the existing strict structured runtime. Run candidate and retained parent
sequentially on 32 fresh seat-swapped pairs at base seed `1300001`, episodes
`0..63`.

Qualify only if paired gains satisfy `G >= L + 3`, candidate-minus-parent net
is at least `-2` separately for P0 and P1, and all transport and alignment
checks pass. A qualification pass authorizes a larger fresh confirmation. It
does not authorize promotion or establish pro-level play.

## Non-claims

This test does not establish that structured representation alone is better,
that PPO generally works, that CP7 is a professional reference, or that the
candidate generalizes beyond the Rally mirror.
