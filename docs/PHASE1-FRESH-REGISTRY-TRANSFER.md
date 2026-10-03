# Phase 1 fresh-origin registry transfer

The transfer library creates a validated in-memory candidate from a real fresh-origin `mtg-kernel-expanded-deck-fresh-checkpoint/v1` and the registry compiled into the current executable. Unlike the imported transfer, it preserves every preallocated parameter and moment bit-for-bit, including rows not yet assigned to a card; nothing is overwritten and no new initialization seed is sampled. A separate, explicit trainer source supports bounded continuation from that candidate, mirroring the imported family's resolved-schedule and scalar/cursor checks exactly. Source tests are engineering verification, not playing-strength evidence.

## Relationship to the imported transfer

This is a parallel engineering path, not a replacement. `phase1_registry_transfer_v1/mod.rs` and its existing imported-only behavior are unchanged; every one of its existing tests still passes unmodified. The fresh code lives in `phase1_registry_transfer_v1/fresh.rs`, a private child module reusing the parent's `require`, `sha`, `bounded`, `mapping`, `registry_cards`, `native_tensors`, `tensor_bits`, `restore` and `hex_digest` helpers verbatim, plus `RegistryTransferScalarsV1` and `RegistryCardMappingV1` unmodified.

| | Imported transfer | Fresh transfer |
|---|---|---|
| Source ancestry | `FrozenPlayPolicyIdentityV1` | `FreshPlayPolicyIdentityV1`, nested unchanged inside a wrapper |
| Newly registered rows | Overwritten with a deterministic name-and-seed initializer; moments zeroed | Preserved bit-for-bit, exactly as sampled at fresh initialization |
| Initialization seed | Required (`initialization_seed`) | None: nothing is sampled, so there is nothing to seed |
| Zero-moment check on new rows | Hard rejection before overwriting | Hard rejection as a pure read-only consistency gate (see below); never mutates |
| `PlayPolicyOriginV1` result | `Imported`, existing ancestry fields mutated in place | `TransferredFreshInitialization`, a new wrapper nesting the original identity unchanged |

## Why every preallocated row is preserved

Net8 already allocates `card_embedding.weight` as `[65537, 16]`. Fresh initialization samples the complete tensor, including rows not yet assigned to a card; padding remains positive zero. Appending registered cards does not require expanding the tensor, and a fresh checkpoint already carries real (if arbitrary) values in every row, assigned or not. There is therefore no analog to the imported path's deterministic per-card seeded initializer: retaining the existing bytes for a newly registered row is both correct and simpler than any invented substitute. Adam step, sampled scorer-bias anchor, loss identity, learning rate and value coefficient are preserved exactly, identically to the imported path.

## The R4 consistency gate, not a heuristic

`transfer_fresh_expanded_checkpoint_to_current_registry_v1` still walks every newly registered card's embedding row and requires its existing first and second Adam moments to be exactly zero:

```
"newly registered card row carries learned optimizer state; the declared source registry does not match this checkpoint's training history"
```

This loop never mutates any tensor. It is a read-only consistency check: a genuinely fresh, never-yet-trained row cannot carry nonzero optimizer moments unless the caller's declared source registry undercounts what this checkpoint was actually trained against (i.e. the checkpoint was trained with more cards already registered than the caller claims). Rejecting that mismatch here is strictly a sanity check on the caller's own provenance claim, not a heuristic about card quality or training history worth preserving. Because nothing is ever overwritten in this path, the check cannot discard real learned state the way a hypothetical fresh-path initializer might; it only ever refuses to proceed with an internally inconsistent request.

## Schemas

| Purpose | Schema string |
|---|---|
| Transfer envelope | `mtg-kernel-expanded-fresh-registry-transfer/v1` |
| `PlayPolicyOriginV1::TransferredFreshInitialization` wrapper | `mtg-kernel-fresh-play-registry-transfer/v1` |
| Trainer source descriptor | `mtg-kernel-expanded-fresh-registry-transfer-source/v1` |
| Successor checkpoint | `mtg-kernel-expanded-deck-fresh-checkpoint/v2-registry-transfer` |
| Resolved schedule, continuation metadata | Unchanged, shared with the imported family (`mtg-kernel-expanded-registry-resolved-schedule/v1`, `mtg-kernel-expanded-registry-continuation/v1`); no fresh-specific variant is defined for either |
| Inference receipt (any transferred-fresh policy) | `mtg-kernel-expanded-deck-inference/v2` (same as plain fresh, because `is_fresh_v1()` is true for this origin too) |
| Trajectory (any transferred-fresh participant) | `mtg-kernel-expanded-deck-trajectory/v3` |

## Request, receipt and envelope shapes

`FreshRegistryTransferRequestV1` mirrors `RegistryTransferRequestV1` with the `initialization_seed` field dropped entirely (not merely ignored): `source_checkpoint_sha256`, `source_registry_sha256`, `source_state_sha256`, `source_adam_step`, `source_card_db_hash`, `destination_registry_sha256`, `destination_card_db_hash`, `features: RegistryTransferFeaturesV1`. There is no required-but-unused field anywhere in this type.

`FreshRegistryTransferReceiptV1` drops the imported receipt's `initializer` field (nothing is initialized) and renames `initialized_embedding_scalar_count` to `preserved_new_embedding_scalar_count`, an honest count of scalars belonging to newly registered rows that were left untouched. Its `mapping_rule` is the fixed literal:

```
exact-shared-card-record-prefix; card_token=card_db_id+1; all-feature-identities-unchanged; preallocated-rows-preserved-unmodified
```

`RegistryTransferScalarsV1` (optimizer identity, Adam beta1/beta2/epsilon/weight-decay bits, Adam step, scorer-bias anchor bits, loss identity, learning-rate and value-coefficient bits) is reused verbatim, unmodified, from the imported module.

The envelope (`FreshTransferEnvelopeV1`) retains the request, receipt, the original `FreshPlayPolicyIdentityV1` byte-for-byte as `source_import`, preserved source trajectory pins, and the full parameter/first-moment/second-moment tensor bits, exactly mirroring the imported envelope's shape.

## Exact validation, in order

`transfer_fresh_expanded_checkpoint_to_current_registry_v1(checkpoint_bytes, source_registry_bytes, request)`:

1. Size bounds on both inputs (512 MiB).
2. Checkpoint and source-registry SHA256 pins match the request.
3. `destination_registry_sha256`/`destination_card_db_hash` in the request match the compiled build.
4. `request.features == RegistryTransferFeaturesV1::current_v1()` exactly: feature migration is unsupported.
5. Checkpoint schema is `mtg-kernel-expanded-deck-fresh-checkpoint/v1` and `loss_identity` is `terminal_reinforce_value/v3`.
6. Saved state hash, Adam step and card-db hash match the request's source pins.
7. Source and destination registries load and cross-check against `CARD_DEFS` order.
8. The checkpoint's own `source_import.destination_registry_sha256`/`destination_card_db_hash`/`destination_card_count` bind to the request's declared source registry.
9. Feature contract, encoding, source and descriptor digests match on both the flat checkpoint fields and the nested `source_import` fields against the request. Unlike the imported path, there is no nested `observation_successor` receipt to unwrap: `FreshPlayPolicyIdentityV1` has no such field at all (fresh identities are natively V3), so this check is a flat six-way equality, not a four-plus-nested nine-way one.
10. `mapping(source, destination)`: exact shared-record prefix; card removal, reordering, renaming, definition changes and duplicate names all rejected (unchanged, shared logic with the imported path).
11. Learning rate and value coefficient are finite and positive.
12. The full native snapshot is reconstructed and its state hash re-validated against the saved pin (covers every float, second moment, padding row, gauge and step bound).
13. **The R4 gate**: every newly registered card's row must already carry positive-zero first and second moments (see above). Read-only; never mutates.
14. State is restored through the real native model and optimizer invariants (`NativePolicyValueTrainStateV1::from_snapshot_v1`).
15. Receipt and envelope are built; `preserved_new_embedding_scalar_count` records how many scalars belong to newly registered rows, all left untouched.

Every one of the steps above has a dedicated rejection test in `fresh.rs`'s own test module: the checkpoint-bytes and destination pins, the checkpoint schema and loss identity, the source-import destination binding (registry, card-db hash, card count), the six-way feature contract/encoding/source/descriptor equality, and non-positive or non-finite optimizer scalars each fail with their own asserted exact message.

`verify_fresh_registry_transfer_artifact_v1` reproduces the whole transfer from preserved source bytes and requires byte-exact structural equality with the persisted envelope, exactly like the imported path's tamper check.

## Origin: `TransferredFreshPlayPolicyIdentityV1`

```
pub struct TransferredFreshPlayPolicyIdentityV1 {
    pub schema: String,                          // "mtg-kernel-fresh-play-registry-transfer/v1"
    pub original: FreshPlayPolicyIdentityV1,      // nested, byte-for-byte unchanged ancestry
    pub destination_registry_sha256: String,      // current registry, post-transfer
    pub destination_card_db_hash: String,         // current card-db hash, post-transfer
    pub destination_card_count: usize,            // current card count, post-transfer
    pub transfer_envelope_sha256: String,         // the fresh-transfer envelope's hash
}
```

`original.*` is historical ancestry: the exact fresh identity that existed before the transfer, including its own now-stale `destination_*` fields. It is never rewritten to claim initialization on the expanded registry. The wrapper's own `destination_*` fields are the current runtime identity, updated by the transfer.

Accessor routing on `PlayPolicyOriginV1`:
- `destination_registry_sha256_v1()`, `destination_card_db_hash_v1()`, `destination_card_count_v1()` read the **wrapper's** own fields (current identity), matching how the imported path's accessors already behave after `from_registry_transfer_v1`'s in-place mutation.
- `feature_contract_digest_v1()`, `feature_encoding_digest_v1()`, `initial_weights_sha256_v1()`, `initial_model_parameter_sha256_v1()`, `origin_git_commit_v1()` read the **nested `original`** ancestry, unchanged. The feature digests are correctly not among the wrapper's own fields: a transfer requires the source's feature identity already equal the current runtime's (feature migration is unsupported), so `original.feature_contract_digest`/`feature_encoding_digest` already describe the current runtime by construction.
- `is_fresh_v1()` returns **true** (R1): the transferred-fresh origin is on the wide/V3 sampling and scoring path exactly like plain fresh initialization, and uses the fresh-family inference/trajectory schemas.
- `as_imported_v1()` returns `None`.

The custom `Deserialize` impl (`OriginVisitor`) nests the `original` sub-document through `map.next_value::<FreshPlayPolicyIdentityV1>()`, exactly like the existing `observation_successor` nested receipt: this gets duplicate-key safety for the whole nested object for free from `FreshPlayPolicyIdentityV1`'s own derived, `deny_unknown_fields` `Deserialize`, without hand-bucketing its 19 fields a second time. The wrapper's own top-level fields (`schema`, `original`, `destination_registry_sha256`, `destination_card_db_hash`, `destination_card_count`, `transfer_envelope_sha256`) are checked against a closed allowlist; any other key is rejected. `"original"` and `"transfer_envelope_sha256"` were also added to the Imported branch's fresh-only-field rejection list, so a `TransferredFresh`-shaped document whose `schema` value is corrupted or misspelled fails with the same clear "fresh origin fields require the explicit fresh schema" error the plain-fresh case already produces, instead of degrading to a confusing missing-field error from the Imported branch (the three `destination_*` names are legitimately shared with Imported already and stay off that list).

## Construction: `from_fresh_registry_transfer_v1`

`FrozenPlayPolicyV1::from_fresh_registry_transfer_v1(transfer, envelope_sha256)` installs the transferred model and its `card_embedding.weight` tensor, then builds the wrapper identity by nesting `transfer.source_import_v1().clone()` (the original ancestry, never mutated) and recording the request's current destination pins plus the envelope hash separately. This is the opposite shape from the imported path's `from_registry_transfer_v1`, which clones the ancestry and mutates several of its fields in place. It deliberately does not call `from_fresh_initialization_v1`: that constructor requires the *input* identity's own `destination_*` fields to already equal the current runtime, which a pre-transfer fresh identity (still describing the old registry) does not satisfy.

## Trainer continuation

`expanded_deck_training_v1/fresh_registry_transfer_source.rs` is a near-verbatim duplicate of the imported `registry_transfer_source::initialize`, with exactly two call substitutions: `verify_fresh_registry_transfer_artifact_v1` in place of `verify_registry_transfer_artifact_v1`, and `FrozenPlayPolicyV1::from_fresh_registry_transfer_v1` in place of `from_registry_transfer_v1`. It reuses `registry_transfer_source::{ExpandedRegistryTransferSourceV1, ExpandedRegistryTransferScheduleV1, ExpandedRegistryContinuationV1, TransferTrainingContextV1, check_pin, batch_sha, read_schedule}` directly: none of these types or functions reference an origin type, and the schedule/continuation schema strings are unchanged and shared between the two families. `registry_transfer_source.rs` itself gained only two purely additive, behavior-preserving changes: widening `check_pin`, `batch_sha` and `read_schedule` from private to `pub(super)`, and a `TransferTrainingContextV1::new` constructor (taking an explicit `completed_updates` so the sibling can build the final, already-correct context once, without a cross-module private-field mutator). Its own `initialize` function, tests and every other line are untouched.

`ExpandedModelSourceV1.play_import` with `checkpoint: null` continues to mean "restore this lineage's pinned trained state", never an Adam reset, exactly as for the imported family: `fresh_registry_transfer_source::initialize` starts `completed_updates` at zero only when `source.checkpoint` is absent, and otherwise validates the resumed checkpoint's provenance against an independently recomputed continuation record before adopting its cursor.

`execute_update_v1`'s checkpoint schema selection is now three-way, discriminated on the actual transfer context's kind rather than a separately threaded flag: a private `TransferContextV1` enum (`Imported` / `Fresh`, both wrapping the identical, reused `registry_transfer_source::TransferTrainingContextV1`) is what `initialize_with_transfer_context` returns; `execute_update_v1` matches on it to choose `fresh_registry_transfer_source::CHECKPOINT_SCHEMA_TRANSFER`, `registry_transfer_source::CHECKPOINT_SCHEMA_TRANSFER`, or the ordinary fresh/imported schema. `inference_schema_v1` and `trajectory_schema_v1` needed no changes at all: both already branch purely on `is_fresh_v1()`, which is now true for both fresh origins by construction (R1).

## Limits

- **BO3 transfer is out of scope.** `load_ordinary_bo3_parent_v1` (via `validate_ordinary_source_descriptor_v1`) continues to reject a fresh registry-transfer-schema `play_import` descriptor structurally: it decodes neither as the fresh-initialization source schema nor as a bare `FrozenPlayPolicyImportV1`, so it fails before any transfer-specific check runs. A test asserts this directly. BO3 checkpoints additionally bind an objective, attempt ledger, progress chain and recovery state that this transfer does not understand; a later explicit objective/registry transition needs its own design and qualification.
- **Complete-agent and sideboard-fit paths are unchanged and out of scope for this change.** They consume `PlayPolicyOriginV1` only through its named accessors (never pattern-matching the enum), so they remain mechanically correct once the accessor arms above exist, but nothing here teaches them to *expect* a transferred-fresh origin as evidence of anything beyond weight identity. Explicit admission is a later, separate decision.
- **Cloud packaging must be extended explicitly.** `phase1_agent_v1/package.rs` and any manifest/relocation tooling that names origin schemas by string must be updated deliberately before they accept a transferred-fresh descriptor; nothing here does that implicitly.
- **A second registry migration of an already-transferred successor is unsupported**, exactly as for the imported family: the transfer function only ever reads a plain `mtg-kernel-expanded-deck-fresh-checkpoint/v1`, never a `TransferredFreshInitialization`-origin checkpoint, as its `source_import`.
- **This is engineering-only verification.** These tests exercise real CPU Net8 forward/backward/Adam continuation on small synthetic V3 decisions and real registry/state hashing; they are not a training campaign, a cloud allocation, or any claim about playing strength, metagame coverage or brewing success. `KERNEL_CARDDB_HASH` and the destination build's Git HEAD are recorded for context and provenance, not presented as an independent behavioral-equivalence or clean-tree attestation.
