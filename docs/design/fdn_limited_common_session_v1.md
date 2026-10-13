# Limited common JSONL playback, source preparation

Base: 3dde78854a99bcc68121a221a5d753961b3649a0. The reviewed parent
fe3c35635c59e555e47bc2f92e9bcae818c48b5d was subsequently merged without
conflicts. Its actual unaccepted 437-definition candidate is preserved verbatim;
this lane adds no card metadata, IDs or profile derivation. This is a separate
`kernel_limited_common_jsonl` protocol, schema 1, and a separate executable
`kernel_limited_common_env`. Frozen `kernel_limited_jsonl` schemas 1 through 4,
RL schemas, model inputs and existing Human projectors keep their identities and
refusals. No registry, catalog/profile literal or rules-vector admission changes.

The new reset uses the actual EnvironmentV2 shuffled deck-pair builder and its
seven committed opening draws. Name-based decks use the existing full-support
preflight and 40 to 10,000-card input bound. The caller binds the running
`KERNEL_CARDDB_HASH`, selects the starting seat, seed, episode and positive
decision limits. Full engine priority, Foundations combat and London mulligans
are selected before the first advance. A distinct appended backend mode inherits
the current complete V40 family validators, source-history transports,
observations, candidate derivation, normalization and application. It does not
select a frozen flat tensor or use the 60-card Standard explicit-deck reset.

Only London, legend, attack destination and ordinary Foundations binary damage
range decisions receive the additional origin adapter. It requires the selected
London/Foundations extensions, an exact actual engine decision on a read-only
probe, unchanged core bytes, the correct actor and surface stage, and complete
ordered regenerated candidates. Other decisions use the existing V40 origin
authority. Labels cannot supply missing private authority. Canonical combat
assignment roles are included before handle canonicalization; exported London
counts and combat packets contain only established public facts and safe handles.

The process is a trusted actor-facing two-seat host, not a remotely authenticated
player service. Each reply projects only the actual current actor's hand and
authorized knowledge, with all current typed common families including Herald,
Akroma, Fishing and Twinflame where the running catalog and representations
support them. A network frontend must route a reply only to that actor. There is
no request-selected observer, raw action semantic, source contract, arena ID,
private binding, snapshot JSON or hidden historical card fallback.

Requests use a tagged `request_type`, `schema_version: 1` and a nonempty
`request_id` (maximum 256 bytes):

* `reset`: `expected_card_db_hash`, `episode_id`, `env_seed`,
  `starting_player`, two `decks` (existing name/count format),
  `max_physical_decisions`, `max_policy_steps`.
* `current`: return the current actor's safe prompt.
* `step`: `episode_id`, `authority_generation`, `actor`, `prompt_seq`,
  `action_index`, all taken from the current reply.
* `snapshot`: save an opaque, process-local snapshot handle (at most 64).
* `restore`: `snapshot_id` from this owning game.

Duplicate JSON keys and unknown fields refuse. Immediate identical request retries
return their receipt; conflicting reuse refuses. The last accepted action also
remains idempotent after a read or snapshot request. A restore authenticates the
original snapshot and derived combat scan, then uses the reviewed V40 checked
monotonic revision rebind. Owning allocation plus a nonserialized generation
outside snapshots binds pending and accepted prompts. Restore and reset
invalidate every old action. Prompt and snapshot counters do not roll back.
Snapshots never cross processes or reset boundaries. Terminal transport omits
raw engine diagnostic strings.

An unsupported current input or unsafe/indistinguishable label is an explicit
`unsupported` state with no executable prompt, not a policy fallback. Divine's
variable-cardinality cast decisions, unsupported ordering/hidden producers and
future Ajani V42 remain outside this source. Model tensors, action vocabularies,
search state and hidden-state sampling are unchanged and remain unqualified.
Four-card preparation is not registration or gameplay qualification.

Prepared tests cover real two-seat London keep/mulligan/bottom/full-priority
progression, opponent-hand privacy, actual action retries, repeated owning
restore, stale/foreign action and catalog refusal, and private-origin corruption.
The four-card catalog reset group is enabled against the parent's actual staged
records, and an actual paid Herald fixture traverses public projection and restore.
Registered physical combat setup traverses real attacker/blocker scans and
Foundations damage allocation with owning restore. Existing paid mixed-family
source fixtures remain inherited and ordinary V40 state/labels retain a parity probe.
Native execution and type/lint qualification belong to the parent lane and have
not been run for this source.
