# Fresh-origin registry transfer: source audit

September 13, 2026. Reviewed tracked source `72a3a28006cfda0d9af1815cd824e701699426ef`. This is a proposed next engineering design, not an implemented or qualified registry migration. Campaign training and paid execution remain paused. The purpose is to support future deck coverage and brewing without resetting learned state or inventing ancestry. A fresh Fable consultation belongs to the implementation decision; this source audit is not its substitute.

## Existing boundary

[`phase1_registry_transfer_v1/mod.rs`](../mtg-kernel/src/phase1_registry_transfer_v1/mod.rs) accepts only imported ordinary `mtg-kernel-expanded-deck-checkpoint/v1` inputs. Its checkpoint/envelope fields use `FrozenPlayPolicyIdentityV1`, and its feature checks require imported observation-transfer provenance. Fresh ordinary checkpoints instead use `mtg-kernel-expanded-deck-fresh-checkpoint/v1` and `FreshPlayPolicyIdentityV1`.

The transfer already validates all 33 parameter tensors, both complete Adam moment sections, step, scorer-bias anchor and scalar bits through the actual native snapshot. Registry records must be an exact shared prefix, and all V3 feature identities must remain unchanged. The explicit trainer source already binds a resolved episode schedule, exact scalar settings and a completed-update cursor.

## Preserve every existing scalar

Net8 already allocates `[65537, 16]` embedding values. Fresh initialization samples the complete tensor, including rows not yet assigned a card; padding remains positive zero. Appending registered cards does not require expanding the tensor.

The imported transfer V1 deliberately replaces newly assigned rows with its name-and-seed initializer, after requiring their existing moments to be zero. It preserves shared cards, remaining unassigned rows and other tensors, but it does **not** preserve every preallocated parameter. Keep that established imported behavior and wire format unchanged.

For the proposed fresh successor, retain all preallocated parameters and moments, including newly assigned rows. Preserve Adam step, sampled anchor, loss and learning-rate/value-coefficient bits. No new initialization seed is needed. The destination model and full-state hashes must equal the source hashes exactly; current registry identity changes separately. Retaining the zero-moment check for newly assigned rows provides a useful consistency check without erasing state.

## Minimal explicit extension

| Source seam | Proposed change |
| --- | --- |
| `phase1_registry_transfer_v1/mod.rs`: input/envelope, mapping and restoration | Add fresh-only input/envelope schemas, strict original fresh metadata validation, exact source registry binding and unchanged-snapshot restoration. Reuse mapping and native state validation; do not loosen imported V1. |
| `sideboard_play_policy_v1/origin.rs`: `PlayPolicyOriginV1` | Add a strict transferred-fresh variant nesting the entire original `FreshPlayPolicyIdentityV1` unchanged. Store current destination registry/card-db/count and transfer-envelope hash separately. Initial weight/seed/producer accessors use nested ancestry; destination accessors use current identity. |
| `sideboard_play_policy_v1.rs`: verified transfer constructor | Install the restored model and matching sideboard embeddings through a fresh-transfer constructor. Do not call `from_fresh_initialization_v1`: it correctly requires original initial weights and target to match the current runtime. |
| `expanded_deck_training_v1/registry_transfer_source.rs` | Add explicit fresh-transfer source and successor-checkpoint schemas. Reuse resolved schedule, exact scalar/cursor checks and readback. Preserve original source checkpoint/registry and envelope pins. |
| `expanded_deck_training_v1.rs`: dispatch and receipts | Admit only the new explicit transfer descriptor, preserve strict ordinary-reader rejection and emit origin-aware inference/trajectory identities. A transfer descriptor with top-level `checkpoint: null` restores its pinned trained state; it must never mean Adam reset. |

The new envelope should retain the source checkpoint/registry pins, original fresh ancestry, complete state, mapping and destination receipt. Its source descriptor can retain the existing four-pin structure: source checkpoint, source registry, transfer envelope and continuation schedule. A distinct fresh-transfer checkpoint schema plus required continuation metadata avoids presenting a migrated checkpoint as an ordinary original-registry checkpoint. Exact schema names should be finalized with the implementation review.

Original generator manifest hashes, seeds, producer commit and original registry remain historical ancestry. They must not be rewritten to claim initialization on the expanded registry. Preserved source-checkpoint pins identify supplied evidence; metadata alone does not independently certify historical generator or engine execution.

## Meaningful qualification and failure cases

- Transfer a trained fresh state with nonzero moments and a non-default sampled scorer anchor. Compare every parameter/moment bit, scalar and step before transfer, after readback and on repeated transfer.
- Exercise an actually appended card in the destination runtime, then update and resume without resetting Adam. Same-registry identity transfer alone does not qualify appended-card gameplay.
- Reject registry removal, reordered IDs, renamed or changed shared records, duplicate keys/names, feature-source or semantic changes, incompatible tensor layout, altered ancestry, wrong source/destination pins and wrong schema/objective.
- Reject malformed/nonfinite state, invalid second moments, padding or gauge changes, unexplained optimizer history in a newly assigned row, altered scalar/cursor/schedule, and edited envelopes even when their outer hashes are recomputed.
- Retain imported V1 byte behavior, ordinary-loader rejection and complete checkpoint-only recovery checks. Keep fixed opponents pinned to destination-compatible runtime sources and keep auxiliary states outside this transfer.

## Separate limitations

BO3 registry transfer remains out of scope. `load_ordinary_bo3_parent_v1` explicitly rejects registry-transfer sources, and BO3 checkpoints additionally bind an objective, attempt ledger, progress chain and recovery state. Do not cast those checkpoints to ordinary state or rewrite their original parent graph. A later explicit objective/registry transition needs its own design and qualification.

This first extension should also reject a second registry migration of a transferred successor, feature migration and dynamic historical schedules. Current shared-record equality includes metadata such as deck membership; adding a new deck by editing existing card records is not an append-only registry transfer. Generated card-rule identity is not derivable from registry JSON alone, so transfer does not prove engine-rule equivalence or newly implemented card correctness.

Existing fitted sideboard heads cannot be silently relabeled for the new registry. Cloud packaging and complete-agent consumers must explicitly understand the new descriptor/origin before accepting it. None of these engineering checks establishes new metagame coverage, learned strategic diversity, playing strength or brewing success.
