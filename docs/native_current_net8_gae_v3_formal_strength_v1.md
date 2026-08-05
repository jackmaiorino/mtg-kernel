# Current Net8 history-value GAE V3 formal strength v1

Status: initial gate complete, candidate closed without confirmation. This
sheet is governed by
`SEQUENTIAL-GATE-CONTRACT-DRAFT-V3.md` plus
`SEQUENTIAL-GATE-CONTRACT-V3-ERRATUM-1.md` in `collab`.
Their frozen SHA-256 values are respectively
`3220e9b17ad13ff44e21e3f0bf4119d7f2b19652db30c0d77e03a164cca79d0f`
and `f205f6067b81bdc4245281cfa47dad1d5ea9980975329e1b9be9594285fd5091`.
The authoritative `eb_cs_reference_v1.py` SHA-256 is
`ffae17bdc020578a34d7cc420e138951fcb587531cf5191c978384a4bd4b73ef`;
its fixture package SHA-256 is
`de9ae592add1d077431ec44bebfc50a08f5263b15ed9b86061ca97ea9a9c41e1`.

## Candidate and comparison

The candidate is the exact eight-update terminal-only history-value GAE state
at `D:\mtg-kernel-composed-factorial-v1\current-row-fresh-eval-v1\history-value-gae.state.f32le`:
file SHA-256 `a0b7752181a562f8e5a0821a490ce20b777b509855d754283536e8242f489b98`,
native-state SHA-256 `ab7dd25ca6619a4a613ca089e1eb8e75981f8e5cfc0bae8535b78cddd7efa952`,
model-parameter SHA-256 `5efe2f167045bde379da3be8af6c480b6702f5d7a849ff8435d8ac6b1d91daa8`.
The parent is the retained update-512 state with native-state SHA-256
`00333d987584d5cf7f9a37f1ba2b558cfd22a60388f2487c1bf1623fcc6686a0`
and model-parameter SHA-256
`5c8e09aabab375a2eb73aba2201b8d616a18bac13f28f74a03d93c6ff0e05c6b`.
Both policies are fixed during measurement. Natural terminal win, draw, or
loss is the only outcome. No critic, shaped reward, or training update enters
the gate.

## Frozen V3 fields

- `gate_class`: `LARGE-EFFECT`
- `delta_worthwhile`: `0.01`
- `delta_promote`: `0.01`
- `alpha`: `0.00875`, candidate slot 01 initial; confirmation independently
  receives `0.00875`
- `c`: `0.5`
- `max_N`: `16384` clusters for each gate
- `conditional_mean_stability`: `IID-MIXTURE`
- `blinded_pilot`: `none`

One inferential cluster is an exact native schedule pair. Episode `2k` puts
the learner in P0 and episode `2k+1` puts it in P1; both share the same
environment root. Each leg is played once by the candidate and once by the
parent under common random numbers. A leg score is `+1`, `0`, or `-1` from the
ordering of candidate versus parent terminal return. The cluster score is the
mean of its two leg scores and is therefore in `{-1,-0.5,0,0.5,1}`.

Pool3 selection remains the frozen per-episode KDF and 40/20/20/20 threshold
rule. The declared joint law is the product of the two leg draws, each with
the exact modulo-100 marginal induced by a uniform u63 KDF result:
`P(primary)=(40*q+8)/2^63` and each other member's probability is
`20*q/2^63`, where `q=floor(2^63/100)`. Both realized component identities
are retained in every cluster record and asserted identical across arms.

## Frozen schedules and freshness

Identifiers use
`mtg-kernel-native-trainer-schedule-sha256-v2;base_seed=970001;pair_index=<k>;episode_p0=<2k>;p0_component=<member>;episode_p1=<2k+1>;p1_component=<member>`
and the V3 erratum's `canonical_ordered_identifier_sha256()`.

- Initial: episodes `131072..163839`, pairs `65536..81919`, schedule SHA-256
  `488b64430f2aa806dbaa2689e6bd0d14570f87ed091ca1ac4c553561d05dfa96`.
- Confirmation: episodes `196608..229375`, pairs `98304..114687`, schedule
  SHA-256 `b82fa7bd4b4220bcfac60415c097448e7d992846871f1d485865dc3e12f9faaa`.

Both are disjoint from training episodes `32768..33279`, the revealed
development evaluation `65536..66559`, each other, and every other revealed
panel used to select this candidate. The confirmation schedule was frozen
before initial outcomes. Formal execution order is ascending cluster order,
parent arm then candidate arm within each 32-cluster acquisition batch.

## Assigned-alpha sizing and launch gate

The sizing model conservatively pairs the development leg counts into their
highest-variance same-sign arrangement: 26 scores at `+1`, one at `+0.5`, 12
at `-1`, one at `-0.5`, and 472 at zero. This preserves the observed
`Delta=0.02734375` while using second moment `0.0751953125`. At the assigned
`alpha=0.00875`, 750 deterministic simulations through the exact V3 reference
implementation produced 749 successes by 16,384 clusters. Success crossing
quantiles were median 4,486, p80 7,007, p90 8,485, p95 9,662, and p99 13,043.
The chosen cap is therefore 16,384 clusters.

Each gate has at most 32,768 episodes per arm, 65,536 physical games total.
At the measured fixed-policy rates of 51.54 candidate and 53.38 parent games
per second, the rollout-only cap is about 21 minutes. Allowing setup and
durable publication, budget 25 minutes per gate and 50 minutes if both gates
run to their caps. Before initial launch, the exact evaluator must pass
candidate/state identity checks, raw pair/component retention tests, one
bit-identical replay seed, and the required 1/2/4-worker by 64/32/16-session
throughput screen on already revealed development roots. GPU 1 is exclusive
for measurement.

The authoritative V3 reference computes every look in ordered cluster space
from the retained raw records. Initial `SUCCESS` is required before launching
the already-frozen confirmation. Promotion requires `SUCCESS` in both gates.
Any other initial verdict closes the candidate without confirmation. This
gate supports only a strength claim against the update-512 parent under the
frozen Rally/Pool3 kernel distribution, not a pro-level or real-MTG claim.

## Completed initial result

The formal initial measurement completed on 2026-08-04 at the full cap of
16,384 clusters. The raw store contains exactly 512 atomic chunk files and no
partial files. The release test completed in 1,256.05 seconds with the selected
4-worker by 16-session topology. The raw report SHA-256 is
`c8bcba394491bacfa737b84f39630323b1c151f7e51bad6bea516af7978a9d9e`.

The candidate had 1,373 favorable, 1,028 unfavorable, and 30,367 tied leg
comparisons. The cluster score sum was `172.5`, giving the terminal-only
paired effect estimate `Delta_hat=0.010528564453125`. The final V3 confidence
sequence was `[0.00475336130478099, 0.018388660485134878]`. Its lower bound
did not reach `delta_promote=0.01`, while its upper bound remained at or above
`delta_worthwhile=0.01`.

The frozen verdict is therefore `INCONCLUSIVE-AT-MAX-N`, with decision
`n=16384`. The analysis SHA-256 is
`fd6940053d9d307621465e39bf792843aaa874b26fd4a4f4abcb4a2979bd1ffb`.
The candidate is not promoted, and the independent confirmation gate is not
launched. The nominal positive estimate is descriptive evidence for this
fixed policy under Rally/Pool3, not a successful gate or a pro-level claim.
