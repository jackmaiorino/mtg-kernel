# Conditional, flash and power creatures admission

This coherent six-card batch builds on accepted v67 PR214/defaultddff546a.
IDs371-376 append Brineborn Cutthroat, Ruby, Daring Tracker, Courageous Goblin,
High Fae Trickster, Elvish Archdruid and Ghalta, Primal Hunger. All metadata
uses the frozen286-name booster manifest and pinned Magea5c90fe180021e70e2a644ade00eeab07f857a40.
The admission script verified current accepted ancestry and registry equality
before writing candidate v68,377 definitions and a40-card reference deck.

Brine counters on own spells during the opponent's turn. Ruby/Courageous check
current controlled power at attack declaration; their boosts and Courageous's
menace bind to the captured source incarnation. High Fae's live printed ability
permits controlled nonland spells as though flash. Archdruid boosts other Elf
creatures and explicitly produces the current Elf count, including noncreature
Elves; admission refuses mana overflow before mutation. Ghalta subtracts the
signed current power sum, clamped once, from its generic cost while preserving
colored pips. Effective Haste and recursive bound-trigger validation compose.
Elder appends to the subtype vocabulary under the new catalog identity.

Sourcebbe6aeb2 passed both all-test typechecks and strict all-target Clippy;
engine141, ferocious2 and mana7 pass in each configuration, rules23 Limited/21
combined pass with one existing ignore. Supported guard34bd02d6d2b44601b119ae0b4d336e8a
ended0. Debug symbols/incremental data are disabled; pinned Rust1.94.1 and
MSVC19.50.35725/14.50.35717, assertions and optimization pins are preserved.
The22 new card groups were typechecked without registration. Failed predecessor
6851e748 remains retained; its nonexistent Frog fixture now uses Goblin.

All38 affected Python cases pass after registration. Actual22 card games,
generated v68 hash/profile, historical v67 reads/stale mutator refusal, native
checks, CI, exact-head review and default acceptance remain pending. Current
accepted coverage remains171; candidate177 is engineering preparation, with
no strength, search, evaluation or training result.
