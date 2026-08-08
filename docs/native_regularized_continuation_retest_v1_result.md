# Native regularized continuation retest v1 result

Status: `ADVANCE`, development-only. Completed 2026-08-06.

## Bound evidence

- Design commit: `e9bd7e5b4ef7b8320bb22edfc573ba50a8496ba7`
- Design document SHA-256: `1f6ea9128e7d5b44f80c34c96c42ccd47325224fb11d22bc4aae0d43cef40b00`
- Evaluation implementation commit: `0153d32e2bce40707c3b22991e2016355e00e89e`
- Training manifest SHA-256: `0a430d62ec6a20d8f752bbcc4d71e15bf8e3a4a339917a07e7afd97d4ff7ef04`
- Parent-drift manifest SHA-256: `428a655b1cab485761c0ae3b665cf41a6cf672047decdfde8b4596e206d6dee8`
- Throughput manifest SHA-256: `cf5d6d4d96c662633a78304725743dd568971a8b73f918d431f401aa9feee916`
- Formal evaluation manifest:
  `D:\mtg-kernel-regularized-continuation-retest-v1\development\seed-982001\full-horizon-evaluation\attempt-001\formal\full-horizon-evaluation-manifest.json`
- Formal evaluation manifest SHA-256:
  `f3128e5f700830df2110d6abb06b5b6f7f8f642ac5064c5d3188afac93aed2c8`

The formal phase contained 21 streams and 58,368 terminal games. All streams
completed before any terminal result was read. Runtime was 814.87 seconds at
71.63 aggregate games per second. All stderr logs were empty.

## Causal prerequisite

The matched beta-zero controls reproduced the late collapse on two of three
seeds, satisfying the frozen threshold of effect at most `-0.025` on at least
two seeds:

| Seed | Control generation 512 minus 384 |
| --- | ---: |
| 970001 | -0.08398 |
| 970002 | -0.00928 |
| 970003 | -0.03833 |

Qualifying seeds were 970001 and 970003. The causal read is valid.

## Terminal results

All quantities below use only terminal W/L/D. Endpoint effect is regularized
generation 512 minus its matched beta-zero generation 512 control on the
common-random-number panel.

| Seed | Candidate score | Control score | Endpoint effect | Candidate 512 minus 384 | Frozen read |
| --- | ---: | ---: | ---: | ---: | --- |
| 970001 | 0.49683 | 0.42725 | +0.06958 | +0.00684 | SUCCESS at 231 |
| 970002 | 0.50244 | 0.48364 | +0.01880 | +0.00293 | INCONCLUSIVE at 2,048 |
| 970003 | 0.50171 | 0.46582 | +0.03589 | +0.00195 | SUCCESS at 670 |

The pooled candidate P1 endpoint effect was `+0.05013`. All three candidate
late-window overall and seat effects stayed above their frozen stability
floors. The H4 seat-linked-drift condition did not reopen. Parent-drift ratios
at generation 512 were 0.00678, 0.03745, and 0.02626, so the late-anchor-loss
trigger did not fire.

All five advancement clauses passed. The frozen development nomination is seed
970002 at generation 512, selected by the highest fixed-panel score of 0.50244
with the lower-seed tie break available if needed.

## Lineage integrity and routing

All three candidate Stores are complete at generation 512, Adam step 512, and
32,768 completed episodes. Their tree hashes were unchanged from immediately
before formal measurement through the final manifest:

| Seed | Store tree SHA-256 |
| --- | --- |
| 970001 | `2d6650f111cebcb8e87271fb3446127306e2c4006da793c45a7aec5d80c7780e` |
| 970002 | `bcecb18db197a5ef14c8512642a3f15191f7dd05e389c02c129853c9496deda7` |
| 970003 | `1a1bdb75099b50b4d250d3e03ab6d882718f017e2c6d715bc8a67d3022b627ec` |

This result satisfies every condition precedent in `CLAUDE #187`, activating
Jack's exact pre-authorization for the scaled self-play population program at
commit `838920e359c7a1152d97c450f4575c6be2309f22`, document SHA-256
`b0e836858379137e9f5068f1ed2d3cb98d0d6507d09170d8272caad2a989ea38`.

## Nonclaims

This is a development result for native Rally BO1. It is not a V3 promotion,
professional-level result, metagame-wide result, multi-deck result, BO3 result,
or human-play result. It establishes that beta 0.1 prevented the reproduced
late collapse under this matched retest and passed the frozen development
advance conditions. It does not establish which downstream population
mechanism will add further strength.
