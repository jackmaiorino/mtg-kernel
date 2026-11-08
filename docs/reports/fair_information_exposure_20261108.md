# Known-hand information exposure in pinned Pauper runs

Audit date 2026-11-08. Source baseline `3b19241fc`. This audit reads source, configurations and identity receipts for [#567](https://github.com/jackmaiorino/mtg-kernel/issues/567) and [#568](https://github.com/jackmaiorino/mtg-kernel/issues/568). **Reachable** means a pinned consumer can receive the affected input and its roster admits a witness. **Ruled out** means the named path or trigger is excluded by the pin. Actual encounters, choice changes and effects on results are **unknown**. CP7 remained sealed.

## Paths

- **#567, native policy input.** A same-name reveal is keyed by physical object in `GameState::reveal_hand_card`. Ninjutsu exposes the hand source while the activation is pending. The historical observation can distinguish a known Ninja being revealed again from a second Ninja being revealed, even though the visible history does not establish that distinction. FlatV4 includes the opponent's known-hand rows as `KnownOpponentHand` objects and includes the canonical observation in its state digest. Spellbench's native scorer reads the FlatV4 tensor. The #548 opt-in covers departures and leaves this reveal case reachable. All engine pins below predate #548. [Hand knowledge](https://github.com/jackmaiorino/mtg-kernel/blob/3b19241fc/mtg-kernel/src/state.rs#L3039) · [Paid activation](https://github.com/jackmaiorino/mtg-kernel/blob/3b19241fc/mtg-kernel/src/engine.rs#L28902) · [Flat observation rows](https://github.com/jackmaiorino/mtg-kernel/blob/4fccd3ebc/mtg-kernel/src/flat_policy_v2.rs#L2621) · [Scorer](https://github.com/jackmaiorino/mtg-kernel/blob/92ec2393/mtg-kernel/src/spellbench_scorer_v1.rs#L389)
- **W1 search export is a separate input.** Amendment 43.8 D supplies hand size, fills unknown occupants from the pool and omits the reducer's opponent-hand names and survivor allocations from production `P`. That rules out #567's extra native row entering the sampled hand through those claims. The live bridge tensor still has the native rows. The W1 `_policy` fallback and the opposing b512 scorer can consume that tensor. Within a composition walk, `_point_policy` scores the local point tensor. Amendment 43.9 adds masked digests for G6 comparisons and leaves the scoring tensor intact. AC0-P reruns, exchange equality and I1–I4 establish the recorded checks on the pinned build. They give no two-world comparison of the unmasked native tensor for #567. [Production hand allocation](https://github.com/jackmaiorino/mtg-kernel/blob/4fccd3ebc/mtg-kernel/src/infoset_v1/export.rs#L1079) · [Masked digest](https://github.com/jackmaiorino/mtg-kernel/blob/4fccd3ebc/mtg-kernel/src/agent_bridge_flat_v4_encode.rs#L152) · [W1 policy fallback](https://github.com/jackmaiorino/spellbench/blob/27c9009bb/integrations/mtg_kernel/kernel_search_bot.py#L244) · [Local point scorer](https://github.com/jackmaiorino/spellbench/blob/27c9009bb/integrations/mtg_kernel/kernel_search_bot.py#L539)
- **#568, face-down departures.** `hand_move` uses the real definition for a named departure, including a cast passed as named by the `SpellCast` branch. Production `P` ignores the survivor allocation but still reads named seams when checking live references. A future export pool with face-down hand departures therefore needs a two-world check of refusal as well as payload. The historical native rows also have the face-down departure gap described in #568. The pinned deck pools exclude the known triggers. Beyond the issue's Disguise and foretell examples, current source includes Exalted Angel's Morph cast from hand. Include Morph in checks for an expanded pool. [Departure and seam recording](https://github.com/jackmaiorino/mtg-kernel/blob/4fccd3ebc/mtg-kernel/src/infoset_v1/projection.rs#L1327) · [Production seam check](https://github.com/jackmaiorino/mtg-kernel/blob/4fccd3ebc/mtg-kernel/src/infoset_v1/export.rs#L737) · [Morph fixture](https://github.com/jackmaiorino/mtg-kernel/blob/3b19241fc/mtg-kernel/tests/eternal/eternal_premodern_p1a_v1.rs#L1418)

## Evidence by run

The native nine-deck roster has the same Git blob `c195edd4666b07471b81da58938bb451073f5fad` at `92ec2393`, `dc6aa3c3`, `4fccd3ebc`, `b0061fa5d` and `c6f744445`. Faeries contains four Ninja of the Deep Hours. The roster has no Flourishing Bloom-Kin, Behold the Multiverse or Exalted Angel. The eight-deck W2 and W1 cohorts include Faeries and exclude those three cards. The W2 rated manifest also lists the admitted card-name domain. Hashes below are SHA-256 prefixes unless identified as source commits. The b512 checkpoint is `8a52be51` and its model state is `662aad1a`.

| Run or artifact | Observed engine and model pins | Exposure |
|---|---|---|
| **b512 OH2 lineage and W2 rated b512** | OH2 trainer `49aa4d21` at `dc6aa3c3`. W2 `pauper-ladder-v1`, 2026-11-01, pins bridge `275c23d1` and scorer `ae5af94a` from `92ec2393`, catalog `af8e1b91`, and b512 `8a52be51`. The rated manifest enables `x_kernel_flat_v4` and `x_public_history_v1`. Sampled and argmax use the same weights. [W2 rated manifest](https://github.com/jackmaiorino/spellbench/blob/404c4c0a/benchmarks/pauper-ladder-v1/snapshots/2026-11-01/manifest.json) | **#567 reachable** in native OH2 FlatV4 collection and the W2 scorer input. **#568 ruled out** for this roster and W2 has no infoset export enabled. |
| **Ordinary chains A, CP and CS and their raw-policy look panels** | Trainer `699e1e08` and evaluator `b4f1c792` both built at `4fccd3ebc`. `eb005f09` pins the chain tools. A and CP bind b512 through dose record `608c0818`. CS binds S0 initial weights `6ae9b265` through `7f9e4122`. Panels use the nine-deck raw policy evaluator, with b512 and H references. `collab@c6b5c513`, `LANES/chains-20261107/{RUN-MANIFEST-node1.json,RUN-MANIFEST-node2.json,DESIGN.md,PANELS.md}`. Trainer source binding is in `collab@30ce14f9`, `LANES/lead/w5/nodes-20261106/STAGE0-NODES-20261106.md`. | **#567 reachable** in FlatV4 collection and in the native policy path used by the look panels. **#568 ruled out** for this roster. |
| **Chains A@65k unrated Spellbench read** | The six endpoint identities are pinned individually. Entrants reuse the W2 bridge `275c23d1`, scorer `ae5af94a`, catalog `af8e1b91`, and `sampled-wide-v1` against the board's b512. `collab@c6b5c513`, `LANES/chains-20261107/spellbench/u0065536-A/RUN-MANIFEST.json`. | **#567 reachable**, same native input as W2. **#568 ruled out**, same roster and extensions as W2. |
| **W1 AC0-P and frozen soak** | Kernel `4fccd3ebc`, bridge `3338b4d8`, scorer `b7deed4d`, census pool `9321631a`, Spellbench `27c9009bb`, catalog `af8e1b91`, and b512 `8a52be51`. Pins in `collab@31a1819c`, `LANES/lead/w1/composition/tools/soak/manifest-template-ac5-16k-k4.json`. AC0-P receipts in `collab@14de90c2`, `LANES/lead/w1/composition/AC0P-RESULT-20261107.md`. | **#567 reachable** in the live native scoring path, with the direct known-hand-claim path into search sampling **ruled out**. **#568 ruled out** for this roster. The general export code retains the latent named-seam path. |
| **W5 Stage 0b and S0 store** | Stage 0b capture `3ffa7f59` at `4fccd3ebc`, b512 `8a52be51`, both node inputs verified. S0 self-play recorder `c148a068` at `b0061fa5d`, b512 `8a52be51`. `collab@30ce14f9`, `LANES/lead/w5/stage0b-binding-20261107/{AUDIT.md,README.md}` and `LANES/lead/w5/s0-evidence/RUN-MANIFEST.json`. | **#567 reachable** in the native b512 policy during capture and in the V4 tensors recorded for S0. The Stage 0b target audit verifies perspective, indexing and the gain calculation. It does not address #567. **#568 ruled out** for this roster. The frozen Stage 0b stop stands. |
| **W7 data scaling** | Recorder `8b3cf913` at `c6f744445` on base `a0ea359a`. The twelve D1 b640 teachers, play imports and checkpoint hashes are in the live `record/teachers.json` (`ba8b6bd8`). The offline fits use the actual V4 tensors the teachers scored. `collab@55ca8ad3`, `LANES/bignet-pilot-20261031/{DESIGN.md,DATA-SCALING-DESIGN-v1.0.md}`. | **#567 reachable** in both the policy that generated the games and the recorded input used for fitting. **#568 ruled out** for this roster. |

### Full pin identifiers

```text
Git commits
3b19241fcc4cbe10a836eb63958fbde83bf76de7  mtg-kernel audit baseline
92ec2393db34bea805a85991163458b9ff890f57  mtg-kernel W2 and A@65k Spellbench binaries
dc6aa3c3c252a5e552102fb9aa5e6dd8ac03cbcd  mtg-kernel OH2 trainer
4fccd3ebc6f119a4746d8ce8b85d3fe3c8481d0c  mtg-kernel W1 binaries, chains trainer/evaluator, W5 Stage 0b
eb005f097f319e48c50ee122df152245f9a213bd  mtg-kernel chains tools
b0061fa5d5dd7cd680191975f5ed15aa1b29c578  mtg-kernel W5 S0 recorder
c6f744445a4d150e48765f4f3db9914689bc5ac0  mtg-kernel W7 recorder
404c4c0a342a462dc3e27d024cd942c38a7ef232  Spellbench A@65k checkout and W2 rated artifacts
27c9009bb302e9cd1b945daeeedc2b936953c5aa  Spellbench W1 frozen checkout

SHA-256
49aa4d21e6f5a0409eafa0d0975f87e937b7ec12202e52c3dda8304c7a7d2dcf  OH2 trainer
275c23d189c99b0edeb5409c7cac343eb1bd28aee9578e465d08bd133dde7297  W2 and A@65k bridge
ae5af94ab594c393f10371593706b654ab39b90ac82c0b27d682f0b6f1d3841d  W2 and A@65k scorer
af8e1b914d76ed6d30318403c6d434fc4d3a651a68d7e37c79c9d09c68cc457e  W2, A@65k and W1 catalog
699e1e08c697fb52c4714f43b45624289fcd9ce896a676bd9843d57a088d0e66  chains trainer
b4f1c792c169a12b250063cc0cd4817930b01a1a7475e6a0ee6db72d53a51d51  chains panel evaluator
3338b4d809a3eada3f6abffd21a8085491ba42500ea0d6caeccad98871cfae03  W1 bridge
b7deed4df80431f446176d20f469449f1dbf133caa7d6de3c14c7db9840ebb76  W1 scorer
9321631ad8c3e931737b8f57fcd8479389c0255b5fabb97d3c4e475a94f2509c  W1 census pool
3ffa7f5999586c338ea34242a9769ab58d1243d5eb696781e086fabc673abbd9  W5 Stage 0b capture
c148a068545bdbcfa8ec460b822578e627826de4aaaa28d2ef2b055772624c02  W5 S0 recorder
8b3cf913fca07e3fd2a4e8088f34201595af848e982336448a1c751c41f701ff  W7 recorder
ba8b6bd87241552aab77629b508c7bfe970aa7a69fa196d237b9c84168489b3b  W7 teacher manifest
8a52be51b92d9f7bbca211c5f0a20c7f291c1c78a74f1dd83fb71c1cb27c8837  b512 checkpoint
662aad1a2c01089f4b5405c0bee73f17a4c3e13c57afb989e56fc21a0c269934  b512 model state
608c0818f73ce0b87564be230f08d48d34f5b23ff131e16bba06b5b5921c1e3f  remapped b512 dose
7f9e4122b9f9b42972b59cd7d24d51ac3b37d0900778553196e4c79d205c5ce8  remapped S0 dose
6ae9b2657032cb04e47484a68e5a97b861e48f41278d9b13c680c11d934d7b88  S0 initial weights
```

## Minimum follow-up checks

1. Run a two-world Ninja witness on an explicitly versioned native policy surface before a new fair run. Keep the observer's visible history and legal menu identical, activate the known copy in one world and the other copy in the second, and compare the opposing known rows and aligned FlatV4 inputs both while the ability is pending and after it resolves. Include positive controls for whole-hand reveals, distinct known arrivals from a search and a public same-name departure. Verify that the historical default still reproduces its pinned bytes.
2. If measuring impact is needed, begin with one fixed non-holdout decision from the witness, one pinned b512 checkpoint and one scorer on the old and new observation modes. Compare aligned logits and fixed-seed choices, then use matched non-holdout games only if the decision warrants them. Record the observation version in any new benchmark identity. Existing aggregate ratings, training results and reads stay attached to their original pins.
3. Before enabling infoset export for a roster with hidden hand departures, run two-world tests for Disguise, Morph and foretell that compare both export bytes and refusal subclass. Check native rows separately. Reuse the existing W1 witnesses for the pinned Pauper roster and keep G6's masked comparison distinct from the full scoring tensor.
