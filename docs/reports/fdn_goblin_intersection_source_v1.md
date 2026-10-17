# Goblin Negotiation creature and planeswalker intersection

Source-only repair based on `66bf05250ac0e1dba577b6a68426801659eaf0c7`.
No Rust compilation, execution, Full promotion or consumer qualification is claimed.

[Official Comprehensive Rules, effective September 25, 2026](https://media.wizards.com/2026/downloads/MagicCompRules%2020260925.txt),
120.3c/e and 120.10, require both damage results and the greatest per-type excess.
For actual damage A, creature lethal C and pre-damage loyalty L, excess is
`max(0, A-C, A-L)`, equivalently `max(0, A-min(C,L))`.
C uses current effective toughness less damage already marked, floored at zero.
[Official Foundations release notes, Goblin Negotiation](https://magic.wizards.com/en/news/feature/foundations-release-notes)
explicitly apply deathtouch's lethal-one rule to this spell. Indestructibility
does not raise C. Damage prevention/replacement changes A, not the original X;
token replacements subsequently act on the resulting Goblin batch.

Pinned Mage `a5c90fe180021e70e2a644ade00eeab07f857a40`,
`PermanentImpl.java::getLethalDamage`, also takes the minimum of creature lethal
and loyalty. Its `Permanent.java::damageWithExcess` computes from proposed damage
and truncates the dealt packet to lethal. This repair deliberately retains the
kernel's actual committed-damage ownership and full packet instead.

The existing native damage commit already marks creature damage and removes
planeswalker loyalty. The repair adds an optional closed intersection input to
the private owning capture, pending and finishing checks, and public V59 facts.
The existing public creature quote remains C. The optional nested schema exposes
L and the effective threshold explicitly. Printed meaning carries the conditional
greatest-excess rule. Human projection and the strict Python grammar carry the
same facts without new dispatch authority or an outer protocol migration.
Absent intersection fields retain ordinary row JSON and the original capture
hash sequence. Existing producer inputs already include both changed Goblin
production files, meaning and public facts. The changed Human projection and
Python grammar are appended as actual source bytes using the existing ordered
length framing. The complete observation producer already includes the owning
capture child and now explicitly includes the changed native threshold parent.
No production child module is introduced and no identity is guessed.

Current FDN has only Ajani as a planeswalker and no registered animation bridge.
Standard Kaito becomes only a creature. New intersection fixtures explicitly use
the existing `temporary_creature` native state primitive on actual Ajani objects,
followed by genuine paid Goblin actions. They do not establish a printed animation
route. Numeric prevention and the deathtouch threshold scene are also explicitly
primitive setup. Original ordinary fixture bodies remain unchanged.
