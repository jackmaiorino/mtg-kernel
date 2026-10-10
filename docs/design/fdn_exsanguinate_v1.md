# FDN Exsanguinate preparation

Exsanguinate is a sorcery for {X}{B}{B}. Each opponent loses X life, then
its controller gains the life actually lost. The pinned
[XMage card](https://github.com/jackmaiorino/mage/blob/a5c90fe180021e70e2a644ade00eeab07f857a40/Mage.Sets/src/mage/cards/e/Exsanguinate.java)
and its `LoseLifeOpponentsYouGainLifeLostEffect` supply this contract.
Tentative admission is v72, ID346, after v63-v71.

Reuse the existing spell X selection, mana payment and stack/context binding.
Append `LoseOpponentsLifeXThenGainLifeLost` to the effect enum. It proposes
life loss for the opponent in the two-seat engine, reads only the committed
life-loss events from that proposal, and gains their actual positive amount.
Losing more than the remaining life is allowed; this is life loss rather
than damage. The exact program records the X and replaced-loss binding;
the rules facets mark their missing dependency vocabulary as opaque.

Two primitive regressions and three prepared admission cases cover both
seats, zero/max X, loss exceeding remaining life, Standard Bloodletter's
replacement, required colored mana, pending X and final-stack restore,
illegal X rejection and terminal resolution. They have not executed.
Registry, fixture, generated catalog/profile and gameplay qualification
await serial admission. Earlier effect discriminants and profiles persist.
Read-only source review at `58c07ab9` found no actionable correctness
defects. Its terminal-coverage note is addressed at `d32c59d1` with an
explicit lethal winner assertion. Native execution remains pending.
