# Phase 1 agent package and BO3 trajectory interfaces

These new interfaces bind an exact player and its auxiliary policies, validate complete match grouping, and derive match-normalized training weights. They do not implement a new optimizer, loss, match collector, search adapter or gameplay learning path. Existing models, opening serialization, BO3 outputs and checkpoint loaders are unchanged.

## Complete package

`CompleteAgentPackageV1` uses schema `mtg-kernel-complete-agent-package/v1`. It reuses `ExpandedSeatBehaviorV1`, including the actual installed model identity, checkpoint/state identity and optimizer step. `source_import` remains ancestry; a sideboard head must bind `identity.model.weights_sha256` and the actual embedding-table digest.

The runtime binds executable and toolchain files, engine commit, committed tracked-tree SHA/contract, clean-build status, card database/registry and feature identities. The package additionally records the actual gameplay sampler and explicit opening, play/draw, sideboard and search policy descriptors. There are no omitted-policy defaults. Changing any descriptor or file location changes the domain-separated package SHA-256; relocating a package does not change its component content hashes.

`validate_metadata_v1` checks internal identity joins. `from_json_v1` also rejects duplicate JSON fields. `AgentRuntimeIdentityV1::verify_current_runtime_v1` verifies the supplied executable file and independently hashes `std::env::current_exe()`. The bytes must match. It compares the recorded commit, tracked-tree SHA/contract and clean-build status with compiled values, and requires a clean build. It also compares compiled toolchain file bytes, card registry bytes and all current V3 feature identities. The tracked-tree digest identifies the committed HEAD tree, so matching that digest without the clean-build check would not establish which dirty source was compiled.

`load_supported_components_v1` requires that current-runtime check before loading model components. It then verifies the actual loaded gameplay identity using the existing strict continuation reader. For a learned sideboard head it verifies checkpoint bytes, the head's play identity and the actual frozen embedding table. A successful return carries a `CurrentAgentRuntimeV1` value with private construction fields. Checking a foreign executable's valid file pin at the same HEAD cannot produce that value.

Supported component loading currently means corrected `KeepSevenV2`, fixed Play/Draw, disabled search, and Keep or the existing greedy learned sideboard head. Learned London/play-draw descriptors and the older native-search descriptor are representable but explicitly rejected by the component loader. They cannot silently fall back to fixed policies. The package does not yet supply a complete match-routing executable. A serialized receipt about a remote or external producer is not current-runtime verification of that process.

The full package identity is specific to a machine's artifact locations and a selected interface/runtime executable. Two packages can contain the same gameplay model, head content and policy settings while their full package hashes differ. Compare the actual `gameplay.identity.model`, head checkpoint content hashes and policy settings when identifying reused agent content. That comparison does not certify equivalent game behavior across different engines, interfaces, builds or operating systems. Such parity remains a separate measured qualification. The toolchain comparison binds the compiled `rust-toolchain.toml` bytes; exact executable identity also binds this binary, but this interface does not independently attest compiler/linker executables or build-machine provenance.

## Actor-visible trajectory

`Bo3TrainingTrajectoryV1` uses schema `mtg-kernel-bo3-training-trajectory/v1`. Match metadata holds physical seats, both registrations, package digests and ending status. This entire record is never a policy input. `Bo3DecisionRecordV1::model_input_v1` returns only the actor-specific decision value.

The actor-specific variants cover play/draw, mulligan, conditional bottom-card selection, sideboard deliberation and V6 gameplay. They contain the actor's own cards and legitimately visible evidence. There are no opponent registration identifiers, full opponent lists, environment seeds, hidden library order, raw `GameState` or diagnostic state hashes in these input variants. Rich V6 references are transport authority and still require the existing V3 tensorizer before scoring; arena identities must not become model features.

`gameplay_from_session_v1` obtains the observation and ordered actions from one validated current actor binding in `FastActorSessionV1`. Sideboard collectors must use the existing `project_sideboard_input_v1` over the actor's completed visible summaries. A future opening collector must project actual actor-visible opening state. The existing human-opening API submits bottom cards together; the new trajectory represents that order as conditional choices, and no conversion or behavior probabilities are fabricated here.

The validator checks:

- Identical runtime/feature contracts across seats and one fixed behavior package per physical seat for the whole match.
- Executable exact-60/15 registrations through the existing registration check. This v1 interface does not implement wider legal deck sizes or assert current format/ban legality.
- Contiguous match decision indices and physical game indices, including natural draws that extend a match beyond three games.
- Both sideboard submissions before the next play/draw choice, using the existing `SideboardDeliberationStateV1` for exact legal-action order, card conservation and Done.
- Sideboard scores and evidence restricted to the completed prior-game prefix. Nonfinite resource summaries are rejected.
- The BO3 state machine's actual chooser, Play/Draw result, opening phases, mulligan counts, hand membership and conditional bottoming sequence.
- Both openings before gameplay, actor and card-contract consistency, gameplay step counts, natural terminal classifications, and first-to-two match results.
- No later game after completion, no continuation after a capped/halted game, and no match target for incomplete or capped trajectories.

Validation cannot prove that a deserialized producer told the truth about visibility, selected actions, probabilities or game outcomes. An external document can put invented cards in a nominally visible field. Trusted native production, pinned artifacts, and replay/evaluation evidence remain necessary. These checks are structural engineering evidence.

## Behavior probabilities and weights

`BehaviorDistributionV1` distinguishes deterministic selection, a normalized finite categorical distribution, and the native sampler's exact Hamilton Q64 masses. A greedy decision has probability one even if an unused softmax assigns another value. Q64 masses use canonical decimal strings to preserve the full `2^64` total through JSON. `hamilton_from_logits_v1` reuses the existing wide sampler apportionment. The collector must record the actual behavior selection and post-search distribution, not an unrelated network distribution.

`validate_v1` returns a `ValidatedBo3TrajectoryV1` only after structural checks. `equal_match_weights_v1` accepts those values plus one learner seat, one learning component and the expected frozen behavior-package digest. It excludes incomplete matches, rejects duplicate match IDs and stale learner packages, and never assigns weights to the opponent or another component.

If N complete matches contain at least one selected learner/component decision, every such match has total weight `1/N`; each of its K selected decisions receives `1/(N*K)`. Matches without that component contribute zero and are excluded from N. Returns are derived as +1/-1 from the natural match winner. There is no supplied target, optimizer call or gradient application. Longer matches therefore do not receive more total loss weight through this helper.

## Verification and remaining integration

Source tests cover exact package roundtrip/digest changes, actual-versus-ancestry head binding, feature mismatches, duplicate JSON/hidden fields, unsupported adapter rejection, probability/Q64 errors, two/three/four-game structure, wrong actors/roles/stale packages, London bottoming prefixes, sideboard omissions/future evidence, caps and incomplete exclusion, duplicate matches, and equal match weighting. Additional runtime tests reject an honestly pinned alternative executable at the same HEAD, reject a mismatched tracked-tree identity, and require a clean compiled source even when executable bytes match.

Root owns adding `pub mod phase1_agent_v1;` to `lib.rs` and running `cargo test -p mtg-kernel --lib phase1_agent_v1` through the bounded build wrapper. No Cargo, native engine, GPU, training or paid compute was started by this implementation lane. Synthetic outcome sequences in the tests exercise validation; they are not playing-strength results.

Root integrated the module and all17 focused tests passed on September13,2026 in `C:/Users/Jack/IdeaProjects/sideboarding-integration-20260912/phase1/agent-package-unit-002.log`. The current-executable mismatch, dirty-build and tracked-tree checks passed. A fresh Fable interface review731282ae-b3e7-4dfd-ad1f-a278a82a2530 failed at its weekly quota before source reads. Root's source review changes were accepted, but there is no Fable endorsement or full-agent playing-strength result.

Remaining work includes a real BO3 trajectory collector/writer, opening and stochastic sideboard policy execution, auxiliary-policy learning, alternating match-return training, search compatibility, portable packaging and human/evaluator routing. These interfaces do not alter frozen experiment measurements or establish mastery of any deck.
