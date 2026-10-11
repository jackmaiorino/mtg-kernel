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

All38 affected Python cases pass after registration. Native admission1 at
source062b33c6 generated identity645f2da1b223a18f and file SHA256
6f0fb6bd1e159d8a46bd5359f576144f90e44627db631fd5d1872e523b10ffb4.
Brine4, High Fae6 and power/Elf6 actual cases pass. Ferocious1 passes and5 fail:
real attacker declaration omitted the new condition and emitted no trigger.
The declaration match now includes it. Guard1f0ecb81d52740aab996cb500b172283
ended101; the failed receipt remains retained. The live profile now uses the
generated hash and preserves historical v67 readability/stale mutator refusal.
Linux Limited CI job114364708658 at repaired source5f0bdae2 passes all22 new
card cases, including all6 ferocious cases, and the generated frozen-literal
check. Native2 guardff99f2e7f20b4de4a50f66a41c9d1ed3 acquired Jack16-17 and
is running against pinned5f0bdae2. Current-head CI38103703197 remains pending;
17 jobs have passed. The actual failed native1 receipt is not overwritten.

Default advanced to1d240477 through PR206 and PR219. Separate owned integration
459bd759 composes both parents without conflicts; the13 accepted default paths
do not overlap the FDN diff. The running source/target remain pinned. Source
review, final CI, native/profile qualification and actual default acceptance
remain required for the composed head. Current
accepted coverage remains171; candidate177 is engineering preparation, with
no strength, search, evaluation or training result.
