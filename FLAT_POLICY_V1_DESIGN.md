# Flat policy v1 design candidate

Status: design candidate only. This document does not admit a new feature,
model, checkpoint, training, evaluation, or performance contract.

## Purpose

The current Python schema-v5 / Net8 path is the correctness reference, but its
canonical JSON, repeated hashing, Python tensor construction, and row-wise
model execution are not a viable actor hot path. Flat policy v1 is a proposed
kernel-owned, typed, ragged representation for a native trainer. It must retain
actor-relative privacy and exact action binding while removing JSON, stable
strings, content hashes, Python, and per-decision allocation from rollout.

The provisional CUDA 2048/128/64 microkernel is not this contract. Its results
may guide capacity planning, but neither its synthetic fixture nor its weight
layout constrains this design.

## Non-negotiable boundaries

- The encoder reads the private `FastActorSessionV1` state and its exact current
  candidate vector. It never accepts caller-constructed action semantics.
- Unknown opponent hand identities and unknown library identities must not
  affect any output bit. Actor-visible known-hand and known-library identities
  are admitted only through the engine knowledge tables.
- Every encoded decision binds episode, environment revision, physical decision,
  policy substep, acting player, ordered legal-action count, and candidate order.
- Rollout inference may omit stable/display strings and audit hashes. Reproducible
  artifacts compute trajectory commitments off the hot path from compact typed
  records.
- Only active row prefixes are copied to inference or committed to artifacts.
  Unused caller capacity is outside the contract, is never hashed or copied, and
  is poison-filled in tests to prove a shorter decision cannot reveal rows from
  a previous actor or episode.
- The representation is ragged. It must not zero-fill a maximum card-pool-sized
  tensor for every decision.
- All dimensions, enum orders, normalizations, card tokens, model parameters,
  initializer, loss, sampler, optimizer, and checkpoint byte order are versioned.
- No production claim is permitted until runner, trainer, checkpoint resume,
  greedy evaluation, and sampled evaluation consume the same accepted contract.

## Lossless typed representation and accelerator projection

The kernel-owned record is a compact typed contract. The accelerator projection
is a separate, generated transform. Generic `f32` rows are not the source of
truth: enum ids, signed integers, masks, optional values, variable-length
references, and role-specific relation payloads remain typed until the
projection step. A candidate projection width is accepted only after every
admitted typed value has a declared mapping and range; widening or changing the
mapping creates a new contract identifier.

The dimensions below remain experiment candidates, not accepted values. In
particular, the current schema-v5 non-hash widths are 123 globals, 98 object
features, 41 edge features, 99 action features, and 25 action-reference
features. A smaller projection needs an explicit injective typed mapping rather
than silent truncation or opaque bit packing into `f32`.

```text
FlatDecisionV1
  binding: FlatDecisionBindingV1
  globals: FlatGlobalsV1
  objects: ragged FlatObjectCoreV1[]
  relations: ragged FlatRelationCoreV1[]
  actions: ragged FlatActionCoreV1[]
  action_refs: ragged FlatActionRefV1[]

FlatDecisionBindingV1
  episode_id: u64
  environment_revision: u64
  bound_policy_step_count: u64
  physical_decision_id: u64
  bound_physical_decision_count: u64
  substep_index: u32
  substep_count: u32
  acting_player: u8
  decision_kind: u8
  legal_action_count: u32

FlatObjectCoreV1
  card_token: u16
  group: u8
  actor_visible_ordinal: u16
  typed dynamic fields and masks (layout generated and versioned)

FlatRelationCoreV1
  source_object: u16
  target_object: u16
  role: u8
  payload: FlatRelationPayloadV1

FlatActionCoreV1
  kind: u8
  typed scalar/enum payload
  ref_start: u32
  ref_len: u16

FlatActionRefV1
  action_index: u32
  role: u8
  order_index: u16
  associated_order: u16
  card_token: u16
  object_index: u16

AcceleratorProjectionCandidateV1
  globals: [f32; 128]
  object_features: [f32; 16]
  relation_features: [f32; 8]
  action_features: [f32; 64]
```

The checked-in partial slice uses `FlatActionDecisionBindingV1`, which also
binds the slice, reference-role, card-token, and commitment versions;
`KERNEL_CARDDB_HASH`; both physical/policy counters; and the first 128 bits of
SHA-256 over the exact ordered compact actions, references, and referenced
object meanings. That truncated digest is a versioned stale-result guard, not
an authorization primitive or a collision-proof artifact commitment. A scored
index is executable only through `consume_current_flat_action_slice_v1`, which
recomputes and compares the complete binding before calling the ordinary step
path.

Reference roles have an explicit v1 mapping independent of Rust discriminant
layout: source=0, candidate=1, card=2, attacker=3, blocker=4,
target-object=5, cards=6, pending-sources=7. For `OrderTriggers`, each
pending-source row is a parallel-vector entry: `order_index=i` names source
`pending_sources[i]`, while `associated_order=order[i]` preserves the raw
permutation entry consumed by `Action::OrderTriggers`. It is deliberately not
the inverse "placement of source i". The non-self-inverse vector `[2, 0, 1]`
is the fixed disambiguating test.

`FastActorSessionV1` owns the only production entry point. It reads its private
`GameState`, private current candidate vector, and current revision directly;
there is no public `(GameState, caller-provided semantics)` encoder. Caller-owned
buffers hold the ragged rows. Encoding is two-pass: pass one validates every
enum, range, actor-visible reference, unique object mapping, row count, and
capacity without writing a row or publishing a length; pass two is infallible
for that unchanged session revision and publishes active lengths only after all
rows are complete. Capacity-sufficient calls allocate nothing.

Encoding fails closed on insufficient capacity, an unknown enum value, a
non-finite projected feature, a stale decision binding, an object reference
outside the actor-visible table, an unsupported card token, a checked integer
conversion failure, or any session revision change. Legal actions never encode
a hidden object using an absent/private sentinel. Optional absence is represented
by the typed action variant; a hidden or unresolved legal reference is an error.

The current implementation is correctness-first and is not yet a performance
candidate. Preflight repeatedly scans the complete arena, performs nested
canonical-index searches, repeats resolution during emission, and recomputes
the same work during consume. This is at least quadratic in relevant paths and
can scale with total arena size rather than referenced-object count. The direct
encoding throughput gate remains blocked until a later refs-only resolver and
private cached binding/commitment redesign is independently validated. This
document does not authorize that redesign in the present slice.

CPU encoding may use array-of-struct caller buffers, but the batched inference
boundary is versioned structure-of-arrays storage with explicit decision and
action offsets. Conversion copies active prefixes only, uses checked offsets and
alignment, and performs no per-decision allocation. Integer fields remain typed
through batching; accelerator float projection happens in the declared packing
kernel rather than by lossy Rust casts.

### Object groups

The first full-state contract must preserve, or explicitly map a typed superset
of, the current 20 actor-relative groups. Combining groups merely to reduce the
pooling width is not a lossless encoder change. Candidate canonical order:

1. self hand
2. self battlefield
3. opponent battlefield
4. self graveyard
5. opponent graveyard
6. public exile
7. stack source
8. combat context
9. continuous-effect context
10. permission context
11. attachment context
12. historical stack target
13. combat-block context
14. pending context
15. private actor-visible context
16. known self-library identity
17. known opponent-library identity
18. known self-hand identity
19. known opponent-hand identity
20. historical paid-cost reference

Adding command-zone rows or another source kind is a typed superset and requires
a contract bump unless the accepted v1 layout already reserved and tested it.

Unknown hand/library cards contribute only actor-visible aggregate counts in the
global vector; they never produce identity-bearing object rows. Object order is
derived only from actor-visible zone order, revealed-hand order, known-library
position, and public context order. Raw arena ids may be used transiently to
validate an incarnation but are never emitted and never choose output order.

### Relations

Relations are required in the first full scorer-capable contract. Omitting them
would collapse states with the same object multiset but different attachment,
stack-target, blocker ordering, effect, permission, pending-choice, or known-card
topology. The accepted vocabulary must cover at least the current roles:
attachment, stack target, combat attacker, combat blocker, continuous-effect
source, continuous-effect affected object/player, permission, pending context,
private actor-visible context, known library, known hand, attached-to, exiled-by,
and paid-cost reference. Action references are a separate ragged table rather
than state relations.

`FlatRelationPayloadV1` is a role-tagged typed union. Its variants carry the
orders and values meaningful for that role. The continuous-effect variant must
preserve layers, timestamp/order, duration, controller and affected-player mask,
global flag, power/toughness deltas, haste, optional set power/toughness,
color/subtype/keyword additions and removals, ward delta, minimum blockers,
landwalk changes, prevention color mask, and cannot-be-prevented flag. Permission
payloads preserve holder, play-versus-cast, incarnation, expiry kind, and holder
turn state. Stack, combat, known-card, paid-cost, pending, and private payloads
preserve their public order/subrole fields. A missing role or out-of-range value
is an encoder error, not a silently ignored edge.

The provisional eight-float accelerator relation projection is accepted only if
each typed variant has a reviewed mapping. If eight floats cannot retain the
declared information and useful model access to it, the projection widens; the
typed record is never weakened to satisfy the candidate width.

### Card tokens

The partial v1 slice uses the already-dense schema-v5/runtime-registry mapping:
`card_token = card_db_id + 1`, with zero reserved for absent/unknown. It does
not use the older schema-v5 hashed-card feature as identity. The binding carries
both `FLAT_ACTION_CARD_TOKEN_MAPPING_VERSION_V1` and `KERNEL_CARDDB_HASH`, so a
registry reorder, replacement, or growth cannot silently reinterpret a token;
it changes the bound registry hash and must be rejected by consumers,
checkpoints, and run manifests.

### Current executable subset

`FlatActionKindV1` deliberately mirrors all 27 schema-v5 scalar kinds so fixed
mapper vectors can cover reserved values. The private session encoder is
narrower: `ChooseEffectColor`, `ChooseEffectNumber`, and
`FinishTargetSelection` are schema-only reservations with no executable
policy-v5 `Action`, and therefore fail closed before commitment or publication.
They are not claimed as consumable kinds. Aggregate combat and ambiguous
semantics are likewise rejected; policy-v5's binary attacker/blocker inclusion
decisions are the executable combat representation.

## Candidate model

The first capacity model should remain small enough for batched inference and
full training to clear the end-to-end gate while retaining explicit card and
relation structure:

```text
card embedding: dense token -> 16
object encoder: (embedding 16 + features 16) -> ReLU(32)
object pooling: sum by accepted generated group count (20 in current mapping)
relation encoder: (source hidden 32 + target hidden 32 + features 8) -> ReLU(32)
relation pooling: sum by relation role
state encoder: (globals + object pools + relation pools) -> ReLU(64) -> ReLU(64)
action encoder: (features 64 + four card embeddings + referenced object pools)
                -> ReLU(64)
policy scorer: (state 64 + action 64) -> ReLU(64) -> 1
value head: state 64 -> ReLU(64) -> 1
```

Relation-role count and the exact action input width remain experimental until
the typed encoder fixture reports Burn and Rally shapes and the typed-to-float
projection is accepted. The implementation must report exact parameter count
and reject checkpoint shape drift. The 2,048-decision Rally artifact observed
p50/p95/max shapes of 35/67/73 objects, 2/8/22 relations, 3/9/13 actions, and
3/9/18 total action references, but it is workload-only Rally/Rally provenance,
not a structural bound for every Burn/Rally state. `OrderTriggers` currently
admits seven references in one action, so no four-reference fast path may define
the contract.

## Training semantics

The first native trainer preserves the current terminal-only objective before
testing richer algorithms:

- sampler: accepted fixed categorical version and hierarchical action seeds;
- one learner term per completed physical decision;
- policy log probability is the sum across that physical decision's substeps;
- value is taken from the first substep;
- terminal return is `-1`, `0`, or `1` for the learner seat;
- advantage is `return - stop_gradient(value)`;
- policy loss is `-joint_log_probability * advantage`;
- value loss is `(value - return)^2` times the versioned value coefficient;
- batch loss is the sum divided by physical-decision term count;
- optimizer is versioned Adam with explicit epsilon, beta, bias correction,
  step order, f32 accumulation policy, and finite-value rejection.

Rollout stores compact encoded inputs, selected indices, physical-decision group
bindings, and returns. Learner forward/backward recomputes from the frozen batch
weight revision; weights publish atomically only after a complete finite update.

## Correctness gates

1. Schema contract digest and generated card-token table are stable and tested.
2. Direct encoder matches schema-v5 actor-visible scalar/action semantics on a
   provenance-bound Rally corpus wherever the contracts overlap.
3. Hidden-state noninterference mutates unknown opponent hand/library identities
   and requires byte-identical active encodings and outputs. Tests also poison
   all inactive buffer tails, reuse them across actors and shorter decisions,
   and prove no tail byte is copied, hashed, or made active.
4. Candidate reorder changes the corresponding ordered action/reference rows
   and the candidate-order commitment; a binding captured for either order is
   rejected by the other. Canonical actor-visible object rows remain independent
   of candidate traversal order, while reference object indices continue to
   identify those stable canonical rows.
5. Snapshot/restore and independently scheduled actors reproduce encoded rows,
   selected actions, terminals, and compact trajectory digests.
6. CPU reference and accelerator implementations match independent golden
   vectors across zero/tail relations, legal widths, near-tie logits, changing
   weights, loss, gradients, Adam moments, and updated parameters.
7. Async failure injection proves streams/graphs are drained before pinned or
   device buffers are released.

## Performance gates

All rates are designated-host, clean-revision, raw-artifact results with fixed
workloads and interference bounds.

- Direct typed encoding: minimum 1.5 million learner decisions/second at the
  declared thread count over provenance-bound Rally decisions.
- Sampler: minimum 5 million selections/second at one thread and 50 million at
  the declared parallel lane over the same width histogram. These are capacity
  floors, not training rates.
- Rollout environment plus encoding plus sampling, with inference latency
  injected at the measured batch distribution: minimum 5,500 games/second and
  zero invalid terminals. This leaves only bounded margin over the provisional
  4,550 games/second integrated target and therefore triggers redesign if the
  learner cannot run concurrently.
- Full learner recompute + loss + backward + Adam: minimum sample throughput is
  `573,000 * training_epochs_per_sample`, with at least 25% measured headroom
  while the inference service remains live.
- Integrated trainer: one-sided lower confidence bound above 4,550 natural
  games/second on the provisional internal gate, followed by the formal
  same-host matched-runtime requirement of at least 1,000x once its sealed XMage
  denominator is available.
- Time-to-learning is a separate promotion gate: fixed seeds and evaluation
  protocol must show a predeclared improvement over the uniform policy. Raw
  games/second cannot substitute for learning.

## Artifact and recovery gates

- Checkpoints contain architecture/feature/token/sampler/loss/optimizer versions,
  complete f32 weights and Adam moments, optimizer step, rollout weight revision,
  seed schedule, episode frontier, aggregate outcomes, and parent commitment.
- Publication is new-generation-only and crash-consistent; resume validates the
  complete authoritative chain and ignores/rejects uncommitted staging by the
  versioned recovery contract.
- Runner and both evaluators load the same checkpoint parser and reject cross-
  contract artifacts before launching an environment.
- A clean clone reproduces a bounded train, interrupted-resume train, greedy
  pair, sampled pair, and their expected logical digests without a Mage checkout.

## Experiment order

1. Preserve the reproducible old-encoder artifact and its workload-only Rally
   width/shape record. Its performance gate is still invalid under the declared
   timing/interference bounds and must not be relabeled as closed.
2. Implement a deliberately partial `FlatActionDecisionSliceV1`: exact private
   session binding, ordered typed action scalars, canonical actor-visible
   referenced-object resolver, and ragged action references. It is not a state
   encoder or scorer input. Validate every reachable policy-v5 action variant,
   `OrderTriggers` lengths through seven, inclusion actions, stale/reorder and
   insufficient-capacity failures, hidden-identity noninterference, poisoned
   tails, semantic parity, and zero allocation with admitted capacities.
3. Inventory every schema-v5 model-input field formerly represented explicitly
   or through canonical digest bytes. Add typed globals and the lossless object
   core; do not freeze 128/16 until the inventory and range proof pass.
4. Add the required role-specific relation union and parity/privacy fixtures.
   Only this globals+objects+relations+actions record may be named a full flat
   policy encoder.
5. Generate and validate the accelerator projection, then measure full encoding
   shapes/rate on all ordered Burn/Rally matchups plus synthetic structural tails.
   Revise candidate dimensions with a new contract identifier if a mapping or
   predeclared gate fails.
6. Implement CPU model/loss/Adam goldens and checkpoint byte layout.
7. Implement safe accelerator forward and training services against those
   goldens.
8. Integrate rollout, learner, recovery, runner, and evaluators.
9. Run the sealed end-to-end speed and time-to-learning gates before resuming
   card breadth.
