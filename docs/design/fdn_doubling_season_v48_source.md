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
