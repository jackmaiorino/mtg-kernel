# Native complete public-history falsifier v1

## Question

Does a compact sequence of both players' completed public actions remove the
structured adapter's repeated P1 value regression?

The prior candidate-only history screen failed. It omitted every CP7 action and
therefore did not test complete public history.

## Fixed data collection

- Checkpoint: exact promoted generation 384.
- Matchup: Rally mirror against deterministic XMage CP7 skill 7.
- Base seed: `1150001`.
- Episodes: `0` through `63`, paired and seat-swapped.
- Collection: emit the outcome and teacher exports from one bridge trajectory.
  The outcome export retains candidate decisions and natural terminal rewards.
  The teacher export retains CP7 decisions. Terminal summaries, final state
  hashes, pair metadata, and the union of policy steps must match exactly before
  joining.
- The smoke at base seed `1140001` is preflight only and is excluded.

The two exports are read-only views of the same bridge session. No game rule,
policy, checkpoint, action mapper, or runtime observation changes.

## Fixed history representation

- Join rows by pair, episode, candidate seat, physical decision, and substep.
- Require the candidate and CP7 physical-decision groups to be disjoint and
  their union to cover every terminal policy step and physical decision.
- For each completed physical decision, average the selected action's first 99
  explicit action features across its autoregressive substeps.
- Add a normalized one-hot histogram of the selected action references' public
  Rally card-definition IDs. This preserves whether the completed action
  involved cards such as Mountain, Great Furnace, Lightning Bolt, or Chain
  Lightning without retaining arena IDs.
- Exclude both 96-feature action digest tails.
- Append two role bits identifying whether the historical actor is the current
  target actor or the other player.
- Retain at most the 16 most recent completed physical decisions.
- Exclude the current physical decision and all future decisions.
- Reject any joined action kind whose selected information is not public after
  the physical decision completes. Require an exact approved semantic schema,
  approved card zones, and exact agreement between selected semantic cards and
  selected tensor references.

The representation is a diagnostic input only. A positive result would still
require a native actor-visible projection before it could enter a live model.

## Fixed model and comparison

Use the same raw structured adapter, whole-pair four-fold split, width 48,
optimizer, 20 epochs, batch size 32, and seed `20260802` as the previous screen.
Run two exact arms on these fresh pairs:

1. Stateless structured adapter.
2. Structured adapter plus complete two-player public history.

The parent checkpoint remains unchanged in both arms and both residual heads
start at zero. Run all independent folds concurrently within available CPU
capacity.

## Gates

The full-history arm must pass the same seven gates:

1. Policy NLL improves by at least 5 percent.
2. Neither acting-player seat has negative policy NLL improvement.
3. Policy top-1 is no worse than 0.5 percentage points below the parent.
4. Episode-balanced value MSE improves by at least 5 percent.
5. Neither candidate-seat value MSE regresses by more than 2 percent.
6. Consistent object permutation changes outputs by at most `1e-5`.
7. Removing valid action references affects at least 20 percent of eligible
   decisions by more than `1e-4`.

The decisive comparison is whether full history improves P1 value MSE over the
fresh stateless arm and clears the parent-relative 2-percent floor.

## Disposition

- Pass: implement the exact actor-visible history projection in the native
  observation contract, confirm on another fresh pair block, then consider a
  small matched live rung.
- Fail: close recurrent public-history compression at this scale. Do not tune
  history length, width, epochs, or thresholds on these folds. Move to the
  selective-search lane.

No derivative training or live candidate evaluation is authorized by this
screen.
