# Flat policy v1 typed-state encoder validation

Date: 2026-07-18

Status: validated bounded engineering checkpoint, not a scorer, trainer, or
science-release certificate

Base source parent: `3de6d2c450d4d32f120f70034324bfc72e1e3339`

## Scope

This record validates the first kernel-owned typed `FlatDecisionV1` producer.
The only production entry point is
`FastActorSessionV1::encode_current_flat_decision_v1`: it derives state and
ordered legal-action meaning from the session's private current decision and
publishes into caller-owned ragged buffers.

The accepted engineering slice includes:

- actor-relative typed globals;
- all 20 versioned object groups and all 14 versioned relation roles;
- typed object rows, relation payload unions, and variable-length subtype,
  ability-use, goad, completed-dungeon, effect-subtype, and context-path rows;
- the existing `FlatActionCoreV1` and `FlatActionRefV1` tables without changing
  their mapping or current-decision authority contract;
- a separate operational `FlatActionObjectV1` table preserving the existing
  action-reference `object_index` meaning; every operational action object must
  resolve to exactly one typed model object, and every reference token must
  agree, before publication;
- exact versioned bindings for the typed layout, feature inventory, enum maps,
  object groups, relation roles, context subroles, card catalog, and existing
  action slice;
- compiled SHA-256 identities for the generated mapping contract, canonical
  feature inventory, and Rust typed-layout source, plus a separately versioned
  exhaustive Rust-internal-to-Python-projection action-reference role
  crosswalk;
- two-pass publication: authoritative construction and validation plus every
  capacity check complete before any caller buffer is written;
- reusable encoder storage and a zero-allocation warmed path for repeated
  encoding of the same unchanged current decision.

The generated inventory classifies each of the 964 authoritative semantic
leaves discovered from `python/mtg_kernel_rl/features.py` exactly once:

| Classification | Count |
| --- | ---: |
| Model input | 778 |
| Operational only | 176 |
| Forbidden | 10 |

The ten forbidden leaves are absent from public model rows. Raw arena ids stay
private. Zone-change counters appear only in the separate operational
`FlatActionObjectV1` table used to bind and revalidate executable action
references; they are absent from model rows and must remain actor-side when a
scoring bridge is added. Names, display strings, stable strings, and diagnostic
hashes are not stored in the typed model tables.

## Focused evidence

The focused Rust tests validate:

- exact equality between the complete encoder's action binding/actions/refs/
  operational action objects and the pre-existing action-slice producer;
- exact runtime binding of the three generated contract digests and every
  entry in the eight-wide Rust to ten-wide Python action-reference role
  crosswalk, including the nonidentity `pending_sources` 7-to-9 mapping;
- exact row counts and typed debug digests for Burn/Burn, Rally/Rally, and
  Burn/Rally initial decisions, plus the first relation-bearing Burn and Rally
  decisions;
- every reached decision in deterministic randomized Burn and Rally rollouts,
  including relation endpoint range checks;
- snapshot, clone, restore, and stale expected-decision behavior;
- no caller-buffer publication for short capacity, including a separate exact
  check for each of the eleven ragged tables;
- valid absolute-seat swaps covering historical stack targets, object
  relations, continuous effects, permissions, attachments, and goads produce
  identical actor-relative globals, objects, auxiliary rows, and relations;
- announcement-time target control survives a same-generation live control
  change; immutable card, owner, or zone conflicts and detached historical
  targets in inadmissible zones fail closed, while an exact later live target
  coalesces with its historical identity;
- one pre-cost object used both as a spell target and an additional paid cost
  resolves to distinct historical target and paid-cost rows, with each relation
  bound to its role-specific row;
- set-like relations use one actor-relative canonical order, including the
  same-object permission tie-break;
- mutations to names, hashes, raw arena ids, and raw zone-change counters do
  not change public model tables, while card identity does;
- every modeled continuous-effect field changes the typed effect signature,
  while the operational timestamp alone does not;
- a source-less player-only effect remains represented;
- warmed complete encoding across multiple Burn and Rally decisions performs
  zero allocations.

The generated Python tests independently recompute the enum maps, inventory,
source hashes, mapping digest, payload digest, checked-in outputs, and the exact
named action-reference crosswalk. The inventory test rejects missing,
duplicate, or differently classified authoritative leaves and requires all
forbidden leaves to map to `absent`. The Rust build independently recomputes
the same canonical mapping/inventory digests, validates recorded Rust/Python/
card source hashes, and refuses stale or malformed generated artifacts before
compilation.

## Validation commands

The following commands passed from this worktree with the locked dependency
graph. `CARGO_TARGET_DIR` was an isolated external target directory, and the
existing locked Python environment was used with `PYTHONPATH=python`.

```powershell
cargo fmt --all -- --check

python python/tools/generate_flat_policy_v1_goldens.py --check
python -O python/tools/generate_flat_policy_v1_goldens.py --check
python -m unittest python.tests.test_flat_policy_v1_goldens -v

cargo test --locked -p mtg-kernel --lib flat_policy_v1::tests -- --nocapture
cargo test --locked -p mtg-kernel `
  --test flat_policy_v1 --test flat_policy_allocation -- --nocapture

cargo clippy --release --locked --workspace --all-targets --all-features `
  -- -D warnings
cargo test --locked --workspace --all-targets --all-features
cargo test --release --locked --workspace --all-targets --all-features
```

Both complete Rust matrices passed. The existing
`source_traces_match_fixture` test remained intentionally ignored because its
external XMage oracle material was not supplied; no XMage process or benchmark
was part of this slice.

## Residual claim boundary

This checkpoint does not implement or validate a typed-to-tensor accelerator
projection, model scorer, native rollout service, optimizer, checkpoint format,
scheduler, CUDA integration, runner/trainer/evaluator consumption, learning,
or end-to-end throughput. No speed claim follows from these tests.

The zero-allocation result is limited to repeated encoding of an unchanged
already-cached current decision with admitted capacities. Building a newly
reached decision still constructs the authoritative observation and may
allocate or hash. The runtime fixture digest uses Rust typed `Debug` output as
a source-bound regression oracle; it is not a canonical cross-implementation
binary artifact format.

Burn and Rally randomized walks plus the bounded synthetic fixtures are strong
engineering coverage, not an exhaustive proof over every constructible pending
or private context. Independent source audits found and closed historical-target
controller, actor-relative ordering, heuristic-destination, and contract-digest
gaps. The generated inventory now proves complete one-to-one leaf
classification. A future accelerator projection remains a separate versioned
contract and must preserve the actor-side operational action-object boundary.
