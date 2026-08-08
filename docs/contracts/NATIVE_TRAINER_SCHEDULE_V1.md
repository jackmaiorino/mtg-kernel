# Native trainer schedule v1

Status: frozen cross-language derivation contract. This checkpoint implements
the stateless seed/seat derivation and its record-facing constants; the
partial-group and ordinal rules below are normative requirements for the
future stateful native scheduler, not enforcement claimed by this module. This
schedule does not change the public scored-rollout seed or fixed-seat behavior.

Identity: `mtg-kernel-native-trainer-schedule-sha256-v1`.

Python reference seed identity: `kernel-python-rl-trainer-sha256-v2`. The
native schedule has its own record identity but deliberately retains this exact
reference string inside the hash's `version` atom. It claims bit-exact seed and
seat reproduction, pinned by one golden file consumed by Rust and Python tests.

## Episode and seat schedule

The episode index is the persisted, absolute, zero-based trainer episode index.
It is never renumbered by a resume, invocation, worker, lane, session, chunk,
round, batch, timing source, or device.

- even episode indices, including episode zero: P0 is learner;
- odd episode indices: P1 is learner;
- pair index: integer floor of `episode_index / 2`;
- paired episodes share one `train-env` seed.

Seat is not a separate action-seed field. Episode parity fixes the physical
seat, while the learner and opponent namespaces domain-separate actor roles.

## Atom and seed encoding

Each atom is:

`u32_be(tag_utf8_len) || tag_utf8 || u64_be(payload_len) || payload`

The hash begins with `version` and `namespace` atoms. Every field then appends
`field-name` and a typed value atom. Required atom tags are `version`,
`namespace`, `field-name`, `u63`, and `str`. Integers are exactly eight-byte
big-endian values in `[0, 2^63 - 1]`; strings are UTF-8. The seed is the first
eight SHA-256 digest bytes interpreted big-endian and masked with
`0x7fff_ffff_ffff_ffff`. The checked-in string-field probe pins the Python
`str` branch even though all six live v1 namespace declarations use integers.

Ordered namespaces and fields:

- `model-init`: `base_seed`;
- `train-env`: `base_seed`, `pair_index`;
- `train-learner-action-group`: `base_seed`, `episode_index`,
  `learner_physical_decision_index`;
- `train-learner-action-substep`: `group_seed`, `substep_index`;
- `train-opponent-action-group`: `base_seed`, `episode_index`,
  `opponent_physical_decision_index`;
- `train-opponent-action-substep`: `group_seed`, `substep_index`.

Normative future-scheduler rule: learner and opponent physical-group ordinals
each start at zero per episode and advance once only after that actor's
complete physical group. The Rust derivation API accepts substep indices as
`u32`; raw or wire inputs use `checked_native_trainer_substep_index_v1`, which
rejects values at or above `2^32`. The u32 value is then encoded as an
eight-byte `u63` atom. Changing one group's width cannot shift a later group
seed.

## Failure and publication boundary

The future stateful scheduler must preflight a complete physical group against
physical and policy caps before its first substep. A cap, halt, truncation,
observer failure, or terminal before the declared final substep invalidates the
episode and update. It cannot publish a group ordinal, optimizer state,
checkpoint, or trainer record. This derivation-only checkpoint records that
rule but does not claim to enforce scheduler state transitions yet.

The checked-in golden file's SHA-256 is
`6b2e1edbbe49b4e02f98794f9057f5c2bb8e3079d2ba8cb3e2a4b9ea6c34867c` and is a
mandatory trainer-record field. A change to this document, algorithm,
namespaces, field order, seat rule, integer domain, partial-group rule,
scheduler exclusion, or golden vectors requires a new schedule version.
