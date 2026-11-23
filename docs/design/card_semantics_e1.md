# Card semantics e1

Opt-in input for the 16-wide expanded-deck learner. Configuration on the run
and its `update` / `update_prepared` command:

```json
"card_semantics": {
  "table_path": "data/card_semantics_v1/pauper_table_v1.json",
  "table_sha256": "<SHA-256 of the exact file bytes>"
}
```

The loader checks the file pin, table schema and compiled `KERNEL_CARDDB_HASH`.
Token `card_id + 1` reads the corresponding dense row. Missing cards get zero
rows. Each column is divided by the smallest power of two at least its maximum
absolute integer (minimum divisor 1). Integers must convert exactly to f32.
Binding is SHA-256 of the 64 lowercase ASCII digest characters, scaled dense
row f32 bits in little-endian row order, then cards and width as little-endian
u64 values. No observation, tensor manifest or initialization changes.

The stored card table is always `c = id + P s`. The existing rules pathway
implements projection, `G^T X` and Adam. The ordinary CPU GAE core stages P's
Adam step at its own `p_step + 1`, stages the unchanged network Adam step,
then adds `delta_new - delta_old` to the candidate card rows, skipping both
signs of zero. It validates everything before committing the network and P.
Network moments are unchanged by folding. The ordinary commit invalidates the
network digest cache. An absent option calls the existing profiled entry point.

Routing: `native_expanded_training_run_v1::update_command` selects
`UpdatePrepared` when preparation_workers > 1. Ordered preparation builds the
same groups and GAE targets, including terminal pregame advantage. In
`execute_update_inner_v1`, plain `GaeAdvantageValueV1` plus CPU reaches
`train_step_gae_card_semantics_v1`, then the existing
`train_step_gae_core_profiled_dims_v1`. Forward workers come from
preparation_workers; either existing backward execution mode is supported.
CUDA, exact-GPU learner installation, wide/S nets, terminal/recovery losses,
auxiliary teachers and distillation are refused.
Other command schemas reject the configuration as an unknown field.

Each enabled checkpoint has one optional `card_semantics` section: schema
`card_semantics_e1`, binding, p_step, weight, first_moments and second_moments
(as u32 bits). Training requires the configured binding and a section when
resuming; flag-off training refuses a section. Fresh starts without a checkpoint
create P at zero. Enabled checkpoint `state_sha256` hashes the domain bytes
`card_semantics_e1` plus NUL, the ordinary 32-byte tensor/optimizer digest,
and compact serde JSON of the section, in that order. Without the section,
the existing digest and serialization are unchanged. Enabled checkpoints use
v1 JSON (auto falls back; explicit v2 is refused pending container qualification).

Ordinary expanded collection and evaluators accept and ignore the section after checking this
integrity digest; they use the folded 33 tensors. Their inference state identity
remains the ordinary tensor/optimizer digest, as do update before/after receipts.
The checkpoint pin and checkpoint integrity digest also bind P. No inference
arithmetic changes or export requirement for collection. Legacy derived learners
using the unconfigured initializer refuse the section; their readers can use
the flag-off export. The width prescreen also refuses semantics parents.

## Offline tools

`card_semantics_e1 REQUEST.json` prints output pins including SHA-256 values.
The request uses an ordinary `ExpandedModelSourceV1` for `source`, including
its checkpoint pin and the lineage play-import pin. Output directories must
not already exist. Neither operation changes source files.

* Export: `{"mode":"export","source":{...},"output_directory":"..."}`.
  Validates the checkpoint, removes the section and restores the ordinary
  state digest. Writes `checkpoint.json` using the existing v1 JSON serializer,
  identical to a flag-off v1 checkpoint with these tensors, optimizer and metadata.
* Fork: `{"mode":"fork-import","source":{...},"initial_source":{...},
  "tokens":[1,2],"card_semantics":{...},"output_directory":"..."}`.
  `initial_source` has no checkpoint and pins the existing fresh-initialization
  descriptor; its manifest/payload are checked by `fresh_initialization_source`.
  Its identity must equal the checkpoint lineage. Tokens are unique, nonzero
  card tokens. Resets only their embedding rows and moments (+0.0), retaining
  the network Adam age. Writes `flag_off.json` and `card_semantics.json` with
  zero P/moments and p_step 0. Reports both pins and checks equality of all
  33 tensors and both moment sections.

Tests cover absent bytes, folded first-step arithmetic and moments, P central
differences, binding/off-mode refusal and integrity, two-step checkpoint resume
with separate ages, unsupported paths, signed zero, failure atomicity, file pin,
card database pin, sparse rows and power-of-two scaling. Tests are reserved
for the coordinating session.
