# Per-instance public stack messages

Owned branch `codex/public-stack-features-v1`, isolated from research source `0c1c0e57`. Previous balanced, entropy and prevention measurements remain unchanged. Fable's known review attempt failed with zero reads and HTTP429 until September22 07:00EDT; the independent design review is still missing. This bounded implementation proceeds under Jack's research authority, without a training launch, promotion or claim of strength.

The existing tensorizer retains stack payloads in canonical hashing but omits baseline stack relations and player-target edges from numeric relations. Adding fields only to current target-edge rows would miss untargeted kicked spells. The new encoder derives unchanged legacy V4 tensors plus one auxiliary baseline per public stack item and a row per public target. It uses the same raw-object-to-tensor-node projection as the actual V4 encoder. Distinct same-card spells retain distinct source nodes; repeated abilities from one permanent retain distinct public stack positions. No hidden state or card-token join is used.

The versioned312-column contract explicitly represents kind, cast method including absence, copy/flashback/madness/kicker, all256 mode values, exact X/face bits, public order, target kind and announcement-time target controller. It does not truncate stack/target counts. A separate learned matrix maps each row plus its source/target object embeddings through tanh, then adds the message to the existing node update. Zero weights give exactly zero messages and preserve the legacy graph dimensions. The native path skips addition of exact zero to preserve signed-zero values. This does not silently enable cost/prevention inputs or alter old architecture/loader identities.

Initial checks cover actual engine-created Ward stacks with duplicate Lightning Bolt sources and repeated Ward-source abilities, both seats, different hidden cards/library order and arena allocation; exact V4 tensors/legal references, zero-projection logits/value parity and deterministic nonzero forward; deliberately counterfactual public metadata to test full X/mode encoding and per-instance alignment; and real kicked/unkicked Goblin Bushwhacker casts with no targets. These are engineering checks, not tactical-quality or playing-strength evidence.

Current scope includes the extractor, scalar model, independent CPU reference, CUDA grouped update/optimizer transport and actor-bound gameplay adapter. Full trajectory/trainer integration, a permutation control and useful-compute qualification remain required before a learning comparison. The device implementation preserves per-item nonlinearity before pooling. No experiment gate or coefficient is selected by this document.

## September 21, 07:48 initial validation

Native source `be3ff42b` passes all five selected tests in `E:/mtg-meta-recovery-20260921/public-stack-engineering-002/test.log`, including the real g115 checkpoint (`88c0b997708c2b5156b44f3940ad9d5d682f78ac24d346978bb3c9f34c59e8d1`). Four new stack tests and one existing human Ward test pass. The initial root001 is preserved: its two failures were fixtures permitting forced passes to another actor or terminal; adding a legal instant and checking actor identity fixed those fixtures without bypassing production validation.

| Check | Actual result |
| --- | --- |
| Zero projection, g115 native logits/value | Bit-identical to legacy on both seats and both hidden variants |
| Nonzero projection | Changes scores; repeated forward and hidden pairs remain bit-identical |
| Fresh native process | Entire g115 reference export byte-identical |
| Existing native model regression | 8 tests pass |
| Independent Torch reference, zero and nonzero | Maximum native/reference difference 0.00000190735, below declared 0.001 + 0.001 relative envelope |
| Projection gradient finite difference | Analytic 0.048748247307827; numerical 0.04874824730966009 |
| CPU Adam save/load continuation | Parameters and moments bit-identical; imported age32400 retained, new projection starts age0 |

Reference code: `python/tools/check_public_stack_reference_v1.py`. Its result is `public-stack-engineering-002/versioned-python-reference.json`; native replay/regression receipt is `public-stack-replay-001/completion.json` under the same September21 evidence root. Native executable SHA256: `ecb72609c864f488d2c0454944e3f703ea6a384f116c8c9a841ad8baa3012664`. The reference uses a synthetic mathematical loss solely for gradient/optimizer checks; it is not a training reward or experiment. The derivative check is one strongest derivative on one fixture, not an exhaustive native-gradient check. No GPU path, complete match replay, trained successor, tactical gain or human-level result is established here. Fable review remains unavailable as stated above.

## September 21, 08:16 CUDA and gameplay validation

`48afc254` adds CUDA stack messages, 34-tensor grouped updates and a separately versioned optimizer snapshot. The new matrix starts at Adam age0, while imported tensors retain age32400. Stack rows carry local source/target nodes into validated per-decision packing offsets. Empty stacks have no fabricated messages; their connected zero derivative multiplies weights by zero before reduction to avoid overflow. `2c66c78b` includes that finite-boundary fix, tested with every new weight set to `f32::MAX` on the empty auxiliary path.

`public-stack-engineering-005` passes all five selected native checks on the final Rust source. The g115 export now covers16 actual states: both seats, two hidden-state variants, and Ward stacks, player targets, untargeted kicker or empty stacks. It is byte-identical to the earlier expanded export.

| Check | Evidence and result |
| --- | --- |
| CUDA gradients and two Adam updates, 34 tensors | `public-stack-cuda-002/qualification.json`: PASS against independent CPU reference;16 decisions grouped into8 physical decisions |
| Chunked grouped update | PASS within frozen tolerances; maximum parameter difference 1.49e-8 |
| Native fresh-process checkpoint resume | Entire final optimizer snapshot byte-identical; legacy age32402 and stack age2 |
| Empty/control path | Zero new-weight gradients and moments; extreme finite weights preserve legacy outputs |
| Normal-update regression after empty-path fix | All four saved update snapshots byte-identical to root001, recorded in `prior-normal-update-parity.json` |
| Real gameplay adapter | `public-stack-gameplay-001/completion.json`:4 games,698 decisions equal archived baseline; fresh-process match replay byte-identical |
| Adapter semantics | Exact sampler stream, hidden-state invariance, captured-score replay, private collector forks, disabled control and stale-capture rejection pass;8 legacy model tests also pass |

CUDA execution uses only GPU1 and takes10.93seconds plus7.32seconds for fresh-process resume, excluding the265.75second build and CPU analysis. Gameplay was compiled at `3c15f9cc`, before the isolated GPU empty-path fix; its Rust policy/scorer sources are unchanged by that fix. The2 unique BO3 cases ran with2 local workers; the repeated case is replay evidence, not another independent match. No paid compute, reward-based learning, evaluation campaign or candidate selection occurred.

Reproducible helpers: `python/tools/qualify_public_stack_cuda_v1.py`, `analyze_public_stack_cuda_v1.py`, `check_public_stack_gameplay_v1.py`. These are bounded engineering checks, not substantial training launchers. No new standalone training launcher has been introduced. Existing guard requirements still apply before substantial work.

Next integration must retain immutable behavior-state capture, complete synchronous batches, deterministic episode ordering, terminal rewards, physical grouping, full optimizer state and crash-consistent publication. Add the standing-rule permutation control before the experiment: extra active model capacity is a competing explanation for any gain over a disabled-input continuation. A fixed column permutation is invertible and is not an adequate information-destroying control. The control needs an explicitly specified, reproducible decision-varying scramble, unchanged sampler RNG, public-only inputs, preserved feature magnitudes/graph bindings and native/CPU/CUDA replay checks. Record the information it still retains. Do not silently treat the current disabled-input flag as that control.

The collector/trainer, control implementation, throughput qualification, formal comparison against matched continuations, independent opponent coverage and actual human calibration remain incomplete. Fable's zero-read quota failure still leaves independent review unavailable. No playing-strength or league-readiness conclusion follows from these engineering results.
