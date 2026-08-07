# Scaled self-play population candidate-02 V3 gate

Status: REVIEW REQUEST. Formal measurement is held until this exact sheet,
specification, implementation commit, and throughput screen receive an
independent countersign.

This sheet is governed by the countersigned Sequential Gate Contract V3 and
Erratum 1 at SHA-256 values
`3220e9b17ad13ff44e21e3f0bf4119d7f2b19652db30c0d77e03a164cca79d0f`
and `f205f6067b81bdc4245281cfa47dad1d5ea9980975329e1b9be9594285fd5091`.
The authoritative reference implementation SHA-256 is
`ffae17bdc020578a34d7cc420e138951fcb587531cf5191c978384a4bd4b73ef`.
The specification also binds the V3 test vectors at SHA-256
`de9ae592add1d077431ec44bebfc50a08f5263b15ed9b86061ca97ea9a9c41e1`
and native trainer schedule goldens at SHA-256
`6b2e1edbbe49b4e02f98794f9057f5c2bb8e3079d2ba8cb3e2a4b9ea6c34867c`.
The machine-readable specification is
`docs/native_scaled_selfplay_candidate_02_v3_spec_v1.json`, presently
SHA-256 `270ebb83378e9677d491cd7e9bf3fb057525cc52a7dfd50f748fc02562cdb7c1`.

## Nomination and comparison

The base population campaign ended at program update 1,024, global generation
1,536. All three Stores are complete, finite, reproducible, and above the
shared promoted(2) control on the scheduled terminal-only native read. The
paired terminal-order deltas over 2,048 games were +42, +66, and +32 for
lineages 970001, 970002, and 970003. Seed 970002 is therefore the development
nominee by the highest fixed-panel endpoint result. Its direct result was
1,045-1,003, with positive paired effects in both P0 and P1. This selection
uses revealed development outcomes and consumes no alpha.

The anchor report's +66 total is a raw terminal-rank difference. Dividing it
by 2,048 games gives 3.2227 percent on that report's scale. V3 instead takes
the sign of candidate-versus-control ordering on each leg and then averages
the two seats. The nominee's V3 development effect is therefore 1.6113
percent, not 3.2227 percent. Its V3 confidence sequence at N=1,024 and the
assigned alpha is `[-0.018477,+0.048906]`. This is not formal evidence.

Candidate-02 is the exact seed-970002 generation-1,536 Store:

- run SHA-256
  `dc171fe72549154e533e337bc39884faa76811809abc0bc573bb975cea500a42`
- checkpoint SHA-256
  `8d6219e0c5acf040de202793b6f73131a30585ce3a1fea33b73e52734e91e53b`
- native-state SHA-256
  `dc0f3c0d6ae9b4c87745c802b0b5b71b398b378d1689e9dd040c86ad12853ba2`
- model-parameter SHA-256
  `429446148ee88c527c307e0d9cde545a450a9f94e5be445b683d1d9955d93e53`

The comparison policy and the opponent are both the immutable promoted(2)
generation-384 policy, run SHA-256
`2c9b7423004428c0e2bb138afafc15ec65957f6bd98c4587bea704fbf9549aae`,
checkpoint SHA-256
`4bd38cf3a9af3fb04428fbc4286d4635007e848c7b9f0740122e430cbba8`, and
model-parameter SHA-256
`db58dbe3f1f76b5bdf3bae4de657711dc818393b2bf1eeae88c02d8866b4d01d`.
Both arms are fixed throughout measurement.

## Frozen V3 fields

- gate class: `LARGE-EFFECT`
- `delta_worthwhile=0.01`
- `delta_promote=0.01`
- candidate slot: 02
- initial alpha: `0.00875`
- mandatory confirmation alpha: `0.00875`
- `c=0.5`
- `max_N=131072` clusters per gate
- blinded pilot: none
- terminal win, draw, or loss is the only outcome

The campaign ledger assigns slot 02 as two separate `0.00875` rows and binds
both frozen schedule hashes. The specification binds that ledger at SHA-256
`7fbcc9350c28954b5b7ba16352959d1c009a909cc73bc37edc9d17b32733ec59`.

For cluster `k`, the candidate arm and control arm each play two natural games
against promoted(2), one from each learner seat, under the same environment
root. A leg score is +1, 0, or -1 from the ordering of candidate-arm terminal
return versus control-arm terminal return. The cluster score is the mean of
the two leg scores and is in `{-1,-0.5,0,0.5,1}`. The declared
conditional-mean class is `IID-MIXTURE`, with degenerate joint component law
`P((Primary,Primary))=1`. Every raw episode retains `Primary` as
the opponent component, and the analyzer requires component, seat, deck,
pair, and environment bindings to match across arms.

## Fresh schedules

The initial schedule uses 64 chunks of 2,048 clusters. Evaluation seeds begin
at `2608070001` and increase by `1000000` per chunk. Its frozen ordered
identifier SHA-256 is
`b8609141d42f5c5230dadc95d279ff17d5869de523a8b579e3ec93c10561868c`.

The confirmation schedule was frozen before any initial outcome. It uses the
same geometry, begins at evaluation seed `2808070001`, and has ordered
identifier SHA-256
`bd7612fc6a937b55a1f0ed9efec0042200a0effe3d3eaee822a5032de8af0100`.

The two schedules are disjoint from each other and from every training,
payoff, native-anchor, CP7, and revealed development schedule used to build or
select this candidate. A machine-readable freshness manifest, SHA-256
`e334b9641891b16337a94dc23cc1f405721087126e5a6c98ac90b3a2171f0a03`,
binds the nomination authorities and excludes every revealed native H2H
evaluation seed through `2000000000`. This includes generation-1,536 root
`1536300001` and all three lineage streams revealed under that root. Initial
and confirmation start above that interval. Formal order is ascending global
cluster index.
Candidate and control arms for two adjacent chunks run concurrently, so four
identical-shape evaluator processes fill the screened CPU topology. If a V3
boundary occurs inside an acquired wave, the first crossing is authoritative
and later clusters from that wave are retained but excluded.

## Sizing and cost

The revealed 1,024-cluster nomination panel had cluster counts
`{-1:3,-0.5:100,0:786,0.5:131,1:4}`, mean `0.01611328125`, and second
moment `0.063232421875`. This panel selected the candidate, so its effect is
not formal evidence and its sizing use is explicitly advisory.

The three endpoint V3 effects were 1.0254, 1.6113, and 0.7813 percent. Their
pooled counts were `{-1:12,-0.5:317,0:2346,0.5:383,1:14}`, with mean 1.1393
percent and second moment `0.0654296875`. This pooled law is retained as a
winner-selection sensitivity model.

At the assigned alpha, an independent 1,000-replicate empirical resampling
from the selected-lineage law produced 989 successes by 131,072 clusters,
with conditional p50, p80, p90, and p95 crossings at 39,548, 61,056, 75,760,
and 86,137. It used Python 3.14.3, NumPy 2.4.3, PCG64 seed 970002, IID
sampling with replacement, 2,048-row generation blocks, and V3 checking at
every look. The implied per-gate success rate is
98.9 percent and the two-gate success rate is about 97.8 percent if that model
is correct. The pooled winner-sensitivity law produced only 12 successes in
1,000 under PCG64 seed 970999, implying about 1.2 percent per gate and 0.014
percent for two gates.
The cap is therefore intended for an effect close to the observed 1.6113
percent. It is not a high-power design for an effect near the 1-point
boundary, and it is highly sensitive to winner shrinkage. This is an honest
launch-economics warning, not a validity gate and not a reason to lower the
1-point practical threshold after seeing the nomination panel.

Each cluster costs four games. The full cap is 524,288 games per gate. At the
measured generation-1,536 anchor throughput of 49.51 games per second, a full
gate is about 2.94 rollout hours. The observed-law median crossing is about
0.88 rollout hours. Confirmation runs only after initial `SUCCESS` and uses a
fresh schedule. Both full caps would cost about 5.9 rollout hours.

## Release and decision procedure

Before formal launch, the exact candidate and control identities, executable
SHA-256, source ancestry, contract and reference hashes, clean implementation
commit, slot and alpha ledger, freshness manifest, throughput screen, and
countersign are checked. The countersign binds the exact runner and independent
analyzer hashes as well as the commit, specification, design, and screen.
The review-request runner SHA-256 is
`d328547e5f369ee7f2a140bc9622d334d164b82701c76a6f1c72f20574e5d71d`;
the standalone analyzer SHA-256 is
`b45e0d6f2d1a3af8937c7ca7196a0bb26ced2322beb63d0f88a0ab65e85c7d25`.

The revealed screen is an end-to-end two-chunk, 256-cluster-per-chunk
candidate/control mini-gate,
not repeated candidate-only arms. The same four raw arms run under 1, 2, and 4
evaluator processes. All corresponding raw outcome hashes and the independent
inferential-core hash must be bit-identical, and four processes must achieve
at least 1.5x aggregate speedup over one process. Formal execution also
requires GPU 1 to have no compute process before every acquisition wave. GPU 1
remains reserved even though this fixed-policy evaluator is CPU-resident.

The acquisition runner writes a create-exclusive, flushed, and fsynced gate
plan before games. After each candidate/control chunk, it fsyncs the raw
outcomes and logs, then writes a create-exclusive, flushed, and fsynced chunk
receipt. It does not derive scores or make promotion decisions. After each
wave it invokes the separate analyzer process and reads only its verdict to
decide whether more acquisition is required.

The standalone analyzer starts from the plan, durable chunk receipts, and raw
outcomes. It rejects duplicate JSON keys, unexpected schemas or keys, missing
or duplicate chunks, any noncontiguous prefix, changed hashes, path escape,
wrong policy identity, wrong row order or strict type, nonintegral terminal
rank, non-Primary component, wrong Rally deck hashes, and any schedule drift.
For every row it independently derives `pair_index`, `episode_index`, seat, and
the native trainer environment seed from the exact SHA-256 schedule algorithm.
It then reconstructs the ordered five-value cluster stream and calls the exact
V3 reference implementation at every look.

The final analysis retains every ordered trajectory record, both the acquired
full-stream hash and authoritative decision-prefix hash, all raw authorities,
per-seat and overall leg counts at acquired and decision N, and explicit
post-decision exclusion. Initial `SUCCESS` is required before confirmation.
Before the first confirmation game, `verify-existing` independently rebuilds
the initial analysis from every retained raw authority and requires canonical
byte equality with the retained full analysis plus recomputed `SUCCESS`.
Promotion requires `SUCCESS` in both gates. Any other initial outcome closes
candidate-02 without confirmation.

Seventeen tests currently pass, including duplicate-key, wrong-schedule,
row-reorder, nonintegral-rank, forged-initial, freshness-overlap, full manifest
reconstruction, native schedule golden, and post-decision exclusion cases.

If candidate-02 closes, the population program's one predeclared extension is
the next development escalation, subject to the already frozen stability
conditions. If both V3 gates succeed, the applicable accumulation route may
open and the population extension is not launched merely to continue compute.

This gate supports only a strength claim against promoted(2) under native
Rally BO1. It is not a professional-level, metagame-wide, multi-deck, BO3,
XMage, or human-play claim.
