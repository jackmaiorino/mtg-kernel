# Flat policy v1 typed-state encoder validation

Date: 2026-07-18

Status: bounded engineering checkpoint candidate, not a scorer, trainer, or
science-release certificate

Base source parent: `13644f30d33c7ab80f01c9d7c71fe59980f0c285`

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

The ten forbidden leaves are absent from public model rows. Raw arena ids and
zone-change counters are used only for private reference validation. Names,
display strings, stable strings, and diagnostic hashes are not stored in the
typed model tables.

## Focused evidence

The focused Rust tests validate:

- exact equality between the complete encoder's action binding/actions/refs/
  operational action objects and the pre-existing action-slice producer;
- exact row counts and typed debug digests for Burn/Burn, Rally/Rally, and
  Burn/Rally initial decisions, plus the first relation-bearing Burn and Rally
  decisions;
- every reached decision in deterministic randomized Burn and Rally rollouts,
  including relation endpoint range checks;
- snapshot, clone, restore, and stale expected-decision behavior;
- no caller-buffer publication for short capacity, including a separate exact
  check for each of the eleven ragged tables;
- absolute-seat swapping of a bounded initial fixture, including an effect
  source and affected opponent, produces identical actor-relative globals,
  objects, auxiliary rows, and relations;
- mutations to names, hashes, raw arena ids, and raw zone-change counters do
  not change public model tables, while card identity does;
- every modeled continuous-effect field changes the typed effect signature,
  while the operational timestamp alone does not;
- a source-less player-only effect remains represented;
- warmed complete encoding across multiple Burn and Rally decisions performs
  zero allocations.

The generated Python tests independently recompute the enum maps, inventory,
source hashes, payload digest, and checked-in outputs. The inventory test
rejects missing, duplicate, or differently classified authoritative leaves and
requires all forbidden leaves to map to `absent`.

## Validation commands

The following commands passed from this worktree with the locked dependency
graph. `CARGO_TARGET_DIR` was an isolated external target directory, and the
existing locked Python environment was used with `PYTHONPATH=python`.

```powershell
cargo fmt --all -- --check

python python/tools/generate_flat_policy_v1_goldens.py --check
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
or private context. The generated inventory proves complete one-to-one leaf
classification; final contract promotion still requires an independent source
audit of the semantic destination mapping before freezing an accelerator
projection.
