# Information-set export visibility v2

Issue #568 fixes an export contract, so consumers opt in explicitly with
`mtg_kernel::infoset_v1::export_v2`. Its state bytes and payload envelope carry
`x_kernel_infoset_v2`. The existing bridge continues to call the historical
`export_v1` and emits `x_kernel_infoset_v1`; frozen instruments are unchanged.

The v2 reducer reads the identity visibility recorded when each zone move or
cast committed. A face-down departure is unidentified: it creates no named
hand seam or departure, ends one fact of each possible name, and updates the
survivor tree as an unidentified departure. A later turn-up or zone reset
cannot retroactively identify that departure. Hidden history references use
unnamed tombstones rather than the card's true definition.

Visibility provenance lives privately beside the observation journal. It is
excluded from the adapter's journal notes, actor inputs, serialized game
state, equality and state hashes. Foretell records its identity visibility
after the generic exile move, before control returns to a decision. Captured
visibility survives in-memory cloning. Deserialization does not reconstruct
it: v2 refuses a history with missing provenance rather than inferring old
visibility from a current object. Existing serialized snapshots remain valid
for their historical v1 contract.

The historical move-to-hand helper also emits a reveal for concealed exile.
V2 ignores reveal notes anchored inside a move that concealed the identity;
a genuine reveal after that move commits remains readable. Any explicit
reveal sharing the concealed move's exact anchor is conservatively ignored,
since the old journal cannot distinguish it from the helper's false reveal.

The canonical allocator currently represents unknown slots only in hands and
libraries. V2 therefore returns the same R2 refusal for every active public
face-down object whose identity the viewer cannot inspect. This check occurs
before name-sensitive invariants, allocation and refusal predicates. Once the
object becomes public, ordinary export can resume, subject to the existing
export restrictions. This change does not claim support for searching while
an unknown face-down public object is active.

Use `PayloadV1::new` with the v2 result to construct a versioned envelope. The
envelope and state digest both bind the schema. Existing v1 envelopes keep
their bytes and digest. Switching a production consumer to v2 is a separate
instrument/versioning decision; this repair does not change a training run,
open a holdout, or establish an effect on playing strength.
