# Other creature power at beginning of combat

Nessian Hornbeetle uses the pinned Mage constructor at
a5c90fe180021e70e2a644ade00eeab07f857a40:
1G,2/2 Insect; beginning of combat on its controller's turn, if another
controlled creature has power at least4, put one counter on the source.
The intervening condition is checked at capture and resolution. This source
preparation remains unregistered; no accepted coverage increases.

A shared pure query scans live objects with current controller, effective
creature type and continuous power. It excludes the captured source incarnation,
so a returned physical source is another object. The ability keeps its captured
controller after control changes. Existing bound-counter materialization and
recursive authentication prevent the old trigger from countering a new source
incarnation. No persistent state field or old enum ordinal is changed.

Limited now emits the existing beginning-of-combat event, already emitted by
Standard. The new trigger and effect condition append to their enums. Rule
features report the controlled battlefield power read and an opaque marker
because this vocabulary lacks the precise combat step, threshold and source
incarnation exclusion. The printed trigger contributes to future catalog hash.

A registered-card regression covers both seats, current control/zone/power,
source exclusion, historical return, restore and stale cached zone lists.
Five unregistered card groups cover metadata and actual cast/combat progression,
self/opponent/three-power/opponent-turn refusal, resolution-time changes and
captured controller, source leave/return, and actual Witness suppression/removal.
Formatting and diff checks pass. Native compilation, actual card games, generated
catalog/profile identity, review, CI and default acceptance remain pending.
