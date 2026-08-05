# Bounded history-value search-teacher distillation v1 result

Status: complete, stopped by the frozen selection-residue gate.

## Decision

The unique result is `STOP_SEARCH_TEACHER_DISTILLATION_V1`. The formal
search-target learner passed every identity, numerical, movement, and per-seat
NLL nonregression check, but it did not clear either substantive search-label
fit threshold. Residue 0 was not evaluated, no full-corpus refit occurred, and
no model or terminal selector was produced.

This closes the exact 256-pair source panel, depth-8 four-sample teacher,
`0.25` margin, width-128 recurrent residual, fixed eight-epoch objective,
movement envelope, and seed domain. It does not show that all search,
distillation, or lookahead is ineffective.

## Formal corpus and throughput

The fresh corpus contained 256 matched pairs, 512 natural games, 8,195 search
diagnostics, and no failed tasks. Every reference and shadow outcome and
opponent-teacher stream was byte-identical. Equal-episode-mass teacher
override rates were `0.120563` at P0 and `0.107498` at P1, both above the
frozen `0.05` pre-fit gate.

A retryable exact-path GPU 1 screen ran the entire 3,833-decision training
split for one epoch. Fifteen optimizer batches took `1.44665` seconds,
evaluation of 2,109 selection decisions took `0.276223` seconds, and GPU 1
used at most 420 MiB of 6,144 MiB. The conservative full-run projection was
2.27 minutes. Average sampled utilization was 22.55 percent and peak sampled
utilization was 41 percent. The fixed batch and model recipe was retained
because the projected run was already short.

## Frozen learner result

The formal process wall time was 37.7 seconds, including approximately 22
seconds of cache loading. The trainer's measured fit and selection phase took
`13.28975` seconds.

| selection metric | search target | fallback control | required | result |
| --- | ---: | ---: | ---: | --- |
| Search-target NLL | 1.306564 | 1.331296 | at least 5% relative improvement | 1.8577%, fail |
| Search-target top-1 | 0.649228 | 0.653436 | at least +3 percentage points | -0.4208 points, fail |
| Mean TV | 0.016890 | 0.020271 | at most 0.03 | pass |
| p90 TV | 0.043630 | 0.052954 | at most 0.10 | pass |
| Maximum absolute log ratio | 0.4899998 | 0.449242 | at most 0.49 | pass |

Search-target NLL was lower than the control at each seat: `1.490618` versus
`1.519605` at P0 and `1.126267` versus `1.146831` at P1. Both fits were finite,
all pre-clip gradients were below 5, and every source and split identity
matched the validated plan.

## Revealed-residue diagnosis

One post-hoc diagnostic reproduced both formal model-state hashes exactly and
used only the already revealed training and selection residues. It did not
touch held-out residue 0 and cannot change the formal stop.

On the 264 selection decisions where the teacher differed from the fallback,
the search learner improved teacher-target NLL by only `3.3409%` relative to
the control and reduced top-1 by `0.9333` percentage points. Even on the 489
training overrides, it improved NLL by only `4.2034%` and top-1 by `1.2043`
points. Selection-override teacher NLL remained `6.1438`, showing that many
teacher actions stayed far outside the parent policy's high-probability
region. The unchanged rows had a small search-arm regression.

The result is therefore not just a held-out generalization miss. The sparse
teacher signal and hard movement envelope did not let this fixed residual
substantially reproduce the search actions. Reweighting or relaxing the cap
would define a new learner, but the same depth-8 selector has already failed a
fresh direct terminal gate at one gain, one loss, and fourteen ties. Chasing
this teacher further is lower priority than obtaining an absolute external
baseline for the strongest current Net8 lineage.

## Evidence

- Source commit: `93b4136575cc848cf07d1eb705406d96c6056df5`.
- Formal corpus report SHA-256:
  `af6c3edafeff6edae3476c5bec3c7e834b31260999b4752df59f34c410ef12ed`.
- Complete-history cache SHA-256:
  `c595083b0d6301883253b251876a9d39eb7b4f6bce351fd1f398c1ce117408c9`.
- Throughput report SHA-256:
  `488463fd66249f8b94c40d1d35afd1a866b2c25dfb27c904c8f4ea1ef54507fe`.
- Formal manifest SHA-256:
  `66365486d7fe889305df4bdf817f23cc496586fd0cc8e92045aed48dc112d14a`.
- Formal learner report SHA-256:
  `6c84b1fd8b32cbf7542bf5f2b45c8c5623784c317f0338c7af9980fd0a7e72a3`.
- Revealed-residue diagnostic report SHA-256:
  `b53928bd0686a6b84ad2ba6a3cbf91b12ce9000aa440f6a6fc24a640eb91f5a7`.
- Evidence root:
  `D:\mtg-kernel-search-teacher-distillation-v1`.

## Next lane

The next rapid implementation is a strict raw-native-state bridge into the
existing XMage scorer, followed by a small common-root CP7 skill-7 benchmark
of current Net8 GAE8 and its fixed GAE16 extension. This measures external
Rally strength and candidate ranking directly in terminal win, draw, or loss
before another training mechanism is chosen.

## Nonclaims

- Offline label fit is not playing strength.
- Search labels were supervised targets, never rewards.
- The post-hoc decomposition did not reopen any threshold or held-out data.
- XMage CP7 is an external software anchor, not professional-level evidence.
