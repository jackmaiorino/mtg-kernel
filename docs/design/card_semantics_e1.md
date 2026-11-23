# Card semantics e1

Opt-in input for the 16-wide expanded-deck learner. Configuration on the run
and its `update` / `update_prepared` command:

```json
"card_semantics": {
  "table_path": "<absolute path of the table file>",
  "table_sha256": "<SHA-256 of the exact file bytes>"
}
```

Run validation requires an absolute table path. Startup loads the table once and
passes the validated projection through initial-state admission and recovery.
The loader opens a regular file,
checks its size, and caps the read at 64 MiB, including growth during the read.
It checks the file pin, table schema and compiled `KERNEL_CARDDB_HASH`.
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
`train_step_gae_card_projection_e1`, then the existing
`train_step_gae_core_profiled_dims_v1`. Forward workers come from
preparation_workers; either existing backward execution mode is supported.
CUDA, exact-GPU learner installation, wide/S nets, FlatV8, terminal/recovery losses,
auxiliary teachers and distillation are refused.
Other command schemas reject the configuration as an unknown field. The run
validates the table and pin, initial checkpoint section and binding, v1 writer
selection (auto resolves to v1), and GPU exact learner exclusion before any
iteration collects. Explicit `card_semantics: null` is invalid at each wire
boundary; omission alone disables the feature.

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
Enabled `update.json` receipts also record `card_semantics: {"binding":"..."}`;
flag-off receipts omit the member. Every recovered and completed update checks
this member against the run, and checks the loaded checkpoint's section presence,
schema, binding, weight/moment sizes and finiteness, non-negative second moments,
and exactly one step of P optimizer age independently. It cannot substitute a
flag-off checkpoint,
an enabled checkpoint into an off run, or a different table binding.
The checkpoint pin and checkpoint integrity digest also bind P. No inference
arithmetic changes or export requirement for collection. Legacy derived learners
using the unconfigured initializer refuse the section; their readers can use
the flag-off export. BO3 transition, V8 inclusion conversion, archive distillation, weights snapshots
and the width prescreen explicitly refuse semantics parents. The projection
module is `native_policy_train_step_v1/card_projection_e1.rs`; it does not refer
to the offline generator or its output path.

The update takes the section from its existing metadata probe. Ordinary source
loads retain admission metadata on the loaded train state, including source-cache
clones, so legacy initialization, run recovery and prescreen checks reuse the
normal read. Archive distillation uses its existing initializer and parent parse.
No flag-off learner path adds a checkpoint read or parse.

Main's loaded-source cache also serves a policy or an identity without a train
state. Every entry keeps the whole section, and the identity-only lookup returns
it, so update validation checks it and restores and ages the run's projection
on a hit without a train state or another read.
The carried learner hands its loaded train state to the update, which still
takes the section from its metadata probe. A run with the option loads its
initial train state to restore its projection from the section, also when it
keeps no carried learner. Main's converted run start is the same load (run
validation refuses the option for a converted source): at a fresh start a
flag-off converted run refuses a section through that ordinary admission, and
the run-start conversion keeps the section on the converted train state.
Without the option, a run start that loads only the policy (a resume after a
completed iteration) does not see the section: the recovered iteration's
receipt and checkpoint checks and the update's own refusal cover that case.

## Offline tools

`card_semantics_e1 REQUEST.json` prints output pins including SHA-256 values.
The request uses an ordinary `ExpandedModelSourceV1` for `source`, including
its checkpoint pin and the lineage play-import pin. Output directories must
not already exist. Neither operation changes source files.

* Export: `{"mode":"export","source":{...},"output_directory":"..."}`.
  Validates the checkpoint, removes the section and restores the ordinary
  state digest. Writes `checkpoint.json` using the existing v1 JSON serializer,
  identical to a flag-off v1 checkpoint with these tensors, optimizer and metadata.
  Loads the published file through the ordinary flag-off loader and compares tensors,
  moments and Adam step bit for bit.
* Fork: `{"mode":"fork-import","source":{...},"initial_source":{...},
  "tokens":[1,2],"card_semantics":{...},"output_directory":"..."}`.
  Admission requires an ordinary 33-tensor checkpoint. It refuses archive
  distillation, E-supported, soft-Q, search-target, EX1, exploration, registry
  transfer, input-contract and existing projection sections, and 37-tensor FlatV8.
  Admission precedes following unsupported sections' external references. The
  source loader also checks transfer context and input contract before creating
  the output directory, including converted run-start sources.
  `initial_source` has no checkpoint and pins the existing fresh-initialization
  descriptor; its manifest/payload are checked by `fresh_initialization_source`.
  Its identity must equal the checkpoint lineage. Tokens are unique, nonzero
  card tokens. Resets only their embedding rows and moments (+0.0), retaining
  the network Adam age. Writes `flag_off.json` and `card_semantics.json` with
  zero P/moments and p_step 0. Reports both pins and checks equality of all
  33 tensors, both moment sections and Adam step. After both files are written,
  loads each through the normal checkpoint loader, admits the off output without
  configuration and the enabled output with the supplied pinned table, then checks
  both readbacks against the intended tensors and against each other.

Tests exercise the flag-on GAE step for central differences of P, post-Adam
failure with the London keep offset set, populated digest-cache invalidation,
checkpoint resume, and the core's unsupported-path refusals. File fixtures cover
tool publication/readback, every fork refusal, production loader admission,
recovery receipt/binding and malformed or stale P state, preservation of unlisted
fork rows and moments, strict null handling, invalid table rows/schema and
bounded file reads. A real `execute_v1(UpdatePrepared)` test
uses Affinity/Terror mainboard cards observed in learner decisions, requires
nonzero P weights and moments, and checks every folded coordinate against the
flag-off table plus its P delta. Main plays different games from the release
line, so the test takes those cards from its own game's learner rows. It
compares one and four preparation/forward
workers bit for bit, including P and moments, with sequential and fixed-partition
backward, and compares the off update with the legacy Update route.
Metadata-probe counters guard flag-off load costs. Run
tests check refusal before collection, including flag-off converted checkpoints
with a section. Update and run entry points exercise S-learner, wide-learner,
FlatV8, input-contract and registry-transfer refusals; the GAE core separately
refuses a 37-tensor FlatV8 model.
