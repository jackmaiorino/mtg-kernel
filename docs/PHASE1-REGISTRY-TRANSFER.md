# Phase 1 explicit registry transfer

The transfer library creates a validated in-memory candidate from a real `mtg-kernel-expanded-deck-checkpoint/v1` and the registry compiled into the current executable. It preserves learned state while assigning embeddings to appended cards. A separate, explicit trainer source now supports bounded continuation from that candidate. Neither interface starts work merely by loading a module, and ordinary checkpoint loading remains strict. Source tests are engineering verification, not playing-strength evidence.

## Actual Net8 layout

`NativePolicyValueTrainSnapshotV1` contains 33 ordered named parameter tensors, matching first and second Adam moments, an Adam step and the canonical scorer-bias anchor. Net8 already allocates `card_embedding.weight` as `[65537, 16]`; token zero is padding and a card uses `card_db_id + 1`. Adding registry entries within the admitted `u16` card-ID domain does not resize the model.

The transfer uses `native_train_state_parameter_layout_v1`, the native state validator and hash, and `NativePolicyValueTrainStateV1::from_snapshot_v1`. This is an actual Net8 checkpoint operation, not a generic array migration. The existing source schema stores learning-rate and value-coefficient bits beside the snapshot. Adam beta1, beta2, epsilon, weight decay and canonical gauge have fixed native V1 identities; the receipt records those exact values.

| State | Transfer behavior |
|---|---|
| Existing card embedding rows, including padding | Copy every parameter and moment bit |
| All other named tensors and moments | Copy every bit, preserving feature-column positions |
| Still-unassigned vocabulary rows | Copy every bit |
| Appended card rows | Deterministic name-and-seed initialization; positive-zero moments |
| Adam step, scorer anchor, loss identity, learning rate, value coefficient | Preserve exactly |

An appended row with a nonzero old moment is rejected. This prevents a purported new-card mapping from silently erasing existing optimizer learning. The previously unassigned parameter row is intentionally replaced only for cards recorded as appended. Initialization uses SHA-256 of its versioned algorithm identity, an explicit seed, the exact card name and embedding column. A signed 24-bit integer divided by `2^28` gives exactly representable values in `[-1/32, 1/32)`, independent of host RNG, card order or registry length. The existing global Adam step continues for new rows too; no separate per-row bias-correction clock is introduced.

## Inputs and output

Call `transfer_expanded_checkpoint_to_current_registry_v1(checkpoint_bytes, source_registry_bytes, request)`. The request supplies exact source checkpoint/registry/state hashes, the expected Adam step and engine card-db hash, current destination registry/card-db pins, the unchanged V3 feature identity and initialization seed.

The source checkpoint must bind the supplied registry through its existing `source_import.destination_registry_sha256`, destination card count and card-db identity. Its state hash is recalculated from the real native snapshot before any candidate changes. Original import ancestry and trajectory pins are retained, not relabeled as the new runtime identity.

The destination is the current build's embedded `cards_v1.json`, cross-checked against `CARD_DEFS` in actual card-ID order. Existing card records must be an exact prefix, including deck membership and all metadata. Duplicate JSON object keys, duplicate names, removals, reorderings, renames and changed definitions are rejected. The first version intentionally does not support even explicitly reviewed shared-definition revisions; a future API must encode that distinct policy.

Feature dimensions alone do not establish compatibility. The source and destination must have exactly equal V3 contract, encoding, Python source and descriptor digests. The API binds current feature-schema and registry-version labels too. Feature additions, removals, reordering and semantic changes are unsupported; all existing columns retain their exact stable meaning and index through these identities.

`VerifiedRegistryTransferV1::artifact_bytes_v1()` returns a new `mtg-kernel-expanded-registry-transfer/v1` envelope, not an old-schema checkpoint. It contains the request, full parameter/moment bits, original provenance and a receipt with every card mapping, scalar setting, initialization rule and source/destination state identity. The current default expanded trainer rejects this new schema.

`verify_registry_transfer_artifact_v1` takes the artifact hash and preserved source checkpoint/registry bytes, reproduces the transfer and requires equality of all state and provenance. Artifact edits remain invalid even if someone updates only its outer byte hash. Runtime restoration uses the real native model and optimizer invariants. The crate-only `into_train_state_v1()` returns the validated state together with its scalar settings to the explicit trainer initializer.

## Explicit trainer continuation

`ExpandedModelSourceV1` retains its original three fields and its original serialized `checkpoint: null`. Its SHA-pinned `play_import` may explicitly point to an `ExpandedRegistryTransferSourceV1` document with schema `mtg-kernel-expanded-registry-transfer-source/v1`. That document pins the original expanded checkpoint, original registry, transfer envelope and a continuation schedule. Ordinary import documents still select the original loader. An envelope put directly in `checkpoint` does not opt in.

The schedule has schema `mtg-kernel-expanded-registry-resolved-schedule/v1`, the restored initial Adam step, exact learning-rate and value-coefficient bits, and ordered batches of existing `ExpandedEpisodeV1` values. No schedule is generated automatically. Every opponent must already be resolved to an exact model source, or use the existing `None` meaning same-current-model self-play. Dynamic historical references such as `CompletedIteration` are explicitly unsupported and rejected before collection. This is a bounded continuation interface, not the broad curriculum runner.

The initializer reproduces the transfer, builds the V3 gameplay policy from its actual installed Net8 parameters, copies the matching embeddings used by sideboarding, and binds current registry identities. Original export fields remain ancestry; `actual_model_identity_v1()` identifies installed weights. The runtime policy uses the distinct `mtg-kernel-registry-transferred-play/v1` identity with the transfer-envelope hash. The original ancestry remains intact in the immutable envelope.

Serial and parallel collection compare the exact ordered batch, including episode IDs, seeds, seats, decks and resolved opponent sources, with the pinned schedule at the restored cursor before publishing or collecting. Updates require identical scalar bits before ingesting trajectories and validate the same ordered schedule before model replay or learning. No changed feature contract or stale behavior state is accepted.

An update publishes `mtg-kernel-expanded-deck-checkpoint/v2-registry-transfer`, preserving the existing top-level parameter, moment, trajectory and scalar fields and adding required `registry_transfer` provenance. This binds the source descriptor, original checkpoint/registry, envelope, schedule, transfer-state hash, original Adam step, completed-update count and consumed-batch hash. Reload requires exact provenance equality, the same scalar bits and `current_adam_step = transfer_adam_step + completed_updates`. Further updates preserve that origin and advance by exactly one. An exhausted schedule permits inference but rejects further collection or updates.

The ordinary V1 checkpoint reader requires its original schema and rejects any transfer-metadata field, including explicit null. Its serialization continues omitting the new optional internal field entirely. Tests cover the old round trip as well as two actual native updates through the new command, immutable publication and reload path. Root ran the four new trainer-wiring tests successfully; they use actual actor-visible decision tensors with the existing explicitly synthetic terminal fixture.

## Verification and limitations

Focused source tests exercise real CPU Net8 forward, backward and Adam with small synthetic V3 decisions. They compare every shared parameter and moment bit before transfer and after the next update, preserve a non-default scorer anchor, verify identity-transfer continuation, learn an appended card without resetting the step, reject learning in a purportedly unassigned row, and reproduce an artifact before rejecting tampering. These are engineering tests; no training campaign or cloud allocation belongs to this module.

The transfer operation itself accepts existing expanded V1 checkpoints only. Legacy Store V2/V3/V4 payloads, wide Net8, auxiliary sideboard/opening optimizer states, a second registry migration of a transferred successor, different objectives, feature migrations and changing shared card definitions require separate interfaces. The trainer's pinned schedule supplies the new explicit continuation cursor and episode seeds; it does not recover an absent historical curriculum or collector RNG state. It does not certify correctness of newly implemented cards or external build history.

`KERNEL_CARDDB_HASH` includes generated rule behavior from `build.rs`, not simply the JSON registry bytes. The receipt binds the declared source and compiled destination card-db hashes, but cannot infer or certify behavioral equivalence between those engines. Caller-provided historical hashes identify preserved inputs; they are not an independent provenance audit. The destination build HEAD is recorded for context, not presented as a clean-tree or executable attestation.

Dynamic historical schedules, automatic curriculum construction and cloud manifest relocation/qualification remain separate integration work. The pinned descriptor and schedule are part of checkpoint provenance; rewriting them is not an ordinary resume. No default training objective, registry, existing agent package or frozen measurement changes in this deliverable.

Root ran all eight focused Rust tests successfully on September 13 with four BelowNormal build jobs. The receipt and log are `C:/Users/Jack/IdeaProjects/sideboarding-integration-20260912/phase1/registry-transfer-unit-001.{json,log}`. Independent read-only source review found no material preservation or mapping defect within this scope. Fresh Fable session `40d42c80-ab8c-4a5f-825d-066c7b11add3` failed at the weekly limit before source reads or feedback. Root accepts this opt-in engineering API under the user's explicit implementation assignment with that review limitation recorded; this is not Fable endorsement, actual campaign migration, or production activation.

The completed trainer integration then passed all 12 focused tests (eight transfer foundation plus four trainer tests) in `registry-transfer-integration-001.{json,log}` and 18 affected training/parallel-collection regressions, with two existing tests intentionally ignored, in `registry-transfer-regression-001.{json,log}` under the same phase1 directory. A separate read-only review found no material state-preservation, cursor/scalar or legacy-loader issue. Root accepts the bounded resolved-schedule implementation. An actual appended-card full-game migration, dynamic historical schedules and cloud execution remain unqualified.
