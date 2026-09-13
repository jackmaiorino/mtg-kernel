# Phase 1 explicit registry transfer

This opt-in library API transfers a real `mtg-kernel-expanded-deck-checkpoint/v1` to the registry compiled into the current executable. It preserves learned state while assigning embeddings to appended cards. It writes no files, starts no work and changes no existing checkpoint loader. Source tests are engineering verification, not playing-strength evidence.

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

`verify_registry_transfer_artifact_v1` takes the artifact hash and preserved source checkpoint/registry bytes, reproduces the transfer and requires equality of all state and provenance. Artifact edits remain invalid even if someone updates only its outer byte hash. Runtime restoration uses the real native model and optimizer invariants. The crate-only `into_train_state_v1()` returns the validated state together with its scalar settings as a minimal future explicit trainer integration seam.

## Verification and limitations

Focused source tests exercise real CPU Net8 forward, backward and Adam with small synthetic V3 decisions. They compare every shared parameter and moment bit before transfer and after the next update, preserve a non-default scorer anchor, verify identity-transfer continuation, learn an appended card without resetting the step, reject learning in a purportedly unassigned row, and reproduce an artifact before rejecting tampering. These are source tests for the lead to compile and run; no training campaign or cloud allocation belongs to this module.

This first API accepts existing expanded V1 checkpoints only. Legacy Store V2/V3/V4 payloads, wide Net8, auxiliary sideboard/opening optimizer states, recursive transfer-envelope continuation, different objectives, feature migrations and changing shared card definitions require separate interfaces. It does not restore episode indices, collector RNG or curriculum state that the source checkpoint does not contain. It does not certify correctness of newly implemented cards or external build history.

`KERNEL_CARDDB_HASH` includes generated rule behavior from `build.rs`, not simply the JSON registry bytes. The receipt binds the declared source and compiled destination card-db hashes, but cannot infer or certify behavioral equivalence between those engines. Caller-provided historical hashes identify preserved inputs; they are not an independent provenance audit. The destination build HEAD is recorded for context, not presented as a clean-tree or executable attestation.

Production integration remains explicit follow-up work: select the new envelope in a versioned trainer path, retain its transfer receipt in subsequent checkpoint provenance, preserve optimizer scalar settings and resume schedule identity, and rebind gameplay/sideboard package identities to the installed parameters and current registry. No default training objective, registry, agent package or frozen measurement changes in this deliverable.

Root ran all eight focused Rust tests successfully on September 13 with four BelowNormal build jobs. The receipt and log are `C:/Users/Jack/IdeaProjects/sideboarding-integration-20260912/phase1/registry-transfer-unit-001.{json,log}`. Independent read-only source review found no material preservation or mapping defect within this scope. Fresh Fable session `40d42c80-ab8c-4a5f-825d-066c7b11add3` failed at the weekly limit before source reads or feedback. Root accepts this opt-in engineering API under the user's explicit implementation assignment with that review limitation recorded; this is not Fable endorsement, actual campaign migration, or production activation.
