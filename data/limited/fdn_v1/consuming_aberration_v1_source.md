# Consuming Aberration native Partial source

Frozen printing: FDN238, Scryfall `bc2b28fd-66b0-457c-80ea-7caed2cc7926`,
Oracle `9b55fb72-237d-4935-b645-8ebc6eb4140e`. Appends local ID467 as Partial;
all preceding definitions/capabilities and the frozen booster membership remain.
The combined registry has468 definitions/276 FDN rows, with the frozen pool
reporting245 Full,16 Partial,25 Missing.

Pinned implementation reference: Mage `a5c90fe180021e70e2a644ade00eeab07f857a40`,
`Mage.Sets/src/mage/cards/c/ConsumingAberration.java:26-38,65-90,100-118`.
The printed3UB Horror has a characteristic-defining ability setting both P/T
to the number of cards in opponents' graveyards in every zone. Its mandatory
own-spell cast trigger is untargeted: each opponent reveals through the first
land (or their entire remaining library), then puts that whole prefix into
their graveyard. The native engine has exactly two seats; PlayerRef::Opponent
therefore implements each opponent completely in its supported game domain.
There is no activation or optional choice to mill.

The definition-owned CDA lives in existing explicitly hashed engine.rs. It
reads the effective copied CardDef, applies printed-ability suppression, and
uses the current controller on battlefield/stack and owner in other zones.
It counts every physical graveyard card, regardless of card type, excluding
transient tokens and spell copies. Checked native i32 conversion has no i16
narrowing or saturation. Earlier layer7b base settings and face-down forms
win, and existing additive layers follow. Checked P/T paths propagate overflow
before a mutation; legacy optional base queries return no numeric value on
that overflow. No CardDef field or serialized state/enum variant is added.

The cast trigger uses the existing pre-SBA generic capture. finalize_cast
captures immediately after genuine paid cast publication; resolution-cast
children capture before later parent instructions. Copies are not casts.
Pending/stack roots retain captured controller and original source contract.
The existing RevealUntilCardTypeAndMill operation binds the actual original
library prefix, publicly reveals only those cards, and lets its owner order
that batch. This printing adds a scoped required waiting/answered stage and
consumes an exact existing StandardSelection guard for the answered prefix.
Other printings retain their existing mill stages. A later source departure,
control change or suppression cannot retarget the original opponent.
Validation is the existing native mutable-world structural contract, not
cryptographic authentication against coordinated rewrites of an entire world.

Actual behavior is inline in engine.rs, trigger.rs and effect.rs, already
explicit inputs to current producer identities. build.rs binds its real
ordinary-permanent program, trigger recipe and all-zone static semantic
recipe. rules_vector_v1/sources.rs is an existing explicit extractor input;
its new native CDA fact and reveal-mill body remain truthfully opaque in V1.
Keyed-name inventories classify the changed native paths. All three native
inventory assertions and Python exact Partial/refusal inventories append467.
Old RL card rows and public native appendices refuse this unrepresented
printing/root; no owning/model/Human/transport/Full/profile admission is added.

Seven native source fixture groups cover both-seat real3UB payment, ordinary
publication and lack of self-trigger, paid Think Twice/permanent casts,
first-land/empty/all-nonland libraries, original public prefix and hidden
suffix, graveyard order, waiting/answered restore and readonly forgeries,
paid Bolt source death, paid Self-Reflection copies, paid Witness Protection
suppression and Broken Wings removal, and paid Counterspell. Control changes,
all-zone movements, transient token exclusion and numeric boundaries are
explicitly labelled native primitives, not claims of paid control spells or
large paid counter placements. There is no claimed nonowner spell-casting
fixture or paid stack-copy producer. A frozen-meaning audit and Python exact
printing/deck-refusal case accompany these groups.

This is source-only implementation. All new Rust is uncompiled and unrun.
Formatting, Python and static audits are not native qualification or evidence
of complete consumer support.
