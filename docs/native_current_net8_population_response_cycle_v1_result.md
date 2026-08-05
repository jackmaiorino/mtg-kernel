# Current Net8 population response cycle v1 result

Status: complete, stopped by the frozen development gate.

## Decision

The unique frozen decision is `STOP_POPULATION_RESPONSE_CYCLE_V1`. The
candidate passed every identity, numerical-stability, movement, and pure-GAE8
seat-validity check, but failed all three original-Pool3 advancement
requirements. No V3 candidate slot or formal alpha was consumed.

| development comparison | candidate better | control better | ties | net | required | result |
| --- | ---: | ---: | ---: | ---: | ---: | --- |
| Original Pool3, candidate vs GAE8 | 43 | 37 | 944 | +6 | +8 | fail |
| Original Pool3, candidate vs update-512 parent | 58 | 51 | 915 | +7 | +16 | fail |
| Pure GAE8, candidate vs GAE8 | 51 | 34 | 939 | +17 | +8 | pass |

The original-Pool3 comparison against the parent also failed the seat floor:
P0 was `-4` and P1 was `+11`. The pure-GAE8 comparison was positive at both
seats, P0 `+12` and P1 `+5`.

## Training and validity

The response started from the exact eight-update GAE state and completed eight
more updates, 512 terminal-only episodes at base seed `980001`, episode indices
`33280..33791`, ending at Adam step 528. Every nonterminal reward remained
zero. The complete-history critic only propagated the natural terminal result.

- Candidate payload SHA-256:
  `b1f71dd78fba0ba5693e28f4a976020bf04ff831f4f0f7c34938c0901e467c72`
- Final native-state SHA-256:
  `89bd00c3aca3a2597c3cdd2741ea89394c509926af933429673ce0395371e802`
- Final model-parameter SHA-256:
  `0112d8c91b2aba2bc0eb7f280e14e6f71d16d7ecabe588d4c9e0c3c4b4eaa704`
- Minimum sampled policy entropy: `0.2184159` nats, floor `0.10`
- Maximum gradient L2: `1.582134`, cap `5.0`
- Final parameter-space movement L2: `0.497320`, cap `0.75`

All 512 training receipts and all three 1,024-episode evaluation-arm schedules
were independently checked for exact range, count, and cross-arm pairing. The
reported candidate payload hash was recomputed from the published file. No
partial or staging artifact remained.

## Throughput

The bounded 1/2/4-worker screen selected 4 workers with 16 sessions each. All
three topologies produced the identical final native state
`3c4230e582fdd079bd92fcb3bf00a085282bea05c79b20027528cf4962effdf3`.

| workers x sessions | games per second | measured seconds |
| --- | ---: | ---: |
| 1 x 32 | 2.4880 | 25.72 |
| 2 x 32 | 15.2181 | 4.21 |
| 4 x 16 | 21.0251 | 3.04 |

The selected topology projected 316.6 seconds for training plus evaluation,
excluding setup. The completed development run took 497.7 seconds including
publication. GPU 1 sampling recorded 1.30 percent mean utilization, 96 percent
peak utilization, and 430 MiB peak memory. The low mean reflects a rollout
workload dominated by CPU rules simulation with short GPU inference bursts.

## Interpretation

This cycle confirms that the response machinery worked against its immediate
target. Against pure GAE8 it achieved `+17/1024`, or `+1.66` percentage points
on terminal order, with both seats positive. That response did not transport
to the broad original Pool3 panel, where it was only `+6/1024` versus GAE8 and
`+7/1024` versus the parent, with the latter regressing at P0.

This closes only the exact one-cycle population mapping, eight-update horizon,
and seed domain. It is a first-iteration transport failure, not evidence that
population methods are generally ineffective. Population response remains a
later option with iterative meta-game updates and an independently constructed
exploiter. The next distinct mechanism is search as a training teacher, using
the confirmed history-value critic at bounded-search leaves and distilling the
teacher's action targets into a policy that runs without search at inference.

## Evidence

- Source commit: `c2e03395b3b3275ca6263f5fabd16459acaaaf0a`
- Frozen design SHA-256:
  `12e125354aa791836078fbeba2b625e7caab4c36c7e3c93ea45b4c306ad2f3f2`
- Preflight manifest SHA-256:
  `9f78bac1576b08407392c3fedbe77b29d5eef9173b83af8458f0d2da6aeb91d5`
- Throughput report SHA-256:
  `f27815a94029253262f5d6cc42516623e42e6571ee8f314393b9f54739fdeb87`
- Development report SHA-256:
  `717c80fe855b9903baef19570119992fc43634d5d0809a69c38ded90c64ced6e`
- Evidence root:
  `D:\mtg-kernel-composed-factorial-v1\population-response-cycle-v1`

The first preflight attempt found a payload-hash versus native-state identity
comparison error and stopped before measurement or publication. Commit
`c2e0339` corrected the identity contract. The retry passed. Fable then
independently recomputed the published report hash, all gate values, movement
bound, numerical status, and the unique frozen decision with zero discrepancy.

## Nonclaims

- This development result does not promote or formally reject a policy.
- It does not consume V3 alpha or authorize a candidate-02 formal gate.
- It does not test robustness against an independently trained exploiter.
- It does not establish CP7, human, or professional-level playing strength.
