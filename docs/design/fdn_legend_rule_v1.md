# Foundations legend-rule choice

Implement rule 704.5j for every controlled legendary permanent sharing its
current name with another. Each controller chooses one to keep. The rest
go to their owners' graveyards, ignoring indestructible and without a
sacrifice marker. Choices follow APNAP order; same-controller name groups
use a deterministic order. All choices finish before the simultaneous
state-based-action pass commits, including lethal creatures in that pass.

A typed continuation records exact incarnations, all groups and the chosen
prefix, other mandatory state-based actions and already collected triggers.
It offers a public keep-permanent action and blocks priority, mana, combat
and unrelated resolution choices. Restore validates the unchanged pass
before accepting an answer. The next fixed-point pass precedes trigger
placement, so removing Dwynen can make another Elf die before priority.

States without this continuation retain their serialized bytes and hashes.
The external custom-game interface exposes the decision and its candidates;
the existing native scorer vocabulary explicitly refuses the new action.
Dwynen receives full-support admission with this path, restoration and
compatibility tests. The opt-in catalog receives v36 `3d41aa36a5a75d8f`;
earlier profiles retain their own literals and remain readable.

Validation covers either survivor, three copies, separate controllers,
APNAP choices without intermediate mutations, indestructibility, ownership,
lethal simultaneous choices, triggered entries, secondary lethal damage,
invalid/stale answers, JSON/session restore and natural external replays.
Executable XMage comparisons remain a required fixture milestone check.

References: [Comprehensive Rules effective September 25, 2026](https://media.wizards.com/2026/downloads/MagicCompRules%2020260925.pdf),
101.4, 704.3 and 704.5j. Implementation is routine engineering; no research
design, formal measurement, training or paid compute changes here.
