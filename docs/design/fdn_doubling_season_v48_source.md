# Doubling Season source preparation

Owned base: 3818ff3b4cb6e542e595c8606f57791553785251. No registry, ID,
profile, generator hash, native, model, sampler or Full admission.

Frozen printing pool contains Doubling Season. Primary recipe is pinned Mage
a5c90fe180021e70e2a644ade00eeab07f857a40,
Mage.Sets/src/mage/cards/d/DoublingSeason.java. Wizards' Foundations release
notes confirm effect-token characteristic preservation, entry counter placement,
loyalty cost exclusion and multiplicative independent sources:
https://magic.wizards.com/en/news/feature/foundations-release-notes.

This first checkpoint appends a definition-owned token/counter descriptor, exact
three-root meaning, and a checked replacement input sampled from active source
incarnations/current controllers/effective printed abilities. Counter effect actor
and recipient controller are separate. Shared +1/+1 placement and the combined
+1/+1/lifelink/stun leaf use the recipient controller; loyalty effects and payments
have separate entry points. Existing Talent scaling remains actor-relative and
old absent descriptors add no catalog digest token. No new GameState fields or
committed event variants are introduced at this checkpoint.

The descriptor is deliberately unregistered. This is not the complete executor.
Remaining required implementation includes the complete shared token blueprint
and simultaneous expanded roster; copy, tapped/attacking, Role, Incubate and haste
tails must initialize every additional token before entry observers. A permanent
spell copy becoming a token is not a token-creation effect. Existing exact
Krenko/Arahbo/Faebloom/Ajani/Fishing operation count proofs need original-event
replacement inputs, not a later query or relaxed raw count assertion.

Other counter placements remain to be routed: entry/X/Bestow/raid/loyalty,
counter transfers and kind-specific oil/bait/time/lore/minus counters. Entry
replacement sources must be sampled before an entire simultaneous batch so a
newly entering Season cannot affect a different member because of commit order.
Loyalty payments keep their cost origin; rule actions must retain their actual
origin. Root's Panda replacement-aware verifier is a separate prerequisite.

All current safe observations refuse the selected descriptor. V48 is reserved
for a future distinct public/private producer only if needed; no predecessor
representation is relabeled. Frozen rules vectors explicitly refuse the new
descriptor. Meaning describes the exact printed recipe and explicitly reports
safe consumer admission false, rather than claiming executor qualification.

Prepared tests use registered real counter recipients and future actual printed
Season casting, effect entry, control transfer, multiple copies, restore and
readonly overflow/refusal. Future printing groups are ignored until serial
registration. All Rust tests are uncompiled/unrun. Pinned full formatting and
diff checks are the only local Rust checks authorized in this lane.


## Shared roster checkpoint

Mage blob: 29542ba45c79761e312862564fc90a4087f654b5. Source SHA-256:
72181a61bc8290fe500a73ffccd70f93a0b51c38c154be4e8b43f1c24ea23fe1.

This checkpoint joins root1914's replacement-aware Panda verifier unchanged.
Actual token proposals expand against one original board before the complete
entry batch. Original requested templates and source/controller snapshots link
all actual CreateToken occurrences in an appended private journal. Checked
native-width counts and fallible roster allocation avoid an artificial token
cap. Tapped dynamic creations initialize every expanded member before observers.
Two new committed variants append after the complete Panda prefix; retained
journals refuse frozen sampling and old observations after Season departure.
No GameState field or mode is added. Ordinary proposal serialization omits the
absent tapped extension.

Shared +1/+1 placement records original effect amount, actual recipient
incarnation/current controller, original balance and actual counter occurrence.
Krenko, Faebloom and Ajani token proofs consult original creation rosters; Ajani,
Librarian, Wildwood and Stromkirk counter proofs consult original counter inputs.
They do not resample the later Season board or weaken count comparisons to
merely positive. Loyalty payment has an explicitly named cost entry point while
public/effect placement retains effect origin. Panda's independent world replay
is unchanged.

These are trusted original engine journals, not cryptographic protection against
replacement of the whole history. Existing operation owners retain original
payment/publication/input evidence. Registered absence/token-batch tests and
future actual Season mixed-controller/tapped-roster/control-transfer/departure/
serialized private-log and omitted-member negatives are prepared.

This remains a partial source checkpoint. Special copy/attachment/attacking/
haste/Incubate initialization still needs expanded roster semantics. Fishing
bait, entry and counter-kind paths above remain assigned work. No actual Season
printing is registered, enabled or claimed Full. A V48 safe producer is not yet
implemented; predecessor paths retain explicit refusal.
