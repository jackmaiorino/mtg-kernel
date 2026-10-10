# FDN Aetherize preparation

Aetherize is an instant for {3}{U}, returning all attacking creatures to
owners' hands. The pinned
[XMage card](https://github.com/jackmaiorino/mage/blob/a5c90fe180021e70e2a644ade00eeab07f857a40/Mage.Sets/src/mage/cards/a/Aetherize.java)
and `ReturnToHandFromBattlefieldAllEffect` supply this contract. Tentative
admission is v73, ID347, after v63-v72.

Append `ReturnAttackingCreaturesToOwnersHands`. It samples unique live
battlefield objects that are current creatures in the attacking list,
then commits one simultaneous hand-movement batch. This uses ordinary
owner-zone movement, public-identity retention and removal from combat.
It has no targets. Its exact program records the attacking/current-type
restriction; rules facets mark their missing attacking vocabulary opaque.
Existing enum discriminants and catalog profiles remain unchanged.

One primitive case and three prepared card cases cover both seats,
borrowed creatures returning to their owner, tapped nonattackers excluded,
real attacker declaration and defending-player priority, pending-stack
restore, departed objects, changed creature types and late attackers.
They have not executed. Registry, fixture, generated catalog/profile and
gameplay qualification await serial admission.