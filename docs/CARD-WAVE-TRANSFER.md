# Card-wave transfer of trainer checkpoints

Card ids are one append-only space: `data/cards_v1.json` (192 Pauper rows), then the Foundations or Standard registry, then `data/eternal_v1/cards_v1.json` under `eternal-fixtures` (`mtg-kernel/build.rs`). A card wave appends rows, so ids keep their place but `KERNEL_CARDDB_HASH` changes, and every loader refuses a checkpoint whose `card_db_hash` is not the running build's. `expanded_deck_training_v1::card_wave_transfer_v1` carries one ordinary expanded-deck checkpoint across one appended wave into the build that runs it.

It uses the method of `phase1_registry_transfer_v1` (`PHASE1-REGISTRY-TRANSFER.md`), with the same initializer function, for every checkpoint the trainer writes on main rather than only V3 imports against `data/cards_v1.json`:

| State | Transfer behavior |
|---|---|
| Rows of existing cards, padding row, still-unassigned rows | Every parameter and moment bit kept |
| Every other tensor and its moments | Every bit kept |
| Rows of appended cards | `initial_card_row(name, seed)` (SHA-256 of initializer id, seed, name, column); both moments +0.0 |
| Adam step, gauge anchor, loss, scalars, `source_import`, trajectories and every other field | Kept |
| `card_db_hash` | Destination hash |
| `state_sha256` | Recomputed by the native train-state hash |

## Use

Library: `transfer_card_wave_checkpoint_v1(checkpoint, source_registry, destination_registry, request)` returns the destination checkpoint bytes and a `mtg-kernel-expanded-card-wave-transfer/v1` record. `verify_card_wave_transfer_v1` reproduces both from the preserved inputs and accepts them only as the reproduced transfer's own bytes. Registries are given as each build's registry files in build order. `registry_identity_v1` pins them as one identity.

The destination is always the build that runs the transfer: `destination_registry` must be that build's own registry files, byte for byte and in its order (the module embeds them as `build.rs` reads them: `data/cards_v1.json`, then the Foundations or Standard file, then the eternal file), and `request.destination_card_db_hash` its `KERNEL_CARDDB_HASH`.

Command, built with the destination build's features (for example `--features eternal-fixtures`):

```
cargo run --release --features eternal-fixtures --bin card_wave_transfer_v1 -- request.json
```

with

```json
{
  "schema": "mtg-kernel-expanded-card-wave-transfer-cli-request/v1",
  "source_checkpoint": "/abs/old/checkpoint.json",
  "source_card_db_hash": "<16 hex: the checkpoint's card_db_hash>",
  "feature_contract_digest": "<the checkpoint's feature_contract_digest>",
  "feature_encoding_digest": "<the checkpoint's feature_encoding_digest>",
  "source_registry": ["/abs/old/data/cards_v1.json", "/abs/old/data/standard/magezero_v1/cards_v1.json", "/abs/old/data/eternal_v1/cards_v1.json"],
  "destination_registry": ["/abs/new/data/cards_v1.json", "/abs/new/data/standard/magezero_v1/cards_v1.json", "/abs/new/data/eternal_v1/cards_v1.json"],
  "initialization_seed": 1,
  "output_directory": "/abs/fresh/output"
}
```

The request is at most 1 MiB of JSON without a repeated key. The feature digests are the caller's pin: the command compares them with the checkpoint and never copies them from it. It writes `checkpoint.json` and `transfer-record.json` to the fresh directory, reads both back and accepts them only as the reproduced transfer's bytes. If either publication or that check fails, it removes what it wrote and the directory. Every input must be a regular file of at most the checkpoint size bound, and is read no further than that bound. It never starts training.

## Refusals

- The source checkpoint, either registry identity or the destination hash differs from its pin.
- The destination registry is not this build's compiled registry files, byte for byte (an edited, merged or re-split copy with the same card names is refused), or a registry file is not of the schema version `build.rs` compiles (2).
- Source and destination card databases are equal. Transferring the same registry, or a transferred checkpoint again into its own build, is refused.
- The checkpoint is not an ordinary expanded-deck checkpoint of an imported or fresh origin under that origin's schema, its origin already went through a registry transfer (a transferred fresh origin, or an import the phase-1 tool re-bound, which keeps the imported form under the schema `mtg-kernel-registry-transferred-play/v1`), or it carries continuation provenance bound to its own card database (registry transfer, archive distillation, E, soft-Q, search targets, EX1; an explicit `null` member counts). A v1 JSON checkpoint must be the canonical serialization of what it parses to, so nothing it holds is dropped on rewrite.
- The checkpoint is not one the trainer would restore: its feature contract, encoding and input contract must resolve through the trainer's own resolver (V3, V4, FlatV7, FlatV8, no-state-digest V3/V4, with the stored no-state-digest columns at +0.0), its loss and the scalars that loss requires must pass the trainer's own loss check, and its learning rate and value coefficient must be positive and finite. The loss scalars must also be ones an update writes: none of the GAE scalars on the terminal loss (the trainer's presence check alone admits one or two), and on the GAE loss the values its update validated (finite gamma and lambda in 0..=1, entropy coefficient 0).
- The origin's feature identity is not of the checkpoint's own feature generation: an import's contract and encoding digests must be V3's for a V3-generation checkpoint and V4's for a V4-generation one (V4, FlatV7, FlatV8, and the no-state-digest conversion of either generation by its base), and a fresh origin's four feature fields must classify (`fresh_lineage_generation_v1`) as that generation. The checkpoint's feature contract also differs from the request.
- The tensors are not the model of that contract (the V8 inclusion net for FlatV8, net-8 otherwise), or the state fails the restore's native validation or its recorded digest.
- The checkpoint is not bound to the declared source registry: its origin records another card database than the checkpoint, another first registry file, or another card count (a fresh origin counts the whole id space; an import counts its first file), or any card-embedding row past the declared source registry holds optimizer state.
- A source row is removed, renamed or reordered, or changes any field except the metadata fields `build.rs` never reads (`complexity`, `java_file`, `sideboard_only`). Fields `build.rs` does not read either but this list does not name are still compared, which can only refuse more.
- No row is appended although the hash changed. This is a rules or field change of existing cards, not a card wave.

## Limits

`KERNEL_CARDDB_HASH` also covers generated rule behavior in `build.rs`. A wave that also changes the rules of an existing card, without changing its registry row, cannot be seen from the registries and is not refused. The record binds both hashes but does not certify engine equivalence.

**The transferred checkpoint does not yet resume through a model source.** Its `source_import` is kept as recorded, and that origin names the source build's card database, first registry file and card count. Every loader rebuilds the policy from the source's `play_import` in the destination build, which refuses an origin of another card database, and then requires `saved.source_import` to equal that policy's identity ("checkpoint warm-start provenance differs"). The transfer passes the restore's card-database gate and every check after the provenance one, but closing the provenance gate needs an origin re-binding (for example a wrapper that nests the original origin with this record's SHA-256, as `TransferredFreshPlayPolicyIdentityV1` does for the phase-1 tool) and a loader path that admits it. That changes the checkpoint origin format and every loader, so it is left to a design decision.

For an imported origin, the origin records only its first registry file's card count. Rows past that count that hold no optimizer state cannot be told apart from rows never assigned, so a declared source registry shorter than the checkpoint's own in a later file is refused only when one of those rows was trained. Such an untrained row would then be re-initialized, and gameplay and training read the card embedding table by card id without checking the origin's count, so the declared source registry must be the checkpoint's own.
