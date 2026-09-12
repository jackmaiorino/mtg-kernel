# Expanded-deck native training

The `expanded_deck_training_v1` binary connects explicit supported registrations
and preboard/postboard configurations to actor-visible V6/flat V3 rollouts,
immutable trajectory files, the existing native CPU loss/backward/Adam code,
and reloadable successor checkpoints. It is a bounded CPU reference path for
integration and development. It is not the parallel GPU production campaign.

Run `expanded_deck_training_v1 CONFIG.json`. The two configuration modes are
`collect` and `update`; their exact typed schemas are in
`mtg-kernel/src/expanded_deck_training_v1.rs`. Real engineering configurations
are under `C:/Users/Jack/IdeaProjects/sideboarding-integration-20260912/engineering-003/expanded-training/`.

A model source pins the original inference-import manifest, the current
successor feature contract/encoding, and an optional successor checkpoint.
With a null checkpoint, validated imported Net8 parameters initialize fresh
Adam state. With a pinned checkpoint, the complete model and Adam state resume.
The source import receipt describes ancestry; the behavior state hash identifies
the actual collecting model and optimizer. Updated parameters are installed in
the next collecting policy. The original export and Store are never rewritten.

Every episode names both registered 60/15 lists, selected 60/15 lists, a seed,
starting player, learner seat, preboard/postboard flag and physical/policy limits.
Labels need not occur in the fixed runtime deck catalog. All cards must be fully
supported, non-token definitions; combined copies are limited to four except
basics. Selected configurations must conserve the registration's 75 cards;
preboard lists must equal registration. This supports future legal-registration
search in the supported pool, not a claim of current external-format legality
or an implemented brewer.

Collection samples both seats with the existing deterministic categorical
sampler and separate seat streams. It records only actor-visible tensor bits,
outputs and selected dense action indices, alongside episode metadata. Opposing
registrations are metadata and are not appended to model inputs. Only natural
terminals are published. Physical decision substeps stay grouped; terminal
returns are attached to the configured learner seat. There is no invented value
label, partial-game target, new reward, or new baseline.

Update checks trajectory hashes, feature/card identities, source ancestry,
behavior model/optimizer identity, legal configurations, terminal metadata,
contiguous physical groups, deterministic sampled choices, and exact captured
output recomputation. It then uses `terminal_reinforce_value/v3` with explicitly
configured positive learning rate and value coefficient. Old native entrypoints
still reject successor features. Old Store contracts remain strict.

Checkpoint files retain parameter and optimizer binary32 bits, hashes, feature
identity, update settings and source trajectories. Publication uses the existing
immutable-file helper; the completion record is written last. Failed directories
can contain valid completed episodes but have no successful collection/update
marker. Preserve them and use a fresh output directory. The writer rereads each
new checkpoint through the real continuation loader before declaring success.
The helper's Windows namespace durability limits still apply; publication is
not a new guarantee against every power-loss scenario.

Keep batches small. The current CPU adapter limits each file and aggregate input
trajectory bytes to 512 MiB, retains canonical tensors for backward, and updates
one batch at a time. It does not establish GPU throughput, scalable replay or
BO3 strength. Fresh post-update rollouts are required; stale behavior-state data
is rejected. Future production integration and learning schedules require their
own reviewed design.

Focused checks cover actual Map/V3 forward/backward, atomic rejection of bad
schema/shape/output/action inputs, legacy loss goldens, explicit registration
conservation, and tensor bit preservation. The native fixture emitter is checked
with `python/tools/verify_native_flat_v3_fixtures.py`; full-game round-trip and
checkpoint evidence are recorded separately after execution.

Independent review was attempted in fresh read-only Fable session
`435736f5-f380-444f-a02c-985f7f04819c`. It failed HTTP 429 weekly quota with zero
reads and tokens, so no Fable endorsement is claimed. Jack explicitly assigned
the engineering; work continued with this limitation recorded. Supplementary
Codex review identified and corrected positive-value-coefficient validation,
complete terminal/deck binding, deterministic action re-sampling, and aggregate
ingestion limits. Dataset review also required explicit planned-deck coverage
and matching observation provenance. The running native campaign and outcomes
did not inform implementation selection.
