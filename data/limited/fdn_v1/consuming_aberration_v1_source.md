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
win, and existing additive layers follow. Checked P/T queries propagate overflow
without changing the queried world;
legacy optional base queries return no numeric value on that overflow.
Native zone transitions now use the scoped final-scene check described below;
source checks are not executed transition qualification.
No CardDef field or serialized state/enum variant is added.

The cast trigger uses the existing pre-SBA generic capture. finalize_cast
captures immediately after genuine paid cast publication; resolution-cast
children capture before later parent instructions. Copies are not casts.
Pending/stack roots retain captured controller and original source contract.
The existing RevealUntilCardTypeAndMill operation binds the actual original
library prefix, publicly reveals only those cards, and lets its owner order
that batch. This printing adds a scoped required waiting/answered stage and
consumes an exact existing StandardSelection guard for the answered prefix.
The existing move-only native Tutelage row owner also selects only this exact
printed trigger, keeping its complete original row/context/pending/remaining-stack
capture and loan cleanup. Tutelage's draw publication guard remains specific to
Tutelage. Public generic begin/resume and synchronous execution cannot substitute
for that native handoff. Other printings retain their existing mill stages.
A later source departure, control change or suppression cannot retarget the original opponent.
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
visible printing/root. The new family name checks do not inspect hidden face-down
printing identity; separately captured face-up roots/history still refuse. A neutral
projection-only primitive verifies both hidden printings match the older neutral
face-down row, without claiming a legal paid manifest/disguise route for them.
No owning/model/Human/transport/Full/profile admission is added.

Fourteen native source fixture groups cover both-seat real3UB payment, ordinary
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

Scoped final-zone P/T source repair: the shared event allocator/reset is now
extracted from publication and reused to materialize the complete simultaneous
scene before the checked full layer7 query. It uses the actual final replacement
destinations, controller/owner rosters, incarnation resets, ending native copies,
entry characteristics, frozen counter replacement inputs, Giada/Kinship counters,
owned chosen-color/type entry constructors, and exact attachment incarnation.
The scene is discarded; it does not execute a public step/resume, event observer,
replacement pass, or linked return. Departing power/toughness are captured from
the original prebatch scene and used by the existing LKI writers. This avoids
querying a temporarily overflowing prefix when a suppressing Aura and Aberration
leave simultaneously. Every batch member must materialize before final PT is
checked; the check does not approximate a graveyard count or filter out a departing
modifier from an otherwise intermediate scene.

Public direct commit/propose, effect move batches, sacrifice groups, SBA groups,
owned initialized permanent entries, copied/token entries and their explicit
attachment rosters reach this check. Ordinary mixed replacement bookkeeping is
staged before it, so a refused batch does not consume its prevention shield.
The existing privately prepared Crystal/combat writers retain their original
permits and replacement ownership. Deferred linked returns remain separate native
entry batches after end_batch, each with its own real entry choices and check;
no future return is guessed or folded into the original simultaneous scene.
Actual-world selection includes suppressed/face-down/copy-hidden battlefield
Aberrations and actual entering definitions. Catalog presence alone does not
allocate a preview; source-absent/default catalogs retain the original path and
serialized/hash field shapes.

Native cast announcement and abort also use extracted original materializers.
A graveyard cast checks its actual Stack placeholder/provenance/reset scene before
installing it, including the negative-power underflow boundary. An unsuccessful
Madness cast checks its exact abort destination/reset/permission scene before
removing the placeholder or inserting the card into the graveyard. Stack remains
an engine action and is not admitted as a generic zone-change effect destination.
Child completion, trigger collection and priority reset run once, after the
materialized transition has passed the check. No public permit, serialized proof,
state cache or owner reconstruction is introduced.

Six added source groups cover real paid Think Twice first-land overflow,
waiting/answered ordering restores, two orders of native simultaneous departures
over a genuinely paid Aberration and Witness Protection (original LKI retained),
surviving-source exposure via direct/proposed/batch moves, a labelled mixed native
prevention primitive that remains unconsumed, a genuine paid Faithless Looting
Madness offer/target/abort with no remaining red source, and a legal public
Flashback announcement at a labelled native negative-power boundary. Large counter,
boost and mana-availability setups are explicit native boundary fixtures, not
claims that those values were paid for by printed spells. All eight pre-existing
source test bodies remain unchanged apart from formatting. This work does not
qualify other non-zone P/T mutators or complete consumer support.

All new Rust remains uncompiled and unrun. The source note describes implemented
coverage, not observed native PASS or Full admission. Pinned formatting, source
extraction audits,82 Python deck tests and6 frozen-pool tests do not replace the
future genuine native transition/restore checks.
