# FDN targeted spells

This batch appends Bite Down, Felling Blow, Fleeting Flight and Joust Through
at ids 181 through 184. The original UG and WG decks retain their pinned bytes.
The Limited catalog moves to v37 `fc090e5b2a7b3e4f`; earlier catalog profiles
remain readable, and publisher/resume boundaries require the live identity.
Default v32 and the frozen flat formats retain their definitions.

| Spell | Rules contract |
| --- | --- |
| Bite Down | Choose a creature you control, then an opposing creature or planeswalker. The first creature deals damage equal to its current power. Check both target incarnations and legality again at resolution. An illegal source deals no damage; no last-known-power substitute. |
| Felling Blow | Sorcery. Add a +1/+1 counter to the legal controlled creature, then measure its power and deal its damage to the legal opposing creature. An illegal victim does not stop the counter; an illegal source prevents both effects. |
| Fleeting Flight | Add a +1/+1 counter, grant flying until cleanup and prevent incoming combat damage to that exact creature incarnation until cleanup. Noncombat and outgoing damage still apply. Unpreventable damage bypasses the replacement. |
| Joust Through | Target a current attacking or blocking creature of either player. Deal three damage, then the spell's controller gains one life even if damage was prevented. All effects fail if the sole target is illegal at resolution. |

Existing source-relative hexproof and protection filters apply at announcement
and resolution. Creature damage sources retain lifelink and deathtouch.
All combat damage passes through the shared combat commit path, which marks
transient proposals as combat damage without changing committed-event schemas.
Prevention is incarnation-bound, projected publicly and saved/restored.

Bite Down's planeswalker recipient needs minimal loyalty state. Appended
reference id 185, Ajani, Caller of the Pride, enters with four loyalty;
damage removes loyalty, and zero loyalty sends it to its owner's graveyard
without destruction. Its entry and exit preserve incarnation boundaries.
Ajani is explicitly partial: abilities and planeswalker attack choices are
unfinished, so custom deck admission refuses it. The synthetic 40-card
reference deck does not contribute to original-fixture coverage.

Optional loyalty and prevention fields in public engine context expose the new
state. Frozen native flat formats refuse these contexts before writing buffers;
the external custom-game interface carries them. Ordinary states omit the new
fields, keeping historical serialized bytes and diagnostic state hashes.

Reference behavior comes from the local XMage implementations
`Mage.Sets/src/mage/cards/{b/BiteDown,f/FellingBlow,f/FleetingFlight,j/JoustThrough,a/AjaniCallerOfThePride}.java`
and `DamageWithPowerFromOneToAnotherTargetEffect.java`.
Source inspection is not executed XMage parity. The validation report records
actual checks separately; executable differential comparisons remain a full
fixture milestone requirement.
