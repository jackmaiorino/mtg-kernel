# Witness Protection implementation

Scope: finish the two Witness Protection copies in the unchanged UG fixture.
The WG fixture remains unchanged. London mulligans follow this card batch.

Reference: XMage `Mage.Sets/src/mage/cards/w/WitnessProtection.java` applies
name in layer 3, creature/Citizen in layer 4, green/white in layer 5, ability
removal in layer 6 and base 1/1 in layer 7b. Supertypes and counters remain.

Use a typed creature-Aura characteristic override with an exact incarnation
attachment. Derive characteristics instead of rewriting printed card identity.
Record permanent and attachment timestamps in the Limited profile. Layer 6
grants survive only when later than the last ability-removal effect. Attachment
to the same host does not acquire a new timestamp. Track keyword counters and
temporary grants in the same clock. Leave triggers use pre-move ability state;
already announced abilities keep their existing contracts and resolve normally.
Keep default state hashes and serialized defaults unchanged.

Acceptance tests map scenarios to behavior:

| Scenario | Action and assertion |
| --- | --- |
| Creature target and blue cost | Cast normally; reject wrong mana/noncreatures |
| Printed characteristics | Aura makes a green/white 1/1 Citizen named Legitimate Businessperson, preserving Legendary |
| Counters, pumps and Equipment | Base becomes 1/1 while later-layer bonuses remain |
| Armor before/after Witness | Older flying/protection disappear; later grants survive |
| Printed mana, activated, static and triggered abilities | Remove legal activations, mana payment sources, lord boosts, ward and future triggers |
| Guarded Heir dies | No Knight trigger when its abilities were removed before leaving |
| Already announced ability | Resolve despite subsequent ability removal |
| Aura/host leaves or returns | Restore printed characteristics; never follow an old incarnation |
| Two different legends renamed | Present the normal same-name legend choice |
| Save/restore | Pending targeting and timestamp-sensitive continuations agree |

Delivery requires focused Rust tests, strict XMage reference cases, relevant
regressions and release catalog checks. Only then publish full support and
qualify original UG/WG games through the external interface. No training,
playing-strength claim, GPU allocation or paid compute is part of this batch.
