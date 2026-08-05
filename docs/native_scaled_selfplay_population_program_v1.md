# Native scaled self-play population program v1

Status: paper-only decision package, amended after `CLAUDE #185` on
2026-08-05. No compute is authorized by this sheet.

## Decision requested

Jack later decides whether to authorize the program, its seed count, and its
GPU allocation. The regularized continuation retest is complementary and is
the prerequisite causal de-risking lane. A retest success de-risks this
launch by showing that the original 512-update collapse can be controlled. A
retest failure voids this schedule as written and routes an amended schedule
for the observed failure class. Neither outcome authorizes compute by itself.

The original macro rung remains the risk to be addressed: 98,304 episodes
ended at 46.18% against promoted(2), with P1 at 41.18% and no better retained
checkpoint. The proposed program tests population pressure and long-horizon
self-play after that failure is either stabilized or explicitly re-designed.

## Training and population contract

- Three matched lineages continue the exact complete generation-512 Stores for
  retest seeds `970001`, `970002`, and `970003`. They use the retest's selected
  beta, immutable parent, environment, architecture, optimizer state, episode
  batch, and terminal objective. If the retest does not leave all three Stores
  complete and stable enough for its causal read, this package is invalid and
  must be amended rather than substituting a lineage.
- The base budget is 1,024 updates at 64 natural episodes per update. Retain
  checkpoints at updates 0, 64, 128, 256, 384, 512, 768, and 1,024. One
  predeclared extension may continue the same lineages to 2,048 updates, with
  checkpoints at 1,280, 1,536, and 2,048.
- The opponent pool has eight policy slots with four protected roles: two
  fixed anchors, two historical policies, two current policies, and two
  response exploiters. Anchor 0 is promoted(2) generation 384. Anchor 1 is
  immutable Pool3 predecessor A, source run
  `8bc06b6cf2e26df8002b5cece2784e0cd165cdd6bbd199a835e06c17e8d5de5c`,
  generation 512, with checkpoint SHA-256
  `03f0e226f884f51bf7128f70bec189bd6ac2c8f231ced8886f2cb7d3e936cc90`.
  Its sidecar and state SHA-256 values are
  `c56a8ba1361ab172c669307084c4522ee06ac79e39b7cf4a306f11effe36b031`
  and `2904dd7b899c21234c64925440277dbfa8d6f552d8f620b153bc8d16c44f523a`.
  Historical policies are immutable checkpoints retained for at least 256 updates;
  current slots hold recent checkpoints from active lineages; exploiters are
  separately trained responses to the current mixture and are not alternate
  weights of the learner.
- Pool identities are bound in the manifest. Start with equal 25% role
  weights. At each completed 128-update checkpoint, a fresh development
  payoff panel applies a clipped multiplicative-weights update with fixed
  rate `eta=0.10`, role floors of 20%, and a 25% cap on any one policy.
  Refresh occurs only at the boundary, never mid-interval or after inspecting
  a later checkpoint. The archive retains displaced policies, so refresh
  changes exposure rather than erasing history.
- Seed-to-slot assignment is deterministic and outcome-free. Sort the three
  seeds as `s0 < s1 < s2`. At program update `t`, where `t` is a multiple of
  128 and refresh index `r=t/128`, bind
  `current_0=latest(s[r mod 3])`,
  `current_1=latest(s[(r+1) mod 3])`, and let
  `h=s[(r+2) mod 3]`. Bind `historical_0` to lineage `h` at generation
  `512+t-256` and `historical_1` at generation `512+t-384`. Thus the initial
  pool uses generations 256 and 128 from the retest lineage omitted from the
  two current slots, and every later historical reference is already retained
  and at least 256 updates behind the current generation. Missing, duplicate,
  or outcome-selected bindings invalidate the refresh manifest.
- Response exploiters are independently trained narrow Net8 policies, not
  learner checkpoints, residuals, or aliases. Their exact architecture is
  `kernel-policy-value-net-8`, model-config schema 5, with 64-wide hidden
  layers, 16-wide card embeddings, and 1,230,994 parameters under model-config
  fingerprint
  `f3836afa17acc74b4856fe18222345116f27c12fa5ad18c34b4dec3f04855251`.
  At program updates 0, 256, 512,
  and 768, and again at 1,024, 1,280, 1,536, and 1,792 only if the extension
  is authorized, build two exploiters from promoted(2) generation 384 with
  fresh Adam, distinct predeclared training seeds, envrand-v2, 256 updates,
  64 natural episodes per update, and terminal W/D/L as the only reward. Their
  policy input is the exact learner-visible native observation and legal-action
  set; no opponent hand, environment seed, privileged state, or future state is
  available. Their frozen target is the six-slot anchor/current/historical mixture after that
  boundary's deterministic refresh, renormalized without either exploiter
  role. The two builds may use the accepted campaign topology, but parameter,
  model, and Store hashes must all be distinct.
- Each exploiter and one fresh promoted(2) control shared by that rebuild round
  then play 1,024 common-random-number seat-swapped pairs against that exact
  six-slot mixture. Require paired
  terminal-order net at least `+24` overall and at least `-8` in each exploiter
  seat. The exploiter also plays 512 fresh seat-swapped pairs against pure
  promoted(2), requiring direct score `(W+0.5D)/games >= 0.47` overall and at
  least `0.44` in each seat. A failing exploiter slot is filled only by the
  next unique checkpoint in a predeclared immutable archive and is labeled
  `historical-fallback`. A build failure, nonzero training exit, incomplete or
  nonfinite Store, missing identity binding, or duplicate parameter or model
  hash is an exploiter failure and follows the same fallback rule. If two
  unique fallbacks are unavailable, launch or continuation is blocked. Any
  fallback result tests population pressure but makes no exploiter-robustness
  claim. The retired 160-parameter CEM response oracle is not reused.
- External CP7 policies are measurement anchors, not native training-pool
  members. No checkpoint is promoted merely for diagnostic movement, payoff,
  entropy, value loss, or utilization.

## Measurement cadence and topology

At updates 256, 384, 512, 768, and 1,024, read each lineage and one shared
promoted(2) control against promoted(2) on 1,024 fresh common-random-number
seat-swapped pairs. Report direct W/L/D and candidate-versus-control paired
terminal order overall and by physical seat. If the extension is authorized,
repeat at 1,280, 1,536, and 2,048. Run CP7 external-anchor panels at 512,
1,024, and 2,048, subject to the separately authorized bridge and panel
contract. Each CP7 panel contains 128 fresh common-random-number seat-swapped
pairs, or 256 games, per lineage. Thus the base program contains 1,536 CP7
games at updates 512 and 1,024, and the extension adds 768 games at update
2,048. These are periodic development measurements only, never automatic
advancement gates.

Every 128-update population refresh evaluates the complete eight-slot payoff
matrix on 512 fresh seat-swapped pairs per unordered matchup. There are
`C(8,2)=28` matchups, so each refresh contains 14,336 pairs and 28,672 games.
The multiplicative-weights input is normalized terminal-order payoff, never a
raw count. Using the retained planning variance `0.036617`, one 512-pair
matchup has net SD `2*sqrt(512*0.036617)=8.66`, or 0.846 percentage points on
the normalized score. With `eta=0.10`, this contributes about `0.000846`
log-weight SD per refresh and `sqrt(8)*0.000846=0.00239` over the eight base
refreshes before clipping, floors, and the policy cap. A smaller panel is not
eligible.

Before the full run, use the bounded topology screen already specified for
the retest: compare one original `2/32/16` run on exclusive GPU 1 with two
simultaneous original-topology runs on GPU 0 and GPU 1. Use the two-device
topology only if it is resource-safe, produces bit-identical same-seed Stores,
and is at least 1.5x faster in aggregate. Otherwise use GPU 1 only. Formal V3
promotion measurement is a later, separate workflow and remains exclusive to
headless GPU 1, regardless of the training topology selected here.

## All-in compute budget

The base program has the following fixed work inventory. Active-lineage
training is `3*1,024*64 = 196,608` episodes. Four exploiter rebuild rounds add
`4*2*256*64 = 131,072` episodes, for 327,680 native training episodes total.
The five native anchor reads contain `5*4*1,024*2 = 40,960` games; eight payoff
matrices contain `8*28,672 = 229,376` games; and four exploiter-evaluation
rounds contain `4*(3*1,024*2 + 2*512*2) = 32,768` games. The base therefore
contains 303,104 native evaluation games and 1,536 CP7 games in addition to
training.

The optional 1,024-update extension has the same 327,680 native training
episodes. Its three native anchor reads contain 24,576 games, eight payoff
matrices contain 229,376 games, four exploiter-evaluation rounds contain
32,768 games, and its CP7 panel contains 768 games. Thus the extension adds
286,720 native evaluation games and 768 CP7 games. Through update 2,048 the
program totals 655,360 native training episodes, 589,824 native evaluation
games, and 2,304 CP7 games.

Wall time is frozen from three directly measured rates, not from the historical
macro rung whose duration was not recorded. The completed prerequisite screen
is
`D:\mtg-kernel-regularized-continuation-retest-v1\preflight\seed-969999\throughput-screen-hotfix-v1\attempt-001\throughput-manifest.json`,
SHA-256
`e7f95a05bae1f69d9db6fc26b701b9e1a6ef8a25ddfe4798fcfb1f41883755be`.
Its official startup-inclusive rates were 1.8370 episodes/s on GPU 1 and
3.5069 episodes/s on both GPUs, a resource-safe, bit-identical 1.9090x
speedup that selected the two-GPU topology.

For long-run pricing, the same screen separates one-time process setup from
the update-4-to-update-8 checkpoint slope. The measured post-warm rates are
13.5080 episodes/s on GPU 1 and 27.2659 episodes/s in aggregate on GPU 0 plus
GPU 1. The corresponding fixed launch terms are 240.805 seconds per GPU-1
process and 254.243 seconds per paired two-GPU launch wave. Every 128-update
pool refresh requires a process boundary. The base therefore has 32 training
process launches: 24 active-lineage segments across eight refresh intervals
and eight exploiter builds. That is 32 serial GPU-1 launches or 16 two-GPU
waves. The extension has the same 32-launch, 16-wave inventory.

The native evaluation planning proxy is 2,048 frozen-identity games in about
150 seconds, or 13.65 games/s. The CP7 proxy is the completed 256-game panel
in 989.00 seconds, or 0.2588 games/s. With no credit for unmeasured native-read
concurrency, the component and all-in wall estimates are:

| Branch | Training wall | Native reads | CP7 | All-in with 10% contingency |
| --- | ---: | ---: | ---: | ---: |
| Base, GPU 1 only fallback | 8.88 h | 6.17 h | 1.65 h | 18.37 h |
| Base, selected GPU 0 + GPU 1 | 4.47 h | 6.17 h | 1.65 h | 13.51 h |
| Extension, GPU 1 only fallback | 8.88 h | 5.83 h | 0.82 h | 17.09 h |
| Extension, selected GPU 0 + GPU 1 | 4.47 h | 5.83 h | 0.82 h | 12.24 h |
| Base plus extension, GPU 1 only fallback | 17.76 h | 12.00 h | 2.47 h | 35.46 h |
| Base plus extension, selected GPU 0 + GPU 1 | 8.94 h | 12.00 h | 2.47 h | 25.75 h |

The 10% operational contingency covers checkpoint publication, topology tails,
and one permitted preflight restart. It does not fund a failed formal run, an
extra beta, a second extension, or any V3 promotion gate. Native-evaluation
concurrency may reduce realized wall time, but it is not credited until a
bounded same-work screen demonstrates the rate and resource safety.

## Reward, stopping, and escalation

Every nonterminal reward is zero. The terminal result is the only reward:
win `+1`, draw `0`, loss `-1`. Estimators may propagate that terminal result,
but no life, material, board, tempo, damage, engine, or other proxy reward is
allowed. Terminal W/L/D is the only strength, success, and promotion signal.
V3 reporting may use its prescribed terminal-order score, including draw
weight `0.5`, but that reporting convention never becomes a training reward.

Hard-stop on any nonfinite or incomplete Store, identity/hash mismatch,
reward-contract violation, broken seat/pair binding, or failed bit-identical
verification. For one anchor read, define paired terminal-order leg net between
the lineage and the shared promoted(2) control. A read crosses the harm
boundary at overall net `<= -50`, P0 net `<= -36`, or P1 net `<= -36`. Stop a
lineage only when two consecutive scheduled reads cross at least one boundary,
and stop at the second read without rescuing an intermediate peak.

At 1,024 pairs, the retained planning variance gives overall net SD
`2*sqrt(1024*0.036617)=12.25`; seat SDs scale to 8.71 and 8.58. The boundaries
are approximately 4.08, 4.13, and 4.20 null SD below zero, with one-read normal
tail approximations `2.3e-5`, `1.8e-5`, and `1.3e-5`. Their union is at most
`5.4e-5` per read under that planning law. Across three lineages and four
adjacent base read-pairs, a conservative union that does not use read
independence is `12*5.4e-5=0.065%`; with the required fresh independent panels,
the planning approximation is `12*(5.4e-5)^2=0.0000035%`. These are sizing
calculations, not strength evidence. The overall boundary is 2.44 percentage
points below zero paired effect, before the recorded 3.82-point pooled macro
collapse, and the seat boundary is 3.52 points below zero, before the recorded
P1 collapse.

The 1,024-to-2,048 extension is allowed once, only if all lineages remain
finite and reproducible, no harm boundary is crossed, and the latest refresh
still satisfies the frozen population and parent-drift envelopes. If the
program is stable but has no predeclared strength signal at 1,024, this one
extension is the only escalation. If the extension also fails, close the lane
and amend the population or horizon design before new compute. If the retest
shows late KL loss, collapse not reproduced, or stable-no-strength, record
that classification and amend this sheet rather than silently changing beta,
pool weights, seeds, or checkpoints.

## V3 promotion route and nonclaims

A development endpoint can nominate a lineage but consumes no alpha. A
nomination requires a fresh, countersigned V3 candidate sheet binding the
exact Store, source, pool, seed schedule, and toolchain. Candidate IDs 02, 03,
and 04 are equally unassigned; nomination claims no slot. At countersign time,
assign the nomination to the lowest then-unassigned slot. If two sheets race,
the first countersigned sheet claims 02 and the next claims 03. The assigned
slot uses its ledger allocation of `alpha=0.00875` for the initial gate and
`alpha=0.00875` for mandatory independent confirmation, with `max_N` sized by
the actual V3 contract. Both gates must pass. Only a successful V3 candidate
opens the applicable accumulation chain.

This program concerns native Rally BO1 self-play. It does not establish
human, professional, metagame-wide, multi-deck, BO3, sideboarding, or
competitive strength. CP7 anchors do not substitute for V3 formal evidence,
and a native or external development result alone does not establish a
pro-level claim. The schedule also does not establish that population
refresh, regularization, environment randomization, or any other component is
causal unless its matched retest and formal contract support that conclusion.
