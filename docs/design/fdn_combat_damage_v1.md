# Foundations combat damage for custom decks

Issue #110's fixture decks need trample and player-controlled damage
assignment. Foundations removed assignment order: nontrampling attackers
may distribute damage arbitrarily among their blockers. Trample permits
player damage only after assigning lethal to every blocker.
[Wizards' release notes](https://magic.wizards.com/en/news/feature/foundations-release-notes)
describe the change and the absence of a priority window between assignment
and dealing damage.

`kernel_limited_env --foundations-combat-v1` selects Limited JSONL schema 3.
Replies carry `priority_mode: engine_windows_v1` and
`combat_rules: foundations_v1`. Schemas 1 and 2 retain their previous
behavior. The Python client and command-line tool expose the same option.
This mode still uses the unconditional opening hand; mulligans remain a
separate requirement of the fixture goal.

`CombatState::foundations_v1` owns the phase and pending assignments.
For each nonforced amount, `choose_combat_damage_range` offers two halves
of an inclusive integer range. Repeating the choice selects any legal
amount with logarithmic prompt depth and two actions per prompt. Actions
identify the exact source, recipient and current bounds. The public V5
projection exposes the damage phase and amounts already assigned, so
later choices can account for earlier allocations.

For nontrampling attackers, every blocker except the last permits any
amount from zero through the remaining power. The last receives the
remainder. For tramplers, player damage is the final recipient; the last
blocker must receive lethal before any remainder reaches the player.
If an earlier blocker received less than lethal, all remaining damage
must go to the last blocker. Overassignment is legal. Marked damage
reduces lethal; deathtouch caps nonzero lethal at one. Prevention and
indestructibility do not increase the amount required to assign lethal.
A blocked attacker stays blocked after its blockers leave; trample then
assigns its damage to the player, while a nontrampler assigns none.

All attackers' and blockers' assignments join one simultaneous event
batch. The existing damage pipeline performs prevention, lifelink,
deathtouch, combat-damage triggers and initiative transfer. No priority,
SBAs or trigger placement occur between assignment answers. The complete
wave then performs the usual SBA and trigger checkpoint.

First/double strike creates two damage waves with real priority between
them. Normal-wave eligibility remembers which incarnations had first or
double strike as the first wave began; current double strike also permits
normal damage. The second wave recomputes power, blockers and marked damage.

The optional combat extension omits itself from legacy serialization and
hashing. Earlier action-semantic variants retain their discriminants.
Frozen native scorer vocabularies explicitly refuse the new Limited action;
the JSONL gameplay interface handles it directly. No scorer alias or
fallback changes an existing trained policy's action meaning.

Verification includes focused combat rules, pending-choice clone/JSON
restoration, deterministic binary gameplay with casts and damage choices,
existing Limited integrations, default state/environment goldens, and
Clippy. XMage's `DamageDistributionTest` and `FirstStrikeTest` supply
reference scenarios; executable comparisons remain a fixture completion
requirement.
