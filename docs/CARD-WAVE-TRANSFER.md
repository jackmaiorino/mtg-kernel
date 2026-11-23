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

Library: `transfer_card_wave_checkpoint_v1(checkpoint, source_registry, destination_registry, request)` returns the destination checkpoint bytes and a `mtg-kernel-expanded-card-wave-transfer/v1` record. `verify_card_wave_transfer_v1` reproduces both from the preserved inputs. Registries are given as each build's registry files in build order. `registry_identity_v1` pins them as one identity.

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
  "source_registry": ["/abs/old/data/cards_v1.json", "/abs/old/data/standard/magezero_v1/cards_v1.json", "/abs/old/data/eternal_v1/cards_v1.json"],
  "destination_registry": ["/abs/new/data/cards_v1.json", "/abs/new/data/standard/magezero_v1/cards_v1.json", "/abs/new/data/eternal_v1/cards_v1.json"],
  "initialization_seed": 1,
  "output_directory": "/abs/fresh/output"
}
```

It writes `checkpoint.json` and `transfer-record.json` to the fresh directory after re-verifying them. It never starts training.

## Refusals

- The source checkpoint, either registry or the destination hash differs from its pin, or the destination is not this build (`KERNEL_CARDDB_HASH`, and the compiled card names in id order).
- Source and destination card databases are equal. Transferring the same registry, or a transferred checkpoint again into its own build, is refused.
- The checkpoint is not an ordinary expanded-deck checkpoint, has a different feature contract from the request, or carries continuation provenance bound to its own card database (registry transfer, archive distillation, E, soft-Q, search targets, EX1).
- A source row is removed, renamed or reordered, or changes any field except the metadata fields `build.rs` never reads (`complexity`, `java_file`, `sideboard_only`).
- No row is appended although the hash changed. This is a rules or field change of existing cards, not a card wave.
- The tensors are not the net-8 or V8 inclusion train-state layout, or the source state fails native validation or its recorded digest.
- An appended card's row already holds optimizer state.

## Limits

`KERNEL_CARDDB_HASH` also covers generated rule behavior in `build.rs`. A wave that also changes the rules of an existing card, without changing its registry row, cannot be seen from the registries and is not refused. The record binds both hashes but does not certify engine equivalence.

The checkpoint's `source_import` is kept as recorded. A model source resumes the transferred checkpoint only with a `play_import` whose loaded identity equals it in the destination build. An origin that binds the registry (an imported export's or a fresh initialization's destination fields) needs its own re-binding, which this transfer does not provide.
