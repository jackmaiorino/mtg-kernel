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

## Completed result

The amended collection completed 256 seat-swapped pairs, pairs `1..256`, with
512 natural terminals, 23,182 policy substeps, and 19,569 complete physical
decisions. Pair `0` was excluded because its second leg reproducibly reached
an unsupported CP7 mapper state. Pair `256`, which has the same fold assignment,
was selected as the replacement before its outcome was inspected. The merged
JSONL SHA-256 is
`317148bc19c6b33214181ed807d672b1a6f135cb6cbee1b5f9139667382fa9b0`.

All four folds ran concurrently with six PyTorch threads each. Total CPU use
held at approximately 100 percent during optimization. Held-out policy
surrogates were:

| Fold | Overall | Candidate P0 | Candidate P1 | Max joint log ratio |
| ---: | ---: | ---: | ---: | ---: |
| 0 | 0.00032994 | 0.00079114 | -0.00013127 | 0.57464 |
| 1 | 0.00032858 | -0.00024638 | 0.00090353 | 0.39770 |
| 2 | -0.00042583 | -0.00054408 | -0.00030758 | 0.75142 |
| 3 | -0.00054040 | 0.00051459 | -0.00159539 | 0.43759 |

The aggregate surrogate was `-0.0000769290`. Candidate P0 was positive at
`0.0001288175`, but candidate P1 was negative at `-0.0002826755`. Only two of
four folds were positive. The maximum absolute physical-decision joint log
ratio was `0.751425`, above the `0.50` cap.

Movement and representation checks passed. Aggregate mean total variation was
`0.0126463`, weighted p90 total variation was `0.0455104`, object-permutation
delta was at most `9.54e-7`, and all 1,024 sampled reference-bearing decisions
changed by more than `1e-4` when references were removed. No fold required
head downscaling.

The aggregate, both-seat, three-of-four-fold, and maximum-joint-ratio gates
failed. The movement, permutation, and reference-use gates passed. Therefore
the fixed development objective is rejected. Do not run the full-data fit or
the fresh base-seed `1300001` strength gate.

This result says the fixed parent-data terminal PPO update did not learn a
stable held-out improvement direction. It does not reject structured models,
terminal-only reward, PPO in other data regimes, or outcome learning in
general.

Primary artifacts:

- Aggregate report:
  `D:\mtg-kernel-structured-outcome-policy-v1\development-aggregate.json`,
  SHA-256
  `f4def90e574a232570148afaf55066197c2c85e3840ba2c2ccded1c4d4ababdf`.
- Combine report:
  `D:\mtg-kernel-structured-outcome-policy-v1\combine-report.json`, SHA-256
  `a7ee4d4662115b531bc4f048e00c38941e1ae67b72a190de58733db9a96f7efe`.
- Collection retry log:
  `D:\mtg-kernel-structured-outcome-policy-v1\collection-preflight-retries.log`.
- Lightweight manifest:
  `D:\mtg-kernel-structured-outcome-policy-v1\manifest.json`, SHA-256
  `87f98e1c34dc156a0ef28c5574f6869f9ea4faf03e2f561b2c0e6fb5af58f86d`.
- Fold-0 deterministic repeat:
  `D:\mtg-kernel-structured-outcome-policy-v1\deterministic-repeat.json`,
  SHA-256
  `4bf448af8c3ab27e603bd2a03687325a857da3ec6058f4588455df4923458467`.
