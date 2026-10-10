# Controller instant/sorcery cost reducers, source preparation

This separate issue #110 preparation follows the 31-name admission. Neither
Mocking Sprite nor Archmage of Runes is registered or accepted by this work.

The pinned Mage constructors at
[a5c90fe1](https://github.com/jackmaiorino/mage/tree/a5c90fe180021e70e2a644ade00eeab07f857a40/Mage.Sets/src/mage/cards)
define Mocking Sprite as 2U 2/1 Faerie Rogue with Flying and Archmage of Runes
as 3UU 3/6 Giant Wizard. Both reduce controller instant/sorcery spells by one
generic; Archmage also draws one on each instant/sorcery cast. Both primary
constructors were read at that commit.

The casting design must determine one complete spell cost: selected base,
additional mana, kicker, Spree, X and increases, then reductions and one floor.
Spell-face types determine modifiers for Adventure/Omen and Bestow. Alternative,
flashback, Escape, Madness and Plotted costs must use the same planner. Freeze
the payment before a sacrifice can remove a reducer, pay mana once, and preserve
nonmana costs, Delve/Convoke, source reservations and one combined life budget.
Activation and resolution payments remain outside spell modifiers.

Source 8c3b9914 implements the u32 combined mana total without truncating at 255.
Both Limited-only and Standard/Limited test compilations and all 15 mana tests
passed under guard ab5c3985b65b40339b8c63a979a7eabd, two cores at BelowNormal,
exit 0, Rust 1.94.1 and MSVC 19.50.35725. Read-only review found no defects.

Source 02dc1469 collects every selected casting route and original nonmana
component groups. Six collector tests use existing registered cards. Native27
is queued under guard 7eb5daccdc9848b68f8cc430b0010f2b, supervisor 51012, with
the existing observer retained. At 20:00 UTC eligible cores were occupied by
CI and Claude's fast-forward claims; no duplicate or raw launch was made.

Source 5c231a1c binds Flying, Archmage's existing cast-draw trigger, static
fingerprints, rules-vector extraction and live modifiers. Sources must be
executable front-face battlefield objects, currently controlled by the caster,
with printed abilities active. Raw Standard modifiers combine before flooring.
Read-only review found no actionable defects; native qualification is pending.

Source 64607708 prepares immutable Delve payment from the complete adjusted
total, preserving minimum-exile and oldest-first choices while excluding
reserved objects. It also sums mandatory life costs with Phyrexian payment and
preserves zero-life payment at negative life. Three new regressions cover these
risks. Execution and review are pending. Convoke still explicitly refuses this
planner until its complete adapter is implemented.

Remaining work: Convoke adapter; offer/pending/X/final-payment integration;
atomic nonmana payment; actual card gameplay and restore tests; metadata and
catalog admission; live profile qualification; CI and default-branch acceptance.
No new card gameplay, increased coverage or experimental result is claimed.
