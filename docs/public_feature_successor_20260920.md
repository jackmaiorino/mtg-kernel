# Public feature successor: engineering qualification

The corrected runtime exposes active Strands prevention, but the existing model has no dedicated global prevention-color inputs. Visible card rows also lack explicit printed mana costs. This successor tests an additive representation without changing frozen V4/Python V7 identities or the reference checkpoint.

Implemented in this isolated worktree:

- Six public state fields: five active prevention colors and a separate damage-cannot-be-prevented flag.
- Thirty-two fields for each already-visible card token: known card, cost presence, front-face mana value, generic/X counts, colored/phyrexian pips and unordered hybrid pairs. These are printed costs, not actual payment or alternate costs. Hidden hands and libraries never supply tokens.
- Independent Rust compiled-cost and Python printed-cost encoders with a separate versioned contract.
- A Python research model that adds two zero-initialized input projections. Existing parameter names, shapes and accumulation order remain intact. Imported Adam ages remain 32400; the two new matrices start at age zero.

Evidence: `E:/mtg-postboard-campaign-20260920/public-feature-qualification-001`.

Native catalogue and actual V4 fixture tests passed. Twelve fixtures cover both seats, three shield configurations and two hidden-hand/library/allocation variants. The Python qualification in `python-002/qualification.json` passed all 192 catalogue rows, all thirteen base arrays and auxiliary rows. Imported parameters and both Adam moments are bit-identical to g115. Initial Python reference/successor logits and values are bit-identical. Both new matrices receive nonzero gradients; two synthetic updates reproduce parameters and Adam after save/resume. Qualification took 3.42 seconds on torch 2.13.0+cpu.

The earlier `python-001` qualification stopped before checkpoint transport because raw native action semantics lacked the Python legal-action envelope. The runner now constructs explicitly diagnostic operational wrappers; all native/Python tensor comparisons pass. No frozen encoder was modified and no measurement was rerun. Preserve both roots. A subsequent boundary check rejects i64::MIN tokens without arithmetic overflow.

No terminal-reward training, native successor scoring/update integration, CUDA optimizer parity, stronger candidate or human-strength result follows from these tests. The synthetic loss is only a gradient-connectivity probe. Existing human delivery and g115 checkpoint remain unchanged.

The subsequent sections record native scoring and CUDA optimizer qualification. The next learning comparison must use the corrected observation runtime, matched seeds and terminal rewards in both arms. Keep canonical retention checks and independent opposition; do not select from CP7 outcomes.

Fable's September 19 consultation failed HTTP429 before source reads, with reset September 22 at 07:00 EDT. No retry or endorsement is implied. Bounded local work proceeds under Jack's explicit authority with this review gap unresolved. No paid compute or broad training campaign is launched.

## Native scorer and complete warm-start games

The native successor now implements the two input projections in the actual Net8 forward calculation. Legacy callers take the existing arithmetic path with no public projection. A distinct `NativePublicInputNetV1` wrapper and `PublicInputPlayPolicyV1` adapter accept only V4 inputs and derive both tensor and public observation from the same bound legal decision. No legacy checkpoint loader automatically admits the successor. The adapter uses the existing per-seat RNG and legal-action sampler exactly once per decision.

Evidence root: `E:/mtg-postboard-campaign-20260920/public-native-forward-001`.

| Check | Result |
| --- | --- |
| g115 native/Python forward, zero/object/state/both projections, 12 fixtures each | Pass, maximum absolute score/value difference 0.000003815 within the fixed 0.001 absolute plus 0.001 relative envelope |
| Native zero projection versus native legacy scores | Bit-identical |
| Hidden-card/allocation pairs under all four projections | Bit-identical scores |
| Nonzero projections connected | Object changed 12/12 fixtures; state 8/12, both 12/12 |
| Malformed/nonfinite weights, old schema, wrong registry | Rejected |
| Corrected-runtime Gates/Affinity full BO3s, both physical seats | All 698 decisions, starts, seeds, game winners and outcomes match baseline |
| Same first match in a fresh native process | Byte-identical output, SHA256 `9e17fc22493ef2f029517bdc6d725b17996797487b525f89a9f778f7d5ed0d08` |
| Existing model/policy regressions | 22 passed, one pre-existing ignored |

Two unique BO3s/four games; three executions/six games including replay. Match tests took 13.43, 9.52 and 13.25 seconds, excluding builds. Debug build times were 43.17 and 44.51 seconds. Both models' added projections were zero in the complete games; no nonzero full-game or learned-model claim follows. The separate numerical probes used fixed signed binary-fraction matrices, not outcomes or training. All runs finished successfully. Existing g115 human delivery remains untouched.

These scorer checks preceded the CUDA work below. The existing `training.rs::ExperimentalDeviceTrainStateV1` assumes 33 tensors and one Adam age, requiring separate successor state rather than admitting the new matrices under its old identity.

## CUDA update and optimizer-state transport

The CUDA core is now implemented separately in `training/public_inputs.rs`. It reuses the established GAE loss and legacy Adam mapper, while the two public matrices have separate moments and a shared new-parameter age. Both projections use separate pre-tanh matmuls. The original forward path passes no additions. Updates prepare the legacy and public candidates before publishing both ages and parameter sets together.

`training/public_inputs/snapshot.rs` serializes all parameters, both moments and both ages under `mtg-kernel-public-input-optimizer-state/v1`, with exact V4, registry and auxiliary contract identities. It validates the legacy state hash and projection dimensions, finiteness and nonnegative second moments. This is optimizer-state transport, not yet a complete campaign checkpoint containing rollout progress, seeds and loss configuration. Existing loaders and the existing 33-tensor optimizer are unchanged.

Final evidence: `E:/mtg-postboard-campaign-20260920/public-cuda-qualification-002/qualification.json`. Earlier `public-cuda-qualification-001` is preserved; it passed the original 12-fixture check before the dummy-card guard was added. The same fixed numerical gates apply in both roots.

| Qualification on physical GPU 1, RTX 3050 | Result |
| --- | --- |
| Exact import of g115 parameters, moments and ages | Pass |
| Initial CUDA legacy/successor logits and values | Bit-identical |
| 35 gradient tensors versus Python | Pass, maximum absolute difference 0.0000023842 |
| Two updates versus Python | Pass, maximum parameter-delta difference 0.00000005961 |
| Both moment sets versus Python | Pass under the fixed absolute/relative envelopes |
| Fresh native process restores step 1 and reproduces step 2 | Byte-identical complete optimizer-state snapshot |
| Dummy-card raw gradient is nonzero, but padding parameters and moments stay zero | Pass, raw gradient L1 18.3531 |
| Invalid projection moments/ages and mismatched/corrupt snapshots | Rejected |
| Existing CUDA empty-relation Adam continuation regression | Pass |

The final ages are 32402 for legacy parameters and 2 for the new matrices. The repeated state SHA256 is `5c1686a9a1063c8f48a62f7d39615e103804aaf7eecfd8121e2020a5603d41dc`. The dummy guard preserves Python's `Embedding(padding_idx=0)` derivative; Burn's embedding operation has no padding parameter. This correction is confined to the new training path. A later matched control must share it with treatment.

The final start suite passed three tests in 18.77 seconds; the fresh-process resume passed in 15.74 seconds; the legacy CUDA regression took 12.21 seconds. The final build took 46.40 seconds, after the first CUDA test build took 2m26s. These are debug qualification timings, not production training throughput. The actual targets and advantages were fixed synthetic numbers on real visible fixtures. No episodes were trained, no new candidate was selected, and no playing-strength result follows.

The remaining implementation is the real collection/update loop: record auxiliary rows alongside each learner tensor from the same bound decision, derive GAE from terminal rewards with the existing grouping, install updated legacy/public parameters into the native play adapter, and persist complete iteration/resume provenance. Add a bridge that validates each V4 view and per-decision row alignment before packing and bounds chunk memory. Then qualify a few real updates and resumed rollout replay before freezing and launching a matched learning pilot. Do not treat the low-level CUDA core's synthetic qualification as that end-to-end qualification. Fable review remains unavailable as recorded above.
