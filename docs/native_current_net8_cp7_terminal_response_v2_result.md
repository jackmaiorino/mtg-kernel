# Current Net8 CP7 terminal response v2 result

Status: complete, stopped because neither movement arm was eligible.

## Decision

The unique result is `STOP_CURRENT_NET8_CP7_TERMINAL_RESPONSE_V2`. Both exact
arms cleared L2, mean-TV, and p90-TV gates but exceeded the unchanged maximum
absolute selected physical-decision joint log-ratio cap. The selector returned
no arm, no package exists, and no fresh gameplay ran.

| arm | L2 | mean TV | p90 TV | max abs log ratio | result |
| --- | ---: | ---: | ---: | ---: | --- |
| Policy-only, value coefficient 0.0 | 0.235847 | 0.020038 | 0.061987 | 1.832218 | fail |
| Low-value, value coefficient 0.1 | 0.239750 | 0.020270 | 0.061355 | 1.892452 | fail |

The policy-only arm's maximum ratio was already `1.033152` after update 1,
when mean TV was only `0.009335`. After update 2, mean TV first cleared the
`0.010` floor at `0.010068`, while the maximum ratio was `1.571885`. Scalar
attenuation of this direction therefore cannot satisfy both frozen thresholds.

## Interpretation

Reducing the value coefficient did not remove the tail excursion. The
policy-only and low-value arms were nearly identical, with policy-only only
slightly lower at the tail. This falsifies the specific v1 diagnosis that the
auxiliary value loss caused unsafe movement through the shared trunk. The
deferred value-head-only arm is not warranted by this screen.

The result does not show that the terminal update lacks useful playing-strength
signal. It shows that its rare selected-probability changes are incompatible
with the current combination of a `0.010` mean-TV activity floor and `1.0`
single-decision log-ratio cap. Changing either threshold or reshaping the
objective defines a new experiment.

## Throughput and evidence

The two serial CPU arms plus selection completed in `61.714` seconds, matching
the 62-second projection. GPU 1 was idle and reserved.

- Source and selector head: `5876e22c846cfb1e8b2e387c07af1235deb4234d`.
- Manifest SHA-256:
  `21af5699e0297344874248dbb797e2bca60653552f0ed0c7fb57c6cb258c448b`.
- Policy-only report SHA-256:
  `000eeeb8b557197a2eb4adedc4c802e928b31f9ed4732dbb630cfc6e745c62e2`.
- Low-value report SHA-256:
  `3cc882acb1568ba8859e2ae9ca250d520234d569849c0725efc73b611b065998`.
- Selection report SHA-256:
  `5fda60847e0bfe253754a7f483fe482844e144150a7d47286a3e8bcb75c9b4e8`.
- Evidence root:
  `D:\mtg-kernel-current-net8-cp7-terminal-response-v2\development\screen-attempt-01`.

Native Pool3 seed `1830001`, fresh CP7 seed `1840001`, and reserved skill-8
seed `1850001` remain untouched.

## Next lane

Before choosing between a broader tail envelope and an explicit trust-region
objective, run one bounded replay of the deterministic policy-only update that
records p99 and maximum row TV, p99 physical-group log ratio, counts above
absolute log ratios 1.0 and 1.5, and the identity of the worst physical group.
This uses only the already revealed corpus and cannot create strength evidence.

## Nonclaims

- Movement diagnostics are not playing strength.
- No arm was selected or evaluated.
- CP7 skill 7 remains a training distribution for this lane.
