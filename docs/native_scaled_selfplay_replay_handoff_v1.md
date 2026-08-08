# Native scaled self-play replay handoff v1

Status: implementation contract. Jack's authorization is recorded by
`CLAUDE #187`; the replay realization and corrected generation mapping are
countersigned by `CLAUDE #188` and `CLAUDE #189`. This document changes no
science choice in the authorized population program.

## Authorities

- Program commit:
  `838920e359c7a1152d97c450f4575c6be2309f22`.
- Program document SHA-256:
  `b0e836858379137e9f5068f1ed2d3cb98d0d6507d09170d8272caad2a989ea38`.
- Retest formal manifest:
  `D:\mtg-kernel-regularized-continuation-retest-v1\development\seed-982001\full-horizon-evaluation\attempt-001\formal\full-horizon-evaluation-manifest.json`.
- Retest formal manifest SHA-256:
  `f3128e5f700830df2110d6abb06b5b6f7f8f642ac5064c5d3188afac93aed2c8`.
- Retest disposition: `ADVANCE`, with the causal control-collapse prerequisite
  satisfied and all three candidate Stores complete and stable.

## Global and program generations

The successor Run has global target generation 1,536. Global generations
0 through 512 replay the exact retest recipe. Program update `t=0` is the
verified global generation-512 handoff. The eight 128-update population
intervals end at global generations 640, 768, 896, 1,024, 1,152, 1,280,
1,408, and 1,536. A permitted program extension to `t=2,048` ends at global
generation 2,560.

The replay phase uses the exact retest seeds, immutable parent, beta `0.1`,
Pool3, environment-randomization V2, architecture, batch size, optimizer,
terminal objective, and seed schedule. It reads no terminal outcomes. The
population engine cannot activate before all three handoff checks pass.

## Three-lineage handoff gate

Each successor Run manifest binds the originating retest Store tree hash, the
retest formal manifest and SHA, and the exact generation-512 checkpoint,
sidecar, and native-state SHA values below.

| Seed | Retest Store tree SHA-256 | Checkpoint SHA-256 | Sidecar SHA-256 | Native-state SHA-256 | Model-parameter SHA-256 |
| --- | --- | --- | --- | --- | --- |
| 970001 | `2d6650f111cebcb8e87271fb3446127306e2c4006da793c45a7aec5d80c7780e` | `21f95221663a7a064d4d5935d19c95dc108a84085513524f48def0b0da21a2bc` | `2ee82c53afb9c4cd8343ca67411d9a0b5db800215688f809a08a44c8016953a5` | `e2e3fdb4216a013fdb043bcb90f33f590d5f7d72a77b5999c423919da3ae3b85` | `a51d05f8f89e3cca652e8c2daaa289a65cfdb317164d07410395430044b54ed0` |
| 970002 | `bcecb18db197a5ef14c8512642a3f15191f7dd05e389c02c129853c9496deda7` | `c3aa704e7670c158da82ad4602a20bcec3240f275ecb7aac9ca42fb341f482df` | `16c834b632e99589c5970dc52164ea12647f954e43e7bfe61b5d4d767133b9aa` | `304053bdc96ef094d97506f5605fc599aae045c770cbd6fa7efcebfccc9069b6` | `1e9022105aec341101c0b14ffa4d509b4073a2f80b213e71dd0065f036e701dd` |
| 970003 | `1a1bdb75099b50b4d250d3e03ab6d882718f017e2c6d715bc8a67d3022b627ec` | `814583b210191bc00ec1cf5f485eb6b83ffce2d4c2e632b87874d64e3b62cb3e` | `50108e3751ab52b6432903cac0b57addb747e287e41bc83f57e0bf9110149788` | `b3a8811923533bda7b1a8d2dbfa0b5b8ec187b1d40a7029d348a0dabbb04dbc3` | `861f28ca95316e68d1552986294aae0f7677af64b21f615d5bfcaff01276602c` |

At global generation 512, each successor native-state file must have Adam step
512, successful-update count 512, completed-episode and next-episode counters
32,768, and raw state and model-parameter SHA-256 values equal to its table
values. Model-only equality is insufficient. Any mismatch is
`FAIL-INVESTIGATE`; no lineage enters population training and no partial
two-of-three handoff is allowed.

## Store and refresh authority

The successor run contract must represent both the fixed replay phase and the
population phase while preserving the canonical bytes of every pre-existing
RunV2 record. An absent population section must serialize to no bytes. The
population section binds the program authority, activation generation 512,
base target 1,536, refresh cadence 128, terminal-only reward, and the identity
of the chained refresh-manifest protocol.

Every refresh manifest binds its previous manifest SHA, global generation,
program update, all eight slot identities and source checkpoint hashes,
normalized policy weights, payoff-panel inputs, and the next interval. A
refresh may reference only already-complete checkpoints. Missing, duplicate,
future, outcome-selected, or hash-mismatched slots fail before training. The
runtime engine must match the active manifest exactly. An unrecorded engine or
mid-interval refresh is invalid.

## Validation before replay

1. Run the focused canonical RunV2 and Store validation tests. Legacy fixture
   bytes and run SHA values must remain bit-identical.
2. Exercise missing, unknown, wrong-generation, wrong-authority, and early
   population-activation failures.
3. Rerun a short matched seed under both the retest and successor records and
   require identical native state through a checkpoint close/reopen.
4. Run the bounded topology screen and report expected wall, achieved rate,
   utilization, and selected topology. No formal promotion measurement shares
   this window.
5. Only then run the 98,304-episode replay and apply the three-lineage handoff
   gate before reading outcomes or building the first population interval.

## Replay accounting

Replay is a mechanical-realization overage of 98,304 episodes. The exact prior
matched training manifest is
`D:\mtg-kernel-regularized-continuation-retest-v1\development\full-horizon-training\attempt-003\training-manifest.json`,
SHA-256 `0a430d62ec6a20d8f752bbcc4d71e15bf8e3a4a339917a07e7afd97d4ff7ef04`.
It records 10,836.1347 seconds at 9.071869 episodes/s on
`gpu0+gpu1, then gpu1`, so the current replay ETA is about 3.01 hours. The
bounded screen may replace this planning rate only with measured same-work
evidence. Replay cost is reported separately from the authorized active
lineage budget.
